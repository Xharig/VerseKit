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
Eine Tastenkombination, die auch im Spiel greift.

Star Citizen laeuft im Vollbild und blendet den Mauszeiger aus. Wer nachsehen
will, ob er einen Bauplan schon hat, muss heraustabben und das Fenster dann
**blind** suchen und anklicken. Am 31.08.2026 als Nutzerwunsch gemeldet:
„Hotkey um die Bauplanliste aufzurufen, da man in SC erst raustabben muss und
die Maus ueber dem SC-Fenster nicht sichtbar ist."

## Was auf welchem System geht

| System | Weg | greift im Spiel |
|---|---|---|
| Windows | `RegisterHotKey` (user32) | ja |
| Linux, X11 | `XGrabKey` (libX11) | ja |
| Linux, Wayland | **gar nicht von innen** | — |

⚠⚠ **Wayland ist keine Faulheit, sondern Absicht des Systems.** Ein Programm
darf dort nicht mithoeren, was in einem anderen Fenster getippt wird — genau
das braeuchte ein globaler Hotkey. Der Weg dorthin fuehrt ueber die
Tastenkombinationen des Schreibtischs; `grund()` sagt das, und die
Einstellungsseite zeigt den fertigen Befehl zum Hinterlegen. Der Doppelstart
holt das laufende Programm schon heute nach vorn (siehe `overlay.zeigen_bitte`).

⚠ **Frueher stand hier: „unter Windows fragil".** Das galt fuer
Tastatur-Haken (`SetWindowsHookEx`), die tief im System sitzen und von
Virenwaechtern angefasst werden. `RegisterHotKey` ist etwas anderes: die dafuer
vorgesehene Schnittstelle, seit Windows 95 unveraendert. Sie kann nur eines
nicht — eine Kombination belegen, die schon jemand anders hat. Dann sagt sie
das, und wir geben es weiter, statt so zu tun, als laege es an uns.

## Gelesen wird nicht mitgehoert

⚠⚠ Es wird **eine** Kombination angemeldet, und das System weckt uns nur bei
genau dieser. Alles andere sieht das Programm nie — kein Mitschreiben, kein
Zugriff auf das, was im Spiel getippt wird. Das ist der Unterschied zwischen
`RegisterHotKey` und einem Tastatur-Haken, und er ist der Grund, warum hier
nur der erste Weg in Frage kam.
"""
import os
import sys
import threading

# Was sich kombinieren laesst. ⚠ Bewusst klein gehalten: Modifikatoren plus
# EINE gewoehnliche Taste. Wer eine Kombination aus drei Buchstaben zulaesst,
# baut sich Konflikte mit den Spiel-Belegungen, die niemand mehr findet.
MODIFIERS = {
    'strg': 'strg', 'ctrl': 'strg', 'control': 'strg',
    'alt': 'alt',
    'umschalt': 'umschalt', 'shift': 'umschalt',
}

DEFAULT = 'Strg+Alt+B'          # B wie Bauplan


def parse(kombination):
    """`"Strg+Alt+B"` -> `({'strg', 'alt'}, 'B')`. Bei Unsinn: `(None, None)`.

    ⚠ Grosz/klein und Leerzeichen sind egal — die Kombination steht in einer
    Einstellungsdatei, die auch von Hand bearbeitet wird.
    """
    if not kombination:
        return None, None
    mods, taste = set(), None
    for teil in str(kombination).replace('-', '+').split('+'):
        teil = teil.strip()
        if not teil:
            continue
        klein = teil.lower()
        if klein in MODIFIERS:
            mods.add(MODIFIERS[klein])
            continue
        if taste is not None:
            return None, None            # zwei gewoehnliche Tasten
        taste = teil.upper()
    if not taste:
        return None, None
    if not (len(taste) == 1 and (taste.isalpha() or taste.isdigit())) and             not (taste.startswith('F') and taste[1:].isdigit()
                 and 1 <= int(taste[1:]) <= 12):
        return None, None
    # ⚠⚠ **Ohne Modifikator nicht.** Eine nackte Taste global zu belegen heisst,
    # sie im Spiel unbrauchbar zu machen — und der Nutzer sucht den Grund dann
    # ueberall, nur nicht hier.
    if not mods:
        return None, None
    return mods, taste


def _wayland():
    return (os.environ.get('XDG_SESSION_TYPE', '').lower() == 'wayland'
            or (os.environ.get('WAYLAND_DISPLAY') and not os.environ.get('DISPLAY')))


def possible():
    """Geht ein echter Hotkey auf diesem System? Gibt `(ja, grund)` zurueck."""
    if sys.platform.startswith('win'):
        return True, ''
    if sys.platform.startswith('linux'):
        if _wayland():
            return False, 'wayland'
        if os.environ.get('DISPLAY'):
            return True, ''
        return False, 'kein_bildschirm'
    return False, 'system'


# ---------------------------------------------------------------------------
# Windows
# ---------------------------------------------------------------------------
#
# ⚠⚠ **Ein EIGENER Faden mit eigener Nachrichtenschlange — nicht der Tk-Faden.**
# `RegisterHotKey(None, ...)` haengt die Kombination an den Faden, der anmeldet;
# der Druck kommt dann als **Faden-Nachricht** in dessen Schlange an. Genau das
# war bis v3.8.0 der Tk-Faden — und dort kam nie etwas an: Tk pumpt seine
# Schlange selbst mit `PeekMessage(NULL, 0, 0, PM_REMOVE)` und nimmt dabei
# ALLES heraus, auch Faden-Nachrichten. Eine Faden-Nachricht hat kein Fenster,
# also kann `DispatchMessage` sie niemandem zustellen — sie ist danach weg.
# Wer wie wir 300 ms spaeter nachsieht, findet eine leere Schlange.
#
# ⚠ Gemessen am 31.08.2026, weil „geht bei mir nicht" allein keine Ursache ist:
# dreimal `WM_HOTKEY` an den eigenen Faden geschickt, ohne Tk **3 von 3**
# angekommen, mit laufendem Tk **0 von 3**. Das ist der ganze Fehler.
#
# Deshalb meldet jetzt ein eigener Faden an und wartet dort mit `GetMessage`
# auf **seiner** Schlange, die ihm niemand leerraeumt. Er setzt nur eine Fahne;
# `poll()` nimmt sie im Tk-Takt herunter. Am Wesentlichen aendert das
# nichts: Es kommt weiterhin ausschliesslich die eine angemeldete Kombination
# an, mitgelesen wird nichts.
WM_HOTKEY = 0x0312
WM_QUIT = 0x0012
PM_NOREMOVE = 0x0000
MOD_ALT, MOD_CONTROL, MOD_SHIFT = 0x0001, 0x0002, 0x0004
MOD_NOREPEAT = 0x4000            # nicht dauerfeuern, solange man haelt
HOTKEY_ID = 0xB9CB                 # irgendeine Zahl, nur fuer uns


def _vk(taste):
    """Der Tastencode von Windows fuer unsere kleine Auswahl."""
    if len(taste) == 1 and (taste.isalpha() or taste.isdigit()):
        return ord(taste)
    if taste.startswith('F') and taste[1:].isdigit():
        return 0x70 + int(taste[1:]) - 1        # VK_F1 = 0x70
    return None


class _Windows:
    def __init__(self):
        self.angemeldet = False
        self._faden = None
        self._tid = 0
        self._treffer = threading.Event()
        self._bereit = threading.Event()
        self._ergebnis = (False, 'faden')

    def register(self, mods, taste):
        flaggen = MOD_NOREPEAT
        flaggen |= MOD_CONTROL if 'strg' in mods else 0
        flaggen |= MOD_ALT if 'alt' in mods else 0
        flaggen |= MOD_SHIFT if 'umschalt' in mods else 0
        code = _vk(taste)
        if code is None:
            return False, 'taste'
        self.unregister()
        self._treffer.clear()
        self._bereit.clear()
        self._ergebnis = (False, 'faden')
        self._faden = threading.Thread(target=self._loop,
                                       args=(flaggen, code),
                                       name='sc-bp-hotkey', daemon=True)
        self._faden.start()
        # ⚠ Hier wird gewartet, und zwar mit Absicht: „belegt" ist eine
        # Auskunft, die der Nutzer sofort braucht, sonst steht er vor einem
        # Feld, das nichts sagt. `RegisterHotKey` antwortet in Millisekunden —
        # zwei Sekunden sind nur die Reissleine, damit ein haengender Faden
        # nicht den Start blockiert.
        self._bereit.wait(2.0)
        ok, warum = self._ergebnis
        self.angemeldet = ok
        return ok, warum

    def _loop(self, flaggen, code):
        """Der eigene Faden: anmelden, warten, aufraeumen.

        ⚠ Anmelden und Warten muessen im SELBEN Faden passieren — die
        Kombination haengt an ihm, nicht am Fenster.
        """
        import ctypes
        from ctypes import wintypes

        class MSG(ctypes.Structure):
            _fields_ = [('hwnd', wintypes.HWND), ('message', wintypes.UINT),
                        ('wParam', wintypes.WPARAM), ('lParam', wintypes.LPARAM),
                        ('time', wintypes.DWORD), ('pt', wintypes.POINT)]

        u32 = ctypes.windll.user32
        msg = MSG()
        try:
            self._tid = ctypes.windll.kernel32.GetCurrentThreadId()
            # ⚠ Die Schlange entsteht erst, wenn sie einmal angefasst wurde.
            # Ohne das geht ein `PostThreadMessage` von aussen ins Leere.
            u32.PeekMessageW(ctypes.byref(msg), None, 0, 0, PM_NOREMOVE)
            if not u32.RegisterHotKey(None, HOTKEY_ID, flaggen, code):
                # ⚠⚠ **Belegt heisst belegt.** Hat ein anderes Programm die
                # Kombination, gibt Windows sie nicht her — daran laesst sich
                # nichts drehen. Der Nutzer muss es erfahren, sonst sucht er
                # den Fehler bei sich.
                self._ergebnis = (False, 'belegt')
                return
            self._ergebnis = (True, '')
        except Exception:
            self._ergebnis = (False, 'system')
            return
        finally:
            self._bereit.set()

        try:
            while True:
                stand = u32.GetMessageW(ctypes.byref(msg), None, 0, 0)
                if stand == 0 or stand == -1:      # 0 = WM_QUIT, -1 = Fehler
                    break
                if msg.message == WM_HOTKEY and msg.wParam == HOTKEY_ID:
                    self._treffer.set()
        except Exception:
            pass
        finally:
            try:
                u32.UnregisterHotKey(None, HOTKEY_ID)
            except Exception:
                pass

    def unregister(self):
        faden, tid = self._faden, self._tid
        self._faden, self._tid = None, 0
        self.angemeldet = False
        if faden is None or not faden.is_alive():
            return
        try:
            import ctypes
            u32 = ctypes.windll.user32
            # `WM_QUIT` beendet das `GetMessage` des Fadens; er meldet die
            # Kombination selbst wieder ab, dort wo er sie angemeldet hat.
            u32.PostThreadMessageW(ctypes.c_uint(tid), ctypes.c_uint(WM_QUIT),
                                   ctypes.c_size_t(0), ctypes.c_ssize_t(0))
        except Exception:
            pass
        faden.join(1.0)

    def poll(self):
        """Wurde gedrueckt? Nimmt die Fahne herunter, die der Faden gesetzt hat.

        ⚠ Bewusst eine Fahne und keine Zaehlung: Zweimal schnell hintereinander
        gedrueckt soll das Fenster einmal nach vorn holen, nicht zweimal.
        """
        if not self.angemeldet:
            return False
        if self._treffer.is_set():
            self._treffer.clear()
            return True
        return False


# ---------------------------------------------------------------------------
# Linux, X11
# ---------------------------------------------------------------------------
#
# ⚠ **Eine EIGENE Verbindung zum Bildschirm, nicht die von Tk.** Tk verarbeitet
# seine Ereignisse selbst; wer sich in dieselbe Verbindung setzt, klaut ihm
# welche und das Fenster reagiert nicht mehr richtig. Eine zweite Verbindung
# kostet fast nichts und stoert niemanden.
SHIFT_MASK, LOCK_MASK, CONTROL_MASK, MOD1_MASK, MOD2_MASK = 1, 2, 4, 8, 16
KEY_PRESS = 2
GRAB_ASYNC = 1

# ⚠⚠ **Feststell- und Nummerntaste zaehlen als Modifikator mit.** Wer nur die
# reine Kombination anmeldet, bekommt sie nicht mehr, sobald jemand Num-Lock
# eingeschaltet hat — und das ist der Normalzustand an einer Tastatur mit
# Ziffernblock. Deshalb alle vier Spielarten anmelden.
EXTRA = (0, LOCK_MASK, MOD2_MASK, LOCK_MASK | MOD2_MASK)


class _X11:
    def __init__(self):
        self.lib = None
        self.anzeige = None
        self.wurzel = None
        self.gegriffen = []

    def _load(self):
        if self.lib is not None:
            return True
        import ctypes
        import ctypes.util
        pfad = ctypes.util.find_library('X11')
        if not pfad:
            return False
        self.lib = ctypes.cdll.LoadLibrary(pfad)
        self.lib.XOpenDisplay.restype = ctypes.c_void_p
        self.lib.XOpenDisplay.argtypes = [ctypes.c_char_p]
        self.lib.XDefaultRootWindow.restype = ctypes.c_ulong
        self.lib.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
        self.lib.XStringToKeysym.restype = ctypes.c_ulong
        self.lib.XStringToKeysym.argtypes = [ctypes.c_char_p]
        self.lib.XKeysymToKeycode.restype = ctypes.c_ubyte
        self.lib.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
        self.anzeige = self.lib.XOpenDisplay(None)
        if not self.anzeige:
            self.lib = None
            return False
        self.wurzel = self.lib.XDefaultRootWindow(self.anzeige)
        return True

    def register(self, mods, taste):
        import ctypes
        if not self._load():
            return False, 'kein_x11'
        maske = 0
        maske |= CONTROL_MASK if 'strg' in mods else 0
        maske |= MOD1_MASK if 'alt' in mods else 0
        maske |= SHIFT_MASK if 'umschalt' in mods else 0
        # X11 nennt die Tasten beim Namen: 'b', '5', 'F3'.
        name = taste.lower() if len(taste) == 1 else taste
        keysym = self.lib.XStringToKeysym(name.encode('ascii', 'ignore'))
        if not keysym:
            return False, 'taste'
        code = self.lib.XKeysymToKeycode(self.anzeige, keysym)
        if not code:
            return False, 'taste'
        self.unregister()
        for zusatz in EXTRA:
            self.lib.XGrabKey(ctypes.c_void_p(self.anzeige), ctypes.c_int(code),
                              ctypes.c_uint(maske | zusatz),
                              ctypes.c_ulong(self.wurzel), ctypes.c_int(1),
                              ctypes.c_int(GRAB_ASYNC), ctypes.c_int(GRAB_ASYNC))
            self.gegriffen.append((code, maske | zusatz))
        self.lib.XSync(ctypes.c_void_p(self.anzeige), ctypes.c_int(0))
        # ⚠ X11 sagt beim Greifen nicht, ob es geklappt hat — der Fehler kommt
        # asynchron. Wir melden Erfolg und lassen den Nutzer sehen, ob die
        # Taste wirkt; eine Falschmeldung „belegt" waere hier geraten.
        return True, ''

    def unregister(self):
        if not self.gegriffen or self.lib is None:
            self.gegriffen = []
            return
        try:
            import ctypes
            for code, maske in self.gegriffen:
                self.lib.XUngrabKey(ctypes.c_void_p(self.anzeige),
                                    ctypes.c_int(code), ctypes.c_uint(maske),
                                    ctypes.c_ulong(self.wurzel))
            self.lib.XSync(ctypes.c_void_p(self.anzeige), ctypes.c_int(0))
        except Exception:
            pass
        self.gegriffen = []

    def poll(self):
        if not self.gegriffen or self.lib is None:
            return False
        try:
            import ctypes
            puffer = (ctypes.c_byte * 224)()      # XEvent ist hoechstens so gross
            getroffen = False
            while self.lib.XPending(ctypes.c_void_p(self.anzeige)) > 0:
                self.lib.XNextEvent(ctypes.c_void_p(self.anzeige),
                                    ctypes.byref(puffer))
                if ctypes.cast(ctypes.byref(puffer),
                               ctypes.POINTER(ctypes.c_int))[0] == KEY_PRESS:
                    getroffen = True
            return getroffen
        except Exception:
            return False


# ---------------------------------------------------------------------------
# Die Wache — eine Stelle für beide Systeme
# ---------------------------------------------------------------------------


class Watch:
    """Meldet die Kombination an und sagt auf Nachfrage, ob gedrückt wurde.

    ⚠⚠ **Es wird NICHT mitgehört.** Angemeldet wird genau eine Kombination;
    alles andere sieht das Programm nie. Das ist der Unterschied zu einem
    Tastatur-Haken — und der Grund, warum hier nur dieser Weg in Frage kam.

    ⚠⚠ **Gewartet wird NEBEN Tk, gefragt wird im Tk-Takt.** Unter Windows
    landet der Druck in der Schlange genau des Fadens, der angemeldet hat —
    und wenn das der Tk-Faden ist, räumt Tk ihn selbst weg, bevor jemand
    nachsieht (gemessen am 31.08.2026: 0 von 3 kamen an). Deshalb hält ein
    eigener Faden die Stellung und setzt eine Fahne; `poll()` nimmt sie
    im selben Takt wie die übrige Warteschlange herunter.
    """

    def __init__(self):
        self.helper = None
        self.kombination = ''
        self.grund = ''

    def register(self, kombination):
        """Sagt `(ja, grund)`. `grund` ist ein Kürzel, kein fertiger Satz —
        die Oberfläche macht daraus einen Text in der richtigen Sprache."""
        self.unregister()
        # ⚠ **Erst die Eingabe, dann das System.** Umgekehrt kam die
        # Eingabeprüfung unter Wayland nie dran: Dort meldete `moeglich()`
        # sofort `wayland`, und eine unsinnige Kombination bekam dieselbe
        # Auskunft wie eine gültige. Der Nutzer las „geht unter Wayland
        # nicht", obwohl schon das Eingetippte keine Kombination war.
        # Unter Wayland aufgefallen, als Prüfung 100 dort rot lief.
        mods, taste = parse(kombination)
        if not mods:
            self.grund = 'kombination'
            return False, 'kombination'
        geht, warum = possible()
        if not geht:
            self.grund = warum
            return False, warum
        helper = _Windows() if sys.platform.startswith('win') else _X11()
        ok, warum = helper.register(mods, taste)
        if not ok:
            self.grund = warum
            return False, warum
        self.helper = helper
        self.kombination = kombination
        self.grund = ''
        return True, ''

    def unregister(self):
        if self.helper is not None:
            try:
                self.helper.unregister()
            except Exception:
                pass
        self.helper = None
        self.kombination = ''

    def poll(self):
        """Wurde die Kombination seit dem letzten Mal gedrückt?"""
        if self.helper is None:
            return False
        try:
            return bool(self.helper.poll())
        except Exception:
            return False
