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
   löscht, was fehlt. VerseKit kennt beides nicht, also gehen die Werte des
   Basetools unverändert zurück.
3. **Nie Kaufdaten.** `preis`, `gekauft`, `paket`, `warbond` bleiben hier.
4. **Entfernt wird nur mit Freigabe** — ein Schiff, das hier fehlt (etwa weil
   ein Pledge eingeschmolzen wurde), wird dem Spieler gezeigt.
5. **Woanders Entferntes kommt nicht still zurück** — nur mit `override` nach
   Zustimmung.
"""
import hashlib

from . import paths

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


def local_ships(fleet_data):
    """Hangar -> {externalId: {'entry', 'ref', 'insurance'}}.

    ⚠ Kaufdaten werden hier schon nicht übernommen — was nicht im Ergebnis
    steht, kann auch nicht versehentlich hinausgehen."""
    out = {}
    for entry in (fleet_data.get('schiffe') or ()):
        name = (entry.get('name') or '').strip()
        if not name:
            continue
        ref = {'name': name[:200]}
        if entry.get('kurz'):
            ref['scRecord'] = entry['kurz'][:200]
        out[external_id(entry)] = {'name': name,
                                   'origin': entry.get('herkunft'),
                                   'ref': ref, 'insurance': insurance(entry)}
    return out


def plan(local, resolved, server, state):
    """Was zu tun ist — ohne etwas zu tun.

    `local`: aus `local_ships()`. `resolved`: externalId -> `bt` des
    Schiffstyps. `server`: {shipId: Schiff} nach dem Holen. `state` trägt
    `links` (externalId -> shipId), `baseline` (externalId -> Versicherung
    beim letzten Abgleich), `removed_elsewhere`, `pending_removals`,
    `overrides`. Rückgabe::

        {'link': [(externalId, shipId)], 'create': [externalId],
         'update': [(externalId, shipId)], 'pull': [(externalId, shipId)],
         'remove': [(externalId, shipId)],
         'take': [shipId], 'drop': [externalId],
         'local_removed': {externalId: shipId},
         'removed_elsewhere': [externalId], 'links': {...}}
    """
    links = dict(state.get('links') or {})
    baseline = state.get('baseline') or {}
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
              'take': [], 'drop': [], 'local_removed': {},
              'removed_elsewhere': [], 'links': {}}
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
        mine = local[ext]['insurance']
        theirs = ship.get('insurance')
        before = baseline.get(ext)
        if before is not None and mine != before and theirs == before:
            result['update'].append((ext, ship_id))       # hier geändert
        elif before is not None and theirs != before and mine == before:
            result['pull'].append((ext, ship_id))          # dort geändert

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


def change_sets(plan_result, local, resolved, server, batch=500):
    """Die Sendungen: [(change_set, externalIds)].

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
        if ext in (plan_result.get('override') or ()):
            op['override'] = True
        ops.append(op)
        keys.append(ext)
    for ext, ship_id in plan_result['update']:
        ship = server[ship_id]
        # ⚠ Name und Ort des Basetools unverändert zurück — sonst löschte
        # das `upsert` sie (Regel 2 oben).
        op = {'op': 'upsert', 'externalId': ext, 'shipId': ship_id,
              'version': ship.get('version', 0),
              'shipType': {k: v for k, v in (ship.get('shipType') or {}).items()
                           if k in ('bt', 'name') and v},
              'insurance': local[ext]['insurance']}
        if ship.get('name'):
            op['name'] = ship['name']
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
