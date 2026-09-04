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
Lagerorte — Stationen, Städte und Aussenposten aus Star Citizen.

Das Lager fragt nach einem Lagerort. Der war bis v3.3.0-rc40 ein **freies
Textfeld** — und damit dasselbe Problem wie beim Rohstoffnamen: Jemand tippt
etwas Beleidigendes hinein, macht ein Bildschirmfoto und verbreitet es. Am Ende
fragt niemand, wer getippt hat; es steht in diesem Werkzeug.

⚠⚠ **Also auch hier eine geschlossene Liste.** Am 30.08.2026 festgelegt:
„Lagerort gilt exakt das Gleiche." Und: „Bei Oma im Keller ist eben keine
Location mit Lager in SC."

## Woher

[UEX Corp](https://uexcorp.space) API 2.0, Endpunkt `terminals` — 826 Terminals
mit Angabe der Raumstation, Stadt oder des Aussenpostens, an dem sie stehen.
Daraus werden die **158 verschiedenen Orte** gezogen; die Terminals selbst
(„Casaba Outlet - Area 18") interessieren nicht.

⚠ **Vollständig geprüft**: Orison, Area 18, Lorville, New Babbage, Baijini
Point, Everus Harbor, Ruin Station — alle dabei. Was auf den ersten Blick
fehlte, heisst dort nur anders: `Pyro Gateway (Stanton)` statt „Pyro Gateway",
`Checkmate Station` statt „Checkmate". Deshalb muss der Vorschlag Teiltexte
finden, nicht nur Wortanfänge.

⚠ Die Daten werden **nicht mitgeliefert**, sondern auf dem Rechner des Nutzers
geholt — wie bei scmdb und den Preisen. Und **höchstens einmal pro Woche**:
Stationen kommen mit einem Spiel-Patch dazu, nicht über Nacht.

⚠ Ohne Netz passiert nichts Schlimmes: Liegt eine alte Ablage da, wird sie
benutzt. Liegt gar keine da, bleibt der Lagerort ein freiwilliges Feld ohne
Prüfung — lieber ohne Vorschlagsliste weiterarbeiten als gar nichts eintragen
können.
"""
from . import uex

QUELLE = 'https://api.uexcorp.uk/2.0/terminals'
CACHE = 'orte.json'
FORMAT = 1

# Eine Woche. Stationen kommen mit einem Patch, nicht über Nacht.
HALTBAR = uex.WOCHE

# Aus diesen Feldern wird der Ortsname gezogen — in dieser Reihenfolge.
FELDER = ('space_station_name', 'city_name', 'outpost_name')

# Abruf und Ablage liegen im gemeinsamen Unterbau — siehe `scbp/uex.py`.
_ablage = uex.Ablage(CACHE, format_nr=FORMAT, haltbar=HALTBAR)


def laden():
    """Der abgelegte Stand — aus dem Speicher, wenn die Datei unverändert ist."""
    return _ablage.laden()


def alle():
    """Alle bekannten Lagerorte, alphabetisch — oder eine leere Liste."""
    return (laden() or {}).get('orte') or []


def alter():
    """Wie alt die Ablage ist, in Sekunden — oder None."""
    return _ablage.alter()


def aktualisieren():
    """Die Ortsliste holen, wenn sie fehlt oder älter als eine Woche ist."""
    if not _ablage.veraltet():
        return True
    roh = uex.holen(QUELLE, 'orte')
    if not roh:
        return False
    namen = set()
    for x in roh:
        for feld in FELDER:
            n = (x.get(feld) or '').strip()
            if n:
                namen.add(n)
    if not namen:
        return False
    return _ablage.sichern({'orte': sorted(namen, key=str.lower)})


def kennt(name):
    """Gibt es diesen Ort? Ohne Ortsliste gilt **alles** als gültig.

    ⚠ Das ist Absicht: Liegt keine Liste vor (erster Start ohne Netz), darf das
    Feld nicht blockieren. Der Lagerort ist freiwillig — ohne ihn ist das Lager
    weiter benutzbar, mit einer Sperre ohne Liste wäre es das nicht.
    """
    if not (name or '').strip():
        return True                      # leer ist erlaubt, das Feld ist freiwillig
    liste = alle()
    if not liste:
        return True
    gesucht = (name or '').strip().lower()
    return any(o.lower() == gesucht for o in liste)


def offizieller_name(eingabe):
    """Die verbindliche Schreibweise — oder `None`, wenn unbekannt."""
    gesucht = (eingabe or '').strip().lower()
    if not gesucht:
        return ''
    for o in alle():
        if o.lower() == gesucht:
            return o
    return None


def aehnliche(name, hoechstens=4):
    """Vorschläge zu einer Eingabe.

    ⚠ **Teiltext, nicht nur Wortanfang.** Wer „pyro" tippt, meint
    `Pyro Gateway (Stanton)`; wer „checkmate" tippt, `Checkmate Station`. Ein
    Vorschlag, der nur auf den Anfang schaut, findet beide nicht.
    """
    import difflib
    text = (name or '').strip().lower()
    if not text:
        return []
    liste = alle()
    treffer = [o for o in liste if text in o.lower()]
    if treffer:
        return treffer[:hoechstens]
    return difflib.get_close_matches(text, liste, n=hoechstens, cutoff=0.6)
