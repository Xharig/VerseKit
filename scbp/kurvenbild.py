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

from . import kurven
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
STUETZSTELLEN = 160


def gross_zeigen(eltern, titel, totzone=None, saettigung=None, exponent=None,
                 kurve=None, ganz=False, schrift=None, klein=None):
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
    fenster = tk.Toplevel(eltern)
    fenster.title(titel)
    fenster.configure(bg=BG)
    rand = 40
    seite = 560
    fenster.geometry('%dx%d' % (seite + rand, seite + rand + 46))
    fenster.minsize(360, 400)

    kopf = tk.Frame(fenster, bg=BG)
    kopf.pack(fill='x', side='top')

    bild = Kurvenbild(fenster, breite=seite, hoehe=seite, ganz=ganz,
                      schrift=schrift, klein=klein)
    bild.pack(fill='both', expand=True, padx=20, pady=(0, 20))
    bild.zeigen(totzone=totzone, saettigung=saettigung, exponent=exponent,
                kurve=kurve)

    zustand = {'ganz': ganz}

    def _umschalten():
        zustand['ganz'] = bild.umschalten()
        knopf.configure(text=(t('s_kv_quadrant') if zustand['ganz']
                              else t('s_kv_ganz')))

    knopf = tk.Label(kopf, text=(t('s_kv_quadrant') if ganz
                                 else t('s_kv_ganz')),
                     bg=FLAECHE, fg=FG, font=klein or schrift,
                     padx=12, pady=6, cursor='hand2')
    knopf.pack(side='left', padx=20, pady=14)
    knopf.bind('<Button-1>', lambda _e: _umschalten())

    werte = tk.Label(
        kopf,
        text='%s %s   ·   %s %s' % (
            t('s_kv_totzone'), '—' if totzone is None else ('%g' % totzone),
            t('s_kv_saettigung'),
            '—' if saettigung is None else ('%g' % saettigung)),
        bg=BG, fg=SUB, font=klein or schrift)
    werte.pack(side='left')
    return fenster


class Kurvenbild:
    """Die Antwortkurve einer Achse auf einer Leinwand.

    Benutzung:

        bild = Kurvenbild(rahmen, breite=260, hoehe=260)
        bild.zeigen(totzone=0.1, saettigung=0.9, exponent=1.5)

    `ganz=True` schaltet auf die Vollansicht (-1 bis 1), Standard ist der
    Quadrant. Umschalten geht jederzeit über `umschalten()`.
    """

    def __init__(self, eltern, breite=260, hoehe=260, ganz=False,
                 schrift=None, klein=None):
        self.ganz = bool(ganz)
        self.schrift = schrift
        self.klein = klein or schrift
        self.werte = {'totzone': 0.0, 'saettigung': 1.0, 'exponent': 1.0,
                      'kurve': None}
        self.zeiger = None          # aktueller Ausschlag, falls gemessen
        self.leinwand = tk.Canvas(eltern, width=breite, height=hoehe,
                                  bg=FLAECHE, highlightthickness=1,
                                  highlightbackground=LINIE, bd=0)
        # ⚠ Neu zeichnen, sobald die Fläche wirklich steht — nicht nur einmal
        # beim Bauen. Siehe Modulkopf.
        self.leinwand.bind('<Configure>', self._neu)

    def pack(self, **kwargs):
        self.leinwand.pack(**kwargs)
        return self

    def grid(self, **kwargs):
        self.leinwand.grid(**kwargs)
        return self

    def zeigen(self, totzone=None, saettigung=None, exponent=None, kurve=None):
        """Neue Werte setzen und zeichnen. Nicht genannte bleiben stehen."""
        if totzone is not None:
            self.werte['totzone'] = totzone
        if saettigung is not None:
            self.werte['saettigung'] = saettigung
        if exponent is not None:
            self.werte['exponent'] = exponent
        self.werte['kurve'] = kurve
        self._zeichnen()

    def umschalten(self, ganz=None):
        """Zwischen Quadrant und Vollansicht wechseln."""
        self.ganz = (not self.ganz) if ganz is None else bool(ganz)
        self._zeichnen()
        return self.ganz

    def ausschlag(self, wert):
        """Den aktuell gemessenen Ausschlag als Punkt einzeichnen.

        `None` nimmt ihn wieder weg. Gedacht für den Achsen-Test: Man bewegt
        den Stick und sieht den Punkt über die eigene Kurve wandern — daran
        erkennt man Drift und tote Ecken sofort.
        """
        self.zeiger = wert
        self._zeichnen()

    # ------------------------------------------------------------------

    def _neu(self, _ereignis=None):
        self._zeichnen()

    def _flaeche(self):
        """Die Zeichenfläche in Pixeln, mit Rand für die Beschriftung."""
        breite = self.leinwand.winfo_width()
        hoehe = self.leinwand.winfo_height()
        # Vor dem ersten Anzeigen meldet Tk 1 Pixel. Dann die gewünschte
        # Größe nehmen, sonst wird in ein 1×1-Feld gezeichnet.
        if breite <= 1:
            breite = int(self.leinwand['width'])
        if hoehe <= 1:
            hoehe = int(self.leinwand['height'])
        rand_links, rand_unten, rand_oben, rand_rechts = 34, 24, 10, 10
        return (rand_links, rand_oben,
                max(10, breite - rand_links - rand_rechts),
                max(10, hoehe - rand_oben - rand_unten))

    def _punkt(self, ein, aus):
        """Von Kurvenwerten (-1..1 bzw. 0..1) auf Bildschirmpunkte."""
        x0, y0, breite, hoehe = self._flaeche()
        if self.ganz:
            lage_x = (ein + 1.0) / 2.0
            lage_y = (aus + 1.0) / 2.0
        else:
            lage_x = ein
            lage_y = aus
        return (x0 + lage_x * breite, y0 + (1.0 - lage_y) * hoehe)

    def _zeichnen(self):
        self.leinwand.delete('all')
        x0, y0, breite, hoehe = self._flaeche()
        totzone = self.werte['totzone'] or 0.0
        saettigung = (1.0 if self.werte['saettigung'] is None
                      else self.werte['saettigung'])

        # 1. Die Felder, in denen nichts passiert — zuerst, damit alles
        #    Weitere darüber liegt.
        self._bereiche(totzone, saettigung)

        # 2. Gitter und Rahmen
        self._gitter()

        # 3. Die Gerade als Vergleich: So liefe es ohne jede Einstellung.
        #    Ohne sie ist nicht zu sehen, ob eine Kurve steil oder flach ist.
        gerade = ((-1.0, -1.0), (1.0, 1.0)) if self.ganz else ((0.0, 0.0),
                                                               (1.0, 1.0))
        a = self._punkt(*gerade[0])
        b = self._punkt(*gerade[1])
        self.leinwand.create_line(a[0], a[1], b[0], b[1], fill=LINIE,
                                  width=1, dash=(3, 3))

        # 4. Die Kurve selbst
        verlauf = kurven.verlauf(totzone, saettigung,
                                 self.werte['exponent'],
                                 self.werte['kurve'],
                                 schritte=STUETZSTELLEN, ganz=self.ganz)
        punkte = []
        for ein, aus in verlauf:
            punkte.extend(self._punkt(ein, aus))
        if len(punkte) >= 4:
            self.leinwand.create_line(*punkte, fill=ACCENT, width=2,
                                      capstyle='round', joinstyle='round')

        # 5. Der gemessene Ausschlag, falls einer anliegt
        if self.zeiger is not None:
            aus = kurven.antwort(self.zeiger, totzone, saettigung,
                                 self.werte['exponent'], self.werte['kurve'])
            px, py = self._punkt(self.zeiger if self.ganz
                                 else abs(self.zeiger), abs(aus)
                                 if not self.ganz else aus)
            self.leinwand.create_oval(px - 4, py - 4, px + 4, py + 4,
                                      fill=GOLD, outline=BG, width=1)

        self._beschriften(totzone, saettigung)

    def _bereiche(self, totzone, saettigung):
        """Totzone und Sättigungsbereich als gedämpfte Flächen.

        Beide sind „verschenkter Weg": In der Totzone bewegt sich nichts,
        jenseits der Sättigung ändert sich nichts mehr. Wer sie sieht, versteht
        sofort, warum sein Stick sich anfühlt, wie er sich anfühlt.
        """
        x0, y0, breite, hoehe = self._flaeche()

        def band(von, bis, farbe):
            if bis <= von:
                return
            a = self._punkt(von, -1.0 if self.ganz else 0.0)
            b = self._punkt(bis, 1.0)
            self.leinwand.create_rectangle(a[0], y0, b[0], y0 + hoehe,
                                           fill=farbe, outline='')

        # Ein sehr dunkles Blaugrau — sichtbar, aber ohne die Kurve zu stören.
        tot_farbe = '#1d2534'
        if totzone > 0:
            band(0.0, totzone, tot_farbe)
            if self.ganz:
                band(-totzone, 0.0, tot_farbe)
        if saettigung < 1.0:
            band(saettigung, 1.0, tot_farbe)
            if self.ganz:
                band(-1.0, -saettigung, tot_farbe)

    def _gitter(self):
        x0, y0, breite, hoehe = self._flaeche()
        # Viertel-Linien — mehr wäre Unruhe, weniger gäbe keinen Anhalt.
        for anteil in (0.25, 0.5, 0.75):
            x = x0 + anteil * breite
            y = y0 + anteil * hoehe
            self.leinwand.create_line(x, y0, x, y0 + hoehe, fill=LINIE)
            self.leinwand.create_line(x0, y, x0 + breite, y, fill=LINIE)
        self.leinwand.create_rectangle(x0, y0, x0 + breite, y0 + hoehe,
                                       outline=LINIE)
        if self.ganz:
            # Die Nulllinien kräftiger — in der Vollansicht sind sie der
            # Bezugspunkt, um den herum die Kurve punktsymmetrisch liegt.
            mitte_x = x0 + breite / 2.0
            mitte_y = y0 + hoehe / 2.0
            self.leinwand.create_line(mitte_x, y0, mitte_x, y0 + hoehe,
                                      fill=SUB)
            self.leinwand.create_line(x0, mitte_y, x0 + breite, mitte_y,
                                      fill=SUB)

    def _beschriften(self, totzone, saettigung):
        x0, y0, breite, hoehe = self._flaeche()
        klein = self.klein
        links = '-1' if self.ganz else '0'
        self.leinwand.create_text(x0, y0 + hoehe + 12, text=links, fill=SUB,
                                  font=klein, anchor='w')
        self.leinwand.create_text(x0 + breite, y0 + hoehe + 12, text='1',
                                  fill=SUB, font=klein, anchor='e')
        self.leinwand.create_text(x0 - 6, y0 + hoehe, text=links, fill=SUB,
                                  font=klein, anchor='e')
        self.leinwand.create_text(x0 - 6, y0, text='1', fill=SUB, font=klein,
                                  anchor='e')
        # Was auf welcher Achse steht — ohne das ist ein Diagramm ein Muster.
        self.leinwand.create_text(x0 + breite / 2.0, y0 + hoehe + 12,
                                  text=t('s_kv_achse_ein'), fill=SUB,
                                  font=klein, anchor='center')
        # ⚠ Die senkrechte Beschriftung braucht `angle` (Tk 8.6). Fehlt es,
        # wird sie weggelassen statt quer über die Kurve gelegt — ein Werkzeug
        # darf an einer Beschriftung nicht scheitern.
        try:
            self.leinwand.create_text(x0 - 20, y0 + hoehe / 2.0,
                                      text=t('s_kv_achse_aus'), fill=SUB,
                                      font=klein, anchor='center', angle=90)
        except tk.TclError:
            pass
