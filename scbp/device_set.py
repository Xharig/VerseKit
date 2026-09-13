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

FILE = 'joystick-saetze.json'

# Was in einem Namen nichts zu suchen hat. Er wird nur angezeigt, nicht zu
# einem Dateinamen — deshalb reicht es, Steuerzeichen und Übermaß abzuwehren.
NAME_MAX = 40


def _load():
    # ⚠ `pfade` hat ein `json_sichern`, aber kein Gegenstück zum Lesen —
    # deshalb hier von Hand. Eine fehlende oder kaputte Datei ist kein Grund
    # abzustürzen: Dann gibt es eben noch keine Sätze.
    data = None
    try:
        gone = pfade.app_datei(FILE)
        if os.path.isfile(gone):
            with open(gone, 'r', encoding='utf-8') as f:
                data = json.load(f)
    except Exception:
        data = None
    if not isinstance(data, dict):
        return {'saetze': {}}
    if not isinstance(data.get('saetze'), dict):
        data['saetze'] = {}
    return data


def _save(data):
    try:
        return bool(pfade.json_sichern(pfade.app_datei(FILE), data))
    except Exception as ausnahme:
        from . import fehler
        fehler.merken('device_set.save', ausnahme)
        return False


def check_name(name):
    """Ist der Name brauchbar? Liefert `(ok, Sprachschluessel)`."""
    name = (name or '').strip()
    if not name:
        return False, 's_gs_f_name_leer'
    if len(name) > NAME_MAX:
        return False, 's_gs_f_name_lang'
    if any(ord(z) < 32 for z in name):
        return False, 's_gs_f_name_zeichen'
    return True, ''


def sets():
    """Alle gespeicherten Sätze, neueste zuerst."""
    data = _load()
    out = []
    for name, entry_set in data['saetze'].items():
        entry = dict(entry_set)
        entry['name'] = name
        out.append(entry)
    out.sort(key=lambda s: s.get('stand', ''), reverse=True)
    return out


def entry_set(name):
    """Ein einzelner Satz — oder `None`."""
    return _load()['saetze'].get((name or '').strip())


def capture(filename=None, folder=None):
    """Den aktuellen Stand einsammeln, ohne ihn zu speichern.

    Getrennt vom Speichern, damit die Oberfläche vorher zeigen kann, was in
    den Satz wandert — und damit sich der Stand mit einem gespeicherten
    vergleichen lässt.
    """
    blocks = kurven.geraete_achsen(filename, folder)
    game = kurven.spielachsen(filename, folder)

    devices = {}
    for block in blocks:
        if not block['kennung'] or not block['aktiv']:
            # Karteileichen gehören nicht in einen Satz — sie würden beim
            # nächsten Anwenden wieder auferstehen.
            continue
        axes = {}
        for axis, values in block['achsen'].items():
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
            axes[axis] = {k: values.get(k) for k in kurven.EIGENSCHAFTEN}
        devices[block['kennung']] = {'name': block['name'], 'achsen': axes}

    numbers = {}
    game_axes = {}
    for entry in game:
        if entry['art'] != 'joystick' or not entry['kennung']:
            continue
        numbers[entry['kennung']] = entry['nummer']
        values = {}
        for axis, props in entry['achsen'].items():
            is_set = {k: v for k, v in props.items()
                       if k in ('exponent', 'invert') and v is not None}
            if is_set:
                values[axis] = is_set
        if values:
            game_axes[entry['kennung']] = values

    return {'stand': time.strftime('%Y-%m-%d %H:%M'),
            'geraete': devices, 'nummern': numbers,
            'spielachsen': game_axes}


def save(name, overwrite=False, filename=None, folder=None):
    """Den aktuellen Stand unter einem Namen ablegen.

    Liefert `(erfolg, meldung, anzahl Geräte)`.
    """
    ok, message = check_name(name)
    if not ok:
        return False, message, 0
    name = name.strip()

    data = _load()
    if name in data['saetze'] and not overwrite:
        return False, 's_gs_f_name_belegt', 0

    fresh = capture(filename, folder)
    if not fresh['geraete']:
        return False, 's_gs_f_nichts', 0

    data['saetze'][name] = fresh
    if not _save(data):
        return False, 's_gs_f_schreiben', 0
    return True, '', len(fresh['geraete'])


def delete(name):
    """Einen Satz entfernen."""
    name = (name or '').strip()
    data = _load()
    if name not in data['saetze']:
        return False, 's_gs_f_unbekannt', 0
    del data['saetze'][name]
    if not _save(data):
        return False, 's_gs_f_schreiben', 0
    return True, '', 1


def preview(name, filename=None, folder=None):
    """Was würde das Anwenden tun? Liefert `(schreibt, fehlt)`.

    `schreibt` ist eine Liste von `(Gerätename, Achse, Eigenschaft, Wert)`,
    `fehlt` eine Liste von Gerätenamen, die im Satz stehen, aber gerade nicht
    da sind.

    ⭐ **Ohne Vorschau kein Knopf.** Wer eine Datei anfasst, an der die
    komplette Steuerung hängt, soll vorher sehen, was passiert — besonders
    hier, wo ein Satz ein Dutzend Werte auf einmal schreibt.
    """
    stored = entry_set(name)
    if not stored:
        return [], []

    present = {}
    for block in kurven.geraete_achsen(filename, folder):
        if block['kennung'] and block['aktiv']:
            present[block['kennung']] = block

    writes = []
    missing = []
    for ident, entry in (stored.get('geraete') or {}).items():
        target = present.get(ident)
        if target is None:
            missing.append(entry.get('name') or ident[:8])
            continue
        for axis, values in (entry.get('achsen') or {}).items():
            for prop, value in values.items():
                if prop not in kurven.EIGENSCHAFTEN:
                    continue
                now = (target['achsen'].get(axis) or {}).get(prop)
                # ⚠ `wert is None` heißt „löschen" und ist damit ebenfalls
                # eine Änderung, wenn gerade etwas dasteht.
                if now != value:
                    writes.append((entry.get('name') or '', axis,
                                     prop, value))
    return writes, missing


def apply(name, filename=None, folder=None):
    """Einen Satz auf die Belegungsdatei schreiben.

    Liefert `(erfolg, meldung, anzahl geschriebener Werte)`. Fehlende Geräte
    sind **kein** Fehler — sie werden übersprungen; genau dafür gibt es Sätze.
    """
    stored = entry_set(name)
    if not stored:
        return False, 's_gs_f_unbekannt', 0

    # ⚠⚠ **Nur schreiben, was sich unterscheidet.**
    #
    # `kurven.setzen()` legt bei JEDEM Aufruf eine Sicherung der
    # `actionmaps.xml` an — richtig so, an ihr hängt die ganze Steuerung. Ein
    # Satz mit drei Geräten würde aber blind 36 Werte schreiben und damit 36
    # Sicherungsdateien hinterlassen, für meist zwei echte Änderungen. Der
    # Vergleich vorweg kostet nichts und macht aus 36 Schreibvorgängen zwei.
    now = {}
    for block in kurven.geraete_achsen(filename, folder):
        if block['kennung'] and block['aktiv']:
            now[block['kennung']] = block

    count = 0
    for ident, entry in (stored.get('geraete') or {}).items():
        target = now.get(ident)
        if target is None:
            continue
        for axis, values in (entry.get('achsen') or {}).items():
            for prop, value in values.items():
                if prop not in kurven.EIGENSCHAFTEN:
                    continue
                ist = (target['achsen'].get(axis) or {}).get(prop)
                if ist == value:
                    continue
                ok_state, message, _ = kurven.setzen(
                    ident, axis, prop, value, filename, folder)
                if not ok_state:
                    return False, message, count
                count += 1
    present = set(now)

    # Die Spielachsen zuletzt — sie hängen an der Nummer, nicht an der
    # Kennung, und sollen nicht schreiben, wenn schon die Achsen scheiterten.
    for ident, axes in (stored.get('spielachsen') or {}).items():
        if ident not in present:
            continue
        number = (stored.get('nummern') or {}).get(ident)
        if not number:
            continue
        for axis, values in axes.items():
            for prop, value in values.items():
                if prop not in kurven.SPIEL_EIGENSCHAFTEN:
                    continue
                ok_state, message, _ = kurven.spiel_setzen(
                    number, axis, prop, value, filename, folder)
                if not ok_state:
                    return False, message, count
                count += 1

    if not count:
        return False, 's_gs_f_nichts_zu_tun', 0
    return True, '', count
