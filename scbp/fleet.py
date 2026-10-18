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
Meine Schiffe — welche ich habe und woher sie kommen.

Ohne diese Liste ist nicht zu beantworten, ob ein Bauplan ins eigene Schiff passt;
das Werkzeug wüsste nur, in *irgendein* Schiff passt es. `ships.py` führt
alle Schiffe des Spiels für den Routenplaner — hier geht es um die eigenen.

## ⚠ Die `Game.log` gibt das nicht her — gemessen

Naheliegend wäre, den Hangar aus dem Spiel mitzulesen, wie bei den Bauplänen.
Geht nicht. Über 202 Logsicherungen stehen zwar zehntausende Schiffsnamen, aber
es sind die Schiffe **in der Umgebung** — mit Abstand am häufigsten `VNCL_War`
(12.534 Treffer), also Vanduul-Gegner.

Vom eigenen Hangar stehen nur **Zahlen** im Log::

    <VehicleListQuery> … Retrieved 65 entitlements out of 78 vehicules.
    <BuildInventoryStowedAggregateRoots> Found [13] vehicle(s) at location

Keine Namen. Diese Zahlen taugen aber als **Gegenprobe**: Wer 42 Schiffe
eingetragen hat und dessen Spiel von 78 spricht, dem fehlt etwas.

## Zwei Wege hinein — und beide werden gebraucht

| Weg | bringt | bringt **nicht** |
|---|---|---|
| Import aus der **Star Citizen: Hangar Extension** (AlyxOne), JSON | alle Echtgeld-Schiffe samt Hersteller, Code, Paketzugehörigkeit und **Versicherung** (`"LTI"` oder `"120MI"`) | im Spiel gekaufte Schiffe; Preis und Kaufdatum |
| dieselbe Erweiterung, **CSV** (Komplett-Export) | Schiffe samt **LTI oder Versicherungsdauer in Monaten** (`versicherung`), Paketname, Datum, Preis | im Spiel gekaufte Schiffe; Schiffskürzel |
| Import aus **Star Citizen Hangar XPLORer** (dolkensp) | alle Echtgeld-Pledges samt LTI und Paketname | im Spiel gekaufte Schiffe |
| **Schiffsliste** im Format `hangar-schiffe` v1, JSON | je Schiff Name aus der RSI-Schiffsliste, Kürzel, Hersteller, Versicherung, Paket, Beiboote, Warbond, Kaufdatum, Preis | im Spiel gekaufte Schiffe |
| **Von Hand** eintragen | alles Übrige | — |

Beide Erweiterungen setzen auf der Pledge-Seite Export-Knöpfe. Gelesen wird
JSON und CSV beider — erkannt am Inhalt, nicht am Dateinamen (`_from_json`,
`_from_csv`). Empfohlen wird die Hangar Extension: Sie wird gepflegt, der
XPLORer nicht mehr. Wer JSON **und** CSV der Extension
einliest, bekommt Kürzel und Paketbeziehung aus dem einen und die
Versicherung aus dem anderen — `_same_ship` führt sie zusammen.

⚠ **Der Export kennt nur Gekauftes.** Wer sich im Spiel eine Cutlass erflogen
hat, findet sie dort nie — deshalb ist der Handeintrag kein Notbehelf, sondern
gleichberechtigt. Jedes Schiff trägt seine `herkunft`.

## ⚠ Die Kaufsumme bleibt im Haus

`pledge_cost` und `pledge_id` sind private Angaben. Sie werden abgelegt, weil
sie dem Spieler gehören und er sie sehen will — aber sie dürfen **nie** in den
Fehlerbericht geraten. Dieselbe Linie wie `paths.redact()` bei den Pfaden.

## Aufbau der Datei (`hangar.json` im eigenen Ordner)

    {"format": 1,
     "schiffe": [
       {"name": "Cutlass Black", "hersteller": "Drake Interplanetary",
        "herkunft": "pledge", "lti": true, "paket": "Standalone Ship",
        "gekauft": "May 18, 2026", "preis": "$120.00 USD",
        "belegung": {}}]}

⚠ **`belegung` bleibt vorerst leer.** Der Platz für eine gespeicherte Auslegung
(welches Teil in welchem Steckplatz) ist mit Absicht schon da: Kommt sie dazu,
soll das keinen Formatwechsel kosten und keine bestehende Datei entwerten.

⚠ **Bezeichner sind englisch, gespeicherte Zeichenketten nicht.** Deutsch
bleiben: der Ablage-Name `hangar.json` und alle Schlüssel darin
(`format`, `schiffe`, `wunsch`, `merkzettel`, `name`, `hersteller`,
`herkunft`, `belegung`, `kurz`, `hkurz`, `lti`, `warbond`, `paket`,
`gekauft`, `preis`, `ref`, `anzahl`, `weg`, `einsatzbereit`, `einheit` — die
Kennung der Einheit im Basetool, `''` = keine —, `schmelzbar` aus dem Import,
`schmelz_wahl` mit den Werten `ja`/`nein`, `aus_basetool` und
`versicherung_basetool`) sowie die Herkunftswerte
`'pledge'`, `'ingame'` und `'katalog'` — sonst verliert jeder beim Update
seinen Hangar.
Ebenso der Seitenname `hangar` in Reiterleiste, Symbolsatz und „Neu"-Marken.
"""
import collections
import copy
import csv
import io
import json
import os
import re
import threading

from . import erkul, errors, paths

FILE = 'hangar.json'
FORMAT = 1

# Woher ein Schiff stammt. Mehr Fälle gibt es nicht — geliehene Schiffe stehen
# nicht im Hangar, und geschenkte sind aus Sicht des Spielers Pledges.
PLEDGE = 'pledge'
INGAME = 'ingame'
# Kam nur über den Abgleich mit dem KRT Profit Basetool herein.
# Ein Pledge-Import räumt es nicht weg (nur `pledge` fällt heraus).
BASETOOL = 'basetool'
# Aus einer Schiffsliste eines anderen Werkzeugs (Fleetview, Fleetyards,
# StarJump FleetViewer) — `quelle` nennt es.
LIST = 'liste'
# Ein Beiboot laut Katalog (`included_vehicles`): mitgeliefert mit dem Schiff
# unter `paket`, aber in keinem Export genannt. Kein Pledge-Inhalt.
CATALOG = 'katalog'
# Am Eintrag: Die Versicherung kam aus dem Basetool, kein Import ändert sie
# (`_take_over`, `basetool_sync._apply_ships`).
BASETOOL_COVER = 'versicherung_basetool'
# Am Eintrag: Er kam aus dem Basetool, ein Export nennt ihn seither
# (`_take_over`).
FROM_BASETOOL = 'aus_basetool'


_SAVE_LOCK = threading.Lock()


def path():
    return paths.app_file(FILE)


def empty():
    return {'format': FORMAT, 'schiffe': []}


# Je geladenem Hangar-Objekt der Stand, den es zuletzt von der Platte kannte:
# `id(data)` -> (data, Kopie). Das Objekt selbst steht mit drin, damit seine
# `id` nicht an ein neues Objekt weitergegeben wird, solange der Eintrag lebt.
_BASES = collections.OrderedDict()
_BASES_MAX = 64
_BASES_LOCK = threading.Lock()


def _remember(data):
    """`data` entspricht jetzt dem, was auf der Platte steht."""
    with _BASES_LOCK:
        _BASES[id(data)] = (data, copy.deepcopy(data))
        _BASES.move_to_end(id(data))
        while len(_BASES) > _BASES_MAX:
            _BASES.popitem(last=False)


def _base_of(data):
    with _BASES_LOCK:
        hit = _BASES.get(id(data))
    return hit[1] if hit is not None and hit[0] is data else None


def _read():
    """Die Datei, wie sie liegt — oder `None`."""
    data = paths.load_json(path(), valid=lambda d: isinstance(d, dict),
                           report_as='fleet.load')
    if data and data.get('format') == FORMAT \
            and isinstance(data.get('schiffe'), list):
        return data
    return None


def changed_elsewhere(data):
    """Hat eine andere Stelle die Datei geändert, seit `data` geladen oder
    gespeichert wurde? Für Seiten, die ihren Stand beim Zeigen auffrischen."""
    base = _base_of(data)
    if base is None:
        return True
    try:
        disk = _read()
    except Exception:
        return False
    return disk is not None and disk != base


def load():
    """Der eigene Hangar — oder eine leere Liste. Eine kaputte Datei wird
    protokolliert und beiseitegelegt (`paths.load_json`)."""
    try:
        data = _read()
        if data is not None:
            _remember(data)
            if merge_duplicates(data):
                save(data)
            return data
    except Exception as exc:
        errors.record('fleet.load', exc)
    data = empty()
    _remember(data)
    return data


def _entry_keys(entries):
    """Liste -> {(Hersteller, Name, laufende Nummer): Eintrag}."""
    seen, out = {}, {}
    for e in entries or ():
        if not isinstance(e, dict):
            continue
        base = (_slim(e.get('hersteller')), _slim(e.get('name')))
        seen[base] = seen.get(base, 0) + 1
        out[base + (seen[base],)] = e
    return out


def _merge_dict(base, mine, target):
    """Was sich von `base` zu `mine` geändert hat, in `target` übernehmen."""
    for key in set(base) | set(mine):
        if mine.get(key) == base.get(key):
            continue
        if key in mine:
            target[key] = copy.deepcopy(mine[key])
        else:
            target.pop(key, None)


def _merge(base, mine, disk):
    """Die eigenen Änderungen (`base` -> `mine`) auf den Stand der Platte.

    ⚠⚠ **Mehrere Seiten halten je ein eigenes Hangar-Objekt.** Schreibt eine
    davon ihren ganzen, älteren Stand zurück, sind die Haken und Wahlen der
    anderen weg. Deshalb geht nur hinaus, was sich an diesem Objekt geändert
    hat — je Schiff Feld für Feld, in der Auslegung Steckplatz für Steckplatz.
    """
    result = copy.deepcopy(disk)
    for list_key in set(base) | set(mine) | set(disk):
        if list_key not in ('schiffe', 'wunsch'):
            continue
        b = _entry_keys(base.get(list_key))
        m = _entry_keys(mine.get(list_key))
        target_list = result.setdefault(list_key, [])
        d = _entry_keys(target_list)
        for key, entry in m.items():
            if key not in b:
                if key not in d:
                    target_list.append(copy.deepcopy(entry))
                continue
            target = d.get(key)
            if target is None:
                continue
            old = b[key]
            for field in set(old) | set(entry):
                if field == 'belegung' and isinstance(entry.get(field), dict):
                    _merge_dict(old.get(field) or {}, entry[field],
                                target.setdefault(field, {}))
                elif entry.get(field) != old.get(field):
                    if field in entry:
                        target[field] = copy.deepcopy(entry[field])
                    else:
                        target.pop(field, None)
        gone = [d[k] for k in b if k not in m and k in d]
        if gone:
            result[list_key] = [e for e in target_list
                                if not any(e is g for g in gone)]
    _merge_dict({k: v for k, v in base.items() if k not in ('schiffe', 'wunsch')},
                {k: v for k, v in mine.items() if k not in ('schiffe', 'wunsch')},
                result)
    return result


def save(data):
    """Atomar ablegen. Gibt zurück, ob es geklappt hat.

    ⚠ Der Rückgabewert wird ausgewertet — ein stilles `False` wäre genau der
    Fehler, der bei `pfade.einstellungen_schreiben` monatelang dafür sorgte,
    dass eine nicht gespeicherte Einstellung nach dem Neustart wieder alt war.
    """
    target = path()
    try:
        with _SAVE_LOCK:
            out = data
            base = _base_of(data)
            if base is not None:
                disk = _read()
                if disk is not None and disk != base:
                    out = _merge(base, data, disk)
            # Nach dem Zusammenführen: Ein älteres Hangar-Objekt kennt die
            # Beiboote laut Katalog nicht, deren Träger es austrägt.
            try:
                from . import included_vehicles
                included_vehicles.prune(out)
                # Auch das eigene Objekt, sonst merkt `_remember` sich einen
                # Stand, der nicht auf der Platte steht.
                if out is not data:
                    included_vehicles.prune(data)
            except Exception as exc:
                errors.record('fleet.save.beiboote', exc)
            os.makedirs(os.path.dirname(target), exist_ok=True)
            with open(target + '.tmp', 'w', encoding='utf-8') as f:
                json.dump(out, f, ensure_ascii=False, indent=1)
            os.replace(target + '.tmp', target)
            _remember(data)
    except Exception as exc:
        errors.record('fleet.save', exc)
        return False
    try:
        from . import basetool_sync
        basetool_sync.store_saved()
    except Exception as exc:
        errors.record('fleet.abgleich_melden', exc)
    return True


def _slim(text):
    return re.sub(r'[^a-z0-9]', '', (text or '').lower())


def count(data=None):
    return len((data or load()).get('schiffe') or [])


def names(data=None):
    """Alle Schiffsnamen im Hangar, alphabetisch."""
    items = (data or load()).get('schiffe') or []
    return sorted((s.get('name') or '' for s in items if s.get('name')),
                  key=str.lower)


def id_sets(data=None):
    """Je Schiff `(name, hersteller, kurz)` — was `erkul` zum Zuordnen braucht.

    ⚠ Der Kurzname aus dem Export trägt die meisten Treffer; ohne ihn fällt die
    Zuordnung von 34 auf 1 zurück. Deshalb wird er durchgereicht und nicht nur
    der Klartextname.
    """
    return [(s.get('name') or '', s.get('hersteller') or '',
             s.get('kurz') or '', s.get('hkurz') or '')
            for s in ((data or load()).get('schiffe') or []) if s.get('name')]


# ------------------------------------------------------------ Wunschliste

def wishlist(data=None):
    """Die Schiffe, die man sich vorgenommen hat — alphabetisch.

    ⚠ **Getrennt vom Hangar, in derselben Datei.** Ein Wunschschiff ist kein
    Besitz: Was hier steht, darf nirgends in die Auskunft einfließen, ob ein
    Teil ins eigene Schiff passt, sonst beantwortet das Werkzeug eine Frage über ein Schiff, das
    der Spieler gar nicht hat. Ein fehlendes Feld gilt als leere Liste — alte
    Dateien bleiben damit gültig, es braucht keinen Formatwechsel.
    """
    items = (data or load()).get('wunsch') or []
    return sorted((w for w in items if isinstance(w, dict) and w.get('name')),
                  key=lambda w: (w.get('name') or '').lower())


def wishlist_contains(data, name):
    wanted = _slim(name)
    return any(_slim(w.get('name')) == wanted
               for w in (data.get('wunsch') or []))


def wishlist_add(data, name, manufacturer=''):
    """Ein Schiff auf die Wunschliste setzen. Gibt zurück, ob es neu war.

    ⚠ Was schon im Hangar steht, kommt **nicht** auf die Wunschliste — man
    wünscht sich nichts, das man hat. Die Anzeige sagt das auch, statt den
    Eintrag stillschweigend zu schlucken.
    """
    if not (name or '').strip():
        return False
    if wishlist_contains(data, name):
        return False
    # `belegung` liegt leer bereit, genau wie beim Hangar-Schiff: Auch ein
    # Wunschschiff lässt sich ausstatten, und so kostet das später keinen
    # Formatwechsel.
    data.setdefault('wunsch', []).append(
        {'name': name.strip(), 'hersteller': (manufacturer or '').strip(),
         'belegung': {}})
    return True


def wishlist_remove(data, name):
    """Einen Eintrag der Wunschliste streichen. Gibt zurück, ob einer wegfiel."""
    wanted = _slim(name)
    before = len(data.get('wunsch') or [])
    data['wunsch'] = [w for w in (data.get('wunsch') or [])
                      if _slim(w.get('name')) != wanted]
    return len(data['wunsch']) != before


def notepad(data=None):
    """Einzelne Gegenstände, die man bauen oder kaufen will — alphabetisch.

    ⭐⭐ **Der Weg zum Farmen ohne Umweg über ein Schiff.** Über die
    Wunschliste führt eine Materialliste nur über ein Schiff und belegte
    Steckplätze. Für einen Helm, eine Waffe oder ein Rüstungsteil gäbe es
    diesen Weg **gar nicht** — obwohl sie genauso Baupläne mit Rohstoffbedarf
    sind.

    ⚠ Ein fehlendes Feld gilt als leere Liste — alte Dateien bleiben gültig,
    es braucht keinen Formatwechsel. Dieselbe Regel wie bei `wishlist()`.
    """
    items = (data or load()).get('merkzettel') or []
    result = []
    for m in items:
        if not isinstance(m, dict) or not m.get('name'):
            continue
        # ⚠⚠⚠ **Eine Kennung mit Leerzeichen ist keine Kennung, sondern ein
        # Name.** Ältere Merkzettel können so etwas enthalten; daraus wird
        # eine kaputte Preisabfrage (`items_prices?uuid=CF-447 Rhino Repeater`),
        # und die Einkaufsliste bleibt leer und lädt endlos.
        #
        # Hier wird es beim Lesen stillschweigend verworfen — so heilen sich
        # vorhandene Merkzettel von selbst, ohne dass jemand etwas neu eintragen
        # muss. Der Posten bleibt vollstaendig: Das Rezept findet `bauweg()`
        # ueber den Namen.
        ref = (m.get('ref') or '').strip()
        if ref and (any(z.isspace() for z in ref) or '"' in ref):
            m = dict(m)
            m['ref'] = ''
        result.append(m)
    return sorted(result, key=lambda m: (m.get('name') or '').lower())


def notepad_contains(data, name):
    wanted = _slim(name)
    return any(_slim(m.get('name')) == wanted
               for m in (data.get('merkzettel') or []))


def notepad_add(data, name, ref='', count=1):
    """Einen Gegenstand auf den Merkzettel setzen. Gibt zurück, ob er neu war.

    ⚠ `ref` ist die Kennung aus den Rezeptdaten. Ohne sie ließe sich später
    weder Preis noch Rezept nachschlagen — der Eintrag wäre ein Name ohne
    Anschluss. Sie darf leer bleiben (dann sucht die Rechnung über den Namen),
    aber sie wird mitgegeben, wo sie vorliegt.

    ⚠ Steht der Gegenstand schon drauf, wird **die Anzahl erhöht** statt eine
    zweite Zeile anzulegen. Zwei Zeilen mit demselben Helm wären in der
    Materialliste doppelt gezählt und in der Anzeige verwirrend.
    """
    name = (name or '').strip()
    if not name:
        return False
    try:
        count = max(1, int(count))
    except (TypeError, ValueError):
        count = 1
    wanted = _slim(name)
    for entry in (data.get('merkzettel') or []):
        if _slim(entry.get('name')) == wanted:
            entry['anzahl'] = int(entry.get('anzahl') or 1) + count
            return False
    data.setdefault('merkzettel', []).append(
        {'name': name, 'ref': (ref or '').strip(), 'anzahl': count,
         'weg': 'bauen'})
    return True


def notepad_remove(data, name):
    """Einen Gegenstand vom Merkzettel streichen. Gibt zurück, ob einer wegfiel."""
    wanted = _slim(name)
    before = len(data.get('merkzettel') or [])
    data['merkzettel'] = [m for m in (data.get('merkzettel') or [])
                          if _slim(m.get('name')) != wanted]
    return len(data['merkzettel']) != before


def notepad_crafted(data, name, count=1):
    """`count` Stück gebaut: Die Stückzahl sinkt, bei null fällt er weg.

    Gibt zurück, ob der Gegenstand auf dem Merkzettel stand."""
    try:
        count = max(1, int(count))
    except (TypeError, ValueError):
        count = 1
    wanted = _slim(name)
    for entry in (data.get('merkzettel') or []):
        if _slim(entry.get('name')) == wanted:
            return notepad_set_count(
                data, entry.get('name'),
                int(entry.get('anzahl') or 1) - count) or True
    return False


def notepad_set_count(data, name, count):
    """Wie oft der Gegenstand gebaut werden soll. `0` streicht ihn."""
    try:
        count = int(count)
    except (TypeError, ValueError):
        return False
    if count <= 0:
        return notepad_remove(data, name)
    wanted = _slim(name)
    for entry in (data.get('merkzettel') or []):
        if _slim(entry.get('name')) == wanted:
            entry['anzahl'] = count
            return True
    return False


def _same_ship(entry, name, manufacturer='', kurz='', hkurz=''):
    """Meint dieser Hangar-Eintrag dasselbe Schiff?

    Vier Wege, jeder für sich reicht:

    1. **Schiffskürzel** (`MISC_Endeavor`) — der sicherste. Beide Exporte
       (Hangar XPLORer und Hangar Extension) tragen dasselbe Kürzel.
    2. **Herstellerkürzel + Name** (`MISC` + `Endeavor`).
    3. **Herstellername + Name** in schlanker Schreibweise — der einzige Weg
       für Einträge von Hand, die kein Kürzel haben.
    4. **Name allein, nur bei einem Schiff aus dem Basetool ohne Hersteller**
       (`_from_basetool`) — das Basetool nennt nur `shipType.name`.

    ⚠ Sonst reicht der Name allein nicht: die Cutlass Black von Drake und
    eine gleichnamige Variante eines anderen Herstellers wären dasselbe.

    ⚠ Warum drei Wege: Der XPLORer schreibt `Musashi Industrial & Starflight
    Concern`, die Hangar Extension `MISC` — ein Import aus der Erweiterung in
    einen XPLORer-Hangar ergäbe sonst Doppel bei gleichem Kürzel.
    """
    wanted = _slim(name)
    e_kurz = _slim(entry.get('kurz'))
    if (e_kurz and _slim(kurz) and e_kurz == _slim(kurz)
            and _names_compatible(_slim(entry.get('name')), wanted)):
        return True
    e_name = _slim(entry.get('name'))
    if not wanted:
        return False
    e_hkurz = _slim(entry.get('hkurz'))
    if e_hkurz and _slim(hkurz) and (
            e_hkurz == _slim(hkurz)
            or frozenset((e_hkurz, _slim(hkurz))) in _MAKER_TWINS):
        # Mit Herstellerkürzel darf der Name um ein Klassenwort abweichen —
        # das CSV der Extension hat kein Schiffskürzel, und `Idris-P` muss
        # trotzdem die `Idris-P Frigate` des XPLORer treffen.
        return _names_compatible(e_name, wanted)
    e_maker = _slim(entry.get('hersteller'))
    if not e_maker and not (e_hkurz and _slim(hkurz)) \
            and _from_basetool(entry):
        # Weg 4: Der Name entscheidet, bis auf ein Klassenwort.
        return _names_compatible(e_name, wanted)
    if e_name != wanted:
        return False
    return e_maker == _slim(manufacturer)


def _from_basetool(entry):
    """Kam dieser Eintrag aus dem Basetool — auch wenn ihn seither ein Export
    nennt (`_take_over`)?"""
    return entry.get('herkunft') == BASETOOL or bool(entry.get(FROM_BASETOOL))


# Klassenwörter, die der Hangar XPLORer an manche Namen hängt („Idris-P
# Frigate"), die Hangar Extension aber nicht („Idris-P"). Nur um so ein Wort
# dürfen sich zwei Namen bei gleichem Kürzel unterscheiden.
_CLASS_WORDS = ('frigate', 'destroyer', 'corvette', 'carrier', 'cruiser')

# Herstellerkürzel, die für dasselbe Schiff nebeneinander vorkommen: Razor,
# Fury, Guardian und Pulse liefen bei CIG von MISC zu Mirai — der XPLORer
# schreibt noch `MISC_Razor_EX`, die Hangar Extension `MRAI_Razor_EX`. Bei
# gleichem Namen ist das ein Schiff, nicht zwei.
_MAKER_TWINS = {frozenset(('misc', 'mrai'))}


def _names_compatible(a, b):
    """Meinen zwei geschliffene Namen bei gleichem Kürzel dasselbe Schiff?

    ⚠ Gleiches Kürzel heißt **nicht** gleiches Schiff: Der XPLORer gibt der
    ATLS GEO dasselbe Kürzel wie der ATLS (`ARGO_ATLS`), die Extension
    kennt `ARGO_ATLS_GEO`. Wer nur das Kürzel vergleicht, macht aus zwei
    Schiffen eines. Deshalb
    müssen die Namen gleich sein oder sich um ein Klassenwort unterscheiden.
    """
    if not a or not b:
        return False
    if a == b:
        return True
    long, short = (a, b) if len(a) > len(b) else (b, a)
    return long.startswith(short) and long[len(short):] in _CLASS_WORDS


def find(data, name, manufacturer='', kurz='', hkurz=''):
    """Der vorhandene Eintrag zu diesem Schiff — oder `None`."""
    for s in (data.get('schiffe') or []):
        if _same_ship(s, name, manufacturer, kurz, hkurz):
            return s
    return None


def contains(data, name, manufacturer='', kurz='', hkurz=''):
    """Steht dieses Schiff schon drin? Siehe `_same_ship`."""
    return find(data, name, manufacturer, kurz, hkurz) is not None


_IMPORT_KEYS = ('kurz', 'hkurz', 'lti', 'warbond', 'paket', 'gekauft', 'preis',
                'versicherung')


def _fill(entry, **rest):
    """Fehlende Angaben nachtragen, vorhandene nicht anrühren.

    ⚠ `lti` wird nur **gesetzt**, nie zurückgenommen: Ältere Exporte der
    Hangar Extension kennen das Feld `insurance` noch nicht und melden überall
    `False` — das darf ein LTI aus dem XPLORer-Import nicht löschen.
    """
    changed = False
    for key in _IMPORT_KEYS:
        value = rest.get(key)
        if value in (None, ''):
            continue
        if key in ('lti', 'versicherung') and entry.get(BASETOOL_COVER):
            continue
        if key in ('lti', 'warbond'):
            if value is True and entry.get(key) is not True:
                entry[key] = True
                changed = True
            continue
        if entry.get(key) in (None, '', 0):
            entry[key] = value
            changed = True
    return changed


def add(data, name, manufacturer='', origin=INGAME, **rest):
    """Ein Schiff eintragen. Gibt zurück, ob es neu war.

    Doppelte werden still übergangen — wer zweimal importiert, soll nicht jedes
    Schiff doppelt im Hangar stehen haben. Bringt der zweite Import Angaben
    mit, die dem vorhandenen Eintrag fehlen (Kürzel, Paket), werden sie
    nachgetragen.
    """
    if not (name or '').strip():
        return False
    found = find(data, name, manufacturer, rest.get('kurz'), rest.get('hkurz'))
    if found is None and origin == BASETOOL and not (manufacturer or '').strip():
        # Aus dem Basetool kommt nur der Name: Trägt genau ein Eintrag ihn
        # (bis auf ein Klassenwort), ist es dieser.
        same = [s for s in (data.get('schiffe') or [])
                if _names_compatible(_slim(s.get('name')), _slim(name))]
        if len(same) == 1:
            found = same[0]
    if found is None and origin != CATALOG:
        # Andere Schreibweise oder kein Hersteller (Basetool, von Hand):
        # das Beiboot laut Katalog ist trotzdem dasselbe Fahrzeug.
        from . import included_vehicles
        found = included_vehicles.catalog_entry(data, name, rest.get('hkurz'))
    if found is not None:
        # Ein Beiboot laut Katalog, das nun ein Export, das Basetool oder der
        # Spieler selbst nennt, ist ab jetzt ein gewöhnlicher Eintrag — und
        # zählt als neu, denn so stand es bisher nicht im Hangar.
        promoted = found.get('herkunft') == CATALOG and origin != CATALOG
        if promoted:
            _promote(found, name, manufacturer, origin, rest)
        elif found.get('herkunft') == BASETOOL \
                and origin not in (BASETOOL, CATALOG):
            _take_over(found, origin, rest.get('quelle'))
        _fill(found, **rest)
        return promoted
    entry = {'name': name.strip(), 'hersteller': (manufacturer or '').strip(),
             'herkunft': origin, 'belegung': {}}
    for key in _IMPORT_KEYS:
        if rest.get(key) not in (None, ''):
            entry[key] = rest[key]
    if rest.get('quelle'):
        entry['quelle'] = rest['quelle']
    data.setdefault('schiffe', []).append(entry)
    return True


def _take_over(entry, origin, source=None):
    """Ein Schiff aus dem Basetool, das nun ein Export oder der Spieler nennt.

    Es bekommt dessen Herkunft (und Quelle, bei einer Liste) und trägt
    `FROM_BASETOOL`. Name und Hersteller bleiben — an ihnen hängt die
    Verknüpfung mit dem Basetool (`exchange_ships.external_id`). Hat es aus
    dem Basetool schon eine Versicherung, trägt es `BASETOOL_COVER`: Dann
    ändert sie kein Import mehr (`_fill`), auch kein späterer."""
    entry['herkunft'] = origin or PLEDGE
    entry[FROM_BASETOOL] = True
    if source:
        entry['quelle'] = source
    if entry.get('lti') or _months(entry.get('versicherung')):
        entry[BASETOOL_COVER] = True


def _months(value):
    try:
        return max(0, int(value))
    except (TypeError, ValueError):
        return 0


def _promote(entry, name, manufacturer, origin, rest):
    """Aus einem Beiboot laut Katalog einen gewöhnlichen Eintrag machen.

    Name, Hersteller, Paket und Quelle kommen von dort, wo das Fahrzeug jetzt
    genannt wird — der Träger unter `paket` stimmte nur für den Katalog.
    Auslegung und Haken des Spielers bleiben."""
    entry['herkunft'] = origin or PLEDGE
    if (name or '').strip():
        entry['name'] = name.strip()
    if (manufacturer or '').strip():
        entry['hersteller'] = manufacturer.strip()
    if (rest.get('hkurz') or '').strip():
        entry['hkurz'] = rest['hkurz'].strip()
    entry['paket'] = (rest.get('paket') or '').strip()
    if rest.get('quelle'):
        entry['quelle'] = rest['quelle']
    else:
        entry.pop('quelle', None)


def bundled_with(data, entry):
    """Der Name des Hangar-Schiffs, in dessen Paket dieses Schiff steckt — oder `''`.

    Die Hangar Extension schreibt unter `includedWith` das Schiff, mit dem
    eines mitkam (die URSA der Carrack, die MPUV Personnel der Idris-P). Beim
    XPLORer steht unter `paket` dagegen der **Pledge-Name** (`Standalone
    Ship`, `Package - Dominus Pack`) — das ist keine Beilage. Unterschieden
    wird deshalb daran, ob der Wert ein Schiff **im eigenen Hangar** nennt.

    ⚠ Verglichen wird nur bis zum ersten ` - `: Die Pledge-Seite schreibt
    `idris p - frigate`, gemeint ist die `Idris-P`.
    """
    wanted = _slim((entry.get('paket') or '').split(' - ', 1)[0])
    if not wanted or wanted == _slim(entry.get('name')):
        return ''
    for s in (data.get('schiffe') or []):
        if s is entry:
            continue
        if _slim(s.get('name')) == wanted:
            return s.get('name') or ''
    return ''


def merge_duplicates(data):
    """Doppelte Einträge zusammenführen. Gibt zurück, ob sich etwas änderte.

    Der **erste** Eintrag bleibt (er trägt die Belegung und die älteren
    Angaben wie LTI), die späteren geben ab, was ihm fehlt, und verschwinden.
    Läuft bei jedem Laden — so räumt sich ein Hangar, der vor diesem Fix
    doppelt importiert wurde, von selbst auf.

    ⚠ Ein Schiff aus dem Basetool bleibt auch als späterer Eintrag stehen und
    nimmt den anderen auf (`_take_over`): Seine Kennung ist die, mit der es im
    Basetool verknüpft ist.
    """
    kept = []
    changed = False
    for s in (data.get('schiffe') or []):
        match = None
        for k in kept:
            if _same_ship(k, s.get('name'), s.get('hersteller'),
                          s.get('kurz'), s.get('hkurz')) \
                    or _same_ship(s, k.get('name'), k.get('hersteller'),
                                  k.get('kurz'), k.get('hkurz')):
                match = k
                break
        if match is None:
            kept.append(s)
            continue
        changed = True
        if s.get('herkunft') == BASETOOL and match.get('herkunft') != BASETOOL:
            index = next(i for i, k in enumerate(kept) if k is match)
            kept[index], match, s = s, s, match
        rest = {key: s.get(key) for key in _IMPORT_KEYS}
        if match.get('herkunft') == CATALOG \
                and s.get('herkunft') != CATALOG:
            _promote(match, s.get('name'), s.get('hersteller'),
                     s.get('herkunft'), dict(rest, quelle=s.get('quelle')))
        elif s.get('herkunft') == CATALOG:
            # Der Träger eines Katalog-Eintrags ist kein Paket des Pledges.
            rest['paket'] = ''
        elif match.get('herkunft') == BASETOOL \
                and s.get('herkunft') != BASETOOL:
            _take_over(match, s.get('herkunft'), s.get('quelle'))
        _fill(match, **rest)
        if not match.get('belegung') and s.get('belegung'):
            match['belegung'] = s['belegung']
        if match.get('einsatzbereit') is None \
                and s.get('einsatzbereit') is not None:
            match['einsatzbereit'] = s['einsatzbereit']
        if match.get('einheit') is None and s.get('einheit') is not None:
            match['einheit'] = s['einheit']
        for key in ('schmelzbar', 'schmelz_wahl'):
            if match.get(key) is None and s.get(key) is not None:
                match[key] = s[key]
    if changed:
        data['schiffe'] = kept
    return changed


def remove(data, name, manufacturer=''):
    """Ein Schiff austragen. Gibt zurück, ob eines wegfiel.

    Seine Beiboote laut Katalog gehen mit (`included_vehicles.prune`); ein
    ausgetragenes Beiboot laut Katalog kommt erst mit dem nächsten Import
    wieder."""
    from . import included_vehicles
    wanted = _slim(manufacturer) + _slim(name)
    before = len(data.get('schiffe') or [])
    data['schiffe'] = [
        s for s in (data.get('schiffe') or [])
        if _slim(s.get('hersteller')) + _slim(s.get('name')) != wanted]
    if len(data['schiffe']) == before:
        return False
    included_vehicles.prune(data)
    return True


# ---------------------------------------------------------------- Import

_EXT_INSURANCE = re.compile(r'^\s*(\d+)\s*(?:mi|mo|months?)?\s*$',
                            re.IGNORECASE)


def _extension_insurance(entry):
    """Die Versicherung aus dem JSON der Hangar Extension → `(lti, monate)`.

    Sie steht in **einem** Feld, im selben Zuschnitt, den der Webhangar zeigt:
    `"insurance": "LTI"` oder `"insurance": "120MI"` — Dauer in Monaten.

    ⚠ **`"0MI"` ist keine Versicherung von null Monaten, sondern gar keine
    Angabe.** Das ist der Rückfallwert der Erweiterung für Pledges, die die
    Information nicht führen — er landet deshalb wie ein fehlendes Feld.

    ⚠ **Fehlt das Feld, gibt es keine Angabe** — kein LTI, keine Dauer. Die
    Schiffszeile bleibt dann ohne Versicherung, statt „keine" zu behaupten.

    ⚠ Ältere Exporte der Erweiterung führen zwei Felder (`insurance` als
    Wahrheitswert, `insuranceMonths` als Zahl). Beide werden weiter gelesen,
    damit solche Exporte gültig bleiben.
    """
    value = entry.get('insurance')
    lti = False
    months = 0
    if isinstance(value, bool):
        lti = value
    elif isinstance(value, str):
        text = value.strip()
        if text.lower() == 'lti':
            lti = True
        else:
            match = _EXT_INSURANCE.match(text)
            if match:
                months = int(match.group(1))
    try:
        months = max(months, int(entry.get('insuranceMonths') or 0))
    except (TypeError, ValueError):
        pass
    return lti, months


def _hangar_extension_entry(entry, name_first=False):
    """Ein Schiff aus dem JSON der **Star Citizen: Hangar Extension** (AlyxOne).

    Die empfohlene Erweiterung — ein gepflegter Fork des XPLORer, MIT. Ihr
    Export sieht anders aus:

        {"manufacturer": {"code": "ARGO", "name": "Argo Astronautics",
                          "shortName": "ARGO"},
         "code": "ARGO_ATLS", "matrix": "ATLS", "name": "ATLS",
         "focus": "Cargo", "status": "Flight-Ready",
         "insurance": "LTI",                   # oder "120MI", wenn vorhanden
         "includedWith": "Idris-P",            # nur bei Paket-Beilagen
         "includedShips": ["G12"]}             # Fahrzeuge im selben Pledge

    ⚠ `matrix` vor `name`: `matrix` ist der Name aus der Ship-Matrix, je
    Ausführung eindeutig (`Prowler Utility`, `ATLS GEO`) und ohne
    Hersteller. `name` ist die Kurzform der Pledge-Seite (`prowler-utility`,
    `reliant kore - mini hauler`) und Rückfall, wenn `matrix` fehlt — dann
    ist `name` wieder der saubere Name.

    ⚠ Sondereditionen tragen in `matrix` ihren eigenen Namen (`Caterpillar
    Pirate Edition`). Teilen sich trotzdem zwei Einträge mit verschiedenem
    `code` eine `matrix`, wären sie über `_same_ship` ein Schiff. Die Sperre
    dagegen sitzt in `_from_json`, weil sie den ganzen Export braucht; sie
    ruft diese Funktion dann mit `name_first=True`.

    `includedShips` liest `_included_ships`, nicht diese Funktion.

    ⚠ **Warbond, Kaufdatum und Preis stehen nur im CSV-Export** — der JSON
    führt Schiffe und Fahrzeuge samt ihrer Beziehungen, dazu die Versicherung
    (siehe `_extension_insurance`). `includedWith` nennt das Paket, mit dem ein
    Schiff kam (die MPUV Personnel der Idris-P); das ist die Angabe, die beim
    XPLORer `pledge_name` heißt, und landet deshalb unter `paket`.
    """
    maker = entry.get('manufacturer') or {}
    matrix = (entry.get('matrix') or '').strip()
    label = (entry.get('name') or '').strip()
    name = (label or matrix) if name_first else (matrix or label)
    if not name:
        return None
    lti, months = _extension_insurance(entry)
    return {
        'name': name,
        'hersteller': (maker.get('name') or '').strip(),
        'kurz': (entry.get('code') or '').strip(),
        'hkurz': (maker.get('code') or maker.get('shortName') or '').strip(),
        'lti': lti,
        'warbond': False,
        # Monate nur ohne LTI — LTI ist die Dauer (wie beim CSV).
        'versicherung': (months if months and not lti else None),
        'paket': (entry.get('includedWith') or '').strip(),
        'gekauft': '',
        'preis': '',
    }


def _included_ships(raw, ships):
    """Die Fahrzeuge aus `includedShips`, die der Export nicht selbst führt.

    `raw` sind die Einträge des Exports, `ships` die daraus gelesenen
    Schiffe in derselben Reihenfolge (`None` für übergangene). Ein genanntes
    Fahrzeug wird ein eigenes Schiff mit dem Träger unter `paket` — wie
    `includedWith`, damit `bundled_with` es dem Träger zuordnet.

    ⚠ Kein Doppel: Steht das Fahrzeug schon als eigener Eintrag im Export
    (oft mit `includedWith`), bleibt es bei diesem. Nennen mehrere Träger
    dasselbe Fahrzeug, entsteht es einmal.

    Den Hersteller nennt der Export hier nicht; er kommt aus dem
    Schiffskatalog (`_catalog_name`). Ohne Katalog bleibt er leer, und
    `beilage` sagt `import_entries`, dass dann der Name zählt.
    """
    known = set()
    for entry, ship in zip(raw, ships):
        if ship:
            for value in (ship.get('name'), entry.get('matrix'),
                          entry.get('name')):
                if _slim(value):
                    known.add(_slim(value))
    known_codes = {_slim(ship.get('kurz')) for ship in ships
                   if ship and _slim(ship.get('kurz'))}
    result = []
    for entry, ship in zip(raw, ships):
        if not ship:
            continue
        for included in entry.get('includedShips') or ():
            # Die Hangar Extension nennt nur den Namen, die Schiffsliste
            # `hangar-schiffe` `{"name": …, "code": …}`.
            if isinstance(included, dict):
                label, code = included.get('name'), included.get('code')
            else:
                label, code = included, ''
            code = code.strip() if isinstance(code, str) else ''
            if not isinstance(label, str) or not _slim(label):
                continue
            if _slim(label) in known or (code and _slim(code) in known_codes):
                continue
            known.add(_slim(label))
            if code:
                known_codes.add(_slim(code))
            name, maker, maker_code = _catalog_name(label.strip())
            result.append({
                'name': name, 'hersteller': maker, 'kurz': code,
                'hkurz': maker_code, 'lti': ship.get('lti'),
                'warbond': False, 'versicherung': ship.get('versicherung'),
                'paket': ship['name'], 'gekauft': '', 'preis': '',
                'beilage': True})
    return result


# ------------------------------------------------ Schiffslisten
#
# ⭐ Drei weitere Werkzeuge, deren Listen auch das KRT Profit Basetool
# einliest. Die Formate stehen im
# offenen Quellcode des Basetools (`FleetExportParser.java`, GPL-3.0):
#
# | Werkzeug | Wurzel | ein Schiff |
# |---|---|---|
# | CCU Game **Fleetview** | Liste | `{"name": "aurora mr", "shipname": "", "type": "ship"}` — je Schiff eine Zeile, klein geschrieben, **ohne Hersteller** |
# | **Fleetyards** | Liste | `{"name": "A1 Spirit", "slug": "crus-a1-spirit", "manufacturerName": "Crusader Industries", "manufacturerCode": "CRUS", "shipName": "Koto", "wanted": false}` |
# | **StarJump** FleetViewer | Objekt `{"type": "starjumpFleetviewer", "canvasItems": [...]}` | `{"itemType": "SHIP", "shipSlug": "perseus", "defaultText": "Perseus"}` |
#
# ⚠⚠ **Eine Schiffsliste ist KEIN Pledge-Export.** Sie kennt oft auch im Spiel
# gekaufte Schiffe und ist selten vollständig. Sie **ergänzt** deshalb nur —
# ausgetragen wird dabei nichts (`import_entries`), und die Schiffe tragen die
# Herkunft `LIST`, damit ein späterer Pledge-Import sie nicht wegräumt.
#
# ⚠ Keine Versicherung, kein Paket, kein Preis: Keine der drei Listen führt
# das. Eigene Schiffsnamen (`shipname`) werden nicht übernommen — VerseKits
# eigene Namen sind die Namen am Terminal im Spiel, etwas anderes.


def _nice(name):
    """`aurora mr` → `Aurora MR` — nur, wenn die Liste alles klein schreibt."""
    name = (name or '').strip()
    if not name or name != name.lower():
        return name
    words = []
    for word in name.split():
        words.append(word.upper() if len(word) <= 2 and word.isalpha()
                     else word[:1].upper() + word[1:])
    return ' '.join(words)


def _catalog_name(name):
    """Name ohne Hersteller → `(Name, Hersteller, Kürzel)` aus dem Schiffskatalog.

    `890 jump` → (`890 Jump`, `Origin`, `ORIG`). UEX führt den Hersteller im
    Namen (`Origin 890 Jump`), `_split_maker` trennt ihn wieder ab — derselbe
    Weg wie beim CSV der Hangar Extension, damit dasselbe Schiff dieselben
    Kürzel trägt und nicht doppelt im Hangar landet."""
    try:
        from . import ships
        entry = ships._find(name)
    except Exception:
        entry = None
    if not entry:
        return _nice(name), '', ''
    maker, code, rest = _split_maker(entry.get('name') or '')
    if not rest or not maker:
        return _nice(name), '', ''
    return rest, maker, code


def _list_entry(name, source, maker='', maker_code='', wanted=False):
    name = (name or '').strip()
    if not name:
        return None
    if not maker_code:
        name, maker, maker_code = _catalog_name(name)
    return {'name': name, 'hersteller': (maker or '').strip(), 'kurz': '',
            'hkurz': (maker_code or '').strip(), 'lti': False,
            'warbond': False, 'paket': '', 'gekauft': '', 'preis': '',
            'herkunft': LIST, 'quelle': source, 'wunsch': bool(wanted)}


def _from_fleetview(entry):
    if entry.get('type') not in (None, '', 'ship'):
        return None
    return _list_entry(entry.get('name'), 'Fleetview')


def _from_fleetyards(entry):
    name = entry.get('name') or ''
    if not name.strip():
        return None
    return _list_entry(name, 'Fleetyards',
                       entry.get('manufacturerName') or '',
                       entry.get('manufacturerCode') or '',
                       wanted=entry.get('wanted') is True)


def _from_starjump(raw):
    """Der FleetViewer von StarJump — ein Objekt, die Schiffe unter `canvasItems`."""
    result = []
    for item in raw.get('canvasItems') or ():
        if not isinstance(item, dict) or \
                (item.get('itemType') or '').lower() != 'ship':
            continue
        name = (item.get('defaultText') or '').strip()
        if not name:
            # Nur der Kurzname: `zeus-mkii-mr` → `zeus mkii mr`
            name = (item.get('shipSlug') or '').replace('-', ' ').strip()
        ship = _list_entry(name, 'StarJump')
        if ship:
            result.append(ship)
    return result


def _is_starjump(raw):
    return isinstance(raw, dict) and (
        str(raw.get('type') or '').lower() == 'starjumpfleetviewer'
        or isinstance(raw.get('canvasItems'), list))


# ------------------------------------- Schiffsliste `hangar-schiffe`
#
# Eine Schiffsliste ohne persönliche Angaben, ein Objekt mit Formatkennung und
# Version:
#
#     {"schema": "hangar-schiffe", "version": 1, …,
#      "schiffe": [{"pledgeId": 900005, "matrix": "600i Explorer",
#                   "matrixZuordnung": "sicher", "name": "600i Explorer",
#                   "code": "ORIG_600i_Explorer", "rsiKennung": "…",
#                   "manufacturer": {"code": "ORIG", "name": "Origin Jumpworks"},
#                   "insurance": "LTI", "insuranceAppliesTo": "ship",
#                   "includedWith": null,
#                   "includedShips": [{"name": "G12", "code": "ORIG_G12"}],
#                   "warbond": false, "purchaseDate": "2021-12-25",
#                   "pledgePriceUsd": 475}]}
#
# ⚠ Eine andere Version wird abgelehnt, nicht geraten (`ExportRejected`).
# Felder, die hier nicht vorkommen, werden übergangen.
#
# ⚠ `pledgePriceUsd` ist der Preis des **ganzen** Pledges und steht bei einem
# Paket an jedem Schiff. Er landet je Schiff unter `preis` und wird nirgends
# zusammengezählt. `pledgeId` wird nicht übernommen.

HANGAR_SHIPS_SCHEMA = 'hangar-schiffe'
HANGAR_SHIPS_VERSIONS = (1,)
# Die andere Ausgabe derselben Erweiterung — eine Übergabedatei je Pledge.
HANGAR_TRANSFER_SCHEMA = 'hangar-uebergabe'

# Fehlermarken, die `read()` statt eines Fehlertexts zurückgibt.
UNSUPPORTED_VERSION = 'version'
TRANSFER_FILE = 'uebergabe'

# `matrixZuordnung`: bei diesen Werten ist `matrix` der Schiffsname.
_MATRIX_MATCHES = ('sicher', 'naeherung')


class ExportRejected(ValueError):
    """Eine Datei, die `read()` bewusst nicht liest — `args[0]` ist die
    Fehlermarke (`UNSUPPORTED_VERSION`, `TRANSFER_FILE`)."""


def _text_field(entry, key):
    value = entry.get(key)
    return value.strip() if isinstance(value, str) else ''


def _usd(value):
    """`475` → `$475.00 USD`, dieselbe Form wie im Export des XPLORer."""
    if isinstance(value, bool) or not isinstance(value, (int, float)) \
            or value < 0:
        return ''
    return '$%.2f USD' % value


def _hangar_ships_matrix(entry):
    """`matrix`, wenn `matrixZuordnung` sie als Schiffsnamen ausweist — sonst `''`."""
    if entry.get('matrixZuordnung') not in _MATRIX_MATCHES:
        return ''
    return _text_field(entry, 'matrix')


def _hangar_ships_entry(entry, name_first=False):
    """Ein Schiff der Schiffsliste `hangar-schiffe`.

    Der Name kommt aus `matrix`, wenn `matrixZuordnung` `sicher` oder
    `naeherung` sagt, sonst aus `name` (dem Titel im Pledge). Bei einer
    Näherung steht der Titel unter `naeherung`, damit die Hangar-Seite sie
    nach dem Import nennen kann. `name_first` wie bei
    `_hangar_extension_entry`: Einträge mit geteilter `matrix` behalten
    `name`.

    `includedWith` ist der Paketname und landet unter `paket`. Die
    Versicherung gilt je Schiff, auch bei `insuranceAppliesTo: "package"`.
    """
    maker = entry.get('manufacturer')
    maker = maker if isinstance(maker, dict) else {}
    matrix = _hangar_ships_matrix(entry)
    label = _text_field(entry, 'name')
    name = (label or matrix) if name_first else (matrix or label)
    if not name:
        return None
    lti, months = _extension_insurance(entry)
    ship = {
        'name': name,
        'hersteller': _text_field(maker, 'name'),
        'kurz': _text_field(entry, 'code') or _text_field(entry, 'rsiKennung'),
        'hkurz': _text_field(maker, 'code'),
        'lti': lti,
        'warbond': entry.get('warbond') is True,
        'versicherung': (months if months and not lti else None),
        'paket': _text_field(entry, 'includedWith'),
        'gekauft': _text_field(entry, 'purchaseDate'),
        'preis': _usd(entry.get('pledgePriceUsd')),
    }
    if name == matrix and entry.get('matrixZuordnung') == 'naeherung' \
            and label and _slim(label) != _slim(matrix):
        ship['naeherung'] = label
    return ship


def _from_hangar_ships(raw):
    """Die Schiffsliste `hangar-schiffe` → Einträge wie bei `_from_json`.

    ⚠ **Geteilte `matrix`** wie bei der Hangar Extension: Zeigen Einträge mit
    verschiedenem `code` auf dieselbe `matrix` (eine Näherung landet leicht
    auf dem Grundtyp), behalten genau diese ihren Titel.
    """
    version = raw.get('version')
    if isinstance(version, bool) or version not in HANGAR_SHIPS_VERSIONS:
        raise ExportRejected(UNSUPPORTED_VERSION)
    entries = raw.get('schiffe')
    entries = [e for e in entries if isinstance(e, dict)] \
        if isinstance(entries, list) else []
    codes_by_matrix = {}
    for entry in entries:
        matrix = _slim(_hangar_ships_matrix(entry))
        if matrix:
            codes_by_matrix.setdefault(matrix, set()).add(
                _text_field(entry, 'code'))
    shared = {m for m, codes in codes_by_matrix.items() if len(codes) > 1}
    ships = [_hangar_ships_entry(
        entry, name_first=_slim(_hangar_ships_matrix(entry)) in shared)
        for entry in entries]
    # `_included_ships` sieht nur Zeichenketten und die Beiboot-Liste.
    views = [{'matrix': _hangar_ships_matrix(entry),
              'name': _text_field(entry, 'name'),
              'includedShips': (entry.get('includedShips')
                                if isinstance(entry.get('includedShips'), list)
                                else [])}
             for entry in entries]
    result = [ship for ship in ships if ship]
    result.extend(_included_ships(views, ships))
    return result


def _from_json(text):
    """Der JSON-Export — vom Hangar XPLORer **oder** von der Hangar Extension.

    Zwei Werkzeuge, zwei Formen, ein Erkenner:

    | | Hangar XPLORer (dolkensp) | Hangar Extension (AlyxOne) |
    |---|---|---|
    | Eintrag | flach: `name`, `ship_name`, `manufacturer_name`, `manufacturer_code`, `ship_code`, `lti`, `pledge_*` | `matrix`, `name`, `code`, `manufacturer` als **Wörterbuch** |
    | Schiffsname | `name` vor `ship_name` | `matrix` vor `name` — außer bei geteilter `matrix` (unten) |
    | Umfang | Schiffe **und** Ausrüstung, Farben, Anzüge (`entity_type`) | nur Schiffe und Fahrzeuge |
    | Pledge-Angaben | LTI, Warbond, Paket, Datum, Preis | Versicherung und Paket — kein Warbond, Datum, Preis (siehe `_hangar_extension_entry`) |
    | Beilagen | eigene Einträge | eigene Einträge mit `includedWith`, dazu `includedShips` am Träger (siehe `_included_ships`) |

    Erkannt wird **je Eintrag** am Wörterbuch `manufacturer` — kein Werkzeug
    schreibt das Feld des anderen. Beide werden eingelesen, auch ältere
    XPLORer-Exporte.

    ⚠ **Geteilte `matrix`:** Tragen Einträge mit verschiedenem `code`
    dieselbe `matrix` (Grundtyp und Sonderedition ohne eigenen
    Matrix-Eintrag), behalten genau diese Einträge `name`. Sonst hießen
    beide gleich und `_same_ship` machte beim Import eines daraus.
    """
    raw = json.loads(text)
    if isinstance(raw, dict):
        schema = raw.get('schema')
        if schema == HANGAR_SHIPS_SCHEMA:
            return _from_hangar_ships(raw)
        if schema == HANGAR_TRANSFER_SCHEMA:
            raise ExportRejected(TRANSFER_FILE)
    if _is_starjump(raw):
        return _from_starjump(raw)
    if not isinstance(raw, list):
        return []
    codes_by_matrix = {}
    for entry in raw:
        if isinstance(entry, dict) and isinstance(entry.get('manufacturer'),
                                                  dict):
            matrix = _slim(entry.get('matrix'))
            if matrix:
                codes_by_matrix.setdefault(matrix, set()).add(
                    (entry.get('code') or '').strip())
    shared = {m for m, codes in codes_by_matrix.items() if len(codes) > 1}
    result = []
    ext_raw, ext_ships = [], []
    for entry in raw:
        if not isinstance(entry, dict):
            continue
        if isinstance(entry.get('manufacturer'), dict):
            ship = _hangar_extension_entry(
                entry, name_first=_slim(entry.get('matrix')) in shared)
            ext_raw.append(entry)
            ext_ships.append(ship)
            if ship:
                result.append(ship)
            continue
        # Fleetyards (camelCase) und Fleetview — erkannt wie im Basetool, an
        # Feldern, die der XPLORer nie schreibt (der führt `ship_code`,
        # `pledge_id`, `entity_type`).
        if not any(k in entry for k in ('pledge_id', 'ship_code',
                                        'entity_type', 'ship_name')):
            if 'shipCode' in entry or 'manufacturerCode' in entry \
                    or 'manufacturerName' in entry or 'slug' in entry:
                ship = _from_fleetyards(entry)
            elif 'shipname' in entry or 'type' in entry:
                ship = _from_fleetview(entry)
            else:
                ship = False
            if ship is not False:
                if ship:
                    result.append(ship)
                continue
        # ⚠ Nur Schiffe. Der Export führt auch Ausrüstung, Farben und Anzüge —
        # ein `Bosco Weapon Display Rack` hat keine Steckplätze.
        if entry.get('entity_type') not in (None, '', 'ship'):
            continue
        # ⚠⚠ **`name` vor `ship_name`** — und das ist kein Geschmack.
        # `ship_name` ist der Grundtyp, `name` die tatsächliche Ausführung.
        # Andersherum würden aus `A.T.L.S.` und `ATLS GEO` zwei Einträge
        # desselben Namens, von denen der zweite als Doppel wegfiele; und die
        # `F7C-M Super Hornet Mk II` hieße Mk I, weil ihr `ship_code` noch auf
        # der alten Ausführung steht.
        name = (entry.get('name') or entry.get('ship_name') or '').strip()
        if not name:
            continue
        result.append({
            'name': name,
            'hersteller': (entry.get('manufacturer_name') or '').strip(),
            'kurz': (entry.get('ship_code') or '').strip(),
            # ⚠ Das Herstellerkürzel ist für die Zuordnung wichtiger als der
            # ausgeschriebene Name: Erkul führt `Roberts Space Industries` als
            # `rsi`. Ohne dieses Feld findet etwa die Ursa Medivac keinen Anschluss.
            'hkurz': (entry.get('manufacturer_code') or '').strip(),
            'lti': bool(entry.get('lti')),
            'warbond': bool(entry.get('warbond')),
            'paket': (entry.get('pledge_name') or '').strip(),
            'gekauft': (entry.get('pledge_date') or '').strip(),
            'preis': (entry.get('pledge_cost') or '').strip(),
        })
    result.extend(_included_ships(ext_raw, ext_ships))
    return result


# Wie die Hangar Extension den Hersteller im CSV vor den Schiffsnamen setzt
# (`Aegis Idris-P`, `RSI Galaxy`, `Kruger L-22 Alpha Wolf`) → Kürzel, wie es
# ihr JSON-Export und erkul führen. Längste Namen zuerst, damit `Consolidated
# Outland` nicht an `Consolidated` scheitert.
_CSV_MAKERS = (
    ('Roberts Space Industries', 'RSI'), ('Consolidated Outland', 'CNOU'),
    ('Musashi Industrial & Starflight Concern', 'MISC'),
    ('Aegis', 'AEGS'), ('Anvil', 'ANVL'), ('Aopoa', 'AOPO'), ('ARGO', 'ARGO'),
    ('Argo', 'ARGO'), ('Banu', 'BANU'), ('Crusader', 'CRUS'), ('CNOU', 'CNOU'),
    ('Drake', 'DRAK'), ('Esperia', 'ESPR'), ('Gatac', 'GAMA'),
    ('Greycat', 'GRIN'), ('Kruger', 'KRIG'), ('MISC', 'MISC'),
    ('Mirai', 'MRAI'), ('Origin', 'ORIG'), ('RSI', 'RSI'), ('Tumbril', 'TMBL'),
    ('Vanduul', 'VNCL'), ("Xi'an", 'XIAN'), ('Xian', 'XIAN'),
)

_MONTHS = re.compile(r'^\s*(\d+)\s*month', re.IGNORECASE)


def _split_maker(full):
    """`Aegis Idris-P` → (`Aegis`, `AEGS`, `Idris-P`); ohne Treffer bleibt der Name ganz."""
    full = (full or '').strip()
    for maker, code in _CSV_MAKERS:
        if full.lower().startswith(maker.lower() + ' '):
            return maker, code, full[len(maker):].strip()
    return '', '', full


def _insurance(content):
    """`Lifetime Insurance` → (True, 0) · `120 Month Insurance` → (False, 120)."""
    text = (content or '').strip()
    if 'lifetime' in text.lower():
        return True, 0
    match = _MONTHS.match(text)
    return False, int(match.group(1)) if match else 0


def _from_extension_csv(rows):
    """Der CSV-Export der **Hangar Extension** — der Komplett-Export je Pledge.

    Eine Zeile je **Inhalt** eines Pledges: Schiffe, Farben, Ausrüstung,
    Versicherung. Zusammengehalten über `Pledge ID`. Gemessen an einem echten
    Export (751 Zeilen, 42 Schiffe):

    | `Content Type` | was es ist | wird |
    |---|---|---|
    | `Ship` | `Aegis Idris-P` — Hersteller und Name in einem Feld | ein Schiff |
    | `Included Ship` | Beilage eines Pakets (`ARGO MPUV Personnel`) | ein Schiff, `paket` = das Schiff des Pledges, sonst der Pledge-Name |
    | `Insurance` | `Lifetime Insurance`, `120 Month Insurance` — **eine je Pledge** | `lti` bzw. `versicherung` (Monate) für jedes Schiff des Pledges |
    | alles andere | Farben, Anzüge, Möbel, Gutscheine | übergangen |

    ⭐ **Das ist die Datei mit der Versicherungsdauer.** Der JSON-Export der
    Erweiterung kennt sie (noch) nicht — dafür kennt er Kürzel und
    Paketbeziehung. Wer beide einliest, bekommt beides; die Doppelerkennung
    (`_same_ship`) führt die Angaben zusammen.

    ⚠ `Pledge ID` und `Pledge Cost` sind privat — sie bleiben im Hangar und
    kommen nie in einen Bericht (siehe Modulkopf).
    """
    pledges = {}
    order = []
    for row in rows:
        pid = (row.get('Pledge ID') or '').strip()
        if pid not in pledges:
            pledges[pid] = {'ships': [], 'included': [], 'lti': False,
                            'months': 0,
                            'name': (row.get('Pledge Name') or '').strip(),
                            'date': (row.get('Pledge Date') or '').strip(),
                            'cost': (row.get('Pledge Cost') or '').strip(),
                            'meltable': _yes_no(row.get('Meltable'))}
            order.append(pid)
        if pledges[pid]['meltable'] is None:
            pledges[pid]['meltable'] = _yes_no(row.get('Meltable'))
        kind = (row.get('Content Type') or '').strip().lower()
        content = (row.get('Pledge Content') or '').strip()
        if kind == 'ship' and content:
            pledges[pid]['ships'].append(content)
        elif kind == 'included ship' and content:
            pledges[pid]['included'].append(content)
        elif kind == 'insurance':
            lti, months = _insurance(content)
            pledges[pid]['lti'] = pledges[pid]['lti'] or lti
            pledges[pid]['months'] = max(pledges[pid]['months'], months)
    result = []
    for pid in order:
        p = pledges[pid]
        for full, included in ([(s, False) for s in p['ships']]
                               + [(s, True) for s in p['included']]):
            maker, code, name = _split_maker(full)
            if not name:
                continue
            # Beilage: bei genau einem Schiff im Pledge ist das der Träger;
            # bei einem Paket mit vielen Schiffen bleibt es der Paketname.
            if included and len(p['ships']) == 1:
                paket = _split_maker(p['ships'][0])[2]
            else:
                paket = p['name']
            result.append({
                'name': name, 'hersteller': maker, 'kurz': '', 'hkurz': code,
                'lti': p['lti'], 'warbond': False, 'paket': paket,
                'gekauft': p['date'], 'preis': p['cost'],
                # Monate nur ohne LTI — LTI ist die Dauer.
                'versicherung': (p['months'] if p['months'] and not p['lti']
                                 else None),
                'schmelzbar': p['meltable'],
            })
    return result


def _yes_no(value):
    """`yes`/`no` aus der Spalte `Meltable` → `True`/`False`, sonst `None`."""
    text = (value or '').strip().lower()
    if text in ('yes', 'true', '1'):
        return True
    if text in ('no', 'false', '0'):
        return False
    return None


def _from_csv(text):
    """Der CSV-Export — vom Hangar XPLORer **oder** von der Hangar Extension.

    Erkannt an der Kopfzeile: Die Extension schreibt `Pledge ID` und
    `Content Type`, der XPLORer `Manufacturer, Ship, Lti, …`.

    ⚠ Die XPLORer-Kopfzeile trägt **Leerzeichen hinter den Kommas**. Ohne
    `skipinitialspace` heißt die zweite Spalte `' Ship'` und wird nie
    gefunden.
    """
    reader = csv.DictReader(io.StringIO(text), skipinitialspace=True)
    fields = [(f or '').strip() for f in (reader.fieldnames or [])]
    if 'Pledge ID' in fields and 'Content Type' in fields:
        return _from_extension_csv(list(reader))
    result = []
    for row in reader:
        name = (row.get('Ship') or '').strip()
        # ⚠ Der Export schreibt bei unbekannten Stücken wörtlich `undefined`
        # in die Namensspalte — das ist kein Schiff, sondern eine Lücke.
        if not name or name == 'undefined':
            continue
        result.append({
            'name': name,
            'hersteller': (row.get('Manufacturer') or '').strip(),
            'kurz': '',
            'hkurz': '',
            'lti': (row.get('Lti') or '').strip().lower() == 'true',
            'warbond': (row.get('Warbond') or '').strip().lower() == 'true',
            'paket': (row.get('Pledge') or '').strip(),
            'gekauft': (row.get('Date') or '').strip(),
            'preis': (row.get('Cost') or '').strip(),
        })
    return result


def read(file_path):
    """Eine Exportdatei einlesen. Gibt `(eintraege, fehlertext)` zurück.

    Erkannt wird am Inhalt, nicht an der Endung — wer eine `.json` in `.txt`
    umbenennt, soll trotzdem weiterkommen.

    Eine Datei, die bewusst nicht gelesen wird (`ExportRejected`), gibt statt
    des Fehlertexts ihre Fehlermarke zurück und landet nicht im
    Fehlerprotokoll.
    """
    try:
        with open(file_path, encoding='utf-8-sig') as f:
            text = f.read()
    except Exception as exc:
        errors.record('fleet.read', exc)
        return [], str(exc)
    head = text.lstrip()[:1]
    try:
        # `{` = StarJump FleetViewer und `hangar-schiffe` (ein Objekt),
        # `[` = alle übrigen JSON.
        entries = _from_json(text) if head in '[{' and head else \
            _from_csv(text)
    except ExportRejected as exc:
        return [], exc.args[0]
    except Exception as exc:
        errors.record('fleet.read.parse', exc)
        return [], str(exc)
    return entries, ''


def import_entries(entries, data=None, save_now=True):
    """Gelesene Einträge in den Hangar übernehmen.

    Gibt `(neu, schon_da, ausgetragen)` zurück — `ausgetragen` sind die Namen.

    ⚠ **Ein Export ist der ganze Pledge-Hangar, keine Ergänzung.** Käme nur
    hinzu, was neu ist, bliebe nach einem Upgrade (etwa Prospector ➔ Sabre
    Raven EX) das alte Schiff im Hangar stehen. Deshalb fällt ein
    Echtgeld-Schiff heraus, das im Export fehlt. Im Spiel gekaufte (`INGAME`)
    bleiben: Die kennt kein Export.

    Zuletzt ergänzt `included_vehicles.apply` die Beiboote laut Katalog und
    trägt die aus, deren Träger nicht mehr da ist.
    """
    from . import included_vehicles
    data = data if data is not None else load()
    new = 0
    # ⚠⚠ Nur ein **Pledge-Export** ist der ganze Hangar. Eine Schiffsliste
    # (Fleetview, Fleetyards, StarJump — Herkunft `LIST`) ergänzt nur.
    pledge_export = all((e.get('herkunft') or PLEDGE) == PLEDGE
                        for e in entries)
    wishes = [e for e in entries if e.get('wunsch')]
    by_name = []
    for e in entries:
        origin = e.get('herkunft') or PLEDGE
        if e.get('wunsch'):
            continue
        if (origin == LIST or e.get('beilage')) \
                and not e.get('hkurz') and not e.get('hersteller'):
            # Ohne Hersteller greift `_same_ship` nur bei Schiffen aus dem
            # Basetool (Weg 4). Steht genau EIN Schiff dieses Namens schon
            # drin, ist es dieses — sonst stünde es gleich doppelt da.
            same = [s for s in (data.get('schiffe') or [])
                    if _slim(s.get('name')) == _slim(e.get('name'))]
            if len(same) == 1:
                if same[0].get('herkunft') == CATALOG:
                    _promote(same[0], e.get('name'), '', origin, e)
                    new += 1
                if origin != LIST:
                    rest = {'paket': e.get('paket'), 'lti': e.get('lti'),
                            'versicherung': e.get('versicherung')}
                    if same[0].get('herkunft') == BASETOOL:
                        _take_over(same[0], origin, e.get('quelle'))
                    _fill(same[0], **rest)
                by_name.append(same[0])
                continue
        is_new = add(data, e.get('name'), e.get('hersteller'),
                     origin=origin, kurz=e.get('kurz'),
                     hkurz=e.get('hkurz'), lti=e.get('lti'),
                     warbond=e.get('warbond'), paket=e.get('paket'),
                     gekauft=e.get('gekauft'), preis=e.get('preis'),
                     versicherung=e.get('versicherung'),
                     quelle=e.get('quelle'))
        if is_new:
            new += 1
    _apply_meltable(data, entries)
    # Fleetyards führt auch Wunschschiffe (`wanted`) — die gehören auf die
    # Wunschliste, nicht in den Hangar. ⚠ Erst NACH dem Hangar: Kam dasselbe
    # Schiff in dieser Datei auch als eigenes herein, gehört es nicht auf die
    # Wunschliste.
    for e in wishes:
        if not contains(data, e.get('name'), e.get('hersteller'),
                        hkurz=e.get('hkurz')) \
                and not any(_slim(s.get('name')) == _slim(e.get('name'))
                            for s in (data.get('schiffe') or [])) \
                and included_vehicles.catalog_entry(
                    data, e.get('name'), e.get('hkurz')) is None:
            wishlist_add(data, e.get('name'), e.get('hersteller'))
    # Wunschschiffe zählen weder als übernommen noch als schon vorhanden.
    wished = len(wishes)
    removed = []
    if entries and pledge_export:
        kept, gone = [], []
        for ship in (data.get('schiffe') or []):
            in_export = any(ship is s for s in by_name) or any(
                _same_ship(ship, e.get('name'), e.get('hersteller'),
                           e.get('kurz'), e.get('hkurz'))
                for e in entries)
            if ship.get('herkunft') == PLEDGE and not in_export:
                gone.append(ship)
            else:
                kept.append(ship)
        if gone:
            # Ein Beiboot, das laut Katalog zu einem verbliebenen Träger
            # gehört, bleibt — als Katalog-Eintrag, mit seiner Auslegung.
            back = included_vehicles.demote({'schiffe': kept}, gone)
            kept.extend(back)
            removed = [s.get('name') or '' for s in gone
                       if not any(s is b for b in back)]
            data['schiffe'] = kept
    # Erst nach dem Austragen: Ein Beiboot laut Katalog folgt seinem Träger.
    # Es zählt weder als übernommen noch als ausgetragen — es stand in keiner
    # Datei.
    included_vehicles.apply(data)
    if save_now:
        save(data)
    return new, len(entries) - new - wished, removed


def _apply_meltable(data, entries):
    """`schmelzbar` aus dem Export an die Hangar-Schiffe schreiben.

    ⚠ Je Schiffstyp gibt es **einen** Eintrag (`merge_duplicates`). Steht
    ein Typ mehrfach im Export, gilt er als schmelzbar, sobald ein Exemplar
    es ist — dieses eine lässt sich schmelzen. Exporte ohne die Angabe
    lassen den gespeicherten Wert stehen.
    """
    for ship in (data.get('schiffe') or []):
        values = [e.get('schmelzbar') for e in entries
                  if isinstance(e.get('schmelzbar'), bool)
                  and _same_ship(ship, e.get('name'), e.get('hersteller'),
                                 e.get('kurz'), e.get('hkurz'))]
        if values:
            ship['schmelzbar'] = any(values)


def unknown(data=None):
    """Welche Schiffe im Hangar erkul **nicht** kennt.

    ⚠ Das ist meistens **keine Panne**, sondern eine Auskunft: Erkul führt nur
    Schiffe, die im Spiel flugfähig sind. Ein Treffer hier heißt in aller Regel,
    dass es das Schiff im Spiel noch nicht gibt — etwa Crucible, Endeavor,
    Galaxy, Liberator, Merchantman.
    """
    result = []
    for s in ((data or load()).get('schiffe') or []):
        if not erkul.knows(s.get('name'), s.get('hersteller'), s.get('kurz'),
                           s.get('hkurz')):
            result.append(s.get('name') or '')
    return result


def wishlist_id_sets(data=None):
    """Dasselbe für die Wunschliste — je Eintrag `(name, hersteller, '', '')`.

    ⚠ **Getrennt von `id_sets()`, mit Absicht.** Beide Listen holen dieselbe
    Art Daten, dürfen aber nie in denselben Topf: `id_sets()` speist auch die
    Auskunft, ob ein Teil ins eigene Schiff passt, und ein Wunschschiff hat
    man nicht. Wer
    die Listen hier zusammenlegt, beantwortet dort eine Frage über fremdes
    Eigentum.

    ⚠ **Ein fehlender Hersteller wird nachgeschlagen.** Ältere Einträge der
    Wunschliste haben keinen — und ohne ihn findet `erkul`
    einen Teil der Schiffe nicht. Statt diese Einträge stillschweigend
    auszulassen, wird der Hersteller bei UEX geholt. Geschrieben wird dabei
    nichts: Eine Leseabfrage, die nebenbei die Datei ändert, überrascht später
    an einer Stelle, an der niemand damit rechnet.
    """
    from . import ships
    result = []
    for w in ((data or load()).get('wunsch') or []):
        if not (isinstance(w, dict) and w.get('name')):
            continue
        name = w.get('name') or ''
        result.append((name, w.get('hersteller') or ships.manufacturer(name),
                       '', ''))
    return result


def fetch_missing(data=None):
    """Die Steckplätze aller Schiffe holen, soweit sie fehlen.

    Gibt zurück, wie viele Schiffe neu geholt wurden.

    ⚠ **Die Wunschliste zählt mit.** Wer sich ein Schiff vornimmt, plant oft
    gleich die Ausstattung mit. Ohne Steckplatz-Daten steht auf der Wunschzeile nur eine Kaufsumme, und
    die Ausstattung ließe sich erst planen, wenn das Schiff schon gekauft ist.
    Geholt wird nur — verrechnet werden Wunschschiffe nirgends als Besitz.
    """
    return erkul.add_missing(id_sets(data) + wishlist_id_sets(data))
