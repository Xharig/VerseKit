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
Ein Eintrag im Startmenü — damit das Werkzeug auffindbar ist.

**Warum das nötig ist.** Unter Windows legt der Installer alles an. Unter Linux
gibt es keinen Installer: Dort lädt man ein AppImage herunter, und das liegt dann
im Download-Ordner. Es steht in keinem Menü, es hat kein Symbol, und wer es
starten will, muss wissen, wo es liegt. Nach einem Neustart sucht man es.

Angelegt wird eine `.desktop`-Datei nach dem Freedesktop-Standard in
`~/.local/share/applications/`. Das ist der Ort für Einträge eines einzelnen
Nutzers — kein Systemordner, keine Administratorrechte, und beim Entfernen bleibt
nichts zurück außer dieser einen Datei.

Zwei Dinge macht die Datei nebenbei mit möglich:

  * **Ein Tastenkürzel.** Die Arbeitsumgebung (KDE, GNOME …) lässt auf jeden
    Menüeintrag eine Tastenkombination legen. Zusammen mit dem
    Einzelinstanz-Wächter aus `overlay.py` ist das der Weg, das Overlay im
    Pop-up-Betrieb zurückzuholen.
  * **Anheften.** Was im Menü steht, lässt sich in die Leiste ziehen.

Das Symbol wird als eigene Datei danebengelegt: Ein AppImage bringt sein Symbol
zwar mit, aber erst nach dem Entpacken — die Menüverwaltung kommt nicht hinein.
"""
import os
import shutil
import subprocess
import sys

WINDOWS = sys.platform.startswith('win')

FILENAME = 'sc-bp-watcher.desktop'
ICON_NAME = 'sc-bp-watcher.png'


def available():
    """Lohnt sich der Eintrag auf diesem System?"""
    return not WINDOWS and sys.platform != 'darwin'


def _program_path():
    """Womit das Programm gestartet wird — AppImage oder das laufende Python.

    ⚠ Bei einem AppImage steht der Pfad **nur** in `APPIMAGE`; `sys.executable`
    zeigt in den entpackten Zwischenordner unter `/tmp`, der beim nächsten Start
    einen anderen Namen hat. Ein Menüeintrag darauf wäre nach einem Neustart tot.
    """
    appimage = os.environ.get('APPIMAGE')
    if appimage and os.path.isfile(appimage):
        return os.path.abspath(appimage), None
    if getattr(sys, 'frozen', False):
        return os.path.abspath(sys.executable), None
    # Aus dem Quellcode: das Startskript nehmen, damit der Eintrag auch nach
    # einem `git pull` noch stimmt.
    wurzel = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    skript = os.path.join(wurzel, 'sc_bp_watcher.py')
    return os.path.abspath(sys.executable), skript


def _menu_dir():
    basis = (os.environ.get('XDG_DATA_HOME')
             or os.path.join(os.path.expanduser('~'), '.local', 'share'))
    return os.path.join(basis, 'applications')


def target_file():
    return os.path.join(_menu_dir(), FILENAME)


def exists():
    """Gibt es den Eintrag schon — und zeigt er noch auf ein Programm, das da ist?"""
    pfad = target_file()
    if not os.path.isfile(pfad):
        return False
    try:
        with open(pfad, encoding='utf-8') as f:
            for zeile in f:
                if zeile.startswith('Exec='):
                    befehl = zeile.split('=', 1)[1].strip().strip('"').split('"')[0]
                    return os.path.exists(befehl.split(' ')[0].strip('"'))
    except OSError:
        return False
    return True


def _write_icon(ordner):
    """Das Programmsymbol neben den Eintrag legen. Gibt den Pfad zurück."""
    quelle = None
    wurzel = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    for kandidat in (os.path.join(getattr(sys, '_MEIPASS', ''), 'icon.png'),
                     os.path.join(wurzel, 'assets', 'icon.png'),
                     os.path.join(wurzel, 'icon.png')):
        if kandidat and os.path.isfile(kandidat):
            quelle = kandidat
            break
    if not quelle:
        return 'sc-bp-watcher'          # kein Bild da: Name reicht als Kennung
    ziel = os.path.join(ordner, ICON_NAME)
    try:
        os.makedirs(ordner, exist_ok=True)
        shutil.copyfile(quelle, ziel)
        return ziel
    except OSError:
        return 'sc-bp-watcher'


def desktop_content(befehl, symbol):
    """Der Text einer `.desktop`-Datei — ohne Dateisystem, ohne Linux.

    ⭐ Steht bewusst als eigene Funktion da: So lässt sich **prüfen, was
    entsteht**, auch auf einem System, das gar keine `.desktop`-Dateien kennt.
    Am 12.09.2026 schlug Prüfung 191 unter Windows fehl, weil sie `anlegen()`
    rief — und das gibt dort „nur unter Linux" zurück. Eine Prüfung, die sich
    auf dem halben Bestand überspringt, prüft die Hälfte nicht.

    ⚠ Name und Untertitel kommen aus `sprache.py` — dieselbe Quelle, aus der
    `beschriftung_nachziehen()` eine vorhandene Datei aktualisiert. Getrennt
    gepflegt wären sie nach dem ersten Wortwechsel auseinander.
    """
    # Lokal importiert, weil `sprache` selbst auf `pfade` aufsetzt.
    from . import sprache
    name = sprache.t('hf_titel')
    return (
        '[Desktop Entry]\n'
        'Type=Application\n'
        'Name=%s\n'
        'Comment=%s\n'
        'Comment[en]=%s\n'
        'Exec=%s\n'
        'Icon=%s\n'
        'Terminal=false\n'
        'Categories=Utility;Game;\n'
        'StartupWMClass=%s\n'
        'Keywords=Star Citizen;Blueprint;Bauplan;\n'
        % (name, sprache.TEXTE['vk_untertitel'][0],
           sprache.TEXTE['vk_untertitel'][1], befehl, symbol, name))


def create():
    """Den Menüeintrag schreiben. Gibt (geklappt, Pfad-oder-Meldung) zurück."""
    if not available():
        return False, 'nur unter Linux'
    programm, skript = _program_path()
    if not os.path.exists(programm):
        return False, programm
    ordner = _menu_dir()
    symbol_ordner = os.path.join(
        os.environ.get('XDG_DATA_HOME')
        or os.path.join(os.path.expanduser('~'), '.local', 'share'),
        'icons', 'hicolor', '256x256', 'apps')
    symbol = _write_icon(symbol_ordner)

    # Pfade mit Leerzeichen gehören in Anführungszeichen — das AppImage liegt bei
    # vielen unter „Programme"/„Downloads", und ohne Anführungszeichen bricht der
    # Start still ab.
    befehl = '"%s"' % programm
    if skript:
        befehl += ' "%s"' % skript

    inhalt = desktop_content(befehl, symbol)
    try:
        os.makedirs(ordner, exist_ok=True)
        pfad = target_file()
        with open(pfad, 'w', encoding='utf-8') as f:
            f.write(inhalt)
        os.chmod(pfad, 0o755)
    except OSError as ausnahme:
        return False, str(ausnahme)

    # Die Menüverwaltung anstoßen. Fehlt das Werkzeug, taucht der Eintrag
    # spätestens nach dem nächsten Anmelden auf — kein Grund für eine Fehlermeldung.
    try:
        if shutil.which('update-desktop-database'):
            subprocess.run(['update-desktop-database', ordner],
                           capture_output=True, timeout=20)
    except Exception:
        pass
    return True, pfad


def refresh_label():
    """Einen **vorhandenen** Eintrag auf den aktuellen Produktnamen bringen.

    ⚠⚠ Gebraucht wegen der Umbenennung zu VerseKit (12.09.2026). `anlegen()`
    schreibt die neuen Beschriftungen — aber beim Update läuft `anlegen()`
    **nicht**: `vorhanden()` meldet den Eintrag als da, und damit ist die Sache
    für den Assistenten erledigt. Bestandsnutzer behielten so dauerhaft
    „SC BP Watcher" im Anwendungsmenü, samt altem `StartupWMClass`.

    Drei Regeln, alle drei wichtig:

    * ⛔ **Legt nie etwas an.** Wer die Verknüpfung bewusst gelöscht hat, soll
      sie nicht durch ein Update zurückbekommen.
    * ⛔ **Fasst `Exec`, `Icon` und den Dateinamen nicht an.** Das sind die
      Anker — der Pfad zum Programm und die Symboldatei.
    * ✅ **Ändert nur, was sich geändert hat.** Steht der neue Name schon da,
      wird die Datei nicht angefasst (kein Zeitstempel, kein Neuschreiben).

    Gibt zurück, ob etwas geändert wurde.
    """
    # ⚠ NICHT `vorhanden()` — das prüft zusätzlich, ob `Exec` auf ein
    # existierendes Programm zeigt. Hier zählt allein: **liegt die Datei da?**
    # Ein AppImage, das gerade verschoben wurde, hätte sonst für immer die alte
    # Beschriftung behalten. Gefunden vom eigenen Migrationstest am 12.09.2026.
    pfad = target_file()
    if not os.path.isfile(pfad):
        return False
    try:
        with open(pfad, encoding='utf-8') as f:
            zeilen = f.readlines()
    except OSError:
        return False

    from . import sprache
    name = sprache.t('hf_titel')
    neu = {
        'Name=': 'Name=%s\n' % name,
        'Comment=': 'Comment=%s\n' % sprache.TEXTE['vk_untertitel'][0],
        'Comment[en]=': 'Comment[en]=%s\n' % sprache.TEXTE['vk_untertitel'][1],
        'StartupWMClass=': 'StartupWMClass=%s\n' % name,
    }
    geaendert = False
    for i, zeile in enumerate(zeilen):
        for vorsatz, ersatz in neu.items():
            if zeile.startswith(vorsatz) and zeile != ersatz:
                zeilen[i] = ersatz
                geaendert = True
                break
    if not geaendert:
        return False
    try:
        with open(pfad, 'w', encoding='utf-8') as f:
            f.writelines(zeilen)
    except OSError:
        return False
    return True


def remove():
    """Den Eintrag wieder wegnehmen."""
    try:
        pfad = target_file()
        if os.path.isfile(pfad):
            os.remove(pfad)
        return True
    except OSError:
        return False
