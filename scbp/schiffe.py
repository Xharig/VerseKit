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
"""
from . import uex
from .katalog import AUS

QUELLE_SCHIFFE = 'https://api.uexcorp.uk/2.0/vehicles'
QUELLE_KAUF = 'https://api.uexcorp.uk/2.0/vehicles_purchases_prices'
QUELLE_MIETE = 'https://api.uexcorp.uk/2.0/vehicles_rentals_prices'
CACHE = 'schiffe.json'
# 2 seit v3.15.0 (die Werft kam dazu), 3 seit dem Entschlüsseln der
# HTML-Zeichen — sonst bliebe „Grey&apos;s Market" in der alten Ablage stehen.
# 4 seit v3.19.0: `konzept` kam dazu, 5: `anbau` (siehe `aktualisieren`).
FORMAT = 5

# Eine Woche — wie bei den Lagerorten. Schiffe kommen mit einem Patch.
HALTBAR = 30 * uex.TAG

# ⚠ An den Patch gebunden: Schiffe, ihre Frachträume und ihre Kaufpreise
# ändern sich mit einer neuen Spielversion, nicht im Wochenrhythmus.
_ablage = uex.Ablage(CACHE, format_nr=FORMAT, haltbar=HALTBAR,
                     patch_bindet=True)


def laden():
    return _ablage.laden() or {}


def alter():
    return _ablage.alter()


def alle():
    """Alle Schiffe **mit Frachtraum**, alphabetisch.

    ⚠ Ohne Laderaum ist ein Schiff für diese Frage uninteressant — wer eine
    Handelsroute plant, sucht keinen Jäger. 139 von 280 bleiben übrig.
    """
    schiffe = laden().get('schiffe') or {}
    return sorted((s.get('name') or '' for s in schiffe.values()
                   if (s.get('scu') or 0) > 0), key=str.lower)


def katalog():
    """Jedes Schiff, das irgendwo **zu kaufen oder zu mieten** ist.

    Je Eintrag `name`, `werft` und `scu`. Gedacht für den Laden-Reiter: Dort
    geht es um die Frage „wo bekomme ich das", und ein Schiff, das nirgends
    angeboten wird, hat darauf keine Antwort — genau wie ein Teil ohne
    Ladenpreis.

    ⚠ **Nicht dasselbe wie `alle()`.** Das liefert die Schiffe mit Laderaum
    für den Routenplaner; hier zählt der Verkaufstresen, nicht der Frachtraum.
    """
    daten = laden()
    schiffe = daten.get('schiffe') or {}
    zu_haben = set(daten.get('kauf') or {}) | set(daten.get('miete') or {})
    raus = []
    for kennung, s in schiffe.items():
        if kennung not in zu_haben or not (s.get('name') or ''):
            continue
        raus.append({'name': s['name'], 'werft': s.get('werft') or '',
                     'scu': int(s.get('scu') or 0)})
    raus.sort(key=lambda x: x['name'].lower())
    return raus


def mit_frachtraum():
    """Alle Schiffe **mit Laderaum**, samt Werft und SCU.

    ⚠ **Nicht auf das Verkaufsangebot beschränkt** — anders als `katalog()`.
    Wer eine Route plant, fliegt sein eigenes Schiff; ob es gerade irgendwo im
    Regal steht, ist dafür belanglos.
    """
    raus = []
    for s in (laden().get('schiffe') or {}).values():
        laderaum = int(s.get('scu') or 0)
        if laderaum <= 0 or not (s.get('name') or ''):
            continue
        raus.append({'name': s['name'], 'werft': s.get('werft') or '',
                     'scu': laderaum})
    raus.sort(key=lambda x: x['name'].lower())
    return raus


def namen_alle():
    """**Alle** Schiffe und Fahrzeuge, alphabetisch — auch ohne Frachtraum.

    ⚠⚠ **Nicht mit `alle()` verwechseln.** Das dort filtert auf Laderaum und
    liefert 134 von 280 — richtig für den Routenplaner, falsch überall sonst.
    Im Hangar war es ein Fehler: Wer einen Arrow, einen Gladius oder ein
    A.T.L.S. IKTI besitzt, konnte ihn **gar nicht eintragen**, weil kein Jäger
    und kein Exo-Anzug Laderaum hat. Gemeldet am 06.09.2026.
    """
    schiffe = laden().get('schiffe') or {}
    return sorted((s.get('name') or '' for s in schiffe.values()
                   if s.get('name') and not s.get('anbau')), key=str.lower)


def _finden(name):
    """Der UEX-Eintrag zu einem Schiffsnamen — oder `None`.

    ⚠⚠ **UEX führt den Hersteller im Namen mit** (`name_full`): „RSI Galaxy",
    „Drake Ironclad Assault". Der Pledge-Export schreibt dagegen nur „Galaxy".
    Ein Vergleich auf Gleichheit findet deshalb **nichts** — und genau daran
    ist die Konzept-Erkennung beim ersten Anlauf gescheitert.

    Deshalb zwei Stufen: erst gleich, dann als **Ende** des UEX-Namens. Der
    zweite Weg zählt nur bei einem **einzigen** Treffer; „Galaxy" darf nicht
    versehentlich das „Galaxy Cargo Module" erwischen.
    """
    gesucht = (name or '').strip().lower()
    if not gesucht:
        return None
    alle_e = list((laden().get('schiffe') or {}).values())
    for s in alle_e:
        if (s.get('name') or '').lower() == gesucht:
            return s
    endet = [s for s in alle_e
             if (s.get('name') or '').lower().endswith(' ' + gesucht)]
    return endet[0] if len(endet) == 1 else None


def kennt(name):
    """Führt UEX ein Schiff dieses Namens?"""
    return _finden(name) is not None


def hersteller(name):
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
    eintrag = _finden(name)
    if not eintrag:
        return ''
    voll = (eintrag.get('name') or '').strip()
    gesucht = (name or '').strip()
    if voll.lower().endswith(' ' + gesucht.lower()):
        return voll[:len(voll) - len(gesucht)].strip()
    return ''


def ist_konzept(name):
    """Ist das Schiff laut UEX ein Konzept — also noch nicht im Spiel?

    ⚠ **Fremdangabe, keine eigene Feststellung.** UEX pflegt das Feld von
    Hand; steht dort nichts, kommt `False` zurück. Die Anzeige darf daraus
    also „Konzept" folgern, aber niemals aus dem Fehlen von Steckplatz-Daten.
    """
    eintrag = _finden(name)
    return bool(eintrag and eintrag.get('konzept'))


def scu(name):
    """Der Frachtraum eines Schiffs in SCU — oder `0`."""
    for s in (laden().get('schiffe') or {}).values():
        if (s.get('name') or '').lower() == (name or '').strip().lower():
            return int(s.get('scu') or 0)
    return 0


def _stellen(name, feld):
    schiffe = laden().get('schiffe') or {}
    kennung = ''
    for schluessel, s in schiffe.items():
        if (s.get('name') or '').lower() == (name or '').strip().lower():
            kennung = schluessel
            break
    if not kennung:
        return []
    liste = (laden().get(feld) or {}).get(kennung) or []
    return sorted(liste, key=lambda z: z['preis'])


def kaufen(name):
    """Wo dieses Schiff zu kaufen ist — billigster zuerst."""
    return _stellen(name, 'kauf')


def mieten(name):
    """Wo dieses Schiff zu mieten ist — billigster zuerst."""
    return _stellen(name, 'miete')


def _preise_einsammeln(roh, preisfeld):
    """Aus einer Preisliste `{schiff_id: [Stellen]}` machen."""
    raus = {}
    for x in roh or []:
        kennung = str(x.get('id_vehicle') or '')
        preis = float(x.get(preisfeld) or 0)
        # ⚠ `0` heisst „hier nicht zu haben", nicht „geschenkt" — dieselbe
        # Falle wie bei den Waren- und Ladenpreisen.
        if not kennung or preis <= 0:
            continue
        raus.setdefault(kennung, []).append({
            'stelle': (x.get('terminal_name') or '').strip(),
            'ort': (x.get('space_station_name') or x.get('city_name')
                    or x.get('outpost_name') or x.get('planet_name')
                    or '').strip(),
            'system': (x.get('star_system_name') or '').strip(),
            'preis': preis,
        })
    return raus


def aktualisieren():
    """Die drei Listen holen, wenn sie fehlen oder älter als eine Woche sind."""
    if AUS:
        return False
    if not _ablage.veraltet():
        return True
    roh = uex.holen(QUELLE_SCHIFFE, 'schiffe')
    if not roh:
        return False
    schiffe = {}
    for x in roh:
        kennung = str(x.get('id') or '')
        name = (x.get('name_full') or x.get('name') or '').strip()
        if kennung and name:
            # ⚠ Der Hersteller kommt seit v3.15.0 mit — im Laden-Reiter sind
            # die Werften die Warengruppen, nach denen jemand sucht („zeig mir
            # die Drakes"). Ohne ihn wären 280 Schiffe eine Namensliste.
            schiffe[kennung] = {'name': name, 'scu': int(x.get('scu') or 0),
                                'werft': (x.get('company_name') or '').strip()}
            # ⭐⭐ **`is_concept` beantwortet eine Frage, die wir sonst raten
            # müssten:** Gibt es das Schiff im Spiel schon? Der Hangar zeigt zu
            # jedem Schiff ohne Steckplatz-Daten, woran das liegt — und ohne
            # dieses Feld stand dort „noch nicht im Spiel" auch bei Schiffen,
            # die längst fliegen (gemeldet 06.09.2026: Ironclad Assault,
            # Super Hornet Mk II). Eine Behauptung, die man nicht belegen kann,
            # gehört nicht ins Werkzeug.
            if x.get('is_concept'):
                schiffe[kennung]['konzept'] = 1
            # ⚠ Anbauteile sind keine Schiffe: „Retaliator Cargo Module",
            # „Endeavor Medical Bay Pod". In einer Schiffsliste stiften sie nur
            # Verwirrung — und bei der Zuordnung landeten sie beim Hauptschiff,
            # wodurch die Bergung für ein Modul die Ausstattung des ganzen
            # Retaliators zeigte.
            if x.get('is_addon'):
                schiffe[kennung]['anbau'] = 1

    # ⚠ Die Preislisten dürfen fehlschlagen, ohne dass alles scheitert: Ohne
    # sie kennt man wenigstens noch die Frachträume, und genau die braucht der
    # Routen-Reiter. Lieber die halbe Auskunft als gar keine.
    kauf = _preise_einsammeln(uex.holen(QUELLE_KAUF, 'schiffe.kauf'),
                              'price_buy')
    miete = _preise_einsammeln(uex.holen(QUELLE_MIETE, 'schiffe.miete'),
                               'price_rent')
    return _ablage.sichern({'schiffe': schiffe, 'kauf': kauf,
                            'miete': miete}, kompakt=True)
