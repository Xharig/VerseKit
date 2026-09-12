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

Hier steckt alles, was sich zwischen Windows und Linux unterscheidet oder für
sich allein prüfbar ist. `sc_bp_watcher.py` daneben ist Startdatei, Oberfläche
und Überwachungs-Thread — es benutzt diese Bausteine, kennt aber selbst kein
Betriebssystem mehr.

    pfade       wo was liegt (die einzige Stelle mit Systempfaden)
    logquelle   Game.log mitlesen und frühere Sitzungen nachlesen
    bestand     der eigene Bauplan-Bestand
    phrasen     die Bauplan-Meldung in der jeweiligen Spielsprache erkennen
    autostart   mit dem Rechner starten

Reine Standardbibliothek, wie das ganze Projekt.
"""
