# SPDX-License-Identifier: GPL-3.0-only
#
# SC BP Watcher — Joysticks und ihre Reihenfolge
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
Welcher Stick ist welche Nummer — und stimmt das noch?

## Das Problem

Star Citizen speichert Tastenbelegungen nicht am Geraet, sondern an einer
**Nummer**: `js1_button10`. Welcher Stick `js1` ist, entscheidet die
Reihenfolge, in der die Geraete gefunden werden. Aendert sie sich — nach einem
Neustart, einem Windows-Update, einem anderen USB-Anschluss —, sitzt die
komplette Belegung am falschen Stick. Wer zwei baugleiche Sticks fliegt
(HOSAS), erlebt das frueher oder spaeter.

## Die beiden Quellen

**Das Spiel schreibt seine eigene Reihenfolge mit.** Ganz oben in jeder
`Game.log`, noch vor den Audiogeraeten:

    - Connected joystick0: <Geraetename>  {AAAAAAAA-0000-0000-0000-504944564944}
    - Connected joystick1: <Geraetename>  {BBBBBBBB-0000-0000-0000-504944564944}

⭐ **Das ist der ganze Trick.** Es ist die Reihenfolge, die *das Spiel*
benutzt — nicht die, die Windows oder Python melden wuerden. Damit braucht es
keine Geraeteabfrage: kein DirectInput, kein `ctypes`, kein getrennter
Windows-/Linux-Weg, kein Fremdpaket. Zwei Textdateien genuegen.

Die zweite ist die `actionmaps.xml` des Spielers. Dort steht, welche Nummer
das Spiel welchem Geraet zugeordnet hat:

    <options type="joystick" instance="1" Product="<Geraetename> {AAAAAAAA-...}">

## ⚠ Ueber die Kennung gehen, nie ueber den Namen

Dieselbe Kennung kann in beiden Dateien unter **verschiedenen Namen** stehen:
Die Geraetesoftware kuerzt sie unterschiedlich ab, und der Spieler darf sie
umbenennen. An einem echten Aufbau gemessen unterschieden sich die Namen
desselben Geraets in Protokoll und Belegung. Wer Geraete am Namen
wiedererkennt, baut auf Sand — die geschweifte Kennung ist der einzige feste
Bezugspunkt.

## ⚠⚠ Umgeschrieben wird per Textersetzung, NICHT ueber den XML-Baum

`xml.etree` kann die Datei lesen, aber nicht unveraendert zurueckschreiben:
Es ordnet Attribute um, wirft Kommentare weg und formatiert Einrueckungen neu.
Bei einer Datei, die das Spiel selbst pflegt, ist das ein unnoetiges Risiko —
eine kaputte `actionmaps.xml` kostet den Spieler seine komplette Belegung.

Deshalb: **lesen** mit `ElementTree` (robust gegen Formatierungsfragen),
**schreiben** mit einer gezielten Textersetzung, die ausser der einen Kennung
nichts anfasst.

## ⚠⚠ Und die Nummern werden NICHT umsortiert

Der erste Entwurf wollte genau das: Position im Protokoll mit Nummer in der
Belegung vergleichen und bei Abweichung alles durchnummerieren. **Das war
falsch** — die Begruendung steht ausfuehrlich ueber `vergleich()`. Kurz: Das
Spiel erkennt seine Geraete an der gespeicherten Kennung wieder, nicht an der
Fundreihenfolge. Wer die Nummern anfasst, zerstoert eine gesunde Belegung.

Repariert wird deshalb nur der eine Fall, in dem wirklich etwas kaputt ist:
ein Geraet meldet sich unter **neuer Kennung** (`kennung_tauschen`).

## Was dieses Modul bewusst NICHT tut

**Es schreibt nichts von allein.** Der Vergleich laeuft mit, das Reparieren
ist ein Knopf. Ein Automatismus, der die Datei anfasst, an der die komplette
Steuerung des Spielers haengt, muesste sich seiner Sache sehr sicher sein —
und diese Sicherheit gibt die Datenlage nicht her.

Ebenfalls nicht: die Datei sperren, damit das Spiel sie nicht ueberschreibt.
Das tun andere Werkzeuge; es ist genau die Sorte Verhalten, bei der
Virenscanner anschlagen.
"""
import json
import os
import re
import shutil
import time
import xml.etree.ElementTree as ET

from . import pfade

# Die Zeile, die das Spiel beim Start schreibt. Der Name darf Leerzeichen
# enthalten, die Kennung steht in geschweiften Klammern dahinter.
#
# ⚠ Der Name wird "nicht gierig" gelesen (`.+?`) und die Leerzeichen davor
# abgeschnitten: Zwischen Name und Kennung stehen im echten Log zwei
# Leerzeichen, bei anderen Geraeten eines.
VERBUNDEN = re.compile(
    r'Connected joystick(\d+):\s*(.+?)\s*\{([0-9A-Fa-f-]+)\}')

# Aus einer Kennung in der `actionmaps.xml` das reine Kennungs-Teil holen.
KENNUNG_IM_NAMEN = re.compile(r'\{([0-9A-Fa-f-]+)\}')

# Jede Eingabe-Vorsilbe in der actionmaps.xml: js1_button10, js2_x, js3_hat1_up
JS_VORSILBE = re.compile(r'\bjs(\d+)_')

# Wieviele Joystick-Plaetze Star Citizen kennt. Mehr als acht meldet das Spiel
# selbst als Grenzfall; die `actionmaps.xml` legt acht leere Plaetze an.
PLAETZE = 8


def _pfad_actionmaps(ordner=None):
    """Wo die Belegungsdatei des Spielers liegt.

    ⚠ Der Ordner heisst je nach Installation `USER` oder `user` — unter Linux
    (Wine, Dateisystem unterscheidet Gross- und Kleinschreibung) sind beide
    Formen schon aufgetreten, teilweise nebeneinander. Deshalb wird gesucht
    statt geraten.
    """
    basis = ordner or pfade.spiel_ordner()
    if not basis:
        return None
    unten = os.path.join('Client', '0', 'Profiles', 'default',
                         'actionmaps.xml')
    for oben in ('USER', 'user'):
        weg = os.path.join(basis, oben, unten)
        if os.path.isfile(weg):
            return weg
    return None


def geraete_aus_text(text):
    """Die verbundenen Geraete aus einem Log-Text, in Fundreihenfolge.

    Liefert je Geraet ein Woerterbuch mit `platz` (die Zahl, die das Spiel
    vergibt), `name` und `kennung`.
    """
    gefunden = []
    gesehen = set()
    for treffer in VERBUNDEN.finditer(text or ''):
        platz = int(treffer.group(1))
        kennung = treffer.group(3).upper()
        # ⚠ Innerhalb einer Sitzung kann dieselbe Zeile mehrfach auftauchen
        # (Neuverbinden im laufenden Spiel). Der erste Fund gilt.
        if platz in gesehen:
            continue
        gesehen.add(platz)
        gefunden.append({'platz': platz,
                         'name': treffer.group(2).strip(),
                         'kennung': kennung})
    gefunden.sort(key=lambda g: g['platz'])
    return gefunden


def geraete(ordner=None):
    """Die Geraete aus dem neuesten Protokoll des Spiels.

    Zuerst die laufende `Game.log`; steht dort nichts (das Spiel lief seit dem
    letzten Einloggen nicht), wird die neueste Sicherung genommen. Ohne
    Spielstart gibt es keine Geraeteliste — dann bleibt die Liste leer, und
    die Oberflaeche sagt das auch so.
    """
    dateien = []
    laufend = pfade.game_log(ordner)
    if laufend and os.path.isfile(laufend):
        dateien.append(laufend)
    try:
        dateien.extend(pfade.log_sicherungen(ordner) or [])
    except Exception:
        pass
    for datei in dateien:
        try:
            # Die Geraetezeilen stehen in den ersten Hundert Zeilen. Eine
            # 13-MB-Datei dafuer ganz zu lesen waere Verschwendung — beim
            # Oeffnen der Seite faellt das sofort auf.
            with open(datei, 'r', encoding='utf-8', errors='replace') as f:
                kopf = f.read(200000)
        except Exception:
            continue
        treffer = geraete_aus_text(kopf)
        if treffer:
            return treffer
    return []


def zuordnung(datei=None, ordner=None):
    """Welche Nummer in der `actionmaps.xml` welchem Geraet gehoert.

    Liefert je belegtem Platz ein Woerterbuch mit `nummer` (die `instance`,
    also das `n` in `js<n>_`), `name` und `kennung`. Leere Plaetze
    (`<options type="joystick" instance="7"/>`) kommen nicht mit — sie sagen
    nichts aus.
    """
    weg = datei or _pfad_actionmaps(ordner)
    if not weg or not os.path.isfile(weg):
        return []
    try:
        baum = ET.parse(weg)
    except Exception:
        # Eine kaputte oder halb geschriebene Datei ist kein Grund
        # abzustuerzen — die Seite zeigt dann „nicht lesbar".
        return []
    heraus = []
    for knoten in baum.getroot().iter('options'):
        if (knoten.get('type') or '').lower() != 'joystick':
            continue
        produkt = knoten.get('Product') or ''
        if not produkt.strip():
            continue
        try:
            nummer = int(knoten.get('instance') or 0)
        except ValueError:
            continue
        kennung = KENNUNG_IM_NAMEN.search(produkt)
        heraus.append({
            'nummer': nummer,
            'name': KENNUNG_IM_NAMEN.sub('', produkt).strip(),
            'kennung': (kennung.group(1).upper() if kennung else ''),
        })
    heraus.sort(key=lambda z: z['nummer'])
    return heraus


# Die Zustaende, die ein Vergleich haben kann.
PASST   = 'passt'    # jedes belegte Geraet ist verbunden — alles in Ordnung
ERSETZT = 'ersetzt'  # ein Geraet meldet sich unter NEUER Kennung (reparierbar)
FEHLT   = 'fehlt'    # ein belegtes Geraet ist gar nicht verbunden
LEER    = 'leer'     # keine Daten (noch nie gespielt, Datei fehlt)


# ⚠⚠ **Die Position im Protokoll ist NICHT die Nummer in der Belegung.**
#
# Gemessen am 04.09.2026 an einem laufenden Aufbau: Das Protokoll meldet
# `joystick0` als linken Stick, waehrend die `actionmaps.xml` `instance="1"`
# (also `js1`) dem **rechten** zuordnet — und die Belegung funktioniert
# trotzdem einwandfrei im Spiel.
#
# Daraus folgt zwingend: **Star Citizen erkennt seine Geraete an der
# gespeicherten Kennung wieder, nicht an der Fundreihenfolge.** Ein Stick, der
# heute an anderer Stelle auftaucht, behaelt seine Nummer und damit seine
# Belegung.
#
# Der erste Entwurf dieses Moduls hat genau das falsch gemacht: Er verglich
# Position mit Nummer, meldete einen voellig gesunden Aufbau als „verrutscht"
# und haette beim Umschreiben **alle drei Geraete durchgetauscht** — aus einer
# funktionierenden Belegung waere Schrott geworden. Der Fehler faellt nur auf,
# wenn man gegen echte Dateien prueft; die Rechnung fuer sich sah stimmig aus.
#
# **Was wirklich schiefgehen kann**, ist etwas anderes: Aendert sich die
# Kennung eines Geraets — anderer USB-Anschluss, neue Firmware, Tausch —, dann
# erkennt das Spiel es nicht wieder, legt es als neues Geraet mit freier Nummer
# an, und die alte Belegung haengt an einer Kennung, die es nicht mehr gibt.
# Spuren davon stehen im Testaufbau: drei `deviceoptions`-Bloecke mit
# demselben Geraetenamen und drei verschiedenen Kennungen.
#
# Genau diesen Fall — und nur diesen — meldet `ERSETZT`.


def vergleich(ordner=None, datei=None):
    """Ist jedes belegte Geraet noch da — und unter derselben Kennung?

    Das Ergebnis traegt alles, was die Oberflaeche braucht:

    | Feld | Bedeutung |
    |---|---|
    | `zustand` | `passt`, `ersetzt`, `fehlt` oder `leer` |
    | `geraete` | was das Spiel zuletzt verbunden hat |
    | `zuordnung` | was in der `actionmaps.xml` steht |
    | `fehlende` | belegte Geraete, die gerade nicht verbunden sind |
    | `neue` | verbundene Geraete ohne Belegung |
    | `ersatz` | `[(alter Eintrag, neues Geraet)]` — eindeutige Faelle |

    **`ersatz` ist bewusst vorsichtig gefuellt:** nur wenn genau **ein**
    belegtes Geraet fehlt und genau **ein** neues dazugekommen ist. Dann ist
    die Zuordnung ohne Raten eindeutig. Bei mehreren gleichzeitig entscheidet
    der Spieler, nicht das Programm — ein falsch geratener Ersatz vertauscht
    zwei Sticks, und das merkt man erst im Gefecht.

    ⚠ Ueber den **Namen** laeuft dabei nichts: Dasselbe Geraet steht in
    Protokoll und Belegung durchaus unter verschiedenen Schreibweisen (die
    eine kuerzt „links" zu einem Buchstaben, die andere schreibt es aus). Ein
    Namensvergleich waere Ratearbeit mit gutem Gefuehl.
    """
    gefunden = geraete(ordner)
    gespeichert = zuordnung(datei, ordner)
    ergebnis = {'zustand': LEER, 'geraete': gefunden,
                'zuordnung': gespeichert, 'fehlende': [], 'neue': [],
                'ersatz': [], 'datei': datei or _pfad_actionmaps(ordner)}
    if not gefunden or not gespeichert:
        return ergebnis

    belegte = {z['kennung'] for z in gespeichert if z['kennung']}
    verbunden = {g['kennung'] for g in gefunden}

    ergebnis['fehlende'] = [z for z in gespeichert
                            if z['kennung'] and z['kennung'] not in verbunden]
    ergebnis['neue'] = [g for g in gefunden if g['kennung'] not in belegte]

    if len(ergebnis['fehlende']) == 1 and len(ergebnis['neue']) == 1:
        ergebnis['ersatz'] = [(ergebnis['fehlende'][0], ergebnis['neue'][0])]
        ergebnis['zustand'] = ERSETZT
    elif ergebnis['fehlende']:
        ergebnis['zustand'] = FEHLT
    else:
        ergebnis['zustand'] = PASST
    return ergebnis


# Jede Geraeteart, die in der `actionmaps.xml` vorkommt, mit ihrer Vorsilbe.
# ⚠ Die Reihenfolge ist die, in der die Geraete in der Oberflaeche erscheinen.
ARTEN = (('joystick', 'js'), ('tastatur', 'kb'), ('maus', 'mo'),
         ('gamepad', 'gp'))
VORSILBE = re.compile(r'^(js|kb|mo|gp)(\d+)_')


def belegungen(datei=None, ordner=None):
    """Was auf den Geraeten liegt — je Geraet eine Liste von Belegungen.

    ⭐ **Das geht fuer JEDES Geraet, ohne eine einzige Geraetevorlage.** Die
    `actionmaps.xml` sagt selbst, welcher Knopf welche Aktion ausloest; ob der
    Stick von Virpil, VKB, Thrustmaster oder von einem Hersteller stammt, den
    niemand kennt, spielt keine Rolle. Vorlagen braucht erst, wer die Knoepfe
    auf einem **Bild** zeigen will.

    **Tastatur und Maus sind mit dabei** (Wunsch von Morkhan, 04.09.2026) —
    sie stehen in derselben Datei und unterscheiden sich nur in der Vorsilbe.
    Wer nachsehen will, welche Taste was tut, muss dafuer nicht ins Spiel.

    Liefert `{kennzeichen: [{…}, …]}` mit `kennzeichen` wie `js1`, `kb1`, `mo1`.
    Je Eintrag:

    * `eingabe` — was gedrueckt wird, ohne Vorsilbe: `button10`, `x`, `f5`
    * `aktion` — der Name, den das Spiel vergibt: `v_eject`
    * `bereich` — die Gruppe drumherum: `spaceship_movement`
    * `art` — `joystick`, `tastatur`, `maus` oder `gamepad`
    """
    weg = datei or _pfad_actionmaps(ordner)
    if not weg or not os.path.isfile(weg):
        return {}
    try:
        baum = ET.parse(weg)
    except Exception:
        return {}
    nach_vorsilbe = {kuerzel: art for art, kuerzel in ARTEN}
    heraus = {}
    for gruppe in baum.getroot().iter('actionmap'):
        bereich = gruppe.get('name') or ''
        for aktion in gruppe.iter('action'):
            name = aktion.get('name') or ''
            for bindung in aktion.iter('rebind'):
                eingabe = (bindung.get('input') or '').strip()
                treffer = VORSILBE.match(eingabe)
                if not treffer:
                    continue
                kennzeichen = treffer.group(1) + treffer.group(2)
                heraus.setdefault(kennzeichen, []).append({
                    'eingabe': eingabe[treffer.end():],
                    'aktion': name,
                    'bereich': bereich,
                    'art': nach_vorsilbe.get(treffer.group(1), ''),
                })
    for liste in heraus.values():
        # Achsen zuerst, dann Knoepfe nach Nummer, dann der Rest — dieselbe
        # Reihenfolge, in der man ein Geraet auch anschaut.
        liste.sort(key=_sortierschluessel)
    return heraus


def art_von(kennzeichen):
    """Aus `js1` wird `joystick`, aus `kb1` `tastatur`."""
    for art, kuerzel in ARTEN:
        if kennzeichen.startswith(kuerzel):
            return art
    return ''


# Welches Feld der `defaultProfile.xml` zu welcher Vorsilbe gehoert.
STANDARD_FELD = {'keyboard': 'kb1', 'joystick': 'js1', 'mouse': 'mo1',
                 'gamepad': 'gp1'}

# Die drei Sichten, die es zu sehen gibt.
MEINE    = 'meine'     # nur, was der Spieler selbst geaendert hat
STANDARD = 'standard'  # nur die Werkseinstellung des Spiels
ALLES    = 'alles'     # beides zusammengefuehrt — die wirkliche Belegung


def standardbelegungen(spielordner=None):
    """Die Werkseinstellung des Spiels, im Format von `belegungen()`.

    ⚠ Der Standard kennt nur **ein** Geraet je Art (`js1`, `kb1` …) — das
    Spiel legt seine Vorgaben nicht je angeschlossenem Stick ab. Wer zwei
    Sticks fliegt, findet die Vorgaben deshalb komplett unter `js1`.
    """
    profil = _profil(spielordner) or {}
    standard = profil.get('standard') or {}
    # ⚠ **Nur Aktionen, die das Spiel selbst benennt.** Ohne `UILabel` taucht
    # eine Aktion auch in den Spieloptionen nicht auf — es sind interne und
    # Entwickler-Befehle (`retry`, `flycam_play`, `hacking_minigame_abort`).
    # Sie mit anzuzeigen blaeht die Liste um rund 180 Zeilen auf, die niemand
    # belegen kann. Eigene Belegungen bleiben davon unberuehrt: Was der
    # Spieler selbst eingetragen hat, wird immer gezeigt.
    benannt = profil.get('etiketten') or {}
    heraus = {}
    for aktion, felder in standard.items():
        if not (benannt.get(aktion) or [''])[0]:
            continue
        for feld, eingabe in felder.items():
            kennzeichen = STANDARD_FELD.get(feld)
            if not kennzeichen or not eingabe:
                continue
            # Im Standard steht die Eingabe teils mit, teils ohne Vorsilbe.
            treffer = VORSILBE.match(eingabe)
            rein = eingabe[treffer.end():] if treffer else eingabe
            if treffer:
                kennzeichen = treffer.group(1) + treffer.group(2)
            heraus.setdefault(kennzeichen, []).append({
                'eingabe': rein,
                'aktion': aktion,
                'bereich': '',
                'art': art_von(kennzeichen),
                'quelle': STANDARD,
            })
    for liste in heraus.values():
        liste.sort(key=_sortierschluessel)
    return heraus


def sicht(welche=ALLES, datei=None, ordner=None):
    """Die Belegungen in einer der drei Sichten.

    | Sicht | Was drinsteht |
    |---|---|
    | `meine` | nur die eigene `actionmaps.xml` — was der Spieler umgestellt hat |
    | `standard` | nur die Werkseinstellung aus der `defaultProfile.xml` |
    | `alles` | beides zusammen; eigene Aenderungen **ersetzen** den Standard |

    ⚠⚠ **Beim Zusammenfuehren gewinnt der Spieler, und zwar je Aktion.**
    Hat er eine Aktion selbst belegt, faellt die Werksvorgabe dafuer weg —
    sonst staende dieselbe Aktion zweimal da, einmal richtig und einmal
    falsch, und niemand wuesste welche gilt.

    ⚠ Eine eigene Belegung mit **leerer** Eingabe ist eine geloeschte: Der
    Spieler hat die Werksvorgabe bewusst entfernt. Sie verdraengt den
    Standard, erscheint aber selbst nicht in der Liste — genau wie im Spiel.
    """
    eigene = belegungen(datei, ordner)
    if welche == MEINE:
        for liste in eigene.values():
            for e in liste:
                e['quelle'] = MEINE
        return {k: [e for e in v if e['eingabe']] for k, v in eigene.items()}
    if welche == STANDARD:
        return standardbelegungen(ordner)

    # Zusammenfuehren: erst merken, welche Aktionen der Spieler angefasst hat.
    angefasst = set()
    for liste in eigene.values():
        for e in liste:
            angefasst.add(e['aktion'])

    heraus = {}
    for kennzeichen, liste in standardbelegungen(ordner).items():
        rest = [dict(e) for e in liste if e['aktion'] not in angefasst]
        if rest:
            heraus[kennzeichen] = rest
    for kennzeichen, liste in eigene.items():
        for e in liste:
            if not e['eingabe']:
                continue           # geloeschte Belegung — nichts anzuzeigen
            neu = dict(e)
            neu['quelle'] = MEINE
            heraus.setdefault(kennzeichen, []).append(neu)
    for liste in heraus.values():
        liste.sort(key=_sortierschluessel)
    return heraus


def _sortierschluessel(eintrag):
    """Achsen vor Knoepfen, Knoepfe nach Zahl statt nach Text.

    Ohne das steht `button10` vor `button2`, was beim Nachschlagen jedes Mal
    stolpern laesst.

    ⚠ **Nur bei Sticks.** Auf einer Tastatur sind `x`, `y` und `z` schlicht
    Buchstaben — die als Achsen nach vorn zu sortieren, stellt die halbe
    Tastatur an den Anfang. Tastatur und Maus werden deshalb alphabetisch
    sortiert.
    """
    e = eintrag['eingabe']
    if eintrag.get('art') in ('tastatur', 'maus'):
        return (0, 0, e)
    achsen = ('x', 'y', 'z', 'rotx', 'roty', 'rotz')
    if e in achsen:
        return (0, achsen.index(e), '')
    if e.startswith('slider'):
        return (1, _zahl_am_ende(e), e)
    if e.startswith('button'):
        return (2, _zahl_am_ende(e), e)
    return (3, 0, e)


def _zahl_am_ende(text):
    treffer = re.search(r'(\d+)', text)
    return int(treffer.group(1)) if treffer else 0


# ---------------------------------------------------------------- Klarnamen
#
# `v_eject` sagt niemandem etwas. „Aussteigen" schon. Die Kette dorthin ist
# dreistufig und fuehrt ausschliesslich ueber Dateien, die auf dem Rechner des
# Spielers ohnehin liegen:
#
#     actionmaps.xml     v_eject
#       └─ defaultProfile.xml   UILabel="@ui_CIEject"
#            └─ global.ini      ui_CIEject=Aussteigen   (bzw. Eject)
#
# ⭐ **Und damit ist es zweisprachig, ohne dass wir etwas uebersetzen.** Die
# `global.ini` liegt je Sprache einmal im Spielordner; welche gelesen wird,
# richtet sich nach der eingestellten Programmsprache — nicht nach der
# Spielsprache. Wer den englischen Client fährt, aber die Oberflaeche auf
# Deutsch hat, bekommt deutsche Aktionsnamen.
#
# ⚠ Die `defaultProfile.xml` steckt im `Data.p4k` und ist **CryXmlB**, kein
# Klartext-XML (siehe `scbp/cryxml.py`). Das Verzeichnis des Archivs ist
# 442 MB gross — deshalb wird das Ergebnis in der Ablage gemerkt und nur neu
# geholt, wenn der Spielstand sich geaendert hat.

# Welcher Ordner der Lokalisierung zu welcher Programmsprache gehoert.
# ⚠ Deutsch heisst dort `german_(germany)`, mit Unterstrichen und Klammern.
INI_ORDNER = {'de': ('german_(germany)', 'german'), 'en': ('english',)}

_KLARNAMEN = {}          # {sprache: {aktion: (name, beschreibung)}}


# ⚠ Aendert sich, was `_profil()` merkt, muss diese Zahl hoch — sonst liest
# eine neue Fassung den Merker der alten und findet die neuen Felder nicht.
MERK_FASSUNG = 2


def _profil(spielordner=None):
    """Alles, was in der `defaultProfile.xml` des Spiels steht.

    Zwei Dinge in einem Durchgang, weil beide aus derselben Datei kommen:

    | Schluessel | Inhalt |
    |---|---|
    | `etiketten` | Aktion → `[UILabel, UIDescription]` — die Klarnamen |
    | `standard` | Aktion → `{'keyboard': 'ralt+y', 'joystick': …}` |

    ⭐ **`standard` ist der Grund, warum die Liste ueberhaupt vollstaendig
    sein kann.** Die `actionmaps.xml` des Spielers enthaelt naemlich nur seine
    **Abweichungen** vom Standard — wer nichts umgestellt hat, hat dort auch
    nichts stehen, und eine Liste allein daraus waere fast leer. Erst beide
    zusammen ergeben „was tut welche Taste".

    Gemerkt wird das Ergebnis in `Intern/aktionsnamen.json`: Das Archiv
    einmal aufzuschlagen dauert spuerbar, und die Daten aendern sich nur mit
    einem Spiel-Patch.
    """
    from . import fehler
    leer = {'etiketten': {}, 'standard': {}}
    merk = pfade.app_datei('aktionsnamen.json')
    stand = ''
    try:
        p4k = os.path.join(spielordner or pfade.spiel_ordner() or '',
                           'Data.p4k')
        if os.path.isfile(p4k):
            stand = str(int(os.path.getmtime(p4k)))
    except Exception:
        pass
    try:
        if os.path.isfile(merk):
            with open(merk, 'r', encoding='utf-8') as f:
                gemerkt = json.load(f)
            if (gemerkt.get('fassung') == MERK_FASSUNG
                    and gemerkt.get('stand') == stand
                    and gemerkt.get('etiketten')):
                return gemerkt
    except Exception:
        pass

    from . import cryxml, spieltexte
    heraus = {'fassung': MERK_FASSUNG, 'stand': stand,
              'etiketten': {}, 'standard': {}}
    try:
        p4k = spieltexte.p4k_pfad(spielordner)
        with open(p4k, 'rb') as f:
            verzeichnis, _ = spieltexte.lies_verzeichnis(
                f, os.path.getsize(p4k))
            methode, cs, rs, off = spieltexte.suche(
                verzeichnis, 'Data/Libs/Config/defaultProfile.xml')
            roh = spieltexte.hole_block(f, off, cs)
        daten = (spieltexte.entpacke_zstd(roh, rs)[0] if methode == 100
                 else __import__('zlib').decompress(roh, -15))
        wurzel = cryxml.lesen(daten)
        for knoten in cryxml.alle(wurzel, 'action'):
            at = knoten.get('attribute') or {}
            name = at.get('name')
            if not name:
                continue
            heraus['etiketten'][name] = [at.get('UILabel', ''),
                                         at.get('UIDescription', '')]
            vorgabe = {}
            for feld in ('keyboard', 'joystick', 'mouse', 'gamepad'):
                wert = (at.get(feld) or '').strip()
                # ⚠ Ein leeres Feld heisst „ab Werk nicht belegt" und ist
                # etwas anderes als „gar kein Feld". Beides kommt vor.
                if wert:
                    vorgabe[feld] = wert
            if vorgabe:
                heraus['standard'][name] = vorgabe
    except Exception as ausnahme:
        # Ohne diese Datei bleibt die Liste benutzbar — dann stehen dort die
        # technischen Namen und nur die eigenen Aenderungen. Schlechter, aber
        # nicht kaputt.
        fehler.merken('joysticks.profil', ausnahme)
        return leer

    try:
        pfade.json_sichern(merk, heraus)
    except Exception:
        pass
    return heraus


def _ini_texte(sprache, spielordner=None):
    """Die `ui_…`-Zeilen der `global.ini` in der gewuenschten Sprache.

    Gelesen werden nur Zeilen, die mit `ui_` beginnen — die Datei hat rund
    12 MB, und alles andere wird hier nicht gebraucht.
    """
    basis = os.path.join(spielordner or pfade.spiel_ordner() or '',
                         'data', 'Localization')
    if not os.path.isdir(basis):
        basis = os.path.join(spielordner or pfade.spiel_ordner() or '',
                             'Data', 'Localization')
    heraus = {}
    for ordner in INI_ORDNER.get(sprache, ('english',)):
        weg = os.path.join(basis, ordner, 'global.ini')
        if not os.path.isfile(weg):
            continue
        try:
            with open(weg, 'r', encoding='utf-8', errors='replace') as f:
                for zeile in f:
                    if not zeile.startswith('ui_'):
                        continue
                    schluessel, _, wert = zeile.partition('=')
                    if wert:
                        heraus[schluessel.strip()] = wert.strip()
        except Exception:
            continue
        if heraus:
            break
    return heraus


def klarnamen(sprache='de', spielordner=None):
    """Aktion → (lesbarer Name, Beschreibung) in der gewuenschten Sprache.

    Fehlt eine der Quellen, kommt ein leeres Woerterbuch zurueck und die
    Oberflaeche zeigt weiter die technischen Namen. **Kein Raten:** Ein
    Etikett ohne Eintrag in der `global.ini` bleibt weg, statt aus dem
    Schluessel einen huebschen Namen zu basteln.
    """
    merk = _KLARNAMEN.get(sprache)
    if merk is not None:
        return merk
    etiketten = (_profil(spielordner) or {}).get('etiketten') or {}
    texte = _ini_texte(sprache, spielordner)
    heraus = {}
    if etiketten and texte:
        for aktion, paar in etiketten.items():
            label, beschreibung = (paar + ['', ''])[:2]
            name = texte.get((label or '').lstrip('@'), '')
            hinweis = texte.get((beschreibung or '').lstrip('@'), '')
            if name:
                heraus[aktion] = (name, hinweis)
    _KLARNAMEN[sprache] = heraus
    return heraus


def vergessen():
    """Den Merker leeren — nach einem Sprachwechsel."""
    _KLARNAMEN.clear()


def _actionmap_finden(wurzel, bereich):
    """Den `<actionmap>`-Block einer Gruppe holen oder anlegen."""
    for knoten in wurzel.iter('actionmap'):
        if (knoten.get('name') or '') == bereich:
            return knoten
    neu = ET.SubElement(wurzel, 'actionmap')
    neu.set('name', bereich)
    return neu


def belegen(aktion, bereich, kennzeichen, eingabe, datei=None, ordner=None):
    """Eine Aktion auf eine Eingabe legen — in der `actionmaps.xml` des Spielers.

    | | |
    |---|---|
    | `aktion` | `v_eject` |
    | `bereich` | `spaceship_general` — die Gruppe, in der die Aktion lebt |
    | `kennzeichen` | `js2`, `kb1`, `mo1` |
    | `eingabe` | `button10`, `f5`, `lalt+y` — **ohne** Vorsilbe |

    Eine **leere** `eingabe` loescht die Belegung: Das Spiel versteht
    `input=""` als „bewusst nicht belegt" und nimmt dann auch nicht die
    Werkseinstellung. Genau so macht es das Spiel selbst.

    ⚠⚠ **Es wird immer nur EIN `rebind` je Aktion und Geraet geschrieben.**
    Star Citizen erlaubt mehrere, aber eine zweite Belegung derselben Aktion
    auf demselben Geraet ist fast nie gewollt — und wer sie unbemerkt anlegt,
    bekommt zwei Zeilen, von denen nur eine wirkt. Bestehende Eintraege
    desselben Geraets werden deshalb ersetzt, nicht ergaenzt. Belegungen auf
    **anderen** Geraeten bleiben unangetastet.

    ⚠ **Nur bei geschlossenem Spiel aufrufen** — Star Citizen schreibt die
    Datei beim Beenden selbst und wuerde die Aenderung ueberschreiben.

    Liefert `(erfolg, meldung, anzahl)`; `meldung` ist bei Erfolg der Pfad der
    Sicherung, sonst ein Sprachschluessel.
    """
    from . import fehler

    weg = datei or _pfad_actionmaps(ordner)
    if not weg or not os.path.isfile(weg):
        return False, 's_js_f_datei', 0
    if not aktion or not kennzeichen:
        return False, 's_js_f_nichts', 0
    treffer = re.match(r'^([a-z]+)(\d+)$', kennzeichen)
    if not treffer:
        return False, 's_js_f_nichts', 0
    vorsilbe = treffer.group(1)

    try:
        baum = ET.parse(weg)
    except Exception as ausnahme:
        fehler.merken('joysticks.belegen_lesen', ausnahme)
        return False, 's_js_f_lesen', 0
    wurzel = baum.getroot()

    # Das Spiel legt die Aktionen unter `<ActionProfiles>` ab, nicht direkt
    # unter der Wurzel. Fehlt der Block, ist die Datei nicht die, für die wir
    # sie halten — dann lieber abbrechen.
    eltern = wurzel.find('ActionProfiles')
    if eltern is None:
        eltern = wurzel
    gruppe = _actionmap_finden(eltern, bereich or 'spaceship_general')

    ziel = None
    for knoten in gruppe.findall('action'):
        if (knoten.get('name') or '') == aktion:
            ziel = knoten
            break
    if ziel is None:
        ziel = ET.SubElement(gruppe, 'action')
        ziel.set('name', aktion)

    voll = ('%s_%s' % (kennzeichen, eingabe)) if eingabe else (kennzeichen + '_')
    ersetzt = False
    for bindung in list(ziel.findall('rebind')):
        vorhanden = (bindung.get('input') or '').strip()
        art = VORSILBE.match(vorhanden)
        # Nur Eintraege desselben Geraetetyps anfassen — eine Tastenbelegung
        # darf beim Setzen einer Stick-Belegung nicht verschwinden.
        if art and art.group(1) == vorsilbe:
            if ersetzt:
                ziel.remove(bindung)
            else:
                bindung.set('input', voll)
                ersetzt = True
        elif not vorhanden and not art:
            ziel.remove(bindung)
    if not ersetzt:
        neu = ET.SubElement(ziel, 'rebind')
        neu.set('input', voll)

    return _schreiben(weg, baum, 1)


def _schreiben(weg, baum, anzahl):
    """Den geaenderten Baum sichern und zurueckschreiben.

    ⚠ Hier **muss** ueber den XML-Baum geschrieben werden — anders als beim
    Kennungstausch, der eine reine Textersetzung ist. Deshalb entsteht vorher
    immer eine Sicherung: Geht etwas schief, ist der Rueckweg ein Umbenennen.
    """
    from . import fehler
    sicherung = '%s.scbpw-%s' % (weg, time.strftime('%Y%m%d-%H%M%S'))
    try:
        shutil.copy2(weg, sicherung)
    except Exception as ausnahme:
        fehler.merken('joysticks.sicherung', ausnahme)
        return False, 's_js_f_sicherung', 0
    try:
        baum.write(weg, encoding='utf-8', xml_declaration=False)
    except Exception as ausnahme:
        try:
            shutil.copy2(sicherung, weg)
        except Exception:
            pass
        fehler.merken('joysticks.schreiben', ausnahme)
        return False, 's_js_f_schreiben', 0
    return True, sicherung, anzahl


def konflikte(aktion, kennzeichen, eingabe, datei=None, ordner=None):
    """Wer sitzt schon auf dieser Eingabe? Liefert die betroffenen Aktionen.

    ⭐ **Wird VOR dem Belegen gefragt.** Eine Taste doppelt zu belegen ist in
    Star Citizen erlaubt und manchmal gewollt (verschiedene Fahrzeugarten),
    aber meistens ein Versehen — und eines, das man erst im Gefecht merkt.
    Deshalb wird es gezeigt und der Spieler entscheidet, statt dass das
    Programm heimlich etwas wegnimmt.
    """
    if not eingabe:
        return []
    heraus = []
    for kz, liste in (sicht(ALLES, datei, ordner) or {}).items():
        if kz != kennzeichen:
            continue
        for e in liste:
            if e['eingabe'] == eingabe and e['aktion'] != aktion:
                heraus.append(e)
    return heraus


def kennung_tauschen(alte, neue, neuer_name='', datei=None, ordner=None):
    """Ein Geraet unter neuer Kennung an seine alte Belegung anschliessen.

    Der Fall: Ein Stick meldet sich mit anderer Kennung (anderer Anschluss,
    neue Firmware, Austauschgeraet). Das Spiel erkennt ihn nicht wieder, seine
    alte Belegung haengt an einer Kennung, die es nicht mehr gibt.

    ⭐ **Die Reparatur fasst KEINE einzige Belegungszeile an.** Es genuegt,
    im Kopf der Datei die Kennung auszutauschen — alle `js<n>_`-Zeilen zeigen
    danach wieder auf ein Geraet, das da ist. Das ist der kleinstmoegliche
    Eingriff in eine Datei, an der die gesamte Steuerung des Spielers haengt.

    Liefert `(erfolg, meldung, anzahl)`. `meldung` ist bei Erfolg der Pfad der
    Sicherung, im Fehlerfall ein **Sprachschluessel** (`s_js_f_…`) — kein
    fertiger Satz. Sonst staende hier deutscher Text, den die englische
    Oberflaeche unuebersetzt anzeigt; Pruefung 17 des Selbsttests faengt genau
    das ab.

    ⚠ **Nur bei geschlossenem Spiel.** Star Citizen schreibt die Datei beim
    Beenden selbst und wuerde die Aenderung sonst ueberschreiben.
    """
    # ⚠ `fehler` lokal importieren — das Modul zieht selbst `pfade`, auf
    # Modulebene waere das ein Zirkelbezug (steht so im Projekt-CLAUDE.md).
    from . import fehler

    weg = datei or _pfad_actionmaps(ordner)
    if not weg or not os.path.isfile(weg):
        return False, 's_js_f_datei', 0
    if not alte or not neue or alte == neue:
        return False, 's_js_f_nichts', 0
    try:
        with open(weg, 'r', encoding='utf-8', errors='replace') as f:
            inhalt = f.read()
    except Exception as ausnahme:
        fehler.merken('joysticks.lesen', ausnahme)
        return False, 's_js_f_lesen', 0

    # ⚠ Gross-/Kleinschreibung der Kennung kann sich zwischen Protokoll und
    # Datei unterscheiden — deshalb wird ohne Ruecksicht darauf gesucht, aber
    # in der Schreibweise ersetzt, die in der Datei steht.
    muster = re.compile(re.escape(alte), re.IGNORECASE)
    treffer = len(muster.findall(inhalt))
    if not treffer:
        return False, 's_js_f_unbekannt', 0

    neu = muster.sub(neue, inhalt)

    # Hat das Spiel fuer das Geraet bereits einen zweiten, leeren Eintrag
    # angelegt, staende die neue Kennung nun zweimal da. Der spaetere (leere)
    # Eintrag wird geleert, damit genau eine Zuordnung uebrig bleibt.
    neu = _doppelten_eintrag_leeren(neu, neue)

    if neu == inhalt:
        return False, 's_js_f_gleich', 0

    sicherung = '%s.scbpw-%s' % (weg, time.strftime('%Y%m%d-%H%M%S'))
    try:
        shutil.copy2(weg, sicherung)
    except Exception as ausnahme:
        # Ohne Sicherung wird nicht geschrieben. Lieber gar nicht helfen als
        # ohne Rueckweg — hier haengt die komplette Steuerung dran.
        fehler.merken('joysticks.sicherung', ausnahme)
        return False, 's_js_f_sicherung', 0
    try:
        with open(weg, 'w', encoding='utf-8', newline='') as f:
            f.write(neu)
    except Exception as ausnahme:
        try:
            shutil.copy2(sicherung, weg)
        except Exception:
            pass
        fehler.merken('joysticks.schreiben', ausnahme)
        return False, 's_js_f_schreiben', 0
    return True, sicherung, treffer


def _doppelten_eintrag_leeren(inhalt, kennung):
    """Steht dieselbe Kennung in zwei `<options>`-Koepfen, bleibt der erste.

    Der zweite wird zu einem leeren Platz (`<options type="joystick"
    instance="N"/>`) — genau die Form, die das Spiel fuer unbelegte Plaetze
    selbst schreibt.
    """
    kopf = re.compile(r'<options\b[^>]*?\btype="joystick"[^>]*?>')
    gesehen = [False]

    def ersetzen(treffer):
        ganz = treffer.group(0)
        if kennung.upper() not in ganz.upper():
            return ganz
        if not gesehen[0]:
            gesehen[0] = True
            return ganz
        nummer = re.search(r'instance="(\d+)"', ganz)
        if not nummer:
            return ganz
        return '<options type="joystick" instance="%s"/>' % nummer.group(1)

    return kopf.sub(ersetzen, inhalt)


