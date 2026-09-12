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
from .catalog import OFF

SOURCE = 'https://api.uexcorp.uk/2.0/terminals'
CACHE = 'orte.json'
FORMAT = 1

# Eine Woche. Stationen kommen mit einem Patch, nicht über Nacht.
SHELF_LIFE = 30 * uex.DAY

# Aus diesen Feldern wird der Ortsname gezogen — in dieser Reihenfolge.
FIELDS = ('space_station_name', 'city_name', 'outpost_name')

# Abruf und Ablage liegen im gemeinsamen Unterbau — siehe `scbp/uex.py`.
# ⚠ An den Patch gebunden: Stationen und Aussenposten kommen mit einer neuen
# Spielversion dazu, nicht zwischendurch.
_store = uex.Store(CACHE, format_no=FORMAT, shelf_life=SHELF_LIFE,
                   patch_bound=True)


def load():
    """Der abgelegte Stand — aus dem Speicher, wenn die Datei unverändert ist."""
    return _store.load()


def all_places():
    """Alle bekannten Lagerorte, alphabetisch — oder eine leere Liste."""
    return (load() or {}).get('orte') or []


def age():
    """Wie alt die Ablage ist, in Sekunden — oder None."""
    return _store.age()


def update():
    """Die Ortsliste holen, wenn sie fehlt oder älter als eine Woche ist."""
    # ⚠ Wie in `prices.py`: Die Abfrage bleibt hier, damit der Rückgabewert
    # bei abgeschaltetem Netz derselbe ist wie vor dem Umbau.
    if OFF:
        return False
    if not _store.stale():
        return True
    raw = uex.fetch(SOURCE, 'places')
    if not raw:
        return False
    names = set()
    for x in raw:
        for field in FIELDS:
            n = (x.get(field) or '').strip()
            if n:
                names.add(n)
    if not names:
        return False
    return _store.save({'orte': sorted(names, key=str.lower)})


def knows(name):
    """Gibt es diesen Ort? Ohne Ortsliste gilt **alles** als gültig.

    ⚠ Das ist Absicht: Liegt keine Liste vor (erster Start ohne Netz), darf das
    Feld nicht blockieren. Der Lagerort ist freiwillig — ohne ihn ist das Lager
    weiter benutzbar, mit einer Sperre ohne Liste wäre es das nicht.
    """
    if not (name or '').strip():
        return True                      # leer ist erlaubt, das Feld ist freiwillig
    items = all_places()
    if not items:
        return True
    wanted = (name or '').strip().lower()
    return any(o.lower() == wanted for o in items)


def official_name(given):
    """Die verbindliche Schreibweise — oder `None`, wenn unbekannt."""
    wanted = (given or '').strip().lower()
    if not wanted:
        return ''
    for o in all_places():
        if o.lower() == wanted:
            return o
    return None


def similar(name, most=4):
    """Vorschläge zu einer Eingabe.

    ⚠ **Teiltext, nicht nur Wortanfang.** Wer „pyro" tippt, meint
    `Pyro Gateway (Stanton)`; wer „checkmate" tippt, `Checkmate Station`. Ein
    Vorschlag, der nur auf den Anfang schaut, findet beide nicht.
    """
    import difflib
    text = (name or '').strip().lower()
    if not text:
        return []
    items = all_places()
    hits = [o for o in items if text in o.lower()]
    if hits:
        return hits[:most]
    return difflib.get_close_matches(text, items, n=most, cutoff=0.6)
