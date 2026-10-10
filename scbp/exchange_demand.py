# -*- coding: utf-8 -*-
#
# SC BP Watcher — zeigt live neue Star-Citizen-Bauplaene an.
# Copyright (C) 2026 Xharig
#
# SPDX-License-Identifier: GPL-3.0-only
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, version 3.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <https://www.gnu.org/licenses/>.
"""
Bedarf der eigenen Einheit aus dem KRT Profit Basetool
(`GET /exchange/v1/me/org-demand`, Recht `exchange.demand.read`).

Was die Einheiten des Mitglieds an offenen Aufträgen noch brauchen —
Rohstoffe mit Mindestgüte und Gegenstände, anonym und nur lesend.

⚠⚠ **Nur im Speicher.** Die Anleitung erlaubt eine Kopie höchstens 7 Tage,
nie in einer Sicherung, nie im Fehlerbericht, nie weitergegeben. Der
Datenordner wird gesichert — also liegt der Bedarf nirgends auf der Platte.
Nach einem Neustart wird er beim nächsten Abgleich neu geholt.

Geholt wird im Takt des Abgleichs (`basetool_sync`, höchstens alle
5 Minuten). Kommt `reason` mit, wird nichts gezeigt und der Grund genannt.
"""
import threading
import time

PATH = '/me/org-demand'
MAX_AGE = 7 * 24 * 3600

_lock = threading.Lock()
_state = {'data': None, 'at': 0.0, 'reason': ''}


def _amount(block):
    """`{'amount': 40, 'unit': 'SCU'}` → `(40.0, 'SCU')` — sonst `(0.0, '')`."""
    if not isinstance(block, dict):
        return 0.0, ''
    try:
        value = float(block.get('amount') or 0)
    except (TypeError, ValueError):
        value = 0.0
    return value, str(block.get('unit') or '')


def _name(block):
    return str((block or {}).get('name') or '').strip() if isinstance(
        block, dict) else ''


def parse(answer):
    """Die Antwort in eine feste Form bringen — Unbekanntes fällt weg.

    `{'materials': [{'name', 'raw', 'min_quality', 'amount', 'unit',
    'source'}], 'items': [{'name', 'amount', 'unit', 'craftable'}],
    'updated': '…', 'reason': ''}`"""
    answer = answer if isinstance(answer, dict) else {}
    materials = []
    for line in answer.get('materials') or []:
        if not isinstance(line, dict):
            continue
        name = _name(line.get('material'))
        amount, unit = _amount(line.get('openQuantity'))
        if not name or amount <= 0:
            continue
        try:
            minimum = int(line.get('minQuality') or 0)
        except (TypeError, ValueError):
            minimum = 0
        raw = [_name(r) for r in (line.get('rawRefs') or [])[:20]]
        materials.append({'name': name, 'raw': [r for r in raw if r],
                          'min_quality': minimum, 'amount': amount,
                          'unit': unit,
                          'source': str(line.get('source') or '')})
    items = []
    for line in answer.get('items') or []:
        if not isinstance(line, dict):
            continue
        name = _name(line.get('item'))
        amount, unit = _amount(line.get('openQuantity'))
        if not name or amount <= 0:
            continue
        items.append({'name': name, 'amount': amount, 'unit': unit,
                      'craftable': bool(line.get('craftableByMe'))})
    materials.sort(key=lambda m: (m['name'].lower(), -m['min_quality']))
    items.sort(key=lambda i: (not i['craftable'], i['name'].lower()))
    return {'materials': materials, 'items': items,
            'updated': str(answer.get('updatedAt') or ''),
            'reason': str(answer.get('reason') or '')}


def refresh(conn, now=None):
    """Den Bedarf holen und im Speicher ablegen. Gibt die Zählung zurück.

    Ein `reason` (etwa `NOT_PERMITTED`) verwirft den vorigen Stand."""
    data = parse(conn.request('GET', PATH))
    with _lock:
        _state['data'] = None if data['reason'] else data
        _state['reason'] = data['reason']
        _state['at'] = time.time() if now is None else now
    return {'materials': len(data['materials']), 'items': len(data['items']),
            'reason': data['reason']}


def current(now=None):
    """Der zuletzt geholte Bedarf — oder None (nichts da, zu alt, Grund)."""
    with _lock:
        data, at = _state['data'], _state['at']
    if data is None:
        return None
    if (time.time() if now is None else now) - at > MAX_AGE:
        forget()
        return None
    return data


def reason():
    """Warum nichts gezeigt wird — `''`, wenn es keinen Grund gibt."""
    with _lock:
        return _state['reason']


def forget():
    """Alles verwerfen — beim Ausschalten und beim Trennen."""
    with _lock:
        _state.update(data=None, at=0.0, reason='')


def visible():
    """Gibt es den Reiter gerade? Nur mit eingeschaltetem Bereich, bestehender
    Verbindung und erteiltem Recht."""
    try:
        from . import basetool, basetool_sync
        if not basetool_sync.area_on(basetool_sync.SETTING_DEMAND):
            return False
        conn = basetool.CONNECTION
        granted = set(conn.granted or ()) | set(
            basetool_sync.STATUS.get('capabilities') or ())
        if not set(basetool.SCOPES_DEMAND) <= granted:
            return False
        return conn.connected()
    except Exception:
        return False
