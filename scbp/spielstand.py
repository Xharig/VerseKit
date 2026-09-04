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
Welcher Spielstand gerade live ist — und ob unsere Zahlen noch dazu passen.

## Wozu das gut ist

Preise, Lagerorte und Ankaufgebote gelten immer für **einen** Spielstand. Kommt
ein Patch, wirft CIG regelmäßig Preise um, benennt Stationen um oder nimmt
Terminals heraus. Unsere Ablage merkt davon nichts: Sie ist einen Tag lang
„frisch" und zeigt weiter die Zahlen von vorgestern.

Das ist die eine Sorte Fehler, die ein Werkzeug **niemals** machen darf — es
sagt etwas Falsches mit der gleichen Bestimmtheit wie etwas Richtiges. Ein
leeres Feld ist ehrlich, eine alte Zahl ohne Hinweis ist es nicht.

## ⚠⚠ Das ist der DRITTE Anlauf — die ersten beiden waren falsch

In `verkauf.py` steht seit dem 30.08.2026 ausdrücklich, dass dort **kein**
Spielstand hingehört. Diese Warnung bleibt richtig, und dieses Modul verstößt
nicht dagegen — es holt die Angabe aus einer **anderen** Quelle:

| Anlauf | Woher | Warum es scheiterte |
|---|---|---|
| 1. | `game_version` in den Preisdaten | Das Feld gibt es dort gar nicht |
| 2. | `game_version` an den Terminals | Bedeutet „in dieser Version zuletzt gesehen" — verteilt sich über 826 Terminals auf 3.24.2 (151×), 4.6.0 (126×), 4.0 (106×) und 84 ohne Angabe. Der häufigste Wert wäre `3.24.2` gewesen, während die Preise aus 4.10.0 stammten |
| 3. | **eigener Endpunkt `game_versions`** | Sagt genau eines: was gerade live ist |

Der Unterschied ist nicht Geschmack, sondern Bedeutung: Die ersten beiden Felder
beschreiben **einzelne Datensätze**, dieser Endpunkt beschreibt **das Spiel**.

Gemessen am 04.09.2026 antwortet er mit `{'live': '4.10.0', 'ptu': None}` — ein
Wert, kein Verteilungsproblem.

## Was daraus folgt

`uex.Ablage` schreibt beim Sichern mit, unter welchem Spielstand die Daten
geholt wurden. Stimmt der später nicht mehr mit `live()` überein, gilt die
Ablage als **überholt** — unabhängig von ihrem Alter. Der Reiter zeigt die
Zahlen dann weiter, sagt aber dazu, dass ein Patch dazwischen liegt.

⚠ **Nicht wegwerfen, nur kennzeichnen.** Alte Preise sind besser als keine:
Nach einem Patch dauert es Tage, bis die Spielergemeinde die neuen Zahlen
gemeldet hat. Wer in dieser Zeit gar nichts sieht, ist schlechter dran als wer
alte Zahlen mit Warnung sieht.

⚠ **Ohne Netz gibt es keine Warnung, und das ist richtig so.** Wer den
Spielstand nicht kennt, kann nicht behaupten, die Daten seien überholt.
`live()` liefert dann `''`, und `ueberholt()` sagt `False`.
"""
from . import uex

QUELLE = 'https://api.uexcorp.uk/2.0/game_versions'
CACHE = 'spielstand.json'
FORMAT = 1

# Zwölf Stunden. Ein Patch erscheint nicht überraschend mitten am Tag, aber wer
# abends spielt, soll den Freitags-Patch nicht erst am Samstag bemerken.
HALTBAR = uex.TAG // 2

_ablage = uex.Ablage(CACHE, format_nr=FORMAT, haltbar=HALTBAR)


def laden():
    """Der abgelegte Stand — oder `{}`."""
    return _ablage.laden()


def live():
    """Die Versionsnummer des Live-Spiels, etwa `'4.10.0'` — oder `''`.

    Leer heißt **nicht** „kein Patch", sondern „wir wissen es nicht". Der
    Unterschied entscheidet darüber, ob eine Warnung angezeigt werden darf.
    """
    return (laden() or {}).get('live') or ''


def ptu():
    """Die Testversion, wenn gerade eine läuft — sonst `''`.

    Nur zur Anzeige. Für die Frage, ob unsere Zahlen passen, zählt `live()`:
    Wer auf dem Testserver spielt, hat ohnehin eigene Preise.
    """
    return (laden() or {}).get('ptu') or ''


def aktualisieren():
    """Den Spielstand holen, wenn die Ablage fehlt oder älter als 12 h ist."""
    if not _ablage.veraltet():
        return True
    roh = uex.holen(QUELLE, 'spielstand')
    # ⚠ Dieser Endpunkt liefert ein Wörterbuch, keine Liste — anders als jeder
    # andere. `uex.holen` reicht beides unverändert durch.
    if not isinstance(roh, dict):
        return False
    stand = (roh.get('live') or '').strip()
    if not stand:
        return False
    return _ablage.sichern({'live': stand,
                            'ptu': (roh.get('ptu') or '').strip()})


def ueberholt(ablage):
    """Liegt ein Patch zwischen dieser Ablage und dem Spiel?

    `ablage` ist eine `uex.Ablage`. Gibt `(ja, stand_der_ablage, live)` zurück
    — die beiden Nummern, damit die Oberfläche sie nennen kann statt nur
    „veraltet" zu sagen.

    ⚠ **Im Zweifel NEIN.** Kennen wir den Live-Stand nicht (kein Netz, erster
    Start) oder trägt die Ablage keinen, wird nicht gewarnt. Eine Warnung, die
    auf Unwissen beruht, ist schlimmer als keine: Sie lehrt den Spieler, sie zu
    überlesen.
    """
    jetzt = live()
    damals = (ablage.laden() or {}).get('spielstand') or ''
    if not jetzt or not damals:
        return False, damals, jetzt
    return damals != jetzt, damals, jetzt
