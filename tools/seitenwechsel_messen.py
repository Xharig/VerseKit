# -*- coding: utf-8 -*-
"""Wo genau steckt die Zeit eines Seitenwechsels?

## Warum es das braucht

`tools/seiten_messen.py` sagt, **wie lange** ein Seitenwechsel dauert. Es sagt
nicht, **wo** die Zeit hingeht — und ohne das bleibt jede Verbesserung Raten.
Genau daran sind hier schon drei Erklärungen gescheitert (Schriftgröße, Zahl
der Symbolbilder, Zahl der Bauteile).

⚠⚠ **Und die Spur im Fehlerbericht misst zu kurz.** `oeffnen()` schreibt
`Seite x: steht (440 ms)` direkt nach dem Ein- und Ausblenden — danach laufen
aber noch `_reiter_faerben()`, `_leistenbreite_nachziehen()` und die
„Neu"-Marken. Wer die 440 ms für den ganzen Wechsel hält, sucht an der
falschen Stelle.

## Wie gemessen wird

**Per Wrapper, ohne den Code anzufassen.** Jede beteiligte Methode wird durch
eine zeitnehmende Hülle ersetzt, dann läuft `oeffnen()` ganz normal. So misst
das Werkzeug den echten Weg — nicht einen nachgebauten.

    python3 tools/seitenwechsel_messen.py

⚠ Gemessen wird der **warme** Fall: Die Seite ist schon gebaut und wird nur
eingeblendet. Das ist der Fall, der den Nutzer bei **jedem** Klick trifft.

## Was die erste Messung ergeben hat (13.09.2026)

| Seite | gesamt | `oeffnen()` | `update()` | Bauteile |
|---|---|---|---|---|
| joysticks | 584 ms | **35 ms** | **535 ms** | 986 |
| wasistneu | 119 ms | 11 ms | 91 ms | **1000** |

**Zwei Erklärungen sind damit erledigt:**

1. ⛔ *„`oeffnen()` ist zu langsam"* — es kostet 35 von 584 ms.
2. ⛔ *„Es hängen noch Aufbau-Aufträge im Leerlauf"* — offen sind nur drei
   Timer (`takt`, `nachziehen`), keine Bau-Aufträge.

Die Zeit steckt in `update()`, also in Tks eigenem Zeichnen.

### ⛔⛔ Und eine dritte „Erkenntnis", die FALSCH war

Hier stand zunächst, die Bauteil-Anzahl sei es nicht — begründet damit, dass
`wasistneu` mit **1000** Bauteilen nur 91 ms braucht.

**Der Vergleich beantwortet die Frage nicht.** Zwei verschiedene Seiten
unterscheiden sich in allem; aus ihrem Unterschied folgt nichts über die
Wirkung der Zahl. Die saubere Gegenprobe (`tools/zeilenzahl_messen.py`) hält
alles konstant außer der Zeilenzahl — **dieselbe** Seite:

| Zeilen | Bauteile | Zeit |
|---|---|---|
| 100 % | 986 | 639 ms |
| 50 % | 95 % | 86 % |
| **25 %** | **68 %** | **68 %** |

**Die Zeit skaliert linear mit der Bauteil-Zahl.** Auf dieser Seite ist die
Zahl also sehr wohl der Posten.

⚠ Der Unterschied zwischen den Seiten liegt in den **Kosten je Bauteil**:
542 µs bei `joysticks` gegen 91 µs bei `wasistneu`. Woran das liegt, ist
offen — Tiefe (beide 8), Typ und Umbruch erklären es nicht. Das wird hier
**nicht geraten**.

⚠⚠ **Nicht zu verwechseln mit der Messung vom 12.09.2026**, wonach zehnmal
weniger Bauteile nur 18 ms sparen. Die betraf das **Bauen** einer Seite.
Hier geht es ums **Anzeigen** einer bereits gebauten — ein anderer Vorgang
mit anderem Ergebnis.
"""
import os
import sys
import time

HIER = os.path.dirname(os.path.abspath(__file__))
WURZEL = os.path.dirname(HIER)
sys.path.insert(0, WURZEL)
sys.path.insert(0, HIER)

import unsichtbar                                            # noqa: E402
unsichtbar.sicherstellen(messend=True)

from scbp import hauptfenster                                # noqa: E402

# Die Methoden, die `oeffnen()` der Reihe nach ruft.
SCHRITTE = ('_aktion_merken', '_gruppe_von_reiter_oeffnen',
            '_seite_fuellen', '_reiter_faerben',
            '_leistenbreite_nachziehen')

WIEDERHOLUNGEN = 3


def _huellen(fenster, konto):
    """Jede Methode aus SCHRITTE durch eine zeitnehmende Huelle ersetzen."""
    echt = {}
    for name in SCHRITTE:
        if not hasattr(fenster, name):
            continue
        echt[name] = getattr(fenster, name)

        def mach(name=name, ruf=echt[name]):
            def huelle(*a, **k):
                start = time.perf_counter()
                try:
                    return ruf(*a, **k)
                finally:
                    konto[name] = konto.get(name, 0.0) + (
                        time.perf_counter() - start) * 1000
            return huelle

        setattr(fenster, name, mach())
    return echt


def main():
    fenster = hauptfenster.Hauptfenster(version='mess')
    fenster.root.update()
    fenster.root.update_idletasks()

    from abnahme import SEITEN
    kennungen = [name for name, _titel in SEITEN]

    # ⚠ Erst ALLE Seiten einmal bauen — gemessen wird nur der warme Fall.
    for kennung in kennungen:
        try:
            fenster.oeffnen(kennung)
            fenster.root.update()
        except Exception as fehler:
            print('  !! %s: %s' % (kennung, fehler))
    fenster.root.update_idletasks()

    ergebnis = []
    for kennung in kennungen:
        if kennung not in fenster.gezeichnet:
            continue
        summe = {}
        for _ in range(WIEDERHOLUNGEN):
            # Auf eine andere Seite und zurueck — sonst misst man nichts.
            andere = next((k for k in kennungen if k != kennung), None)
            if andere:
                fenster.oeffnen(andere)
                fenster.root.update()

            konto = {}
            echt = _huellen(fenster, konto)
            try:
                t0 = time.perf_counter()
                fenster.oeffnen(kennung)
                t_oeffnen = (time.perf_counter() - t0) * 1000

                t0 = time.perf_counter()
                fenster.root.update_idletasks()
                t_idle = (time.perf_counter() - t0) * 1000

                t0 = time.perf_counter()
                fenster.root.update()
                t_update = (time.perf_counter() - t0) * 1000
            finally:
                for name, ruf in echt.items():
                    setattr(fenster, name, ruf)

            # Der Rueckruf `beim_zeigen` laeuft INNERHALB von `oeffnen`, hat
            # aber keine eigene Methode — er wird separat nachgemessen.
            ruf = (getattr(fenster, 'beim_zeigen', {}) or {}).get(kennung)
            t0 = time.perf_counter()
            if ruf:
                try:
                    ruf()
                except Exception:
                    pass
            t_rueckruf = (time.perf_counter() - t0) * 1000

            for name, wert in konto.items():
                summe[name] = summe.get(name, 0.0) + wert
            for name, wert in (('oeffnen gesamt', t_oeffnen),
                               ('update_idletasks', t_idle),
                               ('update', t_update),
                               ('davon Rueckruf', t_rueckruf)):
                summe[name] = summe.get(name, 0.0) + wert

        mittel = {k: v / WIEDERHOLUNGEN for k, v in summe.items()}
        ergebnis.append((mittel.get('oeffnen gesamt', 0)
                         + mittel.get('update_idletasks', 0)
                         + mittel.get('update', 0), kennung, mittel))

    # ⭐⭐ **Bauteile zaehlen, bevor irgendein Schluss gezogen wird.**
    # Die Tabelle sagt, dass `update()` teuer ist. Sie sagt NICHT, wovon.
    # Ohne die Zahl daneben waere „das sind die vielen Widgets" wieder nur
    # eine Erklaerung — davon sind hier schon drei widerlegt worden.
    def _bauteile(widget):
        anzahl = 1
        for kind in widget.winfo_children():
            anzahl += _bauteile(kind)
        return anzahl

    ergebnis.sort(reverse=True)
    kopf = ('oeffnen gesamt', 'davon Rueckruf', 'update_idletasks', 'update',
            '_reiter_faerben', '_leistenbreite_nachziehen')
    print('\n  %-16s %8s %8s %9s %8s %8s %9s %7s'
          % ('Seite', 'gesamt', 'oeffnen', 'Rueckruf', 'idle', 'update',
             'Bauteile', 'µs/St.'))
    print('  ' + '-' * 82)
    for gesamt, kennung, m in ergebnis[:14]:
        try:
            n = _bauteile(fenster.seiten[kennung])
        except Exception:
            n = 0
        je = (m.get(kopf[3], 0) * 1000 / n) if n else 0
        print('  %-16s %6.0fms %6.0fms %7.0fms %6.0fms %6.0fms %7d %7.0f'
              % (kennung, gesamt, m.get(kopf[0], 0), m.get(kopf[1], 0),
                 m.get(kopf[2], 0), m.get(kopf[3], 0), n, je))

    # ⭐⭐ Und die Frage dahinter: **Was** laeuft im Leerlauf, wenn `oeffnen()`
    # laengst zurueck ist? Die Tabelle oben sagt nur, DASS `update()` teuer
    # ist — `update()` arbeitet naemlich alle faelligen Rueckrufe ab, nicht
    # nur die dieser Seite.
    print('\n  Was nach einem Wechsel noch eingeplant ist:')
    for kennung in ('joysticks', 'zerlegen', 'liste'):
        if kennung not in fenster.gezeichnet:
            continue
        andere = next((k for k in kennungen if k != kennung), None)
        if andere:
            fenster.oeffnen(andere)
            fenster.root.update()
        fenster.oeffnen(kennung)
        offen = fenster.root.tk.splitlist(fenster.root.tk.call('after',
                                                               'info'))
        namen = []
        for kennnummer in offen:
            try:
                art, ruf = fenster.root.tk.splitlist(
                    fenster.root.tk.call('after', 'info', kennnummer))[:2]
                namen.append('%s/%s' % (art, str(ruf)[:44]))
            except Exception:
                namen.append('?')
        print('    %-12s %d offen' % (kennung, len(offen)))
        for n in namen[:6]:
            print('        %s' % n)
        fenster.root.update()

    # ⚠ **Die Spalte „Rueckruf" ist eine ZWEITE Messung**, kein Anteil an
    # `oeffnen`. Der Rueckruf laeuft einmal in `oeffnen()` und wird danach
    # noch einmal allein gerufen — nur so laesst sich seine Zeit von der
    # Umgebung trennen. Bei `zerlegen` ist der zweite Aufruf teurer als der
    # erste; die Seite schwankt ohnehin zwischen 1,7 und 3,3 Sekunden.
    fenster.root.destroy()
    return 0


if __name__ == '__main__':
    sys.exit(main())
