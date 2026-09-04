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

An echten Sitzungen gemessen: Ohne Gegenmassnahme steht jeder Auftrag
doppelt im Protokoll. Entdoppelt wird ueber **(Zeitpunkt, Titel, Art)** — die
Nummer taugt dafuer nicht, und zwei echte Annahmen desselben Auftrags in
derselben Millisekunde gibt es nicht.
"""
import json
import os
import re

from . import auftraege, fehler, pfade

DATEI = 'auftragslog.json'
# ⚠ 2 seit dem 04.09.2026. Ein Protokoll im Format 1 enthaelt zwei Fehler, die
# sich nicht nachtraeglich glattziehen lassen — Auftraege, die ewig „laeuft"
# blieben, und Bauplaene, die dadurch am falschen Auftrag haengen. Beides
# entsteht beim Lesen, also wird beim Formatwechsel **komplett neu gelesen**
# statt repariert. Das kostet einmalig ein paar Sekunden beim Start und ist
# der einzige Weg zu sauberen Daten.
FORMAT = 2

# Der Zeitstempel am Zeilenanfang: <2026-08-29T16:02:14.792Z>
_ZEIT = re.compile(r'<(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d)')

# Abgeschlossen oder abgebrochen — nur diese Zeile sagt es.
_ENDE_ART = re.compile(r'<EndMission>.*?MissionId\[(?P<mid>[^\]]*)\]'
                       r'.*?CompletionType\[(?P<art>[^\]]*)\]')

ABGESCHLOSSEN = 'abgeschlossen'
ABGEBROCHEN = 'abgebrochen'
LAEUFT = 'laeuft'
# ⚠ Kein Ende im Log, aber sicher nicht mehr offen — siehe `_verfallene_schliessen`.
# Bewusst NICHT als „abgebrochen" gefuehrt: Wir wissen nur, dass er nicht mehr
# laeuft, nicht warum. Eine Behauptung waere schlimmer als eine ehrliche Luecke.
VERFALLEN = 'verfallen'


def _zeit(zeile):
    m = _ZEIT.search(zeile)
    return m.group(1) if m else ''


# Wie lange nach dem Abgeben ein Bauplan noch zum Auftrag gezaehlt wird.
#
# ⚠ Die Belohnung faellt NACH dem Ende, nicht davor. Gemessen am 29.08.2026:
# Auftrag „Retake Platforms From Nine Tails" endete 17:42:00, der Bauplan
# „H4-PBF Ammo Carrier" kam 17:42:54 — 54 Sekunden spaeter. Ohne Nachlauf
# stuende er bei keinem Auftrag. Fuenf Minuten sind grosszuegig genug fuer eine
# lahme Serververbindung und kurz genug, dass er nicht beim naechsten Auftrag
# landet; laeuft ohnehin schon der naechste, gewinnt der (siehe `_bp_zuordnen`).
BP_NACHLAUF_SEK = 300


def _eintrag(titel, wann, quelle):
    return {'name': titel, 'wann': wann, 'zustand': LAEUFT,
            'ziele_fertig': 0, 'ziele_gesamt': 0, 'bauplaene': [],
            'quelle': quelle}


def _sekunden(stempel):
    """Ein Zeitstempel als Zahl — fuer den Abstand zwischen zwei Ereignissen."""
    try:
        import calendar
        import time as _t
        return calendar.timegm(_t.strptime(stempel[:19], '%Y-%m-%dT%H:%M:%S'))
    except Exception:
        return None


def _bp_zuordnen(name, wann, offen, fertig, gemeldet=None):
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
    `_verfallene_schliessen()` allein genuegt dafuer nicht: Es kann erst
    greifen, wenn die Datei durch ist, waehrend der Bauplan mittendrin faellt.

    Gemessen am 04.09.2026: „Willkommen im System" endete um 07:21:55, eine
    Sekunde spaeter fiel „Clearcut Module" — zugeschrieben wurde es einem
    Auftrag vom **31.08.**, der nur deshalb noch offen schien.

    Verlassen kann man sich darauf, weil das Spiel beim Einloggen jeden
    laufenden Auftrag erneut meldet, also am ANFANG der Datei — lange vor
    jedem Bauplan-Fund darin.
    """
    for ziel in reversed(offen):
        if ziel.get('bauplaene'):
            continue            # hat seinen Bauplan schon — Spielregel
        if gemeldet is not None and ziel['name'] not in gemeldet:
            continue            # laeuft in dieser Sitzung gar nicht
        ziel.setdefault('bauplaene', []).append(name)
        return True
    jetzt = _sekunden(wann)
    if jetzt is None:
        return False
    for eintrag in reversed(fertig):
        if eintrag.get('bauplaene'):
            continue
        ende = _sekunden(eintrag.get('bis') or '')
        if ende is not None and 0 <= jetzt - ende <= BP_NACHLAUF_SEK:
            eintrag.setdefault('bauplaene', []).append(name)
            return True
    return False


def _lesen(pfad, offen, fertig, gesehen, kennung, muster_an, muster_aus,
           bp_muster=None):
    """Ein Log lesen und die Buchfuehrung fortschreiben.

    `offen` und `fertig` werden ueber Dateigrenzen hinweg weitergereicht —
    ein Auftrag kann in einer spaeteren Sitzung enden als er begann.

    Gibt die Titel zurueck, die diese Sitzung als angenommen gemeldet hat —
    **auch die Wiederaufnahmen**. `_verfallene_schliessen()` braucht genau das.
    """
    quelle = os.path.basename(pfad)
    enden = {}          # mission_id -> 'Complete' | 'Abandon'
    ziele = {}          # mission_id -> {objective_id: zustand}
    gemeldet = set()    # welche Auftraege diese Sitzung ueberhaupt nennt

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

                # ⭐ Welcher Bauplan bei welchem Auftrag herauskam. Erkannt wird
                # er mit demselben Muster wie im Bestand (`phrasen.py`) — die
                # Formulierung steht in der `global.ini` des Spielers, nicht
                # hier. Die Zuordnung macht der Zeitpunkt: Ein Bauplan faellt
                # waehrend eines Auftrags oder kurz nach dem Abgeben.
                if bp_muster is not None:
                    for treffer in bp_muster.finditer(zeile):
                        roh_bp = next((g for g in treffer.groups() if g), '')
                        name_bp = auftraege.sauber(roh_bp)
                        if not name_bp:
                            continue
                        bp_wann = _zeit(zeile)
                        if (bp_wann, name_bp, 'bp') in gesehen:
                            continue    # dieselbe Doppelmeldung wie oben
                        gesehen.add((bp_wann, name_bp, 'bp'))
                        _bp_zuordnen(name_bp, bp_wann, offen, fertig, gemeldet)

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
                        # ⚠ VOR der Wiederaufnahme-Pruefung merken: Gerade die
                        # Wiederaufnahme ist der Beweis, dass der Auftrag in
                        # dieser Sitzung noch lief.
                        gemeldet.add(titel)
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
        return gemeldet

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
    return gemeldet


def aus_dateien(pfade):
    """Mehrere Logs als EINE Geschichte auswerten, neuester Auftrag zuerst."""
    # `kennung` merkt sich mission_id -> Titel. `beendet_welchen()` greift
    # darauf zurueck, wenn der Titel beim Ende anders lautet als bei der
    # Annahme — laut Messung dort 62 von 362 Faellen.
    offen, fertig, gesehen, kennung = [], [], set(), {}
    muster_an, muster_aus = auftraege.muster(), auftraege.ende_muster()
    # ⚠ Dasselbe Muster wie im Bauplan-Bestand — die Formulierung steht in der
    # `global.ini` des Spielers. Faellt es aus, laeuft das Protokoll weiter, nur
    # ohne die Bauplan-Zeilen: Ein Auftrags-Protokoll ohne Belohnungen ist
    # brauchbar, gar keines waere es nicht.
    try:
        from . import phrasen
        bp_muster = phrasen.muster()
    except Exception as ausnahme:
        fehler.merken('missionslog.bp_muster', ausnahme)
        bp_muster = None
    for pfad in pfade:
        gemeldet = _lesen(pfad, offen, fertig, gesehen, kennung, muster_an,
                          muster_aus, bp_muster)
        _verfallene_schliessen(offen, fertig, gemeldet, _spielzeit(pfad))
    return sorted(fertig + offen, key=lambda e: e.get('wann') or '',
                  reverse=True)


def _verfallene_schliessen(offen, fertig, gemeldet, sitzung):
    """Auftraege beenden, die eine spaetere Sitzung nicht mehr kennt.

    ⚠⚠ **Das ist die Obergrenze, die dem Protokoll gefehlt hat.** Ausloggen
    beendet keinen Auftrag (siehe `_lesen`) — aber irgendwann ist er trotzdem
    vorbei, und ohne diese Regel stand er fuer immer auf „laeuft". Gemessen am
    04.09.2026: **43** solcher Karteileichen, die aelteste vom 23.06., und sie
    richteten Folgeschaden an — ein scheinbar laufender Auftrag sammelt jeden
    spaeter gefundenen Bauplan ein (siehe `_bp_zuordnen`).

    Die Regel kommt aus dem Spiel selbst, nicht aus einer Zeitschaetzung:
    **Beim Einloggen meldet Star Citizen jeden noch laufenden Auftrag erneut
    als angenommen.** Wird ein Auftrag in einer spaeteren Sitzung also nicht
    mehr genannt, kann er dort nicht mehr offen gewesen sein. An 181 echten
    Sicherungen loeste das alle 43 Faelle auf, ohne einen einzigen Zweifelsfall.

    ⚠ **Eine stumme Sitzung beweist nichts.** Wer sich einloggt und ohne
    Auftrag herumfliegt (oder wessen Log nach einem Absturz abbricht), meldet
    gar nichts — daraus zu schliessen, alle Auftraege seien vorbei, waere
    falsch. Deshalb greift die Regel nur, wenn die Sitzung ueberhaupt einen
    Auftrag genannt hat.

    ⚠ Der Zustand heisst `VERFALLEN`, nicht `ABGEBROCHEN`: Ob der Auftrag
    abgegeben oder aufgegeben wurde, steht in keinem vorhandenen Log.
    """
    if not gemeldet or not offen:
        return
    for eintrag in list(offen):
        if eintrag['name'] in gemeldet:
            continue
        # ⚠ Nur was VOR dieser Sitzung begann. Ein Auftrag, der in genau
        # dieser Sitzung angenommen wurde, steht ohnehin in `gemeldet` — und
        # ohne diese Grenze wuerde die Reihenfolge innerhalb einer Datei
        # zaehlen statt der Sitzungswechsel.
        if (eintrag.get('wann') or '') >= (sitzung or ''):
            continue
        eintrag['zustand'] = VERFALLEN
        offen.remove(eintrag)
        fertig.append(eintrag)


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


# --------------------------------------------------------------- Fortschreiben
#
# ⚠⚠ **Das Protokoll lebt laenger als die Logs.** Star Citizen behaelt nur eine
# Handvoll `logbackups`, und auch die Sicherung auf die NAS haelt eine feste
# Zahl. Wer das Protokoll bei jedem Start allein aus den Logs baut, verliert
# jeden Auftrag, dessen Log inzwischen weggeraeumt wurde — genau die Rueckschau,
# um die es hier geht. Deshalb wird die Datei **fortgeschrieben**, so wie der
# Bauplan-Bestand auch.


def pfad():
    return pfade.app_datei(DATEI)


def laden():
    """Das gespeicherte Protokoll — oder eine leere Liste."""
    try:
        with open(pfad(), encoding='utf-8') as f:
            daten = json.load(f)
        if daten.get('format') == FORMAT:
            return _titel_nachputzen(daten.get('auftraege') or [])
    except Exception:
        pass
    return []


def _titel_nachputzen(eintraege):
    """Marken aus Titeln holen, die vor dem Putz-Fix gespeichert wurden.

    ⚠ **Ohne das bliebe der Fix unsichtbar.** Die Titel werden beim Lesen
    geputzt (`_lesen`), nicht beim Anzeigen — was einmal mit Marke im Protokoll
    steht, behaelt sie. Und neu gelesen wird eine Logdatei nie wieder: Der
    Lesestand merkt sie sich (siehe `nachlese`). Ein Protokoll, das vor dem Fix
    entstand, zeigte die Marken also dauerhaft weiter.

    Laeuft bei jedem Laden, macht aber nur beim ersten Mal Arbeit — danach
    findet sie nichts mehr und gibt die Liste unveraendert zurueck.
    """
    geputzt, veraendert = [], False
    for e in eintraege:
        name = e.get('name') or ''
        rein = auftraege.sauber(name)
        if rein and rein != name:
            e = dict(e, name=rein)
            veraendert = True
        geputzt.append(e)
    if not veraendert:
        return eintraege
    # ⚠ Ueber `zusammenfuehren`, nicht roh zurueck: Zwei Eintraege koennen nach
    # dem Putzen denselben Schluessel tragen (gleicher Auftrag, einmal mit und
    # einmal ohne Marke). Sie gehoeren dann zusammen — und ein abgeschlossener
    # darf dabei nicht auf „laeuft" zurueckfallen.
    return zusammenfuehren(geputzt, [])


def sichern(eintraege):
    """Das Protokoll schreiben. Meldet einen Fehlschlag, statt ihn zu schlucken.

    ⚠ `pfade.json_sichern` legt die Vorgaengerfassung (`.bak.json`) an. Ein
    Protokoll laesst sich nicht neu aufbauen, sobald die Logs fort sind — hier
    waere ein leer geschriebener Stand endgueltig.
    """
    try:
        return pfade.json_sichern(pfad(), {'format': FORMAT,
                                           'auftraege': eintraege})
    except Exception as ausnahme:
        fehler.merken('missionslog.sichern', ausnahme)
        return False


def _schluessel(e):
    """Was einen Auftragsdurchlauf eindeutig macht: Name plus Startzeitpunkt."""
    return ((e.get('name') or ''), (e.get('wann') or ''))


def zusammenfuehren(alt, neu):
    """Gespeichertes und frisch Gelesenes vereinen — ohne etwas zu verlieren.

    ⚠ **Der neue Stand gewinnt nur, wenn er mehr weiss.** Ein Auftrag, der
    gespeichert schon „abgeschlossen" ist, darf nicht wieder auf „laeuft"
    zurueckfallen, bloss weil in einem noch vorhandenen Log nur sein Anfang
    steht. Umgekehrt soll ein Ende, das erst jetzt im Log auftaucht, den alten
    Eintrag ergaenzen.
    """
    zusammen = {}
    for e in list(alt) + list(neu):
        s = _schluessel(e)
        vorher = zusammen.get(s)
        if vorher is None:
            zusammen[s] = dict(e)
            continue
        # Ein beendeter Zustand sticht „laeuft" — egal aus welcher Quelle.
        if vorher.get('zustand') == LAEUFT and e.get('zustand') != LAEUFT:
            vorher.update({k: v for k, v in e.items() if v not in (None, '')})
        elif e.get('zustand') == LAEUFT:
            # Nur fehlende Felder auffuellen, den Zustand nicht anfassen.
            for k, v in e.items():
                if k != 'zustand' and not vorher.get(k) and v:
                    vorher[k] = v
        else:
            vorher.update({k: v for k, v in e.items() if v not in (None, '')})
    return sorted(zusammen.values(), key=lambda e: e.get('wann') or '',
                  reverse=True)


def nachtragen(ordner=None, laufende=None):
    """Logs lesen, ins gespeicherte Protokoll einpflegen, sichern.

    Gibt `(gesamt, neu_dazugekommen)` zurueck.
    """
    alt = laden()
    neu = aus_ordner(ordner, laufende) if (ordner or laufende) else []
    if not neu:
        return len(alt), 0
    bekannt = {_schluessel(e) for e in alt}
    zusammen = zusammenfuehren(alt, neu)
    dazu = sum(1 for e in zusammen if _schluessel(e) not in bekannt)
    sichern(zusammen)
    return len(zusammen), dazu


def nachlese():
    """Beim Start: die aufgehobenen Logs des Spielers durchsehen.

    Genau wie beim Bauplan-Bestand — wer den Watcher zum ersten Mal startet,
    findet sein Protokoll **gefuellt** vor statt leer, denn seine `logbackups/`
    reichen ja Wochen zurueck.

    ⚠ **Nur einmal je Logdatei.** Die Sicherungen sind zusammen leicht ein
    halbes Gigabyte; sie bei jedem Start komplett neu zu lesen, wuerde den Start
    spuerbar bremsen — und seit die Sicherung auf der NAS 100 statt 10 Dateien
    aufhebt, waere es noch mehr. Gemerkt wird Name und Groesse: Waechst eine
    Datei (die laufende `Game.log` tut das staendig), wird sie erneut gelesen.
    Dubletten entstehen dabei nicht, dafuer sorgt `zusammenfuehren()`.
    """
    try:
        sicherungen = list(pfade.log_sicherungen() or [])
    except Exception as ausnahme:
        fehler.merken('missionslog.nachlese', ausnahme)
        return 0, 0

    laufende = None
    spiel = pfade.spiel_ordner()
    if spiel:
        kandidat = os.path.join(spiel, 'Game.log')
        if os.path.isfile(kandidat):
            laufende = kandidat

    gelesen = _gelesene_laden()
    offen_dateien = []
    for pfad_log in sicherungen + ([laufende] if laufende else []):
        try:
            marke = '%d' % os.path.getsize(pfad_log)
        except OSError:
            continue
        if gelesen.get(os.path.basename(pfad_log)) != marke:
            offen_dateien.append((pfad_log, marke))

    if not offen_dateien:
        return len(laden()), 0

    alt = laden()
    bekannt = {_schluessel(e) for e in alt}
    # ⚠ Chronologisch, sonst bekommt ein Auftrag das Ende eines fremden
    # Durchlaufs — siehe `_spielzeit`.
    neu = aus_dateien(sorted((p for p, _m in offen_dateien), key=_spielzeit))
    zusammen = zusammenfuehren(alt, neu)
    dazu = sum(1 for e in zusammen if _schluessel(e) not in bekannt)
    if sichern(zusammen):
        for pfad_log, marke in offen_dateien:
            gelesen[os.path.basename(pfad_log)] = marke
        _gelesene_sichern(gelesen)
    return len(zusammen), dazu


def _gelesene_laden():
    """Welche Logs schon gelesen wurden — leer bei veraltetem Format.

    ⚠⚠ **Der Lesestand muss mit dem Format mitziehen.** Sonst passiert beim
    Formatwechsel das Schlimmste von beidem: `laden()` verwirft das alte
    Protokoll, der Lesestand haelt aber alle 181 Logs fuer erledigt — und der
    Nutzer steht vor einem **leeren** Protokoll ohne jede Fehlermeldung.
    """
    try:
        with open(pfad(), encoding='utf-8') as f:
            daten = json.load(f)
        if daten.get('format') != FORMAT:
            return {}
        return daten.get('gelesen') or {}
    except Exception:
        return {}


def _gelesene_sichern(gelesen):
    """Den Lesestand neben das Protokoll schreiben — in dieselbe Datei."""
    try:
        with open(pfad(), encoding='utf-8') as f:
            daten = json.load(f)
        daten['gelesen'] = gelesen
        pfade.json_sichern(pfad(), daten)
    except Exception as ausnahme:
        fehler.merken('missionslog.lesestand', ausnahme)


# ------------------------------------------------------------------- Ausgeben


def als_csv(eintraege=None):
    """Das Protokoll als Tabelle — oeffnet sich in jedem Tabellenprogramm.

    Dieselbe Bauform wie beim Handelslager: Semikolon als Trenner, damit
    deutsche Excel-Fassungen die Spalten von allein trennen.
    """
    eintraege = laden() if eintraege is None else eintraege
    zeilen = ['Auftrag;Angenommen;Beendet;Zustand;Ziele erledigt;Ziele gesamt']
    for e in eintraege:
        zeilen.append(';'.join((
            (e.get('name') or '').replace(';', ','),
            (e.get('wann') or '').replace('T', ' '),
            (e.get('bis') or '').replace('T', ' '),
            e.get('zustand') or '',
            str(e.get('ziele_fertig') or ''),
            str(e.get('ziele_gesamt') or ''))))
    return '\n'.join(zeilen) + '\n'


def als_json(eintraege=None):
    """Das Protokoll als JSON-Text — fuer die Sicherung neben den anderen Listen."""
    eintraege = laden() if eintraege is None else eintraege
    return json.dumps({'format': FORMAT, 'auftraege': eintraege},
                      ensure_ascii=False, indent=2) + '\n'
