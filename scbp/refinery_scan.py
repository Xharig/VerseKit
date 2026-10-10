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
Das Raffinerie-Terminal vom Bildschirm lesen.

Ablauf: Star-Citizen-Fenster abgreifen (`screen_grab`), als BMP in den
Temp-Ordner legen, mit der Texterkennung von Windows lesen
(`Windows.Media.Ocr` über ctypes, siehe `win_ocr`). Zuerst wird das
ganze Bild in vergrößerten Kacheln nach der Kopfzeile der Ausbeute-Tabelle
abgesucht (`search_jobs`, `find_table`) — das Terminal kann irgendwo auf
einem breiten Bildschirm stehen. Danach wird nur dieser Ausschnitt mehrfach
gelesen, die Tabellenzeilen zusammengesetzt und daraus Zeilen im Format von
`materials.refinery_lines` gebaut.

Geschrieben wird hier nichts ins Lager: Das Ergebnis ist Text, den die
Oberfläche in das Raffinerie-Feld legt. Eingetragen wird erst über den
Knopf dort.

| System | Stand |
|---|---|
| Windows 10/11 | gebaut — braucht eine installierte OCR-Sprache (de oder en) |
| Linux | nicht unterstützt (`supported()` ist False) |

⚠ Die Erkennung arbeitet auf Wörtern mit Begrenzungsrahmen, nicht auf den
Zeilen der OCR: Die Spalten des Terminals stehen weit auseinander, und die
OCR schneidet eine Tabellenzeile oft in mehrere Zeilen. `rows()` legt alle
Wörter mit überlappender Höhe wieder zu einer Zeile zusammen.

⚠ Die kleinen Ziffern des Terminals liest die OCR unzuverlässig: Ziffern
fallen weg oder werden verwechselt. Deshalb mehrere Lesungen mit
verschiedener Vergrößerung (`PASSES`), und ein Wert gilt erst, wenn
mindestens zwei Lesungen ihn gleich lasen (`merge_passes`). Unsichere
Werte bleiben frei und werden namentlich als unsicher angezeigt.
"""
import difflib
import os
import re
import sys
import tempfile
import time

class ScanError(Exception):
    """Lesen nicht möglich — `reason` ist ein Kennwort für die Oberfläche."""

    def __init__(self, reason, detail=''):
        Exception.__init__(self, reason)
        self.reason = reason
        self.detail = detail


# Die Texterkennung läuft im eigenen Prozess über `win_ocr` (ctypes), ohne
# Skript-Interpreter und ohne Unterprozess.
OCR_ENABLED = True


def supported():
    """Gibt es auf diesem System eine Texterkennung, die wir nutzen?"""
    return OCR_ENABLED and sys.platform == 'win32'


# Ergebnis der Tastenkombination im Spiel: Die Lager-Seite meldet sich mit
# `listen()` an; ist sie noch nicht gebaut, wartet das Ergebnis hier.
_HANDOVER = {'listener': None, 'pending': None}


def listen(listener):
    """Die Lager-Seite nimmt Lesungen aus dem Spiel entgegen — Tk-Faden.

    `listener(auftraege, aufgehoben)` gibt True zurück, wenn es angekommen
    ist. Ein Ergebnis, das vor dem Aufbau der Seite kam, wird sofort
    nachgereicht.
    """
    _HANDOVER['listener'] = listener
    pending, _HANDOVER['pending'] = _HANDOVER['pending'], None
    if pending is not None:
        hand_over(*pending)


def hand_over(jobs, kept):
    """Eine Lesung an die Lager-Seite geben — Tk-Faden.

    Ohne Lager-Seite oder mit abgebauter Seite bleibt das Ergebnis liegen,
    bis `listen()` es abholt.
    """
    listener = _HANDOVER['listener']
    if listener is not None:
        try:
            if listener(jobs, kept):
                return True
        except Exception as exc:
            from . import errors
            errors.record('refinery_scan.hand_over', exc)
    _HANDOVER['pending'] = (jobs, kept)
    return False


def scan_from_game():
    """Lesen, während Star Citizen vorn ist — ohne zu warten.

    Gibt `(aufträge, aufgehoben)` oder None, wenn das Spiel nicht vorn ist.
    Läuft in einem Arbeitsfaden.
    """
    from . import screen_grab
    rect = screen_grab.game_rect()
    if not rect:
        return None
    return read_screen(rect)


# --------------------------------------------------------------- Abgriff
def write_bmp(raw, width, height, path):
    """BGRA-Bytes (oben beginnend) als 24-Bit-BMP schreiben."""
    import struct
    row = width * 3
    pad = (4 - row % 4) % 4
    bgr = bytearray(width * height * 3)
    bgr[0::3] = raw[0::4]
    bgr[1::3] = raw[1::4]
    bgr[2::3] = raw[2::4]
    if pad:
        rows = [bytes(bgr[y * row:(y + 1) * row]) + b'\0' * pad
                for y in range(height)]
        data = b''.join(rows)
    else:
        data = bytes(bgr)
    header = struct.pack('<2sIHHI', b'BM', 54 + len(data), 0, 0, 54)
    # Negative Höhe: Zeilen von oben nach unten.
    info = struct.pack('<IiiHHIIiiII', 40, width, -height, 1, 24, 0,
                       len(data), 2835, 2835, 0, 0)
    with open(path, 'wb') as handle:
        handle.write(header + info + data)


def wait_for_game(timeout=15.0, step=0.25, now=time.monotonic,
                  sleep=time.sleep):
    """Warten, bis Star Citizen vorn ist. Gibt die Fensterfläche oder None.

    Läuft in einem Arbeitsfaden; holt nie selbst ein Fenster nach vorn.
    """
    from . import screen_grab
    end = now() + timeout
    while now() < end:
        rect = screen_grab.game_rect()
        if rect:
            # Der Spieler hat eben gewechselt — einen Augenblick, bis das
            # Bild des Spiels wieder vollständig gezeichnet ist.
            sleep(0.6)
            return screen_grab.game_rect() or rect
        sleep(step)
    return None


def grab_game(rect):
    """Die Spielfläche abgreifen und als BMP-Datei ablegen. Gibt den Pfad."""
    from . import screen_grab
    left, top, width, height = rect
    try:
        raw = screen_grab.grab_raw(left, top, width, height)
    except screen_grab.GrabError as error:
        raise ScanError('abgriff', error.reason)
    handle, path = tempfile.mkstemp(prefix='versekit-raffinerie-',
                                    suffix='.bmp')
    os.close(handle)
    write_bmp(raw, width, height, path)
    return path


# ------------------------------------------------------------------ OCR
def ocr_image(path, jobs, save=None, save_crop=None, timeout=180):
    """Ein BMP mit der Texterkennung von Windows lesen. Gibt das
    Ergebnis-dict `{'width', 'height', 'lang', 'passes': [{'box', 'scale',
    'mode', 'words'}, …]}`.

    Je Eintrag in `jobs` ein Durchgang `((x, y, b, h), vergrößerung, art)`;
    die Wortrahmen stehen in Bildpunkten des Originals. `save` legt das
    ganze Bild als PNG ab, verkleinert auf höchstens `KEEP_MAX_WIDTH`;
    `save_crop` = `((x, y, b, h), pfad)` einen Ausschnitt.
    """
    if not supported():
        raise ScanError('nicht_unterstuetzt')
    from . import win_ocr
    try:
        return win_ocr.ocr_image(path, jobs, save=save, save_crop=save_crop,
                                 keep_width=KEEP_MAX_WIDTH, timeout=timeout)
    except win_ocr.OcrError as error:
        if str(error) == 'keine_sprache':
            raise ScanError('keine_sprache')
        raise ScanError('ocr', str(error))
    except (OSError, ValueError) as error:
        raise ScanError('ocr', str(error))


# ------------------------------------------------------- Zeilen bauen
def rows(words):
    """Wörter zu Tabellenzeilen ordnen: [[wort, …], …], oben nach unten.

    Zwei Wörter gehören in dieselbe Zeile, wenn ihre senkrechte Mitte im
    Höhenbereich des anderen liegt. Innerhalb der Zeile von links nach rechts.
    """
    ordered = sorted((w for w in words or () if (w.get('t') or '').strip()),
                     key=lambda w: (w.get('y', 0) + w.get('h', 0) / 2.0))
    lines = []
    for word in ordered:
        middle = word.get('y', 0) + word.get('h', 0) / 2.0
        for line in lines:
            if line['top'] - 1 <= middle <= line['bottom'] + 1:
                line['words'].append(word)
                line['top'] = min(line['top'], word.get('y', 0))
                line['bottom'] = max(line['bottom'],
                                     word.get('y', 0) + word.get('h', 0))
                break
        else:
            lines.append({'top': word.get('y', 0),
                          'bottom': word.get('y', 0) + word.get('h', 0),
                          'words': [word]})
    lines.sort(key=lambda line: line['top'])
    return [sorted(line['words'], key=lambda w: w.get('x', 0))
            for line in lines]



# Zeichen, die die OCR in Ziffernfeldern gern verwechselt.
_DIGIT_LOOKALIKES = str.maketrans({'O': '0', 'o': '0', 'D': '0', 'Q': '0',
                                   'I': '1', 'l': '1', '|': '1', 'i': '1',
                                   '!': '1', 'S': '5', 's': '5', 'B': '8',
                                   'Z': '2', 'z': '2'})
_DIGIT_LOOKALIKE_CHARS = frozenset('OoDQIl|i!SsBZz')


def number(token, loose=False):
    """Eine Ganzzahl aus einem OCR-Wort — oder None.

    Verwechselte Zeichen (O/0, l/1, S/5) werden nur in Wörtern getauscht,
    die mindestens eine echte Ziffer enthalten — mit `loose` auch in
    Wörtern aus mindestens zwei Zeichen, die nur aus verwechselbaren Zeichen
    bestehen (`sos`). Das gilt nur in den Zahlenspalten hinter dem Namen. Tausenderpunkte und
    -kommas fallen weg.
    """
    text = (token or '').strip().strip('.,:;')
    if not text:
        return None
    if not any(c.isdigit() for c in text) and not (
            loose and len(text) >= 2
            and all(c in _DIGIT_LOOKALIKE_CHARS for c in text)):
        return None
    text = text.translate(_DIGIT_LOOKALIKES)
    text = re.sub(r'(?<=\d)[.,\s](?=\d{3}(?!\d))', '', text)
    if not re.fullmatch(r'\d+', text):
        return None
    return int(text)


def _material_index():
    from . import crafting
    names = {}
    for name in crafting.storable():
        names.setdefault(crafting.norm_material(name), name)
    return names


def match_material(text, index=None):
    """Den Rohstoff zu einem OCR-Namen — oder None.

    Erst genau (über `norm_material`), dann unscharf: Der beste Treffer muss
    mindestens 0,8 ähnlich sein und den zweitbesten deutlich schlagen.
    """
    from . import crafting
    index = _material_index() if index is None else index
    wanted = crafting.norm_material(re.sub(r'[^A-Za-zÀ-ÿ() ]', '',
                                           text or '')).strip()
    wanted = re.sub(r'\s+', ' ', wanted)
    if len(wanted) < 3:
        return None
    if wanted in index:
        return index[wanted]
    scored = sorted(((difflib.SequenceMatcher(None, wanted, key).ratio(), key)
                     for key in index), reverse=True)
    if not scored or scored[0][0] < 0.8:
        return None
    if len(scored) > 1 and scored[0][0] - scored[1][0] < 0.08:
        return None
    return index[scored[0][1]]


def material_rows(table, index=None):
    """Die Tabellenzeilen, die mit einem Rohstoff beginnen.

    Gibt eine Liste von dicts: `material`, `y` (senkrechte Mitte), `raw`
    (der gelesene Text) und `numbers` — die Zahlen rechts vom Namen als
    `(x_mitte, wert)`. `loose_numbers` in derselben Form: Wörter ohne echte
    Ziffer, nur aus verwechselbaren Zeichen (`sos`).
    """
    index = _material_index() if index is None else index
    out = []
    for words in table:
        words = [w for w in words if (w.get('t') or '').strip()]
        name_parts = []
        for word in words:
            # Nach dem ersten Namenswort beendet auch ein Wort aus lauter
            # verwechselbaren Zeichen (`sos`) den Namen.
            if number(word['t'], loose=bool(name_parts)) is not None:
                break
            name_parts.append(word['t'].strip())
        if not name_parts or len(name_parts) > 5:
            continue
        material = match_material(' '.join(name_parts), index)
        if material is None:
            continue
        numbers, loose_numbers = [], []
        for word in words[len(name_parts):]:
            middle = word.get('x', 0) + word.get('w', 0) / 2.0
            value = number(word['t'])
            if value is not None:
                numbers.append((middle, value))
                continue
            value = number(word['t'], loose=True)
            if value is not None:
                loose_numbers.append((middle, value))
        top = min(w.get('y', 0) for w in words)
        bottom = max(w.get('y', 0) + w.get('h', 0) for w in words)
        out.append({'material': material, 'y': (top + bottom) / 2.0,
                    'height': max(1, bottom - top),
                    'raw': ' '.join(w['t'].strip() for w in words),
                    'numbers': numbers, 'loose_numbers': loose_numbers})
    return out


def columns(found_rows):
    """Die Zahlenspalten als x-Mitten, links nach rechts — ohne Überschrift.

    Alle Zahlen aller Rohstoffzeilen werden nach x geordnet — auch die aus
    verwechselbaren Zeichen (`loose_numbers`), damit eine Spalte nicht
    verschwindet, wenn sie nur so gelesen wurde. Eine Lücke von mehr als drei
    Zeilenhöhen beginnt eine neue Spalte.
    """
    xs = sorted(x for row in found_rows
                for x, _v in row['numbers'] + list(row.get('loose_numbers') or ()))
    if not xs:
        return []
    heights = sorted(row['height'] for row in found_rows)
    gap = 3.0 * heights[len(heights) // 2]
    groups = [[xs[0]]]
    for x in xs[1:]:
        if x - groups[-1][-1] > gap:
            groups.append([x])
        else:
            groups[-1].append(x)
    return [sum(g) / len(g) for g in groups]


# Überschriften der Spalten, deutsch und englisch, in Großbuchstaben. Die
# Wortteile tragen auch bei verlesenen Buchstaben (`OUALITåT`).
_QUALITY_HEADS = ('UALIT', 'QUALI')
_YIELD_HEADS = ('AUSBEUTE', 'ERTRAG', 'YIELD', 'BEUTE')


def header_columns(table):
    """Die Spaltenmitten aus der Überschriftenzeile — oder None.

    Gibt eine Liste von x-Mitten: zuerst Qualität, dann Ausbeute, danach
    jede weitere Überschrift rechts davon (`ZU`, `FERTIG`, `TO-DO` …), damit
    deren Zahlen nicht in die Ausbeute rutschen.
    """
    for words in table:
        quality = yield_ = None
        for word in words:
            text = (word.get('t') or '').upper()
            middle = word.get('x', 0) + word.get('w', 0) / 2.0
            if quality is None and any(h in text for h in _QUALITY_HEADS):
                quality = middle
            elif yield_ is None and any(h in text for h in _YIELD_HEADS):
                yield_ = middle
        if quality is not None and yield_ is not None and yield_ > quality:
            others = [w.get('x', 0) + w.get('w', 0) / 2.0 for w in words
                      if w.get('x', 0) + w.get('w', 0) / 2.0 > yield_ + 1]
            return [quality, yield_] + others
    return None


def parse_rows(table, index=None):
    """Aus Tabellenzeilen einer Lesung die Ausbeute lesen.

    Gibt eine Liste von dicts je Rohstoffzeile: `material`, `quality`,
    `amount` (cSCU, je None, wenn nicht lesbar), `y`, `height`, `raw`.

    Jede Zahl gehört zur nächstgelegenen Spalte — nach der Überschrift,
    sonst nach den Zahlen selbst (`columns`). Die erste Spalte ist die
    Qualität (0–1000), die zweite die Ausbeute (über 0); weitere Spalten
    bleiben unbeachtet.
    """
    found_rows = material_rows(table, index)
    centers = header_columns(table) or columns(found_rows)
    def _slots(pairs):
        slots = {}
        if not centers:
            return None, None
        for x, value in pairs:
            nearest = min(range(len(centers)), key=lambda i: abs(centers[i] - x))
            slots.setdefault(nearest, value)
        quality, amount = slots.get(0), slots.get(1)
        if quality is not None and not 0 <= quality <= 1000:
            quality = None
        if amount is not None and amount <= 0:
            amount = None
        return quality, amount

    result = []
    for row in found_rows:
        quality, amount = _slots(row['numbers'])
        loose_quality, loose_amount = _slots(row.get('loose_numbers') or ())
        result.append({'material': row['material'], 'quality': quality,
                       'amount': amount, 'loose_quality': loose_quality,
                       'loose_amount': loose_amount, 'y': row['y'],
                       'height': row['height'], 'raw': row['raw']})
    return result


# So viele Durchgänge müssen einen Wert gleich gelesen haben, damit er gilt.
MIN_VOTES = 2


# Liest ein zweiter Wert mindestens so oft, ist die Zelle umstritten. Bei 1
# muss jede Lesung, die überhaupt eine Zahl ergab, dieselbe Zahl ergeben.
RIVAL_VOTES = 1


def _vote(values, minimum, rival=None):
    """Der Wert, den die meisten Durchgänge lasen — oder None.

    None, wenn er seltener als `minimum` vorkommt, ein zweiter Wert gleich
    oft gelesen wurde oder ein zweiter Wert mindestens `rival`-mal vorkommt.
    """
    counts = {}
    for value in values:
        if value is not None:
            counts[value] = counts.get(value, 0) + 1
    if not counts:
        return None
    ranked = sorted(counts.items(), key=lambda kv: -kv[1])
    if ranked[0][1] < minimum:
        return None
    if len(ranked) > 1 and ranked[0][1] == ranked[1][1]:
        return None
    if rival and len(ranked) > 1 and ranked[1][1] >= rival:
        return None
    return ranked[0][0]


# Ziffern, die die OCR in der Terminal-Schrift untereinander verwechselt:
# Die Null hat einen Schrägstrich und wird von allen Durchgängen gleich als 8
# gelesen (und umgekehrt). Ein Wert mit einer dieser Ziffern gilt nur, wenn
# eine zweite, unabhängige Prüfung (`confirm`) ihn bestätigt.
CONFUSABLE = frozenset('08')


def needs_confirm(value):
    """Enthält die Zahl eine Ziffer aus `CONFUSABLE`?"""
    return value is not None and bool(set(str(value)) & CONFUSABLE)


def _guess(values):
    """Der am häufigsten gelesene Wert; bei Gleichstand der zuerst gelesene.
    None, wenn keine Lesung eine Zahl ergab."""
    counts, first = {}, {}
    for position, value in enumerate(values):
        if value is not None:
            counts[value] = counts.get(value, 0) + 1
            first.setdefault(value, position)
    if not counts:
        return None
    return min(counts, key=lambda v: (-counts[v], first[v]))


def merge_passes(passes, index=None, minimum=None, confirm=None, details=None,
                 guess=False, reread=None):
    """Mehrere Lesungen desselben Bildes zu einer Ausbeute zusammenführen.

    Zeilen verschiedener Durchgänge gehören zusammen, wenn ihre senkrechte
    Mitte weniger als eine halbe Zeilenhöhe auseinanderliegt. Rohstoff,
    Qualität und Menge werden je für sich abgestimmt (`_vote`): Ein Wert
    gilt erst, wenn mindestens `minimum` Durchgänge ihn gleich lasen.

    Enthält ein abgestimmter Wert eine Ziffer aus `CONFUSABLE`, gilt er nur,
    wenn `confirm(zeile, spalte, wert)` True zurückgibt — `zeile` ist die
    senkrechte Mitte, `spalte` 0 für Qualität, 1 für Menge. Gibt `confirm`
    eine Zahl zurück, gilt diese statt des gelesenen Werts (eine 8, die eine
    Null ist). Ohne `confirm` ist ein solcher Wert unsicher.

    `details`: Liste, an die je unsicherer Zeile `{'material', 'y'}` gehängt
    wird. `guess`: Unsichere Zeilen stehen trotzdem in `zeilen` — mit dem am
    häufigsten gelesenen Wert (`_guess`), zum Vergleichen mit dem Terminal;
    in `unsicher` stehen sie weiterhin. `reread(zeile, spalte)` liest eine
    fehlende Zelle neu (Zahl oder None); gelingt das für beide, ist die Zeile
    sicher.

    Gibt `(zeilen, unsicher)`: `zeilen` als `(material, qualität,
    menge_cscu)` von oben nach unten; `unsicher` als Liste der Rohstoffe,
    deren Zahlen nicht sicher lesbar waren — sie werden nicht geraten.
    """
    index = _material_index() if index is None else index
    minimum = MIN_VOTES if minimum is None else minimum
    minimum = min(minimum, max(1, len(passes)))
    clusters = []
    for words in passes:
        for row in parse_rows(rows(words), index):
            for cluster in clusters:
                if abs(cluster['y'] - row['y']) < row['height'] / 2.0:
                    cluster['rows'].append(row)
                    break
            else:
                clusters.append({'y': row['y'], 'rows': [row]})
    clusters.sort(key=lambda c: c['y'])
    found, unsure = [], []
    for cluster in clusters:
        material = _vote([r['material'] for r in cluster['rows']], 1)
        quality = _vote([r['quality'] for r in cluster['rows']], minimum,
                        RIVAL_VOTES)
        amount = _vote([r['amount'] for r in cluster['rows']], minimum,
                       RIVAL_VOTES)
        if material is None:
            continue
        for column, value in ((0, quality), (1, amount)):
            if not needs_confirm(value):
                continue
            checked = confirm(cluster['y'], column, value) if confirm else None
            if checked is True:
                continue
            if isinstance(checked, int) and not isinstance(checked, bool):
                value = checked
            else:
                value = None
            if column == 0:
                quality = value
            else:
                amount = value
        if reread is not None and (quality is None or amount is None):
            if quality is None:
                quality = reread(cluster['y'], 0)
            if amount is None:
                amount = reread(cluster['y'], 1)
        if quality is None or amount is None:
            unsure.append(material)
            if details is not None:
                details.append({'material': material, 'y': cluster['y']})
            if guess:
                # Echte Ziffern zuerst; Lesungen aus verwechselbaren Zeichen
                # nur, wenn kein Durchgang echte Ziffern las.
                rows_ = cluster['rows']

                def best(key):
                    value = _guess([r[key] for r in rows_])
                    if value is None:
                        value = _guess([r.get('loose_' + key) for r in rows_])
                    return value
                if quality is None:
                    quality = best('quality')
                if amount is None:
                    amount = best('amount')
                if quality is not None or amount is not None:
                    found.append((material, quality, amount))
            continue
        found.append((material, quality, amount))
    return found, unsure


def as_text(found):
    """Die Zeilen im Format des Raffinerie-Felds (`Material Q Menge cSCU`).
    Ein Wert, der nicht gelesen wurde (None), steht als `?` da."""
    def cell(value):
        return '?' if value is None else '%d' % value
    return '\n'.join('%s %s %s cSCU' % (name, cell(quality), cell(amount))
                     for name, quality, amount in found)


# Die Durchgänge über den Tabellenausschnitt. Die Ziffern im Terminal sind
# auf einem 1440er-Bildschirm nur rund acht Bildpunkte hoch; erst vergrößert
# liest die OCR sie, und jeder Durchgang verliest andere Ziffern.
PASSES = ((3.0, 'plain'), (5.0, 'plain'), (3.5, 'dark'), (4.0, 'dark'))

# Größte Kantenlänge, die `Windows.Media.Ocr` annimmt.
OCR_LIMIT = 10000
# Die Suche nach der Tabelle vergrößert so, dass die Bildhöhe danach etwa
# diesem Wert entspricht: Die Schrift des Terminals wächst mit der
# Bildschirmhöhe, und ab rund 20 Bildpunkten Schrifthöhe liest die OCR sie.
SEARCH_HEIGHT = 4320

# Spaltenköpfe der Ausbeute-Tabelle (`refinery_ui_JobCard_Table_*` in der
# `global.ini`, deutsch und englisch), nur Großbuchstaben. Wortteile, damit
# auch verlesene Buchstaben noch treffen (`QUALITÄT` als `OUAUTiT`).
_HEAD_QUALITY = ('UALI', 'UAUT', 'QUAL', 'QUAU', 'ALIT', 'AUTY')
_HEAD_YIELD = ('AUSBEUT', 'USBEUT', 'YIELD', 'ERTRAG')
# Die Kopfwörter der Materialspalte links davon.
_HEAD_MATERIAL = ('WONNEN', 'GEWONN', 'MATERIA', 'ATERIAL', 'CSCU',
                  'YIELDED', 'IELDED')


def image_size(path):
    """(breite, höhe) einer BMP- oder PNG-Datei — oder None."""
    import struct
    try:
        with open(path, 'rb') as handle:
            head = handle.read(32)
    except OSError:
        return None
    if head[:2] == b'BM' and len(head) >= 26:
        width, height = struct.unpack('<ii', head[18:26])
        return width, abs(height)
    if head[:8] == b'\x89PNG\r\n\x1a\n' and len(head) >= 24:
        return struct.unpack('>II', head[16:24])
    return None


def search_jobs(width, height):
    """Die OCR-Aufträge, mit denen die Tabelle auf dem ganzen Bild gesucht wird.

    Überlappende Kacheln, je etwa 0,9 × 0,5 Bildhöhen groß, vergrößert auf
    `SEARCH_HEIGHT` / Bildhöhe (1 bis 4). Eine Kachel überlappt die nächste
    um ein Viertel, damit eine Zeile, die an einer Kante liegt, in der
    Nachbarkachel ganz steht.
    """
    scale = max(1.0, min(4.0, SEARCH_HEIGHT / float(max(1, height))))
    tile_w = min(width, int(height * 0.9))
    tile_h = min(height, max(1, int(height * 0.5)))
    tile_w = min(tile_w, int(OCR_LIMIT / scale))
    tile_h = min(tile_h, int(OCR_LIMIT / scale))

    def starts(total, size):
        if size >= total:
            return [0]
        step = max(1, int(size * 0.75))
        out = list(range(0, total - size, step))
        out.append(total - size)
        return out

    return [((x, y, tile_w, tile_h), scale, 'plain')
            for y in starts(height, tile_h) for x in starts(width, tile_w)]


def merge_words(passes):
    """Die Wörter mehrerer Kacheln zu einer Liste, ohne Doppelte.

    Ein Wort, das in zwei überlappenden Kacheln gelesen wurde, steht einmal:
    gleicher Text und Mitten weniger als eine halbe Wortlänge auseinander.
    """
    out = []
    for words in passes:
        for word in words or ():
            text = (word.get('t') or '').strip()
            if not text:
                continue
            cx = word.get('x', 0) + word.get('w', 0) / 2.0
            cy = word.get('y', 0) + word.get('h', 0) / 2.0
            limit = max(word.get('h', 1), word.get('w', 1) / 2.0)
            for other in out:
                if (other['t'] == text
                        and abs(other['x'] + other['w'] / 2.0 - cx) < limit
                        and abs(other['y'] + other['h'] / 2.0 - cy)
                        < max(1, word.get('h', 1))):
                    break
            else:
                out.append(dict(word, t=text))
    return out


def _head(word, keys):
    """Enthält das Wort (nur seine Buchstaben, groß) einen der Wortteile?"""
    text = re.sub(r'[^A-Z]', '', (word.get('t') or '').upper())
    return any(key in text for key in keys)


def find_tables(words, width, height):
    """Alle Ausbeute-Tabellen auf dem Bild, links nach rechts, oben nach unten.

    Gibt eine Liste von dicts: `box` = `(x, y, b, h)` und `total` — die
    Summe als Text mit Punkt (`12.76`) oder ''. Stehen zwei Auftragskarten nebeneinander,
    hat jede ihre eigene Kopfzeile und damit ihren eigenen Eintrag.

    Eine Kopfzeile ist eine Zeile mit einem Qualitäts-Kopf und rechts
    daneben einem Ausbeute-Kopf. Die Materialliste links im Terminal
    (Stationsprofil) hat keinen Qualitäts-Kopf und zählt deshalb nicht.

    Der Ausschnitt reicht
    - links bis zum Kopf der Materialspalte (`_HEAD_MATERIAL`, höchstens
      30 Schrifthöhen links der Qualität), ohne ihn bis 24 Schrifthöhen,
    - rechts bis zum letzten Kopfwort (höchstens 25 Schrifthöhen rechts der
      Qualität),
    - oben bis über die Kopfzeile,
    - unten bis zur Summenzeile (ein `cSCU` oder ein Ausbeute-Wort links
      von der Qualitätsspalte, unterhalb der Kopfzeile), ohne sie bis
      55 Schrifthöhen.
    Alles links der Materialspalte bleibt draußen — auch Wörter, die die
    Zeilenbildung derselben Zeile zuschlägt.
    """
    candidates = []
    for line in rows(words):
        for quality in (w for w in line if _head(w, _HEAD_QUALITY)):
            unit = max(4, quality.get('h', 8))
            qx = quality.get('x', 0)
            yield_ = [w for w in line if _head(w, _HEAD_YIELD)
                      and 0 < w.get('x', 0) - qx < 20 * unit]
            if not yield_:
                continue
            heads = [w for w in line if _head(w, _HEAD_MATERIAL)
                     and 0 < qx - w.get('x', 0) < 30 * unit]
            rights = [w for w in line if 0 <= w.get('x', 0) - qx < 25 * unit]
            candidates.append((len(heads) + len(rights), quality, unit,
                               heads, rights))
    # Je Kopfzeile ein Eintrag: Ein zweiter Qualitäts-Kopf in derselben
    # Tabelle (verlesen, doppelt) liegt näher als 30 Schrifthöhen.
    chosen = []
    for candidate in sorted(candidates, key=lambda c: -c[0]):
        quality, unit = candidate[1], candidate[2]
        if any(abs(quality.get('x', 0) - other[1].get('x', 0)) < 30 * unit
               and abs(quality.get('y', 0) - other[1].get('y', 0)) < 10 * unit
               for other in chosen):
            continue
        chosen.append(candidate)
    tables = []
    for _score, quality, unit, heads, rights in chosen:
        table = _table_box(words, width, height, quality, unit, heads, rights)
        if table:
            tables.append(table)
    tables.sort(key=lambda t: (t['box'][0], t['box'][1]))
    return tables


def _table_box(words, width, height, quality, unit, heads, rights):
    """Ausschnitt und Summe zu einer gefundenen Kopfzeile (`find_tables`)."""
    qx = quality.get('x', 0)
    left = min(w.get('x', 0) for w in heads) if heads else qx - 24 * unit
    header = heads + rights
    right = max(w.get('x', 0) + w.get('w', 0) for w in header)
    top = min(w.get('y', 0) for w in header)
    header_bottom = max(w.get('y', 0) + w.get('h', 0) for w in header)
    below = [w for w in words
             if w.get('y', 0) > header_bottom + 2 * unit
             and w.get('y', 0) < header_bottom + 60 * unit
             and left - unit <= w.get('x', 0) <= right
             and (_head(w, ('SCU',))
                  or (_head(w, _HEAD_YIELD) and w.get('x', 0) < qx))]
    total = ''
    if below:
        bottom = min(w.get('y', 0) for w in below)
        line = [w for w in words
                if abs(w.get('y', 0) - bottom) < 3 * unit
                and left - unit <= w.get('x', 0) <= right + 4 * unit]
        for word in sorted(line, key=lambda w: w.get('x', 0)):
            found = re.search(r'\d+(?:[.,]\d+)?', word.get('t') or '')
            if found:
                total = found.group(0).replace(',', '.')
                break
    else:
        bottom = header_bottom + 55 * unit
    x0 = max(0, int(left - unit))
    y0 = max(0, int(top - unit))
    x1 = min(width, int(right + 2 * unit))
    y1 = min(height, int(bottom))
    if x1 <= x0 or y1 <= y0:
        return None
    return {'box': (x0, y0, x1 - x0, y1 - y0), 'total': total}


def find_table(words, width, height):
    """Der Ausschnitt der ersten Ausbeute-Tabelle (`find_tables`) — oder None."""
    tables = find_tables(words, width, height)
    return tables[0]['box'] if tables else None


# Wie viele missglückte Lesungen aufgehoben werden, für einen Fehlerbericht,
# und wie breit ein aufgehobenes Vollbild höchstens ist.
KEEP_SCANS = 3
KEEP_MAX_WIDTH = 2560
KEEP_FOLDER = 'raffinerie-bilder'


def keep_folder():
    from . import paths
    return os.path.join(os.path.dirname(paths.app_file('x')), KEEP_FOLDER)


def kept_images():
    """Die aufgehobenen Bilder (Dateinamen), älteste zuerst."""
    folder = keep_folder()
    try:
        return sorted(f for f in os.listdir(folder)
                      if f.startswith('raffinerie_') and f.endswith('.png'))
    except OSError:
        return []


def keep(source, kind):
    """Ein PNG als `raffinerie_<zeit>_<kind>.png` in `KEEP_FOLDER` legen.

    Danach bleiben nur die neuesten `KEEP_SCANS` Bilder. Gibt True, wenn das
    Bild abgelegt wurde.
    """
    import shutil
    if not source or not os.path.isfile(source):
        return False
    folder = keep_folder()
    try:
        os.makedirs(folder, exist_ok=True)
        now = time.time()
        stamp = '%s-%03d' % (time.strftime('%Y%m%d-%H%M%S', time.localtime(now)),
                             int(now * 1000) % 1000)
        shutil.move(source, os.path.join(
            folder, 'raffinerie_%s_%s.png' % (stamp, kind)))
        for name in kept_images()[:-KEEP_SCANS]:
            os.remove(os.path.join(folder, name))
        return True
    except OSError as exc:
        from . import errors
        errors.record('refinery_scan.keep', exc)
        return False


# ------------------------------------------------------- Ziffern lernen
# Gelernte Ziffernbilder der Terminal-Schrift, je Ziffer eine Liste von
# Mustern. Mit ihnen werden unsichere Zellen Ziffer für Ziffer gelesen
# (`read_cell`) und 0 und 8 auseinandergehalten (`CONFUSABLE`).
DIGIT_FILE = 'raffinerie-ziffern.json'
# Größe eines Musters in Bildpunkten und Höhe, auf die eine Zelle vor dem
# Zerlegen vergrößert wird.
DIGIT_W, DIGIT_H = 10, 14
CELL_HEIGHT = 42
# Höchstens so viele Muster je Ziffer; die ältesten fallen heraus.
DIGIT_KEEP = 40
# Mindestzahl der Muster für 0 und für 8, ab der `classify` zuordnet.
DIGIT_MIN = 2
# Die andere Ziffer muss um diesen Faktor weiter weg sein als die gewählte.
DIGIT_MARGIN = 1.3
# Weiter als das darf eine Ziffer von ihrem nächsten Muster nicht liegen —
# sonst ist sie eine, die noch nicht gelernt wurde.
DIGIT_MAX_DISTANCE = 0.2
# Abstandsfaktor zwischen 0 und 8 bei der Zuordnung über alle Ziffern.
CONFUSABLE_MARGIN = 2.0


def load_digits():
    """Die gelernten Muster: {'0': [[…], …], '8': …} — oder leer."""
    import json
    from . import paths
    try:
        with open(paths.app_file(DIGIT_FILE), encoding='utf-8') as handle:
            data = json.load(handle)
        return {k: v for k, v in data.get('ziffern', {}).items()
                if isinstance(v, list)}
    except (OSError, ValueError, AttributeError):
        return {}


def save_digits(digits):
    import json
    from . import paths
    target = paths.app_file(DIGIT_FILE)
    try:
        os.makedirs(os.path.dirname(target), exist_ok=True)
        with open(target + '.tmp', 'w', encoding='utf-8') as handle:
            json.dump({'format': 1, 'ziffern': digits}, handle)
        os.replace(target + '.tmp', target)
        return True
    except OSError as exc:
        from . import errors
        errors.record('refinery_scan.save_digits', exc)
        return False


def cell_crop(src, box):
    """Graustufenbild `(breite, höhe, bytes)` einer Zahlenzelle, vergrößert
    auf `CELL_HEIGHT`. `src` ist `(breite, höhe, BGRA)` des ganzen Bildes."""
    from . import win_ocr
    x, y, w, h = (int(v) for v in box)
    x, y = max(0, x), max(0, y)
    w, h = min(w, src[0] - x), min(h, src[1] - y)
    if w <= 0 or h <= 0:
        return None
    tw, th, raw = win_ocr.scaled(src, (x, y, w, h), CELL_HEIGHT / float(h))
    return tw, th, bytes(raw[1::4])


def _digit_spans(cols, count):
    """Die waagerechten Bereiche `(links, rechts)` der `count` Ziffern.

    `cols` sind die Spalten mit Schrift. Zusammenhängende Läufe sind die
    Ziffern; ohne `count` werden sie so zurückgegeben. Sind es mehr als
    `count`, werden die Läufe über die kleinsten Lücken zusammengelegt. Sind
    es weniger, wird der Schriftbereich in `count` gleiche Teile geschnitten.
    """
    spans = [[cols[0], cols[0] + 1]]
    for x in cols[1:]:
        if x == spans[-1][1]:
            spans[-1][1] = x + 1
        else:
            spans.append([x, x + 1])
    if count is None:
        return [tuple(s) for s in spans]
    while len(spans) > count:
        gaps = [(spans[i + 1][0] - spans[i][1], i) for i in range(len(spans) - 1)]
        _gap, i = min(gaps)
        spans[i:i + 2] = [[spans[i][0], spans[i + 1][1]]]
    if len(spans) == count:
        return [tuple(s) for s in spans]
    x0, x1 = cols[0], cols[-1] + 1
    step = (x1 - x0) / float(count)
    return [(x0 + step * i, x0 + step * (i + 1)) for i in range(count)]


def glyphs(crop, count=None):
    """Die `count` Ziffern einer Zelle als Muster (Liste von Zahlenreihen).

    Die Schrift setzt Ziffern gleich breit: Der Bereich mit Schrift wird in
    `count` gleiche Teile geschnitten, jeder Teil auf `DIGIT_W` × `DIGIT_H`
    gebracht und auf Mittelwert 0 und Länge 1 gesetzt.
    """
    if not crop or (count is not None and count <= 0):
        return []
    width, height, gray = crop
    low, high = min(gray), max(gray)
    if high - low < 30:
        return []
    cut = (low + high) / 2.0
    cols = [x for x in range(width)
            if any(gray[y * width + x] > cut for y in range(height))]
    rows_ = [y for y in range(height)
             if any(gray[y * width + x] > cut for x in range(width))]
    if not cols or not rows_:
        return []
    y0, y1 = rows_[0], rows_[-1] + 1
    out = []
    for left, right in _digit_spans(cols, count):
        step = right - left
        vector = []
        for gy in range(DIGIT_H):
            sy = min(height - 1, int(y0 + (gy + 0.5) * (y1 - y0) / DIGIT_H))
            for gx in range(DIGIT_W):
                sx = min(width - 1, int(left + (gx + 0.5) * step / DIGIT_W))
                vector.append(gray[sy * width + sx])
        mean = sum(vector) / float(len(vector))
        vector = [v - mean for v in vector]
        norm = sum(v * v for v in vector) ** 0.5 or 1.0
        out.append([round(v / norm, 4) for v in vector])
    return out


def _distance(a, b):
    return 1.0 - sum(p * q for p, q in zip(a, b))


def classify(vector, digits, candidates='08'):
    """Welche der `candidates` ist dieses Ziffernbild — oder None.

    Nur wenn jede Kandidatin mindestens `DIGIT_MIN` Muster hat und die beste
    um `CONFUSABLE_MARGIN` näher liegt als die zweitbeste — 0 und 8 liegen
    in der Terminal-Schrift dicht beieinander.
    """
    scores = []
    for digit in candidates:
        patterns = digits.get(digit) or []
        if len(patterns) < DIGIT_MIN:
            return None
        scores.append((min(_distance(vector, p) for p in patterns), digit))
    scores.sort()
    if len(scores) > 1 and scores[0][0] * CONFUSABLE_MARGIN > scores[1][0]:
        return None
    return scores[0][1]


def classify_any(vector, digits):
    """Welche Ziffer ist dieses Bild — unter allen mit mindestens
    `DIGIT_MIN` Mustern. None, wenn die nächste weiter als
    `DIGIT_MAX_DISTANCE` liegt oder die zweitnächste nicht um `DIGIT_MARGIN`
    weiter weg ist."""
    scores = sorted((min(_distance(vector, p) for p in patterns), digit)
                    for digit, patterns in digits.items()
                    if len(patterns) >= DIGIT_MIN)
    if not scores or scores[0][0] > DIGIT_MAX_DISTANCE:
        return None
    if len(scores) > 1 and scores[0][0] * DIGIT_MARGIN > scores[1][0]:
        return None
    best = scores[0][1]
    if best in CONFUSABLE:
        # 0 und 8 sehen sich in der Terminal-Schrift fast gleich: Die andere
        # der beiden muss um `CONFUSABLE_MARGIN` weiter weg liegen.
        other = [s for s, d in scores if d in CONFUSABLE and d != best]
        if not other or scores[0][0] * CONFUSABLE_MARGIN > other[0]:
            return None
    return best


def read_cell(crop, digits, read=''):
    """Eine Zahlenzelle allein an den gelernten Mustern lesen — oder None.

    Die Ziffern sind die Läufe zwischen den Lücken (`glyphs` ohne Zahl);
    jede muss `classify_any` eindeutig zuordnen. Gelingt das nicht und hat
    das gelesene Wort `read` eine andere Stellenzahl (zwei Ziffern berühren
    sich), wird nach dieser Stellenzahl geschnitten und noch einmal gelesen.
    """
    attempts = [glyphs(crop)]
    value = number(read, loose=True)
    if value is not None and len(str(value)) != len(attempts[0]):
        attempts.append(glyphs(crop, len(str(value))))
    for vectors in attempts:
        if not 1 <= len(vectors) <= 5:
            continue
        out = [classify_any(v, digits) for v in vectors]
        if None not in out:
            return int(''.join(out))
    return None


def _centers(passes, index=None):
    """Die Spaltenmitten der Tafel — aus der Lesung mit den meisten Zahlen."""
    best, most = [], -1
    for words in passes:
        table = rows(words)
        found = material_rows(table, index)
        count = sum(len(r['numbers']) + len(r.get('loose_numbers') or ())
                    for r in found)
        if count > most:
            best, most = (header_columns(table) or columns(found)), count
    return best


def _cell_word(passes, y, column, tolerance, centers):
    """Das Wort der Zelle in Zeile `y`, Spalte `column` (0 Qualität, 1 Menge)
    — aus der ersten Lesung, die dort eine Zahl hat. Oder None."""
    if len(centers) <= column:
        return None
    for words in passes:
        for word in words:
            if number(word.get('t'), loose=True) is None:
                continue
            if abs(word.get('y', 0) + word.get('h', 0) / 2.0 - y) > tolerance:
                continue
            middle = word.get('x', 0) + word.get('w', 0) / 2.0
            nearest = min(range(len(centers)),
                          key=lambda i: abs(centers[i] - middle))
            if nearest == column:
                return word
    return None


def _tolerance(passes):
    heights = sorted(w.get('h', 0) for words in passes for w in words)
    return heights[len(heights) // 2] if heights else 10


def _cell_crop_at(src, passes, y, column, centers):
    word = _cell_word(passes, y, column, _tolerance(passes), centers)
    if word is None:
        return None, None
    return word, cell_crop(src, (word['x'], word['y'], word['w'], word['h']))


def _confirmer(src, passes, digits):
    """`confirm` für `merge_passes`: entscheidet jede 0 und 8 eines Werts
    an den gelernten Mustern. Gibt den Wert (berichtigt) oder None.

    Zuerst wird die ganze Zelle gelesen (`read_cell`); gelingt das, gilt
    dieser Wert. Sonst ordnet `classify` nur jede 0 und 8 des gelesenen Werts zu.
    """
    if not src or not digits:
        return None
    centers = _centers(passes)

    def confirm(y, column, value):
        text = str(value)
        _word, crop = _cell_crop_at(src, passes, y, column, centers)
        if crop is None:
            return None
        whole = read_cell(crop, digits, text)
        if whole is not None:
            return whole
        found = glyphs(crop, len(text))
        if len(found) != len(text):
            return None
        out = []
        for digit, vector in zip(text, found):
            if digit in CONFUSABLE:
                digit = classify(vector, digits)
                if digit is None:
                    return None
            out.append(digit)
        return int(''.join(out))
    return confirm


def _rereader(src, passes, digits):
    """`reread` für `merge_passes`: liest eine Zelle einer unsicheren Zeile
    allein an den gelernten Mustern (`read_cell`) — oder None."""
    if not src or not digits:
        return None
    centers = _centers(passes)

    def reread(y, column):
        word, crop = _cell_crop_at(src, passes, y, column, centers)
        if crop is None:
            return None
        return read_cell(crop, digits, word.get('t') or '')
    return reread


def unsure_cells(src, passes, details):
    """Zahlenzellen der unsicheren Zeilen zum späteren Lernen: je Zelle
    `{'material', 'column', 'read', 'crop'}` — `read` ist das gelesene Wort."""
    if not src:
        return []
    centers = _centers(passes)
    cells = []
    for row in details:
        for column in (0, 1):
            word, crop = _cell_crop_at(src, passes, row['y'], column, centers)
            if crop:
                cells.append({'material': row['material'], 'column': column,
                              'read': word.get('t') or '', 'crop': crop})
    return cells


def _loose(text):
    """Ziffernfolge mit 0 und 8 gleichgesetzt — zum Zuordnen."""
    value = number(text, loose=True)
    return None if value is None else str(value).replace('8', '0')


def learn(cells, typed):
    """Aus nachgetippten Werten lernen. `typed`: Liste `(material, qualität,
    menge_cscu)`. Gibt die Zahl der gelernten Ziffern.

    Eine Zelle wird dem getippten Wert desselben Rohstoffs zugeordnet, der
    — 0 und 8 gleichgesetzt — dieselben Ziffern hat wie das gelesene Wort;
    gibt es den Rohstoff nur einmal, genügt dieselbe Stellenzahl. Eine Zelle
    ohne lesbares Wort lehrt nichts.
    """
    if not cells or not typed:
        return 0
    digits = load_digits()
    learned = 0
    for cell in cells:
        same = [row for row in typed if row[0] == cell['material']]
        values = [str(int(row[1 + cell['column']])) for row in same
                  if row[1 + cell['column']] is not None]
        loose = _loose(cell['read'])
        if loose is None:
            continue
        match = [v for v in values if v.replace('8', '0') == loose]
        if not match and len(values) == 1 and len(values[0]) == len(loose):
            match = values
        if len(match) != 1:
            continue
        value = match[0]
        found = glyphs(cell['crop'], len(value))
        if len(found) != len(value):
            continue
        for digit, vector in zip(value, found):
            patterns = digits.setdefault(digit, [])
            patterns.append(vector)
            del patterns[:-DIGIT_KEEP]
            learned += 1
    if learned:
        save_digits(digits)
    return learned


def _source(path):
    """Das Bild als `(breite, höhe, BGRA)` für Zellbilder — oder None."""
    if not supported():
        return None
    from . import win_ocr
    try:
        return win_ocr.read_bmp(path)
    except (OSError, ValueError, win_ocr.OcrError):
        return None


def read_image(path, ocr=None, keep_failed=False):
    """Ein Bild lesen: `(aufträge, aufgehoben)` für das Raffinerie-Feld.

    Erst die Suche nach den Tabellen über das ganze Bild (`search_jobs`,
    `find_tables`), dann je Tabelle vergrößerte Lesungen nur ihres
    Ausschnitts (`PASSES`), die `merge_passes` zusammenführt.

    `aufträge` ist eine Liste von dicts je gefundener Tabelle, links nach
    rechts: `text` (Zeilen für das Feld), `unsure` (Rohstoffe ohne sichere
    Zahlen), `materials` (die gelesenen Rohstoffe in Reihenfolge) und `total`
    (die Summe als Text oder ''). Ohne Tabelle: leere Liste.

    `keep_failed`: Bleibt eine Lesung leer oder unsicher, wird ein Bild für
    den Fehlerbericht aufgehoben (`keep`) — der Tabellenausschnitt, wenn die
    Tabelle gefunden wurde, sonst das ganze Bild, verkleinert auf
    `KEEP_MAX_WIDTH`. `aufgehoben` sagt, ob das geschah.
    """
    ocr = ocr or ocr_image
    size = image_size(path)
    if size is None:
        raise ScanError('ocr', 'image size')
    width, height = size
    base = os.path.splitext(path)[0]
    full_png = base + '-bild.png' if keep_failed else None
    temps = [full_png]
    try:
        search = ocr(path, search_jobs(width, height), save=full_png)
        words = merge_words(p.get('words') for p in search.get('passes') or ())
        tables = find_tables(words, width, height)
        if not tables:
            return [], bool(keep_failed and keep(full_png, 'bild'))
        jobs, kept = [], False
        src = _source(path)
        digits = load_digits()
        for number_, table in enumerate(tables):
            crop = table['box']
            crop_png = ('%s-tabelle%d.png' % (base, number_)
                        if keep_failed else None)
            temps.append(crop_png)
            data = ocr(path, [(crop, scale, mode) for scale, mode in PASSES],
                       save_crop=(crop, crop_png) if keep_failed else None)
            passes = [p.get('words') or [] for p in data.get('passes') or ()]
            details = []
            found, unsure = merge_passes(
                passes, confirm=_confirmer(src, passes, digits),
                details=details, guess=True,
                reread=_rereader(src, passes, digits))
            if keep_failed and (unsure or not found):
                kept = keep(crop_png, 'tabelle') or kept
            materials = _materials_in_order(passes)
            jobs.append({'text': as_text(found), 'unsure': unsure,
                         'materials': materials, 'total': table['total'],
                         'state': job_state(words, crop),
                         'cells': unsure_cells(src, passes, details)})
        return jobs, kept
    finally:
        for temp in temps:
            if temp and os.path.isfile(temp):
                try:
                    os.remove(temp)
                except OSError:
                    pass


# Wortteile der Kartentitel (`refinery_ui_WorkOrderCard_Title_*`,
# `refinery_ui_WorkOrderComplete`), deutsch und englisch, Großbuchstaben.
_DONE_WORDS = ('ABGESCHLOSS', 'COMPLETE')
_RUNNING_WORDS = ('VERARBEIT', 'PROCESSING')


def job_state(words, box):
    """Zustand der Karte über der Tafel `box`: `fertig`, `laeuft` oder ''.

    Gezählt werden Wörter, deren waagerechte Mitte innerhalb der Tafel liegt.
    Ein Abschluss-Wort schlägt ein Verarbeitungs-Wort.
    """
    x, _y, w, _h = box
    state = ''
    for word in words or ():
        middle = word.get('x', 0) + word.get('w', 0) / 2.0
        if not x <= middle <= x + w:
            continue
        text = (word.get('t') or '').upper()
        if any(part in text for part in _DONE_WORDS):
            return 'fertig'
        if any(part in text for part in _RUNNING_WORDS):
            state = 'laeuft'
    return state


def _materials_in_order(passes):
    """Die Rohstoffe der Tabelle von oben nach unten, aus der Lesung mit den
    meisten Rohstoffzeilen."""
    best = []
    for words in passes:
        names = [r['material'] for r in material_rows(rows(words))]
        if len(names) > len(best):
            best = names
    return best


def job_label(job, most=2):
    """Beschriftung eines Auftrags: die ersten Rohstoffe und die Summe."""
    from .language import t, current
    seen = []
    for name in job.get('materials') or ():
        if name not in seen:
            seen.append(name)
    names = ', '.join(seen[:most]) or '?'
    if len(seen) > most:
        names += ' …'
    if job.get('total'):
        total = job['total']
        if current() == 'de':
            total = total.replace('.', ',')
        names = t('s_rf_auftrag_summe') % (names, total)
    if job.get('state') == 'fertig':
        return t('s_rf_auftrag_fertig') % names
    if job.get('state') == 'laeuft':
        return t('s_rf_auftrag_laeuft') % names
    return names


def read_screen(rect, ocr=None):
    """Die Spielfläche abgreifen und lesen: `(aufträge, aufgehoben)`.

    Die abgegriffene Datei wird danach gelöscht; aufgehoben wird nur, was
    `read_image(keep_failed=True)` bei einer leeren oder unsicheren Lesung
    ablegt.
    """
    path = grab_game(rect)
    try:
        return read_image(path, ocr, keep_failed=True)
    finally:
        try:
            os.remove(path)
        except OSError:
            pass
