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
Erklärtexte beim Überfahren mit der Maus.

Die Titelleiste besteht aus sieben Zeichen — ⟳ ⓘ ☰ ⏻ 🗑 ✕ ◢. Wer sie nicht
selbst gebaut hat, muss raten, was sie tun, und ausprobieren ist bei ✕ und 🗑
eine schlechte Idee. Eine Beschriftung daneben scheidet aus: Das Overlay ist
absichtlich schmal und liegt über dem Spiel.

Benutzung:

    from .notice import attach
    attach(knopf, lambda: t('hinweis_schliessen'))
    attach(knopf, 'fester Text')

Der Text darf eine **Funktion** sein statt einer Zeichenkette. Nötig für alles,
was seinen Zustand wechselt (Autostart an/aus, Stern gesetzt/nicht) — sonst
stünde dort für immer, was beim Programmstart zutraf. Und für die Sprache: Wer
im laufenden Betrieb umschaltet, soll nicht die alten Texte behalten.

Bewusst schlicht gehalten:

* **Ein** Fenster für alle Hinweise, nicht eines je Element. Bei einem Dutzend
  Knöpfen wären das ein Dutzend Fenster, die bei jedem Sprachwechsel und jedem
  Beenden mitgezogen werden müssten.
* `topmost`, weil das Overlay selbst `topmost` ist — ohne das erscheint der
  Hinweis **hinter** dem Fenster, zu dem er gehört.
* Verzögerung von einer knappen halben Sekunde. Ohne sie flackert es beim
  bloßen Überqueren der Leiste.
* Der Hinweis verschwindet auch beim **Klick**. Sonst bliebe er über einem
  Fenster stehen, das gerade zugegangen ist.

⚠ Bis zum 11.09.2026 hieß dieses Modul `hinweis`, die Funktion `anhaengen`
(Sprachumstellung P3). **Nur das Modul ist umbenannt.** Das Wort `hinweis`
steht an vielen anderen Stellen weiter — und muss dort bleiben: als Kennung in
der Nachrichten-Warteschlange (`q.put(('hinweis', …))`), als Feld im
Fehlerprotokoll, das in der Datei des Nutzers steht, und in den Textschlüsseln
`hinweis_…`. Ein pauschales Ersetzen hätte alle drei zerschossen.

⚠ Prüfung 112 im Selbsttest schneidet den Quelltext dieser Datei an den Namen
`attach`, `on_enter` und `cancel` auf. Wer die umbenennt, zieht die Prüfung mit.
"""
import tkinter as tk

DELAY_MS = 450               # bis der Hinweis kommt
OFFSET_X, OFFSET_Y = 12, 22  # neben und unter dem Mauszeiger
WRAP = 420                   # längere Texte brechen um, statt quer zu laufen

BG     = '#1b1b1b'
FG     = '#e8e8e8'
BORDER = '#3a3a3a'


# Fenster, deren Hinweise über dem Zeiger stehen sollen: je Fenster das
# Fenster selbst und die Funktion, die sagt, ob gerade „oben" gilt.
# ⚠ Nicht nach dem Tk-Pfad: Jede Tk-Wurzel heißt `.`, zwei Wurzeln (im
# Selbsttest die Regel) wären sonst dasselbe Fenster.
_ABOVE = {}


def prefer_above(window, ask):
    """Hinweise in diesem Fenster über den Zeiger setzen, solange `ask()` wahr ist.

    ⚠⚠ Für das Overlay mit der Leiste **unten**. Dort gehören die Hinweise
    immer nach oben, nicht erst, wenn unten der Platz ausgeht: Unter der
    Leiste liegt meist die Taskleiste, und ein Hinweis darüber ist kaum zu
    lesen — auch wenn er rechnerisch noch ins Bild passt.
    """
    _ABOVE[id(window)] = (window, ask)


def _wants_above(widget):
    try:
        window, ask = _ABOVE.get(id(widget.winfo_toplevel()), (None, None))
        return bool(window is widget.winfo_toplevel() and ask())
    except Exception:
        return False


def position(pointer_x, pointer_y, width, height, area, above=False):
    """Wohin der Hinweis kommt: neben den Mauszeiger, aber nie aus dem Bild.

    ⚠ Bis v3.62.0 stand er immer rechts unter dem Zeiger. Wer das Overlay in
    die untere rechte Ecke legt, bekam die Hinweise damit rechts und unten
    abgeschnitten. Gemeldet von Aeternitas26 (29.09.2026).

    Mit `above` (Leiste unten) steht er immer **über** dem Zeiger, sonst nur,
    wenn unten kein Platz ist; ist rechts keiner, rückt er nach links an die
    Kante. `area` ist (x, y, breite, hoehe) des Bildschirms unter dem Zeiger,
    ohne Taskleiste.
    """
    sx, sy, sb, sh = area
    x = pointer_x + OFFSET_X
    y = pointer_y + OFFSET_Y
    if x + width > sx + sb:
        x = sx + sb - width
    oben = pointer_y - OFFSET_Y // 2 - height
    if above and oben >= sy:
        y = oben
    elif y + height > sy + sh:
        y = oben
    return max(sx, x), max(sy, y)


class _Window:
    """Das eine Hinweisfenster. Wird beim ersten Bedarf angelegt."""

    def __init__(self):
        self.top = None
        self.label = None

    def show(self, parent, text, pointer_x, pointer_y):
        if not text:
            return
        try:
            if self.top is None or not self.top.winfo_exists():
                self.top = tk.Toplevel(parent)
                self.top.overrideredirect(True)      # keine Fensterdekoration
                self.top.attributes('-topmost', True)
                self.label = tk.Label(self.top, text=text, bg=BG, fg=FG,
                                      font=('Segoe UI', 9), justify='left',
                                      padx=8, pady=4, wraplength=WRAP,
                                      highlightbackground=BORDER,
                                      highlightthickness=1)
                self.label.pack()
            else:
                self.label.configure(text=text)
            self.top.update_idletasks()
            try:
                from . import screen
                area = screen.work_area(parent, pointer_x, pointer_y)
            except Exception:
                area = (0, 0, parent.winfo_screenwidth(),
                        parent.winfo_screenheight())
            x, y = position(pointer_x, pointer_y, self.top.winfo_reqwidth(),
                            self.top.winfo_reqheight(), area,
                            above=_wants_above(parent))
            self.top.wm_geometry('+%d+%d' % (x, y))
            self.top.deiconify()
        except tk.TclError:
            # Fenster ging zwischendurch zu — ein Hinweis ist nichts, wofür
            # das Programm stehenbleiben darf.
            self.top = None

    def hide(self):
        try:
            if self.top is not None and self.top.winfo_exists():
                self.top.withdraw()
        except tk.TclError:
            self.top = None


_window = _Window()


def attach(widget, text):
    """Einem Element einen Erklärtext geben. `text` ist Zeichenkette oder Funktion.

    ⚠⚠ **Nur EIN Binding beim Anhängen — die anderen drei kommen erst, wenn die
    Maus das Element zum ersten Mal berührt.** Das ist keine Spielerei, sondern
    der Grund, warum die Bauplan-Liste zäh aufging: Eine Zeile hängt bis zu vier
    Erklärtexte an, und mit vier Bindings je Text waren das **16 Bindings pro
    Zeile** — bei 40 Zeilen über 600 Stück, die alle gesetzt werden mussten,
    bevor das Fenster stand. Gemessen wurden 55 ms für die Zeilen, rund 1,4 ms
    je Stück.

    ⚠ **Und es geht nichts verloren.** Die drei nachgezogenen Bindings räumen
    einen laufenden Anzeige-Auftrag ab — den es vor dem ersten `<Enter>` gar
    nicht geben kann. Wer nie mit der Maus hinfährt, braucht sie also nie; wer
    hinfährt, hat sie ab dem ersten Mal. Der Nutzer merkt keinen Unterschied.
    """
    state = {'job': None, 'rest': False}

    def get_text():
        return text() if callable(text) else text

    def on_enter(event):
        if not state['rest']:
            # Ab jetzt kann ein Auftrag laufen — also jetzt die Abräumer setzen.
            state['rest'] = True
            widget.bind('<Leave>', cancel, add='+')
            widget.bind('<Button-1>', cancel, add='+')
            widget.bind('<Destroy>', cancel, add='+')
        cancel()
        state['job'] = widget.after(
            DELAY_MS,
            lambda: _window.show(widget, get_text(),
                                 event.x_root, event.y_root))

    def cancel(event=None):
        if state['job'] is not None:
            try:
                widget.after_cancel(state['job'])
            except tk.TclError:
                pass
            state['job'] = None
        _window.hide()

    widget.bind('<Enter>', on_enter, add='+')
