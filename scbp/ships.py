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
Schiffe: wieviel passt rein, wo gibt es eines, was kostet es.

## Wozu das im Bauplan-Werkzeug steht

Es hängt direkt an den **Routen**: Dort gibt man seinen Frachtraum von Hand
ein. Wer sein Schiff kennt, soll es stattdessen auswählen können — „Freelancer
MAX" statt „120". Und die Anschlussfrage ist immer dieselbe: *Womit fahre ich
das, und wo bekomme ich es her?*

## Drei Listen, die zusammengehören

| Endpunkt | was daraus wird | Umfang |
|---|---|---|
| `vehicles` | Name und **Frachtraum** je Schiff | 280, davon **139 mit Laderaum** |
| `vehicles_purchases_prices` | wo zu kaufen, für wieviel | 282 |
| `vehicles_rentals_prices` | wo zu mieten, für wieviel | 336 |

⚠ **Die Preiszeilen tragen keinen Schiffsnamen**, nur `id_vehicle`. Verbunden
wird über diese Kennung — nicht über Namen. Dieselbe Regel wie überall hier.

## ⚠ Warum hier auf Vorrat geholt wird — anders als bei Läden und Routen

Alle drei Listen sind **vollständig unter dem 500er-Deckel** (282, 336, 280).
Ein Abruf liefert also das Ganze, nicht ein Bruchstück. Bei den Ladenpreisen
und den Routen war das umgekehrt — dort wäre „alles holen" ein Rundumschlag
über hunderte Abrufe gewesen.

**Die Regel dahinter:** So eng zuschneiden wie nötig, nicht so eng wie möglich.
Drei Abrufe für eine vollständige Liste sind sparsamer als hundert kleine.

⚠ Und **selten**: Schiffe kommen mit einem Patch dazu, nicht über Nacht —
dieselbe Wochenfrist wie bei den Lagerorten.

⚠ Bis zum 12.09.2026 hieß dieses Modul `schiffe` (Sprachumstellung P4,
Stufe 2). **Nur Bezeichner sind umbenannt, keine Zeichenketten.** Bewusst
gleich geblieben: der Ablage-Name `schiffe.json` und die Schlüssel darin
(`schiffe`, `kauf`, `miete`, `konzept`, `anbau`, `name`, `werft`, `scu`,
`stelle`, `ort`, `system`, `preis`) — sonst gilt jede vorhandene Ablage als
fremd. Ebenso die Kennungen, unter denen `uex.fetch()` meldet
(`'schiffe'`, `'schiffe.kauf'`, `'schiffe.miete'`).
"""
from . import uex
from .katalog import AUS

SOURCE_SHIPS = 'https://api.uexcorp.uk/2.0/vehicles'
SOURCE_BUY = 'https://api.uexcorp.uk/2.0/vehicles_purchases_prices'
SOURCE_RENT = 'https://api.uexcorp.uk/2.0/vehicles_rentals_prices'
CACHE = 'schiffe.json'
# 2 seit v3.15.0 (die Werft kam dazu), 3 seit dem Entschlüsseln der
# HTML-Zeichen — sonst bliebe „Grey&apos;s Market" in der alten Ablage stehen.
# 4 seit v3.19.0: `konzept` kam dazu, 5: `anbau` (siehe `update`).
FORMAT = 5

# Eine Woche — wie bei den Lagerorten. Schiffe kommen mit einem Patch.
SHELF_LIFE = 30 * uex.DAY

# ⚠ An den Patch gebunden: Schiffe, ihre Frachträume und ihre Kaufpreise
# ändern sich mit einer neuen Spielversion, nicht im Wochenrhythmus.
_store = uex.Store(CACHE, format_no=FORMAT, shelf_life=SHELF_LIFE,
                    patch_bound=True)


def load():
    return _store.load() or {}


def age():
    return _store.age()


def names_with_cargo():
    """Alle Schiffe **mit Frachtraum**, alphabetisch.

    ⚠ Ohne Laderaum ist ein Schiff für diese Frage uninteressant — wer eine
    Handelsroute plant, sucht keinen Jäger. 139 von 280 bleiben übrig.
    """
    ships = load().get('schiffe') or {}
    return sorted((s.get('name') or '' for s in ships.values()
                   if (s.get('scu') or 0) > 0), key=str.lower)


def catalog():
    """Jedes Schiff, das irgendwo **zu kaufen oder zu mieten** ist.

    Je Eintrag `name`, `werft` und `scu`. Gedacht für den Laden-Reiter: Dort
    geht es um die Frage „wo bekomme ich das", und ein Schiff, das nirgends
    angeboten wird, hat darauf keine Antwort — genau wie ein Teil ohne
    Ladenpreis.

    ⚠ **Nicht dasselbe wie `names_with_cargo()`.** Das liefert die Schiffe mit
    Laderaum für den Routenplaner; hier zählt der Verkaufstresen, nicht der
    Frachtraum.
    """
    data = load()
    ships = data.get('schiffe') or {}
    available = set(data.get('kauf') or {}) | set(data.get('miete') or {})
    result = []
    for ident, s in ships.items():
        if ident not in available or not (s.get('name') or ''):
            continue
        result.append({'name': s['name'], 'werft': s.get('werft') or '',
                       'scu': int(s.get('scu') or 0)})
    result.sort(key=lambda x: x['name'].lower())
    return result


def with_cargo():
    """Alle Schiffe **mit Laderaum**, samt Werft und SCU.

    ⚠ **Nicht auf das Verkaufsangebot beschränkt** — anders als `catalog()`.
    Wer eine Route plant, fliegt sein eigenes Schiff; ob es gerade irgendwo im
    Regal steht, ist dafür belanglos.
    """
    result = []
    for s in (load().get('schiffe') or {}).values():
        cargo = int(s.get('scu') or 0)
        if cargo <= 0 or not (s.get('name') or ''):
            continue
        result.append({'name': s['name'], 'werft': s.get('werft') or '',
                       'scu': cargo})
    result.sort(key=lambda x: x['name'].lower())
    return result


def all_names():
    """**Alle** Schiffe und Fahrzeuge, alphabetisch — auch ohne Frachtraum.

    ⚠⚠ **Nicht mit `names_with_cargo()` verwechseln.** Das dort filtert auf
    Laderaum und liefert 134 von 280 — richtig für den Routenplaner, falsch
    überall sonst. Im Hangar war es ein Fehler: Wer einen Arrow, einen Gladius
    oder ein A.T.L.S. IKTI besitzt, konnte ihn **gar nicht eintragen**, weil
    kein Jäger und kein Exo-Anzug Laderaum hat. Gemeldet am 06.09.2026.
    """
    ships = load().get('schiffe') or {}
    return sorted((s.get('name') or '' for s in ships.values()
                   if s.get('name') and not s.get('anbau')), key=str.lower)


def _find(name):
    """Der UEX-Eintrag zu einem Schiffsnamen — oder `None`.

    ⚠⚠ **UEX führt den Hersteller im Namen mit** (`name_full`): „RSI Galaxy",
    „Drake Ironclad Assault". Der Pledge-Export schreibt dagegen nur „Galaxy".
    Ein Vergleich auf Gleichheit findet deshalb **nichts** — und genau daran
    ist die Konzept-Erkennung beim ersten Anlauf gescheitert.

    Deshalb zwei Stufen: erst gleich, dann als **Ende** des UEX-Namens. Der
    zweite Weg zählt nur bei einem **einzigen** Treffer; „Galaxy" darf nicht
    versehentlich das „Galaxy Cargo Module" erwischen.
    """
    wanted = (name or '').strip().lower()
    if not wanted:
        return None
    entries = list((load().get('schiffe') or {}).values())
    for s in entries:
        if (s.get('name') or '').lower() == wanted:
            return s
    ends = [s for s in entries
            if (s.get('name') or '').lower().endswith(' ' + wanted)]
    return ends[0] if len(ends) == 1 else None


def knows(name):
    """Führt UEX ein Schiff dieses Namens?"""
    return _find(name) is not None


def manufacturer(name):
    """Der Hersteller zu einem Schiffsnamen — oder `''`.

    ⚠ **Warum das nötig ist.** UEX führt den Hersteller im Namen mit („MISC
    Prospector"), der Spieler tippt aber nur „Prospector". Ohne Hersteller
    findet `erkul` einen Teil der Schiffe nicht: Gemessen am 06.09.2026 fand es
    Vulture und Corsair auch ohne, die **Prospector aber nicht**. Auf der
    Wunschliste gibt es keinen Export, aus dem der Hersteller käme — also wird
    er hier aus dem UEX-Namen geholt und beim Eintragen mitgespeichert.

    Geliefert wird der Teil **vor** dem gesuchten Namen, nicht bloß das erste
    Wort: Bei „Mirai Fury LX" heißt der Hersteller „Mirai", bei „Aegis Dynamics
    Sabre" die vollen zwei Wörter.
    """
    entry = _find(name)
    if not entry:
        return ''
    full = (entry.get('name') or '').strip()
    wanted = (name or '').strip()
    if full.lower().endswith(' ' + wanted.lower()):
        return full[:len(full) - len(wanted)].strip()
    return ''


def is_concept(name):
    """Ist das Schiff laut UEX ein Konzept — also noch nicht im Spiel?

    ⚠ **Fremdangabe, keine eigene Feststellung.** UEX pflegt das Feld von
    Hand; steht dort nichts, kommt `False` zurück. Die Anzeige darf daraus
    also „Konzept" folgern, aber niemals aus dem Fehlen von Steckplatz-Daten.
    """
    entry = _find(name)
    return bool(entry and entry.get('konzept'))


def scu(name):
    """Der Frachtraum eines Schiffs in SCU — oder `0`."""
    for s in (load().get('schiffe') or {}).values():
        if (s.get('name') or '').lower() == (name or '').strip().lower():
            return int(s.get('scu') or 0)
    return 0


def _places(name, field):
    ships = load().get('schiffe') or {}
    ident = ''
    for key, s in ships.items():
        if (s.get('name') or '').lower() == (name or '').strip().lower():
            ident = key
            break
    if not ident:
        return []
    items = (load().get(field) or {}).get(ident) or []
    return sorted(items, key=lambda z: z['preis'])


def buy_at(name):
    """Wo dieses Schiff zu kaufen ist — billigster zuerst."""
    return _places(name, 'kauf')


def rent_at(name):
    """Wo dieses Schiff zu mieten ist — billigster zuerst."""
    return _places(name, 'miete')


def _collect_prices(raw, price_field):
    """Aus einer Preisliste `{schiff_id: [Stellen]}` machen."""
    result = {}
    for x in raw or []:
        ident = str(x.get('id_vehicle') or '')
        price = float(x.get(price_field) or 0)
        # ⚠ `0` heisst „hier nicht zu haben", nicht „geschenkt" — dieselbe
        # Falle wie bei den Waren- und Ladenpreisen.
        if not ident or price <= 0:
            continue
        result.setdefault(ident, []).append({
            'stelle': (x.get('terminal_name') or '').strip(),
            'ort': (x.get('space_station_name') or x.get('city_name')
                    or x.get('outpost_name') or x.get('planet_name')
                    or '').strip(),
            'system': (x.get('star_system_name') or '').strip(),
            'preis': price,
        })
    return result


def update():
    """Die drei Listen holen, wenn sie fehlen oder älter als eine Woche sind."""
    if AUS:
        return False
    if not _store.stale():
        return True
    raw = uex.fetch(SOURCE_SHIPS, 'schiffe')
    if not raw:
        return False
    ships = {}
    for x in raw:
        ident = str(x.get('id') or '')
        name = (x.get('name_full') or x.get('name') or '').strip()
        if ident and name:
            # ⚠ Der Hersteller kommt seit v3.15.0 mit — im Laden-Reiter sind
            # die Werften die Warengruppen, nach denen jemand sucht („zeig mir
            # die Drakes"). Ohne ihn wären 280 Schiffe eine Namensliste.
            ships[ident] = {'name': name, 'scu': int(x.get('scu') or 0),
                            'werft': (x.get('company_name') or '').strip()}
            # ⭐⭐ **`is_concept` beantwortet eine Frage, die wir sonst raten
            # müssten:** Gibt es das Schiff im Spiel schon? Der Hangar zeigt zu
            # jedem Schiff ohne Steckplatz-Daten, woran das liegt — und ohne
            # dieses Feld stand dort „noch nicht im Spiel" auch bei Schiffen,
            # die längst fliegen (gemeldet 06.09.2026: Ironclad Assault,
            # Super Hornet Mk II). Eine Behauptung, die man nicht belegen kann,
            # gehört nicht ins Werkzeug.
            if x.get('is_concept'):
                ships[ident]['konzept'] = 1
            # ⚠ Anbauteile sind keine Schiffe: „Retaliator Cargo Module",
            # „Endeavor Medical Bay Pod". In einer Schiffsliste stiften sie nur
            # Verwirrung — und bei der Zuordnung landeten sie beim Hauptschiff,
            # wodurch die Bergung für ein Modul die Ausstattung des ganzen
            # Retaliators zeigte.
            if x.get('is_addon'):
                ships[ident]['anbau'] = 1

    # ⚠ Die Preislisten dürfen fehlschlagen, ohne dass alles scheitert: Ohne
    # sie kennt man wenigstens noch die Frachträume, und genau die braucht der
    # Routen-Reiter. Lieber die halbe Auskunft als gar keine.
    buy = _collect_prices(uex.fetch(SOURCE_BUY, 'schiffe.kauf'),
                          'price_buy')
    rent = _collect_prices(uex.fetch(SOURCE_RENT, 'schiffe.miete'),
                           'price_rent')
    return _store.save({'schiffe': ships, 'kauf': buy,
                           'miete': rent}, compact=True)
