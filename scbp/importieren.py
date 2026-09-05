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

Dieses Modul **entscheidet nichts allein**: `vorschau()` sagt, was passieren
würde; erst `uebernehmen()` schreibt.
"""
import json
import os
import re
import time

from . import bestand as bestand_datei
from . import fehler

QUELLE = 'import'


def _entklammert(name):
    """Name ohne den Klammer-Zusatz am Ende — für den Notfall-Abgleich."""
    return re.sub(r'\s*\([^()]*\)\s*$', '', name or '').strip()


def erkennen(daten):
    """Aus welchem Format stammt die geladene Datei? Sonst None."""
    if not isinstance(daten, dict):
        return None
    if daten.get('werkzeug') == 'SC BP Watcher' or 'bauplaene' in daten:
        return 'eigen'
    liste = daten.get('blueprints')
    if isinstance(liste, list) and liste:
        erster = liste[0] if isinstance(liste[0], dict) else {}
        if 'key' in erster:
            return 'launcher'
        if 'exportSchemaVersion' in daten or 'ts' in erster:
            return 'scmdb'
        if 'productName' in erster:
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
        if 'tag' in erster and 'name' in erster:
            return 'scmdb2'
    return None


def _zeit_aus(wert):
    """Einen Zeitwert in unsere Schreibweise bringen — oder nichts."""
    try:
        if isinstance(wert, (int, float)) and wert > 0:
            return time.strftime('%Y-%m-%d %H:%M:%S', time.localtime(wert))
        if isinstance(wert, str) and wert.strip():
            roh = wert.strip().replace('Z', '').replace('T', ' ')
            return roh[:19]
    except Exception:
        pass
    return None


def lesen(pfad):
    """Eine Datei einlesen. Gibt (art, [{name, zeit}, …]) zurück.

    Bei einer unlesbaren oder unbekannten Datei ist die Art None — die Meldung
    dazu gehört in die Oberfläche, nicht in eine Ausnahme mitten im Ablauf.
    """
    try:
        with open(pfad, encoding='utf-8-sig') as f:
            daten = json.load(f)
    except Exception as ausnahme:
        fehler.merken('importieren.lesen', ausnahme, os.path.basename(pfad or ''))
        return None, []

    art = erkennen(daten)
    eintraege = []
    if art == 'eigen':
        for e in daten.get('bauplaene') or []:
            if isinstance(e, dict) and e.get('name'):
                eintraege.append({'name': e['name'], 'zeit': _zeit_aus(e.get('zeit'))})
    elif art in ('scmdb', 'basetool'):
        for e in daten.get('blueprints') or []:
            if isinstance(e, dict) and e.get('productName'):
                eintraege.append({'name': e['productName'],
                                  'zeit': _zeit_aus(e.get('ts') or e.get('receivedAt'))})
    elif art == 'scmdb2':
        # ⚠⚠ **Nur, was als erledigt markiert ist.** Die Ausfuhr enthaelt auch
        # Bauplaene, die jemand nur beobachtet oder angesehen hat; `completed`
        # ist das Feld, das „habe ich" bedeutet. Ohne diese Bedingung waere
        # jeder Bauplan der Datenbank im Bestand — und das Werkzeug meldete
        # nie wieder einen Fund.
        #
        # ⚠ Einen Zeitpunkt gibt es in diesem Format nicht. Lieber keiner als
        # ein erfundener: Der Bestand kommt damit zurecht.
        for e in daten.get('blueprints') or []:
            if isinstance(e, dict) and e.get('name') and e.get('completed'):
                eintraege.append({'name': e['name'], 'zeit': None})
    elif art == 'launcher':
        for e in daten.get('blueprints') or []:
            if isinstance(e, dict) and e.get('key'):
                eintraege.append({'name': e['key'], 'zeit': None})
    return art, eintraege


def vorschau(eintraege, daten=None, katalog_namen=None):
    """Was würde passieren? Ändert nichts.

    Rückgabe: dict mit `neu`, `schon_da`, `unbekannt` (Namen) und `gesamt`.
    `unbekannt` sind Namen, die der Katalog nicht kennt — sie kommen trotzdem
    mit, stehen aber getrennt, damit die Fortschrittszahl erklärbar bleibt.
    """
    daten = daten if daten is not None else bestand_datei.laden()
    vorhanden = set(daten.get('bauplaene') or {})
    bekannt = {bestand_datei.norm(n) for n in (katalog_namen or [])}
    ohne_klammer = {bestand_datei.norm(_entklammert(n)) for n in (katalog_namen or [])}

    neu, schon_da, unbekannt, gesehen = [], [], [], set()
    for e in eintraege:
        schluessel = bestand_datei.norm(e.get('name'))
        if not schluessel or schluessel in gesehen:
            continue
        gesehen.add(schluessel)
        if schluessel in vorhanden:
            schon_da.append(e['name'])
            continue
        neu.append(e['name'])
        if bekannt and schluessel not in bekannt:
            # Zweiter Versuch ohne Klammer-Zusatz — aber nur, wenn er eindeutig
            # ist. Sonst würden `Singe Cannon (S1)/(S2)/(S3)` verschmelzen.
            kurz = bestand_datei.norm(_entklammert(e['name']))
            if kurz not in ohne_klammer:
                unbekannt.append(e['name'])

    return {'neu': neu, 'schon_da': schon_da, 'unbekannt': unbekannt,
            'gesamt': len(gesehen)}


def uebernehmen(eintraege, daten=None, speichern=True):
    """Die Einträge in den Bestand aufnehmen. Gibt die Zahl der neuen zurück.

    Zusammenführen: Vorhandenes bleibt unangetastet. Ein Zeitpunkt aus der Datei
    wird übernommen — er ist genauer als „jetzt gerade eingelesen".
    """
    daten = daten if daten is not None else bestand_datei.laden()
    dazu = 0
    for e in eintraege:
        if bestand_datei.hinzufuegen(daten, e.get('name'), QUELLE, e.get('zeit')):
            dazu += 1
    if speichern and dazu:
        bestand_datei.speichern(daten)
    return dazu


if __name__ == '__main__':
    import sys
    if len(sys.argv) < 2:
        print('Aufruf: python3 -m scbp.importieren <datei.json>')
        sys.exit(2)
    art, eintraege = lesen(sys.argv[1])
    print('Format:', art or 'nicht erkannt', '·', len(eintraege), 'Einträge')
    v = vorschau(eintraege)
    print('neu: %d · schon da: %d' % (len(v['neu']), len(v['schon_da'])))
