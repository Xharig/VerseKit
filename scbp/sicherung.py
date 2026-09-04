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
Zwischenspeicher, die sich jederzeit neu laden lassen (`NACHLADBAR`). Kommt
morgen eine neue eigene Datei dazu, ist sie ohne Zutun in der Sicherung. Der
schlimmste Fall ist dann eine etwas größere Datei — nicht ein fehlender
Bestand.

⚠ **Ein Rückweg gehört dazu.** Eine Sicherung, die sich nur schreiben lässt,
löst den Rechnerwechsel nicht: Der Spieler müsste die Dateien von Hand in einen
Ordner legen, den er nicht kennt. `zurueckholen()` ist deshalb kein Zusatz,
sondern die zweite Hälfte derselben Funktion.
"""
import os
import time
import zipfile

from . import fehler, pfade

# Die Kennung im Kopf der Datei — daran ist eine Sicherung dieses Programms zu
# erkennen, auch wenn jemand sie umbenannt hat.
KENNUNG = 'SC-BP-Watcher-Sicherung'
INFODATEI = 'sicherung.txt'

# Was NICHT mitkommt: heruntergeladene Nachschlagewerke und Spuren des
# laufenden Betriebs. Zusammen sind das mehrere Megabyte, und jede Datei davon
# holt sich das Programm beim nächsten Start von allein zurück.
#
# ⚠ Im Zweifel gehört etwas NICHT hierher. Eine zu große Sicherung kostet
# Sekunden, eine zu kleine kostet den Bestand.
NACHLADBAR = (
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
    # ⚠ Der Lesestand gehört ausdrücklich NICHT mit: Er zeigt auf Logdateien
    # des alten Rechners. Am neuen wäre er falsch und würde die Nachlese
    # überspringen — das Auftrags-Protokoll bliebe leer.
    'Intern/logstand.json',
)

# Ganze Ordner, die draußen bleiben.
NACHLADBARE_ORDNER = (
    'export',       # wird bei jedem Fund neu geschrieben
    'Diagnose',     # Fehlerprotokolle des alten Rechners
)


def _mitnehmen(rel):
    """Gehört diese Datei (Pfad relativ zur Ablage) in die Sicherung?"""
    pfad = rel.replace('\\', '/')
    if pfad in NACHLADBAR:
        return False
    erster = pfad.split('/', 1)[0]
    if erster in NACHLADBARE_ORDNER:
        return False
    # Die Sicherung selbst nicht mitsichern, falls sie jemand in die Ablage legt.
    return not pfad.lower().endswith('.zip')


def _dateien():
    """Alle mitzunehmenden Dateien der Ablage — (voller Pfad, Name in der Datei)."""
    wurzel = pfade.app_ordner()
    gefunden = []
    for ordner, _unter, namen in os.walk(wurzel):
        for name in namen:
            voll = os.path.join(ordner, name)
            rel = os.path.relpath(voll, wurzel)
            if _mitnehmen(rel):
                gefunden.append((voll, rel.replace('\\', '/')))
    return sorted(gefunden, key=lambda x: x[1])


def vorschlag():
    """Ein Dateiname mit Datum — eine Sicherung hält einen Stand fest."""
    return 'SC-BP-Watcher-Sicherung-%s.zip' % time.strftime('%Y-%m-%d')


def schreiben(ziel, version=''):
    """Alles Eigene in eine ZIP-Datei schreiben.

    Gibt `(ok, meldung, anzahl)` zurück. Die Meldung ist für den Spieler
    gedacht und nennt im Fehlerfall den Grund — ein stilles `False` hilft
    niemandem.
    """
    dateien = _dateien()
    if not dateien:
        return False, 'leer', 0
    # ⚠ Erst neben das Ziel schreiben, dann umbenennen. Bricht das Schreiben ab
    # (Stick abgezogen, Platte voll), steht sonst eine halbe Sicherung da, die
    # aussieht wie eine ganze.
    temp = ziel + '.teil'
    try:
        with zipfile.ZipFile(temp, 'w', zipfile.ZIP_DEFLATED) as z:
            for voll, rel in dateien:
                z.write(voll, rel)
            z.writestr(INFODATEI, _infotext(version, len(dateien)))
        os.replace(temp, ziel)
        return True, ziel, len(dateien)
    except (OSError, zipfile.BadZipFile) as ausnahme:
        fehler.merken('sicherung.schreiben', ausnahme)
        try:
            if os.path.isfile(temp):
                os.remove(temp)
        except OSError:
            pass
        return False, str(ausnahme), 0


def _infotext(version, anzahl):
    """Eine lesbare Beilage in der Sicherung — für den Menschen, nicht fürs Programm.

    Wer die Datei in einem Jahr findet, soll ohne das Programm erkennen, was er
    da hat. Deshalb Klartext und keine Kennungen.

    ⚠ Auch dieser Text gehört in `sprache.py`: Er landet beim Spieler, und ein
    englischer bekäme sonst eine deutsche Beilage in seiner eigenen Sicherung.
    """
    from .sprache import t
    kopf = '%s\r\n\r\n' % KENNUNG
    return kopf + t('sich_datei_info', time.strftime('%d.%m.%Y %H:%M'),
                    version or '?', anzahl).replace('\n', '\r\n') + '\r\n'


def pruefen(quelle):
    """Was steckt in dieser Datei? Gibt `(ok, anzahl, erstellt_am)` zurueck.

    ⚠ **Vor dem Zurueckholen fragen, nicht danach.** Wer eine fremde oder
    kaputte ZIP auswaehlt, soll das erfahren, bevor sein Bestand ueberschrieben
    ist.
    """
    try:
        with zipfile.ZipFile(quelle) as z:
            namen = [n for n in z.namelist() if not n.endswith('/')]
            if INFODATEI not in namen:
                return False, 0, ''
            kopf = z.read(INFODATEI).decode('utf-8', 'replace')
            if KENNUNG not in kopf:
                return False, 0, ''
            wann = ''
            for zeile in kopf.splitlines():
                if zeile.startswith('Erstellt am '):
                    wann = zeile[len('Erstellt am '):].split(' mit ')[0]
                    break
            return True, len(namen) - 1, wann
    except (OSError, zipfile.BadZipFile, KeyError) as ausnahme:
        fehler.merken('sicherung.pruefen', ausnahme)
        return False, 0, ''


def zurueckholen(quelle):
    """Eine Sicherung einspielen. Gibt `(ok, meldung, anzahl)` zurueck.

    ⚠⚠ **Der vorhandene Stand wird vorher zur Seite gelegt.** Wer sich
    vergreift, hat sonst beides verloren: die alte Sicherung nicht eingespielt
    und den eigenen Bestand ueberschrieben. Die Rueckfall-Datei liegt neben der
    Ablage und traegt Datum und Uhrzeit.

    ⚠ Das Programm muss danach neu starten — die Module halten ihre Daten im
    Arbeitsspeicher und wuerden sie beim naechsten Speichern wieder ueber die
    frisch eingespielten schreiben.
    """
    ok, anzahl, _wann = pruefen(quelle)
    if not ok:
        # ⚠ Ein Kennwort, kein Satz: Was der Spieler liest, steht in
        # `sprache.py`. Ein deutscher Satz an dieser Stelle waere in der
        # englischen Oberflaeche gelandet.
        return False, 'ungueltig', 0

    wurzel = pfade.app_ordner()
    rueckfall = os.path.join(
        os.path.dirname(wurzel.rstrip(os.sep)) or wurzel,
        'SC-BP-Watcher-vorher-%s.zip' % time.strftime('%Y-%m-%d-%H%M%S'))
    vorher_ok, _m, _n = schreiben(rueckfall)

    try:
        with zipfile.ZipFile(quelle) as z:
            for name in z.namelist():
                if name.endswith('/') or name == INFODATEI:
                    continue
                # ⚠ Kein Pfad darf aus der Ablage herausfuehren. Eine ZIP kann
                # `../../` enthalten (bekannt als „Zip Slip"); ohne diese
                # Pruefung schreibt eine praeparierte Datei irgendwohin.
                ziel = os.path.normpath(os.path.join(wurzel, name))
                if not ziel.startswith(os.path.abspath(wurzel) + os.sep):
                    continue
                os.makedirs(os.path.dirname(ziel), exist_ok=True)
                with z.open(name) as her, open(ziel, 'wb') as hin:
                    hin.write(her.read())
    except (OSError, zipfile.BadZipFile) as ausnahme:
        fehler.merken('sicherung.zurueckholen', ausnahme)
        return False, str(ausnahme), 0

    _fremde_pfade_leeren(wurzel)
    return True, (rueckfall if vorher_ok else ''), anzahl


# Einstellungen, die einen Ort auf der Platte nennen. Beim Rechnerwechsel sind
# sie der wahrscheinlichste Grund, warum danach nichts geht.
PFAD_FELDER = ('spiel_ordner', 'launcher_ordner', 'export_ordner')


def _fremde_pfade_leeren(wurzel):
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
    ziel = os.path.join(wurzel, 'Einstellungen', pfade.EINSTELLUNGEN)
    if not os.path.isfile(ziel):
        return
    try:
        import json
        with open(ziel, encoding='utf-8') as f:
            daten = json.load(f)
        if not isinstance(daten, dict):
            return
        geaendert = daten.pop('ablage_ordner', None) is not None
        for feld in PFAD_FELDER:
            wert = daten.get(feld)
            if isinstance(wert, str) and wert.strip() \
                    and not os.path.exists(os.path.expanduser(wert)):
                daten[feld] = ''
                geaendert = True
        if geaendert:
            pfade.json_sichern(ziel, daten)
    except Exception as ausnahme:
        fehler.merken('sicherung.pfade_leeren', ausnahme)
