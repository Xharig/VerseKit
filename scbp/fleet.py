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

Ohne diese Liste ist nicht zu beantworten, ob ein Bauplan ins eigene Schiff passt;
das Werkzeug wüsste nur, in *irgendein* Schiff passt es. `ships.py` führt
alle Schiffe des Spiels für den Routenplaner — hier geht es um die eigenen.

## ⚠ Die `Game.log` gibt das nicht her — gemessen

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
| Import aus der **Star Citizen: Hangar Extension** (AlyxOne), JSON | alle Echtgeld-Schiffe samt Hersteller, Code, Paketzugehörigkeit und **Versicherung** (`"LTI"` oder `"120MI"`) | im Spiel gekaufte Schiffe; Preis und Kaufdatum |
| dieselbe Erweiterung, **CSV** (Komplett-Export) | Schiffe samt **LTI oder Versicherungsdauer in Monaten** (`versicherung`), Paketname, Datum, Preis | im Spiel gekaufte Schiffe; Schiffskürzel |
| Import aus **Star Citizen Hangar XPLORer** (dolkensp) | alle Echtgeld-Pledges samt LTI und Paketname | im Spiel gekaufte Schiffe |
| **Von Hand** eintragen | alles Übrige | — |

Beide Erweiterungen setzen auf der Pledge-Seite Export-Knöpfe. Gelesen wird
JSON und CSV beider — erkannt am Inhalt, nicht am Dateinamen (`_from_json`,
`_from_csv`). Empfohlen wird die Hangar Extension: Sie wird gepflegt, der
XPLORer nicht mehr. Wer JSON **und** CSV der Extension
einliest, bekommt Kürzel und Paketbeziehung aus dem einen und die
Versicherung aus dem anderen — `_same_ship` führt sie zusammen.

⚠ **Der Export kennt nur Gekauftes.** Wer sich im Spiel eine Cutlass erflogen
hat, findet sie dort nie — deshalb ist der Handeintrag kein Notbehelf, sondern
gleichberechtigt. Jedes Schiff trägt seine `herkunft`.

## ⚠ Die Kaufsumme bleibt im Haus

`pledge_cost` und `pledge_id` sind private Angaben. Sie werden abgelegt, weil
sie dem Spieler gehören und er sie sehen will — aber sie dürfen **nie** in den
Fehlerbericht geraten. Dieselbe Linie wie `paths.redact()` bei den Pfaden.

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

⚠ **Bezeichner sind englisch, gespeicherte Zeichenketten nicht.** Deutsch
bleiben: der Ablage-Name `hangar.json` und alle Schlüssel darin
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

from . import erkul, errors, paths

FILE = 'hangar.json'
FORMAT = 1

# Woher ein Schiff stammt. Mehr Fälle gibt es nicht — geliehene Schiffe stehen
# nicht im Hangar, und geschenkte sind aus Sicht des Spielers Pledges.
PLEDGE = 'pledge'
INGAME = 'ingame'
# Kam nur über den Abgleich mit dem KRT Profit Basetool herein.
# Ein Pledge-Import räumt es nicht weg (nur `pledge` fällt heraus).
BASETOOL = 'basetool'
# Aus einer Schiffsliste eines anderen Werkzeugs (Fleetview, Fleetyards,
# StarJump FleetViewer) — `quelle` nennt es.
LIST = 'liste'


def path():
    return paths.app_file(FILE)


def empty():
    return {'format': FORMAT, 'schiffe': []}


def load():
    """Der eigene Hangar — oder eine leere Liste."""
    try:
        with open(path(), encoding='utf-8') as f:
            data = json.load(f)
        if data.get('format') == FORMAT and isinstance(data.get('schiffe'), list):
            if merge_duplicates(data):
                save(data)
            return data
    except FileNotFoundError:
        pass
    except Exception as exc:
        errors.record('fleet.load', exc)
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
    except Exception as exc:
        errors.record('fleet.save', exc)
        return False
    try:
        from . import basetool_sync
        basetool_sync.store_saved()
    except Exception as exc:
        errors.record('fleet.abgleich_melden', exc)
    return True


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

    ⚠ **Getrennt vom Hangar, in derselben Datei.** Ein Wunschschiff ist kein
    Besitz: Was hier steht, darf nirgends in die Auskunft einfließen, ob ein
    Teil ins eigene Schiff passt, sonst beantwortet das Werkzeug eine Frage über ein Schiff, das
    der Spieler gar nicht hat. Ein fehlendes Feld gilt als leere Liste — alte
    Dateien bleiben damit gültig, es braucht keinen Formatwechsel.
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
    """Einen Eintrag der Wunschliste streichen. Gibt zurück, ob einer wegfiel."""
    wanted = _slim(name)
    before = len(data.get('wunsch') or [])
    data['wunsch'] = [w for w in (data.get('wunsch') or [])
                      if _slim(w.get('name')) != wanted]
    return len(data['wunsch']) != before


def notepad(data=None):
    """Einzelne Gegenstände, die man bauen oder kaufen will — alphabetisch.

    ⭐⭐ **Der Weg zum Farmen ohne Umweg über ein Schiff.** Über die
    Wunschliste führt eine Materialliste nur über ein Schiff und belegte
    Steckplätze. Für einen Helm, eine Waffe oder ein Rüstungsteil gäbe es
    diesen Weg **gar nicht** — obwohl sie genauso Baupläne mit Rohstoffbedarf
    sind.

    ⚠ Ein fehlendes Feld gilt als leere Liste — alte Dateien bleiben gültig,
    es braucht keinen Formatwechsel. Dieselbe Regel wie bei `wishlist()`.
    """
    items = (data or load()).get('merkzettel') or []
    result = []
    for m in items:
        if not isinstance(m, dict) or not m.get('name'):
            continue
        # ⚠⚠⚠ **Eine Kennung mit Leerzeichen ist keine Kennung, sondern ein
        # Name.** Ältere Merkzettel können so etwas enthalten; daraus wird
        # eine kaputte Preisabfrage (`items_prices?uuid=CF-447 Rhino Repeater`),
        # und die Seite „Was noch fehlt" bleibt leer und lädt endlos.
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


def notepad_crafted(data, name, count=1):
    """`count` Stück gebaut: Die Stückzahl sinkt, bei null fällt er weg.

    Gibt zurück, ob der Gegenstand auf dem Merkzettel stand."""
    try:
        count = max(1, int(count))
    except (TypeError, ValueError):
        count = 1
    wanted = _slim(name)
    for entry in (data.get('merkzettel') or []):
        if _slim(entry.get('name')) == wanted:
            return notepad_set_count(
                data, entry.get('name'),
                int(entry.get('anzahl') or 1) - count) or True
    return False


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


def _same_ship(entry, name, manufacturer='', kurz='', hkurz=''):
    """Meint dieser Hangar-Eintrag dasselbe Schiff?

    Drei Wege, jeder für sich reicht:

    1. **Schiffskürzel** (`MISC_Endeavor`) — der sicherste. Beide Exporte
       (Hangar XPLORer und Hangar Extension) tragen dasselbe Kürzel.
    2. **Herstellerkürzel + Name** (`MISC` + `Endeavor`).
    3. **Herstellername + Name** in schlanker Schreibweise — der einzige Weg
       für Einträge von Hand, die kein Kürzel haben.

    ⚠ Der Name allein reicht nicht: die Cutlass Black von Drake und eine
    gleichnamige Variante eines anderen Herstellers wären sonst dasselbe.

    ⚠ Warum drei Wege: Der XPLORer schreibt `Musashi Industrial & Starflight
    Concern`, die Hangar Extension `MISC` — ein Import aus der Erweiterung in
    einen XPLORer-Hangar ergäbe sonst Doppel bei gleichem Kürzel.
    """
    wanted = _slim(name)
    e_kurz = _slim(entry.get('kurz'))
    if (e_kurz and _slim(kurz) and e_kurz == _slim(kurz)
            and _names_compatible(_slim(entry.get('name')), wanted)):
        return True
    e_name = _slim(entry.get('name'))
    if not wanted:
        return False
    e_hkurz = _slim(entry.get('hkurz'))
    if e_hkurz and _slim(hkurz) and (
            e_hkurz == _slim(hkurz)
            or frozenset((e_hkurz, _slim(hkurz))) in _MAKER_TWINS):
        # Mit Herstellerkürzel darf der Name um ein Klassenwort abweichen —
        # das CSV der Extension hat kein Schiffskürzel, und `Idris-P` muss
        # trotzdem die `Idris-P Frigate` des XPLORer treffen.
        return _names_compatible(e_name, wanted)
    if e_name != wanted:
        return False
    return _slim(entry.get('hersteller')) == _slim(manufacturer)


# Klassenwörter, die der Hangar XPLORer an manche Namen hängt („Idris-P
# Frigate"), die Hangar Extension aber nicht („Idris-P"). Nur um so ein Wort
# dürfen sich zwei Namen bei gleichem Kürzel unterscheiden.
_CLASS_WORDS = ('frigate', 'destroyer', 'corvette', 'carrier', 'cruiser')

# Herstellerkürzel, die für dasselbe Schiff nebeneinander vorkommen: Razor,
# Fury, Guardian und Pulse liefen bei CIG von MISC zu Mirai — der XPLORer
# schreibt noch `MISC_Razor_EX`, die Hangar Extension `MRAI_Razor_EX`. Bei
# gleichem Namen ist das ein Schiff, nicht zwei.
_MAKER_TWINS = {frozenset(('misc', 'mrai'))}


def _names_compatible(a, b):
    """Meinen zwei geschliffene Namen bei gleichem Kürzel dasselbe Schiff?

    ⚠ Gleiches Kürzel heißt **nicht** gleiches Schiff: Der XPLORer gibt der
    ATLS GEO dasselbe Kürzel wie der ATLS (`ARGO_ATLS`), die Extension
    kennt `ARGO_ATLS_GEO`. Wer nur das Kürzel vergleicht, macht aus zwei
    Schiffen eines. Deshalb
    müssen die Namen gleich sein oder sich um ein Klassenwort unterscheiden.
    """
    if not a or not b:
        return False
    if a == b:
        return True
    long, short = (a, b) if len(a) > len(b) else (b, a)
    return long.startswith(short) and long[len(short):] in _CLASS_WORDS


def find(data, name, manufacturer='', kurz='', hkurz=''):
    """Der vorhandene Eintrag zu diesem Schiff — oder `None`."""
    for s in (data.get('schiffe') or []):
        if _same_ship(s, name, manufacturer, kurz, hkurz):
            return s
    return None


def contains(data, name, manufacturer='', kurz='', hkurz=''):
    """Steht dieses Schiff schon drin? Siehe `_same_ship`."""
    return find(data, name, manufacturer, kurz, hkurz) is not None


_IMPORT_KEYS = ('kurz', 'hkurz', 'lti', 'warbond', 'paket', 'gekauft', 'preis',
                'versicherung')


def _fill(entry, **rest):
    """Fehlende Angaben nachtragen, vorhandene nicht anrühren.

    ⚠ `lti` wird nur **gesetzt**, nie zurückgenommen: Ältere Exporte der
    Hangar Extension kennen das Feld `insurance` noch nicht und melden überall
    `False` — das darf ein LTI aus dem XPLORer-Import nicht löschen.
    """
    changed = False
    for key in _IMPORT_KEYS:
        value = rest.get(key)
        if value in (None, ''):
            continue
        if key in ('lti', 'warbond'):
            if value is True and entry.get(key) is not True:
                entry[key] = True
                changed = True
            continue
        if entry.get(key) in (None, '', 0):
            entry[key] = value
            changed = True
    return changed


def add(data, name, manufacturer='', origin=INGAME, **rest):
    """Ein Schiff eintragen. Gibt zurück, ob es neu war.

    Doppelte werden still übergangen — wer zweimal importiert, soll nicht jedes
    Schiff doppelt im Hangar stehen haben. Bringt der zweite Import Angaben
    mit, die dem vorhandenen Eintrag fehlen (Kürzel, Paket), werden sie
    nachgetragen.
    """
    if not (name or '').strip():
        return False
    found = find(data, name, manufacturer, rest.get('kurz'), rest.get('hkurz'))
    if found is not None:
        _fill(found, **rest)
        return False
    entry = {'name': name.strip(), 'hersteller': (manufacturer or '').strip(),
             'herkunft': origin, 'belegung': {}}
    for key in _IMPORT_KEYS:
        if rest.get(key) not in (None, ''):
            entry[key] = rest[key]
    data.setdefault('schiffe', []).append(entry)
    return True


def bundled_with(data, entry):
    """Der Name des Hangar-Schiffs, in dessen Paket dieses Schiff steckt — oder `''`.

    Die Hangar Extension schreibt unter `includedWith` das Schiff, mit dem
    eines mitkam (die URSA der Carrack, die MPUV Personnel der Idris-P). Beim
    XPLORer steht unter `paket` dagegen der **Pledge-Name** (`Standalone
    Ship`, `Package - Dominus Pack`) — das ist keine Beilage. Unterschieden
    wird deshalb daran, ob der Wert ein Schiff **im eigenen Hangar** nennt.
    """
    wanted = _slim(entry.get('paket'))
    if not wanted or wanted == _slim(entry.get('name')):
        return ''
    for s in (data.get('schiffe') or []):
        if s is entry:
            continue
        if _slim(s.get('name')) == wanted:
            return s.get('name') or ''
    return ''


def merge_duplicates(data):
    """Doppelte Einträge zusammenführen. Gibt zurück, ob sich etwas änderte.

    Der **erste** Eintrag bleibt (er trägt die Belegung und die älteren
    Angaben wie LTI), die späteren geben ab, was ihm fehlt, und verschwinden.
    Läuft bei jedem Laden — so räumt sich ein Hangar, der vor diesem Fix
    doppelt importiert wurde, von selbst auf.
    """
    kept = []
    changed = False
    for s in (data.get('schiffe') or []):
        match = None
        for k in kept:
            if _same_ship(k, s.get('name'), s.get('hersteller'),
                          s.get('kurz'), s.get('hkurz')):
                match = k
                break
        if match is None:
            kept.append(s)
            continue
        changed = True
        _fill(match, **{key: s.get(key) for key in _IMPORT_KEYS})
        if not match.get('belegung') and s.get('belegung'):
            match['belegung'] = s['belegung']
    if changed:
        data['schiffe'] = kept
    return changed


def remove(data, name, manufacturer=''):
    """Ein Schiff austragen. Gibt zurück, ob eines wegfiel."""
    wanted = _slim(manufacturer) + _slim(name)
    before = len(data.get('schiffe') or [])
    data['schiffe'] = [
        s for s in (data.get('schiffe') or [])
        if _slim(s.get('hersteller')) + _slim(s.get('name')) != wanted]
    return len(data['schiffe']) != before


# ---------------------------------------------------------------- Import

_EXT_INSURANCE = re.compile(r'^\s*(\d+)\s*(?:mi|mo|months?)?\s*$',
                            re.IGNORECASE)


def _extension_insurance(entry):
    """Die Versicherung aus dem JSON der Hangar Extension → `(lti, monate)`.

    Sie steht in **einem** Feld, im selben Zuschnitt, den der Webhangar zeigt:
    `"insurance": "LTI"` oder `"insurance": "120MI"` — Dauer in Monaten.

    ⚠ **`"0MI"` ist keine Versicherung von null Monaten, sondern gar keine
    Angabe.** Das ist der Rückfallwert der Erweiterung für Pledges, die die
    Information nicht führen — er landet deshalb wie ein fehlendes Feld.

    ⚠ **Fehlt das Feld, gibt es keine Angabe** — kein LTI, keine Dauer. Die
    Schiffszeile bleibt dann ohne Versicherung, statt „keine" zu behaupten.

    ⚠ Ältere Exporte der Erweiterung führen zwei Felder (`insurance` als
    Wahrheitswert, `insuranceMonths` als Zahl). Beide werden weiter gelesen,
    damit solche Exporte gültig bleiben.
    """
    value = entry.get('insurance')
    lti = False
    months = 0
    if isinstance(value, bool):
        lti = value
    elif isinstance(value, str):
        text = value.strip()
        if text.lower() == 'lti':
            lti = True
        else:
            match = _EXT_INSURANCE.match(text)
            if match:
                months = int(match.group(1))
    try:
        months = max(months, int(entry.get('insuranceMonths') or 0))
    except (TypeError, ValueError):
        pass
    return lti, months


def _hangar_extension_entry(entry):
    """Ein Schiff aus dem JSON der **Star Citizen: Hangar Extension** (AlyxOne).

    Die empfohlene Erweiterung — ein gepflegter Fork des XPLORer, MIT. Ihr
    Export sieht anders aus:

        {"manufacturer": {"code": "ARGO", "name": "Argo Astronautics",
                          "shortName": "ARGO"},
         "code": "ARGO_ATLS", "matrix": "ATLS", "name": "ATLS",
         "focus": "Cargo", "status": "Flight-Ready",
         "insurance": "LTI",                   # oder "120MI", wenn vorhanden
         "includedWith": "Idris-P"}            # nur bei Paket-Beilagen

    ⚠ `name` vor `matrix` — dieselbe Regel wie beim XPLORer (`name` vor
    `ship_name`): `matrix` ist der Grundtyp (`L-22 Alpha Wolf`), `name` die
    Ausführung, wie der Store sie führt (`L22-AlphaWolf`).

    ⚠ **Warbond, Kaufdatum und Preis stehen nur im CSV-Export** — der JSON
    führt Schiffe und Fahrzeuge samt ihrer Beziehungen, dazu die Versicherung
    (siehe `_extension_insurance`). `includedWith` nennt das Paket, mit dem ein
    Schiff kam (die MPUV Personnel der Idris-P); das ist die Angabe, die beim
    XPLORer `pledge_name` heißt, und landet deshalb unter `paket`.
    """
    maker = entry.get('manufacturer') or {}
    name = (entry.get('name') or entry.get('matrix') or '').strip()
    if not name:
        return None
    lti, months = _extension_insurance(entry)
    return {
        'name': name,
        'hersteller': (maker.get('name') or '').strip(),
        'kurz': (entry.get('code') or '').strip(),
        'hkurz': (maker.get('code') or maker.get('shortName') or '').strip(),
        'lti': lti,
        'warbond': False,
        # Monate nur ohne LTI — LTI ist die Dauer (wie beim CSV).
        'versicherung': (months if months and not lti else None),
        'paket': (entry.get('includedWith') or '').strip(),
        'gekauft': '',
        'preis': '',
    }


# ------------------------------------------------ Schiffslisten
#
# ⭐ Drei weitere Werkzeuge, deren Listen auch das KRT Profit Basetool
# einliest. Die Formate stehen im
# offenen Quellcode des Basetools (`FleetExportParser.java`, GPL-3.0):
#
# | Werkzeug | Wurzel | ein Schiff |
# |---|---|---|
# | CCU Game **Fleetview** | Liste | `{"name": "aurora mr", "shipname": "", "type": "ship"}` — je Schiff eine Zeile, klein geschrieben, **ohne Hersteller** |
# | **Fleetyards** | Liste | `{"name": "A1 Spirit", "slug": "crus-a1-spirit", "manufacturerName": "Crusader Industries", "manufacturerCode": "CRUS", "shipName": "Koto", "wanted": false}` |
# | **StarJump** FleetViewer | Objekt `{"type": "starjumpFleetviewer", "canvasItems": [...]}` | `{"itemType": "SHIP", "shipSlug": "perseus", "defaultText": "Perseus"}` |
#
# ⚠⚠ **Eine Schiffsliste ist KEIN Pledge-Export.** Sie kennt oft auch im Spiel
# gekaufte Schiffe und ist selten vollständig. Sie **ergänzt** deshalb nur —
# ausgetragen wird dabei nichts (`import_entries`), und die Schiffe tragen die
# Herkunft `LIST`, damit ein späterer Pledge-Import sie nicht wegräumt.
#
# ⚠ Keine Versicherung, kein Paket, kein Preis: Keine der drei Listen führt
# das. Eigene Schiffsnamen (`shipname`) werden nicht übernommen — VerseKits
# eigene Namen sind die Namen am Terminal im Spiel, etwas anderes.


def _nice(name):
    """`aurora mr` → `Aurora MR` — nur, wenn die Liste alles klein schreibt."""
    name = (name or '').strip()
    if not name or name != name.lower():
        return name
    words = []
    for word in name.split():
        words.append(word.upper() if len(word) <= 2 and word.isalpha()
                     else word[:1].upper() + word[1:])
    return ' '.join(words)


def _catalog_name(name):
    """Name ohne Hersteller → `(Name, Hersteller, Kürzel)` aus dem Schiffskatalog.

    `890 jump` → (`890 Jump`, `Origin`, `ORIG`). UEX führt den Hersteller im
    Namen (`Origin 890 Jump`), `_split_maker` trennt ihn wieder ab — derselbe
    Weg wie beim CSV der Hangar Extension, damit dasselbe Schiff dieselben
    Kürzel trägt und nicht doppelt im Hangar landet."""
    try:
        from . import ships
        entry = ships._find(name)
    except Exception:
        entry = None
    if not entry:
        return _nice(name), '', ''
    maker, code, rest = _split_maker(entry.get('name') or '')
    if not rest or not maker:
        return _nice(name), '', ''
    return rest, maker, code


def _list_entry(name, source, maker='', maker_code='', wanted=False):
    name = (name or '').strip()
    if not name:
        return None
    if not maker_code:
        name, maker, maker_code = _catalog_name(name)
    return {'name': name, 'hersteller': (maker or '').strip(), 'kurz': '',
            'hkurz': (maker_code or '').strip(), 'lti': False,
            'warbond': False, 'paket': '', 'gekauft': '', 'preis': '',
            'herkunft': LIST, 'quelle': source, 'wunsch': bool(wanted)}


def _from_fleetview(entry):
    if entry.get('type') not in (None, '', 'ship'):
        return None
    return _list_entry(entry.get('name'), 'Fleetview')


def _from_fleetyards(entry):
    name = entry.get('name') or ''
    if not name.strip():
        return None
    return _list_entry(name, 'Fleetyards',
                       entry.get('manufacturerName') or '',
                       entry.get('manufacturerCode') or '',
                       wanted=entry.get('wanted') is True)


def _from_starjump(raw):
    """Der FleetViewer von StarJump — ein Objekt, die Schiffe unter `canvasItems`."""
    result = []
    for item in raw.get('canvasItems') or ():
        if not isinstance(item, dict) or \
                (item.get('itemType') or '').lower() != 'ship':
            continue
        name = (item.get('defaultText') or '').strip()
        if not name:
            # Nur der Kurzname: `zeus-mkii-mr` → `zeus mkii mr`
            name = (item.get('shipSlug') or '').replace('-', ' ').strip()
        ship = _list_entry(name, 'StarJump')
        if ship:
            result.append(ship)
    return result


def _is_starjump(raw):
    return isinstance(raw, dict) and (
        str(raw.get('type') or '').lower() == 'starjumpfleetviewer'
        or isinstance(raw.get('canvasItems'), list))


def _from_json(text):
    """Der JSON-Export — vom Hangar XPLORer **oder** von der Hangar Extension.

    Zwei Werkzeuge, zwei Formen, ein Erkenner:

    | | Hangar XPLORer (dolkensp) | Hangar Extension (AlyxOne) |
    |---|---|---|
    | Eintrag | flach: `name`, `ship_name`, `manufacturer_name`, `manufacturer_code`, `ship_code`, `lti`, `pledge_*` | `name`, `matrix`, `code`, `manufacturer` als **Wörterbuch** |
    | Umfang | Schiffe **und** Ausrüstung, Farben, Anzüge (`entity_type`) | nur Schiffe und Fahrzeuge |
    | Pledge-Angaben | LTI, Warbond, Paket, Datum, Preis | Versicherung und Paket — kein Warbond, Datum, Preis (siehe `_hangar_extension_entry`) |

    Erkannt wird **je Eintrag** am Wörterbuch `manufacturer` — kein Werkzeug
    schreibt das Feld des anderen. Beide werden eingelesen, auch ältere
    XPLORer-Exporte.
    """
    raw = json.loads(text)
    if _is_starjump(raw):
        return _from_starjump(raw)
    if not isinstance(raw, list):
        return []
    result = []
    for entry in raw:
        if not isinstance(entry, dict):
            continue
        if isinstance(entry.get('manufacturer'), dict):
            ship = _hangar_extension_entry(entry)
            if ship:
                result.append(ship)
            continue
        # Fleetyards (camelCase) und Fleetview — erkannt wie im Basetool, an
        # Feldern, die der XPLORer nie schreibt (der führt `ship_code`,
        # `pledge_id`, `entity_type`).
        if not any(k in entry for k in ('pledge_id', 'ship_code',
                                        'entity_type', 'ship_name')):
            if 'shipCode' in entry or 'manufacturerCode' in entry \
                    or 'manufacturerName' in entry or 'slug' in entry:
                ship = _from_fleetyards(entry)
            elif 'shipname' in entry or 'type' in entry:
                ship = _from_fleetview(entry)
            else:
                ship = False
            if ship is not False:
                if ship:
                    result.append(ship)
                continue
        # ⚠ Nur Schiffe. Der Export führt auch Ausrüstung, Farben und Anzüge —
        # ein `Bosco Weapon Display Rack` hat keine Steckplätze.
        if entry.get('entity_type') not in (None, '', 'ship'):
            continue
        # ⚠⚠ **`name` vor `ship_name`** — und das ist kein Geschmack.
        # `ship_name` ist der Grundtyp, `name` die tatsächliche Ausführung.
        # Andersherum würden aus `A.T.L.S.` und `ATLS GEO` zwei Einträge
        # desselben Namens, von denen der zweite als Doppel wegfiele; und die
        # `F7C-M Super Hornet Mk II` hieße Mk I, weil ihr `ship_code` noch auf
        # der alten Ausführung steht.
        name = (entry.get('name') or entry.get('ship_name') or '').strip()
        if not name:
            continue
        result.append({
            'name': name,
            'hersteller': (entry.get('manufacturer_name') or '').strip(),
            'kurz': (entry.get('ship_code') or '').strip(),
            # ⚠ Das Herstellerkürzel ist für die Zuordnung wichtiger als der
            # ausgeschriebene Name: Erkul führt `Roberts Space Industries` als
            # `rsi`. Ohne dieses Feld findet etwa die Ursa Medivac keinen Anschluss.
            'hkurz': (entry.get('manufacturer_code') or '').strip(),
            'lti': bool(entry.get('lti')),
            'warbond': bool(entry.get('warbond')),
            'paket': (entry.get('pledge_name') or '').strip(),
            'gekauft': (entry.get('pledge_date') or '').strip(),
            'preis': (entry.get('pledge_cost') or '').strip(),
        })
    return result


# Wie die Hangar Extension den Hersteller im CSV vor den Schiffsnamen setzt
# (`Aegis Idris-P`, `RSI Galaxy`, `Kruger L-22 Alpha Wolf`) → Kürzel, wie es
# ihr JSON-Export und erkul führen. Längste Namen zuerst, damit `Consolidated
# Outland` nicht an `Consolidated` scheitert.
_CSV_MAKERS = (
    ('Roberts Space Industries', 'RSI'), ('Consolidated Outland', 'CNOU'),
    ('Musashi Industrial & Starflight Concern', 'MISC'),
    ('Aegis', 'AEGS'), ('Anvil', 'ANVL'), ('Aopoa', 'AOPO'), ('ARGO', 'ARGO'),
    ('Argo', 'ARGO'), ('Banu', 'BANU'), ('Crusader', 'CRUS'), ('CNOU', 'CNOU'),
    ('Drake', 'DRAK'), ('Esperia', 'ESPR'), ('Gatac', 'GAMA'),
    ('Greycat', 'GRIN'), ('Kruger', 'KRIG'), ('MISC', 'MISC'),
    ('Mirai', 'MRAI'), ('Origin', 'ORIG'), ('RSI', 'RSI'), ('Tumbril', 'TMBL'),
    ('Vanduul', 'VNCL'), ("Xi'an", 'XIAN'), ('Xian', 'XIAN'),
)

_MONTHS = re.compile(r'^\s*(\d+)\s*month', re.IGNORECASE)


def _split_maker(full):
    """`Aegis Idris-P` → (`Aegis`, `AEGS`, `Idris-P`); ohne Treffer bleibt der Name ganz."""
    full = (full or '').strip()
    for maker, code in _CSV_MAKERS:
        if full.lower().startswith(maker.lower() + ' '):
            return maker, code, full[len(maker):].strip()
    return '', '', full


def _insurance(content):
    """`Lifetime Insurance` → (True, 0) · `120 Month Insurance` → (False, 120)."""
    text = (content or '').strip()
    if 'lifetime' in text.lower():
        return True, 0
    match = _MONTHS.match(text)
    return False, int(match.group(1)) if match else 0


def _from_extension_csv(rows):
    """Der CSV-Export der **Hangar Extension** — der Komplett-Export je Pledge.

    Eine Zeile je **Inhalt** eines Pledges: Schiffe, Farben, Ausrüstung,
    Versicherung. Zusammengehalten über `Pledge ID`. Gemessen an einem echten
    Export (751 Zeilen, 42 Schiffe):

    | `Content Type` | was es ist | wird |
    |---|---|---|
    | `Ship` | `Aegis Idris-P` — Hersteller und Name in einem Feld | ein Schiff |
    | `Included Ship` | Beilage eines Pakets (`ARGO MPUV Personnel`) | ein Schiff, `paket` = das Schiff des Pledges, sonst der Pledge-Name |
    | `Insurance` | `Lifetime Insurance`, `120 Month Insurance` — **eine je Pledge** | `lti` bzw. `versicherung` (Monate) für jedes Schiff des Pledges |
    | alles andere | Farben, Anzüge, Möbel, Gutscheine | übergangen |

    ⭐ **Das ist die Datei mit der Versicherungsdauer.** Der JSON-Export der
    Erweiterung kennt sie (noch) nicht — dafür kennt er Kürzel und
    Paketbeziehung. Wer beide einliest, bekommt beides; die Doppelerkennung
    (`_same_ship`) führt die Angaben zusammen.

    ⚠ `Pledge ID` und `Pledge Cost` sind privat — sie bleiben im Hangar und
    kommen nie in einen Bericht (siehe Modulkopf).
    """
    pledges = {}
    order = []
    for row in rows:
        pid = (row.get('Pledge ID') or '').strip()
        if pid not in pledges:
            pledges[pid] = {'ships': [], 'included': [], 'lti': False,
                            'months': 0,
                            'name': (row.get('Pledge Name') or '').strip(),
                            'date': (row.get('Pledge Date') or '').strip(),
                            'cost': (row.get('Pledge Cost') or '').strip()}
            order.append(pid)
        kind = (row.get('Content Type') or '').strip().lower()
        content = (row.get('Pledge Content') or '').strip()
        if kind == 'ship' and content:
            pledges[pid]['ships'].append(content)
        elif kind == 'included ship' and content:
            pledges[pid]['included'].append(content)
        elif kind == 'insurance':
            lti, months = _insurance(content)
            pledges[pid]['lti'] = pledges[pid]['lti'] or lti
            pledges[pid]['months'] = max(pledges[pid]['months'], months)
    result = []
    for pid in order:
        p = pledges[pid]
        for full, included in ([(s, False) for s in p['ships']]
                               + [(s, True) for s in p['included']]):
            maker, code, name = _split_maker(full)
            if not name:
                continue
            # Beilage: bei genau einem Schiff im Pledge ist das der Träger;
            # bei einem Paket mit vielen Schiffen bleibt es der Paketname.
            if included and len(p['ships']) == 1:
                paket = _split_maker(p['ships'][0])[2]
            else:
                paket = p['name']
            result.append({
                'name': name, 'hersteller': maker, 'kurz': '', 'hkurz': code,
                'lti': p['lti'], 'warbond': False, 'paket': paket,
                'gekauft': p['date'], 'preis': p['cost'],
                # Monate nur ohne LTI — LTI ist die Dauer.
                'versicherung': (p['months'] if p['months'] and not p['lti']
                                 else None),
            })
    return result


def _from_csv(text):
    """Der CSV-Export — vom Hangar XPLORer **oder** von der Hangar Extension.

    Erkannt an der Kopfzeile: Die Extension schreibt `Pledge ID` und
    `Content Type`, der XPLORer `Manufacturer, Ship, Lti, …`.

    ⚠ Die XPLORer-Kopfzeile trägt **Leerzeichen hinter den Kommas**. Ohne
    `skipinitialspace` heißt die zweite Spalte `' Ship'` und wird nie
    gefunden.
    """
    reader = csv.DictReader(io.StringIO(text), skipinitialspace=True)
    fields = [(f or '').strip() for f in (reader.fieldnames or [])]
    if 'Pledge ID' in fields and 'Content Type' in fields:
        return _from_extension_csv(list(reader))
    result = []
    for row in reader:
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
        errors.record('fleet.read', exc)
        return [], str(exc)
    head = text.lstrip()[:1]
    try:
        # `{` = StarJump FleetViewer (ein Objekt), `[` = alle übrigen JSON.
        entries = _from_json(text) if head in '[{' and head else \
            _from_csv(text)
    except Exception as exc:
        errors.record('fleet.read.parse', exc)
        return [], str(exc)
    return entries, ''


def import_entries(entries, data=None, save_now=True):
    """Gelesene Einträge in den Hangar übernehmen.

    Gibt `(neu, schon_da, ausgetragen)` zurück — `ausgetragen` sind die Namen.

    ⚠ **Ein Export ist der ganze Pledge-Hangar, keine Ergänzung.** Käme nur
    hinzu, was neu ist, bliebe nach einem Upgrade (etwa Prospector ➔ Sabre
    Raven EX) das alte Schiff im Hangar stehen. Deshalb fällt ein
    Echtgeld-Schiff heraus, das im Export fehlt. Im Spiel gekaufte (`INGAME`)
    bleiben: Die kennt kein Export.
    """
    data = data if data is not None else load()
    new = 0
    # ⚠⚠ Nur ein **Pledge-Export** ist der ganze Hangar. Eine Schiffsliste
    # (Fleetview, Fleetyards, StarJump — Herkunft `LIST`) ergänzt nur.
    pledge_export = all((e.get('herkunft') or PLEDGE) == PLEDGE
                        for e in entries)
    wishes = [e for e in entries if e.get('wunsch')]
    for e in entries:
        origin = e.get('herkunft') or PLEDGE
        if e.get('wunsch'):
            continue
        if origin == LIST and not e.get('hkurz') and not e.get('hersteller'):
            # Ohne Hersteller greift `_same_ship` nicht (Name allein reicht
            # dort bewusst nicht). Steht genau EIN Schiff dieses Namens schon
            # drin, ist es dieses — sonst stünde es gleich doppelt da.
            same = [s for s in (data.get('schiffe') or [])
                    if _slim(s.get('name')) == _slim(e.get('name'))]
            if len(same) == 1:
                continue
        is_new = add(data, e.get('name'), e.get('hersteller'),
                     origin=origin, kurz=e.get('kurz'),
                     hkurz=e.get('hkurz'), lti=e.get('lti'),
                     warbond=e.get('warbond'), paket=e.get('paket'),
                     gekauft=e.get('gekauft'), preis=e.get('preis'),
                     versicherung=e.get('versicherung'))
        if is_new:
            new += 1
            if e.get('quelle'):
                data['schiffe'][-1]['quelle'] = e['quelle']
    # Fleetyards führt auch Wunschschiffe (`wanted`) — die gehören auf die
    # Wunschliste, nicht in den Hangar. ⚠ Erst NACH dem Hangar: Kam dasselbe
    # Schiff in dieser Datei auch als eigenes herein, gehört es nicht auf die
    # Wunschliste.
    for e in wishes:
        if not contains(data, e.get('name'), e.get('hersteller'),
                        hkurz=e.get('hkurz')) \
                and not any(_slim(s.get('name')) == _slim(e.get('name'))
                            for s in (data.get('schiffe') or [])):
            wishlist_add(data, e.get('name'), e.get('hersteller'))
    # Wunschschiffe zählen weder als übernommen noch als schon vorhanden.
    wished = len(wishes)
    removed = []
    if entries and pledge_export:
        kept = []
        for ship in (data.get('schiffe') or []):
            in_export = any(_same_ship(ship, e.get('name'), e.get('hersteller'),
                                       e.get('kurz'), e.get('hkurz'))
                            for e in entries)
            if ship.get('herkunft') == PLEDGE and not in_export:
                removed.append(ship.get('name') or '')
            else:
                kept.append(ship)
        if removed:
            data['schiffe'] = kept
    if save_now:
        save(data)
    return new, len(entries) - new - wished, removed


def unknown(data=None):
    """Welche Schiffe im Hangar erkul **nicht** kennt.

    ⚠ Das ist meistens **keine Panne**, sondern eine Auskunft: Erkul führt nur
    Schiffe, die im Spiel flugfähig sind. Ein Treffer hier heißt in aller Regel,
    dass es das Schiff im Spiel noch nicht gibt — etwa Crucible, Endeavor,
    Galaxy, Liberator, Merchantman.
    """
    result = []
    for s in ((data or load()).get('schiffe') or []):
        if not erkul.knows(s.get('name'), s.get('hersteller'), s.get('kurz'),
                           s.get('hkurz')):
            result.append(s.get('name') or '')
    return result


def wishlist_id_sets(data=None):
    """Dasselbe für die Wunschliste — je Eintrag `(name, hersteller, '', '')`.

    ⚠ **Getrennt von `id_sets()`, mit Absicht.** Beide Listen holen dieselbe
    Art Daten, dürfen aber nie in denselben Topf: `id_sets()` speist auch die
    Auskunft, ob ein Teil ins eigene Schiff passt, und ein Wunschschiff hat
    man nicht. Wer
    die Listen hier zusammenlegt, beantwortet dort eine Frage über fremdes
    Eigentum.

    ⚠ **Ein fehlender Hersteller wird nachgeschlagen.** Ältere Einträge der
    Wunschliste haben keinen — und ohne ihn findet `erkul`
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

    ⚠ **Die Wunschliste zählt mit.** Wer sich ein Schiff vornimmt, plant oft
    gleich die Ausstattung mit. Ohne Steckplatz-Daten steht auf der Wunschzeile nur eine Kaufsumme, und
    die Ausstattung ließe sich erst planen, wenn das Schiff schon gekauft ist.
    Geholt wird nur — verrechnet werden Wunschschiffe nirgends als Besitz.
    """
    return erkul.add_missing(id_sets(data) + wishlist_id_sets(data))
