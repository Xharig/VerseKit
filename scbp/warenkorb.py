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
| `KEINE_DATEN` | zum Schiff fehlen die Steckplätze | „für dieses Schiff liegen keine Daten vor" |
| `NICHTS_OFFEN` | Auslegung = Werksausstattung | „nichts zu besorgen — alles ab Werk" |
| `KEIN_PREIS` | Teil bekannt, UEX hat keinen Preis | Feld bleibt leer, keine Behauptung |
| `KEIN_REZEPT` | dafür gibt es keinen Bauplan | „nur kaufbar" |

`KEIN_PREIS` und `KEIN_REZEPT` hängen am einzelnen Posten, die anderen beiden
am Schiff.

## ⚠ Zugeordnet wird über die Kennung, nie über den Namen

Erkuls `ref`, UEX' `uuid` und die `productEntityClass` der Rezeptdaten sind
dieselbe Entitäts-Kennung. Über Namen ist es in diesem Projekt zweimal
schiefgegangen (`Gold` holte `Golden Medmon` mit). Auch der Bauplan zu einem
Teil wird deshalb über die Kennung gesucht, nicht über die Beschriftung.

## Die Kaufroute

`routen.py` beantwortet eine **andere** Frage — „wo kaufe ich billig und
verkaufe teuer", über UEX' fertige Handelsfahrten. Hier geht es darum, eine
feste Liste mit möglichst wenigen Stopps abzuklappern. Es gibt zwischen beiden
auch keine gemeinsame Schlüsselgröße: `routen.py` rechnet ausschließlich über
Terminal-Nummern, `laeden.py` legt nur Namen ab.

Gerechnet wird deshalb hier, mit einer Überdeckung: Es gewinnt der Ort, der die
meisten offenen Posten deckt, bei Gleichstand der billigere. Das ist dieselbe
Auskunft, die erkul als „1 shop · 1 stop" zeigt.
"""

import re

from . import erkul, fehler

# Zustände eines ganzen Warenkorbs.
KEINE_DATEN = 'keine_daten'
NICHTS_OFFEN = 'nichts_offen'
OFFEN = 'offen'

# Zustände eines einzelnen Weges an einem Posten.
BEKANNT = 'bekannt'
KEIN_PREIS = 'kein_preis'
KEIN_REZEPT = 'kein_rezept'
NICHT_GEPRUEFT = 'nicht_geprueft'

# Die beiden Wege, zwischen denen der Spieler wählt.
KAUFEN = 'kaufen'
BAUEN = 'bauen'

# Was für ein Posten auf der Rechnung steht — ein Einzelteil oder ein ganzes
# Schiff. ⚠ Beide brauchen dieselbe Form (`kauf`, `bau`, `weg`), damit `summe()`
# und `route()` sie ohne Sonderfall verarbeiten.
TEIL = 'teil'
SCHIFF = 'schiff'

# Woher ein Schiff kommt: schon im Hangar oder erst auf der Wunschliste. Der
# Unterschied entscheidet, ob das Schiff selbst als Posten zählt — was man hat,
# muss man nicht kaufen.
HANGAR = 'hangar'
WUNSCH = 'wunsch'

# Woher ein Teil überhaupt zu bekommen ist. ⚠ Das ist keine Feinheit: Militär
# ist **nicht kaufbar, aber herstellbar** — wer nur die Ladenware zeigt, lässt
# genau die Teile weg, für die man Baupläne sammelt.
KAUFBAR = 'kaufbar'
HERSTELLBAR = 'herstellbar'
BEIDES = 'beides'


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
GRUPPE_ZU_ART = {
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
REZEPT_ZU_ART = {
    'quantumdrive': 'QuantumDrive',
    'cooler': 'Cooler',
    'powerplant': 'PowerPlant',
    'shield': 'Shield',
    'radar': 'Radar',
    'mininglaser': 'WeaponMining',
    'tractorbeam': 'TractorBeam',
}

# `size2` → 2. Nur diese Form, nichts geraten.
_GROESSE_AUS_UNTERART = re.compile(r'^size(\d+)$')


def _aus_rezept(eintrag):
    """Art und Größe aus den Rezeptdaten — oder `(None, None)`."""
    art = REZEPT_ZU_ART.get((eintrag.get('art') or '').lower())
    if not art:
        return None, None
    treffer = _GROESSE_AUS_UNTERART.match((eintrag.get('unterart') or '').lower())
    return art, (int(treffer.group(1)) if treffer else None)


def _guete_buchstabe(wert):
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
    text = str(wert or '').strip()
    return {'1': 'A', '2': 'B', '3': 'C', '4': 'D'}.get(text, text)


def _herstellbare(art, groesse):
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
    Namensform; `einordnung()` verknüpft sie längst genauso. Gemessen am
    06.09.2026: **1.592 von 1.597 (99,7 %)** finden ihre Angaben, alle davon
    mit Art und Größe.

    ⚠ Die **Kennung** bleibt trotzdem der Schlüssel des Ergebnisses. Über sie
    hängen später Ladenpreis und Rezept am Teil — der Name ist nur die
    Beschriftung.
    """
    from . import herstellung, katalog
    raus = {}
    try:
        werte = (katalog.laden() or {}).get('bauplaene') or {}
        for eintrag in herstellung.alle():
            kennung = eintrag.get('entity') or ''
            if not kennung:
                continue
            merkmale = werte.get(katalog._norm(eintrag.get('basis') or '')) or {}
            eigene_art = merkmale.get('a') or ''
            eigene = merkmale.get('s')
            if not eigene_art:
                # Der Katalog kennt dieses Teil nicht — dann sagen es die
                # Rezeptdaten selbst. Siehe `REZEPT_ZU_ART`.
                eigene_art, aus_rezept = _aus_rezept(eintrag)
                if eigene is None:
                    eigene = aus_rezept
            if (eigene_art or '') != art:
                continue
            # ⚠ Größe nur vergleichen, wenn beide Seiten eine haben — sonst
            # fällt ein Teil heraus, weil eine Angabe fehlt, nicht weil es
            # nicht passt.
            if groesse is not None and eigene is not None:
                try:
                    if int(eigene) != int(groesse):
                        continue
                except (TypeError, ValueError):
                    pass
            raus[kennung] = {
                'name': eintrag.get('basis') or eintrag.get('name') or '',
                'kennung': kennung,
                'hersteller': (merkmale.get('m')
                               or eintrag.get('hersteller') or ''),
                'guete': _guete_buchstabe(merkmale.get('g')),
                'klasse': merkmale.get('c') or '',
            }
    except Exception as ausnahme:
        fehler.merken('warenkorb.herstellbare', ausnahme)
    return raus


def auswahl(art, groesse):
    """Welche Teile in einen Steckplatz dieser Art und Größe passen.

    Gibt eine Liste `{'name', 'kennung', 'hersteller', 'guete', 'klasse',
    'herkunft'}` zurück, alphabetisch. `herkunft` ist `KAUFBAR`,
    `HERSTELLBAR` oder `BEIDES` — die Anzeige kann das kennzeichnen.

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
    from . import laeden
    if not art:
        return []
    gefunden = _herstellbare(art, groesse)
    for eintrag in gefunden.values():
        eintrag['herkunft'] = HERSTELLBAR

    gruppen = [g for g, a in GRUPPE_ZU_ART.items() if a == art]
    raus = []
    for teil in (laeden.katalog_teile() if gruppen else []):
        if teil.get('kategorie') not in gruppen:
            continue
        # ⚠ Die Größe wird nur geprüft, wenn beide Seiten eine haben. UEX
        # lässt das Feld bei einem Teil der Ware leer — dort dann alles
        # auszusortieren hieße, kaufbare Teile zu verstecken, weil eine
        # fremde Datenbank eine Lücke hat.
        eigene = str(teil.get('groesse') or '').strip()
        if groesse is not None and eigene:
            try:
                if int(float(eigene)) != int(groesse):
                    continue
            except (TypeError, ValueError):
                pass
        # ⭐ **Güte und Klasse gehören beide dazu.** Man stattet ein Schiff für
        # einen Zweck aus — Kampf, Bergbau, unauffällig fliegen —, und welche
        # Komponente dazu passt, sagt erst „C · Industrial" statt nur der Name.
        # Niemand kennt 1.500 Teile auswendig. Gemessen an den Kraftwerken der
        # Größe 1: 20 Civilian, 13 Industrial, 5 Competition, 3 Stealth, keine
        # Lücke — das Feld trägt also wirklich.
        kennung = teil.get('kennung') or ''
        # ⚠ Ist das Teil auch herstellbar, wird der vorhandene Eintrag
        # **ergänzt** statt ein zweiter angelegt — sonst stünde dasselbe Teil
        # zweimal in der Liste, einmal aus jeder Quelle.
        schon = gefunden.get(kennung)
        if schon is not None:
            schon['herkunft'] = BEIDES
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
            for feld, wert in (('klasse', teil.get('klasse')),
                               ('guete', teil.get('guete')),
                               ('hersteller', teil.get('hersteller'))):
                if wert:
                    schon[feld] = wert
            continue
        raus.append({'name': teil.get('name') or '',
                     'kennung': kennung,
                     'hersteller': teil.get('hersteller') or '',
                     'guete': teil.get('guete') or '',
                     'klasse': teil.get('klasse') or '',
                     'herkunft': KAUFBAR})
    raus.extend(gefunden.values())
    raus.sort(key=lambda x: (x['name'] or '').lower())
    return raus


# ------------------------------------------------------------- Die Auslegung
#
# ⚠ Alles hier arbeitet auf **einem Hangar-Eintrag** (ein Schiff aus
# `hangar.laden()['schiffe']`), nicht auf der ganzen Datei. Geschrieben wird
# in das Feld `belegung`, das dort seit v3.19.0-rc1 leer bereitliegt — es
# kostet also keinen Formatwechsel und entwertet keine bestehende Datei.


def belegung(eintrag):
    """Die gespeicherte Auslegung eines Schiffs: Pfad → Teil."""
    werte = (eintrag or {}).get('belegung')
    return werte if isinstance(werte, dict) else {}


def setzen(eintrag, pfad, ref, name, weg=KAUFEN):
    """Ein Teil in einen Steckplatz legen. Gibt zurück, ob sich etwas änderte.

    ⚠ Die **Kennung** ist der Inhalt, der Name nur die Beschriftung daneben.
    Wer später einen Preis dazu sucht, fragt über `ref`.
    """
    if not pfad or not ref:
        return False
    alt = belegung(eintrag).get(pfad)
    neu = {'ref': ref, 'name': name or '', 'weg': weg}
    if alt == neu:
        return False
    eintrag.setdefault('belegung', {})[pfad] = neu
    return True


def loeschen(eintrag, pfad):
    """Einen Steckplatz wieder auf die Werksausstattung zurücksetzen."""
    return (eintrag or {}).get('belegung', {}).pop(pfad, None) is not None


def weg_setzen(eintrag, pfad, weg):
    """Kaufen oder selbst herstellen — die Wahl an einem Posten.

    ⚠ Die Wahl gehört **in** die Auslegung, nicht in ein zweites Feld daneben.
    Zwei Wörterbücher, die über denselben Schlüssel laufen, laufen früher oder
    später auseinander — dann steht eine Wahl für einen Steckplatz da, in dem
    längst nichts mehr liegt.
    """
    if weg not in (KAUFEN, BAUEN):
        return False
    eintrag_platz = belegung(eintrag).get(pfad)
    if not eintrag_platz or eintrag_platz.get('weg') == weg:
        return False
    eintrag_platz['weg'] = weg
    return True


def erledigt(eintrag, pfad):
    """Ist dieser Posten abgehakt?"""
    return bool((belegung(eintrag).get(pfad) or {}).get('erledigt'))


def erledigt_setzen(eintrag, pfad, ja=True):
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
    platz = belegung(eintrag).get(pfad)
    if not platz:
        return False
    if bool(platz.get('erledigt')) == bool(ja):
        return False
    if ja:
        platz['erledigt'] = True
    else:
        platz.pop('erledigt', None)
    return True


# ------------------------------------------------------------- Die Posten


def _bauplan_verzeichnis():
    """Entitäts-Kennung → Name des Bauplans, der genau dieses Teil herstellt.

    ⚠⚠ **Die Umkehrung von `herstellung.entity_von()` — und sie ist der Grund,
    warum hier nichts über Namen läuft.** Der Warenkorb kennt zu jedem Teil nur
    seine Kennung (aus erkul). Um zu wissen, ob es dafür einen Bauplan gibt,
    braucht es den Weg von der Kennung zum Rezept, nicht umgekehrt.

    Der so gefundene Name stammt aus den **Rezeptdaten selbst** und wird nur
    dort wieder nachgeschlagen. Es ist also kein Namensabgleich über zwei
    Quellen hinweg — genau der Fehler, den `laeden.py` im Kopf beschreibt.
    """
    from . import herstellung
    raus = {}
    try:
        for b in herstellung.alle():
            kennung = b.get('entity') or ''
            if kennung:
                raus.setdefault(kennung, b.get('basis') or b.get('name') or '')
    except Exception as ausnahme:
        fehler.merken('warenkorb.bauplan_verzeichnis', ausnahme)
    return raus


def posten(eintrag):
    """Alles, was an diesem Schiff **nicht** ab Werk verbaut ist.

    Gibt `(zustand, liste)` zurück. Je Posten:

        {'pfad', 'art', 'groesse', 'ref', 'name',
         'werk_ref', 'werk_name',   # was stattdessen ab Werk drinsteckt
         'weg'}                     # KAUFEN oder BAUEN

    ⚠ **Ein ab Werk leerer Platz zählt mit.** Batterie, Bordrechner und
    Gravitationsgenerator stehen bei der Cutlass Black leer — legt der Spieler
    dort etwas hinein, ist das der offensichtlichste Warenkorb-Posten
    überhaupt: Dort fehlt etwas.

    ⚠ Und ein Platz, in den der Spieler **genau das Werksteil** legt, ist
    keiner. Verglichen wird über die Kennung.
    """
    if not eintrag:
        return KEINE_DATEN, []
    plaetze = erkul.steckplaetze(eintrag.get('name') or '',
                                 eintrag.get('hersteller') or '',
                                 eintrag.get('kurz') or '',
                                 eintrag.get('hkurz') or '')
    if not plaetze:
        # ⚠ Das ist **nicht** „nichts zu besorgen". Ohne Steckplatz-Daten ist
        # gar keine Aussage möglich, und die beiden Fälle dürfen nie denselben
        # Satz erzeugen.
        return KEINE_DATEN, []

    gewaehlt = belegung(eintrag)
    nach_pfad = dict((p.get('pfad'), p) for p in plaetze)
    raus = []
    for pfad, teil in gewaehlt.items():
        platz = nach_pfad.get(pfad)
        if not platz or not (teil or {}).get('ref'):
            # Ein Steckplatz, den es nicht mehr gibt — nach einem Patch
            # möglich. Er wird übergangen, nicht gemeldet: Der Spieler kann
            # nichts dafür, und ein Fehler wäre er auch nicht.
            continue
        werk = platz.get('werk') or {}
        if werk.get('ref') == teil['ref']:
            continue
        raus.append({
            'pfad': pfad,
            'art': platz.get('art') or '',
            'groesse': platz.get('groesse'),
            'ref': teil['ref'],
            'name': teil.get('name') or '',
            'werk_ref': werk.get('ref') or '',
            'werk_name': werk.get('name') or '',
            'weg': teil.get('weg') or KAUFEN,
            # ⚠ Ein abgehakter Posten bleibt in der Liste — er wird nur nicht
            # mehr mitgerechnet. Ihn verschwinden zu lassen hiesse, dass
            # niemand einen falsch gesetzten Haken zuruecknehmen kann.
            'erledigt': bool(teil.get('erledigt')),
        })
    raus.sort(key=lambda p: (p['art'], p['pfad']))
    return (OFFEN if raus else NICHTS_OFFEN), raus


# ------------------------------------------------------------- Die zwei Wege


def kaufweg(ref, name=''):
    """Was der Posten fertig im Laden kostet.

    Gibt `{'zustand', 'preis', 'laden', 'ort'}` zurück.

    ⚠ **Drei Zustände, und keiner davon ist eine Behauptung über das Spiel.**
    `NICHT_GEPRUEFT` heißt, dass noch niemand nachgesehen hat; `KEIN_PREIS`,
    dass UEX das Teil nicht führt. UEX hat Lücken (gemessen: 435 von 1.604
    Bauplänen) — daraus „nirgends im Handel" zu machen, wäre eine Aussage über
    fremde Daten, nicht über das Spiel.
    """
    from . import laeden
    leer = {'zustand': NICHT_GEPRUEFT, 'preis': None, 'laden': '', 'ort': ''}
    if not ref:
        return leer
    if not laeden.bekannt(ref):
        return leer
    bester = laeden.guenstigster(ref)
    if not bester:
        return {'zustand': KEIN_PREIS, 'preis': None, 'laden': '', 'ort': ''}
    preis, laden_name, ort = bester
    return {'zustand': BEKANNT, 'preis': preis, 'laden': laden_name,
            'ort': ort}


def bauweg(ref, verzeichnis=None):
    """Was der Posten an Material kostet, wenn er selbst hergestellt wird.

    Gibt `{'zustand', 'material', 'dauer', 'bauplan', 'ohne_preis'}` zurück.

    ⚠⚠ **`ohne_preis` ist keine Nebensache.** Ein Rohstoff mit Kaufpreis 0 ist
    nicht kostenlos, sondern **nicht kaufbar** — er muss abgebaut werden. Die
    Materialsumme ist dann eine **Untergrenze**, und wer das nicht dazusagt,
    lässt Selberbauen billiger aussehen, als es ist. Dieselbe Falle wie bei den
    Ankaufgeboten in `verkauf.py`.
    """
    from . import herstellung, preise
    leer = {'zustand': KEIN_REZEPT, 'material': None, 'dauer': None,
            'bauplan': '', 'ohne_preis': []}
    if not ref:
        return leer
    verzeichnis = _bauplan_verzeichnis() if verzeichnis is None else verzeichnis
    bauplan = verzeichnis.get(ref) or ''
    if not bauplan:
        return leer
    try:
        rez = herstellung.rezept(bauplan)
    except Exception as ausnahme:
        fehler.merken('warenkorb.bauweg.rezept', ausnahme)
        return leer
    if not rez or not rez.get('stufen'):
        return leer

    # Aktuell hat jeder Bauplan genau eine Stufe — gerechnet wird trotzdem
    # über alle, damit eine zweite nicht stillschweigend unterschlagen wird.
    material = 0.0
    ohne_preis = []
    dauer = 0
    for stufe in rez['stufen']:
        dauer += int(stufe.get('zeit') or 0)
        for _slot, rohstoff, menge, _guete in (stufe.get('zutaten') or []):
            gefunden = preise.preis(rohstoff)
            kauf = (gefunden or (0, 0, ''))[0]
            if not kauf:
                # Nicht kaufbar (oder gar keine Preisdaten) — der Posten wird
                # benannt, nicht mit 0 verrechnet.
                if rohstoff not in ohne_preis:
                    ohne_preis.append(rohstoff)
                continue
            material += float(kauf) * float(menge or 0)
    return {'zustand': BEKANNT, 'material': material, 'dauer': dauer,
            'bauplan': bauplan, 'ohne_preis': ohne_preis}


def anreichern(liste):
    """Jeden Posten um beide Wege ergänzen — `kauf` und `bau`.

    ⚠ Das Bauplan-Verzeichnis wird **einmal** gebaut, nicht je Posten: Es geht
    über rund 1.600 Baupläne, und bei zwölf Posten wären das zwölf Durchläufe
    für dieselbe Tabelle.
    """
    verzeichnis = _bauplan_verzeichnis()
    for p in liste:
        p['kauf'] = kaufweg(p.get('ref'), p.get('name'))
        p['bau'] = bauweg(p.get('ref'), verzeichnis)
        # ⚠ Ein Posten ohne Bauplan kann nicht gebaut werden — dann steht der
        # Weg auf „kaufen", ganz gleich, was gespeichert war. Sonst rechnet die
        # Summe mit einem Weg, den es nicht gibt.
        if p['weg'] == BAUEN and p['bau']['zustand'] != BEKANNT:
            p['weg'] = KAUFEN
    return liste


def summe(liste):
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
    gesamt = kaufteil = bauteil = 0.0
    dauer = 0
    offen = 0
    unvollstaendig = False
    for p in liste:
        # ⚠⚠ **Abgehaktes kostet nichts mehr.** Wer ein Teil gekauft und
        # eingebaut hat, will nicht, dass es weiter in der Summe steht — sonst
        # bleibt die Zahl gleich, egal wie viel man schon erledigt hat, und
        # die Liste verliert ihren Zweck. Es zaehlt aber auch nicht als
        # „fehlender Preis": Es fehlt nichts, es ist fertig.
        if p.get('erledigt'):
            continue
        if p.get('weg') == BAUEN:
            bau = p.get('bau') or {}
            if bau.get('zustand') != BEKANNT or bau.get('material') is None:
                offen += 1
                continue
            bauteil += bau['material']
            gesamt += bau['material']
            dauer += int(bau.get('dauer') or 0)
            if bau.get('ohne_preis'):
                unvollstaendig = True
        else:
            kauf = p.get('kauf') or {}
            if kauf.get('zustand') != BEKANNT or kauf.get('preis') is None:
                offen += 1
                continue
            kaufteil += kauf['preis']
            gesamt += kauf['preis']
    return {'gesamt': gesamt, 'kaufen': kaufteil, 'bauen': bauteil,
            'dauer': dauer, 'offen': offen,
            'unvollstaendig': unvollstaendig}


# ------------------------------------------------------------- Die Kaufroute


def _angebote(liste):
    """Je zu kaufendem Posten alle Läden, die ihn führen."""
    from . import laeden
    raus = {}
    for p in liste:
        if p.get('weg') != KAUFEN or not p.get('ref'):
            continue
        zeilen = laeden.laeden(p['ref'])
        if not zeilen:
            continue
        raus[p['pfad']] = zeilen
    return raus


def route(liste):
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
    angebote = _angebote(liste)
    nach_pfad = dict((p['pfad'], p) for p in liste)
    offen = set(angebote)
    ohne = [p['pfad'] for p in liste
            if p.get('weg') == KAUFEN and p['pfad'] not in angebote]

    # Ort → {Pfad → billigste Zeile dort}
    orte = {}
    for pfad, zeilen in angebote.items():
        for z in zeilen:
            schluessel = (z.get('system') or '', z.get('ort') or '')
            hier = orte.setdefault(schluessel, {})
            # ⚠ Am selben Ort kann dasselbe Teil in mehreren Terminals liegen —
            # es zählt einmal, und zwar mit dem billigsten Preis.
            if pfad not in hier or z['preis'] < hier[pfad]['preis']:
                hier[pfad] = z

    stopps = []
    while offen:
        bester = None
        for schluessel, hier in orte.items():
            deckt = offen & set(hier)
            if not deckt:
                continue
            kosten = sum(hier[p]['preis'] for p in deckt)
            # Viel Deckung zuerst, dann billig, dann nach Namen — der letzte
            # Schlüssel nur, damit dasselbe Ergebnis stabil bleibt.
            marke = (-len(deckt), kosten, schluessel)
            if bester is None or marke < bester[0]:
                bester = (marke, schluessel, deckt)
        if bester is None:
            break
        _marke, schluessel, deckt = bester
        hier = orte[schluessel]
        eintraege = []
        for pfad in sorted(deckt):
            z = hier[pfad]
            eintraege.append({
                'pfad': pfad,
                'name': (nach_pfad.get(pfad) or {}).get('name') or '',
                'preis': z['preis'],
                'laden': z.get('laden') or '',
            })
        stopps.append({
            'system': schluessel[0],
            'ort': schluessel[1],
            'laeden': sorted(set(e['laden'] for e in eintraege if e['laden'])),
            'posten': eintraege,
            'summe': sum(e['preis'] for e in eintraege),
        })
        offen -= deckt

    return stopps, ohne


def rechnung(daten=None):
    """Die Einkaufsliste über **alle** Schiffe — wie eine Rechnung.

    Gibt zurück::

        {'posten': [...],          # jeder mit Position, Preis und Weg
         'summe': {...},           # dieselbe Form wie `summe()`
         'schiffe': 3,             # wie viele Schiffe beteiligt sind
         'ohne_steckplatzdaten': ['Galaxy', …]}

    Je Posten:

        {'sorte': TEIL | SCHIFF,
         'schiff': 'Cutlass Black', 'quelle': HANGAR | WUNSCH,
         'position': 'Cooler S2',   # wo am Schiff — bei SCHIFF leer
         'name': 'BlastChill', 'ref': …, 'weg': KAUFEN | BAUEN,
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
    abgelegt ist. Was noch nachzuschlagen wäre, sagt `fehlende_preise()` — das
    Holen gehört in die Oberfläche, wo es im Hintergrund laufen kann, und nicht
    in eine Funktion, die beim Aufklappen einer Seite anhält.
    """
    from . import hangar, schiffe as alle_schiffe

    daten = daten if daten is not None else hangar.laden()
    verzeichnis = _bauplan_verzeichnis()
    raus = []
    ohne_daten = []
    beteiligt = set()

    quellen = ([(s, HANGAR) for s in (daten.get('schiffe') or [])]
               + [(w, WUNSCH) for w in (daten.get('wunsch') or [])])

    for eintrag, quelle in quellen:
        name = eintrag.get('name') or ''
        if not name:
            continue

        # 1. Das Schiff selbst — nur beim Wunsch, siehe oben.
        if quelle == WUNSCH:
            beteiligt.add(name)
            posten_schiff = {
                'sorte': SCHIFF, 'schiff': name, 'quelle': quelle,
                'position': '', 'name': name, 'ref': '',
                'weg': KAUFEN,
                'bau': {'zustand': KEIN_REZEPT, 'material': None,
                        'dauer': None, 'bauplan': '', 'ohne_preis': []},
            }
            try:
                stellen = alle_schiffe.kaufen(name)
            except Exception as ausnahme:
                fehler.merken('warenkorb.rechnung.schiffspreis', ausnahme)
                stellen = []
            if stellen:
                # ⚠ `kaufen()` gibt eine **Liste** von Verkaufsstellen zurück,
                # billigste zuerst — kein Tupel wie `laeden.guenstigster()`.
                bester = stellen[0]
                posten_schiff['kauf'] = {
                    'zustand': BEKANNT, 'preis': bester.get('preis'),
                    'laden': bester.get('stelle') or '',
                    'ort': bester.get('ort') or ''}
            else:
                # ⚠ Kein Preis heißt hier meistens „im Spiel nicht für aUEC zu
                # haben" (Konzeptschiff, nur gegen Echtgeld). Behauptet wird das
                # trotzdem nicht — es steht nur kein Preis da.
                posten_schiff['kauf'] = {'zustand': KEIN_PREIS, 'preis': None,
                                         'laden': '', 'ort': ''}
            raus.append(posten_schiff)

        # 2. Die Ausstattung — bei Hangar- und Wunschschiffen gleich.
        zustand, liste = posten(eintrag)
        if zustand == KEINE_DATEN:
            # ⚠ Nur vermerken, wenn jemand am Schiff überhaupt etwas vorhat.
            # Sonst stünden vierzig Konzeptschiffe als Mangel in der Rechnung.
            if belegung(eintrag):
                ohne_daten.append(name)
            continue
        for p in liste:
            beteiligt.add(name)
            p['kauf'] = kaufweg(p.get('ref'), p.get('name'))
            p['bau'] = bauweg(p.get('ref'), verzeichnis)
            if p['weg'] == BAUEN and p['bau']['zustand'] != BEKANNT:
                p['weg'] = KAUFEN
            position = p.get('art') or ''
            if p.get('groesse') is not None:
                position = '%s S%s' % (position, p['groesse'])
            p.update({'sorte': TEIL, 'schiff': name, 'quelle': quelle,
                      'position': position})
            raus.append(p)

    # Schiffe zuerst, dann ihre Teile — wie auf einer Rechnung, auf der die
    # Hauptposition über dem Zubehör steht.
    raus.sort(key=lambda p: ((p['schiff'] or '').lower(),
                             0 if p['sorte'] == SCHIFF else 1,
                             (p.get('position') or ''),
                             (p.get('name') or '').lower()))
    return {'posten': raus, 'summe': summe(raus), 'schiffe': len(beteiligt),
            'ohne_steckplatzdaten': sorted(set(ohne_daten))}


def fehlende_preise(posten_liste):
    """Zu welchen Posten der Ladenpreis noch nachzuschlagen ist.

    Gibt Paare `(kennung, name)` zurück — genau das, was `laeden.holen()`
    braucht.

    ⚠⚠ **Ohne diesen Schritt bleibt eine Rechnung auf `NICHT_GEPRUEFT`
    stehen** und zeigt Posten ohne Preis, obwohl UEX sie kennt. Der Abruf
    gehört aber nicht hierher: Er dauert je Teil eine Netzrunde, und eine
    Rechnung mit zwölf Posten würde die Oberfläche zwölf Mal anhalten. Die
    Anzeige holt sie im Hintergrund nach und zeichnet dann neu.
    """
    raus = []
    gesehen = set()
    for p in posten_liste or []:
        if p.get('sorte') == SCHIFF:
            # Schiffspreise kommen aus `schiffe.py`, nicht aus `laeden.py`.
            continue
        kennung = p.get('ref') or ''
        zustand = (p.get('kauf') or {}).get('zustand')
        if kennung and kennung not in gesehen and zustand == NICHT_GEPRUEFT:
            gesehen.add(kennung)
            raus.append((kennung, p.get('name') or ''))
    return raus


def farmliste(daten=None):
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
    Das ist die Falle, die `rohstoffe.pruefen()` allein nicht abfängt: Es
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
    `rechnung()` setzt den Weg schon auf „kaufen" zurück, sobald zu einem
    Posten kein Rezept vorliegt — hier kommt er dann gar nicht mehr an. Das
    Feld ist ein **Sicherheitsnetz** für den Fall, dass sich das einmal ändert
    oder ein Rezept zwischen den beiden Schritten wegfällt. Lieber ein Feld,
    das meistens leer ist, als ein stiller Verlust.
    """
    from . import herstellung, rohstoffe

    fertig = rechnung(daten)
    verzeichnis = _bauplan_verzeichnis()

    # 1. Bedarf einsammeln: (Rohstoff, Mindestgüte) -> Menge
    bedarf = {}
    ohne_rezept = []
    gebaut = 0
    for p in fertig['posten']:
        if p.get('weg') != BAUEN:
            continue
        gebaut += 1
        bauplan = verzeichnis.get(p.get('ref') or '') or ''
        rez = None
        if bauplan:
            try:
                rez = herstellung.rezept(bauplan)
            except Exception as ausnahme:
                fehler.merken('warenkorb.farmliste.rezept', ausnahme)
        if not rez or not rez.get('stufen'):
            ohne_rezept.append(p.get('name') or '')
            continue
        for stufe in rez['stufen']:
            for _slot, rohstoff, menge, guete in (stufe.get('zutaten') or []):
                schluessel = (herstellung.norm_rohstoff(rohstoff),
                              float(guete or 0))
                eintrag = bedarf.setdefault(schluessel,
                                            {'name': rohstoff, 'menge': 0.0})
                eintrag['menge'] += float(menge or 0)

    # 2. Je Rohstoff den Bestand zuteilen — anspruchsvollste Güte zuerst.
    nach_rohstoff = {}
    for (norm, guete), eintrag in bedarf.items():
        nach_rohstoff.setdefault(norm, []).append(
            (guete, eintrag['name'], eintrag['menge']))

    fehlt, vollstaendig = [], []
    for norm, gruppen in nach_rohstoff.items():
        # ⚠ Absteigend: Wer die höchste Güte verlangt, bekommt zuerst — und
        # nimmt dabei das **gerade noch ausreichende** Erz, damit das bessere
        # für nichts verschwendet wird, das es nicht braucht.
        gruppen.sort(reverse=True)
        posten = [dict(p) for p in rohstoffe.laden()
                  if herstellung.norm_rohstoff(p.get('material')) == norm]
        for guete, name, menge in gruppen:
            passend = sorted(
                (p for p in posten
                 if float(p.get('qualitaet') or 0) >= guete
                 and float(p.get('menge') or 0) > 0),
                key=lambda p: float(p.get('qualitaet') or 0))
            genommen = 0.0
            for p in passend:
                if genommen >= menge:
                    break
                da = float(p.get('menge') or 0)
                nimm = min(da, menge - genommen)
                p['menge'] = da - nimm
                genommen += nimm
            # Was zwar da ist, aber die Güte nicht schafft — als Hinweis, nicht
            # als Bestand. Das Lager wird von Hand gepflegt und kann hinterher
            # hinken; behauptet wird deshalb nichts.
            zu_gering = sum(float(p.get('menge') or 0) for p in posten
                            if float(p.get('qualitaet') or 0) < guete)
            zeile = {'rohstoff': name, 'benoetigt': menge,
                     'vorhanden': genommen,
                     'differenz': max(0.0, menge - genommen),
                     'mindestguete': guete, 'zu_gering': zu_gering}
            (fehlt if zeile['differenz'] > 0 else vollstaendig).append(zeile)

    fehlt.sort(key=lambda z: (-z['differenz'], z['rohstoff'].lower()))
    vollstaendig.sort(key=lambda z: z['rohstoff'].lower())
    return {'fehlt': fehlt, 'vollstaendig': vollstaendig, 'posten': gebaut,
            'ohne_rezept': sorted(set(x for x in ohne_rezept if x))}


def route_summe(stopps):
    """Was diese Route kostet, und wie weit sie führt.

    Gibt `{'gesamt', 'stopps', 'laeden', 'systeme'}` zurück.

    ⚠ **Diese Zahl gehört an die Route, nicht die aus `summe()`.** Sie ist in
    aller Regel etwas höher, weil an einem Ort nicht alles zum Bestpreis liegt
    — siehe die Warnung dort. Angezeigt wird sie deshalb als „auf dieser
    Route", damit niemand die beiden für dieselbe Angabe hält.
    """
    return {
        'gesamt': sum(s['summe'] for s in stopps),
        'stopps': len(stopps),
        'laeden': sum(len(s['laeden']) for s in stopps),
        'systeme': len(set(s['system'] for s in stopps if s['system'])),
    }
