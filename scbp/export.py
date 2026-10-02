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

Mehrere Formate, je eines pro Ziel — dazu `for_scmdb()` für **scmdb.net**.
(Eine Ausgabe für die Baupläne-DB von Star Citizen Deutsch gibt es nicht;
einlesen lässt sich deren Datei.) Die beiden Grundfälle:

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

> **Der Export lädt nichts hoch.** Er schreibt eine Datei, den Rest macht der
> Spieler. Daneben gibt es den direkten Abgleich mit dem KRT
> Profit Basetool (`basetool_sync`) — und dafür gilt der Satz: *Verse-Kit
> schickt nur dorthin, womit du es ausdrücklich verbunden hast — und nur die
> Bereiche, die du freigibst.* Ungefragt verschickt wird weiterhin nichts.
"""
import json
import os
import time

from . import collection as collection_file
from . import catalog as catalog_module
from . import paths

# ⚠⚠ Das Feld `werkzeug` in einer eigenen Exportdatei — daran erkennt
# `importer.detect()` unsere Dateien wieder.
#
# Geschrieben wird der NEUE Name, gelesen werden BEIDE: `importer.py` führt
# dazu `EIGENE_WERKZEUGNAMEN`. Wer hier etwas ändert, muss dort nachsehen —
# sonst sind entweder alte Exporte nicht mehr importierbar oder neue nicht.
#
# ⭐ Nicht auf den Rückfall `or 'bauplaene' in data` in `detect()` verlassen —
# der erkennt einen geänderten Namen nur zufällig.
TOOL_NAME = 'VerseKit'


def _iso(time_string):
    """`2026-08-24 07:57:59` (Ortszeit) -> `2026-08-24T05:57:59Z` oder None.

    Der Bestand hält die Zeit in lesbarer Form; das Basetool erwartet ISO 8601.
    Lässt sich der Wert nicht deuten, wird das Feld **weggelassen** — laut
    Format ist es optional, und ein erfundener Zeitpunkt wäre schlechter als
    gar keiner.

    ⚠⚠ **Der Bestand hält ORTSZEIT, das `Z` heißt UTC.** Nur ein `Z`
    anzuhängen läge um die Zeitzone daneben (im Sommer zwei Stunden). Es
    wird wirklich umgerechnet; der Import rechnet spiegelbildlich zurück
    (`importer._time_from`)."""
    if not time_string:
        return None
    try:
        stamp = time.mktime(time.strptime(str(time_string),
                                          '%Y-%m-%d %H:%M:%S'))
        return time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime(stamp))
    except (ValueError, TypeError, OverflowError):
        return None


def _unique_tags():
    """Bauplanname (klein) -> Tag — nur, wo der Name EINDEUTIG ist.

    Gilt für Basetool UND scmdb. ⚠⚠ Bei einem mehrdeutigen Namen darf nicht
    der erste Tag gewinnen: Das Basetool prüft den Tag **vor** dem Namen und
    springt bei einem Treffer sofort auf dieses Produkt (REQ-INV-019 im
    Basetool). Ein geratener Tag landete also sicher beim falschen Teil —
    etwa beim Kraftwerk der Idris statt dem der Reclaimer, die gleich heißen.
    Ohne Tag nimmt das Basetool den Namen: lieber das als ein sicherer
    Fehlgriff."""
    seen = {}
    try:
        from . import crafting
        for r in crafting.all_items():
            # ⚠ Über die Vergleichsform, nicht `lower()`: Sonst fallen
            # `7MA "Lorica"` (Bestand) und `7MA 'Lorica'` (scmdb) auseinander,
            # ebenso `(16 Schuss)` und `(16 cap)` bei Magazinen.
            base = paths.name_key(r.get('basis') or '')
            tag = (r.get('tag') or '').strip()
            if base and tag:
                seen.setdefault(base, set()).add(tag)
    except Exception:
        return {}
    return {name: next(iter(tags)) for name, tags in seen.items()
            if len(tags) == 1}


def for_basetool(collection=None, tags=None):
    """Die Struktur, die `profit-base.online` beim Import erwartet.

    ⭐ Mit `tag` (die DataForge-Kennung, `BP_CRAFT_…`), wo er eindeutig ist.
    Das Basetool ordnet darüber genauer zu als über den Namen und fällt ohne
    Treffer auf den Namen zurück — ein fehlender Tag schadet nie."""
    data = collection if collection is not None else collection_file.load()
    table = _unique_tags() if tags is None else tags
    entries = []
    for key, e in sorted((data.get('bauplaene') or {}).items()):
        name = (e.get('name') or '').strip()
        if not name:
            continue                     # leere Namen fliegen beim Import raus
        entry = {'productName': name}
        tag = table.get(paths.name_key(name))
        if tag:
            entry['tag'] = tag
        time_text = _iso(e.get('zeit'))
        if time_text:
            entry['receivedAt'] = time_text
        entries.append(entry)
    return {'blueprints': entries}


SCMDB_URL = 'https://scmdb.net/?page=fab&fab=%s'


def for_scmdb(collection=None, version='', tags=None):
    """Die Struktur, die der Import von **scmdb.net** erwartet.

    ⛔⛔ **Maßgeblich ist, was scmdb EINLIEST — nicht, was es ausgibt.**
    scmdb hat genau einen Import (`Import Watcher History`), und der weist
    jede Datei ab, der `exportSchemaVersion` oder die Liste `missions` fehlt
    (`Unrecognized format`). Seine eigene Ausfuhr (`version: 3`, heute 4)
    kann scmdb gar nicht wieder einlesen. Abgelesen am Quelltext der Seite:
    je Bauplan zählt `tag` (exakter Treffer), sonst `productName`
    (Namensvergleich); `missions` darf leer sein. Ein `channel` wird bewusst
    NICHT gesetzt — jeder Wert außer LIVE löst dort eine Warnung aus.

    Die Felder `tag`, `name`, `url` des älteren scmdb-Formats stehen weiter
    mit drin, damit unser eigener Import und Menschen, die hineinsehen,
    etwas davon haben.

    ⚠⚠ **Der Tag ist der Schlüssel, nicht der Name** — und er wird über die
    Vergleichsform gesucht (`_unique_tags`), wie beim Basetool. Ein
    wörtlicher Vergleich fände `(16 Schuss)` nicht unter `(16 cap)`,
    `"Lorica"` nicht unter `'Lorica'`, ein geschütztes Leerzeichen im
    Oracle-Helm nicht unter dem normalen. scmdb riete dann über den Namen —
    und schlüge für ein Magazin **die Waffe** vor. Ein mehrdeutiger Name
    bekommt bewusst keinen Tag: Dann fragt scmdb nach, statt still falsch
    zuzuordnen. Ohne Tag geht der Bauplan trotzdem mit.

    `missions` bleibt leer. Gezählt würden dort nur Aufträge mit
    `debugName` aus den Spieldaten, die wir nicht zuverlässig haben und
    nicht erfinden."""
    data = collection if collection is not None else collection_file.load()
    table = _unique_tags() if tags is None else tags
    entries = []
    for key, e in sorted((data.get('bauplaene') or {}).items()):
        name = (e.get('name') or '').strip()
        if not name:
            continue
        entry = {'tag': table.get(paths.name_key(name), ''),
                 'productName': name, 'name': name}
        if entry['tag']:
            entry['url'] = SCMDB_URL % entry['tag']
        entries.append(entry)
    return {
        'exportSchemaVersion': 1,
        # scmdb zeigt `Watcher v<Wert>` — also nur die Nummer.
        'productName': TOOL_NAME,
        'watcherVersion': _tool_version(),
        'exportedAt': time.strftime('%Y-%m-%dT%H:%M:%S.000Z', time.gmtime()),
        'missions': [],
        'blueprints': entries,
    }


def _tool_version():
    """Die laufende Fassung, für die Anzeige „Watcher v…" bei scmdb.

    `errors.VERSION` setzt der Programmstart. Lokal importiert: `errors`
    importiert selbst `paths`, auf Modulebene wäre das ein Zirkelbezug."""
    try:
        from . import errors
        return errors.VERSION[0] or ''
    except Exception:
        return ''


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
        # heraus — sonst gaelte es als leerer Bestand und wuerde nie
        # geschrieben.
        if kind == 'auftraege':
            from . import mission_log
            entries = mission_log.load()
            if not entries:
                # ⚠ Knapp wie `leerer Bestand` unten, kein ganzer Satz: Diese
                # Rueckmeldungen gehen ins Protokoll, nicht auf die Seite.
                return False, 'leeres Protokoll'
            folder = os.path.dirname(os.path.abspath(path))
            if folder:
                os.makedirs(folder, exist_ok=True)
            with open(path + '.tmp', 'w', encoding='utf-8', newline='\n') as f:
                f.write(mission_log.as_json(entries))
            os.replace(path + '.tmp', path)
            return True, str(len(entries))

        if kind == 'basetool':
            doc = for_basetool(collection)
        elif kind == 'scmdb':
            doc = for_scmdb(collection, version)
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
    except OSError as err:
        return False, str(err)


FILENAMES = {
    'basetool': 'SC-Blueprints-Basetool-%s.json',
    'scmdb':    'scmdb-import-%s.json',
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
        # `…-%s.json` → `….json`, ohne den Bindestrich davor stehen zu lassen.
        return name.replace('-%s', '').replace('%s', '')
    return name % time.strftime('%Y-%m-%d')


def archive_folder():
    """Wohin die Ablage schreibt. Eigener Ordner neben den übrigen Dateien.

    Ein fester Ort statt jedes Mal ein Dateidialog: Wer den Bestand regelmäßig
    hochlädt, will nicht dreimal durch einen Speichern-Dialog klicken. Der
    Dialog bleibt für den Einzelfall daneben bestehen."""
    from . import paths
    own = paths.setting('export_ordner')
    folder = own or os.path.join(paths.app_folder(), 'export')
    try:
        os.makedirs(folder, exist_ok=True)
    except OSError:
        pass
    return folder


OLD_FOLDER = 'Ältere'


def _tidy_old_files(folder):
    """Früher abgelegte Dateien **mit Datum** in einen Unterordner schieben.

    ⚠ Ältere Fassungen legten jede Datei mit dem Tag im Namen ab. Wer die
    Ablage lange benutzt hat, hat dort dreistellig viele Dateien liegen —
    neben den drei Dateien mit festem Namen wäre nicht mehr zu erkennen,
    welche die aktuelle ist.

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
        from . import errors
        errors.record('export._tidy_old_files', exception, folder)


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
    for kind in ('basetool', 'scmdb', 'voll', 'auftraege'):
        target = os.path.join(folder, suggestion(kind, with_date=False))
        ok, _message = write(target, kind, collection, catalog, version)
        if ok:
            written.append(os.path.basename(target))
    return bool(written), folder, written
