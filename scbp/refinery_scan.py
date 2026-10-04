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
(`Windows.Media.Ocr` über `powershell.exe`, ohne Fenster), aus den
erkannten Wörtern die Tabellenzeilen zusammensetzen und daraus Zeilen im
Format von `materials.refinery_lines` bauen.

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
mindestens zwei Lesungen ihn gleich lasen (`merge_passes`). Was nicht
sicher ist, wird gemeldet statt geraten.
"""
import base64
import difflib
import json
import os
import re
import subprocess
import sys
import tempfile
import time

_CREATE_NO_WINDOW = 0x08000000

# Liest das Bild aus `VK_OCR_IMAGE` (wahlweise nur den Ausschnitt
# `VK_OCR_CROP` = x,y,b,h) einmal je Vergrößerung aus `VK_OCR_SCALES` und
# schreibt je Durchgang die Wörter mit Rahmen als JSON — in Bildpunkten des
# Originals. Nimmt die Sprache des Benutzerprofils, sonst die erste
# installierte.
_PS_SCRIPT = r'''
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
try {
Add-Type -AssemblyName System.Runtime.WindowsRuntime
$null = [Windows.Media.Ocr.OcrEngine, Windows.Foundation, ContentType = WindowsRuntime]
$null = [Windows.Graphics.Imaging.BitmapDecoder, Windows.Graphics, ContentType = WindowsRuntime]
$asTask = [System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {
    $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and
    $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1' } | Select-Object -First 1
function Await($op, [Type]$type) {
    $task = $asTask.MakeGenericMethod($type).Invoke($null, @($op))
    $null = $task.Wait(-1)
    $task.Result
}
$langs = @([Windows.Media.Ocr.OcrEngine]::AvailableRecognizerLanguages | ForEach-Object { $_.LanguageTag })
$engine = [Windows.Media.Ocr.OcrEngine]::TryCreateFromUserProfileLanguages()
if ($null -eq $engine -and $langs.Count) {
    $engine = [Windows.Media.Ocr.OcrEngine]::TryCreateFromLanguage([Windows.Globalization.Language]::new($langs[0]))
}
if ($null -eq $engine) { Write-Output '{"error":"no_engine"}'; exit 0 }
Add-Type -AssemblyName System.Drawing
$source = [System.Drawing.Bitmap]::new($env:VK_OCR_IMAGE)
$fw = [int]$source.Width; $fh = [int]$source.Height
$cx = 0; $cy = 0; $cw = $fw; $ch = $fh
if ($env:VK_OCR_CROP) {
    $c = $env:VK_OCR_CROP.Split(',') | ForEach-Object { [int]$_ }
    $cx = [Math]::Max(0, $c[0]); $cy = [Math]::Max(0, $c[1])
    $cw = [Math]::Min($c[2], $source.Width - $cx); $ch = [Math]::Min($c[3], $source.Height - $cy)
}
$limit = [Windows.Media.Ocr.OcrEngine]::MaxImageDimension
$passes = foreach ($wanted in $env:VK_OCR_PASSES.Split(',')) {
    $spec = $wanted.Split(':')
    $scale = [double]::Parse($spec[0], [Globalization.CultureInfo]::InvariantCulture)
    $mode = if ($spec.Count -gt 1) { $spec[1] } else { 'plain' }
    if ($scale -le 0) { $scale = 1.0 }
    if ([Math]::Max($cw, $ch) * $scale -gt $limit) { $scale = $limit / [Math]::Max($cw, $ch) }
    $tw = [int][Math]::Floor($cw * $scale); $th = [int][Math]::Floor($ch * $scale)
    $target = [System.Drawing.Bitmap]::new($tw, $th, [System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
    $g = [System.Drawing.Graphics]::FromImage($target)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $attr = [System.Drawing.Imaging.ImageAttributes]::new()
    if ($mode -eq 'gray') {
        # grayscale, stretched contrast; light text stays light
        $k = 1.8
        $m = [System.Drawing.Imaging.ColorMatrix]::new()
        foreach ($o in 0, 1, 2) {
            $m.Item(0, $o) = 0.30 * $k; $m.Item(1, $o) = 0.59 * $k
            $m.Item(2, $o) = 0.11 * $k; $m.Item(4, $o) = -0.25
        }
        $m.Item(3, 3) = 1.0; $m.Item(4, 4) = 1.0
        $attr.SetColorMatrix($m)
    }
    $g.DrawImage($source, [System.Drawing.Rectangle]::new(0, 0, $tw, $th), $cx, $cy, $cw, $ch,
                 [System.Drawing.GraphicsUnit]::Pixel, $attr)
    $g.Dispose()
    $memory = [System.IO.MemoryStream]::new()
    $target.Save($memory, [System.Drawing.Imaging.ImageFormat]::Bmp)
    $target.Dispose()
    $memory.Position = 0
    $stream = [System.IO.WindowsRuntimeStreamExtensions]::AsRandomAccessStream($memory)
    $decoder = Await ([Windows.Graphics.Imaging.BitmapDecoder]::CreateAsync($stream)) ([Windows.Graphics.Imaging.BitmapDecoder])
    $bitmap = Await ($decoder.GetSoftwareBitmapAsync()) ([Windows.Graphics.Imaging.SoftwareBitmap])
    $result = Await ($engine.RecognizeAsync($bitmap)) ([Windows.Media.Ocr.OcrResult])
    $memory.Dispose()
    $words = foreach ($line in $result.Lines) {
        foreach ($w in $line.Words) {
            $r = $w.BoundingRect
            @{ t = $w.Text; x = [int]($cx + $r.X / $scale); y = [int]($cy + $r.Y / $scale);
               w = [int]($r.Width / $scale); h = [int]($r.Height / $scale) }
        }
    }
    @{ scale = $scale; mode = $mode; words = @($words) }
}
$source.Dispose()
@{ width = $fw; height = $fh;
   lang = $engine.RecognizerLanguage.LanguageTag; passes = @($passes) } | ConvertTo-Json -Depth 5 -Compress
} catch {
    @{ error = 'failed'; detail = $_.Exception.Message } | ConvertTo-Json -Compress
}
'''


class ScanError(Exception):
    """Lesen nicht möglich — `reason` ist ein Kennwort für die Oberfläche."""

    def __init__(self, reason, detail=''):
        Exception.__init__(self, reason)
        self.reason = reason
        self.detail = detail


def supported():
    """Gibt es auf diesem System eine Texterkennung, die wir nutzen?"""
    return sys.platform == 'win32'


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
def _powershell():
    root = os.environ.get('SystemRoot') or r'C:\Windows'
    path = os.path.join(root, 'System32', 'WindowsPowerShell', 'v1.0',
                        'powershell.exe')
    return path if os.path.isfile(path) else 'powershell.exe'


def ocr_image(path, passes=((1.0, 'plain'),), crop=None, timeout=90):
    """Ein Bild mit der Windows-Texterkennung lesen. Gibt das Ergebnis-dict
    `{'width', 'height', 'lang', 'passes': [{'scale', 'mode', 'words'}, …]}`.

    Startet `powershell.exe` (Windows PowerShell 5.1 kennt die
    WinRT-Typen, PowerShell 7 nicht) ohne Fenster. Je Eintrag in `passes`
    ein Durchgang `(vergrößerung, art)` — Art `plain` (unverändert) oder
    `gray` (Graustufe mit gespreiztem Kontrast). Die
    Vergrößerung endet bei `OcrEngine.MaxImageDimension`. `crop` =
    (x, y, b, h) liest nur diesen Ausschnitt; die Wortrahmen stehen immer in
    Bildpunkten des Originals. Das Bild muss BMP, PNG oder JPEG sein.
    """
    if not supported():
        raise ScanError('nicht_unterstuetzt')
    encoded = base64.b64encode(_PS_SCRIPT.encode('utf-16-le')).decode('ascii')
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    env = dict(os.environ, VK_OCR_IMAGE=os.path.abspath(path),
               VK_OCR_PASSES=','.join('%.3f:%s' % (float(s), m)
                                      for s, m in passes),
               VK_OCR_CROP=','.join(str(int(v)) for v in crop) if crop else '')
    try:
        done = subprocess.run(
            [_powershell(), '-NoProfile', '-NonInteractive',
             '-ExecutionPolicy', 'Bypass', '-EncodedCommand', encoded],
            capture_output=True, env=env, timeout=timeout,
            creationflags=_CREATE_NO_WINDOW, startupinfo=startup)
    except (OSError, subprocess.SubprocessError) as error:
        raise ScanError('ocr', str(error))
    text = done.stdout.decode('utf-8', 'replace').strip()
    try:
        data = json.loads(text[text.find('{'):]) if '{' in text else {}
    except ValueError:
        data = {}
    if data.get('error') == 'no_engine':
        raise ScanError('keine_sprache')
    if isinstance(data.get('passes'), dict):
        data['passes'] = [data['passes']]
    if data.get('error') or not isinstance(data.get('passes'), list):
        raise ScanError('ocr', data.get('detail') or
                        done.stderr.decode('utf-8', 'replace')[:300])
    return data


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


def number(token):
    """Eine Ganzzahl aus einem OCR-Wort — oder None.

    Verwechselte Zeichen (O/0, l/1, S/5) werden nur in Wörtern getauscht,
    die mindestens eine echte Ziffer enthalten. Tausenderpunkte und
    -kommas fallen weg.
    """
    text = (token or '').strip().strip('.,:;')
    if not text or not any(c.isdigit() for c in text):
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
    `(x_mitte, wert)`.
    """
    index = _material_index() if index is None else index
    out = []
    for words in table:
        words = [w for w in words if (w.get('t') or '').strip()]
        name_parts = []
        for word in words:
            if number(word['t']) is not None:
                break
            name_parts.append(word['t'].strip())
        if not name_parts or len(name_parts) > 5:
            continue
        material = match_material(' '.join(name_parts), index)
        if material is None:
            continue
        numbers = []
        for word in words[len(name_parts):]:
            value = number(word['t'])
            if value is not None:
                numbers.append((word.get('x', 0) + word.get('w', 0) / 2.0,
                                value))
        top = min(w.get('y', 0) for w in words)
        bottom = max(w.get('y', 0) + w.get('h', 0) for w in words)
        out.append({'material': material, 'y': (top + bottom) / 2.0,
                    'height': max(1, bottom - top),
                    'raw': ' '.join(w['t'].strip() for w in words),
                    'numbers': numbers})
    return out


def columns(found_rows):
    """Die Zahlenspalten als x-Mitten, links nach rechts — ohne Überschrift.

    Alle Zahlen aller Rohstoffzeilen werden nach x geordnet; eine Lücke von
    mehr als drei Zeilenhöhen beginnt eine neue Spalte.
    """
    xs = sorted(x for row in found_rows for x, _v in row['numbers'])
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
    result = []
    for row in found_rows:
        slots = {}
        for x, value in row['numbers']:
            nearest = min(range(len(centers)), key=lambda i: abs(centers[i] - x))
            slots.setdefault(nearest, value)
        quality, amount = slots.get(0), slots.get(1)
        if quality is not None and not 0 <= quality <= 1000:
            quality = None
        if amount is not None and amount <= 0:
            amount = None
        result.append({'material': row['material'], 'quality': quality,
                       'amount': amount, 'y': row['y'],
                       'height': row['height'], 'raw': row['raw']})
    return result


# So viele Durchgänge müssen einen Wert gleich gelesen haben, damit er gilt.
MIN_VOTES = 2


def _vote(values, minimum):
    """Der Wert, den die meisten Durchgänge lasen — oder None.

    None, wenn er seltener als `minimum` vorkommt oder ein zweiter Wert
    gleich oft gelesen wurde.
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
    return ranked[0][0]


def merge_passes(passes, index=None, minimum=None):
    """Mehrere Lesungen desselben Bildes zu einer Ausbeute zusammenführen.

    Zeilen verschiedener Durchgänge gehören zusammen, wenn ihre senkrechte
    Mitte weniger als eine halbe Zeilenhöhe auseinanderliegt. Rohstoff,
    Qualität und Menge werden je für sich abgestimmt (`_vote`): Ein Wert
    gilt erst, wenn mindestens `minimum` Durchgänge ihn gleich lasen.

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
        quality = _vote([r['quality'] for r in cluster['rows']], minimum)
        amount = _vote([r['amount'] for r in cluster['rows']], minimum)
        if material is None:
            continue
        if quality is None or amount is None:
            unsure.append(material)
            continue
        found.append((material, quality, amount))
    return found, unsure


def as_text(found):
    """Die Zeilen im Format des Raffinerie-Felds (`Material Q Menge cSCU`)."""
    return '\n'.join('%s %d %d cSCU' % (name, quality, amount)
                     for name, quality, amount in found)


# Die Durchgänge über den Tabellenausschnitt. Die Ziffern im Terminal sind
# auf einem 1440er-Bildschirm nur rund acht Bildpunkte hoch; erst vergrößert
# liest die OCR sie, und jeder Durchgang verliest andere Ziffern.
PASSES = ((2.0, 'plain'), (3.0, 'plain'), (4.0, 'plain'), (5.0, 'plain'),
          (3.0, 'gray'))
# Anker für den Ausschnitt: die Spaltenüberschrift der Ausbeute-Tabelle.
_ANCHORS = ('materialien', 'materials', 'gewonnene', 'refined', 'cscu')


def table_crop(words, width, height):
    """Den Ausschnitt um die Ausbeute-Tabelle — oder None.

    Gesucht wird die Überschrift (`GEWONNENE MATERIALIEN (cSCU)` bzw.
    englisch); der Ausschnitt reicht von dort nach unten und nach rechts
    über die Zahlenspalten.
    """
    hits = [w for w in words or ()
            if any(a in (w.get('t') or '').lower() for a in _ANCHORS)]
    if not hits:
        return None
    anchor = min(hits, key=lambda w: w.get('y', 0))
    unit = max(8, anchor.get('h', 8))
    left = max(0, anchor.get('x', 0) - 4 * unit)
    top = max(0, anchor.get('y', 0) - unit)
    right = min(width, anchor.get('x', 0) + 45 * unit)
    bottom = min(height, anchor.get('y', 0) + 60 * unit)
    return left, top, right - left, bottom - top


def read_image(path, ocr=None):
    """Ein Bild lesen: `(text, unsicher)` für das Raffinerie-Feld.

    Erst eine Lesung in Originalgröße, um die Tabelle zu finden; dann
    vergrößerte Lesungen nur dieses Ausschnitts (`PASSES`), die
    `merge_passes` zusammenführt. Ohne Tabelle auf dem Bild: `('', [])`.
    """
    ocr = ocr or ocr_image
    first = ocr(path, passes=((1.0, 'plain'),))
    words = (first.get('passes') or [{}])[0].get('words') or []
    crop = table_crop(words, first.get('width') or 0, first.get('height') or 0)
    if crop is None:
        return '', []
    data = ocr(path, passes=PASSES, crop=crop)
    passes = [p.get('words') or [] for p in data.get('passes') or ()]
    found, unsure = merge_passes(passes)
    return as_text(found), unsure


def read_screen(rect, ocr=None):
    """Die Spielfläche abgreifen und lesen. Die Bilddatei wird danach gelöscht."""
    path = grab_game(rect)
    try:
        return read_image(path, ocr)
    finally:
        try:
            os.remove(path)
        except OSError:
            pass
