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

//! Text tools that behave exactly like their Python counterparts.

/// Like Python's `bytes.decode('utf-8', 'ignore')`: invalid bytes are
/// dropped instead of becoming a replacement character (as with
/// `from_utf8_lossy`).
pub fn decode_ignore(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for chunk in bytes.utf8_chunks() {
        out.push_str(chunk.valid());
    }
    out
}

/// Like Python's `bytes.decode('utf-8', 'replace')`: every invalid piece
/// becomes one replacement character. Borrowed when nothing is invalid.
pub fn decode_replace(bytes: &[u8]) -> std::borrow::Cow<'_, str> {
    String::from_utf8_lossy(bytes)
}

/// Like Python's `str.title()` for the short codes from the log (`MIL` → `Mil`).
pub fn title_ascii(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars.flat_map(char::to_lowercase)).collect(),
        None => String::new(),
    }
}

/// Like Python's `str.isspace` for one character: Rust's whitespace plus
/// the separator characters `\x1c` to `\x1f`.
pub fn py_isspace(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// Like Python's `str.strip()` without an argument.
pub fn py_strip(text: &str) -> &str {
    text.trim_matches(py_isspace)
}

/// Like Python's `sum()` over floats since 3.12 (4.12 runs on 3.14): the
/// sum is compensated (Neumaier), so the last bit is the one Python gives.
/// A plain sum differs there, and a sort by such sums breaks ties the other way.
pub fn py_sum(values: impl IntoIterator<Item = f64>) -> f64 {
    let (mut result, mut compensation) = (0.0_f64, 0.0_f64);
    for x in values {
        let t = result + x;
        if result.abs() >= x.abs() {
            compensation += (result - t) + x;
        } else {
            compensation += (x - t) + result;
        }
        result = t;
    }
    if compensation != 0.0 && compensation.is_finite() {
        result += compensation;
    }
    result
}

/// Like Python's `str.rstrip()` without an argument.
pub fn py_rstrip(text: &str) -> &str {
    text.trim_end_matches(py_isspace)
}

/// Whether Python's `str.isprintable` holds for one character: no control
/// character, no separator other than the plain space, no unassigned or
/// private-use code point.
fn py_printable(c: char) -> bool {
    if c == ' ' {
        return true;
    }
    if c.is_control() || py_isspace(c) {
        return false;
    }
    // Format and private-use characters, line and paragraph separators and
    // the zero-width ones Python escapes.
    !matches!(c as u32,
        0xAD | 0x600..=0x605 | 0x61C | 0x6DD | 0x70F | 0x8E2 | 0x180E | 0x200B..=0x200F | 0x2028..=0x202E
        | 0x2060..=0x2064 | 0x2066..=0x206F | 0xFEFF | 0xFFF9..=0xFFFB | 0xE000..=0xF8FF | 0xF0000..=0x10FFFF)
}

/// Python's `repr()` of a string: single quotes unless the text holds a
/// single quote and no double quote; backslash, quote, `\n`, `\r`, `\t` and
/// everything not printable escaped.
pub fn py_repr_str(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') { '"' } else { '\'' };
    let mut out = String::with_capacity(text.len() + 2);
    out.push(quote);
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if py_printable(c) => out.push(c),
            c if (c as u32) < 0x100 => out.push_str(&format!("\\x{:02x}", c as u32)),
            c if (c as u32) < 0x10000 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push_str(&format!("\\U{:08x}", c as u32)),
        }
    }
    out.push(quote);
    out
}

/// Character class for regular expressions like Python's `\s` in a text
/// pattern.
///
/// ⚠ Rust's `\s` does not know the separator characters `\x1c` to `\x1f`;
/// Python does.
pub const PY_SPACE: &str = r"[\s\x1C-\x1F]";
/// Like Python's `\S` in a text pattern.
pub const PY_NONSPACE: &str = r"[^\s\x1C-\x1F]";

/// Lines as when reading a file in Python text mode (`newline=None`):
/// `\r\n`, `\r` and `\n` end a line; the line ending is not part of it.
pub fn lines_universal(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                out.push(&text[start..i]);
                start = i + 1;
            }
            b'\r' => {
                out.push(&text[start..i]);
                if bytes.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    if start < bytes.len() {
        out.push(&text[start..]);
    }
    out
}

/// Like Python's `str.splitlines()`: besides `\n`, `\r` and `\r\n` also
/// `\x0b`, `\x0c`, `\x1c` to `\x1e`, `\x85`, ` ` and ` ` end a line.
pub fn py_splitlines(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if matches!(c, '\n' | '\r' | '\x0b' | '\x0c' | '\x1c'..='\x1e' | '\u{85}' | '\u{2028}' | '\u{2029}') {
            out.push(&text[start..i]);
            start = i + c.len_utf8();
            if c == '\r' && chars.peek().is_some_and(|(_, next)| *next == '\n') {
                start += 1;
                chars.next();
            }
        }
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

/// The SHA-1 of `data` as 40 hex digits (Python's `hashlib.sha1(data).hexdigest()`). Only for identifiers that
/// 4.12 forms the same way (the fingerprint of the data errors, the identifier of a hangar entry), never for
/// anything that must be secure.
pub fn sha1_hex(data: &[u8]) -> String {
    let mut state: [u32; 5] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476, 0xC3D2_E1F0];
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend(((data.len() as u64) * 8).to_be_bytes());
    for block in message.as_chunks::<64>().0 {
        let mut words = [0u32; 80];
        for (word, bytes) in words.iter_mut().zip(block.as_chunks::<4>().0) {
            *word = u32::from_be_bytes(*bytes);
        }
        for i in 16..80 {
            words[i] = (words[i - 3] ^ words[i - 8] ^ words[i - 14] ^ words[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = state;
        for (i, word) in words.iter().enumerate() {
            let (mix, constant) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6u32),
            };
            let next = a.rotate_left(5).wrapping_add(mix).wrapping_add(e).wrapping_add(constant).wrapping_add(*word);
            (e, d, c, b, a) = (d, c, b.rotate_left(30), a, next);
        }
        for (slot, add) in state.iter_mut().zip([a, b, c, d, e]) {
            *slot = slot.wrapping_add(add);
        }
    }
    state.iter().map(|word| format!("{word:08x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitlines_like_python() {
        assert_eq!(py_splitlines("a\r\nb\nc\rd"), ["a", "b", "c", "d"]);
        assert_eq!(py_splitlines("a\n"), ["a"]);
        assert_eq!(py_splitlines("a\n\nb"), ["a", "", "b"]);
        assert_eq!(py_splitlines("a\u{2028}b\x0bc"), ["a", "b", "c"]);
        assert!(py_splitlines("").is_empty());
    }

    #[test]
    fn repr_like_python() {
        assert_eq!(py_repr_str("abc"), "'abc'");
        assert_eq!(py_repr_str("it's"), "\"it's\"");
        assert_eq!(py_repr_str("a\"b'c"), "'a\"b\\'c'");
        assert_eq!(py_repr_str("a\\b\n\t"), "'a\\\\b\\n\\t'");
        assert_eq!(py_repr_str("\u{1}\u{a0}ä"), "'\\x01\\xa0ä'");
        assert_eq!(py_repr_str("\u{200b}"), "'\\u200b'");
    }

    #[test]
    fn decode_drops_invalid_bytes() {
        assert_eq!(decode_ignore(b"ab\xffcd"), "abcd");
        assert_eq!(decode_ignore("Größe".as_bytes()), "Größe");
        // truncated multi-byte character at the end of the block is dropped
        assert_eq!(decode_ignore(&"ö".as_bytes()[..1]), "");
    }

    #[test]
    fn strip_like_python() {
        assert_eq!(py_strip("\u{1f} ja\u{a0}\n"), "ja");
        assert_eq!(py_strip("\u{200b}x"), "\u{200b}x");
    }

    #[test]
    fn the_sum_is_compensated_like_python_3_14() {
        // Python 3.14: sum([0.1] * 10) == 1.0, a plain loop gives 0.9999999999999999.
        let tenths = [0.1_f64; 10];
        assert_eq!(py_sum(tenths), 1.0);
        assert_ne!(tenths.iter().fold(0.0, |a, b| a + b), 1.0, "the plain sum is the other result");
        assert_eq!(py_sum([]), 0.0);
        assert!(py_sum([]).is_sign_positive());
        assert_eq!(py_sum([1e100, 1.0, -1e100]), 1.0);
    }

    #[test]
    fn space_class_like_isspace() {
        let space = regex::Regex::new(&format!("^{PY_SPACE}$")).unwrap();
        let other = regex::Regex::new(&format!("^{PY_NONSPACE}$")).unwrap();
        for c in (0..=0x10FFFF).filter_map(char::from_u32) {
            let s = c.to_string();
            assert_eq!(space.is_match(&s), py_isspace(c), "{:#x}", c as u32);
            assert_eq!(other.is_match(&s), !py_isspace(c), "{:#x}", c as u32);
        }
    }

    #[test]
    fn title_like_python() {
        assert_eq!(title_ascii("MIL"), "Mil");
        assert_eq!(title_ascii("tool"), "Tool");
        assert_eq!(title_ascii(""), "");
    }
}
