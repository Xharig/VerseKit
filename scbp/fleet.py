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
das Werkzeug wüsste nur, in *irgendein* Schiff passt es. `ships.py` führt
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

⚠ Bis zum 12.09.2026 hieß dieses Modul `hangar` (Sprachumstellung P4,
Stufe 2). **Nur Bezeichner sind umbenannt, keine Zeichenketten.** Bewusst
gleich geblieben: der Ablage-Name `hangar.json` und alle Schlüssel darin
(`format`, `schiffe`, `wunsch`, `merkzettel`, `name`, `hersteller`,
`herkunft`, `belegung`, `kurz`, `hkurz`, `lti`, `warbond`, `paket`,
`gekauft`, `preis`, `ref`, `anzahl`, `weg`) sowie die Herkunftswerte
`'pledge'` und `'ingame'` — sonst verliert jeder beim Update seinen Hangar.
Ebenso der Seitenname `hangar` in Reiterleiste, Symbolsatz und „Neu"-Marken.
"""
import csv
import io
import json
import os
import re

from . import erkul, fehler, pfade

FILE = 'hangar.json'
FORMAT = 1

# Woher ein Schiff stammt. Mehr Fälle gibt es nicht — geliehene Schiffe stehen
# nicht im Hangar, und geschenkte sind aus Sicht des Spielers Pledges.
PLEDGE = 'pledge'
INGAME = 'ingame'


def path():
    return pfade.app_datei(FILE)


def empty():
    return {'format': FORMAT, 'schiffe': []}


def load():
    """Der eigene Hangar — oder eine leere Liste."""
    try:
        with open(path(), encoding='utf-8') as f:
            data = json.load(f)
        if data.get('format') == FORMAT and isinstance(data.get('schiffe'), list):
            return data
    except FileNotFoundError:
        pass
    except Exception as exc:
        fehler.merken('fleet.load', exc)
    return empty()


def save(data):
    """Atomar ablegen. Gibt zurück, ob es geklappt hat.

    ⚠ Der Rückgabewert wird ausgewertet — ein stilles `False` wäre genau der
    Fehler, der bei `pfade.einstellungen_schreiben` monatelang dafür sorgte,
    dass eine nicht gespeicherte Einstellung nach dem Neustart wieder alt war.
    """
    target = path()
    try:
        os.makedirs(os.path.dirname(target), exist_ok=True)
        with open(target + '.tmp', 'w', encoding='utf-8') as f:
            json.dump(data, f, ensure_ascii=False, indent=1)
        os.replace(target + '.tmp', target)
        return True
    except Exception as exc:
        fehler.merken('fleet.save', exc)
        return False


def _slim(text):
    return re.sub(r'[^a-z0-9]', '', (text or '').lower())


def count(data=None):
    return len((data or load()).get('schiffe') or [])


def names(data=None):
    """Alle Schiffsnamen im Hangar, alphabetisch."""
    items = (data or load()).get('schiffe') or []
    return sorted((s.get('name') or '' for s in items if s.get('name')),
                  key=str.lower)


def id_sets(data=None):
    """Je Schiff `(name, hersteller, kurz)` — was `erkul` zum Zuordnen braucht.

    ⚠ Der Kurzname aus dem Export trägt die meisten Treffer; ohne ihn fällt die
    Zuordnung von 34 auf 1 zurück. Deshalb wird er durchgereicht und nicht nur
    der Klartextname.
    """
    return [(s.get('name') or '', s.get('hersteller') or '',
             s.get('kurz') or '', s.get('hkurz') or '')
            for s in ((data or load()).get('schiffe') or []) if s.get('name')]


# ------------------------------------------------------------ Wunschliste

def wishlist(data=None):
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
    items = (data or load()).get('wunsch') or []
    return sorted((w for w in items if isinstance(w, dict) and w.get('name')),
                  key=lambda w: (w.get('name') or '').lower())


def wishlist_contains(data, name):
    wanted = _slim(name)
    return any(_slim(w.get('name')) == wanted
               for w in (data.get('wunsch') or []))


def wishlist_add(data, name, manufacturer=''):
    """Ein Schiff auf die Wunschliste setzen. Gibt zurück, ob es neu war.

    ⚠ Was schon im Hangar steht, kommt **nicht** auf die Wunschliste — man
    wünscht sich nichts, das man hat. Die Anzeige sagt das auch, statt den
    Eintrag stillschweigend zu schlucken.
    """
    if not (name or '').strip():
        return False
    if wishlist_contains(data, name):
        return False
    # `belegung` liegt leer bereit, genau wie beim Hangar-Schiff: Auch ein
    # Wunschschiff lässt sich ausstatten, und so kostet das später keinen
    # Formatwechsel.
    data.setdefault('wunsch', []).append(
        {'name': name.strip(), 'hersteller': (manufacturer or '').strip(),
         'belegung': {}})
    return True


def wishlist_remove(data, name):
    """Einen Wunsch streichen. Gibt zurück, ob einer wegfiel."""
    wanted = _slim(name)
    before = len(data.get('wunsch') or [])
    data['wunsch'] = [w for w in (data.get('wunsch') or [])
                      if _slim(w.get('name')) != wanted]
    return len(data['wunsch']) != before


def notepad(data=None):
    """Einzelne Gegenstände, die man bauen oder kaufen will — alphabetisch.

    ⭐⭐ **Der Weg zum Farmen ohne Umweg über ein Schiff.** Bis v3.20.0 führte
    jede Materialliste über die Wunschliste: erst ein Schiff eintragen, dann
    Steckplätze belegen, dann stand das Material da. Für einen Helm, eine Waffe
    oder ein Rüstungsteil gab es diesen Weg **gar nicht** — obwohl sie genauso
    Baupläne mit Rohstoffbedarf sind.

    Gemeldet von **Haldjas** am 06.09.2026: *„‚What to farm' ist irgendwie
    bisschen unnötig komplex — man geht da rein, wird dann zu ‚still missing'
    geschickt und weiß dann aber nicht so genau, was man machen soll. […]
    Eventuell wäre es sinnvoll, direkt unter Crafting Buttons hinzuzufügen, die
    dann die entsprechenden Blueprints auf die Wishlist / zu What to farm
    hinzufügen. Es wäre nämlich auch ganz nützlich, wenn man nicht nur
    Schiffsteile, sondern auch Rüstungen/Waffen für FPS hinzufügen könnte zum
    Workshop, sind ja immerhin auch Blueprints, die Ressourcen brauchen."*

    ⚠ Ein fehlendes Feld gilt als leere Liste — alte Dateien bleiben gültig,
    es braucht keinen Formatwechsel. Dieselbe Entscheidung wie bei
    `wishlist()`.
    """
    items = (data or load()).get('merkzettel') or []
    result = []
    for m in items:
        if not isinstance(m, dict) or not m.get('name'):
            continue
        # ⚠⚠⚠ **Eine Kennung mit Leerzeichen ist keine Kennung, sondern ein
        # Name.** v3.21.0 und v3.22.0 haben genau das gespeichert; daraus wurde
        # eine kaputte Preisabfrage (`items_prices?uuid=CF-447 Rhino Repeater`),
        # die Seite „Was noch fehlt" blieb leer und lud endlos.
        #
        # Hier wird es beim Lesen stillschweigend verworfen — so heilen sich
        # vorhandene Merkzettel von selbst, ohne dass jemand etwas neu eintragen
        # muss. Der Posten bleibt vollstaendig: Das Rezept findet `bauweg()`
        # ueber den Namen.
        ref = (m.get('ref') or '').strip()
        if ref and (any(z.isspace() for z in ref) or '"' in ref):
            m = dict(m)
            m['ref'] = ''
        result.append(m)
    return sorted(result, key=lambda m: (m.get('name') or '').lower())


def notepad_contains(data, name):
    wanted = _slim(name)
    return any(_slim(m.get('name')) == wanted
               for m in (data.get('merkzettel') or []))


def notepad_add(data, name, ref='', count=1):
    """Einen Gegenstand auf den Merkzettel setzen. Gibt zurück, ob er neu war.

    ⚠ `ref` ist die Kennung aus den Rezeptdaten. Ohne sie ließe sich später
    weder Preis noch Rezept nachschlagen — der Eintrag wäre ein Name ohne
    Anschluss. Sie darf leer bleiben (dann sucht die Rechnung über den Namen),
    aber sie wird mitgegeben, wo sie vorliegt.

    ⚠ Steht der Gegenstand schon drauf, wird **die Anzahl erhöht** statt eine
    zweite Zeile anzulegen. Zwei Zeilen mit demselben Helm wären in der
    Materialliste doppelt gezählt und in der Anzeige verwirrend.
    """
    name = (name or '').strip()
    if not name:
        return False
    try:
        count = max(1, int(count))
    except (TypeError, ValueError):
        count = 1
    wanted = _slim(name)
    for entry in (data.get('merkzettel') or []):
        if _slim(entry.get('name')) == wanted:
            entry['anzahl'] = int(entry.get('anzahl') or 1) + count
            return False
    data.setdefault('merkzettel', []).append(
        {'name': name, 'ref': (ref or '').strip(), 'anzahl': count,
         'weg': 'bauen'})
    return True


def notepad_remove(data, name):
    """Einen Gegenstand vom Merkzettel streichen. Gibt zurück, ob einer wegfiel."""
    wanted = _slim(name)
    before = len(data.get('merkzettel') or [])
    data['merkzettel'] = [m for m in (data.get('merkzettel') or [])
                          if _slim(m.get('name')) != wanted]
    return len(data['merkzettel']) != before


def notepad_set_count(data, name, count):
    """Wie oft der Gegenstand gebaut werden soll. `0` streicht ihn."""
    try:
        count = int(count)
    except (TypeError, ValueError):
        return False
    if count <= 0:
        return notepad_remove(data, name)
    wanted = _slim(name)
    for entry in (data.get('merkzettel') or []):
        if _slim(entry.get('name')) == wanted:
            entry['anzahl'] = count
            return True
    return False


def contains(data, name, manufacturer=''):
    """Steht dieses Schiff schon drin?

    ⚠ Verglichen wird über Hersteller **und** Name in schlanker Schreibweise.
    „Cutlass Black" von Drake und eine gleichnamige Variante eines anderen
    Herstellers wären sonst dasselbe.
    """
    wanted = _slim(manufacturer) + _slim(name)
    for s in (data.get('schiffe') or []):
        if _slim(s.get('hersteller')) + _slim(s.get('name')) == wanted:
            return True
    return False


def add(data, name, manufacturer='', origin=INGAME, **rest):
    """Ein Schiff eintragen. Gibt zurück, ob es neu war.

    Doppelte werden still übergangen — wer zweimal importiert, soll nicht jedes
    Schiff doppelt im Hangar stehen haben.
    """
    if not (name or '').strip():
        return False
    if contains(data, name, manufacturer):
        return False
    entry = {'name': name.strip(), 'hersteller': (manufacturer or '').strip(),
             'herkunft': origin, 'belegung': {}}
    for key in ('kurz', 'hkurz', 'lti', 'warbond', 'paket', 'gekauft',
                'preis'):
        if rest.get(key) not in (None, ''):
            entry[key] = rest[key]
    data.setdefault('schiffe', []).append(entry)
    return True


def remove(data, name, manufacturer=''):
    """Ein Schiff austragen. Gibt zurück, ob eines wegfiel."""
    wanted = _slim(manufacturer) + _slim(name)
    before = len(data.get('schiffe') or [])
    data['schiffe'] = [
        s for s in (data.get('schiffe') or [])
        if _slim(s.get('hersteller')) + _slim(s.get('name')) != wanted]
    return len(data['schiffe']) != before


# ---------------------------------------------------------------- Import

def _from_json(text):
    """Der JSON-Export von Hangar XPLORer."""
    raw = json.loads(text)
    if not isinstance(raw, list):
        return []
    result = []
    for entry in raw:
        if not isinstance(entry, dict):
            continue
        # ⚠ Nur Schiffe. Der Export führt auch Ausrüstung, Farben und Anzüge —
        # ein „Bosco Weapon Display Rack" hat keine Steckplätze.
        if entry.get('entity_type') not in (None, '', 'ship'):
            continue
        # ⚠⚠ **`name` vor `ship_name`** — und das ist kein Geschmack.
        # `ship_name` ist der Grundtyp, `name` die tatsächliche Ausführung.
        # Andersherum wurden aus „A.T.L.S." und „ATLS GEO" zwei Einträge
        # desselben Namens, von denen der zweite als Doppel wegfiel; und die
        # „F7C-M Super Hornet Mk II" hieß Mk I, weil ihr `ship_code` noch auf
        # der alten Ausführung steht.
        name = (entry.get('name') or entry.get('ship_name') or '').strip()
        if not name:
            continue
        result.append({
            'name': name,
            'hersteller': (entry.get('manufacturer_name') or '').strip(),
            'kurz': (entry.get('ship_code') or '').strip(),
            # ⚠ Das Herstellerkürzel ist für die Zuordnung wichtiger als der
            # ausgeschriebene Name: Erkul führt „Roberts Space Industries" als
            # `rsi`. Ohne dieses Feld fand die Ursa Medivac keinen Anschluss.
            'hkurz': (entry.get('manufacturer_code') or '').strip(),
            'lti': bool(entry.get('lti')),
            'warbond': bool(entry.get('warbond')),
            'paket': (entry.get('pledge_name') or '').strip(),
            'gekauft': (entry.get('pledge_date') or '').strip(),
            'preis': (entry.get('pledge_cost') or '').strip(),
        })
    return result


def _from_csv(text):
    """Der CSV-Export von Hangar XPLORer.

    ⚠ Die Kopfzeile trägt **Leerzeichen hinter den Kommas** (`Manufacturer,
    Ship, Lti, …`). Ohne `skipinitialspace` heißt die zweite Spalte `' Ship'`
    und wird nie gefunden.
    """
    result = []
    for row in csv.DictReader(io.StringIO(text), skipinitialspace=True):
        name = (row.get('Ship') or '').strip()
        # ⚠ Der Export schreibt bei unbekannten Stücken wörtlich `undefined`
        # in die Namensspalte — das ist kein Schiff, sondern eine Lücke.
        if not name or name == 'undefined':
            continue
        result.append({
            'name': name,
            'hersteller': (row.get('Manufacturer') or '').strip(),
            'kurz': '',
            'hkurz': '',
            'lti': (row.get('Lti') or '').strip().lower() == 'true',
            'warbond': (row.get('Warbond') or '').strip().lower() == 'true',
            'paket': (row.get('Pledge') or '').strip(),
            'gekauft': (row.get('Date') or '').strip(),
            'preis': (row.get('Cost') or '').strip(),
        })
    return result


def read(file_path):
    """Eine Exportdatei einlesen. Gibt `(eintraege, fehlertext)` zurück.

    Erkannt wird am Inhalt, nicht an der Endung — wer eine `.json` in `.txt`
    umbenennt, soll trotzdem weiterkommen.
    """
    try:
        with open(file_path, encoding='utf-8-sig') as f:
            text = f.read()
    except Exception as exc:
        fehler.merken('fleet.read', exc)
        return [], str(exc)
    head = text.lstrip()[:1]
    try:
        entries = _from_json(text) if head == '[' else _from_csv(text)
    except Exception as exc:
        fehler.merken('fleet.read.parse', exc)
        return [], str(exc)
    return entries, ''


def import_entries(entries, data=None, save_now=True):
    """Gelesene Einträge in den Hangar übernehmen.

    Gibt `(neu, schon_da)` zurück.
    """
    data = data if data is not None else load()
    new = 0
    for e in entries:
        if add(data, e.get('name'), e.get('hersteller'),
               origin=PLEDGE, kurz=e.get('kurz'),
               hkurz=e.get('hkurz'), lti=e.get('lti'),
               warbond=e.get('warbond'), paket=e.get('paket'),
               gekauft=e.get('gekauft'), preis=e.get('preis')):
            new += 1
    if save_now:
        save(data)
    return new, len(entries) - new


def unknown(data=None):
    """Welche Schiffe im Hangar erkul **nicht** kennt.

    ⚠ Das ist meistens **keine Panne**, sondern eine Auskunft: Erkul führt nur
    Schiffe, die im Spiel flugfähig sind. Ein Treffer hier heißt in aller Regel
    „gibt es noch nicht" — bei einem echten Hangar-Export vom 06.09.2026 waren
    das Crucible, Endeavor, Galaxy, Liberator, Merchantman und die beiden ATLS.
    """
    result = []
    for s in ((data or load()).get('schiffe') or []):
        if not erkul.knows(s.get('name'), s.get('hersteller'), s.get('kurz'),
                           s.get('hkurz')):
            result.append(s.get('name') or '')
    return result


def wishlist_id_sets(data=None):
    """Dasselbe für die Wunschliste — je Wunsch `(name, hersteller, '', '')`.

    ⚠ **Getrennt von `id_sets()`, mit Absicht.** Beide Listen holen dieselbe
    Art Daten, dürfen aber nie in denselben Topf: `id_sets()` speist auch
    „passt in dein Schiff", und ein Wunschschiff hat man nicht. Wer die Listen
    hier zusammenlegt, beantwortet dort eine Frage über fremdes Eigentum.

    ⚠ **Ein fehlender Hersteller wird nachgeschlagen.** Wünsche, die vor dem
    06.09.2026 eingetragen wurden, haben keinen — und ohne ihn findet `erkul`
    einen Teil der Schiffe nicht. Statt diese Einträge stillschweigend
    auszulassen, wird der Hersteller bei UEX geholt. Geschrieben wird dabei
    nichts: Eine Leseabfrage, die nebenbei die Datei ändert, überrascht später
    an einer Stelle, an der niemand damit rechnet.
    """
    from . import ships
    result = []
    for w in ((data or load()).get('wunsch') or []):
        if not (isinstance(w, dict) and w.get('name')):
            continue
        name = w.get('name') or ''
        result.append((name, w.get('hersteller') or ships.manufacturer(name),
                       '', ''))
    return result


def fetch_missing(data=None):
    """Die Steckplätze aller Schiffe holen, soweit sie fehlen.

    Gibt zurück, wie viele Schiffe neu geholt wurden.

    ⚠ **Die Wunschliste zählt mit.** Wer sich ein Schiff vornimmt, will oft
    gleich planen, was hineinsoll — am 06.09.2026 gefragt: „was ist, wenn
    jemand ein Schiff und dazu ein besseres Fitting bauen oder kaufen will?"
    Ohne Steckplatz-Daten steht auf der Wunschzeile nur eine Kaufsumme, und
    die Ausstattung ließe sich erst planen, wenn das Schiff schon gekauft ist.
    Geholt wird nur — verrechnet werden Wunschschiffe nirgends als Besitz.
    """
    return erkul.add_missing(id_sets(data) + wishlist_id_sets(data))
