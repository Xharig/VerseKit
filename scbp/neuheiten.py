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
"""
import json

from . import pfade

DATEI = 'gesehen.json'

# Welcher Bereich kam mit welcher Version? Beim Bauen eines neuen Bereichs hier
# **eine Zeile ergänzen** — mehr ist nicht zu tun.
NEU_SEIT = {
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


def _teile(version):
    """'2.2.0' -> (2, 2, 0); alles Unlesbare wird zu (0, 0, 0)."""
    zahlen = []
    for stueck in str(version or '').split('-')[0].split('.'):
        try:
            zahlen.append(int(stueck))
        except ValueError:
            zahlen.append(0)
    while len(zahlen) < 3:
        zahlen.append(0)
    return tuple(zahlen[:3])


def _lesen():
    try:
        with open(pfade.app_datei(DATEI), encoding='utf-8') as f:
            daten = json.load(f)
        return daten if isinstance(daten, dict) else {}
    except Exception:
        return {}


def _schreiben(daten):
    try:
        with open(pfade.app_datei(DATEI), 'w', encoding='utf-8') as f:
            json.dump(daten, f, ensure_ascii=False, indent=1)
        return True
    except Exception as ausnahme:
        try:
            from . import fehler
            fehler.merken('neuheiten.schreiben', ausnahme)
        except Exception:
            pass
        return False


def erster_start(eigene_version):
    """Merkt sich beim allerersten Lauf die Version — ohne Marken zu setzen.

    Genau hier entscheidet sich Regel 2: Wer frisch installiert, hat nichts
    verpasst und bekommt deshalb auch nichts markiert.
    """
    daten = _lesen()
    if 'zuletzt' not in daten:
        daten['zuletzt'] = str(eigene_version or '')
        daten['bereiche'] = {k: str(eigene_version or '') for k in NEU_SEIT}
        _schreiben(daten)
        return True
    return False


def ist_neu(bereich, eigene_version):
    """Soll an diesem Bereich eine Marke stehen?"""
    seit = NEU_SEIT.get(bereich)
    if not seit:
        return False
    if _teile(seit) > _teile(eigene_version):
        return False          # kommt erst noch — nichts anzeigen
    gesehen = (_lesen().get('bereiche') or {}).get(bereich)
    return _teile(seit) > _teile(gesehen)


def gesehen(bereich, eigene_version):
    """Bereich wurde geöffnet — die Marke ist damit erledigt."""
    daten = _lesen()
    daten.setdefault('bereiche', {})[bereich] = str(eigene_version or '')
    daten['zuletzt'] = str(eigene_version or '')
    return _schreiben(daten)


def offene(eigene_version):
    """Alle Bereiche, an denen gerade eine Marke stünde."""
    return [b for b in NEU_SEIT if ist_neu(b, eigene_version)]
