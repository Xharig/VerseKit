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

//! Numbers the way Python writes and reads them (`float()`, `repr()`, `'%g'`, `'%.2f'`, `round()`), for the files of
//! the game and the page of the axes. The values in `actionmaps.xml` are written by the game in full width
//! (`0.098999992`) and must come back exactly as they were; what the page shows is rounded the way 4.12 rounds.
//!
//! ⚠ Rounding is done on the **exact** decimal value of the double, half to even, like C's `printf` and Python. Not
//! on what Rust's formatter happens to do at a tie.

use crate::text::py_strip;

/// Python's `float(text)`: blanks around, a sign, digits with single underscores between them, an exponent,
/// `inf`, `infinity` and `nan` in any case. `None` where Python raises.
pub fn parse(text: &str) -> Option<f64> {
    let text = py_strip(text);
    if text.is_empty() {
        return None;
    }
    let bytes = text.as_bytes();
    let mut cleaned = String::with_capacity(text.len());
    for (at, &byte) in bytes.iter().enumerate() {
        if byte == b'_' {
            // Only between two digits.
            let before = at > 0 && bytes[at - 1].is_ascii_digit();
            let after = bytes.get(at + 1).is_some_and(u8::is_ascii_digit);
            if !(before && after) {
                return None;
            }
            continue;
        }
        cleaned.push(char::from(byte));
    }
    // Rust takes what Python takes (`1e5`, `.5`, `5.`, `+1`, `inf`, `nan`) and refuses the rest; text that is not
    // ASCII is not a number here (Python would take other digits too, which no file of the game has).
    if !text.is_ascii() {
        return None;
    }
    cleaned.parse::<f64>().ok()
}

/// The exact decimal value of `|x|` as digits and the place of the point: `x = 0.d1 d2 … × 10^point`, without
/// leading zeros. Empty digits mean zero.
fn exact(x: f64) -> (Vec<u8>, i32) {
    let shown = format!("{:.90}", x.abs());
    let (whole, fraction) = shown.split_once('.').unwrap_or((&shown, ""));
    let mut digits: Vec<u8> = whole.bytes().chain(fraction.bytes()).map(|b| b - b'0').collect();
    let mut point = i32::try_from(whole.len()).unwrap_or(i32::MAX);
    let lead = digits.iter().take_while(|d| **d == 0).count();
    digits.drain(..lead);
    point -= i32::try_from(lead).unwrap_or(0);
    while digits.last() == Some(&0) {
        digits.pop();
    }
    if digits.is_empty() {
        point = 0;
    }
    (digits, point)
}

/// The digits cut to `keep` places and rounded half to even; gives the new digits and whether the point moved by
/// one place (a carry out of the front: 9.99 → 10.0).
fn round_digits(digits: &[u8], keep: i32) -> (Vec<u8>, bool) {
    if keep < 0 {
        return (Vec::new(), false);
    }
    let keep = usize::try_from(keep).unwrap_or(0);
    let mut kept: Vec<u8> = digits.iter().take(keep).copied().collect();
    while kept.len() < keep {
        kept.push(0);
    }
    let rest = digits.get(keep..).unwrap_or(&[]);
    let up = match rest.first() {
        None => false,
        Some(&d) if d > 5 => true,
        Some(&d) if d < 5 => false,
        Some(_) => {
            if rest[1..].iter().any(|d| *d != 0) {
                true
            } else {
                // A tie: to the even last digit.
                kept.last().is_some_and(|d| d % 2 == 1)
            }
        }
    };
    if !up {
        return (kept, false);
    }
    let mut at = kept.len();
    loop {
        if at == 0 {
            kept.insert(0, 1);
            return (kept, true);
        }
        at -= 1;
        if kept[at] == 9 {
            kept[at] = 0;
        } else {
            kept[at] += 1;
            return (kept, false);
        }
    }
}

fn sign_of(x: f64) -> &'static str {
    if x.is_sign_negative() { "-" } else { "" }
}

/// `'%.{places}f' % x`.
pub fn fixed(x: f64, places: usize) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return format!("{}inf", sign_of(x));
    }
    let (digits, point) = exact(x);
    let places_i = i32::try_from(places).unwrap_or(0);
    let (digits, moved) = round_digits(&digits, point + places_i);
    let point = if moved { point + 1 } else { point };
    // `digits` now holds the places before the point and `places` after it.
    let mut whole = String::new();
    let mut fraction = String::new();
    for (at, d) in digits.iter().enumerate() {
        let at = i32::try_from(at).unwrap_or(0);
        if at < point { whole.push(char::from(b'0' + d)) } else { fraction.push(char::from(b'0' + d)) }
    }
    if point < 0 {
        // Everything is after the point and starts with zeros.
        let zeros = usize::try_from(-point).unwrap_or(0).min(places);
        fraction = format!("{}{}", "0".repeat(zeros), fraction);
    }
    if whole.is_empty() {
        whole.push('0');
    }
    while fraction.len() < places {
        fraction.push('0');
    }
    fraction.truncate(places);
    // A negative number that rounds to zero keeps its sign in Python (`'%.2f' % -0.001` is `-0.00`).
    let body = if places == 0 { whole } else { format!("{whole}.{fraction}") };
    format!("{}{body}", sign_of(x))
}

/// `round(x, places)` of Python: the double nearest to the correctly rounded decimal.
pub fn round(x: f64, places: usize) -> f64 {
    if !x.is_finite() {
        return x;
    }
    fixed(x, places).parse::<f64>().unwrap_or(x)
}

/// `repr(x)` of Python: the shortest text that reads back exactly, in the form Python gives it (`0.1`, `1.0`,
/// `1e-05`, `1e+16`).
pub fn repr(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return format!("{}inf", sign_of(x));
    }
    if x == 0.0 {
        return format!("{}0.0", sign_of(x));
    }
    let mut shown = format!("{:e}", x.abs());
    // Python takes the shortest text that reads back exactly **and** is the closest to the real value; Rust's
    // shortest text is not always the closest one (the last of 17 digits can differ by one). The same number of
    // digits, correctly rounded, is the closest; it is taken where it reads back to the same number.
    let digits_len = shown.split_once('e').map_or(0, |(m, _)| m.chars().filter(char::is_ascii_digit).count());
    if digits_len > 0 {
        let closest = format!("{:.*e}", digits_len - 1, x.abs());
        if closest.parse::<f64>().ok() == Some(x.abs()) {
            shown = closest;
        }
    }
    let (mantissa, exponent) = shown.split_once('e').unwrap_or((&shown, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let sign = sign_of(x);
    if (-4..16).contains(&exponent) {
        if exponent >= 0 {
            let whole_len = usize::try_from(exponent).unwrap_or(0) + 1;
            let mut whole = digits.clone();
            while whole.len() < whole_len {
                whole.push('0');
            }
            let (whole, fraction) = whole.split_at(whole_len);
            let fraction = if fraction.is_empty() { "0" } else { fraction };
            format!("{sign}{whole}.{fraction}")
        } else {
            format!("{sign}0.{}{digits}", "0".repeat(usize::try_from(-exponent - 1).unwrap_or(0)))
        }
    } else {
        let (first, rest) = digits.split_at(1);
        let rest = if rest.is_empty() { String::new() } else { format!(".{rest}") };
        let exp_sign = if exponent < 0 { '-' } else { '+' };
        format!("{sign}{first}{rest}e{exp_sign}{:02}", exponent.abs())
    }
}

/// `'%g' % x`: six significant digits, no trailing zeros, scientific outside 1e-4 to 1e6.
pub fn general(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return format!("{}inf", sign_of(x));
    }
    let sign = sign_of(x);
    let (digits, point) = exact(x);
    if digits.is_empty() {
        return format!("{sign}0");
    }
    let (mut digits, moved) = round_digits(&digits, 6);
    let mut point = point;
    if moved {
        point += 1;
        digits.truncate(6);
    }
    let exponent = point - 1;
    while digits.last() == Some(&0) {
        digits.pop();
    }
    let text: String = digits.iter().map(|d| char::from(b'0' + d)).collect();
    if !(-4..6).contains(&exponent) {
        let (first, rest) = text.split_at(1);
        let rest = if rest.is_empty() { String::new() } else { format!(".{rest}") };
        let exp_sign = if exponent < 0 { '-' } else { '+' };
        return format!("{sign}{first}{rest}e{exp_sign}{:02}", exponent.abs());
    }
    if exponent >= 0 {
        let whole_len = usize::try_from(exponent).unwrap_or(0) + 1;
        let mut whole = text;
        while whole.len() < whole_len {
            whole.push('0');
        }
        let (whole, fraction) = whole.split_at(whole_len);
        if fraction.is_empty() { format!("{sign}{whole}") } else { format!("{sign}{whole}.{fraction}") }
    } else {
        format!("{sign}0.{}{text}", "0".repeat(usize::try_from(-exponent - 1).unwrap_or(0)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_reads_numbers_like_this() {
        for (text, expected) in [
            ("0.098999992", Some(0.098999992)),
            ("  0.5\t", Some(0.5)),
            ("1_0", Some(10.0)),
            ("1__0", None),
            ("_1", None),
            ("1_", None),
            ("1e3", Some(1000.0)),
            (".5", Some(0.5)),
            ("5.", Some(5.0)),
            ("+1", Some(1.0)),
            ("-2.5", Some(-2.5)),
            ("", None),
            ("abc", None),
            ("1,5", None),
            ("0x10", None),
            ("1e", None),
        ] {
            assert_eq!(parse(text), expected, "{text:?}");
        }
        assert_eq!(parse("inf"), Some(f64::INFINITY));
        assert_eq!(parse("-Infinity"), Some(f64::NEG_INFINITY));
        assert!(parse("NaN").is_some_and(f64::is_nan));
    }

    #[test]
    fn repr_is_what_python_writes() {
        for (x, expected) in [
            (0.1, "0.1"),
            (1.0, "1.0"),
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (0.098999992, "0.098999992"),
            (0.15000001, "0.15000001"),
            (0.85000002, "0.85000002"),
            (1e-5, "1e-05"),
            (1.5e-7, "1.5e-07"),
            (0.0001, "0.0001"),
            (0.00001234, "1.234e-05"),
            (123456789.0, "123456789.0"),
            (1e15, "1000000000000000.0"),
            (1e16, "1e+16"),
            (1.2345678901234568e17, "1.2345678901234568e+17"),
            (0.5, "0.5"),
            (100.0, "100.0"),
            (-3.25, "-3.25"),
            (0.30000000000000004, "0.30000000000000004"),
            // Seventeen digits: the closest of them, not just any that reads back.
            (10.155244668053857, "10.155244668053857"),
            (1.7793626752830987, "1.7793626752830987"),
            (0.09929432324231557, "0.09929432324231557"),
        ] {
            assert_eq!(repr(x), expected, "{x:e}");
        }
    }

    #[test]
    fn percent_g_is_six_significant_digits() {
        for (x, expected) in [
            (1.5, "1.5"),
            (2.0, "2"),
            (0.0, "0"),
            (0.05, "0.05"),
            (1.0 / 3.0, "0.333333"),
            (2.0 / 3.0, "0.666667"),
            (100000.0, "100000"),
            (1000000.0, "1e+06"),
            (1234567.0, "1.23457e+06"),
            (0.0001, "0.0001"),
            (0.00001, "1e-05"),
            (0.00012345678, "0.000123457"),
            (9.999995, "10"),
            (9.9999951, "10"),
            (1048576.5, "1.04858e+06"),
            (-1.25, "-1.25"),
            (3.0, "3"),
            (0.098999992, "0.099"),
            (10.0, "10"),
        ] {
            assert_eq!(general(x), expected, "{x:e}");
        }
    }

    #[test]
    fn fixed_rounds_the_exact_value_half_to_even() {
        for (x, places, expected) in [
            (0.125, 2, "0.12"),
            (0.375, 2, "0.38"),
            (0.625, 2, "0.62"),
            (0.875, 2, "0.88"),
            (0.5, 0, "0"),
            (1.5, 0, "2"),
            (2.5, 0, "2"),
            (0.099, 2, "0.10"),
            (0.098999992, 2, "0.10"),
            (0.0, 2, "0.00"),
            (1.0, 2, "1.00"),
            (0.005, 2, "0.01"),
            (0.015, 2, "0.01"),
            (0.994999, 2, "0.99"),
            (0.995, 2, "0.99"),
            (9.995, 2, "9.99"),
            (99.999, 2, "100.00"),
            (0.001, 2, "0.00"),
            (-0.001, 2, "-0.00"),
            (-0.125, 2, "-0.12"),
            (12.5, 1, "12.5"),
            (0.85000002, 4, "0.8500"),
        ] {
            assert_eq!(fixed(x, places), expected, "{x:e} / {places}");
        }
    }

    #[test]
    fn round_is_python_round() {
        assert_eq!(round(0.098999992, 4), 0.099);
        assert_eq!(round(0.15000001, 4), 0.15);
        // The exact binary value decides, not the look of the decimal: these are the answers of Python.
        assert_eq!(round(0.12345, 4), 0.1235);
        assert_eq!(round(0.00005, 4), 0.0001);
        assert_eq!(round(0.00015, 4), 0.0001);
        assert_eq!(round(2.5, 0), 2.0);
        assert_eq!(round(0.5, 4), 0.5);
    }
}
