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
"""Welche Module einen Zwischenspeicher fuehren — **eine einzige Quelle**.

⚠⚠ **Diese Datei importiert absichtlich NICHTS.** Sie ist genau deshalb
entstanden: `tools/abnahme.py` ruft auf Modulebene `unsichtbar.sicherstellen()`
auf (ohne `messend`), und das versteckt jedes weitere Fenster. Wer `abnahme`
nur importiert, um an diese Liste zu kommen, legt damit dem restlichen Lauf die
Geometrie lahm — ein verstecktes Fenster meldet `winfo_width() == 1`, und jede
Layout-Pruefung misst danach Unsinn.

Genau das ist am 12.09.2026 passiert: Pruefung 190 importierte `abnahme`, und
drei Pruefungen spaeter fand Pruefung 189 keine Rollleiste mehr. Belegt ueber
einen eigenen Arbeitsbaum auf dem Stand davor — dort 2101 gruen, hier drei rot.
Dieselbe Falle hatte am 07.09.2026 schon den halben Selbsttest gekostet.

**Wer hier etwas hinzufuegt, fuegt keine Importe hinzu.**

Warum die Liste ueberhaupt geteilt wird: Nach jeder Umbenennung standen hier
tote Namen — erst `'bergbau'`, dann `'schiffe'`, zuletzt `'orte'` und
`'routen'`. Ein Name in einer Zeichenkette wird von keiner Importpruefung
gesehen. Ein Versuch, ihn im Quelltext zu SUCHEN, ist am 12.09.2026 ebenfalls
gescheitert: f-Strings, `.format()`, ausgelagerte Listen und Comprehensions
fielen alle durch — vier von fuenf Schreibweisen. Eine Suche nach Schreibweisen
ist immer nur so gut wie ihre Fallliste.

Deshalb: eine Liste, zwei Nutzer. `tools/abnahme.py` arbeitet damit, und
Pruefung 190 im Selbsttest haelt jeden Eintrag gegen ein echtes Modul.
"""

ABLAGE_MODULE = ('shops', 'ships', 'erkul', 'places', 'prices',
                 'mining', 'routes')
