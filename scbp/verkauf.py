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

Das Gegenstück zu `preise.py`. Dort geht es um **kaufen oder abbauen**, hier um
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

Die Ortsnamen kommen aus `orte.py`, das dieselbe Terminal-Liste ohnehin holt —
so wird die fremde Schnittstelle nicht zweimal für dasselbe angefasst.

⚠ **Die Daten werden NICHT mitgeliefert**, sondern auf dem Rechner des Nutzers
geholt — dieselbe Regel wie bei scmdb, `preise.py` und `orte.py`. Und
**höchstens einmal am Tag**.

⚠ **Ohne Netz passiert nichts Schlimmes.** Liegt eine alte Ablage da, wird sie
benutzt; liegt keine da, bleibt der Reiter leer und sagt das auch. Kein Fehler,
kein Absturz.

## ⚠⚠ Zwei Fallen, die beim Bauen zugeschnappt sind

**1. Der Namensfilter von UEX sucht Teiltexte.** `commodity_name=Gold` liefert
`Golden Medmon` gleich mit — und dessen 71.000 aUEC/SCU sahen aus wie ein
sagenhafter Goldpreis, während Gold tatsächlich bei 33.000 liegt. Deshalb wird
hier **ausschliesslich exakt** verglichen, nie mit `in` oder `startswith`.

**2. `norm_rohstoff()` darf hier NICHT benutzt werden.** Die Funktion aus
`herstellung.py` schneidet die Klammer ab, damit die Bergbau-Sicht zu
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

from . import orte, uex
from .katalog import AUS

QUELLE = 'https://api.uexcorp.uk/2.0/commodities_prices_all'
CACHE = 'verkauf.json'
# ⚠ Auf 2 gesetzt, als der Füllstand (`z`) dazukam, auf 3 mit dem Terminalnamen
# (`n`). Eine alte Ablage hätte die Felder nicht — ein höherer Formatstand holt
# sie einmal neu, statt die Anzeige einen Tag lang lückenhaft zu lassen. Ein
# Abruf mehr, dafür sofort vollständig.
FORMAT = 3
ZEITLIMIT = 30

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
FUELLT_SICH = 5
KEIN_BEDARF = 6

# Ein Tag. Preise ändern sich im Spiel laufend, aber nicht im Minutentakt —
# dieselbe Überlegung wie in `preise.py`.
HALTBAR = uex.TAG

# Ab wann eine Meldung als alt gilt und in der Anzeige abgesetzt wird.
# Gemessen am 30.08.2026 über alle 1.880 Ankauf-Einträge: 98,5 % waren jünger
# als eine Woche, die Hälfte jünger als 2,2 Tage, der älteste 15 Tage. Eine
# Woche trennt also sauber zwischen „normal" und „schau genau hin".
ALT = 7 * 24 * 60 * 60

# ⚠ **Auch ein Fehlversuch bremst — aber nur kurz.**
#
# Scheitert der Abruf, wird nichts abgelegt; die Stundensperre greift also
# nicht, denn sie rechnet aus dem letzten **erfolgreichen** Abruf. Ohne diese
# zweite, kurze Bremse könnte jemand bei kaputter Leitung im Sekundentakt
# drücken und jedes Mal eine Anfrage losschicken.
#
# Eine Minute, nicht eine Stunde: Wer sein Netz repariert, soll es sofort
# wieder versuchen dürfen und nicht für einen fremden Ausfall bestraft werden.
FEHLERSPERRE = 60

# Abruf und Ablage liegen im gemeinsamen Unterbau — siehe `scbp/uex.py`.
_ablage = uex.Ablage(CACHE, format_nr=FORMAT, haltbar=HALTBAR)

# Wann zuletzt vergeblich angefragt wurde. Bewusst nur im Arbeitsspeicher: Nach
# einem Neustart des Werkzeugs darf man es sofort wieder versuchen.
_letzter_fehlversuch = {'zeit': 0.0}

# ⭐ **Der Knopf „Jetzt aktualisieren" ist auf einmal pro Stunde begrenzt.**
#
# Wer gerade landet und wissen will, was das Terminal heute zahlt, soll nicht
# bis morgen warten müssen — der tägliche Abruf allein reicht dafür nicht.
# Ohne Sperre wäre der Knopf aber eine Einladung, im Minutentakt zu drücken,
# und ein Werkzeug, das eine fremde Schnittstelle so anfasst, ist ein
# schlechter Gast (dieselbe Überlegung wie in `preise.py`).
#
# Eine Stunde ist der Kompromiss: oft genug für einen Handelsflug, selten genug,
# dass 100 Nutzer zusammen keine Last erzeugen.
SPERRE = 60 * 60

# Zweite Quelle: die Terminal-Liste. Sie liefert System, Ort und das Kennzeichen
# `is_nqa`, das in den Preisdaten fehlt.
#
# ⚠ `orte.py` holt dieselbe Liste — dort aber nur wöchentlich und nur für die
# Ortsnamen. Bewusst **nicht** gekoppelt: Ein Modul, das sich seine Daten selbst
# besorgt, lässt sich einzeln prüfen und geht nicht kaputt, wenn am anderen
# etwas geändert wird. Der Preis dafür ist ein zusätzlicher Abruf pro Tag.
QUELLE_TERMINALS = 'https://api.uexcorp.uk/2.0/terminals'


def laden():
    """Der abgelegte Stand — aus dem Speicher, wenn die Datei unverändert ist."""
    return _ablage.laden()


def alter():
    """Wie alt die Ablage ist, in Sekunden — oder None, wenn keine da ist."""
    return _ablage.alter()


def wartezeit():
    """Wie viele Sekunden der Knopf „Jetzt aktualisieren" noch gesperrt ist.

    `0` heisst: darf sofort. Siehe `SPERRE`.
    """
    seit_fehler = time.time() - _letzter_fehlversuch['zeit']
    rest_fehler = max(0, int(FEHLERSPERRE - seit_fehler))
    a = alter()
    if a is None:
        return rest_fehler
    return max(rest_fehler, int(SPERRE - a) if a < SPERRE else 0)


def aktualisieren(erzwingen=False, fortschritt=None):
    """Die Verkaufspreise holen.

    Ohne `erzwingen` passiert nur etwas, wenn die Ablage fehlt oder älter als
    ein Tag ist — der stille Abruf im Hintergrund.

    Mit `erzwingen=True` ist es der Knopf aus dem Reiter. Der darf höchstens
    einmal pro Stunde (siehe `SPERRE`); ist er noch gesperrt, kommt
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
    if AUS:
        return False, 'aus'
    if erzwingen:
        if wartezeit():
            return False, 'gesperrt'
    elif not _ablage.veraltet():
        return True, ''
    if fortschritt:
        fortschritt('')
    preise = uex.holen(QUELLE, 'verkauf', zeitlimit=ZEITLIMIT)
    if preise is None:
        _letzter_fehlversuch['zeit'] = time.time()
        return False, 'netz'
    if not preise:
        _letzter_fehlversuch['zeit'] = time.time()
        return False, 'leer'
    # Die Terminal-Liste darf fehlschlagen, ohne dass alles scheitert: Ohne sie
    # kennen wir System und `is_nqa` nicht, aber der Terminal-Name steht in den
    # Preisdaten selbst. Lieber eine Liste ohne Systemspalte als gar keine.
    stellen = uex.holen(QUELLE_TERMINALS, 'verkauf.terminals',
                        zeitlimit=ZEITLIMIT) or []

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
    # Reiter, was er **weiss**: wie alt die Meldungen sind (`alter()`). Eine
    # Versionsnummer, die man nicht belegen kann, ist schlimmer als keine.
    terminals = {}
    for x in stellen:
        kennung = x.get('id')
        if kennung is None:
            continue
        ort = (x.get('space_station_name') or x.get('city_name')
               or x.get('outpost_name') or x.get('planet_name') or '')
        terminals[str(kennung)] = {
            'o': ort,
            's': x.get('star_system_name') or '',
            'q': 1 if x.get('is_nqa') else 0,
            # ⚠⚠ **Der Terminalname gehört dazu.** Ohne ihn standen im
            # Routen-Reiter acht Zeilen „Seraphim Station · Stanton"
            # untereinander — eine Station hat viele Terminals (Admin, TDD,
            # Läden), und die verkaufen Verschiedenes. Wer auswählen soll,
            # muss unterscheiden können. Gemeldet am 04.09.2026.
            'n': (x.get('name') or '').strip(),
        }

    # ⚠⚠ **Nur Zeilen mit echtem Ankaufgebot behalten.** `price_sell = 0` heisst
    # „dieses Terminal nimmt die Ware nicht", nicht „es zahlt nichts". Wer die
    # Zeilen mitschleppt, hat 705 Einträge mehr in der Ablage und muss an jeder
    # Stelle daran denken, sie wegzufiltern — einmal vergessen, und im Reiter
    # steht ein Ort mit „0 aUEC" ganz unten in der Liste.
    #
    # ⚠ Der Warenname wird **unverändert** übernommen, mit Klammer und allem.
    # Siehe die zweite Falle im Kopf: `Copper` und `Copper (Ore)` sind zwei
    # verschiedene Waren, und `norm_rohstoff()` würde sie zusammenwerfen.
    waren = {}
    for x in preise:
        preis = float(x.get('price_sell') or 0)
        if preis <= 0:
            continue
        name = (x.get('commodity_name') or '').strip()
        if not name:
            continue
        waren.setdefault(name, []).append({
            't': str(x.get('id_terminal')),
            'n': (x.get('terminal_name') or '').strip(),
            'p': preis,
            'd': int(x.get('date_modified') or 0),
            'k': x.get('container_sizes') or '',
            # ⭐ Wie voll das Lager dort ist, in UEX' eigenen sieben Stufen.
            # Beim **Verkauf** ist voll das Schlechte: Ein randvolles Terminal
            # hat keinen Bedarf mehr und nimmt die Ladung nicht.
            'z': int(x.get('status_sell') or 0),
        })
    if not waren:
        return False, 'leer'
    for zeilen in waren.values():
        zeilen.sort(key=lambda z: -z['p'])
    # ⚠ `kompakt`: Diese Ablage ist mit rund 75 KB die grösste der drei —
    # ohne Leerzeichen zwischen den Feldern spart das spürbar Platz.
    _ablage.sichern({'terminals': terminals, 'waren': waren}, kompakt=True)
    return True, ''


def fuellstand(zeile):
    """Was der Füllstand einer Verkaufsstelle bedeutet — oder `None`.

    Gibt `(schluessel, ist_warnung)` zurück: den Sprachschlüssel für den Text
    und ob es eine Warnung ist (rot) oder ein Hinweis (gold).

    ⚠ **`None` heisst „nichts sagen"** — nicht „alles in Ordnung". Beides
    sieht in der Anzeige gleich aus, und das ist Absicht: Der Normalfall
    braucht kein Zeichen. Siehe `FUELLT_SICH` oben.

    ⚠ Ältere Ablagen kennen das Feld nicht (`z` fehlt). Dann wird ebenfalls
    geschwiegen — eine Warnung aus fehlenden Daten wäre geraten.
    """
    stufe = (zeile or {}).get('z') or 0
    if stufe >= KEIN_BEDARF:
        return 's_vk_voll', True
    if stufe == FUELLT_SICH:
        return 's_vk_fuellt', False
    return None


def waren():
    """Alle Waren mit mindestens einem Ankaufgebot, alphabetisch.

    Rund 150 Namen, genau so geschrieben wie bei UEX — `Copper` und
    `Copper (Ore)` stehen beide darin und sind **nicht** dasselbe.
    """
    return sorted((laden() or {}).get('waren') or {})


def bekannt(name):
    """Kennt die Ablage diese Ware? Exakter Vergleich, siehe Falle 1 im Kopf."""
    return name in ((laden() or {}).get('waren') or {})


def orte_fuer(namen, nur_nqa=False):
    """Wo man die genannten Waren los wird — die beste Stelle zuerst.

    `namen` ist eine Liste von Warennamen, wie sie `waren()` liefert.

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
    einen echten Erlös braucht es das Handelslager (`handelslager.py`).

    `nur_nqa=True` blendet auf die Stellen ein, die keine Fragen stellen —
    für als gestohlen markierte Ladung.
    """
    daten = laden() or {}
    alle = daten.get('waren') or {}
    stellen = daten.get('terminals') or {}
    gesucht = [n for n in namen if n in alle]
    if not gesucht:
        return []

    jetzt = time.time()
    gesammelt = {}
    for ware in gesucht:
        for zeile in alle[ware]:
            kennung = zeile['t']
            stelle = stellen.get(kennung) or {}
            if nur_nqa and not stelle.get('q'):
                continue
            eintrag = gesammelt.setdefault(kennung, {
                'terminal': zeile.get('n') or '?',
                'ort': stelle.get('o') or '',
                'system': stelle.get('s') or '',
                'nqa': bool(stelle.get('q')),
                'treffer': [],
            })
            eintrag['treffer'].append({
                'ware': ware,
                'preis': zeile['p'],
                'kisten': zeile.get('k') or '',
                # Alter in Sekunden. `None`, wenn die Meldung kein Datum hat —
                # dann wird in der Anzeige nichts behauptet.
                'alter': (jetzt - zeile['d']) if zeile.get('d') else None,
                # ⚠ Der Füllstand gehört an die **Ware**, nicht an den Ort:
                # Dasselbe Terminal kann bei Gold randvoll und bei Iron leer
                # sein. Ein Zeichen am Ort wäre für die halbe Ladung falsch.
                'fuellstand': fuellstand(zeile),
            })

    ergebnis = []
    for eintrag in gesammelt.values():
        eintrag['treffer'].sort(key=lambda tr: -tr['preis'])
        eintrag['anzahl'] = len(eintrag['treffer'])
        eintrag['summe'] = sum(tr['preis'] for tr in eintrag['treffer'])
        # Das Alter des Ortes ist das der **ältesten** Meldung, die ihn stützt.
        # Die vorsichtigere Angabe: Wer drei Waren dort verkaufen will, verlässt
        # sich auf alle drei Meldungen, nicht nur auf die frischeste.
        alter_werte = [tr['alter'] for tr in eintrag['treffer']
                       if tr['alter'] is not None]
        eintrag['alter'] = max(alter_werte) if alter_werte else None
        ergebnis.append(eintrag)

    ergebnis.sort(key=lambda e: (-e['anzahl'], -e['summe']))
    return ergebnis


def bester_preis(name):
    """Was die Ware höchstens bringt, je SCU — oder `0.0`.

    Für die schnelle Angabe im Handelslager, ohne die ganze Ortsliste.
    """
    zeilen = ((laden() or {}).get('waren') or {}).get(name) or []
    return max((z['p'] for z in zeilen), default=0.0)
