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
Spielstatistik: was in jeder Sitzung passiert ist — dauerhaft festgehalten.

## ⚠⚠ Warum eine eigene Datei (`statistik.json`)

Star Citizen räumt seine Protokolle weg, und Spieler löschen sie auch selbst.
Wer die Statistik bei jedem Öffnen aus den vorhandenen Logs rechnet, hat nach
jedem Aufräumen eine kleinere Vergangenheit. Deshalb wird jedes Protokoll
**einmal** ausgewertet und das Ergebnis je Sitzung festgehalten — was aus den
Logs verschwindet, bleibt hier stehen. Die Datei liegt im Datenordner neben
`spielzeit.json`, steckt damit in der Sicherung (`backup.py` nimmt alles, was
nicht ausdrücklich nachladbar ist) und zieht beim Rechnerwechsel mit um.

## Was gezählt wird — nur, was an echten Logs belegt ist

Gemessen am 27.09.2026 an 181 Protokollen (528 MB). Gezählt wird nur, was
**in jeder Spielsprache gleich** im Log steht:

| Zahl | Log-Zeile |
|---|---|
| Aufträge | `<EndMission> … Player[<Name>] … CompletionType[Complete\\|Fail]` |
| Quantensprünge | `<Quantum Drive Arrived - Arrived at Final Destination>` |
| Sitzungsdauer | wie `playtime.span_from_log` — Anfang bis letzte Zeile |

⚠ **Aufträge über `EndMission`, nicht über `MissionEnded`.** Nur `EndMission`
nennt den Spieler — `MissionEnded` kommt auch für Aufträge der Party. Und
über die `MissionId` entdoppeln: Das Spiel meldet manches zweimal.

⚠ `Abandon` (abgebrochen) und `Deactivate` werden **nicht** gezählt — eine
dritte Zahl „abgebrochen" wäre ohne Erklärung nicht zu verstehen.

⛔ **Tode, Verletzungen und Zonen werden nicht gezählt** (entschieden
27.09.2026, noch einmal nach dem Vergleich mit dem SC Deutsch Launcher). Ein
Tod steht in keinem eindeutigen Ereignis im Log: `[ActorState] Dead` kommt nur,
wenn das eigene Schiff zerbricht, und `CSCActorCorpseUtils` auch beim Plündern
fremder Leichen. Verletzungen und Rechtsgebiete stehen nur als Einblendungstext
in der Spielsprache. Das bräche bei englischem Client und bei jeder
Textänderung still — lieber keine Zahl als eine falsche. Die Messung steht in
der Vorhaben-Notiz, falls das Spiel später ein Ereignis dafür schreibt.

## Seit Format 2 (v3.58.0-rc2): die Unterseiten

| Zahl | Log-Zeile |
|---|---|
| Schiffe | `<Vehicle Control Flow> … Local client node … control token for '<Schiff>_<Nummer>'` |
| Verlorene Schiffe | `<[ActorState] Dead> … Actor '<Name>' … ejected from zone '<Schiff>' … destroyed vehicle` |
| Waffen | `<AttachmentReceived> Player[<Name>] Attachment[<Kennung>, <Klasse>, <Nummer>] … Port[wep_stocked_2\\|wep_sidearm\\|weapon_attach_hand_right]` |
| Zielwahlen | `<Player Selected Quantum Target - Local> … selected point <Ort>` |
| Abstürze | `crash handler taking over` · `-- GPU CRASH` |
| Verbindungsabbrüche | `<Channel Disconnected> … reason="…"` |

⚠ **Waffen je Stück, nicht je Meldung.** Das Spiel meldet die Ausrüstung bei
jedem Einloggen und Umziehen neu — gezählt wird jede Waffe (Nummer) einmal je
Sitzung. Sonst gewänne, wer am häufigsten den Server wechselt.

⚠ **Keine Reisezeit im Quantum.** Zwischen Zielwahl und Ankunft liegt auch das
Warten, bis man springt; eine Zahl daraus wäre keine Reisezeit. Gezählt werden
Zielwahlen und Ankünfte.

⚠ Beim Wechsel von Format 1 auf 2 wird jedes noch vorhandene Log **einmal neu
gelesen** (der Lesestand wird verworfen). Sitzungen, deren Log schon weg ist,
behalten ihre alten Zahlen — dort fehlen nur die neuen Felder.

## Die Wärmekarte

Kommt aus `spielzeit.json`, nicht aus dieser Datei: Die hält Sitzungen seit
dem 05.09.2026 fest, auch solche, deren Log längst weg ist. Eine eigene
Zeitrechnung hier hätte weniger Vergangenheit als die Spielzeit oben in der
Kopfzeile — zwei Zahlen für dieselbe Sache.
"""
import json
import os
import re
import time

from . import errors, paths

FILE = 'statistik.json'
FORMAT = 2

# Automatisch auswerten (beim Start und beim Öffnen der Seiten). Ab Werk an.
AUTO_SETTING = 'statistik_auto'

_TIMESTAMP = re.compile(r'<(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d)')
_END_MISSION = re.compile(
    r'<EndMission>.*?MissionId\[([^\]]+)\].*?Player\[([^\]]*)\]'
    r'.*?CompletionType\[([A-Za-z]+)\]')
_QUANTUM = '<Quantum Drive Arrived - Arrived at Final Destination>'
_QUIT = '<SystemQuit>'
_CONTROL = re.compile(r"<Vehicle Control Flow>.*?Local client node.*?"
                      r"control token for '([^']+)'")
_DEAD = re.compile(r"<\[ActorState\] Dead>.*?Actor '([^']*)'.*?"
                   r"ejected from zone '([^']+)'.*?destroyed vehicle")
_ATTACHMENT = re.compile(r'<AttachmentReceived> Player\[([^\]]*)\] '
                         r'Attachment\[[^,\]]*, ([^,\]]+), (\d+)\].*?'
                         r'Port\[([^\]]*)\]')
_TARGET = re.compile(r'<Player Selected Quantum Target - Local>.*?'
                     r'selected point (\S+)')
_DISCONNECT = re.compile(r'<Channel Disconnected>.*?reason="([^"]*)"')
_CRASH = ('crash handler taking over', '-- GPU CRASH')
# Nummer am Ende einer Kennung: `AEGS_Sabre_465232524500` -> `AEGS_Sabre`.
_SERIAL = re.compile(r'_\d{6,}$')

# Welche Halterung wofür steht. ⚠ Gemessen: `wep_stocked_2` und `_3` sind die
# beiden Plätze auf dem Rücken, die übrigen `wep_stocked_*` kamen nicht vor.
WEAPON_PORTS = {'wep_sidearm': 'seite',
                'weapon_attach_hand_right': 'hand'}
WEAPON_PORT_PREFIX = ('wep_stocked_', 'ruecken')

DONE = 'Complete'
FAILED = 'Fail'
ABANDONED = 'Abandon'


def path():
    return paths.app_file(FILE)


def auto_enabled():
    """Automatisch auswerten? Ab Werk ja."""
    return paths.setting_bool(AUTO_SETTING, True)


def load():
    """`{'format':…, 'sitzungen': {von: {…}}, 'gelesen': {name: groesse}}`.

    ⚠ Ein älteres Format behält seine Sitzungen, verliert aber den Lesestand:
    So wird jedes noch vorhandene Log einmal mit den neuen Zählungen gelesen,
    und was schon weg ist, bleibt mit seinen alten Zahlen stehen."""
    try:
        with open(path(), encoding='utf-8') as f:
            data = json.load(f)
        if isinstance(data, dict) and isinstance(data.get('sitzungen'), dict):
            if data.get('format') != FORMAT:
                data['gelesen'] = {}
            data.setdefault('gelesen', {})
            return data
    except (OSError, ValueError):
        pass
    except Exception as exception:
        errors.record('play_stats.load', exception)
    return {'format': FORMAT, 'sitzungen': {}, 'gelesen': {}}


def save(data):
    """Schreiben, erst daneben, dann umbenennen — ein Absturz mittendrin darf
    die aufgezeichnete Vergangenheit nicht halbieren."""
    try:
        data['format'] = FORMAT
        target = path()
        folder = os.path.dirname(target)
        if folder and not os.path.isdir(folder):
            os.makedirs(folder)
        pending = target + '.neu'
        with open(pending, 'w', encoding='utf-8') as f:
            json.dump(data, f, ensure_ascii=False)
        os.replace(pending, target)
        return True
    except Exception as exception:
        errors.record('play_stats.save', exception)
        return False


def _seconds(stamp):
    import calendar
    try:
        return calendar.timegm(time.strptime(stamp[:19], '%Y-%m-%dT%H:%M:%S'))
    except Exception:
        return None


def read_log(log_path, account=None, spawn_mark=None):
    """Eine Protokolldatei auswerten -> Sitzungseintrag oder None.

    None, wenn der Spieler nie im Spiel ankam (wie in `playtime`) oder die
    Datei unlesbar ist. `account` ist der Spieler dieser Log; Aufträge zählen
    nur, wenn `Player[…]` er ist. Ohne bekannten Account zählt jeder."""
    if spawn_mark is None:
        from .mission_log import SPAWN_MARKER
        spawn_mark = SPAWN_MARKER
    first = last = None
    arrived = False
    clean_end = False
    crashed = False
    missions = {}
    jumps = set()
    ships = set()
    lost = []
    weapons = {}                  # Platz -> {Klasse: {Nummern}}
    targets = {}
    selections = 0
    disconnects = {}

    def own(player):
        return not account or player.lower() == account.lower()

    try:
        with open(log_path, encoding='utf-8', errors='replace') as f:
            for line in f:
                match = _TIMESTAMP.search(line)
                stamp = match.group(1) if match else None
                if stamp:
                    if first is None:
                        first = stamp
                    last = stamp
                if not arrived and spawn_mark in line:
                    arrived = True
                if _QUIT in line:
                    clean_end = True
                elif _QUANTUM in line and stamp:
                    jumps.add(stamp)
                elif '<EndMission>' in line:
                    found = _END_MISSION.search(line)
                    if found:
                        mission_id, player, kind = found.groups()
                        if own(player):
                            missions[mission_id] = kind
                elif '<AttachmentReceived>' in line:
                    found = _ATTACHMENT.search(line)
                    # ⚠ `Default` ist ein leerer Platzhalter des Spiels,
                    # keine Waffe (85 Mal auf dem Rücken gemessen).
                    if found and own(found.group(1))                             and found.group(2) != 'Default':
                        slot = _weapon_slot(found.group(4))
                        if slot:
                            weapons.setdefault(slot, {}).setdefault(
                                found.group(2), set()).add(found.group(3))
                elif '<Vehicle Control Flow>' in line:
                    found = _CONTROL.search(line)
                    if found:
                        ships.add(_SERIAL.sub('', found.group(1)))
                elif '<[ActorState] Dead>' in line:
                    found = _DEAD.search(line)
                    if found and own(found.group(1)):
                        lost.append(_SERIAL.sub('', found.group(2)))
                elif '<Player Selected Quantum Target - Local>' in line:
                    found = _TARGET.search(line)
                    if found:
                        selections += 1
                        place = place_key(found.group(1))
                        targets[place] = targets.get(place, 0) + 1
                elif '<Channel Disconnected>' in line:
                    found = _DISCONNECT.search(line)
                    if found:
                        reason = found.group(1).strip()
                        disconnects[reason] = disconnects.get(reason, 0) + 1
                elif not crashed and any(mark in line for mark in _CRASH):
                    crashed = True
    except Exception:
        return None
    if not arrived:
        return None
    start, end = _seconds(first or ''), _seconds(last or '')
    if not start or not end or end < start:
        return None
    kinds = list(missions.values())
    return {'von': start, 'bis': end, 'sauber': clean_end,
            'account': account or '',
            'auftraege': kinds.count(DONE),
            'fehlgeschlagen': kinds.count(FAILED),
            'abgebrochen': kinds.count(ABANDONED),
            'spruenge': len(jumps),
            'schiffe': sorted(ships),
            'verloren': lost,
            'waffen': {slot: {cls: len(ids) for cls, ids in found.items()}
                       for slot, found in weapons.items()},
            'zielwahlen': selections,
            'ziele': targets,
            'absturz': crashed,
            'abbrueche': disconnects}


def _weapon_slot(port):
    """`wep_sidearm` -> 'seite' usw.; None für alles, was keine Waffe trägt."""
    if port in WEAPON_PORTS:
        return WEAPON_PORTS[port]
    prefix, slot = WEAPON_PORT_PREFIX
    return slot if port.startswith(prefix) else None


def place_key(raw):
    """Ein Quantenziel ohne Beiwerk: `LOC_` vorn und die Nummer hinten weg.

    ⚠ Die Orte stehen nur als Schlüssel im Log (`rs_ext_pyro6_leo`), und für
    die meisten gibt es keinen Anzeigenamen. Sie bleiben, wie das Spiel sie
    schreibt — nur die Seriennummer eines Missions-Leuchtfeuers fällt weg,
    sonst wäre jedes Leuchtfeuer ein eigenes Ziel."""
    raw = raw[4:] if raw.startswith('LOC_') else raw
    # Alle Leuchtfeuer von Aufträgen sind für die Statistik ein Ziel — ihre
    # Namen (`MISSION_QT_Quantum_Beacon_TSG_…`) sagen nichts über den Ort.
    if raw.startswith(MISSION_BEACON):
        return MISSION_BEACON
    return _SERIAL.sub('', raw)


# Ziele, die kein Ort sind: der eigene Wegpunkt, ein Party-Mitglied, das
# Leuchtfeuer eines Auftrags. Gemessen: zusammen jede fünfte Zielwahl.
MISSION_BEACON = 'MISSION_QT'
PLACE_LABELS = {'NavPoint_Dynamic': 's_sq_p_wegpunkt',
                'PartyMemberMarker': 's_sq_p_party',
                MISSION_BEACON: 's_sq_p_auftrag'}


def catch_up(files):
    """Protokolle einlesen, `statistik.json` fortschreiben. Gibt die Zahl der
    neu erfassten Sitzungen zurück.

    ⚠ **Eigener Lesestand (Name + Größe)**, wie in `playtime`: Ein halbes
    Gigabyte Logs wird so nur beim allerersten Mal gelesen, danach nur, was
    gewachsen ist — im Alltag die laufende `Game.log`.

    ⚠ **Schlüssel ist der Sitzungsanfang, nicht der Dateiname.** Die laufende
    `Game.log` wird beim nächsten Spielstart nach `logbackups/` umbenannt und
    wäre sonst ein zweites Mal „neu"."""
    from . import logsource
    data = load()
    sessions = data['sitzungen']
    read = data['gelesen']
    added = 0
    changed = False
    for log_path in (files or []):
        try:
            size = os.path.getsize(log_path)
        except OSError:
            continue
        name = os.path.basename(log_path)
        if read.get(name) == size:
            continue
        read[name] = size
        changed = True
        try:
            account = logsource.account_of_file(log_path)
        except Exception:
            account = None
        entry = read_log(log_path, account)
        if not entry:
            continue
        key = str(entry['von'])
        if key not in sessions:
            added += 1
        sessions[key] = entry
    LAST_RUN[0] = time.time()
    if changed:
        data['stand'] = LAST_RUN[0]
        save(data)
    return added


# Wann zuletzt nachgesehen wurde — auch wenn es nichts Neues gab. Die Datei
# wird dafür nicht angefasst; sie merkt sich nur den letzten Lauf mit Fund.
LAST_RUN = [None]


def startup_catch_up(files):
    """Beim Start: auswerten, wenn „Automatisch auswerten" an ist.

    Gibt die Zahl neuer Sitzungen zurück, None bei abgeschalteter Automatik.
    ⚠ Die Weiche steht hier und nicht beim Aufrufer, damit sie sich ohne den
    ganzen Start prüfen lässt (Prüfung 270)."""
    if not auto_enabled():
        return None
    return catch_up(files)


def log_files():
    """Alle Protokolle der Installation: die Sicherungen und die laufende."""
    files = list(paths.log_backups() or [])
    game = paths.game_folder()
    if game:
        running = os.path.join(game, 'Game.log')
        if os.path.isfile(running):
            files.append(running)
    return files


def rescan(files=None):
    """„Alles neu auswerten": jedes vorhandene Log noch einmal lesen.

    ⚠ Nur der **Lesestand** wird verworfen, nicht die Sitzungen. Was aus den
    Logs längst verschwunden ist, bliebe sonst nach einem Klick für immer weg
    — genau das, wofür es diese Datei gibt."""
    data = load()
    data['gelesen'] = {}
    save(data)
    return catch_up(log_files() if files is None else files)


def state():
    """Für die Seite „Auswertung": wann, wie viel, wie groß."""
    data = load()
    try:
        size = os.path.getsize(path())
    except OSError:
        size = 0
    return {'stand': LAST_RUN[0] or data.get('stand'),
            'sitzungen': len(data['sitzungen']),
            'dateien': len(data['gelesen']),
            'groesse': size}


def _heatmap(spans):
    """7 × 24 Sekunden in Ortszeit, Montag = 0.

    ⚠ Über Stundengrenzen wird **aufgeteilt**: Eine Sitzung von 23:30 bis 00:30
    zählt je eine halbe Stunde in beide Stunden und in beide Tage — nicht ganz
    in die Startstunde."""
    grid = [[0] * 24 for _ in range(7)]
    for start, end in spans:
        cursor = start
        while cursor < end:
            local = time.localtime(cursor)
            next_hour = cursor + (3600 - local.tm_min * 60 - local.tm_sec)
            piece = min(end, next_hour) - cursor
            grid[local.tm_wday][local.tm_hour] += piece
            cursor += piece
    return grid


def summary(own=None):
    """Alles, was die Seite zeigt — ein Wörterbuch aus fertigen Zahlen.

    `own`: der eigene Account (`logsource.own_account()`). Sitzungen eines
    anderen Accounts zählen nicht mit, wie bei den Bauplänen seit v3.57.0.
    Sitzungen aus `spielzeit.json` ohne Auswertung hier (Log schon weg) zählen
    für Zeit und Wärmekarte — ihr Account ist unbekannt, und unbekannt zählt."""
    from . import logsource, playtime
    stats = load()['sitzungen']

    def counts(account):
        return logsource.counts_for(own, account)

    spans = []
    for entry in playtime.load().get('sitzungen', []):
        start, end = entry.get('von'), entry.get('bis')
        if not start or not end:
            continue
        mine = stats.get(str(start))
        if mine is not None and not counts(mine.get('account')):
            continue
        spans.append((start, end))
    running = playtime._running_span()
    if running:
        spans.append(running)
    merged = playtime._merge_spans(spans)
    total = sum(end - start for start, end in merged)
    counted = [e for e in stats.values() if counts(e.get('account'))]
    sessions = len(merged)
    longest = max((end - start for start, end in merged), default=0)
    return {
        'spielzeit': total,
        'sitzungen': sessions,
        'schnitt': (total // sessions) if sessions else 0,
        'laengste': longest,
        'auftraege': sum(e.get('auftraege', 0) for e in counted),
        'fehlgeschlagen': sum(e.get('fehlgeschlagen', 0) for e in counted),
        'spruenge': sum(e.get('spruenge', 0) for e in counted),
        # Wie viele Sitzungen hier ausgewertet sind — Grundlage für „je
        # Sitzung". Nicht `sitzungen`: Die zählt auch alte Zeiten aus
        # `spielzeit.json`, deren Log zum Auszählen schon weg war.
        'erfasst': len(counted),
        'seit': min((s for s, _e in merged), default=None),
        'waermekarte': _heatmap(merged),
    }


def _counted(own):
    """Die ausgewerteten Sitzungen, die zum eigenen Account gehören."""
    from . import logsource
    return [e for e in load()['sitzungen'].values()
            if logsource.counts_for(own, e.get('account'))]


def _top(counter, limit=5):
    """Die größten Einträge als Liste `(Name, Zahl)`, größte zuerst.

    ⚠ Bei Gleichstand nach Namen, damit die Reihenfolge nicht bei jedem
    Öffnen springt."""
    return sorted(counter.items(), key=lambda kv: (-kv[1], kv[0]))[:limit]


def ships(own=None):
    """Schiffe & Ausrüstung: womit geflogen, was verloren, was getragen."""
    sessions = _counted(own)
    used, lost = {}, {}
    weapons = {'ruecken': {}, 'seite': {}, 'hand': {}}
    flown = 0
    for entry in sessions:
        if entry.get('schiffe'):
            flown += 1
        for ship in entry.get('schiffe') or []:
            used[ship] = used.get(ship, 0) + 1
        for ship in entry.get('verloren') or []:
            lost[ship] = lost.get(ship, 0) + 1
        for slot, found in (entry.get('waffen') or {}).items():
            bucket = weapons.setdefault(slot, {})
            for cls, count in found.items():
                bucket[cls] = bucket.get(cls, 0) + count
    top = _top(used, 1)
    return {'schiffe': len(used),
            'meist': top[0] if top else None,
            'sitzungen_mit_schiff': flown,
            'genutzt': _top(used),
            'verloren': _top(lost),
            'verloren_gesamt': sum(lost.values()),
            'waffen': {slot: (_top(found), sum(found.values()))
                       for slot, found in weapons.items()}}


def missions(own=None, recent=8):
    """Aufträge: Ausgänge aus der Statistik, Namen aus dem Auftragsprotokoll.

    ⚠ Die Zahlen oben kommen aus `<EndMission>` (nur eigene, je Auftrag
    einmal) — dieselben wie auf der Übersicht. Namen stehen dort nicht; die
    kommen aus dem Auftragsprotokoll (`mission_log`), das nur Aufträge kennt,
    deren Titel im Log auftaucht. Deshalb sind die Listen keine Summen."""
    from . import mission_log
    sessions = _counted(own)
    entries = [e for e in mission_log.load()
               if e.get('zustand') != mission_log.RUNNING]
    names = {}
    for e in entries:
        name = e.get('name') or ''
        if name:
            names[name] = names.get(name, 0) + 1
    latest = sorted(entries, key=lambda e: e.get('bis') or e.get('wann') or '',
                    reverse=True)[:recent]
    return {'abgeschlossen': sum(e.get('auftraege', 0) for e in sessions),
            'fehlgeschlagen': sum(e.get('fehlgeschlagen', 0) for e in sessions),
            'abgebrochen': sum(e.get('abgebrochen', 0) for e in sessions),
            'haeufigste': _top(names),
            'letzte': [(e.get('name') or '', e.get('bis') or e.get('wann') or '',
                        e.get('zustand') or '') for e in latest]}


def quantum(own=None):
    """Quantenreisen: Zielwahlen, Ankünfte, häufigste Ziele."""
    sessions = _counted(own)
    targets = {}
    for entry in sessions:
        for place, count in (entry.get('ziele') or {}).items():
            targets[place] = targets.get(place, 0) + count
    return {'spruenge': sum(e.get('spruenge', 0) for e in sessions),
            'zielwahlen': sum(e.get('zielwahlen', 0) for e in sessions),
            'ziele': _top(targets, 6)}


def stability(own=None, recent=8):
    """Stabilität: wie Sitzungen endeten und warum die Verbindung abriss."""
    sessions = _counted(own)
    crashed = sum(1 for e in sessions if e.get('absturz'))
    clean = sum(1 for e in sessions if e.get('sauber') and not e.get('absturz'))
    reasons = {}
    for entry in sessions:
        for reason, count in (entry.get('abbrueche') or {}).items():
            reasons[reason] = reasons.get(reason, 0) + count
    latest = sorted(sessions, key=lambda e: e.get('von') or 0,
                    reverse=True)[:recent]
    return {'sitzungen': len(sessions),
            'sauber': clean,
            'absturz': crashed,
            'ohne_ende': len(sessions) - clean - crashed,
            'gruende': _top(reasons, 8),
            'abbrueche': sum(reasons.values()),
            'letzte': [(e.get('von'), (e.get('bis') or 0) - (e.get('von') or 0),
                        end_of(e), sum((e.get('abbrueche') or {}).values()))
                       for e in latest]}


def end_of(entry):
    """'absturz', 'sauber' oder 'offen' — ein Absturz schlägt ein sauberes
    Ende, denn nach einem Grafikabsturz schreibt das Spiel oft noch `Quit`."""
    if entry.get('absturz'):
        return 'absturz'
    return 'sauber' if entry.get('sauber') else 'offen'


# Anzeigenamen aus der `global.ini` — einmal geladen, dann gemerkt.
_NAMES = [None]


def load_names(fetch=False):
    """Die Originalnamen des Spiels, kleingeschrieben nachschlagbar.

    `fetch=True` liest sie beim ersten Mal aus der `Data.p4k` (rund eine
    Sekunde) — nur aus einem Hintergrund-Thread aufrufen."""
    if _NAMES[0] is None or (fetch and not _NAMES[0]):
        from . import gametext
        try:
            found = gametext.names_or_fetch() if fetch else \
                gametext.saved_names()
        except Exception as exception:
            errors.record('play_stats.load_names', exception)
            found = {}
        _NAMES[0] = {k.lower(): v for k, v in (found or {}).items()}
    return _NAMES[0]


def display_name(cls, vehicle=False):
    """`AEGS_Sabre` -> „Aegis Sabre", `behr_lmg_ballistic_01` -> „FS-9 LMG".

    Ohne Namenstabelle bleibt der Schlüssel stehen, nur lesbarer."""
    names = load_names()
    key = ('vehicle_name' if vehicle else 'item_name') + cls.lower()
    return names.get(key) or cls.replace('_', ' ')


def export(target, own=None, version=''):
    """Die Statistik als Datei zum Aufheben oder Weitergeben.

    ⚠ **Ohne Spielernamen.** Die Datei ist zum Teilen gedacht; wer sie im
    Discord zeigt, soll nicht nebenbei seinen Account verraten."""
    from . import playtime
    data = summary(own)
    stats = load()['sitzungen']
    rows = []
    for entry in playtime.load().get('sitzungen', []):
        start, end = entry.get('von'), entry.get('bis')
        if not start or not end:
            continue
        mine = stats.get(str(start)) or {}
        rows.append({
            'von': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime(start)),
            'bis': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime(end)),
            'auftraege': mine.get('auftraege'),
            'fehlgeschlagen': mine.get('fehlgeschlagen'),
            'spruenge': mine.get('spruenge'),
        })
    out = {'werkzeug': 'VerseKit', 'version': version,
           'erstellt': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
           'gesamt': {k: v for k, v in data.items() if k != 'waermekarte'},
           'waermekarte': data['waermekarte'],
           'sitzungen': rows}
    try:
        with open(target, 'w', encoding='utf-8') as f:
            json.dump(out, f, ensure_ascii=False, indent=1)
        return True
    except Exception as exception:
        errors.record('play_stats.export', exception)
        return False
