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
MIN_WIDTH = 120


class CalibrationWindow:
    """Das Vollbildfenster zum Ausmessen.

    `beim_fertig` bekommt die gemessene Kartenbreite in Pixeln und die
    Pixelbreite des Bildschirms — daraus rechnet der Aufrufer weiter.
    """

    def __init__(self, parent, on_done, font=None, small=None,
                 start_width=None):
        self.on_done = on_done
        self.font = font or ('DejaVu Sans', 11)
        self.small = small or ('DejaVu Sans', 9)

        self.window = tk.Toplevel(parent)
        self.window.title(t('s_fv_titel'))
        self.window.configure(bg=BG)
        # ⚠ Erst anzeigen, dann Vollbild — sonst misst sich das Fenster unter
        # manchen Fensterverwaltungen noch in seiner Ausgangsgröße.
        self.window.update_idletasks()
        self.fullscreen = False
        try:
            self.window.attributes('-fullscreen', True)
            self.fullscreen = True
        except tk.TclError:
            # Nicht jede Umgebung kann das. Dann ein großes Fenster — die
            # Messung der Karte stimmt trotzdem, nur die Bildschirmbreite
            # lässt sich daraus nicht ablesen.
            # ⚠⚠ **Mit Position, nicht nur mit Größe.** Ein `geometry` ohne
            # `+x+y` überlässt die Platzierung dem Fenstermanager — und der
            # weiß nichts vom Hauptfenster. Auf mehreren Bildschirmen landete
            # so am 06.09.2026 ein Fenster außerhalb des sichtbaren Bereichs;
            # weil es modal war, ließ sich das Programm nicht einmal beenden.
            #
            # `mittig_ueber` setzt beides und fällt auf die reine Größe
            # zurück, wenn es kein Elternfenster gibt (eigenständiger Start).
            from .hauptfenster import mittig_ueber
            if parent is None or not mittig_ueber(self.window, parent, 1200, 800):
                self.window.geometry('1200x800')
        self.window.bind('<Escape>', lambda _e: self.close())

        self.canvas = tk.Canvas(self.window, bg=BG, highlightthickness=0,
                                  bd=0, cursor='sb_h_double_arrow')
        self.canvas.pack(fill='both', expand=True)

        self.width = tk.DoubleVar(value=float(start_width or 320))
        self.canvas.bind('<Configure>', lambda _e: self._draw())
        self.canvas.bind('<B1-Motion>', self._drag)
        self.window.bind('<Left>', lambda _e: self._stufe(-1))
        self.window.bind('<Right>', lambda _e: self._stufe(1))
        self.window.bind('<Shift-Left>', lambda _e: self._stufe(-10))
        self.window.bind('<Shift-Right>', lambda _e: self._stufe(10))
        self.window.focus_set()
        self._build()

    # ------------------------------------------------------------------

    def _build(self):
        bar = tk.Frame(self.window, bg=FLAECHE)
        bar.place(relx=0.5, rely=0.94, anchor='center')

        self.slider = tk.Scale(
            bar, from_=MIN_WIDTH, to=900, resolution=1,
            orient='horizontal', variable=self.width,
            command=lambda _w: self._draw(), showvalue=False,
            bg=FLAECHE, fg=FG, troughcolor=BG, activebackground=ACCENT,
            highlightthickness=0, bd=0, sliderrelief='flat', length=420)
        self.slider.pack(side='left', padx=14, pady=10)

        self.value = tk.Label(bar, text='', bg=FLAECHE, fg=ACCENT,
                             font=self.small, width=22, anchor='w')
        self.value.pack(side='left', padx=(0, 14))

        done = tk.Label(bar, text=t('s_fv_passt'), bg=ACCENT, fg=BG,
                          font=self.font, padx=18, pady=6, cursor='hand2')
        done.pack(side='left', padx=(0, 8), pady=10)
        done.bind('<Button-1>', lambda _e: self._finish())

        ab = tk.Label(bar, text=t('s_fv_abbrechen'), bg=FLAECHE, fg=SUB,
                      font=self.small, padx=14, pady=6, cursor='hand2')
        ab.pack(side='left', padx=(0, 14), pady=10)
        ab.bind('<Button-1>', lambda _e: self.close())

    def _stufe(self, um):
        self.width.set(max(MIN_WIDTH, self.width.get() + um))
        self._draw()

    def _drag(self, event):
        """Am Rand ziehen: Die Breite folgt dem Abstand zur Mitte."""
        centre = self.canvas.winfo_width() / 2.0
        fresh = abs(event.x - centre) * 2.0
        self.width.set(max(MIN_WIDTH, fresh))
        self._draw()

    def _draw(self):
        self.canvas.delete('all')
        width_px = self.canvas.winfo_width()
        height_px = self.canvas.winfo_height()
        if width_px <= 1:
            return

        card_width = float(self.width.get())
        # Das Seitenverhältnis der Karte ist genormt — die Höhe folgt daraus
        # und wird NICHT getrennt eingestellt. Zwei Regler wären zwei
        # Fehlerquellen für dieselbe Messung.
        card_height = card_width * (fov.KARTE_HOEHE_MM / fov.KARTE_BREITE_MM)

        mx, my = width_px / 2.0, height_px / 2.0 - 30
        x1, y1 = mx - card_width / 2.0, my - card_height / 2.0
        x2, y2 = mx + card_width / 2.0, my + card_height / 2.0

        # Die Karte selbst — heller Umriss auf dunklem Grund, damit die echte
        # Karte danebengehalten gut abzugleichen ist.
        self.canvas.create_rectangle(x1, y1, x2, y2, outline=ACCENT,
                                       width=2, fill=FLAECHE)
        # Hilfslinien in der Mitte: An einer Kante lässt sich genauer
        # angleichen als an einer Fläche.
        self.canvas.create_line(mx, y1, mx, y2, fill=LINIE)
        self.canvas.create_line(x1, my, x2, my, fill=LINIE)

        self.canvas.create_text(
            mx, y1 - 60, text=t('s_fv_anleitung'), fill=FG,
            font=self.font, anchor='center', justify='center',
            width=min(900, width_px - 80))
        self.canvas.create_text(
            mx, y2 + 40, text=t('s_fv_masse'), fill=SUB, font=self.small,
            anchor='center')

        mm_per_pixel = fov.mm_pro_pixel(card_width)
        if mm_per_pixel:
            total = fov.bildschirmbreite_mm(width_px, mm_per_pixel)
            self.value.configure(
                text=t('s_fv_stand').format(int(card_width),
                                            (total or 0) / 10.0))

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
            is_set = bool(self.window.attributes('-fullscreen'))
        except tk.TclError:
            is_set = False
        width = self.canvas.winfo_width()
        # Zweite Sicherung: Ein „Vollbild", das schmaler ist als die Hälfte
        # dessen, was Tk als Bildschirm meldet, ist keines.
        try:
            enough = width >= self.window.winfo_screenwidth() * 0.5
        except tk.TclError:
            enough = False
        return is_set and enough

    def _finish(self):
        width_px = self.canvas.winfo_width()
        card = float(self.width.get())
        fullscreen = self._wirklich_vollbild()
        self.close()
        if self.on_done:
            self.on_done(card, width_px, fullscreen)

    def close(self):
        try:
            self.window.destroy()
        except tk.TclError:
            pass


def calibrate(parent, on_done, font=None, small=None,
                start_width=None):
    """Das Kalibrierfenster öffnen. Bequemer Einstieg für die Oberfläche."""
    return CalibrationWindow(parent, on_done, font=font,
                            small=small, start_width=start_width)
