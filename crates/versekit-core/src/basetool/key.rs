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

//! The DPoP key of this installation (`basetool.open_key`, `delete_key`).
//!
//! | System | Where the key lives |
//! |---|---|
//! | Windows | CNG, not exportable, in the TPM where one is usable; the same key the previous version created (`KEY_NAME`) |
//! | Linux, macOS | PKCS#8 as PEM in the secret store (Secret Service, otherwise a `0600` file), the same entry as the previous version |
//!
//! ⚠⚠ Keep the key. A new key is a new installation: the player gets a
//! message about a new connection, and many of them raise an alarm at the
//! Basetool. Never create one per start.

use std::sync::Arc;

use ring::rand::SystemRandom;
use ring::signature::{
    ECDSA_P256_SHA256_FIXED, ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair, UnparsedPublicKey,
};

use crate::basetool::jose::{self, Jwk};
use crate::basetool::proof::{DpopKey, KeyError};
use crate::basetool::secrets::{SecretError, SecretStore};

/// Name of the key in Windows CNG, as the previous version created it.
pub const KEY_NAME: &str = "VerseKit Basetool DPoP";
/// Name of the key in the secret store (Linux, macOS).
pub const KEY_SECRET: &str = "dpop-key";
const PEM_LABEL: &str = "PRIVATE KEY";

/// A P-256 key in memory, read from or written to PKCS#8.
pub struct SoftwareKey {
    pair: EcdsaKeyPair,
    pkcs8: Vec<u8>,
    rng: SystemRandom,
}

impl SoftwareKey {
    pub fn generate() -> Result<SoftwareKey, KeyError> {
        let rng = SystemRandom::new();
        let document = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng)
            .map_err(|_| KeyError("key generation failed".into()))?;
        SoftwareKey::from_pkcs8(document.as_ref())
    }

    /// From PKCS#8 DER; it must carry the public key (OpenSSL writes it).
    pub fn from_pkcs8(der: &[u8]) -> Result<SoftwareKey, KeyError> {
        let rng = SystemRandom::new();
        let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, der, &rng)
            .map_err(|e| KeyError(format!("no readable private key ({e})")))?;
        Ok(SoftwareKey { pair, pkcs8: der.to_vec(), rng })
    }

    pub fn from_pem(pem: &str) -> Result<SoftwareKey, KeyError> {
        let mut der = jose::pem_decode(PEM_LABEL, pem).map_err(KeyError)?;
        let key = SoftwareKey::from_pkcs8(&der);
        der.fill(0);
        key
    }

    /// The private key as PEM, only for the secret store.
    pub fn to_pem(&self) -> String {
        jose::pem_encode(PEM_LABEL, &self.pkcs8)
    }
}

impl Drop for SoftwareKey {
    fn drop(&mut self) {
        self.pkcs8.fill(0);
    }
}

impl DpopKey for SoftwareKey {
    fn public_jwk(&self) -> Jwk {
        Jwk::from_uncompressed(self.pair.public_key().as_ref()).unwrap_or(Jwk { x: String::new(), y: String::new() })
    }

    fn sign(&self, data: &[u8]) -> Result<Vec<u8>, KeyError> {
        let signature = self.pair.sign(&self.rng, data).map_err(|_| KeyError("signing failed".into()))?;
        Ok(signature.as_ref().to_vec())
    }
}

/// Checks an ES256 signature (`r || s`) against a public key.
pub fn verify(jwk: &Jwk, data: &[u8], signature: &[u8]) -> bool {
    let (Ok(x), Ok(y)) = (jose::b64url_decode(&jwk.x), jose::b64url_decode(&jwk.y)) else { return false };
    let mut point = vec![4u8];
    point.extend_from_slice(&x);
    point.extend_from_slice(&y);
    UnparsedPublicKey::new(&ECDSA_P256_SHA256_FIXED, point).verify(data, signature).is_ok()
}

/// Where the key of this installation is kept.
pub trait KeyVault: Send + Sync {
    /// The key; with `create` a new one when none exists.
    fn open(&self, create: bool) -> Result<Option<Arc<dyn DpopKey>>, KeyFailure>;
    /// Deletes the key for good (disconnect, `INSTALLATION_REVOKED`).
    fn delete(&self) -> Result<(), KeyFailure>;
}

/// A key or its store failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyFailure {
    Key(KeyError),
    Secret(SecretError),
}

impl std::fmt::Display for KeyFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyFailure::Key(e) => e.fmt(f),
            KeyFailure::Secret(e) => e.fmt(f),
        }
    }
}

/// The key in the secret store as PEM (Linux, macOS; on Windows only for
/// test runs).
pub struct StoredKey {
    pub secrets: Arc<dyn SecretStore>,
}

impl KeyVault for StoredKey {
    fn open(&self, create: bool) -> Result<Option<Arc<dyn DpopKey>>, KeyFailure> {
        if let Some(pem) = self.secrets.load(KEY_SECRET).map_err(KeyFailure::Secret)? {
            let key = SoftwareKey::from_pem(&pem);
            pem.into_bytes().fill(0);
            return Ok(Some(Arc::new(key.map_err(KeyFailure::Key)?)));
        }
        if !create {
            return Ok(None);
        }
        let key = SoftwareKey::generate().map_err(KeyFailure::Key)?;
        let pem = key.to_pem();
        let saved = self.secrets.save(KEY_SECRET, &pem);
        pem.into_bytes().fill(0);
        saved.map_err(KeyFailure::Secret)?;
        Ok(Some(Arc::new(key)))
    }

    fn delete(&self) -> Result<(), KeyFailure> {
        self.secrets.delete(KEY_SECRET).map_err(KeyFailure::Secret)
    }
}

/// Name of the Windows key: test runs give theirs (`legacy::ENV_BASETOOL_KEYNAME`)
/// and clean it up, never touching the player's.
pub fn key_name(host: &crate::paths::Host) -> String {
    host.var(crate::legacy::ENV_BASETOOL_KEYNAME)
        .map(crate::text::py_strip)
        .filter(|n| !n.is_empty())
        .unwrap_or(KEY_NAME)
        .to_string()
}

/// Whether the key has a name of its own (`legacy::ENV_BASETOOL_KEYNAME`, not [`KEY_NAME`]): it is not the key
/// of the player's installation.
pub fn own_key_name(host: &crate::paths::Host) -> bool {
    key_name(host) != KEY_NAME
}

/// The key vault of this system: CNG on Windows, the secret store elsewhere.
pub fn system_keys(secrets: Arc<dyn SecretStore>, host: &crate::paths::Host) -> Arc<dyn KeyVault> {
    #[cfg(windows)]
    {
        let _ = secrets;
        Arc::new(crate::basetool::cng::CngVault { name: key_name(host) })
    }
    #[cfg(not(windows))]
    {
        let _ = host;
        Arc::new(StoredKey { secrets })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basetool::secrets::MemorySecrets;

    #[test]
    fn signs_and_round_trips_through_pem() {
        let key = SoftwareKey::generate().unwrap();
        let signature = key.sign(b"hallo").unwrap();
        assert_eq!(signature.len(), 64);
        assert!(verify(&key.public_jwk(), b"hallo", &signature));
        assert!(!verify(&key.public_jwk(), b"hallO", &signature));
        let again = SoftwareKey::from_pem(&key.to_pem()).unwrap();
        assert_eq!(again.public_jwk(), key.public_jwk());
    }

    #[test]
    fn the_stored_key_stays_the_same() {
        let secrets = Arc::new(MemorySecrets::default());
        let vault = StoredKey { secrets: secrets.clone() };
        assert!(vault.open(false).unwrap().is_none());
        let first = vault.open(true).unwrap().unwrap().public_jwk();
        assert_eq!(vault.open(true).unwrap().unwrap().public_jwk(), first);
        vault.delete().unwrap();
        assert!(vault.open(false).unwrap().is_none());
        assert_ne!(vault.open(true).unwrap().unwrap().public_jwk(), first);
    }
}
