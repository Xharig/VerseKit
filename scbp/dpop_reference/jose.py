# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Lucas Greuloch
"""Encoding helpers for DPoP proofs: base64url, JWK thumbprints, ``ath``, ``htu``, ECDSA forms."""

from __future__ import annotations

import base64
import binascii
import hashlib
import json
import urllib.parse
from collections.abc import Mapping

P256_COORDINATE_BYTES = 32

_THUMBPRINT_MEMBERS = {
    "EC": ("crv", "kty", "x", "y"),
    "RSA": ("e", "kty", "n"),
    "OKP": ("crv", "kty", "x"),
}

_DEFAULT_PORTS = {"https": 443, "http": 80}


def b64url_encode(data: bytes) -> str:
    """Encodes bytes as base64url without padding (RFC 7515 §2)."""
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode("ascii")


def b64url_decode(text: str) -> bytes:
    """Decodes base64url with or without padding.

    Raises:
        ValueError: if the text is not base64url.
    """
    body = text.rstrip("=")
    if any(c in body for c in "+/=") or len(body) % 4 == 1:
        raise ValueError("not base64url")
    standard = body.replace("-", "+").replace("_", "/")
    try:
        return base64.b64decode(standard + "=" * (-len(standard) % 4), validate=True)
    except binascii.Error as error:
        raise ValueError("not base64url") from error


def canonical_json(value: Mapping[str, object]) -> bytes:
    """Serialises a JSON object compactly, as the proof header and claims are sent."""
    return json.dumps(value, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def jwk_thumbprint(jwk: Mapping[str, str]) -> str:
    """Computes the RFC 7638 SHA-256 thumbprint of a public JWK.

    Only the required members of the key type count, in lexicographic order; any other member,
    such as ``kid``, is ignored.

    Raises:
        ValueError: if the key type is unknown or a required member is missing.
    """
    kty = jwk.get("kty")
    members = _THUMBPRINT_MEMBERS.get(kty) if isinstance(kty, str) else None
    if members is None:
        raise ValueError(f"unsupported kty: {kty!r}")
    try:
        required = {name: jwk[name] for name in members}
    except KeyError as error:
        raise ValueError(f"JWK lacks the required member {error.args[0]!r}") from error
    digest = hashlib.sha256(
        json.dumps(required, separators=(",", ":"), sort_keys=True).encode("utf-8")
    ).digest()
    return b64url_encode(digest)


def access_token_hash(access_token: str) -> str:
    """Computes ``ath``: base64url of the SHA-256 of the ASCII access token (RFC 9449 §4.2)."""
    return b64url_encode(hashlib.sha256(access_token.encode("ascii")).digest())


def normalize_htu(url: str) -> str:
    """Returns the ``htu`` of a request URL: scheme, host, non-default port and path only.

    The scheme and host are lower-cased, a default port is dropped, an empty path becomes ``/``,
    and the query and fragment are removed. The path is kept exactly as it is sent, because the
    gateway compares ``htu`` with the request's own path as a plain string.

    Raises:
        ValueError: if the URL is not an absolute ``http`` or ``https`` URL or carries user info.
    """
    parts = urllib.parse.urlsplit(url)
    scheme = parts.scheme.lower()
    if scheme not in _DEFAULT_PORTS:
        raise ValueError("htu must be an http or https URL")
    if parts.username is not None or parts.password is not None:
        raise ValueError("htu must not carry user info")
    host = parts.hostname
    if not host:
        raise ValueError("htu must name a host")
    netloc = f"[{host}]" if ":" in host else host
    port = parts.port
    if port is not None and port != _DEFAULT_PORTS[scheme]:
        netloc = f"{netloc}:{port}"
    return f"{scheme}://{netloc}{parts.path or '/'}"


def ec_public_jwk(x: bytes, y: bytes) -> dict[str, str]:
    """Builds the public JWK of a P-256 key from its affine coordinates.

    Raises:
        ValueError: if a coordinate is not 32 bytes long.
    """
    if len(x) != P256_COORDINATE_BYTES or len(y) != P256_COORDINATE_BYTES:
        raise ValueError("P-256 coordinates are 32 bytes each")
    return {"kty": "EC", "crv": "P-256", "x": b64url_encode(x), "y": b64url_encode(y)}


def der_to_raw(der: bytes, size: int = P256_COORDINATE_BYTES) -> bytes:
    """Converts a DER ``ECDSA-Sig-Value`` into the JWS form ``r || s`` (RFC 7518 §3.4).

    Raises:
        ValueError: if the DER is malformed or an integer does not fit ``size`` bytes.
    """
    if len(der) < 8 or der[0] != 0x30:
        raise ValueError("not a DER sequence")
    length, offset = _der_length(der, 1)
    if offset + length != len(der):
        raise ValueError("DER sequence length mismatch")
    r, offset = _der_integer(der, offset)
    s, offset = _der_integer(der, offset)
    if offset != len(der):
        raise ValueError("trailing bytes after the DER signature")
    return _fixed(r, size) + _fixed(s, size)


def raw_to_der(raw: bytes) -> bytes:
    """Converts a JWS ``r || s`` signature into a DER ``ECDSA-Sig-Value``.

    Raises:
        ValueError: if the signature length is odd or zero.
    """
    if not raw or len(raw) % 2:
        raise ValueError("a raw ECDSA signature has two equal halves")
    half = len(raw) // 2
    body = _der_int(raw[:half]) + _der_int(raw[half:])
    return b"\x30" + _der_len(len(body)) + body


def _der_length(der: bytes, offset: int) -> tuple[int, int]:
    """Reads a DER length at ``offset`` and returns it with the offset after it."""
    if offset >= len(der):
        raise ValueError("truncated DER")
    first = der[offset]
    offset += 1
    if first < 0x80:
        return first, offset
    count = first & 0x7F
    if count == 0 or count > 2 or offset + count > len(der):
        raise ValueError("unsupported DER length")
    return int.from_bytes(der[offset:offset + count], "big"), offset + count


def _der_integer(der: bytes, offset: int) -> tuple[bytes, int]:
    """Reads a positive DER INTEGER at ``offset`` and returns its magnitude and the next offset."""
    if offset >= len(der) or der[offset] != 0x02:
        raise ValueError("expected a DER integer")
    length, offset = _der_length(der, offset + 1)
    if length == 0 or offset + length > len(der):
        raise ValueError("truncated DER integer")
    value = der[offset:offset + length]
    if value[0] & 0x80:
        raise ValueError("negative DER integer")
    return value.lstrip(b"\x00"), offset + length


def _fixed(value: bytes, size: int) -> bytes:
    """Left-pads an unsigned big-endian integer to ``size`` bytes."""
    if len(value) > size:
        raise ValueError("integer too large for the curve")
    return value.rjust(size, b"\x00")


def _der_int(magnitude: bytes) -> bytes:
    """Encodes an unsigned big-endian integer as a DER INTEGER."""
    value = magnitude.lstrip(b"\x00") or b"\x00"
    if value[0] & 0x80:
        value = b"\x00" + value
    return b"\x02" + _der_len(len(value)) + value


def _der_len(length: int) -> bytes:
    """Encodes a DER length."""
    if length < 0x80:
        return bytes([length])
    encoded = length.to_bytes((length.bit_length() + 7) // 8, "big")
    return bytes([0x80 | len(encoded)]) + encoded
