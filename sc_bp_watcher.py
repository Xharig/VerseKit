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
SC BP Watcher — zeigt live an, sobald im SC Deutsch Launcher ein neuer
Bauplan (Blueprint) freigeschaltet wird.

Überwacht:  die Star-Citizen-Game.log (die eigentliche Quelle) und liest beim
            Start auch die aufgehobenen Logs vergangener Sitzungen nach
            + den SC Deutsch Launcher, **falls** er vorhanden ist (er bestätigt
              die Funde und liefert einen gepflegten Katalog)
            + bp_item_types.json bzw. die scmdb-Craftdaten als Katalog-Wache —
              meldet, was im Spiel NEU craftbar wurde
Bestand:    wird ab v2.0 selbst geführt (`bestand.json` im eigenen Ordner) —
            der SC Deutsch Launcher ist damit kein Muss mehr.
Werte:      Art/Größe/Gütegrad/Klasse aus dem Launcher-Katalog, sonst von
            scmdb.net (seit v1.5.0).
Anzeige:    kleines, immer-im-Vordergrund Overlay-Fenster (verschiebbar).

Reines Python-Standardbibliothek-Tool (tkinter) — keine Zusatzpakete nötig.
Läuft unter **Windows und Linux**; wo die Dateien jeweils liegen, weiß `scbp/pfade.py`.
"""
import os, re, sys, json, time, threading, queue
import tkinter as tk
from tkinter import font as tkfont

# Eigene Bausteine. Sie kapseln alles, was sich zwischen Windows und Linux
# unterscheidet — der Rest dieser Datei muss das Betriebssystem nicht kennen.
from scbp import sprache
from scbp import zeichen
from scbp import fehler
from scbp import hinweis
from scbp import (
    auftraege,ablagesymbol, aktualisierung, assistent, autostart, orte, preise,
                  bildschirm, overlay,
                  bestand as bestand_datei, bestandsfenster as bestandsfenster_modul,
                  einstellungsfenster, hinweis, injektion,
                  katalog as katalog_modul, laeden, logquelle, merkliste,
                  pfade, phrasen, schiffe, spielstand, titelleiste, ton,
                  uebersetzung, verkauf, hotkey as hotkey_modul)

try:
    import winsound                      # nur Windows; unter Linux übernimmt tkinter
except ImportError:
    winsound = None

__version__ = '3.17.0'


def _mitgeliefert(name):
    """Pfad zu einer mitgelieferten Datei — im Quellcode wie im fertigen Paket.

    PyInstaller entpackt alles nach `sys._MEIPASS`; daneben zu suchen geht dort
    ins Leere. Beim Start aus dem Quellcode gibt es das Attribut nicht, dann
    gilt der Ordner dieser Datei.
    """
    try:
        basis = getattr(sys, '_MEIPASS', None) or os.path.dirname(
            os.path.abspath(__file__))
        return os.path.join(basis, name)
    except Exception:
        return None

# ---------------------------------------------------------------- Konfiguration
# Wo die Dateien liegen, entscheidet `scbp/pfade.py` je nach Betriebssystem.
# Der SC Deutsch Launcher ist ab jetzt **optional**: Ist er da, wird er genutzt;
# fehlt er (immer unter Linux), fällt nur seine Bestätigung weg — gemeldet wird
# trotzdem, denn die Game.log ist die eigentliche Quelle.
BP_DIR   = pfade.launcher_ordner() or ''
BP_FILE  = pfade.launcher_datei('sc_bp_erledigt.json', BP_DIR)
TYPE_FILE = pfade.launcher_datei('bp_item_types.json', BP_DIR)
CAT_DIR  = pfade.launcher_datei('catalog', BP_DIR)               # Launcher-Katalog (Size/Grade/Klasse)
HAT_LAUNCHER = bool(BP_DIR) and os.path.isdir(BP_DIR)
# Manuelle Korrekturen an Size/Grade/Klasse, Vorrang vor dem Launcher-Katalog.
# Standard: neben den eigenen Einstellungen in %APPDATA%\sc-bp-watcher\.
# Wer die Datei woanders pflegt, setzt die
# Umgebungsvariable SC_BP_OVERRIDES auf den vollen Pfad. Fehlt beides, gilt der
# Katalog unverändert — die Datei ist optional.
OVERRIDES_FILE = os.environ.get('SC_BP_OVERRIDES') or pfade.app_datei(
    'bp-overrides.json')
# Wie oft die Game.log angesehen wird. Einstellbar über `pruefintervall_sekunden`
# in der `einstellungen.json`; 3 Sekunden sind ein guter Mittelweg zwischen
# „steht sofort da" und „liest dauernd die Platte". Grenzen 1–60, damit eine
# vertippte 0 keine Dauerschleife wird.
POLL_SEC = pfade.einstellung_zahl('pruefintervall_sekunden', 3, 1, 60)
# Signalton bei einem Fund — manche wollen im Spiel keinen zusätzlichen Ton.
TON_AN = pfade.einstellung_wahrheit('signalton', True)
DECKKRAFT = pfade.einstellung_zahl('deckkraft_prozent', 93, 30, 100)
# So viele Neuzugänge bleiben im Overlay stehen, ältere rutschen heraus.
#
# ⚠ Zweierlei war hier falsch. Erstens war die Zahl **fest** — die Einstellung
# „Zeilen im Overlay" wurde brav gespeichert und dann nie gelesen. Zweitens war
# die Vorgabe 200: So viele Baupläne sammelt in einer Spielsitzung niemand, und
# ein Overlay, das theoretisch 200 Zeilen hoch werden kann, steht im Weg.
# Jetzt gilt die Einstellung, mit 20 als Vorgabe.
MAX_ROWS_VORGABE = 20


def max_zeilen():
    """Wie viele Zeilen das Overlay behält — jedes Mal frisch gelesen, damit
    eine Änderung in den Einstellungen sofort wirkt und nicht erst nach einem
    Neustart."""
    return pfade.einstellung_zahl('max_zeilen', MAX_ROWS_VORGABE, 5, 100)

# --- Katalog-Wache (ab v1.3.0) ---------------------------------------------
# `bp_item_types.json` listet, was im Spiel überhaupt craftbar ist. Der Launcher
# frischt sie mit den SC-Patches auf. Wächst sie, ist etwas NEU craftbar geworden —
# unabhängig davon, ob man es freigeschaltet hat. Der Stand liegt bewusst in einer
# eigenen Datei, damit ein zweites Werkzeug auf denselben Daten dem Watcher
# nicht die Meldung wegnimmt.
APP_DIR    = pfade.app_ordner()
CAT_SEEN   = pfade.app_datei('catalog-seen.json')
# Optionale Beobachtungsliste: Gegenstände, auf die man besonders wartet.
# Format: {"eintraege": [{"titel": "…", "muster": ["teilstring", …]}, …]} — Muster
# kleingeschrieben, Treffer per Teilstring. Fehlt die Datei, meldet der Watcher
# einfach jeden Katalog-Zuwachs.
WATCHLIST  = pfade.app_datei('watchlist.json')
CAT_POLL   = 60         # Katalogdatei nur jede Minute prüfen (ändert sich nur bei Patches)

# --- scmdb-Craftdaten (ab v1.5.0) ------------------------------------------
# Woher Art, Größe, Gütegrad und Klasse kommen, wenn der Launcher-Katalog sie
# nicht kennt (oder gar nicht da ist). scmdb.net liefert je Spielversion eine
# fertige Datei mit genau diesen Werten — kein Entpacken von `Data.p4k` nötig,
# reines urllib aus der Standardbibliothek.
#
#   versions.json                        -> welche Spielversion ist aktuell
#   crafting_items-<version>.json        -> name, attachType, size, grade,
#                                           componentClass, manufacturer
#
# RANGFOLGE (wichtig): bp-overrides.json  >  Launcher-Katalog/Spieldaten  >  scmdb.
# scmdb füllt nur Lücken und überschreibt nie. Grund: Am 11.08.2026 verglichen —
# 55 von 56 Werten stimmen exakt mit dem überein, was das Spiel selbst in die
# Log schreibt, aber beim Kühler „Elsen" nennt scmdb Grad A, während Log UND
# `components.ini` übereinstimmend B sagen (auch der Hersteller ist dort falsch).
# Eine sehr gute Quelle, aber keine unfehlbare.
SCMDB_BASE     = 'https://scmdb.net/data'
SCMDB_CACHE    = pfade.app_datei('scmdb-items.json')   # aufbereitet, klein
SCMDB_POLL_SEC = 6 * 3600    # nur alle 6 Stunden nach einer neuen Spielversion sehen
# Übersetzung und Bauplan-Angaben: beim Start und danach alle sechs Stunden.
# Häufiger bringt nichts — die Quellen aktualisieren im Tagesrhythmus.
TEXTE_POLL_SEC = 6 * 3600
# ⚠ Der EIGENE Bestand hat einen ganz anderen Takt und darf nicht an diesem
# hängen: Er ändert sich, während der Spieler spielt. Bis zum 05.09.2026 hing
# beides zusammen — wer einen Bauplan freischaltete, sah das Kästchen im Spiel
# frühestens sechs Stunden später, und wer das Werkzeug vorher beendete, nie.
# Gemeldet mit einem Lauf von 20 Minuten: 304 Baupläne im Bestand, 303 in der
# eingetragenen Liste.
#
# Das kostet nichts: Der Fingerabdruck des Bestands ist in 0,4 ms gebaut
# (gemessen mit 407 Bauplänen), also 0,013 % eines Drei-Sekunden-Takts.
# Geschrieben wird weiterhin nur, wenn er sich WIRKLICH geändert hat.
BESTAND_POLL_SEC = 30
SCMDB_TIMEOUT  = 30
# Wer die Netzabfrage nicht will, setzt SC_BP_NO_NET=1 — dann bleibt alles beim
# Launcher-Katalog wie bisher.
SCMDB_AUS      = os.environ.get('SC_BP_NO_NET', '') not in ('', '0')
# Gütegrad steht bei scmdb als Zahl. Zuordnung am 11.08.2026 gegen 56 Log-Zeilen
# geprüft: A=1 (21x), B=2 (20x), C=3 (7x), D=4 (7x).
GRADE_LETTER = {1: 'A', 2: 'B', 3: 'C', 4: 'D'}

# Fenstergröße beim allerersten Start. **Ohne feste Position**: Wo das Fenster
# gut aufgehoben ist, hängt am Monitoraufbau, und eine Position vom Rechner des
# Entwicklers ist auf einem anderen im besten Fall unsichtbar — unter macOS
# stürzt Tk dabei sogar ab. Tk sucht sich beim ersten Mal selbst eine Stelle,
# danach gilt die zuletzt gemerkte (siehe `geometrie_pruefen`).
# Wer sie fest vorgeben will, setzt SC_BP_GEOMETRIE (Format BxH+X+Y).
# Nur die **Größe** — die Position wird beim Start ausgerechnet (mittig auf dem
# Hauptbildschirm, siehe `startlage`). Eine feste Position wäre auf jedem anderen
# Rechner falsch, und gar keine Position lässt Tk nach `+0+0` platzieren — bei einem
# hochkant stehenden Monitor links außen ist dort schlicht kein Bild.
DEFAULT_GEOM  = os.environ.get('SC_BP_GEOMETRIE') or '440x1000'
SETTINGS_FILE = pfade.app_datei('watcher.json')

# Farben (dunkles Overlay)
# Xharig-Grün für dunklen Grund. Bis v1.5.0 stand hier noch #47aa42 — die alte
# Markenfarbe von vor dem Logo-Wechsel. Zwei Grüntöne im selben Programm gehen nicht.
BG, FG, ACCENT, SUB, BAR = '#10141c', '#e6edf3', '#9ce430', '#8b98a5', '#1b2230'
# Für das Verbotszeichen an einer Auftragszeile — dieselbe Warnfarbe wie im
# Hauptfenster, damit „hier wird etwas weggenommen" überall gleich aussieht.
ROT = '#e05555'
PROV = '#d8a03a'        # Gelb für „vorläufig" (aus der Game.log, noch nicht vom Launcher bestätigt)
CATA = '#4aa3d8'        # Blau für „neu im Spiel craftbar" (Katalog-Zuwachs, kein eigener Fund)


# ---------------------------------------------------------------- Daten-Helfer
def load_keys():
    """Liest die freigeschalteten BP-Namen. Gibt set() zurück (leer bei Fehler)."""
    try:
        with open(BP_FILE, encoding='utf-8') as f:
            data = json.load(f)
        return {b['key'] for b in data.get('blueprints', [])}
    except Exception:
        return None   # None = Datei (gerade) nicht lesbar -> Tick überspringen


def load_types():
    """Was im Spiel überhaupt craftbar ist: Name -> Art.

    Erste Wahl ist die Launcher-Datei (deutsche Bezeichnungen, gepflegt). Fehlt
    der Launcher — unter Linux immer —, treten die scmdb-Craftdaten an ihre
    Stelle. Ohne diesen Rückfall wäre die Katalog-Wache dort tot, dabei liegen
    die Daten längst im Zwischenspeicher."""
    try:
        with open(TYPE_FILE, encoding='utf-8') as f:
            return json.load(f)
    except Exception:
        pass
    return {name: (eintrag.get('art') or eintrag.get('attachType') or '—')
            for name, eintrag in (SCMDB or {}).items()} if SCMDB else {}


# Die Merkliste steckt in `scbp/merkliste.py` — sie wird im Fenster per Klick
# gepflegt, nicht mehr nur von Hand in der Datei.


# Vorbelegung, damit `load_types()` weiter unten nicht ins Leere greift: Die
# scmdb-Daten werden erst nach diesen Zeilen geladen (sie brauchen Funktionen,
# die weiter unten stehen). Direkt danach wird TYPES neu gesetzt.
SCMDB, SCMDB_VERSION = {}, ''
TYPES = load_types()


def art_of(key):
    global TYPES
    k = key.lower().replace('\xa0', ' ')
    art = TYPES.get(k)
    if art is None:
        # Frisch freigeschaltetes Item kann neu im Katalog sein -> einmal nachladen
        TYPES = load_types()
        art = TYPES.get(k)
    if art is None:
        art = scmdb_art(key)      # ab v1.5.0: Rückfall auf die scmdb-Craftdaten
    return art or '—'


# Rüstungs-Slots von scmdb -> die hier verwendeten Begriffe. Die Gewichtsklasse (Heavy/Medium/
# Light) steht bei scmdb getrennt in `attachSubType`, beim Launcher steckt sie im
# Begriff selbst („Heavy Armor"). Beides wird hier wieder zusammengesetzt.
_SCMDB_SLOT = {
    'Char_Armor_Helmet':    'Helmet',
    'Char_Armor_Torso':     'Armor',
    'Char_Armor_Legs':      'Armor',
    'Char_Armor_Arms':      'Armor',
    'Char_Armor_Backpack':  'Backpack',
    'Char_Armor_Undersuit': 'Undersuit',
}
# Reine Umbenennungen, wo scmdb zusammenschreibt und der Launcher trennt.
_SCMDB_ART = {
    'QuantumDrive':   'Quantum Drive',
    'PowerPlant':     'Power Plant',
    'WeaponGun':      'Ship Weapon',
    'WeaponPersonal': 'FPS Weapon',
    'WeaponMining':   'Mining Laser',
    'SalvageModifier': 'Salvage Modifier',
}


def scmdb_art(key):
    """Art aus den scmdb-Craftdaten, auf die Begriffe des Launchers gebracht.
    Nur Rückfall — der Launcher-Katalog ist bei Schiffswaffen feiner (er kennt
    `Laser Cannon`, scmdb nur `WeaponGun`)."""
    e = scmdb_of(key)
    if not e or not e.get('a'):
        return None
    a = e['a']
    slot = _SCMDB_SLOT.get(a)
    if slot:
        gewicht = (e.get('sub') or '').strip()
        return ('%s %s' % (gewicht, slot)).strip() if gewicht in (
            'Heavy', 'Medium', 'Light') else slot
    return _SCMDB_ART.get(a, a)


# ------------------------------------------------ Size / Grade / Klasse (M/A/1)
# Ableitung: Launcher-Katalog +
# manuelle Overrides (bp-overrides.json, Vorrang). Ausgabe-Kürzel: Klasse/Grade/Size,
# z. B. Military / Grade A / Size 1  ->  "M/A/1". Nur Size (Waffen) -> "–/–/2".
CLASS_LETTER = {'Military': 'M', 'Stealth': 'S', 'Industrial': 'I',
                'Civilian': 'C', 'Competition': 'K'}
_CLASS_FULL  = {'Civ': 'Civilian', 'Mil': 'Military', 'Ind': 'Industrial',
                'Sth': 'Stealth', 'Cmp': 'Competition'}
_CLASS_SHORT = {v: k for k, v in _CLASS_FULL.items()}


def _norm(s):
    return s.lower().replace('\xa0', ' ').replace('�', ' ').strip()


def load_display():
    """Kleingeschriebener Katalog-Schlüssel -> Schreibweise wie im Spiel.
    `bp_item_types.json` führt alles klein; für die Anzeige holen wir den echten
    Namen aus dem Launcher-Katalog. Wird nur bei einem Katalog-Zuwachs gebraucht."""
    d = {}
    try:
        for line in open(os.path.join(CAT_DIR, 'components.ini'), encoding='utf-8'):
            if '=' not in line: continue
            v = line.strip().split('=', 1)[1]
            m = re.match(r'(.*?)\s*\([^/]+/[^/]+/[^)]+\)', v)
            if m: d.setdefault(_norm(m.group(1)), m.group(1).strip())
    except Exception:
        pass
    try:
        for line in open(os.path.join(CAT_DIR, 'items_raw.ini'), encoding='utf-8'):
            if '=' not in line: continue
            k, v = line.split('=', 1); v = v.strip()
            if k.endswith('_short') or not v: continue
            d.setdefault(_norm(v), v)
    except Exception:
        pass
    return d


def _katalogname(schluessel, anzeige):
    """Der Name, wie ein Mensch ihn lesen soll — für die Meldungen der
    Katalog-Wache.

    Drei Quellen, in dieser Reihenfolge:

      1. **Launcher-Katalog** (`anzeige`) — deutsche, gepflegte Bezeichnungen.
         Gibt es nur, wo der SC Deutsch Launcher installiert ist.
      2. **scmdb-Zwischenspeicher** — dort liegt unter `n` der Name, wie ihn
         das Spiel schreibt („GOLEM MC-4 Ore Pod"). Der Rückfall für Linux und
         für jeden ohne Launcher.
      3. **Der nackte Schlüssel** — nur, wenn beides fehlt.

    ⚠ Hier stand früher `schluessel.title()`. Das war falsch: Der Schlüssel ist
    auf Kleinbuchstaben und Ziffern eingedampft (`golemmc4orepod`), da gibt es
    keine Wortgrenzen mehr zurückzuholen — `.title()` machte daraus
    „Golemmc4Orepod". Der lesbare Name lag die ganze Zeit daneben im Cache.
    """
    aus_launcher = anzeige.get(_norm(schluessel))
    if aus_launcher:
        return aus_launcher
    eintrag = SCMDB.get(schluessel) or {}
    return eintrag.get('n') or schluessel


def load_meta():
    """comp[name] = (Klasse, Size, Grade) für Schiffskomponenten;
    size_by_name[name] = Size für Waffen/Werkzeuge. Katalog + Overrides (Vorrang)."""
    comp, size_by_name = {}, {}
    # Schiffskomponenten aus components.ini:  "Name (Klasse/Size/Grade)"
    try:
        for line in open(os.path.join(CAT_DIR, 'components.ini'), encoding='utf-8'):
            if '=' not in line: continue
            _, v = line.strip().split('=', 1)
            m = re.search(r'^(.*?)\s*\(([^/]+)/([^/]+)/([^)]+)\)', v)
            if m: comp[m.group(1).strip().lower()] = (m.group(2), m.group(3), m.group(4))
    except Exception:
        pass
    # Size aus items_raw.ini:  Schlüssel enthält _S1 / _S01 …
    try:
        for line in open(os.path.join(CAT_DIR, 'items_raw.ini'), encoding='utf-8'):
            line = line.rstrip('\n')
            if '=' not in line: continue
            k, v = line.split('=', 1)
            if k.endswith('_short'): continue
            m = re.search(r'_S0?(\d)\b', k)
            if m: size_by_name.setdefault(v.strip().lower(), m.group(1))
    except Exception:
        pass
    # Manuelle Overrides (Vorrang): vollständige Komponente -> comp, nur Size -> size_by_name
    try:
        ov = json.load(open(OVERRIDES_FILE, encoding='utf-8')).get('overrides', {})
        for k, o in ov.items():
            if o.get('class') and o.get('size') and o.get('grade'):
                comp[k] = (_CLASS_SHORT.get(o['class'], o['class']), str(o['size']), o['grade'])
            elif o.get('size') is not None:
                size_by_name[k] = str(o['size'])
    except Exception:
        pass
    return comp, size_by_name


COMP, SIZE_BY_NAME = load_meta()


# ------------------------------------------------------- scmdb-Craftdaten (v1.5.0)
def _scmdb_key(s):
    """Vergleichsschlüssel: nur Buchstaben und Ziffern. Fängt typografische
    Anführungszeichen und geschützte Leerzeichen mit ab, an denen ein reiner
    Kleinschreib-Vergleich sonst scheitert."""
    return re.sub(r'[^a-z0-9]', '', ' '.join(str(s or '').split()).lower())


def _scmdb_hole(url, timeout=SCMDB_TIMEOUT):
    # Ehrliche Kennung mit Projektadresse: Der Betreiber von scmdb.net soll im
    # Protokoll sehen können, welches Werkzeug da abruft und wo er nachfragen
    # kann. Kostet nichts und ist schlicht anständig.
    import urllib.request
    kennung = 'SC-BP-Watcher/%s (+https://github.com/Xharig/SC-BP-Watcher)' % __version__
    req = urllib.request.Request(url, headers={'User-Agent': kennung})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read().decode('utf-8'))


def load_scmdb():
    """Liest den aufbereiteten Zwischenspeicher. Gibt (items, version) zurück."""
    try:
        with open(SCMDB_CACHE, encoding='utf-8') as f:
            d = json.load(f)
        return d.get('items', {}), d.get('version', '')
    except Exception:
        return {}, ''


def scmdb_aktualisieren():
    """Holt die Craftdaten, wenn eine neue Spielversion da ist. Gibt True zurück,
    wenn der Zwischenspeicher erneuert wurde. Wirft nie — ohne Netz bleibt der
    letzte Stand gültig, ohne Zwischenspeicher läuft alles wie vor v1.5.0."""
    if SCMDB_AUS:
        return False
    try:
        versionen = _scmdb_hole(SCMDB_BASE + '/versions.json', timeout=10)
        live = next((v for v in versionen
                     if 'ptu' not in (v.get('version') or '').lower()), None)
        if not live:
            return False
        version = live.get('version') or ''
        if not version or version == load_scmdb()[1]:
            return False          # schon aktuell
        roh = _scmdb_hole('%s/crafting_items-%s.json' % (SCMDB_BASE, version))
        items = {}
        for e in roh.get('items', []):
            name = e.get('name')
            if not name:
                continue
            items.setdefault(_scmdb_key(name), {
                'n': name,
                'a': e.get('attachType') or e.get('cgItemType'),
                'sub': e.get('attachSubType'),   # Heavy/Medium/Light bei Rüstung
                's': e.get('size'),
                'g': e.get('grade'),
                'c': e.get('componentClass'),
                'm': e.get('manufacturer'),
            })
        os.makedirs(APP_DIR, exist_ok=True)
        with open(SCMDB_CACHE, 'w', encoding='utf-8') as f:
            json.dump({'version': version, 'geholt': time.strftime('%Y-%m-%d %H:%M'),
                       'items': items}, f, ensure_ascii=False)
        return True
    except Exception:
        return False


SCMDB, SCMDB_VERSION = load_scmdb()
# Jetzt, wo die scmdb-Daten stehen, kann der Katalog auch ohne Launcher gefüllt
# werden — vorhin war er es nur, wenn die Launcher-Datei da war.
if not TYPES:
    TYPES = load_types()


def scmdb_of(key):
    """Eintrag aus den scmdb-Craftdaten oder None. Skin-/Sondervarianten mit
    Zusatzname in "…" fallen auf den Grundnamen zurück (wie beim Katalog)."""
    if not SCMDB:
        return None
    e = SCMDB.get(_scmdb_key(key))
    if e is None:
        basis = re.sub(r'\s*"[^"]*"\s*', ' ', str(key))
        if basis != key:
            e = SCMDB.get(_scmdb_key(basis))
    return e


def _size_grade_class(key):
    lk = _norm(key)
    if lk in COMP:
        cl, sz, gr = COMP[lk]
        return sz, gr, _CLASS_FULL.get(cl, cl)
    s = SIZE_BY_NAME.get(lk) or SIZE_BY_NAME.get(key.lower())
    if s is None:  # Skin-/Sondervariante (Zusatz in "…") erbt die Size der Basis
        base = re.sub(r'\s+', ' ', re.sub(r'\s*"[^"]*"\s*', ' ', lk)).strip()
        if base != lk: s = SIZE_BY_NAME.get(base)
    # Rückfall auf scmdb — füllt nur, was die Spieldaten nicht hergeben.
    #
    # ACHTUNG: scmdb vergibt `size` und `grade` an JEDEN Gegenstand, auch an
    # Rüstung und FPS-Waffen, wo beides bedeutungslos ist (ein Helm als „Grade A,
    # Size 1"). Ungefiltert übernommen stünde hinter jedem Rüstungsteil ein
    # erfundenes Kürzel. Deshalb:
    #   * Klasse/Gütegrad nur, wenn scmdb eine `componentClass` führt — das sind
    #     genau die echten Schiffskomponenten (489 von 1591).
    #   * Größe zusätzlich für Schiffswaffen, die haben eine, aber keinen Grad.
    #   * Rüstung, FPS-Waffen, Kleidung: nichts.
    e = scmdb_of(key)
    if e:
        if e.get('c'):                                   # echte Schiffskomponente
            if s is None and e.get('s') is not None:
                s = str(e['s'])
            return s, GRADE_LETTER.get(e.get('g')), e.get('c')
        if e.get('a') == 'WeaponGun' and s is None and e.get('s') is not None:
            s = str(e['s'])                              # Schiffswaffe: nur Größe
    return s, None, None


def meta_of(key):
    """Kürzel Klasse/Grade/Size, z. B. 'M/A/1'. '' wenn nichts bekannt (FPS-Waffe,
    Rüstung). Fehlende Einzelwerte werden als '–' angezeigt."""
    sz, gr, cl = _size_grade_class(key)
    if sz is None and gr is None and cl is None:
        return ''
    c = CLASS_LETTER.get(cl, '–') if cl else '–'
    return f'{c}/{gr or "–"}/{sz or "–"}'


# ------------------------------------------------------- Game.log (Sofort-Meldung)
# Das Lesen der Log steckt seit v1.6 in `scbp/logquelle.py` — samt Nachlese der
# aufgehobenen Sitzungen und einem Lesestand, der Programmneustarts übersteht.
# Welche Formulierung im Log steht, hängt an der Spielsprache; darum kümmert
# sich `scbp/phrasen.py`. Hier bleibt nur, was mit der ANZEIGE zu tun hat.


def kuerzel_aus_zusatz(zusatz):
    """('Civ', '3', 'A') -> 'C/A/3'.

    Der Zusatz hinter dem Namen im Log ist der Rückfall fürs Kürzel, falls ein
    Gegenstand nach einem SC-Patch noch in keinem Katalog steht."""
    if not zusatz:
        return None
    klasse, size, grade = zusatz
    letter = CLASS_LETTER.get(_CLASS_FULL.get(klasse, klasse), '–')
    return f'{letter}/{grade}/{size}'


def _loose(name):
    """Name ohne Klammer-Zusatz am Ende — für den Notfall-Abgleich, wenn Log und
    Launcher unterschiedlich übersetzt sind (gesehen: „Scalpel Sniper Rifle Magazine
    (12 Schuss)" im Log vs. „… (12 cap)" beim Launcher)."""
    return re.sub(r'\s*\([^()]*\)\s*$', '', _norm(name)).strip()



# ------------------------------------------------ Fensterposition merken/laden
def load_geometry():
    """Die gemerkte Fensterlage — oder `None`, wenn es noch keine gibt.

    Bewusst `None` statt der Standardgröße: Nur so unterscheidet der Aufrufer
    „der Nutzer hat sein Fenster irgendwohin gestellt" von „erster Start", und
    nur beim ersten Start soll das Fenster mittig gesetzt werden.
    """
    try:
        return json.load(open(SETTINGS_FILE, encoding='utf-8')).get('geometry') or None
    except Exception:
        return None


GEOM_RE = re.compile(r'^(\d+)x(\d+)(?:\+(-?\d+)\+(-?\d+))?$')


def geometrie_pruefen(geom, root):
    """Liegt die gemerkte Fensterlage auf diesem Rechner überhaupt im Bild?

    Der Watcher speichert seine Lage, damit er beim nächsten Mal wieder dort
    steht — beim Autor auf dem oberen von drei Monitoren, also bei X≈3656 und
    negativem Y. Auf einem Rechner mit einem einzigen Bildschirm zeigt dieselbe
    Angabe ins Nichts: Das Fenster ist unsichtbar, unter macOS reißt Tk sogar
    das ganze Programm mit. Sobald die Version öffentlich wird, landet sie auf
    genau solchen Rechnern.

    Geprüft wird **großzügig**: Mehrere Monitore sollen weiter funktionieren
    (Tk kennt oft nur den Hauptbildschirm), es geht nur darum, offensichtlichen
    Unsinn abzufangen. Passt die Lage nicht, bleibt die Größe erhalten und nur
    die Position fällt weg — Tk platziert das Fenster dann selbst."""
    m = GEOM_RE.match(geom or '')
    if not m:
        return DEFAULT_GEOM
    breite, hoehe, x, y = m.groups()
    if x is None:
        return geom
    # macOS ist kein Zielsystem (Star Citizen gibt es dort nicht), aber am Mac
    # wird geplant und entwickelt. Tk rechnet dort negative Fensterkoordinaten
    # in einen Unsinnswert um und reißt das Programm mit — deshalb zählt die
    # gemerkte Position dort nicht.
    if sys.platform == 'darwin' and (int(x) < 0 or int(y) < 0):
        return '%sx%s' % (breite, hoehe)
    try:
        sb = max(root.winfo_screenwidth(), root.winfo_vrootwidth())
        sh = max(root.winfo_screenheight(), root.winfo_vrootheight())
    except Exception:
        return geom
    # Bis zum Zweifachen der Bildschirmgröße nach jeder Seite gilt als plausibel:
    # Das deckt übliche Mehrschirm-Aufbauten ab, ohne Fantasiewerte durchzulassen.
    if -2 * sb <= int(x) <= 3 * sb and -2 * sh <= int(y) <= 3 * sh:
        return geom
    return '%sx%s' % (breite, hoehe)


def standardlage(root):
    """Die Lage, mit der jeder anfängt: mittig auf dem **Hauptbildschirm**.

    Dieselbe Lage stellt auch der Knopf „Fensterlage zurücksetzen" wieder her.
    Wie viele Bildschirme jemand hat, weiß niemand vorher — die Mitte des
    Hauptbildschirms ist die einzige Stelle, die überall sinnvoll ist.
    """
    m = GEOM_RE.match(DEFAULT_GEOM or '')
    breite, hoehe = (int(m.group(1)), int(m.group(2))) if m else (440, 1000)
    return bildschirm.mittig(root, breite, hoehe)


def startlage(root):
    """Wohin das Overlay beim Start gehört.

    Gemerkte Lage, wenn es eine gibt und sie auf diesem Rechner plausibel ist;
    sonst die Standardlage. `geometrie_pruefen` gibt bei einer unglaubwürdigen
    Lage nur noch die Größe zurück — auch dann wird mittig gesetzt, statt Tk
    raten zu lassen.
    """
    gemerkt = load_geometry()
    if not gemerkt:
        return standardlage(root)
    geprueft = geometrie_pruefen(gemerkt, root)
    if '+' not in geprueft:
        return standardlage(root)
    return geprueft


def save_geometry(geom):
    try:
        os.makedirs(os.path.dirname(SETTINGS_FILE), exist_ok=True)
        json.dump({'geometry': geom}, open(SETTINGS_FILE, 'w', encoding='utf-8'))
    except Exception:
        pass


# ------------------------------------------------ Mit dem Rechner starten
# Steckt seit v1.6 in `scbp/autostart.py`: unter Windows ein Registry-Wert,
# unter Linux eine `.desktop`-Datei in ~/.config/autostart/.
# ⚠ Keine Konstante mehr: Ein Text, der beim Programmstart **einmal**
# festgelegt wird, kann nicht mehr auf einen Sprachwechsel reagieren. Der
# Titel wird bei jedem Gebrauch frisch geholt (`_autostart_titel`).


# ------------------------------------------------------------------ Signalton
def signalton(auffaellig=False):
    """Kurzer Ton bei einem Fund.

    Unter Windows `winsound`, unter Linux ein Systemklang über `scbp/ton.py`.

    Bis v2.0.0-rc3 stand hier für Linux nur `bell()` mit der Begründung
    „bleibt es still, ist das kein Fehler". Beim ersten echten Bauplan blieb
    es still, und das **war** ein Fehler: `bell()` ist die X11-Systemglocke,
    die auf modernen Arbeitsplätzen praktisch überall aus ist. `bell()` bleibt
    als letzter Rückfall — schaden kann es nicht."""
    if not TON_AN:
        return
    if winsound:
        try:
            winsound.MessageBeep(winsound.MB_ICONEXCLAMATION if auffaellig
                                 else winsound.MB_ICONASTERISK)
        except Exception:
            pass
        return
    if ton.abspielen('auffaellig' if auffaellig else 'normal'):
        return
    try:
        _WURZEL[0].bell()
    except Exception:
        pass


# Das Hauptfenster, damit `signalton()` es erreicht, ohne es durchreichen zu müssen.
_WURZEL = [None]



# ---------------------------------------------------------------- Watcher-Thread
class Watcher(threading.Thread):
    def __init__(self, out_queue):
        super().__init__(daemon=True)
        self.q = out_queue
        self.known = None       # BP-Namen aus der Launcher-Datei (None = kein Launcher)
        self.seen = set()       # schon angezeigte Namen (normalisiert) — gegen Dubletten
        self.stand = logquelle.Lesestand()
        self.tail = logquelle.LogTail(self.stand)
        # Zweites Muster: angenommene Auftraege (ab v3.2.0). Faellt der Katalog
        # aus, meldet `auftraege` einfach nichts — der Bauplan-Weg bleibt heil.
        try:
            self.tail.auftrag_muster = auftraege.muster()
            self.tail.auftrag_ende_muster = auftraege.ende_muster()
        except Exception as ausnahme:
            fehler.merken('watcher.auftrag_muster', ausnahme)
        self._auftraege_gesehen = set()   # je Programmlauf, gegen Doppelmeldungen
        # Was gerade laeuft — Titel (ohne unsere Marken) → fertige Zeile.
        # ⚠ Ein Zustand, keine Verlaufsliste: Beim Abschluss muss der Auftrag
        # **verschwinden**. Sonst stehen nach zehn Auftraegen am Abend zehn
        # Zeilen da, von denen neun erledigt sind.
        self._offene_auftraege = {}
        # MissionId -> Auftragsschluessel. ⚠⚠ **Die einzige Bruecke, wenn der
        # Endtitel nicht zum Annahmetitel passt** — gemessen 62 von 362 Enden.
        # Star Citizen fuehrt beide Meldungen mit derselben MissionId, auch
        # ueber einen Neustart des Werkzeugs hinweg.
        self._auftrag_missionen = {}
        # Und was zu diesen Auftraegen gerade ansteht. @@ **Der Auftrag sagt,
        # ob Bauplaene drin sind — das Ziel sagt, wofuer man gerade fliegt.**
        # Beides steht im Protokoll; die Buchfuehrung dazu in `auftraege.Ziele`.
        self._ziele = auftraege.Ziele()
        # ⚠ Messpunkte im Startverlauf. Zwischen „Overlay wird gebaut" und
        # „Overlay steht" lagen bei einem Nutzer **vier Sekunden**, bei einem
        # anderen eine — und dazwischen stand nichts, woran man das haette
        # festmachen koennen. Ein Bericht, der nur Anfang und Ende kennt, sagt
        # bei genau der Frage nichts, fuer die man ihn braucht.
        fehler.spur('Overlay: Bestand wird geladen')
        self.bestand = bestand_datei.laden()   # der eigene, dauerhafte Bestand
        fehler.spur('Overlay: Bestand geladen (%d Bauplaene)'
                    % len(self.bestand.get('bauplaene') or {}))
        # ⚠ Einmal beim Start die Namen an den Katalog angleichen. Was der
        # Watcher vor v3.3.3 aus dem Log gelesen hat, trägt womöglich die
        # Angaben aus dem Spiel im Namen („Balandin (S3 B Military)") und galt
        # dadurch als unbekannt — siehe `bestand.katalogname`. Ohne diesen
        # Durchlauf bliebe der alte Stand für immer schief.
        try:
            berichtigt = bestand_datei.angleichen(self.bestand)
            if berichtigt:
                self._bestand_sichern()
                fehler.spur('Bestand: %d Namen an den Katalog angeglichen'
                            % berichtigt)
        except Exception as ausnahme:
            fehler.merken('watcher.bestand_angleichen', ausnahme)
        fehler.spur('Overlay: Bestand am Katalog geprueft')
        self._neu_einlesen = False            # Auftrag von außen, siehe unten
        self.running = True
        self.cat_next = 0.0     # nächster Katalog-Check (Zeitstempel)
        self.cat_mtime = None   # letzter gesehener Änderungszeitpunkt der Katalogdatei
        self.scmdb_next = 0.0   # nächster Blick auf die scmdb-Craftdaten
        self.kat_next = 0.0     # nächster Blick auf den Bauplan-Katalog
        self.kat_laeuft = False  # holt gerade ein Nebenthread den Katalog?
        self.texte_next = 0.0   # nächster Blick auf Übersetzung und Injektion
        self.bestand_next = 0.0  # nächster Blick auf den EIGENEN Bestand
        self.texte_laeuft = False

    # ---- scmdb-Craftdaten frisch halten (ab v1.5.0) ----
    def _scmdb_tick(self):
        """Sieht selten nach, ob eine neue Spielversion vorliegt, und lädt dann die
        Werte-Datei neu. Läuft im Hintergrund-Thread, damit die Oberfläche nicht
        hängt, und schluckt jeden Fehler — ohne Netz bleibt der letzte Stand."""
        global SCMDB, SCMDB_VERSION
        if time.time() < self.scmdb_next:
            return
        self.scmdb_next = time.time() + SCMDB_POLL_SEC
        if scmdb_aktualisieren():
            SCMDB, SCMDB_VERSION = load_scmdb()
            self.q.put(('status', sprache.Satz('craftdaten_neu', SCMDB_VERSION,
                                            len(SCMDB))))

    def _preise_tick(self):
        """Holt die Rohstoffpreise, wenn die Ablage aelter als ein Tag ist.

        ⚠ Laeuft im Hintergrund-Thread und schluckt jeden Fehler. Ohne Netz
        bleibt der letzte Stand; liegt gar keiner vor, entfaellt die
        Preisangabe still — die Herstellung funktioniert ohne sie genauso wie
        vorher. Es gibt keine Meldung darueber, weil es keine braucht.
        """
        # ⚠⚠ **Der Spielstand zuerst, und das ist kein Zufall.** Jede Ablage
        # wird mit dem Stand gestempelt, der beim Sichern bekannt ist. Käme er
        # nach den Preisen, trüge eine frisch geholte Ablage den Stand von
        # **vor** dem Patch — und wäre damit genau falsch gekennzeichnet.
        try:
            spielstand.aktualisieren()
        except Exception as ausnahme:
            fehler.merken('watcher.spielstand', ausnahme)
        try:
            preise.aktualisieren()
        except Exception as ausnahme:
            fehler.merken('watcher.preise', ausnahme)
        # Die Lagerorte dazu — hoechstens einmal pro Woche, siehe `orte.py`.
        try:
            orte.aktualisieren()
        except Exception as ausnahme:
            fehler.merken('watcher.orte', ausnahme)
        # Und die Ankaufpreise je Terminal fuer den Verkaufs-Reiter — ebenfalls
        # hoechstens einmal am Tag. ⚠ Bewusst **hier** und nicht beim Oeffnen
        # der Seite: Wer den Reiter aufmacht, soll Daten vorfinden statt auf
        # einen Abruf zu warten.
        try:
            verkauf.aktualisieren()
        except Exception as ausnahme:
            fehler.merken('watcher.verkauf', ausnahme)
        # Und die Schiffsliste — höchstens einmal pro Woche, siehe
        # `scbp/schiffe.py`. Sie liefert den Frachtraum für den Routen-Reiter.
        try:
            schiffe.aktualisieren()
        except Exception as ausnahme:
            fehler.merken('watcher.schiffe', ausnahme)
        # ⭐⭐ **Und der Warengruppen-Katalog für den Laden-Reiter — zuletzt.**
        #
        # Er ist der teuerste der Abrufe (76 Stück, gemessen rund 50 s) und
        # stand deshalb lange nur beim Öffnen der Seite. Genau das war das
        # Problem: Wer den Reiter aufmacht, wartet dann eine Minute vor einer
        # leeren Liste. Am 05.09.2026: „Bei Läden ist die lange Ladezeit echt
        # störend."
        #
        # Hier läuft er im Hintergrund-Thread, während der Spieler etwas
        # anderes tut — und dank der Patch-Bindung höchstens einmal je
        # Spielversion. Er steht **hinter** den anderen, damit die schnellen
        # Abrufe nicht auf ihn warten.
        try:
            laeden.katalog_holen()
        except Exception as ausnahme:
            fehler.merken('watcher.laeden_katalog', ausnahme)

    # ---- Bauplan-Katalog holen und frisch halten ----
    def _katalog_tick(self):
        """Holt den Bauplan-Katalog von scmdb, wenn er fehlt oder veraltet ist.

        Bis v2.0.0-rc1 wurde `katalog.aktualisieren()` von **nirgendwo** aufgerufen:
        Der Katalog kam nie an, das Bauplan-Fenster blieb bei jedem Nutzer leer und
        der Hinweistext versprach etwas, das nicht geschah.

        Der Abruf läuft in einem **eigenen** Thread, nicht hier im Watcher-Takt:
        Es sind rund 12 MB, und die Log-Erkennung ist die Kernaufgabe — sie darf
        dafür keine Sekunde stehenbleiben. `kat_laeuft` verhindert, dass bei einer
        langsamen Leitung mehrere Abrufe übereinander laufen."""
        if SCMDB_AUS or self.kat_laeuft or time.time() < self.kat_next:
            return
        self.kat_next = time.time() + SCMDB_POLL_SEC
        self.kat_laeuft = True

        def holen():
            try:
                gab_es_schon = bool(katalog_modul.laden()['bauplaene'])
                if not gab_es_schon:
                    self.q.put(('status', sprache.Satz('katalog_holt')))
                neu, anzahl, version = katalog_modul.aktualisieren()
                if neu:
                    self.q.put(('status', sprache.Satz('katalog_geholt', anzahl, version)))
                else:
                    # Nichts zu tun heißt: schon aktuell — oder kein Netz. Im
                    # zweiten Fall bald noch einmal versuchen statt sechs Stunden
                    # warten, sonst bleibt ein kurzer Aussetzer den ganzen Tag hängen.
                    if not gab_es_schon and not katalog_modul.laden()['bauplaene']:
                        self.kat_next = time.time() + 300
            finally:
                self.kat_laeuft = False

        threading.Thread(target=holen, daemon=True).start()

    # ---- Bauplan-Angaben im Spiel frisch halten ----
    def _texte_tick(self):
        """Übersetzung, Vertragsdaten und Injektion nachziehen — von selbst.

        **Warum das nicht optional sein kann:** Jedes Übersetzungs-Update und
        jeder Spiel-Patch schreibt die `global.ini` neu; die eingetragenen
        Bauplan-Angaben sind dann **weg**, ohne dass irgendetwas darauf
        hinweist. Und nach einem Patch geben Missionen andere Baupläne aus —
        wer dann noch die alten Angaben liest, plant mit falschen Daten.
        Beides fällt niemandem auf, weil das Spiel ja normal weiterläuft.

        Angefasst wird nur, was der Spieler selbst eingerichtet hat: Ohne
        vermerkte Quelle passiert hier gar nichts.

        Läuft im **eigenen** Thread — es sind mehrere Megabyte, und die
        Log-Erkennung darf dafür nicht stehenbleiben."""
        if SCMDB_AUS or self.texte_laeuft:
            return
        # ⚠ Zwei Schalter, und beide müssen hier gelten:
        #   `inj_an`   — schreibt das Werkzeug überhaupt in die Auftragstexte?
        #                Aus lassen will, wer gerade auf PTU spielt oder seine
        #                Textdatei in Ruhe haben möchte.
        #   `inj_auto` — hält es sich von selbst aktuell?
        # Der erste fehlte ganz: Ausschalten ging nur über „Wieder entfernen",
        # und beim nächsten Start schrieb das Werkzeug wieder hinein.
        if not pfade.einstellung_wahrheit('inj_an', True):
            self.texte_next = time.time() + TEXTE_POLL_SEC
            return
        if not pfade.einstellung_wahrheit('inj_auto', True):
            self.texte_next = time.time() + TEXTE_POLL_SEC
            return

        # ⚠ Zwei Anlässe mit sehr verschiedenen Takten — deshalb getrennt:
        #
        #   `faellig`     alle sechs Stunden. Fragt bei FREMDEN Quellen nach
        #                 (neue Übersetzung, neue Vertragsdaten) — Netz, teuer.
        #   `bestand_neu` alle 30 Sekunden. Fragt nur die eigene Bestandsdatei,
        #                 kostet 0,4 ms und braucht kein Netz.
        #
        # Sie hingen bis zum 05.09.2026 zusammen, und damit hing der eigene
        # Fund am Takt der fremden Quellen. Falsch herum: Was der Spieler
        # gerade selbst tut, ist das Schnellste im Spiel, nicht das Langsamste.
        jetzt = time.time()
        faellig = jetzt >= self.texte_next
        bestand_neu = False
        if not faellig and jetzt >= self.bestand_next:
            self.bestand_next = jetzt + BESTAND_POLL_SEC
            bestand_neu = self._bestandsmarke_neu()
        if not faellig and not bestand_neu:
            return

        quelle = next((q for q in uebersetzung.QUELLEN
                       if uebersetzung.installiert(q)), None)
        eigene_texte = bool(uebersetzung.installiert('original'))
        if not quelle and not eigene_texte:
            return                      # nie eingerichtet — Finger weg
        # ⚠ Nur der Sechs-Stunden-Lauf schiebt seinen eigenen Termin. Täte das
        # auch der Bestands-Lauf, verschöbe jeder gefundene Bauplan die
        # Netzabfrage um weitere sechs Stunden — wer viel spielt, bekäme die
        # neue Übersetzung nie.
        if faellig:
            self.texte_next = jetzt + TEXTE_POLL_SEC
        self.texte_laeuft = True

        def arbeit():
            try:
                self._texte_abgleichen(quelle, nur_bestand=not faellig)
            finally:
                self.texte_laeuft = False

        threading.Thread(target=arbeit, daemon=True).start()

    def _bestandsmarke_neu(self):
        """Hat sich der eigene Bestand seit dem letzten Einspielen geändert?

        Bewusst still bei einem Fehler: Diese Frage wird alle 30 Sekunden
        gestellt: Eine kaputte Bestandsdatei würde das Fehlerprotokoll sonst in
        einer halben Stunde mit 60 gleichen Einträgen füllen und die 50
        aufgehobenen Plätze verdrängen — genau die, die eine Meldung brauchbar
        machen. Gemeldet wird sie im Sechs-Stunden-Lauf, dort stört sie nicht.
        """
        try:
            return injektion.bestand_marke() != pfade.einstellung('inj_bestand')
        except Exception:
            return False

    def _texte_abgleichen(self, quelle, nur_bestand=False):
        """Der eigentliche Abgleich. Meldet nur, wenn sich etwas geändert hat.

        `nur_bestand` überspringt die drei Prüfungen, die nach FREMDEN
        Änderungen sehen — zwei davon gehen ins Netz, die dritte liest die
        mehrere Megabyte große `global.ini`. Für einen frisch gefundenen
        Bauplan ist keine davon nötig: Da steht schon fest, was zu tun ist.
        """
        sprache_ordner = (uebersetzung.QUELLEN[quelle]['sprache'] if quelle
                          else 'english')
        ziel = uebersetzung.ziel_ini(sprache_ordner)
        if not ziel:
            return
        kuerzel = injektion._sprachkuerzel(sprache_ordner)
        neu_noetig = False

        # 1. Neue Version der Übersetzung? Die schreibt die Datei komplett neu,
        #    danach ist die Injektion in jedem Fall weg.
        if quelle and not nur_bestand:
            da, kennung = uebersetzung.update_da(quelle)
            if da:
                ok, meldung = uebersetzung.holen(quelle)
                if ok:
                    self.q.put(('status', sprache.Satz('texte_erneuert', kennung)))
                    neu_noetig = True

        # 2. Neue Vertragsdaten? Nach einem Patch geben Missionen anderes aus.
        if not nur_bestand:
            da, kennung = injektion.scdl_update_da(kuerzel)
            if da:
                self.q.put(('status',
                            sprache.Satz('bpdaten_erneuert', kennung)))
                neu_noetig = True

        # 3. Ist die Auszeichnung überhaupt noch drin? Ein Spiel-Patch ersetzt
        #    die Datei, ohne dass jemand etwas davon merkt.
        if not neu_noetig and not nur_bestand and not injektion.ist_drin(ziel):
            neu_noetig = True

        # 4. ⚠⚠ **Hat sich der eigene Bestand geändert?** Bis zum 04.09.2026
        #    fehlte genau diese Bedingung — und damit hörten die Kästchen im
        #    Spiel still auf zu stimmen, sobald ein Bauplan dazukam.
        #
        #    Die Punkte 1 bis 3 fragen alle nach FREMDEN Änderungen. Die
        #    Kästchen sind aber unsere eigene Zutat und hängen am Bestand des
        #    Spielers: Wer einen Bauplan freischaltet, sieht ihn im
        #    Auftragstext trotzdem weiter als fehlend — bis zufällig eine
        #    Übersetzung oder ein Patch die Datei anfasst. Gemeldet mit zwei
        #    Bauplänen, die seit dem 25.08. im Bestand lagen und im Spiel
        #    ungehakt blieben: „ich dachte wir machen die Kästen?"
        #
        #    ⚠ Verglichen wird ein Fingerabdruck, nicht die Anzahl. Der
        #    Bestandsabgleich beim Start benennt Einträge um (`angleichen`);
        #    dabei bleibt die Zahl gleich, die Namen ändern sich — und genau
        #    die stehen in den Kästchen.
        #
        #    ⚠⚠ Am 05.09.2026 nachgebessert: Die Bedingung war da, hing aber im
        #    Sechs-Stunden-Takt der Punkte 1 bis 3 — also im Takt der fremden
        #    Quellen. Wer 20 Minuten spielte und aufhörte, kam nie hierher.
        #    Der Auslöser sitzt jetzt in `_bestandsmarke_neu` und fragt alle 30
        #    Sekunden; diese Stelle hier bleibt trotzdem die maßgebliche —
        #    zwischen Auslösen und Schreiben liegt ein Thread-Wechsel, in dem
        #    sich der Bestand erneut ändern kann.
        marke = None
        if not neu_noetig:
            try:
                marke = injektion.bestand_marke()
                if marke != pfade.einstellung('inj_bestand'):
                    neu_noetig = True
            except Exception as ausnahme:
                fehler.merken('watcher.inj_bestandsmarke', ausnahme)

        if neu_noetig and os.path.isfile(ziel):
            ok, anzahl, _meldung = injektion.einrichten(ziel, sprache_ordner)
            if ok:
                self.q.put(('status', sprache.Satz('inj_aktiv', anzahl)))
                # Erst nach dem Schreiben merken: Scheitert das Einrichten,
                # soll es beim nächsten Durchlauf erneut versucht werden.
                try:
                    pfade.einstellung_setzen(
                        'inj_bestand', marke or injektion.bestand_marke())
                except Exception as ausnahme:
                    fehler.merken('watcher.inj_marke_merken', ausnahme)

    # ---- Katalog-Wache: was ist NEU craftbar im Spiel? ----
    def _catalog_tick(self):
        """Prüft, ob der Craftbar-Katalog gewachsen ist. Der Vergleichsstand überlebt
        Neustarts (CAT_SEEN), sonst käme nach jedem Programmstart alles doppelt."""
        try:
            marke = os.path.getmtime(TYPE_FILE)
        except OSError:
            # Kein Launcher: Dann ist die Spielversion der scmdb-Daten die Marke.
            # Sie ändert sich genau dann, wenn ein Patch neue Baupläne bringt —
            # also genau dann, wenn nachgesehen werden muss.
            marke = SCMDB_VERSION or None
            if marke is None:
                return
        if marke == self.cat_mtime:
            return
        self.cat_mtime = marke
        jetzt = load_types()
        if not jetzt:
            return
        try:
            with open(CAT_SEEN, encoding='utf-8') as f:
                bekannt = set(json.load(f).get('namen', []))
        except Exception:
            bekannt = set()
        if not bekannt:                       # erster Lauf: nur Basis setzen, nichts melden
            self._save_catalog(jetzt)
            return
        neu = sorted(n for n in jetzt if n not in bekannt)
        if not neu:
            self._save_catalog(jetzt)
            return
        anzeige = load_display()
        for name in neu:
            titel = merkliste.treffer(name)
            self.q.put(('catalog', _katalogname(name, anzeige),
                        jetzt.get(name) or '—', time.strftime('%H:%M:%S'), titel))
        self._save_catalog(jetzt)

    @staticmethod
    def _save_catalog(jetzt):
        try:
            os.makedirs(APP_DIR, exist_ok=True)
            with open(CAT_SEEN, 'w', encoding='utf-8') as f:
                json.dump({'stand': time.strftime('%Y-%m-%d %H:%M:%S'),
                           'namen': sorted(jetzt)}, f, ensure_ascii=False, indent=1)
        except Exception:
            pass

    # Bis zu wie vielen nachgelesenen Bauplänen einzeln gemeldet wird.
    #
    # ⚠ **Warum es überhaupt eine Grenze gibt.** Die Nachlese war bis v3.0.3
    # vollständig still — mit gutem Grund: Beim allerersten Start geht sie über
    # **alle** aufgehobenen Sitzungen (auf einem gewachsenen Rechner sind das
    # über hundert), und niemand will danach hunderte Zeilen wegklicken.
    #
    # Nur trifft dieser Fall genau **einmal** zu. Im Alltag findet sie null bis
    # drei — und die will man sehen. Am 28.08.2026 fiel auf, nachdem ein Bauplan
    # still im Bestand gelandet war: „sonst geht das still in den Bestand wie bei
    # mir heute, und niemand sieht es."
    #
    # Also beides: bis zu dieser Zahl einzeln melden, darüber nur die Summe in
    # der Statuszeile wie bisher.
    NACHLESE_MELDEN_BIS = 10

    def _nachgelesenes_melden(self, namen):
        """Nachgelesene Baupläne in die Liste stellen — wenn es wenige sind."""
        if not namen or len(namen) > self.NACHLESE_MELDEN_BIS:
            return
        for name in namen:
            self.q.put(('new', name, art_of(name), meta_of(name) or '',
                        time.strftime('%H:%M:%S'), True))

    def _auftragsstand(self):
        """Was die Anzeige braucht: `(Schluessel, Zeile, Zwischenziele)`.

        ⚠⚠ **Die eine Stelle, die den Auftragsstand nach aussen gibt.** Vorher
        stand `list(self._offene_auftraege.items())` an vier Stellen im Code;
        eine davon zu vergessen hiesse, dass die Leiste je nach Anlass etwas
        anderes zeigt.
        """
        zu_mission = {}
        for kennung, rein in self._auftrag_missionen.items():
            zu_mission.setdefault(rein, kennung)
        return [(rein, zeile, self._ziele.offen(zu_mission.get(rein)))
                for rein, zeile in self._offene_auftraege.items()]

    def auftrag_wegklicken(self, rein):
        """Einen Auftrag von Hand aus der Anzeige nehmen.

        ⚠ Es gibt Faelle, die kein Log meldet: Ein Auftrag geht durch einen
        Fehler im Spiel verloren, oder man muss ausloggen, um einen Fehler
        loszuwerden — dann ist der Auftrag weg, ohne dass „abgeschlossen" oder
        „zurückgezogen" im Log stuende. Der Watcher kann das nicht wissen, der
        Spieler schon. Also darf er es sagen.

        Der Titel bleibt in `_auftraege_gesehen`, damit er nicht beim naechsten
        Log-Abschnitt wieder auftaucht.
        """
        if self._offene_auftraege.pop(rein, None) is not None:
            self.q.put(('auftraege', self._auftragsstand()))
        # Auch dann melden, wenn er in der Leiste schon weg war: Die Zeile in
        # der Liste kann trotzdem noch stehen, und genau die will man los.
        self.q.put(('auftrag_weg', rein))

    def _auftrag_zeile(self, titel, rein):
        """Die fertige Zeile zu einem Auftrag — mit Bauplan-Angabe, wenn möglich.

        ⚠ Kennt der Katalog den Auftrag nicht, bleibt es beim blossen Titel.
        Der ist keine Zusage, sondern nur der Name, den das Spiel selbst
        gemeldet hat. Eine erfundene Bauplan-Angabe waere schlimmer als keine.
        """
        try:
            ergebnis = auftraege.pruefen(
                titel, lambda n: bestand_datei.norm(n) in self.bestand['bauplaene'])
        except Exception as ausnahme:
            fehler.merken('watcher.auftraege', ausnahme)
            return None
        if ergebnis is None:
            return None
        gesamt, fehlend = ergebnis
        if not fehlend:
            zusatz = sprache.Satz('auftrag_komplett', gesamt)
        elif len(fehlend) == 1:
            zusatz = sprache.Satz('auftrag_fehlt', gesamt, fehlend[0])
        else:
            zusatz = sprache.Satz('auftrag_fehlt_mehr', gesamt, len(fehlend),
                                  ', '.join(fehlend[:2]))
        return '%s  →  %s' % (sprache.Satz('auftrag_zeile', rein), zusatz)

    def _auftraege_beim_start(self):
        """Was laut laufendem `Game.log` gerade offen ist.

        Das Spiel meldet nicht nur die Annahme, sondern auch jedes Ende —
        abgeschlossen, zurückgezogen, fehlgeschlagen. Wer das Log einmal ganz
        durchgeht und Buch führt, weiss beim Start, was noch laeuft. Vorher war
        ein Auftrag nach einem Neustart des Watchers einfach weg: Er stand nur
        als Verlaufszeile da, nicht als Zustand.

        ⚠ **Nur die laufende Log-Datei.** Startet man das Spiel neu, beginnt
        eine frische; was in den Sicherungen davor steht, kann laengst erledigt
        sein. Lieber nichts zeigen als etwas Falsches behaupten.
        """
        if not getattr(self.tail, 'auftrag_muster', None):
            return
        try:
            pfad = pfade.game_log()
            if not pfad or not os.path.isfile(pfad):
                return
            with open(pfad, 'rb') as f:
                text = f.read().decode('utf-8', 'ignore')
        except OSError as ausnahme:
            fehler.merken('watcher.auftraege_start', ausnahme)
            return

        try:
            offen, missionen = auftraege.stand_aus_text(
                text, self.tail.auftrag_muster, self.tail.auftrag_ende_muster)
        except Exception as ausnahme:
            fehler.merken('watcher.auftraege_start', ausnahme)
            return
        # ⚠ Die Kennungen mitnehmen, nicht nur die Titel: Endet einer dieser
        # Auftraege spaeter im laufenden Betrieb, ist die MissionId oft das
        # Einzige, was die Endmeldung mit ihm verbindet.
        self._auftrag_missionen.update(missionen)
        # ⚠ Die Ziele aus demselben Text. Ohne das stuende beim Start zwar der
        # Auftrag da, aber ohne das, was gerade zu tun ist — und genau danach
        # schaut man nach einem Neustart zuerst.
        try:
            self._ziele.aufnehmen(auftraege.ziel_ereignisse_aus_text(text))
        except Exception as ausnahme:
            fehler.merken('watcher.ziele_start', ausnahme)

        for titel in offen:
            rein = auftraege.sauber(titel)
            if not rein:
                continue
            # Beim Start nicht in die Verlaufsliste melden — das waere ein
            # Schwall alter Nachrichten. Nur der Stand wird gesetzt.
            self._auftraege_gesehen.add(rein)
            self._offene_auftraege[rein] = (self._auftrag_zeile(titel, rein)
                                            or sprache.Satz('auftrag_zeile', rein))
        if self._offene_auftraege:
            self.q.put(('auftraege', self._auftragsstand()))

    def _auftraege_melden(self):
        """Zu jedem angenommenen Auftrag sagen, ob Bauplaene dabei sind.

        Die eine Frage des Werkzeugs, nur frueher beantwortet: nicht erst wenn
        der Bauplan kommt, sondern schon beim Annehmen.

        ⚠ Kennt der Katalog den Auftrag nicht, wird **geschwiegen**. Eine
        falsche Bauplan-Zusage waere schlimmer als gar keine Meldung — und der
        Katalog kennt 353 von deutlich mehr Auftraegen im Spiel.
        """
        # ⚠⚠ **Zuerst die Ziele — vor dem Ausstieg gleich darunter.** Ein
        # Zwischenziel wechselt staendig, ohne dass sich die Auftragsliste
        # ruehrt. Haengt man das hinter das `return`, steht in der Leiste bis
        # zum naechsten angenommenen Auftrag das Ziel von vor zwanzig Minuten.
        ziele_neu = False
        try:
            ziele_neu = self._ziele.aufnehmen(
                getattr(self.tail, 'ziel_ereignisse', None))
        except Exception as ausnahme:
            fehler.merken('watcher.ziele', ausnahme)
        self.tail.ziel_ereignisse = []

        ereignisse = getattr(self.tail, 'auftrag_ereignisse', None) or []
        if not ereignisse:
            if ziele_neu and self._offene_auftraege:
                self.q.put(('auftraege', self._auftragsstand()))
            return
        self.tail.auftraege = []
        self.tail.auftraege_beendet = []
        self.tail.auftrag_ereignisse = []
        veraendert = False

        # ⚠ **In der Reihenfolge des Logs durchgehen, nicht erst alle Enden.**
        # Bis v3.3.0-rc29 stand hier „zuerst wegnehmen, dann hinzufuegen" —
        # gedacht fuer den Fall, dass jemand einen Auftrag abbricht und sofort
        # neu annimmt. Der umgekehrte Fall kam dabei unter die Raeder: Enthaelt
        # ein Abschnitt erst die Annahme und danach den Abschluss, wurde der
        # Abschluss zuerst verrechnet (und traf ins Leere) und der Auftrag
        # danach als frisch angenommen hingestellt.
        #
        # Genau das passiert nach jedem Neustart des Watchers waehrend einer
        # laufenden Spielsitzung, denn dann liest der erste Abschnitt alles
        # nach, was seit dem letzten Lauf geschah. Am 30.08.2026 gemessen:
        # „Retake Platforms From Nine Tails" um 01:18 angenommen, um 01:59
        # abgeschlossen, Watcher um 02:22 gestartet — und die Zeile stand als
        # laufender Auftrag da.
        #
        # Die Reihenfolge klaert beide Faelle: Was am Ende des Abschnitts offen
        # ist, wird gezeigt; was zuletzt ein Ende hatte, nicht.
        offen_jetzt = {}
        for ist_annahme, titel, mission_id, objective_id in ereignisse:
            # ⚠⚠ **Ausloggen raeumt alles** — und zwar bevor irgendein Titel
            # geprueft wird, denn dieses Ereignis hat keinen. Das Spiel meldet
            # beim Verlassen der Spielwelt kein einziges Auftrags-Ende, im
            # Auftragsbuch ist danach trotzdem alles weg. Begruendung und
            # Messung stehen bei `auftraege.VERLASSEN`.
            if ist_annahme is None:
                offen_jetzt.clear()
                for weg in list(self._offene_auftraege):
                    del self._offene_auftraege[weg]
                    self.q.put(('auftrag_weg', weg))
                    veraendert = True
                    # Damit derselbe Auftrag nach dem naechsten Einloggen
                    # wieder gemeldet wird — man nimmt ihn ja erneut an.
                    self._auftraege_gesehen.discard(weg)
                for kennung in list(self._auftrag_missionen):
                    self._ziele.vergessen(kennung)
                    del self._auftrag_missionen[kennung]
                continue
            rein = auftraege.sauber(titel)
            if not rein:
                continue
            if ist_annahme:
                offen_jetzt[rein] = titel
                if mission_id:
                    self._auftrag_missionen[mission_id] = rein
                continue
            # ⚠⚠ **Nicht jedes Ende meint den Auftrag.** Traegt die Meldung
            # eine ObjectiveId, endet nur ein Zwischenziel — der Auftrag
            # laeuft weiter. Welcher Auftrag gemeint ist, entscheidet
            # `auftraege.beendet_welchen`; dort steht auch, woran das gemessen
            # ist.
            # ⚠ Gesucht wird in BEIDEN Buechern: was dieser Abschnitt neu
            # gebracht hat, und was laengst offen stand. Sonst faende ein
            # Ende seinen Auftrag nur, wenn beide im selben Abschnitt liegen.
            bekannt = dict(self._offene_auftraege)
            bekannt.update(offen_jetzt)
            weg = auftraege.beendet_welchen(rein, mission_id, objective_id,
                                            bekannt, self._auftrag_missionen)
            if weg is None:
                # Zwischenziel — oder ein Missions-Ende ohne auffindbaren
                # Auftrag. Letzteres kam ueber 153 Protokolle kein einziges
                # Mal vor; traete es doch ein, bleibt alles stehen. Geraten
                # wird nicht, und pauschal geraeumt schon gar nicht: Genau das
                # hat in v3.4.4 laufende Auftraege mitgerissen.
                continue
            offen_jetzt.pop(weg, None)
            if self._offene_auftraege.pop(weg, None) is not None:
                veraendert = True
            for kennung in [k for k, v in self._auftrag_missionen.items()
                            if v == weg]:
                self._ziele.vergessen(kennung)
                del self._auftrag_missionen[kennung]
            self.q.put(('auftrag_weg', weg))
            # Damit dieselbe Mission spaeter wieder gemeldet wird. Ohne das
            # bliebe ein wiederholter Auftrag stumm — und genau die macht man
            # im Spiel reihenweise.
            self._auftraege_gesehen.discard(weg)

        for rein, titel in offen_jetzt.items():
            if rein not in self._offene_auftraege:
                # Der blosse Titel ist noch keine Bauplan-Zusage — den darf
                # die Anzeige auch dann führen, wenn der Katalog die Mission
                # nicht kennt. Steht unten ein Ergebnis, wird er ersetzt.
                self._offene_auftraege[rein] = sprache.Satz('auftrag_zeile', rein)
                veraendert = True
            if rein in self._auftraege_gesehen:
                continue
            self._auftraege_gesehen.add(rein)
            zeile = self._auftrag_zeile(titel, rein)
            if zeile is None:
                continue
            self._offene_auftraege[rein] = zeile
            veraendert = True
            # ⚠⚠ **Erst die Leiste, dann der Hinweis.** Andersherum weiss die
            # Anzeige beim Hinweis noch nichts von dem Auftrag und setzt
            # denselben Satz ein zweites Mal darunter (gemeldet 31.08.2026).
            self.q.put(('auftraege', self._auftragsstand()))
            # ⚠ Mit dem Auftragsschluessel. Die Zeile in der Liste gehoert zu
            # genau diesem Auftrag — endet er, muss sie mitverschwinden, und
            # von Hand wegnehmen koennen muss man sie auch.
            self.q.put(('hinweis', zeile, rein))

        if veraendert or ziele_neu:
            # Die Anzeige bekommt den **ganzen** Stand, nicht die Änderung —
            # dann kann sie nicht auseinanderlaufen.
            self.q.put(('auftraege', self._auftragsstand()))

    def _bestand_sichern(self):
        """Bestand schreiben — und die Liste im Hauptfenster nachziehen lassen.

        ⚠⚠ **Warum das eine Methode ist und kein Signal an sieben Stellen.**
        Der Bestand wird an sieben Stellen geschrieben: Live-Fund, Nachlese,
        Launcher, Startabgleich, Angleichen. Jede davon muss die Liste
        auffrischen, und die achte, die jemand später hinzufügt, würde es
        vergessen — genau so entstehen die Fehler, bei denen „bei mir geht es"
        und bei einem anderen nicht. Wer speichert, meldet. Ohne Ausnahme.

        ⚠ **Gemeldet, NACHDEM geschrieben wurde.** Die Liste liest die Datei
        neu; meldete man vorher, könnte sie den Stand von davor erwischen. Das
        wäre ein Wettlauf, der einmal unter hundert Malen zuschlägt — und dann
        fehlt genau ein Bauplan, bis zufällig der nächste kommt.

        ⚠ **Ein Signal je Durchlauf, nicht je Bauplan.** Beim Nachlesen der
        Protokolle kommen Dutzende Funde auf einmal; die Liste einmal neu zu
        zeichnen reicht. Deshalb hängt es hier und nicht in `_emit`.
        """
        bestand_datei.speichern(self.bestand)
        schlange = getattr(self, 'q', None)
        if schlange is not None:
            schlange.put(('liste_frisch',))

    def _emit(self, key, log_meta=None):
        # log_meta = Kürzel aus dem Log-Zusatz; wird nur genommen, wenn der
        # Launcher-Katalog nichts hergibt (brandneues Item nach einem SC-Patch).
        self.q.put(('new', key, art_of(key), meta_of(key) or log_meta or '',
                    time.strftime('%H:%M:%S')))

    # ---- Sprache des Spiels erschließen ----
    def _sprache_erschliessen(self):
        """Herausfinden, wie die Bauplan-Meldung in DIESEM Client lautet.

        Nötig, weil die mitgelieferte Tabelle nur Deutsch sicher kennt; die
        englischen Formulierungen sind Kandidaten, und Französisch oder Spanisch
        stehen gar nicht drin. Der Katalog mit über 700 Bauplan-Namen macht es
        möglich: Wer in einer Logzeile einen bekannten Bauplan findet, kennt
        auch den Text davor.

        Läuft nur, solange die Formulierung nicht ohnehin feststeht — und nur
        einmal, denn danach steht sie in `phrasen.json`."""
        try:
            if phrasen.bestaetigt():
                return
            namen = [e['n'] for e in katalog_modul.laden()['bauplaene'].values()]
            if not namen:
                return
            gefunden = phrasen.selbst_finden(namen, pfade.log_sicherungen())
            if gefunden and phrasen.merken(gefunden):
                self.tail.muster = phrasen.muster()
                self.q.put(('hinweis', sprache.Satz('sprache_erkannt', gefunden)))
        except Exception:
            pass            # ohne Erkennung gilt die mitgelieferte Tabelle

    # ---- Nachlese: was wurde ohne laufenden Watcher freigeschaltet? ----
    def neu_einlesen_anstossen(self):
        """Von außen gerufen: beim nächsten Takt alles noch einmal durchsehen.

        Nur ein Merker — die Arbeit gehört in den eigenen Faden, sonst fassen
        zwei Stellen denselben Bestand an."""
        self._neu_einlesen = True

    def _alles_neu_einlesen(self):
        """Alle Protokolle noch einmal durchsehen, auch die schon bekannten.

        ⚠ Warum es das gibt: Der Lesestand kann weiter sein als der Bestand.
        Etwa wenn der Watcher zu war, während Star Citizen weiterlief — dann
        stehen die Baupläne dieser Sitzung in einer Datei, die er für erledigt
        hält. Oder wenn beim ersten Lauf die Spielsprache noch nicht erkannt war
        und die Protokolle mit der falschen Formulierung durchsucht wurden.

        Gemeldet wird immer, auch die Null: Wer einen Knopf drückt, will wissen,
        dass etwas passiert ist."""
        try:
            funde, bericht = logquelle.alles_neu(phrasen.muster())
        except Exception as ausnahme:
            fehler.merken('watcher.neu_einlesen', ausnahme)
            self.q.put(('bescheid', sprache.Satz('s_be_neu'),
                        sprache.Satz('neu_gelesen_fehler')))
            return
        dazu = []
        for name, _zusatz in funde:
            if bestand_datei.hinzufuegen(self.bestand, name, 'nachlese'):
                dazu.append(name)
        if dazu:
            self._bestand_sichern()
            self.seen = set(bestand_datei.schluessel(self.bestand))
        # ⚠ Als Bescheid, nicht nur als Zeile: Wer diesen Lauf anstoesst,
        # wartet auf genau diese Zahl.
        self.q.put(('bescheid', sprache.Satz('s_be_neu'),
                    sprache.Satz('neu_gelesen',
                                 bericht.get('dateien', 0), len(dazu))))
        # ⚠ Hier erst recht: Wer den Knopf drückt, will das Ergebnis sehen und
        # nicht nur eine Zahl in der Leiste.
        self._nachgelesenes_melden(dazu)
        # Und den Auftragsstand mitziehen. Wer neu einliest, will einen
        # sauberen Stand — auch dann, wenn er zwischendurch einen Auftrag von
        # Hand ausgeblendet hat. Deshalb erst leeren, dann neu aus dem Log
        # ermitteln: angenommen und danach kein Ende gesehen heisst offen.
        vorher = list(self._offene_auftraege)
        self._offene_auftraege = {}
        self._auftrag_missionen = {}
        self._ziele = auftraege.Ziele()
        self._auftraege_beim_start()
        # Und die Zeilen in der Liste dazu: Was jetzt nicht mehr offen ist,
        # darf auch nicht mehr als laufender Auftrag dastehen.
        for rein in vorher:
            if rein not in self._offene_auftraege:
                self.q.put(('auftrag_weg', rein))
        if not self._offene_auftraege:
            # Auch die Leere melden, sonst bleibt eine alte Liste stehen.
            self.q.put(('auftraege', []))

    def _nachlese(self):
        """Beim Start die aufgehobenen Logs durchsehen und in den Bestand nehmen.

        Bewusst **still**: Es geht um Vergangenes, das gehört nicht als Meldung
        in die Liste — sonst stünden nach jedem Start hunderte Zeilen da. Nur
        die Zahl kommt in die Statuszeile, und eine verbleibende Lücke wird
        deutlich gesagt, damit niemand seinen Bestand für vollständig hält."""
        # ⚠ Dasselbe fuer das Auftrags-Protokoll: Wer den Watcher zum ersten Mal
        # startet, soll es gefuellt vorfinden — seine `logbackups/` reichen ja
        # Wochen zurueck. Steht bewusst VOR der Bauplan-Nachlese und in einem
        # eigenen `try`: Geht hier etwas schief, darf der Bestand trotzdem
        # nachgelesen werden.
        try:
            from scbp import missionslog as _ml
            _gesamt, _dazu = _ml.nachlese()
            if _dazu:
                fehler.spur('Auftrags-Protokoll: %d neu, %d gesamt'
                            % (_dazu, _gesamt))
        except Exception as ausnahme:
            fehler.merken('watcher.auftragsprotokoll', ausnahme)

        try:
            funde, bericht = logquelle.nachlesen(self.stand)
        except Exception:
            return
        dazu = []
        for name, _zusatz in funde:
            if bestand_datei.hinzufuegen(self.bestand, name, 'nachlese'):
                dazu.append(name)
        if dazu:
            self._bestand_sichern()
            self.q.put(('status', sprache.Satz('nachgelesen', len(dazu),
                                            bericht['dateien'])))
            self._nachgelesenes_melden(dazu)
        if bericht.get('luecke') and bericht.get('grund'):
            self.q.put(('hinweis', bericht['grund']))

    def _launcher_uebernehmen(self, keys):
        """Was der Launcher kennt, gehört auch in den eigenen Bestand.

        Kein „Import" im Sinne eines einmaligen Grundstocks (den macht das
        Hilfsprogramm unter `tools/`), sondern laufender Betrieb: Er ist die
        genauere Quelle, solange er da ist."""
        neu = 0
        for k in keys:
            if bestand_datei.hinzufuegen(self.bestand, k, 'launcher'):
                neu += 1
        if neu:
            self._bestand_sichern()
        return neu

    def _startbauplaene_eintragen(self):
        """Die acht Startbaupläne in den Bestand — falls noch nicht drin.

        Quelle `start` (Rang 2): höher als ein Log-Fund, niedriger als ein von
        Hand gesetztes Häkchen oder der Launcher. Wer sie also selbst abgehakt
        hat, behält seinen Eintrag."""
        try:
            std = katalog_modul.startbauplaene()
            if not std:
                return
            katalog = katalog_modul.laden()['bauplaene']
            neu = 0
            for schluessel in std:
                name = (katalog.get(schluessel) or {}).get('n')
                if name and bestand_datei.hinzufuegen(self.bestand, name, 'start'):
                    neu += 1
            if neu:
                self._bestand_sichern()
                self.q.put(('status', sprache.Satz('start_eingetragen', neu)))
        except Exception:
            pass          # ein Fehler hier darf den Start nicht aufhalten

    def _katalog_beim_start(self):
        """Fehlt der Katalog ganz, wird er **vor** allem anderen geholt — hier
        ausnahmsweise im Watcher-Takt, nicht nebenher.

        Grund: `_sprache_erschliessen()` braucht die Bauplan-Namen, um aus den
        Logs die Formulierung dieses Clients abzuleiten, und die Nachlese braucht
        diese Formulierung. Käme der Katalog nebenher, liefe beim allerersten
        Start beides ins Leere — bei einem englischen Client hieße das: kein
        einziger Bauplan gefunden, ohne dass jemand den Grund sähe.

        Nur beim ersten Mal. Ist der Katalog da, hält ihn `_katalog_tick()`
        frisch, ohne den Start aufzuhalten."""
        if SCMDB_AUS or katalog_modul.laden()['bauplaene']:
            return
        self.q.put(('status', sprache.Satz('katalog_holt')))
        try:
            neu, anzahl, version = katalog_modul.aktualisieren()
            if neu:
                self.q.put(('status', sprache.Satz('katalog_geholt', anzahl, version)))
                self.kat_next = time.time() + SCMDB_POLL_SEC
            else:
                # Kein Netz: bald noch einmal versuchen, statt sechs Stunden warten.
                self.kat_next = time.time() + 300
        except Exception:
            self.kat_next = time.time() + 300

    def run(self):
        # 0) Ohne Bauplan-Namen lässt sich die Spielsprache nicht erschließen —
        #    also zuerst den Katalog, falls er noch gar nicht da ist.
        self._katalog_beim_start()

        # 1) Klären, wonach überhaupt gesucht wird — sonst liest die
        #    Nachlese mit der falschen Formulierung und findet nichts.
        self._sprache_erschliessen()

        # 2) Startbaupläne eintragen — die hat jeder Spieler von Anfang an,
        #    sie stehen deshalb in **keinem** Log und in keinem Belohnungs-Pool.
        #    Ohne diesen Schritt fehlen sie dauerhaft im Bestand, und der
        #    Fortschritt zeigt weniger an, als man tatsächlich hat.
        self._startbauplaene_eintragen()

        # 3) Vergangenes nachlesen (still, nur in den Bestand)
        self._nachlese()

        # 4) Launcher-Stand holen — wenn es ihn gibt. Ohne ihn wird nicht mehr
        #    gewartet: Bis v1.5.0 hing der Watcher hier in einer Endlosschleife,
        #    wenn die Launcher-Datei fehlte. Unter Linux wäre er nie gestartet.
        if HAT_LAUNCHER:
            for _ in range(10):
                if not self.running:
                    return
                self.known = load_keys()
                if self.known is not None:
                    break
                time.sleep(POLL_SEC)
            if self.known:
                self._launcher_uebernehmen(self.known)

        # 5) Alles, was schon im Bestand steht, gilt als bekannt — es wird nicht
        #    als „neu" gemeldet.
        self.seen = set(bestand_datei.schluessel(self.bestand))
        overlay.NEULESEN_RUECKRUF[0] = self.neu_einlesen_anstossen
        self.tail.new_names()          # Lesestand der Game.log setzen/fortführen
        # ⚠ Was dieser Aufruf an Auftragsereignissen aufgesammelt hat, ist
        # Vergangenheit — es steht in der Log, die gleich ohnehin ganz gelesen
        # wird (`_auftraege_beim_start`). Bliebe es liegen, wertete es der
        # erste Schleifendurchlauf ein zweites Mal aus.
        self.tail.auftraege = []
        self.tail.auftraege_beendet = []
        self.tail.auftrag_ereignisse = []
        # 6) Was laeuft gerade? Das Log weiss es — auch nach einem Neustart
        #    des Watchers. Nach `new_names()`, damit der Lesestand steht und
        #    laufende Meldungen nicht doppelt kommen.
        self._auftraege_beim_start()
        self.q.put(('status', self._statuszeile()))
        while self.running:
            time.sleep(POLL_SEC)

            # -1) Hat jemand um ein erneutes Einlesen gebeten?
            if self._neu_einlesen:
                self._neu_einlesen = False
                self._alles_neu_einlesen()

            # 0) Werte-Daten und Bauplan-Katalog frisch halten
            #    (selten, nur bei neuer Spielversion)
            self._scmdb_tick()
            self._katalog_tick()
            self._texte_tick()
            self._preise_tick()

            # 1) Game.log: die eigentliche Quelle. Ohne Launcher ist die Meldung
            #    endgültig, mit Launcher zunächst vorläufig (er bestätigt gleich).
            geaendert = False
            for name, zusatz in self.tail.new_names():
                nk = _norm(name)
                if nk in self.seen:
                    continue
                self.seen.add(nk)
                if bestand_datei.hinzufuegen(self.bestand, name, 'log'):
                    geaendert = True
                self._emit(name, kuerzel_aus_zusatz(zusatz))
                self._merkliste_erledigen(name)
            if geaendert:
                self._bestand_sichern()

            # 1b) Angenommene Auftraege: bringt der etwas, das noch fehlt?
            #     Bewusst NACH den Bauplaenen — ein frisch erhaltener Bauplan
            #     soll schon im Bestand stehen, wenn der Auftrag geprueft wird.
            self._auftraege_melden()

            # 2) Launcher-Datei: bestätigt die Funde und meldet nach, was im Log
            #    fehlte. Gibt es keinen Launcher, entfällt dieser Schritt still.
            cur = load_keys() if HAT_LAUNCHER else None
            if cur is not None:
                zuwachs = False
                for k in sorted(cur - (self.known or set())):
                    dup = _norm(k) in self.seen      # steht schon in der Liste
                    self.seen.add(_norm(k))
                    if bestand_datei.hinzufuegen(self.bestand, k, 'launcher'):
                        zuwachs = True
                    self._merkliste_erledigen(k)
                    if not dup:
                        self._emit(k)
                if zuwachs:
                    self._bestand_sichern()
                self.known = cur

            # 3) Katalog-Wache (selten, die Datei ändert sich nur bei SC-Patches)
            if time.time() >= self.cat_next:
                self.cat_next = time.time() + CAT_POLL
                self._catalog_tick()

            self.q.put(('status', self._statuszeile()))

    def _merkliste_erledigen(self, name):
        """Worauf gewartet wurde und was jetzt da ist, fliegt von der Merkliste.

        Eine Liste voller längst erfüllter Wünsche wäre keine Merkliste, sondern
        ein Archiv. Der Watcher sagt einmal Bescheid, dann ist es erledigt."""
        try:
            titel = merkliste.erledigen(name)
        except Exception:
            return
        if titel:
            self.q.put(('hinweis', sprache.Satz('merk_erledigt', titel)))

    def _statuszeile(self):
        """Was unten im Fenster steht. Zeigt den **eigenen** Bestand — nicht mehr
        die Launcher-Zahl, denn der Launcher ist ab jetzt nur noch eine von
        mehreren Quellen (und zählt nachweislich zu niedrig)."""
        log_state = '✓' if self.tail.path else '–'
        # ⚠ Vorlage **und** Bausteine über `sprache.t` — beide Schlüssel gab es
        # längst, benutzt wurde keiner. Ergebnis: Wer auf Englisch stellte,
        # bekam eine englische Oberfläche und eine deutsche Statuszeile.
        # ⚠ Ein `Satz`, kein fertiger Text: Auch die Statuszeile bleibt stehen,
        # bis die nächste Meldung kommt — sie muss sich beim Sprachwechsel neu
        # zusammensetzen lassen. Der eingesetzte Baustein ist selbst ein `Satz`
        # und wird dabei mit übersetzt; nur die Uhrzeit bleibt eingefroren, und
        # das ist richtig — der Zeitpunkt der Meldung ändert sich nicht.
        quelle = sprache.Satz('mit_launcher' if (HAT_LAUNCHER and self.known)
                              else 'ohne_launcher')
        return sprache.Satz('ueberwache', bestand_datei.anzahl(self.bestand),
                            log_state, quelle, time.strftime('%H:%M:%S'))

    def stop(self):
        self.running = False


# ---------------------------------------------------------------- GUI / Overlay
# Mauszeiger heißen je Fenstersystem anders. `size_nw_se` gibt es nur unter
# Windows — unter Linux und macOS wirft tkinter dafür einen Fehler und das
# Fenster kommt gar nicht erst hoch. `hand2` dagegen kennen alle drei.
CURSOR_GROESSE = 'size_nw_se' if pfade.WINDOWS else 'bottom_right_corner'


def sicherer_cursor(name):
    """Gibt den Zeigernamen zurück, wenn dieses System ihn kennt — sonst ''.

    Geprüft wird an einem Wegwerf-Widget: Das ist der einzige verlässliche Weg,
    weil die Namensliste von der Tk-Version abhängt, nicht nur vom System."""
    try:
        probe = tk.Label(None, cursor=name)
        probe.destroy()
        return name
    except Exception:
        return ''



class Overlay:
    def __init__(self, wurzel=None):
        """`wurzel` ist die eine Tk-Instanz des Programms — siehe unten, warum es
        nur eine geben darf."""
        # ⚠ Vor allem anderen: Liegen die Dateien noch am alten Ort (bis v2.x
        # versteckt in %APPDATA% bzw. ~/.config), werden sie in den sichtbaren
        # Ordner unter Dokumente **kopiert**. Erst danach darf irgendetwas
        # gelesen werden — sonst startet der Spieler mit leerer Liste, obwohl
        # sein Bestand nur woanders liegt.
        # Nach einem Selbst-Update zeigt Windows sonst weiter die alte Nummer.
        try:
            aktualisierung.windows_eintrag_pflegen(__version__)
        except Exception as ausnahme:
            fehler.merken('start.windows_eintrag', ausnahme)

        self.umzug_meldung = ''
        try:
            if pfade.umzug_noetig():
                anzahl = pfade.umziehen()
                if anzahl:
                    self.umzug_meldung = sprache.t('umzug_fertig', anzahl,
                                                   pfade.app_ordner())
                    sys.stdout.write(self.umzug_meldung + '\n')
        except Exception as ausnahme:
            fehler.merken('start.umzug', ausnahme)

        # ⚠ **Nur eine einzige `tk.Tk()` im ganzen Programm.** Vorher legte der
        # Assistent eine eigene an, zerstörte sie am Ende — und hier entstand eine
        # zweite. Das ist der Fall, den Tk nicht verlässlich verträgt: Nach dem
        # `destroy()` der ersten leben Schriften, Bilder und offene `after`-Aufträge
        # weiter und zeigen auf einen toten Interpreter. Ob das gutgeht, hängt am
        # Zeitpunkt — bei einem Tester (Bomb20, 25.08.2026) endete der **erste**
        # Programmstart reproduzierbar mit `SIGSEGV`, direkt nach dem Nachlesen der
        # Logs. Sein Satz „mit Debugging an lief es durch" ist der Fingerabdruck
        # eines solchen Zeitproblems: Langsamer läuft es zufällig richtig.
        #
        # Deshalb wird die Wurzel **einmal** erzeugt und weitergereicht; der
        # Assistent ist seitdem ein `Toplevel` daran.
        self.root = wurzel if wurzel is not None else tk.Tk()
        # Ab hier werden auch Fehler in Rückrufen der Oberfläche festgehalten.
        # Ohne diesen Haken schreibt Tk sie auf die Standardausgabe — und die
        # sieht in einer .exe oder einem AppImage niemand.
        fehler.haken_setzen(self.root)
        # Die Tastenkombination, die auch im Spiel greift. ⚠ Angemeldet wird
        # erst, wenn die Hauptschleife laeuft (`hotkey_anmelden`) — vorher
        # gibt es den Faden noch nicht, an dem die Meldung haengt.
        self.hotkey = hotkey_modul.Wache()
        _WURZEL[0] = self.root                    # damit signalton() klingeln kann
        # Damit der Knopf „Fensterlage zurücksetzen" das Overlay sofort in die Mitte
        # setzen kann, ohne dass `seiten.py` das Hauptprogramm importieren müsste.
        bildschirm.OVERLAY[0] = self.root
        overlay.OVERLAY_FENSTER[0] = self.root
        # Merken, ob der Zeiger auf dem Overlay steht — das entscheidet, ob eine
        # Einblendung stehen bleibt. Echte Ereignisse statt Positionsabfrage.
        self.root.bind('<Enter>', lambda e: setattr(self, '_maus_drauf', True),
                       add='+')
        self.root.bind('<Leave>', lambda e: setattr(self, '_maus_drauf', False),
                       add='+')
        overlay.OVERLAY_STEUERUNG[0] = self
        # Damit jeder festgehaltene Fehler weiß, aus welcher Version er stammt.
        fehler.VERSION[0] = __version__
        self.root.title('SC BP Watcher')
        self.root.configure(bg=BG)
        self.root.overrideredirect(True)          # randloses Overlay
        self.root.attributes('-topmost', True)    # immer im Vordergrund
        # Wie sich das Fenster im Spiel verhält — siehe scbp/overlay.py.
        # 'immer' = steht dauerhaft da (wie bisher), 'popup' = zeigt sich nur,
        # wenn wirklich ein Bauplan dazukommt.
        self.anzeigeart = pfade.einstellung('overlay_modus') or 'immer'
        self._popup_uhr = None
        self._letzte_lage = ''
        self._anfasser = None
        self._schloss = None
        # Das Schloss zieht mit, egal wer das Durchreichen umschaltet — hier im
        # Overlay oder drüben in den Einstellungen.
        overlay.SCHLOSS_RUECKRUF[0] = self._schloss_anwenden
        self._maus_drauf = False
        # Durchsichtigkeit einstellbar (30–100 %). Wer nur **einen** Monitor hat,
        # legt das Overlay zwangsläufig übers Spiel — dann muss man hindurchsehen
        # können. 93 % bleibt der Standard, das ist auf zwei Bildschirmen richtig.
        self.root.attributes('-alpha', DECKKRAFT / 100.0)
        self.root.geometry(startlage(self.root))
        # ⚠ Erst wenn alles gebaut ist, kennt die Kopfleiste ihre Breite —
        # deshalb ueber `after_idle` und nicht hier direkt.
        self.root.after_idle(self._mindestgroesse_setzen)
        self._icon_setzen()
        self.count = 0
        self.rows = {}          # normalisierter Name -> Zeilen-Widgets (für die Bestätigung)

        # ⚠ Die Schriftgröße aus den Einstellungen gilt **auch hier**. Sie wirkte
        # lange nur im großen Fenster; im Overlay standen feste Größen. Wer sie
        # auf „groß" stellte, weil er die Zeilen im Spiel nicht lesen konnte,
        # änderte damit ausgerechnet das Fenster nicht, um das es ihm ging.
        # Gemeldet von Haldjas, 25.08.2026.
        #
        # Die Grundwerte liegen eins unter den früheren festen Größen, damit die
        # Stufe „normal" (= 1) genau das bisherige Aussehen ergibt — niemand,
        # der nichts eingestellt hat, sieht plötzlich ein anderes Overlay.
        (self.f_title, self.f_item, self.f_sub) = self._schriften_anlegen()
        # Die Symbolgröße hängt an derselben Stufe wie die Schriften.
        zeichen.stufe_setzen(pfade.einstellung('schriftgroesse') or 'normal')

        # --- Titelleiste (Drag-Griff + Schließen) ---
        # ⚠ Die Höhe wächst mit der Schriftgröße mit. Sie stand lange fest auf
        # 26 px — bei „groß" ragten die Symbole dann oben und unten heraus.
        bar = tk.Frame(self.root, bg=BAR, height=zeichen.breite() + 4)
        # Für die Mindestbreite gemerkt: Schmaler als diese Leiste darf das
        # Overlay nicht werden, sonst fehlen die Symbole.
        self.kopf = bar
        bar.pack(fill='x', side='top')
        bar.pack_propagate(False)
        self.bar = bar
        # Sitzt das Overlay in einer UNTEREN Ecke, gehoert die Leiste an den
        # unteren Fensterrand — sonst klebt sie eine Fensterhoehe ueber dem
        # Bildschirmrand. Siehe `_leiste_ausrichten`.
        self._leisten_seite = 'top'
        titel_lbl = tk.Label(bar, text=f'● SC BP Watcher v{__version__}', bg=BAR,
                             fg=ACCENT, font=self.f_title)
        titel_lbl.pack(side='left', padx=8)
        hinweis.anhaengen(titel_lbl, lambda: sprache.t('hinweis_ziehen'))

        # ⚠ Alle Symbole kommen aus `scbp/zeichen.py` — fertige Bilder aus dem
        # Lucide-Satz, nicht mehr Schriftzeichen. Warum, steht dort ausführlich;
        # der Kern: Schriftzeichen füllen ihre Box unterschiedlich weit aus (die
        # Glocke war die größte), mischen gefüllte und gestrichelte
        # Handschriften, und sehen auf jedem Betriebssystem anders aus.
        #
        # Die Reihenfolge ist von **rechts** gedacht, weil `side='right'` packt:
        # Schließen ganz außen, dann Leeren, Einklappen, Liste, Einstellungen,
        # Spielstart, Glocke.
        zu_lbl = zeichen.knopf(bar, 'schliessen', self.quit, ersatz='X',
                               schrift=self.f_title)
        zu_lbl.pack(side='right', padx=8)
        hinweis.anhaengen(zu_lbl, lambda: sprache.t('hinweis_schliessen'))

        # ⚠ Ein Radiergummi, kein Mülleimer. Der Knopf **löscht nichts** — er
        # räumt nur die angezeigten Meldungen weg, die Baupläne bleiben (siehe
        # `hinweis_leeren`). Ein Mülleimer verspricht Vernichtung, und genau
        # deshalb traut sich niemand, ihn zu drücken. Gemeldet am 27.08.2026:
        # „Mülleimer für leeren schon gut, aber gäbe es da was besseres?"
        leeren_lbl = zeichen.knopf(bar, 'leeren', self.clear,
                                   schrift=self.f_title)
        leeren_lbl.pack(side='right')
        hinweis.anhaengen(leeren_lbl, lambda: sprache.t('hinweis_leeren'))

        # Einklappen: nur die Titelleiste bleibt stehen. Für alle mit **einem**
        # Bildschirm — dort liegt das Overlay zwangsläufig über dem Spiel, und
        # Durchsichtigkeit allein reicht nicht, wenn man gerade freie Sicht
        # braucht. Ersetzt zugleich das nie gebaute Ablage-Symbol (Tray): Das
        # bräuchte Zusatzpakete, ein eingeklappter Streifen nicht.
        self.klapp_lbl = zeichen.knopf(bar, 'einklappen', self.umklappen,
                                       schrift=self.f_title)
        self.klapp_lbl.pack(side='right', padx=(0, 6))
        hinweis.anhaengen(self.klapp_lbl, self._hinweis_klappen)

        # ⚠ Der **Hinweg** zum Durchreichen. Bis rc89 gab es nur den Rückweg:
        # Das schwebende Schloss erscheint erst, wenn durchgereicht wird, und
        # verschwindet beim Abschalten wieder — danach führte der einzige Weg
        # über Einstellungen → Overlay. Haldjas (pr0) am 28.08.2026: „man kann
        # das durchklicken entfernen, aber eventuell kann der button zum locken
        # stehen bleiben? sonst muss man ja erst wieder in die einstellungen".
        #
        # Ein **offenes** Schloss heißt „das Overlay fängt Klicks ab" — ein
        # Klick sperrt zu. Das geschlossene Schloss taucht dann als eigenes
        # Fenster auf (`_schloss_anwenden`), weil diese Leiste hier ab dem
        # Moment nicht mehr zu treffen ist.
        #
        # Nur, wo das System es überhaupt kann: Unter nativem Wayland wäre ein
        # Knopf ohne Wirkung schlimmer als keiner — dieselbe Regel wie beim
        # Schalter in den Einstellungen.
        if overlay.durchklickbar_moeglich():
            self.schloss_lbl = zeichen.knopf(bar, 'schloss_auf',
                                             self._schloss_zusperren,
                                             schrift=self.f_title)
            self.schloss_lbl.pack(side='right', padx=(0, 6))
            hinweis.anhaengen(self.schloss_lbl,
                              lambda: sprache.t('hinweis_schloss_zu'))

        # ⚠ **Protokolle erneut einlesen** — der Knopf gehört hierher und nicht
        # nur in die Einstellungen. Der Fall, für den es ihn gibt, tritt genau
        # dann ein, wenn niemand im Einstellungsfenster ist: Watcher zu, Star
        # Citizen läuft weiter, Baupläne kommen. Wer danach merkt, dass einer
        # fehlt, soll ihn dort finden, wo er ohnehin hinsieht.
        self.neulesen_lbl = zeichen.knopf(bar, 'neustart',
                                          self._logs_neu_einlesen,
                                          schrift=self.f_title)
        self.neulesen_lbl.pack(side='right', padx=(0, 6))
        hinweis.anhaengen(self.neulesen_lbl,
                          lambda: sprache.t('hinweis_neulesen'))

        # Zwei Ansichten, ein Programm: die schmale Melde-Leiste bleibt, das
        # Verwaltungsfenster kommt auf Klick dazu.
        # ⚠ Ein Klemmbrett statt der drei Striche. Drei Striche heissen
        # „irgendeine Liste", ein Klemmbrett heisst „deine gesammelten Sachen".
        # Beim Vergleich mit dem SC-Deutsch-Launcher (26.08.2026):
        # „dieses klemmbrett für die BP ist auch besser."
        #
        # ⚠ Der Knopf bleibt, obwohl er auf den ersten Blick dasselbe tut wie das
        # Zahnrad daneben. Tut er nicht: Beide öffnen dasselbe Fenster, aber auf
        # **verschiedenen Seiten** — hier die Bauplan-Liste, dort die
        # Einstellungen. Am 27.08.2026 stand er kurz vor dem Rauswurf, weil
        # genau das täuscht. Ohne ihn führte der Weg zur Hauptsache des
        # Programms über die Einstellungen und einen Reiterwechsel.
        self.liste_lbl = zeichen.knopf(bar, 'liste', self.liste_oeffnen,
                                       schrift=self.f_title)
        self.liste_lbl.pack(side='right', padx=(0, 6))
        hinweis.anhaengen(self.liste_lbl, lambda: sprache.t('hinweis_liste'))

        # Das Zahnrad ist der direkte Griff in die Einstellungen. Bis v3.0.0 lag
        # daneben noch ein zweiter Knopf für den Einrichtungs-Assistenten — der
        # ist am 27.08.2026 entfallen. Gemeldet: „assitant neu starten, reicht
        # glaube ich in den einstellungen, da gehen die leute eh hin wenn die
        # merken es klemmt etwas." Erreichbar bleibt er über das große Fenster,
        # oben rechts („Einrichtung starten").
        self.einst_lbl = zeichen.knopf(bar, 'einstellungen',
                                       self.einstellungen_oeffnen,
                                       schrift=self.f_title)
        self.einst_lbl.pack(side='right', padx=(0, 6))
        hinweis.anhaengen(self.einst_lbl,
                          lambda: sprache.t('hinweis_einstellungen'))

        # ⚠ Der Startknopf gehört **hierher**, nicht auf eine Unterseite. Er saß
        # erst unter „Angaben im Spiel" — also dort, wo es um Auftragstexte
        # geht, und da sucht ihn niemand. Dazu: „wenn leute den suchen
        # müssen ist er falsch platziert."
        #
        # Wer das Spiel starten will, hat das große Fenster nicht offen; er
        # sieht das Overlay. Deshalb steht das Zeichen hier, in Grün — und nur
        # dann, wenn wirklich ein Weg gefunden wurde (`pfade.spielstarter()`).
        #
        # ⚠ Eine Rakete, kein Abspielpfeil. Ein `▶` heißt überall „Video ab",
        # nicht „Programm starten"; eine Rakete sagt beides — starten und
        # Weltraum. Gemeldet am 27.08.2026: „SC Starten ist das symbol nicht
        # eindeutig genug".
        if pfade.spielstarter():
            self.start_lbl = zeichen.knopf(bar, 'starten', self._spiel_starten,
                                           farbe=zeichen.GRUEN,
                                           schrift=self.f_title)
            self.start_lbl.pack(side='right', padx=(0, 6))
            # ⚠ Erklärung wie bei allen anderen Zeichen über `hinweis`,
            # **nicht** über die Statuszeile: Die zeigt echte Meldungen, und
            # der frühere Weg stellte danach `_status_text` wieder her — einen
            # Merker, der nie fortgeschrieben wird. Ein Bauplanfund war nach
            # einem Mausschlenker damit überschrieben.
            hinweis.anhaengen(self.start_lbl, lambda: sprache.t('s_sp_start'))

        # ⚠ Eine Glocke statt des `ⓘ`. Ein „i" heisst „hier steht etwas", eine
        # Glocke heisst „fuer dich ist etwas da" — und genau darum geht es hier,
        # denn das Zeichen faerbt sich gruen, wenn eine neue Version bereitsteht.
        # Gemeldet am 26.08.2026: „Die Glocke für Updates ist auch besser."
        self.info_lbl = zeichen.knopf(bar, 'glocke',
                                      lambda: self.fenster_oeffnen('ueber'),
                                      schrift=self.f_title)
        self.info_lbl.pack(side='right', padx=(0, 6))
        # ⚠ Führt ins **Hauptfenster**, nicht mehr in ein eigenes Infofenster.
        # Es gab zwei Wege zu Änderungen und Updates, und nur einer war zu Ende
        # gebaut: Im Infofenster fehlte der Neustart-Knopf, deshalb lud Morkhan
        # am 26.08.2026 dreimal vergeblich. Ein Weg statt zwei.
        #
        # Und zwar auf **„Update & Über"**: Wer auf das Zeichen klickt, will
        # meistens wissen, ob es etwas Neues gibt — und landet so direkt beim
        # Knopf. „Was ist neu" liegt einen Reiter daneben und ist einen Klick
        # entfernt.
        hinweis.anhaengen(self.info_lbl, self._hinweis_info)
        # Dasselbe für die Sprache: Wer in den Einstellungen auf Englisch
        # stellt, soll die Melde-Leiste **sofort** englisch sehen — nicht erst
        # nach einem Neustart, und nicht halb.
        sprache.anmelden(self._neu_beschriften)
        for w in (bar, bar.winfo_children()[0]):
            w.bind('<Button-1>', self._drag_start)
            w.bind('<B1-Motion>', self._drag_move)
            w.bind('<ButtonRelease-1>', self._save_geo)   # Position nach dem Ziehen merken

        # --- Statuszeile ---
        self._status_text = sprache.t('ov_starte')
        # Woher der Text in der Statuszeile kam — solange dort der
        # Starttext steht, gibt es nichts aufzufrischen.
        self._status_quelle = None
        self.status = tk.Label(self.root, text=self._status_text, bg=BG, fg=SUB,
                               font=self.f_sub, anchor='w')
        self.status.pack(fill='x', padx=8, pady=(4, 2))

        # --- Laufende Auftraege ------------------------------------------
        # ⚠ Ein Zustand, keine Verlaufszeilen: Was abgeschlossen, abgebrochen
        # oder gescheitert ist, verschwindet hier wieder. Wer an einem Abend
        # zehn Auftraege macht, soll nicht zehn tote Zeilen ansehen muessen.
        # Die Leiste bleibt leer und unsichtbar, solange nichts laeuft.
        self._auftrag_zeilen = []
        self.auftragsleiste = tk.Frame(self.root, bg=BG)

        # --- Liste (scrollbar) ---
        wrap = tk.Frame(self.root, bg=BG)
        wrap.pack(fill='both', expand=True, padx=6, pady=(0, 6))
        self._listen_traeger = wrap
        self.canvas = tk.Canvas(wrap, bg=BG, highlightthickness=0)
        # ⚠ Keine tk.Scrollbar: Die reicht Tk an das System durch — unter Linux
        # grau, auf dem Mac hellweiss, und damit der einzige Fleck, der aus dem
        # Bild faellt. Genau so gemeldet: "scrollbalken im watcher selber ist
        # auch nicht passend". Die vier Rollbereiche im Hauptfenster hatten den
        # Umbau schon; hier stand er noch aus.
        from scbp.hauptfenster import rundleiste
        sb = rundleiste(wrap, self.canvas, grund=BG)
        self.list = tk.Frame(self.canvas, bg=BG)
        self.list.bind('<Configure>',
                       lambda e: self.canvas.configure(scrollregion=self.canvas.bbox('all')))
        # Die Liste muss so breit sein wie das Fenster. Bis v1.2.0 stand hier ein fester
        # Wert (312 px) — dadurch wurden lange Namen abgeschnitten und Breiterziehen
        # brachte nichts. Jetzt wird die Breite bei jeder Größenänderung nachgezogen.
        self._list_id = self.canvas.create_window((0, 0), window=self.list, anchor='nw', width=312)
        self._wrap_labels = []          # Untertitel, die umbrechen dürfen
        self.canvas.bind('<Configure>', self._fit_width)
        self.canvas.configure(yscrollcommand=sb.set)
        self.canvas.pack(side='left', fill='both', expand=True)
        sb.pack(side='right', fill='y')

        self._placeholder()

        # Resize-Griff unten rechts
        #
        # ⚠ Er wird beim Einklappen **ausgeblendet** (siehe `umklappen`). Er
        # sitzt auf `rely=1.0`, und bei einem auf Leistenhöhe geschrumpften
        # Fenster ist „unten rechts" dieselbe Stelle wie „oben rechts" — er
        # legte sich dann über das ✕, und man musste zielen, um das Fenster
        # überhaupt schließen zu können. Ein 26 Pixel hohes Fenster in der Höhe
        # zu ziehen ergibt ohnehin keinen Sinn.
        # ⚠⚠ **Seit rc26 hängt der Griff wieder am Fenster.** Der Grund dafür,
        # ihn an die Liste zu hängen, steht unten und war richtig — nur trägt
        # er nicht mehr: Seit die Auftragsleiste über der Liste Platz nimmt,
        # kann die Liste **niedriger werden als der Griff selbst**. Bei einem
        # schmalen Overlay mit einem laufenden Auftrag blieben ihr rund 20
        # Pixel, der Griff braucht 26 — und war schlicht weg. Gemeldet am
        # 29.08.2026, zweimal: „kein Anfasser da zum Vergrössern des Fensters."
        #
        # Das Einklapp-Problem von damals löst inzwischen `_grip_nachziehen()`:
        # Die Methode gab es zu jener Zeit noch nicht, sie setzt den Zustand
        # durch, statt ihn einmal zu setzen.
        #
        # --- Die alte Begründung, zur Einordnung: ---
        # ⚠ Der Griff hängt an der **Liste**, nicht am Fenster.
        #
        # Am Fenster (`self.root`) sitzt er auf `rely=1.0` — bei einem auf
        # Leistenhöhe eingeklappten Overlay ist „unten rechts" dieselbe Zeile
        # wie die Titelleiste, und er stand als Dreieck neben dem ✕. Ihn dort
        # rechtzeitig auszublenden hat dreimal nicht verlässlich geklappt: Der
        # Zustand hängt am Zeitpunkt des Aufbaus.
        #
        # Als Kind der Liste kann er dort gar nicht mehr auftauchen — ist sie
        # eingeklappt, hat sie keine Höhe, und mit ihr ist er weg. Kein Timing,
        # keine Sonderbehandlung. Ein Zustand, der sich aus dem Aufbau ergibt,
        # ist verlässlicher als einer, den man nachträglich herstellt.
        # ⚠ Größer und in der Akzentfarbe. In Grau und Schriftgröße war er
        # kaum zu finden — „der Anfasser zum Größerziehen ist nicht mehr da
        # oder nicht mehr zu sehen" (29.08.2026). Er ist der einzige Weg, das
        # Overlay in der Größe zu ändern; wer ihn nicht sieht, hält die Größe
        # für fest.
        # ⚠ **Symbol aus dem Satz, kein Schriftzeichen.** Bis zum 02.09.2026
        # stand hier das Zeichen „◢" — es zeigte in drei von vier Ecken in die
        # falsche Richtung, und Schriftzeichen als Symbole sind ohnehin
        # ausgeschlossen (sie sind auf jedem System anders gross). Jetzt sind
        # es vier Lucide-Pfeile, einer je Richtung; `_grip_nachziehen()`
        # tauscht den passenden ein.
        self.grip = zeichen.knopf(self.root,
                                  self.GRIFF_SYMBOLE[(False, False)],
                                  farbe=zeichen.GRUEN, grund=BG)
        self.grip.configure(cursor=sicherer_cursor(CURSOR_GROESSE))
        self.grip.place(relx=1.0, rely=1.0, anchor='se')
        self.grip.bind('<B1-Motion>', self._resize)
        # ⚠ Den Zustand **durchsetzen**, nicht einmal setzen. Zweimal wurde der
        # Griff beim Start im eingeklappten Zustand trotzdem angezeigt, obwohl
        # das Verstecken nachweislich aufgerufen wurde — irgendein Schritt im
        # Aufbau stellt ihn danach wieder her. Statt diese Stelle weiter zu
        # suchen, wird bei jedem Layout-Ereignis geprüft, ob der Griff zum
        # Klappzustand passt. Das kostet nichts: Stimmt es schon, kehrt die
        # Prüfung sofort zurück.
        self.root.bind('<Configure>', self._grip_nachziehen, add='+')
        self.root.bind('<Map>', self._grip_nachziehen, add='+')
        self.grip.bind('<ButtonRelease-1>', self._save_geo)   # Größe merken
        hinweis.anhaengen(self.grip, lambda: sprache.t('hinweis_groesse'))

        # Watcher starten
        # Version an die Bauplan-Liste durchreichen — sie landet im
        # scmdb-Export als Kennung des erzeugenden Werkzeugs.
        bestandsfenster_modul.VERSION[0] = __version__
        self.eingeklappt = False
        self.hoehe_offen = None      # Fensterhöhe vor dem Einklappen
        # ⚠⚠ **Auch die Breite.** Eingeklappt behielt das Overlay bis v3.7.0
        # seine volle Breite — bei 1160 Pixeln ist das ein Balken quer ueber
        # den halben Bildschirm, und in eine Ecke bekommt man ihn nie. Genau
        # das am 31.08.2026 gemeldet: „stoert mich irgendwie, dass es nicht
        # komplett in der Ecke sitzt."
        self.breite_offen = None     # Fensterbreite vor dem Einklappen
        # ⚠⚠ **Aus der gemerkten Lage vorbelegen — sonst schrumpft das Overlay
        # bei jedem Start.** `klappzustand_setzen(False)` rechnet die offene
        # Groesse als `max(hoehe_offen, Leiste + 120)`. Stehen die beiden Werte
        # auf None, kommt dabei die Mindestgroesse heraus: Aus 620x316 wurden
        # rund 146 px Hoehe.
        #
        # Getroffen hat es jeden, der eine feste Ecke eingestellt hat — nur
        # dann wird `klappzustand_setzen` beim Start ueberhaupt gerufen (siehe
        # `ecke_beim_start`). Wer sein Overlay frei stehen laesst, merkte
        # nichts; deshalb fiel es lange nicht auf. Gemeldet am 04.09.2026:
        # „das overlay startet bei mir immer in klein, das merkt sich seine
        # groesse grad nicht."
        #
        # ⚠ Nur wenn das Fenster offen gespeichert wurde. War es eingeklappt,
        # ist die gemerkte Groesse die des Streifens — die als „offen" zu
        # uebernehmen hiesse, es liesse sich nie wieder richtig aufklappen.
        if not pfade.einstellung_wahrheit('eingeklappt', False):
            _m_lage = GEOM_RE.match(load_geometry() or '')
            if _m_lage:
                self.breite_offen = int(_m_lage.group(1))
                self.hoehe_offen = int(_m_lage.group(2))
        if pfade.einstellung_wahrheit('eingeklappt', False):
            # Zustand **setzen**, nicht umschalten — siehe `klappzustand_setzen`.
            # `merken=False`: Es ist genau der Zustand, der schon gespeichert
            # ist; ihn erneut zu schreiben wäre nur ein Schreibzugriff mehr.
            self.root.after(120, lambda: self.klappzustand_setzen(True,
                                                                 merken=False))

        self.q = queue.Queue()
        self.watcher = Watcher(self.q)
        self.watcher.start()
        self.root.after(200, self._poll_queue)
        # ⚠ **Und danach stündlich wieder** — siehe `_nach_version_sehen`.
        self.root.after(2000, self._nach_version_sehen)   # nicht beim Start drängeln
        # Kurz nach dem Start fragen, falls der Spielordner umgezogen ist.
        # Vor der Versionsprüfung: Ohne Spielordner nützt die neueste Fassung
        # nichts, und zwei Fenster hintereinander will niemand.
        self.root.after(1200, self._kanal_pruefen)

    def _kanal_pruefen(self):
        """Ist der eingetragene Spielordner weg, aber ein Nachbarkanal da?

        ⚠⚠ Der Fall aus der Praxis: Kommt eine ausgebesserte Fassung neben LIVE,
        laedt kaum jemand 100 GB neu — man benennt LIVE in HOTFIX um, damit der
        Launcher nur die Unterschiede holt. Der eingetragene Ordner ist damit
        weg. Bis v3.10.0 stand der Watcher dann ohne Erklaerung da: „Star Citizen
        nicht gefunden", obwohl in den Einstellungen ein Pfad steht.

        ⚠ Nur **einmal je Programmstart** fragen. Wer „jetzt nicht" sagt, will
        Ruhe — und ein Fenster, das immer wiederkommt, klickt man ungelesen weg.
        """
        if getattr(self, '_kanal_gefragt', False):
            return
        self._kanal_gefragt = True
        try:
            lage = pfade.kanal_abweichung()
            if not lage:
                return
            # ⚠ Lokal importiert wie überall in dieser Datei — `hauptfenster`
            # zieht selbst wieder Bausteine nach, auf Modulebene wäre das ein
            # Zirkelbezug.
            from scbp import hauptfenster as _hf
            eingetragen, _benutzt, kanaele = lage
            gewaehlt = _hf.kanal_waehlen(self.root, eingetragen, kanaele)
            if not gewaehlt:
                return
            pfade.einstellung_setzen('spiel_ordner', gewaehlt)
            _hf.bescheid_geben(
                self.root, sprache.t('s_kn_titel'),
                sprache.t('s_kn_umgestellt') % os.path.basename(gewaehlt))
        except Exception as ausnahme:
            fehler.merken('watcher.kanal_pruefen', ausnahme)

    # ---- Drag & Resize ----
    # ---- Schalter „mit dem Rechner starten" ----
    # ---- Erklärtexte, die ihren Zustand kennen ----
    # --------------------------------------------------------- Schriftgrößen
    # ⚠ `f_title` traegt den Titeltext der Leiste. 9 Punkt waren zu klein —
    # Im Vergleich mit dem SC-Deutsch-Launcher (26.08.2026): „die
    # button größe oben, ist auch deutlich angenehmer". Auf einem 4096 Pixel
    # breiten Bildschirm bei 100 % Skalierung ist das keine Geschmacksfrage.
    #
    # ⚠ Eine vierte Schrift `f_zeichen` gab es hier bis v3.0.0-rc55, eigens für
    # die Symbole, samt einer eigenen Schriftfamilie (`Segoe UI Symbol`) — weil
    # in `Segoe UI` kein einziges der Symbole steckt und Windows sonst zur
    # Farb-Emoji-Schrift greift. Beides ist entfallen: Die Symbole sind seit dem
    # 27.08.2026 **Bilder** und hängen an keiner Schrift mehr (siehe
    # `scbp/zeichen.py`). Damit ist auch die alte Schwierigkeit weg, die gemalten
    # und die geschriebenen Zeichen auf eine Größe zu bringen.
    #
    # Wer eine der Zahlen aendert, sieht sich die Leiste danach an — auf dem
    # Bildschirm, nicht im Code.
    OVERLAY_GRUND = (('f_title', 'Segoe UI Semibold', 11),
                     ('f_item', 'Consolas', 8),
                     ('f_sub', 'Segoe UI', 7))

    def _stufe(self):
        from scbp.hauptfenster import STUFEN
        return STUFEN.get(pfade.einstellung('schriftgroesse') or 'normal', 1)

    def _schriften_anlegen(self):
        n = self._stufe()
        return tuple(tkfont.Font(family=fam, size=grund + n)
                     for _, fam, grund in self.OVERLAY_GRUND)

    def schriftgroesse_anwenden(self, stufe=None):
        """Zieht die Overlay-Schriften nach — sofort, ohne Neustart.

        Tk-Font-Objekte sind benannt: Ein `configure` wirkt auf jedes Widget,
        das die Schrift benutzt. Deshalb genügt es, die drei Objekte zu ändern,
        statt die Zeilen neu zu bauen.
        """
        from scbp.hauptfenster import STUFEN
        n = STUFEN.get(stufe, self._stufe()) if stufe else self._stufe()
        for (name, _, grund) in self.OVERLAY_GRUND:
            try:
                getattr(self, name).configure(size=grund + n)
            except Exception as ausnahme:
                fehler.merken('overlay.schriftgroesse', ausnahme)
        # ⚠ Die Symbole hängen **nicht** an einer Schrift, also müssen sie
        # eigens nachgezogen werden — sonst bleibt die Leiste bei „groß" auf
        # kleinen Symbolen stehen. Die Leistenhöhe wächst mit, sonst ragen sie
        # oben und unten heraus.
        try:
            zeichen.stufe_setzen(stufe or (
                pfade.einstellung('schriftgroesse') or 'normal'))
            self.bar.configure(height=zeichen.breite() + 4)
        except Exception as ausnahme:
            fehler.merken('overlay.symbolgroesse', ausnahme)

    def _spiel_starten(self):
        """Star Citizen starten — über den Weg, den der Spieler ohnehin nutzt."""
        ok, grund = pfade.spiel_starten()
        if ok:
            self._status_setzen(sprache.Satz('s_sp_start_lauft'))
        else:
            self._status_setzen(sprache.Satz('s_sp_start_nein', grund))
            fehler.merken('overlay.spiel_starten', OSError(str(grund)))

    def _ganz_beenden(self):
        """Beenden über das Symbol neben der Uhr — und zwar wirklich.

        ⚠ `destroy()` allein hat das Fenster geschlossen und den Prozess leben
        lassen: Es beendet die Ereignisschleife, nicht das Programm. Läuft noch
        ein Faden (Watcher, Netzabruf), bleibt das Ganze im Speicher stehen —
        genau das, was Haldjas am 25.08.2026 gesehen hat („als hätte er nur das
        symbol von der taskleiste gekillt").

        Zuerst wird sauber zugemacht, damit der Bestand geschrieben wird. Wer
        nach drei Sekunden immer noch hängt, wird hart beendet — bis dahin ist
        alles Wichtige auf der Platte.
        """
        threading.Timer(3.0, lambda: os._exit(0)).start()
        try:
            self.root.destroy()
        except Exception:
            pass

    def _icon_setzen(self):
        """Fenster- und Taskleisten-Icon — auf beiden Systemen und für alle Fenster.

        Vorher stand hier nur `iconbitmap('icon.ico')`. Das hatte zwei Löcher:
        `iconbitmap` mit einer `.ico` ist **Windows-only**, unter Linux blieb das
        Fenster ohne Icon. Und die Datei lag zur Laufzeit gar nicht daneben —
        PyInstallers `--icon` setzt nur das Symbol der `.exe` selbst, es packt
        die Datei nicht mit ein. In der fertigen Version gab es das Icon also
        nirgends, auch unter Windows nicht.

        `iconphoto(True, …)` mit dem PNG kann Tk auf beiden Systemen, und das
        `True` vererbt es an **alle** weiteren Fenster (Liste, Einstellungen,
        Assistent) — sonst müsste jedes es selbst setzen.
        """
        try:
            png = _mitgeliefert(os.path.join('assets', 'icon.png'))
            if png and os.path.exists(png):
                # ⚠ Die Referenz muss am Objekt hängen bleiben. Eine lokale
                # Variable wird nach der Methode aufgeräumt, und Tk zeigt dann
                # ein leeres Icon — das Bild ist weg, bevor es gebraucht wird.
                self._icon_bild = tk.PhotoImage(file=png)
                self.root.iconphoto(True, self._icon_bild)
        except Exception as ausnahme:
            fehler.merken('oberflaeche.icon', ausnahme)

        if sys.platform.startswith('win'):
            # Zusätzlich unter Windows: Die .ico bringt mehrere Auflösungen mit
            # und sieht in der Taskleiste schärfer aus als ein skaliertes PNG.
            try:
                ico = _mitgeliefert('icon.ico')
                if ico and os.path.exists(ico):
                    self.root.iconbitmap(ico)
            except Exception:
                pass

    def _hinweis_info(self):
        """Die Glocke heißt zweierlei — Versionsgeschichte, und bei Grün: „es
        gibt Neues"."""
        gruen = getattr(self.info_lbl, 'symbol_farbe', '') == zeichen.GRUEN
        return sprache.t('hinweis_neue_version' if gruen else 'hinweis_versionen')

    def _status_setzen(self, quelle):
        """Die Statuszeile setzen — und sich merken, **woher** der Text kam.

        ⚠ Warum das eine eigene Methode ist: `_neu_beschriften()` setzt die
        Zeile beim Sprachwechsel aus `_status_quelle` neu. Schreibt irgendeine
        Stelle direkt ins Label, ohne die Quelle mitzuziehen, springt beim
        nächsten Sprachwechsel eine **ältere** Meldung zurück auf den Schirm.
        Deshalb geht jeder Schreibzugriff durch hier.

        Ein fertiger Text (kein `Satz`) ist erlaubt — dann friert die Zeile in
        ihrer Sprache ein, statt falsch zu werden. `None` als Quelle sagt genau
        das: „hier gibt es nichts aufzufrischen"."""
        self._status_quelle = quelle if sprache.auffrischbar(quelle) else None
        self.status.config(text=str(quelle))

    def _autostart_titel(self):
        """Wie der Schalter heißt — je nach System und **aktueller** Sprache."""
        return sprache.t('autostart_win' if pfade.WINDOWS
                         else 'autostart_linux')

    def _neu_beschriften(self):
        """Alle festen Texte der Melde-Leiste erneuern.

        ⚠ Wird beim Sprachwechsel gerufen (angemeldet über
        `sprache.anmelden`). Bis zum 26.08.2026 gab es das nicht: Das
        Einstellungsfenster stellte sich um, das Overlay blieb deutsch stehen.
        Wer die Sprache wechselt, sieht sonst zwei Sprachen nebeneinander —
        und hält es zu Recht für kaputt.

        Die Erklärblasen stehen hier nicht: Die holen ihren Text bei jedem
        Überfahren neu (`lambda: sprache.t(...)`) und sind damit von allein
        aktuell."""
        try:
            if getattr(self, '_ph', None) and self._ph.winfo_exists():
                self._ph.config(text=sprache.t('ov_warte'))
            # Die Statuszeile: Steht dort noch der Starttext, wird der
            # erneuert. Steht dort eine echte Meldung, wird sie **nicht**
            # weggewischt — sondern in der neuen Sprache neu gesetzt, sofern
            # sie ihren Schlüssel mitgebracht hat.
            quelle = getattr(self, '_status_quelle', None)
            if self.status.cget('text') == self._status_text:
                self._status_text = sprache.t('ov_starte')
                self.status.config(text=self._status_text)
            elif sprache.auffrischbar(quelle):
                self.status.config(text=str(quelle))
            # Und jede Hinweiszeile, die noch in der Liste steht. Gegangen wird
            # über die Widgets selbst: Was hinausgerollt ist, ist auch weg —
            # eine mitgeführte Liste müsste man dagegen aufräumen.
            for zeile in self.list.pack_slaves():
                for teil in zeile.winfo_children():
                    quelle = getattr(teil, '_quelle', None)
                    if sprache.auffrischbar(quelle):
                        teil.config(text=str(quelle))
        except Exception as ausnahme:
            fehler.merken('overlay._neu_beschriften', ausnahme)

    # ⚠ Der Autostart-Schalter ist am 27.08.2026 aus der Melde-Leiste
    # entfallen — mitsamt `_show_autostart` und `_toggle_autostart`. Zwei
    # Gründe: Ein Ein/Aus-Zeichen heißt überall „Gerät ausschalten", und es saß
    # direkt neben dem `✕`, das das Programm wirklich schließt — zwei Knöpfe,
    # die beide nach „aus" aussehen. Und es ist eine **Einstellung**, kein
    # Werkzeug; dort steht sie ohnehin (Reiter „Allgemein").

    # ⚠ `_dx`/`_dy` **hier** vorbelegen, nicht erst in `_drag_start`. Tk liefert
    # `<B1-Motion>` nicht immer nach einem `<Button-1>` auf demselben Fenster:
    # Wer den Knopf ausserhalb drückt und in das Overlay zieht, löst nur die
    # Bewegung aus — und `self._dx` gab es dann nicht. Ergebnis war jedes Mal
    # `AttributeError: 'Overlay' object has no attribute '_dx'`.
    #
    # Der Fehler stand am 25.08.2026 in Bomb20s Bericht (rc18) und am 27.08.2026
    # in im eigenen Lauf (rc69) — dazwischen nie behoben, weil er nichts kaputt macht,
    # was man sieht: Das Ziehen tut einmal nichts, und der Fehler landet
    # lautlos im Protokoll.
    _dx = 0
    _dy = 0

    def _drag_start(self, e): self._dx, self._dy = e.x, e.y
    def _drag_move(self, e):
        self.root.geometry(f'+{self.root.winfo_x()+e.x-self._dx}+{self.root.winfo_y()+e.y-self._dy}')
    def _mindestgroesse_setzen(self):
        """Das Overlay darf nicht schmaler werden als seine Symbolleiste.

        Gilt in beide Richtungen: Der Fenstermanager bekommt die Grenze über
        `minsize()`, und eine gespeicherte Groesse von frueher wird angehoben,
        falls sie darunter liegt. Sonst startet das Overlay in genau der Groesse
        wieder, in der die Symbole fehlten.
        """
        try:
            breite = self._mindestbreite()
            self.root.minsize(breite, 120)
            # ⚠⚠ **Nur eingreifen, wenn das Fenster wirklich schon steht.**
            # Beim Start meldet Tk für ein noch nicht angezeigtes Fenster die
            # Breite `1` — der Vergleich traf dann immer zu, und das Overlay
            # wurde auf die Mindestbreite gesetzt. Die gemerkte Größe aus dem
            # letzten Lauf war damit weg: „er startet bei mir immer mit der
            # kleinsten Größe" (30.08.2026). Eingebaut hatte das ausgerechnet
            # die Änderung, die die Symbolleiste retten sollte.
            #
            # `winfo_ismapped()` allein genügt nicht — auch ein gemapptes
            # Fenster meldet kurzzeitig 1. Deshalb beides.
            breit_jetzt = self.root.winfo_width()
            if (self.root.winfo_ismapped() and breit_jetzt > 1
                    and breit_jetzt < breite):
                self.root.geometry('%dx%d' % (
                    breite, max(120, self.root.winfo_height())))
        except Exception as ausnahme:
            fehler.merken('overlay.mindestgroesse', ausnahme)

    def _mindestbreite(self):
        """Wie schmal das Overlay hoechstens werden darf.

        ⚠ Nicht raten, sondern die Kopfleiste fragen. Wird das Fenster
        schmaler, verschwinden die Symbole rechts — und wer sie nicht sieht,
        sucht einen Fehler, den es nicht gibt. Genau so gemeldet am 29.08.2026:
        Glocke und die Symbole rechts waren weg.

        Der Zuschlag deckt Rahmen und Innenabstand. Findet sich keine
        Kopfleiste, bleibt es beim alten Wert.
        """
        try:
            kinder = self.kopf.winfo_children()
            if not kinder:
                return 260
            # ⚠ NICHT `self.kopf.winfo_reqwidth()` nehmen. Die Leiste läuft mit
            # `pack_propagate(False)` — sie gibt die Größe ihrer Kinder
            # absichtlich nicht weiter, damit die Höhe fest bleibt. Gefragt
            # meldet sie deshalb einen Fantasiewert, und die Mindestbreite war
            # wirkungslos: Am 29.08.2026 war im Overlay kein einziges Symbol
            # mehr zu sehen.
            #
            # Also die Kinder selbst zusammenzählen. Der Zuschlag je Element
            # deckt dessen seitlichen Abstand, die 20 am Ende den Rand und den
            # Anfasser zum Ziehen.
            noetig = sum(k.winfo_reqwidth() + 8 for k in kinder) + 20
        except Exception:
            return 260
        return max(260, noetig)

    # Welches Dreieck zu welcher Verankerung gehoert: `(unten, rechts)` → das
    # Zeichen, dessen Spitze in die FREIE Richtung zeigt — dorthin, wohin sich
    # das Fenster ziehen laesst.
    #
    # ⚠ Schriftzeichen statt Bildsymbol ist hier eine Altlast (die Projektregel
    # verlangt Symbole aus dem Satz). Solange es eines ist, bleiben wenigstens
    # alle vier aus derselben Zeichenfamilie — gleiche Groesse, gleiche
    # Strichstaerke. Ein Wechsel auf gedrehte Lucide-Symbole waere der saubere
    # Weg und ist als eigener Schritt vorgemerkt.
    GRIFF_SYMBOLE = {
        (False, False): 'ziehen_ur',   # frei → Griff unten rechts, zieht dorthin
        (False, True):  'ziehen_ul',   # rechts verankert → Griff links
        (True,  False): 'ziehen_or',   # unten verankert → Griff oben
        (True,  True):  'ziehen_ol',   # beides → nach oben links
    }

    def _verankert(self):
        """Welche Fensterkanten liegen fest — `(unten, rechts)`.

        In einer Ecke klebt das Overlay an zwei Raendern des Bildschirms. Diese
        beiden Kanten duerfen sich beim Ziehen NICHT bewegen; waechst das
        Fenster, muss es in die freie Richtung wachsen. Ohne gewaehlte Ecke
        (`frei`) gilt das Uebliche: oben und links liegen fest.
        """
        try:
            ecke = pfade.einstellung('overlay_ecke') or 'frei'
        except Exception:
            ecke = 'frei'
        return (ecke.startswith('unten'), ecke.endswith('rechts'))

    def _resize(self, e):
        """Das Fenster am Griff groesser ziehen — von der freien Ecke aus.

        ⚠⚠ **Diese Rechnung ging von „oben links steht fest" aus** und war
        damit falsch, sobald das Overlay in einer unteren oder rechten Ecke
        klebt: Gezogen wurde gegen den Bildschirmrand, an dem das Fenster
        haengt. Gemeldet am 02.09.2026 zu rc10: *„Fenstergroesse ist auch nicht
        mehr anpassbar, da ich sie nur nach unten ziehen koennte."*

        Jetzt bleibt jede verankerte Kante stehen, und die Groesse waechst in
        die Richtung, in die ueberhaupt Platz ist.
        """
        unten, rechts = self._verankert()
        x0, y0 = self.root.winfo_x(), self.root.winfo_y()
        rechte_kante = x0 + self.root.winfo_width()
        untere_kante = y0 + self.root.winfo_height()
        zeiger_x, zeiger_y = (self.root.winfo_pointerx(),
                              self.root.winfo_pointery())
        if rechts:
            breite = max(self._mindestbreite(), rechte_kante - zeiger_x)
            neu_x = rechte_kante - breite
        else:
            breite = max(self._mindestbreite(), zeiger_x - x0)
            neu_x = x0
        if unten:
            hoehe = max(160, untere_kante - zeiger_y)
            neu_y = untere_kante - hoehe
        else:
            hoehe = max(160, zeiger_y - y0)
            neu_y = y0
        self.root.geometry('%dx%d+%d+%d' % (breite, hoehe, neu_x, neu_y))

    # ---- Liste ----
    def _fit_width(self, e=None):
        """Listenbreite an die Fensterbreite koppeln und lange Untertitel neu umbrechen."""
        w = e.width if e is not None else self.canvas.winfo_width()
        if w < 2:
            return
        self.canvas.itemconfigure(self._list_id, width=w)
        self._wrap_labels = [lb for lb in self._wrap_labels if lb.winfo_exists()]
        for lb in self._wrap_labels:
            lb.config(wraplength=max(160, w - 40))

    def _placeholder(self):
        self._ph = tk.Label(self.list, text=sprache.t('ov_warte'),
                            bg=BG, fg=SUB, font=self.f_sub)
        self._ph.pack(anchor='w', padx=4, pady=6)

    def clear(self):
        for w in self.list.winfo_children():
            w.destroy()
        self.rows.clear()
        self.count = 0
        self._placeholder()

    @staticmethod
    def _sub_text(art, meta, ts, nachlese=False):
        parts = [p for p in (art, meta) if p]
        parts.append(ts)
        # ⚠ Nachgelesenes muss als solches erkennbar sein. Sonst sieht es aus
        # wie ein Fund von eben — und wer gerade nichts freigeschaltet hat,
        # fragt sich, woher der kommt.
        if nachlese:
            parts.append(sprache.t('nachlese_marke'))
        return ' · '.join(parts)

    def add_new(self, key, art, meta, ts, nachlese=False):
        if self.count == 0 and hasattr(self, '_ph') and self._ph.winfo_exists():
            self._ph.destroy()
        self.count += 1
        nk = _norm(key)
        top = self.list.pack_slaves()          # aktuell oberste Zeile (Reihenfolge im Fenster!)
        row = tk.Frame(self.list, bg=BG)
        row.pack(fill='x', anchor='w', padx=2, pady=1)
        # ⚠ **Ein Zustand, nicht zwei.** Bis v3.0.0-rc94 stand ein Fund aus der
        # Game.log **gelb** da — „vorläufig", bis die Launcher-Datei ihn
        # bestätigt. Diese Bestätigung kann es nicht mehr geben: Die Game.log
        # ist die Quelle, der Launcher nur noch eine Ergänzung. Übrig blieb ein
        # Zustand, aus dem nichts mehr herausführt — wer den Launcher hat, sah
        # dauerhaft Gelb, wer ihn nicht hat dauerhaft Grün, bei genau derselben
        # Sicherheit. Das ist keine Auskunft, das ist eine Sackgasse.
        #
        # Noch früher standen hier die Emoji `🟡`/`🟢`: Die nahmen unter Windows
        # die Farb-Emoji-Schrift, erschienen als bunte Klötzchen und ignorierten
        # jede eingestellte Farbe — vor jeder einzelnen Zeile.
        dot = zeichen.zeile(row, 'bestaetigt', farbe=zeichen.GRUEN,
                            grund=BG, schrift=self.f_item)
        dot.pack(side='left', padx=(0, 4))
        txt = tk.Frame(row, bg=BG); txt.pack(side='left', fill='x', expand=True)
        name = tk.Label(txt, text=key, bg=BG, fg=FG, font=self.f_item,
                        anchor='w', justify='left')
        name.pack(fill='x', anchor='w')
        sub = tk.Label(txt, text=self._sub_text(art, meta, ts, nachlese),
                       bg=BG, fg=SUB, font=self.f_sub, anchor='w')
        sub.pack(fill='x', anchor='w')
        row._bpkey = nk
        self.rows[nk] = {'frame': row, 'dot': dot, 'name': name, 'sub': sub, 'ts': ts}
        # neueste oben einsortieren. WICHTIG: pack_slaves() (= Reihenfolge im Fenster),
        # nicht winfo_children() (= Reihenfolge der Erzeugung) — sonst landen neue
        # Zeilen unter den älteren (Fehler bis v1.1.0).
        if top:
            row.pack_configure(before=top[0])
        self._trim()
        self.canvas.yview_moveto(0)
        signalton()
        # ⚠ Nicht mehr direkt `_popup_zeigen()`: Das half nur im
        # Aufblend-Betrieb. Ein eingeklapptes Overlay bei „Immer sichtbar"
        # meldete gar nichts — siehe `bei_fund_zeigen`.
        self.bei_fund_zeigen()

    def _liste_nachziehen(self):
        """Die Seiten im Hauptfenster auf den neuen Bestand bringen.

        ⚠⚠ **Gemeldet von Bushwick4712 am 05.09.2026.** Bis hierher meldete ein
        Fund sich nur im Overlay. Die Liste im Hauptfenster las ihren Bestand
        einmal beim Bauen und danach nie wieder — wer sie offen hatte, sah
        beim nächsten Bauplan weder die neue Anzahl noch den grünen Haken, und
        beim Wechseln auf eine andere Seite und zurück ebenso wenig.

        Erwartet wird das Gegenteil, und zwar von jedem: Bauplan fällt,
        Werkzeug meldet ihn, Liste stimmt. Sofort.

        Gerufen aus `_poll_queue` auf das Signal, das `_bestand_sichern()`
        absetzt — also erst, wenn der Fund wirklich auf der Platte steht.

        ⚠ Nur, wenn die Liste schon gebaut ist. Sie von hier aus zu erzeugen
        wäre falsch — der Aufbau dauert eine knappe Sekunde und gehört in den
        Moment, in dem jemand die Seite tatsächlich öffnet.
        """
        fenster = getattr(self, '_fenster', None)
        if fenster is None:
            return
        try:
            fenster.bestand_geaendert()
        except Exception as ausnahme:
            fehler.merken('oberflaeche.liste_nachziehen', ausnahme)

    def auftraege_zeigen(self, paare):
        """Die laufenden Auftraege setzen — die Leiste zeigt immer den Stand.

        `paare` ist eine Liste aus (Titel ohne Marken, fertige Zeile) — oder
        aus (Titel, Zeile, Zwischenziele). Der Titel dient nur als Schluessel
        fuers Wegklicken.

        ⚠⚠ **Die Zwischenziele stehen eingerueckt unter ihrem Auftrag.** Der
        Auftrag sagt, ob Bauplaene drin sind; das Ziel sagt, was gerade zu tun
        ist. Die Raute ist dieselbe, die das Spiel selbst neben seine Ziele
        setzt — und sie kommt aus dem festgelegten Satz (`zeichen.py`), nicht
        aus einem getippten Zeichen.

        ⚠ Die zwei Formen sind Absicht: Eine Anzeige, die nur Paare bekommt,
        muss weiter gehen. Der Selbsttest ruft sie so auf, und ein Aufruf ohne
        Ziele soll nicht mit einem Fehler enden, nur weil nichts anliegt.
        """
        for w in self.auftragsleiste.winfo_children():
            w.destroy()
        self._auftrag_zeilen = []

        if not paare:
            # Nichts offen? Dann nimmt die Leiste auch keinen Platz weg.
            self.auftragsleiste.pack_forget()
            return

        kopf = tk.Label(self.auftragsleiste, text=sprache.t('ov_auftraege_kopf'),
                        bg=BG, fg=SUB, font=self.f_sub, anchor='w')
        kopf.pack(fill='x')
        kopf._quelle = sprache.Satz('ov_auftraege_kopf')

        for eintrag in paare:
            rein, zeile = eintrag[0], eintrag[1]
            ziele = list(eintrag[2]) if len(eintrag) > 2 and eintrag[2] else []
            z = tk.Frame(self.auftragsleiste, bg=BG)
            z.pack(fill='x')
            lbl = tk.Label(z, text=str(zeile), bg=BG, fg=FG, font=self.f_sub,
                           anchor='w', justify='left')
            if sprache.auffrischbar(zeile):
                lbl._quelle = zeile
            lbl.pack(side='left', fill='x', expand=True, anchor='w')
            # ⚠ In die Umbruchliste. Ohne das steht die Zeile in einer festen
            # Breite und wird am Fensterrand abgeschnitten — auf einem schmalen
            # Overlay endete sie mitten in „dir fehlt: H".
            self._wrap_labels.append(lbl)
            # Zum Ausblenden. Ein Auftrag kann im Spiel verloren gehen, ohne
            # dass das Log ein Wort darüber verliert — dann nimmt man ihn hier
            # selbst heraus.
            # ⚠ Das Symbol kommt aus dem festgelegten Satz (`zeichen.py`,
            # Lucide `ban`) — nichts wird hier selbst gemalt. Genau das war der
            # Grund der Umstellung auf Bilder: gemalte und getippte Zeichen
            # sahen unterschiedlich aus und auf jedem System wieder anders.
            weg = zeichen.zeile(z, 'ausblenden', farbe=zeichen.ROT, grund=BG,
                                schrift=self.f_sub)
            weg.pack(side='right', padx=(8, 2))
            weg.bind('<Button-1>', lambda _e, r=rein: self._auftrag_ausblenden(r))
            hinweis.anhaengen(weg, lambda: sprache.t('ov_auftrag_weg'))
            self._auftrag_zeilen.append(lbl)
            self._ziele_zeigen(ziele)

        # ⚠ Welche Auftraege gerade in der Leiste stehen — `add_hinweis`
        # fragt danach, um denselben Text nicht ein zweites Mal darunter zu
        # setzen.
        self._auftrag_schluessel = {e[0] for e in (paare or [])}

        # ⚠ Vor der Liste einordnen, sonst rutscht die Leiste ans Fensterende.
        self.auftragsleiste.pack(fill='x', padx=8, pady=(0, 2),
                                 before=self._listen_traeger)

    def _ziele_zeigen(self, ziele):
        """Die Zwischenziele eines Auftrags — eingerueckt, eine Zeile je Ziel.

        ⚠ Gedeckelt. Gemessen hat ein Auftrag fast immer **ein** offenes Ziel,
        der Ausreisser hatte sechs; die Grenze faengt nur den unbekannten Fall
        ab. Was nicht mehr passt, wird gezaehlt statt verschwiegen — eine
        abgeschnittene Liste, die sich fuer vollstaendig ausgibt, waere
        schlimmer als gar keine.
        """
        for name in ziele[:auftraege.ZIELE_MAX]:
            zz = tk.Frame(self.auftragsleiste, bg=BG)
            zz.pack(fill='x', padx=(14, 0))
            raute = zeichen.zeile(zz, 'standard', farbe=zeichen.GRAU, grund=BG,
                                  schrift=self.f_sub)
            raute.pack(side='left', padx=(0, 5))
            zl = tk.Label(zz, text=str(name), bg=BG, fg=SUB, font=self.f_sub,
                          anchor='w', justify='left')
            zl.pack(side='left', fill='x', expand=True, anchor='w')
            self._wrap_labels.append(zl)
        rest = len(ziele) - auftraege.ZIELE_MAX
        if rest > 0:
            mehr = tk.Label(self.auftragsleiste,
                            text=sprache.t('ov_ziele_mehr', rest),
                            bg=BG, fg=SUB, font=self.f_sub, anchor='w')
            mehr.pack(fill='x', padx=(14, 0))
            mehr._quelle = sprache.Satz('ov_ziele_mehr', rest)

    def _auftrag_ausblenden(self, rein):
        """Der Spieler nimmt einen Auftrag selbst aus der Anzeige."""
        try:
            self.watcher.auftrag_wegklicken(rein)
        except Exception as ausnahme:
            fehler.merken('fenster.auftrag_weg', ausnahme)

    def hinweis_entfernen(self, auftrag):
        """Die Zeile zu einem Auftrag aus der Liste nehmen.

        Aufgerufen, wenn das Spiel den Auftrag beendet meldet oder der Spieler
        ihn selbst wegnimmt. ⚠ Ohne das bliebe eine Zeile stehen, die behauptet,
        der Auftrag laufe noch — und weil sie in der Liste steht und nicht in
        der Auftragsleiste, trug sie bis v3.3.0-rc29 nicht einmal ein Zeichen
        zum Wegklicken.
        """
        for row in list(self.list.pack_slaves()):
            if getattr(row, '_auftrag', None) != auftrag:
                continue
            for w in row.winfo_children():
                if w in self._wrap_labels:
                    self._wrap_labels.remove(w)
            self.rows.pop(getattr(row, '_bpkey', None), None)
            row.destroy()
            self.count -= 1

    def add_hinweis(self, text, auftrag=None):
        """Eine Zeile, die keine Freischaltung meldet, sondern etwas erklärt —
        etwa „im Bestand fehlt möglicherweise etwas" oder ein angenommener
        Auftrag mit Bauplänen darin.

        Kein Signalton, kein Ausrufezeichen: Es ist eine Information beim Start,
        keine Neuigkeit aus dem Spiel.

        `auftrag` ist der Auftragsschlüssel, falls die Zeile zu einem laufenden
        Auftrag gehört. Dann bekommt sie dasselbe rote Zeichen wie die
        Auftragsleiste und verschwindet, sobald der Auftrag endet.
        """
        # ⚠⚠ **Nicht zweimal dasselbe.** Steht der Auftrag schon in der
        # Auftragsleiste, sagt diese Zeile wortgleich dasselbe noch einmal —
        # direkt darunter. Am 31.08.2026 mit Bildschirmfoto gemeldet: „wieso
        # sehe ich ne quest jetzt 2 mal". Die Leiste zeigt den Zustand; sie
        # gewinnt. Ohne Leiste — oder nachdem der Auftrag dort weggeklickt
        # wurde — erscheint der Hinweis weiterhin.
        if auftrag and auftrag in getattr(self, '_auftrag_schluessel', ()):
            return
        if self.count == 0 and hasattr(self, '_ph') and self._ph.winfo_exists():
            self._ph.destroy()
        self.count += 1
        top = self.list.pack_slaves()
        row = tk.Frame(self.list, bg=BG)
        row.pack(fill='x', anchor='w', padx=2, pady=1)
        zeichen.zeile(row, 'hinweiszeile', grund=BG,
                      schrift=self.f_item).pack(side='left', padx=(0, 4))
        lbl = tk.Label(row, text=str(text), bg=BG, fg=SUB, font=self.f_sub,
                       anchor='w', justify='left')
        # ⚠ Der Träger bleibt am Label hängen. Hinweise stehen in der Liste,
        # bis sie hinausrollen — ohne das hier wäre eine Meldung von vorhin für
        # immer in der Sprache von vorhin. Gefunden am 26.08.2026: englisches
        # Fenster, deutsche Zeile „Keine Log-Sicherungen gefunden".
        # Gemerkt wird am Widget selbst, nicht in einer eigenen Liste — sonst
        # bleiben beim Hinausrollen (`_trim`) Leichen zurück.
        if sprache.auffrischbar(text):
            lbl._quelle = text
        lbl.pack(side='left', fill='x', expand=True, anchor='w')
        self._wrap_labels.append(lbl)
        row._bpkey = None
        row._auftrag = auftrag
        if auftrag:
            # Dasselbe Zeichen wie in der Auftragsleiste (`zeichen.py`, Lucide
            # `ban`) — nichts wird hier selbst gemalt.
            weg = zeichen.zeile(row, 'ausblenden', farbe=zeichen.ROT, grund=BG,
                                schrift=self.f_sub)
            weg.pack(side='right', padx=(8, 2))
            weg.bind('<Button-1>', lambda _e, r=auftrag: self._auftrag_ausblenden(r))
            hinweis.anhaengen(weg, lambda: sprache.t('ov_auftrag_weg'))
        self._fit_width()
        if top:
            row.pack_configure(before=top[0])
        self._trim()
        self.canvas.yview_moveto(0)

    def add_catalog(self, name, art, ts, titel):
        """Katalog-Zuwachs: im Spiel ist etwas NEU craftbar (nicht: selbst freigeschaltet).
        `titel` gesetzt = Treffer aus der Beobachtungsliste → auffällig in Gold."""
        if self.count == 0 and hasattr(self, '_ph') and self._ph.winfo_exists():
            self._ph.destroy()
        self.count += 1
        top = self.list.pack_slaves()
        row = tk.Frame(self.list, bg=BG)
        row.pack(fill='x', anchor='w', padx=2, pady=1)
        zeichen.zeile(row, 'gemerkt' if titel else 'punkt', grund=BG,
                      farbe=zeichen.GELB if titel else zeichen.BLAU,
                      schrift=self.f_item).pack(side='left', padx=(0, 4))
        txt = tk.Frame(row, bg=BG); txt.pack(side='left', fill='x', expand=True)
        tk.Label(txt, text=name, bg=BG, fg=PROV if titel else FG, font=self.f_item,
                 anchor='w', justify='left').pack(fill='x', anchor='w')
        # ⚠ Über `sprache.t`, nicht fest: Beide Schlüssel gab es längst
        # (`jetzt_craftbar`, `neu_craftbar`), benutzt hat sie niemand — die
        # Zeile blieb dadurch auch auf Englisch deutsch.
        unten = (sprache.t('jetzt_craftbar', titel) if titel
                 else sprache.t('neu_craftbar'))
        # Titel aus der Beobachtungsliste können lang sein -> umbrechen statt abschneiden
        sub = tk.Label(txt, text=' · '.join(x for x in (unten, art, ts) if x), bg=BG,
                       fg=PROV if titel else CATA, font=self.f_sub, anchor='w', justify='left')
        sub.pack(fill='x', anchor='w')
        self._wrap_labels.append(sub)
        self._fit_width()
        row._bpkey = None                      # kein BP-Schlüssel: nie „bestätigen"
        if top:
            row.pack_configure(before=top[0])
        self._trim()
        self.canvas.yview_moveto(0)
        signalton(auffaellig=bool(titel))

    def _trim(self):
        """Nur so viele Zeilen behalten wie eingestellt — älteste fliegen raus."""
        rows = self.list.pack_slaves()
        grenze = max_zeilen()
        while len(rows) > grenze:
            old = rows.pop()
            self.rows.pop(getattr(old, '_bpkey', None), None)
            old.destroy()
            self.count -= 1

    # ---- Queue vom Watcher abarbeiten ----
    def _poll_queue(self):
        try:
            while True:
                msg = self.q.get_nowait()
                if msg[0] == 'status':
                    # ⚠ Die Quelle merken, nicht nur den fertigen Text: Kommt
                    # der als `sprache.Satz`, lässt sich die Zeile beim
                    # Sprachwechsel neu auswerten statt in der Sprache von
                    # damals stehen zu bleiben.
                    self._status_setzen(msg[1])
                elif msg[0] == 'hinweis':
                    # Bleibt stehen, bis die nächste Statusmeldung kommt, und
                    # wird farblich abgesetzt — eine Lücke im Bestand soll
                    # auffallen, aber kein Fenster aufreißen.
                    self.add_hinweis(*msg[1:])
                elif msg[0] == 'bescheid':
                    self._bescheid_zeigen(msg[1], msg[2])
                elif msg[0] == 'auftraege':
                    self.auftraege_zeigen(msg[1])
                elif msg[0] == 'auftrag_weg':
                    self.hinweis_entfernen(msg[1])
                elif msg[0] == 'new':
                    self.add_new(*msg[1:])
                elif msg[0] == 'liste_frisch':
                    self._liste_nachziehen()
                elif msg[0] == 'catalog':
                    self.add_catalog(msg[1], msg[2], msg[3], msg[4])
        except queue.Empty:
            pass
        self._hotkey_nachsehen()
        self.root.after(300, self._poll_queue)

    def _bescheid_zeigen(self, titel, text):
        """Ein Ergebnis, das nicht uebersehen werden darf.

        ⚠⚠ **Die Fusszeile reicht nicht.** Sie steht vier Sekunden und ist dann
        leer — und genau in diesen vier Sekunden sieht niemand hin, der gerade
        einen Lauf ueber hunderte Protokolle angestossen hat. Gemeldet am
        31.08.2026: „in der Leiste steht es zu kurz oder gar nicht."

        ⚠ **Die Leiste bekommt es trotzdem.** Ist das Hauptfenster zu — der
        Knopf gibt es auch am Overlay —, gibt es kein Fenster, ueber dem ein
        Dialog stehen koennte. Dann bleibt die Zeile der Weg, und sie ist
        besser als nichts. Verschluckt wird das Ergebnis nie.
        """
        self._status_setzen(text)
        fenster = getattr(self, '_fenster', None)
        if fenster is None:
            return
        try:
            from scbp.hauptfenster import bescheid_geben, nach_vorn
            nach_vorn(fenster.root)
            bescheid_geben(fenster.root, str(titel), str(text))
        except Exception as ausnahme:
            fehler.merken('oberflaeche.bescheid', ausnahme)

    def versionen_zeigen(self):
        """Das Fenster „Was ist neu" öffnen."""
        from scbp.versionsfenster import Versionsfenster
        vorhanden = getattr(self, '_versionen', None)
        if vorhanden is not None:
            try:
                from scbp.hauptfenster import nach_vorn
                nach_vorn(vorhanden.root)
                return
            except Exception:
                pass
        self._versionen = Versionsfenster(
            self.root, eigene_version=__version__,
            beim_schliessen=lambda: setattr(self, '_versionen', None))

    # Wie oft nach einer neuen Fassung gesehen wird, in Millisekunden.
    # Dasselbe Maß wie `aktualisierung.ABSTAND` (eine Stunde) — die Abfrage
    # selbst hat ihren eigenen Zwischenspeicher, hier geht es nur darum, dass
    # überhaupt jemand fragt.
    VERSION_TAKT = 3600 * 1000

    def _nach_version_sehen(self):
        """Im Hintergrund nachsehen, ob es etwas Neues gibt.

        Im Nebenläufer, damit der Start nicht auf das Netz wartet — und still,
        wenn nichts da ist. Ein Werkzeug, das beim Spielen im Vordergrund liegt,
        soll nicht ungefragt Fenster aufreißen; der Knopf färbt sich, mehr nicht.

        ⚠ **Und danach wieder, jede Stunde.** Bis v3.0.1 lief das hier **genau
        einmal**, zwei Sekunden nach dem Start. Der Stundenabstand in
        `aktualisierung.nachsehen()` lief damit ins Leere — er begrenzt, wie oft
        gefragt werden *darf*, aber fragen musste jemand. Wer den Watcher
        durchlaufen ließ, erfuhr nie von einer neuen Fassung; sie erschien erst
        nach einem Neustart. Gemeldet am 28.08.2026, als v3.0.1
        draußen war und der laufende Watcher weiter schwieg — obwohl er die neue
        Fassung längst abgerufen hatte und sie in seinem Zwischenspeicher stand.
        """
        # Erst den nächsten Blick einplanen, dann arbeiten: Wirft das Nachsehen,
        # hört die Reihe sonst still auf.
        try:
            self.root.after(self.VERSION_TAKT, self._nach_version_sehen)
        except tk.TclError:
            return                       # Fenster ist zu, dann reicht es auch

        def arbeit():
            try:
                neu = aktualisierung.nachsehen(__version__)
            except Exception:
                return
            if neu:
                self.root.after(0, lambda: self._version_melden(neu))
        threading.Thread(target=arbeit, daemon=True).start()

    def _version_melden(self, neu):
        try:
            self.info_lbl.faerben(zeichen.GRUEN)
            # ⚠ Zwei eigenständige Sätze in einer Zeile — als `Kette`, damit
            # auch diese Meldung beim Sprachwechsel mitzieht. Das Trennzeichen
            # ist Satzzeichen, kein Text, und braucht deshalb keinen Schlüssel.
            self.q.put(('hinweis', sprache.verbinden(
                ' — ', sprache.Satz('neue_version_da', neu['version']),
                sprache.Satz('was_ist_neu'))))
        except Exception:
            pass

    def _hinweis_klappen(self):
        return sprache.t('hinweis_ausklappen' if self.eingeklappt
                         else 'hinweis_einklappen')

    def umklappen(self):
        """Auf die Titelleiste zusammenschieben — oder wieder aufmachen."""
        self.klappzustand_setzen(not self.eingeklappt)

    def klappzustand_setzen(self, zu, merken=True):
        """Den Klappzustand **herstellen** — nicht umschalten.

        ⚠ Der Unterschied zählt. Beim Programmstart wurde bisher `umklappen()`
        aufgerufen, also ein Umschalter, während das Fenster noch aufgebaut
        wurde. Das Ergebnis hing davon ab, was Tk zu diesem Zeitpunkt schon
        wusste: Der Ziehgriff blieb sichtbar und deckte das ✕ zu, bis man einmal
        von Hand auf- und wieder zuklappte. Wer einen Zustand will, soll ihn
        setzen und nicht auf das Gegenteil des gerade Vermuteten schalten.

        Gemerkt wird die Höhe **vor** dem Einklappen, nicht eine feste Zahl: Wer
        sich das Fenster auf 900 Pixel gezogen hat, will es beim Aufklappen auch
        wieder so haben.
        """
        try:
            leiste = self.root.winfo_children()[0]
            leistenhoehe = max(leiste.winfo_height(), 26)
            if not zu:
                # ⚠ Mindesthöhe erzwingen. Stand in `hoehe_offen` versehentlich
                # die Leistenhöhe, klappte das Fenster auf seine eigene Größe
                # „auf" — der Knopf schaltete um, sichtbar passierte nichts, und
                # das Overlay ließ sich nie wieder öffnen.
                hoehe = max(self.hoehe_offen or 0, leistenhoehe + 120)
                breite = max(self.breite_offen or 0, self._leisten_breite())
            else:
                # ⚠ Die offene Höhe nur merken, wenn das Fenster **wirklich**
                # offen ist. Laufen Zustand und Geometrie einmal auseinander
                # (auf dem Mac genügen zwei schnelle Klicks — Tk kennt die neue
                # Größe erst nach einem Durchlauf der Ereignisschleife), würde
                # hier sonst die Leistenhöhe als „offen" festgeschrieben. Ab da
                # ist das Overlay dauerhaft zu.
                aktuell = self.root.winfo_height()
                if aktuell > leistenhoehe + 40:
                    self.hoehe_offen = aktuell
                    # ⚠ Nur zusammen mit der Hoehe merken: Ist das Fenster
                    # schon eingeklappt, stuende hier die Streifenbreite als
                    # „offen" — und das Overlay bliebe fuer immer schmal.
                    self.breite_offen = self.root.winfo_width()
                breite = self._leisten_breite()
                # Die Höhe der Titelleiste, nicht geraten: Ein fester Wert säße
                # bei anderer Schriftgröße daneben.
                hoehe = leistenhoehe

            # ⚠⚠ **Die Mindestgroesse muss mit.** `_mindestgroesse_setzen()`
            # haelt das Fenster auf 520x120, damit im offenen Zustand keine
            # Symbole abgeschnitten werden. Beim Einklappen wirkt genau diese
            # Grenze gegen uns: `geometry('260x26')` wird gesetzt, das Fenster
            # bleibt aber 520x120 — und die Ecke ist fuer die kleinere Groesse
            # gerechnet. In einer rechten Ecke standen dadurch 252 px, in einer
            # unteren 86 px ausserhalb des Bildschirms; sichtbar blieb ein
            # gruener Strich. Gemeldet von Haldjas (pr0) am 01./02.09.2026.
            #
            # Gemessen statt geraten: `tools/entwurf_ecken_messen.py` baut das
            # Fenster unter Xvfb und vergleicht die **tatsaechliche** Geometrie
            # mit dem Bildschirmrand.
            if zu:
                self.root.minsize(breite, hoehe)
            else:
                self.root.minsize(self._mindestbreite(), 120)
            # ⚠ Die Seite der Leiste VOR der Geometrie festlegen: Umpacken
            # laesst Tk neu rechnen, und was danach gesetzt wird, gilt.
            self._leiste_ausrichten()
            x, y = self._klapp_ecke(breite, hoehe)
            self.root.geometry('%dx%d+%d+%d' % (breite, hoehe, x, y))
            self.klapp_lbl.symbol_tauschen('aufklappen' if zu
                                          else 'einklappen')
            self.eingeklappt = zu
            self._grip_nachziehen()
            # ⚠ Das Schloss ist ein EIGENES Fenster und wandert nicht von
            # allein mit. Beim Klappen ändert sich Höhe UND Lage (in einer
            # unteren Ecke rutscht das Fenster nach oben, weil es kürzer
            # wird) — ohne diese Zeile bleibt das Schloss stehen, wo das
            # Overlay vorher war. Gemeldet von Haldjas (pr0) am 01.09.2026:
            # „das Overlay wird zwar in die jeweiligen Ecken verschoben, aber
            # der Grüne Balken im eingeklappten Zustand sitzt weiter am
            # selben Ort."
            self._schloss_nachziehen()
            # ⚠⚠ **Und der Streifen genauso — er ist das dritte eigene Fenster.**
            #
            # Dieselbe Zeile steht in `ecke_anwenden()`, hier fehlte sie. Der
            # Unterschied fiel nicht auf, weil beide Wege dasselbe tun sollen,
            # aber verschieden gerufen werden: `ecke_anwenden` beim Wechsel auf
            # der Einstellungsseite, DIESE Methode beim **Programmstart**
            # (`verhalten_anwenden` ruft sie per `after(120, …)`).
            #
            # Beim Start wird `_letzte_lage` vorher aus der gespeicherten Lage
            # gesetzt und der Streifen sofort dorthin gezeichnet. Wanderte das
            # Fenster danach in die Ecke, blieb er stehen — und mit ihm das
            # Schloss, das aus derselben Lage rechnet.
            #
            # Gemeldet von Haldjas (pr0) am 02.09.2026: „Overlay war auf links
            # unten eingestellt, balken war rechts unten und hat den watcher
            # aber links unten geöffnet." Nicht reproduzierbar war es, weil der
            # Fall nur beim ERSTEN Start nach einem Eckenwechsel eintritt:
            # Danach ist die gespeicherte Lage die Ecke selbst, und es stimmt
            # wieder. Im Protokoll stand nichts, weil nichts scheiterte.
            if self.anzeigeart == 'popup':
                self._letzte_lage = '%dx%d+%d+%d' % (breite, hoehe, x, y)
                self._anfasser_zeigen()
            if merken:
                pfade.einstellung_setzen('eingeklappt', zu)
        except tk.TclError:
            pass

    ECKEN = ('frei', 'oben-links', 'oben-rechts', 'unten-links', 'unten-rechts')

    def _leisten_breite(self):
        """Wie breit der eingeklappte Streifen sein muss — gemessen, nicht geraten.

        ⚠⚠ Ein fester Wert saesse bei anderer Schriftgroesse und in der anderen
        Sprache daneben: „SC BP Watcher" ist kuerzer als sein englisches
        Gegenstueck, und die Symbolreihe waechst mit der Schrift mit. Also
        fragen wir die Leiste selbst, was sie braucht.

        ⚠⚠ **NICHT `winfo_reqwidth()` der Leiste nehmen** — genau daran ist
        diese Methode bis zum 02.09.2026 gescheitert. Die Kopfleiste laeuft
        mit `pack_propagate(False)` und meldet deshalb `1`; die Rechnung fiel
        jedes Mal auf den Mindestwert 260 zurueck, waehrend die Leiste in
        Wirklichkeit 520 breit ist. In einer rechten Ecke wurde die Position
        fuer 260 gerechnet und das Fenster stand mit 252 px ausserhalb des
        Bildschirms. Dieselbe Falle war in `_mindestbreite()` laengst behoben —
        also wird sie hier benutzt statt ein zweites Mal hineinzutappen.
        """
        try:
            self.root.update_idletasks()
            return max(self._mindestbreite(), 260)
        except Exception:
            return max(self.root.winfo_width(), 260)

    def _leiste_ausrichten(self):
        """Die Titelleiste an den Fensterrand haengen, der zur Ecke passt.

        ⚠⚠ Gemeldet von Haldjas (pr0) am 02.09.2026, nachdem ein erster Versuch
        am eigentlichen Punkt vorbeiging: *„Der Balken und das Schloss sind, der
        Einstellung nach, mit in die Ecken gesprungen, aber eben weiterhin am
        oberen Rand vom Watcher, was bedeutet, dass sie entsprechend weiter oben
        im Bild sitzen, wenn man den watcher unten platziert hat."*

        Das Fenster wanderte also richtig — die Leiste war nur fest oben
        verankert (`side='top'`). Bei einer unteren Ecke sass sie damit eine
        ganze Fensterhoehe ueber dem Bildschirmrand. Hier wechselt sie die
        Seite: oben bleibt oben, unten wandert nach unten.

        ⚠⚠ **Warum das vier Anlaeufe gekostet hat — und woran es WIRKLICH lag.**
        Am 02.09.2026 wurde dieser Umbau zurueckgenommen, mit der Begruendung,
        Tk rechne beim Umpacken die Fenstergroesse neu: Ein eingeklapptes
        Fenster wuchs „von 22 auf 120 px und ragte 86 px unter den
        Bildschirmrand". Beide Zahlen waren der Schluessel, nur hat sie damals
        niemand gelesen: **120 ist die Mindesthoehe** aus
        `_mindestgroesse_setzen()`, und **86 der Ueberstand**, den dieselbe
        Mindestgroesse in einer unteren Ecke erzeugte.

        Es lag also nie am Umpacken. Es lag daran, dass `minsize` beim
        Einklappen stehenblieb — derselbe Fehler, der das eingeklappte Overlay
        in drei von vier Ecken aus dem Bild geschoben hat. Seit er behoben ist,
        bleibt das Fenster beim Umpacken exakt so gross, wie es war (gemessen
        mit `tools/entwurf_leiste_unten.py`: 520x26 vorher wie nachher).

        Die Lehre steht ueber der Technik: Vier Anlaeufe haben ein Symptom
        bekaempft, das eine Ursache zwei Funktionen weiter hatte. Die Zahlen
        aus der ersten Messung hatten sie benannt.
        """
        try:
            ecke = pfade.einstellung('overlay_ecke') or 'frei'
        except Exception:
            ecke = 'frei'
        seite = 'bottom' if ecke.startswith('unten') else 'top'
        # ⚠ Nur anfassen, wenn sich wirklich etwas aendert. Ein Umpacken bei
        # jedem Klappen liesse die Oberflaeche sichtbar zucken.
        if seite == getattr(self, '_leiste_seite', 'top'):
            return
        try:
            teile = []
            for kind in self.root.pack_slaves():
                teile.append((kind, dict(kind.pack_info())))
            if not teile:
                return
            leiste = teile[0][0]
            for kind, _info in teile:
                kind.pack_forget()
            # ⚠ Die Leiste ZUERST packen — in Tk bekommt das zuerst gepackte
            # Bauteil seinen Rand zuerst. Kaeme sie nach der rollenden Flaeche
            # (`expand=True`), draengte diese sie aus dem Fenster; genau die
            # Falle, die in den Projektregeln unter „Fensteraufbau" steht.
            for kind, info in teile:
                info.pop('in', None)
                if kind is leiste:
                    info['side'] = seite
                kind.pack(**info)
            self._leiste_seite = seite
        except (tk.TclError, IndexError) as ausnahme:
            fehler.merken('overlay.leiste_ausrichten', ausnahme)

    def _klapp_ecke(self, breite, hoehe):
        """Wohin das Fenster gehoert — Ecke oder da, wo es steht.

        ⚠⚠ **Ziehen geht im Pop-up-Betrieb nicht.** Das Overlay ist dort
        durchklickbar, damit es im Kampf nicht stoert — und was Mausklicks
        durchreicht, laesst sich auch nicht anfassen. Ohne eine waehlbare Ecke
        gibt es fuer diese Nutzer **gar keinen** Weg, das Overlay zu
        positionieren. Am 31.08.2026 gemeldet.

        ⚠ Gerechnet wird auf dem Schirm, auf dem das Fenster GERADE steht —
        nicht auf dem ersten. Bei drei Monitoren nebeneinander waere „oben
        rechts" sonst immer der linke Bildschirm.
        """
        x, y = self.root.winfo_x(), self.root.winfo_y()
        try:
            ecke = pfade.einstellung('overlay_ecke') or 'frei'
            if ecke not in self.ECKEN or ecke == 'frei':
                return x, y
            # ⚠⚠ **Arbeitsflaeche, nicht die volle Bildschirmflaeche.** Unten
            # liegt unter Windows die Taskleiste. Wurde das Overlay an den
            # echten Bildschirmrand gesetzt, verschwand der 5 px hohe
            # Anfasser-Streifen dahinter: „hovern geht nicht mehr, nur ein
            # Klick auf eine bestimmte Stelle klappt ihn aus" (Haldjas, pr0,
            # 02.09.2026) — getroffen wurde das Stueck, das oberhalb der
            # Leiste herausschaute. `arbeitsflaeche()` faellt auf die volle
            # Flaeche zurueck, wenn das System keine Angabe liefert.
            sx, sy, sb, sh = bildschirm.arbeitsflaeche(self.root, x, y)
            rand = 8
            x = sx + rand if ecke.endswith('links') else sx + sb - breite - rand
            y = sy + rand if ecke.startswith('oben') else sy + sh - hoehe - rand
            # ⚠ Ist das Overlay so gross wie der Bildschirm, zieht der Rand von
            # 8 px es ueber die obere bzw. linke Kante — und dort sitzen
            # Titelleiste und Schloss, also genau die Bedienung. Lieber den
            # Rand aufgeben als den Zugriff. Geklemmt wird auf **diesen**
            # Schirm (`sx`/`sy`), nicht auf null: Bei mehreren Monitoren ist
            # ein negatives Y eine gueltige Angabe.
            x = max(sx, x)
            y = max(sy, y)
            return int(x), int(y)
        except Exception as ausnahme:
            fehler.merken('overlay.klapp_ecke', ausnahme)
            return x, y

    def ecke_anwenden(self):
        """Von der Einstellungsseite gerufen: die Ecke sofort uebernehmen."""
        try:
            # ⚠ Zuerst die Leiste an die passende Seite haengen — bei einer
            # unteren Ecke gehoert sie nach unten. Danach erst messen und
            # setzen, sonst gilt die Groesse von vor dem Umpacken.
            self._leiste_ausrichten()
            self.root.update_idletasks()
            b, h = self.root.winfo_width(), self.root.winfo_height()
            x, y = self._klapp_ecke(b, h)
            self.root.geometry('%dx%d+%d+%d' % (b, h, x, y))
            # ⚠ Ohne diese Zeile bleibt das Schloss in der ALTEN Ecke stehen,
            # waehrend das Overlay in die neue wandert. Es ist ein eigenes
            # Fenster (siehe `_schloss_anwenden`) und folgt nicht von selbst.
            self._schloss_nachziehen()
            self._save_geo()
            # ⚠⚠ **Im Aufblend-Betrieb ist das Overlay versteckt** — sichtbar
            # ist dann nur der gruene Streifen, und der wandert hier nicht von
            # allein mit. Er rechnet aus `_letzte_lage`, also muss die zuerst
            # auf die neue Ecke gebracht werden; sonst zeigt er weiter auf die
            # alte Stelle, bis das Overlay einmal auf- und zugeblendet hat.
            #
            # Gemeldet am 02.09.2026 zu rc11: „wenn ich die Fensterposition von
            # oben links nach rechts wechsle im Nur-bei-Neuzugang-Modus, muss
            # ich erst mit der Maus ueber den Strich fahren, eh es die Position
            # wechselt." Gilt fuer alle vier Ecken.
            #
            # ⚠ Und zwar mit den eben BERECHNETEN Werten, nicht mit
            # `_current_geom()`: Tk uebernimmt eine frisch gesetzte Geometrie
            # erst im naechsten Durchlauf der Ereignisschleife, gefragt kommt
            # also noch die alte zurueck. Der Streifen sass dann eine Ecke
            # hinterher — beim Wechsel auf „oben rechts" landete er mittig,
            # weil er die neue Richtung auf die alte Lage rechnete.
            if self.anzeigeart == 'popup':
                self._letzte_lage = '%dx%d+%d+%d' % (b, h, x, y)
                self._anfasser_zeigen()
        except Exception as ausnahme:
            fehler.merken('overlay.ecke_anwenden', ausnahme)

    def _grip_nachziehen(self, _e=None):
        """Den Ziehgriff zeigen oder verstecken, je nach Klappzustand.

        ⚠ Er sitzt unten rechts — bei einem auf Leistenhöhe geschrumpften
        Fenster ist das dieselbe Stelle wie oben rechts, und er legt sich über
        das ✕. Man muss dann zielen, um das Werkzeug überhaupt schließen zu
        können. Ein 26 Pixel hohes Fenster in der Höhe zu ziehen ergibt ohnehin
        keinen Sinn.

        Bewusst eine eigene Methode: Sie wird auch beim ersten Anzeigen des
        Fensters aufgerufen, damit der Zustand von Anfang an stimmt und nicht
        erst nach dem ersten Umschalten.
        """
        try:
            soll_sichtbar = not self.eingeklappt
            # ⚠ `winfo_ismapped()` taugt hier NICHT. Solange das Fenster noch
            # nicht angezeigt wird, meldet Tk für jedes Kind `False` — die
            # Prüfung hielt den Griff dann für versteckt, kehrte zurück, und er
            # erschien danach trotzdem, weil er per `place` verwaltet blieb.
            # Gefragt werden muss nach der **Platzierung**, nicht nach der
            # Sichtbarkeit.
            ist_platziert = bool(self.grip.place_info())
            # ⚠⚠ **Der Griff gehoert an die FREIE Ecke des Fensters** — die,
            # die von den verankerten Bildschirmraendern wegzeigt. Nur von dort
            # laesst sich in eine Richtung ziehen, in der ueberhaupt Platz ist.
            #
            # Zwei Fliegen mit einer Klappe: Bei unten haengender Leiste liegt
            # „unten rechts" mitten auf ihren Symbolen, der Griff deckte dort
            # das ✕ zu. Die freie Ecke ist immer auch die leistenfreie.
            #
            # Gemeldet am 02.09.2026 zu rc10: „Fenstergroesse ist auch nicht
            # mehr anpassbar, da ich sie nur nach unten ziehen koennte."
            unten, rechts = self._verankert()
            lage = dict(relx=0.0 if rechts else 1.0,
                        rely=0.0 if unten else 1.0,
                        anchor=('n' if unten else 's')
                               + ('w' if rechts else 'e'))
            # ⚠ **Das Dreieck muss dorthin zeigen, wohin man zieht.** Es stand
            # fest auf ◢ (unten rechts) und wies damit in drei von vier Ecken
            # gegen den Bildschirmrand — also genau in die Richtung, in der
            # kein Platz ist. Ein Griff, der in die Irre zeigt, ist schlechter
            # als gar keiner. Gemeldet am 02.09.2026 zu rc11.
            self.grip.symbol_tauschen(self.GRIFF_SYMBOLE[(unten, rechts)])
            # ⚠ **Und er darf keinen Text verdecken.** Sitzt der Griff oben
            # (also bei einer unteren Ecke), liegt er auf der Statuszeile:
            # „405 Baupläne" wurde zu „5 Baupläne", weil das Dreieck die
            # ersten Zeichen verdeckte. Gemeldet am 02.09.2026 zu rc11. Die
            # Zeile rueckt deshalb auf der Seite ein, an der er sitzt.
            try:
                platz = self.grip.winfo_reqwidth() + 6
                if unten:
                    self.status.pack_configure(
                        padx=((platz, 8) if rechts else (8, platz)))
                else:
                    self.status.pack_configure(padx=8)
            except tk.TclError:
                pass
            if ist_platziert == soll_sichtbar:
                if soll_sichtbar:
                    self.grip.place(**lage)   # Seite kann gewechselt haben
                return
            if soll_sichtbar:
                self.grip.place(**lage)
            else:
                self.grip.place_forget()
            if os.environ.get('SC_BP_GRIFF_PROTOKOLL'):
                # Nur auf Zuruf: schreibt mit, wer den Griff wann umstellt.
                # Gedacht, um die Stelle zu finden, die ihn nach dem Start
                # wieder einblendet — ohne dass man das Fenster sehen muss.
                import traceback
                with open(pfade.app_datei('griff-protokoll.txt'), 'a',
                          encoding='utf-8') as f:
                    f.write('%s  eingeklappt=%s  war_platziert=%s\n'
                            % (time.strftime('%H:%M:%S'), self.eingeklappt,
                               ist_platziert))
                    f.write(''.join(traceback.format_stack()[-6:-1]))
                    f.write('-' * 60 + '\n')
        except tk.TclError:
            pass

    def einstellungen_oeffnen(self):
        """Seit v3.0.0 führen beide Wege ins **eine** Fenster — nur auf eine
        andere Seite. Zwei getrennte Fenster hießen: raten, in welchem etwas
        steckt."""
        self.fenster_oeffnen('allgemein')

    def einrichtung_erneut(self):
        """Den Assistenten noch einmal durchlaufen lassen."""
        fertig, zeige_liste = assistent.starten(self.root)
        if fertig and zeige_liste:
            self.liste_oeffnen()

    def liste_oeffnen(self):
        """Das große Fenster auf der Bauplan-Liste öffnen."""
        self.fenster_oeffnen('liste')

    def fenster_oeffnen(self, seite='liste'):
        """Das Hauptfenster zeigen — und darin die gewünschte Seite.

        Ein zweiter Klick holt das vorhandene Fenster nach vorn und wechselt die
        Seite, statt ein zweites aufzumachen. Zwei gleiche Fenster nebeneinander
        sind für niemanden nachvollziehbar."""
        from scbp.hauptfenster import Hauptfenster
        vorhanden = getattr(self, '_fenster', None)
        if vorhanden is not None:
            try:
                # ⚠ Über `nach_vorn()`: `lift()` allein wird unter Wayland
                # ignoriert, und ein minimiertes Fenster bliebe minimiert.
                from scbp.hauptfenster import nach_vorn
                nach_vorn(vorhanden.root)
                vorhanden.oeffnen(seite)
                return
            except Exception:
                pass                       # war schon zu
        # ⚠ `startseite` durchreichen, NICHT hinterher oeffnen: Sonst baut das
        # Fenster erst die Bauplan-Liste und danach die eigentlich gewollte
        # Seite — zwei Aufbauten fuer einen Wunsch.
        self._fenster = Hauptfenster(self.root, beim_schliessen=self._liste_zu,
                                     version=__version__,
                                     beim_schriftwechsel=self.schriftgroesse_anwenden,
                                     startseite=seite)
        self.liste_lbl.faerben(zeichen.GRUEN)

    def _liste_zu(self):
        self._fenster = None
        self.liste_lbl.faerben(zeichen.GRAU)
        # ⚠ Genau hier zieht eine geänderte Anzeigeart. Stellt jemand in den
        # Einstellungen auf „nur bei einem Neuzugang" um, darf das Overlay nicht
        # sofort verschwinden — er steht ja noch davor und will das Ergebnis
        # sehen. Beim Schließen des Fensters ist der richtige Moment: Wer fertig
        # eingestellt hat, will zurück ins Spiel.
        self.verhalten_anwenden()

    def _current_geom(self):
        # Aus winfo bauen (nicht root.geometry()): so bleibt negatives Y als absolute
        # Position erhalten ('+-1439') statt als „vom unteren Rand" missverstanden zu werden.
        return (f'{self.root.winfo_width()}x{self.root.winfo_height()}'
                f'+{self.root.winfo_x()}+{self.root.winfo_y()}')

    def _save_geo(self, e=None):
        save_geometry(self._current_geom())

    def quit(self):
        self._save_geo()
        self.watcher.stop()
        self.root.destroy()

    # ------------------------------------------------- Verhalten im Spiel
    def verhalten_anwenden(self):
        """Pop-up-Betrieb und Durchklickbarkeit setzen — nach dem Aufbau.

        ⚠ Erst hier, nicht im Aufbau: Beides fasst das fertige Fenster an. Vorher
        hat es unter X11 noch keine Kennung, die man einer Maske geben könnte.
        """
        # ⚠ Hier stand bis 02.09.2026 ein Versuch, die Titelleiste bei den
        # unteren Ecken an den unteren Fensterrand zu haengen (gewuenscht von
        # Haldjas/pr0). Er ist zurueckgenommen: Das Umpacken liess Tk die
        # Fenstergroesse neu rechnen, ein eingeklapptes Fenster wuchs von 22
        # auf 120 px und ragte 86 px unter den Bildschirmrand — samt Leiste.
        # Und die Leiste ist im eingeklappten Zustand der EINZIGE Bedienweg.
        # Vier Anlaeufe (before=, Geschwister neu packen, Klappzustand
        # wiederherstellen, after()) haben es nicht geloest. Der Umbau gehoert
        # an den Rechner, an dem man das Overlay im Einsatz sieht.
        # ⚠⚠ **Und das Fenster in die Ecke setzen.** Die gewaehlte Ecke wirkte
        # bisher nur beim Ein- und Ausklappen (`_klappen` ruft `_klapp_ecke`).
        # Wer sie einstellt und das Werkzeug neu startet, fand es deshalb dort
        # wieder, wo es zuletzt STAND — nicht in der Ecke. Am 02.09.2026 am
        # Bildschirm gesehen: „unten links" eingestellt, Overlay saß oben
        # links. Ein aelterer Fehler, der erst auffiel, als die Leiste
        # mitwanderte.
        # ⚠⚠ Ueber `_klappen()`, NICHT mit selbst gemessener Hoehe. Am
        # 02.09.2026 stand hier `winfo_height()` — im eingeklappten Zustand
        # meldet Tk dort die Hoehe des INHALTS (gemessen: 120 px statt 22).
        # Damit gesetzt, ragte das Fenster 86 px unter den Bildschirmrand, und
        # die Leiste lag genau dort. `_klappen()` rechnet die richtige Hoehe
        # selbst aus und setzt Lage und Groesse in einem Zug.
        # ⚠ Per `after()`, nicht sofort: Beim Start steht der Klappzustand hier
        # noch nicht endgueltig fest, und Tk hat die Groesse noch nicht
        # gerechnet. Sofort gesetzt, wird die Hoehe gleich wieder ueberschrieben
        # — gemessen am 02.09.2026: 120 px statt 22, das Fenster ragte 86 px
        # unter den Bildschirmrand und nahm die Leiste mit.
        try:
            if (pfade.einstellung('overlay_ecke') or 'frei') != 'frei':
                # ⚠⚠ Die Methode heisst `klappzustand_setzen`. Hier stand bis
                # v3.9.2 `self._klappen(...)` — ein Name, den es nie gab. Der
                # Aufruf starb bei JEDEM Start mit AttributeError, das
                # `except` darunter fing ihn, und die gewaehlte Ecke wurde
                # deshalb beim Start nie angewandt. Gemeldet von Haldjas (pr0)
                # am 02.09.2026: „nach dem Start sitzt der Header wieder oben,
                # obwohl das Fenster auf links unten eingestellt ist."
                self.root.after(
                    120,
                    lambda: self.klappzustand_setzen(self.eingeklappt,
                                                     merken=False))
        except Exception as ausnahme:
            fehler.merken('overlay.ecke_beim_start', ausnahme)
        self.anzeigeart = pfade.einstellung('overlay_modus') or 'immer'
        if self.anzeigeart == 'popup':
            # ⚠ Die Lage merken, **bevor** versteckt wird. Ein Fenster, das noch
            # nie zu sehen war, meldet `1x1+0+0` — die Mauswache suchte dann in der
            # linken oberen Bildschirmecke statt dort, wo das Overlay steht, und
            # ging nie an. Beim Start im Aufblend-Betrieb ist genau das der Fall.
            self._letzte_lage = self._current_geom()
            if '+' not in self._letzte_lage or self._letzte_lage.startswith('1x1'):
                # Noch nie gezeichnet: dann gilt, was gespeichert ist.
                self._letzte_lage = load_geometry() or startlage(self.root)
            # Läuft gerade eine Einblendung, wird sie nicht abgeschnitten — der
            # Zähler räumt gleich selbst auf.
            if self._popup_uhr is None:
                self.root.withdraw()
                self._anfasser_zeigen()
        else:
            self._anfasser_weg()
            try:
                self.root.deiconify()
            except tk.TclError:
                pass
        self.durchklick_anwenden()

    def durchklick_anwenden(self):
        """Klicks durchreichen, wenn eingestellt — und melden, wenn es nicht geht."""
        an = pfade.einstellung_wahrheit('durchklickbar', False)
        if not an and not getattr(self, '_durchklick_war_an', False):
            return True                  # nie eingeschaltet gewesen: nichts zu tun
        self._durchklick_war_an = an
        try:
            geklappt = overlay.durchklickbar_setzen(self.root, an)
        except Exception as ausnahme:
            fehler.merken('overlay.durchklick', ausnahme)
            geklappt = False
        if an and not geklappt:
            self._status_setzen(sprache.Satz('ov_durchklick_geht_nicht'))
        # ⚠ Den Schiebeschalter auf der Seite „Anzeige" mitziehen, falls sie
        # gerade offen ist. Gemeldet am 02.09.2026: Wer das Durchreichen am
        # Schloss umlegte, sah dort weiter den alten Zustand — richtig wurde er
        # erst beim erneuten Aufrufen der Seite.
        #
        # ⚠ Gemeldet wird der Zustand, der WIRKLICH gilt (`an and geklappt`),
        # nicht der gewuenschte: Spielt das System nicht mit, wird die
        # Einstellung ohnehin zurueckgenommen — dann darf der Schalter nicht
        # „an" zeigen.
        try:
            anzeigen = overlay.DURCHKLICK_ANZEIGE[0]
            if anzeigen is not None:
                anzeigen(an and geklappt)
        except Exception as ausnahme:
            fehler.merken('overlay.durchklick_anzeige', ausnahme)
        # ⚠ Der Rückgabewert wird gebraucht: Der Schloss-Knopf in der Leiste
        # muss die Einstellung zurücknehmen, wenn das System nicht mitspielt —
        # sonst steht dort „durchklickbar an", während nichts durchgereicht wird.
        return geklappt

    # ---------------------------------------------- Der Anfasser holt es zurück
    #
    # Gemeldet am 25.08.2026: „Wie schaut es aus, das Fenster bei Mouseover sichtbar
    # zu machen, damit man den Umweg nicht gehen muss es erneut zu starten? Die
    # Logik kenne ich bisher ohnehin nicht bei anderen Programmen dieser Art."
    #
    # Er hat recht — „zum Zurückholen das Programm neu starten" verlangt kein
    # anderes Overlay.
    #
    # ⚠ Der erste Anlauf fragte die Mausposition ab (`winfo_pointerxy`) und blendete
    # ein, sobald sie im Bereich lag. Das **kann unter Wayland nicht gehen**:
    # Gemessen auf einem Rechner meldete Tk zwölfmal hintereinander exakt
    # dieselben Koordinaten, während die Maus quer über den Schirm fuhr. Eine
    # Anwendung erfährt die Zeigerposition dort nur, solange er über einem **ihrer
    # eigenen** Fenster steht — und ein verstecktes Fenster ist keines.
    #
    # Also bleibt ein Fenster stehen: ein schmaler Streifen an der oberen Kante der
    # letzten Position. Der bekommt echte `<Enter>`-Ereignisse, unter Wayland wie
    # unter X11 und Windows. Nebenbei ist er ehrlicher als eine unsichtbare
    # Zauberzone — man **sieht**, wo das Overlay wartet.
    # Kantenlänge des Schlosses, das im durchlässigen Zustand als einziges
    # klickbar bleibt. Klein genug, um im Gefecht nicht zu stören, groß genug,
    # um es mit der Maus zu treffen.
    SCHLOSS_KANTE = 26

    # Feinausgleich in Pixeln, um den das schwebende Schloss nach rechts gesetzt
    # wird, während es über dem Knopf in der Leiste liegt.
    #
    # ⚠ **Steht auf 0, und das ist das Ergebnis einer Messung — keine
    # Bequemlichkeit.** Am 28.08.2026 stand hier kurzzeitig eine 7. Sie stammte
    # aus einem Bildschirmfoto, auf dem das schwebende Schloss sichtbar links
    # neben dem Knopf saß; ausgezählt ergab das sieben Pixel.
    #
    # Der Haken: Das Foto war **5120×1440** groß — der zweite Monitor, nicht der
    # Hauptbildschirm mit 4096×1152. Dort sind die Symbole 24 px breit statt 22.
    # Ein in Pixeln gemessener Ausgleich gilt damit genau für den einen
    # Bildschirm, auf dem gemessen wurde, und verschiebt ihn auf jedem anderen.
    #
    # Nachgemessen am laufenden Programm auf dem Hauptbildschirm (Fensterlage
    # über `EnumWindows`, Leiste über `PrintWindow` abgegriffen und die
    # Symbolspalten ausgezählt):
    #
    #     Knopf in der Leiste   Symbol bei x = 838 … 855
    #     schwebendes Fenster   x = 843, Symbol also ab 845
    #
    # Mit Ausgleich saß es **sieben Pixel zu weit rechts**; ohne sitzt es
    # deckungsgleich. Der Wert bleibt als benannte Konstante stehen, damit die
    # Stelle auffindbar ist — wer hier wieder eine Zahl einträgt, sollte den
    # Absatz oben gelesen haben.
    SCHLOSS_FEIN_X = 0

    def _schloss_anwenden(self, an, versuch=0):
        """Das Schloss zeigen oder wegnehmen — gerufen aus `overlay.py`.

        ⚠ Der einzige Weg zurück. Werden Klicks durchgereicht, ist am Overlay
        nichts mehr zu treffen: kein Knopf, keine Leiste, auch der Schalter in
        den Einstellungen ist unerreichbar, weil man das Fenster nicht mehr
        aufbekommt. Bis hierher half nur, das Programm ein zweites Mal zu
        starten — und dafür muss man aus dem Spiel heraus. Genau das soll die
        Einstellung ja vermeiden.

        Deshalb ein **eigenes kleines Fenster**: Es liegt über dem Overlay, wird
        nie durchlässig gemacht und trägt das Schloss. Ein Klick darauf hebt das
        Durchreichen wieder auf. Denselben Weg geht Ryze beim TeamSpeak-Plugin.
        """
        if not an:
            return self._schloss_weg()
        try:
            self.root.update_idletasks()
            # ⚠ Es soll aussehen, als würde **dasselbe** Schloss grün. der Autor
            # am 28.08.2026: „am besten wäre das gleiche schloss grün zu färben
            # was eh in der leiste ist, und es damit auch wieder zu entsperren".
            #
            # Genau das passiert hier — mit einer Einschränkung, die bleiben
            # muss: Ein eigenes Fenster ist es trotzdem. Wer Klicks durchreicht,
            # reicht sie für das **ganze** Fenster durch; ein Knopf in der Leiste
            # wäre in dem Moment genauso tot wie der Rest. Deshalb liegt hier ein
            # zweites, nie durchlässiges Fenster **passgenau über** dem Schloss
            # in der Leiste: gleiche Stelle, gleiche Größe, gleiches Bauteil.
            # Für den Spieler ist es ein Schloss, das die Farbe wechselt.
            knopf = getattr(self, 'schloss_lbl', None)
            sichtbar = False
            try:
                # ⚠ `ismapped()` allein genügt nicht — die Maße müssen auch
                # stimmen. Nachgemessen am 28.08.2026 an einem Fenster wie dem
                # Overlay:
                #
                #     direkt nach dem Bau    ismapped=0  w=1   rootx=0
                #     nach update_idletasks  ismapped=0  w=1   rootx=0
                #     nach update            ismapped=1  w=68  rootx=488
                #
                # Ein ungezeichnetes Widget meldet Breite **1** und Position 0.
                # Kommt eine Tk-Fassung mit `ismapped=1` bei noch fehlenden Maßen
                # heraus, säße das Schloss in der Bildschirmecke — schlimmer als
                # der Rückfall. Beides muss stimmen, sonst wird gewartet.
                sichtbar = (knopf is not None and knopf.winfo_ismapped()
                            and knopf.winfo_width() > 1
                            and knopf.winfo_height() > 1)
            except tk.TclError:
                sichtbar = False
            if sichtbar:
                x = knopf.winfo_rootx() + self.SCHLOSS_FEIN_X
                y = knopf.winfo_rooty()
                breite = max(knopf.winfo_width(), 8)
                hoehe = max(knopf.winfo_height(), 8)
            elif (knopf is not None and self._wird_noch_gezeichnet()
                    and (versuch < 10 or self.anzeigeart != 'popup')):
                # ⚠ **Beim Start ist der Knopf noch nicht gezeichnet — dann wird
                # gewartet, nicht geraten.** Gemeldet von Haldjas (pr0) am
                # 28.08.2026 zu rc91: „Starte Watcher — Schloss ist an 2
                # Positionen … position bleibt so bis man den watcher neu
                # startet."
                #
                # `verhalten_anwenden()` läuft unmittelbar vor `mainloop()`. Die
                # Leiste steht da im Baum, aber Tk hat noch nichts gemalt: Weder
                # `winfo_ismapped()` noch die Maße stimmen (ein ungezeichnetes
                # Widget meldet Breite **1** — dieselbe Tk-Falle wie beim
                # Rundrahmen). Das Schloss landete deshalb bei **jedem** Start
                # neben dem Overlay statt darauf, und daneben stand das Schloss
                # der Leiste: zwei Schlösser, eines davon am falschen Platz.
                #
                # Ein kurz aufblitzendes falsches Schloss wäre nur die halbe
                # Reparatur — also gar nicht erst bauen, sondern nachfassen, bis
                # die Leiste steht.
                #
                # ⚠ **Die Begrenzung gilt nur im Aufblend-Betrieb.** Dort ist das
                # Overlay absichtlich weg und kommt vielleicht nie wieder — nach
                # zehn Anläufen weicht das Schloss deshalb auf den
                # Anfasser-Streifen aus, den es dort ja gibt.
                #
                # Steht das Overlay dauerhaft ("Immer sichtbar"), gibt es keinen
                # Anfasser, auf den man ausweichen könnte: Die gemerkte Lage ist
                # dann irgendeine frühere Fensterposition, und das Schloss landete
                # sichtbar daneben, bis es beim nächsten Anlass zurücksprang.
                # Gemeldet am 28.08.2026: "das schloss springt nach ner zeit an
                # die richtige stelle". Also: dort ohne Begrenzung warten — die
                # Leiste kommt, sie ist ja sichtbar.
                self.root.after(300, lambda: self._nachfassen(versuch + 1))
                return
            else:
                # ⚠ **Im Pop-up-Betrieb ist das Overlay versteckt** — und ein
                # verstecktes Fenster taugt nicht als Bezugspunkt. Genau daran
                # scheiterte rc92 bei Haldjas (pr0): Sein Bericht zeigt
                # `overlay_modus=popup`, und `verhalten_anwenden()` ruft dort
                # `withdraw()`, **bevor** je gezeichnet wurde. Der Knopf in der
                # Leiste ist damit dauerhaft nicht gemappt, das Nachfassen läuft
                # zehnmal leer, und danach rechnete diese Stelle aus der Lage
                # eines unsichtbaren Fensters. Für ihn schwebte das Schloss frei
                # neben dem Overlay — „rechts neben dem watcher".
                #
                # Denselben Fall löst `_anfasser_zeigen()` seit jeher richtig:
                # Es rechnet aus `self._letzte_lage`, der gemerkten Position.
                # Das Schloss geht denselben Weg und legt sich an die rechte
                # obere Ecke dieser Lage — dorthin, wo im sichtbaren Zustand der
                # Knopf in der Leiste sitzt. Blendet das Overlay auf, rückt es
                # von selbst an seinen Platz (`_popup_zeigen` fasst nach).
                #
                # ⚠ Und zwar **direkt neben den Anfasser-Streifen**, nicht an
                # die rechte Ecke der gemerkten Lage. Haldjas zu rc93: „das
                # schloss sitzt jetzt neben dem watcher" — richtig gerechnet,
                # aber einsam: Der Streifen sitzt mittig, das Schloss saß gut
                # zweihundert Pixel weiter rechts, wo gar nichts zu sehen ist.
                # Zwei Marken für dieselbe Sache gehören zusammen; dann liest
                # man „hier wartet das Overlay, und hier ist das Schloss".
                lage = GEOM_RE.match(self._letzte_lage or '')
                if lage is not None and lage.group(3) is not None:
                    ov_breite, _h, links, oben = (int(z) for z in lage.groups())
                    # ⚠ Dieselbe Rechnung wie der Streifen selbst — sonst
                    # laufen die beiden auseinander, sobald sich eine von
                    # ihnen aendert. Sie gehoeren zusammen.
                    streifen = self._anfasser_x(links, ov_breite)
                    # ⚠ In einer RECHTEN Ecke gehoert das Schloss **links**
                    # neben den Streifen. Rechts daneben waere es das erste,
                    # was ueber den Bildschirmrand rutscht — der Streifen
                    # klebt dort ja schon an der Kante.
                    try:
                        ecke = pfade.einstellung('overlay_ecke') or 'frei'
                    except Exception:
                        ecke = 'frei'
                    if ecke.endswith('rechts'):
                        x = streifen - self.SCHLOSS_KANTE - 4
                    else:
                        x = streifen + self.ANFASSER_BREITE + 4
                    # ⚠ Auch hier die Ecke fragen: Bei einer unteren gehoert
                    # das Schloss neben den Streifen an der UNTERkante, nicht
                    # an die Oberkante des gemerkten Fensters. Sonst stehen
                    # die beiden wieder an verschiedenen Stellen.
                    y = self._anfasser_y(oben, _h)
                    # Das Schloss ist hoeher als der duenne Streifen — es wird
                    # an ihm ausgerichtet, statt darunter hinauszuragen.
                    if ecke.startswith('unten'):
                        y = max(oben, y - self.SCHLOSS_KANTE
                                + self.ANFASSER_HOEHE)
                else:
                    x = (self.root.winfo_rootx()
                         + max(0, self.root.winfo_width()
                               - self.SCHLOSS_KANTE - 4))
                    y = self.root.winfo_rooty() + 4
                breite = hoehe = self.SCHLOSS_KANTE
            # ⚠ Jedes Mal frisch bauen, nicht wiederverwenden. Die Symbolgröße
            # hängt an der eingestellten Schriftgröße — ein aufgehobenes Fenster
            # trüge nach einem Wechsel das alte Bild und säße daneben.
            self._schloss_wegraeumen()
            self._schloss = tk.Toplevel(self.root)
            self._schloss.overrideredirect(True)
            self._schloss.attributes('-topmost', True)
            self._schloss.configure(bg=BAR, cursor='hand2')
            # ⚠ **Dieselbe Deckkraft wie das Overlay.** Ein Toplevel erbt sie
            # nicht: `DECKKRAFT` wird auf `self.root` gesetzt, das Schloss lag
            # daneben und war voll deckend. Bei 93 % schien der Knopf in der
            # Leiste zu 93 % durch, das Schloss darüber zu 100 % — zwei
            # Schlösser mit verschiedener Sättigung übereinander, und schon der
            # kleinste Versatz sah aus wie zwei getrennte Symbole. Es soll
            # aussehen wie **ein** Schloss, das die Farbe wechselt.
            try:
                self._schloss.attributes('-alpha', DECKKRAFT / 100.0)
            except tk.TclError:
                pass
            # Dasselbe Bauteil wie in der Leiste — nur grün und geschlossen.
            marke = zeichen.knopf(self._schloss, 'schloss_zu',
                                  self._schloss_loesen, farbe=zeichen.GRUEN,
                                  grund=BAR, schrift=self.f_title)
            marke.pack(expand=True)
            self._schloss.bind('<Button-1>', lambda e: self._schloss_loesen())
            hinweis.anhaengen(self._schloss,
                              lambda: sprache.t('hinweis_schloss'))
            self._schloss.geometry('%dx%d+%d+%d' % (breite, hoehe, x, y))
            self._schloss.lift()
            # Und das Schloss darunter sagt jetzt dasselbe: zu und grün. Sichtbar
            # ist es nicht, solange das Fenster darüber liegt — aber wenn dieses
            # aus irgendeinem Grund nicht kommt, steht dort wenigstens nicht das
            # Gegenteil des wahren Zustands.
            self._leistenschloss(True)
        except tk.TclError:
            pass
        except Exception as ausnahme:
            fehler.merken('overlay.schloss', ausnahme)

    def _wird_noch_gezeichnet(self):
        """Fehlt der Knopf, weil Tk noch malt — oder weil das Fenster weg soll?

        ⚠ Diese Unterscheidung fehlte, und sie kostete drei Sekunden bei jedem
        Zublenden. Haldjas (pr0) am 28.08.2026 zu rc95: „wenn der watcher
        minimiert wurde, dauert es nochmal 3 sekunden bis das schloss sich wieder
        seine alte position neben dem balken sucht." Das waren **genau** die
        zehn Nachfass-Versuche à 300 ms — gedacht für den Start, wo die Leiste
        gleich kommt, aber blind auch dann gelaufen, wenn das Overlay gerade
        absichtlich verschwunden ist. Warten auf etwas, das nicht kommt.

        Nachgemessen trennt `root.winfo_ismapped()` die beiden Fälle sauber:

            Start, nach update_idletasks   root=1  knopf=0   ← wird noch gemalt
            nach withdraw()                root=0  knopf=0   ← soll gar nicht da sein
        """
        try:
            return bool(self.root.winfo_ismapped())
        except tk.TclError:
            return False

    def _schloss_nachziehen(self):
        """Das Schloss neu platzieren, wenn es eines gibt.

        Gerufen, sobald sich der Bezugspunkt ändert — im Pop-up-Betrieb also
        beim Auf- und Zublenden. Steht kein Schloss, passiert nichts."""
        try:
            if (self._schloss is not None and self._schloss.winfo_exists()
                    and pfade.einstellung_wahrheit('durchklickbar', False)):
                self._schloss_anwenden(True)
        except Exception:
            pass

    def _nachfassen(self, versuch):
        """Die Lage des Schlosses noch einmal setzen, sobald die Leiste steht.

        ⚠ Nur, solange das Durchreichen überhaupt noch an ist: Wird in der
        Zwischenzeit entsperrt, dürfte ein Nachzügler das Schloss nicht wieder
        aufbauen."""
        try:
            if pfade.einstellung_wahrheit('durchklickbar', False):
                self._schloss_anwenden(True, versuch)
        except Exception:
            pass

    def _leistenschloss(self, zu):
        """Das Schloss in der Leiste auf den wahren Zustand stellen."""
        knopf = getattr(self, 'schloss_lbl', None)
        if knopf is None:
            return
        try:
            knopf.symbol_tauschen('schloss_zu' if zu else 'schloss_auf')
            knopf.faerben(zeichen.GRUEN if zu else zeichen.GRAU)
        except Exception:
            pass                         # Anzeige darf das Schalten nie kippen

    def _schloss_wegraeumen(self):
        """Das schwebende Schloss abbauen, falls eines steht."""
        alt = self._schloss
        self._schloss = None
        try:
            if alt is not None and alt.winfo_exists():
                alt.destroy()
        except tk.TclError:
            pass

    def _schloss_weg(self):
        self._schloss_wegraeumen()
        self._leistenschloss(False)

    def _schloss_loesen(self):
        """Klick aufs Schloss: Klicks wieder abfangen, Overlay ist bedienbar."""
        pfade.einstellung_setzen('durchklickbar', False)
        self._durchklick_war_an = True   # damit `durchklick_anwenden` es aufhebt
        self.durchklick_anwenden()
        self._status_setzen(sprache.Satz('ov_schloss_offen'))

    def _logs_neu_einlesen(self):
        """Klick auf den Knopf in der Leiste: alle Protokolle noch einmal lesen.

        Die Arbeit macht der Watcher-Faden, hier wird nur gebeten. Läuft keiner,
        wird das gesagt, statt so zu tun als sei etwas passiert."""
        if overlay.neu_einlesen_anstossen():
            self._status_setzen(sprache.Satz('s_be_neu_los'))
        else:
            self._status_setzen(sprache.Satz('s_be_neu_kein'))

    def _schloss_zusperren(self):
        """Klick auf das offene Schloss in der Leiste: Klicks ab jetzt ins Spiel.

        Das Gegenstück zu `_schloss_loesen()` — und der Grund, warum es den
        Knopf gibt: Ohne ihn war das Einschalten nur über die Einstellungen
        erreichbar, während das Ausschalten direkt am Overlay ging.

        Klappt es nicht, wird die Einstellung **zurückgenommen**. Ein
        gespeichertes „an", während in Wahrheit nichts durchgereicht wird, wäre
        das schlechteste von beidem — genauso hält es der Schalter in den
        Einstellungen (`seiten._durchklick_um`)."""
        pfade.einstellung_setzen('durchklickbar', True)
        if self.durchklick_anwenden():
            self._status_setzen(sprache.Satz('ov_schloss_zu'))
        else:
            pfade.einstellung_setzen('durchklickbar', False)

    ANFASSER_BREITE = 54
    ANFASSER_HOEHE = 5

    def _anfasser_x(self, links, ov_breite):
        """Wo der Streifen sitzt — an der Seite, die zur gewaehlten Ecke passt.

        ⚠ Bis zum 02.09.2026 sass er **immer mittig**. In einer Ecke sieht das
        falsch aus: Wer das Overlay nach links unten legt, erwartet den Griff
        dort und nicht in der Bildmitte. Der Autor am 02.09.2026: *„Links als
        Auswahl, links an die Leiste statt mittig; rechts ausgewaehlt, pack das
        Schloss und den Strich rechts an das Fenster."*

        Ohne gewaehlte Ecke (`frei`) bleibt es mittig — dort gibt es keine
        Seite, an die er gehoeren wuerde.
        """
        try:
            ecke = pfade.einstellung('overlay_ecke') or 'frei'
        except Exception:
            ecke = 'frei'
        if ecke.endswith('links'):
            return links
        if ecke.endswith('rechts'):
            return links + max(0, ov_breite - self.ANFASSER_BREITE)
        return links + max(0, (ov_breite - self.ANFASSER_BREITE) // 2)

    def _anfasser_y(self, oben, ov_hoehe):
        """Auf welcher Hoehe der Streifen sitzt — Ober- oder Unterkante.

        ⚠⚠ **Dieselbe Falle wie bei der Breite, nur eine Achse weiter.** Am
        02.09.2026 wurde `_anfasser_x` an die Ecke angepasst, `y` blieb dagegen
        stur die **Oberkante** der gemerkten Lage. Bei einer unteren Ecke
        waechst das Overlay nach oben — seine Oberkante liegt dann fast am
        oberen Bildrand, und der Streifen sass mitten im Bild statt unten.
        Gemeldet noch am selben Tag mit einem Bildschirmfoto.

        Bei einer unteren Ecke gehoert er also an die **Unterkante** der
        gemerkten Lage; oben und ohne Ecke bleibt es die Oberkante.
        """
        try:
            ecke = pfade.einstellung('overlay_ecke') or 'frei'
        except Exception:
            ecke = 'frei'
        if ecke.startswith('unten'):
            return oben + max(0, ov_hoehe - self.ANFASSER_HOEHE)
        return oben

    def _anfasser_zeigen(self):
        """Den Streifen an die letzte Position des Overlays legen."""
        if self.anzeigeart != 'popup':
            return self._anfasser_weg()
        lage = self._letzte_lage or ''
        m = GEOM_RE.match(lage)
        if not m or m.group(3) is None:
            return
        breite, _hoehe, links, oben = (int(z) for z in m.groups())
        x = self._anfasser_x(links, breite)
        # ⚠ **Kein `max(0, …)` auf der Höhe.** Hier stand es, und damit
        # widersprach diese Zeile dem, was `_current_geom()` zwei Funktionen
        # weiter oben ausdrücklich bewahrt: „so bleibt negatives Y als absolute
        # Position erhalten (`+-1439`)".
        #
        # Auf **mehreren Bildschirmen** ist ein negatives Y keine kaputte
        # Angabe, sondern eine gültige: Wer einen zweiten Monitor über dem
        # Hauptmonitor liegen hat, arbeitet dort mit Werten unterhalb von null.
        # `max(0, …)` klemmt sie auf die Oberkante des Hauptmonitors — Streifen
        # und Schloss sprangen dadurch auf den falschen Bildschirm. Aufgefallen
        # am 28.08.2026 an einem Aufbau mit zwei Monitoren übereinander.
        #
        # ⚠ Die Ecke entscheidet, ob Ober- oder Unterkante gilt — siehe
        # `_anfasser_y`. `_hoehe` ist die gemerkte Fensterhoehe.
        y = self._anfasser_y(oben, _hoehe)
        try:
            if self._anfasser is None or not self._anfasser.winfo_exists():
                self._anfasser = tk.Toplevel(self.root)
                self._anfasser.overrideredirect(True)
                self._anfasser.attributes('-topmost', True)
                self._anfasser.configure(bg=ACCENT, cursor='hand2')
                try:
                    self._anfasser.attributes('-alpha', 0.55)
                except tk.TclError:
                    pass
                self._anfasser.bind('<Enter>',
                                    lambda e: self._popup_zeigen(wegen_maus=True))
                self._anfasser.bind('<Button-1>',
                                    lambda e: self._popup_zeigen(wegen_maus=True))
                hinweis.anhaengen(self._anfasser,
                                  lambda: sprache.t('hinweis_anfasser'))
            self._anfasser.geometry('%dx%d+%d+%d'
                                    % (self.ANFASSER_BREITE, self.ANFASSER_HOEHE,
                                       x, y))
            self._anfasser.deiconify()
            self._anfasser.lift()
        except tk.TclError:
            pass

    def _anfasser_weg(self):
        try:
            if self._anfasser is not None and self._anfasser.winfo_exists():
                self._anfasser.withdraw()
        except tk.TclError:
            pass

    def bei_fund_zeigen(self):
        """Nach einem Fund sichtbar machen — je nach Betriebsart auf ihrem Weg.

        ⚠⚠ **Ein eingeklapptes Overlay meldete bisher gar nichts.** Im
        Aufblend-Betrieb kam es bei jedem Bauplan zurueck; wer dagegen „Immer
        sichtbar" gewaehlt und die Leiste zugeklappt hatte, bekam nur den
        Signalton. Der Fund stand in der Liste, sichtbar wurde er erst, wenn
        jemand von Hand aufklappte.

        Mit **durchgereichten Mausklicks** war das doppelt aergerlich: Man hoert
        den Ton, kann aber nichts anklicken — erst das Schloss treffen, dann den
        Klapp-Knopf. Zwei Handgriffe mitten im Kampf, fuer eine Meldung, die man
        nur kurz sehen wollte. Ein zugeklapptes Overlay schaltete damit genau
        die Funktion ab, fuer die es da ist.

        Also klappt es jetzt selbst auf und nach derselben Zeit wieder zu, die
        auch der Aufblend-Betrieb benutzt. ⚠ **Ohne `merken`** — der Wunsch des
        Spielers, zugeklappt zu arbeiten, bleibt bestehen; das hier ist nur ein
        Blick, kein neuer Zustand.
        """
        if self.anzeigeart == 'popup':
            return self._popup_zeigen()
        if not self.eingeklappt:
            return                       # steht ohnehin offen, nichts zu tun
        try:
            self.klappzustand_setzen(False, merken=False)
        except tk.TclError:
            return
        # ⚠ Bei mehreren Bauplaenen kurz hintereinander die Uhr neu stellen,
        # nicht mehrere laufen lassen — sonst klappt es mitten im naechsten
        # Fund wieder zu.
        if self._zuklapp_uhr is not None:
            try:
                self.root.after_cancel(self._zuklapp_uhr)
            except (tk.TclError, ValueError):
                pass
        sekunden = pfade.einstellung_zahl('popup_sekunden', 6, 2, 60)
        self._zuklapp_uhr = self.root.after(sekunden * 1000,
                                            self._wieder_zuklappen)

    _zuklapp_uhr = None

    def _wieder_zuklappen(self):
        """Nach dem Blick auf einen Fund zurueck in die Leiste.

        ⚠ Steht die Maus darauf, wird gewartet — dieselbe Ruecksicht wie im
        Aufblend-Betrieb. Ein Fenster, das unter dem Zeiger zuklappt, waehrend
        man es liest, aergert mehr als eines, das zu lange bleibt.
        """
        self._zuklapp_uhr = None
        if self.anzeigeart == 'popup' or self.eingeklappt:
            return
        try:
            if getattr(self, '_maus_drauf', False):
                self._zuklapp_uhr = self.root.after(800,
                                                    self._wieder_zuklappen)
                return
            self.klappzustand_setzen(True, merken=False)
        except tk.TclError:
            pass

    def _popup_zeigen(self, wegen_maus=False):
        """Das Overlay kurz einblenden — im Pop-up-Betrieb nach einem Fund.

        Der Zähler wird bei jedem neuen Fund neu gestellt: Wer drei Baupläne
        hintereinander bekommt, soll nicht dreimal ein Fenster aufblitzen sehen,
        sondern eines, das stehen bleibt, solange etwas passiert.
        """
        if self.anzeigeart != 'popup':
            return
        try:
            self._anfasser_weg()
            self.root.deiconify()
            self.root.lift()
            self.root.attributes('-topmost', True)
        except tk.TclError:
            return
        # ⚠ Jetzt gibt es die Leiste wieder — also gehört das Schloss darauf und
        # nicht mehr an die gemerkte Lage. Ohne diese Zeile bliebe es dort
        # liegen, wo es im versteckten Zustand saß, und läge im schlimmsten Fall
        # neben dem gerade aufgeblendeten Fenster.
        self._schloss_nachziehen()
        if self._popup_uhr is not None:
            try:
                self.root.after_cancel(self._popup_uhr)
            except (tk.TclError, ValueError):
                pass
        sekunden = pfade.einstellung_zahl('popup_sekunden', 6, 2, 60)
        if wegen_maus:
            # Von der Maus geholt: nicht nach ein paar Sekunden wieder wegnehmen,
            # während jemand hinsieht. Es verschwindet, wenn die Maus weg ist —
            # darum kümmert sich `_popup_verstecken`.
            self._wegen_maus = True
        self._popup_uhr = self.root.after(sekunden * 1000, self._popup_verstecken)

    def _popup_verstecken(self):
        self._popup_uhr = None
        if self.anzeigeart != 'popup':
            return
        # Solange die Maus darauf steht, bleibt es stehen. Ein Fenster, das unter
        # dem Mauszeiger verschwindet, während man es ansieht, ist ärgerlicher als
        # eines, das zu lange bleibt.
        # ⚠ Über `<Enter>`/`<Leave>` am Fenster, **nicht** über die Mausposition.
        # Die abzufragen geht unter Wayland nicht: Sobald der Zeiger kein eigenes
        # Fenster mehr berührt, meldet Tk denselben Wert weiter, und das Overlay
        # bliebe für immer stehen.
        if getattr(self, '_maus_drauf', False):
            self._popup_uhr = self.root.after(800, self._popup_verstecken)
            return
        # Solange ein Fenster davor offen ist, bleibt auch das Overlay stehen —
        # sonst verschwindet es unter den Händen, während man die Liste liest.
        for name in ('listenfenster', 'hauptfenster'):
            fenster = getattr(self, name, None)
            try:
                if fenster is not None and fenster.root.winfo_exists():
                    self._popup_uhr = self.root.after(2000, self._popup_verstecken)
                    return
            except (tk.TclError, AttributeError):
                pass
        try:
            # Die Lage merken, bevor das Fenster verschwindet — danach meldet Tk
            # für ein verstecktes Fenster keine brauchbaren Werte mehr, und die
            # Mauswache wüsste nicht, wo sie hinsehen soll.
            jetzt = self._current_geom()
            if '+' in jetzt and not jetzt.startswith('1x1'):
                self._letzte_lage = jetzt
            self.root.withdraw()
            self._anfasser_zeigen()
            # Dasselbe rückwärts: Ohne Leiste gilt wieder die gemerkte Lage.
            self._schloss_nachziehen()
        except tk.TclError:
            pass

    def hotkey_anmelden(self):
        """Die Tastenkombination beim System anmelden — oder ehrlich schweigen.

        ⚠⚠ **Der Grund, warum es sie gibt:** Star Citizen laeuft im Vollbild
        und blendet den Mauszeiger aus. Wer nachsehen will, ob er einen
        Bauplan schon hat, muss heraustabben und das Fenster dann BLIND suchen
        und anklicken. Am 31.08.2026 als Nutzerwunsch gemeldet.

        ⚠ Scheitert es, wird es NICHT gemeldet: Beim Start weiss noch niemand,
        dass es die Kombination gibt, und eine Fehlermeldung ueber etwas, das
        man nie eingestellt hat, verwirrt nur. Der Grund steht auf der
        Einstellungsseite — dort, wo jemand danach sucht.
        """
        try:
            if pfade.einstellungen().get('hotkey_an') is False:
                fehler.spur('Hotkey: ausgeschaltet')
                return
            kombi = (pfade.einstellung('hotkey') or hotkey_modul.STANDARD)
            ok, grund = self.hotkey.anmelden(kombi)
            fehler.spur('Hotkey: %s (%s)'
                        % ('%s angemeldet' % kombi if ok else 'entfaellt',
                           grund or 'ok'))
        except Exception as ausnahme:
            fehler.merken('overlay.hotkey', ausnahme)

    def _hotkey_nachsehen(self):
        """Im selben Takt wie die Warteschlange nachfragen.

        ⚠⚠ **Hier wird nur die Fahne abgeholt.** Gewartet wird woanders:
        Unter Windows landet der Druck in der Schlange des Fadens, der
        angemeldet hat — war das der Tk-Faden, raeumte Tk ihn selbst weg,
        bevor dieser Takt nachsah (v3.8.0 und frueher, gemessen 0 von 3).
        Seither haelt ein eigener Faden die Stellung, siehe `scbp/hotkey.py`.
        """
        try:
            if self.hotkey.nachsehen():
                self.hervorholen()
        except Exception:
            pass

    def hervorholen(self):
        """Von außen gerufen: Fenster her, egal in welchem Betrieb.

        Das ist der Rückweg aus dem Pop-up-Betrieb. Ausgelöst wird er dadurch,
        dass jemand das Programm ein zweites Mal startet (siehe
        `scbp/overlay.py`) — auf die Verknüpfung lässt sich eine ganz normale
        Tastenkombination des Systems legen.
        """
        try:
            self.root.deiconify()
            self.root.lift()
            self.root.attributes('-topmost', True)
            self.liste_oeffnen()
        except Exception as ausnahme:
            fehler.merken('overlay.hervorholen', ausnahme)

    def ablagesymbol_starten(self):
        """Das Symbol neben der Uhr — nur unter Windows und nur, wenn gewünscht.

        ⚠ Es gehört zum Pop-up-Betrieb: Blendet sich das Overlay nur noch bei
        einem Fund ein, braucht es einen Weg zurück. Unter Linux ist das der
        Startmenü-Eintrag, unter Windows dieses Symbol.
        """
        # ⚠ Jeder Ausgang wird in den Startverlauf geschrieben. Zweimal wurde
        # hier auf Verdacht repariert (rc24, rc29), weil niemand sagen konnte,
        # ob das Symbol scheitert oder gar nicht erst versucht wird — Haldjas'
        # Bericht zeigte weder einen Fehler noch eine Spur. Eine Zeile im
        # Startverlauf beantwortet das beim nächsten Bericht sofort.
        if not ablagesymbol.moeglich():
            fehler.spur('Ablagesymbol: entfällt (nicht Windows)')
            return
        if not pfade.einstellung_wahrheit('tray', True):
            fehler.spur('Ablagesymbol: abgeschaltet (Einstellung „tray")')
            return
        try:
            self._ablage = ablagesymbol.Ablagesymbol(
                beim_zeigen=lambda: self.root.after(0, self.hervorholen),
                beim_beenden=lambda: self.root.after(0, self._ganz_beenden))
            geklappt = self._ablage.starten(sprache.t('tray_zeigen'),
                                            sprache.t('tray_beenden'))
            fehler.spur('Ablagesymbol: %s'
                        % ('steht' if geklappt else 'NICHT angelegt'))
            if not geklappt:
                # Der Rückgabewert wurde bisher weggeworfen. Ein „nein" ist
                # aber genau die Auskunft, die in den Bericht gehört.
                fehler.merken('overlay.ablagesymbol',
                              OSError('Ablagesymbol.starten() meldet, dass es '
                                      'nicht angelegt werden konnte'))
        except Exception as ausnahme:
            fehler.spur('Ablagesymbol: Fehler beim Anlegen')
            fehler.merken('overlay.ablagesymbol', ausnahme)

    def run(self):
        self.verhalten_anwenden()
        self.ablagesymbol_starten()
        # Ein zweiter Start soll das vorhandene Fenster hervorholen, statt eine
        # zweite Version zu öffnen. Der Rückruf kommt aus einem eigenen Faden —
        # deshalb die Arbeit per `after` an Tk übergeben, nicht dort erledigen.
        overlay.waechter_starten(
            lambda: self.root.after(0, self.hervorholen))
        self.hotkey_anmelden()
        self.root.mainloop()


if __name__ == '__main__':
    # Ablauf beim Start — in dieser Reihenfolge mit Absicht:
    #
    #   1. Spielordner beschaffen. Wird er nicht gefunden, FRAGEN wir danach,
    #      statt eine Meldung hinzuwerfen und uns zu beenden. Ohne die Game.log
    #      kann das Programm nichts, also ist das die eine Angabe, die es
    #      wirklich braucht.
    #   2. Beim allerersten Mal die alten Logs nachlesen — sichtbar, denn hier
    #      bekommt der Spieler seinen ganzen bisherigen Bestand geschenkt.
    #   3. Erst danach darf von Hand nachgetragen werden, und nur das, was
    #      wirklich keine Logdatei mehr hergibt.
    # ⚠ Läuft schon eine Version? Dann keine zweite öffnen, sondern der
    # vorhandenen sagen, sie soll sich zeigen. Genau darüber führt der Weg zurück,
    # wenn das Overlay im Pop-up-Betrieb unsichtbar ist.
    if overlay.zeigen_bitte():
        sys.exit(0)

    # ⚠ Windows-Kennzeichen, damit der Installer uns findet und vor dem
    # Überschreiben schließen kann. Ohne das bricht das Setup mitten im
    # Kopieren ab: „DeleteFile failed; code 32 — Der Prozess kann nicht auf die
    # Datei zugreifen, da sie von einem anderen Prozess verwendet wird."
    # Beim Testen so gemeldet (Haldjas, 25.08.2026); die Installation blieb halb
    # fertig liegen, und danach startete nur noch das Setup.
    #
    # Der Name muss mit `AppMutex` in `packaging/installer.iss` übereinstimmen.
    # Das Kennzeichen wird nur gesetzt, nie abgefragt — den Einzelstart regelt
    # `overlay.zeigen_bitte()` oben.
    if pfade.WINDOWS:
        try:
            import ctypes
            ctypes.windll.kernel32.CreateMutexW(
                None, False, 'SC-BP-Watcher-Einzelstart')
        except Exception as ausnahme:
            fehler.merken('start.mutex', ausnahme)

    # ⚠ Die **eine** Tk-Instanz des Programms. Sie entsteht hier und wird an alles
    # weitergereicht — Assistent wie Overlay. Vorher legte der Assistent eine
    # eigene an und zerstörte sie am Ende; die zweite, die das Overlay danach
    # anlegte, lief auf einem Interpreter, in dem noch Schriften und Bilder der
    # ersten hingen. Ergebnis war ein `SIGSEGV` beim **ersten** Programmstart —
    # also bei jedem neuen Nutzer, und nur dort, weil der Assistent nur einmal
    # läuft. Gemeldet von Bomb20 am 25.08.2026.
    #
    # Sie bleibt versteckt, bis das Overlay sie übernimmt: Ein leeres graues
    # Fenster hinter dem Assistenten hätte niemand erklären können.
    # ⚠ Erst hier, nicht weiter oben: Eine zweite Instanz beendet sich in
    # `zeigen_bitte()` wieder, und die legt sonst die Absturzspur der laufenden
    # beiseite. Ab dieser Zeile ist ein harter Abbruch nachlesbar — ein SIGSEGV
    # aus Tk hinterlässt sonst nichts, was man melden könnte.
    fehler.absturzfaenger()
    fehler.VERSION[0] = __version__
    fehler.spur('Start, Version %s, %s' % (__version__, sys.platform))
    # ⚠⚠ **Vor dem ersten Fenster.** Sonst haette die Wurzel — und alles, was
    # vor diesem Aufruf entsteht — weiterhin die helle Leiste des Systems.
    # Unter Linux tut der Aufruf nichts und kostet nichts.
    titelleiste.einrichten()
    wurzel = tk.Tk()
    wurzel.withdraw()
    # Die Knöpfe der System-Abfragen auf die Programmsprache bringen. Muss nach
    # dem Tk-Start stehen und vor der ersten Abfrage — der Assistent kann schon
    # eine zeigen.
    sprache.knoepfe_eindeutschen(wurzel)
    fehler.spur('Tk-Wurzel steht')

    zeige_liste = False
    if assistent.noetig():
        fehler.spur('Assistent beginnt')
        fertig, zeige_liste = assistent.starten(eltern=wurzel)
        fehler.spur('Assistent fertig (Liste zeigen: %s)' % zeige_liste)
        if not fertig and not assistent.eingerichtet():
            # ⚠⚠ **Abbrechen beendet nur beim ECHTEN ersten Start.**
            # Bis rc44 beendete jeder Abbruch das Programm — und zwar
            # wortlos. Wer schon eingerichtet war und den unerwarteten
            # Assistenten einfach zumachte, hatte danach gar nichts: kein
            # Overlay, keine Meldung, nichts im Fehlerbericht. Genau so am
            # 30.08.2026 gemeldet („nun läuft er, sehe ihn aber nirgends" —
            # er lief nicht mehr).
            #
            # Ist das Werkzeug schon eingerichtet, ist der Assistent nur ein
            # Angebot. Wer ihn wegklickt, will weiterarbeiten, nicht aufhören.
            fehler.spur('Assistent abgebrochen — erster Start, Ende')
            sys.exit(0)
        if not fertig:
            fehler.spur('Assistent abgebrochen — weiter mit dem Overlay')
    fehler.spur('Overlay wird gebaut')
    fenster = Overlay(wurzel=wurzel)
    fehler.spur('Overlay steht')
    if zeige_liste:
        fehler.spur('Bauplan-Liste wird geöffnet')
        fenster.liste_oeffnen()
        fehler.spur('Bauplan-Liste steht')
    fehler.spur('Hauptschleife läuft')
    fenster.run()
