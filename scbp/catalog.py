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
Der Bauplan-Katalog: welche Baupläne es gibt — und woher sie kommen.

Zwei Fragen beantwortet dieses Modul:

  **Was gibt es überhaupt?** 714 Baupläne (Stand 4.9.0). Das ist etwas anderes
  als alles Craftbare: Die Datei `crafting_items` zählt 1573 Gegenstände,
  aber für die meisten davon droppt nie ein Bauplan. Für eine Liste zum Abhaken
  wäre die große Zahl irreführend.

  **Woher bekomme ich einen bestimmten?** Fraktion, Auftrag, nötiger Ruf,
  Belohnung. Für 655 der 714 (92 %) ist das auflösbar. Dass X fehlt, ist die
  halbe Information; dass X bei Fraktion Y ab Rang Z droppt, ist die ganze.

Die Kette durch die Daten von scmdb.net:

    contracts[].blueprintRewards[].blueprintPool   (GUID)
        -> blueprintPools[GUID].blueprints[].name  = der Bauplan
    contracts[].factionGuid  -> factions[GUID].name
    contracts[].minStanding  -> Rang und nötige Rufpunkte
    contracts[].factionRewardsIndex -> factionRewardsPools[i] = Ruf-Gewinn

> **Die Daten werden NICHT mitgeliefert.** scmdb steht unter CC BY-NC-ND 4.0;
> eine Kopie im Repo wäre eine Weitergabe und verstieße gegen diese Lizenz wie
> gegen die GPL dieses Projekts. Geholt wird auf dem Rechner des Nutzers, so wie
> es ein Browser täte, mit ehrlicher Kennung — und nur einmal je Spielversion.
> `SC_BP_NO_NET=1` schaltet es ab.

Der Sammel-Dump ist rund 12 MB. Deshalb wird er **nicht** aufgehoben, sondern
sofort zu einer kleinen eigenen Datei eingedampft (`katalog-cache.json`, etwa
ein Zwanzigstel davon).
"""
import json
import os
import re
import time
import urllib.error
import urllib.request

from . import errors, patchhistory, paths, language
from .language import t


class Rejected(Exception):
    """Die Seite hat den Abruf abgelehnt (HTTP 403).

    Eigene Ausnahme, damit der Fall von einem echten Netzfehler zu
    unterscheiden ist: Hier ist die Leitung in Ordnung, die Gegenseite mag
    nur nicht. Wiederholen hilft nicht, und die Meldung muss eine andere
    sein — sonst sucht man den Fehler bei sich."""


# ⭐ **Zwei Adressen für dieselben Daten.**
#
# scmdb stellt einen **öffentlichen Spiegel** bereit, ausdrücklich für
# Programme. Der ist die erste Wahl, denn:
#
#   * **kein Bot-Schutz davor.** scmdb.net steht hinter Cloudflare; dessen
#     Regeln können sich ändern, ohne dass hier jemand etwas tut — und dann
#     stünde die Datenversorgung bei allen Nutzern still.
#   * GitHub Raw ist für genau diesen Zweck gedacht und stabil.
#
# scmdb.net bleibt als **Rückfall** stehen: Fällt der Spiegel aus oder hängt er
# hinterher, holt der Watcher weiter von der Originaladresse. Zwei Wege sind
# hier billig zu haben — und der Ausfall einer einzelnen Quelle legt sonst das
# ganze Werkzeug lahm.
MIRROR = 'https://raw.githubusercontent.com/KrovaxCode/SCMDB_DATA/main/data'
SCMDB = 'https://scmdb.net/data'
BASE = MIRROR
CACHE = 'katalog-cache.json'

# Aufbau-Nummer des Katalogs — **nicht** die Spielversion.
#
# ⚠ Der Katalog wird sonst nur erneuert, wenn Star Citizen eine neue Version
# bringt. Ändert sich sein *Aufbau*, weil das Programm etwas Neues hineinlegt,
# reicht das nicht: Wer schon einen Katalog auf der Platte hat, behielte den
# alten bis zum nächsten Patch — der Umbau wäre für ihn unsichtbar.
#
# **Hochzählen, sobald `_missionen()` oder `_herkunft()` etwas anders ablegen.**
#
# 3: `_contracts()` — jeder Vertrag einzeln, mit seiner eigenen Bauplanliste
# und seinem System.
#
# 4: Der Gütegrad kommt aus CIGs eigener Sprachdatei, scmdb nur als Rückfall
# (siehe `game_grades()`).
#
# 5: Auch Baupläne **ohne bekannten Weg** stehen im Katalog (`ohne_weg`).
# Ohne Hochzählen bliebe der Schalter „Auch Baupläne ohne bekannten Weg"
# wirkungslos, denn ein vorhandener Katalog kennt diese Einträge gar nicht.
#
# 6: `_missions()` legt je Auftragstext die Baupläne je System ab
# (`je_system`), für die getrennten Listen im Auftragstext.
FORMAT = 6

# Einstellung: Sollen Baupläne ohne bekannten Weg mitgezählt und angezeigt
# werden? ⛔ Standard **aus** — der Fortschritt bleibt damit die Zahl, die
# Bestandsnutzer kennen (738), und wer mehr sehen will, schaltet es ein.
SETTING_ALL = 'alle_bauplaene'
# ⚠ Geht an scmdb und UEX. Nennt BEIDE Namen — die Nutzungsfreigabe gilt dem
# alten Namen; wer danach filtert, erkennt uns weiter.
USER_AGENT = ('VerseKit/2.0 (ehemals SC-BP-Watcher) '
              '(+https://github.com/Xharig/VerseKit)')
TIMEOUT = 120
OFF = os.environ.get('SC_BP_NO_NET', '') not in ('', '0')

# Die Bezeichnungen der Arten stehen im Sprachmodul — sie sind Oberflächentext
# und müssen mit umschalten. `Char_Armor_Helmet` ist nichts für die Anzeige,
# und das deutsche Wort gehört nicht in eine englische Liste.
def reward_cap(catalog_data, blueprint_key):
    """Kann man sich diesen Bauplan durch **zu hohen Ruf** aussperren?

    Gibt `(rep_max, rang_max)` des großzügigsten Auftrags zurück — oder `None`,
    wenn mindestens ein Weg ohne Obergrenze bleibt.

    ⚠⚠ **Warum das jemand wissen will.** 280 der 353 Aufträge haben eine
    Ruf-OBERGRENZE (`maxStanding`): Steigt der Ruf bei der Fraktion darüber,
    wird der Auftrag **nicht mehr angeboten** — und seine Baupläne sind für
    diesen Spielstand endgültig weg. Wer fleißig Aufträge läuft, steigt im Rang
    und verliert dabei still den Zugang zu Bauplänen, die er noch nicht hat.
    Im Spiel steht das nirgends.

    ⚠ **Maßgeblich ist der großzügigste Weg.** Führen fünf Aufträge zu einem
    Bauplan und einer davon hat keine Obergrenze, ist nichts in Gefahr — dann
    gibt es hier `None`. Nur wenn **alle** Wege gedeckelt sind, zählt der
    höchste; bis dahin ist Zeit.

    ⚠ Der **eigene** Ruf-Stand steht nicht in der `Game.log` — nachgemessen
    über 22 Protokolle: Dort taucht `reputation` ausschließlich als
    Verbindungszeile zu CIGs `ReputationService` auf, nie ein Wert. Deshalb
    sagt diese Auskunft, ab wann der Weg zu ist, nicht wie viel Ruf noch
    bleibt.
    """
    paths = []
    for mission in (catalog_data.get('missionen') or {}).values():
        for name in mission.get('bp') or []:
            if _norm(name) == blueprint_key:
                paths.append(mission)
                break
    if not paths:
        return None
    if any(not w.get('rep_max') for w in paths):
        return None                      # ein offener Weg genügt
    highest_one = max(paths, key=lambda w: w.get('rep_max') or 0)
    return highest_one.get('rep_max'), highest_one.get('rang_max')


def blueprints_for_contract(catalog_data, title):
    """Welche Bauplan-Schlüssel gibt dieser Auftrag her? (Menge, ggf. leer)

    ⛔⛔ **Die EINE Stelle, die einen angezeigten Auftragstitel auflöst** —
    für `seiten._to_contract()` (ob anklickbar) und
    `bestandsfenster.zum_auftrag()` (Liste umstellen).

    Ein **wörtlicher** Vergleich gegen `q['auftrag']` trifft alles, was aus
    der Liste selbst kommt, und **nichts**, was aus dem Spiel kommt: In den
    Herkunftsdaten steht der Auftrag mit Platzhalter,

        'Stop Rival Attack at [LOCATION]'

    im Spiel heißt er `'Stop Rival Attack at Asteroiden Bergbaubasis'`.

    **Zwei Wege, in dieser Reihenfolge:**

    1. Der Name, wie er in den Herkunftsdaten steht. Das ist der Klick in der
       eigenen Liste, und er muss weiter funktionieren.
    2. Über den **Missionsschlüssel** (`contracts.key_for`) — derselbe
       Weg, den das Overlay seit jeher geht. Er löst Platzhalter auf.

    ⚠ Gibt es zu einem Titel auf beiden Wegen etwas, gewinnt Weg 1: Er ist
    genauer, weil er an den einzelnen Bauplänen hängt.
    """
    title = (title or '').strip()
    if not title:
        return set()
    blueprints = catalog_data.get('bauplaene') or {}
    # Weg 1 — wörtlich, wie bisher.
    treffer = {key for key, entry in blueprints.items()
               if any((q.get('auftrag') or '').strip() == title
                      for q in (entry.get('q') or []))}
    if treffer:
        return treffer
    # Weg 2 — über den Missionsschlüssel.
    try:
        from . import contracts
        schluessel = contracts.key_for(title)
        if not schluessel:
            return set()
        namen = (contracts.missions().get(schluessel) or {}).get('bp') or []
    except Exception as error:
        errors.record('katalog.auftrag_aufloesen', error)
        return set()
    # ⚠ Die Namen aus der Missionsliste sind Klartext, die Schlüssel hier sind
    # normalisiert — sonst findet sich nichts wieder.
    gesucht = {_norm(n) for n in namen if n}
    return {key for key in blueprints if key in gesucht}


def contract_traits(catalog_data, blueprint_key):
    """Was die Aufträge zu diesem Bauplan noch hergeben.

    Gibt `{'teilbar': bool|None, 'sperre': Minuten|None}` zurück — beides aus
    CIGs eigenen Vertragsdaten (`canBeShared`, `personalCooldownTime`).

    ⚠ **Teilbar nur, wenn ALLE Wege es sind.** Gilt es nur für einen von vier
    Aufträgen und steht trotzdem als teilbar da, dann
    steht die Staffel am falschen Auftrag. 334 der 353 sind teilbar, die
    Ausnahme ist also selten und genau deshalb wichtig.

    ⚠ **Die Sperre ist die kürzeste.** Sie sagt, wie schnell man es erneut
    versuchen kann; der langsamste Weg interessiert dabei niemanden.
    """
    paths = []
    for mission in (catalog_data.get('missionen') or {}).values():
        for name in mission.get('bp') or []:
            if _norm(name) == blueprint_key:
                paths.append(mission)
                break
    if not paths:
        return {'teilbar': None, 'sperre': None}
    locks = [w['cooldown'] for w in paths if w.get('cooldown')]
    return {'teilbar': all(w.get('teilbar') for w in paths),
            'sperre': min(locks) if locks else None}


def worthwhile_contracts(catalog_data, have):
    """Welcher Auftrag bringt die meisten **fehlenden** Baupläne?

    Liste aus `(titel, fraktion, anzahl, uec, rang, wo)`, absteigend nach der
    Zahl der fehlenden Baupläne.

    ⚠ **Gezählt wird über die Bezugsquellen der Baupläne, nicht über
    `missionen`.** Die Missionsliste führt ihre Aufträge nur unter dem
    Textschlüssel (`headhunters_eliminateall_cfp_M_title_001`) — daraus wird
    ohne die Sprachdatei des Spiels kein lesbarer Titel. Die Bezugsquellen am
    Bauplan tragen den **aufgelösten** Titel bereits, samt Fraktion, Rang,
    Belohnung und Annahmeort. Gleiche Auskunft, kein zusätzlicher Datenweg.
    """
    contracts = {}
    for key, entry in (catalog_data.get('bauplaene') or {}).items():
        if key in have:
            continue                      # was man hat, lohnt nicht mehr
        for source in entry.get('q') or []:
            title = source.get('auftrag')
            if not title:
                continue
            ident = (title, source.get('fraktion') or '')
            entry_a = contracts.setdefault(ident, {
                'anzahl': 0, 'uec': 0, 'rang': source.get('rang'),
                'wo': source.get('wo')})
            entry_a['anzahl'] += 1
            entry_a['uec'] = max(entry_a['uec'], source.get('uec') or 0)
    out = [(title, faction, w['anzahl'], w['uec'], w['rang'], w['wo'])
            for (title, faction), w in contracts.items()]
    out.sort(key=lambda x: (-x[2], -x[3], x[0]))
    return out


def kind_readable(raw):
    """Aus 'Char_Armor_Helmet' wird 'Helm' bzw. 'Helmet'."""
    return language.kind_label(KIND_MERGE.get(raw, raw))


# Arten, die dasselbe meinen und deshalb eine Gruppe bilden.
#
# ⚠ scmdb führt Magazine unter zwei Kennungen: 32 als `WeaponAttachment`
# (alle mit Subtyp „Magazine", nichts anderes steckt darin) und die beiden
# Start-Magazine als `ammo`. Für den Spieler ist das ein und dieselbe Sache —
# ungebündelt zeigt der Filter Magazin nur die zwei, die 32 anderen stehen
# unter Waffenaufsatz.
#
# ⚠ Derselbe Fall bei den Handfeuerwaffen: 87 stehen als `WeaponPersonal` da, die
# S-38 Pistol und das P4-AR Rifle als `weapons`. Ungebündelt ergäbe das zwei
# Gruppen für ein und dieselbe Sache. Wer nach FPS-Waffen filtert, sucht auch die beiden.
KIND_MERGE = {'ammo': 'WeaponAttachment', 'weapons': 'WeaponPersonal'}


def kind_id(entry_or_raw):
    """Die Art, unter der ein Bauplan einsortiert und gefiltert wird.

    Nimmt einen Katalogeintrag oder die rohe Kennung. Zusammengehörende Arten
    (siehe `ART_ZUSAMMEN`) werden auf eine gezogen.
    """
    raw = (entry_or_raw.get('a')
           if isinstance(entry_or_raw, dict) else entry_or_raw)
    return KIND_MERGE.get(raw, raw)


# So viele Bezugsquellen je Bauplan werden behalten.
#
# **Gemessen** am Dump 4.9.0-live.12344265 (655 Baupläne mit Quelle):
# Median 4 Wege, Mittelwert 5,8, Höchstwert 73. Eine Grenze von 3 schnitte
# **54 %** aller Baupläne Wege ab. Ein Weg reicht nicht: Wer gerade bei einer
# anderen Fraktion Ruf sammelt, braucht den zweiten oder dritten.
#
# Bei 12 verliert genau **ein** Bauplan etwas (der mit 73 Vorkommen). Angezeigt
# wird trotzdem nur der leichteste; der Rest steht hinter „weitere Wege".
SOURCES_PER_BP = 12


# ------------------------------------------------------------------ Netz
TRIES = 3


def _fetch(url, timeout=TIMEOUT, tries=TRIES):
    """Eine JSON-Datei holen — mit Wiederholung.

    Der Sammel-Dump ist rund 12 MB, und genau bei der Größe reißt die Leitung
    gern mitten drin ab (hier beim Bauen zweimal passiert). Ein einzelner
    Fehlversuch darf deshalb nicht heißen, dass es den Katalog nicht gibt.

    Die Netzsperre sitzt hier, an der Engstelle: `current_version()`,
    `fetch_file()` und `build()` gehen alle durch diese Funktion, auch wenn
    sie nicht über `update()` gerufen werden (etwa vom Knopf auf der
    Bergbau-Seite oder von der Kommandozeile)."""
    if OFF:
        raise OSError(t('m_h_kein_netz'))
    last = None
    for attempt in range(tries):
        try:
            req = urllib.request.Request(url, headers={'User-Agent': USER_AGENT})
            with urllib.request.urlopen(req, timeout=timeout) as r:
                return json.loads(r.read().decode('utf-8'))
        except urllib.error.HTTPError as err:
            # ⚠ 403 ist eine Absage, kein Wackelkontakt. Cloudflare weist
            # Abrufe ohne eigene Kennung ab; noch zweimal zu fragen kostet nur
            # Zeit und aendert nichts. Sofort raus, mit klarer Meldung.
            if err.code == 403:
                raise Rejected(t('m_abgewiesen')) from err
            last = err
            if attempt + 1 < tries:
                time.sleep(2 * (attempt + 1))
        except Exception as err:
            last = err
            if attempt + 1 < tries:
                time.sleep(2 * (attempt + 1))
    raise last


def fetch_file(name, timeout=TIMEOUT, tries=3):
    """Eine Datendatei holen — erst vom Spiegel, dann von scmdb.net.

    **Die eine Stelle**, über die alle drei Datenmodule holen (Katalog,
    Herstellung, Bergbau). Wer eine Quelle ändert, ändert sie hier — nicht an
    drei Orten mit drei Schreibweisen.
    """
    last = None
    for base_url in (MIRROR, SCMDB):
        try:
            return _fetch('%s/%s' % (base_url, name), timeout=timeout,
                         tries=tries)
        except Rejected as error:
            # 403 heißt: Diese Quelle mag nicht. Die andere darf es versuchen.
            last = error
        except Exception as error:
            last = error
    raise last


def current_version():
    """Die laufende Spielversion laut scmdb (PTU wird übersprungen)."""
    for v in _fetch(BASE + '/versions.json', timeout=15):
        name = (v.get('version') or '')
        if name and 'ptu' not in name.lower():
            return name
    return None


# ------------------------------------------------------------ Aufbereitung
# Alle Anführungszeichen, die in Bauplan-Namen vorkommen — gerade, typografische
# und die französischen. Sie werden beim Vergleichen auf ein einfaches `'`
# gezogen.
#
# ⚠ Warum das nötig ist: Der SC Deutsch Launcher exportiert `7MA "Lorica"` mit
# geraden doppelten Anführungszeichen, scmdb führt denselben Bauplan als
# `7MA 'Lorica'` mit einfachen. Ohne Angleichung sind das zwei verschiedene
# Schlüssel — der Bauplan galt als „fehlt", obwohl er im eigenen Bestand stand.
# Gefunden an einem echten Bestand mit 392 Bauplänen: 391 wurden zugeordnet,
# genau dieser eine nicht. An erfundenen Testdaten wäre das nie aufgefallen.
QUOTES = str.maketrans({
    '"': "'", '„': "'", '“': "'", '”': "'",   # " „ " "
    '‘': "'", '’': "'", '«': "'", '»': "'",  # ' ' « »
})


def _norm(s):
    """Vergleichsform eines Namens — siehe `paths.name_key`."""
    return paths.name_key(s)


def _manufacturer(entry):
    """Der Hersteller eines scmdb-Eintrags, berichtigt nach Herstellerkürzel
    (`crafting.manufacturer_name`)."""
    from .crafting import MANUFACTURER_BY_CODE
    code = (entry.get('manufacturerCode') or '').upper()
    return MANUFACTURER_BY_CODE.get(code) or entry.get('manufacturer')


def _manufacturer_fixed(entry):
    """Ist der Hersteller schon über das Kürzel berichtigt (MaxOx)?"""
    from .crafting import MANUFACTURER_BY_CODE
    return (entry.get('manufacturerCode') or '').upper() in MANUFACTURER_BY_CODE


def _values(raw_items):
    """Name -> Art, Größe, Gütegrad, Klasse, Hersteller.

    ⚠ `_name` trägt den Namen in seiner **Schreibweise aus dem Spiel** mit —
    der Schlüssel ist die Vergleichsform und taugt nicht zum Anzeigen
    (`a03 'canuto' sniper rifle`). Gebraucht wird er für die Baupläne ohne
    bekannten Weg: Sie kommen nur aus dieser Datei, es gibt
    also keine zweite Stelle, die ihren Namen kennt.

    ⛔ Unterstrich-Schlüssel sind **innerlich** und gehören nicht in den
    Katalog-Eintrag. Wer `values_` einmischt, filtert sie heraus.
    """
    values_ = {}
    for e in raw_items.get('items', []):
        name = e.get('name')
        if name:
            values_.setdefault(_norm(name), {
                '_name': name,
                'a': e.get('attachType') or e.get('cgItemType'),
                'sub': e.get('attachSubType'),
                's': e.get('size'),
                'g': e.get('grade'),
                'c': e.get('componentClass'),
                'm': _manufacturer(e),
                '_mfest': _manufacturer_fixed(e),
            })
    return values_


_GRADE_NUMBER = {'A': 1, 'B': 2, 'C': 3, 'D': 4}

# Ein Lesevorgang je Programmlauf — Katalog und Overlay fragen beide.
_GAME_GRADES = [None]


def grades_from_ini(data):
    """Vergleichsname -> Gütegrad als Zahl (1 = A … 4 = D), aus einer `global.ini`.

    Zugeordnet über den Schlüsselstamm: `item_DescRADR_BLTR_S01_Pelerous`
    gehört zu `item_NameRADR_BLTR_S01_Pelerous`. Ein Name, unter dem das Spiel
    **verschiedene** Grade führt, fällt heraus — dann weiß es CIG selbst nicht
    eindeutig, und scmdb bleibt stehen.
    """
    from . import specs
    if isinstance(data, bytes):
        data = data.decode('utf-8-sig', 'ignore')
    descriptions, names = {}, {}
    for line in (data or '').splitlines():
        sep = line.find('=')
        if sep < 1:
            continue
        key = line[:sep].split(',', 1)[0].lower()
        if key.startswith('item_desc'):
            descriptions[key[9:]] = line[sep + 1:]
        elif key.startswith('item_name'):
            names[key[9:]] = line[sep + 1:].strip().lstrip('*').strip()
    found = {}
    for stem, name in names.items():
        grade = specs.grade_from_description(descriptions.get(stem))
        if grade and name:
            found.setdefault(_norm(specs.strip_tag(name)), set()).add(grade)
    return {key: _GRADE_NUMBER[next(iter(grades))]
            for key, grades in found.items() if len(grades) == 1}


def game_grades():
    """Die Gütegrade laut Spiel — `{Vergleichsname: 1–4}`, leer ohne Spiel.

    ⚠⚠ **scmdb liegt beim Gütegrad nachweislich daneben.** Gegen CIGs
    englische Sprachdatei verglichen (4.10.1): Von 305 Schiffskomponenten
    weichen drei ab — Draug (scmdb A, Spiel C), Elsen (A statt B) und
    Pelerous (A statt C). Deshalb gilt: **Spiel vor scmdb.**

    Gelesen wird die **Originaldatei aus der `Data.p4k`**, nicht eine lose
    `global.ini` im Spielordner — die kann von einem anderen Werkzeug
    bearbeitet sein. Die Klasse bleibt bei scmdb. Den Hersteller berichtigt
    `apply_game_makers` — nur bei echt anderem Hersteller, denn CIG schreibt
    selbst uneinheitlich („RSI" neben „Roberts Space Industries",
    „Lighting Power Ltd.").
    """
    if _GAME_GRADES[0] is None:
        grades = {}
        try:
            from . import gametext
            data, _message = gametext.read_from_archive('english')
            if data:
                grades = grades_from_ini(data)
        except Exception as error:
            errors.record('catalog.game_grades', error)
        _GAME_GRADES[0] = grades
    return _GAME_GRADES[0]


def apply_game_grades(values_, grades):
    """Die Gütegrade des Spiels über die von scmdb legen — nur bei echten
    Schiffskomponenten (`c` gesetzt). Gibt die Zahl der Änderungen zurück.

    ⚠ scmdb vergibt einen Grad an **jeden** Gegenstand, auch an Helme. Ohne
    diese Schranke käme über die Sprachdatei nichts Falsches dazu — aber ein
    Grad, der vorher bewusst nicht angezeigt wurde, bekäme plötzlich Gewicht.
    """
    changed = 0
    for key, entry in values_.items():
        grade = grades.get(key)
        if grade and entry.get('c') and entry.get('g') != grade:
            entry['g'] = grade
            changed += 1
    return changed


# Ein Lesevorgang je Programmlauf, wie bei den Gütegraden.
_GAME_MAKERS = [None]


def makers_from_ini(data):
    """`({Vergleichsname: Hersteller}, {Kürzel: voller Name})` aus einer
    `global.ini`.

    Der Hersteller steht in der Beschreibung (`Manufacturer: …`), die Kürzel
    unter `manufacturer_Name<KÜRZEL>`. Ein Name mit verschiedenen Herstellern
    fällt heraus."""
    from . import specs
    if isinstance(data, bytes):
        data = data.decode('utf-8-sig', 'ignore')
    descriptions, names, codes = {}, {}, {}
    for line in (data or '').splitlines():
        sep = line.find('=')
        if sep < 1:
            continue
        key = line[:sep].split(',', 1)[0].lower()
        value = line[sep + 1:].strip()
        if key.startswith('item_desc'):
            descriptions[key[9:]] = line[sep + 1:]
        elif key.startswith('item_name'):
            names[key[9:]] = value.lstrip('*').strip()
        elif key.startswith('manufacturer_name') and value:
            codes[key[len('manufacturer_name'):].upper()] = value
    found = {}
    for stem, name in names.items():
        maker = specs.manufacturer_from_description(descriptions.get(stem))
        if maker and name:
            found.setdefault(_norm(specs.strip_tag(name)), set()).add(maker)
    return ({key: next(iter(m)) for key, m in found.items() if len(m) == 1},
            codes)


def _maker_key(name):
    """`Klaus & Werner` → `klausandwerner` — nur Buchstaben und Ziffern."""
    return re.sub(r'[^a-z0-9]', '', (name or '').lower().replace('&', 'and'))


def same_maker(a, b, codes=None):
    """Meinen zwei Angaben denselben Hersteller — nur anders geschrieben?

    Gleich gelten: Kürzel und voller Name (`VOLT`, `RSI`), Anfangsbuchstaben
    (`GNP`), ein Name im anderen (`Nav-E7` / `Nav-E7 Gadgets`) und knappe
    Vertipper (`Lighting` / `Lightning`)."""
    import difflib
    codes = codes or {}
    full_a = codes.get((a or '').strip().upper(), a)
    full_b = codes.get((b or '').strip().upper(), b)
    ka, kb = _maker_key(full_a), _maker_key(full_b)
    if not ka or not kb:
        return False
    if ka == kb or ka in kb or kb in ka:
        return True
    for short, long_ in ((a, full_b), (b, full_a)):
        initials = ''.join(w[0] for w in re.findall(r'[A-Za-z0-9]+', long_ or '')).lower()
        s = _maker_key(short)
        if len(initials) >= 2 and len(s) >= 2 and (s == initials
                                                  or s.startswith(initials)):
            return True
    return difflib.SequenceMatcher(None, ka, kb).ratio() >= 0.85


def game_makers():
    """Die Hersteller laut Spiel und die Kürzeltabelle — leer ohne Spiel."""
    if _GAME_MAKERS[0] is None:
        result = ({}, {})
        try:
            from . import gametext
            data, _message = gametext.read_from_archive('english')
            if data:
                result = makers_from_ini(data)
        except Exception as error:
            errors.record('catalog.game_makers', error)
        _GAME_MAKERS[0] = result
    return _GAME_MAKERS[0]


def apply_game_makers(values_, makers, codes=None):
    """Den Hersteller laut Spiel über den von scmdb legen — nur, wenn es
    wirklich ein anderer Hersteller ist, nicht bloß eine andere Schreibweise.
    Über das Kürzel Berichtigtes (`_mfest`) bleibt. Gibt die Zahl der
    Änderungen zurück."""
    changed = 0
    for key, entry in values_.items():
        maker = makers.get(key)
        current = entry.get('m')
        if (maker and current and not entry.get('_mfest')
                and not same_maker(current, maker, codes)):
            entry['m'] = maker
            changed += 1
    return changed


# So viele Annahmeorte werden genannt. Mehr hilft niemandem: Wer den Auftrag
# sucht, fliegt den nächsten an — eine Aufzählung von fünfzehn Lagrange-Punkten
# beantwortet die Frage nach dem Ort schlechter als drei Planeten.
PLACES_PER_CONTRACT = 4


def _pickup_places(contract, places_pool):
    """Wo sich der Auftrag annehmen lässt — System und die größeren Orte.

    Ergänzt die Herkunft des Bauplans um den Ort, an dem die Mission
    angenommen wird. Die Angabe steckt in `locations` als Kennungen;
    aufgelöst werden sie über `locationPools`.

    Planeten zuerst, danach der Rest — ein Planetenname ist die Auskunft, mit
    der ein Spieler etwas anfangen kann; „HUR L2" hilft nur, wenn man ohnehin
    schon weiß, wo man ist."""
    idents = contract.get('locations') or []
    planets, other = [], []
    for ident in idents:
        place = places_pool.get(ident)
        if not isinstance(place, dict):
            continue
        name = place.get('name')
        if not name:
            continue
        kind = (place.get('type') or '').lower()
        if kind == 'star':
            continue                       # das System steht ohnehin dabei
        (planets if kind in ('planet', 'moon') else other).append(name)

    def clean(rows):
        seen, out = set(), []
        for n in rows:
            if n not in seen:
                seen.add(n)
                out.append(n)
        return out

    names = clean(planets) or clean(other)
    systems = contract.get('availableSystems') or contract.get('systems') or []
    if not names and not systems:
        return None
    return {'system': ', '.join(systems) or None,
            'orte': names[:PLACES_PER_CONTRACT],
            'mehr': max(0, len(names) - PLACES_PER_CONTRACT)}


# Vorsätze, die jeder Belohnungstopf trägt — sie sagen nichts aus.
_POOL_PREFIX = re.compile(r'^BP_(?:MISSION)?REWARDS?_', re.I)

# Töpfe, deren Namen ein Mensch nicht deuten muss. Alles andere wird nur
# aufgeräumt, nicht gedeutet — lieber „aus: RedWind" als eine erfundene Erklärung.
_POOL_PLAIN = (
    (re.compile(r'^xenothreat', re.I), 'XenoThreat'),
    (re.compile(r'^rdc[_ ]?boss', re.I), 'RDC-Boss'),
    (re.compile(r'^superheavy', re.I), 'Super-Heavy-Mission'),
    (re.compile(r'^cds[_ ]', re.I), 'CDS-Rüstung'),
)


def pool_readable(raw):
    """Aus `BP_REWARDS_Xenothreat2_15_06` wird `XenoThreat`.

    Die 59 Baupläne ohne Auftrag liegen **nicht im Nichts** — sie stehen in
    benannten Belohnungstöpfen. Vorher stand bei ihnen nur ein `?`, und der
    Spieler wusste nicht, ob es ihn nie gibt oder ob nur die Daten fehlen. Der
    Topf-Name sagt ihm wenigstens, wonach er suchen muss.

    Gedeutet wird nur, was eindeutig ist. Der Rest wird lediglich lesbar
    gemacht: Vorsatz weg, Unterstriche zu Leerzeichen, die durchnummerierten
    Endungen (`_15_06`) abgeschnitten — sie sind Stufen desselben Topfes.
    """
    raw = (raw or '').strip()
    if not raw:
        return ''
    core = _POOL_PREFIX.sub('', raw)
    for pattern, plain in _POOL_PLAIN:
        if pattern.search(core):
            return plain
    # Manche Töpfe heißen nach dem **Gegenstand**, nicht nach der Quelle:
    # `behr_rifle_ballistic_01_mr01` ist ein Behring-Gewehr, kein Ort und kein
    # Ereignis. Solche Namen zu zeigen wäre schlechter als nichts — der Spieler
    # läse eine Herkunft, die keine ist. Erkennbar sind sie daran, dass sie
    # durchgehend klein geschrieben sind; die echten Quellen (`RedWind`,
    # `Xenothreat2`) tragen Großbuchstaben.
    if core and core == core.lower():
        return ''
    core = re.sub(r'(_\d+)+$', '', core)          # `_15_06` und Verwandte weg
    core = re.sub(r'[_\-]+', ' ', core).strip()
    # Zweite Schranke: Eine Quelle heißt kurz („RedWind", „RDC Boss"). Wo eine
    # ganze Gegenstandsbeschreibung steht („Carryable 2H FL MissionItem
    # Microsatellite a"), ist es wieder keine Herkunft, sondern das Ding selbst.
    if len(core.split()) > 2:
        return ''
    return core or ''


def _origin(merged):
    """Bauplan-Name -> Liste von Bezugsquellen, leichteste zuerst."""
    pools = {}
    for guid, pool in (merged.get('blueprintPools') or {}).items():
        pools[guid] = [b.get('name') for b in (pool.get('blueprints') or [])
                       if b.get('name')]
    factions = merged.get('factions') or {}
    rewards = merged.get('factionRewardsPools') or []
    places_pool = merged.get('locationPools') or {}

    sources = {}
    for contract in ((merged.get('contracts') or [])
                    + (merged.get('legacyContracts') or [])):
        targets = [r.get('blueprintPool')
                 for r in (contract.get('blueprintRewards') or [])]
        if not targets:
            continue
        faction = factions.get(contract.get('factionGuid')) or {}
        i = contract.get('factionRewardsIndex')
        rep_gain = None
        if isinstance(i, int) and 0 <= i < len(rewards):
            rep_gain = sum(e.get('amount', 0) for e in rewards[i])
        rank = contract.get('minStanding') or {}
        entry = {
            'auftrag': contract.get('title'),
            'typ': contract.get('missionType'),
            'fraktion': faction.get('name') if isinstance(faction, dict) else None,
            'uec': contract.get('rewardUEC'),
            'ruf': rep_gain,
            'rang': rank.get('name'),
            'rep': rank.get('minReputation'),
            'wo': _pickup_places(contract, places_pool),
        }
        for guid in targets:
            for name in pools.get(guid, []):
                sources.setdefault(_norm(name), []).append(entry)

    # Leichtesten Weg zuerst: niedrigste Ruf-Anforderung, bei Gleichstand die
    # höhere Bezahlung. Dubletten (derselbe Auftrag über mehrere Pools) raus.
    for name, rows in sources.items():
        seen, clean = set(), []
        for e in sorted(rows, key=lambda x: ((x['rep'] if x['rep'] is not None
                                               else 10 ** 9),
                                              -(x['uec'] or 0))):
            key = (e['auftrag'], e['fraktion'])
            if key in seen:
                continue
            seen.add(key)
            clean.append(e)
        sources[name] = clean[:SOURCES_PER_BP]
    return pools, sources


def _starter_blueprints(version):
    """Die Baupläne, die jeder Spieler von Anfang an hat.

    **Warum die extra geholt werden müssen:** Der Katalog entsteht aus den
    `blueprintPools` — also aus dem, was Missionen ausschütten. Startbaupläne
    stehen in **keinem** Pool, weil man sie nie als Belohnung bekommt. Sie
    fehlten dadurch vollständig: nicht in der Liste, nicht im Bestand, und wer
    danach suchte, fand nichts.

    Zu erkennen sind sie am Feld `isDefault` in `crafting_blueprints-<version>.json`
    — **nicht** in `crafting_items`, dort gibt es das Feld nicht. Es sind acht:
    P4-AR Rifle und S-38 Pistol samt Magazinen, dazu der Field Recon Suit
    (vier Teile).

    Die Datei ist mit 4,2 MB die größte der drei, wird aber nur beim Neubau des
    Katalogs geholt — also einmal je Spielversion."""
    try:
        raw = _fetch('%s/crafting_blueprints-%s.json' % (BASE, version))
    except Exception:
        return []
    rows = raw.get('blueprints') if isinstance(raw, dict) else raw
    result = []
    for e in rows or []:
        if not e.get('isDefault'):
            continue
        name = e.get('productName')
        if name:
            result.append({'n': name, 'a': e.get('type'), 'start': True})
    return result


def _missions(merged):
    """Missionen, die Baupläne ausschütten — für die Auszeichnung im Spiel.

    Das ist die **Gegenrichtung** zu `_herkunft()`: dort „welcher Bauplan kommt
    woher", hier „welche Baupläne gibt diese Mission". Gebraucht wird sie von
    `scbp/injection.py`, das die Angaben in die Textdatei des Spiels schreibt.

    Angehängt wird an den **Textschlüssel** (`descriptionLocKey`,
    `titleLocKey`), nicht an den Missionsnamen: Der Schlüssel ist in jeder
    Sprache derselbe, der Name nicht. Dadurch funktioniert dieselbe Zuordnung
    für die deutsche Übersetzung wie für die englische Version — und für die
    neun weiteren Sprachen im Spiel gleich mit."""
    pools = {}
    for guid, pool in (merged.get('blueprintPools') or {}).items():
        pools[guid] = [b.get('name') for b in (pool.get('blueprints') or [])
                       if b.get('name')]
    reward_pools = merged.get('factionRewardsPools') or []

    # ⚠ **Ein Schlüssel, viele Verträge.** Mit einem schlichten
    # `ergebnis[titel_key or text_key] = eintrag` überschreiben sich Verträge,
    # die sich einen Textschlüssel teilen, gegenseitig: Der zuletzt
    # eingelesene gewinnt, alle anderen fallen still weg. Gemessen am Dump
    # 4.10.0-live.12519617:
    #
    #     353 Schlüssel, davon 123 mehrfach belegt
    #     319 Verträge fielen weg
    #     797 Bauplan-Einträge wurden dadurch nie angezeigt
    #
    # Deshalb werden alle Varianten **zusammengeführt**: Die Liste zeigt, was
    # der Auftragstyp überhaupt hergibt, das Kästchen sagt, was man davon hat —
    # auch wenn die Stufe, an der man gerade steht, den Bauplan nicht hergibt.
    raw = {}
    for contract in ((merged.get('contracts') or [])
                    + (merged.get('legacyContracts') or [])):
        key = contract.get('titleLocKey') or contract.get('descriptionLocKey')
        if key:
            raw.setdefault(key, []).append(contract)

    result = {}
    for key, variants in raw.items():
        # ⚠ Varianten **ohne** Baupläne bleiben in `gesehen` mit dabei. Sonst
        # wäre nicht erkennbar, dass es Stufen dieses Auftrags gibt, die leer
        # ausgehen. 14 Auftragstexte sind so gebaut, etwa eine Trainee-Stufe,
        # die nichts ausschüttet, aber am selben Text hängt wie die höheren.
        seen = []
        for v in variants:
            names = []
            for r in (v.get('blueprintRewards') or []):
                names.extend(pools.get(r.get('blueprintPool'), []))
            rank = v.get('minStanding') or {}
            systems = v.get('availableSystems') or v.get('systems') or []
            seen.append({'namen': set(names), 'vertrag': v,
                         'systeme': [str(s) for s in systems]
                         if isinstance(systems, list) else [],
                            'rep': rank.get('minReputation'),
                            'rang': rank.get('name'),
                            'uec': v.get('rewardUEC')})
        with_bp = [g for g in seen if g['namen']]
        if not with_bp:
            continue

        all_names = set()
        for g in with_bp:
            all_names |= g['namen']

        # Der **leichteste** Weg liefert die Kopfdaten: niedrigster Ruf, bei
        # Gleichstand die höhere Bezahlung — dieselbe Regel wie in `_herkunft()`.
        def _easiest(amount):
            return sorted(amount, key=lambda g: ((g['rep'] if g['rep'] is not None
                                                 else 10 ** 9),
                                                -(g['uec'] or 0)))[0]

        # Ab welchem Rang ein Bauplan überhaupt zu haben ist. Nur dort, wo sich
        # die Stufen wirklich unterscheiden — sonst stünde an jedem dasselbe.
        # Das ist reine Zusatzinfo: Sie ändert weder Kästchen noch Auswahl,
        # sondern erklärt, warum ein Bauplan auf der aktuellen Stufe fehlt.
        from_rank = {}
        if len(set(g['rep'] for g in with_bp)) > 1:
            for name in all_names:
                q = _easiest([g for g in with_bp if name in g['namen']])
                if q['rang'] and q['rep']:
                    from_rank[name] = {'rang': q['rang'], 'rep': q['rep']}
            # ⚠ **Nur wenn es die Baupläne unterscheidet.** Sonst stünde an
            # *jedem* Bauplan desselben Auftrags derselbe Rang untereinander —
            # Lärm, denn gilt für alle derselbe Rang, steht er ohnehin oben bei
            # der Mindest-Reputation. Die Zeile lohnt sich erst, wenn ein
            # Bauplan einen *höheren* Rang braucht als ein anderer.
            if len(set((b['rang'], b['rep']) for b in from_rank.values())) < 2:
                from_rank = {}

        easy = _easiest(with_bp)
        contract = easy['vertrag']
        rewards = contract.get('blueprintRewards') or []
        # Sicher und Chance über **alle** Varianten: Wenn irgendeine den
        # Bauplan garantiert gibt, ist er erreichbar.
        sure = any(r.get('chance') == 1 for g in with_bp
                     for r in (g['vertrag'].get('blueprintRewards') or []))
        chance = max((r.get('chance') or 0) for g in with_bp
                     for r in (g['vertrag'].get('blueprintRewards') or []))
        rank = contract.get('minStanding') or {}
        highest = contract.get('maxStanding') or {}
        i = contract.get('factionRewardsIndex')
        rep_gain = None
        if isinstance(i, int) and 0 <= i < len(reward_pools):
            rep_gain = sum(e.get('amount', 0) for e in reward_pools[i])
        entry = {
            'bp': sorted(all_names),
            'sicher': sure,
            'chance': chance,
            'rep': rank.get('minReputation'),
            'rang': rank.get('name'),
            'rep_max': highest.get('minReputation'),
            'rang_max': highest.get('name'),
            'uec': contract.get('rewardUEC'),
            'ruf': rep_gain,
            'teilbar': contract.get('canBeShared'),
            'cooldown': (contract.get('personalCooldownTime')
                         if contract.get('hasPersonalCooldown') else None),
        }
        entry = {k: v for k, v in entry.items() if v not in (None, '', [])}
        entry['bp'] = sorted(all_names)
        entry['sicher'] = sure
        if from_rank:
            entry['ab'] = from_rank
        per_system = _per_system(with_bp)
        if per_system:
            entry['je_system'] = per_system
        # Wie viele Stufen dieses Auftrags leer ausgehen — steht als Warnung
        # dran, damit niemand für eine Liste hinfliegt, die seine Stufe nicht
        # hergibt.
        empty = len(seen) - len(with_bp)
        if empty:
            entry['leer'] = empty
            entry['stufen'] = len(seen)
        if contract.get('titleLocKey'):
            entry['titel_key'] = contract.get('titleLocKey')
        if contract.get('descriptionLocKey'):
            entry['text_key'] = contract.get('descriptionLocKey')
        result[key] = entry
    return result


SYSTEM_ORDER = ('Stanton', 'Pyro', 'Nyx')


def system_sort_key(name):
    """Stanton, Pyro, Nyx — danach weitere Systeme alphabetisch."""
    if name in SYSTEM_ORDER:
        return (0, SYSTEM_ORDER.index(name), '')
    return (1, 0, name)


def _per_system(variants):
    """{System: [Baupläne]} über alle Varianten eines Auftragstexts — oder {}.

    Leer, wenn eine Variante kein System nennt, nur ein System vorkommt oder
    alle Systeme dieselbe Liste hergeben: Dann sagt die gemeinsame Liste
    dasselbe."""
    found = {}
    for variant in variants:
        if not variant['systeme']:
            return {}
        for system in variant['systeme']:
            found.setdefault(system, set()).update(variant['namen'])
    if len(found) < 2 or len(set(frozenset(v) for v in found.values())) < 2:
        return {}
    return {system: sorted(found[system])
            for system in sorted(found, key=system_sort_key)}


def _contracts(merged):
    """Jeder Vertrag einzeln — die **Gegenrichtung** zu `_missions()`.

    `_missions()` fasst alle Varianten eines Auftragstexts zusammen; das muss
    so sein (siehe dort: sonst fallen 319 Verträge still weg). Für die Anzeige
    ist es aber zu grob:

        Foxwell_DefendEntitesAndEscort_H_Title   54 Baupläne   (zusammengefasst)
          ├─ …_Nyx_Hard       23
          ├─ …_Pyro_Hard      19
          └─ …_Stanton_Hard   12

    Wer den Auftrag in Nyx annimmt, sieht im Spiel **23** — die
    zusammengefasste Liste sagt 54. Sie ist nicht falsch, aber sie beantwortet die Frage
    nicht, die der Spieler hat.

    ⭐ Und der Log nennt den Vertrag selbst:

        CreateMarker … contractDefinitionId[6c4b94f2-3b43-4e0a-9be5-93186e0a957e]

    Das ist genau die `id` hier. Keine Titelsuche, keine Marken, keine
    Platzhalter, keine Sprache — und keine Namensabbildung über den Tippfehler
    `Entities`/`Entites` in der Quelle.

    ⚠ An 157 Log-Sicherungen gemessen: **687 von 707** Annahmen
    lassen sich so zuordnen (97,2 %). Die übrigen fallen auf den Titelweg
    zurück — deshalb bleibt `_missions()` vollständig erhalten.

    ⚠ Nur Verträge, die überhaupt Baupläne ausschütten: 672 von 1824.
    """
    pools = {}
    for guid, pool in (merged.get('blueprintPools') or {}).items():
        pools[guid] = [b.get('name') for b in (pool.get('blueprints') or [])
                       if b.get('name')]
    result = {}
    for contract in ((merged.get('contracts') or [])
                     + (merged.get('legacyContracts') or [])):
        kennung = contract.get('id')
        if not kennung:
            continue
        names = set()
        for r in (contract.get('blueprintRewards') or []):
            names |= set(pools.get(r.get('blueprintPool')) or [])
        if not names:
            continue
        entry = {'bp': sorted(names)}
        # Das System steht als eigenes Feld dabei — keine Namensrechnerei.
        systems = contract.get('availableSystems') or contract.get('systems')
        if isinstance(systems, list) and systems:
            entry['system'] = sorted(str(s) for s in systems)
        result[kennung] = entry
    return result


def build(version=None, progress=None, from_file=None):
    """Holt die Daten und legt den eigenen Katalog an. Gibt (anzahl, version) zurück.

    `fortschritt` ist eine Funktion für Zwischenmeldungen — das Holen dauert
    ein paar Sekunden, und ein stummes Programm sieht dabei aus wie ein hängendes."""
    def report(text):
        if progress:
            progress(text)

    version = version or current_version()
    if not version:
        return 0, ''

    # Bekannte CIG-Datenfehler: nur die Liste zu genau diesem Build, aus der
    # Ablage (geholt in `update()`). Fehlt sie, bleibt alles roh.
    from . import cig_issues
    issues = cig_issues.issues_for(version)

    report(t('z_werte'))
    raw_items = _fetch('%s/crafting_items-%s.json' % (BASE, version))
    cig_issues.apply_items((raw_items or {}).get('items'), issues)
    values_ = _values(raw_items)
    apply_game_grades(values_, game_grades())
    apply_game_makers(values_, *game_makers())

    if from_file:                       # nur für Entwicklung und Selbsttest
        report(t('z_herkunft_datei') % os.path.basename(from_file))
        with open(from_file, encoding='utf-8') as f:
            merged = json.load(f)
    else:
        report(t('z_herkunft_netz'))
        merged = _fetch('%s/merged-%s.json' % (BASE, version))
    cig_issues.apply_merged(merged, issues)

    report(t('z_auswerten'))
    pools, sources = _origin(merged)
    pool_names = {}
    for one_pool in (merged.get('blueprintPools') or {}).values():
        how = one_pool.get('name') or ''
        for b in (one_pool.get('blueprints') or []):
            if b.get('name'):
                pool_names.setdefault(_norm(b['name']), []).append(how)
    names = {n for rows in pools.values() for n in rows}

    blueprints = {}
    for name in sorted(names):
        k = _norm(name)
        entry = {'n': name}
        # ⛔ `_name` und alles andere mit Unterstrich ist innerlich (siehe
        # `_values`) — der Name steht hier schon als `n` und in der besseren
        # Schreibweise aus dem Belohnungs-Topf.
        entry.update({s: w for s, w in (values_.get(k) or {}).items()
                      if w and not s.startswith('_')})
        q = sources.get(k)
        if q:
            entry['q'] = q
        else:
            # Kein Auftrag schüttet ihn aus — aber der Topf hat einen Namen.
            pool_list = sorted({pool_readable(t) for t in (pool_names.get(k) or []) if t})
            if pool_list:
                entry['topf'] = ' · '.join(pool_list[:2])
        blueprints[k] = entry

    # Startbaupläne dazu — sie stehen in keinem Belohnungs-Pool und würden
    # sonst fehlen. Vorhandene Einträge werden nicht überschrieben.
    report(t('z_startbp'))
    for e in _starter_blueprints(version):
        k = _norm(e['n'])
        if k in blueprints:
            blueprints[k]['start'] = True
        else:
            entry = {'n': e['n'], 'start': True}
            if e.get('a'):
                entry['a'] = e['a']
            blueprints[k] = entry

    # ---- Und alles Übrige, was das Spiel überhaupt herstellen lässt ----
    #
    # Bis hierher steht im Katalog nur, was ein Belohnungs-Topf ausschüttet
    # (738). `crafting_items` kennt aber **alle** herstellbaren Gegenstände
    # (1591) — die Datei wird oben ohnehin schon geholt, es ist also keine
    # neue Quelle, sondern dieselbe vollständiger ausgewertet.
    #
    # Die Übrigen fehlen nicht im Spiel, sie haben nur keinen bekannten Weg: teils
    # über Kioske zu bekommen, teils aus Events, teils noch keiner Mission
    # zugeordnet. Sie tragen `ohne_weg` und bleiben **standardmäßig
    # unsichtbar** (siehe `SETTING_ALL` und `load()`).
    for k, w in values_.items():
        if k in blueprints:
            continue
        entry = {'n': w.get('_name') or k, 'ohne_weg': True}
        entry.update({s: v for s, v in w.items() if v and not s.startswith('_')})
        blueprints[k] = entry

    # ---- Was hat dieser Patch gebracht? ----
    #
    # Verglichen wird gegen **alle je gesehenen** Baupläne, nicht gegen den
    # vorigen Katalog. Der Unterschied ist der ganze Grund für
    # `patchhistory`: Führt die Quelle einen Bauplan zwischendurch nicht, gälte
    # er gegen den vorigen Katalog beim Wiederauftauchen fälschlich als neu.
    #
    # ⚠ Ist noch nichts gesehen worden (erster Katalogbau überhaupt), wird
    # NICHTS als Zugang gewertet — sonst stünden alle 730 Baupläne als „neu" da.
    # Nur die Vergleichsgrundlage wird gesetzt.
    #
    # ⚠ Ältere Installationen haben einen Katalog, aber keine
    # Vergleichsgrundlage — die Datei gehört zur Patch-Historie. Ohne diesen
    # Nachzug griffe dort die Regel oben fälschlich, und der **nächste** Patch
    # bliebe stumm. Der vorhandene Katalog ist die richtige Grundlage — was darin
    # steht, war vor diesem Lauf im Spiel.
    known = _baseline()
    if known:
        # ⚠⚠ **Baupläne ohne bekannten Weg sind kein Patch-Zugang.**
        # Mit FORMAT 5 kommen viele Einträge dazu, die es längst gab — sie
        # standen nur nie im Katalog. Ohne diese Bedingung gälten sie alle als
        # neu craftbar, und die Seite *Geänderte Spielwerte* wäre unbrauchbar.
        # Dieselbe Falle wie beim allerersten Katalogbau, eine Ebene höher.
        #
        # ⭐ Die Bedingung gilt dauerhaft und ist die genauere Aussage: Ein
        # Bauplan ist dann neu für den Spieler, wenn es einen **Weg** zu ihm
        # gibt. Bekommt einer später einen, verliert er `ohne_weg` und zählt
        # genau dann als Zugang.
        access = [e['n'] for k, e in blueprints.items()
                  if k not in known and not e.get('ohne_weg')]
        if access:
            patchhistory.record(version, access)
    patchhistory.set_seen(known | set(blueprints))

    # Der Stempel kommt aus der Historie, nicht aus diesem Lauf. Dadurch trägt
    # auch ein frisch gebauter Katalog die Herkunft aller früheren Patches —
    # die mitgelieferte Historie reicht weiter zurück als das eigene Zusehen.
    origin = patchhistory.version_per_blueprint()
    for k, entry in blueprints.items():
        if k in origin:
            entry['seit'] = origin[k]

    data = {'version': version, 'format': FORMAT,
             'geholt': time.strftime('%Y-%m-%d %H:%M'),
             'bauplaene': blueprints, 'missionen': _missions(merged),
             'vertraege': _contracts(merged),
             'datenfehler': cig_issues.fingerprint(issues)}
    target = paths.app_file(CACHE)
    temp = target + '.tmp'
    with open(temp, 'w', encoding='utf-8') as f:
        json.dump(data, f, ensure_ascii=False)
    os.replace(temp, target)
    return len(blueprints), version


# ------------------------------------------------------------------ Lesen
def load(unreachable=None):
    """Der eigene Katalog von der Platte. Fehlt er, ist er leer.

    ⚠⚠ **`unreachable` ist für alles, was NICHT die Anzeige ist.**
    Ohne Angabe bestimmt die Einstellung des Spielers, ob Baupläne ohne
    bekannten Weg dabei sind — richtig für Liste, Fortschritt und Overlay.

    Falsch ist es überall dort, wo eine **feste** Zahl gemeint ist: Prüfung 73
    hält die Zahlen in der Anleitung gegen die Daten, und die hingen sonst von
    der örtlichen Einstellung ab. `unreachable=False` erzwingt die
    erspielbaren, `True` alle.

    ⚠ **Die Schlüssel werden beim Laden neu gebildet.** Ein Katalog auf der
    Platte kann Monate alt sein und mit einer anderen Fassung von
    `namensform()` geschrieben worden sein — etwa Magazine als
    `… magazine (15 cap)`, während der Bestand sie als `… magazine (15)` führt.
    Solche Baupläne gälten überall als fehlend, obwohl sie im Bestand stehen.

    Deshalb wird hier nicht darauf vertraut, wie die Schlüssel einmal
    geschrieben wurden — sie werden aus dem Namen neu gebildet.
    """
    try:
        with open(paths.app_file(CACHE), encoding='utf-8') as f:
            d = json.load(f)
        if isinstance(d.get('bauplaene'), dict):
            d.setdefault('missionen', {})    # Kataloge ohne Feld `missionen`
            # ⚠ Kataloge vor FORMAT 3 kennen die Vertragsliste nicht. Fehlt
            # sie, faellt die Zuordnung auf den Titelweg zurueck.
            d.setdefault('vertraege', {})
            d['bauplaene'] = _align_keys(d['bauplaene'])
            # ⭐ Baupläne ohne bekannten Weg nur, wenn `SETTING_ALL` gesetzt
            # ist. **Hier** gefiltert und nirgends sonst: Am Katalog
            # hängen 88 Lesestellen — Liste, Fortschritt, Overlay, Suche,
            # Herstellung. Jede einzeln zu filtern hieße, beim nächsten neuen
            # Bereich eine zu vergessen, und dann zählte er anders als der Rest.
            #
            # ⛔ Der **Bestand** des Spielers läuft nicht hier durch
            # (`collection.py`, eigene Datei) — ein Bauplan, den er besitzt,
            # verschwindet also nie, auch wenn der Katalog ihn ausblendet.
            mit = (paths.setting_bool(SETTING_ALL, False)
                   if unreachable is None else bool(unreachable))
            if not mit:
                d['bauplaene'] = {k: e for k, e in d['bauplaene'].items()
                                  if not e.get('ohne_weg')}
            return d
    except Exception:
        pass
    return {'version': '', 'geholt': '', 'bauplaene': {}, 'missionen': {},
            'vertraege': {}}


def _align_keys(blueprints):
    """Schlüssel aus dem Namen neu bilden — für Kataloge älterer Fassungen.

    Passt schon alles, wird nichts angefasst: Bei einem frischen Katalog kostet
    das einen Vergleich je Bauplan und keine neue Verzeichnisstruktur.
    """
    to_rebuild = False
    for key, e in blueprints.items():
        name = e.get('n')
        if name and _norm(name) != key:
            to_rebuild = True
            break
    if not to_rebuild:
        return blueprints
    fresh = {}
    for key, e in blueprints.items():
        name = e.get('n')
        fresh[_norm(name) if name else key] = e
    return fresh


def _baseline():
    """Wogegen dieser Lauf vergleicht, um Zugänge zu erkennen.

    Normalerweise die Liste aller je gesehenen Baupläne. Fehlt sie (ältere
    Installationen), gilt ersatzweise der Katalog, der schon auf der Platte
    liegt: Was darin steht, war vor diesem Lauf im Spiel. Ohne diesen Ersatz
    griffe beim nächsten Patch die Regel für den ersten Katalogbau, und er
    brächte **keinen einzigen** Zugang.

    Leer ist das Ergebnis nur beim allerersten Katalogbau — dann ist es richtig
    so, sonst stünden alle 738 Baupläne als „neu" da."""
    return patchhistory.seen() or set(load().get('bauplaene') or {})


def refresh_stamp():
    """Fehlende `seit`-Stempel im vorhandenen Katalog nachtragen.

    ⚠ **Warum das nötig ist.** Gestempelt wird beim Neubau des Katalogs —
    und neu gebaut wird nur, wenn eine neue Spielversion kommt. Ein älterer
    Katalog ohne Stempel bliebe bis zum nächsten Patch ohne Herkunft, und die
    nach Patch gefilterte Liste bliebe leer, weil der Filter gegen den Stempel
    im Katalog prüft. Dasselbe gilt für „neu im Spiel".

    Der Abgleich kostet nichts und braucht kein Netz: Die Historie liegt beim
    Programm, der Katalog auf der Platte. Geschrieben wird nur, wenn sich
    wirklich etwas ändert.

    Gibt die Zahl der nachgetragenen Stempel zurück."""
    try:
        d = load()
        blueprints = d.get('bauplaene') or {}
        if not blueprints:
            return 0
        origin = patchhistory.version_per_blueprint()
        changed = 0
        for k, entry in blueprints.items():
            since = origin.get(k)
            if since and entry.get('seit') != since:
                entry['seit'] = since
                changed += 1
        if not changed:
            return 0
        target = paths.app_file(CACHE)
        temp = target + '.tmp'
        with open(temp, 'w', encoding='utf-8') as f:
            json.dump(d, f, ensure_ascii=False)
        os.replace(temp, target)
        return changed
    except Exception:
        return 0


def update(progress=None):
    """Erneuert den Katalog, falls eine neue Spielversion vorliegt.

    Gibt (True, Anzahl, Version) zurück, wenn etwas passiert ist. Wirft nie —
    ohne Netz gilt der letzte Stand, und der Watcher läuft ohne Katalog weiter
    (dann fehlt nur die Liste, nicht die Erkennung)."""
    # ⚠ Ganz am Anfang — vor der Netzsperre und vor dem Netz-Zugriff. Bringt
    # eine neue Programmfassung Historie mit, die der Katalog auf der Platte
    # noch nicht kennt, muss der Stempel auch dann nachkommen, wenn gar keine
    # neue Spielversion ansteht. `AUS` verbietet das **Netz**, nicht die Arbeit:
    # Historie und Katalog liegen beide auf der Platte. Stand die Zeile hinter
    # dem Riegel, blieb die Liste bei abgeschaltetem Netz für immer leer.
    refresh_stamp()
    if OFF:
        return False, 0, ''
    try:
        version = current_version()
        present = load()
        # Neu gebaut wird bei neuer Spielversion **oder** bei neuem Aufbau
        # (siehe `FORMAT`). Das zweite ist der Grund, warum ein Katalog-Umbau
        # überhaupt bei jemandem ankommt, der schon einen liegen hat.
        # ⚠⚠ **Die Werkstatt-Daten VOR der Abbruchbedingung holen.**
        #
        # Sie standen ursprünglich hinter `erzeugen()` — und wurden damit bei
        # niemandem je geholt, der schon einen aktuellen Katalog hatte. Also
        # bei **allen bisherigen Nutzern**: Der Katalog stimmte, die Funktion
        # kehrte hier um, und Herstellung, Bergbau und Lager blieben dauerhaft
        # leer, bis CIG das nächste Mal patcht.
        #
        # Beide Module bringen ihre **eigene** Prüfung mit (`schon aktuell?`)
        # und laden nichts doppelt. Sie hängen nicht am Katalog-Format, sondern
        # nur an der Spielversion.
        #
        # Jeweils **eigenes `try`**: Scheitert eines, soll der Katalog trotzdem
        # durchlaufen — eine leere Seite, die das sagt, ist besser als ein
        # verlorener Abruf.
        # Die Liste bekannter CIG-Datenfehler vor allem anderen: Rezepte und
        # Katalog berichtigen damit, was sie aus diesem Build lesen.
        from . import cig_issues
        try:
            if cig_issues.update(version):
                from . import crafting as _crafting_now
                _crafting_now.forget()
        except Exception as error:
            errors.record('katalog.aktualisieren.datenfehler', error)
        try:
            from . import crafting
            crafting.update(version, progress)
        except Exception as error:
            errors.record('katalog.aktualisieren.crafting', error)
        try:
            from . import mining
            ok, meldung = mining.update(version, progress)
            if not ok:
                # Sonst erfährt niemand, warum die Bergbau-Seite leer bleibt.
                errors.record('katalog.aktualisieren.bergbau',
                              RuntimeError(meldung))
        except Exception as error:
            errors.record('katalog.aktualisieren.bergbau', error)

        # Neu gebaut wird auch, wenn sich die Fehlerliste geändert hat —
        # sonst blieben die Berichtigungen bis zum nächsten Patch aus.
        if not version or (version == present.get('version')
                           and present.get('format') == FORMAT
                           and (present.get('datenfehler') or '')
                           == cig_issues.fingerprint(
                               cig_issues.issues_for(version))):
            return False, 0, version or ''
        count, version = build(version, progress)
        # ⛔⛔ **Die Zwischenspeicher gehoeren geleert — sonst wirkt der neue
        # Katalog erst beim naechsten Programmstart.**
        #
        # `contracts` merkt sich `missions` und `contract_definitions` beim ersten
        # Zugriff; der Katalog ist rund 1 MB gross, und bei jedem Auftrag neu
        # zu lesen waere Verschwendung. Die Funktion `contracts.forget()`
        # gibt es genau dafuer — ohne diesen Aufruf hielte der Speicher den
        # Stand VOR dem Katalog-Neubau.
        #
        # ⭐ Dieselbe Sorte wie die toten `getattr`-Namen, die Pruefung 195
        # sucht: Der Code sieht vollstaendig aus, nur ruft ihn niemand.
        try:
            from . import contracts
            contracts.forget()
        except Exception as error:
            errors.record('katalog.vergessen', error)
        return bool(count), count, version
    except Exception:
        return False, 0, ''


def version_short(version):
    """Aus '4.10.0-live.12519617' wird '4.10.0'.

    Die volle Kennung ist eindeutig und wird deshalb gespeichert; im Auswahlfeld
    steht nur die kurze Version, ohne Buildnummer."""
    return (version or '').split('-')[0] or (version or '')


def patches(data=None):
    """[(volle Version, kurze Version, Anzahl), …] — neueste zuerst.

    Alle Spielversionen, aus denen im Katalog Baupläne stammen. Grundlage ist
    derselbe Stempel `seit`, den auch `neue()` benutzt: Kommt ein Patch dazu,
    taucht seine Version hier von allein auf — es gibt keine gepflegte Liste,
    die man vergessen könnte.

    ⚠ **Gezählt wird der Katalog, nicht die Historie.** Mit
    `return patchhistory.patches()` verspräche das Auswahlfeld etwas, das die
    Liste darunter nicht einlösen kann: Der Filter prüft den Stempel im
    Katalog — zwei Quellen für dieselbe Frage, und die Zahl in Klammern ist
    genau die Zusage, wie viele Zeilen kommen.

    Dass die Historie mehr weiß, hilft dabei nicht: Was nicht gestempelt ist,
    kann die Liste nicht zeigen. Dafür sorgt `stempel_nachziehen()`, und zwar
    bevor das Fenster den Katalog liest.

    Vor dem zweiten Katalogbau ist die Liste leer: Ohne Vorgänger wird nichts
    gestempelt, und ein Feld mit einem einzigen Eintrag hilft niemandem."""
    counter = {}
    for e in ((data or load()).get('bauplaene') or {}).values():
        since = e.get('seit')
        if since:
            counter[since] = counter.get(since, 0) + 1
    return [(v, version_short(v), counter[v])
            for v in sorted(counter, key=patchhistory.rank, reverse=True)]


def new_ones(data=None):
    """Die Baupläne, die mit der **zuletzt geholten** Spielversion dazukamen.

    Grundlage ist der Stempel `seit`, den `erzeugen()` setzt. Gezeigt wird nur,
    was zur aktuellen Katalogversion passt — ältere Stempel bleiben zwar in der
    Datei stehen (sie sagen, mit welchem Patch es einen Bauplan gibt), gehören
    aber nicht mehr unter „neu im Spiel".

    Leer ist das Ergebnis, solange der Katalog erst einmal gebaut wurde: Ohne
    Vorgänger gibt es keine Differenz."""
    d = data or load()
    version = d.get('version') or ''
    if not version:
        return set()
    return {k for k, e in d['bauplaene'].items() if e.get('seit') == version}


def starter_blueprints(data=None):
    """Die Vergleichsformen der Baupläne, die jeder von Anfang an hat."""
    return {k for k, e in (data or load())['bauplaene'].items() if e.get('start')}


def names(data=None):
    """Alle Bauplan-Namen in Vergleichsform."""
    return set((data or load())['bauplaene'])


def by_kind(data=None):
    """Baupläne nach Art gruppiert: {'Cooler': [Eintrag, …], …}."""
    groups = {}
    for k, e in (data or load())['bauplaene'].items():
        groups.setdefault(kind_readable(e.get('a')), []).append(e)
    for rows in groups.values():
        rows.sort(key=lambda e: e['n'].lower())
    return groups


# ------------------------------------------------------- Obergruppen
# 25 Kategorien in Buchstabenreihenfolge sind kein Überblick — Andockkragen
# stünde vor Schiffswaffe und die Rüstung mittendrin.
# Wer sucht, denkt in Bereichen: erst das Schiff, dann was man am Mann trägt.
TOP_GROUPS = ('schiff', 'fps', 'ruestung', 'sonstiges')

KIND_GROUP = {
    # Alles, was am Schiff verbaut wird
    'Cooler': 'schiff', 'PowerPlant': 'schiff', 'QuantumDrive': 'schiff',
    'Shield': 'schiff', 'Radar': 'schiff', 'WeaponGun': 'schiff',
    'WeaponMining': 'schiff', 'SalvageModifier': 'schiff',
    'SalvageHead': 'schiff', 'TractorBeam': 'schiff',
    'DockingCollar': 'schiff', 'FuelNozzle': 'schiff', 'Cargo': 'schiff',
    # Was man in die Hand nimmt
    'WeaponPersonal': 'fps', 'WeaponAttachment': 'fps',
    # ⚠ scmdb führt einige Einträge unter kleingeschriebenen Sammelbegriffen
    # statt unter der sonst üblichen Kennung. Ohne diese vier Zeilen landeten
    # die S-38 Pistol und das P4-AR Rifle unter Sonstiges statt bei den
    # FPS-Waffen — dasselbe für den Field Recon Suit bei der Rüstung. Betroffen sind 10 der 722 Baupläne; wer nur auf
    # die Gesamtzahl sieht, merkt davon nichts.
    # Beide fallen inzwischen über `ART_ZUSAMMEN` mit ihrer richtigen Kennung
    # zusammen; die Zeile bleibt für den Fall, dass `obergruppe()` einmal eine
    # rohe Kennung bekommt, die nicht durch `art_kennung()` gelaufen ist.
    'weapons': 'fps', 'ammo': 'fps',
    # Was man am Körper trägt
    'Char_Armor_Helmet': 'ruestung', 'Char_Armor_Torso': 'ruestung',
    'Char_Armor_Legs': 'ruestung', 'Char_Armor_Arms': 'ruestung',
    'Char_Armor_Backpack': 'ruestung', 'Char_Armor_Undersuit': 'ruestung',
    'Char_Clothing_Torso_0': 'ruestung', 'Char_Clothing_Torso_1': 'ruestung',
    'Char_Clothing_Legs': 'ruestung', 'Char_Clothing_Feet': 'ruestung',
    'armour': 'ruestung',
}


# Zusätzliche Suchwörter je Art. Vier Kategorien heißen bewusst englisch, weil
# das Spiel sie so nennt — „Cooler", „Power Plant", „Quantum Drive", „Radar".
# Die deutsche Suche (etwa nach Kühler) fände sonst nichts. Beides
# soll gehen, ohne die im Spiel gebräuchliche Beschriftung zu ändern.
KIND_KEYWORDS = {
    'Cooler':        ('kühler', 'kuehler', 'kuhler'),
    'PowerPlant':    ('generator', 'kraftwerk', 'energie'),
    'QuantumDrive':  ('sprungantrieb', 'quantenantrieb', 'qd'),
    'Radar':         ('scanner', 'ortung'),
    'Shield':        ('schild', 'schilde'),
    'WeaponGun':     ('kanone', 'geschütz', 'geschuetz'),
    'WeaponPersonal': ('gewehr', 'pistole', 'fps'),
    'TractorBeam':   ('traktor', 'schlepper'),
    'Cargo':         ('fracht', 'ladung'),
}


def keywords(raw):
    """Weitere Begriffe, unter denen diese Art gefunden werden soll."""
    return KIND_KEYWORDS.get(raw, ())


def top_group(raw):
    """Zu welchem Bereich gehört diese Art? Unbekanntes landet bei 'sonstiges'."""
    return KIND_GROUP.get(raw, 'sonstiges')


def groups_ordered(data=None):
    """[(obergruppe, art_lesbar, [Einträge]), …] — in der Reihenfolge, in der
    sie angezeigt werden sollen.

    Innerhalb eines Bereichs alphabetisch: Das ist vorhersagbar, und eine
    „sinnvolle" Reihenfolge innerhalb der Schiffsteile hätte jeder anders
    im Kopf."""
    groups = {}
    for e in (data or load())['bauplaene'].values():
        raw = kind_id(e)
        groups.setdefault((top_group(raw), kind_readable(raw)), []).append(e)
    for rows in groups.values():
        rows.sort(key=lambda e: e['n'].lower())
    return [(tgrp, kind, groups[(tgrp, kind)])
            for tgrp, kind in sorted(groups,
                                  key=lambda p: (TOP_GROUPS.index(p[0]),
                                                 p[1].lower()))]


if __name__ == '__main__':
    import sys
    d = load()
    if '--holen' in sys.argv or not d['bauplaene']:
        file_name = next((a.split('=', 1)[1] for a in sys.argv
                      if a.startswith('--datei=')), None)
        n, v = build(progress=lambda t: print(' ', t), from_file=file_name)
        print('Katalog angelegt: %d Baupläne, Version %s' % (n, v))
        d = load()
    with_ = sum(1 for e in d['bauplaene'].values() if e.get('q'))
    print('Version %s · %d Baupläne · %d mit Herkunft'
          % (d['version'], len(d['bauplaene']), with_))
    print('Datei  :', paths.app_file(CACHE),
          '(%.0f KB)' % (os.path.getsize(paths.app_file(CACHE)) / 1024))
    for kind, rows in sorted(by_kind(d).items(), key=lambda x: -len(x[1]))[:8]:
        print('   %4d  %s' % (len(rows), kind))
