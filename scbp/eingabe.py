# SPDX-License-Identifier: GPL-3.0-only
#
# SC BP Watcher — einen Knopfdruck abwarten
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
„Druecke jetzt den Knopf" — Eingaben erkennen, ohne Zusatzpakete.

## Warum ueberhaupt

Eine Belegung von Hand einzutippen heisst, die Knopfnummer zu kennen. Die
steht auf keinem Stick drauf; man zaehlt sie ab, vertut sich, und merkt es
erst im Gefecht. Deshalb macht es jedes ernsthafte Werkzeug so: Der Spieler
drueckt den Knopf, den er meint.

## Wie es ohne Fremdpakete geht

**Linux:** `/dev/input/js*` liefert 8-Byte-Ereignisse — vier Byte Zeit, zwei
Byte Wert, ein Byte Art, ein Byte Nummer. Mehr als `os`, `struct` und
`select` braucht es nicht. Gemessen am 04.09.2026: 32 Knoepfe und 6 Achsen
werden sauber gemeldet.

**Windows:** `winmm.dll` bringt `joyGetPosEx` mit, erreichbar ueber `ctypes` —
ebenfalls Standardbibliothek. Die Knoepfe stehen dort als Bitmaske in einem
Feld; abgefragt wird zyklisch, statt auf Ereignisse zu warten.

⚠️ **Der Windows-Weg ist ungetestet.** Er ist nach der Dokumentation gebaut,
aber hier stand kein Windows zum Ausprobieren bereit. Schlaegt er fehl, faellt
die Oberflaeche auf die Eingabe von Hand zurueck — das ist unbequem, aber
nichts geht kaputt.

## ⭐ Die Bruecke zu Star Citizen: die Kennung IST die Geraetenummer

Star Citizen benennt seine Geraete mit einer Kennung wie

    {03F33344-0000-0000-0000-504944564944}

Das ist kein Zufallswert: `504944564944` ist schlicht `PIDVID` in ASCII, und
davor stehen **PID und VID** des USB-Geraets, je vier Stellen hexadezimal.
Am 04.09.2026 an drei Geraeten gegengeprueft — Linux meldet in
`/sys/class/input/js0/device/id/` genau dieselben Werte.

Damit laesst sich ein gedrueckter Knopf **eindeutig** dem richtigen Geraet in
der `actionmaps.xml` zuordnen, ohne Namensvergleich und ohne Raten.

⚠ Bei zwei **baugleichen** Sticks sind VID und PID gleich. Dann bleibt die
Zuordnung mehrdeutig, und die Oberflaeche muss nachfragen, statt zu raten.
Bei den verbreiteten HOSAS-Aufbauten vergeben die Hersteller pro Seite eigene
PIDs, weshalb der Fall selten ist — aber es gibt ihn.

## ⚠ Was hier NICHT passiert

**Kein Mithoeren im Hintergrund.** Gelesen wird nur, solange der Spieler
ausdruecklich auf „Taste druecken" gewartet hat, und laengstens ein paar
Sekunden. Ein Werkzeug, das dauerhaft Eingabegeraete mitliest, ist genau das,
was man von einem Overlay nicht will — und was Virenscanner zu Recht
anstreichen.
"""
import os
import re
import struct
import sys
import time

WINDOWS = sys.platform.startswith('win')

# Ereignisarten im Linux-Joystick-Protokoll.
ART_KNOPF = 0x01
ART_ACHSE = 0x02
ART_INIT = 0x80          # beim Oeffnen: der Ist-Zustand, kein echter Druck

# So heisst die Kennung, die Star Citizen schreibt: PID, VID, dann „PIDVID".
KENNUNG_MUSTER = '%04X%04X-0000-0000-0000-504944564944'

# Ab dieser Auslenkung gilt eine Achse als bewegt (Bereich -32767..32767).
# Bewusst hoch: Ein Stick ruht selten exakt auf null, und eine zittrige Achse
# darf keine Belegung ausloesen.
ACHSEN_SCHWELLE = 24000


def kennung_aus_ids(vid, pid):
    """Aus VID und PID die Kennung bauen, die Star Citizen benutzt."""
    return KENNUNG_MUSTER % (pid, vid)


def _linux_geraete():
    """Die Joysticks des Systems mit ihrer Star-Citizen-Kennung.

    Liefert `[{'pfad': '/dev/input/js0', 'name': …, 'kennung': …}, …]`.
    """
    heraus = []
    basis = '/sys/class/input'
    try:
        namen = sorted(n for n in os.listdir(basis) if n.startswith('js'))
    except OSError:
        return heraus
    for name in namen:
        pfad = '/dev/input/' + name
        if not os.path.exists(pfad):
            continue
        eintrag = {'pfad': pfad, 'name': '', 'kennung': ''}
        for feld, ziel in (('device/name', 'name'),):
            try:
                with open(os.path.join(basis, name, feld)) as f:
                    eintrag[ziel] = f.read().strip()
            except OSError:
                pass
        try:
            with open(os.path.join(basis, name, 'device/id/vendor')) as f:
                vid = int(f.read().strip(), 16)
            with open(os.path.join(basis, name, 'device/id/product')) as f:
                pid = int(f.read().strip(), 16)
            eintrag['kennung'] = kennung_aus_ids(vid, pid)
        except (OSError, ValueError):
            pass
        heraus.append(eintrag)
    return heraus


def _linux_warten(dauer, abbruch=None):
    """Auf den ersten echten Knopfdruck warten (Linux).

    Liefert `{'kennung':…, 'eingabe':…, 'name':…}` oder `None`.
    """
    import select

    geraete = _linux_geraete()
    offen = {}
    for g in geraete:
        try:
            offen[os.open(g['pfad'], os.O_RDONLY | os.O_NONBLOCK)] = g
        except OSError:
            continue
    if not offen:
        return None
    ende = time.time() + dauer
    try:
        # ⚠ Die ersten Ereignisse nach dem Oeffnen tragen das Init-Bit und
        # beschreiben nur den Ist-Zustand. Wer sie mitzaehlt, bekommt sofort
        # einen „Druck", ohne dass jemand etwas angefasst hat.
        while time.time() < ende:
            if abbruch is not None and abbruch():
                return None
            bereit, _, _ = select.select(list(offen), [], [], 0.15)
            for kennung in bereit:
                try:
                    roh = os.read(kennung, 8)
                except (BlockingIOError, OSError):
                    continue
                if len(roh) < 8:
                    continue
                _zeit, wert, art, nummer = struct.unpack('<IhBB', roh)
                if art & ART_INIT:
                    continue
                g = offen[kennung]
                if art & ART_KNOPF and wert:
                    # ⚠ Star Citizen zaehlt Knoepfe ab **eins**, Linux ab
                    # null. Ohne das Plus sitzt jede Belegung einen Knopf
                    # daneben — und das faellt erst im Spiel auf.
                    return {'kennung': g['kennung'],
                            'eingabe': 'button%d' % (nummer + 1),
                            'name': g['name']}
                if art & ART_ACHSE and abs(wert) >= ACHSEN_SCHWELLE:
                    achse = _achsenname(nummer)
                    if achse:
                        return {'kennung': g['kennung'], 'eingabe': achse,
                                'name': g['name']}
    finally:
        for kennung in offen:
            try:
                os.close(kennung)
            except OSError:
                pass
    return None


# Die uebliche Reihenfolge der Achsen, wie DirectInput sie meldet.
# ⚠ Das ist eine **Annahme**, keine Messung: Welche Achse das Spiel als `x`
# fuehrt, haengt am Treiber. Deshalb darf die Oberflaeche eine erkannte Achse
# anzeigen und bestaetigen lassen, statt sie stillschweigend zu setzen.
ACHSEN = ('x', 'y', 'z', 'rotx', 'roty', 'rotz', 'slider1', 'slider2')


def _achsenname(nummer):
    return ACHSEN[nummer] if 0 <= nummer < len(ACHSEN) else ''


def _windows_warten(dauer, abbruch=None):
    """Auf den ersten Knopfdruck warten (Windows, ueber `winmm`).

    ⚠️ **Ungetestet** — siehe Kopf des Moduls. Bei jedem Fehler kommt `None`
    zurueck, damit die Oberflaeche auf die Eingabe von Hand umschalten kann.
    """
    try:
        import ctypes
        from ctypes import wintypes
    except Exception:
        return None

    class JOYCAPS(ctypes.Structure):
        _fields_ = [('wMid', wintypes.WORD), ('wPid', wintypes.WORD),
                    ('szPname', wintypes.WCHAR * 32),
                    ('wXmin', wintypes.UINT), ('wXmax', wintypes.UINT),
                    ('wYmin', wintypes.UINT), ('wYmax', wintypes.UINT),
                    ('wZmin', wintypes.UINT), ('wZmax', wintypes.UINT),
                    ('wNumButtons', wintypes.UINT),
                    ('wPeriodMin', wintypes.UINT),
                    ('wPeriodMax', wintypes.UINT),
                    ('wRmin', wintypes.UINT), ('wRmax', wintypes.UINT),
                    ('wUmin', wintypes.UINT), ('wUmax', wintypes.UINT),
                    ('wVmin', wintypes.UINT), ('wVmax', wintypes.UINT),
                    ('wCaps', wintypes.UINT),
                    ('wMaxAxes', wintypes.UINT), ('wNumAxes', wintypes.UINT),
                    ('wMaxButtons', wintypes.UINT),
                    ('szRegKey', wintypes.WCHAR * 32),
                    ('szOEMVxD', wintypes.WCHAR * 260)]

    class JOYINFOEX(ctypes.Structure):
        _fields_ = [('dwSize', wintypes.DWORD), ('dwFlags', wintypes.DWORD),
                    ('dwXpos', wintypes.DWORD), ('dwYpos', wintypes.DWORD),
                    ('dwZpos', wintypes.DWORD), ('dwRpos', wintypes.DWORD),
                    ('dwUpos', wintypes.DWORD), ('dwVpos', wintypes.DWORD),
                    ('dwButtons', wintypes.DWORD),
                    ('dwButtonNumber', wintypes.DWORD),
                    ('dwPOV', wintypes.DWORD), ('dwReserved1', wintypes.DWORD),
                    ('dwReserved2', wintypes.DWORD)]

    try:
        winmm = ctypes.WinDLL('winmm')
        anzahl = winmm.joyGetNumDevs()
        if not anzahl:
            return None
        geraete = {}
        for i in range(anzahl):
            caps = JOYCAPS()
            if winmm.joyGetDevCapsW(i, ctypes.byref(caps),
                                    ctypes.sizeof(caps)) != 0:
                continue
            geraete[i] = {'name': caps.szPname,
                          'kennung': kennung_aus_ids(caps.wMid, caps.wPid)}
        if not geraete:
            return None

        # Ausgangszustand merken, damit ein bereits gehaltener Knopf nicht
        # sofort als Druck gilt — das Gegenstueck zum Init-Bit unter Linux.
        vorher = {}
        for i in geraete:
            info = JOYINFOEX()
            info.dwSize = ctypes.sizeof(info)
            info.dwFlags = 0x000000FF                 # JOY_RETURNALL
            if winmm.joyGetPosEx(i, ctypes.byref(info)) == 0:
                vorher[i] = info.dwButtons

        ende = time.time() + dauer
        while time.time() < ende:
            if abbruch is not None and abbruch():
                return None
            for i, g in geraete.items():
                info = JOYINFOEX()
                info.dwSize = ctypes.sizeof(info)
                info.dwFlags = 0x000000FF
                if winmm.joyGetPosEx(i, ctypes.byref(info)) != 0:
                    continue
                neu = info.dwButtons & ~vorher.get(i, 0)
                if neu:
                    nummer = (neu & -neu).bit_length()    # unterstes Bit
                    return {'kennung': g['kennung'],
                            'eingabe': 'button%d' % nummer,
                            'name': g['name']}
                vorher[i] = info.dwButtons
            time.sleep(0.03)
    except Exception:
        return None
    return None


# --------------------------------------------------------- Tastatur und Maus
#
# ⚠⚠ **Tastatur und Maus werden NICHT mitgelesen, sondern abgefragt.**
#
# Fuer Sticks oeffnet dieses Modul Geraetedateien. Fuer die Tastatur waere das
# Gegenstueck ein Mitlesen aller Tastendruecke des Systems — also genau das,
# was ein Keylogger tut. Ein Werkzeug, das das tut, gehoert zu Recht von jedem
# Virenscanner angestrichen, und der Watcher hat mit Fehlalarmen ohnehin
# genug zu tun.
#
# Stattdessen faengt das **eigene Fenster** die Taste ab, waehrend es den
# Eingabezeiger hat (`<KeyPress>`, `<Button>`, `<MouseWheel>` in tkinter).
# Das ist plattformunabhaengig, braucht keine Rechte, und ausserhalb des
# Dialogs wird nichts gesehen.
#
# Die Umsetzung von Tk-Namen auf die Schreibweise des Spiels steht hier, weil
# sie zum Thema gehoert — aufgerufen wird sie aus der Oberflaeche.

# Tk nennt Tasten anders als Star Citizen. Was hier nicht steht, wird
# kleingeschrieben durchgereicht — das deckt Buchstaben und Ziffern ab.
# ⚠ Die Zielnamen sind an den Werkseinstellungen des Spiels abgelesen, nicht
# erfunden (99 verschiedene, Stand 04.09.2026).
TK_ZU_SC = {
    'Escape': 'escape', 'Return': 'enter', 'BackSpace': 'backspace',
    'Tab': 'tab', 'space': 'space', 'Caps_Lock': 'capslock',
    'Shift_L': 'lshift', 'Shift_R': 'rshift',
    'Control_L': 'lctrl', 'Control_R': 'rctrl',
    'Alt_L': 'lalt', 'Alt_R': 'ralt', 'ISO_Level3_Shift': 'ralt',
    'Up': 'up', 'Down': 'down', 'Left': 'left', 'Right': 'right',
    'Home': 'home', 'End': 'end', 'Prior': 'pgup', 'Next': 'pgdn',
    'Insert': 'insert', 'Delete': 'delete', 'Pause': 'pause',
    'comma': 'comma', 'period': 'period', 'slash': 'slash',
    'minus': 'minus', 'equal': 'equals', 'semicolon': 'semicolon',
    'apostrophe': 'apostrophe', 'grave': 'grave', 'backslash': 'backslash',
    'bracketleft': 'lbracket', 'bracketright': 'rbracket',
    'KP_0': 'np_0', 'KP_1': 'np_1', 'KP_2': 'np_2', 'KP_3': 'np_3',
    'KP_4': 'np_4', 'KP_5': 'np_5', 'KP_6': 'np_6', 'KP_7': 'np_7',
    'KP_8': 'np_8', 'KP_9': 'np_9',
    'KP_Add': 'np_add', 'KP_Subtract': 'np_subtract',
    'KP_Multiply': 'np_multiply', 'KP_Divide': 'np_divide',
    'KP_Decimal': 'np_period', 'KP_Enter': 'np_enter',
}

# Diese Tasten sind Umschalter — sie stehen VOR der eigentlichen Taste, mit
# Pluszeichen verbunden: `ralt+y`. So schreibt es auch das Spiel.
UMSCHALTER = ('lshift', 'rshift', 'lctrl', 'rctrl', 'lalt', 'ralt')


def taste_aus_tk(keysym):
    """Aus einem Tk-Tastennamen die Schreibweise des Spiels machen.

    Liefert `''`, wenn die Taste nicht sinnvoll belegt werden kann.
    """
    if not keysym:
        return ''
    if keysym in TK_ZU_SC:
        return TK_ZU_SC[keysym]
    if re.match(r'^F([1-9]|1[0-2])$', keysym):
        return keysym.lower()
    if len(keysym) == 1 and (keysym.isalpha() or keysym.isdigit()):
        return keysym.lower()
    return ''


def maus_aus_tk(nummer=None, rad=None):
    """Maustaste oder Rad in der Schreibweise des Spiels.

    ⚠ Tk zaehlt die mittlere Maustaste als **2** und die rechte als **3**,
    Star Citizen genau andersherum (`mouse2` ist rechts). Ohne diese
    Vertauschung landet jede Belegung auf der falschen Taste.
    """
    if rad:
        return 'mwheel_up' if rad > 0 else 'mwheel_down'
    return {1: 'mouse1', 2: 'mouse3', 3: 'mouse2'}.get(nummer, '')


def verfuegbar():
    """Laesst sich auf diesem System ueberhaupt ein Knopfdruck abwarten?"""
    if WINDOWS:
        try:
            import ctypes
            return bool(ctypes.WinDLL('winmm').joyGetNumDevs())
        except Exception:
            return False
    return bool(_linux_geraete())


def warten(dauer=8.0, abbruch=None):
    """Den naechsten Knopfdruck abwarten — hoechstens `dauer` Sekunden.

    `abbruch` ist eine Funktion, die `True` liefert, wenn abgebrochen werden
    soll (etwa weil der Spieler das Fenster geschlossen hat).

    Liefert `{'kennung':…, 'eingabe':…, 'name':…}` oder `None`, wenn nichts
    kam. **Blockiert** — gehoert deshalb in einen eigenen Faden, nie in den
    der Oberflaeche.
    """
    try:
        return (_windows_warten(dauer, abbruch) if WINDOWS
                else _linux_warten(dauer, abbruch))
    except Exception:
        return None
