"""Prüft den Selbst-Update-Weg — **ohne** eine neue Version zu brauchen.

Das Problem beim Testen einer Update-Funktion: Der Fix steckt immer erst in der
Version *nach* der kaputten. Aus einer defekten Version heraus lässt sich also
nicht prüfen, ob die Reparatur wirkt.

Dieses Werkzeug umgeht das: Es lädt die **eigene** Version noch einmal herunter
und spielt sie ein. Am Ergebnis ändert sich nichts — geprüft wird der *Weg*:
Kommt die Datei an? Liegt sie auf demselben Dateisystem? Läuft das Einspielen
durch, ohne an „[Errno 18] Invalid cross-device link" zu scheitern?

    python3 tools/update_pruefen.py            # nur nachsehen, nichts einspielen
    python3 tools/update_pruefen.py --echt     # auch wirklich einspielen
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from scbp import updater                                   # noqa: E402

APPIMAGE = os.path.expanduser('~/Programme/SC-BP-Watcher.AppImage')


def main():
    echt = '--echt' in sys.argv
    # ⚠ **Nicht** `setdefault`. Genau daran ist dieses Werkzeug beim ersten Lauf
    # gescheitert: `APPIMAGE` war bereits gesetzt — auf ein **fremdes** Programm,
    # weil der Aufruf aus einer Anwendung heraus kam, die selbst ein AppImage ist.
    # `setdefault` ließ den fremden Wert stehen, und `--echt` hat die fremde Datei
    # überschrieben. Hier wird der Wert deshalb hart gesetzt.
    os.environ['APPIMAGE'] = APPIMAGE
    os.environ['APPDIR'] = os.path.dirname(
        os.path.dirname(os.path.abspath(__file__)))
    if not os.path.exists(APPIMAGE):
        print('Kein AppImage unter %s — nichts zu prüfen.' % APPIMAGE)
        return 1
    print('Laufende Datei :', APPIMAGE)
    print('Verpackung     :', updater.verpackung())

    freigabe = updater.neueste(True)
    if not freigabe:
        updater.nachsehen('0.0.0', erzwingen=True)
        freigabe = updater.neueste(True)
    if not freigabe:
        print('Keine Freigabe gefunden — ohne Netz geht das nicht.')
        return 1
    print('Neueste Version:', freigabe.get('version'))

    datei = updater.passende_datei(freigabe, art='appimage')
    if not datei:
        print('Keine passende Datei in der Freigabe.')
        return 1
    print('Datei          : %s (%.1f MB)'
          % (datei['name'], (datei.get('groesse') or 0) / 1048576))

    ort = updater._ablageort_fuer_update(datei['name'])
    print('\nWohin geladen wird:', ort)
    gleiches = (os.stat(os.path.dirname(ort)).st_dev
                == os.stat(os.path.dirname(APPIMAGE)).st_dev)
    print('  gleiches Dateisystem wie das Ziel:', 'JA' if gleiches else 'NEIN')
    if not gleiches:
        print('  ⚠ Dann greift beim Einspielen der Umweg über shutil.move.')

    print('\nLade herunter …')
    ziel = updater.herunterladen(
        datei, fortschritt=lambda p: print('\r  %3d %%' % p, end='', flush=True),
        freigabe=freigabe)
    print('\r  fertig: %s (%.1f MB)' % (ziel, os.path.getsize(ziel) / 1048576))

    if not echt:
        os.remove(ziel)
        print('\nNur nachgesehen — die geladene Datei ist wieder weg.')
        print('Mit --echt wird auch eingespielt.')
        return 0

    print('\nSpiele ein …')
    geklappt, grund = updater.einspielen(ziel)
    print('  Ergebnis:', 'geklappt' if geklappt else 'FEHLER: %s' % grund)
    if geklappt:
        print('  Datei jetzt: %.1f MB, ausführbar: %s'
              % (os.path.getsize(APPIMAGE) / 1048576,
                 os.access(APPIMAGE, os.X_OK)))
    return 0 if geklappt else 1


if __name__ == '__main__':
    sys.exit(main())
