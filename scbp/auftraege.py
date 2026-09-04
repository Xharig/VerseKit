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

from . import fehler, katalog, pfade

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
INI_SCHLUESSEL = ('mobiGlas_ui_MissionEvent_Activated',
                  'mobiGlas_ui_Mission_Shared')

# Und die drei Arten, wie ein Auftrag endet. ⚠ Ohne sie hielte der Watcher
# jeden Auftrag für ewig offen: Wer zehn hintereinander macht, haette zehn
# Zeilen stehen, von denen neun erledigt sind.
#
# `Deactivate` heisst im Spiel „zurückgezogen" — das ist der Abbruch von Hand.
INI_ENDE = (
    'mobiGlas_ui_MissionEvent_Complete',      # abgeschlossen
    'mobiGlas_ui_MissionEvent_Deactivate',    # zurückgezogen (abgebrochen)
    'mobiGlas_ui_MissionEvent_Fail',          # fehlgeschlagen
)

# Rückfall, falls die `global.ini` nicht vorliegt. Beide an echten Logs gemessen
# (aus echten Log-Sicherungen, 29.08.2026: 701 Annahmen, 303 Abschluesse, 112
# Ruecknahmen, 57 Fehlschlaege).
TABELLE = {
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
TABELLE_ENDE = {
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
RAHMEN = r'Added notification "(?:%s):\s*(.+?)\s*:\s*"'

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
_MARKEN = re.compile(
    r'\[SCBPW\].*?\[/SCBPW\]'                       # der ganze eingefügte Block
    r'|<EM4>[^<>]*\[(?:BP|Bauplan)[^\]]*\][^<>]*</EM4>'   # nur die Blase
)
_PLATZHALTER = re.compile(r'~mission\([^)]*\)')

_index = None            # {sauberer Titel: schluessel}
_muster_index = None     # [(kompiliertes Muster, schluessel)] für Platzhalter-Titel
_missionen = None        # Zwischenspeicher: der Katalog ist rund 1 MB gross


def missionen():
    """Die Missionen aus dem Katalog — einmal lesen, dann gemerkt.

    `katalog.laden()` liest jedes Mal die ganze Datei. Bei einem Auftrag alle
    paar Minuten faellt das nicht auf, aber es waere unnoetige Arbeit.
    """
    global _missionen
    if _missionen is None:
        try:
            _missionen = katalog.laden().get('missionen') or {}
        except Exception as ausnahme:
            fehler.merken('auftraege.katalog', ausnahme)
            _missionen = {}
    return _missionen


def vergessen():
    """Zwischenspeicher leeren — nach einem Katalog-Update aufzurufen."""
    global _missionen, _index, _muster_index
    _missionen, _index, _muster_index = None, None, None


# Die `global.ini` schreibt die Meldung mit Platzhalter: `Auftrag angenommen: %s`.
# Fuer die Suche zaehlt nur der Wortlaut davor — mit dem Platzhalter passt die
# Zeile nie, weil im Log der echte Titel steht.
_PLATZHALTER_ENDE = re.compile(r'\s*:?\s*%[sd]\s*$')


def sauber(titel):
    """Titel ohne unsere Marken und ohne doppelte Leerzeichen."""
    return ' '.join(_MARKEN.sub(' ', str(titel)).split())


def _phrase_kuerzen(wert):
    """Aus `Auftrag angenommen: %s` wird `Auftrag angenommen`."""
    return _PLATZHALTER_ENDE.sub('', sauber(wert)).strip()


def _phrasen_zu(schluessel, rueckfall):
    """Den Wortlaut zu einem oder mehreren `global.ini`-Schlüsseln holen.

    Erst aus der `global.ini` des Spiels — die ist immer richtig, auch in
    Sprachen, die wir nie gesehen haben. Sonst die mitgelieferte Tabelle.
    """
    schluessel = (schluessel,) if isinstance(schluessel, str) else tuple(schluessel)
    anfaenge = tuple(s + '=' for s in schluessel)
    gefunden = []
    for pfad in _ini_dateien():
        try:
            with open(pfad, encoding='utf-8', errors='ignore') as f:
                for zeile in f:
                    # ⚠ Kein `break` nach dem ersten Treffer mehr — es sind
                    # jetzt mehrere Schlüssel je Datei zu holen.
                    if zeile.startswith(anfaenge):
                        wert = _phrase_kuerzen(zeile.split('=', 1)[1])
                        if wert and wert not in gefunden:
                            gefunden.append(wert)
        except OSError:
            continue
    for liste in rueckfall.values():
        for p in liste:
            if p not in gefunden:
                gefunden.append(p)
    return gefunden


def phrasen():
    """Womit ein Auftrag bei mir anfaengt — angenommen oder geteilt bekommen."""
    return _phrasen_zu(INI_SCHLUESSEL, TABELLE)


def ende_phrasen():
    """Wie die drei Enden heißen — abgeschlossen, zurückgezogen, gescheitert.

    ⚠ Der Watcher braucht sie nicht, um etwas zu melden, sondern um etwas
    **wegzunehmen**. Ein erledigter Auftrag, der stehen bleibt, ist schlimmer
    als gar keine Anzeige: Nach einem Abend mit zehn Auftraegen stuende dort
    eine Liste, von der nichts mehr stimmt.
    """
    return _phrasen_zu(INI_ENDE, TABELLE_ENDE)


def _ini_dateien():
    """Alle vorhandenen `global.ini` der Installation."""
    try:
        basis = os.path.join(pfade.spiel_ordner() or '', 'data', 'Localization')
    except Exception:
        return []
    if not os.path.isdir(basis):
        return []
    gefunden = []
    try:
        for name in sorted(os.listdir(basis)):
            p = os.path.join(basis, name, 'global.ini')
            if os.path.isfile(p):
                gefunden.append(p)
    except OSError:
        pass
    return gefunden


def muster():
    """Das fertige Suchmuster für die Log-Zeile — angenommene Auftraege."""
    teile = '|'.join(re.escape(p) for p in phrasen())
    return re.compile(RAHMEN % teile)


def ende_muster():
    """Dasselbe für die drei Enden — abgeschlossen, zurückgezogen, gescheitert.

    Bewusst ein zweites Muster statt eines gemeinsamen mit Gruppen: Die beiden
    Listen kommen aus verschiedenen Schlüsseln, und ein Fehlgriff hiesse, dass
    ein abgeschlossener Auftrag als neu angenommen gilt.
    """
    teile = '|'.join(re.escape(p) for p in ende_phrasen())
    return re.compile(RAHMEN % teile)


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
ZUSATZ = re.compile(r'MissionId:\s*\[([^\]]*)\][^\n]*?ObjectiveId:\s*\[([^\]]*)\]')


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
VERLASSEN = re.compile(r'CSessionManager::RequestFrontEnd\]\s*Started')


def kennungen(text, stelle):
    """`(MissionId, ObjectiveId)` der Meldung, die bei `stelle` beginnt.

    Beide stehen am Ende **derselben** Logzeile. Fehlen sie — fremdes Format,
    ältere Spielfassung, ein von Hand gebauter Testtext —, kommt zweimal `''`
    zurück und es wird wie früher über den Titel gerechnet.
    """
    ende = text.find('\n', stelle)
    zeile = text[stelle:ende if ende >= 0 else len(text)]
    treffer = ZUSATZ.search(zeile)
    if not treffer:
        return '', ''
    return treffer.group(1).strip(), treffer.group(2).strip()


def ereignisse_aus_text(text, muster_an=None, muster_aus=None):
    """Alle Auftrags-Ereignisse dieses Textes, in der Reihenfolge des Logs.

    Einträge: `(ist_annahme, titel, mission_id, objective_id)`.

    `ist_annahme` kennt **drei** Werte:

    | Wert | Bedeutung |
    |---|---|
    | `True` | Auftrag angenommen |
    | `False` | Auftrag beendet (abgeschlossen, abgebrochen, gescheitert) |
    | `None` | **Spielwelt verlassen** — alles Offene ist weg, siehe `VERLASSEN` |

    ⚠ Die **eine** Stelle, die Auftragsmeldungen aus einem Logtext holt: Der
    Start liest damit die ganze `Game.log`, der laufende Betrieb damit jeden
    neuen Abschnitt. Zwei Auswertungen mit eigener Buchführung liefen früher
    auseinander.
    """
    muster_an = muster_an or muster()
    muster_aus = muster_aus or ende_muster()
    gefunden = []
    for m in muster_an.finditer(text):
        gefunden.append((m.start(), True, m.group(1)))
    for m in muster_aus.finditer(text):
        gefunden.append((m.start(), False, m.group(1)))
    for m in VERLASSEN.finditer(text):
        gefunden.append((m.start(), None, ''))
    # Die Fundstelle ist die Wahrheit: Sie sagt, was im Spiel zuerst geschah.
    gefunden.sort(key=lambda e: e[0])
    return [(ist_annahme, titel) + kennungen(text, stelle)
            for stelle, ist_annahme, titel in gefunden]


def beendet_welchen(rein, mission_id, objective_id, offen, missionen):
    """Welchen offenen Auftrag beendet dieses Ende — oder keinen (`None`)?

    Drei Schritte, in dieser Reihenfolge:

    1. **Steht eine ObjectiveId dabei, endet nur ein Zwischenziel.** Der
       Auftrag läuft weiter. Der Grund steht oben bei `ZUSATZ`.
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
    if rein in offen:
        return rein
    return missionen.get(mission_id) if mission_id else None


def stand_aus_text(text, muster_an=None, muster_aus=None):
    """Was laut diesem Logtext noch offen ist — mit den Missions-Kennungen.

    Rückgabe: `(titel_liste, {mission_id: schlüssel})`. Die zweite Hälfte
    braucht der laufende Betrieb: Endet später ein Auftrag, der **vor** dem
    Start des Werkzeugs angenommen wurde, ist die MissionId die einzige
    Brücke zurück zu seiner Zeile.
    """
    offen, missionen = {}, {}
    for ist_annahme, titel, mid, oid in ereignisse_aus_text(text, muster_an,
                                                            muster_aus):
        # ⚠ Vor der Titelprüfung: Das Verlassen trägt keinen Titel.
        if ist_annahme is None:
            offen.clear()
            missionen.clear()
            continue
        rein = sauber(titel)
        if not rein:
            continue
        if ist_annahme:
            offen.setdefault(rein, titel)
            if mid:
                missionen[mid] = rein
            continue
        weg = beendet_welchen(rein, mid, oid, offen, missionen)
        if weg is None:
            continue
        offen.pop(weg, None)
        for kennung in [k for k, v in missionen.items() if v == weg]:
            del missionen[kennung]
    return list(offen.values()), missionen


def offene_aus_text(text, muster_an=None, muster_aus=None):
    """Welche Aufträge laut diesem Log-Text noch offen sind.

    Geht den Text **in seiner Reihenfolge** durch und führt Buch: Eine Annahme
    legt den Titel ab, ein Ende nimmt ihn wieder weg. Was am Schluss übrig
    bleibt, lief zu diesem Zeitpunkt noch.

    ⚠ Verglichen wird über `sauber()`, also ohne unsere eingefügten Marken —
    im Log steht der Titel mit `[SCBPW]…[/SCBPW]` darin, und beim Abschluss
    kann die Bauplan-Blase eine andere Zahl tragen als bei der Annahme.

    Gibt die Titel in der Reihenfolge der Annahme zurück.
    """
    return stand_aus_text(text, muster_an, muster_aus)[0]


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
ZIEL_ZUSTAND = re.compile(
    r'<ObjectiveUpserted>[^\n]*?mission_id (\S+) - objective_id (\S+) - '
    r'state MISSION_OBJECTIVE_STATE_(\w+)[^\n]*?flags=(\S*)')

ZIEL_TITEL = re.compile(
    r'Added notification "[^"\n]*?:\s*(.+?)\s*:\s*"[^\n]*?'
    r'ObjectiveId: \[([0-9a-fA-F][0-9a-fA-F-]{7,})\]')

# ⚠ **Nur was das Spiel selbst ins Auftragsbuch schreibt.** Ein Auftrag führt
# neben den sichtbaren Zielen eine Menge interner (`SilentUpdates`, `Hidden`) —
# Zähler, Auslöser, Zonenwächter. Über alle 153 Protokolle gemessen: von 2832
# Zielen tragen 456 zwar `ShowInLog`, aber keinen Wortlaut; **kein einziges**
# hat einen Wortlaut ohne `ShowInLog`. Das Kennzeichen kostet also nichts und
# hält den halben Maschinenraum draußen.
ZIEL_SICHTBAR = 'ShowInLog'

# Wie viele Ziele höchstens untereinander stehen. Gemessen an denselben
# Protokollen: 182 von 226 Aufträgen haben **ein** offenes Ziel, der Ausreißer
# hatte sechs. Die Grenze schützt nur vor dem unbekannten Fall — das Overlay
# darf nicht die Bauplan-Liste vom Bildschirm schieben.
ZIELE_MAX = 6


def ziel_ereignisse_aus_text(text):
    """Alle Ziel-Meldungen dieses Textes, in der Reihenfolge des Logs.

    Zwei Sorten, beide als Tupel:

    * `('zustand', mission_id, objective_id, zustand, kennzeichen)`
    * `('titel', objective_id, wortlaut)`

    Roh und ungewertet — was daraus wird, entscheidet `Ziele`.
    """
    gefunden = []
    for m in ZIEL_ZUSTAND.finditer(text):
        gefunden.append((m.start(), ('zustand', m.group(1), m.group(2),
                                     m.group(3), m.group(4))))
    for m in ZIEL_TITEL.finditer(text):
        gefunden.append((m.start(), ('titel', m.group(2), m.group(1).strip())))
    gefunden.sort(key=lambda e: e[0])
    return [e for _stelle, e in gefunden]


class Ziele:
    """Buchführung über die Zwischenziele — was zu diesem Auftrag ansteht.

    Ein Zustand, keine Verlaufsliste: `aufnehmen()` frisst Abschnitt für
    Abschnitt, `offen()` sagt jederzeit, was gerade dransteht. Damit rechnen
    Start (ganzes Protokoll) und laufender Betrieb (neuer Abschnitt) über
    dieselbe Stelle — genau wie bei den Aufträgen.
    """

    def __init__(self):
        self._titel = {}          # objective_id -> Wortlaut
        self._stand = {}          # mission_id -> {objective_id: (zustand, kennz.)}

    def aufnehmen(self, ereignisse):
        """Einen Abschnitt verbuchen. Sagt, ob sich etwas geändert hat.

        ⚠ Der Rückgabewert ist wichtig: Ziele wechseln, **ohne** dass sich die
        Auftragsliste ändert. Ohne dieses Ja stünde in der Leiste noch das
        Ziel von vor zwanzig Minuten.
        """
        veraendert = False
        for e in ereignisse or ():
            if e[0] == 'titel':
                _art, oid, wortlaut = e
                if wortlaut and self._titel.get(oid) != wortlaut:
                    self._titel[oid] = wortlaut
                    veraendert = True
                continue
            _art, mid, oid, zustand, kennzeichen = e
            je_mission = self._stand.setdefault(mid, {})
            if je_mission.get(oid) != (zustand, kennzeichen):
                je_mission[oid] = (zustand, kennzeichen)
                veraendert = True
        return veraendert

    def offen(self, mission_id):
        """Die offenen Ziele dieses Auftrags, in der Reihenfolge des Logs.

        ⚠ **Ohne Wortlaut wird geschwiegen.** Ein Ziel, dessen Meldung wir nicht
        gesehen haben, bekommt hier keine Zeile — dieselbe Linie wie überall:
        lieber nichts zeigen als etwas Falsches behaupten.
        """
        namen = []
        for oid, (zustand, kennzeichen) in self._stand.get(mission_id,
                                                           {}).items():
            if zustand != 'INPROGRESS' or ZIEL_SICHTBAR not in kennzeichen:
                continue
            wortlaut = self._titel.get(oid)
            if wortlaut and wortlaut not in namen:
                namen.append(wortlaut)
        return namen

    def vergessen(self, mission_id):
        """Der Auftrag ist vorbei — seine Ziele auch."""
        self._stand.pop(mission_id, None)


def _index_bauen():
    """Titel → Missionsschlüssel, aus der `global.ini` und dem Katalog.

    Es werden nur die Schlüssel aufgenommen, die der Katalog überhaupt kennt —
    die `global.ini` hat über 9.000 Einträge, davon sind 353 für uns relevant.
    """
    global _index, _muster_index
    _index, _muster_index = {}, []
    bekannt = set(missionen())
    if not bekannt:
        return
    for pfad in _ini_dateien():
        try:
            with open(pfad, encoding='utf-8', errors='ignore') as f:
                for zeile in f:
                    trenner = zeile.find('=')
                    if trenner < 1:
                        continue
                    schluessel = zeile[:trenner]
                    if schluessel not in bekannt:
                        continue
                    titel = sauber(zeile[trenner + 1:])
                    if not titel:
                        continue
                    if '~mission(' in titel:
                        # Platzhalter -> Muster. Der Rest wird woertlich
                        # genommen, damit „High-Risk Bounty: X" nicht auf
                        # „Low-Risk Bounty: X" passt.
                        roh = '^' + '.+'.join(
                            re.escape(t) for t in _PLATZHALTER.split(titel)) + '$'
                        try:
                            _muster_index.append((re.compile(roh), schluessel))
                        except re.error:
                            pass
                    else:
                        _index.setdefault(titel.lower(), schluessel)
        except OSError:
            continue


def schluessel_zu(titel):
    """Der Missionsschlüssel zu einem angezeigten Titel, oder None."""
    if _index is None:
        _index_bauen()
    rein = sauber(titel)
    treffer = _index.get(rein.lower())
    if treffer:
        return treffer
    for mst, schluessel in _muster_index:
        if mst.match(rein):
            return schluessel
    return None


def pruefen(titel, hat_bereits):
    """Was bringt dieser Auftrag — und was davon fehlt noch?

    `hat_bereits` ist eine Funktion `name -> bool`. Rückgabe ist `None`, wenn
    der Auftrag unbekannt ist oder keine Baupläne bringt; sonst
    `(gesamtzahl, [fehlende Namen])`.
    """
    schluessel = schluessel_zu(titel)
    if not schluessel:
        return None
    eintrag = missionen().get(schluessel) or {}
    namen = [n for n in (eintrag.get('bp') or []) if n]
    if not namen:
        return None
    fehlend = [n for n in namen if not hat_bereits(n)]
    return len(namen), fehlend
