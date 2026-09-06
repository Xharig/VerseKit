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
Meine Schiffe — welche ich habe und woher sie kommen.

Ohne diese Liste ist „passt der Bauplan in mein Schiff?" nicht zu beantworten;
das Werkzeug wüsste nur, in *irgendein* Schiff passt es. `schiffe.py` führt
alle Schiffe des Spiels für den Routenplaner — hier geht es um die eigenen.

## ⚠ Die `Game.log` gibt das nicht her — gemessen am 06.09.2026

Naheliegend wäre, den Hangar aus dem Spiel mitzulesen, wie bei den Bauplänen.
Geht nicht. Über 202 Logsicherungen stehen zwar zehntausende Schiffsnamen, aber
es sind die Schiffe **in der Umgebung** — mit Abstand am häufigsten `VNCL_War`
(12.534 Treffer), also Vanduul-Gegner.

Vom eigenen Hangar stehen nur **Zahlen** im Log::

    <VehicleListQuery> … Retrieved 65 entitlements out of 78 vehicules.
    <BuildInventoryStowedAggregateRoots> Found [13] vehicle(s) at location

Keine Namen. Diese Zahlen taugen aber als **Gegenprobe**: Wer 42 Schiffe
eingetragen hat und dessen Spiel von 78 spricht, dem fehlt etwas.

## Zwei Wege hinein — und beide werden gebraucht

| Weg | bringt | bringt **nicht** |
|---|---|---|
| Import aus **Star Citizen Hangar XPLORer** | alle Echtgeld-Pledges samt LTI und Paketname | im Spiel gekaufte Schiffe |
| **Von Hand** eintragen | alles Übrige | — |

Die Erweiterung (`github.com/dolkensp/HangarXPLOR`) setzt auf der Pledge-Seite
zwei Knöpfe *Download CSV* und *Download JSON*. Beide Formate werden gelesen.

⚠ **Der Export kennt nur Gekauftes.** Wer sich im Spiel eine Cutlass erflogen
hat, findet sie dort nie — deshalb ist der Handeintrag kein Notbehelf, sondern
gleichberechtigt. Jedes Schiff trägt seine `herkunft`.

## ⚠ Die Kaufsumme bleibt im Haus

`pledge_cost` und `pledge_id` sind private Angaben. Sie werden abgelegt, weil
sie dem Spieler gehören und er sie sehen will — aber sie dürfen **nie** in den
Fehlerbericht geraten. Dieselbe Linie wie `pfade.kuerzen()` bei den Pfaden.

## Aufbau der Datei (`hangar.json` im eigenen Ordner)

    {"format": 1,
     "schiffe": [
       {"name": "Cutlass Black", "hersteller": "Drake Interplanetary",
        "herkunft": "pledge", "lti": true, "paket": "Standalone Ship",
        "gekauft": "May 18, 2026", "preis": "$120.00 USD",
        "belegung": {}}]}

⚠ **`belegung` bleibt vorerst leer.** Der Platz für eine gespeicherte Auslegung
(welches Teil in welchem Steckplatz) ist mit Absicht schon da: Kommt sie dazu,
soll das keinen Formatwechsel kosten und keine bestehende Datei entwerten.
"""
import csv
import io
import json
import os
import re

from . import erkul, fehler, pfade

DATEI = 'hangar.json'
FORMAT = 1

# Woher ein Schiff stammt. Mehr Fälle gibt es nicht — geliehene Schiffe stehen
# nicht im Hangar, und geschenkte sind aus Sicht des Spielers Pledges.
PLEDGE = 'pledge'
INGAME = 'ingame'


def pfad():
    return pfade.app_datei(DATEI)


def leer():
    return {'format': FORMAT, 'schiffe': []}


def laden():
    """Der eigene Hangar — oder eine leere Liste."""
    try:
        with open(pfad(), encoding='utf-8') as f:
            daten = json.load(f)
        if daten.get('format') == FORMAT and isinstance(daten.get('schiffe'), list):
            return daten
    except FileNotFoundError:
        pass
    except Exception as ausnahme:
        fehler.merken('hangar.laden', ausnahme)
    return leer()


def speichern(daten):
    """Atomar ablegen. Gibt zurück, ob es geklappt hat.

    ⚠ Der Rückgabewert wird ausgewertet — ein stilles `False` wäre genau der
    Fehler, der bei `pfade.einstellungen_schreiben` monatelang dafür sorgte,
    dass eine nicht gespeicherte Einstellung nach dem Neustart wieder alt war.
    """
    ziel = pfad()
    try:
        os.makedirs(os.path.dirname(ziel), exist_ok=True)
        with open(ziel + '.tmp', 'w', encoding='utf-8') as f:
            json.dump(daten, f, ensure_ascii=False, indent=1)
        os.replace(ziel + '.tmp', ziel)
        return True
    except Exception as ausnahme:
        fehler.merken('hangar.speichern', ausnahme)
        return False


def _schlank(text):
    return re.sub(r'[^a-z0-9]', '', (text or '').lower())


def anzahl(daten=None):
    return len((daten or laden()).get('schiffe') or [])


def namen(daten=None):
    """Alle Schiffsnamen im Hangar, alphabetisch."""
    liste = (daten or laden()).get('schiffe') or []
    return sorted((s.get('name') or '' for s in liste if s.get('name')),
                  key=str.lower)


def kennsaetze(daten=None):
    """Je Schiff `(name, hersteller, kurz)` — was `erkul` zum Zuordnen braucht.

    ⚠ Der Kurzname aus dem Export trägt die meisten Treffer; ohne ihn fällt die
    Zuordnung von 34 auf 1 zurück. Deshalb wird er durchgereicht und nicht nur
    der Klartextname.
    """
    return [(s.get('name') or '', s.get('hersteller') or '',
             s.get('kurz') or '', s.get('hkurz') or '')
            for s in ((daten or laden()).get('schiffe') or []) if s.get('name')]


# ------------------------------------------------------------ Wunschliste

def wunsch_liste(daten=None):
    """Die Schiffe, die man sich vorgenommen hat — alphabetisch.

    ⚠ **Getrennt vom Hangar, in derselben Datei.** Ein Wunsch ist kein Besitz:
    Was hier steht, darf nirgends in „passt in dein Schiff" auftauchen, sonst
    beantwortet das Werkzeug eine Frage über ein Schiff, das der Spieler gar
    nicht hat. Ein fehlendes Feld gilt als leere Liste — alte Dateien bleiben
    damit gültig, es braucht keinen Formatwechsel.

    Der Vorschlag kam von **Zwaersch (KRT)** am 06.09.2026: *„Also Unterpunkt
    könnte man noch ne Wishlist-Option anbieten. Für, ich nenn's mal allgemein
    Vehikel, die man sich erspielen/kaufen möchte."*
    """
    liste = (daten or laden()).get('wunsch') or []
    return sorted((w for w in liste if isinstance(w, dict) and w.get('name')),
                  key=lambda w: (w.get('name') or '').lower())


def wunsch_enthaelt(daten, name):
    suche = _schlank(name)
    return any(_schlank(w.get('name')) == suche
               for w in (daten.get('wunsch') or []))


def wunsch_hinzufuegen(daten, name, hersteller=''):
    """Ein Schiff auf die Wunschliste setzen. Gibt zurück, ob es neu war.

    ⚠ Was schon im Hangar steht, kommt **nicht** auf die Wunschliste — man
    wünscht sich nichts, das man hat. Die Anzeige sagt das auch, statt den
    Eintrag stillschweigend zu schlucken.
    """
    if not (name or '').strip():
        return False
    if wunsch_enthaelt(daten, name):
        return False
    daten.setdefault('wunsch', []).append(
        {'name': name.strip(), 'hersteller': (hersteller or '').strip()})
    return True


def wunsch_entfernen(daten, name):
    """Einen Wunsch streichen. Gibt zurück, ob einer wegfiel."""
    suche = _schlank(name)
    vorher = len(daten.get('wunsch') or [])
    daten['wunsch'] = [w for w in (daten.get('wunsch') or [])
                       if _schlank(w.get('name')) != suche]
    return len(daten['wunsch']) != vorher


def enthaelt(daten, name, hersteller=''):
    """Steht dieses Schiff schon drin?

    ⚠ Verglichen wird über Hersteller **und** Name in schlanker Schreibweise.
    „Cutlass Black" von Drake und eine gleichnamige Variante eines anderen
    Herstellers wären sonst dasselbe.
    """
    suche = _schlank(hersteller) + _schlank(name)
    for s in (daten.get('schiffe') or []):
        if _schlank(s.get('hersteller')) + _schlank(s.get('name')) == suche:
            return True
    return False


def hinzufuegen(daten, name, hersteller='', herkunft=INGAME, **rest):
    """Ein Schiff eintragen. Gibt zurück, ob es neu war.

    Doppelte werden still übergangen — wer zweimal importiert, soll nicht jedes
    Schiff doppelt im Hangar stehen haben.
    """
    if not (name or '').strip():
        return False
    if enthaelt(daten, name, hersteller):
        return False
    eintrag = {'name': name.strip(), 'hersteller': (hersteller or '').strip(),
               'herkunft': herkunft, 'belegung': {}}
    for schluessel in ('kurz', 'hkurz', 'lti', 'warbond', 'paket', 'gekauft',
                       'preis'):
        if rest.get(schluessel) not in (None, ''):
            eintrag[schluessel] = rest[schluessel]
    daten.setdefault('schiffe', []).append(eintrag)
    return True


def entfernen(daten, name, hersteller=''):
    """Ein Schiff austragen. Gibt zurück, ob eines wegfiel."""
    suche = _schlank(hersteller) + _schlank(name)
    vorher = len(daten.get('schiffe') or [])
    daten['schiffe'] = [
        s for s in (daten.get('schiffe') or [])
        if _schlank(s.get('hersteller')) + _schlank(s.get('name')) != suche]
    return len(daten['schiffe']) != vorher


# ---------------------------------------------------------------- Import

def _aus_json(text):
    """Der JSON-Export von Hangar XPLORer."""
    roh = json.loads(text)
    if not isinstance(roh, list):
        return []
    raus = []
    for eintrag in roh:
        if not isinstance(eintrag, dict):
            continue
        # ⚠ Nur Schiffe. Der Export führt auch Ausrüstung, Farben und Anzüge —
        # ein „Bosco Weapon Display Rack" hat keine Steckplätze.
        if eintrag.get('entity_type') not in (None, '', 'ship'):
            continue
        # ⚠⚠ **`name` vor `ship_name`** — und das ist kein Geschmack.
        # `ship_name` ist der Grundtyp, `name` die tatsächliche Ausführung.
        # Andersherum wurden aus „A.T.L.S." und „ATLS GEO" zwei Einträge
        # desselben Namens, von denen der zweite als Doppel wegfiel; und die
        # „F7C-M Super Hornet Mk II" hieß Mk I, weil ihr `ship_code` noch auf
        # der alten Ausführung steht.
        name = (eintrag.get('name') or eintrag.get('ship_name') or '').strip()
        if not name:
            continue
        raus.append({
            'name': name,
            'hersteller': (eintrag.get('manufacturer_name') or '').strip(),
            'kurz': (eintrag.get('ship_code') or '').strip(),
            # ⚠ Das Herstellerkürzel ist für die Zuordnung wichtiger als der
            # ausgeschriebene Name: Erkul führt „Roberts Space Industries" als
            # `rsi`. Ohne dieses Feld fand die Ursa Medivac keinen Anschluss.
            'hkurz': (eintrag.get('manufacturer_code') or '').strip(),
            'lti': bool(eintrag.get('lti')),
            'warbond': bool(eintrag.get('warbond')),
            'paket': (eintrag.get('pledge_name') or '').strip(),
            'gekauft': (eintrag.get('pledge_date') or '').strip(),
            'preis': (eintrag.get('pledge_cost') or '').strip(),
        })
    return raus


def _aus_csv(text):
    """Der CSV-Export von Hangar XPLORer.

    ⚠ Die Kopfzeile trägt **Leerzeichen hinter den Kommas** (`Manufacturer,
    Ship, Lti, …`). Ohne `skipinitialspace` heißt die zweite Spalte `' Ship'`
    und wird nie gefunden.
    """
    raus = []
    for zeile in csv.DictReader(io.StringIO(text), skipinitialspace=True):
        name = (zeile.get('Ship') or '').strip()
        # ⚠ Der Export schreibt bei unbekannten Stücken wörtlich `undefined`
        # in die Namensspalte — das ist kein Schiff, sondern eine Lücke.
        if not name or name == 'undefined':
            continue
        raus.append({
            'name': name,
            'hersteller': (zeile.get('Manufacturer') or '').strip(),
            'kurz': '',
            'hkurz': '',
            'lti': (zeile.get('Lti') or '').strip().lower() == 'true',
            'warbond': (zeile.get('Warbond') or '').strip().lower() == 'true',
            'paket': (zeile.get('Pledge') or '').strip(),
            'gekauft': (zeile.get('Date') or '').strip(),
            'preis': (zeile.get('Cost') or '').strip(),
        })
    return raus


def lesen(dateipfad):
    """Eine Exportdatei einlesen. Gibt `(eintraege, fehlertext)` zurück.

    Erkannt wird am Inhalt, nicht an der Endung — wer eine `.json` in `.txt`
    umbenennt, soll trotzdem weiterkommen.
    """
    try:
        with open(dateipfad, encoding='utf-8-sig') as f:
            text = f.read()
    except Exception as ausnahme:
        fehler.merken('hangar.lesen', ausnahme)
        return [], str(ausnahme)
    kopf = text.lstrip()[:1]
    try:
        eintraege = _aus_json(text) if kopf == '[' else _aus_csv(text)
    except Exception as ausnahme:
        fehler.merken('hangar.lesen.deuten', ausnahme)
        return [], str(ausnahme)
    return eintraege, ''


def uebernehmen(eintraege, daten=None, sichern=True):
    """Gelesene Einträge in den Hangar übernehmen.

    Gibt `(neu, schon_da)` zurück.
    """
    daten = daten if daten is not None else laden()
    neu = 0
    for e in eintraege:
        if hinzufuegen(daten, e.get('name'), e.get('hersteller'),
                       herkunft=PLEDGE, kurz=e.get('kurz'),
                       hkurz=e.get('hkurz'), lti=e.get('lti'),
                       warbond=e.get('warbond'), paket=e.get('paket'),
                       gekauft=e.get('gekauft'), preis=e.get('preis')):
            neu += 1
    if sichern:
        speichern(daten)
    return neu, len(eintraege) - neu


def unbekannt(daten=None):
    """Welche Schiffe im Hangar erkul **nicht** kennt.

    ⚠ Das ist meistens **keine Panne**, sondern eine Auskunft: Erkul führt nur
    Schiffe, die im Spiel flugfähig sind. Ein Treffer hier heißt in aller Regel
    „gibt es noch nicht" — bei einem echten Hangar-Export vom 06.09.2026 waren
    das Crucible, Endeavor, Galaxy, Liberator, Merchantman und die beiden ATLS.
    """
    raus = []
    for s in ((daten or laden()).get('schiffe') or []):
        if not erkul.kennt(s.get('name'), s.get('hersteller'), s.get('kurz'),
                           s.get('hkurz')):
            raus.append(s.get('name') or '')
    return raus


def daten_nachziehen(daten=None):
    """Die Steckplätze aller Hangar-Schiffe holen, soweit sie fehlen.

    Gibt zurück, wie viele Schiffe neu geholt wurden.
    """
    return erkul.nachtragen(kennsaetze(daten))
