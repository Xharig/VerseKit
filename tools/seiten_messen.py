# -*- coding: utf-8 -*-
"""Wie lange braucht jede Seite — beim ERSTEN Öffnen und beim Wiederkommen?

## Wozu

„Wirkt lahm" ist kein Befund, sondern ein Gefühl. Dieses Werkzeug macht Zahlen
daraus, und zwar getrennt nach den zwei Fällen, die sich völlig anders anfühlen:

| Fall | Was passiert |
|---|---|
| **Erstes Öffnen** | Die Seite wird gebaut. Kostet ihre volle Bauzeit |
| **Wiederkommen** | Sie ist schon da und wird nur eingeblendet |

Der zweite Fall MUSS nahe null liegen. Tut er das nicht, arbeitet beim
Einblenden noch etwas — und das trifft den Nutzer bei **jedem** Klick, nicht
nur beim ersten.

⚠ Der Seiten-Vorbau ist seit dem 02.09.2026 abgeschaltet (siehe `VORBAU_AN` in
`hauptfenster.py`). Die erste Anzeige kostet deshalb genau die Bauzeit. Das ist
gewollt — er hat nie etwas beschleunigt, sondern die Arbeit nur vorverlegt und
dabei die Oberfläche 1,7 Sekunden eingefroren.

## Aufruf

    python3 tools/seiten_messen.py

⚠ Die Zahlen schwanken. Deshalb wird das Wiederkommen **mehrfach** gemessen und
der Mittelwert genommen — eine Einzelmessung als Beleg zu nehmen, hat hier
schon zu falschen Schlüssen geführt.
"""
import os
import sys
import time

HIER = os.path.dirname(os.path.abspath(__file__))
WURZEL = os.path.dirname(HIER)
sys.path.insert(0, WURZEL)
sys.path.insert(0, HIER)

import unsichtbar                                            # noqa: E402
unsichtbar.sicherstellen(messend=True)

from scbp import hauptfenster                                # noqa: E402

WIEDERHOLUNGEN = 3


def _durchlauf(fenster, kennung):
    """Einmal oeffnen und warten, bis Tk wirklich fertig ist."""
    start = time.time()
    fenster.oeffnen(kennung)
    fenster.root.update()
    fenster.root.update_idletasks()
    return (time.time() - start) * 1000.0


def main():
    fenster = hauptfenster.Hauptfenster(version='mess')
    fenster.root.update()
    fenster.root.update_idletasks()

    # ⭐ Dieselbe Liste, die auch die Abnahme benutzt — nicht selbst geraten.
    from abnahme import SEITEN
    kennungen = [name for name, _titel in SEITEN]
    if not kennungen:
        print('Keine Seiten gefunden — `abnahme.SEITEN` ist leer.')
        return 1

    ergebnis = []
    for kennung in kennungen:
        try:
            erst = _durchlauf(fenster, kennung)
        except Exception as fehler:
            print('  !! %-16s %s: %s'
                  % (kennung, type(fehler).__name__, fehler))
            continue
        # Wiederkommen: erst woanders hin, dann zurueck.
        anders = [k for k in kennungen if k != kennung][:1]
        wieder = []
        for _ in range(WIEDERHOLUNGEN):
            if anders:
                _durchlauf(fenster, anders[0])
            wieder.append(_durchlauf(fenster, kennung))
        ergebnis.append((erst, sum(wieder) / len(wieder), kennung))

    # ⭐ Und die Frage dahinter: WAS kostet beim Wiederkommen Zeit?
    #
    # Eine gebaute Seite wird nur ein- und ausgeblendet — bis auf den Rueckruf
    # in `beim_zeigen`, der alles auffrischt, was nicht stehenbleiben soll
    # (Suchfelder, Auswahl). Der laeuft bei JEDEM Klick.
    rueckrufe = getattr(fenster, 'beim_zeigen', {}) or {}
    kosten = []
    for kennung in sorted(rueckrufe):
        ruf = rueckrufe[kennung]
        start = time.time()
        try:
            ruf()
            fenster.root.update()
            fenster.root.update_idletasks()
        except Exception as fehler:
            kosten.append((-1.0, kennung, type(fehler).__name__))
            continue
        kosten.append(((time.time() - start) * 1000.0, kennung, ''))
    if kosten:
        kosten.sort(reverse=True)
        print('\n  Was der Rueckruf beim Anzeigen kostet:')
        for ms, kennung, problem in kosten:
            print('    %-18s %7.0f ms %s' % (kennung, ms, problem))

    ergebnis.sort(reverse=True)
    print('\n  %-18s %10s %12s' % ('Seite', 'erstmals', 'wiederkommen'))
    print('  ' + '-' * 42)
    for erst, wieder, kennung in ergebnis:
        marke = ''
        if wieder > 50:
            marke = '  <- blendet nicht nur ein'
        elif erst > 300:
            marke = '  <- teurer Aufbau'
        print('  %-18s %7.0f ms %9.0f ms%s' % (kennung, erst, wieder, marke))
    print('\n  Summe erstmals: %.0f ms ueber %d Seiten'
          % (sum(e for e, _w, _k in ergebnis), len(ergebnis)))
    fenster.root.destroy()
    return 0


if __name__ == '__main__':
    sys.exit(main())
