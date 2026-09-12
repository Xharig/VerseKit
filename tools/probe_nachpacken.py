# -*- coding: utf-8 -*-
"""Kommen die weiteren Zeilen beim Rollen wirklich nach?

Die Beschleunigung der Joystick-Seite beruht darauf, dass zunaechst nur die
sichtbaren Zeilen gepackt werden. Das ist nur dann richtig, wenn beim Rollen
die uebrigen **zuverlaessig** dazukommen — sonst fehlen dem Nutzer stumm
Belegungen, und das waere schlimmer als eine langsame Seite.

    python3 tools/probe_nachpacken.py

⚠ Der Selbsttest kann das nicht: Er misst keine Rollvorgaenge an einem
Fenster mit echter Hoehe.
"""
import os
import sys

WURZEL = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, WURZEL)
sys.path.insert(0, os.path.join(WURZEL, 'tools'))

import unsichtbar                                            # noqa: E402
unsichtbar.sicherstellen(messend=True)

from scbp import seiten, hauptfenster                        # noqa: E402

OK = []


def p(bedingung, text):
    OK.append(bool(bedingung))
    print('  [%s] %s' % ('ok' if bedingung else 'XX', text))


def liste_finden(rahmen):
    bester, zahl = None, 0
    def gehe(w):
        nonlocal bester, zahl
        kinder = [k for k in w.winfo_children() if k.winfo_class() == 'Frame']
        if len(kinder) > zahl:
            bester, zahl = w, len(kinder)
        for k in w.winfo_children():
            gehe(k)
    gehe(rahmen)
    return bester


def gepackt(rahmen):
    return sum(1 for k in rahmen.winfo_children()
               if k.winfo_manager() == 'pack')


def main():
    fenster = hauptfenster.Hauptfenster(version='mess')
    fenster.root.update()
    fenster.oeffnen('joysticks')
    for _ in range(3):
        fenster.root.update()
        fenster.root.update_idletasks()

    liste = liste_finden(fenster.seiten['joysticks'])
    gebaut = len(liste.winfo_children()) if liste else 0
    p(gebaut > seiten.ZEILEN_SOFORT,
      'die Liste hat mehr Zeilen als sofort gezeigt werden (%d)' % gebaut)
    if gebaut <= seiten.ZEILEN_SOFORT:
        fenster.root.destroy()
        return 1

    anfang = gepackt(liste)
    # ⚠ **Nicht genau `ZEILEN_SOFORT`.** Füllt die erste Portion das Fenster
    # nicht aus, meldet Tk sofort „unten angekommen" — und dann ist Nachlegen
    # richtig, sonst stünde der Nutzer vor einer Liste, die ohne Rollbalken
    # endet, obwohl es weitergeht.
    #
    # Die erste Fassung dieser Probe erwartete hier hart 45 und schlug fehl;
    # die Erwartung war falsch, nicht der Code. Gemessen an einem hohen
    # Fenster: 90 von 200.
    p(anfang < gebaut * 0.75,
      'zu Beginn steht nur ein Teil der Liste (%d von %d)' % (anfang, gebaut))

    # ⭐ Rollen wie ein Nutzer: ans Ende der Rollflaeche.
    leinwand = None
    w = liste
    while w is not None and leinwand is None:
        leinwand = getattr(w, 'leinwand', None)
        w = w.master
    p(leinwand is not None, 'die Rollflaeche ist erreichbar')
    if leinwand is None:
        fenster.root.destroy()
        return 1

    for runde in range(6):
        leinwand.yview_moveto(1.0)
        fenster.root.update()
        fenster.root.update_idletasks()
    ende = gepackt(liste)
    p(ende > anfang,
      'nach dem Rollen stehen mehr Zeilen da (%d -> %d)' % (anfang, ende))

    # Immer weiter rollen, bis nichts mehr dazukommt.
    letzte = ende
    for _ in range(12):
        leinwand.yview_moveto(1.0)
        fenster.root.update()
        fenster.root.update_idletasks()
        jetzt = gepackt(liste)
        if jetzt == letzte:
            break
        letzte = jetzt
    p(letzte == gebaut,
      'am Ende sind ALLE Zeilen da — keine geht verloren (%d von %d)'
      % (letzte, gebaut))

    # ⚠ Und die Reihenfolge muss stimmen: `pack()` haengt nur hinten an,
    # aber das muss belegt sein — sonst waere die Liste nach dem Rollen
    # durcheinander.
    reihen = liste.pack_slaves()
    p(reihen == [k for k in liste.winfo_children()
                 if k.winfo_manager() == 'pack'],
      'die Reihenfolge entspricht der Baureihenfolge')

    # Der Rollbalken muss weiter bedient werden.
    p(leinwand.cget('yscrollcommand') != '',
      'der Rollbalken haengt weiterhin dran')

    fenster.root.destroy()
    print('\n  %d von %d' % (sum(1 for b in OK if b), len(OK)))
    return 0 if all(OK) else 1


if __name__ == '__main__':
    sys.exit(main())
