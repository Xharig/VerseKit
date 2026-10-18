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
Hangar-Abgleich mit dem KRT Profit Basetool — der Teil ohne Netz.

⚠⚠ **VerseKit führt je Schiffstyp EINEN Eintrag** (`fleet.merge_duplicates`),
das Basetool jedes Schiff einzeln. Verknüpft wird deshalb je Typ genau ein
Schiff; ein zweites Exemplar desselben Typs im Basetool bleibt unangetastet
und wird nie entfernt.

Die Kennung eines VerseKit-Eintrags (`externalId`) ergibt sich aus Hersteller
und Name — `hangar.json` braucht dafür kein neues Feld, und sie bleibt über
jeden Import hinweg dieselbe.

**Die Regeln** (`resources/ships.md`, Sync-Anleitung):

1. **Erst verknüpfen, dann anlegen.** Beim ersten Abgleich wird jeder Typ, den
   es drüben schon gibt, per `link` an ein vorhandenes Schiff gehängt. Angelegt
   wird nur, was drüben fehlt — sonst stünde der Hangar doppelt da.
2. **Jede Änderung schickt den aktuellen Namen und Ort mit** — ein `upsert`
   löscht, was fehlt. Den Ort kennt VerseKit nicht, er geht unverändert
   zurück. Der Name ist der eigene Name im Fleet Manager (`asop.json`), sofern
   sich das Schiff einem Spielnamen zuordnen lässt; sonst geht auch er
   unverändert zurück. `fitted` ist der Haken `einsatzbereit` im Hangar —
   fehlt er in einer Sendung, behält der Server seinen Wert.
3. **Nie Kaufdaten.** `preis`, `gekauft`, `paket`, `warbond` bleiben hier.
4. **Entfernt wird nur mit Freigabe** — ein Schiff, das hier fehlt (etwa weil
   ein Pledge eingeschmolzen wurde), wird dem Spieler gezeigt.
5. **Woanders Entferntes kommt nicht still zurück** — nur mit `override` nach
   Zustimmung.
"""
import hashlib

from . import asop, paths

LTI = 'LTI'
MONTHS = 'MONTHS'


def external_id(entry):
    """Feste Kennung eines Hangar-Eintrags — aus Hersteller und Name."""
    base = '%s|%s' % (paths.name_key(entry.get('hersteller') or ''),
                      paths.name_key(entry.get('name') or ''))
    return 'vk-' + hashlib.sha1(base.encode('utf-8')).hexdigest()[:24]


def insurance(entry):
    """VerseKits Angaben -> `insurance` des Vertrags."""
    if entry.get('lti'):
        return {'kind': LTI}
    months = entry.get('versicherung')
    try:
        months = max(0, min(120, int(months)))
    except (TypeError, ValueError):
        months = 0
    return {'kind': MONTHS, 'months': months}


def local_ships(fleet_data, own_names=None):
    """Hangar -> {externalId: {'name', 'origin', 'ref', 'insurance',
    'own_name', 'asop_key', 'fitted'}}.

    `own_names` ist `{Hangar-Name: (Schlüssel, eigener Name)}` für die
    Schiffe, die sich einem Spielnamen zuordnen lassen. Bei allen anderen
    ist `own_name` `None` — der Name wird dann weder geschickt noch geholt.
    `fitted` ist `None`, solange der Haken nie gesetzt wurde.

    ⚠ Kaufdaten werden hier schon nicht übernommen — was nicht im Ergebnis
    steht, kann auch nicht versehentlich hinausgehen.

    ⚠ Beiboote laut Katalog (`fleet.CATALOG`) fehlen hier: Sie stehen in
    keinem Pledge, und das Basetool führt Besitz. Übernimmt der Abgleich
    dasselbe Fahrzeug von drüben, wird aus dem Katalog-Eintrag einer aus dem
    Basetool (`fleet.add`) — kein zweiter."""
    from . import fleet
    own_names = own_names or {}
    out = {}
    for entry in (fleet_data.get('schiffe') or ()):
        name = (entry.get('name') or '').strip()
        if not name or entry.get('herkunft') == fleet.CATALOG:
            continue
        ref = {'name': name[:200]}
        if entry.get('kurz'):
            ref['scRecord'] = entry['kurz'][:200]
        key, own = own_names.get(name) or (None, None)
        fitted = entry.get('einsatzbereit')
        unit = entry.get('einheit')
        out[external_id(entry)] = {
            'name': name, 'origin': entry.get('herkunft'), 'ref': ref,
            'insurance': insurance(entry),
            'own_name': None if key is None else (own or '')[:255],
            'asop_key': key,
            'fitted': None if fitted is None else bool(fitted),
            # `None` = nie gewählt, `''` = ausdrücklich keine Einheit.
            'unit': None if unit is None else str(unit)}
    return out


def server_unit(ship):
    """Die Kennung der Einheit eines Schiffs im Basetool — `''` ohne."""
    return str(((ship or {}).get('orgUnit') or {}).get('id') or '')


def _unit_side(mine, theirs, before):
    """`'push'`, `'pull'` oder `None` für die Einheit.

    ⚠ **Bei einem Widerspruch gewinnt das Basetool** (anders als bei Name
    und `fitted`): Dort ändern auch Offiziere und Admins die Einheit, und
    VerseKit soll deren Änderung nie still rückgängig machen. Nie gewählt
    (`None`) heißt: die Einheit von drüben übernehmen."""
    if mine is None:
        return 'pull' if theirs else None
    if mine == theirs:
        return None
    # ⚠ Ohne Stand vom letzten Abgleich (Zustandsdatei weg, neu verbunden)
    # ist nicht zu sagen, wer geändert hat — dann gilt das Basetool. Eine
    # Wahl auf der Karte geht dabei nicht verloren: `link` setzt den Stand
    # schon einen Abgleich früher, danach greift `theirs == before`.
    if before is not None and theirs == before:
        return 'push'
    return 'pull'


def _name_side(mine, theirs, before):
    """`'push'`, `'pull'` oder `None` für den Namen.

    Ohne Stand vom letzten Abgleich: fehlt er auf einer Seite, kommt er von
    der anderen; stehen zwei verschiedene da, geht der aus VerseKit hinaus.
    """
    if mine == theirs:
        return None
    if before is None:
        if not theirs:
            return 'push'
        return 'pull' if not mine else 'push'
    if theirs == before:
        return 'push'
    return 'pull' if mine == before else 'push'


def _fitted_side(mine, theirs, before):
    """`'push'`, `'pull'` oder `None` für `fitted`.

    Ein nie gesetzter Haken (`None`) holt nur ein gesetztes `fitted` von
    drüben — ein `false` dort ist dasselbe wie nichts.
    """
    if mine is None:
        return 'pull' if theirs else None
    if mine == theirs:
        return None
    if before is None or theirs == before:
        return 'push'
    return 'pull' if mine == before else 'push'


def _sides(mine, ship, ext, state):
    """Je Feld, ob es hinaus- oder hereingeht: `(push, pull)` als Mengen."""
    push, pull = set(), set()
    before = (state.get('baseline') or {}).get(ext)
    ours, theirs = mine['insurance'], ship.get('insurance')
    if before is not None and ours != before and theirs == before:
        push.add('insurance')
    elif before is not None and theirs != before and ours == before:
        pull.add('insurance')
    if mine.get('own_name') is not None:
        # ⚠ Verglichen wird in der Länge, die der Fleet Manager fasst — ein
        # längerer Name von drüben käme sonst gekürzt herein und ginge beim
        # nächsten Abgleich gekürzt wieder hinaus.
        before = (state.get('baseline_name') or {}).get(ext)
        side = _name_side(mine['own_name'][:asop.MAX_LENGTH],
                          (ship.get('name') or '')[:asop.MAX_LENGTH],
                          None if before is None else before[:asop.MAX_LENGTH])
        if side:
            (push if side == 'push' else pull).add('name')
    side = _fitted_side(mine.get('fitted'), bool(ship.get('fitted')),
                        (state.get('baseline_fitted') or {}).get(ext))
    if side:
        (push if side == 'push' else pull).add('fitted')
    side = _unit_side(mine.get('unit'), server_unit(ship),
                      (state.get('baseline_unit') or {}).get(ext))
    if side:
        (push if side == 'push' else pull).add('orgUnit')
    return push, pull


def plan(local, resolved, server, state):
    """Was zu tun ist — ohne etwas zu tun.

    `local`: aus `local_ships()`. `resolved`: externalId -> `bt` des
    Schiffstyps. `server`: {shipId: Schiff} nach dem Holen. `state` trägt
    `links` (externalId -> shipId), `baseline`, `baseline_name`,
    `baseline_fitted` und `baseline_unit` (externalId -> Versicherung, Name,
    `fitted`, Einheit beim letzten Abgleich), `removed_elsewhere`,
    `pending_removals`, `overrides`.
    Rückgabe::

        {'link': [(externalId, shipId)], 'create': [externalId],
         'update': [(externalId, shipId)], 'pull': [(externalId, shipId)],
         'push_fields': {externalId: [Feld]},
         'pull_fields': {externalId: [Feld]},
         'remove': [(externalId, shipId)],
         'take': [shipId], 'drop': [externalId],
         'local_removed': {externalId: shipId},
         'removed_elsewhere': [externalId], 'links': {...}}

    Felder sind `insurance`, `name`, `fitted` und `orgUnit`;
    `unit_conflicts` nennt die Schiffe, deren Einheit auf beiden Seiten
    geändert war (das Basetool gewinnt).
    """
    links = dict(state.get('links') or {})
    pending = set(state.get('pending_removals') or ())
    overrides = set(state.get('overrides') or ())
    elsewhere = set(state.get('removed_elsewhere') or ())
    kept = set(state.get('kept') or ())     # „im Basetool behalten"

    # Was der Server schon als unsere Verknüpfung kennt, gilt.
    for ship_id, ship in server.items():
        ext = ship.get('externalId')
        if ext:
            links[ext] = ship_id

    result = {'link': [], 'create': [], 'update': [], 'pull': [], 'remove': [],
              'push_fields': {}, 'pull_fields': {},
              'take': [], 'drop': [], 'local_removed': {},
              'removed_elsewhere': [], 'links': {}, 'unit_conflicts': []}
    taken = set()

    # 1) Verknüpfte: noch da? hier geändert?
    for ext, ship_id in sorted(links.items()):
        ship = server.get(ship_id)
        if ext not in local:
            if ship is not None and ext in kept:
                taken.add(ship_id)          # bleibt drüben, keine Frage mehr
            elif ship is not None and ext not in pending:
                result['local_removed'][ext] = ship_id
            elif ship is not None and ext in pending:
                result['remove'].append((ext, ship_id))
            continue
        if ship is None:
            # Drüben weg und nicht von uns: nie still neu anlegen.
            if local[ext]['origin'] == 'basetool':
                result['drop'].append(ext)
            elif ext in overrides:
                result['create'].append(ext)
            else:
                result['removed_elsewhere'].append(ext)
            continue
        taken.add(ship_id)
        result['links'][ext] = ship_id
        push, pull = _sides(local[ext], ship, ext, state)
        if push:                                           # hier geändert
            result['update'].append((ext, ship_id))
            result['push_fields'][ext] = sorted(push)
        if pull:                                           # dort geändert
            result['pull'].append((ext, ship_id))
            result['pull_fields'][ext] = sorted(pull)
            # Beide Seiten geändert und das Basetool gewinnt — das gehört in
            # die Abgleich-Meldung.
            mine = local[ext].get('unit')
            before = (state.get('baseline_unit') or {}).get(ext)
            if 'orgUnit' in pull and mine is not None and before is not None \
                    and mine != before:
                result['unit_conflicts'].append(ext)

    # 2) Noch nicht verknüpfte: erst an ein vorhandenes Schiff hängen.
    for ext in sorted(local):
        if ext in links and links[ext] in server:
            continue
        if ext in result['removed_elsewhere'] or ext in result['create']:
            continue
        bt = resolved.get(ext)
        if not bt:
            continue
        match = next((sid for sid, ship in sorted(server.items())
                      if sid not in taken and not ship.get('externalId')
                      and (ship.get('shipType') or {}).get('bt') == bt), None)
        if match:
            result['link'].append((ext, match))
            result['links'][ext] = match
            taken.add(match)
        elif ext not in elsewhere or ext in overrides:
            result['create'].append(ext)

    # 3) Drüben Schiffe eines Typs, den es hier nicht gibt: übernehmen.
    local_types = {resolved.get(ext) for ext in local if resolved.get(ext)}
    for ship_id, ship in sorted(server.items()):
        if ship_id in taken or ship.get('externalId'):
            continue
        bt = (ship.get('shipType') or {}).get('bt')
        if bt and bt not in local_types:
            result['take'].append(ship_id)
            local_types.add(bt)          # je Typ nur einmal übernehmen
    result['override'] = [ext for ext in result['create'] if ext in overrides]
    return result


def change_sets(plan_result, local, resolved, server, batch=500,
                new_unit=None, known_units=None):
    """Die Sendungen: [(change_set, externalIds)].

    `new_unit` ist die Einheit für neue Schiffe (Einstellung) — `None` heißt
    „wie bisher", dann entscheidet das Basetool. `known_units` sind die
    Kennungen der Einheiten des Mitglieds (`None` = Liste nicht geholt):
    Beim Anlegen geht eine Einheit nur hinaus, wenn sie darin steht — sonst
    lehnte das Basetool das ganze Anlegen ab, bei jedem Abgleich neu.

    ⚠ `link` steht immer VOR dem Anlegen — so will es die Anleitung, und der
    Server entscheidet in Reihenfolge."""
    ops, keys = [], []

    def ship_type(ext):
        ref = dict(local[ext]['ref'])
        if resolved.get(ext):
            ref['bt'] = resolved[ext]
        return ref

    for ext, ship_id in plan_result['link']:
        ops.append({'op': 'link', 'externalId': ext, 'shipId': ship_id})
        keys.append(ext)
    for ext in plan_result['create']:
        op = {'op': 'upsert', 'externalId': ext, 'shipType': ship_type(ext),
              'insurance': local[ext]['insurance']}
        if local[ext].get('own_name'):
            op['name'] = local[ext]['own_name']
        if local[ext].get('fitted') is not None:
            op['fitted'] = local[ext]['fitted']
        # Beim Anlegen ohne Zähler. Auf der Karte gewählt geht vor der
        # Einstellung; `''` (keine Einheit) geht als `null` hinaus.
        unit = local[ext].get('unit')
        if unit is None:
            unit = new_unit
        if unit is not None and known_units is not None \
                and (unit == '' or unit in known_units):
            op['orgUnit'] = unit or None
        if ext in (plan_result.get('override') or ()):
            op['override'] = True
        ops.append(op)
        keys.append(ext)
    for ext, ship_id in plan_result['update']:
        ship = server[ship_id]
        fields = set((plan_result.get('push_fields') or {}).get(
            ext, ('insurance',)))
        # ⚠ Was nicht hinausgeht, geht mit dem Wert des Basetools zurück —
        # sonst löschte das `upsert` Name und Ort (Regel 2 oben).
        op = {'op': 'upsert', 'externalId': ext, 'shipId': ship_id,
              'version': ship.get('version', 0),
              'shipType': {k: v for k, v in (ship.get('shipType') or {}).items()
                           if k in ('bt', 'name') and v},
              'insurance': (local[ext]['insurance'] if 'insurance' in fields
                            else ship.get('insurance')
                            or local[ext]['insurance'])}
        name = (local[ext].get('own_name') if 'name' in fields
                else ship.get('name'))
        if name:
            op['name'] = name
        if 'fitted' in fields:
            op['fitted'] = bool(local[ext].get('fitted'))
        # ⚠ Ohne `orgUnit` behält das Basetool die Einheit — deshalb nur,
        # wenn sie hier geändert wurde, und dann mit ihrem eigenen Zähler.
        if 'orgUnit' in fields:
            op['orgUnit'] = local[ext].get('unit') or None
            op['orgUnitVersion'] = ship.get('orgUnitVersion', 0)
        if ship.get('location'):
            op['location'] = {k: v for k, v in ship['location'].items()
                              if k in ('name', 'uex') and v}
        ops.append(op)
        keys.append(ext)
    for ext, ship_id in plan_result['remove']:
        ops.append({'op': 'remove', 'shipId': ship_id,
                    'version': server[ship_id].get('version', 0)})
        keys.append(ext)
    for n, op in enumerate(ops, 1):
        op['opId'] = 'h%d' % n
    return [({'ops': ops[i:i + batch]}, keys[i:i + batch])
            for i in range(0, len(ops), batch)]
