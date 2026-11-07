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

//! Streamer mode: while it is on, nothing private reaches the screen in clear text.
//!
//! Private is what a viewer of a stream must not read: the player handle and the own account,
//! folder and file paths (data folder, game folder, the user name inside a path), the names and
//! numbers of units and orgs and their members, the connection name and the pairing code of the
//! Basetool, device codes, the reporter of an error report, mail addresses and tokens.
//!
//! One place does the hiding: [`scrub`] replaces everything private in a text by
//! „[Geschwärzt]“ (`[Redacted]` in English). The user interface calls it for every text it
//! draws (a plug-in of the egui context, see `versekit_ui::streamer`), so no page, tooltip,
//! notice or overlay line needs its own check. Two things are private:
//!
//! * the values the program knows ([`learn`]): whoever reads a handle, a unit or a path into the
//!   program says so here, and every later occurrence in any text is covered, whatever the case;
//! * the shape of a thing: an absolute folder or file path, a mail address, a token.
//!
//! Better one value too many than one too few: a text is covered as soon as it contains such a
//! thing. Only the display is changed. The error report keeps its clear text (it has its own
//! `redact`), nothing stored on the disk is touched.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, RwLock};

use regex::{Regex, RegexBuilder};

use crate::language;
use crate::paths::Paths;

/// The key of the setting in `einstellungen.json`.
pub const SETTING: &str = "streamer";

/// The text that stands where something private was.
pub const REDACTED_KEY: &str = "streamer_geschwaerzt";

/// Shortest value that is learned. Two letters would hide half the interface.
const MIN_LEN: usize = 3;

/// Values that are no secret even if a path or a name contains them.
const HARMLESS: [&str; 14] = [
    "users", "user", "benutzer", "home", "windows", "program files", "programme", "documents", "dokumente",
    "appdata", "roaming", "local", "temp", "star citizen",
];

static ACTIVE: AtomicBool = AtomicBool::new(false);
/// Rises with every new value, so the cached matcher knows when to be built again.
static VERSION: AtomicU64 = AtomicU64::new(0);

fn known() -> &'static RwLock<BTreeSet<String>> {
    static KNOWN: OnceLock<RwLock<BTreeSet<String>>> = OnceLock::new();
    KNOWN.get_or_init(|| RwLock::new(BTreeSet::new()))
}

thread_local! {
    /// Tests only: the mode for this thread, so tests that run side by side do not see each other's.
    static THREAD_ACTIVE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Is streamer mode on?
pub fn active() -> bool {
    THREAD_ACTIVE.with(std::cell::Cell::get).unwrap_or_else(|| ACTIVE.load(Ordering::Relaxed))
}

/// Tests only: the mode for the calling thread alone (`None`: the program's own again).
pub fn set_thread_active(on: Option<bool>) {
    THREAD_ACTIVE.with(|cell| cell.set(on));
}

/// Turns streamer mode on or off for the whole program.
pub fn set_active(on: bool) {
    ACTIVE.store(on, Ordering::Relaxed);
}

/// Reads the setting (off when it is missing).
pub fn load(paths: &Paths) -> bool {
    paths.setting_bool(SETTING, false)
}

/// Remembers a private value: every later text that contains it is covered.
///
/// Empty, very short and harmless values are ignored; the value is kept as it is, compared
/// without regard to case.
pub fn learn(value: &str) {
    let value = value.trim();
    if value.chars().count() < MIN_LEN || HARMLESS.contains(&value.to_lowercase().as_str()) {
        return;
    }
    let mut known = known().write().unwrap_or_else(|e| e.into_inner());
    if known.insert(value.to_string()) {
        VERSION.fetch_add(1, Ordering::Relaxed);
    }
}

/// The id of a game controller (`1234ABCD-0000-0000-0000-504944564944`): all of it, and the first
/// part the pages show alone.
pub fn learn_device_id(id: &str) {
    learn(id);
    if let Some(front) = id.split('-').next() {
        learn(front);
    }
}

/// Learns the value and hands it on: `name: streamer::private(text_of(…))` at the place where a
/// private value enters the program from a file or from the Basetool.
pub fn private(value: String) -> String {
    learn(&value);
    value
}

/// A path is private as a whole, also in the other spelling of the separators.
pub fn learn_path(path: &str) {
    let full = path.trim().trim_end_matches(['\\', '/']);
    learn(full);
    // The same path in the other spelling of the separators.
    learn(&full.replace('\\', "/"));
    learn(&full.replace('/', "\\"));
}

/// The settings whose value tells who the player is: the game account, the name of the reporter,
/// the units of the Basetool, the name of the connection, the pairing code of the hangar.
pub const PRIVATE_SETTINGS: [&str; 6] = [
    "eigener_account",
    "melder_name",
    "basetool_einheit_neu",
    "staffellager_einheit",
    "basetool_name",
    crate::hangar_inbox::SETTING_TOKEN,
];

/// What the program knows about the person at the machine: home, user name, the folders in use,
/// and the private settings. It looks the folders up on the disk, so call it once and again when a
/// folder or a setting changed.
pub fn learn_paths(paths: &Paths) {
    for key in PRIVATE_SETTINGS {
        if let Some(value) = paths.setting(key) {
            learn(&value);
        }
    }
    learn_path(&paths.host.home());
    for var in ["USERNAME", "USER", "LOGNAME"] {
        if let Some(user) = paths.host.var_set(var) {
            learn(user);
        }
    }
    learn_path(&paths.documents());
    learn_path(&paths.app_folder());
    if let Some(folder) = paths.game_folder() {
        learn_path(&folder);
    }
    for (_, folder) in paths.chosen_channel_folders() {
        learn_path(&folder);
    }
    for folder in paths.game_roots() {
        learn_path(&folder);
    }
}

/// Forgets what was learned (for tests).
pub fn forget_all() {
    known().write().unwrap_or_else(|e| e.into_inner()).clear();
    VERSION.fetch_add(1, Ordering::Relaxed);
}

/// The text that stands for something hidden, in the current language.
pub fn placeholder() -> String {
    language::t(REDACTED_KEY)
}

/// The shapes of private things: an absolute path (`C:\Users\…`, `\\server\share`, `/home/…`,
/// `/Users/…`, `~/…`), a mail address, a token.
fn shape_patterns() -> &'static Regex {
    static SHAPES: OnceLock<Regex> = OnceLock::new();
    SHAPES.get_or_init(|| {
        // `\b` keeps `https://` out: the `s` of `https` has a letter before it.
        let drive = r"\b[A-Za-z]:[\\/](?:[^\\/:*?<>|\r\n]+[\\/])*[^\\/:*?<>|\s\r\n]*";
        let unc = r"\\\\[^\\/:*?<>|\s]+\\(?:[^\\/:*?<>|\r\n]+\\)*[^\\/:*?<>|\s\r\n]*";
        let unix = r"(?:~|/(?:home|Users|root|mnt|media|run/media|var/home))/(?:[^/:*?<>|\r\n]+/)*[^/:*?<>|\s\r\n]*";
        let mail = r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}";
        let jwt = r"eyJ[A-Za-z0-9_-]{4,}\.[A-Za-z0-9_-]{4,}\.[A-Za-z0-9_-]*";
        let url_secret = r"(?i)[?&](?:token|key|api[_-]?key|secret|auth|password|access[_-]?token|code|handoff)=[^&\s]+";
        // A device id (GUID), a long hex key and the pairing code of the hangar.
        let guid = r"\b[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}\b";
        let hex_key = r"\b[0-9A-Fa-f]{32,}\b";
        let pairing = r"\bVK1-\d+-[0-9A-Za-z]+";
        let all = format!(
            "(?:{drive})|(?:{unc})|(?:{unix})|(?:{mail})|(?:{jwt})|(?:{url_secret})|(?:{guid})|(?:{hex_key})|(?:{pairing})"
        );
        Regex::new(&all).unwrap_or_else(|_| Regex::new("^\\b$").expect("empty pattern"))
    })
}

/// The learned values as one matcher, rebuilt when the set changed.
fn matcher() -> Option<std::sync::Arc<Regex>> {
    static CACHE: Mutex<Option<(u64, Option<std::sync::Arc<Regex>>)>> = Mutex::new(None);
    let version = VERSION.load(Ordering::Relaxed);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((built, matcher)) = cache.as_ref()
        && *built == version
    {
        return matcher.clone();
    }
    let built = {
        let known = known().read().unwrap_or_else(|e| e.into_inner());
        let mut values: Vec<&String> = known.iter().collect();
        // The longest first, so a path wins over the user name inside it.
        values.sort_by_key(|v| std::cmp::Reverse(v.len()));
        if values.is_empty() {
            None
        } else {
            let alternatives: Vec<String> = values.iter().map(|v| regex::escape(v)).collect();
            // A name is private as a word of its own; a path or anything with a separator stands
            // wherever it is found.
            RegexBuilder::new(&alternatives.join("|"))
                .case_insensitive(true)
                .size_limit(32 << 20)
                .build()
                .ok()
                .map(std::sync::Arc::new)
        }
    };
    *cache = Some((version, built.clone()));
    built
}

/// Does `text[start..end]` stand on its own as a word? A handle inside a longer word
/// (`Rob` in `Robust`) is not the handle.
fn is_word(text: &str, start: usize, end: usize, value: &str) -> bool {
    // Paths and anything with a separator are covered wherever they stand.
    if value.contains(['\\', '/', ':', '.', '@']) {
        return true;
    }
    let before = text[..start].chars().next_back();
    let after = text[end..].chars().next();
    !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
}

/// The text with everything private replaced by the placeholder. Streamer mode off: unchanged.
pub fn scrub(text: &str) -> String {
    if !active() {
        return text.to_string();
    }
    cover(text)
}

/// [`scrub`] whether streamer mode is on or not (the filter checks the mode itself).
pub fn cover(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let mark = placeholder();
    // First the shapes (a whole path with the user name inside), then the learned values.
    let shaped = shape_patterns().replace_all(text, mark.as_str()).into_owned();
    let Some(matcher) = matcher() else {
        return shaped;
    };
    let mut out = String::with_capacity(shaped.len());
    let mut last = 0;
    for hit in matcher.find_iter(&shaped) {
        if !is_word(&shaped, hit.start(), hit.end(), hit.as_str()) {
            continue;
        }
        out.push_str(&shaped[last..hit.start()]);
        out.push_str(&mark);
        last = hit.end();
    }
    out.push_str(&shaped[last..]);
    out
}

/// Does the text hold something [`cover`] would hide? Used to skip the work for plain texts.
pub fn needs_cover(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    if shape_patterns().is_match(text) {
        return true;
    }
    matcher().is_some_and(|m| m.find_iter(text).any(|hit| is_word(text, hit.start(), hit.end(), hit.as_str())))
}

#[cfg(test)]
mod tests {
    use super::*;

    // The values are global; these tests share one lock.
    static LOCK: Mutex<()> = Mutex::new(());

    fn on() -> std::sync::MutexGuard<'static, ()> {
        let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        forget_all();
        set_thread_active(Some(true));
        language::set_thread_language(language::Choice::De);
        guard
    }

    #[test]
    fn nothing_changes_while_the_mode_is_off() {
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_thread_active(None);
        learn("Waldmeister");
        assert_eq!(scrub(r"Waldmeister in C:\Users\Anna\Doku"), r"Waldmeister in C:\Users\Anna\Doku");
    }

    #[test]
    fn a_learned_value_is_covered_in_any_case_and_anywhere() {
        let _guard = on();
        learn("Waldmeister");
        let out = scrub("Verbunden als WALDMEISTER, Waldmeister!");
        assert!(!out.to_lowercase().contains("waldmeister"), "{out}");
        assert!(out.contains("[Geschwärzt]"), "{out}");
        set_thread_active(None);
    }

    #[test]
    fn a_value_inside_a_longer_word_stays() {
        let _guard = on();
        learn("Rob");
        assert_eq!(scrub("Robust und Rob"), "Robust und [Geschwärzt]");
        set_thread_active(None);
    }

    #[test]
    fn paths_mails_and_tokens_are_covered_without_being_learned() {
        let _guard = on();
        for private in [
            r"C:\Users\Anna Musterfrau\Documents\Verse-Kit",
            r"D:\Spiele\StarCitizen\LIVE\Game.log",
            r"\\nas\daten\verse",
            "/home/anna/.config/verse-kit",
            "anna@example.org",
        ] {
            let out = scrub(&format!("Ordner: {private} fehlt"));
            assert!(out.starts_with("Ordner: [Geschwärzt]"), "{private} -> {out}");
            assert!(out.ends_with("fehlt"), "{private} -> {out}");
        }
        set_thread_active(None);
    }

    #[test]
    fn the_placeholder_follows_the_language() {
        let _guard = on();
        language::set_thread_language(language::Choice::En);
        assert_eq!(scrub(r"C:\Users\Anna\x"), "[Redacted]");
        language::set_thread_language(language::Choice::Auto);
        set_thread_active(None);
    }

    #[test]
    fn short_and_harmless_values_are_not_learned() {
        let _guard = on();
        learn("ab");
        learn("Users");
        assert_eq!(scrub("ab Users"), "ab Users");
        set_thread_active(None);
    }
}
