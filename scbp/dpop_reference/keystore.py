# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Lucas Greuloch
"""Opens the installation's DPoP key: in CNG on Windows, in a 0600 file elsewhere."""

from __future__ import annotations

import os
import sys
from pathlib import Path

from .base import DpopKey, DpopKeyError


def open_installation_key(name: str, *, key_file: str | os.PathLike[str] | None = None) -> DpopKey:
    """Opens the installation's key, creating it on first use.

    On Windows the key is a persisted, non-exportable CNG key called ``name``, in the TPM where
    one is usable. Elsewhere it is an OpenSSL key in ``key_file``, created with mode 0600; a
    client shows the member that a file is used.

    Raises:
        ValueError: if ``key_file`` is missing outside Windows.
        DpopKeyError: if the key cannot be opened or created.
    """
    if sys.platform == "win32":
        from .cng import CngKey

        return CngKey.open_or_create(name)
    if key_file is None:
        raise ValueError("key_file is required outside Windows")
    from .openssl import OpenSslKey

    return OpenSslKey.load_or_create(Path(key_file).expanduser())


def delete_installation_key(name: str, *, key_file: str | os.PathLike[str] | None = None) -> None:
    """Deletes the installation's key for good, as a disconnect or ``INSTALLATION_REVOKED`` needs.

    Raises:
        DpopKeyError: if the key exists but cannot be deleted.
    """
    if sys.platform == "win32":
        from .cng import CngKey

        key = CngKey.open(name)
        if key is not None:
            key.delete()
        return
    if key_file is not None:
        try:
            Path(key_file).expanduser().unlink(missing_ok=True)
        except OSError as error:
            raise DpopKeyError(f"cannot delete {key_file}: {error.strerror}") from error
