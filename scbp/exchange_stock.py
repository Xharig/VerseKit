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
Lager-Abgleich mit dem KRT Profit Basetool — der Teil ohne Netz.

Das Basetool führt das persönliche Lager als **Posten**: ein Material oder ein
Item an einem Ort, mit Qualität und „gestohlen", und einer Menge
(`resources/stock.md`). VerseKit hat zwei Lager, die beide dazu passen:

| VerseKit | Posten beim Basetool |
|---|---|
| Rohstofflager (`materials`, `rohstoffe.json`): Material, Menge in SCU oder Stück, Qualität, Ort | Material mit seiner Qualität |
| Handelslager (`trade_cargo`, `handelslager.json`): Ware oder Item, Menge, Ort, gestohlen | Material unter Qualität 0, Item in ganzen Stück (`PIECE`) |

**Material oder Item** sagt der Posten selbst: `kind` (`MATERIAL`/`ITEM`),
fehlt das Feld, ist ein Posten ohne `materialKind` ein Item. Ein Material
behält immer die Qualität, unter der es gebucht ist; ein Item steht immer
unter Qualität 0. `materialKind.commodity` zeigt nur an, dass UEX das
Material als Ware führt — das gilt auch für Erze, Metalle und Edelsteine und
entscheidet hier nichts.

**Die Regeln** (Sync-Anleitung und `client-security.md`):

1. **Erster Abgleich fügt nur hinzu.** Was hier fehlt, kommt herein; was dort
   fehlt, geht hinaus (erwartete Menge 0). Steht ein Posten auf **beiden**
   Seiten mit verschiedener Menge, entscheidet der Spieler — nie VerseKit.
2. **Danach zählt der Vergleich mit dem letzten Stand** (`baseline`): Hat sich
   nur hier etwas geändert, geht es hinaus — mit der Menge, die zuletzt dort
   stand, als `expectedQuantity`. Hat es sich nur dort geändert, wird es hier
   übernommen. Haben sich beide Seiten geändert: wieder der Spieler.
3. **`VERSION_CONFLICT` wird durch Holen gelöst**, nie durch blindes
   Wiederholen — der nächste Durchgang holt und vergleicht neu.
4. **Nur Orte aus `catalog/locations`.** Ein Ort, den das Basetool nicht
   kennt, bleibt hier und wird angezeigt; er wird nie angelegt.
5. SCU mit höchstens drei Nachkommastellen, Stück ganzzahlig.

Ein Posten heißt hier über seine **Kennung** `material|ort|qualität|gestohlen`
(alles klein, Material über den `bt`-Schlüssel des Basetools). Mehrere
Einträge eines VerseKit-Lagers mit derselben Kennung werden zusammengezählt —
genau so bucht das Basetool sie auch.
"""
SCU = 'SCU'
PIECE = 'PIECE'


def _num(amount, unit):
    if unit == PIECE:
        return int(round(float(amount or 0)))
    return round(float(amount or 0), 3)


def identity(material_key, location, quality, stolen):
    return '%s|%s|%d|%d' % ((material_key or '').lower(),
                            (location or '').strip().lower(),
                            int(quality or 0), 1 if stolen else 0)


def is_item(lot):
    """Ist dieser Posten des Basetools ein Item (kein Material)?"""
    kind = str(lot.get('kind') or '').upper()
    if kind in ('MATERIAL', 'ITEM'):
        return kind == 'ITEM'
    return not isinstance(lot.get('materialKind'), dict)


def server_lots(items):
    """Posten des Basetools -> {Kennung: Posten}. Mehrere Posten mit derselben
    Kennung (sollte es nicht geben) werden zusammengezählt."""
    out = {}
    for lot in items:
        material = lot.get('material') or {}
        location = lot.get('location') or {}
        key = identity(material.get('bt') or material.get('name'),
                       location.get('name'), lot.get('quality'),
                       lot.get('stolen'))
        quantity = lot.get('quantity') or {}
        unit = quantity.get('unit') or SCU
        if key in out:
            out[key]['amount'] = _num(out[key]['amount']
                                      + float(quantity.get('amount') or 0),
                                      unit)
            continue
        out[key] = {'key': lot.get('key'), 'material': material,
                    'location': location,
                    'quality': int(lot.get('quality') or 0),
                    'stolen': bool(lot.get('stolen')),
                    'amount': _num(quantity.get('amount'), unit),
                    'unit': unit, 'item': is_item(lot)}
    return out


def item_flags(items, known=None):
    """{bt: True/False} — welche `bt` das Basetool als Item führt.

    `known` ist das Gelernte vom letzten Mal, damit es nicht mit dem letzten
    Posten verschwindet."""
    out = dict(known or {})
    for lot in items:
        bt = (lot.get('material') or {}).get('bt')
        if bt:
            out[bt] = is_item(lot)
    return out


def local_lots(raw, trade, resolve, places, is_piece, items=None):
    """Beide VerseKit-Lager -> ({Kennung: Posten}, {Grund: [Name]}).

    `resolve(name)` -> `bt` oder None, `places` -> {ort klein: location-ref}
    (aus `catalog/locations`), `is_piece(name)` -> zählt in Stück.
    `items` -> {bt: True} für Items (siehe `item_flags`): Sie stehen unter
    Qualität 0 und zählen in ganzen Stück, egal in welchem Lager.
    Nicht Zuordenbares landet im zweiten Wert — `material` (unbekanntes
    Material), `location` (Ort, den das Basetool nicht führt, oder leer)."""
    out, skipped = {}, {'material': [], 'location': []}
    items = items or {}

    def put(store, name, amount, location, quality, stolen, index):
        bt = resolve(name)
        if not bt:
            skipped['material'].append(name)
            return
        place = places.get((location or '').strip().lower())
        if not place:
            skipped['location'].append(location or '—')
            return
        if items.get(bt):
            unit, quality = PIECE, 0
        else:
            unit = PIECE if (store == 'raw' and is_piece(name)) else SCU
        key = identity(bt, place['name'], quality, stolen)
        entry = out.setdefault(key, {
            'material': {'bt': bt, 'name': name[:200]}, 'location': place,
            'quality': int(quality or 0), 'stolen': bool(stolen),
            'amount': 0, 'unit': unit, 'store': store, 'rows': []})
        entry['amount'] = _num(entry['amount'] + float(amount or 0), unit)
        entry['rows'].append(index)

    for index, row in enumerate(raw or ()):
        quality = row.get('qualitaet')
        put('raw', row.get('material') or '', row.get('menge'),
            row.get('ort'), int(round(quality)) if quality is not None else 0,
            row.get('gestohlen'), index)
    for index, row in enumerate(trade or ()):
        put('trade', row.get('ware') or '', row.get('menge'), row.get('ort'),
            0, row.get('gestohlen'), index)
    return out, skipped


def relocate(raw, trade, resolve, places, server, items):
    """Falsch einsortierte Zeilen ins richtige Lager ziehen — nur hier, ohne
    eine Sendung ans Basetool. Rückgabe: (raw, trade, Anzahl umgezogen).

    * Ein Item im Rohstofflager kommt ins Handelslager. Seine Kennung bleibt
      dieselbe (Qualität 0).
    * Ein Material im Handelslager kommt ins Rohstofflager, wenn sein Posten
      unter Qualität 0 dort nicht steht, aber **genau ein** Posten desselben
      Materials am selben Ort, gleich gestohlen und mit derselben Menge, eine
      Qualität trägt — die bekommt die Zeile. Ohne eindeutigen Treffer bleibt
      sie, wo sie ist.

    `server` -> `server_lots(...)`, `items` -> `item_flags(...)`."""
    items = items or {}
    keep_raw, new_trade, moved = [], [], 0
    for row in raw or ():
        bt = resolve(row.get('material') or '')
        if bt and items.get(bt):
            new_trade.append({'ware': row.get('material') or '',
                              'menge': _num(row.get('menge'), PIECE),
                              'ort': row.get('ort') or '',
                              'gestohlen': bool(row.get('gestohlen')),
                              'stueck': True})
            moved += 1
        else:
            keep_raw.append(row)

    taken = set()
    for row in keep_raw:
        bt = resolve(row.get('material') or '')
        place = places.get((row.get('ort') or '').strip().lower())
        if bt and place:
            quality = row.get('qualitaet')
            taken.add(identity(bt, place['name'],
                               int(round(quality)) if quality is not None
                               else 0, row.get('gestohlen')))

    groups = {}
    for index, row in enumerate(trade or ()):
        bt = resolve(row.get('ware') or '')
        place = places.get((row.get('ort') or '').strip().lower())
        if not bt or not place or items.get(bt, True):
            continue
        stolen = bool(row.get('gestohlen'))
        group = groups.setdefault(identity(bt, place['name'], 0, stolen),
                                  {'bt': bt.lower(), 'place': place['name'],
                                   'stolen': stolen, 'rows': []})
        group['rows'].append(index)

    drop, new_raw = set(), []
    for key, group in groups.items():
        if key in server:
            continue
        first = trade[group['rows'][0]]
        total = sum(float(trade[i].get('menge') or 0) for i in group['rows'])
        matches = [
            lot for lot_key, lot in server.items()
            if not lot['item'] and lot['quality'] and lot_key not in taken
            and ((lot['material'] or {}).get('bt') or '').lower()
            == group['bt']
            and ((lot['location'] or {}).get('name') or '').strip().lower()
            == group['place'].strip().lower()
            and lot['stolen'] == group['stolen']
            and _num(total, lot['unit']) == lot['amount']]
        if len(matches) != 1:
            continue
        lot = matches[0]
        row = {'material': first.get('ware') or '',
               'menge': _num(total, lot['unit']),
               'qualitaet': lot['quality'], 'ort': first.get('ort') or ''}
        if group['stolen']:
            row['gestohlen'] = True
        new_raw.append(row)
        drop.update(group['rows'])
        moved += len(group['rows'])

    trade_out = [r for i, r in enumerate(trade or ()) if i not in drop]
    return keep_raw + new_raw, trade_out + new_trade, moved


def plan(local, server, baseline, decisions=None, open_conflicts=()):
    """Was zu tun ist — ohne etwas zu tun.

    `baseline` ist {Kennung: Menge} vom letzten Abgleich, None beim ersten.
    `decisions`: {Kennung: 'mine' | 'theirs'} — was der Spieler bei einem
    Konflikt entschieden hat. `open_conflicts`: Konflikte vom letzten Mal, die
    noch niemand entschieden hat — sie bleiben Konflikte, bis der Spieler
    entscheidet oder beide Seiten wieder übereinstimmen. Rückgabe::

        {'push': [(Kennung, Menge, erwartet)],
         'take': [(Kennung, Menge)],          # hier übernehmen (0 = austragen)
         'conflicts': {Kennung: (hier, dort)}}
    """
    decisions = decisions or {}
    first = baseline is None
    base = baseline or {}
    push, take, conflicts = [], [], {}
    for key in sorted(set(local) | set(server)):
        here = local[key]['amount'] if key in local else 0
        there = server[key]['amount'] if key in server else 0
        if here == there:
            continue
        decision = decisions.get(key)
        if decision == 'mine':
            push.append((key, here, there))
            continue
        if decision == 'theirs':
            take.append((key, there))
            continue
        if key in open_conflicts:
            conflicts[key] = (here, there)
            continue
        if first:
            if there == 0:
                push.append((key, here, 0))
            elif here == 0:
                take.append((key, there))
            else:
                conflicts[key] = (here, there)
            continue
        before = base.get(key, 0)
        if there == before:
            push.append((key, here, before))
        elif here == before:
            take.append((key, there))
        else:
            conflicts[key] = (here, there)
    return {'push': push, 'take': take, 'conflicts': conflicts}


def change_sets(plan_result, local, server, batch=500, overrides=()):
    """Die Sendungen: [(change_set, Kennungen)] — `set-quantity` je Posten.

    `overrides`: Kennungen, bei denen der Spieler „meine Menge" gewählt hat —
    sie gehen mit `override`, weil ein dort geleerter Posten sonst als
    `REMOVED_ELSEWHERE` abgelehnt würde.

    ⚠ Ein Posten, der hier gar nicht mehr steht, wird mit den Angaben des
    Basetools auf 0 gesetzt — Material, Ort und Einheit kennt nur noch der
    Server."""
    ops, keys = [], []
    for n, (key, amount, expected) in enumerate(plan_result['push'], 1):
        source = local.get(key) or server.get(key)
        if source is None:
            continue
        unit = source['unit']
        location = dict(source['location'])
        ops.append({'opId': 's%d' % n, 'op': 'set-quantity',
                    'material': {k: v for k, v in source['material'].items()
                                 if k in ('bt', 'name') and v},
                    'location': {k: v for k, v in location.items()
                                 if k in ('name', 'uex') and v},
                    'quality': source['quality'], 'stolen': source['stolen'],
                    'quantity': {'amount': _num(amount, unit), 'unit': unit},
                    'expectedQuantity': {'amount': _num(expected, unit),
                                         'unit': unit}})
        if key in overrides:
            ops[-1]['override'] = True
        keys.append(key)
    return [({'ops': ops[i:i + batch]}, keys[i:i + batch])
            for i in range(0, len(ops), batch)]
