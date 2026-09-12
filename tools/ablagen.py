# SPDX-License-Identifier: GPL-3.0-only
#
# Copyright (C) 2026 Xharig
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License version 3 as
# published by the Free Software Foundation.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
"""Welche Module einen Zwischenspeicher fuehren — **ermittelt, nicht gepflegt**.

⚠⚠ **Eine Liste von Hand war schon der dritte Fehlversuch.** Die Geschichte
dieser Datei ist eine Kette von Reparaturen, die jeweils die vorige ersetzt
haben:

| Versuch | Warum er scheiterte |
|---|---|
| Namen direkt in `abnahme.py` | Nach jeder Umbenennung standen dort tote Namen — `'bergbau'`, `'schiffe'`, zuletzt `'orte'`/`'routen'`, jedes Mal monatelang unbemerkt |
| den Quelltext nach `'scbp.' + x` durchsuchen | f-Strings, `.format()`, ausgelagerte Listen und Comprehensions fielen durch — vier von fuenf Schreibweisen |
| eine gemeinsame Liste hier als Konstante | Sie war beim Anlegen schon falsch: `selling` und `gamebuild` fehlten, `mining` stand drin **ohne** Ablage |

⭐⭐ Der dritte Fehlschlag ist der lehrreiche: **Eine gemeinsame Quelle
verhindert auseinanderlaufende Kopien — gemeinsame Auslassungen verhindert sie
nicht.** Beide Seiten lasen dieselbe Liste, und die war eben falsch.

Deshalb wird hier nichts mehr aufgezaehlt, sondern **gemessen**: Welches Modul
eine `uex.Store`-Instanz traegt, fuehrt einen Zwischenspeicher. Das kann nicht
veralten, weil es keine Namen kennt.

⚠ Diese Datei darf importieren — `pkgutil` und `importlib` sind harmlos. Was
sie NICHT darf, ist beim Import Nebenwirkungen ausloesen. Genau daran ist der
Vorgaenger gescheitert: `abnahme.py` ruft auf Modulebene
`unsichtbar.sicherstellen()`, und wer es nur importierte, nahm dem restlichen
Prueflauf die Fenster-Geometrie. Pruefung 190 misst das inzwischen direkt —
sie prueft die **Wirkung** (bleibt ein Fenster messbar?), nicht die Form.
"""

import importlib
import pkgutil


def ablage_module(mit_fehlern=False):
    """Alle `scbp`-Module mit mindestens einer `uex.Store`-Instanz.

    Gibt Namen ohne das `scbp.`-Praefix zurueck, alphabetisch. Mit
    `mit_fehlern=True` zusaetzlich die Module, die sich **nicht laden liessen**
    — als `(namen, fehler)`.

    ⚠⚠ **Ein uebersprungener Import ist ein BEFUND, kein Nebenergebnis.**
    Die erste Fassung verschluckte ihn mit `except Exception: continue`, und
    die Begruendung dazu war falsch: „faellt an anderer Stelle auf". Faellt es
    nicht. Pruefung 190 vergleicht diese Menge mit einer zweiten Ermittlung —
    die denselben Weg geht und denselben Import ueberspringt. Scheitert also
    `selling`, fehlt es in **beiden** Mengen, der Vergleich bleibt gruen, und
    die Importpruefung danach sieht den Namen gar nicht erst.

    Zwei Ermittlungen mit demselben Algorithmus sind keine zwei Ermittlungen.
    Deshalb wird der Fehlschlag jetzt nach oben gereicht.
    """
    import scbp
    from scbp import uex

    gefunden = []
    fehler = []
    for eintrag in pkgutil.iter_modules(scbp.__path__):
        try:
            modul = importlib.import_module('scbp.' + eintrag.name)
        except Exception as ausnahme:
            fehler.append('%s (%s)' % (eintrag.name,
                                       type(ausnahme).__name__))
            continue
        if any(isinstance(getattr(modul, name, None), uex.Store)
               for name in dir(modul)):
            gefunden.append(eintrag.name)
    if mit_fehlern:
        return tuple(sorted(gefunden)), tuple(sorted(fehler))
    return tuple(sorted(gefunden))
