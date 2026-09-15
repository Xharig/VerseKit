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
Angenommene Aufträge — „bringt mir der etwas, das mir fehlt?"

Der Watcher beantwortet seine eigene Frage damit **früher**: Nicht erst wenn der
Bauplan im Spiel auftaucht, sondern schon beim Annehmen des Auftrags.

    Auftrag angenommen: Retake Platforms From Nine Tails
      → 3 Baupläne · dir fehlt: H4-PBF Ammo Carrier

Es ist bewusst **keine Auftragsverwaltung**: keine Liste, kein Reiter, kein
zweites Fenster. Eine Zeile im Overlay, wie ein Bauplanfund auch.

## Der Weg durch die Daten

| Schritt | Woher |
|---|---|
| 1. Auftrag angenommen | `Game.log`, Zeile `Added notification "<Phrase>: <Titel>: "` |
| 2. Welche Phrase | `mobiGlas_ui_MissionEvent_Activated` aus der `global.ini` |
| 3. Titel → Missionsschlüssel | Rückwärtssuche in der `global.ini` |
| 4. Schlüssel → Baupläne | `missionen[<schlüssel>]['bp']` im Katalog |
| 5. Was davon fehlt | der eigene Bestand |

## ⚠ Die Fallen, alle an echten Daten gemessen (29.08.2026)

1. **Auf den SCHLÜSSEL gehen, nie auf die Formulierung.** Auf Deutsch heißen
   `mobiGlas_ui_MissionEvent_Available` **und** `mobiGlas_ui_ObjectiveEvent_Activated`
   beide „Neuer Auftrag" — das sind Zwischenziele. Wer darauf hört, meldet bei
   jedem Etappenziel. Nur `MissionEvent_Activated` ist die Annahme.
2. **Der Titel trägt unsere eigenen Marken.** Im Log steht
   `Retake Platforms From Nine Tails <EM4>[BP!]</EM4>`, in der injizierten
   `global.ini` sogar `…[SCBPW] <EM4>[BP 4/8]</EM4>[/SCBPW]`. Vor jedem
   Vergleich müssen sie weg — auf **beiden** Seiten.
3. **58 von 353 Titeln enthalten Platzhalter** (`~mission(TargetName)`), die das
   Spiel erst zur Laufzeit einsetzt. Ein wörtlicher Vergleich scheitert dort.
   Deshalb wird aus dem Titel ein Muster gebaut: `High-Risk Bounty:
   ~mission(TargetName)` wird zu einem Muster mit `.+` an der Platzhalter-Stelle,
   der Rest woertlich. Damit sind 337 der
   353 erreichbar statt 279.
4. **Ist der Auftrag unbekannt, wird geschwiegen.** Für 16 der 353 findet sich
   kein Titel, und nicht jeder angenommene Auftrag steht überhaupt im Katalog.
   Lieber nichts melden als raten — eine falsche Bauplan-Zusage ist schlimmer
   als keine.

Die Auftrags-Herkunft stammt aus dem Katalog von scmdb.net und wird **nicht
mitgeliefert** (CC BY-NC-ND). Fehlt sie, tut dieses Modul nichts und der Watcher
läuft unverändert weiter.
"""
import os
import re

from . import fehler, catalog, pfade

# Der sprachneutrale Schlüssel für „Auftrag angenommen" — in jeder Sprache derselbe.
# Dazu das Teilen in der Gruppe: Wer einen Auftrag geteilt **bekommt**, soll
# genauso erfahren, dass darin Baupläne stecken.
#
# Gemessen an Logs, in denen **beide** Rollen vorkamen — geteilt und
# geteilt bekommen: Auf jedes „geteilt" folgt eine Annahme. Streng genommen
# genuegte also die Annahme allein. Das Teilen bleibt trotzdem drin, weil es
# nichts kostet: Steht der Titel schon in der Liste, bleibt es bei einem
# Eintrag. Faellt die Annahme in einer kuenftigen Spielfassung einmal weg,
# steht der Auftrag trotzdem da.
INI_KEYS = ('mobiGlas_ui_MissionEvent_Activated',
                  'mobiGlas_ui_Mission_Shared')

# Und die drei Arten, wie ein Auftrag endet. ⚠ Ohne sie hielte der Watcher
# jeden Auftrag für ewig offen: Wer zehn hintereinander macht, haette zehn
# Zeilen stehen, von denen neun erledigt sind.
#
# `Deactivate` heisst im Spiel „zurückgezogen" — das ist der Abbruch von Hand.
INI_END_KEYS = (
    'mobiGlas_ui_MissionEvent_Complete',      # abgeschlossen
    'mobiGlas_ui_MissionEvent_Deactivate',    # zurückgezogen (abgebrochen)
    'mobiGlas_ui_MissionEvent_Fail',          # fehlgeschlagen
)

# Rückfall, falls die `global.ini` nicht vorliegt. Beide an echten Logs gemessen
# (aus echten Log-Sicherungen, 29.08.2026: 701 Annahmen, 303 Abschluesse, 112
# Ruecknahmen, 57 Fehlschlaege).
TABLE = {
    # ⚠ Schweizerdeutsch ist eine **eigene Fassung** derselben Übersetzung
    # (`live-CH`) und schreibt „Uftrag" statt „Auftrag". Am 30.08.2026 direkt
    # in der Quelle nachgesehen (`rjcncpt/StarCitizen-Deutsch-INI`, Ordner
    # `live-CH`) — nicht geraten:
    #
    #     mobiGlas_ui_MissionEvent_Activated=Uftrag angenommen: %s
    #     mobiGlas_ui_MissionEvent_Complete=Uftrag abgschlosse: %s
    #
    # Ohne diese Einträge erkennt der Watcher dort **keinen einzigen Auftrag** —
    # still, ohne Fehlermeldung. Greift nur als Rückfall: Liegt eine lesbare
    # `global.ini` vor, gewinnt die immer.
    'de': ['Auftrag angenommen', 'Auftrag geteilt',
           'Uftrag angenommen', 'Uftrag geteilt'],          # live-CH
    'en': ['Contract Accepted', 'Contract Shared'],
}

# ⚠ „Auftrag geteilt" gehoert NICHT hierher — das ist ein Anfang, kein Ende.
END_TABLE = {
    # ⚠ Auch hier die Schweizer Fassung — und die weicht bei JEDEM der drei
    # Enden ab: „abgschlosse", „fehlgschlage". Nur „zurückgezogen" ist gleich.
    'de': ['Auftrag abgeschlossen', 'Auftrag zurückgezogen',
           'Auftrag fehlgeschlagen',
           'Uftrag abgschlosse', 'Uftrag zurückgezogen',
           'Uftrag fehlgschlage'],                          # live-CH
    'en': ['Contract Complete', 'Contract Withdrawn', 'Contract Failed'],
}

# Dieselbe Zeilenform wie bei den Bauplänen — sie ist zu eigen, als dass sie
# zufällig entstünde.
FRAME = r'Added notification "(?:%s):\s*(.+?)\s*:\s*"'

# Bauplan-Marken im Titel — vor jedem Vergleich weg, sonst gilt derselbe
# Auftrag als zwei verschiedene.
#
# ⚠ Nicht nur unsere eigenen. Dieselbe Marke setzen auch MrKraken StarStrings
# und der SC Deutsch Launcher, und zwar in Formen, die `injektion.py` längst
# kennt (`TITELMARKE`) — hier fehlten sie:
#
#   `<EM4>[BP]?</EM4>`            Zusatz HINTER der Klammer, nicht darin
#   `<EM4>[150 Rep] [BP]*</EM4>`  Vorspann davor, Zeichen dahinter
#
# Die alte Fassung erlaubte nur `!` INNERHALB der Klammer und liess deshalb
# 103 von 347 Titeln ungeputzt stehen. Bewusst dieselbe Form wie
# `injektion.TITELMARKE`: zwei Verstaendnisse derselben Marke laufen
# auseinander, sobald jemand nur eines von beiden pflegt.
_MARKS = re.compile(
    r'\[SCBPW\].*?\[/SCBPW\]'                       # der ganze eingefügte Block
    r'|<EM4>[^<>]*\[(?:BP|Bauplan)[^\]]*\][^<>]*</EM4>'   # nur die Blase
)
_PLACEHOLDER = re.compile(r'~mission\([^)]*\)')

# ⛔ Die Kennung, die „keine Kennung" bedeutet. Jede **geteilte** Auftrags-
# meldung traegt sie — an 157 Log-Sicherungen gemessen: 403 von 403.
# Wer sie wie eine echte MissionId fuehrt, wirft alle geteilten Auftraege in
# einen Topf; ein Ende raeumte dann den falschen weg.
NULL_ID = '00000000-0000-0000-0000-000000000000'

# ⛔⛔ Der Schlüssel in der `global.ini` kann eine Variantenkennung tragen:
#
#   Foxwell_DefendDestructibleEntites_H_Title_001=Orange Lvl. Contract: …
#   Foxwell_DefendEntitesAndEscort_H_Title,P=Orange Lvl. Contract: …
#                                       ^^
#
# Das `,P` ist die CryEngine-Schreibweise für eine Textvariante. Wer die Zeile
# schlicht bei `=` abschneidet, behält sie mit — und findet den Auftrag im
# Katalog dann nicht mehr. **16 Missionen waren dadurch unsichtbar**, darunter
# die mit 54 Bauplänen.
#
# ⚠ Und unsichtbar heißt hier nicht „keine Angabe": Der Platz wurde von einem
# Nachbarn eingenommen, dessen Muster auf ein bloßes Präfix zusammenfällt
# (siehe `_pattern_weak`). Gemeldet wurden dann dessen Baupläne — also die
# einer ganz anderen Mission.
_VARIANT = re.compile(r',[A-Za-z]{1,3}$')

_contract_defs = None        # {vertrag_id: {'bp': [...], 'system': [...]}}
_index = None            # {sauberer Titel: schluessel}
_pattern_index = None     # [(kompiliertes Muster, schluessel)] für Platzhalter-Titel
_missions = None        # Zwischenspeicher: der Katalog ist rund 1 MB gross


def missions():
    """Die Missionen aus dem Katalog — einmal lesen, dann gemerkt.

    `catalog.load()` liest jedes Mal die ganze Datei. Bei einem Auftrag alle
    paar Minuten faellt das nicht auf, aber es waere unnoetige Arbeit.
    """
    global _missions
    if _missions is None:
        try:
            _missions = catalog.load().get('missionen') or {}
        except Exception as exception:
            fehler.merken('contracts.catalog', exception)
            _missions = {}
    return _missions


def forget():
    """Zwischenspeicher leeren — nach einem Katalog-Update aufzurufen.

    ⚠ **Alle** Zwischenspeicher, auch neue. Bliebe einer stehen, arbeitete das
    Werkzeug nach einem Katalog-Update mit zwei Ständen gleichzeitig.
    """
    global _missions, _index, _pattern_index, _contract_defs
    _missions, _index, _pattern_index, _contract_defs = None, None, None, None


# Die `global.ini` schreibt die Meldung mit Platzhalter: `Auftrag angenommen: %s`.
# Fuer die Suche zaehlt nur der Wortlaut davor — mit dem Platzhalter passt die
# Zeile nie, weil im Log der echte Titel steht.
_PLACEHOLDER_END = re.compile(r'\s*:?\s*%[sd]\s*$')


def clean(title):
    """Titel ohne unsere Marken und ohne doppelte Leerzeichen."""
    return ' '.join(_MARKS.sub(' ', str(title)).split())


def _shorten_phrase(value):
    """Aus `Auftrag angenommen: %s` wird `Auftrag angenommen`."""
    return _PLACEHOLDER_END.sub('', clean(value)).strip()


def _phrases_for(key, fallback):
    """Den Wortlaut zu einem oder mehreren `global.ini`-Schlüsseln holen.

    Erst aus der `global.ini` des Spiels — die ist immer richtig, auch in
    Sprachen, die wir nie gesehen haben. Sonst die mitgelieferte Tabelle.
    """
    key = (key,) if isinstance(key, str) else tuple(key)
    prefixes = tuple(s + '=' for s in key)
    found = []
    for path in _ini_files():
        try:
            with open(path, encoding='utf-8', errors='ignore') as f:
                for line in f:
                    # ⚠ Kein `break` nach dem ersten Treffer mehr — es sind
                    # jetzt mehrere Schlüssel je Datei zu holen.
                    if line.startswith(prefixes):
                        value = _shorten_phrase(line.split('=', 1)[1])
                        if value and value not in found:
                            found.append(value)
        except OSError:
            continue
    for candidates in fallback.values():
        for p in candidates:
            if p not in found:
                found.append(p)
    return found


def start_phrases():
    """Womit ein Auftrag bei mir anfaengt — angenommen oder geteilt bekommen."""
    return _phrases_for(INI_KEYS, TABLE)


def end_phrases():
    """Wie die drei Enden heißen — abgeschlossen, zurückgezogen, gescheitert.

    ⚠ Der Watcher braucht sie nicht, um etwas zu melden, sondern um etwas
    **wegzunehmen**. Ein erledigter Auftrag, der stehen bleibt, ist schlimmer
    als gar keine Anzeige: Nach einem Abend mit zehn Auftraegen stuende dort
    eine Liste, von der nichts mehr stimmt.
    """
    return _phrases_for(INI_END_KEYS, END_TABLE)


def _ini_files():
    """Alle vorhandenen `global.ini` der Installation."""
    try:
        base = os.path.join(pfade.spiel_ordner() or '', 'data', 'Localization')
    except Exception:
        return []
    if not os.path.isdir(base):
        return []
    found = []
    try:
        for name in sorted(os.listdir(base)):
            p = os.path.join(base, name, 'global.ini')
            if os.path.isfile(p):
                found.append(p)
    except OSError:
        pass
    return found


def start_pattern():
    """Das fertige Suchmuster für die Log-Zeile — angenommene Auftraege."""
    parts = '|'.join(re.escape(p) for p in start_phrases())
    return re.compile(FRAME % parts)


def end_pattern():
    """Dasselbe für die drei Enden — abgeschlossen, zurückgezogen, gescheitert.

    Bewusst ein zweites Muster statt eines gemeinsamen mit Gruppen: Die beiden
    Listen kommen aus verschiedenen Schlüsseln, und ein Fehlgriff hiesse, dass
    ein abgeschlossener Auftrag als neu angenommen gilt.
    """
    parts = '|'.join(re.escape(p) for p in end_phrases())
    return re.compile(FRAME % parts)


# ⚠⚠ **Der Zusatz hinter der Meldung entscheidet, ob ein Ende zählt.**
# Star Citizen hängt an jede Auftrags-Benachrichtigung an, zu welcher Mission
# und zu welchem **Ziel** sie gehört — und daran hängt alles:
#
#     "Auftrag angenommen: Retake Platforms From Nine Tails: "
#         MissionId: [916223dd…], ObjectiveId: []
#     "Auftrag zurückgezogen: Obere Plattform erreichen: "
#         MissionId: [916223dd…], ObjectiveId: [40418b42…]
#
# Dieselbe Mission, zwei Ebenen. Die zweite Zeile nimmt **das Zwischenziel**
# weg, nicht den Auftrag — der läuft weiter, und direkt danach steht im Log
# schon das nächste Ziel. Wer den Unterschied nicht macht, löscht laufende
# Aufträge: am 31.08.2026 mit Bildschirmfoto gemeldet — Auftrag im Spiel
# sichtbar aktiv, Leiste leer.
#
# An allen 153 Protokollen gemessen (31.08.2026): 473 Enden, davon **111 mit**
# ObjectiveId. Alle 111 waren Zwischenziele, und in allen 111 Fällen lief die
# Mission danach nachweislich weiter.
SUFFIX = re.compile(r'MissionId:\s*\[([^\]]*)\][^\n]*?ObjectiveId:\s*\[([^\]]*)\]')

# ⭐⭐ **Der Log nennt den Vertrag selbst — samt Region und Stufe.**
#
#   <CLocalMissionPhaseMarker::CreateMarker> Creating objective marker:
#     missionId [7a12d7cf-…], generator name [Foxwell_DefendEntitiesAndEscort],
#     contract [Foxwell_DefendEntitiesAndEscort_Nyx_Hard],
#     contractDefinitionId[6c4b94f2-3b43-4e0a-9be5-93186e0a957e], …
#
# Die `contractDefinitionId` ist die `id` eines Vertrags im Katalog
# (`catalog._contracts`). Damit faellt der ganze Titel-Umweg weg: keine
# Marken, keine Platzhalter, keine Sprache, keine Praefix-Muster — und keine
# Namensabbildung ueber den Tippfehler `Entities`/`Entites` in der Quelle.
#
# ⚠ An 157 Log-Sicherungen gemessen (13.09.2026): Zu **687 von 707** Annahmen
# laesst sich so ein Vertrag finden (97,2 %). Die uebrigen 20 tragen keinen
# Marker — dort bleibt es beim Titelweg. Deshalb ersetzt das den alten Weg
# nicht, es geht ihm nur vor.
#
# ⚠ `missionId` steht hier klein geschrieben und ohne Doppelpunkt — anders als
# in `SUFFIX`. Zwei Schreibweisen derselben Sache im selben Log.
CONTRACT_MARKER = re.compile(
    r'CreateMarker[^\n]*?missionId \[([0-9a-fA-F-]+)\][^\n]*?'
    r'contractDefinitionId\[([0-9a-fA-F-]+)\]')


def contracts_from_text(text):
    """`{mission_id: vertrag_id}` — welcher Vertrag steckt hinter der Mission?

    ⚠ Der **erste** Marker gewinnt. Ein Auftrag setzt im Lauf mehrere Ziele,
    alle mit derselben `contractDefinitionId`; gemessen gab es ueber 157
    Protokolle keinen Fall, in dem eine MissionId zwei verschiedene Vertraege
    nannte. Faende sich doch einer, waere der erste der bei der Annahme.
    """
    found = {}
    for m in CONTRACT_MARKER.finditer(text):
        found.setdefault(m.group(1), m.group(2))
    return found

# ⚠⚠ **Ein Auftrag kann enden, ohne dass es eine Meldung dazu gibt.**
# Gemeldet am 04.09.2026: Ein Auftrag wurde angenommen und war vier Sekunden
# spaeter wieder weg, weil ein anderer Spieler schneller war. Im Protokoll
# steht dazu **keine** „Auftrag abgeschlossen"-Meldung, nur diese Zeile:
#
#   <EndMission> … MissionId[e0b968d5-…] … CompletionType[Abandon]
#                                          Reason[Player left]
#
# Wer nur auf die Meldungen hoert, fuehrt so einen Auftrag fuer immer als
# laufend — im Overlay standen zwei, im Spiel war einer.
#
# ⚠ Die Kennung steht hier **ohne** Doppelpunkt und ohne ObjectiveId daneben,
# `SUFFIX` greift also nicht. Sie wird deshalb gleich hier mitgelesen.
#
# ⚠ Bewusst ohne Blick auf `CompletionType`: Ob abgeschlossen oder abgebrochen
# — beides heisst, dass der Auftrag nicht mehr laeuft. Fuer die Live-Anzeige
# ist das die ganze Frage.
ENDMISSION = re.compile(r'<EndMission>[^\n]*?MissionId\[([^\]]*)\]')


# ⚠⚠ **Wer die Spielwelt verlässt, verliert seine Aufträge — lautlos.**
# Star Citizen meldet beim Ausloggen **kein einziges** Auftrags-Ende. Im
# Auftragsbuch ist danach trotzdem alles weg. Wer nur auf Enden hört, führt
# Aufträge von vorgestern als „laufend" — gemeldet am 31.08.2026: Das Spiel war
# nicht einmal gestartet, und in der Leiste stand „Willkommen im System".
#
# Der Marker ist sprachneutral und deckt **beide** Fälle ab: zurück ins
# Hauptmenü und Spiel beenden. Er steht in jeder Fassung an derselben Stelle:
#
#     [CSessionManager::RequestFrontEnd] Started - RequestFrontEndReason="…"!
#
# An 23 Protokollen gemessen (31.08.2026): **39** Ausloggen-Marker, 19
# Annahmen, 3 echte Enden, 87 Zwischenziele. Kein einziger Auftrag hat ein
# Ausloggen überlebt — es gibt **0** Fälle, in denen nach einem Marker noch ein
# Ende für einen davor angenommenen Auftrag kam.
#
# ⚠ Das ist **nicht** das pauschale Räumen aus v3.4.4. Dort räumte ein Ende,
# das sich keinem Auftrag zuordnen ließ — geraten also. Hier sagt das Spiel
# selbst, dass die Spielwelt verlassen wurde. Der Unterschied ist der zwischen
# „ich weiß nicht, was das war" und „der Spieler ist raus".
LEFT_GAME = re.compile(r'CSessionManager::RequestFrontEnd\]\s*Started')


def identifiers(text, position):
    """`(MissionId, ObjectiveId)` der Meldung, die bei `stelle` beginnt.

    Beide stehen am Ende **derselben** Logzeile. Fehlen sie — fremdes Format,
    ältere Spielfassung, ein von Hand gebauter Testtext —, kommt zweimal `''`
    zurück und es wird wie früher über den Titel gerechnet.
    """
    end = text.find('\n', position)
    line = text[position:end if end >= 0 else len(text)]
    hit = SUFFIX.search(line)
    if not hit:
        return '', ''
    return hit.group(1).strip(), hit.group(2).strip()


def events_from_text(text, start_pat=None, end_pat=None):
    """Alle Auftrags-Ereignisse dieses Textes, in der Reihenfolge des Logs.

    Einträge: `(ist_annahme, titel, mission_id, objective_id)`.

    `ist_annahme` kennt **drei** Werte:

    | Wert | Bedeutung |
    |---|---|
    | `True` | Auftrag angenommen |
    | `False` | Auftrag beendet (abgeschlossen, abgebrochen, gescheitert) |
    | `None` | **Spielwelt verlassen** — alles Offene ist weg, siehe `LEFT_GAME` |

    ⚠ Die **eine** Stelle, die Auftragsmeldungen aus einem Logtext holt: Der
    Start liest damit die ganze `Game.log`, der laufende Betrieb damit jeden
    neuen Abschnitt. Zwei Auswertungen mit eigener Buchführung liefen früher
    auseinander.
    """
    start_pat = start_pat or start_pattern()
    end_pat = end_pat or end_pattern()
    found = []
    for m in start_pat.finditer(text):
        found.append((m.start(), True, m.group(1), None))
    for m in end_pat.finditer(text):
        found.append((m.start(), False, m.group(1), None))
    for m in LEFT_GAME.finditer(text):
        found.append((m.start(), None, '', None))
    # ⚠ Das stille Ende — siehe `ENDMISSION`. Ohne Titel, dafuer mit der
    # Kennung: `which_ended` findet den Auftrag ueber sie (Schritt 3 dort).
    for m in ENDMISSION.finditer(text):
        found.append((m.start(), False, '', m.group(1).strip()))
    # Die Fundstelle ist die Wahrheit: Sie sagt, was im Spiel zuerst geschah.
    found.sort(key=lambda e: e[0])
    result = []
    for position, is_accept, title, mid in found:
        # ⛔⛔ **Eine ANNAHME mit unaufgelöstem `~mission(...)` fällt hier raus
        # — an der Quelle, nicht in der Buchführung.**
        #
        # Das Spiel meldet denselben Auftrag zweimal, eine Sekunde auseinander:
        #
        #   "Auftrag geteilt:    … Stop Rival Attack at ~mission(Location): "
        #                        MissionId: [00000000-0000-0000-0000-…]
        #   "Auftrag angenommen: … Stop Rival Attack at Asteroiden Bergbau…: "
        #                        MissionId: [4f1e…]
        #
        # Beide gelten als Annahme (`INI_KEYS`), also standen zwei Zeilen
        # da — und die erste ging nicht von selbst weg, weil das Ende nur den
        # aufgelösten Titel trägt.
        #
        # ⚠⚠ **v3.32.3 hat das in `state_from_text` gefiltert. Das war zu weit
        # unten.** Es gibt ZWEI Buchführungen: `state_from_text` für den Start
        # und `_auftraege_melden()` im Watcher für den laufenden Betrieb — und
        # die zweite baut ihre Liste direkt aus diesen Ereignissen. Der Filter
        # griff also nur beim Start; beim nächsten angenommenen Auftrag stand
        # der Doppeleintrag wieder da. Dieselbe Falle wie bei den zwei
        # Schreibwegen in die `global.ini` (siehe Projektregeln).
        #
        # An allen 157 Log-Sicherungen gemessen: 146 Platzhalter-Titel, davon
        # **146 aus der geteilten Meldung und 0 aus einer Annahme**.
        #
        # ⚠ Nur Annahmen. Ein ENDE mit Platzhalter muss durch — sonst bliebe
        # der Auftrag für immer stehen, und das ist der schlimmere Fehler.
        if is_accept is True and _PLACEHOLDER.search(title or ''):
            continue
        if mid is None:
            result.append((is_accept, title) + identifiers(text, position))
        else:
            # ⚠ Keine ObjectiveId: Ein `EndMission` beendet den ganzen Auftrag,
            # nie ein Zwischenziel. Stuende hier eine, wuerde
            # `which_ended` das Ende als Zwischenziel abtun.
            result.append((is_accept, title, mid, ''))
    return result


def which_ended(plain, mission_id, objective_id, pending, mission_keys):
    """Welchen offenen Auftrag beendet dieses Ende — oder keinen (`None`)?

    Drei Schritte, in dieser Reihenfolge:

    1. **Steht eine ObjectiveId dabei, endet nur ein Zwischenziel.** Der
       Auftrag läuft weiter. Der Grund steht oben bei `SUFFIX`.
    2. Sonst über den Titel — der Normalfall, 300 von 362 gemessen.
    3. Sonst über die MissionId. Sie steht bei **jeder** der 1102 gemessenen
       Annahmen und bei **jedem** der 362 Missions-Enden. Damit sind auch die
       restlichen 62 zugeordnet, bei denen der Endtitel vom Annahmetitel
       abweicht.

    Ergebnis über 153 Protokolle: **0** Missions-Enden bleiben unzuordenbar.
    Deshalb wird hier weder geraten noch pauschal geräumt — beides hatte
    laufende Aufträge mitgerissen.
    """
    if objective_id:
        return None
    if plain in pending:
        return plain
    return mission_keys.get(mission_id) if mission_id else None


def state_from_text(text, start_pat=None, end_pat=None):
    """Was laut diesem Logtext noch offen ist — mit den Missions-Kennungen.

    Rückgabe: `(titel_liste, {mission_id: schlüssel})`. Die zweite Hälfte
    braucht der laufende Betrieb: Endet später ein Auftrag, der **vor** dem
    Start des Werkzeugs angenommen wurde, ist die MissionId die einzige
    Brücke zurück zu seiner Zeile.
    """
    pending, mission_keys = {}, {}
    for is_accept, title, mid, oid in events_from_text(text, start_pat,
                                                            end_pat):
        # ⚠ Vor der Titelprüfung: Das Verlassen trägt keinen Titel.
        if is_accept is None:
            pending.clear()
            mission_keys.clear()
            continue
        plain = clean(title)
        # ⚠⚠ **Ein Ende darf titellos sein, eine Annahme nicht.** Das stille
        # Ende aus `<EndMission>` (siehe `ENDMISSION`) traegt nur die Kennung —
        # und genau die genuegt, `which_ended` findet den Auftrag darueber.
        # Stand hier bis zum 04.09.2026 ein pauschales `if not rein: continue`,
        # wurde es verworfen, bevor es zum Zuge kam: Ein Auftrag, den ein
        # anderer Spieler wegschnappte, blieb fuer immer als laufend stehen.
        if not plain and not (mid and not is_accept):
            continue
        if is_accept:
            # ⚠ Platzhalter-Titel sind hier schon weg — `events_from_text`
            # laesst sie gar nicht erst durch. Dort steht auch, warum: Es gibt
            # ZWEI Buchfuehrungen, und ein Filter an nur einer wirkt nur zur
            # Haelfte. Genau daran ist v3.32.3 gescheitert.
            pending.setdefault(plain, title)
            # ⚠ Die Nullkennung ist keine Kennung. Sie als solche zu führen
            # hiesse, alle geteilten Auftraege in einen Topf zu werfen — und
            # ein Ende raeumte dann den falschen weg.
            if mid and mid != NULL_ID:
                mission_keys[mid] = plain
            continue
        gone = which_ended(plain, mid, oid, pending, mission_keys)
        if gone is None:
            continue
        pending.pop(gone, None)
        for ident in [k for k, v in mission_keys.items() if v == gone]:
            del mission_keys[ident]
    return list(pending.values()), mission_keys


def open_from_text(text, start_pat=None, end_pat=None):
    """Welche Aufträge laut diesem Log-Text noch offen sind.

    Geht den Text **in seiner Reihenfolge** durch und führt Buch: Eine Annahme
    legt den Titel ab, ein Ende nimmt ihn wieder weg. Was am Schluss übrig
    bleibt, lief zu diesem Zeitpunkt noch.

    ⚠ Verglichen wird über `clean()`, also ohne unsere eingefügten Marken —
    im Log steht der Titel mit `[SCBPW]…[/SCBPW]` darin, und beim Abschluss
    kann die Bauplan-Blase eine andere Zahl tragen als bei der Annahme.

    Gibt die Titel in der Reihenfolge der Annahme zurück.
    """
    return state_from_text(text, start_pat, end_pat)[0]


# ---------------------------------------------------------------------------
# Zwischenziele — was gerade zu tun ist
# ---------------------------------------------------------------------------
#
# Der Auftrag sagt, ob Baupläne drin sind. Das Zwischenziel sagt, **wofür man
# gerade fliegt**. Beides steht im Protokoll, an zwei verschiedenen Stellen:
#
# | | Quelle | sprachneutral? |
# |---|---|---|
# | Zustand | `<ObjectiveUpserted> … state MISSION_OBJECTIVE_STATE_…` | ja |
# | Wortlaut | `Added notification "…: <Ziel>: " … ObjectiveId: [x]` | nein |
#
# ⚠ **Der Zustand kommt aus der sprachneutralen Zeile, nie aus dem Wortlaut.**
# Dieselbe Falle wie bei den Aufträgen: Auf Deutsch heißt die Ziel-Annahme
# „Neuer Auftrag" — genau wie eine Auftrags-Meldung. Wer darauf hört, zählt
# falsch. `ObjectiveUpserted` steht in jeder Sprache gleich da.
#
# ⚠ **Der Wortlaut wird über die ObjectiveId zugeordnet, nicht über die
# Phrase.** Damit ist es egal, wie die Meldung heißt und in welcher Sprache
# sie steht.
OBJECTIVE_STATE = re.compile(
    r'<ObjectiveUpserted>[^\n]*?mission_id (\S+) - objective_id (\S+) - '
    r'state MISSION_OBJECTIVE_STATE_(\w+)[^\n]*?flags=(\S*)')

OBJECTIVE_TITLE = re.compile(
    r'Added notification "[^"\n]*?:\s*(.+?)\s*:\s*"[^\n]*?'
    r'ObjectiveId: \[([0-9a-fA-F][0-9a-fA-F-]{7,})\]')

# ⚠ **Nur was das Spiel selbst ins Auftragsbuch schreibt.** Ein Auftrag führt
# neben den sichtbaren Zielen eine Menge interner (`SilentUpdates`, `Hidden`) —
# Zähler, Auslöser, Zonenwächter. Über alle 153 Protokolle gemessen: von 2832
# Zielen tragen 456 zwar `ShowInLog`, aber keinen Wortlaut; **kein einziges**
# hat einen Wortlaut ohne `ShowInLog`. Das Kennzeichen kostet also nichts und
# hält den halben Maschinenraum draußen.
OBJECTIVE_VISIBLE = 'ShowInLog'

# Wie viele Ziele höchstens untereinander stehen. Gemessen an denselben
# Protokollen: 182 von 226 Aufträgen haben **ein** offenes Ziel, der Ausreißer
# hatte sechs. Die Grenze schützt nur vor dem unbekannten Fall — das Overlay
# darf nicht die Bauplan-Liste vom Bildschirm schieben.
OBJECTIVES_MAX = 6


def objective_events_from_text(text):
    """Alle Ziel-Meldungen dieses Textes, in der Reihenfolge des Logs.

    Zwei Sorten, beide als Tupel:

    * `('zustand', mission_id, objective_id, zustand, kennzeichen)`
    * `('titel', objective_id, wortlaut)`

    Roh und ungewertet — was daraus wird, entscheidet `Objectives`.
    """
    found = []
    for m in OBJECTIVE_STATE.finditer(text):
        found.append((m.start(), ('zustand', m.group(1), m.group(2),
                                     m.group(3), m.group(4))))
    for m in OBJECTIVE_TITLE.finditer(text):
        found.append((m.start(), ('titel', m.group(2), m.group(1).strip())))
    found.sort(key=lambda e: e[0])
    return [e for _stelle, e in found]


class Objectives:
    """Buchführung über die Zwischenziele — was zu diesem Auftrag ansteht.

    Ein Zustand, keine Verlaufsliste: `aufnehmen()` frisst Abschnitt für
    Abschnitt, `offen()` sagt jederzeit, was gerade dransteht. Damit rechnen
    Start (ganzes Protokoll) und laufender Betrieb (neuer Abschnitt) über
    dieselbe Stelle — genau wie bei den Aufträgen.
    """

    def __init__(self):
        self._titles = {}          # objective_id -> Wortlaut
        self._states = {}          # mission_id -> {objective_id: (zustand, kennz.)}

    def absorb(self, events):
        """Einen Abschnitt verbuchen. Sagt, ob sich etwas geändert hat.

        ⚠ Der Rückgabewert ist wichtig: Ziele wechseln, **ohne** dass sich die
        Auftragsliste ändert. Ohne dieses Ja stünde in der Leiste noch das
        Ziel von vor zwanzig Minuten.
        """
        changed = False
        for e in events or ():
            if e[0] == 'titel':
                _kind, oid, wording = e
                if wording and self._titles.get(oid) != wording:
                    self._titles[oid] = wording
                    changed = True
                continue
            _kind, mid, oid, state, flags = e
            per_mission = self._states.setdefault(mid, {})
            if per_mission.get(oid) != (state, flags):
                per_mission[oid] = (state, flags)
                changed = True
        return changed

    def open_for(self, mission_id):
        """Die offenen Ziele dieses Auftrags, in der Reihenfolge des Logs.

        ⚠ **Ohne Wortlaut wird geschwiegen.** Ein Ziel, dessen Meldung wir nicht
        gesehen haben, bekommt hier keine Zeile — dieselbe Linie wie überall:
        lieber nichts zeigen als etwas Falsches behaupten.
        """
        names = []
        for oid, (state, flags) in self._states.get(mission_id,
                                                           {}).items():
            if state != 'INPROGRESS' or OBJECTIVE_VISIBLE not in flags:
                continue
            wording = self._titles.get(oid)
            if wording and wording not in names:
                names.append(wording)
        return names

    def forget(self, mission_id):
        """Der Auftrag ist vorbei — seine Ziele auch."""
        self._states.pop(mission_id, None)


def _build_index():
    """Titel → Missionsschlüssel, aus der `global.ini` und dem Katalog.

    Es werden nur die Schlüssel aufgenommen, die der Katalog überhaupt kennt —
    die `global.ini` hat über 9.000 Einträge, davon sind 353 für uns relevant.
    """
    global _index, _pattern_index
    _index, _pattern_index = {}, []
    known = set(missions())
    if not known:
        return
    for path in _ini_files():
        try:
            with open(path, encoding='utf-8', errors='ignore') as f:
                for line in f:
                    sep = line.find('=')
                    if sep < 1:
                        continue
                    key = line[:sep]
                    if key not in known:
                        # ⛔ Variantenkennung abstreifen (`…_Title,P`) und noch
                        # einmal nachsehen — siehe `_VARIANT`.
                        key = _VARIANT.sub('', key)
                        if key not in known:
                            continue
                    title = clean(line[sep + 1:])
                    if not title:
                        continue
                    if '~mission(' in title:
                        # Platzhalter -> Muster. Der Rest wird woertlich
                        # genommen, damit „High-Risk Bounty: X" nicht auf
                        # „Low-Risk Bounty: X" passt.
                        raw = '^' + '.+'.join(
                            re.escape(t) for t in _PLACEHOLDER.split(title)) + '$'
                        try:
                            _pattern_index.append((re.compile(raw), key))
                        except re.error:
                            pass
                    else:
                        _index.setdefault(title.lower(), key)
        except OSError:
            continue


# ⛔⛔ Ein Muster, dessen Platzhalter am ZEILENENDE steht, endet auf `.+$` —
# und ist damit nur noch ein Präfix. `^Orange Lvl. Contract: Protect .+$` passt
# auf **jeden** Auftrag dieser Familie, nicht auf einen bestimmten.
#
# Der Kommentar bei `_build_index` verspricht, der Rest werde wörtlich
# genommen, damit „High-Risk Bounty: X" nicht auf „Low-Risk Bounty: X" passt.
# Das stimmt — aber nur, solange der Platzhalter **in der Mitte** steht.
# Gemessen am 13.09.2026: **70 von 116** Mustern enden so.
def _pattern_weak(pattern):
    """Fällt dieses Muster auf ein bloßes Präfix zusammen?"""
    return pattern.endswith('.+$')


def _weight(pattern):
    """Wie viel wörtlicher Text steht in diesem Muster — je mehr, desto genauer."""
    raw = pattern[1:-1] if pattern.startswith('^') and pattern.endswith('$') else pattern
    return sum(len(teil.replace('\\', '')) for teil in raw.split('.+'))


def key_for(title):
    """Der Missionsschlüssel zu einem angezeigten Titel, oder None.

    Drei Stufen, in dieser Reihenfolge:

    1. Der **wörtliche** Titel — eindeutig, kein Spielraum.
    2. Unter den passenden Mustern das **genaueste**: ein Muster mit
       Platzhalter in der Mitte schlägt ein bloßes Präfix, und bei gleicher
       Bauart gewinnt das mit mehr wörtlichem Text.
    3. Zeigen die gleich guten Muster auf **verschiedene** Aufträge, wird
       geschwiegen.

    ⚠⚠ Punkt 3 ist der Kern. Vorher gewann schlicht das erste Muster in der
    Liste — und damit meldete das Werkzeug die Baupläne einer **anderen**
    Mission. `_auftrag_zeile` sagt es im eigenen Docstring: Eine erfundene
    Bauplan-Angabe ist schlimmer als keine.
    """
    if _index is None:
        _build_index()
    plain = clean(title)
    hit = _index.get(plain.lower())
    if hit:
        return hit
    matching = [(mst.pattern, key)
               for mst, key in _pattern_index if mst.match(plain)]
    if not matching:
        return None
    # Genauer heisst: kein blosses Praefix, und mehr woertlicher Text.
    rank = max((not _pattern_weak(p), _weight(p)) for p, _s in matching)
    best = {s for p, s in matching
             if (not _pattern_weak(p), _weight(p)) == rank}
    if len(best) != 1:
        return None
    return best.pop()


def contract_definitions():
    """Die Verträge aus dem Katalog — einmal lesen, dann gemerkt.

    ⚠ Fehlt der Abschnitt (Katalog vor FORMAT 3), kommt ein leeres
    Wörterbuch — dann gilt weiter der Titelweg, wie bis v3.32.4.
    """
    global _contract_defs
    if _contract_defs is None:
        try:
            _contract_defs = catalog.load().get('vertraege') or {}
        except Exception as exception:
            fehler.merken('contracts.contract_definitions', exception)
            _contract_defs = {}
    return _contract_defs


def check(title, has_already, contract_id=None):
    """Was bringt dieser Auftrag — und was davon fehlt noch?

    `hat_bereits` ist eine Funktion `name -> bool`. Rückgabe ist `None`, wenn
    der Auftrag unbekannt ist oder keine Baupläne bringt; sonst
    `(gesamtzahl, [fehlende Namen])`.

    ⭐⭐ **`vertrag_id` schlägt den Titel.** Sie kommt aus der
    `CreateMarker`-Zeile des Logs (`contractDefinitionId`, siehe
    `contracts_from_text`) und trifft **genau einen** Vertrag — mit dessen
    eigener Bauplanliste statt der über alle Regionen zusammengefassten:

        Foxwell_DefendEntitesAndEscort_H_Title   54   (Titelweg)
          ├─ …_Nyx_Hard      23                       (Vertragsweg)
          ├─ …_Pyro_Hard     19
          └─ …_Stanton_Hard  12

    Wer den Auftrag in Nyx annimmt, sieht im Spiel 23 — und bekam bis v3.32.4
    die 54 gemeldet. Nicht falsch, aber nicht die Frage, die er hat.

    ⚠ **Der Titelweg bleibt der Rückfall**, nicht der Ersatz: Zu 20 von 707
    gemessenen Annahmen gibt es keinen Marker, und ein Katalog vor FORMAT 3
    kennt die Verträge gar nicht.
    """
    if contract_id:
        entry = contract_definitions().get(contract_id) or {}
        names = [n for n in (entry.get('bp') or []) if n]
        if names:
            missing = [n for n in names if not has_already(n)]
            return len(names), missing
    key = key_for(title)
    if not key:
        return None
    entry = missions().get(key) or {}
    names = [n for n in (entry.get('bp') or []) if n]
    if not names:
        return None
    missing = [n for n in names if not has_already(n)]
    return len(names), missing
