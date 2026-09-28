# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Lucas Greuloch
"""Persisted, non-exportable P-256 DPoP keys in Windows CNG (``ncrypt.dll``) through ``ctypes``."""

from __future__ import annotations

import ctypes
import hashlib
import struct
import sys

from .base import DpopKeyError
from .jose import P256_COORDINATE_BYTES, ec_public_jwk, jwk_thumbprint

SOFTWARE_PROVIDER = "Microsoft Software Key Storage Provider"
PLATFORM_PROVIDER = "Microsoft Platform Crypto Provider"

_ALGORITHM = "ECDSA_P256"
_PUBLIC_BLOB = "ECCPUBLICBLOB"
_PRIVATE_BLOB = "ECCPRIVATEBLOB"
_EXPORT_POLICY = "Export Policy"
_PUBLIC_P256_MAGIC = 0x31534345
_SILENT = 0x40
_PERSIST = 0x80000000
_EXPORT_FLAGS = 0x1 | 0x2
_OK = 0
_BAD_SIGNATURE = 0x80090006
_MISSING_KEY = {0x80090016, 0x8009000D}

_lib: ctypes.WinDLL | None = None
_platform_available: bool | None = None


def _ncrypt() -> ctypes.WinDLL:
    """Loads ``ncrypt.dll`` once and declares the functions used here.

    Raises:
        DpopKeyError: if this is not Windows or the library cannot be loaded.
    """
    global _lib
    if _lib is not None:
        return _lib
    if sys.platform != "win32":
        raise DpopKeyError("Windows CNG is available on Windows only")
    try:
        lib = ctypes.WinDLL("ncrypt.dll")
    except OSError as error:
        raise DpopKeyError(f"cannot load ncrypt.dll: {error}") from error
    handle, status, dword = ctypes.c_size_t, ctypes.c_long, ctypes.c_ulong
    text, buffer, out = ctypes.c_wchar_p, ctypes.c_void_p, ctypes.POINTER(dword)
    signatures = {
        "NCryptOpenStorageProvider": [ctypes.POINTER(handle), text, dword],
        "NCryptIsAlgSupported": [handle, text, dword],
        "NCryptOpenKey": [handle, ctypes.POINTER(handle), text, dword, dword],
        "NCryptCreatePersistedKey": [handle, ctypes.POINTER(handle), text, text, dword, dword],
        "NCryptSetProperty": [handle, text, buffer, dword, dword],
        "NCryptGetProperty": [handle, text, buffer, dword, ctypes.POINTER(dword), dword],
        "NCryptFinalizeKey": [handle, dword],
        "NCryptExportKey": [handle, handle, text, buffer, buffer, dword, out, dword],
        "NCryptSignHash": [handle, buffer, buffer, dword, buffer, dword, out, dword],
        "NCryptVerifySignature": [handle, buffer, buffer, dword, buffer, dword, dword],
        "NCryptDeleteKey": [handle, dword],
        "NCryptFreeObject": [handle],
    }
    for name, argtypes in signatures.items():
        function = getattr(lib, name)
        function.restype = status
        function.argtypes = argtypes
    _lib = lib
    return lib


def _code(status: int) -> int:
    """Returns a ``SECURITY_STATUS`` as an unsigned 32-bit code."""
    return status & 0xFFFFFFFF


def _check(status: int, what: str) -> None:
    """Raises when a CNG call did not succeed.

    Raises:
        DpopKeyError: naming the call and its status code.
    """
    if status != _OK:
        raise DpopKeyError(f"{what} failed with 0x{_code(status):08X}")


def _open_provider(provider: str) -> int:
    """Opens a key storage provider and returns its handle.

    Raises:
        DpopKeyError: if the provider is not present.
    """
    handle = ctypes.c_size_t(0)
    _check(_ncrypt().NCryptOpenStorageProvider(ctypes.byref(handle), provider, 0),
           f"opening {provider}")
    return handle.value


def _free(handle: int | None) -> None:
    """Frees a CNG handle, ignoring an empty one."""
    if handle:
        _ncrypt().NCryptFreeObject(handle)


class CngKey:
    """A persisted P-256 key in a CNG key storage provider; the private key never leaves it."""

    def __init__(self, provider_handle: int, key_handle: int, name: str, provider: str) -> None:
        """Takes ownership of open provider and key handles; use the class methods instead.

        Raises:
            DpopKeyError: if the public key cannot be read; both handles are then freed.
        """
        self._provider_handle: int | None = provider_handle
        self._key_handle: int | None = key_handle
        self.name = name
        self.provider = provider
        try:
            self._jwk = self._read_public_jwk()
        except DpopKeyError:
            self.close()
            raise

    @staticmethod
    def is_available() -> bool:
        """Tells whether Windows CNG can be used on this machine."""
        try:
            _ncrypt()
        except DpopKeyError:
            return False
        return True

    @staticmethod
    def platform_provider_available() -> bool:
        """Tells whether the TPM-backed Platform Crypto Provider is present and supports P-256."""
        global _platform_available
        if _platform_available is None:
            try:
                provider = _open_provider(PLATFORM_PROVIDER)
            except DpopKeyError:
                _platform_available = False
            else:
                try:
                    _platform_available = (
                        _ncrypt().NCryptIsAlgSupported(provider, _ALGORITHM, 0) == _OK
                    )
                finally:
                    _free(provider)
        return _platform_available

    @classmethod
    def open(cls, name: str, provider: str | None = None) -> CngKey | None:
        """Opens an existing key by name, or returns ``None`` when no provider holds it.

        Without ``provider`` the Platform Crypto Provider is tried first, then the software one.

        Raises:
            DpopKeyError: if a provider fails for another reason than a missing key.
        """
        for candidate in cls._providers(provider):
            try:
                provider_handle = _open_provider(candidate)
            except DpopKeyError:
                if provider is not None:
                    raise
                continue
            key_handle = ctypes.c_size_t(0)
            status = _ncrypt().NCryptOpenKey(provider_handle, ctypes.byref(key_handle), name, 0,
                                             _SILENT)
            if status == _OK:
                return cls(provider_handle, key_handle.value, name, candidate)
            _free(provider_handle)
            if _code(status) not in _MISSING_KEY:
                _check(status, f"opening the key {name!r}")
        return None

    @classmethod
    def create(cls, name: str, provider: str | None = None) -> CngKey:
        """Creates and persists a new non-exportable key.

        Without ``provider`` the TPM is used when available and the software provider otherwise,
        also when creating the key in the TPM fails. An existing key of the same name is never
        overwritten.

        Raises:
            DpopKeyError: if the key cannot be created, for example because the name is taken.
        """
        if provider is not None:
            return cls._create_in(name, provider)
        if cls.platform_provider_available():
            try:
                return cls._create_in(name, PLATFORM_PROVIDER)
            except DpopKeyError:
                pass
        return cls._create_in(name, SOFTWARE_PROVIDER)

    @classmethod
    def open_or_create(cls, name: str, provider: str | None = None) -> CngKey:
        """Opens the named key, or creates it when no provider holds it.

        Raises:
            DpopKeyError: if the key can be neither opened nor created.
        """
        key = cls.open(name, provider)
        return key if key is not None else cls.create(name, provider)

    def public_jwk(self) -> dict[str, str]:
        """Returns the public key as a JWK."""
        return dict(self._jwk)

    def thumbprint(self) -> str:
        """Returns the RFC 7638 thumbprint of the public key."""
        return jwk_thumbprint(self._jwk)

    def sign(self, data: bytes) -> bytes:
        """Signs the SHA-256 of ``data`` in the provider and returns ``r || s``.

        Raises:
            DpopKeyError: if the provider refuses to sign.
        """
        digest = hashlib.sha256(data).digest()
        lib = _ncrypt()
        size = ctypes.c_ulong(0)
        _check(lib.NCryptSignHash(self._handle(), None, digest, len(digest), None, 0,
                                  ctypes.byref(size), _SILENT), "NCryptSignHash")
        signature = ctypes.create_string_buffer(size.value)
        _check(lib.NCryptSignHash(self._handle(), None, digest, len(digest), signature, size.value,
                                  ctypes.byref(size), _SILENT), "NCryptSignHash")
        if size.value != 2 * P256_COORDINATE_BYTES:
            raise DpopKeyError(f"unexpected signature length {size.value}")
        return signature.raw[: size.value]

    def verify(self, data: bytes, signature: bytes) -> bool:
        """Checks an ``r || s`` ES256 signature over ``data`` in the provider.

        Raises:
            DpopKeyError: if the provider fails for another reason than a bad signature.
        """
        if len(signature) != 2 * P256_COORDINATE_BYTES:
            return False
        digest = hashlib.sha256(data).digest()
        status = _ncrypt().NCryptVerifySignature(self._handle(), None, digest, len(digest),
                                                 signature, len(signature), _SILENT)
        if _code(status) == _BAD_SIGNATURE:
            return False
        _check(status, "NCryptVerifySignature")
        return True

    def export_policy(self) -> int | None:
        """Returns the key's export policy flags, or ``None`` when the provider does not say."""
        value = ctypes.c_ulong(0)
        size = ctypes.c_ulong(0)
        status = _ncrypt().NCryptGetProperty(self._handle(), _EXPORT_POLICY, ctypes.byref(value),
                                             ctypes.sizeof(value), ctypes.byref(size), _SILENT)
        return value.value if status == _OK else None

    def delete(self) -> None:
        """Deletes the key from its provider for good and releases the handles.

        Raises:
            DpopKeyError: if the provider refuses the deletion.
        """
        status = _ncrypt().NCryptDeleteKey(self._handle(), 0)
        _check(status, f"deleting the key {self.name!r}")
        self._key_handle = None
        self.close()

    def close(self) -> None:
        """Releases the handles; the persisted key stays."""
        _free(self._key_handle)
        _free(self._provider_handle)
        self._key_handle = None
        self._provider_handle = None

    def __enter__(self) -> CngKey:
        """Returns the key for a ``with`` block."""
        return self

    def __exit__(self, *exc_info: object) -> None:
        """Releases the handles at the end of a ``with`` block."""
        self.close()

    @staticmethod
    def _providers(provider: str | None) -> list[str]:
        """Lists the providers to search, the TPM first."""
        return [provider] if provider is not None else [PLATFORM_PROVIDER, SOFTWARE_PROVIDER]

    @classmethod
    def _create_in(cls, name: str, provider: str) -> CngKey:
        """Creates, restricts and finalises a key in one provider."""
        lib = _ncrypt()
        provider_handle = _open_provider(provider)
        key_handle = ctypes.c_size_t(0)
        try:
            _check(lib.NCryptCreatePersistedKey(provider_handle, ctypes.byref(key_handle),
                                                _ALGORITHM, name, 0, 0),
                   f"creating the key {name!r}")
            policy = ctypes.c_ulong(0)
            status = lib.NCryptSetProperty(key_handle.value, _EXPORT_POLICY, ctypes.byref(policy),
                                           ctypes.sizeof(policy), _PERSIST)
            if provider == SOFTWARE_PROVIDER:
                _check(status, "setting the export policy")
            _check(lib.NCryptFinalizeKey(key_handle.value, _SILENT), f"finalising the key {name!r}")
        except DpopKeyError:
            _free(key_handle.value)
            _free(provider_handle)
            raise
        return cls(provider_handle, key_handle.value, name, provider)

    def _handle(self) -> int:
        """Returns the key handle, refusing a closed key."""
        if not self._key_handle:
            raise DpopKeyError("the key is closed")
        return self._key_handle

    def _exports_private_key(self) -> bool:
        """Tells whether the provider hands out the private key, which it must refuse."""
        lib = _ncrypt()
        size = ctypes.c_ulong(0)
        status = lib.NCryptExportKey(self._handle(), 0, _PRIVATE_BLOB, None, None, 0,
                                     ctypes.byref(size), _SILENT)
        if status != _OK or size.value == 0:
            return False
        blob = ctypes.create_string_buffer(size.value)
        status = lib.NCryptExportKey(self._handle(), 0, _PRIVATE_BLOB, None, blob, size.value,
                                     ctypes.byref(size), _SILENT)
        ctypes.memset(blob, 0, len(blob))
        return status == _OK

    def _read_public_jwk(self) -> dict[str, str]:
        """Exports the public blob and turns it into a JWK."""
        lib = _ncrypt()
        size = ctypes.c_ulong(0)
        _check(lib.NCryptExportKey(self._handle(), 0, _PUBLIC_BLOB, None, None, 0,
                                   ctypes.byref(size), _SILENT), "exporting the public key")
        blob = ctypes.create_string_buffer(size.value)
        _check(lib.NCryptExportKey(self._handle(), 0, _PUBLIC_BLOB, None, blob, size.value,
                                   ctypes.byref(size), _SILENT), "exporting the public key")
        data = blob.raw[: size.value]
        magic, length = struct.unpack_from("<II", data)
        if (magic != _PUBLIC_P256_MAGIC or length != P256_COORDINATE_BYTES
                or len(data) != 8 + 2 * length):
            raise DpopKeyError("the provider returned no P-256 public key")
        return ec_public_jwk(data[8:8 + length], data[8 + length:])
