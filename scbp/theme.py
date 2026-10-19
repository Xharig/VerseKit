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

⚠⚠ **Warum es diese Datei gibt.** Steht jede Farbe in jeder Datei einzeln,
bräuchte ein zweites Farbschema jede dieser Stellen doppelt. Deshalb holt
sich jedes Fenster seine Farben von hier.

⚠⚠ **Das Schema `original` ist der Bezug.** Seine Werte sind die
Grundfarben des Programms, Zeichen für Zeichen; belegt per Bildvergleich
aller Seiten.

⚠ **Gewählt wird beim Start.** Die Farben werden beim Laden der Module
gelesen (als Konstanten), ein Wechsel wirkt deshalb nach einem
Neustart. Die Seite „Darstellung" bietet ihn an.

## Die Schemata

| Kennung | Name | Vorbild |
|---|---|---|
| `original` | Verse-Kit (Original) | die Markenfarbe `#9ce430` auf Nachtblau |
| `krt` | KRT (Orange) | das Design-System des Profit Basetools (Stand 2026-10): Grau `#202020` mit Flächen `#35393D`, Hausfarbe `#FB8A28`, Bedienelemente mit Füllung und sichtbarem Rand, eckige Kästen mit orangen Eckwinkeln, Überschriften und Knöpfe in Großbuchstaben |
| `eis` | Eis (Cyan) | `#38d6f5` auf Nachtblau — kühl, passt zum Weltall |
| `nebel` | Nebel (Violett) | `#a78bfa` auf tiefem Violettgrau — ruhig, für lange Abende |
| `glut` | Glut (Rot) | `#ff5a4f` auf Anthrazit; Fehlerfarbe gelb-orange |
| `kontrast` | Hoher Kontrast | `#ffd400` und Weiß auf Schwarz — für schlechte Augen und helle Räume |

Jedes Schema bringt seinen eigenen Symbolsatz mit (`icon_set`, gebaut von
`tools/symbole_bauen.py`).
"""
from . import paths

SETTING = 'farbschema'

# Hausfarbe des KRT Profit Basetools — für Knöpfe, die dorthin gehören, in
# jedem Farbschema gleich.
KRT_ORANGE = '#fb8a28'

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
        'control': '#161c28', 'control_border': '#232c3d',
        'hover_fg': '#9ce430',
        'square': False, 'upper_headings': False, 'accent_headings': False,
        'filled_buttons': False, 'icon_set': 'gruen',
    },
    'krt': {
        'label': 's_da_krt',
        'bg': '#202020', 'surface': '#35393d', 'bar': '#2a2d31',
        'fg': '#dadbde', 'sub': '#b0b3b7', 'accent': '#fb8a28',
        'line': '#404145', 'line2': '#404145', 'field': '#3f474c',
        'gold': '#ffd23f', 'red': '#ff7a70', 'red_pale': '#d98a82',
        'yellow': '#eeb64b', 'accent_dark': '#473321', 'hover': '#3f474c',
        'track': '#3f474c', 'track_on': '#784a23', 'heat_low': '#2a2d31',
        'selected': '#594839', 'danger_fill': '#3a1a1c',
        'scroll_groove': '#35393d', 'scroll_grip': '#8f9399',
        'scroll_grip_light': '#eeb64b',
        'control': '#3f474c', 'control_border': '#8f9399',
        'hover_fg': '#eeb64b',
        'square': True, 'upper_headings': True, 'accent_headings': True,
        'filled_buttons': True, 'icon_set': 'orange',
    },
    # ⭐ Vier weitere Schemata in der Form des Originals (runde Kästen,
    # Überschriften wie gewohnt) — nur die Farben wechseln. Aufgebaut wie
    # `original`: Grund, Fläche und Leiste in
    # drei Stufen desselben Tons, der Akzent als einzige laute Farbe.
    'eis': {
        'label': 's_da_eis',
        'bg': '#0b1219', 'surface': '#111b25', 'bar': '#15212d',
        'fg': '#e3f1f7', 'sub': '#86a0ae', 'accent': '#38d6f5',
        'line': '#1d2c3a', 'line2': '#243546', 'field': '#081017',
        'gold': '#e8c353', 'red': '#e05252', 'red_pale': '#c98a8a',
        'yellow': '#d8a03a', 'accent_dark': '#0e2a33', 'hover': '#1b2836',
        'track': '#243446', 'track_on': '#12394a', 'heat_low': '#15212d',
        'selected': '#162633', 'danger_fill': '#2a1414',
        'scroll_groove': '#070c11', 'scroll_grip': '#4f7184',
        'scroll_grip_light': '#78a6bb',
        'control': '#111b25', 'control_border': '#1d2c3a',
        'hover_fg': '#38d6f5',
        'square': False, 'upper_headings': False, 'accent_headings': False,
        'filled_buttons': False, 'icon_set': 'cyan',
    },
    'nebel': {
        'label': 's_da_nebel',
        'bg': '#131020', 'surface': '#1a1629', 'bar': '#201b32',
        'fg': '#ece8f5', 'sub': '#9a93ad', 'accent': '#a78bfa',
        'line': '#2a2440', 'line2': '#322b4b', 'field': '#0e0c18',
        'gold': '#e8c353', 'red': '#e05252', 'red_pale': '#c98a8a',
        'yellow': '#d8a03a', 'accent_dark': '#241c3d', 'hover': '#262040',
        'track': '#2f2847', 'track_on': '#352a5c', 'heat_low': '#201b32',
        'selected': '#221c36', 'danger_fill': '#2a1418',
        'scroll_groove': '#0c0a14', 'scroll_grip': '#6b6190',
        'scroll_grip_light': '#8f84b6',
        'control': '#1a1629', 'control_border': '#2a2440',
        'hover_fg': '#a78bfa',
        'square': False, 'upper_headings': False, 'accent_headings': False,
        'filled_buttons': False, 'icon_set': 'violett',
    },
    # ⚠ Rot ist hier der Akzent — die Fehlerfarbe (`red`) wird deshalb
    # gelb-orange, sonst sähe jede Warnung aus wie ein gewählter Reiter.
    'glut': {
        'label': 's_da_glut',
        'bg': '#151515', 'surface': '#1c1c1d', 'bar': '#222223',
        'fg': '#ebe6e4', 'sub': '#9d9592', 'accent': '#ff5a4f',
        'line': '#2c2a2a', 'line2': '#343131', 'field': '#0f0f0f',
        'gold': '#e8c353', 'red': '#ffa53a', 'red_pale': '#d9a870',
        'yellow': '#d8a03a', 'accent_dark': '#3a1614', 'hover': '#2a2727',
        'track': '#353131', 'track_on': '#4a1d1a', 'heat_low': '#222223',
        'selected': '#2a2222', 'danger_fill': '#2e1d08',
        'scroll_groove': '#0c0c0c', 'scroll_grip': '#6a6260',
        'scroll_grip_light': '#9a8e8a',
        'control': '#1c1c1d', 'control_border': '#2c2a2a',
        'hover_fg': '#ff5a4f',
        'square': False, 'upper_headings': False, 'accent_headings': False,
        'filled_buttons': False, 'icon_set': 'glut',
    },
    # Für schlechte Augen und helle Räume: Weiß auf Schwarz, kräftiges Gelb,
    # deutlich sichtbare Linien.
    'kontrast': {
        'label': 's_da_kontrast',
        'bg': '#000000', 'surface': '#0d0d0d', 'bar': '#141414',
        'fg': '#ffffff', 'sub': '#c8c8c8', 'accent': '#ffd400',
        'line': '#5a5a5a', 'line2': '#6e6e6e', 'field': '#000000',
        'gold': '#ffb000', 'red': '#ff6b6b', 'red_pale': '#ffa3a3',
        'yellow': '#ffb000', 'accent_dark': '#332b00', 'hover': '#262626',
        'track': '#444444', 'track_on': '#665500', 'heat_low': '#141414',
        'selected': '#2b2600', 'danger_fill': '#330000',
        'scroll_groove': '#000000', 'scroll_grip': '#9a9a9a',
        'scroll_grip_light': '#ffffff',
        'control': '#0d0d0d', 'control_border': '#5a5a5a',
        'hover_fg': '#ffd400',
        'square': False, 'upper_headings': False, 'accent_headings': False,
        'filled_buttons': False, 'icon_set': 'kontrast',
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
CONTROL = _S['control']
CONTROL_BORDER = _S['control_border']
HOVER_FG = _S['hover_fg']
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


def control_colors(fill, border):
    """Füllung und Rand eines Bedienelements, das die Grundlinie als Rand trägt.

    Im Schema `krt` werden daraus Control-Füllung und Control-Rand des
    Design-Systems; in den anderen Schemata bleiben beide Werte gleich.
    """
    if border and border.lower() == LINE.lower():
        border = CONTROL_BORDER
        if fill and fill.lower() == SURFACE.lower():
            fill = CONTROL
    return fill, border


def heading(text):
    """Eine Überschrift so, wie das Schema sie schreibt."""
    return text.upper() if UPPER_HEADINGS and text else text
