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
Zeigt, was Maus und Trackpad wirklich melden.

Wozu: Rollen verhält sich auf jedem System anders, und beim Trackpad noch
einmal anders als beim Rad. Windows meldet Rasten zu je 120, Linux schickt
Tastennummern (4 nach oben, 5 nach unten), macOS meldet kleine Beträge — wie
klein, hängt vom Gerät ab. Wer das rät, baut einen Griff, der auf dem eigenen
Rechner geht und auf dem nächsten nicht.

    python3 tools/rad_messen.py

Fenster öffnet sich, dann:

  1. einmal mit der **Maus** rollen (falls vorhanden),
  2. einmal mit dem **Trackpad** streichen — sanft und kräftig,
  3. Fenster schließen.

Zum Schluss steht in der Zusammenfassung, welche Beträge angekommen sind.
Genau die braucht `main_window.rad_anschliessen()`.
"""
import os
import sys

# Siehe unsichtbar.py: nie ein Fenster ueber einem laufenden Spiel.
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import unsichtbar                                              # noqa: E402
unsichtbar.sicherstellen(messend=True)

import tkinter as tk                                           # noqa: E402

# ⛔ Vor der ersten Ausgabe: Die Windows-Konsole kann kein `⚠` — siehe ausgabe.py.
import ausgabe                                                 # noqa: E402
ausgabe.utf8()


def main():
    ereignisse = []

    wurzel = tk.Tk()
    wurzel.title('Rad messen — bitte rollen und streichen')
    wurzel.geometry('560x360')
    wurzel.configure(bg='#10141c')

    kopf = tk.Label(
        wurzel,
        text='Roll mit der Maus und streich mit dem Trackpad.\n'
             'Jedes Ereignis erscheint unten. Danach Fenster schließen.',
        bg='#10141c', fg='#e6edf3', font=('Helvetica', 13), justify='left')
    kopf.pack(padx=16, pady=(16, 8), anchor='w')

    protokoll = tk.Text(wurzel, bg='#0c1017', fg='#9ce430', bd=0,
                        highlightthickness=0, font=('Menlo', 11))
    protokoll.pack(fill='both', expand=True, padx=16, pady=(0, 16))

    def notieren(e, quelle):
        delta = getattr(e, 'delta', None)
        nummer = getattr(e, 'num', None)
        ereignisse.append((quelle, delta, nummer))
        protokoll.insert('end', '%-12s delta=%-8s num=%s\n'
                         % (quelle, delta, nummer))
        protokoll.see('end')

    def streichen(e):
        """Trackpad: dx und dy stecken gepackt in einer Zahl."""
        roh = int(getattr(e, 'delta', 0) or 0)
        waagerecht = roh & 0xFFFF
        senkrecht = (roh >> 16) & 0xFFFF
        if waagerecht >= 0x8000:
            waagerecht -= 0x10000
        if senkrecht >= 0x8000:
            senkrecht -= 0x10000
        ereignisse.append(('TouchpadScroll', senkrecht, None))
        protokoll.insert('end', 'Touchpad     roh=%-10s hoch/runter=%-5s '
                         'seitlich=%s\n' % (roh, senkrecht, waagerecht))
        protokoll.see('end')

    wurzel.bind_all('<MouseWheel>', lambda e: notieren(e, 'MouseWheel'))
    wurzel.bind_all('<Button-4>', lambda e: notieren(e, 'Button-4'))
    wurzel.bind_all('<Button-5>', lambda e: notieren(e, 'Button-5'))
    # macOS kennt zusätzlich waagerechtes Rollen — hier nur, um zu sehen,
    # ob es überhaupt auftaucht.
    wurzel.bind_all('<Shift-MouseWheel>', lambda e: notieren(e, 'Shift-Wheel'))
    # ⚠ Der eigentliche Grund für dieses Werkzeug: Vom Trackpad kam kein
    # einziges <MouseWheel> an. Seit Tk 8.7 gibt es dafür ein eigenes
    # Ereignis; ältere Versionen kennen es nicht und werfen beim Binden.
    try:
        wurzel.bind_all('<TouchpadScroll>', streichen, add='+')
        kopf.configure(text=kopf.cget('text') + '\n(Tk %s — Trackpad-Ereignis '
                       'ist angeschlossen.)' % tk.TkVersion)
    except tk.TclError:
        kopf.configure(text=kopf.cget('text') + '\n(Tk %s kennt '
                       '<TouchpadScroll> nicht.)' % tk.TkVersion)

    wurzel.mainloop()

    if not ereignisse:
        print('Nichts angekommen. Entweder wurde nicht gerollt, oder Tk '
              'bekommt die Ereignisse auf diesem Gerät gar nicht.')
        return 1

    betraege = [d for _, d, _ in ereignisse if isinstance(d, int) and d]
    print('\n%d Ereignisse.' % len(ereignisse))
    if betraege:
        print('Beträge: kleinster %s, größter %s' % (min(betraege, key=abs),
                                                     max(betraege, key=abs)))
        print('Alle vorkommenden Beträge: %s'
              % sorted({abs(b) for b in betraege}))
    nummern = sorted({n for _, _, n in ereignisse if n not in (None, '??')})
    if nummern:
        print('Tastennummern: %s' % nummern)
    print('\nGebraucht wird: Bei welchem Betrag soll eine Zeile gerollt '
          'werden?\nSteht dort etwas kleiner als 1, muss aufaddiert werden.')
    return 0


if __name__ == '__main__':
    sys.exit(main())
