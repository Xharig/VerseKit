# SPDX-License-Identifier: GPL-3.0-only
#
# SC BP Watcher — die Antwortkurve einer Achse zeichnen
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
Drei Zahlen als Bild — was Totzone, Sättigung und Exponent zusammen anrichten.

## Warum überhaupt zeichnen

„Totzone 0,099, Sättigung 0,7425, Exponent 1,5" sagt niemandem etwas. Erst die
Linie zeigt, dass die ersten zehn Prozent des Wegs nichts tun, dass oben ein
Viertel des Wegs verschenkt ist und wie steil es dazwischen zugeht. Genau
deshalb zeichnet Star Citizen dieselbe Kurve in seinem Einstellungsbildschirm.

⚠ **Das ist kein Symbol.** Die Projektregel „es wird nichts selbst gemalt"
gilt für **Symbole** — Häkchen, Kreuze, Zahnräder, die aus dem Lucide-Satz
kommen müssen. Ein Diagramm ist Inhalt, so wie ein Balken oder ein
Bildschirmfoto Inhalt ist; es gibt keine Vorlage, die die Kurve *dieser* Achse
zeigen könnte.

## Zwei Ansichten, mit Absicht

| Ansicht | Bereich | Wofür |
|---|---|---|
| **Vollansicht** | -1 bis 1 | die ganze Achse, beide Richtungen, Knick in der Mitte sichtbar |
| **Quadrant** | 0 bis 1 | groß und genau — die Ansicht, die auch das Spiel zeigt |

Die Kurve ist punktsymmetrisch: Was nach links passiert, ist das Spiegelbild
von rechts. Deshalb genügt der Quadrant, um alles Wesentliche zu zeigen — und
weil er nur ein Viertel der Fläche darstellen muss, wird jedes Detail
viermal so groß.

## ⚠ Die Leinwand kennt ihre Größe erst, wenn sie steht

Unter Wayland liefert Tk die endgültigen Maße erst, wenn das Fenster
tatsächlich angezeigt wird — dieselbe Falle, die im Projekt schon einmal
Knopfbeschriftungen abgeschnitten hat. Deshalb hängt sich dieses Bauteil an
`<Configure>` und zeichnet neu, sobald sich die Fläche ändert. Wer stattdessen
einmal beim Bauen zeichnet, bekommt eine Kurve, die in der Ecke klebt.
"""
import tkinter as tk

from . import curves
from .sprache import t

BG      = '#10141c'
FLAECHE = '#161c28'
FG      = '#e6edf3'
SUB     = '#8b98a5'
ACCENT  = '#9ce430'
LINIE   = '#232c3d'
GOLD    = '#e8c353'
ROT     = '#e05252'

# Wieviele Stützstellen der Streckenzug bekommt. Tk kennt keine Kurven; zu
# wenige Punkte machen aus dem Knick an der Totzone eine sanfte Rundung — und
# genau der Knick ist die Aussage.
SUPPORT_POINTS = 160


def show_large(parent, title, totzone=None, saturation=None, exponent=None,
                 curve=None, whole=False, font=None, small=None):
    """Die Kurve groß in einem eigenen Fenster — zum genauen Hinsehen.

    Auf der Seite ist das Bild klein, weil daneben die Regler und die
    Achsenliste stehen. Wer eine Kurve wirklich beurteilen will, braucht
    Fläche: Ob die Mitte weich oder hart einsetzt, sieht man bei 240 Pixeln
    schlicht nicht.

    ⚠ **Öffnet sich nur auf Knopfdruck.** Ein Fenster, das von allein
    aufgeht, reißt den Tastaturfokus mit — wer gerade fliegt, landet im
    Desktop.

    ⚠ **Die Mindestgröße wird mitgesetzt.** Sonst gilt sie weiter, wenn
    jemand das Fenster kleiner zieht, und die gesetzte Größe stimmt nicht
    mehr mit der tatsächlichen überein — im Projekt schon einmal die Ursache
    dafür, dass ein Fenster aus dem Bildschirm wanderte.
    """
    window = tk.Toplevel(parent)
    window.title(title)
    window.configure(bg=BG)
    margin = 40
    side = 560
    window.geometry('%dx%d' % (side + margin, side + margin + 46))
    window.minsize(360, 400)

    head = tk.Frame(window, bg=BG)
    head.pack(fill='x', side='top')

    plot = CurvePlot(window, width=side, height=side, whole=whole,
                      font=font, small=small)
    plot.pack(fill='both', expand=True, padx=20, pady=(0, 20))
    plot.show(totzone=totzone, saturation=saturation, exponent=exponent,
                curve=curve)

    zustand = {'ganz': whole}

    def _toggle():
        zustand['ganz'] = plot.toggle()
        button.configure(text=(t('s_kv_quadrant') if zustand['ganz']
                              else t('s_kv_ganz')))

    button = tk.Label(head, text=(t('s_kv_quadrant') if whole
                                 else t('s_kv_ganz')),
                     bg=FLAECHE, fg=FG, font=small or font,
                     padx=12, pady=6, cursor='hand2')
    button.pack(side='left', padx=20, pady=14)
    button.bind('<Button-1>', lambda _e: _toggle())

    werte = tk.Label(
        head,
        text='%s %s   ·   %s %s' % (
            t('s_kv_totzone'), '—' if totzone is None else ('%g' % totzone),
            t('s_kv_saettigung'),
            '—' if saturation is None else ('%g' % saturation)),
        bg=BG, fg=SUB, font=small or font)
    werte.pack(side='left')
    return window


class CurvePlot:
    """Die Antwortkurve einer Achse auf einer Leinwand.

    Benutzung:

        bild = CurvePlot(rahmen, breite=260, hoehe=260)
        plot.show(totzone=0.1, saettigung=0.9, exponent=1.5)

    `ganz=True` schaltet auf die Vollansicht (-1 bis 1), Standard ist der
    Quadrant. Umschalten geht jederzeit über `umschalten()`.
    """

    def __init__(self, parent, width=260, height=260, whole=False,
                 font=None, small=None):
        self.whole = bool(whole)
        self.font = font
        self.small = small or font
        self.werte = {'totzone': 0.0, 'saettigung': 1.0, 'exponent': 1.0,
                      'kurve': None}
        self.zeiger = None          # aktueller Ausschlag, falls gemessen
        self.canvas = tk.Canvas(parent, width=width, height=height,
                                  bg=FLAECHE, highlightthickness=1,
                                  highlightbackground=LINIE, bd=0)
        # ⚠ Neu zeichnen, sobald die Fläche wirklich steht — nicht nur einmal
        # beim Bauen. Siehe Modulkopf.
        self.canvas.bind('<Configure>', self._neu)

    def pack(self, **kwargs):
        self.canvas.pack(**kwargs)
        return self

    def grid(self, **kwargs):
        self.canvas.grid(**kwargs)
        return self

    def show(self, totzone=None, saturation=None, exponent=None, curve=None):
        """Neue Werte setzen und zeichnen. Nicht genannte bleiben stehen."""
        if totzone is not None:
            self.werte['totzone'] = totzone
        if saturation is not None:
            self.werte['saettigung'] = saturation
        if exponent is not None:
            self.werte['exponent'] = exponent
        self.werte['kurve'] = curve
        self._draw()

    def toggle(self, whole=None):
        """Zwischen Quadrant und Vollansicht wechseln."""
        self.whole = (not self.whole) if whole is None else bool(whole)
        self._draw()
        return self.whole

    def deflection(self, wert):
        """Den aktuell gemessenen Ausschlag als Punkt einzeichnen.

        `None` nimmt ihn wieder weg. Gedacht für den Achsen-Test: Man bewegt
        den Stick und sieht den Punkt über die eigene Kurve wandern — daran
        erkennt man Drift und tote Ecken sofort.
        """
        self.zeiger = wert
        self._draw()

    # ------------------------------------------------------------------

    def _neu(self, _event=None):
        self._draw()

    def _area(self):
        """Die Zeichenfläche in Pixeln, mit Rand für die Beschriftung."""
        width = self.canvas.winfo_width()
        height = self.canvas.winfo_height()
        # Vor dem ersten Anzeigen meldet Tk 1 Pixel. Dann die gewünschte
        # Größe nehmen, sonst wird in ein 1×1-Feld gezeichnet.
        if width <= 1:
            width = int(self.canvas['width'])
        if height <= 1:
            height = int(self.canvas['height'])
        margin_left, margin_bottom, margin_top, margin_right = 34, 24, 10, 10
        return (margin_left, margin_top,
                max(10, width - margin_left - margin_right),
                max(10, height - margin_top - margin_bottom))

    def _punkt(self, on_state, off_state):
        """Von Kurvenwerten (-1..1 bzw. 0..1) auf Bildschirmpunkte."""
        x0, y0, width, height = self._area()
        if self.whole:
            pos_x = (on_state + 1.0) / 2.0
            pos_y = (off_state + 1.0) / 2.0
        else:
            pos_x = on_state
            pos_y = off_state
        return (x0 + pos_x * width, y0 + (1.0 - pos_y) * height)

    def _draw(self):
        self.canvas.delete('all')
        x0, y0, width, height = self._area()
        totzone = self.werte['totzone'] or 0.0
        saturation = (1.0 if self.werte['saettigung'] is None
                      else self.werte['saettigung'])

        # 1. Die Felder, in denen nichts passiert — zuerst, damit alles
        #    Weitere darüber liegt.
        self._bereiche(totzone, saturation)

        # 2. Gitter und Rahmen
        self._grid()

        # 3. Die Gerade als Vergleich: So liefe es ohne jede Einstellung.
        #    Ohne sie ist nicht zu sehen, ob eine Kurve steil oder flach ist.
        straight = ((-1.0, -1.0), (1.0, 1.0)) if self.whole else ((0.0, 0.0),
                                                               (1.0, 1.0))
        a = self._punkt(*straight[0])
        b = self._punkt(*straight[1])
        self.canvas.create_line(a[0], a[1], b[0], b[1], fill=LINIE,
                                  width=1, dash=(3, 3))

        # 4. Die Kurve selbst
        verlauf = curves.progression(totzone, saturation,
                                 self.werte['exponent'],
                                 self.werte['kurve'],
                                 steps=SUPPORT_POINTS, whole=self.whole)
        points = []
        for on_state, off_state in verlauf:
            points.extend(self._punkt(on_state, off_state))
        if len(points) >= 4:
            self.canvas.create_line(*points, fill=ACCENT, width=2,
                                      capstyle='round', joinstyle='round')

        # 5. Der gemessene Ausschlag, falls einer anliegt
        if self.zeiger is not None:
            off_state = curves.answer(self.zeiger, totzone, saturation,
                                 self.werte['exponent'], self.werte['kurve'])
            px, py = self._punkt(self.zeiger if self.whole
                                 else abs(self.zeiger), abs(off_state)
                                 if not self.whole else off_state)
            self.canvas.create_oval(px - 4, py - 4, px + 4, py + 4,
                                      fill=GOLD, outline=BG, width=1)

        self._label_axes(totzone, saturation)

    def _bereiche(self, totzone, saturation):
        """Totzone und Sättigungsbereich als gedämpfte Flächen.

        Beide sind „verschenkter Weg": In der Totzone bewegt sich nichts,
        jenseits der Sättigung ändert sich nichts mehr. Wer sie sieht, versteht
        sofort, warum sein Stick sich anfühlt, wie er sich anfühlt.
        """
        x0, y0, width, height = self._area()

        def band(von, bis, colour):
            if bis <= von:
                return
            a = self._punkt(von, -1.0 if self.whole else 0.0)
            b = self._punkt(bis, 1.0)
            self.canvas.create_rectangle(a[0], y0, b[0], y0 + height,
                                           fill=colour, outline='')

        # Ein sehr dunkles Blaugrau — sichtbar, aber ohne die Kurve zu stören.
        tot_farbe = '#1d2534'
        if totzone > 0:
            band(0.0, totzone, tot_farbe)
            if self.whole:
                band(-totzone, 0.0, tot_farbe)
        if saturation < 1.0:
            band(saturation, 1.0, tot_farbe)
            if self.whole:
                band(-1.0, -saturation, tot_farbe)

    def _grid(self):
        x0, y0, width, height = self._area()
        # Viertel-Linien — mehr wäre Unruhe, weniger gäbe keinen Anhalt.
        for share in (0.25, 0.5, 0.75):
            x = x0 + share * width
            y = y0 + share * height
            self.canvas.create_line(x, y0, x, y0 + height, fill=LINIE)
            self.canvas.create_line(x0, y, x0 + width, y, fill=LINIE)
        self.canvas.create_rectangle(x0, y0, x0 + width, y0 + height,
                                       outline=LINIE)
        if self.whole:
            # Die Nulllinien kräftiger — in der Vollansicht sind sie der
            # Bezugspunkt, um den herum die Kurve punktsymmetrisch liegt.
            centre_x = x0 + width / 2.0
            centre_y = y0 + height / 2.0
            self.canvas.create_line(centre_x, y0, centre_x, y0 + height,
                                      fill=SUB)
            self.canvas.create_line(x0, centre_y, x0 + width, centre_y,
                                      fill=SUB)

    def _label_axes(self, totzone, saturation):
        x0, y0, width, height = self._area()
        small = self.small
        left = '-1' if self.whole else '0'
        self.canvas.create_text(x0, y0 + height + 12, text=left, fill=SUB,
                                  font=small, anchor='w')
        self.canvas.create_text(x0 + width, y0 + height + 12, text='1',
                                  fill=SUB, font=small, anchor='e')
        self.canvas.create_text(x0 - 6, y0 + height, text=left, fill=SUB,
                                  font=small, anchor='e')
        self.canvas.create_text(x0 - 6, y0, text='1', fill=SUB, font=small,
                                  anchor='e')
        # Was auf welcher Achse steht — ohne das ist ein Diagramm ein Muster.
        self.canvas.create_text(x0 + width / 2.0, y0 + height + 12,
                                  text=t('s_kv_achse_ein'), fill=SUB,
                                  font=small, anchor='center')
        # ⚠ Die senkrechte Beschriftung braucht `angle` (Tk 8.6). Fehlt es,
        # wird sie weggelassen statt quer über die Kurve gelegt — ein Werkzeug
        # darf an einer Beschriftung nicht scheitern.
        try:
            self.canvas.create_text(x0 - 20, y0 + height / 2.0,
                                      text=t('s_kv_achse_aus'), fill=SUB,
                                      font=small, anchor='center', angle=90)
        except tk.TclError:
            pass
