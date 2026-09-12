# SPDX-License-Identifier: GPL-3.0-only
#
# SC BP Watcher — Spielzeit
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
Wie lange wurde gespielt — insgesamt und in dieser Sitzung.

## ⚠⚠ Warum es dafuer eine eigene Datei braucht

Star Citizen hebt seine Protokolle nur begrenzt auf. Gemessen am 05.09.2026:
188 Sicherungen, die **88 Tage** abdecken — alles davor ist weg, obwohl
laenger gespielt wurde. Wer die Spielzeit allein aus den vorhandenen Logs
rechnet, bekommt also jeden Monat eine kleinere Vergangenheit.

Deshalb wird jede erkannte Sitzung **fortgeschrieben**: einmal gelesen, fuer
immer gezaehlt. Was aus den Logs verschwindet, bleibt hier stehen.

## Was gezaehlt wird

Eine Sitzung zaehlt, wenn der Spieler wirklich im Spiel angekommen ist
(`missionslog.SPAWN_MARKE`). Ein Start, der nie so weit kam, ist keine
Spielzeit — auch wenn das Protokoll zwanzig Minuten lang ist, weil jemand im
Ladebildschirm haengen blieb.

Kurze Sitzungen zaehlen dagegen mit: Wer sich einloggt, kurz nachsieht und
wieder geht, hat gespielt. Gemessen sind das 43 Sitzungen mit zusammen 1,3
Stunden — eine Grenze zu ziehen waere eine Behauptung ueber „richtiges"
Spielen, und die steht dem Programm nicht zu.

## ⚠ Ueberlappungen

Zeitspannen werden **zusammengefuehrt**, nicht summiert. In den echten Daten
gab es einen Fall mit 7,9 Stunden Ueberschneidung — vermutlich zwei parallel
mitgeschriebene Protokolle. Ohne Zusammenfuehren stuenden die doppelt in der
Summe, und die Zahl waere falsch, ohne dass es jemandem auffiele.

## ⚠ Die Sicherung nimmt diese Datei von allein mit

`backup.py` sichert **alles** ausser dem, was ausdruecklich als nachladbar
gilt. `spielzeit.json` gehoert NICHT dort hinein: Sie laesst sich nicht neu
beschaffen, sobald die Logs rotiert sind. Beim Rechnerwechsel kommt sie damit
ohne Zutun mit.
"""
import json
import os
import re

from . import fehler, pfade

FILE = 'spielzeit.json'
FORMAT = 1

# Der Zeitstempel am Zeilenanfang: <2026-08-29T16:02:14.792Z>
_TIMESTAMP = re.compile(r'<(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d)')

# ⚠ Eine Sitzung, die laenger als das dauert, ist keine mehr. Star Citizen
# haelt keine 24-Stunden-Sitzung durch; so ein Wert entsteht durch eine
# verstellte Uhr oder ein Protokoll, das zwei Laeufe enthaelt. Lieber eine
# Sitzung verwerfen als die Gesamtzahl mit einem Ausreisser verderben.
MAX_SPAN_SEC = 24 * 3600


def _seconds(stamp):
    try:
        import calendar
        import time as _t
        return calendar.timegm(_t.strptime(stamp[:19], '%Y-%m-%dT%H:%M:%S'))
    except Exception:
        return None


def path():
    return pfade.app_datei(FILE)


def load():
    """Die gespeicherten Sitzungen — `{'format':…, 'sitzungen':[…]}`."""
    try:
        with open(path(), encoding='utf-8') as f:
            data = json.load(f)
        if isinstance(data, dict) and isinstance(data.get('sitzungen'), list):
            return data
    except (OSError, ValueError):
        pass
    except Exception as ausnahme:
        fehler.merken('playtime.load', ausnahme)
    return {'format': FORMAT, 'sitzungen': []}


def save(data):
    """Schreiben. Meldet, wenn es scheitert — sonst waere die Zeit still weg."""
    try:
        data['format'] = FORMAT
        ziel = path()
        ordner = os.path.dirname(ziel)
        if ordner and not os.path.isdir(ordner):
            os.makedirs(ordner)
        # ⚠ Erst daneben schreiben, dann umbenennen: Ein Absturz mittendrin
        # haette sonst eine halbe Datei hinterlassen — und damit die ganze
        # aufgezeichnete Vergangenheit.
        vorlaeufig = ziel + '.neu'
        with open(vorlaeufig, 'w', encoding='utf-8') as f:
            json.dump(data, f, ensure_ascii=False)
        os.replace(vorlaeufig, ziel)
        return True
    except Exception as ausnahme:
        fehler.merken('playtime.save', ausnahme)
        return False


def span_from_log(log_path, spawn_mark=None):
    """(von, bis) einer Protokolldatei in Sekunden — oder None.

    ⚠ Gibt None zurueck, wenn der Spieler nie im Spiel ankam. Ein
    Ladebildschirm, in dem jemand zwanzig Minuten haengt, ist keine Spielzeit.
    """
    if spawn_mark is None:
        from .missionslog import SPAWN_MARKE
        spawn_mark = SPAWN_MARKE
    erste = letzte = None
    drin = False
    try:
        with open(log_path, encoding='utf-8', errors='replace') as f:
            for line in f:
                if not drin and spawn_mark in line:
                    drin = True
                match = _TIMESTAMP.search(line)
                if match:
                    if erste is None:
                        erste = match.group(1)
                    letzte = match.group(1)
    except Exception:
        return None
    if not drin:
        return None
    start, end = _seconds(erste or ''), _seconds(letzte or '')
    if not start or not end or end < start:
        return None
    if (end - start) > MAX_SPAN_SEC:
        return None
    return (start, end)


def _merge_spans(spans):
    """Ueberlappende Zeitraeume verschmelzen — sonst zaehlt Zeit doppelt.

    ⚠ In den echten Daten gab es genau einen solchen Fall, mit **7,9 Stunden**
    Ueberschneidung. Ohne diesen Schritt stuende die Zeit zweimal in der Summe.
    """
    if not spans:
        return []
    geordnet = sorted(spans)
    raus = [list(geordnet[0])]
    for start, end in geordnet[1:]:
        if start <= raus[-1][1]:
            raus[-1][1] = max(raus[-1][1], end)
        else:
            raus.append([start, end])
    return raus


def catch_up(files):
    """Protokolle einlesen und die Datenbank fortschreiben.

    Gibt die Zahl der neu dazugekommenen Sitzungen zurueck. Bereits bekannte
    werden am Startzeitpunkt erkannt und nicht doppelt gezaehlt — der
    **Dateiname** taugt dafuer nicht: Die laufende `Game.log` wird beim
    naechsten Spielstart zu einer `logbackups/…`-Datei umbenannt und waere
    dann ein zweites Mal „neu".

    ⚠⚠ **Uebergeben werden ALLE Protokolle, nicht nur die frisch
    hinzugekommenen.** Der erste Anlauf am 05.09.2026 bekam nur die Dateien,
    die das Auftrags-Protokoll noch nicht kannte — und das kannte auf einem
    gewachsenen Rechner laengst alle. Ergebnis: Die Anzeige stand auf
    „0 min", obwohl 188 Protokolle mit 286 Stunden dalagen. Gemeldet mit „ich
    dachte er liest die alten logs und zaehlt zusammen".

    Damit das nicht jeden Start eine Sekunde kostet, hat diese Datei ihren
    **eigenen** Lesestand: Dateiname und Groesse. Waechst eine Datei (die
    laufende `Game.log` tut das staendig), wird sie erneut gelesen.
    """
    data = load()
    bekannt = {}
    for eintrag in data['sitzungen']:
        bekannt[eintrag.get('von')] = eintrag
    gelesen = data.get('gelesen')
    if not isinstance(gelesen, dict):
        gelesen = {}
        data['gelesen'] = gelesen

    tmp_path = 0
    for log_path in (files or []):
        # ⚠ Vor dem Lesen fragen, ob es noetig ist: 188 Dateien sind zusammen
        # leicht ein halbes Gigabyte.
        try:
            marke = os.path.getsize(log_path)
        except OSError:
            continue
        name = os.path.basename(log_path)
        if gelesen.get(name) == marke:
            continue
        gelesen[name] = marke

        span = span_from_log(log_path)
        if not span:
            continue
        start, end = span
        vorhanden = bekannt.get(start)
        if vorhanden is None:
            eintrag = {'von': start, 'bis': end}
            data['sitzungen'].append(eintrag)
            bekannt[start] = eintrag
            tmp_path += 1
        elif end > vorhanden.get('bis', 0):
            # ⚠ Dieselbe Sitzung, aber laenger als beim letzten Mal: Die
            # laufende Game.log waechst ja noch. Ohne diesen Zweig bliebe die
            # heutige Sitzung fuer immer auf ihrem ersten Stand stehen.
            vorhanden['bis'] = end

    data['sitzungen'].sort(key=lambda e: e.get('von') or 0)
    save(data)
    return tmp_path


def total(data=None, with_running=True):
    """Die aufgezeichnete Spielzeit in Sekunden.

    ⚠⚠ **Die laufende Sitzung zaehlt mit ihrem AKTUELLEN Stand.** In der
    Datenbank steht sie mit dem Stand vom letzten Nachlesen — waehrend gespielt
    wird, waere die Gesamtzahl also eingefroren, und nach zwei Stunden Spielen
    stuende oben dieselbe Zahl wie beim Start. Das sieht kaputt aus.

    ⚠ Doppelt gezaehlt wird dabei nichts: `_zusammenfuehren()` verschmilzt die
    gespeicherte kuerzere Spanne mit der aktuellen laengeren, weil sie
    denselben Anfang haben. Genau dafuer ist es da.
    """
    data = data if data is not None else load()
    spans = [(e.get('von'), e.get('bis')) for e in data.get('sitzungen', [])
               if e.get('von') and e.get('bis')]
    if with_running:
        jetzt = _running_span()
        if jetzt:
            spans.append(jetzt)
    return sum(end - start for start, end in _merge_spans(spans))


def since(data=None):
    """Ab wann aufgezeichnet wurde — als Sekunden, oder None."""
    data = data if data is not None else load()
    zeiten = [e.get('von') for e in data.get('sitzungen', []) if e.get('von')]
    return min(zeiten) if zeiten else None


def _running_span():
    """(von, bis) der gerade laufenden Sitzung — oder None.

    ⚠ **Das Ende kommt aus der Schreibzeit der Datei, nicht aus der Uhr.**
    Wer das Spiel schliesst und den Watcher offen laesst, saehe sonst eine
    Sitzung, die weiterlaeuft, obwohl niemand spielt.

    ⚠ **Der Anfang wird nicht bei jedem Aufruf neu gesucht.** Die `Game.log`
    hat Megabyte, und diese Frage wird jede Minute gestellt. Gemerkt wird er,
    solange dieselbe Datei dieselbe Sitzung ist; faengt das Spiel neu an, wird
    die Datei **kuerzer** — daran ist der Wechsel zu erkennen.
    """
    filename = pfade.game_log()
    if not filename or not pfade.spiel_laeuft():
        return None
    try:
        size = os.path.getsize(filename)
        if (_cache.get('datei') != filename
                or _cache.get('groesse', 0) > size
                or not _cache.get('von')):
            span = span_from_log(filename)
            _cache['datei'] = filename
            _cache['von'] = span[0] if span else None
        _cache['groesse'] = size
        start = _cache.get('von')
        if not start:
            return None
        end = int(os.path.getmtime(filename))
        if not (0 <= (end - start) <= MAX_SPAN_SEC):
            return None
        return (start, end)
    except Exception:
        return None


def session_now():
    """Die laufende Sitzung in Sekunden — 0, wenn das Spiel nicht laeuft."""
    span = _running_span()
    return (span[1] - span[0]) if span else 0


_cache = {}


def as_text(seconds):
    """Sekunden als „3 h 14 min" — kurz genug fuer eine Kopfzeile.

    ⚠ Keine Sekundenanzeige: Sie aendert sich staendig, zieht den Blick auf
    sich und sagt bei einer Spielzeit nichts. Unter einer Minute steht „0 min",
    nicht „gerade eben" — eine Zahl bleibt eine Zahl.
    """
    try:
        seconds = max(0, int(seconds))
    except Exception:
        return '0 min'
    stunden, rest = divmod(seconds, 3600)
    minuten = rest // 60
    if stunden:
        return '%d h %02d min' % (stunden, minuten)
    return '%d min' % minuten
