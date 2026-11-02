# -*- coding: utf-8 -*-
#
# Verse-Kit — Begleiter für Star Citizen.
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
Retired: writes nothing and never overwrites the language files.

Since v5 `crates/versekit-core/lang/de.json` and `en.json` are the original
of the texts. This script used to generate them from `scbp/language.py`; an old
run would destroy every new and changed v5 line, so it only prints a notice
(in German, for the people who run it).
"""
import sys

MESSAGE = """\
export_language.py ist stillgelegt — es wurde NICHTS geschrieben.

Die Texte von v5 stehen in
    crates/versekit-core/lang/de.json
    crates/versekit-core/lang/en.json
Sie sind das Original: neue und geänderte Texte werden dort von Hand
eingetragen, Deutsch und Englisch zusammen. Prüfung:
    cargo test -p versekit-core --test language_files

Die Texte von 4.12 (Tag v4.12.0, scbp/language.py) werden nicht mehr nach JSON
übertragen.
"""

if __name__ == '__main__':
    sys.exit(MESSAGE)
