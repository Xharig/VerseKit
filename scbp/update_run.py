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
Ein-Klick-Update: Laufmarke, Update-Sperre und der Helfer unter Windows.

Bis v3.29.0 endete ein Update unter Windows mit einem geschlossenen Watcher:
Der Installer lief still, startete bei `/SILENT` absichtlich nichts, und der
Nutzer musste selbst wieder starten. Dieses Modul nimmt das ab — und es hält
nicht nur den Ablauf fest, der gelingt, sondern auch, was geschah, wenn er
mittendrin stirbt.

Drei Bausteine, alle nur Standardbibliothek:

* **Die Laufmarke** (`update-lauf.json`) — geschrieben, bevor der Watcher
  abtritt. Der nächste Start liest sie und sagt, was aus dem Update wurde:
  fertig, abgebrochen, gescheitert oder unklar. Die Version prüft dabei der
  **gestartete Watcher selbst** — ein Rückgabewert 0 allein ist kein Beleg.
* **Die Sperre** (`update-sperre.json`) — damit zwei Klicks oder zwei
  Instanzen nicht zwei Installer loslassen.
  ⚠ Bewusst **keine** Portbindung. Unter Windows bindet ein zweiter Prozess
  mit `SO_REUSEADDR` denselben Port anstandslos — gemessen am 11.09.2026.
  Eine Datei, die nur mit `O_EXCL` entsteht, sperrt auf jedem System gleich.
* **Der Helfer** — eine `.cmd` in `%TEMP%`. Sie wartet, bis der alte Watcher
  weg ist, gleicht die Prüfsumme **unmittelbar vor dem Start** noch einmal ab,
  startet den Installer, schreibt dessen Rückgabewert auf und fährt danach den
  Watcher wieder hoch.
"""
import json
import os
import sys
import time

from . import pfade

RUN_FILE = 'update-lauf.json'
RESULT_FILE = 'update-ergebnis.txt'
LOCK_FILE = 'update-sperre.json'
LOG_FILE = 'update-helfer.txt'
LOG_FILE_OLD = 'update-helfer.1.txt'
HELPER_NAME = 'scbp-update-helfer.cmd'

# Nach welchen Rückgabewerten der Helfer den Watcher wieder startet
# (entschieden 11.09.2026):
#
#   0  Installation gelungen
#   2  abgebrochen, bevor etwas geändert wurde
#   3  vor dem Einspielen gescheitert — gemessen: Inno ändert dabei nichts
#   5  abgebrochen während des Einspielens — gemessen: Inno rollt zurück, die
#      bisherige Fassung liegt unverändert da
#
# Jeder andere Wert startet **nichts**. Gemeldet wird er beim nächsten Start
# von Hand, über die Laufmarke.
RESTART_AFTER = (0, 2, 3, 5)

# Eigene Rückgabewerte des Helfers — weit weg von denen des Installers.
RC_CHECKSUM_BAD = 90
RC_OLD_STUCK = 91

# So lange wartet der Helfer, bis die alte Fassung wirklich weg ist.
WAIT_SECONDS = 60
# Eine Sperre, die älter ist, gilt als verwaist — auch wenn die PID noch lebt
# (Windows vergibt PIDs wieder).
LOCK_MAX_AGE = 15 * 60
# Eine Laufmarke, die älter ist, wird still weggeräumt: Wer tagelang nicht
# gestartet hat, braucht keine Meldung über ein Update von damals.
RUN_MAX_AGE = 24 * 3600


# ⚠⚠ **In dieser Datei steht KEIN einziger Pfad.** Alles kommt über die
# Umgebung (`SCBP_*`), die der Watcher beim Start des Helfers setzt.
#
# Warum: `cmd` liest eine `.cmd`-Datei in der OEM-Codepage. Ein Umlaut im
# Benutzernamen — und damit in jedem Pfad unter dem Heimverzeichnis — käme als
# Zeichensalat an, und jeder Pfad darin zeigte ins Leere. Umgebungsvariablen dagegen reicht Windows als Unicode
# durch, und `cmd` setzt sie beim Ausführen ein, ohne sie noch einmal zu
# zerlegen: `&`, `^`, `%`, `!`, Klammern und Apostroph überstehen das, solange
# sie in Anführungszeichen stehen. Deshalb bleibt die Datei reines ASCII.
#
# ⚠ `DisableDelayedExpansion`, sonst verschwände jedes `!` aus einem Pfad.
#
# ⚠ `ping` statt `timeout`: `timeout` bricht ohne Konsole mit „Input
# redirection is not supported" ab — und der Helfer läuft ohne Konsole.
#
# ⚠ Die Prüfsumme prüft `certutil` (liegt jedem Windows bei). Scheitert der
# Abgleich, wird die Datei gelöscht und **nichts** installiert.
# ⚠ **Die Protokollzeilen sind englisch** — wie das Setup-Protokoll von Inno
# daneben. Beides liest nur, wer einen Fehlerbericht auswertet. Ein deutscher
# Satz in dieser Konstante gälte der Textprüfung im Selbsttest als fester
# Oberflächentext, und eine Ausnahme für die ganze Datei würde dort künftig
# echte Funde verdecken.
HELPER_TEMPLATE = r'''@echo off
rem VerseKit - update helper. Rewritten on every update.
rem All paths come from the environment (SCBP_*); none is stored in this file.
setlocal DisableDelayedExpansion
call :log Helper started
set /a waited=0
:old_version
tasklist /FI "PID eq %SCBP_PID%" /NH >"%SCBP_ERGEBNIS%.pid" 2>nul
findstr /C:" %SCBP_PID% " "%SCBP_ERGEBNIS%.pid" >nul && goto still_there
if "%SCBP_PID2%"=="" goto gone
tasklist /FI "PID eq %SCBP_PID2%" /NH >"%SCBP_ERGEBNIS%.pid" 2>nul
findstr /C:" %SCBP_PID2% " "%SCBP_ERGEBNIS%.pid" >nul && goto still_there
goto gone
:still_there
set /a waited+=1
if %waited% GEQ %SCBP_WARTEN% goto stuck
ping -n 2 127.0.0.1 >nul
goto old_version
:stuck
call :log The old version does not exit - nothing installed
set rc=91
goto result
:gone
call :log Old version has exited
certutil -hashfile "%SCBP_SETUP%" SHA256 >"%SCBP_ERGEBNIS%.summe" 2>nul
findstr /I /C:"%SCBP_SHA256%" "%SCBP_ERGEBNIS%.summe" >nul
if errorlevel 1 goto bad_checksum
call :log Checksum matches, starting installer
"%SCBP_SETUP%" /SILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS /DIR="%SCBP_ZIEL%" /LOG="%SCBP_SETUPLOG%"
set rc=%errorlevel%
call :log Installer finished, exit code %rc%
goto result
:bad_checksum
call :log Checksum mismatch - file discarded, nothing installed
call :log Expected %SCBP_SHA256%, certutil said:
type "%SCBP_ERGEBNIS%.summe" >>"%SCBP_LOG%" 2>nul
del "%SCBP_SETUP%" >nul 2>&1
set rc=90
:result
del "%SCBP_ERGEBNIS%.summe" >nul 2>&1
del "%SCBP_ERGEBNIS%.pid" >nul 2>&1
>"%SCBP_ERGEBNIS%" echo %rc%
del "%SCBP_SPERRE%" >nul 2>&1
for %%c in (%SCBP_NEUSTART%) do if "%rc%"=="%%c" goto restart
call :log No restart after exit code %rc%
goto :eof
:restart
call :log Starting the watcher
start "" "%SCBP_EXE%"
goto :eof
:log
>>"%SCBP_LOG%" echo %date% %time% %*
goto :eof
'''


# ------------------------------------------------------------------ Grundlagen

def _path(name):
    return pfade.app_datei(name)


def _make_dir(path):
    try:
        os.makedirs(os.path.dirname(path), exist_ok=True)
    except OSError:
        pass


def _remove(path):
    """Eine Datei entfernen. Fehlt sie, ist das kein Fehler."""
    try:
        os.remove(path)
    except FileNotFoundError:
        pass
    except OSError as ausnahme:
        from . import fehler
        fehler.merken('update_run.remove', ausnahme)


def _json_write(path, data):
    """Erst daneben schreiben, dann tauschen — nie eine halbe Datei."""
    _make_dir(path)
    staging = path + '.neu'
    with open(staging, 'w', encoding='utf-8') as f:
        json.dump(data, f)
    os.replace(staging, path)


def _json_read(path):
    try:
        with open(path, encoding='utf-8') as f:
            data = json.load(f)
        return data if isinstance(data, dict) else None
    except (OSError, ValueError):
        return None


def _norm(version):
    return str(version or '').strip().lower().lstrip('v')


# ---------------------------------------------------------------- Prozesse

def _kernel32():
    import ctypes
    from ctypes import wintypes
    k = ctypes.WinDLL('kernel32', use_last_error=True)
    k.OpenProcess.restype = wintypes.HANDLE
    k.OpenProcess.argtypes = (wintypes.DWORD, wintypes.BOOL, wintypes.DWORD)
    k.GetExitCodeProcess.argtypes = (wintypes.HANDLE,
                                     ctypes.POINTER(wintypes.DWORD))
    k.QueryFullProcessImageNameW.argtypes = (
        wintypes.HANDLE, wintypes.DWORD, wintypes.LPWSTR,
        ctypes.POINTER(wintypes.DWORD))
    k.CloseHandle.argtypes = (wintypes.HANDLE,)
    return k


_QUERY_ONLY = 0x1000          # PROCESS_QUERY_LIMITED_INFORMATION
_STILL_ACTIVE = 259              # STILL_ACTIVE


def pid_alive(pid):
    """Lebt dieser Prozess noch? Ein Fehler beim Fragen zählt als „nein"."""
    try:
        pid = int(pid)
    except (TypeError, ValueError):
        return False
    if pid <= 0:
        return False
    if pfade.WINDOWS:
        import ctypes
        from ctypes import wintypes
        k = _kernel32()
        handle = k.OpenProcess(_QUERY_ONLY, False, pid)
        if not handle:
            # „Zugriff verweigert" heißt: Es gibt ihn, wir dürfen nur nicht
            # hinein. Alles andere heißt: Es gibt ihn nicht.
            return ctypes.get_last_error() == 5
        try:
            code = wintypes.DWORD()
            if not k.GetExitCodeProcess(handle, ctypes.byref(code)):
                return True
            return code.value == _STILL_ACTIVE
        finally:
            k.CloseHandle(handle)
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    except OSError:
        return False
    return True


def _process_image(pid):
    """Die Programmdatei eines Prozesses (nur Windows), sonst None."""
    if not pfade.WINDOWS:
        return None
    import ctypes
    from ctypes import wintypes
    k = _kernel32()
    handle = k.OpenProcess(_QUERY_ONLY, False, int(pid))
    if not handle:
        return None
    try:
        buffer = ctypes.create_unicode_buffer(1024)
        size = wintypes.DWORD(1024)
        if k.QueryFullProcessImageNameW(handle, 0, buffer, ctypes.byref(size)):
            return buffer.value
        return None
    finally:
        k.CloseHandle(handle)


def old_pids(exe=None):
    """Die Prozesse, auf deren Ende der Helfer warten muss.

    ⚠ **Zwei, nicht einer.** Die gepackte `.exe` startet sich zweimal: Ein
    Bootloader entpackt nach `%TEMP%` und startet darin das eigentliche
    Programm. Der Bootloader lebt weiter, bis er seinen Ordner aufgeräumt hat,
    und **hält so lange die `.exe`**. Wartet der Helfer nur auf das Programm,
    greift der Installer nach einer Datei, die noch belegt ist.

    ⚠ Der Elternprozess zählt nur, wenn er **dieselbe Datei** ist. Wer den
    Watcher aus einem Quellcode-Start oder über den Explorer bekommt, hat
    einen Vater, der nie endet — auf den zu warten hieße, nie zu installieren.
    """
    exe = os.path.normcase(os.path.abspath(exe or sys.executable))
    pids = [os.getpid()]
    try:
        parent = os.getppid()
        filename = _process_image(parent)
        if filename and os.path.normcase(os.path.abspath(filename)) == exe:
            pids.append(parent)
    except Exception:
        pass
    return pids


# ------------------------------------------------------------------- Sperre

def _orphaned(path):
    data = _json_read(path)
    if data is None:
        # Unlesbar: Entweder schreibt gerade jemand hinein — dann ist sie
        # Sekundenbruchteile alt —, oder sie ist ein Überbleibsel.
        try:
            return time.time() - os.path.getmtime(path) > 10
        except OSError:
            return True
    try:
        age = time.time() - float(data.get('zeit') or 0)
    except (TypeError, ValueError):
        return True
    if age > LOCK_MAX_AGE:
        return True
    return not pid_alive(data.get('pid'))


def take_lock():
    """True: Wir dürfen. False: Ein anderes Update ist gerade unterwegs.

    ⚠ Scheitert schon das Anlegen (Platte voll, keine Rechte), wird das Update
    **nicht** blockiert. Die Sperre schützt vor dem doppelten Lauf, nicht vor
    einer fremden Datei — das tut die Prüfsumme. Ein Werkzeug, das sich an
    einer fehlenden Sperrdatei aufhängt, wäre schlimmer als der seltene
    Doppellauf.
    """
    path = _path(LOCK_FILE)
    _make_dir(path)
    for _attempt in range(2):
        try:
            fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
        except FileExistsError:
            if not _orphaned(path):
                return False
            _remove(path)
            continue
        except OSError as ausnahme:
            from . import fehler
            fehler.merken('update_run.take_lock', ausnahme)
            return True
        with os.fdopen(fd, 'w', encoding='utf-8') as f:
            json.dump({'pid': os.getpid(), 'zeit': time.time()}, f)
        return True
    return False


def hand_lock_to(pid):
    """Die Sperre gehört ab jetzt dem Helfer — der Watcher tritt ja gleich ab."""
    try:
        _json_write(_path(LOCK_FILE), {'pid': int(pid), 'zeit': time.time()})
    except (OSError, TypeError, ValueError) as ausnahme:
        from . import fehler
        fehler.merken('update_run.hand_lock_to', ausnahme)


def release_lock():
    _remove(_path(LOCK_FILE))


def lock_held():
    """Hält gerade jemand Lebendiges die Sperre?"""
    path = _path(LOCK_FILE)
    return os.path.exists(path) and not _orphaned(path)


# --------------------------------------------------------------- Laufmarke

def begin_run(target, previous, installer, checksum):
    """Festhalten, was gleich passiert — bevor der Watcher abtritt."""
    _remove(_path(RESULT_FILE))
    _json_write(_path(RUN_FILE), {
        'ziel': str(target or ''), 'alt': str(previous or ''),
        'installer': str(installer or ''), 'sha256': str(checksum or ''),
        'start': time.time(),
    })


def read_run():
    return _json_read(_path(RUN_FILE))


def read_result():
    """Der Rückgabewert, den der Helfer aufgeschrieben hat — oder None."""
    try:
        with open(_path(RESULT_FILE), encoding='ascii', errors='replace') as f:
            return int(f.read().split()[0])
    except (OSError, ValueError, IndexError):
        return None


def _cleanup(run):
    _remove(_path(RUN_FILE))
    _remove(_path(RESULT_FILE))
    installer = str(run.get('installer') or '')
    # Nur eine Datei, die erkennbar uns gehört — nie etwas Fremdes.
    #
    # ⚠⚠ Die Prüfung geht über `pfade.gehoert_uns()`, weil sie BEIDE
    # Namen kennen muss. Bis zum 12.09.2026 stand hier nur
    # 'sc-bp-watcher' — ein `VerseKit-Setup.exe` wäre nach dem Update
    # liegen geblieben, während die Laufmarke gelöscht wurde. Vom Prüfer
    # mit einer protokollierenden Attrappe nachgewiesen (F04).
    if installer and pfade.gehoert_uns(installer):
        _remove(installer)


def evaluate(own_version):
    """Beim Start: Was ist aus dem letzten Update geworden?

    Gibt ein Wörterbuch mit `art` zurück — `fertig`, `abgebrochen`, `fehler`
    oder `unklar` — oder None, wenn es nichts zu sagen gibt. Räumt danach auf.

    ⚠ Die Version entscheidet, nicht der Rückgabewert. Meldet der Installer 0,
    läuft aber weiter die alte Fassung, ist das **kein** Erfolg — genau diese
    falsche Erfolgsmeldung soll es nicht geben.

    ⚠ Hält der Helfer die Sperre noch, läuft das Update gerade — wer jetzt
    von Hand startet, bekommt keine Meldung, und die Laufmarke bleibt liegen.
    """
    run = read_run()
    if not run or lock_held():
        return None
    code = read_result()
    target = str(run.get('ziel') or '')
    previous = str(run.get('alt') or '')
    try:
        age = time.time() - float(run.get('start') or 0)
    except (TypeError, ValueError):
        age = RUN_MAX_AGE + 1
    _cleanup(run)
    if not 0 <= age <= RUN_MAX_AGE:
        return None

    own = _norm(own_version)
    if own and own == _norm(target):
        kind = 'fertig'
    elif code is None:
        kind = 'unklar'
    elif code in (2, 5) and own == _norm(previous):
        kind = 'abgebrochen'
    else:
        kind = 'fehler'
    if kind != 'fertig':
        from . import fehler
        fehler.merken('update_run.update_%s' % kind, RuntimeError(
            'Ziel %s, laufend %s, vorher %s, Rückgabewert %s'
            % (target or '?', own_version or '?', previous or '?',
               '–' if code is None else code)))
    return {'art': kind, 'ziel': target, 'alt': previous, 'code': code,
            'eigen': str(own_version or '')}


def message(result):
    """Der Satz für den Nutzer — als `Satz`, damit er beim Sprachwechsel mitzieht."""
    from . import sprache
    kind = result.get('art')
    if kind == 'fertig':
        return sprache.Satz('up_erg_fertig', result.get('ziel'))
    if kind == 'abgebrochen':
        return sprache.Satz('up_erg_abgebrochen', result.get('eigen'))
    if kind == 'unklar':
        return sprache.Satz('up_erg_unklar', result.get('ziel'))
    return sprache.Satz('up_erg_fehler', result.get('ziel'),
                        result.get('code'))


# ------------------------------------------------------------------ Helfer

def rotate_log():
    """Das letzte Protokoll als vorletztes behalten.

    ⚠ Überschreiben hieße: Ein zweiter Versuch löscht genau den Fehler, nach
    dem jemand fragt. Zwei Stände reichen — es geht um den letzten Versuch und
    den davor, nicht um ein Tagebuch.
    """
    previous = _path(LOG_FILE)
    if os.path.exists(previous):
        try:
            os.replace(previous, _path(LOG_FILE_OLD))
        except OSError as ausnahme:
            from . import fehler
            fehler.merken('update_run.rotate_log', ausnahme)


def _log_line(entry):
    """Eine Zeile vom Watcher selbst. Nur ASCII — der Helfer schreibt OEM.

    ⚠ Der Parameter heißt mit Absicht nicht `text`: Die Textprüfung wertet
    jeden Parameter dieses Namens als Oberflächentext. Diese Zeile landet nur
    in der Diagnose und ist englisch wie der Rest des Helfer-Protokolls.
    """
    path = _path(LOG_FILE)
    _make_dir(path)
    try:
        with open(path, 'a', encoding='ascii', errors='backslashreplace',
                  newline='\r\n') as f:
            f.write('%s %s\n' % (time.strftime('%Y-%m-%d %H:%M:%S'), entry))
    except OSError:
        pass


def helper_env(base_env, setup, checksum, target_dir, setup_log,
                    exe, pids):
    """Die Umgebung für den Helfer: die gesäuberte des Watchers plus `SCBP_*`."""
    import tempfile
    env = dict(base_env)
    env.update({
        'SCBP_SETUP': setup,
        'SCBP_SHA256': str(checksum).lower(),
        'SCBP_ZIEL': target_dir,
        # Ohne Ablage schreibt das Setup trotzdem mit — nur eben nach %TEMP%.
        'SCBP_SETUPLOG': setup_log or os.path.join(
            tempfile.gettempdir(), 'scbp-update-setup.txt'),
        'SCBP_LOG': _path(LOG_FILE),
        'SCBP_ERGEBNIS': _path(RESULT_FILE),
        'SCBP_SPERRE': _path(LOCK_FILE),
        'SCBP_EXE': exe,
        'SCBP_PID': str(pids[0]),
        'SCBP_PID2': str(pids[1]) if len(pids) > 1 else '',
        'SCBP_WARTEN': str(WAIT_SECONDS),
        'SCBP_NEUSTART': ' '.join(str(c) for c in RESTART_AFTER),
    })
    return env


def write_helper():
    """Die Vorlage nach `%TEMP%` legen. Gibt den Pfad zurück."""
    import tempfile
    path = os.path.join(tempfile.gettempdir(), HELPER_NAME)
    with open(path, 'w', encoding='ascii', newline='\r\n') as f:
        f.write(HELPER_TEMPLATE)
    return path


def helper_flags():
    """Wie der Helfer gestartet wird — an EINER Stelle, für Programm und Selbsttest.

    ⚠⚠ **Kein `DETACHED_PROCESS`.** Ohne eigene Konsole bekommt jedes
    Konsolenprogramm, das `cmd` startet (`tasklist`, `findstr`, `certutil`),
    eine neue, sichtbare Konsole — und benutzt deren Ein- und Ausgabe statt der
    Umleitungen. Im ersten Echttest (11.09.2026) hing so `find` in einem offenen
    Fenster und wartete auf die Tastatur, und `certutil` schrieb die Summe in
    sein Fenster statt in die Datei. Der Helfer verwarf daraufhin ein
    einwandfreies Update (Rückgabewert 90) — sicher, aber aus dem falschen Grund.

    `CREATE_NO_WINDOW` gibt `cmd` eine eigene, **unsichtbare** Konsole, die alle
    Kinder erben. Die eigene Prozessgruppe löst ihn vom Watcher, der gleich
    abtritt. Der Installer hängt am Helfer, und der lebt bis zu dessen Ende —
    Innos Meldung über einen fehlenden Elternprozess kann so nicht entstehen.
    """
    import subprocess
    return (getattr(subprocess, 'CREATE_NO_WINDOW', 0)
            | getattr(subprocess, 'CREATE_NEW_PROCESS_GROUP', 0))


def start_helper(setup, checksum, target_dir, setup_log, base_env,
                   flags, exe=None):
    """Den Helfer loslassen und ihm die Sperre übergeben. Gibt den Prozess."""
    import subprocess
    import tempfile
    exe = exe or sys.executable
    pids = old_pids(exe)
    helper = write_helper()
    rotate_log()
    _log_line('Watcher hands over: waiting for PID %s, installer %s'
                     % ('/'.join(str(p) for p in pids),
                        os.path.basename(setup)))
    env = helper_env(base_env, setup, checksum, target_dir,
                          setup_log, exe, pids)
    # Das doppelte Anführungszeichen ist cmd-Eigenart: `cmd /c "…"` streicht
    # das äußere Paar, ein Pfad mit Leerzeichen braucht deshalb ein eigenes.
    process = subprocess.Popen('cmd /c ""%s""' % helper, env=env,
                               cwd=tempfile.gettempdir(), creationflags=flags)
    hand_lock_to(process.pid)
    return process
