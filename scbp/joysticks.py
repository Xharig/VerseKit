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

# ⚠⚠ **Der Mappings-Ordner heisst in beiden Schreibweisen** — genau wie
# `USER`/`user` weiter oben. Am 04.09.2026 lagen auf einem Linux-Rechner
# `controls/mappings` **und** `Controls/mappings` nebeneinander, mit
# verschiedenen Dateien darin (verschiedene Inodes). Deshalb wird auch hier
# gesucht statt geraten — und beim Auflisten nach Namen entdoppelt.
MAPPING_ORDNER = (('controls', 'mappings'), ('Controls', 'mappings'),
                  ('controls', 'Mappings'), ('Controls', 'Mappings'))

# Die Rubriken im Kopfblock eines Profils. Sie stammen aus einer echten Ausgabe
# des Spiels (Alpha 4.10) — es sind Sprachschluessel des Spiels, keine
# eigenen Erfindungen.
#
# ⚠ Kommen mit einem Patch Rubriken dazu, gehoert die Liste nachgezogen. Sie
# beschreibt, was im Belegungs-Bildschirm als Abschnitt auftaucht.
PROFIL_RUBRIKEN = (
    '@ui_CCSeatGeneral', '@ui_CCSpaceFlight', '@ui_CGLightControllerDesc',
    '@ui_CCFPS', '@ui_CCEVA', '@ui_CCVehicle', '@ui_CGEASpectator',
    '@ui_CGUIGeneral', '@ui_CGOpticalTracking', '@ui_CGInteraction',
    '@ui_CCCamera',
)

# Was in einem Profilnamen nichts zu suchen hat. Der Name wird zum Dateinamen,
# und ueber ihn laedt das Spiel das Profil (`pp_rebindkeys load <Name>`).
NAME_VERBOTEN = re.compile(r'[^A-Za-z0-9_\-]')


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


def alle_mapping_ordner(ordner=None):
    """**Alle** vorhandenen Mappings-Ordner, neuester zuerst.

    ⚠⚠ Auf einem Linux-Rechner lagen am 04.09.2026 `controls/mappings` **und**
    `Controls/mappings` nebeneinander — mit **verschiedenen** Dateien darin.

    Deshalb zwei verschiedene Fragen, die nicht dieselbe Antwort haben:

    | Frage | Antwort |
    |---|---|
    | „Wohin schreibe ich ein Profil?" | **einer** — `_pfad_mappings()` |
    | „Was ist an Profilen da?" | **alle** — diese Funktion |

    Wer beim Sichern nur einen Ordner liest, laesst die Profile des anderen
    zurueck, ohne dass es auffaellt.
    """
    basis = ordner or pfade.spiel_ordner()
    if not basis:
        return []
    gefunden = []
    for oben in ('USER', 'user'):
        for mitte in ('Client', 'client'):
            for teile in MAPPING_ORDNER:
                weg = os.path.join(basis, oben, mitte, '0', *teile)
                if os.path.isdir(weg) and weg not in gefunden:
                    gefunden.append(weg)
    try:
        gefunden.sort(key=os.path.getmtime, reverse=True)
    except OSError:
        pass
    return gefunden


def _pfad_mappings(ordner=None, anlegen=False):
    """Wohin ein neues Profil geschrieben wird — **ein** Ordner oder `None`.

    ⚠ Gibt es ihn mehrfach (siehe `alle_mapping_ordner`), gewinnt der **zuletzt
    geaenderte**: Das ist der, in den das Spiel selbst zuletzt geschrieben hat,
    und damit der, in dem es auch sucht.
    """
    gefunden = alle_mapping_ordner(ordner)
    if gefunden:
        return gefunden[0]
    if not anlegen:
        return None
    # Noch keiner da — dann neben der `actionmaps.xml` anlegen, damit die
    # Gross- und Kleinschreibung zur vorhandenen Installation passt.
    aktiv = _pfad_actionmaps(ordner)
    if not aktiv:
        return None
    # …/<USER>/<client>/0/Profiles/default/actionmaps.xml -> …/<client>/0
    null = os.path.dirname(os.path.dirname(os.path.dirname(aktiv)))
    weg = os.path.join(null, 'controls', 'mappings')
    try:
        os.makedirs(weg, exist_ok=True)
    except OSError:
        return None
    return weg


def profile(ordner=None):
    """Die gespeicherten Profile, alphabetisch — nur die ladbaren.

    ⚠ Star Citizen legt beim eigenen Export **zwei** Dateien an: `<Name>.xml`
    und `layout_<Name>_exported.xml`. Geladen wird ueber die erste; die zweite
    ist eine Zweitschrift und wuerde die Liste nur verdoppeln.
    """
    namen = set()
    # ⚠ Ueber **alle** Ordner, nicht nur den, in den geschrieben wuerde —
    # sonst fehlen dem Spieler in der Liste Profile, die er sehr wohl hat.
    for weg in alle_mapping_ordner(ordner):
        try:
            for datei in os.listdir(weg):
                if not datei.lower().endswith('.xml'):
                    continue
                if datei.lower().startswith('layout_'):
                    continue
                namen.add(datei[:-4])
        except OSError:
            continue
    return sorted(namen, key=str.lower)


def profil_datei(name, ordner=None):
    """Der Pfad zu einem gespeicherten Profil — oder `None`.

    ⚠ Gesucht wird in **allen** Schreibweisen des Mappings-Ordners, neuester
    zuerst. Liegt derselbe Name mehrfach, gewinnt der zuletzt geaenderte —
    dieselbe Regel wie beim Sichern.
    """
    if not (name or '').strip():
        return None
    gesucht = name.strip() + '.xml'
    for weg in alle_mapping_ordner(ordner):
        voll = os.path.join(weg, gesucht)
        if os.path.isfile(voll):
            return voll
    return None


def name_pruefen(name):
    """Taugt der Name als Profilname? Gibt `(ok, Meldungsschluessel)`.

    Er wird zum Dateinamen und ist zugleich das, was der Spieler im Spiel
    eintippt — deshalb keine Leerzeichen und keine Sonderzeichen.
    """
    name = (name or '').strip()
    if not name:
        return False, 's_js_f_name_leer'
    if NAME_VERBOTEN.search(name):
        return False, 's_js_f_name_zeichen'
    if len(name) > 60:
        return False, 's_js_f_name_lang'
    return True, ''


def als_profil(name, datei=None, ordner=None):
    """Aus der aktiven Belegung einen Baum im **Profil-Format** des Spiels.

    ⚠⚠ **Die beiden Formate sind nicht dasselbe** — gemessen am 04.09.2026 an
    einer echten Ausgabe des Spiels:

    | | aktive `actionmaps.xml` | Profil im Mappings-Ordner |
    |---|---|---|
    | Wurzel | `<ActionMaps>` **ohne Attribute** | `<ActionMaps version=… profileName=…>` |
    | darunter | ein `<ActionProfiles>`, das alles traegt | dieselben Bloecke **direkt** an der Wurzel |
    | Kopf | keiner | `<CustomisationUIHeader>` mit `<devices>` und `<categories>` |

    Wer die aktive Datei bloss kopiert, bekommt **kein ladbares Profil**. Genau
    das tat die Ausgabe bis v3.14.

    Liefert `(baum, None)` oder `(None, Meldungsschluessel)`.
    """
    weg = datei or _pfad_actionmaps(ordner)
    if not weg or not os.path.isfile(weg):
        return None, 's_js_f_datei'
    try:
        wurzel = ET.parse(weg).getroot()
    except Exception:
        return None, 's_js_f_fremd'
    profile_knoten = wurzel.find('ActionProfiles')
    if profile_knoten is None:
        # Schon im Profil-Format (jemand hat eine Ausgabe uebergeben).
        profile_knoten = wurzel

    neu = ET.Element('ActionMaps')
    for schluessel in ('version', 'optionsVersion', 'rebindVersion'):
        wert = profile_knoten.get(schluessel)
        if wert is not None:
            neu.set(schluessel, wert)
    neu.set('profileName', name)

    kopf = ET.SubElement(neu, 'CustomisationUIHeader',
                         {'label': name, 'description': '', 'image': ''})
    geraete_knoten = ET.SubElement(kopf, 'devices')
    ET.SubElement(geraete_knoten, 'keyboard', {'instance': '1'})
    ET.SubElement(geraete_knoten, 'mouse', {'instance': '1'})
    # ⚠ **Nur Plaetze mit `Product`.** Die `actionmaps.xml` legt acht
    # Joystick-Plaetze an, auch leere; eine Messung fand fuenf davon unbelegt.
    # Leere Plaetze im Kopf wuerden Geraete versprechen, die es nicht gibt.
    for wahl in profile_knoten.findall('options'):
        if wahl.get('type') == 'joystick' and wahl.get('Product'):
            ET.SubElement(geraete_knoten, 'joystick',
                          {'instance': wahl.get('instance') or '1'})
    rubriken = ET.SubElement(kopf, 'categories')
    for label in PROFIL_RUBRIKEN:
        ET.SubElement(rubriken, 'category', {'label': label})

    # Alles Uebrige unveraendert eine Ebene hoeher haengen — die Belegung
    # selbst wird **nicht** angefasst.
    for kind in list(profile_knoten):
        neu.append(kind)
    return ET.ElementTree(neu), None


def profil_speichern(name, datei=None, ordner=None, ueberschreiben=False):
    """Die aktive Belegung als ladbares Profil ablegen.

    Danach kennt das Spiel sie unter diesem Namen — im Spiel zu laden mit
    `pp_rebindkeys load <Name>`.

    Liefert `(erfolg, Meldung_oder_Pfad)`.
    """
    from . import fehler
    ok, meldung = name_pruefen(name)
    if not ok:
        return False, meldung
    name = name.strip()
    ziel_ordner = _pfad_mappings(ordner, anlegen=True)
    if not ziel_ordner:
        return False, 's_js_f_datei'
    ziel = os.path.join(ziel_ordner, name + '.xml')
    if os.path.exists(ziel) and not ueberschreiben:
        return False, 's_js_f_name_belegt'
    baum, meldung = als_profil(name, datei, ordner)
    if baum is None:
        return False, meldung
    try:
        # Erst daneben schreiben, dann umlegen: Bricht es ab, steht kein
        # halbes Profil im Ordner, das das Spiel zu laden versucht.
        vorlaeufig = ziel + '.tmp'
        baum.write(vorlaeufig, encoding='utf-8', xml_declaration=False)
        os.replace(vorlaeufig, ziel)
    except Exception as ausnahme:
        fehler.merken('joysticks.profil_speichern', ausnahme)
        return False, 's_js_f_schreiben'
    return True, ziel


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


# Tastennamen des Spiels, die als Kuerzel nicht zu verstehen sind. Was hier
# nicht steht, wird gross geschrieben durchgereicht (`f5` → `F5`, `a` → `A`).
TASTE_LESBAR = {
    'lshift': 's_js_t_lshift', 'rshift': 's_js_t_rshift',
    'lctrl': 's_js_t_lctrl', 'rctrl': 's_js_t_rctrl',
    'lalt': 's_js_t_lalt', 'ralt': 's_js_t_ralt',
    'space': 's_js_t_space', 'enter': 's_js_t_enter',
    'escape': 's_js_t_escape', 'backspace': 's_js_t_backspace',
    'tab': 's_js_t_tab', 'comma': 's_js_t_comma', 'period': 's_js_t_period',
    'slash': 's_js_t_slash', 'minus': 's_js_t_minus',
    'equals': 's_js_t_equals', 'up': 's_js_t_up', 'down': 's_js_t_down',
    'left': 's_js_t_left', 'right': 's_js_t_right', 'home': 's_js_t_home',
    'end': 's_js_t_end', 'pgup': 's_js_t_pgup', 'pgdn': 's_js_t_pgdn',
    'insert': 's_js_t_insert', 'delete': 's_js_t_delete',
    'pause': 's_js_t_pause', 'lbracket': 's_js_t_lbracket',
    'rbracket': 's_js_t_rbracket',
    'mouse1': 's_js_t_mouse1', 'mouse2': 's_js_t_mouse2',
    'mouse3': 's_js_t_mouse3', 'mwheel_up': 's_js_t_mwheel_up',
    'mwheel_down': 's_js_t_mwheel_down',
}

ACHSEN_LESBAR = {'x': 'X', 'y': 'Y', 'z': 'Z'}
DREHACHSEN = {'rotx': 'X', 'roty': 'Y', 'rotz': 'Z'}


def eingabe_lesbar(eingabe, art=''):
    """Aus `x` wird „Achse X", aus `button12` „Knopf 12".

    ⚠⚠ **Warum das noetig ist:** In der Spalte stand nur `x` — und `x` ist
    auf einer Tastatur ein Buchstabe. Wer die Zeile eines Sticks las, konnte
    denken, dort sei die Taste X gemeint. Dasselbe gilt fuer `y` und `z`.

    Zweisprachig ueber die Sprachdatei; was dort nicht steht, wird gross
    geschrieben durchgereicht, statt einen huebschen Namen zu erfinden.
    """
    from .sprache import t
    if not eingabe:
        return ''
    # Zusammengesetzte Eingaben: `ralt+y` → „Alt rechts + Y"
    if '+' in eingabe:
        return ' + '.join(eingabe_lesbar(teil, art)
                          for teil in eingabe.split('+') if teil)
    if art in ('tastatur', 'maus') or eingabe in TASTE_LESBAR:
        schluessel = TASTE_LESBAR.get(eingabe)
        if schluessel:
            return t(schluessel)
        if eingabe.startswith('np_'):
            return t('s_js_t_np', eingabe[3:].upper())
        return eingabe.upper()
    if eingabe in ACHSEN_LESBAR:
        return t('s_js_e_achse', ACHSEN_LESBAR[eingabe])
    if eingabe in DREHACHSEN:
        return t('s_js_e_drehachse', DREHACHSEN[eingabe])
    treffer = re.match(r'^button(\d+)$', eingabe)
    if treffer:
        return t('s_js_e_knopf', int(treffer.group(1)))
    treffer = re.match(r'^slider(\d+)$', eingabe)
    if treffer:
        return t('s_js_e_schieber', int(treffer.group(1)))
    treffer = re.match(r'^hat(\d+)_(\w+)$', eingabe)
    if treffer:
        richtungen = {'up': '↑', 'down': '↓', 'left': '←', 'right': '→'}
        pfeil = richtungen.get(treffer.group(2), treffer.group(2))
        return t('s_js_e_hut', int(treffer.group(1)), pfeil)
    return eingabe


def art_von(kennzeichen):
    """Aus `js1` wird `joystick`, aus `kb1` `tastatur`."""
    for art, kuerzel in ARTEN:
        if kennzeichen.startswith(kuerzel):
            return art
    return ''


# Welches Feld der `defaultProfile.xml` zu welcher Vorsilbe gehoert.
STANDARD_FELD = {'keyboard': 'kb1', 'joystick': 'js1', 'mouse': 'mo1',
                 'gamepad': 'gp1'}

# Die vier Sichten, die es zu sehen gibt.
MEINE    = 'meine'     # nur, was der Spieler selbst geaendert hat
STANDARD = 'standard'  # nur die Werkseinstellung des Spiels
ALLES    = 'alles'     # beides zusammengefuehrt — die wirkliche Belegung
FREI     = 'frei'      # Aktionen, auf die noch gar nichts zeigt


def gruppe_von(aktion, spielordner=None):
    """In welchem `actionmap` lebt eine Aktion?

    Wird beim Neubelegen gebraucht: Star Citizen sortiert Aktionen in
    Gruppen, und eine Belegung in der falschen Gruppe findet das Spiel nicht.
    """
    return ((_profil(spielordner) or {}).get('gruppen') or {}).get(aktion, '')


def unbelegte(spielordner=None, datei=None):
    """Aktionen, auf die weder eigene noch Werksbelegung zeigt.

    ⭐ **Ohne diese Liste kaeme man an sie gar nicht heran.** Die Belegungs-
    ansicht zeigt, was belegt ist — eine Aktion ohne jede Belegung taucht dort
    naturgemaess nicht auf, und der Spieler koennte sie nie anklicken, um sie
    zu belegen. Am 04.09.2026 gemessen: **411 von 646** benannten Aktionen
    sind ab Werk unbelegt (Emotes, Bergbau-Feinheiten, Notfallbefehle).

    Liefert dieselbe Form wie `sicht()`, unter dem Schluessel `frei`, mit
    leerer `eingabe`.
    """
    profil = _profil(spielordner) or {}
    benannt = profil.get('etiketten') or {}
    belegt = set()
    for liste in (sicht(ALLES, datei, spielordner) or {}).values():
        for e in liste:
            belegt.add(e['aktion'])
    gruppen = profil.get('gruppen') or {}
    heraus = []
    for aktion, paar in benannt.items():
        if aktion in belegt or not (paar or [''])[0]:
            continue
        heraus.append({'eingabe': '', 'aktion': aktion,
                       'bereich': gruppen.get(aktion, ''),
                       'art': '', 'quelle': FREI})
    # Nach Gruppe, dann nach Name — so stehen zusammengehoerige Aktionen
    # beieinander (alle Emotes, alle Bergbau-Befehle).
    heraus.sort(key=lambda e: (e['bereich'], e['aktion']))
    return {FREI: heraus} if heraus else {}


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

    ⚠⚠ **Beim Zusammenfuehren gewinnt der Spieler — aber nur auf DEM GERAET,
    das er angefasst hat.**

    Der erste Entwurf warf die Werksvorgabe fuer **alle** Geraete weg, sobald
    eine Aktion irgendwo eigen belegt war. Ergebnis: Wer „Respawn" auf einen
    Stick legt, sah die Taste `F` nicht mehr — obwohl sie im Spiel weiter
    funktioniert. Gemeldet am 04.09.2026, und zwar zu Recht: In der Liste
    „noch nicht belegt" standen Scheinwerfer, Hocken, Respawn und die linke
    Maustaste, die alle laengst eine Taste haben.

    **So macht es das Spiel:** Eine eigene Stick-Belegung ersetzt die
    Stick-Vorgabe. Tastatur, Maus und Gamepad bleiben davon unberuehrt.
    Deshalb wird nach **(Aktion, Geraeteart)** verdraengt, nicht nach Aktion.

    ⚠ Eine eigene Belegung mit **leerer** Eingabe ist eine geloeschte: Der
    Spieler hat die Werksvorgabe bewusst entfernt. Sie verdraengt den
    Standard, erscheint aber selbst nicht in der Liste — genau wie im Spiel.
    """
    if welche == FREI:
        return unbelegte(ordner, datei)
    eigene = belegungen(datei, ordner)
    if welche == MEINE:
        for liste in eigene.values():
            for e in liste:
                e['quelle'] = MEINE
        return {k: [e for e in v if e['eingabe']] for k, v in eigene.items()}
    if welche == STANDARD:
        return standardbelegungen(ordner)

    # Zusammenfuehren: erst merken, welche Aktion der Spieler auf welcher
    # **Geraeteart** angefasst hat — siehe die Warnung oben.
    angefasst = set()
    for kennzeichen, liste in eigene.items():
        art = art_von(kennzeichen)
        for e in liste:
            angefasst.add((e['aktion'], art))

    heraus = {}
    for kennzeichen, liste in standardbelegungen(ordner).items():
        art = art_von(kennzeichen)
        rest = [dict(e) for e in liste
                if (e['aktion'], art) not in angefasst]
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
MERK_FASSUNG = 3


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
    leer = {'etiketten': {}, 'standard': {}, 'gruppen': {}}
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
              'etiketten': {}, 'standard': {}, 'gruppen': {}}
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
        # ⚠ Über die **Gruppen** gehen, nicht flach über alle `action`-Knoten:
        # Nur so kommt mit, in welchem `actionmap` eine Aktion lebt. Ohne die
        # Gruppe landet eine neu angelegte Belegung in der falschen Sektion,
        # und das Spiel findet sie nicht.
        for gruppe in cryxml.alle(wurzel, 'actionmap'):
            bereich = (gruppe.get('attribute') or {}).get('name', '')
            for knoten in (gruppe.get('kinder') or []):
                if knoten.get('name') != 'action':
                    continue
                at = knoten.get('attribute') or {}
                name = at.get('name')
                if name and bereich:
                    heraus.setdefault('gruppen', {})[name] = bereich
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
    # ⚠ Rueckfall auf Englisch: Wo CIG keinen deutschen Text hinterlegt hat,
    # ist der englische immer noch besser als ein technisches Kuerzel.
    ersatz = (_ini_texte('en', spielordner) if sprache != 'en' else {})
    heraus = {}
    for aktion, paar in (etiketten or {}).items():
        label, beschreibung = ((paar or []) + ['', ''])[:2]
        schluessel = (label or '').lstrip('@')
        name = texte.get(schluessel) or ersatz.get(schluessel) or ''
        h_schluessel = (beschreibung or '').lstrip('@')
        hinweis = texte.get(h_schluessel) or ersatz.get(h_schluessel) or ''
        if not name:
            # ⚠⚠ **Dritte Stufe, und sie ist noetig.** Gemessen am 04.09.2026:
            # 314 Aktionen haben gar kein Etikett, bei weiteren 68 zeigt es
            # ins Leere — dafuer gibt es auch im Spiel selbst keinen Namen.
            # In der Liste stand dann `v_ads_stable_max_zoom_hold`.
            #
            # Aufbereitet wird **rein mechanisch**: Vorsilbe ab, Unterstriche
            # zu Leerzeichen, Wortanfaenge gross. Das ist Formatierung, kein
            # Erfinden — und die Oberflaeche zeigt solche Namen grau, damit
            # der Unterschied zu einer echten Bezeichnung sichtbar bleibt.
            name = _technisch_lesbar(aktion)
            heraus[aktion] = (name, hinweis, False)
            continue
        heraus[aktion] = (name, hinweis, True)
    _KLARNAMEN[sprache] = heraus
    return heraus


# Die Vorsilben, mit denen das Spiel seine Aktionen sortiert. Sie sagen nur,
# in welchem Zusammenhang die Aktion steht, und stehen in der Anzeige im Weg.
VORSILBEN_AKTION = ('v_', 'pl_', 'ui_', 'mg_', 'ca_', 'sc_')


def _technisch_lesbar(aktion):
    """`v_ads_stable_max_zoom_hold` → `Ads Stable Max Zoom Hold`."""
    rest = aktion
    for v in VORSILBEN_AKTION:
        if rest.startswith(v):
            rest = rest[len(v):]
            break
    rest = rest.replace('_', ' ').strip()
    return ' '.join(w[:1].upper() + w[1:] for w in rest.split()) or aktion


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


def zuruecksetzen(datei=None, ordner=None):
    """Alle eigenen Belegungen verwerfen — zurueck auf Werkseinstellung.

    ⚠⚠ **Das ist der Knopf, der am meisten kaputtmachen kann.** Er wirft die
    komplette Arbeit weg, die jemand in seine Steuerung gesteckt hat. Deshalb:

    * Die Oberflaeche fragt vorher **ausdruecklich** nach.
    * Vorher entsteht eine Sicherung neben der Datei — der Rueckweg ist ein
      Umbenennen.
    * Entfernt werden **nur die Tastenbelegungen** (`<actionmap>`). Was an
      den Geraeten eingestellt ist — Totzonen, Kurven, Empfindlichkeit
      (`<deviceoptions>`, `<options>`) — bleibt stehen. Das sind
      Geraeteeinstellungen, keine Belegung, und wer „Belegung zuruecksetzen"
      drueckt, will seine Totzonen nicht neu einmessen.

    Liefert `(erfolg, meldung, anzahl geloeschter Gruppen)`.
    """
    from . import fehler
    weg = datei or _pfad_actionmaps(ordner)
    if not weg or not os.path.isfile(weg):
        return False, 's_js_f_datei', 0
    try:
        baum = ET.parse(weg)
    except Exception as ausnahme:
        fehler.merken('joysticks.zuruecksetzen_lesen', ausnahme)
        return False, 's_js_f_lesen', 0

    wurzel = baum.getroot()
    eltern = wurzel.find('ActionProfiles')
    if eltern is None:
        eltern = wurzel
    weggeworfen = 0
    for gruppe in list(eltern.findall('actionmap')):
        eltern.remove(gruppe)
        weggeworfen += 1
    if not weggeworfen:
        return False, 's_js_f_gleich', 0
    return _schreiben(weg, baum, weggeworfen)


def ausgeben(ziel, sprache='de', datei=None, ordner=None):
    """Die Belegung als lesbare Datei ausgeben.

    Zwei Formate, am Dateinamen erkannt:

    | Endung | Was drin steht |
    |---|---|
    | `.xml` | die `actionmaps.xml` **unveraendert** — zum Sichern und Teilen |
    | `.csv` | Geraet, Eingabe, Aktion, Gruppe — zum Nachschlagen und Drucken |

    ⭐ **Die XML-Kopie ist der Weg, den man sonst nur im Spiel hat**
    (`pp_rebindkeys export …` in der Konsole). Wer seine Belegung sichern oder
    einem Staffelkameraden geben will, muss dafuer jetzt nicht mehr ins Spiel.

    Liefert `(erfolg, meldung)`.
    """
    from . import fehler
    quelle = datei or _pfad_actionmaps(ordner)
    if not quelle or not os.path.isfile(quelle):
        return False, 's_js_f_datei'
    try:
        if ziel.lower().endswith('.csv'):
            namen = klarnamen(sprache, ordner)
            zeilen = ['Geraet;Eingabe;Aktion;Bezeichnung;Gruppe;Quelle']
            for kennzeichen, liste in sorted(sicht(ALLES, datei,
                                                   ordner).items()):
                for e in liste:
                    klar = (namen.get(e['aktion']) or ('', '', False))[0]
                    zeilen.append(';'.join(
                        # ⚠ Semikolon im Text wuerde die Spalten zerreissen —
                        # es kommt in Bezeichnungen des Spiels tatsaechlich vor.
                        (feld or '').replace(';', ',')
                        for feld in (kennzeichen, e['eingabe'], e['aktion'],
                                     klar, e['bereich'], e.get('quelle', ''))))
            with open(ziel, 'w', encoding='utf-8-sig', newline='') as f:
                # ⚠ `utf-8-sig`: Excel liest UTF-8 ohne Vorspann als
                # Windows-1252 und macht aus „Schleudersitz" Buchstabensalat.
                f.write(chr(10).join(zeilen) + chr(10))
        else:
            shutil.copy2(quelle, ziel)
    except Exception as ausnahme:
        fehler.merken('joysticks.ausgeben', ausnahme)
        return False, 's_js_f_schreiben'
    return True, ziel


def _als_aktive_form(wurzel):
    """Ein Profil zurueck in die Form der `actionmaps.xml` bringen.

    ⚠⚠ **Der Rueckweg gehoert zum Hinweg.** Ein Profil aus dem Mappings-Ordner
    traegt seine Angaben an der Wurzel und hat einen `CustomisationUIHeader`;
    die aktive Datei hat ein `<ActionProfiles>` und keinen Kopf. Wer ein Profil
    einfach ueber die aktive Datei kopiert, legt die falsche Form dorthin —
    derselbe Fehler wie beim Ausgeben, nur andersherum.

    Steckt die Datei schon in der aktiven Form, wird sie unveraendert
    zurueckgegeben.
    """
    if wurzel.find('ActionProfiles') is not None:
        return wurzel
    neu = ET.Element('ActionMaps')
    profile_knoten = ET.SubElement(neu, 'ActionProfiles')
    for schluessel in ('version', 'optionsVersion', 'rebindVersion'):
        wert = wurzel.get(schluessel)
        if wert is not None:
            profile_knoten.set(schluessel, wert)
    # ⚠ Die aktive Belegung heisst im Spiel immer `default` — der Profilname
    # aus der Datei gilt nur fuer das Profil, nicht fuer die aktive Steuerung.
    profile_knoten.set('profileName', 'default')
    for kind in list(wurzel):
        if kind.tag == 'CustomisationUIHeader':
            continue          # der Kopf gehoert nur ins Profil
        profile_knoten.append(kind)
    return neu


def einlesen(quelle, datei=None, ordner=None):
    """Eine zuvor ausgegebene Belegung wieder einspielen.

    Nimmt **beide** Formen an: die Kopie einer `actionmaps.xml` und ein Profil
    aus dem Mappings-Ordner (siehe `_als_aktive_form`).

    ⚠ Es wird geprueft, ob die Datei ueberhaupt danach aussieht — sonst
    landet irgendeine XML-Datei als Steuerung im Spiel. Und auch hier gilt:
    erst Sicherung, dann schreiben.
    """
    from . import fehler
    ziel = datei or _pfad_actionmaps(ordner)
    if not ziel or not os.path.isfile(ziel):
        return False, 's_js_f_datei', 0
    if not quelle or not os.path.isfile(quelle):
        return False, 's_js_f_datei', 0
    try:
        baum = ET.parse(quelle)
    except Exception:
        return False, 's_js_f_fremd', 0
    wurzel = baum.getroot()
    if wurzel.tag != 'ActionMaps':
        return False, 's_js_f_fremd', 0
    anzahl = len(list(wurzel.iter('actionmap')))
    try:
        sicherung = '%s.scbpw-%s' % (ziel, time.strftime('%Y%m%d-%H%M%S'))
        shutil.copy2(ziel, sicherung)
        ET.ElementTree(_als_aktive_form(wurzel)).write(
            ziel, encoding='utf-8', xml_declaration=False)
    except Exception as ausnahme:
        fehler.merken('joysticks.einlesen', ausnahme)
        return False, 's_js_f_schreiben', 0
    return True, sicherung, anzahl


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


def belegungen_tauschen(kennung_a, kennung_b, datei=None, ordner=None):
    """Zwei Geraete ueber Kreuz: Was auf dem einen lag, liegt danach auf dem anderen.

    Der Fall: Nach einem Neustart oder einem anderen USB-Anschluss hat das
    Spiel die Nummern anders vergeben — der linke Stick ist jetzt `js1` statt
    `js2`. Damit sitzt die komplette Belegung auf der falschen Hand.

    ⭐ **Getauscht werden nur die beiden `Product`-Angaben** in den
    `<options type="joystick">`-Bloecken. Keine einzige der 400 Belegungszeilen
    wird angefasst: Das Spiel erkennt seine Geraete an der gespeicherten
    Kennung wieder (gemessen 04.09.2026), also genuegt es zu sagen, welche
    Kennung nun welche Nummer ist. Danach wirken alle `js1_`-Zeilen auf dem
    anderen Stick.

    ⚠⚠ **Die `<deviceoptions>` bleiben ausdruecklich unangetastet.** Dort
    stehen Totzone und Saettigung, und die gehoeren zum **physischen** Geraet,
    nicht zur Nummer: Ein ausgeleierter Stick braucht seine groessere Totzone
    weiterhin, egal welche Belegung gerade auf ihm liegt. Wer hier stur die
    ganze Datei durchtauscht, verschiebt sie auf das falsche Geraet.

    ⚠⚠ **Und es geschieht in EINEM Durchgang.** Zweimal `kennung_tauschen`
    (A→B, dann B→A) waere falsch: Der zweite Lauf fande auch die gerade
    geschriebenen B's und drehte alles zurueck. Deshalb ersetzt ein einziger
    Ausdruck beide Kennungen gleichzeitig.

    Liefert `(erfolg, meldung, anzahl)` wie die Nachbarfunktionen.
    """
    from . import fehler

    weg = datei or _pfad_actionmaps(ordner)
    if not weg or not os.path.isfile(weg):
        return False, 's_js_f_datei', 0
    a = (kennung_a or '').strip()
    b = (kennung_b or '').strip()
    if not a or not b or a.upper() == b.upper():
        return False, 's_js_f_nichts', 0

    try:
        with open(weg, 'r', encoding='utf-8', errors='replace') as f:
            inhalt = f.read()
    except Exception as ausnahme:
        fehler.merken('joysticks.tausch_lesen', ausnahme)
        return False, 's_js_f_lesen', 0

    # Nur die Joystick-Bloecke — `<deviceoptions>` bleiben aussen vor.
    block_muster = re.compile(
        r'<options\b[^>]*\btype="joystick"[^>]*/>|'
        r'<options\b[^>]*\btype="joystick"[^>]*>.*?</options>', re.S)
    paar = re.compile('(%s|%s)' % (re.escape(a), re.escape(b)),
                      re.IGNORECASE)
    getauscht = [0]

    def kreuz(treffer):
        """Jede gefundene Kennung durch die jeweils andere ersetzen."""
        gefunden = treffer.group(0)
        getauscht[0] += 1
        return b if gefunden.upper() == a.upper() else a

    def block_umschreiben(treffer):
        return paar.sub(kreuz, treffer.group(0))

    neu = block_muster.sub(block_umschreiben, inhalt)

    if getauscht[0] < 2:
        # Weniger als zwei Treffer heisst: Mindestens eines der beiden Geraete
        # hat gar keinen Block — dann gaebe es nichts zu tauschen, und ein
        # halber Tausch waere schlimmer als keiner.
        return False, 's_js_f_unbekannt', 0
    if neu == inhalt:
        return False, 's_js_f_gleich', 0

    sicherung = '%s.scbpw-%s' % (weg, time.strftime('%Y%m%d-%H%M%S'))
    try:
        shutil.copy2(weg, sicherung)
    except Exception as ausnahme:
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
        fehler.merken('joysticks.tausch_schreiben', ausnahme)
        return False, 's_js_f_schreiben', 0
    return True, sicherung, getauscht[0]


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


