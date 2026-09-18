# -*- coding: utf-8 -*-
#
# SC BP Watcher — App-Icon aus einer Bildvorlage bauen.
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
Baut `icon.ico` (und eine PNG-Vorschau) aus `assets/icon-source.png`.

⚠ **Das ist seit 18.09.2026 der einzige Weg.** Vorher gab es daneben
`make_icon.py`, das ein eigenes Symbol rechnerisch zeichnete (Scope-Ring mit
Punkt). Mit dem Figuren-Logo ist dieses Motiv weg — und ein Skript, das bei
jedem Aufruf `icon.ico` und `assets/icon.png` mit dem alten Symbol
überschreibt, ist eine Falle, keine Alternative. Es wurde entfernt; über die
Git-Historie ist es jederzeit wieder da.

Zwei Kniffe, damit das Icon auch klein noch etwas taugt:

  * **Rand wegschneiden.** Die Vorlage hat ringsum durchsichtige Fläche. Die kostet
    bei 16 Pixeln jeden zweiten Bildpunkt, also wird eng auf das Motiv beschnitten.
  * **Kleine Größen enger.** Für 16–32 Pixel wird zusätzlich hineingezoomt, damit
    die Mittelform groß genug bleibt. Der äußere Ring wird dabei angeschnitten —
    besser ein klar erkennbarer Kern als ein vollständiger grauer Fleck.

Aufruf:  python tools/make_icon_from_art.py [--src assets/icon-source.png]
         (braucht Pillow; nur zum Icon-Bauen, der Watcher selbst bleibt Stdlib-only)

Hinweis zu den Vorlagen: Sie liegen als **512x512, eng beschnitten** im Repo. Das
ist reichlich für das größte Symbol (256) und hält die Dateien bei rund 250 KB.
Die Rohbilder aus der Bilderzeugung sind gern 1536x1024 mit viel durchsichtigem
Rand — die gehören so nicht ins Repo, weil sie dauerhaft in der Git-Historie
liegen bleiben. Erst zuschneiden, dann auf 512 setzen, dann einchecken.
"""
import argparse
import os
import sys

# ⚠ Ohne das bricht die Ausgabe unter Windows an jedem Zeichen, das cp1252
# nicht kennt — hier steckt eines im Docstring. Prüfung 173 wacht darüber.
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import ausgabe                                                 # noqa: E402
ausgabe.utf8()

try:
    from PIL import Image
except ImportError:
    print('FEHLER: Pillow fehlt.  pip install pillow')
    sys.exit(2)

# ⭐ Bis 48 statt bis 32 (18.09.2026): Mit dem Figuren-Logo sind bei 48 Pixeln
# die HUD-Flächen neben der Figur nur noch Matsch. Genau 48 ist die Größe der
# Desktop-Symbole und vieler Dock-Leisten — dort zählt die Figur, nicht das
# Beiwerk. Ab 64 ist wieder Platz für das ganze Motiv.
# Ab dieser Kantenlänge gilt die normale Beschneidung; darunter wird zugezoomt.
KLEIN_BIS = 48
KLEIN_ZOOM = 0.86        # so viel vom Motiv bleibt bei den kleinen Größen übrig
GROESSEN = [16, 20, 24, 32, 40, 48, 64, 128, 256]


def eng_beschneiden(bild, rand=0.01, schwelle=40):
    """Schneidet durchsichtigen Rand weg und macht das Ergebnis quadratisch.

    Über einen **Schwellwert**, nicht über `getbbox()`: Die Vorlagen haben einen
    weichen dunklen Schein außen herum. Der ist zwar fast durchsichtig, zählt für
    `getbbox()` aber als Inhalt — der Rahmen bliebe drin und das Motiv würde bei
    16 Pixeln unnötig klein."""
    alpha = bild.getchannel('A')
    maske = alpha.point(lambda a: 255 if a >= schwelle else 0)
    kasten = maske.getbbox() or bild.getbbox()
    if kasten:
        bild = bild.crop(kasten)
    b, h = bild.size
    kante = max(b, h)
    zusatz = int(kante * rand)
    kante += zusatz * 2
    leinwand = Image.new('RGBA', (kante, kante), (0, 0, 0, 0))
    leinwand.paste(bild, ((kante - b) // 2, (kante - h) // 2), bild)
    return leinwand


def zoom(bild, anteil):
    """Schneidet mittig einen Ausschnitt heraus (anteil < 1 = näher dran)."""
    kante = bild.size[0]
    neu = int(kante * anteil)
    rand = (kante - neu) // 2
    return bild.crop((rand, rand, rand + neu, rand + neu))


def main():
    ap = argparse.ArgumentParser(description='Baut icon.ico aus einer Bildvorlage.')
    ap.add_argument('--src', default=os.path.join('assets', 'icon-source.png'),
                    help='Hauptmotiv (für 40 Pixel und größer)')
    ap.add_argument('--src-small', default=os.path.join('assets', 'icon-source-small.png'),
                    help='vereinfachte Version für 16–32 Pixel; fehlt sie, wird '
                         'das Hauptmotiv zugezoomt verwendet')
    ap.add_argument('--ico', default='icon.ico')
    ap.add_argument('--png', default=os.path.join('assets', 'icon.png'))
    args = ap.parse_args()

    if not os.path.exists(args.src):
        print('FEHLER: %s gibt es nicht.' % args.src)
        return 2

    quelle = Image.open(args.src).convert('RGBA')
    print('Hauptmotiv: %s  %dx%d' % (args.src, quelle.size[0], quelle.size[1]))
    motiv = eng_beschneiden(quelle)
    print('  eng beschnitten: %dx%d' % motiv.size)

    # Eigene vereinfachte Version für die kleinen Größen — sonst Notlösung Zoom.
    if os.path.exists(args.src_small):
        klein = eng_beschneiden(Image.open(args.src_small).convert('RGBA'))
        print('Kleinmotiv: %s  -> %dx%d' % (args.src_small, klein.size[0], klein.size[1]))
    else:
        klein = zoom(motiv, KLEIN_ZOOM)
        print('Kleinmotiv: keine eigene Vorlage — Hauptmotiv auf %d %% gezoomt'
              % int(KLEIN_ZOOM * 100))

    bilder = []
    for g in GROESSEN:
        basis = klein if g <= KLEIN_BIS else motiv
        bilder.append(basis.resize((g, g), Image.LANCZOS))
    print('  Größen: %s  (bis %d px aus dem Kleinmotiv)'
          % (', '.join(str(g) for g in GROESSEN), KLEIN_BIS))

    bilder[-1].save(args.ico, format='ICO',
                    sizes=[(g, g) for g in GROESSEN], append_images=bilder[:-1])
    os.makedirs(os.path.dirname(args.png) or '.', exist_ok=True)
    motiv.resize((256, 256), Image.LANCZOS).save(args.png)

    print('Geschrieben: %s (%.1f KB)' % (args.ico, os.path.getsize(args.ico) / 1024.0))
    print('Geschrieben: %s' % args.png)
    return 0


if __name__ == '__main__':
    sys.exit(main())
