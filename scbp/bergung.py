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
| Was ein Teil im Laden kostet | `laeden.py` → UEX Corp |

Verbunden über die **Entitäts-Kennung** (`ref` bei erkul, `uuid` bei UEX) —
nie über den Namen. Über Namen ist es im Projekt schon zweimal schiefgegangen.

## ⚠ Warum eigene Abrufe statt der Hangar-Ablage

Ein Wrack ist **nicht dein Schiff**. `erkul.plaetze()` beantwortet „passt das in
meines" und hat deshalb nur die Schiffe im Hangar abgelegt; hier geht es um
jedes Schiff, das einem im Verse begegnet. Deshalb wird das gewählte Schiff bei
Bedarf einzeln geholt und getrennt abgelegt.
"""
import json
import os
import time

from . import erkul, fehler, pfade

DATEI = 'bergung.json'
FORMAT = 1

# Wie viele Schiffe hier vorgehalten werden. Wer Bergung spielt, sieht immer
# wieder dieselben Rümpfe — mehr als das braucht niemand, und die Ablage soll
# nicht unbemerkt wachsen.
HOECHSTENS = 40

# Was als Beute zählt. Rumpfpanzerung, Treibstofftanks und Lebenserhaltung
# lassen sich nicht ausbauen und gehören deshalb nicht in eine Liste, die
# „das kannst du mitnehmen" verspricht.
#
# ⚠ Die Namen kommen wörtlich aus erkuls `category`/`type` — nicht übersetzen.
MITNEHMBAR = frozenset((
    'PowerPlant', 'Cooler', 'Shield', 'QuantumDrive', 'Radar', 'JumpDrive',
    'WeaponGun', 'Turret', 'MissileLauncher', 'Missile', 'BombLauncher',
    'Bomb', 'MiningLaser', 'WeaponMining', 'SalvageHead', 'TractorBeam',
    'QuantumInterdictionGenerator', 'EMP', 'MiningModifier',
    'SalvageModifier', 'FlightController',
))


def _ablage():
    return pfade.app_datei(DATEI)


def laden():
    """Die gemerkten Wracks — oder ein leerer Stand."""
    try:
        with open(_ablage(), encoding='utf-8') as f:
            daten = json.load(f)
        if daten.get('format') == FORMAT:
            return daten
    except FileNotFoundError:
        pass
    except Exception as ausnahme:
        fehler.merken('bergung.laden', ausnahme)
    return {'format': FORMAT, 'schiffe': {}}


def _sichern(daten):
    """Atomar ablegen; der Rückgabewert wird ausgewertet."""
    ziel = _ablage()
    try:
        os.makedirs(os.path.dirname(ziel), exist_ok=True)
        with open(ziel + '.tmp', 'w', encoding='utf-8') as f:
            json.dump(daten, f, ensure_ascii=False)
        os.replace(ziel + '.tmp', ziel)
        return True
    except Exception as ausnahme:
        fehler.merken('bergung.sichern', ausnahme)
        return False


def _teile_sammeln(knoten, raus):
    """Alle bestückten Steckplätze eines Schiffs einsammeln, rekursiv.

    ⚠ **Rekursiv, und das ist kein Beiwerk.** Die Waffen sitzen im Turm, der
    Turm am Rumpf; der Bergbaukopf des Prospectors liegt drei Ebenen tief im
    Arm. Wer nur die oberste Ebene liest, findet bei einem bewaffneten Schiff
    keine einzige Waffe — also ausgerechnet das, was etwas wert ist.
    """
    if not isinstance(knoten, list):
        return
    for platz in knoten:
        if not isinstance(platz, dict):
            continue
        teil = platz.get('item')
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
        if isinstance(teil, dict) and teil.get('ref') \
                and platz.get('kind') != 'fixed':
            # ⚠⚠ **`type` vor `category`, und das ist kein Geschmack.**
            # Erkul führt eine Schiffskanone als `category: AssembledWeapon`
            # (die Bauform) und `type: WeaponGun` (die Sache). Wer `category`
            # zuerst nimmt, verliert **jede Waffe** — bei der Cutlass Black
            # waren das vier Repeater und zwei Gatlings, also ausgerechnet das
            # Wertvollste an einem Wrack. Gemessen am 06.09.2026 beim ersten
            # Durchlauf: 24 Stück statt 43.
            art = ''
            for kandidat in (teil.get('type'), teil.get('category')):
                if kandidat in MITNEHMBAR:
                    art = kandidat
                    break
            if art:
                raus.append({
                    'ref': teil['ref'],
                    'name': (teil.get('i18n') or {}).get('name')
                            or teil.get('className') or '?',
                    'art': art,
                    'groesse': teil.get('size'),
                    'guete': teil.get('grade'),
                })
        for feld in ('slots', 'children', 'ports', 'hardpoints'):
            _teile_sammeln(platz.get(feld), raus)
        if isinstance(teil, dict):
            for feld in ('slots', 'ports', 'hardpoints'):
                _teile_sammeln(teil.get(feld), raus)


def werksausstattung(schiff_id, pfad):
    """Was ab Werk in diesem Schiff steckt — Liste mit Anzahl je Teil.

    Holt die Schiffsdatei bei erkul und dampft sie auf das ein, was sich
    ausbauen lässt. Gibt `[]` zurück, wenn nichts zu holen war.
    """
    roh = erkul._holen('%s/%s' % (erkul.ZWEIG, pfad), 'bergung')
    if not isinstance(roh, dict):
        return []
    gefunden = []
    _teile_sammeln(roh.get('slots'), gefunden)

    # Gleiche Teile zusammenfassen — vier Repeater sind eine Zeile mit „4×",
    # nicht vier Zeilen.
    gezaehlt = {}
    for teil in gefunden:
        eintrag = gezaehlt.setdefault(teil['ref'], dict(teil, anzahl=0))
        eintrag['anzahl'] += 1
    raus = list(gezaehlt.values())
    raus.sort(key=lambda x: (x['art'], -(x.get('groesse') or 0), x['name']))
    return raus


def schiff_merken(schiff_id, name, teile):
    """Ein ausgewertetes Schiff ablegen, damit es beim nächsten Mal dasteht."""
    daten = laden()
    schiffe = daten.setdefault('schiffe', {})
    schiffe[schiff_id] = {'name': name, 'teile': teile, 'stand': time.time(),
                          'spielversion': erkul.spielversion()}
    # ⚠ Älteste zuerst weg, nicht willkürlich: Wer ein Schiff gerade
    # nachgeschlagen hat, will es morgen wieder ohne Abruf sehen.
    if len(schiffe) > HOECHSTENS:
        nach_alter = sorted(schiffe.items(), key=lambda p: p[1].get('stand', 0))
        for alt, _ in nach_alter[:len(schiffe) - HOECHSTENS]:
            schiffe.pop(alt, None)
    _sichern(daten)


def gemerkt(schiff_id):
    """Ein früher ausgewertetes Schiff — oder `None`.

    ⚠ Ein Stand aus einer **anderen Spielversion** gilt als nicht vorhanden.
    CIG tauscht mit jedem Patch Bestückungen aus; eine alte Liste sähe richtig
    aus und wäre es nicht.
    """
    eintrag = (laden().get('schiffe') or {}).get(schiff_id)
    if not eintrag:
        return None
    if eintrag.get('spielversion') != erkul.spielversion():
        return None
    return eintrag


def wert(teile, preis_von):
    """Ladenwert der Teile — `(summe, mit_preis, ohne_preis)`.

    `preis_von` ist eine Funktion `kennung -> preis oder None`; sie kommt von
    `laeden.py` und wird hier nur benutzt, nicht nachgebaut.

    ⚠⚠ **Was keinen Preis hat, wird NICHT geschätzt.** Es wird gezählt und
    genannt. Eine Summe, in der drei erfundene Zahlen stecken, sieht genauso
    aus wie eine echte — und wer danach entscheidet, ob er im Feuer aussteigt,
    hat ein Recht darauf zu wissen, wie belastbar sie ist.
    """
    summe = mit = ohne = 0
    for teil in teile:
        preis = preis_von(teil['ref'])
        anzahl = int(teil.get('anzahl') or 1)
        if preis:
            summe += preis * anzahl
            mit += anzahl
        else:
            ohne += anzahl
    return summe, mit, ohne


def vergessen():
    """Alle gemerkten Wracks verwerfen. Gibt die Zahl der Schiffe zurück.

    ⚠⚠ **Dafür gibt es einen Knopf, weil es sonst Handarbeit wäre.** Ohne ihn
    müsste jemand `bergung.json` im Ablage-Ordner suchen und löschen — und wer
    das nicht weiß, sitzt bei einem alten oder falschen Stand fest. Ein
    Zwischenspeicher, den nur der Entwickler leeren kann, ist keiner.

    Der Wunsch kam am 06.09.2026, direkt beim Bau: „denk direkt mit an den
    Reset-Knopf, sonst muss man es per Hand löschen."
    """
    daten = laden()
    anzahl = len(daten.get('schiffe') or {})
    _sichern({'format': FORMAT, 'schiffe': {}})
    return anzahl
