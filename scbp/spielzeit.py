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

`sicherung.py` sichert **alles** ausser dem, was ausdruecklich als nachladbar
gilt. `spielzeit.json` gehoert NICHT dort hinein: Sie laesst sich nicht neu
beschaffen, sobald die Logs rotiert sind. Beim Rechnerwechsel kommt sie damit
ohne Zutun mit.
"""
import json
import os
import re

from . import fehler, pfade

DATEI = 'spielzeit.json'
FORMAT = 1

# Der Zeitstempel am Zeilenanfang: <2026-08-29T16:02:14.792Z>
_ZEIT = re.compile(r'<(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d)')

# ⚠ Eine Sitzung, die laenger als das dauert, ist keine mehr. Star Citizen
# haelt keine 24-Stunden-Sitzung durch; so ein Wert entsteht durch eine
# verstellte Uhr oder ein Protokoll, das zwei Laeufe enthaelt. Lieber eine
# Sitzung verwerfen als die Gesamtzahl mit einem Ausreisser verderben.
GRENZE_SEK = 24 * 3600


def _sekunden(stempel):
    try:
        import calendar
        import time as _t
        return calendar.timegm(_t.strptime(stempel[:19], '%Y-%m-%dT%H:%M:%S'))
    except Exception:
        return None


def pfad():
    return pfade.app_datei(DATEI)


def laden():
    """Die gespeicherten Sitzungen — `{'format':…, 'sitzungen':[…]}`."""
    try:
        with open(pfad(), encoding='utf-8') as f:
            daten = json.load(f)
        if isinstance(daten, dict) and isinstance(daten.get('sitzungen'), list):
            return daten
    except (OSError, ValueError):
        pass
    except Exception as ausnahme:
        fehler.merken('spielzeit.laden', ausnahme)
    return {'format': FORMAT, 'sitzungen': []}


def sichern(daten):
    """Schreiben. Meldet, wenn es scheitert — sonst waere die Zeit still weg."""
    try:
        daten['format'] = FORMAT
        ziel = pfad()
        ordner = os.path.dirname(ziel)
        if ordner and not os.path.isdir(ordner):
            os.makedirs(ordner)
        # ⚠ Erst daneben schreiben, dann umbenennen: Ein Absturz mittendrin
        # haette sonst eine halbe Datei hinterlassen — und damit die ganze
        # aufgezeichnete Vergangenheit.
        vorlaeufig = ziel + '.neu'
        with open(vorlaeufig, 'w', encoding='utf-8') as f:
            json.dump(daten, f, ensure_ascii=False)
        os.replace(vorlaeufig, ziel)
        return True
    except Exception as ausnahme:
        fehler.merken('spielzeit.sichern', ausnahme)
        return False


def spanne_aus_log(dateipfad, spawn_marke=None):
    """(von, bis) einer Protokolldatei in Sekunden — oder None.

    ⚠ Gibt None zurueck, wenn der Spieler nie im Spiel ankam. Ein
    Ladebildschirm, in dem jemand zwanzig Minuten haengt, ist keine Spielzeit.
    """
    if spawn_marke is None:
        from .missionslog import SPAWN_MARKE
        spawn_marke = SPAWN_MARKE
    erste = letzte = None
    drin = False
    try:
        with open(dateipfad, encoding='utf-8', errors='replace') as f:
            for zeile in f:
                if not drin and spawn_marke in zeile:
                    drin = True
                treffer = _ZEIT.search(zeile)
                if treffer:
                    if erste is None:
                        erste = treffer.group(1)
                    letzte = treffer.group(1)
    except Exception:
        return None
    if not drin:
        return None
    von, bis = _sekunden(erste or ''), _sekunden(letzte or '')
    if not von or not bis or bis < von:
        return None
    if (bis - von) > GRENZE_SEK:
        return None
    return (von, bis)


def _zusammenfuehren(spannen):
    """Ueberlappende Zeitraeume verschmelzen — sonst zaehlt Zeit doppelt.

    ⚠ In den echten Daten gab es genau einen solchen Fall, mit **7,9 Stunden**
    Ueberschneidung. Ohne diesen Schritt stuende die Zeit zweimal in der Summe.
    """
    if not spannen:
        return []
    geordnet = sorted(spannen)
    raus = [list(geordnet[0])]
    for von, bis in geordnet[1:]:
        if von <= raus[-1][1]:
            raus[-1][1] = max(raus[-1][1], bis)
        else:
            raus.append([von, bis])
    return raus


def nachtragen(dateien):
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
    daten = laden()
    bekannt = {}
    for eintrag in daten['sitzungen']:
        bekannt[eintrag.get('von')] = eintrag
    gelesen = daten.get('gelesen')
    if not isinstance(gelesen, dict):
        gelesen = {}
        daten['gelesen'] = gelesen

    neu = 0
    for dateipfad in (dateien or []):
        # ⚠ Vor dem Lesen fragen, ob es noetig ist: 188 Dateien sind zusammen
        # leicht ein halbes Gigabyte.
        try:
            marke = os.path.getsize(dateipfad)
        except OSError:
            continue
        name = os.path.basename(dateipfad)
        if gelesen.get(name) == marke:
            continue
        gelesen[name] = marke

        spanne = spanne_aus_log(dateipfad)
        if not spanne:
            continue
        von, bis = spanne
        vorhanden = bekannt.get(von)
        if vorhanden is None:
            eintrag = {'von': von, 'bis': bis}
            daten['sitzungen'].append(eintrag)
            bekannt[von] = eintrag
            neu += 1
        elif bis > vorhanden.get('bis', 0):
            # ⚠ Dieselbe Sitzung, aber laenger als beim letzten Mal: Die
            # laufende Game.log waechst ja noch. Ohne diesen Zweig bliebe die
            # heutige Sitzung fuer immer auf ihrem ersten Stand stehen.
            vorhanden['bis'] = bis

    daten['sitzungen'].sort(key=lambda e: e.get('von') or 0)
    sichern(daten)
    return neu


def gesamt(daten=None, mit_laufender=True):
    """Die aufgezeichnete Spielzeit in Sekunden.

    ⚠⚠ **Die laufende Sitzung zaehlt mit ihrem AKTUELLEN Stand.** In der
    Datenbank steht sie mit dem Stand vom letzten Nachlesen — waehrend gespielt
    wird, waere die Gesamtzahl also eingefroren, und nach zwei Stunden Spielen
    stuende oben dieselbe Zahl wie beim Start. Das sieht kaputt aus.

    ⚠ Doppelt gezaehlt wird dabei nichts: `_zusammenfuehren()` verschmilzt die
    gespeicherte kuerzere Spanne mit der aktuellen laengeren, weil sie
    denselben Anfang haben. Genau dafuer ist es da.
    """
    daten = daten if daten is not None else laden()
    spannen = [(e.get('von'), e.get('bis')) for e in daten.get('sitzungen', [])
               if e.get('von') and e.get('bis')]
    if mit_laufender:
        jetzt = _laufende_spanne()
        if jetzt:
            spannen.append(jetzt)
    return sum(bis - von for von, bis in _zusammenfuehren(spannen))


def seit(daten=None):
    """Ab wann aufgezeichnet wurde — als Sekunden, oder None."""
    daten = daten if daten is not None else laden()
    zeiten = [e.get('von') for e in daten.get('sitzungen', []) if e.get('von')]
    return min(zeiten) if zeiten else None


def _laufende_spanne():
    """(von, bis) der gerade laufenden Sitzung — oder None.

    ⚠ **Das Ende kommt aus der Schreibzeit der Datei, nicht aus der Uhr.**
    Wer das Spiel schliesst und den Watcher offen laesst, saehe sonst eine
    Sitzung, die weiterlaeuft, obwohl niemand spielt.

    ⚠ **Der Anfang wird nicht bei jedem Aufruf neu gesucht.** Die `Game.log`
    hat Megabyte, und diese Frage wird jede Minute gestellt. Gemerkt wird er,
    solange dieselbe Datei dieselbe Sitzung ist; faengt das Spiel neu an, wird
    die Datei **kuerzer** — daran ist der Wechsel zu erkennen.
    """
    datei = pfade.game_log()
    if not datei or not pfade.spiel_laeuft():
        return None
    try:
        groesse = os.path.getsize(datei)
        if (_merker.get('datei') != datei
                or _merker.get('groesse', 0) > groesse
                or not _merker.get('von')):
            spanne = spanne_aus_log(datei)
            _merker['datei'] = datei
            _merker['von'] = spanne[0] if spanne else None
        _merker['groesse'] = groesse
        von = _merker.get('von')
        if not von:
            return None
        bis = int(os.path.getmtime(datei))
        if not (0 <= (bis - von) <= GRENZE_SEK):
            return None
        return (von, bis)
    except Exception:
        return None


def sitzung_jetzt():
    """Die laufende Sitzung in Sekunden — 0, wenn das Spiel nicht laeuft."""
    spanne = _laufende_spanne()
    return (spanne[1] - spanne[0]) if spanne else 0


_merker = {}


def als_text(sekunden):
    """Sekunden als „3 h 14 min" — kurz genug fuer eine Kopfzeile.

    ⚠ Keine Sekundenanzeige: Sie aendert sich staendig, zieht den Blick auf
    sich und sagt bei einer Spielzeit nichts. Unter einer Minute steht „0 min",
    nicht „gerade eben" — eine Zahl bleibt eine Zahl.
    """
    try:
        sekunden = max(0, int(sekunden))
    except Exception:
        return '0 min'
    stunden, rest = divmod(sekunden, 3600)
    minuten = rest // 60
    if stunden:
        return '%d h %02d min' % (stunden, minuten)
    return '%d min' % minuten
