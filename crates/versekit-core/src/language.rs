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

//! User interface texts in German and English.
//!
//! The texts live in one JSON file per language (`lang/de.json`,
//! `lang/en.json`), embedded at build time. **The files are the original:**
//! edit them by hand, German and English together, one entry per line; then
//! run `cargo test -p versekit-core` (`tests/language_files.rs` checks them).
//!
//! Placeholders are `{name}` or `{name:spec}`; `{{` and `}}` are literal
//! braces. Supported specs: `g` and `+g` (shortest number, signed), `+`
//! (signed integer), `.Nf` and `+.Nf` (fixed decimals). Integer placeholders
//! expect [`Arg::Int`].
//!
//! Numbers and dates per language: [`crate::formats`].

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

use regex::Regex;

use crate::jsonio::{self, Map, Value};
use crate::paths::{self, Host};
use crate::text::py_strip;

const DE_JSON: &str = include_str!("../lang/de.json");
const EN_JSON: &str = include_str!("../lang/en.json");

/// A language of the user interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lang {
    De,
    En,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::De, Lang::En];

    pub fn code(self) -> &'static str {
        match self {
            Lang::De => "de",
            Lang::En => "en",
        }
    }

    pub fn from_code(code: &str) -> Option<Lang> {
        match code {
            "de" => Some(Lang::De),
            "en" => Some(Lang::En),
            _ => None,
        }
    }

    fn index(self) -> usize {
        match self {
            Lang::De => 0,
            Lang::En => 1,
        }
    }
}

/// What the user chose in the settings (`sprache`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    De,
    En,
    Auto,
}

impl Choice {
    pub fn code(self) -> &'static str {
        match self {
            Choice::De => "de",
            Choice::En => "en",
            Choice::Auto => "auto",
        }
    }
}

/// The texts of one language.
#[derive(Default)]
pub struct Table {
    pub texts: HashMap<String, String>,
    /// Readable names of erkul field paths (`feldnamen`).
    pub fields: HashMap<String, String>,
    /// Names of recipe properties (`eigenschaften`).
    pub properties: HashMap<String, String>,
    /// Property names for mining lasers (`eigenschaften_laser`).
    pub properties_laser: HashMap<String, String>,
}

fn section(root: &Value, name: &str) -> HashMap<String, String> {
    root.get(name)
        .and_then(Value::as_object)
        .map(|map| map.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect())
        .unwrap_or_default()
}

/// Parses one language file; a broken file yields an empty table, so every
/// text shows its key instead of crashing (the tests keep the files valid).
pub fn parse_table(source: &str) -> Table {
    let Ok(root) = jsonio::loads(source) else { return Table::default() };
    Table {
        texts: section(&root, "texte"),
        fields: section(&root, "feldnamen"),
        properties: section(&root, "eigenschaften"),
        properties_laser: section(&root, "eigenschaften_laser"),
    }
}

/// Both embedded tables, German first.
pub fn tables() -> &'static [Table; 2] {
    static TABLES: OnceLock<[Table; 2]> = OnceLock::new();
    TABLES.get_or_init(|| [parse_table(DE_JSON), parse_table(EN_JSON)])
}

fn table(lang: Lang) -> &'static Table {
    &tables()[lang.index()]
}

// ---------------------------------------------------------------------------
// Choosing the language
// ---------------------------------------------------------------------------

fn de_or_en(value: &str) -> Lang {
    if value.to_lowercase().starts_with("de") { Lang::De } else { Lang::En }
}

/// What the operating system says. Anything but German counts as English.
pub fn system_language(host: &Host) -> Lang {
    for name in ["SC_BP_SPRACHE", "LANG", "LC_ALL"] {
        if let Some(value) = host.var_set(name) {
            return de_or_en(value);
        }
    }
    let ident = os_locale(host).to_lowercase();
    if ident.starts_with("de") || ident.starts_with("german") { Lang::De } else { Lang::En }
}

#[cfg(windows)]
fn os_locale(_host: &Host) -> String {
    use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;
    let mut buffer = [0u16; 85];
    // SAFETY: the buffer holds 85 UTF-16 units, as passed.
    let written = unsafe { GetUserDefaultLocaleName(buffer.as_mut_ptr(), buffer.len() as i32) };
    if written <= 0 {
        return String::new();
    }
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

/// Python's `locale.getlocale()` reports the `LC_CTYPE` locale taken from the
/// environment at startup; `LC_ALL` and `LANG` were already checked above.
/// ⚠ Python also needs the locale to be installed; that is not checked here.
#[cfg(not(windows))]
fn os_locale(host: &Host) -> String {
    host.var_set("LC_CTYPE").unwrap_or_default().to_string()
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Int(n) => n != "0",
        Value::Float(x) => *x != 0.0,
        Value::Str(s) => !s.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
    }
}

/// The choice stored in the settings (`language.chosen()`).
///
/// ⚠ A non-empty value that is not text (a number, `true`) makes Python raise;
/// here it counts as `auto`.
pub fn choice_of(settings: &Map) -> Choice {
    let value = settings.iter().find(|(k, _)| k == "sprache").map(|(_, v)| v);
    let Some(Value::Str(text)) = value.filter(|v| truthy(v)) else { return Choice::Auto };
    match py_strip(text).to_lowercase().as_str() {
        "de" => Choice::De,
        "en" => Choice::En,
        _ => Choice::Auto,
    }
}

fn resolve(choice: Choice, host: &Host) -> Lang {
    match choice {
        Choice::De => Lang::De,
        Choice::En => Lang::En,
        Choice::Auto => system_language(host),
    }
}

/// 0 = not decided yet, otherwise `Lang::index() + 1`.
static CURRENT: AtomicU8 = AtomicU8::new(0);

fn store(lang: Lang) {
    CURRENT.store(lang.index() as u8 + 1, Ordering::Relaxed);
}

thread_local! {
    /// A language only this thread uses (0 = none), for checks that draw in
    /// both languages while other threads go on in theirs.
    static THREAD_LANGUAGE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

/// Runs `work` with `lang` as the language of this thread only; the language
/// of the program and of all other threads stays as it is.
pub fn with_language<R>(lang: Lang, work: impl FnOnce() -> R) -> R {
    let before = THREAD_LANGUAGE.with(|l| l.replace(lang.index() as u8 + 1));
    let result = work();
    THREAD_LANGUAGE.with(|l| l.set(before));
    result
}

/// Like [`set_language`], for this thread only: German or English from now
/// on in this thread, `Auto` gives the thread back to the language of the
/// program. For checks that run through both languages while other threads
/// go on in theirs.
pub fn set_thread_language(choice: Choice) {
    let value = match choice {
        Choice::De => Lang::De.index() as u8 + 1,
        Choice::En => Lang::En.index() as u8 + 1,
        Choice::Auto => 0,
    };
    THREAD_LANGUAGE.with(|l| l.set(value));
}

/// The language texts are written in. Decided once from the settings.
pub fn current() -> Lang {
    match THREAD_LANGUAGE.with(std::cell::Cell::get) {
        1 => return Lang::De,
        2 => return Lang::En,
        _ => {}
    }
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::De,
        2 => Lang::En,
        _ => {
            let paths = paths::global();
            let lang = resolve(choice_of(&paths.settings()), &paths.host);
            store(lang);
            lang
        }
    }
}

/// Switches the language for this run without saving it. Returns whether it
/// changed (then the windows have to relabel themselves).
pub fn set_language(choice: Choice) -> bool {
    let before = CURRENT.load(Ordering::Relaxed);
    store(resolve(choice, &paths::global().host));
    CURRENT.load(Ordering::Relaxed) != before
}

// ---------------------------------------------------------------------------
// Texts and placeholders
// ---------------------------------------------------------------------------

/// A value for a placeholder.
#[derive(Clone, Debug, PartialEq)]
pub enum Arg {
    Int(i64),
    Float(f64),
    Text(String),
    /// Rendered in the same language as the surrounding text.
    Phrase(Box<Phrase>),
    /// Parts joined by a separator, each rendered in the same language.
    Chain(String, Vec<Arg>),
    /// A time stamp written with the date format of a key (Python's
    /// `Moment`, default key `m_erster_datum`).
    Moment(f64, String),
}

impl Arg {
    /// A date in the format of `m_erster_datum`.
    pub fn moment(timestamp: f64) -> Arg {
        Arg::Moment(timestamp, "m_erster_datum".into())
    }
}

macro_rules! arg_from_int {
    ($($t:ty),*) => {$(
        impl From<$t> for Arg {
            fn from(n: $t) -> Arg {
                Arg::Int(n as i64)
            }
        }
    )*};
}
arg_from_int!(i8, i16, i32, i64, u8, u16, u32, usize);

impl From<f64> for Arg {
    fn from(x: f64) -> Arg {
        Arg::Float(x)
    }
}

impl From<f32> for Arg {
    fn from(x: f32) -> Arg {
        Arg::Float(f64::from(x))
    }
}

impl From<&str> for Arg {
    fn from(s: &str) -> Arg {
        Arg::Text(s.to_string())
    }
}

impl From<String> for Arg {
    fn from(s: String) -> Arg {
        Arg::Text(s)
    }
}

impl From<Phrase> for Arg {
    fn from(p: Phrase) -> Arg {
        Arg::Phrase(Box::new(p))
    }
}

/// A text that is put into words only when shown (Python's `Phrase`).
///
/// Store this instead of a finished string wherever a message stays on
/// screen: after switching the language it renders in the new one.
#[derive(Clone, Debug, PartialEq)]
pub struct Phrase {
    pub key: String,
    pub args: Vec<(String, Arg)>,
}

impl Phrase {
    pub fn new(key: &str) -> Phrase {
        Phrase { key: key.to_string(), args: Vec::new() }
    }

    /// Adds a value for placeholder `name`.
    pub fn with(mut self, name: &str, value: impl Into<Arg>) -> Phrase {
        self.args.push((name.to_string(), value.into()));
        self
    }

    pub fn render(&self, lang: Lang) -> String {
        let args: Vec<(&str, Arg)> = self.args.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
        tf_in(lang, &self.key, &args)
    }
}

impl std::fmt::Display for Phrase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.render(current()))
    }
}

/// The raw text of a key, or `None` if the key does not exist.
///
/// An empty translation falls back to German, like Python.
pub fn text_in(lang: Lang, key: &str) -> Option<&'static str> {
    let german = table(Lang::De).texts.get(key)?;
    match table(lang).texts.get(key) {
        Some(text) if !text.is_empty() => Some(text),
        _ => Some(german),
    }
}

thread_local! {
    /// Whether every text shows its key instead; only the thread that draws the window asks.
    static SHOW_KEYS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// In this thread every text of the language file shows its key in brackets
/// instead of the text.
pub fn set_show_keys(on: bool) {
    SHOW_KEYS.with(|k| k.set(on));
}

/// Whether the texts of this thread show their keys.
pub fn show_keys() -> bool {
    SHOW_KEYS.with(std::cell::Cell::get)
}

/// A text in `lang`; a missing key returns the key itself.
pub fn t_in(lang: Lang, key: &str) -> String {
    if show_keys() && text_in(lang, key).is_some() {
        return format!("[{key}]");
    }
    text_in(lang, key).map_or_else(|| key.to_string(), str::to_string)
}

/// A text in `lang` with its placeholders filled.
pub fn tf_in(lang: Lang, key: &str, args: &[(&str, Arg)]) -> String {
    if show_keys() && text_in(lang, key).is_some() {
        return format!("[{key}]");
    }
    match text_in(lang, key) {
        Some(text) => fill_in(lang, text, args),
        None => key.to_string(),
    }
}

/// A text in the current language.
pub fn t(key: &str) -> String {
    t_in(current(), key)
}

/// A text in the current language with its placeholders filled.
pub fn tf(key: &str, args: &[(&str, Arg)]) -> String {
    tf_in(current(), key, args)
}

/// How long ago, as the status page writes it: „gerade eben“, „vor 1 Std.“,
/// „vor 3 Tagen“, with singular and plural kept apart (`pages._relative_time`).
pub fn relative_age_in(lang: Lang, seconds: f64) -> String {
    let age = seconds.max(0.0);
    if age < 90.0 {
        return t_in(lang, "s_st_gerade");
    }
    let form = |count: i64, key: &str| {
        if count == 1 { t_in(lang, &format!("{key}_1")) } else { tf_in(lang, key, &[("0", Arg::Int(count))]) }
    };
    if age < 3600.0 {
        form((age / 60.0) as i64, "s_st_vor_min")
    } else if age < 86_400.0 {
        form((age / 3600.0) as i64, "s_st_vor_std")
    } else if age < 60.0 * 86_400.0 {
        form((age / 86_400.0) as i64, "s_st_vor_tag")
    } else {
        form(((age / (30.0 * 86_400.0)) as i64).max(1), "s_st_vor_monat")
    }
}

/// [`relative_age_in`] in the current language.
pub fn relative_age(seconds: f64) -> String {
    relative_age_in(current(), seconds)
}

/// Fills `{name}` placeholders in the current language.
pub fn fill(template: &str, args: &[(&str, Arg)]) -> String {
    fill_in(current(), template, args)
}

/// Fills `{name}` and `{name:spec}` placeholders.
///
/// A placeholder without a value, or whose spec does not fit the value, stays
/// as it is: a mistake should show, not crash.
pub fn fill_in(lang: Lang, template: &str, args: &[(&str, Arg)]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    for piece in pieces(template) {
        match piece {
            Piece::Literal(text) => out.push_str(text),
            Piece::Field { raw, name, spec } => {
                let rendered =
                    args.iter().find(|(k, _)| *k == name).and_then(|(_, value)| render_arg(lang, value, spec));
                out.push_str(rendered.as_deref().unwrap_or(raw));
            }
        }
    }
    out
}

/// The placeholders of a text as `(name, spec)`, in order.
pub fn placeholders(template: &str) -> Vec<(String, String)> {
    pieces(template)
        .into_iter()
        .filter_map(|piece| match piece {
            Piece::Field { name, spec, .. } => Some((name.to_string(), spec.to_string())),
            Piece::Literal(_) => None,
        })
        .collect()
}

enum Piece<'a> {
    Literal(&'a str),
    /// `raw` is the whole `{name:spec}`.
    Field {
        raw: &'a str,
        name: &'a str,
        spec: &'a str,
    },
}

/// Splits a template into literal text and placeholders. `{{` and `}}` become
/// one brace; a lone `}` or an unclosed `{` stays literal.
fn pieces(template: &str) -> Vec<Piece<'_>> {
    let mut found = Vec::new();
    let mut rest = template;
    while let Some(pos) = rest.find(['{', '}']) {
        found.push(Piece::Literal(&rest[..pos]));
        let tail = &rest[pos..];
        if tail.starts_with("{{") || tail.starts_with("}}") {
            found.push(Piece::Literal(&tail[..1]));
            rest = &tail[2..];
        } else if let Some(after) = tail.strip_prefix('}') {
            found.push(Piece::Literal("}"));
            rest = after;
        } else if let Some(end) = tail.find('}') {
            let inner = &tail[1..end];
            let (name, spec) = inner.split_once(':').unwrap_or((inner, ""));
            found.push(Piece::Field { raw: &tail[..=end], name, spec });
            rest = &tail[end + 1..];
        } else {
            found.push(Piece::Literal(tail));
            rest = "";
        }
    }
    found.push(Piece::Literal(rest));
    found
}

/// Whether a spec is one `fill` understands.
pub fn spec_supported(spec: &str) -> bool {
    if matches!(spec, "" | "g" | "+g" | "+") {
        return true;
    }
    fixed_places(spec).is_some()
}

/// `.Nf` / `+.Nf` → (sign, N).
fn fixed_places(spec: &str) -> Option<(bool, usize)> {
    let (plus, body) = match spec.strip_prefix('+') {
        Some(body) => (true, body),
        None => (false, spec),
    };
    let digits = body.strip_prefix('.')?.strip_suffix('f')?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok().map(|n| (plus, n))
}

fn render_arg(lang: Lang, value: &Arg, spec: &str) -> Option<String> {
    let number = match value {
        Arg::Int(n) => Some(*n as f64),
        Arg::Float(x) => Some(*x),
        _ => None,
    };
    match spec {
        "" => Some(match value {
            Arg::Int(n) => n.to_string(),
            Arg::Float(x) => py_str_float(*x),
            Arg::Text(s) => s.clone(),
            Arg::Phrase(p) => p.render(lang),
            Arg::Chain(sep, parts) => {
                parts.iter().filter_map(|part| render_arg(lang, part, "")).collect::<Vec<_>>().join(sep)
            }
            Arg::Moment(timestamp, key) => crate::formats::moment(lang, *timestamp, key),
        }),
        "+" => match value {
            Arg::Int(n) => Some(format!("{n:+}")),
            Arg::Float(x) if x.is_finite() => Some(format!("{:+}", x.trunc() as i64)),
            _ => None,
        },
        "g" | "+g" => number.map(|x| py_g(x, spec == "+g")),
        _ => {
            let (plus, places) = fixed_places(spec)?;
            number.map(|x| py_fixed(x, places, plus))
        }
    }
}

fn special_float(x: f64, plus: bool) -> Option<String> {
    let sign = if x.is_sign_negative() {
        "-"
    } else if plus {
        "+"
    } else {
        ""
    };
    if x.is_nan() {
        return Some(format!("{}nan", if plus { "+" } else { "" }));
    }
    if x.is_infinite() {
        return Some(format!("{sign}inf"));
    }
    None
}

/// Python's `str(float)`.
pub fn py_str_float(x: f64) -> String {
    special_float(x, false).unwrap_or_else(|| jsonio::float_text(x))
}

/// Python's `'%.Nf'` / `'%+.Nf'`.
pub fn py_fixed(x: f64, places: usize, plus: bool) -> String {
    if let Some(text) = special_float(x, plus) {
        return text;
    }
    if plus { format!("{x:+.places$}") } else { format!("{x:.places$}") }
}

/// Python's `'%g'` / `'%+g'`: six significant digits, trailing zeros removed,
/// exponent form below 1e-4 and from 1e6 on.
pub fn py_g(x: f64, plus: bool) -> String {
    if let Some(text) = special_float(x, plus) {
        return text;
    }
    const PRECISION: i32 = 6;
    let scientific = format!("{:.*e}", (PRECISION - 1) as usize, x);
    let Some((mantissa, exp)) = scientific.split_once('e') else { return scientific };
    let exp: i32 = exp.parse().unwrap_or(0);
    let body = if (-4..PRECISION).contains(&exp) {
        let places = (PRECISION - 1 - exp) as usize;
        strip_zeros(&format!("{x:.places$}"))
    } else {
        let digits = strip_zeros(mantissa);
        format!("{digits}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs())
    };
    if plus && !x.is_sign_negative() { format!("+{body}") } else { body }
}

fn strip_zeros(number: &str) -> String {
    if !number.contains('.') {
        return number.to_string();
    }
    number.trim_end_matches('0').trim_end_matches('.').to_string()
}

// ---------------------------------------------------------------------------
// Helpers around the tables
// ---------------------------------------------------------------------------

/// Blueprint kind (raw scmdb term) in `lang`; `art_unbekannt` if empty, the
/// raw term itself if there is no entry.
pub fn kind_label_in(lang: Lang, raw: &str) -> String {
    let key = format!("art_{raw}");
    if text_in(Lang::De, &key).is_some() {
        t_in(lang, &key)
    } else if !raw.is_empty() {
        raw.to_string()
    } else {
        t_in(lang, "art_unbekannt")
    }
}

pub fn kind_label(raw: &str) -> String {
    kind_label_in(current(), raw)
}

fn field_in(lang: Lang, path: &str) -> Option<&'static str> {
    let german = table(Lang::De).fields.get(path)?;
    match table(lang).fields.get(path) {
        Some(text) if !text.is_empty() => Some(text),
        _ => Some(german),
    }
}

/// Readable name of an erkul field path, or the path itself.
///
/// Looked up exactly first, then with indices removed (`[0]` → `[]`): the same
/// field at two positions needs only one entry.
pub fn pa_field_in(lang: Lang, path: &str) -> String {
    static INDEX: OnceLock<Option<Regex>> = OnceLock::new();
    if let Some(text) = field_in(lang, path) {
        return text.to_string();
    }
    let generic =
        INDEX.get_or_init(|| Regex::new(r"\[\d+\]").ok()).as_ref().map(|rx| rx.replace_all(path, "[]").into_owned());
    generic.as_deref().and_then(|g| field_in(lang, g)).map_or_else(|| path.to_string(), str::to_string)
}

pub fn pa_field(path: &str) -> String {
    pa_field_in(current(), path)
}

/// Name of a recipe property in `lang`.
///
/// English keeps the game's name, except `weapon_damage`. `laser` for mining
/// lasers: there `weapon_damage` is the laser power.
pub fn property_name_in(lang: Lang, name: &str, key: Option<&str>, laser: bool) -> String {
    let table = table(lang);
    let Some(key) = key else { return name.to_string() };
    if laser && let Some(text) = table.properties_laser.get(key) {
        return text.clone();
    }
    match table.properties.get(key) {
        Some(text) if !text.is_empty() => text.clone(),
        _ => name.to_string(),
    }
}

pub fn property_name(name: &str, key: Option<&str>, laser: bool) -> String {
    property_name_in(current(), name, key, laser)
}

/// The window title, with a warning for test builds (`SC_BP_TESTFASSUNG`).
pub fn window_title_in(lang: Lang, text: &str, host: &Host) -> String {
    match host.var("SC_BP_TESTFASSUNG") {
        Some(value) if !value.is_empty() && value != "0" => format!("{text}   {}", t_in(lang, "s_testfassung")),
        _ => text.to_string(),
    }
}

pub fn window_title(text: &str) -> String {
    window_title_in(current(), text, &paths::global().host)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pypath::Flavor;

    fn args(values: &[(&'static str, Arg)]) -> Vec<(&'static str, Arg)> {
        values.to_vec()
    }

    #[test]
    fn relative_age_one_and_many() {
        let de = |s: f64| relative_age_in(Lang::De, s);
        assert_eq!(de(-5.0), "gerade eben");
        assert_eq!(de(89.9), "gerade eben");
        assert_eq!(de(90.0), "vor 1 Min.");
        assert_eq!(de(300.0), "vor 5 Min.");
        assert_eq!(de(3600.0), "vor 1 Std.");
        assert_eq!(de(3.0 * 3600.0 + 59.0), "vor 3 Std.");
        assert_eq!(de(86_400.0), "vor 1 Tag");
        assert_eq!(de(3.0 * 86_400.0), "vor 3 Tagen");
        assert_eq!(de(59.0 * 86_400.0), "vor 59 Tagen");
        assert_eq!(de(60.0 * 86_400.0), "vor 2 Monaten");
        assert_eq!(de(400.0 * 86_400.0), "vor 13 Monaten");
        let en = relative_age_in(Lang::En, 3.0 * 86_400.0);
        assert_ne!(en, "vor 3 Tagen");
        assert!(en.contains('3'), "{en}");
        assert_ne!(relative_age_in(Lang::En, 86_400.0), relative_age_in(Lang::En, 2.0 * 86_400.0));
    }

    /// `parse_table` turns a broken file into an empty table, so this is what
    /// notices it; the content of the files is checked in `tests/language_files.rs`.
    #[test]
    fn the_embedded_files_load() {
        let [de, en] = tables();
        assert!(de.texts.len() > 3000, "{}", de.texts.len());
        assert_eq!(de.texts.len(), en.texts.len());
        assert!(!de.fields.is_empty() && !de.properties.is_empty() && !de.properties_laser.is_empty());
        assert!(parse_table("{ not json").texts.is_empty());
    }

    #[test]
    fn fills_like_python() {
        let a = args(&[("0", 3.into()), ("1", 714.into()), ("2", 0.into())]);
        assert_eq!(tf_in(Lang::De, "von_gesamt", &a), "· 3 von 714 (0 %)");
        assert_eq!(tf_in(Lang::En, "von_gesamt", &a), "· 3 of 714 (0 %)");
        assert_eq!(t_in(Lang::En, "bauplaene"), "Blueprints");
        assert_eq!(t_in(Lang::De, "gibt_es_nicht"), "gibt_es_nicht");
        assert_eq!(fill_in(Lang::De, "{{x}} {a} {b}", &args(&[("a", "A".into())])), "{x} A {b}");
        assert_eq!(fill_in(Lang::De, "{a:.2f}", &args(&[("a", "text".into())])), "{a:.2f}");
        assert_eq!(fill_in(Lang::De, "x } y {", &[]), "x } y {");
        assert_eq!(fill_in(Lang::De, "{a:+}", &args(&[("a", (-2.7).into())])), "-2");
        assert_eq!(fill_in(Lang::De, "{a:+.1f}", &args(&[("a", 0.25.into())])), "+0.2");
    }

    #[test]
    fn numbers_like_python() {
        for (x, expected) in [
            (0.0, "0"),
            (-0.0, "-0"),
            (2.5, "2.5"),
            (100000.0, "100000"),
            (1000000.0, "1e+06"),
            (999999.5, "1e+06"),
            (123456789.0, "1.23457e+08"),
            (0.0001, "0.0001"),
            (0.00001, "1e-05"),
            (0.000012345, "1.2345e-05"),
            (1e100, "1e+100"),
            (f64::NAN, "nan"),
            (f64::NEG_INFINITY, "-inf"),
        ] {
            assert_eq!(py_g(x, false), expected, "{x}");
        }
        assert_eq!(py_g(1.5, true), "+1.5");
        assert_eq!(py_str_float(3.0), "3.0");
        assert_eq!(py_str_float(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(py_str_float(1e16), "1e+16");
        assert_eq!(py_fixed(2.5, 0, false), "2");
        assert_eq!(py_fixed(-0.4, 0, false), "-0");
        assert_eq!(py_fixed(1.005, 2, false), "1.00");
    }

    #[test]
    fn choice_from_settings() {
        let with = |v: Value| vec![("sprache".to_string(), v)];
        assert_eq!(choice_of(&with(Value::str(" EN\n"))), Choice::En);
        assert_eq!(choice_of(&with(Value::str("de"))), Choice::De);
        assert_eq!(choice_of(&with(Value::str("fr"))), Choice::Auto);
        assert_eq!(choice_of(&with(Value::int(1))), Choice::Auto);
        assert_eq!(choice_of(&Vec::new()), Choice::Auto);
    }

    #[test]
    fn system_language_from_environment() {
        let host = |vars: &[(&str, &str)]| Host::new(Flavor::Posix, vars, "/");
        assert_eq!(system_language(&host(&[("LANG", "de_DE.UTF-8")])), Lang::De);
        assert_eq!(system_language(&host(&[("SC_BP_SPRACHE", "en"), ("LANG", "de_DE")])), Lang::En);
        assert_eq!(system_language(&host(&[("SC_BP_SPRACHE", ""), ("LC_ALL", "DE")])), Lang::De);
        #[cfg(not(windows))]
        {
            assert_eq!(system_language(&host(&[("LC_CTYPE", "de_AT.UTF-8")])), Lang::De);
            assert_eq!(system_language(&host(&[])), Lang::En);
        }
        // Windows falls back to the user locale of the system and ignores LC_CTYPE.
        #[cfg(windows)]
        {
            let ident = os_locale(&host(&[])).to_lowercase();
            assert!(!ident.is_empty());
            let expected = if ident.starts_with("de") { Lang::De } else { Lang::En };
            assert_eq!(system_language(&host(&[("LC_CTYPE", "en_US.UTF-8")])), expected);
            assert_eq!(system_language(&host(&[])), expected);
        }
    }

    #[test]
    fn helpers() {
        assert_eq!(kind_label_in(Lang::En, "weapons"), "Personal weapon");
        assert_eq!(kind_label_in(Lang::De, "Neu"), "Neu");
        assert_eq!(kind_label_in(Lang::De, ""), "Sonstiges");
        assert_eq!(pa_field_in(Lang::De, "resource.states[3].flows"), "Energiefluss");
        assert_eq!(pa_field_in(Lang::En, "x.y[1]"), "x.y[1]");
        assert_eq!(property_name_in(Lang::De, "Damage", Some("weapon_damage"), true), "Laserleistung");
        assert_eq!(property_name_in(Lang::En, "Damage", Some("weapon_damage"), false), "Damage (Impact Force)");
        assert_eq!(property_name_in(Lang::En, "Fire Rate", Some("weapon_firerate"), false), "Fire Rate");
        assert_eq!(property_name_in(Lang::De, "Fire Rate", Some("weapon_firerate"), true), "Feuerrate");
        let host = Host::new(Flavor::Posix, &[("SC_BP_TESTFASSUNG", "0")], "/");
        assert_eq!(window_title_in(Lang::De, "Verse-Kit", &host), "Verse-Kit");
    }

    #[test]
    fn phrase_renders_nested_values() {
        let inner = Phrase::new("bauplaene");
        let phrase = Phrase::new("ab_rang").with("0", inner);
        assert_eq!(phrase.render(Lang::De), "ab Baupläne");
        assert_eq!(phrase.render(Lang::En), "from Blueprints");
    }

    /// The language of one thread decides what that thread reads, nests, goes
    /// back when the work is over, and never reaches another thread.
    #[test]
    fn a_thread_reads_its_own_language() {
        for (first, second) in [(Lang::De, Lang::En), (Lang::En, Lang::De)] {
            with_language(first, || {
                assert_eq!(current(), first);
                with_language(second, || {
                    assert_eq!(current(), second);
                    let other = std::thread::spawn(move || with_language(first, current)).join().expect("thread");
                    assert_eq!(other, first, "the other thread has its own");
                    assert_eq!(current(), second, "and did not touch this one");
                });
                assert_eq!(current(), first, "the nested work gave the language back");
            });
        }
        set_thread_language(Choice::En);
        assert_eq!(current(), Lang::En);
        set_thread_language(Choice::De);
        assert_eq!(current(), Lang::De);
        set_thread_language(Choice::Auto);
    }
}
