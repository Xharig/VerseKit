# -*- coding: utf-8 -*-
#
# VerseKit — companion for Star Citizen.
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
Prüft „Beenden“ am Symbol neben der Uhr: danach läuft kein Prozess des Laufs mehr (Windows, auf einem eigenen,
unsichtbaren Desktop).

Ablauf je Fall: Das Programm startet (Overlay) · ein zweiter Start holt das Hauptfenster · im Fall `verborgen`
wird das Hauptfenster wie vom Spieler geschlossen (WM_CLOSE: es geht in die Ablage und läuft weiter) · dann
„Beenden“ im Menü des Symbols (WM_COMMAND an das Fenster des Symbols, wie Windows es nach der Wahl schickt) ·
nach höchstens 20 Sekunden darf kein Prozess mit der Wegwerf-Kopie in der Befehlszeile mehr laufen.
Fall `fremd`: erst das Hauptfenster von Hand (`--hauptfenster`, wie eine Verknüpfung des Spielers), dann das
Overlay daneben, dann das Overlay geschlossen wie über das X seiner Leiste (WM_CLOSE) · auch dann bleibt nichts.

    python -I -X utf8 tools/beenden_pruefen.py <versekit.exe> <vorlage-der-daten> [verborgen|sichtbar|fremd ...]
    python -I -X utf8 tools/beenden_pruefen.py <versekit.exe> <vorlage> fremd --gegenprobe

`<vorlage-der-daten>` wird für jeden Fall in einen frischen Wegwerf-Ordner daneben kopiert (das Programm läuft mit
`--daten <kopie>` und schreibt nur dort). `--gegenprobe` erwartet ein ROT (eine Fassung mit dem Fehler).
Nichts erscheint auf dem Bildschirm des Nutzers; beendet wird nur, was die Prüfung selbst gestartet hat, und das
auch nur mit WM_CLOSE, wie der Nutzer es täte.
"""
import ctypes
import os
import shutil
import subprocess
import sys
import time
from ctypes import wintypes

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import eigener_desktop  # noqa: E402

WM_CLOSE = 0x0010
WM_COMMAND = 0x0111
# `tray::Entry::Quit.command()`: COMMAND_FIRST (1001) + 7.
BEENDEN = 1008
user32 = ctypes.WinDLL('user32', use_last_error=True)


def prozesse(datei, kopie):
    """Laufende Prozesse dieser Datei MIT der Wegwerf-Kopie in der Befehlszeile: [(Nummer, Befehlszeile)].
    ⚠ Nur die eigenen: Andere Fassungen (des Nutzers, anderer Sitzungen) laufen mit demselben Dateinamen."""
    out = subprocess.run(
        ['powershell', '-NoProfile', '-Command',
         'Get-CimInstance Win32_Process -Filter "Name=\'%s\'" | ForEach-Object '
         '{ "$($_.ProcessId)|$($_.CommandLine)" }' % datei],
        capture_output=True, text=True).stdout
    found = []
    for line in out.splitlines():
        parts = line.strip().split('|', 1)
        if len(parts) == 2 and kopie.lower() in parts[1].lower():
            found.append((int(parts[0]), parts[1]))
    return found


def fenster(desktop_name, pids):
    """Fenster dieser Prozesse auf dem Desktop: [(Griff, Prozess, Titel, Klasse, sichtbar)]."""
    desktop = user32.OpenDesktopW(desktop_name, 0, False, 0x0100 | 0x0001)
    found = []
    callback = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)

    def each(hwnd, _):
        pid = wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
        if pid.value in pids:
            titel = ctypes.create_unicode_buffer(256)
            user32.GetWindowTextW(hwnd, titel, 256)
            klasse = ctypes.create_unicode_buffer(256)
            user32.GetClassNameW(hwnd, klasse, 256)
            found.append((hwnd, pid.value, titel.value, klasse.value, bool(user32.IsWindowVisible(hwnd))))
        return True

    if desktop:
        user32.EnumDesktopWindows(desktop, callback(each), 0)
        user32.CloseDesktop(desktop)
    return found


def desktop_anlegen(name):
    """Der eigene Desktop; der Griff bleibt offen, solange die Prüfung läuft (⚠ ohne offenen Griff und ohne
    Fenster darauf löscht Windows den Desktop, und ein gerade gestarteter Prozess stirbt mit ihm)."""
    (u32, *_rest) = eigener_desktop._bauteile()
    desktop = u32.CreateDesktopW(name, None, None, 0, 0x10000000, None)
    if not desktop:
        raise ctypes.WinError(ctypes.get_last_error())
    return desktop


def starten(argv, name):
    """Startet `argv` auf dem Desktop `name` (von [`desktop_anlegen`] offen gehalten) und kehrt sofort zurück
    (`eigener_desktop.starten` wartet auf das Ende).

    ⚠ Alles Starten und jedes `subprocess.run` laufen hier im selben Faden: `CreateProcessW` mit vererbbaren Griffen
    nähme sonst die Rohre eines gleichzeitig laufenden PowerShell-Aufrufs mit, und dessen Lesen endete nie."""
    (_u32, kernel32, _msvcrt, _wt, STARTUPINFOW, PROCESS_INFORMATION) = eigener_desktop._bauteile()
    si = STARTUPINFOW()
    si.cb = ctypes.sizeof(si)
    si.lpDesktop = name
    pi = PROCESS_INFORMATION()
    zeile = ctypes.create_unicode_buffer(subprocess.list2cmdline(argv))
    if not kernel32.CreateProcessW(None, zeile, None, None, False, 0, None, None, ctypes.byref(si),
                                   ctypes.byref(pi)):
        raise ctypes.WinError(ctypes.get_last_error())
    kernel32.CloseHandle(pi.hThread)
    kernel32.CloseHandle(pi.hProcess)


def sichtbare(name, pids):
    """Die sichtbaren Fenster der Prozesse, deren Titel mit „VerseKit“ beginnt (nicht die Hilfsfenster von winit)."""
    return [f for f in fenster(name, pids) if f[4] and f[2].startswith('VerseKit')]


def warten(bedingung, sekunden, schritt=0.5):
    """Wartet, bis `bedingung()` wahr ist; liefert ihr letztes Ergebnis."""
    ende = time.time() + sekunden
    while True:
        wert = bedingung()
        if wert or time.time() >= ende:
            return wert
        time.sleep(schritt)


def aufraeumen(name, datei, kopie):
    """Schließt, was von der Prüfung noch läuft, wie der Nutzer es täte: WM_CLOSE an die eigenen Fenster, zuerst
    das Overlay, dann das Hauptfenster (das ohne Overlay nicht mehr in die Ablage geht). True, wenn nichts bleibt."""
    for _ in range(3):
        rest = prozesse(datei, kopie)
        if not rest:
            return True
        for pid, _ in sorted(rest, key=lambda r: '--hauptfenster' in r[1]):
            for hwnd, _, titel, _, _ in fenster(name, {pid}):
                if titel.startswith(('VerseKit', 'Verse-Kit')):
                    user32.PostMessageW(hwnd, WM_CLOSE, 0, 0)
            time.sleep(3)
        warten(lambda: not prozesse(datei, kopie), 10)
    return not prozesse(datei, kopie)


def ablauf(exe, kopie, art, name, datei):
    """Die Schritte eines Falls; gibt die Befunde zurück."""
    def overlays():
        return {p for p, zeile in prozesse(datei, kopie) if '--hauptfenster' not in zeile}

    def haupt():
        return [p for p, zeile in prozesse(datei, kopie) if '--hauptfenster' in zeile]

    def overlay_steht():
        return [f for f in fenster(name, overlays()) if f[2].lower() == 'versekit overlay']

    def haupt_steht():
        return len(haupt()) == 1 and sichtbare(name, set(haupt()))

    befunde = []
    if art == 'fremd':
        # The main window first, by hand (as a shortcut of the player would): the overlay did not start it.
        starten([exe, '--hauptfenster', '--daten', kopie], name)
        if not warten(haupt_steht, 60):
            return ['das Hauptfenster steht nicht: %s' % prozesse(datei, kopie)]
        starten([exe, '--daten', kopie], name)
        if not warten(overlay_steht, 60):
            return ['das Overlay steht nicht neben dem Hauptfenster']
    else:
        starten([exe, '--daten', kopie], name)
        if not warten(overlay_steht, 60):
            return ['das Overlay steht nicht']
        time.sleep(3)
        # A second start hands over to the overlay (it opens the main window) and ends.
        starten([exe, '--daten', kopie], name)
        if not warten(haupt_steht, 60):
            return ['das Hauptfenster steht nicht: %s' % prozesse(datei, kopie)]
    # Das Hauptfenster lebt eine Weile (die Lage setzt sich, die Seiten laufen an).
    time.sleep(4)
    hp = haupt()[0]
    if art == 'verborgen':
        for hwnd, *_ in sichtbare(name, {hp}):
            user32.PostMessageW(hwnd, WM_CLOSE, 0, 0)
        if not warten(lambda: not sichtbare(name, {hp}), 10):
            befunde.append('das Hauptfenster ging nicht in die Ablage')
        if hp not in {p for p, _ in prozesse(datei, kopie)}:
            befunde.append('das Hauptfenster endete beim Schließen, obwohl das Overlay läuft')
        time.sleep(2)
    if art == 'fremd':
        # The X of the overlay's bar and the system's closing take the same way (`on_exit`): WM_CLOSE at the overlay.
        for hwnd, *_ in overlay_steht():
            user32.PostMessageW(hwnd, WM_CLOSE, 0, 0)
    else:
        ablage = [f for f in fenster(name, overlays()) if f[3] == 'VerseKitAblage']
        if not ablage:
            befunde.append('das Fenster des Symbols fehlt: %s' % fenster(name, overlays()))
        for hwnd, *_ in ablage:
            user32.PostMessageW(hwnd, WM_COMMAND, BEENDEN, 0)
    if not warten(lambda: not prozesse(datei, kopie), 20):
        uebrig = prozesse(datei, kopie)
        befunde.append('nach „Beenden“ laufen noch Prozesse: %s' % [
            ('Hauptfenster' if '--hauptfenster' in zeile else 'Overlay', pid) for pid, zeile in uebrig])
        befunde.append('ihre Fenster: %s' % [(f[2], f[3], f[4]) for f in fenster(name, {p for p, _ in uebrig})])
    return befunde


def fall(exe, vorlage, art):
    """Ein Fall (`verborgen` oder `sichtbar`) in einer frischen Kopie; gibt die Befunde zurück (leer = gut)."""
    kopie = os.path.join(os.path.dirname(os.path.abspath(vorlage)),
                         'beenden-kopie-%s-%d' % (art, int(time.time())))
    shutil.copytree(vorlage, kopie)
    name = eigener_desktop.eindeutiger_name()
    datei = os.path.basename(exe)
    befunde = ['abgebrochen']
    desktop = desktop_anlegen(name)
    try:
        befunde = ablauf(exe, kopie, art, name, datei)
    except OSError as fehler:
        # A start that the system refuses (no such file, no desktop) is a finding in the log, never a dialog.
        befunde = ['Start verweigert: %s' % fehler]
    finally:
        if not aufraeumen(name, datei, kopie):
            befunde.append('auch nach WM_CLOSE laufen noch Prozesse: %s' % prozesse(datei, kopie))
        user32.CloseDesktop(desktop)
    if not befunde:
        shutil.rmtree(kopie, ignore_errors=True)
    return befunde


def main(argv):
    gegenprobe = '--gegenprobe' in argv
    args = [a for a in argv if not a.startswith('--')]
    if len(args) < 2 or not eigener_desktop.verfuegbar():
        print(__doc__)
        return 2
    if '--leer' in argv:
        # The check of the check: a start with an empty program path is refused in the log, without a dialog.
        args[0] = ''
    exe, vorlage = os.path.abspath(args[0]) if args[0] else '', args[1]
    # ⚠ Only a program file that is there is ever started: an empty or wrong path must not reach the shell.
    if not os.path.isfile(exe) or not exe.lower().endswith('.exe'):
        print('FEHL Programmdatei fehlt oder ist keine .exe: %r' % args[0])
        print('ERGEBNIS rot')
        return 2
    if not os.path.isdir(vorlage):
        print('FEHL Vorlage der Daten fehlt: %r' % vorlage)
        print('ERGEBNIS rot')
        return 2
    arten = args[2:] or ['verborgen', 'sichtbar', 'fremd']
    rot = False
    for art in arten:
        befunde = fall(exe, vorlage, art)
        for b in befunde:
            print('FEHL', art, b, flush=True)
        print('FALL', art, 'rot' if befunde else 'grün', flush=True)
        rot |= bool(befunde)
    print('ERGEBNIS', 'rot' if rot else 'grün', flush=True)
    if gegenprobe:
        print('GEGENPROBE', 'ist rot geworden, wie verlangt' if rot else 'HÄTTE ROT WERDEN MÜSSEN')
        return 0 if rot else 1
    return 1 if rot else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
