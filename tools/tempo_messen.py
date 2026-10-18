"""Wie teuer ist jeder Reiter? — Seitenwechsel messen, ohne zu raten.

Warum das ein eigenes Werkzeug ist: Ob das Programm träger geworden ist,
lässt sich nicht mit dem Auge klären — und die Uhrzeit lügt, sobald nebenher
etwas anderes (etwa Star Citizen) den Rechner belegt. Verlässlich ist erst
das **Zählen**.

Deshalb misst dieses Werkzeug zwei Dinge, die von fremder Last kaum abhängen:

| Größe | was sie sagt |
|---|---|
| **Tk-Aufrufe** | wie viel Arbeit Tk gegeben wird — exakt, bei jedem Lauf gleich |
| **CPU-Zeit** | wie lange DIESER Prozess gerechnet hat (`time.process_time`), nicht wie lange die Uhr lief |

Gemessen wird jeder Reiter beim **ersten** Öffnen (die Seite wird gebaut)
und beim **Wiederkommen** (sie wird nur gezeigt und frischt sich auf, dreimal,
der mittlere Wert zählt). Dazu das erste Öffnen des Hauptfensters.

Die Spalte **Seite** ist die Zeit in `open_page` allein — dieselbe Zahl, die
die Startspur je Seite in Millisekunden in den Fehlerbericht schreibt. Die
CPU-Zeit enthält dagegen auch das anschließende `update()` (Zeichnen, andere
fällige Rückrufe) und alle Fäden des Prozesses.

Aufruf:
    python tools/tempo_messen.py              # Wegwerf-Ordner, leere Daten
    SC_BP_HOME=<Kopie> python tools/tempo_messen.py
    python tools/tempo_messen.py --json aus.json

⚠ Mit echten Daten messen heißt: eine **Kopie** nehmen (siehe
`tools/bilder_machen.py`, `datenstand_kopieren`) — nie den echten Ordner.
"""
import json
import os
import sys
import tempfile
import time

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import unsichtbar                                              # noqa: E402
unsichtbar.sicherstellen(messend=True)
import ausgabe                                                 # noqa: E402
ausgabe.utf8()

os.environ.setdefault('SC_BP_HOME', tempfile.mkdtemp(prefix='tempo-'))
os.environ['SC_BP_NO_NET'] = '1'

import tkinter as tk                                           # noqa: E402


class _Zaehler:
    """Steht zwischen Python und Tk und zählt jeden Aufruf mit."""

    def __init__(self, echt):
        self._echt = echt
        self.anzahl = 0

    def call(self, *args):
        self.anzahl += 1
        return self._echt.call(*args)

    def __getattr__(self, name):
        return getattr(self._echt, name)


def _ruhe(wurzel, sekunden=0.3):
    """Tk alles abarbeiten lassen, was nachläuft (`after`, Umbrüche)."""
    ende = time.time() + sekunden
    while time.time() < ende:
        wurzel.update()
        time.sleep(0.01)


def messen(fenstergroesse='1294x1284'):
    """Gibt ein Wörterbuch zurück: Hauptfenster und je Seite erst/wieder."""
    from scbp import main_window, pages, language
    language.set_language('de')
    wurzel = tk.Tk()
    wurzel.withdraw()
    zaehler = _Zaehler(wurzel.tk)
    wurzel.tk = zaehler

    def einmal(tun):
        vorher_n = zaehler.anzahl
        vorher_cpu = time.process_time()
        vorher_uhr = time.perf_counter()
        tun()
        return {'tk': zaehler.anzahl - vorher_n,
                'cpu_ms': round((time.process_time() - vorher_cpu) * 1000),
                'uhr_ms': round((time.perf_counter() - vorher_uhr) * 1000)}

    ergebnis = {'seiten': {}}
    fenster = {}

    def oeffnen():
        fenster['f'] = main_window.MainWindow(wurzel)
        fenster['f'].root.geometry(fenstergroesse)
        fenster['f'].open_page('allgemein')
        wurzel.update()

    ergebnis['hauptfenster'] = einmal(oeffnen)
    f = fenster['f']
    _ruhe(wurzel)
    seiten = list(pages.page_ids())
    for runde in ('erst', 'wieder', 'wieder', 'wieder'):
        for seite in seiten:
            nur_seite = []

            def zeigen(s=seite):
                start = time.perf_counter()
                f.open_page(s)
                nur_seite.append((time.perf_counter() - start) * 1000)
                wurzel.update()
            try:
                wert = einmal(zeigen)
                wert['seite_ms'] = round(nur_seite[0], 1)
            except Exception as ausnahme:          # eine Seite darf nicht alles kippen
                wert = {'fehler': str(ausnahme)}
            ergebnis['seiten'].setdefault(seite, {}).setdefault(
                runde, []).append(wert)
            _ruhe(wurzel, 0.05)
        _ruhe(wurzel, 0.5)
    # Je Runde ein Wert: das erste Öffnen gibt es einmal, vom Wiederkommen
    # zählt der mittlere von drei Läufen (nach der Seitenzeit sortiert).
    for werte in ergebnis['seiten'].values():
        werte['erst'] = werte['erst'][0]
        laeufe = sorted(werte['wieder'],
                        key=lambda w: w.get('seite_ms', float('inf')))
        werte['wieder'] = laeufe[len(laeufe) // 2]
    ergebnis['schliessen'] = einmal(lambda: (f.close(), wurzel.update()))
    try:
        wurzel.destroy()
    except tk.TclError:
        pass
    return ergebnis


def ausgeben(e):
    h = e['hauptfenster']
    print('Hauptfenster öffnen: %6d Tk-Aufrufe · %5d ms CPU · %5d ms Uhr'
          % (h['tk'], h['cpu_ms'], h['uhr_ms']))
    s = e['schliessen']
    print('Hauptfenster schließen: %3d Tk-Aufrufe · %5d ms CPU · %5d ms Uhr'
          % (s['tk'], s['cpu_ms'], s['uhr_ms']))
    print()
    print('%-22s %8s %8s %10s | %8s %8s %10s' % (
        'Reiter', 'erst Tk', 'erst CPU', 'erst Seite',
        'wieder Tk', 'wied CPU', 'wied Seite'))
    zeilen = sorted(e['seiten'].items(),
                    key=lambda kv: -kv[1].get('wieder', {}).get('seite_ms', 0))
    summe = {'erst': [0, 0, 0.0], 'wieder': [0, 0, 0.0]}
    for seite, w in zeilen:
        teile = []
        for runde in ('erst', 'wieder'):
            x = w.get(runde, {})
            if 'fehler' in x:
                teile += ['FEHLER', '', '']
                continue
            summe[runde][0] += x.get('tk', 0)
            summe[runde][1] += x.get('cpu_ms', 0)
            summe[runde][2] += x.get('seite_ms', 0)
            teile += ['%d' % x.get('tk', 0), '%d ms' % x.get('cpu_ms', 0),
                      '%.0f ms' % x.get('seite_ms', 0)]
        print('%-22s %8s %8s %10s | %8s %8s %10s' % (seite, *teile))
    print('%-22s %8d %5d ms %7.0f ms | %8d %5d ms %7.0f ms' % (
        'Summe', summe['erst'][0], summe['erst'][1], summe['erst'][2],
        summe['wieder'][0], summe['wieder'][1], summe['wieder'][2]))


if __name__ == '__main__':
    ergebnis = messen()
    ausgeben(ergebnis)
    if '--json' in sys.argv:
        ziel = sys.argv[sys.argv.index('--json') + 1]
        with open(ziel, 'w', encoding='utf-8') as datei:
            json.dump(ergebnis, datei, indent=1)
        print('\nGeschrieben:', ziel)
