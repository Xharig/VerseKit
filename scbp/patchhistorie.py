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
Welcher Patch hat welche Baupläne gebracht — dauerhaft festgehalten.

**Warum es dieses Modul gibt.** Der Vergleich lief früher gegen den Katalog, der
gerade auf der Platte lag. Das ging schief: Am 26.08.2026 meldete das Werkzeug
74 neue Baupläne, von denen **53 längst im Spiel waren**. Und nachsehen ließ es
sich nicht mehr — scmdb hält nur die aktuelle Spielversion vor, die Daten zu
4.9.0 waren am selben Tag schon gelöscht.

Daraus folgen die zwei Aufgaben hier:

  **Nichts geht mehr verloren.** Was ein Patch gebracht hat, wird
  festgeschrieben — unabhängig davon, wie lange die Quelle ihre Daten vorhält.

  **Der Vergleich wird verlässlich.** Verglichen wird gegen die Liste aller je
  gesehenen Baupläne, nicht gegen einen Zwischenspeicher, der sich unter der
  Hand ändern kann.

Drei Dateien, mit Absicht getrennt:

    daten/patch-historie.json     mitgeliefert, im Repo, klein
    <Nutzer>/patch-historie.json  was dieses Gerät selbst beobachtet hat
    <Nutzer>/bauplaene-gesehen.json   alle je gesehenen Namen (Vergleichsgrundlage)

⚠ **Warum die Historie nur die Zugänge führt und nie den ganzen Katalog.**
Eine vollständige Bauplan-Liste wäre ein wesentlicher Teil der Datenbank von
scmdb — die steht unter CC BY-NC-ND, und das Datenbankherstellerrecht schützt
das Sammeln unabhängig davon, wie man die Daten hinterher aufbereitet. Die
Zugänge je Patch sind etwas anderes: eine Handvoll Namen, die dieses Werkzeug
selbst beobachtet hat, und die Namen gehören ohnehin CIG. Deshalb steht in der
Datei ausdrücklich, woher sie stammt und dass sie weitergegeben werden darf —
so wie bei `daten/katalog.json` auch.

Die Namen werden **im Klartext** gespeichert, nicht in der Vergleichsform. Die
Datei liegt im Repo und soll dort lesbar sein: Wer sich einen Patch ansieht,
soll „MISC Ore Pod" lesen und nicht „miscorepod". Verglichen wird trotzdem über
die Vergleichsform, sonst scheitert es an Schreibweisen.
"""
import json
import os
import time

from . import pfade

MITGELIEFERT = 'patch-historie.json'
LOKAL = 'patch-historie.json'
GESEHEN = 'bauplaene-gesehen.json'


def _norm(s):
    """Vergleichsform — dieselbe wie im Katalog, damit beide zueinander passen."""
    return pfade.namensform(s)


# ----------------------------------------------------------------- Historie
def _lies(pfad):
    try:
        with open(pfad, encoding='utf-8') as f:
            d = json.load(f)
        return d.get('patches') or {}
    except Exception:
        return {}


def _vereinen(alt, neu):
    """Zwei Einträge derselben Spielversion zu einem zusammenfassen.

    ⚠ **Vereinigen, nicht ersetzen.** Hier stand einmal schlicht ein `update()`,
    und damit warf jeder eigene Fund die mitgelieferte Liste derselben Version
    weg. Am 28.08.2026 sah der Watcher drei nachgereichte Schiffswaffen in
    4.10.0, schrieb sie als *die* Zugänge dieser Version — und aus dem Filter
    „4.10.0" verschwanden die 21 mitgelieferten Baupläne. Von 24 blieben 3.

    Der Grund liegt in der Natur der eigenen Funde: Was `eintragen()` schreibt,
    ist immer nur der **Zuwachs seit dem letzten Lauf**, nie die vollständige
    Liste eines Patches. Als vollständige Liste gelesen ist sie zwangsläufig
    falsch, sobald eine Quelle etwas nachreicht.

    Beim Datum gewinnt das **frühere**. Der Patch kam, als er kam — dass dieses
    Gerät zwei Tage später noch etwas nachgetragen bekam, verschiebt ihn nicht."""
    namen = list(alt.get('neu') or [])
    bekannt = {_norm(n) for n in namen}
    for name in neu.get('neu') or []:
        if _norm(name) not in bekannt:
            bekannt.add(_norm(name))
            namen.append(name)
    daten = [d for d in (alt.get('datum'), neu.get('datum')) if d]
    return {'datum': min(daten) if daten else '', 'neu': sorted(namen)}


def laden():
    """Die ganze Historie: was mitgeliefert wurde, ergänzt um eigene Funde.

    Bei gleicher Spielversion werden beide Listen **vereinigt** — was in
    `_vereinen()` steht, gilt hier: keine der beiden Seiten kennt den Patch
    vollständig, erst zusammen ergeben sie ihn."""
    zusammen = {}
    for pfad in (pfade.programm_datei(MITGELIEFERT), pfade.app_datei(LOKAL)):
        for version, eintrag in _lies(pfad).items():
            alt = zusammen.get(version)
            zusammen[version] = _vereinen(alt, eintrag) if alt else dict(eintrag)
    return zusammen


def eintragen(version, namen, datum=None):
    """Einen Patch in die **eigene** Historie schreiben. Die mitgelieferte Datei
    bleibt unangetastet — sie gehört zum Programm, nicht zum Nutzer.

    ⚠ Auch hier wird **ergänzt**, nicht ersetzt: Reicht die Quelle in derselben
    Spielversion später etwas nach, wäre der erste eigene Fund sonst weg."""
    if not version or not namen:
        return
    ziel = pfade.app_datei(LOKAL)
    eigene = _lies(ziel)
    neu = {'datum': datum or time.strftime('%Y-%m-%d'), 'neu': sorted(namen)}
    eigene[version] = (_vereinen(eigene[version], neu)
                       if version in eigene else neu)
    _schreib(ziel, eigene)


def _schreib(ziel, patches):
    daten = {
        'hinweis': ('Welcher Patch welche Baupläne gebracht hat. Nur die '
                    'Zugänge je Spielversion, nie der ganze Katalog.'),
        'quelle': 'eigene Beobachtung von VerseKit',
        'weitergabe': True,
        'patches': patches,
    }
    try:
        os.makedirs(os.path.dirname(ziel), exist_ok=True)
        temp = ziel + '.tmp'
        with open(temp, 'w', encoding='utf-8') as f:
            json.dump(daten, f, ensure_ascii=False, indent=1)
        os.replace(temp, ziel)
    except Exception:
        pass


def version_je_bauplan():
    """Vergleichsform -> Spielversion, in der der Bauplan zuerst auftauchte.

    Taucht derselbe Name in mehreren Patches auf (kann passieren, wenn eine
    Quelle ihn zwischendurch verliert und wiederbringt), gilt der **früheste**
    Eintrag — sonst wandert ein alter Bauplan bei jedem Wackler nach vorn."""
    ergebnis = {}
    for version, eintrag in sorted(laden().items(), key=lambda p: rang(p[0])):
        for name in eintrag.get('neu') or []:
            ergebnis.setdefault(_norm(name), version)
    return ergebnis


def rang(version):
    """Sortierschlüssel: 4.10.0 gehört hinter 4.9.0, nicht davor.

    Als Text verglichen käme „4.9" nach „4.10", weil „9" größer ist als „1"."""
    kurz = (version or '').split('-')[0]
    return [int(x) if x.isdigit() else 0 for x in kurz.split('.')]


def patches():
    """[(volle Version, kurze Version, Anzahl), …] — neueste zuerst."""
    d = laden()
    return [(v, (v.split('-')[0] or v), len(d[v].get('neu') or []))
            for v in sorted(d, key=rang, reverse=True)]


def neueste():
    """Die jüngste Spielversion der Historie, oder ''."""
    d = laden()
    return sorted(d, key=rang)[-1] if d else ''


# ------------------------------------------------------- Alle je gesehenen
def gesehen():
    """Alle Baupläne, die dieses Gerät je im Katalog gesehen hat (Vergleichsform).

    Das ist die Vergleichsgrundlage — **nicht** der letzte Katalog. Der Unterschied
    zählt: Verliert eine Quelle zwischendurch Einträge und bringt sie später
    zurück, gelten sie sonst als neu, obwohl sich im Spiel nichts getan hat."""
    try:
        with open(pfade.app_datei(GESEHEN), encoding='utf-8') as f:
            return set(json.load(f).get('namen') or [])
    except Exception:
        return set()


def gesehen_setzen(schluessel):
    """Die Vergleichsgrundlage überschreiben."""
    ziel = pfade.app_datei(GESEHEN)
    try:
        os.makedirs(os.path.dirname(ziel), exist_ok=True)
        temp = ziel + '.tmp'
        with open(temp, 'w', encoding='utf-8') as f:
            json.dump({'stand': time.strftime('%Y-%m-%d %H:%M:%S'),
                       'namen': sorted(schluessel)}, f, ensure_ascii=False)
        os.replace(temp, ziel)
    except Exception:
        pass
