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
Persönlich und gemeinsam: wie sich die eigenen Posten beim KRT Profit
Basetool aufteilen, und das Teilen in den Pool einer Einheit.

Der Posten von `GET /exchange/v1/me/stock` trägt `split`: den persönlichen
Teil und je Einheit, in deren Pool das Mitglied davon etwas hält, die Menge
(`resources/stock.md`, ADR-0278). Persönlich plus Pools ergibt die Menge des
Postens — ein geteiltes Item zählt also nie doppelt.

| Op | was sie tut | `expectedQuantity` |
|---|---|---|
| `share` | persönlich → Pool der Einheit | `split.personal` |
| `unshare` | eigener Anteil im Pool → persönlich | Menge dieses Pools |

Beide adressieren den Posten wie `set-quantity` (Material, Ort, Qualität,
gestohlen) und nennen immer `orgUnit`; das Basetool wählt nie selbst. Die
Felder kommen aus dem Posten des Basetools, nicht aus der eigenen Zeile — so
trifft die Op genau den Posten, dessen Aufteilung die Seite zeigt.

**Welche eigene Zeile zu welchem Posten gehört**, sagt dieselbe Kennung wie
im Abgleich (`exchange_stock.identity`): Material über den gemerkten `bt`
(Items mit UEX-Nummer über `uex:<Nummer>`), Ort, Qualität, gestohlen.
Liegen mehrere Zeilen in einem Posten, bekommen die ersten den geteilten
Anteil, der Rest ist persönlich.

⚠ Ohne Verbindung (Modul Basetool aus, Lager-Abgleich aus, Recht fehlt,
getrennt) gibt es keine Aufteilung: Alles gilt als persönlich, und die Seite
zeigt weder Umschalter noch Knöpfe.
"""
SCOPE_ALL = 'all'
SCOPE_PERSONAL = 'personal'
SCOPE_SHARED = 'shared'
SCOPES = (SCOPE_ALL, SCOPE_PERSONAL, SCOPE_SHARED)

SHARE = 'share'
UNSHARE = 'unshare'
CHANGES = '/me/stock/changes'

PIECE = 'PIECE'

# Ablehnungen der beiden Ops -> Textschlüssel; Unbekanntes nennt den Code.
REASONS = {
    'VERSION_CONFLICT': 's_ml_r_konflikt',
    'QUANTITY_ABOVE_EXPECTED': 's_ml_r_zu_viel',
    'INSUFFICIENT_STOCK': 's_ml_r_zu_wenig',
    'STOCK_EARMARKED': 's_ml_r_reserviert',
    'STOCK_OFFERED': 's_ml_r_angeboten',
    'NOT_A_MEMBER': 's_ml_r_kein_mitglied',
    'UNIT_MISMATCH': 's_ml_r_einheit',
    'LOCATION_UNKNOWN': 's_ml_r_ort',
    'STOLEN_MARKING_DISABLED': 's_ml_r_gestohlen',
    'UNMATCHED': 's_ml_r_unbekannt',
    'AMBIGUOUS': 's_ml_r_unbekannt',
}


# Wie lange die Antwort von `connected()` gilt: Sie fragt den
# Schlüsselspeicher, und die Seite zeichnet beim Tippen in der Suche oft neu.
CONNECTED_FOR = 5.0
_connected = {'at': -CONNECTED_FOR, 'conn': None, 'value': False}


def available():
    """Gibt es eine Aufteilung? Nur mit Modul Basetool, eingeschaltetem
    Lager-Abgleich, erteiltem Lagerrecht und Verbindung."""
    try:
        import time
        from . import basetool, basetool_sync, modules
        if not (modules.enabled('basetool')
                and basetool_sync.area_on(basetool_sync.SETTING_STOCK)):
            return False
        conn = basetool.CONNECTION
        granted = set(conn.granted or ()) | set(
            basetool_sync.STATUS.get('capabilities') or ())
        if not set(basetool.SCOPES_STOCK) <= granted:
            return False
        now = time.monotonic()
        if _connected['conn'] is not conn \
                or now - _connected['at'] >= CONNECTED_FOR:
            _connected.update(at=now, conn=conn, value=bool(conn.connected()))
        return _connected['value']
    except Exception:
        return False


def _number(value):
    try:
        return float(value or 0)
    except (TypeError, ValueError):
        return 0.0


def _amount(block):
    return _number((block or {}).get('amount'))


def snapshot(state=None):
    """Was die Seite zum Zuordnen braucht, aus dem Stand des letzten
    Abgleichs: `{'lots': {Kennung: Posten}, 'names': {Schlüssel: bt},
    'items': {bt: True}}` — oder `None` ohne Verbindung oder solange das
    Lager noch nie abgeglichen wurde."""
    if state is None:
        if not available():
            return None
        from . import basetool_sync
        state = basetool_sync.current_state()
    from . import exchange_stock
    sub = (state or {}).get('stock')
    if not isinstance(sub, dict) or 'server' not in sub:
        return None
    names = {}
    for cache in ('resolved_uex', 'resolved', 'resolved_items'):
        for key, value in (sub.get(cache) or {}).items():
            if isinstance(value, str):
                names.setdefault(key, value)
    lots = {}
    for lot in (sub.get('server') or {}).values():
        material = lot.get('material') or {}
        bt = material.get('bt')
        name = (material.get('name') or '').strip().lower()
        if name and bt:
            names.setdefault(name, bt)
        key = exchange_stock.identity(bt or name,
                                      (lot.get('location') or {}).get('name'),
                                      lot.get('quality'), lot.get('stolen'))
        lots[key] = lot
    items = {bt: True for bt, flag in (sub.get('items') or {}).items()
             if flag}
    return {'lots': lots, 'names': names, 'items': items}


def split_of(lot):
    """`(persönlich, [(Einheit, Menge)])` eines Postens. Ohne `split` (ältere
    Basetool-Fassung) ist alles persönlich."""
    split = lot.get('split')
    if not isinstance(split, dict):
        return _amount(lot.get('quantity')), []
    pools = []
    for pool in split.get('pools') or ():
        unit = str(((pool or {}).get('orgUnit') or {}).get('id') or '')
        amount = _amount((pool or {}).get('quantity'))
        if unit and amount > 0:
            pools.append((unit, amount))
    return _amount(split.get('personal')), pools


def row_key(row, snap):
    """Die Kennung des Postens beim Basetool, zu dem eine eigene Zeile
    gehört — oder `None`, wenn ihr Material nie aufgelöst wurde."""
    from . import exchange_stock
    lookup = (row.get('lookup') or row.get('name') or '').strip().lower()
    bt = snap['names'].get(lookup)
    if not bt:
        return None
    quality = row.get('quality') if row.get('store') == 'raw' else 0
    # Ein Item steht beim Basetool immer unter Qualität 0 — auch wenn die
    # Zeile noch im Rohstofflager liegt (wie `exchange_stock.local_lots`).
    if row.get('item') or (snap.get('items') or {}).get(bt):
        quality = 0
    return exchange_stock.identity(bt, row.get('place'), quality or 0,
                                   row.get('stolen'))


def attach(rows, snap):
    """Jeder Zeile ihren Anteil geben: `personal`, `shared` (Menge) und
    `lot` (der Posten beim Basetool oder `None`). Ohne `snap` ist alles
    persönlich."""
    by_lot = {}
    for row in rows:
        row['personal'], row['shared'], row['lot'] = row['amount'], 0.0, None
        key = row_key(row, snap) if snap else None
        if key and key in snap['lots']:
            by_lot.setdefault(key, []).append(row)
    for key, members in by_lot.items():
        lot = snap['lots'][key]
        _personal, pools = split_of(lot)
        left = sum(amount for _unit, amount in pools)
        for row in members:
            shared = min(row['amount'], left)
            left -= shared
            row.update(lot=lot, shared=round(shared, 3),
                       personal=round(row['amount'] - shared, 3))
    return rows


def scoped(rows, scope):
    """Die Zeilen der gewählten Sicht — in Persönlich und Gemeinsam mit dem
    Anteil als Menge."""
    if scope == SCOPE_PERSONAL:
        return [dict(r, amount=r['personal']) for r in rows
                if r.get('personal', r['amount']) > 0]
    if scope == SCOPE_SHARED:
        return [dict(r, amount=r['shared']) for r in rows
                if r.get('shared', 0) > 0]
    return rows


def unit_label(unit_id):
    """Name einer Einheit aus der Einheiten-Liste; eine verlassene Einheit
    (steht noch in `pools`, aber nicht mehr in der Liste) heißt so. Ohne
    geholte Liste (Recht fehlt, noch nicht abgeglichen) nur „Einheit"."""
    from . import exchange_orgunits
    from .language import t
    for unit in exchange_orgunits.units():
        if unit['id'] == unit_id:
            return exchange_orgunits.label(unit)
    return t('s_ml_einheit_weg' if exchange_orgunits.fetched()
             else 's_ml_einheit')


def _op(kind, lot, unit_id, amount, expected):
    material = lot.get('material') or {}
    location = lot.get('location') or {}
    unit = (lot.get('quantity') or {}).get('unit') or 'SCU'

    def quantity(value):
        value = int(round(value)) if unit == PIECE else round(value, 3)
        return {'amount': value, 'unit': unit}
    ref = {'bt': material.get('bt')}
    if material.get('name'):
        ref['name'] = str(material['name'])[:200]
    place = {k: v for k, v in location.items() if k in ('name', 'uex') and v}
    return {'op': kind, 'opId': kind[0] + '1', 'material': ref,
            'location': place, 'quality': int(lot.get('quality') or 0),
            'stolen': bool(lot.get('stolen')), 'orgUnit': {'id': unit_id},
            'quantity': quantity(amount), 'expectedQuantity': quantity(expected)}


def share_op(lot, unit_id, amount):
    """Ins Lager der Einheit geben: erwartet wird der persönliche Teil."""
    personal, _pools = split_of(lot)
    return _op(SHARE, lot, unit_id, amount, personal)


def unshare_op(lot, unit_id, amount):
    """Aus dem Pool der Einheit zurücknehmen: erwartet wird dessen Menge."""
    _personal, pools = split_of(lot)
    expected = next((a for u, a in pools if u == unit_id), 0.0)
    return _op(UNSHARE, lot, unit_id, amount, expected)


def check_amount(text, limit, piece):
    """Die eingetippte Menge als Zahl — oder `None`, wenn sie nicht über 0,
    nicht höchstens `limit` oder (bei Stück) nicht ganz ist."""
    from . import materials
    value = materials.parse_number(text)
    if value is None or value <= 0 or value > limit + 1e-9:
        return None
    if piece and abs(value - round(value)) > 1e-9:
        return None
    return value


def send(conn, op):
    """Die Op schicken. Rückgabe `(True, '')` oder `(False, Grund)`."""
    from . import basetool
    answer = conn.request('POST', CHANGES, {'ops': [op]},
                          idempotency_key=basetool.new_idempotency_key())
    for entry in (answer or {}).get('results') or ():
        if isinstance(entry, dict) and entry.get('result') not in (
                'applied', 'unchanged'):
            return False, str(entry.get('reason') or entry.get('result')
                              or 'UNKNOWN')[:64]
    return True, ''


def reason_text(reason):
    from .language import t
    key = REASONS.get(reason)
    return t(key) if key else t('s_ml_r_anders') % reason
