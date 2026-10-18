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
"""Anti-AFK: drückt in festem Abstand Linke Alt + X (Visier abwischen).

Nur unter Windows, nur solange Star Citizen das Vordergrundfenster ist, und
nur, wenn seit dem Abstand weder Tastatur noch Maus benutzt wurde. Gesendet
wird mit `SendInput` aus `user32.dll` über ctypes, als Scancodes — ohne
Tastatur-Hook, ohne Zusatzpakete, ohne eigenen Prozess.

Der Schalter steht nach jedem Programmstart auf aus und wird nicht
gespeichert.

`tick()` ruft die Oberfläche alle `TICK_MS` Millisekunden; dort wird
geprüft und gesendet, nichts läuft in einem eigenen Faden.
"""
import ctypes
import random
import sys
import time

GAME_EXE = 'starcitizen.exe'

INTERVAL = 300          # Sekunden ohne Eingabe, bis gedrückt wird
JITTER = 0              # zufällige Streuung um den Abstand, ± Sekunden
TICK_MS = 5000          # Takt, in dem die Oberfläche `tick()` ruft
HOLD = 0.08             # Sekunden zwischen Drücken und Loslassen

SCAN_LEFT_ALT = 0x38
SCAN_X = 0x2D
KEYS = (SCAN_LEFT_ALT, SCAN_X)

INPUT_KEYBOARD = 1
KEYEVENTF_KEYUP = 0x0002
KEYEVENTF_SCANCODE = 0x0008
PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
TOKEN_QUERY = 0x0008
TOKEN_ELEVATION = 20

# Nach einem Senden muss die letzte Eingabe jünger sein als das —
# sonst kam der Tastendruck nicht an.
ARRIVED_WITHIN = 2.0


class SendError(Exception):
    """`SendInput` hat nicht alle Ereignisse eingespeist."""


def supported():
    return sys.platform == 'win32'


# --------------------------------------------------------------- Windows-API

# Feste Breiten wie in den Windows-Kopfdateien; so lassen sich die Strukturen
# auch außerhalb Windows bauen und vermessen.
_ULONG_PTR = ctypes.c_size_t
_DWORD = ctypes.c_uint32
_WORD = ctypes.c_uint16
_LONG = ctypes.c_int32


class _KeybdInput(ctypes.Structure):
    _fields_ = [('wVk', _WORD), ('wScan', _WORD), ('dwFlags', _DWORD),
                ('time', _DWORD), ('dwExtraInfo', _ULONG_PTR)]


class _MouseInput(ctypes.Structure):
    _fields_ = [('dx', _LONG), ('dy', _LONG), ('mouseData', _DWORD),
                ('dwFlags', _DWORD), ('time', _DWORD),
                ('dwExtraInfo', _ULONG_PTR)]


class _HardwareInput(ctypes.Structure):
    _fields_ = [('uMsg', _DWORD), ('wParamL', _WORD), ('wParamH', _WORD)]


class _InputUnion(ctypes.Union):
    _fields_ = [('mi', _MouseInput), ('ki', _KeybdInput),
                ('hi', _HardwareInput)]


class _Input(ctypes.Structure):
    _anonymous_ = ('u',)
    _fields_ = [('type', _DWORD), ('u', _InputUnion)]


class _LastInputInfo(ctypes.Structure):
    _fields_ = [('cbSize', ctypes.c_uint), ('dwTime', _DWORD)]


class _Api:
    """Eigene Instanzen von user32/kernel32/advapi32 mit festen Signaturen.

    Eigene `WinDLL`-Objekte statt `ctypes.windll.*`: Andere Module setzen
    dort `argtypes` für dieselben Funktionen, und die gelten prozessweit.
    """

    def __init__(self):
        from ctypes import wintypes as w
        self.user32 = ctypes.WinDLL('user32', use_last_error=True)
        self.kernel32 = ctypes.WinDLL('kernel32', use_last_error=True)
        self.advapi32 = ctypes.WinDLL('advapi32', use_last_error=True)
        u, k, a = self.user32, self.kernel32, self.advapi32
        u.SendInput.argtypes = (ctypes.c_uint, ctypes.POINTER(_Input),
                                ctypes.c_int)
        u.SendInput.restype = ctypes.c_uint
        u.GetForegroundWindow.argtypes = ()
        u.GetForegroundWindow.restype = w.HWND
        u.GetWindowThreadProcessId.argtypes = (w.HWND, ctypes.POINTER(w.DWORD))
        u.GetWindowThreadProcessId.restype = w.DWORD
        u.GetLastInputInfo.argtypes = (ctypes.POINTER(_LastInputInfo),)
        u.GetLastInputInfo.restype = w.BOOL
        k.GetTickCount.argtypes = ()
        k.GetTickCount.restype = w.DWORD
        k.OpenProcess.argtypes = (w.DWORD, w.BOOL, w.DWORD)
        k.OpenProcess.restype = w.HANDLE
        k.CloseHandle.argtypes = (w.HANDLE,)
        k.CloseHandle.restype = w.BOOL
        k.GetCurrentProcess.argtypes = ()
        k.GetCurrentProcess.restype = w.HANDLE
        k.QueryFullProcessImageNameW.argtypes = (
            w.HANDLE, w.DWORD, w.LPWSTR, ctypes.POINTER(w.DWORD))
        k.QueryFullProcessImageNameW.restype = w.BOOL
        a.OpenProcessToken.argtypes = (w.HANDLE, w.DWORD,
                                       ctypes.POINTER(w.HANDLE))
        a.OpenProcessToken.restype = w.BOOL
        a.GetTokenInformation.argtypes = (w.HANDLE, ctypes.c_int,
                                          ctypes.c_void_p, w.DWORD,
                                          ctypes.POINTER(w.DWORD))
        a.GetTokenInformation.restype = w.BOOL


_api = None


def _get_api():
    """Die Windows-API, beim ersten Gebrauch geladen; None außerhalb Windows."""
    global _api
    if not supported():
        return None
    if _api is None:
        _api = _Api()
    return _api


def _foreground_pid():
    api = _get_api()
    if api is None:
        return 0
    hwnd = api.user32.GetForegroundWindow()
    if not hwnd:
        return 0
    from ctypes import wintypes
    pid = wintypes.DWORD(0)
    api.user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
    return pid.value


def _process_name(pid):
    api = _get_api()
    if api is None or not pid:
        return ''
    handle = api.kernel32.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,
                                      False, pid)
    if not handle:
        return ''
    try:
        from ctypes import wintypes
        buffer = ctypes.create_unicode_buffer(1024)
        size = wintypes.DWORD(len(buffer))
        if not api.kernel32.QueryFullProcessImageNameW(
                handle, 0, buffer, ctypes.byref(size)):
            return ''
        return buffer.value.replace('/', '\\').split('\\')[-1].lower()
    finally:
        api.kernel32.CloseHandle(handle)


def game_in_front():
    """Gehört das Vordergrundfenster zu `StarCitizen.exe`?"""
    if not supported():
        return False
    try:
        return _process_name(_foreground_pid()) == GAME_EXE
    except Exception:
        return False


def idle_seconds():
    """Sekunden seit der letzten Tastatur- oder Mauseingabe — oder None."""
    api = _get_api()
    if api is None:
        return None
    info = _LastInputInfo()
    info.cbSize = ctypes.sizeof(_LastInputInfo)
    if not api.user32.GetLastInputInfo(ctypes.byref(info)):
        return None
    return ((api.kernel32.GetTickCount() - info.dwTime) & 0xFFFFFFFF) / 1000.0


def _elevated(handle):
    """Läuft der Prozess hinter `handle` mit Administratorrechten? None = unklar."""
    from ctypes import wintypes
    api = _get_api()
    if api is None:
        return None
    token = wintypes.HANDLE()
    if not api.advapi32.OpenProcessToken(handle, TOKEN_QUERY,
                                         ctypes.byref(token)):
        return None
    try:
        value = wintypes.DWORD(0)
        size = wintypes.DWORD(0)
        if not api.advapi32.GetTokenInformation(
                token, TOKEN_ELEVATION, ctypes.byref(value),
                ctypes.sizeof(value), ctypes.byref(size)):
            return None
        return bool(value.value)
    finally:
        api.kernel32.CloseHandle(token)


def game_needs_admin():
    """Läuft das Spiel im Vordergrund erhöht, dieses Programm aber nicht?

    Dann verwirft Windows jede gesendete Taste (UIPI), ohne dass `SendInput`
    einen Fehler meldet. None, wenn sich das nicht feststellen lässt.
    """
    api = _get_api()
    if api is None:
        return None
    try:
        own = _elevated(api.kernel32.GetCurrentProcess())
        handle = api.kernel32.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,
                                          False, _foreground_pid())
        if not handle:
            return None
        try:
            game = _elevated(handle)
        finally:
            api.kernel32.CloseHandle(handle)
    except Exception:
        return None
    if own is None or game is None:
        return None
    return game and not own


def _key_events(scancodes, up):
    events = (_Input * len(scancodes))()
    order = reversed(scancodes) if up else scancodes
    for slot, code in zip(events, order):
        slot.type = INPUT_KEYBOARD
        slot.ki.wVk = 0
        slot.ki.wScan = code
        slot.ki.dwFlags = KEYEVENTF_SCANCODE | (KEYEVENTF_KEYUP if up else 0)
    return events


def _send(events):
    api = _get_api()
    if api is None:
        raise SendError('no api')
    count = len(events)
    sent = api.user32.SendInput(count, events, ctypes.sizeof(_Input))
    if sent != count:
        raise SendError('SendInput %d/%d, Fehler %d'
                        % (sent, count, ctypes.get_last_error()))


def press(scancodes=KEYS):
    """Tasten der Reihe nach drücken, `HOLD` halten, rückwärts loslassen.

    Das Loslassen läuft auch, wenn das Drücken scheitert — eine hängende
    Alt-Taste bliebe sonst im System stehen.
    """
    try:
        _send(_key_events(scancodes, up=False))
        time.sleep(HOLD)
    finally:
        _send(_key_events(scancodes, up=True))


# ------------------------------------------------------------------ Zustand

_state = {'on': False, 'threshold': INTERVAL, 'last_sent': None,
          'count': 0, 'error': None}
_switch_listeners = []


def _new_threshold():
    return INTERVAL + random.uniform(-JITTER, JITTER)


def enabled():
    return _state['on']


def last_error():
    """Textschlüssel des letzten Abbruchs — oder None."""
    return _state['error']


def sent_count():
    return _state['count']


def on_switch(callback):
    """`callback(an)` nach jedem Umschalten — für alle Anzeigen des Schalters."""
    if callback not in _switch_listeners:
        _switch_listeners.append(callback)


def _notify():
    for callback in list(_switch_listeners):
        try:
            callback(_state['on'])
        except Exception:
            # Eine Anzeige, deren Fenster zu ist, fliegt heraus.
            _switch_listeners.remove(callback)


def set_enabled(on):
    """Ein- oder ausschalten — der eine Weg für Seite und Overlay.

    Außerhalb Windows bleibt der Schalter aus. Gibt den neuen Zustand.
    """
    on = bool(on) and supported()
    _state['on'] = on
    _state['last_sent'] = None
    _state['threshold'] = _new_threshold()
    if on:
        _state['error'] = None
    _notify()
    return on


def _fail(key):
    _state['error'] = key
    _state['on'] = False
    _notify()


def tick(now=None):
    """Einmal nachsehen und, wenn alles passt, die Tasten drücken.

    Gibt zurück, was geschah: `'off'`, `'unsupported'`, `'not_in_front'`,
    `'active'`, `'too_soon'`, `'sent'` oder `'blocked'`/`'failed'` (dann ist
    der Schalter aus und `last_error()` sagt den Grund).
    """
    if not _state['on']:
        return 'off'
    if not supported():
        _state['on'] = False
        return 'unsupported'
    now = time.monotonic() if now is None else now
    if not game_in_front():
        return 'not_in_front'
    idle = idle_seconds()
    if idle is None or idle < _state['threshold']:
        return 'active'
    last = _state['last_sent']
    if last is not None and now - last < _state['threshold']:
        return 'too_soon'
    if game_needs_admin():
        _fail('aafk_err_admin')
        return 'blocked'
    try:
        press()
    except Exception:
        _fail('aafk_err_send')
        return 'failed'
    after = idle_seconds()
    if after is not None and after >= ARRIVED_WITHIN:
        _fail('aafk_err_admin')
        return 'blocked'
    _state['last_sent'] = now
    _state['count'] += 1
    _state['threshold'] = _new_threshold()
    return 'sent'
