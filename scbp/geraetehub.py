# SPDX-License-Identifier: GPL-3.0-only
#
# SC BP Watcher — alle Eingabegeräte an einem Ort
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
Welcher Stick ist welcher — und woher weiß ich das eigentlich?

## Drei Quellen, die dasselbe Gerät verschieden nennen

Über ein und denselben Joystick gibt es im Rechner **drei** Aussagen, und
keine davon ist für sich vollständig:

| Quelle | Was sie weiß | Was sie nicht weiß |
|---|---|---|
| **das System** (`eingabe.geraete()`) | was **jetzt** angesteckt ist | nichts über das Spiel |
| **die Game.log** (`joysticks.geraete()`) | was das Spiel zuletzt gesehen hat | ob es noch da ist |
| **die actionmaps.xml** (`joysticks.zuordnung()`) | welche `js`-Nummer die Belegung meint | ob es das Gerät gibt |

⚠⚠ **Und die Nummern stimmen nicht überein.** Gemessen am 06.09.2026 an
einem Aufbau mit drei Geräten:

    linker Stick    System: js0    Spiel: js2
    rechter Stick   System: js1    Spiel: js1
    Pedale          System: js2    Spiel: js3

Wer „js1" sagt, muss dazusagen, wessen js1 er meint. Genau daran scheitern
die meisten Anleitungen im Netz.

## Was sie verbindet

Die **Kennung** — dieselbe geschweifte Zeichenfolge in allen drei Quellen
(`03F33344-0000-0000-0000-504944564944`). Sie kommt unter Linux aus
`/sys/class/input`, unter Windows aus `winmm`, im Spiel aus der Game.log.
Über sie lässt sich zusammenführen, was sonst nur nebeneinanderläge.

⚠ **Nie über den Namen.** Dasselbe Gerät heißt an den drei Stellen
verschieden: „VIRPIL Controls 20241226 L-VPC Stick WarBRD-D" im System,
„L-VPC Stick WarBRD-D" im Protokoll, „LEFT VPC Stick WarBRD-D" in der
Belegung — je nachdem, wer es zuletzt umbenannt hat.

## Die Zustände, die dabei herauskommen

| Zustand | Bedeutung |
|---|---|
| `bereit` | angeschlossen, dem Spiel bekannt, hat eine Nummer |
| `ohne_nummer` | angeschlossen, aber die Belegung kennt es nicht — im Spiel tut es nichts |
| `abgesteckt` | die Belegung erwartet es, es ist aber nicht da |
| `unbekannt` | angeschlossen, das Spiel hat es noch nie gesehen |
"""
import time

from . import eingabe, joysticks

# Die Zustände eines Geräts im Hub.
BEREIT = 'bereit'
OHNE_NUMMER = 'ohne_nummer'
ABGESTECKT = 'abgesteckt'
UNBEKANNT = 'unbekannt'


def _kurz(kennung):
    """Die ersten acht Zeichen — sie unterscheiden die Geräte bereits."""
    return (kennung or '')[:8]


def uebersicht(ordner=None, datei=None):
    """Alle Geräte aus allen drei Quellen, über die Kennung zusammengeführt.

    Liefert eine Liste; je Gerät:

    | Feld | Bedeutung |
    |---|---|
    | `kennung` | der gemeinsame Bezugspunkt |
    | `name` | der beste verfügbare Name (Belegung vor Protokoll vor System) |
    | `zustand` | `bereit`, `ohne_nummer`, `abgesteckt` oder `unbekannt` |
    | `nummer` | die `js`-Nummer des Spiels, oder `None` |
    | `systempfad` | `/dev/input/js0` bzw. `joy0`, oder `''` |
    | `angeschlossen` | ist es **jetzt** da? |
    | `im_spiel` | hat das Spiel es schon einmal gesehen? |

    Sortiert: erst was eine Nummer hat (nach Nummer), dann der Rest.
    """
    live = {}
    for geraet in eingabe.geraete() or []:
        if geraet.get('kennung'):
            live[geraet['kennung'].upper()] = geraet

    gesehen = {}
    for geraet in joysticks.geraete(ordner) or []:
        if geraet.get('kennung'):
            gesehen[geraet['kennung'].upper()] = geraet

    belegt = {}
    for eintrag in joysticks.zuordnung(datei, ordner) or []:
        if eintrag.get('kennung'):
            belegt[eintrag['kennung'].upper()] = eintrag

    heraus = []
    for kennung in set(live) | set(gesehen) | set(belegt):
        am_system = live.get(kennung)
        im_log = gesehen.get(kennung)
        in_belegung = belegt.get(kennung)

        # ⚠ Der Name der **Belegung** gewinnt: Den hat der Spieler zuletzt
        # gesehen, und oft hat er ihn selbst vergeben. Der Systemname ist am
        # ausführlichsten, aber auch am sperrigsten („VIRPIL Controls
        # 20241226 L-VPC Stick WarBRD-D").
        name = ''
        for quelle in (in_belegung, im_log, am_system):
            if quelle and quelle.get('name'):
                name = quelle['name']
                break

        if in_belegung and am_system:
            zustand = BEREIT
        elif in_belegung:
            zustand = ABGESTECKT
        elif am_system and im_log:
            zustand = OHNE_NUMMER
        elif am_system:
            zustand = UNBEKANNT
        else:
            # Nur im Protokoll, sonst nirgends: war mal da, ist weg, hat
            # keine Belegung. Das ist Altbestand, kein eigener Zustand.
            zustand = ABGESTECKT

        heraus.append({
            'kennung': kennung,
            'kurz': _kurz(kennung),
            'name': name,
            'zustand': zustand,
            'nummer': (in_belegung or {}).get('nummer'),
            'systempfad': (am_system or {}).get('pfad', ''),
            'systemname': (am_system or {}).get('name', ''),
            'angeschlossen': bool(am_system),
            'im_spiel': bool(im_log),
        })

    heraus.sort(key=lambda g: (g['nummer'] is None, g['nummer'] or 0,
                               g['name'].lower()))
    return heraus


def zusammenfassung(ordner=None, datei=None):
    """Der Hub in Zahlen — für eine Kopfzeile, die den Zustand nennt.

    | Feld | Bedeutung |
    |---|---|
    | `geraete` | die volle Liste |
    | `bereit` / `ohne_nummer` / `abgesteckt` / `unbekannt` | Anzahl je Zustand |
    | `alles_gut` | nichts fehlt, nichts hängt ohne Nummer herum |
    """
    liste = uebersicht(ordner, datei)
    zaehler = {BEREIT: 0, OHNE_NUMMER: 0, ABGESTECKT: 0, UNBEKANNT: 0}
    for geraet in liste:
        zaehler[geraet['zustand']] = zaehler.get(geraet['zustand'], 0) + 1
    return {
        'geraete': liste,
        'bereit': zaehler[BEREIT],
        'ohne_nummer': zaehler[OHNE_NUMMER],
        'abgesteckt': zaehler[ABGESTECKT],
        'unbekannt': zaehler[UNBEKANNT],
        'alles_gut': not (zaehler[ABGESTECKT] or zaehler[OHNE_NUMMER]
                          or zaehler[UNBEKANNT]),
    }


class Wache:
    """Merkt, wenn ein Gerät kommt oder geht.

    ⭐ **Warum das nützt:** Star Citizen liest die Geräte beim Start. Wer
    danach einen Stick absteckt oder umsteckt, merkt es erst im Gefecht —
    oder gar nicht, weil das Spiel die Belegung stillschweigend ins Leere
    laufen lässt.

    ⚠ **Sie fragt nur ab, sie hört nicht zu.** Kein Systemdienst, keine
    Ereignisse, kein Fremdpaket: Bei jedem `pruefen()` wird die Geräteliste
    einmal gelesen und mit der vorigen verglichen. Das kostet unter Linux ein
    `listdir` — wenig genug, um es alle paar Sekunden zu tun, und es
    funktioniert auf beiden Systemen gleich.

    Benutzung:

        wache = geraetehub.Wache()
        wache.pruefen()          # der erste Aufruf setzt nur die Grundlage
        …
        neu, weg = wache.pruefen()
    """

    def __init__(self):
        self.stand = None
        self.zuletzt = 0.0

    def pruefen(self, mindestabstand=0.0):
        """Was hat sich seit dem letzten Mal geändert?

        Liefert `(dazugekommen, verschwunden)` — je eine Liste von Geräten
        wie in `uebersicht()`. Beim **ersten** Aufruf immer `([], [])`: Da
        gibt es nichts zu vergleichen, und alles als „neu" zu melden wäre ein
        Fehlalarm bei jedem Programmstart.

        `mindestabstand` in Sekunden bremst die Abfrage; ein Aufruf davor
        liefert `([], [])`, ohne etwas zu lesen.
        """
        jetzt = time.time()
        if mindestabstand and (jetzt - self.zuletzt) < mindestabstand:
            return [], []
        self.zuletzt = jetzt

        aktuell = {}
        for geraet in eingabe.geraete() or []:
            if geraet.get('kennung'):
                aktuell[geraet['kennung'].upper()] = geraet

        if self.stand is None:
            self.stand = aktuell
            return [], []

        dazu = [aktuell[k] for k in aktuell if k not in self.stand]
        weg = [self.stand[k] for k in self.stand if k not in aktuell]
        self.stand = aktuell
        return dazu, weg
