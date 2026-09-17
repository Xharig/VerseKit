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
| Wie oft? | alle 10 Minuten nachsehen (bis 17.09.2026: 30) |
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
# ⚠⚠ 10 statt 30 Minuten (17.09.2026). Mehrfach gewünscht, „30 Minuten ist zu
# lang" — und mehrfach nur an Einzelfällen nachgebessert, statt den Takt selbst
# zu ändern. 6 Abfragen je Stunde bleiben weit unter GitHubs 60 ohne Anmeldung.
CHECK_INTERVAL_S = 10 * 60
# Solange ein Update auf das Ende des Spiels wartet: so oft nachsehen.
# ⚠ 20 statt 60 Sekunden (17.09.2026) — Wunsch Bushwick4712 (KRT): das Update
# gut eine Minute nach Spielende, nicht fünf. Die Prozessliste zu lesen kostet
# unter Windows wenige Millisekunden.
GAME_POLL_S = 20
# Ist die Prozessliste lesbar und das Spiel NICHT darin, gilt es als beendet,
# sobald die `Game.log` so lange still ist. Die kurze Frist fängt das Aufräumen
# beim Beenden ab (Absturzmelder, letzte Zeilen). Nur ohne lesbare Prozessliste
# gilt weiter die lange Frist aus `paths.GAME_IDLE_SEC` (fünf Minuten).
EXIT_QUIET_S = 30
# So alt muss eine Freigabe sein, bevor sie automatisch geholt wird.
# ⚠ 2 statt 10 Minuten (17.09.2026): Der langsame Abruf direkt nach dem
# Veröffentlichen dauerte gemessen rund drei Minuten und war nach kurzer Zeit
# vorbei; zehn Minuten Wartezeit standen dazu in keinem Verhältnis.
FRESH_WAIT_S = 2 * 60

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


def wait_left(release, now=None):
    """Wie viele Sekunden die Freigabe noch reifen muss — 0, wenn sie reif ist.

    Ohne Datum (alte Zwischenspeicher kennen nur den Tag) gilt sie als reif —
    lieber holen als nie.

    ⚠⚠ **Die Restzeit, nicht die ganze Frist** (17.09.2026). Vorher wurde bei
    einer zu frischen Freigabe stets volle `FRESH_WAIT_S` später erneut
    gefragt — und dieser geplante Termin sperrte das Nachsehen beim
    Spielende. Gemessen: v3.48.0 um 03:08:29 erschienen, der Takt um ~03:18
    fand sie 9½ Minuten alt und plante 03:28; das Spiel ging um 03:19 zu, die
    Spielende-Wache fand die Fassung, durfte wegen des Termins aber nichts
    tun. Das Update musste von Hand eingespielt werden.
    """
    stamp = (release or {}).get('zeit') or ''
    if not stamp:
        return 0
    try:
        published = datetime.strptime(stamp, '%Y-%m-%dT%H:%M:%SZ').replace(
            tzinfo=timezone.utc).timestamp()
    except ValueError:
        return 0
    age = (now if now is not None else time.time()) - published
    return max(0, int(FRESH_WAIT_S - age + 0.999))


def ripe(release, now=None):
    """Ist die Freigabe alt genug, um sie zu holen? Siehe `wait_left`."""
    return wait_left(release, now) == 0


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


def _log_quiet_for():
    """Sekunden seit dem letzten Schreiben der `Game.log` — oder None."""
    try:
        log_file = paths.game_log()
        if not log_file:
            return None
        return time.time() - os.path.getmtime(log_file)
    except Exception:
        return None


def game_running():
    """Läuft Star Citizen? Im Zweifel **ja**.

    | Prozessliste | Entscheidung |
    |---|---|
    | Spiel darin | läuft |
    | lesbar, Spiel **nicht** darin | läuft nur, wenn die Log in den letzten `EXIT_QUIET_S` geschrieben wurde |
    | nicht lesbar | die Log allein, mit der langen Frist (`paths.game_running`) |

    ⚠ Bis zum 17.09.2026 galt die lange Frist **immer**: Das Update kam frühestens
    fünf Minuten nach Spielende, obwohl die Prozessliste längst sagte, dass das
    Spiel zu ist. Gemessen am selben Abend: Spiel um 00:08:25 beendet, VerseKit
    wartete bis 00:13:25 — und um 00:13:15 lief das Spiel schon wieder.
    """
    try:
        process = (_windows_game_process() if sys.platform == 'win32'
                   else _linux_game_process())
    except Exception:
        process = None
    if process:
        return True
    if process is False:
        quiet = _log_quiet_for()
        return quiet is not None and quiet < EXIT_QUIET_S
    return bool(paths.game_running())
