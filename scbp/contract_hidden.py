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
Von Hand ausgeblendete Aufträge — über einen Neustart hinweg.

Gemerkt wird die MissionId, nicht der Titel: Nimmt man denselben Auftrag
später noch einmal an, bekommt er eine neue Kennung und erscheint wieder.
Beim Start liest der Watcher die ganze `Game.log` nach; ohne diese Liste
stünde ein ausgeblendeter Auftrag danach wieder da.
"""
import json
import os

from . import paths

FILE = 'auftraege-ausgeblendet.json'
# Wie viele Kennungen höchstens aufgehoben werden, die neuesten zuletzt.
KEEP = 300


def _file():
    return paths.app_file(FILE)


def load():
    """Die ausgeblendeten MissionIds als Menge."""
    try:
        with open(_file(), encoding='utf-8') as f:
            data = json.load(f)
    except (OSError, ValueError):
        return set()
    ids = data.get('ids') if isinstance(data, dict) else None
    return {i for i in ids or () if isinstance(i, str) and i}


def add(mission_ids):
    """Kennungen dazunehmen und speichern. Gibt True zurück, wenn es klappte."""
    new = [i for i in mission_ids or () if i]
    if not new:
        return False
    try:
        with open(_file(), encoding='utf-8') as f:
            old = (json.load(f) or {}).get('ids') or []
    except (OSError, ValueError, AttributeError):
        old = []
    ids = [i for i in old if i not in new] + new
    target = _file()
    tmp = target + '.neu'
    try:
        with open(tmp, 'w', encoding='utf-8') as f:
            json.dump({'version': 1, 'ids': ids[-KEEP:]}, f, indent=1)
        os.replace(tmp, target)
        return True
    except OSError as exc:
        from . import errors
        errors.record('contract_hidden.add', exc)
        return False


def hidden(mission_ids, known=None):
    """Sind alle diese Kennungen ausgeblendet? Ohne Kennung: nein."""
    ids = [i for i in mission_ids or () if i]
    if not ids:
        return False
    known = load() if known is None else known
    return all(i in known for i in ids)
