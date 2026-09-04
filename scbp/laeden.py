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
CACHE = 'laeden.json'
KATALOG_CACHE = 'laeden-katalog.json'
FORMAT = 1

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
_katalog = uex.Ablage(KATALOG_CACHE, format_nr=FORMAT, haltbar=uex.WOCHE)


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
    # ⚠ Zwei Listen: `kaufbar_id` für die UEX-Kennung, `kaufbar_uuid` für die
    # Entitäts-Kennung aus dem Spiel. Ein Teil kann über den einen Weg bekannt
    # sein und über den anderen nicht — beide Wege müssen antworten können.
    kaufbar_id, kaufbar_uuid = set(), set()
    id_zu_uuid = {}
    for nummer, k in enumerate(gewaehlt, start=1):
        teile = uex.holen(QUELLE_TEILE % k['id'], 'laeden.katalog')
        for x in teile or []:
            name = (x.get('name') or '').strip().lower()
            kennung = x.get('id')
            if not kennung:
                continue
            uuid = (x.get('uuid') or '').strip()
            if uuid:
                id_zu_uuid[str(kennung)] = uuid
            if not name:
                continue
            if name in namen and namen[name] != kennung:
                doppelt.add(name)
            namen[name] = kennung
        preise = uex.holen(QUELLE_PREISE_KATEGORIE % k['id'], 'laeden.kaufbar')
        for x in preise or []:
            if (x.get('price_buy') or 0) <= 0:
                continue
            teil = str(x.get('id_item') or '')
            if teil:
                kaufbar_id.add(teil)
                if teil in id_zu_uuid:
                    kaufbar_uuid.add(id_zu_uuid[teil])
        if fortschritt:
            fortschritt(nummer, len(gewaehlt))
    # Mehrdeutige Namen fliegen raus — siehe Kopf.
    for name in doppelt:
        namen.pop(name, None)
    if not namen:
        return False
    return _katalog.sichern({'namen': namen,
                             'kaufbar_id': sorted(kaufbar_id),
                             'kaufbar_uuid': sorted(kaufbar_uuid)},
                            kompakt=True)


def katalog_da():
    """Liegt der Katalog vor? Ohne ihn lässt sich nicht filtern."""
    return bool((_katalog.laden() or {}).get('kaufbar_uuid') is not None)


def katalog_holen(fortschritt=None):
    """Den Katalog von außen anstoßen — für die Ladenliste."""
    return _katalog_sichern(fortschritt)


def ist_kaufbar(kennung, name=''):
    """Wird dieses Teil irgendwo verkauft? `None` heißt „nicht bekannt".

    ⚠ **Der Unterschied zwischen `False` und `None` ist wichtig.** `False`
    heißt: Wir haben die Preisliste der ganzen Kategorie geholt und das Teil
    kam darin nicht vor — es ist wirklich nirgends im Handel. `None` heißt:
    Wir haben noch nie nachgesehen. Nur beim ersten darf gefiltert werden.
    """
    daten = _katalog.laden() or {}
    ids = daten.get('kaufbar_id')
    uuids = daten.get('kaufbar_uuid')
    if ids is None or uuids is None:
        return None
    if kennung and kennung in set(uuids):
        return True
    if name:
        uex_kennung = (daten.get('namen') or {}).get(name.strip().lower())
        if uex_kennung is not None and str(uex_kennung) in set(ids):
            return True
    return False


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
