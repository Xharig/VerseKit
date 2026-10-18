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
Module: Seiten der Seitenleiste ein- und ausblenden — je Modul, egal in
welcher Gruppe sie stehen; eine Gruppe ohne sichtbare Seite verschwindet.

Die Leiste kommt an ihre Grenze. Wer nicht handelt, braucht „Handel" nicht zu
sehen.

⚠⚠ **Die Regeln, auf die es ankommt:**

1. **Ab Werk ist alles an.** Wer VerseKit neu hat, sieht alles, was es kann.
2. **Baupläne, Einstellungen und Info sind nie abschaltbar** — der Kern des
   Werkzeugs und der Weg zum Fehlerbericht. Ihre Seiten stehen deshalb
   nicht in `SWITCHABLE`; nur die Basetool-Verbindung in den
   Einstellungen folgt dem Modul Basetool. Die Statistik lässt sich
   ausblenden. Ausgewertet wird trotzdem weiter, solange „Automatisch
   auswerten" an ist — sonst fehlten beim Wiedereinschalten die Wochen
   dazwischen.
3. **Ausblenden, nicht abschalten.** Es wird nichts gelöscht und nichts
   angehalten; die Daten bleiben, und schaltet man wieder ein, ist alles da.
4. **Ein Sprung auf eine ausgeblendete Seite** (z. B. von der Bauplan-Liste
   zur Herstellung) landet auf „Module" und sagt, was aus ist — statt still
   nichts zu tun.
"""
from . import paths

# Modul -> seine Seiten. Jede hat einen Reiter in der Leiste; Prüfung 271
# vergleicht das.
SWITCHABLE = {
    # ⚠ Der Schlüssel `modul_basetool` ist zugleich der Schalter des
    # Kartenbereichs Basetool (`cards.BASETOOL_SETTING`): ein Schalter für
    # beide Ansichten.
    'basetool': ('basetool', 'bedarf'),
    'schiffe': ('hangar', 'wunschliste', 'einkaufsliste'),
    'werkstatt': ('herstellung', 'farmliste', 'bergbau', 'raffinerien'),
    'bergung': ('bergung', 'zerlegen'),
    'handel': ('laeden', 'verkauf', 'routen'),
    'statistiken': ('statistik_auswertung', 'statistik', 'statistik_schiffe',
                    'statistik_auftraege', 'statistik_quantum',
                    'statistik_stabil'),
}

# Seiten außerhalb der Gruppe, die ein Schalter trotzdem mit ausblendet
# (`MainWindow._tab_hidden`) — damit die Seite Module sie mit nennt.
ALSO_HIDES = {'basetool': ('staffellager',)}

# Der Name jedes Moduls auf der Seite Module. Ein Modul blendet seine Reiter
# einzeln aus, egal in welcher Gruppe der Leiste sie stehen
# (`MainWindow._module_hidden`).
LABELS = {'basetool': 'k_basetool_titel', 'schiffe': 'hf_gruppe_schiffe', 'werkstatt': 's_mo_werkstatt',
          'bergung': 'hf_gruppe_bergung', 'handel': 'hf_gruppe_handel',
          'statistiken': 'hf_gruppe_statistik'}


def _key(group):
    return 'modul_%s' % group


def enabled(group):
    """Ist die Gruppe an? Nicht abschaltbare Gruppen sind immer an."""
    if group not in SWITCHABLE:
        return True
    return paths.setting_bool(_key(group), True)


def set_enabled(group, on):
    if group in SWITCHABLE:
        return paths.set_setting(_key(group), bool(on))
    return False


def group_of(page):
    """Zu welcher abschaltbaren Gruppe gehört eine Seite — oder None."""
    for group, pages in SWITCHABLE.items():
        if page in pages:
            return group
    return None


def hidden_pages():
    """Alle Seiten, deren Gruppe gerade aus ist."""
    return {page for group, pages in SWITCHABLE.items()
            if not enabled(group) for page in pages}
