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

//! Comparing version numbers (`updater.is_newer`).

const MARKS: [&str; 4] = ["-rc", "-beta", "-alpha", "-dev"];

/// A run of digits as a number; larger than `u64` counts as the largest.
fn number(digits: &str) -> u64 {
    digits.bytes().fold(0u64, |n, d| n.saturating_mul(10).saturating_add(u64::from(d - b'0')))
}

/// The digits at the start of `text`.
fn digits(text: &str) -> &str {
    &text[..text.bytes().take_while(u8::is_ascii_digit).count()]
}

/// `v2.0.1-fork.3` → `(2, 0, 1)`: the first `n.n.n`; without one `(0, 0, 0)`.
pub fn parts(version: &str) -> (u64, u64, u64) {
    for (start, _) in version.char_indices() {
        let rest = &version[start..];
        let a = digits(rest);
        if a.is_empty() || (start > 0 && version.as_bytes()[start - 1].is_ascii_digit()) {
            continue;
        }
        let Some(after_a) = rest[a.len()..].strip_prefix('.') else { continue };
        let b = digits(after_a);
        let Some(after_b) = after_a[b.len()..].strip_prefix('.').filter(|_| !b.is_empty()) else { continue };
        let c = digits(after_b);
        if !c.is_empty() {
            return (number(a), number(b), number(c));
        }
    }
    (0, 0, 0)
}

/// The first pre-release mark: where it ends, and its rank.
fn first_mark(version: &str) -> Option<(usize, u8)> {
    MARKS
        .iter()
        .filter_map(|m| version.find(m).map(|at| (at, at + m.len(), rank_of(m))))
        .min()
        .map(|(_, end, rank)| (end, rank))
}

/// `-dev` before `-alpha` before `-beta` before `-rc`.
fn rank_of(mark: &str) -> u8 {
    match mark {
        "-rc" => 3,
        "-beta" => 2,
        "-alpha" => 1,
        _ => 0,
    }
}

/// Where the first pre-release mark ends.
fn mark_end(version: &str) -> Option<usize> {
    first_mark(version).map(|(end, _)| end)
}

/// The rank of the pre-release mark: `-dev` 0, `-alpha` 1, `-beta` 2, `-rc` 3; without one 0.
pub fn prerelease_rank(version: &str) -> u8 {
    first_mark(version).map_or(0, |(_, rank)| rank)
}

/// Whether the version carries `-dev`, `-rc`, `-beta` or `-alpha`.
pub fn is_prerelease(version: &str) -> bool {
    mark_end(version).is_some()
}

/// The number after the pre-release mark: `2.0.0-rc2` → 2, `-dev` → 0.
pub fn prerelease_number(version: &str) -> u64 {
    let Some(end) = mark_end(version) else { return 0 };
    let rest = &version[end..];
    number(digits(rest.strip_prefix('.').unwrap_or(rest)))
}

/// Whether `other` is newer than `own`. A pre-release counts as older than
/// the same number without one.
pub fn is_newer(other: &str, own: &str) -> bool {
    let (a, b) = (parts(other), parts(own));
    if a != b {
        return a > b;
    }
    let (pa, pb) = (is_prerelease(other), is_prerelease(own));
    if pa != pb {
        return pb;
    }
    if !pa {
        return false;
    }
    let (ra, rb) = (prerelease_rank(other), prerelease_rank(own));
    if ra != rb {
        return ra > rb;
    }
    prerelease_number(other) > prerelease_number(own)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_like_the_updater() {
        assert!(is_newer("5.0.1", "5.0.0"));
        assert!(!is_newer("5.0.0", "5.0.0"));
        assert!(is_newer("5.0.0", "5.0.0-dev"));
        assert!(!is_newer("5.0.0-dev", "5.0.0"));
        assert!(is_newer("2.0.0-rc10", "2.0.0-rc2"));
        assert!(is_newer("v4.13.0", "4.12.9"));
        assert!(!is_newer("kaputt", "1.0.0"));
        assert_eq!(parts("99999999999999999999999.1.2"), (u64::MAX, 1, 2));
    }

    /// The marks of one number stand in a fixed order, whatever number follows them; the tag of a beta release
    /// (`beta-v…`) compares like its version.
    #[test]
    fn dev_alpha_beta_rc_finished() {
        let order = ["5.7.0-dev", "5.7.0-alpha.9", "5.7.0-beta.1", "5.7.0-beta.12", "5.7.0-rc1", "5.7.0-rc2", "5.7.0"];
        for (i, lower) in order.iter().enumerate() {
            for higher in &order[i + 1..] {
                assert!(is_newer(higher, lower), "{higher} after {lower}");
                assert!(!is_newer(lower, higher), "{lower} not after {higher}");
            }
        }
        assert!(is_newer("beta-v5.7.0-beta.2", "5.7.0-beta.1"));
        assert!(!is_newer("beta-v5.7.0-beta.1", "5.7.0-beta.1"));
        assert!(is_newer("5.7.0-rc1", "beta-v5.7.0-beta.3"));
        assert!(is_newer("beta-v5.7.0-beta.1", "5.6.1"));
        // Unchanged where only one kind of mark is in play.
        assert!(is_newer("5.0.0-rc10", "5.0.0-rc9"));
        assert!(is_newer("5.0.0-rc1", "5.0.0-dev"));
    }
}
