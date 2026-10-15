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
MangoHud beim Start über *RSI Launcher starten* einschalten — nur Linux.

MangoHud wird **nicht** mitgeliefert. Es kommt aus der Paketverwaltung des
Systems; hier wird nur erkannt, ob es da ist, und beim Start die Variable
`MANGOHUD=1` gesetzt. Die erbt das Startskript, daraus Wine und daraus das
Spiel.

⚠ **Nur `MANGOHUD=1`, nie `MANGOHUD_CONFIG`.** Ist `MANGOHUD_CONFIG` gesetzt,
liest MangoHud die Datei `~/.config/MangoHud/MangoHud.conf` nicht mehr (außer
mit `read_cfg` vorn) — die eigene Einrichtung des Spielers wäre still weg.

Der RSI Launcher selbst bekommt keine Anzeige: `RSI Launcher.exe` steht in der
eingebauten Ausnahmeliste von MangoHud (`src/blacklist.cpp`).
"""
import glob
import os
import shutil
import sys

SETTING = 'mangohud'

# Wo die Paketverwaltung die Vulkan-Schicht ablegt. Die Schicht ist das, was
# `MANGOHUD=1` wirken lässt — das Programm `mangohud` ist nur ein Startwrapper.
LAYER_DIRS = ('/usr/share/vulkan/implicit_layer.d',
              '/usr/local/share/vulkan/implicit_layer.d',
              '/etc/vulkan/implicit_layer.d',
              '/etc/xdg/vulkan/implicit_layer.d',
              '~/.local/share/vulkan/implicit_layer.d',
              '~/.config/vulkan/implicit_layer.d')


def possible():
    """Läuft das hier überhaupt? Nur unter Linux."""
    return sys.platform.startswith('linux')


def installed():
    """Ist MangoHud auf diesem System eingerichtet?"""
    if not possible():
        return False
    for folder in LAYER_DIRS:
        pattern = os.path.join(os.path.expanduser(folder), '*ango*ud*.json')
        if glob.glob(pattern):
            return True
    return shutil.which('mangohud') is not None


def wanted():
    """Hat der Spieler den Schalter eingeschaltet?"""
    from . import paths
    return paths.setting_bool(SETTING, False)


def set_wanted(value):
    from . import paths
    paths.set_setting(SETTING, bool(value))


def environment(base=None):
    """Die Umgebung für den Spielstart — oder `None`, wenn nichts zu ändern ist.

    `None` heißt: Der Start erbt die Umgebung wie bisher. Nur wenn der
    Schalter an ist und MangoHud da ist, kommt eine Kopie mit `MANGOHUD=1`."""
    if not (wanted() and installed()):
        return None
    env = dict(os.environ if base is None else base)
    env['MANGOHUD'] = '1'
    return env
