# -*- coding: utf-8 -*-
#
# Verse-Kit — companion for Star Citizen.
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
Trims the data of the Fleetmanifest (Maluh, MIT) to what the fleet analysis
uses and writes `daten/flottenanalyse-v5.json`.

Usage:
    python -I tools/fleetmanifest_convert.py SOURCE_DIR COMMIT COMMIT_DATE

SOURCE_DIR holds copies of `flottenmanifest/data/loops.json`, `ships.json`
and `included.json` of https://github.com/MaluhTV/SC_Fleetmanifest at COMMIT
(COMMIT_DATE as YYYY-MM-DD). The license text stays in
`daten/flottenanalyse-LICENSE.txt`.
"""
import json
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TARGET = os.path.join(ROOT, 'daten', 'flottenanalyse-v5.json')

INFO = ('Gameplay loops per ship from the Fleetmanifest by Maluh (KRT), '
        'github.com/MaluhTV/SC_Fleetmanifest, MIT, see flottenanalyse-LICENSE.txt. '
        "The assignment is the author's assessment, not game data. Trimmed to "
        'the fields the analysis uses (loops.json, ships.json without '
        'dimensions, crew, notes and images, included.json). Made by '
        'tools/fleetmanifest_convert.py, do not edit by hand.')


def dump(value):
    return json.dumps(value, ensure_ascii=False, separators=(', ', ': '))


def load(folder, name):
    with open(os.path.join(folder, name), encoding='utf-8') as f:
        return json.load(f)


def trimmed_ship(ship):
    item = {'name': ship['name']}
    if ship.get('aliases'):
        item['aliases'] = ship['aliases']
    item['size'] = ship['size']
    item['status'] = ship['status']
    item['loops'] = ship['loops']
    if ship.get('motherships'):
        item['motherships'] = ship['motherships']
    if ship.get('archetype'):
        item['archetype'] = ship['archetype']
    if ship.get('upgradeable') is False:
        item['upgradeable'] = False
    return item


def rows(items, indent='    '):
    last = len(items) - 1
    return [indent + dump(item) + (',' if i < last else '') for i, item in enumerate(items)]


def build(folder, commit, commit_date):
    ships_doc = load(folder, 'ships.json')
    loops_doc = load(folder, 'loops.json')
    included_doc = load(folder, 'included.json')
    source = {
        'repository': 'https://github.com/MaluhTV/SC_Fleetmanifest',
        'commit': commit,
        'commit_date': commit_date,
        'ships_updated': ships_doc['meta']['updated'],
        'included_checked': included_doc['checked'],
        'license': 'MIT',
    }
    loops = [{'id': l['id'], 'group': l['group'], 'de': l['de'], 'en': l['en']}
             for l in loops_doc['loops']]
    parents = list(included_doc['parents'].items())
    lines = ['{', '  "_info": ' + dump(INFO) + ',', '  "source": ' + dump(source) + ',',
             '  "loops": [', *rows(loops), '  ],',
             '  "ships": [', *rows([trimmed_ship(s) for s in ships_doc['ships']]), '  ],',
             '  "included": {']
    last = len(parents) - 1
    lines += ['    ' + dump(p) + ': ' + dump(v) + (',' if i < last else '')
              for i, (p, v) in enumerate(parents)]
    lines += ['  }', '}']
    return '\n'.join(lines) + '\n'


def main(argv):
    if len(argv) != 4:
        print(__doc__)
        return 2
    text = build(argv[1], argv[2], argv[3])
    json.loads(text)
    with open(TARGET, 'w', encoding='utf-8', newline='\n') as f:
        f.write(text)
    print('written:', TARGET, len(text), 'bytes')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
