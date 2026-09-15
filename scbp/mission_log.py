# SPDX-License-Identifier: GPL-3.0-only
#
# SC BP Watcher — Auftrags-Protokoll
# Copyright (C) 2026 Xharig
#
# This program is free software: you can redistribute it and/or modify it under
# the terms of the GNU General Public License version 3 as published by the
# Free Software Foundation.
#
# This program is distributed in the hope that it will be useful, but WITHOUT
# ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
# FOR A PARTICULAR PURPOSE.  See the GNU General Public License for more
# details.
#
# You should have received a copy of the GNU General Public License along with
# this program.  If not, see <https://www.gnu.org/licenses/>.
"""
Welche Auftraege wann gespielt wurden — das Protokoll vergangener Auftraege.

Beantwortet drei Fragen und sonst keine: **welcher Auftrag**, **wann**, **wie
oft**. Keine Belohnungen, keine Kategorien — das steht nicht im Log.

## ⚠⚠ Dieses Modul erkennt KEINE Auftraege

Das tut `contracts.py`, und zwar besser, als es hier je entstehen wuerde: Es
holt die Formulierungen („Auftrag angenommen") aus der `global.ini` des
Spielers statt sie einzutragen, geht auf den Missions-**Schluessel** statt auf
den Wortlaut (sonst gilt jedes Zwischenziel als Auftrag), putzt die eigenen
Bauplan-Marken aus dem Titel und kennt drei Enden statt einem.

Der erste Entwurf dieses Moduls hat all das danebengebaut und dieselben Fallen
einzeln neu entdeckt. **Zwei Auswertungen derselben Logzeilen laufen beim
naechsten Patch auseinander** — deshalb kommt hier jede Auftragserkennung aus
`contracts.py`.

Was dieses Modul beitraegt, ist genau das, was dort fehlt:

| | |
|---|---|
| **Wann** | `contracts.events_from_text()` liefert keinen Zeitpunkt — hier wird Zeile fuer Zeile gelesen, damit der Zeitstempel danebensteht |
| **Ueber Sitzungen hinweg** | Jedes Einloggen beginnt eine neue `Game.log`. Ein Auftrag, abends angenommen und morgens beendet, steht in zwei Dateien |
| **Abgeschlossen oder abgebrochen** | `contracts.py` kennt nur „beendet". Der Unterschied steht in `<EndMission> … CompletionType[Complete\\|Abandon]` |
| **Wie oft, und Suche** | Zaehlen und Filtern ueber den Namen |

## ⚠ Die Doppelmeldung

Das Spiel schickt dieselbe Annahme **zweimal in derselben Millisekunde**, nur
mit verschiedener Nummer in den eckigen Klammern:

    <2026-08-29T16:02:14.792Z> [41] Auftrag angenommen: Retake Platforms …
    <2026-08-29T16:02:14.792Z> [44] Auftrag angenommen: Retake Platforms …

An echten Sitzungen gemessen: Ohne Gegenmassnahme steht jeder Auftrag
doppelt im Protokoll. Entdoppelt wird ueber **(Zeitpunkt, Titel, Art)** — die
Nummer taugt dafuer nicht, und zwei echte Annahmen desselben Auftrags in
derselben Millisekunde gibt es nicht.
"""
import json
import os
import re

from . import contracts, fehler, pfade

FILE = 'auftragslog.json'
# ⚠ 2 seit dem 04.09.2026. Ein Protokoll im Format 1 enthaelt zwei Fehler, die
# sich nicht nachtraeglich glattziehen lassen — Auftraege, die ewig „laeuft"
# blieben, und Bauplaene, die dadurch am falschen Auftrag haengen. Beides
# entsteht beim Lesen, also wird beim Formatwechsel **komplett neu gelesen**
# statt repariert. Das kostet einmalig ein paar Sekunden beim Start und ist
# der einzige Weg zu sauberen Daten.
FORMAT = 2

# Der Zeitstempel am Zeilenanfang: <2026-08-29T16:02:14.792Z>
_TIME = re.compile(r'<(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d)')

# Abgeschlossen oder abgebrochen — nur diese Zeile sagt es.
_END_KIND = re.compile(r'<EndMission>.*?MissionId\[(?P<mid>[^\]]*)\]'
                       r'.*?CompletionType\[(?P<art>[^\]]*)\]')

COMPLETED = 'abgeschlossen'
ABORTED = 'abgebrochen'
RUNNING = 'laeuft'
# ⚠ Kein Ende im Log, aber sicher nicht mehr offen — siehe `_close_expired`.
# Bewusst NICHT als „abgebrochen" gefuehrt: Wir wissen nur, dass er nicht mehr
# laeuft, nicht warum. Eine Behauptung waere schlimmer als eine ehrliche Luecke.
EXPIRED = 'verfallen'
# ⚠⚠ **Neu am 06.09.2026 — vorher galt Scheitern als Erfolg.** Das Spiel kennt
# vier Ausgaenge, ausgewertet wurde nur einer davon:
#
#     Complete     316   abgeschlossen
#     Abandon      110   abgebrochen
#     Fail          57   fiel unter „abgeschlossen" — falsch
#     Deactivate     2   fiel unter „abgeschlossen" — falsch
#
# 57 gescheiterte Auftraege standen gruen im Protokoll. Wer nachsieht, wie oft
# ihm ein Auftrag misslungen ist, bekam die falsche Antwort.
FAILED = 'fehlgeschlagen'


def _state_for(kind):
    """Aus `CompletionType[…]` den Zustand — der Ausgang steht im Log.

    ⚠ `Deactivate` (2 von 485) heisst, dass das Spiel den Auftrag selbst
    zurueckgezogen hat. Weder Leistung noch Aufgabe des Spielers, deshalb
    `EXPIRED`: Die Spur endet, ueber das Warum wird nichts behauptet —
    dieselbe Zurueckhaltung wie bei `EXPIRED` selbst.

    ⚠ Ein unbekannter oder fehlender Ausgang gilt weiter als abgeschlossen.
    Das ist der Stand von vorher und deckt jedes Ende ab, das ohne
    `<EndMission>` nur als Mitteilung im Log steht.
    """
    kind = (kind or '').lower()
    if kind.startswith('abandon'):
        return ABORTED
    if kind.startswith('fail'):
        return FAILED
    if kind.startswith('deactivate'):
        return EXPIRED
    return COMPLETED


# Woran man erkennt, dass der Spieler wirklich im Spiel angekommen ist.
# ⚠ An 188 echten Protokollen gemessen (05.09.2026): In 187 kommt diese Zeile
# vor, und in KEINEM einzigen wurde ein Auftrag genannt, ohne dass sie davor
# stand. Sie ist damit die verlaessliche Grenze zwischen „Spiel gestartet" und
# „Spieler ist drin".
SPAWN_MARKER = 'OnClientSpawned'

# Wie lange eine Sitzung gelaufen sein muss, damit ihr SCHWEIGEN etwas beweist.
#
# ⚠⚠ **Warum es diese Zahl gibt (05.09.2026).** Wer sich ausloggt, ohne
# abzugeben oder abzubrechen, hinterlaesst kein Ende im Log; aufgeraeumt wurde
# so ein Auftrag nur, wenn eine spaetere Sitzung ihn nicht mehr nannte. Eine
# Sitzung ganz OHNE Auftrag galt dabei als aussagelos — zu Recht, denn ein
# abgebrochener Start nennt auch keinen.
#
# Gemeldet wurde genau der Fall dazwischen: eine vollstaendige Sitzung von
# 162 Minuten, in der kein einziger Auftrag vorkam, und trotzdem stand die
# Karteileiche vom Vortag weiter da.
#
# ⚠ **Der erste Anlauf war falsch und wurde durch Messung widerlegt.** „Spawn
# vorhanden, kein Auftrag" allein haette an 188 Protokollen **acht** Auftraege
# geschlossen, die kurz danach wieder auftauchten — kurze Fehlstarts nennen
# den Auftrag eben doch nicht immer. Mit der Mindestdauer durchgespielt:
#
#     ohne Grenze  95 geschlossen, 8 davon falsch
#     30 Minuten   83 geschlossen, 2 davon falsch
#     60 Minuten   81 geschlossen, 0 davon falsch
#     90 Minuten   80 geschlossen, 0 davon falsch
#
# Genommen sind 90 Minuten: Das kostet gegenueber 60 genau EINEN aufgeraeumten
# Auftrag und verdoppelt den Abstand zur Fehlergrenze. Eine ehrliche
# Karteileiche ist besser als ein faelschlich geschlossener Auftrag — dieselbe
# Abwaegung wie bei `EXPIRED` selbst.
SESSION_COUNTS_SEC = 90 * 60


def _time_of(line):
    m = _TIME.search(line)
    return m.group(1) if m else ''


# Wie lange nach dem Abgeben ein Bauplan noch zum Auftrag gezaehlt wird.
#
# ⚠ Die Belohnung faellt NACH dem Ende, nicht davor. Gemessen am 29.08.2026:
# Auftrag „Retake Platforms From Nine Tails" endete 17:42:00, der Bauplan
# „H4-PBF Ammo Carrier" kam 17:42:54 — 54 Sekunden spaeter. Ohne Nachlauf
# stuende er bei keinem Auftrag. Fuenf Minuten sind grosszuegig genug fuer eine
# lahme Serververbindung und kurz genug, dass er nicht beim naechsten Auftrag
# landet; laeuft ohnehin schon der naechste, gewinnt der (siehe `_assign_bp`).
BP_GRACE_SEC = 300


def _entry(title, when, source):
    return {'name': title, 'wann': when, 'zustand': RUNNING,
            'ziele_fertig': 0, 'ziele_gesamt': 0, 'bauplaene': [],
            'quelle': source}


def _seconds(stamp):
    """Ein Zeitstempel als Zahl — fuer den Abstand zwischen zwei Ereignissen."""
    try:
        import calendar
        import time as _t
        return calendar.timegm(_t.strptime(stamp[:19], '%Y-%m-%dT%H:%M:%S'))
    except Exception:
        return None


def _assign_bp(name, when, pending, done, reported=None):
    """Einen gefundenen Bauplan dem Auftrag zuschreiben, zu dem er gehoert.

    ⚠ **Laufender Auftrag zuerst, erst dann der gerade beendete.** Wer einen
    Auftrag abgibt und sofort den naechsten annimmt, bekommt die Belohnung des
    alten — waehrend der neue schon laeuft. Andersherum gepruefte Reihenfolge
    haette sie dem neuen zugeschrieben.

    ⚠⚠ **Ein Auftrag gibt hoechstens EINEN Bauplan her.** Das ist eine Regel
    des Spiels, keine Annahme. Wer sie nicht kennt, baut genau den Fehler, der
    hier lange drinsteckte: Ein Auftrag, der faelschlich als „laeuft" stehen
    blieb, sammelte jeden spaeter gefundenen Bauplan ein — gemessen am
    04.09.2026 hingen an einem Auftrag vom 23.06. **zwoelf** Stueck, an einem
    vom 07.08. sieben, darunter Teile, die es dort gar nicht gibt.

    Wer schon einen hat, scheidet deshalb aus. Bleibt niemand uebrig, wird der
    Bauplan **keinem** Auftrag zugeschrieben: Er kann aus der Herstellung
    stammen oder aus einem Auftrag, dessen Annahme in keinem noch vorhandenen
    Log steht. Lieber keine Zuordnung als eine erfundene.

    ⚠⚠ **`gemeldet` sind die Auftraege DIESER Sitzung.** Ein offener Auftrag
    aus einer frueheren Sitzung, den das Spiel hier nicht mehr nennt, laeuft
    nicht mehr — er darf nichts bekommen. Das Aufraeumen in
    `_close_expired()` allein genuegt dafuer nicht: Es kann erst
    greifen, wenn die Datei durch ist, waehrend der Bauplan mittendrin faellt.

    Gemessen am 04.09.2026: „Willkommen im System" endete um 07:21:55, eine
    Sekunde spaeter fiel „Clearcut Module" — zugeschrieben wurde es einem
    Auftrag vom **31.08.**, der nur deshalb noch offen schien.

    Verlassen kann man sich darauf, weil das Spiel beim Einloggen jeden
    laufenden Auftrag erneut meldet, also am ANFANG der Datei — lange vor
    jedem Bauplan-Fund darin.
    """
    for target in reversed(pending):
        if target.get('bauplaene'):
            continue            # hat seinen Bauplan schon — Spielregel
        if reported is not None and target['name'] not in reported:
            continue            # laeuft in dieser Sitzung gar nicht
        target.setdefault('bauplaene', []).append(name)
        return True
    now = _seconds(when)
    if now is None:
        return False
    for entry in reversed(done):
        if entry.get('bauplaene'):
            continue
        end = _seconds(entry.get('bis') or '')
        if end is not None and 0 <= now - end <= BP_GRACE_SEC:
            entry.setdefault('bauplaene', []).append(name)
            return True
    return False


def _read(path, pending, done, seen, ident, start_pat, end_pat,
           bp_pattern=None):
    """Ein Log lesen und die Buchfuehrung fortschreiben.

    `offen` und `fertig` werden ueber Dateigrenzen hinweg weitergereicht —
    ein Auftrag kann in einer spaeteren Sitzung enden als er begann.

    Gibt `(gemeldet, aussagekraeftig)` zurueck:

    - `gemeldet` sind die Titel, die diese Sitzung als angenommen gemeldet hat
      — **auch die Wiederaufnahmen**. `_close_expired()` braucht das.
    - `aussagekraeftig` sagt, ob man einer Sitzung OHNE jeden Auftrag glauben
      darf, dass wirklich keiner mehr offen war. Siehe `SESSION_COUNTS_SEC`.
    """
    source = os.path.basename(path)
    endings = {}          # mission_id -> 'Complete' | 'Abandon'
    objectives = {}          # mission_id -> {objective_id: zustand}
    reported = set()    # welche Auftraege diese Sitzung ueberhaupt nennt
    # Fuer die Frage, ob eine stumme Sitzung etwas beweist: War der Spieler
    # ueberhaupt im Spiel, und wie lange lief es?
    spawn = False
    first_time = last_time = None

    try:
        with open(path, encoding='utf-8', errors='replace') as f:
            for line in f:
                if not spawn and SPAWN_MARKER in line:
                    spawn = True
                _t = _TIME.search(line)
                if _t:
                    if first_time is None:
                        first_time = _t.group(1)
                    last_time = _t.group(1)
                # Die Art des Endes merken, bevor das Ereignis selbst kommt —
                # im Log steht EndMission vor der Mitteilung.
                a = _END_KIND.search(line)
                if a:
                    endings[a.group('mid')] = a.group('art')

                # ('zustand', mission_id, objective_id, zustand, kennzeichen)
                for obj_event in contracts.objective_events_from_text(line):
                    if obj_event and obj_event[0] == 'zustand':
                        objectives.setdefault(obj_event[1], {})[obj_event[2]] = obj_event[3]

                # ⭐ Welcher Bauplan bei welchem Auftrag herauskam. Erkannt wird
                # er mit demselben Muster wie im Bestand (`phrases.py`) — die
                # Formulierung steht in der `global.ini` des Spielers, nicht
                # hier. Die Zuordnung macht der Zeitpunkt: Ein Bauplan faellt
                # waehrend eines Auftrags oder kurz nach dem Abgeben.
                if bp_pattern is not None:
                    for hit in bp_pattern.finditer(line):
                        raw_bp = next((g for g in hit.groups() if g), '')
                        bp_name = contracts.clean(raw_bp)
                        if not bp_name:
                            continue
                        bp_when = _time_of(line)
                        if (bp_when, bp_name, 'bp') in seen:
                            continue    # dieselbe Doppelmeldung wie oben
                        seen.add((bp_when, bp_name, 'bp'))
                        _assign_bp(bp_name, bp_when, pending, done, reported)

                events = contracts.events_from_text(
                    line, start_pat, end_pat)
                if not events:
                    continue
                when = _time_of(line)

                for is_accept, raw, mission_id, objective_id in events:
                    # ⚠ IMMER durch `clean()`. Im Log steht der Titel mal als
                    # „Retake Platforms From Nine Tails <EM4>[BP!]</EM4>", mal
                    # mit „[SCBPW] … [/SCBPW]" — je nachdem, was der Watcher
                    # gerade ins Spiel eingetragen hat. Ungeputzt gilt derselbe
                    # Auftrag als zwei verschiedene: gemessen 3× und 2× statt 5×.
                    title = contracts.clean(raw)
                    key = (when, title, is_accept)
                    if key in seen:
                        continue        # Doppelmeldung, siehe Modulkopf
                    seen.add(key)

                    if is_accept is None:
                        # ⚠⚠ Spielwelt verlassen — hier NICHT raeumen.
                        #
                        # `contracts.py` raeumt an dieser Stelle auf, und das ist
                        # dort richtig: Das Overlay soll nach dem Ausloggen keine
                        # Auftraege mehr anzeigen, die nicht mehr anstehen.
                        #
                        # Ein Protokoll hat die umgekehrte Aufgabe. Ausloggen
                        # beendet keinen Auftrag — er laeuft im Spiel weiter und
                        # wird oft in der naechsten Sitzung abgeschlossen. Wer
                        # hier raeumt, verliert genau die Auftraege, die ueber
                        # zwei Abende gingen: Beim Testen an den echten
                        # Sicherungen blieben von sechs Auftraegen nur die
                        # uebrig, die in derselben Sitzung endeten.
                        continue

                    if is_accept:
                        # ⚠ Titel mit rohem Platzhalter gehoeren nicht ins
                        # Protokoll: `Ling Family - Rang: ~mission(ReputationRank)`
                        # setzt das Spiel erst beim Anzeigen ein, die Werte
                        # stehen nirgends im Log. Als eigener Eintrag waere das
                        # ein zweiter Auftrag, den es nie gab — daneben stand
                        # derselbe mit aufgeloestem Rang („NEULING").
                        if not title or '~mission(' in title:
                            continue
                        # ⚠ VOR der Wiederaufnahme-Pruefung merken: Gerade die
                        # Wiederaufnahme ist der Beweis, dass der Auftrag in
                        # dieser Sitzung noch lief.
                        reported.add(title)
                        # ⚠⚠ **Wiederaufnahme ist keine neue Annahme.** Beim
                        # Einloggen meldet das Spiel jeden laufenden Auftrag
                        # erneut als angenommen. Ohne diese Pruefung stand
                        # „Retake Platforms From Nine Tails" 29× im Protokoll,
                        # obwohl es fuenf Durchlaeufe waren — einmal je Sitzung,
                        # in der er offen war.
                        #
                        # Das ist auch der Grund, warum `contracts.py` beim
                        # Verlassen der Welt raeumt: Fuer die Live-Anzeige ist
                        # Raeumen die einfachere Loesung. Ein Protokoll darf
                        # nicht raeumen (sonst fehlen Auftraege ueber zwei
                        # Abende) und muss die Wiederaufnahme deshalb hier
                        # abfangen.
                        already_pending = any(
                            e['name'] == title for e in pending) or (
                                mission_id and mission_id in ident
                                and any(e['name'] == ident[mission_id]
                                        for e in pending))
                        if already_pending:
                            continue
                        pending.append(_entry(title, when, source))
                        if mission_id:
                            ident[mission_id] = title
                        continue

                    # ⚠⚠ Ein Ende — aber WELCHES? Die Zuordnung macht
                    # `which_ended()`, nicht dieses Modul. Sein erster
                    # Schritt ist der entscheidende: Steht eine ObjectiveId
                    # dabei, endet nur ein Zwischenziel und der Auftrag laeuft
                    # weiter. Ohne diesen Filter landete „Obere Plattform
                    # erreichen" achtmal als eigener Auftrag im Protokoll —
                    # es ist ein Ziel innerhalb von „Retake Platforms".
                    #
                    # Und wenn nichts zugeordnet werden kann, wird NICHTS
                    # eingetragen. Ein erfundener Auftrag ist schlimmer als ein
                    # fehlender.
                    hit = contracts.which_ended(
                        title, mission_id, objective_id,
                        [e['name'] for e in pending], ident)
                    if not hit:
                        continue
                    state = _state_for(endings.get(mission_id, ''))
                    # ⚠ Den AELTESTEN passenden schliessen, nicht den juengsten.
                    # Sonst bekommt ein Auftrag das Ende eines spaeteren
                    # Durchlaufs und im Protokoll steht ein Ende vor seinem
                    # Anfang („21:26 abgeschlossen → 17:42").
                    for entry in pending:
                        if entry['name'] == hit:
                            entry['zustand'] = state
                            entry['bis'] = when
                            pending.remove(entry)
                            done.append(entry)
                            break
    except OSError as exception:
        fehler.merken('mission_log.read', exception)
        return reported

    # Fortschritt nur, wo die Zuordnung eindeutig ist: Das Log verbindet Titel
    # und Missionskennung nirgends. Bei genau einem offenen Auftrag und genau
    # einer Kennung kann es nur diese sein — sonst bliebe es Raten, und eine
    # falsche Zahl ist schlechter als keine.
    if len(pending) == 1 and len(objectives) == 1:
        progress = list(objectives.values())[0]
        # Phasen-Ziele beschreiben den Abschnitt, nicht eine Aufgabe, die der
        # Spieler abhakt — sie gehoeren nicht in „3 von 5".
        real = {k: v for k, v in progress.items() if not str(k).startswith('phase_')}
        if real:
            pending[0]['ziele_gesamt'] = len(real)
            pending[0]['ziele_fertig'] = sum(
                1 for v in real.values() if str(v).upper().endswith('COMPLETED'))

    duration = 0
    a, b = _seconds(first_time or ''), _seconds(last_time or '')
    if a and b:
        duration = b - a
    return reported, (spawn and duration >= SESSION_COUNTS_SEC)


def from_files(paths):
    """Mehrere Logs als EINE Geschichte auswerten, neuester Auftrag zuerst."""
    # `kennung` merkt sich mission_id -> Titel. `which_ended()` greift
    # darauf zurueck, wenn der Titel beim Ende anders lautet als bei der
    # Annahme — laut Messung dort 62 von 362 Faellen.
    pending, done, seen, ident = [], [], set(), {}
    start_pat, end_pat = contracts.start_pattern(), contracts.end_pattern()
    # ⚠ Dasselbe Muster wie im Bauplan-Bestand — die Formulierung steht in der
    # `global.ini` des Spielers. Faellt es aus, laeuft das Protokoll weiter, nur
    # ohne die Bauplan-Zeilen: Ein Auftrags-Protokoll ohne Belohnungen ist
    # brauchbar, gar keines waere es nicht.
    try:
        from . import phrases
        bp_pattern = phrases.pattern()
    except Exception as exception:
        fehler.merken('mission_log.bp_pattern', exception)
        bp_pattern = None
    for path in paths:
        reported, counts = _read(path, pending, done, seen, ident,
                                  start_pat, end_pat, bp_pattern)
        _close_expired(pending, done, reported, _session_start(path),
                               silent_counts=counts)
    return sorted(done + pending, key=lambda e: e.get('wann') or '',
                  reverse=True)


def _reported_titles(log_path):
    """`(gemeldete Titel, zaehlt ihr Schweigen)` — ohne die volle Auswertung.

    ⚠ Wird gebraucht, um den **gespeicherten** Bestand nachzubewerten. Die
    volle Auswertung (`_read`) schreibt dabei in `offen`/`fertig` und
    verdoppelte Eintraege; hier geht es nur um die zwei Fragen, die
    `_close_expired()` stellt: Welche Auftraege nennt diese Sitzung,
    und darf ihr Schweigen etwas beweisen?
    """
    reported = set()
    spawn = False
    first = last = None
    start_pat, end_pat = contracts.start_pattern(), contracts.end_pattern()
    with open(log_path, encoding='utf-8', errors='replace') as f:
        for line in f:
            if not spawn and SPAWN_MARKER in line:
                spawn = True
            hit = _TIME.search(line)
            if hit:
                if first is None:
                    first = hit.group(1)
                last = hit.group(1)
            for pattern in (start_pat, end_pat):
                t = pattern.search(line)
                if t:
                    reported.add(contracts.clean(t.group(1)))
                    break
    duration = 0
    a, b = _seconds(first or ''), _seconds(last or '')
    if a and b:
        duration = b - a
    return reported, (spawn and duration >= SESSION_COUNTS_SEC)


def _close_expired(pending, done, reported, session,
                           silent_counts=False):
    """Auftraege beenden, die eine spaetere Sitzung nicht mehr kennt.

    ⚠⚠ **Das ist die Obergrenze, die dem Protokoll gefehlt hat.** Ausloggen
    beendet keinen Auftrag (siehe `_read`) — aber irgendwann ist er trotzdem
    vorbei, und ohne diese Regel stand er fuer immer auf „laeuft". Gemessen am
    04.09.2026: **43** solcher Karteileichen, die aelteste vom 23.06., und sie
    richteten Folgeschaden an — ein scheinbar laufender Auftrag sammelt jeden
    spaeter gefundenen Bauplan ein (siehe `_assign_bp`).

    Die Regel kommt aus dem Spiel selbst, nicht aus einer Zeitschaetzung:
    **Beim Einloggen meldet Star Citizen jeden noch laufenden Auftrag erneut
    als angenommen.** Wird ein Auftrag in einer spaeteren Sitzung also nicht
    mehr genannt, kann er dort nicht mehr offen gewesen sein. An 181 echten
    Sicherungen loeste das alle 43 Faelle auf, ohne einen einzigen Zweifelsfall.

    ⚠ **Eine stumme Sitzung beweist meistens nichts.** Wer sich einloggt und
    ohne Auftrag herumfliegt (oder wessen Log nach einem Absturz abbricht),
    meldet gar nichts — daraus zu schliessen, alle Auftraege seien vorbei,
    waere falsch.

    ⚠⚠ **Mit EINER Ausnahme, seit 05.09.2026: einer langen Sitzung.** Gemeldet
    wurde der Fall, der bis dahin durchs Raster fiel — nach dem Ausloggen ohne
    Abgabe blieb der letzte Auftrag fuer immer auf „laeuft", auch nachdem
    danach 162 Minuten lang gespielt worden war, ohne dass ein einziger
    Auftrag vorkam. Dazu: „er wurde nicht wieder gemeldet, kann er auch nicht
    da er weg ist."

    Wer 90 Minuten im Spiel ist und in dieser ganzen Zeit keinen Auftrag im
    Journal hat, hat keinen — anders als bei einem Fehlstart nach zwei
    Minuten. Wo die Grenze liegt und warum genau dort, steht bei
    `SESSION_COUNTS_SEC`; sie ist gemessen, nicht geschaetzt.

    ⚠ Der Zustand heisst `EXPIRED`, nicht `ABORTED`: Ob der Auftrag
    abgegeben oder aufgegeben wurde, steht in keinem vorhandenen Log.
    """
    if not pending:
        return
    if not reported:
        # Nur eine lange, vollstaendige Sitzung darf aus ihrem Schweigen
        # etwas folgern.
        if not silent_counts:
            return
        for entry in list(pending):
            entry['zustand'] = EXPIRED
            pending.remove(entry)
            done.append(entry)
        return
    for entry in list(pending):
        if entry['name'] in reported:
            continue
        # ⚠ Nur was VOR dieser Sitzung begann. Ein Auftrag, der in genau
        # dieser Sitzung angenommen wurde, steht ohnehin in `gemeldet` — und
        # ohne diese Grenze wuerde die Reihenfolge innerhalb einer Datei
        # zaehlen statt der Sitzungswechsel.
        if (entry.get('wann') or '') >= (session or ''):
            continue
        entry['zustand'] = EXPIRED
        pending.remove(entry)
        done.append(entry)


def from_folder(folder, running_log=None):
    """Alle Logs eines Ordners auswerten — `ordner` darf eine Liste sein.

    Windows und Linux sichern in getrennte Ordner; wer auf beiden spielt, will
    ein Protokoll, nicht zwei. `laufende` ist die gerade beschriebene
    `Game.log`, falls sie mitgelesen werden soll.
    """
    folders = [folder] if isinstance(folder, str) else list(folder or [])
    files = []
    for o in folders:
        if o and os.path.isdir(o):
            for name in os.listdir(o):
                if name.lower().endswith('.log'):
                    files.append(os.path.join(o, name))
    if running_log and os.path.isfile(running_log):
        files.append(running_log)

    return from_files(sorted(set(files), key=_session_start))


def _session_start(path):
    """Wann diese Sitzung gespielt wurde — aus dem ersten Zeitstempel im Log.

    ⚠⚠ **Nicht die Aenderungszeit der Datei nehmen.** Auf einer Sicherung ist
    das der Zeitpunkt des Kopierens: Alle zehn Logs auf der NAS trugen dieselbe
    Zeit (03.09.2026 11:22), weil sie in einem Rutsch gesichert wurden. Die
    Reihenfolge war damit zufaellig — und da ein Auftrag ueber mehrere Sitzungen
    laeuft, bekam er das Ende eines fremden Durchlaufs. Im Protokoll stand dann
    „21:26 abgeschlossen → 17:42": ein Ende vor seinem Anfang.
    ⚠ Auch der Dateiname taugt nicht: „30 Aug 26" sortiert alphabetisch falsch.
    """
    try:
        with open(path, encoding='utf-8', errors='replace') as f:
            for _ in range(200):        # der Stempel steht ganz oben
                line = f.readline()
                if not line:
                    break
                when = _time_of(line)
                if when:
                    return when
    except OSError:
        pass
    # Ohne Stempel ans Ende — lieber hinten anstellen als die Reihe verdrehen.
    return '9999'


def search(entries, text):
    """Nach Auftragsnamen filtern, ohne Ruecksicht auf Gross- und Kleinschreibung."""
    text = (text or '').strip().lower()
    if not text:
        return entries
    return [e for e in entries if text in (e.get('name') or '').lower()]


def summarize(entries):
    """Wie oft wurde welcher Auftrag gespielt? Name -> (gesamt, abgeschlossen)."""
    counter = {}
    for e in entries:
        total, done = counter.get(e['name'], (0, 0))
        counter[e['name']] = (total + 1,
                              done + (1 if e['zustand'] == COMPLETED
                                        else 0))
    return counter


# --------------------------------------------------------------- Fortschreiben
#
# ⚠⚠ **Das Protokoll lebt laenger als die Logs.** Star Citizen behaelt nur eine
# Handvoll `logbackups`, und auch die Sicherung auf die NAS haelt eine feste
# Zahl. Wer das Protokoll bei jedem Start allein aus den Logs baut, verliert
# jeden Auftrag, dessen Log inzwischen weggeraeumt wurde — genau die Rueckschau,
# um die es hier geht. Deshalb wird die Datei **fortgeschrieben**, so wie der
# Bauplan-Bestand auch.


def file_path():
    return pfade.app_datei(FILE)


def load():
    """Das gespeicherte Protokoll — oder eine leere Liste."""
    try:
        with open(file_path(), encoding='utf-8') as f:
            data = json.load(f)
        if data.get('format') == FORMAT:
            return _clean_titles(data.get('auftraege') or [])
    except Exception:
        pass
    return []


def _clean_titles(entries):
    """Marken aus Titeln holen, die vor dem Putz-Fix gespeichert wurden.

    ⚠ **Ohne das bliebe der Fix unsichtbar.** Die Titel werden beim Lesen
    geputzt (`_read`), nicht beim Anzeigen — was einmal mit Marke im Protokoll
    steht, behaelt sie. Und neu gelesen wird eine Logdatei nie wieder: Der
    Lesestand merkt sie sich (siehe `scan_backlog`). Ein Protokoll, das vor dem Fix
    entstand, zeigte die Marken also dauerhaft weiter.

    Laeuft bei jedem Laden, macht aber nur beim ersten Mal Arbeit — danach
    findet sie nichts mehr und gibt die Liste unveraendert zurueck.
    """
    cleaned, changed = [], False
    for e in entries:
        name = e.get('name') or ''
        plain = contracts.clean(name)
        if plain and plain != name:
            e = dict(e, name=plain)
            changed = True
        cleaned.append(e)
    if not changed:
        return entries
    # ⚠ Ueber `merge`, nicht roh zurueck: Zwei Eintraege koennen nach
    # dem Putzen denselben Schluessel tragen (gleicher Auftrag, einmal mit und
    # einmal ohne Marke). Sie gehoeren dann zusammen — und ein abgeschlossener
    # darf dabei nicht auf „laeuft" zurueckfallen.
    return merge(cleaned, [])


def save(entries):
    """Das Protokoll schreiben. Meldet einen Fehlschlag, statt ihn zu schlucken.

    ⚠ `pfade.json_sichern` legt die Vorgaengerfassung (`.bak.json`) an. Ein
    Protokoll laesst sich nicht neu aufbauen, sobald die Logs fort sind — hier
    waere ein leer geschriebener Stand endgueltig.
    """
    try:
        return pfade.json_sichern(file_path(), {'format': FORMAT,
                                           'auftraege': entries})
    except Exception as exception:
        fehler.merken('mission_log.save', exception)
        return False


def _key(e):
    """Was einen Auftragsdurchlauf eindeutig macht: Name plus Startzeitpunkt."""
    return ((e.get('name') or ''), (e.get('wann') or ''))


def merge(old, new):
    """Gespeichertes und frisch Gelesenes vereinen — ohne etwas zu verlieren.

    ⚠ **Der neue Stand gewinnt nur, wenn er mehr weiss.** Ein Auftrag, der
    gespeichert schon „abgeschlossen" ist, darf nicht wieder auf „laeuft"
    zurueckfallen, bloss weil in einem noch vorhandenen Log nur sein Anfang
    steht. Umgekehrt soll ein Ende, das erst jetzt im Log auftaucht, den alten
    Eintrag ergaenzen.
    """
    merged = {}
    for e in list(old) + list(new):
        s = _key(e)
        before = merged.get(s)
        if before is None:
            merged[s] = dict(e)
            continue
        # Ein beendeter Zustand sticht „laeuft" — egal aus welcher Quelle.
        if before.get('zustand') == RUNNING and e.get('zustand') != RUNNING:
            before.update({k: v for k, v in e.items() if v not in (None, '')})
        elif e.get('zustand') == RUNNING:
            # Nur fehlende Felder auffuellen, den Zustand nicht anfassen.
            for k, v in e.items():
                if k != 'zustand' and not before.get(k) and v:
                    before[k] = v
        else:
            before.update({k: v for k, v in e.items() if v not in (None, '')})
    return sorted(merged.values(), key=lambda e: e.get('wann') or '',
                  reverse=True)


def reassess(folder=None, running_log=None):
    """Alle Protokolle noch einmal auswerten — auch die schon gelesenen.

    Gibt `(gesamt, neu_dazu, berichtigt)` zurueck.

    ⚠⚠ **Warum es das braucht (06.09.2026).** Ein gespeicherter Auftrag wird
    nie wieder angefasst: `scan_backlog()` liest nur Dateien hinter dem Lesestand.
    Wird die Auswertung verbessert — an dem Tag lernte sie, `Fail` von
    `Complete` zu unterscheiden —, wirkt das ausschliesslich auf kuenftige
    Auftraege. Die 52 bereits falsch einsortierten blieben falsch, fuer immer.

    Ein Fix, der den Altbestand nicht erreicht, ist ein halber Fix. Deshalb
    gibt es diesen Weg: alles noch einmal lesen und die Zustaende berichtigen.

    ⚠ **Zusammenfuehren, nicht ersetzen.** Auftraege aus Protokollen, die das
    Spiel laengst geloescht hat, stehen nur noch hier — ein Neuaufbau wuerde
    sie verlieren. Genau dieser Unterschied hat am 05.09.2026 einem Melder
    seinen Bestand von 232 auf 3 gebracht.
    """
    old = load()
    before = {_key(e): e.get('zustand') for e in old}
    # ⚠⚠ **Nicht `from_folder`.** Das sieht nur direkt in den uebergebenen
    # Ordner — die aufgehobenen Sitzungen liegen aber eine Ebene tiefer in
    # `logbackups/`. Damit fand der erste Anlauf genau EINE Datei statt 199
    # und berichtigte nichts. `pfade.log_sicherungen` kennt den richtigen Ort
    # und nimmt seit v3.17.3 auch die Nachbarkanaele mit.
    files = list(pfade.log_sicherungen(folder) if folder else [])
    if running_log and os.path.isfile(running_log):
        files.append(running_log)
    new = from_files(sorted(set(files), key=_session_start)) if files else []
    if not new:
        return len(old), 0, 0
    merged = merge(old, new)
    added = corrected = 0
    for e in merged:
        s = _key(e)
        if s not in before:
            added += 1
        elif e.get('zustand') != before[s]:
            corrected += 1
    save(merged)
    return len(merged), added, corrected


def catch_up(folder=None, running_log=None):
    """Logs lesen, ins gespeicherte Protokoll einpflegen, sichern.

    Gibt `(gesamt, neu_dazugekommen)` zurueck.
    """
    old = load()
    new = from_folder(folder, running_log) if (folder or running_log) else []
    if not new:
        return len(old), 0
    known = {_key(e) for e in old}
    merged = merge(old, new)
    added = sum(1 for e in merged if _key(e) not in known)
    save(merged)
    return len(merged), added


def scan_backlog():
    """Beim Start: die aufgehobenen Logs des Spielers durchsehen.

    Genau wie beim Bauplan-Bestand — wer den Watcher zum ersten Mal startet,
    findet sein Protokoll **gefuellt** vor statt leer, denn seine `logbackups/`
    reichen ja Wochen zurueck.

    ⚠ **Nur einmal je Logdatei.** Die Sicherungen sind zusammen leicht ein
    halbes Gigabyte; sie bei jedem Start komplett neu zu lesen, wuerde den Start
    spuerbar bremsen — und seit die Sicherung auf der NAS 100 statt 10 Dateien
    aufhebt, waere es noch mehr. Gemerkt wird Name und Groesse: Waechst eine
    Datei (die laufende `Game.log` tut das staendig), wird sie erneut gelesen.
    Dubletten entstehen dabei nicht, dafuer sorgt `merge()`.
    """
    try:
        backups = list(pfade.log_sicherungen() or [])
    except Exception as exception:
        fehler.merken('mission_log.scan_backlog', exception)
        return 0, 0

    running_log = None
    game = pfade.spiel_ordner()
    if game:
        candidate = os.path.join(game, 'Game.log')
        if os.path.isfile(candidate):
            running_log = candidate

    read_marks = _load_read_marks()
    pending_files = []
    for log_path in backups + ([running_log] if running_log else []):
        try:
            mark = '%d' % os.path.getsize(log_path)
        except OSError:
            continue
        if read_marks.get(os.path.basename(log_path)) != mark:
            pending_files.append((log_path, mark))

    # ⚠⚠⚠ **Hier stand einmal eine Begrenzung auf die neuesten 20 Protokolle
    # — und sie war falsch.** Gemessen am 06.09.2026:
    #
    #     erster Lauf (20 Protokolle):    423 ms
    #     zweiter Lauf (die übrigen 185): 7226 ms
    #
    # Die Arbeit war nicht weg, nur verschoben — und beim zweiten Start
    # bekäme der Spieler sie ungebremst ab, ohne zu wissen warum. Der Autor
    # brachte es auf den Punkt: *„einmal beim Start, sonst die letzten 3?"*
    #
    # **Genau so läuft es, und zwar schon immer:** Der Lesestand oben sorgt
    # dafür, dass nach dem ersten Mal nur noch die gewachsenen Dateien
    # drankommen — im Alltag zwei bis drei. Die einmaligen neun Sekunden beim
    # allerersten Start sind der Preis für ein Protokoll, das rückwirkend
    # gefüllt ist; sie laufen im Hintergrund und treffen jeden Spieler genau
    # einmal.
    #
    # ⚠ Wer hier wieder begrenzen will, muss zuerst den **zweiten** Start
    # messen, nicht nur den ersten.

    # ⚠⚠ **ALLE Protokolle, nicht nur die hier offenen.** Der erste Anlauf gab
    # `offen_dateien` weiter — und auf einem Rechner, dessen Auftrags-Protokoll
    # schon eingelesen war, ist die Liste leer. Die Spielzeit stand dadurch auf
    # „0 min", obwohl 188 Protokolle dalagen. `spielzeit` hat einen eigenen
    # Lesestand und ueberspringt selbst, was es kennt.
    try:
        from . import playtime as _sz
        _sz.catch_up(backups + ([running_log] if running_log else []))
    except Exception as exception:
        fehler.merken('mission_log.playtime', exception)

    old = load()

    # ⚠⚠ **Die Nachbewertung läuft AUCH, wenn nichts Neues da ist.** Genau
    # das war der Fehler im ersten Anlauf: Sie stand hinter dem frühen
    # Rücksprung — und wer alle Protokolle längst gelesen hat (also jeder im
    # Alltag), kam nie dorthin. Gemessen: 3 Karteileichen vorher, 3 nachher.
    #
    # Herangezogen werden die **jüngsten** Protokolle, nicht die neuen: Was
    # noch offen ist, entscheidet die letzte Sitzung, nicht die zuletzt
    # gelesene Datei. Drei reichen und kosten fast nichts; alle 195 zu lesen
    # wäre bei jedem Start eine Sekunde für nichts.
    _candidates = sorted(backups + ([running_log] if running_log else []),
                         key=_session_start)[-3:]
    _pending_now = [e for e in old if e.get('zustand') == RUNNING]
    if _pending_now:
        _done = []
        for _path in _candidates:
            try:
                _reported, _counts = _reported_titles(_path)
            except Exception as exception:
                fehler.merken('mission_log.reassess', exception)
                continue
            _close_expired(_pending_now, _done, _reported,
                                   _session_start(_path), silent_counts=_counts)
        if _done:
            save(old)

    if not pending_files:
        return len(old), 0
    known = {_key(e) for e in old}
    # ⚠ Chronologisch, sonst bekommt ein Auftrag das Ende eines fremden
    # Durchlaufs — siehe `_session_start`.
    fresh = sorted((p for p, _m in pending_files), key=_session_start)
    new = from_files(fresh)
    merged = merge(old, new)

    added = sum(1 for e in merged if _key(e) not in known)
    if save(merged):
        for log_path, mark in pending_files:
            read_marks[os.path.basename(log_path)] = mark
        _save_read_marks(read_marks)
    return len(merged), added


def _load_read_marks():
    """Welche Logs schon gelesen wurden — leer bei veraltetem Format.

    ⚠⚠ **Der Lesestand muss mit dem Format mitziehen.** Sonst passiert beim
    Formatwechsel das Schlimmste von beidem: `laden()` verwirft das alte
    Protokoll, der Lesestand haelt aber alle 181 Logs fuer erledigt — und der
    Nutzer steht vor einem **leeren** Protokoll ohne jede Fehlermeldung.
    """
    try:
        with open(file_path(), encoding='utf-8') as f:
            data = json.load(f)
        if data.get('format') != FORMAT:
            return {}
        return data.get('gelesen') or {}
    except Exception:
        return {}


def _save_read_marks(read_marks):
    """Den Lesestand neben das Protokoll schreiben — in dieselbe Datei."""
    try:
        with open(file_path(), encoding='utf-8') as f:
            data = json.load(f)
        data['gelesen'] = read_marks
        pfade.json_sichern(file_path(), data)
    except Exception as exception:
        fehler.merken('mission_log.read_marks', exception)


# ------------------------------------------------------------------- Ausgeben


def as_csv(entries=None):
    """Das Protokoll als Tabelle — oeffnet sich in jedem Tabellenprogramm.

    Dieselbe Bauform wie beim Handelslager: Semikolon als Trenner, damit
    deutsche Excel-Fassungen die Spalten von allein trennen.
    """
    entries = load() if entries is None else entries
    lines = ['Auftrag;Angenommen;Beendet;Zustand;Ziele erledigt;Ziele gesamt']
    for e in entries:
        lines.append(';'.join((
            (e.get('name') or '').replace(';', ','),
            (e.get('wann') or '').replace('T', ' '),
            (e.get('bis') or '').replace('T', ' '),
            e.get('zustand') or '',
            str(e.get('ziele_fertig') or ''),
            str(e.get('ziele_gesamt') or ''))))
    return '\n'.join(lines) + '\n'


def as_json(entries=None):
    """Das Protokoll als JSON-Text — fuer die Sicherung neben den anderen Listen."""
    entries = load() if entries is None else entries
    return json.dumps({'format': FORMAT, 'auftraege': entries},
                      ensure_ascii=False, indent=2) + '\n'
