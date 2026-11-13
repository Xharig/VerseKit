// VerseKit — companion for Star Citizen.
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

//! The build provenance of a fetched update file: was it built by the release workflow of this repository, for
//! the tag of its release?
//!
//! The release workflow attests every file it attaches (`actions/attest-build-provenance`): a Sigstore bundle
//! whose certificate names the repository, the workflow file and the tag, signed with a key that existed only
//! inside that run, and entered into the public transparency log. GitHub keeps the bundle apart from the release
//! files. [`check`] asks GitHub for it by the SHA-256 of the file and verifies it here:
//!
//! 1. the statement names the file's SHA-256 and is a SLSA provenance;
//! 2. its signature (DSSE) is made with the key of the certificate;
//! 3. the certificate is signed by the Sigstore intermediate below ([`FULCIO_INTERMEDIATE`]);
//! 4. the certificate names this repository, `release.yml` and the tag of the release;
//! 5. the transparency log ([`REKOR_KEY`]) promised the entry while the certificate was valid, and the entry
//!    is this signature.
//!
//! ⚠ For now the result only goes into the logs: an update runs on whatever comes out ([`Outcome`]). Releases
//! from before the attestations have none.

mod der;
mod snappy;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use ring::{digest, signature};
use serde_json::Value;

use crate::net::{FetchError, Net};

/// Where GitHub lists the attestations of a file: `<API_BASE><owner>/<repo>/attestations/sha256:<hex>`.
pub const API_BASE: &str = "https://api.github.com/repos/";
/// The workflow that builds and attests the releases.
pub const RELEASE_WORKFLOW: &str = ".github/workflows/release.yml";
/// Who issues the tokens of GitHub Actions; the certificate names it.
pub const ACTIONS_ISSUER: &str = "https://token.actions.githubusercontent.com";
/// The most bundles fetched for one file (each is one request outside the API's hourly limit). A run made again
/// attests the same file again, so there can be several.
pub const MAX_BUNDLES: usize = 6;
/// The largest bundle, packed or unpacked, inside the answer or behind its address; real ones have about 14 KB.
pub const BUNDLE_LIMIT: usize = 1 << 20;
/// The largest answer of the API read: room for [`MAX_BUNDLES`] bundles inside it.
pub const ANSWER_LIMIT: usize = MAX_BUNDLES * BUNDLE_LIMIT;
/// The whole check gets no more time than this; after it the outcome is [`Outcome::Unreachable`].
pub const DEADLINE: std::time::Duration = std::time::Duration::from_secs(20);
/// The hosts a `bundle_url` may point to (the host itself or a name below it).
pub const BUNDLE_HOSTS: [&str; 3] = ["blob.core.windows.net", "github.com", "githubusercontent.com"];
/// The longest reason written into a log line, in characters.
pub const REASON_LIMIT: usize = 200;

/// The public key of the Sigstore intermediate `sigstore-intermediate` (P-384), from the trusted root of the
/// public Sigstore instance (valid since 2022-04-13). It signs the short-lived certificates of the builds.
pub const FULCIO_INTERMEDIATE: &str = "MHYwEAYHKoZIzj0CAQYFK4EEACIDYgAE8RVS/ysH+NOvuDZyPIZtilgUF9NlarYpAd9HP1vBBH1U5CV77LSS7s0ZiH4nE7Hv7ptS6LvvR/STk798LVgMzLlJ4HeIfF3tHSaexLcYpSASr1kS0N/RgBJz/9jWCiXn";
/// The public key of the transparency log `rekor.sigstore.dev` (P-256, valid since 2021-01-12); its SHA-256 is the
/// log's id in a bundle.
pub const REKOR_KEY: &str = "MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAE2G2Y+2tabdTV5BcGiBIx0a9fAFwrkBbmLSGtks4L3qX6yYY0zufBnhC8Ur/iy55GhWP/9A/bY2LhC30M9+RYtw==";

const DSSE_TYPE: &str = "application/vnd.in-toto+json";
const STATEMENT_TYPE: &str = "https://in-toto.io/Statement/v1";
const SLSA_PROVENANCE: &str = "https://slsa.dev/provenance/v1";
/// Fulcio's extensions, 1.3.6.1.4.1.57264.1.N; the last byte is N.
const FULCIO_OID: &[u8] = &[0x2b, 0x06, 0x01, 0x04, 0x01, 0x83, 0xbf, 0x30, 0x01];
const EXT_ISSUER: u8 = 8;
const EXT_BUILD_SIGNER: u8 = 9;
const EXT_SOURCE_REPOSITORY: u8 = 12;

/// Who must have built the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Builder {
    /// `owner/repo`.
    pub repo: String,
    /// The workflow file in the repository.
    pub workflow: String,
    /// The ref of the run, `refs/tags/v5.1.0`.
    pub git_ref: String,
}

impl Builder {
    /// The release workflow of [`crate::updater::REPO`], run for the tag `tag`.
    pub fn release(tag: &str) -> Self {
        Self {
            repo: crate::updater::REPO.to_string(),
            workflow: RELEASE_WORKFLOW.to_string(),
            git_ref: format!("refs/tags/{tag}"),
        }
    }

    /// The URI the certificate names as the signing workflow.
    fn signer(&self) -> String {
        format!("https://github.com/{}/{}@{}", self.repo, self.workflow, self.git_ref)
    }

    fn repository(&self) -> String {
        format!("https://github.com/{}", self.repo)
    }

    /// Where GitHub lists the attestations of the file with this SHA-256.
    pub fn api_url(&self, sha256: &str) -> String {
        format!("{API_BASE}{}/attestations/sha256:{}", self.repo, sha256.to_ascii_lowercase())
    }
}

/// What the check found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// A bundle passed every step.
    Verified,
    /// GitHub has no attestation of this file (every release from before the attestations).
    Missing,
    /// There are attestations, but none passed; why the first failed.
    Unverified(String),
    /// GitHub or the bundle could not be reached; what happened.
    Unreachable(String),
}

impl Outcome {
    pub fn is_verified(&self) -> bool {
        matches!(self, Self::Verified)
    }

    /// The line for the update log, in English like the installer's lines next to it.
    pub fn log_text(&self, builder: &Builder) -> String {
        match self {
            Self::Verified => format!("Build provenance verified: built by {}", builder.signer()),
            Self::Missing => "Build provenance: GitHub has no attestation for this file - update continues".into(),
            Self::Unverified(why) => format!("Build provenance NOT verified ({}) - update continues", clean(why)),
            Self::Unreachable(why) => {
                format!("Build provenance not checked, GitHub not reachable ({}) - update continues", clean(why))
            }
        }
    }
}

/// A reason from the net or a certificate made fit for a log line: no control characters, no query of an address
/// (a `bundle_url` carries a signed access token there), at most [`REASON_LIMIT`] characters.
pub fn clean(text: &str) -> String {
    let words: Vec<String> = text
        .split_whitespace()
        .map(|word| match word.split_once('?') {
            Some((address, _)) if address.contains("://") => format!("{address}?…"),
            _ => word.to_string(),
        })
        .collect();
    let joined: String = words.join(" ").chars().filter(|c| !c.is_control()).collect();
    if joined.chars().count() > REASON_LIMIT {
        format!("{}…", joined.chars().take(REASON_LIMIT).collect::<String>())
    } else {
        joined
    }
}

/// [`check`] in a thread of its own, at most [`DEADLINE`] long: a GitHub that does not answer must not hold the
/// update (and its lock) for the full time limit of every request. The thread is left to end by itself.
pub fn check_within(
    net: std::sync::Arc<dyn Net>,
    sha256: &str,
    builder: &Builder,
    deadline: std::time::Duration,
) -> Outcome {
    let (send, receive) = std::sync::mpsc::channel();
    let (sha256, builder_copy) = (sha256.to_string(), builder.clone());
    let spawned = std::thread::Builder::new().name("provenance".into()).spawn(move || {
        let _ = send.send(check(&*net, &sha256, &builder_copy));
    });
    if spawned.is_err() {
        return Outcome::Unreachable("no thread for the check".into());
    }
    receive
        .recv_timeout(deadline)
        .unwrap_or_else(|_| Outcome::Unreachable(format!("no answer within {} s", deadline.as_secs_f64().round())))
}

/// Ask GitHub for the attestations of the file with `sha256` (lower-case hex) and verify them against `builder`.
/// One request to the API (its hourly limit), then at most [`MAX_BUNDLES`] bundles from where GitHub keeps them.
pub fn check(net: &dyn Net, sha256: &str, builder: &Builder) -> Outcome {
    if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Outcome::Unverified("no SHA-256 of the file".into());
    }
    let sha256 = sha256.to_ascii_lowercase();
    let body = match net.get(&builder.api_url(&sha256)) {
        Ok(body) => body,
        Err(FetchError::Missing) => return Outcome::Missing,
        Err(error) => return Outcome::Unreachable(error.to_string()),
    };
    if body.len() > ANSWER_LIMIT {
        return Outcome::Unreachable("the answer is too large".into());
    }
    let Some(mut list) = serde_json::from_slice::<Value>(&body)
        .ok()
        .and_then(|v| v.get("attestations").and_then(Value::as_array).cloned())
    else {
        return Outcome::Unreachable("the answer is no list of attestations".into());
    };
    if list.is_empty() {
        return Outcome::Missing;
    }
    // The attestations of a workflow first: the ones GitHub makes itself for a release are no build provenance.
    list.sort_by_key(|entry| entry.get("initiator").and_then(Value::as_str) == Some("github"));
    let mut refused = None;
    let mut unreachable = None;
    for entry in list.iter().take(MAX_BUNDLES) {
        let bundle = match bundle_of(net, entry) {
            Ok(bundle) => bundle,
            Err(why) => {
                unreachable.get_or_insert(why);
                continue;
            }
        };
        match verify(&bundle, &sha256, builder) {
            Ok(()) => return Outcome::Verified,
            Err(why) => {
                refused.get_or_insert(why);
            }
        }
    }
    match (refused, unreachable) {
        (Some(why), _) => Outcome::Unverified(why),
        (None, Some(why)) => Outcome::Unreachable(why),
        (None, None) => Outcome::Missing,
    }
}

/// The bundle of one entry of the list: in the answer itself, or behind `bundle_url` (Snappy-packed).
fn bundle_of(net: &dyn Net, entry: &Value) -> Result<Value, String> {
    if let Some(bundle) = entry.get("bundle").filter(|b| b.is_object()) {
        let size = serde_json::to_vec(bundle).map_or(usize::MAX, |b| b.len());
        return if size > BUNDLE_LIMIT { Err("the bundle is too large".into()) } else { Ok(bundle.clone()) };
    }
    let url = entry.get("bundle_url").and_then(Value::as_str).unwrap_or("");
    if url.is_empty() {
        return Err("an attestation without its bundle".into());
    }
    if !bundle_host_ok(url) {
        return Err("the bundle is to come from an unknown host".into());
    }
    let packed = net.get(url).map_err(|e| format!("bundle: {e}"))?;
    if packed.len() > BUNDLE_LIMIT {
        return Err("the bundle is too large".into());
    }
    let bytes = if packed.first() == Some(&b'{') {
        packed
    } else {
        snappy::decode(&packed, BUNDLE_LIMIT).ok_or("the bundle cannot be unpacked")?
    };
    serde_json::from_slice(&bytes).map_err(|_| "the bundle is no JSON".to_string())
}

/// Is `url` an `https` address on one of the [`BUNDLE_HOSTS`] (the host itself or a name below it)?
fn bundle_host_ok(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    if authority.contains('@') {
        return false;
    }
    let host = authority.split(':').next().unwrap_or("").to_ascii_lowercase();
    BUNDLE_HOSTS.iter().any(|allowed| host == *allowed || host.ends_with(&format!(".{allowed}")))
}

fn b64(value: Option<&Value>) -> Option<Vec<u8>> {
    STANDARD.decode(value?.as_str()?).ok()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The keys a bundle is checked against, as `SubjectPublicKeyInfo`.
pub struct Anchors {
    /// The certificate authority of the builds (P-384).
    pub intermediate: Vec<u8>,
    /// The transparency log (P-256); its SHA-256 is the log's id.
    pub log: Vec<u8>,
}

impl Anchors {
    /// The public Sigstore instance: [`FULCIO_INTERMEDIATE`] and [`REKOR_KEY`].
    pub fn public() -> Self {
        Self {
            intermediate: STANDARD.decode(FULCIO_INTERMEDIATE).unwrap_or_default(),
            log: STANDARD.decode(REKOR_KEY).unwrap_or_default(),
        }
    }
}

/// Verify one bundle (Sigstore bundle 0.3 with a DSSE envelope) for the file with `sha256` and `builder`, against
/// the public Sigstore instance.
///
/// # Errors
///
/// The first step that failed, in English for the logs.
pub fn verify(bundle: &Value, sha256: &str, builder: &Builder) -> Result<(), String> {
    verify_with(bundle, sha256, builder, &Anchors::public())
}

/// [`verify`] against `anchors`.
///
/// # Errors
///
/// The first step that failed, in English for the logs.
pub fn verify_with(bundle: &Value, sha256: &str, builder: &Builder, anchors: &Anchors) -> Result<(), String> {
    let envelope = bundle.get("dsseEnvelope").ok_or("no signed statement in the bundle")?;
    if envelope.get("payloadType").and_then(Value::as_str) != Some(DSSE_TYPE) {
        return Err("the statement is of another type".into());
    }
    let payload = b64(envelope.get("payload")).ok_or("the statement is not readable")?;
    let sig_text = envelope
        .get("signatures")
        .and_then(Value::as_array)
        .and_then(|list| list.first())
        .and_then(|s| s.get("sig"))
        .and_then(Value::as_str)
        .ok_or("the statement is not signed")?;
    let sig = STANDARD.decode(sig_text).map_err(|_| "the signature is not readable")?;

    // 1. What is attested: this file, by a SLSA provenance.
    let statement: Value = serde_json::from_slice(&payload).map_err(|_| "the statement is no JSON")?;
    if statement.get("_type").and_then(Value::as_str) != Some(STATEMENT_TYPE)
        || statement.get("predicateType").and_then(Value::as_str) != Some(SLSA_PROVENANCE)
    {
        return Err("the statement is no build provenance".into());
    }
    let names_file = statement.get("subject").and_then(Value::as_array).is_some_and(|subjects| {
        subjects.iter().any(|s| {
            s.get("digest")
                .and_then(|d| d.get("sha256"))
                .and_then(Value::as_str)
                .is_some_and(|d| d.eq_ignore_ascii_case(sha256))
        })
    });
    if !names_file {
        return Err("the statement is about another file".into());
    }

    // 2. The signature over the statement, with the key of the certificate.
    let material = bundle.get("verificationMaterial").ok_or("no certificate in the bundle")?;
    let leaf_der = b64(material.get("certificate").and_then(|c| c.get("rawBytes")))
        .or_else(|| b64(material.pointer("/x509CertificateChain/certificates/0/rawBytes")))
        .ok_or("no certificate in the bundle")?;
    let leaf = der::certificate(&leaf_der).ok_or("the certificate is not readable")?;
    let (curve, point) = der::ec_public_key(leaf.public_key).ok_or("the key of the certificate is not readable")?;
    if curve != der::OID_P256 {
        return Err("the key of the certificate is of another kind".into());
    }
    signature::UnparsedPublicKey::new(&signature::ECDSA_P256_SHA256_ASN1, point)
        .verify(&pae(DSSE_TYPE, &payload), &sig)
        .map_err(|_| "the signature does not match the statement")?;

    // 3. The certificate comes from Sigstore.
    let (intermediate_curve, intermediate_point) = der::ec_public_key(&anchors.intermediate).ok_or("intermediate")?;
    if intermediate_curve != der::OID_P384 {
        return Err("intermediate".into());
    }
    if leaf.algorithm != der::OID_ECDSA_SHA384 {
        return Err("the certificate is signed in another way".into());
    }
    signature::UnparsedPublicKey::new(&signature::ECDSA_P384_SHA384_ASN1, intermediate_point)
        .verify(leaf.signed, leaf.signature)
        .map_err(|_| "the certificate is not from Sigstore")?;

    // 4. Who built it.
    let extension = |number: u8| {
        leaf.extensions
            .iter()
            .find(|(oid, _)| {
                oid.len() == FULCIO_OID.len() + 1 && oid.starts_with(FULCIO_OID) && oid.last() == Some(&number)
            })
            .and_then(|(_, value)| der::expect(value, der::UTF8_STRING))
            .and_then(|(text, _)| std::str::from_utf8(text.body).ok())
    };
    // The repository in any case (GitHub's names do not tell case apart); issuer, workflow and tag exactly.
    if extension(EXT_ISSUER) != Some(ACTIONS_ISSUER) {
        return Err("the certificate is not for GitHub Actions".into());
    }
    if !extension(EXT_SOURCE_REPOSITORY).is_some_and(|v| v.eq_ignore_ascii_case(&builder.repository())) {
        return Err(format!("built in another repository: {}", extension(EXT_SOURCE_REPOSITORY).unwrap_or("?")));
    }
    if extension(EXT_BUILD_SIGNER) != Some(builder.signer().as_str()) {
        return Err(format!("built by another workflow or tag: {}", extension(EXT_BUILD_SIGNER).unwrap_or("?")));
    }

    // 5. The transparency log: the promise for this entry, made while the certificate was valid.
    transparency(&anchors.log, material, &payload, sig_text, &leaf_der, (leaf.not_before, leaf.not_after))
}

/// The bytes a DSSE signature covers (pre-authentication encoding).
fn pae(kind: &str, payload: &[u8]) -> Vec<u8> {
    let mut out = format!("DSSEv1 {} {} {} ", kind.len(), kind, payload.len()).into_bytes();
    out.extend_from_slice(payload);
    out
}

/// Step 5: an entry of the log `rekor` (its key) whose signed promise (SET) checks out, whose time lies within the
/// validity of the certificate, and whose content is this statement and signature.
fn transparency(
    rekor: &[u8],
    material: &Value,
    payload: &[u8],
    sig_text: &str,
    leaf_der: &[u8],
    (not_before, not_after): (i64, i64),
) -> Result<(), String> {
    let rekor_id = digest::digest(&digest::SHA256, rekor);
    let (rekor_curve, rekor_point) = der::ec_public_key(rekor).ok_or("log key")?;
    if rekor_curve != der::OID_P256 {
        return Err("log key".into());
    }
    let entries = material.get("tlogEntries").and_then(Value::as_array).cloned().unwrap_or_default();
    let entry = entries
        .iter()
        .find(|e| b64(e.pointer("/logId/keyId")).is_some_and(|id| id == rekor_id.as_ref()))
        .ok_or("no entry in the known transparency log")?;
    let number =
        |key: &str| entry.get(key).and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())));
    let integrated = number("integratedTime").ok_or("the log entry has no time")?;
    let index = number("logIndex").ok_or("the log entry has no index")?;
    let body_text = entry.get("canonicalizedBody").and_then(Value::as_str).ok_or("the log entry has no content")?;
    let promise = b64(entry.pointer("/inclusionPromise/signedEntryTimestamp")).ok_or("the log entry has no promise")?;
    // The promise covers the canonical JSON of these four fields, keys in this order.
    let signed = format!(
        r#"{{"body":{},"integratedTime":{integrated},"logID":"{}","logIndex":{index}}}"#,
        Value::String(body_text.to_string()),
        hex(rekor_id.as_ref())
    );
    signature::UnparsedPublicKey::new(&signature::ECDSA_P256_SHA256_ASN1, rekor_point)
        .verify(signed.as_bytes(), &promise)
        .map_err(|_| "the promise of the transparency log does not match")?;
    if integrated < not_before || integrated > not_after {
        return Err("signed outside the validity of the certificate".into());
    }
    let body: Value = STANDARD
        .decode(body_text)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or("the content of the log entry is not readable")?;
    entry_matches(&body, payload, sig_text, leaf_der)
}

/// Is the content of a log entry (`kind` dsse) about exactly this statement, this signature and this certificate?
fn entry_matches(body: &Value, payload: &[u8], sig_text: &str, leaf_der: &[u8]) -> Result<(), String> {
    let payload_hash = hex(digest::digest(&digest::SHA256, payload).as_ref());
    if body.pointer("/spec/payloadHash/value").and_then(Value::as_str) != Some(payload_hash.as_str()) {
        return Err("the log entry is about another statement".into());
    }
    let pem = pem_of(leaf_der);
    let same_signature = body.pointer("/spec/signatures").and_then(Value::as_array).is_some_and(|list| {
        list.iter().any(|s| {
            s.get("signature").and_then(Value::as_str) == Some(sig_text)
                && b64(s.get("verifier")).is_some_and(|v| String::from_utf8_lossy(&v).trim() == pem)
        })
    });
    if !same_signature {
        return Err("the log entry is about another signature".into());
    }
    Ok(())
}

/// A certificate as PEM, lines of 64 characters, as the log keeps it (without the last line break).
fn pem_of(der: &[u8]) -> String {
    let lines: Vec<String> =
        STANDARD.encode(der).as_bytes().chunks(64).map(|c| String::from_utf8_lossy(c).into_owned()).collect();
    format!("-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----", lines.join("\n"))
}

#[cfg(test)]
mod tests;
