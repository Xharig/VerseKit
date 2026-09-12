# -*- coding: utf-8 -*-
"""Hält die Start-Datei auf dem Mac-Desktop aktuell — Name und Symbol.

**Wozu.** Auf dem Desktop liegt `VerseKit (Test) vX.Y.Z.command`. Sie startet
den Quellcode **mit den echten Bauplan-Daten** — deshalb laufen die Screenshots
für die Anleitung darüber: Auf leeren Listen sieht man dem Werkzeug nicht an,
dass es benutzt wird.

Zwei Dinge macht dieses Skript:

1. **Die Versionsnummer im Dateinamen nachziehen.** Sie steht dort, damit auf
   einen Blick klar ist, welcher Stand gerade startet — bei einem Werkzeug, von
   dem es Testfassungen im Tagesabstand gibt, ist das keine Kleinigkeit. Und
   `(Test)` bleibt im Namen, damit die Datei nie mit einer fertigen Version
   verwechselt wird.
2. **Das Programmsymbol setzen.** Ohne das zeigt der Finder das weiße Blatt für
   Terminal-Dateien.

**Aufruf** (aus dem Projektordner, nach jedem neuen RC):

    python3 tools/mac_testknopf.py

⚠ Nur für macOS. Unter Windows und Linux tut das Skript nichts — dort gibt es
weder diesen Desktop noch diese Art von Datei.

⚠ Das Symbol steckt am Mac **nicht in der Datei**, sondern in ihren erweiterten
Attributen (`com.apple.ResourceFork`). Ein Bordmittel auf der Kommandozeile gibt
es dafür nicht; der Weg führt über AppKit, also über `pyobjc`. Fehlt das Paket,
wird trotzdem umbenannt — nur das Symbol bleibt, wie es war. Nachinstallieren:

    pip install pyobjc-framework-Cocoa
"""

import os
import re
import sys

HIER = os.path.dirname(os.path.abspath(__file__))
PROJEKT = os.path.dirname(HIER)
DESKTOP = os.path.expanduser('~/Desktop')
SYMBOL = os.path.join(PROJEKT, 'assets', 'icon.png')

# Was auf dem Desktop gesucht wird — mit oder ohne Versionsnummer im Namen.
#
# ⚠ `.+` und nicht `[^.]*`: Eine Versionsnummer enthält **Punkte** („3.0.0-rc58").
# Das erste Muster verbot sie und fand deshalb ausgerechnet die Datei nicht mehr,
# die das Skript selbst benannt hatte — beim ersten Lauf fiel das nicht auf, weil
# im Namen damals noch gar keine Nummer stand.
# ⚠⚠ BEIDE Namen erkennen (Umbenennung zu VerseKit, 12.09.2026). Das Skript
# BENENNT die vorhandene Datei um, statt eine zweite anzulegen — deshalb reicht
# es, den alten Namen weiter zu finden: Beim naechsten Lauf heisst sie neu, und
# es bleibt bei EINEM Startknopf.
#
# Wer hier nur den neuen Namen stehen lassen wuerde, bekaeme „Keine Start-Datei
# gefunden" und muesste sie von Hand anlegen.
MUSTER = re.compile(r'^(?:SC BP Watcher|VerseKit) \(Test\)( v.+)?\.command$')
NAME = 'VerseKit'


def fassung():
    """Die Versionsnummer aus `sc_bp_watcher.py`, ohne die Datei zu laden."""
    quelle = os.path.join(PROJEKT, 'sc_bp_watcher.py')
    with open(quelle, encoding='utf-8') as f:
        for zeile in f:
            treffer = re.match(r"__version__ = '([^']+)'", zeile.strip())
            if treffer:
                return treffer.group(1)
    return None


def symbol_setzen(ziel):
    """Das Programmsymbol auf die Datei legen. Gibt zurück, ob es geklappt hat."""
    try:
        from AppKit import NSWorkspace, NSImage
    except ImportError:
        print('  · pyobjc fehlt — Symbol bleibt unverändert')
        print('    (pip install pyobjc-framework-Cocoa)')
        return False
    if not os.path.exists(SYMBOL):
        print('  ! Symbol nicht gefunden: %s' % SYMBOL)
        return False
    bild = NSImage.alloc().initWithContentsOfFile_(SYMBOL)
    if bild is None:
        print('  ! Symbol nicht lesbar')
        return False
    return bool(NSWorkspace.sharedWorkspace().setIcon_forFile_options_(
        bild, ziel, 0))


def main():
    if sys.platform != 'darwin':
        print('Nur für macOS — hier ist nichts zu tun.')
        return
    if not os.path.isdir(DESKTOP):
        sys.exit('Kein Desktop-Ordner gefunden: %s' % DESKTOP)

    treffer = [n for n in os.listdir(DESKTOP) if MUSTER.match(n)]
    if not treffer:
        sys.exit('Keine Start-Datei auf dem Desktop gefunden.\n'
                 'Erwartet: „%s (Test).command" oder mit Version dahinter '
                 '(der alte Name „SC BP Watcher (Test)" wird auch erkannt).'
                 % NAME)
    if len(treffer) > 1:
        print('  ! Mehrere gefunden, nehme die erste: %s' % ', '.join(treffer))

    alt = os.path.join(DESKTOP, treffer[0])
    v = fassung()
    if not v:
        sys.exit('Versionsnummer nicht gefunden in sc_bp_watcher.py')

    neu = os.path.join(DESKTOP, '%s (Test) v%s.command' % (NAME, v))
    if alt != neu:
        # ⚠ `os.rename` und nicht kopieren: Die erweiterten Attribute — und damit
        # das Symbol — wandern beim Umbenennen mit, beim Kopieren nicht
        # zwangsläufig.
        os.rename(alt, neu)
        print('  · umbenannt auf  %s' % os.path.basename(neu))
    else:
        print('  · Name stimmt schon: %s' % os.path.basename(neu))

    if symbol_setzen(neu):
        print('  · Symbol gesetzt')
    # Der Finder merkt die Änderung nicht immer von allein.
    os.utime(neu, None)
    print('\nFertig — auf dem Desktop liegt jetzt:\n  %s'
          % os.path.basename(neu))


if __name__ == '__main__':
    main()
