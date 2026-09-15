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
Eingabezeiger hat. Siehe `scbp/input_device.py`.

## ⚠ Das Spiel muss zu sein

Star Citizen schreibt die `actionmaps.xml` beim Beenden selbst und wuerde jede
Aenderung ueberschreiben. Das Fenster sagt es, statt es vorauszusetzen.
"""
import threading
import tkinter as tk

from . import input_device, fehler, joysticks
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
PATIENCE = 20.0


class BindingWindow:
    """Fragt eine Eingabe ab und schreibt sie auf Wunsch in die Belegung."""

    def __init__(self, parent, action, section, device_id, plain_name='',
                 previous='', done=None):
        self.action = action
        self.section = section
        self.device_id = device_id
        self.done = done
        self.detected = None
        self._running = True

        self.root = tk.Toplevel(parent)
        self.root.title(t('hf_titel') + ' — ' + t('s_js_b_titel'))
        self.root.configure(bg=BG)
        # ⚠⚠ **Mit Position, nicht nur mit Größe.** Ein `geometry` ohne
        # `+x+y` überlässt die Platzierung dem Fenstermanager — und der
        # weiß nichts vom Hauptfenster. Auf mehreren Bildschirmen landete
        # so am 06.09.2026 ein Fenster außerhalb des sichtbaren Bereichs;
        # weil es modal war, ließ sich das Programm nicht einmal beenden.
        #
        # `center_over` setzt beides und fällt auf die reine Größe
        # zurück, wenn es kein Elternfenster gibt (eigenständiger Start).
        from .main_window import center_over
        if parent is None or not center_over(self.root, parent, 520, 340):
            self.root.geometry('520x340')
        self.root.resizable(False, False)
        self.root.transient(parent)
        self.root.protocol('WM_DELETE_WINDOW', self.close)

        head = tk.Frame(self.root, bg=BAR)
        head.pack(fill='x')
        tk.Label(head, text=(plain_name or action), bg=BAR, fg=FG,
                 font=('Segoe UI', 11, 'bold'), anchor='w',
                 wraplength=470, justify='left').pack(fill='x', padx=16,
                                                      pady=(11, 2))
        tk.Label(head, text='%s · %s' % (device_id, section or ''),
                 bg=BAR, fg=SUB, font=('Segoe UI', 9),
                 anchor='w').pack(fill='x', padx=16, pady=(0, 11))

        body = tk.Frame(self.root, bg=BG)
        body.pack(fill='both', expand=True, padx=16, pady=12)

        if previous:
            tk.Label(body, text=t('s_js_b_bisher', previous), bg=BG, fg=SUB,
                     font=('Segoe UI', 9), anchor='w').pack(fill='x')

        self.prompt = tk.Label(body, text=t('s_js_b_druecke'), bg=BG,
                                     fg=FG, font=('Segoe UI', 11), anchor='w',
                                     wraplength=470, justify='left')
        self.prompt.pack(fill='x', pady=(12, 6))

        self.display = tk.Label(body, text='—', bg=FLAECHE, fg=ACCENT,
                                font=('Segoe UI', 14, 'bold'), pady=14)
        self.display.pack(fill='x')

        self.conflict = tk.Label(body, text='', bg=BG, fg=GOLD,
                                 font=('Segoe UI', 9), anchor='w',
                                 wraplength=470, justify='left')
        self.conflict.pack(fill='x', pady=(8, 0))

        tk.Label(body, text=t('s_js_spiel_zu'), bg=BG, fg=SUB,
                 font=('Segoe UI', 9), anchor='w', wraplength=470,
                 justify='left').pack(fill='x', pady=(8, 0))

        footer = tk.Frame(self.root, bg=BG)
        footer.pack(fill='x', padx=16, pady=(0, 14))
        self.ok = tk.Label(footer, text=' %s ' % t('s_js_b_uebernehmen'),
                           bg=FLAECHE, fg=SUB, font=('Segoe UI', 10),
                           padx=12, pady=7)
        self.ok.pack(side='left')
        self.ok.bind('<Button-1>', lambda e: self._take_over())
        clear = tk.Label(footer, text=' %s ' % t('s_js_b_loeschen'),
                            bg=FLAECHE, fg=ROT, font=('Segoe UI', 10),
                            padx=12, pady=7, cursor='hand2')
        clear.pack(side='left', padx=(8, 0))
        clear.bind('<Button-1>', lambda e: self._clear())
        cancel = tk.Label(footer, text=' %s ' % t('s_js_b_abbruch'), bg=BG,
                           fg=SUB, font=('Segoe UI', 10), padx=12, pady=7,
                           cursor='hand2')
        cancel.pack(side='right')
        cancel.bind('<Button-1>', lambda e: self.close())

        # ⚠ Tastatur und Maus fängt **dieses Fenster** ab, nichts sonst.
        self.root.bind('<KeyPress>', self._taste)
        self.root.bind('<Button>', self._maustaste)
        self.root.bind('<MouseWheel>', self._rad)          # Windows/macOS
        self.root.bind('<Button-4>', lambda e: self._apply('mo1',
                                                            'mwheel_up'))
        self.root.bind('<Button-5>', lambda e: self._apply('mo1',
                                                            'mwheel_down'))
        self.root.focus_force()
        self.root.grab_set()

        if input_device.available():
            self._listen()
        else:
            # Kein Stick erkennbar — Tastatur und Maus gehen trotzdem.
            self.prompt.configure(text=t('s_js_b_nur_tastatur'))

    # ------------------------------------------------------------- erkennen

    def _listen(self):
        """Auf einen Stick-Knopf warten — in einem eigenen Faden.

        ⚠ Der Faden fasst **keine** Oberfläche an. Das Ergebnis wird über
        `after` in den Faden der Oberfläche zurückgereicht; Tk ist nicht
        nebenläufig und stürzt sonst irgendwann wortlos ab.
        """
        def arbeit():
            try:
                match = input_device.wait(PATIENCE, stop_flag=lambda: not self._running)
            except Exception as ausnahme:
                fehler.merken('binding_window.listen', ausnahme)
                match = None
            if match and self._running:
                try:
                    self.root.after(0, lambda: self._vom_stick(match))
                except Exception:
                    pass

        threading.Thread(target=arbeit, daemon=True).start()

    def _vom_stick(self, match):
        """Ein Stick hat gemeldet — welches Gerät war es?"""
        device_id = self._device_to_id(match.get('kennung'))
        if not device_id:
            # Das Gerät steht noch in keiner Belegung. Dann ist unklar, welche
            # Nummer das Spiel ihm gibt — lieber sagen als raten.
            self.prompt.configure(text=t('s_js_b_fremd'), fg=GOLD)
            return
        self._apply(device_id, match.get('eingabe', ''))

    def _device_to_id(self, tag):
        """Aus der Geräte-Kennung die Nummer machen, die das Spiel benutzt."""
        if not tag:
            return ''
        try:
            for z in joysticks.assignment():
                if (z.get('kennung') or '').upper() == tag.upper():
                    return 'js%d' % z['nummer']
        except Exception as ausnahme:
            fehler.merken('binding_window.mapping', ausnahme)
        return ''

    def _taste(self, event):
        name = input_device.key_from_tk(getattr(event, 'keysym', ''))
        if name == 'escape':
            self.close()
            return
        if name:
            self._apply('kb1', name)

    def _maustaste(self, event):
        # 4 und 5 sind unter X11 das Rad — die haben eigene Bindungen.
        if getattr(event, 'num', 0) in (4, 5):
            return
        name = input_device.mouse_from_tk(number=getattr(event, 'num', 0))
        if name:
            self._apply('mo1', name)

    def _rad(self, event):
        self._apply('mo1', input_device.mouse_from_tk(
            wheel=getattr(event, 'delta', 0)))

    def _apply(self, device_id, name):
        """Eine erkannte Eingabe anzeigen — geschrieben wird noch nicht."""
        if not name:
            return
        self._running = False
        self.detected = (device_id, name)
        self.display.configure(text='%s  %s' % (device_id, name))
        self.ok.configure(fg=ACCENT, cursor='hand2')
        self.prompt.configure(text=t('s_js_b_nochmal'), fg=SUB)
        # Wieder lauschen: Wer sich vertan hat, drückt einfach nochmal.
        self._running = True
        if input_device.available():
            self._listen()

        try:
            others = joysticks.conflicts(self.action, device_id, name)
        except Exception:
            others = []
        if others:
            names = []
            try:
                label = joysticks.labels(_language())
            except Exception:
                label = {}
            for e in others[:3]:
                names.append(label.get(e['aktion'], ('', ''))[0]
                             or e['aktion'])
            self.conflict.configure(text=t('s_js_b_konflikt',
                                           ', '.join(names)))
        else:
            self.conflict.configure(text='')

    # -------------------------------------------------------------- schreiben

    def _take_over(self):
        if not self.detected:
            return
        device_id, name = self.detected
        self._write(device_id, name)

    def _clear(self):
        """Die Belegung entfernen — und zwar so, wie das Spiel es versteht."""
        self._write(self.device_id, '')

    def _write(self, device_id, name):
        # ⚠⚠ **Kein `messagebox`.** Der System-Dialog von Tk landet nicht
        # zuverlässig über dem Elternfenster: Am 06.09.2026 erschien er beim
        # Speichern der Belegung **außerhalb aller Bildschirme** — und weil er
        # modal ist, war das Programm damit unbedienbar und ließ sich nicht
        # einmal mehr beenden. Dazu kommen die bekannten Punkte: heller Kasten
        # im dunklen Programm, Knöpfe in der Systemsprache.
        #
        # `ask_yes_no` setzt sich mittig über das Elternfenster und wird
        # mit ihm geschlossen.
        from .main_window import ask_yes_no
        ok_state, message, _ = joysticks.bind_action(self.action, self.section,
                                               device_id, name)
        if ok_state:
            ask_yes_no(self.root, t('s_js_b_titel'),
                          t('s_js_fertig', message), only_ok=True)
            self.close(True)
        else:
            ask_yes_no(self.root, t('s_js_b_titel'),
                          t('s_js_schief', t(message)), only_ok=True)

    def close(self, changed=False):
        self._running = False
        try:
            self.root.grab_release()
        except Exception:
            pass
        try:
            self.root.destroy()
        except Exception:
            pass
        if changed and self.done:
            try:
                self.done()
            except Exception as ausnahme:
                fehler.merken('binding_window.done', ausnahme)


def _language():
    from .sprache import aktuelle
    try:
        return aktuelle()
    except Exception:
        return 'de'
