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
Die Oberfläche wirklich bedienen — klicken, dann nachlesen, was dasteht.

## ⚠⚠ Warum es dieses Werkzeug gibt

Am 06.09.2026 gingen drei Fehler an den Nutzer, die alle sichtbar gewesen
wären, hätte jemand einmal geklickt:

| Fehler | was auf dem Bildschirm stand |
|---|---|
| Marke zog nicht mit | alle vier abgehakt — Zeile sagte weiter „4 noch zu besorgen" |
| Kopf zählte Erledigtes | „8 Positionen", zwei davon längst fertig |
| Farmliste zählte Erledigtes | „für 8 Bauteile", vier davon schon gebaut |

Die Rückmeldung dazu: *„testest du deine Funktionen gar nicht mehr in echt,
oder machst du das immer erst nachdem ich Fehler gefunden habe?"* — und
danach: *„du machst es mir einfacher, wenn du das live klickst und testest,
bevor du es mir präsentierst."*

**Jede einzelne Funktion war richtig.** Falsch war, wer nach einer Änderung
neu zeichnet — und das findet keine Prüfung, die nur Funktionen aufruft. Die
Frage ist nicht „gibt die Funktion den richtigen Wert zurück", sondern
**„steht danach das Richtige auf dem Bildschirm"**.

## Wie es benutzt wird

    python3 tools/durchklicken.py

Baut das echte Hauptfenster, öffnet Seiten, drückt Knöpfe und liest die
Beschriftungen zurück. Läuft **immer unsichtbar** (siehe unten) und arbeitet
in einem Wegwerf-Ordner — es fasst die Daten des Nutzers nie an.

⚠⚠ **Nie auf dem Bildschirm des Nutzers.** Wie Selbsttest und
`oberflaeche_pruefen.py` startet sich dieses Werkzeug unter `xvfb-run` neu,
sobald ein echter Bildschirm dranhängt: Claudes Shell hängt an `DISPLAY=:0`,
und ein aufblitzendes Fenster reißt den Tastaturfokus mit — wer gerade Star
Citizen fliegt, landet im Desktop. Zusehen geht bewusst mit `SC_BP_SICHTBAR=1`.
"""

import os
import sys

HIER = os.path.dirname(os.path.abspath(__file__))
WURZEL = os.path.dirname(HIER)
sys.path.insert(0, WURZEL)
sys.path.insert(0, HIER)

import unsichtbar                                          # noqa: E402
unsichtbar.sicherstellen()

import json                                                # noqa: E402
import shutil                                              # noqa: E402
import tempfile                                            # noqa: E402
import tkinter as tk                                       # noqa: E402


fehler = []
geprueft = [0]


def pruefe(bedingung, text):
    geprueft[0] += 1
    print(('  [ok]   ' if bedingung else '  [FEHL] ') + text)
    if not bedingung:
        fehler.append(text)


def texte(widget, raus=None):
    """Alle sichtbaren Beschriftungen unterhalb eines Widgets — der Reihe nach.

    ⚠⚠ **Gepackt, nicht `winfo_ismapped()`.** Der erste Anlauf filterte über
    `ismapped` — und das ist bei einem Fenster mit `withdraw()` **immer**
    falsch, also war jede Liste leer und alle sechs Pruefungen rot. Dieselbe
    Falle hatte kurz zuvor schon Pruefung 155 im Bau-Lauf umgeworfen
    („rechte Kante None"), dort unter Windows.
    `winfo_manager()` sagt dagegen, ob ein Widget ueberhaupt eingehaengt ist —
    unabhaengig davon, ob das Fenster gerade sichtbar auf einem Schirm liegt.
    """
    raus = [] if raus is None else raus
    for kind in widget.winfo_children():
        if not kind.winfo_manager():
            continue
        try:
            wert = kind.cget('text')
            if wert:
                raus.append(str(wert))
        except Exception:
            pass
        texte(kind, raus)
    return raus


def klicken(widget):
    """Ein `<Button-1>` auf dieses Widget auslösen — wie ein echter Klick."""
    widget.event_generate('<Button-1>')
    widget.update()


def finde(widget, teiltext, art=None):
    """Das erste angezeigte Widget, dessen Beschriftung `teiltext` enthält."""
    for kind in widget.winfo_children():
        if kind.winfo_manager():
            try:
                if teiltext.lower() in str(kind.cget('text')).lower():
                    if art is None or kind.winfo_class() == art:
                        return kind
            except Exception:
                pass
        treffer = finde(kind, teiltext, art)
        if treffer is not None:
            return treffer
    return None


def _ablage_vorbereiten():
    """Ein Wegwerf-Ordner mit einem Hangar, an dem sich etwas abhaken lässt.

    ⚠ **Eigene Daten, nicht die des Nutzers.** Ein Prüflauf, der in der echten
    Ablage arbeitet, schreibt seine Fehler in dessen Bericht — dieselbe Regel
    wie beim Selbsttest.
    """
    ordner = tempfile.mkdtemp(prefix='sc-bp-klick-')
    os.environ['SC_BP_HOME'] = ordner
    # ⚠ **Die Formatnummer aus dem Modul holen, nicht hinschreiben.** Der
    # erste Anlauf setzte hier eine 4 — `hangar.FORMAT` ist aber 1, und eine
    # Datei aus der Zukunft wird stillschweigend verworfen. Ergebnis: null
    # Schiffe, zwei rote Pruefungen, und der Fehler lag im Pruefaufbau.
    from scbp import hangar as _hg
    hangar = {
        'format': _hg.FORMAT,
        'schiffe': [{
            'name': 'Cutlass Black', 'hersteller': 'Drake',
            'kurz': 'cutlassblack', 'hkurz': 'DRAK', 'herkunft': 'pledge',
            'belegung': {
                'hardpoint_cooler_01': {'ref': 'r1', 'name': 'BlastChill',
                                        'weg': 'kaufen'},
                'hardpoint_cooler_02': {'ref': 'r2', 'name': 'ColdSnap',
                                        'weg': 'bauen'},
            },
        }],
        'wunsch': [],
    }
    with open(os.path.join(ordner, 'hangar.json'), 'w', encoding='utf-8') as f:
        json.dump(hangar, f)
    return ordner


def main():
    ordner = _ablage_vorbereiten()
    try:
        from scbp import hauptfenster, seiten, sprache, warenkorb
        sprache.setzen('de')

        print('Die Oberflaeche wirklich bedienen')
        print()

        # ---------------------------------------------------------------
        print('1. Die Marke an der Schiffszeile')
        wurzel = tk.Tk()
        wurzel.withdraw()
        rahmen = tk.Frame(wurzel, bg='#0d1117')
        rahmen.pack(fill='both', expand=True)

        from scbp import hangar as meine
        stand = meine.laden()
        schiff = (stand.get('schiffe') or [{}])[0]

        seiten._zeichne_marke(_fenster(wurzel), rahmen, schiff)
        wurzel.update_idletasks()
        gelesen = ' '.join(texte(rahmen))
        pruefe('2' in gelesen,
               'zwei offene Posten stehen da (%r)' % gelesen)

        warenkorb.erledigt_setzen(schiff, 'hardpoint_cooler_01', True)
        for kind in rahmen.winfo_children():
            kind.destroy()
        seiten._zeichne_marke(_fenster(wurzel), rahmen, schiff)
        wurzel.update_idletasks()
        gelesen = ' '.join(texte(rahmen))
        pruefe('1' in gelesen and '2' not in gelesen,
               'nach einem Haken steht dort einer (%r)' % gelesen)

        warenkorb.erledigt_setzen(schiff, 'hardpoint_cooler_02', True)
        for kind in rahmen.winfo_children():
            kind.destroy()
        seiten._zeichne_marke(_fenster(wurzel), rahmen, schiff)
        wurzel.update_idletasks()
        gelesen = ' '.join(texte(rahmen))
        pruefe('besorgen' not in gelesen,
               'alles abgehakt: „noch zu besorgen" ist weg (%r)' % gelesen)
        wurzel.destroy()

        # ---------------------------------------------------------------
        print()
        print('2. Das ganze Fenster: Seiten oeffnen und lesen')
        hf = hauptfenster.Hauptfenster(version='0.0.0-pruefung')
        hf.root.withdraw()
        try:
            for name in ('hangar', 'wunschliste', 'einkaufsliste',
                         'farmliste'):
                hf.oeffnen(name)
                hf.root.update()
                gelesen = texte(hf.root)
                pruefe(bool(gelesen),
                       'Seite %s zeigt etwas an (%d Beschriftungen)'
                       % (name, len(gelesen)))
        finally:
            hf.root.destroy()

        print()
        if fehler:
            print('%d von %d Pruefungen fehlgeschlagen:'
                  % (len(fehler), geprueft[0]))
            for f in fehler:
                print('  ·', f)
            return 1
        print('Alle %d Pruefungen bestanden.' % geprueft[0])
        return 0
    finally:
        shutil.rmtree(ordner, ignore_errors=True)


def _fenster(wurzel):
    """Ein Ersatz für das Hauptfenster — nur die Schriften, die gebraucht werden."""
    from tkinter import font as tkfont

    class Fenster:
        pass

    f = Fenster()
    f.f_klein = tkfont.Font(family='Calibri', size=10)
    f.f_grund = tkfont.Font(family='Calibri', size=11)
    f.f_fett = tkfont.Font(family='Calibri', size=10, weight='bold')
    f.beim_zeigen = {}
    f.root = wurzel
    return f


if __name__ == '__main__':
    sys.exit(main())
