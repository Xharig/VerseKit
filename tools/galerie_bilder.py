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
Die Bilder der Galerie „Was ist neu" bauen.

Nimmt für jeden Höhepunkt aus `scbp/gallery.py` das Webseiten-Bild
(`assets/screenshot-<seite>.png` und `-en.png`, siehe
`tools/bilder_machen.py`),
halbiert es und legt es unter `assets/galerie/<bild>.png` ab. Ohne
Zusatzpakete: Tk kann PNG lesen, verkleinern (`subsample`) und schreiben.

Aufruf nach dem Bilderlauf:  python3 tools/galerie_bilder.py
"""
import os
import sys

WURZEL = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, WURZEL)
sys.path.insert(0, os.path.join(WURZEL, 'tools'))

# Webseiten-Bild je Galerie-Bild, wo der Name abweicht.
QUELLE = {}


def main():
    import unsichtbar
    unsichtbar.sicherstellen()
    import tkinter as tk
    from scbp import gallery, pages

    ziel = os.path.join(WURZEL, 'assets', 'galerie')
    os.makedirs(ziel, exist_ok=True)
    bilder = sorted({h['bild'] for punkte in gallery.HIGHLIGHTS.values()
                     for h in punkte})
    namen = pages.page_ids()
    root = tk.Tk()
    root.withdraw()
    fehlt = []
    try:
        from bilder_machen import SEITEN
        for bild in bilder:
            seite = QUELLE.get(bild, bild)
            stamm = SEITEN.get(seite) if seite in namen else None
            if not stamm:
                fehlt.append('%s (keine Seite)' % bild)
                continue
            for zusatz in ('', '-en'):
                quelle = os.path.join(WURZEL, 'assets', stamm + zusatz + '.png')
                if not os.path.isfile(quelle):
                    fehlt.append(os.path.basename(quelle))
                    continue
                gross = tk.PhotoImage(master=root, file=quelle)
                klein = gross.subsample(2, 2)
                klein.write(os.path.join(ziel, bild + zusatz + '.png'),
                            format='png')
                print('ok', bild + zusatz, klein.width(), 'x', klein.height())
    finally:
        root.destroy()
    if fehlt:
        print('fehlt:', ', '.join(fehlt))
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
