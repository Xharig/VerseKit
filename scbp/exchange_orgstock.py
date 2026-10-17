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
Das Lager der eigenen Einheiten aus dem KRT Profit Basetool
(`GET /exchange/v1/org-stock`, Recht `exchange.orgstock.read`).

Ein **Pool** je Einheit, in der das Mitglied direkt Mitglied ist; darin ein
**Posten** (`lot`) je Ware, Ort, Qualität und Gestohlen-Marke, über alle
Mitglieder zusammengezählt. Mit `exchange.orgstock.details` trägt jeder Posten
eines Pools, in dem das Mitglied Logistiker ist, seine **Einträge** — je
Besitzer einer, mit Notiz und Reservierungen.

Mit `exchange.orgstock.write` bucht ein Logistiker im Pool seiner Einheit:
einbuchen, ausbuchen, verkaufen, verschieben (Ort, Besitzer, Einheit), Notiz,
gestohlen, Reservierungen. Gesendet wird ein Änderungssatz an
`POST /exchange/v1/org-stock/changes`; was er geändert hat, kommt über den Feed.

⚠⚠ **Nur im Speicher**, wie der Bedarf: Es ist der Bestand anderer Mitglieder.
Die Anleitung erlaubt höchstens 7 Tage, nie in einer Sicherung — der
Datenordner wird gesichert, also liegt hier nichts auf der Platte. Nach einem
Neustart wird der Schnappschuss neu geholt, danach nur noch der Feed.

⚠ **Kennungen sind undurchsichtig** — `key`, `id`, `ref` werden gespeichert und
zurückgeschickt, nie zerlegt. Ein Besitzer wird an `ref` erkannt, nicht am
Namen.
"""
import threading
import time

PATH = '/org-stock'
CHANGES = '/org-stock/changes'
MEMBERS = '/org-stock/members'
TARGETS = '/org-stock/targets'
RESOLVE = '/catalog/resolve'
MAX_AGE = 7 * 24 * 3600
PAGE_LIMIT = 500

SCU = 'SCU'
PIECE = 'PIECE'

# Die vier Sichten der Seite.
ITEMS = 'items'
RAW = 'rohstoffe'
TRADE = 'handelsware'
COMPONENTS = 'komponenten'
CATEGORIES = (ITEMS, RAW, TRADE, COMPONENTS)

# Absagen je Anweisung, die die Seite in Worte fasst. Was hier fehlt, zeigt
# den Code.
REASONS = ('NOT_PERMITTED', 'HOLDER_UNKNOWN', 'VERSION_CONFLICT',
           'INSUFFICIENT_STOCK', 'STOCK_EARMARKED', 'STOCK_OFFERED',
           'OVER_ALLOCATION', 'EARMARK_TARGET_INVALID', 'UNIT_MISMATCH',
           'LOCATION_UNKNOWN', 'STOLEN_MARKING_DISABLED', 'UNMATCHED',
           'AMBIGUOUS')

_lock = threading.RLock()
# Ein Abruf zur Zeit: Takt und eine Buchung von der Seite holen sonst
# denselben Feed doppelt und überschreiben sich den Cursor.
_fetch_lock = threading.Lock()
_state = {'lots': {}, 'units': [], 'cursor': None, 'at': 0.0,
          'details': False, 'write': False, 'logistician': set(),
          'rosters': {}}


# ------------------------------------------------------------- Einlesen
def _text(value, limit=255):
    return str(value or '').strip()[:limit]


def _amount(block):
    """`{'amount': 40, 'unit': 'SCU'}` → `(40.0, 'SCU')`."""
    if not isinstance(block, dict):
        return 0.0, SCU
    try:
        value = float(block.get('amount') or 0)
    except (TypeError, ValueError):
        value = 0.0
    unit = _text(block.get('unit')) or SCU
    return value, unit


def _unit_ref(block):
    block = block if isinstance(block, dict) else {}
    return {'id': _text(block.get('id'), 128), 'name': _text(block.get('name'))}


def parse_entry(entry):
    """Ein Eintrag eines Postens — ein Besitzer, seine Menge, Notiz und
    Reservierungen. Ohne `id` wertlos (`None`)."""
    if not isinstance(entry, dict) or not _text(entry.get('id'), 128):
        return None
    holder = entry.get('holder') if isinstance(entry.get('holder'), dict) \
        else {}
    withheld = bool(holder.get('withheld'))
    amount, unit = _amount(entry.get('quantity'))
    jobs = []
    for job in entry.get('jobOrders') or ():
        if not isinstance(job, dict) or not _text(job.get('id'), 128):
            continue
        number = job.get('number')
        jobs.append({'id': _text(job.get('id'), 128),
                     'number': number if isinstance(number, int) else None,
                     'amount': _amount(job.get('quantity'))[0],
                     'delivered': bool(job.get('delivered'))})
    missions = []
    for mission in entry.get('missions') or ():
        if not isinstance(mission, dict) or not _text(mission.get('id'), 128):
            continue
        missions.append({'id': _text(mission.get('id'), 128),
                         'name': _text(mission.get('name')),
                         'amount': _amount(mission.get('quantity'))[0]})
    free = entry.get('free') if isinstance(entry.get('free'), dict) else {}
    version = entry.get('version')
    return {'id': _text(entry.get('id'), 128),
            'version': version if isinstance(version, int) else 0,
            'withheld': withheld,
            'holder': '' if withheld else _text(holder.get('name'), 200),
            'ref': '' if withheld else _text(holder.get('ref'), 128),
            'amount': amount, 'unit': unit,
            'note': '' if withheld else _text(entry.get('note'), 2000),
            'jobs': jobs, 'missions': missions,
            'free_jobs': _amount(free.get('jobOrders'))[0]
            if free.get('jobOrders') is not None else amount,
            'free_missions': _amount(free.get('missions'))[0]
            if free.get('missions') is not None else amount}


def parse_lot(item):
    """Ein Posten in fester Form — oder `None`, wenn Schlüssel, Einheit oder
    Ware fehlen.

    `entries` ist `None`, wenn der Posten keine trägt (kein Logistiker oder
    kein Recht `details`), und eine Liste, wenn er welche trägt."""
    if not isinstance(item, dict):
        return None
    key = _text(item.get('key'), 128)
    unit = _unit_ref(item.get('unit'))
    material = item.get('material') if isinstance(item.get('material'),
                                                  dict) else {}
    name = _text(material.get('name'))
    if not key or not unit['id'] or not (name or material.get('bt')):
        return None
    kind = item.get('materialKind')
    location = item.get('location') if isinstance(item.get('location'),
                                                  dict) else {}
    amount, unit_q = _amount(item.get('quantity'))
    earmarked = item.get('earmarked') if isinstance(item.get('earmarked'),
                                                    dict) else {}
    try:
        quality = int(item.get('quality') or 0)
    except (TypeError, ValueError):
        quality = 0
    entries = None
    if isinstance(item.get('entries'), list):
        entries = [e for e in (parse_entry(x) for x in item['entries']) if e]
    return {'key': key, 'unit': unit,
            'material': {k: v for k, v in (('bt', _text(material.get('bt'),
                                                         128)),
                                           ('name', name)) if v},
            'name': name or _text(material.get('bt'), 128),
            'kind': dict(kind) if isinstance(kind, dict) else None,
            'location': {k: v for k, v in location.items()
                         if k in ('name', 'uex') and v},
            'place': _text(location.get('name')),
            'quality': max(0, min(1000, quality)),
            'stolen': bool(item.get('stolen')),
            'amount': amount, 'unit_q': unit_q,
            'job': _amount(earmarked.get('jobOrders'))[0],
            'mission': _amount(earmarked.get('missions'))[0],
            'entries': entries}


def parse_units(units):
    out = []
    for unit in units or ():
        if not isinstance(unit, dict) or not _text(unit.get('id'), 128):
            continue
        out.append({'id': _text(unit.get('id'), 128),
                    'name': _text(unit.get('name')),
                    'shorthand': _text(unit.get('shorthand')),
                    'kind': _text(unit.get('kind'))})
    return out


def is_item(lot):
    """Ein Item trägt kein `materialKind` — nur Materialien haben eins."""
    return lot.get('kind') is None


# ------------------------------------------------------------ Einordnen
_COMPONENT_GROUPS = ('schiffswaffe', 'schiffsmodul', 'schiffswerkzeug')
_component_cache = {}


def _is_component(name):
    """Ein Schiffsteil (Waffe, Kühler, Kraftwerk, Schild, Werkzeug …)?

    Das Basetool liefert die Art eines Items nicht mit; eingeordnet wird über
    die eigenen Rezept- und Katalogdaten, wie in der Bauplan-Liste."""
    key = (name or '').strip().lower()
    if key in _component_cache:
        return _component_cache[key]
    found = False
    try:
        from . import categories, crafting
        raw = crafting.recipe_raw(name) or {}
        art = ''
        try:
            from . import catalog, collection
            entry = (catalog.load().get('bauplaene') or {}).get(
                collection.norm(name)) or {}
            art = entry.get('a') or ''
        except Exception:
            art = ''
        group = categories.classify(art=art, tag=raw.get('tag') or '',
                                    unterart=raw.get('subtype') or '',
                                    rezeptart=raw.get('type') or '')[0]
        found = group in _COMPONENT_GROUPS
    except Exception:
        found = False
    _component_cache[key] = found
    return found


def category(lot):
    """In welche Sicht ein Posten gehört.

    Materialien: Erz, Mineral, Ernte oder Raffiniertes → Rohstoffe; der Rest
    mit `commodity` → Handelsware. ⚠ `commodity` allein heißt nur „steht im
    UEX-Katalog", dort stehen auch Erze und Metalle. Ohne die UEX-Kennzeichen
    entscheidet `type`: `RAW`/`REFINED` → Rohstoffe.
    Items: Schiffsteile → Komponenten, alles Übrige → Items."""
    kind = lot.get('kind')
    if kind is None:
        return COMPONENTS if _is_component(lot.get('name')) else ITEMS
    if any(kind.get(flag) for flag in ('raw', 'mineral', 'harvestable',
                                        'refined')):
        return RAW
    if kind.get('commodity'):
        uex_known = any(flag in kind for flag in ('raw', 'mineral',
                                                   'harvestable', 'refined'))
        if uex_known or (kind.get('type') or '') == 'NO_REFINE':
            return TRADE
    return RAW


# --------------------------------------------------------------- Abruf
def _fetch(conn, cursor):
    """Schnappschuss (ohne Cursor) oder Feed (mit) bis zum Ende."""
    pages = []
    while True:
        query = {'limit': PAGE_LIMIT}
        if cursor:
            query['cursor'] = cursor
        page = conn.request('GET', PATH, query=query)
        pages.append(page)
        cursor = page.get('nextCursor') or cursor
        if not page.get('hasMore'):
            return pages, cursor


def apply_pages(lots, pages, details):
    """Seiten auf einen Stand legen. Rückgabe: (Posten, Einheiten).

    Jede Seite nennt die Einheiten, die das Mitglied gerade liest — gilt die
    der letzten Seite. Posten einer Einheit, die nicht mehr genannt wird,
    fallen weg. Ohne `details` fallen die Einträge weg."""
    lots = dict(lots)
    units = None
    for page in pages:
        for stone in page.get('removed') or ():
            if isinstance(stone, dict):
                lots.pop(_text(stone.get('key'), 128), None)
        for item in page.get('items') or ():
            lot = parse_lot(item)
            if lot is not None:
                lots[lot['key']] = lot
        if isinstance(page.get('units'), list):
            units = parse_units(page['units'])
    units = units if units is not None else []
    known = {u['id'] for u in units}
    lots = {k: v for k, v in lots.items() if v['unit']['id'] in known}
    if not details:
        for lot in lots.values():
            lot['entries'] = None
    return lots, units


def refresh(conn, details=False, write=False, now=None):
    """Den Pool holen: beim ersten Mal den Schnappschuss, danach den Feed.

    Kommt das Recht `details` dazu oder fällt es weg, wird neu geholt — so
    verlangt es die Anleitung. Mit `write` wird je Einheit die Mitgliederliste
    gefragt; wer dort `NOT_PERMITTED` bekommt, ist dort kein Logistiker.
    Gibt die Zählung zurück."""
    from . import basetool
    with _fetch_lock:
        with _lock:
            cursor = _state['cursor']
            lots = dict(_state['lots'])
            if bool(details) != _state['details'] or not _state['at']:
                cursor, lots = None, {}
        if cursor:
            try:
                pages, cursor = _fetch(conn, cursor)
            except basetool.ApiError as error:
                if error.code != 'CURSOR_EXPIRED':
                    raise
                lots = {}
                pages, cursor = _fetch(conn, None)
        else:
            pages, cursor = _fetch(conn, None)
        lots, units = apply_pages(lots, pages, details)
        logistician, rosters = set(), {}
        for lot in lots.values():
            if lot['entries'] is not None:
                logistician.add(lot['unit']['id'])
        if write:
            for unit in units:
                try:
                    rosters[unit['id']] = members(conn, unit['id'])
                    logistician.add(unit['id'])
                except basetool.ApiError as error:
                    if error.code != 'NOT_PERMITTED':
                        raise
                    logistician.discard(unit['id'])
        with _lock:
            _state.update(lots=lots, units=units, cursor=cursor,
                          at=time.time() if now is None else now,
                          details=bool(details), write=bool(write),
                          logistician=logistician, rosters=rosters)
    return {'units': len(units), 'lots': len(lots),
            'logistician': len(logistician)}


def members(conn, unit_id):
    """Wer im Pool dieser Einheit als Besitzer eingebucht werden kann:
    `[{'ref', 'name'}]`. Wirft `ApiError('NOT_PERMITTED')` für eine Einheit,
    in der das Mitglied kein Logistiker ist."""
    answer = conn.request('GET', MEMBERS, query={'unit': unit_id})
    out = []
    for item in (answer or {}).get('items') or ():
        if isinstance(item, dict) and _text(item.get('ref'), 128):
            out.append({'ref': _text(item.get('ref'), 128),
                        'name': _text(item.get('name'), 200)})
    return out


def targets(conn, unit_id, material_bt):
    """Aufträge und Missionen, für die ein Eintrag dieses Pools reserviert
    werden kann: `{'jobs': [{id, number, status}], 'missions': [{id, name,
    status}]}`."""
    answer = conn.request('GET', TARGETS, query={'unit': unit_id,
                                                 'material': material_bt})
    jobs, missions = [], []
    for job in (answer or {}).get('jobOrders') or ():
        if isinstance(job, dict) and _text(job.get('id'), 128):
            number = job.get('number')
            jobs.append({'id': _text(job.get('id'), 128),
                         'number': number if isinstance(number, int) else None,
                         'status': _text(job.get('status'))})
    for mission in (answer or {}).get('missions') or ():
        if isinstance(mission, dict) and _text(mission.get('id'), 128):
            missions.append({'id': _text(mission.get('id'), 128),
                             'name': _text(mission.get('name')),
                             'status': _text(mission.get('status'))})
    return {'jobs': jobs, 'missions': missions}


def _catalog_ref(raw):
    """`{bt, name}` aus einer Antwort von `catalog/resolve` — oder `None`."""
    if not isinstance(raw, dict):
        return None
    bt = _text(raw.get('bt'), 128)
    if not bt:
        return None
    return {'bt': bt, 'name': _text(raw.get('name'), 200) or bt}


def resolve(conn, kind, ref):
    """Eine Referenz über `POST /catalog/resolve` auflösen::

        ('resolved', {bt, name, kind})
        ('ambiguous', [{bt, name, kind}, …])   höchstens zehn
        ('unmatched', None)

    `kind` ist die angefragte Art (`ITEM` oder `MATERIAL`)."""
    answer = conn.request('POST', RESOLVE, {'kind': kind, 'refs': [ref]})
    for result in (answer or {}).get('results') or ():
        if not isinstance(result, dict) or result.get('index') != 0:
            continue
        if result.get('status') == 'resolved':
            found = _catalog_ref(result.get('ref'))
            if found:
                return 'resolved', dict(found, kind=kind)
        elif result.get('status') == 'ambiguous':
            candidates = [dict(c, kind=kind) for c in map(
                _catalog_ref, result.get('candidates') or ()) if c]
            if candidates:
                return 'ambiguous', candidates[:10]
    return 'unmatched', None


def resolve_material(conn, name, known=None, piece=False):
    """Die Ware eines Einbuchens vorab auflösen, Rückgabe wie `resolve`.

    `known`: `(kind, item-ref)` aus der Warenliste (`stock_catalog`) — dann
    zuerst über diese Referenz. Sonst, und wenn sie nichts findet, über den
    Namen: bei `piece` (Stück) erst als ITEM, dann als MATERIAL, bei SCU
    umgekehrt. Ist die erste Art eindeutig, gilt sie. Ist sie mehrdeutig,
    bleibt das Ergebnis mehrdeutig — auch wenn die zweite Art eindeutig
    ist; deren Treffer kommt dann als weiterer Kandidat dazu (und bleibt
    auch bei zehn Kandidaten in der Auswahl)."""
    if known:
        status, found = resolve(conn, known[0], known[1])
        if status != 'unmatched':
            return status, found
    ref = {'name': name.strip()[:200]}
    candidates, sure = [], []
    for kind in (('ITEM', 'MATERIAL') if piece else ('MATERIAL', 'ITEM')):
        status, found = resolve(conn, kind, ref)
        if status == 'resolved':
            if not candidates:
                return status, found
            sure.append(found)
        elif status == 'ambiguous':
            candidates += found
    if not candidates and not sure:
        return 'unmatched', None
    out = []
    for candidate in sure + candidates:
        if candidate['bt'] not in {c['bt'] for c in out}:
            out.append(candidate)
    keep = [c for c in out if c in sure]
    rest = [c for c in out if c not in sure]
    return 'ambiguous', rest[:10 - len(keep)] + keep


# ------------------------------------------------------------- Abfragen
def current(now=None):
    """`{'lots', 'units', 'at', 'details', 'write', 'logistician', 'rosters'}`
    — oder `None` (nichts geholt, älter als 7 Tage)."""
    with _lock:
        if not _state['at']:
            return None
        if (time.time() if now is None else now) - _state['at'] > MAX_AGE:
            _forget_locked()
            return None
        return {'lots': dict(_state['lots']), 'units': list(_state['units']),
                'at': _state['at'], 'details': _state['details'],
                'write': _state['write'],
                'logistician': set(_state['logistician']),
                'rosters': {k: list(v) for k, v in _state['rosters'].items()}}


def lots_of(data, unit_id=None, view=None, search=''):
    """Die Posten einer Einheit und Sicht, nach Name, Ort, Qualität."""
    needle = (search or '').strip().lower()
    out = []
    for lot in (data or {}).get('lots', {}).values():
        if unit_id and lot['unit']['id'] != unit_id:
            continue
        if view and category(lot) != view:
            continue
        if needle and needle not in lot['name'].lower() \
                and needle not in lot['place'].lower():
            continue
        out.append(lot)
    out.sort(key=lambda l: (l['name'].lower(), l['place'].lower(),
                            -l['quality'], l['stolen']))
    return out


def counts(data, unit_id=None):
    """Anzahl Posten je Sicht für eine Einheit."""
    out = dict.fromkeys(CATEGORIES, 0)
    for lot in (data or {}).get('lots', {}).values():
        if unit_id and lot['unit']['id'] != unit_id:
            continue
        out[category(lot)] += 1
    return out


def can_write(data, unit_id):
    """Darf hier gebucht werden? Recht `write` und Logistiker der Einheit."""
    return bool(data and data.get('write')
                and unit_id in (data.get('logistician') or ()))


def visible():
    """Gibt es den Reiter? Nur mit eingeschaltetem Bereich, Verbindung und
    erteiltem Leserecht."""
    try:
        from . import basetool, basetool_sync
        if not basetool_sync.area_on(basetool_sync.SETTING_ORGSTOCK):
            return False
        conn = basetool.CONNECTION
        granted = set(conn.granted or ()) | set(
            basetool_sync.STATUS.get('capabilities') or ())
        if not set(basetool.SCOPES_ORGSTOCK) <= granted:
            return False
        return conn.connected()
    except Exception:
        return False


def _forget_locked():
    _state.update(lots={}, units=[], cursor=None, at=0.0, details=False,
                  write=False, logistician=set(), rosters={})


def forget():
    """Alles verwerfen — beim Ausschalten, beim Trennen, ohne Recht."""
    with _lock:
        _forget_locked()


# ------------------------------------------------------------ Schreiben
def quantity(amount, unit):
    """`{'amount', 'unit'}` — SCU auf drei Stellen, Stück ganzzahlig."""
    if unit == PIECE:
        return {'amount': int(round(float(amount or 0))), 'unit': PIECE}
    return {'amount': round(float(amount or 0), 3), 'unit': unit or SCU}


def _earmarks(job_amounts, mission_amounts, unit):
    """`({auftrag: menge}, {mission: menge})` → die beiden Listen einer
    Anweisung; Mengen von 0 fallen weg."""
    jobs = [{'jobOrder': ident, 'quantity': quantity(amount, unit)}
            for ident, amount in sorted((job_amounts or {}).items())
            if float(amount or 0) > 0]
    missions = [{'mission': ident, 'quantity': quantity(amount, unit)}
                for ident, amount in sorted((mission_amounts or {}).items())
                if float(amount or 0) > 0]
    return jobs, missions


def op_book_in(unit_id, holder_ref, material, location, quality_value, stolen,
               amount, unit, note='', merge_scu=False):
    """Neuer Eintrag im Pool, gehalten von `holder_ref` (aus der
    Mitgliederliste). `material` und `location` sind `{bt?, name}` bzw.
    `{name, uex?}`; ein Name allein wird beim Basetool aufgelöst."""
    op = {'op': 'book-in', 'unit': unit_id, 'holder': holder_ref,
          'material': material, 'location': location,
          'quality': 0 if unit == PIECE else int(quality_value or 0),
          'stolen': bool(stolen), 'quantity': quantity(amount, unit)}
    if (note or '').strip():
        op['note'] = note.strip()
    if merge_scu and unit != PIECE:
        op['mergeScu'] = True
    return op


def _taking(name, unit_id, entry, amount, unit, from_jobs=None,
            from_missions=None):
    op = {'op': name, 'unit': unit_id, 'entry': entry['id'],
          'version': entry['version'], 'quantity': quantity(amount, unit)}
    jobs, missions = _earmarks(from_jobs, from_missions, unit)
    if jobs:
        op['fromJobOrders'] = jobs
    if missions:
        op['fromMissions'] = missions
    return op


def op_book_out(unit_id, entry, amount, unit, from_jobs=None,
                from_missions=None):
    return _taking('book-out', unit_id, entry, amount, unit, from_jobs,
                   from_missions)


def op_sell(unit_id, entry, amount, unit, terminal, proceeds, from_jobs=None,
            from_missions=None):
    op = _taking('sell', unit_id, entry, amount, unit, from_jobs,
                 from_missions)
    op['terminal'] = (terminal or '').strip()
    op['proceeds'] = int(round(float(proceeds or 0)))
    return op


def op_move_location(unit_id, entry, amount, unit, location, from_jobs=None,
                     from_missions=None, merge_scu=False):
    op = _taking('move-location', unit_id, entry, amount, unit, from_jobs,
                 from_missions)
    op['location'] = location
    if merge_scu and unit != PIECE:
        op['mergeScu'] = True
    return op


def op_move_holder(unit_id, entry, amount, unit, to_unit, holder_ref,
                   from_jobs=None, from_missions=None, merge_scu=False):
    op = _taking('move-holder', unit_id, entry, amount, unit, from_jobs,
                 from_missions)
    op['toUnit'] = to_unit
    op['holder'] = holder_ref
    if merge_scu and unit != PIECE:
        op['mergeScu'] = True
    return op


def op_move_pool(unit_id, entry, to_unit, amount=None, unit=SCU,
                 merge_scu=False):
    """Ohne `amount` zieht der ganze Eintrag samt Reservierungen um."""
    op = {'op': 'move-pool', 'unit': unit_id, 'entry': entry['id'],
          'version': entry['version'], 'toUnit': to_unit}
    if amount is not None and float(amount or 0) > 0:
        op['quantity'] = quantity(amount, unit)
    if merge_scu and unit != PIECE:
        op['mergeScu'] = True
    return op


def op_note(unit_id, entry, note):
    """Ohne Text (oder leer) wird die Notiz gelöscht."""
    op = {'op': 'note', 'unit': unit_id, 'entry': entry['id'],
          'version': entry['version']}
    if (note or '').strip():
        op['note'] = note.strip()
    return op


def op_stolen(unit_id, entry, stolen, amount=None, unit=SCU):
    """Ohne `amount` gilt die Marke für den ganzen Eintrag."""
    op = {'op': 'stolen', 'unit': unit_id, 'entry': entry['id'],
          'version': entry['version'], 'stolen': bool(stolen)}
    if amount is not None and float(amount or 0) > 0:
        op['quantity'] = quantity(amount, unit)
    return op


def op_allocation(kind, unit_id, entry, target_kind, target_id, amount=None,
                  unit=SCU):
    """`kind`: `add`, `change` oder `remove`; `target_kind`: `jobOrder` oder
    `mission` — nie beides."""
    op = {'op': 'allocation-' + kind, 'unit': unit_id, 'entry': entry['id'],
          'version': entry['version'], target_kind: target_id}
    if kind != 'remove':
        op['quantity'] = quantity(amount, unit)
    return op


def read_result(result):
    """Die Antwort auf einen Änderungssatz: `{'applied', 'unchanged',
    'failed': [(index, ergebnis, grund)], 'offers'}`."""
    result = result if isinstance(result, dict) else {}
    failed = []
    for entry in result.get('results') or ():
        if not isinstance(entry, dict):
            continue
        if entry.get('result') in ('unchanged', 'applied'):
            continue
        failed.append((entry.get('index'), entry.get('result') or '',
                       _text(entry.get('reason'), 64)))
    return {'applied': int(result.get('applied') or 0),
            'unchanged': int(result.get('unchanged') or 0),
            'failed': failed,
            'offers': int(result.get('offersReduced') or 0)
            + int(result.get('offersRemoved') or 0)}


def send(conn, ops):
    """Einen Änderungssatz schicken. Rückgabe wie `read_result`, dazu
    `confirm_url`, wenn das Basetool eine Massenänderung erst im Browser
    bestätigt haben will (dann ist nichts geschrieben)."""
    from . import basetool
    try:
        answer = conn.request('POST', CHANGES, {'ops': list(ops)},
                              idempotency_key=basetool.new_idempotency_key())
    except basetool.ApiError as error:
        if error.code != 'MASS_CHANGE_CONFIRMATION_REQUIRED':
            raise
        url = basetool.trusted_address(
            (error.problem or {}).get('confirmationUrl') or '')
        return {'applied': 0, 'unchanged': 0, 'failed': [], 'offers': 0,
                'confirm_url': url or None}
    out = read_result(answer)
    out['confirm_url'] = None
    return out
