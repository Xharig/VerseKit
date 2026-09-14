# SPDX-License-Identifier: GPL-3.0-only
#
# SC BP Watcher — Blickwinkel und Sitzabstand
# Copyright (C) 2026 Xharig
#
# This program is free software: you can redistribute it and/or modify it under
# the terms of the GNU General Public License version 3 as published by the
# Free Software Foundation.
#
# This program is distributed in the hope that it will be useful, but WITHOUT
# ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
# FOR A PARTICULAR PURPOSE.  See the GNU General Public License for more
# details.
#
# You should have received a copy of the GNU General Public License along with
# this program.  If not, see <https://www.gnu.org/licenses/>.
"""
Welcher Blickwinkel passt zu deinem Bildschirm — und wo musst du dafür sitzen?

## Die Idee in einem Satz

Es gibt genau **einen** Blickwinkel, bei dem das Bild so groß erscheint wie das
Dargestellte in Wirklichkeit wäre: wenn der Bildschirm im Auge denselben Winkel
einnimmt wie das, was er zeigt. Dann stimmen Größen und Entfernungen — ein
Schiff, das nah aussieht, ist auch nah.

    Blickwinkel = 2 · arctan( Bildschirmbreite / (2 · Sitzabstand) )

Weiter aufgedreht sieht man mehr, aber alles wirkt kleiner und weiter weg;
enger gestellt wirkt alles näher, dafür sieht man weniger. Beides kann man
wollen — nur sollte man wissen, wo der neutrale Punkt liegt.

## ⚠⚠ Warum von Hand kalibriert wird und nicht automatisch

Naheliegend wäre, den Rechner die Bildschirmgröße selbst herausfinden zu
lassen. Gemessen am 06.09.2026 an einem Aufbau mit drei Bildschirmen:

| Quelle | Antwort | Brauchbar? |
|---|---|---|
| `winfo_screenmmwidth()` (Tk) | 1640 mm | ❌ das ist der **gesamte Desktop** über alle drei |
| `xrandr` (EDID) | 1193 mm | ✅ pro Bildschirm — aber nur unter X11 |
| Karte anhalten | exakt | ✅ überall, auch unter Wayland und Windows |

Dazu kommt: EDID-Angaben sind gerundet, bei manchen Geräten schlicht falsch,
und bei zwei gleichen Bildschirmen weiß niemand, auf welchem gespielt wird.
**Eine Karte an den Bildschirm zu halten dauert zehn Sekunden und stimmt.**

Jede Bankkarte, jeder Führerschein und jeder Personalausweis hat exakt
dieselbe Größe — ISO/IEC 7810, Format ID-1: **85,60 × 53,98 mm**. Das ist
weltweit genormt und liegt in jedem Portemonnaie.

## Wie die Pixelbreite des richtigen Bildschirms gefunden wird

Nicht über eine Geräteabfrage, sondern über das Kalibrierfenster selbst:
Es geht **auf dem Bildschirm, auf dem es geöffnet wird, in den Vollbildmodus**
und misst sich anschließend selbst (`winfo_width()`). Damit stimmt der Wert
auch bei drei verschiedenen Bildschirmen — und ohne eine einzige
systemabhängige Zeile.
"""
import math
import os
import re

from . import pfade

# ISO/IEC 7810 ID-1 — Bankkarte, Führerschein, Personalausweis.
# ⚠ Diese Zahlen sind eine Norm, keine Schätzung. Nicht „glätten".
CARD_WIDTH_MM = 85.60
CARD_HEIGHT_MM = 53.98

# Wie weit darf der Sitzabstand vom rechnerischen Punkt abweichen, bevor es
# gemeldet wird? Die Grenzen sind bewusst großzügig: Der neutrale Blickwinkel
# ist ein Bezugspunkt, kein Gebot — viele fliegen absichtlich weiter offen.
GREEN = 0.08   # bis 8 % Abweichung: stimmt
YELLOW = 0.25    # bis 25 %: spürbar, aber vertretbar

# Wo Star Citizen seine Grafikeinstellungen ablegt.
ATTRIBUTE = ('attributes.xml',)


def radians(deg):
    return deg * math.pi / 180.0


def degrees(rad):
    return rad * 180.0 / math.pi


def field_of_view(width_mm, distance_mm):
    """Der neutrale Blickwinkel für diesen Bildschirm und Abstand, in Grad.

    Das ist der Wert, bei dem das Bild weder vergrößert noch verkleinert
    wirkt. Liefert `None`, wenn die Eingaben unbrauchbar sind — ein Rechner,
    der bei Abstand 0 abstürzt, ist schlechter als einer, der nichts sagt.
    """
    try:
        width_mm = float(width_mm)
        distance_mm = float(distance_mm)
    except (TypeError, ValueError):
        return None
    if width_mm <= 0 or distance_mm <= 0:
        return None
    return degrees(2.0 * math.atan(width_mm / (2.0 * distance_mm)))


def distance_for(width_mm, angle_deg):
    """Bei welchem Abstand ist dieser Blickwinkel der neutrale? In Millimetern.

    Die Umkehrung von `field_of_view()` — und die eigentlich nützliche Richtung:
    Wer sein Spiel schon eingestellt hat, will wissen, wo er dafür sitzen
    müsste, statt seine Einstellung umzuwerfen.
    """
    try:
        width_mm = float(width_mm)
        angle_deg = float(angle_deg)
    except (TypeError, ValueError):
        return None
    if width_mm <= 0 or not (0 < angle_deg < 180):
        return None
    return width_mm / (2.0 * math.tan(radians(angle_deg) / 2.0))


def horizontal_from_vertical(vertical_deg, aspect):
    """Aus dem senkrechten Blickwinkel den waagerechten rechnen.

    Gebraucht, weil Spiele die beiden verschieden handhaben. Star Citizen
    hält den **senkrechten** Winkel fest und erweitert waagerecht, wenn der
    Bildschirm breiter wird — dasselbe Verhalten, das anderswo „Hor+" heißt.
    """
    try:
        vertical_deg = float(vertical_deg)
        aspect = float(aspect)
    except (TypeError, ValueError):
        return None
    if vertical_deg <= 0 or aspect <= 0:
        return None
    half = math.tan(radians(vertical_deg) / 2.0) * aspect
    return degrees(2.0 * math.atan(half))


def vertical_from_horizontal(horizontal_deg, aspect):
    """Die Gegenrichtung zu `horizontal_from_vertical()`."""
    try:
        horizontal_deg = float(horizontal_deg)
        aspect = float(aspect)
    except (TypeError, ValueError):
        return None
    if horizontal_deg <= 0 or aspect <= 0:
        return None
    half = math.tan(radians(horizontal_deg) / 2.0) / aspect
    return degrees(2.0 * math.atan(half))


def rating(actual_distance_mm, target_distance_mm):
    """Wie weit liegt der tatsächliche Sitzabstand vom neutralen Punkt?

    Liefert `('gruen'|'gelb'|'rot', Abweichung als Anteil)`. Das Vorzeichen
    der Abweichung sagt die Richtung: **positiv heißt zu weit weg**, negativ
    zu nah dran.

    ⚠ Rot heißt „weit daneben", nicht „falsch". Wer bewusst weiter offen
    fliegt, um mehr zu sehen, macht nichts verkehrt — er soll nur wissen,
    dass er es tut.
    """
    try:
        actual = float(actual_distance_mm)
        target = float(target_distance_mm)
    except (TypeError, ValueError):
        return 'rot', 0.0
    if target <= 0:
        return 'rot', 0.0
    deviation = (actual - target) / target
    amount = abs(deviation)
    if amount <= GREEN:
        return 'gruen', deviation
    if amount <= YELLOW:
        return 'gelb', deviation
    return 'rot', deviation


def _attribute_file(game_folder=None):
    """Die `attributes.xml` des Spielers — dort steht der eingestellte Wert."""
    try:
        root = game_folder or pfade.spiel_ordner()
    except Exception:
        root = None
    if not root:
        return ''
    # Derselbe Ordner wie die `actionmaps.xml`, nur eine andere Datei.
    # ⚠ Groß- und Kleinschreibung wechselt (USER/user, Client/client) —
    # deshalb wird gesucht statt geraten, genau wie bei den Belegungen.
    for top in ('USER', 'user'):
        for middle in ('Client', 'client'):
            path = os.path.join(root, top, middle, '0', 'Profiles',
                               'default', 'attributes.xml')
            if os.path.isfile(path):
                return path
    return ''


def game_setting(game_folder=None):
    """Was im Spiel eingestellt ist: Blickwinkel und Auflösung.

    Liefert ein Wörterbuch mit `fov`, `breite`, `hoehe` und `datei` — jeder
    Wert `None`, wenn er nicht dasteht. Damit lässt sich die Rechnung gegen
    die Wirklichkeit halten, ohne dass der Spieler etwas abtippen muss.

    ⚠ Gelesen wird nur. An dieser Datei hängen alle Grafikeinstellungen;
    sie zu schreiben ist nicht Sache dieses Werkzeugs.
    """
    out = {'fov': None, 'breite': None, 'hoehe': None, 'datei': ''}
    path = _attribute_file(game_folder)
    if not path:
        return out
    out['datei'] = path
    try:
        with open(path, 'r', encoding='utf-8', errors='replace') as f:
            text = f.read()
    except Exception:
        return out

    def pick(name):
        hit = re.search(
            r'<Attr\s+name="%s"\s+value="([^"]*)"' % name, text)
        if not hit:
            return None
        try:
            return float(hit.group(1))
        except ValueError:
            return None

    out['fov'] = pick('FOV')
    out['breite'] = pick('Width')
    out['hoehe'] = pick('Height')
    return out


def mm_per_pixel(measured_px, card_mm=CARD_WIDTH_MM):
    """Aus der abgemessenen Karte die Größe eines Pixels errechnen.

    `gemessene_pixel` ist die Breite, auf die der Spieler das Rechteck
    gezogen hat, bis es mit seiner Karte übereinstimmte.
    """
    try:
        measured_px = float(measured_px)
    except (TypeError, ValueError):
        return None
    if measured_px <= 0:
        return None
    return float(card_mm) / measured_px


def screen_width_mm(width_px, mm_per_px):
    """Die Breite des Bildschirms in Millimetern."""
    try:
        width_px = float(width_px)
        mm_per_px = float(mm_per_px)
    except (TypeError, ValueError):
        return None
    if width_px <= 0 or mm_per_px <= 0:
        return None
    return width_px * mm_per_px


KEYS = ('fov_mm_je_pixel', 'fov_pixelbreite', 'fov_abstand_mm')


def stored():
    """Die zuletzt gespeicherte Kalibrierung, falls es eine gibt.

    ⚠⚠ **Gelesen wird direkt aus dem Wörterbuch, nicht über die Helfer.**
    `pfade.einstellung()` ist für **Pfade** gedacht und gibt bei allem, was
    kein Text ist, `None` zurück — die Kalibrierung war damit nach jedem
    Neustart weg, ohne eine einzige Fehlermeldung. `einstellung_zahl()`
    wiederum liefert `int`, und „0,2330 mm je Pixel" ist keine ganze Zahl.
    Für Fließkommawerte gibt es hier keinen passenden Helfer.
    """
    try:
        all_hits = pfade.einstellungen() or {}
    except Exception:
        return {}
    data = {}
    for key in KEYS:
        value = all_hits.get(key)
        if value is None:
            continue
        try:
            data[key] = float(value)
        except (TypeError, ValueError):
            pass
    return data


def remember(mm_per_px=None, width_px=None, distance_mm=None):
    """Die Kalibrierung sichern, damit sie nicht bei jedem Start neu anfällt.

    ⚠ Nur was genannt wird, wird geschrieben — so lässt sich der Sitzabstand
    ändern, ohne die Kalibrierung zu verlieren, und umgekehrt.
    """
    pairs = (('fov_mm_je_pixel', mm_per_px),
             ('fov_pixelbreite', width_px),
             ('fov_abstand_mm', distance_mm))
    written = 0
    for key, value in pairs:
        if value is None:
            continue
        try:
            pfade.einstellung_setzen(key, float(value))
            written += 1
        except Exception:
            from . import fehler
            fehler.merken('fov.remember', Exception('%s' % key))
    return written
