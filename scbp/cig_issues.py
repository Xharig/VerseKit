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
Bekannte Fehler in CIGs Spieldaten — und was dort eigentlich stehen sollte.

Der scmdb-Spiegel führt je Spiel-Build eine Datei
`cig_data_issues-<build>.json`. Der Parser dahinter bleibt bei den Rohdaten
und legt nur daneben ab, welcher Wert gemeint ist. Vier Abschnitte, jeder mit
einem `_comment`:

    blueprintRecords   je Bauplan-GUID: `expected` (entityClass, name) oder
                       slot + propertyKey + `expected` (modifierAtStart …)
    blueprintPools     je Topf-GUID: `duplicateReferences` — derselbe
                       Bauplan doppelt im Topf, `expected` der gemeinte oder
                       `null`
    entities           je entityClass: `expected` attachType / cgItemType
    contracts          je debugName: `field` + `expected` (factionGuid,
                       factionName)

Zugeordnet wird **nur** über GUID und debugName, nie über Namen. Die GUIDs
stammen aus demselben Parser wie `crafting_blueprints-<build>.json` (Feld
`guid`) und `merged-<build>.json` (`blueprintRecord`, `entityClass`) — sie
passen ohne Umrechnung.

Fehlt die Datei oder das Netz, gibt es schlicht keine Berichtigung; die
Rohdaten bleiben dann, wie sie sind.

Jeder berichtigte Datensatz trägt danach den Schlüssel `MARK` mit dem alten
Wert (`statt`) und der Erklärung (`note`) — daran erkennt die Oberfläche,
dass sie einen Hinweis zeigen soll.
"""
import hashlib
import json
import os
import time

from . import errors, paths

SOURCE = 'cig_data_issues-%s.json'
CACHE = 'cig-datenfehler.json'
FORMAT = 1

# Ohne Datei für den laufenden Build wird frühestens nach dieser Zeit erneut
# gefragt — erscheint sie später, kommt sie so trotzdem an.
RETRY_S = 6 * 3600

TIMEOUT = 30

# Schlüssel, unter dem ein berichtigter Datensatz den alten Wert trägt.
MARK = 'datenfehler'

OFF = os.environ.get('SC_BP_NO_NET', '') not in ('', '0')

_EMPTY = {'format': FORMAT, 'build': None, 'geholt': 0, 'daten': {}}

# Gelesener Stand im Speicher; `generation` zählt jedes Schreiben, damit
# abhängige Zwischenspeicher (Rezepte) merken, dass sie neu rechnen müssen.
_memory = {'daten': None, 'generation': 0}


# --------------------------------------------------------------- Ablage
def _path():
    return paths.app_file(CACHE)


def load():
    """Die abgelegte Fassung — `{'build':…, 'geholt':…, 'daten':{…}}`."""
    if _memory['daten'] is not None:
        return _memory['daten']
    data = dict(_EMPTY)
    try:
        with open(_path(), encoding='utf-8') as f:
            raw = json.load(f)
        if isinstance(raw, dict) and raw.get('format') == FORMAT \
                and isinstance(raw.get('daten'), dict):
            data = raw
    except (OSError, ValueError):
        pass
    except Exception as error:
        errors.record('cig_issues.load', error)
    _memory['daten'] = data
    return data


def _save(data):
    try:
        target = _path()
        os.makedirs(os.path.dirname(target), exist_ok=True)
        with open(target + '.tmp', 'w', encoding='utf-8') as f:
            json.dump(data, f, ensure_ascii=False)
        os.replace(target + '.tmp', target)
        return True
    except Exception as error:
        errors.record('cig_issues._save', error)
        return False
    finally:
        _memory['daten'] = None
        _memory['generation'] += 1


def generation():
    """Zähler, der bei jedem Schreiben der Ablage wächst."""
    return _memory['generation']


def forget():
    """Den Speicher leeren — die Ablage wird beim nächsten Zugriff neu gelesen."""
    _memory['daten'] = None
    _memory['generation'] += 1


def issues_for(build):
    """Die Fehlerliste zu genau diesem Build — `{}`, wenn keine vorliegt.

    Eine Liste zu einem anderen Build gilt nicht: Nach einem Patch können
    dieselben GUIDs andere Werte tragen.
    """
    if not build:
        return {}
    data = load()
    if data.get('build') != build:
        return {}
    return data.get('daten') or {}


def update(build):
    """Die Fehlerliste zum Build holen, wenn sie fehlt oder veraltet ist.

    Gibt `True` zurück, wenn sich der Inhalt geändert hat. Wirft nie.
    """
    if OFF or not build:
        return False
    old = load()
    now = time.time()
    if old.get('build') == build and (
            old.get('daten') or now - float(old.get('geholt') or 0) < RETRY_S):
        return False
    raw = None
    try:
        from .catalog import fetch_file
        raw = fetch_file(SOURCE % build, timeout=TIMEOUT, tries=1)
    except Exception as error:
        errors.record('cig_issues.update', error)
    if not isinstance(raw, dict):
        raw = old.get('daten') if old.get('build') == build else {}
    before = fingerprint(issues_for(build))
    _save({'format': FORMAT, 'build': build, 'geholt': now,
           'daten': raw or {}})
    return fingerprint(issues_for(build)) != before


def fingerprint(issues):
    """Kurze Prüfsumme über die Einträge — `''`, wenn es keine gibt.

    Wer Berichtigungen in eine eigene Ablage übernimmt, merkt sich diesen Wert
    und baut neu, sobald er sich ändert.
    """
    entries = {section: _entries(issues, section)
               for section in ('blueprintRecords', 'blueprintPools',
                               'entities', 'contracts')}
    if not any(entries.values()):
        return ''
    text = json.dumps(entries, sort_keys=True, ensure_ascii=False)
    return hashlib.sha1(text.encode('utf-8')).hexdigest()[:12]


# -------------------------------------------------------------- Nachschlagen
def _entries(issues, section):
    """Die Einträge eines Abschnitts ohne `_comment`, GUIDs klein geschrieben."""
    block = (issues or {}).get(section)
    if not isinstance(block, dict):
        return {}
    return {(k.lower() if section != 'contracts' else k): v
            for k, v in block.items()
            if not k.startswith('_') and isinstance(v, dict)}


def _guid(value):
    return (value or '').strip().lower() if isinstance(value, str) else ''


def blueprint_issue(issues, guid):
    """Der Eintrag zu einer Bauplan-GUID — oder `None`."""
    return _entries(issues, 'blueprintRecords').get(_guid(guid))


def pool_issue(issues, guid):
    """Der Eintrag zu einer Topf-GUID — oder `None`."""
    return _entries(issues, 'blueprintPools').get(_guid(guid))


def entity_issue(issues, entity_class):
    """Der Eintrag zu einer entityClass — oder `None`."""
    return _entries(issues, 'entities').get(_guid(entity_class))


def contract_issue(issues, debug_name):
    """Der Eintrag zu einem Vertrag (debugName) — oder `None`."""
    return _entries(issues, 'contracts').get(debug_name or '')


def faction_fix(issues, debug_name):
    """`(factionGuid, factionName, note)` für einen Vertrag — oder `None`.

    Nur Einträge mit `field == 'factionGuid'` und einem gesetzten `expected`.
    """
    entry = contract_issue(issues, debug_name)
    if not entry or entry.get('field') != 'factionGuid':
        return None
    expected = entry.get('expected')
    if not isinstance(expected, dict) or not expected.get('factionGuid'):
        return None
    return (_guid(expected['factionGuid']), expected.get('factionName') or '',
            entry.get('note') or '')


# -------------------------------------------------------------- Anwenden
def _mark(record, before, note):
    record[MARK] = {'statt': before, 'note': note or ''}


def apply_blueprints(blueprints, issues, products=None):
    """Rezept-Einträge aus `crafting_blueprints` berichtigen. Gibt die Zahl
    der Änderungen zurück.

    * `expected.entityClass` / `expected.name` → `productEntityClass` /
      `productName`; steht das neue Produkt in `products`, kommt auch sein
      Hersteller mit.
    * slot + propertyKey + `expected` → die Werte des Modifikators in genau
      diesem Slot. Treffen mehrere Spannen, bleibt alles, wie es ist — welche
      gemeint ist, sagt die Liste dann nicht.
    """
    changed = 0
    if not _entries(issues, 'blueprintRecords'):
        return 0
    for b in blueprints or []:
        entry = blueprint_issue(issues, b.get('guid'))
        if not entry:
            continue
        expected = entry.get('expected')
        if not isinstance(expected, dict):
            continue
        note = entry.get('note') or ''
        if entry.get('slot') and entry.get('propertyKey'):
            changed += _apply_modifier(b, entry['slot'], entry['propertyKey'],
                                       expected, note)
            continue
        before = {}
        new_entity = expected.get('entityClass')
        if new_entity and _guid(b.get('productEntityClass')) != _guid(new_entity):
            before['entityClass'] = b.get('productEntityClass')
            b['productEntityClass'] = new_entity
            product = (products or {}).get(new_entity) or {}
            if product.get('manufacturer'):
                b['manufacturer'] = product['manufacturer']
        new_name = expected.get('name')
        if new_name and b.get('productName') != new_name:
            before['name'] = b.get('productName')
            b['productName'] = new_name
        if before:
            _mark(b, before, note)
            changed += 1
    return changed


def _apply_modifier(blueprint, slot_name, property_key, expected, note):
    hits = []
    for tier in blueprint.get('tiers') or []:
        for slot in tier.get('slots') or []:
            if slot.get('name') != slot_name:
                continue
            for m in slot.get('modifiers') or []:
                if m.get('propertyKey') == property_key:
                    hits.append(m)
    if len(hits) != 1:
        return 0
    modifier = hits[0]
    before = {k: modifier.get(k) for k, v in expected.items()
              if modifier.get(k) != v}
    if not before:
        return 0
    modifier.update(expected)
    _mark(modifier, before, note)
    return 1


def apply_items(items, issues):
    """Gegenstände (`crafting_items`, Produkte) berichtigen: attachType und
    cgItemType nach `entities`. Gibt die Zahl der Änderungen zurück."""
    changed = 0
    if not _entries(issues, 'entities'):
        return 0
    for item in items or []:
        if not isinstance(item, dict):
            continue
        entry = entity_issue(issues, item.get('entityClass'))
        if not entry or not isinstance(entry.get('expected'), dict):
            continue
        before = {k: item.get(k) for k, v in entry['expected'].items()
                  if v and item.get(k) != v}
        if not before:
            continue
        item.update({k: v for k, v in entry['expected'].items() if v})
        _mark(item, before, entry.get('note'))
        changed += 1
    return changed


def apply_merged(merged, issues):
    """Die Sammeldatei (`merged-<build>.json`) berichtigen.

    * Topf-Einträge, deren Bauplan ein anderes Produkt meint, bekommen dessen
      Namen und entityClass.
    * Doppelte Verweise in einem Topf: der zweite wird durch den gemeinten
      Bauplan ersetzt — oder entfernt, wenn `expected` leer ist.
    * Verträge mit falscher Fraktion bekommen die gemeinte `factionGuid`.

    Gibt die Zahl der Änderungen zurück.
    """
    changed = 0
    if not merged or not fingerprint(issues):
        return 0
    for pool_guid, pool in (merged.get('blueprintPools') or {}).items():
        if not isinstance(pool, dict):
            continue
        rows = pool.get('blueprints') or []
        for row in rows:
            entry = blueprint_issue(issues, row.get('blueprintRecord'))
            expected = (entry or {}).get('expected')
            if not isinstance(expected, dict) or entry.get('slot'):
                continue
            before = {}
            if expected.get('name') and row.get('name') != expected['name']:
                before['name'] = row.get('name')
                row['name'] = expected['name']
            if expected.get('entityClass') and \
                    _guid(row.get('entityClass')) != _guid(expected['entityClass']):
                before['entityClass'] = row.get('entityClass')
                row['entityClass'] = expected['entityClass']
            if before:
                _mark(row, before, entry.get('note'))
                changed += 1
        pool_entry = pool_issue(issues, pool_guid)
        for dup in (pool_entry or {}).get('duplicateReferences') or []:
            changed += _fix_duplicate(pool, dup)
    for contract in ((merged.get('contracts') or [])
                     + (merged.get('legacyContracts') or [])):
        fix = faction_fix(issues, contract.get('debugName'))
        if not fix or _guid(contract.get('factionGuid')) == fix[0]:
            continue
        _mark(contract, {'factionGuid': contract.get('factionGuid')}, fix[2])
        contract['factionGuid'] = fix[0]
        changed += 1
    return changed


def _fix_duplicate(pool, dup):
    if not isinstance(dup, dict):
        return 0
    wanted = _guid(dup.get('blueprintRecord'))
    rows = pool.get('blueprints') or []
    places = [i for i, row in enumerate(rows)
              if _guid(row.get('blueprintRecord')) == wanted]
    if len(places) < 2:
        return 0
    second = places[1]
    expected = dup.get('expected')
    old = rows[second]
    if isinstance(expected, dict) and expected.get('blueprintRecord'):
        row = dict(old)
        row['blueprintRecord'] = expected['blueprintRecord']
        if expected.get('entityClass'):
            row['entityClass'] = expected['entityClass']
        if expected.get('name'):
            row['name'] = expected['name']
        _mark(row, {'blueprintRecord': old.get('blueprintRecord'),
                    'name': old.get('name')}, dup.get('note'))
        rows[second] = row
    else:
        del rows[second]
    return 1
