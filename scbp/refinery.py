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
Welche Verarbeitungsmethode die Raffinerie nehmen soll.

Das Terminal bietet neun Methoden an und zeigt zu jeder **eine Zeile** mit drei
Einstufungen — etwa `GERINGE ERTRÄGE // HOHE GESCHWINDIGKEIT // MODERATE
KOSTEN`. Vergleichen kann man nur durch Durchklicken, und die Ausbeute steht
nirgends als Zahl. Dieses Modul beantwortet die Frage vorab: Man sagt, was einem
wichtig ist, und bekommt die Methode.

**Woher die Werte stammen: aus dem Spiel, abgelesen am 03.09.2026 (Alpha 4.10).**

⚠⚠ **Nicht durch Werte aus der Netz-Gemeinde ersetzen.** Der grosse
Community-Rechner (Regolith) führt echte Faktoren statt Stufen, was verlockend
ist — beim Abgleich gegen das Spiel stimmten davon aber nur **zwei von neun**
Methoden. Regolith wird nicht mehr gepflegt und hat ein Balancing verpasst: Nach
seinen Zahlen wäre `Dinyx Solventation` die beste Methode, tatsächlich ist sie
die schlechteste. Auch Websuche taugt nicht, sie mischt abgelöste Namen unter
(`Dynasty Solventation`, `Electro-Arc Pyrometallurgy`). **Einzige gültige Quelle
ist der Bildschirm im Spiel.**

**Nach einem grösseren Patch neu ablesen:** jede Methode einmal anklicken, die
Zeile darunter abfotografieren. Neun Bilder, fünf Minuten. Dann `LEVELS` unten
nachziehen und `PATCH` hochsetzen — alles andere rechnet sich daraus.

**Das Raster** — die neun Methoden sind kein Wildwuchs, sondern vollständig:
drei Ertragsstufen mal drei Kostenstufen, jede Kombination genau einmal. Daraus
folgt, dass mehr Geld immer Tempo kauft und nie Material, und dass höherer
Ertrag immer Zeit kostet. Deshalb braucht die Empfehlung **keine Gewichtung**:
Erste und zweite Priorität genügen, es bleibt genau eine Methode übrig.

⚠ Bis zum 11.09.2026 hieß dieses Modul `raffinerie` (Sprachumstellung P4,
Stufe 2). **Nur Bezeichner sind umbenannt, keine Zeichenketten.** Bewusst
gleich geblieben: die Kennungen der Methoden (`'pyrometric'`, `'gaskin'`, …)
und die Achsen `'ertrag'`, `'kosten'`, `'tempo'` — über sie holt die
Oberfläche ihre Texte (`s_rm_…`).
"""

# Der Spielstand, aus dem die Stufen abgelesen wurden. Steht in der Oberfläche,
# damit erkennbar bleibt, wie alt die Angaben sind.
PATCH = '4.10'
READ_ON = '2026-09-03'

# Die drei Achsen. Grössere Zahl heisst **besser für den Spieler**:
# viel Ertrag, wenig Kosten, hohes Tempo. Damit ist jeder Vergleich im Modul
# ein simples „grösser ist besser", ohne Sonderfall für die Kosten.
LOW, MODERATE, HIGH = 1, 2, 3
VERY_SLOW, SLOW, MEDIUM, FAST = 0, 1, 2, 3

# Methode -> (Ertrag, Tempo, Kostenvorteil)
#
# ⚠ Die dritte Zahl ist der **Kostenvorteil**, nicht der Preis: 3 heisst
# „geringe Kosten". Im Spiel steht dort `GERINGE KOSTEN` — beim Nachtragen also
# umdrehen, sonst empfiehlt das Werkzeug künftig die teuerste Methode.
LEVELS = {
    'pyrometric':  (HIGH,     VERY_SLOW, HIGH),
    'gaskin':      (HIGH,     SLOW,      MODERATE),
    'dinyx':       (HIGH,     SLOW,      LOW),
    'ferron':      (MODERATE, SLOW,      HIGH),
    'kazen':       (MODERATE, MEDIUM,    MODERATE),
    'thermonatic': (MODERATE, FAST,      LOW),
    'electro':     (LOW,      MEDIUM,    HIGH),
    'cormack':     (LOW,      FAST,      MODERATE),
    'xcr':         (LOW,      FAST,      LOW),
}

# Wie die Methode im Spiel heisst. Englisch in beiden Sprachfassungen: Das
# Terminal zeigt sie auch im deutschen Client englisch, und wer sie dort suchen
# will, braucht genau diese Schreibweise.
NAMES = {
    'pyrometric':  'Pyrometric Chromalysis',
    'gaskin':      'Gaskin Process',
    'dinyx':       'Dinyx Solventation',
    'ferron':      'Ferron Exchange',
    'kazen':       'Kazen Winnowing',
    'thermonatic': 'Thermonatic Deposition',
    'electro':     'Electrostarolysis',
    'cormack':     'Cormack Method',
    'xcr':         'XCR Reaction',
}

# Die drei Achsen als Auswahl. Reihenfolge = Reihenfolge in der Oberfläche.
AXES = ('ertrag', 'kosten', 'tempo')
_POSITION = {'ertrag': 0, 'tempo': 1, 'kosten': 2}


def level(key, axis):
    """Die Stufe einer Methode auf einer Achse — grösser ist besser."""
    values = LEVELS.get(key)
    if not values or axis not in _POSITION:
        return 0
    return values[_POSITION[axis]]


def recommend(first=None, second=None):
    """Die passende Methode — `(kennung, alle_kennungen_sortiert)`.

    `first` und `second` sind Achsen aus `AXES`. Ohne Angabe gilt die
    Standard-Rangfolge Ertrag → Kosten → Tempo: Sie führt auf `pyrometric`, die
    einzige Methode mit höchstem Ertrag **und** geringsten Kosten.

    ⚠ Die zweite Achse darf nicht die erste sein — sonst entschiede sie nichts.
    Sie wird in dem Fall stillschweigend übergangen, statt eine Fehlermeldung zu
    erzeugen: Die Oberfläche lässt die Wahl zu, und ein Hinweis „das geht nicht"
    wäre für den Spieler ohne Erkenntnisgewinn.
    """
    rank = [a for a in (first, second) if a in _POSITION]
    # Doppelte raus, danach den Rest in fester Reihenfolge anhängen — so ist
    # das Ergebnis **immer eindeutig**, auch wenn nichts gewählt wurde.
    seen, order = set(), []
    for axis in rank + ['ertrag', 'kosten', 'tempo']:
        if axis not in seen:
            seen.add(axis)
            order.append(axis)

    def sort_key(key):
        return tuple(-level(key, axis) for axis in order)

    all_keys = sorted(LEVELS, key=sort_key)
    return all_keys[0], all_keys


def dominated():
    """Methoden, die von einer anderen in **jeder** Hinsicht geschlagen werden.

    Gibt `{schlechte: bessere}`. Solche Methoden gibt es wirklich — sie zu
    nennen ist der handfesteste Rat, den das Werkzeug geben kann: Wer sie
    wählt, verliert etwas und gewinnt nichts.

    ⚠ **Wird gerechnet, nicht eingetragen.** Ändert CIG eine Stufe, stimmt die
    Aussage weiter. Eine fest hinterlegte Liste wäre beim nächsten Patch eine
    Falschaussage, die niemandem auffällt.
    """
    result = {}
    for a in LEVELS:
        for b in LEVELS:
            if a == b:
                continue
            better_somewhere = False
            worse_somewhere = False
            for axis in _POSITION:
                if level(b, axis) > level(a, axis):
                    better_somewhere = True
                elif level(b, axis) < level(a, axis):
                    worse_somewhere = True
            if better_somewhere and not worse_somewhere:
                result[a] = b
                break
    return result


def grid():
    """Die Tabelle Ertrag × Kosten — `{(ertrag, kosten): kennung}`.

    Dass jede der neun Zellen genau einmal belegt ist, ist der Kern der Sache
    und wird im Selbsttest geprüft: Fällt das Raster auseinander, wurde beim
    Nachtragen etwas verwechselt.
    """
    return {(level(k, 'ertrag'), level(k, 'kosten')): k for k in LEVELS}
