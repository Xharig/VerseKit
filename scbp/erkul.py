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

    BlastChill  →  94ea5bb5-070c-4c75-b90d-66c26c38bb2a   (in laeden.py dokumentiert)
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

Geholt wird deshalb **auf Zuruf**: Steht ein Schiff im Hangar (`hangar.py`) und
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
import urllib.request
import zlib

from . import fehler, uex
from .katalog import AUS, KENNUNG

# Der Zweig, aus dem gelesen wird. PTU führt eigene Daten, die den Spieler auf
# LIVE nur verwirren würden.
ZWEIG = 'LIVE'
BASIS = 'https://cdn.erkul.games'

CACHE = 'erkul-schiffe.json'
# ⚠ 2 seit v3.19.0-rc4: Jeder Eintrag trägt jetzt seine **Original-Kennung**
# (`id`, z. B. `anvl_arrow`). Ohne sie kennt die Ablage nur den geschliffenen
# Schlüssel `anvlarrow` — ein einziges Wort, in dem die wortweise Zuordnung
# keine Wortgrenzen mehr findet. Eine Ablage aus rc1–rc3 sieht deshalb aus wie
# „keine Steckplatz-Daten", obwohl die Daten da sind. Aufgefallen ist das am
# Anleitungsbild, das mit einer kopierten alten Ablage lief.
FORMAT = 2

# Notfrist. Maßgeblich ist die Spielversion aus `catalog.bin` — diese Frist
# greift nur, falls sich die gar nicht ermitteln lässt.
HALTBAR = 30 * uex.TAG

# ⭐⭐ **`patch_bindet=True`: Der Patch entscheidet, nicht die Uhr.**
# Steckplätze ändern sich mit einem Spiel-Patch und sonst nie. Eine Zeitfrist
# würde denselben Stand alle 30 Tage wegwerfen und neu holen — Abrufe, die
# niemandem nützen und die erkul bezahlt.
_ablage = uex.Ablage(CACHE, format_nr=FORMAT, haltbar=HALTBAR,
                     patch_bindet=True)

# ⚠ Steckplätze, die den Spieler nichts angehen. `invisible` und `uneditable`
# heißt: Das Spiel zeigt sie nicht und lässt sie nicht tauschen — ein Bauplan
# kann dort also nie landen. Sie trotzdem anzuzeigen hieße, eine Möglichkeit zu
# behaupten, die es nicht gibt.
VERBORGEN = ('invisible', 'uneditable')

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
INTERESSANT = frozenset((
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


def _holen(pfad, stelle):
    """Eine erkul-Datei abrufen und auspacken — oder `None`.

    Wirft **nie**: Ohne Netz läuft das Werkzeug weiter wie vorher, genau wie
    bei UEX. Der Grund steht dort ausführlich.
    """
    if AUS:
        return None
    adresse = '%s/%s' % (BASIS, pfad.lstrip('/'))
    try:
        anfrage = urllib.request.Request(
            adresse, headers={'User-Agent': KENNUNG})
        with urllib.request.urlopen(anfrage, timeout=uex.ZEITLIMIT) as antwort:
            roh = antwort.read()
        # ⚠ `-15` = raw deflate, ohne zlib-Kopf. Mit `zlib.decompress(roh)`
        # allein scheitert es an genau dieser Stelle — der Kopf fehlt, weil
        # erkul die Dateien schon gepackt ablegt statt sie zu übertragen.
        return json.loads(zlib.decompress(roh, -15).decode('utf-8'))
    except Exception as ausnahme:
        fehler.merken('erkul.holen.' + stelle, ausnahme)
        return None


def katalog():
    """Das Inhaltsverzeichnis — Spielversion und aktuelle Dateinamen."""
    return _holen('%s/catalog.bin' % ZWEIG, 'katalog')


def _hersteller_tabelle(kat):
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
    pfad = next((f.get('path') for f in (kat.get('families') or [])
                 if (f.get('path') or '').startswith('manufacturers')), '')
    if not pfad:
        return {}
    liste = _holen('%s/%s' % (ZWEIG, pfad), 'hersteller')
    raus = {}
    for eintrag in (liste or []):
        kuerzel = (eintrag.get('className') or '').strip()
        klar = ((eintrag.get('i18n') or {}).get('name') or '').strip()
        if kuerzel and klar:
            raus[_schlank(klar)] = kuerzel
    return raus


def laden():
    return _ablage.laden() or {}


def alter():
    return _ablage.alter()


def spielversion():
    """Die Spielversion, zu der die Ablage gehört — oder `''`."""
    return laden().get('spielversion') or ''


def kandidaten(name, hersteller='', kurz='', hkurz=''):
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
    raus = []
    # ⚠ Das **Kürzel** des Herstellers vor seinem ausgeschriebenen Namen:
    # Erkul führt „Roberts Space Industries" als `rsi`. Ohne diese Zeile fand
    # die Ursa Medivac keinen Anschluss, obwohl `rsi_ursa_medivac` existiert.
    for versuch in (kurz, '%s %s' % (hkurz, name), name,
                    '%s %s' % (hersteller, name)):
        schlank = _schlank(versuch)
        if schlank and schlank not in raus:
            raus.append(schlank)
    return raus


def kennung(name, hersteller='', kurz='', hkurz=''):
    """Ein Schiff → erkuls Kennung, oder `''`.

    Drei Stufen, in dieser Reihenfolge: Buchstabenvergleich über den Kurznamen,
    dann über den angezeigten Namen, dann **wortweise**. Die dritte kostet
    etwas mehr, greift aber genau dort, wo die ersten beiden scheitern — und
    das ist bei jedem vierten Schiff der Fall.
    """
    abgelegt = laden()
    bekannt = abgelegt.get('schiffe') or {}
    # ⚠ Auch hier: „Anvil Aerospace" muss zu `anvl` werden, sonst findet ein
    # von Hand eingetragenes Schiff seine eigenen Daten nicht wieder.
    hkurz = hkurz or (abgelegt.get('hersteller') or {}).get(
        _schlank(hersteller), '')
    for schlank in kandidaten(name, hersteller, kurz, hkurz):
        if schlank in bekannt:
            return schlank
    # ⚠ Wortweise wird gegen die **Original-Kennungen** gesucht (`id`), nicht
    # gegen die geschliffenen Schlüssel — sonst fehlen die Wortgrenzen.
    nach_id = dict((v.get('id') or k, k) for k, v in bekannt.items())
    treffer = _wortweise_suchen(nach_id, name, hersteller, kurz, hkurz)
    return nach_id.get(treffer, '') if treffer else ''


def _schlank(text):
    """`ANVL_F7C_Hornet` → `anvlf7chornet` — alles weg außer Buchstaben/Ziffern.

    ⚠ Bewusst **ohne** Trennzeichen: Der Export schreibt `L_22_Alpha_Wolf`,
    erkul `l22alphawolf`. Wer die Unterstriche behält, vergleicht zwei
    Schreibweisen derselben Sache und findet nichts.
    """
    return re.sub(r'[^a-z0-9]', '', (text or '').lower())


# Römische Zahlen, wie sie in Schiffsnamen vorkommen. Weiter als V geht es
# nicht — es gibt kein „Mk VI".
_ROEMISCH = {'i': '1', 'ii': '2', 'iii': '3', 'iv': '4', 'v': '5'}


def _woerter(text):
    """Ein Name → seine Wörter, mit `Mk II` zu `mk2` vereinheitlicht.

    ⚠ Punkte und Bindestriche fallen **vor** dem Zerlegen weg, nicht danach:
    Aus `F7C-M` wird `f7cm` (ein Wort, wie bei erkul), nicht `f7c` + `m`.
    """
    t = (text or '').lower().replace('.', '').replace('-', '')
    raus = []
    for wort in re.split(r'[^a-z0-9]+', t):
        if not wort:
            continue
        treffer = re.fullmatch(r'mk\s*([ivx]+|\d+)', wort)
        if treffer:
            zahl = treffer.group(1)
            raus.append('mk' + _ROEMISCH.get(zahl, zahl))
            continue
        # „Mk" und „II" getrennt geschrieben — zusammenziehen.
        if wort in _ROEMISCH and raus and raus[-1] == 'mk':
            raus[-1] = 'mk' + _ROEMISCH[wort]
            continue
        raus.append(wort)
    return raus


def _passt_wortweise(erkul_id, gesucht, kette=''):
    """Deckt `gesucht` alle Wörter dieser erkul-Kennung ab? Gibt die Güte zurück.

    ⭐ **Die dritte Zuordnungsstufe** — sie fängt genau die Fälle, an denen
    Buchstabenvergleich scheitert, und das sind keine Ausnahmen:

    | im Hangar | bei erkul | woran es scheiterte |
    |---|---|---|
    | `Drake Ironclad Assault` | `drak_ironclad_assault` | Hersteller ausgeschrieben |
    | `F7C-M Super Hornet Mk II` | `anvl_hornet_f7cm_mk2` | römische Zahl, andere Wortfolge |

    ⚠ **Ein erkul-Wort darf auch Anfang eines gesuchten Worts sein** — genau so
    verhält sich jedes Herstellerkürzel (`drak` → Drake, `aegs` → Aegis,
    `anvl` → Anvil). Ohne das bleibt der halbe Hangar ohne Zuordnung.

    ⚠ Und **umgekehrt gilt es nicht**: `gesucht` darf mehr Wörter haben als
    erkul (`Super` steht nur im Hangar-Namen), aber jedes erkul-Wort muss
    vorkommen. Sonst würde `anvl_hornet_f7cm` auch auf eine Mk II passen.
    """
    eigene = _woerter(erkul_id)
    guete = 0
    for wort in eigene:
        if wort in gesucht:
            guete += 3
        elif any(g.startswith(wort) for g in gesucht):
            # Herstellerkürzel: `drak` steht für „Drake", `aegs` für „Aegis".
            guete += 2
        elif kette and wort in kette:
            # ⚠ Der umgekehrte Fall: erkul schreibt `alphawolf` **zusammen**,
            # der Hangar führt „L-22 Alpha Wolf" getrennt. Dafür braucht es die
            # Wörter in ihrer **Reihenfolge** — aus einer Menge verkettet käme
            # „alphakrigl22wolf" heraus, und darin steht `alphawolf` nicht.
            # Genau daran ist der erste Anlauf gescheitert, und Prüfung 139 hat
            # es gefangen.
            guete += 1
        else:
            return 0
    return guete


def _wortweise_suchen(verzeichnis, name, hersteller='', kurz='', hkurz=''):
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
    folge = _woerter(' '.join(x for x in (hkurz, hersteller, name) if x))
    gesucht = set(folge)
    if not gesucht:
        return ''
    # ⚠ Die Wörter **in ihrer Reihenfolge** aneinandergehängt — nur so findet
    # sich `alphawolf` in „Alpha Wolf" wieder.
    kette = ''.join(folge)
    bewertet = []
    for kennung_ in verzeichnis:
        guete = _passt_wortweise(kennung_, gesucht, kette)
        if guete:
            bewertet.append((guete, kennung_))
    if not bewertet:
        return ''
    bewertet.sort(reverse=True)
    if len(bewertet) > 1 and bewertet[0][0] == bewertet[1][0]:
        return ''
    return bewertet[0][1]


def _plaetze_sammeln(knoten, raus):
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
    if not isinstance(knoten, list):
        return
    for platz in knoten:
        if not isinstance(platz, dict):
            continue
        _einen_platz(platz, raus)
        for feld in ('hardpoints', 'ports', 'children', 'slots'):
            _plaetze_sammeln(platz.get(feld), raus)
        teil = platz.get('item')
        if isinstance(teil, dict):
            for feld in ('ports', 'hardpoints', 'slots'):
                _plaetze_sammeln(teil.get(feld), raus)


def _einen_platz(platz, raus):
    """Einen einzelnen Steckplatz auswerten, wenn er den Spieler angeht."""
    flaggen = platz.get('flags') or {}
    if any(flaggen.get(f) for f in VERBORGEN):
        return
    arten = []
    for nimmt in (platz.get('accepts') or []):
        art = (nimmt or {}).get('type')
        if art in INTERESSANT:
            arten.append(art)
    if not arten:
        return
    groesse = platz.get('maxSize')
    if groesse is None:
        groesse = platz.get('minSize')
    if groesse is None:
        return
    for art in arten:
        raus.append((art, int(groesse)))


def schiff_holen(erkul_id, pfad):
    """Ein einzelnes Schiff holen und auf seine Steckplätze eindampfen."""
    roh = _holen('%s/%s' % (ZWEIG, pfad), 'schiff')
    if not isinstance(roh, dict):
        return None
    gefunden = []
    _plaetze_sammeln((roh.get('vehicle') or {}).get('hardpoints'), gefunden)
    # Zusätzlich die belegten Plätze durchgehen: Was in einem Turm steckt,
    # bringt seine eigenen Waffenplätze mit.
    _plaetze_sammeln(roh.get('slots'), gefunden)
    gezaehlt = {}
    for art, groesse in gefunden:
        gezaehlt[(art, groesse)] = gezaehlt.get((art, groesse), 0) + 1
    plaetze = [{'art': a, 'groesse': g, 'anzahl': n}
               for (a, g), n in sorted(gezaehlt.items())]
    name = ((roh.get('vehicle') or {}).get('vehicleDisplayName')
            or (roh.get('i18n') or {}).get('name') or erkul_id)
    return {'name': name, 'plaetze': plaetze}


def nachtragen(saetze):
    """Die genannten Schiffe holen, soweit sie noch fehlen.

    `saetze` sind Tripel `(name, hersteller, kurz)` aus dem Hangar. Gibt
    zurück, wie viele wirklich geholt wurden — `0` heißt „alles war schon da"
    **oder** „kein Netz", und beides ist in Ordnung: Was fehlt, wird beim
    nächsten Mal nachgeholt.
    """
    if AUS or not saetze:
        return 0
    kat = katalog()
    if not isinstance(kat, dict):
        return 0
    version = kat.get('dataVersion') or ''
    daten = laden()
    # ⚠ Neuer Patch → alles Alte gilt nicht mehr. Steckplätze ändern sich mit
    # einem Patch, und ein Schiff, das gestern vier Waffenplätze hatte, kann
    # heute drei haben.
    bekannt = ({} if daten.get('spielversion') != version
               else dict(daten.get('schiffe') or {}))

    # Der Index nennt jedes Schiff mit seiner aktuellen Datei.
    # ⚠⚠ **Zwei Sichten auf dasselbe Verzeichnis, und beide werden gebraucht.**
    # `verzeichnis` hat den geschliffenen Schlüssel (`drakironcladassault`) für
    # den Buchstabenvergleich; `roh_ids` behält die Original-Kennung
    # (`drak_ironclad_assault`), weil die wortweise Suche die **Wortgrenzen**
    # braucht. Beim ersten Anlauf gab es nur die erste Sicht — die wortweise
    # Stufe lief damit gegen ein einziges langes Wort und traf nie etwas.
    # ⚠ Einmal je Lauf geholt, dann abgelegt — siehe `_hersteller_tabelle`.
    hersteller_kuerzel = (daten.get('hersteller')
                          if daten.get('spielversion') == version
                          else None) or _hersteller_tabelle(kat)
    verzeichnis = {}
    roh_ids = {}
    for gruppe in (kat.get('groups') or []):
        pfad = gruppe.get('indexPath')
        if not pfad:
            continue
        index = _holen('%s/%s' % (ZWEIG, pfad), 'index')
        for eintrag in ((index or {}).get('blobs') or []):
            if eintrag.get('id') and eintrag.get('path'):
                verzeichnis[_schlank(eintrag['id'])] = eintrag
                roh_ids[eintrag['id']] = eintrag

    geholt = 0
    for satz in saetze:
        name, hersteller, kurz, hkurz = (list(satz) + ['', '', ''])[:4]
        # Der ausgeschriebene Hersteller wird zum Kürzel, wenn keines dabei ist.
        hkurz = hkurz or hersteller_kuerzel.get(_schlank(hersteller), '')
        moegliche = kandidaten(name, hersteller, kurz, hkurz)
        if any(k in bekannt for k in moegliche):
            continue
        treffer = next((k for k in moegliche if k in verzeichnis), '')
        quelle = verzeichnis
        if not treffer:
            treffer = _wortweise_suchen(roh_ids, name, hersteller, kurz, hkurz)
            quelle = roh_ids
            # Abgelegt wird immer unter dem geschliffenen Schlüssel, damit
            # `kennung()` beide Wege gleich behandelt.
            schluessel = _schlank(treffer) if treffer else ''
        else:
            schluessel = treffer
        if not treffer or schluessel in bekannt:
            continue
        eins = schiff_holen(schluessel, quelle[treffer]['path'])
        if eins:
            # ⚠ Die Original-Kennung bleibt am Eintrag: `kennung()` braucht sie
            # für dieselbe wortweise Suche gegen die Ablage.
            eins['id'] = treffer
            bekannt[schluessel] = eins
            geholt += 1

    if geholt or daten.get('spielversion') != version:
        # ⚠ `hersteller_kuerzel`, **nicht** `hersteller` — das ist die
        # Schleifenvariable aus dem Schiffs-Tupel und wäre hier eine
        # Zeichenkette, wo ein Wörterbuch erwartet wird.
        _ablage.sichern({'spielversion': version, 'schiffe': bekannt,
                         'hersteller': hersteller_kuerzel})
    return geholt


def plaetze(name, hersteller='', kurz='', hkurz=''):
    """Die Steckplätze eines Schiffs — oder `[]`, wenn es unbekannt ist."""
    schluessel = kennung(name, hersteller, kurz, hkurz)
    if not schluessel:
        return []
    return ((laden().get('schiffe') or {}).get(schluessel) or {}).get('plaetze') or []


def passt(art, groesse, name, hersteller='', kurz='', hkurz=''):
    """Passt ein Teil dieser Art und Größe in dieses Schiff?

    Gibt die Anzahl passender Steckplätze zurück, `0` wenn keiner passt.

    ⚠ **`0` heißt nicht immer „passt nicht".** Ist das Schiff gar nicht in der
    Ablage — weil es im Spiel noch nicht existiert oder noch nicht geholt
    wurde —, kommt ebenfalls `0`. Die Anzeige muss beide Fälle auseinander
    halten; `kennt()` sagt, welcher vorliegt.
    """
    if not art or groesse is None:
        return 0
    try:
        groesse = int(groesse)
    except (TypeError, ValueError):
        return 0
    summe = 0
    for platz in plaetze(name, hersteller, kurz, hkurz):
        if platz.get('art') == art and int(platz.get('groesse', -1)) == groesse:
            summe += int(platz.get('anzahl') or 0)
    return summe


def kennt(name, hersteller='', kurz='', hkurz=''):
    """Liegen für dieses Schiff überhaupt Steckplatz-Daten vor?"""
    return bool(kennung(name, hersteller, kurz, hkurz))


def passende_schiffe(art, groesse, schiffe):
    """In welche dieser Schiffe passt ein Teil dieser Art und Größe?

    `schiffe` sind die Einträge aus `hangar.laden()['schiffe']`. Zurück kommen
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
    if not art or groesse is None:
        return []
    raus = []
    for s in (schiffe or []):
        anzahl = passt(art, groesse, s.get('name') or '',
                       s.get('hersteller') or '', s.get('kurz') or '',
                       s.get('hkurz') or '')
        if anzahl:
            raus.append((s.get('name') or '', anzahl))
    raus.sort(key=lambda x: (-x[1], x[0].lower()))
    return raus
