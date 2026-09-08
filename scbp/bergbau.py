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
Wo welches Erz abzubauen ist.

Beantwortet **zwei** Fragen mit denselben Daten — beide sind echte Fälle:

  * „Ich brauche Iron, wo bekomme ich das?"  → 27 Orte
  * „Ich bin auf Daymar, was gibt es hier?"  → 14 Erze

⚠ **Das ist bewusst keine Kopie von scmdb.** Keine Wahrscheinlichkeits-Balken,
kein Refinery-Vergleich — wer es genau wissen will, ist auf scmdb.net besser
aufgehoben, und dorthin wird auch verwiesen. Der Wert hier ist, dass man das
Spiel nicht verlassen muss und aus dem Rezept direkt herspringt.

**Woher die Daten kommen**

`mining_data-<build>.json` von scmdb.net, 0,4 MB, einmal je Spiel-Build. Nichts
davon wird mitgeliefert (CC BY-NC-ND); die Nutzung ist von Krovax am 29.08.2026
freigegeben, die Weitergabe nicht.

**Die Kette durch die Daten** (drei Anläufe gekostet, deshalb hier festgehalten)

    locations[]                     50 Orte in Nyx, Pyro, Stanton
      groups[]                      FPS_Mineables · SpaceShip_Mineables ·
                                    SpaceShip_Mineables_Rare ·
                                    GroundVehicle_Mineables · (Salvage/Harvest)
        deposits[]
          compositionGuid   ─┐
                             ├─→ compositions[guid].parts[].elementName
                             ┘

⚠ **Nicht über `presetName` gehen.** Bei Erz-Vorkommen ist das Feld leer (364
mal); nur Wrackteile tragen dort einen Namen. Wer darüber verknüpft, bekommt
0 Treffer — genau so gemessen am 29.08.2026.

⚠ **Nur die Erz-Gruppen nehmen.** `Salvage_*` und `Harvestables` stehen in
derselben Liste, sind aber Wracks und Pflanzen.
"""
import json
import re
import os

from . import fehler, pfade
from .katalog import AUS, hole_datei
from .herstellung import norm_rohstoff
from .sprache import t

# Nur der Dateiname — siehe `katalog.hole_datei()`.
QUELLE = 'mining_data-%s.json'
CACHE = 'mining-data.json'
FORMAT = 1

# Das Geruest, wenn noch nichts geladen ist.
LEER = {'format': FORMAT, 'build': None, 'locations': [], 'compositions': {}}

# Welche Gruppe bedeutet welche Abbauart. Alles, was hier nicht steht, ist kein
# Erz (Wracks, Pflanzen) und wird übergangen.
ARTEN = {
    'FPS_Mineables':            'fps',
    'SpaceShip_Mineables':      'schiff',
    'SpaceShip_Mineables_Rare': 'schiff_selten',
    'GroundVehicle_Mineables':  'fahrzeug',
}




# ⚠⚠ **Die Daten bleiben im Speicher.**
#
# `laden()` las bis zum 29.08.2026 bei JEDEM Aufruf die ganze Datei von der
# Platte — bei den Rezepten sind das 4 MB und **22 ms**. Das fiel niemandem
# auf, solange nur beim Seitenaufbau geladen wurde. Mit dem Qualitäts-Regler
# wurde daraus ein Ladevorgang **pro Mausbewegung**: über 600 ms Rechenzeit je
# Sekunde, und der Regler ruckelte so, dass er unbenutzbar war.
#
# Gemerkt wird zusammen mit Zeitstempel und Größe der Datei. Ändert sich eine
# von beiden — etwa weil ein neuer Spiel-Build geladen wurde — wird neu
# gelesen. Damit bleibt der Zwischenspeicher richtig, ohne dass jemand ihn von
# Hand leeren muss.
_gemerkt = {'stand': None, 'daten': None}


def laden():
    """Der abgelegte Stand — aus dem Speicher, wenn die Datei unverändert ist."""
    pfad = pfade.app_datei(CACHE)
    try:
        st = os.stat(pfad)
        kennung = (st.st_mtime_ns, st.st_size)
    except OSError:
        kennung = None
    if kennung is not None and _gemerkt['stand'] == kennung:
        return _gemerkt['daten']
    try:
        with open(pfad, encoding='utf-8') as f:
            daten = json.load(f)
        if daten.get('format') == FORMAT:
            _gemerkt['stand'], _gemerkt['daten'] = kennung, daten
            return daten
    except Exception:
        pass
    return LEER.copy()

def stand():
    return laden().get('build')


def aktualisieren(build, fortschritt=None):
    """Die Bergbau-Daten holen, wenn sie fehlen oder veraltet sind."""
    if AUS:
        return False, t('m_h_kein_netz')
    da = laden()
    # ⚠ `refineries` fehlt in Ablagen von vor v3.3.0 — dort wurden beim Sichern
    # nur Orte und Zusammensetzungen behalten. Fehlt der Abschnitt, wird einmal
    # neu geholt; danach passiert wieder nichts. Die Datei ist 0,4 MB.
    if (da.get('build') == build and da.get('locations')
            and da.get('refineries') is not None
            and da.get('elemente') is not None):
        return True, t('m_b_aktuell') % len(da['locations'])
    if fortschritt:
        fortschritt(t('z_laedt') % ('Bergbau', 0.4))
    roh = hole_datei(QUELLE % build)
    orte = roh.get('locations') or []
    if not orte:
        return False, t('m_b_leer')
    # ⚠ Die Raffinerien gehoeren dazu. Sie stehen im selben Abruf und
    # beantworten die Frage, die nach „wo baue ich das ab?" kommt: „und wohin
    # bringe ich es?" 20 Raffinerien, 10 verschiedene Profile — bei Quartz
    # liegen zwischen der besten und der schlechtesten 14 Prozentpunkte.
    _sichern({'format': FORMAT, 'build': build, 'locations': orte,
              'compositions': roh.get('compositions') or {},
              'refineries': roh.get('refineries') or [],
              'refineryProfiles': roh.get('refineryProfiles') or {},
              # Die Stammdaten je Rohstoff — darin stehen Seltenheit und
              # Scan-Signatur, ohne die der Signatur-Rechner nichts kann.
              'elemente': roh.get('mineableElements') or {}})
    return True, t('m_b_geladen') % len(orte)


def _sichern(daten):
    ziel = pfade.app_datei(CACHE)
    try:
        os.makedirs(os.path.dirname(ziel), exist_ok=True)
        with open(ziel + '.tmp', 'w', encoding='utf-8') as f:
            json.dump(daten, f, ensure_ascii=False)
        os.replace(ziel + '.tmp', ziel)
        # ⚠ Zwischenspeicher verwerfen: Zeitstempel und Groesse koennen sich
        # binnen derselben Sekunde wiederholen, dann bliebe der alte Stand.
        _gemerkt['stand'] = None
        return True
    except Exception as ausnahme:
        fehler.merken('bergbau._sichern', ausnahme)
        return False


# ------------------------------------------------------------- Auswerten
#
# ⭐ **Wie viel von einem Erz liegt an einem Ort?** „Auf Daymar gibt es
# Aluminium" beantwortet die Frage nur halb — entscheidend ist, ob jeder
# zehnte Brocken Aluminium ist oder jeder hundertste. Die Zahl steckt in
# denselben Bergbaudaten und kostet **keinen zusätzlichen Abruf**:
#
#     groups[].groupProbability          wie oft diese Gruppe überhaupt kommt
#       deposits[].relativeProbability   Gewicht des Vorkommens in der Gruppe
#         compositions[guid].parts[]
#           probability · minPercent · maxPercent
#
# Das Gewicht eines Erzes an einem Ort ist also
#
#     groupProbability × (relativeProbability ÷ Summe der Gruppe)
#                      × probability × Mittel(minPercent, maxPercent)
#
# und der Anteil dieses Gewichts an der Summe aller Erze am Ort ist die
# **Konzentration**: Welcher Bruchteil der Brocken hier dieses Erz ist.
#
# ⚠ **Der Prozentsatz gehört in die Rechnung.** Ohne ihn (nur über
# `probability`) kam für Beryl am Aaron Halo 12,7 % heraus; mit ihm 18,4 %.
# Gegengerechnet gegen die Anzeige von strata.celd.space, die dieselben
# scmdb-Daten auswertet: dort steht 18,0 % — die Restabweichung kommt daher,
# dass celd den Halo in Segmente zerlegt und wir ihn als einen Ort führen.
# Aslarite 13,9 zu 14,2 · Copper 9,0 zu 10,3. (Gemessen 08.09.2026.)
#
# ⚠ **Es ist ein Anteil, keine Fördermenge.** Ein kleiner Fleck, an dem fast
# nur Titanium liegt, steht damit genauso gut da wie ein riesiges Feld mit
# demselben Anteil. Wie groß der Ort ist, sagt die Zahl **nicht**.

# Die sechs Stufen auf denselben Anteil — feste Bänder, keine Rangfolge unter
# den Orten. Damit heißt „viel" an jedem Ort dasselbe. In Prozent, weil die
# Stufe zu der Zahl passen muss, die danebensteht (siehe `stufe()`).
#
# ⚠ **Höher angesetzt als bei strata.celd.space** (dort 25/15/8/4/1). Die
# rechnen über alles am Ort, wir je Abbauart — dadurch liegen unsere Anteile
# durchweg höher, und mit ihren Schwellen stand auf Daymar siebenmal
# „fast nur das" untereinander: 59, 48, 40, 35, 33, 31, 26 Prozent, alle
# gleich benannt. Eine Stufe, die für fast jede Zeile dasselbe sagt, sagt
# nichts. (Gemessen 08.09.2026 am fertigen Bild.)
STUFEN = ((50, 6), (30, 5), (15, 4), (7, 3), (2, 2))


def stufe(anteil):
    """1 bis 6 — von „kaum etwas" bis „fast nur das".

    ⚠ **Gerechnet wird auf der gerundeten Prozentzahl**, nicht auf dem
    Rohwert. Sonst steht in der Liste zweimal „25 %" untereinander, einmal
    mit „sehr viel" und einmal mit „viel" daneben — Titanium liegt am Yela-
    Gürtel bei 0,2499 und an Lagrange E bei 0,2520. Wer das sieht, hält das
    Programm für kaputt, und mit Recht: Was gleich aussieht, muss gleich
    heißen.
    """
    prozent = round((anteil or 0.0) * 100)
    for schwelle, wert in STUFEN:
        if prozent >= schwelle:
            return wert
    return 1


def _topf(art):
    """Womit man hinfährt — `schiff_selten` ist derselbe Prospector."""
    return 'schiff' if art.startswith('schiff') else art


def _am_ort(ort, compositions):
    """Was an einem Ort liegt — und zu welchem Anteil.

    Gibt `(arten, anteile, je_geraet)`:

    | Feld | Inhalt |
    |---|---|
    | `arten` | `{Erzname: {Abbauart, …}}` |
    | `anteile` | `{Erzname: Anteil 0..1}` — der höchste über alle Geräte |
    | `je_geraet` | `{Gerät: {Erzname: Anteil}}` — getrennt, wie gerechnet |

    ⚠ **`je_geraet` ist die ehrliche Fassung.** Nur innerhalb eines Geräts
    sind die Zahlen vergleichbar; `anteile` ist die Kurzform für Stellen, die
    ohnehin nur eine Zahl je Zeile zeigen können.

    ⚠⚠ **Je Abbauart getrennt gerechnet.** In einem Topf standen auf Daymar
    34 % Aphorite über 4 % Quartz — und das ist für jeden falsch, der die
    Zahl liest: Aphorite holt man mit dem Multi-Tool aus einer Höhle, Quartz
    mit dem Prospector aus einem Felsen. Wer mit dem Schiff kommt, sieht die
    Handabbau-Vorkommen nie. Deshalb wird je Topf (`fps`, `fahrzeug`,
    `schiff`) auf 100 % normiert; die Zahl heißt damit „so viel von dem, was
    du mit **diesem** Gerät hier abbaust".

    Kommt ein Erz in mehreren Töpfen vor (Carinite: Hand und Fahrzeug), zählt
    der höhere Anteil — sonst stünde an derselben Zeile zweimal etwas anderes.
    """
    arten, gewicht = {}, {}
    for g in ort.get('groups') or []:
        art = ARTEN.get(g.get('groupName'))
        if not art:
            continue
        # ⚠ Fehlt die Angabe, zählt die Gruppe voll — sonst fiele ein ganzer
        # Ort auf 0 und stünde ohne Erze da.
        gruppe = g.get('groupProbability')
        gruppe = 1.0 if gruppe is None else gruppe
        vorkommen = g.get('deposits') or []
        # Die `relativeProbability` ist **relativ innerhalb der Gruppe** —
        # erst durch die Summe geteilt wird daraus ein Anteil.
        summe = sum(d.get('relativeProbability') or 0.0 for d in vorkommen)
        for d in vorkommen:
            c = compositions.get(d.get('compositionGuid'))
            if not c:
                continue
            anteil_vorkommen = ((d.get('relativeProbability') or 0.0) / summe
                                if summe else 1.0 / max(len(vorkommen), 1))
            for teil in c.get('parts') or []:
                name = teil.get('elementName')
                if not name:
                    continue
                arten.setdefault(name, set()).add(art)
                mitte = ((teil.get('minPercent') or 0.0)
                         + (teil.get('maxPercent') or 0.0)) / 2.0
                wahrscheinlich = teil.get('probability')
                wahrscheinlich = (1.0 if wahrscheinlich is None
                                  else wahrscheinlich)
                schluessel = (_topf(art), name)
                gewicht[schluessel] = (gewicht.get(schluessel, 0.0)
                                       + gruppe * anteil_vorkommen
                                       * wahrscheinlich * mitte)
    # Je Topf auf 100 % normieren, dann je Erz den höheren Wert behalten.
    summen = {}
    for (topf, _n), w in gewicht.items():
        summen[topf] = summen.get(topf, 0.0) + w
    anteile = {n: 0.0 for n in arten}
    je_geraet = {}
    for (topf, name), w in gewicht.items():
        gesamt = summen.get(topf) or 0.0
        wert = w / gesamt if gesamt else 0.0
        je_geraet.setdefault(topf, {})[name] = wert
        anteile[name] = max(anteile.get(name, 0.0), wert)
    return arten, anteile, je_geraet


def _erze_am_ort(ort, compositions):
    """{Erzname: {Abbauart, …}} für einen Ort."""
    return _am_ort(ort, compositions)[0]


def orte():
    """Alle Orte mit Erzen.

    `[{name, system, typ, erze:{name:{art}}, anteile:{name:(Anteil, Stufe)}}]`

    ⚠ `anteile` steht **neben** `erze`, nicht darin: An `erze` hängen der
    Selbsttest und die Lager-Abbauart, und beides soll von der Konzentration
    nichts wissen müssen.
    """
    daten = laden()
    comp = daten.get('compositions') or {}
    raus = []
    for o in daten.get('locations') or []:
        erze, anteile, je_geraet = _am_ort(o, comp)
        if not erze:
            continue
        raus.append({'name': o.get('locationName') or '?',
                     'system': o.get('system') or '',
                     'typ': o.get('locationType') or '',
                     'erze': erze,
                     'anteile': {n: (a, stufe(a))
                                 for n, a in anteile.items()},
                     'je_geraet': {g: {n: (a, stufe(a)) for n, a in werte.items()}
                                   for g, werte in je_geraet.items()}})
    raus.sort(key=lambda x: x['name'].lower())
    return raus


def abbauart(name):
    """Wie wird dieser Rohstoff abgebaut? — Menge aus `fps`, `fahrzeug`, `schiff`.

    ⚠ Gebraucht im Lager: Wer „Iron" einträgt, will auf einen Blick sehen, ob
    er dafür mit dem Multi-Tool loszieht oder ein Schiff braucht. Die Angabe
    steckt in den Bergbaudaten an jedem Fundort; hier werden sie über alle Orte
    des Rohstoffs zusammengefasst.

    `schiff_selten` zählt als `schiff` — für die Frage „womit hole ich das?"
    macht die Seltenheit keinen Unterschied.
    """
    gesucht = norm_rohstoff(name)
    arten = set()
    for e in erze():
        if norm_rohstoff(e.get('name')) != gesucht:
            continue
        for eintrag in e.get('orte') or []:
            for art in (eintrag[2] if len(eintrag) > 2 else ()):
                arten.add('schiff' if art.startswith('schiff') else art)
    return arten


def erze():
    """Alle Erze: `[{name, orte:[(Ort, System, {Art}, Anteil, Stufe)]}]`.

    Die Gegenrichtung zu `orte()`.

    ⚠ **Die Fundorte stehen nach Konzentration, nicht alphabetisch.** Wer
    fragt „wo hole ich Titanium?", will den ergiebigsten Ort zuerst sehen —
    eine alphabetische Liste beantwortet die Frage nicht, sie zeigt nur alle.

    ⚠ Die beiden hinteren Felder kamen später dazu. Wer die Liste auswertet,
    entpackt sie deshalb nachgiebig (`eintrag[3] if len(eintrag) > 3`) —
    `abbauart()` unten macht es genauso.
    """
    sammlung = {}
    for o in orte():
        je_geraet = o.get('je_geraet') or {}
        for name, arten in o['erze'].items():
            anteil, hoehe = (o.get('anteile') or {}).get(name, (0.0, 1))
            # Sechstes Feld: je Gerät `(Anteil, Stufe, wie viele Erze dieses
            # Gerät hier überhaupt findet)`. Die letzte Zahl trägt die
            # Aussage „das ist hier das einzige" — siehe `orte()`.
            fein = {}
            for geraet, werte in je_geraet.items():
                if name in werte:
                    fein[geraet] = werte[name] + (len(werte),)
            sammlung.setdefault(name, []).append(
                (o['name'], o['system'], arten, anteil, hoehe, fein))
    # ⚠ **Kein blankes `sorted()`.** Sobald zwei Einträge in Ort und System
    # übereinstimmen, verglich Python die Mengen dahinter — und Mengen haben
    # keine Reihenfolge. Deshalb ausdrücklich nur über die Zahlen sortieren.
    raus = [{'name': n, 'orte': sorted(v, key=lambda x: (-x[3], x[0].lower()))}
            for n, v in sammlung.items()]
    raus.sort(key=lambda x: x['name'].lower())
    return raus


# Wie oft ein Vorkommen höchstens auftritt — das begrenzt, welche Vielfachen
# der Signatur überhaupt vorkommen können. Steht als `rarity` an jedem
# Rohstoff.
MAX_BROCKEN = {'legendary': 2, 'epic': 3, 'rare': 4, 'uncommon': 5,
               'common': 6}

# Grundsignaturen für Vorkommen ohne eigenen Wert. ⚠ `roc` und `fps` stehen so
# in den Daten (`groundScanSignature` 4000, `fpsScanSignature` 3000).
# `salvage` steht dort **nicht** — der Wert stammt aus der Tabelle auf
# scmdb.net. Wenn er je falsch ist, ist er dort genauso falsch.
GRUND_SIGNATUR = (('roc', 4000, 7), ('fps', 3000, 10), ('salvage', 2000, 15))


# ⚠⚠ Das Spiel zeigt die Signatur als `17,200` — mit Tausenderkomma. Bis zum
# 08.09.2026 machte ein schlichtes `replace(',', '.')` daraus **17,2**, Faktor
# tausend daneben und ohne eine Zeile Fehlermeldung: Wer genau abschrieb, was
# im HUD stand, bekam Unsinn vorgesetzt.
#
# ⚠ Die Regel wohnt bewusst in `rohstoffe` und nicht hier — dort steht mit
# `zahl_lesen` seit jeher alles, was eine getippte Zahl entgegennimmt, und zwei
# Fassungen derselben Regel liefen garantiert auseinander. Der Unterschied
# steckt allein im Schalter: Hier gilt `ganzzahlig=True`, weil Signaturen ganze
# Zahlen im Tausenderbereich sind (`8,600` meint 8600, nie 8,6). Bei Mengen ist
# es umgekehrt, dort sind Kommazahlen der Regelfall.
def _zahltext(roh):
    """Abgelesene oder getippte Signatur auf die Punkt-Schreibweise bringen."""
    from .rohstoffe import trennzeichen_klaeren
    return trennzeichen_klaeren(roh, ganzzahlig=True)


def signatur_suchen(eingabe):
    """Aus einem gescannten Wert den Rohstoff bestimmen.

    ⭐ **Das Werkzeug, das ein Miner im Spiel wirklich braucht.** Der Scanner
    zeigt eine Zahl; welcher Brocken dahintersteckt, sagt er nicht. Die Zahl
    ist die Signatur des Rohstoffs mal der Anzahl der Brocken im Vorkommen —
    wie oft, begrenzt die Seltenheit (legendär höchstens 2, verbreitet 6).

    Eingabeformen, wie bei scmdb:

    | Eingabe | Bedeutung |
    |---|---|
    | `8600` | genau dieser Wert |
    | `~5000` | ±10 % Spielraum |
    | `4000-9000` | alles dazwischen |
    | `17,200` / `17.200` | genau wie im HUD abgeschrieben — 17200 |

    Gibt `[(Name, Anzahl Brocken, Signatur, Abweichung in Prozent)]`, die
    genaueste Übereinstimmung zuerst.

    ⚠ **Ohne Toleranz wird nichts gerundet.** Wer `8600` eingibt und nichts
    trifft, soll das erfahren und `~8600` versuchen — nicht einen Treffer
    vorgesetzt bekommen, der um 300 danebenliegt.
    """
    text = _zahltext(eingabe)
    if not text:
        return []
    toleranz, unten, oben = 0.0, None, None
    try:
        if text.startswith('~'):
            wert = float(text[1:])
            toleranz = 0.10
            unten, oben = wert * 0.9, wert * 1.1
        elif '-' in text[1:]:
            a, b = text.split('-', 1) if not text.startswith('-') else (None, None)
            unten, oben = sorted((float(a), float(b)))
            wert = (unten + oben) / 2.0
        else:
            wert = float(text)
            unten = oben = wert
    except (ValueError, TypeError):
        return []
    if unten is None:
        return []

    da = laden()
    elemente = da.get('elemente') or {}
    treffer = []
    for _g, e in elemente.items():
        sig = e.get('scanSignature')
        if not sig:
            continue
        hoechstens = MAX_BROCKEN.get(e.get('rarity'), 6)
        for anzahl in range(1, hoechstens + 1):
            gesamt = sig * anzahl
            if unten <= gesamt <= oben:
                ab = (gesamt - wert) / wert * 100.0 if wert else 0.0
                treffer.append((e.get('name') or '?', anzahl, gesamt, ab))
    # Und die pauschalen Vorkommen ohne eigenen Rohstoff.
    for name, sig, hoechstens in GRUND_SIGNATUR:
        for anzahl in range(1, hoechstens + 1):
            gesamt = sig * anzahl
            if unten <= gesamt <= oben:
                ab = (gesamt - wert) / wert * 100.0 if wert else 0.0
                treffer.append((name, anzahl, gesamt, ab))
    treffer.sort(key=lambda x: (abs(x[3]), x[0]))
    return treffer


def pflanzen():
    """Die Pflanzen, die man von Hand ernten kann — mit lesbarem Namen.

    ⚠ Sie stehen **nicht** bei den Mineralien (`mineableElements`), sondern als
    Vorkommen mit `presetName` an den Fundorten, in Gruppen namens
    `Harvestables`. Der Watcher hat sie deshalb nie gekannt: Er las nur
    Vorkommen mit `compositionGuid` und ueberging alle anderen — 661 von 1.025.

    Die Namen stehen dort zusammengeschrieben (`Plant HeartoftheWoods`); hier
    werden sie auseinandergenommen zu „Heart of the Woods", so wie das Spiel
    sie zeigt.

    Gibt eine alphabetische Liste.
    """
    da = laden()
    raus = set()
    for o in da.get('locations') or []:
        for g in o.get('groups') or []:
            if g.get('groupName') != 'Harvestables':
                continue
            for dep in g.get('deposits') or []:
                name = (dep.get('presetName') or '').strip()
                if not name.startswith('Plant '):
                    continue
                raus.add(_lesbar(name[len('Plant '):]))
    return sorted(raus, key=str.lower)


def _lesbar(zusammen):
    """`HeartoftheWoods` wird zu `Heart of the Woods`.

    ⚠ Die kleinen Bindewoerter stehen in den Daten klein und mitten im Wort;
    sie an jedem Grossbuchstaben zu trennen ergaebe „Heart of the Woods" nur
    zufaellig richtig. Deshalb erst die bekannten Woerter herausloesen, dann an
    Grossbuchstaben trennen.
    """
    # ⚠ Das Bindewort darf auch von einem KLEINBUCHSTABEN gefolgt sein.
    # „HeartoftheWoods" ist genau so gebaut: auf „of" folgt „the". Mit
    # `(?=[A-Z])` blieb daraus „Heartof the Woods".
    text = re.sub(r'(?<=[a-z])(of|the|and)(?=[a-zA-Z])', r' \1 ', zusammen)
    text = re.sub(r'(?<=[a-z])(?=[A-Z])', ' ', text)
    return ' '.join(text.split())


def raffinerien_fuer(rohstoff):
    """Welche Raffinerie holt aus diesem Erz am meisten heraus?

    Gibt `[(Namen, System, Bonus in Prozent)]`, beste zuerst. Raffinerien mit
    demselben Profil werden zusammengefasst — bei zehn Profilen auf zwanzig
    Stationen stuenden sonst Dubletten da.

    ⚠ **Was nicht im Profil steht, ist 0 %**, nicht „unbekannt". So haelt es
    die Quelle, und so steht es auch in deren Tabelle.

    ⚠ Verglichen wird ueber `norm_rohstoff` — die Profile sagen
    `Aluminum (Ore)`, die Rezepte `Aluminium`, die Bergbaudaten
    `Aluminium (Ore)`. Ohne Angleichung findet man zu keinem Erz eine
    Raffinerie.
    """
    from .herstellung import norm_rohstoff
    da = laden()
    profile = da.get('refineryProfiles') or {}
    if not profile:
        return []
    gesucht = norm_rohstoff(rohstoff)
    # Erst je Profil den Bonus bestimmen ...
    bonus_je_profil = {}
    for pid, werte in profile.items():
        bonus_je_profil[pid] = 0
        for mat, wert in (werte or {}).items():
            if norm_rohstoff(mat) == gesucht:
                bonus_je_profil[pid] = wert
                break
    # ... dann die Stationen dazu buendeln.
    gebuendelt = {}
    for r in da.get('refineries') or []:
        pid = r.get('profileId')
        if pid not in bonus_je_profil:
            continue
        eintrag = gebuendelt.setdefault(pid, {'namen': [], 'system': r.get('system'),
                                              'bonus': bonus_je_profil[pid]})
        eintrag['namen'].append(r.get('name') or '')
    raus = [(e['namen'], e['system'], e['bonus']) for e in gebuendelt.values()]
    raus.sort(key=lambda x: (-x[2], x[0][0] if x[0] else ''))
    return raus


def orte_fuer(rohstoff):
    """Wo gibt es diesen Rohstoff? Verträgt beide Schreibweisen.

    ⚠ Die Baupläne sagen `Aslarite`, hier heißt es `Aslarite (Raw)` — deshalb
    über `norm_rohstoff()` vergleichen. Ohne das findet der Sprung aus dem
    Rezept **nichts** (gemessen: 0 von 26)."""
    gesucht = norm_rohstoff(rohstoff)
    for e in erze():
        if norm_rohstoff(e['name']) == gesucht:
            return e
    return None
