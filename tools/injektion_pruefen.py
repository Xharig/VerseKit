"""Prüft den Kreislauf der Injektion auf einer **Kopie** der global.ini.

Einspielen und wieder entfernen muss den Wortlaut unverändert lassen. Genau das
misst dieses Werkzeug — an der echten Datei, aber nie in ihr.

⚠ Warum es das gibt: Beim Umbau auf die markenlose Version schnitt ein zu grobes
Muster 589 Zeichen aus einem Auftragstext, der uns gar nichts anging. Aufgefallen
ist das nur, weil hier Zeichen für Zeichen verglichen wird. Auf dem Bildschirm
hätte es niemand gesehen.

    python3 tools/injektion_pruefen.py
"""

import os, sys, shutil, tempfile, filecmp
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
heim = tempfile.mkdtemp(prefix='marken-')
os.environ['SC_BP_HOME'] = heim
os.environ['SC_BP_NO_NET'] = '1'
from scbp import injektion, collection as bestand_datei

# ⚠ Kein fester Pfad: Der Spielordner liegt bei jedem woanders, und ein
# Heimverzeichnis im Quelltext ist eine persoenliche Angabe im oeffentlichen
# Repo (Projektregel "Keine persoenlichen Daten"). Ueber SC_INSTALL_DIR
# setzbar, sonst werden die ueblichen Orte der Reihe nach probiert.
def _global_ini():
    kandidaten = []
    eigen = os.environ.get("SC_INSTALL_DIR")
    if eigen:
        kandidaten.append(eigen)
    kandidaten += [
        os.path.expanduser("~/Games/star-citizen/drive_c/Program Files/"
                           "Roberts Space Industries/StarCitizen/LIVE"),
        r"C:\Program Files\Roberts Space Industries\StarCitizen\LIVE",
    ]
    for basis in kandidaten:
        pfad = os.path.join(basis, "data", "Localization",
                            "german_(germany)", "global.ini")
        if os.path.exists(pfad):
            return pfad
    raise SystemExit("global.ini nicht gefunden — SC_INSTALL_DIR setzen.")


ECHT = _global_ini()
arbeit = os.path.join(heim, 'global.ini')

# Der aktuelle Stand hat noch die ALTEN Marken -> erst sauber machen
shutil.copyfile(ECHT, arbeit)
print('Kopie angelegt:', os.path.getsize(arbeit) // 1024, 'KB')
print('Marken vorher :', sum(1 for z in open(arbeit, encoding='utf-8', errors='ignore') if '[SCBPW]' in z))

# Den echten Bestand des laufenden Systems holen, damit die Haken stimmen
import json
bestand = json.load(open(os.path.expanduser('~/Dokumente/SC BP Watcher/Bauplaene/bestand.json'), encoding='utf-8'))

ok, n, meldung = injektion.entfernen(arbeit)
print('\n1) Alles entfernen  ->', ok, n, meldung)
print('   Marken danach    :', sum(1 for z in open(arbeit, encoding='utf-8', errors='ignore') if '[SCBPW]' in z))
print('   ist_drin         :', injektion.ist_drin(arbeit))
sauber = os.path.join(heim, 'sauber.ini')
shutil.copyfile(arbeit, sauber)

ok, n, meldung = injektion.einrichten(arbeit, 'german_(germany)', bestand=bestand)
print('\n2) Neu einspielen   ->', ok, n, meldung)
print('   Marken im Text   :', sum(1 for z in open(arbeit, encoding='utf-8', errors='ignore') if '[SCBPW]' in z))
print('   ist_drin         :', injektion.ist_drin(arbeit))
merk = os.path.join(heim, 'injektion-urtext.json')
print('   Urtexte gemerkt  :', len(json.load(open(merk, encoding='utf-8'))['texte']) if os.path.exists(merk) else 'DATEI FEHLT')

ok, n, meldung = injektion.entfernen(arbeit)
print('\n3) Wieder entfernen ->', ok, n, meldung)
print('   ist_drin         :', injektion.ist_drin(arbeit))
gleich = filecmp.cmp(arbeit, sauber, shallow=False)
print('   Wortlaut wie vorher:', 'JA' if gleich else 'NEIN')
if not gleich:
    a = open(arbeit, encoding='utf-8', errors='ignore').read().splitlines()
    b = open(sauber, encoding='utf-8', errors='ignore').read().splitlines()
    abweich = [(i, x, y) for i, (x, y) in enumerate(zip(a, b)) if x != y]
    print('   Abweichungen:', len(abweich))
    for i, x, y in abweich[:3]:
        print('     Zeile', i, '\n       ist :', x[:110], '\n       soll:', y[:110])
