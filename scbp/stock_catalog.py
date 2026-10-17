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
Die Warenliste zum Einbuchen ins Lager der Einheit (`orgstock_page`).

Das KRT Profit Basetool führt als **ITEM** jeden Gegenstand aus den
spielbezogenen UEX-Warengruppen der Art `item` und als **MATERIAL** jede
UEX-Ware (`commodities`) — unter genau dem Namen, den UEX liefert. Diese Liste
holt dieselben Quellen und merkt sich je Name die UEX-Nummer. Mit ihr fragt
die Seite `catalog/resolve` über `uexId`, nicht über einen geratenen Namen.

| Abruf | Anzahl |
|---|---|
| `categories` | 1 |
| `items?id_category=…` | je spielbezogene Item-Warengruppe einer (rund 50) |
| `commodities` | 1 |

⚠ Die Liste hängt am Patch (`patch_bound`), nicht an der Uhr: Neue Teile
kommen mit einem Patch, und der Aufbau dauert rund eine halbe Minute.

⚠ Ein Name kann zugleich Gegenstand und Ware sein (`Golden Medmon`). Dann
stehen beide Nummern am Eintrag; welche gilt, entscheidet die Mengeneinheit
des Formulars (`reference`).
"""
import threading

from . import uex

SOURCE_CATEGORIES = 'https://api.uexcorp.uk/2.0/categories'
SOURCE_ITEMS = 'https://api.uexcorp.uk/2.0/items?id_category=%d'
SOURCE_COMMODITIES = 'https://api.uexcorp.uk/2.0/commodities'
CACHE = 'lager-katalog.json'
FORMAT = 1

ITEM = 'ITEM'
MATERIAL = 'MATERIAL'

# UEX-Bereich -> Sprachschlüssel. Ein Bereich, der hier fehlt, bleibt englisch
# stehen.
SECTION_KEYS = {
    'Armor': 's_uk_armor',
    'Avionics': 's_uk_avionics',
    'Personal Weapons': 's_uk_personal_weapons_s',
    'Propulsion': 's_uk_propulsion',
    'Systems': 's_uk_systems',
    'Undersuits': 's_uk_undersuits_s',
    'Utility': 's_uk_utility',
    'Vehicle Weapons': 's_uk_vehicle_weapons_s',
    'Clothing': 's_sl_k_clothing',
    'Miscellaneous': 's_sl_k_miscellaneous',
    'Liveries': 's_sl_k_liveries',
    'Consumable': 's_sl_k_consumable',
    'Technology': 's_sl_k_technology',
    'Module': 's_sl_k_module',
    'Decorations': 's_sl_k_decorations',
    'Flair': 's_sl_k_flair',
}
COMMODITY_KEY = 's_sl_k_ware'

_store = uex.Store(CACHE, format_no=FORMAT, shelf_life=90 * uex.DAY,
                   patch_bound=True)
_LOCK = threading.Lock()
_derived = {'data': None, 'choices': {}}


def due():
    """Muss die Liste (neu) geholt werden? Liest nur die Ablage."""
    return _store.stale()


def update():
    """Die Liste bei UEX holen, wenn sie fällig ist. `False`, wenn ein Abruf
    scheitert — dann bleibt der alte Stand liegen."""
    with _LOCK:
        if not _store.stale():
            return True
        categories = uex.fetch(SOURCE_CATEGORIES, 'stock_catalog.categories')
        if not categories:
            return False
        chosen = [c for c in categories if isinstance(c, dict)
                  and str(c.get('is_game_related')) == '1'
                  and (c.get('type') or '').lower() == 'item' and c.get('id')]
        items = []
        for category in chosen:
            rows = uex.fetch(SOURCE_ITEMS % int(category['id']),
                             'stock_catalog.items')
            # ⚠ Eine fehlende Warengruppe ließe ihre Teile still aus der Liste
            # fallen — dann lieber den alten Stand behalten.
            if rows is None:
                return False
            section = (category.get('section') or '').strip()
            for row in rows:
                if not isinstance(row, dict):
                    continue
                name = (row.get('name') or '').strip()
                number = _number(row.get('id'))
                if name and number:
                    items.append([name, number, section])
        goods = uex.fetch(SOURCE_COMMODITIES, 'stock_catalog.commodities')
        if not goods:
            return False
        commodities = []
        for row in goods:
            if not isinstance(row, dict):
                continue
            name = (row.get('name') or '').strip()
            number = _number(row.get('id'))
            if name and number:
                commodities.append([name, number])
        if not items and not commodities:
            return False
        return _store.save({'teile': items, 'waren': commodities},
                           compact=True)


def _number(value):
    try:
        number = int(value)
    except (TypeError, ValueError):
        return 0
    return number if number > 0 else 0


def choices():
    """Die Liste aus der Ablage, ohne Netz::

        {Name klein: {'name': Anzeigename, 'item': UEX-Nummer oder None,
                      'material': UEX-Nummer oder None, 'section': Bereich}}

    Leer, solange nichts geholt ist. Führt UEX einen Namen mehrfach als
    Gegenstand, gilt der erste."""
    data = _store.load() or {}
    if _derived['data'] is data:
        return _derived['choices']
    found = {}
    for row in data.get('teile') or ():
        if not (isinstance(row, list) and len(row) == 3):
            continue
        name, number, section = row
        entry = found.setdefault(name.lower(), {
            'name': name, 'item': None, 'material': None, 'section': section})
        if entry['item'] is None:
            entry['item'] = number
            entry['section'] = section
    for row in data.get('waren') or ():
        if not (isinstance(row, list) and len(row) == 2):
            continue
        name, number = row
        entry = found.setdefault(name.lower(), {
            'name': name, 'item': None, 'material': None, 'section': ''})
        if entry['material'] is None:
            entry['material'] = number
    _derived['data'] = data
    _derived['choices'] = found
    return found


def label(entry):
    """Was rechts neben dem Namen in der Auswahl steht: Ware, Bereich oder
    beides."""
    from .language import t
    parts = []
    if entry.get('material') is not None:
        parts.append(t(COMMODITY_KEY))
    if entry.get('item') is not None:
        section = entry.get('section') or ''
        key = SECTION_KEYS.get(section)
        parts.append(t(key) if key else section)
    return ' · '.join(p for p in parts if p)


def reference(name, piece):
    """`(kind, item-ref)` für `catalog/resolve` — oder `None`, wenn der Name
    nicht in der Liste steht. Steht er dort als Gegenstand und als Ware,
    entscheidet `piece` (Stück -> Gegenstand, SCU -> Ware)."""
    entry = choices().get((name or '').strip().lower())
    if entry is None:
        return None
    use_item = entry['item'] is not None and (entry['material'] is None
                                              or piece)
    number = entry['item'] if use_item else entry['material']
    return (ITEM if use_item else MATERIAL,
            {'uexId': number, 'name': entry['name'][:200]})
