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
| `joysticks.devices()` — die Game.log | das Gerät war zuletzt wirklich angeschlossen |
| `joysticks.assignment()` — die Belegung | das Gerät hat eine `js`-Nummer, Belegungen hängen daran |

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
`joysticks._write()`, das vorher eine Sicherung anlegt.
"""
import os
import re
import shutil
import time

from . import joysticks

# Die physischen Achsen, die in einer `actionmaps.xml` vorkommen können.
# ⚠ Die Reihenfolge ist die, in der sie in der Oberfläche erscheinen sollen —
# erst die beiden Hauptachsen, dann Drehung, dann die Schieber.
AXES = ('x', 'y', 'z', 'rotx', 'roty', 'rotz', 'slider1', 'slider2')

# Was an einer physischen Achse einstellbar ist, mit erlaubtem Wertebereich.
# Beide sind Anteile von 0 bis 1: Totzone ist der tote Bereich um die Mitte,
# Sättigung der Punkt, ab dem der Vollausschlag erreicht gilt.
PROPERTIES = {
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
DEFAULT = {
    'deadzone':   0.0,
    'saturation': 1.0,
    'exponent':   1.0,
}

# Was an einer Spielachse einstellbar ist.
# ⚠ `exponent` ist KEIN Anteil — gemessen wurden 1, 1.1, 1.5 und 3. Ein Wert
# unter 1 macht die Mitte grober, über 1 feiner. Die Grenzen hier sind großzügig
# gewählt; das Spiel selbst schreibt nichts außerhalb.
GAME_PROPERTIES = {
    'exponent': (0.1, 10.0),
    'invert':   (0, 1),
}

# Ein `<deviceoptions>`-Block, mit oder ohne Inhalt.
BLOCK_RE = re.compile(
    r'<deviceoptions\b[^>]*?/>|<deviceoptions\b.*?</deviceoptions>', re.S)

# Ein einzelner `<option …/>`-Eintrag darin.
ENTRY_RE = re.compile(r'<option\s+([^>]*?)/>')

# Ein Attribut in einem solchen Eintrag.
ATTRIBUTE_RE = re.compile(r'(\w+)="([^"]*)"')

# Die Kennung in geschweiften Klammern, wie überall im Projekt.
IDENT_RE = re.compile(r'\{([0-9A-Fa-f-]{8,})\}')


def _number(text):
    """Einen Attributwert in eine Zahl wandeln — oder `None`.

    Das Spiel schreibt Fließkommazahlen in voller Breite (`0.098999992`).
    Gerundet wird erst bei der Anzeige, nie beim Lesen: Wer hier rundet und
    zurückschreibt, ändert Werte, die der Spieler nicht angefasst hat.
    """
    try:
        return float(text)
    except (TypeError, ValueError):
        return None


def _ident_from(text):
    """Die reine Kennung aus einem Namen mit geschweiftem Anhang."""
    match = IDENT_RE.search(text or '')
    return match.group(1).upper() if match else ''


def _name_without_ident(text):
    """Der Gerätename ohne die geschweifte Kennung, sauber beschnitten."""
    return IDENT_RE.sub('', text or '').strip()


def valid_idents(folder=None, filename=None):
    """Welche Geräte-Kennungen gelten aktuell als lebendig?

    Zusammengetragen aus beiden Quellen, die das Nachbarmodul kennt: was das
    Spiel zuletzt verbunden hatte, und was in der Belegung eine Nummer hat.
    Eine Kennung aus **einer** der beiden reicht — ein Stick, der gerade
    abgesteckt ist, aber eine `js`-Nummer hat, ist keine Karteileiche.
    """
    alive = set()
    try:
        for device in joysticks.devices(folder) or []:
            if device.get('kennung'):
                alive.add(device['kennung'].upper())
    except Exception:
        pass
    try:
        for entry in joysticks.assignment(filename, folder) or []:
            if entry.get('kennung'):
                alive.add(entry['kennung'].upper())
    except Exception:
        pass
    return alive


def device_axes(filename=None, folder=None):
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
    gone = filename or joysticks._actionmaps_path(folder)
    if not gone:
        return []
    try:
        with open(gone, 'r', encoding='utf-8', errors='replace') as f:
            text = f.read()
    except Exception:
        return []

    alive = valid_idents(folder, filename)
    out = []
    for match in BLOCK_RE.finditer(text):
        block = match.group(0)
        head = re.match(r'<deviceoptions[^>]*>', block)
        head = head.group(0) if head else ''
        name_raw = re.search(r'name="([^"]*)"', head)
        name_raw = name_raw.group(1) if name_raw else ''
        ident = _ident_from(name_raw)

        axes = {}
        duplicates = set()
        for entry in ENTRY_RE.finditer(block):
            attributes = dict(ATTRIBUTE_RE.findall(entry.group(1)))
            axis = attributes.pop('input', '')
            if not axis:
                continue
            target = axes.setdefault(axis, {})
            for key, value in attributes.items():
                if key not in PROPERTIES:
                    continue
                fresh = _number(value)
                previous = target.get(key)
                # ⚠ Der LETZTE gewinnt (siehe Modulkopf). Ein Widerspruch wird
                # gemerkt, damit die Oberfläche ihn zeigen kann — ein doppelter
                # IDENTISCHER Wert ist dagegen der Normalfall und kein Hinweis.
                if previous is not None and fresh is not None and previous != fresh:
                    duplicates.add(axis)
                target[key] = fresh

        # Jede bekannte Eigenschaft auftauchen lassen, auch wenn sie fehlt —
        # „nicht gesetzt" ist eine Aussage und soll in der Oberfläche stehen.
        for axis in axes:
            for key in PROPERTIES:
                axes[axis].setdefault(key, None)

        out.append({
            'name': _name_without_ident(name_raw),
            'kennung': ident,
            # ⚠ Ohne Kennung ist nichts zu beurteilen. Maus und Tastatur
            # stehen ohne geschweiften Anhang in der Datei — sie als tot zu
            # melden wäre schlicht falsch.
            'aktiv': (not ident) or ident in alive,
            'achsen': axes,
            'mehrfach': sorted(duplicates),
            'roh': block,
        })
    _keep_only_managed(out, gone)
    _conflicts_across_blocks(out)
    _sort_orphans(out)
    return out


def _managed_names(gone):
    """{Kennung: Name}, wie das Spiel die Geraete **gerade** nennt.

    Gelesen aus den `<options type="joystick" Product="…">`-Koepfen — dort
    steht der Name, den das Spiel beim letzten Schreiben benutzt hat.
    """
    dropped = {}
    try:
        with open(gone, 'r', encoding='utf-8', errors='replace') as f:
            text = f.read()
    except Exception:
        return dropped
    for head in re.finditer(r'<options\b[^>]*>', text):
        raw = head.group(0)
        if 'type="joystick"' not in raw:
            continue
        name = re.search(r'Product="([^"]*)"', raw)
        if not name:
            continue
        ident = _ident_from(name.group(1))
        if ident:
            dropped[ident] = _name_without_ident(name.group(1))
    return dropped


def _keep_only_managed(blocks, gone):
    """Bei mehreren Bloecken einer Kennung ist nur **einer** aktiv.

    ⚠⚠⚠ **Sonst steht dasselbe Geraet zweimal in der Leiste.** Am 06.09.2026
    zeigte die Seite fuenf Reiter fuer drei Sticks:

        L-VPC Stick WarBRD-D      <- so nennt das Linux-Spiel ihn
        LEFT VPC Stick WarBRD-D   <- so hiess er unter Windows
        R-VPC Stick WarBRD-D
        RIGHT VPC Stick WarBRD-D
        VPC Rudder Pedals

    Beide Namen tragen **dieselbe Kennung** — es ist ein Stick, mit einem
    alten und einem neuen Namen. `aktiv` haengt aber nur an der Kennung, und
    die stimmt bei beiden. Also galten beide als lebendig, mit
    verschiedenen Werten darin. Der Spieler stellte etwas ein und traf dabei
    womoeglich den toten Eintrag: *„kuemmer dich mal um die falschen Sticks,
    die nerven."*

    **Es gewinnt der Name, den das Spiel gerade fuehrt.** Steht kein
    gefuehrter Name zur Verfuegung (aeltere Datei, fremder Aufbau), bleibt
    alles wie es war — lieber einen Reiter zuviel als den richtigen
    weggeraeumt.
    """
    managed = _managed_names(gone)
    if not managed:
        return
    per_ident = {}
    for block in blocks:
        if block['kennung']:
            per_ident.setdefault(block['kennung'], []).append(block)
    for ident, group in per_ident.items():
        if len(group) < 2:
            continue
        name = managed.get(ident)
        if not name:
            continue
        # ⚠ Nur eingreifen, wenn der gefuehrte Name wirklich dabei ist. Passt
        # keiner, weiss niemand, welcher gilt — dann lieber nichts tun.
        match = [b for b in group if b['name'] == name]
        if not match:
            continue
        for block in group:
            block['aktiv'] = block is match[0]


def _conflicts_across_blocks(blocks):
    """Widersprüche finden, die über zwei Blöcke derselben Kennung gehen.

    Gemessen: Ein Gerät stand **zweimal mit derselben Kennung** in der Datei,
    einmal mit x-Totzone 0,297 und einmal mit 0,099. Innerhalb eines Blocks
    fällt das nicht auf — dafür muss man die Blöcke zusammenlegen.

    ⚠⚠⚠ **Verglichen wird nur innerhalb desselben Zustands** — aktive gegen
    aktive, tote gegen tote. Ein toter Block wirkt nicht; was dort steht, kann
    dem, was gilt, gar nicht widersprechen.

    Seit `_nur_der_gefuehrte_bleibt` denselben Stick unter altem und neuem
    Namen auseinanderhält, stand sonst an **jeder** Achse ein Warndreieck: Der
    aktive Block trug die eingestellten Werte, der alte Windows-Block die
    Werksangaben — beide zusammen ergaben einen Widerspruch, der keiner war.
    Gemeldet am 06.09.2026 mit der Frage, was die gelben Dreiecke überhaupt
    bedeuten. Eine Warnung, die überall steht, sagt nichts mehr.

    ⚠ **Nicht einfach die toten weglassen.** Der erste Versuch tat genau das
    und brach die ältere Prüfung: Zwei **tote** Blöcke derselben Kennung mit
    verschiedenen Werten sind sehr wohl ein Widerspruch — er gehört nur in die
    Liste der Alteinträge, nicht an eine gültige Achse. Nach Zustand gruppieren
    hält beides auseinander.
    """
    by_ident = {}
    for block in blocks:
        if block['kennung']:
            key = (block['kennung'], bool(block.get('aktiv')))
            by_ident.setdefault(key, []).append(block)

    for group in by_ident.values():
        if len(group) < 2:
            continue
        seen = {}
        for block in group:
            for axis, props in block['achsen'].items():
                for name, value in props.items():
                    if value is None:
                        continue
                    key = (axis, name)
                    if key in seen and seen[key] != value:
                        for part in group:
                            if axis in part['achsen']:
                                part['mehrfach'] = sorted(
                                    set(part['mehrfach']) | {axis})
                    seen[key] = value


def _sort_orphans(blocks):
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
    active_names = {block['name'] for block in blocks
                    if block['aktiv'] and block['name']}
    for block in blocks:
        dead = not block['aktiv']
        block['ueberholt'] = dead and block['name'] in active_names
        block['verwaist'] = dead and not block['ueberholt']


def game_axes(filename=None, folder=None):
    """Was an den Spielachsen eingestellt ist — je `<options type=…>`-Block.

    Liefert je Block ein Wörterbuch mit `art` (`joystick`, `keyboard`,
    `gamepad`), `nummer` (die `instance`), `name`, `kennung` und `achsen`.
    Eine Achse trägt `exponent`, `invert` und `kurve`.

    `kurve` ist eine Liste von `(ein, aus)`-Paaren aus `<nonlinearity_curve>`.
    ⚠ **In allen gemessenen Dateien war dieser Block leer** — das Spiel legt
    ihn an, füllt ihn aber erst, wenn der Spieler im Kurven-Bildschirm etwas
    verschiebt. Eine leere Kurve bedeutet „gerade Linie", nicht „kaputt".
    """
    gone = filename or joysticks._actionmaps_path(folder)
    if not gone:
        return []
    try:
        with open(gone, 'r', encoding='utf-8', errors='replace') as f:
            text = f.read()
    except Exception:
        return []

    alive = valid_idents(folder, filename)
    pattern = re.compile(
        r'<options\b[^>]*?/>|<options\b.*?</options>', re.S)
    out = []
    for match in pattern.finditer(text):
        block = match.group(0)
        head = re.match(r'<options[^>]*>', block)
        head = head.group(0) if head else ''
        kind = re.search(r'type="([^"]*)"', head)
        kind = (kind.group(1) if kind else '').lower()
        product = re.search(r'Product="([^"]*)"', head)
        product = product.group(1) if product else ''
        if not product.strip():
            # Ein leerer Platzhalter (`<options type="joystick" instance="7"/>`)
            # sagt nichts aus — das Spiel legt acht davon an.
            continue
        number = re.search(r'instance="(\d+)"', head)
        number = int(number.group(1)) if number else 0
        ident = _ident_from(product)

        axes = {}
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
        inner = re.match(r'<options\b[^>]*>(.*)</options>\s*$', block, re.S)
        block_content = inner.group(1) if inner else ''

        # Jedes Kind-Element ist eine Spielachse. Sie kann selbstschließend
        # sein (`<flight_view exponent="1"/>`) oder eine Kurve enthalten.
        children = re.finditer(
            r'<(\w+)((?:\s+\w+="[^"]*")*)\s*(?:/>|>(.*?)</\1>)',
            block_content, re.S)
        for child in children:
            axis = child.group(1)
            if axis in ('nonlinearity_curve', 'point'):
                continue
            attributes = dict(ATTRIBUTE_RE.findall(child.group(2) or ''))
            content = child.group(3) or ''
            curve = [(_number(a), _number(b)) for a, b in
                     re.findall(r'<point\s+in="([^"]*)"\s+out="([^"]*)"',
                                content)]
            axes[axis] = {
                'exponent': _number(attributes.get('exponent')),
                'invert': (None if 'invert' not in attributes
                           else _number(attributes.get('invert'))),
                'kurve': curve,
                'hat_kurvenblock': 'nonlinearity_curve' in content,
            }

        out.append({
            'art': kind,
            'nummer': number,
            'name': _name_without_ident(product),
            'kennung': ident,
            'aktiv': (not ident) or ident in alive,
            'achsen': axes,
        })
    return out


def orphans(filename=None, folder=None, blocks=None):
    """Die Blöcke, deren Einstellungen nicht mehr wirken.

    Das ist der Befund, der einem Spieler am meisten bringt: „Du hast hier
    etwas eingestellt, und es tut nichts." Geliefert werden nur Blöcke, die
    **überhaupt einen Wert tragen** — ein leerer toter Block ist kein Problem,
    sondern nur Altpapier.

    Sortiert: **überholte zuerst.** Bei ihnen steht das Gerät noch am Tisch,
    nur unter neuer Kennung — dort lohnt sich das Hinsehen. Verwaiste Blöcke
    gehören zu Geräten, die es nicht mehr gibt; die sind bloß Ballast.
    """
    if blocks is None:
        blocks = device_axes(filename, folder)
    out = []
    for block in blocks:
        if block['aktiv']:
            continue
        has_values = any(
            any(value is not None for value in props.values())
            for props in block['achsen'].values())
        if has_values:
            out.append(block)
    out.sort(key=lambda b: (not b.get('ueberholt'), b['name']))
    return out


def adoptable(filename=None, folder=None, blocks=None):
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
    if blocks is None:
        blocks = device_axes(filename, folder)

    active = {}
    for block in blocks:
        if block['aktiv'] and block['name']:
            # Bei mehreren aktiven Blöcken gleichen Namens gewinnt der letzte,
            # aus demselben Grund wie bei den Einzelwerten.
            active[block['name']] = block

    out = []
    for block in blocks:
        if not block.get('ueberholt'):
            continue
        target = active.get(block['name'])
        if target is None:
            continue
        differences = []
        for axis, props in block['achsen'].items():
            for name, previous in props.items():
                if previous is None:
                    continue
                now = (target['achsen'].get(axis) or {}).get(name)
                # ⚠⚠ **Mit Toleranz vergleichen.** Das Spiel schreibt
                # `0.098999992`, das Werkzeug `0.099` — zwei Zahlen, die in
                # der Anzeige beide als „0.1" erscheinen. Ohne Toleranz stand
                # deshalb „Totzone: war 0.1 → jetzt 0.1" im Befund: ein
                # Unterschied, den niemand sehen kann und der keiner ist.
                # Ein Tausendstel Totzone spürt kein Mensch.
                if now is not None and abs(now - previous) < 1e-3:
                    continue
                if now != previous:
                    differences.append((axis, name, previous, now))
        if differences:
            differences.sort(key=lambda z: (AXES.index(z[0])
                                             if z[0] in AXES else 99, z[1]))
            out.append({'name': block['name'], 'alt': block, 'neu': target,
                           'werte': differences})
    return out


def answer(input_name, deadzone_value=0.0, saturation=1.0, exponent=1.0, curve=None):
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
        input_name = float(input_name)
    except (TypeError, ValueError):
        return 0.0

    sign = -1.0 if input_name < 0 else 1.0
    amount = abs(input_name)
    if amount > 1.0:
        amount = 1.0

    deadzone_value = 0.0 if deadzone_value is None else max(0.0, min(1.0, float(deadzone_value)))
    saturation = 1.0 if saturation is None else max(0.0, min(1.0,
                                                            float(saturation)))
    exponent = 1.0 if exponent is None else float(exponent)

    if amount <= deadzone_value:
        return 0.0

    # ⚠ Sättigung unterhalb der Totzone wäre ein Widerspruch — dann bliebe
    # kein Weg übrig, auf dem sich überhaupt etwas ändern kann. Statt durch
    # Null zu teilen, gilt dann alles jenseits der Totzone als Vollausschlag.
    span = saturation - deadzone_value
    if span <= 0:
        return sign

    share = (amount - deadzone_value) / span
    if share > 1.0:
        share = 1.0

    if curve:
        return sign * _from_curve(share, curve)

    if exponent > 0 and exponent != 1.0:
        share = share ** exponent
    return sign * share


def _from_curve(share, curve):
    """Zwischen den gesetzten Punkten geradlinig ablesen.

    Die Punkte kommen aus `<nonlinearity_curve>` und sind auf 0..1 normiert.
    Zwischen zwei Punkten wird linear interpoliert — dieselbe Vereinfachung,
    die auch das Zeichnen benutzt, und für eine Vorschau genau genug.
    """
    points = sorted((a, b) for a, b in curve
                    if a is not None and b is not None)
    if not points:
        return share
    # Die Enden festnageln, damit außerhalb nicht ins Leere gelesen wird.
    if points[0][0] > 0:
        points.insert(0, (0.0, 0.0))
    if points[-1][0] < 1:
        points.append((1.0, 1.0))

    for nr in range(len(points) - 1):
        left_x, left_y = points[nr]
        right_x, right_y = points[nr + 1]
        if left_x <= share <= right_x:
            width = right_x - left_x
            if width <= 0:
                return right_y
            pos = (share - left_x) / width
            return left_y + pos * (right_y - left_y)
    return points[-1][1]


def progression(deadzone_value=0.0, saturation=1.0, exponent=1.0, curve=None,
            steps=120, whole=False):
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
    steps = max(2, int(steps))
    start = -1.0 if whole else 0.0
    extent = 1.0 - start
    out = []
    for nr in range(steps + 1):
        on_state = start + extent * nr / steps
        out.append((on_state, answer(on_state, deadzone_value, saturation, exponent, curve)))
    return out


def apply(ident, axis, prop, value, filename=None, folder=None):
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

    gone = filename or joysticks._actionmaps_path(folder)
    if not gone:
        return False, 's_js_f_datei', 0
    if prop not in PROPERTIES:
        return False, 's_kv_f_eigenschaft', 0
    if value is not None:
        bottom, top = PROPERTIES[prop]
        try:
            value = float(value)
        except (TypeError, ValueError):
            return False, 's_kv_f_wert', 0
        if not (bottom <= value <= top):
            return False, 's_kv_f_bereich', 0

    ident = (ident or '').upper()
    if not ident:
        return False, 's_kv_f_kennung', 0

    try:
        tree = ET.parse(gone)
    except Exception as ausnahme:
        fehler.merken('curves.setzen_lesen', ausnahme)
        return False, 's_js_f_lesen', 0

    target = None
    for node in tree.getroot().iter('deviceoptions'):
        if _ident_from(node.get('name') or '') == ident:
            # Bei mehreren Blöcken derselben Kennung gewinnt der letzte —
            # also wird auch dort geschrieben, wo das Spiel zuletzt schrieb.
            target = node
    if target is None:
        return False, 's_kv_f_geraet', 0

    keep = None
    removed = 0
    for entry in list(target.findall('option')):
        if (entry.get('input') or '') != axis:
            continue
        if prop not in entry.attrib:
            continue
        if keep is None:
            keep = entry
        else:
            target.remove(entry)
            removed += 1

    if value is None:
        if keep is not None:
            # Nur das eine Attribut löschen — trägt der Eintrag noch etwas
            # anderes (die andere Eigenschaft), bleibt er stehen.
            keep.attrib.pop(prop, None)
            if not [k for k in keep.attrib if k != 'input']:
                target.remove(keep)
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
        text = repr(float(value))
        if keep is None:
            keep = ET.SubElement(target, 'option')
            keep.set('input', axis)
        keep.set(prop, text)

    return joysticks._write(gone, tree, 1 + removed)


def apply_to_game(number, axis, prop, value, filename=None, folder=None):
    """Exponent oder Invertierung einer Spielachse schreiben.

    `nummer` ist die `instance` — das `n` in `js<n>_`. Anders als bei den
    physischen Achsen ist sie hier der Bezugspunkt, weil das Spiel die
    Spielachsen an der Nummer führt und die Kennung nur danebensteht.

    Liefert `(erfolg, meldung, anzahl)`.
    """
    import xml.etree.ElementTree as ET

    from . import fehler

    gone = filename or joysticks._actionmaps_path(folder)
    if not gone:
        return False, 's_js_f_datei', 0
    if prop not in GAME_PROPERTIES:
        return False, 's_kv_f_eigenschaft', 0
    if value is not None:
        bottom, top = GAME_PROPERTIES[prop]
        try:
            value = float(value)
        except (TypeError, ValueError):
            return False, 's_kv_f_wert', 0
        if not (bottom <= value <= top):
            return False, 's_kv_f_bereich', 0

    try:
        tree = ET.parse(gone)
    except Exception as ausnahme:
        fehler.merken('curves.spiel_setzen_lesen', ausnahme)
        return False, 's_js_f_lesen', 0

    target = None
    for node in tree.getroot().iter('options'):
        if (node.get('type') or '').lower() != 'joystick':
            continue
        try:
            if int(node.get('instance') or 0) == int(number):
                target = node
                break
        except ValueError:
            continue
    if target is None:
        return False, 's_kv_f_geraet', 0

    node = target.find(axis)
    if value is None:
        if node is not None:
            node.attrib.pop(prop, None)
            # Ein Element ohne Attribute und ohne Kurve sagt nichts mehr aus.
            if not node.attrib and len(node) == 0:
                target.remove(node)
    else:
        if node is None:
            node = ET.SubElement(target, axis)
        text = ('%d' % int(value)) if prop == 'invert' else ('%g' % value)
        node.set(prop, text)

    return joysticks._write(gone, tree, 1)


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
ACTION_TO_AXIS = (
    ('v_view_', 'flight_view_'),
    ('v_mining_', 'mining_'),
    ('v_', 'flight_move_'),
)


def _axis_name(action):
    """Aus dem Aktionsnamen der Belegung den Namen in `<options>` machen."""
    for front, replacement in ACTION_TO_AXIS:
        if action.startswith(front):
            return replacement + action[len(front):]
    return action


def functions_per_axis(number, axes, filename=None, folder=None):
    """Für mehrere physische Achsen auf einmal: was darauf liegt.

    ⚠ **Eine Dateilesung für alle Achsen, nicht eine je Achse.**
    `spielachsen_auf()` liest die `actionmaps.xml` bei jedem Aufruf neu — für
    eine Tabelle mit acht Zeilen wären das acht Lesungen einer Datei, die
    zwanzigtausend Zeichen hat, und das bei jedem Zeichnen der Seite.

    Gibt `{achse: [funktionen]}` zurück; Achsen ohne Funktion fehlen.
    """
    gone = filename or joysticks._actionmaps_path(folder)
    if not gone:
        return {}
    try:
        with open(gone, 'r', encoding='utf-8', errors='replace') as f:
            text = f.read()
    except Exception:
        return {}
    dropped = {}
    for axis in axes:
        match = game_axes_of(number, axis, filename=gone)
        if match:
            dropped[axis] = match
    return dropped


def game_axes_of(number, axis, filename=None, folder=None):
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
    gone = filename or joysticks._actionmaps_path(folder)
    if not gone:
        return []
    try:
        with open(gone, 'r', encoding='utf-8', errors='replace') as f:
            text = f.read()
    except Exception:
        return []

    wanted = 'js%s_%s' % (number, axis)
    actions = []
    # Jede `<action name="…">` mit ihren `<rebind>`-Kindern durchgehen.
    for match in re.finditer(
            r'<action\s+name="([^"]*)"\s*>(.*?)</action>', text, re.S):
        name, content = match.group(1), match.group(2)
        for binding in re.finditer(r'<rebind\s+input="([^"]*)"', content):
            if binding.group(1).strip() == wanted:
                actions.append(name)
                break

    # Die Einstellungen dieser Nummer dazuholen.
    values = {}
    for block in game_axes(filename, folder):
        if block['art'] == 'joystick' and block['nummer'] == int(number):
            values = block['achsen']
            break

    out = []
    seen = set()
    for action in actions:
        name = _axis_name(action)
        if name in seen:
            continue
        seen.add(name)
        props = values.get(name) or {}
        out.append({'achse': name, 'aktion': action,
                       'exponent': props.get('exponent'),
                       'invert': props.get('invert')})
    return out


def clean_up(filename=None, folder=None, count_only=False):
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

    gone = filename or joysticks._actionmaps_path(folder)
    if not gone or not os.path.isfile(gone):
        return False, 's_js_f_datei', 0
    try:
        with open(gone, 'r', encoding='utf-8', errors='replace') as f:
            content = f.read()
    except Exception as ausnahme:
        fehler.merken('curves.aufraeumen_lesen', ausnahme)
        return False, 's_js_f_lesen', 0

    alive = valid_idents(folder, filename)
    cuts = []
    for match in BLOCK_RE.finditer(content):
        head = re.match(r'<deviceoptions[^>]*>', match.group(0))
        name = re.search(r'name="([^"]*)"', head.group(0) if head else '')
        ident = _ident_from(name.group(1) if name else '')
        # ⚠ Ein Block OHNE Kennung (Maus, Tastatur) ist nicht tot, sondern
        # nur nicht zuordenbar — der bleibt.
        if ident and ident not in alive:
            cuts.append((match.start(), match.end()))

    if not cuts:
        return False, 's_gs_f_nichts_zu_tun', 0
    if count_only:
        return True, '', len(cuts)

    # Von hinten nach vorn schneiden, sonst verschieben sich die Stellen.
    fresh = content
    for start, end in reversed(cuts):
        # Die Leerzeile mitnehmen, die der Block hinterlässt.
        to_value = end
        while to_value < len(fresh) and fresh[to_value] in ' \t':
            to_value += 1
        if to_value < len(fresh) and fresh[to_value] == '\n':
            to_value += 1
        before = start
        while before > 0 and fresh[before - 1] in ' \t':
            before -= 1
        fresh = fresh[:before] + fresh[to_value:]

    backup = '%s.scbpw-%s' % (gone, time.strftime('%Y%m%d-%H%M%S'))
    try:
        shutil.copy2(gone, backup)
    except Exception as ausnahme:
        fehler.merken('curves.aufraeumen_sicherung', ausnahme)
        return False, 's_js_f_sicherung', 0
    try:
        with open(gone, 'w', encoding='utf-8', newline='') as f:
            f.write(fresh)
    except Exception as ausnahme:
        try:
            shutil.copy2(backup, gone)
        except Exception:
            pass
        fehler.merken('curves.aufraeumen_schreiben', ausnahme)
        return False, 's_js_f_schreiben', 0
    return True, backup, len(cuts)


def align(from_ident, by_ident, filename=None, folder=None):
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
    from_ident = (from_ident or '').upper()
    by_ident = (by_ident or '').upper()
    if not from_ident or not by_ident:
        return False, 's_kv_f_kennung', 0
    if from_ident == by_ident:
        return False, 's_kv_f_geraet', 0

    blocks = device_axes(filename, folder)
    source = target = None
    for block in blocks:
        # Bei mehreren Blöcken derselben Kennung gewinnt der letzte — dieselbe
        # Regel wie überall in diesem Modul.
        if block['kennung'] == from_ident:
            source = block
        if block['kennung'] == by_ident:
            target = block
    if source is None or target is None:
        return False, 's_kv_f_geraet', 0

    shared = [a for a in AXES
                 if a in source['achsen'] and a in target['achsen']]
    if not shared:
        return False, 's_ac_nichts_gemeinsam', 0

    # ⚠ Nur schreiben, was sich unterscheidet. `setzen()` legt bei jedem
    # Aufruf eine Sicherung an — zwölf blinde Schreibvorgänge hinterließen
    # zwölf Sicherungsdateien für meist zwei echte Änderungen.
    count = 0
    for axis in shared:
        for prop in PROPERTIES:
            value = source['achsen'][axis].get(prop)
            ist = target['achsen'][axis].get(prop)
            if ist == value:
                continue
            ok_state, message, _ = apply(by_ident, axis, prop,
                                        value, filename, folder)
            if not ok_state:
                return False, message, count
            count += 1
    return True, '', count


def summary(filename=None, folder=None):
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
    blocks = device_axes(filename, folder)
    conflicts = []
    for block in blocks:
        for axis in block['mehrfach']:
            conflicts.append((block['name'], axis))
    return {
        'bloecke': sorted(blocks, key=lambda b: (not b['aktiv'], b['name'])),
        'leichen': orphans(blocks=blocks),
        'uebernehmbar': adoptable(blocks=blocks),
        'spiel': game_axes(filename, folder),
        'widersprueche': conflicts,
    }
