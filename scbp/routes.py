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
Handelsrouten: Wo kaufe ich billig, wo verkaufe ich teuer — und was bleibt übrig?

## Der Wunsch dahinter

Gewünscht von **YoshimitsuDE** (04.09.2026): „Tradingtool, Preis Einkauf und
Verkauf, eventuell mit guten Routen und Profitmaximierung." Genauer: Man gibt
seinen **Frachtraum** an und bekommt eine Route über zwei, drei oder mehr
Stationen zurück, dazu den Gewinn — wahlweise nach **bestem Gewinn** oder nach
**kurzer Strecke**.

## ⭐ UEX rechnet die einzelnen Fahrten schon selbst

Der Endpunkt `commodities_routes` liefert fertige Fahrten. Eine Zeile ist
„kauf X hier, verkauf es dort" und bringt alles mit, was die Frage braucht::

    Kaufterminal · Verkaufsterminal · Ware
    Einkaufspreis · Verkaufspreis · Spanne · Rendite
    Entfernung in Gm        <- damit ist „kurze Route" beantwortbar
    verfügbare Menge · gesuchte Menge · nötiges Kapital

Wir rechnen daraus nur noch, **was in den eigenen Laderaum passt** und wie sich
Fahrten aneinanderhängen lassen.

## ⚠⚠ Der Zuschnitt: je Startort, nicht auf Vorrat

Gemessen am 04.09.2026:

| Zuschnitt | Ergebnis |
|---|---|
| `id_star_system_origin` | **HTTP 400** — gibt es nicht |
| `id_planet_origin` | bei 7 von 10 Planeten **exakt 500 Zeilen** → abgeschnitten |
| **`id_terminal_origin`** | **69 Zeilen**, weit unter dem Deckel |

Der Spieler sagt ohnehin, wo er gerade steht — also ein Abruf, eine Antwort.
Ein vollständiges Abbild bräuchte rund 250 Abrufe und wäre gegenüber UEX
unhöflich.

## ⚠ Ketten kosten weitere Abrufe — deshalb gedeckelt

Für „und wo fahre ich danach hin?" braucht es die Fahrten **ab dem Zielort**.
Das ist je Kandidat ein weiterer Abruf. Deshalb werden nur die
`CHAIN_CANDIDATES` besten Ziele weiterverfolgt: höchstens ein paar Abrufe
statt siebzig.

## ⚠⚠ Und eine Warnung, die in jede Anzeige gehört

`scu_origin` (wieviel dort liegt) ist **von Spielern gemeldet** und altert. Die
Spitzenreiter sind fast immer kleine Mengen mit riesiger Spanne — steht die
Ware nicht mehr da, ist die ganze Fahrt wertlos. Das Alter der Daten gehört
deshalb an jede Route, nicht in eine Fußnote.
"""
import time

from . import uex
from .catalog import OFF

SOURCE = 'https://api.uexcorp.uk/2.0/commodities_routes?id_terminal_origin=%s'
CACHE = 'routen.json'
FORMAT = 1

# Sechs Stunden. Kürzer als bei den Ladenpreisen: Eine Handelsspanne lebt von
# Beständen, und die ändern sich im Lauf eines Abends.
SHELF_LIFE = 6 * 60 * 60

# Wieviele Startorte die Ablage behält.
#
# ⚠⚠ **Muss über der Zahl der Handelsposten liegen (184).** Stand hier vorher
# auf 25 — mit dem Rundumlauf aus `fetch_all()` hätte sich die Ablage dabei
# selbst leergeräumt: Ab dem 26. Posten wäre bei jedem weiteren der älteste
# hinausgeflogen, und am Ende stünden 25 zufällige statt aller 184 da. Der
# Fehler wäre nicht aufgefallen — die Liste hätte einfach weniger gezeigt.
#
# 200 deckt alle Posten ab und bleibt bei rund 2 MB.
MAX_STARTS = 200

# ⚠ Wieviele Ziele für eine Kette weiterverfolgt werden. Jeder kostet einen
# eigenen Abruf — bei 69 Fahrten je Startort wären es sonst 69.
CHAIN_CANDIDATES = 5

# ⚠ Wie viele Fahrten auf der **Rückfahrt** einer Rundreise betrachtet werden.
# Deutlich mehr als `CHAIN_CANDIDATES`, weil die Fahrt zurück zum Startort
# selten zu den gewinnstärksten gehört — sie muss aber gefunden werden, sonst
# gibt es gar keine Rundreise. Kostet keinen Abruf: Die Fahrten des Ortes
# liegen bereits in der Ablage.
RETURN_CANDIDATES = 400

# ⚠ Wieviele Fahrten eine Route höchstens hat. Jede Stufe kostet Abrufe, und
# eine Route über sechs Stationen plant ohnehin niemand: Bis man beim letzten
# Stopp ist, sind die Preise vom Anfang alt.
MAX_STOPS = 4

_store = uex.Store(CACHE, format_no=FORMAT, shelf_life=SHELF_LIFE)


def _all():
    return (_store.load() or {}).get('starts') or {}


def age(start):
    """Wie alt die Fahrten ab diesem Ort sind — oder `None`."""
    entry = _all().get(str(start))
    if not entry:
        return None
    try:
        return time.time() - float(entry.get('geholt') or 0)
    except (TypeError, ValueError):
        return None


def trips(start):
    """Alle bekannten Fahrten ab diesem Terminal.

    `None` heißt „noch nicht nachgesehen", `[]` heißt „von hier lohnt nichts".
    """
    entry = _all().get(str(start))
    if entry is None:
        return None
    return entry.get('fahrten') or []


def fetch(start, force=False):
    """Die Fahrten ab einem Terminal nachschlagen."""
    if OFF or not start:
        return False
    a = age(start)
    if not force and a is not None and a < SHELF_LIFE:
        return True
    raw = uex.fetch(SOURCE % start, 'routes')
    if raw is None:
        return False

    items = []
    for x in raw:
        buy = float(x.get('price_origin') or 0)
        sell = float(x.get('price_destination') or 0)
        # ⚠ Drei Gründe, eine Zeile wegzulassen — und alle drei sind nötig:
        # ohne Einkaufspreis kann man nicht kaufen, ohne Aufschlag lohnt es
        # nicht, und ohne Vorrat steht dort nichts im Regal.
        if buy <= 0 or sell <= buy or not (x.get('scu_origin') or 0):
            continue
        items.append({
            'ware': (x.get('commodity_name') or '').strip(),
            'ziel': str(x.get('id_terminal_destination') or ''),
            'zielname': (x.get('destination_terminal_name') or '').strip(),
            'zielort': (x.get('destination_planet_name')
                        or x.get('destination_orbit_name') or '').strip(),
            'zielsystem': (x.get('destination_star_system_name') or '').strip(),
            'ek': buy,
            'vk': sell,
            'gewinn_scu': sell - buy,
            # Entfernung in Gm. `0`, wenn UEX keine kennt — dann wird bei
            # „kurze Route" nichts behauptet.
            'strecke': float(x.get('distance') or 0),
            'vorrat': int(x.get('scu_origin') or 0),
            'bedarf': int(x.get('scu_destination') or 0),
        })
    items.sort(key=lambda f: -f['gewinn_scu'])

    starts = dict(_all())
    starts[str(start)] = {'geholt': time.time(), 'fahrten': items}
    if len(starts) > MAX_STARTS:
        by_age = sorted(starts.items(),
                        key=lambda p: p[1].get('geholt') or 0)
        for key, _value in by_age[:len(starts) - MAX_STARTS]:
            starts.pop(key, None)
    return _store.save({'starts': starts}, compact=True)


def amount_and_profit(trip, free_scu, money):
    """Wieviel passt wirklich — und was bringt es? `(menge, gewinn)`.

    ⚠ **Drei Grenzen, nicht eine.** Der Laderaum ist die offensichtliche; die
    beiden anderen werden gern vergessen und machen jede Rechnung falsch:

    | Grenze | warum |
    |---|---|
    | Frachtraum | mehr passt nicht ins Schiff |
    | **Vorrat am Startort** | mehr steht dort nicht im Regal |
    | **Geld** | mehr kann man nicht bezahlen |

    Wer nur den Laderaum rechnet, verspricht bei 45 verfügbaren SCU den Gewinn
    für 96 — mehr als das Doppelte.
    """
    amount = min(int(free_scu or 0), int(trip.get('vorrat') or 0))
    if trip.get('bedarf'):
        amount = min(amount, int(trip['bedarf']))
    if trip.get('ek'):
        amount = min(amount, int((money or 0) // trip['ek']))
    amount = max(0, amount)
    return amount, amount * trip['gewinn_scu']


def what_limits(trip, free_scu, money):
    """Woran hängt die Menge — Frachtraum, Vorrat, Bedarf oder Geld?

    ⭐ **Die nützlichste Auskunft der ganzen Zeile.** „69 von 120 SCU" sagt
    noch nicht, ob ein größeres Schiff hilft. Steht dort „begrenzt durch Geld",
    weiß man: mehr Kapital, mehr Gewinn — ein größerer Frachter brächte nichts.
    Am 04.09.2026 aufgefallen, weil bei 120 SCU Frachtraum und 70 SCU Vorrat
    nur 69 mitkamen und der Grund nirgends stand (das Geld reichte nicht).

    Gibt einen Sprachschlüssel zurück, oder `''`, wenn nichts begrenzt.
    """
    amount, _profit = amount_and_profit(trip, free_scu, money)
    if not amount:
        return ''
    # ⚠ Reihenfolge nach Ärgerlichkeit: Was der Spieler ändern **kann**
    # (Geld, Schiff) zuerst — der Vorrat am Terminal ist nicht seine Sache.
    if trip.get('ek') and amount == int((money or 0) // trip['ek']) \
            and amount < int(free_scu or 0):
        return 's_rt_grenze_geld'
    if amount == int(free_scu or 0):
        return 's_rt_grenze_frachtraum'
    if amount == int(trip.get('vorrat') or 0):
        return 's_rt_grenze_vorrat'
    if trip.get('bedarf') and amount == int(trip['bedarf']):
        return 's_rt_grenze_bedarf'
    return ''


def single_trips(start, scu, money, most=20):
    """Die lohnendsten Einzelfahrten ab einem Ort, beste zuerst.

    Je Eintrag: die Fahrt, dazu `menge` und `gewinn` für **dieses** Schiff und
    **dieses** Geld.
    """
    result = []
    for f in trips(start) or []:
        amount, profit = amount_and_profit(f, scu, money)
        if amount > 0 and profit > 0:
            entry = dict(f)
            entry['menge'], entry['gewinn'] = amount, profit
            entry['grenze'] = what_limits(f, scu, money)
            result.append(entry)
    result.sort(key=lambda e: -e['gewinn'])
    return result[:most]


def chain(start, scu, money, short=False, most=5, stops=2,
          round_trip=False, fetch_missing=True):
    """Mehrere Fahrten hintereinander: Ziel der einen ist Start der nächsten.

    `stops` sagt, über wie viele Fahrten geplant wird (2 bis `MAX_STOPS`).
    `round_trip=True` verlangt, dass die letzte Fahrt **zurück zum Startort**
    führt — A → B → C → A.

    `short=True` sortiert nach **Gesamtstrecke** statt nach Gewinn — für den
    Abend, an dem man nicht quer durchs System fliegen will.

    ⚠⚠ **Warum die Rundreise mehr ist als Bequemlichkeit.** Ohne sie steht man
    am Ende irgendwo mit leerem Laderaum und muss die Rückfahrt leer fliegen —
    die zählt in der Rechnung nicht, kostet aber dieselbe Zeit. Eine Route, die
    dort endet, wo sie anfing, lässt sich **wiederholen**.

    ⚠ Jede weitere Stufe kostet Abrufe: je Kandidat einen. Deshalb wird der
    Baum bei jeder Stufe auf `CHAIN_CANDIDATES` beschnitten — sonst wären es
    bei drei Stopps schon einige hundert.

    Gibt eine Liste von `(gesamtgewinn, [fahrten])` zurück.
    """
    stops = max(2, min(int(stops or 2), MAX_STOPS))
    # Ein Zweig ist (Gewinn bisher, Ort jetzt, Liste der Fahrten).
    branches = [(0.0, str(start), [])]
    for step in range(stops):
        next_branches = []
        last_step = (step == stops - 1)
        for profit_so_far, place, so_far in branches:
            # ⚠ `fetch_missing=False` rechnet **nur** mit dem, was schon abgelegt
            # ist. Für „beste Route überall" ist das Pflicht: Dort werden 184
            # Startorte durchgerechnet, und jeder fehlende Zwischenstopp wäre
            # ein Netzabruf — Minuten statt Sekunden.
            if trips(place) is None and (not fetch_missing or not fetch(place)):
                continue
            # ⚠⚠ **Auf der Rückfahrt zählen ALLE Fahrten, nicht nur die fünf
            # besten.** Die Fahrt, die zufällig zum Startort zurückführt,
            # steht so gut wie nie unter den gewinnstärksten fünf — und wenn
            # sie nicht dabei ist, gibt es überhaupt keine Rundreise.
            #
            # Genau daran ist sie bis v3.15.0-rc4 immer gescheitert: gemessen
            # ab Nyx Gateway 192 Einzelfahrten, und bei 2, 3 und 4 Stationen
            # jedes Mal „keine Route". Am 05.09.2026 gefragt: „Wenn ich
            # Rundreise angebe — was kaufe ich auf dem Rückweg?" Gar nichts,
            # es kam nie eine zustande.
            #
            # Teuer ist das nicht: Die Fahrten dieses Ortes liegen bereits in
            # der Ablage, es wird nur weniger davon weggeworfen.
            limit = (RETURN_CANDIDATES if (round_trip and last_step)
                     else CHAIN_CANDIDATES)
            # Nach jeder Fahrt ist mehr Geld da — das darf die nächste nutzen.
            for f in single_trips(place, scu, (money or 0) + profit_so_far,
                                  most=limit):
                if not f.get('ziel'):
                    continue
                # ⚠ Denselben Ort nicht zweimal anfahren — außer als Rückkehr
                # zum Start, und das nur auf der letzten Stufe.
                seen = {b['ziel'] for b in so_far} | {str(start)}
                if f['ziel'] in seen:
                    if not (round_trip and last_step
                            and f['ziel'] == str(start)):
                        continue
                next_branches.append((profit_so_far + f['gewinn'], f['ziel'],
                                      so_far + [f]))
        # ⚠⚠ **Erst aussortieren, dann kürzen.** Auf der letzten Stufe einer
        # Rundreise zählen nur Zweige, die wirklich am Start enden. Wer vorher
        # auf die fünf gewinnstärksten kürzt, wirft genau die weg — und die
        # Prüfung darunter findet dann nichts mehr vor.
        if round_trip and last_step:
            next_branches = [z for z in next_branches if z[1] == str(start)]
        # Nur die besten Zweige weiterverfolgen, sonst explodiert der Baum.
        next_branches.sort(key=lambda z: -z[0])
        branches = next_branches[:CHAIN_CANDIDATES]
        if not branches:
            return []

    done = [(g, way) for g, place, way in branches
            if len(way) == stops
            and (not round_trip or place == str(start))]
    if short:
        # ⚠ Fahrten ohne Streckenangabe fliegen heraus, statt als „0 Gm" ganz
        # nach oben zu rutschen — das wäre eine erfundene Nähe.
        done = [(g, w) for g, w in done
                if all(f.get('strecke') for f in w)]
        done.sort(key=lambda p: (sum(f['strecke'] for f in p[1]), -p[0]))
    else:
        done.sort(key=lambda p: -p[0])
    return done[:most]


def trade_posts():
    """Alle Terminals, die mit Ware handeln — `[(kennung, name)]`.

    Kommt aus der Verkaufs-Ablage; ein eigener Abruf wäre Verschwendung.
    """
    from . import selling
    spots = (selling.load() or {}).get('terminals') or {}
    result = []
    for ident, spot in spots.items():
        kind = spot.get('t')
        # Ältere Ablagen kennen die Art nicht — dann lieber mitnehmen als
        # eine leere Liste liefern.
        if kind is not None and kind not in selling.TRADE_TYPES:
            continue
        result.append((ident, spot.get('n') or spot.get('o') or '?'))
    return result


def fetch_all(progress=None, cancel=None):
    """Die Fahrten **aller** Handelsposten holen — für „beste Route überhaupt".

    ⚠⚠ **Das ist der teuerste Abruf im ganzen Werkzeug, und deshalb kein
    Automatismus.** Gemessen am 04.09.2026: 184 Handelsposten, rund **0,5 s je
    Abruf** — zusammen **92 Sekunden** und rund **1,9 MB** Ablage bei etwa
    11.000 Fahrten.

    Er läuft nur, wenn der Spieler ihn ausdrücklich anstößt. Beim Start
    ungefragt anderthalb Minuten lang eine fremde Schnittstelle abzugrasen wäre
    unhöflich — gegenüber UEX und gegenüber dem Spieler, der davon nichts hat,
    solange er nicht danach fragt.

    ⚠ **Warum es keinen billigeren Weg gibt** (beides gemessen): Ein Abruf je
    Planet ist bei 500 Zeilen gedeckelt, und die Antwort ist **unsortiert** —
    bei ArcCorp stand die größte Spanne (26,1 Mio.) nicht in den ersten
    Zeilen. Der Deckel schneidet also willkürlich ab; die zehn Planet-Abrufe
    lieferten ein Bruchstück, das man für das Ganze halten würde.

    `progress(fertig, gesamt)` wird nach jedem Posten gerufen, `cancel()`
    kann den Lauf beenden.
    """
    if OFF:
        return 0
    posts = trade_posts()
    done_count = 0
    for ident, _name in posts:
        if cancel and cancel():
            break
        fetch(ident)
        done_count += 1
        if progress:
            progress(done_count, len(posts))
    return done_count


def best_anywhere(scu, money, most=15):
    """Die lohnendsten Einzelfahrten über **alle** bekannten Startorte.

    ⚠ Rechnet nur mit dem, was schon abgelegt ist — sie holt **nichts** nach.
    Wer noch keinen Rundumlauf gemacht hat, sieht eben nur seine bisherigen
    Startorte. Das ist ehrlicher, als beim Öffnen einer Seite anderthalb
    Minuten ins Netz zu greifen.

    Je Eintrag zusätzlich `startname` — sonst wüsste niemand, wo die Fahrt
    beginnt.
    """
    names = dict(trade_posts())
    result = []
    for ident, entry in (_all() or {}).items():
        for f in entry.get('fahrten') or []:
            amount, profit = amount_and_profit(f, scu, money)
            if amount <= 0 or profit <= 0:
                continue
            e = dict(f)
            e['menge'], e['gewinn'] = amount, profit
            e['grenze'] = what_limits(f, scu, money)
            e['startname'] = names.get(ident, '?')
            result.append(e)
    result.sort(key=lambda e: -e['gewinn'])
    return result[:most]


def best_chains_anywhere(scu, money, short=False, stops=2, round_trip=False,
                         most=15):
    """Die besten **Ketten** über alle abgelegten Startorte.

    ⚠⚠ **Die Schalter mussten auch hier gelten.** Am 05.09.2026 gemeldet:
    „Ich möchte eine Rundreise über 3 Stationen, kurze Strecken — die Anzeige
    bleibt aber so wie am Anfang geladen." Zu Recht: `best_anywhere` kannte
    nur Einzelfahrten, die Schalter darüber färbten sich und bewirkten nichts.
    Ein Bedienelement, das sich einschalten lässt und nichts tut, ist
    schlimmer als keins.

    ⚠ **Holt nichts nach** — dieselbe Regel wie bei `best_anywhere`. Gerechnet
    wird mit dem, was der Rundumlauf gesammelt hat; gemessen bleibt das nach
    einem vollen Lauf unter einer Sekunde.

    Gibt `[(gewinn, startname, [fahrten])]` zurück.
    """
    names = dict(trade_posts())
    result = []
    for ident in list(_all() or {}):
        for profit, way in chain(ident, scu, money, short=short,
                                 most=3, stops=stops,
                                 round_trip=round_trip, fetch_missing=False):
            result.append((profit, names.get(ident, '?'), way))
    if short:
        result.sort(key=lambda p: (sum(f.get('strecke') or 0 for f in p[2]),
                                   -p[0]))
    else:
        result.sort(key=lambda p: -p[0])
    return result[:most]


def known_starts():
    """Wieviele **Handelsposten** schon abgelegt sind — für die Anzeige.

    ⚠⚠ **Nur die, die auch in der Postenliste stehen.** Vorher wurde einfach
    die Ablage gezählt — und die enthält noch Startorte aus der Zeit vor dem
    Handelsfilter (Läden, Tankstellen). In der Anzeige stand deshalb
    „187 von 184 Handelsposten": mehr, als es gibt.

    Eine Zahl, die größer ist als ihr Nenner, macht die ganze Anzeige
    unglaubwürdig — auch die Teile, die stimmen.
    """
    idents = {k for k, _n in trade_posts()}
    if not idents:
        return len(_all() or {})
    return sum(1 for k in (_all() or {}) if k in idents)


def forget():
    """Alles Nachgeschlagene verwerfen — für den Selbsttest und die Diagnose."""
    _store.save({'starts': {}}, compact=True)
    _store.forget()
