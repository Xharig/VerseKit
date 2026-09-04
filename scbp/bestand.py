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
"""
import json
import os
import time

from . import fehler, pfade

# 3 (29.08.2026): `namensform()` gleicht jetzt auch die SPRACHE der Mengenangabe
#   an — `(16 Schuss)` und `(16 cap)` sind derselbe Bauplan. Gespeicherte
#   Bestaende haben die Dublette noch drin, deshalb muss der Umzug erneut laufen.
DATEI_VERSION = 3

# Rangfolge der Quellen: Ein Eintrag wird nur „aufgewertet", nie herabgestuft.
# Sonst überschriebe eine spätere vorläufige Log-Zeile eine bereits vom
# Launcher bestätigte Angabe.
RANG = {'log': 1, 'nachlese': 1, 'start': 2, 'hand': 3, 'launcher': 4}


def norm(s):
    """Vergleichsform eines Namens — siehe `pfade.namensform`."""
    return pfade.namensform(s)


def _jetzt():
    return time.strftime('%Y-%m-%d %H:%M:%S')


def pfad():
    return pfade.app_datei('bestand.json')


def leer():
    return {'version': DATEI_VERSION, 'stand': _jetzt(), 'bauplaene': {}}


def _schluessel_erneuern(daten):
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
    alt_bp = daten.get('bauplaene') or {}
    neu_bp, geaendert = {}, False
    for schluessel, eintrag in alt_bp.items():
        eintrag = eintrag if isinstance(eintrag, dict) else {}
        frisch = norm(eintrag.get('name') or schluessel)
        if frisch != schluessel:
            geaendert = True
        da = neu_bp.get(frisch)
        if da is None:
            neu_bp[frisch] = eintrag
            continue
        # Dublette zusammenführen
        alt_zeit = str(da.get('zeit') or '')
        neu_zeit = str(eintrag.get('zeit') or '')
        if neu_zeit and (not alt_zeit or neu_zeit < alt_zeit):
            eintrag = dict(eintrag)
            eintrag.setdefault('quelle', da.get('quelle'))
            neu_bp[frisch] = eintrag
        elif RANG.get(eintrag.get('quelle'), 0) > RANG.get(da.get('quelle'), 0):
            da['quelle'] = eintrag.get('quelle')
    if geaendert:
        daten['bauplaene'] = neu_bp
    return geaendert


def laden():
    """Bestand von der Platte. Fehlt die Datei oder ist sie beschädigt, wird mit
    einem leeren Bestand weitergearbeitet — der Watcher soll nie am Start scheitern."""
    try:
        with open(pfad(), encoding='utf-8') as f:
            daten = json.load(f)
    except Exception:
        return leer()
    if not isinstance(daten.get('bauplaene'), dict):
        return leer()
    # ⚠ Erst umziehen, dann die Version hochsetzen — und nur dann schreiben,
    # wenn sich wirklich etwas geändert hat. Ein Schreibfehler darf den Start
    # nicht aufhalten: Der Bestand im Speicher stimmt dann trotzdem, nur der
    # Umzug wiederholt sich beim nächsten Mal.
    # ⚠ Gegen DATEI_VERSION pruefen, nicht gegen eine feste Zahl: Beim Sprung
    # auf 3 waere ein hart geschriebenes `< 2` stillschweigend wirkungslos
    # geblieben, und die Dubletten haetten ueberlebt.
    if daten.get('version', 1) < DATEI_VERSION:
        if _schluessel_erneuern(daten):
            daten['version'] = DATEI_VERSION
            try:
                speichern(daten)
            except Exception as ausnahme:
                fehler.merken('bestand.schluessel_erneuern', ausnahme)
        else:
            daten['version'] = DATEI_VERSION
    daten.setdefault('version', DATEI_VERSION)
    return daten


def zuruecksetzen():
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
    try:
        os.remove(pfad())
    except FileNotFoundError:
        return None
    except OSError as stoerung:
        return stoerung
    return None


def speichern(daten):
    """Schreibt den Bestand — mit Vorgängerfassung und ohne Halbfertiges.

    Erst in eine Nebendatei schreiben, dann umbenennen: Stürzt der Rechner
    mitten im Schreiben ab, ist die alte Datei noch vollständig da. Die
    Vorgängerfassung (`bestand.bak.json`) bleibt als Rückfall liegen."""
    daten['version'] = DATEI_VERSION
    daten['stand'] = _jetzt()
    ziel = pfad()
    temp = ziel + '.tmp'
    try:
        os.makedirs(os.path.dirname(ziel), exist_ok=True)
        with open(temp, 'w', encoding='utf-8') as f:
            json.dump(daten, f, ensure_ascii=False, indent=1, sort_keys=True)
        if os.path.exists(ziel):
            sicherung = ziel.replace('.json', '.bak.json')
            try:
                os.replace(ziel, sicherung)
            except OSError:
                pass
        os.replace(temp, ziel)
        _ablage_nachziehen(daten)
        return True
    except Exception as ausnahme:
        # Hier ist der eigene Bauplan-Bestand betroffen — das Wichtigste, was
        # das Werkzeug hat. Ein stiller Fehlschlag wäre nicht zu verzeihen.
        fehler.merken('bestand.speichern', ausnahme, ziel)
        try:
            os.remove(temp)
        except OSError:
            pass
        return False


def _ablage_nachziehen(daten):
    """Die drei Ausgabe-Dateien auf den neuen Stand bringen — still.

    ⚠ **Warum das hier hängt und nicht am Knopf.** Die Ausgabe-Dateien für das
    KRT Profit Basetool, für scmdb.net und die Vollsicherung wurden bisher
    **nur** geschrieben, wenn jemand auf „Alle drei in die Ablage" klickte.
    Wer das einmal gemacht hatte, hielt sie danach für aktuell — sie standen
    aber für immer auf dem Stand jenes Klicks. Aufgefallen ist es, als jemand das
    Werkzeug jemandem vorführte und selbst suchen musste, wo die Dateien
    herkommen (27.08.2026): „die werden ja bei drops direkt fortgeschrieben
    oder?" Nein — bis jetzt nicht.

    An `speichern()` hängt es, weil hier **jede** Bestandsänderung
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
        export.ablegen(daten)
    except Exception as ausnahme:
        fehler.merken('bestand.ablage_nachziehen', ausnahme)


ANHANG_RE = __import__('re').compile(r'\s*\([^()]*\)\s*$')


def katalogname(name, bekannt=None):
    """Den Namen so, wie ihn der Katalog kennt — ohne angehängte Angaben.

    ⚠⚠ **`bekannt` durchreichen, wenn viele Namen hintereinander laufen.**
    Ohne den Parameter holt sich diese Funktion den Katalog selbst — und
    `katalog.laden()` liest jedes Mal die ganze Datei (rund 1 MB). Bei einem
    Aufruf faellt das nicht auf, bei 406 hintereinander schon: Gemessen am
    04.09.2026 brauchte `angleichen()` dadurch **3,6 Sekunden** bei jedem
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
    if bekannt is None:
        from . import katalog
        try:
            bekannt = katalog.laden().get('bauplaene') or {}
        except Exception:
            return name
    if not bekannt or norm(name) in bekannt:
        return name
    ohne = ANHANG_RE.sub('', name).strip()
    if ohne and ohne != name and norm(ohne) in bekannt:
        return ohne
    return _eindeutiger_treffer(name, bekannt)


# Ab so vielen Wörtern darf über die Wortmenge zugeordnet werden.
MINDEST_WOERTER = 2


def _eindeutiger_treffer(name, bekannt):
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
    woerter = set(norm(name).split())
    if len(woerter) < MINDEST_WOERTER:
        return name
    treffer = [k for k in bekannt if woerter <= set(k.split())]
    if len(treffer) != 1:
        return name
    eintrag = bekannt[treffer[0]]
    return (eintrag.get('n') if isinstance(eintrag, dict) else None) or treffer[0]


def angleichen(daten):
    """Gespeicherte Namen nachträglich an den Katalog angleichen.

    Für alles, was vor dieser Berichtigung schon mit Anhang abgelegt wurde.
    Gibt die Zahl der berichtigten Einträge zurück; `0` heißt „nichts zu tun".
    """
    # ⚠ Den Katalog EINMAL holen und durchreichen — nicht je Bauplan neu.
    # Siehe `katalogname`: Ohne das las diese Schleife die 1-MB-Katalogdatei
    # einmal pro Eintrag und brauchte bei 406 Bauplaenen 3,6 Sekunden.
    from . import katalog
    try:
        bekannt = katalog.laden().get('bauplaene') or {}
    except Exception:
        return 0

    berichtigt = 0
    for schluessel in list(daten['bauplaene']):
        eintrag = daten['bauplaene'][schluessel]
        alt_name = eintrag.get('name') or schluessel
        neu_name = katalogname(alt_name, bekannt)
        if neu_name == alt_name:
            continue
        daten['bauplaene'].pop(schluessel)
        neuer = norm(neu_name)
        # Gibt es den Bauplan schon unter dem richtigen Namen, bleibt der
        # ältere Eintrag stehen — er hat den früheren Fundzeitpunkt.
        if neuer not in daten['bauplaene']:
            eintrag['name'] = neu_name
            daten['bauplaene'][neuer] = eintrag
        berichtigt += 1
    return berichtigt


def hinzufuegen(daten, name, quelle='log', zeit=None):
    """Einen Bauplan aufnehmen. Gibt True zurück, wenn er vorher nicht drin war.

    Ein schon bekannter Bauplan wird nicht doppelt angelegt; steht die neue
    Quelle höher (z. B. `launcher` statt `log`), wird sie nachgetragen.

    ⚠ Der Name läuft vorher durch `katalogname()` — siehe dort, warum.
    """
    name = katalogname(name)
    schluessel = norm(name)
    if not schluessel:
        return False
    eintrag = daten['bauplaene'].get(schluessel)
    if eintrag is None:
        daten['bauplaene'][schluessel] = {
            'name': name.strip(),
            'quelle': quelle,
            'zeit': zeit or _jetzt(),
        }
        return True
    if RANG.get(quelle, 0) > RANG.get(eintrag.get('quelle'), 0):
        eintrag['quelle'] = quelle
    return False


def entfernen(daten, name):
    """Häkchen wieder wegnehmen (Verwaltungsfenster)."""
    return daten['bauplaene'].pop(norm(name), None) is not None


def enthaelt(daten, name):
    return norm(name) in daten['bauplaene']


def schluessel(daten):
    """Alle Namen in Vergleichsform — als Menge, für schnelle Abgleiche."""
    return set(daten['bauplaene'])


def namen(daten):
    """Die Namen in Schreibweise wie gefunden, alphabetisch."""
    return sorted((e.get('name') or k) for k, e in daten['bauplaene'].items())


def anzahl(daten):
    return len(daten['bauplaene'])


def nach_quelle(daten):
    """Wie viele Baupläne kommen woher — für die Statusanzeige."""
    zaehler = {}
    for e in daten['bauplaene'].values():
        q = e.get('quelle') or 'unbekannt'
        zaehler[q] = zaehler.get(q, 0) + 1
    return zaehler


if __name__ == '__main__':
    b = laden()
    print('Datei:  ', pfad())
    print('Anzahl: ', anzahl(b))
    print('Quellen:', nach_quelle(b) or '—')
