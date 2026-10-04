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
Eine Adresse öffnen — und unter Windows notfalls über das System selbst.

Python prüft Zertifikate gegen die Stammzertifikate, die **schon** im
Windows-Speicher liegen. Windows selbst lädt fehlende Stammzertifikate aber
erst nach, wenn eine Windows-Verbindung sie braucht. Fehlt das passende, endet
jeder Abruf mit `CERTIFICATE_VERIFY_FAILED`, obwohl Browser und Spiel-Launcher
dieselbe Seite problemlos öffnen.

`open_url()` versucht es erst normal. Scheitert unter Windows **genau** die
Zertifikatsprüfung, geht derselbe Abruf über WinINet — die Verbindung, die
auch der Systembrowser nutzt. Sie prüft das Zertifikat ebenso, holt aber ein
fehlendes Stammzertifikat nach. Danach klappt meist auch der normale Weg
wieder.

Die Antwort verhält sich in beiden Fällen gleich: `read(n)`, `headers.get()`,
als `with`-Block nutzbar. Ein HTTP-Fehler kommt als `urllib.error.HTTPError`.
"""
import os
import ssl
import urllib.error
import urllib.request

# Nur unter Windows gibt es den zweiten Weg.
WINDOWS = os.name == 'nt'


def is_certificate_error(exc):
    """Ist die Ausnahme eine gescheiterte Zertifikatsprüfung?"""
    if isinstance(exc, ssl.SSLCertVerificationError):
        return True
    reason = getattr(exc, 'reason', None)
    if isinstance(reason, ssl.SSLCertVerificationError):
        return True
    return 'CERTIFICATE_VERIFY_FAILED' in str(exc)


def open_url(request, timeout=30):
    """Wie `urllib.request.urlopen(request, timeout=…)`, mit Windows-Ausweg.

    Nur bei einer gescheiterten Zertifikatsprüfung unter Windows wird der
    zweite Weg genommen; jeder andere Fehler kommt unverändert beim Aufrufer
    an. Scheitert auch der zweite Weg, kommt der **erste** Fehler zurück —
    er nennt die eigentliche Ursache.

    Mit gesetztem `SC_BP_NO_NET` bleibt der WinINet-Weg zu: Er geht an
    `urllib` vorbei, und eine Netzsperre, die dort ansetzt, sähe ihn nicht."""
    try:
        return urllib.request.urlopen(request, timeout=timeout)
    except Exception as exc:
        if not WINDOWS or not is_certificate_error(exc):
            raise
        if os.environ.get('SC_BP_NO_NET', '') not in ('', '0'):
            raise
        try:
            return _wininet_open(request, timeout)
        except urllib.error.HTTPError:
            raise
        except Exception:
            raise exc


# ------------------------------------------------------------------ WinINet
_OPEN_TYPE_PRECONFIG = 0
_FLAG_RELOAD = 0x80000000
_FLAG_NO_CACHE_WRITE = 0x04000000
_FLAG_NO_UI = 0x00000200
_FLAG_NO_COOKIES = 0x00080000
_OPTION_CONNECT_TIMEOUT = 2
_OPTION_SEND_TIMEOUT = 5
_OPTION_RECEIVE_TIMEOUT = 6
_QUERY_STATUS_CODE = 19
_QUERY_CONTENT_LENGTH = 5
_QUERY_FLAG_NUMBER = 0x20000000


def _wininet():
    import ctypes
    from ctypes import wintypes
    lib = ctypes.WinDLL('wininet', use_last_error=True)
    lib.InternetOpenW.restype = ctypes.c_void_p
    lib.InternetOpenW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD,
                                  wintypes.LPCWSTR, wintypes.LPCWSTR,
                                  wintypes.DWORD]
    lib.InternetOpenUrlW.restype = ctypes.c_void_p
    lib.InternetOpenUrlW.argtypes = [ctypes.c_void_p, wintypes.LPCWSTR,
                                     wintypes.LPCWSTR, wintypes.DWORD,
                                     wintypes.DWORD, ctypes.c_void_p]
    lib.InternetSetOptionW.restype = wintypes.BOOL
    lib.InternetSetOptionW.argtypes = [ctypes.c_void_p, wintypes.DWORD,
                                       ctypes.c_void_p, wintypes.DWORD]
    lib.HttpQueryInfoW.restype = wintypes.BOOL
    lib.HttpQueryInfoW.argtypes = [ctypes.c_void_p, wintypes.DWORD,
                                   ctypes.c_void_p,
                                   ctypes.POINTER(wintypes.DWORD),
                                   ctypes.POINTER(wintypes.DWORD)]
    lib.InternetReadFile.restype = wintypes.BOOL
    lib.InternetReadFile.argtypes = [ctypes.c_void_p, ctypes.c_void_p,
                                     wintypes.DWORD,
                                     ctypes.POINTER(wintypes.DWORD)]
    lib.InternetCloseHandle.restype = wintypes.BOOL
    lib.InternetCloseHandle.argtypes = [ctypes.c_void_p]
    return lib


def _wininet_open(request, timeout):
    """Den Abruf über WinINet öffnen. Gibt eine `_WinInetResponse` zurück."""
    return _WinInetResponse(request, timeout)


class _WinInetResponse:
    """Antwort eines WinINet-Abrufs mit der Schnittstelle, die die Aufrufer
    von `urlopen` kennen. WinINet folgt Weiterleitungen selbst."""

    def __init__(self, request, timeout):
        import ctypes
        from ctypes import wintypes
        self._ctypes = ctypes
        self._lib = _wininet()
        self._session = None
        self._handle = None
        if isinstance(request, str):
            request = urllib.request.Request(request)
        url = request.full_url
        headers = dict(request.header_items())
        agent = headers.pop('User-agent', None) or 'Python-urllib'
        lines = ''.join('%s: %s\r\n' % kv for kv in headers.items())
        try:
            self._session = self._lib.InternetOpenW(
                agent, _OPEN_TYPE_PRECONFIG, None, None, 0)
            if not self._session:
                raise OSError(ctypes.get_last_error(), 'InternetOpenW')
            millis = wintypes.DWORD(int(max(1, timeout) * 1000))
            for option in (_OPTION_CONNECT_TIMEOUT, _OPTION_SEND_TIMEOUT,
                           _OPTION_RECEIVE_TIMEOUT):
                self._lib.InternetSetOptionW(self._session, option,
                                             ctypes.byref(millis), 4)
            flags = (_FLAG_RELOAD | _FLAG_NO_CACHE_WRITE | _FLAG_NO_UI
                     | _FLAG_NO_COOKIES)
            self._handle = self._lib.InternetOpenUrlW(
                self._session, url, lines or None, len(lines), flags, None)
            if not self._handle:
                raise OSError(ctypes.get_last_error(), 'InternetOpenUrlW')
            self.status = self._query_number(_QUERY_STATUS_CODE) or 0
            length = self._query_number(_QUERY_CONTENT_LENGTH)
            self.headers = {} if length is None else {
                'Content-Length': str(length)}
            if self.status >= 400:
                code = self.status
                self.close()
                raise urllib.error.HTTPError(url, code, 'HTTP Error %d' % code,
                                             None, None)
        except BaseException:
            self.close()
            raise

    def _query_number(self, what):
        ctypes = self._ctypes
        from ctypes import wintypes
        value = wintypes.DWORD(0)
        size = wintypes.DWORD(4)
        ok = self._lib.HttpQueryInfoW(self._handle, what | _QUERY_FLAG_NUMBER,
                                      ctypes.byref(value), ctypes.byref(size),
                                      None)
        return value.value if ok else None

    def read(self, amount=-1):
        ctypes = self._ctypes
        from ctypes import wintypes
        if not self._handle:
            return b''
        parts = []
        remaining = amount if amount is not None and amount >= 0 else None
        while remaining is None or remaining > 0:
            size = 64 * 1024 if remaining is None else min(64 * 1024, remaining)
            buf = ctypes.create_string_buffer(size)
            got = wintypes.DWORD(0)
            if not self._lib.InternetReadFile(self._handle, buf, size,
                                              ctypes.byref(got)):
                raise OSError(ctypes.get_last_error(), 'InternetReadFile')
            if not got.value:
                break
            parts.append(buf.raw[:got.value])
            if remaining is not None:
                remaining -= got.value
        return b''.join(parts)

    def close(self):
        for name in ('_handle', '_session'):
            handle = getattr(self, name, None)
            if handle:
                try:
                    self._lib.InternetCloseHandle(handle)
                except Exception:
                    pass
                setattr(self, name, None)

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.close()
        return False
