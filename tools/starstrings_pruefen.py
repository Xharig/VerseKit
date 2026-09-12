"""Prüft, dass die Injektion **MrKrakens StarStrings unangetastet lässt**.

Wer StarStrings als Grundlage auswählt, will StarStrings. Der Watcher darf dort
nur beisteuern, was ein fremdes Projekt nicht kann — die Kästchen zum eigenen
Bestand — und muss alles andere so lassen, wie MrKraken es geschrieben hat.

⚠ Warum es das gibt: Bis zum 29.08.2026 tat er das nicht. Gemessen an der echten
Fassung von diesem Tag:

  * **17** seiner Auftrags-Kennzeichnungen `<EM4>[BP]</EM4>` schnitt der
    Formen-Notnagel heraus — und weil danach der bereits geschnittene Wortlaut
    als „Urtext" gemerkt wurde, kamen sie auch beim Zurücksetzen nie wieder.
  * bei den übrigen **297** stand die Marke danach doppelt.
  * **136** Gegenstandsnamen bekamen ihr Kürzel ein zweites Mal angehängt
    (`[CS1] Spark-G Missile (CS1)`).

Gemessen wird an der echten Datei, aber nie in ihr.

    python3 tools/starstrings_pruefen.py

Braucht Netz (StarStrings + Vertragsdaten). Ein bereits geladenes Archiv lässt
sich über `SS_ZIP=/pfad/StarStrings-LIVE.zip` mitgeben.
"""

import filecmp
import io
import json
import os
import re
import shutil
import sys
import tempfile
import urllib.request
import zipfile

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
heim = tempfile.mkdtemp(prefix='starstrings-')
os.environ['SC_BP_HOME'] = heim
from scbp import injektion

ADRESSE = ('https://github.com/MrKraken/StarStrings/releases/download/'
           'latest/StarStrings-LIVE.zip')
# Ein Kennzeichen von MrKraken, das genau so aussieht wie unseres.
FREMDE_MARKE = re.compile(r'<EM4>\[BP\]</EM4>')


def hole_ini():
    eigen = os.environ.get('SS_ZIP')
    if eigen:
        roh = open(eigen, 'rb').read()
    else:
        print('StarStrings wird geladen …')
        req = urllib.request.Request(
            ADRESSE, headers={'User-Agent': 'SC-BP-Watcher-Pruefung'})
        with urllib.request.urlopen(req, timeout=120) as r:
            roh = r.read()
    with zipfile.ZipFile(io.BytesIO(roh)) as z:
        name = [x for x in z.namelist()
                if x.lower().endswith('global.ini')][0]
        return z.read(name)


arbeit = os.path.join(heim, 'global.ini')
with open(arbeit, 'wb') as f:
    f.write(hole_ini())
urfassung = os.path.join(heim, 'urfassung.ini')
shutil.copyfile(arbeit, urfassung)


def lies(pfad):
    return open(pfad, encoding='utf-8', errors='ignore').read()


def markierte_schluessel(text):
    """Welche Einträge tragen eine Bauplan-Marke — auf den Schlüssel genau.

    ⚠ Nicht bloß zählen: Der Watcher setzt an Aufträgen, die MrKraken **nicht**
    gekennzeichnet hat, seine eigene Marke — das ist der Zweck. Die Zahl steigt
    also. Geprüft werden muss, dass **seine** Einträge ihre behalten."""
    treffer = set()
    for zeile in text.splitlines():
        if '=' not in zeile:
            continue
        schluessel, wert = zeile.split('=', 1)
        # ⚠ MrKraken schreibt **drei** Formen: `<EM4>[BP]</EM4>`,
        # `<EM4>[10 Rep] [BP]</EM4>` und `<EM4>[150 Rep] [BP]*</EM4>` (allein
        # die letzte 267 mal). Der erste Anlauf hier suchte nach `[BP]</EM4>`
        # und prüfte damit 47 statt 314 Einträge.
        if injektion.TITELMARKE.search(wert):
            treffer.add(schluessel)
    return treffer


vorher = lies(arbeit)
marken_vorher = len(FREMDE_MARKE.findall(vorher))
seine_schluessel = markierte_schluessel(vorher)
namen_vorher = len([z for z in vorher.splitlines()
                    if injektion.FREMDES_KUERZEL.search(z.split('=', 1)[-1])])
print('StarStrings: %d KB, %d Kennzeichnungen [BP], %d Namen mit Kürzel'
      % (len(vorher) // 1024, marken_vorher, namen_vorher))

fehler = []

# ---------------------------------------------------------------- 0) frisch?
# So, wie es nach `translation.fetch()` aussieht: eben eingesetzt, nie berührt.
injektion.urtext_verwerfen()
if not injektion.ist_frisch():
    fehler.append('Die eingesetzte Datei gilt nicht als frisch.')
if injektion.ist_drin(arbeit):
    fehler.append('ist_drin() meldet eine Injektion, obwohl nur MrKrakens '
                  'eigene Kennzeichnungen dastehen.')
print('\n0) frisch eingesetzt  -> ist_frisch=%s  ist_drin=%s'
      % (injektion.ist_frisch(), injektion.ist_drin(arbeit)))

# ------------------------------------------------------------ 1) einspielen
ok, n, meldung = injektion.einrichten(arbeit, 'english')
print('\n1) Einspielen         ->', ok, n, meldung)
nachher = lies(arbeit)
marken_nachher = len(FREMDE_MARKE.findall(nachher))
doppelt = len(re.findall(r'<EM4>\[BP\]</EM4>\s*<EM4>\[BP\]</EM4>', nachher))
namen_nachher = len([z for z in nachher.splitlines()
                     if injektion.FREMDES_KUERZEL.search(z.split('=', 1)[-1])])
kaestchen = nachher.count('[x]') + nachher.count('[  ]')
behalten = seine_schluessel & markierte_schluessel(nachher)
print('   MrKrakens Einträge : %d von %d behalten ihre Marke'
      % (len(behalten), len(seine_schluessel)))
print('   Marken insgesamt   : %d (vorher %d — die Zunahme ist unser Beitrag '
      'an Aufträgen, die er nicht gekennzeichnet hat)'
      % (marken_nachher, marken_vorher))
print('   doppelte Marken    : %d' % doppelt)
print('   Namen mit Kürzel   : %d von %d unverändert' % (namen_nachher,
                                                         namen_vorher))
print('   eigene Kästchen    : %d' % kaestchen)
print('   ist_drin           :', injektion.ist_drin(arbeit))

if behalten != seine_schluessel:
    fehler.append('MrKrakens Kennzeichnungen: %d von %d verloren.'
                  % (len(seine_schluessel - behalten), len(seine_schluessel)))
if doppelt:
    fehler.append('%d Titel tragen die Marke doppelt.' % doppelt)
if namen_nachher != namen_vorher:
    fehler.append('%d Namen mit fremdem Kürzel wurden verändert.'
                  % (namen_vorher - namen_nachher))
if not kaestchen:
    fehler.append('Keine Kästchen eingetragen — der eigene Beitrag fehlt.')
if not injektion.ist_drin(arbeit):
    fehler.append('ist_drin() erkennt die eigene Injektion nicht.')

# ------------------------------------------------------------- 2) entfernen
ok, n, meldung = injektion.entfernen(arbeit, 'english')
print('\n2) Entfernen          ->', ok, n, meldung)
gleich = filecmp.cmp(arbeit, urfassung, shallow=False)
print('   Wortlaut wie MrKraken ihn schrieb:', 'JA' if gleich else 'NEIN')
if not gleich:
    fehler.append('Nach dem Zurücksetzen weicht der Wortlaut ab.')
    a, b = lies(arbeit).splitlines(), lies(urfassung).splitlines()
    gezeigt = 0
    for i, (x, y) in enumerate(zip(a, b)):
        if x != y and gezeigt < 5:
            print('   Zeile %d\n     ist : %s\n     soll: %s'
                  % (i + 1, x[:140], y[:140]))
            gezeigt += 1

print('\n' + '=' * 60)
if fehler:
    print('FEHLGESCHLAGEN:')
    for f in fehler:
        print('  - ' + f)
    sys.exit(1)
print('BESTANDEN — StarStrings bleibt unangetastet, die Kästchen kommen dazu.')
