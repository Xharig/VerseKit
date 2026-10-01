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
Shader-Zwischenspeicher von Star Citizen und den Grafiktreibern leeren.

Das Spiel legt unter `AppData\\Local\\Star Citizen` je Spielversion einen
Ordner `starcitizen_(sc-alpha-…)_…` an. Darin:

| Unterordner | Inhalt | Was passiert |
|---|---|---|
| `shaders` | übersetzte Shader | wird geleert |
| `vulkanshadercache` | Vulkan-Pipeline-Speicher | wird geleert |
| `GraphicsSettings` | Grafikeinstellungen des Spielers | **bleibt** |

Daneben `crashes` (Absturzberichte, werden geleert) und die Speicher der
Grafiktreiber (`paths.gpu_cache_dirs`). Geleert wird nur der **Inhalt** dieser
Ordner; die Ordner selbst bleiben stehen. Dateien, die gerade benutzt werden,
bleiben liegen und werden gezählt.
"""
import os
import shutil

from . import paths

VERSION_PREFIX = 'starcitizen_'
SC_CACHE_SUBDIRS = ('shaders', 'vulkanshadercache')
CRASH_DIR = 'crashes'

KIND_SC = 'sc'
KIND_CRASH = 'crash'
KIND_GPU = 'gpu'


def _size(path):
    """Gesamtgröße aller Dateien unter `path` in Byte."""
    total = 0
    for top, _dirs, files in os.walk(path):
        for name in files:
            try:
                total += os.path.getsize(os.path.join(top, name))
            except OSError:
                pass
    return total


def find(sc_roots=None, gpu_dirs=None):
    """Alles, was sich leeren lässt, als Liste `(Art, Ordner, Byte)`.

    Ohne Angaben werden die echten Orte über `paths` gesucht."""
    if sc_roots is None:
        sc_roots = paths.sc_cache_roots()
    if gpu_dirs is None:
        gpu_dirs = paths.gpu_cache_dirs()
    targets = []
    for root in sc_roots:
        try:
            names = sorted(os.listdir(root))
        except OSError:
            continue
        for name in names:
            version_dir = os.path.join(root, name)
            if not (name.lower().startswith(VERSION_PREFIX)
                    and os.path.isdir(version_dir)):
                continue
            for sub in SC_CACHE_SUBDIRS:
                path = os.path.join(version_dir, sub)
                if os.path.isdir(path):
                    targets.append((KIND_SC, path, _size(path)))
        crash = os.path.join(root, CRASH_DIR)
        if os.path.isdir(crash):
            targets.append((KIND_CRASH, crash, _size(crash)))
    for path in gpu_dirs:
        if os.path.isdir(path):
            targets.append((KIND_GPU, path, _size(path)))
    return targets


def total_by_kind(targets):
    """Byte je Art als Wörterbuch `{Art: Byte}`."""
    totals = {KIND_SC: 0, KIND_CRASH: 0, KIND_GPU: 0}
    for kind, _path, size in targets:
        totals[kind] = totals.get(kind, 0) + size
    return totals


def _clear_dir(path):
    """Leert `path`, lässt den Ordner selbst stehen. Liefert
    `(freigegebene Byte, übersprungene Dateien)`."""
    freed, skipped = 0, 0
    try:
        names = os.listdir(path)
    except OSError:
        return 0, 1
    for name in names:
        full = os.path.join(path, name)
        if os.path.isdir(full) and not os.path.islink(full):
            sub_freed, sub_skipped = _clear_dir(full)
            freed += sub_freed
            skipped += sub_skipped
            if not sub_skipped:
                try:
                    os.rmdir(full)
                except OSError:
                    shutil.rmtree(full, ignore_errors=True)
            continue
        try:
            size = os.path.getsize(full)
        except OSError:
            size = 0
        try:
            os.remove(full)
            freed += size
        except OSError:
            skipped += 1
    return freed, skipped


def clear(targets):
    """Leert jeden Ordner aus `find()`. Liefert
    `(freigegebene Byte, übersprungene Dateien)`."""
    freed, skipped = 0, 0
    for _kind, path, _size_before in targets:
        f, s = _clear_dir(path)
        freed += f
        skipped += s
    return freed, skipped


def format_size(size, decimal=','):
    """Byte als lesbare Größe: `740 MB`, `14,9 GB` (mit `decimal` als
    Dezimalzeichen)."""
    gb = size / 1024 ** 3
    if gb >= 1:
        return ('%.1f GB' % gb).replace('.', decimal)
    return '%d MB' % round(size / 1024 ** 2)
