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

//! Two aids for a test against the real Basetool: the write guard and the request journal.
//!
//! * **Write guard** ([`refuses`], [`WriteBudget`]): a ceiling on the changes the whole run of the program may
//!   send (`VERSEKIT_BASETOOL_WRITE_MAX`). Reading always goes; of everything else only the three requests that
//!   change nothing of the player's data go (the name of the connection, the account check, resolving names).
//!   A send to a `…/changes` route goes while it carries no more changes than are left, and takes them from
//!   the budget; at `0` left none goes. Any other writing request is refused: the list of what may go is
//!   closed on purpose.
//! * **Journal** ([`Journal`]): one line per request in `basetool-anfragen.log` next to the error log
//!   (`VERSEKIT_BASETOOL_JOURNAL=1`): time, method, path without query, status, duration, bytes sent and
//!   received, whether the guard refused it and, for a send, the number of changes and what is left of the
//!   budget. ⚠ Never a header, a body, a token, an address with a query or an identifier: a path segment that
//!   looks like an identifier is written as `:id`.

use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::basetool::jose::UrlParts;
use crate::jsonio::{self, Newline, Value};
use crate::paths::Paths;

/// The journal file, in the folder of the error log.
pub const JOURNAL_FILE: &str = "basetool-anfragen.log";
/// The journal is cut to its younger half when it grows beyond this many bytes.
pub const JOURNAL_MAX_BYTES: u64 = 200 * 1024;

/// The writing requests that change nothing of the player's data and so pass the guard.
const HARMLESS: [&str; 3] = ["/me/installation", "/me/account-check", "/catalog/resolve"];

/// Whether `path` is a route that takes change sets.
fn is_send(method: &str, path: &str) -> bool {
    method.eq_ignore_ascii_case("POST") && path.ends_with("/changes")
}

/// The number of changes a send carries (`ops`); `None` for any other request. A send in another shape (no
/// body, no `ops` list) counts as one: what cannot be counted never goes for free.
pub fn change_count(method: &str, path: &str, body: Option<&Value>) -> Option<usize> {
    if !is_send(method, path) {
        return None;
    }
    Some(match body.and_then(|b| b.get("ops")) {
        Some(Value::Array(ops)) => ops.len(),
        _ => 1,
    })
}

/// Whether the guard with ceiling `max` refuses this request.
pub fn refuses(max: u32, method: &str, path: &str, body: Option<&Value>) -> bool {
    if method.eq_ignore_ascii_case("GET") || method.eq_ignore_ascii_case("HEAD") {
        return false;
    }
    match change_count(method, path, body) {
        Some(count) => max == 0 || count > max as usize,
        None => !(method.eq_ignore_ascii_case("POST") && HARMLESS.contains(&path)),
    }
}

/// The write guard of one run: the ceiling and the changes still left. A send that passes takes its changes,
/// sent or not (one that failed on the way may still have arrived).
pub struct WriteBudget {
    ceiling: u32,
    left: Mutex<u32>,
}

impl WriteBudget {
    pub fn new(ceiling: u32) -> WriteBudget {
        WriteBudget { ceiling, left: Mutex::new(ceiling) }
    }

    pub fn ceiling(&self) -> u32 {
        self.ceiling
    }

    fn left_now(&self) -> std::sync::MutexGuard<'_, u32> {
        self.left.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The changes still left.
    pub fn left(&self) -> u32 {
        *self.left_now()
    }

    /// Whether the guard refuses this request; a send it lets through takes its changes from the budget.
    pub fn take(&self, method: &str, path: &str, body: Option<&Value>) -> bool {
        let mut left = self.left_now();
        if refuses(*left, method, path, body) {
            return true;
        }
        if let Some(count) = change_count(method, path, body) {
            *left = left.saturating_sub(u32::try_from(count).unwrap_or(u32::MAX));
        }
        false
    }
}

/// The budget of this run for `ceiling`, shared by every connection the run makes: a new connection (after
/// "Disconnect", or for another data folder) counts on. Each ceiling has a budget of its own; the program has
/// one ceiling per run, only the checks use several side by side.
pub fn run_budget(ceiling: u32) -> Arc<WriteBudget> {
    static RUN: Mutex<Vec<Arc<WriteBudget>>> = Mutex::new(Vec::new());
    let mut run = RUN.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(budget) = run.iter().find(|b| b.ceiling == ceiling) {
        return budget.clone();
    }
    let budget = Arc::new(WriteBudget::new(ceiling));
    run.push(budget.clone());
    budget
}

/// One request as the journal notes it.
pub struct Entry<'a> {
    /// Seconds since 1970.
    pub time: f64,
    pub method: &'a str,
    /// An address or a path; only the path without query is kept.
    pub url: &'a str,
    pub status: u16,
    pub millis: u128,
    /// Bytes of the body sent and of the answer received; never the bodies themselves.
    pub sent: usize,
    pub received: usize,
    /// The guard refused it: it never left.
    pub refused: bool,
    pub changes: Option<usize>,
    /// What is left of the budget of the write guard after this send; `None` without a guard or for no send.
    pub left: Option<u32>,
}

/// A path segment that could be an identifier (a number mixed in, a long run of hex digits).
fn looks_like_id(segment: &str) -> bool {
    let has_digit = segment.bytes().any(|b| b.is_ascii_digit());
    (has_digit && segment.len() >= 8) || (segment.len() >= 16 && segment.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// The path of an address without host, query and identifiers; only plain characters stay.
pub fn clean_path(url: &str) -> String {
    let path = UrlParts::split(url).map(|p| p.path).unwrap_or_default();
    let cleaned: Vec<String> = path
        .split('/')
        .map(|segment| {
            if looks_like_id(segment) {
                return ":id".to_string();
            }
            segment
                .chars()
                .take(40)
                .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~' | ':') { c } else { '?' })
                .collect()
        })
        .collect();
    let joined = cleaned.join("/");
    if joined.is_empty() { "/".to_string() } else { joined }
}

fn stamp(time: f64) -> String {
    crate::formats::local_time(time as i64)
        .map(|t| crate::formats::strftime("%Y-%m-%d %H:%M:%S", &t))
        .unwrap_or_else(|| "0000-00-00 00:00:00".to_string())
}

/// The line of an entry (without line break).
pub fn line(entry: &Entry<'_>) -> String {
    let method: String = entry.method.chars().filter(char::is_ascii_alphabetic).take(8).collect();
    let mut out = format!(
        "{} {} {} status={} dauer_ms={} gesendet_b={} empfangen_b={} abgewiesen={}",
        stamp(entry.time),
        method,
        clean_path(entry.url),
        entry.status,
        entry.millis,
        entry.sent,
        entry.received,
        if entry.refused { "ja" } else { "nein" },
    );
    if let Some(count) = entry.changes {
        out.push_str(&format!(" aenderungen={count}"));
    }
    if let Some(left) = entry.left {
        out.push_str(&format!(" rest={left}"));
    }
    out
}

/// The request journal: appends one line per request, keeps the file small.
pub struct Journal {
    file: PathBuf,
    lock: Mutex<()>,
}

impl Journal {
    /// A journal in `file`.
    pub fn at(file: PathBuf) -> Journal {
        Journal { file, lock: Mutex::new(()) }
    }

    /// The journal of these paths if `VERSEKIT_BASETOOL_JOURNAL` is `1`, else none (and no file).
    pub fn from_env(paths: &Paths) -> Option<Journal> {
        if !crate::testmode::basetool_journal_on(&paths.host) {
            return None;
        }
        // Next to the error log, wherever the data folder sorts its diagnostics.
        let log = paths.app_file(crate::error_log::FILE);
        let folder = std::path::Path::new(&log).parent()?;
        Some(Journal::at(folder.join(JOURNAL_FILE)))
    }

    pub fn file(&self) -> &std::path::Path {
        &self.file
    }

    /// Cuts the file to its younger half, at a line start.
    fn trim(&self) {
        let Ok(meta) = std::fs::metadata(&self.file) else { return };
        if meta.len() <= JOURNAL_MAX_BYTES {
            return;
        }
        let Ok(bytes) = std::fs::read(&self.file) else { return };
        let keep = &bytes[bytes.len() - (JOURNAL_MAX_BYTES / 2) as usize..];
        let start = keep.iter().position(|b| *b == b'\n').map_or(0, |i| i + 1);
        let text = String::from_utf8_lossy(&keep[start..]).into_owned();
        let _ = jsonio::write_text_atomic(&self.file.to_string_lossy(), &text, Newline::Keep);
    }

    /// Notes one request; a journal that cannot be written is silent (it must never stop a request).
    pub fn note(&self, entry: &Entry<'_>) {
        let _guard = self.lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let path = self.file.to_string_lossy().into_owned();
        if crate::writeguard::check("append", &path).is_err() {
            return;
        }
        if let Some(folder) = self.file.parent() {
            let _ = jsonio::make_dirs(&folder.to_string_lossy());
        }
        self.trim();
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&self.file) {
            let _ = writeln!(file, "{}", line(entry));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ops(n: usize) -> Value {
        Value::Object(vec![("ops".into(), Value::Array((0..n).map(|i| Value::str(format!("op{i}"))).collect()))])
    }

    #[test]
    fn the_guard_counts_the_changes_of_a_send() {
        let route = "/me/blueprints/changes";
        assert!(!refuses(1, "POST", route, Some(&ops(1))), "one change under ceiling 1");
        assert!(refuses(1, "POST", route, Some(&ops(2))), "two changes over ceiling 1");
        assert!(!refuses(5, "POST", "/org-stock/changes", Some(&ops(5))));
        assert!(refuses(5, "POST", "/org-stock/changes", Some(&ops(6))));
        // Ceiling 0 fetches only: not even an empty send goes.
        assert!(refuses(0, "POST", route, Some(&ops(1))));
        assert!(refuses(0, "POST", route, Some(&ops(0))));
        // A send without a readable `ops` counts as one: never at 0, and it uses up a budget like any change.
        assert!(refuses(0, "POST", route, None));
        assert!(!refuses(1, "POST", route, None));
        let other_shape = Value::Object(vec![("aenderungen".into(), Value::Array(Vec::new()))]);
        assert_eq!(change_count("POST", route, Some(&other_shape)), Some(1));
        assert_eq!(change_count("POST", route, None), Some(1));
        let budget = WriteBudget::new(1);
        assert!(!budget.take("POST", route, None), "the first one goes");
        assert!(budget.take("POST", route, Some(&other_shape)), "and with it the budget is used up");
    }

    #[test]
    fn the_guard_lets_reading_and_the_three_harmless_requests_pass() {
        for max in [0, 1] {
            assert!(!refuses(max, "GET", "/me/blueprints", None));
            assert!(!refuses(max, "get", "/me/stock/changes", None), "a read of a route that takes changes");
            for path in HARMLESS {
                assert!(!refuses(max, "POST", path, Some(&Value::Object(Vec::new()))), "{path}");
            }
        }
    }

    #[test]
    fn the_guard_refuses_every_other_writing_request() {
        for (method, path) in [
            ("POST", "/me/something-new"),
            ("PUT", "/me/installation"),
            ("DELETE", "/me/blueprints"),
            ("PATCH", "/me/stock"),
            ("POST", "/me/installation/x"),
        ] {
            assert!(refuses(1000, method, path, None), "{method} {path}");
        }
    }

    #[test]
    fn the_budget_of_a_run_counts_down_over_every_send() {
        let route = "/me/blueprints/changes";
        let budget = WriteBudget::new(2);
        assert!(!budget.take("POST", route, Some(&ops(1))), "the first change goes");
        assert_eq!(budget.left(), 1);
        assert!(budget.take("POST", "/me/stock/changes", Some(&ops(2))), "two changes, one left: refused whole");
        assert_eq!(budget.left(), 1, "a refused send takes nothing");
        assert!(!budget.take("POST", "/me/ships/changes", Some(&ops(1))), "another route draws on the same budget");
        assert_eq!(budget.left(), 0);
        assert!(budget.take("POST", route, Some(&ops(1))), "used up: no send goes");
        assert!(budget.take("POST", route, Some(&ops(0))), "used up: not even an empty one");
        // Reading and the harmless writes take nothing and still go.
        assert!(!budget.take("GET", "/me/blueprints", None));
        assert!(!budget.take("POST", "/me/installation", Some(&Value::Object(Vec::new()))));
        assert_eq!((budget.ceiling(), budget.left()), (2, 0));
    }

    #[test]
    fn every_connection_of_a_run_shares_one_budget() {
        // An odd ceiling no other check uses, so the budget of the run is this test's alone.
        let first = run_budget(9_101);
        assert!(!first.take("POST", "/me/blueprints/changes", Some(&ops(1))));
        let again = run_budget(9_101);
        assert!(Arc::ptr_eq(&first, &again), "a new connection counts on");
        assert_eq!(again.left(), 9_100);
        // Counter-check: another ceiling starts afresh, and the first one keeps counting.
        assert_eq!(run_budget(9_102).left(), 9_102);
        assert_eq!(run_budget(9_101).left(), 9_100, "another ceiling in between does not reset it");
    }

    #[test]
    fn a_path_loses_its_query_its_host_and_its_identifiers() {
        assert_eq!(
            clean_path("https://api.example.test/v1/me/blueprints?cursor=GEHEIM&limit=500"),
            "/v1/me/blueprints"
        );
        assert_eq!(
            clean_path("https://x.test/org-stock/units/3f2a9c1e-77b0-4c1d-9a55-0123456789ab/members"),
            "/org-stock/units/:id/members"
        );
        assert_eq!(clean_path("https://x.test/.well-known/openid-configuration"), "/.well-known/openid-configuration");
        // A line break in an address is dropped by the parser, a space becomes `?`: never a second line.
        assert_eq!(clean_path("https://x.test/a b\nc"), "/a?bc");
        assert_eq!(clean_path("https://x.test/a%0Ab"), "/a?0Ab");
        assert_eq!(clean_path("https://x.test"), "/");
    }

    #[test]
    fn a_line_holds_only_the_allowed_facts() {
        let entry = Entry {
            time: 0.0,
            method: "POST",
            url: "https://x.test/me/blueprints/changes?idempotency=SECRET",
            status: 200,
            millis: 143,
            sent: 87,
            received: 412,
            refused: false,
            changes: Some(1),
            left: Some(0),
        };
        let text = line(&entry);
        assert!(
            text.ends_with(
                " POST /me/blueprints/changes status=200 dauer_ms=143 gesendet_b=87 empfangen_b=412 abgewiesen=nein \
                 aenderungen=1 rest=0"
            ),
            "{text}"
        );
        assert!(!text.contains("SECRET") && !text.contains('?'));
        let refused = Entry { refused: true, status: 0, sent: 0, received: 0, changes: Some(3), ..entry };
        assert!(
            line(&refused).contains("status=0 dauer_ms=143 gesendet_b=0 empfangen_b=0 abgewiesen=ja aenderungen=3")
        );
        let read = Entry { changes: None, left: None, ..entry };
        assert!(!line(&read).contains("aenderungen") && !line(&read).contains("rest="), "{}", line(&read));
    }

    #[test]
    fn the_journal_appends_and_cuts_its_old_end() {
        let dir = crate::testdir::fresh("basetool-journal-kappen");
        let journal = Journal::at(dir.join("Diagnose").join(JOURNAL_FILE));
        let entry = Entry {
            time: 0.0,
            method: "GET",
            url: "https://x.test/me/stock",
            status: 200,
            millis: 1,
            sent: 0,
            received: 10,
            refused: false,
            changes: None,
            left: None,
        };
        journal.note(&entry);
        journal.note(&entry);
        let first = std::fs::read_to_string(journal.file()).expect("written");
        assert_eq!(first.lines().count(), 2);
        // Fill beyond the limit with numbered lines: the oldest go, the newest stay, nothing is cut in a line.
        let mut big = String::new();
        let mut number = 0;
        while (big.len() as u64) < JOURNAL_MAX_BYTES + 5000 {
            big.push_str(&format!("{number:08} GET /me/stock status=200 dauer_ms=1 abgewiesen=nein\n"));
            number += 1;
        }
        std::fs::write(journal.file(), &big).expect("filled");
        journal.note(&entry);
        let after = std::fs::read_to_string(journal.file()).expect("cut");
        assert!((after.len() as u64) < JOURNAL_MAX_BYTES, "cut to the younger half: {}", after.len());
        assert!(after.lines().all(|l| l.contains("status=")), "no half line stays");
        assert!(!after.contains("00000000 GET"), "the oldest line went");
        assert!(after.contains(&format!("{:08} GET", number - 1)), "the newest stayed");
        let _ = std::fs::remove_dir_all(dir);
    }
}
