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
Eine Seite beim Wiederkommen nur neu zeichnen, wenn sich ihre Daten geändert
haben.

Eine Seite wird einmal gebaut und danach nur ein- und ausgeblendet; was frisch
sein soll, frischt `on_show` auf. Baut eine Seite dort ihren ganzen Inhalt neu,
kostet das jeden Klick auf den Reiter — auch wenn sich nichts geändert hat.

`Guard` merkt sich beim Zeichnen, **welche eigenen Dateien** dabei gelesen
wurden: `paths.app_file()` meldet jeden Pfad an `note()`, solange ein Wächter
aufzeichnet. Zu jeder Datei steht ihr Stempel (`stamp`) von **vor** dem Lesen,
dazu der Ablage-Ordner. Beim nächsten Zeigen genügt dann je Datei ein `stat`:
Ist alles gleich und auch der Zusatzwert (`extra`), bleibt die Seite stehen.

⭐ Die Liste der Quellen entsteht beim Zeichnen selbst, sie wird nicht von Hand
aufgezählt. Eine von Hand gepflegte Liste wird beim nächsten neuen Baustein
unvollständig, und dann zeigt die Seite einen alten Stand.

⚠⚠ Was **nicht** über `app_file()` läuft, sieht der Wächter nicht — Dateien
des Spiels (`Game.log`, `attributes.xml`), Stände nur im Speicher (Abgleich
mit dem Basetool), Lesen in Nebenfäden und Texte, die von der Uhrzeit
abhängen. Das gehört in `extra`: eine Funktion, deren Ergebnis sich ändert,
sobald sich so eine Quelle ändert.

⚠ Irrt sich der Wächter, dann in die harmlose Richtung: Der Stempel wird vor
dem Lesen genommen. Ändert sich eine Datei während des Zeichnens, passt er
beim nächsten Mal nicht mehr, und die Seite wird noch einmal gebaut — nie
bleibt ein alter Stand stehen, weil der Stempel schon den neuen trug.
"""
import os
import threading

_local = threading.local()


def stamp(path):
    """`(Änderungszeit in ns, Größe, Dateikennung)` einer Datei — None, wenn
    es sie nicht gibt.

    ⚠ Die Kennung (`st_ino`) gehört dazu: Unter Windows rückt die
    Änderungszeit nur im Takt der Systemuhr vor. Zwei gleich große Fassungen
    kurz nacheinander hätten sonst denselben Stempel — gespeichert wird aber
    über eine neue Datei und `os.replace`, und die hat eine neue Kennung."""
    try:
        info = os.stat(path)
    except OSError:
        return None
    return (info.st_mtime_ns, info.st_size, info.st_ino)


def recording():
    """Zeichnet in diesem Faden gerade ein Wächter auf?

    Für Abkürzungen, die `app_file()` bewusst überspringen (gedrosselte
    Frisch-Prüfungen): Sie melden ihre Datei dann trotzdem an."""
    return getattr(_local, 'seen', None) is not None


def note(path):
    """Von `paths.app_file()` gerufen: Datei merken, wenn gerade aufgezeichnet
    wird. Nur im eigenen Faden — was ein Nebenfaden liest, gehört nicht zur
    Seite, die gerade gezeichnet wird."""
    seen = getattr(_local, 'seen', None)
    if seen is not None and path not in seen:
        seen[path] = stamp(path)


class Guard:
    """Wächter für eine Seite. Gebrauch::

        guard = freshness.Guard(extra=lambda: (spiel_datei_stempel, ...))

        def on_show():
            if guard.fresh():
                return
            guard.draw(neu_zeichnen)

    `extra` liefert einen vergleichbaren Wert für alles, was nicht über
    `app_file()` gelesen wird. Wirft es, gilt die Seite als nicht frisch."""

    def __init__(self, extra=None):
        self.extra = extra
        self.files = None
        self.extra_value = None
        self.folder = None
        self._spoiled = False

    def _extra(self):
        return self.extra() if self.extra is not None else None

    @staticmethod
    def _folder():
        """Der Ablage-Ordner. ⚠ Wird er gewechselt, liegen die alten Dateien
        unverändert weiter da — ihre Stempel allein sähen keinen Unterschied,
        während `app_file()` schon in den neuen Ordner zeigt."""
        from . import paths
        return paths.app_folder()

    def fresh(self):
        """Ist seit dem letzten `draw()` alles gleich geblieben?"""
        if self.files is None:
            return False
        try:
            if (self._extra() != self.extra_value
                    or self._folder() != self.folder):
                return False
        except Exception:
            return False
        return all(stamp(path) == old for path, old in self.files.items())

    def draw(self, function):
        """`function()` zeichnen lassen und dabei die gelesenen Dateien merken."""
        self.files = None
        try:
            extra_value = self._extra()
            folder = self._folder()
            known = True
        except Exception:
            extra_value, folder, known = None, None, False
        outer = getattr(_local, 'seen', None)
        seen = {}
        _local.seen = seen
        self._spoiled = False
        try:
            function()
        finally:
            _local.seen = outer
            # Ein äußerer Wächter (Seite in der Seite) braucht dieselben
            # Dateien — sonst hielte er sich für frisch, obwohl der innere
            # Teil neue Daten zeigen müsste.
            if outer is not None:
                for path, old in seen.items():
                    outer.setdefault(path, old)
        if known and not self._spoiled:
            self.files, self.extra_value, self.folder = (seen, extra_value,
                                                         folder)

    def invalidate(self):
        """Beim nächsten Zeigen auf jeden Fall neu zeichnen — etwa solange
        noch Daten aus dem Netz ausstehen. Darf auch während `draw()` fallen."""
        self.files = None
        self._spoiled = True
