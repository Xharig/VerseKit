# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Lucas Greuloch
"""P-256 DPoP keys through OpenSSL 3's libcrypto via ``ctypes``, stored in a 0600 PEM file."""

from __future__ import annotations

import ctypes
import ctypes.util
import glob
import os
import stat
import sys
from pathlib import Path

from .base import DpopKeyError
from .jose import P256_COORDINATE_BYTES, der_to_raw, ec_public_jwk, jwk_thumbprint, raw_to_der

LIBCRYPTO_ENV = "DPOP_REFERENCE_LIBCRYPTO"

_OPENSSL_3 = 0x30000000
_EVP_PKEY_EC = 408
_BIO_CTRL_INFO = 3
_P256_GROUPS = {b"prime256v1", b"P-256", b"secp256r1"}

_lib: ctypes.CDLL | None = None
_load_error: str | None = None


def _candidates() -> list[str]:
    """Lists the library names or paths to try, the environment override first."""
    override = os.environ.get(LIBCRYPTO_ENV)
    if override:
        return [override]
    if sys.platform == "darwin":
        return []
    if sys.platform == "win32":
        found: list[str] = []
        try:
            import _hashlib

            found += glob.glob(os.path.join(os.path.dirname(_hashlib.__file__), "libcrypto-3*.dll"))
        except (ImportError, AttributeError):
            pass
        for prefix in (sys.base_prefix, sys.prefix):
            found += glob.glob(os.path.join(prefix, "DLLs", "libcrypto-3*.dll"))
        return found
    names = ["libcrypto.so.3"]
    located = ctypes.util.find_library("crypto")
    if located:
        names.append(located)
    return names


def _bind(lib: ctypes.CDLL) -> None:
    """Declares the argument and result types of every libcrypto function used here."""
    p, i, sz, ul = ctypes.c_void_p, ctypes.c_int, ctypes.c_size_t, ctypes.c_ulong
    signatures = {
        "OpenSSL_version_num": (ul, []),
        "ERR_get_error": (ul, []),
        "ERR_error_string_n": (None, [ul, ctypes.c_char_p, sz]),
        "EVP_PKEY_CTX_new_from_name": (p, [p, ctypes.c_char_p, ctypes.c_char_p]),
        "EVP_PKEY_CTX_free": (None, [p]),
        "EVP_PKEY_keygen_init": (i, [p]),
        "EVP_PKEY_CTX_set_group_name": (i, [p, ctypes.c_char_p]),
        "EVP_PKEY_generate": (i, [p, ctypes.POINTER(p)]),
        "EVP_PKEY_free": (None, [p]),
        "EVP_PKEY_get_base_id": (i, [p]),
        "EVP_PKEY_get_octet_string_param": (i, [p, ctypes.c_char_p, p, sz, ctypes.POINTER(sz)]),
        "EVP_PKEY_get_utf8_string_param": (i, [p, ctypes.c_char_p, p, sz, ctypes.POINTER(sz)]),
        "EVP_MD_CTX_new": (p, []),
        "EVP_MD_CTX_free": (None, [p]),
        "EVP_sha256": (p, []),
        "EVP_DigestSignInit": (i, [p, p, p, p, p]),
        "EVP_DigestSign": (i, [p, p, ctypes.POINTER(sz), p, sz]),
        "EVP_DigestVerifyInit": (i, [p, p, p, p, p]),
        "EVP_DigestVerify": (i, [p, p, sz, p, sz]),
        "BIO_s_mem": (p, []),
        "BIO_new": (p, [p]),
        "BIO_new_mem_buf": (p, [p, i]),
        "BIO_free": (i, [p]),
        "BIO_ctrl": (ctypes.c_long, [p, i, ctypes.c_long, p]),
        "PEM_write_bio_PrivateKey": (i, [p, p, p, p, i, p, p]),
        "PEM_read_bio_PrivateKey": (p, [p, p, p, p]),
    }
    for name, (restype, argtypes) in signatures.items():
        function = getattr(lib, name)
        function.restype = restype
        function.argtypes = argtypes


def libcrypto() -> ctypes.CDLL:
    """Loads OpenSSL 3's libcrypto once and returns it.

    The library is taken from ``DPOP_REFERENCE_LIBCRYPTO`` when set; otherwise ``libcrypto.so.3``
    on Linux and Python's own ``libcrypto-3`` DLL on Windows. On macOS only the override is used,
    because the system's ``libcrypto.dylib`` stub aborts the process that loads it.

    Raises:
        DpopKeyError: if no OpenSSL 3 libcrypto can be loaded.
    """
    global _lib, _load_error
    if _lib is not None:
        return _lib
    if _load_error is not None:
        raise DpopKeyError(_load_error)
    problems = []
    for candidate in _candidates():
        try:
            lib = ctypes.CDLL(candidate)
            _bind(lib)
        except (OSError, AttributeError) as error:
            problems.append(f"{candidate}: {error}")
            continue
        if lib.OpenSSL_version_num() < _OPENSSL_3:
            problems.append(f"{candidate}: older than OpenSSL 3")
            continue
        _lib = lib
        return lib
    _load_error = "no OpenSSL 3 libcrypto found" + (f" ({'; '.join(problems)})" if problems else "")
    raise DpopKeyError(_load_error)


def _fail(what: str) -> DpopKeyError:
    """Builds an error naming the failed step and OpenSSL's first queued reason."""
    lib = libcrypto()
    code = lib.ERR_get_error()
    reason = ""
    if code:
        buffer = ctypes.create_string_buffer(256)
        lib.ERR_error_string_n(code, buffer, len(buffer))
        reason = ": " + buffer.value.decode("ascii", "replace")
    while lib.ERR_get_error():
        pass
    return DpopKeyError(what + reason)


class OpenSslKey:
    """A P-256 key held by libcrypto; its private half leaves it only into a 0600 PEM file."""

    def __init__(self, pkey: int) -> None:
        """Takes ownership of an ``EVP_PKEY`` pointer; use the class methods instead.

        Raises:
            DpopKeyError: if the key is not a P-256 key; the pointer is then freed.
        """
        self._pkey: int | None = pkey
        try:
            if not self._is_p256():
                raise DpopKeyError("not a P-256 key")
            self._jwk = self._read_public_jwk()
        except DpopKeyError:
            self.close()
            raise

    @staticmethod
    def is_available() -> bool:
        """Tells whether an OpenSSL 3 libcrypto can be loaded on this machine."""
        try:
            libcrypto()
        except DpopKeyError:
            return False
        return True

    @classmethod
    def generate(cls) -> OpenSslKey:
        """Generates a new key in memory.

        Raises:
            DpopKeyError: if libcrypto is unavailable or generation fails.
        """
        lib = libcrypto()
        ctx = lib.EVP_PKEY_CTX_new_from_name(None, b"EC", None)
        if not ctx:
            raise _fail("EVP_PKEY_CTX_new_from_name failed")
        try:
            pkey = ctypes.c_void_p()
            if (
                lib.EVP_PKEY_keygen_init(ctx) <= 0
                or lib.EVP_PKEY_CTX_set_group_name(ctx, b"P-256") <= 0
                or lib.EVP_PKEY_generate(ctx, ctypes.byref(pkey)) <= 0
                or not pkey.value
            ):
                raise _fail("P-256 key generation failed")
        finally:
            lib.EVP_PKEY_CTX_free(ctx)
        return cls(pkey.value)

    @classmethod
    def load(cls, path: str | os.PathLike[str]) -> OpenSslKey:
        """Loads a key from a PEM file readable by its owner alone.

        Raises:
            DpopKeyError: if the file is readable by others, is not a P-256 key or cannot be read.
        """
        path = Path(path)
        try:
            mode = path.stat().st_mode
            if os.name == "posix" and mode & (stat.S_IRWXG | stat.S_IRWXO):
                raise DpopKeyError(f"{path} is accessible by other users; expected mode 0600")
            pem = path.read_bytes()
        except OSError as error:
            raise DpopKeyError(f"cannot read {path}: {error.strerror}") from error
        lib = libcrypto()
        buffer = ctypes.create_string_buffer(pem, len(pem))
        bio = lib.BIO_new_mem_buf(buffer, len(pem))
        if not bio:
            raise _fail("BIO_new_mem_buf failed")
        try:
            pkey = lib.PEM_read_bio_PrivateKey(bio, None, None, None)
        finally:
            lib.BIO_free(bio)
            ctypes.memset(buffer, 0, len(pem))
        if not pkey:
            raise _fail(f"{path} holds no readable private key")
        try:
            return cls(pkey)
        except DpopKeyError as error:
            raise DpopKeyError(f"{path}: {error}") from error

    @classmethod
    def load_or_create(cls, path: str | os.PathLike[str]) -> OpenSslKey:
        """Loads the key at ``path``, or generates one and saves it there with mode 0600.

        Raises:
            DpopKeyError: if the key cannot be loaded, generated or saved.
        """
        path = Path(path)
        if path.exists():
            return cls.load(path)
        key = cls.generate()
        try:
            key.save(path)
        except DpopKeyError:
            key.close()
            raise
        return key

    def save(self, path: str | os.PathLike[str]) -> None:
        """Writes the private key as an unencrypted PKCS #8 PEM file with mode 0600.

        The file is created exclusively, so an existing key is never overwritten; its directory
        is created with mode 0700 when missing.

        Raises:
            DpopKeyError: if the file exists or cannot be written.
        """
        lib = libcrypto()
        bio = lib.BIO_new(lib.BIO_s_mem())
        if not bio:
            raise _fail("BIO_new failed")
        try:
            if lib.PEM_write_bio_PrivateKey(bio, self._handle(), None, None, 0, None, None) <= 0:
                raise _fail("PEM_write_bio_PrivateKey failed")
            data = ctypes.c_void_p()
            length = lib.BIO_ctrl(bio, _BIO_CTRL_INFO, 0, ctypes.byref(data))
            pem = ctypes.string_at(data.value, length)
        finally:
            lib.BIO_free(bio)
        path = Path(path)
        try:
            path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
            flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_BINARY", 0)
            descriptor = os.open(path, flags, 0o600)
        except OSError as error:
            raise DpopKeyError(f"cannot create {path}: {error.strerror}") from error
        with os.fdopen(descriptor, "wb") as file:
            file.write(pem)
            file.flush()
            os.fsync(file.fileno())

    def public_jwk(self) -> dict[str, str]:
        """Returns the public key as a JWK."""
        return dict(self._jwk)

    def thumbprint(self) -> str:
        """Returns the RFC 7638 thumbprint of the public key."""
        return jwk_thumbprint(self._jwk)

    def sign(self, data: bytes) -> bytes:
        """Signs ``data`` with ECDSA over SHA-256 and returns ``r || s``.

        Raises:
            DpopKeyError: if signing fails.
        """
        lib = libcrypto()
        ctx = lib.EVP_MD_CTX_new()
        if not ctx:
            raise _fail("EVP_MD_CTX_new failed")
        try:
            if lib.EVP_DigestSignInit(ctx, None, lib.EVP_sha256(), None, self._handle()) <= 0:
                raise _fail("EVP_DigestSignInit failed")
            length = ctypes.c_size_t(0)
            if lib.EVP_DigestSign(ctx, None, ctypes.byref(length), data, len(data)) <= 0:
                raise _fail("EVP_DigestSign failed")
            signature = ctypes.create_string_buffer(length.value)
            if lib.EVP_DigestSign(ctx, signature, ctypes.byref(length), data, len(data)) <= 0:
                raise _fail("EVP_DigestSign failed")
        finally:
            lib.EVP_MD_CTX_free(ctx)
        return der_to_raw(signature.raw[: length.value])

    def verify(self, data: bytes, signature: bytes) -> bool:
        """Checks an ``r || s`` ES256 signature over ``data``.

        Raises:
            DpopKeyError: if libcrypto fails for another reason than a bad signature.
        """
        if len(signature) != 2 * P256_COORDINATE_BYTES:
            return False
        der = raw_to_der(signature)
        lib = libcrypto()
        ctx = lib.EVP_MD_CTX_new()
        if not ctx:
            raise _fail("EVP_MD_CTX_new failed")
        try:
            if lib.EVP_DigestVerifyInit(ctx, None, lib.EVP_sha256(), None, self._handle()) <= 0:
                raise _fail("EVP_DigestVerifyInit failed")
            result = lib.EVP_DigestVerify(ctx, der, len(der), data, len(data))
        finally:
            lib.EVP_MD_CTX_free(ctx)
        while lib.ERR_get_error():
            pass
        return result == 1

    def close(self) -> None:
        """Frees the key in libcrypto; a saved file stays."""
        if self._pkey is not None:
            libcrypto().EVP_PKEY_free(self._pkey)
            self._pkey = None

    def __enter__(self) -> OpenSslKey:
        """Returns the key for a ``with`` block."""
        return self

    def __exit__(self, *exc_info: object) -> None:
        """Frees the key at the end of a ``with`` block."""
        self.close()

    def _handle(self) -> int:
        """Returns the ``EVP_PKEY`` pointer, refusing a closed key."""
        if self._pkey is None:
            raise DpopKeyError("the key is closed")
        return self._pkey

    def _is_p256(self) -> bool:
        """Tells whether the key is an EC key on P-256."""
        lib = libcrypto()
        if lib.EVP_PKEY_get_base_id(self._handle()) != _EVP_PKEY_EC:
            return False
        buffer = ctypes.create_string_buffer(64)
        length = ctypes.c_size_t(0)
        if lib.EVP_PKEY_get_utf8_string_param(
            self._handle(), b"group", buffer, len(buffer), ctypes.byref(length)
        ) <= 0:
            return False
        return buffer.value in _P256_GROUPS

    def _read_public_jwk(self) -> dict[str, str]:
        """Reads the uncompressed public point and turns it into a JWK."""
        lib = libcrypto()
        buffer = ctypes.create_string_buffer(1 + 2 * P256_COORDINATE_BYTES + 32)
        length = ctypes.c_size_t(0)
        if lib.EVP_PKEY_get_octet_string_param(
            self._handle(), b"encoded-pub-key", buffer, len(buffer), ctypes.byref(length)
        ) <= 0:
            raise _fail("cannot read the public key")
        point = buffer.raw[: length.value]
        if len(point) != 1 + 2 * P256_COORDINATE_BYTES or point[0] != 0x04:
            raise DpopKeyError("not an uncompressed P-256 point")
        return ec_public_jwk(point[1:33], point[33:65])
