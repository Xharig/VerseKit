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

//! The error report as text (`report.build`): what the player pastes into an
//! issue or sends, so the questions that otherwise have to be asked one by
//! one are already answered.
//!
//! This module sets the report together from [`Facts`]; what goes into them
//! is gathered by [`crate::report_facts`]. Rules that hold for every line:
//!
//! 1. No user name, no ids: what comes from outside (values of the facts, the
//!    message, place and message of an error, call sites, the dump of a
//!    crash, update lines) goes through [`Redactor`]. A name stands only at
//!    the top under "Von" ("From"), as in the field of the page. Unlike 4.12
//!    the two trails go through it too: the program writes them itself, but
//!    a step may name a handle or an address of the game's log.
//! 2. Nothing is sent from here: the text is returned, nothing more.
//! 3. The report never fails: a fact that cannot be found shows as a dash.
//!
//! ## Layout
//!
//! The report is read in public issues, so it is laid out from top to bottom
//! in a fixed order of sections, each under a `## heading`, its lines
//! indented by two spaces and its values in one column:
//!
//! ```text
//! VerseKit 5.0.0 · Bericht vom …           title
//! Von            name                      who sends it
//! ## Was passiert ist                      what the player wrote
//! ## System / Spiel / Bestand / Ordner …   the blocks of facts
//! ## Startverlauf / Zuletzt geöffnete …    the two trails
//! ## Startverlauf · Overlay                 the trail of each other process given
//! ## Letzter Update-Versuch                 only if there was one
//! ## Harter Abbruch                         only if there was one
//! ## Harter Abbruch · Overlay               the same for each other process
//! ## Letzte Fehler                           running version first, then older
//! ## Schwärzung                              what was left out
//! ```
//!
//! This is a deliberate departure from 4.12, where the sections ran into
//! each other (`tools/parity_report.py` compares the content, not the lines).

use std::sync::OnceLock;

use regex::Regex;

use crate::formats;
use crate::jsonio::{self, Value};
use crate::language::{Arg, Lang, tf_in};
use crate::redact::Redactor;
use crate::text::{PY_SPACE, py_isspace, py_repr_str, py_splitlines, py_strip};

/// So many characters wide a line of the report is at most; longer texts are
/// wrapped. A single word that is longer on its own is cut at a separator in
/// the errors and the crash; in the message and the facts (paths) it stays whole.
pub const LINE_WIDTH: usize = 88;
/// The value column starts at least here; a longer label pushes it right for
/// the whole report, so all values stand in one column.
pub const COLUMN_MIN: usize = 20;
/// Every line under a heading is indented by this many spaces.
const INDENT: usize = 2;
/// So many call sites per error of the running version.
pub const TRACE_SHOWN: usize = 4;
/// So many characters of the code line of a call site.
const TRACE_CODE: usize = 90;
/// How many lines of the start trail and of the page trail the report shows.
const START_SHOWN: usize = 12;
const PAGES_SHOWN: usize = 24;
/// How many lines of the crash the report shows.
pub const CRASH_SHOWN: usize = 14;
/// How many errors the report lists.
pub const ERRORS_SHOWN: usize = 8;

/// One line of the facts block: label and value. No value or an empty one
/// shows as a dash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub label: String,
    pub value: Option<String>,
}

impl Entry {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Entry {
        Entry { label: label.into(), value: Some(value.into()) }
    }

    pub fn missing(label: impl Into<String>) -> Entry {
        Entry { label: label.into(), value: None }
    }
}

/// One section of facts: a heading (already in the language of the report)
/// and its lines. A block without lines is left out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub title: String,
    pub entries: Vec<Entry>,
}

impl Block {
    pub fn new(title: impl Into<String>, entries: Vec<Entry>) -> Block {
        Block { title: title.into(), entries }
    }
}

/// Everything the report is made of.
#[derive(Clone, Debug)]
pub struct Facts {
    pub lang: Lang,
    pub version: String,
    /// The date of the report, already written with `b_datum`.
    pub now: String,
    /// Who sends it; shown unchanged under "Von".
    pub reporter: String,
    /// What the player wrote; may be empty.
    pub message: String,
    /// The sections of facts, each under a heading of its own.
    pub blocks: Vec<Block>,
    /// The start trail up to the boundary, and the page trail after it.
    pub trail_start: Vec<String>,
    pub trail_pages: Vec<String>,
    /// What the last update attempt left behind, line by line.
    pub update_lines: Vec<String>,
    /// The hard crash of the last run, line by line, with its time and version.
    pub crash: Vec<String>,
    pub crash_at: Option<f64>,
    pub crash_version: String,
    /// The other processes of the program (the overlay, the teaching window).
    pub others: Vec<OtherRun>,
    /// The newest errors, newest first, and how many are kept in all.
    pub errors: Vec<Value>,
    pub error_total: usize,
}

/// What another process of the program left behind: its name for the
/// headings, its hard crash of the last run and its trail. Empty parts are
/// left out of the report.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OtherRun {
    pub name: String,
    pub crash: Vec<String>,
    pub crash_at: Option<f64>,
    pub crash_version: String,
    pub trail: Vec<String>,
}

fn text(lang: Lang, key: &str) -> String {
    crate::language::t_in(lang, key)
}

fn filled(lang: Lang, key: &str, args: &[(&str, Arg)]) -> String {
    tf_in(lang, key, args)
}

/// Python's `str()` of a JSON value, as `%s` writes it.
pub fn py_text(value: &Value) -> String {
    match value {
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Int(n) => n.clone(),
        Value::Float(x) => jsonio::float_text(*x),
        Value::Str(s) => s.clone(),
        other => py_repr(other),
    }
}

/// Python's `repr()` of a JSON value.
pub fn py_repr(value: &Value) -> String {
    match value {
        Value::Str(s) => py_repr_str(s),
        Value::Array(items) => format!("[{}]", items.iter().map(py_repr).collect::<Vec<_>>().join(", ")),
        Value::Object(map) => format!(
            "{{{}}}",
            map.iter().map(|(k, v)| format!("{}: {}", py_repr_str(k), py_repr(v))).collect::<Vec<_>>().join(", ")
        ),
        other => py_text(other),
    }
}

/// Python's truth value of a JSON value.
pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Int(n) => n != "0",
        Value::Float(x) => *x != 0.0,
        Value::Str(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(m) => !m.is_empty(),
    }
}

/// `str.ljust(width)`: padded with spaces to `width` characters.
fn ljust(text: &str, width: usize) -> String {
    let count = text.chars().count();
    format!("{text}{}", " ".repeat(width.saturating_sub(count)))
}

/// Characters after which a word that is too long may be cut.
const BREAK_AFTER: [char; 8] = ['/', '\\', ':', ',', ';', '-', '_', ')'];

/// Where a word may be cut at `at` (the piece ends before it): after one of
/// [`BREAK_AFTER`], and before the `.!` that starts a part of a widget path
/// of the interface (`.!frame4.!canvas`), so no piece starts with a lone `!`.
fn may_cut_at(chars: &[char], at: usize) -> bool {
    (at > 0 && BREAK_AFTER.contains(&chars[at - 1])) || (chars[at] == '.' && chars.get(at + 1) == Some(&'!'))
}

/// A word cut into pieces of at most `width` characters: at the last place
/// that may be cut in the second half of a piece, else hard.
fn split_long(word: &str, width: usize) -> Vec<String> {
    let width = width.max(8);
    let mut rest: Vec<char> = word.chars().collect();
    let mut pieces = Vec::new();
    while rest.len() > width {
        let cut = (width / 2..=width).rev().find(|&at| at < rest.len() && may_cut_at(&rest, at)).unwrap_or(width);
        pieces.push(rest[..cut].iter().collect());
        rest.drain(..cut);
    }
    if !rest.is_empty() {
        pieces.push(rest.iter().collect());
    }
    pieces
}

/// Like [`wrap`], but a single word longer than `width` (a widget path of
/// the interface, a path of files) is cut too, so no line sticks out.
pub fn wrap_hard(text: &str, width: usize) -> Vec<String> {
    wrap(text, width)
        .into_iter()
        .flat_map(|line| if line.chars().count() > width { split_long(&line, width) } else { vec![line] })
        .collect()
}

/// A text spread over several lines without cutting words.
///
/// ⚠ Not `textwrap`: a single line without a space (a path) stays whole.
/// Own line breaks stay: someone who types a list wants a list. Blank lines
/// separate paragraphs, but never more than one in a row.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    for paragraph in text.split('\n') {
        if py_strip(paragraph).is_empty() {
            if result.last().is_some_and(|last| !last.is_empty()) {
                result.push(String::new());
            }
            continue;
        }
        let mut running = String::new();
        let mut running_len = 0usize;
        for word in paragraph.split(py_isspace).filter(|w| !w.is_empty()) {
            let word_len = word.chars().count();
            if !running.is_empty() && running_len + 1 + word_len > width {
                result.push(std::mem::take(&mut running));
                running = word.to_string();
                running_len = word_len;
            } else if running.is_empty() {
                running = word.to_string();
                running_len = word_len;
            } else {
                running.push(' ');
                running.push_str(word);
                running_len += 1 + word_len;
            }
        }
        if !running.is_empty() {
            result.push(running);
        }
    }
    while result.last().is_some_and(String::is_empty) {
        result.pop();
    }
    if result.is_empty() {
        result.push(String::new());
    }
    result
}

/// Equal lines in a row as one line (`… (3×)`), the time of the first stays.
///
/// ⚠ Only what is equal **directly one after the other**; and the text
/// behind the time is compared, not the time.
pub fn dense(entries: &[String]) -> Vec<String> {
    let mut out: Vec<(String, String, usize)> = Vec::new();
    for entry in entries {
        let after = entry.split_once("  ").map_or(entry.as_str(), |(_, rest)| rest);
        let key = py_strip(after).to_string();
        match out.last_mut() {
            Some(last) if last.1 == key => last.2 += 1,
            _ => out.push((entry.clone(), key, 1)),
        }
    }
    out.into_iter().map(|(line, _, n)| if n == 1 { line } else { format!("{line}  ({n}×)") }).collect()
}

fn starts_thread(line: &str) -> bool {
    let lower = line.to_lowercase();
    lower.starts_with("thread ") || lower.starts_with("current thread")
}

/// From a hard crash the lines that say something: the head, then the block
/// of the guilty thread (`Current thread`), then the rest, cut at `limit`.
///
/// ⚠ Where the guilty thread stands in the dump is chance; the first lines
/// often show only threads that wait. A report that cuts the cause and shows
/// the bystanders sends the reader the wrong way.
pub fn crash_brief(lines: &[String], limit: usize) -> Vec<String> {
    let first = lines.iter().position(|l| starts_thread(l)).unwrap_or(lines.len());
    let place = lines.iter().position(|l| l.to_lowercase().starts_with("current thread"));
    let Some(place) = place else { return lines.iter().take(limit).cloned().collect() };
    let end = (place + 1..lines.len()).find(|&i| starts_thread(&lines[i])).unwrap_or(lines.len());
    let mut out: Vec<String> = lines[..first].to_vec();
    out.extend(lines[place..end].iter().cloned());
    out.extend(
        lines.iter().enumerate().filter(|(i, _)| *i >= first && !(place..end).contains(i)).map(|(_, l)| l.clone()),
    );
    out.truncate(limit);
    out
}

fn frame_pattern() -> Option<&'static Regex> {
    static PATTERN: OnceLock<Option<Regex>> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(&format!(r#"^{PY_SPACE}*File "(.+)", line (\d+), in (.+)$"#)).ok()).as_ref()
}

/// The call sites of a stored trace, one line each: from
/// `File ".../src/pages.py", line 12, in show` and the code line below it
/// `src/pages.py:12 in show: code`. Of the path the last two parts stay.
/// Sites in `tkinter` itself drop out while another is left. Every line goes
/// through the redactor once more.
pub fn trace_frames(trace: &str, redactor: &Redactor) -> Vec<String> {
    let Some(pattern) = frame_pattern() else { return Vec::new() };
    // [file, line number, function, code]
    let mut frames: Vec<[String; 4]> = Vec::new();
    for row in py_splitlines(trace) {
        if let Some(found) = pattern.captures(row) {
            let path = &found[1];
            let parts: Vec<&str> = path.split(['\\', '/']).filter(|p| !p.is_empty()).collect();
            let tail = &parts[parts.len().saturating_sub(2)..];
            frames.push([tail.join("/"), found[2].to_string(), py_strip(&found[3]).to_string(), String::new()]);
        } else if row.starts_with("    ")
            && !row.trim_matches([' ', '^', '~']).is_empty()
            && frames.last().is_some_and(|f| f[3].is_empty())
            && let Some(last) = frames.last_mut()
        {
            last[3] = py_strip(row).to_string();
        }
    }
    let own: Vec<&[String; 4]> =
        frames.iter().filter(|f| f[0].split('/').next().unwrap_or_default().to_lowercase() != "tkinter").collect();
    let chosen: Vec<&[String; 4]> = if own.is_empty() { frames.iter().collect() } else { own };
    chosen
        .into_iter()
        .map(|[file, number, function, code]| {
            let mut line = format!("{file}:{number} in {function}");
            if !code.is_empty() {
                line.push_str(": ");
                // ⚠ Redacted before it is cut: a cut through a path would
                // leave half of a name that no rule finds any more.
                let code = redactor.redact(code);
                if code.chars().count() <= TRACE_CODE {
                    line.push_str(&code);
                } else {
                    line.extend(code.chars().take(TRACE_CODE - 1));
                    line.push('…');
                }
            }
            redactor.redact(&line)
        })
        .collect()
}

fn field(entry: &Value, key: &str) -> Option<Value> {
    entry.get(key).cloned()
}

/// `e.get(key, default)` written with `%s`.
fn field_text(entry: &Value, key: &str, default: &str) -> String {
    field(entry, key).map_or_else(|| default.to_string(), |v| py_text(&v))
}

/// `int(e.get('anzahl') or 1)`; an unreadable count counts once.
fn times_of(entry: &Value) -> u64 {
    match entry.get("anzahl") {
        Some(Value::Int(n)) => n.parse().unwrap_or(1),
        Some(Value::Str(s)) => py_strip(s).replace('_', "").parse().unwrap_or(1),
        Some(Value::Float(x)) if *x >= 1.0 => *x as u64,
        _ => 1,
    }
    .max(1)
}

/// Equal errors one after the other as one entry:
/// (entry, how often, until when).
fn bundle(latest: &[Value]) -> Vec<(&Value, u64, String)> {
    let ident = |e: &Value| ["fassung", "stelle", "art", "meldung"].map(|k| e.get(k).cloned().unwrap_or(Value::Null));
    let mut bundled: Vec<([Value; 4], &Value, u64, String)> = Vec::new();
    for entry in latest {
        let id = ident(entry);
        let times = times_of(entry);
        let last = match entry.get("zuletzt") {
            Some(v) if truthy(v) => py_text(v),
            _ => field_text(entry, "zeit", "—"),
        };
        match bundled.last_mut() {
            Some(top) if top.0.iter().zip(&id).all(|(a, b)| jsonio::py_eq_value(a, b)) => {
                top.2 += times;
                top.3 = last;
            }
            _ => bundled.push((id, entry, times, last)),
        }
    }
    bundled.into_iter().map(|(_, e, n, last)| (e, n, last)).collect()
}

/// The lines of the report, and the column its values stand in.
struct Page {
    lines: Vec<String>,
    column: usize,
}

impl Page {
    fn new(column: usize) -> Page {
        Page { lines: Vec::new(), column }
    }

    /// One line; trailing spaces never stay.
    fn push(&mut self, line: impl Into<String>) {
        let line: String = line.into();
        self.lines.push(line.trim_end().to_string());
    }

    /// A blank line, unless there is one already or nothing yet.
    fn blank(&mut self) {
        if self.lines.last().is_some_and(|l| !l.is_empty()) {
            self.lines.push(String::new());
        }
    }

    /// A heading of `level` (2: section, 3: part of a section) after a blank line.
    fn heading(&mut self, level: usize, title: &str) {
        self.blank();
        self.push(format!("{} {title}", "#".repeat(level)));
    }

    /// Lines of text, each indented; an empty one stays empty.
    fn body(&mut self, indent: usize, pieces: &[String]) {
        for piece in pieces {
            if piece.is_empty() {
                self.push("");
            } else {
                self.push(format!("{}{piece}", " ".repeat(indent)));
            }
        }
    }

    /// How wide a value is that stands in the value column.
    fn room(&self) -> usize {
        LINE_WIDTH.saturating_sub(self.column).max(24)
    }

    /// A line `label  value` with the value in the value column; a value of
    /// several lines hangs under its first one.
    fn row(&mut self, indent: usize, label: &str, value: &[String]) {
        let width = self.column.saturating_sub(indent).max(label.chars().count() + 1);
        let hang = " ".repeat(indent + width);
        let first = value.first().map_or("", String::as_str);
        self.push(format!("{}{}{first}", " ".repeat(indent), ljust(label, width)));
        for piece in value.iter().skip(1) {
            self.push(if piece.is_empty() { String::new() } else { format!("{hang}{piece}") });
        }
    }
}

/// Where the values stand: right of the longest label of the whole report,
/// at the least at [`COLUMN_MIN`].
fn value_column(facts: &Facts) -> usize {
    let lang = facts.lang;
    let long = |indent: usize, label: &str| indent + label.chars().count() + 1;
    let mut column = COLUMN_MIN.max(long(0, &text(lang, "b_melder")));
    for entry in facts.blocks.iter().flat_map(|b| &b.entries) {
        column = column.max(long(INDENT, &entry.label));
    }
    for key in ["bf_c_wann", "bf_c_fassung", "bf_c_hinweis", "bf_c_befund", "bf_c_ablauf"] {
        column = column.max(long(INDENT, &text(lang, key)));
    }
    for key in
        ["bf_f_zeit", "bf_f_wiederholt", "bf_f_fassung", "bf_f_bereich", "bf_f_art", "bf_f_meldung", "bf_f_stelle"]
    {
        column = column.max(long(2 * INDENT, &text(lang, key)));
    }
    column
}

/// Which part of the error list an entry belongs to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Part {
    /// From the running version (or all, if the version is not known).
    Current,
    /// From another version.
    Older,
    /// No version written down.
    Unknown,
}

/// One error of the list, with what is shown under it.
struct Item<'a> {
    entry: &'a Value,
    times: u64,
    until: String,
    version: String,
    part: Part,
    /// The call sites shown with it (the newest entry of a place only).
    frames: Vec<String>,
    /// The earlier entry of the same place whose call sites stand for this one.
    same_as: Option<usize>,
}

/// The errors, in parts: the running version first, then the older ones.
///
/// ⚠ The mark "not from the running version" stands once above the older
/// ones, not on every line. It states, it does not judge: someone with an old
/// version may send an error nobody has seen yet.
fn errors_section(page: &mut Page, facts: &Facts, redactor: &Redactor) {
    let lang = facts.lang;
    if facts.errors.is_empty() {
        page.heading(2, &text(lang, "bf_h_fehler"));
        page.body(INDENT, &[text(lang, "bf_f_keine")]);
        return;
    }
    page.heading(
        2,
        &filled(lang, "b_fehler", &[("0", Arg::from(facts.errors.len())), ("1", Arg::from(facts.error_total))]),
    );
    let parted = !facts.version.is_empty();

    // The call sites go to the newest entry per place (as in 4.12): up to
    // `TRACE_SHOWN` of the running version, of another only the last.
    let mut items: Vec<Item> = Vec::new();
    let mut traced: Vec<(Value, usize)> = Vec::new();
    for (entry, times, until) in bundle(&facts.errors) {
        let version = match entry.get("fassung") {
            Some(v) if truthy(v) => py_text(v),
            _ => "?".to_string(),
        };
        let part = if !parted || version == facts.version {
            Part::Current
        } else if version == "?" {
            Part::Unknown
        } else {
            Part::Older
        };
        let place = entry.get("stelle").cloned().unwrap_or(Value::Null);
        let mut frames = Vec::new();
        let mut same_as = None;
        if let Some((_, at)) = traced.iter().find(|(p, _)| jsonio::py_eq_value(p, &place)) {
            same_as = Some(*at).filter(|at| !items[*at].frames.is_empty());
        } else {
            traced.push((place, items.len()));
            let trace = match entry.get("spur") {
                Some(v) if truthy(v) => py_text(v),
                _ => String::new(),
            };
            let all = trace_frames(&trace, redactor);
            let shown = if part == Part::Current { TRACE_SHOWN } else { 1 };
            frames = all[all.len().saturating_sub(shown)..].to_vec();
        }
        items.push(Item { entry, times, until, version, part, frames, same_as });
    }

    let parts: &[Part] = if parted { &[Part::Current, Part::Older, Part::Unknown] } else { &[Part::Current] };
    let members = |part: Part| (0..items.len()).filter(|&i| items[i].part == part).collect::<Vec<usize>>();
    // The number of an entry is its place in the list as shown.
    let mut number = vec![0usize; items.len()];
    for (shown, at) in parts.iter().flat_map(|&p| members(p)).enumerate() {
        number[at] = shown + 1;
    }

    for &part in parts {
        let these = members(part);
        if parted {
            if these.is_empty() && part != Part::Current {
                continue;
            }
            let (title, hint) = match part {
                Part::Current => (filled(lang, "bf_f_laufend", &[("0", Arg::Text(facts.version.clone()))]), None),
                Part::Older => (text(lang, "bf_f_aelter"), Some(text(lang, "bf_alt"))),
                Part::Unknown => (text(lang, "bf_f_unbekannt"), Some(text(lang, "bf_ohne_fassung"))),
            };
            page.heading(3, &title);
            if let Some(hint) = hint {
                page.body(INDENT, &wrap(&hint, LINE_WIDTH - INDENT));
            } else if these.is_empty() {
                page.body(INDENT, &wrap(&text(lang, "bf_f_laufend_keine"), LINE_WIDTH - INDENT));
            }
        }
        for at in these {
            error_block(page, lang, &items[at], number[at], &number, redactor);
        }
    }
}

/// One error as a block of rows: time, repetitions, version, area, kind,
/// message, call sites.
fn error_block(page: &mut Page, lang: Lang, item: &Item, number: usize, numbers: &[usize], redactor: &Redactor) {
    let width = page.room();
    // ⚠ Time and count are written by the program itself. Place, kind, version
    // and message may come from an older file or from outside (paths, widget
    // names, texts of the system): they go through the redactor once more,
    // though the log redacted place and message when it wrote them.
    let plain = |key: &str| wrap_hard(&field_text(item.entry, key, "—"), width);
    let shown = |key: &str| wrap_hard(&redactor.redact(&field_text(item.entry, key, "—")), width);
    page.blank();
    page.push(format!("{}{}", " ".repeat(INDENT), filled(lang, "bf_f_nr", &[("0", Arg::from(number))])));
    let at = 2 * INDENT;
    page.row(at, &text(lang, "bf_f_zeit"), &plain("zeit"));
    if item.times > 1 {
        let value = filled(
            lang,
            "bf_f_wert_wiederholt",
            &[("0", Arg::Int(i64::try_from(item.times).unwrap_or(i64::MAX))), ("1", Arg::Text(item.until.clone()))],
        );
        page.row(at, &text(lang, "bf_f_wiederholt"), &wrap_hard(&value, width));
    }
    page.row(at, &text(lang, "bf_f_fassung"), &wrap_hard(&redactor.redact(&item.version), width));
    page.row(at, &text(lang, "bf_f_bereich"), &shown("stelle"));
    page.row(at, &text(lang, "bf_f_art"), &shown("art"));
    page.row(at, &text(lang, "bf_f_meldung"), &shown("meldung"));
    if !item.frames.is_empty() {
        // Every call site on its own line, a long one hangs by two more spaces.
        let lines: Vec<String> = item
            .frames
            .iter()
            .flat_map(|frame| {
                wrap_hard(frame, width.saturating_sub(INDENT))
                    .into_iter()
                    .enumerate()
                    .map(|(i, piece)| if i == 0 { piece } else { format!("{}{piece}", " ".repeat(INDENT)) })
                    .collect::<Vec<_>>()
            })
            .collect();
        page.row(at, &text(lang, "bf_f_stelle"), &lines);
    } else if let Some(earlier) = item.same_as {
        let value = filled(lang, "bf_f_wie", &[("0", Arg::from(numbers[earlier]))]);
        page.row(at, &text(lang, "bf_f_stelle"), &[value]);
    }
}

/// One hard crash: its lines, when and in which version.
struct Crash<'a> {
    lines: &'a [String],
    at: Option<f64>,
    version: &'a str,
}

/// The hard crash of the last run under `title`: when, which version, what it
/// was, and the dump cut to what says something, threads set off from their lines.
fn crash_section(page: &mut Page, facts: &Facts, title: &str, crash: &Crash<'_>, redactor: &Redactor) {
    let lang = facts.lang;
    page.heading(2, title);
    let at = crash.at.filter(|t| *t != 0.0);
    let date = at
        .and_then(formats::floor_seconds)
        .and_then(formats::local_time)
        .map_or_else(|| "—".to_string(), |t| formats::strftime(&text(lang, "b_datum"), &t));
    page.row(INDENT, &text(lang, "bf_c_wann"), &[date]);
    // ⚠ The mark states, it does not judge: the file outlives any number of
    // clean runs, so the version stands next to the date.
    let (version, note) = if crash.version.is_empty() {
        (text(lang, "bf_c_ohne_fassung"), Some(text(lang, "bf_ohne_fassung")))
    } else {
        let old = !facts.version.is_empty() && crash.version != facts.version;
        (crash.version.to_string(), old.then(|| text(lang, "bf_alt")))
    };
    page.row(INDENT, &text(lang, "bf_c_fassung"), &wrap_hard(&redactor.redact(&version), page.room()));
    if let Some(note) = note {
        page.row(INDENT, &text(lang, "bf_c_hinweis"), &wrap(&note, page.room()));
    }
    page.row(INDENT, &text(lang, "bf_c_befund"), &wrap(&text(lang, "bf_c_befund_text"), page.room()));
    page.push(format!("{}{}", " ".repeat(INDENT), text(lang, "bf_c_ablauf")));
    // Redacted, other than in the Python version: the dump holds paths.
    let mut in_thread = false;
    for line in crash_brief(crash.lines, CRASH_SHOWN) {
        let line = redactor.redact(&line);
        let line = py_strip(&line);
        let indent = if starts_thread(line) {
            in_thread = true;
            2 * INDENT
        } else if in_thread {
            3 * INDENT
        } else {
            2 * INDENT
        };
        // The following pieces hang by two more spaces: they are cut shorter.
        let pieces = wrap_hard(line, LINE_WIDTH.saturating_sub(indent + INDENT));
        for (i, piece) in pieces.iter().enumerate() {
            page.push(format!("{}{piece}", " ".repeat(if i == 0 { indent } else { indent + INDENT })));
        }
    }
    if crash.lines.len() > CRASH_SHOWN {
        let more = filled(lang, "bf_c_mehr", &[("0", Arg::from(crash.lines.len() - CRASH_SHOWN))]);
        page.push(format!("{}{more}", " ".repeat(2 * INDENT)));
    }
}

/// The report as text.
pub fn build(facts: &Facts, redactor: &Redactor) -> String {
    // The name under "Von" goes everywhere else, even before it is saved.
    let redactor = &redactor.with_names(&[facts.reporter.as_str()]);
    let lang = facts.lang;
    let mut page = Page::new(value_column(facts));
    page.push(filled(
        lang,
        "b_kopf",
        &[
            ("0", Arg::Text(if facts.version.is_empty() { "—".into() } else { facts.version.clone() })),
            ("1", Arg::Text(facts.now.clone())),
        ],
    ));
    page.blank();
    // ⚠ Not redacted: the one thing the player states on purpose. A user
    // name equal to the account would otherwise vanish from the line.
    let reporter = if facts.reporter.is_empty() { text(lang, "s_melder_leer") } else { facts.reporter.clone() };
    page.row(0, &text(lang, "b_melder"), &[reporter]);

    let note = py_strip(&facts.message);
    if !note.is_empty() {
        page.heading(2, &text(lang, "b_meldung"));
        page.body(INDENT, &wrap(&redactor.redact(note), LINE_WIDTH - INDENT));
    }
    for block in facts.blocks.iter().filter(|b| !b.entries.is_empty()) {
        page.heading(2, &block.title);
        let room = page.room();
        for entry in &block.entries {
            let value = entry.value.as_deref().filter(|v| !v.is_empty()).unwrap_or("—");
            page.row(INDENT, &entry.label, &wrap(&redactor.redact(value), room));
        }
    }

    let redacted = |lines: Vec<String>| -> Vec<String> { lines.iter().map(|l| redactor.redact(l)).collect() };
    let start = redacted(dense(&facts.trail_start));
    if !start.is_empty() {
        page.heading(2, &text(lang, "bf_h_start"));
        page.body(INDENT, &[text(lang, "bf_n_start"), String::new()]);
        let from = start.len().saturating_sub(START_SHOWN);
        page.body(INDENT, &start[from..]);
    }
    // The page of the report itself is not part of the list: the report is
    // made while it is built and would stand last and unfinished every time.
    // The page is called "diagnose" in the trail of 4.12 and "hf_diagnose" by
    // its key in v5.
    let mut pages = facts.trail_pages.clone();
    while pages.last().is_some_and(|l| l.contains("Seite diagnose") || l.contains("Seite hf_diagnose")) {
        pages.pop();
    }
    let pages = redacted(dense(&pages));
    if !pages.is_empty() {
        page.heading(2, &text(lang, "bf_h_seiten"));
        page.body(INDENT, &[text(lang, "bf_n_seiten"), String::new()]);
        let from = pages.len().saturating_sub(PAGES_SHOWN);
        page.body(INDENT, &pages[from..]);
    }
    for other in facts.others.iter().filter(|o| !o.trail.is_empty()) {
        let trail = redacted(dense(&other.trail));
        page.heading(2, &filled(lang, "bf_h_start_von", &[("0", Arg::from(other.name.as_str()))]));
        page.body(INDENT, &[text(lang, "bf_n_start"), String::new()]);
        let from = trail.len().saturating_sub(START_SHOWN);
        page.body(INDENT, &trail[from..]);
    }

    if !facts.update_lines.is_empty() {
        page.heading(2, &text(lang, "bf_h_update"));
        let lines: Vec<String> = facts.update_lines.iter().map(|e| redactor.redact(e)).collect();
        page.body(INDENT, &lines);
    }

    // ⚠ The crash stands before the errors: it is the graver finding. An
    // entry in the error list means the program lived on.
    if !facts.crash.is_empty() {
        let crash = Crash { lines: &facts.crash, at: facts.crash_at, version: &facts.crash_version };
        crash_section(&mut page, facts, &text(lang, "bf_h_absturz"), &crash, redactor);
    }
    for other in facts.others.iter().filter(|o| !o.crash.is_empty()) {
        let crash = Crash { lines: &other.crash, at: other.crash_at, version: &other.crash_version };
        let title = filled(lang, "bf_h_absturz_von", &[("0", Arg::from(other.name.as_str()))]);
        crash_section(&mut page, facts, &title, &crash, redactor);
    }
    errors_section(&mut page, facts, redactor);

    page.heading(2, &text(lang, "bf_h_schwaerzung"));
    page.body(INDENT, &wrap(&text(lang, "b_fuss"), LINE_WIDTH - INDENT));
    // Beyond 4.12: what else the redaction took out, on a line of its own.
    page.body(INDENT, &wrap(&text(lang, "bf_schwaerzung_mehr"), LINE_WIDTH - INDENT));
    page.lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::{Host, Paths};
    use crate::pypath::Flavor;

    fn lines(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn redactor() -> Redactor {
        let home = [("HOME", "/home/spieler"), ("USERPROFILE", "/home/spieler")];
        Redactor::new(&Paths::new(Host::new(Flavor::Posix, &home, "/")))
    }

    /// An entry of the error log as `fehler.json` holds it.
    fn error(time: &str, version: &str, place: &str, message: &str, trace: &str) -> Value {
        Value::Object(vec![
            ("zeit".into(), Value::str(time)),
            ("fassung".into(), Value::str(version)),
            ("stelle".into(), Value::str(place)),
            ("art".into(), Value::str("ValueError")),
            ("meldung".into(), Value::str(message)),
            ("spur".into(), Value::str(trace)),
        ])
    }

    fn repeated(mut entry: Value, times: i64, last: &str) -> Value {
        if let Value::Object(map) = &mut entry {
            jsonio::map_set(map, "anzahl", Value::int(times));
            jsonio::map_set(map, "zuletzt", Value::str(last));
        }
        entry
    }

    fn facts(version: &str, errors: Vec<Value>) -> Facts {
        Facts {
            lang: Lang::De,
            version: version.into(),
            now: "21.10.2026, 09:05".into(),
            reporter: "Tester".into(),
            message: "Es hängt".into(),
            blocks: vec![Block::new(
                "System",
                vec![Entry::new("System", "Linux"), Entry::new("Programm / Oberfläche", "egui 0.37")],
            )],
            trail_start: Vec::new(),
            trail_pages: Vec::new(),
            update_lines: Vec::new(),
            crash: Vec::new(),
            crash_at: None,
            crash_version: String::new(),
            others: Vec::new(),
            error_total: errors.len(),
            errors,
        }
    }

    fn trace(file: &str, number: u32, function: &str) -> String {
        format!(
            "Traceback (most recent call last):\n  File \"{file}\", line {number}, in {function}\n    code()\nValueError: x"
        )
    }

    fn built(facts: &Facts) -> String {
        build(facts, &redactor())
    }

    #[test]
    fn wrapping_keeps_own_breaks_and_whole_words() {
        assert_eq!(wrap("eins zwei drei", 8), ["eins", "zwei", "drei"]);
        assert_eq!(wrap("1. a\n2. b\n\n\n\n3. c", 76), ["1. a", "2. b", "", "3. c"]);
        assert_eq!(
            wrap("kurz\nPfad_ohne_Leerzeichen_der_zu_lang_ist", 10),
            ["kurz", "Pfad_ohne_Leerzeichen_der_zu_lang_ist"]
        );
        assert_eq!(wrap("  \n ", 76), [""]);
    }

    #[test]
    fn a_word_that_is_too_long_is_cut_at_a_separator() {
        assert_eq!(wrap_hard("x /aaaa/bbbb/cccc/dddd/eeee/ffff", 12), ["x", "/aaaa/bbbb/", "cccc/dddd/", "eeee/ffff"]);
        // A part of a widget path starts with `.!` and is not torn from its `!`.
        assert_eq!(wrap_hard(".!frame4.!frame8.!canvas.!frame", 16), [".!frame4.!frame8", ".!canvas.!frame"]);
        // Without a separator: hard, at the width.
        assert_eq!(wrap_hard("abcdefghijklmnopqrstuvwxyz", 10), ["abcdefghij", "klmnopqrst", "uvwxyz"]);
        // Short texts and own breaks stay as they are.
        assert_eq!(wrap_hard("a b\nc", 10), ["a b", "c"]);
    }

    #[test]
    fn equal_lines_in_a_row_become_one() {
        let entries = lines(&[
            "10:00:00  Liste: zeichnen",
            "10:00:01  Liste: zeichnen",
            "10:00:02  Seite x",
            "10:00:03  Liste: zeichnen",
        ]);
        assert_eq!(
            dense(&entries),
            ["10:00:00  Liste: zeichnen  (2×)", "10:00:02  Seite x", "10:00:03  Liste: zeichnen"]
        );
    }

    #[test]
    fn the_guilty_thread_comes_first() {
        let dump = lines(&[
            "Fatal error: x",
            "Thread 0x1 (most recent call first):",
            "  wartet",
            "Current thread 0x2 (most recent call first):",
            "  schuld",
            "Thread 0x3 (most recent call first):",
            "  wartet auch",
        ]);
        assert_eq!(
            crash_brief(&dump, 14),
            [
                "Fatal error: x",
                "Current thread 0x2 (most recent call first):",
                "  schuld",
                "Thread 0x1 (most recent call first):",
                "  wartet",
                "Thread 0x3 (most recent call first):",
                "  wartet auch"
            ]
        );
        assert_eq!(crash_brief(&lines(&["a", "b", "c"]), 2), ["a", "b"]);
    }

    #[test]
    fn the_sections_stand_in_a_fixed_order_under_headings() {
        let mut all = facts("5.0.0", vec![error("2026-10-20 01:00:00", "5.0.0", "a", "kaputt", "")]);
        all.trail_start = lines(&["10:00:00  Start"]);
        all.trail_pages = lines(&["10:00:05  Seite liste: zeigen"]);
        all.update_lines = lines(&["Setup-Protokoll, Stand x"]);
        all.crash = lines(&["Fatal error: x"]);
        all.crash_version = "5.0.0".into();
        let text = built(&all);
        let headings: Vec<&str> = text.lines().filter(|l| l.starts_with("## ")).collect();
        assert_eq!(
            headings,
            [
                "## Was passiert ist",
                "## System",
                "## Startverlauf des letzten Laufs",
                "## Zuletzt geöffnete Seiten",
                "## Letzter Update-Versuch",
                "## Harter Abbruch",
                "## Letzte Fehler (1 von 1 aufgehoben)",
                "## Schwärzung",
            ]
        );
        assert!(!text.lines().any(|l| l.ends_with(' ')), "no line ends with a space:\n{text}");
    }

    #[test]
    fn the_other_processes_get_their_own_trail_and_crash() {
        let mut all = facts("5.0.0", Vec::new());
        all.crash = lines(&["Fatal error: panic: hauptfenster"]);
        all.crash_version = "5.0.0".into();
        all.others = vec![
            OtherRun {
                name: "Overlay".into(),
                crash: lines(&["Fatal error: panic: overlay"]),
                crash_version: "4.9.0".into(),
                trail: lines(&["10:00:00  Start, Version 5.0.0, windows", "10:00:01  Overlay wird gebaut"]),
                ..OtherRun::default()
            },
            // Nothing left behind: no section.
            OtherRun { name: "Anlernfenster".into(), ..OtherRun::default() },
        ];
        let text = built(&all);
        let headings: Vec<&str> = text.lines().filter(|l| l.starts_with("## ")).collect();
        assert_eq!(
            headings,
            [
                "## Was passiert ist",
                "## System",
                "## Startverlauf · Overlay",
                "## Harter Abbruch",
                "## Harter Abbruch · Overlay",
                "## Letzte Fehler",
                "## Schwärzung",
            ],
            "{text}"
        );
        assert!(text.contains("Overlay wird gebaut") && text.contains("Fatal error: panic: overlay"), "{text}");
        let overlay = text.split("## Harter Abbruch · Overlay").nth(1).unwrap_or_default();
        assert!(overlay.contains("4.9.0"), "the overlay's crash names its own version:\n{overlay}");
    }

    #[test]
    fn the_report_reads_top_down_as_laid_out() {
        let errors = vec![
            repeated(
                error(
                    "2026-10-20 08:00:00",
                    "5.0.0",
                    "liste.zeichnen",
                    "kaputt",
                    &trace("src/liste.rs", 12, "zeichnen"),
                ),
                3,
                "2026-10-20 08:05:00",
            ),
            error("2026-10-19 09:00:00", "4.12.0", "alt", "alt kaputt", ""),
        ];
        let expected = "\
VerseKit 5.0.0 · Bericht vom 21.10.2026, 09:05

Von                     Tester

## Was passiert ist
  Es hängt

## System
  System                Linux
  Programm / Oberfläche egui 0.37

## Letzte Fehler (2 von 2 aufgehoben)

### Laufende Fassung 5.0.0

  Fehler 1
    Zeit                2026-10-20 08:00:00
    Wiederholt          3× dasselbe, bis 2026-10-20 08:05:00
    Fassung             5.0.0
    Bereich             liste.zeichnen
    Art                 ValueError
    Meldung             kaputt
    Stelle              src/liste.rs:12 in zeichnen: code()

### Ältere Fassungen
  Nicht aus der laufenden Fassung. Prüfen, ob es ihn noch gibt.

  Fehler 2
    Zeit                2026-10-19 09:00:00
    Fassung             4.12.0
    Bereich             alt
    Art                 ValueError
    Meldung             alt kaputt
";
        let text = built(&facts("5.0.0", errors));
        // The footer is a text of the language file: compare up to there.
        let (body, footer) = text.split_once("\n## Schwärzung\n").expect("footer");
        assert_eq!(body.trim_end(), expected.trim_end(), "\n{text}");
        assert!(footer.starts_with("  Pfade gekürzt"), "{text}");
    }

    #[test]
    fn all_values_stand_in_one_column() {
        let errors = vec![error("2026-10-20 08:00:00", "5.0.0", "a", "kaputt", "")];
        let text = built(&facts("5.0.0", errors));
        let column = |label: &str, value: &str| {
            let line =
                text.lines().find(|l| l.trim_start().starts_with(label) && l.contains(value)).unwrap_or_default();
            line.find(value)
        };
        let von = column("Von", "Tester");
        assert!(von.is_some());
        assert_eq!(von, column("System", "Linux"));
        assert_eq!(von, column("Zeit", "2026-10-20"));
        assert_eq!(von, column("Meldung", "kaputt"));
    }

    #[test]
    fn the_running_version_comes_first_and_the_mark_stands_once() {
        let errors = vec![
            error("2026-10-21 10:00:00", "4.12.0", "a", "alt eins", ""),
            error("2026-10-20 10:00:00", "4.11.0", "b", "alt zwei", ""),
            error("2026-10-19 10:00:00", "5.0.0", "c", "neu", ""),
            error("2026-10-18 10:00:00", "4.10.0", "d", "alt drei", ""),
        ];
        let text = built(&facts("5.0.0", errors));
        assert_eq!(text.matches("Nicht aus der laufenden Fassung").count(), 1, "{text}");
        let current = text.find("### Laufende Fassung 5.0.0").expect("part of the running version");
        let older = text.find("### Ältere Fassungen").expect("part of older versions");
        assert!(current < older);
        // The error of the running version is number 1, though it is not the newest.
        let first = text.find("  Fehler 1\n").expect("number 1");
        assert!(current < first && first < older, "{text}");
        assert!(text[first..older].contains("neu"), "{text}");
        for number in 2..=4 {
            assert!(text[older..].contains(&format!("  Fehler {number}\n")), "{text}");
        }
    }

    #[test]
    fn a_running_version_without_errors_says_so() {
        let text = built(&facts("5.0.0", vec![error("2026-10-21 10:00:00", "4.12.0", "a", "alt", "")]));
        assert!(
            text.contains("### Laufende Fassung 5.0.0\n  Keiner der aufgelisteten Fehler stammt aus dieser Fassung.\n"),
            "{text}"
        );
    }

    #[test]
    fn without_a_known_version_the_errors_are_not_parted() {
        let errors = vec![
            error("2026-10-21 10:00:00", "4.12.0", "a", "eins", ""),
            error("2026-10-20 10:00:00", "", "b", "zwei", ""),
        ];
        let text = built(&facts("", errors));
        assert!(!text.contains("### "), "{text}");
        assert!(!text.contains("Nicht aus der laufenden Fassung"), "{text}");
        assert!(text.contains("  Fehler 1\n") && text.contains("  Fehler 2\n"), "{text}");
        assert!(text.contains("Fassung             ?\n"), "an unknown version shows as a question mark:\n{text}");
    }

    #[test]
    fn errors_without_a_written_version_have_a_part_of_their_own() {
        let errors = vec![
            error("2026-10-21 10:00:00", "", "a", "ohne", ""),
            error("2026-10-20 10:00:00", "5.0.0", "b", "mit", ""),
        ];
        let text = built(&facts("5.0.0", errors));
        let unknown = text.find("### Fassung nicht festgehalten").expect("part without version");
        assert!(text.find("### Laufende Fassung 5.0.0").is_some_and(|c| c < unknown), "{text}");
        assert!(text[unknown..].contains("Prüfen, ob es ihn noch gibt."), "{text}");
        assert!(!text.contains("### Ältere Fassungen"), "{text}");
    }

    #[test]
    fn a_place_shows_its_call_sites_once_and_the_others_point_to_it() {
        let site = trace("src/liste.rs", 12, "zeichnen");
        let errors = vec![
            error("2026-10-21 10:00:00", "5.0.0", "liste", "eins", &site),
            error("2026-10-21 09:00:00", "5.0.0", "liste", "zwei", &site),
            error("2026-10-21 08:00:00", "5.0.0", "andere", "drei", ""),
            error("2026-10-21 07:00:00", "5.0.0", "andere", "vier", &site),
        ];
        let text = built(&facts("5.0.0", errors));
        assert_eq!(text.matches("src/liste.rs:12 in zeichnen").count(), 1, "{text}");
        assert!(text.contains("Stelle              wie Fehler 1\n"), "{text}");
        // The first of a place has no call sites: nothing to point to.
        assert_eq!(text.matches("wie Fehler").count(), 1, "{text}");
    }

    #[test]
    fn a_pointer_names_the_number_as_shown_not_the_place_in_the_file() {
        let site = trace("src/liste.rs", 12, "zeichnen");
        // The newest entry is of an older version, the second of the running one:
        // the running one is shown first and the pointer refers to the number shown.
        let errors = vec![
            error("2026-10-21 10:00:00", "4.12.0", "liste", "alt", &site),
            error("2026-10-20 10:00:00", "5.0.0", "liste", "neu", &site),
        ];
        let text = built(&facts("5.0.0", errors));
        let older = text.find("### Ältere Fassungen").expect("older");
        assert!(text[older..].contains("Stelle              src/liste.rs:12 in zeichnen: code()"), "{text}");
        assert!(text[..older].contains("Stelle              wie Fehler 2\n"), "{text}");
    }

    #[test]
    fn repetitions_belong_to_the_block_of_the_error() {
        let errors = vec![repeated(error("2026-10-20 08:00:00", "5.0.0", "a", "kaputt", ""), 4, "2026-10-20 08:09:00")];
        let text = built(&facts("5.0.0", errors));
        let block = text.split("  Fehler 1\n").nth(1).unwrap_or_default();
        let rows: Vec<&str> = block.lines().take(3).collect();
        assert!(rows[0].trim_start().starts_with("Zeit"), "{text}");
        assert!(rows[1].contains("4× dasselbe, bis 2026-10-20 08:09:00"), "{text}");
        assert!(rows[2].trim_start().starts_with("Fassung"), "{text}");
        assert!(!text.contains("(4× dasselbe"), "no old line of its own:\n{text}");
    }

    #[test]
    fn equal_errors_one_after_the_other_become_one_block() {
        let errors = vec![
            error("2026-10-20 08:03:00", "5.0.0", "a", "kaputt", ""),
            error("2026-10-20 08:02:00", "5.0.0", "a", "kaputt", ""),
            error("2026-10-20 08:01:00", "5.0.0", "b", "anders", ""),
        ];
        let text = built(&facts("5.0.0", errors));
        assert!(text.contains("  Fehler 2\n") && !text.contains("  Fehler 3\n"), "{text}");
        assert!(text.contains("2× dasselbe, bis 2026-10-20 08:02:00"), "{text}");
    }

    #[test]
    fn the_texts_of_the_errors_are_redacted_again() {
        // An entry written by an older version may hold what the log had not cut.
        let errors =
            vec![error("2026-10-20 08:00:00", "5.0.0", "/home/spieler/ort", "fehlt: /home/spieler/x.json", "")];
        let text = built(&facts("5.0.0", errors));
        assert!(text.contains("<heim>/x.json") && text.contains("<heim>/ort"), "{text}");
        assert!(!text.contains("spieler"), "{text}");
    }

    #[test]
    fn what_comes_from_outside_is_redacted_everywhere_it_stands() {
        let home = "/home/spieler";
        // The code line is cut after it is redacted: no half of a name is left.
        let code_trace = format!(
            "Traceback (most recent call last):\n  File \"{home}/src/liste.rs\", line 3, in zeichnen\n    {}{home}/abc\nValueError: x",
            "x".repeat(82)
        );
        let errors = vec![error(
            "2026-10-20 08:00:00",
            "5.0.0",
            &format!("{home}/ort"),
            &format!("fehlt {home}/x.json"),
            &code_trace,
        )];
        let mut all = facts("5.0.0", errors);
        all.crash =
            lines(&[&format!("Fatal error in {home}/dump.so"), &format!("  File \"{home}/a.py\", line 1 in f")]);
        all.crash_version = format!("{home}/v");
        all.update_lines = lines(&[&format!("Log {home}/setup.txt")]);
        all.message = format!("Datei {home}/n.txt");
        all.blocks[0].entries.push(Entry::new("Ordner", format!("{home}/daten")));
        let text = built(&all);
        assert!(!text.contains("spieler") && !text.contains("/home"), "{text}");
        assert!(text.contains("<heim>"), "{text}");
        assert!(text.lines().any(|l| l.contains("xxxxxxxxxx") && !l.contains("spi")), "{text}");
    }

    /// Lines of the game's log may stand in a message, an error, a crash, an update line or a
    /// trail: handle, player number, computer and address go everywhere, the number of a thing
    /// stays, and the name under "Von" stays as the player gave it.
    #[test]
    fn what_the_game_log_says_about_the_player_goes_everywhere_but_from() {
        let dir = std::env::temp_dir().join(format!("versekit-bericht-schwaerzen-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("einstellungen.json"), r#"{"eigener_account": "Testpilot"}"#).unwrap();
        let data = dir.to_string_lossy().into_owned();
        let vars = [("HOME", "/home/spieler"), ("SC_BP_HOME", data.as_str()), ("COMPUTERNAME", "RECHNER-XY9")];
        let redactor = Redactor::new(&Paths::new(Host::new(Flavor::native(), &vars, "/")));
        let log = "Handle[Testpilot] for 'Testpilot' [2001234567] in 'Harvestable_Oza_123' [123] geid 2001234567 \
                   von 203.0.113.7 auf RECHNER-XY9";
        let mut all = facts("5.0.0", vec![error("2026-10-20 08:00:00", "5.0.0", "logscan", log, "")]);
        all.reporter = "Testpilot".into();
        all.message = log.into();
        all.crash = lines(&[log]);
        all.update_lines = lines(&[log]);
        all.trail_start = lines(&[&format!("10:00:00  {log}")]);
        all.trail_pages = lines(&[&format!("10:00:05  {log}")]);
        all.blocks[0].entries.push(Entry::new("Spiel", log));
        let text = build(&all, &redactor);
        let from = text.lines().find(|l| l.starts_with("Von")).expect("from");
        assert!(from.contains("Testpilot"), "{from}");
        let rest: String = text.lines().filter(|l| !l.starts_with("Von")).collect::<Vec<_>>().join("\n");
        for gone in ["Testpilot", "2001234567", "203.0.113.7", "RECHNER-XY9"] {
            assert!(!rest.contains(gone), "{gone} in {rest}");
        }
        // Message, both trails and the update stand on lines of their own; the number stays in each.
        assert!(rest.matches("'Harvestable_Oza_123' [123]").count() >= 4, "{rest}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The name typed under "Von" and not saved yet goes everywhere else, whole and word by word;
    /// the redaction says below the sentence of 4.12 what else it replaced.
    #[test]
    fn the_reporter_given_goes_everywhere_but_from_and_the_footer_says_what_else_went() {
        let mut all = facts("5.0.0", vec![error("2026-10-20 08:00:00", "5.0.0", "a", "Kai Beispielmann sah es", "")]);
        all.reporter = "Kai Beispielmann".into();
        all.message = "Beispielmann meldet".into();
        let text = built(&all);
        let from = text.lines().find(|l| l.starts_with("Von")).expect("from");
        assert!(from.contains("Kai Beispielmann"), "{from}");
        let rest: String = text.lines().filter(|l| !l.starts_with("Von")).collect::<Vec<_>>().join("\n");
        assert!(!rest.contains("Beispielmann"), "{rest}");
        let footer = text.split("## Schwärzung").nth(1).expect("footer");
        let more: String = footer.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(more.contains(&super::text(Lang::De, "bf_schwaerzung_mehr")), "{footer}");
    }

    #[test]
    fn no_line_of_crash_and_errors_sticks_out() {
        let mut all = facts("5.0.0", vec![error("2026-10-20 08:00:00", "5.0.0", "a", "kaputt", "")]);
        let long = format!("  File \"{}\", line 1 in funktion_mit_langem_namen", "pfad/".repeat(30));
        all.crash = lines(&["Fatal error: x", "Current thread 0x1 (most recent call first):", &long]);
        all.crash_version = "5.0.0".into();
        let text = built(&all);
        let start = text.find("## Harter Abbruch").unwrap_or_default();
        for line in text[start..].lines() {
            assert!(line.chars().count() <= LINE_WIDTH, "{} chars: {line}", line.chars().count());
        }
    }

    #[test]
    fn long_messages_break_evenly_and_no_line_sticks_out() {
        let long = format!("invalid command name \".!toplevel2{}\"", ".!frame4.!frame8.!canvas".repeat(6));
        let errors = vec![error("2026-10-20 08:00:00", "5.0.0", "oberflaeche", &long, "")];
        let text = built(&facts("5.0.0", errors));
        let start = text.find("## Letzte Fehler").unwrap_or_default();
        for line in text[start..].lines() {
            assert!(line.chars().count() <= LINE_WIDTH, "{} chars: {line}", line.chars().count());
        }
        // Every following line of a message hangs under its first one.
        let rows: Vec<&str> = text.lines().skip_while(|l| !l.trim_start().starts_with("Meldung")).take(3).collect();
        assert!(rows[1].starts_with(&" ".repeat(24)) && !rows[1].trim_start().is_empty(), "{text}");
    }

    #[test]
    fn no_errors_say_so() {
        let text = built(&facts("5.0.0", Vec::new()));
        assert!(text.contains("## Letzte Fehler\n  Keine aufgezeichnet.\n"), "{text}");
    }

    #[test]
    fn the_crash_is_a_block_with_the_threads_set_off() {
        let mut all = facts("5.0.0", Vec::new());
        all.crash = lines(&[
            "Windows fatal exception: access violation",
            "Thread 0x1 [sc-bp-hotkey] (most recent call first):",
            "  File \"a.py\", line 1 in wait",
            "Current thread 0x2 [MainThread] (most recent call first):",
            "  File \"b.py\", line 2 in main",
        ]);
        all.crash_version = "4.12.0".into();
        let text = built(&all);
        let block = text.split("## Harter Abbruch\n").nth(1).unwrap_or_default();
        let block = block.split("\n\n").next().unwrap_or_default();
        assert!(block.contains("  Fassung               4.12.0\n"), "{text}");
        assert!(block.contains("  Hinweis               Nicht aus der laufenden Fassung."), "{text}");
        assert!(block.contains("  Ablauf\n    Windows fatal exception: access violation\n"), "{text}");
        // The guilty thread first, its lines one step deeper than the heading of the thread.
        assert!(
            block.contains(
                "    Current thread 0x2 [MainThread] (most recent call first):\n      File \"b.py\", line 2 in main\n"
            ),
            "{text}"
        );
        assert!(
            block.contains(
                "    Thread 0x1 [sc-bp-hotkey] (most recent call first):\n      File \"a.py\", line 1 in wait"
            ),
            "{text}"
        );
        // The same version: no mark.
        all.crash_version = "5.0.0".into();
        assert!(!built(&all).contains("Hinweis"), "no mark for the running version");
        // No version written down: said so, with the note.
        all.crash_version = String::new();
        let unknown = built(&all);
        assert!(
            unknown.contains("nicht festgehalten") && unknown.contains("Prüfen, ob es ihn noch gibt."),
            "{unknown}"
        );
    }

    #[test]
    fn a_long_crash_is_cut_and_says_how_much_is_left() {
        let mut all = facts("5.0.0", Vec::new());
        all.crash = (0..30).map(|i| format!("Fatal line {i}")).collect();
        all.crash_version = "5.0.0".into();
        let text = built(&all);
        assert!(text.contains("    … 16 weitere Zeilen\n"), "{text}");
    }

    #[test]
    fn the_english_report_is_laid_out_the_same() {
        let mut all = facts("5.0.0", vec![error("2026-10-20 08:00:00", "4.12.0", "a", "broken", "")]);
        all.lang = Lang::En;
        let text = built(&all);
        for heading in [
            "## What happened",
            "## Recent errors (1 of 1 kept)",
            "### Running version 5.0.0",
            "### Older versions",
            "## Redaction",
        ] {
            assert!(text.lines().any(|l| l == heading), "{heading} missing:\n{text}");
        }
        assert!(text.contains("Not from the running version. Check whether it still occurs."), "{text}");
    }
}
