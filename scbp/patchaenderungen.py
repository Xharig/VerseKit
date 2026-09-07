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

**Nicht zu verwechseln mit `patchhistorie.py`.** Die beiden beantworten zwei
verschiedene Fragen und haben zwei verschiedene Quellen:

    patchhistorie.py    welche BAUPLÄNE ein Patch gebracht hat
                        Quelle: eigene Beobachtung des Watchers
    patchaenderungen.py welche WERTE ein Patch geändert hat
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

from . import erkul, fehler, patchhistorie, pfade

ORDNER = 'Patches'

# Die Zusammenfassung, die erkul je Patch im Inhaltsverzeichnis mitliefert.
# `unchanged` steht bewusst nicht dabei: Ein Patch, bei dem sich nichts geändert
# hat, führt trotzdem tausende unveränderte Einträge — das ist kein Inhalt.
ZAEHLER = ('added', 'removed', 'modified')


def _sicherer_name(version):
    """Dateiname aus einer Spielversion — ohne alles, was Ordner sprengt.

    ⚠ Die Version kommt aus dem Netz und landet als Dateiname auf der Platte.
    Ungeprüft wäre ein `../` darin ein Weg aus dem Ablageordner heraus. Erlaubt
    sind deshalb nur Ziffern, Buchstaben, Punkt und Bindestrich; alles andere
    wird zu `_`."""
    return re.sub(r'[^0-9A-Za-z.\-]', '_', version or 'unbekannt')


def _ordner():
    """Der Ablageordner für die Patch-Dateien — angelegt, falls er fehlt.

    ⚠ Bei gesetztem `SC_BP_HOME` (Selbsttest, Wegwerf-Ordner) bleibt es flach,
    genau wie `pfade.app_datei()` es dort auch tut. Dort geht es um einen
    isolierten Ordner, nicht um Übersicht."""
    basis = pfade.app_ordner()
    if os.environ.get('SC_BP_HOME'):
        return basis
    ziel = os.path.join(basis, ORDNER)
    try:
        os.makedirs(ziel, exist_ok=True)
    except OSError:
        return basis
    return ziel


def _datei(version):
    return os.path.join(_ordner(), 'patch-%s.json' % _sicherer_name(version))


# --------------------------------------------------------------- Die Quelle
def _zahl(wert):
    """`summary`-Werte kommen als Zahl, könnten aber auch fehlen."""
    try:
        return int(wert or 0)
    except (TypeError, ValueError):
        return 0


def _hat_inhalt(zusammenfassung):
    """Hat dieser Patch überhaupt etwas geändert?

    Das ist die Regel „die leeren wegwerfen" an genau einer Stelle. Sie wird
    zweimal gebraucht — beim Abholen und beim Anzeigen —, deshalb steht sie
    hier und nicht doppelt."""
    z = zusammenfassung or {}
    return any(_zahl(z.get(k)) for k in ZAEHLER)


def angebotene():
    """Was erkul gerade vorhält: [{version, datum, summary, path, bytes}, …].

    Neueste zuerst. Ohne Netz eine leere Liste — wie überall im Werkzeug
    läuft es dann einfach ohne diese Angaben weiter."""
    kat = erkul.katalog()
    if not kat:
        return []
    raus = []
    for p in kat.get('patches') or []:
        version = p.get('dataVersion') or ''
        if not version:
            continue
        raus.append({
            'version': version,
            'datum': (p.get('generatedAt') or '')[:10],
            'summary': p.get('summary') or {},
            'path': p.get('path') or '',
            'bytes': _zahl(p.get('bytes')),
        })
    raus.sort(key=lambda e: patchhistorie.rang(e['version']), reverse=True)
    return raus


def _abrufen(eintrag):
    """Die Änderungsliste eines Patches von erkul holen — oder `None`.

    ⚠ Der Zweig-Präfix ist Pflicht (siehe Modulkopf)."""
    pfad = eintrag.get('path')
    if not pfad:
        return None
    return erkul._holen('%s/%s' % (erkul.ZWEIG, pfad), 'changelog')


# --------------------------------------------------------------- Die Ablage
def gespeicherte():
    """Die Spielversionen, die hier schon liegen — neueste zuerst."""
    try:
        namen = os.listdir(_ordner())
    except OSError:
        return []
    versionen = []
    for name in namen:
        if name.startswith('patch-') and name.endswith('.json'):
            eintrag = _lesen_datei(os.path.join(_ordner(), name))
            if eintrag and eintrag.get('version'):
                versionen.append(eintrag['version'])
    versionen.sort(key=patchhistorie.rang, reverse=True)
    return versionen


def _lesen_datei(pfad):
    try:
        with open(pfad, encoding='utf-8') as f:
            return json.load(f)
    except Exception:
        return None


def laden(version):
    """Die abgelegte Änderungsliste einer Spielversion — oder `None`."""
    return _lesen_datei(_datei(version))


def _schreiben(version, daten):
    """Eine Patch-Datei ablegen. Erst daneben, dann umbenennen.

    ⚠ Ohne den Umweg über `.tmp` stünde bei einem Abbruch mitten im Schreiben
    eine halbe JSON-Datei da, die beim nächsten Lesen still als „kaputt" gilt —
    und der Patch wäre verloren, obwohl erkul ihn längst nicht mehr vorhält."""
    ziel = _datei(version)
    try:
        temp = ziel + '.tmp'
        with open(temp, 'w', encoding='utf-8') as f:
            # ⚠ **Kompakt, ohne Einrückung** — anders als `patch-historie.json`.
            # Die liegt im Repo und soll lesbar sein; diese hier liest niemand
            # von Hand, sie hat vierstellige Einträge. Gemessen am 07.09.2026:
            # mit `indent=1` sind es 570 KB für zwei Patches, ohne 373 KB —
            # 35 % Aufschlag für Leerzeichen, die keiner sieht. Der Plan
            # rechnete mit den kleineren Zahlen.
            json.dump(daten, f, ensure_ascii=False, separators=(',', ':'))
        os.replace(temp, ziel)
        return True
    except Exception as ausnahme:
        fehler.merken('patchaenderungen.schreiben', ausnahme)
        return False


# ------------------------------------------------------------- Der Abgleich
def abgleichen():
    """Neue Patches von erkul holen und ablegen. Gibt die neu abgelegten zurück.

    Der Ablauf in einem Satz: **was erkul anbietet, was hier noch fehlt, und
    davon nur das mit Inhalt.**

    ⚠ Leere Patches werden nicht abgelegt, aber auch **nicht erneut geholt** —
    sie stehen im Inhaltsverzeichnis mit ihrer Zusammenfassung, und die reicht
    zur Entscheidung. Ein Abruf pro leerem Patch wäre Verkehr, den erkul
    bezahlt und der niemandem nützt.

    Wirft nie: Ohne Netz kommt eine leere Liste zurück, und das Werkzeug läuft
    weiter wie vorher."""
    vorhanden = set(gespeicherte())
    neu = []
    for eintrag in angebotene():
        version = eintrag['version']
        if version in vorhanden or not _hat_inhalt(eintrag['summary']):
            continue
        daten = _abrufen(eintrag)
        if not daten:
            continue
        daten['version'] = version
        daten['datum'] = eintrag['datum']
        daten['quelle'] = 'erkul.games'
        if _schreiben(version, daten):
            neu.append(version)
    return neu


# ------------------------------------------------------------- Aufbereitung
def uebersicht():
    """Alles, was sich anzeigen lässt: [{version, kurz, datum, …}, …].

    **Vereinigt beide Seiten** — was erkul gerade anbietet und was hier liegt.
    Genau darin steckt der Gewinn der lokalen Ablage: Ein Patch, den erkul
    inzwischen fallen gelassen hat, steht hier weiter, und zwar mit
    `bei_erkul=False`. Wer nur eine der beiden Seiten liest, verliert ihn."""
    zusammen = {}
    for eintrag in angebotene():
        zusammen[eintrag['version']] = {
            'version': eintrag['version'],
            'kurz': eintrag['version'].split('-')[0],
            'datum': eintrag['datum'],
            'summary': eintrag['summary'],
            'leer': not _hat_inhalt(eintrag['summary']),
            'bei_erkul': True,
            'abgelegt': False,
        }
    for version in gespeicherte():
        daten = laden(version) or {}
        eintrag = zusammen.get(version)
        if eintrag is None:
            eintrag = {
                'version': version,
                'kurz': version.split('-')[0],
                'datum': daten.get('datum') or '',
                'summary': daten.get('summary') or {},
                'leer': False,
                'bei_erkul': False,
            }
            zusammen[version] = eintrag
        eintrag['abgelegt'] = True
    raus = list(zusammen.values())
    raus.sort(key=lambda e: patchhistorie.rang(e['version']), reverse=True)
    return raus


def _wert(eintrag, schluessel):
    """`oldValue`/`newValue` — beide dürfen fehlen, und das ist die Aussage.

    ⚠ **Fehlt einer, ist das kein Datenfehler, sondern der Inhalt.** Gemessen
    am 07.09.2026 an der C-788 Cannon: `weapon.ammo.impactRadius` hat nur einen
    `oldValue` — das Feld ist mit dem Patch **weggefallen**. Wer stumpf
    `eintrag['newValue']` liest, bekommt hier einen `KeyError` und reißt die
    ganze Anzeige mit. Zurück kommt deshalb `(wert, vorhanden)`."""
    return eintrag.get(schluessel), schluessel in eintrag


def aenderungen(version, art=None):
    """Die Änderungen eines Patches, flach und anzeigefertig.

    Liefert je Eintrag: Kategorie, was passiert ist (`neu`/`weg`/`geaendert`),
    Name, Größe und die einzelnen Feldänderungen mit altem und neuem Wert.
    Mit `art` lässt sich auf eine Kategorie einschränken (`'weapons'` …)."""
    daten = laden(version)
    if not daten:
        return []
    raus = []
    for kategorie in daten.get('categories') or []:
        kind = kategorie.get('kind') or ''
        if art and kind != art:
            continue
        for zustand, schluessel in (('neu', 'added'), ('weg', 'removed'),
                                    ('geaendert', 'modified')):
            # ⚠ `unchanged` ist eine ZAHL, die anderen drei sind LISTEN — im
            # selben Feld derselben Datei. Ein `for x in kategorie[…]` über
            # `unchanged` liefe über eine Zahl und wirft.
            posten = kategorie.get(schluessel)
            if not isinstance(posten, list):
                continue
            for p in posten:
                felder = []
                for aenderung in p.get('changes') or []:
                    alt, hat_alt = _wert(aenderung, 'oldValue')
                    neu, hat_neu = _wert(aenderung, 'newValue')
                    felder.append({
                        'pfad': aenderung.get('path') or '',
                        'alt': alt, 'hat_alt': hat_alt,
                        'neu': neu, 'hat_neu': hat_neu,
                    })
                raus.append({
                    'art': kind,
                    'zustand': zustand,
                    'id': p.get('id') or '',
                    'name': p.get('name') or p.get('className') or p.get('id') or '',
                    'groesse': p.get('size'),
                    'felder': felder,
                })
    return raus


def kategorien(version):
    """[(Kategorie, Anzahl geänderter Posten), …] — nur was sich geändert hat.

    Für die Filterleiste: Eine Auswahl mit 24 Einträgen, von denen 20 leer
    sind, ist keine Auswahl."""
    zaehler = {}
    for eintrag in aenderungen(version):
        zaehler[eintrag['art']] = zaehler.get(eintrag['art'], 0) + 1
    return sorted(zaehler.items(), key=lambda p: (-p[1], p[0]))
