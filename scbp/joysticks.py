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
falsch** — die Begruendung steht ausfuehrlich ueber `compare()`. Kurz: Das
Spiel erkennt seine Geraete an der gespeicherten Kennung wieder, nicht an der
Fundreihenfolge. Wer die Nummern anfasst, zerstoert eine gesunde Belegung.

Repariert wird deshalb nur der eine Fall, in dem wirklich etwas kaputt ist:
ein Geraet meldet sich unter **neuer Kennung** (`swap_id`).

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
CONNECTED = re.compile(
    r'Connected joystick(\d+):\s*(.+?)\s*\{([0-9A-Fa-f-]+)\}')

# Aus einer Kennung in der `actionmaps.xml` das reine Kennungs-Teil holen.
ID_IN_NAME = re.compile(r'\{([0-9A-Fa-f-]+)\}')

# Jede Eingabe-Vorsilbe in der actionmaps.xml: js1_button10, js2_x, js3_hat1_up
JS_PREFIX = re.compile(r'\bjs(\d+)_')

# Wieviele Joystick-Plaetze Star Citizen kennt. Mehr als acht meldet das Spiel
# selbst als Grenzfall; die `actionmaps.xml` legt acht leere Plaetze an.
SLOTS = 8

# ⚠⚠ **Der Mappings-Ordner heisst in beiden Schreibweisen** — genau wie
# `USER`/`user` weiter oben. Am 04.09.2026 lagen auf einem Linux-Rechner
# `controls/mappings` **und** `Controls/mappings` nebeneinander, mit
# verschiedenen Dateien darin (verschiedene Inodes). Deshalb wird auch hier
# gesucht statt geraten — und beim Auflisten nach Namen entdoppelt.
MAPPING_FOLDERS = (('controls', 'mappings'), ('Controls', 'mappings'),
                  ('controls', 'Mappings'), ('Controls', 'Mappings'))

# Die Rubriken im Kopfblock eines Profils. Sie stammen aus einer echten Ausgabe
# des Spiels (Alpha 4.10) — es sind Sprachschluessel des Spiels, keine
# eigenen Erfindungen.
#
# ⚠ Kommen mit einem Patch Rubriken dazu, gehoert die Liste nachgezogen. Sie
# beschreibt, was im Belegungs-Bildschirm als Abschnitt auftaucht.
PROFILE_CATEGORIES = (
    '@ui_CCSeatGeneral', '@ui_CCSpaceFlight', '@ui_CGLightControllerDesc',
    '@ui_CCFPS', '@ui_CCEVA', '@ui_CCVehicle', '@ui_CGEASpectator',
    '@ui_CGUIGeneral', '@ui_CGOpticalTracking', '@ui_CGInteraction',
    '@ui_CCCamera',
)

# Was in einem Profilnamen nichts zu suchen hat. Der Name wird zum Dateinamen,
# und ueber ihn laedt das Spiel das Profil (`pp_rebindkeys load <Name>`).
NAME_FORBIDDEN = re.compile(r'[^A-Za-z0-9_\-]')


def all_actionmaps(folder=None):
    """**Alle** vorhandenen Belegungsdateien, neueste zuerst.

    ⚠⚠⚠ **Es kann mehrere geben, und sie haben verschiedene Inhalte.** Unter
    Windows sind `USER` und `user` derselbe Ordner; unter Linux (Wine, das
    Dateisystem unterscheidet Gross- und Kleinschreibung) sind es **zwei**.
    Wer aus einer Windows-Installation herueberzieht, hat danach beide — und
    merkt nichts davon.

    Gemessen am 06.09.2026 auf einem Linux-Rechner, in **einer**
    Installation:

    | Ordner | geaendert | Inhalt |
    |---|---|---|
    | `LIVE/user/client/…` | 17:19 — **das Spiel** | keine Exponenten |
    | `LIVE/USER/client/…` | 17:13 — die Karteileiche | 22 Exponenten |

    Die Karteileiche stammte erkennbar von Windows: Sie nannte die Geraete
    „RIGHT VPC Stick" und „Tastatur", waehrend das Linux-Spiel „R-VPC Stick"
    und „Wine Keyboard" schreibt. **Und sie hatte eine andere Reihenfolge** —
    `instance=1` war dort der rechte Stick, im Spiel ist es der linke. Wer sie
    liest, zeigt dem Spieler also nicht nur veraltete Werte, sondern die Werte
    des **falschen Geraets**.
    """
    base = folder or pfade.spiel_ordner()
    if not base:
        return []
    found = []
    for top in ('USER', 'user'):
        for mid in ('Client', 'client'):
            path = os.path.join(base, top, mid, '0', 'Profiles', 'default',
                               'actionmaps.xml')
            if not os.path.isfile(path):
                continue
            # ⚠ Auf einem Dateisystem, das Gross-/Kleinschreibung NICHT
            # unterscheidet, zeigen mehrere Schreibweisen auf **dieselbe**
            # Datei. Ueber die Inode entdoppeln, nicht ueber den Namen.
            try:
                ident = os.stat(path).st_ino
            except OSError:
                continue
            if ident not in [k for k, _ in found]:
                found.append((ident, path))
    paths = [w for _, w in found]
    try:
        paths.sort(key=os.path.getmtime, reverse=True)
    except OSError:
        pass
    return paths


def _actionmaps_path(folder=None):
    """Wo die Belegungsdatei liegt, **mit der das Spiel wirklich arbeitet**.

    ⚠⚠⚠ **Die erste Fassung nahm stur `USER` zuerst** — die erste Schreibweise,
    die es gab, unabhaengig vom Alter. Am 06.09.2026 war das die falsche: Der
    Watcher zeigte 22 Exponenten und eine Empfindlichkeit von 2, waehrend im
    Spiel ueberall 1,00 stand — gemeldet mit einem berechtigten „mir reicht
    es langsam".

    Dabei stand die Loesung schon im selben Modul — `_mappings_path()` nimmt
    seit dem 04.09.2026 den **zuletzt geaenderten** Ordner, aus genau diesem
    Grund. Hier wurde sie nicht angewendet. Eine Lehre, die nur an einer von
    zwei Stellen gezogen wird, ist keine.

    **Die juengste gewinnt.** Sobald das Spiel schreibt, ist seine Datei die
    juengste — die Wahl korrigiert sich damit von selbst und bleibt richtig,
    auch wenn der Spieler die Installation wechselt.
    """
    paths = all_actionmaps(folder)
    return paths[0] if paths else None


def all_mapping_folders(folder=None):
    """**Alle** vorhandenen Mappings-Ordner, neuester zuerst.

    ⚠⚠ Auf einem Linux-Rechner lagen am 04.09.2026 `controls/mappings` **und**
    `Controls/mappings` nebeneinander — mit **verschiedenen** Dateien darin.

    Deshalb zwei verschiedene Fragen, die nicht dieselbe Antwort haben:

    | Frage | Antwort |
    |---|---|
    | „Wohin schreibe ich ein Profil?" | **einer** — `_mappings_path()` |
    | „Was ist an Profilen da?" | **alle** — diese Funktion |

    Wer beim Sichern nur einen Ordner liest, laesst die Profile des anderen
    zurueck, ohne dass es auffaellt.
    """
    base = folder or pfade.spiel_ordner()
    if not base:
        return []
    found = []
    for top in ('USER', 'user'):
        for mid in ('Client', 'client'):
            for parts in MAPPING_FOLDERS:
                path = os.path.join(base, top, mid, '0', *parts)
                if os.path.isdir(path) and path not in found:
                    found.append(path)
    try:
        found.sort(key=os.path.getmtime, reverse=True)
    except OSError:
        pass
    return found


def _mappings_path(folder=None, create=False):
    """Wohin ein neues Profil geschrieben wird — **ein** Ordner oder `None`.

    ⚠ Gibt es ihn mehrfach (siehe `all_mapping_folders`), gewinnt der **zuletzt
    geaenderte**: Das ist der, in den das Spiel selbst zuletzt geschrieben hat,
    und damit der, in dem es auch sucht.
    """
    found = all_mapping_folders(folder)
    if found:
        return found[0]
    if not create:
        return None
    # Noch keiner da — dann neben der `actionmaps.xml` anlegen, damit die
    # Gross- und Kleinschreibung zur vorhandenen Installation passt.
    active = _actionmaps_path(folder)
    if not active:
        return None
    # …/<USER>/<client>/0/Profiles/default/actionmaps.xml -> …/<client>/0
    zero = os.path.dirname(os.path.dirname(os.path.dirname(active)))
    path = os.path.join(zero, 'controls', 'mappings')
    try:
        os.makedirs(path, exist_ok=True)
    except OSError:
        return None
    return path


def profiles(folder=None):
    """Die gespeicherten Profile, alphabetisch — nur die ladbaren.

    ⚠ Star Citizen legt beim eigenen Export **zwei** Dateien an: `<Name>.xml`
    und `layout_<Name>_exported.xml`. Geladen wird ueber die erste; die zweite
    ist eine Zweitschrift und wuerde die Liste nur verdoppeln.
    """
    names = set()
    # ⚠ Ueber **alle** Ordner, nicht nur den, in den geschrieben wuerde —
    # sonst fehlen dem Spieler in der Liste Profile, die er sehr wohl hat.
    for path in all_mapping_folders(folder):
        try:
            for filename in os.listdir(path):
                if not filename.lower().endswith('.xml'):
                    continue
                if filename.lower().startswith('layout_'):
                    continue
                names.add(filename[:-4])
        except OSError:
            continue
    return sorted(names, key=str.lower)


def profile_file(name, folder=None):
    """Der Pfad zu einem gespeicherten Profil — oder `None`.

    ⚠ Gesucht wird in **allen** Schreibweisen des Mappings-Ordners, neuester
    zuerst. Liegt derselbe Name mehrfach, gewinnt der zuletzt geaenderte —
    dieselbe Regel wie beim Sichern.
    """
    if not (name or '').strip():
        return None
    wanted = name.strip() + '.xml'
    for path in all_mapping_folders(folder):
        full = os.path.join(path, wanted)
        if os.path.isfile(full):
            return full
    return None


def check_name(name):
    """Taugt der Name als Profilname? Gibt `(ok, Meldungsschluessel)`.

    Er wird zum Dateinamen und ist zugleich das, was der Spieler im Spiel
    eintippt — deshalb keine Leerzeichen und keine Sonderzeichen.
    """
    name = (name or '').strip()
    if not name:
        return False, 's_js_f_name_leer'
    if NAME_FORBIDDEN.search(name):
        return False, 's_js_f_name_zeichen'
    if len(name) > 60:
        return False, 's_js_f_name_lang'
    return True, ''


def as_profile(name, filename=None, folder=None):
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
    path = filename or _actionmaps_path(folder)
    if not path or not os.path.isfile(path):
        return None, 's_js_f_datei'
    try:
        root = ET.parse(path).getroot()
    except Exception:
        return None, 's_js_f_fremd'
    profiles_node = root.find('ActionProfiles')
    if profiles_node is None:
        # Schon im Profil-Format (jemand hat eine Ausgabe uebergeben).
        profiles_node = root

    new = ET.Element('ActionMaps')
    for key in ('version', 'optionsVersion', 'rebindVersion'):
        value = profiles_node.get(key)
        if value is not None:
            new.set(key, value)
    new.set('profileName', name)

    head = ET.SubElement(new, 'CustomisationUIHeader',
                         {'label': name, 'description': '', 'image': ''})
    devices_node = ET.SubElement(head, 'devices')
    ET.SubElement(devices_node, 'keyboard', {'instance': '1'})
    ET.SubElement(devices_node, 'mouse', {'instance': '1'})
    # ⚠ **Nur Plaetze mit `Product`.** Die `actionmaps.xml` legt acht
    # Joystick-Plaetze an, auch leere; eine Messung fand fuenf davon unbelegt.
    # Leere Plaetze im Kopf wuerden Geraete versprechen, die es nicht gibt.
    for option in profiles_node.findall('options'):
        if option.get('type') == 'joystick' and option.get('Product'):
            ET.SubElement(devices_node, 'joystick',
                          {'instance': option.get('instance') or '1'})
    categories = ET.SubElement(head, 'categories')
    for label in PROFILE_CATEGORIES:
        ET.SubElement(categories, 'category', {'label': label})

    # Alles Uebrige unveraendert eine Ebene hoeher haengen — die Belegung
    # selbst wird **nicht** angefasst.
    for child in list(profiles_node):
        new.append(child)
    return ET.ElementTree(new), None


def save_profile(name, filename=None, folder=None, overwrite=False):
    """Die aktive Belegung als ladbares Profil ablegen.

    Danach kennt das Spiel sie unter diesem Namen — im Spiel zu laden mit
    `pp_rebindkeys load <Name>`.

    Liefert `(erfolg, Meldung_oder_Pfad)`.
    """
    from . import fehler
    ok, message = check_name(name)
    if not ok:
        return False, message
    name = name.strip()
    target_folder = _mappings_path(folder, create=True)
    if not target_folder:
        return False, 's_js_f_datei'
    target = os.path.join(target_folder, name + '.xml')
    if os.path.exists(target) and not overwrite:
        return False, 's_js_f_name_belegt'
    tree, message = as_profile(name, filename, folder)
    if tree is None:
        return False, message
    try:
        # Erst daneben schreiben, dann umlegen: Bricht es ab, steht kein
        # halbes Profil im Ordner, das das Spiel zu laden versucht.
        temp = target + '.tmp'
        tree.write(temp, encoding='utf-8', xml_declaration=False)
        os.replace(temp, target)
    except Exception as exception:
        fehler.merken('joysticks.save_profile', exception)
        return False, 's_js_f_schreiben'
    return True, target


def devices_from_text(text):
    """Die verbundenen Geraete aus einem Log-Text, in Fundreihenfolge.

    Liefert je Geraet ein Woerterbuch mit `platz` (die Zahl, die das Spiel
    vergibt), `name` und `kennung`.
    """
    found = []
    seen = set()
    for hit in CONNECTED.finditer(text or ''):
        slot = int(hit.group(1))
        ident = hit.group(3).upper()
        # ⚠ Innerhalb einer Sitzung kann dieselbe Zeile mehrfach auftauchen
        # (Neuverbinden im laufenden Spiel). Der erste Fund gilt.
        if slot in seen:
            continue
        seen.add(slot)
        found.append({'platz': slot,
                         'name': hit.group(2).strip(),
                         'kennung': ident})
    found.sort(key=lambda g: g['platz'])
    return found


def devices(folder=None):
    """Die Geraete aus dem neuesten Protokoll des Spiels.

    Zuerst die laufende `Game.log`; steht dort nichts (das Spiel lief seit dem
    letzten Einloggen nicht), wird die neueste Sicherung genommen. Ohne
    Spielstart gibt es keine Geraeteliste — dann bleibt die Liste leer, und
    die Oberflaeche sagt das auch so.
    """
    files = []
    running = pfade.game_log(folder)
    if running and os.path.isfile(running):
        files.append(running)
    try:
        files.extend(pfade.log_sicherungen(folder) or [])
    except Exception:
        pass
    for filename in files:
        try:
            # Die Geraetezeilen stehen in den ersten Hundert Zeilen. Eine
            # 13-MB-Datei dafuer ganz zu lesen waere Verschwendung — beim
            # Oeffnen der Seite faellt das sofort auf.
            with open(filename, 'r', encoding='utf-8', errors='replace') as f:
                head = f.read(200000)
        except Exception:
            continue
        hit = devices_from_text(head)
        if hit:
            return hit
    return []


def assignment(filename=None, folder=None):
    """Welche Nummer in der `actionmaps.xml` welchem Geraet gehoert.

    Liefert je belegtem Platz ein Woerterbuch mit `nummer` (die `instance`,
    also das `n` in `js<n>_`), `name` und `kennung`. Leere Plaetze
    (`<options type="joystick" instance="7"/>`) kommen nicht mit — sie sagen
    nichts aus.
    """
    path = filename or _actionmaps_path(folder)
    if not path or not os.path.isfile(path):
        return []
    try:
        tree = ET.parse(path)
    except Exception:
        # Eine kaputte oder halb geschriebene Datei ist kein Grund
        # abzustuerzen — die Seite zeigt dann „nicht lesbar".
        return []
    out = []
    for node in tree.getroot().iter('options'):
        if (node.get('type') or '').lower() != 'joystick':
            continue
        product = node.get('Product') or ''
        if not product.strip():
            continue
        try:
            number = int(node.get('instance') or 0)
        except ValueError:
            continue
        ident = ID_IN_NAME.search(product)
        out.append({
            'nummer': number,
            'name': ID_IN_NAME.sub('', product).strip(),
            'kennung': (ident.group(1).upper() if ident else ''),
        })
    out.sort(key=lambda z: z['nummer'])
    return out


# Die Zustaende, die ein Vergleich haben kann.
MATCHES   = 'passt'    # jedes belegte Geraet ist verbunden — alles in Ordnung
REPLACED = 'ersetzt'  # ein Geraet meldet sich unter NEUER Kennung (reparierbar)
MISSING   = 'fehlt'    # ein belegtes Geraet ist gar nicht verbunden
EMPTY    = 'leer'     # keine Daten (noch nie gespielt, Datei fehlt)


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
# Genau diesen Fall — und nur diesen — meldet `REPLACED`.


def compare(folder=None, filename=None):
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
    found = devices(folder)
    stored = assignment(filename, folder)
    result = {'zustand': EMPTY, 'geraete': found,
                'zuordnung': stored, 'fehlende': [], 'neue': [],
                'ersatz': [], 'datei': filename or _actionmaps_path(folder)}
    if not found or not stored:
        return result

    bound_ids = {z['kennung'] for z in stored if z['kennung']}
    connected = {g['kennung'] for g in found}

    result['fehlende'] = [z for z in stored
                            if z['kennung'] and z['kennung'] not in connected]
    result['neue'] = [g for g in found if g['kennung'] not in bound_ids]

    if len(result['fehlende']) == 1 and len(result['neue']) == 1:
        result['ersatz'] = [(result['fehlende'][0], result['neue'][0])]
        result['zustand'] = REPLACED
    elif result['fehlende']:
        result['zustand'] = MISSING
    else:
        result['zustand'] = MATCHES
    return result


# Jede Geraeteart, die in der `actionmaps.xml` vorkommt, mit ihrer Vorsilbe.
# ⚠ Die Reihenfolge ist die, in der die Geraete in der Oberflaeche erscheinen.
KINDS = (('joystick', 'js'), ('tastatur', 'kb'), ('maus', 'mo'),
         ('gamepad', 'gp'))
PREFIX = re.compile(r'^(js|kb|mo|gp)(\d+)_')


def bindings(filename=None, folder=None):
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
    path = filename or _actionmaps_path(folder)
    if not path or not os.path.isfile(path):
        return {}
    try:
        tree = ET.parse(path)
    except Exception:
        return {}
    by_prefix = {abbrev: kind for kind, abbrev in KINDS}
    out = {}
    for group in tree.getroot().iter('actionmap'):
        section = group.get('name') or ''
        for action in group.iter('action'):
            name = action.get('name') or ''
            for binding in action.iter('rebind'):
                input_device = (binding.get('input') or '').strip()
                hit = PREFIX.match(input_device)
                if not hit:
                    continue
                device_id = hit.group(1) + hit.group(2)
                out.setdefault(device_id, []).append({
                    'eingabe': input_device[hit.end():],
                    'aktion': name,
                    'bereich': section,
                    'art': by_prefix.get(hit.group(1), ''),
                })
    for entries in out.values():
        # Achsen zuerst, dann Knoepfe nach Nummer, dann der Rest — dieselbe
        # Reihenfolge, in der man ein Geraet auch anschaut.
        entries.sort(key=_sort_key)
    return out


# Tastennamen des Spiels, die als Kuerzel nicht zu verstehen sind. Was hier
# nicht steht, wird gross geschrieben durchgereicht (`f5` → `F5`, `a` → `A`).
KEY_READABLE = {
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

AXES_READABLE = {'x': 'X', 'y': 'Y', 'z': 'Z'}
ROTATION_AXES = {'rotx': 'X', 'roty': 'Y', 'rotz': 'Z'}


def input_readable(input_device, kind=''):
    """Aus `x` wird „Achse X", aus `button12` „Knopf 12".

    ⚠⚠ **Warum das noetig ist:** In der Spalte stand nur `x` — und `x` ist
    auf einer Tastatur ein Buchstabe. Wer die Zeile eines Sticks las, konnte
    denken, dort sei die Taste X gemeint. Dasselbe gilt fuer `y` und `z`.

    Zweisprachig ueber die Sprachdatei; was dort nicht steht, wird gross
    geschrieben durchgereicht, statt einen huebschen Namen zu erfinden.
    """
    from .sprache import t
    if not input_device:
        return ''
    # Zusammengesetzte Eingaben: `ralt+y` → „Alt rechts + Y"
    if '+' in input_device:
        return ' + '.join(input_readable(part, kind)
                          for part in input_device.split('+') if part)
    if kind in ('tastatur', 'maus') or input_device in KEY_READABLE:
        key = KEY_READABLE.get(input_device)
        if key:
            return t(key)
        if input_device.startswith('np_'):
            return t('s_js_t_np', input_device[3:].upper())
        return input_device.upper()
    if input_device in AXES_READABLE:
        return t('s_js_e_achse', AXES_READABLE[input_device])
    if input_device in ROTATION_AXES:
        return t('s_js_e_drehachse', ROTATION_AXES[input_device])
    hit = re.match(r'^button(\d+)$', input_device)
    if hit:
        return t('s_js_e_knopf', int(hit.group(1)))
    hit = re.match(r'^slider(\d+)$', input_device)
    if hit:
        return t('s_js_e_schieber', int(hit.group(1)))
    hit = re.match(r'^hat(\d+)_(\w+)$', input_device)
    if hit:
        directions = {'up': '↑', 'down': '↓', 'left': '←', 'right': '→'}
        arrow = directions.get(hit.group(2), hit.group(2))
        return t('s_js_e_hut', int(hit.group(1)), arrow)
    return input_device


def kind_of(device_id):
    """Aus `js1` wird `joystick`, aus `kb1` `tastatur`."""
    for kind, abbrev in KINDS:
        if device_id.startswith(abbrev):
            return kind
    return ''


# Welches Feld der `defaultProfile.xml` zu welcher Vorsilbe gehoert.
DEFAULT_FIELD = {'keyboard': 'kb1', 'joystick': 'js1', 'mouse': 'mo1',
                 'gamepad': 'gp1'}

# Die vier Sichten, die es zu sehen gibt.
MINE    = 'meine'     # nur, was der Spieler selbst geaendert hat
DEFAULT = 'standard'  # nur die Werkseinstellung des Spiels
ALL    = 'alles'     # beides zusammengefuehrt — die wirkliche Belegung
FREE     = 'frei'      # Aktionen, auf die noch gar nichts zeigt


def group_of(action, game_folder=None):
    """In welchem `actionmap` lebt eine Aktion?

    Wird beim Neubelegen gebraucht: Star Citizen sortiert Aktionen in
    Gruppen, und eine Belegung in der falschen Gruppe findet das Spiel nicht.
    """
    return ((_profile(game_folder) or {}).get('gruppen') or {}).get(action, '')


def unbound(game_folder=None, filename=None):
    """Aktionen, auf die weder eigene noch Werksbelegung zeigt.

    ⭐ **Ohne diese Liste kaeme man an sie gar nicht heran.** Die Belegungs-
    ansicht zeigt, was belegt ist — eine Aktion ohne jede Belegung taucht dort
    naturgemaess nicht auf, und der Spieler koennte sie nie anklicken, um sie
    zu belegen. Am 04.09.2026 gemessen: **411 von 646** benannten Aktionen
    sind ab Werk unbelegt (Emotes, Bergbau-Feinheiten, Notfallbefehle).

    Liefert dieselbe Form wie `view()`, unter dem Schluessel `frei`, mit
    leerer `input_device`.
    """
    profile_data = _profile(game_folder) or {}
    named = profile_data.get('etiketten') or {}
    bound_actions = set()
    for entries in (view(ALL, filename, game_folder) or {}).values():
        for e in entries:
            bound_actions.add(e['aktion'])
    groups = profile_data.get('gruppen') or {}
    out = []
    for action, pair in named.items():
        if action in bound_actions or not (pair or [''])[0]:
            continue
        out.append({'eingabe': '', 'aktion': action,
                       'bereich': groups.get(action, ''),
                       'art': '', 'quelle': FREE})
    # Nach Gruppe, dann nach Name — so stehen zusammengehoerige Aktionen
    # beieinander (alle Emotes, alle Bergbau-Befehle).
    out.sort(key=lambda e: (e['bereich'], e['aktion']))
    return {FREE: out} if out else {}


def default_bindings(game_folder=None):
    """Die Werkseinstellung des Spiels, im Format von `bindings()`.

    ⚠ Der Standard kennt nur **ein** Geraet je Art (`js1`, `kb1` …) — das
    Spiel legt seine Vorgaben nicht je angeschlossenem Stick ab. Wer zwei
    Sticks fliegt, findet die Vorgaben deshalb komplett unter `js1`.
    """
    profile_data = _profile(game_folder) or {}
    defaults = profile_data.get('standard') or {}
    # ⚠ **Nur Aktionen, die das Spiel selbst benennt.** Ohne `UILabel` taucht
    # eine Aktion auch in den Spieloptionen nicht auf — es sind interne und
    # Entwickler-Befehle (`retry`, `flycam_play`, `hacking_minigame_abort`).
    # Sie mit anzuzeigen blaeht die Liste um rund 180 Zeilen auf, die niemand
    # belegen kann. Eigene Belegungen bleiben davon unberuehrt: Was der
    # Spieler selbst eingetragen hat, wird immer gezeigt.
    named = profile_data.get('etiketten') or {}
    out = {}
    for action, fields in defaults.items():
        if not (named.get(action) or [''])[0]:
            continue
        for field, input_device in fields.items():
            device_id = DEFAULT_FIELD.get(field)
            if not device_id or not input_device:
                continue
            # Im Standard steht die Eingabe teils mit, teils ohne Vorsilbe.
            hit = PREFIX.match(input_device)
            plain = input_device[hit.end():] if hit else input_device
            if hit:
                device_id = hit.group(1) + hit.group(2)
            out.setdefault(device_id, []).append({
                'eingabe': plain,
                'aktion': action,
                'bereich': '',
                'art': kind_of(device_id),
                'quelle': DEFAULT,
            })
    for entries in out.values():
        entries.sort(key=_sort_key)
    return out


def view(which=ALL, filename=None, folder=None):
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
    if which == FREE:
        return unbound(folder, filename)
    own = bindings(filename, folder)
    if which == MINE:
        for entries in own.values():
            for e in entries:
                e['quelle'] = MINE
        return {k: [e for e in v if e['eingabe']] for k, v in own.items()}
    if which == DEFAULT:
        return default_bindings(folder)

    # Zusammenfuehren: erst merken, welche Aktion der Spieler auf welcher
    # **Geraeteart** angefasst hat — siehe die Warnung oben.
    touched = set()
    for device_id, entries in own.items():
        kind = kind_of(device_id)
        for e in entries:
            touched.add((e['aktion'], kind))

    out = {}
    for device_id, entries in default_bindings(folder).items():
        kind = kind_of(device_id)
        remainder = [dict(e) for e in entries
                if (e['aktion'], kind) not in touched]
        if remainder:
            out[device_id] = remainder
    for device_id, entries in own.items():
        for e in entries:
            if not e['eingabe']:
                continue           # geloeschte Belegung — nichts anzuzeigen
            new = dict(e)
            new['quelle'] = MINE
            out.setdefault(device_id, []).append(new)
    for entries in out.values():
        entries.sort(key=_sort_key)
    return out


def _sort_key(entry):
    """Achsen vor Knoepfen, Knoepfe nach Zahl statt nach Text.

    Ohne das steht `button10` vor `button2`, was beim Nachschlagen jedes Mal
    stolpern laesst.

    ⚠ **Nur bei Sticks.** Auf einer Tastatur sind `x`, `y` und `z` schlicht
    Buchstaben — die als Achsen nach vorn zu sortieren, stellt die halbe
    Tastatur an den Anfang. Tastatur und Maus werden deshalb alphabetisch
    sortiert.
    """
    e = entry['eingabe']
    if entry.get('art') in ('tastatur', 'maus'):
        return (0, 0, e)
    axes = ('x', 'y', 'z', 'rotx', 'roty', 'rotz')
    if e in axes:
        return (0, axes.index(e), '')
    if e.startswith('slider'):
        return (1, _trailing_number(e), e)
    if e.startswith('button'):
        return (2, _trailing_number(e), e)
    return (3, 0, e)


def _trailing_number(text):
    hit = re.search(r'(\d+)', text)
    return int(hit.group(1)) if hit else 0


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
INI_FOLDERS = {'de': ('german_(germany)', 'german'), 'en': ('english',)}

# {(sprache, Quellenmarke): {aktion: (name, beschreibung)}}
# ⚠ Die Marke gehoert in den Schluessel, nicht nur die Sprache — siehe
# `_source_mark()`. Aendert sich der Spielordner oder eine Uebersetzungsdatei,
# entsteht dadurch von selbst ein neuer Eintrag.
_LABELS = {}


# ⚠ Aendert sich, was `_profile()` merkt, muss diese Zahl hoch — sonst liest
# eine neue Fassung den Merker der alten und findet die neuen Felder nicht.
#
# ⭐ **4 seit dem 12.09.2026**, und diesmal aus einem anderen Grund: Nicht der
# Inhalt hat sich geaendert, sondern die Art, wie der Stand gebildet wird
# (siehe `_p4k_mark()`). Alte Eintraege tragen einen Stand, der nach der
# neuen Regel nicht mehr zustande kaeme — sie muessen weg.
CACHE_VERSION = 4


def _p4k_mark(game_folder=None):
    """Woran man erkennt, dass ein ANDERES Archiv vorliegt.

    ⚠⚠ **Der blosse Zeitstempel reicht nicht**, und die Begruendung „die
    Datei ist 100 GB gross, zwei Fassungen in derselben Sekunde gibt es
    nicht" geht am Fall vorbei: Das Archiv muss nicht *geschrieben* werden,
    um zu wechseln. Zwei Installationen nebeneinander, eine Ruecksicherung,
    eine Kopie mit erhaltenem Datum — und der gemeinsame Merker
    `aktionsnamen.json` liefert weiter die Etiketten der anderen Installation.
    Vom Pruefer am 12.09.2026 nachgestellt.

    Deshalb: **Pfad, Groesse und `st_mtime_ns`** — reine Metadaten, kein
    Lesen. Der aeussere Merker in `labels()` kann das nicht heilen; er
    liegt im Arbeitsspeicher, dieser hier auf der Platte.
    """
    from . import gametext
    try:
        path = gametext.p4k_path(game_folder)
        if not path:
            return ''
        path = os.path.realpath(path)
        st_info = os.stat(path)
        return '%s:%d:%d' % (path, st_info.st_size, st_info.st_mtime_ns)
    except Exception:
        return ''


def _profile(game_folder=None):
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
    empty_result = {'etiketten': {}, 'standard': {}, 'gruppen': {}}
    cache_file = pfade.app_datei('aktionsnamen.json')
    mark = _p4k_mark(game_folder)
    try:
        if os.path.isfile(cache_file):
            with open(cache_file, 'r', encoding='utf-8') as f:
                cached = json.load(f)
            if (cached.get('fassung') == CACHE_VERSION
                    and cached.get('stand') == mark
                    and cached.get('etiketten')):
                return cached
    except Exception:
        pass

    from . import cryxml, gametext
    out = {'fassung': CACHE_VERSION, 'stand': mark,
              'etiketten': {}, 'standard': {}, 'gruppen': {}}
    try:
        p4k = gametext.p4k_path(game_folder)
        with open(p4k, 'rb') as f:
            directory, _ = gametext.read_directory(
                f, os.path.getsize(p4k))
            method, cs, rs, off = gametext.find_entry(
                directory, 'Data/Libs/Config/defaultProfile.xml')
            raw = gametext.fetch_block(f, off, cs)
        data = (gametext.unpack_zstd(raw, rs)[0] if method == 100
                 else __import__('zlib').decompress(raw, -15))
        root = cryxml.read(data)
        # ⚠ Über die **Gruppen** gehen, nicht flach über alle `action`-Knoten:
        # Nur so kommt mit, in welchem `actionmap` eine Aktion lebt. Ohne die
        # Gruppe landet eine neu angelegte Belegung in der falschen Sektion,
        # und das Spiel findet sie nicht.
        for group in cryxml.find_all(root, 'actionmap'):
            section = (group.get('attribute') or {}).get('name', '')
            for node in (group.get('kinder') or []):
                if node.get('name') != 'action':
                    continue
                at = node.get('attribute') or {}
                name = at.get('name')
                if name and section:
                    out.setdefault('gruppen', {})[name] = section
        for node in cryxml.find_all(root, 'action'):
            at = node.get('attribute') or {}
            name = at.get('name')
            if not name:
                continue
            out['etiketten'][name] = [at.get('UILabel', ''),
                                         at.get('UIDescription', '')]
            preset = {}
            for field in ('keyboard', 'joystick', 'mouse', 'gamepad'):
                value = (at.get(field) or '').strip()
                # ⚠ Ein leeres Feld heisst „ab Werk nicht belegt" und ist
                # etwas anderes als „gar kein Feld". Beides kommt vor.
                if value:
                    preset[field] = value
            if preset:
                out['standard'][name] = preset
    except Exception as exception:
        # Ohne diese Datei bleibt die Liste benutzbar — dann stehen dort die
        # technischen Namen und nur die eigenen Aenderungen. Schlechter, aber
        # nicht kaputt.
        fehler.merken('joysticks.profile', exception)
        return empty_result

    try:
        pfade.json_sichern(cache_file, out)
    except Exception:
        pass
    return out


def _ini_texts(language, game_folder=None):
    """Die `ui_…`-Zeilen der `global.ini` in der gewuenschten Sprache.

    Gelesen werden nur Zeilen, die mit `ui_` beginnen — die Datei hat rund
    12 MB, und alles andere wird hier nicht gebraucht.
    """
    base = os.path.join(game_folder or pfade.spiel_ordner() or '',
                         'data', 'Localization')
    if not os.path.isdir(base):
        base = os.path.join(game_folder or pfade.spiel_ordner() or '',
                             'Data', 'Localization')
    out = {}
    for folder in INI_FOLDERS.get(language, ('english',)):
        path = os.path.join(base, folder, 'global.ini')
        if not os.path.isfile(path):
            continue
        try:
            with open(path, 'r', encoding='utf-8', errors='replace') as f:
                for line in f:
                    if not line.startswith('ui_'):
                        continue
                    key, _, value = line.partition('=')
                    if value:
                        out[key.strip()] = value.strip()
        except Exception:
            continue
        if out:
            break
    return out


def _source_mark(language, game_folder=None):
    """Woran man erkennt, dass die Klarnamen-Quellen sich geaendert haben.

    ⚠⚠ **Der Merker allein nach Sprache reicht nicht.** Die Namen kommen aus
    zwei Dateien im SPIELORDNER — `defaultProfile.xml` und der `global.ini`
    der jeweiligen Sprache. Wer den Spielordner umstellt oder das Spiel
    aktualisiert, bekam bisher weiter die alten Namen; nur ein `forget()`
    half, und das wurde aus Tempogruenden seltener gerufen.

    Vom Pruefer am 12.09.2026 nachgestellt: Uebersetzungsquelle aendern,
    Sprache gleich lassen — `labels()` lieferte den alten Namen.

    Die Marke ist absichtlich **billig**: Pfad, Groesse und Zeitstempel, kein
    Lesen des Inhalts. Die `global.ini` hat rund 12 MB.

    ⚠⚠ **Sie muss die Dateien beobachten, die WIRKLICH gelesen werden.** Die
    erste Fassung nahm eine lose `Data/defaultProfile.xml` — die gibt es gar
    nicht: `_profile()` holt sie aus dem Archiv `Data.p4k`. Und bei deutscher
    Oberflaeche fehlte die englische `global.ini`, obwohl `labels()`
    daraus jede fehlende Uebersetzung nachtraegt. Beide echten Quellen
    konnten sich also aendern, ohne dass die Marke sich ruehrte — vom Pruefer
    am 12.09.2026 nachgestellt.

    ⚠ **Und `st_mtime_ns`, nicht `int(st_mtime)`.** Auf Sekunden gerundet
    fallen zwei gleich grosse Fassungen derselben Sekunde zusammen. Das
    kostet nichts und schliesst eine Luecke.

    ⚠ Sie bleibt trotzdem eine **Heuristik**, kein Inhaltsbeweis. Eigene
    Schreibwege (`injektion`) sind gedeckt — sie schreiben die Datei neu und
    aendern damit Groesse und Zeitstempel. Nicht gedeckt ist eine
    Ruecksicherung oder Kopie **mit erhaltenem Aenderungsdatum**: gleich
    gross, gleicher Zeitstempel, anderer Inhalt. Wer das sicher erkennen
    will, braucht eine Inhaltspruefung — und die kostet bei 12 MB je Anzeige
    mehr, als der ganze Merker einspart. Wer hier kuenftig selbst schreibt,
    ohne den Zeitstempel zu aendern, ruft `forget()`.
    """
    folder = game_folder or pfade.spiel_ordner() or ''
    # Das Archiv, aus dem die Etiketten kommen — ueber **dieselbe** Funktion
    # wie der Merker auf der Platte, damit beide dasselbe Archiv meinen.
    paths = []
    # Jede `global.ini`, die `labels()` anfassen kann — die der Sprache
    # UND die englische, denn sie fuellt die Luecken der Uebersetzung.
    folder_names = list(INI_FOLDERS.get(language, ('english',)))
    if language != 'en':
        folder_names += [n for n in INI_FOLDERS['en'] if n not in folder_names]
    for sub in ('data', 'Data'):
        for name in folder_names:
            paths.append(os.path.join(folder, sub, 'Localization', name,
                                     'global.ini'))
    parts = [folder, language, _p4k_mark(game_folder)]
    for path in paths:
        try:
            st_info = os.stat(path)
            parts.append('%s:%d:%d' % (path, st_info.st_size,
                                       st_info.st_mtime_ns))
        except OSError:
            continue                  # Datei gibt es nicht — zaehlt als „leer"
    return '|'.join(parts)


def labels(language='de', game_folder=None):
    """Aktion → (lesbarer Name, Beschreibung) in der gewuenschten Sprache.

    Fehlt eine der Quellen, kommt ein leeres Woerterbuch zurueck und die
    Oberflaeche zeigt weiter die technischen Namen. **Kein Raten:** Ein
    Etikett ohne Eintrag in der `global.ini` bleibt weg, statt aus dem
    Schluessel einen huebschen Namen zu basteln.
    """
    # ⚠ Der Schluessel ist NICHT nur die Sprache, sondern auch der Zustand der
    # Quelldateien — sonst bleiben die Namen stehen, wenn sich der Spielordner
    # oder das Spiel geaendert hat. Siehe `_source_mark()`.
    key = (language, _source_mark(language, game_folder))
    cache_file = _LABELS.get(key)
    if cache_file is not None:
        return cache_file
    label_pairs = (_profile(game_folder) or {}).get('etiketten') or {}
    texts = _ini_texts(language, game_folder)
    # ⚠ Rueckfall auf Englisch: Wo CIG keinen deutschen Text hinterlegt hat,
    # ist der englische immer noch besser als ein technisches Kuerzel.
    fallback = (_ini_texts('en', game_folder) if language != 'en' else {})
    out = {}
    for action, pair in (label_pairs or {}).items():
        label, description = ((pair or []) + ['', ''])[:2]
        # ⚠⚠ **Nicht `schluessel` nennen.** Genau das hiess hier bis zum
        # 12.09.2026 so wie der Merker-Schluessel oben — die Schleife
        # ueberschrieb ihn, und abgelegt wurde am Ende unter dem letzten
        # Etikett (`'ui_…'`). Gesucht wird aber nach `(Sprache, Marke)`: Der
        # Merker traf nie, die 12-MB-INI wurde bei **jedem** Aufruf neu
        # gelesen. Vom Pruefer nachgestellt — zwei unveraenderte Aufrufe,
        # zwei Lesevorgaenge.
        l_key = (label or '').lstrip('@')
        name = texts.get(l_key) or fallback.get(l_key) or ''
        h_key = (description or '').lstrip('@')
        hint = texts.get(h_key) or fallback.get(h_key) or ''
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
            name = _technical_readable(action)
            out[action] = (name, hint, False)
            continue
        out[action] = (name, hint, True)
    _LABELS[key] = out
    return out


# Die Vorsilben, mit denen das Spiel seine Aktionen sortiert. Sie sagen nur,
# in welchem Zusammenhang die Aktion steht, und stehen in der Anzeige im Weg.
ACTION_PREFIXES = ('v_', 'pl_', 'ui_', 'mg_', 'ca_', 'sc_')


def _technical_readable(action):
    """`v_ads_stable_max_zoom_hold` → `Ads Stable Max Zoom Hold`."""
    remainder = action
    for v in ACTION_PREFIXES:
        if remainder.startswith(v):
            remainder = remainder[len(v):]
            break
    remainder = remainder.replace('_', ' ').strip()
    return ' '.join(w[:1].upper() + w[1:] for w in remainder.split()) or action


def forget():
    """Den Merker leeren — nach einem Sprachwechsel."""
    _LABELS.clear()


def _find_actionmap(root, section):
    """Den `<actionmap>`-Block einer Gruppe holen oder anlegen."""
    for node in root.iter('actionmap'):
        if (node.get('name') or '') == section:
            return node
    new = ET.SubElement(root, 'actionmap')
    new.set('name', section)
    return new


def bind_action(action, section, device_id, input_device, filename=None, folder=None):
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

    path = filename or _actionmaps_path(folder)
    if not path or not os.path.isfile(path):
        return False, 's_js_f_datei', 0
    if not action or not device_id:
        return False, 's_js_f_nichts', 0
    hit = re.match(r'^([a-z]+)(\d+)$', device_id)
    if not hit:
        return False, 's_js_f_nichts', 0
    prefix = hit.group(1)

    try:
        tree = ET.parse(path)
    except Exception as exception:
        fehler.merken('joysticks.bind_action_read', exception)
        return False, 's_js_f_lesen', 0
    root = tree.getroot()

    # Das Spiel legt die Aktionen unter `<ActionProfiles>` ab, nicht direkt
    # unter der Wurzel. Fehlt der Block, ist die Datei nicht die, für die wir
    # sie halten — dann lieber abbrechen.
    parent = root.find('ActionProfiles')
    if parent is None:
        parent = root
    group = _find_actionmap(parent, section or 'spaceship_general')

    target = None
    for node in group.findall('action'):
        if (node.get('name') or '') == action:
            target = node
            break
    if target is None:
        target = ET.SubElement(group, 'action')
        target.set('name', action)

    full = ('%s_%s' % (device_id, input_device)) if input_device else (device_id + '_')
    replaced = False
    for binding in list(target.findall('rebind')):
        existing = (binding.get('input') or '').strip()
        kind = PREFIX.match(existing)
        # Nur Eintraege desselben Geraetetyps anfassen — eine Tastenbelegung
        # darf beim Setzen einer Stick-Belegung nicht verschwinden.
        if kind and kind.group(1) == prefix:
            if replaced:
                target.remove(binding)
            else:
                binding.set('input', full)
                replaced = True
        elif not existing and not kind:
            target.remove(binding)
    if not replaced:
        new = ET.SubElement(target, 'rebind')
        new.set('input', full)

    return _write(path, tree, 1)


def _write(path, tree, anzahl):
    """Den geaenderten Baum sichern und zurueckschreiben.

    ⚠ Hier **muss** ueber den XML-Baum geschrieben werden — anders als beim
    Kennungstausch, der eine reine Textersetzung ist. Deshalb entsteht vorher
    immer eine Sicherung: Geht etwas schief, ist der Rueckweg ein Umbenennen.
    """
    from . import fehler
    backup_file = '%s.scbpw-%s' % (path, time.strftime('%Y%m%d-%H%M%S'))
    try:
        shutil.copy2(path, backup_file)
    except Exception as exception:
        fehler.merken('joysticks.backup', exception)
        return False, 's_js_f_sicherung', 0
    try:
        tree.write(path, encoding='utf-8', xml_declaration=False)
    except Exception as exception:
        try:
            shutil.copy2(backup_file, path)
        except Exception:
            pass
        fehler.merken('joysticks.write', exception)
        return False, 's_js_f_schreiben', 0
    return True, backup_file, anzahl


def conflicts(action, device_id, input_device, filename=None, folder=None):
    """Wer sitzt schon auf dieser Eingabe? Liefert die betroffenen Aktionen.

    ⭐ **Wird VOR dem Belegen gefragt.** Eine Taste doppelt zu belegen ist in
    Star Citizen erlaubt und manchmal gewollt (verschiedene Fahrzeugarten),
    aber meistens ein Versehen — und eines, das man erst im Gefecht merkt.
    Deshalb wird es gezeigt und der Spieler entscheidet, statt dass das
    Programm heimlich etwas wegnimmt.
    """
    if not input_device:
        return []
    out = []
    for kz, entries in (view(ALL, filename, folder) or {}).items():
        if kz != device_id:
            continue
        for e in entries:
            if e['eingabe'] == input_device and e['aktion'] != action:
                out.append(e)
    return out


def reset(filename=None, folder=None):
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
    path = filename or _actionmaps_path(folder)
    if not path or not os.path.isfile(path):
        return False, 's_js_f_datei', 0
    try:
        tree = ET.parse(path)
    except Exception as exception:
        fehler.merken('joysticks.reset_read', exception)
        return False, 's_js_f_lesen', 0

    root = tree.getroot()
    parent = root.find('ActionProfiles')
    if parent is None:
        parent = root
    removed = 0
    for group in list(parent.findall('actionmap')):
        parent.remove(group)
        removed += 1
    if not removed:
        return False, 's_js_f_gleich', 0
    return _write(path, tree, removed)


def export_file(target, language='de', filename=None, folder=None):
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
    source = filename or _actionmaps_path(folder)
    if not source or not os.path.isfile(source):
        return False, 's_js_f_datei'
    try:
        if target.lower().endswith('.csv'):
            names = labels(language, folder)
            lines = ['Geraet;Eingabe;Aktion;Bezeichnung;Gruppe;Quelle']
            for device_id, entries in sorted(view(ALL, filename,
                                                   folder).items()):
                for e in entries:
                    label_text = (names.get(e['aktion']) or ('', '', False))[0]
                    lines.append(';'.join(
                        # ⚠ Semikolon im Text wuerde die Spalten zerreissen —
                        # es kommt in Bezeichnungen des Spiels tatsaechlich vor.
                        (field or '').replace(';', ',')
                        for field in (device_id, e['eingabe'], e['aktion'],
                                     label_text, e['bereich'], e.get('quelle', ''))))
            with open(target, 'w', encoding='utf-8-sig', newline='') as f:
                # ⚠ `utf-8-sig`: Excel liest UTF-8 ohne Vorspann als
                # Windows-1252 und macht aus „Schleudersitz" Buchstabensalat.
                f.write(chr(10).join(lines) + chr(10))
        else:
            shutil.copy2(source, target)
    except Exception as exception:
        fehler.merken('joysticks.export_file', exception)
        return False, 's_js_f_schreiben'
    return True, target


def _as_active_form(root):
    """Ein Profil zurueck in die Form der `actionmaps.xml` bringen.

    ⚠⚠ **Der Rueckweg gehoert zum Hinweg.** Ein Profil aus dem Mappings-Ordner
    traegt seine Angaben an der Wurzel und hat einen `CustomisationUIHeader`;
    die aktive Datei hat ein `<ActionProfiles>` und keinen Kopf. Wer ein Profil
    einfach ueber die aktive Datei kopiert, legt die falsche Form dorthin —
    derselbe Fehler wie beim Ausgeben, nur andersherum.

    Steckt die Datei schon in der aktiven Form, wird sie unveraendert
    zurueckgegeben.
    """
    if root.find('ActionProfiles') is not None:
        return root
    new = ET.Element('ActionMaps')
    profiles_node = ET.SubElement(new, 'ActionProfiles')
    for key in ('version', 'optionsVersion', 'rebindVersion'):
        value = root.get(key)
        if value is not None:
            profiles_node.set(key, value)
    # ⚠ Die aktive Belegung heisst im Spiel immer `default` — der Profilname
    # aus der Datei gilt nur fuer das Profil, nicht fuer die aktive Steuerung.
    profiles_node.set('profileName', 'default')
    for child in list(root):
        if child.tag == 'CustomisationUIHeader':
            continue          # der Kopf gehoert nur ins Profil
        profiles_node.append(child)
    return new


def import_file(source, filename=None, folder=None):
    """Eine zuvor ausgegebene Belegung wieder einspielen.

    Nimmt **beide** Formen an: die Kopie einer `actionmaps.xml` und ein Profil
    aus dem Mappings-Ordner (siehe `_as_active_form`).

    ⚠ Es wird geprueft, ob die Datei ueberhaupt danach aussieht — sonst
    landet irgendeine XML-Datei als Steuerung im Spiel. Und auch hier gilt:
    erst Sicherung, dann schreiben.
    """
    from . import fehler
    target = filename or _actionmaps_path(folder)
    if not target or not os.path.isfile(target):
        return False, 's_js_f_datei', 0
    if not source or not os.path.isfile(source):
        return False, 's_js_f_datei', 0
    try:
        tree = ET.parse(source)
    except Exception:
        return False, 's_js_f_fremd', 0
    root = tree.getroot()
    if root.tag != 'ActionMaps':
        return False, 's_js_f_fremd', 0
    anzahl = len(list(root.iter('actionmap')))
    try:
        backup_file = '%s.scbpw-%s' % (target, time.strftime('%Y%m%d-%H%M%S'))
        shutil.copy2(target, backup_file)
        ET.ElementTree(_as_active_form(root)).write(
            target, encoding='utf-8', xml_declaration=False)
    except Exception as exception:
        fehler.merken('joysticks.import_file', exception)
        return False, 's_js_f_schreiben', 0
    return True, backup_file, anzahl


def swap_id(old_id, new_id, new_name='', filename=None, folder=None):
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
    # Modulebene waere das ein Zirkelbezug.
    from . import fehler

    path = filename or _actionmaps_path(folder)
    if not path or not os.path.isfile(path):
        return False, 's_js_f_datei', 0
    if not old_id or not new_id or old_id == new_id:
        return False, 's_js_f_nichts', 0
    try:
        with open(path, 'r', encoding='utf-8', errors='replace') as f:
            content = f.read()
    except Exception as exception:
        fehler.merken('joysticks.read', exception)
        return False, 's_js_f_lesen', 0

    # ⚠ Gross-/Kleinschreibung der Kennung kann sich zwischen Protokoll und
    # Datei unterscheiden — deshalb wird ohne Ruecksicht darauf gesucht, aber
    # in der Schreibweise ersetzt, die in der Datei steht.
    pattern = re.compile(re.escape(old_id), re.IGNORECASE)
    hit = len(pattern.findall(content))
    if not hit:
        return False, 's_js_f_unbekannt', 0

    new = pattern.sub(new_id, content)

    # Hat das Spiel fuer das Geraet bereits einen zweiten, leeren Eintrag
    # angelegt, staende die neue Kennung nun zweimal da. Der spaetere (leere)
    # Eintrag wird geleert, damit genau eine Zuordnung uebrig bleibt.
    new = _clear_duplicate_entry(new, new_id)

    if new == content:
        return False, 's_js_f_gleich', 0

    backup_file = '%s.scbpw-%s' % (path, time.strftime('%Y%m%d-%H%M%S'))
    try:
        shutil.copy2(path, backup_file)
    except Exception as exception:
        # Ohne Sicherung wird nicht geschrieben. Lieber gar nicht helfen als
        # ohne Rueckweg — hier haengt die komplette Steuerung dran.
        fehler.merken('joysticks.backup', exception)
        return False, 's_js_f_sicherung', 0
    try:
        with open(path, 'w', encoding='utf-8', newline='') as f:
            f.write(new)
    except Exception as exception:
        try:
            shutil.copy2(backup_file, path)
        except Exception:
            pass
        fehler.merken('joysticks.write', exception)
        return False, 's_js_f_schreiben', 0
    return True, backup_file, hit


def swap_bindings(id_a, id_b, filename=None, folder=None):
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

    ⚠⚠ **Und es geschieht in EINEM Durchgang.** Zweimal `swap_id`
    (A→B, dann B→A) waere falsch: Der zweite Lauf fande auch die gerade
    geschriebenen B's und drehte alles zurueck. Deshalb ersetzt ein einziger
    Ausdruck beide Kennungen gleichzeitig.

    Liefert `(erfolg, meldung, anzahl)` wie die Nachbarfunktionen.
    """
    from . import fehler

    path = filename or _actionmaps_path(folder)
    if not path or not os.path.isfile(path):
        return False, 's_js_f_datei', 0
    a = (id_a or '').strip()
    b = (id_b or '').strip()
    if not a or not b or a.upper() == b.upper():
        return False, 's_js_f_nichts', 0

    try:
        with open(path, 'r', encoding='utf-8', errors='replace') as f:
            content = f.read()
    except Exception as exception:
        fehler.merken('joysticks.swap_read', exception)
        return False, 's_js_f_lesen', 0

    # Nur die Joystick-Bloecke — `<deviceoptions>` bleiben aussen vor.
    block_pattern = re.compile(
        r'<options\b[^>]*\btype="joystick"[^>]*/>|'
        r'<options\b[^>]*\btype="joystick"[^>]*>.*?</options>', re.S)
    pair = re.compile('(%s|%s)' % (re.escape(a), re.escape(b)),
                      re.IGNORECASE)
    swapped = [0]

    def cross(hit):
        """Jede gefundene Kennung durch die jeweils andere ersetzen."""
        found = hit.group(0)
        swapped[0] += 1
        return b if found.upper() == a.upper() else a

    def rewrite_block(hit):
        return pair.sub(cross, hit.group(0))

    new = block_pattern.sub(rewrite_block, content)

    if swapped[0] < 2:
        # Weniger als zwei Treffer heisst: Mindestens eines der beiden Geraete
        # hat gar keinen Block — dann gaebe es nichts zu tauschen, und ein
        # halber Tausch waere schlimmer als keiner.
        return False, 's_js_f_unbekannt', 0
    if new == content:
        return False, 's_js_f_gleich', 0

    backup_file = '%s.scbpw-%s' % (path, time.strftime('%Y%m%d-%H%M%S'))
    try:
        shutil.copy2(path, backup_file)
    except Exception as exception:
        fehler.merken('joysticks.backup', exception)
        return False, 's_js_f_sicherung', 0
    try:
        with open(path, 'w', encoding='utf-8', newline='') as f:
            f.write(new)
    except Exception as exception:
        try:
            shutil.copy2(backup_file, path)
        except Exception:
            pass
        fehler.merken('joysticks.swap_write', exception)
        return False, 's_js_f_schreiben', 0
    return True, backup_file, swapped[0]


def _clear_duplicate_entry(content, ident):
    """Steht dieselbe Kennung in zwei `<options>`-Koepfen, bleibt der erste.

    Der zweite wird zu einem leeren Platz (`<options type="joystick"
    instance="N"/>`) — genau die Form, die das Spiel fuer unbelegte Plaetze
    selbst schreibt.
    """
    head = re.compile(r'<options\b[^>]*?\btype="joystick"[^>]*?>')
    seen = [False]

    def ersetzen(hit):
        whole = hit.group(0)
        if ident.upper() not in whole.upper():
            return whole
        if not seen[0]:
            seen[0] = True
            return whole
        number = re.search(r'instance="(\d+)"', whole)
        if not number:
            return whole
        return '<options type="joystick" instance="%s"/>' % number.group(1)

    return head.sub(ersetzen, content)


