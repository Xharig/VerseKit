# -*- coding: utf-8 -*-
#
# SC BP Watcher — englische global.ini aus dem Data.p4k holen.
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
Zieht die **englische** `global.ini` aus `Data.p4k` — ohne unp4k oder ein anderes
Fremdwerkzeug herunterzuladen.

Warum das geht: `Data.p4k` ist ein ZIP64-Archiv mit CIG-eigenen Extra-Feldern. Pythons
`zipfile` stolpert über die (`BadZipFile: Corrupt extra field c072`), aber das
Inhaltsverzeichnis selbst ist stinknormal und lässt sich von Hand lesen. Wir brauchen
genau **eine** von 1,36 Mio. Dateien, also wird auch nur deren Block gelesen — die
147 GB werden nie angefasst.

Einziger Sonderfall ist die Kompression: CIG nutzt Verfahren **100 = zstd**. Das kann
Pythons Standardbibliothek erst ab 3.14 (`compression.zstd`). Deshalb in dieser
Reihenfolge:

  1. `compression.zstd`  (Python >= 3.14)
  2. `zstandard` / `pyzstd`  (falls installiert)
  3. **7-Zip** ab Version 22 kann zstd und ist auf den meisten Rechnern ohnehin da

Läuft NICHT beim Nutzer — ein Werkzeug zum Nachsehen in der Sprachdatei.

Aufruf:
    python tools/extract_global_ini.py
    python tools/extract_global_ini.py --sprache german_(germany) --out roh.ini
"""
import argparse
import io
import os
import struct
import subprocess
import sys
import tempfile

SC_DIRS = [
    os.environ.get('SC_INSTALL_DIR', ''),
    r'C:\Program Files\Roberts Space Industries\StarCitizen\LIVE',
    r'C:\Program Files (x86)\Roberts Space Industries\StarCitizen\LIVE',
]
SIEBENZIP = [
    os.environ.get('SEVENZIP', ''),
    r'C:\Program Files\7-Zip\7z.exe',
    r'C:\Program Files (x86)\7-Zip\7z.exe',
]
ZSTD_METHODE = 100          # CIGs Kennung für zstd im ZIP-Kopf


def finde_p4k():
    for basis in SC_DIRS:
        if not basis:
            continue
        p = os.path.join(basis, 'Data.p4k')
        if os.path.exists(p):
            return p
    return None


def lies_verzeichnis(f, groesse):
    """Gibt den rohen Block des zentralen Inhaltsverzeichnisses zurück."""
    f.seek(max(0, groesse - 66000))
    ende = f.read()
    pos = ende.rfind(b'PK\x06\x07')                      # ZIP64-Locator
    if pos < 0:
        raise RuntimeError('Kein ZIP64-Locator gefunden — unerwartetes Archivformat.')
    _, _, z64_off, _ = struct.unpack('<IIQI', ende[pos:pos + 20])
    f.seek(z64_off)
    kopf = f.read(56)
    if kopf[:4] != b'PK\x06\x06':
        raise RuntimeError('EOCD64-Signatur fehlt.')
    *_, anzahl, _, cd_size, cd_off = struct.unpack('<IQHHIIQQQQ', kopf)
    f.seek(cd_off)
    return f.read(cd_size), anzahl


def suche(cd, zielname):
    """Findet den Eintrag und löst die ZIP64-Platzhalter im Extra-Feld auf.
    Gibt (methode, komprimierte_groesse, rohgroesse, lokaler_offset) zurück."""
    ziel = zielname.lower().replace('/', '\\')
    i = 0
    while i < len(cd) - 46:
        if cd[i:i + 4] != b'PK\x01\x02':
            i += 1
            continue
        (_, _, _, _, methode, _, _, _, cs, rs,
         n_len, e_len, k_len, _, _, _, off) = struct.unpack('<IHHHHHHIIIHHHHHII', cd[i:i + 46])
        name = cd[i + 46:i + 46 + n_len].decode('utf-8', 'replace')
        if name.lower().replace('/', '\\') == ziel:
            extra = cd[i + 46 + n_len:i + 46 + n_len + e_len]
            # ZIP64-Feld (0x0001): die überlaufenen Werte stehen dort der Reihe nach
            j = 0
            while j < len(extra) - 4:
                hid, hlen = struct.unpack('<HH', extra[j:j + 4])
                if hid == 0x0001:
                    daten, k = extra[j + 4:j + 4 + hlen], 0
                    for feld in ('rs', 'cs', 'off'):
                        grenze = {'rs': rs, 'cs': cs, 'off': off}[feld]
                        if grenze == 0xFFFFFFFF and k + 8 <= len(daten):
                            wert = struct.unpack('<Q', daten[k:k + 8])[0]
                            k += 8
                            if feld == 'rs':   rs = wert
                            elif feld == 'cs': cs = wert
                            else:              off = wert
                    break
                j += 4 + hlen
            return methode, cs, rs, off
        i += 46 + n_len + e_len + k_len
    return None


def hole_block(f, lokal_off, cs):
    """Liest die komprimierten Bytes. Der lokale Kopf hat eigene Namens- und
    Extra-Längen — die echten Daten beginnen erst dahinter."""
    f.seek(lokal_off)
    kopf = f.read(30)
    if kopf[:4] != b'PK\x03\x04':
        raise RuntimeError('Lokaler Dateikopf fehlt bei Offset %d.' % lokal_off)
    n_len, e_len = struct.unpack('<HH', kopf[26:30])
    f.seek(lokal_off + 30 + n_len + e_len)
    return f.read(cs)


def entpacke_zstd(roh, erwartet):
    """Versucht der Reihe nach: Standardbibliothek, Fremdmodul, 7-Zip."""
    try:
        from compression import zstd as _z          # Python >= 3.14
        return _z.decompress(roh), 'compression.zstd'
    except Exception:
        pass
    for modul in ('zstandard', 'pyzstd'):
        try:
            m = __import__(modul)
            if modul == 'zstandard':
                return m.ZstdDecompressor().decompress(
                    roh, max_output_size=max(erwartet, 1) * 2), modul
            return m.decompress(roh), modul
        except Exception:
            continue
    for exe in SIEBENZIP:
        if exe and os.path.exists(exe):
            tmp = tempfile.mkdtemp(prefix='p4k-')
            quelle = os.path.join(tmp, 'block.zst')
            with io.open(quelle, 'wb') as g:
                g.write(roh)
            # 7-Zip legt die entpackte Datei ohne Endung daneben
            r = subprocess.run([exe, 'e', quelle, '-o' + tmp, '-y'],
                               stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            ziel = os.path.join(tmp, 'block')
            if r.returncode == 0 and os.path.exists(ziel):
                with io.open(ziel, 'rb') as g:
                    return g.read(), '7-Zip (%s)' % os.path.basename(exe)
            raise RuntimeError('7-Zip scheiterte:\n%s' % r.stdout.decode('utf-8', 'replace')[:400])
    raise RuntimeError(
        'Kein zstd-Entpacker gefunden. Möglichkeiten:\n'
        '  * Python 3.14 oder neuer (bringt compression.zstd mit)\n'
        '  * pip install zstandard\n'
        '  * 7-Zip ab Version 22 installieren (Pfad notfalls über SEVENZIP setzen)')


def main():
    ap = argparse.ArgumentParser(description='Holt die global.ini aus Data.p4k.')
    ap.add_argument('--sprache', default='english', help='Sprachordner im Archiv (Standard: english)')
    ap.add_argument('--p4k', help='Pfad zur Data.p4k')
    ap.add_argument('--out', default='global.ini', help='Zieldatei')
    args = ap.parse_args()

    p4k = args.p4k or finde_p4k()
    if not p4k or not os.path.exists(p4k):
        print('FEHLER: Data.p4k nicht gefunden. --p4k angeben oder SC_INSTALL_DIR setzen.')
        return 2

    zielname = 'Data\\Localization\\%s\\global.ini' % args.sprache
    print('Archiv: %s (%.1f GB)' % (p4k, os.path.getsize(p4k) / 1024.0 ** 3))
    print('Suche:  %s' % zielname)

    with io.open(p4k, 'rb') as f:
        cd, anzahl = lies_verzeichnis(f, os.path.getsize(p4k))
        print('  Inhaltsverzeichnis: %d Einträge (%.0f MB)' % (anzahl, len(cd) / 1024.0 ** 2))
        treffer = suche(cd, zielname)
        if not treffer:
            print('FEHLER: %s steckt nicht im Archiv.' % zielname)
            return 3
        methode, cs, rs, off = treffer
        print('  gefunden: %.1f KB komprimiert -> %.1f KB roh (Verfahren %d)'
              % (cs / 1024.0, rs / 1024.0, methode))
        roh = hole_block(f, off, cs)

    if methode == 0:
        daten, wie = roh, 'unkomprimiert'
    elif methode == ZSTD_METHODE:
        daten, wie = entpacke_zstd(roh, rs)
    else:
        print('FEHLER: unbekanntes Kompressionsverfahren %d.' % methode)
        return 4

    if rs and len(daten) != rs:
        print('WARNUNG: %d Bytes entpackt, erwartet waren %d.' % (len(daten), rs))

    with io.open(args.out, 'wb') as g:
        g.write(daten)
    print('  entpackt mit: %s' % wie)
    print('Geschrieben: %s (%.1f KB)' % (os.path.abspath(args.out), len(daten) / 1024.0))
    return 0


if __name__ == '__main__':
    sys.exit(main())
