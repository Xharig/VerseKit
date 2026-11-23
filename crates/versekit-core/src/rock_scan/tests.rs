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

use super::*;

/// The words of a panel: each line a row 20 points high, the words 80 points apart.
fn words(lines: &[&str]) -> Vec<Word> {
    let mut out = Vec::new();
    for (row, line) in lines.iter().enumerate() {
        for (column, text) in line.split_whitespace().enumerate() {
            out.push(Word::new(text, 10.0 + 80.0 * column as f64, 10.0 + 20.0 * row as f64, 60.0, 14.0));
        }
    }
    out
}

const PANEL: [&str; 9] = [
    "SCAN RESULTS",
    "QUANTAINIUM (RAW)",
    "MASS: 10895",
    "RESISTANCE: 31%",
    "INSTABILITY: 9.50",
    "COMPOSITION 5.50 SCU",
    "32.15% QUANTAINIUM (RAW)",
    "67.85% INERT MATERIALS",
    "OVERCHARGE",
];

fn digits(read: &Option<Read>) -> Option<&str> {
    read.as_ref().map(|r| r.digits.as_str())
}

#[test]
fn a_panel_gives_every_value_with_its_frame() {
    let panel = parse(&words(&PANEL));
    assert!(panel.found);
    assert_eq!(digits(&panel.mass), Some("10895"));
    assert_eq!(digits(&panel.resistance), Some("31"));
    assert_eq!(digits(&panel.instability), Some("9.50"));
    assert_eq!(digits(&panel.scu), Some("5.50"));
    let shares: Vec<(&str, &str)> = panel.shares.iter().map(|(n, r)| (n.as_str(), r.digits.as_str())).collect();
    assert_eq!(shares, vec![("QUANTAINIUM (RAW)", "32.15"), ("INERT MATERIALS", "67.85")]);
    assert_eq!(panel.mass.unwrap().frame, (90.0, 50.0, 60.0, 14.0));
}

#[test]
fn the_slashed_zero_and_lookalikes_become_digits_and_anything_else_no_value() {
    assert_eq!(number_text("1Ø895").as_deref(), Some("10895"));
    assert_eq!(number_text("l2,680").as_deref(), Some("12680"));
    assert_eq!(number_text("0%").as_deref(), Some("0"));
    assert_eq!(number_text("9,50").as_deref(), Some("9.50"));
    assert_eq!(number_text("5.50SCU"), None);
    assert_eq!(number_text("OO"), None, "no real digit");
    assert_eq!(number_text("9.5.0"), None);
    assert_eq!(number_text("12x4"), None);
}

#[test]
fn a_mass_with_a_point_is_no_mass() {
    let panel = parse(&words(&["MASS: 108.95"]));
    assert_eq!(panel.mass, None);
}

#[test]
fn the_german_labels_are_read_too() {
    let panel = parse(&words(&["SCANERGEBNISSE", "MASSE: 5300", "WIDERSTAND: 0%", "INSTABILITÄT: 5.32"]));
    assert_eq!(digits(&panel.mass), Some("5300"));
    assert_eq!(digits(&panel.resistance), Some("0"));
    assert_eq!(digits(&panel.instability), Some("5.32"));
}

#[test]
fn a_value_counts_only_where_every_reading_agrees_and_at_least_two_read_it() {
    let one = parse(&words(&PANEL));
    let mut other = PANEL;
    other[2] = "MASS: 10395";
    let two = parse(&words(&other));
    let merged = merge(&[one.clone(), two]);
    assert_eq!(merged.mass, None, "two readings disagree");
    assert_eq!(digits(&merged.resistance), Some("31"));
    let alone = merge(std::slice::from_ref(&one));
    assert_eq!(alone.mass, None, "one reading is not enough");
    let missing = parse(&words(&["SCAN RESULTS", "MASS:", "RESISTANCE: 31%"]));
    let merged = merge(&[one.clone(), one.clone(), missing]);
    assert_eq!(digits(&merged.mass), Some("10895"), "a reading without the value does not stop the others");
}

#[test]
fn the_shares_count_only_whole_and_adding_up_to_100() {
    let one = parse(&words(&PANEL));
    assert_eq!(merge(&[one.clone(), one.clone()]).shares.len(), 2);
    let mut short = PANEL;
    short[7] = "57.85% INERT MATERIALS";
    let short = parse(&words(&short));
    assert!(merge(&[short.clone(), short]).shares.is_empty(), "90 % is not the whole rock");
    let cut = parse(&words(&PANEL[..7]));
    assert!(merge(&[one, cut]).shares.is_empty(), "a reading lost a row");
}

#[test]
fn a_share_reads_with_the_name_first_and_with_a_gap_before_the_sign() {
    let panel = parse(&words(&["COMPOSITION 2.00 SCU", "GOLD (ORE) 40.00%", "60.00 % INERT MATERIALS"]));
    let shares: Vec<(&str, &str)> = panel.shares.iter().map(|(n, r)| (n.as_str(), r.digits.as_str())).collect();
    assert_eq!(shares, vec![("GOLD (ORE)", "40.00"), ("INERT MATERIALS", "60.00")]);
    assert_eq!(digits(&panel.scu), Some("2.00"));
}

/// The panel as the game writes it in German (LIVE, November 2026): the quality after each share, the same ore
/// twice in two qualities, the game's own word in the bar below the instability, a small `ä`.
#[test]
fn the_german_panel_with_qualities_and_the_games_word_is_read_whole() {
    let lines = [
        "SCANERGEBNISSE",
        "BEXALITE (RAW)",
        "MASSE: 32975",
        "WIDERSTAND: 61%",
        "INSTABILITäT: 496.36",
        "( UNMöGLICH )",
        "ZUSAMMENSETZUNG 34.99 SCU",
        "6.76% BEXALITE (RAW) 597",
        "74.53% BEXALITE (RAW) 302",
        "3.40% GOLD (ORE) 553",
        "4.98% BORASE (ORE) 584",
        "10.31% INERT MATERIALS 0",
    ];
    let panel = parse(&words(&lines));
    assert!(panel.found);
    assert_eq!(digits(&panel.mass), Some("32975"));
    assert_eq!(digits(&panel.resistance), Some("61"));
    assert_eq!(digits(&panel.instability), Some("496.36"));
    assert_eq!(digits(&panel.scu), Some("34.99"));
    assert_eq!(panel.verdict.as_deref(), Some("UNMÖGLICH"));
    let shares: Vec<(&str, &str)> = panel.shares.iter().map(|(n, r)| (n.as_str(), r.digits.as_str())).collect();
    assert_eq!(
        shares,
        vec![
            ("BEXALITE (RAW)", "6.76"),
            ("BEXALITE (RAW)", "74.53"),
            ("GOLD (ORE)", "3.40"),
            ("BORASE (ORE)", "4.98"),
            ("INERT MATERIALS", "10.31")
        ]
    );
    let qualities: Vec<Option<&str>> = panel.qualities.iter().map(|q| q.as_ref().map(|r| r.digits.as_str())).collect();
    assert_eq!(qualities, vec![Some("597"), Some("302"), Some("553"), Some("584"), Some("0")]);
    let sure = Sure::of(&merge(&[panel.clone(), panel.clone()]), |_| true);
    assert_eq!(sure.shares.len(), 5, "99.98 % adds up");
    assert_eq!(sure.qualities, vec![Some(597.0), Some(302.0), Some(553.0), Some(584.0), Some(0.0)]);
    assert_eq!(sure.verdict.as_deref(), Some("UNMÖGLICH"));
    let unsure_quality = Sure::of(&merge(&[panel.clone(), panel]), |r| r.digits != "302");
    assert!(unsure_quality.shares.is_empty() && unsure_quality.qualities.is_empty(), "one quality unsure: the list goes");
}

#[test]
fn readings_that_differ_in_a_quality_give_no_shares() {
    let lines = |quality: &str| {
        let row = format!("100.00% GOLD (ORE) {quality}");
        parse(&words(&["SCANERGEBNISSE", "ZUSAMMENSETZUNG 1.00 SCU", &row]))
    };
    assert_eq!(merge(&[lines("553"), lines("553")]).shares.len(), 1);
    assert!(merge(&[lines("553"), lines("583")]).shares.is_empty());
    assert_eq!(lines("1553").qualities, vec![None], "beyond 1000 is no quality");
}

#[test]
fn the_games_word_counts_only_where_the_readings_agree() {
    let with = |word: &str| parse(&words(&["SCAN RESULTS", "MASS: 5123", "INSTABILITY: 9.51", word, "COMPOSITION 2.15 SCU"]));
    assert_eq!(merge(&[with("EASY"), with("EASY")]).verdict.as_deref(), Some("EASY"));
    assert_eq!(merge(&[with("EASY"), with("EAST")]).verdict, None);
    assert_eq!(with("12.5").verdict, None, "a row with digits is no word");
}

/// A chunk of 763 kg: the HUD writes its SCU in thousandths (`627.13m SCU`), the number covering the end of the
/// German label (`ZUSAMMENSETZU`).
#[test]
fn thousandths_of_an_scu_and_a_label_cut_short_are_read() {
    let read = |scu_row: &str| {
        let panel = parse(&words(&["SCANERGEBNISSE", "MASSE: 763", "WIDERSTAND: 55%", scu_row, "50.51% BORASE (ORE) 584"]));
        Sure::of(&merge(&[panel.clone(), panel]), |_| true)
    };
    let milli = read("ZUSAMMENSETZU 627.13m SCU");
    assert!(milli.scu.is_some_and(|scu| (scu - 0.62713).abs() < 1e-9), "{:?}", milli.scu);
    assert_eq!(read("627.13m SCU").scu.map(|s| (s * 1e5).round()), Some(62713.0), "the label covered whole");
    assert_eq!(read("ZUSAMMENSETZUNG 627.13 SCU").scu, None, "763 kg in 627 SCU is no rock: an m lost");
    assert_eq!(read("ZUSAMMENSETZUNG 0.86 SCU").scu, Some(0.86));
    for label in ["ANTEILE", "INHALT", "GEHALT"] {
        let row = format!("{label} 627.13m SCU");
        assert!(read(&row).scu.is_some(), "a shorter word of a translation: {label}");
    }
    let cargo = parse(&words(&["SCANERGEBNISSE", "MASSE: 763", "CARGO 1.72 / 96.00 SCU"]));
    assert_eq!(cargo.scu, None, "the cargo of the ship is no composition");
    let no_mass = Sure::of(&merge(&vec![parse(&words(&["SCAN RESULTS", "COMPOSITION 0.86 SCU"])); 2]), |_| true);
    assert_eq!(no_mass.scu, None, "without a mass the SCU cannot be checked");
}

/// A chunk not scanned yet: the panel stands, but empty.
#[test]
fn an_empty_panel_gives_no_values() {
    let panel = parse(&words(&[
        "SCANERGEBNISSE",
        "MASSE: 0.00",
        "WIDERSTAND: 0%",
        "INSTABILITäT: 0.00",
        "ZUSAMMENSETZUNG 0.00 SCU",
    ]));
    assert!(panel.found, "the panel is in sight");
    assert_eq!(panel, Panel { found: true, ..Panel::default() }, "but no 0 % resistance and no 0 SCU");
}

#[test]
fn a_picture_without_the_panel_finds_nothing() {
    let panel = parse(&words(&["OVERCHARGE", "OPTIMAL", "12,680"]));
    assert!(!panel.found);
    assert_eq!(panel, Panel::default());
}

/// A grey crop with the digits drawn as blocks of 9 × 12 points, 3 points apart: `0` a ring with a slash from the
/// lower left to the upper right, `8` two rings one above the other, `1` a bar two points wide.
fn crop(text: &str) -> Gray {
    let (w, h) = (4 + text.len() * 12, 18);
    let mut data = vec![10u8; w * h];
    for (i, c) in text.chars().enumerate() {
        let left = 2 + i * 12;
        let top = 3;
        let mut set = |x: usize, y: usize| data[(top + y) * w + left + x] = 240;
        match c {
            '1' => (0..12).for_each(|y| {
                set(4, y);
                set(5, y);
            }),
            '0' | '8' => {
                for y in 0..12 {
                    for x in 0..9 {
                        let ring = x == 0 || x == 8 || y == 0 || y == 11;
                        let inner = if c == '8' { y == 5 || y == 6 } else { x + y == 10 || x + y == 11 };
                        if ring || inner {
                            set(x, y);
                        }
                    }
                }
            }
            _ => unreachable!(),
        }
    }
    Gray::new(w, h, data)
}

fn read(text: &str) -> Read {
    Read { digits: text.into(), frame: (0.0, 0.0, 1.0, 1.0) }
}

#[test]
fn a_zero_and_an_eight_are_told_apart_by_their_holes() {
    assert!(zero_or_eight(&read("10"), &crop("10")));
    assert!(zero_or_eight(&read("18"), &crop("18")));
    assert!(!zero_or_eight(&read("18"), &crop("10")), "an 8 read where the picture has a slashed zero");
    assert!(!zero_or_eight(&read("10"), &crop("18")), "a 0 read where the picture has an 8");
    assert!(!zero_or_eight(&read("108"), &crop("10")), "a digit more than the picture has");
    assert!(zero_or_eight(&read("11"), &Gray::new(1, 1, vec![0])), "no 0 and no 8: nothing to check");
}

#[test]
fn only_checked_values_are_sure_and_a_resistance_beyond_100_is_none() {
    let panel = merge(&[parse(&words(&PANEL)), parse(&words(&PANEL))]);
    let all = Sure::of(&panel, |_| true);
    assert_eq!((all.mass, all.resistance, all.instability, all.scu), (Some(10895.0), Some(31.0), Some(9.5), Some(5.5)));
    assert_eq!(all.shares, vec![("QUANTAINIUM (RAW)".to_string(), 32.15), ("INERT MATERIALS".to_string(), 67.85)]);
    let no_eights = Sure::of(&panel, |r| !r.digits.contains('8'));
    assert_eq!(no_eights.mass, None);
    assert!(no_eights.shares.is_empty(), "one share unsure: the list goes whole");
    assert_eq!(no_eights.resistance, Some(31.0));
    let mut odd = PANEL;
    odd[3] = "RESISTANCE: 131%";
    let odd = merge(&[parse(&words(&odd)), parse(&words(&odd))]);
    assert_eq!(Sure::of(&odd, |_| true).resistance, None);
}

#[test]
fn a_value_stands_after_two_pictures_alike_and_goes_with_the_panel() {
    let mut steady = Steady::default();
    let sure = |mass: f64| Sure { mass: Some(mass), resistance: Some(20.0), ..Sure::default() };
    assert!(steady.step(Some(sure(5000.0))).is_empty(), "one picture is not enough");
    let both = steady.step(Some(sure(5000.0)));
    assert_eq!((both.mass, both.resistance), (Some(5000.0), Some(20.0)));
    let changed = steady.step(Some(sure(5300.0)));
    assert_eq!((changed.mass, changed.resistance), (None, Some(20.0)), "each value on its own");
    assert!(steady.step(None).is_empty());
    assert!(steady.step(Some(sure(5300.0))).is_empty(), "after the panel went, two pictures again");
}
