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

//! The panel of the scanned rock read from the words of the text recognition: mass, resistance, instability, the
//! SCU of the composition and the share of each material.
//!
//! ```text
//! SCAN RESULTS
//! COPPER (ORE)
//! MASS:          10895
//! RESISTANCE:    0%
//! INSTABILITY:   9.50
//! COMPOSITION    5.50 SCU
//! 32.15%  COPPER (ORE)
//! 67.85%  INERT MATERIALS
//! ```
//!
//! A value has to pass three checks before it counts; anything less leaves it unknown, never guessed:
//!
//! | Check | Why |
//! |---|---|
//! | Every reading of the same picture that read it ([`PASSES`] at least [`MIN_PASSES`]) gives the same value ([`merge`]) | the recognition drops and swaps single digits |
//! | Each 0 and 8 in it is checked on the picture by the holes of the digit ([`zero_or_eight`]) | the HUD writes the zero with a slash; every reading mistakes it for an 8 alike |
//! | Two pictures in a row give the same value ([`Steady`]) | the HUD flickers while the rock turns |
//!
//! The shares count only all together, when they add up to 100 % ([`SHARE_SUM_SLACK`]).

use crate::refinery_scan::{Mode, Word, rows};
use crate::signature_scan::{self as digits, Gray};

/// The words of the labels, English as the game writes them and German as the translation writes them.
const MASS: [&str; 2] = ["MASS", "MASSE"];
const RESISTANCE: [&str; 2] = ["RESISTANCE", "WIDERSTAND"];
const INSTABILITY: [&str; 3] = ["INSTABILITY", "INSTABILITÄT", "INSTABILITAET"];
/// Besides the words of today the shorter ones a translation may take (`ANTEILE`, `INHALT`, `GEHALT`); the row is
/// also found without its label ([`is_composition_row`]).
const COMPOSITION: [&str; 5] = ["COMPOSITION", "ZUSAMMENSETZUNG", "ANTEILE", "INHALT", "GEHALT"];
/// The head of the panel: its first word.
const HEAD: [&str; 2] = ["SCAN", "SCANERGEBNISSE"];

/// The readings of one picture at different enlargements and in different brightness.
pub const PASSES: [(f64, Mode); 3] = [(2.0, Mode::Plain), (3.0, Mode::Plain), (2.5, Mode::Dark)];
/// A value counts when at least so many readings of one picture read it, all alike.
pub const MIN_PASSES: usize = 2;
/// The shares may add up to 100 % give or take this much (the HUD rounds each share).
pub const SHARE_SUM_SLACK: f64 = 0.6;
/// Kilograms of rock per SCU of its composition, lowest and highest: rocks seen in the game lay between about 780
/// and 1030; the range is wide, it only keeps out numbers that are no SCU at all.
pub const KG_PER_SCU: (f64, f64) = (200.0, 5000.0);

/// One value as one reading gave it: its text with the digits as read, and where it stands in the picture.
#[derive(Clone, Debug, PartialEq)]
pub struct Read {
    /// Only digits and at most one point: `10895`, `9.50`.
    pub digits: String,
    pub frame: (f64, f64, f64, f64),
}

impl Read {
    /// The number; digits ending in `m` are thousandths (`627.13m` SCU is 0.62713 SCU).
    pub fn value(&self) -> Option<f64> {
        match self.digits.strip_suffix('m') {
            Some(milli) => milli.parse::<f64>().ok().map(|v| v / 1000.0),
            None => self.digits.parse().ok(),
        }
    }
}

/// The SCU as the HUD writes it before `SCU`: a number, or a number with `m` for thousandths (`627.13m`); the `m`
/// is kept in the digits.
fn scu_text(text: &str) -> Option<String> {
    let text = text.trim();
    match text.strip_suffix(['m', 'M']) {
        Some(milli) if milli.chars().last().is_some_and(|c| c.is_ascii_digit()) => number_text(milli).map(|d| d + "m"),
        _ => number_text(text),
    }
}

/// The row of the composition: its label, also cut short where the number covers its end (`ZUSAMMENSETZU`), or else
/// the row that ends in `SCU`.
fn is_composition_row(row: &[Word]) -> bool {
    let cut = |w: &Word| {
        let text: String = w.t.chars().filter(|c| c.is_alphabetic()).collect::<String>().to_uppercase();
        text.len() >= 8 && COMPOSITION.iter().any(|label| label.starts_with(&text))
    };
    // The cargo of the ship (`CARGO 1.72 / 96.00 SCU`) also ends in SCU: a row with a slash is not the composition.
    row.iter().any(|w| is_label(w, &COMPOSITION) || cut(w))
        || (row.last().is_some_and(|w| w.t.trim().to_uppercase().ends_with("SCU")) && !row.iter().any(|w| w.t.contains('/')))
}

/// What one reading of the panel gave.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Panel {
    /// The head of the panel was seen.
    pub found: bool,
    pub mass: Option<Read>,
    pub resistance: Option<Read>,
    pub instability: Option<Read>,
    pub scu: Option<Read>,
    /// The materials by name as the HUD writes them, with their share in percent.
    pub shares: Vec<(String, Read)>,
    /// The quality of each row of [`Panel::shares`], in the same order; `None` where a row shows none.
    pub qualities: Vec<Option<Read>>,
    /// The game's own word on cracking with the laser in use, in the bar below the instability (`UNMÖGLICH`), in
    /// capitals; kept to compare the answer with, never shown as the answer.
    pub verdict: Option<String>,
}

/// Characters the recognition gives for digits of the HUD.
fn as_digit(c: char) -> Option<char> {
    match c {
        '0'..='9' => Some(c),
        'O' | 'o' | 'D' | 'Q' | 'Ø' | 'ø' => Some('0'),
        'I' | 'l' | '|' | 'i' | '!' => Some('1'),
        'S' | 's' => Some('5'),
        'B' => Some('8'),
        'Z' | 'z' => Some('2'),
        _ => None,
    }
}

/// The number in the text of the words after a label: digits (and lookalikes beside a real digit), one decimal
/// point, thousands separators dropped. `None` where anything else stands in it or no real digit.
pub fn number_text(text: &str) -> Option<String> {
    let text: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let text = text.trim_end_matches(['%', '.', ',', ':']).trim_start_matches([':', '.']);
    if !text.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    for (i, c) in chars.iter().enumerate() {
        if let Some(digit) = as_digit(*c) {
            out.push(digit);
        } else if *c == '.' || *c == ',' {
            // Three digits after it and no more: a thousands separator.
            let after = chars[i + 1..].iter().take_while(|c| as_digit(**c).is_some()).count();
            if after == 3 && *c == ',' {
                continue;
            }
            if out.contains('.') || out.is_empty() {
                return None;
            }
            out.push('.');
        } else {
            return None;
        }
    }
    (!out.is_empty() && !out.ends_with('.')).then_some(out)
}

fn is_label(word: &Word, labels: &[&str]) -> bool {
    let text: String = word.t.chars().filter(|c| c.is_alphabetic()).collect::<String>().to_uppercase();
    labels.contains(&text.as_str())
}

/// The words right of the label in its row, as one text, and their frame.
fn after_label(row: &[Word], labels: &[&str]) -> Option<(String, (f64, f64, f64, f64))> {
    let at = row.iter().position(|w| is_label(w, labels))?;
    let rest = &row[at + 1..];
    let rest: Vec<&Word> = rest.iter().filter(|w| !w.t.trim().trim_matches(':').is_empty()).collect();
    let first = rest.first()?;
    let last = rest.last()?;
    let text: String = rest.iter().map(|w| w.t.as_str()).collect::<Vec<_>>().join(" ");
    let frame = (first.x, first.y.min(last.y), last.x + last.w - first.x, first.h.max(last.h));
    Some((text, frame))
}

fn field(rows: &[Vec<Word>], labels: &[&str], integer: bool) -> Option<Read> {
    rows.iter().find_map(|row| {
        let (text, frame) = after_label(row, labels)?;
        let digits = number_text(&text)?;
        (!integer || !digits.contains('.')).then_some(Read { digits, frame })
    })
}

/// A row of the composition: `32.15% COPPER (ORE) 597` or `COPPER (ORE) 32.15%`, the name, the share and the
/// quality where the row ends with it (a whole number after the name; `0` for the inert rest).
fn share_row(row: &[Word]) -> Option<(String, Read, Option<Read>)> {
    let at = row.iter().position(|w| w.t.contains('%'))?;
    let word = &row[at];
    let before_percent = word.t.split('%').next().unwrap_or("");
    let (digits, frame) = if before_percent.chars().any(|c| c.is_ascii_digit()) {
        (number_text(before_percent)?, (word.x, word.y, word.w, word.h))
    } else {
        // `32.15 %`: the number is the word before.
        let before = at.checked_sub(1).map(|i| &row[i])?;
        (number_text(&before.t)?, (before.x, before.y, before.w + word.w, before.h))
    };
    let name: String = row
        .iter()
        .enumerate()
        .filter(|(i, w)| *i != at && number_text(&w.t).is_none())
        .map(|(_, w)| w.t.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let name: String = name.chars().filter(|c| c.is_alphabetic() || matches!(c, ' ' | '(' | ')')).collect();
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let quality = row
        .last()
        .filter(|_| row.len() > at + 1)
        .and_then(|last| number_text(&last.t).map(|digits| Read { digits, frame: (last.x, last.y, last.w, last.h) }))
        .filter(|read| !read.digits.contains('.') && read.value().is_some_and(|q| q <= 1000.0));
    (name.chars().filter(|c| c.is_alphabetic()).count() >= 3).then_some((name, Read { digits, frame }, quality))
}

/// The word of the bar below the instability: a row of letters only (the frame of the bar may come as brackets),
/// in capitals; `None` for a row with digits or a label.
fn verdict_of(row: &[Word]) -> Option<String> {
    let text: String = row.iter().map(|w| w.t.as_str()).collect::<Vec<_>>().join(" ");
    if text.chars().any(|c| c.is_ascii_digit()) || row.iter().any(|w| is_label(w, &COMPOSITION)) {
        return None;
    }
    let word: String = text.chars().filter(|c| c.is_alphabetic() || *c == ' ').collect::<String>().to_uppercase();
    let word = word.split_whitespace().collect::<Vec<_>>().join(" ");
    (word.chars().filter(|c| c.is_alphabetic()).count() >= 3).then_some(word)
}

/// One reading of the panel from the words of the recognition.
pub fn parse(words: &[Word]) -> Panel {
    let rows = rows(words);
    let found = rows.iter().any(|row| row.iter().any(|w| is_label(w, &HEAD)))
        || rows.iter().any(|row| row.iter().any(|w| is_label(w, &MASS)));
    let mut panel = Panel {
        found,
        mass: field(&rows, &MASS, true),
        resistance: field(&rows, &RESISTANCE, false),
        instability: field(&rows, &INSTABILITY, false),
        ..Panel::default()
    };
    // A rock not scanned yet shows the panel with nothing in it (`0.00`, `0%`): no values, never a rock of no mass.
    if field(&rows, &MASS, false).and_then(|r| r.value()) == Some(0.0) {
        return Panel { found, ..Panel::default() };
    }
    let composition_at = rows.iter().position(|row| is_composition_row(row));
    if let Some(instability_at) = rows.iter().position(|row| row.iter().any(|w| is_label(w, &INSTABILITY))) {
        let end = composition_at.unwrap_or(rows.len()).max(instability_at + 1);
        panel.verdict = rows[instability_at + 1..end].iter().find_map(|row| verdict_of(row));
    }
    if let Some(at) = composition_at {
        let row = &rows[at];
        if let Some(scu_at) = row.iter().position(|w| w.t.to_uppercase().contains("SCU")) {
            let scu_word = &row[scu_at];
            let inline = scu_word.t.to_uppercase().replace("SCU", "");
            panel.scu = if inline.chars().any(|c| c.is_ascii_digit()) {
                scu_text(&inline.to_lowercase()).map(|digits| Read { digits, frame: (scu_word.x, scu_word.y, scu_word.w, scu_word.h) })
            } else if scu_at >= 1 {
                let before = &row[scu_at - 1];
                scu_text(&before.t).map(|digits| Read { digits, frame: (before.x, before.y, before.w, before.h) })
            } else {
                None
            };
        }
        let rows: Vec<(String, Read, Option<Read>)> = rows[at + 1..].iter().map_while(|row| share_row(row)).collect();
        panel.qualities = rows.iter().map(|(_, _, quality)| quality.clone()).collect();
        panel.shares = rows.into_iter().map(|(name, share, _)| (name, share)).collect();
    }
    panel
}

/// The value all readings that read it agree on, from at least [`MIN_PASSES`] of them; with the frame of the first.
fn agreed(values: &[Option<&Read>]) -> Option<Read> {
    let read: Vec<&Read> = values.iter().flatten().copied().collect();
    let first = read.first()?;
    (read.len() >= MIN_PASSES && read.iter().all(|r| r.digits == first.digits)).then(|| (*first).clone())
}

/// What several readings of the same picture agree on. The shares count only when the list is the same in every
/// reading that read one, and adds up to 100 %.
pub fn merge(passes: &[Panel]) -> Panel {
    let pick = |get: fn(&Panel) -> Option<&Read>| agreed(&passes.iter().map(get).collect::<Vec<_>>());
    let lists: Vec<&Panel> = passes.iter().filter(|p| !p.shares.is_empty()).collect();
    let digits = |q: &Option<Read>| q.as_ref().map(|r| r.digits.clone());
    let same = |a: &Panel, b: &Panel| {
        a.shares.len() == b.shares.len()
            && a.shares.iter().zip(&b.shares).all(|(x, y)| x.0 == y.0 && x.1.digits == y.1.digits)
            && a.qualities.iter().map(digits).eq(b.qualities.iter().map(digits))
    };
    let (shares, qualities) = match lists.first() {
        Some(first) if lists.len() >= MIN_PASSES && lists.iter().all(|l| same(first, l)) && adds_up(&first.shares) => {
            (first.shares.clone(), first.qualities.clone())
        }
        _ => (Vec::new(), Vec::new()),
    };
    Panel {
        found: passes.iter().any(|p| p.found),
        mass: pick(|p| p.mass.as_ref()),
        resistance: pick(|p| p.resistance.as_ref()),
        instability: pick(|p| p.instability.as_ref()),
        scu: pick(|p| p.scu.as_ref()),
        shares,
        qualities,
        verdict: {
            let read: Vec<&String> = passes.iter().filter_map(|p| p.verdict.as_ref()).collect();
            read.first().filter(|first| read.len() >= MIN_PASSES && read.iter().all(|r| r == *first)).map(|w| (*w).clone())
        },
    }
}

fn adds_up(shares: &[(String, Read)]) -> bool {
    let sum: Option<f64> = shares.iter().map(|(_, r)| r.value()).sum();
    sum.is_some_and(|sum| (sum - 100.0).abs() <= SHARE_SUM_SLACK)
}

/// What the holes of one digit of the picture say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Holes {
    /// A zero: one hole in the middle, or two split by the slash.
    Zero,
    /// An eight: two holes one above the other.
    Eight,
    /// Neither, or not clear.
    Other,
}

/// The holes of a digit on the fixed grid.
pub fn holes_of(pattern: &digits::Pattern) -> Holes {
    if digits::slashed_zero(pattern) {
        return Holes::Zero;
    }
    match digits::holes(pattern) {
        (1, Some(top)) if (0.35..=0.65).contains(&top) => Holes::Zero,
        (2, _) => Holes::Eight,
        _ => Holes::Other,
    }
}

/// Is every 0 and 8 of `read` what the picture shows? `crop` is the grey picture of the value (bright writing);
/// its digits are the bright areas left to right. Where the areas do not match the digits one by one, or one 0 or
/// 8 has the holes of the other or none clear, the value is not sure. A value without 0 and 8 needs no check.
pub fn zero_or_eight(read: &Read, crop: &Gray) -> bool {
    let wanted: Vec<char> = read.digits.chars().filter(char::is_ascii_digit).collect();
    if !wanted.iter().any(|c| matches!(c, '0' | '8')) {
        return true;
    }
    digits::thresholds(crop).into_iter().any(|threshold| {
        let mut boxes: Vec<digits::Box4> = digits::components(crop, threshold)
            .into_iter()
            .filter(|b| {
                let h = b[3] - b[1] + 1;
                // Digits fill most of the height; the point and the per cent sign's small rings do not.
                h * 10 >= crop.h as i32 * 6
            })
            .collect();
        boxes.sort_by_key(|b| b[0]);
        let boxes: Vec<digits::Box4> = boxes.into_iter().filter(|b| !is_percent_sign(b, crop)).collect();
        boxes.len() == wanted.len()
            && boxes.iter().zip(&wanted).all(|(b, digit)| match digit {
                '0' => holes_of(&digits::normalize(crop, *b, threshold)) == Holes::Zero,
                '8' => holes_of(&digits::normalize(crop, *b, threshold)) == Holes::Eight,
                _ => true,
            })
    })
}

/// The per cent sign is wider than a digit of its height.
fn is_percent_sign(b: &digits::Box4, _crop: &Gray) -> bool {
    let (w, h) = (b[2] - b[0] + 1, b[3] - b[1] + 1);
    w * 10 > h * 9
}

/// Is a value written in the green the HUD uses for a resistance with the laser head applied? `rgb`: the mean
/// colour of its bright points. The plain values are near white (about 195, 237, 212), the green one about
/// (106, 232, 86): red and blue well below green.
pub fn is_head_green(rgb: (f64, f64, f64)) -> bool {
    let (r, g, b) = rgb;
    g > 120.0 && r < 0.65 * g && b < 0.65 * g
}

/// What one picture gave after the checks: each value only when sure.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sure {
    pub mass: Option<f64>,
    pub resistance: Option<f64>,
    pub instability: Option<f64>,
    pub scu: Option<f64>,
    pub shares: Vec<(String, f64)>,
    /// The quality of each share, in the same order.
    pub qualities: Vec<Option<f64>>,
    /// The game's own word ([`Panel::verdict`]).
    pub verdict: Option<String>,
    /// The HUD shows the resistance with the factor of the laser head in use applied (it writes it in green): the
    /// value is the effective one, not the rock's own.
    pub with_head: bool,
}

impl Sure {
    /// The values of a merged reading whose 0 and 8 the picture confirms (`check`); a share list only whole, with
    /// every quality confirmed too.
    pub fn of(panel: &Panel, check: impl Fn(&Read) -> bool) -> Sure {
        let keep = |read: &Option<Read>| read.as_ref().filter(|r| check(r)).and_then(Read::value);
        let all_sure =
            panel.shares.iter().all(|(_, r)| check(r)) && panel.qualities.iter().flatten().all(&check);
        let (shares, qualities) = if all_sure {
            (
                panel.shares.iter().filter_map(|(name, r)| r.value().map(|v| (name.clone(), v))).collect(),
                panel.qualities.iter().map(|q| q.as_ref().and_then(Read::value)).collect(),
            )
        } else {
            (Vec::new(), Vec::new())
        };
        let mass = keep(&panel.mass);
        Sure {
            mass,
            resistance: keep(&panel.resistance).filter(|v| (0.0..=100.0).contains(v)),
            instability: keep(&panel.instability),
            // A label of the HUD laid over the number (a distance, `637.13m`) must not pass for the SCU: it counts
            // only beside a mass it fits.
            scu: keep(&panel.scu).filter(|scu| {
                *scu > 0.0 && mass.is_some_and(|mass| (KG_PER_SCU.0..=KG_PER_SCU.1).contains(&(mass / scu)))
            }),
            shares,
            qualities,
            verdict: panel.verdict.clone(),
            with_head: false,
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == Sure::default()
    }
}

/// Two pictures in a row: a value stands when the last two gave it alike, each value on its own.
#[derive(Clone, Debug, Default)]
pub struct Steady {
    last: Option<Sure>,
}

impl Steady {
    /// Takes the values of the next picture (`None`: no panel seen) and gives what stands now.
    pub fn step(&mut self, next: Option<Sure>) -> Sure {
        let Some(next) = next else {
            self.last = None;
            return Sure::default();
        };
        let out = match &self.last {
            None => Sure::default(),
            Some(last) => {
                let both = |a: Option<f64>, b: Option<f64>| a.zip(b).filter(|(a, b)| a == b).map(|(a, _)| a);
                Sure {
                    mass: both(last.mass, next.mass),
                    resistance: both(last.resistance, next.resistance).filter(|_| last.with_head == next.with_head),
                    with_head: next.with_head,
                    instability: both(last.instability, next.instability),
                    scu: both(last.scu, next.scu),
                    shares: if last.shares == next.shares && last.qualities == next.qualities {
                        next.shares.clone()
                    } else {
                        Vec::new()
                    },
                    qualities: if last.shares == next.shares && last.qualities == next.qualities {
                        next.qualities.clone()
                    } else {
                        Vec::new()
                    },
                    verdict: next.verdict.clone().filter(|word| last.verdict.as_ref() == Some(word)),
                }
            }
        };
        self.last = Some(next);
        out
    }
}

#[cfg(test)]
mod tests;
