# -*- coding: utf-8 -*-
"""Gegenprobe zu den vier Befunden vom 12.09.2026 (Runde 3).

Was hier geprueft wird, laesst der Selbsttest nicht zu: Er kennt keinen
echten Spielordner, und die Fragen sind alle vom Typ „passiert das WIRKLICH
nur einmal" — dafuer braucht es gezaehlte Aufrufe statt Rueckgabewerte.

| # | Befund | Frage |
|---|---|---|
| 1 | Merker-Schluessel ueberschrieben | Liest der zweite Aufruf die INI noch einmal? |
| 2 | Marke sieht die falschen Dateien | Aendert sich `Data.p4k` oder die englische INI mit? |
| 3 | Zeitstempel auf Sekunden gerundet | Fallen `.1` und `.9` derselben Sekunde zusammen? |

    python3 tools/probe_abdruecke.py

⚠ Sie legt sich ihren Spielordner selbst an (`tempfile`) — kein Zugriff auf
eine echte Installation, keine Nutzerdaten.
"""
import os
import shutil
import sys
import tempfile

WURZEL = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, WURZEL)

from scbp import joysticks                                   # noqa: E402

OK = []


def p(bedingung, text):
    OK.append((bool(bedingung), text))
    print('  [%s] %s' % ('ok' if bedingung else 'XX', text))


def spielordner_bauen(wurzel):
    """Ein Minimal-Spiel: das Archiv und zwei Uebersetzungsdateien."""
    with open(os.path.join(wurzel, 'Data.p4k'), 'wb') as f:
        f.write(b'PK\x00\x00nicht echt')
    for name in ('german_(germany)', 'english'):
        ordner = os.path.join(wurzel, 'data', 'Localization', name)
        os.makedirs(ordner)
        with open(os.path.join(ordner, 'global.ini'), 'w',
                  encoding='utf-8') as f:
            f.write('ui_CIEject=%s\n' % name)


def main():
    tmp = tempfile.mkdtemp(prefix='probe-abdruck-')
    try:
        spielordner_bauen(tmp)
        de_ini = os.path.join(tmp, 'data', 'Localization',
                              'german_(germany)', 'global.ini')
        en_ini = os.path.join(tmp, 'data', 'Localization',
                              'english', 'global.ini')
        p4k = os.path.join(tmp, 'Data.p4k')

        # ── 1. Trifft der Merker beim zweiten Mal? ────────────────────────
        #
        # Gezaehlt wird, wie oft die INI wirklich gelesen wird. Ein
        # Rueckgabewert wuerde hier nichts verraten — er ist in beiden
        # Faellen derselbe.
        gelesen = [0]
        echt_ini = joysticks._ini_texte
        echt_profil = joysticks._profil

        def zaehl_ini(sprache, spielordner=None):
            gelesen[0] += 1
            return echt_ini(sprache, spielordner)

        # ⚠ Ohne Etiketten laeuft die Schleife gar nicht, und genau in ihr
        # steckte der Fehler — der echte `_profil()` kann aus einem
        # Wegwerf-Archiv nichts holen, also wird er ersetzt.
        joysticks._ini_texte = zaehl_ini
        joysticks._profil = lambda spielordner=None: {
            'etiketten': {'v_eject': ['@ui_CIEject', '@ui_CIEjectDesc'],
                          'v_flare': ['@ui_CIFlare', '']},
            'standard': {}, 'gruppen': {}}
        try:
            joysticks.vergessen()
            joysticks.klarnamen('de', tmp)
            erste = gelesen[0]
            joysticks.klarnamen('de', tmp)
            p(gelesen[0] == erste,
              'zweiter Aufruf liest nichts nach (%d -> %d Lesevorgaenge)'
              % (erste, gelesen[0]))
            p(all(isinstance(s, tuple) and len(s) == 2
                  for s in joysticks._KLARNAMEN),
              'abgelegt unter (Sprache, Marke) — nicht unter einem Etikett '
              '(%r)' % [type(s).__name__ for s in joysticks._KLARNAMEN])
        finally:
            joysticks._ini_texte = echt_ini
            joysticks._profil = echt_profil
            joysticks.vergessen()

        # ── 2. Sieht die Marke die echten Quellen? ────────────────────────
        vorher = joysticks._quellenmarke('de', tmp)
        p(joysticks._quellenmarke('de', tmp) == vorher,
          'nichts geaendert -> gleiche Marke')

        os.utime(p4k, ns=(1, 1))
        p(joysticks._quellenmarke('de', tmp) != vorher,
          'Data.p4k geaendert -> andere Marke')

        # Die englische INI zaehlt AUCH bei deutscher Oberflaeche: Sie
        # fuellt jede Luecke der Uebersetzung.
        vorher = joysticks._quellenmarke('de', tmp)
        with open(en_ini, 'a', encoding='utf-8') as f:
            f.write('ui_Neu=neu\n')
        p(joysticks._quellenmarke('de', tmp) != vorher,
          'englische global.ini geaendert -> andere Marke (bei Sprache de)')

        vorher = joysticks._quellenmarke('de', tmp)
        with open(de_ini, 'a', encoding='utf-8') as f:
            f.write('ui_Neu=neu\n')
        p(joysticks._quellenmarke('de', tmp) != vorher,
          'deutsche global.ini geaendert -> andere Marke')

        p(joysticks._quellenmarke('de', tmp)
          != joysticks._quellenmarke('en', tmp),
          'andere Sprache -> andere Marke')

        # ── 3. Gleiche Groesse, gleiche Sekunde ──────────────────────────
        with open(de_ini, 'w', encoding='utf-8') as f:
            f.write('ui_A=1\n')
        os.utime(de_ini, ns=(1_000_100_000_000, 1_000_100_000_000))
        eins = joysticks._quellenmarke('de', tmp)
        with open(de_ini, 'w', encoding='utf-8') as f:
            f.write('ui_B=2\n')            # gleiche Laenge, anderer Inhalt
        os.utime(de_ini, ns=(1_000_900_000_000, 1_000_900_000_000))
        p(joysticks._quellenmarke('de', tmp) != eins,
          'gleiche Groesse, .1 und .9 derselben Sekunde -> andere Marke')
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    print('\n  %d von %d' % (sum(1 for b, _ in OK if b), len(OK)))
    return 0 if all(b for b, _ in OK) else 1


if __name__ == '__main__':
    sys.exit(main())
