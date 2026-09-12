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
Der eigene Bauplan-Bestand — die Liste „welche habe ich".

Bis v1.5.0 kam sie ausschließlich vom SC Deutsch Launcher. Ab jetzt führt der
Watcher sie selbst: Jeder Bauplan, der in der Game.log auftaucht, wird
dauerhaft festgehalten. Damit läuft das Programm ohne den Launcher — und
unter Linux, wo es ihn gar nicht gibt.

**Warum das nicht der schlechtere Weg ist:** Am 11.08.2026 gemessen — dem
Launcher fehlt die P4-AR Rifle, obwohl sie im Fabricator als „im Besitz" steht.
Startbaupläne wurden nie „erhalten" und stehen deshalb in keinem Log. Seine
Zahl ist eine Untergrenze, kein Bestand. Ein selbst geführter Bestand, der
Startbaupläne kennt und Nachlese aus den Log-Sicherungen betreibt, ist genauer.

Die Datei liegt im App-Ordner (`bestand.json`) und sieht so aus:

    {
      "version": 1,
      "stand": "2026-08-24 02:31:00",
      "bauplaene": {
        "7ca 'nargun'": {"name": "7CA 'Nargun'", "quelle": "log",
                         "zeit": "2026-08-24 02:31:00"}
      }
    }

Der Schlüssel ist der kleingeschriebene Name — derselbe Abgleich, den auch
das Hauptprogramm benutzt, damit Log-Fund und Launcher-Eintrag zusammenfinden.
Bekannte Quellen: `log` (aus der laufenden Game.log), `nachlese` (aus einer
Log-Sicherung), `launcher` (vom SC Deutsch Launcher bestätigt), `start`
(Startbauplan, war von Anfang an da) und `hand` (im Fenster abgehakt).

⚠ Bis zum 11.09.2026 hieß dieses Modul `bestand` (Sprachumstellung P4,
Stufe 1). **Nur Bezeichner sind umbenannt, keine Zeichenketten.** Bewusst
gleich geblieben, weil sie in den Dateien jedes Nutzers stehen: die Dateinamen
`bestand.json`, `bestand.bak.json` und `hochwasser.json`, die Schlüssel
`version`, `stand`, `bauplaene`, `name`, `quelle`, `zeit` und `ordner` und die
Quellwerte oben. Ebenso der Schlüssel `'unbekannt'`, den `by_source()` für
Einträge ohne Quelle liefert. Umbenannt, stünde bei jedem Nutzer nach dem
Update ein leerer Bestand da.
"""
import json
import os
import time

from . import fehler, pfade

# 3 (29.08.2026): `namensform()` gleicht jetzt auch die SPRACHE der Mengenangabe
#   an — `(16 Schuss)` und `(16 cap)` sind derselbe Bauplan. Gespeicherte
#   Bestaende haben die Dublette noch drin, deshalb muss der Umzug erneut laufen.
FILE_VERSION = 3

# Rangfolge der Quellen: Ein Eintrag wird nur „aufgewertet", nie herabgestuft.
# Sonst überschriebe eine spätere vorläufige Log-Zeile eine bereits vom
# Launcher bestätigte Angabe.
RANK = {'log': 1, 'nachlese': 1, 'start': 2, 'hand': 3, 'launcher': 4}


def norm(s):
    """Vergleichsform eines Namens — siehe `pfade.namensform`."""
    return pfade.namensform(s)


def _now():
    return time.strftime('%Y-%m-%d %H:%M:%S')


def path():
    return pfade.app_datei('bestand.json')


def empty():
    return {'version': FILE_VERSION, 'stand': _now(), 'bauplaene': {}}


def _renew_keys(data):
    """Gespeicherte Schlüssel noch einmal durch `namensform()` schicken.

    ⚠ **Warum das nötig war.** Bis v3.0.0 schnitt nur das Log-Lesen den
    Klassen-Zusatz ab. Namen aus der **Launcher-Datei** und aus **Importen**
    landeten mitsamt Zusatz im Bestand — `xl-1 (mil/2/a)` statt `xl-1`. Die
    Bauplan-Liste sucht nach `xl-1` und fand nichts: Der Bauplan galt als
    fehlend, obwohl er dastand.

    Seit v3.0.0 schneidet `namensform()` selbst ab. Das hilft aber nur neuen
    Einträgen — die **gespeicherten** Schlüssel bleiben, wie sie sind. Deshalb
    werden sie hier einmalig neu gebildet.

    Gemessen an Morkhans Bericht (28.08.2026): **320 Baupläne** im Bestand,
    Launcher wird gefunden — und im Spiel trotzdem alles leer.

    Treffen zwei alte Schlüssel auf denselben neuen, gewinnt der **ältere
    Fund**: Wann ein Bauplan zum ersten Mal auftauchte, ist die Angabe, die
    zählt. Gibt es sie nicht, gewinnt der mit dem höheren Rang (Launcher
    schlägt Log).
    """
    old_bp = data.get('bauplaene') or {}
    new_bp, changed = {}, False
    for key, entry in old_bp.items():
        entry = entry if isinstance(entry, dict) else {}
        fresh = norm(entry.get('name') or key)
        if fresh != key:
            changed = True
        present = new_bp.get(fresh)
        if present is None:
            new_bp[fresh] = entry
            continue
        # Dublette zusammenführen
        old_time = str(present.get('zeit') or '')
        new_time = str(entry.get('zeit') or '')
        if new_time and (not old_time or new_time < old_time):
            entry = dict(entry)
            entry.setdefault('quelle', present.get('quelle'))
            new_bp[fresh] = entry
        elif RANK.get(entry.get('quelle'), 0) > RANK.get(present.get('quelle'), 0):
            present['quelle'] = entry.get('quelle')
    if changed:
        data['bauplaene'] = new_bp
    return changed


def load():
    """Bestand von der Platte. Fehlt die Datei oder ist sie beschädigt, wird mit
    einem leeren Bestand weitergearbeitet — der Watcher soll nie am Start scheitern."""
    try:
        with open(path(), encoding='utf-8') as f:
            data = json.load(f)
    except Exception:
        return empty()
    if not isinstance(data.get('bauplaene'), dict):
        return empty()
    # ⚠ Erst umziehen, dann die Version hochsetzen — und nur dann schreiben,
    # wenn sich wirklich etwas geändert hat. Ein Schreibfehler darf den Start
    # nicht aufhalten: Der Bestand im Speicher stimmt dann trotzdem, nur der
    # Umzug wiederholt sich beim nächsten Mal.
    # ⚠ Gegen FILE_VERSION pruefen, nicht gegen eine feste Zahl: Beim Sprung
    # auf 3 waere ein hart geschriebenes `< 2` stillschweigend wirkungslos
    # geblieben, und die Dubletten haetten ueberlebt.
    if data.get('version', 1) < FILE_VERSION:
        if _renew_keys(data):
            data['version'] = FILE_VERSION
            try:
                save(data)
            except Exception as exc:
                fehler.merken('collection.renew_keys', exc)
        else:
            data['version'] = FILE_VERSION
    data.setdefault('version', FILE_VERSION)
    return data


def _high_water_file():
    """Wo die groesste je gesehene Bauplan-Zahl steht — NEBEN der Ablage.

    ⚠ Bewusst nicht IM Datenordner: Genau der ist ja weg, wenn es darauf
    ankommt. Die Marke liegt im Konfigurationsordner, dort, wo auch der
    Zweitzeiger sitzt.
    """
    return os.path.join(os.path.dirname(pfade._zweitzeiger()), 'hochwasser.json')


def check_shrinkage(data):
    """Sind ploetzlich Bauplaene weniger als je zuvor? Dann melden.

    ⚠⚠⚠ **Der Fall, aus dem das entstand.** Am 06.09.2026 zeigte der Watcher
    nach einem Neustart 406 statt 413 Bauplaenen. Verloren war nichts — er
    schaute nur in einen anderen Ordner, weil die Zeiger-Datei beim Aufraeumen
    im Dateimanager mit weggeworfen worden war. Er nahm den leeren Standardort,
    legte dort einen Bestand an und sagte **kein Wort** dazu.

    Zurueck blieb eine Zahl, die kleiner war als gestern, und keine Erklaerung:
    *„wieso aendert sich immer wieder der Ordner, die ganze Zeit hat es doch
    geklappt?"*

    ⚠ Geprueft wird gegen den **Hoechststand**, nicht gegen den letzten Lauf.
    Ein Bestand wird nie kleiner: Bauplaene verschwinden nicht von selbst. Wird
    er es doch, stimmt etwas mit dem ORT nicht — und genau das soll dastehen,
    solange der Spieler es noch mit dem Neustart in Verbindung bringt.

    ⚠ Ein bewusstes Zuruecksetzen ist kein Schwund: `reset()` setzt die
    Marke mit zurueck, sonst meldete das Programm hinterher ewig einen Verlust,
    den der Spieler selbst gewollt hat.

    Zurueck kommt `None`, wenn alles stimmt — sonst `(jetzt, hoechststand,
    ordner)` fuer die Meldung.
    """
    now = len(data.get('bauplaene') or {})
    mark = _high_water_file()
    highest, folder = 0, None
    try:
        if os.path.isfile(mark):
            stored = json.load(open(mark, encoding='utf-8'))
            highest = int(stored.get('bauplaene') or 0)
            folder = stored.get('ordner')
    except Exception:
        highest, folder = 0, None

    if now >= highest:
        # Neuer Hoechststand — merken, samt Ordner, damit die Meldung spaeter
        # sagen kann, WO die Bauplaene zuletzt lagen.
        try:
            os.makedirs(os.path.dirname(mark), exist_ok=True)
            tmp = mark + '.tmp'
            with open(tmp, 'w', encoding='utf-8') as f:
                json.dump({'bauplaene': now, 'ordner': pfade.app_ordner(),
                           'stand': _now()}, f, ensure_ascii=False,
                          indent=2)
            os.replace(tmp, mark)
        except Exception:
            pass
        return None

    return (now, highest, folder)


def shrinkage_state():
    """Dasselbe wie `check_shrinkage`, aber **ohne** die Marke zu veraendern.

    ⚠ Fuer die Oberflaeche. Wuerde eine Seite `check_shrinkage` aufrufen, wuerde
    sie beim ersten Blick den aktuellen (kleineren) Stand als neuen Hoechstwert
    festschreiben — und die Meldung waere nach einmal Hinsehen fuer immer weg.
    """
    try:
        data = load()
        now = len(data.get('bauplaene') or {})
        mark = _high_water_file()
        if not os.path.isfile(mark):
            return None
        stored = json.load(open(mark, encoding='utf-8'))
        highest = int(stored.get('bauplaene') or 0)
        if now >= highest:
            return None
        return (now, highest, stored.get('ordner'))
    except Exception:
        return None


def reset_high_water():
    """Die Marke loeschen — nach einem gewollten Zuruecksetzen."""
    try:
        os.remove(_high_water_file())
    except OSError:
        pass


def reset():
    """Den Bauplan-Bestand von der Platte nehmen.

    Rückgabe: `None`, wenn danach keine Bestandsdatei mehr da ist — **auch
    dann, wenn vorher schon keine da war**. Sonst die Störung, die im Weg
    stand (keine Rechte, Datei gesperrt).

    ⚠⚠ **„War schon weg" ist Erfolg, kein Fehler.** Bis v3.5.0 lag das
    `os.remove` unmittelbar in der Oberfläche, und ein `FileNotFoundError`
    landete still in der Diagnose: Der Nutzer drückte den roten Knopf,
    bestätigte die Warnfrage — und dann passierte **nichts**. Kein Haken,
    keine Meldung. Das Werkzeug sah kaputt aus, obwohl der Zustand genau der
    gewünschte war.

    ⚠ Der Fall trifft nicht die Ausnahme, sondern den Anfänger: Wer noch
    keinen einzigen Bauplan hat, hat auch keine Bestandsdatei. Am 31.08.2026
    aus einem Nutzerbericht mit „Inventory 0 blueprints" (Linux, CachyOS).

    ⚠ Hier und nicht in der Oberfläche, damit es sich prüfen lässt — ohne
    Fenster, auf jedem System.
    """
    # ⚠⚠ **Die Hochwasser-Marke muss mit.** Sonst meldet `check_shrinkage`
    # nach einem gewollten Zuruecksetzen bei jedem Start einen Verlust, den
    # der Spieler selbst ausgeloest hat — und eine Warnung, die immer kommt,
    # liest nach dem dritten Mal niemand mehr.
    reset_high_water()
    try:
        os.remove(path())
    except FileNotFoundError:
        return None
    except OSError as err:
        return err
    return None


def save(data):
    """Schreibt den Bestand — mit Vorgängerfassung und ohne Halbfertiges.

    Erst in eine Nebendatei schreiben, dann umbenennen: Stürzt der Rechner
    mitten im Schreiben ab, ist die alte Datei noch vollständig da. Die
    Vorgängerfassung (`bestand.bak.json`) bleibt als Rückfall liegen."""
    data['version'] = FILE_VERSION
    data['stand'] = _now()
    target = path()
    tmp = target + '.tmp'
    try:
        os.makedirs(os.path.dirname(target), exist_ok=True)
        with open(tmp, 'w', encoding='utf-8') as f:
            json.dump(data, f, ensure_ascii=False, indent=1, sort_keys=True)
        if os.path.exists(target):
            bak = target.replace('.json', '.bak.json')
            try:
                os.replace(target, bak)
            except OSError:
                pass
        os.replace(tmp, target)
        _update_exports(data)
        return True
    except Exception as exc:
        # Hier ist der eigene Bauplan-Bestand betroffen — das Wichtigste, was
        # das Werkzeug hat. Ein stiller Fehlschlag wäre nicht zu verzeihen.
        fehler.merken('collection.save', exc, target)
        try:
            os.remove(tmp)
        except OSError:
            pass
        return False


def _update_exports(data):
    """Die drei Ausgabe-Dateien auf den neuen Stand bringen — still.

    ⚠ **Warum das hier hängt und nicht am Knopf.** Die Ausgabe-Dateien für das
    KRT Profit Basetool, für scmdb.net und die Vollsicherung wurden bisher
    **nur** geschrieben, wenn jemand auf „Alle drei in die Ablage" klickte.
    Wer das einmal gemacht hatte, hielt sie danach für aktuell — sie standen
    aber für immer auf dem Stand jenes Klicks. Aufgefallen ist es, als jemand das
    Werkzeug jemandem vorführte und selbst suchen musste, wo die Dateien
    herkommen (27.08.2026): „die werden ja bei drops direkt fortgeschrieben
    oder?" Nein — bis jetzt nicht.

    An `save()` hängt es, weil hier **jede** Bestandsänderung
    vorbeikommt: der Fund im Spiel, die Nachlese beim Start, die Bestätigung
    durch den Launcher und der Import einer fremden Datei. Ein Aufruf statt
    fünf, und keine Stelle kann vergessen werden.

    ⚠ **Fehler bleiben still.** Diese Dateien sind eine Bequemlichkeit; der
    Bestand ist die Hauptsache und liegt zu diesem Zeitpunkt bereits sicher auf
    der Platte. Ein voller Datenträger oder ein gesperrter Ordner darf die
    Erkennung nicht anhalten — gemerkt wird der Fehler trotzdem, damit er im
    Diagnosebericht auftaucht.
    """
    try:
        from . import export
        export.ablegen(data)
    except Exception as exc:
        fehler.merken('collection.update_exports', exc)


SUFFIX_RE = __import__('re').compile(r'\s*\([^()]*\)\s*$')


def catalog_name(name, known=None):
    """Den Namen so, wie ihn der Katalog kennt — ohne angehängte Angaben.

    ⚠⚠ **`known` durchreichen, wenn viele Namen hintereinander laufen.**
    Ohne den Parameter holt sich diese Funktion den Katalog selbst — und
    `catalog.load()` liest jedes Mal die ganze Datei (rund 1 MB). Bei einem
    Aufruf faellt das nicht auf, bei 406 hintereinander schon: Gemessen am
    04.09.2026 brauchte `align()` dadurch **3,6 Sekunden** bei jedem
    Programmstart — und berichtigte dabei null Eintraege. Wer 26 Bauplaene hat,
    merkt nichts; wer 400 hat, wartet.

    ⚠⚠ **Warum das nötig ist: Wir vergiften uns die eigene Erkennung.** Das
    Werkzeug (und der SC Deutsch Launcher) schreiben Klasse, Größe und Gütegrad
    an die Gegenstandsnamen im Spiel. Schaltet das Spiel danach frei, steht in
    der `Game.log` nicht mehr „Balandin", sondern **„Balandin (S3 B Military)"**
    — und genau das wurde gespeichert. Der Katalog kennt den Namen nicht, also
    tauchte der Bauplan in der Liste **nie als vorhanden** auf, der Fortschritt
    blieb zu niedrig, und mit jedem Fund wurde es schlimmer.

    Gemeldet am 30.08.2026 von **Morkhan (KRT)**: 315 gespeicherte Baupläne,
    davon 23 dem Katalog unbekannt — zwölf davon nur wegen des Anhangs.

    ⚠ **Die Klammer wird nur abgeschnitten, wenn sie die Ursache ist.** 39
    Katalognamen tragen selbst eine — „A03 Sniper Rifle Magazine (15 cap)",
    „Artimex Arms (Modified)". Deshalb die Bedingung: der volle Name ist
    unbekannt **und** der gekürzte bekannt. Damit greift die Regel auch bei
    einem Anhang, den es heute noch gar nicht gibt.
    """
    if not name:
        return name
    if known is None:
        from . import catalog
        try:
            known = catalog.load().get('bauplaene') or {}
        except Exception:
            return name
    if not known or norm(name) in known:
        return name
    short = SUFFIX_RE.sub('', name).strip()
    if short and short != name and norm(short) in known:
        return short
    return _unique_match(name, known)


# Ab so vielen Wörtern darf über die Wortmenge zugeordnet werden.
MIN_WORDS = 2


def _unique_match(name, known):
    """Ein Altname, dessen Wörter in **genau einem** Katalognamen stecken.

    ⚠ Wozu: Die Übersetzung benennt Gegenstände gelegentlich um. Wer den
    Bauplan vorher bekommen hat, trägt den alten Namen für immer im Bestand —
    `BlackFire Racing Flight Suit`, während der Katalog heute
    `Neutrino Racing Flight Suit BlackFire` sagt. Dieselben Wörter, andere
    Reihenfolge, ein zusätzlicher Reihenname. Ein Zeichenketten-Vergleich fängt
    das nie.

    ⚠⚠ **Und deshalb wird hier nicht geraten.** Zugeordnet wird nur, wenn
    **genau ein** Katalogeintrag sämtliche Wörter enthält. `Parallax` allein
    steckt in fünf Einträgen — bleibt also stehen, statt willkürlich einem
    davon zugeschlagen zu werden. Ein falsch zugeordneter Bauplan ist schlimmer
    als ein offen ausgewiesener.

    ⚠ Mindestens **zwei** Wörter. Ein einzelnes Wort steckt schnell in einem
    fremden Namen (`Tailwind` in `Tailwind Flight Suit`), und dann wäre die
    Eindeutigkeit nur Zufall.
    """
    words = set(norm(name).split())
    if len(words) < MIN_WORDS:
        return name
    hits = [k for k in known if words <= set(k.split())]
    if len(hits) != 1:
        return name
    entry = known[hits[0]]
    return (entry.get('n') if isinstance(entry, dict) else None) or hits[0]


def align(data):
    """Gespeicherte Namen nachträglich an den Katalog angleichen.

    Für alles, was vor dieser Berichtigung schon mit Anhang abgelegt wurde.
    Gibt die Zahl der berichtigten Einträge zurück; `0` heißt „nichts zu tun".
    """
    # ⚠ Den Katalog EINMAL holen und durchreichen — nicht je Bauplan neu.
    # Siehe `catalog_name`: Ohne das las diese Schleife die 1-MB-Katalogdatei
    # einmal pro Eintrag und brauchte bei 406 Bauplaenen 3,6 Sekunden.
    from . import catalog
    try:
        known = catalog.load().get('bauplaene') or {}
    except Exception:
        return 0

    fixed = 0
    for key in list(data['bauplaene']):
        entry = data['bauplaene'][key]
        old_name = entry.get('name') or key
        new_name = catalog_name(old_name, known)
        if new_name == old_name:
            continue
        data['bauplaene'].pop(key)
        new_key = norm(new_name)
        # Gibt es den Bauplan schon unter dem richtigen Namen, bleibt der
        # ältere Eintrag stehen — er hat den früheren Fundzeitpunkt.
        if new_key not in data['bauplaene']:
            entry['name'] = new_name
            data['bauplaene'][new_key] = entry
        fixed += 1
    return fixed


def add(data, name, source='log', when=None):
    """Einen Bauplan aufnehmen. Gibt True zurück, wenn er vorher nicht drin war.

    Ein schon bekannter Bauplan wird nicht doppelt angelegt; steht die neue
    Quelle höher (z. B. `launcher` statt `log`), wird sie nachgetragen.

    ⚠ Der Name läuft vorher durch `catalog_name()` — siehe dort, warum.
    """
    name = catalog_name(name)
    key = norm(name)
    if not key:
        return False
    entry = data['bauplaene'].get(key)
    if entry is None:
        data['bauplaene'][key] = {
            'name': name.strip(),
            'quelle': source,
            'zeit': when or _now(),
        }
        return True
    if RANK.get(source, 0) > RANK.get(entry.get('quelle'), 0):
        entry['quelle'] = source
    return False


def remove(data, name):
    """Häkchen wieder wegnehmen (Verwaltungsfenster)."""
    return data['bauplaene'].pop(norm(name), None) is not None


def contains(data, name):
    return norm(name) in data['bauplaene']


def keys(data):
    """Alle Namen in Vergleichsform — als Menge, für schnelle Abgleiche."""
    return set(data['bauplaene'])


def names(data):
    """Die Namen in Schreibweise wie gefunden, alphabetisch."""
    return sorted((e.get('name') or k) for k, e in data['bauplaene'].items())


def count(data):
    return len(data['bauplaene'])


def by_source(data):
    """Wie viele Baupläne kommen woher — für die Statusanzeige."""
    counter = {}
    for e in data['bauplaene'].values():
        q = e.get('quelle') or 'unbekannt'
        counter[q] = counter.get(q, 0) + 1
    return counter


if __name__ == '__main__':
    b = load()
    print('Datei:  ', path())
    print('Anzahl: ', count(b))
    print('Quellen:', by_source(b) or '—')
