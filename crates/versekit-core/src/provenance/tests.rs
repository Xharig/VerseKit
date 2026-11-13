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

use super::*;
use crate::net::{Fake, FetchError};

/// A real bundle of a public project, as GitHub delivers it behind `bundle_url` (see the `.HERKUNFT.txt` next to it).
const PACKED: &[u8] = include_bytes!("../../tests/data/herkunft-buendel.json.sn");
/// The SHA-256 of one of the files the bundle attests.
const FILE: &str = "5b3b310d80e6bdb144d256b4c3794f01eac0cf3f6df44e893e5c85573f1138e5";
const BUNDLE_URL: &str = "https://tmaproduction.blob.core.windows.net/attestations/212613049/60065904.json.sn?sig=x";

fn builder() -> Builder {
    Builder {
        repo: "cli/cli".into(),
        workflow: ".github/workflows/deployment.yml".into(),
        git_ref: "refs/heads/trunk".into(),
    }
}

fn bundle() -> Value {
    let bytes = snappy::decode(PACKED, BUNDLE_LIMIT).expect("the stored bundle unpacks");
    serde_json::from_slice(&bytes).expect("the stored bundle is JSON")
}

fn list(entries: &str) -> Vec<u8> {
    format!(r#"{{"attestations":[{entries}]}}"#).into_bytes()
}

fn by_url() -> String {
    format!(r#"{{"repository_id":1,"bundle_url":"{BUNDLE_URL}","initiator":"user","bundle":null}}"#)
}

/// The bundle with its statement changed by `change`, the signature left as it was.
fn with_statement(change: impl Fn(&str) -> String) -> Value {
    let mut b = bundle();
    let payload = STANDARD.decode(b["dsseEnvelope"]["payload"].as_str().unwrap()).unwrap();
    let changed = change(&String::from_utf8(payload).unwrap());
    b["dsseEnvelope"]["payload"] = Value::String(STANDARD.encode(changed));
    b
}

#[test]
fn a_real_bundle_passes_every_step() {
    assert_eq!(verify(&bundle(), FILE, &builder()), Ok(()));
    assert_eq!(verify(&bundle(), &FILE.to_uppercase(), &builder()), Ok(()), "hex in capitals");
}

#[test]
fn a_bundle_about_another_file_is_refused() {
    let other = FILE.replace('5', "6");
    assert_eq!(verify(&bundle(), &other, &builder()), Err("the statement is about another file".into()));
}

#[test]
fn another_repository_workflow_or_tag_is_refused() {
    let ours = Builder::release("v2.103.3");
    assert!(verify(&bundle(), FILE, &ours).is_err_and(|why| why.starts_with("built in another repository")));
    let other_workflow = Builder { workflow: ".github/workflows/release.yml".into(), ..builder() };
    assert!(verify(&bundle(), FILE, &other_workflow).is_err_and(|why| why.starts_with("built by another workflow")));
    let other_ref = Builder { git_ref: "refs/tags/v2.103.3".into(), ..builder() };
    assert!(verify(&bundle(), FILE, &other_ref).is_err_and(|why| why.starts_with("built by another workflow or tag")));
}

#[test]
fn a_changed_statement_breaks_the_signature() {
    let changed = with_statement(|text| text.replacen("linux_386.deb", "linux_387.deb", 1));
    assert_eq!(verify(&changed, FILE, &builder()), Err("the signature does not match the statement".into()));
}

#[test]
fn a_changed_certificate_is_not_from_sigstore() {
    let mut b = bundle();
    let mut der = STANDARD.decode(b["verificationMaterial"]["certificate"]["rawBytes"].as_str().unwrap()).unwrap();
    let at = der.windows(14).position(|w| w == b"deployment.yml").expect("the workflow in the certificate");
    der[at] = b'e';
    b["verificationMaterial"]["certificate"]["rawBytes"] = Value::String(STANDARD.encode(&der));
    assert_eq!(verify(&b, FILE, &builder()), Err("the certificate is not from Sigstore".into()));
}

#[test]
fn the_transparency_log_must_vouch_for_this_signature() {
    let mut moved = bundle();
    let time = moved["verificationMaterial"]["tlogEntries"][0]["integratedTime"].as_str().unwrap().to_string();
    let later = (time.parse::<i64>().unwrap() + 1).to_string();
    moved["verificationMaterial"]["tlogEntries"][0]["integratedTime"] = Value::String(later);
    assert_eq!(verify(&moved, FILE, &builder()), Err("the promise of the transparency log does not match".into()));

    let mut without = bundle();
    without["verificationMaterial"]["tlogEntries"][0].as_object_mut().unwrap().remove("inclusionPromise");
    assert_eq!(verify(&without, FILE, &builder()), Err("the log entry has no promise".into()));

    let mut other_log = bundle();
    other_log["verificationMaterial"]["tlogEntries"][0]["logId"]["keyId"] =
        Value::String("zxGZFVvd0FEmjR8WrFwMdcAJ9vtaY/QXf44Y1wUeP6A=".into());
    assert_eq!(verify(&other_log, FILE, &builder()), Err("no entry in the known transparency log".into()));
}

#[test]
fn a_statement_of_another_kind_is_refused() {
    let release = with_statement(|text| {
        text.replace("https://slsa.dev/provenance/v1", "https://in-toto.io/attestation/release/v0.2")
    });
    assert_eq!(verify(&release, FILE, &builder()), Err("the statement is no build provenance".into()));
    assert!(verify(&json_null(), FILE, &builder()).is_err());
}

fn json_null() -> Value {
    Value::Null
}

#[test]
fn check_asks_the_api_once_and_fetches_the_packed_bundle() {
    let api = builder().api_url(FILE);
    let net = Fake::default().with(&api, list(&by_url())).with(BUNDLE_URL, PACKED.to_vec());
    assert_eq!(check(&net, FILE, &builder()), Outcome::Verified);
    assert_eq!(net.asked(), vec![api, BUNDLE_URL.to_string()], "one request to the API, one for the bundle");
}

#[test]
fn check_takes_a_bundle_inside_the_answer() {
    let api = builder().api_url(FILE);
    let entry = format!(r#"{{"bundle":{},"bundle_url":null}}"#, bundle());
    let net = Fake::default().with(&api, list(&entry));
    assert_eq!(check(&net, FILE, &builder()), Outcome::Verified);
    assert_eq!(net.asked(), vec![api]);
}

#[test]
fn without_attestations_the_outcome_is_missing() {
    let api = builder().api_url(FILE);
    assert_eq!(check(&Fake::default(), FILE, &builder()), Outcome::Missing, "GitHub answers 404");
    let empty = Fake::default().with(&api, list(""));
    assert_eq!(check(&empty, FILE, &builder()), Outcome::Missing, "an empty list");
}

#[test]
fn without_github_the_outcome_is_unreachable() {
    let api = builder().api_url(FILE);
    let limited = Fake::default().with_rate_limit(&api, 900);
    assert!(matches!(check(&limited, FILE, &builder()), Outcome::Unreachable(_)), "the hourly limit");
    let broken = Fake::default().with(&api, b"<html>".to_vec());
    assert!(matches!(check(&broken, FILE, &builder()), Outcome::Unreachable(_)), "no JSON");
    let no_bundle = Fake::default().with(&api, list(&by_url()));
    assert!(
        matches!(check(&no_bundle, FILE, &builder()), Outcome::Unreachable(why) if why.starts_with("bundle:")),
        "the bundle cannot be fetched"
    );
}

#[test]
fn a_bundle_for_someone_else_is_unverified() {
    let ours = Builder::release("v5.1.0");
    let net = Fake::default().with(&ours.api_url(FILE), list(&by_url())).with(BUNDLE_URL, PACKED.to_vec());
    assert!(
        matches!(check(&net, FILE, &ours), Outcome::Unverified(why) if why.starts_with("built in another repository"))
    );
}

#[test]
fn a_broken_checksum_asks_nobody() {
    let net = Fake::default();
    assert!(matches!(check(&net, "abc", &builder()), Outcome::Unverified(_)));
    assert!(net.asked().is_empty());
}

#[test]
fn the_release_builder_names_tag_workflow_and_api() {
    let ours = Builder::release("v5.1.0");
    assert_eq!(ours.signer(), "https://github.com/Xharig/VerseKit/.github/workflows/release.yml@refs/tags/v5.1.0");
    assert_eq!(
        ours.api_url(&"AB".repeat(32)),
        format!("https://api.github.com/repos/Xharig/VerseKit/attestations/sha256:{}", "ab".repeat(32))
    );
    for outcome in
        [Outcome::Verified, Outcome::Missing, Outcome::Unverified("x".into()), Outcome::Unreachable("y".into())]
    {
        assert!(!outcome.log_text(&ours).is_empty());
    }
}

// ------------------------------------------------------------ limits of what comes from the net

#[test]
fn reasons_lose_control_characters_queries_and_length() {
    assert_eq!(
        clean("bundle: https://x.blob.core.windows.net/a.json.sn?sig=geheim&se=1 failed"),
        "bundle: https://x.blob.core.windows.net/a.json.sn?… failed"
    );
    assert_eq!(clean("a\u{1b}[31mb\u{7}c"), "a[31mbc");
    assert_eq!(clean("zwei\nzeilen"), "zwei zeilen");
    assert_eq!(clean("was? wirklich"), "was? wirklich", "a question mark outside an address stays");
    let long = clean(&"x".repeat(500));
    assert_eq!(long.chars().count(), REASON_LIMIT + 1, "cut, with the mark of the cut");
    let logged = Outcome::Unverified(format!("built by {}", "y".repeat(400))).log_text(&builder());
    assert!(logged.chars().count() < REASON_LIMIT + 80, "{logged}");
}

#[test]
fn a_bundle_comes_only_from_the_hosts_of_github() {
    for good in [
        "https://tmaproduction.blob.core.windows.net/attestations/1.json.sn?sig=x",
        "https://github.com/a/b",
        "https://objects.githubusercontent.com/x",
        "https://api.github.com:443/x",
    ] {
        assert!(bundle_host_ok(good), "{good}");
    }
    for bad in [
        "http://tmaproduction.blob.core.windows.net/x",
        "https://example.org/x",
        "https://blob.core.windows.net.example.org/x",
        "https://evilgithub.com/x",
        "https://github.com@example.org/x",
        "",
    ] {
        assert!(!bundle_host_ok(bad), "{bad}");
    }
    let api = builder().api_url(FILE);
    let foreign = r#"{"bundle_url":"https://example.org/b.json.sn","bundle":null}"#;
    let net = Fake::default().with(&api, list(foreign)).with("https://example.org/b.json.sn", PACKED.to_vec());
    assert!(matches!(check(&net, FILE, &builder()), Outcome::Unreachable(why) if why.contains("unknown host")));
    assert_eq!(net.asked(), vec![api], "the foreign address is never asked");
}

#[test]
fn too_large_answers_are_not_read() {
    let api = builder().api_url(FILE);
    let huge = vec![b'{'; BUNDLE_LIMIT + 1];
    let net = Fake::default().with(&api, list(&by_url())).with(BUNDLE_URL, huge.clone());
    assert!(
        matches!(check(&net, FILE, &builder()), Outcome::Unreachable(why) if why.contains("too large")),
        "plain JSON"
    );
    let mut packed = huge;
    packed[0] = 0x80;
    let net = Fake::default().with(&api, list(&by_url())).with(BUNDLE_URL, packed);
    assert!(matches!(check(&net, FILE, &builder()), Outcome::Unreachable(why) if why.contains("too large")), "packed");
    let mut fat = bundle();
    fat["padding"] = Value::String("x".repeat(BUNDLE_LIMIT));
    let net = Fake::default().with(&api, list(&format!(r#"{{"bundle":{fat}}}"#)));
    assert!(matches!(check(&net, FILE, &builder()), Outcome::Unreachable(why) if why.contains("too large")), "inside");
    let net = Fake::default().with(&api, vec![b' '; ANSWER_LIMIT + 1]);
    assert!(matches!(check(&net, FILE, &builder()), Outcome::Unreachable(why) if why.contains("too large")), "answer");
}

/// A net that takes `wait` for every answer.
struct Slow {
    inner: Fake,
    wait: std::time::Duration,
}

impl Net for Slow {
    fn get(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        std::thread::sleep(self.wait);
        self.inner.get(url)
    }

    fn head(&self, url: &str) -> Result<crate::net::Headers, FetchError> {
        self.inner.head(url)
    }
}

#[test]
fn the_whole_check_ends_at_its_deadline() {
    let api = builder().api_url(FILE);
    let fast = Fake::default().with(&api, list(&by_url())).with(BUNDLE_URL, PACKED.to_vec());
    let short = std::time::Duration::from_millis(100);
    assert_eq!(
        check_within(std::sync::Arc::new(fast), FILE, &builder(), std::time::Duration::from_secs(10)),
        Outcome::Verified
    );
    let slow = Slow { inner: Fake::default().with(&api, list(&by_url())), wait: std::time::Duration::from_secs(2) };
    let began = std::time::Instant::now();
    let outcome = check_within(std::sync::Arc::new(slow), FILE, &builder(), short);
    assert!(matches!(&outcome, Outcome::Unreachable(why) if why.starts_with("no answer within")), "{outcome:?}");
    assert!(
        began.elapsed() < std::time::Duration::from_secs(1),
        "not waited for the slow answer: {:?}",
        began.elapsed()
    );
}

#[test]
fn of_several_attestations_one_that_holds_is_enough() {
    let api = builder().api_url(FILE);
    let other = "https://tmaproduction.blob.core.windows.net/attestations/1/other.json.sn";
    let missing = "https://tmaproduction.blob.core.windows.net/attestations/1/gone.json.sn";
    let mut foreign = bundle();
    foreign["verificationMaterial"]["tlogEntries"][0]["integratedTime"] = Value::String("1".into());
    let entries = [
        format!(r#"{{"bundle_url":"{missing}","initiator":"user"}}"#),
        format!(r#"{{"bundle":{foreign},"initiator":"user"}}"#),
        format!(r#"{{"bundle_url":"{other}","initiator":"github"}}"#),
        by_url(),
    ]
    .join(",");
    let net = Fake::default().with(&api, list(&entries)).with(BUNDLE_URL, PACKED.to_vec());
    assert_eq!(check(&net, FILE, &builder()), Outcome::Verified);
    assert!(!net.asked().contains(&other.to_string()), "GitHub's own attestation comes last: {:?}", net.asked());
    // Without the good one: the reason of the one that did not hold, not of the one that was not there.
    let entries = [format!(r#"{{"bundle_url":"{missing}"}}"#), format!(r#"{{"bundle":{foreign}}}"#)].join(",");
    let net = Fake::default().with(&api, list(&entries));
    assert!(matches!(check(&net, FILE, &builder()), Outcome::Unverified(why) if why.contains("promise")));
}

// ------------------------------------------------------------ forged material

#[test]
fn identity_is_compared_exactly_but_the_repository_in_any_case() {
    let shouting = Builder { repo: "CLI/CLI".into(), ..builder() };
    assert!(
        verify(&bundle(), FILE, &shouting).is_err_and(|why| why.starts_with("built by another workflow or tag")),
        "the repository passes in any case, the signer with it does not"
    );
    let branch = Builder { git_ref: "refs/heads/TRUNK".into(), ..builder() };
    assert!(verify(&bundle(), FILE, &branch).is_err_and(|why| why.starts_with("built by another workflow or tag")));
}

fn der_length(length: usize) -> Vec<u8> {
    match length {
        0..=127 => vec![length as u8],
        128..=255 => vec![0x81, length as u8],
        _ => vec![0x82, (length >> 8) as u8, length as u8],
    }
}

fn der_tlv(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    out.extend(der_length(body.len()));
    out.extend_from_slice(body);
    out
}

/// A key pair of its own.
fn key_of(alg: &'static ring::signature::EcdsaSigningAlgorithm) -> ring::signature::EcdsaKeyPair {
    let rng = ring::rand::SystemRandom::new();
    let document = ring::signature::EcdsaKeyPair::generate_pkcs8(alg, &rng).unwrap();
    ring::signature::EcdsaKeyPair::from_pkcs8(alg, document.as_ref(), &rng).unwrap()
}

/// The `SubjectPublicKeyInfo` of an elliptic curve key on `curve`.
fn spki_of(key: &ring::signature::EcdsaKeyPair, curve: &[u8]) -> Vec<u8> {
    use ring::signature::KeyPair;
    let mut algorithm = der_tlv(der::OID, der::OID_EC_KEY);
    algorithm.extend(der_tlv(der::OID, curve));
    let mut bits = vec![0];
    bits.extend_from_slice(key.public_key().as_ref());
    let mut body = der_tlv(der::SEQUENCE, &algorithm);
    body.extend(der_tlv(der::BIT_STRING, &bits));
    der_tlv(der::SEQUENCE, &body)
}

/// The real bundle with a certificate and a signature of our own: the certificate keeps every extension (the right
/// identity) and its validity, only its key is `leaf_key` and it is signed by `ca_key`.
fn with_own_certificate(leaf_key: &ring::signature::EcdsaKeyPair, ca_key: &ring::signature::EcdsaKeyPair) -> Value {
    use ring::signature::KeyPair;
    let rng = ring::rand::SystemRandom::new();
    let mut forged = bundle();
    let real = STANDARD.decode(forged["verificationMaterial"]["certificate"]["rawBytes"].as_str().unwrap()).unwrap();
    // The real certificate with every extension as it was, only the key is ours and the signature of our own CA.
    let (outer, _) = der::read(&real).unwrap();
    let (tbs, rest) = der::read(outer.body).unwrap();
    let (algorithm, _) = der::read(rest).unwrap();
    let parsed = der::certificate(&real).unwrap();
    let mut tbs = tbs.whole.to_vec();
    let at = tbs.windows(parsed.public_key.len()).position(|w| w == parsed.public_key).unwrap();
    let point = parsed.public_key.len() - 65;
    tbs[at + point..at + parsed.public_key.len()].copy_from_slice(leaf_key.public_key().as_ref());
    let signature = ca_key.sign(&rng, &tbs).unwrap();
    let mut bits = vec![0];
    bits.extend_from_slice(signature.as_ref());
    let mut body = tbs;
    body.extend_from_slice(algorithm.whole);
    body.extend(der_tlv(der::BIT_STRING, &bits));
    let certificate = der_tlv(der::SEQUENCE, &body);
    assert!(der::certificate(&certificate).is_some(), "a well-formed certificate");
    forged["verificationMaterial"]["certificate"]["rawBytes"] = Value::String(STANDARD.encode(&certificate));
    let payload = STANDARD.decode(forged["dsseEnvelope"]["payload"].as_str().unwrap()).unwrap();
    let sig = leaf_key.sign(&rng, &pae(DSSE_TYPE, &payload)).unwrap();
    forged["dsseEnvelope"]["signatures"][0]["sig"] = Value::String(STANDARD.encode(sig.as_ref()));
    forged
}

#[test]
fn a_certificate_of_its_own_with_the_right_identity_is_refused() {
    use ring::signature::{ECDSA_P256_SHA256_ASN1_SIGNING, ECDSA_P384_SHA384_ASN1_SIGNING};
    let forged =
        with_own_certificate(&key_of(&ECDSA_P256_SHA256_ASN1_SIGNING), &key_of(&ECDSA_P384_SHA384_ASN1_SIGNING));
    assert_eq!(verify(&forged, FILE, &builder()), Err("the certificate is not from Sigstore".into()));
}

/// The bundle `forged` with one log entry of `log_key` whose content names `payload_hash` and the bundle's own
/// signature and certificate, promised (SET) by `log_key`.
fn with_own_log_entry(forged: &mut Value, log_key: &ring::signature::EcdsaKeyPair, payload_hash: &str) {
    let rng = ring::rand::SystemRandom::new();
    let log = spki_of(log_key, der::OID_P256);
    let log_id = digest::digest(&digest::SHA256, &log);
    let sig = forged["dsseEnvelope"]["signatures"][0]["sig"].as_str().unwrap().to_string();
    let leaf = STANDARD.decode(forged["verificationMaterial"]["certificate"]["rawBytes"].as_str().unwrap()).unwrap();
    let body = serde_json::json!({
        "apiVersion": "0.0.1",
        "kind": "dsse",
        "spec": {
            "payloadHash": {"algorithm": "sha256", "value": payload_hash},
            "signatures": [{"signature": sig, "verifier": STANDARD.encode(pem_of(&leaf))}],
        },
    });
    let body_text = STANDARD.encode(serde_json::to_vec(&body).unwrap());
    let entry = &mut forged["verificationMaterial"]["tlogEntries"][0];
    let time = entry["integratedTime"].as_str().unwrap().to_string();
    let index = entry["logIndex"].as_str().unwrap().to_string();
    let signed = format!(
        r#"{{"body":{},"integratedTime":{time},"logID":"{}","logIndex":{index}}}"#,
        Value::String(body_text.clone()),
        hex(log_id.as_ref())
    );
    let promise = log_key.sign(&rng, signed.as_bytes()).unwrap();
    entry["canonicalizedBody"] = Value::String(body_text);
    entry["logId"]["keyId"] = Value::String(STANDARD.encode(log_id.as_ref()));
    entry["inclusionPromise"]["signedEntryTimestamp"] = Value::String(STANDARD.encode(promise.as_ref()));
}

#[test]
fn a_whole_chain_of_its_own_passes_only_with_a_log_entry_about_this_statement() {
    use ring::signature::{ECDSA_P256_SHA256_ASN1_SIGNING, ECDSA_P384_SHA384_ASN1_SIGNING};
    let (leaf_key, ca_key, log_key) = (
        key_of(&ECDSA_P256_SHA256_ASN1_SIGNING),
        key_of(&ECDSA_P384_SHA384_ASN1_SIGNING),
        key_of(&ECDSA_P256_SHA256_ASN1_SIGNING),
    );
    let anchors = Anchors { intermediate: spki_of(&ca_key, der::OID_P384), log: spki_of(&log_key, der::OID_P256) };
    let mut forged = with_own_certificate(&leaf_key, &ca_key);
    let payload = STANDARD.decode(forged["dsseEnvelope"]["payload"].as_str().unwrap()).unwrap();
    // Every step holds with these anchors: certificate, signature, identity, promise, time, content.
    with_own_log_entry(&mut forged, &log_key, &hex(digest::digest(&digest::SHA256, &payload).as_ref()));
    assert_eq!(verify_with(&forged, FILE, &builder(), &anchors), Ok(()));
    assert!(verify(&forged, FILE, &builder()).is_err(), "never against the public anchors");
    // The same with a log entry, rightly promised, about another statement.
    with_own_log_entry(&mut forged, &log_key, &"0".repeat(64));
    assert_eq!(
        verify_with(&forged, FILE, &builder(), &anchors),
        Err("the log entry is about another statement".into())
    );
}

#[test]
fn a_changed_log_entry_breaks_the_promise() {
    let entry = |b: &mut Value| -> String {
        let text = b["verificationMaterial"]["tlogEntries"][0]["canonicalizedBody"].as_str().unwrap().to_string();
        String::from_utf8(STANDARD.decode(text).unwrap()).unwrap()
    };
    let mut b = bundle();
    let original = entry(&mut b);
    let hash =
        original.split("\"payloadHash\"").nth(1).unwrap().split("\"value\":\"").nth(1).unwrap()[..64].to_string();
    for changed in [
        original.replacen(&hash, &"0".repeat(64), 1),
        original.replacen("\"verifier\":\"LS0t", "\"verifier\":\"LS0T", 1),
    ] {
        assert_ne!(changed, original);
        let mut b = bundle();
        b["verificationMaterial"]["tlogEntries"][0]["canonicalizedBody"] = Value::String(STANDARD.encode(changed));
        assert_eq!(verify(&b, FILE, &builder()), Err("the promise of the transparency log does not match".into()));
    }
}

#[test]
fn the_log_entry_must_name_this_statement_signature_and_certificate() {
    let b = bundle();
    let entry = &b["verificationMaterial"]["tlogEntries"][0];
    let body: Value =
        serde_json::from_slice(&STANDARD.decode(entry["canonicalizedBody"].as_str().unwrap()).unwrap()).unwrap();
    let payload = STANDARD.decode(b["dsseEnvelope"]["payload"].as_str().unwrap()).unwrap();
    let sig = b["dsseEnvelope"]["signatures"][0]["sig"].as_str().unwrap();
    let leaf = STANDARD.decode(b["verificationMaterial"]["certificate"]["rawBytes"].as_str().unwrap()).unwrap();
    assert_eq!(entry_matches(&body, &payload, sig, &leaf), Ok(()));
    let mut other_payload = payload.clone();
    other_payload.push(b' ');
    assert_eq!(
        entry_matches(&body, &other_payload, sig, &leaf),
        Err("the log entry is about another statement".into())
    );
    let other_sig = sig.replacen('M', "N", 1);
    assert_eq!(
        entry_matches(&body, &payload, &other_sig, &leaf),
        Err("the log entry is about another signature".into())
    );
    let mut other_leaf = leaf.clone();
    let last = other_leaf.len() - 1;
    other_leaf[last] ^= 1;
    assert_eq!(
        entry_matches(&body, &payload, sig, &other_leaf),
        Err("the log entry is about another signature".into())
    );
}

// ------------------------------------------------------------ broken input never panics

/// A fixed sequence of numbers (xorshift), so that a failure can be repeated.
struct Dice(u64);

impl Dice {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

#[test]
fn random_and_cut_input_never_panics() {
    let mut dice = Dice(0x5eed_1234_abcd_0001);
    let b = bundle();
    let leaf = STANDARD.decode(b["verificationMaterial"]["certificate"]["rawBytes"].as_str().unwrap()).unwrap();
    let text = serde_json::to_string(&b).unwrap();
    for _ in 0..3000 {
        let bytes: Vec<u8> = (0..dice.below(300)).map(|_| dice.next() as u8).collect();
        let _ = der::read(&bytes);
        let _ = der::elements(&bytes);
        let _ = der::certificate(&bytes);
        let _ = der::ec_public_key(&bytes);
        let _ = snappy::decode(&bytes, 4096);
    }
    for end in 0..leaf.len() {
        assert!(der::certificate(&leaf[..end]).is_none(), "cut at {end}");
    }
    for end in 0..PACKED.len() {
        assert!(snappy::decode(&PACKED[..end], BUNDLE_LIMIT).is_none(), "cut at {end}");
    }
    for _ in 0..400 {
        let mut leaf = leaf.clone();
        let at = dice.below(leaf.len());
        leaf[at] = dice.next() as u8;
        let _ = der::certificate(&leaf);
        let mut packed = PACKED.to_vec();
        let at = dice.below(packed.len());
        packed[at] = dice.next() as u8;
        let _ = snappy::decode(&packed, BUNDLE_LIMIT);
        let mut json = text.clone().into_bytes();
        let at = dice.below(json.len());
        json[at] = b"0aZ\"{}[],:\\"[dice.below(11)];
        if let Ok(value) = serde_json::from_slice::<Value>(&json) {
            let _ = verify(&value, FILE, &builder());
        }
    }
}
