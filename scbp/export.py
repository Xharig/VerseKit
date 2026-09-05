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

Zwei Formate, zwei Zwecke:

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

from . import bestand as bestand_datei
from . import katalog as katalog_modul


def _iso(zeit_text):
    """„2026-08-24 07:57:59" -> „2026-08-24T07:57:59Z" oder None.

    Der Bestand hält die Zeit in lesbarer Form; das Basetool erwartet ISO 8601.
    Lässt sich der Wert nicht deuten, wird das Feld **weggelassen** — laut
    Format ist es optional, und ein erfundener Zeitpunkt wäre schlechter als
    gar keiner."""
    if not zeit_text:
        return None
    try:
        t = time.strptime(str(zeit_text), '%Y-%m-%d %H:%M:%S')
        return time.strftime('%Y-%m-%dT%H:%M:%SZ', t)
    except (ValueError, TypeError):
        return None


def fuer_basetool(bestand=None):
    """Die Struktur, die `profit-base.online` beim Import erwartet."""
    daten = bestand if bestand is not None else bestand_datei.laden()
    eintraege = []
    for schluessel, e in sorted((daten.get('bauplaene') or {}).items()):
        name = (e.get('name') or '').strip()
        if not name:
            continue                     # leere Namen fliegen beim Import raus
        satz = {'productName': name}
        zeit = _iso(e.get('zeit'))
        if zeit:
            satz['receivedAt'] = zeit
        eintraege.append(satz)
    return {'blueprints': eintraege}


SCMDB_URL = 'https://scmdb.net/?page=fab&fab=%s'


def _scmdb_tags():
    """Bauplanname (klein) -> Tag, aus den Rezeptdaten.

    Der Tag (`BP_CRAFT_AMRS_LaserCannon_S2`) ist bei scmdb der Schlüssel — der
    Name ist nur Beiwerk. `rezept()` gibt ihn nicht heraus, `alle()` schon.

    ⚠ Liegen keine Rezeptdaten vor (frische Installation, kein Netz), ist die
    Zuordnung leer. Der Export läuft dann trotzdem, nur ohne Tags — er darf
    nicht am Netz hängen."""
    tabelle = {}
    try:
        from . import herstellung
        for r in herstellung.alle():
            tag = (r.get('tag') or '').strip()
            if not tag:
                continue
            for schluessel in (r.get('name'), r.get('basis')):
                if schluessel:
                    tabelle.setdefault(schluessel.strip().lower(), tag)
    except Exception:
        return {}
    return tabelle


def fuer_scmdb(bestand=None, version='', tags=None):
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
    daten = bestand if bestand is not None else bestand_datei.laden()
    tabelle = _scmdb_tags() if tags is None else tags
    eintraege = []
    for schluessel, e in sorted((daten.get('bauplaene') or {}).items()):
        name = (e.get('name') or '').strip()
        if not name:
            continue
        satz = {'tag': tabelle.get(name.lower(), ''), 'name': name}
        if satz['tag']:
            satz['url'] = SCMDB_URL % satz['tag']
        satz['completed'] = True
        satz['favorite'] = False
        eintraege.append(satz)
    return {
        'version': 3,
        'exportedAt': time.strftime('%Y-%m-%dT%H:%M:%S.000Z', time.gmtime()),
        'missions': [],
        'blueprints': eintraege,
    }


def vollstaendig(bestand=None, katalog=None):
    """Alles, was das Werkzeug über den eigenen Bestand weiß."""
    daten = bestand if bestand is not None else bestand_datei.laden()
    kat = (katalog if katalog is not None else katalog_modul.laden())
    kb = kat.get('bauplaene') or {}
    eintraege = []
    for schluessel, e in sorted((daten.get('bauplaene') or {}).items()):
        k = kb.get(schluessel) or {}
        satz = {
            'name': e.get('name'),
            'quelle': e.get('quelle'),
            'zeit': e.get('zeit'),
            'art': katalog_modul.art_lesbar(k.get('a')) if k.get('a') else None,
            'klasse': k.get('c'),
            'size': k.get('s'),
            'grade': k.get('g'),
            'hersteller': k.get('m'),
        }
        eintraege.append({kk: v for kk, v in satz.items() if v not in (None, '')})
    return {
        'werkzeug': 'SC BP Watcher',
        'erstellt': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
        'spielversion': kat.get('version') or None,
        'anzahl': len(eintraege),
        'bauplaene': eintraege,
    }


def schreiben(pfad, art='basetool', bestand=None, katalog=None, version=''):
    """Eine Version in eine Datei schreiben. (Erfolg, Meldung)."""
    try:
        # ⚠ Das Auftrags-Protokoll ist kein Bauplan-Bestand: eigene Struktur,
        # eigene Leer-Pruefung. Es faellt deshalb VOR der gemeinsamen Zaehlung
        # heraus — sonst gaelte es als „leerer Bestand" und wuerde nie
        # geschrieben.
        if art == 'auftraege':
            from . import missionslog
            eintraege = missionslog.laden()
            if not eintraege:
                # ⚠ Knapp wie „leerer Bestand" unten, kein ganzer Satz: Diese
                # Rueckmeldungen gehen ins Protokoll, nicht auf die Seite.
                return False, 'leeres Protokoll'
            ordner = os.path.dirname(os.path.abspath(pfad))
            if ordner:
                os.makedirs(ordner, exist_ok=True)
            with open(pfad + '.tmp', 'w', encoding='utf-8', newline='\n') as f:
                f.write(missionslog.als_json(eintraege))
            os.replace(pfad + '.tmp', pfad)
            return True, str(len(eintraege))

        if art == 'basetool':
            doc = fuer_basetool(bestand)
        elif art == 'scmdb':
            doc = fuer_scmdb(bestand, version)
        else:
            doc = vollstaendig(bestand, katalog)
        anzahl = len(doc.get('blueprints') or doc.get('bauplaene') or [])
        if not anzahl:
            return False, 'leerer Bestand'
        ordner = os.path.dirname(os.path.abspath(pfad))
        if ordner:
            os.makedirs(ordner, exist_ok=True)
        with open(pfad + '.tmp', 'w', encoding='utf-8', newline='\n') as f:
            json.dump(doc, f, ensure_ascii=False, indent=1)
        os.replace(pfad + '.tmp', pfad)
        return True, str(anzahl)
    except OSError as fehler:
        return False, str(fehler)


DATEINAMEN = {
    'basetool': 'SC-Blueprints-Basetool-%s.json',
    'scmdb':    'scmdb-import-%s.json',
    'voll':     'SC-BP-Watcher-Bestand-%s.json',
    'auftraege': 'SC-BP-Watcher-Auftraege-%s.json',
}


def vorschlag(art='basetool', mit_datum=True):
    """Ein sinnvoller Dateiname.

    ⚠ **Mit Datum nur im Speichern-Dialog.** Wer von Hand speichert, hält einen
    Stand fest — da gehört der Tag in den Namen. Die Ablage dagegen wird bei
    jedem neuen Bauplan mitgeschrieben; mit Datum entstünden dort **jeden Tag
    drei neue Dateien**, und wer eine hochladen will, müsste erst die richtige
    heraussuchen. Genau das Suchen sollte die Ablage abschaffen. Dort steht
    deshalb immer derselbe Name, und die Datei ist immer die aktuelle.
    """
    name = DATEINAMEN.get(art, DATEINAMEN['voll'])
    if not mit_datum:
        # „…-%s.json" → „….json", ohne den Bindestrich davor stehen zu lassen.
        return name.replace('-%s', '').replace('%s', '')
    return name % time.strftime('%Y-%m-%d')


def ablage_ordner():
    """Wohin die Ablage schreibt. Eigener Ordner neben den übrigen Dateien.

    Ein fester Ort statt jedes Mal ein Dateidialog: Wer den Bestand regelmäßig
    hochlädt, will nicht dreimal durch einen Speichern-Dialog klicken. Der
    Dialog bleibt für den Einzelfall daneben bestehen."""
    from . import pfade
    eigen = pfade.einstellung('export_ordner')
    ordner = eigen or os.path.join(pfade.app_ordner(), 'export')
    try:
        os.makedirs(ordner, exist_ok=True)
    except OSError:
        pass
    return ordner


ALTORDNER = 'Ältere'


def _altbestand_wegraeumen(ordner):
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
    muster = [(name.split('-%s')[0], name.split('%s')[-1])
              for name in DATEINAMEN.values()]
    umzug = []
    try:
        vorhanden = os.listdir(ordner)
    except OSError:
        return
    for datei in vorhanden:
        voll = os.path.join(ordner, datei)
        if not os.path.isfile(voll):
            continue
        # Nur die alten, datierten Versionen: Anfang und Endung wie bei uns,
        # aber länger als der feste Name — das Datum steckt dazwischen.
        for anfang, endung in muster:
            if (datei.startswith(anfang) and datei.endswith(endung)
                    and len(datei) > len(anfang) + len(endung)):
                umzug.append(datei)
                break
    if not umzug:
        return
    alt = os.path.join(ordner, ALTORDNER)
    try:
        os.makedirs(alt, exist_ok=True)
        for datei in umzug:
            ziel = os.path.join(alt, datei)
            if os.path.exists(ziel):
                os.remove(os.path.join(ordner, datei))   # liegt dort schon
            else:
                os.replace(os.path.join(ordner, datei), ziel)
    except OSError as ausnahme:
        from . import fehler
        fehler.merken('export.altbestand', ausnahme, ordner)


def ablegen(bestand=None, katalog=None, version=''):
    """**Alle** Versionen auf einmal in die Ablage schreiben.

    Gibt (Erfolg, Ordner, Liste der Dateien) zurück. Ein Fehler bei einer
    Version lässt die anderen nicht ausfallen — lieber zwei von drei Dateien
    als gar keine."""
    ordner = ablage_ordner()
    _altbestand_wegraeumen(ordner)
    geschrieben = []
    # ⚠ Das Auftrags-Protokoll gehoert mit in die Ablage: Es ist eine eigene
    # Liste wie der Bestand, und wer seine Daten sichert, meint alle. Fehlt es
    # hier, merkt das niemand — bis der Rechner neu aufgesetzt ist.
    for art in ('basetool', 'scmdb', 'voll', 'auftraege'):
        ziel = os.path.join(ordner, vorschlag(art, mit_datum=False))
        ok, _meldung = schreiben(ziel, art, bestand, katalog, version)
        if ok:
            geschrieben.append(os.path.basename(ziel))
    return bool(geschrieben), ordner, geschrieben
