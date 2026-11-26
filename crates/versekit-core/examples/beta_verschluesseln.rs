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

//! Builds the files of a beta release (`beta.rs`): the key ring for a list of testers and the encrypted files, with
//! `SHA256SUMS.txt` over them. Not part of the program players get; `tools/beta_runde.ps1` calls it.
//!
//! ```text
//! beta_verschluesseln neue-codes <Anzahl>
//! beta_verschluesseln verschluesseln --tag beta-v5.7.0-beta.1 --liste <tester.json> --aus <Ordner> <Datei>...
//! ```
//!
//! The list is `[{"tester": "…", "code": "…"}]` and lies outside the repository. ⚠ Codes and names are read only
//! from that file, never from the command line, and never printed: the output names how many testers there are.
//! `neue-codes` prints fresh codes on the console of whoever runs it (for trying it out until the bot makes them).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ring::rand::{SecureRandom, SystemRandom};
use versekit_core::beta::{Code, Cost, KEYRING_FILE, Keyring, RoundKey, SUFFIX, is_beta_tag, seal_file};
use versekit_core::updater::{CHECKSUM_FILE, checksum_of};

/// The environment variable that holds the secret of the build.
const SECRET_ENV: &str = "VERSEKIT_BETA_SCHLUESSEL";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("neue-codes") => new_codes(&args[1..]),
        Some("neuer-schluessel") => new_secret(),
        Some("verschluesseln") => encrypt(&args[1..]),
        _ => Err("usage: neue-codes <count> | neuer-schluessel | verschluesseln --tag <tag> [--liste <file>] --aus <folder> [--schluessel-aus-env [--nur-dateien | --nur-schluesselbund]] <file>...".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("beta_verschluesseln: {why}");
            ExitCode::FAILURE
        }
    }
}

fn new_codes(args: &[String]) -> Result<(), String> {
    let count: usize = args.first().and_then(|n| n.parse().ok()).filter(|n| (1..=500).contains(n)).ok_or("a count of 1 to 500")?;
    let mut ids = Vec::new();
    while ids.len() < count {
        let code = Code::generate()?;
        // The first four characters name the entry: never twice in one round.
        if !ids.iter().any(|c: &Code| c.id() == code.id()) {
            ids.push(code);
        }
    }
    for code in ids {
        println!("{}", code.shown());
    }
    Ok(())
}

/// The key of the round `tag` from the secret of the build in [`SECRET_ENV`] (64 hexadecimal characters).
fn derived_key(tag: &str) -> Result<RoundKey, String> {
    let text = std::env::var(SECRET_ENV).map_err(|_| format!("{SECRET_ENV} is not set"))?;
    let text = text.trim();
    if text.len() < 64 || text.len() % 2 != 0 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("{SECRET_ENV} is no hexadecimal secret of at least 32 bytes"));
    }
    let bytes: Vec<u8> = (0..text.len()).step_by(2).filter_map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok()).collect();
    RoundKey::derive(&bytes, tag)
}

/// A new secret of the build, 64 hexadecimal characters.
fn new_secret() -> Result<(), String> {
    let mut bytes = [0u8; 32];
    SystemRandom::new().fill(&mut bytes).map_err(|_| "no random bytes".to_string())?;
    println!("{}", bytes.iter().map(|b| format!("{b:02x}")).collect::<String>());
    Ok(())
}

fn encrypt(args: &[String]) -> Result<(), String> {
    let mut tag = None;
    let mut list = None;
    let mut out = None;
    let mut files = Vec::new();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--tag" => tag = rest.next().cloned(),
            "--liste" => list = rest.next().map(PathBuf::from),
            "--aus" => out = rest.next().map(PathBuf::from),
            "--schnell" | "--schluessel-aus-env" | "--nur-dateien" | "--nur-schluesselbund" => {}
            file => files.push(PathBuf::from(file)),
        }
    }
    let flag = |name: &str| args.iter().any(|a| a == name);
    let (only_files, only_ring) = (flag("--nur-dateien"), flag("--nur-schluesselbund"));
    if only_files && only_ring {
        return Err("--nur-dateien and --nur-schluesselbund exclude each other".into());
    }
    if (only_files || only_ring) && !flag("--schluessel-aus-env") {
        return Err("the split modes need --schluessel-aus-env: two runs only agree on a key drawn from the secret".into());
    }
    let tag = tag.filter(|t| is_beta_tag(t)).ok_or("--tag beta-v<version>, e.g. beta-v5.7.0-beta.1")?;
    let out = out.ok_or("--aus <folder>")?;
    if only_ring == !files.is_empty() {
        return Err(if only_ring { "--nur-schluesselbund takes no files" } else { "no file to encrypt" }.into());
    }
    // `--schnell` only for trying it out: the least cost a key ring may have.
    let cost = if flag("--schnell") { Cost::LOWEST } else { Cost::DEFAULT };
    let key = if flag("--schluessel-aus-env") { derived_key(&tag)? } else { RoundKey::generate()? };
    fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut written = Vec::new();
    let mut testers = 0;
    if !only_files {
        let codes = read_list(&list.ok_or("--liste <tester.json>")?)?;
        testers = codes.len();
        written.push(write(&out, KEYRING_FILE, Keyring::seal(&tag, cost, &codes, &key)?.to_json().as_bytes())?);
    }
    for file in &files {
        let name = file.file_name().and_then(|n| n.to_str()).ok_or("a file without a name")?;
        let plain = fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
        let sealed = seal_file(&key, &tag, name, &plain)?;
        written.push(write(&out, &format!("{name}{SUFFIX}"), &sealed)?);
    }
    // In a split run the checksums come from the job that gathers the parts.
    if !only_files && !only_ring {
        let mut sums = String::new();
        for path in &written {
            let sum = checksum_of(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            sums.push_str(&format!("{sum}  {name}\n"));
        }
        write(&out, CHECKSUM_FILE, sums.as_bytes())?;
    }
    println!("{tag}: {testers} testers, {} files encrypted, written to {}", files.len(), out.display());
    Ok(())
}

/// The codes of the list; each must be a code, and no two may start alike.
fn read_list(path: &Path) -> Result<Vec<Code>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&text).map_err(|_| "the list is no JSON")?;
    let entries = value.as_array().ok_or("the list is no array")?;
    let mut codes: Vec<Code> = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        // ⚠ The error names the place in the list, never the tester or the code.
        let code = entry.get("code").and_then(|c| c.as_str()).and_then(Code::parse).ok_or(format!("entry {}: no code", i + 1))?;
        if codes.iter().any(|c| c.id() == code.id()) {
            return Err(format!("entry {}: the first four characters are taken already", i + 1));
        }
        codes.push(code);
    }
    if codes.is_empty() {
        return Err("the list is empty".into());
    }
    Ok(codes)
}

fn write(folder: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let path = folder.join(name);
    fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}
