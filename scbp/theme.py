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
| `krt` | KRT (Orange) | das Profit Basetool: fast schwarz, `#FF8000`, eckige Kästen mit orangen Eckwinkeln, Überschriften in Großbuchstaben |
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
        # Wie Kästen, Überschriften und Knöpfe gezeichnet werden.
        'square': False, 'upper_headings': False, 'accent_headings': False,
        'filled_buttons': False, 'icon_set': 'gruen',
    },
    # ⭐ Entschieden am 27.09.2026: „wie beim Basetool". Farben aus der
    # KRT-Palette (`--krt-*`), die Form vom Profit Basetool.
    'krt': {
        'label': 's_da_krt',
        'bg': '#0e0e0e', 'surface': '#1a1a1a', 'bar': '#141414',
        'fg': '#e4e4e4', 'sub': '#969696', 'accent': '#ff8000',
        'line': '#2e2e2e', 'line2': '#3a3a3a', 'field': '#080808',
        'gold': '#f0c060', 'red': '#f04747', 'red_pale': '#d99090',
        'yellow': '#e0a040', 'accent_dark': '#2e1a06', 'hover': '#242424',
        'track': '#333333', 'track_on': '#4a2a08', 'heat_low': '#1c1c1c',
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
