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
Alles Eigene in **eine Datei** sichern — und wieder zurückholen.

Gedacht für den Rechnerwechsel: Eine Datei auf den Stick, am neuen Rechner
einlesen, weiterspielen. Bauplan-Bestand, beide Lager, Auftrags-Protokoll,
Merkliste, Einstellungen — alles, was nirgends sonst zu holen ist.

⚠⚠ **Aussperren statt aufzählen.** Der nächstliegende Weg wäre eine Liste der
Dateien, die mitkommen. Genau das gab es schon einmal, als `.gitignore` im
Ablage-Ordner — und als mit dem Auftrags-Protokoll eine neue eigene Datei
dazukam, fiel sie stillschweigend heraus. Niemand merkt so etwas, bis der
Rechner neu aufgesetzt ist.

Deshalb hier andersherum: Mitgenommen wird **alles**, ausgenommen die
Zwischenspeicher, die sich jederzeit neu laden lassen (`RELOADABLE`). Kommt
morgen eine neue eigene Datei dazu, ist sie ohne Zutun in der Sicherung. Der
schlimmste Fall ist dann eine etwas größere Datei — nicht ein fehlender
Bestand.

⚠ **Ein Rückweg gehört dazu.** Eine Sicherung, die sich nur schreiben lässt,
löst den Rechnerwechsel nicht: Der Spieler müsste die Dateien von Hand in einen
Ordner legen, den er nicht kennt. `restore()` ist deshalb kein Zusatz,
sondern die zweite Hälfte derselben Funktion.

⚠ Bis zum 11.09.2026 hieß dieses Modul `sicherung` (Sprachumstellung P4,
Stufe 1). **Nur Bezeichner sind umbenannt, keine Zeichenketten.** Bewusst
gleich geblieben, weil sie in den Sicherungen der Nutzer stehen: die Kennung
`SC-BP-Watcher-Sicherung`, die Beilage `sicherung.txt` und der Vorsatz
`Steuerung/` im Archiv — umbenannt, ließe sich keine ältere Sicherung mehr
einspielen. Ebenso die Kennwörter `'leer'` und `'ungueltig'`, über die die
Oberfläche ihre Meldung wählt, und die Einstellungsschlüssel in `PATH_FIELDS`.
"""
import os
import time
import zipfile

from . import fehler, pfade

# Die Kennung im Kopf der Datei — daran ist eine Sicherung dieses Programms zu
# erkennen, auch wenn jemand sie umbenannt hat.
MARKER = 'SC-BP-Watcher-Sicherung'
INFO_FILE = 'sicherung.txt'

# Was NICHT mitkommt: heruntergeladene Nachschlagewerke und Spuren des
# laufenden Betriebs. Zusammen sind das mehrere Megabyte, und jede Datei davon
# holt sich das Programm beim nächsten Start von allein zurück.
#
# ⚠ Im Zweifel gehört etwas NICHT hierher. Eine zu große Sicherung kostet
# Sekunden, eine zu kleine kostet den Bestand.
RELOADABLE = (
    'Intern/bp-contracts-de.json',      # Auftragstexte, vom Netz
    'Intern/bp-contracts-en.json',
    'Intern/crafting-blueprints.json',  # Rezepte, vom Netz
    'Intern/katalog-cache.json',        # der Bauplan-Katalog, vom Netz
    'Intern/mining-data.json',          # Bergbau-Daten, vom Netz
    'Intern/orte.json',                 # Fundorte, vom Netz
    'Intern/preise.json',               # UEX-Preise, täglich frisch
    'Intern/scmdb-items.json',          # Gegenstandsdaten, vom Netz
    'Intern/serverstatus.json',         # Statusmeldungen von CIG
    'Intern/uebersetzung.json',         # aus der global.ini des Spiels
    'Intern/verkauf.json',              # Verkaufsorte, vom Netz
    'Intern/aktionsnamen.json',         # Aktionsnamen, aus dem Spielarchiv
    # ⚠ Der Lesestand gehört ausdrücklich NICHT mit: Er zeigt auf Logdateien
    # des alten Rechners. Am neuen wäre er falsch und würde die Nachlese
    # überspringen — das Auftrags-Protokoll bliebe leer.
    'Intern/logstand.json',
)

# Ganze Ordner, die draußen bleiben.
RELOADABLE_FOLDERS = (
    'export',       # wird bei jedem Fund neu geschrieben
    'Diagnose',     # Fehlerprotokolle des alten Rechners
)


# ⚠⚠ **Die Belegung liegt NICHT in unserer Ablage, sondern im Spielordner.**
# Bis v3.14 fiel sie deshalb durch jedes Raster: Der Knopf hiess „Sicherung",
# nahm aber ausgerechnet das nicht mit, was am schwersten wiederzubeschaffen
# ist — eine verlorene HOTAS-Belegung sind Stunden.
#
# Sie kommt unter einem eigenen Vorsatz ins Archiv, damit beim Zurueckholen
# klar zu trennen ist, was in die Ablage gehoert und was ins Spiel.
CONTROLS = 'Steuerung/'


def _binding_files(folder=None):
    """Die Belegungsdateien des Spielers — (voller Pfad, Name im Archiv).

    Zwei Sorten, beide noetig:

    | Was | Wo | Warum |
    |---|---|---|
    | die **aktive** Belegung | `Profiles/default/actionmaps.xml` | was gerade im Spiel gilt — darin stecken auch Totzone, Saettigung und Empfindlichkeit jeder Achse |
    | die **Spieleinstellungen** | `Profiles/default/attributes.xml` | Blickwinkel, Aufloesung, Grafik, Lautstaerken |
    | die **gespeicherten Profile** | `controls/mappings/*.xml` | Kampf, Bergbau, Frachtflug — wer sie sich angelegt hat, verliert sonst alles ausser dem zuletzt geladenen |

    ⚠ Den Mappings-Ordner gibt es in mehreren Schreibweisen (siehe
    `joysticks.MAPPING_ORDNER`). Gleichnamige Dateien werden **entdoppelt**,
    die neuere gewinnt — sonst laege dieselbe Belegung zweimal im Archiv, und
    beim Zurueckholen entschiede der Zufall.

    ⚠⚠ **Eine Sicherung nimmt alles mit, was erreichbar ist.** Die
    `attributes.xml` fehlte anfangs — damit waere beim Zurueckholen zwar die
    Steuerung wieder da gewesen, aber der eingestellte Blickwinkel weg.
    Wuensch dazu am 06.09.2026: „Sicherung sollte allgemein immer alles
    verfuegbare sichern, nicht nur einzelne Teile." Kommt eine weitere Datei
    des Spielers dazu, gehoert sie hierher — nicht in eine zweite Liste.
    """
    from . import joysticks
    found = {}
    active = joysticks._pfad_actionmaps(folder)
    if active and os.path.isfile(active):
        found['actionmaps.xml'] = active
        # Die Spieleinstellungen liegen im selben Ordner. Ueber den Pfad der
        # Belegung gefunden, damit die Gross-/Kleinschreibung stimmt (USER
        # oder user, Client oder client) — dieselbe Falle wie ueberall hier.
        neighbour = os.path.join(os.path.dirname(active), 'attributes.xml')
        if os.path.isfile(neighbour):
            found['attributes.xml'] = neighbour
    # ⚠ Ueber **alle** Schreibweisen des Ordners sammeln, nicht nur ueber den,
    # in den geschrieben wuerde. Beim Sichern zaehlt Vollstaendigkeit.
    for mappings in joysticks.alle_mapping_ordner(folder):
        try:
            for name in os.listdir(mappings):
                if not name.lower().endswith('.xml'):
                    continue
                full = os.path.join(mappings, name)
                if not os.path.isfile(full):
                    continue
                key = 'mappings/' + name
                previous = found.get(key)
                if previous and os.path.getmtime(previous) >= os.path.getmtime(full):
                    continue
                found[key] = full
        except OSError:
            continue
    return sorted(((full, CONTROLS + rel)
                   for rel, full in found.items()), key=lambda x: x[1])


def _include(rel):
    """Gehört diese Datei (Pfad relativ zur Ablage) in die Sicherung?"""
    path = rel.replace('\\', '/')
    if path in RELOADABLE:
        return False
    first = path.split('/', 1)[0]
    if first in RELOADABLE_FOLDERS:
        return False
    # Die Sicherung selbst nicht mitsichern, falls sie jemand in die Ablage legt.
    return not path.lower().endswith('.zip')


def _files():
    """Alle mitzunehmenden Dateien der Ablage — (voller Pfad, Name in der Datei)."""
    root = pfade.app_ordner()
    found = []
    for folder, _sub, names in os.walk(root):
        for name in names:
            full = os.path.join(folder, name)
            rel = os.path.relpath(full, root)
            if _include(rel):
                found.append((full, rel.replace('\\', '/')))
    return sorted(found, key=lambda x: x[1])


def suggestion():
    """Ein Dateiname mit Datum — eine Sicherung hält einen Stand fest."""
    return 'SC-BP-Watcher-Sicherung-%s.zip' % time.strftime('%Y-%m-%d')


def write(target, version='', game_folder=None):
    """Alles Eigene in eine ZIP-Datei schreiben.

    Gibt `(ok, meldung, anzahl)` zurück. Die Meldung ist für den Spieler
    gedacht und nennt im Fehlerfall den Grund — ein stilles `False` hilft
    niemandem.

    ⚠ `game_folder` ist für Prüfläufe da. Ohne ihn wird der eingerichtete
    Spielordner genommen — und ein Prüflauf, der ihn vergisst, schreibt in die
    **echte** Steuerung des Spielers. Genau das ist am 04.09.2026 passiert.
    """
    files = _files()
    if not files:
        return False, 'leer', 0
    # Die Belegung kommt aus dem Spielordner dazu. Fehlt das Spiel (noch nicht
    # eingerichtet), bleibt die Liste leer — das ist kein Fehler.
    files = files + _binding_files(game_folder)
    # ⚠ Erst neben das Ziel schreiben, dann umbenennen. Bricht das Schreiben ab
    # (Stick abgezogen, Platte voll), steht sonst eine halbe Sicherung da, die
    # aussieht wie eine ganze.
    tmp = target + '.teil'
    try:
        with zipfile.ZipFile(tmp, 'w', zipfile.ZIP_DEFLATED) as z:
            for full, rel in files:
                z.write(full, rel)
            z.writestr(INFO_FILE, _info_text(version, len(files)))
        os.replace(tmp, target)
        return True, target, len(files)
    except (OSError, zipfile.BadZipFile) as exc:
        fehler.merken('backup.write', exc)
        try:
            if os.path.isfile(tmp):
                os.remove(tmp)
        except OSError:
            pass
        return False, str(exc), 0


def _info_text(version, count):
    """Eine lesbare Beilage in der Sicherung — für den Menschen, nicht fürs Programm.

    Wer die Datei in einem Jahr findet, soll ohne das Programm erkennen, was er
    da hat. Deshalb Klartext und keine Kennungen.

    ⚠ Auch dieser Text gehört in `sprache.py`: Er landet beim Spieler, und ein
    englischer bekäme sonst eine deutsche Beilage in seiner eigenen Sicherung.
    """
    from .sprache import t
    head = '%s\r\n\r\n' % MARKER
    return head + t('sich_datei_info', time.strftime('%d.%m.%Y %H:%M'),
                    version or '?', count).replace('\n', '\r\n') + '\r\n'


def check(source):
    """Was steckt in dieser Datei? Gibt `(ok, anzahl, erstellt_am)` zurueck.

    ⚠ **Vor dem Zurueckholen fragen, nicht danach.** Wer eine fremde oder
    kaputte ZIP auswaehlt, soll das erfahren, bevor sein Bestand ueberschrieben
    ist.
    """
    try:
        with zipfile.ZipFile(source) as z:
            names = [n for n in z.namelist() if not n.endswith('/')]
            if INFO_FILE not in names:
                return False, 0, ''
            head = z.read(INFO_FILE).decode('utf-8', 'replace')
            if MARKER not in head:
                return False, 0, ''
            when = ''
            for line in head.splitlines():
                if line.startswith('Erstellt am '):
                    when = line[len('Erstellt am '):].split(' mit ')[0]
                    break
            return True, len(names) - 1, when
    except (OSError, zipfile.BadZipFile, KeyError) as exc:
        fehler.merken('backup.check', exc)
        return False, 0, ''


def restore(source):
    """Eine Sicherung einspielen. Gibt `(ok, meldung, anzahl)` zurueck.

    ⚠⚠ **Der vorhandene Stand wird vorher zur Seite gelegt.** Wer sich
    vergreift, hat sonst beides verloren: die alte Sicherung nicht eingespielt
    und den eigenen Bestand ueberschrieben. Die Rueckfall-Datei liegt neben der
    Ablage und traegt Datum und Uhrzeit.

    ⚠ Das Programm muss danach neu starten — die Module halten ihre Daten im
    Arbeitsspeicher und wuerden sie beim naechsten Speichern wieder ueber die
    frisch eingespielten schreiben.
    """
    ok, count, _when = check(source)
    if not ok:
        # ⚠ Ein Kennwort, kein Satz: Was der Spieler liest, steht in
        # `sprache.py`. Ein deutscher Satz an dieser Stelle waere in der
        # englischen Oberflaeche gelandet.
        return False, 'ungueltig', 0

    root = pfade.app_ordner()
    fallback = os.path.join(
        os.path.dirname(root.rstrip(os.sep)) or root,
        'SC-BP-Watcher-vorher-%s.zip' % time.strftime('%Y-%m-%d-%H%M%S'))
    previous_ok, _m, _n = write(fallback)

    try:
        with zipfile.ZipFile(source) as z:
            for name in z.namelist():
                if name.endswith('/') or name == INFO_FILE:
                    continue
                # ⚠⚠ **Die Belegung wird hier ausdruecklich NICHT eingespielt.**
                # Sie gehoert ins Spiel, nicht in unsere Ablage — und eine
                # falsch zurueckgespielte `actionmaps.xml` kostet den Spieler
                # seine komplette Steuerung. Dafuer gibt es
                # `restore_bindings()`, das der Spieler eigens ausloest.
                if name.startswith(CONTROLS):
                    continue
                # ⚠ Kein Pfad darf aus der Ablage herausfuehren. Eine ZIP kann
                # `../../` enthalten (bekannt als „Zip Slip"); ohne diese
                # Pruefung schreibt eine praeparierte Datei irgendwohin.
                target = os.path.normpath(os.path.join(root, name))
                if not target.startswith(os.path.abspath(root) + os.sep):
                    continue
                os.makedirs(os.path.dirname(target), exist_ok=True)
                with z.open(name) as src, open(target, 'wb') as dst:
                    dst.write(src.read())
    except (OSError, zipfile.BadZipFile) as exc:
        fehler.merken('backup.restore', exc)
        return False, str(exc), 0

    _clear_foreign_paths(root)
    return True, (fallback if previous_ok else ''), count


def bindings_in_archive(source):
    """Was an Belegung in dieser Sicherung steckt — Namen, nicht Pfade.

    Damit der Spieler **vorher** sieht, was er einspielen wuerde. Gibt
    `(aktiv_dabei, [Profilnamen])` zurueck.
    """
    active = False
    profiles = []
    try:
        with zipfile.ZipFile(source) as z:
            for name in z.namelist():
                if not name.startswith(CONTROLS) or name.endswith('/'):
                    continue
                rest = name[len(CONTROLS):]
                if rest == 'actionmaps.xml':
                    active = True
                elif rest.startswith('mappings/') and rest.endswith('.xml'):
                    profiles.append(os.path.basename(rest)[:-4])
    except (OSError, zipfile.BadZipFile) as exc:
        fehler.merken('backup.bindings_in_archive', exc)
        return False, []
    return active, sorted(profiles, key=str.lower)


def restore_bindings(source, with_active=False, game_folder=None):
    """Die gesicherte Belegung ins Spiel zurueckspielen.

    ⚠⚠ **Getrennt vom uebrigen Zurueckholen, und mit Absicht umstaendlicher.**
    Die gespeicherten Profile dazuzulegen ist harmlos — sie liegen nur herum,
    bis der Spieler eines laedt. Die **aktive** Belegung zu ueberschreiben ist
    es nicht: Wer sich vergreift, sitzt vor einem Schiff, das auf nichts mehr
    reagiert. Deshalb kommt sie nur mit, wenn `with_active` ausdruecklich
    gesetzt ist — und die alte wird vorher zur Seite gelegt.

    Gibt `(ok, meldung, anzahl)` zurueck.
    """
    from . import joysticks
    ok, _count, _when = check(source)
    if not ok:
        return False, 'ungueltig', 0

    target_folder = joysticks._pfad_mappings(game_folder, anlegen=True)
    active_target = joysticks._pfad_actionmaps(game_folder)
    written = 0
    fallback = ''
    try:
        with zipfile.ZipFile(source) as z:
            for name in z.namelist():
                if not name.startswith(CONTROLS) or name.endswith('/'):
                    continue
                rest = name[len(CONTROLS):]
                if rest == 'actionmaps.xml':
                    if not (with_active and active_target):
                        continue
                    fallback = '%s.scbpw-%s' % (
                        active_target, time.strftime('%Y%m%d-%H%M%S'))
                    with open(active_target, 'rb') as src, \
                            open(fallback, 'wb') as dst:
                        dst.write(src.read())
                    target = active_target
                elif rest == 'attributes.xml':
                    # ⚠⚠ **Gehoert zum aktiven Stand, nicht zu den Profilen.**
                    # Darin steht der Blickwinkel und die Grafik — sie
                    # zurueckzuspielen aendert, wie das Spiel aussieht.
                    # Deshalb nur zusammen mit der aktiven Belegung, und mit
                    # derselben Sicherung daneben.
                    #
                    # Ohne diesen Zweig waere sie zwar im Archiv gelandet,
                    # aber nie wieder herausgekommen — eine Sicherung, die
                    # nicht zurueckkommt, ist keine.
                    if not (with_active and active_target):
                        continue
                    target = os.path.join(os.path.dirname(active_target),
                                          'attributes.xml')
                    if os.path.isfile(target):
                        with open(target, 'rb') as src, \
                                open('%s.scbpw-%s' % (
                                    target, time.strftime('%Y%m%d-%H%M%S')),
                                     'wb') as dst:
                            dst.write(src.read())
                elif rest.startswith('mappings/') and target_folder:
                    # ⚠ Nur der reine Dateiname. Ein Pfad aus dem Archiv duerfte
                    # sonst aus dem Mappings-Ordner herausfuehren.
                    target = os.path.join(target_folder, os.path.basename(rest))
                else:
                    continue
                with z.open(name) as src, open(target, 'wb') as dst:
                    dst.write(src.read())
                written += 1
    except (OSError, zipfile.BadZipFile) as exc:
        fehler.merken('backup.restore_bindings', exc)
        return False, str(exc), written
    return True, fallback, written


# Einstellungen, die einen Ort auf der Platte nennen. Beim Rechnerwechsel sind
# sie der wahrscheinlichste Grund, warum danach nichts geht.
PATH_FIELDS = ('spiel_ordner', 'launcher_ordner', 'export_ordner')


def _clear_foreign_paths(root):
    """Pfade des alten Rechners entfernen, wenn es sie hier nicht gibt.

    ⚠⚠ **Genau dafuer ist diese Funktion da: der Rechnerwechsel.** Auf dem
    alten Rechner stand das Spiel vielleicht unter `D:\\Spiele`, hier liegt es
    woanders. Ein Pfad, der ins Leere zeigt, ist schlimmer als gar keiner: Ein
    leeres Feld laesst das Programm selbst suchen, ein falsches nicht — es
    meldet dann „keine Game.log gefunden", und niemand kommt auf die
    Einstellung, die man gerade eingespielt hat.

    Was hier existiert, bleibt unangetastet: Wer seine Sicherung auf demselben
    Rechner einspielt, soll seine Pfade behalten.

    ⚠ `ablage_ordner` wird **immer** entfernt, auch wenn es ihn gibt. Er steht
    seit dem 04.09.2026 ohnehin in der Zeiger-Datei unter Dokumente und nicht
    mehr hier; eine aeltere Sicherung kann ihn aber noch enthalten. Bliebe er
    stehen, wuerde das Programm nach dem Einspielen woanders hinschauen als
    dorthin, wo der Spieler die Sicherung gerade eingespielt hat.
    """
    target = os.path.join(root, 'Einstellungen', pfade.EINSTELLUNGEN)
    if not os.path.isfile(target):
        return
    try:
        import json
        with open(target, encoding='utf-8') as f:
            data = json.load(f)
        if not isinstance(data, dict):
            return
        changed = data.pop('ablage_ordner', None) is not None
        for field in PATH_FIELDS:
            value = data.get(field)
            if isinstance(value, str) and value.strip() \
                    and not os.path.exists(os.path.expanduser(value)):
                data[field] = ''
                changed = True
        if changed:
            pfade.json_sichern(target, data)
    except Exception as exc:
        fehler.merken('backup.clear_foreign_paths', exc)
