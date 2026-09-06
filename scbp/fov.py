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
KARTE_BREITE_MM = 85.60
KARTE_HOEHE_MM = 53.98

# Wie weit darf der Sitzabstand vom rechnerischen Punkt abweichen, bevor es
# gemeldet wird? Die Grenzen sind bewusst großzügig: Der neutrale Blickwinkel
# ist ein Bezugspunkt, kein Gebot — viele fliegen absichtlich weiter offen.
GRUEN = 0.08   # bis 8 % Abweichung: stimmt
GELB = 0.25    # bis 25 %: spürbar, aber vertretbar

# Wo Star Citizen seine Grafikeinstellungen ablegt.
ATTRIBUTE = ('attributes.xml',)


def bogen(grad):
    return grad * math.pi / 180.0


def grad(bogenmass):
    return bogenmass * 180.0 / math.pi


def blickwinkel(breite_mm, abstand_mm):
    """Der neutrale Blickwinkel für diesen Bildschirm und Abstand, in Grad.

    Das ist der Wert, bei dem das Bild weder vergrößert noch verkleinert
    wirkt. Liefert `None`, wenn die Eingaben unbrauchbar sind — ein Rechner,
    der bei Abstand 0 abstürzt, ist schlechter als einer, der nichts sagt.
    """
    try:
        breite_mm = float(breite_mm)
        abstand_mm = float(abstand_mm)
    except (TypeError, ValueError):
        return None
    if breite_mm <= 0 or abstand_mm <= 0:
        return None
    return grad(2.0 * math.atan(breite_mm / (2.0 * abstand_mm)))


def abstand_fuer(breite_mm, winkel_grad):
    """Bei welchem Abstand ist dieser Blickwinkel der neutrale? In Millimetern.

    Die Umkehrung von `blickwinkel()` — und die eigentlich nützliche Richtung:
    Wer sein Spiel schon eingestellt hat, will wissen, wo er dafür sitzen
    müsste, statt seine Einstellung umzuwerfen.
    """
    try:
        breite_mm = float(breite_mm)
        winkel_grad = float(winkel_grad)
    except (TypeError, ValueError):
        return None
    if breite_mm <= 0 or not (0 < winkel_grad < 180):
        return None
    return breite_mm / (2.0 * math.tan(bogen(winkel_grad) / 2.0))


def waagerecht_aus_senkrecht(senkrecht_grad, seitenverhaeltnis):
    """Aus dem senkrechten Blickwinkel den waagerechten rechnen.

    Gebraucht, weil Spiele die beiden verschieden handhaben. Star Citizen
    hält den **senkrechten** Winkel fest und erweitert waagerecht, wenn der
    Bildschirm breiter wird — dasselbe Verhalten, das anderswo „Hor+" heißt.
    """
    try:
        senkrecht_grad = float(senkrecht_grad)
        seitenverhaeltnis = float(seitenverhaeltnis)
    except (TypeError, ValueError):
        return None
    if senkrecht_grad <= 0 or seitenverhaeltnis <= 0:
        return None
    halb = math.tan(bogen(senkrecht_grad) / 2.0) * seitenverhaeltnis
    return grad(2.0 * math.atan(halb))


def senkrecht_aus_waagerecht(waagerecht_grad, seitenverhaeltnis):
    """Die Gegenrichtung zu `waagerecht_aus_senkrecht()`."""
    try:
        waagerecht_grad = float(waagerecht_grad)
        seitenverhaeltnis = float(seitenverhaeltnis)
    except (TypeError, ValueError):
        return None
    if waagerecht_grad <= 0 or seitenverhaeltnis <= 0:
        return None
    halb = math.tan(bogen(waagerecht_grad) / 2.0) / seitenverhaeltnis
    return grad(2.0 * math.atan(halb))


def bewertung(ist_abstand_mm, soll_abstand_mm):
    """Wie weit liegt der tatsächliche Sitzabstand vom neutralen Punkt?

    Liefert `('gruen'|'gelb'|'rot', Abweichung als Anteil)`. Das Vorzeichen
    der Abweichung sagt die Richtung: **positiv heißt zu weit weg**, negativ
    zu nah dran.

    ⚠ Rot heißt „weit daneben", nicht „falsch". Wer bewusst weiter offen
    fliegt, um mehr zu sehen, macht nichts verkehrt — er soll nur wissen,
    dass er es tut.
    """
    try:
        ist = float(ist_abstand_mm)
        soll = float(soll_abstand_mm)
    except (TypeError, ValueError):
        return 'rot', 0.0
    if soll <= 0:
        return 'rot', 0.0
    abweichung = (ist - soll) / soll
    betrag = abs(abweichung)
    if betrag <= GRUEN:
        return 'gruen', abweichung
    if betrag <= GELB:
        return 'gelb', abweichung
    return 'rot', abweichung


def _attributdatei(spielordner=None):
    """Die `attributes.xml` des Spielers — dort steht der eingestellte Wert."""
    try:
        wurzel = spielordner or pfade.spiel_ordner()
    except Exception:
        wurzel = None
    if not wurzel:
        return ''
    # Derselbe Ordner wie die `actionmaps.xml`, nur eine andere Datei.
    # ⚠ Groß- und Kleinschreibung wechselt (USER/user, Client/client) —
    # deshalb wird gesucht statt geraten, genau wie bei den Belegungen.
    for oben in ('USER', 'user'):
        for mitte in ('Client', 'client'):
            weg = os.path.join(wurzel, oben, mitte, '0', 'Profiles',
                               'default', 'attributes.xml')
            if os.path.isfile(weg):
                return weg
    return ''


def spiel_einstellung(spielordner=None):
    """Was im Spiel eingestellt ist: Blickwinkel und Auflösung.

    Liefert ein Wörterbuch mit `fov`, `breite`, `hoehe` und `datei` — jeder
    Wert `None`, wenn er nicht dasteht. Damit lässt sich die Rechnung gegen
    die Wirklichkeit halten, ohne dass der Spieler etwas abtippen muss.

    ⚠ Gelesen wird nur. An dieser Datei hängen alle Grafikeinstellungen;
    sie zu schreiben ist nicht Sache dieses Werkzeugs.
    """
    heraus = {'fov': None, 'breite': None, 'hoehe': None, 'datei': ''}
    weg = _attributdatei(spielordner)
    if not weg:
        return heraus
    heraus['datei'] = weg
    try:
        with open(weg, 'r', encoding='utf-8', errors='replace') as f:
            text = f.read()
    except Exception:
        return heraus

    def hole(name):
        treffer = re.search(
            r'<Attr\s+name="%s"\s+value="([^"]*)"' % name, text)
        if not treffer:
            return None
        try:
            return float(treffer.group(1))
        except ValueError:
            return None

    heraus['fov'] = hole('FOV')
    heraus['breite'] = hole('Width')
    heraus['hoehe'] = hole('Height')
    return heraus


def mm_pro_pixel(gemessene_pixel, karte_mm=KARTE_BREITE_MM):
    """Aus der abgemessenen Karte die Größe eines Pixels errechnen.

    `gemessene_pixel` ist die Breite, auf die der Spieler das Rechteck
    gezogen hat, bis es mit seiner Karte übereinstimmte.
    """
    try:
        gemessene_pixel = float(gemessene_pixel)
    except (TypeError, ValueError):
        return None
    if gemessene_pixel <= 0:
        return None
    return float(karte_mm) / gemessene_pixel


def bildschirmbreite_mm(pixelbreite, mm_je_pixel):
    """Die Breite des Bildschirms in Millimetern."""
    try:
        pixelbreite = float(pixelbreite)
        mm_je_pixel = float(mm_je_pixel)
    except (TypeError, ValueError):
        return None
    if pixelbreite <= 0 or mm_je_pixel <= 0:
        return None
    return pixelbreite * mm_je_pixel


SCHLUESSEL = ('fov_mm_je_pixel', 'fov_pixelbreite', 'fov_abstand_mm')


def gespeichert():
    """Die zuletzt gespeicherte Kalibrierung, falls es eine gibt.

    ⚠⚠ **Gelesen wird direkt aus dem Wörterbuch, nicht über die Helfer.**
    `pfade.einstellung()` ist für **Pfade** gedacht und gibt bei allem, was
    kein Text ist, `None` zurück — die Kalibrierung war damit nach jedem
    Neustart weg, ohne eine einzige Fehlermeldung. `einstellung_zahl()`
    wiederum liefert `int`, und „0,2330 mm je Pixel" ist keine ganze Zahl.
    Für Fließkommawerte gibt es hier keinen passenden Helfer.
    """
    try:
        alle = pfade.einstellungen() or {}
    except Exception:
        return {}
    daten = {}
    for schluessel in SCHLUESSEL:
        wert = alle.get(schluessel)
        if wert is None:
            continue
        try:
            daten[schluessel] = float(wert)
        except (TypeError, ValueError):
            pass
    return daten


def merken(mm_je_pixel=None, pixelbreite=None, abstand_mm=None):
    """Die Kalibrierung sichern, damit sie nicht bei jedem Start neu anfällt.

    ⚠ Nur was genannt wird, wird geschrieben — so lässt sich der Sitzabstand
    ändern, ohne die Kalibrierung zu verlieren, und umgekehrt.
    """
    paare = (('fov_mm_je_pixel', mm_je_pixel),
             ('fov_pixelbreite', pixelbreite),
             ('fov_abstand_mm', abstand_mm))
    geschrieben = 0
    for schluessel, wert in paare:
        if wert is None:
            continue
        try:
            pfade.einstellung_setzen(schluessel, float(wert))
            geschrieben += 1
        except Exception:
            from . import fehler
            fehler.merken('fov.merken', Exception('%s' % schluessel))
    return geschrieben
