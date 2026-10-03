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
Einen Befehl auf einem eigenen, unsichtbaren Windows-Desktop starten.

Das Gegenstück zu Xvfb unter Linux. Windows kann mehrere Desktops in derselben
Sitzung führen (`CreateDesktop`); angezeigt wird immer nur einer. Fenster auf
einem anderen Desktop sind echt — sie haben Größe und Lage, Rückrufe laufen —,
erreichen aber nie den Bildschirm und nie den Tastaturfokus dessen, der gerade
arbeitet oder spielt.

Achtung: Durchsichtige Fenster auf dem eigenen Desktop reichen dafür **nicht**: Sie
existieren dort wirklich, und Windows gibt ihnen beim Öffnen den Fokus.

Ein- und Ausgabe des Kindprozesses hängen an denen des Aufrufers — wer die
Ausgabe abfängt, bekommt sie wie gewohnt.

Nur Standardbibliothek (`ctypes`).
"""
import ctypes
import os
import sys

DESKTOP_NAME = 'VerseKit-Pruefung'

_GENERIC_ALL = 0x10000000
_STARTF_USESTDHANDLES = 0x00000100
_CREATE_UNICODE_ENVIRONMENT = 0x00000400
_INFINITE = 0xFFFFFFFF


def verfuegbar():
    """Gibt es den Weg hier überhaupt — also Windows mit `user32`?"""
    return sys.platform == 'win32'


def _bauteile():
    """Die Windows-Schnittstellen — erst beim Gebrauch geladen."""
    import msvcrt
    from ctypes import wintypes

    user32 = ctypes.WinDLL('user32', use_last_error=True)
    kernel32 = ctypes.WinDLL('kernel32', use_last_error=True)

    class STARTUPINFOW(ctypes.Structure):
        _fields_ = [('cb', wintypes.DWORD), ('lpReserved', wintypes.LPWSTR),
                    ('lpDesktop', wintypes.LPWSTR),
                    ('lpTitle', wintypes.LPWSTR),
                    ('dwX', wintypes.DWORD), ('dwY', wintypes.DWORD),
                    ('dwXSize', wintypes.DWORD), ('dwYSize', wintypes.DWORD),
                    ('dwXCountChars', wintypes.DWORD),
                    ('dwYCountChars', wintypes.DWORD),
                    ('dwFillAttribute', wintypes.DWORD),
                    ('dwFlags', wintypes.DWORD),
                    ('wShowWindow', wintypes.WORD),
                    ('cbReserved2', wintypes.WORD),
                    ('lpReserved2', ctypes.POINTER(ctypes.c_byte)),
                    ('hStdInput', wintypes.HANDLE),
                    ('hStdOutput', wintypes.HANDLE),
                    ('hStdError', wintypes.HANDLE)]

    class PROCESS_INFORMATION(ctypes.Structure):
        _fields_ = [('hProcess', wintypes.HANDLE),
                    ('hThread', wintypes.HANDLE),
                    ('dwProcessId', wintypes.DWORD),
                    ('dwThreadId', wintypes.DWORD)]

    user32.CreateDesktopW.restype = wintypes.HANDLE
    user32.CreateDesktopW.argtypes = [wintypes.LPCWSTR, wintypes.LPCWSTR,
                                      ctypes.c_void_p, wintypes.DWORD,
                                      wintypes.DWORD, ctypes.c_void_p]
    user32.CloseDesktop.argtypes = [wintypes.HANDLE]
    kernel32.CreateProcessW.argtypes = [
        wintypes.LPCWSTR, wintypes.LPWSTR, ctypes.c_void_p, ctypes.c_void_p,
        wintypes.BOOL, wintypes.DWORD, ctypes.c_void_p, wintypes.LPCWSTR,
        ctypes.POINTER(STARTUPINFOW), ctypes.POINTER(PROCESS_INFORMATION)]
    kernel32.WaitForSingleObject.argtypes = [wintypes.HANDLE, wintypes.DWORD]
    kernel32.GetExitCodeProcess.argtypes = [wintypes.HANDLE,
                                            ctypes.POINTER(wintypes.DWORD)]
    kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
    return (user32, kernel32, msvcrt, wintypes, STARTUPINFOW,
            PROCESS_INFORMATION)


def _befehlszeile(argv):
    """Die Argumente als eine Windows-Befehlszeile, richtig gequotet."""
    import subprocess
    return subprocess.list2cmdline(argv)


def starten(argv, env=None):
    """`argv` auf dem eigenen Desktop ausführen und den Rückgabewert liefern.

    Wirft `OSError`, wenn der Desktop oder der Prozess nicht entsteht — der
    Aufrufer fällt dann auf seinen bisherigen Weg zurück.
    """
    (user32, kernel32, msvcrt, wintypes, STARTUPINFOW,
     PROCESS_INFORMATION) = _bauteile()

    def griff(strom):
        try:
            h = msvcrt.get_osfhandle(strom.fileno())
            os.set_handle_inheritable(h, True)
            return h
        except Exception:
            return None

    desktop = user32.CreateDesktopW(DESKTOP_NAME, None, None, 0,
                                    _GENERIC_ALL, None)
    if not desktop:
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        for strom in (sys.stdout, sys.stderr):
            try:
                strom.flush()
            except Exception:
                pass
        si = STARTUPINFOW()
        si.cb = ctypes.sizeof(si)
        si.lpDesktop = DESKTOP_NAME
        si.dwFlags = _STARTF_USESTDHANDLES
        si.hStdInput = griff(sys.stdin)
        si.hStdOutput = griff(sys.stdout)
        si.hStdError = griff(sys.stderr)
        pi = PROCESS_INFORMATION()
        zeile = ctypes.create_unicode_buffer(_befehlszeile(argv))
        block, flags = None, 0
        if env is not None:
            block = ctypes.create_unicode_buffer(
                ''.join('%s=%s\0' % kv for kv in env.items()) + '\0')
            flags = _CREATE_UNICODE_ENVIRONMENT
        if not kernel32.CreateProcessW(None, zeile, None, None, True, flags,
                                       block, None, ctypes.byref(si),
                                       ctypes.byref(pi)):
            raise ctypes.WinError(ctypes.get_last_error())
        try:
            kernel32.WaitForSingleObject(pi.hProcess, _INFINITE)
            code = wintypes.DWORD()
            kernel32.GetExitCodeProcess(pi.hProcess, ctypes.byref(code))
            return code.value
        finally:
            kernel32.CloseHandle(pi.hThread)
            kernel32.CloseHandle(pi.hProcess)
    finally:
        user32.CloseDesktop(desktop)
