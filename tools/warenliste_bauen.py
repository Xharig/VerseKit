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
Die mitgelieferte Startliste der UEX-Warenliste bauen: `daten/lager-katalog.json`.

Holt die Liste wie das Programm (`stock_catalog.update`) in einem
Wegwerf-Ordner und schreibt sie kompakt nach `daten/`. Ohne eigene Ablage
nimmt Verse-Kit diese Datei sofort und frischt sie im Hintergrund auf.

Aufruf (mit Netz, vor einem Release):

    python tools/warenliste_bauen.py
"""
import json
import os
import shutil
import sys
import tempfile

HIER = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, HIER)


def main():
    heim = tempfile.mkdtemp(prefix='warenliste-')
    os.environ['SC_BP_HOME'] = heim
    try:
        from scbp import stock_catalog
        if not stock_catalog.update():
            print('Abruf gescheitert — daten/ bleibt unverändert.')
            return 1
        with open(os.path.join(heim, stock_catalog.CACHE),
                  encoding='utf-8') as handle:
            data = json.load(handle)
        ziel = os.path.join(HIER, 'daten', stock_catalog.BUNDLED)
        with open(ziel, 'w', encoding='utf-8', newline='\n') as handle:
            json.dump(data, handle, ensure_ascii=False, separators=(',', ':'))
            handle.write('\n')
        print('%s: %d Gegenstände, %d Waren, %d KB'
              % (os.path.relpath(ziel, HIER), len(data.get('teile') or ()),
                 len(data.get('waren') or ()), os.path.getsize(ziel) // 1024))
        return 0
    finally:
        shutil.rmtree(heim, ignore_errors=True)


if __name__ == '__main__':
    sys.exit(main())
