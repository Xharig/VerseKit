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
Die Verbindung zum KRT Profit Basetool (Exchange API v1) — Anmeldung und Netz.

Alles hier folgt gelucs Anleitung unter https://krt-profit.github.io/basetool/
(Quelle `docs/exchange/` im Repo krt-profit/basetool). Die Stellen, an denen
man es leicht falsch macht, stehen jeweils dabei.

**Anmeldung** — Geräte-Anmeldung (RFC 8628): VerseKit holt einen Code, der
Spieler tippt ihn im Browser ein und stimmt zu. ⚠⚠ Gezeigt wird **nur** der
Code und die nackte Adresse, **nie** `verification_uri_complete`: Der Link mit
eingebautem Code überspringt die Seite, auf der das Basetool vor Code-Betrug
warnt, und gewöhnt Leute daran, Code-Links anzuklicken — genau das, was ein
Angreifer ihnen schickt.

**DPoP** (RFC 9449) — jedes Token hängt an einem Schlüssel dieser Installation;
jede Anfrage bekommt einen frischen, signierten Nachweis. Die Signatur macht
gelucs MIT-Referenz (`scbp/dpop_reference/`), der Schlüssel liegt unter Windows
nicht exportierbar in CNG, unter Linux im Secret Service (`secret_store`).

**Server-Nonce** — die erste Anfrage bekommt `401 DPOP_INVALID` mit
`use_dpop_nonce`. Das ist **normal**, kein Fehler: einmal mit der mitgelieferten
Nonce wiederholen, jede Antwort bringt eine frische.

**Aussteller festgenagelt** — nur `https://profit-base.online/auth/realms/iri`.
Die Testumgebung (gelucs Sandbox) lässt sich **ausschließlich** über
Umgebungsvariablen wählen, nie in der Oberfläche — ein Schalter dort wäre ein
Hebel für Betrug:

| Variable | Wofür |
|---|---|
| `SC_BP_BASETOOL_ISSUER` | anderer Aussteller (Sandbox) |
| `SC_BP_BASETOOL_API` | andere Adresse der Schnittstelle |
| `SC_BP_BASETOOL_CLIENT` | andere Programmkennung (`sandbox-client`) |
| `SC_BP_BASETOOL_CA` | Zertifikatsdatei der Sandbox |

Die letzten drei wirken nur zusammen mit `SC_BP_BASETOOL_ISSUER`. Ist irgendeine
gesetzt, zeigt die Seite einen Hinweis.

⚠ **Kein Token und kein Schlüssel verlässt diesen Baustein** — nicht ins
Fehlerprotokoll, nicht in den Bericht, nicht in die Sicherung. Fehlermeldungen
tragen nur den Code des Servers (`CLIENT_REVOKED` …), nie eine Kopfzeile.
"""
import json
import os
import random
import ssl
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
from email.utils import parsedate_to_datetime

from . import secret_store

ISSUER = 'https://profit-base.online/auth/realms/iri'
API = 'https://ingest.profit-base.online/exchange/v1'
CLIENT_ID = 'versekit'
HOMEPAGE = 'https://versekit.xharig.com'

# Die Rechte, die VerseKit anfragen darf. `exchange.connect` und
# `offline_access` immer — ohne `offline_access` endete die Verbindung, sobald
# der Spieler sich im Browser beim Basetool abmeldet.
SCOPE_CONNECT = 'exchange.connect'
SCOPE_OFFLINE = 'offline_access'
SCOPES_BLUEPRINTS = ('exchange.blueprints.read', 'exchange.blueprints.write')
SCOPES_STOCK = ('exchange.stock.read', 'exchange.stock.write')
SCOPES_HANGAR = ('exchange.hangar.read', 'exchange.hangar.write')
SCOPES_DEMAND = ('exchange.demand.read',)

REFRESH_SECRET = 'refresh-token'
KEY_SECRET = 'dpop-key'
KEY_NAME = 'VerseKit Basetool DPoP'

TIMEOUT = 30
# Der Hausschalter „kein Netz" gilt auch hier. ⚠ Nur der eigene Rechner
# (127.0.0.1) bleibt erreichbar — dort läuft der Nachbau aus dem Selbsttest.
OFF = os.environ.get('SC_BP_NO_NET', '') not in ('', '0')
# Das Zugangs-Token lebt 300 s. So viel vorher wird erneuert.
REFRESH_MARGIN = 30


def _env(name, default):
    return (os.environ.get(name) or '').strip() or default


OVERRIDE_VARS = ('SC_BP_BASETOOL_ISSUER', 'SC_BP_BASETOOL_API',
                 'SC_BP_BASETOOL_CLIENT', 'SC_BP_BASETOOL_CA')


def connection_config():
    """Aussteller, Schnittstelle und Kennung — ab Werk die echten.

    Schnittstelle, Kennung und Zertifikat lassen sich nur zusammen mit einem
    anderen Aussteller umstellen (Testumgebung). Ohne ihn bleiben sie beim
    echten Basetool. `override` ist gesetzt, sobald irgendeine der Variablen
    vorhanden ist — die Oberfläche zeigt dann einen Hinweis."""
    sandbox = bool(_env('SC_BP_BASETOOL_ISSUER', ''))
    override = any(_env(name, '') for name in OVERRIDE_VARS)
    if not sandbox:
        return {'issuer': ISSUER, 'api': API, 'client_id': CLIENT_ID, 'ca': '',
                'sandbox': False, 'override': override}
    return {'issuer': _env('SC_BP_BASETOOL_ISSUER', ISSUER).rstrip('/'),
            'api': _env('SC_BP_BASETOOL_API', API).rstrip('/'),
            'client_id': _env('SC_BP_BASETOOL_CLIENT', CLIENT_ID),
            'ca': _env('SC_BP_BASETOOL_CA', ''),
            'sandbox': True, 'override': True}


def user_agent():
    from . import errors
    return 'VerseKit/%s (+%s)' % (errors.VERSION[0] or '0', HOMEPAGE)


class ApiError(Exception):
    """Eine Absage des Basetools — mit seinem festen Code, nie mit Kopfzeilen.

    `action` sagt, was VerseKit daraufhin tut:

    | action | Bedeutung |
    |---|---|
    | `retry` | später noch einmal (Zurückhalten, `retry_after` beachten) |
    | `relogin` | Verbindung ist vorbei, neu verbinden nur auf Knopfdruck |
    | `forget_key` | diese Installation wurde getrennt — Schlüssel verwerfen |
    | `stop` | aufhören und dem Spieler sagen, warum |
    | `update` | VerseKit ist zu alt |
    """
    RETRY = ('RATE_LIMITED', 'DPOP_PROOF_LIMIT', 'QUOTA_EXCEEDED',
             'SERVICE_UNAVAILABLE', 'EXCHANGE_DISABLED', 'REGISTRY_UNAVAILABLE',
             'EXCHANGE_BUDGET_EXHAUSTED', 'RELAY_BUSY', 'BACKEND_RELAY_FAILED',
             'IDEMPOTENCY_IN_PROGRESS', 'NETWORK')
    RELOGIN = ('UNAUTHENTICATED', 'CLIENT_REVOKED', 'invalid_grant')
    FORGET_KEY = ('INSTALLATION_REVOKED',)

    def __init__(self, code, status=0, retry_after=None, problem=None):
        Exception.__init__(self, code)
        self.code = code or 'UNKNOWN'
        self.status = status
        self.retry_after = retry_after
        self.problem = problem or {}
        self.path = ''

    def describe(self):
        """Eine Zeile fürs Fehlerprotokoll — Code, Status, Anfrage, Wartezeit,
        dazu `detail` und `correlationId` des Servers. Nie eine Kopfzeile,
        nie ein Token; `paths.redact` schwärzt zusätzlich alles JWT-Artige."""
        parts = [self.code, 'HTTP %s' % self.status]
        if self.path:
            parts.append(self.path)
        if self.retry_after:
            parts.append('Retry-After %ss' % self.retry_after)
        for key in ('detail', 'correlationId'):
            value = self.problem.get(key)
            if value:
                parts.append('%s=%s' % (key, str(value)[:160]))
        return ' · '.join(parts)

    @property
    def action(self):
        if self.code in self.FORGET_KEY:
            return 'forget_key'
        if self.code in self.RELOGIN:
            return 'relogin'
        if self.code == 'CLIENT_VERSION_UNSUPPORTED':
            return 'update'
        if self.code in self.RETRY or self.status >= 500 or self.status == 0:
            return 'retry'
        return 'stop'


def backoff(attempt, retry_after=None):
    """Wartezeit nach dem `attempt`-ten Fehlschlag (1, 2, …) in Sekunden.

    Verbindlich laut Sync-Anleitung: mindestens 5 s, verdoppelt bis höchstens
    5 min, mit Zufall obendrauf (nie darunter), und **nie** kürzer als
    `Retry-After` — auch wenn das länger als 5 min ist (Tageskontingent)."""
    base = min(300.0, 5.0 * (2 ** max(0, attempt - 1)))
    wait = min(300.0, base + random.uniform(0.0, base * 0.2))
    if retry_after:
        wait = max(wait, float(retry_after))
    return wait


# ---------------------------------------------------------------- Netz
def _ssl_context(cfg):
    if cfg['ca']:
        return ssl.create_default_context(cafile=cfg['ca'])
    return None


def _http(method, url, headers, body, cfg):
    """(Status, Kopfzeilen, Rohinhalt). Absagen kommen als Status zurück, nicht
    als Ausnahme — nur ein Netzfehler wirft `ApiError('NETWORK')`."""
    if OFF and urllib.parse.urlsplit(url).hostname not in ('127.0.0.1',
                                                           'localhost'):
        raise ApiError('NETWORK', 0, problem={'detail': 'SC_BP_NO_NET'})
    request =urllib.request.Request(url, data=body, method=method,
                                     headers=headers)
    try:
        with urllib.request.urlopen(request, timeout=TIMEOUT,
                                    context=_ssl_context(cfg)) as answer:
            return answer.status, dict(answer.headers.items()), answer.read()
    except urllib.error.HTTPError as answer:
        try:
            raw = answer.read()
        except Exception:
            raw = b''
        return answer.code, dict(answer.headers.items()), raw
    except (urllib.error.URLError, OSError, ValueError) as exc:
        raise ApiError('NETWORK', 0, problem={'detail': type(exc).__name__})


def _header(headers, name):
    lowered = name.lower()
    for key, value in (headers or {}).items():
        if key.lower() == lowered:
            return value
    return None


def _json(raw):
    try:
        value = json.loads(raw.decode('utf-8')) if raw else {}
    except (ValueError, UnicodeDecodeError):
        return {}
    return value if isinstance(value, dict) else {'value': value}


def _retry_after(headers):
    value = _header(headers, 'Retry-After')
    try:
        return max(1, int(value)) if value is not None else None
    except ValueError:
        return None


class _Clock:
    """Uhrabweichung je Server, gemessen am `Date` der Antworten.

    ⚠ Keycloak nimmt ein `iat` nur von etwa 25 s vorher bis 15 s nachher —
    ein Rechner, dessen Uhr 15 s vorgeht, scheitert sonst schon an der
    Anmeldung. Gerechnet wird deshalb mit der Zeit des jeweiligen Servers."""

    def __init__(self):
        self.offsets = {}

    @staticmethod
    def _origin(url):
        parts = urllib.parse.urlsplit(url)
        return '%s://%s' % (parts.scheme.lower(), parts.netloc.lower())

    def learn(self, url, headers):
        date = _header(headers, 'Date')
        if not date:
            return
        try:
            server = parsedate_to_datetime(date).timestamp()
        except (TypeError, ValueError, IndexError, OverflowError):
            return
        self.offsets[self._origin(url)] = server - time.time()

    def now(self, url):
        return int(time.time() + self.offsets.get(self._origin(url), 0.0))


# ----------------------------------------------------------- Schlüssel
def _dpop():
    from . import dpop_reference
    return dpop_reference


def _key_name():
    # Prüfläufe legen ihren CNG-Schlüssel unter eigenem Namen an und räumen
    # ihn wieder weg — nie den des Spielers.
    return _env('SC_BP_BASETOOL_KEYNAME', KEY_NAME)


def _pem_of(key):
    """Der private Schlüssel als PEM — nur für den Secret Service unter Linux."""
    import ctypes
    from .dpop_reference import openssl as ossl
    lib = ossl.libcrypto()
    bio = lib.BIO_new(lib.BIO_s_mem())
    if not bio:
        raise ossl.DpopKeyError('BIO_new failed')
    try:
        if lib.PEM_write_bio_PrivateKey(bio, key._handle(), None, None, 0,
                                        None, None) <= 0:
            raise ossl.DpopKeyError('PEM_write_bio_PrivateKey failed')
        data = ctypes.c_void_p()
        length = lib.BIO_ctrl(bio, ossl._BIO_CTRL_INFO, 0, ctypes.byref(data))
        return ctypes.string_at(data.value, length)
    finally:
        lib.BIO_free(bio)


def _key_from_pem(pem):
    import ctypes
    from .dpop_reference import openssl as ossl
    lib = ossl.libcrypto()
    buffer = ctypes.create_string_buffer(pem, len(pem))
    bio = lib.BIO_new_mem_buf(buffer, len(pem))
    if not bio:
        raise ossl.DpopKeyError('BIO_new_mem_buf failed')
    try:
        pkey = lib.PEM_read_bio_PrivateKey(bio, None, None, None)
    finally:
        lib.BIO_free(bio)
        ctypes.memset(buffer, 0, len(pem))
    if not pkey:
        raise ossl.DpopKeyError('no readable private key')
    return ossl.OpenSslKey(pkey)


def open_key(create=True):
    """Den Schlüssel dieser Installation öffnen — und beim ersten Mal anlegen.

    ⚠⚠ **Den Schlüssel behalten.** Ein neuer Schlüssel ist eine neue
    Installation: Der Spieler bekommt eine Meldung über eine neue Verbindung,
    und viele davon schlagen beim Basetool Alarm. Nie je Start neu anlegen."""
    dpop = _dpop()
    if secret_store.WINDOWS:
        from .dpop_reference.cng import CngKey
        if not create:
            return CngKey.open(_key_name())
        return dpop.open_installation_key(_key_name())
    # Unter Linux geht der Schlüssel als PEM durch `secret_store` — also in
    # den Secret Service, und nur ohne ihn in die `0600`-Datei im
    # `0700`-Ordner. EIN Weg für beide Fälle: Sonst läge nach einem
    # Rückfall der Schlüssel an einer Stelle und würde an der anderen gesucht.
    from .dpop_reference.openssl import OpenSslKey
    pem = secret_store.load(KEY_SECRET)
    if pem:
        return _key_from_pem(pem.encode('ascii'))
    if not create:
        return None
    key = OpenSslKey.generate()
    secret_store.save(KEY_SECRET, _pem_of(key).decode('ascii'))
    return key


def delete_key():
    """Schlüssel löschen — beim Trennen und nach `INSTALLATION_REVOKED`."""
    if secret_store.WINDOWS:
        _dpop().delete_installation_key(_key_name())
        return
    secret_store.delete(KEY_SECRET)


# ---------------------------------------------------------- Verbindung
class DeviceLogin:
    """Eine laufende Geräte-Anmeldung. `user_code` und `verification_uri`
    sind alles, was der Spieler zu sehen bekommt."""

    def __init__(self, answer, scopes):
        self.device_code = answer.get('device_code') or ''
        self.user_code = answer.get('user_code') or ''
        self.verification_uri = answer.get('verification_uri') or ''
        self.interval = max(5, int(answer.get('interval') or 5))
        self.expires_at = time.time() + int(answer.get('expires_in') or 600)
        self.scopes = scopes

    def expired(self):
        return time.time() >= self.expires_at


class Connection:
    """Eine Verbindung für das ganze Programm (`CONNECTION`), fadenfest."""

    def __init__(self):
        self._lock = threading.RLock()
        self._key = None
        self._access = None
        self._access_until = 0.0
        self._endpoints = None
        self._clock = _Clock()
        self._nonces = None
        self.granted = ()

    # ------------------------------------------------------------ Zustand
    def connected(self):
        try:
            return bool(secret_store.load(REFRESH_SECRET))
        except secret_store.SecretError:
            return False

    def _key_ready(self):
        with self._lock:
            if self._key is None:
                self._key = open_key(create=True)
            return self._key

    def _nonce_cache(self):
        if self._nonces is None:
            self._nonces = _dpop().NonceCache()
        return self._nonces

    def _proof(self, method, url, access_token=None):
        return _dpop().build_proof(
            self._key_ready(), method, url, access_token=access_token,
            nonce=self._nonce_cache().get(url),
            issued_at=self._clock.now(url))

    def _learn(self, url, headers):
        self._clock.learn(url, headers)
        self._nonce_cache().update(url, headers)

    # ---------------------------------------------------------- Endpunkte
    def endpoints(self):
        """Aus dem Discovery-Dokument — aber nur, wenn es den erwarteten
        Aussteller nennt. Sonst gilt es nicht."""
        with self._lock:
            if self._endpoints is not None:
                return self._endpoints
        cfg = connection_config()
        url = cfg['issuer'] + '/.well-known/openid-configuration'
        status, headers, raw = _http('GET', url, {'User-Agent': user_agent(),
                                                  'Accept': 'application/json'},
                                     None, cfg)
        doc = _json(raw)
        if status != 200 or doc.get('issuer') != cfg['issuer']:
            raise ApiError('ISSUER_MISMATCH' if status == 200 else 'NETWORK',
                           status)
        found = {'device': doc.get('device_authorization_endpoint'),
                 'token': doc.get('token_endpoint'),
                 'revoke': doc.get('revocation_endpoint')}
        if not found['device'] or not found['token']:
            raise ApiError('ISSUER_MISMATCH', status)
        with self._lock:
            self._endpoints = found
        return found

    # ------------------------------------------------------- Anmeldung
    def start_login(self, scopes):
        """Schritt 1–2: Schlüssel bereit, Gerätecode holen. Noch kein Token."""
        self._key_ready()
        cfg = connection_config()
        wanted = [SCOPE_CONNECT] + [s for s in scopes if s != SCOPE_CONNECT]
        wanted.append(SCOPE_OFFLINE)
        body = urllib.parse.urlencode({'client_id': cfg['client_id'],
                                       'scope': ' '.join(wanted)}).encode()
        url = self.endpoints()['device']
        status, headers, raw = _http(
            'POST', url, {'User-Agent': user_agent(),
                          'Content-Type': 'application/x-www-form-urlencoded',
                          'Accept': 'application/json'}, body, cfg)
        self._clock.learn(url, headers)
        answer = _json(raw)
        if status != 200 or not answer.get('device_code'):
            raise ApiError(answer.get('error') or 'DEVICE_LOGIN_FAILED',
                           status, _retry_after(headers))
        return DeviceLogin(answer, tuple(wanted))

    def poll_login(self, login):
        """Einmal nachfragen. 'pending', 'slow_down', 'denied', 'expired'
        oder 'ok'. Wer fragt, wartet vorher `login.interval` Sekunden."""
        if login.expired():
            return 'expired'
        cfg = connection_config()
        answer, status = self._token_request({
            'grant_type': 'urn:ietf:params:oauth:grant-type:device_code',
            'device_code': login.device_code, 'client_id': cfg['client_id']})
        error = answer.get('error')
        if status == 200:
            self._take_tokens(answer)
            return 'ok'
        if error == 'authorization_pending':
            return 'pending'
        if error == 'slow_down':
            login.interval += 5            # für immer, so will es RFC 8628
            return 'slow_down'
        if error == 'access_denied':
            return 'denied'
        if error == 'expired_token':
            return 'expired'
        raise ApiError(error or 'TOKEN_FAILED', status)

    def _token_request(self, form):
        """POST an den Token-Endpunkt mit Nachweis; Nonce-Anforderung und
        Uhrzeit-Absage je einmal wiederholen."""
        cfg = connection_config()
        url = self.endpoints()['token']
        body = urllib.parse.urlencode(form).encode()
        answer, status = {}, 0
        for attempt in range(3):
            headers = {'User-Agent': user_agent(), 'Accept': 'application/json',
                       'Content-Type': 'application/x-www-form-urlencoded',
                       'DPoP': self._proof('POST', url)}
            status, answer_headers, raw = _http('POST', url, headers, body, cfg)
            answer = _json(raw)
            self._learn(url, answer_headers)
            if attempt < 2 and _dpop().asks_for_nonce(
                    status, answer_headers, answer.get('error')):
                continue
            # Ein abgelehnter Nachweis sagt nichts über das Token — meist ist
            # es die Uhr. Die ist jetzt am `Date` der Antwort nachgestellt.
            if (attempt == 0 and status == 400 and answer.get('error') in (
                    'invalid_dpop_proof', 'invalid_request')
                    and 'refresh_token' in form):
                continue
            break
        return answer, status

    def _take_tokens(self, answer):
        # ⚠ Ein Token, das nicht an den Schlüssel gebunden ist, taugt für die
        # Schnittstelle nicht — ablehnen, nicht still benutzen.
        if (answer.get('token_type') or '').lower() != 'dpop':
            raise ApiError('DPOP_REQUIRED', 200)
        with self._lock:
            self._access = answer.get('access_token')
            self._access_until = time.time() + int(answer.get('expires_in') or 300)
            before = self.granted
            self.granted = tuple(sorted((answer.get('scope') or '').split()))
        # Welche Rechte das Basetool wirklich erteilt hat — in die Startspur,
        # damit ein Bericht fehlende Erlaubnisse erklären kann. Nur bei einer
        # Änderung: erneuert wird alle fünf Minuten.
        if self.granted != before:
            from . import errors
            errors.trail('Basetool: Rechte erteilt: %s'
                         % (' '.join(self.granted) or '(keine)'))
        if answer.get('refresh_token'):
            secret_store.save(REFRESH_SECRET, answer['refresh_token'])

    def _refresh(self):
        refresh = secret_store.load(REFRESH_SECRET)
        if not refresh:
            raise ApiError('NOT_CONNECTED')
        answer, status = self._token_request({
            'grant_type': 'refresh_token', 'refresh_token': refresh,
            'client_id': connection_config()['client_id']})
        if status == 200:
            self._take_tokens(answer)
            return
        if answer.get('error') == 'invalid_grant':
            # Verbindung vorbei (getrennt, 30 Tage unbenutzt, 90 Tage um).
            # Token weg, Schlüssel bleibt — er ist dieselbe Installation.
            self._forget_tokens()
            raise ApiError('invalid_grant', status)
        raise ApiError(answer.get('error') or 'TOKEN_FAILED', status)

    def _access_token(self):
        with self._lock:
            if self._access and time.time() < self._access_until - REFRESH_MARGIN:
                return self._access
            self._refresh()
            return self._access

    def _forget_tokens(self):
        with self._lock:
            self._access = None
            self._access_until = 0.0
        try:
            secret_store.delete(REFRESH_SECRET)
        except secret_store.SecretError:
            pass

    # ------------------------------------------------------------ Anfragen
    def request(self, method, path, body=None, query=None,
                idempotency_key=None):
        """Eine Anfrage an die Schnittstelle -> Antwort als dict.

        Nonce-Anforderung: einmal wiederholen. `UNAUTHENTICATED`: einmal erneuern
        und wiederholen, dann nicht noch einmal — keine Schleife. Eine
        wiederholte Schreibanfrage behält ihren `Idempotency-Key`."""
        cfg = connection_config()
        url = cfg['api'] + path
        if query:
            url += '?' + urllib.parse.urlencode(query)
        raw_body = None
        if body is not None:
            raw_body = json.dumps(body, ensure_ascii=False).encode('utf-8')
        nonce_retry = refreshed = False
        while True:
            token = self._access_token()
            headers = {'User-Agent': user_agent(),
                       'Accept': 'application/json, application/problem+json',
                       'Authorization': 'DPoP ' + token,
                       'DPoP': self._proof(method, url, access_token=token)}
            if raw_body is not None:
                headers['Content-Type'] = 'application/json'
            if idempotency_key:
                headers['Idempotency-Key'] = idempotency_key
            status, answer_headers, raw = _http(method, url, headers, raw_body,
                                                cfg)
            self._learn(url, answer_headers)
            answer = _json(raw)
            if 200 <= status < 300:
                return answer
            code = answer.get('code') or ('HTTP_%d' % status)
            if not nonce_retry and _dpop().asks_for_nonce(status,
                                                          answer_headers):
                nonce_retry = True
                continue
            if code == 'UNAUTHENTICATED' and not refreshed:
                refreshed = True
                with self._lock:
                    self._access = None
                continue
            error = ApiError(code, status, _retry_after(answer_headers),
                             answer)
            # Für das Fehlerprotokoll: welche Anfrage — nur der Pfad, ohne
            # Abfrageteil (dort stünde der Cursor).
            error.path = '%s %s' % (method, path)
            if error.action == 'forget_key':
                self._forget_tokens()
                self._drop_key()
            elif code == 'CLIENT_REVOKED':
                self._forget_tokens()
            raise error

    # ------------------------------------------------------------ Trennen
    def _drop_key(self):
        with self._lock:
            key, self._key = self._key, None
        try:
            if key is not None:
                key.close()
            delete_key()
        except Exception:
            pass

    def disconnect(self):
        """„Trennen": Token widerrufen (soweit es geht), Token löschen,
        Schlüssel löschen — genau in dieser Reihenfolge."""
        refresh = None
        try:
            refresh = secret_store.load(REFRESH_SECRET)
        except secret_store.SecretError:
            pass
        if refresh:
            try:
                cfg = connection_config()
                url = self.endpoints().get('revoke')
                if url:
                    body = urllib.parse.urlencode({
                        'token': refresh, 'token_type_hint': 'refresh_token',
                        'client_id': cfg['client_id']}).encode()
                    _http('POST', url, {
                        'User-Agent': user_agent(),
                        'Content-Type': 'application/x-www-form-urlencoded',
                        'DPoP': self._proof('POST', url)}, body, cfg)
            except Exception:
                pass                        # Widerruf ist nur ein Versuch
        self._forget_tokens()
        self._drop_key()
        with self._lock:
            self.granted = ()
        from . import exchange_demand
        exchange_demand.forget()

    # ------------------------------------------------------- Kurzwege
    def service_document(self):
        return self.request('GET', '')

    def label_installation(self, label):
        return self.request('POST', '/me/installation', {'label': label})

    def account_check(self, handle):
        return (self.request('POST', '/me/account-check',
                             {'handle': handle}).get('result') or 'unknown')


def new_idempotency_key():
    return str(uuid.uuid4())


def default_label():
    """`VerseKit Windows` / `VerseKit Linux` — ⚠ nie der Rechnername, und
    der Bindestrich ist der einfache, kein Gedankenstrich."""
    return 'VerseKit Windows' if secret_store.WINDOWS else 'VerseKit Linux'


LABEL_OK = __import__('re').compile(r'^[^\W][\w .-]{0,39}$')


def label_valid(label):
    """Buchstaben, Ziffern, Leerzeichen, `-`, `_`, `.`; höchstens 40 Zeichen,
    nicht mit Leerzeichen am Anfang. `\\w` schließt `_` ein und Umlaute."""
    return bool(label) and bool(LABEL_OK.match(label)) and len(label) <= 40


CONNECTION = Connection()
