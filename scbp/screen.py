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
Welcher Bildschirm ist der Hauptbildschirm — und wo ist dessen Mitte?

**Warum es dieses Modul gibt.** Tk kennt nur *einen* Bildschirm: Bei mehreren
Monitoren meldet `winfo_screenwidth()` die Größe der gesamten zusammengesetzten
Fläche. Wer ein Fenster damit „mittig" setzt, landet bei drei Monitoren genau auf
einer Kante — oder in einer Lücke, wenn die Bildschirme unterschiedlich hoch sind.
Ohne Positionsangabe stellt Tk das Fenster irgendwohin, in der Praxis nach `+0+0`;
bei einem hochkant gestellten Monitor links außen ist das ein Bereich, in dem gar
kein Bild liegt. Genau so ist das Overlay einmal unauffindbar geworden.

Deshalb wird der Hauptbildschirm hier beim jeweiligen System erfragt:

  * **Windows** — `GetSystemMetrics`. Der Primärbildschirm liegt dort immer bei
    (0,0), seine Größe steht in SM_CXSCREEN/SM_CYSCREEN. Das ist der Zielfall:
    Star Citizen gibt es nur unter Windows.
  * **Linux** — `xrandr --listmonitors`. Die Zeile mit dem Stern ist der primäre
    Monitor, dahinter steht seine Lage. Das funktioniert auch unter Wayland, weil
    Tk ohnehin über XWayland läuft und dort dieselben Angaben ankommen (samt
    Skalierung, die Tk auch sonst benutzt).
  * **Sonst** — die gesamte Fläche. Schlechter als nichts ist es nicht.

Alles ist gutmütig gebaut: Findet sich nichts, wird die Tk-Fläche genommen, statt
einen Fehler zu werfen. Eine falsch platzierte Fensterecke darf nie ein Programm
anhalten.
"""
import os
import re
import subprocess
import sys

# Das Overlay-Fenster. Das Hauptprogramm trägt sich hier beim Start ein, damit der
# Knopf „Fensterlage zurücksetzen" es sofort verschieben kann — ohne dass dieses
# Modul das Hauptprogramm importieren muss (das gäbe einen Ringschluss).
OVERLAY = [None]

WINDOWS = sys.platform.startswith('win')

# Zeilenform von `xrandr --listmonitors`:
#   0: +*DP-2 5120/1193x1440/336+1080+1440  DP-2
# Gebraucht werden die vier Zahlen: Breite, Höhe, X, Y. Die Angaben mit Schrägstrich
# dahinter sind Millimeter — die interessieren nicht.
_XRANDR = re.compile(r'^\s*\d+:\s*\+\*\S*\s+(\d+)/\d+x(\d+)/\d+\+(\d+)\+(\d+)')


def _whole_area(root):
    """Rückfall: alles, was Tk sieht."""
    try:
        return 0, 0, root.winfo_screenwidth(), root.winfo_screenheight()
    except Exception:
        return 0, 0, 1920, 1080


def _windows_primary():
    try:
        import ctypes
        benutzer = ctypes.windll.user32
        # 0 = SM_CXSCREEN, 1 = SM_CYSCREEN — beides bezieht sich auf den Primärschirm.
        width = int(benutzer.GetSystemMetrics(0))
        height = int(benutzer.GetSystemMetrics(1))
        if width > 0 and height > 0:
            return 0, 0, width, height
    except Exception:
        pass
    return None


# Dieselbe Zeilenform wie oben, nur ohne den Stern — also **alle** Monitore.
_XRANDR_ALL = re.compile(r'^\s*\d+:\s*\+\*?\S*\s+(\d+)/\d+x(\d+)/\d+\+(\d+)\+(\d+)')


def _linux_all_screens():
    try:
        env = dict(os.environ)
        env['LC_ALL'] = 'C'
        output = subprocess.run(['xrandr', '--listmonitors'],
                                 capture_output=True, text=True, timeout=3,
                                 env=env).stdout
        screens = []
        for line in output.splitlines():
            match = _XRANDR_ALL.match(line)
            if match:
                b, h, x, y = (int(z) for z in match.groups())
                if b > 0 and h > 0:
                    screens.append((x, y, b, h))
        return screens
    except Exception:
        return []


def _windows_all_screens():
    """Alle Monitore über EnumDisplayMonitors."""
    try:
        import ctypes
        from ctypes import wintypes

        class RECT(ctypes.Structure):
            _fields_ = [('left', ctypes.c_long), ('top', ctypes.c_long),
                        ('right', ctypes.c_long), ('bottom', ctypes.c_long)]

        screens = []
        rueckruf_typ = ctypes.WINFUNCTYPE(ctypes.c_int, ctypes.c_void_p,
                                          ctypes.c_void_p,
                                          ctypes.POINTER(RECT), ctypes.c_double)

        def sammeln(_h, _dc, rechteck, _daten):
            r = rechteck.contents
            screens.append((r.left, r.top, r.right - r.left, r.bottom - r.top))
            return 1

        ctypes.windll.user32.EnumDisplayMonitors(None, None,
                                                 rueckruf_typ(sammeln), 0)
        return [s for s in screens if s[2] > 0 and s[3] > 0]
    except Exception:
        return []


def _linux_primary():
    try:
        env = dict(os.environ)
        env['LC_ALL'] = 'C'
        output = subprocess.run(['xrandr', '--listmonitors'],
                                 capture_output=True, text=True, timeout=3,
                                 env=env).stdout
        for line in output.splitlines():
            match = _XRANDR.match(line)
            if match:
                b, h, x, y = (int(z) for z in match.groups())
                if b > 0 and h > 0:
                    return x, y, b, h
    except Exception:
        pass
    return None


def primary(root):
    """Lage und Größe des Hauptbildschirms als (x, y, breite, hoehe)."""
    gefunden = _windows_primary() if WINDOWS else _linux_primary()
    return gefunden or _whole_area(root)


def all_screens(root):
    """Alle Bildschirme als (x, y, breite, hoehe) — oder die Gesamtfläche."""
    if WINDOWS:
        gefunden = _windows_all_screens()
        if gefunden:
            return gefunden
        einer = _windows_primary()
        return [einer] if einer else [_whole_area(root)]
    gefunden = _linux_all_screens()
    return gefunden or [_whole_area(root)]


def _windows_work_area():
    """Der nutzbare Bereich des Hauptschirms — **ohne Taskleiste**.

    ⚠⚠ Warum das noetig ist (gemeldet 02.09.2026 von Haldjas, pr0): Wer das
    Overlay in eine untere Ecke legt, bekam es an den echten Bildschirmrand
    gesetzt — und dort liegt unter Windows die Taskleiste. Der schmale
    Anfasser-Streifen (5 px hoch) verschwand dahinter: „hovern geht nicht mehr,
    nur ein Klick auf eine bestimmte Stelle klappt ihn aus." Getroffen wurde
    genau das Stueck, das oberhalb der Leiste herausschaute.

    `SPI_GETWORKAREA` (48) liefert das Rechteck, das Windows selbst fuer
    Fenster vorsieht. Schlaegt der Aufruf fehl, geben wir `None` zurueck und
    der Aufrufer bleibt bei der vollen Flaeche — lieber die alte Lage als gar
    keine.
    """
    try:
        import ctypes
        from ctypes import wintypes

        rechteck = wintypes.RECT()
        ok = ctypes.windll.user32.SystemParametersInfoW(
            48, 0, ctypes.byref(rechteck), 0)
        if ok and rechteck.right > rechteck.left \
                and rechteck.bottom > rechteck.top:
            return (int(rechteck.left), int(rechteck.top),
                    int(rechteck.right - rechteck.left),
                    int(rechteck.bottom - rechteck.top))
    except Exception:
        pass
    return None


def _linux_work_area():
    """Dasselbe unter Linux: `_NET_WORKAREA` des Fensterverwalters.

    ⚠ Nicht jede Umgebung setzt die Eigenschaft, und unter Wayland gibt es sie
    haeufig gar nicht. Dann `None` — der Aufrufer nimmt die volle Flaeche.
    """
    try:
        env = dict(os.environ)
        env['LC_ALL'] = 'C'
        output = subprocess.run(
            ['xprop', '-root', '_NET_WORKAREA'],
            capture_output=True, text=True, timeout=3, env=env).stdout
        zahlen = [int(z) for z in re.findall(r'\d+', output.split('=')[-1])]
        if len(zahlen) >= 4 and zahlen[2] > 0 and zahlen[3] > 0:
            return zahlen[0], zahlen[1], zahlen[2], zahlen[3]
    except Exception:
        pass
    return None


def work_area(root, x, y):
    """Wo ein Fenster wirklich hindarf — Schirm unter (x, y) ohne Taskleiste.

    ⚠ Der Arbeitsbereich wird nur fuer den **Hauptschirm** gemeldet; beide
    Systeme kennen dafuer keine Angabe je Monitor. Liegt der Punkt auf einem
    anderen Schirm, gilt deshalb weiter dessen volle Flaeche — besser als eine
    Zahl, die vom falschen Bildschirm stammt.
    """
    schirm = screen_at(root, x, y)
    arbeit = _windows_work_area() if WINDOWS else _linux_work_area()
    if not arbeit:
        return schirm
    ax, ay, ab, ah = arbeit
    sx, sy, sb, sh = schirm
    # Nur uebernehmen, wenn der Arbeitsbereich wirklich auf DIESEM Schirm
    # liegt - sonst waere es die Angabe eines fremden Monitors.
    if sx <= ax < sx + sb and sy <= ay < sy + sh:
        return ax, ay, ab, ah
    return schirm


def screen_at(root, x, y):
    """Auf welchem Bildschirm liegt dieser Punkt? (x, y, breite, hoehe)

    ⚠ Gebraucht überall dort, wo etwas neben ein Bedienelement geklappt wird.
    Tk kennt nur **einen** Bildschirm: `winfo_screenheight()` meldet die Höhe der
    gesamten zusammengesetzten Fläche. Bei zwei übereinander stehenden Monitoren
    sind das doppelt so viele Pixel, wie tatsächlich zu sehen sind — eine
    Auswahlliste auf dem oberen Schirm „passt" dann rechnerisch nach unten und
    klappt in Wirklichkeit ins Nichts. Gemeldet als „Alle Arten und Alle Quellen
    sind nicht auswählbar": Die langen Listen gingen unterhalb des Bildes auf.
    """
    for sx, sy, sb, sh in all_screens(root):
        if sx <= x < sx + sb and sy <= y < sy + sh:
            return sx, sy, sb, sh
    return primary(root)


def centered(root, width, height):
    """Geometrie-Angabe für ein Fenster in der Mitte des Hauptbildschirms.

    Das Ergebnis ist die übliche Tk-Form `BREITExHOEHE+X+Y`. Passt das Fenster
    nicht auf den Bildschirm, wird es an dessen Kante gesetzt statt darüber hinaus —
    ein Fenster, dessen Titelleiste oberhalb des Bildes liegt, lässt sich nicht mehr
    anfassen.
    """
    sx, sy, sb, sh = primary(root)
    x = sx + max(0, (sb - width) // 2)
    y = sy + max(0, (sh - height) // 2)
    return '%dx%d+%d+%d' % (width, height, x, y)
