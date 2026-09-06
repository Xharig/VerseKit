# SPDX-License-Identifier: GPL-3.0-only
#
# SC BP Watcher — Totzone, Sättigung und Kurve je Achse
# Copyright (C) 2026 Xharig
#
# This program is free software: you can redistribute it and/or modify it under
# the terms of the GNU General Public License version 3 as published by the
# Free Software Foundation.
#
# This program is distributed in the hope that it will be useful, but WITHOUT
# ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
# FOR A PARTICULAR PURPOSE.  See the GNU General Public License for more
# details.
#
# You should have received a copy of the GNU General Public License along with
# this program.  If not, see <https://www.gnu.org/licenses/>.
"""
Wie scharf reagiert eine Achse — und gilt diese Einstellung überhaupt noch?

## Zwei Ebenen, die oft verwechselt werden

Star Citizen stellt eine Achse an **zwei verschiedenen Stellen** ein, und beide
stehen in derselben `actionmaps.xml`. Wer das durcheinanderbringt, sucht seine
Einstellung an der falschen Stelle.

| Ebene | Element | Gilt für | Was dort steht |
|---|---|---|---|
| **physisch** | `<deviceoptions name="Gerät {Kennung}">` | die Achse am Gerät (`x`, `rotz`, `slider1`) | Totzone, Sättigung |
| **logisch** | `<options type="joystick" instance="N">` | die Achse im Spiel (`flight_move_pitch`) | Exponent, Invertierung, Kurve |

Die Totzone gilt also für **alles**, was auf dieser Achse liegt; der Exponent
nur für die eine Flugfunktion. Beides zusammen ergibt, was der Spieler spürt.

## ⚠⚠ Die Kennung entscheidet, nicht der Name

Gemessen am 06.09.2026 an einem echten Aufbau: Für **einen** Stick standen
**drei** `<deviceoptions>`-Blöcke in der Datei, alle unter demselben Namen,
aber mit drei verschiedenen Kennungen — und nur einer davon gehörte zum
tatsächlich angeschlossenen Gerät.

    LEFT VPC Stick WarBRD-D  {03F3…}   Totzone 0,099                ← aktiv
    LEFT VPC Stick WarBRD-D  {83F3…}   Totzone 0,099 + Sättigung    ← Leiche
    LEFT VPC Stick WarBRD-D  {83F4…}   Totzone 0,396                ← Leiche

Die Folge war handfest: Die Sättigung, die der Spieler eingestellt hatte,
hing an einer Kennung, die es nicht mehr gab — sie war **wirkungslos**, und
im Spiel ist das nirgends zu sehen. Der rechte Stick lief mit Sättigung, der
linke ohne, bei gleichem Namen und gleicher Beschriftung.

**Genau deshalb gibt es dieses Modul.** Ein Editor, der nur Werte anzeigt,
hätte 0,7425 angezeigt und damit gelogen.

Woher eine Kennung ihre Gültigkeit bezieht:

| Quelle | Bedeutung |
|---|---|
| `joysticks.geraete()` — die Game.log | das Gerät war zuletzt wirklich angeschlossen |
| `joysticks.zuordnung()` — die Belegung | das Gerät hat eine `js`-Nummer, Belegungen hängen daran |

Steht eine Kennung in **keiner** von beiden, ist ihr Block eine Karteileiche.

## ⚠ Mehrfache Einträge sind der Normalfall, nicht die Ausnahme

Star Citizen **hängt an, statt zu ersetzen**. In derselben Messung:

- Jeder Sättigungswert stand **doppelt** hintereinander — jedes Mal.
- Totzone und Sättigung derselben Achse stehen in **getrennten** `<option>`-
  Elementen, nicht zusammen in einem.
- Ein Gerät hatte **zwei Blöcke mit derselben Kennung** und widersprüchlichen
  Werten (x-Totzone 0,297 gegen 0,099).

Beim Lesen gilt deshalb: **der letzte Eintrag gewinnt.** Das ist die Annahme,
die zum Anhänge-Verhalten passt — ein Wert, den das Spiel zuletzt geschrieben
hat, ist der, den der Spieler zuletzt eingestellt hat.

⚠ Diese Annahme ist **nicht am laufenden Spiel gegengeprüft**. Wer sie prüfen
will: zwei widersprüchliche Werte für dieselbe Achse eintragen, Spiel starten,
im Einstellungsbildschirm nachsehen, welcher ankommt. Bis dahin steht sie hier
als das, was sie ist — eine begründete Annahme, keine Messung.

## Was dieses Modul NICHT tut

Es schreibt nichts von allein — wie das ganze Nachbarmodul `joysticks.py`.
Gelesen wird jederzeit, geschrieben nur auf Knopfdruck, und dann über
`joysticks._schreiben()`, das vorher eine Sicherung anlegt.
"""
import os
import re
import shutil
import time

from . import joysticks

# Die physischen Achsen, die in einer `actionmaps.xml` vorkommen können.
# ⚠ Die Reihenfolge ist die, in der sie in der Oberfläche erscheinen sollen —
# erst die beiden Hauptachsen, dann Drehung, dann die Schieber.
ACHSEN = ('x', 'y', 'z', 'rotx', 'roty', 'rotz', 'slider1', 'slider2')

# Was an einer physischen Achse einstellbar ist, mit erlaubtem Wertebereich.
# Beide sind Anteile von 0 bis 1: Totzone ist der tote Bereich um die Mitte,
# Sättigung der Punkt, ab dem der Vollausschlag erreicht gilt.
EIGENSCHAFTEN = {
    'deadzone':   (0.0, 1.0),
    'saturation': (0.0, 1.0),
}

# ⚠⚠ **Was gilt, wenn nichts in der Datei steht — und das ist NICHT 0.**
#
# Fehlt die Sättigung, nutzt das Spiel den vollen Weg: 1,0. Fehlt die
# Totzone, gibt es keine: 0,0. Wer beide gleich behandelt, baut eine Falle —
# ein Regler, der bei fehlender Sättigung links auf 0 steht, schreibt beim
# ersten Anfassen einen Wert, nach dem der Stick fast nicht mehr steuert.
#
# Dieselbe Tabelle gilt für die Kurvenrechnung in `antwort()`; die Werte
# stehen hier, damit Oberfläche und Rechnung nicht auseinanderlaufen.
STANDARD = {
    'deadzone':   0.0,
    'saturation': 1.0,
    'exponent':   1.0,
}

# Was an einer Spielachse einstellbar ist.
# ⚠ `exponent` ist KEIN Anteil — gemessen wurden 1, 1.1, 1.5 und 3. Ein Wert
# unter 1 macht die Mitte grober, über 1 feiner. Die Grenzen hier sind großzügig
# gewählt; das Spiel selbst schreibt nichts außerhalb.
SPIEL_EIGENSCHAFTEN = {
    'exponent': (0.1, 10.0),
    'invert':   (0, 1),
}

# Ein `<deviceoptions>`-Block, mit oder ohne Inhalt.
BLOCK = re.compile(
    r'<deviceoptions\b[^>]*?/>|<deviceoptions\b.*?</deviceoptions>', re.S)

# Ein einzelner `<option …/>`-Eintrag darin.
EINTRAG = re.compile(r'<option\s+([^>]*?)/>')

# Ein Attribut in einem solchen Eintrag.
ATTRIBUT = re.compile(r'(\w+)="([^"]*)"')

# Die Kennung in geschweiften Klammern, wie überall im Projekt.
KENNUNG = re.compile(r'\{([0-9A-Fa-f-]{8,})\}')


def _zahl(text):
    """Einen Attributwert in eine Zahl wandeln — oder `None`.

    Das Spiel schreibt Fließkommazahlen in voller Breite (`0.098999992`).
    Gerundet wird erst bei der Anzeige, nie beim Lesen: Wer hier rundet und
    zurückschreibt, ändert Werte, die der Spieler nicht angefasst hat.
    """
    try:
        return float(text)
    except (TypeError, ValueError):
        return None


def _kennung_aus(text):
    """Die reine Kennung aus einem Namen mit geschweiftem Anhang."""
    treffer = KENNUNG.search(text or '')
    return treffer.group(1).upper() if treffer else ''


def _name_ohne_kennung(text):
    """Der Gerätename ohne die geschweifte Kennung, sauber beschnitten."""
    return KENNUNG.sub('', text or '').strip()


def gueltige_kennungen(ordner=None, datei=None):
    """Welche Geräte-Kennungen gelten aktuell als lebendig?

    Zusammengetragen aus beiden Quellen, die das Nachbarmodul kennt: was das
    Spiel zuletzt verbunden hatte, und was in der Belegung eine Nummer hat.
    Eine Kennung aus **einer** der beiden reicht — ein Stick, der gerade
    abgesteckt ist, aber eine `js`-Nummer hat, ist keine Karteileiche.
    """
    lebendig = set()
    try:
        for geraet in joysticks.geraete(ordner) or []:
            if geraet.get('kennung'):
                lebendig.add(geraet['kennung'].upper())
    except Exception:
        pass
    try:
        for eintrag in joysticks.zuordnung(datei, ordner) or []:
            if eintrag.get('kennung'):
                lebendig.add(eintrag['kennung'].upper())
    except Exception:
        pass
    return lebendig


def geraete_achsen(datei=None, ordner=None):
    """Was an den physischen Achsen eingestellt ist — je `<deviceoptions>`-Block.

    Liefert eine Liste von Blöcken in der Reihenfolge der Datei. Jeder Block:

    | Feld | Bedeutung |
    |---|---|
    | `name` | Gerätename ohne Kennung |
    | `kennung` | die geschweifte Kennung, groß geschrieben |
    | `aktiv` | gilt der Block noch? (Kennung ist verbunden oder belegt) |
    | `achsen` | `{'x': {'deadzone': 0.099, 'saturation': None}, …}` |
    | `mehrfach` | Achsen, für die es widersprüchliche Einträge gab |

    ⚠ **`aktiv=False` heißt: die Werte hier wirken nicht.** Sie stehen in der
    Datei, sie sehen echt aus, und das Spiel ignoriert sie. Die Oberfläche muss
    das deutlich zeigen — sonst stellt der Spieler etwas ein, das nichts tut.
    """
    weg = datei or joysticks._pfad_actionmaps(ordner)
    if not weg:
        return []
    try:
        with open(weg, 'r', encoding='utf-8', errors='replace') as f:
            text = f.read()
    except Exception:
        return []

    lebendig = gueltige_kennungen(ordner, datei)
    heraus = []
    for treffer in BLOCK.finditer(text):
        block = treffer.group(0)
        kopf = re.match(r'<deviceoptions[^>]*>', block)
        kopf = kopf.group(0) if kopf else ''
        name_roh = re.search(r'name="([^"]*)"', kopf)
        name_roh = name_roh.group(1) if name_roh else ''
        kennung = _kennung_aus(name_roh)

        achsen = {}
        mehrfach = set()
        for eintrag in EINTRAG.finditer(block):
            attribute = dict(ATTRIBUT.findall(eintrag.group(1)))
            achse = attribute.pop('input', '')
            if not achse:
                continue
            ziel = achsen.setdefault(achse, {})
            for schluessel, wert in attribute.items():
                if schluessel not in EIGENSCHAFTEN:
                    continue
                neu = _zahl(wert)
                alt = ziel.get(schluessel)
                # ⚠ Der LETZTE gewinnt (siehe Modulkopf). Ein Widerspruch wird
                # gemerkt, damit die Oberfläche ihn zeigen kann — ein doppelter
                # IDENTISCHER Wert ist dagegen der Normalfall und kein Hinweis.
                if alt is not None and neu is not None and alt != neu:
                    mehrfach.add(achse)
                ziel[schluessel] = neu

        # Jede bekannte Eigenschaft auftauchen lassen, auch wenn sie fehlt —
        # „nicht gesetzt" ist eine Aussage und soll in der Oberfläche stehen.
        for achse in achsen:
            for schluessel in EIGENSCHAFTEN:
                achsen[achse].setdefault(schluessel, None)

        heraus.append({
            'name': _name_ohne_kennung(name_roh),
            'kennung': kennung,
            # ⚠ Ohne Kennung ist nichts zu beurteilen. Maus und Tastatur
            # stehen ohne geschweiften Anhang in der Datei — sie als tot zu
            # melden wäre schlicht falsch.
            'aktiv': (not kennung) or kennung in lebendig,
            'achsen': achsen,
            'mehrfach': sorted(mehrfach),
            'roh': block,
        })
    _widersprueche_ueber_bloecke(heraus)
    _leichen_einordnen(heraus)
    return heraus


def _widersprueche_ueber_bloecke(bloecke):
    """Widersprüche finden, die über zwei Blöcke derselben Kennung gehen.

    Gemessen: Ein Gerät stand **zweimal mit derselben Kennung** in der Datei,
    einmal mit x-Totzone 0,297 und einmal mit 0,099. Innerhalb eines Blocks
    fällt das nicht auf — dafür muss man die Blöcke zusammenlegen.
    """
    nach_kennung = {}
    for block in bloecke:
        if block['kennung']:
            nach_kennung.setdefault(block['kennung'], []).append(block)

    for gruppe in nach_kennung.values():
        if len(gruppe) < 2:
            continue
        gesehen = {}
        for block in gruppe:
            for achse, eigenschaften in block['achsen'].items():
                for name, wert in eigenschaften.items():
                    if wert is None:
                        continue
                    schluessel = (achse, name)
                    if schluessel in gesehen and gesehen[schluessel] != wert:
                        for teil in gruppe:
                            if achse in teil['achsen']:
                                teil['mehrfach'] = sorted(
                                    set(teil['mehrfach']) | {achse})
                    gesehen[schluessel] = wert


def _leichen_einordnen(bloecke):
    """Einen toten Block danach unterscheiden, ob sein Gerät noch da ist.

    Das ist der Unterschied zwischen „egal" und „hier ist dir etwas verloren
    gegangen":

    | Lage | Feld | Bedeutung |
    |---|---|---|
    | Gerät gibt es gar nicht mehr | `verwaist` | alter Stick, verkauft, eingelagert — Altpapier |
    | Gerät ist da, aber unter **neuer** Kennung | `ueberholt` | die Einstellung ist übernehmbar |

    Erkannt wird das am **Namen**: Steht derselbe Gerätename auch in einem
    aktiven Block, dann hat dasselbe Gerät eine neue Kennung bekommen.

    ⚠ Der Name ist im ganzen Projekt sonst tabu — hier ist er zulässig, weil
    er nichts entscheidet, sondern nur einen **Hinweis** einordnet. Geschrieben
    wird daraufhin nichts; der Spieler bekommt den Fund gezeigt und entscheidet.
    """
    aktive_namen = {block['name'] for block in bloecke
                    if block['aktiv'] and block['name']}
    for block in bloecke:
        tot = not block['aktiv']
        block['ueberholt'] = tot and block['name'] in aktive_namen
        block['verwaist'] = tot and not block['ueberholt']


def spielachsen(datei=None, ordner=None):
    """Was an den Spielachsen eingestellt ist — je `<options type=…>`-Block.

    Liefert je Block ein Wörterbuch mit `art` (`joystick`, `keyboard`,
    `gamepad`), `nummer` (die `instance`), `name`, `kennung` und `achsen`.
    Eine Achse trägt `exponent`, `invert` und `kurve`.

    `kurve` ist eine Liste von `(ein, aus)`-Paaren aus `<nonlinearity_curve>`.
    ⚠ **In allen gemessenen Dateien war dieser Block leer** — das Spiel legt
    ihn an, füllt ihn aber erst, wenn der Spieler im Kurven-Bildschirm etwas
    verschiebt. Eine leere Kurve bedeutet „gerade Linie", nicht „kaputt".
    """
    weg = datei or joysticks._pfad_actionmaps(ordner)
    if not weg:
        return []
    try:
        with open(weg, 'r', encoding='utf-8', errors='replace') as f:
            text = f.read()
    except Exception:
        return []

    lebendig = gueltige_kennungen(ordner, datei)
    muster = re.compile(
        r'<options\b[^>]*?/>|<options\b.*?</options>', re.S)
    heraus = []
    for treffer in muster.finditer(text):
        block = treffer.group(0)
        kopf = re.match(r'<options[^>]*>', block)
        kopf = kopf.group(0) if kopf else ''
        art = re.search(r'type="([^"]*)"', kopf)
        art = (art.group(1) if art else '').lower()
        produkt = re.search(r'Product="([^"]*)"', kopf)
        produkt = produkt.group(1) if produkt else ''
        if not produkt.strip():
            # Ein leerer Platzhalter (`<options type="joystick" instance="7"/>`)
            # sagt nichts aus — das Spiel legt acht davon an.
            continue
        nummer = re.search(r'instance="(\d+)"', kopf)
        nummer = int(nummer.group(1)) if nummer else 0
        kennung = _kennung_aus(produkt)

        achsen = {}
        # ⚠⚠ **Erst den Kopf abschneiden, dann nach Kindern suchen.**
        #
        # Der erste Entwurf suchte die Kinder im ganzen Block — und das erste,
        # was der Regex fand, war `<options …>` **selbst**: Er verschlang alle
        # 351 Zeichen, und weil `finditer` nicht überlappend sucht, gab es
        # danach keinen Treffer mehr. Ergebnis: `achsen` blieb immer leer, ohne
        # eine einzige Fehlermeldung.
        #
        # Ein Muster, das Kinder sucht, darf das Elternelement nicht sehen
        # können. Deshalb wird hier der Bereich zwischen dem ersten `>` und
        # dem schließenden `</options>` herausgeschnitten.
        inneres = re.match(r'<options\b[^>]*>(.*)</options>\s*$', block, re.S)
        inhalt_block = inneres.group(1) if inneres else ''

        # Jedes Kind-Element ist eine Spielachse. Sie kann selbstschließend
        # sein (`<flight_view exponent="1"/>`) oder eine Kurve enthalten.
        kinder = re.finditer(
            r'<(\w+)((?:\s+\w+="[^"]*")*)\s*(?:/>|>(.*?)</\1>)',
            inhalt_block, re.S)
        for kind in kinder:
            achse = kind.group(1)
            if achse in ('nonlinearity_curve', 'point'):
                continue
            attribute = dict(ATTRIBUT.findall(kind.group(2) or ''))
            inhalt = kind.group(3) or ''
            kurve = [(_zahl(a), _zahl(b)) for a, b in
                     re.findall(r'<point\s+in="([^"]*)"\s+out="([^"]*)"',
                                inhalt)]
            achsen[achse] = {
                'exponent': _zahl(attribute.get('exponent')),
                'invert': (None if 'invert' not in attribute
                           else _zahl(attribute.get('invert'))),
                'kurve': kurve,
                'hat_kurvenblock': 'nonlinearity_curve' in inhalt,
            }

        heraus.append({
            'art': art,
            'nummer': nummer,
            'name': _name_ohne_kennung(produkt),
            'kennung': kennung,
            'aktiv': (not kennung) or kennung in lebendig,
            'achsen': achsen,
        })
    return heraus


def leichen(datei=None, ordner=None, bloecke=None):
    """Die Blöcke, deren Einstellungen nicht mehr wirken.

    Das ist der Befund, der einem Spieler am meisten bringt: „Du hast hier
    etwas eingestellt, und es tut nichts." Geliefert werden nur Blöcke, die
    **überhaupt einen Wert tragen** — ein leerer toter Block ist kein Problem,
    sondern nur Altpapier.

    Sortiert: **überholte zuerst.** Bei ihnen steht das Gerät noch am Tisch,
    nur unter neuer Kennung — dort lohnt sich das Hinsehen. Verwaiste Blöcke
    gehören zu Geräten, die es nicht mehr gibt; die sind bloß Ballast.
    """
    if bloecke is None:
        bloecke = geraete_achsen(datei, ordner)
    heraus = []
    for block in bloecke:
        if block['aktiv']:
            continue
        hat_werte = any(
            any(wert is not None for wert in eigenschaften.values())
            for eigenschaften in block['achsen'].values())
        if hat_werte:
            heraus.append(block)
    heraus.sort(key=lambda b: (not b.get('ueberholt'), b['name']))
    return heraus


def uebernehmbar(datei=None, ordner=None, bloecke=None):
    """Was ließe sich aus einem überholten Block in den aktiven übernehmen?

    Der Fall, für den das hier gebaut ist: Ein Stick hat eine neue Kennung
    bekommen (anderer USB-Anschluss, Firmware, Neuinstallation). Das Spiel
    legt ihn als neues Gerät an — **ohne** die Einstellungen. Die alten stehen
    weiter in der Datei und tun nichts.

    Geliefert wird je Fall ein Wörterbuch:

    | Feld | Bedeutung |
    |---|---|
    | `name` | der Gerätename, der in beiden Blöcken steht |
    | `alt` / `neu` | der überholte und der aktive Block |
    | `werte` | `[(Achse, Eigenschaft, alter Wert, jetziger Wert)]` |

    In `werte` steht **nur, was sich unterscheidet** — und zwar in beide
    Richtungen: Ein Wert, der im alten Block steht und im neuen fehlt, ist
    verloren gegangen; ein abweichender Wert ist eine stille Änderung.

    ⚠ **Es wird nichts übernommen.** Diese Funktion stellt fest, sie handelt
    nicht — wie das ganze Modul.
    """
    if bloecke is None:
        bloecke = geraete_achsen(datei, ordner)

    aktive = {}
    for block in bloecke:
        if block['aktiv'] and block['name']:
            # Bei mehreren aktiven Blöcken gleichen Namens gewinnt der letzte,
            # aus demselben Grund wie bei den Einzelwerten.
            aktive[block['name']] = block

    heraus = []
    for block in bloecke:
        if not block.get('ueberholt'):
            continue
        ziel = aktive.get(block['name'])
        if ziel is None:
            continue
        unterschiede = []
        for achse, eigenschaften in block['achsen'].items():
            for name, alt in eigenschaften.items():
                if alt is None:
                    continue
                jetzt = (ziel['achsen'].get(achse) or {}).get(name)
                # ⚠⚠ **Mit Toleranz vergleichen.** Das Spiel schreibt
                # `0.098999992`, das Werkzeug `0.099` — zwei Zahlen, die in
                # der Anzeige beide als „0.1" erscheinen. Ohne Toleranz stand
                # deshalb „Totzone: war 0.1 → jetzt 0.1" im Befund: ein
                # Unterschied, den niemand sehen kann und der keiner ist.
                # Ein Tausendstel Totzone spürt kein Mensch.
                if jetzt is not None and abs(jetzt - alt) < 1e-3:
                    continue
                if jetzt != alt:
                    unterschiede.append((achse, name, alt, jetzt))
        if unterschiede:
            unterschiede.sort(key=lambda z: (ACHSEN.index(z[0])
                                             if z[0] in ACHSEN else 99, z[1]))
            heraus.append({'name': block['name'], 'alt': block, 'neu': ziel,
                           'werte': unterschiede})
    return heraus


def antwort(eingabe, totzone=0.0, saettigung=1.0, exponent=1.0, kurve=None):
    """Was kommt hinten heraus, wenn der Stick um `eingabe` ausgelenkt ist?

    Das ist die Rechnung hinter der Kurve, die Star Citizen im
    Einstellungsbildschirm zeichnet — und die einzige Art, einem Spieler zu
    zeigen, was seine drei Zahlen zusammen eigentlich anrichten. Totzone,
    Sättigung und Exponent einzeln als Zahl zu lesen, sagt nämlich fast nichts.

    | Schritt | Wirkung |
    |---|---|
    | Totzone | alles darunter wird zu 0 — die tote Mitte |
    | Sättigung | ab hier gilt Vollausschlag, der Rest des Wegs ist wirkungslos |
    | Exponent | über 1 macht die Mitte feiner, unter 1 gröber |
    | Kurve | liegen Punkte vor, gewinnen sie über den Exponenten |

    `eingabe` läuft von -1 bis 1; das Vorzeichen bleibt erhalten, gerechnet
    wird auf dem Betrag. Das ist der Grund, warum die Quadranten-Ansicht
    überhaupt genügt: Die Kurve ist punktsymmetrisch, die andere Hälfte ist
    ihr Spiegelbild.

    ⚠ **Diese Formel ist nachgebaut, nicht aus dem Spiel entnommen.** Sie gibt
    das übliche Verhalten wieder (Totzone abschneiden, auf den Restweg neu
    aufspannen, Exponent anwenden) und stimmt an den Eckpunkten nachweislich:
    bei Totzone 0 / Sättigung 1 / Exponent 1 kommt die Gerade heraus. Ob CIG
    im Detail identisch rechnet, ist damit **nicht** gesagt — die Anzeige ist
    eine gute Vorschau, kein Beweis.
    """
    try:
        eingabe = float(eingabe)
    except (TypeError, ValueError):
        return 0.0

    vorzeichen = -1.0 if eingabe < 0 else 1.0
    betrag = abs(eingabe)
    if betrag > 1.0:
        betrag = 1.0

    totzone = 0.0 if totzone is None else max(0.0, min(1.0, float(totzone)))
    saettigung = 1.0 if saettigung is None else max(0.0, min(1.0,
                                                            float(saettigung)))
    exponent = 1.0 if exponent is None else float(exponent)

    if betrag <= totzone:
        return 0.0

    # ⚠ Sättigung unterhalb der Totzone wäre ein Widerspruch — dann bliebe
    # kein Weg übrig, auf dem sich überhaupt etwas ändern kann. Statt durch
    # Null zu teilen, gilt dann alles jenseits der Totzone als Vollausschlag.
    spanne = saettigung - totzone
    if spanne <= 0:
        return vorzeichen

    anteil = (betrag - totzone) / spanne
    if anteil > 1.0:
        anteil = 1.0

    if kurve:
        return vorzeichen * _aus_kurve(anteil, kurve)

    if exponent > 0 and exponent != 1.0:
        anteil = anteil ** exponent
    return vorzeichen * anteil


def _aus_kurve(anteil, kurve):
    """Zwischen den gesetzten Punkten geradlinig ablesen.

    Die Punkte kommen aus `<nonlinearity_curve>` und sind auf 0..1 normiert.
    Zwischen zwei Punkten wird linear interpoliert — dieselbe Vereinfachung,
    die auch das Zeichnen benutzt, und für eine Vorschau genau genug.
    """
    punkte = sorted((a, b) for a, b in kurve
                    if a is not None and b is not None)
    if not punkte:
        return anteil
    # Die Enden festnageln, damit außerhalb nicht ins Leere gelesen wird.
    if punkte[0][0] > 0:
        punkte.insert(0, (0.0, 0.0))
    if punkte[-1][0] < 1:
        punkte.append((1.0, 1.0))

    for nr in range(len(punkte) - 1):
        links_x, links_y = punkte[nr]
        rechts_x, rechts_y = punkte[nr + 1]
        if links_x <= anteil <= rechts_x:
            breite = rechts_x - links_x
            if breite <= 0:
                return rechts_y
            lage = (anteil - links_x) / breite
            return links_y + lage * (rechts_y - links_y)
    return punkte[-1][1]


def verlauf(totzone=0.0, saettigung=1.0, exponent=1.0, kurve=None,
            schritte=120, ganz=False):
    """Die Kurve als Liste von `(ein, aus)`-Paaren — fertig zum Zeichnen.

    `ganz=False` liefert den **Quadranten** (0 bis 1) — die Ansicht, die Star
    Citizen selbst zeigt und in der man tatsächlich etwas erkennt.
    `ganz=True` liefert die **Vollansicht** (-1 bis 1), in der die Kurve als
    Ganzes durch den Nullpunkt läuft.

    ⚠ `schritte` bestimmt nur die Feinheit der Zeichnung, nicht das Ergebnis.
    Tk kennt keine Kurven — gezeichnet wird ein Streckenzug, und der braucht
    genug Stützstellen, damit der Knick an der Totzone nicht wie eine Rundung
    aussieht.
    """
    schritte = max(2, int(schritte))
    anfang = -1.0 if ganz else 0.0
    weite = 1.0 - anfang
    heraus = []
    for nr in range(schritte + 1):
        ein = anfang + weite * nr / schritte
        heraus.append((ein, antwort(ein, totzone, saettigung, exponent, kurve)))
    return heraus


def setzen(kennung, achse, eigenschaft, wert, datei=None, ordner=None):
    """Totzone oder Sättigung einer physischen Achse schreiben.

    | | |
    |---|---|
    | `kennung` | die geschweifte Kennung des Geräts, **ohne** Klammern |
    | `achse` | `x`, `rotz`, `slider1` … |
    | `eigenschaft` | `deadzone` oder `saturation` |
    | `wert` | Zahl von 0 bis 1, oder `None` zum Entfernen |

    ⚠⚠ **Es wird über die Kennung gegangen, nie über den Namen.** Derselbe
    Name steht in dieser Datei mehrfach, für verschiedene Geräte. Ein Schreiben
    nach Namen träfe irgendeinen Block — womöglich eine Karteileiche, und der
    Spieler wunderte sich, warum nichts passiert.

    ⚠ **Alle Doppel werden dabei eingesammelt.** Star Citizen schreibt jeden
    Sättigungswert doppelt und legt bei Bedarf weitere Einträge an; würde hier
    nur der erste geändert, bliebe der zweite mit dem alten Wert stehen und
    gewönne (der letzte gewinnt). Es bleibt genau **ein** Eintrag je Achse und
    Eigenschaft übrig.

    ⚠ **Nur bei geschlossenem Spiel aufrufen** — Star Citizen schreibt die
    Datei beim Beenden selbst und überschriebe die Änderung.

    Liefert `(erfolg, meldung, anzahl)` wie die Schreibfunktionen nebenan.
    """
    import xml.etree.ElementTree as ET

    from . import fehler

    weg = datei or joysticks._pfad_actionmaps(ordner)
    if not weg:
        return False, 's_js_f_datei', 0
    if eigenschaft not in EIGENSCHAFTEN:
        return False, 's_kv_f_eigenschaft', 0
    if wert is not None:
        unten, oben = EIGENSCHAFTEN[eigenschaft]
        try:
            wert = float(wert)
        except (TypeError, ValueError):
            return False, 's_kv_f_wert', 0
        if not (unten <= wert <= oben):
            return False, 's_kv_f_bereich', 0

    kennung = (kennung or '').upper()
    if not kennung:
        return False, 's_kv_f_kennung', 0

    try:
        baum = ET.parse(weg)
    except Exception as ausnahme:
        fehler.merken('kurven.setzen_lesen', ausnahme)
        return False, 's_js_f_lesen', 0

    ziel = None
    for knoten in baum.getroot().iter('deviceoptions'):
        if _kennung_aus(knoten.get('name') or '') == kennung:
            # Bei mehreren Blöcken derselben Kennung gewinnt der letzte —
            # also wird auch dort geschrieben, wo das Spiel zuletzt schrieb.
            ziel = knoten
    if ziel is None:
        return False, 's_kv_f_geraet', 0

    behalten = None
    entfernt = 0
    for eintrag in list(ziel.findall('option')):
        if (eintrag.get('input') or '') != achse:
            continue
        if eigenschaft not in eintrag.attrib:
            continue
        if behalten is None:
            behalten = eintrag
        else:
            ziel.remove(eintrag)
            entfernt += 1

    if wert is None:
        if behalten is not None:
            # Nur das eine Attribut löschen — trägt der Eintrag noch etwas
            # anderes (die andere Eigenschaft), bleibt er stehen.
            behalten.attrib.pop(eigenschaft, None)
            if not [k for k in behalten.attrib if k != 'input']:
                ziel.remove(behalten)
    else:
        # ⚠⚠ **`repr()`, nicht `%g`.**
        #
        # Das Spiel schreibt die volle Fließkommabreite (`0.098999992`).
        # `'%g'` kürzt auf sechs Stellen und machte daraus `0.099` — ein
        # anderer Wert, obwohl der Spieler diese Achse gar nicht angefasst
        # hatte. Aufgefallen beim Anwenden eines Gerätesatzes: Die Vorschau
        # kündigte **eine** Änderung an, geschrieben wurden **zehn**, weil
        # jeder unveränderte Wert durch das Runden zur Änderung wurde.
        #
        # Der Modulkopf sagt das für das Lesen bereits ausdrücklich — beim
        # Schreiben gilt es genauso. `repr()` liefert die kürzeste
        # Darstellung, die exakt wieder eingelesen wird.
        text = repr(float(wert))
        if behalten is None:
            behalten = ET.SubElement(ziel, 'option')
            behalten.set('input', achse)
        behalten.set(eigenschaft, text)

    return joysticks._schreiben(weg, baum, 1 + entfernt)


def spiel_setzen(nummer, achse, eigenschaft, wert, datei=None, ordner=None):
    """Exponent oder Invertierung einer Spielachse schreiben.

    `nummer` ist die `instance` — das `n` in `js<n>_`. Anders als bei den
    physischen Achsen ist sie hier der Bezugspunkt, weil das Spiel die
    Spielachsen an der Nummer führt und die Kennung nur danebensteht.

    Liefert `(erfolg, meldung, anzahl)`.
    """
    import xml.etree.ElementTree as ET

    from . import fehler

    weg = datei or joysticks._pfad_actionmaps(ordner)
    if not weg:
        return False, 's_js_f_datei', 0
    if eigenschaft not in SPIEL_EIGENSCHAFTEN:
        return False, 's_kv_f_eigenschaft', 0
    if wert is not None:
        unten, oben = SPIEL_EIGENSCHAFTEN[eigenschaft]
        try:
            wert = float(wert)
        except (TypeError, ValueError):
            return False, 's_kv_f_wert', 0
        if not (unten <= wert <= oben):
            return False, 's_kv_f_bereich', 0

    try:
        baum = ET.parse(weg)
    except Exception as ausnahme:
        fehler.merken('kurven.spiel_setzen_lesen', ausnahme)
        return False, 's_js_f_lesen', 0

    ziel = None
    for knoten in baum.getroot().iter('options'):
        if (knoten.get('type') or '').lower() != 'joystick':
            continue
        try:
            if int(knoten.get('instance') or 0) == int(nummer):
                ziel = knoten
                break
        except ValueError:
            continue
    if ziel is None:
        return False, 's_kv_f_geraet', 0

    knoten = ziel.find(achse)
    if wert is None:
        if knoten is not None:
            knoten.attrib.pop(eigenschaft, None)
            # Ein Element ohne Attribute und ohne Kurve sagt nichts mehr aus.
            if not knoten.attrib and len(knoten) == 0:
                ziel.remove(knoten)
    else:
        if knoten is None:
            knoten = ET.SubElement(ziel, achse)
        text = ('%d' % int(wert)) if eigenschaft == 'invert' else ('%g' % wert)
        knoten.set(eigenschaft, text)

    return joysticks._schreiben(weg, baum, 1)


# Wie die Aktion in der Belegung zum Element in `<options>` heißt.
#
# ⚠⚠ **Gemessen, nicht geraten** (06.09.2026 an einer echten Datei):
#
#     <action name="v_pitch">        <rebind input="js2_y"/>
#     <options …><flight_move_pitch exponent="1.5"/>
#
# Die Belegung nennt die Aktion `v_pitch`, die Einstellung heißt
# `flight_move_pitch`. Drei Formen kommen vor, und die Reihenfolge zählt:
# `v_view_pitch` muss VOR `v_pitch` geprüft werden, sonst würde es als
# „view_pitch" unter `flight_move_` einsortiert.
AKTION_ZU_ACHSE = (
    ('v_view_', 'flight_view_'),
    ('v_mining_', 'mining_'),
    ('v_', 'flight_move_'),
)


def _achsenname(aktion):
    """Aus dem Aktionsnamen der Belegung den Namen in `<options>` machen."""
    for vorn, ersatz in AKTION_ZU_ACHSE:
        if aktion.startswith(vorn):
            return ersatz + aktion[len(vorn):]
    return aktion


def funktionen_je_achse(nummer, achsen, datei=None, ordner=None):
    """Für mehrere physische Achsen auf einmal: was darauf liegt.

    ⚠ **Eine Dateilesung für alle Achsen, nicht eine je Achse.**
    `spielachsen_auf()` liest die `actionmaps.xml` bei jedem Aufruf neu — für
    eine Tabelle mit acht Zeilen wären das acht Lesungen einer Datei, die
    zwanzigtausend Zeichen hat, und das bei jedem Zeichnen der Seite.

    Gibt `{achse: [funktionen]}` zurück; Achsen ohne Funktion fehlen.
    """
    weg = datei or joysticks._pfad_actionmaps(ordner)
    if not weg:
        return {}
    try:
        with open(weg, 'r', encoding='utf-8', errors='replace') as f:
            text = f.read()
    except Exception:
        return {}
    raus = {}
    for achse in achsen:
        treffer = spielachsen_auf(nummer, achse, datei=weg)
        if treffer:
            raus[achse] = treffer
    return raus


def spielachsen_auf(nummer, achse, datei=None, ordner=None):
    """Welche Spielachsen liegen auf dieser physischen Achse?

    ⭐ **Warum das gebraucht wird:** Die Empfindlichkeit (der Exponent) hängt
    nicht an der physischen Achse `y`, sondern an der Spielachse
    `flight_move_pitch`. Wer sie einstellen will, muss wissen, welche
    Spielachse überhaupt auf welchem Stickweg liegt — und das steht nur in
    der Belegung.

    ⚠ **Es sind oft MEHRERE.** Gemessen lagen auf `js2_y` gleichzeitig
    `v_pitch` und `v_strafe_vertical`, jede mit eigener Empfindlichkeit. Ein
    einzelner Regler je physischer Achse wäre also schlicht falsch.

    Liefert je Treffer ein Wörterbuch mit `achse` (Name in `<options>`),
    `aktion` (Name in der Belegung), `exponent` und `invert`.
    """
    weg = datei or joysticks._pfad_actionmaps(ordner)
    if not weg:
        return []
    try:
        with open(weg, 'r', encoding='utf-8', errors='replace') as f:
            text = f.read()
    except Exception:
        return []

    gesucht = 'js%s_%s' % (nummer, achse)
    aktionen = []
    # Jede `<action name="…">` mit ihren `<rebind>`-Kindern durchgehen.
    for treffer in re.finditer(
            r'<action\s+name="([^"]*)"\s*>(.*?)</action>', text, re.S):
        name, inhalt = treffer.group(1), treffer.group(2)
        for bindung in re.finditer(r'<rebind\s+input="([^"]*)"', inhalt):
            if bindung.group(1).strip() == gesucht:
                aktionen.append(name)
                break

    # Die Einstellungen dieser Nummer dazuholen.
    werte = {}
    for block in spielachsen(datei, ordner):
        if block['art'] == 'joystick' and block['nummer'] == int(nummer):
            werte = block['achsen']
            break

    heraus = []
    gesehen = set()
    for aktion in aktionen:
        name = _achsenname(aktion)
        if name in gesehen:
            continue
        gesehen.add(name)
        eigenschaften = werte.get(name) or {}
        heraus.append({'achse': name, 'aktion': aktion,
                       'exponent': eigenschaften.get('exponent'),
                       'invert': eigenschaften.get('invert')})
    return heraus


def aufraeumen(datei=None, ordner=None, nur_zaehlen=False):
    """Tote `<deviceoptions>`-Blöcke aus der Belegungsdatei entfernen.

    ⭐ **Warum das nötig ist:** Star Citizen legt bei jeder neuen
    Gerätekennung einen weiteren Block an und räumt nie auf. An einem echten
    Aufbau standen für **einen** Stick drei Blöcke — und weil sie sich
    untereinander widersprechen, wurde man den Hinweis „diese Einstellungen
    wirken nicht mehr" nie los: Übernahm man den einen, wich der nächste ab.

    Entfernt werden **nur** Blöcke, deren Kennung zu keinem verbundenen und
    zu keinem belegten Gerät gehört. Was gerade gilt, bleibt unangetastet.

    ⚠ Über Textausschnitte, nicht über den XML-Baum — wie beim
    Kennungstausch. Es wird genau der Bereich eines Blocks herausgeschnitten,
    sonst nichts; Einrückung und Kommentare des Spiels bleiben, wie sie sind.

    Mit `nur_zaehlen=True` wird nichts geschrieben, sondern nur gemeldet, wie
    viele Blöcke wegfielen — für die Rückfrage vor dem Löschen.

    Liefert `(erfolg, meldung, anzahl)`.
    """
    from . import fehler

    weg = datei or joysticks._pfad_actionmaps(ordner)
    if not weg or not os.path.isfile(weg):
        return False, 's_js_f_datei', 0
    try:
        with open(weg, 'r', encoding='utf-8', errors='replace') as f:
            inhalt = f.read()
    except Exception as ausnahme:
        fehler.merken('kurven.aufraeumen_lesen', ausnahme)
        return False, 's_js_f_lesen', 0

    lebendig = gueltige_kennungen(ordner, datei)
    schnitte = []
    for treffer in BLOCK.finditer(inhalt):
        kopf = re.match(r'<deviceoptions[^>]*>', treffer.group(0))
        name = re.search(r'name="([^"]*)"', kopf.group(0) if kopf else '')
        kennung = _kennung_aus(name.group(1) if name else '')
        # ⚠ Ein Block OHNE Kennung (Maus, Tastatur) ist nicht tot, sondern
        # nur nicht zuordenbar — der bleibt.
        if kennung and kennung not in lebendig:
            schnitte.append((treffer.start(), treffer.end()))

    if not schnitte:
        return False, 's_gs_f_nichts_zu_tun', 0
    if nur_zaehlen:
        return True, '', len(schnitte)

    # Von hinten nach vorn schneiden, sonst verschieben sich die Stellen.
    neu = inhalt
    for anfang, ende in reversed(schnitte):
        # Die Leerzeile mitnehmen, die der Block hinterlässt.
        nach = ende
        while nach < len(neu) and neu[nach] in ' \t':
            nach += 1
        if nach < len(neu) and neu[nach] == '\n':
            nach += 1
        vor = anfang
        while vor > 0 and neu[vor - 1] in ' \t':
            vor -= 1
        neu = neu[:vor] + neu[nach:]

    sicherung = '%s.scbpw-%s' % (weg, time.strftime('%Y%m%d-%H%M%S'))
    try:
        shutil.copy2(weg, sicherung)
    except Exception as ausnahme:
        fehler.merken('kurven.aufraeumen_sicherung', ausnahme)
        return False, 's_js_f_sicherung', 0
    try:
        with open(weg, 'w', encoding='utf-8', newline='') as f:
            f.write(neu)
    except Exception as ausnahme:
        try:
            shutil.copy2(sicherung, weg)
        except Exception:
            pass
        fehler.merken('kurven.aufraeumen_schreiben', ausnahme)
        return False, 's_js_f_schreiben', 0
    return True, sicherung, len(schnitte)


def angleichen(von_kennung, nach_kennung, datei=None, ordner=None):
    """Alle Achsenwerte eines Geräts auf ein anderes übertragen.

    ⭐ **Wofür das da ist:** Wer zwei Sticks fliegt, will auf beiden Seiten
    dasselbe Gefühl. Von Hand sind das ein Dutzend Mal dieselbe Zahl — und
    einmal vertippt fällt es erst im Gefecht auf.

    Übertragen werden Totzone und Sättigung **nur für Achsen, die es auf
    beiden Geräten gibt**. Ein Pedalsatz hat kein `rotx`; einen Wert dafür zu
    erfinden wäre schlimmer als keiner.

    ⚠ **Auch ein fehlender Wert wird übertragen — als Löschen.** Hat die
    Quelle keine Sättigung und das Ziel eine, muss sie weg; sonst sind die
    beiden hinterher eben nicht gleich, und genau das war der Zweck.

    Liefert `(erfolg, meldung, anzahl)`; `anzahl` ist die Zahl der
    geschriebenen Werte.
    """
    von_kennung = (von_kennung or '').upper()
    nach_kennung = (nach_kennung or '').upper()
    if not von_kennung or not nach_kennung:
        return False, 's_kv_f_kennung', 0
    if von_kennung == nach_kennung:
        return False, 's_kv_f_geraet', 0

    bloecke = geraete_achsen(datei, ordner)
    quelle = ziel = None
    for block in bloecke:
        # Bei mehreren Blöcken derselben Kennung gewinnt der letzte — dieselbe
        # Regel wie überall in diesem Modul.
        if block['kennung'] == von_kennung:
            quelle = block
        if block['kennung'] == nach_kennung:
            ziel = block
    if quelle is None or ziel is None:
        return False, 's_kv_f_geraet', 0

    gemeinsam = [a for a in ACHSEN
                 if a in quelle['achsen'] and a in ziel['achsen']]
    if not gemeinsam:
        return False, 's_ac_nichts_gemeinsam', 0

    # ⚠ Nur schreiben, was sich unterscheidet. `setzen()` legt bei jedem
    # Aufruf eine Sicherung an — zwölf blinde Schreibvorgänge hinterließen
    # zwölf Sicherungsdateien für meist zwei echte Änderungen.
    anzahl = 0
    for achse in gemeinsam:
        for eigenschaft in EIGENSCHAFTEN:
            wert = quelle['achsen'][achse].get(eigenschaft)
            ist = ziel['achsen'][achse].get(eigenschaft)
            if ist == wert:
                continue
            erfolg, meldung, _ = setzen(nach_kennung, achse, eigenschaft,
                                        wert, datei, ordner)
            if not erfolg:
                return False, meldung, anzahl
            anzahl += 1
    return True, '', anzahl


def zusammenfassung(datei=None, ordner=None):
    """Ein Überblick für die Oberfläche — was gilt, was nicht, wo klemmt es.

    | Feld | Bedeutung |
    |---|---|
    | `bloecke` | alle physischen Blöcke, aktive zuerst |
    | `leichen` | tote Blöcke, die trotzdem Werte tragen |
    | `uebernehmbar` | Gerät da, Einstellung an alter Kennung hängengeblieben |
    | `spiel` | die Spielachsen-Blöcke |
    | `widersprueche` | `[(Gerät, Achse)]` — mehrfach mit verschiedenen Werten |

    ⚠ Die Datei wird dabei **einmal** gelesen und das Ergebnis weitergereicht.
    Vorher las jede Teilfunktion sie neu — bei einer Seite, die beim Tippen
    neu zeichnet, wären das mehrere Dateizugriffe je Tastendruck.
    """
    bloecke = geraete_achsen(datei, ordner)
    widersprueche = []
    for block in bloecke:
        for achse in block['mehrfach']:
            widersprueche.append((block['name'], achse))
    return {
        'bloecke': sorted(bloecke, key=lambda b: (not b['aktiv'], b['name'])),
        'leichen': leichen(bloecke=bloecke),
        'uebernehmbar': uebernehmbar(bloecke=bloecke),
        'spiel': spielachsen(datei, ordner),
        'widersprueche': widersprueche,
    }
