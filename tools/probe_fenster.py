# -*- coding: utf-8 -*-
"""Lassen sich die eigenstaendigen Fenster wirklich oeffnen?

⛔⛔ **Warum es das braucht:** Der Selbsttest deckt diese Fenster nicht ab —
sie leben in GUI-Rueckrufen, die er nie ausloest. Bei der Umbenennung am
13.09.2026 war er **2208/2208 gruen**, waehrend zwei der vier Fenster beim
Oeffnen mit `TypeError` starben:

    rundleiste() got an unexpected keyword argument 'bg_colour'
    verlauf() got an unexpected keyword argument 'whole'

Die Ursache ist lehrreich: Umbenannt wurden Schluesselwoerter, die **fremden**
Funktionen gehoeren. `code_umbenennen()` hat eine Sperre fuer fremde
Attribute — ein `rundleiste(grund=BG)` ist aber ein ganz normales NAME-Token
im eigenen Modul.

⚠ Gegen die **eingelesene Signatur** pruefen faengt es nur halb: `pack(side=)`
meldet dann Fehlalarme, weil es im Projekt eine gleichnamige Funktion gibt.
Der Rauchtest ist die verlaessliche Probe — er ruft die Dinge einfach auf.

    python3 tools/probe_fenster.py

⚠ Er oeffnet echte Fenster. `unsichtbar.sicherstellen()` haelt sie vom
Bildschirm fern; ohne das blitzen sie auf und reissen den Tastaturfokus mit.
"""
import os
import sys

WURZEL = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, WURZEL)
sys.path.insert(0, os.path.join(WURZEL, 'tools'))

import unsichtbar                                            # noqa: E402
unsichtbar.sicherstellen(messend=True)

import tkinter as tk                                         # noqa: E402
from scbp import (version_window, binding_window,            # noqa: E402
                  curve_plot, fov_window)

OK = []


def p(bedingung, text):
    OK.append(bool(bedingung))
    print('  [%s] %s' % ('ok' if bedingung else 'XX', text))


def main():
    wurzel = tk.Tk()
    wurzel.geometry('900x700+20+20')
    wurzel.update()

    # ── Kurvenbild: eingebettet, zeichnet auf eine Leinwand ──────────────
    try:
        rahmen = tk.Frame(wurzel)
        rahmen.pack(fill='both', expand=True)
        bild = curve_plot.CurvePlot(rahmen, 260, 160)
        wurzel.update()
        bild._draw()
        wurzel.update()
        p(True, 'CurvePlot baut und zeichnet')
    except Exception as fehler:
        p(False, 'CurvePlot: %s: %s' % (type(fehler).__name__, fehler))

    # ── und seine grosse Ansicht (eigenes Fenster) ───────────────────────
    try:
        curve_plot.show_large(wurzel, 'Probe', 0.1, 0.5, 1.0, [], False,
                              ('Segoe UI', 10), ('Segoe UI', 9))
        wurzel.update()
        p(True, 'show_large oeffnet')
        for kind in wurzel.winfo_children():
            if isinstance(kind, tk.Toplevel):
                kind.destroy()
    except Exception as fehler:
        p(False, 'show_large: %s: %s' % (type(fehler).__name__, fehler))

    # ── Versionsfenster ─────────────────────────────────────────────────
    try:
        fenster = version_window.VersionWindow(wurzel, '0.0.0', [])
        wurzel.update()
        p(True, 'VersionWindow oeffnet')
        fenster.close()
        wurzel.update()
    except Exception as fehler:
        p(False, 'VersionWindow: %s: %s' % (type(fehler).__name__, fehler))

    # ── Die beiden uebrigen wenigstens laden und ihre Namen pruefen ──────
    #
    # ⚠ `BindingWindow` wartet auf einen echten Tastendruck und
    # `CalibrationWindow` auf eine Ziehbewegung — beide lassen sich hier
    # nicht sinnvoll durchspielen. Geprueft wird, dass es sie gibt: Ein
    # falscher Klassenname faellt so trotzdem auf.
    p(hasattr(binding_window, 'BindingWindow'),
      'BindingWindow ist da')
    p(hasattr(fov_window, 'CalibrationWindow')
      and hasattr(fov_window, 'calibrate'),
      'CalibrationWindow und calibrate() sind da')

    wurzel.destroy()
    print('\n  %d von %d' % (sum(1 for b in OK if b), len(OK)))
    return 0 if all(OK) else 1


if __name__ == '__main__':
    sys.exit(main())
