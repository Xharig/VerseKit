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
Wohin ein Fehlerbericht geht, wenn der Nutzer „Absenden" drückt.

**Warum es diesen Weg überhaupt gibt.** Den Bericht zu kopieren und in Discord
einzufügen scheitert an drei Stellen: Er steckt unter „Fortgeschritten", er ist
zu lang für eine Nachricht, und man muss wissen, wohin damit. der Autor am
28.08.2026: „ich will nicht jedem eine Stunde erklären, wie ich zu dem Bericht
komme, das ist nervenaufreibend." Ein Knopf ist die einzige Fassung, die bei
Nicht-Bastlern ankommt.

⭐⭐ **Seit v3.57.2 geht der Bericht an eine eigene Weiterleitung, nicht mehr
direkt an Discord.** Bis dahin setzte der Bau die Discord-Webhook-Adresse aus
einem Secret in diese Datei — und damit stand sie in jeder veröffentlichten
`.exe`, für jeden auslesbar, der dort sucht. Wer sie hatte, konnte in den
Kanal schreiben, auch `@everyone`. Der alte Webhook ist gelöscht.

Die Weiterleitung ist ein Cloudflare Worker (`tools/bericht-worker/`). Sie
nimmt nur an, was wie ein Verse-Kit-Bericht aussieht, bremst die Menge je
Absender, schaltet alle Erwähnungen ab und hält den Webhook als Geheimnis. Ihre
Adresse hier ist **kein Geheimnis** — sie darf im Quelltext stehen, und genau
deshalb braucht der Bau nichts mehr einzusetzen. Muss der Webhook getauscht
werden, geschieht das beim Worker, ohne neue Version.

⚠ **Nie auf einem Heimserver.** Die Weiterleitung muss aus dem Internet
erreichbar sein; ein eigener Dienst zu Hause hieße, das Heimnetz zu öffnen.
"""

# Die öffentliche Adresse der Weiterleitung.
RELAY = 'https://versekit-bericht.xharig.workers.dev/bericht'


def target():
    """Die Adresse, an die gesendet wird.

    ⚠ **`SC_BP_BERICHT_ZIEL` schlägt die eingebaute Adresse** — zum Prüfen
    gegen einen eigenen Testdienst. Steht dort etwas, das nicht mit `https://`
    beginnt (etwa `aus`), gibt es kein Ziel: So schalten Prüfläufe das Senden
    ab, ohne dass ein Bericht aus Versehen hinausgeht.
    """
    import os
    override = os.environ.get('SC_BP_BERICHT_ZIEL')
    if override is not None:
        return override.strip()
    return RELAY


def available():
    """Kann überhaupt gesendet werden?"""
    return target().startswith('https://')
