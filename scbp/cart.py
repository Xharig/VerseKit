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
Was ich für mein Schiff noch besorgen muss — und ob ich es kaufe oder baue.

## Die Frage

Wer sein Schiff umbaut, hat eine Wunsch-Auslegung im Kopf: hier ein besserer
Kühler, dort eine andere Waffe. Ab Werk steckt etwas anderes drin. Der
Unterschied ist die Einkaufsliste.

    ab Werk:  ColdSnap (S2)          <- erkul
    gewünscht: BlastChill (S2)       <- die gespeicherte Auslegung
    ─────────────────────────────────
    Warenkorb: 1× BlastChill

## ⭐⭐ Und für jeden Posten gibt es ZWEI Wege

Das ist der Punkt, an dem dieses Werkzeug mehr kann als die Auslegungs-Seiten
im Netz: Es kennt die Baupläne des Spielers. Also steht an jedem Posten nicht
nur ein Preis, sondern beides nebeneinander::

    BlastChill    kaufen  22.730 aUEC bei Dumper's Depot · Area18
                  bauen    9.410 aUEC Material · 4 min 20 s

**Die Wahl trifft der Spieler, Posten für Posten.** Nicht das Programm: Wer
gerade kein Erz hat, kauft trotz des besseren Preises, und wer Zeit hat, baut.
Beide Zahlen stehen da, entschieden wird von Hand — und die Summe rechnet mit
dem, was gewählt wurde.

## ⚠⚠ Vier Zustände, vier Sätze — nicht ein „geht nicht"

Die teuerste Verwechslung dieses Projekts ist, „keine Daten" und „passt nicht"
gleich aussehen zu lassen: Beides ist eine leere Liste. Am 06.09.2026 stand
deshalb bei **jedem** Bauplan „passt in keines deiner Schiffe", obwohl nur die
Steckplatz-Daten fehlten.

Hier wird das auseinandergehalten:

| Kennung | heißt | was der Spieler liest |
|---|---|---|
| `NO_DATA` | zum Schiff fehlen die Steckplätze | „für dieses Schiff liegen keine Daten vor" |
| `NOTHING_OPEN` | Auslegung = Werksausstattung | „nichts zu besorgen — alles ab Werk" |
| `NO_PRICE` | Teil bekannt, UEX hat keinen Preis | Feld bleibt leer, keine Behauptung |
| `NO_RECIPE` | dafür gibt es keinen Bauplan | „nur kaufbar" |

`NO_PRICE` und `NO_RECIPE` hängen am einzelnen Posten, die anderen beiden
am Schiff.

## ⚠ Zugeordnet wird über die Kennung, nie über den Namen

Erkuls `ref`, UEX' `uuid` und die `productEntityClass` der Rezeptdaten sind
dieselbe Entitäts-Kennung. Über Namen ist es in diesem Projekt zweimal
schiefgegangen (`Gold` holte `Golden Medmon` mit). Auch der Bauplan zu einem
Teil wird deshalb über die Kennung gesucht, nicht über die Beschriftung.

## Die Kaufroute

`routes.py` beantwortet eine **andere** Frage — „wo kaufe ich billig und
verkaufe teuer", über UEX' fertige Handelsfahrten. Hier geht es darum, eine
feste Liste mit möglichst wenigen Stopps abzuklappern. Es gibt zwischen beiden
auch keine gemeinsame Schlüsselgröße: `routes.py` rechnet ausschließlich über
Terminal-Nummern, `shops.py` legt nur Namen ab.

Gerechnet wird deshalb hier, mit einer Überdeckung: Es gewinnt der Ort, der die
meisten offenen Posten deckt, bei Gleichstand der billigere. Das ist dieselbe
Auskunft, die erkul als „1 shop · 1 stop" zeigt.
"""

import re

from . import erkul, fehler

# Zustände eines ganzen Warenkorbs.
NO_DATA = 'keine_daten'
NOTHING_OPEN = 'nichts_offen'
OPEN = 'offen'

# Zustände eines einzelnen Weges an einem Posten.
KNOWN = 'bekannt'
NO_PRICE = 'kein_preis'
NO_RECIPE = 'kein_rezept'
NOT_CHECKED = 'nicht_geprueft'

# Die beiden Wege, zwischen denen der Spieler wählt.
BUY = 'kaufen'
CRAFT = 'bauen'

# Was für ein Posten auf der Rechnung steht — ein Einzelteil oder ein ganzes
# Schiff. ⚠ Beide brauchen dieselbe Form (`kauf`, `bau`, `weg`), damit `total()`
# und `route()` sie ohne Sonderfall verarbeiten.
PART = 'teil'
SHIP = 'schiff'

# Woher ein Schiff kommt: schon im Hangar oder erst auf der Wunschliste. Der
# Unterschied entscheidet, ob das Schiff selbst als Posten zählt — was man hat,
# muss man nicht kaufen.
HANGAR = 'hangar'
WISHLIST = 'wunsch'

# ⭐⭐ **Der Merkzettel: Einzelteile ohne Schiff.** Bis v3.20.0 fuehrte jeder Weg
# zum Farmen ueber ein Schiff — man legte eines auf die Wunschliste, waehlte
# Steckplaetze, und daraus entstand die Materialliste. Fuer einen Helm, eine
# Waffe oder ein Ruestungsteil gab es diesen Weg **gar nicht**.
#
# Gemeldet von Haldjas am 06.09.2026: *„‚What to farm' ist irgendwie bisschen
# unnoetig komplex — man geht da rein, wird dann zu ‚still missing' geschickt
# und weiss dann aber nicht so genau, was man machen soll. […] Es waere naemlich
# auch ganz nuetzlich, wenn man nicht nur Schiffsteile, sondern auch
# Ruestungen/Waffen fuer FPS hinzufuegen koennte zum Workshop, sind ja immerhin
# auch Blueprints, die Ressourcen brauchen."*
#
# Ein Merkzettel-Posten hat kein Schiff und keine Position — er ist einfach
# etwas, das man bauen oder kaufen will. Alles andere (Preis, Material, Route)
# rechnet sich genauso wie bei einem Schiffsteil.
NOTEPAD = 'merkzettel'

# Woher ein Teil überhaupt zu bekommen ist. ⚠ Das ist keine Feinheit: Militär
# ist **nicht kaufbar, aber herstellbar** — wer nur die Ladenware zeigt, lässt
# genau die Teile weg, für die man Baupläne sammelt.
BUYABLE = 'kaufbar'
CRAFTABLE = 'herstellbar'
BOTH = 'beides'


# ⚠⚠ **UEX-Warengruppe → Steckplatz-Art. Die einzige Übersetzungstabelle hier
# — und sie ist leider nötig.**
#
# Zwischen Bauplan-Art und Steckplatz-Art braucht es keine: scmdb und erkul
# benennen die Arten gleich (`WeaponGun`, `Cooler`, …, geprüft über alle 1.605
# Gegenstände). Diese Tabelle überbrückt etwas anderes: UEX gliedert seinen
# **Ladenkatalog** nach Warengruppen (`Coolers`, `Power Plants`), erkul die
# Steckplätze nach Bauart (`Cooler`, `PowerPlant`). Ohne die Brücke lässt sich
# zu einem Kühlerplatz nicht sagen, welche kaufbaren Teile hineinpassen.
#
# ⚠ Sie darf **nur die Auswahlliste filtern**, nie eine Zuordnung entscheiden.
# Zugeordnet wird ausschließlich über die Entitäts-Kennung. Veraltet die
# Tabelle, weil UEX eine Warengruppe umbenennt, wird die Liste an einem Platz
# leer — der Rückfall unten fängt das ab, und es geht nichts kaputt.
GROUP_TO_KIND = {
    'Coolers': 'Cooler',
    'Power Plants': 'PowerPlant',
    'Shield Generators': 'Shield',
    'Quantum Drives': 'QuantumDrive',
    'Radar': 'Radar',
    'Jump Modules': 'JumpDrive',
    'Guns': 'WeaponGun',
    'Turrets': 'Turret',
    'Point Defense Cannon': 'Turret',
    'Missiles': 'Missile',
    'Missile Racks': 'MissileLauncher',
    'Torpedo Tubes': 'MissileLauncher',
    'Bombs': 'Bomb',
    'Bomb Racks': 'BombLauncher',
    'Mining Laser Heads': 'WeaponMining',
    'Mining Modules': 'MiningModifier',
    'Salvage Beams': 'SalvageHead',
    'Scraper Beams': 'SalvageHead',
    'Tractor Beams': 'TractorBeam',
    'Batteries': 'Battery',
    'Container': 'Container',
    'Flight Blade': 'FlightController',
    'Gravity Generator': 'GravityGenerator',
}


# ⚠⚠ **Rezept-Art → Steckplatz-Art, der Rückfall wenn der Katalog schweigt.**
#
# Der Bauplan-Katalog kennt nur **738** der 1.597 herstellbaren Dinge — er
# entsteht aus den Belohnungs-Töpfen, und was in keiner Mission steckt, steht
# dort nicht. Gemessen am 06.09.2026 fiel dadurch der Militär-Quantenantrieb
# **Crossfield** aus der Auswahl, obwohl es einen Bauplan dafür gibt.
#
# Die Rezeptdaten selbst tragen die Angabe aber mit: `type` sagt die Gattung
# (`quantumdrive`), `subtype` bei Komponenten die Größe (`size2`).
#
# ⚠ Bei **Waffen** steht in `subtype` die Waffenart (`laser`, `ballistic`),
# keine Größe — dort trägt nur der Katalog. Das ist die verbleibende Lücke und
# kein Fehler: Lieber ein Teil weniger anbieten als eines in der falschen
# Größe.
RECIPE_TO_KIND = {
    'quantumdrive': 'QuantumDrive',
    'cooler': 'Cooler',
    'powerplant': 'PowerPlant',
    'shield': 'Shield',
    'radar': 'Radar',
    'mininglaser': 'WeaponMining',
    'tractorbeam': 'TractorBeam',
}

# `size2` → 2. Nur diese Form, nichts geraten.
_SIZE_FROM_SUBTYPE = re.compile(r'^size(\d+)$')


def _from_recipe(entry):
    """Art und Größe aus den Rezeptdaten — oder `(None, None)`."""
    kind = RECIPE_TO_KIND.get((entry.get('art') or '').lower())
    if not kind:
        return None, None
    found = _SIZE_FROM_SUBTYPE.match((entry.get('unterart') or '').lower())
    return kind, (int(found.group(1)) if found else None)


def _grade_letter(value):
    """Die Güte als Buchstabe — `2` wird zu `B`.

    ⚠⚠ **Der Bauplan-Katalog führt die Güte als Zahl, das Spiel als
    Buchstabe.** Ungewandelt stand in der Auswahlliste „2 · Tarnung" statt
    „B · Tarnung", und zwar bei **224 von 304** Teilen — gemischt mit denen,
    die ihren Buchstaben aus UEX bekamen. Zwei Schreibweisen für dieselbe
    Angabe in einer Liste sind schlimmer als eine fehlende: Wer „A" und „1"
    nebeneinander sieht, hält es für zwei verschiedene Dinge.

    Die Zuordnung ist an echten Daten abgelesen, nicht angenommen: `Bolt`
    stand vor der Umstellung als **B** da und kommt aus dem Katalog als **2**,
    `Huracan` ebenso. Also 1=A, 2=B, 3=C, 4=D.

    ⚠ Alles andere geht **unverändert** durch. Steht dort schon ein Buchstabe,
    bleibt er; steht etwas Unerwartetes darin, wird es gezeigt und nicht
    stillschweigend verworfen — eine Zahl 7 wäre ein Hinweis darauf, dass sich
    die Quelle geändert hat, und den will man sehen.
    """
    text = str(value or '').strip()
    return {'1': 'A', '2': 'B', '3': 'C', '4': 'D'}.get(text, text)


def _craftable(kind, size):
    """Alle **herstellbaren** Teile dieser Art und Größe — Kennung → Angaben.

    ⭐⭐ **Ohne diese Quelle fehlt dem Spieler die halbe Welt, und zwar
    ausgerechnet die interessante Hälfte.** Die Auswahl speiste sich bis zum
    06.09.2026 nur aus UEX — und UEX führt **Ladenware**. Militärkomponenten
    gibt es im Laden nicht, also standen sie nirgends. Gemessen an den
    Quantenantrieben der Größe 2:

    | | UEX (kaufbar) | Spieldaten (herstellbar) |
    |---|---|---|
    | Civilian | 9 | 9 |
    | Industrial | 3 | 3 |
    | Stealth | 2 | 3 |
    | Competition | 2 | 2 |
    | **Military** | **0** | **3** |

    Der Hinweis dazu: *„ich weiß Militär ist nicht kaufbar, aber herstellbar
    ist es."* Genau so ist es — und wer Baupläne sammelt, will die zuerst
    sehen.

    ⚠ **Verknüpft wird über den Namen — hier ausnahmsweise zu Recht.** Die
    „nie über Namen"-Regel gilt für Zuordnungen über **Quellengrenzen**
    hinweg (dort holte `Gold` einmal `Golden Medmon` mit). Rezeptdaten und
    Katalog stammen dagegen beide aus derselben Quelle und benutzen dieselbe
    Namensform; `classification()` verknüpft sie längst genauso. Gemessen am
    06.09.2026: **1.592 von 1.597 (99,7 %)** finden ihre Angaben, alle davon
    mit Art und Größe.

    ⚠ Die **Kennung** bleibt trotzdem der Schlüssel des Ergebnisses. Über sie
    hängen später Ladenpreis und Rezept am Teil — der Name ist nur die
    Beschriftung.
    """
    from . import crafting, katalog
    result = {}
    try:
        values = (katalog.laden() or {}).get('bauplaene') or {}
        for entry in crafting.all_items():
            ident = entry.get('entity') or ''
            if not ident:
                continue
            traits = values.get(katalog._norm(entry.get('basis') or '')) or {}
            own_kind = traits.get('a') or ''
            own = traits.get('s')
            if not own_kind:
                # Der Katalog kennt dieses Teil nicht — dann sagen es die
                # Rezeptdaten selbst. Siehe `RECIPE_TO_KIND`.
                own_kind, from_recipe = _from_recipe(entry)
                if own is None:
                    own = from_recipe
            if (own_kind or '') != kind:
                continue
            # ⚠ Größe nur vergleichen, wenn beide Seiten eine haben — sonst
            # fällt ein Teil heraus, weil eine Angabe fehlt, nicht weil es
            # nicht passt.
            if size is not None and own is not None:
                try:
                    if int(own) != int(size):
                        continue
                except (TypeError, ValueError):
                    pass
            result[ident] = {
                'name': entry.get('basis') or entry.get('name') or '',
                'kennung': ident,
                'hersteller': (traits.get('m')
                               or entry.get('hersteller') or ''),
                'guete': _grade_letter(traits.get('g')),
                'klasse': traits.get('c') or '',
            }
    except Exception as ausnahme:
        fehler.merken('cart.craftable', ausnahme)
    return result


def choices(kind, size):
    """Welche Teile in einen Steckplatz dieser Art und Größe passen.

    Gibt eine Liste `{'name', 'kennung', 'hersteller', 'guete', 'klasse',
    'herkunft'}` zurück, alphabetisch. `herkunft` ist `BUYABLE`,
    `CRAFTABLE` oder `BOTH` — die Anzeige kann das kennzeichnen.

    ⚠⚠ **Zwei Quellen, weil keine allein reicht:** UEX kennt nur, was im Laden
    steht (kein Militär), die Spieldaten kennen nur, was herstellbar ist. Erst
    zusammen ergibt das die Welt, in der der Spieler sein Schiff ausstattet.
    Zusammengeführt wird über die **Entitäts-Kennung**, nicht über den Namen.

    ⚠⚠ **Geschlossene Liste, kein Freitext** — dieselbe Regel wie beim
    Lagerort und beim Handelslager. Angenommen wird nur, was eine der beiden
    Quellen kennt; sonst steht am Ende ein ausgedachter oder beleidigender
    Name im Werkzeug, und ein Bildschirmfoto davon macht die Runde.

    ⚠ Kennt keine Quelle die Art, kommt eine **leere** Liste zurück — und die
    Anzeige sagt das, statt wahllos den halben Katalog anzubieten. Ein Kühler,
    der in einem Waffenplatz zur Auswahl steht, ist schlimmer als gar keine
    Auswahl.
    """
    from . import shops
    if not kind:
        return []
    found = _craftable(kind, size)
    for entry in found.values():
        entry['herkunft'] = CRAFTABLE

    groups = [g for g, a in GROUP_TO_KIND.items() if a == kind]
    result = []
    for part in (shops.catalog_items() if groups else []):
        if part.get('kategorie') not in groups:
            continue
        # ⚠ Die Größe wird nur geprüft, wenn beide Seiten eine haben. UEX
        # lässt das Feld bei einem Teil der Ware leer — dort dann alles
        # auszusortieren hieße, kaufbare Teile zu verstecken, weil eine
        # fremde Datenbank eine Lücke hat.
        own = str(part.get('groesse') or '').strip()
        if size is not None and own:
            try:
                if int(float(own)) != int(size):
                    continue
            except (TypeError, ValueError):
                pass
        # ⭐ **Güte und Klasse gehören beide dazu.** Man stattet ein Schiff für
        # einen Zweck aus — Kampf, Bergbau, unauffällig fliegen —, und welche
        # Komponente dazu passt, sagt erst „C · Industrial" statt nur der Name.
        # Niemand kennt 1.500 Teile auswendig. Gemessen an den Kraftwerken der
        # Größe 1: 20 Civilian, 13 Industrial, 5 Competition, 3 Stealth, keine
        # Lücke — das Feld trägt also wirklich.
        ident = part.get('kennung') or ''
        # ⚠ Ist das Teil auch herstellbar, wird der vorhandene Eintrag
        # **ergänzt** statt ein zweiter angelegt — sonst stünde dasselbe Teil
        # zweimal in der Liste, einmal aus jeder Quelle.
        already = found.get(ident)
        if already is not None:
            already['herkunft'] = BOTH
            # ⚠⚠ **UEX' Angaben gewinnen wirklich — nicht nur bei einer Lücke.**
            # Bis zum 06.09.2026 stand hier `if wert and not schon.get(feld)`:
            # Die Angabe aus dem Bauplan-Katalog blieb stehen, sobald sie
            # irgendetwas enthielt. Und weil der Katalog die Güte als **Zahl**
            # führt, stand bei `Bolt` plötzlich „2 · Tarnung", wo vorher
            # richtig „B · Tarnung" stand — gemessen an 137 Teilen mit
            # Herkunft „beides".
            #
            # UEX pflegt Klasse und Güte für seine Ladenware gründlicher; wo
            # es etwas führt, gilt das. Der Katalog füllt nur die Lücken.
            for field, value in (('klasse', part.get('klasse')),
                                 ('guete', part.get('guete')),
                                 ('hersteller', part.get('hersteller'))):
                if value:
                    already[field] = value
            continue
        result.append({'name': part.get('name') or '',
                       'kennung': ident,
                       'hersteller': part.get('hersteller') or '',
                       'guete': part.get('guete') or '',
                       'klasse': part.get('klasse') or '',
                       'herkunft': BUYABLE})
    result.extend(found.values())
    result.sort(key=lambda x: (x['name'] or '').lower())
    return result


# ------------------------------------------------------------- Die Auslegung
#
# ⚠ Alles hier arbeitet auf **einem Hangar-Eintrag** (ein Schiff aus
# `fleet.load()['schiffe']`), nicht auf der ganzen Datei. Geschrieben wird
# in das Feld `belegung`, das dort seit v3.19.0-rc1 leer bereitliegt — es
# kostet also keinen Formatwechsel und entwertet keine bestehende Datei.


def loadout(entry):
    """Die gespeicherte Auslegung eines Schiffs: Pfad → Teil."""
    values = (entry or {}).get('belegung')
    return values if isinstance(values, dict) else {}


def set_part(entry, path, ref, name, method=BUY):
    """Ein Teil in einen Steckplatz legen. Gibt zurück, ob sich etwas änderte.

    ⚠ Die **Kennung** ist der Inhalt, der Name nur die Beschriftung daneben.
    Wer später einen Preis dazu sucht, fragt über `ref`.
    """
    if not path or not ref:
        return False
    old = loadout(entry).get(path)
    new = {'ref': ref, 'name': name or '', 'weg': method}
    if old == new:
        return False
    entry.setdefault('belegung', {})[path] = new
    return True


def clear_part(entry, path):
    """Einen Steckplatz wieder auf die Werksausstattung zurücksetzen."""
    return (entry or {}).get('belegung', {}).pop(path, None) is not None


def set_method(entry, path, method):
    """Kaufen oder selbst herstellen — die Wahl an einem Posten.

    ⚠ Die Wahl gehört **in** die Auslegung, nicht in ein zweites Feld daneben.
    Zwei Wörterbücher, die über denselben Schlüssel laufen, laufen früher oder
    später auseinander — dann steht eine Wahl für einen Steckplatz da, in dem
    längst nichts mehr liegt.
    """
    if method not in (BUY, CRAFT):
        return False
    slot = loadout(entry).get(path)
    if not slot or slot.get('weg') == method:
        return False
    slot['weg'] = method
    return True


def open_count(entry):
    """Wie viele Plätze an diesem Schiff noch offen sind — **ohne** Netz.

    ⭐⭐ **Damit man es sieht, ohne aufzuklappen.** Am 06.09.2026 gefragt: *„wie
    sehe ich ohne Aufklappen, dass ich dort noch nicht besorgte Komponenten
    habe?"* Gar nicht — in der Hangar-Liste stand nur „gekauft · LTI · 39
    Steckplätze", und bei vierzig Schiffen klappt niemand alle auf.

    ⚠ **Gezählt wird die gespeicherte Auslegung, nicht `line_items()`.** Das ist
    der ganze Sinn: `line_items()` braucht die Steckplatz-Daten und läuft je
    Schiff einmal durch — bei vierzig Schiffen wäre das Zeichnen der Liste ein
    spürbares Warten, und genau deshalb wird der Warenkorb erst beim Aufklappen
    gebaut. Hier reicht ein Blick ins eigene Feld.

    ⚠ Die Zahl kann in einem Fall zu hoch sein: wenn jemand genau das Werksteil
    ausgewählt hat, das ohnehin drinsteckt. Das ist eine seltene Eingabe, und
    die Marke sagt „hier ist etwas eingetragen" — beim Aufklappen steht dann
    die genaue Liste. Lieber einmal zu viel hinweisen als einen offenen Posten
    verschweigen.
    """
    return sum(1 for part in loadout(entry).values()
               if isinstance(part, dict) and not part.get('erledigt'))


def fully_fitted(entry):
    """Steckt an diesem Schiff alles drin, was geplant war?

    ⭐⭐ **Das ist keine Fleißmeldung, sondern eine Warnung.** Am 06.09.2026
    erklärt: *„ich habe z. B. Super Hornet gefittet und versichert im Spiel,
    und wenn ich das Schiff ohne Versicherung neu claime, würde ich die
    Komponenten verlieren — also muss ich wissen, wo ich schon was fertig
    gefittet habe, nicht nur wo ich noch was kaufen muss."*

    Genau so ist es: Ein neu geclaimtes Schiff kommt in seiner
    **Werksausstattung** zurück. Wer ein aufgerüstetes Schiff ohne die
    passende Versicherung claimt, verliert alles, was er eingebaut hat — und
    das können mehrere hunderttausend aUEC sein. Diese Auskunft ist im Zweifel
    mehr wert als jede Preisliste in diesem Werkzeug.

    ⚠ **Abgeleitet, nicht abgefragt.** Fertig ist, wo etwas geplant **und**
    alles davon abgehakt ist. Ein zusätzlicher Schalter „ist fertig" wäre eine
    zweite Wahrheit neben den Haken — und die beiden liefen früher oder später
    auseinander.

    ⚠ Ein Schiff ohne jede Planung ist **nicht** fertig gefittet, sondern
    unberührt. Beides sähe im Code gleich aus (keine offenen Posten), bedeutet
    aber das Gegenteil: einmal „alles drin", einmal „nie etwas vorgehabt".
    """
    values = loadout(entry)
    if not values:
        return False
    return all(part.get('erledigt') for part in values.values()
               if isinstance(part, dict))


def is_done(entry, path):
    """Ist dieser Posten abgehakt?"""
    return bool((loadout(entry).get(path) or {}).get('erledigt'))


def set_done(entry, path, on=True):
    """Einen Posten abhaken oder den Haken wieder wegnehmen.

    ⭐⭐ **Warum es das braucht — das Werkzeug kann es nicht selbst merken.**
    Am 06.09.2026 gefragt: *„wenn etwas von der Liste gekauft wurde, und im
    Schiff eingebaut ist, wie erfährt die Einkaufsliste davon, dass das Teil
    nun eingebaut ist?"* Die ehrliche Antwort ist: **gar nicht.**

    Das Spiel schreibt nicht in die `Game.log`, was in einem Schiff steckt. Der
    Watcher kennt zwei Dinge: was ab Werk verbaut ist (aus erkul) und was der
    Spieler hier eingetragen hat. Was davon im Hangar Wirklichkeit geworden
    ist, weiß nur er selbst.

    Also wird nichts erraten — es wird abgehakt, wie auf jedem Einkaufszettel.
    Und **genauso beim Selbstherstellen**: Auch dort merkt das Werkzeug nicht,
    dass der Bauauftrag fertig und das Teil eingebaut ist. Ein Haken für beide
    Wege, nicht zwei verschiedene Mechanismen.

    ⚠ Der Haken sitzt **in** der Auslegung, wie schon die Kauf/Bau-Wahl —
    nicht in einer zweiten Liste daneben, die über dieselben Schlüssel läuft
    und irgendwann auseinanderdriftet.
    """
    slot = loadout(entry).get(path)
    if not slot:
        return False
    if bool(slot.get('erledigt')) == bool(on):
        return False
    if on:
        slot['erledigt'] = True
    else:
        slot.pop('erledigt', None)
    return True


# ------------------------------------------------------------- Die Posten


def _blueprint_index():
    """Entitäts-Kennung → Name des Bauplans, der genau dieses Teil herstellt.

    ⚠⚠ **Die Umkehrung von `crafting.entity_of()` — und sie ist der Grund,
    warum hier nichts über Namen läuft.** Der Warenkorb kennt zu jedem Teil nur
    seine Kennung (aus erkul). Um zu wissen, ob es dafür einen Bauplan gibt,
    braucht es den Weg von der Kennung zum Rezept, nicht umgekehrt.

    Der so gefundene Name stammt aus den **Rezeptdaten selbst** und wird nur
    dort wieder nachgeschlagen. Es ist also kein Namensabgleich über zwei
    Quellen hinweg — genau der Fehler, den `shops.py` im Kopf beschreibt.
    """
    from . import crafting
    result = {}
    try:
        for b in crafting.all_items():
            ident = b.get('entity') or ''
            if ident:
                result.setdefault(ident, b.get('basis') or b.get('name') or '')
    except Exception as ausnahme:
        fehler.merken('cart.blueprint_index', ausnahme)
    return result


def line_items(entry):
    """Alles, was an diesem Schiff **nicht** ab Werk verbaut ist.

    Gibt `(zustand, liste)` zurück. Je Posten:

        {'pfad', 'art', 'groesse', 'ref', 'name',
         'werk_ref', 'werk_name',   # was stattdessen ab Werk drinsteckt
         'weg'}                     # BUY oder CRAFT

    ⚠ **Ein ab Werk leerer Platz zählt mit.** Batterie, Bordrechner und
    Gravitationsgenerator stehen bei der Cutlass Black leer — legt der Spieler
    dort etwas hinein, ist das der offensichtlichste Warenkorb-Posten
    überhaupt: Dort fehlt etwas.

    ⚠ Und ein Platz, in den der Spieler **genau das Werksteil** legt, ist
    keiner. Verglichen wird über die Kennung.
    """
    if not entry:
        return NO_DATA, []
    slots = erkul.hardpoints(entry.get('name') or '',
                               entry.get('hersteller') or '',
                               entry.get('kurz') or '',
                               entry.get('hkurz') or '')
    if not slots:
        # ⚠ Das ist **nicht** „nichts zu besorgen". Ohne Steckplatz-Daten ist
        # gar keine Aussage möglich, und die beiden Fälle dürfen nie denselben
        # Satz erzeugen.
        return NO_DATA, []

    chosen = loadout(entry)
    by_path = dict((p.get('pfad'), p) for p in slots)
    result = []
    for path, part in chosen.items():
        slot = by_path.get(path)
        if not slot or not (part or {}).get('ref'):
            # Ein Steckplatz, den es nicht mehr gibt — nach einem Patch
            # möglich. Er wird übergangen, nicht gemeldet: Der Spieler kann
            # nichts dafür, und ein Fehler wäre er auch nicht.
            continue
        factory = slot.get('werk') or {}
        if factory.get('ref') == part['ref']:
            continue
        result.append({
            'pfad': path,
            'art': slot.get('art') or '',
            'groesse': slot.get('groesse'),
            'ref': part['ref'],
            'name': part.get('name') or '',
            'werk_ref': factory.get('ref') or '',
            'werk_name': factory.get('name') or '',
            'weg': part.get('weg') or BUY,
            # ⚠ Ein abgehakter Posten bleibt in der Liste — er wird nur nicht
            # mehr mitgerechnet. Ihn verschwinden zu lassen hiesse, dass
            # niemand einen falsch gesetzten Haken zuruecknehmen kann.
            'erledigt': bool(part.get('erledigt')),
        })
    result.sort(key=lambda p: (p['art'], p['pfad']))
    return (OPEN if result else NOTHING_OPEN), result


# ------------------------------------------------------------- Die zwei Wege


def buy_option(ref, name=''):
    """Was der Posten fertig im Laden kostet.

    Gibt `{'zustand', 'preis', 'laden', 'ort'}` zurück.

    ⚠ **Drei Zustände, und keiner davon ist eine Behauptung über das Spiel.**
    `NOT_CHECKED` heißt, dass noch niemand nachgesehen hat; `NO_PRICE`,
    dass UEX das Teil nicht führt. UEX hat Lücken (gemessen: 435 von 1.604
    Bauplänen) — daraus „nirgends im Handel" zu machen, wäre eine Aussage über
    fremde Daten, nicht über das Spiel.
    """
    from . import shops
    blank = {'zustand': NOT_CHECKED, 'preis': None, 'laden': '', 'ort': ''}
    if not ref:
        return blank
    if not shops.known(ref):
        return blank
    best = shops.cheapest(ref)
    if not best:
        return {'zustand': NO_PRICE, 'preis': None, 'laden': '', 'ort': ''}
    price, shop_name, place = best
    return {'zustand': KNOWN, 'preis': price, 'laden': shop_name,
            'ort': place}


def craft_option(ref, index=None, name=''):
    """Was der Posten an Material kostet, wenn er selbst hergestellt wird.

    Gibt `{'zustand', 'material', 'dauer', 'bauplan', 'ohne_preis'}` zurück.

    ⚠⚠ **`ohne_preis` ist keine Nebensache.** Ein Rohstoff mit Kaufpreis 0 ist
    nicht kostenlos, sondern **nicht kaufbar** — er muss abgebaut werden. Die
    Materialsumme ist dann eine **Untergrenze**, und wer das nicht dazusagt,
    lässt Selberbauen billiger aussehen, als es ist. Dieselbe Falle wie bei den
    Ankaufgeboten in `selling.py`.

    ⚠⚠⚠ **`ref` ist die Entitäts-Kennung, NICHT der Name.** Merkzettel-Posten
    kommen aus der Herstellungsliste, und die kennt nur den Bauplannamen — beim
    ersten Anlauf am 06.09.2026 landete der Name im `ref`-Feld, `index`
    fand nichts, und der Posten fiel stillschweigend auf „kaufen" zurück. Auf
    „Was ich farmen muss" stand daraufhin „Nichts auf selbst herstellen
    gestellt", obwohl zwei Waffen vorgemerkt waren.

    Deshalb der zweite Weg über `name`: Findet die Kennung nichts, wird der
    **Bauplanname** direkt genommen — `crafting.recipe()` sucht ohnehin über
    ihn.
    """
    from . import crafting, prices
    blank = {'zustand': NO_RECIPE, 'material': None, 'dauer': None,
             'bauplan': '', 'ohne_preis': []}
    if not ref and not name:
        return blank
    index = _blueprint_index() if index is None else index
    blueprint = (index.get(ref) or '') if ref else ''
    if not blueprint and name:
        # ⚠ Der Rückweg für Posten ohne Kennung. `recipe()` nimmt den Namen,
        # also reicht er — geprüft wird gleich unten, ob wirklich einer kommt.
        blueprint = name
    if not blueprint:
        return blank
    try:
        rec = crafting.recipe(blueprint)
    except Exception as ausnahme:
        fehler.merken('cart.craft_option.recipe', ausnahme)
        return blank
    if not rec or not rec.get('stufen'):
        return blank

    # Aktuell hat jeder Bauplan genau eine Stufe — gerechnet wird trotzdem
    # über alle, damit eine zweite nicht stillschweigend unterschlagen wird.
    material = 0.0
    unpriced = []
    time_total = 0
    for step in rec['stufen']:
        time_total += int(step.get('zeit') or 0)
        for _slot, raw, amount, _grade in (step.get('zutaten') or []):
            found = prices.price(raw)
            buy = (found or (0, 0, ''))[0]
            if not buy:
                # Nicht kaufbar (oder gar keine Preisdaten) — der Posten wird
                # benannt, nicht mit 0 verrechnet.
                if raw not in unpriced:
                    unpriced.append(raw)
                continue
            material += float(buy) * float(amount or 0)
    return {'zustand': KNOWN, 'material': material, 'dauer': time_total,
            'bauplan': blueprint, 'ohne_preis': unpriced}


def enrich(items):
    """Jeden Posten um beide Wege ergänzen — `kauf` und `bau`.

    ⚠ Das Bauplan-Verzeichnis wird **einmal** gebaut, nicht je Posten: Es geht
    über rund 1.600 Baupläne, und bei zwölf Posten wären das zwölf Durchläufe
    für dieselbe Tabelle.
    """
    index = _blueprint_index()
    for p in items:
        p['kauf'] = buy_option(p.get('ref'), p.get('name'))
        p['bau'] = craft_option(p.get('ref'), index)
        # ⚠ Ein Posten ohne Bauplan kann nicht gebaut werden — dann steht der
        # Weg auf „kaufen", ganz gleich, was gespeichert war. Sonst rechnet die
        # Summe mit einem Weg, den es nicht gibt.
        if p['weg'] == CRAFT and p['bau']['zustand'] != KNOWN:
            p['weg'] = BUY
    return items


def total(items):
    """Was der Warenkorb **günstigstenfalls** kostet — nach der getroffenen Wahl.

    Gibt `{'gesamt', 'kaufen', 'bauen', 'dauer', 'offen', 'unvollstaendig'}`
    zurück.

    ⚠⚠ **Das ist der Bestpreis, nicht der Reisepreis — und beide Zahlen
    gehören beschriftet.** Hier zählt je Posten der billigste Laden im ganzen
    Verse; `route()` rechnet dagegen mit den Läden, die auf einer kurzen Route
    wirklich liegen. Die beiden Zahlen weichen ab, sobald der billigste Laden
    woanders steht — im Probelauf 48.300 gegen 49.960 aUEC, weil der billigere
    BlastChill an einer Station lag, die sonst nichts führt.
    Das ist **kein Fehler**, sondern der Preis dafür, nicht durch drei Systeme
    zu fliegen. Wer beide Zahlen unbeschriftet nebeneinanderstellt, erzeugt
    aber genau den Verdacht, das Werkzeug rechne falsch. Also: hier
    „günstigstenfalls", bei der Route „auf dieser Route".

    ⚠ `offen` zählt die Posten, zu denen **keine Zahl** vorliegt. Sie fehlen in
    der Summe, und das muss dabeistehen: Eine Summe, der drei Posten fehlen,
    sieht genauso aus wie eine vollständige.
    """
    overall = buy_part = craft_part = 0.0
    time_total = 0
    unpriced = 0
    incomplete = False
    for p in items:
        # ⚠⚠ **Abgehaktes kostet nichts mehr.** Wer ein Teil gekauft und
        # eingebaut hat, will nicht, dass es weiter in der Summe steht — sonst
        # bleibt die Zahl gleich, egal wie viel man schon erledigt hat, und
        # die Liste verliert ihren Zweck. Es zaehlt aber auch nicht als
        # „fehlender Preis": Es fehlt nichts, es ist fertig.
        if p.get('erledigt'):
            continue
        # ⚠ **Die Anzahl zählt mit.** Merkzettel-Posten dürfen mehrfach geplant
        # sein („drei Helme") — dann kosten sie auch dreimal so viel und
        # brauchen dreimal so lange. Ein Schiffsteil hat kein `anzahl`; dort
        # bleibt es bei 1, weil ein Steckplatz genau ein Teil aufnimmt.
        try:
            pieces = max(1, int(p.get('anzahl') or 1))
        except (TypeError, ValueError):
            pieces = 1
        if p.get('weg') == CRAFT:
            craft = p.get('bau') or {}
            if craft.get('zustand') != KNOWN or craft.get('material') is None:
                unpriced += 1
                continue
            craft_part += craft['material'] * pieces
            overall += craft['material'] * pieces
            time_total += int(craft.get('dauer') or 0) * pieces
            if craft.get('ohne_preis'):
                incomplete = True
        else:
            buy = p.get('kauf') or {}
            if buy.get('zustand') != KNOWN or buy.get('preis') is None:
                unpriced += 1
                continue
            buy_part += buy['preis'] * pieces
            overall += buy['preis'] * pieces
    return {'gesamt': overall, 'kaufen': buy_part, 'bauen': craft_part,
            'dauer': time_total, 'offen': unpriced,
            'unvollstaendig': incomplete}


# ------------------------------------------------------------- Die Kaufroute


def _offers(items):
    """Je zu kaufendem Posten alle Läden, die ihn führen."""
    from . import shops
    result = {}
    for p in items:
        if p.get('weg') != BUY or not p.get('ref'):
            continue
        rows = shops.shops_for(p['ref'])
        if not rows:
            continue
        result[p['pfad']] = rows
    return result


def route(items):
    """Die Einkaufsroute: möglichst wenige Stopps für alles Gekaufte.

    Gibt `(stopps, ohne)` zurück — `stopps` ist eine Liste::

        {'ort': 'Area18', 'system': 'Stanton',
         'laeden': ['Dumper's Depot'],
         'posten': [{'pfad', 'name', 'preis', 'laden'}],
         'summe': 22730.0}

    `ohne` sind die Pfade der Posten, zu denen kein Laden bekannt ist.

    ⚠⚠ **Gewählt wird nach Deckung, nicht nach Preis.** Wer stur den billigsten
    Laden je Posten nimmt, bekommt acht Posten in acht Systemen — rechnerisch
    das Beste, in der Praxis ein verlorener Abend. Erst bei gleicher Deckung
    entscheidet der Preis.

    ⚠ Ein **Stopp** ist ein Ort, ein **Laden** ein Terminal darin. Zwei Läden
    an derselben Station sind ein Stopp — genau die Unterscheidung, die erkul
    mit „1 shop · 1 stop" trifft.
    """
    offers = _offers(items)
    # ⚠ Nur Posten mit Steckplatz — ein ganzes Schiff hat keinen, und
    # Abgehaktes muss man nicht mehr abholen (siehe `_offers`).
    buyable = [p for p in items if p.get('pfad') and not p.get('erledigt')]
    by_path = dict((p['pfad'], p) for p in buyable)
    open_paths = set(offers)
    without = [p['pfad'] for p in buyable
               if p.get('weg') == BUY and p['pfad'] not in offers]

    # Ort → {Pfad → billigste Zeile dort}
    places = {}
    for path, rows in offers.items():
        for row in rows:
            key = (row.get('system') or '', row.get('ort') or '')
            here = places.setdefault(key, {})
            # ⚠ Am selben Ort kann dasselbe Teil in mehreren Terminals liegen —
            # es zählt einmal, und zwar mit dem billigsten Preis.
            if path not in here or row['preis'] < here[path]['preis']:
                here[path] = row

    stops = []
    while open_paths:
        best = None
        for key, here in places.items():
            covers = open_paths & set(here)
            if not covers:
                continue
            cost = sum(here[p]['preis'] for p in covers)
            # Viel Deckung zuerst, dann billig, dann nach Namen — der letzte
            # Schlüssel nur, damit dasselbe Ergebnis stabil bleibt.
            mark = (-len(covers), cost, key)
            if best is None or mark < best[0]:
                best = (mark, key, covers)
        if best is None:
            break
        _mark, key, covers = best
        here = places[key]
        entries = []
        for path in sorted(covers):
            row = here[path]
            entries.append({
                'pfad': path,
                'name': (by_path.get(path) or {}).get('name') or '',
                'preis': row['preis'],
                'laden': row.get('laden') or '',
            })
        stops.append({
            'system': key[0],
            'ort': key[1],
            'laeden': sorted(set(e['laden'] for e in entries if e['laden'])),
            'posten': entries,
            'summe': sum(e['preis'] for e in entries),
        })
        open_paths -= covers

    return stops, without


def invoice(data=None):
    """Die Einkaufsliste über **alle** Schiffe — wie eine Rechnung.

    Gibt zurück::

        {'posten': [...],          # jeder mit Position, Preis und Weg
         'summe': {...},           # dieselbe Form wie `total()`
         'schiffe': 3,             # wie viele Schiffe beteiligt sind
         'ohne_steckplatzdaten': ['Galaxy', …]}

    Je Posten:

        {'sorte': PART | SHIP,
         'schiff': 'Cutlass Black', 'quelle': HANGAR | WISHLIST,
         'position': 'Cooler S2',   # wo am Schiff — bei SHIP leer
         'name': 'BlastChill', 'ref': …, 'weg': BUY | CRAFT,
         'kauf': {...}, 'bau': {...}}

    ⭐⭐ **Ein Schiff ist selbst ein Posten — aber nur, wenn man es noch nicht
    hat.** Was im Hangar steht, ist bezahlt; dort zählen nur die Teile, die noch
    fehlen. Ein Wunschschiff dagegen kostet erst einmal sich selbst, und dann
    noch seine Ausstattung. Beides in einer Rechnung ist genau die Frage, die
    vor dem Kauf im Kopf steht: *was kostet mich das am Ende?*

    ⚠ **Jeder Posten trägt seine Position.** Eine Rechnung ohne Positionen ist
    eine Zahl, mit der niemand etwas anfangen kann — bei zwölf Kühlern in vier
    Schiffen weiß man sonst nicht, welcher wohin gehört.

    ⚠ **Ohne Netzzugriff.** Diese Funktion rechnet nur mit dem, was schon
    abgelegt ist. Was noch nachzuschlagen wäre, sagt `missing_prices()` — das
    Holen gehört in die Oberfläche, wo es im Hintergrund laufen kann, und nicht
    in eine Funktion, die beim Aufklappen einer Seite anhält.
    """
    from . import fleet, ships as alle_schiffe

    data = data if data is not None else fleet.load()
    index = _blueprint_index()
    result = []
    without_data = []
    involved = set()

    sources = ([(s, HANGAR) for s in (data.get('schiffe') or [])]
               + [(w, WISHLIST) for w in (data.get('wunsch') or [])])

    for entry, source in sources:
        name = entry.get('name') or ''
        if not name:
            continue

        # 1. Das Schiff selbst — nur beim Wunsch, siehe oben.
        if source == WISHLIST:
            involved.add(name)
            ship_item = {
                'sorte': SHIP, 'schiff': name, 'quelle': source,
                'position': '', 'name': name, 'ref': '',
                'weg': BUY,
                'bau': {'zustand': NO_RECIPE, 'material': None,
                        'dauer': None, 'bauplan': '', 'ohne_preis': []},
            }
            try:
                places = alle_schiffe.buy_at(name)
            except Exception as ausnahme:
                fehler.merken('cart.invoice.ship_price', ausnahme)
                places = []
            if places:
                # ⚠ `buy_at()` gibt eine **Liste** von Verkaufsstellen zurück,
                # billigste zuerst — kein Tupel wie `shops.cheapest()`.
                best = places[0]
                ship_item['kauf'] = {
                    'zustand': KNOWN, 'preis': best.get('preis'),
                    'laden': best.get('stelle') or '',
                    'ort': best.get('ort') or ''}
            else:
                # ⚠ Kein Preis heißt hier meistens „im Spiel nicht für aUEC zu
                # haben" (Konzeptschiff, nur gegen Echtgeld). Behauptet wird das
                # trotzdem nicht — es steht nur kein Preis da.
                ship_item['kauf'] = {'zustand': NO_PRICE, 'preis': None,
                                     'laden': '', 'ort': ''}
            result.append(ship_item)

        # 2. Die Ausstattung — bei Hangar- und Wunschschiffen gleich.
        state, items = line_items(entry)
        if state == NO_DATA:
            # ⚠ Nur vermerken, wenn jemand am Schiff überhaupt etwas vorhat.
            # Sonst stünden vierzig Konzeptschiffe als Mangel in der Rechnung.
            if loadout(entry):
                without_data.append(name)
            continue
        for p in items:
            involved.add(name)
            p['kauf'] = buy_option(p.get('ref'), p.get('name'))
            p['bau'] = craft_option(p.get('ref'), index)
            if p['weg'] == CRAFT and p['bau']['zustand'] != KNOWN:
                p['weg'] = BUY
            position = p.get('art') or ''
            if p.get('groesse') is not None:
                position = '%s S%s' % (position, p['groesse'])
            p.update({'sorte': PART, 'schiff': name, 'quelle': source,
                      'position': position})
            result.append(p)

    # 3. Der Merkzettel — Einzelteile, die zu keinem Schiff gehören.
    #
    # ⭐⭐ **Helm, Waffe, Rüstung: Baupläne wie jeder andere.** Sie haben nur
    # keinen Steckplatz, an dem sie hängen könnten. Deshalb tragen sie kein
    # Schiff und keine Position — alles andere (Preis, Rezept, Material,
    # Route) rechnet sich genauso.
    #
    # ⚠ Die `anzahl` gehört an den Posten, nicht in mehrere Zeilen: Drei
    # gleiche Helme sollen einmal dastehen und dreifaches Material fordern,
    # nicht dreimal untereinander stehen.
    for entry in fleet.notepad(data):
        name = entry.get('name') or ''
        if not name:
            continue
        ref = entry.get('ref') or ''
        try:
            amount = max(1, int(entry.get('anzahl') or 1))
        except (TypeError, ValueError):
            amount = 1
        p = {'sorte': PART, 'schiff': '', 'quelle': NOTEPAD,
             'position': '', 'name': name, 'ref': ref,
             'anzahl': amount,
             'weg': entry.get('weg') or CRAFT,
             'erledigt': bool(entry.get('erledigt'))}
        p['kauf'] = buy_option(ref, name)
        # ⚠ **Mit `name`**, denn ein Merkzettel-Posten hat oft keine
        # Entitäts-Kennung: Er entsteht in der Herstellungsliste, und die kennt
        # nur den Bauplannamen. Ohne diesen zweiten Weg fällt jeder vorgemerkte
        # Gegenstand auf „kaufen" zurück, und die Materialliste bleibt leer.
        p['bau'] = craft_option(ref, index, name=name)
        # ⚠ Dieselbe Regel wie bei den Schiffsteilen: Ohne Rezept ist „bauen"
        # keine Wahl, sondern eine leere Behauptung.
        if p['weg'] == CRAFT and p['bau']['zustand'] != KNOWN:
            p['weg'] = BUY
        result.append(p)

    # Schiffe zuerst, dann ihre Teile — wie auf einer Rechnung, auf der die
    # Hauptposition über dem Zubehör steht.
    #
    # ⚠ Merkzettel-Posten haben kein Schiff und landen dadurch von selbst
    # ganz oben. Das ist gewollt: Sie sind eine eigene kleine Liste und sollen
    # nicht zwischen den Schiffsteilen verschwinden.
    result.sort(key=lambda p: ((p['schiff'] or '').lower(),
                               0 if p['sorte'] == SHIP else 1,
                               (p.get('position') or ''),
                               (p.get('name') or '').lower()))
    return {'posten': result, 'summe': total(result), 'schiffe': len(involved),
            'ohne_steckplatzdaten': sorted(set(without_data))}


def missing_prices(item_list):
    """Zu welchen Posten der Ladenpreis noch nachzuschlagen ist.

    Gibt Paare `(kennung, name)` zurück — genau das, was `shops.fetch()`
    braucht.

    ⚠⚠ **Ohne diesen Schritt bleibt eine Rechnung auf `NOT_CHECKED`
    stehen** und zeigt Posten ohne Preis, obwohl UEX sie kennt. Der Abruf
    gehört aber nicht hierher: Er dauert je Teil eine Netzrunde, und eine
    Rechnung mit zwölf Posten würde die Oberfläche zwölf Mal anhalten. Die
    Anzeige holt sie im Hintergrund nach und zeichnet dann neu.
    """
    result = []
    seen = set()
    for p in item_list or []:
        if p.get('sorte') == SHIP:
            # Schiffspreise kommen aus `ships.py`, nicht aus `shops.py`.
            continue
        ident = p.get('ref') or ''
        state = (p.get('kauf') or {}).get('zustand')
        if ident and ident not in seen and state == NOT_CHECKED:
            seen.add(ident)
            result.append((ident, p.get('name') or ''))
    return result


def farm_list(data=None):
    """Was noch zu farmen ist — Material für **alle** Posten auf „bauen".

    Gibt zurück::

        {'fehlt':         [{'rohstoff', 'benoetigt', 'vorhanden',
                            'differenz', 'mindestguete', 'zu_gering'}, …],
         'vollstaendig':  [dieselbe Form],
         'posten':        4,        # wie viele Posten gebaut werden
         'ohne_rezept':   ['…']}    # Sicherheitsnetz, siehe unten

    ⭐ Die Gegenrichtung zur Einkaufsliste: Dort steht, was Geld kostet, hier,
    was Zeit kostet. Zusammen beantworten sie *„was muss ich noch tun, bis mein
    Schiff so aussieht, wie ich es will?"*

    ⚠⚠ **Zusammengezählt wird ÜBER alle Posten, nicht Posten für Posten.**
    Das ist die Falle, die `materials.check()` allein nicht abfängt: Es
    rechnet **ein** Rezept gegen das Lager. Bei zwei Posten mit je 2 Iron und
    3 Iron im Lager meldet es zweimal „reicht" — zusammen fehlt aber eines.
    Wer die Fehlmengen einzeln addiert, bekommt dasselbe Erz mehrfach
    angerechnet und sagt dem Spieler, er könne losbauen.

    ⚠ **Die Mindestgüte gehört mit in die Rechnung.** Erz mit Q 200 in einem
    Rezept, das Q 500 verlangt, ist für diesen Bauplan nichts wert. Fordern
    mehrere Posten dasselbe Material in verschiedenen Güten, wird der Bestand
    von der **anspruchsvollsten Anforderung abwärts** zugeteilt — sonst
    verbraucht ein anspruchsloser Posten das gute Erz, und der anspruchsvolle
    steht ohne da.

    ⚠ Ohne Netz, ohne Schätzen: Ein Posten, dessen Rezept sich nicht lesen
    lässt, steht unter `ohne_rezept` und wird **nicht** stillschweigend mit
    null Materialbedarf verrechnet.

    ⚠ `ohne_rezept` bleibt im Regelfall **leer**, und das ist richtig so:
    `invoice()` setzt den Weg schon auf „kaufen" zurück, sobald zu einem
    Posten kein Rezept vorliegt — hier kommt er dann gar nicht mehr an. Das
    Feld ist ein **Sicherheitsnetz** für den Fall, dass sich das einmal ändert
    oder ein Rezept zwischen den beiden Schritten wegfällt. Lieber ein Feld,
    das meistens leer ist, als ein stiller Verlust.
    """
    from . import crafting, materials

    done = invoice(data)
    index = _blueprint_index()

    # 1. Bedarf einsammeln: (Rohstoff, Mindestgüte) -> Menge
    needed = {}
    without_recipe = []
    crafted = 0
    for p in done['posten']:
        if p.get('weg') != CRAFT:
            continue
        # ⚠⚠ **Was gebaut UND eingebaut ist, braucht kein Material mehr.**
        # Bis zum 06.09.2026 zählte die Farmliste auch abgehakte Posten mit:
        # Vier von acht Schilden waren fertig, und trotzdem stand „für 8
        # geplante Bauteile · fehlt 8,8 Stileron" da. Dazu die Rückmeldung:
        # „der Wert ändert sich auch nicht, wenn ich Sachen als eingebaut
        # markiert habe — das erwartet aber jeder User, denn wenn ich es
        # hergestellt habe, dann brauch ich das Material ja nicht mehr."
        #
        # Genau so. Eine Farmliste, die nach getaner Arbeit dieselbe Zahl
        # zeigt, schickt den Spieler ein zweites Mal in denselben Asteroiden.
        if p.get('erledigt'):
            continue
        # ⚠ **Die Anzahl zählt mit.** Merkzettel-Posten dürfen mehrfach
        # geplant sein („drei Helme") — dann ist auch dreifaches Material
        # nötig. Schiffsteile haben kein `anzahl`; für sie bleibt es bei 1,
        # weil ein Steckplatz genau ein Teil aufnimmt.
        try:
            pieces = max(1, int(p.get('anzahl') or 1))
        except (TypeError, ValueError):
            pieces = 1
        crafted += pieces
        # ⚠⚠ **Auch hier der Rückweg über den Namen** — dieselbe Falle wie in
        # `craft_option()`. Ein Merkzettel-Posten hat keine Entitäts-Kennung: Er
        # entsteht in der Herstellungsliste, und die kennt nur den
        # Bauplannamen. Ohne diese Zeile meldete die Seite „2 Teile konnten
        # nicht gerechnet werden" und darunter „Alles da" — bei null Erz im
        # Lager. Zwei Sätze, die sich widersprechen, und beide falsch.
        blueprint = index.get(p.get('ref') or '') or ''
        if not blueprint:
            blueprint = p.get('name') or ''
        rec = None
        if blueprint:
            try:
                rec = crafting.recipe(blueprint)
            except Exception as ausnahme:
                fehler.merken('cart.farm_list.recipe', ausnahme)
        if not rec or not rec.get('stufen'):
            without_recipe.append(p.get('name') or '')
            continue
        for step in rec['stufen']:
            for _slot, raw, amount, grade in (step.get('zutaten') or []):
                key = (crafting.norm_material(raw), float(grade or 0))
                entry = needed.setdefault(key, {'name': raw, 'menge': 0.0})
                entry['menge'] += float(amount or 0) * pieces

    # 2. Je Rohstoff den Bestand zuteilen — anspruchsvollste Güte zuerst.
    by_material = {}
    for (norm, grade), entry in needed.items():
        by_material.setdefault(norm, []).append(
            (grade, entry['name'], entry['menge']))

    missing, complete = [], []
    for norm, groups in by_material.items():
        # ⚠ Absteigend: Wer die höchste Güte verlangt, bekommt zuerst — und
        # nimmt dabei das **gerade noch ausreichende** Erz, damit das bessere
        # für nichts verschwendet wird, das es nicht braucht.
        groups.sort(reverse=True)
        stock = [dict(p) for p in materials.load()
                 if crafting.norm_material(p.get('material')) == norm]
        for grade, name, amount in groups:
            usable = sorted(
                (p for p in stock
                 if float(p.get('qualitaet') or 0) >= grade
                 and float(p.get('menge') or 0) > 0),
                key=lambda p: float(p.get('qualitaet') or 0))
            taken = 0.0
            for p in usable:
                if taken >= amount:
                    break
                have = float(p.get('menge') or 0)
                take = min(have, amount - taken)
                p['menge'] = have - take
                taken += take
            # Was zwar da ist, aber die Güte nicht schafft — als Hinweis, nicht
            # als Bestand. Das Lager wird von Hand gepflegt und kann hinterher
            # hinken; behauptet wird deshalb nichts.
            too_low = sum(float(p.get('menge') or 0) for p in stock
                          if float(p.get('qualitaet') or 0) < grade)
            row = {'rohstoff': name, 'benoetigt': amount,
                   'vorhanden': taken,
                   'differenz': max(0.0, amount - taken),
                   'mindestguete': grade, 'zu_gering': too_low}
            (missing if row['differenz'] > 0 else complete).append(row)

    missing.sort(key=lambda z: (-z['differenz'], z['rohstoff'].lower()))
    complete.sort(key=lambda z: z['rohstoff'].lower())
    return {'fehlt': missing, 'vollstaendig': complete, 'posten': crafted,
            'ohne_rezept': sorted(set(x for x in without_recipe if x))}


def route_total(stops):
    """Was diese Route kostet, und wie weit sie führt.

    Gibt `{'gesamt', 'stopps', 'laeden', 'systeme'}` zurück.

    ⚠ **Diese Zahl gehört an die Route, nicht die aus `total()`.** Sie ist in
    aller Regel etwas höher, weil an einem Ort nicht alles zum Bestpreis liegt
    — siehe die Warnung dort. Angezeigt wird sie deshalb als „auf dieser
    Route", damit niemand die beiden für dieselbe Angabe hält.
    """
    return {
        'gesamt': sum(s['summe'] for s in stops),
        'stopps': len(stops),
        'laeden': sum(len(s['laeden']) for s in stops),
        'systeme': len(set(s['system'] for s in stops if s['system'])),
    }
