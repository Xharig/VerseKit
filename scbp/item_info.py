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
Bauplan-Angaben unter der Beschreibung eines herstellbaren Gegenstands.

CIG schreibt in die Beschreibung schon Hersteller, Größe, Gütegrad und Klasse.
Was dort fehlt, weiß nur Verse-Kit: ob du den Bauplan hast, woher er kommt,
was das Herstellen braucht und wo es das Teil im Laden gibt::

    ---------------------------------------------------------
    VERSE-KIT: BAUPLAN & HERSTELLUNG
    Bauplan: [  ] fehlt — aus "Wartungsauftrag", ab Rang Junior (+3 weitere)
    Herstellen: 3 Zutaten · 4 min
    Im Laden: New Babbage · 12.400 aUEC

⚠ **Der Block braucht immer ein Kästchen** (`[x]` oder `[  ]`). Daran erkennt
`injection._has_box` einen Block als unseren; ohne Kästchen gälte er beim
Aufräumen ohne Merkdatei als fremd und bliebe stehen. Deshalb gibt es für
Gegenstände, deren Besitz unklar ist (gleicher Name, verschiedene Teile),
gar keinen Block.

⚠ **Keine Feldnamen wie `Größe:` oder `Klasse:` im Block.** `specs.build_table`
liest die Beschreibungen mitsamt unserem Block und nimmt den ersten Treffer.

Zuordnung Bauplan → Beschreibungsschlüssel: zuerst über den Tag
(`BP_CRAFT_COOL_JSPN_S01_FrostStar_SCItem` → `item_DescCOOL_JSPN_S01_FrostStar`),
dann über den englischen Originalnamen (`gametext.saved_names`). Beides ohne
Netz.
"""
import re

from . import paths

# Name der Einstellung. Standard **an**, wie die übrigen Angaben im Spiel.
SETTING = 'angaben_in_beschreibung'

TAG_PREFIX = 'bp_craft_'

# Platzhalter in Auftragstiteln (`~mission(Location)`) — im Block sinnlos.
_PLACEHOLDER = re.compile(r'~\w+\([^)]*\)')


def enabled():
    return paths.setting_bool(SETTING, True)


def _loose(stem):
    """Wie `specs._loose_stem`: ohne führenden Unterstrich, ohne `_scitem`."""
    stem = stem.lstrip('_')
    if stem.endswith('_scitem'):
        stem = stem[:-len('_scitem')]
    return stem


def desc_keys(lines):
    """{loser Stamm: [Beschreibungsschlüssel in Originalschreibweise]} und
    {Namensschlüssel: loser Stamm}.

    Ein Gegenstand kann zwei Beschreibungen haben (eine davon veraltet) —
    beide bekommen den Block, denn welche das Spiel zeigt, steht nirgends."""
    descs, names = {}, {}
    for line in lines:
        sep = line.find('=')
        if sep < 1:
            continue
        key = line[:sep].split(',', 1)[0]
        lower = key.lower()
        if lower.startswith('item_desc'):
            descs.setdefault(_loose(lower[9:]), []).append(key)
        elif lower.startswith('item_name'):
            names[key] = _loose(lower[9:])
    return descs, names


def _tag_stems(tag):
    """Mögliche lose Stämme aus einem Bauplan-Tag."""
    lower = (tag or '').lower()
    if not lower.startswith(TAG_PREFIX):
        return []
    stem = _loose(lower[len(TAG_PREFIX):])
    result = [stem]
    if stem.endswith('_temp'):
        result.append(stem[:-len('_temp')])
    return result


def _name_index(names, original_names):
    """{normalisierter englischer Name: loser Stamm} — nur eindeutige."""
    from .catalog import _norm
    found, double = {}, set()
    for key, stem in names.items():
        name = (original_names or {}).get(key)
        if not name:
            continue
        k = _norm(name)
        if k in found and found[k] != stem:
            double.add(k)
        found[k] = stem
    for k in double:
        found.pop(k, None)
    return found


def _time_text(seconds):
    seconds = int(seconds or 0)
    if seconds <= 0:
        return ''
    if seconds < 60:
        return '%d s' % seconds
    minutes = (seconds + 30) // 60
    if minutes < 60:
        return '%d min' % minutes
    return '%d h %d min' % (minutes // 60, minutes % 60)


def _clean_title(text):
    text = _PLACEHOLDER.sub('', text or '')
    return re.sub(r'\s{2,}', ' ', text).strip(' -:,')


def origins(missions):
    """{normalisierter Bauplanname: [(rep, -uec, titel_key, rang)]}, der
    leichteste Weg zuerst — dieselbe Regel wie „Woher?"."""
    from .catalog import _norm
    result = {}
    for entry in (missions or {}).values():
        key = entry.get('titel_key')
        if not key:
            continue
        per_bp = entry.get('ab') or {}
        for name in entry.get('bp') or []:
            cond = per_bp.get(name) or {}
            rep = cond.get('rep', entry.get('rep'))
            rank = cond.get('rang', entry.get('rang'))
            result.setdefault(_norm(name), []).append(
                (rep if isinstance(rep, (int, float)) else 10 ** 9,
                 -(entry.get('uec') or 0), key, rank or ''))
    for routes in result.values():
        routes.sort(key=lambda r: (r[0], r[1]))
    return result


def _blueprint_line(have, routes, words, title_of, box_have, box_missing):
    if have:
        return '%s: %s %s' % (words['it_bp'], box_have, words['it_hast'])
    line = '%s: %s %s' % (words['it_bp'], box_missing, words['it_fehlt'])
    # Der leichteste Weg **mit lesbarem Titel** — ein Titel nur aus
    # Platzhaltern sagt im Spiel nichts. Gezählt wird nach Wortlaut: Zwei
    # Schlüssel mit demselben Titel sind für den Spieler ein Auftrag.
    named = [(r, _clean_title(title_of(r[2]))) for r in routes or ()]
    named = [(r, t_) for r, t_ in named if t_]
    if not named:
        return '%s — %s' % (line, words['it_kein_auftrag'])
    first, title = named[0]
    part = ' — %s "%s"' % (words['it_aus'], title)
    if first[3]:
        part += ', %s %s' % (words['it_ab_rang'], first[3])
    others = len({t_ for _r, t_ in named}) - 1
    if others > 0:
        part += ' (+%d %s)' % (others, words['it_weitere'])
    return line + part


def _craft_line(blueprint, words):
    tiers = (blueprint or {}).get('tiers') or []
    if not tiers:
        return ''
    tier = tiers[0]
    count = len(tier.get('slots') or [])
    if not count:
        return ''
    noun = words['it_zutat'] if count == 1 else words['it_zutaten']
    parts = ['%d %s' % (count, noun)]
    span = _time_text(tier.get('craftTimeSeconds'))
    if span:
        parts.append(span)
    return '%s: %s' % (words['it_herstellen'], ' · '.join(parts))


def _shop_line(item, shop_data, words, digits):
    """Günstigster Laden — über die Kennung, sonst über den Namen.

    ⚠ „nicht erhältlich" nur, wenn UEX das Teil kennt (Kennung oder Name)
    und **keiner** der beiden Wege einen Preis hat. UEX führt manche Teile
    unter einer anderen Kennung als die Rezeptdaten; nur über die Kennung
    gesucht, stünde dort fälschlich „nur Herstellen"."""
    entity = item.get('entity') or ''
    name = (item.get('basis') or '').lower()
    hit = (shop_data.get('kennung') or {}).get(entity) \
        or (shop_data.get('name') or {}).get(name)
    if hit:
        price, place = hit
        return '%s: %s · %s aUEC' % (words['it_laden'], place, digits(price))
    if entity in (shop_data.get('kennungen') or ()) \
            or name in (shop_data.get('namen') or ()):
        return '%s: %s' % (words['it_laden'], words['it_nicht_laden'])
    # UEX kennt das Teil gar nicht — lieber nichts sagen als raten.
    return ''


def _names_in(lines, names):
    """{Namensschlüssel: Name ohne Kürzel} aus den Zeilen selbst."""
    from .specs import strip_tag
    found = {}
    for line in lines:
        sep = line.find('=')
        if sep < 1:
            continue
        key = line[:sep].split(',', 1)[0]
        if key in names:
            found[key] = strip_tag(line[sep + 1:]).strip()
    return found


def build(lines, missions, owned, words, title_of, line, heading_markup,
          box_have, box_missing, digits, english=False, crafting_data=None,
          shop_data=None, original_names=None):
    """{Beschreibungsschlüssel: Block} für alle herstellbaren Gegenstände.

    `title_of(titel_key)` liefert den Auftragstitel aus derselben Datei,
    ohne unsere Marke. `crafting_data`, `shop_data` (Preise, bekannte Namen)
    und `original_names` sind für die Prüfung da; sonst kommen sie aus den
    Ablagen — ohne Netz."""
    from . import crafting
    from .catalog import _norm
    descs, names = desc_keys(lines)
    if not descs:
        return {}
    if crafting_data is None:
        crafting_data = crafting.load()
    by_tag = {}
    for b in crafting_data.get('blueprints') or []:
        if b.get('tag'):
            by_tag[b['tag']] = b
    if shop_data is None:
        from . import shops
        shop_data = shops.cheapest_by_entity()
    if original_names is None:
        from . import gametext
        try:
            original_names = gametext.saved_names() or {}
        except Exception:
            original_names = {}
    # Eine englische Datei trägt die Originalnamen selbst — dort braucht es
    # die abgelegten nicht (die fehlen bei englischen Clients oft ganz).
    if english:
        # Die abgelegten gehen vor: Sie stammen unverändert aus der Data.p4k.
        merged = _names_in(lines, names)
        merged.update(original_names)
        original_names = merged
    by_name = _name_index(names, original_names)
    routes_of = origins(missions)

    items = crafting.with_collection(owned)
    result = {}
    for item in items:
        if item.get('habe') is None:
            continue
        stem = next((s for t_ in item.get('tags') or []
                     for s in _tag_stems(t_) if s in descs), None)
        if stem is None:
            stem = by_name.get(_norm(item.get('basis') or ''))
        if stem is None or stem not in descs:
            continue
        rows = [_blueprint_line(item['habe'],
                                routes_of.get(_norm(item.get('basis') or '')),
                                words, title_of, box_have, box_missing)]
        craft = _craft_line(by_tag.get(item.get('tag')), words)
        if craft:
            rows.append(craft)
        shop = _shop_line(item, shop_data, words, digits)
        if shop:
            rows.append(shop)
        block = '\\n' + '\\n'.join(
            ['', line, '', heading_markup % words['it_kopf']] + rows)
        for key in descs[stem]:
            result[key] = block
    return result
