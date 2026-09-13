"""Prüft das Zusammenspiel mit dem **SC Deutsch Launcher**.

Beide Werkzeuge schöpfen aus derselben Quelle — den Vertragsdaten des
SCDL-Teams (`blueprints/Data/bp-contracts_short.json`). Sie schreiben deshalb
**wortgleiche** Blöcke an dieselben Textschlüssel und dieselbe Titelmarke
` <EM4>[BP]</EM4>` (die Rohdaten geben sie für 369 der 818 Aufträge vor).

Wer beide benutzt, darf davon nichts merken:

  * die Bauplan-Liste steht **einmal** da, nicht zweimal untereinander
  * die Marke steht **einmal** am Titel
  * unsere Liste hat die **Kästchen** — das ist der Mehrwert, deshalb ersetzt
    unser Block seinen
  * beim Zurücksetzen bekommt der Spieler **den Stand des Launchers zurück**,
    nicht eine gerupfte Datei

Der Launcher läuft nur unter Windows. Damit das hier überall prüfbar ist, wird
sein Ergebnis aus derselben Quelle nachgebaut, aus der er es selbst nimmt: die
Rohblöcke ohne Kästchen, an dieselben Schlüssel.

    python3 tools/launcher_pruefen.py

Braucht Netz. Bereits geladene Dateien lassen sich über `DE_ZIP=…` und
`BP_JSON=…` mitgeben.
"""

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
# ⛔ Vor der ersten Ausgabe: Die Windows-Konsole kann kein `↔` — siehe ausgabe.py.
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import ausgabe                                                 # noqa: E402
ausgabe.utf8()

heim = tempfile.mkdtemp(prefix='launcher-')
os.environ['SC_BP_HOME'] = heim
from scbp import injektion

DE_ZIP = ('https://github.com/rjcncpt/StarCitizen-Deutsch-INI/releases/'
          'latest/download/StarCitizen.Deutsch.LIVE.zip')
BP_JSON = ('https://raw.githubusercontent.com/rjcncpt/StarCitizen-Deutsch-INI/'
           'master/blueprints/Data/bp-contracts_short.json')
UEBERSCHRIFT = 'MÖGLICHE BAUPLÄNE FÜR DIESEN MISSIONSTYP'


def hole(adresse, umgebung):
    eigen = os.environ.get(umgebung)
    if eigen:
        return open(eigen, 'rb').read()
    print('lädt %s …' % umgebung)
    req = urllib.request.Request(adresse,
                                 headers={'User-Agent': 'SC-BP-Watcher-Pruefung'})
    with urllib.request.urlopen(req, timeout=180) as r:
        return r.read()


with zipfile.ZipFile(io.BytesIO(hole(DE_ZIP, 'DE_ZIP'))) as z:
    name = [x for x in z.namelist() if x.lower().endswith('global.ini')][0]
    grund = z.read(name).decode('utf-8', 'ignore')
daten = json.loads(hole(BP_JSON, 'BP_JSON').decode('utf-8'))

# ---------------------------------------------------------------------------
# Den Launcher nachbilden: seine Blöcke und Titelmarken an dieselben Schlüssel,
# roh aus der Quelle — also mit `    - Name` statt Kästchen.
anhang = {}
for e in daten['entries']:
    if e.get('descriptionLocKey') and e.get('description'):
        anhang[e['descriptionLocKey']] = e['description']
    if e.get('titleLocKey') and e.get('title'):
        anhang[e['titleLocKey']] = e['title']

zeilen, gesetzt = [], 0
for zeile in grund.splitlines():
    teile = injektion._zeile_zerlegen(zeile)
    if teile and teile[0] in anhang:
        schluessel, zusatz, text = teile
        zeilen.append('%s%s=%s%s' % (schluessel, zusatz, text,
                                     anhang[schluessel]))
        gesetzt += 1
    else:
        zeilen.append(zeile)

arbeit = os.path.join(heim, 'global.ini')
with open(arbeit, 'w', encoding='utf-8') as f:
    f.write('\n'.join(zeilen) + '\n')
launcherstand = os.path.join(heim, 'launcher.ini')
shutil.copyfile(arbeit, launcherstand)

lies = lambda p: open(p, encoding='utf-8', errors='ignore').read()
def doppelt_je_zeile(text, was):
    """Wie viele **Einträge** tragen `was` mehr als einmal?

    ⚠ Zeilenweise zählen, nicht über die Datei: Ein Eintrag ist eine Zeile, und
    zwei Blöcke in zwei aufeinanderfolgenden Zeilen sind völlig in Ordnung. Der
    erste Anlauf suchte über die ganze Datei und meldete 180 Doppelungen, die
    keine waren."""
    return sum(1 for z in text.splitlines() if z.count(was) > 1)


vorher = lies(arbeit)
bloecke_vorher = vorher.count(UEBERSCHRIFT)
marken_vorher = len(injektion.TITELMARKE.findall(vorher))
# ⚠ Grundlinie: CIG hat selbst eine Zeile im Format `\n    - `. Ohne sie
# abzuziehen meldet die Prüfung einen Launcher-Rest, der keiner ist.
ohne_kasten_vorher = 0
print('Launcher-Stand nachgebaut: %d Einträge ergänzt, %d Blöcke, %d Titelmarken'
      % (gesetzt, bloecke_vorher, marken_vorher))

fehler = []

# ---- 0) Der Watcher hat hier nie geschrieben -------------------------------
if injektion.ist_drin(arbeit):
    fehler.append('ist_drin() hält den Launcher-Stand für eine eigene '
                  'Injektion.')
print('\n0) nur der Launcher   -> ist_drin=%s (muss False sein)'
      % injektion.ist_drin(arbeit))

# ---- 1) Einspielen --------------------------------------------------------
ok, n, meldung = injektion.einrichten(arbeit, 'german_(germany)')
nachher = lies(arbeit)
bloecke = nachher.count(UEBERSCHRIFT)
doppelblock = doppelt_je_zeile(nachher, UEBERSCHRIFT)
doppelmarke = len(re.findall(r'<EM4>\[BP\]</EM4>\s*<EM4>\[BP[^\]]*\]</EM4>',
                             nachher))
marken = len(injektion.TITELMARKE.findall(nachher))
kaestchen = nachher.count('[x]') + nachher.count('[  ]')
# ⚠ Ein echter Launcher-Rest ist eine Bauplan-Zeile ohne Kästchen in einem
# Eintrag, der **überhaupt keine** Kästchen hat — dann wurde sein Block nicht
# ersetzt. Einzelne `- `-Zeilen innerhalb unseres Blocks sind etwas anderes
# (die Rohdaten führen unter `# Region:` ebenfalls Listenzeilen).
ohne_kasten = sum(1 for z in nachher.splitlines()
                  if re.search(r'\\n    - ', z)
                  and injektion.EIGENER_NACHWEIS.search(z)
                  and not injektion._hat_kaestchen(z))
print('\n1) Einspielen         ->', ok, meldung)
print('   Blöcke             : %d (vorher %d)' % (bloecke, bloecke_vorher))
print('   doppelte Blöcke    : %d' % doppelblock)
print('   Titelmarken        : %d (vorher %d)' % (marken, marken_vorher))
print('   doppelte Marken    : %d' % doppelmarke)
print('   Kästchen           : %d' % kaestchen)
print('   unersetzte Blöcke  : %d' % ohne_kasten)

if doppelblock:
    fehler.append('%d Beschreibungen tragen die Liste doppelt.' % doppelblock)
if doppelmarke:
    fehler.append('%d Titel tragen die Marke doppelt.' % doppelmarke)
# ⚠⚠ **Nicht mehr „gar keine neue Marke", sondern „keine UNERKLÄRTE".**
#
# Bis zum 03.09.2026 stand hier `if marken > marken_vorher`. Die Regel dahinter
# war richtig — der Watcher darf keine zweite Marke setzen, wo der Launcher
# schon eine hat (297 Doppelungen waren hier schon einmal der Schaden). Sie war
# nur zu grob formuliert: Sie verbot **jeden** eigenen Titel-Beitrag.
#
# Seit `_reihen_stamm` markiert der Watcher auch die SCHRITTE einer Reihe.
# Der Launcher tut das nicht, weil die Quelle nur den Schlüssel der Reihe
# kennt — im Spiel steht man aber im Schritt: `Battaglia_Story01B_title`
# („Bergbau-Gelegenheit") blieb unmarkiert, während der Bauplan an
# `Battaglia_Story01_title` hing. Genau diese vier Marken sind gewollt.
#
# Die schärfere Fassung prüft deshalb **welche** Marken dazugekommen sind:
# erlaubt ist nur, was sich als Schritt eines bereits markierten Auftrags
# ausweist. Alles andere bleibt ein Fehler — die Doppelungs-Wache
# (`doppelmarke`) darüber gilt ohnehin unverändert.
def marken_schluessel(text):
    """Alle Schlüssel, deren Wert eine Titelmarke trägt."""
    raus = set()
    for z in text.splitlines():
        k, _, v = z.partition('=')
        if k and injektion.TITELMARKE.search(v):
            raus.add(k.strip())
    return raus


alte_marken = marken_schluessel(vorher)
neue_marken = marken_schluessel(nachher) - alte_marken
bekannte_staemme = {injektion._stamm(k) for k in alte_marken}
bekannte_staemme.discard('')

unerklaert = [k for k in neue_marken
              if not injektion._reihen_stamm(injektion._stamm(k),
                                             bekannte_staemme)]
print('   neue Marken        : %d (davon unerklärt: %d)'
      % (len(neue_marken), len(unerklaert)))
for k in sorted(neue_marken)[:8]:
    grund = injektion._reihen_stamm(injektion._stamm(k), bekannte_staemme)
    print('     %-42s %s' % (k, ('Schritt von ' + grund) if grund
                             else '⚠ OHNE GRUND'))
if unerklaert:
    fehler.append('%d Titelmarken sind ohne erkennbaren Grund dazugekommen: %s'
                  % (len(unerklaert), ', '.join(sorted(unerklaert)[:5])))
if not kaestchen:
    fehler.append('Keine Kästchen — der eigene Beitrag fehlt.')
if ohne_kasten:
    fehler.append('%d Blöcke stehen noch ohne Kästchen da — der Block des '
                  'Launchers wurde nicht ersetzt.' % ohne_kasten)

# ---- 2) Entfernen ---------------------------------------------------------
ok, n, meldung = injektion.entfernen(arbeit, 'german_(germany)')
print('\n2) Entfernen          ->', ok, meldung)
a, b = lies(arbeit).splitlines(), lies(launcherstand).splitlines()
abweichung = [i for i, (x, y) in enumerate(zip(a, b)) if x != y]
print('   Zeilen: %d ↔ %d, abweichend: %d' % (len(a), len(b), len(abweichung)))
print('   Launcher-Stand wieder da:', 'JA' if not abweichung
      and len(a) == len(b) else 'NEIN')
if abweichung or len(a) != len(b):
    fehler.append('Nach dem Zurücksetzen fehlt dem Spieler sein '
                  'Launcher-Stand.')
    for i in abweichung[:5]:
        print('   Zeile %d\n     ist : %s\n     soll: %s'
              % (i + 1, a[i][:140], b[i][:140]))

print('\n' + '=' * 60)
if fehler:
    print('FEHLGESCHLAGEN:')
    for f in fehler:
        print('  - ' + f)
    sys.exit(1)
print('BESTANDEN — eine Liste, eine Marke, mit Kästchen; '
      'Zurücksetzen gibt dem Launcher das Seine.')
