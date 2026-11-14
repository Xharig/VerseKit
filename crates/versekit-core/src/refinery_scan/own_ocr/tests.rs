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

/// A dark picture with bright blocks: (x, y, width, height).
fn picture(width: i64, height: i64, blocks: &[(i64, i64, i64, i64)]) -> Bitmap {
    let mut raw = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let lit = blocks.iter().any(|&(bx, by, bw, bh)| x >= bx && x < bx + bw && y >= by && y < by + bh);
            let v = if lit { 220 } else { 25 };
            let at = ((y * width + x) * 4) as usize;
            raw[at..at + 4].copy_from_slice(&[v, v, v, 255]);
        }
    }
    Bitmap { width, height, raw }
}

fn job(bitmap: &Bitmap) -> Job {
    Job { area: (0, 0, bitmap.width, bitmap.height), scale: 1.0, mode: Mode::Plain }
}

/// Letters as blocks 6 wide and 10 high, 3 apart; a word gap of 9.
fn letters_at(x: i64, y: i64, count: usize) -> Vec<(i64, i64, i64, i64)> {
    (0..count).map(|i| (x + i as i64 * 9, y, 6, 10)).collect()
}

#[test]
fn letters_close_together_make_a_word_and_a_wide_gap_a_new_one() {
    let mut blocks = letters_at(10, 10, 3);
    blocks.extend(letters_at(10 + 2 * 9 + 6 + 9, 10, 2));
    blocks.extend(letters_at(10, 40, 4));
    let cut = segment(&picture(120, 60, &blocks), &job(&picture(120, 60, &blocks))).expect("cut");
    let mut counts: Vec<(i64, usize)> =
        cut.words.iter().map(|w| ((w.frame.1 as i64) * 1000 + w.frame.0 as i64, w.shapes.len())).collect();
    counts.sort_unstable();
    assert_eq!(counts.iter().map(|c| c.1).collect::<Vec<_>>(), vec![3, 2, 4], "{counts:?}");
    let first = &cut.words.iter().find(|w| w.shapes.len() == 3).expect("word").frame;
    assert_eq!(*first, (10.0, 10.0, 24.0, 10.0));
}

#[test]
fn a_dot_above_a_letter_belongs_to_it_and_a_line_below_does_not() {
    // An i: a dot of 2 high above a stem of 10, gap 2; below it at a line's distance another letter.
    let blocks = [(10, 10, 4, 2), (10, 14, 4, 10), (10, 30, 4, 10), (17, 30, 6, 10)];
    let bitmap = picture(60, 50, &blocks);
    let cut = segment(&bitmap, &job(&bitmap)).expect("cut");
    let mut heights: Vec<f32> = cut.words.iter().map(|w| w.frame.3).collect();
    heights.sort_by(f32::total_cmp);
    assert_eq!(heights, vec![10.0, 14.0], "the i whole, the line below apart");
}

#[test]
fn dust_and_the_thin_lines_of_panels_are_no_writing() {
    let blocks = [(5, 5, 1, 1), (10, 20, 100, 1), (10, 30, 6, 10), (19, 30, 6, 10)];
    let bitmap = picture(130, 50, &blocks);
    let cut = segment(&bitmap, &job(&bitmap)).expect("cut");
    assert_eq!(cut.words.len(), 1, "{:?}", cut.words.iter().map(|w| w.frame).collect::<Vec<_>>());
    assert_eq!(cut.words[0].shapes.len(), 2);
}

#[test]
fn two_touching_letters_are_cut_where_the_least_ink_is() {
    // Two blocks joined by a thin bridge at the bottom: one piece, far wider than a letter.
    let blocks = [(10, 10, 7, 12), (17, 20, 3, 2), (20, 10, 7, 12)];
    let bitmap = picture(50, 40, &blocks);
    let cut = segment(&bitmap, &job(&bitmap)).expect("cut");
    assert_eq!(cut.words.len(), 1);
    assert_eq!(cut.words[0].shapes.len(), 2);
}

#[test]
fn a_word_stands_in_pixels_of_the_picture_whatever_the_enlargement() {
    let blocks = letters_at(20, 10, 2);
    let bitmap = picture(60, 30, &blocks);
    let cut = segment(&bitmap, &Job { area: (10, 5, 40, 20), scale: 3.0, mode: Mode::Plain }).expect("cut");
    assert_eq!(cut.size, (120, 60));
    let word = cut.word("ab", &cut.words[0]);
    assert!((word.x - 20.0).abs() <= 1.0 && (word.y - 10.0).abs() <= 1.0, "{word:?}");
    assert!((word.w - 15.0).abs() <= 1.5 && (word.h - 10.0).abs() <= 1.5, "{word:?}");
}

fn shape(fill: u8, aspect: f32) -> Shape {
    Shape { pixels: vec![fill; FW * FH], aspect, top: 0.0, bottom: 0.0 }
}

#[test]
fn a_font_goes_through_its_bytes_unchanged() {
    let font =
        Font { glyphs: vec![('A', shape(10, 0.6)), ('ä', Shape { top: -0.25, bottom: 0.125, ..shape(200, 0.5) })] };
    assert_eq!(Font::from_bytes(&font.to_bytes()), Some(font));
    assert_eq!(Font::from_bytes(b"nichts"), None);
    assert_eq!(Font::from_bytes(&[]), None);
}

#[test]
fn a_letter_is_named_by_the_nearest_picture_and_an_unsure_digit_is_a_question_mark() {
    let font = Font { glyphs: vec![('0', shape(100, 0.6)), ('8', shape(140, 0.6)), ('I', shape(250, 0.2))] };
    let piece = |fills: &[(u8, f32)]| Piece {
        frame: (0.0, 0.0, 1.0, 1.0),
        shapes: fills.iter().map(|&(f, a)| shape(f, a)).collect(),
    };
    assert_eq!(font.read(&piece(&[(101, 0.6), (139, 0.6), (240, 0.2)])), "08I");
    assert_eq!(font.read(&piece(&[(120, 0.6)])), "?", "halfway between 0 and 8");
    assert_eq!(font.read(&piece(&[(101, 0.6), (120, 3.0)])), "0", "nothing near that width: no letter");
    // Far from every picture: left out beside letters, a question mark where a digit is nearest.
    let far = Font { glyphs: vec![('0', shape(0, 0.6)), ('I', shape(0, 0.2))] };
    assert_eq!(far.read(&piece(&[(255, 0.6), (0, 0.2)])), "?I");
    assert_eq!(far.read(&piece(&[(255, 0.25), (0, 0.6)])), "0", "a symbol in front of a word goes");
}

#[test]
fn a_flat_letter_is_a_dash_in_the_middle_and_a_point_at_the_base() {
    let font = Font { glyphs: vec![('4', shape(100, 0.6))] };
    let flat = |top: f32, bottom: f32| Shape { top, bottom, ..shape(100, 0.5) };
    let piece =
        Piece { frame: (0.0, 0.0, 1.0, 1.0), shapes: vec![flat(0.4, -0.45), flat(0.4, -0.45), flat(0.85, 0.0)] };
    assert_eq!(font.read(&piece), "--.");
}

/// The terminal writes `--` for a yield not known yet: on a line with numbers it is read as dashes, not as 44.
#[test]
fn two_dashes_beside_a_number_are_dashes() {
    // Digits 6 wide and 10 high; the dashes 5 wide and 2 high in the middle of the line, far to the right.
    let mut blocks = letters_at(10, 10, 3);
    blocks.extend([(70, 14, 5, 2), (78, 14, 5, 2)]);
    let bitmap = picture(100, 30, &blocks);
    let cut = segment(&bitmap, &job(&bitmap)).expect("cut");
    let font = Font { glyphs: vec![('4', shape(100, 0.6))] };
    let dashes = cut.words.iter().find(|w| w.frame.0 >= 60.0).expect("the dashes");
    assert_eq!(font.read(dashes), "--");
}

/// The shipped font holds the digits, the capitals of the table and the small letters of the time left.
#[test]
fn the_shipped_font_knows_the_characters_of_the_table() {
    let font = Font::shipped();
    for c in "0123456789ABCDEGILMNORSTUY()hm".chars() {
        assert!(font.glyphs.iter().any(|(g, _)| *g == c), "{c} missing");
    }
    assert!(font.glyphs.iter().all(|(g, _)| !g.is_lowercase() || "hmd".contains(*g)));
}

#[test]
fn building_a_font_keeps_different_pictures_and_drops_doubles() {
    let samples: Vec<(char, Shape)> =
        [10u8, 11, 10, 200, 201, 120].iter().map(|&f| ('5', shape(f, 0.6))).chain([('7', shape(50, 0.6))]).collect();
    let font = Font::build(&samples, 10);
    let fives: Vec<u8> = font.glyphs.iter().filter(|(c, _)| *c == '5').map(|(_, s)| s.pixels[0]).collect();
    assert_eq!(fives.len(), 3, "{fives:?}");
    assert_eq!(Font::build(&samples, 2).glyphs.iter().filter(|(c, _)| *c == '5').count(), 2);
    assert!(font.glyphs.iter().any(|(c, _)| *c == '7'));
}
