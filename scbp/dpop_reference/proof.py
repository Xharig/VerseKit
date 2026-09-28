# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Lucas Greuloch
"""Builds DPoP proof JWTs (RFC 9449 §4) and keeps the server nonce between requests."""

from __future__ import annotations

import secrets
import threading
import time
from collections.abc import Mapping

from .base import DpopKey
from .jose import access_token_hash, b64url_encode, canonical_json, normalize_htu

PROOF_TYPE = "dpop+jwt"
ALGORITHM = "ES256"
NONCE_HEADER = "DPoP-Nonce"
USE_DPOP_NONCE = "use_dpop_nonce"


def build_proof(
    key: DpopKey,
    method: str,
    url: str,
    *,
    access_token: str | None = None,
    nonce: str | None = None,
    issued_at: int | None = None,
    jti: str | None = None,
) -> str:
    """Builds and signs one DPoP proof for one request.

    Args:
        key: the installation's key; only its public JWK enters the header.
        method: the HTTP method of the request.
        url: the request URL; its query and fragment are left out of ``htu``.
        access_token: the access token the request carries, which adds ``ath``; ``None`` at the
            token endpoint.
        nonce: the server's latest ``DPoP-Nonce``, or ``None`` when none is known yet.
        issued_at: ``iat`` in seconds since the epoch; defaults to now.
        jti: the proof's unique id; defaults to 144 random bits.

    Returns:
        The compact JWS.

    Raises:
        ValueError: if the method is empty, the URL is not usable as ``htu``, or the key's JWK
            carries private material.
    """
    if not method:
        raise ValueError("method is required")
    jwk = dict(key.public_jwk())
    if "d" in jwk:
        raise ValueError("the proof header must carry the public key only")
    header = {"typ": PROOF_TYPE, "alg": ALGORITHM, "jwk": jwk}
    claims: dict[str, object] = {
        "jti": jti or secrets.token_urlsafe(18),
        "htm": method.upper(),
        "htu": normalize_htu(url),
        "iat": int(time.time()) if issued_at is None else int(issued_at),
    }
    if access_token is not None:
        claims["ath"] = access_token_hash(access_token)
    if nonce:
        claims["nonce"] = nonce
    signing_input = ".".join(b64url_encode(canonical_json(part)) for part in (header, claims))
    signature = key.sign(signing_input.encode("ascii"))
    return signing_input + "." + b64url_encode(signature)


def authorization_header(access_token: str) -> str:
    """Returns the ``Authorization`` value of a DPoP-bound request."""
    return "DPoP " + access_token


def asks_for_nonce(status: int, headers: Mapping[str, str], body_error: str | None = None) -> bool:
    """Tells whether an answer is a nonce challenge that one retry with a fresh proof resolves.

    A resource server answers ``401`` with ``WWW-Authenticate: DPoP …, error="use_dpop_nonce"``; an
    authorization server answers ``400`` with the JSON ``error`` ``use_dpop_nonce``. Both carry the
    nonce in ``DPoP-Nonce``.
    """
    if _header(headers, NONCE_HEADER) is None:
        return False
    if status == 401:
        challenge = _header(headers, "WWW-Authenticate") or ""
        return f'error="{USE_DPOP_NONCE}"' in challenge
    return status == 400 and body_error == USE_DPOP_NONCE


class NonceCache:
    """Remembers the latest ``DPoP-Nonce`` of each server origin, safe to share between threads."""

    def __init__(self) -> None:
        """Creates an empty cache."""
        self._nonces: dict[str, str] = {}
        self._lock = threading.Lock()

    def get(self, url: str) -> str | None:
        """Returns the latest nonce of the URL's origin, or ``None``."""
        with self._lock:
            return self._nonces.get(_origin(url))

    def update(self, url: str, headers: Mapping[str, str]) -> None:
        """Stores the ``DPoP-Nonce`` of an answer from the URL's origin, if it carries one."""
        nonce = _header(headers, NONCE_HEADER)
        if nonce:
            with self._lock:
                self._nonces[_origin(url)] = nonce


def _origin(url: str) -> str:
    """Returns the scheme, host and port part of a normalised URL."""
    htu = normalize_htu(url)
    return htu[: htu.index("/", htu.index("//") + 2)]


def _header(headers: Mapping[str, str], name: str) -> str | None:
    """Looks a header up case-insensitively."""
    lowered = name.lower()
    for key, value in headers.items():
        if key.lower() == lowered:
            return value
    return None
