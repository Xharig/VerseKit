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
Was Star Citizen beim Start über den Rechner ins `Game.log` schreibt.

Das Spiel misst beim Start selbst und schreibt unter anderem:

| Zeile | Inhalt |
|---|---|
| `Host CPU: …`, `Logical CPU Count: …` | Prozessor, Threads |
| `…MB physical memory installed` | Arbeitsspeicher |
| `[VK] Chosen Vulkan GPU Device (…) Driver Version (…) Vulkan API (…)` | Grafikkarte, Treiber |
| `GPU: DedicatedVidMemMB = …` | Grafikspeicher |
| `CPU benchmark: … ms (int+mem), … ms (fp+mem)` / `GPU benchmark: … ms` | Messzeiten |
| `Performance Index: … (CPU), … (GPU)` | Index des Spiels |
| `Final rating: Machine class N` | Einstufung des Spiels, 1 bis 5 |

Die Einstufung entsteht nur, wenn das Spiel seine automatische Erkennung
laufen lässt (erster Start, Wahl einer Voreinstellung). Sie wird deshalb auch
in den aufgehobenen Logs gesucht, neueste zuerst.

Die Stufen 1 bis 5 sind die Stufen der Grafik-Vorlagen im Archiv
(`Engine/Config/CVarGroups/sys_spec_Full.cfg`: Abschnitte 1, 2, 3, 4, 5),
im Menü Low, Medium, High, Very High, Ultra.
"""
import os
import re

from . import errors, paths

CLASSES = 5

_PATTERNS = {
    'cpu': re.compile(r'Host CPU: (.+)'),
    'threads': re.compile(r'Logical CPU Count: (\d+)'),
    'ram_mb': re.compile(r'(\d+)MB physical memory installed'),
    'gpu_vk': re.compile(r'\[VK\] Chosen Vulkan GPU Device \((.+?)\) Driver '
                         r'Version \((.+?)\) Vulkan API \((.+?)\)'),
    'vram_mb': re.compile(r'GPU: DedicatedVidMemMB = (\d+)'),
    'display': re.compile(r'Current display mode is (\d+)x(\d+)'),
    'dlss': re.compile(r'GPU: DLSS Support = (\w+)'),
    'cpu_bench': re.compile(r'CPU benchmark: ([\d.]+) ms \(int\+mem\), '
                            r'([\d.]+) ms \(fp\+mem\)'),
    'gpu_bench': re.compile(r'GPU benchmark: ([\d.]+) ms'),
    'index': re.compile(r'Performance Index: ([\d.]+) \(CPU\), '
                        r'([\d.]+) \(GPU\)'),
}
_CLASS = re.compile(r'Final rating: Machine class (\d+)')
_FIRST_ADAPTER = re.compile(r'^<[^>]*> - (.+?) \(vendor = ', re.M)

# Bis hierher steht der Startblock — weiter wird für die Hardware nicht gelesen.
_HEAD_BYTES = 600000
# So viele aufgehobene Logs werden nach der Einstufung durchsucht.
_BACKUPS_SEARCHED = 40


def _text(path, limit=None):
    with open(path, 'rb') as f:
        data = f.read(limit) if limit else f.read()
    return data.decode('utf-8', 'replace')


def parse(text):
    """Die Werte aus einem Log-Text — fehlende stehen als None da."""
    out = {'cpu': None, 'threads': None, 'ram_mb': None, 'gpu': None,
           'driver': None, 'vulkan': None, 'vram_mb': None, 'display': None,
           'dlss': None, 'cpu_bench': None, 'gpu_bench': None,
           'cpu_index': None, 'gpu_index': None, 'machine_class': None}
    hits = {k: p.search(text) for k, p in _PATTERNS.items()}
    if hits['cpu']:
        out['cpu'] = hits['cpu'].group(1).strip()
    if hits['threads']:
        out['threads'] = int(hits['threads'].group(1))
    if hits['ram_mb']:
        out['ram_mb'] = int(hits['ram_mb'].group(1))
    if hits['gpu_vk']:
        out['gpu'], out['driver'], out['vulkan'] = (
            g.strip() for g in hits['gpu_vk'].groups())
    else:
        first = _FIRST_ADAPTER.search(text)
        if first:
            out['gpu'] = first.group(1).strip()
    if hits['vram_mb']:
        out['vram_mb'] = int(hits['vram_mb'].group(1))
    if hits['display']:
        out['display'] = (int(hits['display'].group(1)),
                          int(hits['display'].group(2)))
    if hits['dlss']:
        out['dlss'] = hits['dlss'].group(1) == 'Available'
    if hits['cpu_bench']:
        out['cpu_bench'] = tuple(float(g) for g in hits['cpu_bench'].groups())
    if hits['gpu_bench']:
        out['gpu_bench'] = float(hits['gpu_bench'].group(1))
    if hits['index']:
        out['cpu_index'] = float(hits['index'].group(1))
        out['gpu_index'] = float(hits['index'].group(2))
    classes = _CLASS.findall(text)
    if classes:
        value = int(classes[-1])
        if 1 <= value <= CLASSES:
            out['machine_class'] = value
    return out


# Gemerkt je Datei-Stand `(Pfad, Änderungszeit in ns, Größe)`: Ändert sich die
# Datei, ändert sich der Schlüssel, und es wird neu gelesen. Für die laufende
# `Game.log` steht das Paar (Stand, Werte) unter einem Schlüssel, damit ein
# anderer Faden nie den neuen Stand mit alten Werten liest.
#
# ⚠ Grafik-Seite und Übersicht lesen hier in Nebenfäden. Ein Nebenfaden, der
# Logs ganz durchsucht, macht dem Oberflächen-Faden das GIL streitig — jeder
# Tk-Aufruf der Seite wartet dann mit.
_log_memo = {'eintrag': None}
_class_memo = {}


def _stamp(path):
    info = os.stat(path)
    return (path, info.st_mtime_ns, info.st_size)


def _class_in(path):
    """Die letzte gültige Einstufung in einem aufgehobenen Log, oder None."""
    stamp = _stamp(path)
    if stamp in _class_memo:
        return _class_memo[stamp]
    found = None
    for value in reversed(_CLASS.findall(_text(path))):
        if 1 <= int(value) <= CLASSES:
            found = int(value)
            break
    _class_memo[stamp] = found
    return found


def _class_from_backups(folder=None):
    """Die jüngste Einstufung aus den aufgehobenen Logs, oder None."""
    try:
        backups = paths.log_backups(folder)
    except Exception as exc:
        errors.record('machine_info.backups', exc)
        return None
    searched = list(reversed(backups))[:_BACKUPS_SEARCHED]
    # Nur behalten, was noch durchsucht wird — sonst wächst das Gedächtnis
    # mit jedem Spielstart.
    # `list()` und `pop()`: Grafik-Seite und Übersicht lesen in eigenen
    # Fäden, womöglich gleichzeitig.
    keep = set(searched)
    for stamp in [s for s in list(_class_memo) if s[0] not in keep]:
        _class_memo.pop(stamp, None)
    for path in searched:
        try:
            found = _class_in(path)
        except OSError:
            continue
        if found is not None:
            return found
    return None


def read(folder=None):
    """Die Werte aus der laufenden `Game.log` (Hardware aus dem Startblock,
    Einstufung aus dem ganzen Log oder den aufgehobenen). None ohne Log."""
    log = paths.game_log(folder)
    if not log:
        return None
    try:
        stamp = _stamp(log)
        entry = _log_memo['eintrag']
        if entry is not None and entry[0] == stamp:
            out = dict(entry[1])
        else:
            head = _text(log, _HEAD_BYTES)
            out = parse(head)
            if out['machine_class'] is None and stamp[2] > _HEAD_BYTES:
                out['machine_class'] = parse(_text(log))['machine_class']
            _log_memo['eintrag'] = (stamp, dict(out))
    except OSError as exc:
        errors.record('machine_info.read', exc)
        return None
    if out['machine_class'] is None:
        out['machine_class'] = _class_from_backups(folder)
    return out
