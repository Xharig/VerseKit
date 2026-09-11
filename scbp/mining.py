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

⚠ Bis zum 11.09.2026 hieß dieses Modul `bergbau` (Sprachumstellung P4,
Stufe 2). **Nur Bezeichner sind umbenannt, keine Zeichenketten.** Bewusst
gleich geblieben: der Dateiname `mining-data.json` und die Schlüssel darin
(`format`, `build`, `locations`, `compositions`, `refineries`,
`refineryProfiles`, `elemente`) — sonst wird jede vorhandene Ablage als
veraltet verworfen und neu geholt. Ebenso die Abbauarten `fps`, `schiff`,
`schiff_selten`, `fahrzeug`, die Schlüssel der Ergebnisse von `locations()`
(`name`, `system`, `typ`, `erze`, `anteile`, `je_geraet`) und der Seitenname
`bergbau` in Reiterleiste, Symbolsatz und „Neu"-Marken.
"""
import json
import re
import os

from . import fehler, pfade
from .katalog import AUS, hole_datei
from .herstellung import norm_rohstoff
from .sprache import t

# Nur der Dateiname — siehe `katalog.hole_datei()`.
SOURCE = 'mining_data-%s.json'
CACHE = 'mining-data.json'
FORMAT = 1

# Das Geruest, wenn noch nichts geladen ist.
EMPTY = {'format': FORMAT, 'build': None, 'locations': [], 'compositions': {}}

# Welche Gruppe bedeutet welche Abbauart. Alles, was hier nicht steht, ist kein
# Erz (Wracks, Pflanzen) und wird übergangen.
KINDS = {
    'FPS_Mineables':            'fps',
    'SpaceShip_Mineables':      'schiff',
    'SpaceShip_Mineables_Rare': 'schiff_selten',
    'GroundVehicle_Mineables':  'fahrzeug',
}




# ⚠⚠ **Die Daten bleiben im Speicher.**
#
# `load()` las bis zum 29.08.2026 bei JEDEM Aufruf die ganze Datei von der
# Platte — bei den Rezepten sind das 4 MB und **22 ms**. Das fiel niemandem
# auf, solange nur beim Seitenaufbau geladen wurde. Mit dem Qualitäts-Regler
# wurde daraus ein Ladevorgang **pro Mausbewegung**: über 600 ms Rechenzeit je
# Sekunde, und der Regler ruckelte so, dass er unbenutzbar war.
#
# Gemerkt wird zusammen mit Zeitstempel und Größe der Datei. Ändert sich eine
# von beiden — etwa weil ein neuer Spiel-Build geladen wurde — wird neu
# gelesen. Damit bleibt der Zwischenspeicher richtig, ohne dass jemand ihn von
# Hand leeren muss.
_cached = {'stand': None, 'daten': None}


def load():
    """Der abgelegte Stand — aus dem Speicher, wenn die Datei unverändert ist."""
    path = pfade.app_datei(CACHE)
    try:
        st = os.stat(path)
        stamp = (st.st_mtime_ns, st.st_size)
    except OSError:
        stamp = None
    if stamp is not None and _cached['stand'] == stamp:
        return _cached['daten']
    try:
        with open(path, encoding='utf-8') as f:
            data = json.load(f)
        if data.get('format') == FORMAT:
            _cached['stand'], _cached['daten'] = stamp, data
            return data
    except Exception:
        pass
    return EMPTY.copy()

def current_build():
    return load().get('build')


def update(build, progress=None):
    """Die Bergbau-Daten holen, wenn sie fehlen oder veraltet sind."""
    if AUS:
        return False, t('m_h_kein_netz')
    current = load()
    # ⚠ `refineries` fehlt in Ablagen von vor v3.3.0 — dort wurden beim Sichern
    # nur Orte und Zusammensetzungen behalten. Fehlt der Abschnitt, wird einmal
    # neu geholt; danach passiert wieder nichts. Die Datei ist 0,4 MB.
    if (current.get('build') == build and current.get('locations')
            and current.get('refineries') is not None
            and current.get('elemente') is not None):
        return True, t('m_b_aktuell') % len(current['locations'])
    if progress:
        progress(t('z_laedt') % ('Bergbau', 0.4))
    raw = hole_datei(SOURCE % build)
    locations_ = raw.get('locations') or []
    if not locations_:
        return False, t('m_b_leer')
    # ⚠ Die Raffinerien gehoeren dazu. Sie stehen im selben Abruf und
    # beantworten die Frage, die nach „wo baue ich das ab?" kommt: „und wohin
    # bringe ich es?" 20 Raffinerien, 10 verschiedene Profile — bei Quartz
    # liegen zwischen der besten und der schlechtesten 14 Prozentpunkte.
    _save({'format': FORMAT, 'build': build, 'locations': locations_,
           'compositions': raw.get('compositions') or {},
           'refineries': raw.get('refineries') or [],
           'refineryProfiles': raw.get('refineryProfiles') or {},
           # Die Stammdaten je Rohstoff — darin stehen Seltenheit und
           # Scan-Signatur, ohne die der Signatur-Rechner nichts kann.
           'elemente': raw.get('mineableElements') or {}})
    return True, t('m_b_geladen') % len(locations_)


def _save(data):
    target = pfade.app_datei(CACHE)
    try:
        os.makedirs(os.path.dirname(target), exist_ok=True)
        with open(target + '.tmp', 'w', encoding='utf-8') as f:
            json.dump(data, f, ensure_ascii=False)
        os.replace(target + '.tmp', target)
        # ⚠ Zwischenspeicher verwerfen: Zeitstempel und Groesse koennen sich
        # binnen derselben Sekunde wiederholen, dann bliebe der alte Stand.
        _cached['stand'] = None
        return True
    except Exception as exc:
        fehler.merken('mining._save', exc)
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
# Stufe zu der Zahl passen muss, die danebensteht (siehe `level()`).
#
# ⚠ **Höher angesetzt als bei strata.celd.space** (dort 25/15/8/4/1). Die
# rechnen über alles am Ort, wir je Abbauart — dadurch liegen unsere Anteile
# durchweg höher, und mit ihren Schwellen stand auf Daymar siebenmal
# „fast nur das" untereinander: 59, 48, 40, 35, 33, 31, 26 Prozent, alle
# gleich benannt. Eine Stufe, die für fast jede Zeile dasselbe sagt, sagt
# nichts. (Gemessen 08.09.2026 am fertigen Bild.)
LEVELS = ((50, 6), (30, 5), (15, 4), (7, 3), (2, 2))


def level(share):
    """1 bis 6 — von „kaum etwas" bis „fast nur das".

    ⚠ **Gerechnet wird auf der gerundeten Prozentzahl**, nicht auf dem
    Rohwert. Sonst steht in der Liste zweimal „25 %" untereinander, einmal
    mit „sehr viel" und einmal mit „viel" daneben — Titanium liegt am Yela-
    Gürtel bei 0,2499 und an Lagrange E bei 0,2520. Wer das sieht, hält das
    Programm für kaputt, und mit Recht: Was gleich aussieht, muss gleich
    heißen.
    """
    percent = round((share or 0.0) * 100)
    for threshold, value in LEVELS:
        if percent >= threshold:
            return value
    return 1


def _pot(kind):
    """Womit man hinfährt — `schiff_selten` ist derselbe Prospector."""
    return 'schiff' if kind.startswith('schiff') else kind


def _at_location(location, compositions):
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
    kinds, weight = {}, {}
    for g in location.get('groups') or []:
        kind = KINDS.get(g.get('groupName'))
        if not kind:
            continue
        # ⚠ Fehlt die Angabe, zählt die Gruppe voll — sonst fiele ein ganzer
        # Ort auf 0 und stünde ohne Erze da.
        group = g.get('groupProbability')
        group = 1.0 if group is None else group
        deposits = g.get('deposits') or []
        # Die `relativeProbability` ist **relativ innerhalb der Gruppe** —
        # erst durch die Summe geteilt wird daraus ein Anteil.
        total = sum(d.get('relativeProbability') or 0.0 for d in deposits)
        for d in deposits:
            c = compositions.get(d.get('compositionGuid'))
            if not c:
                continue
            deposit_share = ((d.get('relativeProbability') or 0.0) / total
                             if total else 1.0 / max(len(deposits), 1))
            for part in c.get('parts') or []:
                name = part.get('elementName')
                if not name:
                    continue
                kinds.setdefault(name, set()).add(kind)
                middle = ((part.get('minPercent') or 0.0)
                          + (part.get('maxPercent') or 0.0)) / 2.0
                probability = part.get('probability')
                probability = (1.0 if probability is None
                               else probability)
                key = (_pot(kind), name)
                weight[key] = (weight.get(key, 0.0)
                               + group * deposit_share
                               * probability * middle)
    # Je Topf auf 100 % normieren, dann je Erz den höheren Wert behalten.
    totals = {}
    for (pot, _n), w in weight.items():
        totals[pot] = totals.get(pot, 0.0) + w
    shares = {n: 0.0 for n in kinds}
    per_device = {}
    for (pot, name), w in weight.items():
        pot_total = totals.get(pot) or 0.0
        value = w / pot_total if pot_total else 0.0
        per_device.setdefault(pot, {})[name] = value
        shares[name] = max(shares.get(name, 0.0), value)
    return kinds, shares, per_device


def _ores_at_location(location, compositions):
    """{Erzname: {Abbauart, …}} für einen Ort."""
    return _at_location(location, compositions)[0]


def locations():
    """Alle Orte mit Erzen.

    `[{name, system, typ, erze:{name:{art}}, anteile:{name:(Anteil, Stufe)}}]`

    ⚠ `anteile` steht **neben** `erze`, nicht darin: An `erze` hängen der
    Selbsttest und die Lager-Abbauart, und beides soll von der Konzentration
    nichts wissen müssen.
    """
    data = load()
    comp = data.get('compositions') or {}
    result = []
    for o in data.get('locations') or []:
        ores_, shares, per_device = _at_location(o, comp)
        if not ores_:
            continue
        result.append({'name': o.get('locationName') or '?',
                       'system': o.get('system') or '',
                       'typ': o.get('locationType') or '',
                       'erze': ores_,
                       'anteile': {n: (a, level(a))
                                   for n, a in shares.items()},
                       'je_geraet': {g: {n: (a, level(a)) for n, a in values.items()}
                                     for g, values in per_device.items()}})
    result.sort(key=lambda x: x['name'].lower())
    return result


def mining_kinds(name):
    """Wie wird dieser Rohstoff abgebaut? — Menge aus `fps`, `fahrzeug`, `schiff`.

    ⚠ Gebraucht im Lager: Wer „Iron" einträgt, will auf einen Blick sehen, ob
    er dafür mit dem Multi-Tool loszieht oder ein Schiff braucht. Die Angabe
    steckt in den Bergbaudaten an jedem Fundort; hier werden sie über alle Orte
    des Rohstoffs zusammengefasst.

    `schiff_selten` zählt als `schiff` — für die Frage „womit hole ich das?"
    macht die Seltenheit keinen Unterschied.
    """
    wanted = norm_rohstoff(name)
    kinds = set()
    for e in ores():
        if norm_rohstoff(e.get('name')) != wanted:
            continue
        for entry in e.get('orte') or []:
            for kind in (entry[2] if len(entry) > 2 else ()):
                kinds.add('schiff' if kind.startswith('schiff') else kind)
    return kinds


def ores():
    """Alle Erze: `[{name, orte:[(Ort, System, {Art}, Anteil, Stufe)]}]`.

    Die Gegenrichtung zu `locations()`.

    ⚠ **Die Fundorte stehen nach Konzentration, nicht alphabetisch.** Wer
    fragt „wo hole ich Titanium?", will den ergiebigsten Ort zuerst sehen —
    eine alphabetische Liste beantwortet die Frage nicht, sie zeigt nur alle.

    ⚠ Die beiden hinteren Felder kamen später dazu. Wer die Liste auswertet,
    entpackt sie deshalb nachgiebig (`eintrag[3] if len(eintrag) > 3`) —
    `mining_kinds()` oben macht es genauso.
    """
    collected = {}
    for o in locations():
        per_device = o.get('je_geraet') or {}
        for name, kinds in o['erze'].items():
            share, lvl = (o.get('anteile') or {}).get(name, (0.0, 1))
            # Sechstes Feld: je Gerät `(Anteil, Stufe, wie viele Erze dieses
            # Gerät hier überhaupt findet)`. Die letzte Zahl trägt die
            # Aussage „das ist hier das einzige" — siehe `locations()`.
            fine = {}
            for device, values in per_device.items():
                if name in values:
                    fine[device] = values[name] + (len(values),)
            collected.setdefault(name, []).append(
                (o['name'], o['system'], kinds, share, lvl, fine))
    # ⚠ **Kein blankes `sorted()`.** Sobald zwei Einträge in Ort und System
    # übereinstimmen, verglich Python die Mengen dahinter — und Mengen haben
    # keine Reihenfolge. Deshalb ausdrücklich nur über die Zahlen sortieren.
    result = [{'name': n, 'orte': sorted(v, key=lambda x: (-x[3], x[0].lower()))}
              for n, v in collected.items()]
    result.sort(key=lambda x: x['name'].lower())
    return result


# Wie oft ein Vorkommen höchstens auftritt — das begrenzt, welche Vielfachen
# der Signatur überhaupt vorkommen können. Steht als `rarity` an jedem
# Rohstoff.
MAX_CHUNKS = {'legendary': 2, 'epic': 3, 'rare': 4, 'uncommon': 5,
              'common': 6}

# Grundsignaturen für Vorkommen ohne eigenen Wert. ⚠ `roc` und `fps` stehen so
# in den Daten (`groundScanSignature` 4000, `fpsScanSignature` 3000).
# `salvage` steht dort **nicht** — der Wert stammt aus der Tabelle auf
# scmdb.net. Wenn er je falsch ist, ist er dort genauso falsch.
BASE_SIGNATURES = (('roc', 4000, 7), ('fps', 3000, 10), ('salvage', 2000, 15))


# ⚠⚠ Das Spiel zeigt die Signatur als `17,200` — mit Tausenderkomma. Bis zum
# 08.09.2026 machte ein schlichtes `replace(',', '.')` daraus **17,2**, Faktor
# tausend daneben und ohne eine Zeile Fehlermeldung: Wer genau abschrieb, was
# im HUD stand, bekam Unsinn vorgesetzt.
#
# ⚠ Die Regel wohnt bewusst in `materials` und nicht hier — dort steht mit
# `parse_number` seit jeher alles, was eine getippte Zahl entgegennimmt, und zwei
# Fassungen derselben Regel liefen garantiert auseinander. Der Unterschied
# steckt allein im Schalter: Hier gilt `integer=True`, weil Signaturen ganze
# Zahlen im Tausenderbereich sind (`8,600` meint 8600, nie 8,6). Bei Mengen ist
# es umgekehrt, dort sind Kommazahlen der Regelfall.
def _number_text(raw):
    """Abgelesene oder getippte Signatur auf die Punkt-Schreibweise bringen."""
    from .materials import normalize_separators
    return normalize_separators(raw, integer=True)


def find_signature(query):
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
    text = _number_text(query)
    if not text:
        return []
    tolerance, low, high = 0.0, None, None
    try:
        if text.startswith('~'):
            value = float(text[1:])
            tolerance = 0.10
            low, high = value * 0.9, value * 1.1
        elif '-' in text[1:]:
            a, b = text.split('-', 1) if not text.startswith('-') else (None, None)
            low, high = sorted((float(a), float(b)))
            value = (low + high) / 2.0
        else:
            value = float(text)
            low = high = value
    except (ValueError, TypeError):
        return []
    if low is None:
        return []

    current = load()
    elements = current.get('elemente') or {}
    hits = []
    for _g, e in elements.items():
        sig = e.get('scanSignature')
        if not sig:
            continue
        most = MAX_CHUNKS.get(e.get('rarity'), 6)
        for count in range(1, most + 1):
            total = sig * count
            if low <= total <= high:
                deviation = (total - value) / value * 100.0 if value else 0.0
                hits.append((e.get('name') or '?', count, total, deviation))
    # Und die pauschalen Vorkommen ohne eigenen Rohstoff.
    for name, sig, most in BASE_SIGNATURES:
        for count in range(1, most + 1):
            total = sig * count
            if low <= total <= high:
                deviation = (total - value) / value * 100.0 if value else 0.0
                hits.append((name, count, total, deviation))
    hits.sort(key=lambda x: (abs(x[3]), x[0]))
    return hits


def plants():
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
    current = load()
    result = set()
    for o in current.get('locations') or []:
        for g in o.get('groups') or []:
            if g.get('groupName') != 'Harvestables':
                continue
            for dep in g.get('deposits') or []:
                name = (dep.get('presetName') or '').strip()
                if not name.startswith('Plant '):
                    continue
                result.add(_readable(name[len('Plant '):]))
    return sorted(result, key=str.lower)


def _readable(joined):
    """`HeartoftheWoods` wird zu `Heart of the Woods`.

    ⚠ Die kleinen Bindewoerter stehen in den Daten klein und mitten im Wort;
    sie an jedem Grossbuchstaben zu trennen ergaebe „Heart of the Woods" nur
    zufaellig richtig. Deshalb erst die bekannten Woerter herausloesen, dann an
    Grossbuchstaben trennen.
    """
    # ⚠ Das Bindewort darf auch von einem KLEINBUCHSTABEN gefolgt sein.
    # „HeartoftheWoods" ist genau so gebaut: auf „of" folgt „the". Mit
    # `(?=[A-Z])` blieb daraus „Heartof the Woods".
    text = re.sub(r'(?<=[a-z])(of|the|and)(?=[a-zA-Z])', r' \1 ', joined)
    text = re.sub(r'(?<=[a-z])(?=[A-Z])', ' ', text)
    return ' '.join(text.split())


def refineries_for(material):
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
    current = load()
    profiles = current.get('refineryProfiles') or {}
    if not profiles:
        return []
    wanted = norm_rohstoff(material)
    # Erst je Profil den Bonus bestimmen ...
    bonus_per_profile = {}
    for pid, values in profiles.items():
        bonus_per_profile[pid] = 0
        for mat, value in (values or {}).items():
            if norm_rohstoff(mat) == wanted:
                bonus_per_profile[pid] = value
                break
    # ... dann die Stationen dazu buendeln.
    bundled = {}
    for r in current.get('refineries') or []:
        pid = r.get('profileId')
        if pid not in bonus_per_profile:
            continue
        entry = bundled.setdefault(pid, {'namen': [], 'system': r.get('system'),
                                         'bonus': bonus_per_profile[pid]})
        entry['namen'].append(r.get('name') or '')
    result = [(e['namen'], e['system'], e['bonus']) for e in bundled.values()]
    result.sort(key=lambda x: (-x[2], x[0][0] if x[0] else ''))
    return result


def locations_for(material):
    """Wo gibt es diesen Rohstoff? Verträgt beide Schreibweisen.

    ⚠ Die Baupläne sagen `Aslarite`, hier heißt es `Aslarite (Raw)` — deshalb
    über `norm_rohstoff()` vergleichen. Ohne das findet der Sprung aus dem
    Rezept **nichts** (gemessen: 0 von 26)."""
    wanted = norm_rohstoff(material)
    for e in ores():
        if norm_rohstoff(e['name']) == wanted:
            return e
    return None
