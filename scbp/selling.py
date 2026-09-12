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
Verkaufspreise je Terminal — „wo werde ich das los?"

Das Gegenstück zu `prices.py`. Dort geht es um **kaufen oder abbauen**, hier um
die Frage danach: Der Laderaum ist voll, wo bringt die Ladung am meisten?

## Warum das hier steht und nicht auf einer Webseite

Zwei Seiten beantworten die Frage bereits (sc-trade.tools, uexcorp.space) —
aber **beide nur für eine Ware auf einmal**. Wer Gold, Copper und Iron im
Laderaum hat, muss dort dreimal fragen und die Ergebnisse selbst
übereinanderlegen. Genau diese Arbeit nimmt dieser Reiter ab, weil er das
Handelslager kennt.

Gemessen am 30.08.2026 für je 100 SCU Gold, Copper und Iron:

| Weg | Erlös |
|---|---|
| alles an **einem** Ort (TDD Area 18) | 4.010.000 aUEC |
| jede Ware am je besten Ort, drei Stopps | 4.100.000 aUEC |

**Zwei Prozent für zwei zusätzliche Anflüge.** Ein Stopp lohnt fast immer —
und das ist die Antwort, die keine der beiden Seiten von sich aus gibt.

## Woher

[UEX Corp](https://uexcorp.space) API 2.0, Endpunkt `commodities_prices_all` —
2.585 Einträge, rund 1 MB. Kein Schlüssel nötig, ein einfacher GET. Behalten
wird davon nur, was ein Ankaufgebot hat (1.880 Zeilen, rund 75 KB).

Die Ortsnamen kommen aus `places.py`, das dieselbe Terminal-Liste ohnehin holt —
so wird die fremde Schnittstelle nicht zweimal für dasselbe angefasst.

⚠ **Die Daten werden NICHT mitgeliefert**, sondern auf dem Rechner des Nutzers
geholt — dieselbe Regel wie bei scmdb, `prices.py` und `places.py`. Und
**höchstens einmal am Tag**.

⚠ **Ohne Netz passiert nichts Schlimmes.** Liegt eine alte Ablage da, wird sie
benutzt; liegt keine da, bleibt der Reiter leer und sagt das auch. Kein Fehler,
kein Absturz.

## ⚠⚠ Zwei Fallen, die beim Bauen zugeschnappt sind

**1. Der Namensfilter von UEX sucht Teiltexte.** `commodity_name=Gold` liefert
`Golden Medmon` gleich mit — und dessen 71.000 aUEC/SCU sahen aus wie ein
sagenhafter Goldpreis, während Gold tatsächlich bei 33.000 liegt. Deshalb wird
hier **ausschliesslich exakt** verglichen, nie mit `in` oder `startswith`.

**2. `norm_material()` darf hier NICHT benutzt werden.** Die Funktion aus
`crafting.py` schneidet die Klammer ab, damit die Bergbau-Sicht zu
`Aslarite (Raw)` einen Fundort findet — für die Herstellung richtig. Beim
Verkauf wäre es falsch, denn Erz und veredelte Ware sind **verschiedene Waren
mit verschiedenen Preisen**:

| Ware | bester Ankauf je SCU |
|---|---|
| Copper | 4.400 |
| Copper (Ore) | 1.200 |
| Gold | 33.000 |
| Gold (Ore) | kein Ankauf |

Wer das zusammenwirft, verspricht jemandem das 3,7-fache. Beide Prüfungen
stehen in `tools/selbsttest.py`, damit sie nicht wieder hereinrutschen.

## Gestohlene Ware

Als gestohlen markierte Ladung nimmt nicht jedes Terminal an. UEX kennzeichnet
die Stellen, die keine Fragen stellen, mit `is_nqa` (*no questions asked*) —
**15 Terminals**, davon 7 mit Ankaufgeboten (Brio's Breaker Yard, GrimHEX,
Nuen Waste Management, Raven's Roost und drei weitere). Der Reiter blendet auf
Wunsch auf diese Auswahl ein.

Die Idee zu diesem Reiter stammt von **Morkhan (KRT)** (30.08.2026).
"""
import time

from . import places, uex
from .catalog import OFF

SOURCE = 'https://api.uexcorp.uk/2.0/commodities_prices_all'
CACHE = 'verkauf.json'
# ⚠ Auf 2 gesetzt, als der Füllstand (`z`) dazukam, auf 3 mit dem Terminalnamen
# (`n`), auf 4 mit der Terminal-Art (`t`). Eine alte Ablage hätte die Felder
# nicht — ein höherer Formatstand holt sie einmal neu, statt die Anzeige einen
# Tag lang lückenhaft zu lassen. Ein Abruf mehr, dafür sofort vollständig.
FORMAT = 4

# Welche Terminal-Arten mit **Ware** handeln. Alles andere taugt für eine
# Handelsroute nicht — siehe die Begründung bei `'t'` weiter unten.
TRADE_TYPES = ('commodity', 'commodity_raw')
TIMEOUT = 30

# ⭐⭐ **Beim Verkauf ist „voll" das Schlechte.** Das ist der Punkt, an dem die
# Ampel überhaupt nützt: Ein Terminal mit vollem Lager hat keinen Bedarf mehr
# und nimmt die Ladung nicht — obwohl der Preis noch dransteht. Wer das erst
# nach dem Anflug merkt, hat die Strecke umsonst gemacht.
#
# ⚠⚠ **Gemessen am 04.09.2026 über alle 1.880 Ankaufzeilen: 90,2 % stehen auf
# Stufe 1** (leer, nimmt alles). Eine Ampel, die zu neun Zehnteln grün leuchtet,
# ist keine Ampel, sondern Farbe. Deshalb bleiben die unauffälligen Stufen
# **stumm** — angezeigt wird nur, was eine Entscheidung ändert:
#
# | Stufe | Lager | wird gezeigt |
# |---|---|---|
# | 1–2 | leer bis sehr wenig (92,5 %) | nichts — der Normalfall |
# | 3–4 | mittel (4,8 %) | nichts |
# | 5 | füllt sich (1,7 %) | Hinweis in Gold |
# | 6–7 | fast voll / voll (1,0 %) | Warnung in Rot |
#
# Ein Zeichen, das fast immer da ist, wird übersehen. Eines, das selten kommt,
# wird gelesen.
FILLING_UP = 5
NO_DEMAND = 6

# Ein Tag. Preise ändern sich im Spiel laufend, aber nicht im Minutentakt —
# dieselbe Überlegung wie in `prices.py`.
SHELF_LIFE = uex.DAY

# Ab wann eine Meldung als alt gilt und in der Anzeige abgesetzt wird.
# Gemessen am 30.08.2026 über alle 1.880 Ankauf-Einträge: 98,5 % waren jünger
# als eine Woche, die Hälfte jünger als 2,2 Tage, der älteste 15 Tage. Eine
# Woche trennt also sauber zwischen „normal" und „schau genau hin".
OLD = 7 * 24 * 60 * 60

# ⚠ **Auch ein Fehlversuch bremst — aber nur kurz.**
#
# Scheitert der Abruf, wird nichts abgelegt; die Stundensperre greift also
# nicht, denn sie rechnet aus dem letzten **erfolgreichen** Abruf. Ohne diese
# zweite, kurze Bremse könnte jemand bei kaputter Leitung im Sekundentakt
# drücken und jedes Mal eine Anfrage losschicken.
#
# Eine Minute, nicht eine Stunde: Wer sein Netz repariert, soll es sofort
# wieder versuchen dürfen und nicht für einen fremden Ausfall bestraft werden.
ERROR_LOCK = 60

# Abruf und Ablage liegen im gemeinsamen Unterbau — siehe `scbp/uex.py`.
_store = uex.Store(CACHE, format_no=FORMAT, shelf_life=SHELF_LIFE)

# Wann zuletzt vergeblich angefragt wurde. Bewusst nur im Arbeitsspeicher: Nach
# einem Neustart des Werkzeugs darf man es sofort wieder versuchen.
_last_failure = {'zeit': 0.0}

# ⭐ **Der Knopf „Jetzt aktualisieren" ist auf einmal pro Stunde begrenzt.**
#
# Wer gerade landet und wissen will, was das Terminal heute zahlt, soll nicht
# bis morgen warten müssen — der tägliche Abruf allein reicht dafür nicht.
# Ohne Sperre wäre der Knopf aber eine Einladung, im Minutentakt zu drücken,
# und ein Werkzeug, das eine fremde Schnittstelle so anfasst, ist ein
# schlechter Gast (dieselbe Überlegung wie in `prices.py`).
#
# Eine Stunde ist der Kompromiss: oft genug für einen Handelsflug, selten genug,
# dass 100 Nutzer zusammen keine Last erzeugen.
LOCK = 60 * 60

# Zweite Quelle: die Terminal-Liste. Sie liefert System, Ort und das Kennzeichen
# `is_nqa`, das in den Preisdaten fehlt.
#
# ⚠ `places.py` holt dieselbe Liste — dort aber nur wöchentlich und nur für die
# Ortsnamen. Bewusst **nicht** gekoppelt: Ein Modul, das sich seine Daten selbst
# besorgt, lässt sich einzeln prüfen und geht nicht kaputt, wenn am anderen
# etwas geändert wird. Der Preis dafür ist ein zusätzlicher Abruf pro Tag.
SOURCE_TERMINALS = 'https://api.uexcorp.uk/2.0/terminals'


def load():
    """Der abgelegte Stand — aus dem Speicher, wenn die Datei unverändert ist."""
    return _store.load()


def age():
    """Wie alt die Ablage ist, in Sekunden — oder None, wenn keine da ist."""
    return _store.age()


def wait_time():
    """Wie viele Sekunden der Knopf „Jetzt aktualisieren" noch gesperrt ist.

    `0` heisst: darf sofort. Siehe `LOCK`.
    """
    since_error = time.time() - _last_failure['zeit']
    rest_error = max(0, int(ERROR_LOCK - since_error))
    a = age()
    if a is None:
        return rest_error
    return max(rest_error, int(LOCK - a) if a < LOCK else 0)


def update(force=False, progress=None):
    """Die Verkaufspreise holen.

    Ohne `force` passiert nur etwas, wenn die Ablage fehlt oder älter als
    ein Tag ist — der stille Abruf im Hintergrund.

    Mit `force=True` ist es der Knopf aus dem Reiter. Der darf höchstens
    einmal pro Stunde (siehe `LOCK`); ist er noch gesperrt, kommt
    `(False, 'gesperrt')` zurück und die Oberfläche zeigt die Restzeit.

    Gibt `(Erfolg, Grund)` zurück. `Grund` ist eine Kennung, kein fertiger Satz
    — die Übersetzung macht die Oberfläche:

    | Grund | heisst |
    |---|---|
    | `''` | alles in Ordnung, nichts zu melden |
    | `'gesperrt'` | Knopf noch in der Stundensperre |
    | `'netz'` | Abruf fehlgeschlagen (kein Netz, Schnittstelle weg) |
    | `'leer'` | Antwort kam an, enthielt aber keine Preise |
    | `'aus'` | Netzzugriff ist abgeschaltet (`SC_BP_NO_NET=1`) |
    """
    if OFF:
        return False, 'aus'
    if force:
        if wait_time():
            return False, 'gesperrt'
    elif not _store.stale():
        return True, ''
    if progress:
        progress('')
    rows = uex.fetch(SOURCE, 'selling', timeout=TIMEOUT)
    if rows is None:
        _last_failure['zeit'] = time.time()
        return False, 'netz'
    if not rows:
        _last_failure['zeit'] = time.time()
        return False, 'leer'
    # Die Terminal-Liste darf fehlschlagen, ohne dass alles scheitert: Ohne sie
    # kennen wir System und `is_nqa` nicht, aber der Terminal-Name steht in den
    # Preisdaten selbst. Lieber eine Liste ohne Systemspalte als gar keine.
    spots = uex.fetch(SOURCE_TERMINALS, 'selling.terminals',
                      timeout=TIMEOUT) or []

    # ⚠⚠ **Hier steht mit Absicht KEIN Spielstand.**
    #
    # Die Preisdaten führen keinen (`commodities_prices_all` hat kein
    # `game_version`), und das Feld an den Terminals bedeutet etwas anderes:
    # „in dieser Version zuletzt gesehen". Gemessen am 30.08.2026 über 826
    # Terminals verteilt es sich auf 3.24.2 (151×), 4.6.0 (126×), 4.0 (106×)
    # und 84 ohne Angabe — der häufigste Wert wäre also `3.24.2` gewesen,
    # während die Preise tatsächlich aus 4.10.0 stammten.
    #
    # Zwei Anläufe, beide falsch. Statt einen dritten Kniff zu suchen, sagt der
    # Reiter, was er **weiss**: wie alt die Meldungen sind (`age()`). Eine
    # Versionsnummer, die man nicht belegen kann, ist schlimmer als keine.
    terminals = {}
    for x in spots:
        ident = x.get('id')
        if ident is None:
            continue
        place = (x.get('space_station_name') or x.get('city_name')
                 or x.get('outpost_name') or x.get('planet_name') or '')
        terminals[str(ident)] = {
            'o': place,
            's': x.get('star_system_name') or '',
            'q': 1 if x.get('is_nqa') else 0,
            # ⚠⚠ **Der Terminalname gehört dazu.** Ohne ihn standen im
            # Routen-Reiter acht Zeilen „Seraphim Station · Stanton"
            # untereinander — eine Station hat viele Terminals (Admin, TDD,
            # Läden), und die verkaufen Verschiedenes. Wer auswählen soll,
            # muss unterscheiden können. Gemeldet am 04.09.2026.
            'n': (x.get('name') or '').strip(),
            # ⚠⚠ **Die Art des Terminals — und die entscheidet, ob es für
            # Handel überhaupt taugt.** Gemessen am 04.09.2026 über alle 826:
            # nur **161 `commodity`** und **23 `commodity_raw`**; die übrigen
            # 642 sind Läden (`item`, 481), Tankstellen (`fuel`, 99),
            # Miet- und Kaufstationen für Schiffe.
            #
            # Xharig dazu: „Wer kauft Schiffswaffen und verkauft die? Wir
            # wollen den Leuten sinnvolle Routen geben, nicht die komplette
            # Liste aller Shops." Seraphim Station hat 16 Terminals — und
            # **genau eines** davon handelt mit Ware.
            't': (x.get('type') or '').strip(),
        }

    # ⚠⚠ **Nur Zeilen mit echtem Ankaufgebot behalten.** `price_sell = 0` heisst
    # „dieses Terminal nimmt die Ware nicht", nicht „es zahlt nichts". Wer die
    # Zeilen mitschleppt, hat 705 Einträge mehr in der Ablage und muss an jeder
    # Stelle daran denken, sie wegzufiltern — einmal vergessen, und im Reiter
    # steht ein Ort mit „0 aUEC" ganz unten in der Liste.
    #
    # ⚠ Der Warenname wird **unverändert** übernommen, mit Klammer und allem.
    # Siehe die zweite Falle im Kopf: `Copper` und `Copper (Ore)` sind zwei
    # verschiedene Waren, und `norm_material()` würde sie zusammenwerfen.
    goods_map = {}
    for x in rows:
        price = float(x.get('price_sell') or 0)
        if price <= 0:
            continue
        name = (x.get('commodity_name') or '').strip()
        if not name:
            continue
        goods_map.setdefault(name, []).append({
            't': str(x.get('id_terminal')),
            'n': (x.get('terminal_name') or '').strip(),
            'p': price,
            'd': int(x.get('date_modified') or 0),
            'k': x.get('container_sizes') or '',
            # ⭐ Wie voll das Lager dort ist, in UEX' eigenen sieben Stufen.
            # Beim **Verkauf** ist voll das Schlechte: Ein randvolles Terminal
            # hat keinen Bedarf mehr und nimmt die Ladung nicht.
            'z': int(x.get('status_sell') or 0),
        })
    if not goods_map:
        return False, 'leer'
    for lines in goods_map.values():
        lines.sort(key=lambda z: -z['p'])
    # ⚠ `compact`: Diese Ablage ist mit rund 75 KB die grösste der drei —
    # ohne Leerzeichen zwischen den Feldern spart das spürbar Platz.
    _store.save({'terminals': terminals, 'waren': goods_map}, compact=True)
    return True, ''


def fill_level(row):
    """Was der Füllstand einer Verkaufsstelle bedeutet — oder `None`.

    Gibt `(schluessel, ist_warnung)` zurück: den Sprachschlüssel für den Text
    und ob es eine Warnung ist (rot) oder ein Hinweis (gold).

    ⚠ **`None` heisst „nichts sagen"** — nicht „alles in Ordnung". Beides
    sieht in der Anzeige gleich aus, und das ist Absicht: Der Normalfall
    braucht kein Zeichen. Siehe `FILLING_UP` oben.

    ⚠ Ältere Ablagen kennen das Feld nicht (`z` fehlt). Dann wird ebenfalls
    geschwiegen — eine Warnung aus fehlenden Daten wäre geraten.
    """
    level = (row or {}).get('z') or 0
    if level >= NO_DEMAND:
        return 's_vk_voll', True
    if level == FILLING_UP:
        return 's_vk_fuellt', False
    return None


def goods():
    """Alle Waren mit mindestens einem Ankaufgebot, alphabetisch.

    Rund 150 Namen, genau so geschrieben wie bei UEX — `Copper` und
    `Copper (Ore)` stehen beide darin und sind **nicht** dasselbe.
    """
    return sorted((load() or {}).get('waren') or {})


def known(name):
    """Kennt die Ablage diese Ware? Exakter Vergleich, siehe Falle 1 im Kopf."""
    return name in ((load() or {}).get('waren') or {})


def places_for(names, nqa_only=False):
    """Wo man die genannten Waren los wird — die beste Stelle zuerst.

    `names` ist eine Liste von Warennamen, wie sie `goods()` liefert.

    Zurück kommt eine Liste von Orten. Sortiert wird **zuerst nach der Zahl der
    abgenommenen Waren**, erst danach nach Preis:

        Ein Ort, der alle drei Waren nimmt, steht über einem, der nur die
        teuerste nimmt.

    Das ist der ganze Sinn des Reiters. Gemessen am 30.08.2026 kostet der
    Umweg über mehrere Terminals mehr Zeit, als er einbringt — zwei Prozent
    Mehrerlös für zwei zusätzliche Anflüge (siehe Kopf).

    ⚠ **Ohne Mengen wird nicht summiert.** `summe` ist die Summe der Preise je
    SCU, also eine Rangzahl zum Sortieren — **kein Erlös**. Wer sie als Erlös
    anzeigt, behauptet etwas über Mengen, die das Werkzeug nicht kennt. Für
    einen echten Erlös braucht es das Handelslager (`trade_cargo.py`).

    `nqa_only=True` blendet auf die Stellen ein, die keine Fragen stellen —
    für als gestohlen markierte Ladung.
    """
    data = load() or {}
    all_goods = data.get('waren') or {}
    spots = data.get('terminals') or {}
    wanted = [n for n in names if n in all_goods]
    if not wanted:
        return []

    now = time.time()
    collected = {}
    for item in wanted:
        for row in all_goods[item]:
            ident = row['t']
            spot = spots.get(ident) or {}
            if nqa_only and not spot.get('q'):
                continue
            entry = collected.setdefault(ident, {
                'terminal': row.get('n') or '?',
                'ort': spot.get('o') or '',
                'system': spot.get('s') or '',
                'nqa': bool(spot.get('q')),
                'treffer': [],
            })
            entry['treffer'].append({
                'ware': item,
                'preis': row['p'],
                'kisten': row.get('k') or '',
                # Alter in Sekunden. `None`, wenn die Meldung kein Datum hat —
                # dann wird in der Anzeige nichts behauptet.
                'alter': (now - row['d']) if row.get('d') else None,
                # ⚠ Der Füllstand gehört an die **Ware**, nicht an den Ort:
                # Dasselbe Terminal kann bei Gold randvoll und bei Iron leer
                # sein. Ein Zeichen am Ort wäre für die halbe Ladung falsch.
                'fuellstand': fill_level(row),
            })

    result = []
    for entry in collected.values():
        entry['treffer'].sort(key=lambda tr: -tr['preis'])
        entry['anzahl'] = len(entry['treffer'])
        entry['summe'] = sum(tr['preis'] for tr in entry['treffer'])
        # Das Alter des Ortes ist das der **ältesten** Meldung, die ihn stützt.
        # Die vorsichtigere Angabe: Wer drei Waren dort verkaufen will, verlässt
        # sich auf alle drei Meldungen, nicht nur auf die frischeste.
        ages = [tr['alter'] for tr in entry['treffer']
                if tr['alter'] is not None]
        entry['alter'] = max(ages) if ages else None
        result.append(entry)

    result.sort(key=lambda e: (-e['anzahl'], -e['summe']))
    return result


# Waren, die in der Bestenliste nichts zu suchen haben.
#
# ⚠⚠ **Das sind Event-Geschenke, keine Handelsware.** Gemeldet am 08.09.2026:
# „die ersten 2 auf der Liste sind Karten vom Event, die gibt es gar nicht als
# 1 SCU." Sie tragen einen Preis, weil ein Terminal sie ankauft — aber niemand
# fliegt eine SCU davon irgendwohin. In der Bestenliste standen sie auf Platz
# 1 und 2 und verdrängten alles, womit sich wirklich Geld verdienen lässt.
#
# ⚠ Ausgeschlossen wird nur die BESTENLISTE. Wer den Namen sucht, bekommt
# weiterhin seine Ortsliste — die Ware verschwindet nicht aus dem Programm.
#
# ⚠ Die Erkennung geht über den Namen, weil die Daten nichts hergeben:
# `container_sizes` steht bei den Karten auf denselben Werten wie bei Erzen.
# Kommt ein neues Event dazu, gehört sein Geschenk hier hinein.
NOT_IN_TOP_LIST = (
    'luminalia gift',
    'year of the rat envelope',
)


def in_top_list(name):
    """Gehört diese Ware in „Was gerade am besten zahlt"?"""
    return (name or '').strip().lower() not in NOT_IN_TOP_LIST


# Ab welchem Vielfachen des zweithöchsten Gebots ein Preis als Ausreißer gilt.
#
# ⚠ **Die Zahl ist gemessen, nicht gesetzt.** Am 08.09.2026 über alle 114 Waren
# geprüft: Genau **zwei** liegen über Faktor 3, und beide sind offensichtlich
# falsch — „Year of the Rat Envelope" 82.200.000 gegen 2.200.000 sonst
# (Faktor 37), „Luminalia Gift" 95.500.000 gegen 5.500.000 (Faktor 17), beide
# am selben Terminal. Das sieht nach einer vorangestellten Ziffer aus. Alle
# übrigen 112 Waren bleiben unter Faktor 3 — die Grenze trennt also sauber,
# ohne echte Preisunterschiede wegzuwerfen.
OUTLIER_FACTOR = 3.0


def _without_outliers(rows):
    """Gebote ohne den einen Wert, der aus der Reihe fällt.

    ⚠ Erst ab drei Geboten. Bei zweien lässt sich nicht sagen, welches das
    falsche ist — und bei einem gibt es nichts zu vergleichen.
    """
    values = sorted((z.get('p') or 0.0) for z in rows)
    if len(values) < 3 or values[-2] <= 0:
        return rows
    if values[-1] / values[-2] < OUTLIER_FACTOR:
        return rows
    highest = values[-1]
    return [z for z in rows if (z.get('p') or 0.0) < highest]


def best_price(name, with_outliers=False):
    """Was die Ware höchstens bringt, je SCU — oder `0.0`.

    Für die schnelle Angabe im Handelslager, ohne die ganze Ortsliste.

    ⚠ **Ein einzelnes absurdes Gebot wird verworfen.** Sonst steht in der
    Bestenliste ein Preis, den es nicht gibt, und verdrängt die Waren, mit
    denen sich wirklich Geld verdienen lässt. `with_outliers=True` gibt den
    Rohwert zurück — für die Ortsliste, wo jedes Terminal zu sehen sein soll.
    """
    rows = ((load() or {}).get('waren') or {}).get(name) or []
    if not with_outliers:
        rows = _without_outliers(rows)
    return max((z['p'] for z in rows), default=0.0)
