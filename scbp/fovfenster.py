# SPDX-License-Identifier: GPL-3.0-only
#
# SC BP Watcher — Bildschirm mit einer Karte ausmessen
# Copyright (C) 2026 Xharig
#
# This program is free software: you can redistribute it and/or modify it under
# the terms of the GNU General Public License version 3 as published by the
# Free Software Foundation.
#
# This program is distributed in the hope that it will be useful, but WITHOUT
# ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
# FOR A PARTICULAR PURPOSE.  See the GNU General Public License for more
# details.
#
# You should have received a copy of the GNU General Public License along with
# this program.  If not, see <https://www.gnu.org/licenses/>.
"""
Halt eine Bankkarte an den Bildschirm — der Rest ergibt sich.

## Warum ein Vollbildfenster

Zwei Gründe, und beide sind zwingend:

1. **Die Pixelbreite des richtigen Bildschirms.** Bei mehreren Bildschirmen
   liefert Tk die Maße des **gesamten** Desktops (gemessen: 6201 × 2881 über
   drei Geräte) — unbrauchbar. Ein Fenster im Vollbildmodus misst sich dagegen
   selbst und weiß damit genau, wie breit **dieser** Bildschirm ist. Keine
   Geräteabfrage, kein systemabhängiger Sonderweg.
2. **Platz für die Karte.** Das Rechteck muss auf jedem Bildschirm in
   Originalgröße darstellbar sein, ohne dass ein Fensterrahmen dazwischenkommt.

## Der Ablauf

1. Fenster geht im Vollbildmodus auf dem Bildschirm auf, auf dem gespielt wird.
2. In der Mitte liegt ein Rechteck in Kartenform.
3. Der Spieler legt seine Karte an und zieht das Rechteck auf ihre Größe —
   mit dem Regler, den Pfeiltasten oder am Rand mit der Maus.
4. Ein Klick auf „Passt" rechnet daraus, wie groß ein Pixel wirklich ist.

Jede Bankkarte, jeder Führerschein und jeder Personalausweis hat exakt
dieselbe Größe: **85,60 × 53,98 mm** (ISO/IEC 7810, ID-1).

## ⚠ Öffnet sich nur auf Knopfdruck

Ein Vollbildfenster, das von allein aufgeht, reißt den Tastaturfokus mit —
wer gerade fliegt, landet im Desktop. Deshalb: nie automatisch, nie beim
Programmstart, und **Escape schließt immer**.
"""
import tkinter as tk

from . import fov
from .sprache import t

BG = '#10141c'
FLAECHE = '#161c28'
FG = '#e6edf3'
SUB = '#8b98a5'
ACCENT = '#9ce430'
LINIE = '#232c3d'

# Die Karte wird nie kleiner als das gezeichnet — darunter lässt sich nichts
# mehr sinnvoll anlegen, und der Messfehler wüchse ins Unbrauchbare.
KLEINSTE_BREITE = 120


class Kalibrierfenster:
    """Das Vollbildfenster zum Ausmessen.

    `beim_fertig` bekommt die gemessene Kartenbreite in Pixeln und die
    Pixelbreite des Bildschirms — daraus rechnet der Aufrufer weiter.
    """

    def __init__(self, eltern, beim_fertig, schrift=None, klein=None,
                 startbreite=None):
        self.beim_fertig = beim_fertig
        self.schrift = schrift or ('DejaVu Sans', 11)
        self.klein = klein or ('DejaVu Sans', 9)

        self.fenster = tk.Toplevel(eltern)
        self.fenster.title(t('s_fv_titel'))
        self.fenster.configure(bg=BG)
        # ⚠ Erst anzeigen, dann Vollbild — sonst misst sich das Fenster unter
        # manchen Fensterverwaltungen noch in seiner Ausgangsgröße.
        self.fenster.update_idletasks()
        self.vollbild = False
        try:
            self.fenster.attributes('-fullscreen', True)
            self.vollbild = True
        except tk.TclError:
            # Nicht jede Umgebung kann das. Dann ein großes Fenster — die
            # Messung der Karte stimmt trotzdem, nur die Bildschirmbreite
            # lässt sich daraus nicht ablesen.
            self.fenster.geometry('1200x800')
        self.fenster.bind('<Escape>', lambda _e: self.schliessen())

        self.leinwand = tk.Canvas(self.fenster, bg=BG, highlightthickness=0,
                                  bd=0, cursor='sb_h_double_arrow')
        self.leinwand.pack(fill='both', expand=True)

        self.breite = tk.DoubleVar(value=float(startbreite or 320))
        self.leinwand.bind('<Configure>', lambda _e: self._zeichnen())
        self.leinwand.bind('<B1-Motion>', self._ziehen)
        self.fenster.bind('<Left>', lambda _e: self._stufe(-1))
        self.fenster.bind('<Right>', lambda _e: self._stufe(1))
        self.fenster.bind('<Shift-Left>', lambda _e: self._stufe(-10))
        self.fenster.bind('<Shift-Right>', lambda _e: self._stufe(10))
        self.fenster.focus_set()
        self._bauen()

    # ------------------------------------------------------------------

    def _bauen(self):
        leiste = tk.Frame(self.fenster, bg=FLAECHE)
        leiste.place(relx=0.5, rely=0.94, anchor='center')

        self.regler = tk.Scale(
            leiste, from_=KLEINSTE_BREITE, to=900, resolution=1,
            orient='horizontal', variable=self.breite,
            command=lambda _w: self._zeichnen(), showvalue=False,
            bg=FLAECHE, fg=FG, troughcolor=BG, activebackground=ACCENT,
            highlightthickness=0, bd=0, sliderrelief='flat', length=420)
        self.regler.pack(side='left', padx=14, pady=10)

        self.wert = tk.Label(leiste, text='', bg=FLAECHE, fg=ACCENT,
                             font=self.klein, width=22, anchor='w')
        self.wert.pack(side='left', padx=(0, 14))

        fertig = tk.Label(leiste, text=t('s_fv_passt'), bg=ACCENT, fg=BG,
                          font=self.schrift, padx=18, pady=6, cursor='hand2')
        fertig.pack(side='left', padx=(0, 8), pady=10)
        fertig.bind('<Button-1>', lambda _e: self._fertig())

        ab = tk.Label(leiste, text=t('s_fv_abbrechen'), bg=FLAECHE, fg=SUB,
                      font=self.klein, padx=14, pady=6, cursor='hand2')
        ab.pack(side='left', padx=(0, 14), pady=10)
        ab.bind('<Button-1>', lambda _e: self.schliessen())

    def _stufe(self, um):
        self.breite.set(max(KLEINSTE_BREITE, self.breite.get() + um))
        self._zeichnen()

    def _ziehen(self, ereignis):
        """Am Rand ziehen: Die Breite folgt dem Abstand zur Mitte."""
        mitte = self.leinwand.winfo_width() / 2.0
        neu = abs(ereignis.x - mitte) * 2.0
        self.breite.set(max(KLEINSTE_BREITE, neu))
        self._zeichnen()

    def _zeichnen(self):
        self.leinwand.delete('all')
        breite_px = self.leinwand.winfo_width()
        hoehe_px = self.leinwand.winfo_height()
        if breite_px <= 1:
            return

        karte_breite = float(self.breite.get())
        # Das Seitenverhältnis der Karte ist genormt — die Höhe folgt daraus
        # und wird NICHT getrennt eingestellt. Zwei Regler wären zwei
        # Fehlerquellen für dieselbe Messung.
        karte_hoehe = karte_breite * (fov.KARTE_HOEHE_MM / fov.KARTE_BREITE_MM)

        mx, my = breite_px / 2.0, hoehe_px / 2.0 - 30
        x1, y1 = mx - karte_breite / 2.0, my - karte_hoehe / 2.0
        x2, y2 = mx + karte_breite / 2.0, my + karte_hoehe / 2.0

        # Die Karte selbst — heller Umriss auf dunklem Grund, damit die echte
        # Karte danebengehalten gut abzugleichen ist.
        self.leinwand.create_rectangle(x1, y1, x2, y2, outline=ACCENT,
                                       width=2, fill=FLAECHE)
        # Hilfslinien in der Mitte: An einer Kante lässt sich genauer
        # angleichen als an einer Fläche.
        self.leinwand.create_line(mx, y1, mx, y2, fill=LINIE)
        self.leinwand.create_line(x1, my, x2, my, fill=LINIE)

        self.leinwand.create_text(
            mx, y1 - 60, text=t('s_fv_anleitung'), fill=FG,
            font=self.schrift, anchor='center', justify='center',
            width=min(900, breite_px - 80))
        self.leinwand.create_text(
            mx, y2 + 40, text=t('s_fv_masse'), fill=SUB, font=self.klein,
            anchor='center')

        mm_je_pixel = fov.mm_pro_pixel(karte_breite)
        if mm_je_pixel:
            gesamt = fov.bildschirmbreite_mm(breite_px, mm_je_pixel)
            self.wert.configure(
                text=t('s_fv_stand').format(int(karte_breite),
                                            (gesamt or 0) / 10.0))

    def _wirklich_vollbild(self):
        """Steht das Fenster wirklich über den ganzen Bildschirm?

        ⚠⚠ **Das muss geprüft werden, nicht angenommen.** Der Vollbildmodus
        kann fehlschlagen, ohne einen Fehler zu werfen — unter einer nackten
        X-Sitzung ohne Fensterverwaltung blieb das Fenster bei **394 × 276**
        stehen, während `attributes('-fullscreen', True)` klaglos durchlief.

        Die Kartenmessung stimmt dann trotzdem (die Karte liegt ja auf dem
        Bildschirm), aber die **Bildschirmbreite** wäre die Fensterbreite —
        und damit wäre die ganze Rechnung falsch, ohne dass es jemand merkt.
        Lieber nichts speichern als einen falschen Wert.
        """
        try:
            gesetzt = bool(self.fenster.attributes('-fullscreen'))
        except tk.TclError:
            gesetzt = False
        breite = self.leinwand.winfo_width()
        # Zweite Sicherung: Ein „Vollbild", das schmaler ist als die Hälfte
        # dessen, was Tk als Bildschirm meldet, ist keines.
        try:
            genug = breite >= self.fenster.winfo_screenwidth() * 0.5
        except tk.TclError:
            genug = False
        return gesetzt and genug

    def _fertig(self):
        breite_px = self.leinwand.winfo_width()
        karte = float(self.breite.get())
        vollbild = self._wirklich_vollbild()
        self.schliessen()
        if self.beim_fertig:
            self.beim_fertig(karte, breite_px, vollbild)

    def schliessen(self):
        try:
            self.fenster.destroy()
        except tk.TclError:
            pass


def kalibrieren(eltern, beim_fertig, schrift=None, klein=None,
                startbreite=None):
    """Das Kalibrierfenster öffnen. Bequemer Einstieg für die Oberfläche."""
    return Kalibrierfenster(eltern, beim_fertig, schrift=schrift,
                            klein=klein, startbreite=startbreite)
