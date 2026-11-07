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

//! Read blueprints and the account from a Game.log.

use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::phrases::Sources;
use crate::text::{PY_NONSPACE, PY_SPACE, decode_ignore, py_strip, title_ascii};

/// How much of a log file is read in one go.
pub const BLOCK: usize = 4 * 1024 * 1024;
/// How far into the start of the file the login is searched for.
pub const ACCOUNT_HEAD: usize = 16 * 1024 * 1024;

const ABBREV: &str = "Civ|Mil|Ind|Sth|Cmp|Las|Ele|Pla|Dis|Mic|Bal|Nah|Min|Slv|Med|Tool|Trc";
const DASH: &str = "\u{2013}|-";

/// Suffix after an item name: `(Mil/1/A)`, `(Ind/4/–)`, `(Bal)`, `(IR1)`.
///
/// ⚠ The abbreviations must match the suffixes Verse-Kit itself writes onto
/// item names.
static SUFFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i){PY_SPACE}*\((?:({ABBREV})/(\d+|{DASH})/([A-D]|{DASH})|({ABBREV})|(IR|EM|CS)(\d{{1,2}}))\){PY_SPACE}*$"
    ))
    .expect("fester Ausdruck")
});

/// The lines that name the player, in the order they are tried: the two of the logs of Star Citizen 4 first, so
/// that those logs are read as before; the line of the login session of Star Citizen 5 last.
static ACCOUNT_RES: LazyLock<[Regex; 3]> = LazyLock::new(|| {
    [
        Regex::new(r"User Login Success - Handle\[([^\]\r\n]+)\]").expect("fester Ausdruck"),
        Regex::new(&format!(r" - name ({PY_NONSPACE}+) - state STATE_CURRENT")).expect("fester Ausdruck"),
        Regex::new(r"<CLoginSession>[^\r\n]*?Fully authenticated : handle\[([^\]\r\n]+)\]").expect("fester Ausdruck"),
    ]
});

/// The fixed number of the account in the line of the logged-in character; it stays when the handle changes.
static ACCOUNT_ID_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<AccountLoginCharacterStatus_Character>[^\r\n]*? - accountId (\d+)\b").expect("fester Ausdruck")
});

/// Whom a log belongs to: the handle and, where the log names it, the fixed account number.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Identity {
    pub handle: Option<String>,
    pub id: Option<String>,
}

/// Class, size, grade of a suffix; missing parts are None.
pub type Extra = (String, Option<String>, Option<String>);

/// A find: the name without suffix and the suffix, if there was one.
pub type Find = (String, Option<Extra>);

/// `7CA 'Nargun' (Civ/3/A)` → `("7CA 'Nargun'", Some(("Civ", "3", "A")))`.
pub fn split_names(raw: &str) -> Find {
    let Some(caps) = SUFFIX_RE.captures(raw) else { return (py_strip(raw).to_string(), None) };
    let start = caps.get(0).map_or(raw.len(), |m| m.start());
    let name = py_strip(&raw[..start]).to_string();
    let group = |i: usize| caps.get(i).map(|m| m.as_str().to_string());
    if let Some(class) = group(1) {
        return (name, Some((title_ascii(&class), group(2), group(3).map(|g| g.to_uppercase()))));
    }
    if let Some(class) = group(4) {
        return (name, Some((title_ascii(&class), None, None)));
    }
    let seeker = group(5).unwrap_or_default().to_uppercase();
    (name, Some((seeker, group(6), None)))
}

/// The blueprint names from a text section. The first non-empty group counts.
pub fn names_from_text(text: &str, pattern: &Regex, sources: &Sources) -> Vec<Find> {
    let mut out = Vec::new();
    for caps in pattern.captures_iter(text) {
        if let Some(value) = caps.iter().skip(1).flatten().map(|m| m.as_str()).find(|v| !v.is_empty()) {
            let (name, extra) = split_names(value);
            out.push((sources.resolve_key(&name), extra));
        }
    }
    out
}

fn read_block(file: &mut File, size: usize) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(size);
    file.by_ref().take(size as u64).read_to_end(&mut buf)?;
    Ok(buf)
}

/// Search a whole log file block by block. Blocks are cut at the last line
/// boundary; the remainder carries over into the next block.
pub fn read_file(path: &Path, pattern: &Regex, sources: &Sources) -> Vec<Find> {
    let mut found = Vec::new();
    let Ok(mut file) = File::open(path) else { return found };
    let mut rest: Vec<u8> = Vec::new();
    while let Ok(block) = read_block(&mut file, BLOCK) {
        if block.is_empty() {
            break;
        }
        let mut joined = std::mem::take(&mut rest);
        joined.extend_from_slice(&block);
        match joined.iter().rposition(|&b| b == b'\n') {
            None => rest = joined,
            Some(cut) => {
                rest = joined[cut + 1..].to_vec();
                found.extend(names_from_text(&decode_ignore(&joined[..cut]), pattern, sources));
            }
        }
    }
    if !rest.is_empty() {
        found.extend(names_from_text(&decode_ignore(&rest), pattern, sources));
    }
    found
}

/// The account from a text section. The expressions are tried in order,
/// not by position of the match.
pub fn account_from_text(text: &str) -> Option<String> {
    ACCOUNT_RES
        .iter()
        .find_map(|rx| rx.captures(text).and_then(|c| c.get(1)).map(|m| py_strip(m.as_str()).to_string()))
        // The handle of the player: private for the streamer mode, wherever it is read.
        .inspect(|found| crate::streamer::learn(found))
}

/// The account a log belongs to. Only the start is read; across the block
/// boundary the last 400 bytes of the block are carried along.
///
/// ⚠ An empty match (`Handle[ ]`) does not count, the search continues in
/// the next block.
pub fn account_of_file(path: &Path) -> Option<String> {
    identity_of_file(path).handle
}

/// The fixed account number from a text section.
pub fn account_id_from_text(text: &str) -> Option<String> {
    ACCOUNT_ID_RE.captures(text).and_then(|c| c.get(1)).map(|m| m.as_str().to_string())
}

/// Handle and account number of a log. The handle is searched as in [`account_of_file`]; the account
/// number in the same block and, if it is not there, in the one block after it.
pub fn identity_of_file(path: &Path) -> Identity {
    let mut found = Identity::default();
    let Ok(mut file) = File::open(path) else { return found };
    let mut read = 0;
    let mut rest: Vec<u8> = Vec::new();
    let mut after_handle = 0;
    while read < ACCOUNT_HEAD || after_handle > 0 {
        let Ok(block) = read_block(&mut file, BLOCK) else { break };
        if block.is_empty() {
            break;
        }
        read += block.len();
        let mut joined = rest.clone();
        joined.extend_from_slice(&block);
        let text = decode_ignore(&joined);
        if found.id.is_none() {
            found.id = account_id_from_text(&text);
        }
        if found.handle.is_none()
            && let Some(handle) = account_from_text(&text).filter(|h| !h.is_empty())
        {
            found.handle = Some(handle);
            after_handle = 2;
        }
        if found.handle.is_some() {
            after_handle -= 1;
            if found.id.is_some() || after_handle == 0 {
                break;
            }
        }
        rest = block[block.len().saturating_sub(400)..].to_vec();
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phrases::pattern;

    fn s(v: &str) -> Option<String> {
        Some(v.to_string())
    }

    #[test]
    fn split_full_suffix() {
        assert_eq!(
            split_names("7CA 'Nargun' (Civ/3/A)"),
            ("7CA 'Nargun'".into(), Some(("Civ".into(), s("3"), s("A"))))
        );
        assert_eq!(
            split_names("Glacis (ind/4/\u{2013})"),
            ("Glacis".into(), Some(("Ind".into(), s("4"), s("\u{2013}"))))
        );
    }

    #[test]
    fn split_other_forms() {
        assert_eq!(
            split_names("P4-AR \"Warhawk\" Rifle (Bal)"),
            ("P4-AR \"Warhawk\" Rifle".into(), Some(("Bal".into(), None, None)))
        );
        assert_eq!(
            split_names("'Arrow' I Missile (IR1)"),
            ("'Arrow' I Missile".into(), Some(("IR".into(), s("1"), None)))
        );
        assert_eq!(split_names("Singe Cannon (S2)"), ("Singe Cannon (S2)".into(), None));
        assert_eq!(split_names("  Helm  "), ("Helm".into(), None));
    }

    #[test]
    fn names_in_text() {
        let sources = Sources::default();
        let rx = pattern(&["Bauplan erhalten".into()]);
        let text = "x\n<SHUDEvent_OnNotification> Added notification \"Bauplan erhalten: 7CA 'Nargun' (Civ/3/A): \" [1]\n\
                    <SHUDEvent_OnNotification> Removed notification \"Bauplan erhalten: Helm: \"\n";
        assert_eq!(
            names_from_text(text, &rx, &sources),
            vec![("7CA 'Nargun'".to_string(), Some(("Civ".to_string(), s("3"), s("A"))))]
        );
    }

    #[test]
    fn account_order_and_trim() {
        let text = "<AccountLoginCharacterStatus_Character> Character: x - name Zweit - state STATE_CURRENT\n\
                    <Legacy login response> [CIG-net] User Login Success - Handle[ Erst ] - Time";
        assert_eq!(account_from_text(text), s("Erst"));
        assert_eq!(account_from_text("nichts"), None);
        // Python counts the separators \x1c to \x1f as whitespace.
        assert_eq!(account_from_text("User Login Success - Handle[\u{1f}Erst\u{1c}]"), s("Erst"));
        assert_eq!(account_from_text(" - name Zw\u{1f}eit - state STATE_CURRENT"), None);
    }

    /// The line of the login of Star Citizen 5 as the log of the PTU has it.
    const LOGIN_5: &str = "<2026-11-06T07:28:47.909Z> [Notice] <CLoginSession> [CompleteAuthentication] {success} \
                           Fully authenticated : handle[Xharig] hub[1] in 7.9s [Team_GameServices][Login]";

    #[test]
    fn the_handle_is_read_from_the_login_of_star_citizen_5_and_the_older_lines_keep_their_place() {
        assert_eq!(account_from_text(LOGIN_5), s("Xharig"));
        // The character line of the new game (state unspecified) says nothing of the player.
        let new_character = "<AccountLoginCharacterStatus_Character> Character: x - name Xharig - state STATE_UNSPECIFIED";
        assert_eq!(account_from_text(new_character), None);
        assert_eq!(account_from_text(&format!("{new_character}\n{LOGIN_5}")), s("Xharig"));
        // A log that has the old lines is read as before, the new line comes last.
        let old = "<Legacy login response> [CIG-net] User Login Success - Handle[Alt] - Time";
        assert_eq!(account_from_text(&format!("{LOGIN_5}\n{old}")), s("Alt"));
        // Only the line of the login session counts, and an empty handle gives no name.
        assert_eq!(account_from_text("<Party> Fully authenticated : handle[Fremd]"), None);
        assert_eq!(account_from_text("<CLoginSession> Fully authenticated : handle[] hub[1]"), None);
    }

    #[test]
    fn the_handle_of_a_log_of_star_citizen_5_is_found_in_the_file() {
        let dir = std::env::temp_dir().join(format!("versekit-logscan-5-{}", crate::testdir::run_tag()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("ptu.log");
        std::fs::write(&file, format!("<2026-11-06T07:28:40.000Z> [Notice] start\n{LOGIN_5}\n")).unwrap();
        assert_eq!(account_of_file(&file), s("Xharig"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn strip_and_space_like_python() {
        assert_eq!(split_names("\u{1f}Helm\u{1e}"), ("Helm".into(), None));
        assert_eq!(split_names("Glacis\u{1c}(Ind/4/A)\u{1d}"), ("Glacis".into(), Some(("Ind".into(), s("4"), s("A")))));
    }

    #[test]
    fn empty_account_reads_on() {
        let dir = std::env::temp_dir().join(format!("versekit-logscan-{}", crate::testdir::run_tag()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("leer.log");
        std::fs::write(&file, "User Login Success - Handle[ ] x\n - name Zweit - state STATE_CURRENT\n").unwrap();
        // The first expression matches with an empty name; Python then keeps
        // searching and finds nothing more in the same file.
        assert_eq!(account_of_file(&file), None);
        std::fs::write(&file, " - name Zweit - state STATE_CURRENT\n").unwrap();
        assert_eq!(account_of_file(&file), s("Zweit"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn identity_takes_the_account_number_of_the_character_line_only() {
        let dir = std::env::temp_dir().join(format!("versekit-logscan-id-{}", crate::testdir::run_tag()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("id.log");
        let character = "<AccountLoginCharacterStatus_Character> Character: createdAt 1 - updatedAt 2 - geid 111 \
                         - accountId 4242 - name Neu - state STATE_CURRENT\n";
        std::fs::write(
            &file,
            format!("<Party> member accountId 999 joined\nUser Login Success - Handle[Neu] - Time\n{character}"),
        )
        .unwrap();
        assert_eq!(identity_of_file(&file), Identity { handle: s("Neu"), id: s("4242") });
        std::fs::write(&file, "User Login Success - Handle[Alt] - Time\n").unwrap();
        assert_eq!(identity_of_file(&file), Identity { handle: s("Alt"), id: None }, "a log without the line");
        assert_eq!(account_id_from_text("<Party> accountId 5 - name X"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
