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
Einen vorhandenen Bauplan-Bestand einlesen.

**Wozu:** Der Watcher baut seinen Bestand aus den Spielprotokollen auf. Wer die
nicht mehr hat — neuer Rechner, aufgeräumte Platte, frisch dazugestoßen — steht
vor einer leeren Liste, obwohl er die Baupläne längst besitzt. Wer seinen Stand
anderswo gepflegt hat (KRT Profit Basetool, scmdb.net, SC Deutsch Launcher, eine
eigene Sicherung), soll ihn hier einlesen können.

**Vier Formate, keine Formatfrage.** Der Spieler wählt eine *Datei*; woher sie
stammt, erkennt das Programm am Inhalt:

  | Format | Erkennungsmerkmal |
  |---|---|
  | eigene Sicherung | `werkzeug: "SC BP Watcher"`, Liste `bauplaene` |
  | scmdb.net (älter) | `exportSchemaVersion`, `blueprints[].productName` + `ts` |
  | scmdb.net (neuer) | `blueprints[].tag` + `name`, nur `completed: true` |
  | KRT Profit Basetool | `blueprints[].productName` (+ `receivedAt`) |
  | SC Deutsch Launcher | `blueprints[].key` |
  | Baupläne DB (Star Citizen Deutsch) | `blueprints[].key` + `isDone`, nur `isDone: true` |

**Zusammenführen, nie ersetzen.** Vorhandenes bleibt, Neues kommt dazu. Wer
wirklich ersetzen will, setzt vorher den Bestand zurück — dafür gibt es einen
eigenen Knopf. Ein Import, der stillschweigend überschreibt, kann einen mühsam
gesammelten Bestand vernichten.

**Unbekannte Namen kommen mit.** Steht ein Name nicht im Katalog (alte
Schreibweise, Tippfehler, ein Bauplan, den scmdb noch nicht führt), wird er
trotzdem übernommen und **gekennzeichnet**. Ein Eintrag zu wenig ist schlimmer
als einer zu viel: Wer denkt, ihm fehle ein Bauplan, jagt ihn ein zweites Mal.
Vorher wird versucht, ihn über den Klammer-Zusatz zuzuordnen — dieselbe Falle
wie `(12 Schuss)` gegen `(12 cap)`.

Dieses Modul **entscheidet nichts allein**: `preview()` sagt, was passieren
würde; erst `merge()` schreibt.

⚠ Bis zum 11.09.2026 hieß dieses Modul `importieren`, die Funktionen
`erkennen`, `lesen`, `vorschau` und `uebernehmen` (Sprachumstellung P4,
Stufe 1). **Nur Bezeichner sind umbenannt, keine Zeichenketten.** Bewusst gleich
geblieben: die Formatkennungen (`'eigen'`, `'scmdb'`, `'scmdb2'`, `'basetool'`,
`'launcher'`, `'bpdb'`) — `seiten.py` holt darüber die Bezeichnung für
die Anzeige —, die
Schlüssel `name` und `zeit` der Einträge, die Schlüssel `neu`, `schon_da`,
`unbekannt` und `gesamt` der Vorschau und der Quellwert `'import'`, der im
Bestand jedes Nutzers steht.
"""
import json
import os
import re
import time

from . import collection
from . import fehler

SOURCE = 'import'

# ⚠⚠ Unter welchen Namen wir unsere EIGENEN Exportdateien wiedererkennen.
# BEIDE gelten dauerhaft (Umbenennung zu VerseKit, 12.09.2026):
#   * 'VerseKit'      — was `export.py` ab jetzt schreibt
#   * 'SC BP Watcher' — was in jeder vor der Umbenennung erzeugten Datei steht
# ⛔ Keinen streichen. Der Auftrag verlangt ausdrücklich, dass bestehende
# Sicherungen und Exporte weiter importierbar bleiben.
EIGENE_WERKZEUGNAMEN = ('VerseKit', 'SC BP Watcher')


def _strip_suffix(name):
    """Name ohne den Klammer-Zusatz am Ende — für den Notfall-Abgleich."""
    return re.sub(r'\s*\([^()]*\)\s*$', '', name or '').strip()


def detect(data):
    """Aus welchem Format stammt die geladene Datei? Sonst None."""
    if not isinstance(data, dict):
        return None
    if data.get('werkzeug') in EIGENE_WERKZEUGNAMEN or 'bauplaene' in data:
        return 'eigen'
    items = data.get('blueprints')
    if isinstance(items, list) and items:
        first = items[0] if isinstance(items[0], dict) else {}
        if 'key' in first:
            # ⚠⚠ **Die „Baupläne DB" von Star Citizen Deutsch** — die
            # Bauplan-Übersicht im Browser, unter
            # `rjcncpt.github.io/StarCitizen-Deutsch-INI/`.
            #
            # ⛔ **Das heißt NICHT „der Launcher ist jetzt eine Webseite".**
            # Der SC Deutsch Launcher bleibt ein Programm; die Bauplan-
            # Übersicht ist eine eigene Seite daneben. Genau diese
            # Falschaussage stand hier am 13.09.2026 einen Tag lang, auch im
            # Changelog.
            #
            # Sie schreibt dieselben `key`-Einträge wie das Launcher-Programm,
            # hängt aber an jeden zwei Schalter:
            #
            #     Programm:  {"blueprints": [{"key": …}]}
            #     DB-Seite:  {"exported": …, "mode": "vollständig",
            #                 "blueprints": [{"key": …, "isDone": …,
            #                                 "isMarked": …}]}
            #
            # `isDone` heißt „habe ich", `isMarked` heißt „will ich".
            #
            # ⚠ Der Unterschied ist nicht kosmetisch: Die Seite bietet „Alle
            # als JSON" an — dann steht **jeder** Bauplan der Datenbank in der
            # Datei, die allermeisten mit `isDone: false`. Ohne diese
            # Unterscheidung landete die halbe Datenbank im Bestand, und das
            # Werkzeug meldete nie wieder einen Fund. Dieselbe Falle wie
            # `completed` bei `scmdb2`.
            #
            # ⚠ Gesucht wird über **alle** Einträge, nicht nur den ersten:
            # Welcher Bauplan vorn steht, entscheidet die Sortierung der
            # Seite, nicht das Format.
            if any(isinstance(e, dict) and ('isDone' in e or 'isMarked' in e)
                   for e in items):
                return 'bpdb'
            return 'launcher'
        if 'exportSchemaVersion' in data or 'ts' in first:
            return 'scmdb'
        if 'productName' in first:
            return 'basetool'
        # ⚠⚠ **Die neuere Ausfuhr von scmdb.net** (dort „Tracking-Export").
        # Am 05.09.2026 gemeldet: Eine Datei von einem Mitspieler wurde mit
        # „Diese Datei kenne ich nicht" abgewiesen — zu Recht, denn scmdb hat
        # das Format gewechselt und wir kannten nur das alte:
        #
        #     alt:  {"exportSchemaVersion": …, "blueprints": [{"productName": …, "ts": …}]}
        #     neu:  {"version": 3, "blueprints": [{"tag": …, "name": …, "completed": true}]}
        #
        # ⚠ Erkannt wird an `tag` + `name`, nicht an `version`: Die Zahl waere
        # beim naechsten Formatwechsel wieder eine andere, die Felder bleiben.
        if 'tag' in first and 'name' in first:
            return 'scmdb2'
    return None


def _time_from(value):
    """Einen Zeitwert in unsere Schreibweise bringen — oder nichts."""
    try:
        if isinstance(value, (int, float)) and value > 0:
            return time.strftime('%Y-%m-%d %H:%M:%S', time.localtime(value))
        if isinstance(value, str) and value.strip():
            raw = value.strip().replace('Z', '').replace('T', ' ')
            return raw[:19]
    except Exception:
        pass
    return None


def read(path):
    """Eine Datei einlesen. Gibt (art, [{name, zeit}, …]) zurück.

    Bei einer unlesbaren oder unbekannten Datei ist die Art None — die Meldung
    dazu gehört in die Oberfläche, nicht in eine Ausnahme mitten im Ablauf.
    """
    try:
        with open(path, encoding='utf-8-sig') as f:
            data = json.load(f)
    except Exception as exc:
        fehler.merken('importer.read', exc, os.path.basename(path or ''))
        return None, []

    kind = detect(data)
    entries = []
    if kind == 'eigen':
        for e in data.get('bauplaene') or []:
            if isinstance(e, dict) and e.get('name'):
                entries.append({'name': e['name'], 'zeit': _time_from(e.get('zeit'))})
    elif kind in ('scmdb', 'basetool'):
        for e in data.get('blueprints') or []:
            if isinstance(e, dict) and e.get('productName'):
                entries.append({'name': e['productName'],
                                'zeit': _time_from(e.get('ts') or e.get('receivedAt'))})
    elif kind == 'scmdb2':
        # ⚠⚠ **Nur, was als erledigt markiert ist.** Die Ausfuhr enthaelt auch
        # Bauplaene, die jemand nur beobachtet oder angesehen hat; `completed`
        # ist das Feld, das „habe ich" bedeutet. Ohne diese Bedingung waere
        # jeder Bauplan der Datenbank im Bestand — und das Werkzeug meldete
        # nie wieder einen Fund.
        #
        # ⚠ Einen Zeitpunkt gibt es in diesem Format nicht. Lieber keiner als
        # ein erfundener: Der Bestand kommt damit zurecht.
        for e in data.get('blueprints') or []:
            if isinstance(e, dict) and e.get('name') and e.get('completed'):
                entries.append({'name': e['name'], 'zeit': None})
    elif kind == 'launcher':
        for e in data.get('blueprints') or []:
            if isinstance(e, dict) and e.get('key'):
                entries.append({'name': e['key'], 'zeit': None})
    elif kind == 'bpdb':
        # ⚠⚠ **Nur, was als erspielt markiert ist.** `isMarked` ist ein
        # Wunschzettel, kein Besitz — wer seine vorgemerkten Baupläne ausgibt
        # und hier einliest, hätte sonst genau die im Bestand, die er gerade
        # erst sucht. Und `isDone: false` heißt schlicht „habe ich nicht".
        #
        # ⚠ Die DB-Seite schreibt die englische Mengenangabe (`(8 Cap)`),
        # die Log-Nachlese die des Spiels (`(8 Schuss)`). Das gleicht
        # `collection.norm()` über `pfade.namensform()` an — hier ist dazu
        # nichts zu tun, aber es erklärt, warum beides denselben Bauplan meint.
        for e in data.get('blueprints') or []:
            if isinstance(e, dict) and e.get('key') and e.get('isDone'):
                entries.append({'name': e['key'], 'zeit': None})
    return kind, entries


def preview(entries, data=None, catalog_names=None):
    """Was würde passieren? Ändert nichts.

    Rückgabe: dict mit `neu`, `schon_da`, `unbekannt` (Namen) und `gesamt`.
    `unbekannt` sind Namen, die der Katalog nicht kennt — sie kommen trotzdem
    mit, stehen aber getrennt, damit die Fortschrittszahl erklärbar bleibt.
    """
    data = data if data is not None else collection.load()
    present = set(data.get('bauplaene') or {})
    known = {collection.norm(n) for n in (catalog_names or [])}
    known_short = {collection.norm(_strip_suffix(n)) for n in (catalog_names or [])}

    new, existing, unknown, seen = [], [], [], set()
    for e in entries:
        key = collection.norm(e.get('name'))
        if not key or key in seen:
            continue
        seen.add(key)
        if key in present:
            existing.append(e['name'])
            continue
        new.append(e['name'])
        if known and key not in known:
            # Zweiter Versuch ohne Klammer-Zusatz — aber nur, wenn er eindeutig
            # ist. Sonst würden `Singe Cannon (S1)/(S2)/(S3)` verschmelzen.
            short = collection.norm(_strip_suffix(e['name']))
            if short not in known_short:
                unknown.append(e['name'])

    return {'neu': new, 'schon_da': existing, 'unbekannt': unknown,
            'gesamt': len(seen)}


def merge(entries, data=None, save=True):
    """Die Einträge in den Bestand aufnehmen. Gibt die Zahl der neuen zurück.

    Zusammenführen: Vorhandenes bleibt unangetastet. Ein Zeitpunkt aus der Datei
    wird übernommen — er ist genauer als „jetzt gerade eingelesen".
    """
    data = data if data is not None else collection.load()
    added = 0
    for e in entries:
        if collection.add(data, e.get('name'), SOURCE, e.get('zeit')):
            added += 1
    if save and added:
        collection.save(data)
    return added


if __name__ == '__main__':
    import sys
    if len(sys.argv) < 2:
        print('Aufruf: python3 -m scbp.importer <datei.json>')
        sys.exit(2)
    kind, entries = read(sys.argv[1])
    print('Format:', kind or 'nicht erkannt', '·', len(entries), 'Einträge')
    v = preview(entries)
    print('neu: %d · schon da: %d' % (len(v['neu']), len(v['schon_da'])))
