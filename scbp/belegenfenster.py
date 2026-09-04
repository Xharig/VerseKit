# SPDX-License-Identifier: GPL-3.0-only
#
# SC BP Watcher — eine Aktion neu belegen
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
„Druecke jetzt die Taste" — das Fenster zum Neubelegen.

## Der Ablauf, und warum er so ist

1. Der Spieler klickt eine Zeile der Belegungsliste an.
2. Dieses Fenster oeffnet sich und **wartet auf eine Eingabe** — Stick-Knopf,
   Taste oder Maustaste, alles gleichberechtigt.
3. Was erkannt wurde, steht sofort da, zusammen mit dem, was bisher auf
   dieser Eingabe lag.
4. **Erst ein Klick auf „Uebernehmen" schreibt.** Nichts passiert nebenbei.

⚠⚠ **Warum nicht sofort schreiben, sobald etwas erkannt ist?** Weil die
Erkennung danebenliegen kann — eine zittrige Achse, ein Knopf, der beim
Loslassen prellt, unter Windows ein ungetesteter Weg. Zwischen „erkannt" und
„geschrieben" gehoert ein Mensch. In der Datei haengt die komplette Steuerung
des Spielers.

## Zwei Wege hinein, mit Absicht

| Was | Wie erkannt |
|---|---|
| Stick, Pedale, Gamepad | Geraetedatei bzw. `winmm` — in einem eigenen Faden |
| Tastatur, Maus | Tk-Ereignisse **dieses Fensters** |

⚠ Die Tastatur wird ausdruecklich **nicht** systemweit mitgelesen. Das waere
ein Keylogger; hier hoert nur das eigene Fenster zu, solange es den
Eingabezeiger hat. Siehe `scbp/eingabe.py`.

## ⚠ Das Spiel muss zu sein

Star Citizen schreibt die `actionmaps.xml` beim Beenden selbst und wuerde jede
Aenderung ueberschreiben. Das Fenster sagt es, statt es vorauszusetzen.
"""
import threading
import tkinter as tk

from . import eingabe, fehler, joysticks
from .sprache import t

BG      = '#10141c'
FLAECHE = '#161c28'
BAR     = '#1b2230'
FG      = '#e6edf3'
SUB     = '#8b98a5'
ACCENT  = '#9ce430'
LINIE   = '#232c3d'
GOLD    = '#e8c353'
ROT     = '#e05252'

# Wie lange auf einen Stick-Knopf gewartet wird, bevor der Faden aufgibt.
# Kurz genug, dass ein vergessenes Fenster nichts offen haelt; lang genug,
# dass man den richtigen Knopf sucht.
GEDULD = 20.0


class Belegenfenster:
    """Fragt eine Eingabe ab und schreibt sie auf Wunsch in die Belegung."""

    def __init__(self, eltern, aktion, bereich, kennzeichen, klarname='',
                 bisher='', fertig=None):
        self.aktion = aktion
        self.bereich = bereich
        self.kennzeichen = kennzeichen
        self.fertig = fertig
        self.erkannt = None
        self._laeuft = True

        self.root = tk.Toplevel(eltern)
        self.root.title('SC BP Watcher — ' + t('s_js_b_titel'))
        self.root.configure(bg=BG)
        self.root.geometry('520x340')
        self.root.resizable(False, False)
        self.root.transient(eltern)
        self.root.protocol('WM_DELETE_WINDOW', self.schliessen)

        kopf = tk.Frame(self.root, bg=BAR)
        kopf.pack(fill='x')
        tk.Label(kopf, text=(klarname or aktion), bg=BAR, fg=FG,
                 font=('Segoe UI', 11, 'bold'), anchor='w',
                 wraplength=470, justify='left').pack(fill='x', padx=16,
                                                      pady=(11, 2))
        tk.Label(kopf, text='%s · %s' % (kennzeichen, bereich or ''),
                 bg=BAR, fg=SUB, font=('Segoe UI', 9),
                 anchor='w').pack(fill='x', padx=16, pady=(0, 11))

        leib = tk.Frame(self.root, bg=BG)
        leib.pack(fill='both', expand=True, padx=16, pady=12)

        if bisher:
            tk.Label(leib, text=t('s_js_b_bisher', bisher), bg=BG, fg=SUB,
                     font=('Segoe UI', 9), anchor='w').pack(fill='x')

        self.aufforderung = tk.Label(leib, text=t('s_js_b_druecke'), bg=BG,
                                     fg=FG, font=('Segoe UI', 11), anchor='w',
                                     wraplength=470, justify='left')
        self.aufforderung.pack(fill='x', pady=(12, 6))

        self.anzeige = tk.Label(leib, text='—', bg=FLAECHE, fg=ACCENT,
                                font=('Segoe UI', 14, 'bold'), pady=14)
        self.anzeige.pack(fill='x')

        self.konflikt = tk.Label(leib, text='', bg=BG, fg=GOLD,
                                 font=('Segoe UI', 9), anchor='w',
                                 wraplength=470, justify='left')
        self.konflikt.pack(fill='x', pady=(8, 0))

        tk.Label(leib, text=t('s_js_spiel_zu'), bg=BG, fg=SUB,
                 font=('Segoe UI', 9), anchor='w', wraplength=470,
                 justify='left').pack(fill='x', pady=(8, 0))

        fuss = tk.Frame(self.root, bg=BG)
        fuss.pack(fill='x', padx=16, pady=(0, 14))
        self.ok = tk.Label(fuss, text=' %s ' % t('s_js_b_uebernehmen'),
                           bg=FLAECHE, fg=SUB, font=('Segoe UI', 10),
                           padx=12, pady=7)
        self.ok.pack(side='left')
        self.ok.bind('<Button-1>', lambda e: self._uebernehmen())
        loeschen = tk.Label(fuss, text=' %s ' % t('s_js_b_loeschen'),
                            bg=FLAECHE, fg=ROT, font=('Segoe UI', 10),
                            padx=12, pady=7, cursor='hand2')
        loeschen.pack(side='left', padx=(8, 0))
        loeschen.bind('<Button-1>', lambda e: self._loeschen())
        abbruch = tk.Label(fuss, text=' %s ' % t('s_js_b_abbruch'), bg=BG,
                           fg=SUB, font=('Segoe UI', 10), padx=12, pady=7,
                           cursor='hand2')
        abbruch.pack(side='right')
        abbruch.bind('<Button-1>', lambda e: self.schliessen())

        # ⚠ Tastatur und Maus fängt **dieses Fenster** ab, nichts sonst.
        self.root.bind('<KeyPress>', self._taste)
        self.root.bind('<Button>', self._maustaste)
        self.root.bind('<MouseWheel>', self._rad)          # Windows/macOS
        self.root.bind('<Button-4>', lambda e: self._setzen('mo1',
                                                            'mwheel_up'))
        self.root.bind('<Button-5>', lambda e: self._setzen('mo1',
                                                            'mwheel_down'))
        self.root.focus_force()
        self.root.grab_set()

        if eingabe.verfuegbar():
            self._lauschen()
        else:
            # Kein Stick erkennbar — Tastatur und Maus gehen trotzdem.
            self.aufforderung.configure(text=t('s_js_b_nur_tastatur'))

    # ------------------------------------------------------------- erkennen

    def _lauschen(self):
        """Auf einen Stick-Knopf warten — in einem eigenen Faden.

        ⚠ Der Faden fasst **keine** Oberfläche an. Das Ergebnis wird über
        `after` in den Faden der Oberfläche zurückgereicht; Tk ist nicht
        nebenläufig und stürzt sonst irgendwann wortlos ab.
        """
        def arbeit():
            try:
                treffer = eingabe.warten(GEDULD, abbruch=lambda: not self._laeuft)
            except Exception as ausnahme:
                fehler.merken('belegenfenster.lauschen', ausnahme)
                treffer = None
            if treffer and self._laeuft:
                try:
                    self.root.after(0, lambda: self._vom_stick(treffer))
                except Exception:
                    pass

        threading.Thread(target=arbeit, daemon=True).start()

    def _vom_stick(self, treffer):
        """Ein Stick hat gemeldet — welches Gerät war es?"""
        kennzeichen = self._geraet_zu_kennzeichen(treffer.get('kennung'))
        if not kennzeichen:
            # Das Gerät steht noch in keiner Belegung. Dann ist unklar, welche
            # Nummer das Spiel ihm gibt — lieber sagen als raten.
            self.aufforderung.configure(text=t('s_js_b_fremd'), fg=GOLD)
            return
        self._setzen(kennzeichen, treffer.get('eingabe', ''))

    def _geraet_zu_kennzeichen(self, kennung):
        """Aus der Geräte-Kennung die Nummer machen, die das Spiel benutzt."""
        if not kennung:
            return ''
        try:
            for z in joysticks.zuordnung():
                if (z.get('kennung') or '').upper() == kennung.upper():
                    return 'js%d' % z['nummer']
        except Exception as ausnahme:
            fehler.merken('belegenfenster.zuordnung', ausnahme)
        return ''

    def _taste(self, ereignis):
        name = eingabe.taste_aus_tk(getattr(ereignis, 'keysym', ''))
        if name == 'escape':
            self.schliessen()
            return
        if name:
            self._setzen('kb1', name)

    def _maustaste(self, ereignis):
        # 4 und 5 sind unter X11 das Rad — die haben eigene Bindungen.
        if getattr(ereignis, 'num', 0) in (4, 5):
            return
        name = eingabe.maus_aus_tk(nummer=getattr(ereignis, 'num', 0))
        if name:
            self._setzen('mo1', name)

    def _rad(self, ereignis):
        self._setzen('mo1', eingabe.maus_aus_tk(
            rad=getattr(ereignis, 'delta', 0)))

    def _setzen(self, kennzeichen, name):
        """Eine erkannte Eingabe anzeigen — geschrieben wird noch nicht."""
        if not name:
            return
        self._laeuft = False
        self.erkannt = (kennzeichen, name)
        self.anzeige.configure(text='%s  %s' % (kennzeichen, name))
        self.ok.configure(fg=ACCENT, cursor='hand2')
        self.aufforderung.configure(text=t('s_js_b_nochmal'), fg=SUB)
        # Wieder lauschen: Wer sich vertan hat, drückt einfach nochmal.
        self._laeuft = True
        if eingabe.verfuegbar():
            self._lauschen()

        try:
            andere = joysticks.konflikte(self.aktion, kennzeichen, name)
        except Exception:
            andere = []
        if andere:
            namen = []
            try:
                klar = joysticks.klarnamen(_sprache())
            except Exception:
                klar = {}
            for e in andere[:3]:
                namen.append(klar.get(e['aktion'], ('', ''))[0]
                             or e['aktion'])
            self.konflikt.configure(text=t('s_js_b_konflikt',
                                           ', '.join(namen)))
        else:
            self.konflikt.configure(text='')

    # -------------------------------------------------------------- schreiben

    def _uebernehmen(self):
        if not self.erkannt:
            return
        kennzeichen, name = self.erkannt
        self._schreiben(kennzeichen, name)

    def _loeschen(self):
        """Die Belegung entfernen — und zwar so, wie das Spiel es versteht."""
        self._schreiben(self.kennzeichen, '')

    def _schreiben(self, kennzeichen, name):
        from tkinter import messagebox
        erfolg, meldung, _ = joysticks.belegen(self.aktion, self.bereich,
                                               kennzeichen, name)
        if erfolg:
            messagebox.showinfo(t('s_js_b_titel'), t('s_js_fertig', meldung),
                                parent=self.root)
            self.schliessen(True)
        else:
            messagebox.showwarning(t('s_js_b_titel'),
                                   t('s_js_schief', t(meldung)),
                                   parent=self.root)

    def schliessen(self, geaendert=False):
        self._laeuft = False
        try:
            self.root.grab_release()
        except Exception:
            pass
        try:
            self.root.destroy()
        except Exception:
            pass
        if geaendert and self.fertig:
            try:
                self.fertig()
            except Exception as ausnahme:
                fehler.merken('belegenfenster.fertig', ausnahme)


def _sprache():
    from .sprache import aktuelle
    try:
        return aktuelle()
    except Exception:
        return 'de'
