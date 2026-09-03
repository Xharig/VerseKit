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

Das tut `auftraege.py`, und zwar besser, als es hier je entstehen wuerde: Es
holt die Formulierungen („Auftrag angenommen") aus der `global.ini` des
Spielers statt sie einzutragen, geht auf den Missions-**Schluessel** statt auf
den Wortlaut (sonst gilt jedes Zwischenziel als Auftrag), putzt die eigenen
Bauplan-Marken aus dem Titel und kennt drei Enden statt einem.

Der erste Entwurf dieses Moduls hat all das danebengebaut und dieselben Fallen
einzeln neu entdeckt. **Zwei Auswertungen derselben Logzeilen laufen beim
naechsten Patch auseinander** — deshalb kommt hier jede Auftragserkennung aus
`auftraege.py`.

Was dieses Modul beitraegt, ist genau das, was dort fehlt:

| | |
|---|---|
| **Wann** | `auftraege.ereignisse_aus_text()` liefert keinen Zeitpunkt — hier wird Zeile fuer Zeile gelesen, damit der Zeitstempel danebensteht |
| **Ueber Sitzungen hinweg** | Jedes Einloggen beginnt eine neue `Game.log`. Ein Auftrag, abends angenommen und morgens beendet, steht in zwei Dateien |
| **Abgeschlossen oder abgebrochen** | `auftraege.py` kennt nur „beendet". Der Unterschied steht in `<EndMission> … CompletionType[Complete\\|Abandon]` |
| **Wie oft, und Suche** | Zaehlen und Filtern ueber den Namen |

## ⚠ Die Doppelmeldung

Das Spiel schickt dieselbe Annahme **zweimal in derselben Millisekunde**, nur
mit verschiedener Nummer in den eckigen Klammern:

    <2026-08-29T16:02:14.792Z> [41] Auftrag angenommen: Retake Platforms …
    <2026-08-29T16:02:14.792Z> [44] Auftrag angenommen: Retake Platforms …

Gemessen an echten Sicherungen: Ohne Gegenmassnahme steht jeder Auftrag
doppelt im Protokoll. Entdoppelt wird ueber **(Zeitpunkt, Titel, Art)** — die
Nummer taugt dafuer nicht, und zwei echte Annahmen desselben Auftrags in
derselben Millisekunde gibt es nicht.
"""
import os
import re

from . import auftraege, fehler

# Der Zeitstempel am Zeilenanfang: <2026-08-29T16:02:14.792Z>
_ZEIT = re.compile(r'<(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d)')

# Abgeschlossen oder abgebrochen — nur diese Zeile sagt es.
_ENDE_ART = re.compile(r'<EndMission>.*?MissionId\[(?P<mid>[^\]]*)\]'
                       r'.*?CompletionType\[(?P<art>[^\]]*)\]')

ABGESCHLOSSEN = 'abgeschlossen'
ABGEBROCHEN = 'abgebrochen'
LAEUFT = 'laeuft'


def _zeit(zeile):
    m = _ZEIT.search(zeile)
    return m.group(1) if m else ''


def _eintrag(titel, wann, quelle):
    return {'name': titel, 'wann': wann, 'zustand': LAEUFT,
            'ziele_fertig': 0, 'ziele_gesamt': 0, 'quelle': quelle}


def _lesen(pfad, offen, fertig, gesehen, kennung, muster_an, muster_aus):
    """Ein Log lesen und die Buchfuehrung fortschreiben.

    `offen` und `fertig` werden ueber Dateigrenzen hinweg weitergereicht —
    ein Auftrag kann in einer spaeteren Sitzung enden als er begann.
    """
    quelle = os.path.basename(pfad)
    enden = {}          # mission_id -> 'Complete' | 'Abandon'
    ziele = {}          # mission_id -> {objective_id: zustand}

    try:
        with open(pfad, encoding='utf-8', errors='replace') as f:
            for zeile in f:
                # Die Art des Endes merken, bevor das Ereignis selbst kommt —
                # im Log steht EndMission vor der Mitteilung.
                a = _ENDE_ART.search(zeile)
                if a:
                    enden[a.group('mid')] = a.group('art')

                # ('zustand', mission_id, objective_id, zustand, kennzeichen)
                for zust in auftraege.ziel_ereignisse_aus_text(zeile):
                    if zust and zust[0] == 'zustand':
                        ziele.setdefault(zust[1], {})[zust[2]] = zust[3]

                ereignisse = auftraege.ereignisse_aus_text(
                    zeile, muster_an, muster_aus)
                if not ereignisse:
                    continue
                wann = _zeit(zeile)

                for ist_annahme, roh, mission_id, objective_id in ereignisse:
                    # ⚠ IMMER durch `sauber()`. Im Log steht der Titel mal als
                    # „Retake Platforms From Nine Tails <EM4>[BP!]</EM4>", mal
                    # mit „[SCBPW] … [/SCBPW]" — je nachdem, was der Watcher
                    # gerade ins Spiel eingetragen hat. Ungeputzt gilt derselbe
                    # Auftrag als zwei verschiedene: gemessen 3× und 2× statt 5×.
                    titel = auftraege.sauber(roh)
                    schluessel = (wann, titel, ist_annahme)
                    if schluessel in gesehen:
                        continue        # Doppelmeldung, siehe Modulkopf
                    gesehen.add(schluessel)

                    if ist_annahme is None:
                        # ⚠⚠ Spielwelt verlassen — hier NICHT raeumen.
                        #
                        # `auftraege.py` raeumt an dieser Stelle auf, und das ist
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

                    if ist_annahme:
                        # ⚠ Titel mit rohem Platzhalter gehoeren nicht ins
                        # Protokoll: `Ling Family - Rang: ~mission(ReputationRank)`
                        # setzt das Spiel erst beim Anzeigen ein, die Werte
                        # stehen nirgends im Log. Als eigener Eintrag waere das
                        # ein zweiter Auftrag, den es nie gab — daneben stand
                        # derselbe mit aufgeloestem Rang („NEULING").
                        if not titel or '~mission(' in titel:
                            continue
                        # ⚠⚠ **Wiederaufnahme ist keine neue Annahme.** Beim
                        # Einloggen meldet das Spiel jeden laufenden Auftrag
                        # erneut als angenommen. Ohne diese Pruefung stand
                        # „Retake Platforms From Nine Tails" 29× im Protokoll,
                        # obwohl es fuenf Durchlaeufe waren — einmal je Sitzung,
                        # in der er offen war.
                        #
                        # Das ist auch der Grund, warum `auftraege.py` beim
                        # Verlassen der Welt raeumt: Fuer die Live-Anzeige ist
                        # Raeumen die einfachere Loesung. Ein Protokoll darf
                        # nicht raeumen (sonst fehlen Auftraege ueber zwei
                        # Abende) und muss die Wiederaufnahme deshalb hier
                        # abfangen.
                        schon_offen = any(
                            e['name'] == titel for e in offen) or (
                                mission_id and mission_id in kennung
                                and any(e['name'] == kennung[mission_id]
                                        for e in offen))
                        if schon_offen:
                            continue
                        offen.append(_eintrag(titel, wann, quelle))
                        if mission_id:
                            kennung[mission_id] = titel
                        continue

                    # ⚠⚠ Ein Ende — aber WELCHES? Die Zuordnung macht
                    # `beendet_welchen()`, nicht dieses Modul. Sein erster
                    # Schritt ist der entscheidende: Steht eine ObjectiveId
                    # dabei, endet nur ein Zwischenziel und der Auftrag laeuft
                    # weiter. Ohne diesen Filter landete „Obere Plattform
                    # erreichen" achtmal als eigener Auftrag im Protokoll —
                    # es ist ein Ziel innerhalb von „Retake Platforms".
                    #
                    # Und wenn nichts zugeordnet werden kann, wird NICHTS
                    # eingetragen. Ein erfundener Auftrag ist schlimmer als ein
                    # fehlender.
                    treffer = auftraege.beendet_welchen(
                        titel, mission_id, objective_id,
                        [e['name'] for e in offen], kennung)
                    if not treffer:
                        continue
                    art = enden.get(mission_id, '')
                    zustand = (ABGEBROCHEN if art.lower().startswith('abandon')
                               else ABGESCHLOSSEN)
                    # ⚠ Den AELTESTEN passenden schliessen, nicht den juengsten.
                    # Sonst bekommt ein Auftrag das Ende eines spaeteren
                    # Durchlaufs und im Protokoll steht ein Ende vor seinem
                    # Anfang („21:26 abgeschlossen → 17:42").
                    for eintrag in offen:
                        if eintrag['name'] == treffer:
                            eintrag['zustand'] = zustand
                            eintrag['bis'] = wann
                            offen.remove(eintrag)
                            fertig.append(eintrag)
                            break
    except OSError as ausnahme:
        fehler.merken('missionslog.lesen', ausnahme)
        return

    # Fortschritt nur, wo die Zuordnung eindeutig ist: Das Log verbindet Titel
    # und Missionskennung nirgends. Bei genau einem offenen Auftrag und genau
    # einer Kennung kann es nur diese sein — sonst bliebe es Raten, und eine
    # falsche Zahl ist schlechter als keine.
    if len(offen) == 1 and len(ziele) == 1:
        stand = list(ziele.values())[0]
        # Phasen-Ziele beschreiben den Abschnitt, nicht eine Aufgabe, die der
        # Spieler abhakt — sie gehoeren nicht in „3 von 5".
        echte = {k: v for k, v in stand.items() if not str(k).startswith('phase_')}
        if echte:
            offen[0]['ziele_gesamt'] = len(echte)
            offen[0]['ziele_fertig'] = sum(
                1 for v in echte.values() if str(v).upper().endswith('COMPLETED'))


def aus_dateien(pfade):
    """Mehrere Logs als EINE Geschichte auswerten, neuester Auftrag zuerst."""
    # `kennung` merkt sich mission_id -> Titel. `beendet_welchen()` greift
    # darauf zurueck, wenn der Titel beim Ende anders lautet als bei der
    # Annahme — laut Messung dort 62 von 362 Faellen.
    offen, fertig, gesehen, kennung = [], [], set(), {}
    muster_an, muster_aus = auftraege.muster(), auftraege.ende_muster()
    for pfad in pfade:
        _lesen(pfad, offen, fertig, gesehen, kennung, muster_an, muster_aus)
    return sorted(fertig + offen, key=lambda e: e.get('wann') or '',
                  reverse=True)


def aus_ordner(ordner, laufende=None):
    """Alle Logs eines Ordners auswerten — `ordner` darf eine Liste sein.

    Windows und Linux sichern in getrennte Ordner; wer auf beiden spielt, will
    ein Protokoll, nicht zwei. `laufende` ist die gerade beschriebene
    `Game.log`, falls sie mitgelesen werden soll.
    """
    ordnerliste = [ordner] if isinstance(ordner, str) else list(ordner or [])
    dateien = []
    for o in ordnerliste:
        if o and os.path.isdir(o):
            for name in os.listdir(o):
                if name.lower().endswith('.log'):
                    dateien.append(os.path.join(o, name))
    if laufende and os.path.isfile(laufende):
        dateien.append(laufende)

    return aus_dateien(sorted(set(dateien), key=_spielzeit))


def _spielzeit(pfad):
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
        with open(pfad, encoding='utf-8', errors='replace') as f:
            for _ in range(200):        # der Stempel steht ganz oben
                zeile = f.readline()
                if not zeile:
                    break
                wann = _zeit(zeile)
                if wann:
                    return wann
    except OSError:
        pass
    # Ohne Stempel ans Ende — lieber hinten anstellen als die Reihe verdrehen.
    return '9999'


def suchen(eintraege, text):
    """Nach Auftragsnamen filtern, ohne Ruecksicht auf Gross- und Kleinschreibung."""
    text = (text or '').strip().lower()
    if not text:
        return eintraege
    return [e for e in eintraege if text in (e.get('name') or '').lower()]


def zusammenfassen(eintraege):
    """Wie oft wurde welcher Auftrag gespielt? Name -> (gesamt, abgeschlossen)."""
    zaehler = {}
    for e in eintraege:
        gesamt, fertig = zaehler.get(e['name'], (0, 0))
        zaehler[e['name']] = (gesamt + 1,
                              fertig + (1 if e['zustand'] == ABGESCHLOSSEN
                                        else 0))
    return zaehler
