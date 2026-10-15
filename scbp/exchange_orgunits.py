# -*- coding: utf-8 -*-
#
# SC BP Watcher — zeigt live neue Star-Citizen-Baupläne an.
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
Die Einheiten des Mitglieds aus dem KRT Profit Basetool
(`GET /exchange/v1/me/org-units`, Recht `exchange.orgunits.read`).

Gebraucht für die Auswahl, in welcher Einheit ein Schiff steht: auf der
Schiffskarte im Hangar und als *Einheit für neue Schiffe*.

⚠ **Nur im Speicher**, wie der Bedarf: Die Liste ist bei jedem Abgleich neu
geholt, denn eine Mitgliedschaft kann jederzeit beginnen oder enden (so steht
es in der Anleitung des Basetools). Die Kennung `id` ist undurchsichtig —
gespeichert und zurückgeschickt, nie zerlegt.

Eine Auswahl gibt es nur bei **mehr als einer** Einheit: Wer in einer ist,
bekommt sie beim Basetool ohnehin von selbst.
"""
import threading

PATH = '/me/org-units'

_lock = threading.Lock()
_state = {'units': None}


def parse(answer):
    """`{'items': [{id, name, shorthand, kind}]}` → Liste von Einheiten.

    Unbekannte `kind`-Werte bleiben wählbar (Anleitung: „keep it
    selectable"). Ohne `id` ist ein Eintrag wertlos und fällt weg."""
    out = []
    for item in (answer or {}).get('items') or []:
        if not isinstance(item, dict):
            continue
        ident = str(item.get('id') or '').strip()
        if not ident:
            continue
        out.append({'id': ident,
                    'name': str(item.get('name') or '').strip(),
                    'shorthand': str(item.get('shorthand') or '').strip(),
                    'kind': str(item.get('kind') or '').strip()})
    return out


def refresh(conn):
    """Die Einheiten holen und im Speicher ablegen. Gibt die Anzahl zurück."""
    units = parse(conn.request('GET', PATH))
    with _lock:
        _state['units'] = units
    return len(units)


def units():
    """Die zuletzt geholten Einheiten — leer, solange keine geholt sind."""
    with _lock:
        return list(_state['units'] or ())


def label(unit):
    """Wie eine Einheit in der Auswahl heißt: `Staffel Alpha (ALP)`."""
    name = unit.get('name') or unit.get('shorthand') or unit.get('id')
    short = unit.get('shorthand')
    return '%s (%s)' % (name, short) if short and short != name else name


def choice_visible():
    """Gibt es etwas zu wählen? Nur bei mehr als einer Einheit."""
    return len(units()) > 1


def forget():
    """Alles verwerfen — beim Ausschalten, beim Trennen, ohne Recht."""
    with _lock:
        _state['units'] = None
