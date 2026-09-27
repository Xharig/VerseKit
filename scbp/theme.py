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
Farbschemata — die EINE Stelle, an der die Farben des Programms stehen.

⚠⚠ **Warum es diese Datei gibt (v3.58.0-rc5).** Bis dahin stand jede Farbe in
jeder Datei einzeln: dasselbe Grün in 13 Dateien, der Hintergrund der
Eingabefelder 100 Mal direkt im Code. Ein zweites Farbschema hätte jede
dieser Stellen doppelt gebraucht. Jetzt holt sich jedes Fenster seine Farben
von hier.

⚠⚠ **„Verse-Kit (Original)" ist pixelgleich mit allem davor.** Die Werte
unten sind die alten, Zeichen für Zeichen. Belegt wird das mit einem
Bildvergleich aller Seiten vor und nach dem Umbau.

⚠ **Gewählt wird beim Start.** Die Farben werden beim Laden der Module
gelesen (als Konstanten wie bisher), ein Wechsel wirkt deshalb nach einem
Neustart. Die Seite „Darstellung" bietet ihn an.

## Die Schemata

| Kennung | Name | Vorbild |
|---|---|---|
| `original` | Verse-Kit (Original) | die Markenfarbe `#9ce430` auf Nachtblau |
| `krt` | KRT (Orange) | das Design-System des Profit Basetools: Schwarz, Hausfarbe `#E77E23`, eckige Kästen mit orangen Eckwinkeln, Überschriften und Knöpfe in Großbuchstaben |
"""
from . import paths

SETTING = 'farbschema'

SCHEMES = {
    'original': {
        'label': 's_da_original',
        'bg': '#10141c', 'surface': '#161c28', 'bar': '#1b2230',
        'fg': '#e6edf3', 'sub': '#8b98a5', 'accent': '#9ce430',
        'line': '#232c3d', 'line2': '#2a3345', 'field': '#0c1017',
        'gold': '#e8c353', 'red': '#e05252', 'red_pale': '#c98a8a',
        'yellow': '#d8a03a', 'accent_dark': '#1d2a14', 'hover': '#222b3b',
        'track': '#2b3547', 'track_on': '#2a3a1c', 'heat_low': '#1b2230',
        'selected': '#1d2634', 'danger_fill': '#2a1414',
        'scroll_groove': '#0b0e14', 'scroll_grip': '#5a6b85',
        'scroll_grip_light': '#7d90ad',
        # Wie Kästen, Überschriften und Knöpfe gezeichnet werden.
        'square': False, 'upper_headings': False, 'accent_headings': False,
        'filled_buttons': False, 'icon_set': 'gruen',
    },
    # ⭐ Entschieden am 27.09.2026: „wie beim Basetool". Die Werte stammen aus
    # dem Design-System des Profit Basetools (`colors_and_type.css`, Stand
    # 27.09.2026): Hausfarbe #E77E23, Seitengrund
    # Schwarz, Flächen #141414, Haarlinien #282828, Text #D2D2D2, Ecken 0.
    # Die Form ebenfalls von dort: `.hud-box` mit zwei 10-px-Eckwinkeln,
    # Überschriften und Knöpfe in Großbuchstaben, Knöpfe orange gefüllt mit
    # schwarzer Schrift.
    'krt': {
        'label': 's_da_krt',
        'bg': '#000000', 'surface': '#141414', 'bar': '#141414',
        'fg': '#d2d2d2', 'sub': '#8a8a8a', 'accent': '#e77e23',
        'line': '#282828', 'line2': '#282828', 'field': '#1c1c1c',
        'gold': '#ffd23f', 'red': '#f2564b', 'red_pale': '#d98a82',
        'yellow': '#eeb64b', 'accent_dark': '#c45c00', 'hover': '#282828',
        'track': '#282828', 'track_on': '#5a2e08', 'heat_low': '#141414',
        'selected': '#282828', 'danger_fill': '#2a0a0c',
        # Rollbalken laut Design-System: Griff #646464, beim Überfahren
        # #EEB64B (die helle Zierfarbe).
        'scroll_groove': '#0a0a0a', 'scroll_grip': '#646464',
        'scroll_grip_light': '#eeb64b',
        'square': True, 'upper_headings': True, 'accent_headings': True,
        'filled_buttons': True, 'icon_set': 'orange',
    },
}

DEFAULT = 'original'


def _chosen():
    try:
        name = paths.settings().get(SETTING)
    except Exception:
        name = None
    return name if name in SCHEMES else DEFAULT


NAME = _chosen()
_S = SCHEMES[NAME]

BG = _S['bg']
SURFACE = _S['surface']
BAR = _S['bar']
FG = _S['fg']
SUB = _S['sub']
ACCENT = _S['accent']
LINE = _S['line']
LINE2 = _S['line2']
FIELD = _S['field']
GOLD = _S['gold']
RED = _S['red']
RED_PALE = _S['red_pale']
YELLOW = _S['yellow']
ACCENT_DARK = _S['accent_dark']
HOVER = _S['hover']
TRACK = _S['track']
TRACK_ON = _S['track_on']
HEAT_LOW = _S['heat_low']
SELECTED = _S['selected']
DANGER_FILL = _S['danger_fill']
SCROLL_GROOVE = _S['scroll_groove']
SCROLL_GRIP = _S['scroll_grip']
SCROLL_GRIP_LIGHT = _S['scroll_grip_light']
SQUARE = _S['square']
UPPER_HEADINGS = _S['upper_headings']
ACCENT_HEADINGS = _S['accent_headings']
FILLED_BUTTONS = _S['filled_buttons']
ICON_SET = _S['icon_set']


def choose(name):
    """Das Schema für den nächsten Start merken."""
    if name in SCHEMES:
        return paths.set_setting(SETTING, name)
    return False


def heading(text):
    """Eine Überschrift so, wie das Schema sie schreibt."""
    return text.upper() if UPPER_HEADINGS and text else text
