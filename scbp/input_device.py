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
KIND_BUTTON = 0x01
KIND_AXIS = 0x02
KIND_INIT = 0x80          # beim Oeffnen: der Ist-Zustand, kein echter Druck

# So heisst die Kennung, die Star Citizen schreibt: PID, VID, dann „PIDVID".
ID_PATTERN = '%04X%04X-0000-0000-0000-504944564944'

# Ab dieser Auslenkung gilt eine Achse als bewegt (Bereich -32767..32767).
# Bewusst hoch: Ein Stick ruht selten exakt auf null, und eine zittrige Achse
# darf keine Belegung ausloesen.
AXIS_THRESHOLD = 24000


def ident_from_ids(vid, pid):
    """Aus VID und PID die Kennung bauen, die Star Citizen benutzt."""
    return ID_PATTERN % (pid, vid)


def _linux_devices():
    """Die Joysticks des Systems mit ihrer Star-Citizen-Kennung.

    Liefert `[{'pfad': '/dev/input/js0', 'name': …, 'kennung': …}, …]`.
    """
    out = []
    base = '/sys/class/input'
    try:
        names = sorted(n for n in os.listdir(base) if n.startswith('js'))
    except OSError:
        return out
    for name in names:
        path = '/dev/input/' + name
        if not os.path.exists(path):
            continue
        entry = {'pfad': path, 'name': '', 'kennung': ''}
        for field, target in (('device/name', 'name'),):
            try:
                with open(os.path.join(base, name, field)) as f:
                    entry[target] = f.read().strip()
            except OSError:
                pass
        try:
            with open(os.path.join(base, name, 'device/id/vendor')) as f:
                vid = int(f.read().strip(), 16)
            with open(os.path.join(base, name, 'device/id/product')) as f:
                pid = int(f.read().strip(), 16)
            entry['kennung'] = ident_from_ids(vid, pid)
        except (OSError, ValueError):
            pass
        out.append(entry)
    return out


# HID-Verwendungen, die als Eingabegeraet fuer ein Spiel zaehlen
# (Usage Page 0x01 „Generic Desktop"): Joystick, Gamepad, Mehrachsen-Geraet.
_GAME_USAGES = (0x04, 0x05, 0x08)

# Gerätename je Systempfad — einmal gelesen, danach gemerkt. Siehe
# `_windows_devices`, warum das Oeffnen nicht bei jedem Takt passieren soll.
_names_by_path = {}


def _windows_devices():
    """Die Joysticks des Systems unter Windows, ueber Raw Input.

    Dasselbe Ergebnis wie `_linux_devices()`: `[{'pfad', 'name', 'kennung'}]`.
    `pfad` ist der Geraetepfad des Systems (`\\\\?\\HID#VID_…`).

    ⛔⛔ **Nicht mehr ueber `winmm`** (16.09.2026). Bis v3.43.0 stand hier
    `joyGetNumDevs()` mit `joyGetDevCapsW()` fuer jeden der 16 Plaetze — und
    die Geraete-Seite ruft das **alle drei Sekunden**. Am 12.09.2026 riss das
    ein Windows mit HOTAS-Aufbau hart herunter:
    `Windows fatal exception: code 0xc0000374` (Heap-Beschaedigung), mitten in
    `joyGetDevCapsW`. Die Struktur war richtig (728 Byte wie `JOYCAPSW`); die
    Beschaedigung entsteht im Treiberpfad dahinter, und dagegen hilft auf
    Python-Seite kein `try` — der Prozess ist einfach weg.

    `GetRawInputDeviceList` fragt nur die Geraeteliste des Systems ab und
    spricht keinen Joystick-Treiber an. Den Namen gibt es danach nur ueber ein
    kurzes Oeffnen des Geraets (`HidD_GetProductString`) — das geschieht
    **einmal je Geraet** und wird gemerkt, nicht bei jedem Takt.

    ⚠ Alle Aufrufe mit `argtypes`/`restype`: Ohne sie reicht `ctypes` Griffe
    als 32-Bit-Zahl durch, und auf 64 Bit ist ein Griff breiter.

    ⚠ **Das ist NICHT die Nummer, die Star Citizen benutzt.** Wie unter Linux
    auch: Die Reihenfolge des Systems und die des Spiels sind zwei
    verschiedene Dinge (gemessen 06.09.2026 — derselbe Stick war unter Linux
    `js0` und im Spiel `js2`). Verbunden werden beide ueber die Kennung.
    """
    import ctypes
    from ctypes import wintypes

    class RAWINPUTDEVICELIST(ctypes.Structure):
        _fields_ = [('hDevice', wintypes.HANDLE), ('dwType', wintypes.DWORD)]

    class RID_DEVICE_INFO_HID(ctypes.Structure):
        _fields_ = [('dwVendorId', wintypes.DWORD),
                    ('dwProductId', wintypes.DWORD),
                    ('dwVersionNumber', wintypes.DWORD),
                    ('usUsagePage', wintypes.USHORT),
                    ('usUsage', wintypes.USHORT)]

    class _Union(ctypes.Union):
        # Die Tastatur-Variante ist mit 24 Byte die groesste — sie bestimmt die
        # Groesse, die Windows in `cbSize` erwartet (32 Byte insgesamt).
        _fields_ = [('hid', RID_DEVICE_INFO_HID),
                    ('keyboard', wintypes.DWORD * 6)]

    class RID_DEVICE_INFO(ctypes.Structure):
        _fields_ = [('cbSize', wintypes.DWORD), ('dwType', wintypes.DWORD),
                    ('u', _Union)]

    RIM_TYPEHID = 2
    RIDI_DEVICENAME = 0x20000007
    RIDI_DEVICEINFO = 0x2000000b
    FAIL = ctypes.c_uint(-1).value

    out = []
    try:
        user32 = ctypes.WinDLL('user32', use_last_error=True)
        list_fn = user32.GetRawInputDeviceList
        list_fn.argtypes = [ctypes.c_void_p, ctypes.POINTER(wintypes.UINT),
                            wintypes.UINT]
        list_fn.restype = wintypes.UINT
        info_fn = user32.GetRawInputDeviceInfoW
        info_fn.argtypes = [wintypes.HANDLE, wintypes.UINT, ctypes.c_void_p,
                            ctypes.POINTER(wintypes.UINT)]
        info_fn.restype = wintypes.UINT

        entry_size = ctypes.sizeof(RAWINPUTDEVICELIST)
        count = wintypes.UINT(0)
        if list_fn(None, ctypes.byref(count), entry_size) == FAIL:
            return []
        # Zwischen Zaehlen und Holen kann ein Geraet dazukommen — dann ist der
        # Puffer zu klein, und es wird mit der neuen Zahl noch einmal versucht.
        for _attempt in range(3):
            slots = max(1, count.value)
            buffer = (RAWINPUTDEVICELIST * slots)()
            count.value = slots          # Eingang: Groesse des Puffers
            got = list_fn(buffer, ctypes.byref(count), entry_size)
            if got != FAIL:
                break
        else:
            return []

        seen = set()
        for entry in buffer[:got]:
            if entry.dwType != RIM_TYPEHID:
                continue
            info = RID_DEVICE_INFO()
            info.cbSize = ctypes.sizeof(RID_DEVICE_INFO)
            size = wintypes.UINT(info.cbSize)
            if info_fn(entry.hDevice, RIDI_DEVICEINFO, ctypes.byref(info),
                       ctypes.byref(size)) == FAIL:
                continue
            hid = info.u.hid
            if hid.usUsagePage != 0x01 or hid.usUsage not in _GAME_USAGES:
                continue

            size = wintypes.UINT(0)
            info_fn(entry.hDevice, RIDI_DEVICENAME, None, ctypes.byref(size))
            if not size.value:
                continue
            name_buf = ctypes.create_unicode_buffer(size.value + 1)
            if info_fn(entry.hDevice, RIDI_DEVICENAME, name_buf,
                       ctypes.byref(size)) == FAIL:
                continue
            path = name_buf.value
            # Ein Geraet mit mehreren Funktionsbloecken taucht mehrfach auf
            # (`…&Col01`, `…&Col02`) — es ist trotzdem EIN Stick.
            key = re.sub(r'&col[0-9a-f]+', '', path.lower())
            if key in seen:
                continue
            seen.add(key)

            if path not in _names_by_path:
                _names_by_path[path] = _windows_product_name(path)
            out.append({'pfad': path,
                        'name': _names_by_path[path],
                        'kennung': ident_from_ids(hid.dwVendorId & 0xFFFF,
                                                  hid.dwProductId & 0xFFFF)})
    except Exception:
        return []
    return out


def _windows_product_name(path):
    """Der Produktname eines HID-Geraets — oder `''`.

    ⚠ Geoeffnet wird **ohne Lese- und Schreibrecht** (Zugriff 0). Das reicht
    fuer die Beschreibung und kollidiert nicht mit dem Spiel, das den Stick
    gerade benutzt.
    """
    import ctypes
    from ctypes import wintypes
    try:
        kernel32 = ctypes.WinDLL('kernel32', use_last_error=True)
        hid = ctypes.WinDLL('hid')
        create = kernel32.CreateFileW
        create.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD,
                           ctypes.c_void_p, wintypes.DWORD, wintypes.DWORD,
                           wintypes.HANDLE]
        create.restype = wintypes.HANDLE
        close = kernel32.CloseHandle
        close.argtypes = [wintypes.HANDLE]
        close.restype = wintypes.BOOL
        product = hid.HidD_GetProductString
        product.argtypes = [wintypes.HANDLE, ctypes.c_void_p, wintypes.ULONG]
        product.restype = wintypes.BOOLEAN

        FILE_SHARE_READ_WRITE = 0x1 | 0x2
        OPEN_EXISTING = 3
        handle = create(path, 0, FILE_SHARE_READ_WRITE, None, OPEN_EXISTING,
                        0, None)
        if not handle or handle == ctypes.c_void_p(-1).value:
            return ''
        try:
            # 126 Zeichen ist die Obergrenze, die HID fuer Zeichenketten
            # vorsieht; der Puffer ist in Byte anzugeben.
            text = ctypes.create_unicode_buffer(127)
            if product(handle, text, ctypes.sizeof(text)):
                return text.value.strip()
            return ''
        finally:
            close(handle)
    except Exception:
        return ''


def devices():
    """Die angeschlossenen Joysticks — auf beiden Systemen gleich.

    ⭐ **Das ist die dritte Sicht auf dieselben Geraete.** Die anderen beiden
    stehen in `joysticks.py`: was das Spiel zuletzt verbunden hatte
    (`Game.log`) und was in der Belegung eine Nummer hat (`actionmaps.xml`).
    Erst zusammen ergeben sie ein Bild — und nur diese hier weiss, was
    **jetzt gerade** angesteckt ist.

    Liefert `[{'pfad', 'name', 'kennung'}, …]`, leer bei einem System ohne
    Joystick oder wenn die Abfrage nicht geht.
    """
    if sys.platform == 'win32':
        return _windows_devices()
    return _linux_devices()


def _linux_wait(duration, stop_flag=None):
    """Auf den ersten echten Knopfdruck warten (Linux).

    Liefert `{'kennung':…, 'eingabe':…, 'name':…}` oder `None`.
    """
    import select

    devices = _linux_devices()
    open_handle = {}
    for g in devices:
        try:
            open_handle[os.open(g['pfad'], os.O_RDONLY | os.O_NONBLOCK)] = g
        except OSError:
            continue
    if not open_handle:
        return None
    end_time = time.time() + duration
    try:
        # ⚠ Die ersten Ereignisse nach dem Oeffnen tragen das Init-Bit und
        # beschreiben nur den Ist-Zustand. Wer sie mitzaehlt, bekommt sofort
        # einen „Druck", ohne dass jemand etwas angefasst hat.
        while time.time() < end_time:
            if stop_flag is not None and stop_flag():
                return None
            ready, _, _ = select.select(list(open_handle), [], [], 0.15)
            for ident in ready:
                try:
                    raw = os.read(ident, 8)
                except (BlockingIOError, OSError):
                    continue
                if len(raw) < 8:
                    continue
                _time_source, value, kind, number = struct.unpack('<IhBB', raw)
                if kind & KIND_INIT:
                    continue
                g = open_handle[ident]
                if kind & KIND_BUTTON and value:
                    # ⚠ Star Citizen zaehlt Knoepfe ab **eins**, Linux ab
                    # null. Ohne das Plus sitzt jede Belegung einen Knopf
                    # daneben — und das faellt erst im Spiel auf.
                    return {'kennung': g['kennung'],
                            'eingabe': 'button%d' % (number + 1),
                            'name': g['name']}
                if kind & KIND_AXIS and abs(value) >= AXIS_THRESHOLD:
                    axis = _axis_name(number)
                    if axis:
                        return {'kennung': g['kennung'], 'eingabe': axis,
                                'name': g['name']}
    finally:
        for ident in open_handle:
            try:
                os.close(ident)
            except OSError:
                pass
    return None


# Die uebliche Reihenfolge der Achsen, wie DirectInput sie meldet.
# ⚠ Das ist eine **Annahme**, keine Messung: Welche Achse das Spiel als `x`
# fuehrt, haengt am Treiber. Deshalb darf die Oberflaeche eine erkannte Achse
# anzeigen und bestaetigen lassen, statt sie stillschweigend zu setzen.
AXES = ('x', 'y', 'z', 'rotx', 'roty', 'rotz', 'slider1', 'slider2')


def _axis_name(number):
    return AXES[number] if 0 <= number < len(AXES) else ''


# „Joystick3OEMName" -> Platz 3. ⚠ Die Registry zaehlt ab **1**, `winmm` ab
# **0** — der Versatz ist gemessen, nicht aus der Konvention geschlossen (siehe
# `_winmm_index_map`).
_SLOT_NAME = re.compile(r'^Joystick(\d+)OEMName$', re.I)
# Der Wert dahinter: „VID_3344&PID_03F3".
_SLOT_IDS = re.compile(r'^VID_([0-9A-F]{4})&PID_([0-9A-F]{4})$', re.I)

# Wo Windows die Zuordnung Platz -> Geraet fuehrt. Darunter liegt je ein
# Schluessel pro Treiber (`DINPUT.DLL`), darin `CurrentJoystickSettings`.
_JOYSTICK_KEY = r'System\CurrentControlSet\Control\MediaResources\Joystick'


def _slots_from_values(pairs):
    """Aus den Registry-Werten die Zuordnung `{platz: kennung}` bauen.

    `pairs` ist eine Folge von `(name, wert)`, wie `winreg.EnumValue` sie
    liefert — etwa `('Joystick1OEMName', 'VID_3344&PID_03F3')`.

    ⚠ Bewusst **getrennt** von der Registry-Abfrage: So laesst sich die
    Auswertung auf jedem System pruefen, auch dort, wo es `winreg` gar nicht
    gibt. Die Abfrage selbst holt nur die Rohwerte.

    ⚠ **Registry ab 1, `winmm` ab 0** — der Versatz ist gemessen, siehe
    `_winmm_index_map`.
    """
    out = {}
    for name, value in pairs:
        slot = _SLOT_NAME.match(name or '')
        if not slot or not isinstance(value, str):
            continue
        ids = _SLOT_IDS.match(value.strip())
        if not ids:
            continue
        number = int(slot.group(1)) - 1
        if number < 0:
            continue
        # Mehrere Treiber koennen denselben Platz fuehren — der erste Fund
        # gilt, spaetere ueberschreiben ihn nicht.
        out.setdefault(number, ident_from_ids(int(ids.group(1), 16),
                                              int(ids.group(2), 16)))
    return out


def _winmm_index_map():
    """Welcher `winmm`-Platz gehoert zu welcher Geraete-Kennung?

    Liefert `{platz: kennung}`, im Zweifel ein leeres Woerterbuch.

    ⛔⛔ **Das ersetzt `joyGetDevCapsW`** (20.09.2026). Frueher stand die Frage
    an den Treiber: `joyGetDevCapsW` liefert `wMid`/`wPid` und damit die
    Kennung. Genau dieser Aufruf hat am 12.09.2026 ein Windows mit
    HOTAS-Aufbau hart heruntergerissen (`0xc0000374`, Heap-Beschaedigung) —
    aus der Geraeteabfrage ist er deshalb seit v3.43.1 raus, hier stand er bis
    heute noch. Die Registry beantwortet dieselbe Frage, ohne einen
    Joystick-Treiber anzusprechen.

    ⚠ **Der Versatz ist gemessen** (20.09.2026, drei VIRPIL-Geraete): Platz 0
    trug `VID_3344&PID_03F3` und stand in der Registry als `Joystick1OEMName`,
    Platz 1 als `Joystick2OEMName`, Platz 2 als `Joystick3OEMName`. Drei von
    drei, keine Abweichung. **Registry ab 1, `winmm` ab 0.**

    ⚠ Liefert die Registry nichts — fremder Treiber, andere Windows-Fassung —,
    bleibt das Ergebnis leer. Der Aufrufer faellt dann auf „nur ein Geraet
    angeschlossen" zurueck und sonst auf die Eingabe von Hand. Geraten wird
    nicht: Eine falsche Kennung schreibt die Belegung am Ende dem falschen
    Stick zu, und das faellt keinem auf.
    """
    if not WINDOWS:
        return {}
    try:
        import winreg
    except Exception:
        return {}

    out = {}
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, _JOYSTICK_KEY) as root:
            drivers = []
            index = 0
            while True:
                try:
                    drivers.append(winreg.EnumKey(root, index))
                except OSError:
                    break
                index += 1
    except OSError:
        return {}

    pairs = []
    for driver in drivers:
        path = '%s\\%s\\CurrentJoystickSettings' % (_JOYSTICK_KEY, driver)
        try:
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, path) as key:
                index = 0
                while True:
                    try:
                        name, value, _kind = winreg.EnumValue(key, index)
                    except OSError:
                        break
                    index += 1
                    pairs.append((name, value))
        except OSError:
            continue
    out.update(_slots_from_values(pairs))
    return out


def _windows_wait(duration, stop_flag=None):
    """Auf den ersten Knopfdruck warten (Windows, ueber `winmm`).

    ⚠️ **Ungetestet** — siehe Kopf des Moduls. Bei jedem Fehler kommt `None`
    zurueck, damit die Oberflaeche auf die Eingabe von Hand umschalten kann.

    ⛔ **Kein `joyGetDevCapsW`** — die Kennung kommt aus `_winmm_index_map()`,
    die Belegtpruefung aus `joyGetPosEx`, der ohnehin gleich danach laeuft und
    nie im Absturzpfad war.
    """
    try:
        import ctypes
        from ctypes import wintypes
    except Exception:
        return None

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
        # ⚠ Feste Typangaben — ohne sie reicht `ctypes` Zeiger und Nummern
        # geraten durch (siehe `_windows_devices`).
        winmm.joyGetNumDevs.argtypes = []
        winmm.joyGetNumDevs.restype = wintypes.UINT
        winmm.joyGetPosEx.argtypes = [wintypes.UINT, ctypes.c_void_p]
        winmm.joyGetPosEx.restype = wintypes.UINT
        count = winmm.joyGetNumDevs()
        if not count:
            return None

        # `winmm` nennt jeden HID-Stick „Microsoft-PC-Joysticktreiber" — der
        # echte Name kommt aus der Raw-Input-Liste, verbunden ueber die Kennung.
        known = {d['kennung']: d.get('name', '') for d in _windows_devices()
                 if d.get('kennung')}
        by_slot = _winmm_index_map()
        if not by_slot and len(known) == 1:
            # Ohne Registry-Eintrag ist die Zuordnung nur dann eindeutig, wenn
            # ueberhaupt nur ein Geraet angeschlossen ist. Bei mehreren wird
            # nicht geraten — dann bleibt `devices` leer und die Oberflaeche
            # schaltet auf die Eingabe von Hand um.
            by_slot = dict.fromkeys(range(count), next(iter(known)))

        # Ausgangszustand merken, damit ein bereits gehaltener Knopf nicht
        # sofort als Druck gilt — das Gegenstueck zum Init-Bit unter Linux.
        # `joyGetPosEx` sagt im selben Zug, ob der Platz ueberhaupt belegt ist.
        devices = {}
        before = {}
        for i in range(count):
            ident = by_slot.get(i)
            if not ident:
                continue
            info = JOYINFOEX()
            info.dwSize = ctypes.sizeof(info)
            info.dwFlags = 0x000000FF                 # JOY_RETURNALL
            if winmm.joyGetPosEx(i, ctypes.byref(info)) != 0:
                continue
            devices[i] = {'name': known.get(ident, ''), 'kennung': ident}
            before[i] = info.dwButtons
        if not devices:
            return None

        end_time = time.time() + duration
        while time.time() < end_time:
            if stop_flag is not None and stop_flag():
                return None
            for i, g in devices.items():
                info = JOYINFOEX()
                info.dwSize = ctypes.sizeof(info)
                info.dwFlags = 0x000000FF
                if winmm.joyGetPosEx(i, ctypes.byref(info)) != 0:
                    continue
                fresh = info.dwButtons & ~before.get(i, 0)
                if fresh:
                    number = (fresh & -fresh).bit_length()    # unterstes Bit
                    return {'kennung': g['kennung'],
                            'eingabe': 'button%d' % number,
                            'name': g['name']}
                before[i] = info.dwButtons
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
TK_TO_SC = {
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
MODIFIERS = ('lshift', 'rshift', 'lctrl', 'rctrl', 'lalt', 'ralt')


def key_from_tk(keysym):
    """Aus einem Tk-Tastennamen die Schreibweise des Spiels machen.

    Liefert `''`, wenn die Taste nicht sinnvoll belegt werden kann.
    """
    if not keysym:
        return ''
    if keysym in TK_TO_SC:
        return TK_TO_SC[keysym]
    if re.match(r'^F([1-9]|1[0-2])$', keysym):
        return keysym.lower()
    if len(keysym) == 1 and (keysym.isalpha() or keysym.isdigit()):
        return keysym.lower()
    return ''


def mouse_from_tk(number=None, wheel=None):
    """Maustaste oder Rad in der Schreibweise des Spiels.

    ⚠ Tk zaehlt die mittlere Maustaste als **2** und die rechte als **3**,
    Star Citizen genau andersherum (`mouse2` ist rechts). Ohne diese
    Vertauschung landet jede Belegung auf der falschen Taste.
    """
    if wheel:
        return 'mwheel_up' if wheel > 0 else 'mwheel_down'
    return {1: 'mouse1', 2: 'mouse3', 3: 'mouse2'}.get(number, '')


def available():
    """Laesst sich auf diesem System ueberhaupt ein Knopfdruck abwarten?"""
    if WINDOWS:
        try:
            import ctypes
            return bool(ctypes.WinDLL('winmm').joyGetNumDevs())
        except Exception:
            return False
    return bool(_linux_devices())


def wait(duration=8.0, stop_flag=None):
    """Den naechsten Knopfdruck abwarten — hoechstens `dauer` Sekunden.

    `abbruch` ist eine Funktion, die `True` liefert, wenn abgebrochen werden
    soll (etwa weil der Spieler das Fenster geschlossen hat).

    Liefert `{'kennung':…, 'eingabe':…, 'name':…}` oder `None`, wenn nichts
    kam. **Blockiert** — gehoert deshalb in einen eigenen Faden, nie in den
    der Oberflaeche.
    """
    try:
        return (_windows_wait(duration, stop_flag) if WINDOWS
                else _linux_wait(duration, stop_flag))
    except Exception:
        return None
