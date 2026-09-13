# -*- coding: utf-8 -*-
"""Baut die Übersichtstafel aller Symbole — zum Nachschlagen in der Sammlung.

Wozu: Wer ein Symbol braucht, muss sehen können, was es gibt. Eine Liste von
Namen hilft dabei nicht — man erkennt ein Symbol am Bild, nicht am Wort.

Die Tafel landet in der Notizsammlung des Autors neben der Notiz
`04 Ressourcen/Branding & Vorlagen/Symbole (Lucide).md`. Kommen Symbole dazu,
erst `symbole_bauen.py` laufen lassen, dann dieses hier.

    python tools/symbole_tafel.py

Ohne Sammlung (etwa auf einem anderen Rechner) landet die Tafel im Projektordner
unter `assets/symbole/uebersicht.png`.
"""

import os
import sys

try:
    from PIL import Image, ImageDraw, ImageFont
except ImportError:                                   # pragma: no cover
    sys.exit('Pillow fehlt. Erst installieren:  pip install pillow')

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from symbole_bauen import KNOPF_SYMBOLE, ZEILEN_SYMBOLE, ZIEL   # noqa: E402

# ⛔ Vor der ersten Ausgabe: Die Windows-Konsole kann kein `→` — siehe ausgabe.py.
import ausgabe                                                 # noqa: E402
ausgabe.utf8()


# Xharig-Branding: dunkler Grund, Neongrün als Akzent.
GRUND = '#0d0d0d'
KASTEN = '#1c1c1c'
NEON = '#9ce430'
TEXT = '#e6edf3'
GRAU = '#8b98a5'

ZOOM = 3                     # Symbole vergrößert zeigen, sonst erkennt man nichts
SPALTEN = 6
ZELLE_B, ZELLE_H = 190, 132
RAND = 34

# Wohin die Übersichtstafel zusätzlich gelegt wird — für die eigene
# Dokumentation. ⚠ **Kein fester privater Pfad im Quelltext**: Der Ordner einer
# persönlichen Wissenssammlung geht niemanden etwas an, und dieses Repo ist
# öffentlich. Wer das Ziel nutzen will, setzt `SC_BP_SYMBOLTAFEL`; sonst bleibt
# die Tafel einfach im Projekt liegen.
ZUSATZZIEL = os.environ.get('SC_BP_SYMBOLTAFEL') or ''


def _schrift(groesse, fett=False):
    """Eine Schrift, die auf dem jeweiligen System wirklich existiert."""
    kandidaten = [
        '/System/Library/Fonts/Supplemental/Arial Bold.ttf' if fett
        else '/System/Library/Fonts/Supplemental/Arial.ttf',
        '/usr/share/fonts/TTF/DejaVuSans%s.ttf' % ('-Bold' if fett else ''),
        '/usr/share/fonts/truetype/dejavu/DejaVuSans%s.ttf'
        % ('-Bold' if fett else ''),
        'C:\\Windows\\Fonts\\segoeui%s.ttf' % ('b' if fett else ''),
    ]
    for pfad in kandidaten:
        if os.path.exists(pfad):
            return ImageFont.truetype(pfad, groesse)
    return ImageFont.load_default()


def _abschnitt(tafel, stift, y, titel, anzahl):
    """Eine Überschrift mit grüner Linie darunter."""
    stift.text((RAND, y), titel.upper(), font=_schrift(19, True), fill=NEON)
    stift.text((RAND, y + 27), '%d Symbole' % anzahl,
               font=_schrift(14), fill=GRAU)
    stift.line((RAND, y + 52, tafel.width - RAND, y + 52), fill='#2a3340', width=1)
    return y + 70


def _zellen(tafel, stift, y, symbole, px):
    """Ein Raster aus Symbol + Name + Vorlagenname."""
    for i, (name, vorlage) in enumerate(sorted(symbole.items())):
        sx = RAND + (i % SPALTEN) * ZELLE_B
        sy = y + (i // SPALTEN) * ZELLE_H
        stift.rounded_rectangle((sx, sy, sx + ZELLE_B - 14, sy + ZELLE_H - 14),
                                radius=9, fill=KASTEN)
        quelle = os.path.join(ZIEL, str(px), '%s-gruen.png' % name)
        if os.path.exists(quelle):
            b = Image.open(quelle).convert('RGBA')
            b = b.resize((px * ZOOM, px * ZOOM), Image.LANCZOS)
            tafel.paste(b, (sx + (ZELLE_B - 14 - b.width) // 2, sy + 16), b)
        stift.text((sx + (ZELLE_B - 14) // 2, sy + 74), name,
                   font=_schrift(15, True), fill=TEXT, anchor='ma')
        stift.text((sx + (ZELLE_B - 14) // 2, sy + 95), vorlage,
                   font=_schrift(12), fill=GRAU, anchor='ma')
    zeilen = (len(symbole) + SPALTEN - 1) // SPALTEN
    return y + zeilen * ZELLE_H + 26


def main():
    breite = RAND * 2 + SPALTEN * ZELLE_B
    zeilen_k = (len(KNOPF_SYMBOLE) + SPALTEN - 1) // SPALTEN
    zeilen_z = (len(ZEILEN_SYMBOLE) + SPALTEN - 1) // SPALTEN
    hoehe = 108 + 70 + zeilen_k * ZELLE_H + 26 + 70 + zeilen_z * ZELLE_H + 40

    tafel = Image.new('RGB', (breite, hoehe), GRUND)
    stift = ImageDraw.Draw(tafel)

    # Signature-Element des Xharig-Brandings: dünne grüne Kreisringe, teils aus
    # dem Rand ragend. Siehe Sammlung → „Xharig Branding".
    for mx, my, r in ((breite - 90, 60, 150), (70, hoehe - 40, 190)):
        stift.ellipse((mx - r, my - r, mx + r, my + r), outline='#1e2a12', width=2)

    stift.text((RAND, 34), 'SYMBOLE', font=_schrift(30, True), fill=NEON)
    stift.text((RAND, 72), 'Lucide · fester Symbolsatz für Xharig-Projekte',
               font=_schrift(15), fill=GRAU)

    y = _abschnitt(tafel, stift, 118, 'Knöpfe und Reiter', len(KNOPF_SYMBOLE))
    y = _zellen(tafel, stift, y, KNOPF_SYMBOLE, 22)
    y = _abschnitt(tafel, stift, y, 'In der Textzeile', len(ZEILEN_SYMBOLE))
    _zellen(tafel, stift, y, ZEILEN_SYMBOLE, 18)

    ziele = []
    # ⚠ Nur wenn wirklich ein Ziel gesetzt ist — sonst legt ein leerer Pfad
    # einen Ordner im aktuellen Verzeichnis an.
    if ZUSATZZIEL and os.path.isdir(os.path.dirname(ZUSATZZIEL)):
        if not os.path.isdir(ZUSATZZIEL):
            os.makedirs(ZUSATZZIEL)
        ziele.append(os.path.join(ZUSATZZIEL, 'symbole-uebersicht.png'))
    ziele.append(os.path.join(ZIEL, 'uebersicht.png'))

    for z in ziele:
        tafel.save(z, optimize=True)
        print('  %s  (%d KB)' % (z, os.path.getsize(z) // 1024))


if __name__ == '__main__':
    main()
