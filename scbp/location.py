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
Standort: an welchem Landeort der Spieler gerade ist — aus der `Game.log`.

| Was | Log-Zeile |
|---|---|
| Ortswechsel | `<Update Inventory Location> Player [<Name>] is changing location. Landing [<alt>] -> [<neu>]. Location […] -> […]` |
| Name zur Nummer | `<RequestLocationInventory> Player[<Name>] requested inventory for Location[<Kennung>]`, danach `<Query Inventory> … Inventory[<Spieler>:Location:<Nummer>]` |
| Name zur Nummer | `ATC Location: <Kennung> [<Nummer>]` |

⚠⚠ **Im Ortswechsel steht der Ort nur als Nummer.** Die Nummer ist keine
Prüfsumme des Namens und steht nicht in der `Data.p4k`. Den Namen nennt das
Spiel nur, wenn man an dem Ort das Inventar öffnet. Die Zuordnung wird
deshalb aus allen Logs **gelernt** und in `orte-gelernt.json` festgehalten —
sie zieht mit dem Datenordner um und bleibt, wenn die Logs gelöscht werden.
Gemessen an 221 Logs: jede Nummer hat genau einen Namen und umgekehrt, und
94 % aller Aufenthalte an einem Landeort sind so benennbar. Was noch nicht
gelernt ist, zeigt die Kachel als unbekannten Ort, mit dem Hinweis, dort
einmal das Inventar zu öffnen.

`Landing [0]` heißt: an keinem Landeort — im Flug oder draußen unterwegs.

Der Anzeigename kommt aus dem Datensatz `StarMapObject.<Kennung>` (Feld
`name`) und der englischen `global.ini`, einmal je Spielstand gelesen und als
`orte-namen.json` abgelegt.
"""
import json
import os
import re
import threading

from . import errors, paths

LEARNED_FILE = 'orte-gelernt.json'
NAMES_FILE = 'orte-namen.json'
# Ändert sich das Auslesen der Namen, wird mit neuer Nummer neu gelesen.
NAMES_FORMAT = 1
MAP_TYPE = 'StarMapObject'
# Weniger Namen heißt: Datenbank nicht verstanden — dann nichts ablegen.
NAMES_MINIMUM = 50
NOWHERE = '0'

_CHANGE = re.compile(r'<Update Inventory Location> Player \[[^\]]*\] is '
                     r'changing location\. Landing \[(\d+)\] -> \[(\d+)\]')
_REQUEST = re.compile(r'<RequestLocationInventory> Player\[[^\]]*\] requested '
                      r'inventory for Location\[([^\]]+)\]')
_QUERY = re.compile(r'<Query Inventory> Request\[\d+\] '
                    r'Inventory\[\d+:Location:(\d+)\]')
_ATC = re.compile(r'ATC Location: ([A-Za-z][\w-]*) \[(\d+)\]')
_TIMESTAMP = re.compile(r'<(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d)')

_LOCK = threading.Lock()
_NAMES = {'stand': None, 'orte': None}


# ------------------------------------------------------------------ Lernen
def pairs_in(lines):
    """{Nummer: Kennung} aus Protokollzeilen."""
    found = {}
    pending = None
    for line in lines:
        if '<RequestLocationInventory>' in line:
            match = _REQUEST.search(line)
            pending = match.group(1) if match else None
        elif pending and '<Query Inventory>' in line:
            match = _QUERY.search(line)
            if match:
                found[match.group(1)] = pending
                pending = None
        elif 'ATC Location:' in line:
            match = _ATC.search(line)
            if match:
                found[match.group(2)] = match.group(1)
    return found


def pairs_from(log_path):
    """{Nummer: Kennung} aus einer Protokolldatei — leer, wenn unlesbar."""
    try:
        with open(log_path, encoding='utf-8', errors='replace') as f:
            return pairs_in(f)
    except OSError:
        return {}


def remember(pairs):
    """Paare in die gelernte Zuordnung übernehmen, ohne den Lesestand zu
    ändern. Gibt die Zahl der neuen Orte zurück."""
    if not pairs:
        return 0
    with _LOCK:
        data = load_learned()
        places = data['orte']
        fresh = {n: k for n, k in pairs.items() if places.get(n) != k}
        if not fresh:
            return 0
        added = sum(1 for n in fresh if n not in places)
        places.update(fresh)
        _save_learned(data)
        return added


def _learned_path():
    return paths.app_file(LEARNED_FILE)


def load_learned():
    """`{'orte': {Nummer: Kennung}, 'gelesen': {Dateiname: Größe}}`."""
    try:
        with open(_learned_path(), encoding='utf-8') as f:
            data = json.load(f)
        if isinstance(data, dict) and isinstance(data.get('orte'), dict):
            data.setdefault('gelesen', {})
            return data
    except (OSError, ValueError):
        pass
    except Exception as exception:
        errors.record('location.load_learned', exception)
    return {'orte': {}, 'gelesen': {}}


def _save_learned(data):
    try:
        target = _learned_path()
        folder = os.path.dirname(target)
        if folder and not os.path.isdir(folder):
            os.makedirs(folder)
        with open(target + '.neu', 'w', encoding='utf-8') as f:
            json.dump(data, f, ensure_ascii=False)
        os.replace(target + '.neu', target)
        return True
    except Exception as exception:
        errors.record('location.save_learned', exception)
        return False


def learn(files):
    """Die Zuordnung aus Protokollen fortschreiben. Gibt die Zahl der neu
    gelernten Orte zurück.

    ⚠ Eigener Lesestand (Name + Größe) wie in `play_stats`: Jede Datei wird
    nur gelesen, wenn sie neu oder gewachsen ist."""
    with _LOCK:
        data = load_learned()
        places, read = data['orte'], data['gelesen']
        added = 0
        changed = False
        for log_path in files or []:
            try:
                size = os.path.getsize(log_path)
            except OSError:
                continue
            name = os.path.basename(log_path)
            if read.get(name) == size:
                continue
            read[name] = size
            changed = True
            for number, key in pairs_from(log_path).items():
                if places.get(number) != key:
                    if number not in places:
                        added += 1
                    places[number] = key
        if changed:
            _save_learned(data)
        return added


# ------------------------------------------------------------------ Lesen
def running_log():
    """Die laufende `Game.log` der Installation — oder None."""
    game = paths.game_folder()
    if not game:
        return None
    path = os.path.join(game, 'Game.log')
    return path if os.path.isfile(path) else None


def last_change(log_path, pairs=None):
    """Der letzte Ortswechsel: `(alt, neu, Sekunden)` — oder None.

    Von hinten gelesen, in wachsenden Stücken: Die `Game.log` wird lang, der
    letzte Wechsel steht fast immer im letzten Megabyte. Ist `pairs` ein
    Wörterbuch, kommen die Namenspaare aus dem gelesenen Stück hinein — so
    lernt der aktuelle Ort seinen Namen, ohne die ganze Datei zu lesen."""
    try:
        size = os.path.getsize(log_path)
        chunk = 1 << 20
        with open(log_path, 'rb') as f:
            while True:
                start = max(0, size - chunk)
                f.seek(start)
                text = f.read(size - start).decode('utf-8', 'replace')
                found = None
                for found in _CHANGE.finditer(text):
                    pass
                if found:
                    if pairs is not None:
                        pairs.update(pairs_in(text.splitlines()))
                    line_start = text.rfind('\n', 0, found.start()) + 1
                    stamp = _TIMESTAMP.search(text, line_start, found.start())
                    from .play_stats import _seconds
                    return (found.group(1), found.group(2),
                            _seconds(stamp.group(1)) if stamp else None)
                if start == 0:
                    return None
                chunk *= 4
    except OSError:
        return None


def where(log_path=None, learn_missing=True):
    """Der aktuelle Ort als Wörterbuch — oder None ohne Protokoll.

    `nummer`: die Nummer aus dem Log (None, wenn noch kein Wechsel drinsteht),
    `kennung`: der gelernte Name dazu oder None, `unterwegs`: an keinem
    Landeort, `vorher`: die Kennung des zuletzt verlassenen Orts, `seit`:
    Sekunden des Wechsels. Fehlt ein Name, wird er im gelesenen Ende der
    Datei gesucht und gemerkt."""
    log_path = log_path or running_log()
    if not log_path:
        return None
    pairs = {}
    found = last_change(log_path, pairs)
    if not found:
        return {'nummer': None, 'kennung': None, 'unterwegs': False,
                'vorher': None, 'seit': None}
    before, now, since = found
    wanted = [n for n in (now, before) if n != NOWHERE]
    places = load_learned()['orte']
    if learn_missing and any(n not in places for n in wanted):
        remember(pairs)
        places = load_learned()['orte']
    return {'nummer': now,
            'kennung': places.get(now) if now != NOWHERE else None,
            'unterwegs': now == NOWHERE,
            'vorher': places.get(before) if before != NOWHERE else None,
            'seit': since}


def overlay_text(place):
    """Die eine Zeile fürs Overlay — leer, wenn es nichts zu zeigen gibt."""
    from .language import t
    if not place or not place.get('nummer'):
        return ''
    if place['unterwegs']:
        if place.get('vorher'):
            return t('ov_unterwegs') % display_name(place['vorher'])
        return t('s_uv_unterwegs')
    if not place.get('kennung'):
        return t('ov_standort') % t('s_uv_ort_unbekannt')
    return t('ov_standort') % display_name(place['kennung'])


# ------------------------------------------------------------------ Namen
def _names_path():
    return paths.app_file(NAMES_FILE)


def place_names():
    """{Kennung in Kleinschrift: Anzeigename} — leer, solange nicht gelesen."""
    path = _names_path()
    try:
        stamp = os.path.getmtime(path)
    except OSError:
        return {}
    if _NAMES['stand'] != stamp:
        try:
            with open(path, encoding='utf-8') as f:
                data = json.load(f)
            _NAMES['orte'] = data.get('orte') if isinstance(data, dict) else {}
        except (OSError, ValueError):
            _NAMES['orte'] = {}
        _NAMES['stand'] = stamp
    return _NAMES['orte'] or {}


def display_name(key):
    """`Stanton2_Orison` -> `Orison`; ohne Namenstabelle die Kennung,
    nur lesbarer."""
    if not key:
        return ''
    return place_names().get(key.lower()) or key.replace('_', ' ')


def names_from(db, texts):
    """{Kennung in Kleinschrift: Anzeigename} aus einer gelesenen Datenbank.

    `db` ist ein `datacore.DataCore`, `texts` das Ergebnis von
    `quality_goods.parse_ini`. Platzhalter (`@LOC_EMPTY` …) fallen weg."""
    found = {}
    for name, _file, si, _guid, ii in db.records():
        if db.type_name(si) != MAP_TYPE:
            continue
        data = db.read(si, ii, 0)
        key = str(data.get('name') or '').lstrip('@').lower()
        value = texts.get(key)
        if value and not value.startswith('@'):
            found[name.split('.', 1)[-1].lower()] = value
    return found


def refresh_names(game_folder=None):
    """Nach einem Spiel-Patch die Namen neu lesen. True, wenn neu abgelegt.
    Wirft nie."""
    try:
        from . import datacore, gametext
        from .quality_goods import parse_ini
        stamp = gametext.archive_stamp(game_folder)
        try:
            with open(_names_path(), encoding='utf-8') as f:
                current = json.load(f)
        except (OSError, ValueError):
            current = {}
        if not stamp or (current.get('stand') == stamp
                         and current.get('format') == NAMES_FORMAT):
            return False
        with datacore.READ_LOCK:
            raw = gametext.read_archive_file(datacore.ARCHIVE_PATH,
                                             game_folder)
            if not raw:
                return False
            ini, _message = gametext.read_from_archive('english', game_folder)
            if not ini:
                return False
            found = names_from(datacore.DataCore(raw), parse_ini(ini))
        if len(found) < NAMES_MINIMUM:
            errors.trail('Ortsnamen: nur %d erkannt, nicht abgelegt'
                         % len(found))
            return False
        path = _names_path()
        with open(path + '.tmp', 'w', encoding='utf-8') as f:
            json.dump({'format': NAMES_FORMAT, 'stand': stamp, 'orte': found},
                      f, ensure_ascii=False)
        os.replace(path + '.tmp', path)
        return True
    except Exception as exception:
        errors.record('location.refresh_names', exception)
        return False


def start():
    """Im Hintergrund: Namen nach einem Patch lesen, Orte aus allen Logs
    lernen."""
    def work():
        refresh_names()
        try:
            from .play_stats import log_files
            learn(log_files())
        except Exception as exception:
            errors.record('location.start', exception)
    threading.Thread(target=work, daemon=True, name='standort').start()
