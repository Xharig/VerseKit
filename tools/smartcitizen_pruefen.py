"""Prüft, dass Watcher und **Smart Citizen** sich in der `global.ini` vertragen.

Smart Citizen (Osiris-DevWorks, Apache-2.0) hängt an Gegenstands- und
Missionsbeschreibungen eigene Blöcke an — Werte, die CIG im Spiel nicht zeigt.
Es ist damit nach StarStrings und dem SC Deutsch Launcher das **dritte**
Werkzeug, das in dieselbe Datei schreibt, und das erste, das sich mit uns
inhaltlich überschneidet: Es führt einen eigenen Bauplan-Bestand und schreibt
Bauplan-Abschnitte in Missionsbeschreibungen.

Geprüft wird in **beide** Richtungen — das ist der Sinn:

  1. **Wir über deren Stand.** Bleiben ihre Blöcke heil, entsteht nichts
     doppelt, und kommt beim Zurücksetzen ihr Wortlaut zeichengenau zurück?
  2. **Sie über unseren Stand.** Ihr `append_enhancements` schneidet den
     bestehenden Text **ab dem ersten** ihrer Marker weg. Steht unser
     Bauplan-Block dahinter, verschwindet er bei deren nächstem Lauf — ohne
     dass jemand etwas merkt. Diese Richtung sagt vorher, was der Nutzer sonst
     als „meine Baupläne sind plötzlich weg" meldet.

⚠ Richtung 2 ist keine Kritik an Smart Citizen: Das Abschneiden ist dort
richtig — es entfernt den **eigenen** alten Block, bevor der neue kommt. Nur
kennt es unsere Marken nicht.

Gemessen wird an der echten deutschen Datei, aber nie in ihr.

    python3 tools/smartcitizen_pruefen.py

Braucht Netz (Übersetzung + Smart Citizens Generator). Bereits geladene Dateien
lassen sich mitgeben:

    DE_ZIP=/pfad/StarCitizen.Deutsch.LIVE.zip SC_GEN=/pfad/generate_enhancements_ini.py
"""

import filecmp
import io
import os
import re
import shutil
import sys
import tempfile
import urllib.request
import zipfile

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
heim = tempfile.mkdtemp(prefix='smartcitizen-')
os.environ['SC_BP_HOME'] = heim
from scbp import injektion

# ⛔ Vor der ersten Ausgabe: Die Windows-Konsole kann kein `⚠` — siehe ausgabe.py.
import ausgabe                                                 # noqa: E402
ausgabe.utf8()

DE_ZIP = ('https://github.com/rjcncpt/StarCitizen-Deutsch-INI/releases/'
          'latest/download/StarCitizen.Deutsch.LIVE.zip')
SC_GEN = ('https://raw.githubusercontent.com/Osiris-DevWorks/smart-citizen/'
          'main/scripts/generate_enhancements_ini.py')

# Smart Citizens Marken, wörtlich aus `scripts/generate_enhancements_ini.py`
# (Stand 02.09.2026). Sie stehen hier als Erwartung, nicht als Kopie: Der
# Abgleich unten holt die Datei und meldet, wenn sich dort etwas geändert hat.
SC_TRENNER = '\\n\\n--- STATS ---\\n'
SC_MARKER = ('\\n\\n--- STATS ---', '\\n\\n<EM3>STATS</EM3>',
             '\\n\\n<EM3>MISSION DETAILS</EM3>',
             '\\n\\n<EM3>== Stats ==</EM3>',
             '\\n\\n<EM3>== Mission Details ==</EM3>',
             '\\n\\n== Stats ==', '\\n\\n== Mission Details ==')


def hole(adresse, umgebung, binaer=True):
    eigen = os.environ.get(umgebung)
    if eigen:
        return open(eigen, 'rb').read()
    print('lädt %s …' % umgebung)
    req = urllib.request.Request(adresse,
                                 headers={'User-Agent': 'SC-BP-Watcher-Pruefung'})
    with urllib.request.urlopen(req, timeout=180) as r:
        return r.read()


def sc_anhaengen(vorhandener_wert, block, trenner=SC_TRENNER, davor=False):
    """Smart Citizens `append_enhancements`, Zeile für Zeile nachgebaut.

    ⚠ Nicht importiert, sondern nachgebaut: Deren Datei ist 7.700 Zeilen lang,
    zieht PyQt6 nach und braucht die entpackte `Data.p4k`. Der Abgleich weiter
    unten hält die Nachbildung ehrlich — ändert sich dort das Format, fällt die
    Prüfung auf, statt stillschweigend das Falsche zu messen.
    """
    if vorhandener_wert is None:
        vorhandener_wert = ''
    if not block:
        return vorhandener_wert
    for marker in SC_MARKER:
        if marker in vorhandener_wert:
            vorhandener_wert = vorhandener_wert[:vorhandener_wert.index(marker)]
            break
    if davor:
        return block + '\\n\\n------\\n\\n' + vorhandener_wert
    return vorhandener_wert + trenner + block


lies = lambda p: open(p, encoding='utf-8', errors='ignore').read()

# ---------------------------------------------------------------------------
# 0) Nachbildung gegen das Original halten
# ---------------------------------------------------------------------------
quelle = hole(SC_GEN, 'SC_GEN').decode('utf-8', 'ignore')
fehler = []
abweichend = [m for m in SC_MARKER if repr(m)[1:-1] not in quelle
              and m.replace('\\\\n', '\\n') not in quelle]
if 'ENHANCEMENT_SEPARATOR' not in quelle:
    fehler.append('Smart Citizens Generator hat keinen ENHANCEMENT_SEPARATOR '
                  'mehr — das Format hat sich geändert, die Nachbildung ist '
                  'überholt.')
if '--- STATS ---' not in quelle:
    fehler.append('Die Marke "--- STATS ---" steht nicht mehr in deren Quelle.')
print('0) Nachbildung: %d von %d Markern im Original wiedergefunden'
      % (len(SC_MARKER) - len(abweichend), len(SC_MARKER)))
if abweichend:
    print('   nicht gefunden:', ', '.join(repr(m) for m in abweichend))
    # ⚠ Das ist die dritte Sicherung und muss **fehlschlagen**, nicht nur
    # melden. `FREMDER_ANHANG` in `scbp/injektion.py` setzt darauf, dass deren
    # Marker in der Form `--- WORT ---` / `== Wort ==` / `<EMn>Wort</EMn>`
    # bleiben. Verschwindet einer aus deren Quelle, ist entweder die Form
    # gewandert oder unsere Nachbildung veraltet — beides heißt: hinsehen,
    # bevor das nächste Release rausgeht.
    fehler.append('%d Marker stehen nicht mehr in Smart Citizens Quelle: %s. '
                  'Entweder haben sie umbenannt (dann FREMDER_ANHANG in '
                  'scbp/injektion.py gegenprüfen) oder diese Nachbildung ist '
                  'veraltet.'
                  % (len(abweichend), ', '.join(repr(m) for m in abweichend)))

# Und die Gegenprobe: greift unser Muster auf **jeden** ihrer Marker?
nicht_erkannt = [m for m in SC_MARKER
                 if not injektion.FREMDER_ANHANG.search('Text' + m + '\\nWert')]
print('   unser Muster erkennt: %d von %d ihrer Marken'
      % (len(SC_MARKER) - len(nicht_erkannt), len(SC_MARKER)))
if nicht_erkannt:
    fehler.append('FREMDER_ANHANG erkennt %d ihrer Marken nicht: %s. '
                  'Bei genau diesen landet unser Block wieder dahinter und '
                  'wird bei deren nächstem Lauf weggeschnitten.'
                  % (len(nicht_erkannt), ', '.join(repr(m)
                                                   for m in nicht_erkannt)))

# ---------------------------------------------------------------------------
# Deren Stand nachbauen: an jede 30. Beschreibung ein Stats-Block
# ---------------------------------------------------------------------------
with zipfile.ZipFile(io.BytesIO(hole(DE_ZIP, 'DE_ZIP'))) as z:
    name = [x for x in z.namelist() if x.lower().endswith('global.ini')][0]
    grund = z.read(name).decode('utf-8', 'ignore')

BLOCK = ('Weight: 10.0 kg\\nFire Rate: 650 RPM\\nAlpha Dmg: 14.5 (Phys) | '
         'DPS: 157.1\\nAmmo: 75\\nVelocity: 600 m/s')

# ⚠ **Alle** Beschreibungen, keine Stichprobe. Der erste Anlauf nahm jede 30.
# und meldete „13 Einträge betroffen" — eine Zahl, die nur die Stichprobe
# beschrieb, nicht die Lage. Smart Citizen fasst im Betrieb jede passende
# Beschreibung an; die Prüfung muss dasselbe tun, sonst sieht ein echter
# Konflikt harmlos aus.
zeilen, gesetzt, ihre_schluessel = [], 0, set()
for zeile in grund.splitlines():
    teile = injektion._zeile_zerlegen(zeile)
    if teile and re.search(r'desc', teile[0], re.I):
        schluessel, zusatz, text = teile
        zeilen.append('%s%s=%s' % (schluessel, zusatz,
                                   sc_anhaengen(text, BLOCK)))
        ihre_schluessel.add(schluessel)
        gesetzt += 1
    else:
        zeilen.append(zeile)

arbeit = os.path.join(heim, 'global.ini')
with open(arbeit, 'w', encoding='utf-8') as f:
    f.write('\n'.join(zeilen) + '\n')
urfassung = os.path.join(heim, 'smartcitizen.ini')
shutil.copyfile(arbeit, urfassung)

vorher = lies(arbeit)
stats_vorher = vorher.count('--- STATS ---')
print('\nSmart-Citizen-Stand nachgebaut: %d Blöcke in %d Beschreibungen'
      % (stats_vorher, gesetzt))

# ---------------------------------------------------------------------------
# 1) Erkennt der Watcher deren Stand fälschlich als eigenen?
# ---------------------------------------------------------------------------
if injektion.ist_drin(arbeit):
    fehler.append('ist_drin() hält Smart Citizens Stand für eine eigene '
                  'Injektion — dann meldet der Watcher "steht schon drin" und '
                  'trägt nie etwas ein.')
print('\n1) nur Smart Citizen  -> ist_drin=%s (muss False sein)'
      % injektion.ist_drin(arbeit))

# ---------------------------------------------------------------------------
# 2) Wir schreiben über deren Stand
# ---------------------------------------------------------------------------
ok, n, meldung = injektion.einrichten(arbeit, 'german_(germany)')
nachher = lies(arbeit)
stats_nachher = nachher.count('--- STATS ---')
zerschnitten = sum(1 for z in nachher.splitlines()
                   if z.count('--- STATS ---') > 1)
print('\n2) Einspielen         ->', ok, n, meldung)
print('   ihre Stats-Blöcke  : %d von %d unverändert'
      % (stats_nachher, stats_vorher))
print('   doppelte Blöcke    : %d' % zerschnitten)
print('   ist_drin           :', injektion.ist_drin(arbeit))

if stats_nachher != stats_vorher:
    fehler.append('%d ihrer Stats-Blöcke sind beim Einspielen verschwunden.'
                  % (stats_vorher - stats_nachher))
if zerschnitten:
    fehler.append('%d Einträge tragen den Stats-Block doppelt.' % zerschnitten)
if not injektion.ist_drin(arbeit):
    fehler.append('ist_drin() erkennt die eigene Injektion nicht.')

# ---------------------------------------------------------------------------
# 3) Zurücksetzen — kommt ihr Wortlaut zeichengenau zurück?
# ---------------------------------------------------------------------------
ok, n, meldung = injektion.entfernen(arbeit, 'german_(germany)')
gleich = filecmp.cmp(arbeit, urfassung, shallow=False)
print('\n3) Entfernen          ->', ok, n, meldung)
print('   Wortlaut wie Smart Citizen ihn schrieb:', 'JA' if gleich else 'NEIN')
if not gleich:
    fehler.append('Nach dem Zurücksetzen weicht der Wortlaut ab.')
    a, b = lies(arbeit).splitlines(), lies(urfassung).splitlines()
    gezeigt = 0
    for i, (x, y) in enumerate(zip(a, b)):
        if x != y and gezeigt < 5:
            print('   Zeile %d\n     ist : %s\n     soll: %s'
                  % (i + 1, x[:140], y[:140]))
            gezeigt += 1

# ---------------------------------------------------------------------------
# 4) ⭐ Die andere Richtung: was macht IHR Lauf mit UNSEREM Block?
# ---------------------------------------------------------------------------
# Der eigentliche Grund für diese Prüfung. Deren `append_enhancements` schneidet
# ab dem ersten eigenen Marker alles weg. Steht unser Bauplan-Block dahinter,
# ist er beim nächsten Smart-Citizen-Lauf still verschwunden.
shutil.copyfile(urfassung, arbeit)
injektion.einrichten(arbeit, 'german_(germany)')
mit_uns = lies(arbeit)

verloren, geprueft = 0, 0
for zeile in mit_uns.splitlines():
    teile = injektion._zeile_zerlegen(zeile)
    if not teile or teile[0] not in ihre_schluessel:
        continue
    text = teile[2]
    if not injektion.EIGENER_NACHWEIS.search(text):
        continue          # an dieser Zeile haben wir gar nichts geschrieben
    geprueft += 1
    danach = sc_anhaengen(text, BLOCK)
    if not injektion.EIGENER_NACHWEIS.search(danach):
        verloren += 1

print('\n4) Ihr Lauf über unseren Stand')
print('   gemeinsame Einträge: %d' % geprueft)
print('   unser Block würde verschwinden bei: %d' % verloren)

if geprueft and verloren:
    fehler.append(
        'Bei %d von %d gemeinsamen Einträgen schneidet Smart Citizens '
        'append_enhancements unseren Bauplan-Block weg. Ursache: unser Block '
        'steht HINTER ihrem "--- STATS ---"-Marker, und sie schneiden ab dem '
        'ersten Marker alles ab. Abhilfe: unseren Block VOR ihrem Marker '
        'einfügen, oder beim Einspielen erkennen, dass schon ein fremder '
        'Anhang da ist.' % (verloren, geprueft))

print('\n' + '=' * 60)
if fehler:
    print('FEHLGESCHLAGEN:')
    for f in fehler:
        print('  - ' + f)
    sys.exit(1)
print('BESTANDEN — Watcher und Smart Citizen kommen sich nicht ins Gehege.')
