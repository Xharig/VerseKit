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
Den eigenen Bauplan-Bestand als Datei ausgeben.

Mehrere Formate, je eines pro Ziel — dazu `for_scmdb()` für **scmdb.net** und
`for_bpdb()` für die **Baupläne DB** von Star Citizen Deutsch. Die beiden
Grundfälle:

**1. Für das KRT Profit Basetool** (`profit-base.online`) — dessen Import nimmt
eine JSON entgegen und gleicht sie in einer Vorschau gegen seinen Katalog ab:

    {"blueprints": [{"productName": "Manticore Helmet",
                     "receivedAt": "2026-08-02T01:49:03.322Z"}]}

`productName` ist Pflicht, `receivedAt` optional (ISO 8601). Ein kaputter
Zeitwert lässt den Import **nicht** scheitern — deshalb wird er weggelassen,
wenn er nicht sauber zu bilden ist, statt etwas Erfundenes zu schreiben.

**2. Als vollständige Sicherung** — alles, was hier bekannt ist: Name, Art,
Klasse, Größe, Gütegrad, Hersteller, Quelle und Zeitpunkt. Für eigene
Auswertungen und als Rückfall, unabhängig von jedem fremden Dienst.

> **Hochgeladen wird nichts.** Der Export schreibt eine Datei, den Rest macht
> der Spieler. Alles andere hieße fremde Zugangsdaten verwalten und ungefragt
> Daten verschicken — das gehört nicht in ein Overlay.
"""
import json
import os
import time

from . import collection as collection_file
from . import catalog as catalog_module

# ⚠⚠ Das Feld `werkzeug` in einer eigenen Exportdatei — daran erkennt
# `importer.detect()` unsere Dateien wieder.
#
# Geschrieben wird der NEUE Name, gelesen werden BEIDE: `importer.py` führt
# dazu `EIGENE_WERKZEUGNAMEN`. Wer hier etwas ändert, muss dort nachsehen —
# sonst sind entweder alte Exporte nicht mehr importierbar oder neue nicht.
#
# ⭐ Bis zum 12.09.2026 stand hier `'SC BP Watcher'` fest, und `detect()`
# verglich genauso fest. Dass ein neuer Name trotzdem erkannt würde, lag nur
# am Rückfall `or 'bauplaene' in data` — Zufall, kein Entwurf.
TOOL_NAME = 'VerseKit'


def _iso(time_string):
    """„2026-08-24 07:57:59" -> „2026-08-24T07:57:59Z" oder None.

    Der Bestand hält die Zeit in lesbarer Form; das Basetool erwartet ISO 8601.
    Lässt sich der Wert nicht deuten, wird das Feld **weggelassen** — laut
    Format ist es optional, und ein erfundener Zeitpunkt wäre schlechter als
    gar keiner."""
    if not time_string:
        return None
    try:
        t = time.strptime(str(time_string), '%Y-%m-%d %H:%M:%S')
        return time.strftime('%Y-%m-%dT%H:%M:%SZ', t)
    except (ValueError, TypeError):
        return None


def for_basetool(collection=None):
    """Die Struktur, die `profit-base.online` beim Import erwartet."""
    data = collection if collection is not None else collection_file.load()
    entries = []
    for key, e in sorted((data.get('bauplaene') or {}).items()):
        name = (e.get('name') or '').strip()
        if not name:
            continue                     # leere Namen fliegen beim Import raus
        entry = {'productName': name}
        time_text = _iso(e.get('zeit'))
        if time_text:
            entry['receivedAt'] = time_text
        entries.append(entry)
    return {'blueprints': entries}


def for_bpdb(collection=None):
    """Die Struktur, die die **Baupläne DB** von Star Citizen Deutsch einliest.

    Das ist die Bauplan-Übersicht im Browser
    (`rjcncpt.github.io/StarCitizen-Deutsch-INI/`) — **nicht** der SC Deutsch
    Launcher, der bleibt ein Programm. Ihr Import erwartet eine Liste
    `blueprints` mit `key` und einem Schalter je Eintrag:

        {"blueprints": [{"key": "Manticore Helmet",
                         "isDone": true, "isMarked": false}]}

    `isDone` heißt „habe ich", `isMarked` „will ich". Wir schreiben deshalb
    **nur** erspielte Baupläne, jeden mit `isDone: true` — alles andere wäre
    ein fremder Wunschzettel in ihrer Liste.

    ⚠ **Ohne diese Version kommt der eigene Bestand dort nicht hinein.** Die
    Seite prüft auf `blueprints`; die Vollsicherung führt `bauplaene`, die
    Basetool-Version `productName` und scmdb `tag`. Alle drei werden mit
    „Ungültiges Dateiformat" abgewiesen.

    ⚠ Einen Zeitpunkt gibt es in diesem Format nicht — er würde beim Import
    ohnehin verworfen, die Seite merkt sich nur „erspielt/vorgemerkt".

    Die Umschlagfelder (`exported`, `mode`, `total`, …) schreibt die Seite in
    ihre eigenen Ausfuhren. Für den Import braucht sie keines davon; sie
    stehen trotzdem drin, damit die Datei zwischen ihren eigenen nicht wie ein
    Fremdkörper aussieht — und damit ein Mensch sie später zuordnen kann.
    """
    data = collection if collection is not None else collection_file.load()
    entries = []
    for key, e in sorted((data.get('bauplaene') or {}).items()):
        name = (e.get('name') or '').strip()
        if not name:
            continue
        entries.append({'key': name, 'isDone': True, 'isMarked': False})
    return {
        'exported': time.strftime('%Y-%m-%dT%H:%M:%S.000Z', time.gmtime()),
        'mode': 'erspielt',
        'total': len(entries),
        'done_count': len(entries),
        'mark_count': 0,
        'blueprints': entries,
    }


SCMDB_URL = 'https://scmdb.net/?page=fab&fab=%s'


def _scmdb_tags():
    """Bauplanname (klein) -> Tag, aus den Rezeptdaten.

    Der Tag (`BP_CRAFT_AMRS_LaserCannon_S2`) ist bei scmdb der Schlüssel — der
    Name ist nur Beiwerk. `recipe()` gibt ihn nicht heraus, `all_items()` schon.

    ⚠ Liegen keine Rezeptdaten vor (frische Installation, kein Netz), ist die
    Zuordnung leer. Der Export läuft dann trotzdem, nur ohne Tags — er darf
    nicht am Netz hängen."""
    table = {}
    try:
        from . import crafting
        for r in crafting.all_items():
            tag = (r.get('tag') or '').strip()
            if not tag:
                continue
            for key in (r.get('name'), r.get('basis')):
                if key:
                    table.setdefault(key.strip().lower(), tag)
    except Exception:
        return {}
    return table


def for_scmdb(collection=None, version='', tags=None):
    """Die Struktur, die der Import von **scmdb.net** erwartet.

    Abgelesen an einer echten Exportdatei von scmdb.net (05.09.2026): ein
    Umschlag mit `version: 3`, darin `missions` und `blueprints` mit `tag`,
    `name`, `url`, `completed` und `favorite`.

    ⚠⚠ **Der Tag ist der Schlüssel, nicht der Name.** Gemessen an einem
    gewachsenen Bestand: 409 von 413 Bauplänen finden über die Rezeptdaten
    ihren Tag. Die vier übrigen sind deutsche Bezeichnungen ohne Gegenstück im
    Rezeptsatz; sie werden trotzdem mit ausgegeben, damit sie nicht
    stillschweigend verschwinden — ob scmdb sie ohne Tag zuordnen kann,
    entscheidet deren Import.

    ⚠ **Das Format hat gewechselt.** Bis v3.17.3 schrieb der Watcher
    `exportSchemaVersion: 1` mit `productName` und `ts` (Epochsekunden),
    abgelesen am `--export` ihres alten Log-Watchers v0.1.9. Diese Felder gibt
    es in Fassung 3 nicht mehr — auch den Zeitstempel nicht, was kein Verlust
    ist: Wer seinen Bestand aus der Launcher-Datei übernommen hatte, trug
    ohnehin für **alle** Einträge den Zeitpunkt des Imports.

    `missions` bleibt leer. Ihre Einträge tragen einen `hash` aus dem
    Auftragssystem von scmdb, den wir nicht haben und nicht erfinden."""
    data = collection if collection is not None else collection_file.load()
    table = _scmdb_tags() if tags is None else tags
    entries = []
    for key, e in sorted((data.get('bauplaene') or {}).items()):
        name = (e.get('name') or '').strip()
        if not name:
            continue
        entry = {'tag': table.get(name.lower(), ''), 'name': name}
        if entry['tag']:
            entry['url'] = SCMDB_URL % entry['tag']
        entry['completed'] = True
        entry['favorite'] = False
        entries.append(entry)
    return {
        'version': 3,
        'exportedAt': time.strftime('%Y-%m-%dT%H:%M:%S.000Z', time.gmtime()),
        'missions': [],
        'blueprints': entries,
    }


def complete(collection=None, catalog=None):
    """Alles, was das Werkzeug über den eigenen Bestand weiß."""
    data = collection if collection is not None else collection_file.load()
    cat = (catalog if catalog is not None else catalog_module.load())
    kb = cat.get('bauplaene') or {}
    entries = []
    for key, e in sorted((data.get('bauplaene') or {}).items()):
        k = kb.get(key) or {}
        entry = {
            'name': e.get('name'),
            'quelle': e.get('quelle'),
            'zeit': e.get('zeit'),
            'art': catalog_module.kind_readable(k.get('a')) if k.get('a') else None,
            'klasse': k.get('c'),
            'size': k.get('s'),
            'grade': k.get('g'),
            'hersteller': k.get('m'),
        }
        entries.append({kk: v for kk, v in entry.items() if v not in (None, '')})
    return {
        'werkzeug': TOOL_NAME,
        'erstellt': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
        'spielversion': cat.get('version') or None,
        'anzahl': len(entries),
        'bauplaene': entries,
    }


def write(path, kind='basetool', collection=None, catalog=None, version=''):
    """Eine Version in eine Datei schreiben. (Erfolg, Meldung)."""
    try:
        # ⚠ Das Auftrags-Protokoll ist kein Bauplan-Bestand: eigene Struktur,
        # eigene Leer-Pruefung. Es faellt deshalb VOR der gemeinsamen Zaehlung
        # heraus — sonst gaelte es als „leerer Bestand" und wuerde nie
        # geschrieben.
        if kind == 'auftraege':
            from . import missionslog
            entries = missionslog.laden()
            if not entries:
                # ⚠ Knapp wie „leerer Bestand" unten, kein ganzer Satz: Diese
                # Rueckmeldungen gehen ins Protokoll, nicht auf die Seite.
                return False, 'leeres Protokoll'
            folder = os.path.dirname(os.path.abspath(path))
            if folder:
                os.makedirs(folder, exist_ok=True)
            with open(path + '.tmp', 'w', encoding='utf-8', newline='\n') as f:
                f.write(missionslog.als_json(entries))
            os.replace(path + '.tmp', path)
            return True, str(len(entries))

        if kind == 'basetool':
            doc = for_basetool(collection)
        elif kind == 'scmdb':
            doc = for_scmdb(collection, version)
        elif kind == 'bpdb':
            doc = for_bpdb(collection)
        else:
            doc = complete(collection, catalog)
        count = len(doc.get('blueprints') or doc.get('bauplaene') or [])
        if not count:
            return False, 'leerer Bestand'
        folder = os.path.dirname(os.path.abspath(path))
        if folder:
            os.makedirs(folder, exist_ok=True)
        with open(path + '.tmp', 'w', encoding='utf-8', newline='\n') as f:
            json.dump(doc, f, ensure_ascii=False, indent=1)
        os.replace(path + '.tmp', path)
        return True, str(count)
    except OSError as fehler:
        return False, str(fehler)


FILENAMES = {
    'basetool': 'SC-Blueprints-Basetool-%s.json',
    'scmdb':    'scmdb-import-%s.json',
    # ⚠ Nicht `sc_bp_erledigt.json` — so heißt die Datei, die das
    # Launcher-Programm selbst schreibt und die der Watcher überwacht. Zwei
    # Dateien mit demselben Namen und entgegengesetzter Richtung sind eine
    # Verwechslung, die man erst merkt, wenn der Bestand falsch ist.
    'bpdb': 'bauplaene-db-import-%s.json',
    'voll':     'SC-BP-Watcher-Bestand-%s.json',
    'auftraege': 'SC-BP-Watcher-Auftraege-%s.json',
}


def suggestion(kind='basetool', with_date=True):
    """Ein sinnvoller Dateiname.

    ⚠ **Mit Datum nur im Speichern-Dialog.** Wer von Hand speichert, hält einen
    Stand fest — da gehört der Tag in den Namen. Die Ablage dagegen wird bei
    jedem neuen Bauplan mitgeschrieben; mit Datum entstünden dort **jeden Tag
    drei neue Dateien**, und wer eine hochladen will, müsste erst die richtige
    heraussuchen. Genau das Suchen sollte die Ablage abschaffen. Dort steht
    deshalb immer derselbe Name, und die Datei ist immer die aktuelle.
    """
    name = FILENAMES.get(kind, FILENAMES['voll'])
    if not with_date:
        # „…-%s.json" → „….json", ohne den Bindestrich davor stehen zu lassen.
        return name.replace('-%s', '').replace('%s', '')
    return name % time.strftime('%Y-%m-%d')


def archive_folder():
    """Wohin die Ablage schreibt. Eigener Ordner neben den übrigen Dateien.

    Ein fester Ort statt jedes Mal ein Dateidialog: Wer den Bestand regelmäßig
    hochlädt, will nicht dreimal durch einen Speichern-Dialog klicken. Der
    Dialog bleibt für den Einzelfall daneben bestehen."""
    from . import pfade
    own = pfade.einstellung('export_ordner')
    folder = own or os.path.join(pfade.app_ordner(), 'export')
    try:
        os.makedirs(folder, exist_ok=True)
    except OSError:
        pass
    return folder


OLD_FOLDER = 'Ältere'


def _tidy_old_files(folder):
    """Früher abgelegte Dateien **mit Datum** in einen Unterordner schieben.

    ⚠ Bis rc65 trug jede abgelegte Datei den Tag im Namen. Wer die Ablage ein
    halbes Jahr lang benutzt hat, hat dort dreistellig viele Dateien liegen —
    Gemeldet am 27.08.2026: „da liegen eh schon viele drin". Neben den drei
    Dateien mit festem Namen wäre nicht mehr zu erkennen, welche die aktuelle
    ist. Genau das Suchen sollte die Ablage abnehmen.

    ⚠ **Nichts wird gelöscht.** Verschoben wird in `Ältere/`, und nur, was zu
    einem unserer drei Namensmuster passt. Was jemand sonst in den Ordner gelegt
    hat, bleibt unangetastet — es ist sein Ordner, nicht unserer.
    """
    pattern = [(name.split('-%s')[0], name.split('%s')[-1])
              for name in FILENAMES.values()]
    moved = []
    try:
        present = os.listdir(folder)
    except OSError:
        return
    for file in present:
        full = os.path.join(folder, file)
        if not os.path.isfile(full):
            continue
        # Nur die alten, datierten Versionen: Anfang und Endung wie bei uns,
        # aber länger als der feste Name — das Datum steckt dazwischen.
        for start, extension in pattern:
            if (file.startswith(start) and file.endswith(extension)
                    and len(file) > len(start) + len(extension)):
                moved.append(file)
                break
    if not moved:
        return
    old = os.path.join(folder, OLD_FOLDER)
    try:
        os.makedirs(old, exist_ok=True)
        for file in moved:
            target = os.path.join(old, file)
            if os.path.exists(target):
                os.remove(os.path.join(folder, file))   # liegt dort schon
            else:
                os.replace(os.path.join(folder, file), target)
    except OSError as exception:
        from . import fehler
        fehler.merken('export._tidy_old_files', exception, folder)


def archive(collection=None, catalog=None, version=''):
    """**Alle** Versionen auf einmal in die Ablage schreiben.

    Gibt (Erfolg, Ordner, Liste der Dateien) zurück. Ein Fehler bei einer
    Version lässt die anderen nicht ausfallen — lieber zwei von drei Dateien
    als gar keine."""
    folder = archive_folder()
    _tidy_old_files(folder)
    written = []
    # ⚠ Das Auftrags-Protokoll gehoert mit in die Ablage: Es ist eine eigene
    # Liste wie der Bestand, und wer seine Daten sichert, meint alle. Fehlt es
    # hier, merkt das niemand — bis der Rechner neu aufgesetzt ist.
    for kind in ('basetool', 'scmdb', 'bpdb', 'voll', 'auftraege'):
        target = os.path.join(folder, suggestion(kind, with_date=False))
        ok, _message = write(target, kind, collection, catalog, version)
        if ok:
            written.append(os.path.basename(target))
    return bool(written), folder, written
