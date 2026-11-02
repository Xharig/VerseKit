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

//! Takes personal data out of a text before it can become public
//! (`paths.redact`): error log and error report end up in a public issue.
//!
//! Replaced: the home folder (`<heim>`), a Documents folder outside it
//! (`<dokumente>`) and its own parent (`<eigener-ordner>`), every further
//! occurrence of the user name (`<benutzer>`), and credentials: the report
//! address, Discord webhooks, tokens, JWTs, `Authorization`/`DPoP` headers.
//! Other paths (a game folder on `D:\`) stay.
//!
//! Beyond 4.12, for lines of the game's log in a report: the player handles
//! the program knows (`<SPIELER>`), the RSI player number behind
//! `for '<name>' [..]` and `geid`/`playerId`/`accountId` (`<SPIELERNR>`),
//! the name of the computer (`<RECHNER>`), IP addresses but `127.0.0.1` and
//! `::1` (`<IP>`), mail addresses (`<MAIL>`) and the values of
//! `shopId`/`kioskId` (`<KENNUNG>`). Numbers of things, as in
//! `'Harvestable_Oza_123' [123]`, stay.

use std::sync::OnceLock;

use regex::{NoExpand, Regex};

use crate::paths::{self, Host, Paths};

/// What goes out when redacting fails.
pub const REDACT_FAILED: &str = "<nicht geschwärzt, weggelassen>";

/// Where error reports are sent (`report_target.RELAY`).
pub const RELAY: &str = "https://bericht.xharig.com/bericht";

/// Parent folders that are not personal (`_private_folders`).
const GENERIC_PARENTS: [&str; 6] = ["users", "benutzer", "home", "onedrive", "daten", "data"];

// Python's `\s` also covers `\x1c` to `\x1f`.
const WEBHOOK: &str = r"(?i)https://[\w.-]*discord(?:app)?\.com/api/webhooks/[^\s\x1C-\x1F]+";
const AUTH_HEADER: &str = r"(?i)((?:Authorization|DPoP|DPoP-Nonce|Idempotency-Key)[\s\x1C-\x1F]*[:=][\s\x1C-\x1F]*)(?:(?:DPoP|Bearer)[\s\x1C-\x1F]+)?[^\s\x1C-\x1F,;]+";
const JWT: &str = r"eyJ[A-Za-z0-9_-]{4,}\.[A-Za-z0-9_-]{4,}\.[A-Za-z0-9_-]*";
const FORM_SECRET: &str = r"(\b(?:refresh_token|access_token|device_code|client_secret)=)[^&\s\x1C-\x1F]+";
const CREDENTIAL: &str = r"(?i)([?&](?:token|key|api[_-]?key|apikey|secret|auth|password|passwd|pw|access[_-]?token|signature|sig|refresh[_-]?token|device[_-]?code|handoff)=)[^&\s\x1C-\x1F]+";

/// The player number after a name without underscore (a thing or a figure of the game has one,
/// `Harvestable_Oza_123`) behind `for`, `by` or `Kill:`, or after a handle already replaced:
/// `for 'Name' [2000123]`, `killed by 'Name' [2000124]`, `'<SPIELER>' [2000123]`.
const PLAYER_NUMBER: &str = r"((?:\bfor|\bby|\bKill:) '[A-Za-z0-9-]+' \[|'<SPIELER>' \[)\d+\]";
/// `geid 2000123`, `playerGEID=2000123`, `owner_geid[5]`, `accountId: 55`, `"player_id":"9"`.
const PLAYER_KEY: &str = r#"(?i)\b([a-z_]*geid|[a-z_]*?(?:player|account|user|character|owner)_?id)(["']?(?:[ \t]*[=:][ \t]*|[ \t]+)["'\[]?|\[)\d+"#;
/// Where the game's log names the player: `Handle[Name]`, `- name Name - state STATE_CURRENT`,
/// `nickname="Name"`.
const HANDLE_IN_LOG: &str =
    r#"Handle\[([^\]\s]+)\]| - name (\S+) - state STATE_CURRENT|(?i:nickname)[ \t]*[=:][ \t]*"?([A-Za-z0-9_-]+)"#;
/// `shopId=…`, `kioskId: …`; only a value with a digit is a number of a place.
const SHOP_KEY: &str = r#"(?i)\b(shopid|kioskid)(["']?(?:[ \t]*[=:][ \t]*|[ \t]+)["'\[]?)([A-Za-z0-9_-]+)"#;
const MAIL: &str = r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}";
const IPV4: &str = r"\d{1,3}(?:\.\d{1,3}){3}";
const IPV6: &str = r"[0-9A-Fa-f:]{3,}";
/// Computer names that say nothing about anyone.
const PLAIN_HOSTS: [&str; 2] = ["localhost", "localhost.localdomain"];

struct Patterns {
    webhook: Regex,
    auth_header: Regex,
    jwt: Regex,
    form_secret: Regex,
    credential: Regex,
    player_number: Regex,
    player_key: Regex,
    handle_in_log: Regex,
    shop_key: Regex,
    mail: Regex,
    ipv4: Regex,
    ipv6: Regex,
}

fn patterns() -> Option<&'static Patterns> {
    static PATTERNS: OnceLock<Option<Patterns>> = OnceLock::new();
    PATTERNS
        .get_or_init(|| {
            Some(Patterns {
                webhook: Regex::new(WEBHOOK).ok()?,
                auth_header: Regex::new(AUTH_HEADER).ok()?,
                jwt: Regex::new(JWT).ok()?,
                form_secret: Regex::new(FORM_SECRET).ok()?,
                credential: Regex::new(CREDENTIAL).ok()?,
                player_number: Regex::new(PLAYER_NUMBER).ok()?,
                player_key: Regex::new(PLAYER_KEY).ok()?,
                handle_in_log: Regex::new(HANDLE_IN_LOG).ok()?,
                shop_key: Regex::new(SHOP_KEY).ok()?,
                mail: Regex::new(MAIL).ok()?,
                ipv4: Regex::new(IPV4).ok()?,
                ipv6: Regex::new(IPV6).ok()?,
            })
        })
        .as_ref()
}

/// A character that belongs to a name or number: `needle` does not stand alone next to it.
fn name_char(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

/// Every occurrence of `needle` that stands alone (no letter, digit, `_` or `-` right before or
/// after it), ignoring case, replaced with `mark`.
fn replace_word(text: &str, needle: &Regex, mark: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for found in needle.find_iter(text) {
        let before = text[..found.start()].chars().next_back();
        let after = text[found.end()..].chars().next();
        if name_char(before) || name_char(after) {
            continue;
        }
        out.push_str(&text[last..found.start()]);
        out.push_str(mark);
        last = found.end();
    }
    out.push_str(&text[last..]);
    out
}

/// IPv4 addresses with each part up to 255, standing alone; `127.0.0.1` stays.
fn replace_ipv4(text: &str, pattern: &Regex) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for found in pattern.find_iter(text) {
        let before = text[..found.start()].chars().next_back();
        let rest = &text[found.end()..];
        let mut after = rest.chars();
        let next = after.next();
        let joined = before.is_some_and(|c| c.is_alphanumeric() || c == '.')
            || next.is_some_and(|c| c.is_alphanumeric())
            || (next == Some('.') && after.next().is_some_and(|c| c.is_ascii_digit()));
        let parts_ok = found.as_str().split('.').all(|part| part.parse::<u16>().is_ok_and(|n| n <= 255));
        if joined || !parts_ok || found.as_str() == "127.0.0.1" {
            continue;
        }
        out.push_str(&text[last..found.start()]);
        out.push_str("<IP>");
        last = found.end();
    }
    out.push_str(&text[last..]);
    out
}

/// The place of an IPv6 address in the run of hex digits and colons at `start..end`, if it is one:
/// standing alone, with a digit, and three groups or `::` after a group of four (`fe80::1`), so
/// neither a time `12:30:00` nor `a::b` of a code path counts. `::1` and `::` stay.
fn ipv6_at(text: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    let candidate = &text[start..end];
    let before = text[..start].chars().next_back();
    let mut rest = text[end..].chars();
    let after = rest.next();
    let joined = before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.')
        || after.is_some_and(|c| c.is_alphanumeric() || c == '_')
        || (after == Some('.') && rest.next().is_some_and(|c| c.is_ascii_digit()));
    let address = candidate.parse::<std::net::Ipv6Addr>().ok()?;
    let groups: Vec<&str> = candidate.split(':').filter(|g| !g.is_empty()).collect();
    let enough = groups.len() >= 3 || (candidate.contains("::") && groups.first().is_some_and(|g| g.len() == 4));
    let digit = candidate.bytes().any(|b| b.is_ascii_digit());
    let plain = address.is_loopback() || address.is_unspecified();
    (!joined && enough && digit && !plain).then_some((start, end))
}

/// IPv6 addresses (see [`ipv6_at`]). A colon or a word glued before the address
/// (`ip:2001:db8::1`, `Address:2001:…`) and a colon after it (`…:1: error`) belong to the text.
fn replace_ipv6(text: &str, pattern: &Regex) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for found in pattern.find_iter(text) {
        let (start, end) = (found.start(), found.end());
        let mut tries = vec![(start, end)];
        if let Some(colon) = text[start..end].find(':').filter(|at| !text[start + at..end].starts_with("::")) {
            tries.push((start + colon + 1, end));
        }
        for (from, to) in tries.clone() {
            if text[from..to].ends_with(':') && !text[from..to].ends_with("::") {
                tries.push((from, to - 1));
            }
        }
        let Some((from, to)) = tries.into_iter().find_map(|(from, to)| ipv6_at(text, from, to)) else { continue };
        out.push_str(&text[last..from]);
        out.push_str("<IP>");
        last = to;
    }
    out.push_str(&text[last..]);
    out
}

/// The three spellings of a path: as it is, with `/`, with `\`.
fn variants(path: &str) -> [String; 3] {
    [path.to_string(), path.replace('\\', "/"), path.replace('/', "\\")]
}

fn ignoring_case(needle: &str) -> Option<Regex> {
    Regex::new(&format!("(?i){}", regex::escape(needle))).ok()
}

/// The patterns for what is replaced ignoring case, each with its mark.
type CaseRules = Vec<(Regex, &'static str)>;

/// Redacts texts for one machine. The Documents folder is looked up when it is
/// built (Python caches it the same way); the handles the program knows are
/// read on the first use, so a redactor built at the start of the program
/// knows those of then. One that has to know later ones is built anew.
pub struct Redactor {
    host: Host,
    home: String,
    name: String,
    private: Vec<(String, &'static str)>,
    /// Names given on top of the known handles (the reporter).
    extra: Vec<String>,
    /// Compiled on the first use: a report redacts a hundred texts, and
    /// compiling the same patterns for each of them took most of its time.
    rules: OnceLock<Option<CaseRules>>,
    /// The names that stand alone in a text: player handles and the computer, longest first.
    word_rules: OnceLock<Option<CaseRules>>,
}

impl Redactor {
    pub fn new(paths: &Paths) -> Redactor {
        let host = paths.host.clone();
        let flavor = host.flavor;
        let home = host.home();
        let name = flavor.basename(home.trim_end_matches(['\\', '/']));
        Redactor {
            private: private_folders(paths, &home),
            host,
            home,
            name,
            extra: Vec::new(),
            rules: OnceLock::new(),
            word_rules: OnceLock::new(),
        }
    }

    /// The same redactor, which also replaces `names` (each as a whole, and each of its words of
    /// three characters or more) as a player.
    pub fn with_names(&self, names: &[&str]) -> Redactor {
        let mut extra = self.extra.clone();
        for name in names {
            let name = crate::text::py_strip(name);
            extra.push(name.to_string());
            extra.extend(name.split_whitespace().map(str::to_string));
        }
        Redactor {
            host: self.host.clone(),
            home: self.home.clone(),
            name: self.name.clone(),
            private: self.private.clone(),
            extra,
            rules: OnceLock::new(),
            word_rules: OnceLock::new(),
        }
    }

    /// The text without personal data, or [`REDACT_FAILED`].
    pub fn redact(&self, text: &str) -> String {
        self.try_redact(text).unwrap_or_else(|| REDACT_FAILED.to_string())
    }

    /// The folders outside the home folder and the user name, in this order;
    /// `None` if one pattern cannot be built (then nothing is let through).
    fn rules(&self) -> Option<&CaseRules> {
        self.rules
            .get_or_init(|| {
                let mut rules = Vec::new();
                for (folder, mark) in &self.private {
                    for variant in variants(folder) {
                        if variant.chars().count() > 3 {
                            rules.push((ignoring_case(&variant)?, *mark));
                        }
                    }
                }
                if self.name.chars().count() > 2 {
                    rules.push((ignoring_case(&self.name)?, "<benutzer>"));
                }
                Some(rules)
            })
            .as_ref()
    }

    /// The handles the program knows, the names given and the computer name, longest first.
    fn word_rules(&self) -> Option<&CaseRules> {
        self.word_rules
            .get_or_init(|| {
                let paths = Paths::new(self.host.clone());
                let mut handles = crate::logsource::known_accounts(&paths);
                if let Some(reporter) = paths.setting("melder_name") {
                    handles.extend(reporter.split_whitespace().map(str::to_string));
                    handles.push(reporter);
                }
                handles.extend(self.extra.iter().cloned());
                let mut words: Vec<(String, &'static str)> = Vec::new();
                let known = |words: &Vec<(String, &str)>, w: &str| words.iter().any(|(k, _)| k.eq_ignore_ascii_case(w));
                for handle in handles {
                    if handle.chars().count() >= 3 && !known(&words, &handle) {
                        words.push((handle, "<SPIELER>"));
                    }
                }
                for computer in computer_names(&self.host) {
                    if !known(&words, &computer) {
                        words.push((computer, "<RECHNER>"));
                    }
                }
                // The longest first, so that a longer name is replaced whole.
                words.sort_by_key(|(word, _)| std::cmp::Reverse(word.chars().count()));
                words.iter().map(|(word, mark)| Some((ignoring_case(word)?, *mark))).collect()
            })
            .as_ref()
    }

    fn try_redact(&self, text: &str) -> Option<String> {
        let mut text = text.to_string();
        for variant in variants(&self.home) {
            if variant.chars().count() > 3 {
                text = text.replace(&variant, "<heim>");
            }
        }
        for (rule, mark) in self.rules()? {
            text = rule.replace_all(&text, NoExpand(mark)).into_owned();
        }
        let text = self.redact_secrets(text)?;
        self.redact_personal(text)
    }

    /// What the game's log and the system carry about the player: mail, handles, player
    /// numbers, numbers of places, the computer, IP addresses.
    fn redact_personal(&self, text: String) -> Option<String> {
        let p = patterns()?;
        let mut text = p.mail.replace_all(&text, NoExpand("<MAIL>")).into_owned();
        // A handle the log names in this very text, also one the program does not know yet.
        let mut named: Vec<String> = Vec::new();
        for found in p.handle_in_log.captures_iter(&text) {
            if let Some(name) = (1..=3).find_map(|i| found.get(i)).map(|m| m.as_str())
                && !named.iter().any(|n| n.eq_ignore_ascii_case(name))
            {
                named.push(name.to_string());
            }
        }
        named.sort_by_key(|name| std::cmp::Reverse(name.chars().count()));
        for name in &named {
            text = replace_word(&text, &ignoring_case(name)?, "<SPIELER>");
        }
        for (rule, mark) in self.word_rules()? {
            text = replace_word(&text, rule, mark);
        }
        let text = p.player_number.replace_all(&text, "${1}<SPIELERNR>]");
        let text = p.player_key.replace_all(&text, "${1}${2}<SPIELERNR>");
        let text = p.shop_key.replace_all(&text, |c: &regex::Captures| {
            if c[3].bytes().any(|b| b.is_ascii_digit()) {
                format!("{}{}<KENNUNG>", &c[1], &c[2])
            } else {
                c[0].to_string()
            }
        });
        let text = replace_ipv4(&text, &p.ipv4);
        Some(replace_ipv6(&text, &p.ipv6))
    }

    fn redact_secrets(&self, mut text: String) -> Option<String> {
        // The own report address first, whatever it looks like.
        let target = match self.host.var(crate::legacy::ENV_REPORT_TARGET) {
            Some(value) => crate::text::py_strip(value).to_string(),
            None => RELAY.to_string(),
        };
        if target.chars().count() > 12 {
            text = text.replace(&target, "<meldeadresse>");
        }
        let p = patterns()?;
        let text = p.webhook.replace_all(&text, NoExpand("<meldeadresse>"));
        let text = p.auth_header.replace_all(&text, "${1}<geheim>");
        let text = p.jwt.replace_all(&text, NoExpand("<geheim>"));
        let text = p.form_secret.replace_all(&text, "${1}<geheim>");
        Some(p.credential.replace_all(&text, "${1}<geheim>").into_owned())
    }
}

/// The Documents folder and its parent when they lie outside the home folder
/// (each with its mark, longest first). The parent only counts if it is no
/// drive and no collective folder like `Users`.
fn private_folders(paths: &Paths, home: &str) -> Vec<(String, &'static str)> {
    let flavor = paths.host.flavor;
    let home = flavor.normcase(&flavor.normpath(home));
    let docs = flavor.normpath(&paths.documents());
    let inside = flavor.normcase(&docs);
    let mut found = Vec::new();
    if inside != home && !inside.starts_with(&format!("{home}{}", flavor.sep())) {
        let parent = flavor.dirname(&docs);
        found.push((docs, "<dokumente>"));
        let drive_root = flavor.dirname(&parent) == parent;
        let generic = GENERIC_PARENTS.contains(&flavor.basename(&parent).to_lowercase().as_str());
        if !parent.is_empty() && !drive_root && !generic {
            found.push((parent, "<eigener-ordner>"));
        }
    }
    found
}

/// The name of the computer (`COMPUTERNAME`, `HOSTNAME`, on Unix also the system's, but not
/// in the checks, which run on made-up computers), and its first part when it has dots.
fn computer_names(host: &Host) -> Vec<String> {
    #[cfg(all(unix, not(test)))]
    let system = system_host_name();
    #[cfg(any(not(unix), test))]
    let system: Option<String> = None;
    let names: Vec<String> =
        ["COMPUTERNAME", "HOSTNAME"].iter().filter_map(|v| host.var_set(v)).map(str::to_string).chain(system).collect();
    let mut found: Vec<String> = Vec::new();
    for name in names {
        let name = name.trim();
        let short = name.split('.').next().unwrap_or_default();
        for candidate in [name, short] {
            let plain = PLAIN_HOSTS.iter().any(|p| p.eq_ignore_ascii_case(candidate));
            if candidate.chars().count() >= 3 && !plain && !found.iter().any(|f| f.eq_ignore_ascii_case(candidate)) {
                found.push(candidate.to_string());
            }
        }
    }
    found
}

#[cfg(all(unix, not(test)))]
fn system_host_name() -> Option<String> {
    let mut buffer = [0u8; 256];
    // SAFETY: gethostname writes at most `len` bytes into the buffer passed.
    let result = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
    if result != 0 {
        return None;
    }
    let end = buffer.iter().position(|b| *b == 0).unwrap_or(buffer.len());
    String::from_utf8(buffer[..end].to_vec()).ok()
}

/// Redacts with the paths of the running program.
pub fn redact(text: &str) -> String {
    static GLOBAL: OnceLock<Redactor> = OnceLock::new();
    GLOBAL.get_or_init(|| Redactor::new(paths::global())).redact(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pypath::Flavor;

    fn redactor(home: &str, vars: &[(&str, &str)]) -> Redactor {
        let mut all = vec![("HOME", home)];
        all.extend_from_slice(vars);
        Redactor::new(&Paths::new(Host::new(Flavor::Posix, &all, "/")))
    }

    #[test]
    fn home_and_name() {
        let r = redactor("/home/spieler", &[]);
        assert_eq!(r.redact("/home/spieler/.config/x und SPIELER"), "<heim>/.config/x und <benutzer>");
        assert_eq!(r.redact("D:\\Games\\StarCitizen"), "D:\\Games\\StarCitizen");
        // Names of up to two characters are not replaced (too many false hits).
        assert_eq!(redactor("/home/ab", &[]).redact("ab cd"), "ab cd");
    }

    #[test]
    fn secrets() {
        let r = redactor("/home/spieler", &[]);
        assert_eq!(r.redact("https://discord.com/api/webhooks/123/abc x"), "<meldeadresse> x"); // privacy-ok: made-up webhook
        assert_eq!(r.redact("GET https://x.de/a?id=3&token=geheim&b=1"), "GET https://x.de/a?id=3&token=<geheim>&b=1");
        assert_eq!(r.redact("Authorization: Bearer abc.def"), "Authorization: <geheim>");
        assert_eq!(r.redact("DPoP-Nonce: n1"), "DPoP-Nonce: <geheim>");
        assert_eq!(r.redact("t eyJhbGciOi.eyJzdWIiOi.sig"), "t <geheim>");
        assert_eq!(r.redact("refresh_token=xyz&a=1"), "refresh_token=<geheim>&a=1");
        assert_eq!(r.redact("an https://bericht.xharig.com/bericht"), "an <meldeadresse>");
        let own = redactor("/home/spieler", &[("SC_BP_BERICHT_ZIEL", " https://example.org/hook ")]);
        assert_eq!(own.redact("an https://example.org/hook!"), "an <meldeadresse>!");
    }

    /// A data folder of its own with the settings and the account files given.
    fn redactor_with(name: &str, files: &[(&str, &str)], vars: &[(&str, &str)]) -> (Redactor, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("versekit-schwaerzen-{name}-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (file, text) in files {
            std::fs::write(dir.join(file), text).unwrap();
        }
        let data = dir.to_string_lossy().into_owned();
        let mut all = vec![("HOME", "/home/spieler"), ("SC_BP_HOME", data.as_str())];
        all.extend_from_slice(vars);
        (Redactor::new(&Paths::new(Host::new(Flavor::native(), &all, "/"))), dir)
    }

    #[test]
    fn the_handles_the_program_knows_are_replaced_where_they_stand_alone() {
        let settings = r#"{"eigener_account": "Star_Pilot-7", "melder_name": "Meldewesen"}"#;
        let logs = r#"{"/x/Game.log": [1, 2, "Zweitkonto"], "/x/b.log": [1, 2, "ab"]}"#;
        let state = r#"{"sicherungen": {"a.log": {"account": "AltAccount"}, "b.log": {}}}"#;
        let (r, dir) = redactor_with(
            "handles",
            &[("einstellungen.json", settings), ("konten-je-log.json", logs), ("logstand.json", state)],
            &[],
        );
        assert_eq!(
            r.redact("Handle[Star_Pilot-7] und star_pilot-7, zweitkonto, AltAccount; Meldewesen schreibt"),
            "Handle[<SPIELER>] und <SPIELER>, <SPIELER>, <SPIELER>; <SPIELER> schreibt"
        );
        // Inside a longer name or word it stays; so does a name under three characters.
        assert_eq!(r.redact("Star_Pilot-77 Meldewesens ab"), "Star_Pilot-77 Meldewesens ab");
        let _ = std::fs::remove_dir_all(dir);
        let (all, dir) = redactor_with("alle", &[("einstellungen.json", r#"{"eigener_account": "*"}"#)], &[]);
        assert_eq!(all.redact("a * b"), "a * b", "all accounts is no handle");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_player_number_goes_and_the_number_of_a_thing_stays() {
        let (r, dir) = redactor_with("geid", &[("einstellungen.json", r#"{"eigener_account": "Pilot_X"}"#)], &[]);
        let line = "<Kill> 'Harvestable_Oza_123' [123] killed for 'Gegner' [2001234567] by 'Pilot_X' [2007654321]";
        assert_eq!(
            r.redact(line),
            "<Kill> 'Harvestable_Oza_123' [123] killed for 'Gegner' [<SPIELERNR>] by '<SPIELER>' [<SPIELERNR>]"
        );
        assert_eq!(r.redact("for 'Some_Thing' [55]"), "for 'Some_Thing' [55]", "a name with an underscore is a thing");
        assert_eq!(
            r.redact("geid 200123 - accountId 45 - playerGEID=200124, \"playerId\":\"77\" geidx 3"),
            "geid <SPIELERNR> - accountId <SPIELERNR> - playerGEID=<SPIELERNR>, \"playerId\":\"<SPIELERNR>\" geidx 3"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn places_mail_and_the_computer_go() {
        let (r, dir) = redactor_with("rechner", &[], &[("COMPUTERNAME", "DESKTOP-K4F2"), ("HOSTNAME", "localhost")]);
        assert_eq!(
            r.redact(
                "shopId=1234 kioskId: \"ab-12\" shopId missing; mail an jemand.x+1@example.co.uk auf desktop-k4f2"
            ),
            "shopId=<KENNUNG> kioskId: \"<KENNUNG>\" shopId missing; mail an <MAIL> auf <RECHNER>"
        );
        assert_eq!(r.redact("localhost DESKTOP-K4F2X"), "localhost DESKTOP-K4F2X");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn ip_addresses_go_but_the_own_computer_and_version_numbers_stay() {
        let r = redactor("/home/spieler", &[]);
        assert_eq!(
            r.redact("von 203.0.113.20 und 10.0.0.1. an 127.0.0.1:8080, nicht 300.1.1.1, nicht 1.2.3.4.5"), // privacy-ok: documentation addresses
            "von <IP> und <IP>. an 127.0.0.1:8080, nicht 300.1.1.1, nicht 1.2.3.4.5"
        );
        assert_eq!(r.redact("Windows 10.0.26200.1 Treiber 31.0.15.3623"), "Windows 10.0.26200.1 Treiber 31.0.15.3623");
        assert_eq!(
            r.redact("[2001:db8:a1c0::1f]:443 fe80::1:2:3%eth0 ::1 um 12:30:00 in versekit_core::usage, cafe::bad"),
            "[<IP>]:443 <IP>%eth0 ::1 um 12:30:00 in versekit_core::usage, cafe::bad"
        );
        assert_eq!(r.redact("2001:db8:0:0:0:0:2:1: Fehler"), "<IP>: Fehler");
        assert_eq!(r.redact("zu 2001:db8:0:0:0:0:2:1."), "zu <IP>.");
        assert_eq!(r.redact("ip:2001:db8::1:2:3 Address:2001:db8::7"), "ip:<IP> Address:<IP>");
        assert_eq!(r.redact("fe80::1 und 2001:db8:1:: und 1::2"), "<IP> und <IP> und 1::2");
    }

    #[test]
    fn a_handle_the_log_names_goes_even_unknown() {
        let r = redactor("/home/spieler", &[]);
        assert_eq!(
            r.redact("User Login Success - Handle[Neu_Ling] ... Neu_Ling betritt; - name Fremder - state STATE_CURRENT, Fremder"),
            "User Login Success - Handle[<SPIELER>] ... <SPIELER> betritt; - name <SPIELER> - state STATE_CURRENT, <SPIELER>"
        );
        assert_eq!(r.redact("nickname=\"Gast-1\" sagt Gast-1"), "nickname=\"<SPIELER>\" sagt <SPIELER>");
    }

    #[test]
    fn the_numbers_of_both_sides_of_a_kill_go() {
        let r = redactor("/home/spieler", &[]);
        assert_eq!(
            r.redact("CActor::Kill: 'Opfer' [2001] in zone 'Zone_1' killed by 'Taeter' [2002] using 'gun_s1' [3003]"),
            "CActor::Kill: 'Opfer' [<SPIELERNR>] in zone 'Zone_1' killed by 'Taeter' [<SPIELERNR>] using 'gun_s1' [3003]"
        );
        assert_eq!(
            r.redact("player_geid=5 owner_geid[6] account_id: 7 accountGeid 8 user_id=9 id=10 geidx 3"),
            "player_geid=<SPIELERNR> owner_geid[<SPIELERNR>] account_id: <SPIELERNR> accountGeid <SPIELERNR> \
             user_id=<SPIELERNR> id=10 geidx 3"
        );
    }

    #[test]
    fn names_given_go_whole_and_word_by_word() {
        let r = redactor("/home/spieler", &[]).with_names(&["Kai Beispielmann", " "]);
        assert_eq!(r.redact("Kai Beispielmann, Beispielmann und Kai"), "<SPIELER>, <SPIELER> und <SPIELER>");
    }
}
