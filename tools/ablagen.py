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
| eine gemeinsame Liste hier als Konstante | Sie war beim Anlegen schon falsch: `selling` und `spielstand` fehlten, `mining` stand drin **ohne** Ablage |

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


def ablage_module():
    """Alle `scbp`-Module mit mindestens einer `uex.Store`-Instanz.

    Gibt Namen ohne das `scbp.`-Praefix zurueck, alphabetisch. Module, die sich
    nicht laden lassen, werden uebergangen — das faellt an anderer Stelle auf
    (Pruefung 190 haelt die Menge gegen die tatsaechlich vorhandenen Module).
    """
    import scbp
    from scbp import uex

    gefunden = []
    for eintrag in pkgutil.iter_modules(scbp.__path__):
        try:
            modul = importlib.import_module('scbp.' + eintrag.name)
        except Exception:
            continue
        if any(isinstance(getattr(modul, name, None), uex.Store)
               for name in dir(modul)):
            gefunden.append(eintrag.name)
    return tuple(sorted(gefunden))
