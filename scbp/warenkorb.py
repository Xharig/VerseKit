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


def auswahl(art, groesse):
    """Welche kaufbaren Teile in einen Steckplatz dieser Art und Größe passen.

    Gibt eine Liste `{'name', 'kennung', 'hersteller', 'guete'}` zurück,
    alphabetisch.

    ⚠⚠ **Geschlossene Liste, kein Freitext** — dieselbe Regel wie beim
    Lagerort und beim Handelslager. Angenommen wird nur, was UEX kennt; sonst
    steht am Ende ein ausgedachter oder beleidigender Name im Werkzeug, und ein
    Bildschirmfoto davon macht die Runde.

    ⚠ Kennt die Tabelle die Art nicht, kommt eine **leere** Liste zurück — und
    die Anzeige sagt das, statt wahllos den halben Katalog anzubieten. Ein
    Kühler, der in einem Waffenplatz zur Auswahl steht, ist schlimmer als gar
    keine Auswahl.
    """
    from . import laeden
    if not art:
        return []
    gruppen = [g for g, a in GRUPPE_ZU_ART.items() if a == art]
    if not gruppen:
        return []
    raus = []
    for teil in laeden.katalog_teile():
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
        raus.append({'name': teil.get('name') or '',
                     'kennung': teil.get('kennung') or '',
                     'hersteller': teil.get('hersteller') or '',
                     'guete': teil.get('guete') or '',
                     'klasse': teil.get('klasse') or ''})
    raus.sort(key=lambda x: x['name'].lower())
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
