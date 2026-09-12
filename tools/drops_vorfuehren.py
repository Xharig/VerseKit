# -*- coding: utf-8 -*-
"""Lässt Bauplan-Funde einlaufen — zum Vorführen und für die Bildschirmfotos.

**Wozu.** Für die Bilder in der Anleitung muss man dem Werkzeug bei der Arbeit
zusehen können. Ohne laufendes Star Citizen passiert aber nichts: Die Melde-Leiste
bleibt leer, und ein leeres Overlay erklärt niemandem, wozu es gut ist.

Dieses Skript schreibt echte Fund-Zeilen in eine Wegwerf-`Game.log`, so wie das
Spiel es täte. Der Watcher liest sie im selben Moment und meldet sie — es gibt
keinen Sonderweg und keinen Testmodus im Programm selbst. Was hier zu sehen ist,
ist genau das, was auch beim Spielen passiert.

**Alle vier Zustände auf einmal**, damit das Bild die Zeichen-Erklärung in der
Anleitung vollständig bebildert:

| Zeichen | Zustand | Wie es hier entsteht |
|---|---|---|
| gelb | vorläufig, nur aus der Log | ein Fund ohne Launcher-Bestätigung |
| grün | bestätigt | ein Fund, den der Launcher-Export kennt |
| Stern | von der Merkliste | ein Fund, der vorher vorgemerkt wurde |
| blau | neu im Spiel craftbar | kommt vom Katalog selbst, echte Patch-Daten |

**Die Namen sind echt** und stammen aus dem Katalog — und zwar aus dem, was
der Autor *nicht* hat. Ausgedachte Namen wären hier schädlich: Am Bildschirmfoto
sähe man ihnen nichts an, aber ein Leser, der sie nachschlägt, findet nichts.

**Aufruf** — in dieser Reihenfolge:

    python3 tools/drops_vorfuehren.py --vorbereiten   # Merkliste, Wegwerf-Ordner
    #  … jetzt den Watcher starten …
    python3 tools/drops_vorfuehren.py                 # die Funde laufen ein

Der erste Schritt muss **vor** dem Watcher-Start laufen: Die Merkliste wird
beim Start eingelesen, eine spätere Änderung sieht er nicht mehr.

Zum Aufräumen hinterher:

    python3 tools/drops_vorfuehren.py --aufraeumen
"""

import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# ⚠ **Muss vor dem Import von `scbp` stehen.** `pfade` liest `SC_BP_HOME` beim
# Laden aus; wird die Variable danach gesetzt, wirkt sie nicht mehr.
#
# Und sie muss überhaupt gesetzt werden: Die Startdatei auf dem Desktop legt den
# Watcher auf diese Test-Ablage. Läuft dieses Skript ohne die Variable, schreibt
# es in die **normale** Ablage — und der Watcher sieht nichts, ohne dass ein
# Fehler erscheint. Genau daran ist der erste Versuch am 27.08.2026 gescheitert.
TEST_ABLAGE = os.path.join(os.path.expanduser('~'), 'Documents',
                           'SC BP Watcher Test')
if not os.environ.get('SC_BP_HOME') and os.path.isdir(TEST_ABLAGE):
    os.environ['SC_BP_HOME'] = TEST_ABLAGE

from scbp import watchlist, pfade                      # noqa: E402

# Ein Wegwerf-Spielordner neben der Ablage des Testlaufs — nicht im Projekt,
# damit nichts davon je in einen Commit rutscht.
ORDNER = os.path.join(os.path.expanduser('~'), 'Documents',
                      'SC BP Watcher Test', 'Spiel-Vorfuehrung')

# Der Blueprint-Ordner des SC Deutsch Launchers auf der NAS. Nur lesend benutzt.
LAUNCHER = ('/Volumes/06_Spiele/PC Games/Star Citizen/SC-Deutsch-Launcher/'
            'blueprints')

# Die Zeile, die Star Citizen beim Freischalten schreibt. Wortlaut aus
# `scbp/phrasen.py` — wird der dort geändert, muss er hier mitziehen.
ZEILE = ('<%s> [Notice] <SHUDEvent_OnNotification> Added notification '
         '"Bauplan erhalten: %s: " [136]\n')

# Echte Baupläne aus dem Katalog, die der Autor noch nicht hat.
# Bewusst verschiedene Arten — eine Liste aus lauter Schilden sieht aus wie ein
# Fehler, nicht wie ein Spielabend.
# Das dritte Feld sagt, ob der Bauplan **im Launcher-Export steht**. Daran hängt
# die Farbe, und deshalb sind hier beide Sorten vertreten:
#
#   False → steht nicht im Export → bleibt **gelb** („vorläufig, aus der Log")
#   True  → steht im Export       → wird **grün** („bestätigt")
#
# ⚠ Der gelbe Zustand entsteht nur, wenn der Watcher überhaupt einen Launcher
# kennt. Ohne einen meldet er jeden Log-Fund sofort als gesichert — es gibt ja
# keine zweite Stelle, die noch bestätigen könnte. Siehe `launcher_anschliessen`.
FUNDE = [
    ('Aufeis', 'Cooler', False),
    ('CF-337 Panther Repeater', 'Schiffswaffe', False),
    ('10-Series Greatsword Cannon', 'Schiffswaffe', True),
]
# Der hier wird vorher auf die Merkliste gesetzt und kommt deshalb in Gold
# mit Stern — der vierte Zustand.
GEMERKT = ('Arclight "Midnight" Pistol', 'FPS-Waffe', False)

# ⚠ Zwei Patch-Zugänge — die blauen Meldungen der Katalog-Wache. Sie entstehen
# **nicht** aus der Log, sondern aus dem Vergleich mit `catalog-seen.json`: Was
# dort fehlt, gilt als frisch craftbar. Deshalb genügt es, zwei Namen aus dem
# Vergleichsstand herauszunehmen — beim nächsten Blick meldet der Watcher sie
# von selbst. Zwei reichen; eine lange blaue Reihe erschlägt die Funde daneben.
PATCH_ZUGAENGE = 2


def _stempel():
    return time.strftime('%Y-%m-%dT%H:%M:%S.000Z', time.gmtime())


def einrichten():
    """Wegwerf-Spielordner anlegen und dem Watcher als Spielordner geben."""
    os.makedirs(os.path.join(ORDNER, 'logbackups'), exist_ok=True)
    log = os.path.join(ORDNER, 'Game.log')
    if not os.path.exists(log):
        with open(log, 'w', encoding='utf-8') as f:
            f.write('<%s> [Notice] <Legacy> Spiel gestartet\n' % _stempel())
    vorher = pfade.einstellung('spiel_ordner')
    if vorher != ORDNER:
        pfade.einstellung_setzen('spiel_ordner', ORDNER)
        print('  Spielordner eingetragen: %s' % ORDNER)
    return log


def launcher_anschliessen():
    """Den Launcher-Export auf der NAS als Launcher eintragen — wenn er da ist.

    ⚠ Ohne das gibt es **kein Gelb**. Der Watcher meldet einen Log-Fund nur dann
    als „vorläufig", wenn eine zweite Stelle ihn bestätigen könnte; kennt er
    keinen Launcher, ist der Fund sofort gesichert. Am 27.08.2026 war die NAS
    eingehängt und die Leiste trotzdem ganz grün — weil die Startdatei den Export
    nur **einliest**, statt ihn als Launcher anzumelden. Zwei verschiedene Dinge.
    """
    if not os.path.isdir(LAUNCHER):
        print('  · NAS nicht eingehängt — ohne Launcher bleibt alles grün')
        print('    (%s)' % LAUNCHER)
        return False
    if pfade.einstellung('launcher_ordner') != LAUNCHER:
        pfade.einstellung_setzen('launcher_ordner', LAUNCHER)
        print('  Launcher angemeldet: %s' % LAUNCHER)
    return True


def _bestand_freimachen():
    """Die vorgeführten Funde aus dem Bestand nehmen.

    Wer schon drinsteht, wird nicht noch einmal gemeldet — und gerade die
    Baupläne, die den grünen Zustand zeigen sollen, stehen ja im Export und
    damit im Bestand.
    """
    from scbp import collection as bd
    daten = bd.load()
    weg = []
    for name, _, _ in FUNDE + [GEMERKT]:
        if bd.contains(daten, name):
            bd.remove(daten, name)
            weg.append(name)
    if weg:
        bd.save(daten)
        print('  Aus dem Bestand genommen: %s' % ', '.join(w[:22] for w in weg))


def _katalogstand_beschneiden():
    """Zwei Namen aus dem Vergleichsstand nehmen — sie gelten dann als neu.

    ⚠ Der Watcher prüft das beim ersten Durchlauf nach dem Start und danach nur
    noch jede Minute. Deshalb gehört das **vor** den Start; hinterher wartet man
    im Zweifel eine Minute und weiß nicht, warum nichts kommt.
    """
    import json
    stand = pfade.app_datei('catalog-seen.json')
    sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    import sc_bp_watcher as app
    alle = sorted(app.load_types() or {})
    if len(alle) < 10:
        print('  ! Katalog noch nicht geholt — keine Patch-Zugänge möglich')
        return
    # Von hinten nehmen: irgendwelche zwei, aber immer dieselben, damit ein
    # zweiter Durchlauf dasselbe Bild ergibt.
    fehlen = alle[-PATCH_ZUGAENGE:]
    bekannt = [n for n in alle if n not in fehlen]
    with open(stand, 'w', encoding='utf-8') as f:
        json.dump({'stand': time.strftime('%Y-%m-%d %H:%M:%S'),
                   'namen': bekannt}, f, ensure_ascii=False, indent=1)
    print('  Als Patch-Zugang vorgemerkt: %s' % ', '.join(fehlen))


def vorbereiten():
    """Alles, was **vor** dem Watcher-Start passieren muss.

    ⚠ Die Merkliste liest der Watcher beim Start ein. Wer sie ändert, während er
    schon läuft, ändert nichts an dem, was er sieht — der vorgemerkte Fund käme
    dann ohne Stern durch. Deshalb dieser eigene Schritt.
    """
    einrichten()
    launcher_anschliessen()
    _bestand_freimachen()
    _katalogstand_beschneiden()
    if not watchlist.contains(GEMERKT[0]):
        # ⚠ `add()` gibt die geänderten Daten nur **zurück**, es
        # speichert sie nicht — das steht so in seinem Docstring. Ohne
        # `save()` dahinter passiert nichts, und der Fund käme ohne Stern.
        watchlist.save(watchlist.add(GEMERKT[0]))
        print('  Auf die Merkliste gesetzt: %s' % GEMERKT[0])
    else:
        print('  Steht schon auf der Merkliste: %s' % GEMERKT[0])
    print('\n  Jetzt den Watcher starten, dann:')
    print('    python3 tools/drops_vorfuehren.py')


def vorfuehren():
    log = einrichten()

    if not watchlist.contains(GEMERKT[0]):
        print('  ⚠ %s steht nicht auf der Merkliste.' % GEMERKT[0])
        print('    Der Fund kommt dann ohne Stern. Erst vorbereiten:')
        print('      python3 tools/drops_vorfuehren.py --vorbereiten')
        print('    (und den Watcher danach neu starten)\n')

    print('\n  ⚠ Der Watcher muss jetzt laufen — sonst schreibt das hier ins Leere.')
    print('  Die Funde kommen einzeln, mit Pause dazwischen.\n')
    time.sleep(3)

    reihe = FUNDE + [GEMERKT]
    with open(log, 'a', encoding='utf-8') as f:
        for i, (name, art, im_export) in enumerate(reihe, 1):
            f.write(ZEILE % (_stempel(), name))
            f.flush()
            os.fsync(f.fileno())          # sonst hängt die Zeile im Puffer
            print('  %d/%d  %-30s %-14s %s' % (
                i, len(reihe), name[:30], art,
                'wird grün' if im_export else 'bleibt gelb'))
            time.sleep(4)

    print('\n  Fertig. In der Melde-Leiste stehen jetzt %d Funde,'
          % len(reihe))
    print('  darunter einer in Gold mit Stern (%s).' % GEMERKT[0])
    print('\n  Hinterher aufräumen:')
    print('    python3 tools/drops_vorfuehren.py --aufraeumen')


def aufraeumen():
    """Den Vorführ-Stand zurücknehmen — Merkliste, Spielordner, Log."""
    if watchlist.contains(GEMERKT[0]):
        watchlist.save(watchlist.remove(GEMERKT[0]))
        print('  Von der Merkliste genommen: %s' % GEMERKT[0])
    if pfade.einstellung('launcher_ordner') == LAUNCHER:
        pfade.einstellung_setzen('launcher_ordner', '')
        print('  Launcher-Eintrag zurückgenommen')
    if pfade.einstellung('spiel_ordner') == ORDNER:
        pfade.einstellung_setzen('spiel_ordner', '')
        print('  Spielordner-Eintrag zurückgenommen')
    log = os.path.join(ORDNER, 'Game.log')
    if os.path.exists(log):
        os.remove(log)
        print('  Wegwerf-Log gelöscht')
    # ⚠ Die vorgeführten Funde **aus dem Bestand nehmen**. Sonst meldet der
    # Watcher sie beim nächsten Durchlauf nicht mehr — er kennt sie ja bereits,
    # und die Vorführung liefe ins Leere, ohne dass man den Grund sähe.
    from scbp import collection as bd
    daten = bd.load()
    weg = []
    for name, _, _ in FUNDE + [GEMERKT]:
        if bd.contains(daten, name):
            bd.remove(daten, name)
            weg.append(name)
    if weg:
        bd.save(daten)
        print('  Aus dem Bestand genommen: %d (%s)'
              % (len(weg), ', '.join(w[:18] for w in weg)))

    # ⚠ Und den Lesestand zurücksetzen. Er merkt sich, bis wohin die Log gelesen
    # wurde — ohne das würden dieselben Zeilen beim nächsten Mal übersprungen.
    stand = pfade.app_datei('logstand.json')
    if os.path.exists(stand):
        os.remove(stand)
        print('  Lesestand zurückgesetzt')

    # Den Katalogstand wieder vollständig machen, sonst meldet der Watcher die
    # zwei bei jedem Start erneut.
    katstand = pfade.app_datei('catalog-seen.json')
    if os.path.exists(katstand):
        os.remove(katstand)
        print('  Katalogstand zurückgesetzt (wird beim nächsten Start neu gesetzt)')

    print('\n  Wiederholbar: --vorbereiten, Watcher starten, dann ohne Schalter.')


if __name__ == '__main__':
    if '--aufraeumen' in sys.argv:
        aufraeumen()
    elif '--vorbereiten' in sys.argv:
        vorbereiten()
    else:
        vorfuehren()
