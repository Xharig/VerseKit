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
Einen kleinen Bildschirmausschnitt holen — für den Signatur-Scanner.

| System | Weg | Stand |
|---|---|---|
| Windows | GDI (`BitBlt`) über `ctypes` | gebaut 17.09.2026 |
| Linux | Wayland-Portal | folgt — siehe Vorhaben-Notiz |

⚠⚠ **Physische Bildpunkte, nicht die logischen von Tk.** VerseKit läuft ohne
DPI-Kennung; Windows rechnet ihm bei 125 % alles herunter. Ein Abgriff in dieser
Rechnung liefert ein **gestauchtes, verwaschenes** Bild — bei Ziffern von neun
Punkten Breite ist das das Ende jeder Erkennung. Deshalb schaltet der
abgreifende Faden sich für die Dauer des Aufrufs auf „DPI-bewusst"
(`SetThreadDpiAwarenessContext`) — ⚠ nur in Arbeitsfäden, nie im Tk-Faden
(siehe `dpi_scale`).

⚠ **Das Scan-Fenster selbst fotografiert sich nicht mit.** Es ist ein
geschichtetes Fenster (`-transparentcolor`), und `BitBlt` ohne `CAPTUREBLT`
lässt geschichtete Fenster aus. Genau deshalb fehlt der Schalter hier.

Rückgabe ist ein Graustufenraster: Liste von Zeilen, je Zeile eine Liste von
0–255. Graustufe = **hellster** der drei Farbkanäle, nicht die Luminanz — die
HUD-Schrift ist farbig, und über die Luminanz verlöre ein türkiser Strich
gegen den dunklen Grund ein Drittel seines Abstands.
"""
import ctypes
import sys
import threading

# Kennung für „pro Bildschirm DPI-bewusst, Fassung 2" (Windows 10 1703+).
_PER_MONITOR_AWARE_V2 = -4
_SRCCOPY = 0x00CC0020


class GrabError(Exception):
    """Abgriff nicht möglich — `reason` ist ein Kennwort für die Oberfläche."""

    def __init__(self, reason):
        Exception.__init__(self, reason)
        self.reason = reason


def supported():
    """Kann dieses System überhaupt abgreifen?"""
    return sys.platform == 'win32'


class _Aware(object):
    """Für die Dauer eines `with` physische Bildpunkte rechnen (nur dieser Faden)."""

    def __enter__(self):
        self.before = None
        try:
            user32 = ctypes.windll.user32
            fn = user32.SetThreadDpiAwarenessContext
            fn.restype = ctypes.c_void_p
            fn.argtypes = [ctypes.c_void_p]
            self.before = fn(ctypes.c_void_p(_PER_MONITOR_AWARE_V2))
        except Exception:
            self.before = None      # älteres Windows: dann eben logisch
        return self

    def __exit__(self, *_exc):
        if self.before:
            try:
                fn = ctypes.windll.user32.SetThreadDpiAwarenessContext
                fn(ctypes.c_void_p(self.before))
            except Exception:
                pass
        return False


class _BITMAPINFOHEADER(ctypes.Structure):
    _fields_ = [('biSize', ctypes.c_uint32), ('biWidth', ctypes.c_int32),
                ('biHeight', ctypes.c_int32), ('biPlanes', ctypes.c_uint16),
                ('biBitCount', ctypes.c_uint16),
                ('biCompression', ctypes.c_uint32),
                ('biSizeImage', ctypes.c_uint32),
                ('biXPelsPerMeter', ctypes.c_int32),
                ('biYPelsPerMeter', ctypes.c_int32),
                ('biClrUsed', ctypes.c_uint32),
                ('biClrImportant', ctypes.c_uint32)]


_scale_cache = []


def dpi_scale():
    """Physische Bildpunkte je logischem Punkt (125 % → 1,25), einmal gemessen.

    ⚠⚠ **In einem eigenen Faden gemessen, nie im Tk-Faden.** Im RC 1 schaltete
    das Scan-Fenster den Tk-Faden selbst auf „DPI-bewusst", um die Lage zu
    lesen — die gemischten Rechnungen ließen das Fenster beim Ziehen springen
    und nach dem Speichern unten rechts versetzt wieder aufgehen (gemeldet
    17.09.2026). Jetzt rechnet Tk nur logisch, und umgerechnet wird an genau
    einer Stelle mit diesem Faktor.
    """
    if _scale_cache:
        return _scale_cache[0]
    result = [1.0]

    def measure():
        try:
            user32 = ctypes.windll.user32
            logical = user32.GetSystemMetrics(0)
            with _Aware():
                physical = user32.GetSystemMetrics(0)
            if logical > 0 and physical > 0:
                result[0] = physical / float(logical)
        except Exception:
            pass

    if supported():
        worker = threading.Thread(target=measure, daemon=True)
        worker.start()
        worker.join(2.0)
    _scale_cache.append(result[0])
    return result[0]


def to_physical(rect, scale=None):
    """(links, oben, breite, höhe) von logisch (Tk) nach physisch."""
    scale = dpi_scale() if scale is None else scale
    return tuple(int(round(v * scale)) for v in rect)


def to_logical(rect, scale=None):
    """(links, oben, breite, höhe) von physisch nach logisch (Tk)."""
    scale = dpi_scale() if scale is None else scale
    return tuple(int(round(v / scale)) for v in rect)


def widget_rect(widget):
    """Die Fläche eines Tk-Elements in **physischen** Punkten — oder None."""
    try:
        width, height = widget.winfo_width(), widget.winfo_height()
        if width <= 1 or height <= 1:
            return None
        return to_physical((widget.winfo_rootx(), widget.winfo_rooty(),
                            width, height))
    except Exception:
        return None


def grab(left, top, width, height):
    """Den Ausschnitt holen — Graustufenraster (Liste von Zeilen).

    Wirft `GrabError`, wenn es nicht geht.
    """
    if not supported():
        raise GrabError('nicht_unterstuetzt')
    width, height = int(width), int(height)
    if width <= 0 or height <= 0 or width * height > 4000000:
        raise GrabError('bereich_ungueltig')
    user32 = ctypes.windll.user32
    gdi32 = ctypes.windll.gdi32
    for fn, res, args in (
            (user32.GetDC, ctypes.c_void_p, [ctypes.c_void_p]),
            (user32.ReleaseDC, ctypes.c_int, [ctypes.c_void_p, ctypes.c_void_p]),
            (gdi32.CreateCompatibleDC, ctypes.c_void_p, [ctypes.c_void_p]),
            (gdi32.DeleteDC, ctypes.c_int, [ctypes.c_void_p]),
            (gdi32.SelectObject, ctypes.c_void_p, [ctypes.c_void_p, ctypes.c_void_p]),
            (gdi32.DeleteObject, ctypes.c_int, [ctypes.c_void_p]),
            (gdi32.CreateDIBSection, ctypes.c_void_p,
             [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_uint,
              ctypes.POINTER(ctypes.c_void_p), ctypes.c_void_p, ctypes.c_uint32]),
            (gdi32.BitBlt, ctypes.c_int,
             [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int,
              ctypes.c_int, ctypes.c_void_p, ctypes.c_int, ctypes.c_int,
              ctypes.c_uint32])):
        fn.restype, fn.argtypes = res, args

    with _Aware():
        screen = user32.GetDC(None)
        if not screen:
            raise GrabError('abgriff_fehlgeschlagen')
        memory = bitmap = old = None
        try:
            memory = gdi32.CreateCompatibleDC(screen)
            info = _BITMAPINFOHEADER()
            info.biSize = ctypes.sizeof(_BITMAPINFOHEADER)
            info.biWidth = width
            info.biHeight = -height          # oben beginnend
            info.biPlanes = 1
            info.biBitCount = 32
            bits = ctypes.c_void_p()
            bitmap = gdi32.CreateDIBSection(memory, ctypes.byref(info), 0,
                                            ctypes.byref(bits), None, 0)
            if not memory or not bitmap or not bits.value:
                raise GrabError('abgriff_fehlgeschlagen')
            old = gdi32.SelectObject(memory, bitmap)
            if not gdi32.BitBlt(memory, 0, 0, width, height, screen,
                                int(left), int(top), _SRCCOPY):
                raise GrabError('abgriff_fehlgeschlagen')
            raw = ctypes.string_at(bits.value, width * height * 4)
        finally:
            if old:
                gdi32.SelectObject(memory, old)
            if bitmap:
                gdi32.DeleteObject(bitmap)
            if memory:
                gdi32.DeleteDC(memory)
            user32.ReleaseDC(None, screen)
    return to_gray(raw, width, height)


def to_gray(raw, width, height):
    """BGRA-Bytes → Graustufenraster, Wert = hellster Kanal."""
    rows = []
    for y in range(height):
        start = y * width * 4
        line = raw[start:start + width * 4]
        rows.append([max(line[i], line[i + 1], line[i + 2])
                     for i in range(0, width * 4, 4)])
    return rows


def foreground_is_game():
    """Ist Star Citizen gerade das aktive Fenster? (Windows)

    ⚠ Gelesen wird nur, solange das Spiel vorn ist: Sonst fotografiert der
    Scanner den Desktop, und ein Browser mit einer Zahl darin wäre ein Treffer.
    """
    if not supported():
        return False
    try:
        from ctypes import wintypes
        user32 = ctypes.windll.user32
        kernel32 = ctypes.windll.kernel32
        user32.GetForegroundWindow.restype = ctypes.c_void_p
        hwnd = user32.GetForegroundWindow()
        if not hwnd:
            return False
        pid = wintypes.DWORD(0)
        user32.GetWindowThreadProcessId.argtypes = [ctypes.c_void_p,
                                                    ctypes.POINTER(wintypes.DWORD)]
        user32.GetWindowThreadProcessId(ctypes.c_void_p(hwnd), ctypes.byref(pid))
        kernel32.OpenProcess.restype = ctypes.c_void_p
        handle = kernel32.OpenProcess(0x1000, False, pid.value)
        if not handle:
            return False
        try:
            buffer = ctypes.create_unicode_buffer(1024)
            size = wintypes.DWORD(len(buffer))
            kernel32.QueryFullProcessImageNameW.argtypes = [
                ctypes.c_void_p, wintypes.DWORD, wintypes.LPWSTR,
                ctypes.POINTER(wintypes.DWORD)]
            if not kernel32.QueryFullProcessImageNameW(
                    ctypes.c_void_p(handle), 0, buffer, ctypes.byref(size)):
                return False
            return buffer.value.replace('/', '\\').split('\\')[-1].lower() \
                == 'starcitizen.exe'
        finally:
            kernel32.CloseHandle.argtypes = [ctypes.c_void_p]
            kernel32.CloseHandle(ctypes.c_void_p(handle))
    except Exception:
        return False
