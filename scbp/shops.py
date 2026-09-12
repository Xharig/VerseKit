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
Was ein fertiges Teil im Laden kostet — und wo es dort liegt.

## Die Frage, die nur dieses Werkzeug beantworten kann

Der Watcher kennt das **Rezept** (`crafting.py`) und die **Rohstoffpreise**
(`prices.py`). Daraus ergibt sich, was Selberbauen kostet. Fehlte bisher die
andere Hälfte: Was kostet dasselbe Teil fertig im Regal?

Erst beide Zahlen nebeneinander beantworten die Frage, um die es wirklich geht
— **lohnt der Aufwand überhaupt?** Keine fremde Seite kann das, weil keine
weiss, welche Baupläne du hast und was in deinem Lager liegt.

## ⭐⭐ Die Brücke lag schon da: `productEntityClass`

Jeder Bauplan in unseren Rezeptdaten trägt eine Entitäts-Kennung, und das ist
**exakt** das `uuid`, unter dem UEX denselben Gegenstand führt::

    BlastChill  →  94ea5bb5-070c-4c75-b90d-66c26c38bb2a
                →  items_prices?uuid=94ea5bb5-…   →  vier Läden mit Preis

⚠⚠ **Deshalb wird NIE über den Namen zugeordnet.** Genau daran ist es hier
schon einmal schiefgegangen: `commodity_name=Gold` liefert `Golden Medmon`
gleich mit, dessen 71.000 aUEC wie ein sagenhafter Goldpreis aussahen. Über
eine Kennung gibt es diese Fehlerklasse nicht — entweder es ist dasselbe Teil
oder gar keins.

**Gemessen am 04.09.2026** über alle 1.604 Baupläne mit Kennung: **1.169 (72,9 %)
kennt UEX**, und bei **1.118 davon (95,6 %) stimmt sogar der Name überein**.
Diese Namensgleichheit ist die eigentliche Gegenprobe — eine falsche Kennung
ergäbe zufällige Paarungen, keine tausend Treffer mit demselben Namen.

Die 435 ohne Treffer sind echte Lücken bei UEX (Testgegenstände wie
`Metamaterial Test #146`, Munitionsmagazine, 54 Radargeräte). Dort bleibt das
Feld **leer** — dieselbe Regel wie bei den Rohstoffpreisen: lieber nichts
sagen als etwas erfinden.

## Warum je Gegenstand geholt wird, nicht auf Vorrat

`items_prices?uuid=…` liefert genau einen Gegenstand. Das ist der billigste
Zuschnitt, den diese Schnittstelle hergibt:

| Weg | Abrufe | Bewertung |
|---|---|---|
| alles im Voraus | ~34 Kategorien, danach 859 Gegenstände im Speicher | unhöflich, und 90 % davon sieht nie jemand |
| **je Gegenstand, wenn jemand hinschaut** | **1** | so viel wie nötig |

⚠ Ein Gegenstand wird **höchstens einmal am Tag** neu geholt, auch wenn man
zehnmal auf ihn schaut. Die Frist steht in `SHELF_LIFE`.

⚠ **Ohne Netz passiert nichts Schlimmes.** Liegt ein alter Stand da, wird er
benutzt und sein Alter angezeigt; liegt keiner da, bleibt die Spalte leer.
"""
import time

from . import uex
from .katalog import AUS

SOURCE = 'https://api.uexcorp.uk/2.0/items_prices?uuid=%s'
SOURCE_BY_ID = 'https://api.uexcorp.uk/2.0/items_prices?id_item=%s'
SOURCE_CATEGORIES = 'https://api.uexcorp.uk/2.0/categories'
SOURCE_ITEMS = 'https://api.uexcorp.uk/2.0/items?id_category=%d'
SOURCE_CATEGORY_PRICES = ('https://api.uexcorp.uk/2.0/'
                          'items_prices?id_category=%d')
# ⭐⭐ **Klasse und Güte je Teil.** Genau die Angaben, nach denen UEX auf
# seiner eigenen Seite filtert (Class · Grade · Size) — und genau die, die der
# Watcher bei Bauplänen längst als Kürzel `M/1/A` führt. Ein Abruf je
# Warengruppe, gemessen 392 Zeilen für Kühler in 0,5 s.
SOURCE_ATTRIBUTES = ('https://api.uexcorp.uk/2.0/'
                     'items_attributes?id_category=%d')
CACHE = 'laeden.json'
CATALOG_CACHE = 'laeden-katalog.json'
FORMAT = 1

# ⚠ Eigene Formatnummer für den Katalog. Er hat seit v3.15.0 eine andere
# Struktur (er führt die kaufbaren Teile selbst, seit `3` samt Hersteller und
# Größe); der Preis-Zwischenspeicher daneben ist unverändert und soll deshalb
# nicht mit weggeworfen werden.
FORMAT_CATALOG = 5

# ⚠⚠ **Ein Teil ohne `uuid` wird über seine UEX-Nummer geführt.** Rund ein
# Drittel des Katalogs hat keine Entitäts-Kennung — darunter der Boomtube
# Rocket Launcher, nach dem am 04.09.2026 gefragt wurde. Für die wäre `fetch()`
# ohne diesen Umweg blind. Der Schlüssel `id:123` unterscheidet sich von jeder
# echten Kennung, also bleiben Ablage, Alter und Nachschlagen unverändert.
ID_PREFIX = 'id:'

# ⚠⚠ **Die Kennung trägt nicht überall — gemeldet und nachgemessen 04.09.2026.**
#
# Xharig: „CF-Repeater sind nicht alle in den Läden abrufbar, da sollten aber
# alle Größen kaufbar sein." Stimmt: Von neun CF-Teilen hatten nur zwei einen
# Ladenpreis. Nachgegangen, statt es auf UEX zu schieben:
#
# | Befund | Anzahl |
# |---|---|
# | UEX kennt das Teil gar nicht | 4 |
# | **UEX führt es unter einer ANDEREN Kennung** | **3** |
# | ordentlich zugeordnet | 2 |
#
# Die drei mittleren waren **unser** Fehler. Über alle Baupläne gerechnet:
# über die Kennung 1.167 von 1.599 (73,0 %), mit Namens-Rückfall **1.542
# (96,4 %)** — 375 Teile mehr, davon rund ein Drittel mit echten Kaufpreisen.
#
# ⚠ **Der Rückfall vergleicht den GANZEN Namen, nie einen Teiltext.** Die
# Teiltext-Suche ist die Falle, an der es hier schon einmal schiefging: `Gold`
# liefert `Golden Medmon` mit. Gleichheit hat dieses Problem nicht.
#
# ⚠ **Und die Kennung bleibt zuerst dran.** Bei 6 Teilen zeigen Kennung und
# Name auf **verschiedene** UEX-Einträge — dort gewinnt die Kennung, weil sie
# aus der Spieldatei stammt und der Name nur eine Beschriftung ist.
#
# ⚠ Ein Name, den UEX **mehrfach** führt, wird gar nicht zugeordnet: Eine
# geratene Zuordnung wäre schlimmer als keine.
SECTIONS = ('Systems', 'Vehicle Weapons', 'Utility', 'Personal Weapons',
            'Armor', 'Avionics', 'Undersuits', 'Propulsion')

# ⭐⭐ **Ladenpreise hängen am Patch, nicht an der Uhr.** Sie ändern sich, wenn
# CIG etwas ändert — anders als die Warenpreise im Handel, die täglich
# schwanken. Deshalb `patch_bindet=True` unten und hier nur noch eine
# Notfrist: Sie greift, wenn sich die Spielversion nicht ermitteln lässt.
#
# Am 05.09.2026 angeregt: „Damit die Listen schneller laden — wäre es möglich,
# die als Datenbank beim Spieler abzulegen und nur bei Bedarf zu
# aktualisieren? Schiffspreise, Waffenpreise erneuern sich ja nicht so
# häufig." Die Ablage gab es schon; sie warf ihren Inhalt nur zu oft weg.
SHELF_LIFE = 30 * uex.TAG

# ⚠ Wieviele Gegenstände die Ablage höchstens behält. Ohne Grenze wüchse sie
# mit jedem angesehenen Teil weiter; 400 deckt jeden realistischen Bestand ab
# und bleibt unter 200 KB. Beim Überschreiten fliegt der älteste Eintrag.
MAX_ITEMS = 400

_store = uex.Ablage(CACHE, format_nr=FORMAT, haltbar=SHELF_LIFE,
                    patch_bindet=True)

# Der Warengruppen-Katalog. Eigene Ablage, dieselbe Regel: Er ändert sich mit
# einem Patch, nicht mit dem Tag — und sein Aufbau kostet rund 50 Sekunden.
# Genau der Lauf, der bisher jede Woche umsonst fällig wurde.
_catalog = uex.Ablage(CATALOG_CACHE, format_nr=FORMAT_CATALOG,
                      haltbar=90 * uex.TAG, patch_bindet=True)


def _save_catalog(progress=None):
    """Den Katalog aufbauen: Namen **und** wer überhaupt kaufbar ist.

    ⚠ Das kostet rund 76 Abrufe (zwei je Kategorie) und dauert gemessen etwa
    **70 Sekunden**. Deshalb passiert es **nicht** beim Programmstart, sondern
    höchstens einmal pro Woche — und nur, wenn jemand die Ladenliste öffnet
    oder eine Zuordnung über die Kennung leer ausgeht.

    ⭐⭐ **Warum die zweite Hälfte dazugehört.** Xharig am 04.09.2026: „FPS-
    Waffen, die gar nicht kaufbar sind, machen in der Liste auch keinen Sinn —
    bzw. alles, was nicht kaufbar ist." Er hat recht: Ein Reiter, der zeigt, wo
    ein Teil im Regal steht, darf nicht mit 910 Rüstungsteilen anfangen, von
    denen die meisten nirgends verkauft werden. Man wählt aus, klickt, und
    bekommt „dazu liegen keine Preise vor" — jedes Mal.

    Ein Abruf **je Kategorie** liefert alle Preiszeilen darin auf einmal
    (gemessen: 4.282 Zeilen in sechs Kategorien, davon 710 kaufbare Teile).
    Das ist unvergleichlich billiger, als 1.597 Teile einzeln zu fragen.
    """
    if not _catalog.veraltet():
        return True
    # ⚠⚠ **Erst den Spielstand, dann den Katalog.** Gesichert wird mit dem
    # Stand, der in diesem Augenblick bekannt ist — fehlt er, trägt der
    # Katalog keinen Stempel, und die Patch-Bindung greift beim nächsten Mal
    # nicht. Beim Programmstart läuft derselbe Abruf; wer diese Seite sofort
    # öffnet, ist ihm aber möglicherweise zuvorgekommen.
    try:
        from . import spielstand
        spielstand.aktualisieren()
    except Exception:
        pass
    cats = uex.holen(SOURCE_CATEGORIES, 'shops.categories')
    if not cats:
        return False
    chosen = [k for k in cats if k.get('section') in SECTIONS]
    names = {}
    duplicates = set()
    id_to_uuid = {}
    # ⭐⭐ **Die kaufbaren Teile werden mitgeschrieben, nicht nur gezählt.**
    # Bis v3.14.0 speiste sich die Ladenliste aus den **Bauplänen** — sie
    # zeigte also nur, was man auch herstellen kann. Am 04.09.2026 gefragt:
    # „Wie soll man da wissen, wo es Boomtube-Raketen gibt?" Gar nicht: Der
    # Boomtube Rocket Launcher ist nicht craftbar und stand deshalb nirgends,
    # obwohl UEX seine Läden kennt.
    #
    # Gemessen über die 38 Kategorien unserer Abschnitte: **3.958 Teile,
    # davon 1.528 mit Kaufpreis** — gegenüber 893 craftbaren. Die Abrufe
    # dafür laufen ohnehin schon; bisher wurde das Ergebnis weggeworfen.
    categories = []
    items_out = []
    for number, k in enumerate(chosen, start=1):
        cat_index = len(categories)
        categories.append([k.get('section') or '', k.get('name') or ''])
        items = uex.holen(SOURCE_ITEMS % k['id'], 'shops.catalog')
        raw = {}
        for x in items or []:
            name = (x.get('name') or '').strip()
            ident = x.get('id')
            if not ident:
                continue
            uuid = (x.get('uuid') or '').strip()
            if uuid:
                id_to_uuid[str(ident)] = uuid
            # ⭐ Hersteller und Größe kommen mit — sie sind die zwei weiteren
            # Auswahlmenüs im Laden-Reiter. Gemessen: `company_name` ist bei
            # Geschützen 143 von 154 gefüllt, `size` 150 von 154 (Größe 1–10);
            # bei Rüstung dagegen fast leer, dort fällt das Menü von selbst
            # weg (`_filterleiste` lässt ein Feld ohne Auswahl aus).
            raw[str(ident)] = {
                'n': name,
                'h': (x.get('company_name') or '').strip(),
                'g': str(x.get('size') or '').strip(),
                'c': '',
                'q': '',
            }
            lower = name.lower()
            if not lower:
                continue
            if lower in names and names[lower] != ident:
                duplicates.add(lower)
            names[lower] = ident
        # ⚠ Die Attribute überschreiben die Größe aus der Teileliste: Dort ist
        # sie nur bei 466 von 1.528 gefüllt, hier bei 70 von 73 (Kühler).
        traits = {}
        for x in uex.holen(SOURCE_ATTRIBUTES % k['id'], 'shops.attributes') or []:
            item_no = str(x.get('id_item') or '')
            field = (x.get('attribute_name') or '').strip()
            value = str(x.get('value') or '').strip()
            # ⚠⚠ **Die Güte heißt nicht überall gleich.** Bei Kühlern steht
            # sie als `Grade`, bei Radar als `Grade Letter` (daneben gibt es
            # dort ein `Grade Numeric`). Wer nur auf `Grade` prüft, bekommt
            # für Radar gar keine Güte — gemessen: 182 statt der möglichen
            # Treffer, und das Menü fiel bei Radar ganz weg.
            if field in ('Grade', 'Grade Letter'):
                field = 'Grade'
            if not item_no or not value or field not in ('Size', 'Class',
                                                         'Grade'):
                continue
            traits.setdefault(item_no, {})[field] = value
        for item_no, found in traits.items():
            if item_no not in raw:
                continue
            raw[item_no]['c'] = found.get('Class', '')
            raw[item_no]['q'] = found.get('Grade', '')
            if found.get('Size'):
                raw[item_no]['g'] = found['Size']
        price_rows = uex.holen(SOURCE_CATEGORY_PRICES % k['id'], 'shops.buyable')
        seen = set()
        for x in price_rows or []:
            if (x.get('price_buy') or 0) <= 0:
                continue
            item = str(x.get('id_item') or '')
            if not item:
                continue
            # Ein Teil steht in vielen Terminals — hier zählt es einmal.
            if item in seen or item not in raw:
                continue
            seen.add(item)
            entry = raw[item]
            items_out.append({'n': entry['n'],
                              's': id_to_uuid.get(item) or ID_PREFIX + item,
                              'k': cat_index,
                              'h': entry['h'],
                              'g': entry['g'],
                              'c': entry['c'],
                              'q': entry['q']})
        if progress:
            progress(number, len(chosen))
    # Mehrdeutige Namen fliegen raus — siehe Kopf.
    for name in duplicates:
        names.pop(name, None)
    if not names:
        return False
    items_out.sort(key=lambda x: x['n'].lower())
    return _catalog.sichern({'namen': names,
                             'kategorien': categories,
                             'teile': items_out},
                            kompakt=True)


def catalog_ready():
    """Liegt der Katalog vor? Ohne ihn gibt es keine Ladenliste.

    ⚠ Geprüft wird auf `teile` — die Liste, aus der der Reiter lebt. Ein
    Katalog im alten Format (nur Kennungen, keine Teile) zählt nicht als da;
    er wird über `FORMAT_CATALOG` ohnehin verworfen.
    """
    return bool((_catalog.laden() or {}).get('teile'))


def fetch_catalog(progress=None):
    """Den Katalog von außen anstoßen — für die Ladenliste."""
    return _save_catalog(progress)


# ⚠ **UEX-Name → Sprachschlüssel.** Die Namen kommen englisch aus der Quelle
# und stehen in den Auswahlmenüs — also gehören sie übersetzt. Ein Name, der
# hier fehlt (UEX legt eine Warengruppe nach), bleibt englisch stehen: besser
# ein englisches Wort als ein geratenes deutsches.
#
# ⚠ Zwei Namen kommen doppelt vor — `Personal Weapons` und `Undersuits` sind
# zugleich Bereich **und** Warengruppe. Deshalb zwei Tabellen statt einer.
SECTION_KEYS = {
    'Armor': 's_uk_armor',
    'Avionics': 's_uk_avionics',
    'Personal Weapons': 's_uk_personal_weapons_s',
    'Propulsion': 's_uk_propulsion',
    'Systems': 's_uk_systems',
    'Undersuits': 's_uk_undersuits_s',
    'Utility': 's_uk_utility',
    'Vehicle Weapons': 's_uk_vehicle_weapons_s',
}

GROUP_KEYS = {
    'Arms': 's_uk_arms',
    'Attachments': 's_uk_attachments',
    'Backpacks': 's_uk_backpacks',
    'Batteries': 's_uk_batteries',
    'Bomb Racks': 's_uk_bomb_racks',
    'Bombs': 's_uk_bombs',
    'Container': 's_uk_container',
    'Coolers': 's_uk_coolers',
    'Docking Collars': 's_uk_docking_collars',
    'External Fuel Tanks': 's_uk_external_fuel_tanks',
    'Fabricator': 's_uk_fabricator',
    'Flight Blade': 's_uk_flight_blade',
    'Fuel Nozzle': 's_uk_fuel_nozzle',
    'Full Set': 's_uk_full_set',
    'Gadgets': 's_uk_gadgets',
    'Gravity Generator': 's_uk_gravity_generator',
    'Guns': 's_uk_guns',
    'Helmets': 's_uk_helmets',
    'Jump Modules': 's_uk_jump_modules',
    'Legs': 's_uk_legs',
    'Life Support Generator': 's_uk_life_support_generator',
    'Mining Laser Heads': 's_uk_mining_laser_heads',
    'Mining Modules': 's_uk_mining_modules',
    'Missile Racks': 's_uk_missile_racks',
    'Missiles': 's_uk_missiles',
    'Personal Weapons': 's_uk_personal_weapons',
    'Point Defense Cannon': 's_uk_point_defense_cannon',
    'Power Plants': 's_uk_power_plants',
    'Quantum Drives': 's_uk_quantum_drives',
    'Radar': 's_uk_radar',
    'Salvage Beams': 's_uk_salvage_beams',
    'Scraper Beams': 's_uk_scraper_beams',
    'Shield Generators': 's_uk_shield_generators',
    'Torpedo Tubes': 's_uk_torpedo_tubes',
    'Torso': 's_uk_torso',
    'Tractor Beams': 's_uk_tractor_beams',
    'Turrets': 's_uk_turrets',
    'Undersuits': 's_uk_undersuits',
}


def catalog_items():
    """Alles, was UEX in unseren Abschnitten **verkauft** — oder leere Liste.

    Je Eintrag: `name`, `kennung` (Entitäts-Kennung oder `id:…`),
    `kategorie` und `abschnitt` als englische UEX-Namen. Übersetzt wird erst
    in der Anzeige — hier bleibt stehen, was die Quelle sagt.

    ⚠ **Das ist die Liste für den Laden-Reiter**, nicht `crafting.all_items()`.
    Der Unterschied ist der Zweck: Die Herstellung fragt „was kann ich
    bauen", der Laden „was kann ich kaufen". Das zweite ist die größere
    Menge — und die, nach der jemand sucht, der ein Teil braucht.
    """
    data = _catalog.laden() or {}
    cats = data.get('kategorien') or []
    result = []
    for x in data.get('teile') or []:
        nr = x.get('k')
        pair = cats[nr] if isinstance(nr, int) and 0 <= nr < len(cats) else ['', '']
        result.append({'name': x.get('n') or '',
                       'kennung': x.get('s') or '',
                       'abschnitt': pair[0],
                       'kategorie': pair[1],
                       'hersteller': x.get('h') or '',
                       'groesse': x.get('g') or '',
                       'klasse': x.get('c') or '',
                       'guete': x.get('q') or ''})
    return result




def _uex_id(name):
    """Die UEX-Kennung zu einem Namen — oder `None`. Baut den Katalog bei Bedarf."""
    if not (name or '').strip():
        return None
    table = (_catalog.laden() or {}).get('namen') or {}
    if not table:
        if not _save_catalog():
            return None
        table = (_catalog.laden() or {}).get('namen') or {}
    return table.get(name.strip().lower())


def _all():
    return (_store.laden() or {}).get('teile') or {}


def known(ident):
    """Liegt zu dieser Kennung schon etwas vor? (Auch ein leeres Ergebnis.)"""
    return bool(ident) and ident in _all()


def age(ident):
    """Wie alt der Stand zu diesem Gegenstand ist — oder `None`."""
    entry = _all().get(ident or '')
    if not entry:
        return None
    try:
        return time.time() - float(entry.get('geholt') or 0)
    except (TypeError, ValueError):
        return None


def shops_for(ident):
    """Alle Läden, die dieses Teil führen — teuerster zuletzt.

    Je Eintrag: `laden`, `ort`, `system`, `preis`, `zustand`.
    Leere Liste heißt **„UEX kennt das Teil nicht"**, `None` heißt
    **„noch nicht nachgesehen"**. Der Unterschied gehört in die Anzeige:
    einmal „nirgends im Handel", einmal gar nichts.
    """
    entry = _all().get(ident or '')
    if entry is None:
        return None
    return entry.get('zeilen') or []


def cheapest(ident):
    """Der billigste Laden — `(preis, laden, ort)` oder `None`."""
    rows = shops_for(ident)
    if not rows:
        return None
    best = min(rows, key=lambda z: z['preis'])
    return best['preis'], best.get('laden') or '?', best.get('ort') or ''


def fetch(ident, name='', force=False):
    """Die Ladenpreise zu einem Gegenstand nachschlagen.

    `name` ist der **Rückfall**: Kommt über die Kennung nichts, wird der
    ganze Name im UEX-Katalog gesucht (siehe `SECTIONS` im Kopf). Ohne
    `name` bleibt es beim alten Verhalten.

    Gibt `True` zurück, wenn danach ein Stand vorliegt — auch ein leerer
    („UEX kennt es nicht" ist ein gültiges Ergebnis und wird gemerkt, sonst
    fragt das Werkzeug bei jedem Blick erneut nach).
    """
    if not ident:
        return False
    # ⚠⚠⚠ **Eine Kennung mit Leerzeichen ist keine Kennung.** Am 06.09.2026
    # landete ein Bauplan**name** im Kennungsfeld eines Merkzettel-Postens, und
    # daraus wurde eine kaputte Adresse:
    #
    #     /2.0/items_prices?uuid=CF-447 Rhino Repeater
    #     InvalidURL: URL can't contain control characters
    #
    # Der Abruf scheiterte, es wurde nichts gemerkt — und weil nichts gemerkt
    # war, versuchte es die Seite beim nächsten Blick sofort wieder. „Was noch
    # fehlt" blieb leer und lud endlos: *„der sucht als was und will was
    # laden, hört aber nicht auf."*
    #
    # ⚠ Die Wache steht HIER und nicht nur an der Fundstelle: Sie fängt jede
    # künftige Stelle mit, die versehentlich einen Namen weiterreicht. Ein
    # falscher Aufruf soll gar nicht erst hinausgehen.
    if any(z.isspace() for z in ident) or '"' in ident:
        # ⚠ `fehler` lokal importieren — auf Modulebene wäre es ein
        # Zirkelbezug (`fehler.py` importiert selbst `pfade`). Steht so in den
        # Projektregeln; beim ersten Anlauf stand der Aufruf hier ohne jeden
        # Import und hätte beim ersten Auslösen einen `NameError` geworfen.
        from . import fehler as _f
        _f.merken('shops.fetch',
                  ValueError('keine Kennung, sondern ein Name: %r'
                             % ident[:60]))
        return False
    # ⚠ **Die Netz-Abschaltung steht NACH der Wache.** Eine kaputte Kennung
    # ist kaputt, ob das Netz an ist oder nicht — und im Prüflauf ist es aus.
    # Stand die Prüfung `AUS` davor, lief die Wache dort nie und hätte
    # unbemerkt verrotten können.
    if AUS:
        return False
    a = age(ident)
    if not force and a is not None and a < SHELF_LIFE:
        return True
    # ⚠ Ein Teil ohne Entitäts-Kennung wird über seine UEX-Nummer geholt —
    # siehe `ID_PREFIX`. Ohne diesen Zweig bliebe ein Drittel des Katalogs
    # stumm, darunter jeder Boomtube-Werfer.
    if ident.startswith(ID_PREFIX):
        raw = uex.holen(SOURCE_BY_ID % ident[len(ID_PREFIX):],
                        'shops.id')
    else:
        raw = uex.holen(SOURCE % ident, 'shops')
    if raw is None:
        return False
    # ⚠ Erst wenn die Kennung leer ausgeht, wird der Name bemüht — und auch
    # dann nur der ganze, nie ein Teiltext. Begründung im Kopf des Moduls.
    if not raw and name:
        uex_ident = _uex_id(name)
        if uex_ident:
            by_id = uex.holen(SOURCE_BY_ID % uex_ident, 'shops.name')
            if by_id:
                raw = by_id

    rows = []
    for x in raw:
        price = float(x.get('price_buy') or 0)
        # ⚠ `price_buy = 0` heisst „dieses Terminal verkauft es nicht", nicht
        # „es ist umsonst". Dieselbe Falle wie bei den Ankaufgeboten in
        # `verkauf.py` — einmal vergessen, und im Reiter steht ein Laden mit
        # „0 aUEC" ganz oben, weil er der billigste zu sein scheint.
        if price <= 0:
            continue
        rows.append({
            'laden': (x.get('terminal_name') or '').strip(),
            'ort': (x.get('space_station_name') or x.get('city_name')
                    or x.get('outpost_name') or '').strip(),
            'system': (x.get('star_system_name') or '').strip(),
            'preis': price,
            # 100 = fabrikneu. Gebrauchte Ware ist billiger und weniger wert —
            # ein Preis ohne diese Zahl wäre die halbe Wahrheit.
            'zustand': int(x.get('durability') or 0),
        })
    rows.sort(key=lambda z: z['preis'])

    items = dict(_all())
    items[ident] = {'geholt': time.time(), 'zeilen': rows}
    # ⚠ Älteste zuerst hinaus, wenn es zu viele werden.
    if len(items) > MAX_ITEMS:
        by_age = sorted(items.items(),
                        key=lambda p: p[1].get('geholt') or 0)
        for key, _value in by_age[:len(items) - MAX_ITEMS]:
            items.pop(key, None)
    return _store.sichern({'teile': items}, kompakt=True)


def forget():
    """Alles Nachgeschlagene verwerfen — für den Selbsttest und die Diagnose."""
    _store.sichern({'teile': {}}, kompakt=True)
    _store.vergessen()
