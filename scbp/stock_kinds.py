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
Welche Art ein Lagerposten ist — und das Kürzel Klasse/Größe/Grad an einem
Gegenstand. Gebraucht von `my_stock_page` und `orgstock_page`.

Alles ohne Netz: Gelesen wird, was schon auf der Platte liegt.

Material (`material_kind`), in dieser Reihenfolge:

1. `materialKind` des Basetools (aus dem gespeicherten Abgleich, falls da),
2. die Schreibweise (`(Ore)`, `(Raw)`, `Raw …` → Erz),
3. die eigenen Rohstoffdaten — Pflanzen, Stück-Mineralien aus den Rezepten,
   abbaubare Mineralien ohne Erz-Zusatz (= raffiniert),
4. die UEX-Warenliste (`stock_catalog`) und das Handelslager → Handelsware,
5. sonst Sonstiges.

Gegenstände (`item_kind`): Das Basetool liefert für Items keine Art. Gelesen
werden der eigene Katalog (Art und Klasse aus scmdb), die Rezeptdaten und
der UEX-Bereich der Warenliste.

⚠ Das Kürzel (`tag`) hängt am **Namen**: Das Basetool schickt keinen
Spielschlüssel. Gelesen wird der eigene Katalog, dessen Schlüssel die
Vergleichsform `paths.name_key` ist. Steht ein Name nicht darin, gibt es kein
Kürzel — lieber keines als ein geratenes.
"""
import os

from . import paths

# Arten bei Material
ORE = 'ore'
REFINED = 'refined'
MINERAL = 'mineral'
PLANT = 'plant'
COMMODITY = 'commodity'
OTHER = 'other'
MATERIAL_KINDS = (ORE, REFINED, MINERAL, PLANT, COMMODITY, OTHER)

# Arten bei Gegenständen — dieselben wie im Basetool
VEHICLE_ITEM = 'vehicle_item'
VEHICLE_WEAPON = 'vehicle_weapon'
WEAPON = 'weapon'
WEAPON_ATTACHMENT = 'weapon_attachment'
ARMOR = 'armor'
CLOTHING = 'clothing'
FOOD = 'food'
# Artefakte, Medaillen, Barren — im Basetool Sonstiges, hier eigens benannt.
COLLECTIBLE = 'collectible'
ITEM_KINDS = (VEHICLE_ITEM, VEHICLE_WEAPON, WEAPON, WEAPON_ATTACHMENT, ARMOR,
              CLOTHING, FOOD, COLLECTIBLE, OTHER)

# Oberkategorie aus `categories.classify` → Art
_FROM_GROUP = {
    'schiffswaffe': VEHICLE_WEAPON,
    'schiffsmodul': VEHICLE_ITEM,
    'schiffswerkzeug': VEHICLE_ITEM,
    'fpswaffe': WEAPON,
    'ruestung': ARMOR,
    'kleidung': CLOTHING,
}

# UEX-Bereich (`stock_catalog`) → Art
_FROM_SECTION = {
    'Systems': VEHICLE_ITEM,
    'Propulsion': VEHICLE_ITEM,
    'Avionics': VEHICLE_ITEM,
    'Utility': VEHICLE_ITEM,
    'Module': VEHICLE_ITEM,
    'Vehicle Weapons': VEHICLE_WEAPON,
    'Personal Weapons': WEAPON,
    'Armor': ARMOR,
    'Undersuits': ARMOR,
    'Clothing': CLOTHING,
    'Miscellaneous': COLLECTIBLE,
}

_GRADES = 'ABCD'


def label(kind):
    """Der Anzeigename einer Art."""
    from .language import t
    return t('s_ml_k_' + (kind or OTHER))


# ------------------------------------------------------------ Daten lesen
_catalog = {'stamp': None, 'entries': {}}


def _catalog_entries():
    """Die Einträge des eigenen Katalogs, nach Vergleichsform — einmal je
    Stand der Datei gelesen, auch die ohne bekannten Weg."""
    from . import catalog
    try:
        stamp = os.path.getmtime(paths.app_file(catalog.CACHE))
    except OSError:
        stamp = None
    if stamp != _catalog['stamp']:
        entries = {}
        if stamp is not None:
            try:
                entries = catalog.load(unreachable=True).get('bauplaene') or {}
            except Exception as exc:
                from . import errors
                errors.record('stock_kinds.catalog', exc)
        _catalog['stamp'], _catalog['entries'] = stamp, entries
    return _catalog['entries']


def server_kinds():
    """`{Name klein: materialKind}` aus dem letzten Lager-Abgleich mit dem
    Basetool — leer ohne Abgleich."""
    try:
        from . import basetool_sync
        state = basetool_sync.current_state() or {}
    except Exception:
        return {}
    found = {}
    for lot in ((state.get('stock') or {}).get('server') or {}).values():
        if not isinstance(lot, dict):
            continue
        name = ((lot.get('material') or {}).get('name') or '').strip().lower()
        kind = lot.get('materialKind')
        if name and isinstance(kind, dict):
            found.setdefault(name, kind)
    return found


_names = {'stand': None, 'plants': frozenset(), 'pieces': frozenset(),
          'minerals': frozenset()}


def _material_names():
    """Pflanzen, Stück-Mineralien und abbaubare Mineralien, jeweils in der
    Form von `crafting.norm_material`."""
    from . import crafting
    try:
        build = crafting.current_build()
    except Exception:
        build = None
    if _names['stand'] == build and _names['stand'] is not None:
        return _names
    plants, minerals = set(), set()
    try:
        from . import mining
        plants = {crafting.norm_material(p) for p in mining.plants()}
        plants |= {crafting.norm_material(g)
                   for g in crafting.plant_aliases().values()}
        minerals = {crafting.norm_material(e.get('name') or '')
                    for e in (mining.load().get('elemente') or {}).values()}
    except Exception as exc:
        from . import errors
        errors.record('stock_kinds.mining', exc)
    try:
        pieces = set(crafting.piece_materials())
    except Exception:
        pieces = set()
    _names.update(stand=build, plants=frozenset(plants - {''}),
                  pieces=frozenset(pieces - {''}),
                  minerals=frozenset(minerals - {''}))
    return _names


# --------------------------------------------------------------- Material
def _from_server(kind, key, names):
    item_type = (kind.get('type') or '').upper()
    if item_type == 'REFINED':
        return REFINED
    if item_type not in ('RAW', 'NO_REFINE'):
        return None
    # ⚠ Das Basetool setzt `harvestable` auch bei Mineralien aus dem Hand- und
    # Fahrzeugabbau (Dolivine, Aphorite …). Die eigenen Listen entscheiden
    # deshalb zuerst; `harvestable` gilt nur für Namen, die keine kennt.
    if key in names['plants']:
        return PLANT
    if key in names['pieces']:
        return MINERAL
    if key in names['minerals']:
        return ORE
    if kind.get('harvestable'):
        return PLANT
    if item_type == 'RAW':
        return ORE
    return COMMODITY if kind.get('commodity') else OTHER


def material_kind(name, server=None, for_sale=False):
    """Die Art eines Materials. `server` ist `server_kinds()` (einmal je
    Zeichnen geholt), `for_sale` sagt, dass der Posten im Handelslager
    liegt."""
    from . import crafting
    text = (name or '').strip()
    lower = text.lower()
    key = crafting.norm_material(text)
    names = _material_names()
    kind = (server or {}).get(lower)
    if isinstance(kind, dict):
        found = _from_server(kind, key, names)
        if found:
            return found
    if '(ore)' in lower or '(raw)' in lower or lower.startswith('raw '):
        return ORE
    if key in names['plants']:
        return PLANT
    if key in names['pieces']:
        return MINERAL
    if key in names['minerals']:
        return REFINED
    if for_sale:
        return COMMODITY
    try:
        from . import stock_catalog
        entry = stock_catalog.choices().get(lower)
    except Exception:
        entry = None
    if entry and entry.get('material') is not None:
        return COMMODITY
    return OTHER


# ----------------------------------------------------------- Gegenstände
def _from_recipe(name):
    from . import categories, crafting
    try:
        raw = crafting.recipe_raw(name) or {}
    except Exception:
        return None
    if not raw:
        return None
    group, sub = categories.classify(tag=raw.get('tag') or '',
                                     unterart=raw.get('subtype') or '',
                                     rezeptart=raw.get('type') or '')
    return _group_kind(group, sub)


def _group_kind(group, sub):
    if group == 'ausruestung' and sub == 'aufsatz':
        return WEAPON_ATTACHMENT
    return _FROM_GROUP.get(group)


def item_kind(name):
    """Die Art eines Gegenstands — `OTHER`, wenn keine Quelle ihn kennt."""
    from . import categories
    entry = _catalog_entries().get(paths.name_key((name or '').strip()))
    if entry:
        group, sub = categories.classify(art=entry.get('a') or '')
        found = _group_kind(group, sub)
        if found:
            return found
        if entry.get('c'):
            return VEHICLE_ITEM
    found = _from_recipe(name)
    if found:
        return found
    try:
        from . import stock_catalog
        uex_entry = stock_catalog.choices().get((name or '').strip().lower())
    except Exception:
        uex_entry = None
    if uex_entry and uex_entry.get('item') is not None:
        return _FROM_SECTION.get(uex_entry.get('section') or '', OTHER)
    return OTHER


# ---------------------------------------------------------------- Kürzel
def _grade_letter(value):
    if isinstance(value, int) and 1 <= value <= len(_GRADES):
        return _GRADES[value - 1]
    text = str(value or '').strip().upper()
    return text if text in tuple(_GRADES) else None


def _size_text(value):
    text = str(value or '').strip().upper().lstrip('S')
    return text if text.isdigit() and len(text) <= 2 else None


def tag(name):
    """Das Kürzel `(Mil/3/A)` eines Gegenstands — oder None."""
    from . import specs
    entry = _catalog_entries().get(paths.name_key((name or '').strip()))
    if not entry:
        return None
    return specs.tag_from_values(specs.class_short(entry.get('c')),
                                 _size_text(entry.get('s')),
                                 _grade_letter(entry.get('g')))
