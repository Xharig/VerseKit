# -*- coding: utf-8 -*-
"""Gegenprobe: Beschreibt der Abdruck der Bauplan-Liste wirklich das Bild?

Zwei Ausgaenge von `_zeichnen()` fuehren am Ende der Funktion vorbei — und
genau die hat der Pruefer am 12.09.2026 gefunden:

| Ausgang | Was passiert |
|---|---|
| **Leerer Katalog** | Die Zeilen sind schon zerstoert, dann `return` |
| **Lange Liste** | Der Aufbau laeuft erst im Leerlauf (`after_idle`) |

In beiden Faellen behauptete der alte Abdruck weiter, das vorige Bild stehe
noch — und `neu_laden()` sprang ab. Auf dem Bildschirm blieb es leer.

    python3 tools/probe_liste_abdruck.py

⚠ Laeuft in einem Wegwerf-Ordner (`SC_BP_HOME`) — nie in der echten Ablage.
"""
import json
import os
import shutil
import sys
import tempfile

WURZEL = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, WURZEL)
sys.path.insert(0, os.path.join(WURZEL, 'tools'))

HEIM = tempfile.mkdtemp(prefix='probe-liste-')
os.environ['SC_BP_HOME'] = HEIM            # ⚠ vor dem Import von `scbp`
os.environ['SC_BP_NO_NET'] = '1'

import unsichtbar                                            # noqa: E402
unsichtbar.sicherstellen(messend=True)

import tkinter as tk                                         # noqa: E402
from scbp import bestandsfenster, catalog as katalog_modul   # noqa: E402

OK = []


def p(bedingung, text):
    OK.append((bool(bedingung), text))
    print('  [%s] %s' % ('ok' if bedingung else 'XX', text))


def katalog_schreiben(namen):
    daten = {'version': 'probe', 'geholt': '2026-09-12', 'missionen': {},
             'bauplaene': {n.lower(): {'n': n, 'a': 'Waffe'} for n in namen}}
    with open(os.path.join(HEIM, katalog_modul.CACHE), 'w',
              encoding='utf-8') as f:
        json.dump(daten, f)


def zeilen(seite):
    return len(seite.inhalt.winfo_children())


def main():
    namen = ['Probe Alpha', 'Probe Beta', 'Probe Gamma',
             'Probe Delta', 'Probe Epsilon']
    katalog_schreiben(namen)

    wurzel = tk.Tk()
    wurzel.geometry('720x780')
    rahmen = tk.Frame(wurzel)
    rahmen.pack(fill='both', expand=True)
    seite = bestandsfenster.Bestandsfenster(rahmen=rahmen)
    wurzel.update()

    # ── 1. Der Weg ueber den leeren Katalog ──────────────────────────────
    seite.neu_laden(auch_katalog=True)
    wurzel.update()
    voll = zeilen(seite)
    p(voll > 1, 'Zustand A gezeichnet (%d Bauteile im Inhalt)' % voll)
    p(seite._letzter_stand is not None, 'A: Abdruck geschrieben')

    katalog_schreiben([])                      # Katalog weg
    seite.neu_laden(auch_katalog=True)
    wurzel.update()
    leer = zeilen(seite)
    p(leer < voll, 'leerer Katalog: Liste ist weg (%d Bauteile)' % leer)

    katalog_schreiben(namen)                   # A wiederherstellen
    seite.neu_laden(auch_katalog=True)
    wurzel.update()
    zurueck = zeilen(seite)
    p(zurueck == voll,
      'A kommt zurueck (%d Bauteile, erwartet %d)' % (zurueck, voll))

    # ── 2. Der Weg ueber die Bloecke ─────────────────────────────────────
    #
    # Der Deckel wird heruntergesetzt, damit schon fuenf Zeilen in Bloecken
    # gebaut werden — sonst braeuchte die Probe ueber 700 Eintraege.
    seite.alle_zeigen = True
    seite._zeilen_deckel = lambda: 2
    seite._letzter_stand = None
    seite._zeichnen()
    p(seite._letzter_stand is None,
      'Bloecke: nach `_zeichnen()` noch KEIN Abdruck — der Aufbau ist nur '
      'eingeplant')
    wurzel.update()                            # jetzt laeuft `after_idle`
    p(seite._letzter_stand is not None,
      'Bloecke: nach dem Aufbau ist der Abdruck gueltig')
    p(len(seite._blockteile) > 0,
      'Bloecke: es stehen welche (%d)' % len(seite._blockteile))

    # ── 3. Zwei Zeichenauftraege kurz hintereinander ─────────────────────
    #
    # ⚠⚠ Der Fall, den die Laufnummer abfaengt: Der lange Aufbau ist
    # eingeplant, laeuft aber erst im Leerlauf — und bis dahin hat laengst
    # ein zweiter Zeichenvorgang stattgefunden. Ohne Nummer legt der alte
    # Auftrag dann seine Bloecke ueber das neue Bild.
    seite.alle_zeigen = True
    seite._zeilen_deckel = lambda: 2
    seite._zeichnen()                          # lang: plant Bloecke ein
    seite.alle_zeigen = False                  # und jetzt: kurze Ansicht
    seite._zeilen_deckel = lambda: 999
    seite._zeichnen()                          # geradlinig, setzt Abdruck
    kurz = seite._letzter_stand
    p(kurz is not None, 'kurze Ansicht: Abdruck steht')
    wurzel.update()                            # der ueberholte Auftrag laeuft
    p(len(seite._blockteile) == 0,
      'der ueberholte Auftrag hat KEINE Bloecke gelegt (%d)'
      % len(seite._blockteile))
    p(seite._letzter_stand == kurz,
      'und er hat den Abdruck der kurzen Ansicht nicht angefasst')

    # ── 4. Der Aufbau scheitert ──────────────────────────────────────────
    #
    # ⛔ `_bloecke_pflegen()` faengt `TclError` ab. Seine Rueckkehr ist damit
    # kein Beleg fuer einen geglueckten Aufbau — geprueft wird, dass der
    # Abdruck bei einem Fehlschlag UNGUELTIG bleibt.
    echt_bauen = seite._block_bauen

    def kaputt(_nummer):
        raise tk.TclError('Probe: Aufbau absichtlich gescheitert')

    seite._block_bauen = kaputt
    seite.alle_zeigen = True
    seite._zeilen_deckel = lambda: 2
    seite._zeichnen()
    wurzel.update()
    p(seite._letzter_stand is None,
      'gescheiterter Aufbau -> Abdruck bleibt ungueltig (%r)'
      % (seite._letzter_stand,))
    p(len(seite._blockteile) == 0,
      'und es steht wirklich nichts (%d Bloecke)' % len(seite._blockteile))
    seite._block_bauen = echt_bauen

    wurzel.destroy()
    print('\n  %d von %d' % (sum(1 for b, _ in OK if b), len(OK)))
    return 0 if all(b for b, _ in OK) else 1


if __name__ == '__main__':
    try:
        code = main()
    finally:
        shutil.rmtree(HEIM, ignore_errors=True)
    sys.exit(code)
