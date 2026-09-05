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

Der Watcher kennt das **Rezept** (`herstellung.py`) und die **Rohstoffpreise**
(`preise.py`). Daraus ergibt sich, was Selberbauen kostet. Fehlte bisher die
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
zehnmal auf ihn schaut. Die Frist steht in `HALTBAR`.

⚠ **Ohne Netz passiert nichts Schlimmes.** Liegt ein alter Stand da, wird er
benutzt und sein Alter angezeigt; liegt keiner da, bleibt die Spalte leer.
"""
import time

from . import uex
from .katalog import AUS

QUELLE = 'https://api.uexcorp.uk/2.0/items_prices?uuid=%s'
QUELLE_UEBER_ID = 'https://api.uexcorp.uk/2.0/items_prices?id_item=%s'
QUELLE_KATEGORIEN = 'https://api.uexcorp.uk/2.0/categories'
QUELLE_TEILE = 'https://api.uexcorp.uk/2.0/items?id_category=%d'
QUELLE_PREISE_KATEGORIE = ('https://api.uexcorp.uk/2.0/'
                           'items_prices?id_category=%d')
# ⭐⭐ **Klasse und Güte je Teil.** Genau die Angaben, nach denen UEX auf
# seiner eigenen Seite filtert (Class · Grade · Size) — und genau die, die der
# Watcher bei Bauplänen längst als Kürzel `M/1/A` führt. Ein Abruf je
# Warengruppe, gemessen 392 Zeilen für Kühler in 0,5 s.
QUELLE_ATTRIBUTE = ('https://api.uexcorp.uk/2.0/'
                    'items_attributes?id_category=%d')
CACHE = 'laeden.json'
KATALOG_CACHE = 'laeden-katalog.json'
FORMAT = 1

# ⚠ Eigene Formatnummer für den Katalog. Er hat seit v3.15.0 eine andere
# Struktur (er führt die kaufbaren Teile selbst, seit `3` samt Hersteller und
# Größe); der Preis-Zwischenspeicher daneben ist unverändert und soll deshalb
# nicht mit weggeworfen werden.
FORMAT_KATALOG = 4

# ⚠⚠ **Ein Teil ohne `uuid` wird über seine UEX-Nummer geführt.** Rund ein
# Drittel des Katalogs hat keine Entitäts-Kennung — darunter der Boomtube
# Rocket Launcher, nach dem am 04.09.2026 gefragt wurde. Für die wäre `holen()`
# ohne diesen Umweg blind. Der Schlüssel `id:123` unterscheidet sich von jeder
# echten Kennung, also bleiben Ablage, Alter und Nachschlagen unverändert.
ID_PRAEFIX = 'id:'

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
ABSCHNITTE = ('Systems', 'Vehicle Weapons', 'Utility', 'Personal Weapons',
              'Armor', 'Avionics', 'Undersuits', 'Propulsion')

# Ein Tag, wie bei den Rohstoffpreisen. Ladenpreise sind stabiler als
# Warenpreise — sie ändern sich mit dem Patch, nicht mit der Tageszeit.
HALTBAR = uex.TAG

# ⚠ Wieviele Gegenstände die Ablage höchstens behält. Ohne Grenze wüchse sie
# mit jedem angesehenen Teil weiter; 400 deckt jeden realistischen Bestand ab
# und bleibt unter 200 KB. Beim Überschreiten fliegt der älteste Eintrag.
HOECHSTENS = 400

_ablage = uex.Ablage(CACHE, format_nr=FORMAT, haltbar=HALTBAR)

# Der Namens-Katalog. Eigene Ablage, eigene Frist: Er ändert sich mit einem
# Patch, nicht mit dem Tag.
_katalog = uex.Ablage(KATALOG_CACHE, format_nr=FORMAT_KATALOG,
                      haltbar=uex.WOCHE)


def _katalog_sichern(fortschritt=None):
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
    if not _katalog.veraltet():
        return True
    kats = uex.holen(QUELLE_KATEGORIEN, 'laeden.kategorien')
    if not kats:
        return False
    gewaehlt = [k for k in kats if k.get('section') in ABSCHNITTE]
    namen = {}
    doppelt = set()
    id_zu_uuid = {}
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
    kategorien = []
    teile_raus = []
    for nummer, k in enumerate(gewaehlt, start=1):
        kat_index = len(kategorien)
        kategorien.append([k.get('section') or '', k.get('name') or ''])
        teile = uex.holen(QUELLE_TEILE % k['id'], 'laeden.katalog')
        roh = {}
        for x in teile or []:
            name = (x.get('name') or '').strip()
            kennung = x.get('id')
            if not kennung:
                continue
            uuid = (x.get('uuid') or '').strip()
            if uuid:
                id_zu_uuid[str(kennung)] = uuid
            # ⭐ Hersteller und Größe kommen mit — sie sind die zwei weiteren
            # Auswahlmenüs im Laden-Reiter. Gemessen: `company_name` ist bei
            # Geschützen 143 von 154 gefüllt, `size` 150 von 154 (Größe 1–10);
            # bei Rüstung dagegen fast leer, dort fällt das Menü von selbst
            # weg (`_filterleiste` lässt ein Feld ohne Auswahl aus).
            roh[str(kennung)] = {
                'n': name,
                'h': (x.get('company_name') or '').strip(),
                'g': str(x.get('size') or '').strip(),
                'c': '',
                'q': '',
            }
            klein = name.lower()
            if not klein:
                continue
            if klein in namen and namen[klein] != kennung:
                doppelt.add(klein)
            namen[klein] = kennung
        # ⚠ Die Attribute überschreiben die Größe aus der Teileliste: Dort ist
        # sie nur bei 466 von 1.528 gefüllt, hier bei 70 von 73 (Kühler).
        merkmale = {}
        for x in uex.holen(QUELLE_ATTRIBUTE % k['id'], 'laeden.attribute') or []:
            teil_nr = str(x.get('id_item') or '')
            feld = (x.get('attribute_name') or '').strip()
            wert = str(x.get('value') or '').strip()
            # ⚠⚠ **Die Güte heißt nicht überall gleich.** Bei Kühlern steht
            # sie als `Grade`, bei Radar als `Grade Letter` (daneben gibt es
            # dort ein `Grade Numeric`). Wer nur auf `Grade` prüft, bekommt
            # für Radar gar keine Güte — gemessen: 182 statt der möglichen
            # Treffer, und das Menü fiel bei Radar ganz weg.
            if feld in ('Grade', 'Grade Letter'):
                feld = 'Grade'
            if not teil_nr or not wert or feld not in ('Size', 'Class',
                                                       'Grade'):
                continue
            merkmale.setdefault(teil_nr, {})[feld] = wert
        for teil_nr, gefunden in merkmale.items():
            if teil_nr not in roh:
                continue
            roh[teil_nr]['c'] = gefunden.get('Class', '')
            roh[teil_nr]['q'] = gefunden.get('Grade', '')
            if gefunden.get('Size'):
                roh[teil_nr]['g'] = gefunden['Size']
        preise = uex.holen(QUELLE_PREISE_KATEGORIE % k['id'], 'laeden.kaufbar')
        gesehen = set()
        for x in preise or []:
            if (x.get('price_buy') or 0) <= 0:
                continue
            teil = str(x.get('id_item') or '')
            if not teil:
                continue
            # Ein Teil steht in vielen Terminals — hier zählt es einmal.
            if teil in gesehen or teil not in roh:
                continue
            gesehen.add(teil)
            eintrag = roh[teil]
            teile_raus.append({'n': eintrag['n'],
                               's': id_zu_uuid.get(teil) or ID_PRAEFIX + teil,
                               'k': kat_index,
                               'h': eintrag['h'],
                               'g': eintrag['g'],
                               'c': eintrag['c'],
                               'q': eintrag['q']})
        if fortschritt:
            fortschritt(nummer, len(gewaehlt))
    # Mehrdeutige Namen fliegen raus — siehe Kopf.
    for name in doppelt:
        namen.pop(name, None)
    if not namen:
        return False
    teile_raus.sort(key=lambda x: x['n'].lower())
    return _katalog.sichern({'namen': namen,
                             'kategorien': kategorien,
                             'teile': teile_raus},
                            kompakt=True)


def katalog_da():
    """Liegt der Katalog vor? Ohne ihn gibt es keine Ladenliste.

    ⚠ Geprüft wird auf `teile` — die Liste, aus der der Reiter lebt. Ein
    Katalog im alten Format (nur Kennungen, keine Teile) zählt nicht als da;
    er wird über `FORMAT_KATALOG` ohnehin verworfen.
    """
    return bool((_katalog.laden() or {}).get('teile'))


def katalog_holen(fortschritt=None):
    """Den Katalog von außen anstoßen — für die Ladenliste."""
    return _katalog_sichern(fortschritt)


# ⚠ **UEX-Name → Sprachschlüssel.** Die Namen kommen englisch aus der Quelle
# und stehen in den Auswahlmenüs — also gehören sie übersetzt. Ein Name, der
# hier fehlt (UEX legt eine Warengruppe nach), bleibt englisch stehen: besser
# ein englisches Wort als ein geratenes deutsches.
#
# ⚠ Zwei Namen kommen doppelt vor — `Personal Weapons` und `Undersuits` sind
# zugleich Bereich **und** Warengruppe. Deshalb zwei Tabellen statt einer.
BEREICH_TEXTE = {
    'Armor': 's_uk_armor',
    'Avionics': 's_uk_avionics',
    'Personal Weapons': 's_uk_personal_weapons_s',
    'Propulsion': 's_uk_propulsion',
    'Systems': 's_uk_systems',
    'Undersuits': 's_uk_undersuits_s',
    'Utility': 's_uk_utility',
    'Vehicle Weapons': 's_uk_vehicle_weapons_s',
}

GRUPPE_TEXTE = {
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


def katalog_teile():
    """Alles, was UEX in unseren Abschnitten **verkauft** — oder leere Liste.

    Je Eintrag: `name`, `kennung` (Entitäts-Kennung oder `id:…`),
    `kategorie` und `abschnitt` als englische UEX-Namen. Übersetzt wird erst
    in der Anzeige — hier bleibt stehen, was die Quelle sagt.

    ⚠ **Das ist die Liste für den Laden-Reiter**, nicht `herstellung.alle()`.
    Der Unterschied ist der Zweck: Die Herstellung fragt „was kann ich
    bauen", der Laden „was kann ich kaufen". Das zweite ist die größere
    Menge — und die, nach der jemand sucht, der ein Teil braucht.
    """
    daten = _katalog.laden() or {}
    kats = daten.get('kategorien') or []
    raus = []
    for x in daten.get('teile') or []:
        nr = x.get('k')
        paar = kats[nr] if isinstance(nr, int) and 0 <= nr < len(kats) else ['', '']
        raus.append({'name': x.get('n') or '',
                     'kennung': x.get('s') or '',
                     'abschnitt': paar[0],
                     'kategorie': paar[1],
                     'hersteller': x.get('h') or '',
                     'groesse': x.get('g') or '',
                     'klasse': x.get('c') or '',
                     'guete': x.get('q') or ''})
    return raus




def _uex_id(name):
    """Die UEX-Kennung zu einem Namen — oder `None`. Baut den Katalog bei Bedarf."""
    if not (name or '').strip():
        return None
    tabelle = (_katalog.laden() or {}).get('namen') or {}
    if not tabelle:
        if not _katalog_sichern():
            return None
        tabelle = (_katalog.laden() or {}).get('namen') or {}
    return tabelle.get(name.strip().lower())


def _alle():
    return (_ablage.laden() or {}).get('teile') or {}


def bekannt(kennung):
    """Liegt zu dieser Kennung schon etwas vor? (Auch ein leeres Ergebnis.)"""
    return bool(kennung) and kennung in _alle()


def alter(kennung):
    """Wie alt der Stand zu diesem Gegenstand ist — oder `None`."""
    eintrag = _alle().get(kennung or '')
    if not eintrag:
        return None
    try:
        return time.time() - float(eintrag.get('geholt') or 0)
    except (TypeError, ValueError):
        return None


def laeden(kennung):
    """Alle Läden, die dieses Teil führen — teuerster zuletzt.

    Je Eintrag: `laden`, `ort`, `system`, `preis`, `zustand`.
    Leere Liste heißt **„UEX kennt das Teil nicht"**, `None` heißt
    **„noch nicht nachgesehen"**. Der Unterschied gehört in die Anzeige:
    einmal „nirgends im Handel", einmal gar nichts.
    """
    eintrag = _alle().get(kennung or '')
    if eintrag is None:
        return None
    return eintrag.get('zeilen') or []


def guenstigster(kennung):
    """Der billigste Laden — `(preis, laden, ort)` oder `None`."""
    liste = laeden(kennung)
    if not liste:
        return None
    bester = min(liste, key=lambda z: z['preis'])
    return bester['preis'], bester.get('laden') or '?', bester.get('ort') or ''


def holen(kennung, name='', erzwingen=False):
    """Die Ladenpreise zu einem Gegenstand nachschlagen.

    `name` ist der **Rückfall**: Kommt über die Kennung nichts, wird der
    ganze Name im UEX-Katalog gesucht (siehe `ABSCHNITTE` im Kopf). Ohne
    `name` bleibt es beim alten Verhalten.

    Gibt `True` zurück, wenn danach ein Stand vorliegt — auch ein leerer
    („UEX kennt es nicht" ist ein gültiges Ergebnis und wird gemerkt, sonst
    fragt das Werkzeug bei jedem Blick erneut nach).
    """
    if AUS or not kennung:
        return False
    a = alter(kennung)
    if not erzwingen and a is not None and a < HALTBAR:
        return True
    # ⚠ Ein Teil ohne Entitäts-Kennung wird über seine UEX-Nummer geholt —
    # siehe `ID_PRAEFIX`. Ohne diesen Zweig bliebe ein Drittel des Katalogs
    # stumm, darunter jeder Boomtube-Werfer.
    if kennung.startswith(ID_PRAEFIX):
        roh = uex.holen(QUELLE_UEBER_ID % kennung[len(ID_PRAEFIX):],
                        'laeden.id')
    else:
        roh = uex.holen(QUELLE % kennung, 'laeden')
    if roh is None:
        return False
    # ⚠ Erst wenn die Kennung leer ausgeht, wird der Name bemüht — und auch
    # dann nur der ganze, nie ein Teiltext. Begründung im Kopf des Moduls.
    if not roh and name:
        uex_kennung = _uex_id(name)
        if uex_kennung:
            ueber_id = uex.holen(QUELLE_UEBER_ID % uex_kennung, 'laeden.name')
            if ueber_id:
                roh = ueber_id

    zeilen = []
    for x in roh:
        preis = float(x.get('price_buy') or 0)
        # ⚠ `price_buy = 0` heisst „dieses Terminal verkauft es nicht", nicht
        # „es ist umsonst". Dieselbe Falle wie bei den Ankaufgeboten in
        # `verkauf.py` — einmal vergessen, und im Reiter steht ein Laden mit
        # „0 aUEC" ganz oben, weil er der billigste zu sein scheint.
        if preis <= 0:
            continue
        zeilen.append({
            'laden': (x.get('terminal_name') or '').strip(),
            'ort': (x.get('space_station_name') or x.get('city_name')
                    or x.get('outpost_name') or '').strip(),
            'system': (x.get('star_system_name') or '').strip(),
            'preis': preis,
            # 100 = fabrikneu. Gebrauchte Ware ist billiger und weniger wert —
            # ein Preis ohne diese Zahl wäre die halbe Wahrheit.
            'zustand': int(x.get('durability') or 0),
        })
    zeilen.sort(key=lambda z: z['preis'])

    teile = dict(_alle())
    teile[kennung] = {'geholt': time.time(), 'zeilen': zeilen}
    # ⚠ Älteste zuerst hinaus, wenn es zu viele werden.
    if len(teile) > HOECHSTENS:
        nach_alter = sorted(teile.items(),
                            key=lambda p: p[1].get('geholt') or 0)
        for schluessel, _wert in nach_alter[:len(teile) - HOECHSTENS]:
            teile.pop(schluessel, None)
    return _ablage.sichern({'teile': teile}, kompakt=True)


def vergessen():
    """Alles Nachgeschlagene verwerfen — für den Selbsttest und die Diagnose."""
    _ablage.sichern({'teile': {}}, kompakt=True)
    _ablage.vergessen()
