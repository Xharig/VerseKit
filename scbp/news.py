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
„Neu"-Marken an den Bereichen, die eine Version mitgebracht hat.

Eine Änderungsliste liest kaum jemand. Eine kleine Marke am Reiter dagegen sieht
man beim ersten Blick — und sie führt den Spieler genau dorthin, wo das Neue
liegt, statt es ihm zu beschreiben.

Damit das trägt, gelten zwei Regeln:

  1. **Die Marke verschwindet, sobald der Bereich einmal offen war.** Ohne das
     wäre nach drei Versionen alles markiert, und niemand schaut mehr hin. Eine
     Marke, die bleibt, ist Deko; eine, die verschwindet, ist eine Nachricht.
  2. **Bei einer frischen Installation wird nichts markiert.** Für einen
     Neuling ist alles neu — Marken an jedem Reiter wären dort nur Lärm. Sie
     erscheinen nur, wenn jemand von einer älteren Version kommt.

Gepflegt wird nur die Tabelle unten: Bereich -> in welcher Version kam er dazu.
Der Rest ergibt sich.

⚠ Bis zum 11.09.2026 hieß dieses Modul `neuheiten` (Sprachumstellung P3:
Bezeichner englisch, Kommentare deutsch). **Der Dateiname `gesehen.json`, die
Schlüssel `zuletzt` und `bereiche` und die Bereichsnamen in `NEW_SINCE` sind
dabei bewusst gleich geblieben** — sie stehen in der Datei jedes Nutzers.
Umbenannt, verlöre jeder beim Update seine gesehenen Marken, und alles wäre
wieder markiert.
"""
import json

from . import pfade

FILE = 'gesehen.json'

# Welcher Bereich kam mit welcher Version? Beim Bauen eines neuen Bereichs hier
# **eine Zeile ergänzen** — mehr ist nicht zu tun.
NEW_SINCE = {
    'asop':        '3.28.0',   # eigene Schiffsnamen im Fleet Manager
    'patchaenderungen': '3.24.0',  # was ein Spiel-Patch an Werten geändert hat
    # Die Schiffs-Gruppe, alle drei aus v3.19.0
    'hangar':      '3.19.0',   # Mein Hangar: welche Schiffe mir gehören
    'wunschliste': '3.19.0',   # was ich mir vornehme, mit Preis und Ort
    'bergung':     '3.19.0',   # was in einem Wrack steckt und was es wert ist
    'auftragslog': '3.12.0',   # Auftrags-Protokoll: was wann gespielt wurde
    'herstellung': '3.3.0',
    'bergbau': '3.3.0',
    'lager': '3.3.0',
    'liste':       '3.0.0',    # Bauplan-Liste im neuen Fenster
    'fortschritt': '3.0.0',    # Fortschritt je Art
    'bestand':     '3.0.0',    # Bestand einlesen und ausgeben
    'wasistneu':   '3.0.0',    # Änderungen als eigener Reiter
    'ueber':       '3.0.0',    # Version, Testkanal, Autor
    'diagnose':    '3.0.0',    # Fehlerbericht und Melden
    'serverstatus': '3.0.0',   # Lage der CIG-Server als eigener Reiter
    'danke':       '3.0.0',    # wem was gehört, und Dank an die Beteiligten
}


def _parts(version):
    """'2.2.0' -> (2, 2, 0); alles Unlesbare wird zu (0, 0, 0)."""
    numbers = []
    for piece in str(version or '').split('-')[0].split('.'):
        try:
            numbers.append(int(piece))
        except ValueError:
            numbers.append(0)
    while len(numbers) < 3:
        numbers.append(0)
    return tuple(numbers[:3])


def _read():
    try:
        with open(pfade.app_datei(FILE), encoding='utf-8') as f:
            data = json.load(f)
        return data if isinstance(data, dict) else {}
    except Exception:
        return {}


def _write(data):
    try:
        with open(pfade.app_datei(FILE), 'w', encoding='utf-8') as f:
            json.dump(data, f, ensure_ascii=False, indent=1)
        return True
    except Exception as exc:
        try:
            from . import fehler
            fehler.merken('news.write', exc)
        except Exception:
            pass
        return False


def first_start(own_version):
    """Merkt sich beim allerersten Lauf die Version — ohne Marken zu setzen.

    Genau hier entscheidet sich Regel 2: Wer frisch installiert, hat nichts
    verpasst und bekommt deshalb auch nichts markiert.
    """
    data = _read()
    if 'zuletzt' not in data:
        data['zuletzt'] = str(own_version or '')
        data['bereiche'] = {k: str(own_version or '') for k in NEW_SINCE}
        _write(data)
        return True
    return False


def is_new(area, own_version):
    """Soll an diesem Bereich eine Marke stehen?"""
    since = NEW_SINCE.get(area)
    if not since:
        return False
    if _parts(since) > _parts(own_version):
        return False          # kommt erst noch — nichts anzeigen
    seen = (_read().get('bereiche') or {}).get(area)
    return _parts(since) > _parts(seen)


def mark_seen(area, own_version):
    """Bereich wurde geöffnet — die Marke ist damit erledigt."""
    data = _read()
    data.setdefault('bereiche', {})[area] = str(own_version or '')
    data['zuletzt'] = str(own_version or '')
    return _write(data)


def open_areas(own_version):
    """Alle Bereiche, an denen gerade eine Marke stünde."""
    return [b for b in NEW_SINCE if is_new(b, own_version)]
