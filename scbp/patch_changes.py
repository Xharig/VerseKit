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
Was hat der Patch an den Werten geändert — dauerhaft festgehalten.

**Nicht zu verwechseln mit `patchhistory.py`.** Die beiden beantworten zwei
verschiedene Fragen und haben zwei verschiedene Quellen:

    patchhistory.py    welche BAUPLÄNE ein Patch gebracht hat
                        Quelle: eigene Beobachtung des Watchers
    patch_changes.py     welche WERTE ein Patch geändert hat
                        Quelle: erkul (fertige Diffs, CIG-Daten)

Sie liegen bewusst getrennt: andere Quelle, andere Lizenzlage, anderer
Lebenszyklus. Die Bauplan-Historie darf weitergegeben werden und liegt im Repo;
die Werte-Diffs stammen von erkul und bleiben beim Spieler.

⭐ **Warum überhaupt lokal ablegen — der Silberstreif.** Erkul hebt nur die
**letzten zehn** Patches auf (Stand 07.09.2026 zurück bis 01.07.2026). Wer sie
beim Spieler ablegt, dessen Historie **wächst über die von erkul hinaus** —
nach einem Jahr hat er etwas, das es sonst nirgends gibt. Genau deshalb wird
hier abgelegt statt jedes Mal frisch abgerufen.

**Aufbewahrung — entschieden am 06.09.2026:** *alles aufheben, die leeren
wegwerfen.* Von zehn Patches ändern nur zwei überhaupt Daten; die anderen acht
sind knapp 480 Byte reines „nichts passiert". Gemessen am 07.09.2026:

    4.10.0-LIVE.12519617   17 +   1 -   352 ~    34 KB gepackt
    4.9.0-LIVE.12232306    24 +   4 -   250 ~    20 KB gepackt
    die übrigen acht        0     0       0        je ~0,5 KB  → verworfen

Das sind grob 10 echte Patches im Jahr, also rund 2 MB jährlich. Keine
Stückzahl-Grenze, kein Aufräumen nötig.

⚠ **Eine Datei je Patch, nicht eine große.** Nach fünf Jahren stünden sonst
10 MB in einer einzigen JSON, die vollständig geladen werden müsste, nur um
einen einzelnen Patch anzuzeigen. Die Dateien liegen im Unterordner `Patches`
neben den anderen Ablagen.

⚠ **Der Zweig-Präfix `LIVE/` ist Pflicht.** `_holen('changelog.x.bin')` liefert
`null`; richtig ist `_holen('LIVE/changelog.x.bin')`. Erkul legt jede Datei
unter ihrem Zweig ab, und der Fehlerfall ist still — es kommt keine Meldung,
nur nichts.
"""
import json
import os
import re

from . import erkul, fehler, patchhistory, pfade

FOLDER = 'Patches'

# Die Zusammenfassung, die erkul je Patch im Inhaltsverzeichnis mitliefert.
# `unchanged` steht bewusst nicht dabei: Ein Patch, bei dem sich nichts geändert
# hat, führt trotzdem tausende unveränderte Einträge — das ist kein Inhalt.
COUNTER = ('added', 'removed', 'modified')


def _safe_name(version):
    """Dateiname aus einer Spielversion — ohne alles, was Ordner sprengt.

    ⚠ Die Version kommt aus dem Netz und landet als Dateiname auf der Platte.
    Ungeprüft wäre ein `../` darin ein Weg aus dem Ablageordner heraus. Erlaubt
    sind deshalb nur Ziffern, Buchstaben, Punkt und Bindestrich; alles andere
    wird zu `_`."""
    return re.sub(r'[^0-9A-Za-z.\-]', '_', version or 'unbekannt')


def _folder():
    """Der Ablageordner für die Patch-Dateien — angelegt, falls er fehlt.

    ⚠ Bei gesetztem `SC_BP_HOME` (Selbsttest, Wegwerf-Ordner) bleibt es flach,
    genau wie `pfade.app_datei()` es dort auch tut. Dort geht es um einen
    isolierten Ordner, nicht um Übersicht."""
    base = pfade.app_ordner()
    if os.environ.get('SC_BP_HOME'):
        return base
    target = os.path.join(base, FOLDER)
    try:
        os.makedirs(target, exist_ok=True)
    except OSError:
        return base
    return target


def _file(version):
    return os.path.join(_folder(), 'patch-%s.json' % _safe_name(version))


# --------------------------------------------------------------- Die Quelle
def _number(value):
    """`summary`-Werte kommen als Zahl, könnten aber auch fehlen."""
    try:
        return int(value or 0)
    except (TypeError, ValueError):
        return 0


def _has_content(summary):
    """Hat dieser Patch überhaupt etwas geändert?

    Das ist die Regel „die leeren wegwerfen" an genau einer Stelle. Sie wird
    zweimal gebraucht — beim Abholen und beim Anzeigen —, deshalb steht sie
    hier und nicht doppelt."""
    z = summary or {}
    return any(_number(z.get(k)) for k in COUNTER)


def offered():
    """Was erkul gerade vorhält: [{version, datum, summary, path, bytes}, …].

    Neueste zuerst. Ohne Netz eine leere Liste — wie überall im Werkzeug
    läuft es dann einfach ohne diese Angaben weiter."""
    cat = erkul.ship_catalog()
    if not cat:
        return []
    out = []
    for p in cat.get('patches') or []:
        version = p.get('dataVersion') or ''
        if not version:
            continue
        out.append({
            'version': version,
            'datum': (p.get('generatedAt') or '')[:10],
            'summary': p.get('summary') or {},
            'path': p.get('path') or '',
            'bytes': _number(p.get('bytes')),
        })
    out.sort(key=lambda e: patchhistory.rank(e['version']), reverse=True)
    return out


def _fetch(entry):
    """Die Änderungsliste eines Patches von erkul holen — oder `None`.

    ⚠ Der Zweig-Präfix ist Pflicht (siehe Modulkopf)."""
    path = entry.get('path')
    if not path:
        return None
    return erkul._fetch('%s/%s' % (erkul.BRANCH, path), 'changelog')


# --------------------------------------------------------------- Die Ablage
def stored():
    """Die Spielversionen, die hier schon liegen — neueste zuerst."""
    try:
        names = os.listdir(_folder())
    except OSError:
        return []
    versions = []
    for name in names:
        if name.startswith('patch-') and name.endswith('.json'):
            entry = _read_file(os.path.join(_folder(), name))
            if entry and entry.get('version'):
                versions.append(entry['version'])
    versions.sort(key=patchhistory.rank, reverse=True)
    return versions


def _read_file(path):
    try:
        with open(path, encoding='utf-8') as f:
            return json.load(f)
    except Exception:
        return None


def load(version):
    """Die abgelegte Änderungsliste einer Spielversion — oder `None`."""
    return _read_file(_file(version))


def _write(version, data):
    """Eine Patch-Datei ablegen. Erst daneben, dann umbenennen.

    ⚠ Ohne den Umweg über `.tmp` stünde bei einem Abbruch mitten im Schreiben
    eine halbe JSON-Datei da, die beim nächsten Lesen still als „kaputt" gilt —
    und der Patch wäre verloren, obwohl erkul ihn längst nicht mehr vorhält."""
    target = _file(version)
    try:
        temp = target + '.tmp'
        with open(temp, 'w', encoding='utf-8') as f:
            # ⚠ **Kompakt, ohne Einrückung** — anders als `patch-historie.json`.
            # Die liegt im Repo und soll lesbar sein; diese hier liest niemand
            # von Hand, sie hat vierstellige Einträge. Gemessen am 07.09.2026:
            # mit `indent=1` sind es 570 KB für zwei Patches, ohne 373 KB —
            # 35 % Aufschlag für Leerzeichen, die keiner sieht. Der Plan
            # rechnete mit den kleineren Zahlen.
            json.dump(data, f, ensure_ascii=False, separators=(',', ':'))
        os.replace(temp, target)
        return True
    except Exception as exception:
        fehler.merken('patch_changes._write', exception)
        return False


# ------------------------------------------------------------- Der Abgleich
def sync():
    """Neue Patches von erkul holen und ablegen. Gibt die neu abgelegten zurück.

    Der Ablauf in einem Satz: **was erkul anbietet, was hier noch fehlt, und
    davon nur das mit Inhalt.**

    ⚠ Leere Patches werden nicht abgelegt, aber auch **nicht erneut geholt** —
    sie stehen im Inhaltsverzeichnis mit ihrer Zusammenfassung, und die reicht
    zur Entscheidung. Ein Abruf pro leerem Patch wäre Verkehr, den erkul
    bezahlt und der niemandem nützt.

    Wirft nie: Ohne Netz kommt eine leere Liste zurück, und das Werkzeug läuft
    weiter wie vorher."""
    present = set(stored())
    new = []
    for entry in offered():
        version = entry['version']
        if version in present or not _has_content(entry['summary']):
            continue
        data = _fetch(entry)
        if not data:
            continue
        data['version'] = version
        data['datum'] = entry['datum']
        data['quelle'] = 'erkul.games'
        if _write(version, data):
            new.append(version)
    return new


# ------------------------------------------------------------- Aufbereitung
def overview():
    """Alles, was sich anzeigen lässt: [{version, kurz, datum, …}, …].

    **Vereinigt beide Seiten** — was erkul gerade anbietet und was hier liegt.
    Genau darin steckt der Gewinn der lokalen Ablage: Ein Patch, den erkul
    inzwischen fallen gelassen hat, steht hier weiter, und zwar mit
    `bei_erkul=False`. Wer nur eine der beiden Seiten liest, verliert ihn."""
    combined = {}
    for entry in offered():
        combined[entry['version']] = {
            'version': entry['version'],
            'kurz': entry['version'].split('-')[0],
            'datum': entry['datum'],
            'summary': entry['summary'],
            'leer': not _has_content(entry['summary']),
            'bei_erkul': True,
            'abgelegt': False,
        }
    for version in stored():
        data = load(version) or {}
        entry = combined.get(version)
        if entry is None:
            entry = {
                'version': version,
                'kurz': version.split('-')[0],
                'datum': data.get('datum') or '',
                'summary': data.get('summary') or {},
                'leer': False,
                'bei_erkul': False,
            }
            combined[version] = entry
        entry['abgelegt'] = True
    out = list(combined.values())
    out.sort(key=lambda e: patchhistory.rank(e['version']), reverse=True)
    return out


def _value(entry, key):
    """`oldValue`/`newValue` — beide dürfen fehlen, und das ist die Aussage.

    ⚠ **Fehlt einer, ist das kein Datenfehler, sondern der Inhalt.** Gemessen
    am 07.09.2026 an der C-788 Cannon: `weapon.ammo.impactRadius` hat nur einen
    `oldValue` — das Feld ist mit dem Patch **weggefallen**. Wer stumpf
    `eintrag['newValue']` liest, bekommt hier einen `KeyError` und reißt die
    ganze Anzeige mit. Zurück kommt deshalb `(wert, vorhanden)`."""
    return entry.get(key), key in entry


def changes(version, wanted=None):
    """Die Änderungen eines Patches, flach und anzeigefertig.

    Liefert je Eintrag: Kategorie, was passiert ist (`neu`/`weg`/`geaendert`),
    Name, Größe und die einzelnen Feldänderungen mit altem und neuem Wert.
    Mit `wanted` lässt sich auf eine Kategorie einschränken (`'weapons'` …)."""
    data = load(version)
    if not data:
        return []
    out = []
    for category in data.get('categories') or []:
        kind = category.get('kind') or ''
        if wanted and kind != wanted:
            continue
        for state, key in (('neu', 'added'), ('weg', 'removed'),
                                    ('geaendert', 'modified')):
            # ⚠ `unchanged` ist eine ZAHL, die anderen drei sind LISTEN — im
            # selben Feld derselben Datei. Ein `for x in kategorie[…]` über
            # `unchanged` liefe über eine Zahl und wirft.
            items = category.get(key)
            if not isinstance(items, list):
                continue
            for p in items:
                fields = []
                for change in p.get('changes') or []:
                    old, has_old = _value(change, 'oldValue')
                    new, has_new = _value(change, 'newValue')
                    fields.append({
                        'pfad': change.get('path') or '',
                        'alt': old, 'hat_alt': has_old,
                        'neu': new, 'hat_neu': has_new,
                    })
                out.append({
                    'art': kind,
                    'zustand': state,
                    'id': p.get('id') or '',
                    'name': p.get('name') or p.get('className') or p.get('id') or '',
                    'groesse': p.get('size'),
                    'felder': fields,
                })
    return out


def categories(version):
    """[(Kategorie, Anzahl geänderter Posten), …] — nur was sich geändert hat.

    Für die Filterleiste: Eine Auswahl mit 24 Einträgen, von denen 20 leer
    sind, ist keine Auswahl."""
    counter = {}
    for entry in changes(version):
        counter[entry['art']] = counter.get(entry['art'], 0) + 1
    return sorted(counter.items(), key=lambda p: (-p[1], p[0]))
