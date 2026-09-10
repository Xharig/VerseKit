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
Eine Sperrdatei mit Prüfsummen erzeugen — für den Bau, nicht für das Programm.

**Wozu.** Der Bau nagelt seine Werkzeuge namentlich fest (PyInstaller,
pyflakes, pip). Was `pip` **dazu** holt, war offen: `altgraph`, `packaging`,
`setuptools`, `pyinstaller-hooks-contrib`, unter Windows zusätzlich `pefile`
und `pywin32-ctypes`. Ein neues `setuptools` konnte den Bau also weiter
verändern, ohne dass irgendwo etwas geändert wurde.

**Was hier herauskommt.** Eine Datei im `requirements`-Format, in der **jedes**
Paket mit Version und SHA-256 steht. Damit lässt sich der Bau mit
`pip install --require-hashes -r <datei>` festnageln: Passt eine Summe nicht,
bricht er ab, statt still etwas anderes einzubauen.

⚠⚠ **Eine Sperrdatei gilt nur für die Plattform, auf der sie entstand.**
Am 10.09.2026 gemessen: `pip download --platform win_amd64` löst die
Windows-Pakete **nicht** auf — `--platform` steuert nur, welche Wheels als
verträglich gelten, die Marker `sys_platform == "win32"` kommen dagegen vom
**laufenden** System. Ergebnis eines anderswo erzeugten Locks: `pefile` und
`pywin32-ctypes` fehlen, dafür ist `macholib` drin. Der Windows-Bau wäre damit
mit `--require-hashes` abgebrochen.

**Deshalb entsteht jede Sperrdatei dort, wo sie gilt** — und dafür braucht es
keinen zweiten Rechner: Der Bau-Ablauf `sperrdateien.yml` lässt dieses Skript
auf `windows-latest` **und** `ubuntu-latest` laufen und hängt beide Ergebnisse
als Anhang an den Lauf.

**Aufruf**

    python .github/scripts/sperrdatei_bauen.py ZIEL.txt PAKET==VERSION [...]

Reine Standardbibliothek plus `pip` — es wird nichts installiert, was nicht
ohnehin da wäre.
"""
import hashlib
import os
import re
import shutil
import subprocess
import sys
import tempfile


def _normalisiert(name):
    """Paketnamen so schreiben, wie `pip` sie vergleicht (PEP 503)."""
    return re.sub(r'[-_.]+', '-', name).lower()


def _name_und_version(dateiname):
    """Aus einem Paket-Dateinamen (Name, Version) holen — oder None.

    Wheels heissen `name-version-python-abi-plattform.whl`, Quellpakete
    `name-version.tar.gz`. Der Name darf selbst Bindestriche enthalten,
    deshalb wird beim Quellpaket von **hinten** getrennt.
    """
    if dateiname.endswith('.whl'):
        teile = dateiname[:-4].split('-')
        if len(teile) < 2:
            return None
        return _normalisiert(teile[0]), teile[1]
    for endung in ('.tar.gz', '.tar.bz2', '.zip'):
        if dateiname.endswith(endung):
            stamm = dateiname[:-len(endung)]
            name, _, version = stamm.rpartition('-')
            if not name or not version:
                return None
            return _normalisiert(name), version
    return None


def _summe(pfad):
    h = hashlib.sha256()
    with open(pfad, 'rb') as f:
        for block in iter(lambda: f.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def sammeln(pakete, ordner):
    """Die Pakete samt allem, was sie brauchen, herunterladen.

    ⚠ **Kein `--no-deps`.** Genau die Unterabhängigkeiten sind der Zweck
    dieser Datei — wer sie weglässt, nagelt wieder nur fest, was ohnehin schon
    namentlich dastand.
    """
    befehl = [sys.executable, '-m', 'pip', 'download',
              '--dest', ordner, '--disable-pip-version-check'] + list(pakete)
    print('$ ' + ' '.join(befehl), flush=True)
    subprocess.check_call(befehl)


def eintraege(ordner):
    """`{(name, version): [summen]}` aus einem Ordner voller Pakete."""
    gefunden = {}
    for datei in sorted(os.listdir(ordner)):
        teile = _name_und_version(datei)
        if teile is None:
            print('  übersprungen (unbekannte Form): %s' % datei)
            continue
        gefunden.setdefault(teile, []).append(_summe(os.path.join(ordner, datei)))
    return gefunden


def schreiben(ziel, gefunden, pakete):
    """Die Sperrdatei ablegen — mit Kopf, damit sie sich selbst erklärt."""
    zeilen = [
        '# Sperrdatei fuer den Bau — MASCHINELL ERZEUGT, nicht von Hand aendern.',
        '#',
        '# Erzeugt von .github/scripts/sperrdatei_bauen.py auf der Plattform,',
        '# fuer die sie gilt (%s, Python %d.%d).'
        % (sys.platform, sys.version_info[0], sys.version_info[1]),
        '#',
        '# Angefordert: %s' % ', '.join(pakete),
        '#',
        '# ⚠ Eine hier fehlende Unterabhaengigkeit bricht den Bau mit',
        '#   --require-hashes ab. Das ist gewollt: laut ist besser als still.',
        '#',
        '# ⚠ Nach jedem Anheben eines Werkzeugs neu erzeugen — Ablauf',
        '#   "Sperrdateien bauen" von Hand starten und die Anhaenge einchecken.',
        '',
    ]
    for (name, version) in sorted(gefunden):
        summen = sorted(set(gefunden[(name, version)]))
        zeilen.append('%s==%s \\' % (name, version))
        for i, summe in enumerate(summen):
            ende = '' if i == len(summen) - 1 else ' \\'
            zeilen.append('    --hash=sha256:%s%s' % (summe, ende))
    with open(ziel, 'w', encoding='utf-8', newline='\n') as f:
        f.write('\n'.join(zeilen) + '\n')


def main(argv):
    if len(argv) < 3:
        print(__doc__)
        return 2
    ziel, pakete = argv[1], argv[2:]
    ordner = tempfile.mkdtemp(prefix='sperrdatei-')
    try:
        sammeln(pakete, ordner)
        gefunden = eintraege(ordner)
        if not gefunden:
            print('FEHLER: nichts heruntergeladen — so entstuende eine leere '
                  'Sperrdatei, und die wuerde jeden Bau durchwinken.')
            return 1
        schreiben(ziel, gefunden, pakete)
    finally:
        shutil.rmtree(ordner, ignore_errors=True)

    print('\n%d Pakete in %s:' % (len(gefunden), ziel))
    for (name, version) in sorted(gefunden):
        print('  %-28s %s' % (name, version))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
