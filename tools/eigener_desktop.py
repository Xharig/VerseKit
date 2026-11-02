# -*- coding: utf-8 -*-
#
# Verse-Kit — Begleiter für Star Citizen.
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
import itertools
import os
import sys
import time

DESKTOP_NAME = 'VerseKit-Pruefung'

# Wie oft dieser Prozess schon einen Desktop angelegt hat: gibt dem Namen eine Zahl dazu.
_ZAEHLER = itertools.count(1)


def eindeutiger_name():
    """Ein Desktop-Name, den kein anderer Lauf trägt.

    ⚠ Der Name gilt für die ganze Sitzung: `CreateDesktop` mit einem Namen, den es schon gibt,
    liefert den vorhandenen Desktop. Prüfläufe verschiedener Programme oder Sitzungen, die alle
    `VerseKit-Pruefung` nähmen, teilten sich dann Fenster, Zeiger und Nachrichten und stören sich
    gegenseitig (ein Fenster des einen liegt unter dem Zeiger des anderen). Darum: Prozessnummer,
    Zeit und eine Zählung dazu.
    """
    return '%s-%d-%d-%d' % (DESKTOP_NAME, os.getpid(), int(time.time() * 1000) % 10_000_000, next(_ZAEHLER))

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


_FENSTER_TITEL = 'vk-sichtbar-auf-dem-desktop'

# Das Kind A legt ein Fenster an und wartet; das Kind B sucht es. Auf demselben Desktop findet B das
# Fenster (Ausgang 1), auf einem eigenen nicht (Ausgang 0).
_KIND_A = (
    "import ctypes, time\n"
    "u = ctypes.WinDLL('user32')\n"
    "u.CreateWindowExW.restype = ctypes.c_void_p\n"
    "u.CreateWindowExW(0, 'STATIC', %r, 0, 0, 0, 10, 10, None, None, None, None)\n"
    "time.sleep(4)\n" % _FENSTER_TITEL
)
_KIND_B = (
    "import ctypes, sys\n"
    "u = ctypes.WinDLL('user32')\n"
    "u.FindWindowW.restype = ctypes.c_void_p\n"
    "sys.exit(1 if u.FindWindowW(None, %r) else 0)\n" % _FENSTER_TITEL
)


def pruefen(gegenprobe=False):
    """Zwei gleichzeitige Läufe sehen die Fenster des anderen nicht — Gegenprobe: gleicher Name.

    Gibt 0 zurück, wenn das Fenster von A für B unsichtbar blieb (bei `gegenprobe` erwartet man das
    Gegenteil, die Ausgabe sagt dann `ERGEBNIS rot`).
    """
    import threading
    gemeinsam = DESKTOP_NAME if gegenprobe else None
    ergebnis = {}

    def kind_a():
        ergebnis['a'] = starten([sys.executable, '-I', '-c', _KIND_A], name=gemeinsam)

    thread = threading.Thread(target=kind_a)
    thread.start()
    time.sleep(1.5)
    ergebnis['b'] = starten([sys.executable, '-I', '-c', _KIND_B], name=gemeinsam)
    thread.join()
    gesehen = ergebnis['b'] != 0
    print('Fenster des anderen Laufs gesehen: %s' % ('ja' if gesehen else 'nein'))
    print('ERGEBNIS rot' if gesehen else 'ERGEBNIS gruen')
    return 1 if gesehen else 0


def _befehlszeile(argv):
    """Die Argumente als eine Windows-Befehlszeile, richtig gequotet."""
    import subprocess
    return subprocess.list2cmdline(argv)


def starten(argv, env=None, name=None):
    """`argv` auf dem eigenen Desktop ausführen und den Rückgabewert liefern.

    Jeder Aufruf bekommt einen Desktop mit eigenem Namen (`eindeutiger_name`), sodass sich
    gleichzeitige Läufe nicht sehen. `name` setzt den Namen fest; `DESKTOP_NAME` selbst ist der
    gemeinsame Desktop von früher (nur für die Gegenprobe der Selbstprüfung).

    Wirft `OSError`, wenn der Desktop oder der Prozess nicht entsteht — der
    Aufrufer fällt dann auf seinen bisherigen Weg zurück.
    """
    (user32, kernel32, msvcrt, wintypes, STARTUPINFOW,
     PROCESS_INFORMATION) = _bauteile()
    name = name or eindeutiger_name()

    def griff(strom):
        try:
            h = msvcrt.get_osfhandle(strom.fileno())
            os.set_handle_inheritable(h, True)
            return h
        except Exception:
            return None

    desktop = user32.CreateDesktopW(name, None, None, 0, _GENERIC_ALL, None)
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
        si.lpDesktop = name
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


if __name__ == '__main__':
    # Selbstprüfung: `python -I tools/eigener_desktop.py --pruefen [--gegenprobe]`
    if '--pruefen' in sys.argv[1:]:
        sys.exit(pruefen('--gegenprobe' in sys.argv[1:]))
    print(__doc__)
