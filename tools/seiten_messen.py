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
    """Einmal oeffnen und warten, bis Tk wirklich fertig ist.

    ⚠ `perf_counter()` statt `time()`: Die Systemuhr kann springen, und unter
    Windows ist ihre Aufloesung fuer Millisekunden zu grob.

    ⚠ `update()` arbeitet auch **andere faellige Rueckrufe** ab. Die Zahlen
    sind damit „was der Nutzer wartet", nicht „was diese eine Funktion
    kostet" — fuer die zweite Frage ist ein Profillauf das richtige Werkzeug.
    """
    start = time.perf_counter()
    fenster.oeffnen(kennung)
    fenster.root.update()
    fenster.root.update_idletasks()
    return (time.perf_counter() - start) * 1000.0


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

    # ⚠⚠ **Erst ALLE kalten Aufbauten, dann erst die Wiederbesuche.**
    #
    # Die erste Fassung mass je Seite sofort auch das Wiederkommen — und baute
    # dafuer die naechste Seite schon auf. Deren „erstmals" war dann in
    # Wahrheit ein warmer Aufruf. Vom Pruefer nachgestellt (12.09.2026) mit
    # simulierten 100 ms kalt / 1 ms warm: Zwei Seiten erschienen beide mit
    # 1 ms als Erstoeffnung.
    #
    # ⚠ Und die Startseite ist beim Bauen des Fensters **schon offen**. Ihr
    # „erstmals" laesst sich hier nicht mehr messen; sie wird deshalb
    # gekennzeichnet statt stillschweigend falsch gezaehlt.
    schon_offen = set(getattr(fenster, 'gezeichnet', ()) or ())
    kalt = {}
    for kennung in kennungen:
        if kennung in schon_offen:
            kalt[kennung] = None            # war vor der Messung schon da
            continue
        try:
            kalt[kennung] = _durchlauf(fenster, kennung)
        except Exception as fehler:
            print('  !! %-16s %s: %s'
                  % (kennung, type(fehler).__name__, fehler))

    ergebnis = []
    for kennung in kennungen:
        if kennung not in kalt:
            continue
        anders = [k for k in kennungen if k != kennung][:1]
        wieder = []
        for _ in range(WIEDERHOLUNGEN):
            if anders:
                _durchlauf(fenster, anders[0])
            wieder.append(_durchlauf(fenster, kennung))
        ergebnis.append((kalt[kennung] if kalt[kennung] is not None else -1.0,
                         sum(wieder) / len(wieder), kennung))

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
        erst_text = ('   (war offen)' if erst < 0 else '%7.0f ms' % erst)
        print('  %-18s %12s %9.0f ms%s'
              % (kennung, erst_text, wieder, marke))
    echte = [e for e, _w, _k in ergebnis if e >= 0]
    print('\n  Summe erstmals: %.0f ms ueber %d Seiten (%d waren schon offen)'
          % (sum(echte), len(echte), len(ergebnis) - len(echte)))
    fenster.root.destroy()
    return 0


if __name__ == '__main__':
    sys.exit(main())
