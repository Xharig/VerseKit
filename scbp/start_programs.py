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
Startprogramme: weitere Programme mit dem Spiel starten.

Jeder Eintrag sagt, **wann** er startet:

| `wann` | Auslöser |
|---|---|
| `launcher` | zusammen mit dem RSI Launcher (Knopf `RSI Launcher starten`) |
| `spiel` | sobald Star Citizen läuft (die Spielende-Wache sieht alle 20 s nach) |
| `ersetzt` | **statt** des RSI Launchers — der Startknopf öffnet dann dieses Programm |

⚠⚠ **Es wird genau das ausgeführt, was der Spieler einträgt — nichts sonst.**
Keine Voreinträge, kein Befehl aus dem Netz. Der Hinweis auf der Seite sagt
das (eigenes Risiko).

⚠⚠ **Ein Prüflauf startet nie etwas.** Alles läuft über `_popen`, das die
Prüfung ersetzt.
"""
import os
import threading

from . import errors, paths

SETTING_ON = 'startprogramme_an'
SETTING_LIST = 'startprogramme'
WHEN = ('launcher', 'spiel', 'ersetzt')

# Was in dieser Sitzung gestartet wurde und beim Spielende wieder zu beenden
# ist: Liste von Popen-Objekten.
_RUNNING = []


def enabled():
    """Hauptschalter — ab Werk an (die Liste ist ab Werk ja leer)."""
    return paths.setting_bool(SETTING_ON, True)


def new_entry():
    return {'an': True, 'name': '', 'datei': '', 'argumente': '',
            'wann': 'launcher', 'warten': 0, 'beenden': False}


def entries():
    """Alle Einträge, jeder vollständig (fehlende Felder mit Vorgabe)."""
    # ⚠ `paths.setting()` liefert nur Text — eine Liste gälte dort als „nicht
    # gesetzt", und jeder Eintrag wäre beim Neuzeichnen verschwunden (von
    # Prüfung 271 gefunden, bevor es ausgeliefert wurde).
    raw = paths.settings().get(SETTING_LIST)
    result = []
    for item in (raw if isinstance(raw, list) else []):
        if isinstance(item, dict):
            entry = new_entry()
            entry.update({k: v for k, v in item.items() if k in entry})
            if entry['wann'] not in WHEN:
                entry['wann'] = 'launcher'
            result.append(entry)
    return result


def save(items):
    return paths.set_setting(SETTING_LIST, list(items))


def _active(when):
    if not enabled():
        return []
    return [e for e in entries()
            if e['an'] and e['wann'] == when and (e['datei'] or '').strip()]


def _command(entry):
    """Datei und Argumente als Liste für `Popen`."""
    import shlex
    command = [entry['datei'].strip()]
    args = (entry.get('argumente') or '').strip()
    if args:
        try:
            command += shlex.split(args, posix=not paths.WINDOWS)
        except ValueError:
            command += args.split()
    return command


def _popen(command, cwd):
    """Der einzige Weg nach draußen — die Prüfung ersetzt genau diese Stelle."""
    import subprocess
    extra = {}
    if paths.WINDOWS:
        extra['creationflags'] = getattr(subprocess, 'DETACHED_PROCESS', 0)
    else:
        extra['start_new_session'] = True
    return subprocess.Popen(command, cwd=cwd, stdout=subprocess.DEVNULL,
                            stderr=subprocess.DEVNULL, **extra)


def launch(entry, delay=True):
    """Einen Eintrag starten — nach der eingestellten Wartezeit.

    Gibt (True, '') oder (False, Grund) zurück; mit Wartezeit sofort
    (True, ''), der Start folgt im Hintergrund."""
    path = (entry.get('datei') or '').strip()
    if not path or not os.path.exists(path):
        return False, path or '—'

    def go():
        try:
            process = _popen(_command(entry), os.path.dirname(path) or None)
            if entry.get('beenden'):
                _RUNNING.append(process)
        except Exception as exception:
            errors.record('start_programs.launch', exception, path)

    try:
        wait = max(0, int(entry.get('warten') or 0)) if delay else 0
    except (TypeError, ValueError):
        wait = 0
    if wait:
        timer = threading.Timer(wait, go)
        timer.daemon = True
        timer.start()
    else:
        go()
    return True, ''


def replacement():
    """Der Eintrag, der den RSI Launcher ersetzt — oder None."""
    found = _active('ersetzt')
    return found[0] if found else None


def on_launcher():
    """Mit dem RSI Launcher: alle Einträge mit `wann='launcher'`."""
    for entry in _active('launcher'):
        launch(entry)


def on_game_started():
    for entry in _active('spiel'):
        launch(entry)


def on_game_ended():
    """Beim Spielende: was mit der Option zum Wiederbeenden gestartet wurde, beenden."""
    while _RUNNING:
        process = _RUNNING.pop()
        try:
            if process.poll() is None:
                process.terminate()
        except Exception as exception:
            errors.record('start_programs.on_game_ended', exception)
