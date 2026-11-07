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

//! The words are built the way the text recognition gives them (text and frame in pixels); the digits are drawn
//! in a small font of their own with a slashed zero.

use super::digits::{Cell, Digits, Gray, classify, glyphs, learn_into, read_cell};
use super::*;
use crate::jsonio;

const MATERIALS: [&str; 9] = [
    "Agricium",
    "Aslarite",
    "Feynmaline",
    "Heart of the Woods",
    "Janalite",
    "Ouratite",
    "Stileron",
    "Taranite",
    "Titanium",
];

fn index_of(extra: &[&str]) -> Index {
    let names: Vec<String> = MATERIALS.iter().chain(extra).map(|s| s.to_string()).collect();
    Index::new(&names, |_| false)
}

fn word(t: &str, x: f64, y: f64) -> Word {
    Word::new(t, x, y, 8.0 * t.chars().count() as f64, 12.0)
}

fn sized(t: &str, x: f64, y: f64, w: f64, h: f64) -> Word {
    Word::new(t, x, y, w, h)
}

/// A row of a made-up table: name, quality, yield, to do.
type Row<'a> = (&'a str, Option<Int>, Option<Int>, Option<Int>);

/// A refinery table as the recognition reads it; `None` leaves a cell out.
fn table(values: &[Row]) -> Vec<Word> {
    let mut words = vec![
        word("GEWONNENE", 10.0, 10.0),
        word("MATERIALIEN", 90.0, 10.0),
        sized("(CSCU)", 190.0, 10.0, 40.0, 12.0),
        sized("OUALITåT", 222.0, 10.0, 48.0, 12.0),
        sized("AUSBEUTE", 284.0, 10.0, 55.0, 12.0),
        sized("ZU", 363.0, 10.0, 13.0, 12.0),
        sized("FERTIG", 400.0, 10.0, 36.0, 12.0),
    ];
    for (i, (name, quality, amount, to_do)) in values.iter().enumerate() {
        let y = 50.0 + i as f64 * 40.0;
        let mut x = 20.0;
        for part in name.split(' ') {
            words.push(word(part, x, y));
            x += 8.0 * part.len() as f64 + 6.0;
        }
        for (value, column) in [(quality, 240.0), (amount, 305.0), (to_do, 364.0)] {
            if let Some(value) = value {
                words.push(sized(&value.to_string(), column, y + 1.0, 18.0, 10.0));
            }
        }
    }
    words.push(word("VERBLEIBENDE", 10.0, 400.0));
    words.push(word("ZEIT", 110.0, 400.0));
    words.push(word("57m", 300.0, 400.0));
    words.push(word("AUSBEUTE", 10.0, 360.0));
    words.push(word("3.73", 300.0, 360.0));
    words
}

/// The words of a picture of 5120 × 1440 with the terminal in the middle: at the left the profile of the station
/// (materials with the heads MATERIAL and AUSBEUTE at the heights of the rows of the table), right of it the yield
/// table with its heads, two rows and the row of the sum.
fn wide_screen(with_quality: bool) -> Vec<Word> {
    let mut words = vec![
        sized("MATERIAL", 1620.0, 520.0, 60.0, 9.0),
        sized("AUSBEUTE", 1860.0, 520.0, 55.0, 9.0),
        sized("Iron", 1650.0, 560.0, 30.0, 9.0),
        sized("(Ore)", 1685.0, 560.0, 35.0, 9.0),
        sized("+8%", 1880.0, 560.0, 25.0, 9.0),
        sized("Torite", 1650.0, 625.0, 40.0, 9.0),
        sized("(Ore)", 1695.0, 625.0, 35.0, 9.0),
        sized("+1%", 1880.0, 625.0, 25.0, 9.0),
        sized("GEWONNENE", 2000.0, 560.0, 60.0, 9.0),
        sized("MATERIALIEN", 2065.0, 560.0, 70.0, 9.0),
        sized("(CSCU)", 2140.0, 560.0, 40.0, 9.0),
        sized("AUSBEUTE", 2285.0, 560.0, 55.0, 9.0),
        sized("ZU", 2360.0, 556.0, 13.0, 9.0),
        sized("FERTIG", 2405.0, 561.0, 36.0, 9.0),
        sized("GOLD", 2030.0, 600.0, 40.0, 12.0),
        sized("553", 2235.0, 602.0, 18.0, 9.0),
        sized("122", 2295.0, 602.0, 18.0, 9.0),
        sized("122", 2350.0, 602.0, 18.0, 9.0),
        sized("TUNGSTEN", 2030.0, 650.0, 70.0, 12.0),
        sized("531", 2235.0, 652.0, 18.0, 9.0),
        sized("234", 2295.0, 652.0, 18.0, 9.0),
        sized("AUSBEUTE", 2005.0, 990.0, 60.0, 12.0),
        sized("12.76", 2270.0, 985.0, 60.0, 22.0),
        sized("cSCU", 2335.0, 982.0, 25.0, 9.0),
    ];
    if with_quality {
        words.push(sized("OUAUTiT", 2220.0, 560.0, 45.0, 9.0));
    }
    words
}

/// Two job cards side by side, each with heads, rows and sum.
fn two_cards() -> Vec<Word> {
    let mut words = wide_screen(true);
    for w in wide_screen(true) {
        if w.x < 1990.0 {
            continue;
        }
        let t = match w.t.as_str() {
            "GOLD" => "OURATITE",
            "TUNGSTEN" => "AGRICIUM",
            "12.76" => "7.33",
            other => other,
        };
        words.push(Word { t: t.to_string(), x: w.x + 620.0, ..w });
    }
    words
}

fn inside(area: Area, words: &[Word]) -> Vec<Word> {
    let (x, y, w, h) = (area.0 as f64, area.1 as f64, area.2 as f64, area.3 as f64);
    words.iter().filter(|o| x <= o.x && o.x + o.w <= x + w && y <= o.y && o.y + o.h <= y + h).cloned().collect()
}

fn found(rows: &[(&str, Int, Int)]) -> Vec<(String, Option<Int>, Option<Int>)> {
    rows.iter().map(|(m, q, a)| (m.to_string(), Some(*q), Some(*a))).collect()
}

#[test]
fn misread_names_find_their_material_and_headings_find_none() {
    let index = index_of(&[]);
    assert_eq!(match_material("OURATlTE", &index).as_deref(), Some("Ouratite"));
    assert_eq!(match_material("AGRICIIJM", &index).as_deref(), Some("Agricium"));
    assert_eq!(match_material("TITANIUM", &index).as_deref(), Some("Titanium"));
    assert_eq!(match_material("VERBLEIBENDE ZEIT", &index), None);
    assert_eq!(match_material("AUSBEUTE", &index), None);
    assert_eq!(match_material("xx", &index), None);
}

#[test]
fn numbers_swap_lookalikes_only_beside_a_real_digit() {
    assert_eq!(number("4B0", false), Some(480));
    assert_eq!(number("1.459", false), Some(1459));
    assert_eq!(number("1.234.567", false), Some(1_234_567));
    assert_eq!(number("12.76", false), None);
    assert_eq!(number("see", false), None);
    assert_eq!(number("57m", false), None);
    assert_eq!(number("sos", false), None);
    assert_eq!(number("sos", true), Some(505));
    assert_eq!(number("GOLD", true), None);
    assert_eq!(number(" 66:", false), Some(66));
}

#[test]
fn one_reading_gives_the_rows_and_a_missing_yield_stays_empty() {
    let index = index_of(&[]);
    let reading = table(&[
        ("TITANIUM", Some(516), Some(66), Some(66)),
        ("OURATlTE", Some(310), Some(421), Some(421)),
        ("HEART OF THE WOODS", Some(500), Some(12), Some(12)),
        ("ASLARITE", Some(287), None, Some(658)),
    ]);
    let read: Vec<_> =
        parse_rows(&rows(&reading), &index).into_iter().map(|r| (r.material, r.quality, r.amount)).collect();
    assert_eq!(
        read,
        vec![
            ("Titanium".to_string(), Some(516), Some(66)),
            ("Ouratite".to_string(), Some(310), Some(421)),
            ("Heart of the Woods".to_string(), Some(500), Some(12)),
            ("Aslarite".to_string(), Some(287), None),
        ]
    );
}

#[test]
fn a_value_counts_when_two_readings_read_it_alike_and_none_other_differs() {
    let index = index_of(&[]);
    let passes = vec![
        table(&[
            ("TITANIUM", Some(516), Some(66), Some(66)),
            ("OURATITE", Some(523), Some(41), Some(42)),
            ("STILERON", Some(330), Some(43), Some(44)),
            ("ASLARITE", Some(287), None, Some(658)),
            ("TARANITE", Some(525), Some(4), Some(5)),
        ]),
        table(&[
            ("TITANIUM", Some(516), Some(66), Some(66)),
            ("OURATITE", None, Some(41), Some(42)),
            ("STILERON", Some(338), Some(43), Some(44)),
            ("ASLARITE", Some(287), None, Some(658)),
            ("TARANITE", None, Some(4), Some(5)),
        ]),
        table(&[
            ("TITANIUM", Some(516), Some(66), Some(66)),
            ("OURATITE", Some(523), Some(41), Some(42)),
            ("STILERON", Some(33), Some(43), Some(44)),
            ("ASLARITE", None, None, Some(658)),
            ("TARANITE", None, Some(4), Some(5)),
        ]),
    ];
    let merged = merge_passes(&passes, &index, None, &Checks::default(), false);
    assert_eq!(merged.found, found(&[("Titanium", 516, 66), ("Ouratite", 523, 41)]));
    assert_eq!(merged.unsure, vec!["Stileron", "Aslarite", "Taranite"]);
    assert_eq!(as_text(&merged.found, &index), "Titanium 516 66 cSCU\nOuratite 523 41 cSCU");
    // Counter-check: with one vote as the limit the single reading of the quality of Taranite would stand.
    let one = merge_passes(&passes, &index, Some(1), &Checks::default(), false);
    assert!(one.found.contains(&("Taranite".to_string(), Some(525), Some(4))));

    let dispute = vec![
        table(&[("OURATITE", Some(523), Some(41), Some(42))]),
        table(&[("OURATITE", Some(523), Some(41), Some(42))]),
        table(&[("OURATITE", Some(23), Some(41), Some(42))]),
    ];
    let merged = merge_passes(&dispute, &index, None, &Checks::default(), false);
    assert!(merged.found.is_empty());
    assert_eq!(merged.unsure, vec!["Ouratite"]);

    let unknown = vec![vec![word("BANANE", 20.0, 50.0), word("500", 240.0, 50.0)]];
    assert_eq!(merge_passes(&unknown, &index, None, &Checks::default(), false), Merged::default());
}

#[test]
fn the_heads_keep_the_column_to_do_out_of_the_yield() {
    let index = index_of(&[]);
    let with_heads = table(&[("ASLARITE", Some(287), None, Some(658))]);
    let without: Vec<Word> = with_heads.iter().filter(|w| w.y != 10.0).cloned().collect();
    assert_eq!(parse_rows(&rows(&with_heads), &index)[0].amount, None);
    assert_eq!(parse_rows(&rows(&without), &index)[0].amount, Some(658));
}

#[test]
fn a_zero_or_eight_counts_only_when_confirmed() {
    let index = index_of(&["Savrilium"]);
    let reading = table(&[("SAVRILIUM", Some(905), Some(183), Some(183)), ("TITANIUM", Some(516), Some(66), Some(66))]);
    let passes = vec![reading; 4];
    let merged = merge_passes(&passes, &index, None, &Checks::default(), false);
    assert_eq!(merged.found, found(&[("Titanium", 516, 66)]));
    assert_eq!(merged.unsure, vec!["Savrilium"]);
    let yes = Checks { confirm: Some(Box::new(|_, _, _| Checked::Yes)), reread: None };
    assert!(merge_passes(&passes, &index, None, &yes, false).found.contains(&(
        "Savrilium".to_string(),
        Some(905),
        Some(183)
    )));

    let gold_index = Index::new(&["Gold".to_string()], |_| false);
    let gold = vec![
        sized("Gold", 0.0, 0.0, 40.0, 10.0),
        sized("588", 100.0, 0.0, 30.0, 10.0),
        sized("122", 200.0, 0.0, 30.0, 10.0),
    ];
    let two = vec![gold.clone(), gold];
    for (answer, expected) in [
        (Checked::Value(508), found(&[("Gold", 508, 122)])),
        (Checked::Yes, found(&[("Gold", 588, 122)])),
        (Checked::No, Vec::new()),
    ] {
        let checks = Checks { confirm: Some(Box::new(move |_, _, _| answer)), reread: None };
        assert_eq!(merge_passes(&two, &gold_index, None, &checks, false).found, expected, "{answer:?}");
    }
    let no = Checks { confirm: Some(Box::new(|_, _, _| Checked::No)), reread: None };
    let guessed = merge_passes(&two, &gold_index, None, &no, true);
    assert_eq!(guessed.found, found(&[("Gold", 588, 122)]));
    assert_eq!(guessed.unsure, vec!["Gold"]);
}

#[test]
fn loose_readings_end_the_name_and_give_way_to_real_digits() {
    let index = Index::new(&["Lindinium".to_string()], |_| false);
    let loose = vec![
        sized("Lindinium", 0.0, 0.0, 60.0, 10.0),
        sized("sos", 100.0, 0.0, 30.0, 10.0),
        sized("371", 200.0, 0.0, 30.0, 10.0),
    ];
    let real = vec![
        sized("Lindinium", 0.0, 0.0, 60.0, 10.0),
        sized("305", 100.0, 0.0, 30.0, 10.0),
        sized("tus", 200.0, 0.0, 30.0, 10.0),
    ];
    let row = material_rows(std::slice::from_ref(&loose), &index);
    assert_eq!(row.len(), 1);
    assert_eq!(row[0].loose_numbers[0].1, 505);
    let merged =
        merge_passes(&[loose.clone(), loose, real.clone(), real.clone()], &index, None, &Checks::default(), true);
    assert_eq!(merged.found, found(&[("Lindinium", 305, 371)]));
    let merged = merge_passes(&[real.clone(), real], &index, None, &Checks::default(), true);
    assert_eq!(as_text(&merged.found, &index), "Lindinium 305 ? cSCU");
}

#[test]
fn goods_in_pieces_get_no_unit() {
    let index = Index::new(&["Hadanite".to_string(), "Gold".to_string()], |name| name == "Hadanite");
    let rows = vec![("Hadanite".to_string(), Some(800), Some(3)), ("Gold".to_string(), None, Some(12))];
    assert_eq!(as_text(&rows, &index), "Hadanite 800 3\nGold ? 12 cSCU");
}

#[test]
fn the_tiles_cover_a_wide_screen_below_the_limit_of_the_recognition() {
    let jobs = search_jobs(5120, 1440);
    let covered = (0..5120).step_by(64).all(|x| {
        (0..1440).step_by(64).all(|y| {
            jobs.iter().any(|j| j.area.0 <= x && x < j.area.0 + j.area.2 && j.area.1 <= y && y < j.area.1 + j.area.3)
        })
    });
    assert!(covered);
    assert!(jobs.iter().all(|j| (j.area.2.max(j.area.3) as f64) * j.scale <= OCR_LIMIT && j.scale >= 2.5));
    assert_eq!(search_jobs(800, 600)[0].scale, 4.0);
}

#[test]
fn the_part_of_the_table_leaves_the_profile_of_the_station_out() {
    let index = index_of(&["Gold", "Tungsten", "Iron", "Torite"]);
    let words = wide_screen(true);
    let area = find_table(&words, 5120, 1440).expect("table");
    assert!(1915 < area.0 && area.0 <= 2000 && area.1 <= 560, "{area:?}");
    assert!(900 < area.1 + area.3 && area.1 + area.3 <= 990 && area.0 + area.2 >= 2441, "{area:?}");
    let doubled = merge_words(&[words[..20].to_vec(), words[10..].to_vec()]);
    assert_eq!(doubled.len(), words.len());
    assert_eq!(find_table(&doubled, 5120, 1440), Some(area));
    let within = inside(area, &words);
    let merged = merge_passes(&[within.clone(), within], &index, None, &Checks::default(), false);
    assert_eq!(merged.found, found(&[("Gold", 553, 122), ("Tungsten", 531, 234)]));
    assert!(merged.unsure.is_empty());
    // Counter-checks: without the part Torite of the profile would stand as unsure; without the head of the
    // quality the head of the profile is no anchor.
    assert!(
        merge_passes(&[words.clone(), words], &index, None, &Checks::default(), false)
            .unsure
            .contains(&"Torite".to_string())
    );
    assert_eq!(find_table(&wide_screen(false), 5120, 1440), None);
    assert!(find_table(&table(&[]), 5120, 1440).is_some());
    assert_eq!(find_table(&[word("HANGAR", 10.0, 10.0)], 5120, 1440), None);
}

#[test]
fn two_cards_side_by_side_are_two_tables_with_their_own_sums() {
    let tables = find_tables(&two_cards(), 5120, 1440);
    assert_eq!(tables.len(), 2);
    assert!(tables[0].area.0 + tables[0].area.2 <= tables[1].area.0 + 1);
    assert_eq!(tables.iter().map(|t| t.total.as_str()).collect::<Vec<_>>(), ["12.76", "7.33"]);
    assert_eq!(find_tables(&wide_screen(true), 5120, 1440).len(), 1);
}

/// The card as the German terminal shows it: the head of the yield cut to `AUSBEU`; with `amount` the head `ANZ.`
/// between quality and yield, as while setting a job up, with a number of its own in each row.
fn cut_heads(amount: bool) -> Vec<Word> {
    let mut words: Vec<Word> = wide_screen(true)
        .into_iter()
        .map(|w| if w.t == "AUSBEUTE" && w.x > 2200.0 { Word { t: "AUSBEU".into(), ..w } } else { w })
        .collect();
    if amount {
        for w in &mut words {
            if w.x >= 2280.0 && w.y < 900.0 {
                w.x += 60.0;
            }
        }
        words.push(sized("ANZ.", 2280.0, 560.0, 25.0, 9.0));
        words.push(sized("1212", 2280.0, 602.0, 24.0, 9.0));
        words.push(sized("424", 2280.0, 652.0, 18.0, 9.0));
    }
    words
}

#[test]
fn the_german_head_cut_to_ausbeu_still_finds_the_table_and_its_lines() {
    let index = index_of(&["Gold", "Tungsten", "Iron", "Torite"]);
    for amount in [false, true] {
        let words = cut_heads(amount);
        assert_eq!(find_tables(&words, 5120, 1440).len(), 1, "amount column: {amount}");
        let mut ocr = FakeOcr { words, calls: 0 };
        let (jobs, _) = read_image((5120, 1440), None, &mut ocr, &index, &Digits::default(), None).expect("read");
        // The number below `ANZ.` slips neither into the quality nor into the yield.
        assert_eq!(jobs.iter().map(|j| j.text.as_str()).collect::<Vec<_>>(), ["Gold 553 122 cSCU\nTungsten 531 234 cSCU"]);
    }
}

#[test]
fn the_symbol_and_raw_in_front_of_a_name_and_a_doubled_label_are_no_hindrance() {
    let index = index_of(&["Ouratite", "Iron"]);
    assert_eq!(match_material("e RAW OURATITE", &index).as_deref(), Some("Ouratite"));
    assert_eq!(match_material("e IRON", &index).as_deref(), Some("Iron"));
    let words = [
        sized("VERBLEIBENDE", 100.0, 500.0, 90.0, 12.0),
        sized("LEIBENDE", 150.0, 500.0, 60.0, 12.0),
        sized("ZEIT", 200.0, 500.0, 30.0, 12.0),
        sized("3h", 300.0, 500.0, 20.0, 12.0),
        sized("Om", 330.0, 500.0, 20.0, 12.0),
    ];
    assert_eq!(remaining_seconds(&words, (90, 0, 300, 600)), Some(3 * 3600));
}

#[test]
fn a_head_missed_in_one_reading_leaves_its_numbers_in_their_own_column() {
    // A running job: QUALITÄT, AUSBEU, ZU, FERTIG; this reading missed the head ZU and the yield of the first row.
    let index = index_of(&["Iron"]);
    let table = rows(&[
        sized("(CSCQßALlTäAUSBEU", 1210.0, 100.0, 152.0, 12.0),
        sized("FERTIG", 1430.0, 100.0, 36.0, 12.0),
        sized("IRON", 1080.0, 140.0, 30.0, 12.0),
        sized("664", 1265.0, 140.0, 18.0, 12.0),
        sized("301", 1381.0, 140.0, 18.0, 12.0),
        sized("251", 1439.0, 140.0, 18.0, 12.0),
        sized("IRON", 1080.0, 180.0, 30.0, 12.0),
        sized("907", 1265.0, 180.0, 18.0, 12.0),
        sized("192", 1323.0, 180.0, 18.0, 12.0),
        sized("105", 1381.0, 180.0, 18.0, 12.0),
        sized("87", 1442.0, 180.0, 12.0, 12.0),
    ]);
    let parsed = parse_rows(&table, &index);
    assert_eq!(parsed.iter().map(|r| (r.quality, r.amount)).collect::<Vec<_>>(), [(Some(664), None), (Some(907), Some(192))]);
}

#[test]
fn heads_read_into_one_word_are_cut_apart_and_the_table_is_found() {
    // As the recognition read the German terminal: `(CSCU)`, `QUALITÄT` and `AUSBEU` in one word, Q as W, I as l.
    let words: Vec<Word> = wide_screen(true)
        .into_iter()
        .filter(|w| !(w.t == "(CSCU)" || w.t == "OUAUTiT" || (w.t == "AUSBEUTE" && w.x > 2200.0 && w.y < 900.0)))
        .chain([sized("(CSCWALlTäAUSBEU", 2140.0, 560.0, 200.0, 9.0)])
        .collect();
    let split = split_heads(&[sized("(CSCWALlTäAUSBEU", 2140.0, 560.0, 200.0, 9.0), sized("AUSBEUTE", 0.0, 0.0, 50.0, 9.0)]);
    assert_eq!(split.iter().map(|w| w.t.as_str()).collect::<Vec<_>>(), ["(CSCWALlTä", "AUSBEU", "AUSBEUTE"]);
    assert_eq!(find_tables(&words, 5120, 1440).len(), 1);
    let index = index_of(&["Gold", "Tungsten", "Iron", "Torite"]);
    let mut ocr = FakeOcr { words, calls: 0 };
    let (jobs, _) = read_image((5120, 1440), None, &mut ocr, &index, &Digits::default(), None).expect("read");
    assert_eq!(jobs.iter().map(|j| j.text.as_str()).collect::<Vec<_>>(), ["Gold 553 122 cSCU\nTungsten 531 234 cSCU"]);
}

#[test]
fn a_job_read_before_and_after_scrolling_is_one_list_and_another_job_is_none() {
    let first = "Iron 664 551 cSCU\nIron 710 98 cSCU\nQuantainium 344 70 cSCU\nLaranite 698 106 cSCU";
    let second = "Iron 710 98 cSCU\nQuantainium 344 70 cSCU\nLaranite 698 106 cSCU\nTungsten 530 35 cSCU";
    let (joined, added) = join_scrolled(first, second).expect("the same job");
    assert_eq!(joined, format!("{first}\nTungsten 530 35 cSCU"));
    assert_eq!(added, 1);
    // Read twice without scrolling: nothing comes twice.
    assert_eq!(join_scrolled(first, first), Some((first.to_string(), 0)));
    // Two equal lines in the new reading stay two.
    let (twice, _) = join_scrolled("Iron 664 551 cSCU", "Iron 664 551 cSCU\nIron 664 551 cSCU").expect("same");
    assert_eq!(twice, "Iron 664 551 cSCU\nIron 664 551 cSCU");
    // No line in common, or an empty field: another job, it replaces.
    assert_eq!(join_scrolled(first, "Gold 553 122 cSCU"), None);
    assert_eq!(join_scrolled("", second), None);
}

/// The text recognition of the tests: the words of `words` that lie wholly in the part of each job.
struct FakeOcr {
    words: Vec<Word>,
    calls: usize,
}

impl Ocr for FakeOcr {
    fn read(&mut self, jobs: &[Job]) -> Result<Vec<Vec<Word>>, ScanError> {
        self.calls += 1;
        Ok(jobs.iter().map(|j| inside(j.area, &self.words)).collect())
    }
}

#[test]
fn a_picture_with_two_cards_gives_two_jobs_and_keeps_what_was_unsure() {
    let index = index_of(&["Gold", "Tungsten", "Iron", "Torite"]);
    let mut ocr = FakeOcr { words: two_cards(), calls: 0 };
    let (jobs, kept) = read_image((5120, 1440), None, &mut ocr, &index, &Digits::default(), None).expect("read");
    assert_eq!(
        jobs.iter().map(|j| j.text.as_str()).collect::<Vec<_>>(),
        ["Gold 553 122 cSCU\nTungsten 531 234 cSCU", "Ouratite 553 122 cSCU\nAgricium 531 234 cSCU"]
    );
    assert_eq!(jobs[1].materials, ["Ouratite", "Agricium"]);
    assert_eq!(jobs[1].total, "7.33");
    assert!(!kept);
    assert_eq!(ocr.calls, 3);

    let mut asked = Vec::new();
    let mut keep = |what: Keep| {
        asked.push(what);
        true
    };
    let mut nothing = FakeOcr { words: vec![word("HANGAR", 10.0, 10.0)], calls: 0 };
    let read = read_image((5120, 1440), None, &mut nothing, &index, &Digits::default(), Some(&mut keep)).expect("read");
    assert_eq!(read, (Vec::new(), true));
    let mut unsure = FakeOcr { words: wide_screen(true).into_iter().filter(|w| w.t != "234").collect(), calls: 0 };
    let (jobs, kept) =
        read_image((5120, 1440), None, &mut unsure, &index, &Digits::default(), Some(&mut keep)).expect("read");
    assert!(kept);
    assert_eq!(jobs[0].unsure, ["Tungsten"]);
    let mut sure = FakeOcr { words: wide_screen(true), calls: 0 };
    let (_, kept) =
        read_image((5120, 1440), None, &mut sure, &index, &Digits::default(), Some(&mut keep)).expect("read");
    assert!(!kept);
    assert_eq!(asked.len(), 2);
    assert_eq!(asked[0], Keep::Picture);
    assert!(matches!(asked[1], Keep::Table(_)));
}

#[test]
fn the_state_of_a_card_comes_from_the_words_over_its_table() {
    let words = vec![
        sized("ABGESCHLOSSEN", 120.0, 150.0, 120.0, 12.0),
        sized("WIRD", 520.0, 150.0, 40.0, 12.0),
        sized("VERARBEITET", 565.0, 150.0, 100.0, 12.0),
        sized("FERTIG", 700.0, 230.0, 50.0, 12.0),
    ];
    assert_eq!(job_state(&words, (100, 200, 300, 400)), State::Done);
    assert_eq!(job_state(&words, (500, 200, 300, 400)), State::Running);
    assert_eq!(job_state(&[sized("Completed", 520.0, 150.0, 90.0, 12.0)], (500, 200, 300, 400)), State::Done);
    assert_eq!(job_state(&words, (900, 200, 100, 100)), State::Unknown);
}

#[test]
fn the_time_left_is_read_with_misread_digits_put_right() {
    let card = table(&[("TITANIUM", Some(516), Some(66), Some(66))]);
    assert_eq!(remaining_seconds(&card, (0, 0, 500, 500)), Some(57 * 60));
    let words = vec![
        word("VERBLEIBENDE", 10.0, 100.0),
        word("ZEIT", 110.0, 100.0),
        word("43m", 200.0, 100.0),
        word("57s", 240.0, 100.0),
        word("Ih", 600.0, 100.0),
        word("3m", 640.0, 100.0),
    ];
    assert_eq!(remaining_seconds(&words, (0, 0, 400, 400)), Some(43 * 60 + 57));
    assert_eq!(remaining_seconds(&words, (500, 0, 400, 400)), Some(3780));
    assert_eq!(remaining_seconds(&[word("GOLD", 10.0, 10.0)], (0, 0, 400, 400)), None);
    assert_eq!(
        remaining_seconds(&[word("2d", 10.0, 10.0), word("4H", 40.0, 10.0)], (0, 0, 400, 400)),
        Some(2 * 86400 + 4 * 3600)
    );
}

// ------------------------------------------------------------ digits

/// The digits of the test font, 5 × 7: the zero carries a slash.
fn shape(digit: char) -> [&'static str; 7] {
    match digit {
        '0' => [".###.", "#...#", "#..##", "#.#.#", "##..#", "#...#", ".###."],
        '1' => ["..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###."],
        '2' => [".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####"],
        '3' => ["####.", "....#", "....#", ".###.", "....#", "....#", "####."],
        '4' => ["...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#."],
        '5' => ["#####", "#....", "####.", "....#", "....#", "#...#", ".###."],
        '6' => [".###.", "#....", "#....", "####.", "#...#", "#...#", ".###."],
        '7' => ["#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#..."],
        '8' => [".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###."],
        _ => [".###.", "#...#", "#...#", ".####", "....#", "....#", ".###."],
    }
}

/// A cell with `text` drawn bright on dark, 4 pixels per dot, one empty dot between the digits.
fn drawn(text: &str) -> Gray {
    let scale = 4;
    let width = (text.len() * 6 + 2) * scale;
    let height = 9 * scale;
    let mut gray = vec![32u8; width * height];
    for (i, digit) in text.chars().enumerate() {
        for (row, line) in shape(digit).iter().enumerate() {
            for (col, dot) in line.chars().enumerate() {
                if dot != '#' {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        let x = (1 + i * 6 + col) * scale + dx;
                        let y = (1 + row) * scale + dy;
                        gray[y * width + x] = 240;
                    }
                }
            }
        }
    }
    Gray { width, height, gray }
}

fn cell(material: &str, read: &str, text: &str) -> Cell {
    Cell { material: material.into(), column: 0, read: read.into(), crop: drawn(text) }
}

#[test]
fn zero_and_eight_are_learned_from_typed_values_and_told_apart() {
    let mut learned = Digits::default();
    let cells = [cell("Gold", "888", "808"), cell("Iron", "888", "800")];
    let typed = [("Gold".to_string(), Some(808.0), Some(1.0)), ("Iron".to_string(), Some(800.0), Some(1.0))];
    assert_eq!(learn_into(&mut learned, &cells, &typed), 6);
    assert_eq!(learned.get("0").len(), 3);
    assert_eq!(learned.get("8").len(), 3);
    let read: String =
        glyphs(Some(&drawn("0880")), Some(4)).iter().map(|v| classify(v, &learned).unwrap_or('?')).collect();
    assert_eq!(read, "0880");
    // Without patterns of both digits nothing is assigned.
    let mut only_eights = Digits::default();
    learn_into(&mut only_eights, &[cell("Gold", "888", "888")], &[("Gold".to_string(), Some(888.0), None)]);
    assert_eq!(classify(&glyphs(Some(&drawn("0")), Some(1))[0], &only_eights), None);
}

#[test]
fn a_whole_cell_is_read_from_learned_digits_and_an_unknown_digit_is_not_guessed() {
    let mut learned = Digits::default();
    for text in ["1234", "5678", "9012", "3456", "7890"] {
        let typed = [("Gold".to_string(), Some(text.parse::<f64>().unwrap_or(0.0)), Some(1.0))];
        learn_into(&mut learned, &[cell("Gold", text, text)], &typed);
    }
    assert_eq!(read_cell(&drawn("3051"), &learned, ""), Some(3051));
    let without_three = Digits::from_value(&{
        let mut value = learned.to_value();
        if let jsonio::Value::Object(top) = &mut value
            && let Some((_, jsonio::Value::Object(digits))) = top.iter_mut().find(|(k, _)| k == "ziffern")
        {
            digits.retain(|(k, _)| k != "3");
        }
        value
    });
    assert_eq!(read_cell(&drawn("3051"), &without_three, ""), None);
    let ones = Digits::from_value(
        &jsonio::loads(&format!(
            r#"{{"ziffern": {{"1": {}}}}}"#,
            jsonio::dumps(learned.to_value().get("ziffern").and_then(|z| z.get("1")).expect("ones"), None, false)
        ))
        .expect("json"),
    );
    assert_eq!(read_cell(&drawn("3"), &ones, ""), None);
    let cells = [cell("Gold", "888", "808"), cell("Iron", "888", "800")];
    assert_eq!(learn_into(&mut learned.clone(), &cells, &[("Gold".to_string(), Some(12.0), Some(1.0))]), 0);
    assert_eq!(
        learn_into(
            &mut learned.clone(),
            &cells,
            &[("Gold".to_string(), Some(123.0), Some(1.0)), ("Gold".to_string(), Some(456.0), Some(1.0))]
        ),
        0
    );
}

#[test]
fn at_most_forty_patterns_stay_per_digit() {
    let mut learned = Digits::default();
    for _ in 0..45 {
        learned.add("7", vec![1.0]);
    }
    assert_eq!(learned.get("7").len(), digits::DIGIT_KEEP);
}

/// A picture of the tests: one grey value per pixel, enlarged by taking the nearest pixel.
struct GrayPicture {
    width: i64,
    height: i64,
    gray: Vec<u8>,
}

impl Picture for GrayPicture {
    fn size(&self) -> (i64, i64) {
        (self.width, self.height)
    }

    fn scaled(&self, area: Area, scale: f64, _mode: Mode) -> Option<(usize, usize, Vec<u8>)> {
        let tw = ((area.2 as f64 * scale) as usize).max(1);
        let th = ((area.3 as f64 * scale) as usize).max(1);
        let mut out = Vec::with_capacity(tw * th * 4);
        for y in 0..th {
            for x in 0..tw {
                let sx = area.0 as usize + (x * area.2 as usize) / tw;
                let sy = area.1 as usize + (y * area.3 as usize) / th;
                let g = self.gray[sy * self.width as usize + sx];
                out.extend_from_slice(&[g, g, g, 255]);
            }
        }
        Some((tw, th, out))
    }
}

#[test]
fn a_confusable_value_is_confirmed_at_the_learned_digits_of_the_picture() {
    let index = Index::new(&["Gold".to_string()], |_| false);
    // The picture: the quality cell of the row Gold shows 508, the recognition reads 588 in every reading.
    let digits_cell = drawn("508");
    let (width, height) = (400usize, 60usize);
    let mut gray = vec![32u8; width * height];
    for y in 0..digits_cell.height {
        for x in 0..digits_cell.width {
            gray[(10 + y) * width + 100 + x] = digits_cell.gray[y * digits_cell.width + x];
        }
    }
    let picture = GrayPicture { width: width as i64, height: height as i64, gray };
    let words = vec![
        sized("Gold", 0.0, 10.0, 40.0, digits_cell.height as f64),
        sized("588", 100.0, 10.0, digits_cell.width as f64, digits_cell.height as f64),
        sized("122", 250.0, 10.0, 30.0, digits_cell.height as f64),
    ];
    let mut learned = Digits::default();
    let typed = [("Gold".to_string(), Some(808.0), None), ("Iron".to_string(), Some(800.0), None)];
    learn_into(&mut learned, &[cell("Gold", "888", "808"), cell("Iron", "888", "800")], &typed);
    for text in ["1234", "5678", "9012"] {
        learn_into(&mut learned, &[cell("Gold", text, text)], &[("Gold".to_string(), text.parse().ok(), None)]);
    }
    let passes = vec![words.clone(), words];
    let checks = Checks {
        confirm: Some(confirmer(&picture, &passes, &learned, &index)),
        reread: Some(rereader(&picture, &passes, &learned, &index)),
    };
    let merged = merge_passes(&passes, &index, None, &checks, false);
    assert_eq!(merged.found, found(&[("Gold", 508, 122)]));
    // Counter-check: without learned digits the value stays unsure.
    let none = Digits::default();
    let checks = Checks { confirm: Some(confirmer(&picture, &passes, &none, &index)), reread: None };
    assert!(merge_passes(&passes, &index, None, &checks, false).found.is_empty());
}
