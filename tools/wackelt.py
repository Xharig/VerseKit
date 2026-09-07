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
Findet Prüfungen, die mal grün und mal rot sind.

**Wozu das gut ist.** Am 05./06.09.2026 wurde viermal beobachtet: Beim ersten
Lauf schlägt eine Prüfung an, beim Wiederholen ist dieselbe grün. Welche es
war, ließ sich hinterher nicht mehr sagen — 1794 Prüfungen ziehen am Auge
vorbei, und wer den Lauf zweimal von Hand vergleicht, findet den Unterschied
nicht. Ein Test, der mal so und mal so ausgeht, wird irgendwann ignoriert; und
dann fällt ein echter Fehler nicht mehr auf.

Dieses Werkzeug lässt den Selbsttest mehrfach laufen und nennt **genau die
Prüfungen, die zwischen den Läufen umspringen**.

**Und es sagt gleich, woran es liegt.** Dafür gibt es zwei Betriebsarten:

    python3 tools/wackelt.py              # zwei Läufe, DERSELBE Ablageordner
    python3 tools/wackelt.py --frisch     # zwei Läufe, jeder mit LEEREM Ordner

| Wackelt in… | Bedeutung |
|---|---|
| gleichem Ordner, nicht in frischem | Die Prüfung hängt am **Zustand der Ablage** — der erste Lauf legt etwas an, den der zweite vorfindet. Das ist der häufige Fall und immer ein Mangel der Prüfung: Sie soll sich ihre Daten selbst hinlegen. |
| beidem | Echte Unbeständigkeit — Zeitmessung, Thread, Netz, Reihenfolge. |
| nur in frischem | Die Prüfung braucht etwas, das erst entsteht, und übersieht das beim ersten Mal. |

⚠ **Gearbeitet wird immer in einem Wegwerf-Ordner** (`SC_BP_HOME`), nie in der
echten Ablage. Sonst stünden die Spuren dieser Läufe hinterher im eigenen
Bestand und im Fehlerbericht.

⚠ **Es dauert.** Ein Selbsttest-Durchlauf braucht mehrere Minuten; die Voreinstellung
sind zwei. Jeder Lauf meldet sich, sobald er fertig ist, damit man sieht, dass
noch etwas passiert.

Weitere Schalter:

    --laeufe 3        mehr als zwei Durchläufe (findet seltenere Wackler)
    --behalten        die Ausgaben nicht wegwerfen, Pfade werden genannt
"""
import argparse
import os
import re
import shutil
import subprocess
import sys
import tempfile
from collections import OrderedDict

WURZEL = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SELBSTTEST = os.path.join(WURZEL, 'tools', 'selbsttest.py')

# ⚠ Die eigene Ausgabe auf UTF-8 stellen, sonst stirbt sie unter Windows.
# Dort steht die Konsole auf cp1252, und die Pruefttexte enthalten Umlaute und
# Pfeile. Beim ersten Versuch am 07.09.2026 brach genau hier alles ab
# (`UnicodeEncodeError` auf `�`) — und zwar erst NACH zwei fertigen
# Laeufen, also nach zehn Minuten Wartezeit. Dieselbe Falle wie in den
# Riegel-Selbsttests.
for _strom in (sys.stdout, sys.stderr):
    try:
        _strom.reconfigure(encoding='utf-8', errors='replace')
    except (AttributeError, ValueError):        # sehr alte Fassungen, Umleitung
        pass

# `  [ok]   text` bzw. `  [FEHL] text` — so schreibt `pruefe()` in selbsttest.py.
#
# ⚠ `re.MULTILINE` ist Pflicht. Ohne das bindet `^` nur an den Anfang der
# GANZEN Ausgabe, und `findall()` liefert null Treffer — beim ersten echten
# Lauf am 07.09.2026 meldete die Fortschrittszeile brav „0 Pruefungen, 0 rot"
# fuer beide Durchlaeufe, waehrend die Auswertung darunter 1794 zaehlte.
# (`auswerten()` war nie betroffen: Es geht zeilenweise mit `.match()`.)
ZEILE = re.compile(r'^ {2}\[(ok|FEHL)\] {1,3}(.*)$', re.MULTILINE)
# `60. Mausrad rollt die Klappliste, nicht die Seite dahinter`
ABSCHNITT = re.compile(r'^(\d+[a-z]?)\. (.*)$')

# ⚠ Zahlen aus dem Prüftext werfen, bevor verglichen wird. Viele Prüfungen
# schreiben ihren Messwert mit hinein („das Rad rollt die Klappliste
# (0.000 -> 0.101)"). Ohne diesen Schritt gälte jede solche Zeile bei jedem
# Lauf als eine ANDERE Prüfung, und das Werkzeug fände nie ein Paar zum
# Vergleichen. Die Zahl bleibt in der Anzeige erhalten, nur der Schlüssel
# wird davon befreit.
ZAHLEN = re.compile(r'-?\d+(?:[.,]\d+)?')


def _schluessel(abschnitt, text, gesehen):
    """Eine über Läufe hinweg stabile Kennung für eine einzelne Prüfung.

    Abschnitt + entzifferter Text. Kommt derselbe Text in einem Abschnitt
    mehrfach vor (bei Schleifen üblich), zählt ein Zusatz sie durch.
    """
    roh = '%s|%s' % (abschnitt, ZAHLEN.sub('#', text).strip())
    gesehen[roh] = gesehen.get(roh, 0) + 1
    return '%s#%d' % (roh, gesehen[roh])


def auswerten(ausgabe):
    """Aus einem Lauf: Kennung -> (Ergebnis, Abschnitt, Titel, Anzeigetext)."""
    ergebnisse = OrderedDict()
    abschnitt, titel, gesehen = '0', '(vor dem ersten Abschnitt)', {}
    for roh in ausgabe.splitlines():
        kopf = ABSCHNITT.match(roh)
        if kopf:
            abschnitt, titel = kopf.group(1), kopf.group(2)
            continue
        treffer = ZEILE.match(roh)
        if treffer:
            stand, text = treffer.group(1), treffer.group(2)
            ergebnisse[_schluessel(abschnitt, text, gesehen)] = (
                stand, abschnitt, titel, text)
    return ergebnisse


def einmal_laufen(ablage, nummer, gesamt):
    """Einen Selbsttest starten und seine Ausgabe zurückgeben."""
    umgebung = dict(os.environ)
    umgebung['SC_BP_HOME'] = ablage
    # ⚠ Ohne das schreibt der Kindprozess unter Windows in cp1252 und stirbt an
    # den Umlauten der Prüftexte — dieselbe Falle wie in den Riegel-Selbsttests.
    umgebung['PYTHONIOENCODING'] = 'utf-8'

    print('  Lauf %d von %d laeuft … ' % (nummer, gesamt), end='')
    sys.stdout.flush()
    lauf = subprocess.run([sys.executable, SELBSTTEST],
                          capture_output=True, text=True,
                          encoding='utf-8', errors='replace',
                          env=umgebung, cwd=WURZEL)

    # ⚠ **Nur `stdout` auswerten, `stderr` getrennt halten.** Beides
    # aneinanderzuhaengen sah harmlos aus und war es nicht: Tk schreibt beim
    # Aufraeumen Zeilen wie `invalid command name "…poll_queue"` ohne
    # abschliessenden Umbruch. Die klebten dann hinten an einem Pruefttext,
    # und aus „…nicht nur zufaellig einen" wurde „…zufaellig eneninvalid
    # command n" — eine Pruefung, die es in keinem anderen Lauf gibt. Das
    # Werkzeug haette also GERADE die Wackler erfunden, die es finden soll.
    ausgabe = lauf.stdout or ''
    nebenher = lauf.stderr or ''

    anzahl = len(ZEILE.findall(ausgabe))
    rot = sum(1 for z in ausgabe.splitlines() if z.startswith('  [FEHL]'))
    abbruch = 'Traceback' in ausgabe or 'Traceback' in nebenher
    print('fertig: %d Pruefungen, %d rot%s'
          % (anzahl, rot, ', ABGEBROCHEN' if abbruch else ''))
    return ausgabe


def main():
    zerleger = argparse.ArgumentParser(
        description='Findet Pruefungen, die zwischen zwei Laeufen umspringen.')
    zerleger.add_argument('--frisch', action='store_true',
                          help='jeder Lauf bekommt einen LEEREN Ablageordner')
    zerleger.add_argument('--laeufe', type=int, default=2,
                          help='Anzahl der Durchlaeufe (Vorgabe: 2)')
    zerleger.add_argument('--behalten', action='store_true',
                          help='Ausgaben nicht wegwerfen')
    args = zerleger.parse_args()

    if args.laeufe < 2:
        print('Mindestens zwei Laeufe — sonst gibt es nichts zu vergleichen.')
        return 2

    art = 'jeweils LEERER Ordner' if args.frisch else 'DERSELBE Ordner'
    print('Selbsttest %dx laufen lassen (%s).' % (args.laeufe, art))
    print('Das dauert einige Minuten je Lauf.')
    print()

    arbeitsordner = tempfile.mkdtemp(prefix='scbp-wackelt-')
    gemeinsam = os.path.join(arbeitsordner, 'ablage')
    os.makedirs(gemeinsam, exist_ok=True)

    laeufe, ausgaben = [], []
    try:
        for i in range(1, args.laeufe + 1):
            if args.frisch:
                ablage = os.path.join(arbeitsordner, 'ablage-%d' % i)
                os.makedirs(ablage, exist_ok=True)
            else:
                ablage = gemeinsam
            ausgabe = einmal_laufen(ablage, i, args.laeufe)
            ausgaben.append(ausgabe)
            laeufe.append(auswerten(ausgabe))

        if args.behalten:
            for i, ausgabe in enumerate(ausgaben, 1):
                ziel = os.path.join(arbeitsordner, 'lauf-%d.txt' % i)
                with open(ziel, 'w', encoding='utf-8') as datei:
                    datei.write(ausgabe)
                print('  Ausgabe %d: %s' % (i, ziel))

        print()
        return berichten(laeufe, args)
    finally:
        if not args.behalten:
            shutil.rmtree(arbeitsordner, ignore_errors=True)


def berichten(laeufe, args):
    """Vergleicht die Läufe und schreibt den Befund."""
    erster = laeufe[0]

    # ⚠ Prüfungen, die NICHT in jedem Lauf vorkommen, sind ebenfalls ein
    # Befund — und zwar ein schwerwiegenderer: Bricht ein Lauf ab, fehlt alles
    # dahinter. Das darf nicht als „stabil" durchgehen, nur weil es sich nicht
    # vergleichen lässt.
    ueberall = set(erster)
    for weiterer in laeufe[1:]:
        ueberall &= set(weiterer)

    fehlend = [k for k in erster if k not in ueberall]
    for weiterer in laeufe[1:]:
        fehlend += [k for k in weiterer if k not in ueberall and k not in erster]

    wackler = []
    for kennung in erster:
        if kennung not in ueberall:
            continue
        staende = [lauf[kennung][0] for lauf in laeufe]
        if len(set(staende)) > 1:
            _, abschnitt, titel, text = erster[kennung]
            wackler.append((abschnitt, titel, text, staende))

    breite = 72
    print('=' * breite)
    if not wackler and not fehlend:
        print('Keine Pruefung ist umgesprungen — %d Pruefungen, %d Laeufe.'
              % (len(ueberall), len(laeufe)))
        print()
        if args.frisch:
            print('Gegenprobe: ohne --frisch laufen lassen (derselbe Ordner).')
            print('Wackelt es DORT, haengt die Pruefung am Zustand der Ablage.')
        else:
            print('Gegenprobe: mit --frisch laufen lassen (je leerer Ordner).')
            print('Wackelt es dort NICHT, ist es kein Zufall, sondern der')
            print('Ordnerzustand — dann legt sich die Pruefung ihre Daten')
            print('nicht selbst hin.')
        return 0

    if wackler:
        print('%d Pruefung(en) sind umgesprungen:' % len(wackler))
        print()
        letzter = None
        for abschnitt, titel, text, staende in wackler:
            if abschnitt != letzter:
                print('  %s. %s' % (abschnitt, titel))
                letzter = abschnitt
            print('     %-52s %s' % (text[:52], ' -> '.join(staende)))
        print()

    if fehlend:
        print('%d Pruefung(en) kamen nicht in JEDEM Lauf vor:' % len(fehlend))
        # ⚠ Ehrlich bleiben: Das hat ZWEI Ursachen, und die zweite ist
        # harmlos. Ein Abbruch reisst alles Dahinterliegende mit — das ist der
        # schlimme Fall. Wurde dagegen nur ein Prueftext umformuliert und die
        # Laeufe stammen aus verschiedenen Codestaenden, ist es dieselbe
        # Pruefung unter neuem Namen und gar kein Befund.
        print('(entweder ein Abbruch — dann fehlt alles dahinter —')
        print(' oder ein umformulierter Pruefttext zwischen zwei Staenden)')
        print()
        for kennung in fehlend[:15]:
            quelle = erster.get(kennung)
            for lauf in laeufe:
                quelle = quelle or lauf.get(kennung)
            if quelle:
                print('     %s. %s' % (quelle[1], quelle[3][:60]))
        if len(fehlend) > 15:
            print('     … und %d weitere' % (len(fehlend) - 15))
        print()

    print('=' * breite)
    if args.frisch:
        print('Diese Laeufe hatten JE EINEN LEEREN Ordner. Was hier wackelt,')
        print('haengt nicht am Ordnerzustand, sondern an Zeit, Threads, Netz')
        print('oder Reihenfolge.')
    else:
        print('Diese Laeufe teilten sich EINEN Ordner. Jetzt gegenpruefen:')
        print()
        print('    %s %s --frisch' % (os.path.basename(sys.executable),
                                      os.path.relpath(__file__, os.getcwd())))
        print()
        print('Bleibt es dort ruhig, liegt es am Zustand der Ablage — die')
        print('Pruefung legt sich ihre Daten nicht selbst hin.')
    return 1


if __name__ == '__main__':
    sys.exit(main())
