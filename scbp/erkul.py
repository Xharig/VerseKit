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
Welche Steckplätze ein Schiff hat — die Datenquelle dahinter.

Beantwortet die eine Frage, die zwischen Bauplan und Schiff steht: *Wohin
gehört das Teil eigentlich, und in welcher Größe?* Ohne diese Angabe ist ein
frisch freigeschalteter Bauplan eine Zeile ohne Anschluss.

## Woher

[erkul.games](https://erkul.games), das Auslegungs-Werkzeug der Community.
Seine Daten liegen offen unter `cdn.erkul.games` — Dateiendung `.bin`, innen
aber ganz normales JSON, nur **raw-deflate** gepackt::

    json.loads(zlib.decompress(rohbytes, -15))

`zlib` gehört zur Standardbibliothek. **Keine neue Abhängigkeit** — das ist die
wichtigste Regel des Projekts, und sie wird auch hier nicht aufgeweicht.

> **Nichts davon wird mitgeliefert.** Geholt wird zur Laufzeit auf dem Rechner
> des Nutzers, von der Original-Adresse, und dort abgelegt. Dieselbe Linie wie
> bei scmdb und UEX.

## ⭐⭐ Warum sich das überhaupt anschließen lässt: dieselbe Kennung

Erkuls Feld `ref` ist **exakt** die Entitäts-Kennung, unter der auch UEX und
scmdb denselben Gegenstand führen. Gegengeprüft am 06.09.2026::

    BlastChill  →  94ea5bb5-070c-4c75-b90d-66c26c38bb2a   (in shops.py dokumentiert)
                →  94ea5bb5-070c-4c75-b90d-66c26c38bb2a   (erkul liefert dasselbe)

⚠⚠ **Deshalb wird auch hier NIE über den Namen zugeordnet.** Genau daran ist es
bei den Ladenpreisen schon einmal schiefgegangen.

## ⚠ Der Sparmechanismus steckt in `catalog.bin`

Die Dateinamen tragen einen Hash, der sich **mit jedem Patch ändert**:
`coolers.a84a269d.bin`. Ein fest verdrahteter Name wäre nach dem nächsten
Donnerstag tot.

Deshalb steht am Anfang immer der Katalog (2,7 KB). Er nennt die Spielversion
und alle aktuellen Dateinamen. Steht dort dieselbe `dataVersion` wie beim
letzten Mal, ist Schluss — **ein kleiner Abruf, sonst nichts**. Das ist der
ganze Grund, warum dieses Modul einen Server nicht belastet.

## ⚠ Und es werden nur die Schiffe geholt, die der Spieler wirklich hat

Erkul führt 194 Schiffe und 25 Bodenfahrzeuge, jedes in einer eigenen Datei von
rund 16 KB. Alle zu holen wären 219 Abrufe für eine Frage, die sich auf drei
oder dreißig Schiffe bezieht.

Geholt wird deshalb **auf Zuruf**: Steht ein Schiff im Hangar (`fleet.py`) und
fehlt in der Ablage, wird genau dieses eine geholt. Ein voller Hangar kostet
einmalig so viele Abrufe, wie er Schiffe hat — danach nie wieder, bis CIG
patcht.

## Was abgelegt wird — und was nicht

Aus 16 KB Rohdaten je Schiff bleiben ein paar Zeilen übrig: **welche Art
Steckplatz in welcher Größe, und wie viele davon.** Alles andere (Kennwerte,
Beschreibungen, Schubwerte) fliegt raus. Es geht um die Frage „passt das
hinein" — nicht darum, erkul nachzubauen.

    {"drak_cutlass_black": {
        "name": "Drake Cutlass Black",
        "plaetze": [{"art": "Cooler", "groesse": 2, "anzahl": 2}, …]}}

⚠ **`api.erkul.games` wird nicht angefasst.** Der Wurzelabruf sagt ausdrücklich,
dass sie privat ist und Fremdzugriff nicht erlaubt. Dieses Modul spricht
ausschließlich mit dem CDN.
"""
import json
import re
import unicodedata
import urllib.request
import zlib

from . import fehler, uex
from .katalog import AUS, KENNUNG

# Der Zweig, aus dem gelesen wird. PTU führt eigene Daten, die den Spieler auf
# LIVE nur verwirren würden.
BRANCH = 'LIVE'
BASE = 'https://cdn.erkul.games'

CACHE = 'erkul-schiffe.json'
# ⚠ 2 seit v3.19.0-rc4: Jeder Eintrag trägt jetzt seine **Original-Kennung**
# (`id`, z. B. `anvl_arrow`). Ohne sie kennt die Ablage nur den geschliffenen
# Schlüssel `anvlarrow` — ein einziges Wort, in dem die wortweise Zuordnung
# keine Wortgrenzen mehr findet. Eine Ablage aus rc1–rc3 sieht deshalb aus wie
# „keine Steckplatz-Daten", obwohl die Daten da sind. Aufgefallen ist das am
# Anleitungsbild, das mit einer kopierten alten Ablage lief.
#
# ⚠ 3: Neben der gezählten Übersicht (`plaetze`) liegt jetzt die **einzelne**
# Steckplatzliste (`slots`) mit dem Teil, das ab Werk darinsteckt. Ohne sie
# lässt sich weder eine Auslegung speichern noch sagen, was am Schiff *nicht*
# ab Werk verbaut ist — „zwei Kühlerplätze Größe 2" nennt keinen Platz, dem
# sich ein Teil zuordnen ließe.
FORMAT = 3

# Notfrist. Maßgeblich ist die Spielversion aus `catalog.bin` — diese Frist
# greift nur, falls sich die gar nicht ermitteln lässt.
SHELF_LIFE = 30 * uex.DAY

# ⭐⭐ **`patch_bound=True`: Der Patch entscheidet, nicht die Uhr.**
# Steckplätze ändern sich mit einem Spiel-Patch und sonst nie. Eine Zeitfrist
# würde denselben Stand alle 30 Tage wegwerfen und neu holen — Abrufe, die
# niemandem nützen und die erkul bezahlt.
_store = uex.Store(CACHE, format_no=FORMAT, shelf_life=SHELF_LIFE,
                     patch_bound=True)

# ⚠ Steckplätze, die den Spieler nichts angehen. `invisible` und `uneditable`
# heißt: Das Spiel zeigt sie nicht und lässt sie nicht tauschen — ein Bauplan
# kann dort also nie landen. Sie trotzdem anzuzeigen hieße, eine Möglichkeit zu
# behaupten, die es nicht gibt.
HIDDEN = ('invisible', 'uneditable')

# Welche Steckplatz-Arten überhaupt interessant sind. Ein Schiff hat auch
# Plätze für Türen, Sitze und Leuchten; die tauchen in keinem Bauplan auf.
#
# ⚠ Die Namen kommen wörtlich aus erkuls Feld `accepts[].type` — nicht
# übersetzen, nicht schön machen. Übersetzt wird erst in der Anzeige.
#
# ⭐⭐ **Und sie sind bei scmdb dieselben.** Gegengeprüft am 06.09.2026 über
# alle 1.605 Gegenstände aus `crafting_items`: `WeaponGun`, `PowerPlant`,
# `Cooler`, `Shield`, `Radar`, `QuantumDrive`, `WeaponMining`, `TractorBeam`,
# `SalvageHead` heißen in beiden Quellen gleich. Deshalb braucht es **keine**
# Übersetzungstabelle zwischen Bauplan-Art und Steckplatz-Art — und keine, die
# bei jedem Patch nachgepflegt werden müsste.
#
# ⚠ Was scmdb hat und erkul nicht: `Char_Armor_*` und `WeaponPersonal`, also
# rund 1.100 Rüstungsteile und FPS-Waffen. Für die gibt es hier nie eine
# Antwort — sie kommen gar nicht erst bis hierher, weil ihnen die Größe fehlt.
INTERESTING = frozenset((
    'Cooler', 'PowerPlant', 'Shield', 'QuantumDrive', 'Radar', 'JumpDrive',
    'WeaponGun', 'Turret', 'TurretBase', 'MissileLauncher', 'Missile',
    'BombLauncher', 'Bomb', 'MiningLaser', 'WeaponMining', 'SalvageHead',
    'TractorBeam', 'TowingBeam', 'QuantumInterdictionGenerator', 'EMP',
    'FlightController', 'Paints',
    # ⚠ Die Aufsätze für Bergbau- und Bergungsköpfe. Ihre Steckplätze sitzen
    # **im Laser**, nicht am Rumpf (`BONE_ItemPort_Consumable_1`) — beim
    # Prospector drei Ebenen tief. Ohne sie fehlt genau die Sorte Bauplan, die
    # Bergbau-Spieler zuerst freischalten.
    'MiningModifier', 'SalvageModifier',
    # Erzbehälter und Frachtaufsätze — die Ore Pods des Prospectors.
    'Container', 'Cargo',
))

# ⚠⚠ **Eine eigene Menge für die Auslegung — und das ist Absicht.**
# `INTERESTING` beantwortet „passt mein *Bauplan* hier hinein". Der Warenkorb
# fragt etwas anderes: „was kann ich in diesen Platz überhaupt einbauen".
# Beides fällt auseinander, weil man Dinge kaufen kann, für die es keinen
# Bauplan gibt.
#
# Gemessen an der Cutlass Black (06.09.2026): Vier tauschbare Plätze stehen ab
# Werk leer und fielen durch `INTERESTING` heraus — Batterie, Bordrechner
# (`Avionics`), Gravitationsgenerator und der Cockpit-Anhänger. Die ersten drei
# führt UEX in seinen Warengruppen (`Batteries`, `Avionics`), sie sind also
# kaufbar und gehören in einen Warenkorb. Ein leerer Platz ist der
# offensichtlichste Warenkorb-Posten überhaupt: Dort *fehlt* etwas.
#
# ⚠ `Flair_Cockpit` bleibt draußen. Zierrat fürs Cockpit ist keine Ausrüstung,
# und UEX führt dafür keine Warengruppe — es stünde also dauerhaft ein Posten
# in der Liste, zu dem es nie einen Preis gibt.
#
# ⚠⚠ **`INTERESTING` wird dafür NICHT erweitert.** Das würde `slot_counts()` und
# `fits()` mit ändern, und daran hängt „passt der Bauplan in mein Schiff".
# Eine Menge, die zwei Fragen zugleich beantworten soll, beantwortet die
# zweite falsch.
SWAPPABLE = frozenset(INTERESTING | {'Battery', 'Avionics',
                                     'GravityGenerator'})


def _fetch(path, where):
    """Eine erkul-Datei abrufen und auspacken — oder `None`.

    Wirft **nie**: Ohne Netz läuft das Werkzeug weiter wie vorher, genau wie
    bei UEX. Der Grund steht dort ausführlich.
    """
    if AUS:
        return None
    address = '%s/%s' % (BASE, path.lstrip('/'))
    try:
        request = urllib.request.Request(
            address, headers={'User-Agent': KENNUNG})
        with urllib.request.urlopen(request, timeout=uex.TIMEOUT) as reply:
            raw = reply.read()
        # ⚠ `-15` = raw deflate, ohne zlib-Kopf. Mit `zlib.decompress(roh)`
        # allein scheitert es an genau dieser Stelle — der Kopf fehlt, weil
        # erkul die Dateien schon gepackt ablegt statt sie zu übertragen.
        return json.loads(zlib.decompress(raw, -15).decode('utf-8'))
    except Exception as error:
        fehler.merken('erkul.holen.' + where, error)
        return None


def ship_catalog():
    """Das Inhaltsverzeichnis — Spielversion und aktuelle Dateinamen."""
    return _fetch('%s/catalog.bin' % BRANCH, 'katalog')


def _maker_table(cat):
    """Ausgeschriebener Herstellername → erkuls Kürzel.

    ⭐⭐ **Erkul liefert diese Tabelle selbst mit** (152 Einträge in
    `manufacturers.<hash>.bin`, Feld `className` neben dem Klarnamen). Ohne sie
    bleibt ein Handeintrag wie „Anvil Arrow" ohne Steckplätze: Die Kennung
    heißt `anvl_arrow`, und `anvl` ist **kein Präfix** von „Anvil" — der Vokal
    fehlt in der Mitte. Dieselbe Zusammenziehung bei `aegs` (Aegis) und `misc`.

    ⚠ Aufgefallen ist das erst am **Anleitungsbild**: Dort stand bei vier
    erfundenen Beispielschiffen „keine Steckplatz-Daten", während der echte
    Hangar sauber aussah — weil dort das Herstellerkürzel aus dem Pledge-Export
    mitkommt. Von Hand eingetragene Schiffe haben es nicht.
    """
    path = next((f.get('path') for f in (cat.get('families') or [])
                 if (f.get('path') or '').startswith('manufacturers')), '')
    if not path:
        return {}
    rows_in = _fetch('%s/%s' % (BRANCH, path), 'hersteller')
    out = {}
    for entry in (rows_in or []):
        abbrev = (entry.get('className') or '').strip()
        plain = ((entry.get('i18n') or {}).get('name') or '').strip()
        if abbrev and plain:
            out[_slim(plain)] = abbrev
    return out


def load():
    return _store.load() or {}


def age():
    return _store.age()


def game_version():
    """Die Spielversion, zu der die Ablage gehört — oder `''`."""
    return load().get('spielversion') or ''


def candidates(name, maker='', short='', maker_short=''):
    """Alle Schreibweisen, unter denen erkul dieses Schiff führen könnte.

    Die Reihenfolge ist die Trefferquote, gemessen an einem echten
    Hangar-Export (42 Einträge, 06.09.2026):

    | Stufe | Treffer |
    |---|---|
    | Kurzname des Exports (`ANVL_Arrow` → `anvlarrow`) | **33** |
    | Herstellerkürzel + angezeigter Name | **1** |

    ⚠ Die restlichen acht sind **kein Zuordnungsfehler**: Crucible, Endeavor,
    Galaxy, Liberator, Merchantman und die beiden ATLS gibt es im Spiel noch
    gar nicht. Erkul führt nur Flugfähiges — ein Fehltreffer heißt hier also
    „noch nicht im Spiel", nicht „unbekannt". Das ist eine Auskunft, keine
    Panne, und wird dem Spieler auch so gesagt.
    """
    out = []
    # ⚠ Das **Kürzel** des Herstellers vor seinem ausgeschriebenen Namen:
    # Erkul führt „Roberts Space Industries" als `rsi`. Ohne diese Zeile fand
    # die Ursa Medivac keinen Anschluss, obwohl `rsi_ursa_medivac` existiert.
    for attempt in (short, '%s %s' % (maker_short, name), name,
                    '%s %s' % (maker, name)):
        slimmed = _slim(attempt)
        if slimmed and slimmed not in out:
            out.append(slimmed)
    return out


def ident(name, maker='', short='', maker_short=''):
    """Ein Schiff → erkuls Kennung, oder `''`.

    Drei Stufen, in dieser Reihenfolge: Buchstabenvergleich über den Kurznamen,
    dann über den angezeigten Namen, dann **wortweise**. Die dritte kostet
    etwas mehr, greift aber genau dort, wo die ersten beiden scheitern — und
    das ist bei jedem vierten Schiff der Fall.
    """
    stored = load()
    known_ships = stored.get('schiffe') or {}
    # ⚠ Auch hier: „Anvil Aerospace" muss zu `anvl` werden, sonst findet ein
    # von Hand eingetragenes Schiff seine eigenen Daten nicht wieder.
    maker_short = maker_short or (stored.get('hersteller') or {}).get(
        _slim(maker), '')
    for slimmed in candidates(name, maker, short, maker_short):
        if slimmed in known_ships:
            return slimmed
    # ⚠ Wortweise wird gegen die **Original-Kennungen** gesucht (`id`), nicht
    # gegen die geschliffenen Schlüssel — sonst fehlen die Wortgrenzen.
    by_id = dict((v.get('id') or k, k) for k, v in known_ships.items())
    hits = _search_wordwise(by_id, name, maker, short, maker_short)
    return by_id.get(hits, '') if hits else ''


def _slim(text):
    """`ANVL_F7C_Hornet` → `anvlf7chornet` — alles weg außer Buchstaben/Ziffern.

    ⚠ Bewusst **ohne** Trennzeichen: Der Export schreibt `L_22_Alpha_Wolf`,
    erkul `l22alphawolf`. Wer die Unterstriche behält, vergleicht zwei
    Schreibweisen derselben Sache und findet nichts.
    """
    # ⚠ **Akzente werden übersetzt, nicht weggeworfen.** Aus „San tok.Yāi"
    # wurde sonst `santokyi` — das `ā` fiel als Sonderzeichen heraus, und der
    # Name unterschied sich damit von jedem, der ihn ohne Strich schreibt.
    parts = unicodedata.normalize('NFKD', text or '')
    without_accent = ''.join(z for z in parts if not unicodedata.combining(z))
    return re.sub(r'[^a-z0-9]', '', without_accent.lower())


# Römische Zahlen, wie sie in Schiffsnamen vorkommen. Weiter als V geht es
# nicht — es gibt kein „Mk VI".
_ROMAN = {'i': '1', 'ii': '2', 'iii': '3', 'iv': '4', 'v': '5'}


def _words(text):
    """Ein Name → seine Wörter, mit `Mk II` zu `mk2` vereinheitlicht.

    ⚠ Punkte und Bindestriche fallen **vor** dem Zerlegen weg, nicht danach:
    Aus `F7C-M` wird `f7cm` (ein Wort, wie bei erkul), nicht `f7c` + `m`.
    """
    # ⚠ **Akzente werden übersetzt, nicht weggeworfen.** `San tok.Yāi` zerfiel
    # zu `san toky i`, weil das `ā` als Trennzeichen durchging — erkul schreibt
    # `santokyai`. Ein Schiff mit diakritischem Zeichen im Namen war damit
    # nicht auffindbar.
    import unicodedata
    parts = unicodedata.normalize('NFKD', text or '')
    without_accent = ''.join(z for z in parts if not unicodedata.combining(z))
    t = without_accent.lower().replace('.', '').replace('-', '')
    out = []
    for word in re.split(r'[^a-z0-9]+', t):
        if not word:
            continue
        hits = re.fullmatch(r'mk\s*([ivx]+|\d+)', word)
        if hits:
            number = hits.group(1)
            out.append('mk' + _ROMAN.get(number, number))
            continue
        # „Mk" und „II" getrennt geschrieben — zusammenziehen.
        if word in _ROMAN and out and out[-1] == 'mk':
            out[-1] = 'mk' + _ROMAN[word]
            continue
        out.append(word)
    return out


def _is_abbrev(short, long):
    """Ist `short` eine Zusammenziehung von `long`? (`aegs` ↔ `aegis`)

    Geprüft wird, ob die Buchstaben von `short` **in dieser Reihenfolge** in
    `long` vorkommen — mit Lücken, aber ohne Umsortieren.

    ⚠ **Nur für kurze Kürzel und nur bei gleichem Anfang.** Ohne diese beiden
    Bremsen wäre fast jedes kurze Wort Teilfolge von fast jedem langen, und
    die Zuordnung fände überall etwas. `aegs`/`aegis` besteht, `pir`/`pirate`
    ebenso — `abc`/`aXbXc` in einem beliebigen Schiffsnamen nicht mehr, weil
    der Anfang stimmen muss.
    """
    if not short or not long or len(short) > 5 or len(short) >= len(long):
        return False
    if short[0] != long[0]:
        return False
    where = 0
    for char in long:
        if where < len(short) and char == short[where]:
            where += 1
    return where == len(short)


def _matches_wordwise(erkul_id, wanted, chain=''):
    """Deckt `wanted` alle Wörter dieser erkul-Kennung ab? Gibt die Güte zurück.

    ⭐ **Die dritte Zuordnungsstufe** — sie fängt genau die Fälle, an denen
    Buchstabenvergleich scheitert, und das sind keine Ausnahmen:

    | im Hangar | bei erkul | woran es scheiterte |
    |---|---|---|
    | `Drake Ironclad Assault` | `drak_ironclad_assault` | Hersteller ausgeschrieben |
    | `F7C-M Super Hornet Mk II` | `anvl_hornet_f7cm_mk2` | römische Zahl, andere Wortfolge |

    ⚠ **Ein erkul-Wort darf auch Anfang eines gesuchten Worts sein** — genau so
    verhält sich jedes Herstellerkürzel (`drak` → Drake, `aegs` → Aegis,
    `anvl` → Anvil). Ohne das bleibt der halbe Hangar ohne Zuordnung.

    ⚠ Und **umgekehrt gilt es nicht**: `wanted` darf mehr Wörter haben als
    erkul (`Super` steht nur im Hangar-Namen), aber jedes erkul-Wort muss
    vorkommen. Sonst würde `anvl_hornet_f7cm` auch auf eine Mk II passen.
    """
    own = _words(erkul_id)
    # ⭐⭐ **Das erste Wort darf unerklärt bleiben — es ist das
    # Herstellerkürzel, und die Namen passen oft schlicht nicht zusammen.**
    # Gemessen am 06.09.2026 über alle 280 UEX-Schiffe: UEX schreibt „C.O.
    # Mustang Alpha", erkul `cnou_mustang_alpha`; „Greycat PTV" heißt dort
    # `gama_ptv`, „Esperia Blade" ist `vncl_blade`. Weder Wortanfang noch
    # Zusammenziehung greifen da — es sind verschiedene Namen für dieselbe
    # Firma.
    #
    # ⚠ **Nur das erste Wort, und nur eines.** Der Rest muss vollständig
    # passen, sonst würde `aegs_gladius_valiant` auch auf eine schlichte
    # Gladius passen. Und weil der Treffer schwächer bewertet wird, gewinnt
    # bei zwei Kandidaten weiterhin der mit dem passenden Hersteller.
    without_maker = False
    score = 0
    for where, word in enumerate(own):
        if word in wanted:
            score += 3
        elif any(g.startswith(word) for g in wanted):
            # Herstellerkürzel als Wortanfang: `drak` steht für „Drake".
            score += 2
        elif any(_is_abbrev(word, g) for g in wanted):
            # ⭐⭐ **Kürzel mit fehlendem Vokal in der Mitte.** Erkul zieht
            # Herstellernamen zusammen, statt sie abzuschneiden: `aegs` für
            # „Aegis", `anvl` für „Anvil". Ein Vergleich auf Wortanfang findet
            # das nie — `aegs` ist kein Anfang von `aegis`, der Buchstabe `i`
            # fehlt mittendrin.
            #
            # ⚠ Eine Hersteller-Tabelle löst das NICHT, obwohl erkul eine
            # mitliefert: Dort tragen **fünf** verschiedene Kürzel den Namen
            # „Aegis Dynamics" (`fski`, `mxox`, `prar`, `aeg`, `tras`) — und
            # `aegs`, das die Schiffe benutzen, ist nicht darunter. Gemessen
            # am 06.09.2026, nachdem „Aegis Gladius Valiant" als „fliegt im
            # Spiel noch nicht" gemeldet wurde.
            score += 1
        elif chain and word in chain:
            # ⚠ Der umgekehrte Fall: erkul schreibt `alphawolf` **zusammen**,
            # der Hangar führt „L-22 Alpha Wolf" getrennt. Dafür braucht es die
            # Wörter in ihrer **Reihenfolge** — aus einer Menge verkettet käme
            # „alphakrigl22wolf" heraus, und darin steht `alphawolf` nicht.
            # Genau daran ist der erste Anlauf gescheitert, und Prüfung 139 hat
            # es gefangen.
            score += 1
        elif where == 0 and len(own) > 1 and len(word) <= 5:
            # Das Herstellerkürzel passt zu keinem Wort — hingenommen, aber
            # ohne Punkte. Ein Treffer mit passendem Hersteller schlägt diesen
            # damit immer.
            without_maker = True
        else:
            return 0
    # ⚠ Ohne den Hersteller muss mindestens **ein** eigenes Wort übrig sein,
    # das wirklich getroffen hat — sonst passt `gama_ptv` auf jeden Namen mit
    # drei Buchstaben.
    if without_maker and score < 3:
        return 0
    return score


# ⭐⭐ **Handzuordnungen für die Fälle, die kein Verfahren löst.**
#
# Am 06.09.2026 wurden alle 265 Schiffe einzeln durchgeprüft. 220 fanden ihre
# Daten von selbst, 35 sind Konzepte — von den zehn übrigen ließen sich fünf
# **nicht** durch bessere Regeln retten, weil die Namen schlicht verschieden
# sind:
#
# | im Werkzeug | bei erkul | woran es liegt |
# |---|---|---|
# | Aegis Hammerhead | `aegs_hammerhead_gs` | Zusatz, den es hier nicht gibt |
# | Aegis Idris-P | `aegs_idris_p` | Bindestrich: `idrisp` gegen `idris`+`p` |
# | Aopoa San tok.Yāi | `xnaa_santokyai` | anderer Hersteller, zusammengeschrieben |
#
# ⚠⚠ **Warum eine Liste und keine klügere Regel.** Zwei Anläufe, das Verfahren
# zu verallgemeinern, haben mehr zerstört als repariert: Ein Bindestrich, der
# beide Schreibweisen erzeugt, rettet die Idris — und bricht die F7C-M Super
# Hornet, weil `f7c_mk2` und `f7cm_mk2` dann gleich gut passen. Gemessen fiel
# die Trefferquote von 220 auf unter 200, mit Ausfällen bei Aurora, Kruger und
# Mirai. Danach: zurück auf den funktionierenden Stand, und die Handvoll Reste
# ausdrücklich benennen.
#
# Dasselbe Muster wie bei `bp-overrides.json`: Eine kurze, sichtbare Liste
# schlägt eine Regel, die niemand mehr durchschaut.
#
# ⚠ Ein Eintrag hier ist eine **Behauptung** und wird beim Patch nicht geprüft.
# Fällt eine Zuordnung auf, gehört sie geändert oder gestrichen — nicht ergänzt.
MANUAL_MAP = {
    'aegishammerhead': 'aegs_hammerhead_gs',
    'aegishammerheadbestinshowedition': 'aegs_hammerhead_gs',
    'aegisidrisp': 'aegs_idris_p',
    'aegisidrism': 'aegs_idris_m',
    'aopoasantokyai': 'xnaa_santokyai',
    # Drei Kandidaten (`rover`, `rover_emerald`, `medivac`) — ohne Zusatz ist
    # der Rover gemeint, das ist die Grundausführung.
    'rsiursa': 'rsi_ursa_rover',
}


def _search_wordwise(listing, name, maker='', short='', maker_short=''):
    """Die beste wortweise Zuordnung — oder `''`, wenn sie nicht eindeutig ist.

    ⚠ **Bei Gleichstand wird nichts zurückgegeben.** Zwei gleich gute Treffer
    heißen, dass die Angabe nicht ausreicht; irgendeinen davon zu nehmen wäre
    geraten. Lieber „keine Daten" sagen als das falsche Schiff zeigen.
    """
    # ⚠⚠ **Der Kurzcode bleibt hier draußen** — anders als in den Stufen davor.
    # Er kann veraltet sein: Der Pledge-Export führt die „F7C-M Super Hornet
    # Mk II" unter `ANVL_F7C_M_Super_Hornet_Mk_I`. Nimmt man ihn mit, stehen
    # `mk1` **und** `mk2` in der Suchmenge, und `anvl_hornet_f7c_mk2` wird
    # genauso gut bewertet wie `anvl_hornet_f7cm_mk2` — Gleichstand, also gar
    # keine Zuordnung. Der angezeigte Name ist die verlässlichere Angabe.
    # ⚠ Die Handzuordnung zuerst — sie ist eine bewusste Entscheidung und
    # schlägt jedes Verfahren.
    manual = MANUAL_MAP.get(_slim(name))
    if manual and manual in listing:
        return manual

    seq = _words(' '.join(x for x in (maker_short, maker, name) if x))
    wanted = set(seq)
    if not wanted:
        return ''
    # ⚠ Die Wörter **in ihrer Reihenfolge** aneinandergehängt — nur so findet
    # sich `alphawolf` in „Alpha Wolf" wieder.
    chain = ''.join(seq)
    scored = []
    for ident_ in listing:
        score = _matches_wordwise(ident_, wanted, chain)
        if score:
            scored.append((score, ident_))
    if not scored:
        return ''
    scored.sort(reverse=True)
    if len(scored) > 1 and scored[0][0] == scored[1][0]:
        return ''
    return scored[0][1]


def _collect_slots(node, out):
    """Alle nutzbaren Steckplätze einsammeln, rekursiv.

    ⚠⚠ **Die Steckplätze stehen unter `vehicle.hardpoints`, NICHT unter
    `slots`.** Die beiden sehen sich ähnlich und meinen Verschiedenes:
    `hardpoints` sagt, *was hineinpasst* (`accepts`, `minSize`, `maxSize`),
    `slots` nur, *was gerade drinsteckt*. Wer `slots` liest, bekommt für jedes
    Schiff **null** Steckplätze zurück — kein Fehler, keine Meldung, einfach
    eine leere Liste. Genau das ist hier beim ersten Anlauf passiert.

    ⚠ Rekursiv bleibt es trotzdem: Bei einem Turm hängen die Waffenplätze am
    Turm-Bauteil, nicht am Rumpf. Wer nur die oberste Ebene liest, findet bei
    einem bewaffneten Schiff keine einzige Waffe.
    """
    if not isinstance(node, list):
        return
    for slot_node in node:
        if not isinstance(slot_node, dict):
            continue
        _one_slot(slot_node, out)
        for field in ('hardpoints', 'ports', 'children', 'slots'):
            _collect_slots(slot_node.get(field), out)
        part = slot_node.get('item')
        if isinstance(part, dict):
            for field in ('ports', 'hardpoints', 'slots'):
                _collect_slots(part.get(field), out)


def _one_slot(slot_node, out):
    """Einen einzelnen Steckplatz auswerten, wenn er den Spieler angeht."""
    flags = slot_node.get('flags') or {}
    if any(flags.get(f) for f in HIDDEN):
        return
    kinds = []
    for takes in (slot_node.get('accepts') or []):
        kind = (takes or {}).get('type')
        if kind in INTERESTING:
            kinds.append(kind)
    if not kinds:
        return
    size = slot_node.get('maxSize')
    if size is None:
        size = slot_node.get('minSize')
    if size is None:
        return
    for kind in kinds:
        out.append((kind, int(size)))


def _hardpoint_index(node, out, path=''):
    """Jeden Steckplatz unter seinem **vollen Pfad** ablegen.

    ⚠⚠ **Der bloße `name` reicht als Schlüssel NICHT.** Bei der Cutlass Black
    heißt der Waffenplatz in jedem der vier Gimbal-Halter gleich
    (`hardpoint_class_2`, sechsmal vergeben), und `missile_01_attach` gibt es
    ebenfalls sechsmal. Wer danach schlüsselt, überschreibt vier Waffen mit
    einer und merkt es nicht.

    ⚠ Und `portPath` hilft nicht weiter: Gemessen am 06.09.2026 hat es bei
    **allen 68** Steckplätzen der Cutlass die Länge 1 — die Verschachtelung
    steht dort gar nicht drin. Der Pfad wird deshalb hier selbst gebaut.
    """
    if not isinstance(node, list):
        return
    for slot_node in node:
        if not isinstance(slot_node, dict):
            continue
        own_name = slot_node.get('name') or slot_node.get('portName') or ''
        full = (path + '/' + own_name) if path else own_name
        if own_name:
            out.setdefault(full, slot_node)
        for field in ('hardpoints', 'ports', 'children', 'slots'):
            _hardpoint_index(slot_node.get(field), out, full)
        part = slot_node.get('item')
        if isinstance(part, dict):
            for field in ('ports', 'hardpoints', 'slots'):
                _hardpoint_index(part.get(field), out, full)


def _one_hardpoint(slot, hp_index, path):
    """Einen einzelnen Steckplatz in die Ablageform bringen — oder `None`.

    Art und Größe kommen bevorzugt aus dem **Steckplatz** (`hardpoints`), nicht
    aus dem Teil, das gerade darinsteckt: Der Platz sagt, was hineinpasst, das
    Teil nur, was zufällig ab Werk gewählt wurde. Erst wenn es zu einem Platz
    keinen Eintrag gibt — bei 26 der 94 Pfade der Cutlass, weil ein
    Raketenplatz erst durch das eingebaute Rack entsteht —, muss das Teil
    aushelfen.
    """
    desc = hp_index.get(path) or {}
    kinds = [(n or {}).get('type') for n in (desc.get('accepts') or [])]
    kinds = [a for a in kinds if a in SWAPPABLE]
    part = slot.get('item') if isinstance(slot.get('item'), dict) else None

    kind = kinds[0] if kinds else ((part or {}).get('type') or '')
    if kind not in SWAPPABLE:
        return None

    size = desc.get('maxSize')
    if size is None:
        size = desc.get('minSize')
    if size is None and part is not None:
        size = part.get('size')
    try:
        size = int(size)
    except (TypeError, ValueError):
        # ⚠ Ein Platz ohne Größe ist keine Panne — Lackierungen haben keine.
        # Er bleibt trotzdem drin: Auch eine Lackierung wird gekauft.
        size = None

    entry = {'pfad': path, 'art': kind, 'groesse': size}
    if part is not None and part.get('ref'):
        # ⚠⚠ **Die Kennung ist das Entscheidende, nicht der Name.** `ref` ist
        # dieselbe Entitäts-Kennung wie bei UEX und scmdb — nur über sie hängt
        # später ein Ladenpreis am Werksteil. Der Name steht daneben, damit
        # etwas Lesbares angezeigt werden kann, und wird nie zum Zuordnen
        # benutzt.
        entry['werk'] = {
            'ref': part['ref'],
            'name': ((part.get('i18n') or {}).get('name')
                     or part.get('className') or ''),
        }
    return entry


def _collect_loadout(node, hp_index, out, path=''):
    """Alle **tauschbaren** Steckplätze samt Werksausstattung, rekursiv.

    ⚠⚠ **Der Filter ist `kind`, nicht die Flaggen.** Erkul sagt an jedem Slot
    selbst, ob der Spieler ihn tauschen kann: `swappable` oder `fixed`. Bei der
    Cutlass Black sind das 47 gegen 47 — die festen sind Panzerung und
    Struktur. In einen festen Platz kommt nie ein anderes Teil, dort gibt es
    also auch nichts zu kaufen.

    ⚠ **Trotzdem darf bei `fixed` nicht abgebrochen werden.** `hardpoint_turret`
    ist fest, aber die beiden Gimbal-Halter darin und ihre CF-337 Panther sind
    tauschbar. Wer die Rekursion an einem festen Platz beendet, verliert genau
    die Turmwaffen — und das fällt nicht auf, weil eine kürzere Liste immer
    noch wie eine Liste aussieht.
    """
    if not isinstance(node, list):
        return
    for slot in node:
        if not isinstance(slot, dict):
            continue
        own_name = slot.get('portName') or slot.get('name') or ''
        full = (path + '/' + own_name) if path else own_name
        if own_name and slot.get('kind') == 'swappable':
            entry = _one_hardpoint(slot, hp_index, full)
            if entry:
                out.append(entry)
        _collect_loadout(slot.get('children'), hp_index, out, full)


def fetch_ship(erkul_id, path):
    """Ein einzelnes Schiff holen und auf seine Steckplätze eindampfen.

    Abgelegt werden **zwei** Sichten auf dieselbe Sache, und beide werden
    gebraucht:

    | Feld | sagt | wofür |
    |---|---|---|
    | `plaetze` | „zwei Kühlerplätze Größe 2" | passt mein Bauplan hinein? |
    | `slots` | „*dieser* Platz trägt ab Werk ColdSnap" | Auslegung und Warenkorb |

    ⚠ Die gezählte Sicht bleibt unverändert erhalten. Sie beantwortet ihre
    Frage besser als eine Einzelliste, und „passt in mein Schiff" hängt daran.
    """
    raw = _fetch('%s/%s' % (BRANCH, path), 'schiff')
    if not isinstance(raw, dict):
        return None
    found = []
    _collect_slots((raw.get('vehicle') or {}).get('hardpoints'), found)
    # Zusätzlich die belegten Plätze durchgehen: Was in einem Turm steckt,
    # bringt seine eigenen Waffenplätze mit.
    _collect_slots(raw.get('slots'), found)
    counted = {}
    for kind, size in found:
        counted[(kind, size)] = counted.get((kind, size), 0) + 1
    slot_counts = [{'art': a, 'groesse': g, 'anzahl': n}
               for (a, g), n in sorted(counted.items())]

    # ⚠⚠ **Die Werksausstattung steht in `slots`, NICHT in einem Feld
    # `default`.** Ein solches Feld gibt es nicht — nachgesehen am 06.09.2026
    # über die vollständige Struktur (`default`, `defaults`, `installed`,
    # `loadout`, `equipped`: keines vorhanden). Was drinsteckt, hängt als
    # `item` am Slot, und die tieferen Ebenen hängen an **`children`**, nicht
    # an `ports`: `ports` beschreibt wieder nur Plätze. Bei der Cutlass Black
    # stecken 90 Gegenstände in drei Ebenen — wer nur die oberste liest,
    # verliert 26 davon, darunter jede Turmwaffe und jede Rakete.
    hp_index = {}
    _hardpoint_index((raw.get('vehicle') or {}).get('hardpoints'),
                           hp_index)
    slots = []
    _collect_loadout(raw.get('slots'), hp_index, slots)

    name = ((raw.get('vehicle') or {}).get('vehicleDisplayName')
            or (raw.get('i18n') or {}).get('name') or erkul_id)
    return {'name': name, 'plaetze': slot_counts, 'slots': slots}


def add_missing(rows):
    """Die genannten Schiffe holen, soweit sie noch fehlen.

    `rows` sind Tripel `(name, maker, short)` aus dem Hangar. Gibt
    zurück, wie viele wirklich geholt wurden — `0` heißt „alles war schon da"
    **oder** „kein Netz", und beides ist in Ordnung: Was fehlt, wird beim
    nächsten Mal nachgeholt.
    """
    if AUS or not rows:
        return 0
    cat = ship_catalog()
    if not isinstance(cat, dict):
        return 0
    version = cat.get('dataVersion') or ''
    data = load()
    # ⚠ Neuer Patch → alles Alte gilt nicht mehr. Steckplätze ändern sich mit
    # einem Patch, und ein Schiff, das gestern vier Waffenplätze hatte, kann
    # heute drei haben.
    known_ships = ({} if data.get('spielversion') != version
               else dict(data.get('schiffe') or {}))

    # Der Index nennt jedes Schiff mit seiner aktuellen Datei.
    # ⚠⚠ **Zwei Sichten auf dasselbe Verzeichnis, und beide werden gebraucht.**
    # `listing` hat den geschliffenen Schlüssel (`drakironcladassault`) für
    # den Buchstabenvergleich; `roh_ids` behält die Original-Kennung
    # (`drak_ironclad_assault`), weil die wortweise Suche die **Wortgrenzen**
    # braucht. Beim ersten Anlauf gab es nur die erste Sicht — die wortweise
    # Stufe lief damit gegen ein einziges langes Wort und traf nie etwas.
    # ⚠ Einmal je Lauf geholt, dann abgelegt — siehe `_maker_table`.
    maker_abbrev = (data.get('hersteller')
                          if data.get('spielversion') == version
                          else None) or _maker_table(cat)
    listing = {}
    raw_ids = {}
    for group in (cat.get('groups') or []):
        path = group.get('indexPath')
        if not path:
            continue
        index = _fetch('%s/%s' % (BRANCH, path), 'index')
        for entry in ((index or {}).get('blobs') or []):
            if entry.get('id') and entry.get('path'):
                listing[_slim(entry['id'])] = entry
                raw_ids[entry['id']] = entry

    fetched = 0
    for row in rows:
        name, maker, short, maker_short = (list(row) + ['', '', ''])[:4]
        # Der ausgeschriebene Hersteller wird zum Kürzel, wenn keines dabei ist.
        maker_short = maker_short or maker_abbrev.get(_slim(maker), '')
        possible = candidates(name, maker, short, maker_short)
        if any(k in known_ships for k in possible):
            continue
        hits = next((k for k in possible if k in listing), '')
        source = listing
        if not hits:
            hits = _search_wordwise(raw_ids, name, maker, short, maker_short)
            source = raw_ids
            # Abgelegt wird immer unter dem geschliffenen Schlüssel, damit
            # `ident()` beide Wege gleich behandelt.
            key = _slim(hits) if hits else ''
        else:
            key = hits
        if not hits or key in known_ships:
            continue
        one = fetch_ship(key, source[hits]['path'])
        if one:
            # ⚠ Die Original-Kennung bleibt am Eintrag: `ident()` braucht sie
            # für dieselbe wortweise Suche gegen die Ablage.
            one['id'] = hits
            known_ships[key] = one
            fetched += 1

    if fetched or data.get('spielversion') != version:
        # ⚠ `maker_abbrev`, **nicht** `maker` — das ist die
        # Schleifenvariable aus dem Schiffs-Tupel und wäre hier eine
        # Zeichenkette, wo ein Wörterbuch erwartet wird.
        _store.save({'spielversion': version, 'schiffe': known_ships,
                         'hersteller': maker_abbrev})
    return fetched


def slot_counts(name, maker='', short='', maker_short=''):
    """Die Steckplätze eines Schiffs — oder `[]`, wenn es unbekannt ist."""
    key = ident(name, maker, short, maker_short)
    if not key:
        return []
    return ((load().get('schiffe') or {}).get(key) or {}).get('plaetze') or []


def hardpoints(name, maker='', short='', maker_short=''):
    """Die **einzelnen** tauschbaren Steckplätze eines Schiffs.

    Je Eintrag `pfad`, `art`, `groesse` und — wenn ab Werk etwas darinsteckt —
    `werk` mit `ref` und `name`. Leere Liste heißt „keine Daten"; ob das Schiff
    überhaupt bekannt ist, sagt `knows()`.

    ⚠ Das ist **nicht** `slot_counts()`. Dort steht die gezählte Übersicht („zwei
    Kühlerplätze Größe 2"), hier jeder Platz einzeln mit seiner Kennung. Nur
    hier lässt sich ein Teil einem Platz zuordnen.
    """
    key = ident(name, maker, short, maker_short)
    if not key:
        return []
    entry = (load().get('schiffe') or {}).get(key) or {}
    return entry.get('slots') or []


def stock_loadout(name, maker='', short='', maker_short=''):
    """Was ab Werk in diesem Schiff steckt — je Teil einmal, mit Anzahl.

    Zurück kommen Einträge `{'ref', 'name', 'art', 'groesse', 'anzahl'}`,
    häufigste zuerst. Gezählt wird über die **Kennung**, nicht über den Namen:
    Vier Gimbal-Halter desselben Typs sind ein Posten mit `anzahl: 4`.

    ⚠ Es ist die **Standard**-Ausstattung. Was in einem angetroffenen Schiff
    wirklich steckt, kann jemand getauscht haben — die Anzeige sagt deshalb
    „ab Werk steckt hier … drin", nie „in diesem Schiff liegt …".
    """
    counted = {}
    for slot_node in hardpoints(name, maker, short, maker_short):
        stock = slot_node.get('werk') or {}
        ref = stock.get('ref')
        if not ref:
            continue
        entry = counted.setdefault(ref, {
            'ref': ref, 'name': stock.get('name') or '',
            'art': slot_node.get('art') or '', 'groesse': slot_node.get('groesse'),
            'anzahl': 0})
        entry['anzahl'] += 1
    out = list(counted.values())
    out.sort(key=lambda x: (-x['anzahl'], (x['name'] or '').lower()))
    return out


def fits(kind, size, name, maker='', short='', maker_short=''):
    """Passt ein Teil dieser Art und Größe in dieses Schiff?

    Gibt die Anzahl passender Steckplätze zurück, `0` wenn keiner passt.

    ⚠ **`0` heißt nicht immer „passt nicht".** Ist das Schiff gar nicht in der
    Ablage — weil es im Spiel noch nicht existiert oder noch nicht geholt
    wurde —, kommt ebenfalls `0`. Die Anzeige muss beide Fälle auseinander
    halten; `knows()` sagt, welcher vorliegt.
    """
    if not kind or size is None:
        return 0
    try:
        size = int(size)
    except (TypeError, ValueError):
        return 0
    total = 0
    for slot_node in slot_counts(name, maker, short, maker_short):
        if slot_node.get('art') == kind and int(slot_node.get('groesse', -1)) == size:
            total += int(slot_node.get('anzahl') or 0)
    return total


def knows(name, maker='', short='', maker_short=''):
    """Liegen für dieses Schiff überhaupt Steckplatz-Daten vor?"""
    return bool(ident(name, maker, short, maker_short))


def matching_ships(kind, size, ships):
    """In welche dieser Schiffe passt ein Teil dieser Art und Größe?

    `ships` sind die Einträge aus `fleet.load()['schiffe']`. Zurück kommen
    Paare `(Schiffsname, Anzahl Steckplätze)`, die meisten Plätze zuerst.

    ⭐ **Das ist die Auskunft, die keine fremde Seite geben kann** — nicht,
    weil die Daten geheim wären, sondern weil keine Seite weiß, welche Schiffe
    *dir* gehören und welche Baupläne *du* hast. Erkul kennt die Schiffe, der
    Watcher kennt den Spieler; erst zusammen ergibt es eine Antwort.

    ⚠ Eine leere Liste heißt „passt nirgends hinein" — **nicht** „keine Daten".
    Wer beides gleich behandelt, sagt jemandem mit leerem Hangar, sein Teil
    sei nutzlos. Die Anzeige prüft deshalb vorher, ob überhaupt ein Schiff
    eingetragen ist.
    """
    if not kind or size is None:
        return []
    out = []
    for s in (ships or []):
        count = fits(kind, size, s.get('name') or '',
                       s.get('hersteller') or '', s.get('kurz') or '',
                       s.get('hkurz') or '')
        if count:
            out.append((s.get('name') or '', count))
    out.sort(key=lambda x: (-x[1], x[0].lower()))
    return out
