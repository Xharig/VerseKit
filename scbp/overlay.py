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
Wie sich das Overlay im Spiel verhält.

Anlass war eine Rückmeldung von Haldjas (pr0): „Das Overlay ist permanent zu sehen und
nicht durchklickbar. Wenn ich im Kampf mit der Maus hineinkomme, wird das
unangenehm." — Recht hat er. Ein Werkzeug, das beim Spielen im Weg steht, benutzt
niemand, egal wie gut es sonst ist.

Zwei getrennte Antworten darauf, beide abschaltbar:

**1. Nur zeigen, wenn es etwas zu sagen gibt.** Statt dauernd dazustehen bleibt das
Overlay unsichtbar und taucht bei einem neuen Bauplan für ein paar Sekunden auf.
Was dann noch fehlt, ist ein Weg zurück — den liefert der Einzelinstanz-Wächter
(siehe `please_show`): Das Programm ein zweites Mal starten holt das vorhandene
Fenster hervor, statt eine zweite Version zu öffnen. Damit reicht eine ganz normale
Tastenkombination des Systems auf die Verknüpfung.

**2. Mausklicks durchreichen.** Das Fenster ist dann zwar sichtbar, fängt aber keine
Klicks mehr ab — der Schuss geht ins Spiel, nicht ins Overlay.

  * **Windows:** sauber über `WS_EX_TRANSPARENT`. Genau so machen es die
    Spiel-Overlays, die man kennt.
  * **Linux/X11:** über die XShape-Erweiterung, indem die Eingabe-Region des
    Fensters auf leer gesetzt wird. Tk bietet das nicht an, `libXext` schon.
    Läuft auch unter XWayland, weil das eine X11-Umsetzung ist.
  * **Wayland (nativ):** geht nicht. Ein gewöhnliches Fenster kann dort keine
    Eingaben weiterreichen. Wir sagen das ehrlich, statt es still zu ignorieren.

⚠ Wer durchklickbar einschaltet, kann das Overlay auch nicht mehr **verschieben**
oder seine Knöpfe treffen — es reicht ja alles durch. Deshalb hängt an der
Einstellung ein deutlicher Hinweis, und der Weg ins Fenster führt über den
zweiten Programmstart.
"""
import ctypes
import socket
import sys
import threading

WINDOWS = sys.platform.startswith('win')

# Fester Port auf dem eigenen Rechner für den Einzelinstanz-Wächter. Nichts
# davon geht ins Netz: gebunden wird ausschließlich an 127.0.0.1.
WATCHDOG_PORT = 47913
_watchdog = [None]

# Das Overlay-Fenster selbst. Das Hauptprogramm trägt sich beim Start ein, damit
# die Einstellungsseite eine Änderung sofort anwenden kann, ohne das
# Hauptprogramm importieren zu müssen (das gäbe einen Ringschluss).
OVERLAY_WINDOW = [None]

# Das Overlay-Objekt selbst (nicht nur sein Tk-Fenster). Darüber kann die
# Einstellungsseite eine Änderung sofort anwenden lassen.
OVERLAY_CONTROL = [None]


# --------------------------------------------------------- Mausklicks durchreichen

def _windows_click_through(fenster, an):
    try:
        fenster.update_idletasks()
        kennung = ctypes.windll.user32.GetParent(fenster.winfo_id()) or fenster.winfo_id()
        GWL_EXSTYLE = -20
        WS_EX_LAYERED = 0x00080000
        WS_EX_TRANSPARENT = 0x00000020
        hole = getattr(ctypes.windll.user32, 'GetWindowLongPtrW',
                       ctypes.windll.user32.GetWindowLongW)
        setze = getattr(ctypes.windll.user32, 'SetWindowLongPtrW',
                        ctypes.windll.user32.SetWindowLongW)
        stil = hole(kennung, GWL_EXSTYLE)
        if an:
            stil |= WS_EX_LAYERED | WS_EX_TRANSPARENT
        else:
            stil &= ~WS_EX_TRANSPARENT
        setze(kennung, GWL_EXSTYLE, stil)
        return True
    except Exception:
        return False


def _x11_click_through(fenster, an):
    """Die Eingabe-Region auf leer setzen — dann fällt jeder Klick hindurch.

    ⚠ Die Typen müssen von Hand gesetzt werden. Ohne `restype` nimmt ctypes für
    jeden Rückgabewert ein 32-Bit-`int` an — auf einem 64-Bit-System wird der
    Zeiger auf den Display damit abgeschnitten, und der nächste Aufruf greift ins
    Nichts. Der erste Anlauf hat das Programm genau so mit einem Speicherauszug
    beendet, nicht mit einer Fehlermeldung.
    """
    from ctypes import c_int, c_ulong, c_void_p
    try:
        fenster.update_idletasks()
        x11 = ctypes.cdll.LoadLibrary('libX11.so.6')
        xext = ctypes.cdll.LoadLibrary('libXext.so.6')

        x11.XOpenDisplay.restype = c_void_p
        x11.XOpenDisplay.argtypes = [c_void_p]
        x11.XCloseDisplay.restype = c_int
        x11.XCloseDisplay.argtypes = [c_void_p]
        x11.XFlush.restype = c_int
        x11.XFlush.argtypes = [c_void_p]
        x11.XCreateRegion.restype = c_void_p
        x11.XCreateRegion.argtypes = []
        x11.XDestroyRegion.restype = c_int
        x11.XDestroyRegion.argtypes = [c_void_p]
        xext.XShapeCombineRegion.restype = None
        xext.XShapeCombineRegion.argtypes = [c_void_p, c_ulong, c_int, c_int,
                                             c_int, c_void_p, c_int]
        xext.XShapeCombineMask.restype = None
        xext.XShapeCombineMask.argtypes = [c_void_p, c_ulong, c_int, c_int,
                                           c_int, c_ulong, c_int]
        xext.XShapeQueryExtension.restype = c_int
        xext.XShapeQueryExtension.argtypes = [c_void_p, ctypes.POINTER(c_int),
                                              ctypes.POINTER(c_int)]

        anzeige = x11.XOpenDisplay(None)
        if not anzeige:
            return False
        try:
            # Gibt es die Erweiterung auf diesem Server überhaupt? Ohne diese
            # Frage würde ein Aufruf ins Leere laufen.
            ereignis, fehlerbasis = c_int(), c_int()
            if not xext.XShapeQueryExtension(anzeige, ctypes.byref(ereignis),
                                             ctypes.byref(fehlerbasis)):
                return False
            kennung = c_ulong(fenster.winfo_id())
            ShapeInput, ShapeSet = 2, 0
            if an:
                # Eine Region ohne Fläche: Nichts davon nimmt noch Klicks an.
                region = x11.XCreateRegion()
                if not region:
                    return False
                xext.XShapeCombineRegion(anzeige, kennung, ShapeInput, 0, 0,
                                         region, ShapeSet)
                x11.XDestroyRegion(region)
            else:
                # Ohne Maske (None = 0) gilt wieder das ganze Fenster.
                xext.XShapeCombineMask(anzeige, kennung, ShapeInput, 0, 0, 0,
                                       ShapeSet)
            x11.XFlush(anzeige)
            return True
        finally:
            x11.XCloseDisplay(anzeige)
    except Exception:
        return False


def click_through_possible():
    """Lässt sich auf diesem System überhaupt durchklicken?"""
    if WINDOWS:
        return True
    # Unter nativem Wayland gibt es keinen X-Server, an den wir uns wenden könnten.
    import os
    if os.environ.get('WAYLAND_DISPLAY') and not os.environ.get('DISPLAY'):
        return False
    return True


# Wer immer das Durchreichen umschaltet — das Schloss am Overlay muss mitziehen.
# Die Anwendung trägt hier ihre eigene Funktion ein; ohne Eintrag passiert
# nichts, damit `overlay.py` ohne Oberfläche prüfbar bleibt.
#
# ⚠ Warum es das Schloss überhaupt gibt: Wer Klicks durchreichen lässt, kommt an
# das Overlay nicht mehr heran — auch nicht an den Schalter, mit dem er es
# wieder abstellt. Der Rückweg war bis dahin, **das Programm ein zweites Mal zu
# starten**, und dafür muss man aus dem Spiel heraus. Gemeldet am 27.08.2026:
# „der zweite Programmstart ist die denkbar dümmste Lösung, weil man dann
# raustabben muss aus dem Spiel." Ryze löst es beim TeamSpeak-Plugin mit einem
# Schloss, das anklickbar bleibt — denselben Weg gehen wir.
LOCK_CALLBACK = [None]

# ⭐ **Der Bericht muss mitwachsen.** Am 13.09.2026 kam die Meldung „im
# eingeklappten Zustand sitzt das Schloss nicht ganz genau da, wo es sitzen
# sollte" — und dazu die entscheidende Beobachtung „beim Einklappen wird das
# Fenster leicht groesser". Beides liess sich hier nicht nachstellen, und im
# Fehlerbericht stand nichts davon: keine Fenstergroesse, kein Klappzustand,
# keine Mindestbreite, kein Versatz des schwebenden Schlosses.
#
# Das ist genau der Fall, fuer den die Regel gilt: Wird ein Fehler behandelt,
# der im Bericht **nicht sichtbar** gewesen waere, wird die Sichtbarkeit im
# selben Arbeitsgang mitgebaut. Eine Zeile im Bericht haette die Messungen
# eines ganzen Abends ersetzt.
#
# Das Overlay haengt seine Auskunft hier ein; `report.build()` fragt sie ab.
# Ueber ein Modul, weil der Bericht aus `seiten.py` gebaut wird und von dort
# das Overlay nicht erreichbar ist — derselbe Weg wie beim Schloss-Rueckruf.
STATE_REPORT = [None]

# ⚠⚠ **Der Schalter in den Einstellungen muss mitgehen.** Das Durchreichen
# laesst sich an ZWEI Stellen umlegen: mit dem Schloss am Overlay und mit dem
# Schiebeschalter auf der Seite „Anzeige". Wer die Seite offen hatte und das
# Schloss benutzte, sah dort weiter den alten Zustand — richtig wurde er erst,
# wenn man die Seite schloss und neu aufrief. Zwei Anzeigen fuer denselben
# Zustand, die sich widersprechen, sind schlimmer als eine.
#
# Die Seite haengt hier ihre Zeichenfunktion ein; das Overlay ruft sie nach
# jeder Aenderung. `None` heisst schlicht: Die Seite wurde nie gebaut.
CLICK_THROUGH_DISPLAY = [None]

# ⭐ **Dasselbe fuer die Ecken-Auswahl** (13.09.2026). Wer das Overlay mit der
# Hand an eine andere Stelle zieht, hat damit gesagt „hier will ich es" — die
# eingestellte Ecke muss dann weichen, sonst schnappt das Fenster beim naechsten
# Anlass zurueck. Gemeldet: „beim Schliessen des Einstellungsfensters wird die
# Position des Overlays wieder zurueckgesetzt, ich wollte es auf meinen 2.
# Bildschirm ziehen."
#
# Das Overlay stellt die Einstellung dabei selbst auf „frei verschiebbar" — und
# die Auswahlliste auf der Seite „Anzeige" muss das sehen. Sonst steht dort
# weiter „unten links", waehrend das Fenster woanders sitzt: zwei Anzeigen fuer
# denselben Zustand, die sich widersprechen.
CORNER_DISPLAY = [None]

# Dasselbe für „Protokolle erneut einlesen". Beide Bedienelemente — der Knopf am
# Overlay und der in den Einstellungen — rufen hier an; die Arbeit macht der
# Watcher-Faden.
#
# ⚠ **Warum nicht einfach in der Oberfläche einlesen.** Der Bestand liegt im
# Watcher-Faden und wird von dort geschrieben. Läse eine Seite nebenher ein und
# speicherte, überschriebe der Faden das beim nächsten Fund mit seinem eigenen,
# älteren Stand — die neu gefundenen Baupläne wären wieder weg. Es gibt genau
# einen Ort, an dem der Bestand angefasst wird, und das bleibt so.
RESCAN_CALLBACK = [None]


def request_rescan():
    """Bitten, alle Protokolle noch einmal durchzusehen.

    Gibt zurück, ob jemand zugehört hat — ohne laufenden Watcher passiert
    nichts, und der Aufrufer soll das sagen können statt so zu tun."""
    ruf = RESCAN_CALLBACK[0]
    if ruf is None:
        return False
    try:
        ruf()
        return True
    except Exception:
        return False


def set_click_through(fenster, an):
    """Klicks durchreichen (oder wieder abfangen). Gibt zurück, ob es geklappt hat."""
    geklappt = (_windows_click_through(fenster, an) if WINDOWS
                else _x11_click_through(fenster, an))
    ruf = LOCK_CALLBACK[0]
    if ruf is not None:
        try:
            ruf(an and geklappt)
        except Exception:
            pass                          # das Schloss darf das Schalten nie kippen
    return geklappt


# ------------------------------------------------ Zweiter Start holt das Fenster

def start_watchdog(beim_ruf):
    """Lauschen, ob jemand das Fenster hervorholen möchte.

    Warum überhaupt: Im Pop-up-Betrieb ist das Overlay die meiste Zeit unsichtbar.
    Ohne Rückweg wäre das Werkzeug dann unerreichbar — man käme an keine Liste und
    an keine Einstellung mehr. Statt einen eigenen Tastatur-Haken ins System zu
    setzen (unter Windows fragil, unter Wayland unmöglich), nutzen wir den
    einfachsten Weg, den jedes System schon kennt: **das Programm noch einmal
    starten**. Die zweite Version merkt, dass schon eine läuft, sagt ihr Bescheid
    und beendet sich. Auf die Verknüpfung lässt sich dann eine ganz normale
    Tastenkombination legen.

    `beim_ruf` wird im Lausch-Thread aufgerufen — dort nichts zeichnen, sondern
    die Arbeit an den Tk-Thread übergeben (`after`).
    """
    try:
        horcher = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        horcher.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        horcher.bind(('127.0.0.1', WATCHDOG_PORT))
        horcher.listen(4)
    except OSError:
        return False                     # läuft schon — wir sind die zweite Version
    _watchdog[0] = horcher

    def lauschen():
        while True:
            try:
                verbindung, _ = horcher.accept()
            except OSError:
                return
            try:
                verbindung.settimeout(2)
                verbindung.recv(64)
                beim_ruf()
            except Exception:
                pass
            finally:
                try:
                    verbindung.close()
                except OSError:
                    pass

    faden = threading.Thread(target=lauschen, daemon=True)
    faden.start()
    return True


def stop_watchdog():
    """Den Horcher schließen — nötig vor einem Neustart des Programms.

    ⚠ Ohne das kann sich das Programm nicht selbst neu starten: Die frisch
    gestartete Version sieht den belegten Port, hält sich für die zweite Instanz,
    sagt der alten „zeig dich" und beendet sich. Man bliebe ewig auf der alten
    Version sitzen und wüsste nicht, warum.
    """
    horcher = _watchdog[0]
    _watchdog[0] = None
    if horcher is None:
        return
    # ⚠ **`close()` allein genügt nicht** — und daran ist der Selbst-Neustart
    # unter Linux gescheitert, drei Anläufe lang.
    #
    # Im Lausch-Faden steht `accept()` und wartet. Ein `close()` aus einem
    # anderen Faden weckt es nicht: Der Faden bleibt hängen, der Deskriptor
    # bleibt gültig, **der Port bleibt belegt**. Die frisch gestartete Fassung
    # kann sich dann nicht binden, hält sich für die zweite Instanz — und
    # beendet sich planmäßig wieder.
    #
    # Für den Nutzer sah das aus wie „geht aus und kommt nicht wieder". Im
    # Protokoll stand am 27.08.2026 endlich der Beweis: `neustart_tot,
    # Rückgabewert 0 — keine Ausgabe`. Kein Absturz, sondern ein geordneter
    # Abgang. Gemessen, nachdem zwei geratene Reparaturen es nicht gelöst hatten.
    #
    # `shutdown()` bricht das wartende `accept()` ab — erst danach gibt `close()`
    # den Port wirklich frei.
    try:
        horcher.shutdown(socket.SHUT_RDWR)
    except OSError:
        pass                      # schon zu, oder nie verbunden — beides egal
    try:
        horcher.close()
    except OSError:
        pass


def please_show():
    """Einer laufenden Version sagen, sie soll sich zeigen. True = ausgerichtet."""
    try:
        with socket.create_connection(('127.0.0.1', WATCHDOG_PORT), timeout=2) as s:
            s.sendall(b'zeigen')
        return True
    except OSError:
        return False
