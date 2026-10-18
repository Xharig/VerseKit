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
Arbeit aus Nebenfäden in den Tk-Faden bringen.

⭐⭐ **Ein Nebenfaden fasst Tk nicht an — auch nicht `root.after()`.**
`root.after()` aus einem Nebenfaden geht nur, solange der Hauptfaden gerade
IN `mainloop()` steckt — nicht während einer Rückfrage, einer
`update()`-Schleife oder beim Abbau. Sonst wirft es `RuntimeError: main
thread is not in main loop`, und das Ergebnis kommt nie an (Python 3.12 und
3.14 gleich).

Deshalb legt ein Nebenfaden die Tat in `CALLS` ab, und der Tk-Faden holt sie
alle 100 ms ab (`start_poller`). Gestartet wird der Abholer beim Start des
Overlays — er läuft also auch, wenn das Hauptfenster nie geöffnet wird.

⚠ **Je Takt höchstens `SLICE_S` Sekunden.** Eine lange Warteschlange würde
sonst den Tk-Faden am Stück blockieren; was übrig bleibt, kommt im nächsten,
sofort folgenden Takt dran.
"""
import queue
import threading
import time
import tkinter as tk

CALLS = queue.Queue()
POLL_MS = 100
SLICE_S = 0.05
_POLLER = [None]


def start_poller(root):
    """Die Warteschlange im Tk-Faden abarbeiten — je Tk-Wurzel einmal.

    Nur aus dem Tk-Faden aufrufen."""
    # ⚠ An der Tk-Wurzel, nicht an einem `Toplevel`: Ginge das zu, stünde der
    # Abholer still, und was Nebenfäden danach ablegen, sammelte sich an.
    try:
        root = root._root()
    except AttributeError:
        pass
    if _POLLER[0] is root:
        return

    def poll():
        left = drain()
        try:
            root.after(1 if left else POLL_MS, poll)
        except (tk.TclError, RuntimeError):
            if _POLLER[0] is root:
                _POLLER[0] = None

    try:
        root.after(POLL_MS, poll)
        _POLLER[0] = root
    except (tk.TclError, RuntimeError) as exception:
        from . import errors
        errors.record('tk_calls.start_poller', exception)


def drain(slice_s=SLICE_S):
    """Abarbeiten, bis die Schlange leer oder die Zeit um ist. True, wenn
    noch etwas übrig ist. Nur im Tk-Faden."""
    deadline = time.monotonic() + slice_s
    while True:
        if time.monotonic() > deadline:
            return not CALLS.empty()
        try:
            action = CALLS.get_nowait()
        except queue.Empty:
            return False
        try:
            action()
        except Exception as exception:
            from . import errors
            errors.record('tk_calls.action', exception)


def from_thread(widget, action):
    """`action` im Tk-Faden ausführen, solange `widget` lebt.

    Aus jedem Faden aufrufbar. Ist `widget` inzwischen weg (Seite gewechselt,
    Fenster zu), passiert nichts."""
    def run():
        try:
            if not widget.winfo_exists():
                return
        except tk.TclError:
            return
        action()
    CALLS.put(run)


def call(action):
    """`action` im Tk-Faden ausführen — ohne Bezug auf ein Widget."""
    CALLS.put(action)


def in_main_thread():
    return threading.current_thread() is threading.main_thread()


def run_in_thread(work, done, place='tk_calls.run_in_thread'):
    """`work()` in einem Nebenfaden, danach `done(ergebnis)` im Tk-Faden.

    Für Netz- und Dateiarbeit, die ein Knopf anstößt: Im Tk-Faden stünde so
    lange das ganze Fenster. `done` kommt **immer** — wirft `work`, wird der
    Fehler gemerkt und `done(None)` gerufen, damit eine Sperre gegen einen
    zweiten Lauf nicht für immer hängen bleibt."""
    def run():
        result = None
        try:
            result = work()
        except Exception as exception:
            from . import errors
            errors.record(place, exception)
        finally:
            CALLS.put(lambda: done(result))
    threading.Thread(target=run, daemon=True).start()
