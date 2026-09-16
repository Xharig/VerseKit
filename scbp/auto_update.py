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
Neue Versionen von selbst einspielen — aber nie mitten im Spiel.

Entschieden am 16.09.2026, als Patch-Versionen von der Tagesgrenze befreit
wurden: Ein behobener Fehler soll alle erreichen, ohne dass jemand klickt.

| Frage | Antwort |
|---|---|
| Wie oft? | alle 30 Minuten nachsehen |
| Für wen? | für alle, abschaltbar (`update_automatisch`, Standard an) |
| Testversionen? | ja, wenn „Auch Testversionen" an ist — das regelt `updater.check` |
| Während Star Citizen läuft? | **nein** — erst, wenn das Spiel zu ist |

⚠⚠ **„Läuft das Spiel?" wird hier strenger beantwortet als anderswo.**
`paths.game_running()` schaut nur, ob die `Game.log` in den letzten fünf
Minuten geschrieben wurde — dort kostet ein Irrtum ein falsches Wort. Hier
kostet er das Overlay mitten im Flug, samt Fokussprung in den Desktop. Deshalb
zählt zusätzlich die **Prozessliste**: Steht `StarCitizen.exe` darin, wird
gewartet, auch wenn das Log gerade schweigt.

⚠ **Frisch veröffentlicht heißt noch nicht abholbereit.** Am 16.09.2026 kam
v3.43.1 um 19:25 heraus; wer 90 Sekunden später lud, brauchte drei Minuten für
19 MB — kurz danach dauerte dieselbe Datei 1,3 Sekunden. Eine Freigabe wird
deshalb erst geholt, wenn sie `FRESH_WAIT` alt ist.

Diese Datei entscheidet nur **ob**. Das **Wie** — Herunterladen mit Prüfsumme,
Installer, Neustart-Helfer — ist derselbe Weg wie beim Knopf in „Update & Über".
"""
import os
import re
import sys
import time
from datetime import datetime, timezone

from . import paths

# Wie oft nachgesehen wird. `updater.MIN_INTERVAL` passt dazu.
CHECK_INTERVAL_S = 30 * 60
# Solange ein Update auf das Ende des Spiels wartet: so oft nachsehen.
GAME_POLL_S = 60
# So alt muss eine Freigabe sein, bevor sie automatisch geholt wird.
FRESH_WAIT_S = 10 * 60

SETTING = 'update_automatisch'

# Die Programmdatei des Spiels — unter Windows wie unter Wine/Proton.
GAME_EXE = 'starcitizen.exe'


def enabled():
    """Darf automatisch eingespielt werden?

    ⚠ Nur, wenn überhaupt nach Versionen gesehen wird — wer „Nach neuen
    Versionen sehen" ausschaltet, meint damit auch: nichts von selbst holen.
    """
    return (paths.setting_bool('update_pruefen', True)
            and paths.setting_bool(SETTING, True))


def ripe(release, now=None):
    """Ist die Freigabe alt genug, um sie zu holen?

    Ohne Datum (alte Zwischenspeicher kennen nur den Tag) gilt sie als reif —
    lieber holen als nie.
    """
    stamp = (release or {}).get('zeit') or ''
    if not stamp:
        return True
    try:
        published = datetime.strptime(stamp, '%Y-%m-%dT%H:%M:%SZ').replace(
            tzinfo=timezone.utc).timestamp()
    except ValueError:
        return True
    return ((now if now is not None else time.time()) - published) >= FRESH_WAIT_S


# ------------------------------------------------------------ Läuft das Spiel?


def _windows_game_process():
    """Steht `StarCitizen.exe` in der Prozessliste? (Windows)"""
    import ctypes
    from ctypes import wintypes
    k = ctypes.WinDLL('kernel32', use_last_error=True)
    enum = k.K32EnumProcesses
    enum.argtypes = [ctypes.POINTER(wintypes.DWORD), wintypes.DWORD,
                     ctypes.POINTER(wintypes.DWORD)]
    enum.restype = wintypes.BOOL
    open_ = k.OpenProcess
    open_.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    open_.restype = wintypes.HANDLE
    query = k.QueryFullProcessImageNameW
    query.argtypes = [wintypes.HANDLE, wintypes.DWORD, wintypes.LPWSTR,
                      ctypes.POINTER(wintypes.DWORD)]
    query.restype = wintypes.BOOL
    close = k.CloseHandle
    close.argtypes = [wintypes.HANDLE]
    close.restype = wintypes.BOOL

    size = 4096
    while True:
        pids = (wintypes.DWORD * size)()
        used = wintypes.DWORD(0)
        if not enum(pids, ctypes.sizeof(pids), ctypes.byref(used)):
            return None                  # unbekannt
        count = used.value // ctypes.sizeof(wintypes.DWORD)
        if count < size:
            break
        size *= 2
    buffer = ctypes.create_unicode_buffer(1024)
    for pid in pids[:count]:
        if not pid:
            continue
        handle = open_(0x1000, False, pid)     # PROCESS_QUERY_LIMITED_INFORMATION
        if not handle:
            continue
        try:
            length = wintypes.DWORD(len(buffer))
            if query(handle, 0, buffer, ctypes.byref(length)):
                if os.path.basename(buffer.value).lower() == GAME_EXE:
                    return True
        finally:
            close(handle)
    return False


def _linux_game_process(proc='/proc'):
    """Läuft `StarCitizen.exe` unter Wine/Proton? (Linux)"""
    try:
        entries = os.listdir(proc)
    except OSError:
        return None
    for entry in entries:
        if not entry.isdigit():
            continue
        try:
            with open(os.path.join(proc, entry, 'cmdline'), 'rb') as f:
                command = f.read().replace(b'\0', b' ').decode('utf-8', 'replace')
        except OSError:
            continue
        # Wine führt den Windows-Pfad in der Kommandozeile: `C:\…\StarCitizen.exe`
        if re.search(r'(^|[\\/ ])starcitizen\.exe(\s|$)', command.lower()):
            return True
    return False


def game_running():
    """Läuft Star Citizen? Im Zweifel **ja**.

    Ja, wenn der Prozess da ist **oder** das Log gerade geschrieben wird. Lässt
    sich die Prozessliste nicht lesen, zählt das Log allein.
    """
    try:
        process = (_windows_game_process() if sys.platform == 'win32'
                   else _linux_game_process())
    except Exception:
        process = None
    if process:
        return True
    return bool(paths.game_running())
