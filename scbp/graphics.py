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
Grafik-Einstellungen von Star Citizen in der `attributes.xml` lesen und
schreiben.

Die Datei liegt neben der `actionmaps.xml`
(`USER/Client/0/Profiles/default/attributes.xml`), je Zeile ein
`<Attr name="…" value="…"/>`.

⚠⚠ **Das Spiel speichert nur, was vom Grundwert abweicht.** Steht eine
Einstellung auf ihrem Grundwert, fehlt die Zeile ganz. Beim Lesen gilt eine
fehlende Zeile als Grundwert; beim Schreiben entfernt der Grundwert die Zeile,
jeder andere Wert setzt sie oder fügt sie ein. Die Grundwerte stammen aus
der Vorlage im Archiv (`Data/Libs/Config/Profiles/default/attributes.xml`).

⚠ Welche Zahl welcher Menüeintrag ist, steht in keiner Datei des Spiels —
die Tabellen unten sind im Spielmenü gemessen (Menü umstellen, Datei lesen).

⚠ Das Spiel schreibt die Datei, sobald man seine Einstellungen verlässt.
Geschrieben wird deshalb nur bei geschlossenem Spiel; der Aufrufer prüft das
(`auto_update.game_running()`), bevor er `write()` ruft.
"""
import os
import re
import shutil
import time

from . import errors

# (Name in der Datei, Art, Grundwert, Wahlmöglichkeiten)
# Art `wahl`: feste Werte mit Textschlüssel · `schalter`: 0/1 ·
# `regler`: 0…1, in der Oberfläche 0…100.
SETTINGS = (
    ('WindowMode', 'wahl', 0, ((0, 's_gr_fenster_0'), (1, 's_gr_fenster_1'),
                               (2, 's_gr_fenster_2'))),
    ('VSync', 'schalter', 1, ()),
    ('Upscaling', 'wahl', 0, ((0, 's_gr_up_0'), (1, 's_gr_up_1'),
                              (2, 's_gr_up_2'), (3, 's_gr_up_3'),
                              (4, 's_gr_up_4'))),
    ('UpscalingTechnique', 'wahl', 0, ((0, 's_gr_tech_0'), (1, 's_gr_tech_1'),
                                       (2, 's_gr_tech_2'))),
    ('MotionBlur', 'schalter', 1, ()),
    ('FilmGrain', 'schalter', 1, ()),
    ('Sharpening', 'regler', 0.0, ()),
    ('ChromaticAberration', 'regler', 1.0, ()),
)

_LINE = re.compile(r'^[ \t]*<Attr\s+name="([^"]*)"\s+value="([^"]*)"\s*/>[ \t]*\r?\n?',
                   re.M)


def attribute_file(game_folder=None):
    """Pfad der `attributes.xml` oder ''."""
    from . import fov
    return fov._attribute_file(game_folder)


def _number(text):
    try:
        value = float(text)
    except (TypeError, ValueError):
        return None
    return int(value) if value == int(value) else value


def _format(value):
    if isinstance(value, float) and value != int(value):
        return ('%.4f' % value).rstrip('0').rstrip('.')
    return str(int(value))


def read(path=None):
    """{Name: Wert} für alle Einstellungen aus `SETTINGS` — fehlende Zeilen
    stehen mit ihrem Grundwert da. None, wenn die Datei fehlt oder nicht
    lesbar ist."""
    path = path or attribute_file()
    if not path:
        return None
    try:
        with open(path, 'r', encoding='utf-8', newline='') as f:
            text = f.read()
    except OSError as exc:
        errors.record('graphics.read', exc)
        return None
    found = {m.group(1): m.group(2) for m in _LINE.finditer(text)}
    out = {}
    for name, kind, default, choices in SETTINGS:
        value = _number(found.get(name)) if name in found else default
        if value is None:
            value = default
        out[name] = value
    return out


def _apply(text, name, value, default):
    """`text` mit der Einstellung `name` auf `value` — Grundwert entfernt die
    Zeile, sonst wird sie ersetzt oder an ihrer Stelle im Alphabet eingefügt."""
    newline = '\r\n' if '\r\n' in text else '\n'
    matches = list(_LINE.finditer(text))
    own = [m for m in matches if m.group(1) == name]
    if value == default:
        for m in reversed(own):
            text = text[:m.start()] + text[m.end():]
        return text
    new_line = ' <Attr name="%s" value="%s"/>%s' % (name, _format(value), newline)
    if own:
        m = own[0]
        return text[:m.start()] + new_line + text[m.end():]
    after = [m for m in matches if m.group(1) > name]
    if after:
        at = after[0].start()
    elif matches:
        at = matches[-1].end()
    else:
        close = text.rfind('</Attributes>')
        if close < 0:
            raise ValueError('no </Attributes> in the file')
        at = close
    return text[:at] + new_line + text[at:]


def write(changes, path=None):
    """Die Werte aus `changes` ({Name: Wert}) in die Datei schreiben.

    Vorher wird die Datei als `attributes.xml.scbpw-<Zeit>` daneben gesichert.
    Liefert `(erfolg, sicherung_oder_textschluessel)`."""
    path = path or attribute_file()
    if not path:
        return False, 's_gr_f_keine_datei'
    defaults = {name: default for name, _k, default, _c in SETTINGS}
    unknown = [name for name in changes if name not in defaults]
    if unknown:
        raise ValueError('unknown setting: %s' % ', '.join(unknown))
    try:
        with open(path, 'r', encoding='utf-8', newline='') as f:
            text = f.read()
    except OSError as exc:
        errors.record('graphics.write_read', exc)
        return False, 's_gr_f_lesen'
    fresh = text
    for name, value in changes.items():
        fresh = _apply(fresh, name, value, defaults[name])
    if fresh == text:
        return True, ''
    backup = '%s.scbpw-%s' % (path, time.strftime('%Y%m%d-%H%M%S'))
    try:
        shutil.copy2(path, backup)
    except OSError as exc:
        errors.record('graphics.backup', exc)
        return False, 's_gr_f_sicherung'
    try:
        with open(path, 'w', encoding='utf-8', newline='') as f:
            f.write(fresh)
    except OSError as exc:
        try:
            shutil.copy2(backup, path)
        except OSError:
            pass
        errors.record('graphics.write', exc)
        return False, 's_gr_f_schreiben'
    return True, os.path.basename(backup)
