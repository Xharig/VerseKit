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
Die Bausteine von VerseKit.

Hier steckt der größte Teil dessen, was sich zwischen Windows und Linux
unterscheidet, und alles, was für sich allein prüfbar ist.
`sc_bp_watcher.py` daneben ist Startdatei, Oberfläche und
Überwachungs-Thread; es enthält nur noch wenige eigene Weichen nach dem
Betriebssystem (Fenstersymbol, Vordergrund, Einzelstart-Sperre).

    paths       wo was liegt (die einzige Stelle mit Systempfaden)
    logsource   Game.log mitlesen und frühere Sitzungen nachlesen
    collection  der eigene Bauplan-Bestand
    phrases     die Bauplan-Meldung in der jeweiligen Spielsprache erkennen
    autostart   mit dem Rechner starten
    tk_calls    Arbeit aus Nebenfäden in den Tk-Faden bringen

Reine Standardbibliothek, wie das ganze Projekt.
"""
