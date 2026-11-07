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

//! Reading and writing JSON files, byte for byte like Python's `json` with
//! `ensure_ascii=False`, plus atomic writing, the previous-version backup and
//! setting broken files aside.
//!
//! ⚠ Own reader and writer instead of `serde_json`: the files contain
//! `NaN`, `Infinity` and `-Infinity` (Python writes them, `serde_json` rejects
//! them), integers of any length and duplicate keys. Floating-point numbers
//! are written like Python's `repr` (`1e+16`, `1.5e-07`, `100.0`).
//!
//! ⚠ Python writes in text mode: on Windows every `\n` becomes `\r\n`.
//! `save_json` reproduces this, otherwise every file would be rewritten in
//! full after the first save.

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, RwLock};
use std::thread::{self, ThreadId};
use std::time::Duration;

use crate::errors;
use crate::pypath::Flavor;
use crate::writeguard;

/// A JSON value, as returned by Python's `json.loads`.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    /// Integer of any length, as decimal text without `-0`.
    Int(String),
    Float(f64),
    Str(String),
    Array(Vec<Value>),
    /// Keys in insertion order, each only once.
    Object(Map),
}

pub type Map = Vec<(String, Value)>;

impl Value {
    /// Value for a key, if this is an object.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(map) => map.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&Map> {
        match self {
            Value::Object(map) => Some(map),
            _ => None,
        }
    }

    pub fn str(s: impl Into<String>) -> Value {
        Value::Str(s.into())
    }

    pub fn int(n: i64) -> Value {
        Value::Int(n.to_string())
    }
}

/// Like `dict[key] = value`: replaces in place or appends at the end.
pub fn map_set(map: &mut Map, key: &str, value: Value) {
    match map.iter_mut().find(|(k, _)| k == key) {
        Some(slot) => slot.1 = value,
        None => map.push((key.to_string(), value)),
    }
}

/// A number as Python compares it: `true` is `1`.
fn py_number(value: &Value) -> Option<f64> {
    match value {
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        Value::Int(text) => text.parse().ok(),
        Value::Float(x) => Some(*x),
        _ => None,
    }
}

/// `a == b` in Python: `1`, `1.0` and `true` are equal, objects ignore key order.
pub fn py_eq_value(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Null, Value::Null) => true,
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(x, y)| py_eq_value(x, y)),
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter().all(|(k, v)| y.iter().find(|(l, _)| l == k).is_some_and(|(_, w)| py_eq_value(v, w)))
        }
        _ => matches!((py_number(a), py_number(b)), (Some(x), Some(y)) if x == y),
    }
}

/// `a == b` in Python for two `dict.get` results: a missing value is `None`.
pub fn py_eq(a: Option<&Value>, b: Option<&Value>) -> bool {
    py_eq_value(a.unwrap_or(&Value::Null), b.unwrap_or(&Value::Null))
}

// ---------------------------------------------------------------- Reading

/// Why a text is not JSON.
#[derive(Clone, Debug, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub pos: usize,
    /// Nested too deeply: in Python a `RecursionError`, not a `ValueError`.
    pub too_deep: bool,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (Byte {})", self.message, self.pos)
    }
}

impl std::error::Error for ParseError {}

/// From this depth on, Python gives up with `RecursionError`.
const MAX_DEPTH: usize = 900;
/// Python rejects longer integers (`sys.get_int_max_str_digits`).
const MAX_INT_DIGITS: usize = 4300;

/// Like `json.loads(text)`.
///
/// ⚠ A lone surrogate half (`\ud800` without its partner) cannot be held by a
/// Rust string; it becomes `U+FFFD`.
pub fn loads(text: &str) -> Result<Value, ParseError> {
    let mut p = Parser { s: text.as_bytes(), pos: 0, depth: 0 };
    if text.starts_with('\u{feff}') {
        return Err(p.err("Unexpected UTF-8 BOM (decode using utf-8-sig)"));
    }
    p.ws();
    let value = p.value()?;
    p.ws();
    if p.pos != p.s.len() {
        return Err(p.err("Extra data"));
    }
    Ok(value)
}

struct Parser<'a> {
    s: &'a [u8],
    pos: usize,
    depth: usize,
}

impl Parser<'_> {
    fn err(&self, message: &str) -> ParseError {
        ParseError { message: message.to_string(), pos: self.pos, too_deep: false }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.pos).copied()
    }

    fn rest_starts(&self, word: &[u8]) -> bool {
        self.s[self.pos..].starts_with(word)
    }

    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn literal(&mut self, word: &[u8], value: Value) -> Result<Value, ParseError> {
        self.pos += word.len();
        Ok(value)
    }

    fn value(&mut self) -> Result<Value, ParseError> {
        let literal = Self::literal;
        match self.peek() {
            Some(b'"') => {
                self.pos += 1;
                Ok(Value::Str(self.string()?))
            }
            Some(b'{') => {
                self.pos += 1;
                self.nested(Self::object)
            }
            Some(b'[') => {
                self.pos += 1;
                self.nested(Self::array)
            }
            Some(b'n') if self.rest_starts(b"null") => literal(self, b"null", Value::Null),
            Some(b't') if self.rest_starts(b"true") => literal(self, b"true", Value::Bool(true)),
            Some(b'f') if self.rest_starts(b"false") => literal(self, b"false", Value::Bool(false)),
            Some(b'N') if self.rest_starts(b"NaN") => literal(self, b"NaN", Value::Float(f64::NAN)),
            Some(b'I') if self.rest_starts(b"Infinity") => literal(self, b"Infinity", Value::Float(f64::INFINITY)),
            Some(b'-') if self.rest_starts(b"-Infinity") => {
                literal(self, b"-Infinity", Value::Float(f64::NEG_INFINITY))
            }
            _ => self.number(),
        }
    }

    fn nested(&mut self, inner: fn(&mut Self) -> Result<Value, ParseError>) -> Result<Value, ParseError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(ParseError { too_deep: true, ..self.err("maximum recursion depth exceeded") });
        }
        let result = inner(self);
        self.depth -= 1;
        result
    }

    fn digits(&mut self) {
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
    }

    /// `-?(0|[1-9]\d*)(\.\d+)?([eE][-+]?\d+)?`, like Python's number scanner.
    fn number(&mut self) -> Result<Value, ParseError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        match self.peek() {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => self.digits(),
            _ => {
                self.pos = start;
                return Err(self.err("Expecting value"));
            }
        }
        let mut is_float = false;
        if self.peek() == Some(b'.') && matches!(self.s.get(self.pos + 1), Some(b'0'..=b'9')) {
            self.pos += 1;
            self.digits();
            is_float = true;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            let e_start = self.pos;
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if matches!(self.peek(), Some(b'0'..=b'9')) {
                self.digits();
                is_float = true;
            } else {
                self.pos = e_start;
            }
        }
        let text = std::str::from_utf8(&self.s[start..self.pos]).expect("nur ASCII");
        if is_float {
            return text.parse::<f64>().map(Value::Float).map_err(|_| self.err("Expecting value"));
        }
        let digits = text.trim_start_matches('-');
        if digits.len() > MAX_INT_DIGITS {
            return Err(self.err("Exceeds the limit (4300 digits) for integer string conversion"));
        }
        Ok(Value::Int(if digits == "0" { "0".into() } else { text.to_string() }))
    }

    fn hex4(&self, at: usize) -> Option<u32> {
        let chunk = self.s.get(at..at + 4)?;
        let text = std::str::from_utf8(chunk).ok()?;
        if !text.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        u32::from_str_radix(text, 16).ok()
    }

    /// A string after the opening `"`. Control characters are forbidden
    /// (`strict=True`).
    fn string(&mut self) -> Result<String, ParseError> {
        let mut out = String::new();
        loop {
            let start = self.pos;
            while let Some(b) = self.peek() {
                if b == b'"' || b == b'\\' || b < 0x20 {
                    break;
                }
                self.pos += 1;
            }
            out.push_str(std::str::from_utf8(&self.s[start..self.pos]).expect("Grenzen auf ASCII"));
            match self.peek() {
                None => return Err(self.err("Unterminated string starting at")),
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => self.escape(&mut out)?,
                Some(_) => return Err(self.err("Invalid control character at")),
            }
        }
    }

    fn escape(&mut self, out: &mut String) -> Result<(), ParseError> {
        let Some(code) = self.s.get(self.pos + 1).copied() else {
            return Err(self.err("Unterminated string starting at"));
        };
        let simple = match code {
            b'"' => Some('"'),
            b'\\' => Some('\\'),
            b'/' => Some('/'),
            b'b' => Some('\u{8}'),
            b'f' => Some('\u{c}'),
            b'n' => Some('\n'),
            b'r' => Some('\r'),
            b't' => Some('\t'),
            b'u' => None,
            _ => return Err(self.err("Invalid \\escape")),
        };
        if let Some(c) = simple {
            out.push(c);
            self.pos += 2;
            return Ok(());
        }
        let Some(mut c) = self.hex4(self.pos + 2) else {
            return Err(self.err("Invalid \\uXXXX escape"));
        };
        self.pos += 6;
        if (0xd800..=0xdbff).contains(&c) && self.pos + 6 < self.s.len() && self.rest_starts(b"\\u") {
            let Some(c2) = self.hex4(self.pos + 2) else {
                return Err(self.err("Invalid \\uXXXX escape"));
            };
            if (0xdc00..=0xdfff).contains(&c2) {
                c = 0x10000 + ((c - 0xd800) << 10) + (c2 - 0xdc00);
                self.pos += 6;
            }
        }
        out.push(char::from_u32(c).unwrap_or('\u{fffd}'));
        Ok(())
    }

    fn object(&mut self) -> Result<Value, ParseError> {
        let mut map: Map = Vec::new();
        let mut index: HashMap<String, usize> = HashMap::new();
        self.ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Value::Object(map));
        }
        loop {
            if self.peek() != Some(b'"') {
                return Err(self.err("Expecting property name enclosed in double quotes"));
            }
            self.pos += 1;
            let key = self.string()?;
            self.ws();
            if self.peek() != Some(b':') {
                return Err(self.err("Expecting ':' delimiter"));
            }
            self.pos += 1;
            self.ws();
            let value = self.value()?;
            // Duplicate key: position of the first, value of the last, like dict.
            match index.get(&key) {
                Some(&i) => map[i].1 = value,
                None => {
                    index.insert(key.clone(), map.len());
                    map.push((key, value));
                }
            }
            self.ws();
            match self.peek() {
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Value::Object(map));
                }
                Some(b',') => {
                    self.pos += 1;
                    self.ws();
                }
                _ => return Err(self.err("Expecting ',' delimiter")),
            }
        }
    }

    fn array(&mut self) -> Result<Value, ParseError> {
        let mut items = Vec::new();
        self.ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Value::Array(items));
        }
        loop {
            items.push(self.value()?);
            self.ws();
            match self.peek() {
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Value::Array(items));
                }
                Some(b',') => {
                    self.pos += 1;
                    self.ws();
                }
                _ => return Err(self.err("Expecting ',' delimiter")),
            }
        }
    }
}

// ---------------------------------------------------------------- Writing

/// Digits and decimal exponent from Rust's `{:e}` form (`1.5e-7` → `15`, -7).
fn split_sci(sci: &str) -> (String, i32) {
    let (mantissa, exp) = sci.split_once('e').expect("Exponent");
    let digits = mantissa.trim_start_matches('-').chars().filter(|c| *c != '.').collect();
    (digits, exp.parse().expect("Exponent ist eine Zahl"))
}

/// The first `sig` digits of the exact value (Rust rounds at the last one).
fn exact_digits(x: f64, sig: usize) -> (String, i32) {
    split_sci(&format!("{:.*e}", sig - 1, x))
}

/// Round `all` to `n` digits, exactly at the midpoint to the even digit.
fn round_half_even(all: &str, exp: i32, n: usize) -> (String, i32) {
    let (head, tail) = all.split_at(n);
    let tail = tail.as_bytes();
    let rest_zero = tail[1..].iter().all(|&b| b == b'0');
    let last_odd = head.as_bytes()[n - 1] % 2 == 1;
    let up = tail[0] > b'5' || (tail[0] == b'5' && (!rest_zero || last_odd));
    if !up {
        return (head.to_string(), exp);
    }
    let mut d = head.as_bytes().to_vec();
    for i in (0..n).rev() {
        if d[i] == b'9' {
            d[i] = b'0';
        } else {
            d[i] += 1;
            return (String::from_utf8(d).expect("Ziffern"), exp);
        }
    }
    d.insert(0, b'1');
    d.pop();
    (String::from_utf8(d).expect("Ziffern"), exp + 1)
}

/// Shortest digits that read back as `x`; among equally short ones the
/// one nearest to the exact value, on a tie the even one.
///
/// ⚠ Rust's `{:e}` finds the shortest length, but rounds up exactly at the
/// midpoint: `-101199904575609.625` becomes `…609.63` there, `…609.62` in Python.
fn repr_digits(x: f64) -> (String, i32) {
    let x = x.abs();
    let (shortest, exp) = split_sci(&format!("{x:e}"));
    if x == 0.0 {
        return (shortest, exp);
    }
    let n = shortest.len();
    let (mut all, mut all_exp) = exact_digits(x, n + 25);
    let tail = &all.as_bytes()[n..];
    let near_middle = matches!(tail[0], b'4' | b'5')
        && (tail[1..].iter().all(|&b| b == b'0') || tail[1..].iter().all(|&b| b == b'9'));
    if near_middle {
        // Only the full digits show whether it is really exactly the midpoint.
        (all, all_exp) = exact_digits(x, 800);
    }
    let (nearest, nearest_exp) = round_half_even(&all, all_exp, n);
    let text = format!("{}.{}e{nearest_exp}", &nearest[..1], &nearest[1..]);
    if text.parse::<f64>() == Ok(x) { (nearest, nearest_exp) } else { (shortest, exp) }
}

/// Floating-point number like `json.dumps`: `float.__repr__`, plus `NaN`,
/// `Infinity`, `-Infinity`.
///
/// `repr_digits` picks the digits like Python. So does the format: exponent
/// from 1e16 and below 1e-4, otherwise written out with at least one decimal
/// place.
pub fn float_text(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    let sign = if x.is_sign_negative() { "-" } else { "" };
    let (digits, exp) = repr_digits(x);
    let n = digits.len() as i32;
    let decpt = exp + 1;
    let body = if decpt <= -4 || decpt > 16 {
        let (first, rest) = digits.split_at(1);
        let dot = if rest.is_empty() { String::new() } else { format!(".{rest}") };
        format!("{first}{dot}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs())
    } else if decpt <= 0 {
        format!("0.{}{digits}", "0".repeat((-decpt) as usize))
    } else if decpt < n {
        let (a, b) = digits.split_at(decpt as usize);
        format!("{a}.{b}")
    } else {
        format!("{digits}{}.0", "0".repeat((decpt - n) as usize))
    };
    format!("{sign}{body}")
}

/// String like `json.dumps(…, ensure_ascii=False)`.
pub(crate) fn push_json_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Like `json.dumps(value, ensure_ascii=False, indent=indent, sort_keys=…)`.
///
/// With indentation, `,` separates at the end of the line, without it `, `;
/// the key is always followed by `: ` before the value. Empty containers stay
/// `{}` and `[]`.
pub fn dumps(value: &Value, indent: Option<usize>, sort_keys: bool) -> String {
    let mut out = String::new();
    encode(&mut out, value, indent, sort_keys, 0);
    out
}

fn encode(out: &mut String, value: &Value, indent: Option<usize>, sort_keys: bool, level: usize) {
    let newline = |out: &mut String, level: usize| {
        if let Some(width) = indent {
            out.push('\n');
            out.push_str(&" ".repeat(width * level));
        }
    };
    let separator = if indent.is_some() { "," } else { ", " };
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(n) => out.push_str(n),
        Value::Float(x) => out.push_str(&float_text(*x)),
        Value::Str(s) => push_json_str(out, s),
        Value::Array(items) if items.is_empty() => out.push_str("[]"),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(separator);
                }
                newline(out, level + 1);
                encode(out, item, indent, sort_keys, level + 1);
            }
            newline(out, level);
            out.push(']');
        }
        Value::Object(map) if map.is_empty() => out.push_str("{}"),
        Value::Object(map) => {
            let mut entries: Vec<&(String, Value)> = map.iter().collect();
            if sort_keys {
                entries.sort_by(|a, b| a.0.cmp(&b.0));
            }
            out.push('{');
            for (i, (key, item)) in entries.into_iter().enumerate() {
                if i > 0 {
                    out.push_str(separator);
                }
                newline(out, level + 1);
                push_json_str(out, key);
                out.push_str(": ");
                encode(out, item, indent, sort_keys, level + 1);
            }
            newline(out, level);
            out.push('}');
        }
    }
}

// ---------------------------------------------------------------- Error kinds

/// How Python classifies an error: `FileNotFoundError`.
pub fn is_not_found(e: &io::Error) -> bool {
    #[cfg(windows)]
    if let Some(code) = e.raw_os_error() {
        return matches!(code, 2 | 3 | 15 | 18 | 53 | 67 | 161 | 206);
    }
    e.kind() == io::ErrorKind::NotFound
}

/// How Python classifies an error: `PermissionError`: on Windows, locked
/// files count as well.
pub fn is_permission(e: &io::Error) -> bool {
    #[cfg(windows)]
    if let Some(code) = e.raw_os_error() {
        return matches!(code, 5 | 16 | 19..=34 | 36 | 65 | 82 | 83 | 108 | 132 | 158 | 167);
    }
    e.kind() == io::ErrorKind::PermissionDenied
}

/// Why a file could not be read, in Python's classification.
#[derive(Debug)]
pub enum ReadError {
    /// `FileNotFoundError`
    NotFound,
    /// `ValueError`: not UTF-8, not JSON, unexpected structure.
    Value(String),
    /// `RecursionError`: nested too deeply.
    TooDeep(String),
    /// Any other `OSError` (locked, a folder, no access).
    Os(io::Error),
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadError::NotFound => write!(f, "Datei fehlt"),
            ReadError::Value(m) | ReadError::TooDeep(m) => write!(f, "{m}"),
            ReadError::Os(e) => write!(f, "{e}"),
        }
    }
}

impl From<io::Error> for ReadError {
    fn from(e: io::Error) -> Self {
        if is_not_found(&e) { ReadError::NotFound } else { ReadError::Os(e) }
    }
}

impl From<ParseError> for ReadError {
    fn from(e: ParseError) -> Self {
        if e.too_deep { ReadError::TooDeep(e.to_string()) } else { ReadError::Value(e.to_string()) }
    }
}

/// Text of a file, strictly as UTF-8 (like `open(…, encoding='utf-8')`).
pub fn read_text(path: &str) -> Result<String, ReadError> {
    let bytes = fs::read(path)?;
    String::from_utf8(bytes).map_err(|e| ReadError::Value(e.to_string()))
}

/// Read a JSON file (like `json.load`), without fallback.
pub fn read_json(path: &str) -> Result<Value, ReadError> {
    Ok(loads(&read_text(path)?)?)
}

// ---------------------------------------------------------------- Files

/// Line endings when writing, like Python's `newline=` in `open(…, 'w')`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Newline {
    /// `None`: `\n` becomes the system's line ending (Windows `\r\n`).
    Native,
    /// `''` or `'\n'`: unchanged.
    Keep,
    /// `'\r\n'`
    CrLf,
}

/// `text` as a Python text-mode file would write it.
pub fn translate(text: &str, newline: Newline) -> String {
    match newline {
        Newline::Native if cfg!(windows) => text.replace('\n', "\r\n"),
        Newline::CrLf => text.replace('\n', "\r\n"),
        _ => text.to_string(),
    }
}

/// What `json.dump(value, f, ensure_ascii=False, indent=indent)` writes to a
/// file opened in text mode.
pub fn file_bytes(value: &Value, indent: usize) -> Vec<u8> {
    translate(&dumps(value, Some(indent), false), Newline::Native).into_bytes()
}

/// `bestand.json` → `bestand.bak.json`; without `.json`, `.bak` is appended.
pub fn backup_name(target: &str) -> String {
    match target.strip_suffix(".json") {
        Some(stem) => format!("{stem}.bak.json"),
        None => format!("{target}.bak"),
    }
}

/// Local time as `YYYYMMDD-HHMMSS`, like `time.strftime('%Y%m%d-%H%M%S')`.
pub fn time_stamp() -> String {
    let (y, mo, d, h, mi, s) = local_time();
    format!("{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}")
}

/// Local time as year, month, day, hour, minute, second.
#[cfg(windows)]
pub(crate) fn local_time() -> (u32, u32, u32, u32, u32, u32) {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    let mut t =
        SYSTEMTIME { wYear: 0, wMonth: 0, wDayOfWeek: 0, wDay: 0, wHour: 0, wMinute: 0, wSecond: 0, wMilliseconds: 0 };
    // SAFETY: GetLocalTime only fills the structure passed in.
    unsafe { GetLocalTime(&mut t) };
    (t.wYear.into(), t.wMonth.into(), t.wDay.into(), t.wHour.into(), t.wMinute.into(), t.wSecond.into())
}

#[cfg(unix)]
pub(crate) fn local_time() -> (u32, u32, u32, u32, u32, u32) {
    // SAFETY: time and localtime_r only write to the values passed in.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&now, &mut tm);
        (
            (tm.tm_year + 1900) as u32,
            (tm.tm_mon + 1) as u32,
            tm.tm_mday as u32,
            tm.tm_hour as u32,
            tm.tm_min as u32,
            tm.tm_sec as u32,
        )
    }
}

/// Null-terminated UTF-16 for Windows calls.
#[cfg(windows)]
pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A file path for direct Windows calls: from 248 characters on, made
/// absolute and prefixed with `\\?\`.
///
/// ⚠ Without the prefix, long paths fail at the old limit of 260 characters
/// (`MoveFileExW` reports "not found"); `std::fs` and Python with long paths
/// enabled cope with them.
#[cfg(windows)]
pub(crate) fn wide_path(p: &str) -> Vec<u16> {
    if p.encode_utf16().count() < 248 || p.starts_with(r"\\?\") {
        return wide(p);
    }
    let cwd = std::env::current_dir().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
    let full = Flavor::Windows.abspath(p, &cwd);
    match full.strip_prefix(r"\\") {
        Some(unc) => wide(&format!(r"\\?\UNC\{unc}")),
        None => wide(&format!(r"\\?\{full}")),
    }
}

/// `os.replace`: rename and replace an existing target.
pub fn os_replace(source: &str, target: &str) -> io::Result<()> {
    writeguard::check("replace", target)?;
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_REPLACE_EXISTING, MoveFileExW};
        let (s, t) = (wide_path(source), wide_path(target));
        // SAFETY: both pointers point to null-terminated UTF-16 buffers.
        if unsafe { MoveFileExW(s.as_ptr(), t.as_ptr(), MOVEFILE_REPLACE_EXISTING) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    fs::rename(source, target)
}

/// `os.remove`, blocked like any write access.
pub fn remove_file(path: &str) -> io::Result<()> {
    writeguard::check("remove", path)?;
    fs::remove_file(path)
}

/// `os.makedirs(folder, exist_ok=True)`.
///
/// ⚠ Consults the write guard even for an existing folder: Python's `makedirs`
/// also attempts a `mkdir` there.
pub fn make_dirs(folder: &str) -> io::Result<()> {
    writeguard::check("makedirs", folder)?;
    fs::create_dir_all(folder)
}

/// Create a file anew or truncate it, and write the text.
pub fn write_new(path: &str, bytes: &[u8]) -> io::Result<()> {
    writeguard::check("open-w", path)?;
    let mut file = fs::File::create(path)?;
    file.write_all(bytes)
}

/// `shutil.copy2`: copy the content together with the modification time.
pub fn copy2(source: &str, target: &str) -> io::Result<()> {
    writeguard::check("copy2", target)?;
    fs::copy(source, target)?;
    #[cfg(not(windows))]
    {
        let meta = fs::metadata(source)?;
        let times = fs::FileTimes::new().set_accessed(meta.accessed()?).set_modified(meta.modified()?);
        fs::File::options().write(true).open(target)?.set_times(times)?;
    }
    Ok(())
}

/// `os.replace` with brief retries while Windows locks the file.
///
/// ⚠ On Windows the rename fails while another process (usually the virus
/// scanner) has the target file open. Only this case is retried; anything
/// else (disk full, folder gone) fails immediately.
pub fn replace_file(source: &str, target: &str) -> io::Result<()> {
    replace_file_with(source, target, 10, Duration::from_millis(50))
}

pub fn replace_file_with(source: &str, target: &str, attempts: u32, pause: Duration) -> io::Result<()> {
    for attempt in 0..attempts {
        match os_replace(source, target) {
            Ok(()) => return Ok(()),
            Err(e) if is_permission(&e) && attempt + 1 < attempts => thread::sleep(pause),
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Remove the attributes that make overwriting fail (read-only, hidden,
/// system). Windows only; true if one was removed.
pub fn make_overwritable(path: &str) -> bool {
    if writeguard::check("attributes", path).is_err() {
        return false;
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileAttributesW, INVALID_FILE_ATTRIBUTES, SetFileAttributesW,
        };
        const BLOCKING: u32 = 0x1 | 0x2 | 0x4;
        let name = wide_path(path);
        // SAFETY: null-terminated UTF-16 buffer.
        let attrs = unsafe { GetFileAttributesW(name.as_ptr()) };
        if attrs == INVALID_FILE_ATTRIBUTES || attrs & BLOCKING == 0 {
            return false;
        }
        let cleared = attrs & !BLOCKING;
        // SAFETY: as above.
        unsafe { SetFileAttributesW(name.as_ptr(), if cleared == 0 { 0x80 } else { cleared }) != 0 }
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

thread_local! {
    static THREAD_NUMBER: u64 = {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        NEXT.fetch_add(1, Ordering::Relaxed)
    };
}

/// One temporary file per process and thread, so two writers of the same file
/// never share one.
pub fn temp_name(target: &str) -> String {
    format!("{target}.{}-{}.tmp", std::process::id(), THREAD_NUMBER.with(|n| *n))
}

/// Write text without ever leaving the file half-written or empty: first a
/// temporary file, then rename. If anything fails, the target stays
/// unchanged and the error is returned.
pub fn write_text_atomic(target: &str, text: &str, newline: Newline) -> io::Result<()> {
    let temp = temp_name(target);
    let result = (|| {
        write_new(&temp, translate(text, newline).as_bytes())?;
        match replace_file(&temp, target) {
            Err(e) if is_permission(&e) => {
                if !make_overwritable(target) {
                    return Err(e);
                }
                replace_file(&temp, target)
            }
            other => other,
        }
    })();
    if result.is_err() {
        let _ = remove_file(&temp);
    }
    result
}

/// How long a process waits for the write lock of another process before it goes on without it.
/// The holders keep it for milliseconds, so after this long the other process is stuck.
pub const PROCESS_WAIT: Duration = Duration::from_secs(15);

/// How often a waiting process asks the file lock again.
const PROCESS_POLL: Duration = Duration::from_millis(20);

/// The write lock across the processes of one data set: an operating system lock on a lock file that
/// nobody reads or writes. The system lets go of it when the process ends, also when it crashes or is killed,
/// so no lock is ever left behind. (`LockFileEx` on Windows, `flock` on Linux and macOS.)
///
/// Two handles of the same file lock each other, in one process as well as between two.
pub struct ProcessLock {
    file: fs::File,
}

/// What came of asking a [`ProcessLock`].
#[derive(Debug)]
pub enum Taken {
    /// This handle holds the lock now.
    Held,
    /// Someone else still held it when the time was up.
    TimedOut,
    /// The file system cannot lock (not "occupied"): do not ask again.
    Unsupported(io::Error),
}

impl ProcessLock {
    /// Open (and, where it is missing, create) the lock file together with its folder.
    pub fn open(path: &str) -> io::Result<ProcessLock> {
        writeguard::check("open-w", path)?;
        let folder = Flavor::native().dirname(path);
        if !folder.is_empty() {
            make_dirs(&folder)?;
        }
        let file = fs::OpenOptions::new().create(true).truncate(false).write(true).open(path)?;
        Ok(ProcessLock { file })
    }

    /// Ask until the lock is free or `limit` has passed.
    pub fn acquire(&self, limit: Duration) -> Taken {
        let start = std::time::Instant::now();
        loop {
            match self.file.try_lock() {
                Ok(()) => return Taken::Held,
                Err(fs::TryLockError::WouldBlock) => {
                    if start.elapsed() >= limit {
                        return Taken::TimedOut;
                    }
                    thread::sleep(PROCESS_POLL);
                }
                Err(fs::TryLockError::Error(e)) => return Taken::Unsupported(e),
            }
        }
    }

    /// Let go of the lock this handle holds.
    pub fn release(&self) {
        let _ = self.file.unlock();
    }
}

/// The lock file this process shares with the others, once [`share_saves_between_processes`] has set it.
static PROCESS_LOCK: RwLock<Option<ProcessLock>> = RwLock::new(None);

/// How long the process waits, in milliseconds ([`PROCESS_WAIT`], unless a check shortens it).
static PROCESS_WAIT_MS: AtomicU64 = AtomicU64::new(PROCESS_WAIT.as_millis() as u64);

/// Shorten or restore the wait of this process for the lock of another one (for checks).
pub fn set_process_wait(limit: Duration) {
    PROCESS_WAIT_MS.store(u64::try_from(limit.as_millis()).unwrap_or(u64::MAX), Ordering::Relaxed);
}

/// Called once at the start of each program: from now on the write lock is shared with every other process
/// that names the same file. Gives `false` where the file cannot be opened (a write lock for test runs on real
/// data, a folder that cannot be made): the lock then stays inside this process, and one report says so.
pub fn share_saves_between_processes(lock_file: &str) -> bool {
    match ProcessLock::open(lock_file) {
        Ok(lock) => {
            *PROCESS_LOCK.write().unwrap_or_else(|e| e.into_inner()) = Some(lock);
            true
        }
        Err(e) => {
            errors::record("jsonio.sperre", &format!("lock file {lock_file}: {e}"));
            false
        }
    }
}

/// One writer at a time, also across nested calls in the same thread
/// (like Python's `RLock`).
struct SaveLock {
    state: Mutex<SaveState>,
    free: Condvar,
}

struct SaveState {
    owner: Option<ThreadId>,
    depth: usize,
    /// The outermost call holds the lock of the process pair too.
    shared: bool,
}

static SAVE_LOCK: SaveLock =
    SaveLock { state: Mutex::new(SaveState { owner: None, depth: 0, shared: false }), free: Condvar::new() };

/// Holds the write lock until it is dropped.
pub struct SaveGuard(());

/// Take the write lock; the same thread may hold it multiple times.
///
/// With [`share_saves_between_processes`] set, the outermost call takes the lock of the other processes as
/// well, after the one of the threads, and lets go of it first. It waits for it at most [`PROCESS_WAIT`]:
/// then it goes on without it and reports `jsonio.sperre`, so a stuck process never freezes the other one.
pub fn lock_saves() -> SaveGuard {
    let me = thread::current().id();
    let mut state = SAVE_LOCK.state.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        match state.owner {
            None => {
                state.owner = Some(me);
                state.depth = 1;
                state.shared = false;
                break;
            }
            Some(owner) if owner == me => {
                state.depth += 1;
                return SaveGuard(());
            }
            Some(_) => state = SAVE_LOCK.free.wait(state).unwrap_or_else(|e| e.into_inner()),
        }
    }
    // The entry of the owner is set, so the other threads wait at the condition: the mutex is free while
    // this one waits for the file.
    drop(state);
    let shared = take_process_lock();
    SAVE_LOCK.state.lock().unwrap_or_else(|e| e.into_inner()).shared = shared;
    SaveGuard(())
}

/// Take the lock of the processes, if there is one. `true` where this thread holds it now.
fn take_process_lock() -> bool {
    let guard = PROCESS_LOCK.read().unwrap_or_else(|e| e.into_inner());
    let Some(lock) = guard.as_ref() else {
        return false;
    };
    let limit = Duration::from_millis(PROCESS_WAIT_MS.load(Ordering::Relaxed));
    match lock.acquire(limit) {
        Taken::Held => true,
        Taken::TimedOut => {
            errors::record("jsonio.sperre", &format!("another process held the write lock for {limit:?}, going on without it"));
            false
        }
        Taken::Unsupported(e) => {
            drop(guard);
            *PROCESS_LOCK.write().unwrap_or_else(|e| e.into_inner()) = None;
            errors::record("jsonio.sperre", &format!("the file system does not lock files, the lock stays in this process: {e}"));
            false
        }
    }
}

impl Drop for SaveGuard {
    fn drop(&mut self) {
        let mut state = SAVE_LOCK.state.lock().unwrap_or_else(|e| e.into_inner());
        state.depth -= 1;
        if state.depth == 0 {
            if state.shared
                && let Some(lock) = PROCESS_LOCK.read().unwrap_or_else(|e| e.into_inner()).as_ref()
            {
                lock.release();
            }
            state.owner = None;
            state.shared = false;
            SAVE_LOCK.free.notify_all();
        }
    }
}

/// Write JSON atomically and with a previous-version backup (`….bak.json`,
/// copied, not moved: a main file exists at every point in time).
///
/// Reports nothing itself; if anything fails, the temporary file is removed
/// and the error is returned.
pub fn save_json(target: &str, data: &Value, indent: usize, sort_keys: bool) -> io::Result<()> {
    let temp = temp_name(target);
    let _lock = lock_saves();
    let result = (|| {
        let folder = Flavor::native().dirname(target);
        if !folder.is_empty() {
            make_dirs(&folder)?;
        }
        let text = dumps(data, Some(indent), sort_keys);
        write_new(&temp, translate(&text, Newline::Native).as_bytes())?;
        if Path::new(target).exists() {
            // The backup is the belt, the atomic write the suspenders: if the
            // backup fails, the write goes ahead anyway.
            let _ = copy2(target, &backup_name(target));
        }
        replace_file(&temp, target)
    })();
    if result.is_err() {
        let _ = remove_file(&temp);
    }
    result
}

/// Checks whether the data read has the expected structure.
pub type Validator<'a> = &'a dyn Fn(&Value) -> bool;

/// Read JSON; for a broken file, the previous-version backup.
///
/// If the file is missing, the result is `None` (unless a backup sits next to
/// it). If it is broken (not JSON, or `valid` says no), that is reported, the
/// file is set aside as `….kaputt-<time>.json` (otherwise the next save would
/// overwrite it) and the backup is read. If that is no good either: `None`.
///
/// ⚠ Nested too deeply, Python aborts with `RecursionError`; here that is
/// reported, `None` is returned and the file is left untouched.
pub fn load_json(target: &str, valid: Option<Validator<'_>>, report_as: &str) -> Option<Value> {
    let read = |path: &str| -> Result<Value, ReadError> {
        let data = read_json(path)?;
        if let Some(check) = valid
            && !check(&data)
        {
            return Err(ReadError::Value("unerwarteter Aufbau".into()));
        }
        Ok(data)
    };
    let backup = backup_name(target);
    match read(target) {
        Ok(data) => return Some(data),
        Err(ReadError::NotFound) => {
            if !Path::new(&backup).is_file() {
                return None;
            }
        }
        Err(ReadError::Value(message)) => {
            errors::record(report_as, &message);
            let aside = match target.strip_suffix(".json") {
                Some(stem) => format!("{stem}.kaputt-{}.json", time_stamp()),
                None => format!("{target}.kaputt-{}", time_stamp()),
            };
            if let Err(e) = os_replace(target, &aside) {
                errors::record(report_as, &e);
            }
        }
        Err(other) => {
            errors::record(report_as, &other);
            return None;
        }
    }
    match read(&backup) {
        Ok(data) => Some(data),
        Err(ReadError::NotFound) => None,
        Err(other) => {
            errors::record(report_as, &other);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(text: &str, indent: Option<usize>) -> String {
        dumps(&loads(text).expect("gültiges JSON"), indent, false)
    }

    #[test]
    fn floats_like_python_repr() {
        let cases = [
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (1.0, "1.0"),
            (100.0, "100.0"),
            (0.1, "0.1"),
            (0.0001, "0.0001"),
            (0.00001, "1e-05"),
            (1.5e-7, "1.5e-07"),
            (1e15, "1000000000000000.0"),
            (1e16, "1e+16"),
            (123456789012345678.0, "1.2345678901234568e+17"),
            (1e100, "1e+100"),
            (5e-324, "5e-324"),
            (1.7976931348623157e308, "1.7976931348623157e+308"),
            (2.5, "2.5"),
            (1234.5678, "1234.5678"),
            (f64::NAN, "NaN"),
            (f64::INFINITY, "Infinity"),
            (f64::NEG_INFINITY, "-Infinity"),
            // Exactly midway between two equally short digit sequences: the even one.
            (-(101_199_904_575_609.0 + 0.625), "-101199904575609.62"),
            (101_199_904_575_609.0 + 0.875, "101199904575609.88"),
            (9.5, "9.5"),
            (999999999999999.9, "999999999999999.9"),
            (9999999999999998.0, "9999999999999998.0"),
        ];
        for (x, want) in cases {
            assert_eq!(float_text(x), want, "{x:?}");
        }
    }

    #[test]
    fn reads_nan_and_infinity_and_writes_them_back() {
        let text = "[NaN, Infinity, -Infinity, 1e400, -1e400]";
        assert_eq!(roundtrip(text, None), "[NaN, Infinity, -Infinity, Infinity, -Infinity]");
        // An ordinary number never becomes NaN.
        assert_eq!(roundtrip("[1.0, 2, -0, 0.5e1]", None), "[1.0, 2, 0, 5.0]");
    }

    #[test]
    fn big_ints_stay_exact() {
        let big = "123456789012345678901234567890";
        assert_eq!(roundtrip(&format!("[{big}, -{big}]"), None), format!("[{big}, -{big}]"));
        assert!(loads(&"9".repeat(4301)).is_err());
        assert!(loads(&"9".repeat(4300)).is_ok());
    }

    #[test]
    fn indent_layout_like_python() {
        let text = r#"{"a": [1, {"b": null}], "c": {}, "d": [], "e": "x"}"#;
        assert_eq!(
            roundtrip(text, Some(1)),
            "{\n \"a\": [\n  1,\n  {\n   \"b\": null\n  }\n ],\n \"c\": {},\n \"d\": [],\n \"e\": \"x\"\n}"
        );
        assert_eq!(
            roundtrip(text, Some(0)),
            "{\n\"a\": [\n1,\n{\n\"b\": null\n}\n],\n\"c\": {},\n\"d\": [],\n\"e\": \"x\"\n}"
        );
        assert_eq!(dumps(&loads(r#"{"b":1,"a":2}"#).unwrap(), Some(2), true), "{\n  \"a\": 2,\n  \"b\": 1\n}");
    }

    #[test]
    fn strings_escaped_like_ensure_ascii_false() {
        let text = r#"["ä\u00e9\n\t\"\\\/\u0001\u007f\u2028😀\ud83d\ude00", "\u00"]"#;
        assert!(loads(text).is_err(), "kaputtes \\u");
        let text = r#"["ä\u00e9\n\t\"\\\/\u0001\u007f\u2028😀\ud83d\ude00"]"#;
        assert_eq!(roundtrip(text, None), "[\"äé\\n\\t\\\"\\\\/\\u0001\u{7f}\u{2028}😀😀\"]");
    }

    #[test]
    fn lone_surrogate_becomes_replacement() {
        assert_eq!(loads(r#""\ud800x""#).unwrap(), Value::str("\u{fffd}x"));
        assert_eq!(loads(r#""\ud800\u0041""#).unwrap(), Value::str("\u{fffd}A"));
    }

    #[test]
    fn duplicate_keys_keep_first_place_last_value() {
        assert_eq!(roundtrip(r#"{"a": 1, "b": 2, "a": 3}"#, None), r#"{"a": 3, "b": 2}"#);
    }

    #[test]
    fn rejects_what_python_rejects() {
        for bad in [
            "",
            "{",
            "[1,]",
            "{\"a\":1,}",
            "01",
            "1.",
            ".5",
            "+1",
            "nul",
            "\"a\nb\"",
            "\u{feff}{}",
            "[1] x",
            "{'a': 1}",
            "\"\\x\"",
            "-",
            "Infinityx",
        ] {
            assert!(loads(bad).is_err(), "{bad:?} müsste scheitern");
        }
        assert_eq!(loads(" \t\r\n[1e5, 1E+5, 2e-1]\n ").unwrap(), loads("[100000.0, 100000.0, 0.2]").unwrap());
        assert!(loads(&"[".repeat(2000)).unwrap_err().too_deep);
    }

    #[test]
    fn backup_names() {
        assert_eq!(backup_name("x/bestand.json"), "x/bestand.bak.json");
        assert_eq!(backup_name("a.json.d/log"), "a.json.d/log.bak");
    }

    fn scratch(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("versekit-jsonio-{name}-{}", crate::testdir::run_tag()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.to_string_lossy().into_owned()
    }

    #[test]
    fn save_writes_platform_newlines_and_backup() {
        let dir = scratch("save");
        let target = Flavor::native().join(&dir, "a.json");
        let value = loads(r#"{"x": [1, 2.5]}"#).unwrap();
        save_json(&target, &value, 1, false).unwrap();
        let nl = if cfg!(windows) { "\r\n" } else { "\n" };
        let want = ["{", " \"x\": [", "  1,", "  2.5", " ]", "}"].join(nl);
        assert_eq!(fs::read_to_string(&target).unwrap(), want);
        assert!(!Path::new(&backup_name(&target)).exists());
        save_json(&target, &Value::Array(vec![]), 1, false).unwrap();
        assert_eq!(fs::read_to_string(backup_name(&target)).unwrap(), want);
        assert_eq!(fs::read_to_string(&target).unwrap(), "[]");
        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn long_paths_save_and_move() {
        let dir = scratch("lang");
        let deep = Flavor::native().join_all(&dir, &[&"a".repeat(120), &"b".repeat(120)]);
        let target = Flavor::native().join(&deep, "c.json");
        assert!(target.len() > 260);
        save_json(&target, &Value::int(1), 1, false).unwrap();
        save_json(&target, &Value::int(2), 1, false).unwrap();
        assert_eq!(read_json(&target).unwrap(), Value::int(2));
        assert_eq!(read_json(&backup_name(&target)).unwrap(), Value::int(1));
        fs::write(&target, "kaputt").unwrap();
        assert_eq!(load_json(&target, None, "test"), Some(Value::int(1)));
        assert!(!Path::new(&target).exists(), "beiseitegelegt");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn broken_file_moves_aside_and_backup_is_used() {
        let dir = scratch("broken");
        let target = Flavor::native().join(&dir, "bestand.json");
        fs::write(&target, "{kaputt").unwrap();
        fs::write(backup_name(&target), "{\"gut\": 1}").unwrap();
        let got = load_json(&target, None, "test");
        assert_eq!(got, Some(loads("{\"gut\": 1}").unwrap()));
        assert!(!Path::new(&target).exists());
        let names: Vec<String> =
            fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert!(names.iter().any(|n| n.starts_with("bestand.kaputt-") && n.ends_with(".json")), "{names:?}");
        // If the main file is missing, the backup applies.
        assert_eq!(load_json(&target, None, "test"), Some(loads("{\"gut\": 1}").unwrap()));
        // A validator that says no makes even a valid file count as broken.
        let other = Flavor::native().join(&dir, "liste.json");
        fs::write(&other, "[1]").unwrap();
        assert_eq!(load_json(&other, Some(&|v: &Value| v.as_object().is_some()), "test"), None);
        assert!(!Path::new(&other).exists());
        // No file at all: nothing.
        assert_eq!(load_json(&Flavor::native().join(&dir, "fehlt.json"), None, "test"), None);
        // With a BOM it is not JSON for `json.load`.
        let bom = Flavor::native().join(&dir, "bom.json");
        fs::write(&bom, "\u{feff}{}").unwrap();
        assert_eq!(load_json(&bom, None, "test"), None);
        assert!(!Path::new(&bom).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn too_deep_is_left_alone() {
        let dir = scratch("deep");
        let target = Flavor::native().join(&dir, "tief.json");
        fs::write(&target, "[".repeat(5000)).unwrap();
        assert_eq!(load_json(&target, None, "test"), None);
        assert!(Path::new(&target).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn guard_blocks_every_write() {
        let dir = scratch("guard");
        let target = Flavor::native().join(&dir, "a.json");
        fs::write(&target, "kaputt").unwrap();
        writeguard::deny(true);
        let saved = save_json(&target, &Value::Null, 1, false);
        let loaded = load_json(&target, None, "test");
        let denied = writeguard::take_denied();
        writeguard::deny(false);
        assert!(saved.is_err());
        assert_eq!(loaded, None);
        assert!(!denied.is_empty());
        assert_eq!(fs::read_to_string(&target).unwrap(), "kaputt", "nichts verschoben, nichts geschrieben");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn two_handles_of_one_lock_file_lock_each_other() {
        let dir = scratch("sperre-zwei");
        let file = Flavor::native().join(&dir, "sperre.lock");
        let first = ProcessLock::open(&file).unwrap();
        let second = ProcessLock::open(&file).unwrap();
        assert!(matches!(first.acquire(Duration::ZERO), Taken::Held));
        assert!(matches!(second.acquire(Duration::ZERO), Taken::TimedOut), "belegt, solange das erste hält");
        first.release();
        assert!(matches!(second.acquire(Duration::ZERO), Taken::Held), "frei nach dem Loslassen");
        second.release();
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn waiting_for_a_held_lock_ends_at_the_limit() {
        let dir = scratch("sperre-grenze");
        let file = Flavor::native().join(&dir, "sperre.lock");
        let holder = ProcessLock::open(&file).unwrap();
        let waiter = ProcessLock::open(&file).unwrap();
        assert!(matches!(holder.acquire(Duration::ZERO), Taken::Held));
        let start = std::time::Instant::now();
        assert!(matches!(waiter.acquire(Duration::from_millis(300)), Taken::TimedOut));
        let waited = start.elapsed();
        assert!(waited >= Duration::from_millis(300) && waited < Duration::from_secs(5), "{waited:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_lock_file_that_cannot_be_opened_is_no_lock() {
        let dir = scratch("sperre-unbrauchbar");
        // The "folder" of the lock file is a file: it can neither be made nor opened.
        let blocker = Flavor::native().join(&dir, "datei");
        fs::write(&blocker, "x").unwrap();
        let file = Flavor::native().join(&blocker, "sperre.lock");
        assert!(ProcessLock::open(&file).is_err());
        assert!(!share_saves_between_processes(&file), "kein Absturz, die Sperre bleibt im Prozess");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_write_guard_makes_no_lock_file() {
        let dir = scratch("sperre-schutz");
        let file = Flavor::native().join(&dir, "sperre.lock");
        writeguard::deny(true);
        let opened = ProcessLock::open(&file);
        writeguard::take_denied();
        writeguard::deny(false);
        assert!(opened.is_err());
        assert!(!Path::new(&file).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    /// Two threads writing the same file at the same time (two fetches of the slots) each have their own
    /// temporary file: neither renames the other's away, every write lands, and no temporary file is left.
    #[test]
    fn two_threads_writing_one_file_keep_their_own_temporary_file() {
        let dir = scratch("zwei-schreiber");
        let target = Flavor::native().join(&dir, "ablage.json");
        let names: Vec<String> = (0..2)
            .map(|_| {
                let target = target.clone();
                thread::spawn(move || temp_name(&target)).join().unwrap()
            })
            .collect();
        assert_ne!(names[0], names[1], "one temporary name per thread");
        assert_eq!(temp_name(&target), temp_name(&target), "the same thread keeps its name");
        let start = std::sync::Arc::new(std::sync::Barrier::new(2));
        let writers: Vec<_> = ["a", "b"]
            .into_iter()
            .map(|mark| {
                let (target, start) = (target.clone(), std::sync::Arc::clone(&start));
                thread::spawn(move || {
                    let text = mark.repeat(4096);
                    start.wait();
                    (0..200).filter(|_| write_text_atomic(&target, &text, Newline::Keep).is_err()).count()
                })
            })
            .collect();
        let failed: Vec<usize> = writers.into_iter().map(|w| w.join().unwrap()).collect();
        assert_eq!(failed, [0, 0], "every write landed");
        let text = fs::read_to_string(&target).unwrap();
        assert!(text == "a".repeat(4096) || text == "b".repeat(4096), "one whole text, not a mix");
        let left: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(left.is_empty(), "{left:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn nested_calls_in_one_thread_and_two_threads_do_not_deadlock() {
        let outer = lock_saves();
        let inner = lock_saves();
        drop(inner);
        let (tx, rx) = std::sync::mpsc::channel();
        let other = thread::spawn(move || {
            let _lock = lock_saves();
            tx.send(()).unwrap();
        });
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err(), "der andere Faden wartet, solange der äußere hält");
        drop(outer);
        assert!(rx.recv_timeout(Duration::from_secs(5)).is_ok(), "nach dem äußeren Loslassen kommt der andere dran");
        other.join().unwrap();
    }
}
