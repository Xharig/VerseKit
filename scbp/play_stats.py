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

⛔ **Tode und Verletzungen werden nicht gezählt** (entschieden 27.09.2026).
Ein Tod steht in keinem Ereignis mehr im Log, nur als Einblendungstext in der
Spielsprache („Notfalldienste sind unterwegs"). Das bräche bei englischem
Client und bei jeder Textänderung still — lieber keine Zahl als eine falsche.

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
FORMAT = 1

_TIMESTAMP = re.compile(r'<(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d)')
_END_MISSION = re.compile(
    r'<EndMission>.*?MissionId\[([^\]]+)\].*?Player\[([^\]]*)\]'
    r'.*?CompletionType\[([A-Za-z]+)\]')
_QUANTUM = '<Quantum Drive Arrived - Arrived at Final Destination>'
_QUIT = '<SystemQuit>'

DONE = 'Complete'
FAILED = 'Fail'


def path():
    return paths.app_file(FILE)


def load():
    """`{'format':…, 'sitzungen': {von: {…}}, 'gelesen': {name: groesse}}`."""
    try:
        with open(path(), encoding='utf-8') as f:
            data = json.load(f)
        if isinstance(data, dict) and isinstance(data.get('sitzungen'), dict):
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
    missions = {}
    jumps = set()
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
                        if not account or player.lower() == account.lower():
                            missions[mission_id] = kind
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
            'spruenge': len(jumps)}


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
    if changed:
        save(data)
    return added


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
