# -*- coding: utf-8 -*-
"""Skaliert `update()` mit der Zeilenzahl? — die Gegenprobe auf DERSELBEN Seite.

⚠⚠ Bisher wurde die Bauteil-Frage an **zwei verschiedenen** Seiten gemessen
(`joysticks` 986 Bauteile / 535 ms gegen `wasistneu` 1000 / 91 ms). Daraus
folgt nur: Die Zahl allein erklaert es nicht. Es folgt NICHT, dass die Zahl
egal ist — die Seiten unterscheiden sich in allem.

Dieser Lauf haelt alles konstant ausser der Zeilenzahl: **dieselbe Seite**,
dieselben Widget-Arten, nur halb so viele Belegungen.

Gemessen wird der **warme** Fall (Seite schon gebaut, wird nur eingeblendet)
— das ist der, der bei jedem Klick anfaellt.

## Ergebnis vom 13.09.2026 (zwei Laeufe)

| Zeilen | Bauteile | Zeit |
|---|---|---|
| 100 % | 986 | 639 / 616 ms |
| 50 % | 95 % | 86 / 91 % |
| **25 %** | **68 %** | **68 / 66 %** |

**Die Zeit skaliert linear mit der Bauteil-Zahl.** Auf dieser Seite ist die
Zahl also der Posten — eine Virtualisierung (nur sichtbare Zeilen vorhalten)
wuerde hier wirken.

⛔⛔ **Damit ist eine fruehere Schlussfolgerung von mir widerlegt.** Sie
lautete „die Bauteil-Anzahl ist es nicht" und stuetzte sich darauf, dass
`wasistneu` mit **1000** Bauteilen nur 91 ms braucht. Zwei verschiedene
Seiten unterscheiden sich aber in allem — aus ihrem Unterschied folgt nichts
ueber die Wirkung der Zahl. Genau dafuer gibt es diesen Lauf: **eine Sache
aendern, alles andere festhalten.**

⚠ Nicht zu verwechseln mit der Messung vom 12.09.2026 (zehnmal weniger
Bauteile sparen 18 ms). Die betraf das **Bauen**, nicht das **Anzeigen**.

⚠ Der Sockel bleibt: Bei 25 % der Zeilen sind noch 68 % der Bauteile da
(Kopfzeile, Rahmen, Geraeteblock). Eine echte Virtualisierung mit nur den
~30 sichtbaren Zeilen laege deutlich darunter.
"""
import os
import sys
import time

WURZEL = r'E:\GitHub\Projekte\SC-BP-Watcher'
sys.path.insert(0, WURZEL)
sys.path.insert(0, os.path.join(WURZEL, 'tools'))

import unsichtbar                                            # noqa: E402
unsichtbar.sicherstellen(messend=True)

WIEDERHOLUNGEN = 4


def _bauteile(widget):
    n = 1
    for kind in widget.winfo_children():
        n += _bauteile(kind)
    return n


def messen(anteil):
    """Die Joystick-Seite mit `anteil` der Belegungen bauen und messen."""
    # ⚠ Frisches Fenster je Durchgang: Eine einmal gebaute Seite wird nicht
    # neu gebaut, und genau darum geht es hier.
    from scbp import joysticks, hauptfenster

    echt = joysticks.sicht

    def gekuerzt(*a, **k):
        voll = echt(*a, **k) or {}
        if anteil >= 1.0:
            return voll
        heraus = {}
        for schluessel, liste in voll.items():
            try:
                heraus[schluessel] = liste[:max(1, int(len(liste) * anteil))]
            except TypeError:
                heraus[schluessel] = liste
        return heraus

    joysticks.sicht = gekuerzt
    try:
        fenster = hauptfenster.Hauptfenster(version='mess')
        fenster.root.update()
        fenster.root.update_idletasks()
        fenster.oeffnen('joysticks')
        fenster.root.update()
        fenster.root.update_idletasks()
        n = _bauteile(fenster.seiten['joysticks'])

        zeiten = []
        for _ in range(WIEDERHOLUNGEN):
            fenster.oeffnen('spiel')
            fenster.root.update()
            t0 = time.perf_counter()
            fenster.oeffnen('joysticks')
            fenster.root.update_idletasks()
            fenster.root.update()
            zeiten.append((time.perf_counter() - t0) * 1000)
        fenster.root.destroy()
        zeiten.sort()
        # ⚠ Mittelwert OHNE den hoechsten Wert: Der erste Durchgang traegt
        # noch Reste des Aufbaus. Eine Einzelmessung hat hier schon einmal
        # zu einem falschen Schluss gefuehrt.
        mittel = sum(zeiten[:-1]) / max(1, len(zeiten) - 1)
        return n, mittel, zeiten
    finally:
        joysticks.sicht = echt


def main():
    print('  %-10s %10s %12s   %s'
          % ('Anteil', 'Bauteile', 'Wechsel', 'Einzelwerte'))
    print('  ' + '-' * 66)
    werte = []
    for anteil in (1.0, 0.5, 0.25):
        n, mittel, zeiten = messen(anteil)
        werte.append((anteil, n, mittel))
        print('  %-10s %10d %9.0f ms   %s'
              % ('%d %%' % (anteil * 100), n, mittel,
                 ' '.join('%.0f' % z for z in zeiten)))

    print()
    voll_n, voll_ms = werte[0][1], werte[0][2]
    for anteil, n, ms in werte[1:]:
        if voll_n and voll_ms:
            print('  %3d %% der Zeilen -> %d %% der Bauteile, %d %% der Zeit'
                  % (anteil * 100, round(100 * n / voll_n),
                     round(100 * ms / voll_ms)))
    print('\n  Lesart: Sinkt die Zeit etwa wie die Bauteile, ist die Zahl der')
    print('  Posten. Bleibt sie stehen, liegt es woanders.')
    return 0


if __name__ == '__main__':
    sys.exit(main())
