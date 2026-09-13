# -*- coding: utf-8 -*-
#
# SC BP Watcher — zeigt live neue Star-Citizen-Bauplaene an.
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
Die Titelleiste des Systems dunkel stellen (nur Windows).

Das Fenster ist von innen komplett dunkel — und obendrauf sass eine **weisse**
Leiste mit dem Fenstertitel und den drei Knoepfen. Am 31.08.2026 gemeldet:
„die Kopfleiste ist weiss, das ist mega haesslich". Sie gehoert nicht zum
Programm, sondern zu Windows: Wer im System das helle Design faehrt, bekommt
sie hell — egal, wie dunkel der Inhalt ist.

Windows laesst sie sich fensterweise umstellen, ueber
`DwmSetWindowAttribute` mit `DWMWA_USE_IMMERSIVE_DARK_MODE`. Reine
`ctypes`-Sache, kein Zusatzpaket — die Grundregel des Projekts bleibt
unangetastet.

⚠ **Die Kennzahl hat sich einmal geaendert.** Bis Windows 10 Build 18985 war
sie **19**, danach **20**. Deshalb werden beide versucht: Auf einem alten
System schlaegt 20 fehl und 19 greift, auf einem neuen umgekehrt. Raten waere
hier billiger als eine Fallunterscheidung nach Build-Nummer — die stimmt bei
Insider-Fassungen naemlich nicht.

⚠ **Erst wenn das Fenster wirklich existiert.** Vor dem ersten Zeichnen hat es
noch kein Handle; der Aufruf liefe ins Leere. Deshalb `update_idletasks()`
davor.

⚠ **Es darf nichts kosten, wenn es nicht geht.** Unter Linux, unter Wine, auf
einem Windows ohne `dwmapi` — ueberall gilt: still weiterlaufen. Eine helle
Leiste ist haesslich, ein Absturz beim Fensterbau waere schlimmer.
"""
import sys


ATTRIBUTES = (20, 19)                # neu zuerst, dann die alte Kennzahl

# Fuer das Neuzeichnen des Rahmens. ⚠ NOACTIVATE ist Pflicht: Ohne das holt
# sich das Fenster den Fokus — und wer gerade Star Citizen fliegt, landet
# mitten im Kampf auf dem Schreibtisch.
SWP_NOSIZE, SWP_NOMOVE, SWP_NOZORDER = 0x0001, 0x0002, 0x0004
SWP_NOACTIVATE, SWP_FRAMECHANGED = 0x0010, 0x0020


def _handle(window):
    """Das Fenster-Handle, an dem die Titelleiste haengt.

    ⚠ `winfo_id()` liefert bei Tk den ZEICHENBEREICH, nicht den Rahmen. Die
    Titelleiste gehoert dem Elternfenster — ohne `GetParent` faerbt man ein
    Fenster ohne Leiste, und der Aufruf meldet trotzdem Erfolg.
    """
    import ctypes
    return ctypes.windll.user32.GetParent(window.winfo_id())


def set_dark(window):
    """Die Titelleiste dieses Fensters dunkel stellen. Sagt, ob es klappte."""
    if not sys.platform.startswith('win'):
        return False
    try:
        import ctypes
        from ctypes import wintypes
        handle = _handle(window)
        if not handle:
            return False
        value = ctypes.c_int(1)
        for attribute in ATTRIBUTES:
            result = ctypes.windll.dwmapi.DwmSetWindowAttribute(
                wintypes.HWND(handle), ctypes.c_uint(attribute),
                ctypes.byref(value), ctypes.sizeof(value))
            if result == 0:
                return True
    except Exception:
        pass                          # helle Leiste ist haesslich, nicht schlimm
    return False


def redraw_frame(window):
    """Windows zwingen, den Fensterrahmen neu zu zeichnen.

    ⚠⚠ **Ohne das bleibt die Leiste weiss.** Genau daran ist v3.6.0
    gescheitert: `DwmSetWindowAttribute` meldete Erfolg, die Einstellung stand
    auch — aber Windows zeichnet einen Rahmen, der bereits auf dem Bildschirm
    ist, nicht von selbst neu. Am 31.08.2026 gemeldet: „Meine Leiste ist
    weiss", mit Bildschirmfoto einer frisch gebauten 3.6.0.

    ⚠ **Und vorher geht es nicht.** Naheliegend waere, die Einstellung schon
    beim Bauen des Fensters zu setzen, bevor es je gezeichnet wurde — dann
    braeuchte es kein Neuzeichnen. Gemessen: Zu diesem Zeitpunkt gibt es das
    Fenster-Handle noch nicht. Es bleibt also bei diesem Weg.
    """
    if not sys.platform.startswith('win'):
        return False
    try:
        import ctypes
        handle = _handle(window)
        if not handle:
            return False
        ctypes.windll.user32.SetWindowPos(
            ctypes.c_void_p(handle), None, 0, 0, 0, 0,
            SWP_NOSIZE | SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE
            | SWP_FRAMECHANGED)
        return True
    except Exception:
        return False


def install():
    """Jedes Fenster des Programms bekommt die dunkle Leiste — auch kuenftige.

    ⚠⚠ **Eine Stelle statt sieben.** Das Programm baut an sieben Orten echte
    Fenster (Hauptfenster, Bauplan-Liste, Einstellungen, Assistent, Versionen,
    Dialoge, Wurzel). Sie alle einzeln anzufassen hiesse: Das achte wird
    vergessen, und dann sitzt wieder eine weisse Leiste mitten im dunklen
    Programm. Derselbe Weg wie in `tools/unsichtbar.py`.

    ⚠ **Gehaengt wird an `<Map>`, nicht an den Bau.** Vor dem ersten Anzeigen
    gibt es noch kein Fensterhandle — der Aufruf liefe ins Leere und meldete
    trotzdem Erfolg.
    """
    if not sys.platform.startswith('win'):
        return False
    try:
        import tkinter as tk
    except ImportError:
        return False

    for cls in (tk.Tk, tk.Toplevel):
        if getattr(cls, '_scbp_dark_titlebar', False):
            continue
        original = cls.__init__

        def build(self, *a, _original=original, **k):
            _original(self, *a, **k)
            try:
                # ⚠⚠ **Erst beim Anzeigen, nicht schon hier.** Naheliegend
                # waere, die Einstellung gleich beim Bauen zu setzen — dann
                # zeichnete Windows die Leiste von Anfang an richtig. Gemessen
                # am 31.08.2026: **Zu diesem Zeitpunkt gibt es das Handle noch
                # gar nicht** (`GetParent` liefert 0), der Aufruf ginge ins
                # Leere und meldete das nicht einmal. Also nur `<Map>` — und
                # dort dann mit erzwungenem Neuzeichnen.
                self.bind('<Map>', lambda _e, w=self: _once(w), add='+')
            except Exception:
                pass

        cls.__init__ = build
        cls._scbp_dark_titlebar = True
    return True


RETRIES = 10                     # Versuche, danach bis zum naechsten <Map> ruhen
RETRY_MS = 50                    # Abstand dazwischen


def _once(window, attempt=0):
    """Beim ersten Anzeigen faerben — danach nie wieder.

    ⚠ `<Map>` feuert bei jedem Wiederherstellen aus der Taskleiste. Ohne
    Merker liefe das Neuzeichnen dutzendfach — jedes Mal, wenn jemand das
    Fenster aus der Taskleiste holt.

    ⚠⚠ **Der Merker wird erst gesetzt, wenn es GEKLAPPT hat.** Bis zum
    02.09.2026 stand er eine Zeile zu frueh — vor dem Versuch. Lieferte
    `GetParent` in diesem Moment noch 0 (das Fenster war gemappt, der Rahmen
    aber noch nicht fertig), gab `set_dark()` False zurueck, und das Fenster galt
    trotzdem als erledigt: **fuer immer helle Leiste, ohne einen zweiten
    Versuch.** Es war ein Wettlauf, deshalb sah es zufaellig aus — Overlay und
    versteckte Fenster gewannen ihn, das jedes Mal neu gebaute Hauptfenster
    verlor ihn. Gemessen am laufenden Programm: drei Fenster mit gesetztem
    Attribut, das sichtbare Hauptfenster mit `DWMWA_USE_IMMERSIVE_DARK_MODE`
    auf **0**. Setzen von Hand liess die Leiste sofort umschlagen — das Setzen
    genuegt also, es wurde nur nie ausgefuehrt.

    ⚠ **Und bei Misserfolg wird der Merker NICHT gesetzt.** Dann versucht es
    das naechste `<Map>` erneut (Wiederherstellen aus der Taskleiste). Ein
    Fenster, das nie faerbbar ist, kostet dadurch `RETRIES` erfolglose
    Aufrufe pro Anzeigen — ein paar Millisekunden, und dafuer heilt sich der
    Fall selbst, statt dauerhaft hell zu bleiben.
    """
    try:
        if getattr(window, '_scbp_titlebar_set', False):
            return
        if set_dark(window):
            window._scbp_titlebar_set = True
            redraw_frame(window)
            return
        if attempt + 1 < RETRIES:
            window.after(RETRY_MS,
                         lambda: _once(window, attempt + 1))
    except Exception:
        pass
