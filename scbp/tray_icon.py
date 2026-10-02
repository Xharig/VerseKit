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
Das Symbol im Infobereich der Windows-Taskleiste („Ablage", System Tray).

**Wozu.** Das Overlay kann sich zurückhalten und nur bei einem
neuen Bauplan aufblenden. Dann ist es aber die meiste Zeit unsichtbar, und wer
an die Liste oder die Einstellungen will, braucht einen Weg dorthin. Unter Linux
ist das der Startmenü-Eintrag; unter Windows gehört das Symbol neben die Uhr —
dort sucht man Hintergrundprogramme.

**Warum von Hand und nicht mit einer Bibliothek.** Das Programm kommt ohne
Zusatzpakete aus; das ist Absicht und steht so in der Roadmap. Windows bietet
`Shell_NotifyIcon` an, und der Weg dorthin führt über `ctypes`.

**Wie es zusammenhängt.** Ein Symbol im Infobereich braucht ein Fenster, an das
Windows seine Nachrichten schicken kann — Klicks landen dort als selbst
vergebene Nachrichtennummer. Dieses Fenster ist unsichtbar und tut sonst nichts.
Weil es eine eigene Nachrichtenschleife braucht, läuft es in einem eigenen
Faden; alles, was daraufhin passieren soll, wird per Rückruf an den Tk-Faden
übergeben — Tk verträgt keine Aufrufe aus fremden Fäden.

⚠ Alles hier ist gutmütig gebaut: Klappt irgendein Schritt nicht, gibt es eben
kein Symbol, und das Programm läuft weiter. Ein Werkzeug darf nicht daran
scheitern, dass ein Symbol neben der Uhr fehlt.
"""
import ctypes
import os
import sys
import threading
import time

WINDOWS = sys.platform.startswith('win')

# Eigene Nachrichtennummer für alles, was das Symbol meldet. WM_APP ist der
# Bereich, den Windows für Programme frei lässt.
WM_APP = 0x8000
MESSAGE = WM_APP + 17

WM_DESTROY = 0x0002
WM_CLOSE = 0x0010
WM_COMMAND = 0x0111
WM_LBUTTONUP = 0x0202
WM_RBUTTONUP = 0x0205

NIM_ADD, NIM_MODIFY, NIM_DELETE = 0, 1, 2
NIF_MESSAGE, NIF_ICON, NIF_TIP = 0x01, 0x02, 0x04

IMAGE_ICON = 1
LR_LOADFROMFILE, LR_DEFAULTSIZE = 0x0010, 0x0040
IDI_APPLICATION = 32512

TPM_RIGHTBUTTON = 0x0002
MF_STRING = 0x0000
MF_GRAYED = 0x0001
MF_SEPARATOR = 0x0800

# ⚠ Windows schickt diese Nachricht an ALLE obersten Fenster, wenn die
# Taskleiste neu entsteht — beim Explorer-Neustart, aber auch, wenn ein
# Programm sehr früh startet und die Taskleiste noch gar nicht da war. Ohne
# darauf zu hören, ist das Symbol danach für immer weg.
#
# Das passiert etwa mit der Installer-Einstellung `RestartApplications`, die
# das Programm direkt nach dem Setup wieder startet — da ist die Taskleiste
# manchmal noch nicht bereit.
WM_TASKBARCREATED = None          # wird beim Start registriert

# Die erste Befehlsnummer im Menü; jeder anklickbare Eintrag bekommt die
# nächste. Windows meldet einen Klick als WM_COMMAND mit genau dieser Nummer.
CMD_FIRST = 1001



# ---------------------------------------------------------------------------
# ⚠ Signaturen festlegen — sonst stimmen auf 64-Bit-Windows die Handles nicht.
#
# Ohne `restype` nimmt ctypes an, eine Windows-Funktion gebe ein `int` zurück,
# und das ist auf 64 Bit **32 Bit breit**. Fenster-, Icon- und Menü-Handles sind
# aber zeigergroß. Liegt so ein Handle über der 32-Bit-Grenze, kommt bei uns ein
# abgeschnittener Wert an — und der zeigt auf nichts.
#
# Das ist kein theoretisches Problem: Windows vergibt Handles meist im unteren
# Bereich, deshalb geht es fast immer gut. Wenn nicht, bleibt etwa das
# Rechtsklick-Menü **leer**: `CreatePopupMenu` liefert ein gekürztes Handle,
# und die `AppendMenuW` laufen ins Leere, ohne dass es jemand merkt.
#
# Dasselbe Muster kann auch das Symbol selbst gelegentlich ausbleiben lassen;
# mehrfaches Versuchen behandelt nur das Symptom.
def _set_signatures():
    """Einmal beim Laden: sagen, was die Windows-Funktionen wirklich liefern."""
    if not WINDOWS:
        return
    from ctypes import wintypes
    benutzer = ctypes.windll.user32
    kern = ctypes.windll.kernel32

    benutzer.CreatePopupMenu.restype = wintypes.HMENU
    benutzer.CreatePopupMenu.argtypes = []

    benutzer.AppendMenuW.restype = wintypes.BOOL
    benutzer.AppendMenuW.argtypes = [wintypes.HMENU, wintypes.UINT,
                                     ctypes.c_void_p, wintypes.LPCWSTR]

    benutzer.TrackPopupMenu.restype = wintypes.BOOL
    benutzer.TrackPopupMenu.argtypes = [wintypes.HMENU, wintypes.UINT,
                                        ctypes.c_int, ctypes.c_int,
                                        ctypes.c_int, wintypes.HWND,
                                        ctypes.c_void_p]

    benutzer.DestroyMenu.restype = wintypes.BOOL
    benutzer.DestroyMenu.argtypes = [wintypes.HMENU]

    benutzer.LoadImageW.restype = wintypes.HANDLE
    benutzer.LoadImageW.argtypes = [wintypes.HINSTANCE, wintypes.LPCWSTR,
                                    wintypes.UINT, ctypes.c_int, ctypes.c_int,
                                    wintypes.UINT]

    # ⚠ `CreateWindowExW` braucht neben `restype` auch **`argtypes`** — ohne
    # das rät ctypes: Es reicht jedes Argument als `c_int` weiter, also 32 Bit.
    # Das Modulhandle in Argument 11 ist unter 64-Bit-Windows breiter, und der
    # Aufruf endet mit
    #
    #     ArgumentError: argument 11: OverflowError: int too long to convert
    #
    # Das Symbol erscheint dann nie in der Ablage.
    #
    # Deshalb bekommt hier **jede** benutzte Funktion ihre Signatur. Eine
    # halb deklarierte Schnittstelle ist schlimmer als eine gar nicht
    # deklarierte: Sie sieht richtig aus und trägt nur bis zum ersten Handle,
    # das über zwei Milliarden liegt.
    LRESULT = ctypes.c_ssize_t          # in wintypes gibt es den Typ nicht

    benutzer.CreateWindowExW.restype = wintypes.HWND
    benutzer.CreateWindowExW.argtypes = [wintypes.DWORD, wintypes.LPCWSTR,
                                         wintypes.LPCWSTR, wintypes.DWORD,
                                         ctypes.c_int, ctypes.c_int,
                                         ctypes.c_int, ctypes.c_int,
                                         wintypes.HWND, wintypes.HMENU,
                                         wintypes.HINSTANCE, wintypes.LPVOID]

    benutzer.RegisterClassW.restype = wintypes.ATOM
    benutzer.RegisterClassW.argtypes = [ctypes.c_void_p]

    benutzer.DefWindowProcW.restype = LRESULT
    benutzer.DefWindowProcW.argtypes = [wintypes.HWND, wintypes.UINT,
                                        wintypes.WPARAM, wintypes.LPARAM]

    benutzer.GetMessageW.restype = wintypes.BOOL
    benutzer.GetMessageW.argtypes = [ctypes.c_void_p, wintypes.HWND,
                                     wintypes.UINT, wintypes.UINT]

    benutzer.PostMessageW.restype = wintypes.BOOL
    benutzer.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT,
                                      wintypes.WPARAM, wintypes.LPARAM]

    benutzer.RegisterWindowMessageW.restype = wintypes.UINT
    benutzer.RegisterWindowMessageW.argtypes = [wintypes.LPCWSTR]

    benutzer.LoadIconW.restype = wintypes.HICON
    benutzer.LoadIconW.argtypes = [wintypes.HINSTANCE, wintypes.LPCWSTR]

    benutzer.GetCursorPos.restype = wintypes.BOOL
    benutzer.GetCursorPos.argtypes = [ctypes.c_void_p]

    benutzer.SetForegroundWindow.restype = wintypes.BOOL
    benutzer.SetForegroundWindow.argtypes = [wintypes.HWND]

    benutzer.PostQuitMessage.restype = None
    benutzer.PostQuitMessage.argtypes = [ctypes.c_int]

    schale = ctypes.windll.shell32
    schale.Shell_NotifyIconW.restype = wintypes.BOOL
    schale.Shell_NotifyIconW.argtypes = [wintypes.DWORD, ctypes.c_void_p]

    kern.GetModuleHandleW.restype = wintypes.HMODULE
    kern.GetModuleHandleW.argtypes = [wintypes.LPCWSTR]


try:
    _set_signatures()
except Exception:
    # Läuft etwas davon auf einer alten Windows-Version nicht, ist das kein
    # Grund, das Programm nicht zu starten — dann eben ohne Symbol.
    pass

def available():
    return WINDOWS


class TrayIcon(object):
    """Das Symbol neben der Uhr. `start()` und `stop()` — mehr braucht es nicht."""

    # ⚠ Der Standardtitel ist nur ein Notnagel: Der Aufrufer uebergibt
    # `sprache.t('hf_titel')`. Er traegt trotzdem den aktuellen Namen — ein
    # Standardwert mit altem Namen ist eine Zeitbombe fuer den Fall, dass der
    # Aufrufer ihn einmal weglaesst.
    def __init__(self, beim_zeigen=None, beim_beenden=None, titel='Verse-Kit',
                 beim_menue=None):
        self.beim_zeigen = beim_zeigen
        self.beim_beenden = beim_beenden
        # ⭐ Ist `beim_menue` gesetzt, zeichnet der Aufrufer das Menü selbst
        # (der Watcher als Tk-Menü in den Markenfarben — das
        # Windows-Standardmenü ist weiß). `_show_menu()` mit
        # dem grauen Windows-Menü bleibt als Rückfall für alle, die nur
        # `menu_set()` benutzen.
        self.beim_menue = beim_menue
        self.titel = titel
        self.fenster = None
        self._faden = None
        self._laeuft = False
        # Das Rechtsklick-Menü: `[(nummer, text, flags, tat), …]` — gebaut in
        # `menu_set()`, gelesen von `_show_menu()` und `_handle()`.
        self._eintraege = []
        self._befehle = {}
        # ⚠ Muss als Attribut gehalten werden. Ein Rückruf, den nur Windows
        # kennt, wird von Python sonst irgendwann aufgeräumt — und der nächste
        # Klick auf das Symbol beendet das Programm mit einem Speicherauszug.
        self._fensterfunktion = None
        self._klasse = None

    # ------------------------------------------------------------- Aufbau
    def _load_icon(self):
        """Das Programmsymbol — sonst das Standardsymbol von Windows."""
        benutzer = ctypes.windll.user32
        for ordner in (getattr(sys, '_MEIPASS', ''),
                       os.path.dirname(os.path.abspath(sys.executable)),
                       os.path.dirname(os.path.dirname(os.path.abspath(__file__)))):
            if not ordner:
                continue
            pfad = os.path.join(ordner, 'icon.ico')
            if os.path.isfile(pfad):
                kennung = benutzer.LoadImageW(None, pfad, IMAGE_ICON, 0, 0,
                                              LR_LOADFROMFILE | LR_DEFAULTSIZE)
                if kennung:
                    return kennung
        return benutzer.LoadIconW(None, ctypes.c_wchar_p(IDI_APPLICATION))

    def _data(self, flags):
        """Die NOTIFYICONDATAW-Struktur, die Windows erwartet."""
        from ctypes import wintypes

        class NOTIFYICONDATAW(ctypes.Structure):
            _fields_ = [
                ('cbSize', wintypes.DWORD),
                ('hWnd', wintypes.HWND),
                ('uID', wintypes.UINT),
                ('uFlags', wintypes.UINT),
                ('uCallbackMessage', wintypes.UINT),
                ('hIcon', wintypes.HICON),
                ('szTip', wintypes.WCHAR * 128),
                ('dwState', wintypes.DWORD),
                ('dwStateMask', wintypes.DWORD),
                ('szInfo', wintypes.WCHAR * 256),
                ('uVersion', wintypes.UINT),
                ('szInfoTitle', wintypes.WCHAR * 64),
                ('dwInfoFlags', wintypes.DWORD),
            ]

        daten = NOTIFYICONDATAW()
        daten.cbSize = ctypes.sizeof(NOTIFYICONDATAW)
        daten.hWnd = self.fenster
        daten.uID = 1
        daten.uFlags = flags
        daten.uCallbackMessage = MESSAGE
        daten.hIcon = self._symbol
        daten.szTip = self.titel[:127]
        return daten

    def _create_icon(self, versuche=5):
        """Das Symbol bei Windows anmelden — mit Wiederholung.

        ⚠ `Shell_NotifyIcon` schlägt fehl, solange die Taskleiste nicht bereit
        ist. Das passiert öfter, als man denkt: beim Autostart, direkt nach
        einer Installation (der Installer startet das Programm wieder) und bei
        jedem Explorer-Neustart. Vorher wurde das Fehlschlagen stillschweigend
        hingenommen — das Symbol fehlte dann für immer, ohne dass irgendwo
        etwas darüber stand.

        Fünf Versuche im Abstand von einer Sekunde. Reicht auch das nicht,
        kommt später `WM_TASKBARCREATED` und es wird erneut versucht.
        """
        daten = self._data(NIF_MESSAGE | NIF_ICON | NIF_TIP)
        for versuch in range(versuche):
            if ctypes.windll.shell32.Shell_NotifyIconW(NIM_ADD,
                                                       ctypes.byref(daten)):
                return True
            if versuch + 1 < versuche:
                time.sleep(1.0)
        try:
            from . import errors
            errors.record('tray_icon.create_icon',
                          OSError('Shell_NotifyIcon: Fehler %d nach %d '
                                  'Versuchen'
                                  % (ctypes.windll.kernel32.GetLastError(),
                                     versuche)))
        except Exception:
            pass
        return False

    def menu_set(self, eintraege):
        """Das Rechtsklick-Menü festlegen — ohne dass Windows dafür laufen muss.

        `eintraege` ist eine Liste aus

        * `(text, tat)` — ein anklickbarer Punkt; `tat` wird beim Klick gerufen
        * `(text, None)` — eine ausgegraute Auskunftszeile (z. B. die Version)
        * `None` — eine Trennlinie

        Typische Einträge: Fenster zeigen, RSI Launcher, Einstellungen,
        Übersetzung, Discord, Ko-fi, Version, Beenden. Die Texte kommen vom
        Aufrufer — dieses Modul hängt nicht an `sprache`.

        Gibt die Zuordnung Befehlsnummer → Tat zurück (für Prüfungen).
        """
        self._eintraege = []
        self._befehle = {}
        nummer = CMD_FIRST
        for eintrag in eintraege or ():
            if eintrag is None:
                self._eintraege.append((0, None, MF_SEPARATOR, None))
                continue
            text, tat = eintrag
            if tat is None:
                self._eintraege.append((0, text, MF_STRING | MF_GRAYED, None))
                continue
            self._eintraege.append((nummer, text, MF_STRING, tat))
            self._befehle[nummer] = tat
            nummer += 1
        return dict(self._befehle)

    def _show_menu(self):
        """Das Rechtsklick-Menü aus `_eintraege` aufbauen und zeigen."""
        benutzer = ctypes.windll.user32
        menue = benutzer.CreatePopupMenu()
        if not menue:
            return
        try:
            # Der Rückgabewert wurde bisher weggeworfen — deshalb fiel ein
            # leeres Menü niemandem auf. Jetzt steht es im Fehlerbericht.
            for kennung, beschriftung, flags, _tat in self._eintraege:
                if not benutzer.AppendMenuW(menue, flags, kennung,
                                            beschriftung):
                    from . import errors
                    errors.record('tray_icon.show_menu',
                                  OSError('AppendMenuW ist gescheitert (%s), '
                                          'Fehler %d'
                                          % (beschriftung,
                                             ctypes.windll.kernel32.GetLastError())))
            from ctypes import wintypes

            class POINT(ctypes.Structure):
                _fields_ = [('x', wintypes.LONG), ('y', wintypes.LONG)]

            punkt = POINT()
            benutzer.GetCursorPos(ctypes.byref(punkt))
            # ⚠ Ohne SetForegroundWindow bleibt das Menü stehen, bis man ein
            # zweites Mal klickt — ein bekannter Sonderfall von Windows.
            benutzer.SetForegroundWindow(self.fenster)
            benutzer.TrackPopupMenu(menue, TPM_RIGHTBUTTON, punkt.x, punkt.y,
                                    0, self.fenster, None)
            benutzer.PostMessageW(self.fenster, 0, 0, 0)
        finally:
            benutzer.DestroyMenu(menue)

    def _handle(self, fenster, nachricht, wparam, lparam):
        try:
            if nachricht == MESSAGE:
                if lparam == WM_LBUTTONUP:
                    self._call(self.beim_zeigen)
                elif lparam == WM_RBUTTONUP:
                    if self.beim_menue:
                        self._call(self.beim_menue)
                    else:
                        self._show_menu()
                return 0
            if nachricht == WM_COMMAND:
                befehl = wparam & 0xFFFF
                self._call(self._befehle.get(befehl))
                return 0
            if nachricht == WM_DESTROY:
                ctypes.windll.user32.PostQuitMessage(0)
                return 0
        except Exception:
            pass
        return ctypes.windll.user32.DefWindowProcW(fenster, nachricht,
                                                   wparam, lparam)

    @staticmethod
    def _call(was):
        if was:
            try:
                was()
            except Exception:
                pass

    # ------------------------------------------------------------ Betrieb
    def start(self, text_zeigen='Fenster zeigen', text_beenden='Beenden',
              menue=None):
        """Symbol anlegen. Gibt zurück, ob es geklappt hat.

        `menue` — siehe `menu_set()`. Ohne Angabe gibt es nur zwei Punkte:
        Fenster zeigen und Beenden."""
        if not WINDOWS or self._laeuft:
            return False
        if menue is None:
            menue = [(text_zeigen, self.beim_zeigen),
                     (text_beenden, self.beim_beenden)]
        self.menu_set(menue)
        bereit = threading.Event()
        self._geklappt = False
        self._faden = threading.Thread(target=self._loop, args=(bereit,),
                                       daemon=True)
        self._faden.start()
        bereit.wait(5)
        return self._geklappt

    def _loop(self, bereit):
        from ctypes import wintypes
        try:
            benutzer = ctypes.windll.user32
            kern = ctypes.windll.kernel32

            # ⚠ `c_long` ist unter Windows **32 Bit**, der Rueckgabewert einer
            # Fensterfunktion (`LRESULT`) unter 64-Bit-Windows aber 64. Was
            # `DefWindowProcW` liefert, wurde hier also abgeschnitten.
            WNDPROC = ctypes.WINFUNCTYPE(ctypes.c_ssize_t, wintypes.HWND,
                                         wintypes.UINT, wintypes.WPARAM,
                                         wintypes.LPARAM)
            self._fensterfunktion = WNDPROC(self._handle)

            class WNDCLASS(ctypes.Structure):
                _fields_ = [('style', wintypes.UINT),
                            ('lpfnWndProc', WNDPROC),
                            ('cbClsExtra', ctypes.c_int),
                            ('cbWndExtra', ctypes.c_int),
                            ('hInstance', wintypes.HINSTANCE),
                            ('hIcon', wintypes.HICON),
                            ('hCursor', wintypes.HANDLE),
                            ('hbrBackground', wintypes.HBRUSH),
                            ('lpszMenuName', wintypes.LPCWSTR),
                            ('lpszClassName', wintypes.LPCWSTR)]

            klasse = WNDCLASS()
            klasse.lpfnWndProc = self._fensterfunktion
            klasse.lpszClassName = 'SCBPWatcherAblage'
            klasse.hInstance = kern.GetModuleHandleW(None)
            self._klasse = klasse
            benutzer.RegisterClassW(ctypes.byref(klasse))

            self.fenster = benutzer.CreateWindowExW(
                0, klasse.lpszClassName, klasse.lpszClassName, 0,
                0, 0, 0, 0, None, None, klasse.hInstance, None)
            if not self.fenster:
                # ⚠ Nicht **stumm** zurückgehen: Sonst steht im Fehlerbericht
                # kein Fehler, und trotzdem fehlt das Symbol.
                from . import errors
                errors.record('tray_icon.window',
                              OSError('CreateWindowExW lieferte kein Fenster, '
                                      'Fehler %d'
                                      % ctypes.windll.kernel32.GetLastError()))
                bereit.set()
                return

            # Die Nachricht anmelden, mit der Windows das Neuentstehen der
            # Taskleiste meldet — siehe WM_TASKBARCREATED oben.
            global WM_TASKBARCREATED
            if WM_TASKBARCREATED is None:
                WM_TASKBARCREATED = benutzer.RegisterWindowMessageW(
                    'TaskbarCreated')

            self._symbol = self._load_icon()
            self._geklappt = self._create_icon()
            self._laeuft = self._geklappt
            bereit.set()

            nachricht = wintypes.MSG()
            while benutzer.GetMessageW(ctypes.byref(nachricht), None, 0, 0) > 0:
                # Taskleiste neu entstanden (Explorer-Neustart) — das Symbol ist
                # damit weg und muss erneut registriert werden. Ohne das bleibt
                # es bis zum nächsten Programmstart verschwunden.
                if (WM_TASKBARCREATED
                        and nachricht.message == WM_TASKBARCREATED):
                    self._geklappt = self._create_icon(versuche=3)
                    self._laeuft = self._geklappt
                benutzer.TranslateMessage(ctypes.byref(nachricht))
                benutzer.DispatchMessageW(ctypes.byref(nachricht))
        except Exception as ausnahme:
            # ⚠ Dieser Zweig darf nichts verschlucken: Fensterklasse, Fenster,
            # Symbol — alles, was **vor** `_symbol_anlegen` schiefgeht, wäre
            # sonst spurlos weg, und der Bericht zeigte keine Fehler, während
            # das Symbol fehlt.
            try:
                from . import errors
                errors.record('tray_icon.loop', ausnahme)
            except Exception:
                pass                  # selbst das Melden darf nichts umwerfen
            bereit.set()
        finally:
            self._laeuft = False

    def stop(self):
        """Symbol wieder wegnehmen — sonst bleibt eine tote Hülle neben der Uhr.

        ⚠ Kein `DestroyWindow` direkt von hier: Windows lässt ein Fenster
        **nur von dem Faden** zerstören, der es erzeugt hat. Von hier aus
        scheitert der Aufruf still im `except` — das Symbol verschwindet zwar
        (`Shell_NotifyIconW` darf fadenübergreifend), die Nachrichtenschleife
        läuft aber weiter.

        Der Faden ist ein Hintergrundfaden und geht beim Programmende ohnehin
        mit. Wer hier aufräumt, will trotzdem, dass danach nichts mehr läuft.
        Deshalb wird eine Nachricht geschickt, statt fremd zuzugreifen —
        `WM_CLOSE` landet in der
        Schleife, die von selbst zu `WM_DESTROY` und `PostQuitMessage` kommt.
        """
        if not WINDOWS or not self.fenster:
            return
        fenster = self.fenster
        try:
            daten = self._data(NIF_MESSAGE)
            ctypes.windll.shell32.Shell_NotifyIconW(NIM_DELETE,
                                                    ctypes.byref(daten))
            ctypes.windll.user32.PostMessageW(fenster, WM_CLOSE, 0, 0)
        except Exception:
            pass
        finally:
            self.fenster = None
            self._laeuft = False
