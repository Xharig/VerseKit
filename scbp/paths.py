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
Wo liegt was — je nach Betriebssystem.

Das ist die einzige Stelle im Programm, die Windows- und Linux-Pfade kennt.
Alles andere fragt hier nach und muss nicht wissen, auf welchem System es läuft.

Drei Sorten Pfade:

  1. **Eigene Dateien** (Einstellungen, Bestand, Lesestand)
     Windows:  %APPDATA%\\sc-bp-watcher\\
     Linux:    ~/.config/sc-bp-watcher/     (bzw. $XDG_CONFIG_HOME)

  2. **Star Citizen selbst** (Game.log und die Sicherungen)
     Windows:  C:\\Program Files\\Roberts Space Industries\\StarCitizen\\LIVE\\
     Linux:    im Wine-Präfix, z. B. ~/Games/star-citizen/drive_c/Program Files/…

  3. **SC Deutsch Launcher** (optional, nur wenn vorhanden)
     Er ist kein Muss — wenn er da ist, wird er weiter genutzt,
     weil er den vollständigen Bestand und einen gepflegten Katalog liefert.

**Wer die Sachen woanders liegen hat, trägt die Pfade selbst ein.** Dafür gibt es
`einstellungen.json` im eigenen Ordner:

    {
      "spiel_ordner":    "/mnt/spiele/StarCitizen/LIVE",
      "launcher_ordner": "D:\\SCDL\\blueprints"
    }

Die Datei wird angelegt, sobald das Spiel nicht gefunden wird — mit Kommentar
und leeren Feldern zum Ausfüllen. Ein leeres Feld heißt „bitte suchen".

Rangfolge, wenn mehrere Angaben da sind:

  1. Umgebungsvariable — `SC_BP_HOME`, `SC_INSTALL_DIR`, `SC_BP_LAUNCHER`
     (für einen einmaligen Sonderfall, ohne etwas zu ändern)
  2. `einstellungen.json` — der normale Weg für einen dauerhaft eigenen Pfad
  3. die Suche an den üblichen Stellen
"""
import glob
import json
import os
import re
import sys

WINDOWS = sys.platform.startswith('win')

# Spielkanäle in der Reihenfolge, in der gesucht wird. LIVE zuerst — wer PTU
# spielt, hat meist beides installiert, gemeint ist aber fast immer LIVE.
#
# ⚠⚠ **HOTFIX gehört dazu.** Legt CIG neben LIVE eine ausgebesserte Fassung auf
# denselben Server, ist das eine eigene Installation daneben. Und der übliche
# Weg dorthin ist nicht „nochmal 100 GB laden", sondern: **den vorhandenen
# LIVE-Ordner in HOTFIX umbenennen**, damit der Launcher nur die Unterschiede
# holt. Damit ist der eingetragene Spielordner von einem Tag auf den anderen
# **weg** — und der Watcher fand gar nichts mehr, weil HOTFIX in dieser Liste
# fehlte. Genau so gesehen am 03.09.2026 bei Haldjas: eingetragen war
# `…\StarCitizen\LIVE`, im Bericht standen „Spiel nicht gefunden", keine
# Game.log und 0 gelesene Protokolle.
CHANNELS = ('LIVE', 'HOTFIX', 'PTU', 'EPTU', 'TECH-PREVIEW')

# ⚠⚠ **Nur diese beiden teilen sich EINEN Bauplan-Bestand.**
#
# Am 05.09.2026 richtiggestellt, nachdem der erste Anlauf alle Kanäle
# einbezogen hatte: „PTU EPTU und TECHNICAL-PREVIEW müssen wir aber ignorieren
# ausschließen, die BP Bestände sind im LIVE und HOTFIX nicht verfügbar, die
# sind getrennt davon."
#
# Das ist eine Regel des Spiels, keine Vermutung: Die Testumgebungen laufen auf
# eigenen Spielständen. Wer dort Baupläne freischaltet, hat sie auf LIVE
# **nicht** — sie mitzulesen würde also einen Bestand behaupten, den es nicht
# gibt. Und ein zu viel eingetragener Bauplan ist schlimmer als ein fehlender:
# Man plant damit und steht dann ohne da.
#
# HOTFIX ist der Sonderfall, für den es diese Liste überhaupt gibt — er läuft
# auf demselben Spielstand wie LIVE und wird nur angelegt, wenn eine Ausbesserung
# neben der laufenden Fassung erscheint.
SHARED_STOCK_CHANNELS = ('LIVE', 'HOTFIX')

# Unterpfad ab dem Wurzelverzeichnis eines Laufwerks bis zum Spielkanal.
SC_SUBPATH = os.path.join('Roberts Space Industries', 'StarCitizen')


# ------------------------------------------------------------ 1. Eigene Dateien
# Wohin welche Datei gehört. Zweck: Wer den Ordner öffnet, soll sehen, was
# seins ist — Baupläne und Ausgaben getrennt vom technischen Kleinkram.
# Was hier nicht steht, landet in „Intern"; das sind Zwischenspeicher und
# Lesestände, die niemanden interessieren.
SUBFOLDERS = {
    'bestand.json':      'Bauplaene',
    'bestand.bak.json':  'Bauplaene',
    'merkliste.json':    'Bauplaene',
    'watchlist.json':    'Bauplaene',
    'catalog-seen.json': 'Bauplaene',
    'bp-overrides.json': 'Bauplaene',
    # Das Auftrags-Protokoll wird fortgeschrieben, nicht neu gebaut: Die
    # Game-Logs reichen nur wenige Sitzungen zurueck, das Protokoll soll bleiben.
    'auftragslog.json':     'Intern',
    'auftragslog.bak.json': 'Intern',
    'einstellungen.json': 'Einstellungen',
    'phrasen.json':       'Einstellungen',
    'gesehen.json':       'Einstellungen',
    'fehler.json':        'Diagnose',
    'bericht.txt':        'Diagnose',
    # Das Protokoll des Setups beim Selbst-Update. Gehoert zur Diagnose:
    # Meldet jemand 'das Update geht nicht', steht hier, woran es lag.
    'update-setup.txt':   'Diagnose',
    # Das Protokoll des Update-Helfers — der letzte Versuch und der davor.
    'update-helfer.txt':   'Diagnose',
    'update-helfer.1.txt': 'Diagnose',
}
FOLDER_NAME = 'SC BP Watcher'
SETTINGS_FILE = 'einstellungen.json'

# ⚠⚠⚠ Die Dateinamen, unter denen eine Datei UNS gehört — beide dauerhaft.
#
# Steht hier, weil **drei** Stellen sie brauchen und sie nie auseinanderlaufen
# dürfen:
#   * `updater.eigenes_appimage()` — erkennt das laufende AppImage
#   * der Riegel in `updater.einspielen()` — überschreibt nie Fremdes
#   * `update_run._aufraeumen()` — löscht nur den eigenen Installer
#
# Warum beide Namen (Umbenennung zu VerseKit, 12.09.2026):
#   `sc-bp-watcher` — so heißen die Dateien bei jedem, der vorher installiert
#     hat. Ein AppImage wird beim Update an seinem Platz ersetzt und behält
#     seinen Namen.
#   `versekit` — so heißen die Release-Dateien für alle, die neu dazukommen.
#
# ⛔ Keinen streichen. Wer den alten entfernt, lässt die Hälfte der Nutzer
# stehen; wer den neuen entfernt, die andere Hälfte.
OWN_FILENAMES = ('sc-bp-watcher', 'versekit')


def is_ours(filename):
    """Gehört diese Datei zu diesem Programm? (nach ihrem NAMEN)

    Der Maßstab ist der Dateiname, nicht der Pfad — beim AppImage ist das die
    einzige verlässliche Angabe, weil `sys.executable` im Einhängepunkt liegt.
    """
    lower = os.path.basename(filename or '').lower()
    return any(n in lower for n in OWN_FILENAMES)


def _documents():
    """Der Dokumente-Ordner des Nutzers — oder das Heimatverzeichnis."""
    home = os.path.expanduser('~')
    if WINDOWS:
        # Der Ordner kann umbenannt oder verschoben sein; die Registry weiß es.
        try:
            import winreg
            key_path = (r'Software\Microsoft\Windows\CurrentVersion'
                          r'\Explorer\Shell Folders')
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, key_path) as k:
                value = winreg.QueryValueEx(k, 'Personal')[0]
                if value and os.path.isdir(os.path.expandvars(value)):
                    return os.path.expandvars(value)
        except Exception:
            pass
    else:
        # Unter Linux sagt es die XDG-Angabe; sie ist übersetzt („Dokumente").
        try:
            config_file = os.path.join(os.environ.get('XDG_CONFIG_HOME')
                                  or os.path.join(home, '.config'),
                                  'user-dirs.dirs')
            with open(config_file, encoding='utf-8') as f:
                for line in f:
                    if line.startswith('XDG_DOCUMENTS_DIR'):
                        value = line.split('=', 1)[1].strip().strip('"')
                        value = value.replace('$HOME', home)
                        if os.path.isdir(value):
                            return value
        except Exception:
            pass
    for name in ('Documents', 'Dokumente'):
        p = os.path.join(home, name)
        if os.path.isdir(p):
            return p
    return home


def legacy_app_folder():
    """Wo die Dateien bis v2.x lagen — Rückfall und Quelle für den Umzug."""
    if WINDOWS:
        return os.path.join(os.environ.get('APPDATA', os.path.expanduser('~')),
                            'sc-bp-watcher')
    base = os.environ.get('XDG_CONFIG_HOME') or os.path.expanduser('~/.config')
    return os.path.join(base, 'sc-bp-watcher')


def _second_pointer():
    """Der Ablage-Ort ein zweites Mal — dort, wo niemand aufraeumt.

    ⚠⚠⚠ **Warum es diese Datei gibt.** Der Ablage-Ort haengt an einer einzigen
    Datei mit einer einzigen Zeile, sichtbar unter Dokumente. Am 06.09.2026
    wurde sie beim Aufraeumen im Dateimanager mit weggeworfen — zusammen mit
    zwei danebenliegenden Altstaenden, die wirklich Muell waren und fast
    genauso hiessen:

        SC BP Watcher                        <- der aktive Zeiger
        SC BP Watcher (vor Umzug 2026-08-31) <- Altlast
        SC-BP-Watcher-SICHERUNG-vor-v2-test  <- Altlast

    Danach schaute das Programm wieder in den Standardordner, legte dort einen
    leeren Bestand an und zeigte statt 413 Bauplaenen nur noch die 406, die es
    zufaellig vorfand — **ohne ein Wort**. Der Spieler sah nur eine kleinere
    Zahl und konnte sich nicht erklaeren, wieso: *„wieso aendert sich immer
    wieder der Ordner, die ganze Zeit hat es doch geklappt?"*

    Der zweite Zeiger liegt im Konfigurationsordner (`~/.config` bzw.
    `%APPDATA%`) — dort raeumt niemand mit dem Dateimanager auf, und er faellt
    beim Sortieren von Dokumenten nicht ins Auge. **Beide Dateien heilen
    einander:** Fehlt eine, wird sie aus der anderen wieder angelegt.
    """
    if os.name == 'nt':
        base = os.environ.get('APPDATA') or os.path.expanduser('~')
    else:
        base = (os.environ.get('XDG_CONFIG_HOME')
                 or os.path.join(os.path.expanduser('~'), '.config'))
    return os.path.join(base, 'sc-bp-watcher', 'ablage.json')


def _read_pointer(pointer_path):
    """Den Ablage-Ort aus einer Zeiger-Datei holen, oder `None`."""
    try:
        if not os.path.isfile(pointer_path):
            return None
        with open(pointer_path, encoding='utf-8') as f:
            value = json.load(f).get('ablage_ordner')
        if not isinstance(value, str) or not value.strip():
            return None
        return value.strip()
    except Exception:
        return None


def _write_pointer(pointer_path, value):
    """Einen Zeiger anlegen — still, denn er ist nur die Zweitschrift."""
    try:
        os.makedirs(os.path.dirname(pointer_path), exist_ok=True)
        temp = pointer_path + '.tmp'
        with open(temp, 'w', encoding='utf-8') as f:
            json.dump({'ablage_ordner': value}, f, ensure_ascii=False, indent=2)
        os.replace(temp, pointer_path)
        return True
    except Exception:
        return False


def _storage_from_file():
    """Einen selbst gewählten Ablage-Ort lesen — **ohne** `setting()`.

    ⚠ Hier lauert eine Schleife: `setting()` liest ihre Datei über
    `app_file()`, und das fragt wieder `app_folder()`. Wer an dieser Stelle die
    normale Einstellungs-Funktion benutzt, baut eine Endlosrekursion — die
    obendrein unsichtbar bleibt, weil ringsherum `try/except` steht. Deshalb
    wird die Datei hier am Standardort direkt gelesen.

    ⚠⚠ **Zwei Zeiger, und sie heilen einander** (siehe `_zweitzeiger`). Gelesen
    wird zuerst der unter Dokumente, weil der Spieler ihn dort auch von Hand
    aendern kann. Fehlt er, springt der zweite ein — und legt den ersten
    wieder an. Der Ordner muss dabei **existieren**: Ein Zeiger auf einen Ort,
    den es nicht mehr gibt, ist schlimmer als keiner, denn er wuerde einen
    leeren Ordner an falscher Stelle erzeugen.
    """
    first = pointer_file()
    second = _second_pointer()
    a, b = _read_pointer(first), _read_pointer(second)

    for value, missing_at in ((a, second), (b, first)):
        if value is None:
            continue
        if not os.path.isdir(os.path.expanduser(value)):
            # ⚠ Zeigt ins Leere — etwa weil eine externe Platte nicht
            # eingehaengt ist. Dann lieber den anderen fragen.
            continue
        if _read_pointer(missing_at) != value:
            _write_pointer(missing_at, value)
        return value

    # Keiner von beiden taugt: den ersten unveraendert zurueckgeben, damit
    # eine Platte, die gerade nicht da ist, nicht stillschweigend verfaellt.
    return a or b


def app_folder():
    """Ordner für unsere eigenen Dateien. Wird bei Bedarf angelegt.

    Seit v3.0.0 liegt er **sichtbar** unter Dokumente statt versteckt in
    `%APPDATA%` bzw. `~/.config` — dort sucht kein normaler Spieler, und seinen
    Bauplan-Bestand sollte er finden können. Ein eigener Ort geht weiterhin über
    `SC_BP_HOME` oder die Einstellung `ablage_ordner`.
    """
    custom = os.environ.get('SC_BP_HOME') or _storage_from_file()
    p = (os.path.expanduser(custom) if custom
         else os.path.join(_documents(), FOLDER_NAME))
    try:
        os.makedirs(p, exist_ok=True)
    except OSError:
        pass
    return p


def bundled_file(name):
    """Voller Pfad zu einer **mitgelieferten** Datei aus dem Ordner `daten/`.

    Das sind Dateien, die zum Programm gehören und nur gelesen werden — im
    Gegensatz zu `app_file()`, wo die Daten des Nutzers liegen.

    ⚠ Zwei Fälle: Läuft das Programm aus dem Quellcode, liegt `daten/` neben
    `scbp/`. Ist es zu einer Datei gepackt (PyInstaller, AppImage), entpackt
    sich alles in einen Wegwerf-Ordner, dessen Pfad in `sys._MEIPASS` steht.
    Wer das nicht abfängt, sucht im gepackten Programm an der falschen Stelle
    und findet nichts."""
    bundled = getattr(sys, '_MEIPASS', None)
    base = bundled or os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    return os.path.join(base, 'daten', name)


def app_file(name):
    """Voller Pfad zu einer eigenen Datei, z. B. app_datei('bestand.json').

    Sortiert nach `SUBFOLDERS` in Unterordner ein. Wer ein `SC_BP_HOME` gesetzt
    hat (Selbsttest, Sonderfälle), bekommt den flachen Ordner von früher — dort
    geht es um Wegwerf-Ordner, nicht um Übersicht.
    """
    base = app_folder()
    if os.environ.get('SC_BP_HOME'):
        return os.path.join(base, name)
    subfolder = SUBFOLDERS.get(name, 'Intern')
    target = os.path.join(base, subfolder)
    try:
        os.makedirs(target, exist_ok=True)
    except OSError:
        return os.path.join(base, name)
    return os.path.join(target, name)


def migration_needed():
    """Liegen im alten Ordner Dateien, die im neuen fehlen?"""
    old = legacy_app_folder()
    if not os.path.isdir(old) or os.environ.get('SC_BP_HOME'):
        return False
    try:
        existing = [n for n in os.listdir(old) if n.endswith(('.json', '.txt'))]
    except OSError:
        return False
    if not existing:
        return False
    # Schon umgezogen? Dann liegt der Bestand am neuen Ort.
    return not os.path.exists(app_file('bestand.json'))


def migrate():
    """Die Dateien aus dem alten Ordner in den neuen **kopieren**.

    Kopieren, nicht verschieben: Geht beim Umzug etwas schief — Rechte, ein
    Virenscanner, ein abgebrochener Start — ist der mühsam gesammelte
    Bauplan-Bestand sonst weg. Der alte Ordner bleibt unangetastet liegen; er
    kostet ein paar Kilobyte und ist der Rückweg.

    Gibt die Zahl der kopierten Dateien zurück.
    """
    import shutil
    old = legacy_app_folder()
    copied = 0
    try:
        names = sorted(os.listdir(old))
    except OSError:
        return 0
    for name in names:
        source = os.path.join(old, name)
        if not os.path.isfile(source):
            continue
        target = app_file(name)
        if os.path.exists(target):
            continue                     # nichts überschreiben
        try:
            os.makedirs(os.path.dirname(target), exist_ok=True)
            shutil.copy2(source, target)
            copied += 1
        except OSError:
            pass
    return copied


# ------------------------------------------- Ablage-Ordner wechseln (v3.19.0)

# Was beim Wechsel mitkommt. Alles andere im Ordner geht uns nichts an — wer
# seinen Ablage-Ordner auf einen Ordner legt, in dem noch etwas anderes liegt,
# soll das behalten dürfen.
MOVE_EXTENSIONS = ('.json', '.txt')


def _storage_files(folder):
    """Alle eigenen Dateien eines Ablage-Ordners, mit ihrem Unterordner.

    Gibt Paare `(voller Pfad, Name)` zurück — **rekursiv**, weil die Ablage
    seit v3.0.0 nach `Bauplaene/`, `Einstellungen/`, `Diagnose/` und `Intern/`
    sortiert. Ein flacher Durchlauf fände dort **nichts** und meldete „nichts zu
    tun", während der ganze Bestand danebenliegt.
    """
    result = []
    if not os.path.isdir(folder):
        return result
    for root, _subdirs, files in os.walk(folder):
        for name in files:
            if name.endswith(MOVE_EXTENSIONS):
                result.append((os.path.join(root, name), name))
    return result


def storage_status(target):
    """Was am Zielort los ist — bevor irgendetwas angefasst wird.

    Gibt `(schreibbar, eigene_dateien, grund)` zurück:

    | | |
    |---|---|
    | `schreibbar` | lässt sich dort überhaupt etwas anlegen |
    | `eigene_dateien` | wie viele Dateien dort schon liegen (also ein Bestand) |
    | `grund` | Klartext, warum nicht schreibbar — sonst `''` |

    ⚠ **Vorher prüfen, nicht beim Scheitern.** Ein Wechsel, der auf halber
    Strecke an den Rechten hängenbleibt, hinterlässt die Hälfte am neuen und
    die Hälfte am alten Ort — und der Nutzer weiß von beidem nichts.
    """
    try:
        os.makedirs(target, exist_ok=True)
    except OSError as exc:
        return False, 0, str(exc)
    probe = os.path.join(target, '.schreibprobe')
    try:
        with open(probe, 'w', encoding='utf-8') as f:
            f.write('x')
        os.remove(probe)
    except OSError as exc:
        # ⚠ Der häufigste Fall bei Doppelstart: eine Systemplatte, die im
        # anderen System **nur lesend** eingehängt ist. Sie sieht aus wie ein
        # gültiger Ordner, lässt sich auswählen — und nimmt nichts an.
        return False, 0, str(exc)
    return True, len(_storage_files(target)), ''


def _checksum(path):
    """SHA-256 einer Datei — oder `''`, wenn sie sich nicht lesen lässt."""
    import hashlib
    digest = hashlib.sha256()
    try:
        with open(path, 'rb') as f:
            for block in iter(lambda: f.read(65536), b''):
                digest.update(block)
    except OSError:
        return ''
    return digest.hexdigest()


def move_storage(source_dir, target_dir):
    """Die eigenen Dateien in den neuen Ablage-Ordner kopieren — geprüft.

    Gibt `(kopiert, uebersprungen, misslungen)` zurück.

    ⚠⚠ **Kopieren, nicht verschieben, und nichts löschen.** Der alte Ordner
    bleibt vollständig liegen. Er kostet ein paar hundert Kilobyte und ist der
    einzige Rückweg, wenn beim Wechsel etwas schiefgeht — ein Bauplan-Bestand
    ist Monate an Spielzeit, kein Zwischenspeicher. Dieselbe Überlegung wie bei
    `umziehen()` weiter oben.

    ⚠⚠ **Jede Datei wird nach dem Kopieren gegengeprüft** (SHA-256). Ohne das
    heißt „kopiert" nur „`copy2` hat nicht geworfen" — bei einer vollen Platte
    oder einer Netzfreigabe, die mittendrin abbricht, liegt am Ziel eine halbe
    Datei, und niemand merkt es. Eine kaputte `bestand.json` fällt erst beim
    nächsten Start auf, und dann ist der alte Ordner vielleicht schon weg.

    ⚠ **Vorhandenes am Ziel wird NICHT überschrieben.** Wer auf einen Ordner
    wechselt, in dem schon ein Bestand liegt (der zweite Rechner beim
    Doppelstart), will dessen Daten behalten — das Zusammenführen ist ein
    eigener Vorgang, kein Nebeneffekt eines Pfadwechsels.
    """
    import shutil
    copied = skipped = failed = 0
    if not os.path.isdir(source_dir) or os.path.abspath(source_dir) == os.path.abspath(target_dir):
        return 0, 0, 0
    for source, name in _storage_files(source_dir):
        subfolder = SUBFOLDERS.get(name, 'Intern')
        target = os.path.join(target_dir, subfolder, name)
        if os.path.exists(target):
            skipped += 1
            continue
        try:
            os.makedirs(os.path.dirname(target), exist_ok=True)
            shutil.copy2(source, target)
        except OSError as exc:
            _report_error('paths.move_storage.' + name, exc)
            failed += 1
            continue
        # Die Gegenprobe. Stimmt sie nicht, gilt die Datei als misslungen und
        # die halbe Kopie wird weggeräumt — sie wäre schlimmer als keine.
        if _checksum(source) != _checksum(target):
            failed += 1
            try:
                os.remove(target)
            except OSError:
                pass
            continue
        copied += 1
    return copied, skipped, failed


# ------------------------------------------------------- Selbst gesetzte Pfade

def searched_game_locations(limit=6):
    """Die Orte, an denen tatsächlich nach Star Citizen gesucht wird.

    Wird dem Nutzer angezeigt, wenn nichts gefunden wurde. Ohne diese Angabe
    weiß er nicht, wonach er suchen soll — und ein Pfad, den er selbst eintragen
    soll, ist ohne Vorbild schwer zu erraten. Vorhandene Ordner kommen zuerst:
    Wer seine Installation dort halb wiederfindet, sieht sofort, wie der Rest
    aussehen muss."""
    candidates = []
    for root in _game_roots():
        p = os.path.join(root, SC_SUBPATH, CHANNELS[0])
        if p not in candidates:
            candidates.append(p)
    # Wenn gar nichts zusammenkommt, sind auf diesem Rechner weder Wine-Präfixe
    # noch Programmordner da — und ausgerechnet dann braucht der Nutzer das
    # Vorbild am dringendsten. Also die typischen Orte zeigen, auch wenn es sie
    # hier nicht gibt.
    if not candidates:
        home = os.path.expanduser('~')
        if WINDOWS:
            candidates = [os.path.join('C:\\Program Files', SC_SUBPATH,
                                       CHANNELS[0])]
        else:
            for prefix in (os.path.join(home, 'Games', 'star-citizen'),
                            os.path.join(home, '.wine')):
                candidates.append(os.path.join(prefix, 'drive_c', 'Program Files',
                                               SC_SUBPATH, CHANNELS[0]))
            candidates.append(os.path.join(
                home, '.local', 'share', 'lutris', 'prefixes', '<Name>', 'drive_c',
                'Program Files', SC_SUBPATH, CHANNELS[0]))
    # existierende zuerst, Reihenfolge sonst beibehalten
    da = [p for p in candidates if os.path.isdir(os.path.dirname(p))]
    rest = [p for p in candidates if p not in da]
    return (da + rest)[:limit]


def searched_launcher_locations(limit=3):
    """Dasselbe für den Blueprint-Ordner des SC Deutsch Launchers."""
    if WINDOWS:
        return [os.path.join(os.environ.get('APPDATA', '%APPDATA%'),
                             'sc-deutsch-launcher', 'blueprints')]
    places = list(_windows_launcher())
    for prefix in _wine_prefixes()[:limit]:
        places.append(os.path.join(prefix, 'drive_c', 'users', '<Benutzer>',
                                 'AppData', 'Roaming', 'sc-deutsch-launcher',
                                 'blueprints'))
    return places[:limit]


def _template():
    """Der Inhalt der Einstellungsdatei — mit den echten Suchorten dieses Rechners.

    Die Hinweiszeilen stehen bewusst **direkt unter** dem jeweiligen Feld: In
    einer JSON-Datei gibt es keine ausgegraute Beschriftung, das Nächstliegende
    ist ein Feld daneben, das man beim Ausfüllen zwangsläufig liest. Sie werden
    nicht ausgewertet — was drinsteht, ändert nichts."""
    return {
        '_hinweis': 'Eigene Pfade eintragen, wenn Star Citizen oder der '
                    'SC Deutsch Launcher nicht an den ueblichen Stellen liegen. '
                    'Leeres Feld = automatisch suchen. Nach dem Aendern den '
                    'Watcher neu starten. Zeilen mit _ sind nur Erklaerung.',
        'spiel_ordner': '',
        '_spiel_ordner_gemeint_ist': 'Der Ordner, in dem die Game.log liegt — '
                                     'meist "LIVE".',
        '_spiel_ordner_gesucht_wird_hier': searched_game_locations(),
        'sprache': 'auto',
        '_sprache_moeglich': 'auto (Systemsprache), de, en',
        'pruefintervall_sekunden': 3,
        '_pruefintervall_gemeint_ist': 'Wie oft die Game.log angesehen wird. '
                                       'Erlaubt 1 bis 60.',
        'signalton': True,
        '_signalton_gemeint_ist': 'Kurzer Ton, wenn ein Bauplan erscheint.',
        'deckkraft_prozent': 93,
        '_deckkraft_gemeint_ist': 'Wie undurchsichtig das Fenster ist. 100 = '
                                  'blickdicht, 30 = stark durchscheinend. '
                                  'Erlaubt 30 bis 100.',
        'launcher_ordner': '',
        '_launcher_ordner_gemeint_ist': 'Optional. Der Ordner "blueprints" des '
                                        'SC Deutsch Launchers. Ohne ihn laeuft '
                                        'der Watcher trotzdem.',
        '_launcher_ordner_gesucht_wird_hier': searched_launcher_locations(),
    }


def _report_error(label, exc):
    """Einen Fehler ins Protokoll geben, ohne dabei selbst zu scheitern.

    ⚠ Der Import steht **absichtlich** in der Funktion: `scbp/fehler.py`
    importiert seinerseits `paths`. Auf Modulebene wäre das ein Zirkelbezug und
    keines der beiden Module ließe sich mehr laden.

    ⚠ Und das Melden hängt in einem eigenen `try`: Wenn schon das Schreiben der
    Einstellungen scheitert, kann auch das Fehlerprotokoll klemmen. Dann ist der
    ursprüngliche Fehler zwar verloren — aber das Programm läuft weiter, und
    darum geht es.
    """
    try:
        from . import fehler
        fehler.merken(label, exc)
    except Exception:
        pass


def settings():
    """Die selbst eingetragenen Pfade. Fehlt die Datei, ist sie leer."""
    try:
        with open(app_file(SETTINGS_FILE), encoding='utf-8') as f:
            data = json.load(f)
        return data if isinstance(data, dict) else {}
    except Exception:
        return {}


def setting(name):
    """Ein einzelner selbst gesetzter Pfad — oder None, wenn nichts eingetragen ist.

    ⚠⚠ **Alles, was kein Text ist, gilt als „nicht gesetzt".** Vorher stand
    hier `(… or '').strip()`. Bei einem Ja/Nein-Wert überlebt `True` das
    `or ''` und `.strip()` fliegt mit einem AttributeError — und zwar nicht
    leise: Am 03.09.2026 hat ein einziger falscher Aufruf
    (`setting('lager_raffinerie_offen')` statt `setting_bool`) den
    Aufbau der ganzen Lager-Seite abgerissen. Die Liste der Posten fehlte,
    die Daten waren unversehrt, und weil eine Seite nur einmal gebaut wird,
    half auch Zuklappen nicht mehr.

    Ein Pfad ist immer Text. Kommt etwas anderes, ist das ein Aufruf an der
    falschen Adresse — dann `None` zurückzugeben ist richtig und kostet
    niemanden eine Seite. Für Ja/Nein gibt es `setting_bool`, für
    Zahlen `setting_int`.
    """
    value = settings().get(name)
    if not isinstance(value, str):
        return None
    value = value.strip()
    return os.path.expanduser(value) if value else None


def save_json(target, data, indent=1, sort_keys=False):
    """JSON schreiben — ohne Halbfertiges und mit Vorgängerfassung.

    Erst in eine Nebendatei, dann umbenennen: Stürzt der Rechner mitten im
    Schreiben ab, ist die alte Datei noch vollständig da. Die vorige Fassung
    bleibt als `….bak.json` liegen.

    ⚠ **Warum das hier steht und nicht in jedem Modul einzeln.** `collection.py`
    hatte diese Sicherung von Anfang an, die beiden Lager (Werkstatt und
    Handel) nicht — sie schrieben zwar atomar, aber ohne Rückfall. Genau dort
    stehen **eigene Eingaben**, die es nirgends sonst zu holen gibt: Ein
    Bauplan-Bestand liesse sich aus der Game.log neu aufbauen, ein Lager nicht.
    Zwei Fassungen derselben Regel gehen irgendwann auseinander, deshalb eine.

    ⚠ **Meldet nichts selbst.** Der Aufrufer weiss, unter welchem Namen der
    Fehler ins Protokoll gehört (`materials.save` gegen
    `trade_cargo.save`) — und `paths` darf `fehler` nicht einbinden, das
    gäbe einen Ringschluss. Bei einem Fehlschlag fliegt die Ausnahme; die
    Nebendatei ist dann schon weggeräumt.
    """
    temp = target + '.tmp'
    try:
        folder = os.path.dirname(target)
        if folder:
            os.makedirs(folder, exist_ok=True)
        with open(temp, 'w', encoding='utf-8') as f:
            json.dump(data, f, ensure_ascii=False, indent=indent,
                      sort_keys=sort_keys)
        if os.path.exists(target):
            # ⚠ Nicht `ziel.replace('.json', …)` — das trifft auch einen
            # Ordnernamen, in dem „.json" vorkommt. Nur die Endung zählt.
            backup = (target[:-5] + '.bak.json' if target.endswith('.json')
                         else target + '.bak')
            try:
                os.replace(target, backup)
            except OSError:
                # Kein Grund abzubrechen: Die Vorgängerfassung ist der Gürtel,
                # das atomare Schreiben der Hosenträger. Einer genügt.
                pass
        os.replace(temp, target)
        return True
    except Exception:
        try:
            os.remove(temp)
        except OSError:
            pass
        raise


def pointer_file():
    """Die Datei, aus der der Ablage-Ort gelesen wird — immer am Standardort.

    ⚠ Sie ist NICHT dieselbe wie die Einstellungsdatei in der Ablage, sobald
    ein eigener Ablage-Ort gesetzt ist. `_storage_from_file()` liest ausschliesslich
    hier; alles andere steht in der Ablage selbst.
    """
    return os.path.join(_documents(), FOLDER_NAME, 'Einstellungen',
                        SETTINGS_FILE)


def _set_storage_folder(value):
    """Den Ablage-Ort in die Zeiger-Datei schreiben — nicht in die Ablage.

    ⚠⚠ **Sonst merkt sich das Programm den neuen Ort an einer Stelle, die es
    nie wieder liest.** `set_setting()` schreibt ueber `app_file()`,
    und das zeigt in den **aktuellen** Ablage-Ordner. Der neue Ort landete
    damit in der Einstellungsdatei des ALTEN Ordners, waehrend
    `_storage_from_file()` weiter den unveraenderten Zeiger unter Dokumente las.
    Ergebnis: Die Umstellung liess sich speichern, war aber nach jedem Neustart
    wieder weg — gemeldet am 04.09.2026, „bei jedem Neustart ist der alte Pfad
    wieder drin".

    In der Zeiger-Datei steht **nur** dieses eine Feld. Zwei befuellte
    Einstellungsdateien wuerden garantiert auseinanderlaufen, und gelesen wird
    hier ohnehin nichts anderes.
    """
    target = pointer_file()
    existing = {}
    try:
        with open(target, encoding='utf-8') as f:
            loaded = json.load(f)
        if isinstance(loaded, dict):
            existing = loaded
    except Exception:
        pass
    existing['ablage_ordner'] = value
    temp = target + '.tmp'
    try:
        os.makedirs(os.path.dirname(target), exist_ok=True)
        with open(temp, 'w', encoding='utf-8') as f:
            json.dump(existing, f, ensure_ascii=False, indent=2)
        os.replace(temp, target)
        # ⚠⚠ **Immer beide schreiben** (siehe `_zweitzeiger`). Wuerde nur der
        # sichtbare gepflegt, waere die Zweitschrift nach der ersten Umstellung
        # veraltet — und wenn sie dann einspringt, landet der Spieler in einem
        # Ordner, den er vor Wochen verlassen hat. Ein falscher Zeiger ist
        # schlimmer als keiner.
        _write_pointer(_second_pointer(), value)
        return True
    except OSError as exc:
        _report_error('paths._set_storage_folder', exc)
        return False


def set_setting(name, value):
    """Einen Pfad dauerhaft merken — ohne die Erklärzeilen zu verlieren.

    Gelesen wird die vorhandene Datei (oder die Vorlage), geändert nur das eine
    Feld. So bleiben die Hinweise mit den Suchorten stehen, auch wenn das
    Programm die Datei schreibt."""
    if name == 'ablage_ordner':
        # ⚠ Sonderweg, siehe `_set_storage_folder`: Dieses eine Feld gehoert
        # in die Zeiger-Datei, sonst wirkt die Umstellung nur bis zum Neustart.
        return _set_storage_folder(value)
    data = settings() or _template()
    data[name] = value
    target = app_file(SETTINGS_FILE)
    temp = target + '.tmp'
    try:
        with open(temp, 'w', encoding='utf-8') as f:
            json.dump(data, f, ensure_ascii=False, indent=2)
        os.replace(temp, target)
        return True
    except OSError as exc:
        # ⚠ Niemand prüft den Rückgabewert dieser Funktion — geprüft am
        # 26.08.2026, alle Aufrufer werfen ihn weg. Scheitert das Schreiben
        # (volle Platte, fehlende Rechte, Ordner weg), wäre die Einstellung
        # nach dem Neustart einfach wieder alt, ohne jeden Hinweis.
        _report_error('paths.set_setting', exc)
        return False


def setting_int(name, default, minimum=None, maximum=None):
    """Eine Zahl aus den Einstellungen, mit Grenzen.

    Unsinnige Werte werden auf den erlaubten Bereich gezogen statt abgelehnt:
    Wer 0 einträgt, meint „so oft wie möglich" und soll kein Programm bekommen,
    das die Platte durchdreht — aber auch keine Fehlermeldung."""
    value = settings().get(name)
    try:
        number = int(value)
    except (TypeError, ValueError):
        return default
    if minimum is not None:
        number = max(minimum, number)
    if maximum is not None:
        number = min(maximum, number)
    return number


def setting_bool(name, default):
    """Ein Ja/Nein aus den Einstellungen. Fehlt es, gilt der Standard."""
    value = settings().get(name)
    if isinstance(value, bool):
        return value
    if isinstance(value, str):
        return value.strip().lower() in ('ja', 'yes', 'true', '1', 'an', 'on')
    return default


def create_template():
    """Legt `einstellungen.json` zum Ausfüllen an, falls sie noch fehlt.

    Passiert genau dann, wenn das Spiel nicht gefunden wurde: Dann braucht der
    Nutzer die Datei, und sie soll schon dastehen, statt dass er sie nach
    Anleitung selbst erzeugen muss."""
    target = app_file(SETTINGS_FILE)
    if os.path.exists(target):
        return target
    try:
        with open(target, 'w', encoding='utf-8') as f:
            json.dump(_template(), f, ensure_ascii=False, indent=2)
    except OSError:
        pass
    return target


# --------------------------------------------------------- 2. Star Citizen selbst
def _wine_prefixes():
    """Mögliche Wine-Präfixe unter Linux — die Orte, an denen die verbreiteten
    Installationswege (lug-helper, Lutris, Bottles, Heroic) landen. Reihenfolge
    ist Absicht: der lug-helper-Standard zuerst, er ist unter Linux der übliche Weg."""
    home = os.path.expanduser('~')
    fixed = [
        os.path.join(home, 'Games', 'star-citizen'),
        os.path.join(home, '.wine'),
        os.path.join(home, 'Games', 'star-citizen-live'),
    ]
    patterns = [
        os.path.join(home, '.local', 'share', 'lutris', 'prefixes', '*'),
        os.path.join(home, '.var', 'app', 'net.lutris.Lutris', 'data', 'lutris',
                     'prefixes', '*'),
        os.path.join(home, '.local', 'share', 'bottles', 'bottles', '*'),
        os.path.join(home, 'Games', '*'),
    ]
    found = list(fixed)
    for m in patterns:
        found.extend(sorted(glob.glob(m)))
    # Doppelte raus, Reihenfolge behalten
    seen, unique = set(), []
    for p in found:
        if p not in seen:
            seen.add(p)
            unique.append(p)
    return unique


def _game_roots():
    """Verzeichnisse, unter denen `Roberts Space Industries\\StarCitizen` liegen kann."""
    if WINDOWS:
        roots = []
        for drive in 'CDEFGH':
            for programs_dir in ('Program Files', 'Program Files (x86)'):
                roots.append('%s:\\%s' % (drive, programs_dir))
        return roots
    roots = []
    for prefix in _wine_prefixes():
        c = os.path.join(prefix, 'drive_c')
        if not os.path.isdir(c):
            continue
        roots.append(os.path.join(c, 'Program Files'))
        roots.append(os.path.join(c, 'Program Files (x86)'))
        roots.append(c)                      # manche installieren direkt nach C:\
    return roots


def game_folder():
    """Ordner des Spielkanals (enthält die Game.log) oder None.

    Erst die Umgebungsvariable, dann die üblichen Orte. Es wird nur nachgesehen,
    ob die Game.log dort liegt — geraten wird nicht."""
    for custom in (os.environ.get('SC_INSTALL_DIR'), setting('spiel_ordner')):
        if not custom:
            continue
        custom = os.path.expanduser(custom)
        if os.path.isfile(os.path.join(custom, 'Game.log')):
            return custom
        for k in CHANNELS:                      # auch der Ordner darüber ist erlaubt
            p = os.path.join(custom, k)
            if os.path.isfile(os.path.join(p, 'Game.log')):
                return p
        # ⚠ Und **neben** ihm. Wird LIVE in HOTFIX umbenannt (der uebliche Weg
        # zu einer ausgebesserten Fassung, siehe KANAELE), zeigt der eingetragene
        # Pfad ins Leere, waehrend der Nachbarordner danebensteht. Ohne diesen
        # Zweig fand nur derjenige sein Spiel wieder, der es am Standardort
        # installiert hat — wer es auf einer zweiten Platte liegen hat, stand
        # ohne Grund vor „Star Citizen nicht gefunden".
        parent = os.path.dirname(custom.rstrip(os.sep))
        if parent and os.path.isdir(parent):
            siblings = []
            for k in CHANNELS:
                p = os.path.join(parent, k)
                log = os.path.join(p, 'Game.log')
                if not os.path.isfile(log):
                    continue
                try:
                    siblings.append((os.path.getmtime(log), p))
                except OSError:
                    siblings.append((0.0, p))
            if siblings:
                # Der zuletzt bespielte Kanal gewinnt — gemessen, nicht geraten.
                return max(siblings)[1]
    for root in _game_roots():
        base = os.path.join(root, SC_SUBPATH)
        if not os.path.isdir(base):
            continue
        for k in CHANNELS:
            p = os.path.join(base, k)
            if os.path.isfile(os.path.join(p, 'Game.log')):
                return p
    return None


def _channel_bases():
    """Ordner, in denen die Kanäle nebeneinander liegen können.

    Der eingetragene Spielordner zuerst: Wer sein Spiel auf einer anderen Platte
    hat, findet sich in den Standardorten nicht wieder — sein Nachbarkanal liegt
    aber genau eine Ebene über dem, was er eingetragen hat.
    """
    bases = []
    for custom in (os.environ.get('SC_INSTALL_DIR'), setting('spiel_ordner')):
        if not custom:
            continue
        custom = os.path.expanduser(custom).rstrip(os.sep)
        # Der eingetragene Pfad kann der Kanal selbst sein oder der Ordner
        # darüber — beide Deutungen kommen mit hinein, doppelt schadet nicht.
        for candidate in (os.path.dirname(custom), custom):
            if candidate and candidate not in bases:
                bases.append(candidate)
    for root in _game_roots():
        base = os.path.join(root, SC_SUBPATH)
        if base not in bases:
            bases.append(base)
    return bases


def available_channels():
    """Jeder Spielkanal, in dem wirklich eine Game.log liegt — neueste zuerst.

    Liefert Tupel `(Kanalname, Ordner, Zeitstempel der Game.log)`.

    ⚠ Sortiert wird nach dem **Alter der Game.log**, nicht nach der Reihenfolge
    in `KANAELE`. Wer zuletzt gespielt hat, hat dort die frischeste Datei — das
    ist eine gemessene Tatsache und keine Annahme darüber, was jemand „meint".
    """
    found = {}
    for base in _channel_bases():
        if not os.path.isdir(base):
            continue
        for k in CHANNELS:
            folder = os.path.join(base, k)
            log = os.path.join(folder, 'Game.log')
            if not os.path.isfile(log):
                continue
            if folder in found:
                continue
            try:
                stamp = os.path.getmtime(log)
            except OSError:
                stamp = 0.0
            found[folder] = (k, folder, stamp)
    return sorted(found.values(), key=lambda e: e[2], reverse=True)


def channel_mismatch():
    r"""Es wird aus einem anderen Kanal gelesen als eingetragen ist — oder None.

    ⚠⚠ **Der Fall, für den es das gibt.** Legt CIG eine ausgebesserte Fassung
    neben LIVE, ist der übliche Weg dorthin nicht ein zweiter 100-GB-Download,
    sondern: den vorhandenen LIVE-Ordner in HOTFIX umbenennen, damit der
    Launcher nur die Unterschiede holt. Von einem Moment auf den anderen gibt es
    den eingetragenen Ordner also nicht mehr.

    Ohne diese Meldung merkt das niemand: Der Watcher findet den Nachbarkanal
    zwar von allein, liest dann aber stillschweigend woanders — oder er meldet
    „Star Citizen nicht gefunden", obwohl in den Einstellungen ein Pfad steht.
    Beides sieht für den Spieler nach einem kaputten Programm aus. Gemeldet von
    Haldjas am 03.09.2026, der genau das getan und es danach vergessen hatte.

    Gibt `(eingetragen, benutzt, alle_kanaele)` zurück; `benutzt` kann None
    sein, wenn gar nichts mehr gefunden wird.
    """
    configured = setting('spiel_ordner')
    if not configured:
        return None                       # nie etwas eingetragen: nichts zu melden
    configured = os.path.expanduser(configured).rstrip(os.sep)
    if os.path.isfile(os.path.join(configured, 'Game.log')):
        return None                       # der eingetragene Ordner trägt noch
    channels = available_channels()
    if not channels:
        return None                       # gar kein Kanal da — das ist die
                                          # bekannte Meldung „nicht gefunden",
                                          # dafür braucht es keine Auswahl
    used = game_folder()
    return (configured, used, channels)


def resolve_game_folder(chosen):
    """Aus einem vom Nutzer gewählten Ordner den tatsächlichen Spielordner machen.

    Nimmt ihm die Sucherei ab: Er darf den LIVE-Ordner treffen, den darüber
    (`StarCitizen`), den Programme-Ordner oder gleich das ganze Wine-Präfix —
    solange irgendwo darunter eine `Game.log` liegt, wird sie gefunden.
    Gibt den Ordner mit der Game.log zurück oder None."""
    if not chosen:
        return None
    chosen = os.path.expanduser(chosen.strip().rstrip(os.sep)) or os.sep
    if os.path.isfile(chosen):                 # jemand hat die Game.log selbst gewählt
        chosen = os.path.dirname(chosen)
    if os.path.isfile(os.path.join(chosen, 'Game.log')):
        return chosen
    # Eine Ebene tiefer: der Kanal (LIVE/PTU/…)
    for k in CHANNELS:
        p = os.path.join(chosen, k)
        if os.path.isfile(os.path.join(p, 'Game.log')):
            return p
    # Tiefer suchen, aber begrenzt — ein ganzes Laufwerk durchzugehen wäre
    # unhöflich. Vier Ebenen decken Wine-Präfix -> drive_c -> Programme ->
    # Roberts Space Industries -> StarCitizen -> LIVE ab.
    root_depth = chosen.rstrip(os.sep).count(os.sep)
    for base, subdirs, files in os.walk(chosen):
        if base.count(os.sep) - root_depth > 5:
            subdirs[:] = []
            continue
        if 'Game.log' in files:
            return base
    return None



def _launcher_from_registry():
    r"""Wo der RSI Launcher laut Windows installiert ist — oder None.

    ⚠ Feste Pfadlisten gehen genau dann schief, wenn jemand woanders
    installiert hat. Genau das ist am 26.08.2026 passiert: Auf einem fremden Rechner
    fehlte der Startknopf im Overlay, weil keiner der abgesuchten Orte passte.

    Der Eintrag in der Deinstallations-Liste ist verlässlicher, hat aber zwei
    Tücken, die beide geprüft sind:

    * **Der Schlüsselname ist eine GUID** und bei jeder Installation anders —
      es hilft nur, alle Einträge durchzugehen und den `DisplayName` zu prüfen.
    * **`InstallLocation` ist leer.** Der Pfad steckt statt dessen in
      `DisplayIcon`, das auf `…\RSI Launcher\uninstallerIcon.ico` zeigt. Aus
      dessen Ordner ergibt sich der Launcher.

    Gesucht wird in allen drei Zweigen — der Launcher trägt sich unter HKLM ein,
    aber eine Installation nur für den angemeldeten Nutzer landet unter HKCU.
    """
    if not WINDOWS:
        return None
    try:
        import winreg
    except ImportError:
        return None

    branches = (
        (winreg.HKEY_LOCAL_MACHINE,
         r'SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall'),
        (winreg.HKEY_LOCAL_MACHINE,
         r'SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall'),
        (winreg.HKEY_CURRENT_USER,
         r'Software\Microsoft\Windows\CurrentVersion\Uninstall'),
    )
    for root, path in branches:
        try:
            with winreg.OpenKey(root, path) as uninstall_key:
                count = winreg.QueryInfoKey(uninstall_key)[0]
                for i in range(count):
                    try:
                        name = winreg.EnumKey(uninstall_key, i)
                        with winreg.OpenKey(uninstall_key, name) as entry:
                            found = _launcher_from_entry(winreg, entry)
                            if found:
                                return found
                    except OSError:
                        continue      # einzelner Eintrag unlesbar — weiter
        except OSError:
            continue                  # Zweig gibt es nicht
    return None


def _launcher_from_entry(winreg, entry):
    """Aus einem Deinstallations-Eintrag den Launcher-Pfad ziehen — oder None."""
    def value(field):
        try:
            return str(winreg.QueryValueEx(entry, field)[0] or '')
        except OSError:
            return ''

    if 'rsi launcher' not in value('DisplayName').lower():
        return None

    candidates = []
    place = value('InstallLocation').strip('" ')
    if place:
        candidates.append(os.path.join(place, 'RSI Launcher.exe'))
    # `DisplayIcon` zeigt auf eine Datei **im** Launcher-Ordner.
    symbol = value('DisplayIcon').split(',')[0].strip('" ')
    if symbol:
        candidates.append(os.path.join(os.path.dirname(symbol),
                                       'RSI Launcher.exe'))
    for path in candidates:
        if path and os.path.isfile(path):
            return path
    return None



def clean_environment():
    """Umgebung für fremde Programme — ohne unsere eigenen Bibliothekspfade.

    ⚠ Das ist im AppImage entscheidend. Dort zeigen `LD_LIBRARY_PATH`,
    `PYTHONHOME` und `PYTHONPATH` in das entpackte Paket. Startet man daraus ein
    Systemprogramm wie `zenity`, lädt es unsere mitgelieferten Bibliotheken statt
    seiner eigenen und stirbt sofort — der Dialog erscheint nicht, und für den
    Nutzer sieht es aus, als täte der Knopf nichts. AppImage legt die
    ursprünglichen Werte unter `*_ORIG` ab; die gelten hier wieder.
    """
    env = dict(os.environ)
    # ⚠⚠ **Die Spuren des PyInstaller-Bootloaders müssen raus.** Eine gepackte
    # `.exe` startet sich zweimal: Der Bootloader entpackt und startet sich
    # selbst als Kind — woran das Kind sich erkennt, steht in `_PYI_*` (früher
    # `_MEIPASS2`). Erbt ein NEU gestarteter Watcher diese Variablen, hält er
    # sich für das Kind eines Bootloaders, prüft seinen „Vater" und bricht mit
    # „Security validation failure: … parent process" ab. Im Echttest vom
    # 11.09.2026 war das Update fertig eingespielt — und der Neustart
    # scheiterte genau daran. Für jedes andere Programm sind die Variablen
    # ohnehin bedeutungslos.
    for name in list(env):
        if name.upper().startswith(('_PYI_', '_MEI')):
            env.pop(name, None)
    for name in ('LD_LIBRARY_PATH', 'PYTHONHOME', 'PYTHONPATH',
                 'PYTHONDONTWRITEBYTECODE', 'QT_PLUGIN_PATH', 'GTK_PATH',
                 'GDK_PIXBUF_MODULE_FILE', 'GI_TYPELIB_PATH', 'XDG_DATA_DIRS',
                 'PERLLIB', 'GSETTINGS_SCHEMA_DIR'):
        original = env.pop(name + '_ORIG', None)
        if original:
            env[name] = original
        else:
            env.pop(name, None)
    return env


def browser_commands(url):
    """Die Wege, eine Adresse zu öffnen — in der Reihenfolge, in der es
    versucht wird. Getrennt von `open_in_browser()`, damit sich das **prüfen** lässt,
    ohne dabei einen Browser aufzureißen: Ein Prüflauf, der Fenster öffnet,
    reißt den Tastaturfokus mit und wirft den Spieler aus dem Spiel.
    """
    if sys.platform.startswith('linux'):
        return [['xdg-open', url], ['gio', 'open', url]]
    if sys.platform == 'darwin':
        return [['open', url]]
    return []                      # Windows: `webbrowser` macht es richtig


def open_in_browser(url):
    """Eine Adresse im Browser aufmachen. Gibt zurück, ob es geklappt hat.

    ⚠⚠ **Warum nicht einfach `webbrowser.open()`.** Im AppImage zeigen
    `LD_LIBRARY_PATH` und `PYTHONHOME` in unser entpacktes Paket. Jedes daraus
    gestartete Systemprogramm lädt unsere Bibliotheken statt seiner eigenen und
    stirbt sofort — `webbrowser.open()` meldet trotzdem Erfolg, weil es nur das
    Starten prüft, nicht das Überleben. Für den Nutzer sieht das aus, als täte
    der Knopf **gar nichts**: Genau so gemeldet am 30.08.2026 für „Kaffee
    spendieren" und „Discord", und im Fehlerbericht stand dazu **keine Zeile**,
    weil auch keine Ausnahme flog.

    Deshalb: `xdg-open` selbst starten, mit der Umgebung von `clean_environment()`,
    und kurz nachsehen, ob es überlebt. Erst danach `webbrowser` als Rückfall.
    """
    import subprocess
    import webbrowser
    env = clean_environment()
    for command in browser_commands(url):
        try:
            proc = subprocess.Popen(command, env=env,
                                    stdout=subprocess.DEVNULL,
                                    stderr=subprocess.DEVNULL)
        except OSError:
            continue               # Programm gibt es hier nicht — nächstes
        try:
            # Läuft es nach einer knappen Sekunde noch, hat es die Adresse
            # angenommen. Beendet es sich mit einem Fehler, war es nichts.
            if proc.wait(1.0) != 0:
                continue
        except subprocess.TimeoutExpired:
            pass
        return True
    old = dict(os.environ)
    try:
        os.environ.clear()
        os.environ.update(env)
        return bool(webbrowser.open(url))
    except Exception:
        return False
    finally:
        os.environ.clear()
        os.environ.update(old)


def game_starter():
    """Womit sich Star Citizen starten lässt — oder `None`.

    Auf beiden Systemen dieselbe Frage, nur ein anderer Ort:

    * **Windows** — der RSI Launcher unter `%LOCALAPPDATA%\\Programs`. Das ist
      derselbe Weg, den auch der SC-Deutsch-Launcher geht.
    * **Linux** — das Startskript `sc-launch.sh` im Wine-Präfix, das der
      `lug-helper` der Star Citizen Linux Users Group beim Einrichten anlegt.
      Es bringt Präfix, Wine-Version und Umgebung schon mit. Ein eigener
      Wine-Aufruf wäre hier falsch. **Nicht** der `lug-helper` selbst: Der
      verwaltet nur und kann das Spiel gar nicht starten (siehe unten).

    Ein eigener Weg geht über die Einstellung `spielstarter` — wer Lutris oder
    Heroic benutzt, trägt dort seinen Startbefehl ein.

    Wird nichts gefunden, gibt es auch keinen Knopf. Ein Knopf, der nichts tut,
    ist schlimmer als keiner.
    """
    custom = (setting('spielstarter') or '').strip()
    if custom:
        return custom if os.path.exists(os.path.expanduser(custom)) else custom

    if WINDOWS:
        places = []

        # ⚠ **Zuerst neben dem Spielordner suchen** — das ist der einzige Ort,
        # den wir sicher kennen. Der Launcher legt sich standardmäßig neben die
        # Spielinstallation:
        #
        #     …\Roberts Space Industries\StarCitizen\LIVE   ← das Spiel
        #     …\Roberts Space Industries\RSI Launcher\      ← der Launcher
        #
        # Vorher wurden nur feste Orte unter %LOCALAPPDATA% und %PROGRAMFILES%
        # abgesucht. Bei Haldjas liegt das Spiel in
        # `C:\Program Files\Roberts Space Industries\…` — der Launcher damit an
        # einer Stelle, die nicht in der Liste stand, und der Knopf erschien gar
        # nicht erst: „nicht sicher wo sich die funktion versteckt, aber ich hab
        # sie nicht gefunden" (25.08.2026).
        #
        # Vom Spielordner aus zu suchen trifft jede Installation, egal wohin sie
        # gelegt wurde — statt immer neue feste Pfade nachzutragen.
        game = game_folder()
        if game:
            # …\StarCitizen\LIVE  →  zwei Ebenen hoch  →  …\Roberts Space Industries
            rsi = os.path.dirname(os.path.dirname(game))
            places.append(os.path.join(rsi, 'RSI Launcher', 'RSI Launcher.exe'))
            # Eine Ebene weiter hoch, falls jemand ohne Zweig-Ordner installiert
            places.append(os.path.join(os.path.dirname(rsi), 'RSI Launcher',
                                     'RSI Launcher.exe'))

        # ⚠ **Vor** den festen Orten: Was Windows selbst weiss, schlaegt jede
        # Liste. Wer den Launcher auf ein anderes Laufwerk gelegt hat, faellt
        # sonst durch — genau so fehlte auf einem fremden Rechner der Startknopf.
        from_registry = _launcher_from_registry()
        if from_registry:
            places.append(from_registry)


        for env in ('LOCALAPPDATA', 'PROGRAMFILES', 'PROGRAMW6432'):
            root = os.environ.get(env)
            if not root:
                continue
            places.append(os.path.join(root, 'Programs', 'RSI Launcher',
                                     'RSI Launcher.exe'))
            places.append(os.path.join(root, 'RSI Launcher',
                                     'RSI Launcher.exe'))
            places.append(os.path.join(root, 'Roberts Space Industries',
                                     'RSI Launcher', 'RSI Launcher.exe'))
        for place in places:
            if os.path.isfile(place):
                return place
        return None

    # ⚠ **Der `lug-helper` startet das Spiel nicht.** Hier stand er trotzdem —
    # gefunden wurde er auch zuverlässig, nur ist er das falsche Programm. Sein
    # `--help` kennt Präfix, Wine-Runner, DXVK und Launcher-Reparatur und
    # **keine einzige Startoption**; ohne Argumente öffnet er sein
    # Zenity-Verwaltungsmenü.
    #
    # Aufgefallen am 27.08.2026: Bomb20 meldete „das Starten von SC klappt
    # nicht", der Autor sah dasselbe auf einem System, auf dem der Helper unter
    # `/usr/bin/lug-helper` liegt — Knopf da, Meldung „Star Citizen wird
    # gestartet …", und nichts geschah.
    #
    # Gestartet wird über das Skript, das der Helper anlegt: `sc-launch.sh`,
    # direkt im Wine-Präfix. Und das Präfix steht immer über dem Spielordner:
    #
    #     ~/Games/star-citizen/                   ← Präfix, hier liegt das Skript
    #     ~/Games/star-citizen/drive_c/Program Files/…/StarCitizen/LIVE
    #
    # Deshalb derselbe Gedanke wie im Windows-Zweig oben: **vom Spielordner aus
    # suchen**, statt Orte zu raten. Über `drive_c` liegt das Präfix, egal wohin
    # jemand installiert hat und wie er den Helper eingerichtet hat.
    SCRIPT = 'sc-launch.sh'
    places = []
    game = game_folder()
    if game:
        path = os.path.abspath(game)
        # Hochsteigen, bis `drive_c` erreicht ist — eine Ebene darüber liegt das
        # Präfix. Die Schleife endet spätestens an der Wurzel.
        while True:
            parent = os.path.dirname(path)
            if parent == path:
                break
            if os.path.basename(path).lower() == 'drive_c':
                places.append(os.path.join(parent, SCRIPT))
                break
            path = parent

    # Rückfall: der Standardort des LUG Helper, falls der Spielordner (noch)
    # nicht bekannt ist.
    home = os.path.expanduser('~')
    places.append(os.path.join(home, 'Games', 'star-citizen', SCRIPT))

    for place in places:
        if os.path.isfile(place) and os.access(place, os.X_OK):
            return place

    # ⚠ Bewusst **kein** Rückfall auf `lug-helper`: Er würde gefunden, der Knopf
    # erschiene — und täte wieder nichts. „Ein Knopf, der nichts tut, ist
    # schlimmer als keiner" gilt auch hier. Wer über Lutris oder Heroic spielt,
    # trägt seinen Startbefehl in der Einstellung `spielstarter` ein.
    return None


def _start_command(starter):
    """Aus dem eingetragenen Text die Liste machen, die `Popen` braucht.

    ⚠ **Ein Startbefehl ist nicht immer ein Dateiname.** Hier stand
    `Popen([starter])` — damit gilt der ganze Text als **eine** Datei. Wer
    `flatpak run org.starcitizen-lug.Helper` oder `lutris rungame/star-citizen`
    einträgt, bekommt „Datei nicht gefunden", weil nach einer Datei mit
    Leerzeichen im Namen gesucht wird.

    `shlex.split` zerlegt so, wie eine Shell es täte — samt
    Anführungszeichen für Pfade mit Leerzeichen.

    ⚠ **Eine vorhandene Datei wird NICHT zerlegt.** Sonst zerfiele
    `/home/ich/Meine Spiele/sc-launch.sh` in drei Teile. Erst wenn es die Datei
    so nicht gibt, ist es ein Befehl mit Argumenten.
    """
    if os.path.exists(starter):
        return [starter]
    try:
        import shlex
        parts = shlex.split(starter, posix=not WINDOWS)
    except ValueError:
        return [starter]              # unpaariges Anführungszeichen o. ä.
    return parts or [starter]


def start_game():
    """Star Citizen starten. Gibt (True, '') oder (False, Grund) zurück."""
    starter = game_starter()
    if not starter:
        # ⚠ Dieser Grund landet über `s_sp_start_nein` sichtbar in der
        # Statuszeile — also übersetzen. `sprache` lokal holen: `paths` wird
        # sehr früh geladen, ein Import oben wäre ein Zirkelbezug.
        from . import language
        return False, language.t('s_sp_kein_starter')
    try:
        import subprocess
        # Losgelöst starten: Der Watcher soll weiterlaufen und nicht am Spiel
        # hängen — und beim Beenden das Spiel nicht mitreißen.
        extra = {}
        if WINDOWS:
            extra['creationflags'] = getattr(subprocess, 'DETACHED_PROCESS', 0)
        else:
            extra['start_new_session'] = True
        subprocess.Popen(_start_command(starter), stdout=subprocess.DEVNULL,
                         stderr=subprocess.DEVNULL, **extra)
        return True, ''
    except Exception as exc:
        return False, str(exc)


def game_log(folder=None):
    """Pfad zur aktiven Game.log oder None."""
    folder = folder or game_folder()
    if not folder:
        return None
    p = os.path.join(folder, 'Game.log')
    return p if os.path.isfile(p) else None


# Wie lange die `Game.log` still sein darf, bevor das Spiel als beendet gilt.
#
# ⚠ Star Citizen schreibt im Sekundentakt — auch im Hauptmenü, auch beim
# Herumstehen. Fünf Minuten sind deshalb sehr großzügig gewählt: Sie decken
# einen hängenden Ladebildschirm ab und liegen trotzdem weit unter der Zeit,
# nach der ein Mensch fragt „warum steht da noch läuft?".
GAME_IDLE_SEC = 300


def game_running(folder=None):
    """Schreibt das Spiel gerade noch — läuft es also?

    ⚠⚠ **Wozu das gebraucht wird (05.09.2026).** Das Auftrags-Protokoll führte
    einen Auftrag als „läuft", während das Spiel längst geschlossen war: Wer
    sich ausloggt, ohne abzugeben oder abzubrechen, hinterlässt kein
    Ende-Ereignis im Log. Gemeldet mit „Spiel ist aus, und die Quest die da
    auf läuft steht ist von gestern nacht".

    ⚠ **Der Zustand ist trotzdem richtig, nur das Wort war es nicht.** Der
    Auftrag ist im Spiel weiter angenommen — beim nächsten Einloggen meldet
    Star Citizen ihn erneut. Ihn zu beenden wäre also falsch. Falsch war
    „läuft": Das behauptet „jetzt gerade". Mit dieser Auskunft heißt derselbe
    Zustand bei geschlossenem Spiel „noch offen", und das stimmt in beiden
    Fällen.

    ⚠ Bewusst über die Schreibzeit der Datei und nicht über die Prozessliste:
    Das ist auf jedem System gleich, braucht keine Sonderrechte und geht
    niemanden etwas an außer dem Spiel selbst. Ein Irrtum kostet hier auch
    nichts — es hängt nur ein Wort daran.
    """
    import time
    try:
        log_file = game_log(folder)
        if not log_file:
            return False
        return (time.time() - os.path.getmtime(log_file)) < GAME_IDLE_SEC
    except Exception:
        return False


def log_backups(folder=None):
    """Die aufgehobenen Logs vergangener Sitzungen, älteste zuerst.

    Star Citizen legt bei jedem Spielstart die vorige Game.log unter
    `logbackups/` ab. Daraus lässt sich nachlesen, was ohne laufenden
    Watcher freigeschaltet wurde.

    ⚠⚠ **Auch die Protokolle der NACHBARKANÄLE.** Wer von HOTFIX auf LIVE
    wechselt (oder von PTU zurück), lässt seine ganze Vorgeschichte im anderen
    Ordner liegen — der Watcher sah davon nichts, obwohl es dieselbe Person mit
    demselben Spielstand ist.

    Am 05.09.2026 gemeldet: Nach einem Wechsel von HOTFIX auf LIVE kamen aus
    221 Protokollen nur **drei** Baupläne heraus, weil die übrigen im
    HOTFIX-Ordner lagen. Dazu: „er hat im HOTFIX noch alle logs liegen … können
    wir die aus allen Ordner also Live und HOTFIX in die log durchsuchung
    einbeziehen?"

    ⚠⚠ **Nur LIVE und HOTFIX** — siehe `SHARED_STOCK_CHANNELS`. PTU, EPTU und
    TECH-PREVIEW laufen auf eigenen Spielständen; ihre Baupläne hat man auf
    LIVE nicht, und sie mitzulesen hieße, einen Bestand zu behaupten, den es
    nicht gibt.

    ⚠ Es sind **Geschwisterordner**, nicht Unterordner: Neben
    `…/StarCitizen/LIVE` liegt `HOTFIX`. Doppelt gelesen wird nichts — jeder
    Kanal hat sein eigenes `logbackups/`.
    """
    folder = folder or game_folder()
    if not folder:
        return []
    # Bewusst alles nehmen, was dort liegt: Star Citizen hat die Benennung der
    # Sicherungen über die Jahre mehrfach geändert (mal `Game.log.<Datum>`, mal
    # mit Endung dahinter). Ein Muster auf `*.log` verpasst dann die Hälfte.
    # Ausgenommen sind nur Dinge, die sicher kein Text sind.
    skip_ext = ('.zip', '.7z', '.gz', '.rar', '.dmp', '.mdmp', '.png', '.jpg')
    hits = []
    seen = set()
    for channel_dir in _channel_siblings(folder):
        for p in glob.glob(os.path.join(channel_dir, 'logbackups', '*')):
            if not os.path.isfile(p) or p.lower().endswith(skip_ext):
                continue
            # ⚠ Über den echten Pfad entdoppeln: Ist der eingetragene Ordner
            # selbst schon ein Kanal, steht er zweimal in der Liste.
            real = os.path.realpath(p)
            if real in seen:
                continue
            seen.add(real)
            hits.append(p)
    return sorted(hits, key=lambda p: (_mtime(p), p))


def _channel_siblings(folder):
    """Der Ordner selbst und die Kanäle daneben, die denselben Bestand haben.

    ⚠⚠ **Nur `SHARED_STOCK_CHANNELS`** — also LIVE und HOTFIX. Ein PTU-Ordner
    daneben bleibt unangetastet, auch wenn er voller Protokolle steckt: Dort
    freigeschaltete Baupläne hat man auf LIVE nicht.

    ⚠ Und der eingetragene Ordner selbst kommt IMMER mit, auch wenn er PTU
    heißt — wer dort spielt, will sein eigenes Protokoll gelesen haben. Nur
    Nachbarn werden gefiltert.

    ⚠ Kein Raten: Nachbarn werden nur gesucht, wenn der übergeordnete Ordner
    wirklich `…/StarCitizen` heißt und der eingetragene einer der bekannten
    Kanäle ist. Wer sein Spiel woanders liegen hat, bekommt genau seinen
    Ordner — und nichts, was zufällig danebensteht.
    """
    result = [folder]
    try:
        parent = os.path.dirname(os.path.normpath(folder))
        name = os.path.basename(os.path.normpath(folder)).upper()
        if name not in SHARED_STOCK_CHANNELS:
            return result
        if os.path.basename(parent).lower() != 'starcitizen':
            return result
        for channel in SHARED_STOCK_CHANNELS:
            if channel == name:
                continue
            neighbour = os.path.join(parent, channel)
            if os.path.isdir(os.path.join(neighbour, 'logbackups')):
                result.append(neighbour)
    except Exception:
        pass
    return result


def _mtime(p):
    try:
        return os.path.getmtime(p)
    except OSError:
        return 0.0


def localization_folder(folder=None):
    """`data/Localization` im Spielordner — dort liegen die entpackten `global.ini`,
    sofern welche vorhanden sind (der SC Deutsch Launcher legt die deutsche dort ab)."""
    folder = folder or game_folder()
    if not folder:
        return None
    p = os.path.join(folder, 'data', 'Localization')
    return p if os.path.isdir(p) else None


# ---------------------------------------------------- 3. SC Deutsch Launcher (optional)
def launcher_folder():
    """Blueprint-Ordner des SC Deutsch Launchers oder None.

    Unter Windows liegt er in %APPDATA%. Unter Linux nur dann, wenn jemand den
    Launcher unter Wine betreibt — dann steckt dasselbe AppData im Wine-Präfix."""
    # Eine **gesetzte** Angabe gilt allein — auch wenn der Ordner dort nicht
    # existiert. Wer einen Pfad einträgt, will keine Suche woanders; sonst
    # nimmt das Programm klammheimlich einen anderen Launcher-Stand her als den
    # angegebenen. (Fiel im Selbsttest auf: Der baut eine Installation ohne
    # Launcher nach, bekam aber den echten von der Windows-Platte untergeschoben.)
    for custom in (os.environ.get('SC_BP_LAUNCHER'), setting('launcher_ordner')):
        if custom is not None and custom != '':
            custom = os.path.expanduser(custom)
            return custom if os.path.isdir(custom) else None
    if os.environ.get('SC_BP_LAUNCHER') == '':
        return None                    # ausdrücklich abgeschaltet
    if WINDOWS:
        p = os.path.join(os.environ.get('APPDATA', ''), 'sc-deutsch-launcher',
                         'blueprints')
        return p if os.path.isdir(p) else None
    for prefix in _wine_prefixes():
        pattern = os.path.join(prefix, 'drive_c', 'users', '*', 'AppData',
                              'Roaming', 'sc-deutsch-launcher', 'blueprints')
        for p in sorted(glob.glob(pattern)):
            if os.path.isdir(p):
                return p
    # Dual-Boot: Der Launcher läuft unter Windows, seine Daten liegen auf der
    # Windows-Platte — die unter Linux meist eingehängt ist. Ohne diesen Blick
    # steht ein umgestiegener Spieler ohne seinen alten Bauplan-Stand da,
    # obwohl der zwei Ordner weiter vollständig vorliegt. Genau so passiert.
    for p in _windows_launcher():
        return p
    return None


def _windows_launcher():
    """Launcher-Daten auf einer eingehängten Windows-Platte."""
    home = os.path.expanduser('~')
    places = ['/run/media/*/*', '/media/*/*', '/mnt/*',
            os.path.join(home, '.local', 'share', '*')]
    for place in places:
        pattern = os.path.join(place, 'Users', '*', 'AppData', 'Roaming',
                              'sc-deutsch-launcher', 'blueprints')
        for p in sorted(glob.glob(pattern)):
            if os.path.isdir(p):
                yield p


def launcher_file(name, folder=None):
    """Pfad zu einer Launcher-Datei, auch wenn es den Launcher nicht gibt.

    Gibt immer einen Pfad zurück (nie None), damit die aufrufende Stelle wie
    bisher einfach versuchen kann, ihn zu öffnen. Ohne Launcher zeigt er ins
    Leere und das Öffnen scheitert — genau das ist gewollt."""
    folder = folder if folder is not None else (launcher_folder() or '')
    return os.path.join(folder, name)


# ------------------------------------------------------------------ Übersicht
def redact(text):
    """Persönliches aus einem Text nehmen — für Fehlerprotokoll und Bericht.

    Pfade verraten den Benutzernamen (`C:\\Users\\Spieler\\…`,
    `/home/spieler/…`), und genau solche Texte landen in einem **öffentlichen**
    Issue. Ersetzt werden das Heimatverzeichnis und danach jedes weitere
    Vorkommen des Benutzernamens.

    Lieber einmal zu viel ersetzt als ein Name zu viel im Netz.
    """
    try:
        text = str(text)
        home = os.path.expanduser('~')
        name = os.path.basename(home.rstrip('\\/'))

        for variant in (home, home.replace('\\', '/'), home.replace('/', '\\')):
            if variant and len(variant) > 3:
                text = text.replace(variant, '<heim>')

        if name and len(name) > 2:
            text = re.sub(re.escape(name), '<benutzer>', text, flags=re.I)

        return _redact_secrets(text)
    except Exception:
        return str(text)


# Adressen, die ein Geheimnis IM PFAD tragen. Bei einem Discord-Webhook ist der
# hintere Teil der Schlüssel: Wer ihn hat, kann in den Melde-Kanal schreiben.
_WEBHOOK_RE = re.compile(
    r'https://[\w.-]*discord(?:app)?\.com/api/webhooks/\S+', re.I)
# Und Parameter, die nach Zugang klingen. ⚠ Bewusst eng: `?id_category=3` und
# `?uuid=…` bleiben stehen — sie sagen, WELCHER Abruf schiefging, und ohne sie
# ist ein Netzfehler nicht mehr zu deuten.
_CREDENTIAL_RE = re.compile(
    r'([?&](?:token|key|api[_-]?key|apikey|secret|auth|password|passwd|pw|'
    r'access[_-]?token|signature|sig)=)[^&\s]+', re.I)


def _redact_secrets(text):
    """Zugangsdaten aus einem Text nehmen, der öffentlich werden kann.

    ⚠⚠ **Warum das nötig ist — gemessen, nicht vermutet (05.09.2026).** Der
    Fehlerbericht landet in einem öffentlichen Issue. `absenden()` gibt den
    Grund eines gescheiterten Sendeversuchs bewusst nicht zurück, weil die
    Adresse geheim ist — schreibt die Ausnahme aber eine Zeile darüber mit
    `fehler.merken()` ins Protokoll, und das Protokoll steht im Bericht.

    Vier realistische Fehlerfälle durchgespielt: Drei sind harmlos (`urllib`
    nennt die Adresse nicht), einer nicht — jede Meldung, die eine Adresse
    selbst in ihren Text schreibt. Genau dieses Muster gibt es bereits: Die
    Netzabrufe hängen die abgerufene Adresse an ihre Meldung. Dort ist sie
    öffentlich und nützlich; beim Melde-Kanal wäre sie ein Schlüssel.

    ⚠ **Eng gefasst mit Absicht.** Eine Adresse im Fehlertext ist das
    Wertvollste am ganzen Eintrag — sie sagt, welcher Abruf schiefging.
    Gekürzt wird deshalb nur, was ein Zugang IST: der Webhook-Pfad und
    Parameter, die so heißen. Alles andere bleibt lesbar.
    """
    # ⚠⚠ **Zuerst die EIGENE Adresse — sie ist die einzige, die wirklich weh
    # tut.** Das Muster darunter erkennt Discord-Webhooks; steht in
    # `SC_BP_BERICHT_ZIEL` aber etwas anderes (ein eigener Dienst, eine
    # Weiterleitung), greift es nicht. Hier wird ersetzt, was tatsächlich
    # eingetragen ist, ganz gleich wie es aussieht.
    #
    # ⚠ Lokal importiert: `report_target` kommt ohne `paths` aus, aber ein
    # Import auf Modulebene würde diese Reihenfolge für immer festschreiben.
    try:
        from . import report_target
        url = report_target.target()
        # Die Längenschwelle ist kein Schmuck: Ohne sie würde ein leeres Ziel
        # jede Stelle im Text treffen und den ganzen Bericht zerlegen.
        if url and len(url) > 12:
            text = text.replace(url, '<meldeadresse>')
    except Exception:
        pass                    # lieber ungekürzt als gar kein Bericht

    text = _WEBHOOK_RE.sub('<meldeadresse>', text)
    return _CREDENTIAL_RE.sub(r'\1<geheim>', text)


def overview():
    """Was wurde gefunden — für Statusanzeige und Fehlersuche."""
    game = game_folder()
    return {
        'system': 'Windows' if WINDOWS else sys.platform,
        'app_ordner': app_folder(),
        'spiel_ordner': game,
        'game_log': game_log(game),
        'sicherungen': len(log_backups(game)),
        'launcher': launcher_folder(),
        'einstellungen': app_file(SETTINGS_FILE),
        'selbst_gesetzt': {k: v for k, v in settings().items()
                           if not k.startswith('_') and v},
    }


if __name__ == '__main__':
    for k, v in overview().items():
        print('%-14s %s' % (k, v))


# --------------------------------------------------------- Namen vergleichen
# Alle Anführungszeichen, die in Bauplan-Namen vorkommen — gerade,
# typografische und die französischen. Beim Vergleichen werden sie auf ein
# einfaches `'` gezogen.
#
# ⚠⚠ **Vollständig halten — eine Lücke fällt nie von allein auf.** Bis zum
# 30.08.2026 fehlte ausgerechnet das **öffnende** typografische
# Anführungszeichen `\u201c`. Aus `SW16BR1 “Buzzsaw” Repeater` wurde dadurch
# `sw16br1 “buzzsaw' repeater` — das schließende war angeglichen, das öffnende
# nicht. Drei Baupläne im Katalog tragen es, und keiner von ihnen konnte je zu
# einem Fund aus einer anderen Quelle passen. Gefunden beim Abgleich
# einer von Hand geführten Bauplanliste gegen den Katalog, nicht durch eine
# Meldung: Der Bauplan gilt einfach als „fehlt", und niemand kommt auf die Idee,
# dass ein Anführungszeichen daran schuld ist.
#
# Prüfung 80 im Selbsttest zieht alle hier gelisteten Zeichen durch
# `namensform()` und verlangt dasselbe Ergebnis.
QUOTES = str.maketrans({
    '"': "'",
    '\u201c': "'",      # “  oeffnend, typografisch — fehlte bis 30.08.2026
    '\u201d': "'",      # ”  schliessend, typografisch
    '\u201e': "'",      # „  deutsches oeffnendes unten
    '\u2018': "'",      # ‘  einfach, oeffnend
    '\u2019': "'",      # ’  einfach, schliessend (auch Apostroph)
    '\u201a': "'",      # ‚  einfach unten
    '\u00ab': "'",      # «  franzoesisch
    '\u00bb': "'",      # »  franzoesisch
    '\u2039': "'",      # ‹  franzoesisch, einfach
    '\u203a': "'",      # ›  franzoesisch, einfach
})


# Der Klassen-Zusatz am Namensende — `7CA 'Nargun' (Civ/3/A)`, `XL-1 (Mil/2/A)`,
# `P4-AR Rifle (Bal)`, `'Arrow' I Missile (IR1)`.
#
# ⚠ **Warum das hierher gehört und nicht nur ins Log-Lesen.** Bis zum 28.08.2026
# schnitt nur `logsource.split_names()` den Zusatz ab. Namen aus der
# **Launcher-Datei** und aus **Importen** (Basetool, scmdb, eigene Sicherung)
# gingen ungeschnitten in den Bestand — und `XL-1 (Mil/2/A)` findet `XL-1` nie.
# Der Bauplan galt als fehlend, obwohl er dastand.
#
# Aufgefallen an Morkhan: Er hatte die Baupläne gemeinsam mit dem Autor gefarmt,
# hatte den SC Deutsch Launcher mit gepflegter Datei — und im Spiel standen die
# Kästchen trotzdem leer. Gemeldet: „vergleich doch mal die Logik, was habe ich,
# mit meiner BP-Liste, und hör auf zu raten."
#
# Bewusst eng: Nur die bekannten Kürzel, damit echte Namensklammern wie
# `Singe Cannon (S2)` oder `(30 cap)` stehen bleiben. Die Liste muss zu
# `scbp/specs.py` passen — Selbsttest 32 wacht darüber.
_CLASS_CODES = ('civ|mil|ind|sth|cmp'
                 '|las|ele|pla|dis|mic|bal'
                 '|nah|min|slv|med|tool|trc')
CLASS_CODE_RE = re.compile(
    r'\s*\((?:(?:%s)/(?:\d{1,2}|\u2013|-)/(?:[a-d]|\u2013|-)'
    r'|(?:%s)'
    r'|(?:ir|em|cs)\d{1,2})\)\s*$' % (_CLASS_CODES, _CLASS_CODES))

# \u26a0\u26a0 **MrKraken StarStrings stellt dasselbe K\u00fcrzel VORAN \u2014 ohne Klammern.**
# Wer StarStrings einsetzt, hat im Spiel `Ind/2/B Citadel` stehen, wo der
# Katalog `Citadel` kennt; die uebliche Schreibweise waere `Citadel (Ind/2/B)`.
# `CLASS_CODE_RE` oben faengt nur die Klammerform. Die vorangestellte blieb stehen,
# der Name fand seinen Katalog-Eintrag nicht und galt als \u201enicht im Katalog".
#
# Gemessen am 04.09.2026 in der ausgelieferten StarStrings-Datei: **465**
# Eintraege in dieser Form \u2014 Kuehler, Schilde, Kraftwerke. Bei einem Melder
# waren vier von 26 Bauplaenen betroffen, also jeder sechste.
#
# \u26a0 Es liegt NICHT an der Spielsprache. Das war der erste Verdacht (der Melder
# spielt auf Englisch), und er war falsch \u2014 er benutzt StarStrings, und das
# schreibt die Namen so. Wer hier etwas aendert, prueft es an der echten Datei
# nach (`tools/starstrings_pruefen.py` zeigt, woher sie kommt).
#
# Genauso eng gehalten wie oben: nur die bekannten Kuerzel, nur am Anfang, und
# es muss ein Leerzeichen samt Namen folgen. `Ind/2/B` allein bleibt stehen \u2014
# ein Name, der nur aus dem Kuerzel besteht, waere sonst leer.
CLASS_CODE_PREFIX_RE = re.compile(
    r'^\s*(?:%s)/(?:\d{1,2}|\u2013|-)/(?:[a-d]|\u2013|-)\s+(?=\S)'
    % _CLASS_CODES)


# Die Mengenangabe am Namensende — `(16 cap)`, `(16 Schuss)`, `(40 rounds)`.
#
# ⚠ **Warum das Wort weg muss und die Zahl bleibt.** Der SC Deutsch Launcher
# liest den **englischen** Katalog und schreibt `Ravager-212 Twin Shotgun
# Magazine (16 cap)`. Die **Log-Nachlese** liest dieselbe Kiste in der Sprache,
# in der das Spiel laeuft — bei der Autor auf Deutsch: `... (16 Schuss)`.
# Ergebnis: derselbe Bauplan zweimal im Bestand, und die angezeigte Zahl ist zu
# hoch. Gemessen am 29.08.2026 an einem echten Bestand: 405 angezeigt, 403 echt.
#
# Die **Zahl** ist Teil der Identitaet — ein 40er- und ein 60er-Magazin sind
# verschiedene Bauplaene. Deshalb wird `(16 Schuss)` zu `(16)`, nicht zu nichts.
# Und es greift nur, wenn die Klammer mit einer Ziffer beginnt und danach ein
# Wort folgt: `Singe Cannon (S2)` und `(1/A)` bleiben unangetastet.
QUANTITY_RE = re.compile(r'\((\d+)\s+[^)]*\)')


def name_key(s):
    """Ein Bauplan-Name als Vergleichsschlüssel — die EINZIGE Stelle dafür.

    ⚠ Diese Funktion stand dreimal im Programm: in `collection.py`, `catalog.py`
    und `watchlist.py`. Der Kommentar in `collection.py` behauptete „identisch zum
    Hauptprogramm" — und war es nicht mehr. Wer eine davon anfasst, verschiebt
    stillschweigend, welche Baupläne noch zueinander finden.

    Angeglichen wird viererlei:

    * **Groß- und Kleinschreibung.**
    * **Geschützte und kaputte Leerzeichen** (`\xa0`, `\ufffd`).
    * **Anführungszeichen.** Der SC Deutsch Launcher exportiert
      `7MA "Lorica"` mit geraden doppelten, scmdb führt denselben Bauplan als
      `7MA 'Lorica'` mit einfachen. Ohne Angleichung sind das zwei Schlüssel,
      und der Bauplan gilt als „fehlt", obwohl er im Bestand steht. Gefunden
      an einem echten Bestand mit 392 Bauplänen — genau einer fiel durch.
    * **Die Sprache der Mengenangabe** (`(16 cap)` ↔ `(16 Schuss)`) — siehe
      `QUANTITY_RE` oben. Die Zahl bleibt stehen, nur das Wort faellt weg.
    """
    return QUANTITY_RE.sub(r'(\1)',
                        CLASS_CODE_PREFIX_RE.sub(
                            '',
                            CLASS_CODE_RE.sub('', str(s).lower()
.replace('\xa0', ' ')
.replace('\ufffd', ' ')
.translate(QUOTES)).strip())).strip()
