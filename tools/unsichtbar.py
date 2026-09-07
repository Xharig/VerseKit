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
Fensterlos prüfen — hält Prüfläufe vom Bildschirm des Nutzers fern.

Alle Werkzeuge, die Oberfläche prüfen, bauen echte tkinter-Fenster. Auf einem
Rechner, an dem gerade jemand arbeitet oder spielt, blitzen die auf und reißen
den Tastaturfokus mit: Wer in Star Citizen fliegt, landet mitten im Kampf auf
dem Desktop. Genau das ist am 29.08.2026 passiert — rund zwanzig Prüfläufe
während einer laufenden Spielsitzung.

Der Ausweg ist ein unsichtbarer Bildschirm (Xvfb). Sich daran zu erinnern hat
nicht funktioniert, deshalb erledigt es das Werkzeug jetzt selbst: Hängt ein
echter Bildschirm dran, startet es sich auf einem unsichtbaren neu.

Auf Windows und am Mac gibt es kein Xvfb. Dort wird stattdessen jedes Fenster
sofort nach dem Bauen versteckt (`withdraw()`), und die drei Befehle, die ein
Fenster nach vorn holen, laufen ins Leere. Geprueft wird dabei genauso viel —
die Widgets stehen, die Groessen stimmen —, nur sehen tut man nichts.

Einbau — ganz oben in der `main()` des Werkzeugs, vor dem ersten Fenster:

    import unsichtbar
    unsichtbar.sicherstellen()

Werkzeuge, die Groessen messen, rufen `sicherstellen(messend=True)` auf —
ein verstecktes Fenster liefert keine Geometrie.

Wer die Fenster ausnahmsweise sehen will, setzt `SC_BP_SICHTBAR=1`.
"""
import os
import shutil
import subprocess
import sys

# Merker im Kindprozess — ohne ihn würde er sich endlos weiter neu starten.
SCHON_UNSICHTBAR = 'SC_BP_UNSICHTBAR'

# Notausgang: Fenster bewusst sichtbar bauen (Fehlersuche von Hand).
SICHTBAR_GEWOLLT = 'SC_BP_SICHTBAR'


def noetig():
    """Muss dieser Lauf umgeleitet werden?

    Nein, wenn wir schon im unsichtbaren Kindprozess stecken, wenn jemand
    ausdrücklich zusehen will, wenn ohnehin kein Bildschirm dranhängt (Server,
    CI) oder wenn Xvfb gar nicht installiert ist.
    """
    if os.environ.get(SCHON_UNSICHTBAR):
        return False
    if os.environ.get(SICHTBAR_GEWOLLT):
        return False
    if not (os.environ.get('DISPLAY') or os.environ.get('WAYLAND_DISPLAY')):
        return False
    return bool(shutil.which('xvfb-run'))


def verstecken():
    """Notlösung ohne Xvfb (Windows, Mac): Fenster bauen, aber nie zeigen.

    Jedes frisch gebaute Fenster verschwindet sofort, und die drei Befehle, mit
    denen ein Fenster sich nach vorn drängt, werden stillgelegt. Sonst holt ein
    `deiconify()` mitten in der Prüfung alles doch wieder auf den Bildschirm.

    Layout-Prüfungen stört das nicht: Nach `update_idletasks()` stehen Größen
    und Positionen auch bei einem versteckten Fenster.
    """
    try:
        import tkinter as tk
    except ImportError:
        return

    for klasse in (tk.Tk, tk.Toplevel):
        if getattr(klasse, '_scbp_versteckt', False):
            continue
        urspruenglich = klasse.__init__

        def bauen(self, *a, _urspruenglich=urspruenglich, **k):
            _urspruenglich(self, *a, **k)
            try:
                self.withdraw()
            except Exception:
                pass

        klasse.__init__ = bauen
        klasse._scbp_versteckt = True

        # Nach-vorn-Holen stilllegen — genau das reisst den Fokus.
        for name in ('deiconify', 'lift', 'focus_force'):
            setattr(klasse, name, lambda self, *a, **k: None)


def unsichtbar_machen():
    """Zweite Notlösung für Werkzeuge, die GRÖSSEN messen (Windows, Mac).

    Ein verstecktes Fenster liefert keine echte Geometrie — `winfo_width()`
    sagt dort 1. Wer messen will, braucht ein aufgebautes Fenster. Also bleibt
    es aufgebaut, wird aber völlig durchsichtig gestellt und weit neben jeden
    Monitor geschoben.

    ⚠ Das ist der schwächere Schutz: Das Fenster existiert wirklich, kann in
    der Fensterliste auftauchen und theoretisch Fokus ziehen. Deshalb nur dort
    einsetzen, wo gemessen wird — sonst `verstecken()`.
    """
    try:
        import tkinter as tk
    except ImportError:
        return

    for klasse in (tk.Tk, tk.Toplevel):
        if getattr(klasse, '_scbp_beiseite', False):
            continue
        urspruenglich = klasse.__init__

        def bauen(self, *a, _urspruenglich=urspruenglich, **k):
            _urspruenglich(self, *a, **k)
            try:
                self.attributes('-alpha', 0.0)
                self.geometry('+9000+9000')
            except Exception:
                pass

        klasse.__init__ = bauen
        klasse._scbp_beiseite = True
        # Nach vorn holen bleibt auch hier verboten.
        setattr(klasse, 'lift', lambda self, *a, **k: None)

        # ⚠ `focus_force` wird UMGELEITET, nicht stillgelegt (07.09.2026).
        #
        # Stillgelegt war es bis dahin — und damit fielen vier Pruefungen auf
        # Windows durch, die den Fokus in ein Eingabefeld setzen und danach
        # nachsehen, ob er dort steht (Abschnitt 126, Fehlerbericht). Ohne
        # Fokus liefert `focus_get()` None, und die Pruefung meldet „nicht im
        # Feld", ohne dass am Programm etwas fehlt. Aufgefallen erst, als der
        # Selbsttest unter Windows ueberhaupt so weit kam.
        #
        # Ganz freigeben ist keine Loesung: `focus_force` reisst den
        # Tastaturfokus auf Betriebssystem-Ebene an sich — wer gerade Star
        # Citizen fliegt, landet mitten im Kampf auf dem Desktop. Genau davor
        # schuetzt diese Datei.
        #
        # `focus_set` tut das NICHT: Es verteilt den Fokus nur innerhalb der
        # Anwendung. Gemessen am 07.09.2026 unter Windows an einem
        # durchsichtigen, weit weggeschobenen Fenster:
        #   ohne alles          -> focus_get() ist das Fenster
        #   nur focus_set()     -> focus_get() ist das Widget  ✅
        #   mit focus_force()   -> dasselbe Ergebnis
        # Der Zwang bringt hier also nichts, was `focus_set` nicht auch kann.
        #
        # Unter Xvfb bleibt der Zwang noetig (kein Fenstermanager vergibt dort
        # Fokus) — dieser Zweig laeuft dort aber gar nicht: Linux nimmt vorher
        # den echten unsichtbaren Bildschirm.
        #
        # ⚠ `_setzen` als Vorgabewert binden, nicht `klasse` aus der Schleife
        # greifen: Sonst zeigt die Lambda auf die ZULETZT durchlaufene Klasse
        # (`Toplevel`), auch wenn sie an `Tk` haengt. Hier faellt das nicht auf,
        # weil beide `focus_set` von `Misc` erben — genau solche stillen
        # Zufaelle brechen beim naechsten Umbau.
        setattr(klasse, 'focus_force',
                lambda self, *a, _setzen=klasse.focus_set, **k: _setzen(self))


def sicherstellen(breite=1400, hoehe=1000, messend=False):
    """Sorgt dafür, dass dieser Lauf keine Fenster auf den Bildschirm bringt.

    Linux mit Xvfb: startet sich selbst auf einem unsichtbaren Bildschirm neu —
    dann endet der Prozess hier mit dem Rückgabewert des Kindprozesses.
    Sonst (Windows, Mac): versteckt die Fenster und kehrt zurück.
    """
    if os.environ.get(SICHTBAR_GEWOLLT):
        return
    if os.environ.get(SCHON_UNSICHTBAR):
        return
    if os.environ.get('CI'):
        # Auf einem Bau-Laeufer sitzt niemand vor dem Bildschirm. Der Workflow
        # bringt seinen eigenen Xvfb mit, und auf dem Windows-Laeufer wuerde
        # ein verstecktes Fenster die Groessenmessungen verfaelschen: Zwei
        # Pruefungen meldeten dort [1, 1] px, weil ein withdraw()-Fenster keine
        # Geometrie hat. Also hier nicht eingreifen.
        return

    if not noetig():
        # Kein Xvfb zur Hand. Hängt trotzdem ein Bildschirm dran, muss der
        # zweite Weg greifen — sonst blitzt die Prüfung auf dem Bildschirm auf.
        if os.environ.get('DISPLAY') or os.environ.get('WAYLAND_DISPLAY') \
                or sys.platform in ('win32', 'darwin'):
            unsichtbar_machen() if messend else verstecken()
        return

    umgebung = dict(os.environ, **{SCHON_UNSICHTBAR: '1'})
    # -a sucht sich selbst eine freie Nummer, damit sich parallele Läufe
    # nicht gegenseitig den Bildschirm wegnehmen.
    befehl = [
        'xvfb-run', '-a',
        '-s', '-screen 0 %dx%dx24' % (breite, hoehe),
        sys.executable, os.path.abspath(sys.argv[0]),
    ] + sys.argv[1:]

    sys.exit(subprocess.call(befehl, env=umgebung))
