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
Was in einem Wrack steckt — und ob sich das Aussteigen lohnt.

## Die Frage, um die es geht

Vor dir treibt ein Schiff. Aussteigen kostet Zeit und ist gefährlich; der
Laderaum ist begrenzt. **Was ist da drin, und was ist es wert?**

Der Wunsch kam von **Zwaersch (KRT)** am 06.09.2026 — und er ist der Grund,
warum dieses Werkzeug überhaupt an die Schiffsdaten angeschlossen wurde.

## ⚠⚠ Was hier NICHT beantwortet werden kann: der Verkaufserlös

Naheliegend wäre „das bringt dir X aUEC". Diese Zahl gibt es nicht. Gemessen am
06.09.2026 an der Werksausstattung einer Cutlass Black: Von vier geprüften
Teilen hatten **drei überhaupt keinen Verkaufspreis** bei UEX, das vierte einen
einzelnen Ausreißer (414 aUEC gegen 15.103 Kaufpreis). Verkaufspreise für
Schiffskomponenten pflegt dort praktisch niemand.

Was es gibt, ist der **Ladenwert**: was du für dasselbe Teil im Regal bezahlen
müsstest. Für die Bergung ist das ohnehin die ehrlichere Zahl — Komponenten
nimmt man mit, um sie selbst zu fliegen oder weiterzugeben, nicht um sie am
NPC-Terminal abzuwerfen. Und die Rangfolge stimmt: Ein S2-C-Kühler ist weniger
wert als ein S3-A-Repeater, egal welche Zahl daneben steht.

## ⚠⚠ Es gilt für NPC-Wracks — bei Spielerschiffen NICHT

Das ist keine Feinheit, sondern entscheidet, ob die ganze Auskunft etwas wert
ist. Aus dem Spiel, am 06.09.2026:

> NPC-Wracks sind grundsätzlich lootbar, je nach Zustand. Spielerschiffe sind
> meist unbrauchbar — bzw. werden es, sobald der Spieler die Versicherung
> beansprucht. Damit sind auch ausgebaute Teile wertlos. Bei Spielerschiffen
> macht deshalb nur Salvagen Sinn.

⚠ **„Unbrauchbar", nicht „Brikett".** Die erste Fassung übersetzte das
englische „brick" wörtlich. Zwaersch (KRT) dazu am 06.09.2026: *„diese 1-zu-1-
Übersetzung — ich hätte es als unbrauchbar oder unbenutzbar beschrieben."* Wer
die Sache kennt, benennt sie anders als ein Wörterbuch.

Ein Werkzeug, das vor einem Spielerwrack „hier liegen 400.000 aUEC" meldet,
schickt jemanden ins Feuer für nichts. Deshalb steht der Unterschied **auf der
Seite selbst**, nicht nur hier im Quelltext: Die Zahlen gelten für NPC-Wracks;
bei einem Spielerschiff lohnt das Abkratzen der Hülle, nicht das Ausbauen.

⚠ Das Werkzeug kann **nicht erkennen**, was für ein Wrack vor einem treibt —
das steht in keiner Datei, die es liest. Also wird es gesagt, statt geraten.

## ⚠ Und es ist die WERKSausstattung, nicht der Inhalt dieses Wracks

Was erkul liefert, ist die Bestückung ab Werk. Was der Vorbesitzer eingebaut
hat, weiß niemand — schon gar nicht ein Werkzeug, das das Wrack nie gesehen
hat. Die Anzeige sagt deshalb „ab Werk steckt hier … drin", nie „in diesem
Wrack liegt …". Dieselbe Linie wie beim Lager: lieber ein Hinweis als eine
Behauptung.

## Woher die Daten kommen

Zwei Quellen, beide schon im Werkzeug:

| | |
|---|---|
| Werksausstattung je Schiff | `erkul.py` → `cdn.erkul.games` |
| Was ein Teil im Laden kostet | `shops.py` → UEX Corp |

Verbunden über die **Entitäts-Kennung** (`ref` bei erkul, `uuid` bei UEX) —
nie über den Namen. Über Namen ist es im Projekt schon zweimal schiefgegangen.

## ⚠ Warum eigene Abrufe statt der Hangar-Ablage

Ein Wrack ist **nicht dein Schiff**. `erkul.slot_counts()` beantwortet „passt das in
meines" und hat deshalb nur die Schiffe im Hangar abgelegt; hier geht es um
jedes Schiff, das einem im Verse begegnet. Deshalb wird das gewählte Schiff bei
Bedarf einzeln geholt und getrennt abgelegt.

⚠ Bis zum 12.09.2026 hieß dieses Modul `bergung` (Sprachumstellung P4,
Stufe 2). **Nur Bezeichner sind umbenannt, keine Zeichenketten.** Bewusst
gleich geblieben: der Ablage-Name `bergung.json` und die Schlüssel darin
(`format`, `schiffe`, `name`, `teile`, `stand`, `spielversion`), die Schlüssel
der Ergebnisse (`ref`, `art`, `groesse`, `guete`, `anzahl`, `rohstoff`,
`drin`, `zurueck`, `verloren`, `anteil`, `dauer`, `gesperrt`) und die
Bauteil-Arten in `REMOVABLE` — die kommen wörtlich von erkul.
"""
import json
import os
import time

from . import erkul, fehler, pfade

FILE = 'bergung.json'
FORMAT = 1

# Wie viele Schiffe hier vorgehalten werden. Wer Bergung spielt, sieht immer
# wieder dieselben Rümpfe — mehr als das braucht niemand, und die Ablage soll
# nicht unbemerkt wachsen.
MAX_SHIPS = 40

# Was als Beute zählt. Rumpfpanzerung, Treibstofftanks und Lebenserhaltung
# lassen sich nicht ausbauen und gehören deshalb nicht in eine Liste, die
# „das kannst du mitnehmen" verspricht.
#
# ⚠ Die Namen kommen wörtlich aus erkuls `category`/`type` — nicht übersetzen.
REMOVABLE = frozenset((
    'PowerPlant', 'Cooler', 'Shield', 'QuantumDrive', 'Radar', 'JumpDrive',
    'WeaponGun', 'Turret', 'MissileLauncher', 'Missile', 'BombLauncher',
    'Bomb', 'MiningLaser', 'WeaponMining', 'SalvageHead', 'TractorBeam',
    'QuantumInterdictionGenerator', 'EMP', 'MiningModifier',
    'SalvageModifier', 'FlightController',
))


def _store_path():
    return pfade.app_datei(FILE)


def load():
    """Die gemerkten Wracks — oder ein leerer Stand."""
    try:
        with open(_store_path(), encoding='utf-8') as f:
            data = json.load(f)
        if data.get('format') == FORMAT:
            return data
    except FileNotFoundError:
        pass
    except Exception as exc:
        fehler.merken('salvage.load', exc)
    return {'format': FORMAT, 'schiffe': {}}


def _save(data):
    """Atomar ablegen; der Rückgabewert wird ausgewertet."""
    target = _store_path()
    try:
        os.makedirs(os.path.dirname(target), exist_ok=True)
        with open(target + '.tmp', 'w', encoding='utf-8') as f:
            json.dump(data, f, ensure_ascii=False)
        os.replace(target + '.tmp', target)
        return True
    except Exception as exc:
        fehler.merken('salvage.save', exc)
        return False


def _collect_parts(node, result):
    """Alle bestückten Steckplätze eines Schiffs einsammeln, rekursiv.

    ⚠ **Rekursiv, und das ist kein Beiwerk.** Die Waffen sitzen im Turm, der
    Turm am Rumpf; der Bergbaukopf des Prospectors liegt drei Ebenen tief im
    Arm. Wer nur die oberste Ebene liest, findet bei einem bewaffneten Schiff
    keine einzige Waffe — also ausgerechnet das, was etwas wert ist.
    """
    if not isinstance(node, list):
        return
    for slot in node:
        if not isinstance(slot, dict):
            continue
        item = slot.get('item')
        # ⚠⚠ **Festverbautes zählt nicht — es lässt sich nicht ausbauen.**
        # Jeder Steckplatz trägt ein `kind`: `swappable` oder `fixed`.
        # Panzerung und Strukturteile sitzen in `fixed`-Plätzen; wer sie
        # mitrechnet, weist einen Wert aus, den niemand aus dem Wrack
        # herausbekommt — und genau danach entscheidet jemand, ob er im Feuer
        # aussteigt.
        #
        # ⚠ **Trotzdem wird weiter in die Kinder gelaufen** (siehe unten): Ein
        # fester Turm hat tauschbare Waffen darin. Wer bei `fixed` abbricht,
        # verliert die Turmwaffen — also das Wertvollste am Schiff.
        if isinstance(item, dict) and item.get('ref') \
                and slot.get('kind') != 'fixed':
            # ⚠⚠ **`type` vor `category`, und das ist kein Geschmack.**
            # Erkul führt eine Schiffskanone als `category: AssembledWeapon`
            # (die Bauform) und `type: WeaponGun` (die Sache). Wer `category`
            # zuerst nimmt, verliert **jede Waffe** — bei der Cutlass Black
            # waren das vier Repeater und zwei Gatlings, also ausgerechnet das
            # Wertvollste an einem Wrack. Gemessen am 06.09.2026 beim ersten
            # Durchlauf: 24 Stück statt 43.
            kind = ''
            for candidate in (item.get('type'), item.get('category')):
                if candidate in REMOVABLE:
                    kind = candidate
                    break
            if kind:
                result.append({
                    'ref': item['ref'],
                    'name': (item.get('i18n') or {}).get('name')
                            or item.get('className') or '?',
                    'art': kind,
                    'groesse': item.get('size'),
                    'guete': item.get('grade'),
                })
        for field in ('slots', 'children', 'ports', 'hardpoints'):
            _collect_parts(slot.get(field), result)
        if isinstance(item, dict):
            for field in ('slots', 'ports', 'hardpoints'):
                _collect_parts(item.get(field), result)


def factory_loadout(ship_id, path):
    """Was ab Werk in diesem Schiff steckt — Liste mit Anzahl je Teil.

    Holt die Schiffsdatei bei erkul und dampft sie auf das ein, was sich
    ausbauen lässt. Gibt `[]` zurück, wenn nichts zu holen war.
    """
    raw = erkul._fetch('%s/%s' % (erkul.BRANCH, path), 'bergung')
    if not isinstance(raw, dict):
        return []
    found = []
    _collect_parts(raw.get('slots'), found)

    # Gleiche Teile zusammenfassen — vier Repeater sind eine Zeile mit „4×",
    # nicht vier Zeilen.
    counted = {}
    for part in found:
        entry = counted.setdefault(part['ref'], dict(part, anzahl=0))
        entry['anzahl'] += 1
    result = list(counted.values())
    result.sort(key=lambda x: (x['art'], -(x.get('groesse') or 0), x['name']))
    return result


def remember_ship(ship_id, name, parts):
    """Ein ausgewertetes Schiff ablegen, damit es beim nächsten Mal dasteht."""
    data = load()
    ships = data.setdefault('schiffe', {})
    ships[ship_id] = {'name': name, 'teile': parts, 'stand': time.time(),
                      'spielversion': erkul.game_version()}
    # ⚠ Älteste zuerst weg, nicht willkürlich: Wer ein Schiff gerade
    # nachgeschlagen hat, will es morgen wieder ohne Abruf sehen.
    if len(ships) > MAX_SHIPS:
        by_age = sorted(ships.items(), key=lambda p: p[1].get('stand', 0))
        for old, _ in by_age[:len(ships) - MAX_SHIPS]:
            ships.pop(old, None)
    _save(data)


def remembered(ship_id):
    """Ein früher ausgewertetes Schiff — oder `None`.

    ⚠ Ein Stand aus einer **anderen Spielversion** gilt als nicht vorhanden.
    CIG tauscht mit jedem Patch Bestückungen aus; eine alte Liste sähe richtig
    aus und wäre es nicht.
    """
    entry = (load().get('schiffe') or {}).get(ship_id)
    if not entry:
        return None
    if entry.get('spielversion') != erkul.game_version():
        return None
    return entry


def dismantle_rules():
    """Die Regeln des Fabricators — Ausbeute, Dauer und was verloren geht.

    Gibt `{'anteil', 'dauer', 'gesperrt'}` zurück; `gesperrt` ist eine Menge
    kleingeschriebener Rohstoffnamen.

    ⚠ **Alles gemessen, nichts angenommen.** Die Werte stehen in den
    Craftdaten unter `dismantle` und kommen von dort, nicht aus einer Formel:
    Anteil 0,5 · Dauer 15 s · sechs gesperrte Rohstoffe. Ändert CIG das mit
    einem Patch, ändert sich diese Auskunft mit — ohne dass jemand eine Zahl
    im Quelltext nachziehen muss.
    """
    from . import crafting
    raw = (crafting.load() or {}).get('dismantle') or {}
    # ⚠ Die Feldnamen kommen aus den Craftdaten und werden hier NICHT
    # übersetzt: `efficiency`, `dismantleTimeSeconds`, `blacklistedResources`.
    # Wer sie beim Einlesen umbenennt, muss die Umbenennung bei jedem Patch
    # nachziehen.
    blocked = set()
    for entry in raw.get('blacklistedResources') or []:
        name = (entry or {}).get('name') if isinstance(entry, dict) else entry
        if name:
            blocked.add(str(name).strip().lower())
    # ⚠ **Auch die gesperrten Gegenstandsklassen zählen mit** — dort stehen
    # Erze wie „Saldynium (Ore)", die als Rohstoff denselben Namen tragen.
    for entry in raw.get('blacklistedEntityClasses') or []:
        name = (entry or {}).get('name') if isinstance(entry, dict) else entry
        if name:
            blocked.add(str(name).strip().lower())
            # „Saldynium (Ore)" und „Saldynium" sind dasselbe Erz.
            short = str(name).split('(')[0].strip().lower()
            if short:
                blocked.add(short)
    return {'anteil': float(raw.get('efficiency') or 0.5),
            'dauer': int(raw.get('dismantleTimeSeconds') or 0),
            'gesperrt': blocked}


def dismantle(blueprint):
    """Was beim Zerlegen dieses Teils herauskommt.

    Gibt `(zeilen, dauer)` zurück. Je Zeile::

        {'rohstoff', 'drin', 'zurueck', 'verloren'}

    ⭐⭐ **Die Frage eines Bergungsspielers, bevor er ausbaut.** Vorschlag vom
    06.09.2026: *„unter Bergung ein Extra-Fenster, wo man Waffen, Komponenten
    etc. prüfen kann, wie viel Material man rausbekommt, wenn man es im
    Fabricator zerlegt … so kann ein Salvager gleich entscheiden, brauche ich
    die Komponente evtl. zum Zerlegen."*

    ⚠⚠ **Manche Rohstoffe kommen NIE zurück** — und das ist die eigentliche
    Auskunft. Sechs stehen auf der Sperrliste des Fabricators, darunter
    **Quantainium** und **Stileron**. Wer ein Teil nur wegen des Quantainiums
    zerlegt, hat umsonst geschleppt. Ein Rechner, der stumpf die Hälfte
    ausweist, würde genau hier in die Irre führen.

    ⚠ Gerechnet wird über **alle Stufen** des Rezepts: Ein mehrstufiges Teil
    verbraucht auf jeder Stufe Material, und zerlegt wird das fertige Stück.
    """
    from . import crafting

    recipe_ = crafting.recipe(blueprint)
    if not recipe_ or not recipe_.get('stufen'):
        return [], 0

    rules = dismantle_rules()
    demand = {}
    for tier in recipe_.get('stufen') or []:
        for entry in tier.get('zutaten') or []:
            # (Bauteil, Rohstoff, Menge, Güte)
            if len(entry) < 3:
                continue
            name = str(entry[1] or '').strip()
            if not name:
                continue
            try:
                amount = float(entry[2] or 0)
            except (TypeError, ValueError):
                continue
            demand[name] = demand.get(name, 0.0) + amount

    rows = []
    for name in sorted(demand, key=lambda x: x.lower()):
        lost = name.lower() in rules['gesperrt']
        rows.append({
            'rohstoff': name,
            'drin': demand[name],
            'zurueck': 0.0 if lost else demand[name] * rules['anteil'],
            'verloren': lost,
        })
    return rows, rules['dauer']


def value(parts, price_of):
    """Ladenwert der Teile — `(summe, mit_preis, ohne_preis)`.

    `price_of` ist eine Funktion `kennung -> preis oder None`; sie kommt von
    `shops.py` und wird hier nur benutzt, nicht nachgebaut.

    ⚠⚠ **Was keinen Preis hat, wird NICHT geschätzt.** Es wird gezählt und
    genannt. Eine Summe, in der drei erfundene Zahlen stecken, sieht genauso
    aus wie eine echte — und wer danach entscheidet, ob er im Feuer aussteigt,
    hat ein Recht darauf zu wissen, wie belastbar sie ist.
    """
    total = with_price = without_price = 0
    for part in parts:
        price = price_of(part['ref'])
        count = int(part.get('anzahl') or 1)
        if price:
            total += price * count
            with_price += count
        else:
            without_price += count
    return total, with_price, without_price


def forget():
    """Alle gemerkten Wracks verwerfen. Gibt die Zahl der Schiffe zurück.

    ⚠⚠ **Dafür gibt es einen Knopf, weil es sonst Handarbeit wäre.** Ohne ihn
    müsste jemand `bergung.json` im Ablage-Ordner suchen und löschen — und wer
    das nicht weiß, sitzt bei einem alten oder falschen Stand fest. Ein
    Zwischenspeicher, den nur der Entwickler leeren kann, ist keiner.

    Der Wunsch kam am 06.09.2026, direkt beim Bau: „denk direkt mit an den
    Reset-Knopf, sonst muss man es per Hand löschen."
    """
    data = load()
    count = len(data.get('schiffe') or {})
    _save({'format': FORMAT, 'schiffe': {}})
    return count
