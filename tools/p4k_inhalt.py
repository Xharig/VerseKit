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
p4k_inhalt.py — Bestandsaufnahme von Star Citizens `Data.p4k`.

**Wozu.** Bisher wurde aus dem Archiv genau **eine** Datei geholt (die englische
`global.ini`, siehe `extract_global_ini.py`). Die Frage, was dort sonst noch
liegt und ob sich daraus Funktionen bauen lassen, war nie beantwortet — man
hätte raten müssen. Dieses Werkzeug beantwortet sie mit Zahlen.

**Es liest NUR das Inhaltsverzeichnis.** Die rund 147 GB Nutzdaten werden nie
angefasst; gelesen werden das Ende der Datei und der Verzeichnisblock (bei 4.9
rund 442 MB, 1,36 Mio. Einträge). Nichts wird entpackt, nichts geschrieben.

Ausgabe:
  * die obersten Ordner mit Anzahl und Rohgröße
  * die häufigsten Dateiendungen
  * gezielte Stichproben (`--suche <teilwort>`), um zu sehen, ob etwas
    Brauchbares existiert, bevor man es entpackt

Aufruf:
    python3 tools/p4k_inhalt.py
    python3 tools/p4k_inhalt.py --suche mission
    python3 tools/p4k_inhalt.py --tiefe 2 --top 40
"""
import argparse
import os
import struct
import sys
from collections import Counter, defaultdict

# ⛔ Vor der ersten Ausgabe: Die Windows-Konsole kann kein `⚠` — siehe ausgabe.py.
import ausgabe                                                 # noqa: E402
ausgabe.utf8()

# Dieselbe Suche wie in extract_global_ini.py. Wer das Spiel woanders liegen
# hat — zweites System, eingehaengte Fremdplatte — setzt SC_INSTALL_DIR.
# ⚠ Hier stehen KEINE persoenlichen Pfade: Das Repo ist oeffentlich.
SC_DIRS = [
    os.environ.get('SC_INSTALL_DIR'),
    os.path.expanduser('~/Games/star-citizen/drive_c/Program Files/'
                       'Roberts Space Industries/StarCitizen/LIVE'),
    r'C:\Program Files\Roberts Space Industries\StarCitizen\LIVE',
]


def finde_p4k():
    for basis in SC_DIRS:
        if not basis:
            continue
        p = os.path.join(basis, 'Data.p4k')
        if os.path.exists(p):
            return p
    return None


def lies_verzeichnis(f, groesse):
    """Der rohe Block des zentralen Inhaltsverzeichnisses.

    Das klassische EOCD traegt bei einem so grossen Archiv nur
    0xFFFFFFFF-Platzhalter — die echten Werte stehen im ZIP64-Kopf, den der
    Locator `PK\\x06\\x07` am Dateiende ausweist.
    """
    f.seek(max(0, groesse - 66000))
    ende = f.read()
    pos = ende.rfind(b'PK\x06\x07')
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


def eintraege(cd):
    """Laeuft das Verzeichnis durch und liefert (name, rohgroesse, methode).

    ⚠ Rohgroesse und Offset koennen auf 0xFFFFFFFF stehen; die echten 64-Bit-Werte
    liegen dann im Extra-Feld 0x0001, und zwar NUR fuer die uebergelaufenen Felder
    und in fester Reihenfolge (Rohgroesse, Komprimiertgroesse, Offset). Wer das
    ueberspringt, bekommt bei den grossen Dateien Unsinn heraus.
    """
    i, n = 0, len(cd)
    while i < n - 46:
        if cd[i:i + 4] != b'PK\x01\x02':
            i += 1
            continue
        (_, _, _, _, methode, _, _, _, cs, rs,
         n_len, e_len, k_len, _, _, _, off) = struct.unpack('<IHHHHHHIIIHHHHHII', cd[i:i + 46])
        name = cd[i + 46:i + 46 + n_len].decode('utf-8', 'replace')
        if rs == 0xFFFFFFFF or cs == 0xFFFFFFFF or off == 0xFFFFFFFF:
            extra = cd[i + 46 + n_len:i + 46 + n_len + e_len]
            j = 0
            while j < len(extra) - 4:
                hid, hlen = struct.unpack('<HH', extra[j:j + 4])
                if hid == 0x0001:
                    daten, k = extra[j + 4:j + 4 + hlen], 0
                    for feld, grenze in (('rs', rs), ('cs', cs), ('off', off)):
                        if grenze == 0xFFFFFFFF and k + 8 <= len(daten):
                            wert = struct.unpack('<Q', daten[k:k + 8])[0]
                            k += 8
                            if feld == 'rs':
                                rs = wert
                            elif feld == 'cs':
                                cs = wert
                    break
                j += 4 + hlen
        yield name, rs, methode
        i += 46 + n_len + e_len + k_len


def gr(b):
    """Groesse lesbar — die Zahlen gehen bis in den Terabyte-Bereich."""
    for e in ('B', 'KB', 'MB', 'GB', 'TB'):
        if b < 1024 or e == 'TB':
            return '%.1f %s' % (b, e)
        b /= 1024.0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--tiefe', type=int, default=1,
                    help='wie viele Ordnerebenen zusammengefasst werden (Standard 1)')
    ap.add_argument('--top', type=int, default=30, help='wie viele Zeilen je Liste')
    ap.add_argument('--suche', action='append', default=[],
                    help='Teilwort im Pfad; mehrfach angebbar')
    a = ap.parse_args()

    p4k = finde_p4k()
    if not p4k:
        print('Data.p4k nicht gefunden. Pfad ueber SC_INSTALL_DIR setzen.')
        return 1
    groesse = os.path.getsize(p4k)
    print('Archiv:  %s' % p4k)
    print('Groesse: %s' % gr(groesse))

    with open(p4k, 'rb') as f:
        cd, anzahl = lies_verzeichnis(f, groesse)
    print('Verzeichnis: %s, %d Eintraege laut Kopf' % (gr(len(cd)), anzahl))
    print()

    ordner_n, ordner_b = Counter(), Counter()
    endung_n, endung_b = Counter(), Counter()
    treffer = defaultdict(list)
    gesamt_n = gesamt_b = 0

    for name, rs, _methode in eintraege(cd):
        gesamt_n += 1
        gesamt_b += rs
        teile = name.replace('\\', '/').split('/')
        ordner_n['/'.join(teile[:a.tiefe]) if len(teile) > a.tiefe else '(Wurzel)'] += 1
        ordner_b['/'.join(teile[:a.tiefe]) if len(teile) > a.tiefe else '(Wurzel)'] += rs
        endung = os.path.splitext(teile[-1])[1].lower() or '(ohne)'
        endung_n[endung] += 1
        endung_b[endung] += rs
        if a.suche:
            klein = name.lower()
            for wort in a.suche:
                if wort.lower() in klein and len(treffer[wort]) < a.top:
                    treffer[wort].append((name, rs))

    print('Gelesen: %d Eintraege, %s roh' % (gesamt_n, gr(gesamt_b)))
    print()
    print('=== Oberste Ordner (nach Anzahl) ===')
    for k, v in ordner_n.most_common(a.top):
        print('  %-42s %8d  %10s' % (k[:42], v, gr(ordner_b[k])))
    print()
    print('=== Haeufigste Endungen ===')
    for k, v in endung_n.most_common(a.top):
        print('  %-14s %8d  %10s' % (k, v, gr(endung_b[k])))

    for wort in a.suche:
        print()
        print('=== Suche: %s (erste %d) ===' % (wort, a.top))
        if not treffer[wort]:
            print('  nichts gefunden')
        for name, rs in treffer[wort]:
            print('  %-80s %10s' % (name[:80], gr(rs)))
    return 0


if __name__ == '__main__':
    sys.exit(main())
