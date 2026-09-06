# SPDX-License-Identifier: GPL-3.0-only
#
# SC BP Watcher — Geraetesaetze: eine Einrichtung unter einem Namen
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
„Mit Pedalen" und „ohne Pedale" — zwei Namen statt zwölf Zahlen.

## Was ein Satz enthält — und was nicht

| Enthalten | Nicht enthalten |
|---|---|
| welche Kennung welche Nummer hat (`js1`, `js2`, …) | die Tastenbelegung selbst |
| Totzone und Sättigung je Achse | welche Taste welche Aktion auslöst |
| Exponent und Invertierung je Spielachse | |

⚠⚠ **Die Belegung gehört bewusst NICHT dazu.** Star Citizen kann das selbst:
Es speichert komplette Profile in `controls/mappings`, und der Watcher legt
sie über „Als Profil sichern" dort ab, wo das Spiel sie findet
(`pp_rebindkeys load <Name>`). Beides zu speichern hieße, zwei Wahrheiten über
dieselbe Sache zu führen — und beim nächsten Mal wüsste niemand mehr, welche
gilt. Ein Gerätesatz beantwortet die andere Frage: **wie** die Achsen
reagieren, nicht **was** auf ihnen liegt.

## ⚠ Beim Anwenden wird nur geschrieben, was auch da ist

Der Sinn der Sache sind wechselnde Aufbauten — mal mit Pedalen, mal ohne.
Fehlt ein Gerät, wird sein Teil des Satzes **übersprungen** und gemeldet,
statt die Einstellung auf ein anderes Gerät zu schreiben. Ein Wert am
falschen Stick ist schlimmer als ein fehlender.
"""
import json
import os
import time

from . import kurven, pfade

DATEI = 'joystick-saetze.json'

# Was in einem Namen nichts zu suchen hat. Er wird nur angezeigt, nicht zu
# einem Dateinamen — deshalb reicht es, Steuerzeichen und Übermaß abzuwehren.
NAME_MAX = 40


def _laden():
    # ⚠ `pfade` hat ein `json_sichern`, aber kein Gegenstück zum Lesen —
    # deshalb hier von Hand. Eine fehlende oder kaputte Datei ist kein Grund
    # abzustürzen: Dann gibt es eben noch keine Sätze.
    daten = None
    try:
        weg = pfade.app_datei(DATEI)
        if os.path.isfile(weg):
            with open(weg, 'r', encoding='utf-8') as f:
                daten = json.load(f)
    except Exception:
        daten = None
    if not isinstance(daten, dict):
        return {'saetze': {}}
    if not isinstance(daten.get('saetze'), dict):
        daten['saetze'] = {}
    return daten


def _sichern(daten):
    try:
        return bool(pfade.json_sichern(pfade.app_datei(DATEI), daten))
    except Exception as ausnahme:
        from . import fehler
        fehler.merken('geraetesatz.sichern', ausnahme)
        return False


def name_pruefen(name):
    """Ist der Name brauchbar? Liefert `(ok, Sprachschluessel)`."""
    name = (name or '').strip()
    if not name:
        return False, 's_gs_f_name_leer'
    if len(name) > NAME_MAX:
        return False, 's_gs_f_name_lang'
    if any(ord(z) < 32 for z in name):
        return False, 's_gs_f_name_zeichen'
    return True, ''


def saetze():
    """Alle gespeicherten Sätze, neueste zuerst."""
    daten = _laden()
    heraus = []
    for name, satz in daten['saetze'].items():
        eintrag = dict(satz)
        eintrag['name'] = name
        heraus.append(eintrag)
    heraus.sort(key=lambda s: s.get('stand', ''), reverse=True)
    return heraus


def satz(name):
    """Ein einzelner Satz — oder `None`."""
    return _laden()['saetze'].get((name or '').strip())


def aufnehmen(datei=None, ordner=None):
    """Den aktuellen Stand einsammeln, ohne ihn zu speichern.

    Getrennt vom Speichern, damit die Oberfläche vorher zeigen kann, was in
    den Satz wandert — und damit sich der Stand mit einem gespeicherten
    vergleichen lässt.
    """
    bloecke = kurven.geraete_achsen(datei, ordner)
    spiel = kurven.spielachsen(datei, ordner)

    geraete = {}
    for block in bloecke:
        if not block['kennung'] or not block['aktiv']:
            # Karteileichen gehören nicht in einen Satz — sie würden beim
            # nächsten Anwenden wieder auferstehen.
            continue
        achsen = {}
        for achse, werte in block['achsen'].items():
            # ⚠⚠ **Auch was NICHT gesetzt ist, gehört in den Satz.**
            #
            # Der erste Entwurf speicherte nur belegte Werte. Ein Satz war
            # damit keine Zustandsbeschreibung, sondern eine Ergänzungsliste:
            # Wer für „mit Pedalen" eine Sättigung setzte und danach „ohne
            # Pedale" anwandte, behielt sie — der Satz kannte das Feld ja gar
            # nicht und ließ es in Ruhe. Beim Umschalten sammelten sich so
            # Werte an, die in keinem Satz standen.
            #
            # Ein `None` heißt beim Anwenden **löschen**. Dieselbe Regel wie
            # beim Angleichen zweier Sticks: Sonst sind zwei Zustände, die
            # gleich heißen, eben nicht gleich.
            achsen[achse] = {k: werte.get(k) for k in kurven.EIGENSCHAFTEN}
        geraete[block['kennung']] = {'name': block['name'], 'achsen': achsen}

    nummern = {}
    spielachsen = {}
    for eintrag in spiel:
        if eintrag['art'] != 'joystick' or not eintrag['kennung']:
            continue
        nummern[eintrag['kennung']] = eintrag['nummer']
        werte = {}
        for achse, eigenschaften in eintrag['achsen'].items():
            gesetzt = {k: v for k, v in eigenschaften.items()
                       if k in ('exponent', 'invert') and v is not None}
            if gesetzt:
                werte[achse] = gesetzt
        if werte:
            spielachsen[eintrag['kennung']] = werte

    return {'stand': time.strftime('%Y-%m-%d %H:%M'),
            'geraete': geraete, 'nummern': nummern,
            'spielachsen': spielachsen}


def speichern(name, ueberschreiben=False, datei=None, ordner=None):
    """Den aktuellen Stand unter einem Namen ablegen.

    Liefert `(erfolg, meldung, anzahl Geräte)`.
    """
    ok, meldung = name_pruefen(name)
    if not ok:
        return False, meldung, 0
    name = name.strip()

    daten = _laden()
    if name in daten['saetze'] and not ueberschreiben:
        return False, 's_gs_f_name_belegt', 0

    neu = aufnehmen(datei, ordner)
    if not neu['geraete']:
        return False, 's_gs_f_nichts', 0

    daten['saetze'][name] = neu
    if not _sichern(daten):
        return False, 's_gs_f_schreiben', 0
    return True, '', len(neu['geraete'])


def loeschen(name):
    """Einen Satz entfernen."""
    name = (name or '').strip()
    daten = _laden()
    if name not in daten['saetze']:
        return False, 's_gs_f_unbekannt', 0
    del daten['saetze'][name]
    if not _sichern(daten):
        return False, 's_gs_f_schreiben', 0
    return True, '', 1


def vorschau(name, datei=None, ordner=None):
    """Was würde das Anwenden tun? Liefert `(schreibt, fehlt)`.

    `schreibt` ist eine Liste von `(Gerätename, Achse, Eigenschaft, Wert)`,
    `fehlt` eine Liste von Gerätenamen, die im Satz stehen, aber gerade nicht
    da sind.

    ⭐ **Ohne Vorschau kein Knopf.** Wer eine Datei anfasst, an der die
    komplette Steuerung hängt, soll vorher sehen, was passiert — besonders
    hier, wo ein Satz ein Dutzend Werte auf einmal schreibt.
    """
    gespeichert = satz(name)
    if not gespeichert:
        return [], []

    vorhanden = {}
    for block in kurven.geraete_achsen(datei, ordner):
        if block['kennung'] and block['aktiv']:
            vorhanden[block['kennung']] = block

    schreibt = []
    fehlt = []
    for kennung, eintrag in (gespeichert.get('geraete') or {}).items():
        ziel = vorhanden.get(kennung)
        if ziel is None:
            fehlt.append(eintrag.get('name') or kennung[:8])
            continue
        for achse, werte in (eintrag.get('achsen') or {}).items():
            for eigenschaft, wert in werte.items():
                if eigenschaft not in kurven.EIGENSCHAFTEN:
                    continue
                jetzt = (ziel['achsen'].get(achse) or {}).get(eigenschaft)
                # ⚠ `wert is None` heißt „löschen" und ist damit ebenfalls
                # eine Änderung, wenn gerade etwas dasteht.
                if jetzt != wert:
                    schreibt.append((eintrag.get('name') or '', achse,
                                     eigenschaft, wert))
    return schreibt, fehlt


def anwenden(name, datei=None, ordner=None):
    """Einen Satz auf die Belegungsdatei schreiben.

    Liefert `(erfolg, meldung, anzahl geschriebener Werte)`. Fehlende Geräte
    sind **kein** Fehler — sie werden übersprungen; genau dafür gibt es Sätze.
    """
    gespeichert = satz(name)
    if not gespeichert:
        return False, 's_gs_f_unbekannt', 0

    # ⚠⚠ **Nur schreiben, was sich unterscheidet.**
    #
    # `kurven.setzen()` legt bei JEDEM Aufruf eine Sicherung der
    # `actionmaps.xml` an — richtig so, an ihr hängt die ganze Steuerung. Ein
    # Satz mit drei Geräten würde aber blind 36 Werte schreiben und damit 36
    # Sicherungsdateien hinterlassen, für meist zwei echte Änderungen. Der
    # Vergleich vorweg kostet nichts und macht aus 36 Schreibvorgängen zwei.
    jetzt = {}
    for block in kurven.geraete_achsen(datei, ordner):
        if block['kennung'] and block['aktiv']:
            jetzt[block['kennung']] = block

    anzahl = 0
    for kennung, eintrag in (gespeichert.get('geraete') or {}).items():
        ziel = jetzt.get(kennung)
        if ziel is None:
            continue
        for achse, werte in (eintrag.get('achsen') or {}).items():
            for eigenschaft, wert in werte.items():
                if eigenschaft not in kurven.EIGENSCHAFTEN:
                    continue
                ist = (ziel['achsen'].get(achse) or {}).get(eigenschaft)
                if ist == wert:
                    continue
                erfolg, meldung, _ = kurven.setzen(
                    kennung, achse, eigenschaft, wert, datei, ordner)
                if not erfolg:
                    return False, meldung, anzahl
                anzahl += 1
    vorhanden = set(jetzt)

    # Die Spielachsen zuletzt — sie hängen an der Nummer, nicht an der
    # Kennung, und sollen nicht schreiben, wenn schon die Achsen scheiterten.
    for kennung, achsen in (gespeichert.get('spielachsen') or {}).items():
        if kennung not in vorhanden:
            continue
        nummer = (gespeichert.get('nummern') or {}).get(kennung)
        if not nummer:
            continue
        for achse, werte in achsen.items():
            for eigenschaft, wert in werte.items():
                if eigenschaft not in kurven.SPIEL_EIGENSCHAFTEN:
                    continue
                erfolg, meldung, _ = kurven.spiel_setzen(
                    nummer, achse, eigenschaft, wert, datei, ordner)
                if not erfolg:
                    return False, meldung, anzahl
                anzahl += 1

    if not anzahl:
        return False, 's_gs_f_nichts_zu_tun', 0
    return True, '', anzahl
