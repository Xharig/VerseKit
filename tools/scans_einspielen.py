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
Eingesandte Scan-Bilder in die mitgelieferten Ziffernvorlagen einspielen.

Ein Fehlerbericht mit Scan-Bildern bringt eine ZIP `scan-bilder-<name>.zip`
mit `bilder/<wert>_<zeit>.png`. Der Dateiname trägt die Zahl, die der Spieler
beim Anlernen getippt hat. Dieses Werkzeug misst, was die Beilage
(`daten/signatur-ziffern.json`) daraus heute liest, lernt die Bilder an und
übernimmt die neuen Muster nur, wenn dabei nichts falsch gelesen wird.

    python3 tools/scans_einspielen.py --bergbau <mining-data.json> <zip|ordner> ...
        Misst nur und schreibt nichts.

    python3 tools/scans_einspielen.py --bergbau <…> --uebernehmen <zip|ordner> ...
        Schreibt die Beilage und den Prüfsatz `tools/pruefdaten/
        signatur-eingesandt.json` — nur, wenn alle drei Messungen sauber sind.

`--ohne <dateiname>` nimmt ein Bild heraus (z. B. falsch beschriftet).

**Drei Messungen**, alle in einem Wegwerf-Ordner (`SC_BP_HOME`):

1. *ab Werk*: jedes Bild nur mit der Beilage.
2. *kreuzweise*: dreigeteilt — mit zwei Dritteln anlernen, das dritte lesen.
3. *alte Prüfsätze*: `signatur-windows-klein.json` und `-gross.json` mit der
   neuen Beilage gegen die alte. Weniger richtig oder ein Fehllesen → abgelehnt.

⚠ **Bilder mit Werten außerhalb der Bergbaudaten werden nicht angelernt**,
sondern zur Sichtprüfung gelistet: Der Scanner kann sie ohnehin nicht melden,
und ein falsch beschriftetes Bild vergiftet die Vorlagen unsichtbar.

⚠ **Ohne Bergbaudaten kennt der Leser nur die festen Grundsignaturen** — die
Messung wäre wertlos. Deshalb ist `--bergbau` Pflicht.
"""
import argparse
import json
import os
import re
import shutil
import sys
import tempfile
import zipfile

import ausgabe                                                 # noqa: E402
ausgabe.utf8()

HIER = os.path.dirname(os.path.abspath(__file__))
WURZEL = os.path.dirname(HIER)
PRUEFSATZ = os.path.join(HIER, 'pruefdaten', 'signatur-eingesandt.json')
ALTE_SAETZE = ('signatur-windows-klein.json', 'signatur-windows-gross.json')
# Höchstens so viele Muster je Ziffer in der Beilage — jedes Muster kostet
# beim Lesen einen Vergleich.
BEILAGE_JE_ZIFFER = 40
BILD = re.compile(r'^(\d+)_[^/\\]*\.png$', re.IGNORECASE)


def bilder_holen(quellen, ohne):
    """[(name, wert, raster)] aus ZIPs und Ordnern, dazu die Herkunft."""
    import signatur_lesen
    gefunden = []
    for quelle in quellen:
        ordner = quelle
        if os.path.isfile(quelle) and quelle.lower().endswith('.zip'):
            ordner = tempfile.mkdtemp(prefix='scans-zip-')
            with zipfile.ZipFile(quelle) as z:
                for info in z.infolist():
                    teile = info.filename.replace('\\', '/').split('/')
                    if info.filename.startswith(('/', '\\')) or '..' in teile:
                        continue
                    if not BILD.match(teile[-1]):
                        continue
                    z.extract(info, ordner)
        for wurzel, _ordner, dateien in os.walk(ordner):
            for datei in sorted(dateien):
                treffer = BILD.match(datei)
                if not treffer or datei in ohne:
                    continue
                bild = signatur_lesen.png_lesen(os.path.join(wurzel, datei))
                if bild:
                    gefunden.append((datei, int(treffer.group(1)), bild[0],
                                     os.path.basename(quelle)))
    return gefunden


def lesen(ss, bilder, vorlagen, werte):
    """Zählt richtig / still / falsch; die falschen mit Soll und Ist."""
    zahl = {'richtig': 0, 'still': 0, 'falsch': []}
    for name, soll, raster in bilder:
        ist = ss.read(raster, vorlagen, werte)['wert']
        if ist == soll:
            zahl['richtig'] += 1
        elif ist is None:
            zahl['still'] += 1
        else:
            zahl['falsch'].append((name, soll, ist))
    return zahl


def angelernt(ss, paths, bilder):
    """Die Muster, die `learn()` aus diesen Bildern speichert — je Ziffer."""
    eigene = paths.app_file(ss.OWN_TEMPLATE_FILE)
    muster = {}
    for _name, soll, raster in bilder:
        if os.path.exists(eigene):
            os.remove(eigene)
        ok, _grund, _bilanz = ss.learn(raster, '{:,}'.format(soll))
        if not ok:
            continue
        for ziffer, liste in ss._read_templates(eigene).items():
            ziel = muster.setdefault(ziffer, [])
            for m in liste:
                if m not in ziel:
                    ziel.append(m)
    if os.path.exists(eigene):
        os.remove(eigene)
    return muster


def kreuzweise(ss, paths, bilder, werte):
    """Dreigeteilt: zwei Drittel anlernen, das dritte lesen."""
    eigene = paths.app_file(ss.OWN_TEMPLATE_FILE)
    gesamt = {'richtig': 0, 'still': 0, 'falsch': []}
    for teil in range(3):
        lernen = [b for i, b in enumerate(bilder) if i % 3 != teil]
        pruefen = [b for i, b in enumerate(bilder) if i % 3 == teil]
        muster = angelernt(ss, paths, lernen)
        with open(eigene, 'w', encoding='utf-8') as f:
            json.dump({'format': 1, 'raster': [ss.NORM_W, ss.NORM_H],
                       'ziffern': muster}, f)
        zahl = lesen(ss, pruefen, ss.templates(), werte)
        os.remove(eigene)
        gesamt['richtig'] += zahl['richtig']
        gesamt['still'] += zahl['still']
        gesamt['falsch'] += zahl['falsch']
    return gesamt


def zusammenfuehren(alt, neu):
    """Alte Beilage zuerst, neue Muster dahinter, je Ziffer gedeckelt."""
    ergebnis = {}
    for ziffer in sorted(set(alt) | set(neu)):
        liste = list(alt.get(ziffer, []))
        for m in neu.get(ziffer, []):
            if len(liste) >= BEILAGE_JE_ZIFFER:
                break
            if m not in liste:
                liste.append(m)
        ergebnis[ziffer] = liste
    return ergebnis


def alte_saetze(ss):
    """Die Bilder der vorhandenen Prüfsätze als [(schlüssel, soll, raster)]."""
    bilder = []
    for name in ALTE_SAETZE:
        with open(os.path.join(HIER, 'pruefdaten', name), encoding='utf-8') as f:
            for schluessel, raster in json.load(f)['bilder'].items():
                soll = int(schluessel.split('#')[0].replace(',', ''))
                bilder.append((name + ':' + schluessel, soll, raster))
    return bilder


def zeile(titel, zahl, gesamt):
    return '  %-28s %3d richtig · %3d still · %d falsch (von %d)%s' % (
        titel, zahl['richtig'], zahl['still'], len(zahl['falsch']), gesamt,
        ''.join('\n      falsch: %s soll %d, gelesen %d' % f
                for f in zahl['falsch']))


def main():
    teil = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    teil.add_argument('quellen', nargs='+')
    teil.add_argument('--bergbau', required=True,
                      help='eine Kopie von mining-data.json')
    teil.add_argument('--uebernehmen', action='store_true')
    teil.add_argument('--ohne', action='append', default=[])
    args = teil.parse_args()

    heim = tempfile.mkdtemp(prefix='scans-einspielen-')
    os.environ['SC_BP_HOME'] = heim
    os.environ['SC_BP_NO_NET'] = '1'
    sys.path.insert(0, WURZEL)
    sys.path.insert(0, HIER)
    from scbp import paths, mining, signature_scan as ss
    ziel = paths.app_file(mining.CACHE)
    if not os.path.normcase(ziel).startswith(os.path.normcase(heim)):
        sys.exit('Abbruch: der Wegwerf-Ordner greift nicht (%s)' % ziel)
    os.makedirs(os.path.dirname(ziel), exist_ok=True)
    shutil.copyfile(args.bergbau, ziel)

    werte = ss.possible_values()
    alle = bilder_holen(args.quellen, set(args.ohne))
    nutzbar = [(n, w, r) for n, w, r, _q in alle if w in werte]
    sichten = [(n, w, q) for n, w, _r, q in alle if w not in werte]
    print('%d Bilder, %d mögliche Werte aus den Bergbaudaten' % (len(alle), len(werte)))
    if sichten:
        print('\nNicht angelernt, bitte ansehen (Wert nicht in den Bergbaudaten):')
        for n, w, q in sichten:
            print('  %-36s %6d  (%s)' % (n, w, q))
    if not nutzbar:
        print('\nNichts anzulernen.')
        return 0

    beilage_alt = ss._read_templates(paths.bundled_file(ss.TEMPLATE_FILE))
    vorher = lesen(ss, nutzbar, ss.templates(), werte)
    kreuz = kreuzweise(ss, paths, nutzbar, werte)
    neu = angelernt(ss, paths, nutzbar)
    beilage_neu = zusammenfuehren(beilage_alt, neu)

    alt_bilder = alte_saetze(ss)
    alt_werte = sorted(set(werte) | {s for _n, s, _r in alt_bilder})
    alt_mit_alt = lesen(ss, alt_bilder, ss.templates(), alt_werte)
    probe = os.path.join(heim, 'beilage-neu.json')
    with open(probe, 'w', encoding='utf-8') as f:
        json.dump({'format': 1, 'raster': [ss.NORM_W, ss.NORM_H],
                   'ziffern': beilage_neu}, f)
    echt = paths.bundled_file
    paths.bundled_file = (lambda name: probe if name == ss.TEMPLATE_FILE
                          else echt(name))
    try:
        mit_neu = ss.templates()
        alt_mit_neu = lesen(ss, alt_bilder, mit_neu, alt_werte)
        nachher = lesen(ss, nutzbar, mit_neu, werte)
    finally:
        paths.bundled_file = echt

    print('\nEingesandte Bilder (%d nutzbar):' % len(nutzbar))
    print(zeile('ab Werk (alte Beilage)', vorher, len(nutzbar)))
    print(zeile('kreuzweise angelernt', kreuz, len(nutzbar)))
    print(zeile('mit neuer Beilage', nachher, len(nutzbar)))
    print('\nAlte Prüfsätze (%d Bilder):' % len(alt_bilder))
    print(zeile('alte Beilage', alt_mit_alt, len(alt_bilder)))
    print(zeile('neue Beilage', alt_mit_neu, len(alt_bilder)))
    print('\nBeilage je Ziffer: %s → %s' % (
        {z: len(v) for z, v in sorted(beilage_alt.items())},
        {z: len(v) for z, v in sorted(beilage_neu.items())}))

    gruende = []
    if kreuz['falsch']:
        gruende.append('kreuzweise falsch gelesen')
    if alt_mit_neu['falsch']:
        gruende.append('alte Prüfsätze falsch gelesen')
    if alt_mit_neu['richtig'] < alt_mit_alt['richtig']:
        gruende.append('alte Prüfsätze schlechter')
    if gruende:
        print('\nNICHT übernehmbar: ' + ', '.join(gruende))
        return 1
    if not args.uebernehmen:
        print('\nÜbernehmbar. Mit --uebernehmen schreiben.')
        return 0

    pfad = os.path.join(WURZEL, 'daten', ss.TEMPLATE_FILE)
    with open(pfad, 'w', encoding='utf-8') as f:
        json.dump({'format': 1, 'raster': [ss.NORM_W, ss.NORM_H],
                   'ziffern': beilage_neu}, f)
    satz = {'hinweis': ('Eingesandte Scan-Bilder (Windows), Schlüssel = Wert '
                        'aus dem Dateinamen, per Auge geprüft. Die Beilage '
                        'enthält Muster aus diesen Bildern.'),
            'bilder': {}}
    if os.path.exists(PRUEFSATZ):
        with open(PRUEFSATZ, encoding='utf-8') as f:
            satz = json.load(f)
    for _n, soll, raster in nutzbar:
        schluessel, nummer = '{:,}'.format(soll), 1
        while (schluessel if nummer == 1 else '%s#%d' % (schluessel, nummer)) \
                in satz['bilder']:
            nummer += 1
        satz['bilder'][schluessel if nummer == 1
                       else '%s#%d' % (schluessel, nummer)] = raster
    with open(PRUEFSATZ, 'w', encoding='utf-8') as f:
        json.dump(satz, f)
    print('\nGeschrieben: %s und %s' % (os.path.relpath(pfad, WURZEL),
                                         os.path.relpath(PRUEFSATZ, WURZEL)))
    return 0


if __name__ == '__main__':
    sys.exit(main())
