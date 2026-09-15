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
| **das System** (`input_device.devices()`) | was **jetzt** angesteckt ist | nichts über das Spiel |
| **die Game.log** (`joysticks.devices()`) | was das Spiel zuletzt gesehen hat | ob es noch da ist |
| **die actionmaps.xml** (`joysticks.assignment()`) | welche `js`-Nummer die Belegung meint | ob es das Gerät gibt |

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

from . import input_device, joysticks

# Die Zustände eines Geräts im Hub.
READY = 'bereit'
NO_NUMBER = 'ohne_nummer'
UNPLUGGED = 'abgesteckt'
UNKNOWN = 'unbekannt'


def _short(ident):
    """Die ersten acht Zeichen — sie unterscheiden die Geräte bereits."""
    return (ident or '')[:8]


def overview(folder=None, filename=None):
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
    for device in input_device.devices() or []:
        if device.get('kennung'):
            live[device['kennung'].upper()] = device

    seen = {}
    for device in joysticks.devices(folder) or []:
        if device.get('kennung'):
            seen[device['kennung'].upper()] = device

    in_use = {}
    for entry in joysticks.assignment(filename, folder) or []:
        if entry.get('kennung'):
            in_use[entry['kennung'].upper()] = entry

    out = []
    for ident in set(live) | set(seen) | set(in_use):
        on_system = live.get(ident)
        in_log = seen.get(ident)
        in_binding = in_use.get(ident)

        # ⚠ Der Name der **Belegung** gewinnt: Den hat der Spieler zuletzt
        # gesehen, und oft hat er ihn selbst vergeben. Der Systemname ist am
        # ausführlichsten, aber auch am sperrigsten („VIRPIL Controls
        # 20241226 L-VPC Stick WarBRD-D").
        name = ''
        for source in (in_binding, in_log, on_system):
            if source and source.get('name'):
                name = source['name']
                break

        if in_binding and on_system:
            status = READY
        elif in_binding:
            status = UNPLUGGED
        elif on_system and in_log:
            status = NO_NUMBER
        elif on_system:
            status = UNKNOWN
        else:
            # Nur im Protokoll, sonst nirgends: war mal da, ist weg, hat
            # keine Belegung. Das ist Altbestand, kein eigener Zustand.
            status = UNPLUGGED

        out.append({
            'kennung': ident,
            'kurz': _short(ident),
            'name': name,
            'zustand': status,
            'nummer': (in_binding or {}).get('nummer'),
            'systempfad': (on_system or {}).get('pfad', ''),
            'systemname': (on_system or {}).get('name', ''),
            'angeschlossen': bool(on_system),
            'im_spiel': bool(in_log),
        })

    out.sort(key=lambda g: (g['nummer'] is None, g['nummer'] or 0,
                               g['name'].lower()))
    return out


def summary(folder=None, filename=None):
    """Der Hub in Zahlen — für eine Kopfzeile, die den Zustand nennt.

    | Feld | Bedeutung |
    |---|---|
    | `geraete` | die volle Liste |
    | `bereit` / `ohne_nummer` / `abgesteckt` / `unbekannt` | Anzahl je Zustand |
    | `alles_gut` | nichts fehlt, nichts hängt ohne Nummer herum |
    """
    items = overview(folder, filename)
    counter = {READY: 0, NO_NUMBER: 0, UNPLUGGED: 0, UNKNOWN: 0}
    for device in items:
        counter[device['zustand']] = counter.get(device['zustand'], 0) + 1
    return {
        'geraete': items,
        'bereit': counter[READY],
        'ohne_nummer': counter[NO_NUMBER],
        'abgesteckt': counter[UNPLUGGED],
        'unbekannt': counter[UNKNOWN],
        'alles_gut': not (counter[UNPLUGGED] or counter[NO_NUMBER]
                          or counter[UNKNOWN]),
    }


# Was der Assistent vorschlagen kann.
SWAP = 'tausch'        # dasselbe Gerät unter neuer Kennung → umhängen
START = 'starten'      # das Spiel kennt es noch nicht → einmal starten
PLUG_IN = 'anstecken'  # die Belegung erwartet es → anstecken oder aufräumen


def suggestions(folder=None, filename=None):
    """Was ist zu tun? Konkrete Schritte statt bloßer Zustände.

    ## Der Fall, für den das gebaut ist

    Ein Stick bekommt eine neue Kennung — anderer USB-Anschluss, neue
    Firmware, Neuinstallation. Danach steht in der Übersicht **zweimal
    dasselbe Gerät**: einmal als `abgesteckt` (die alte Kennung, an der die
    ganze Belegung hängt) und einmal als `ohne_nummer` (die neue, die das
    Spiel noch nicht kennt). Wer das nicht weiß, sieht zwei Probleme, wo
    eines ist — und die Lösung ist ein einziger Handgriff:
    `joysticks.swap_id()` hängt die alte Belegung an die neue
    Kennung, ohne eine einzige Belegungszeile anzufassen.

    ## ⚠⚠ Geraten wird nicht

    Ein Vorschlag zum Umhängen entsteht **nur**, wenn genau **ein** Gerät
    fehlt und genau **ein** neues ohne Nummer dasteht. Bei mehreren wäre die
    Zuordnung Ratearbeit — und ein falsch geratener Ersatz vertauscht zwei
    Sticks, was man erst im Gefecht merkt. Dieselbe Vorsicht wie in
    `joysticks.compare()`, aus demselben Grund.

    ⚠ Der **Name** spielt dabei keine Rolle, auch wenn er verlockend wäre.
    Dasselbe Gerät heißt an den drei Stellen verschieden; ein Namensvergleich
    wäre Ratearbeit mit gutem Gefühl.

    Liefert je Vorschlag:

    | Feld | Bedeutung |
    |---|---|
    | `art` | `tausch`, `starten` oder `anstecken` |
    | `geraet` | das betroffene Gerät aus `uebersicht()` |
    | `alt` | beim Tausch: der Eintrag, dessen Belegung übernommen wird |
    """
    items = overview(folder, filename)
    missing = [g for g in items if g['zustand'] == UNPLUGGED]
    without = [g for g in items if g['zustand'] == NO_NUMBER]
    fresh = [g for g in items if g['zustand'] == UNKNOWN]

    out = []

    # ⭐ Der eine eindeutige Fall — und nur der.
    #
    # ⚠ Auch ein `unbekanntes` Gerät kommt als Kandidat in Frage: Ob das
    # Spiel die neue Kennung schon einmal gesehen hat, hängt bloß daran, ob
    # seither eine Runde gespielt wurde. Für die Frage „ist das derselbe
    # Stick unter neuem Namen" sagt das nichts aus.
    candidates = without + fresh
    if len(missing) == 1 and len(candidates) == 1:
        out.append({'art': SWAP, 'geraet': candidates[0],
                       'alt': missing[0]})
        return out

    # Sonst: je Gerät der Schritt, der für sich genommen stimmt.
    for device in fresh:
        out.append({'art': START, 'geraet': device, 'alt': None})
    for device in missing:
        out.append({'art': PLUG_IN, 'geraet': device, 'alt': None})
    return out


def reassign(old_id, new_id, filename=None, folder=None):
    """Die Belegung eines Geräts auf seine neue Kennung umhängen.

    Reicht an `joysticks.swap_id()` durch — der Schritt, den der
    Vorschlag `tausch` anbietet. Steht hier, damit die Oberfläche nur ein
    Modul kennen muss.

    ⚠ **Es wird keine einzige Belegungszeile angefasst.** Getauscht wird die
    Kennung im Kopf der Datei; alle `js<n>_`-Zeilen zeigen danach wieder auf
    ein Gerät, das da ist.
    """
    # ⛔ Schluesselwort-Aufruf: Die Parameter heissen seit P4 Stufe 10c
    # `filename`/`folder` — wer sie hier umbenennt, zieht `swap_id` mit.
    return joysticks.swap_id(old_id, new_id,
                                      filename=filename, folder=folder)


class Watchdog:
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

        watchdog = device_hub.Watchdog()
        watchdog.check()          # der erste Aufruf setzt nur die Grundlage
        …
        neu, weg = watchdog.check()
    """

    def __init__(self):
        self.state = None
        self.last = 0.0

    def check(self, min_gap=0.0):
        """Was hat sich seit dem letzten Mal geändert?

        Liefert `(dazugekommen, verschwunden)` — je eine Liste von Geräten
        wie in `uebersicht()`. Beim **ersten** Aufruf immer `([], [])`: Da
        gibt es nichts zu vergleichen, und alles als „neu" zu melden wäre ein
        Fehlalarm bei jedem Programmstart.

        `mindestabstand` in Sekunden bremst die Abfrage; ein Aufruf davor
        liefert `([], [])`, ohne etwas zu lesen.
        """
        now = time.time()
        if min_gap and (now - self.last) < min_gap:
            return [], []
        self.last = now

        current = {}
        for device in input_device.devices() or []:
            if device.get('kennung'):
                current[device['kennung'].upper()] = device

        if self.state is None:
            self.state = current
            return [], []

        added = [current[k] for k in current if k not in self.state]
        gone = [self.state[k] for k in self.state if k not in current]
        self.state = current
        return added, gone
