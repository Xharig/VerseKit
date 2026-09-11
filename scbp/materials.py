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
Das eigene Rohstoff-Lager — von Hand geführt.

**Warum von Hand.** Die `Game.log` sagt **nichts** über Rohstoffe: In 17 MB
Protokollen (aktuelles plus 18 Sicherungen, Stand 29.08.2026) kommt kein
einziges Mal `craft`, `resource`, `inventory` oder `cargo` vor. Was im
Frachtraum liegt, kann der Watcher also nicht wissen — anders als bei den
Bauplänen, die im Log stehen.

**Der Vorschlag dahinter** stammt von **Horthy (KRT)** (29.08.2026):
Rohstoffe selbst eintragen, und beim Herstellen sagt man dem Werkzeug „Bauplan
X baue ich jetzt" — dann zieht es die Zutaten ab. Die Mengen kennt es seit
v3.3.0 ohnehin (`crafting.recipe()`).

⚠ **Haltung: Hinweis, keine Behauptung.**

Zugänge muss der Spieler eintragen — wer das zweimal vergisst, hat ein
lückenhaftes Lager. Deshalb sagt das Werkzeug **nie** „du kannst das nicht
bauen", sondern höchstens „dir fehlt Iron". Ein veraltetes Lager wird dadurch
nicht falsch, nur weniger hilfreich. Dieselbe Linie wie bei der gelöschten
Zählung `[BP 3/12]`: lieber nichts sagen als etwas Unwahres.

**Aufbau der Datei** (`rohstoffe.json` im eigenen Ordner)

    {"format": 1,
     "posten": [{"material": "Iron", "menge": 12.5,
                 "qualitaet": 80, "ort": "Daymar"}]}

Mehrere Posten desselben Materials sind Absicht: 12 SCU Iron von Daymar mit
80 % Güte sind etwas anderes als 3 SCU aus dem Aaron Halo.

⚠ Bis zum 11.09.2026 hieß dieses Modul `rohstoffe` (Sprachumstellung P4,
Stufe 2). **Nur Bezeichner sind umbenannt, keine Zeichenketten.** Bewusst
gleich geblieben, weil sie in der Datei jedes Nutzers stehen: der Dateiname
`rohstoffe.json` und die Schlüssel `format`, `posten`, `material`, `menge`,
`qualitaet` und `ort`. Ebenso die Schlüssel `name` und `menge`, die `stock()`
für die Anzeige liefert, die Einheit `'cscu'` und die Textschlüssel `s_rf_…`.
`norm_material` kommt aus `crafting.py` und heißt dort weiter so.
`calculate` und `parse_number` reicht `trade_cargo.py` weiter — das
Handelslager rechnet mit denselben Regeln.
"""
import json
import re
import os

from . import fehler, pfade
from .crafting import norm_material

FILE = 'rohstoffe.json'
FORMAT = 1


def load():
    """Alle Posten — oder eine leere Liste."""
    try:
        with open(pfade.app_datei(FILE), encoding='utf-8') as f:
            data = json.load(f)
        if data.get('format') == FORMAT:
            return data.get('posten') or []
    except Exception:
        pass
    return []


def save(entries):
    """Die Posten schreiben. Meldet einen Fehlschlag, statt ihn zu schlucken.

    ⚠ Die **Vorgängerfassung** (`rohstoffe.bak.json`) legt
    `pfade.json_sichern` an. Bis 31.08.2026 fehlte sie hier: Geschrieben wurde
    atomar, aber ohne Rückfall — ein leer gespeichertes Lager war endgültig
    weg. Ein Lager sind eigene Eingaben, die kein Neuaufbau zurückholt.
    """
    target = pfade.app_datei(FILE)
    try:
        return pfade.json_sichern(target, {'format': FORMAT, 'posten': entries})
    except Exception as exc:
        fehler.merken('materials.save', exc)
        return False


def as_csv(entries=None):
    """Das Lager als Tabelle — Material, Menge, Qualität, Lagerort.

    Warum CSV und nicht nur JSON: Eine Tabelle öffnet sich in jedem
    Tabellenprogramm und lässt sich weiterreichen. Das eigene JSON ist zum
    Zurücklesen da, die Tabelle zum Ansehen und Teilen.

    ⚠ Semikolon als Trenner und Komma als Dezimalzeichen — so erwartet es ein
    deutsches Excel/LibreOffice. Mit Punkt und Komma-Trenner landet „1.36" dort
    als Datum oder in einer Spalte zu viel.
    """
    entries = load() if entries is None else entries
    lines = ['Material;Menge;Qualitaet;Lagerort']
    for p in entries:
        amount = ('%g' % float(p.get('menge') or 0)).replace('.', ',')
        quality = ('%g' % float(p['qualitaet'])) if p.get('qualitaet') else ''
        lines.append(';'.join((
            (p.get('material') or '').replace(';', ','),
            amount, quality,
            (p.get('ort') or '').replace(';', ','))))
    return '\n'.join(lines) + '\n'


def as_json(entries=None):
    """Das Lager als JSON-Text — dasselbe Format, das `load()` wieder liest.

    Damit ist der Export zugleich eine Sicherung: Datei wegschreiben, später
    zurückspielen, fertig.
    """
    entries = load() if entries is None else entries
    return json.dumps({'format': FORMAT, 'posten': entries},
                      ensure_ascii=False, indent=1)


def from_json(text):
    """Ein früher ausgegebenes Lager wieder einlesen.

    Gibt die Postenliste zurück oder `None`, wenn die Datei nicht passt. ⚠ Es
    wird **nichts** gespeichert — das entscheidet die Oberfläche, nachdem sie
    gefragt hat, ob ersetzt oder ergänzt werden soll.
    """
    try:
        data = json.loads(text)
    except Exception:
        return None
    if not isinstance(data, dict) or data.get('format') != FORMAT:
        return None
    entries = data.get('posten')
    if not isinstance(entries, list):
        return None
    clean = []
    for p in entries:
        if not isinstance(p, dict) or not (p.get('material') or '').strip():
            continue
        clean.append({'material': str(p.get('material')).strip(),
                      'menge': float(p.get('menge') or 0),
                      'qualitaet': p.get('qualitaet'),
                      'ort': str(p.get('ort') or '').strip()})
    return clean


# ⚠⚠ Tausendertrennzeichen gegen Dezimalkomma — ein Zeichen, zwei Bedeutungen.
# `17,200` heisst im Spiel siebzehntausendzweihundert, `12,5` heisst zwoelf
# Komma fuenf. Bis zum 08.09.2026 machte ein schlichtes `replace(',', '.')`
# aus beidem eine Kommazahl — bei der Scan-Signatur lag das Ergebnis damit
# Faktor tausend daneben, ohne eine Zeile Fehlermeldung.
#
# ⚠ **Geraten wird nicht.** Aufgeloest wird nur, was eindeutig ist:
#
# | Eingabe | Warum eindeutig | Ergebnis |
# |---|---|---|
# | `1.234,56` | beide Zeichen — das hintere trennt Dezimalstellen | 1234.56 |
# | `1,234,567` | dasselbe Zeichen mehrfach — kann nur Tausender sein | 1234567 |
# | `12,5` | ein Zeichen, keine Dreiergruppe | 12.5 |
#
# Bleibt der eine mehrdeutige Fall: EIN Trennzeichen mit GENAU drei Ziffern
# dahinter (`1,500`). Den entscheidet der Aufrufer ueber `integer`:
#
# * `integer=True` (Scan-Signatur) → 1500. Signaturen sind ganzzahlig und
#   liegen im Tausenderbereich; eine Signatur von 1,5 gibt es nicht.
# * `integer=False` (Mengen, Standard) → 1,5. Hier sind Kommazahlen der
#   Regelfall — `12,5 SCU` tippt jeder, `1.500 SCU` fast niemand. Ein falsch
#   aufgeloester Tausender wuerde hier das Lager um Faktor tausend verbuchen.
_THOUSANDS = re.compile(r'(\d)[.,](\d{3})(?!\d)')


def normalize_separators(text, integer=False):
    """Trennzeichen aufloesen und auf die Punkt-Schreibweise bringen."""
    raw = (text or '').strip()
    if not raw:
        return ''
    commas, dots = raw.count(','), raw.count('.')

    # ⚠⚠ **Beide Zeichen: Das HINTERE trennt die Dezimalstellen.** Punkt.
    #
    # Bis zum 10.09.2026 lief auch dieser Fall ueber die Dreiergruppen-Schleife
    # unten — und die frass bei drei Nachkommastellen eine Gruppe zu viel:
    # `1,234.567` wurde erst zu `1234.567` und dann zu **1234567**. Wer eine
    # Menge mit drei Nachkommastellen eintippt, hatte sie um Faktor tausend im
    # Lager stehen, ohne jede Meldung — genau der Fehler, gegen den diese
    # Funktion ueberhaupt geschrieben wurde.
    #
    # Mit beiden Zeichen braucht es die Schleife gar nicht: Welches Zeichen
    # welche Rolle hat, steht fest, sobald man weiss, welches hinten steht.
    if commas and dots:
        decimal = ',' if raw.rfind(',') > raw.rfind('.') else '.'
        raw = raw.replace('.' if decimal == ',' else ',', '')
        return raw.replace(',', '.')

    # Ab hier gibt es nur EIN Zeichen. Mehrfach kann es nur Tausender sein;
    # einmal ist es mehrdeutig und wird ueber `integer` entschieden.
    if integer or commas > 1 or dots > 1:
        previous = None
        # In der Schleife, sonst bliebe bei `1,234,567` die vordere Gruppe stehen.
        while previous != raw:
            previous = raw
            raw = _THOUSANDS.sub(r'\1\2', raw)
    return raw.replace(',', '.')


def parse_number(text):
    """Eine getippte Zahl lesen — Komma und Punkt gelten gleich.

    ⚠ Die einen tippen `12,5`, die anderen `12.5`. Python kennt nur den Punkt,
    und `float('12,5')` wirft. Ohne diese Stelle haette jeder zweite Nutzer
    beim Eintragen eine Fehlermeldung bekommen und nicht gewusst, warum.

    ⚠ Tausendertrennzeichen werden aufgeloest, aber nur wo es eindeutig ist —
    `1.234,56` wird 1234.56, `12,5` bleibt 12,5. Siehe `normalize_separators`.

    Auch das lange Minus vom Ziffernblock (`−`) wird angenommen, sonst
    scheitert das Abbuchen an einem Zeichen, das man nicht sieht.

    Gibt `None`, wenn es keine Zahl ist — dann meldet die Oberfläche das.
    """
    raw = normalize_separators(text).replace('−', '-')
    if not raw:
        return None
    try:
        return float(raw)
    except ValueError:
        return None


def same_stack(a_material, a_quality, a_place, b):
    """Sind das zwei Eintragungen für **denselben** Stapel?

    Gleich heisst: gleiches Material, gleiche Qualität, gleicher Lagerort. Nur
    dann darf zusammengezählt werden — unterschiedliche Qualität ist ein
    anderer Stapel, und was in Orison liegt, hilft in Pyro nicht.

    ⚠ Verglichen wird über `norm_material` und ohne Rücksicht auf Gross- und
    Kleinschreibung: „orison" und „Orison" sind derselbe Ort, „Iron (Ore)" und
    „Iron" dasselbe Material.
    """
    if norm_material(a_material) != norm_material(b.get('material')):
        return False
    if (a_place or '').strip().lower() != (b.get('ort') or '').strip().lower():
        return False
    a_q = None if a_quality is None else int(round(float(a_quality)))
    b_q = b.get('qualitaet')
    b_q = None if b_q is None else int(round(float(b_q)))
    return a_q == b_q


def add(material, amount, quality=None, place=''):
    """Einen Posten hinzufügen. Gibt die neue Gesamtmenge des Materials zurück.

    ⚠⚠ **Gleiches Material, gleiche Qualität, gleicher Ort wird
    ZUSAMMENGEZÄHLT**, nicht ein zweites Mal in die Liste gestellt. Wer zweimal
    Savrilium Q 600 in Orison einträgt, hat einen Stapel mit der Summe — keine
    zwei Zeilen, die gleich aussehen und einzeln gepflegt werden müssten.

    Am 30.08.2026 gemeldet, mit Ansage: „ich hab mich mal extra dumm gestellt,
    weil das sind die Fälle wie es passieren wird." Genau so: Man trägt nach
    jedem Abbauflug nach und weiss nicht mehr, ob der Stapel schon dasteht.

    Ohne das Zusammenfassen zerfällt ein Lager mit der Zeit in Dutzende
    Zeilen desselben Materials, und die Herstellung rechnet zwar richtig, aber
    niemand findet mehr etwas.
    """
    entries = load()
    for p in entries:
        if same_stack(material, quality, place, p):
            p['menge'] = round(float(p.get('menge') or 0) + float(amount or 0), 6)
            save(entries)
            return amount_of(material)
    entries.append({'material': (material or '').strip(),
                    'menge': float(amount or 0),
                    'qualitaet': quality,
                    'ort': (place or '').strip()})
    save(entries)
    return amount_of(material)


def change(index, material, amount, quality=None, place=''):
    """Einen vorhandenen Posten überschreiben (Position in der Liste).

    Gebraucht wird das dauernd: Man vertippt sich bei der Menge, gibt jemandem
    zwei SCU ab oder trägt den Lagerort nach. Ohne diesen Weg blieb nur
    löschen und neu tippen — und wer beim Tippen den Namen anders schreibt,
    hat den Posten anschliessend doppelt.

    Gibt True zurück, wenn es die Nummer gab.
    """
    entries = load()
    if not (0 <= index < len(entries)):
        return False
    entries[index] = {'material': (material or '').strip(),
                      'menge': float(amount or 0),
                      'qualitaet': quality,
                      'ort': (place or '').strip()}
    save(entries)
    return True


def remove(index):
    """Einen Posten löschen (Position in der Liste)."""
    entries = load()
    if 0 <= index < len(entries):
        entries.pop(index)
        save(entries)
        return True
    return False


def amount_of(material):
    """Wie viel ist von diesem Material da? Über alle Posten summiert.

    ⚠ Über `norm_material()` vergleichen — das Rezept sagt `Aslarite`, im Lager
    steht vielleicht `Aslarite (Raw)`, weil es aus der Bergbau-Sicht kopiert
    wurde."""
    wanted = norm_material(material)
    return sum(p.get('menge') or 0 for p in load()
               if norm_material(p.get('material')) == wanted)


def amount_with_quality(material, min_quality=0):
    """(passend, zu_gering) — wie viel taugt für die geforderte Qualität?

    ⚠ **Die Qualität ist keine Randnotiz.** 1.540 der 1.607 Baupläne (96 %)
    ändern die Werte des Produkts je nach Materialqualität, und 1.227 Zutaten
    fordern ausdrücklich eine Mindestqualität. Erz mit Q 200 in einem Rezept,
    das Q 500 verlangt, ist für diesen Bauplan nichts wert.

    Die Skala läuft **0 bis 1000** (aus den `modifiers` der Rezepte abgelesen).

    ⚠ Gefiltert, aber **nicht gesperrt**: Was nicht reicht, kommt als zweiter
    Wert zurück und wird als Hinweis angezeigt. Behauptet wird nichts — das
    Lager ist von Hand gepflegt und kann hinterherhinken.
    """
    wanted = norm_material(material)
    suitable = too_low = 0.0
    limit = float(min_quality or 0)
    for p in load():
        if norm_material(p.get('material')) != wanted:
            continue
        amount = float(p.get('menge') or 0)
        if float(p.get('qualitaet') or 0) >= limit:
            suitable += amount
        else:
            too_low += amount
    return suitable, too_low


def best_quality(material, min_quality=0):
    """Die höchste brauchbare Qualität dieses Materials im Lager — oder None.

    Damit lässt sich ausrechnen, **welche Werte** das Produkt bekäme; siehe
    `crafting.values_with_stock()`."""
    wanted = norm_material(material)
    best = None
    for p in load():
        if norm_material(p.get('material')) != wanted:
            continue
        if float(p.get('menge') or 0) <= 0:
            continue
        q = float(p.get('qualitaet') or 0)
        if q >= float(min_quality or 0) and (best is None or q > best):
            best = q
    return best


def stock():
    """{Material: Gesamtmenge} — für die Anzeige im Rezept."""
    result = {}
    for p in load():
        name = (p.get('material') or '').strip()
        if not name:
            continue
        key = norm_material(name)
        previous = result.get(key)
        result[key] = {
            'name': previous['name'] if previous else name,
            'menge': (previous['menge'] if previous else 0) + (p.get('menge') or 0),
        }
    return result


# Eine Rechnung im Mengenfeld: `1.04+3`, `12,5-0,5`.
_CALCULATION = re.compile(r'^\s*([\d.,]+)\s*([+-])\s*([\d.,]+)\s*$')
# Nur eine Auf-/Abbuchung: `+3`, `-0,5`.
_BOOKING = re.compile(r'^\s*([+\-−])\s*([\d.,]+)\s*$')


# Wie viele SCU ein cSCU ist. Das Raffinerie-Terminal im Spiel rechnet in
# **cSCU** (Hundertstel-SCU) — das Lager in SCU.
CSCU = 0.01


def refinery_lines(text, unit='cscu'):
    """Die Ausbeute eines Raffinerie-Auftrags aus getipptem Text lesen.

    Erwartet je Zeile `Material Qualität Menge`, so wie es im Terminal steht:

        Titanium 295 188
        Heart of the Woods 500 12

    Gibt `(posten, fehler)` zurück — `posten` als Liste
    `(material, menge_scu, qualitaet)`, `fehler` als Liste `(zeile, grund)`.

    ⚠⚠ **Die Zahlen von hinten lesen, nicht von vorn.** Fünf der 52
    einlagerbaren Namen haben Leerzeichen (`Heart of the Woods`,
    `Pressurized Ice`, …). Wer am ersten Leerzeichen trennt, verliert sie alle.
    Die letzten beiden Felder sind Qualität und Menge, davor steht der Name —
    egal wie viele Wörter er hat.

    ⚠ **cSCU ist die Voreinstellung**, weil das Terminal so rechnet
    („GEWONNENE MATERIALIEN (cSCU)"). Bei der falschen Annahme steht im Lager
    alles um den Faktor 100 daneben, und die Herstellung rechnet mit Unsinn.
    """
    from . import crafting
    from .sprache import t
    entries, errors = [], []
    factor = CSCU if unit == 'cscu' else 1.0
    for raw in (text or '').splitlines():
        line = raw.strip()
        if not line:
            continue
        parts = line.replace('\t', ' ').split()
        if len(parts) < 3:
            errors.append((line, t('s_rf_zu_kurz')))
            continue
        name = ' '.join(parts[:-2])
        try:
            # ⚠ Dieselbe Trennzeichen-Regel wie beim Eintippen (08.09.2026).
            # Vorher scheiterte eine Zeile mit BEIDEN Zeichen ganz: aus
            # `1.234,56` wurde `1.234.56`, und das warf — die Zeile landete
            # unter „keine Zahl", obwohl sie eindeutig lesbar war. Die Menge
            # kann hier vierstellig sein (cSCU-Ausbeute), das Dezimalkomma
            # bleibt bei SCU der Regelfall; deshalb ohne `integer`.
            quality = int(float(normalize_separators(parts[-2])))
            value = float(normalize_separators(parts[-1]))
        except ValueError:
            errors.append((line, t('s_rf_keine_zahl')))
            continue
        real = crafting.storage_name(name)
        if not real:
            # ⚠ Kein stiller Fehlschlag und keine stille Zuordnung: Der Name
            # wird **nicht** geraten, aber der wahrscheinlichste Treffer steht
            # daneben. Wer „Aslerite" tippt, soll „Aslarite" lesen und selbst
            # entscheiden — das Werkzeug entscheidet es nicht für ihn.
            similar = crafting.similar_materials(name, 2)
            reason = t('s_rf_unbekannt') % name
            if similar:
                reason += ' ' + t('s_rf_meintest') % ' · '.join(similar)
            errors.append((line, reason))
            continue
        if not 0 <= quality <= 1000:
            errors.append((line, t('s_rf_qualitaet')))
            continue
        if value <= 0:
            errors.append((line, t('s_rf_menge')))
            continue
        entries.append((real, round(value * factor, 4), quality))
    return entries, errors


def calculate(text, previous=0.0):
    """Was im Mengenfeld steht — als Zahl.

    Drei Schreibweisen, alle erlaubt:

    | Eingabe | Bedeutung | Ergebnis bei Bestand 1,04 |
    |---|---|---|
    | `4,5` | die neue Menge | 4,5 |
    | `+3` | dazubuchen | 4,04 |
    | `1.04+3` | ausrechnen | 4,04 |

    ⚠⚠ **Die dritte Form ist die, die vorher fehlte — und die natürlichste.**
    Beim Bearbeiten steht die aktuelle Menge bereits im Feld. Wer drei dazu
    buchen will, tippt hinten `+3` an und hat `1.04+3` dastehen. Genau das
    wurde bis v3.3.0-rc39 abgelehnt („Trag eine Menge ein, zum Beispiel 12,5"),
    weil nur ein **führendes** Vorzeichen zählte. Am 30.08.2026 gemeldet:
    „der Text unten sagt mach +5 wird auf oder -5 abgebucht, gehen tuts aber
    nicht, wie genau es geht kapier ich nicht."

    Beide Wege kommen aufs Gleiche — das ist kein Zufall, sondern der Punkt:
    Man muss nicht wissen, welchen das Programm meint.

    Gibt `None`, wenn nichts Sinnvolles dasteht. `previous` zählt nur bei der
    reinen Buchung.
    """
    raw = (text or '').strip()
    if not raw:
        return None
    m = _BOOKING.match(raw)
    if m:
        value = parse_number(m.group(2))
        if value is None:
            return None
        return float(previous or 0) + (-value if m.group(1) in '-−' else value)
    m = _CALCULATION.match(raw)
    if m:
        left, sign, right = (parse_number(m.group(1)), m.group(2),
                             parse_number(m.group(3)))
        if left is None or right is None:
            return None
        return left - right if sign == '-' else left + right
    return parse_number(raw)


def check(ingredients, count=1):
    """Was fehlt für dieses Rezept — bei `count` Stück?

    [(Material, gebraucht, da, fehlt, zu_geringe_qualitaet, mindestqualitaet)]

    `ingredients` ist die Liste aus `crafting.recipe()` — (Slot, Material,
    Menge, Güte). Zurück kommt **jede** Zutat, auch die vorhandenen: Die
    Anzeige soll zeigen, was da ist, nicht nur was fehlt.

    ⚠ **`count` muss hier durch, nicht nur beim Abziehen.** Wer 10 in das
    Stückzahl-Feld tippt, sieht sonst weiter den Bedarf für ein einziges Stück
    — und daneben „dir fehlt nichts", während in Wirklichkeit das Zehnfache
    gebraucht wird. Am 30.08.2026 gemeldet: „10 als Menge eingegeben sollte
    auch 10fache Menge an benötigtem Material sein, angezeigt wird es nicht."
    Die zurückgegebene `gebraucht`-Menge ist deshalb bereits multipliziert.
    """
    result = []
    factor = max(1, int(count or 1))
    needed = {}
    for _slot, material, amount, _quality in ingredients:
        key = norm_material(material)
        needed[key] = (needed.get(key, 0)
                       + (amount or 0) * factor)
    for _slot, material, amount, quality in ingredients:
        key = norm_material(material)
        # ⚠ Seit 29.08.2026 zählt nur, was die geforderte Qualität erreicht.
        # Vorher wurde `guete` durchgereicht und nie benutzt — dadurch galt Erz
        # als brauchbar, das für dieses Rezept zu schlecht ist.
        suitable, too_low = amount_with_quality(material, quality)
        required = needed[key]
        result.append((material, (amount or 0) * factor, suitable,
                       max(0.0, required - suitable), too_low, quality))
    return result


def deduct(ingredients, count=1):
    """Die Zutaten eines Rezepts aus dem Lager nehmen — `count` mal.

    ⚠ **`count` gibt es, damit niemand zählen muss.** Wer zehn Stück am Stück
    baut, klickt sonst zehnmal — und beim elften Klick stimmt der Bestand nicht
    mehr, ohne dass es auffällt. Am 29.08.2026 genau so gemeldet: „ich klicke
    dann aber sogar 11 mal, weil ich mich verzählt habe."

    Gibt `(True, [])` zurück, wenn alles da war — sonst `(False, [(Material,
    Fehlmenge), …])`.

    ⚠⚠ **Reicht das Lager nicht, wird GAR NICHTS abgezogen.** Bis
    v3.3.0-rc35 wurde genommen, so weit es reichte, und der Rest gemeldet.
    Das ist falsch: Fehlt eine Zutat, war der Gegenstand überhaupt nicht
    herstellbar — der Klick war ein Versehen oder ein Vertipper in der
    Stückzahl. Wer mit „Anzahl 10" klickte und Material für drei hatte, stand
    danach mit einem leergeräumten Lager und ohne die zehn Stück da, und der
    Bestand liess sich nur von Hand wieder zusammensuchen. Am 30.08.2026
    festgelegt: „Kann der Bestand im Lager ins Minus gehen? Darf er nicht,
    wenn was fehlt ist es ja nicht herstellbar."

    Ins Minus konnte er dabei nie geraten (`min(vorhanden, gebraucht)`) —
    aber „auf null geräumt" ist fast so schlimm. Deshalb erst rechnen, dann
    nehmen: Es wird in zwei Durchgängen gearbeitet, und der erste fasst nichts
    an.

    ⚠ Abgezogen wird vom **ältesten** Posten zuerst. Wer zwei Posten desselben
    Materials führt (verschiedene Güte oder Fundort), soll den älteren zuerst
    leer sehen — sonst bleiben lauter Reste stehen.
    """
    entries = load()
    factor = max(1, int(count or 1))

    # Mehrfach dieselbe Zutat im Rezept? Dann zaehlt die Summe, sonst wuerde
    # jeder Durchgang fuer sich pruefen und beide fuer machbar halten.
    demand = {}
    for _slot, material, amount, quality in ingredients:
        key = (norm_material(material), float(quality or 0))
        demand[key] = (demand.get(key, (material, 0.0))[0],
                       demand.get(key, (material, 0.0))[1]
                       + float(amount or 0) * factor)

    # --- Erster Durchgang: nur rechnen. Nichts wird angefasst. ---
    missing = []
    for (wanted, minimum), (name, needed) in demand.items():
        available = 0.0
        for p in entries:
            if (norm_material(p.get('material')) == wanted
                    and float(p.get('qualitaet') or 0) >= minimum):
                available += float(p.get('menge') or 0)
        if available + 1e-9 < needed:
            missing.append((name, round(needed - available, 6)))
    if missing:
        # Nichts angefasst, nichts gespeichert — das Lager bleibt, wie es war.
        return False, missing

    # --- Zweiter Durchgang: jetzt wirklich nehmen. ---
    for (wanted, minimum), (_name, needed) in demand.items():
        remaining = needed
        for p in entries:
            if remaining <= 1e-9:
                break
            if norm_material(p.get('material')) != wanted:
                continue
            if float(p.get('qualitaet') or 0) < minimum:
                continue
            available = float(p.get('menge') or 0)
            taken = min(available, remaining)
            p['menge'] = round(available - taken, 6)
            remaining -= taken
    # Leere Posten verschwinden — sonst füllt sich die Liste mit Nullen.
    entries = [p for p in entries if (p.get('menge') or 0) > 1e-9]
    save(entries)
    return True, []
