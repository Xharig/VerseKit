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
Geheimnisse ablegen — dort, wo das System sie hält, nie bei den Daten.

Gebraucht für die Verbindung zum KRT Profit Basetool: das Erneuerungs-Token
und unter Linux der DPoP-Schlüssel. Die Vorgaben stehen in gelucs
Sicherheitsanforderungen (`docs/exchange/client-security.md`):

| System | Ablage |
|---|---|
| Windows | **DPAPI** (`CryptProtectData`, an den Windows-Benutzer gebunden), die verschlüsselte Datei unter `%LOCALAPPDATA%` |
| Linux | **Secret Service** über `secret-tool` (unter KDE die KWallet) |
| Linux ohne Secret Service | Datei mit `0600` in einem Ordner mit `0700` — und der Spieler **sieht**, dass es so ist (`backend() == 'file'`) |

⚠⚠ **Nie im Datenordner.** Der liegt bei vielen in einer Cloud oder auf einer
geteilten Platte, wandert in die Sicherungs-ZIP und in den Fehlerbericht. Die
Ablage hier liegt deshalb unter `%LOCALAPPDATA%` bzw. `$XDG_CONFIG_HOME` — beides
geht nicht mit `SC_BP_HOME` mit und nicht in die Sicherung.

⚠ `%LOCALAPPDATA%` statt `%APPDATA%`: Das zweite wandert in Firmennetzen mit dem
Profil auf andere Rechner. Ein DPAPI-Blob taugt dort ohnehin nichts, und der
Schlüssel dazu soll den Rechner nie verlassen.

Für Prüfläufe lenkt `SC_BP_SECRETS` den Ordner um; mit `SC_BP_SECRETS_FILE=1`
wird unter Linux der Secret Service übergangen, damit ein Prüflauf nie in die
echte Schlüsselverwaltung des Spielers schreibt.
"""
import ctypes
import os
import shutil
import subprocess
import sys

WINDOWS = sys.platform == 'win32'

# Kennung im Secret Service — `secret-tool` sucht über diese Attribute.
APPLICATION = 'versekit'
LABEL = 'VerseKit — KRT Profit Basetool'

_backend_cache = [None]
# Woher jedes Geheimnis zuletzt wirklich kam bzw. wohin es ging:
# Name -> 'dpapi' | 'secret-service' | 'file'.
_origin = {}


class SecretError(Exception):
    """Ein Geheimnis ließ sich nicht ablegen oder lesen.

    `code` sagt der Oberfläche, warum: `SECRET_REFUSED` (Secret Service hat
    abgelehnt), `SECRET_UNSAFE` (Datei oder Ordner für andere lesbar und nicht
    zu korrigieren) oder `SECRET_FAILED`."""

    def __init__(self, message, code='SECRET_FAILED'):
        Exception.__init__(self, message)
        self.code = code


def uses_file():
    """Liegt (mindestens) ein Geheimnis in der Datei statt im Secret Service?
    Danach richtet sich der Hinweis auf der Seite — nicht nach der Vermutung,
    welcher Speicher zu Beginn verfügbar war."""
    if WINDOWS:
        return False
    return 'file' in _origin.values() or backend() == 'file'


def folder():
    """Der eigene, nur für den Benutzer lesbare Ordner (`0700`)."""
    override = os.environ.get('SC_BP_SECRETS')
    if override:
        base = override
    elif WINDOWS:
        base = os.path.join(os.environ.get('LOCALAPPDATA')
                            or os.path.expanduser('~'), 'VerseKit', 'basetool')
    else:
        config = (os.environ.get('XDG_CONFIG_HOME')
                  or os.path.join(os.path.expanduser('~'), '.config'))
        base = os.path.join(config, 'versekit', 'basetool')
    return base


def _ensure_folder():
    path = folder()
    os.makedirs(path, mode=0o700, exist_ok=True)
    if not WINDOWS:
        # `makedirs` beachtet den Modus nur beim Anlegen und nur unter der
        # umask — also nachziehen, auch bei einem schon vorhandenen Ordner.
        _tighten(path, 0o700)
    return path


def _tighten(path, mode):
    """Ordner oder Datei auf `mode` bringen — oder ablehnen.

    Gehört der Eintrag einem anderen Benutzer oder lässt er sich nicht
    nachziehen, wird er nicht benutzt."""
    try:
        info = os.lstat(path)
    except FileNotFoundError:
        return
    except OSError as exc:
        raise SecretError('%s: %s' % (os.path.basename(path), exc.strerror),
                          'SECRET_UNSAFE')
    import stat
    if stat.S_ISLNK(info.st_mode):
        raise SecretError('%s: symbolic link' % os.path.basename(path),
                          'SECRET_UNSAFE')
    if hasattr(os, 'getuid') and info.st_uid != os.getuid():
        raise SecretError('%s: owned by another user' % os.path.basename(path),
                          'SECRET_UNSAFE')
    if stat.S_IMODE(info.st_mode) & 0o077:
        try:
            os.chmod(path, mode)
        except OSError as exc:
            raise SecretError('%s: %s' % (os.path.basename(path), exc.strerror),
                              'SECRET_UNSAFE')
        if stat.S_IMODE(os.lstat(path).st_mode) & 0o077:
            raise SecretError('%s: readable by others'
                              % os.path.basename(path), 'SECRET_UNSAFE')


def _file(name):
    safe = ''.join(c for c in name if c.isalnum() or c in '-_.')
    return os.path.join(folder(), safe + ('.dpapi' if WINDOWS else '.secret'))


# ---------------------------------------------------------------- Windows
class _Blob(ctypes.Structure):
    _fields_ = [('cbData', ctypes.c_ulong),
                ('pbData', ctypes.POINTER(ctypes.c_char))]


_UI_FORBIDDEN = 0x1


def _dpapi(data, protect):
    crypt32 = ctypes.WinDLL('crypt32.dll', use_last_error=True)
    kernel32 = ctypes.WinDLL('kernel32.dll')
    kernel32.LocalFree.argtypes = [ctypes.c_void_p]
    kernel32.LocalFree.restype = ctypes.c_void_p
    source = ctypes.create_string_buffer(data, len(data))
    blob_in = _Blob(len(data), ctypes.cast(source, ctypes.POINTER(ctypes.c_char)))
    blob_out = _Blob()
    call = crypt32.CryptProtectData if protect else crypt32.CryptUnprotectData
    call.argtypes = [ctypes.POINTER(_Blob), ctypes.c_void_p, ctypes.c_void_p,
                     ctypes.c_void_p, ctypes.c_void_p, ctypes.c_ulong,
                     ctypes.POINTER(_Blob)]
    call.restype = ctypes.c_int
    try:
        if not call(ctypes.byref(blob_in), None, None, None, None,
                    _UI_FORBIDDEN, ctypes.byref(blob_out)):
            raise SecretError('DPAPI %s fehlgeschlagen (%d)'
                              % ('protect' if protect else 'unprotect',
                                 ctypes.get_last_error()))
        return ctypes.string_at(blob_out.pbData, blob_out.cbData)
    finally:
        ctypes.memset(source, 0, len(data))
        if blob_out.pbData:
            kernel32.LocalFree(ctypes.cast(blob_out.pbData, ctypes.c_void_p))


# ------------------------------------------------------------------ Linux
def _secret_tool():
    if WINDOWS or os.environ.get('SC_BP_SECRETS_FILE', '') not in ('', '0'):
        return None
    return shutil.which('secret-tool')


def _run(args, stdin=None):
    return subprocess.run(args, input=stdin, capture_output=True, timeout=20)


def _secret_service_ok():
    """Antwortet der Secret Service? `lookup` ohne Treffer endet mit 1 und
    **ohne** Fehlermeldung; fehlt der Dienst, steht etwas auf stderr."""
    tool = _secret_tool()
    if not tool:
        return False
    try:
        result = _run([tool, 'lookup', 'application', APPLICATION,
                       'name', '__probe__'])
    except (OSError, subprocess.SubprocessError):
        return False
    return result.returncode in (0, 1) and not result.stderr.strip()


def backend():
    """'dpapi', 'secret-service' oder 'file' — Letzteres zeigt die Seite an."""
    if _backend_cache[0] is None:
        if WINDOWS:
            _backend_cache[0] = 'dpapi'
        elif _secret_service_ok():
            _backend_cache[0] = 'secret-service'
        else:
            _backend_cache[0] = 'file'
    return _backend_cache[0]


def _write_private(path, data):
    """Exklusiv neu anlegen mit `0600` — nie eine fremde Datei überschreiben."""
    _ensure_folder()
    temp = path + '.neu'
    try:
        os.remove(temp)
    except OSError:
        pass
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_BINARY', 0)
    fd = os.open(temp, flags, 0o600)
    with os.fdopen(fd, 'wb') as handle:
        handle.write(data)
        handle.flush()
        os.fsync(handle.fileno())
    os.replace(temp, path)


# --------------------------------------------------------------- Aufrufe
def save(name, text):
    """Ein Geheimnis ablegen (ersetzt ein vorhandenes gleichen Namens)."""
    data = text.encode('utf-8')
    kind = backend()
    if kind == 'dpapi':
        _write_private(_file(name), _dpapi(data, True))
        _origin[name] = 'dpapi'
        return
    if kind == 'secret-service':
        try:
            result = _run([_secret_tool(), 'store',
                           '--label=%s (%s)' % (LABEL, name),
                           'application', APPLICATION, 'name', name], stdin=data)
        except (OSError, subprocess.SubprocessError) as exc:
            raise SecretError('secret-tool: %s' % exc, 'SECRET_REFUSED')
        if result.returncode != 0:
            # Abgelehnt (gesperrte Wallet, abgebrochene Abfrage): kein
            # stiller Ausweg in die Datei — der Spieler erfährt es.
            raise SecretError('secret-tool store: %d' % result.returncode,
                              'SECRET_REFUSED')
        _origin[name] = 'secret-service'
        return
    _write_private(_file(name), data)
    _origin[name] = 'file'


def load(name):
    """Das Geheimnis oder None, wenn es keins gibt."""
    kind = backend()
    if kind == 'secret-service':
        try:
            result = _run([_secret_tool(), 'lookup', 'application', APPLICATION,
                           'name', name])
        except (OSError, subprocess.SubprocessError) as exc:
            raise SecretError('secret-tool: %s' % exc)
        if result.returncode == 0 and result.stdout:
            _origin[name] = 'secret-service'
            return result.stdout.decode('utf-8')
    path = _file(name)
    if not os.path.exists(path):
        return None
    if not WINDOWS:
        _tighten(folder(), 0o700)
        _tighten(path, 0o600)
    try:
        with open(path, 'rb') as handle:
            data = handle.read()
    except FileNotFoundError:
        return None
    except OSError as exc:
        raise SecretError('%s: %s' % (os.path.basename(path), exc.strerror))
    if kind == 'dpapi':
        data = _dpapi(data, False)
        _origin[name] = 'dpapi'
    else:
        _origin[name] = 'file'
    return data.decode('utf-8')


def delete(name):
    """Löschen — überall, wo es liegen könnte. Fehlt es, ist das kein Fehler."""
    tool = _secret_tool()
    if tool:
        try:
            _run([tool, 'clear', 'application', APPLICATION, 'name', name])
        except (OSError, subprocess.SubprocessError):
            pass
    _origin.pop(name, None)
    try:
        os.remove(_file(name))
    except FileNotFoundError:
        pass
    except OSError as exc:
        raise SecretError('%s: %s' % (name, exc.strerror))
