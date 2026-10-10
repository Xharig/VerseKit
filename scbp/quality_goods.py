# -*- coding: utf-8 -*-
#
# SC BP Watcher — zeigt live neue Star-Citizen-Bauplaene an.
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
Waren mit Güte — aus der Spieldatenbank der eigenen Installation gelesen.

Jede Ware, die im Spiel eine Güte trägt, hat in `Game2.dcb` einen Datensatz
vom Typ `CraftingQualityQuantizationRecord` (`Quantization_<Name>`). Daraus
entsteht die Liste; den lesbaren Namen liefert die englische `global.ini`.

Zugeordnet wird in dieser Reihenfolge, und nur eindeutig:

1. ein `ResourceType`, der auf die Stufung verweist,
2. ein `ResourceType` mit demselben Namen (ohne `Ore_`/`Raw`, ohne `_`),
3. ein Text `items_commodities_…`, der alle Wortteile des Namens enthält
   und als einziger passt.

Was sich so nicht benennen lässt, bleibt draußen — geraten wird nicht.

Einheit: Was raffiniert wird (`ResourceType` mit `refinedVersion`), zählt in
SCU — Erze und Rohmineralien. Alles andere in Stück: Edelsteine, Tierteile,
Pflanzen, und auch Saldynium und Jaclium, die zwar `Ore_` heißen, aber keine
raffinierte Fassung haben.

Gelesen wird einmal je Spielstand (Größe und Zeit der `Data.p4k`), im
Hintergrund; das Ergebnis liegt als `waren-guete.json` im Datenordner.
"""
import json
import os
import re
import threading

from . import paths

FILE = 'waren-guete.json'
QUANT_TYPE = 'CraftingQualityQuantizationRecord'
QUANT_PREFIX = 'Quantization_'
NULL_REF = 'REF:00000000-0000-0000-0000-000000000000'
# Ändert sich die Auswertung, wird mit neuer Nummer auch ohne Patch neu gelesen.
FORMAT = 2
# Weniger Treffer heißt: Datenbank nicht verstanden — dann nichts ablegen.
MINIMUM = 20

_CACHE = {'mtime': None, 'daten': None}
_LOCK = threading.Lock()


def _flat(name):
    """`Ore_Iron`, `Raw_Ice`, `RawSilicon` → `iron`, `ice`, `silicon`."""
    s = (name or '').lower().replace('_', '')
    for prefix in ('ore', 'raw'):
        if s.startswith(prefix):
            s = s[len(prefix):]
    return s


def _words(stem):
    """`QuasiTongue` → `['quasi', 'tongue']`."""
    return [w.lower() for w in re.findall(r'[A-Z]+(?=[A-Z][a-z]|$)|[A-Z]?[a-z]+|\d+',
                                          stem)]


def parse_ini(raw):
    """`global.ini` → {schlüssel in Kleinschrift: text}."""
    texts = {}
    for line in raw.decode('utf-8-sig', 'ignore').splitlines():
        key, sep, value = line.partition('=')
        if sep:
            texts.setdefault(key.split(',', 1)[0].strip().lower(), value.strip())
    return texts


def extract(db, texts):
    """[(name, 'stueck'|'scu')] aus einer gelesenen Datenbank.

    `db` ist ein `datacore.DataCore`, `texts` das Ergebnis von `parse_ini`."""
    quant, resource_types = {}, []
    for name, _file, si, guid, ii in db.records():
        kind = db.type_name(si)
        if kind == QUANT_TYPE:
            quant[guid] = name.split('.', 1)[-1][len(QUANT_PREFIX):]
        elif kind == 'ResourceType':
            resource_types.append((name.split('.', 1)[-1], si, ii))
    links = []
    for rt_name, si, ii in resource_types:
        data = db.read(si, ii, 3)
        refs = set(re.findall(r'REF:([0-9a-f-]{36})',
                              json.dumps(data, default=str)))
        key = str(data.get('displayName') or '').lstrip('@').lower()
        refined = data.get('refinedVersion') not in (None, '', NULL_REF)
        links.append((rt_name, key, {quant[g] for g in refs if g in quant},
                      refined))
    commodity_keys = [k for k in texts if k.startswith('items_commodities_')
                      and not k.endswith('_desc')]
    result = []
    for stem in sorted(set(quant.values())):
        if stem.upper() == 'TEMPLATE':
            continue
        matches = [lk for lk in links if stem in lk[2]]
        if not matches:
            matches = [lk for lk in links if _flat(lk[0]) == _flat(stem)]
        name = None
        for _rt, key, _q, _r in matches:
            if texts.get(key) and not texts[key].startswith('@'):
                name = texts[key]
                break
        if name is None:
            direct = texts.get('items_commodities_' + stem.lower())
            if direct:
                name = direct
        if name is None:
            words = _words(stem)
            hits = [k for k in commodity_keys if words
                    and all(w in k for w in words)]
            if len(hits) == 1:
                name = texts[hits[0]]
        if not name:
            continue
        scu = any(refined for _rt, _k, _q, refined in matches)
        result.append((name, 'scu' if scu else 'stueck'))
    return sorted(set(result), key=lambda e: e[0].lower())


def _path():
    return paths.app_file(FILE)


def load():
    """{'stand': …, 'waren': [[name, einheit], …]} — leer ohne Datei."""
    with _LOCK:
        try:
            mtime = os.path.getmtime(_path())
        except OSError:
            return {}
        if _CACHE['mtime'] == mtime and _CACHE['daten'] is not None:
            return _CACHE['daten']
        try:
            with open(_path(), encoding='utf-8') as f:
                data = json.load(f)
        except (OSError, ValueError):
            data = {}
        if not isinstance(data, dict):
            data = {}
        _CACHE['mtime'], _CACHE['daten'] = mtime, data
        return data


def goods():
    """[(name, einheit)] — leer, solange nichts gelesen wurde."""
    out = []
    for entry in load().get('waren') or []:
        if isinstance(entry, (list, tuple)) and len(entry) == 2:
            out.append((str(entry[0]), str(entry[1])))
    return out


def unit(name):
    """'stueck', 'scu' oder None, wenn die Ware hier nicht steht."""
    from .crafting import norm_material
    data = load()
    if _CACHE.get('einheiten_von') is not data:
        _CACHE['einheiten'] = {norm_material(n): u for n, u in goods()}
        _CACHE['einheiten_von'] = data
    return _CACHE['einheiten'].get(norm_material(name))


def save(stamp, entries):
    """Die Liste mit ihrem Spielstand atomar ablegen."""
    path = _path()
    with open(path + '.tmp', 'w', encoding='utf-8', newline='\n') as f:
        json.dump({'format': FORMAT, 'stand': stamp,
                   'waren': [list(e) for e in entries]}, f,
                  ensure_ascii=False, indent=1)
    os.replace(path + '.tmp', path)


def refresh(spielordner=None):
    """Nach einem Spiel-Patch neu lesen. True, wenn neu abgelegt wurde.

    Liest nur, wenn sich der Stempel der `Data.p4k` oder das `FORMAT`
    geändert hat. Wirft nie."""
    try:
        from . import datacore, gametext
        stamp = gametext.archive_stamp(spielordner)
        current = load()
        if not stamp or (current.get('stand') == stamp
                         and current.get('format') == FORMAT):
            return False
        raw = gametext.read_archive_file(datacore.ARCHIVE_PATH, spielordner)
        if not raw:
            return False
        ini, _meldung = gametext.read_from_archive('english', spielordner)
        if not ini:
            return False
        entries = extract(datacore.DataCore(raw), parse_ini(ini))
        if len(entries) < MINIMUM:
            from . import errors
            errors.trail('Waren mit Güte: nur %d erkannt, nicht abgelegt'
                         % len(entries))
            return False
        save(stamp, entries)
        return True
    except Exception as exc:
        from . import errors
        errors.record('quality_goods.refresh', exc)
        return False


def start():
    """Im Hintergrund einmal nachsehen, ob ein Patch neu zu lesen ist."""
    threading.Thread(target=refresh, daemon=True, name='waren-guete').start()
