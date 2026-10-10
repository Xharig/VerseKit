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
Die Galerie „Was ist neu" — einmal nach einem Update, dann weggeklickt.

Je Version eine Handvoll Höhepunkte mit Bild, Titel, kurzem Text und dem
Bedienweg; danach die übrigen Punkte des Changelogs als Liste. Die Texte
stehen in `language.py`, die Bilder unter `assets/galerie/<bild>.png`
(`-en` für Englisch), gebaut von `tools/galerie_bilder.py` aus den
Webseiten-Bildern.

Erscheinen: wenn das Hauptfenster aufgeht und die eigene Version Höhepunkte
hat, die noch nicht gezeigt wurden. Eine frische Installation bekommt keine
Galerie — wer nichts verpasst hat, braucht keine Neuigkeiten.
"""
import os
import re
import sys

from . import paths

SETTING = 'galerie_gesehen'

# Je Version die Höhepunkte, in der Reihenfolge der Galerie.
# `art`: neu/bess/fix (wie im Changelog) · `bild`: Name unter assets/galerie ·
# `reiter`: Kennung der Seite, die „Ansehen" öffnet · `changelog`: Anfang des
# fett gesetzten Titels im Changelog (de, en) — diese Punkte fehlen dann in
# der Schlussliste der Galerie.
HIGHLIGHTS = {
    '3.97.0': (
        {'art': 'neu', 'bild': 'grafik', 'reiter': 'grafik',
         'titel': 'gal_grafik_t', 'text': 'gal_grafik_x', 'weg': 'gal_grafik_w',
         'changelog': ('Grafik', 'Graphics')},
        {'art': 'neu', 'bild': 'bedarf', 'reiter': 'bedarf',
         'titel': 'gal_bedarf_t', 'text': 'gal_bedarf_x', 'weg': 'gal_bedarf_w',
         'changelog': ('Bedarf der Einheit', 'Unit demand')},
        {'art': 'neu', 'bild': 'lager', 'reiter': 'lager',
         'titel': 'gal_raffinerie_t', 'text': 'gal_raffinerie_x',
         'weg': 'gal_raffinerie_w',
         'changelog': ('Raffinerie per Tastenkombination',
                       'Read the refinery with a shortcut')},
        {'art': 'bess', 'bild': 'shader', 'reiter': 'shader',
         'titel': 'gal_spiel_t', 'text': 'gal_spiel_x', 'weg': 'gal_spiel_w',
         'changelog': ('Spiel-Einstellungen in einer eigenen Gruppe',
                       'Game settings in their own group')},
    ),
}


def _parts(version):
    m = re.search(r'(\d+)\.(\d+)\.(\d+)', str(version or ''))
    return tuple(int(x) for x in m.groups()) if m else (0, 0, 0)


def _series(version):
    """(Haupt, Neben) — Patch-Versionen gehören zur Galerie ihrer Reihe."""
    return _parts(version)[:2]


def highlights(version):
    """Die Höhepunkte der Reihe dieser Version (3.97.1 → 3.97.0), oder ()."""
    for key, points in HIGHLIGHTS.items():
        if _series(key) == _series(version):
            return points
    return ()


def due(version):
    """Soll die Galerie jetzt erscheinen?

    Einmal je Reihe: Wer sie unter 3.97.0 weggeklickt hat, bekommt sie unter
    3.97.1 nicht noch einmal. Merkt sich bei einer frischen Installation die
    Version, ohne zu zeigen — erst das nächste Update bringt eine Galerie."""
    if not highlights(version):
        return False
    seen = paths.setting(SETTING)
    if seen:
        return _series(seen) < _series(version)
    from . import news
    if news._read().get('zuletzt'):
        return True
    mark_shown(version)
    return False


def mark_shown(version):
    """Galerie für diese Version weggeklickt."""
    return paths.set_setting(SETTING, '%d.%d.%d' % _parts(version))


def image_path(name, language):
    """Pfad des Bildes in der Sprache, sonst des deutschen, sonst ''."""
    base = getattr(sys, '_MEIPASS', None) or os.path.dirname(
        os.path.dirname(os.path.abspath(__file__)))
    folder = os.path.join(base, 'assets', 'galerie')
    names = ([name + '-en.png'] if language == 'en' else []) + [name + '.png']
    for file_name in names:
        path = os.path.join(folder, file_name)
        if os.path.isfile(path):
            return path
    return ''


def _title(line):
    """Der fett gesetzte Titel eines Changelog-Punkts, sonst die Zeile."""
    m = re.match(r'\s*\*\*(.+?)\*\*', line)
    return m.group(1).strip() if m else line.strip()


def rest(version, language):
    """Die übrigen Punkte der Reihe: `[(art, titel)]` ohne die Höhepunkte.

    Gelesen werden alle Changelog-Einträge der Reihe bis zur eigenen
    Version (3.97.0 und 3.97.1), ältere zuerst."""
    from . import updater
    own = _parts(version)
    entries = sorted((e for e in updater.history()
                      if _series(e.get('version')) == _series(version)
                      and _parts(e.get('version')) <= own),
                     key=lambda e: _parts(e.get('version')))
    index = 1 if language == 'en' else 0
    skip = [h['changelog'][index] for h in highlights(version)]
    out = []
    for entry in entries:
        for kind, line in updater.points_by_kind(entry.get('text') or ''):
            title = _title(line)
            if any(title.startswith(s) for s in skip) or (kind, title) in out:
                continue
            out.append((kind, title))
    return out
