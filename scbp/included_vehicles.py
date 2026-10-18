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
Beiboote laut Katalog — welches Schiff welches Fahrzeug mitbringt.

Kein Hangar-Export führt das verlässlich: Der Pledge nennt Schiff,
Versicherung und Zusätze, ein mitgeliefertes Beiboot (die Skylark der
Constellation Mk V Centaurus, die P-52 Merlin der Mk IV Andromeda) steht dort
nicht — CIG führt solche Fahrzeuge über das Leihsystem, solange das
Mutterschiff im Hangar ist. Die Zuordnung kommt deshalb aus der
mitgelieferten Datei `daten/beiboote.json`, und **jeder Eintrag trägt seinen
Beleg** bei RSI: die Adresse (der Support-Artikel `Included Vessels` oder
die Schiffsseite), das Abrufdatum und ein kurzes Stück des Textes, das das
Fahrzeug nennt — aus der Tabelle des Artikels die Zeile als
`Schiff | Fahrzeuge`.

    {"schiffe": [
      {"schiff": "Constellation Mk V Centaurus",
       "auch": ["Constellation Centaurus"], "hkurz": "RSI",
       "fahrzeuge": [{"name": "Skylark", "hersteller": "Roberts Space
                      Industries", "hkurz": "RSI"}],
       "beleg": {"adresse": "https://robertsspaceindustries.com/…",
                 "abgerufen": "2026-10-18", "auszug": "…"}}]}

⚠ **Ohne Beleg kein Fahrzeug.** Ein Schiff, bei dem RSI kein Beiboot nennt
(die Constellation Mk IV Taurus), steht mit leerer `fahrzeuge`-Liste und
`beleg: null` in der Datei — damit sichtbar bleibt, dass es geprüft und
nichts gefunden wurde. Ein Hangar oder Platz an Bord ist kein Beiboot.

⚠⚠ **Ein Beiboot laut Katalog ist kein Pledge-Inhalt.** Es landet mit der
Herkunft `fleet.CATALOG` im Hangar, die Karte sagt `s_hg_katalog`, und es geht
weder ans Basetool noch bekommt es einen Schmelz-Status. Kommt dasselbe
Fahrzeug später aus einem Export, dem Basetool oder von Hand, wird daraus ein
gewöhnlicher Eintrag unter dem neuen Namen (`fleet.add` über
`catalog_entry`).

Was `apply()` am Hangar tut (nach jedem Import; `prune()` auch beim
Speichern):

| Lage | Wirkung |
|---|---|
| Träger im Hangar, Beiboot fehlt | Beiboot laut Katalog dazu, `paket` = Träger |
| Beiboot steht schon drin (eigener Pledge, `includedShips`, von Hand) | nichts — kein Doppel, kein Feld überschrieben |
| Beiboot laut Katalog und derselbe Eintrag unter anderer Schreibweise | der Katalog-Eintrag fällt weg, Auslegung geht hinüber |
| Beiboot laut Katalog, Träger nicht mehr im Hangar | fällt weg |

⚠ Ein ausgetragenes Beiboot laut Katalog kommt mit dem nächsten Import
wieder, solange der Träger da ist — wie ein ausgetragenes Pledge-Schiff.
"""
import json
import re
import urllib.parse

from . import errors, paths

FILE = 'beiboote.json'
# Ein Beleg kommt von CIG selbst: Schiffsseite oder Support-Artikel.
RSI_HOST = 'robertsspaceindustries.com'

_cache = {}


def load():
    """Die Einträge der mitgelieferten Datei — `[]`, wenn sie fehlt."""
    if 'entries' in _cache:
        return _cache['entries']
    entries = []
    try:
        with open(paths.bundled_file(FILE), encoding='utf-8') as f:
            raw = json.load(f)
        if isinstance(raw, dict) and isinstance(raw.get('schiffe'), list):
            entries = [e for e in raw['schiffe'] if isinstance(e, dict)]
    except Exception as exc:
        errors.record('included_vehicles.load', exc)
    _cache['entries'] = entries
    return entries


def _slim(text):
    return re.sub(r'[^a-z0-9]', '', (text or '').lower())


def _names(record):
    """Der Name und seine anderen Schreibweisen, geschliffen."""
    result = {_slim(record.get('schiff') or record.get('name'))}
    result.update(_slim(a) for a in (record.get('auch') or ())
                  if isinstance(a, str))
    result.discard('')
    return result


def has_evidence(record):
    """Trägt der Eintrag Adresse bei RSI, Abrufdatum und Auszug?"""
    proof = record.get('beleg')
    if not isinstance(proof, dict):
        return False
    address = urllib.parse.urlsplit(str(proof.get('adresse') or ''))
    host = (address.hostname or '').lower()
    return (address.scheme == 'https'
            and (host == RSI_HOST or host.endswith('.' + RSI_HOST))
            and bool(str(proof.get('abgerufen') or '').strip())
            and bool(str(proof.get('auszug') or '').strip()))


def record_for(entry, entries=None):
    """Der Katalogeintrag zu einem Hangar-Schiff — oder `None`.

    Verglichen wird der Name samt anderen Schreibweisen (`auch`): Die
    Schiffsliste nennt `Constellation Mk IV Andromeda`, ältere Exporte
    `Constellation Andromeda`. Trägt der Hangar-Eintrag ein Herstellerkürzel,
    muss es passen.
    """
    wanted = _slim(entry.get('name'))
    if not wanted:
        return None
    maker = _slim(entry.get('hkurz'))
    for record in (load() if entries is None else entries):
        if wanted not in _names(record):
            continue
        if maker and _slim(record.get('hkurz')) \
                and maker != _slim(record.get('hkurz')):
            continue
        return record
    return None


def vehicles_for(entry, entries=None):
    """Die belegten Beiboote eines Hangar-Schiffs: `[Fahrzeug]`.

    Ein Eintrag ohne Beleg liefert nichts, auch wenn dort Fahrzeuge
    stünden."""
    record = record_for(entry, entries)
    if not record or not has_evidence(record):
        return []
    return [v for v in (record.get('fahrzeuge') or ())
            if isinstance(v, dict) and _slim(v.get('name'))]


def _present(ships, vehicle):
    """Steht das Fahrzeug schon im Hangar — unter welchem Namen auch immer?"""
    from . import fleet
    wanted = _names(vehicle)
    for s in ships:
        if _slim(s.get('name')) in wanted:
            return s
        if fleet._same_ship(s, vehicle.get('name'),
                            vehicle.get('hersteller') or '', '',
                            vehicle.get('hkurz') or ''):
            return s
    return None


def _vehicle_named(name, entries=None):
    """Das Fahrzeug der Datei, das `name` meint (auch über `auch`) — oder `None`."""
    wanted = _slim(name)
    if not wanted:
        return None
    for record in (load() if entries is None else entries):
        for vehicle in (record.get('fahrzeuge') or ()):
            if isinstance(vehicle, dict) and wanted in _names(vehicle):
                return vehicle
    return None


def catalog_entry(data, name, maker_code='', entries=None):
    """Der Hangar-Eintrag laut Katalog, den `name` meint — oder `None`.

    Für `fleet.add`: Nennt ein Export, das Basetool oder der Spieler das
    Fahrzeug unter anderer Schreibweise (`URSA Rover` statt `Ursa`) oder ohne
    Hersteller, ist es trotzdem dieser Eintrag und kein zweiter. Ein anderes
    Herstellerkürzel heißt ein anderes Schiff."""
    from . import fleet
    vehicle = _vehicle_named(name, entries)
    wanted = _names(vehicle) if vehicle else {_slim(name)}
    maker = _slim(maker_code)
    for s in (data.get('schiffe') or []):
        if s.get('herkunft') != fleet.CATALOG \
                or _slim(s.get('name')) not in wanted:
            continue
        if maker and _slim(s.get('hkurz')) and maker != _slim(s.get('hkurz')):
            continue
        return s
    return None


# Was der Spieler an einem Beiboot laut Katalog eingestellt hat und beim
# Zusammenlegen mit einem gleichen Eintrag nicht verloren gehen darf.
_PLAYER_KEYS = ('einsatzbereit', 'einheit', 'schmelz_wahl')


def _absorb(target, entry):
    """Die Einstellungen von `entry` an `target` geben, wo dort nichts steht."""
    if not target.get('belegung') and entry.get('belegung'):
        target['belegung'] = entry['belegung']
    for key in _PLAYER_KEYS:
        if target.get(key) is None and entry.get(key) is not None:
            target[key] = entry[key]


def wanted(data, entries=None):
    """`[(Fahrzeug, Name des Trägers)]` — was laut Katalog dazugehört.

    Träger sind alle Hangar-Schiffe außer den Beibooten laut Katalog selbst.
    Nennen zwei Träger dasselbe Fahrzeug, zählt der erste."""
    from . import fleet
    result, seen = [], set()
    for s in (data.get('schiffe') or []):
        if s.get('herkunft') == fleet.CATALOG:
            continue
        for vehicle in vehicles_for(s, entries):
            key = _slim(vehicle.get('name'))
            if key in seen:
                continue
            seen.add(key)
            result.append((vehicle, s.get('name') or ''))
    return result


def prune(data, entries=None):
    """Beiboote laut Katalog austragen, die nicht mehr dazugehören.

    Das sind zwei Fälle: Der Träger ist weg, oder dasselbe Fahrzeug steht
    inzwischen als gewöhnlicher Eintrag da (etwa als `URSA Rover` aus einem
    anderen Export). Im zweiten Fall gehen Auslegung und Einstellungen an
    diesen Eintrag. Gibt die Namen zurück."""
    from . import fleet
    keep = {_slim(v.get('name')) for v, _ in wanted(data, entries)}
    ships = data.get('schiffe') or []
    others = [s for s in ships if s.get('herkunft') != fleet.CATALOG]
    gone = []
    for s in ships:
        if s.get('herkunft') != fleet.CATALOG:
            continue
        # Über den Hauptnamen der Datei: Ein zurückgestufter Eintrag kann
        # `URSA Rover` heißen und ist trotzdem die `Ursa`.
        vehicle = _vehicle_named(s.get('name'), entries) or s
        if _slim(vehicle.get('name')) not in keep:
            gone.append(s)
            continue
        twin = _present(others, vehicle)
        if twin is not None:
            _absorb(twin, s)
            gone.append(s)
    if gone:
        data['schiffe'] = [s for s in ships if not any(s is g for g in gone)]
    return [s.get('name') or '' for s in gone]


# Pledge-Angaben, die ein zurückgestuftes Beiboot nicht mehr trägt — es
# behauptet nur noch, was der Katalog sagt.
_PLEDGE_KEYS = ('lti', 'versicherung', 'warbond', 'gekauft', 'preis',
                'schmelzbar', 'quelle', 'kurz', 'versicherung_basetool')


def demote(data, candidates, entries=None):
    """Ausgetragene Schiffe, die laut Katalog zu einem Träger in `data`
    gehören, als Beiboot laut Katalog behalten. Gibt diese zurück.

    Für `fleet.import_entries`: Fehlt ein beförderter Eintrag (`URSA Rover`
    aus dem CSV) im nächsten Export, bliebe sonst nur ein frischer
    Katalog-Eintrag ohne die Auslegung des Spielers."""
    from . import fleet
    kept = []
    ships = data.get('schiffe') or []
    for vehicle, carrier in wanted(data, entries):
        if _present(ships, vehicle) is not None:
            continue
        ship = _present([c for c in candidates
                         if not any(c is k for k in kept)], vehicle)
        if ship is None:
            continue
        ship['herkunft'] = fleet.CATALOG
        ship['paket'] = carrier
        for key in _PLEDGE_KEYS:
            ship.pop(key, None)
        kept.append(ship)
    return kept


def apply(data, entries=None):
    """Beiboote laut Katalog ergänzen und verwaiste austragen.

    Gibt `(dazu, weg)` zurück, je eine Liste von Namen. Vorhandene Einträge
    werden nie angefasst: Steht das Fahrzeug schon im Hangar, bleibt es, wie
    es ist. Ein Beiboot laut Katalog nennt immer einen Träger, der noch da
    ist."""
    from . import fleet
    removed = prune(data, entries)
    added = []
    ships = data.setdefault('schiffe', [])
    for vehicle, carrier in wanted(data, entries):
        found = _present(ships, vehicle)
        if found is not None:
            if found.get('herkunft') == fleet.CATALOG:
                found['paket'] = carrier
            continue
        ships.append({'name': vehicle['name'].strip(),
                      'hersteller': (vehicle.get('hersteller') or '').strip(),
                      'hkurz': (vehicle.get('hkurz') or '').strip(),
                      'herkunft': fleet.CATALOG, 'paket': carrier,
                      'belegung': {}})
        added.append(vehicle['name'].strip())
    return added, removed
