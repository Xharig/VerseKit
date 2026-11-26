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

//! The update stage „Betatester“: beta builds that lie in public on GitHub, but encrypted, and open only with a
//! personal code.
//!
//! A beta release carries the tag `beta-v5.7.0-beta.1` (a pre-release of the main repository), the files
//! `VerseKit-Setup.exe.vkbeta` and `VerseKit-x86_64.AppImage.vkbeta`, the key ring [`KEYRING_FILE`] and
//! `SHA256SUMS.txt` over all three. ⚠ The tag starts with `beta-`: [`crate::updater::is_app_tag`] takes it for no
//! version of the app, so no version before this one and no player on the stages „Stable“ or „Test version“ ever sees
//! a beta. They are kept in a remembered list of their own ([`CACHE`]), never in `versionen.json`.
//!
//! The files of one release are encrypted with a random key of the round (AES-256-GCM, the key of a file drawn from
//! it by HKDF). The key ring holds that key once per tester, wrapped with a key drawn from the tester's code by
//! Argon2id, with a salt of its own per entry. The first four characters of a code name its entry, so that one
//! Argon2 run is enough and a code that turns up somewhere tells whose it was; the key ring holds no names.
//!
//! ⚠ The code is kept only in the settings of this computer ([`SETTING_CODE`]), is never sent anywhere and never
//! written into a log or an error report: [`Code`] prints as `Code(…)`, and every failure is one of the few
//! sentences of [`Problem`].

use std::fmt;
use std::fs;
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};

use argon2::{Algorithm, Argon2, Params, Version};
use ring::aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use ring::digest;
use ring::hkdf;
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::{Value as Json, json};

use crate::errors;
use crate::jsonio::{self, Value};
use crate::language::t;
use crate::net::Net;
use crate::paths::Paths;
use crate::updater::{self, Asset, ChecksumProblem, Downloaded, Packaging, Release, UpdateError};
use crate::version::is_newer;

/// The setting of the stage: `true` while the player is a beta tester.
pub const SETTING_ON: &str = "betatester";
/// The setting with the player's code. ⚠ Hidden in the error report.
pub const SETTING_CODE: &str = "beta_code";
/// What the tag of a beta release starts with.
pub const TAG_PREFIX: &str = "beta-";
/// The key ring of a beta release.
pub const KEYRING_FILE: &str = "VerseKit-Beta.vkkey";
/// The ending of an encrypted file.
pub const SUFFIX: &str = ".vkbeta";
/// The remembered beta releases, in `Intern`; written only while the stage is on.
pub const CACHE: &str = "beta-versionen.json";
/// What the last look at the key ring said for which code and which release, in `Intern`.
pub const STATE_FILE: &str = "beta-freigabe.json";
/// The format of the key ring and of the files.
pub const FORMAT: &str = "versekit-beta-1";
/// The start of an encrypted file.
const MAGIC: &[u8; 8] = b"VKBETA1\n";
/// The longest head of an encrypted file.
const HEAD_LIMIT: usize = 64 * 1024;
/// The largest key ring read.
pub const KEYRING_LIMIT: usize = 1024 * 1024;
/// The largest encrypted file opened; it is decrypted in memory.
pub const FILE_LIMIT: u64 = 512 * 1024 * 1024;
/// The characters of a code (Crockford Base32: no I, L, O, U).
pub const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/// The characters of a code, without the dashes.
pub const CODE_LEN: usize = 20;
/// The first characters of a code: the name of its entry in the key ring.
pub const ID_LEN: usize = 4;
const SALT_LEN: usize = 16;
const KEY_LEN: usize = 32;
const TAG_LEN: usize = 16;

// ------------------------------------------------------------ the code

/// A code as the player typed it, brought to its plain form: 20 characters of [`ALPHABET`]. ⚠ Never printed:
/// `{:?}` shows `Code(…)`.
#[derive(Clone, PartialEq, Eq)]
pub struct Code(String);

impl fmt::Debug for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Code(…)")
    }
}

impl Code {
    /// The code in `text`: case, dashes and blanks do not matter, `O` is read as `0`, `I` and `L` as `1`. `None` when
    /// it is no code.
    pub fn parse(text: &str) -> Option<Code> {
        let mut plain = String::with_capacity(CODE_LEN);
        for c in text.chars() {
            if c == '-' || c.is_whitespace() {
                continue;
            }
            if !c.is_ascii() {
                return None;
            }
            let c = match c.to_ascii_uppercase() {
                'O' => '0',
                'I' | 'L' => '1',
                c => c,
            };
            if !ALPHABET.contains(&(c as u8)) || plain.len() == CODE_LEN {
                return None;
            }
            plain.push(c);
        }
        (plain.len() == CODE_LEN).then_some(Code(plain))
    }

    /// A new code from the random source of the system (for the build tool and the checks).
    ///
    /// # Errors
    ///
    /// When the system gives no random bytes.
    pub fn generate() -> Result<Code, String> {
        let mut bytes = [0u8; CODE_LEN];
        SystemRandom::new().fill(&mut bytes).map_err(|_| "no random bytes".to_string())?;
        // 256 is a multiple of 32: every character equally likely.
        Ok(Code(bytes.iter().map(|b| char::from(ALPHABET[usize::from(b & 31)])).collect()))
    }

    /// The name of the entry in the key ring: the first four characters.
    pub fn id(&self) -> &str {
        &self.0[..ID_LEN]
    }

    /// The code in groups of four, `QX4M-K7QM-2XRA-9FHD-W3TB`.
    pub fn shown(&self) -> String {
        self.0.as_bytes().chunks(ID_LEN).map(|c| String::from_utf8_lossy(c).into_owned()).collect::<Vec<_>>().join("-")
    }

    /// Only the name of the entry, the rest hidden: `QX4M-••••-••••-••••-••••`.
    pub fn masked(&self) -> String {
        let mut shown = self.id().to_string();
        for _ in 1..CODE_LEN / ID_LEN {
            shown.push_str("-••••");
        }
        shown
    }
}

// ------------------------------------------------------------ what can go wrong

/// Why a beta could not be opened. Each case has its own sentence; none of them carries the code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    /// No code is stored.
    NoCode,
    /// The key ring has no entry for this code: not (any more) in this round, or the code of an earlier round.
    NotInRound,
    /// The entry is there, but the code does not open it.
    WrongCode,
    /// The key ring or a file is broken (or was changed).
    Damaged,
    /// The key ring or the file belongs to another release or another file.
    Foreign,
}

impl Problem {
    /// The sentence for the player.
    pub fn message(self) -> String {
        t(match self {
            Self::NoCode => "up_beta_kein_code",
            Self::NotInRound => "up_beta_runde",
            Self::WrongCode => "up_beta_falsch",
            Self::Damaged => "up_beta_kaputt",
            Self::Foreign => "up_beta_fremd",
        })
    }

    /// In English for the update log.
    pub fn log_text(self) -> &'static str {
        match self {
            Self::NoCode => "beta: no code stored",
            Self::NotInRound => "beta: the code is not part of this round",
            Self::WrongCode => "beta: the code does not fit",
            Self::Damaged => "beta: key ring or file damaged",
            Self::Foreign => "beta: key ring or file of another release",
        }
    }

    fn word(self) -> &'static str {
        match self {
            Self::NoCode => "kein_code",
            Self::NotInRound => "runde",
            Self::WrongCode => "falsch",
            Self::Damaged => "kaputt",
            Self::Foreign => "fremd",
        }
    }

    fn from_word(word: &str) -> Option<Problem> {
        [Self::NoCode, Self::NotInRound, Self::WrongCode, Self::Damaged, Self::Foreign]
            .into_iter()
            .find(|p| p.word() == word)
    }
}

// ------------------------------------------------------------ keys

/// The key of a round. ⚠ Never stored and never printed.
#[derive(Clone, PartialEq, Eq)]
pub struct RoundKey([u8; KEY_LEN]);

impl fmt::Debug for RoundKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RoundKey(…)")
    }
}

impl RoundKey {
    /// A new random key (for the build tool and the checks).
    ///
    /// # Errors
    ///
    /// When the system gives no random bytes.
    pub fn generate() -> Result<RoundKey, String> {
        Ok(RoundKey(random()?))
    }

    /// The key of the round `tag`, drawn from a secret of the build (at least 32 bytes). The same secret and the same
    /// tag give the same key, so that the two build jobs and the job of the key ring agree without passing a key
    /// around; another tag gives another key.
    ///
    /// # Errors
    ///
    /// A secret shorter than 32 bytes, or a tag that is no beta tag.
    pub fn derive(secret: &[u8], tag: &str) -> Result<RoundKey, String> {
        if secret.len() < KEY_LEN {
            return Err("the build secret is shorter than 32 bytes".into());
        }
        if !is_beta_tag(tag) {
            return Err("the tag of a beta release is beta-v<version>".into());
        }
        let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, FORMAT.as_bytes()).extract(secret);
        let info = [b"round|".as_slice(), tag.as_bytes()];
        let okm = prk.expand(&info, hkdf::HKDF_SHA256).map_err(|_| "no round key")?;
        let mut key = [0u8; KEY_LEN];
        okm.fill(&mut key).map_err(|_| "no round key")?;
        Ok(RoundKey(key))
    }

    /// The key of the files, drawn from the key of the round.
    fn file_key(&self) -> Option<[u8; KEY_LEN]> {
        let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, FORMAT.as_bytes()).extract(&self.0);
        let okm = prk.expand(&[b"file"], hkdf::HKDF_SHA256).ok()?;
        let mut key = [0u8; KEY_LEN];
        okm.fill(&mut key).ok()?;
        Some(key)
    }
}

fn random<const N: usize>() -> Result<[u8; N], String> {
    let mut bytes = [0u8; N];
    SystemRandom::new().fill(&mut bytes).map_err(|_| "no random bytes".to_string())?;
    Ok(bytes)
}

/// What Argon2id may cost: memory in KiB, passes and lanes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cost {
    pub memory_kib: u32,
    pub passes: u32,
    pub lanes: u32,
}

impl Cost {
    /// What a release is built with: 64 MiB, three passes (about half a second).
    pub const DEFAULT: Cost = Cost { memory_kib: 64 * 1024, passes: 3, lanes: 1 };
    /// The least a key ring may ask for; the checks use it to stay quick.
    pub const LOWEST: Cost = Cost { memory_kib: 8 * 1024, passes: 1, lanes: 1 };

    /// ⚠ Only within fixed bounds: a key ring that asks for gigabytes or a thousand passes would make the program
    /// hang, one that asks for nothing would make guessing cheap.
    pub fn allowed(&self) -> bool {
        (8 * 1024..=256 * 1024).contains(&self.memory_kib) && (1..=10).contains(&self.passes) && (1..=4).contains(&self.lanes)
    }
}

/// The key that wraps the key of the round for one tester.
fn wrapping_key(code: &Code, salt: &[u8], cost: Cost) -> Option<[u8; KEY_LEN]> {
    if !cost.allowed() {
        return None;
    }
    let params = Params::new(cost.memory_kib, cost.passes, cost.lanes, Some(KEY_LEN)).ok()?;
    let mut key = [0u8; KEY_LEN];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params).hash_password_into(code.0.as_bytes(), salt, &mut key).ok()?;
    Some(key)
}

fn aead_key(key: &[u8; KEY_LEN]) -> Option<LessSafeKey> {
    UnboundKey::new(&AES_256_GCM, key).ok().map(LessSafeKey::new)
}

fn seal(key: &[u8; KEY_LEN], nonce: [u8; NONCE_LEN], aad: &[u8], data: &mut Vec<u8>) -> Result<(), String> {
    aead_key(key)
        .ok_or("no key")?
        .seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::from(aad), data)
        .map_err(|_| "sealing failed".to_string())
}

/// Opens `data` in place; `false` when the key does not fit or a byte was changed.
fn open(key: &[u8; KEY_LEN], nonce: [u8; NONCE_LEN], aad: &[u8], data: &mut Vec<u8>) -> bool {
    let Some(key) = aead_key(key) else { return false };
    match key.open_in_place(Nonce::assume_unique_for_key(nonce), Aad::from(aad), data) {
        Ok(plain) => {
            let length = plain.len();
            data.truncate(length);
            true
        }
        Err(_) => false,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) ||!text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok()).collect()
}

fn unhex_fixed<const N: usize>(text: &str) -> Option<[u8; N]> {
    unhex(text)?.try_into().ok()
}

fn sha256(bytes: &[u8]) -> String {
    hex(digest::digest(&digest::SHA256, bytes).as_ref())
}

// ------------------------------------------------------------ the key ring

/// One tester's entry: the key of the round, wrapped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The first four characters of the tester's code.
    pub id: String,
    salt: [u8; SALT_LEN],
    nonce: [u8; NONCE_LEN],
    wrapped: Vec<u8>,
}

/// The key ring of a beta release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keyring {
    /// The tag of the release it belongs to.
    pub tag: String,
    pub cost: Cost,
    pub entries: Vec<Entry>,
}

fn entry_aad(tag: &str, id: &str) -> Vec<u8> {
    format!("{FORMAT}|key|{tag}|{id}").into_bytes()
}

impl Keyring {
    /// The key ring for `tag`: `key` wrapped once per code.
    ///
    /// # Errors
    ///
    /// Two codes with the same first four characters, a cost out of bounds, or no random bytes.
    pub fn seal(tag: &str, cost: Cost, codes: &[Code], key: &RoundKey) -> Result<Keyring, String> {
        if !cost.allowed() {
            return Err("the cost of Argon2id is out of bounds".into());
        }
        let mut entries: Vec<Entry> = Vec::with_capacity(codes.len());
        for code in codes {
            if entries.iter().any(|e| e.id == code.id()) {
                return Err(format!("two codes start with {}", code.id()));
            }
            let salt: [u8; SALT_LEN] = random()?;
            let nonce: [u8; NONCE_LEN] = random()?;
            let wrapping = wrapping_key(code, &salt, cost).ok_or("Argon2id failed")?;
            let mut wrapped = key.0.to_vec();
            seal(&wrapping, nonce, &entry_aad(tag, code.id()), &mut wrapped)?;
            entries.push(Entry { id: code.id().to_string(), salt, nonce, wrapped });
        }
        Ok(Keyring { tag: tag.to_string(), cost, entries })
    }

    /// As it lies in the release.
    pub fn to_json(&self) -> String {
        let entries: Vec<Json> = self
            .entries
            .iter()
            .map(|e| json!({"id": e.id, "salz": hex(&e.salt), "nonce": hex(&e.nonce), "schluessel": hex(&e.wrapped)}))
            .collect();
        let value = json!({
            "format": FORMAT,
            "tag": self.tag,
            "argon2id": {"speicher_kib": self.cost.memory_kib, "durchlaeufe": self.cost.passes, "spuren": self.cost.lanes},
            "eintraege": entries,
        });
        serde_json::to_string_pretty(&value).unwrap_or_default()
    }

    /// The key ring in `bytes`.
    ///
    /// # Errors
    ///
    /// [`Problem::Damaged`] for anything that is not exactly a key ring of this format.
    pub fn parse(bytes: &[u8]) -> Result<Keyring, Problem> {
        if bytes.len() > KEYRING_LIMIT {
            return Err(Problem::Damaged);
        }
        let value: Json = serde_json::from_slice(bytes).map_err(|_| Problem::Damaged)?;
        let text = |v: &Json, key: &str| v.get(key).and_then(Json::as_str).map(str::to_string).ok_or(Problem::Damaged);
        let number = |v: &Json, key: &str| {
            v.get(key).and_then(Json::as_u64).and_then(|n| u32::try_from(n).ok()).ok_or(Problem::Damaged)
        };
        if text(&value, "format")? != FORMAT {
            return Err(Problem::Damaged);
        }
        let cost = value.get("argon2id").ok_or(Problem::Damaged)?;
        let cost =
            Cost { memory_kib: number(cost, "speicher_kib")?, passes: number(cost, "durchlaeufe")?, lanes: number(cost, "spuren")? };
        if !cost.allowed() {
            return Err(Problem::Damaged);
        }
        let mut entries = Vec::new();
        for entry in value.get("eintraege").and_then(Json::as_array).ok_or(Problem::Damaged)? {
            let id = text(entry, "id")?;
            if id.len() != ID_LEN || !id.bytes().all(|b| ALPHABET.contains(&b)) {
                return Err(Problem::Damaged);
            }
            let wrapped = unhex(&text(entry, "schluessel")?).ok_or(Problem::Damaged)?;
            if wrapped.len() != KEY_LEN + TAG_LEN {
                return Err(Problem::Damaged);
            }
            entries.push(Entry {
                id,
                salt: unhex_fixed(&text(entry, "salz")?).ok_or(Problem::Damaged)?,
                nonce: unhex_fixed(&text(entry, "nonce")?).ok_or(Problem::Damaged)?,
                wrapped,
            });
        }
        Ok(Keyring { tag: text(&value, "tag")?, cost, entries })
    }

    /// The key of the round with `code`, for the release `tag`.
    ///
    /// # Errors
    ///
    /// [`Problem::Foreign`] for the key ring of another release, [`Problem::NotInRound`] without an entry for the
    /// code, [`Problem::WrongCode`] when the code does not open it.
    pub fn unlock(&self, tag: &str, code: &Code) -> Result<RoundKey, Problem> {
        if self.tag != tag {
            return Err(Problem::Foreign);
        }
        let entry = self.entries.iter().find(|e| e.id == code.id()).ok_or(Problem::NotInRound)?;
        let wrapping = wrapping_key(code, &entry.salt, self.cost).ok_or(Problem::Damaged)?;
        let mut key = entry.wrapped.clone();
        if !open(&wrapping, entry.nonce, &entry_aad(tag, &entry.id), &mut key) {
            return Err(Problem::WrongCode);
        }
        key.try_into().map(RoundKey).map_err(|_| Problem::Damaged)
    }
}

// ------------------------------------------------------------ the files

/// `plain` encrypted as the file `name` of the release `tag`. The head (tag, name, size, checksum of the plain file)
/// is bound to the content: a file of another release or another name does not open as this one.
///
/// # Errors
///
/// No random bytes.
pub fn seal_file(key: &RoundKey, tag: &str, name: &str, plain: &[u8]) -> Result<Vec<u8>, String> {
    let file_key = key.file_key().ok_or("no file key")?;
    let nonce: [u8; NONCE_LEN] = random()?;
    let head = json!({
        "format": FORMAT,
        "tag": tag,
        "name": name,
        "groesse": plain.len(),
        "sha256": sha256(plain),
        "nonce": hex(&nonce),
    })
    .to_string();
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&u32::try_from(head.len()).map_err(|_| "head too long")?.to_le_bytes());
    out.extend_from_slice(head.as_bytes());
    let mut body = plain.to_vec();
    seal(&file_key, nonce, &out, &mut body)?;
    out.extend_from_slice(&body);
    Ok(out)
}

/// The plain file in `data` and its SHA-256, when it is the file `name` of the release `tag` and opens with `key`.
///
/// # Errors
///
/// [`Problem::Foreign`] for a file of another release or name, [`Problem::Damaged`] for anything broken or changed.
pub fn open_file(key: &RoundKey, tag: &str, name: &str, mut data: Vec<u8>) -> Result<(Vec<u8>, String), Problem> {
    let start = MAGIC.len() + 4;
    if data.len() < start || &data[..MAGIC.len()] != MAGIC {
        return Err(Problem::Damaged);
    }
    let mut length = [0u8; 4];
    length.copy_from_slice(&data[MAGIC.len()..start]);
    let length = usize::try_from(u32::from_le_bytes(length)).map_err(|_| Problem::Damaged)?;
    if length > HEAD_LIMIT || data.len() < start + length + TAG_LEN {
        return Err(Problem::Damaged);
    }
    let head: Json = serde_json::from_slice(&data[start..start + length]).map_err(|_| Problem::Damaged)?;
    let text = |key: &str| head.get(key).and_then(Json::as_str).ok_or(Problem::Damaged);
    if text("format")? != FORMAT {
        return Err(Problem::Damaged);
    }
    if text("tag")? != tag || text("name")? != name {
        return Err(Problem::Foreign);
    }
    let size = head.get("groesse").and_then(Json::as_u64).ok_or(Problem::Damaged)?;
    let checksum = text("sha256")?.to_lowercase();
    let nonce: [u8; NONCE_LEN] = unhex_fixed(text("nonce")?).ok_or(Problem::Damaged)?;
    let file_key = key.file_key().ok_or(Problem::Damaged)?;
    let mut body = data.split_off(start + length);
    // ⚠ The key came from a key ring that opened: a body that does not open was changed or cut off.
    if !open(&file_key, nonce, &data, &mut body) {
        return Err(Problem::Damaged);
    }
    let actual = sha256(&body);
    if u64::try_from(body.len()).ok() != Some(size) || actual != checksum {
        return Err(Problem::Damaged);
    }
    Ok((body, actual))
}

// ------------------------------------------------------------ the stage

/// The three stages of updating.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Stage {
    #[default]
    Stable,
    Test,
    Beta,
}

impl Stage {
    /// The stage the settings say.
    pub fn of(paths: &Paths) -> Stage {
        if on(paths) {
            Stage::Beta
        } else if paths.setting_bool(updater::PRERELEASE_SETTING, false) {
            Stage::Test
        } else {
            Stage::Stable
        }
    }

    /// Whether test versions count: on the stages „Test version“ and „Betatester“.
    pub fn with_prerelease(self) -> bool {
        self != Stage::Stable
    }
}

/// Is the stage „Betatester“ on?
pub fn on(paths: &Paths) -> bool {
    paths.setting_bool(SETTING_ON, false)
}

/// The stored code, if there is one.
pub fn stored_code(paths: &Paths) -> Option<Code> {
    match paths.settings_value(SETTING_CODE) {
        Some(Value::Str(text)) => Code::parse(&text),
        _ => None,
    }
}

/// Chooses a stage. Leaving „Betatester“ deletes the code: the next round brings a new one anyway. Returns whether
/// the settings could be written.
pub fn choose(paths: &Paths, stage: Stage) -> bool {
    let mut ok = paths.set_setting(SETTING_ON, Value::Bool(stage == Stage::Beta));
    ok &= paths.set_setting(updater::PRERELEASE_SETTING, Value::Bool(stage.with_prerelease()));
    if stage != Stage::Beta {
        ok &= forget_code(paths);
    }
    ok
}

/// Deletes what was found out with a code and the remembered beta releases, keeping the code itself.
pub fn forget_state(paths: &Paths) {
    remove_state(paths);
    match fs::remove_file(paths.app_file(CACHE)) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => errors::record("beta.remove_cache", &error),
        _ => {}
    }
}

/// Deletes the code, what was found out with it and the remembered beta releases.
pub fn forget_code(paths: &Paths) -> bool {
    forget_state(paths);
    if paths.settings_value(SETTING_CODE).is_none() {
        return true;
    }
    paths.set_setting(SETTING_CODE, Value::Str(String::new()))
}

/// Stores the code; what was found out with an earlier one no longer counts.
pub fn save_code(paths: &Paths, code: &Code) -> bool {
    remove_state(paths);
    paths.set_setting(SETTING_CODE, Value::Str(code.shown()))
}

// ------------------------------------------------------------ the beta releases

/// Is `tag` the tag of a beta release: `beta-` and then a version of the app?
pub fn is_beta_tag(tag: &str) -> bool {
    tag.strip_prefix(TAG_PREFIX).is_some_and(updater::is_app_tag)
}

/// The beta releases in an answer of the GitHub API.
pub fn releases_from_api(body: &[u8]) -> Option<Vec<Release>> {
    updater::releases_matching(body, is_beta_tag)
}

/// The remembered beta releases.
pub fn cache_read(paths: &Paths) -> Vec<Release> {
    fs::read(paths.app_file(CACHE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Json>(&bytes).ok())
        .map(|value| updater::cache_from_value(&value).releases)
        .unwrap_or_default()
        .into_iter()
        .filter(|r| is_beta_tag(&r.version))
        .collect()
}

/// Remembers the beta releases.
///
/// # Errors
///
/// When the file cannot be written.
pub fn cache_write(paths: &Paths, releases: &[Release], at: f64) -> io::Result<()> {
    let cache = updater::Cache { checked: at, releases: releases.to_vec() };
    let text = serde_json::to_string(&updater::cache_to_value(&cache)).map_err(io::Error::other)?;
    jsonio::write_text_atomic(&paths.app_file(CACHE), &text, jsonio::Newline::Keep)
}

/// The newest beta release in `releases`.
pub fn newest(releases: &[Release]) -> Option<&Release> {
    let mut best: Option<&Release> = None;
    for release in releases.iter().filter(|r| is_beta_tag(&r.version)) {
        if best.is_none_or(|b| is_newer(&release.version, &b.version)) {
            best = Some(release);
        }
    }
    best
}

// ------------------------------------------------------------ what the code opens

/// What is known of the stage and the code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// The stage is off.
    Off,
    NoCode,
    /// No beta release is known.
    NoBeta,
    /// The newest beta was not looked at with this code yet.
    Unchecked,
    /// The code opens the newest beta (its tag).
    Fits(String),
    Problem(Problem),
}

/// A mark of the code that does not give it away, so that the state of another code never counts.
fn fingerprint(code: &Code) -> String {
    sha256(format!("{FORMAT}|fingerprint|{}", code.0).as_bytes())[..16].to_string()
}

fn state_read(paths: &Paths) -> Option<(String, String, String)> {
    let value: Json = serde_json::from_slice(&fs::read(paths.app_file(STATE_FILE)).ok()?).ok()?;
    let text = |key: &str| value.get(key).and_then(Json::as_str).map(str::to_string);
    Some((text("code")?, text("tag")?, text("ergebnis")?))
}

fn state_write(paths: &Paths, code: &Code, tag: &str, outcome: &str) {
    let text = json!({"code": fingerprint(code), "tag": tag, "ergebnis": outcome}).to_string();
    if let Err(error) = jsonio::write_text_atomic(&paths.app_file(STATE_FILE), &text, jsonio::Newline::Keep) {
        errors::record("beta.state_write", &error);
    }
}

fn remove_state(paths: &Paths) {
    match fs::remove_file(paths.app_file(STATE_FILE)) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => errors::record("beta.remove_state", &error),
        _ => {}
    }
}

/// What is known, from the remembered list and the last look, without the net.
pub fn status(paths: &Paths) -> Status {
    status_for(paths, on(paths))
}

/// [`status`] for a window that knows the stage before its settings are written (`stage_on`).
pub fn status_for(paths: &Paths, stage_on: bool) -> Status {
    if !stage_on {
        return Status::Off;
    }
    let Some(code) = stored_code(paths) else { return Status::NoCode };
    let releases = cache_read(paths);
    let Some(release) = newest(&releases) else { return Status::NoBeta };
    match state_read(paths) {
        Some((mark, tag, outcome)) if mark == fingerprint(&code) && tag == release.version => {
            if outcome == "passt" {
                Status::Fits(tag)
            } else {
                Problem::from_word(&outcome).map_or(Status::Unchecked, Status::Problem)
            }
        }
        _ => Status::Unchecked,
    }
}

/// The newest beta release, when the stage is on and the stored code opens it.
pub fn unlocked(paths: &Paths) -> Option<Release> {
    unlocked_for(paths, on(paths))
}

/// [`unlocked`] for a window that knows the stage before its settings are written (`stage_on`).
pub fn unlocked_for(paths: &Paths, stage_on: bool) -> Option<Release> {
    let Status::Fits(tag) = status_for(paths, stage_on) else { return None };
    cache_read(paths).into_iter().find(|r| r.version == tag)
}

/// The unlocked beta, when it is newer than `own`.
pub fn offer(paths: &Paths, own: &str) -> Option<Release> {
    unlocked(paths).filter(|r| is_newer(&r.version, own))
}

/// Why the key ring could not be had.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Fail {
    Problem(Problem),
    Checksums(ChecksumProblem),
    Net(String),
}

/// The key ring of `release`, checked against the published checksum.
fn fetch_keyring(net: &dyn Net, release: &Release) -> Result<Keyring, Fail> {
    let Some(asset) = release.assets.iter().find(|a| a.name == KEYRING_FILE) else {
        return Err(Fail::Problem(Problem::Damaged));
    };
    if !updater::url_ok(&asset.url) {
        return Err(Fail::Problem(Problem::Foreign));
    }
    let table = updater::fetch_checksums(net, release).map_err(Fail::Checksums)?;
    let expected = table.get(KEYRING_FILE).ok_or(Fail::Problem(Problem::Damaged))?;
    let body = net.get(&asset.url).map_err(|e| Fail::Net(e.to_string()))?;
    if body.len() > KEYRING_LIMIT || sha256(&body) != *expected {
        return Err(Fail::Problem(Problem::Damaged));
    }
    Keyring::parse(&body).map_err(Fail::Problem)
}

/// Looks with the stored code at the key ring of the newest beta, when that was not done for this code and this
/// release yet (`force`: in any case), and remembers what came out. The net is asked only then.
///
/// # Errors
///
/// When the key ring could not be fetched; nothing is remembered then.
pub fn refresh(net: &dyn Net, paths: &Paths, force: bool) -> Result<Status, String> {
    let known = status(paths);
    let unchecked = matches!(known, Status::Unchecked);
    if (!force && !unchecked) || matches!(known, Status::Off | Status::NoCode | Status::NoBeta) {
        return Ok(known);
    }
    let Some(code) = stored_code(paths) else { return Ok(Status::NoCode) };
    let releases = cache_read(paths);
    let Some(release) = newest(&releases) else { return Ok(Status::NoBeta) };
    let outcome = match fetch_keyring(net, release) {
        Ok(ring) => ring.unlock(&release.version, &code).map(|_| ()),
        Err(Fail::Problem(problem)) => Err(problem),
        Err(Fail::Checksums(ChecksumProblem::Missing | ChecksumProblem::Foreign)) => Err(Problem::Damaged),
        Err(Fail::Checksums(_)) => return Err("the checksums of the beta could not be fetched".into()),
        Err(Fail::Net(error)) => return Err(error),
    };
    // A release whose files are still being attached, or a mirror that is behind, looks damaged for a while: that
    // is asked again at the next look instead of being remembered for this code and this release.
    if outcome == Err(Problem::Damaged) {
        return Err("the beta files are not complete or do not match yet".into());
    }
    state_write(paths, &code, &release.version, outcome.map_or_else(Problem::word, |()| "passt"));
    Ok(status(paths))
}

// ------------------------------------------------------------ fetching

/// The encrypted file of a beta release that fits the packaging.
pub fn matching_asset(release: &Release, kind: Packaging) -> Option<&Asset> {
    let endings: &[&str] = match kind {
        Packaging::Source => return None,
        Packaging::AppImage => &[".appimage.vkbeta"],
        Packaging::Exe => &["-setup.exe.vkbeta"],
    };
    release.assets.iter().find(|asset| {
        let name = asset.name.to_lowercase();
        endings.iter().any(|ending| name.ends_with(ending)) && updater::url_ok(&asset.url)
    })
}

/// The name of the plain file for the encrypted `name`, when it is one an update may install.
pub fn plain_name(name: &str) -> Option<String> {
    let lower = name.to_lowercase();
    let cut = name.len().checked_sub(SUFFIX.len()).filter(|_| lower.ends_with(SUFFIX))?;
    let plain = updater::safe_filename(&name[..cut])?;
    (plain == name[..cut]).then_some(plain)
}

/// Fetches the beta file `asset` of `release`, opens it with `code` and puts the plain file to `target`.
///
/// The key ring comes first (a wrong code is told before 60 megabytes come in), then the encrypted file against its
/// published checksum, then it is opened in memory and checked once more against the checksum in its head. ⚠ On
/// every way out every file of the way is removed again: an encrypted, a half-written or an unchecked file never
/// stays.
///
/// # Errors
///
/// See [`UpdateError`]; [`UpdateError::Beta`] for what is the beta's own.
pub fn download(
    net: &dyn Net,
    release: &Release,
    asset: &Asset,
    code: &Code,
    target: &Path,
    progress: &mut dyn FnMut(u8),
) -> Result<Downloaded, UpdateError> {
    if !updater::url_ok(&asset.url) || !is_beta_tag(&release.version) {
        return Err(UpdateError::ForeignSource);
    }
    let Some(name) = plain_name(&asset.name) else {
        return Err(UpdateError::ForeignFile(if asset.name.is_empty() { "?".into() } else { asset.name.clone() }));
    };
    let key = match fetch_keyring(net, release) {
        Ok(ring) => ring.unlock(&release.version, code).map_err(UpdateError::Beta)?,
        Err(Fail::Problem(problem)) => return Err(UpdateError::Beta(problem)),
        Err(Fail::Checksums(problem)) => return Err(UpdateError::NoChecksum(problem)),
        Err(Fail::Net(error)) => return Err(UpdateError::Fetch(error)),
    };
    let table = updater::fetch_checksums(net, release).map_err(UpdateError::NoChecksum)?;
    let Some(expected) = table.get(&asset.name).cloned() else {
        return Err(UpdateError::NoChecksum(ChecksumProblem::Missing));
    };
    let encrypted = with_suffix(target, SUFFIX);
    let partial = with_suffix(target, ".teil");
    let result = updater::fetch_and_verify(net, &asset.url, &encrypted, &expected, progress)
        .and_then(|()| decrypt_to(&key, &release.version, &name, &encrypted, &partial, target));
    for path in [&encrypted, &partial] {
        updater::discard(path);
    }
    match result {
        Ok(checksum) => Ok(Downloaded { path: target.to_path_buf(), checksum }),
        Err(error) => {
            updater::discard(target);
            Err(error)
        }
    }
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

/// Opens the encrypted file and puts the plain one to `target` (through `partial`, so that `target` is either whole
/// or not there). The checksum of the plain file.
fn decrypt_to(
    key: &RoundKey,
    tag: &str,
    name: &str,
    encrypted: &Path,
    partial: &Path,
    target: &Path,
) -> Result<String, UpdateError> {
    let disk = |e: io::Error| UpdateError::Disk(e.to_string());
    let file = fs::File::open(encrypted).map_err(disk)?;
    if file.metadata().map_err(disk)?.len() > FILE_LIMIT {
        return Err(UpdateError::Beta(Problem::Damaged));
    }
    let mut data = Vec::new();
    file.take(FILE_LIMIT + 1).read_to_end(&mut data).map_err(disk)?;
    let (plain, checksum) = open_file(key, tag, name, data).map_err(UpdateError::Beta)?;
    fs::write(partial, &plain).map_err(disk)?;
    drop(plain);
    if !updater::still_matches(partial, &checksum) {
        return Err(UpdateError::ChecksumWrong);
    }
    fs::rename(partial, target).map_err(disk)?;
    Ok(checksum)
}

#[cfg(test)]
mod tests;
