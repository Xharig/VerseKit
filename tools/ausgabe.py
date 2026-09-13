# -*- coding: utf-8 -*-
#
# SC BP Watcher — zeigt live neue Star-Citizen-Baupläne an.
# Copyright (C) 2026 Xharig
#
# SPDX-License-Identifier: GPL-3.0-only
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, version 3.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <https://www.gnu.org/licenses/>.
"""
Ausgabe, die unter Windows nicht abstürzt.

⛔⛔ **Ein Prüflauf darf nicht an seiner eigenen Ausgabe scheitern.**

Die Windows-Konsole schreibt standardmäßig in `cp1252`. Umlaute gehen darin,
Pfeile und Warnzeichen nicht — und `print()` wirft dann keinen Hinweis, sondern
einen **UnicodeEncodeError** mitten im Lauf. Der Lauf bricht ab, und zwar an
einer Stelle, die mit dem Geprüften nichts zu tun hat.

Genau so am 14.09.2026 vor dem Release von v3.34.1: `launcher_pruefen.py`
starb in Zeile 205 an einem einzigen `↔`. Inhaltlich war alles in Ordnung —
mit `PYTHONIOENCODING=utf-8` lief derselbe Lauf durch und bestand. Nur sah das
auf dem Bildschirm aus wie ein durchgefallener Prüflauf kurz vor einer
Veröffentlichung.

⚠ **Das trifft nicht nur ein Werkzeug.** Gemessen am selben Tag tragen **acht**
Werkzeuge unter `tools/` Zeichen, die `cp1252` nicht kennt:

    ← → ↔ − ⏻ ─ └ ├ ▾ ⚠ ⛔ ✅ ✓ ✕ ✗ ❌ ⭐ ▶

Die meisten stehen in Kommentaren und werden nie gedruckt — bis jemand eine
Meldung ergänzt. Deshalb wird die Ausgabe **vorsorglich** umgestellt, nicht
erst, wenn es knallt.

⚠ Unter Linux ist die Ausgabe ohnehin UTF-8; dort ändert sich nichts. Deshalb
fällt so ein Fehler beim Entwickeln nie auf und erst auf dem Zweitsystem.

**Benutzung:** ganz oben im Werkzeug, vor der ersten Ausgabe:

    from . import ausgabe        # bzw. `import ausgabe`
    ausgabe.utf8()

Prüfung 207 im Selbsttest hält fest, dass jedes Werkzeug mit solchen Zeichen
diesen Aufruf auch wirklich macht.
"""
import sys


def utf8():
    """Standardausgabe und Fehlerkanal auf UTF-8 stellen.

    ⚠ **Nichts hiervon darf selbst scheitern.** Wird die Ausgabe umgeleitet
    (Prüflauf im Bau-Ablauf, `subprocess`, eine Testhülle), ist `reconfigure`
    unter Umständen gar nicht da — ein Absturz an dieser Stelle wäre genau der
    Fehler, den das Modul verhindern soll.

    ⚠ `errors='replace'` ist Absicht: Kann ein Terminal ein Zeichen trotzdem
    nicht darstellen, steht dort ein Ersatzzeichen. Ein unleserliches Zeichen
    ist ein Schönheitsfehler, ein abgebrochener Prüflauf ein Fehlalarm.
    """
    for strom in (sys.stdout, sys.stderr):
        try:
            strom.reconfigure(encoding='utf-8', errors='replace')
        except Exception:
            pass
