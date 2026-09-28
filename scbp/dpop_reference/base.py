# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Lucas Greuloch
"""The interface every DPoP key backend implements, and the error it raises."""

from __future__ import annotations

from typing import Protocol, runtime_checkable


class DpopKeyError(Exception):
    """A key could not be created, loaded, used or deleted."""


@runtime_checkable
class DpopKey(Protocol):
    """A P-256 key pair whose private half stays in its backend."""

    def public_jwk(self) -> dict[str, str]:
        """Returns the public key as a JWK with ``kty``, ``crv``, ``x`` and ``y`` only."""
        ...

    def thumbprint(self) -> str:
        """Returns the RFC 7638 thumbprint of the public key, the installation's identity."""
        ...

    def sign(self, data: bytes) -> bytes:
        """Signs ``data`` with ES256 and returns the 64-byte ``r || s`` signature."""
        ...

    def verify(self, data: bytes, signature: bytes) -> bool:
        """Checks an ES256 ``r || s`` signature over ``data`` with this key's public half."""
        ...

    def close(self) -> None:
        """Releases the backend's handles; the stored key is kept."""
        ...
