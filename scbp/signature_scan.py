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
#
# ---------------------------------------------------------------------------
# Die Vorgehensweise (Signatur auf mögliche Werte einrasten, Abstimmung über
# mehrere Bilder, vom Spieler aufgezogener Scan-Bereich) ist angelehnt an
# eine Arbeit von ryze, veröffentlicht unter der MIT-Lizenz:
#
# MIT License
#
# Copyright (c) 2026 ryze
#
# Permission is hereby granted, free of charge, to any person obtaining a copy
# of this software and associated documentation files (the "Software"), to deal
# in the Software without restriction, including without limitation the rights
# to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
# copies of the Software, and to permit persons to whom the Software is
# furnished to do so, subject to the following conditions:
#
# The above copyright notice and this permission notice shall be included in
# all copies or substantial portions of the Software.
#
# THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
# IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
# FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
# AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
# LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
# OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
# SOFTWARE.
# ---------------------------------------------------------------------------
"""
Die Scan-Signatur aus einem Bildausschnitt lesen.

**Wozu.** Der Bergbau-Scanner im Spiel zeigt eine Zahl; welcher Brocken
dahintersteckt, sagt `mining.find_signature`. Bisher musste die Zahl
abgetippt werden.

**Wie.** Der Spieler legt mit dem Scan-Fenster fest, **wo** die Zahl steht
(`scan_window.py`) — die Lage hängt am Schiff, eine feste Stelle gibt es
nicht. In diesem Ausschnitt sind die Ziffern die hellen Flächen; jede wird auf
ein festes Raster gebracht und gegen angelernte Vorlagen verglichen. Kein
Tesseract, kein Fremdpaket.

⭐⭐ **Gelesen wird ein MÖGLICHER Wert, nicht Ziffer für Ziffer.** Die Anzeige
ist immer Grundsignatur mal Brockenzahl. Aus dieser Menge wird der Wert
gewählt, dessen Ziffern insgesamt am besten passen — „1?,800" hat nur eine
gültige Lesart, auch wenn die Null für sich einer Acht ähnelt. Danke an ryze.

Der Erkennungskern stammt aus dem Entwurf vom 09./10.09.2026 (Zweig
`mining-scanner`), dort gegen 82 Bilder mit bekannter Antwort vermessen. Was
wegfällt, ist die Suche nach der Pille im ganzen Bild — die war der
fehleranfälligste Teil, und der Spieler zeigt die Stelle jetzt selbst.

Rückgaben tragen **Kennwörter**, keine Sätze — den Text setzt die Oberfläche
aus `language.py`.
"""
import json
import os
import struct
import time
import zlib

# Die Vorlagen werden vor dem Vergleich auf diese feste Größe gebracht. Damit
# spielt es keine Rolle, ob die Zahl auf 1080p oder auf einem Ultrawide steht.
NORM_W, NORM_H = 16, 24

SAMPLE_FOLDER = 'signatur-bilder'
SAMPLE_LIMIT = 200
TEMPLATE_FILE = 'signatur-ziffern.json'          # mitgeliefert, `daten/`
OWN_TEMPLATE_FILE = 'signatur-ziffern-eigene.json'  # selbst angelernt
REGION_SETTING = 'signatur_bereich'

# Aufschlag auf den Abstand, wenn die Löcher nicht zusammenpassen. Kein harter
# Ausschluss: Bei einem verrauschten Zeichen kann ein Loch zulaufen.
HOLE_PENALTY = 0.25

# Schlechtester mittlerer Abstand, der noch als gelesen gilt, und der Vorsprung
# vor dem zweitbesten Wert. Gemessen am 10.09.2026 gegen 82 Bilder:
# | Vorsprung | richtig | falsch | schweigt |
# |---|---|---|---|
# | 0,020 | 62 | 1 | 19 |
# | 0,050 | 45 | 2 | 35 |
MAX_DISTANCE = 0.34
VALUE_MARGIN = 0.02
# Höchster Abstand EINER Ziffer. Abgestimmt am 17.09.2026 (82 Aufnahmen /
# gezeichnete Ziffern, richtig/falsch): 0,30 → 74/2 · 42/2; **0,26 → 72/1 ·
# 33/0**; 0,22 → 70/1 · 15/1. Ein höherer Vorsprung half nirgends.
MAX_DIGIT_DISTANCE = 0.26
# So viele selbst angelernte Bilder je Ziffer — die jüngsten bleiben.
MAX_OWN_PER_DIGIT = 24

# Welche Lochstruktur eine Ziffer haben MUSS — (Anzahl, Lage von oben).
# ⚠ Am 10.09.2026 hatten fünf von acht angelernten „Sechsen" zwei Löcher: Achten
# in der falschen Zeile. Dieser Prüfstein hält solche Vorlagen fern.
# Mindestens so viele Zeichen — weniger wären zu leicht mit Rauschen zu
# verwechseln, und eine Signatur unter 100 gibt es nicht.
MIN_CHARS = 3

REQUIRED_HOLES = {
    '0': (1, 0.5), '1': (0, None), '2': (0, None), '3': (0, None),
    '4': (1, None), '5': (0, None), '6': (1, 0.65), '7': (0, None),
    '8': (2, None), '9': (1, 0.32),
}


# --------------------------------------------------------------------------
# Bild zerlegen
# --------------------------------------------------------------------------

def otsu(raster):
    """Die Trennlinie zwischen Text und Grund selbst bestimmen (Otsu)."""
    counts = [0] * 256
    for row in raster:
        for value in row:
            counts[value] += 1
    total = sum(counts)
    if not total:
        return 128
    weighted = sum(i * counts[i] for i in range(256))
    sum_a, weight_a, best, threshold = 0.0, 0, -1.0, 128
    for i in range(256):
        weight_a += counts[i]
        if not weight_a:
            continue
        weight_b = total - weight_a
        if not weight_b:
            break
        sum_a += i * counts[i]
        mean_a = sum_a / weight_a
        mean_b = (weighted - sum_a) / weight_b
        quality = weight_a * weight_b * (mean_a - mean_b) ** 2
        if quality > best:
            best, threshold = quality, i
    return threshold


def thresholds(raster):
    """Mehrere Trennlinien — Otsu und „nur das Hellste".

    ⚠⚠ Otsu allein reicht nicht: Vor einem hellen Asteroiden rutscht die
    Schwelle ab, und die Ziffern verschmelzen mit dem Geröll (09.09.2026
    gemessen). Die Ziffern belegen nur wenige Punkte des Ausschnitts — hohe
    Perzentile treffen genau sie.
    """
    counts = [0] * 256
    for row in raster:
        for value in row:
            counts[value] += 1
    total = sum(counts)
    base = otsu(raster)
    found = [base]
    top = base
    if total:
        for share in (0.97, 0.99, 0.995):
            limit, running = total * share, 0
            for value in range(256):
                running += counts[value]
                if running >= limit:
                    found.append(value)
                    top = max(top, value)
                    break
        # ⚠⚠ **Stufen zwischen Otsu und dem Hellsten** (17.09.2026). Bei kleiner
        # HUD-Schrift (Ziffern 5×11, Striche 1–2 Punkte) lagen alle Perzentile
        # auf dem hellen Ortungssymbol (≈240), Otsu auf dem Pillengrund (≈128)
        # — sauber getrennt standen die Ziffern erst bei 160–190, und genau
        # dort gab es keine Schwelle.
        for share in (0.25, 0.45, 0.65):
            found.append(int(base + (top - base) * share))
    return sorted({v for v in found if 8 <= v <= 245})


def components(raster, threshold):
    """Zusammenhängende helle Flächen als (links, oben, rechts, unten)."""
    height = len(raster)
    width = len(raster[0]) if height else 0
    seen = [bytearray(width) for _ in range(height)]
    boxes = []
    for y in range(height):
        row = raster[y]
        for x in range(width):
            if seen[y][x] or row[x] <= threshold:
                continue
            stack = [(x, y)]
            seen[y][x] = 1
            left = right = x
            top = bottom = y
            while stack:
                px, py = stack.pop()
                left, right = min(left, px), max(right, px)
                top, bottom = min(top, py), max(bottom, py)
                for nx in (px - 1, px, px + 1):
                    if nx < 0 or nx >= width:
                        continue
                    for ny in (py - 1, py, py + 1):
                        if 0 <= ny < height and not seen[ny][nx] \
                                and raster[ny][nx] > threshold:
                            seen[ny][nx] = 1
                            stack.append((nx, ny))
            # Einzelne helle Punkte sind Bildrauschen, keine Ziffer.
            if (right - left) >= 1 and (bottom - top) >= 3:
                boxes.append((left, top, right, bottom))
    boxes.sort()
    return boxes


def _median(values):
    ordered = sorted(values)
    return ordered[len(ordered) // 2] if ordered else 0


def split_merged(boxes):
    """Zusammengeflossene Ziffern wieder auftrennen — Ziffern sind gleich breit.

    ⚠ Das Komma bleibt unangetastet: Es ist schmaler als eine Ziffer, nie breiter.
    """
    if len(boxes) < 2:
        return list(boxes)
    single = _median([b[2] - b[0] + 1 for b in boxes])
    if single < 2:
        return list(boxes)
    result = []
    for box in boxes:
        wide = box[2] - box[0] + 1
        parts = int(round(wide / float(single)))
        if parts >= 2 and wide >= single * 1.65:
            step = wide // parts
            for i in range(parts):
                left = box[0] + i * step
                right = box[2] if i == parts - 1 else box[0] + (i + 1) * step - 1
                result.append((left, box[1], right, box[3]))
        else:
            result.append(box)
    return result


def _separator_fits(chars):
    """Steht an drittletzter Stelle ein Trennzeichen (ab vier Zeichen)?

    ⚠⚠ Star Citizen setzt ab Tausend immer eines, drei Stellen von hinten —
    schmal und flach, wo die Ziffern gleich hoch sind. Zufälliges Geröll hat
    dieses Muster nicht.
    """
    if len(chars) < 4:
        return True
    digit_h = _median([c[3] - c[1] + 1 for c in chars])
    digit_w = _median([c[2] - c[0] + 1 for c in chars])
    sep = chars[len(chars) - 4]
    if ((sep[3] - sep[1] + 1) <= digit_h * 0.7
            and (sep[2] - sep[0] + 1) <= max(2, digit_w * 0.7)):
        return True
    # ⚠ Das Komma ist blass und fällt bei höheren Schwellen weg. Dann bleibt
    # seine **Lücke**: vor den letzten drei Ziffern deutlich breiter als
    # zwischen den übrigen. Gemessen an „19,275": Lücke 4 gegen 0–1.
    gaps = [chars[i + 1][0] - chars[i][2] - 1 for i in range(len(chars) - 1)]
    before_last_three = gaps[-3]
    others = gaps[:-3] + gaps[-2:]
    return before_last_three >= max(2, 2 * max(others or [0]) + 1)


def only_digits(chars):
    """Das Trennzeichen heraus — erkannt an der Größe, nicht an der Stelle."""
    if len(chars) < 2:
        return list(chars)
    middle = _median([c[3] - c[1] + 1 for c in chars])
    return [c for c in chars if (c[3] - c[1] + 1) > middle * 0.7]


def digit_rows(boxes, width=None):
    """Die Zeichenreihe der Signatur finden — als Kandidatenliste.

    Übernommen aus dem Entwurf vom 09./10.09.2026 (`zeichenreihe_finden`), dort
    gegen 82 Aufnahmen vermessen; auf aufgezogenen Ausschnitten derselben
    Aufnahmen am 17.09.2026: 66 richtig, 1 falsch. ⚠ Eine eigene, vereinfachte
    Suche schaffte auf denselben Bildern nur 25 richtig bei 7 falschen — nicht
    wieder „vereinfachen", ohne gegen die Aufnahmen zu messen.

    Gesucht: ähnlich hohe Flächen auf einer Grundlinie, **angeführt vom
    Ortungssymbol** (das von links abgeschnitten wird), dicht beieinander, mit
    dem Trennzeichen an der richtigen Stelle. Gibt die Ziffernflächen (ohne
    Trennzeichen) der Reihe, deren Mitte der Ausschnittmitte am nächsten ist.
    """
    if not boxes:
        return []
    rows = []
    for box in sorted(boxes, key=lambda b: b[3]):
        height = box[3] - box[1] + 1
        for row in rows:
            base = sum(b[3] for b in row) / float(len(row))
            mean_h = sum(b[3] - b[1] + 1 for b in row) / float(len(row))
            # Die untere Grenze muss das Komma durchlassen (3x5 gegen 9x11).
            if abs(box[3] - base) <= max(2, mean_h * 0.45) \
                    and 0.3 <= height / mean_h <= 2.2:
                row.append(box)
                break
        else:
            rows.append([box])

    best, best_distance = [], None
    for row in rows:
        if len(row) < MIN_CHARS + 1:            # Ortungssymbol zählt mit
            continue
        ordered = sorted(row, key=lambda b: b[0])
        middle = _median([b[3] - b[1] + 1 for b in ordered])
        ordered = [b for b in ordered
                   if 0.3 <= (b[3] - b[1] + 1) / float(middle) <= 1.9]
        if len(ordered) < MIN_CHARS + 1:
            continue
        # Zeichen einer Zahl stehen dicht — getrennt an Lücken > 2 Zeichenhöhen.
        bundles = [[ordered[0]]]
        for box in ordered[1:]:
            if box[0] - bundles[-1][-1][2] > max(6, middle * 2.0):
                bundles.append([box])
            else:
                bundles[-1].append(box)
        ordered = max(bundles, key=len)
        if len(ordered) < MIN_CHARS + 1:
            continue
        # Ziffern sind gleich hoch: von links abschneiden, was diese Höhe nicht
        # hat — das sind die Teile des Ortungssymbols. Nichts abzuschneiden
        # heißt: kein Symbol, keine Signatur.
        digit_h = _median([b[3] - b[1] + 1 for b in ordered])
        start = 0
        while start < len(ordered):
            if 0.85 <= (ordered[start][3] - ordered[start][1] + 1) / float(digit_h) <= 1.15:
                break
            start += 1
        if start == 0 or start >= len(ordered):
            continue
        rest = split_merged(ordered[start:])
        if len(rest) < MIN_CHARS or not _separator_fits(rest):
            continue
        if width:
            distance = abs((rest[0][0] + rest[-1][2]) / 2.0 - width / 2.0)
            if best_distance is None or distance < best_distance:
                best, best_distance = rest, distance
        elif len(rest) > len(best):
            best = rest
    return [only_digits(best)] if best else []


def normalize(raster, box, threshold):
    """Eine Fläche auf das feste Raster bringen, Seitenverhältnis bleibt."""
    left, top, right, bottom = box
    src_w, src_h = right - left + 1, bottom - top + 1
    factor = min(NORM_W / float(src_w), NORM_H / float(src_h))
    dst_w, dst_h = max(1, int(src_w * factor)), max(1, int(src_h * factor))
    pad_x, pad_y = (NORM_W - dst_w) // 2, (NORM_H - dst_h) // 2
    pattern = [0] * (NORM_W * NORM_H)
    for y in range(dst_h):
        sy = top + min(src_h - 1, int(y / factor))
        for x in range(dst_w):
            sx = left + min(src_w - 1, int(x / factor))
            if raster[sy][sx] > threshold:
                pattern[(pad_y + y) * NORM_W + (pad_x + x)] = 1
    return pattern


def holes(pattern):
    """Geschlossene Flächen eines Zeichens — (Anzahl, höchste Lage 0..1).

    ⚠⚠ Das Merkmal, das Null von Acht trennt: Am 10.09.2026 gingen 60 von 63
    Fehlschlägen auf genau diese Verwechslung zurück.
    """
    width, height = NORM_W, NORM_H
    seen = [[False] * width for _ in range(height)]
    stack = [(x, 0) for x in range(width)] + [(x, height - 1) for x in range(width)] \
        + [(0, y) for y in range(height)] + [(width - 1, y) for y in range(height)]
    while stack:
        x, y = stack.pop()
        if not (0 <= x < width and 0 <= y < height):
            continue
        if seen[y][x] or pattern[y * width + x]:
            continue
        seen[y][x] = True
        stack.extend(((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)))
    found = []
    for y0 in range(height):
        for x0 in range(width):
            if pattern[y0 * width + x0] or seen[y0][x0]:
                continue
            ys, inner = [], [(x0, y0)]
            seen[y0][x0] = True
            while inner:
                x, y = inner.pop()
                ys.append(y)
                for nx, ny in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)):
                    if 0 <= nx < width and 0 <= ny < height \
                            and not seen[ny][nx] and not pattern[ny * width + nx]:
                        seen[ny][nx] = True
                        inner.append((nx, ny))
            if len(ys) >= 3:
                found.append(sum(ys) / float(len(ys)) / height)
    if not found:
        return (0, None)
    return (len(found), min(found))


def _holes_match(a, b):
    if a[0] != b[0]:
        return False
    if a[1] is None or b[1] is None:
        return True
    return abs(a[1] - b[1]) <= 0.2


def plausible_template(digit, pattern):
    """Passt die Lochstruktur zu der Ziffer, als die sie gelernt werden soll?"""
    required = REQUIRED_HOLES.get(digit)
    if required is None:
        return False
    actual = holes(pattern)
    if actual[0] != required[0]:
        return False
    if required[1] is None or actual[1] is None:
        return True
    return abs(actual[1] - required[1]) <= 0.22


# --------------------------------------------------------------------------
# Vorlagen
# --------------------------------------------------------------------------

def _read_templates(path):
    try:
        with open(path, encoding='utf-8') as f:
            data = json.load(f)
        if data.get('format') == 1 and isinstance(data.get('ziffern'), dict):
            return data['ziffern']
    except Exception:
        pass
    return {}


def templates():
    """Mitgelieferte und selbst angelernte Vorlagen — je Ziffer (Muster, Löcher)."""
    from . import paths
    merged = {}
    for path in (paths.bundled_file(TEMPLATE_FILE),
                 paths.app_file(OWN_TEMPLATE_FILE)):
        for digit, examples in _read_templates(path).items():
            if digit not in REQUIRED_HOLES:
                continue
            bucket = merged.setdefault(digit, [])
            for pattern in examples:
                if isinstance(pattern, list) and len(pattern) == NORM_W * NORM_H:
                    filled = fill(pattern)
                    if filled not in [p for p, _h in bucket]:
                        bucket.append((filled, holes(filled)))
    return merged


def fill(pattern):
    """Ein Muster auf seine Umrisse zuschneiden und aufs ganze Raster strecken."""
    xs = [i % NORM_W for i in range(NORM_W * NORM_H) if pattern[i]]
    if not xs:
        return list(pattern)
    ys = [i // NORM_W for i in range(NORM_W * NORM_H) if pattern[i]]
    left, top = min(xs), min(ys)
    width, height = max(xs) - left + 1, max(ys) - top + 1
    result = [0] * (NORM_W * NORM_H)
    for y in range(NORM_H):
        source = (top + y * height // NORM_H) * NORM_W + left
        for x in range(NORM_W):
            result[y * NORM_W + x] = pattern[source + x * width // NORM_W]
    return result


def learned_digits():
    """Wie viele der zehn Ziffern Vorlagen haben."""
    known = templates()
    return sum(1 for d in '0123456789' if known.get(d))


# --------------------------------------------------------------------------
# Lesen
# --------------------------------------------------------------------------

def possible_values():
    """Alle Zahlen, die der Scanner überhaupt zeigen kann."""
    from . import mining
    values = set()
    data = mining.load()
    for _g, element in (data.get('elemente') or {}).items():
        sig = element.get('scanSignature')
        if not sig:
            continue
        for count in range(1, mining.MAX_CHUNKS.get(element.get('rarity'), 6) + 1):
            values.add(int(sig * count))
    for _name, sig, most in mining.BASE_SIGNATURES:
        for count in range(1, most + 1):
            values.add(int(sig * count))
    return sorted(values)


def _score_value(table, text):
    total, worst = 0.0, 0.0
    for column, digit in zip(table, text):
        distance = column.get(digit)
        if distance is None:
            return None
        total += distance
        worst = max(worst, distance)
    # ⚠ Der Mittelwert allein versteckt eine einzelne falsche Ziffer: Aus
    # „3,400" wurde „3,000", weil drei gute Ziffern die Vier überstimmten.
    if worst > MAX_DIGIT_DISTANCE:
        return None
    return total / len(text)


def digit_table(patterns, known):
    """Je Zeichen den Abstand zu jeder Ziffer — {ziffer: abstand} je Stelle."""
    table = []
    size = float(NORM_W * NORM_H)
    for pattern in patterns:
        # ⚠⚠ Verglichen wird **gestreckt** (`fill`): beide Seiten füllen das
        # Raster. Unter Windows waren die Ziffern 5×11 statt 9×11 wie in den
        # Linux-Aufnahmen, aus denen die Vorlagen stammen — mit erhaltenem
        # Seitenverhältnis passte keine Vorlage (17.09.2026). An den 82
        # Aufnahmen kostet das nichts (65 → 66 richtig, weiter 1 falsch).
        filled = fill(pattern)
        own = holes(filled)
        column = {}
        for digit, examples in known.items():
            if not examples:
                continue
            column[digit] = min(
                sum(1 for i in range(len(filled)) if filled[i] != example[i]) / size
                + (0.0 if _holes_match(own, example_holes) else HOLE_PENALTY)
                for example, example_holes in examples)
        table.append(column)
    return table


def match_values(patterns, known, values):
    """Den möglichen Wert wählen, der am besten passt.

    Gibt (wert, mittlerer_abstand) oder (None, abstand).
    """
    if not patterns or not known or not values:
        return None, 1.0
    table = digit_table(patterns, known)
    scored = []
    for value in values:
        text = str(value)
        if len(text) != len(table):
            continue
        score = _score_value(table, text)
        if score is not None:
            scored.append((score, value))
    if not scored:
        return None, 1.0
    scored.sort()
    best, value = scored[0]
    if best > MAX_DISTANCE:
        return None, best
    if len(scored) > 1 and scored[1][0] - best < VALUE_MARGIN:
        return None, best
    return value, best


def read(raster, known=None, values=None):
    """Die Signatur in einem Ausschnitt lesen.

    Gibt ein dict:

    | Feld | Inhalt |
    |---|---|
    | `wert` | die gelesene Signatur oder None |
    | `grund` | Kennwort, wenn nichts gelesen: `kein_text`, `nicht_angelernt`, `unsicher`, `keine_werte` |
    | `abstand` | wie gut es passte (0 = genau) |
    | `ziffern` | die Ziffernflächen der besten Reihe — fürs Anlernen |
    | `schwelle` | die Trennlinie dazu |
    """
    result = {'wert': None, 'grund': 'kein_text', 'abstand': 1.0,
              'ziffern': [], 'schwelle': None}
    if not raster or not raster[0]:
        return result
    known = templates() if known is None else known
    values = possible_values() if values is None else values

    best = None
    fallback = None
    for threshold in thresholds(raster):
        for digits in digit_rows(components(raster, threshold), len(raster[0])):
            if fallback is None or len(digits) > len(fallback[1]):
                fallback = (threshold, digits)
            if not known:
                continue
            patterns = [normalize(raster, box, threshold) for box in digits]
            value, distance = match_values(patterns, known, values)
            if value is not None and (best is None or distance < best[0]):
                best = (distance, value, threshold, digits)
            elif best is None and (result['abstand'] > distance):
                result['abstand'] = distance
    if best is not None:
        result.update(wert=best[1], grund=None, abstand=best[0],
                      schwelle=best[2], ziffern=best[3])
        return result
    if fallback is not None:
        result['schwelle'], result['ziffern'] = fallback
        result['grund'] = ('nicht_angelernt' if not known
                           else 'keine_werte' if not values else 'unsicher')
    return result


def learn(raster, typed):
    """Den Ausschnitt als die getippte Zahl anlernen.

    Gibt (erfolg, kennwort, bilanz). `bilanz` sagt dem Spieler, was geschah:
    `erkannt` (Ziffern im Bild), `neu` (neu gespeichert), `bekannt` (dieses
    Ziffernbild gab es schon), `unklar` (Stellen, 1-basiert, die nicht wie ihre
    Ziffer aussahen und deshalb NICHT gespeichert wurden).

    ⚠ Bis 17.09.2026 stand dort nur die Zahl der neuen Bilder: „Gelernt (4
    Ziffern)" bei einer fünfstelligen Zahl — und niemand wusste, ob das Richtige
    angekommen war.

    ⚠ Passt die Zahl der gefundenen Ziffern nicht zur getippten Zahl, wird
    NICHT geraten — eine falsch zugeordnete Vorlage vergiftet jede spätere
    Erkennung, und man sieht es ihr nicht an.
    """
    from . import paths
    digits_typed = [c for c in str(typed) if c.isdigit()]
    if len(digits_typed) < 2:
        return False, 'anlernen_leer', {}
    candidates = []
    for threshold in thresholds(raster):
        for digits in digit_rows(components(raster, threshold), len(raster[0])):
            if len(digits) == len(digits_typed):
                patterns = [normalize(raster, b, threshold) for b in digits]
                fitting = sum(1 for p, d in zip(patterns, digits_typed)
                              if plausible_template(d, p))
                candidates.append((fitting, patterns))
    if not candidates:
        return False, 'anlernen_anzahl', {}
    candidates.sort(key=lambda c: -c[0])
    patterns = candidates[0][1]

    path = paths.app_file(OWN_TEMPLATE_FILE)
    own = _read_templates(path)
    columns = digit_table(patterns, templates())
    stats = {'erkannt': len(patterns), 'neu': 0, 'bekannt': 0, 'unklar': []}
    for position, (pattern, digit) in enumerate(zip(patterns, digits_typed), 1):
        if not plausible_template(digit, pattern):
            stats['unklar'].append(position)
            continue
        # ⚠⚠ **„Bekannt" heißt: VerseKit hätte diese Ziffer schon VOR dem
        # Anlernen richtig gelesen** — beste Vorlage ist die getippte Ziffer,
        # nah genug. Bitgleichheit taugte nicht: Bei kleiner Schrift sieht
        # dieselbe Ziffer in jedem Abgriff anders aus, und beim zehnten „2,000"
        # hieß es wieder „4 neu" („das Fenster lügt meine User an", 17.09.2026).
        # Gespeichert wird trotzdem immer — mehr Beispiele, bessere Erkennung.
        # Sind alle bekannt, weiß der Spieler: genug angelernt.
        column = columns[position - 1]
        best = min(column, key=column.get) if column else None
        if best == digit and column[digit] <= MAX_DIGIT_DISTANCE:
            stats['bekannt'] += 1
        else:
            stats['neu'] += 1
        bucket = own.setdefault(digit, [])
        if pattern not in bucket:
            bucket.append(pattern)
            bucket[:] = bucket[-MAX_OWN_PER_DIGIT:]
    try:
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path + '.tmp', 'w', encoding='utf-8') as f:
            json.dump({'format': 1, 'raster': [NORM_W, NORM_H], 'ziffern': own}, f)
        os.replace(path + '.tmp', path)
    except OSError:
        return False, 'anlernen_speichern', {}
    save_sample(raster, ''.join(digits_typed))
    return True, None, stats


def png_bytes(raster):
    """Graustufenraster als PNG (reine Standardbibliothek)."""
    height = len(raster)
    width = len(raster[0]) if height else 0
    raw = bytearray()
    for row in raster:
        raw.append(0)
        raw.extend(max(0, min(255, int(v))) for v in row)

    def chunk(kind, body):
        block = kind + body
        return (struct.pack('>I', len(body)) + block
                + struct.pack('>I', zlib.crc32(block) & 0xFFFFFFFF))

    return (b'\x89PNG\r\n\x1a\n'
            + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 0, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(bytes(raw), 6))
            + chunk(b'IEND', b''))


def sample_folder():
    from . import paths
    return os.path.join(os.path.dirname(paths.app_file(OWN_TEMPLATE_FILE)),
                        SAMPLE_FOLDER)


def samples():
    """Die abgelegten Scan-Bilder (Dateinamen), älteste zuerst."""
    folder = sample_folder()
    try:
        return sorted((f for f in os.listdir(folder) if f.endswith('.png')),
                      key=lambda f: os.path.getmtime(os.path.join(folder, f)))
    except OSError:
        return []


def sample_archive():
    """Scan-Bilder und eigene Ziffernvorlagen als ZIP (Bytes) — oder None.

    Für den Fehlerbericht: Bild + richtige Zahl im Dateinamen ist genau das,
    woraus sich die Erkennung für alle verbessern lässt. Nichts Persönliches —
    nur der Ausschnitt um die Zahl.
    """
    import io
    import zipfile
    from . import paths
    names = samples()
    if not names:
        return None
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, 'w', zipfile.ZIP_DEFLATED) as archive:
        for name in names:
            archive.write(os.path.join(sample_folder(), name), 'bilder/' + name)
        own = paths.app_file(OWN_TEMPLATE_FILE)
        if os.path.isfile(own):
            archive.write(own, OWN_TEMPLATE_FILE)
    return buffer.getvalue()


def save_sample(raster, number):
    """Das angelernte Bild mit der richtigen Zahl ablegen.

    ⭐⭐ **Bild + richtige Antwort ist das Wertvollste für die Erkennung.** Aus
    genau solchen Paaren (82 Aufnahmen vom 10.09., Live-Bilder vom 17.09.2026)
    wurde der Kern vermessen und verbessert. Die Ziffernvorlagen allein sagen
    nicht, woran eine Lesung scheiterte. Name `<zahl>_<zeit>.png`, höchstens
    `SAMPLE_LIMIT`, die ältesten gehen zuerst. Nur der Ausschnitt um die Zahl —
    nichts vom übrigen Bildschirm.
    """
    folder = sample_folder()
    try:
        os.makedirs(folder, exist_ok=True)
        # Millisekunden im Namen: zweimal in derselben Sekunde angelernt
        # überschrieb sonst das erste Bild.
        now = time.time()
        name = '%s_%s-%03d.png' % (number, time.strftime('%Y%m%d-%H%M%S',
                                                        time.localtime(now)),
                                   int(now * 1000) % 1000)
        with open(os.path.join(folder, name), 'wb') as f:
            f.write(png_bytes(raster))
        files = sorted((f for f in os.listdir(folder) if f.endswith('.png')),
                       key=lambda f: os.path.getmtime(os.path.join(folder, f)))
        for old in files[:-SAMPLE_LIMIT]:
            os.remove(os.path.join(folder, old))
        return True
    except OSError:
        return False


# --------------------------------------------------------------------------
# Der festgelegte Bereich
# --------------------------------------------------------------------------

def region():
    """Der gemerkte Scan-Bereich (links, oben, breite, höhe) in Bildpunkten."""
    from . import paths
    # ⚠ `settings().get`, nicht `setting()`: Das gibt nur Text zurück, und eine
    # Liste gälte als „nicht gesetzt" — der Bereich wäre nach jedem Speichern weg.
    raw = paths.settings().get(REGION_SETTING)
    try:
        left, top, width, height = (int(v) for v in raw)
    except (TypeError, ValueError):
        return None
    if width < 8 or height < 6 or width > 2000 or height > 600:
        return None
    return left, top, width, height


def set_region(rect):
    from . import paths
    return paths.set_setting(REGION_SETTING, list(rect) if rect else None)
