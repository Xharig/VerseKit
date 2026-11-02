// Verse-Kit — companion for Star Citizen.
// Copyright (C) 2026 Xharig
//
// SPDX-License-Identifier: GPL-3.0-only
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! The DPoP key in Windows CNG (`dpop_reference/cng.py`): persisted, not
//! exportable, in the TPM (Platform Crypto Provider) where it supports
//! P-256, otherwise in the software provider. The private key never leaves
//! the provider. Same providers, algorithm and name as the previous
//! version, so an existing connection keeps its key.

use std::sync::{Arc, OnceLock};

use windows_sys::Win32::Security::Cryptography::{
    NCRYPT_KEY_HANDLE, NCRYPT_PERSIST_FLAG, NCRYPT_PROV_HANDLE, NCRYPT_SILENT_FLAG, NCryptCreatePersistedKey,
    NCryptDeleteKey, NCryptExportKey, NCryptFinalizeKey, NCryptFreeObject, NCryptIsAlgSupported, NCryptOpenKey,
    NCryptOpenStorageProvider, NCryptSetProperty, NCryptSignHash,
};

use crate::basetool::jose::{Jwk, sha256};
use crate::basetool::key::{KeyFailure, KeyVault};
use crate::basetool::proof::{DpopKey, KeyError};

const SOFTWARE_PROVIDER: &str = "Microsoft Software Key Storage Provider";
const PLATFORM_PROVIDER: &str = "Microsoft Platform Crypto Provider";
const ALGORITHM: &str = "ECDSA_P256";
const PUBLIC_BLOB: &str = "ECCPUBLICBLOB";
const EXPORT_POLICY: &str = "Export Policy";
const PUBLIC_P256_MAGIC: u32 = 0x3153_4345;
/// `NTE_BAD_KEYSET` and `NTE_NO_KEY`: the provider does not hold the key.
const MISSING_KEY: [u32; 2] = [0x8009_0016, 0x8009_000D];

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn check(status: i32, what: &str) -> Result<(), KeyError> {
    if status == 0 { Ok(()) } else { Err(KeyError(format!("{what} failed with 0x{:08X}", status as u32))) }
}

fn free(handle: usize) {
    if handle != 0 {
        // SAFETY: the handle came from CNG and is freed only once.
        unsafe { NCryptFreeObject(handle) };
    }
}

fn open_provider(name: &str) -> Result<NCRYPT_PROV_HANDLE, KeyError> {
    let mut handle: NCRYPT_PROV_HANDLE = 0;
    let name = wide(name);
    // SAFETY: the name ends with a zero and the handle is written once.
    check(unsafe { NCryptOpenStorageProvider(&mut handle, name.as_ptr(), 0) }, "opening the key provider")?;
    Ok(handle)
}

fn platform_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        let Ok(provider) = open_provider(PLATFORM_PROVIDER) else { return false };
        let algorithm = wide(ALGORITHM);
        // SAFETY: open provider handle, zero-terminated name.
        let supported = unsafe { NCryptIsAlgSupported(provider, algorithm.as_ptr(), 0) } == 0;
        free(provider);
        supported
    })
}

/// An open CNG key; the handles are freed when it is dropped.
pub struct CngKey {
    provider: NCRYPT_PROV_HANDLE,
    key: NCRYPT_KEY_HANDLE,
    jwk: Jwk,
}

// SAFETY: CNG handles may be used from any thread.
unsafe impl Send for CngKey {}
// SAFETY: signing with one key handle from several threads is allowed.
unsafe impl Sync for CngKey {}

impl CngKey {
    fn take(provider: NCRYPT_PROV_HANDLE, key: NCRYPT_KEY_HANDLE) -> Result<CngKey, KeyError> {
        match read_public_jwk(key) {
            Ok(jwk) => Ok(CngKey { provider, key, jwk }),
            Err(e) => {
                free(key);
                free(provider);
                Err(e)
            }
        }
    }

    /// The named key, the TPM searched first; `None` when no provider holds it.
    pub fn open(name: &str) -> Result<Option<CngKey>, KeyError> {
        let key_name = wide(name);
        for candidate in [PLATFORM_PROVIDER, SOFTWARE_PROVIDER] {
            let Ok(provider) = open_provider(candidate) else { continue };
            let mut key: NCRYPT_KEY_HANDLE = 0;
            // SAFETY: open provider, zero-terminated name, handle written once.
            let status = unsafe { NCryptOpenKey(provider, &mut key, key_name.as_ptr(), 0, NCRYPT_SILENT_FLAG) };
            if status == 0 {
                return CngKey::take(provider, key).map(Some);
            }
            free(provider);
            if !MISSING_KEY.contains(&(status as u32)) {
                check(status, "opening the key")?;
            }
        }
        Ok(None)
    }

    /// A new, non-exportable key; in the TPM when possible, also when that
    /// fails the software provider. An existing key is never overwritten.
    /// The write lock guards only the key of the player's installation.
    pub fn create(name: &str) -> Result<CngKey, KeyError> {
        Self::guard_only(name)?;
        if platform_available()
            && let Ok(key) = create_in(name, PLATFORM_PROVIDER)
        {
            return Ok(key);
        }
        create_in(name, SOFTWARE_PROVIDER)
    }

    /// The write lock that [`CngKey::create`] asks before it makes a key `name`.
    pub fn guard_only(name: &str) -> Result<(), KeyError> {
        guard_players_key("cng-key", name)
    }

    /// Deletes the key `name` in its provider for good; the write lock guards only the key of the
    /// player's installation.
    pub fn delete(mut self, name: &str) -> Result<(), KeyError> {
        guard_players_key("cng-key-delete", name)?;
        // SAFETY: open key handle; CNG frees it with the deletion, on a
        // failure it stays open and `Drop` frees it.
        let status = unsafe { NCryptDeleteKey(self.key, 0) };
        check(status, "deleting the key")?;
        self.key = 0;
        Ok(())
    }
}

impl Drop for CngKey {
    fn drop(&mut self) {
        free(self.key);
        free(self.provider);
    }
}

fn create_in(name: &str, provider_name: &str) -> Result<CngKey, KeyError> {
    let provider = open_provider(provider_name)?;
    let (algorithm, key_name, policy_name) = (wide(ALGORITHM), wide(name), wide(EXPORT_POLICY));
    let mut key: NCRYPT_KEY_HANDLE = 0;
    let result = (|| {
        // SAFETY: open provider, zero-terminated texts, handle written once.
        check(
            unsafe { NCryptCreatePersistedKey(provider, &mut key, algorithm.as_ptr(), key_name.as_ptr(), 0, 0) },
            "creating the key",
        )?;
        let policy: u32 = 0;
        // SAFETY: the value is four bytes long and lives through the call.
        let status =
            unsafe { NCryptSetProperty(key, policy_name.as_ptr(), (&raw const policy).cast(), 4, NCRYPT_PERSIST_FLAG) };
        if provider_name == SOFTWARE_PROVIDER {
            check(status, "setting the export policy")?;
        }
        // SAFETY: the key handle is open.
        check(unsafe { NCryptFinalizeKey(key, NCRYPT_SILENT_FLAG) }, "finalising the key")
    })();
    match result {
        Ok(()) => CngKey::take(provider, key),
        Err(e) => {
            free(key);
            free(provider);
            Err(e)
        }
    }
}

fn read_public_jwk(key: NCRYPT_KEY_HANDLE) -> Result<Jwk, KeyError> {
    let blob_type = wide(PUBLIC_BLOB);
    let mut size: u32 = 0;
    // SAFETY: size query without an output buffer.
    check(
        unsafe {
            NCryptExportKey(
                key,
                0,
                blob_type.as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut(),
                0,
                &mut size,
                NCRYPT_SILENT_FLAG,
            )
        },
        "exporting the public key",
    )?;
    let mut blob = vec![0u8; size as usize];
    // SAFETY: the buffer holds `size` bytes.
    check(
        unsafe {
            NCryptExportKey(
                key,
                0,
                blob_type.as_ptr(),
                std::ptr::null(),
                blob.as_mut_ptr(),
                size,
                &mut size,
                NCRYPT_SILENT_FLAG,
            )
        },
        "exporting the public key",
    )?;
    blob.truncate(size as usize);
    let word = |at: usize| blob.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    match (word(0), word(4)) {
        (Some(PUBLIC_P256_MAGIC), Some(32)) if blob.len() == 8 + 64 => {
            Jwk::from_coordinates(&blob[8..40], &blob[40..72]).map_err(KeyError)
        }
        _ => Err(KeyError("the provider returned no P-256 public key".into())),
    }
}

impl DpopKey for CngKey {
    fn public_jwk(&self) -> Jwk {
        self.jwk.clone()
    }

    fn sign(&self, data: &[u8]) -> Result<Vec<u8>, KeyError> {
        let digest = sha256(data);
        let mut signature = vec![0u8; 64];
        let mut size: u32 = 0;
        // SAFETY: the digest and the 64-byte buffer live through the call.
        check(
            unsafe {
                NCryptSignHash(
                    self.key,
                    std::ptr::null(),
                    digest.as_ptr(),
                    digest.len() as u32,
                    signature.as_mut_ptr(),
                    64,
                    &mut size,
                    NCRYPT_SILENT_FLAG,
                )
            },
            "NCryptSignHash",
        )?;
        if size != 64 {
            return Err(KeyError(format!("unexpected signature length {size}")));
        }
        Ok(signature)
    }
}

/// Asks the write lock before `op` on the key of the player's installation; a key with a name of
/// its own (a test run) passes.
fn guard_players_key(op: &str, name: &str) -> Result<(), KeyError> {
    if name != crate::basetool::key::KEY_NAME {
        return Ok(());
    }
    crate::writeguard::check(op, name).map_err(|e| KeyError(e.to_string()))
}

/// The CNG key under a fixed name.
pub struct CngVault {
    pub name: String,
}

impl KeyVault for CngVault {
    fn open(&self, create: bool) -> Result<Option<Arc<dyn DpopKey>>, KeyFailure> {
        let found = CngKey::open(&self.name).map_err(KeyFailure::Key)?;
        match found {
            Some(key) => Ok(Some(Arc::new(key))),
            None if create => Ok(Some(Arc::new(CngKey::create(&self.name).map_err(KeyFailure::Key)?))),
            None => Ok(None),
        }
    }

    fn delete(&self) -> Result<(), KeyFailure> {
        match CngKey::open(&self.name).map_err(KeyFailure::Key)? {
            Some(key) => key.delete(&self.name).map_err(KeyFailure::Key),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_test_key_signs_stays_and_goes() {
        let vault = CngVault { name: format!("VerseKit Pruefung {}", crate::testdir::run_tag()) };
        let _ = vault.delete();
        assert!(vault.open(false).unwrap().is_none());
        let key = vault.open(true).unwrap().unwrap();
        let signature = key.sign(b"hallo").unwrap();
        assert!(crate::basetool::key::verify(&key.public_jwk(), b"hallo", &signature));
        // Opened again: the same key, not a new installation.
        assert_eq!(vault.open(false).unwrap().unwrap().public_jwk(), key.public_jwk());
        drop(key);
        vault.delete().unwrap();
        assert!(vault.open(false).unwrap().is_none());
    }

    #[test]
    fn under_the_write_lock_a_test_key_is_made_and_deleted_and_the_players_key_is_refused() {
        crate::writeguard::deny(true);
        crate::writeguard::allow_under(None);
        let _ = crate::writeguard::take_denied();
        let vault = CngVault { name: format!("VerseKit Pruefung Sperre {}", crate::testdir::run_tag()) };
        let made = vault.open(true);
        let deleted = vault.delete();
        let denied_for_test_key = crate::writeguard::take_denied();
        let players = CngKey::guard_only(crate::basetool::key::KEY_NAME);
        let denied_for_players = crate::writeguard::take_denied();
        crate::writeguard::deny(false);
        assert!(made.unwrap().is_some(), "a test key is made under the lock");
        deleted.expect("a test key is deleted under the lock");
        assert!(vault.open(false).unwrap().is_none());
        assert!(!denied_for_test_key.iter().any(|d| d.starts_with("cng-key")), "{denied_for_test_key:?}");
        assert!(players.is_err(), "the player's key is refused under the lock");
        let refused = format!("cng-key {}", crate::basetool::key::KEY_NAME);
        assert!(denied_for_players.contains(&refused), "{denied_for_players:?}");
    }
}
