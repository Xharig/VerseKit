# -*- coding: utf-8 -*-
"""Legt einen Wegwerf-Katalog und einen Beispielbestand an.

Wozu: Wer die Oberfläche auf einem Rechner ohne Star Citizen ansehen will (Mac,
Zweitrechner), bekommt sonst eine leere Liste und kann nichts beurteilen. Die
Daten landen in dem Ordner, der als Argument kommt — **nie** in der echten
Ablage.

    python3 tools/probe_daten.py /pfad/zum/wegwerf-ordner
"""
import json
import os
import sys

# Ein kleiner, aber echter Ausschnitt: verschiedene Arten, Klassen, Größen,
# mit und ohne Bezugsquelle. Genug, um Liste, Filter und Herkunft zu beurteilen.
#
# ⚠ Alle Werte MÜSSEN im Format des echten scmdb-Katalogs stehen. Der
# Gütegrad ist dort eine ZAHL (1–4), kein Buchstabe: Stand hier 'A', schlug
# `kuerzel()` in einer Zahlen-Tabelle nach und schrieb „–" — in der Liste
# stand „M/–/1" statt „M/1/A". `formate_pruefen()` unten faengt das ab.
#
# ⚠ Die Art MUSS die echte Kennung aus dem scmdb-Katalog sein — `WeaponGun`,
# nicht „Ship weapon". Hier standen ausgedachte Namen, und weil
# `catalog.KIND_GROUP` die nicht kennt, landete alles in „Sonstiges": Der
# Filter „nur FPS-Waffen" zeigte nichts, „nur Schiffsteile" zeigte nichts, und
# unter Sonstiges tauchte ein Netzteil namens XL-1 auf. Die Oberfläche war in
# Ordnung — die Testdaten waren es nicht. `_arten_pruefen()` unten lässt das
# nicht wieder durchgehen.
BEISPIELE = [
    ("7CA 'Nargun'", 'Cooler', 'Military', 1, '1', True,
     [('Foxwell Enforcement', 'Red Lvl. Contract: Protect Fuel Tanks',
       'Veteran Contractor', 15000, 48000, 'Stanton')]),
    ('Aufeis', 'Cooler', 'Civilian', 2, '2', True,
     [('Covalex', 'Hauling: Priority Freight', 'Associate', 6000, 18000, 'Stanton')]),
    ('Blizzard', 'Cooler', 'Military', 1, '3', False,
     [('Headhunters', 'Bounty: Hostile Gunship', 'Enforcer', 24000, 61000, 'Pyro')]),
    ('Attrition-5 Repeater', 'WeaponGun', None, None, '3', True,
     [('Bit-Zeros', 'Salvage: Derelict Sweep', 'Trusted Hand', 9000, 32000, 'Pyro')]),
    ('Singe Cannon (S2)', 'WeaponGun', None, None, '2', False,
     [('Foxwell Enforcement', 'Red Lvl. Contract: Hold the Line',
       'Contractor', 11000, 38000, 'Stanton')]),
    ('P4-AR Rifle', 'WeaponPersonal', None, None, None, True, []),
    ('S-38 Pistol', 'WeaponPersonal', None, None, None, True, []),
    ('Manticore Helmet', 'Char_Armor_Helmet', None, None, None, True,
     [('Headhunters', 'Bounty: Vanduul Scout Party', 'Enforcer', 22500, 55000, 'Pyro')]),
    ('Aves Shrike Helmet', 'Char_Armor_Helmet', None, None, None, False,
     [('Bit-Zeros', 'Mercenary: Clear the Outpost',
       'Veteran Contractor', 15000, 44000, 'Pyro'),
      ('Covalex', 'Hauling: Priority Freight', 'Associate', 6000, 18000, 'Stanton')]),
    ('BUL-H4 Armor', 'Char_Armor_Torso', None, None, None, False, []),      # XenoThreat
    ('Purgatory Camo', 'Pattern', None, None, None, False, []),  # RedWind
    ('XL-1', 'PowerPlant', 'Industrial', 2, '2', False,
     [('Rayari', 'Research: Sample Retrieval', 'Associate', 7500, 21000, 'Stanton')]),
    ('Breton Shield', 'Shield', 'Military', 1, '2', False,
     [('Headhunters', 'Bounty: Marked Target', 'Contractor', 12000, 30000, 'Pyro')]),
]
TOPF = {'BUL-H4 Armor': 'XenoThreat', 'Purgatory Camo': 'RedWind'}


def main():
    """Echten Katalog holen und einen plausiblen Bestand dazu erfinden.

    Mit dreizehn Beispielen lässt sich nichts beurteilen — Filter, Gruppen und
    Zähler zeigen erst mit dem vollen Katalog, ob sie taugen. Der Katalog kommt
    deshalb wirklich von scmdb.net (einmal geholt, danach aus dem Zwischen-
    speicher). Nur der Besitzstand ist erfunden: Wer welche Baupläne hat, weiß
    nur der eigene Rechner.
    """
    ziel = sys.argv[1] if len(sys.argv) > 1 else None
    if not ziel:
        print(__doc__.strip())
        return 2
    os.makedirs(ziel, exist_ok=True)
    os.environ['SC_BP_HOME'] = ziel

    sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    from scbp import catalog as katalog_modul

    katalog = katalog_modul.load()
    if not (katalog.get('bauplaene') or {}):
        print('Hole den Bauplan-Katalog von scmdb.net (etwa 12 MB, einmalig) …')
        try:
            katalog_modul.update(progress=lambda x: print('  ' + str(x)))
            katalog = katalog_modul.load()
        except Exception as ausnahme:
            print('Ging nicht (%s) — es bleibt bei den Beispielen.' % ausnahme)

    bauplaene = katalog.get('bauplaene') or {}
    if bauplaene:
        # Besitz gleichmäßig verteilt, aber immer gleich: Wer den Testlauf
        # zweimal startet, soll nicht plötzlich andere Baupläne besitzen.
        import hashlib
        bestand = {}
        for schluessel, e in bauplaene.items():
            wuerfel = int(hashlib.md5(schluessel.encode()).hexdigest(), 16) % 100
            if wuerfel < 55:
                bestand[schluessel] = {'name': e.get('n') or schluessel,
                                       'quelle': 'log',
                                       'zeit': '2026-08-25 01:14:03'}
        # ⚠ NICHT flach in den Zielordner schreiben: `pfade.app_datei()`
        # sortiert die Dateien seit v3.0.0 in Unterordner (Bauplaene/,
        # Intern/, Einstellungen/). Flach abgelegt findet das Programm sie
        # nicht — die Liste bleibt leer, und es sieht aus, als fehlten die
        # Daten. Genau so ist es passiert.
        from scbp import pfade as pfade_modul
        with open(pfade_modul.app_datei('bestand.json'), 'w',
                  encoding='utf-8') as f:
            json.dump({'version': 1, 'stand': '2026-08-25 01:14:03',
                       'bauplaene': bestand}, f, ensure_ascii=False)
        print('Echter Katalog: %d Baupläne, davon %d im Testbestand'
              % (len(bauplaene), len(bestand)))
        return 0

    return _beispiele(ziel)


def arten_pruefen():
    """Stehen alle Beispiel-Arten wirklich im Katalog-Schema?

    Liefert die Arten, die `catalog.KIND_GROUP` nicht kennt und die deshalb
    in „Sonstiges" verschwinden würden — bis auf `Pattern`, das absichtlich
    dort landet, damit auch dieser Bereich etwas zu zeigen hat.

    Der Selbsttest ruft das auf. Grund: Ausgedachte Art-Namen in den Testdaten
    sehen aus wie ein Fehler der Oberfläche. Genau so ist es einmal gelaufen —
    „nur FPS-Waffen" zeigte nichts, und gesucht wurde tagelang am Filter.
    """
    sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    from scbp import catalog

    absicht = ('Pattern',)
    return sorted({art for _, art, _, _, _, _, _ in BEISPIELE
                   if art not in catalog.KIND_GROUP and art not in absicht})


def formate_pruefen():
    """Haben die Beispieldaten dieselben Formate wie der echte Katalog?

    Bisher geprüft:

    * **Gütegrad** als Zahl 1–4, nicht als Buchstabe. Stand hier 'A', schlug
      `kuerzel()` in einer Zahlen-Tabelle nach, fand nichts und schrieb „–":
      In der Liste stand „M/–/1" statt „M/1/A".
    * **Klasse** als ausgeschriebener Name („Military"), nicht als Kürzel.

    Beides ist derselbe Fehler wie die ausgedachten Art-Kennungen: Testdaten
    in einem Format, das es echt nicht gibt — und die Oberfläche sieht dann
    kaputt aus, obwohl sie stimmt.
    """
    beanstandet = []
    for name, _, klasse, grad, groesse, _, _ in BEISPIELE:
        if grad is not None and not isinstance(grad, int):
            beanstandet.append('%s: Grad %r ist keine Zahl 1–4' % (name, grad))
        if klasse is not None and len(str(klasse)) <= 2:
            beanstandet.append('%s: Klasse %r ist ein Kürzel statt des Namens'
                               % (name, klasse))
        if groesse is not None and not str(groesse).isdigit():
            beanstandet.append('%s: Größe %r ist keine Zahl' % (name, groesse))
    return beanstandet


def _beispiele(ziel):
    """Rückfall ohne Netz: ein kleiner, aber vielfältiger Ausschnitt."""
    bauplaene, bestand = {}, {}
    for name, art, klasse, grad, groesse, habe, quellen in BEISPIELE:
        k = name.lower().strip()
        e = {'n': name, 'a': art}
        if klasse:
            e['c'] = klasse
        if grad:
            e['g'] = grad
        if groesse:
            e['s'] = groesse
        if quellen:
            e['q'] = [{'fraktion': f, 'auftrag': a, 'rang': r, 'rep': rep,
                       'uec': uec, 'wo': {'system': wo, 'orte': []}}
                      for f, a, r, rep, uec, wo in quellen]
        elif name in TOPF:
            e['topf'] = TOPF[name]
        bauplaene[k] = e
        if habe:
            bestand[k] = {'name': name, 'quelle': 'log',
                          'zeit': '2026-08-25 01:14:03'}

    from scbp import pfade as pfade_modul
    with open(pfade_modul.app_datei('katalog-cache.json'), 'w',
              encoding='utf-8') as f:
        json.dump({'version': 'probe', 'geholt': '2026-08-25',
                   'bauplaene': bauplaene, 'missionen': {}}, f, ensure_ascii=False)
    with open(pfade_modul.app_datei('bestand.json'), 'w',
              encoding='utf-8') as f:
        json.dump({'version': 1, 'stand': '2026-08-25 01:14:03',
                   'bauplaene': bestand}, f, ensure_ascii=False)
    print('Probedaten: %d Baupläne, davon %d im Bestand'
          % (len(bauplaene), len(bestand)))
    return 0


if __name__ == '__main__':
    sys.exit(main())
