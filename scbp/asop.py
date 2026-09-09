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
Eigene Schiffsnamen im Fleet Manager (ASOP).

Im Fleet Manager stehen die Schiffe mit ihrem Werksnamen. Wer mehrere
Abwandlungen derselben Reihe hat, unterscheidet sie beim Abrufen nicht — und
die Liste ist genau der Moment, in dem man sich entscheiden muss.

Der Watcher schreibt ohnehin in die `global.ini`; dort stehen die Namen als
`vehicle_Name…`-Schlüssel. Es wird also **nichts berechnet und nichts aus der
`Data.p4k` geholt** — es wird ein vorhandener Text ersetzt.

## ⚠⚠ Was das NICHT kann — und zwar grundsätzlich

Ein `vehicle_Name…`-Schlüssel gehört zum **Muster**, nicht zum einzelnen
Schiff. Wer zwei *gleiche* Hornets besitzt, bekommt für beide denselben Namen;
das Spiel kennt an dieser Stelle keinen Unterschied. Unterscheiden lassen sich
**Abwandlungen** (F7C, F7C-M, F7A, Mk I gegen Mk II) — und das ist der Fall,
der im Fleet Manager wirklich weh tut.

Das gilt für jedes Werkzeug, das über die Sprachdatei geht, auch für fremde.
Es steht hier, damit niemand später eine Funktion sucht, die es nicht geben
kann.

## Die Zuordnung — gemessen, nicht geraten (09.09.2026)

Der eigene Hangar führt Klartextnamen (`Ursa Medivac`) und einen Kurznamen aus
dem Pledge-Export (`RSI_Ursa`). Die `global.ini` führt Schlüssel
(`vehicle_NameRSI_URSA_Medivac`). Beides trifft sich nicht immer.

An einem echten Hangar mit 41 Schiffen gegen 655 Schlüssel gemessen:

| Weg | Treffer |
|---|---|
| 1 Kurzname ist der Schlüssel | 36 |
| 3 Wert endet auf den Namen | 3 (Railen/Gatac, Ursa Medivac, F7C-M Mk II) |
| 4 Schlüssel beginnt mit dem Kurznamen | 1 (C8R Pisces → …_Rescue) |
| ohne Zuordnung | 1 (Paladin — steht gar nicht in der Datei) |

⚠ **Jede unscharfe Stufe liefert nur bei GENAU EINEM Treffer etwas.** Bei
mehreren wird nichts zurückgegeben. Ein falsch benanntes Schiff ist schlimmer
als ein unbenanntes: Der Spieler ruft dann im Ernstfall das falsche ab, und die
Ursache steht in einer 12-MB-Datei.

⚠ Und der Kurzname kann selbst falsch sein: Im gemessenen Hangar trug die
**Mk II** den Kurznamen `ANVL_F7C_M_Super_Hornet_Mk_I`. Gerettet hat das der
Klartextname — deshalb ist die Leiter mehrstufig und nicht ein Nachschlagen.
"""
import json
import os
import re

from . import fehler, pfade

DATEI = 'asop.json'
FORMAT = 1

VORSATZ = 'vehicle_Name'
KURZ_ENDE = '_short'

# Das Sternchen als Merker vor dem Namen. Ein Zeichen, das im Fleet Manager
# sofort auffällt und in keinem Werksnamen vorkommt.
STERN = '*'

# ⚠ Obergrenze für einen eigenen Namen. Nicht willkürlich: Der längste
# Werksname in der gemessenen Datei hat 38 Zeichen, und der Fleet Manager
# schneidet ab, statt umzubrechen. Wer 200 Zeichen einträgt, sieht im Spiel
# weniger als vorher.
MAX_LAENGE = 40


def pfad():
    return pfade.app_datei(DATEI)


def leer():
    return {'format': FORMAT, 'namen': {}}


def laden():
    """Die eigenen Namen — oder eine leere Sammlung."""
    try:
        with open(pfad(), encoding='utf-8') as f:
            daten = json.load(f)
        if daten.get('format') == FORMAT and isinstance(daten.get('namen'), dict):
            return daten
    except FileNotFoundError:
        pass
    except Exception as ausnahme:
        fehler.merken('asop.laden', ausnahme)
    return leer()


def speichern(daten):
    """Atomar ablegen. Gibt zurück, ob es geklappt hat.

    ⚠ Der Rückgabewert wird ausgewertet — ein stilles `False` ist genau der
    Fehler, der an anderer Stelle monatelang dafür sorgte, dass eine nicht
    gespeicherte Einstellung nach dem Neustart wieder alt war.
    """
    ziel = pfad()
    try:
        os.makedirs(os.path.dirname(ziel), exist_ok=True)
        with open(ziel + '.tmp', 'w', encoding='utf-8') as f:
            json.dump(daten, f, ensure_ascii=False, indent=1)
        os.replace(ziel + '.tmp', ziel)
        return True
    except Exception as ausnahme:
        fehler.merken('asop.speichern', ausnahme)
        return False


def setzen(daten, schluessel, name, stern=False):
    """Einen eigenen Namen eintragen — oder wieder löschen.

    Ein leerer Name **ohne** Stern löscht den Eintrag: Wer das Feld leert, will
    den Werksnamen zurück, und ein leerer Eintrag in der Datei wäre nur Ballast.
    """
    namen = daten.setdefault('namen', {})
    name = (name or '').strip()[:MAX_LAENGE]
    if not name and not stern:
        namen.pop(schluessel, None)
    else:
        namen[schluessel] = {'name': name, 'stern': bool(stern)}
    return daten


def eintrag(daten, schluessel):
    e = (daten.get('namen') or {}).get(schluessel) or {}
    return (e.get('name') or ''), bool(e.get('stern'))


def anzahl(daten=None):
    return len((daten or laden()).get('namen') or {})


# ------------------------------------------------------------ Zuordnung

def _schlank(text):
    return re.sub(r'[^a-z0-9]', '', (text or '').lower())


def schluessel_lesen(zeilen):
    """Aus den Zeilen der `global.ini` die Fahrzeugnamen holen.

    Gibt `{schluessel: wert}` für die **langen** Namen zurück. Die
    `…_short`-Fassungen bleiben draußen: Sie stehen im Fleet Manager nicht, und
    wer beide anfasst, hat den Namen zweimal zu pflegen.
    """
    tabelle = {}
    for zeile in zeilen:
        if not zeile.startswith(VORSATZ) or '=' not in zeile:
            continue
        schluessel, wert = zeile.split('=', 1)
        # ⚠ Ein Schlüssel kann einen Zusatz tragen (`,P=…`). Der gehört nicht
        # zum Namen — `_zeile_zerlegen` in `injektion.py` trennt ihn ebenso ab.
        if ',' in schluessel:
            continue
        if schluessel[len(VORSATZ):].lower().endswith(KURZ_ENDE):
            continue
        tabelle[schluessel] = wert.rstrip('\r\n')
    return tabelle


def zuordnen(schiffe, tabelle):
    """Zu jedem Schiff den passenden `vehicle_Name`-Schlüssel suchen.

    Gibt eine Liste `{'name', 'kurz', 'schluessel', 'werksname', 'weg'}` zurück,
    nach Namen sortiert. Ohne Zuordnung bleibt `schluessel` leer — dann sagt die
    Oberfläche das, statt zu raten.
    """
    nach_schluessel, nach_wert = {}, {}
    for schluessel, wert in tabelle.items():
        nach_schluessel.setdefault(_schlank(schluessel[len(VORSATZ):]), schluessel)
        nach_wert.setdefault(_schlank(wert), schluessel)

    ergebnis = []
    for s in schiffe:
        name = (s.get('name') or '').strip()
        if not name:
            continue
        kurz = (s.get('kurz') or '').strip()
        schluessel, weg = _leiter(name, kurz, tabelle, nach_schluessel, nach_wert)
        ergebnis.append({'name': name, 'kurz': kurz, 'schluessel': schluessel or '',
                         'werksname': tabelle.get(schluessel, '') if schluessel else '',
                         'weg': weg})
    ergebnis.sort(key=lambda e: e['name'].lower())
    return ergebnis


def _leiter(name, kurz, tabelle, nach_schluessel, nach_wert):
    """Genau zuerst, unscharf zuletzt — und nie bei mehreren Kandidaten.

    Die Reihenfolge ist an echten Daten entstanden (siehe Modulkopf); jede
    Stufe hat dort mindestens einen Fall, den keine frühere löst.
    """
    n, k = _schlank(name), _schlank(kurz)
    schluessel = nach_schluessel.get(k) if k else None
    if schluessel:
        return schluessel, 'kurz'
    schluessel = nach_wert.get(n)
    if schluessel:
        return schluessel, 'name'
    if n:
        treffer = [s for s, w in tabelle.items() if _schlank(w).endswith(n)]
        if len(treffer) == 1:
            return treffer[0], 'wertende'
    if k:
        treffer = [s for s in tabelle
                   if _schlank(s[len(VORSATZ):]).startswith(k)]
        if len(treffer) == 1:
            return treffer[0], 'kurzanfang'
    if n:
        treffer = [s for s, w in tabelle.items() if n in _schlank(w)]
        if len(treffer) == 1:
            return treffer[0], 'imwert'
        if len(treffer) > 1:
            return None, 'mehrdeutig'
    return None, 'nichts'


# ------------------------------------------------------------ Einspielen

def anzeigename(werksname, eigener, stern):
    """Wie der Name im Spiel stehen soll.

    Ohne eigenen Namen bleibt der Werksname — nur der Stern kommt davor. So
    kann man ein Schiff markieren, ohne es umzutaufen.
    """
    grund = (eigener or '').strip() or (werksname or '')
    # ⚠ Einen schon vorhandenen Stern abschneiden. Sonst steht nach dem zweiten
    # Einspielen `**Name` da — und nach dem dritten `***Name`.
    grund = grund.lstrip(STERN).strip()
    return (STERN + grund) if stern else grund


def tabelle_bauen(zeilen, daten=None):
    """`{schluessel: (eigener Name, Stern)}` für die Injektion — oder leer.

    ⚠⚠ **Hier steht der Wunsch, nicht der fertige Text.** Den Werksnamen setzt
    die Injektion ein, und zwar den **zurückgesetzten** — sie hat die Zeile
    vorher durch `_saeubern()` geschickt. Würde hier schon ein fertiger Wert
    gebaut, käme der Werksname aus der *laufenden* Datei: Bei einem zweiten
    Lauf wäre das unser eigener Text von vorhin, und ein Stern setzte sich vor
    den Stern.

    ⚠ Leer heißt „nichts anfassen". Das ist der Regelfall: Wer keine eigenen
    Namen vergeben hat, soll auch keine geänderte Zeile in seiner `global.ini`
    haben.
    """
    daten = daten if daten is not None else laden()
    namen = daten.get('namen') or {}
    if not namen:
        return {}
    vorhanden = schluessel_lesen(zeilen)
    fertig = {}
    for schluessel, e in namen.items():
        if schluessel not in vorhanden:
            # Der Schlüssel ist aus der Datei verschwunden (anderer Patch,
            # andere Sprachquelle). Dann gibt es nichts zu setzen — und der
            # Eintrag bleibt trotzdem stehen, falls er wiederkommt.
            continue
        name, stern = (e.get('name') or ''), bool(e.get('stern'))
        if name or stern:
            fertig[schluessel] = (name, stern)
    return fertig
