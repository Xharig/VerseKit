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
Was ein Bauplan zum Herstellen braucht.

Beantwortet **eine** Frage: „Ich will die XL-1 bauen — was brauche ich dafür?"
Also Zutaten, Mengen und Herstellzeit zu jedem der 1.607 Baupläne.

⚠ **Was hier NICHT beantwortet wird: ob man es herstellen kann.** Der Watcher
liest Baupläne aus der `Game.log`; was an Erz im Frachtraum oder im Lager liegt,
steht dort nicht. Also „braucht 0,3 SCU Iron" — nie „du kannst das jetzt bauen".
Dieselbe Linie wie bei der Zählung `[BP 3/12]`, die am 28.08.2026 herausflog,
weil sie mehr behauptete, als sie wusste.

**Woher die Daten kommen**

`crafting_blueprints-<build>.json` von scmdb.net, 4,1 MB, einmal je Spiel-Build.
Dazu `crafting_items-<build>.json` (1,3 MB) für die Eigenschaften des Produkts —
die lädt der Katalog ohnehin schon, siehe `katalog.py`.

> **Nichts davon wird mitgeliefert.** scmdb steht unter CC BY-NC-ND 4.0; geholt
> wird zur Laufzeit auf dem Rechner des Nutzers, von der Original-Adresse. Die
> Nutzung ist von Krovax (scmdb) am 29.08.2026 ausdrücklich freigegeben — die
> Weitergabe **nicht**, und die könnte er auch gar nicht erlauben: die Rohdaten
> sind CIGs Eigentum.

**Aufbau der Quelle** (gemessen 29.08.2026, Build 4.10.0-live.12519617)

    blueprints[1607]
      productName          "Drake Ore Pod"
      manufacturer         "Drake Interplanetary"
      productEntityClass   -> items[].entityClass in crafting_items
      type / subtype       "orepod" / None
      tiers[]              je Preisstufe eine
        craftTimeSeconds   95
        slots[]            "Frame", "Core", ...
          options[]        type="resource", resourceName="Iron",
                           quantity=0.3, minQuality=0

Die Struktur sieht mehrere `tiers` je Bauplan vor. **Gemessen an Build
4.10.0-live.12519617 hat aber keiner mehr als einen** (0 von 1607) — hier steht
bewusst keine Warnung vor einem Fall, den es nicht gibt. Gelesen werden trotzdem
alle Stufen, damit es nicht bricht, falls CIG welche nachliefert.

⚠ **Die Rohstoffnamen sind nicht dieselben wie im Bergbau.** Hier steht
`Aslarite`, in `mining_data` steht `Aslarite (Raw)`. Für die spätere Verknüpfung
gibt es `norm_material()`.

⚠ Bis zum 12.09.2026 hieß dieses Modul `herstellung` (Sprachumstellung P4,
Stufe 2). **Nur Bezeichner sind umbenannt, keine Zeichenketten.** Bewusst
gleich geblieben: der Dateiname `crafting-blueprints.json` und alle Schlüssel
darin (`format`, `build`, `blueprints`, `dismantle`) — sonst gilt jede
vorhandene Ablage als veraltet und wird neu geholt (4,1 MB). Ebenso die
Schlüssel der Ergebnisse (`basis`, `name`, `hersteller`, `art`, `unterart`,
`stufen`, `tag`, `tags`, `entity`, `habe`, `zeit`, `zutaten`, `slot`,
`material`, `menge`, `mindestguete`, `wirkungen`, `eigenschaft`, `key`,
`mods`, `qualitaet`, `faktor`, `besser_hoch`, `absolut`, `spanne`) und die
Textschlüssel `he_art_…` / `he_sub_…`.

⚠ `norm_rohstoff()` heißt jetzt `norm_material()`. Drei bereits umgestellte
Module holen sie direkt (`materials`, `mining`, `prices`) — ihre Hinweise sind
mitgezogen.
"""
import json
import os
import re
import time

from . import fehler, pfade
from .katalog import AUS, hole_datei, _norm
from .sprache import t

# Die Datei heißt beim Anbieter so; <build> ist die Spielversion.
# Nur der Dateiname — welche Adresse benutzt wird, entscheidet
# `katalog.hole_datei()` (Spiegel zuerst, scmdb.net als Rückfall).
SOURCE = 'crafting_blueprints-%s.json'
CACHE = 'crafting-blueprints.json'

# Aufbau-Nummer wie im Katalog: hochzählen, sobald hier etwas anders abgelegt
# wird. Sonst behielte jeder seinen alten Stand bis zum nächsten Spiel-Patch,
# und der Umbau wäre für ihn unsichtbar.
FORMAT = 1

# Das Geruest, wenn noch nichts geladen ist.
EMPTY = {'format': FORMAT, 'build': None, 'blueprints': []}


# --------------------------------------------------------------- Holen/Laden


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
        # ⚠⚠ **Und das Roh-Verzeichnis mit.** Seit `recipe_raw()` den
        # Frisch-Check drosselt (`_RAW_FRESH_S`), wuerde es sonst bis zu eine
        # halbe Sekunde lang die alten Rezepte weiterreichen — genau nach dem
        # Schreiben, wo die neuen gebraucht werden. Hier zurueckgesetzt,
        # schlaegt jede Aenderung sofort durch.
        _raw_cache['stand'] = None
        _raw_cache['daten'] = None
        _raw_cache['geprueft'] = 0.0
        return True
    except Exception as exc:
        fehler.merken('crafting._save', exc)
        return False


def forget():
    """Zwischenspeicher leeren — nach einem Rezept-Update aufzurufen."""
    global _by_material
    _by_material = {}


def current_build():
    """Für welchen Spiel-Build liegen die Rezepte hier? Oder None."""
    return load().get('build')


def update(build, progress=None):
    """Die Rezepte holen, wenn sie fehlen oder zu einem alten Build gehören.

    Gibt (Erfolg, Meldung) zurück. **Sparsam**: Liegt derselbe Build schon da,
    wird gar nichts abgerufen — die Datei ist 4,1 MB groß."""
    if AUS:
        return False, t('m_h_kein_netz')
    raw_stand = load()
    # ⚠ `dismantle` fehlt in Ablagen von vor v3.3.0 — dort wurden beim Sichern
    # nur die Bauplaene behalten. Fehlt der Abschnitt, wird einmal neu geholt;
    # danach ist er da und es passiert wieder nichts.
    if (raw_stand.get('build') == build and raw_stand.get('blueprints')
            and raw_stand.get('dismantle') is not None):
        return True, t('m_h_aktuell') % len(raw_stand['blueprints'])
    if progress:
        progress(t('z_laedt') % ('Herstellung', 4.1))
    raw = hole_datei(SOURCE % build)
    items = raw.get('blueprints') or []
    if not items:
        return False, t('m_h_leer')
    # ⚠ Nicht nur die Bauplaene sichern. Im selben Abruf steht, welche
    # Rohstoffe beim Zerlegen NICHT zurueckkommen (`dismantle`) — sechs
    # Stueck, darunter Lindinium und Quantainium. Das gehoert ans Rezept:
    # Ein Bauteil daraus ist eine Einbahnstrasse.
    _save({'format': FORMAT, 'build': build, 'blueprints': items,
           'dismantle': raw.get('dismantle') or {}})
    forget()
    return True, t('m_h_geladen') % len(items)


# ------------------------------------------------------------- Auswerten
def norm_material(name):
    """Rohstoffnamen vergleichbar machen — über Datenquellen hinweg.

    ⚠ Die Baupläne sagen `Aslarite`, `mining_data` sagt `Aslarite (Raw)`, und
    bei Agricium steht dort `Agricium (Ore)`. Ohne diese Angleichung findet die
    Bergbau-Sicht später zu **keinem** Rohstoff einen Fundort — beim Messen am
    29.08.2026 waren es 0 von 26.

    Dazu die englische/amerikanische Schreibweise: `Aluminium` / `Aluminum`.
    """
    if not name:
        return ''
    short = name.split('(')[0].strip().lower()
    return short.replace('aluminium', 'aluminum')


def _ingredients(tier):
    """Die Zutaten einer Ausbaustufe: [(Slot, Rohstoff, Menge, Mindestgüte)]."""
    result = []
    for slot in tier.get('slots') or []:
        for o in slot.get('options') or []:
            if o.get('type') == 'resource' and o.get('resourceName'):
                result.append((slot.get('name') or '',
                               o['resourceName'],
                               o.get('quantity') or 0,
                               o.get('minQuality') or 0))
    return result


def _name_from_tag(tag):
    """Ein lesbarer Ersatzname, wenn `productName` fehlt.

    ⚠ **Fünf Baupläne tragen keinen Produktnamen** (gemessen 29.08.2026):
    die Kühler von Idris und Pioneer, die Radare von Idris, Lephari und Polaris.
    Ohne Ersatz stünden in der Liste fünf Einträge namens „?".

    Aus `BP_CRAFT_RADR_GNRP_S03_Idris_TEMP` wird `RADR GNRP S03 Idris`.
    """
    raw = (tag or '').replace('BP_CRAFT_', '').replace('_SCItem', '')
    raw = raw.replace('_TEMP', '').replace('_', ' ').strip()
    return raw or '?'


def _distinguisher(tag, name):
    """Woran man zwei gleichnamige Gegenstände auseinanderhält.

    Aus `BP_CRAFT_POWR_AEGS_S04_Idris_SCItem` neben `…_S04_Reclaimer_SCItem`
    wird `Idris` bzw. `Reclaimer`; bei `…_S02_BroadSpec_Lite` neben
    `…_S03_BroudSpec` bleibt die Größe `S02` / `S03` übrig.

    Genommen wird das, was **nicht** schon im Namen steht.
    """
    raw = _name_from_tag(tag)
    in_name = {w.lower() for w in (name or '').split()}
    parts = [w for w in raw.split()
             if w.lower() not in in_name and w.upper() not in ('BP', 'CRAFT')]
    # Von hinten: dort steht der Schiffs-/Variantenname, vorne die Kürzel.
    return ' '.join(parts[-2:]) if parts else raw


def _merge(items):
    """Mehrere Baupläne zu einem Listeneintrag — oder eben nicht.

    ⚠ **14 Produktnamen kommen mehrfach vor** (29.08.2026):

      * **10** sind echte Dubletten — dasselbe Rezept, nur eine andere Nummer
        im Tag (`…_01_01_13` neben `…_01_01_15`). Die gehören zusammen, sonst
        steht derselbe Gegenstand zweimal in der Liste.
      * **4** sind verschiedene Gegenstände mit gleichem Namen: „Main
        Powerplant" gibt es für Idris und Reclaimer, „BroadSpec" in S02 und
        S03, und „FoxFire" heißt bei scmdb auch der JUST Goliath. Die müssen
        getrennt bleiben — sonst verschwindet ein Rezept.

    Unterschieden wird am **Rezept**: gleiche Zutaten = ein Eintrag.
    """
    by_recipe = {}
    for b in items:
        key = tuple(sorted(
            '%s|%s|%s' % (slot, material, amount)
            for t_ in (b.get('tiers') or [])
            for slot, material, amount, _q in _ingredients(t_)))
        by_recipe.setdefault(key, []).append(b)
    return list(by_recipe.values())


def all_items():
    """Alle herstellbaren Dinge, für die Liste in der Oberfläche.

    Je Eintrag: Name, Hersteller, Art, Anzahl Ausbaustufen — und `tags`, weil
    zu einem Eintrag mehrere Baupläne gehören können.

    **Ein Eintrag je Gegenstand**, nicht je Bauplan: Gleiche Namen mit gleichem
    Rezept werden zusammengefasst (siehe `_merge`). Sonst zählt die
    Übersicht zu hoch — beim Messen am 29.08.2026 kamen so 406 „herstellbare"
    heraus, obwohl es 404 Baupläne waren."""
    by_name = {}
    for b in load().get('blueprints') or []:
        name = b.get('productName') or _name_from_tag(b.get('tag'))
        by_name.setdefault(_norm(name), []).append(b)

    result = []
    for group in by_name.values():
        parts = _merge(group)
        for part in parts:
            b = part[0]
            name = b.get('productName') or _name_from_tag(b.get('tag'))
            # ⚠ Bleiben mehrere Einträge unter demselben Namen übrig, sind es
            # **verschiedene Gegenstände** (Idris- und Reclaimer-Kraftwerk,
            # BroadSpec in zwei Größen). Ohne Unterscheidung stünden sie
            # zweimal gleich in der Liste, und niemand wüsste, welches welches
            # ist. Der Zusatz kommt aus dem Tag.
            #
            # ⚠ `basis` bleibt dabei der ursprüngliche Name — **danach** wird
            # mit dem Bestand verglichen. Wer den Anzeigenamen vergleicht,
            # findet den eigenen Bauplan nicht mehr wieder.
            display = name
            if len(parts) > 1:
                display = '%s (%s)' % (name, _distinguisher(b.get('tag'), name))
            result.append({
                'basis': name,
                'name': display,
                'hersteller': b.get('manufacturer') or '',
                'art': b.get('type') or '',
                'unterart': b.get('subtype') or '',
                'stufen': len(b.get('tiers') or []),
                'tag': b.get('tag') or '',
                'tags': [x.get('tag') or '' for x in part],
                'entity': b.get('productEntityClass') or '',
            })
    result.sort(key=lambda x: x['name'].lower())
    return result


_classification_cache = {'stand': None, 'daten': None}


# Name -> Entitäts-Kennung, einmal gebaut. Der Schlüssel ist der Formatstand
# der Rezeptdaten: Werden die neu geholt, fällt das auf und die Tabelle wird
# neu aufgebaut.
_entity_ids = {'stand': None, 'tabelle': {}}


def entity_of(name):
    """Die Entitäts-Kennung zu einem Bauplan — oder `''`.

    ⭐ Das ist die Brücke zu den Ladenpreisen: Dieselbe Kennung führt UEX als
    `uuid` (siehe `scbp/shops.py`). **Zugeordnet wird darüber, nie über den
    Namen** — über Namen ist es hier schon einmal schiefgegangen.

    ⚠ Gemerkt, nicht gesucht: Ohne Tabelle liefe bei jedem Aufklappen eine
    Schleife über rund 1.600 Baupläne.
    """
    if not name:
        return ''
    # Der Build der Rezeptdaten. Wird neu geholt, ändert er sich — und die
    # Tabelle wird von selbst neu gebaut.
    build_now = current_build()
    if _entity_ids['stand'] != build_now:
        table = {}
        for b in all_items():
            ident = b.get('entity') or ''
            if not ident:
                continue
            # Beide Schreibweisen: Die Oberfläche kennt mal den Grundnamen,
            # mal den mit Unterscheider in Klammern.
            table[b.get('basis') or ''] = ident
            table[b.get('name') or ''] = ident
        table.pop('', None)
        _entity_ids['stand'], _entity_ids['tabelle'] = build_now, table
    return _entity_ids['tabelle'].get(name, '')


def _key(name):
    """Namen vergleichbar machen — nur Buchstaben und Ziffern, klein."""
    return re.sub(r'[^a-z0-9]+', '', (name or '').lower())


def classification():
    """Zu jedem Bauplan seine Art und Unterart aus den Rezeptdaten.

        {'10seriesgreatswordcannon': ('weapons', 'laser'), …}

    ⚠ **Das ist der Schlüssel für die Filter.** Der Katalog kennt bei
    Schiffswaffen nur `WeaponGun` — welche davon ballistisch und welche Laser
    sind, steht ausschliesslich hier. Umgekehrt kennt er die Körperteile der
    Rüstung (`Char_Armor_Helmet`), die den Rezeptdaten fehlen; dort steht
    stattdessen die **Rolle** (`combat`, `engineer`, `stealth`).

    Beide Quellen zusammen ergeben also erst das vollständige Bild. Verknüpft
    wird über den Namen — gemessen am 29.08.2026: **738 von 738** Bauplänen des
    Katalogs finden so ihr Rezept.

    Wird einmal gelesen und gemerkt; die 2-MB-Datei bei jedem Filterklick neu
    zu lesen wäre dieselbe Falle wie beim Qualitätsregler.
    """
    data = load()
    stamp = id(data)
    if _classification_cache['stand'] == stamp:
        return _classification_cache['daten']
    mapping = {}
    for b in data.get('blueprints') or []:
        name = b.get('productName') or _name_from_tag(b.get('tag'))
        if not name:
            continue
        mapping[_key(name)] = ((b.get('type') or ''),
                               (b.get('subtype') or ''))
    _classification_cache['stand'] = stamp
    _classification_cache['daten'] = mapping
    return mapping


# Wie die Unterarten und Arten im Fenster heissen sollen. Was hier fehlt,
# wird unveraendert gezeigt — lieber der englische Rohwert als gar nichts.
def _translated(prefix, value):
    """Den Anzeigenamen holen — oder den Rohwert, wenn er unbekannt ist.

    Die Namen stehen in `sprache.py` unter `he_art_*` und `he_sub_*`. Fehlt
    einer (neue Waffenart nach einem Patch), wird der englische Rohwert
    gezeigt: lieber `tachyon` als eine leere Zeile.
    """
    from .sprache import TEXTE
    key = 'he_%s_%s' % (prefix, (value or '').lower())
    if key in TEXTE:
        return t(key)
    return value or ''


def kind_name(value):
    """Wie eine Rezept-Art im Fenster heisst."""
    return _translated('art', value)


def subkind_name(value):
    """Wie eine Unterart im Fenster heisst — Waffenart oder Rüstungsrolle."""
    return _translated('sub', value)


_raw_cache = {'stand': None, 'daten': None, 'geprueft': 0.0}

# ⚠⚠ **Wie lange ein einmal geprueftes Verzeichnis als frisch gilt.**
# `load()` fragt bei JEDEM Aufruf das Dateisystem (`os.stat`), um zu sehen,
# ob sich der Zwischenspeicher geaendert hat. Einzeln ist das nichts — in einer
# Schleife ueber den ganzen Katalog aber alles: Am 02.09.2026 gemessen,
# **738 Nachschlaege = 51 ms, davon 50 ms allein `load()`**; der eigentliche
# Verzeichnis-Zugriff kostete 0,1 ms. Das war der groesste Einzelposten beim
# Oeffnen der Bauplan-Liste (gemeldet als „bis Symbole und Text links geladen
# sind" — Haldjas, pr0, und am selben Tag erneut).
#
# Ein halbe Sekunde Nachlauf ist unbedenklich: Die Rezeptdaten aendern sich nur,
# wenn der Katalog neu geschrieben wird — und `_save()` setzt den Merker
# dann selbst zurueck, sodass die Aenderung SOFORT durchschlaegt.
_RAW_FRESH_S = 0.5


def recipe_raw(name):
    """Der unveränderte Rezept-Eintrag zu einem Namen — oder `None`.

    Gebraucht für die Einordnung: Dort zählt der **Tag**
    (`BP_CRAFT_APAR_BallisticGatling_S4`), und den gibt `recipe()` nicht heraus,
    weil er für die Anzeige nichts taugt.

    Wird als Verzeichnis gemerkt — 738 Baupläne einzeln durch eine Liste mit
    1607 Einträgen zu suchen wäre bei jedem Filterklick eine halbe Million
    Vergleiche.
    """
    # ⚠ Steht ein Verzeichnis und ist der Frisch-Check keine halbe Sekunde her,
    # geht es ohne `load()` weiter — siehe `_RAW_FRESH_S`. Ohne diese Abkuerzung
    # kostet ein Durchlauf ueber den Katalog 738 Dateisystem-Abfragen.
    now = time.monotonic()
    if _raw_cache['daten'] is not None \
            and now - _raw_cache['geprueft'] < _RAW_FRESH_S:
        return _raw_cache['daten'].get(_key(name))

    data = load()
    stamp = id(data)
    # ⚠ `daten is None` gehoert mit in die Bedingung: Sonst genuegt ein
    # geleertes Verzeichnis bei gleich gebliebener Kennung, und die letzte
    # Zeile laeuft in ein `None.get(...)`. Beim Bau der Gegenprobe zu dieser
    # Drosselung genau so passiert (02.09.2026).
    if _raw_cache['stand'] != stamp or _raw_cache['daten'] is None:
        index = {}
        for b in data.get('blueprints') or []:
            n_ = b.get('productName') or _name_from_tag(b.get('tag'))
            if n_:
                index.setdefault(_key(n_), b)
        _raw_cache['stand'] = stamp
        _raw_cache['daten'] = index
    _raw_cache['geprueft'] = now
    return _raw_cache['daten'].get(_key(name))


def subkind_of(name):
    """Die Unterart eines Bauplans — `ballistic`, `laser`, `combat` … oder ''."""
    return classification().get(_key(name), ('', ''))[1]


def kind_of(name):
    """Die Rezept-Art eines Bauplans — `weapons`, `armour`, `cooler` … oder ''."""
    return classification().get(_key(name), ('', ''))[0]
def recipe(name_or_tag):
    """Das Rezept zu einem Bauplan — oder None.

    Gibt je Ausbaustufe die Zutaten und die Herstellzeit zurück:

        {'name': 'Drake Ore Pod', 'hersteller': 'Drake Interplanetary',
         'stufen': [{'zeit': 95,
                     'zutaten': [('Frame', 'Iron', 0.3, 0)]}]}

    Gelesen werden alle Stufen; aktuell hat jeder Bauplan genau eine.
    """
    wanted = (name_or_tag or '').strip().lower()
    for b in load().get('blueprints') or []:
        if wanted in ((b.get('productName') or '').lower(),
                      (b.get('tag') or '').lower()):
            return {
                'name': b.get('productName') or b.get('tag') or '?',
                'hersteller': b.get('manufacturer') or '',
                'art': b.get('type') or '',
                'stufen': [{'zeit': (t_.get('craftTimeSeconds') or 0),
                            'zutaten': _ingredients(t_)}
                           for t_ in (b.get('tiers') or [])],
            }
    return None


def material_demand():
    """Wie viele Baupläne brauchen welchen Rohstoff? {Rohstoff: Anzahl}.

    Grundlage für die spätere Umkehrsicht („dir fehlen 12, dafür brauchst du
    vor allem Aslarite"). Gemessen am 29.08.2026: Aslarite steckt in 856 der
    1.607 Baupläne."""
    counter = {}
    for b in load().get('blueprints') or []:
        names = set()
        for t_ in b.get('tiers') or []:
            for _slot, material, _amount, _quality in _ingredients(t_):
                names.add(material)
        for n in names:
            counter[n] = counter.get(n, 0) + 1
    return counter


# ------------------------------------------------- Verknüpfung mit dem Bestand
#
# ⭐ **Das ist der Teil, den kein anderes Werkzeug kann.** scmdb lässt Besitz von
# Hand markieren („Mark Owned"); der Watcher weiß ihn aus der `Game.log`. Damit
# ist die Herstellungs-Liste keine Nachschlagetabelle, sondern trägt denselben
# Mehrwert wie die Bauplan-Liste: das Kästchen.
#
# ⚠ **Immer über `_norm()` vergleichen, nie stumpf.** Gemessen am 29.08.2026 an
# einem echten Bestand: 404 von 404 Bauplänen finden ihr Produkt — ohne
# Normalisierung nur 402. Die beiden Ausreißer (`7MA "Lorica"`, `Oracle Helmet`)
# sind die bekannte Anführungszeichen-Falle, die `katalog._norm()` behandelt.


def owns(collection_keys, product_name):
    """Hat der Spieler den Bauplan zu diesem Produkt?

    `collection_keys` ist das Ergebnis von `collection.keys(...)` — also
    bereits normalisierte Namen. Deshalb wird hier nur die andere Seite
    normalisiert."""
    return _norm(product_name or '') in (collection_keys or set())


def with_collection(collection_keys):
    """Alle herstellbaren Dinge, jedes mit der Angabe „Bauplan vorhanden".

    Gibt dieselbe Liste wie `all_items()` zurück, je Eintrag zusätzlich `habe`:

        True   der Bauplan liegt vor
        False  er fehlt
        None   **unklar** — siehe unten

    ⚠ **`None` ist der wichtige Fall.** Drei Gegenstandsnamen meinen mehrere
    verschiedene Dinge („BroadSpec" gibt es in S02 und S03, „Main Powerplant"
    für Idris und Reclaimer). Der Bestand kennt nur den Namen, nicht die
    Variante. Wer hier beide anhakt, verspricht dem Spieler, er könne **beide**
    bauen — und das wissen wir nicht.

    Gemessen am 29.08.2026 an einem echten Bestand: Ohne diese Unterscheidung
    standen 405 Häkchen in einer Liste, obwohl es 404 Baupläne sind. Die Linie
    ist dieselbe wie überall im Werkzeug: Was wir nicht wissen, behaupten wir
    nicht — „kennt der Katalog den Auftrag nicht, wird geschwiegen"."""
    result = all_items()
    ambiguous = set()
    seen = set()
    for e in result:
        k = _norm(e['basis'])
        if k in seen:
            ambiguous.add(k)
        seen.add(k)
    for e in result:
        # ⚠ Gegen `basis` vergleichen, nicht gegen den Anzeigenamen.
        owned = owns(collection_keys, e['basis'])
        e['habe'] = (None if (owned and _norm(e['basis']) in ambiguous) else owned)
    return result


def counts(collection_keys):
    """(sicher, gesamt, unklar) — für die Zeile über der Liste.

    Gedacht als Gegenstück zum Bauplan-Fortschritt: Dort steht, wie viele
    Baupläne man kennt; hier, wie viele der herstellbaren Dinge man davon
    tatsächlich bauen kann.

    ⚠ **Gezählt werden die eigenen Baupläne, nicht die Listeneinträge.** Vier
    Produktnamen kommen doppelt vor und meinen verschiedene Gegenstände
    („Main Powerplant" für Idris und Reclaimer, „BroadSpec" in zwei Größen).
    Wer über die Liste zählt, zählt so einen Bauplan zweimal — beim Messen am
    29.08.2026 kamen 405 heraus, obwohl der Bestand 404 hatte.

    ⚠⚠ **Das gilt für `unklar` genauso** (03.09.2026). Bis dahin zählte es die
    Listen*einträge*: Ein einziger Bauplan „BroadSpec" ergab **zwei** unklare,
    weil zwei Gegenstände so heißen. Solange die Zahl niemand sah, fiel das
    nicht auf — sobald sie neben der Kopfzahl steht, rechnet der Spieler
    `404 + 2 = 406` und hat 405 Baupläne. Gezählt werden deshalb die
    betroffenen **Namen**, nicht die Einträge; dann geht die Rechnung auf:
    404 sicher + 1 unklar = 405 im Bestand."""
    items = with_collection(collection_keys)
    certain = sum(1 for e in items if e['habe'] is True)
    unclear = len({_norm(e['basis']) for e in items if e['habe'] is None})
    return certain, len(items), unclear


# ------------------------------------------------- Was die Qualität bewirkt
#
# ⭐ **Der Teil, den keine Webseite leisten kann.** Die Rezepte sagen nicht nur,
# *welches* Material gebraucht wird, sondern auch, **wie stark die Qualität die
# Werte des Produkts verändert**:
#
#     {"startQuality": 0, "endQuality": 1000,
#      "modifierAtStart": 0.9, "modifierAtEnd": 1.1,
#      "propertyName": "Damage Mitigation"}
#
# Also: mieses Erz → 0,9-fache Schadensminderung, bestes Erz → 1,1-fache.
# Dazwischen wird linear gerechnet.
#
# Gemessen am 29.08.2026: **1.540 von 1.607 Bauplänen (96 %)** haben solche
# Angaben. Betroffen sind Min/Max Temp, Damage Mitigation, Integrity, Power
# Pips, Impact Force, Coolant Rating, Schildstärke, Rückstoß und mehr.
#
# ⚠ **Die Skala ist 0 bis 1000** — daher stammen auch die `minQuality`-Werte
# 500 bis 900 in den Rezepten.
#
# ⚠ **Es gibt mehrere Spannen je Eigenschaft** (etwa 0–500 und 501–1000): Die
# Kurve ist stückweise linear, nicht durchgehend. Wer nur die erste Spanne
# nimmt, rechnet oberhalb davon falsch.


def _range_for(modifiers, quality):
    """Die Spanne, in die diese Qualität fällt — sonst die nächstgelegene."""
    q = float(quality or 0)
    for m in modifiers:
        if float(m.get('startQuality', 0)) <= q <= float(m.get('endQuality', 0)):
            return m
    # Außerhalb aller Spannen: die mit der nächsten Grenze nehmen, damit das
    # Ergebnis nicht einfach verschwindet.
    if not modifiers:
        return None
    return min(modifiers,
               key=lambda m: min(abs(q - float(m.get('startQuality', 0))),
                                 abs(q - float(m.get('endQuality', 0)))))


def factor(modifiers, quality):
    """Der Multiplikator für diese Qualität — linear in der passenden Spanne."""
    m = _range_for(modifiers, quality)
    if not m:
        return None
    start, end = float(m.get('startQuality', 0)), float(m.get('endQuality', 0))
    a, b = float(m.get('modifierAtStart', 1)), float(m.get('modifierAtEnd', 1))
    if end == start:
        return b
    share = (float(quality or 0) - start) / (end - start)
    share = max(0.0, min(1.0, share))            # außerhalb nicht extrapolieren
    return a + share * (b - a)


def higher_is_better(modifiers):
    """Hebt bessere Qualität diesen Wert — oder senkt sie ihn?

    ⚠ **Nicht jede Eigenschaft wird durch eine höhere Zahl besser.** Gemessen
    an allen 6524 Modifikatoren des Spielstands 4.10.0: Bei **852** sinkt der
    Faktor mit steigender Qualität, und dort ist genau das die Verbesserung —
    weniger Rückstoß, weniger Quantum-Treibstoff:

    | Eigenschaft | Fälle |
    |---|---|
    | Recoil Smoothness / Handling / Kick | je 245 |
    | Quantum Fuel Burn | 114 |
    | Damage Mitigation | 3 (Ausreisser in den Quelldaten) |

    Ohne diese Unterscheidung stand ein Rückstoss von `× 0.800` — der
    bestmögliche Wert — in der Warnfarbe da, als wäre er schlecht, und
    `× 1.080` bei mieser Qualität in Grün. Am 30.08.2026 gemeldet: „ist es
    realistisch das sich bei niedrigerer Qualität die Werte erhöhen?"

    ⚠ Die Richtung steht **im Modifikator selbst** und wird nicht nach
    Eigenschaftsnamen geraten: `modifierAtEnd` gegen `modifierAtStart`. Damit
    stimmt sie auch dort, wo dieselbe Eigenschaft mal so und mal anders läuft —
    `armor_damagemitigation` tut das.

    Mehrteilige Spannen (0–500 / 501–1000) beschreiben EINE Kurve. Verglichen
    wird deshalb der Anfang der ersten mit dem Ende der letzten; ein flaches
    Teilstück in der Mitte würde sonst die Richtung verfälschen.
    """
    if not modifiers:
        return True
    first = min(modifiers, key=lambda m: float(m.get('startQuality', 0)))
    last = max(modifiers, key=lambda m: float(m.get('endQuality', 0)))
    return (float(last.get('modifierAtEnd', 1))
            >= float(first.get('modifierAtStart', 1)))


def is_absolute(modifiers):
    """Ist das ein Multiplikator — oder eine glatte Zahl?

    ⚠ **Nicht jede Wirkung ist ein Faktor.** `itemresource_powergeneration`
    („Power Pips") führt in den Spieldaten Werte von **−3 bis +3**, in festen
    Qualitätsstufen: unter Q250 gibt es −3 Pips, ab Q900 +3. Das sind
    Stückzahlen, keine Multiplikatoren.

    Als Faktor gelesen stand dort `× -1.000` — eine Zahl, die es nicht gibt:
    Ein Multiplikator von −1 würde den Wert umkehren, einer von 0 ihn
    auslöschen. 598 der 6524 Modifikatoren im Spielstand 4.10.0 sind so.

    ⚠ Erkannt wird das **an der Zahl, nicht am Namen**: Ein Multiplikator liegt
    immer über null. Taucht irgendwo im Satz eine Null oder ein negativer Wert
    auf, kann es keiner sein. Damit stimmt die Erkennung auch für Eigenschaften,
    die es heute noch nicht gibt.
    """
    for m in modifiers or []:
        if (float(m.get('modifierAtStart', 1)) <= 0
                or float(m.get('modifierAtEnd', 1)) <= 0):
            return True
    return False


def range_of(modifiers):
    """Was ist mit diesem Material überhaupt erreichbar?

    Gibt `(q_von, q_bis, f_von, f_bis, basis)` — die Qualitätsspanne, die
    Faktorspanne und die Qualität, bei der sich **nichts** ändert (Faktor 1).

    ⚠ **Ohne diese Angabe ist ein Faktor nicht einzuordnen.** `× 0.867` sagt
    für sich genommen nichts: Ist das nah am Machbaren oder bleibt noch viel?
    Erst `× 1.2–0.8` daneben macht klar, dass 0.867 schon gut zwei Drittel des
    Wegs sind. scmdb zeigt das aus demselben Grund unter jeder Zeile.

    `basis` ist der Nullpunkt — die Qualität, ab der es besser statt schlechter
    wird. Bei fast allen Rezepten liegt er bei 500.
    """
    if not modifiers:
        return None
    first = min(modifiers, key=lambda m: float(m.get('startQuality', 0)))
    last = max(modifiers, key=lambda m: float(m.get('endQuality', 0)))
    q_from = float(first.get('startQuality', 0))
    q_to = float(last.get('endQuality', 0))
    f_from = float(first.get('modifierAtStart', 1))
    f_to = float(last.get('modifierAtEnd', 1))

    # Wo ist der Faktor genau 1? Das Teilstück suchen, das die 1 enthält.
    base = None
    for m in sorted(modifiers, key=lambda x: float(x.get('startQuality', 0))):
        a = float(m.get('modifierAtStart', 1))
        b = float(m.get('modifierAtEnd', 1))
        if a == b:
            continue
        if min(a, b) <= 1.0 <= max(a, b):
            s = float(m.get('startQuality', 0))
            e = float(m.get('endQuality', 0))
            base = s + (1.0 - a) / (b - a) * (e - s)
            break
    return q_from, q_to, f_from, f_to, base


def dismantle_block():
    """Rohstoffe, die beim Zerlegen NICHT zurückkommen.

    Steht in den Rezeptdaten unter `dismantle.blacklistedResources`. Wer ein
    Stück wieder auseinandernimmt, bekommt die Hälfte des Materials zurück
    (`efficiency` 0.5) — diese sechs aber gar nicht. Das gehört ans Rezept,
    denn es ändert die Rechnung: Ein Bauteil aus Lindinium ist eine Einbahn-
    strasse.

    Gibt `(set(Namen), efficiency, sekunden)`.
    """
    try:
        d = load().get('dismantle') or {}
    except Exception:
        return set(), 0.5, 15
    names = {r.get('name') for r in (d.get('blacklistedResources') or [])
             if r.get('name')}
    names |= {r.get('name') for r in (d.get('blacklistedEntityClasses') or [])
              if r.get('name')}
    return names, float(d.get('efficiency', 0.5)), int(d.get('dismantleTimeSeconds', 15))


def slots(name_or_tag):
    """Die Slots eines Bauplans mit Material **und** Qualitätswirkung.

    [{slot, material, menge, mindestguete, wirkungen:[{eigenschaft, key, mods}]}]
    """
    wanted = (name_or_tag or '').strip().lower()
    for b in load().get('blueprints') or []:
        if wanted not in ((b.get('productName') or '').lower(),
                          (b.get('tag') or '').lower()):
            continue
        result = []
        for t_ in b.get('tiers') or []:
            for s in t_.get('slots') or []:
                material = amount = quality = None
                for o in s.get('options') or []:
                    if o.get('type') == 'resource' and o.get('resourceName'):
                        material = o['resourceName']
                        amount = o.get('quantity') or 0
                        quality = o.get('minQuality') or 0
                        break
                by_property = {}
                for m in s.get('modifiers') or []:
                    by_property.setdefault(
                        (m.get('propertyName'), m.get('propertyKey')),
                        []).append(m)
                result.append({
                    'slot': s.get('name') or '',
                    'material': material,
                    'menge': amount,
                    'mindestguete': quality,
                    'wirkungen': [{'eigenschaft': n, 'key': k, 'mods': v}
                                  for (n, k), v in by_property.items()],
                })
        return result
    return None


# ---------------------------------------------------------------------------
# Die Eigenschaften auf Deutsch
# ---------------------------------------------------------------------------
#
# ⚠ **Die Tabelle steht in `sprache.py`**, wie jeder andere Oberflächentext
# auch — Projektregel, und Prüfung 17 wacht darüber. Hier steht nur der Weg
# dorthin, weil die Rezeptdaten hier zu Hause sind.


def property_name(name, key=None):
    """Der Name der Eigenschaft in der eingestellten Sprache.

    ⚠⚠ **Der SCHLÜSSEL entscheidet, nicht der englische Text.**
    `propertyKey` ist sprachneutral und ändert sich nicht, wenn CIG die
    Beschriftung umformuliert — dieselbe Regel wie bei den Auftragsmeldungen
    (`auftraege.INI_SCHLUESSEL`). Über den englischen Namen zu gehen hieße:
    beim nächsten Patch fällt die Hälfte still auf Englisch zurück.
    """
    from . import sprache
    return sprache.eigenschaft(name, key)


def values_with_stock(name_or_tag, quality_per_material):
    """Was käme mit **diesem** Material heraus?

    `quality_per_material` ist {Material: Qualität} — in der Regel die beste
    brauchbare Qualität aus dem eigenen Lager
    (`materials.best_quality()`). Materialien ohne Eintrag werden
    übersprungen; über sie ist nichts bekannt, und geraten wird nicht.

    Gibt [{eigenschaft, material, qualitaet, faktor}] zurück.
    """
    result = []
    for s in (slots(name_or_tag) or []):
        q = (quality_per_material or {}).get(s['material'])
        if q is None:
            continue
        for w in s['wirkungen']:
            f = factor(w['mods'], q)
            if f is None:
                continue
            result.append({'eigenschaft': w['eigenschaft'], 'key': w['key'],
                           'material': s['material'], 'qualitaet': q,
                           'faktor': f, 'slot': s['slot'],
                           # ⚠ Ohne diese Angabe faerbt die Anzeige einen guten
                           # Wert als Warnung. Siehe `higher_is_better`.
                           'besser_hoch': higher_is_better(w['mods']),
                           # ⚠ Und ohne diese steht „× -1.000" da, wo „-1 Pip"
                           # hingehoert. Siehe `is_absolute`.
                           'absolut': is_absolute(w['mods']),
                           # Was waere ueberhaupt erreichbar? Siehe `range_of`.
                           'spanne': range_of(w['mods'])})
    return result


def material_names():
    """Alle Materialien, die in Rezepten vorkommen — alphabetisch.

    ⚠ **Damit niemand raten oder tippen muss.** Ein freies Textfeld für einen
    Namen, der exakt passen muss, ist eine stille Fehlerquelle: Wer „Aslerite"
    schreibt, bekommt nie einen Treffer und erfährt auch nicht, warum. Gemessen
    am 29.08.2026 sind es **26** Materialien — eine Liste, die in jede Auswahl
    passt.
    """
    names = set()
    for b in load().get('blueprints') or []:
        for t_ in b.get('tiers') or []:
            for slot, material, _amount, _quality in _ingredients(t_):
                if material:
                    names.add(material)
    return sorted(names, key=lambda x: x.lower())


def storable():
    """**Alles**, was im Lager stehen darf — die abschliessende Liste.

    Drei Quellen, alle aus den Spieldaten:

    | Quelle | Anzahl | wofür |
    |---|---|---|
    | Rezept-Materialien | 26 | was zum Herstellen gebraucht wird |
    | Mineralien aus den Bergbaudaten | 39 | auch was (noch) in keinem Rezept steht |
    | Pflanzen (`Harvestables`) | 13 | von Hand geerntet, mit Qualität |

    ⚠⚠ **Diese Liste ist eine Zusage, keine Empfehlung.** Was nicht darin
    steht, lässt sich nicht eintragen — auch nicht „trotzdem". Der Grund ist
    kein Ordnungssinn: Ein freies Textfeld heisst, dass jemand Schimpfwörter,
    Religiöses oder Politisches eintragen, ein Bildschirmfoto machen und es
    verbreiten kann. Am Ende fragt niemand, wer das getippt hat — es steht in
    diesem Werkzeug, also kommt es scheinbar von dessen Autor. Am 30.08.2026
    unmissverständlich festgelegt: „NUR was auch in der Rohstoff-Liste ist darf
    speicherbar sein, sonst nichts."

    Fehlt etwas in der Liste, wird die **Liste** ergänzt (sie kommt aus den
    Daten), nicht die Sperre gelockert.
    """
    # ⚠ Nach dem **angeglichenen** Namen zusammenfassen. „Agricium" und
    # „Agricium (Ore)" sind für uns dasselbe; beide anzubieten macht die
    # Vorschlagsliste doppelt so lang und die Auswahl zur Ratefrage.
    #
    # Vorrang hat die Schreibweise aus den Rezepten — die steht auch in der
    # Herstellung, und zwei Schreibweisen für einen Stapel wären genau der
    # Fehler, den die Liste verhindern soll.
    by_key = {}
    for n in material_names():
        by_key.setdefault(norm_material(n), n)
    try:
        from . import mining
        more = [(e.get('name') or '').strip()
                for e in (mining.load().get('elemente') or {}).values()]
        more += mining.plants()
        for n in more:
            if n:
                by_key.setdefault(norm_material(n), n)
    except Exception as exc:
        # Ohne Bergbaudaten bleibt es bei den Rezept-Materialien. Weniger
        # Auswahl ist hinnehmbar — ein offenes Textfeld nicht.
        fehler.merken('crafting.storable', exc)
    return sorted(by_key.values(), key=str.lower)


def knows_material(name):
    """Ist dieser Name einem Rezept-Material zuzuordnen?"""
    if not (name or '').strip():
        return False
    wanted = norm_material(name)
    return any(norm_material(n) == wanted for n in material_names())


def may_store(name):
    """Darf dieser Name im Lager stehen? Siehe `storable()`."""
    if not (name or '').strip():
        return False
    wanted = norm_material(name)
    return any(norm_material(n) == wanted for n in storable())


def storage_name(given):
    """Die verbindliche Schreibweise für das Lager — oder `None`.

    ⚠ Damit landet nie die Tippweise des Nutzers im Lager, sondern immer der
    Name aus den Spieldaten. Sonst stehen „orison-savrilium" und „Savrilium"
    als zwei Stapel da.
    """
    wanted = norm_material(given)
    if not wanted:
        return None
    for n in storable():
        if norm_material(n) == wanted:
            return n
    return None


def official_name(given):
    """Die verbindliche Schreibweise zu einer Eingabe — oder `None`.

    ⚠ **Der Name ist der Schlüssel.** Steht im Lager `aslarite` oder
    `Aslerite`, findet kein Rezept den Bestand, und niemand sieht, warum: Die
    Liste sieht richtig aus, nur die Häkchen bleiben aus. Deshalb wird die
    Eingabe hier auf einen bekannten Namen gezogen, statt sie zu übernehmen,
    wie sie getippt wurde.

    Was zusammengeführt wird:
      * Groß- und Kleinschreibung sowie Leerzeichen am Rand
      * die Bergbau-Schreibweise mit Klammer (`Aslarite (Raw)`)
      * `Aluminium` gegen `Aluminum`
      * ein knapper Vertipper, solange er **eindeutig** einem Namen zuzuordnen
        ist — bei zwei ähnlich nahen Kandidaten wird nichts geraten

    Gibt `None` zurück, wenn nichts sicher passt. Dann entscheidet die
    Oberfläche, ob sie nachfragt.
    """
    import difflib
    text = (given or '').strip()
    if not text:
        return None
    all_names = material_names()
    if not all_names:
        # ⚠ Keine Rezeptdaten geladen — dann gibt es nichts zu vergleichen.
        # Hier `None` zu melden hiesse: „kenne ich nicht", und die Oberfläche
        # wuerde **jede** Eingabe abweisen. Wer beim ersten Start ohne Netz
        # sein Lager fuellen will, kaeme nicht weiter. Also durchlassen.
        return text
    wanted = norm_material(text)

    for n in all_names:
        if norm_material(n) == wanted:
            return n

    # Vertipper: hohe Schwelle, und nur wenn der zweitbeste Treffer deutlich
    # schlechter ist. Sonst macht die Berichtigung aus einem falschen Namen
    # einen anderen falschen Namen.
    by_key = {norm_material(n): n for n in all_names}
    close = difflib.get_close_matches(wanted, list(by_key), n=2, cutoff=0.82)
    if len(close) == 1:
        return by_key[close[0]]
    if len(close) == 2:
        g = difflib.SequenceMatcher
        a = g(None, wanted, close[0]).ratio()
        b = g(None, wanted, close[1]).ratio()
        if a - b >= 0.08:
            return by_key[close[0]]
    return None


_by_material = {}


def blueprints_with(material):
    """Welche Baupläne brauchen diesen Rohstoff? Namen, alphabetisch.

    ⭐ **Die Gegenrichtung, die gefehlt hat.** Die Suche schaute nur auf
    Bauplan-NAMEN. Wer „sad" tippte, um zu sehen, was aus Sadaryx wird, bekam
    „Cru*sad*er Edition" — und nie eine Antwort. Am 30.08.2026 gemeldet:
    „Was kann ich aus Sadaryx herstellen? Meine User werden es nie erfahren."

    ⚠ Eine **leere Liste ist auch eine Antwort**, und zwar oft die richtige:
    26 der 52 einlagerbaren Namen kommen in keinem einzigen Rezept vor — alle
    13 Pflanzen und 13 Mineralien, darunter Sadaryx. Das muss dastehen, statt
    dass jemand weitersucht.
    """
    global _by_material
    if not _by_material:
        for b in load().get('blueprints') or []:
            name = b.get('productName')
            if not name:
                continue
            for t_ in b.get('tiers') or []:
                for s in _ingredients(t_):
                    _material = s[1]
                    if _material:
                        _by_material.setdefault(norm_material(_material),
                                                set()).add(name)
    return sorted(_by_material.get(norm_material(material)) or (),
                  key=str.lower)


def similar_storage_names(name, most=4):
    """Vorschläge aus der **Lager**-Liste — Mineralien und Pflanzen.

    Wie `similar_materials`, nur über `storable()`. ⚠ Eigene Funktion
    statt eines Schalters: Die Rezept-Vorschläge in der Herstellung dürfen
    keine Pflanzen anbieten, die dort nie vorkommen.
    """
    import difflib
    text = (name or '').strip().lower()
    if not text:
        return []
    all_names = storable()
    # Erst, was den Text enthält — „ran" soll „Laranite" und „Taranite" finden.
    hits = [n for n in all_names if text in n.lower()]
    if hits:
        return hits[:most]
    return difflib.get_close_matches(text, all_names, n=most, cutoff=0.6)


def similar_materials(name, most=3):
    """Vorschläge zu einem Namen, der so nicht bekannt ist.

    Erst Namen, die den Text enthalten; sonst die mit der kleinsten
    Tippabweichung. Damit aus „Aslerite" ein „Aslarite" wird, statt eines
    stillen Fehlschlags."""
    import difflib
    text = (name or '').strip().lower()
    if not text:
        return []
    all_names = material_names()
    hits = [n for n in all_names if text in n.lower()]
    if hits:
        return hits[:most]
    return difflib.get_close_matches(text, all_names, n=most, cutoff=0.6)
