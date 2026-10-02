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
Welche Steckplätze ein Schiff hat — die Datenquelle dahinter.

Beantwortet die eine Frage, die zwischen Bauplan und Schiff steht: *Wohin
gehört das Teil eigentlich, und in welcher Größe?* Ohne diese Angabe ist ein
frisch freigeschalteter Bauplan eine Zeile ohne Anschluss.

## Woher

[erkul.games](https://erkul.games), das Auslegungs-Werkzeug der Community.
Seine Daten liegen offen unter `cdn.erkul.games` — Dateiendung `.bin`, innen
aber ganz normales JSON, nur **raw-deflate** gepackt::

    json.loads(zlib.decompress(rohbytes, -15))

`zlib` gehört zur Standardbibliothek. **Keine neue Abhängigkeit** — das ist die
wichtigste Regel des Projekts, und sie wird auch hier nicht aufgeweicht.

> **Nichts davon wird mitgeliefert.** Geholt wird zur Laufzeit auf dem Rechner
> des Nutzers, von der Original-Adresse, und dort abgelegt. Dieselbe Linie wie
> bei scmdb und UEX.

## ⭐⭐ Warum sich das überhaupt anschließen lässt: dieselbe Kennung

Erkuls Feld `ref` ist **exakt** die Entitäts-Kennung, unter der auch UEX und
scmdb denselben Gegenstand führen. Gegengeprüft::

    BlastChill  →  94ea5bb5-070c-4c75-b90d-66c26c38bb2a   (in shops.py dokumentiert)
                →  94ea5bb5-070c-4c75-b90d-66c26c38bb2a   (erkul liefert dasselbe)

⚠⚠ **Deshalb wird auch hier NIE über den Namen zugeordnet** — Namen weichen
zwischen den Quellen ab, die Kennung nicht.

## ⚠ Der Sparmechanismus steckt in `catalog.bin`

Die Dateinamen tragen einen Hash, der sich **mit jedem Patch ändert**:
`coolers.a84a269d.bin`. Ein fest verdrahteter Name wäre nach dem nächsten
Donnerstag tot.

Deshalb steht am Anfang immer der Katalog (2,7 KB). Er nennt die Spielversion
und alle aktuellen Dateinamen. Steht dort dieselbe `dataVersion` wie beim
letzten Mal, ist Schluss — **ein kleiner Abruf, sonst nichts**. Das ist der
ganze Grund, warum dieses Modul einen Server nicht belastet.

## ⚠ Und es werden nur die Schiffe geholt, die der Spieler wirklich hat

Erkul führt 194 Schiffe und 25 Bodenfahrzeuge, jedes in einer eigenen Datei von
rund 16 KB. Alle zu holen wären 219 Abrufe für eine Frage, die sich auf drei
oder dreißig Schiffe bezieht.

Geholt wird deshalb **nach Bedarf**: Steht ein Schiff im Hangar (`fleet.py`) und
fehlt in der Ablage, wird genau dieses eine geholt. Ein voller Hangar kostet
einmalig so viele Abrufe, wie er Schiffe hat — danach nie wieder, bis CIG
patcht.

## Was abgelegt wird — und was nicht

Aus 16 KB Rohdaten je Schiff bleiben ein paar Zeilen übrig: **welche Art
Steckplatz in welcher Größe, und wie viele davon.** Alles andere (Kennwerte,
Beschreibungen, Schubwerte) fliegt raus. Es geht darum, ob ein Teil hineinpasst
— nicht darum, erkul nachzubauen.

    {"drak_cutlass_black": {
        "name": "Drake Cutlass Black",
        "plaetze": [{"art": "Cooler", "groesse": 2, "anzahl": 2}, …]}}

⚠ **`api.erkul.games` wird nicht angefasst.** Der Wurzelabruf sagt ausdrücklich,
dass sie privat ist und Fremdzugriff nicht erlaubt. Dieses Modul spricht
ausschließlich mit dem CDN.
"""
import json
import re
import unicodedata
import urllib.request
import zlib

from . import errors, uex
from .catalog import OFF, USER_AGENT

# Der Zweig, aus dem gelesen wird. PTU führt eigene Daten, die den Spieler auf
# LIVE nur verwirren würden.
BRANCH = 'LIVE'
BASE = 'https://cdn.erkul.games'

CACHE = 'erkul-schiffe.json'
# ⚠ Ab 2: Jeder Eintrag trägt seine **Original-Kennung** (`id`, z. B.
# `anvl_arrow`). Ohne sie kennt die Ablage nur den geschliffenen Schlüssel
# `anvlarrow` — ein einziges Wort, in dem die wortweise Zuordnung keine
# Wortgrenzen mehr findet. Eine Ablage im Format 1 sieht deshalb aus, als
# fehlten die Steckplatz-Daten, obwohl sie da sind.
#
# ⚠ Ab 3: Neben der gezählten Übersicht (`plaetze`) liegt die **einzelne**
# Steckplatzliste (`slots`) mit dem Teil, das ab Werk darinsteckt. Ohne sie
# lässt sich weder eine Auslegung speichern noch sagen, was am Schiff *nicht*
# ab Werk verbaut ist — eine gezählte Angabe (zwei Kühlerplätze Größe 2)
# nennt keinen Platz, dem sich ein Teil zuordnen ließe.
#
# ⚠ Ab 4: Jeder Platz trägt zusätzlich `passt` (siehe
# `_constraints`). Ein alter Stand ohne das Feld wird dadurch einmal neu geholt
# — sonst bliebe die Rack- und Lackauswahl bis zum nächsten Patch leer.
FORMAT = 4

# Notfrist. Maßgeblich ist die Spielversion aus `catalog.bin` — diese Frist
# greift nur, falls sich die gar nicht ermitteln lässt.
SHELF_LIFE = 30 * uex.DAY

# ⭐⭐ **`patch_bound=True`: Der Patch entscheidet, nicht die Uhr.**
# Steckplätze ändern sich mit einem Spiel-Patch und sonst nie. Eine Zeitfrist
# würde denselben Stand alle 30 Tage wegwerfen und neu holen — Abrufe, die
# niemandem nützen und die erkul bezahlt.
_store = uex.Store(CACHE, format_no=FORMAT, shelf_life=SHELF_LIFE,
                     patch_bound=True)

# ⚠ Steckplätze, die den Spieler nichts angehen. `invisible` und `uneditable`
# heißt: Das Spiel zeigt sie nicht und lässt sie nicht tauschen — ein Bauplan
# kann dort also nie landen. Sie trotzdem anzuzeigen hieße, eine Möglichkeit zu
# behaupten, die es nicht gibt.
HIDDEN = ('invisible', 'uneditable')

# Welche Steckplatz-Arten überhaupt interessant sind. Ein Schiff hat auch
# Plätze für Türen, Sitze und Leuchten; die tauchen in keinem Bauplan auf.
#
# ⚠ Die Namen kommen wörtlich aus erkuls Feld `accepts[].type` — nicht
# übersetzen, nicht schön machen. Übersetzt wird erst in der Anzeige.
#
# ⭐⭐ **Und sie sind bei scmdb dieselben.** Gegengeprüft über
# alle 1.605 Gegenstände aus `crafting_items`: `WeaponGun`, `PowerPlant`,
# `Cooler`, `Shield`, `Radar`, `QuantumDrive`, `WeaponMining`, `TractorBeam`,
# `SalvageHead` heißen in beiden Quellen gleich. Deshalb braucht es **keine**
# Übersetzungstabelle zwischen Bauplan-Art und Steckplatz-Art — und keine, die
# bei jedem Patch nachgepflegt werden müsste.
#
# ⚠ Was scmdb hat und erkul nicht: `Char_Armor_*` und `WeaponPersonal`, also
# rund 1.100 Rüstungsteile und FPS-Waffen. Für die gibt es hier nie eine
# Antwort — sie kommen gar nicht erst bis hierher, weil ihnen die Größe fehlt.
INTERESTING = frozenset((
    'Cooler', 'PowerPlant', 'Shield', 'QuantumDrive', 'Radar', 'JumpDrive',
    'WeaponGun', 'Turret', 'TurretBase', 'MissileLauncher', 'Missile',
    'BombLauncher', 'Bomb', 'MiningLaser', 'WeaponMining', 'SalvageHead',
    'TractorBeam', 'TowingBeam', 'QuantumInterdictionGenerator', 'EMP',
    'FlightController', 'Paints',
    # ⚠ Die Aufsätze für Bergbau- und Bergungsköpfe. Ihre Steckplätze sitzen
    # **im Laser**, nicht am Rumpf (`BONE_ItemPort_Consumable_1`) — beim
    # Prospector drei Ebenen tief. Ohne sie fehlt genau die Sorte Bauplan, die
    # Bergbau-Spieler zuerst freischalten.
    'MiningModifier', 'SalvageModifier',
    # Erzbehälter und Frachtaufsätze — die Ore Pods des Prospectors.
    'Container', 'Cargo',
))

# ⚠⚠ **Eine eigene Menge für die Auslegung — und das ist Absicht.**
# `INTERESTING` beantwortet, ob ein *Bauplan* hier hineinpasst. Der Warenkorb
# fragt etwas anderes: was sich in diesen Platz überhaupt einbauen lässt.
# Beides fällt auseinander, weil man Dinge kaufen kann, für die es keinen
# Bauplan gibt.
#
# Gemessen an der Cutlass Black: Vier tauschbare Plätze stehen ab
# Werk leer und fielen durch `INTERESTING` heraus — Batterie, Bordrechner
# (`Avionics`), Gravitationsgenerator und der Cockpit-Anhänger. Die ersten drei
# führt UEX in seinen Warengruppen (`Batteries`, `Avionics`), sie sind also
# kaufbar und gehören in einen Warenkorb. Ein leerer Platz ist der
# offensichtlichste Warenkorb-Posten überhaupt: Dort *fehlt* etwas.
#
# ⚠ `Flair_Cockpit` bleibt draußen. Zierrat fürs Cockpit ist keine Ausrüstung,
# und UEX führt dafür keine Warengruppe — es stünde also dauerhaft ein Posten
# in der Liste, zu dem es nie einen Preis gibt.
#
# ⚠⚠ **`INTERESTING` wird dafür NICHT erweitert.** Das würde `slot_counts()` und
# `fits()` mit ändern, und daran hängt, ob der Bauplan ins Schiff passt.
# Eine Menge, die zwei Fragen zugleich beantworten soll, beantwortet die
# zweite falsch.
SWAPPABLE = frozenset(INTERESTING | {'Battery', 'Avionics',
                                     'GravityGenerator'})


# ⭐⭐ **Teile, die an Kennzeichnungen hängen statt nur an Art und Größe.**
#
# Racks, Raketen, Bomben und Lackierungen passen nicht in jeden Platz der
# passenden Größe, sondern nur dorthin, wo die Kennzeichnungen stimmen. Beim
# Torpedoplatz der Eclipse:
#
#     Platz:  accepts MissileLauncher/MissileRack + BombLauncher/BombRack,
#             minSize 3 … maxSize 10, portTags [Eclipse_BombRack],
#             requiredTags [$Eclipse_BombRack]
#     Rack:   tags [Eclipse_BombRack], requiredTags [Eclipse_BombRack]
#
# Nach Art und Größe allein passen **126** der 160 Racks — nach der Regel in
# `fits_slot` genau die **4**, die auch erkul anbietet. Die Größe allein hätte
# obendrein die Bomben-Racks verloren: Der 20×S3-Rack ist ein Teil der Größe 3
# in einem Platz der Größe 10.
#
# Diese Teile stehen weder bei UEX im Laden noch unter den Bauplänen — ohne
# erkul bleibt die Auswahl leer. Deshalb eine eigene, kleine Ablage.
PART_FAMILIES = ('missileracks', 'rocketpods', 'missiles', 'bombs', 'paints')
PART_KINDS = frozenset(('MissileLauncher', 'BombLauncher', 'Missile', 'Bomb',
                        'Paints'))
PARTS_CACHE = 'erkul-teile.json'
PARTS_FORMAT = 1

_parts_store = uex.Store(PARTS_CACHE, format_no=PARTS_FORMAT,
                         shelf_life=SHELF_LIFE, patch_bound=True)


def _constraints(desc):
    """Was ein Platz annimmt, in Ablageform — oder `None`.

    `typen` sind Paare `[Art, [Untertypen]]` (leere Liste = jeder Untertyp).
    `verlangt` sind die Kennzeichnungen, die ein Teil **tragen** muss; erkul
    schreibt sie mit führendem `$`, das gehört nicht zum Namen.
    """
    if not isinstance(desc, dict):
        return None
    takes = []
    for node in (desc.get('accepts') or []):
        if isinstance(node, dict) and node.get('type'):
            takes.append([node['type'], list(node.get('subTypes') or [])])
    if not takes:
        return None
    return {'typen': takes,
            'min': desc.get('minSize'), 'max': desc.get('maxSize'),
            'porttags': list(desc.get('portTags') or []),
            'verlangt': [str(x).lstrip('$')
                         for x in (desc.get('requiredTags') or [])]}


def fits_slot(passt, part):
    """Passt dieses Teil in diesen Platz? Dieselbe Regel wie im Spiel.

    Vier Bedingungen, alle müssen stimmen:

    1. Art und Untertyp stehen in der Liste des Platzes.
    2. Die Größe des Teils liegt zwischen `min` und `max` des Platzes — nicht
       gleich der Platzgröße (siehe oben: Bomben-Rack S3 im Platz S10).
    3. Was das **Teil** verlangt, bietet der Platz (`requiredTags` ⊆ `portTags`).
    4. Was der **Platz** verlangt, trägt das Teil (`requiredTags` ⊆ `tags`).

    ⚠ Ohne 3 und 4 stünden am Torpedoplatz der Eclipse die Racks jedes anderen
    Schiffs zur Wahl — 126 statt 4.
    """
    if not passt or not part:
        return False
    kind, sub = part.get('typ') or '', part.get('untertyp') or ''
    if not any(t == kind and (not subs or sub in subs)
               for t, subs in (passt.get('typen') or [])):
        return False
    size = part.get('groesse')
    try:
        size = int(size) if size is not None else None
    except (TypeError, ValueError):
        size = None
    if size is not None:
        low, high = passt.get('min'), passt.get('max')
        if low is not None and size < int(low):
            return False
        if high is not None and size > int(high):
            return False
    if not set(part.get('verlangt') or []) <= set(passt.get('porttags') or []):
        return False
    if not set(passt.get('verlangt') or []) <= set(part.get('tags') or []):
        return False
    return True


def _part_name(raw):
    return ((raw.get('i18n') or {}).get('name') or raw.get('className') or '')


def _slim_part(raw):
    """Ein Teil aus einer erkul-Familie auf das Nötige eindampfen — oder `None`.

    Ein Rack bringt seine **eigenen** Plätze mit (`plaetze`) und was ab Werk
    darin steckt (`werk`, Platzname → Teil). Genau daraus entstehen nach einem
    Rack-Tausch die neuen Plätze darunter: aus 3 × Torpedo S9 werden
    20 × Bombe S3.
    """
    if not isinstance(raw, dict) or not raw.get('ref'):
        return None
    part = {'ref': raw['ref'], 'name': _part_name(raw),
            'typ': raw.get('type') or '', 'untertyp': raw.get('subType') or '',
            'groesse': raw.get('size'),
            'tags': list(raw.get('tags') or []),
            'verlangt': list(raw.get('requiredTags') or []),
            'guete': raw.get('grade') or ''}
    ports = []
    for port in (raw.get('ports') or []):
        if not isinstance(port, dict) or not port.get('name'):
            continue
        if any(((port.get('flags') or {}).get(f)) for f in HIDDEN):
            continue
        passt = _constraints(port)
        if passt and any(t in SWAPPABLE for t, _ in passt['typen']):
            ports.append({'name': port['name'], 'passt': passt})
    if ports:
        stock = {}
        for slot in (raw.get('slots') or []):
            item = slot.get('item') if isinstance(slot, dict) else None
            if (isinstance(item, dict) and item.get('ref')
                    and slot.get('portName')):
                stock[slot['portName']] = {'ref': item['ref'],
                                           'name': _part_name(item)}
        part['plaetze'] = ports
        part['werk'] = stock
    return part


def _refresh_parts(cat):
    """Die Teilelisten holen, wenn der Patch neu ist. Wirft nie.

    ⚠ Gespeichert wird nur, wenn **alle** Familien kamen. Eine halbe Ablage
    sähe aus wie eine vollständige, und dann fehlten still die Lackierungen.
    """
    version = (cat or {}).get('dataVersion') or ''
    if not version or _parts_store.load().get('spielversion') == version:
        return
    wanted = dict((f.get('kind'), f.get('path'))
                  for f in (cat.get('families') or []) if isinstance(f, dict))
    parts = {}
    for family in PART_FAMILIES:
        path = wanted.get(family)
        if not path:
            continue
        raw = _fetch('%s/%s' % (BRANCH, path), 'teile')
        if not isinstance(raw, list):
            return
        for item in raw:
            one = _slim_part(item)
            if one:
                parts[one['ref']] = one
    if parts:
        _parts_store.save({'spielversion': version, 'teile': parts})


def parts():
    """Alle abgelegten Teile mit Kennzeichnungen: Kennung → Teil. Ohne Netz."""
    return _parts_store.load().get('teile') or {}


def compatible(slot):
    """Welche abgelegten Teile in diesen Platz passen — nach `fits_slot`."""
    passt = (slot or {}).get('passt')
    if not passt:
        return []
    return [p for p in parts().values() if fits_slot(passt, p)]


def with_choices(slots, chosen):
    """Die Platzliste nach den eigenen Rack-Wechseln.

    ⚠⚠ **Ein anderes Rack bringt andere Plätze.** Tauscht jemand an der
    Eclipse das Torpedo-Rack gegen den 20×S3-Bomben-Rack, gibt es danach
    keine drei Torpedoplätze mehr, sondern zwanzig Bombenplätze. Die Plätze
    unter dem Rack werden deshalb durch die des gewählten Racks ersetzt,
    samt dessen Werksbestückung. Anzeige **und** Warenkorb lesen diese
    Liste — sonst rechnete der Warenkorb mit Torpedos, die gar nicht mehr
    drin sind.
    """
    chosen = chosen or {}
    index = parts()
    if not index:
        return slots
    out, gone, racks = [], [], {}
    for slot in slots:
        path = slot.get('pfad') or ''
        if any(path.startswith(prefix) for prefix in gone):
            continue
        # ⚠⚠ **Auch die Plätze im Werks-Rack brauchen `passt`.** erkul führt
        # sie am Rack, nicht am Schiff — bei den drei Torpedoplätzen der
        # Eclipse fehlte die Angabe deshalb, und ihre Auswahl blieb leer. Sie
        # kommt hier aus den Rack-Daten, über den Platznamen.
        parent, _, own = path.rpartition('/')
        if not slot.get('passt') and parent in racks:
            port = next((p for p in racks[parent].get('plaetze') or []
                         if p.get('name') == own), None)
            if port:
                slot = dict(slot, passt=port['passt'])
        out.append(slot)
        pick = (chosen.get(path) or {}).get('ref')
        stock = (slot.get('werk') or {}).get('ref')
        rack = index.get(pick or stock) or {}
        if not rack.get('plaetze'):
            continue
        racks[path] = rack
        if not pick or pick == stock:
            continue
        prefix = path + '/'
        gone.append(prefix)
        for port in rack['plaetze']:
            passt = port['passt']
            kind = next((t for t, _ in passt['typen'] if t in SWAPPABLE), '')
            child = {'pfad': prefix + port['name'], 'art': kind,
                     'groesse': passt.get('max'), 'passt': passt}
            stock = (rack.get('werk') or {}).get(port['name'])
            if stock:
                child['werk'] = dict(stock)
            out.append(child)
    return out


def _fetch(path, where):
    """Eine erkul-Datei abrufen und auspacken — oder `None`.

    Wirft **nie**: Ohne Netz läuft das Werkzeug weiter wie vorher, genau wie
    bei UEX. Der Grund steht dort ausführlich.
    """
    if OFF:
        return None
    address = '%s/%s' % (BASE, path.lstrip('/'))
    try:
        request = urllib.request.Request(
            address, headers={'User-Agent': USER_AGENT})
        with urllib.request.urlopen(request, timeout=uex.TIMEOUT) as reply:
            raw = reply.read()
        # ⚠ `-15` = raw deflate, ohne zlib-Kopf. Mit `zlib.decompress(roh)`
        # allein scheitert es an genau dieser Stelle — der Kopf fehlt, weil
        # erkul die Dateien schon gepackt ablegt statt sie zu übertragen.
        return json.loads(zlib.decompress(raw, -15).decode('utf-8'))
    except Exception as error:
        errors.record('erkul.holen.' + where, error)
        return None


def ship_catalog():
    """Das Inhaltsverzeichnis — Spielversion und aktuelle Dateinamen."""
    return _fetch('%s/catalog.bin' % BRANCH, 'katalog')


def _maker_table(cat):
    """Ausgeschriebener Herstellername → erkuls Kürzel.

    ⭐⭐ **Erkul liefert diese Tabelle selbst mit** (152 Einträge in
    `manufacturers.<hash>.bin`, Feld `className` neben dem Klarnamen). Ohne sie
    bleibt ein Handeintrag wie `Anvil Arrow` ohne Steckplätze: Die Kennung
    heißt `anvl_arrow`, und `anvl` ist **kein Präfix** von `Anvil` — der Vokal
    fehlt in der Mitte. Dieselbe Zusammenziehung bei `aegs` (Aegis) und `misc`.

    ⚠ Schiffe aus dem Pledge-Export bringen das Herstellerkürzel mit; von
    Hand eingetragene Schiffe haben es nicht und brauchen diese Tabelle.
    """
    path = next((f.get('path') for f in (cat.get('families') or [])
                 if (f.get('path') or '').startswith('manufacturers')), '')
    if not path:
        return {}
    rows_in = _fetch('%s/%s' % (BRANCH, path), 'hersteller')
    out = {}
    for entry in (rows_in or []):
        abbrev = (entry.get('className') or '').strip()
        plain = ((entry.get('i18n') or {}).get('name') or '').strip()
        if abbrev and plain:
            out[_slim(plain)] = abbrev
    return out


def load():
    return _store.load() or {}


def age():
    return _store.age()


def game_version():
    """Die Spielversion, zu der die Ablage gehört — oder `''`."""
    return load().get('spielversion') or ''


def candidates(name, maker='', short='', maker_short=''):
    """Alle Schreibweisen, unter denen erkul dieses Schiff führen könnte.

    Die Reihenfolge ist die Trefferquote, gemessen an einem echten
    Hangar-Export (42 Einträge):

    | Stufe | Treffer |
    |---|---|
    | Kurzname des Exports (`ANVL_Arrow` → `anvlarrow`) | **33** |
    | Herstellerkürzel + angezeigter Name | **1** |

    ⚠ Die restlichen acht sind **kein Zuordnungsfehler**: Crucible, Endeavor,
    Galaxy, Liberator, Merchantman und die beiden ATLS gibt es im Spiel noch
    gar nicht. Erkul führt nur Flugfähiges — ein Fehltreffer heißt hier also:
    noch nicht im Spiel, nicht unbekannt. Das ist eine Auskunft, keine
    Panne, und wird dem Spieler auch so gesagt.
    """
    out = []
    # ⚠ Das **Kürzel** des Herstellers vor seinem ausgeschriebenen Namen:
    # Erkul führt `Roberts Space Industries` als `rsi`. Ohne diese Zeile fände
    # die Ursa Medivac keinen Anschluss, obwohl `rsi_ursa_medivac` existiert.
    for attempt in (short, '%s %s' % (maker_short, name), name,
                    '%s %s' % (maker, name)):
        slimmed = _slim(attempt)
        if slimmed and slimmed not in out:
            out.append(slimmed)
    return out


def ident(name, maker='', short='', maker_short=''):
    """Ein Schiff → erkuls Kennung, oder `''`.

    Drei Stufen, in dieser Reihenfolge: Buchstabenvergleich über den Kurznamen,
    dann über den angezeigten Namen, dann **wortweise**. Die dritte kostet
    etwas mehr, greift aber genau dort, wo die ersten beiden scheitern — und
    das ist bei jedem vierten Schiff der Fall.
    """
    stored = load()
    known_ships = stored.get('schiffe') or {}
    # ⚠ Auch hier: `Anvil Aerospace` muss zu `anvl` werden, sonst findet ein
    # von Hand eingetragenes Schiff seine eigenen Daten nicht wieder.
    maker_short = maker_short or (stored.get('hersteller') or {}).get(
        _slim(maker), '')
    for slimmed in candidates(name, maker, short, maker_short):
        if slimmed in known_ships:
            return slimmed
    # ⚠ Wortweise wird gegen die **Original-Kennungen** gesucht (`id`), nicht
    # gegen die geschliffenen Schlüssel — sonst fehlen die Wortgrenzen.
    by_id = dict((v.get('id') or k, k) for k, v in known_ships.items())
    hits = _search_wordwise(by_id, name, maker, short, maker_short)
    return by_id.get(hits, '') if hits else ''


def _slim(text):
    """`ANVL_F7C_Hornet` → `anvlf7chornet` — alles weg außer Buchstaben/Ziffern.

    ⚠ Bewusst **ohne** Trennzeichen: Der Export schreibt `L_22_Alpha_Wolf`,
    erkul `l22alphawolf`. Wer die Unterstriche behält, vergleicht zwei
    Schreibweisen derselben Sache und findet nichts.
    """
    # ⚠ **Akzente werden übersetzt, nicht weggeworfen.** Aus „San tok.Yāi"
    # wurde sonst `santokyi` — das `ā` fiel als Sonderzeichen heraus, und der
    # Name unterschied sich damit von jedem, der ihn ohne Strich schreibt.
    parts = unicodedata.normalize('NFKD', text or '')
    without_accent = ''.join(z for z in parts if not unicodedata.combining(z))
    return re.sub(r'[^a-z0-9]', '', without_accent.lower())


# Römische Zahlen, wie sie in Schiffsnamen vorkommen. Weiter als V geht es
# nicht — es gibt kein „Mk VI".
_ROMAN = {'i': '1', 'ii': '2', 'iii': '3', 'iv': '4', 'v': '5'}


def _words(text):
    """Ein Name → seine Wörter, mit `Mk II` zu `mk2` vereinheitlicht.

    ⚠ Punkte und Bindestriche fallen **vor** dem Zerlegen weg, nicht danach:
    Aus `F7C-M` wird `f7cm` (ein Wort, wie bei erkul), nicht `f7c` + `m`.
    """
    # ⚠ **Akzente werden übersetzt, nicht weggeworfen.** `San tok.Yāi` zerfiel
    # zu `san toky i`, weil das `ā` als Trennzeichen durchging — erkul schreibt
    # `santokyai`. Ein Schiff mit diakritischem Zeichen im Namen war damit
    # nicht auffindbar.
    import unicodedata
    parts = unicodedata.normalize('NFKD', text or '')
    without_accent = ''.join(z for z in parts if not unicodedata.combining(z))
    t = without_accent.lower().replace('.', '').replace('-', '')
    out = []
    for word in re.split(r'[^a-z0-9]+', t):
        if not word:
            continue
        hits = re.fullmatch(r'mk\s*([ivx]+|\d+)', word)
        if hits:
            number = hits.group(1)
            out.append('mk' + _ROMAN.get(number, number))
            continue
        # „Mk" und „II" getrennt geschrieben — zusammenziehen.
        if word in _ROMAN and out and out[-1] == 'mk':
            out[-1] = 'mk' + _ROMAN[word]
            continue
        out.append(word)
    return out


def _is_abbrev(short, long):
    """Ist `short` eine Zusammenziehung von `long`? (`aegs` ↔ `aegis`)

    Geprüft wird, ob die Buchstaben von `short` **in dieser Reihenfolge** in
    `long` vorkommen — mit Lücken, aber ohne Umsortieren.

    ⚠ **Nur für kurze Kürzel und nur bei gleichem Anfang.** Ohne diese beiden
    Bremsen wäre fast jedes kurze Wort Teilfolge von fast jedem langen, und
    die Zuordnung fände überall etwas. `aegs`/`aegis` besteht, `pir`/`pirate`
    ebenso — `abc`/`aXbXc` in einem beliebigen Schiffsnamen nicht mehr, weil
    der Anfang stimmen muss.
    """
    if not short or not long or len(short) > 5 or len(short) >= len(long):
        return False
    if short[0] != long[0]:
        return False
    where = 0
    for char in long:
        if where < len(short) and char == short[where]:
            where += 1
    return where == len(short)


def _matches_wordwise(erkul_id, wanted, chain=''):
    """Deckt `wanted` alle Wörter dieser erkul-Kennung ab? Gibt die Güte zurück.

    ⭐ **Die dritte Zuordnungsstufe** — sie fängt genau die Fälle, an denen
    Buchstabenvergleich scheitert, und das sind keine Ausnahmen:

    | im Hangar | bei erkul | woran es scheiterte |
    |---|---|---|
    | `Drake Ironclad Assault` | `drak_ironclad_assault` | Hersteller ausgeschrieben |
    | `F7C-M Super Hornet Mk II` | `anvl_hornet_f7cm_mk2` | römische Zahl, andere Wortfolge |

    ⚠ **Ein erkul-Wort darf auch Anfang eines gesuchten Worts sein** — genau so
    verhält sich jedes Herstellerkürzel (`drak` → Drake, `aegs` → Aegis,
    `anvl` → Anvil). Ohne das bleibt der halbe Hangar ohne Zuordnung.

    ⚠ Und **umgekehrt gilt es nicht**: `wanted` darf mehr Wörter haben als
    erkul (`Super` steht nur im Hangar-Namen), aber jedes erkul-Wort muss
    vorkommen. Sonst würde `anvl_hornet_f7cm` auch auf eine Mk II passen.
    """
    own = _words(erkul_id)
    # ⭐⭐ **Das erste Wort darf unerklärt bleiben — es ist das
    # Herstellerkürzel, und die Namen passen oft schlicht nicht zusammen.**
    # Gemessen über alle 280 UEX-Schiffe: UEX schreibt `C.O. Mustang Alpha`,
    # erkul `cnou_mustang_alpha`; `Greycat PTV` heißt dort `gama_ptv`,
    # `Esperia Blade` ist `vncl_blade`. Weder Wortanfang noch
    # Zusammenziehung greifen da — es sind verschiedene Namen für dieselbe
    # Firma.
    #
    # ⚠ **Nur das erste Wort, und nur eines.** Der Rest muss vollständig
    # passen, sonst würde `aegs_gladius_valiant` auch auf eine schlichte
    # Gladius passen. Und weil der Treffer schwächer bewertet wird, gewinnt
    # bei zwei Kandidaten weiterhin der mit dem passenden Hersteller.
    without_maker = False
    score = 0
    for where, word in enumerate(own):
        if word in wanted:
            score += 3
        elif any(g.startswith(word) for g in wanted):
            # Herstellerkürzel als Wortanfang: `drak` steht für „Drake".
            score += 2
        elif any(_is_abbrev(word, g) for g in wanted):
            # ⭐⭐ **Kürzel mit fehlendem Vokal in der Mitte.** Erkul zieht
            # Herstellernamen zusammen, statt sie abzuschneiden: `aegs` für
            # `Aegis`, `anvl` für `Anvil`. Ein Vergleich auf Wortanfang findet
            # das nie — `aegs` ist kein Anfang von `aegis`, der Buchstabe `i`
            # fehlt mittendrin.
            #
            # ⚠ Eine Hersteller-Tabelle löst das NICHT, obwohl erkul eine
            # mitliefert: Dort tragen **fünf** verschiedene Kürzel den Namen
            # `Aegis Dynamics` (`fski`, `mxox`, `prar`, `aeg`, `tras`) — und
            # `aegs`, das die Schiffe benutzen, ist nicht darunter.
            score += 1
        elif chain and word in chain:
            # ⚠ Der umgekehrte Fall: erkul schreibt `alphawolf` **zusammen**,
            # der Hangar führt `L-22 Alpha Wolf` getrennt. Dafür braucht es die
            # Wörter in ihrer **Reihenfolge** — aus einer Menge verkettet käme
            # `alphakrigl22wolf` heraus, und darin steht `alphawolf` nicht.
            # Prüfung 139 hält das fest.
            score += 1
        elif where == 0 and len(own) > 1 and len(word) <= 5:
            # Das Herstellerkürzel passt zu keinem Wort — hingenommen, aber
            # ohne Punkte. Ein Treffer mit passendem Hersteller schlägt diesen
            # damit immer.
            without_maker = True
        else:
            return 0
    # ⚠ Ohne den Hersteller muss mindestens **ein** eigenes Wort übrig sein,
    # das wirklich getroffen hat — sonst passt `gama_ptv` auf jeden Namen mit
    # drei Buchstaben.
    if without_maker and score < 3:
        return 0
    return score


# ⭐⭐ **Handzuordnungen für die Fälle, die kein Verfahren löst.**
#
# Von 265 einzeln durchgeprüften Schiffen fanden 220 ihre
# Daten von selbst, 35 sind Konzepte — von den zehn übrigen ließen sich fünf
# **nicht** durch bessere Regeln retten, weil die Namen schlicht verschieden
# sind:
#
# | im Werkzeug | bei erkul | woran es liegt |
# |---|---|---|
# | Aegis Hammerhead | `aegs_hammerhead_gs` | Zusatz, den es hier nicht gibt |
# | Aegis Idris-P | `aegs_idris_p` | Bindestrich: `idrisp` gegen `idris`+`p` |
# | Aopoa San tok.Yāi | `xnaa_santokyai` | anderer Hersteller, zusammengeschrieben |
#
# ⚠⚠ **Warum eine Liste und keine klügere Regel.** Allgemeinere Regeln
# zerstören mehr, als sie reparieren: Ein Bindestrich, der beide
# Schreibweisen erzeugt, rettet die Idris — und bricht die F7C-M Super Hornet,
# weil `f7c_mk2` und `f7cm_mk2` dann gleich gut passen. Gemessen fällt die
# Trefferquote damit von 220 auf unter 200, mit Ausfällen bei Aurora, Kruger
# und Mirai. Deshalb werden die Reste ausdrücklich benannt.
#
# Dasselbe Muster wie bei `bp-overrides.json`: Eine kurze, sichtbare Liste
# schlägt eine Regel, die niemand mehr durchschaut.
#
# ⚠ Ein Eintrag hier ist eine **Behauptung** und wird beim Patch nicht geprüft.
# Fällt eine Zuordnung auf, gehört sie geändert oder gestrichen — nicht ergänzt.
MANUAL_MAP = {
    'aegishammerhead': 'aegs_hammerhead_gs',
    'aegishammerheadbestinshowedition': 'aegs_hammerhead_gs',
    'aegisidrisp': 'aegs_idris_p',
    'aegisidrism': 'aegs_idris_m',
    'aopoasantokyai': 'xnaa_santokyai',
    # Drei Kandidaten (`rover`, `rover_emerald`, `medivac`) — ohne Zusatz ist
    # der Rover gemeint, das ist die Grundausführung.
    'rsiursa': 'rsi_ursa_rover',
}


def _search_wordwise(listing, name, maker='', short='', maker_short=''):
    """Die beste wortweise Zuordnung — oder `''`, wenn sie nicht eindeutig ist.

    ⚠ **Bei Gleichstand wird nichts zurückgegeben.** Zwei gleich gute Treffer
    heißen, dass die Angabe nicht ausreicht; irgendeinen davon zu nehmen wäre
    geraten. Lieber „keine Daten" sagen als das falsche Schiff zeigen.
    """
    # ⚠⚠ **Der Kurzcode bleibt hier draußen** — anders als in den Stufen davor.
    # Er kann veraltet sein: Der Pledge-Export führt die „F7C-M Super Hornet
    # Mk II" unter `ANVL_F7C_M_Super_Hornet_Mk_I`. Nimmt man ihn mit, stehen
    # `mk1` **und** `mk2` in der Suchmenge, und `anvl_hornet_f7c_mk2` wird
    # genauso gut bewertet wie `anvl_hornet_f7cm_mk2` — Gleichstand, also gar
    # keine Zuordnung. Der angezeigte Name ist die verlässlichere Angabe.
    # ⚠ Die Handzuordnung zuerst — sie ist ausdrücklich gesetzt und schlägt
    # jedes Verfahren.
    manual = MANUAL_MAP.get(_slim(name))
    if manual and manual in listing:
        return manual

    seq = _words(' '.join(x for x in (maker_short, maker, name) if x))
    wanted = set(seq)
    if not wanted:
        return ''
    # ⚠ Die Wörter **in ihrer Reihenfolge** aneinandergehängt — nur so findet
    # sich `alphawolf` in „Alpha Wolf" wieder.
    chain = ''.join(seq)
    scored = []
    for ident_ in listing:
        score = _matches_wordwise(ident_, wanted, chain)
        if score:
            scored.append((score, ident_))
    if not scored:
        return ''
    scored.sort(reverse=True)
    if len(scored) > 1 and scored[0][0] == scored[1][0]:
        return ''
    return scored[0][1]


def _collect_slots(node, out):
    """Alle nutzbaren Steckplätze einsammeln, rekursiv.

    ⚠⚠ **Die Steckplätze stehen unter `vehicle.hardpoints`, NICHT unter
    `slots`.** Die beiden sehen sich ähnlich und meinen Verschiedenes:
    `hardpoints` sagt, *was hineinpasst* (`accepts`, `minSize`, `maxSize`),
    `slots` nur, *was gerade drinsteckt*. Wer `slots` liest, bekommt für jedes
    Schiff **null** Steckplätze zurück — kein Fehler, keine Meldung, einfach
    eine leere Liste.

    ⚠ Rekursiv bleibt es trotzdem: Bei einem Turm hängen die Waffenplätze am
    Turm-Bauteil, nicht am Rumpf. Wer nur die oberste Ebene liest, findet bei
    einem bewaffneten Schiff keine einzige Waffe.
    """
    if not isinstance(node, list):
        return
    for slot_node in node:
        if not isinstance(slot_node, dict):
            continue
        _one_slot(slot_node, out)
        for field in ('hardpoints', 'ports', 'children', 'slots'):
            _collect_slots(slot_node.get(field), out)
        part = slot_node.get('item')
        if isinstance(part, dict):
            for field in ('ports', 'hardpoints', 'slots'):
                _collect_slots(part.get(field), out)


def _one_slot(slot_node, out):
    """Einen einzelnen Steckplatz auswerten, wenn er den Spieler angeht."""
    flags = slot_node.get('flags') or {}
    if any(flags.get(f) for f in HIDDEN):
        return
    kinds = []
    for takes in (slot_node.get('accepts') or []):
        kind = (takes or {}).get('type')
        if kind in INTERESTING:
            kinds.append(kind)
    if not kinds:
        return
    size = slot_node.get('maxSize')
    if size is None:
        size = slot_node.get('minSize')
    if size is None:
        return
    for kind in kinds:
        out.append((kind, int(size)))


def _hardpoint_index(node, out, path=''):
    """Jeden Steckplatz unter seinem **vollen Pfad** ablegen.

    ⚠⚠ **Der bloße `name` reicht als Schlüssel NICHT.** Bei der Cutlass Black
    heißt der Waffenplatz in jedem der vier Gimbal-Halter gleich
    (`hardpoint_class_2`, sechsmal vergeben), und `missile_01_attach` gibt es
    ebenfalls sechsmal. Wer danach schlüsselt, überschreibt vier Waffen mit
    einer und merkt es nicht.

    ⚠ Und `portPath` hilft nicht weiter: Gemessen hat es bei
    **allen 68** Steckplätzen der Cutlass die Länge 1 — die Verschachtelung
    steht dort gar nicht drin. Der Pfad wird deshalb hier selbst gebaut.
    """
    if not isinstance(node, list):
        return
    for slot_node in node:
        if not isinstance(slot_node, dict):
            continue
        own_name = slot_node.get('name') or slot_node.get('portName') or ''
        full = (path + '/' + own_name) if path else own_name
        if own_name:
            out.setdefault(full, slot_node)
        for field in ('hardpoints', 'ports', 'children', 'slots'):
            _hardpoint_index(slot_node.get(field), out, full)
        part = slot_node.get('item')
        if isinstance(part, dict):
            for field in ('ports', 'hardpoints', 'slots'):
                _hardpoint_index(part.get(field), out, full)


def _one_hardpoint(slot, hp_index, path):
    """Einen einzelnen Steckplatz in die Ablageform bringen — oder `None`.

    Art und Größe kommen bevorzugt aus dem **Steckplatz** (`hardpoints`), nicht
    aus dem Teil, das gerade darinsteckt: Der Platz sagt, was hineinpasst, das
    Teil nur, was zufällig ab Werk gewählt wurde. Erst wenn es zu einem Platz
    keinen Eintrag gibt — bei 26 der 94 Pfade der Cutlass, weil ein
    Raketenplatz erst durch das eingebaute Rack entsteht —, muss das Teil
    aushelfen.
    """
    desc = hp_index.get(path) or {}
    kinds = [(n or {}).get('type') for n in (desc.get('accepts') or [])]
    kinds = [a for a in kinds if a in SWAPPABLE]
    part = slot.get('item') if isinstance(slot.get('item'), dict) else None

    kind = kinds[0] if kinds else ((part or {}).get('type') or '')
    if kind not in SWAPPABLE:
        return None

    size = desc.get('maxSize')
    if size is None:
        size = desc.get('minSize')
    if size is None and part is not None:
        size = part.get('size')
    try:
        size = int(size)
    except (TypeError, ValueError):
        # ⚠ Ein Platz ohne Größe ist keine Panne — Lackierungen haben keine.
        # Er bleibt trotzdem drin: Auch eine Lackierung wird gekauft.
        size = None

    entry = {'pfad': path, 'art': kind, 'groesse': size}
    # ⭐ Was der Platz annimmt — für Racks, Raketen, Bomben und Lackierungen
    # entscheidet das statt der bloßen Größe. Siehe `fits_slot`.
    passt = _constraints(desc)
    if passt:
        entry['passt'] = passt
    if part is not None and part.get('ref'):
        # ⚠⚠ **Die Kennung ist das Entscheidende, nicht der Name.** `ref` ist
        # dieselbe Entitäts-Kennung wie bei UEX und scmdb — nur über sie hängt
        # später ein Ladenpreis am Werksteil. Der Name steht daneben, damit
        # etwas Lesbares angezeigt werden kann, und wird nie zum Zuordnen
        # benutzt.
        entry['werk'] = {
            'ref': part['ref'],
            'name': ((part.get('i18n') or {}).get('name')
                     or part.get('className') or ''),
        }
    return entry


def _collect_loadout(node, hp_index, out, path=''):
    """Alle **tauschbaren** Steckplätze samt Werksausstattung, rekursiv.

    ⚠⚠ **Der Filter ist `kind`, nicht die Flaggen.** Erkul sagt an jedem Slot
    selbst, ob der Spieler ihn tauschen kann: `swappable` oder `fixed`. Bei der
    Cutlass Black sind das 47 gegen 47 — die festen sind Panzerung und
    Struktur. In einen festen Platz kommt nie ein anderes Teil, dort gibt es
    also auch nichts zu kaufen.

    ⚠ **Trotzdem darf bei `fixed` nicht abgebrochen werden.** `hardpoint_turret`
    ist fest, aber die beiden Gimbal-Halter darin und ihre CF-337 Panther sind
    tauschbar. Wer die Rekursion an einem festen Platz beendet, verliert genau
    die Turmwaffen — und das fällt nicht auf, weil eine kürzere Liste immer
    noch wie eine Liste aussieht.
    """
    if not isinstance(node, list):
        return
    for slot in node:
        if not isinstance(slot, dict):
            continue
        own_name = slot.get('portName') or slot.get('name') or ''
        full = (path + '/' + own_name) if path else own_name
        if own_name and slot.get('kind') == 'swappable':
            entry = _one_hardpoint(slot, hp_index, full)
            if entry:
                out.append(entry)
        _collect_loadout(slot.get('children'), hp_index, out, full)


def fetch_ship(erkul_id, path):
    """Ein einzelnes Schiff holen und auf seine Steckplätze eindampfen.

    Abgelegt werden **zwei** Sichten auf dieselbe Sache, und beide werden
    gebraucht:

    | Feld | sagt | wofür |
    |---|---|---|
    | `plaetze` | zwei Kühlerplätze Größe 2 | passt mein Bauplan hinein? |
    | `slots` | *dieser* Platz trägt ab Werk ColdSnap | Auslegung und Warenkorb |

    ⚠ Die gezählte Sicht bleibt unverändert erhalten. Sie beantwortet ihre
    Frage besser als eine Einzelliste, und die Passt-Prüfung hängt daran.
    """
    raw = _fetch('%s/%s' % (BRANCH, path), 'schiff')
    if not isinstance(raw, dict):
        return None
    found = []
    _collect_slots((raw.get('vehicle') or {}).get('hardpoints'), found)
    # Zusätzlich die belegten Plätze durchgehen: Was in einem Turm steckt,
    # bringt seine eigenen Waffenplätze mit.
    _collect_slots(raw.get('slots'), found)
    counted = {}
    for kind, size in found:
        counted[(kind, size)] = counted.get((kind, size), 0) + 1
    slot_counts = [{'art': a, 'groesse': g, 'anzahl': n}
               for (a, g), n in sorted(counted.items())]

    # ⚠⚠ **Die Werksausstattung steht in `slots`, NICHT in einem Feld
    # `default`.** Ein solches Feld gibt es nicht — nachgesehen über die
    # vollständige Struktur (`default`, `defaults`, `installed`,
    # `loadout`, `equipped`: keines vorhanden). Was drinsteckt, hängt als
    # `item` am Slot, und die tieferen Ebenen hängen an **`children`**, nicht
    # an `ports`: `ports` beschreibt wieder nur Plätze. Bei der Cutlass Black
    # stecken 90 Gegenstände in drei Ebenen — wer nur die oberste liest,
    # verliert 26 davon, darunter jede Turmwaffe und jede Rakete.
    hp_index = {}
    _hardpoint_index((raw.get('vehicle') or {}).get('hardpoints'),
                           hp_index)
    slots = []
    _collect_loadout(raw.get('slots'), hp_index, slots)

    name = ((raw.get('vehicle') or {}).get('vehicleDisplayName')
            or (raw.get('i18n') or {}).get('name') or erkul_id)
    return {'name': name, 'plaetze': slot_counts, 'slots': slots}


def add_missing(rows):
    """Die genannten Schiffe holen, soweit sie noch fehlen.

    `rows` sind Tripel `(name, maker, short)` aus dem Hangar. Gibt
    zurück, wie viele wirklich geholt wurden — `0` heißt: alles war schon da
    **oder** kein Netz, und beides ist in Ordnung: Was fehlt, wird beim
    nächsten Mal nachgeholt.
    """
    if OFF or not rows:
        return 0
    cat = ship_catalog()
    if not isinstance(cat, dict):
        return 0
    # Die Teilelisten hängen am selben Katalog und am selben Patch — ein
    # Abruf, wenn sich der Spielstand geändert hat, sonst keiner.
    _refresh_parts(cat)
    version = cat.get('dataVersion') or ''
    data = load()
    # ⚠ Neuer Patch → alles Alte gilt nicht mehr. Steckplätze ändern sich mit
    # einem Patch, und ein Schiff, das gestern vier Waffenplätze hatte, kann
    # heute drei haben.
    known_ships = ({} if data.get('spielversion') != version
               else dict(data.get('schiffe') or {}))

    # Der Index nennt jedes Schiff mit seiner aktuellen Datei.
    # ⚠⚠ **Zwei Sichten auf dasselbe Verzeichnis, und beide werden gebraucht.**
    # `listing` hat den geschliffenen Schlüssel (`drakironcladassault`) für
    # den Buchstabenvergleich; `roh_ids` behält die Original-Kennung
    # (`drak_ironclad_assault`), weil die wortweise Suche die **Wortgrenzen**
    # braucht. Mit der ersten Sicht allein liefe die wortweise Stufe gegen
    # ein einziges langes Wort und träfe nie etwas.
    # ⚠ Einmal je Lauf geholt, dann abgelegt — siehe `_maker_table`.
    maker_abbrev = (data.get('hersteller')
                          if data.get('spielversion') == version
                          else None) or _maker_table(cat)
    listing = {}
    raw_ids = {}
    for group in (cat.get('groups') or []):
        path = group.get('indexPath')
        if not path:
            continue
        index = _fetch('%s/%s' % (BRANCH, path), 'index')
        for entry in ((index or {}).get('blobs') or []):
            if entry.get('id') and entry.get('path'):
                listing[_slim(entry['id'])] = entry
                raw_ids[entry['id']] = entry

    fetched = 0
    for row in rows:
        name, maker, short, maker_short = (list(row) + ['', '', ''])[:4]
        # Der ausgeschriebene Hersteller wird zum Kürzel, wenn keines dabei ist.
        maker_short = maker_short or maker_abbrev.get(_slim(maker), '')
        possible = candidates(name, maker, short, maker_short)
        if any(k in known_ships for k in possible):
            continue
        hits = next((k for k in possible if k in listing), '')
        source = listing
        if not hits:
            hits = _search_wordwise(raw_ids, name, maker, short, maker_short)
            source = raw_ids
            # Abgelegt wird immer unter dem geschliffenen Schlüssel, damit
            # `ident()` beide Wege gleich behandelt.
            key = _slim(hits) if hits else ''
        else:
            key = hits
        if not hits or key in known_ships:
            continue
        one = fetch_ship(key, source[hits]['path'])
        if one:
            # ⚠ Die Original-Kennung bleibt am Eintrag: `ident()` braucht sie
            # für dieselbe wortweise Suche gegen die Ablage.
            one['id'] = hits
            known_ships[key] = one
            fetched += 1

    if fetched or data.get('spielversion') != version:
        # ⚠ `maker_abbrev`, **nicht** `maker` — das ist die
        # Schleifenvariable aus dem Schiffs-Tupel und wäre hier eine
        # Zeichenkette, wo ein Wörterbuch erwartet wird.
        _store.save({'spielversion': version, 'schiffe': known_ships,
                         'hersteller': maker_abbrev})
    return fetched


def slot_counts(name, maker='', short='', maker_short=''):
    """Die Steckplätze eines Schiffs — oder `[]`, wenn es unbekannt ist."""
    key = ident(name, maker, short, maker_short)
    if not key:
        return []
    return ((load().get('schiffe') or {}).get(key) or {}).get('plaetze') or []


def hardpoints(name, maker='', short='', maker_short='', chosen=None):
    """Die **einzelnen** tauschbaren Steckplätze eines Schiffs.

    Mit `chosen` (die eigene Auslegung, `cart.loadout()`) kommen nach einem
    Rack-Wechsel die Plätze des gewählten Racks zurück — siehe `with_choices`.

    Je Eintrag `pfad`, `art`, `groesse` und — wenn ab Werk etwas darinsteckt —
    `werk` mit `ref` und `name`. Leere Liste heißt „keine Daten"; ob das Schiff
    überhaupt bekannt ist, sagt `knows()`.

    ⚠ Das ist **nicht** `slot_counts()`. Dort steht die gezählte Übersicht („zwei
    Kühlerplätze Größe 2"), hier jeder Platz einzeln mit seiner Kennung. Nur
    hier lässt sich ein Teil einem Platz zuordnen.
    """
    key = ident(name, maker, short, maker_short)
    if not key:
        return []
    entry = (load().get('schiffe') or {}).get(key) or {}
    # ⚠ Immer durch `with_choices`, auch ohne eigene Wahl: Dort bekommen die
    # Plätze im Werks-Rack ihre Angabe, was sie annehmen.
    return with_choices(entry.get('slots') or [], chosen)


def stock_loadout(name, maker='', short='', maker_short=''):
    """Was ab Werk in diesem Schiff steckt — je Teil einmal, mit Anzahl.

    Zurück kommen Einträge `{'ref', 'name', 'art', 'groesse', 'anzahl'}`,
    häufigste zuerst. Gezählt wird über die **Kennung**, nicht über den Namen:
    Vier Gimbal-Halter desselben Typs sind ein Posten mit `anzahl: 4`.

    ⚠ Es ist die **Standard**-Ausstattung. Was in einem angetroffenen Schiff
    wirklich steckt, kann jemand getauscht haben — die Anzeige spricht
    deshalb von der Werksausstattung, nie vom tatsächlichen Inhalt.
    """
    counted = {}
    for slot_node in hardpoints(name, maker, short, maker_short):
        stock = slot_node.get('werk') or {}
        ref = stock.get('ref')
        if not ref:
            continue
        entry = counted.setdefault(ref, {
            'ref': ref, 'name': stock.get('name') or '',
            'art': slot_node.get('art') or '', 'groesse': slot_node.get('groesse'),
            'anzahl': 0})
        entry['anzahl'] += 1
    out = list(counted.values())
    out.sort(key=lambda x: (-x['anzahl'], (x['name'] or '').lower()))
    return out


def fits(kind, size, name, maker='', short='', maker_short=''):
    """Passt ein Teil dieser Art und Größe in dieses Schiff?

    Gibt die Anzahl passender Steckplätze zurück, `0` wenn keiner passt.

    ⚠ **`0` heißt nicht immer „passt nicht".** Ist das Schiff gar nicht in der
    Ablage — weil es im Spiel noch nicht existiert oder noch nicht geholt
    wurde —, kommt ebenfalls `0`. Die Anzeige muss beide Fälle auseinander
    halten; `knows()` sagt, welcher vorliegt.
    """
    if not kind or size is None:
        return 0
    try:
        size = int(size)
    except (TypeError, ValueError):
        return 0
    total = 0
    for slot_node in slot_counts(name, maker, short, maker_short):
        if slot_node.get('art') == kind and int(slot_node.get('groesse', -1)) == size:
            total += int(slot_node.get('anzahl') or 0)
    return total


def knows(name, maker='', short='', maker_short=''):
    """Liegen für dieses Schiff überhaupt Steckplatz-Daten vor?"""
    return bool(ident(name, maker, short, maker_short))


def matching_ships(kind, size, ships):
    """In welche dieser Schiffe passt ein Teil dieser Art und Größe?

    `ships` sind die Einträge aus `fleet.load()['schiffe']`. Zurück kommen
    Paare `(Schiffsname, Anzahl Steckplätze)`, die meisten Plätze zuerst.

    ⭐ **Das ist die Auskunft, die keine fremde Seite geben kann** — nicht,
    weil die Daten geheim wären, sondern weil keine Seite weiß, welche Schiffe
    *dir* gehören und welche Baupläne *du* hast. Erkul kennt die Schiffe, der
    Watcher kennt den Spieler; erst zusammen ergibt es eine Antwort.

    ⚠ Eine leere Liste heißt: passt nirgends hinein — **nicht**: keine Daten.
    Wer beides gleich behandelt, sagt jemandem mit leerem Hangar, sein Teil
    sei nutzlos. Die Anzeige prüft deshalb vorher, ob überhaupt ein Schiff
    eingetragen ist.
    """
    if not kind or size is None:
        return []
    out = []
    for s in (ships or []):
        count = fits(kind, size, s.get('name') or '',
                       s.get('hersteller') or '', s.get('kurz') or '',
                       s.get('hkurz') or '')
        if count:
            out.append((s.get('name') or '', count))
    out.sort(key=lambda x: (-x[1], x[0].lower()))
    return out
