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
Systemmeldungen — eine Mitteilung des Betriebssystems, auch wenn das Overlay
gerade nicht zu sehen ist.

* **Windows:** eine Sprechblase am Symbol neben der Uhr
  (`tray_icon.TrayIcon.balloon`). Windows 10/11 zeigt sie als Mitteilung
  unten rechts. Ohne Symbol (ausgeschaltet) gibt es keine Meldung.
* **Linux:** `notify-send`, falls vorhanden — mit der Umgebung aus
  `paths.clean_environment()`, sonst stirbt es im AppImage an den
  mitgebrachten Bibliotheken.

Kein PowerShell und kein Hilfsprogramm unter Windows: Ein Skript, das eine
`.exe` im Hintergrund startet, hält der Virenscanner für verdächtig.

Die Einstellung `systemmeldung` (ab Werk an) schaltet alles ab.
"""
import shutil
import subprocess
import sys

WINDOWS = sys.platform.startswith('win')
SETTING = 'systemmeldung'

_SENDER = None if WINDOWS else shutil.which('notify-send')


def enabled():
    from . import paths
    return paths.setting_bool(SETTING, True)


def available():
    """Kann dieses System überhaupt eine Systemmeldung zeigen?"""
    return WINDOWS or bool(_SENDER)


def _popen(command):
    """Der Weg nach draußen unter Linux — eine eigene Stelle, damit ein
    Prüflauf sie ersetzen kann."""
    from . import paths
    return subprocess.Popen(command, env=paths.clean_environment(),
                            stdout=subprocess.DEVNULL,
                            stderr=subprocess.DEVNULL)


def show(title, text, tray=None):
    """Eine Systemmeldung zeigen. Gibt zurück, ob sie losgeschickt wurde.

    `tray` ist das laufende `TrayIcon` (nur unter Windows nötig)."""
    if not enabled():
        return False
    try:
        if WINDOWS:
            return bool(tray is not None and tray.balloon(title, text))
        if not _SENDER:
            return False
        _popen([_SENDER, '-a', 'Verse-Kit', title, text])
        return True
    except Exception as exc:
        from . import errors
        errors.record('notify.show', exc)
        return False
