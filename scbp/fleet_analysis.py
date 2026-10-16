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
Flottenanalyse — was der eigene Hangar abdeckt und was fehlt.

Grundlage ist die mitgelieferte Datei `daten/flottenanalyse.json`: je Schiff
die Gameplay-Loops (Fracht, Bergbau, Jäger …) mit Haupt-, Neben- oder
geplanter Rolle, bei Snubs die passenden Mutterschiffe, und ob sich das
Schiff per CCU upgraden lässt. Die Daten stammen aus dem Fleetmanifest
(MIT, Lizenztext in `daten/flottenanalyse-LICENSE.txt`); die Loop-Zuordnung
ist eine Einschätzung, keine Spieldaten.

Drei Auskünfte:

| Auskunft | was sie sagt |
|---|---|
| `coverage` | je Loop die eigenen Schiffe in Haupt-, Neben- und geplanter Rolle |
| `gaps` | Loops ohne Schiff in Haupt- oder geplanter Rolle |
| `orphan_snubs` | Snubs, deren Mutterschiff nicht im Hangar steht |

Dazu je Hangar-Schiff der Schmelz-Status (`melt_state`): Schmelzen kommt aus
dem Import oder aus der Wahl an der Schiffskarte, CCU aus den Daten.

⚠ Im Spiel gekaufte Schiffe lassen sich weder schmelzen noch per CCU
upgraden — für sie gibt es keinen Schmelz-Status (`melt_applies`).
"""
import json
import re

from . import errors, paths

FILE = 'flottenanalyse.json'
LICENSE_FILE = 'flottenanalyse-LICENSE.txt'

PRIMARY = 'primary'
SECONDARY = 'secondary'
PLANNED = 'planned'

# Schmelz-Status = schmelzbar × per CCU upgradebar.
MELTABLE = 'meltable'      # schmelzen und upgraden
MELT_ONLY = 'meltOnly'     # schmelzen, aber kein CCU
UPGRADE = 'upgrade'        # nicht schmelzbar, nur CCU
LOCKED = 'locked'          # weder noch
MELT_STATES = (MELTABLE, MELT_ONLY, UPGRADE, LOCKED)

# Gespeicherte Werte am Hangar-Eintrag (`hangar.json`, deutsch wie alle
# Schlüssel dort): `schmelzbar` kommt aus dem Import, `schmelz_wahl` von
# der Schiffskarte und geht dem Import vor.
CHOICE_YES = 'ja'
CHOICE_NO = 'nein'

_cache = {}


def load():
    """Die mitgelieferten Daten — oder leere Listen, wenn die Datei fehlt."""
    if 'doc' in _cache:
        return _cache['doc']
    doc = {'loops': [], 'ships': []}
    try:
        with open(paths.bundled_file(FILE), encoding='utf-8') as f:
            raw = json.load(f)
        if isinstance(raw, dict) and isinstance(raw.get('ships'), list) \
                and isinstance(raw.get('loops'), list):
            doc = raw
    except Exception as exc:
        errors.record('fleet_analysis.load', exc)
    _cache['doc'] = doc
    _cache.pop('index', None)
    return doc


def _norm(name):
    """Wie das Fleetmanifest vergleicht: klein, Sonderzeichen werden Leerzeichen."""
    return re.sub(r'[^a-z0-9]+', ' ', (name or '').lower()).strip()


def _index():
    """`{normalisierter Name oder Alias: Schiff}` und die Schlüssel, längste zuerst.

    Der Hauptname geht einem gleichlautenden Alias eines anderen Schiffs vor."""
    if 'index' in _cache:
        return _cache['index']
    index = {}
    ships = load().get('ships') or []
    for s in ships:
        index[_norm(s.get('name'))] = s
    for s in ships:
        for alias in s.get('aliases') or ():
            index.setdefault(_norm(alias), s)
    index.pop('', None)
    keys = sorted(index, key=len, reverse=True)
    _cache['index'] = (index, keys)
    return _cache['index']


# Herstellernamen, die Exporte vor den Schiffsnamen setzen („Drake Cutlass
# Black"). Ohne sie trifft der Name nicht.
_MAKERS = ('roberts space industries', 'consolidated outland',
           'musashi industrial starflight concern', 'aegis dynamics', 'aegis',
           'anvil aerospace', 'anvil', 'aopoa', 'argo astronautics', 'argo',
           'banu', 'crusader industries', 'crusader', 'cnou',
           'drake interplanetary', 'drake', 'esperia', 'gatac', 'greycat',
           'kruger intergalactic', 'kruger', 'misc', 'mirai',
           'origin jumpworks', 'origin', 'rsi', 'tumbril', 'vanduul', 'xi an',
           'xian')


def match(name, manufacturer=''):
    """Das Schiff der Daten zu einem Hangar-Namen — oder `None`.

    1. Name oder Alias genau (Groß/klein und Satzzeichen zählen nicht).
    2. Ein Datenname als Anfang: `Cutlass Black Best In Show Edition` →
       `Cutlass Black`.
    3. Dasselbe ohne vorangestellten Hersteller.
    """
    index, keys = _index()
    candidates = [_norm(name)]
    maker = _norm(manufacturer)
    for c in list(candidates):
        for m in ((maker,) if maker else ()) + _MAKERS:
            if m and c.startswith(m + ' '):
                candidates.append(c[len(m) + 1:])
                break
    for c in candidates:
        if c in index:
            return index[c]
    for c in candidates:
        for key in keys:
            if c.startswith(key + ' '):
                return index[key]
    return None


def loops():
    """Die Loops in Anzeigereihenfolge: `[{'id', 'de', 'en'}]`."""
    return list(load().get('loops') or [])


def loop_name(loop_id, lang='de'):
    for loop in loops():
        if loop.get('id') == loop_id:
            return loop.get(lang) or loop.get('de') or loop_id
    return loop_id


def melt_applies(entry):
    """Gibt es für dieses Hangar-Schiff einen Schmelz-Status?

    Nur bei Echtgeld-Schiffen (`herkunft` = Pledge). Im Spiel gekaufte
    lassen sich weder schmelzen noch upgraden; Schiffslisten anderer
    Werkzeuge und der Basetool-Abgleich sagen nicht, wie ein Schiff in den
    Hangar kam."""
    from . import fleet
    return (entry.get('herkunft') or fleet.PLEDGE) == fleet.PLEDGE


def upgradeable(entry):
    """Lässt sich das Schiff per CCU upgraden? Ohne Treffer in den Daten: ja."""
    ship = match(entry.get('name'), entry.get('hersteller') or '')
    return not (ship and ship.get('upgradeable') is False)


def meltable(entry):
    """Schmelzbar: `True`, `False` oder `None` (unbekannt).

    Die Wahl an der Schiffskarte geht dem Import vor."""
    choice = entry.get('schmelz_wahl')
    if choice == CHOICE_YES:
        return True
    if choice == CHOICE_NO:
        return False
    value = entry.get('schmelzbar')
    return value if isinstance(value, bool) else None


def state_for(melt, upgrade):
    """`(schmelzbar, upgradebar)` → einer der vier Zustände, `None` bei unbekannt."""
    if melt is None:
        return None
    if melt:
        return MELTABLE if upgrade else MELT_ONLY
    return UPGRADE if upgrade else LOCKED


def melt_state(entry):
    """Der Schmelz-Status eines Hangar-Schiffs — `None`, wenn unbekannt oder
    nicht anwendbar."""
    if not melt_applies(entry):
        return None
    return state_for(meltable(entry), upgradeable(entry))


def analyse(entries):
    """Die Auswertung über die Hangar-Einträge.

    Gibt ein Wörterbuch zurück:

    - `matched`: `[(eintrag, schiff)]` — Hangar-Schiffe mit Treffer
    - `unknown`: Namen ohne Treffer
    - `coverage`: `{loop_id: {'primary': [...], 'secondary': [...],
      'planned': [...]}}` mit den Namen aus dem Hangar, alphabetisch
    - `gaps`: Loop-IDs ohne Schiff in Haupt- oder geplanter Rolle
    - `orphan_snubs`: `[(snub_name, [mutterschiffe])]`

    ⚠ Ein Schiff, das zweimal im Hangar steht, zählt je Loop einmal.
    Treffen zwei verschiedene Hangar-Namen denselben Datensatz (`ATLS GEO`
    und `ATLS IKTI` → `ATLS`), stehen beide Namen da.
    """
    matched, unknown = [], []
    for e in entries or ():
        if not isinstance(e, dict) or not e.get('name'):
            continue
        ship = match(e.get('name'), e.get('hersteller') or '')
        if ship is None:
            unknown.append(e.get('name'))
        else:
            matched.append((e, ship))

    # Je Datensatz das Schiff und die Hangar-Namen, die ihn treffen.
    owned = {}
    for e, ship in matched:
        names = owned.setdefault(ship.get('name'), (ship, []))[1]
        if e.get('name') not in names:
            names.append(e.get('name'))

    coverage = {}
    for loop in loops():
        lid = loop.get('id')
        roles = {PRIMARY: [], SECONDARY: [], PLANNED: []}
        for ship, names in owned.values():
            role = (ship.get('loops') or {}).get(lid)
            if role in roles:
                roles[role].extend(names)
        for role_names in roles.values():
            role_names.sort(key=str.lower)
        coverage[lid] = roles

    gaps = [lid for lid, roles in coverage.items()
            if not roles[PRIMARY] and not roles[PLANNED]]

    orphan_snubs = []
    for ship, names in owned.values():
        mothers = ship.get('motherships') or []
        if mothers and not any(m in owned for m in mothers):
            for name in names:
                orphan_snubs.append((name, list(mothers)))
    orphan_snubs.sort(key=lambda x: x[0].lower())

    return {'matched': matched, 'unknown': sorted(set(unknown), key=str.lower),
            'coverage': coverage, 'gaps': gaps,
            'orphan_snubs': orphan_snubs}
