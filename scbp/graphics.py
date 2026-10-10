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
    ('UpscalingModel', 'wahl', 0, ((0, 's_gr_modell_0'), (1, 's_gr_modell_1'))),
    ('MotionBlur', 'schalter', 1, ()),
    ('FilmGrain', 'schalter', 1, ()),
    ('Sharpening', 'regler', 0.0, ()),
    ('ChromaticAberration', 'regler', 1.0, ()),
)

_LINE = re.compile(r'^[ \t]*<Attr\s+name="([^"]*)"\s+value="([^"]*)"\s*/>[ \t]*\r?\n?',
                   re.M)


# Die Qualitätszeilen im Grafikmenü des Spiels, in dessen Reihenfolge.
# `SysSpec_Particles` und `SysSpec_PlanetTerrainVirtualTextures` stehen in der
# Datei, aber nicht im Menü — sie fehlen hier.
QUALITY_ROWS = (
    ('SysSpec_ObjectDetail', 's_gr_q_objektdetails'),
    ('SysSpec_ObjectViewDistance', 's_gr_q_sichtweite'),
    ('SysSpec_TextureQuality', 's_gr_q_texturqualitaet'),
    ('SysSpec_TextureDetail', 's_gr_q_texturdetails'),
    ('SysSpec_TextureGround', 's_gr_q_bodentexturen'),
    ('SysSpec_TextureFiltering', 's_gr_q_texturfilter'),
    ('SysSpec_ShadowMaps', 's_gr_q_schatten'),
    ('SysSpec_ShadowScreenSpace', 's_gr_q_schatten_bild'),
    ('SysSpec_PlanetVolumetricClouds', 's_gr_q_wolken'),
    ('SysSpec_GasCloud', 's_gr_q_gaswolken'),
    ('SysSpec_Fog', 's_gr_q_nebel'),
    ('SysSpec_WaterCaustics', 's_gr_q_wasserkaustik'),
    ('SysSpec_WaterSim', 's_gr_q_wassersim'),
    ('SysSpec_Shading', 's_gr_q_shader'),
    ('SysSpec_PostProcessing', 's_gr_q_nachbearbeitung'),
    ('SysSpec_VideoComms', 's_gr_q_video'),
)

# Was jede Voreinstellung je Zeile setzt — aus der Vorlage des Spiels
# (`Engine/Config/CVarGroups/sys_spec_Full.cfg`, Abschnitte 1 bis 5). Nicht
# genannte Zeilen stehen auf der Stufe selbst.
_PRESET_EXCEPTIONS = {
    2: {'SysSpec_TextureFiltering': 3, 'SysSpec_TextureGround': 3},
    4: {'SysSpec_TextureDetail': 3, 'SysSpec_TextureFiltering': 3,
        'SysSpec_TextureGround': 3},
    5: {'SysSpec_GasCloud': 4, 'SysSpec_PlanetVolumetricClouds': 4,
        'SysSpec_PostProcessing': 4, 'SysSpec_Shading': 4,
        'SysSpec_ShadowScreenSpace': 4, 'SysSpec_TextureDetail': 3,
        'SysSpec_TextureFiltering': 3, 'SysSpec_TextureGround': 3,
        'SysSpec_VideoComms': 4, 'SysSpec_WaterCaustics': 4,
        'SysSpec_WaterSim': 4, 'SysSpec_Particles': 4},
}

# Die Gesamtqualität (`SysSpec`) — in der Datei nur eine Merkzahl; was gilt,
# stehen in den Zeilen. `preset_rows` setzt sie so, wie es das Spiel bei der
# Wahl einer Voreinstellung tut.
OVERALL = 'SysSpec'


def preset_rows(level):
    """{Zeile: Stufe} für alle Zeilen aus `EDIT_ROWS` bei Voreinstellung `level`."""
    return {row: preset_level(level, row) for row, _key in EDIT_ROWS}


# Zum Einstellen: alle Qualitätszeilen der Datei, mit ihrer höchsten Stufe —
# aus den Abschnitten von `Engine/Config/CVarGroups/sys_spec_<Zeile>.cfg`
# (1, 2, Standard 3, 4 und bei manchen 5). Dazu die zwei Zeilen, die im
# Spielmenü nicht stehen; für die Hinweise (`compare`) zählen sie nicht.
EDIT_ROWS = QUALITY_ROWS + (
    ('SysSpec_Particles', 's_gr_q_partikel'),
    ('SysSpec_PlanetTerrainVirtualTextures', 's_gr_q_planetentexturen'),
)
TOP_LEVEL = {row: 4 for row, _key in EDIT_ROWS}
TOP_LEVEL.update({row: 5 for row in (
    'SysSpec_ObjectDetail', 'SysSpec_ObjectViewDistance',
    'SysSpec_TextureQuality', 'SysSpec_ShadowMaps',
    'SysSpec_PlanetVolumetricClouds', 'SysSpec_Fog',
    'SysSpec_PlanetTerrainVirtualTextures', 'SysSpec')})


def read_levels(path=None):
    """{Zeile: Stufe} für alle Zeilen aus `EDIT_ROWS`, die in der Datei stehen."""
    path = path or attribute_file()
    if not path:
        return {}
    try:
        with open(path, 'r', encoding='utf-8', newline='') as f:
            text = f.read()
    except OSError as exc:
        errors.record('graphics.read_levels', exc)
        return {}
    found = {m.group(1): _number(m.group(2)) for m in _LINE.finditer(text)}
    rows = [row for row, _key in EDIT_ROWS] + [OVERALL]
    return {row: found[row] for row in rows
            if isinstance(found.get(row), int) and 1 <= found[row] <= 5}


def preset_level(machine_class, row):
    """Die Stufe, die die Voreinstellung `machine_class` (1–5) für `row` setzt."""
    return _PRESET_EXCEPTIONS.get(machine_class, {}).get(row, machine_class)


def read_quality(path=None):
    """{Zeile: Stufe 1–5} für die Zeilen aus `QUALITY_ROWS`, die in der Datei
    stehen. Fehlt eine Zeile, folgt sie der Voreinstellung — sie fehlt dann
    auch hier."""
    path = path or attribute_file()
    if not path:
        return {}
    try:
        with open(path, 'r', encoding='utf-8', newline='') as f:
            text = f.read()
    except OSError as exc:
        errors.record('graphics.read_quality', exc)
        return {}
    found = {m.group(1): _number(m.group(2)) for m in _LINE.finditer(text)}
    return {row: found[row] for row, _key in QUALITY_ROWS
            if isinstance(found.get(row), int) and 1 <= found[row] <= 5}


def compare(levels, machine_class):
    """Die eigenen Zeilen gegen die Voreinstellung des Spiels.

    Gibt `(darüber, darunter)` zurück — je eine Liste `(zeile, ist, soll)`
    in der Reihenfolge des Menüs."""
    above, below = [], []
    for row, _key in QUALITY_ROWS:
        if row not in levels:
            continue
        wanted = preset_level(machine_class, row)
        if levels[row] > wanted:
            above.append((row, levels[row], wanted))
        elif levels[row] < wanted:
            below.append((row, levels[row], wanted))
    return above, below


RENDERER = 'GraphicsRenderer'
RENDERER_CHOICES = ((1, 's_gr_renderer_1'), (0, 's_gr_renderer_0'))


def renderer_file():
    """`GraphicsSettings.json` der zuletzt gestarteten Spielversion, oder ''.

    Liegt nicht im Spielordner, sondern unter
    `%LOCALAPPDATA%/Star Citizen/starcitizen_(sc-alpha-<version>)_<kennung>/
    GraphicsSettings/` — ein Ordner je Spielversion. Genommen wird der Ordner,
    den das Spiel zuletzt angefasst hat."""
    base = os.environ.get('LOCALAPPDATA') or ''
    if not base:
        return ''
    candidates = []
    try:
        for top in os.listdir(base):
            if top.lower() != 'star citizen':
                continue
            root = os.path.join(base, top)
            for name in os.listdir(root):
                path = os.path.join(root, name, 'GraphicsSettings',
                                    'GraphicsSettings.json')
                if name.lower().startswith('starcitizen_') \
                        and os.path.isfile(path):
                    candidates.append((os.path.getmtime(os.path.join(root, name)),
                                       path))
    except OSError:
        return ''
    return max(candidates)[1] if candidates else ''


def read_renderer(path=None):
    """1 = Vulkan, 0 = DirectX 11 — oder None, wenn die Datei fehlt."""
    import json
    path = path or renderer_file()
    if not path:
        return None
    try:
        with open(path, encoding='utf-8') as f:
            value = json.load(f)['GraphicsSettings'][RENDERER]
    except (OSError, ValueError, KeyError, TypeError) as exc:
        errors.record('graphics.read_renderer', exc)
        return None
    return value if value in (0, 1) else None


def write_renderer(value, path=None):
    """Renderer setzen, vorher sichern. `(erfolg, sicherung_oder_textschluessel)`."""
    import json
    path = path or renderer_file()
    if not path:
        return False, 's_gr_f_keine_datei'
    if value not in (0, 1):
        raise ValueError('renderer must be 0 or 1')
    try:
        with open(path, encoding='utf-8') as f:
            data = json.load(f)
        settings = data['GraphicsSettings']
    except (OSError, ValueError, KeyError, TypeError) as exc:
        errors.record('graphics.write_renderer_read', exc)
        return False, 's_gr_f_lesen'
    if settings.get(RENDERER) == value:
        return True, ''
    settings[RENDERER] = value
    backup = '%s.scbpw-%s' % (path, time.strftime('%Y%m%d-%H%M%S'))
    try:
        shutil.copy2(path, backup)
    except OSError as exc:
        errors.record('graphics.renderer_backup', exc)
        return False, 's_gr_f_sicherung'
    try:
        with open(path, 'w', encoding='utf-8', newline='\n') as f:
            json.dump(data, f, indent=2)
            f.write('\n')
    except OSError as exc:
        try:
            shutil.copy2(backup, path)
        except OSError:
            pass
        errors.record('graphics.write_renderer', exc)
        return False, 's_gr_f_schreiben'
    return True, os.path.basename(backup)


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
    # Qualitätszeilen: `None` nimmt die Zeile heraus — dann folgt die Zeile
    # der Gesamtstufe des Spiels.
    defaults.update({row: None for row, _key in EDIT_ROWS})
    defaults[OVERALL] = None
    unknown = [name for name in changes if name not in defaults]
    bad = [name for name, value in changes.items()
           if name in TOP_LEVEL and value is not None
           and not (isinstance(value, int) and 1 <= value <= TOP_LEVEL[name])]
    if bad:
        raise ValueError('level out of range: %s' % ', '.join(bad))
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
