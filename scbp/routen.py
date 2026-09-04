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
`KETTEN_KANDIDATEN` besten Ziele weiterverfolgt: höchstens ein paar Abrufe
statt siebzig.

## ⚠⚠ Und eine Warnung, die in jede Anzeige gehört

`scu_origin` (wieviel dort liegt) ist **von Spielern gemeldet** und altert. Die
Spitzenreiter sind fast immer kleine Mengen mit riesiger Spanne — steht die
Ware nicht mehr da, ist die ganze Fahrt wertlos. Das Alter der Daten gehört
deshalb an jede Route, nicht in eine Fußnote.
"""
import time

from . import uex
from .katalog import AUS

QUELLE = 'https://api.uexcorp.uk/2.0/commodities_routes?id_terminal_origin=%s'
CACHE = 'routen.json'
FORMAT = 1

# Sechs Stunden. Kürzer als bei den Ladenpreisen: Eine Handelsspanne lebt von
# Beständen, und die ändern sich im Lauf eines Abends.
HALTBAR = 6 * 60 * 60

# Wieviele Startorte die Ablage behält.
#
# ⚠⚠ **Muss über der Zahl der Handelsposten liegen (184).** Stand hier vorher
# auf 25 — mit dem Rundumlauf aus `alle_holen()` hätte sich die Ablage dabei
# selbst leergeräumt: Ab dem 26. Posten wäre bei jedem weiteren der älteste
# hinausgeflogen, und am Ende stünden 25 zufällige statt aller 184 da. Der
# Fehler wäre nicht aufgefallen — die Liste hätte einfach weniger gezeigt.
#
# 200 deckt alle Posten ab und bleibt bei rund 2 MB.
HOECHSTENS = 200

# ⚠ Wieviele Ziele für eine Kette weiterverfolgt werden. Jeder kostet einen
# eigenen Abruf — bei 69 Fahrten je Startort wären es sonst 69.
KETTEN_KANDIDATEN = 5

# ⚠ Wieviele Fahrten eine Route höchstens hat. Jede Stufe kostet Abrufe, und
# eine Route über sechs Stationen plant ohnehin niemand: Bis man beim letzten
# Stopp ist, sind die Preise vom Anfang alt.
MAX_STOPPS = 4

_ablage = uex.Ablage(CACHE, format_nr=FORMAT, haltbar=HALTBAR)


def _alle():
    return (_ablage.laden() or {}).get('starts') or {}


def alter(start):
    """Wie alt die Fahrten ab diesem Ort sind — oder `None`."""
    eintrag = _alle().get(str(start))
    if not eintrag:
        return None
    try:
        return time.time() - float(eintrag.get('geholt') or 0)
    except (TypeError, ValueError):
        return None


def fahrten(start):
    """Alle bekannten Fahrten ab diesem Terminal.

    `None` heißt „noch nicht nachgesehen", `[]` heißt „von hier lohnt nichts".
    """
    eintrag = _alle().get(str(start))
    if eintrag is None:
        return None
    return eintrag.get('fahrten') or []


def holen(start, erzwingen=False):
    """Die Fahrten ab einem Terminal nachschlagen."""
    if AUS or not start:
        return False
    a = alter(start)
    if not erzwingen and a is not None and a < HALTBAR:
        return True
    roh = uex.holen(QUELLE % start, 'routen')
    if roh is None:
        return False

    liste = []
    for x in roh:
        ek = float(x.get('price_origin') or 0)
        vk = float(x.get('price_destination') or 0)
        # ⚠ Drei Gründe, eine Zeile wegzulassen — und alle drei sind nötig:
        # ohne Einkaufspreis kann man nicht kaufen, ohne Aufschlag lohnt es
        # nicht, und ohne Vorrat steht dort nichts im Regal.
        if ek <= 0 or vk <= ek or not (x.get('scu_origin') or 0):
            continue
        liste.append({
            'ware': (x.get('commodity_name') or '').strip(),
            'ziel': str(x.get('id_terminal_destination') or ''),
            'zielname': (x.get('destination_terminal_name') or '').strip(),
            'zielort': (x.get('destination_planet_name')
                        or x.get('destination_orbit_name') or '').strip(),
            'zielsystem': (x.get('destination_star_system_name') or '').strip(),
            'ek': ek,
            'vk': vk,
            'gewinn_scu': vk - ek,
            # Entfernung in Gm. `0`, wenn UEX keine kennt — dann wird bei
            # „kurze Route" nichts behauptet.
            'strecke': float(x.get('distance') or 0),
            'vorrat': int(x.get('scu_origin') or 0),
            'bedarf': int(x.get('scu_destination') or 0),
        })
    liste.sort(key=lambda f: -f['gewinn_scu'])

    starts = dict(_alle())
    starts[str(start)] = {'geholt': time.time(), 'fahrten': liste}
    if len(starts) > HOECHSTENS:
        nach_alter = sorted(starts.items(),
                            key=lambda p: p[1].get('geholt') or 0)
        for schluessel, _wert in nach_alter[:len(starts) - HOECHSTENS]:
            starts.pop(schluessel, None)
    return _ablage.sichern({'starts': starts}, kompakt=True)


def menge_und_gewinn(fahrt, scu_frei, geld):
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
    menge = min(int(scu_frei or 0), int(fahrt.get('vorrat') or 0))
    if fahrt.get('bedarf'):
        menge = min(menge, int(fahrt['bedarf']))
    if fahrt.get('ek'):
        menge = min(menge, int((geld or 0) // fahrt['ek']))
    menge = max(0, menge)
    return menge, menge * fahrt['gewinn_scu']


def einzelfahrten(start, scu, geld, hoechstens=20):
    """Die lohnendsten Einzelfahrten ab einem Ort, beste zuerst.

    Je Eintrag: die Fahrt, dazu `menge` und `gewinn` für **dieses** Schiff und
    **dieses** Geld.
    """
    raus = []
    for f in fahrten(start) or []:
        menge, gewinn = menge_und_gewinn(f, scu, geld)
        if menge > 0 and gewinn > 0:
            eintrag = dict(f)
            eintrag['menge'], eintrag['gewinn'] = menge, gewinn
            raus.append(eintrag)
    raus.sort(key=lambda e: -e['gewinn'])
    return raus[:hoechstens]


def kette(start, scu, geld, kurz=False, hoechstens=5, stopps=2,
          rundreise=False):
    """Mehrere Fahrten hintereinander: Ziel der einen ist Start der nächsten.

    `stopps` sagt, über wie viele Fahrten geplant wird (2 bis `MAX_STOPPS`).
    `rundreise=True` verlangt, dass die letzte Fahrt **zurück zum Startort**
    führt — A → B → C → A.

    `kurz=True` sortiert nach **Gesamtstrecke** statt nach Gewinn — für den
    Abend, an dem man nicht quer durchs System fliegen will.

    ⚠⚠ **Warum die Rundreise mehr ist als Bequemlichkeit.** Ohne sie steht man
    am Ende irgendwo mit leerem Laderaum und muss die Rückfahrt leer fliegen —
    die zählt in der Rechnung nicht, kostet aber dieselbe Zeit. Eine Route, die
    dort endet, wo sie anfing, lässt sich **wiederholen**.

    ⚠ Jede weitere Stufe kostet Abrufe: je Kandidat einen. Deshalb wird der
    Baum bei jeder Stufe auf `KETTEN_KANDIDATEN` beschnitten — sonst wären es
    bei drei Stopps schon einige hundert.

    Gibt eine Liste von `(gesamtgewinn, [fahrten])` zurück.
    """
    stopps = max(2, min(int(stopps or 2), MAX_STOPPS))
    # Ein Zweig ist (Gewinn bisher, Ort jetzt, Liste der Fahrten).
    zweige = [(0.0, str(start), [])]
    for stufe in range(stopps):
        naechste = []
        letzte_stufe = (stufe == stopps - 1)
        for gewinn_bisher, ort, bisher in zweige:
            if fahrten(ort) is None and not holen(ort):
                continue
            # Nach jeder Fahrt ist mehr Geld da — das darf die nächste nutzen.
            for f in einzelfahrten(ort, scu, (geld or 0) + gewinn_bisher,
                                   hoechstens=KETTEN_KANDIDATEN):
                if not f.get('ziel'):
                    continue
                # ⚠ Denselben Ort nicht zweimal anfahren — außer als Rückkehr
                # zum Start, und das nur auf der letzten Stufe.
                schon = {b['ziel'] for b in bisher} | {str(start)}
                if f['ziel'] in schon:
                    if not (rundreise and letzte_stufe
                            and f['ziel'] == str(start)):
                        continue
                naechste.append((gewinn_bisher + f['gewinn'], f['ziel'],
                                 bisher + [f]))
        # Nur die besten Zweige weiterverfolgen, sonst explodiert der Baum.
        naechste.sort(key=lambda z: -z[0])
        zweige = naechste[:KETTEN_KANDIDATEN]
        if not zweige:
            return []

    fertige = [(g, weg) for g, ort, weg in zweige
               if len(weg) == stopps
               and (not rundreise or ort == str(start))]
    if kurz:
        # ⚠ Fahrten ohne Streckenangabe fliegen heraus, statt als „0 Gm" ganz
        # nach oben zu rutschen — das wäre eine erfundene Nähe.
        fertige = [(g, w) for g, w in fertige
                   if all(f.get('strecke') for f in w)]
        fertige.sort(key=lambda p: (sum(f['strecke'] for f in p[1]), -p[0]))
    else:
        fertige.sort(key=lambda p: -p[0])
    return fertige[:hoechstens]


def handelsposten():
    """Alle Terminals, die mit Ware handeln — `[(kennung, name)]`.

    Kommt aus der Verkaufs-Ablage; ein eigener Abruf wäre Verschwendung.
    """
    from . import verkauf
    stellen = (verkauf.laden() or {}).get('terminals') or {}
    raus = []
    for kennung, stelle in stellen.items():
        art = stelle.get('t')
        # Ältere Ablagen kennen die Art nicht — dann lieber mitnehmen als
        # eine leere Liste liefern.
        if art is not None and art not in verkauf.HANDELSARTEN:
            continue
        raus.append((kennung, stelle.get('n') or stelle.get('o') or '?'))
    return raus


def alle_holen(fortschritt=None, abbruch=None):
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

    `fortschritt(fertig, gesamt)` wird nach jedem Posten gerufen, `abbruch()`
    kann den Lauf beenden.
    """
    if AUS:
        return 0
    posten = handelsposten()
    fertig = 0
    for kennung, _name in posten:
        if abbruch and abbruch():
            break
        holen(kennung)
        fertig += 1
        if fortschritt:
            fortschritt(fertig, len(posten))
    return fertig


def beste_ueberall(scu, geld, hoechstens=15):
    """Die lohnendsten Einzelfahrten über **alle** bekannten Startorte.

    ⚠ Rechnet nur mit dem, was schon abgelegt ist — sie holt **nichts** nach.
    Wer noch keinen Rundumlauf gemacht hat, sieht eben nur seine bisherigen
    Startorte. Das ist ehrlicher, als beim Öffnen einer Seite anderthalb
    Minuten ins Netz zu greifen.

    Je Eintrag zusätzlich `startname` — sonst wüsste niemand, wo die Fahrt
    beginnt.
    """
    namen = dict(handelsposten())
    raus = []
    for kennung, eintrag in (_alle() or {}).items():
        for f in eintrag.get('fahrten') or []:
            menge, gewinn = menge_und_gewinn(f, scu, geld)
            if menge <= 0 or gewinn <= 0:
                continue
            e = dict(f)
            e['menge'], e['gewinn'] = menge, gewinn
            e['startname'] = namen.get(kennung, '?')
            raus.append(e)
    raus.sort(key=lambda e: -e['gewinn'])
    return raus[:hoechstens]


def bekannte_starts():
    """Wieviele Startorte schon abgelegt sind — für die Anzeige."""
    return len(_alle() or {})


def vergessen():
    """Alles Nachgeschlagene verwerfen — für den Selbsttest und die Diagnose."""
    _ablage.sichern({'starts': {}}, kompakt=True)
    _ablage.vergessen()
