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
Die `user.cfg` je Spielkanal sichtbar machen — und eigene Zeilen pflegen.

Anlass (27.09.2026): *„man sieht nicht, was in der user.cfg eingetragen wird
und ist."* VerseKit schreibt dort seit langem `g_language` (siehe
`translation.set_user_cfg`), gezeigt wurde die Datei nie.

⚠⚠ **Die Sprachzeilen gehören VerseKit.** `g_language` und
`g_languageAudio` setzt das Programm selbst, abhängig von der gewählten
Textquelle, und prüft sie bei jedem Start (`_spielsprache_pruefen`). Wer sie
unter „Eigene Zeilen" änderte, würde beim nächsten Start still überschrieben
— deshalb stehen sie dort nicht, und `write_own` fasst sie nie an.

⚠ **Vor der ersten Änderung eine Sicherung** (`user.cfg.versekit-vorher`):
In der Datei stehen oft Grafikeinstellungen, die der Spieler mühsam
zusammengesucht hat. Die Sicherung wird nur einmal angelegt und danach nie
überschrieben — sie ist der Stand, bevor VerseKit dort etwas anders schrieb.
"""
import os
import re

from . import errors, paths

FILE = 'user.cfg'
BACKUP = 'user.cfg.versekit-vorher'
# Die Zeilen, die VerseKit selbst verwaltet.
MANAGED = ('g_language', 'g_languageAudio')
# `schlüssel = wert` — Punkte und Unterstriche im Schlüssel sind üblich
# (`pl_pit.forceSoftwareCursor`). Leere Zeilen und Kommentare (`;`, `--`)
# sind erlaubt.
_LINE = re.compile(r'^\s*[A-Za-z_][\w.]*\s*=\s*\S.*$')


def installed_channels():
    """Jeder Kanal, dessen Ordner es gibt — `[(name, ordner, installiert)]`.

    ⚠ Anders als `paths.available_channels` auch ohne `Game.log`: Ein frisch
    installierter PTU hat noch keine, ist aber da. `installiert` heißt, die
    `Data.p4k` liegt im Ordner."""
    seen, result = set(), []
    for base in paths._channel_bases():
        if not os.path.isdir(base):
            continue
        for name in paths.CHANNELS:
            folder = os.path.join(base, name)
            if folder in seen or not os.path.isdir(folder):
                continue
            seen.add(folder)
            result.append((name, folder,
                           os.path.isfile(os.path.join(folder, 'Data.p4k'))))
    order = {name: i for i, name in enumerate(paths.CHANNELS)}
    return sorted(result, key=lambda e: order.get(e[0], 99))


def read(folder):
    """Der Inhalt der `user.cfg` — `None`, wenn es keine gibt."""
    path = os.path.join(folder, FILE)
    try:
        with open(path, encoding='utf-8', errors='replace') as f:
            return f.read()
    except OSError:
        return None


def _key(line):
    return line.split('=', 1)[0].strip() if '=' in line else ''


def own_lines(text):
    """Alle Zeilen außer den von VerseKit verwalteten, ohne Leerzeilen."""
    return [z.rstrip() for z in (text or '').splitlines()
            if z.strip() and _key(z) not in MANAGED]


def status(folder):
    """Kurzbefund für die Blase je Kanal.

    `'keine'` (keine Installation), `'ohne_datei'`, `'ohne_sprache'`
    (Datei da, aber kein `g_language`), `'ok'`."""
    if not os.path.isfile(os.path.join(folder, 'Data.p4k')):
        return 'keine'
    text = read(folder)
    if text is None:
        return 'ohne_datei'
    if not any(_key(z) == 'g_language' for z in text.splitlines()):
        return 'ohne_sprache'
    return 'ok'


def check(lines):
    """Ungültige Zeilen -> Liste `(nummer, zeile)`; leer heißt alles gut."""
    bad = []
    for number, line in enumerate(lines, 1):
        stripped = line.strip()
        if not stripped or stripped.startswith((';', '--')):
            continue
        if not _LINE.match(stripped):
            bad.append((number, line))
    return bad


def write_own(folder, lines):
    """Die eigenen Zeilen schreiben, die verwalteten bleiben, wie sie sind.

    Gibt `(ok, fehlerhafte_zeilen)` zurück. Mit fehlerhaften Zeilen wird
    **nichts** geschrieben — eine halb übernommene Datei wäre schlimmer als
    keine Änderung."""
    bad = check(lines)
    if bad:
        return False, bad
    path = os.path.join(folder, FILE)
    old = read(folder) or ''
    managed = [z.rstrip() for z in old.splitlines() if _key(z) in MANAGED]
    fresh = [z.rstrip() for z in lines
             if z.strip() and _key(z) not in MANAGED] + managed
    try:
        backup = os.path.join(folder, BACKUP)
        if old and not os.path.exists(backup):
            with open(backup, 'w', encoding='utf-8') as f:
                f.write(old)
        with open(path + '.tmp', 'w', encoding='utf-8') as f:
            f.write('\n'.join(fresh) + '\n')
        os.replace(path + '.tmp', path)
        return True, []
    except OSError as exception:
        errors.record('usercfg.write_own', exception, path)
        return False, []
