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

BG     = '#1b1b1b'
FG     = '#e8e8e8'
BORDER = '#3a3a3a'


class _Window:
    """Das eine Hinweisfenster. Wird beim ersten Bedarf angelegt."""

    def __init__(self):
        self.top = None
        self.label = None

    def show(self, parent, text, x, y):
        if not text:
            return
        try:
            if self.top is None or not self.top.winfo_exists():
                self.top = tk.Toplevel(parent)
                self.top.overrideredirect(True)      # keine Fensterdekoration
                self.top.attributes('-topmost', True)
                self.label = tk.Label(self.top, text=text, bg=BG, fg=FG,
                                      font=('Segoe UI', 9), justify='left',
                                      padx=8, pady=4,
                                      highlightbackground=BORDER,
                                      highlightthickness=1)
                self.label.pack()
            else:
                self.label.configure(text=text)
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
                                 event.x_root + OFFSET_X,
                                 event.y_root + OFFSET_Y))

    def cancel(event=None):
        if state['job'] is not None:
            try:
                widget.after_cancel(state['job'])
            except tk.TclError:
                pass
            state['job'] = None
        _window.hide()

    widget.bind('<Enter>', on_enter, add='+')
