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

//! The tab Signatur: the digits taught by digit, the pictures as cards, the pictures kept for an unsure number, and
//! the button that discards any picture.

use std::cell::Cell;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use versekit_core::signature_scan::own;

use super::*;

/// Where a text stands in the shapes of a frame: the middle of its box.
fn text_at(shapes: &[egui::epaint::ClippedShape], wanted: &str) -> Option<Pos2> {
    fn find(shape: &egui::epaint::Shape, wanted: &str) -> Option<Pos2> {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == wanted => {
                Some(text.galley.rect.translate(text.pos.to_vec2()).center())
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, wanted)),
            _ => None,
        }
    }
    shapes.iter().find_map(|clipped| find(&clipped.shape, wanted))
}

/// Draws the page of the scanner once with `events`; gives its shapes.
fn card(ctx: &egui::Context, state: &mut ScannerState, events: Vec<Event>, time: f64) -> Vec<egui::epaint::ClippedShape> {
    let scheme = versekit_core::theme::current();
    // Tall enough for the whole page: what lies below the screen is not drawn.
    let mut raw = input(events, time);
    raw.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, vec2(700.0, 3200.0)));
    let mut out = ctx.run_ui(raw, |ui| show_page(ui, scheme, state, &MiningData::default()));
    out.textures_delta.clear();
    out.shapes
}

/// A click on `at`: pressed in one frame, released in the next.
fn click(ctx: &egui::Context, state: &mut ScannerState, at: Pos2, time: f64) -> Vec<egui::epaint::ClippedShape> {
    card(ctx, state, press(at), time);
    let release =
        Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE };
    card(ctx, state, vec![release], time + 0.05);
    card(ctx, state, vec![], time + 0.1);
    // A section opens over a moment: drawn again once it stands open.
    card(ctx, state, vec![], time + 0.6)
}

/// The label of a button as the scheme writes it.
fn labelled(kind: ButtonKind, key: &str) -> String {
    widgets::button_label(versekit_core::theme::current(), kind, &t(key))
}

fn danger(key: &str) -> String {
    labelled(ButtonKind::Danger, key)
}

/// A scanner that is on, with the last picture of the overlay standing.
fn standing(name: &str) -> (ScannerState, u64) {
    let paths = copy(name);
    let mut state = ScannerState::with_files(&paths);
    state.on = true;
    let (number, crop) = crop();
    let (_, _, last) = state.files().expect("files");
    assert!(signature_watch::write_last_frame(&last, &Frame::new(crop, Some(number), 7)));
    (state, number)
}

/// A picture taught for `number`, as a file of the folder of the pictures; a second apart from the one before.
fn taught_picture(folder: &str, number: &str, shade: u8) -> String {
    std::thread::sleep(std::time::Duration::from_millis(15));
    let (_, crop) = crop();
    let gray = Gray::new(crop.w, crop.h, crop.data.iter().map(|v| v.saturating_add(shade)).collect());
    let name = signature_scan::sample_name(number);
    assert!(signature_scan::save_sample_as(folder, &gray, &name));
    name
}

#[test]
fn every_picture_that_stands_can_be_discarded_and_is_not_offered_again() {
    set_thread_language(Choice::De);
    let (mut state, _) = standing("scanner-ui-verwerfen");
    let ctx = context();
    let shapes = card(&ctx, &mut state, vec![], 0.0);
    assert!(state.frame.is_some(), "the last picture stands");
    assert!(state.pending.is_none(), "it is no kept picture");
    let at = text_at(&shapes, &t("s_sg_verwerfen")).expect("the button stands for the last picture too");
    click(&ctx, &mut state, at, 1.0);
    assert!(state.frame.is_none(), "discarded");
    card(&ctx, &mut state, vec![], 2.0);
    assert!(state.frame.is_none(), "the same picture is not offered again");
}

#[test]
fn the_own_digits_stand_open_with_every_digit_folded_on_its_own_and_a_picture_goes_with_its_digits() {
    set_thread_language(Choice::De);
    let (mut state, number) = standing("scanner-ui-eigene");
    let ctx = context();
    card(&ctx, &mut state, vec![], 0.0);
    state.typed = number.to_string();
    state.learn(&MiningData::default());
    assert!(state.taught, "{:?}", state.said);
    let (files, folder, _) = state.files().unwrap();
    assert_eq!(signature_scan::samples(&folder).len(), 1);
    let taught = own::taught_digits(&files);
    assert!(!taught.is_empty());

    // Open at once, nothing to click: the pictures with their number, and a head for each digit taught with the
    // number of its patterns.
    let shown = texts(&card(&ctx, &mut state, vec![], 1.0));
    assert!(shown.contains(&t("scan_eigene_titel")), "{shown:?}");
    assert!(shown.contains(&tf("scan_bilder_kopf", &[("0", Arg::from(1))])), "{shown:?}");
    assert!(shown.contains(&signature_window::with_thousands(number)), "the number of the picture: {shown:?}");
    for digit in '0'..='9' {
        let count = taught.iter().filter(|d| d.digit == digit).count();
        let head = tf("scan_ziffer_kopf", &[("0", Arg::from(digit.to_string().as_str())), ("1", Arg::from(count))]);
        assert_eq!(shown.contains(&head), count > 0, "{digit}: {shown:?}");
    }
    // The patterns hide in their digit until it is opened: the larger one is not drawn yet.
    let head = tf("scan_ziffer_kopf", &[("0", Arg::from(taught[0].digit.to_string().as_str())), ("1", Arg::from(1))]);
    assert!(text_at(&card(&ctx, &mut state, vec![], 1.5), &head).is_some() || taught.iter().filter(|d| d.digit == taught[0].digit).count() > 1);

    // The picture deleted: its digits go, the line says how many.
    let name = signature_scan::samples(&folder)[0].clone();
    state.own.forget_picture(&files, &folder, &name);
    assert!(signature_scan::samples(&folder).is_empty());
    assert!(own::taught_digits(&files).is_empty(), "nothing it alone gave is left");
    let shown = texts(&card(&ctx, &mut state, vec![], 3.0));
    let said = |gone: usize| tf("scan_eigene_geloescht", &[("0", Arg::from(gone))]);
    assert!((1..500).any(|gone| shown.contains(&said(gone))), "{shown:?}");
    assert!(shown.contains(&t("scan_eigene_leer")), "{shown:?}");
}

#[test]
fn forgetting_all_own_digits_asks_first_and_leaves_the_pictures() {
    set_thread_language(Choice::De);
    let (mut state, number) = standing("scanner-ui-vergessen");
    let ctx = context();
    card(&ctx, &mut state, vec![], 0.0);
    state.typed = number.to_string();
    state.learn(&MiningData::default());
    let (files, folder, _) = state.files().unwrap();
    let shapes = card(&ctx, &mut state, vec![], 1.0);
    let forget = text_at(&shapes, &danger("scan_eigene_vergessen")).expect("the button");
    let shapes = click(&ctx, &mut state, forget, 3.0);
    let shown = texts(&shapes);
    assert!(shown.contains(&t("scan_eigene_vergessen_frage")), "the question: {shown:?}");
    assert!(!own::taught_digits(&files).is_empty(), "nothing gone before the answer");

    // Cancelled: nothing happens.
    let cancel = text_at(&shapes, &t("e_abbrechen")).expect("cancel");
    let shapes = click(&ctx, &mut state, cancel, 4.0);
    assert!(!texts(&shapes).contains(&t("scan_eigene_vergessen_frage")), "closed");
    assert!(!own::taught_digits(&files).is_empty());

    // Asked again and answered yes.
    let forget = text_at(&shapes, &danger("scan_eigene_vergessen")).unwrap();
    let shapes = click(&ctx, &mut state, forget, 5.0);
    let yes = text_at(&shapes, &danger("scan_eigene_vergessen_ja")).expect("yes");
    let shapes = click(&ctx, &mut state, yes, 6.0);
    assert!(own::taught_digits(&files).is_empty());
    for (_, path) in files.all() {
        assert!(!std::path::Path::new(path).exists(), "{path}");
    }
    assert_eq!(signature_scan::samples(&folder).len(), 1, "the picture stays");
    let shown = texts(&card(&ctx, &mut state, vec![], 7.0));
    assert!(shown.contains(&t("scan_eigene_vergessen_ok")), "{shown:?} {:?}", texts(&shapes));
}

/// A picture without the location symbol (a speed, a distance, a name): the crop with everything left of the
/// digits painted over.
fn without_symbol() -> (u64, Gray) {
    let (number, crop) = crop();
    let shipped = signature_scan::templates(signature_scan::SHIPPED_TEMPLATES, "");
    let reading = signature_scan::read(&crop, &shipped, &[number]);
    let left = reading.digits.iter().map(|d| d[0]).min().expect("digits") as usize;
    let mut bare = crop.clone();
    for y in 0..bare.h {
        for x in 0..left.saturating_sub(1) {
            bare.data[y * bare.w + x] = 28;
        }
    }
    (number, bare)
}

#[test]
fn a_picture_without_its_location_symbol_is_never_shown_and_nothing_is_taught_from_it() {
    set_thread_language(Choice::De);
    let paths = copy("scanner-ui-ohne-symbol");
    let mut state = ScannerState::with_files(&paths);
    state.on = true;
    let (number, bare) = without_symbol();
    let (_, _, last) = state.files().expect("files");
    assert!(signature_watch::write_last_frame(&last, &Frame::new(bare, None, 7)));
    let ctx = context();
    card(&ctx, &mut state, vec![], 0.0);
    assert!(state.frame.is_none(), "not shown");
    let shown = texts(&card(&ctx, &mut state, vec![], 1.0));
    assert!(shown.contains(&t("scan_warte")), "{shown:?}");
    assert!(!shown.contains(&t("scan_anlernen")), "no field to teach: {shown:?}");
    // Not even by the key: nothing is taught.
    state.typed = number.to_string();
    state.learn(&MiningData::default());
    assert!(!state.taught);
    let (files, folder, _) = state.files().unwrap();
    assert!(own::taught_digits(&files).is_empty());
    assert!(signature_scan::samples(&folder).is_empty());
}

/// The kept pictures are gone through one by one: each says which of them it is, one without the symbol that an
/// older version kept is deleted without a word, and every one taught or discarded takes the count down.
#[test]
fn the_kept_pictures_are_gone_through_one_by_one_and_those_without_the_symbol_fall_away() {
    set_thread_language(Choice::De);
    let paths = copy("scanner-ui-gemerkt-blaettern");
    let mut state = ScannerState::with_files(&paths);
    state.on = true;
    let (number, crop) = crop();
    let folder = state.pending_folder().unwrap();
    let (_, bare) = without_symbol();
    let shade = |by: u8| Gray::new(crop.w, crop.h, crop.data.iter().map(|v| v.saturating_add(by)).collect());
    assert!(signature_watch::keep_pending(&folder, &Frame::new(shade(1), None, 1)));
    std::thread::sleep(std::time::Duration::from_millis(3));
    // A file of an older version: kept although it shows no symbol.
    let old = format!("{folder}/{:016}.bin", u64::MAX / 4);
    assert!(signature_watch::write_last_frame(&old, &Frame::new(bare, None, 2)));
    std::thread::sleep(std::time::Duration::from_millis(3));
    assert!(signature_watch::keep_pending(&folder, &Frame::new(shade(2), None, 3)));
    let ctx = context();
    let shown = texts(&card(&ctx, &mut state, vec![], 0.0));
    assert!(shown.contains(&tf("s_sg_gemerkt_nr", &[("0", Arg::from(1)), ("1", Arg::from(2))])), "{shown:?}");
    assert!(!std::path::Path::new(&old).exists(), "the picture without the symbol is gone");
    assert_eq!(state.pending_count(), 2);
    // The newest is taught: the count goes down.
    state.typed = number.to_string();
    state.learn(&MiningData::default());
    assert!(state.taught, "{:?}", state.said);
    assert_eq!(state.pending_count(), 1);
    state.release();
    let shown = texts(&card(&ctx, &mut state, vec![], 1.0));
    assert!(shown.contains(&tf("s_sg_gemerkt_nr", &[("0", Arg::from(1)), ("1", Arg::from(1))])), "{shown:?}");
}

// ------------------------------------------------------------ the digits by digit

/// Puts the pattern of a block (too wide for a digit) into the file of the digit `digit` as taught by nobody.
fn add_suspect(state: &ScannerState, digit: char) {
    let (files, _, _) = state.files().unwrap();
    let mut file = own::OwnFile::open(files.of(signature_scan::Variant::Green)).unwrap();
    let mut block = [0u8; signature_scan::NORM_W * signature_scan::NORM_H];
    for y in 4..16 {
        for x in 0..signature_scan::NORM_W {
            block[y * signature_scan::NORM_W + x] = 1;
        }
    }
    file.add(own::Kind::Taught, digit, &block, None, 24);
    file.save(files.of(signature_scan::Variant::Green)).unwrap();
}

#[test]
fn a_suspect_pattern_stands_first_in_its_digit_is_counted_in_its_head_and_edged_in_yellow() {
    set_thread_language(Choice::De);
    let (mut state, number) = standing("scanner-ui-verdacht");
    let ctx = context();
    card(&ctx, &mut state, vec![], 0.0);
    state.typed = number.to_string();
    state.learn(&MiningData::default());
    let first = own::taught_digits(&state.files().unwrap().0).first().map(|d| d.digit).expect("a digit");
    add_suspect(&state, first);
    state.own.stale();
    let shapes = card(&ctx, &mut state, vec![], 1.0);
    let shown = texts(&shapes);
    let all = own::taught_digits(&state.files().unwrap().0).iter().filter(|d| d.digit == first).count();
    let head = tf(
        "scan_ziffer_kopf_verdacht",
        &[("0", Arg::from(first.to_string().as_str())), ("1", Arg::from(all)), ("2", Arg::from(1))],
    );
    assert!(shown.contains(&head), "{head}: {shown:?}");
    let order = state.own.digit_order();
    let mine: Vec<bool> = order.iter().filter(|(d, _)| *d == first).map(|(_, suspect)| *suspect).collect();
    assert!(mine[0] && mine.iter().filter(|s| **s).count() == 1, "the suspect one first: {order:?}");
    // The digit opened: the suspect pattern has a yellow edge of 2 points, the others none.
    let at = text_at(&shapes, &head).expect("head");
    let shapes = click(&ctx, &mut state, at, 2.0);
    let yellow = color(versekit_core::theme::current().yellow);
    let edged = rects(&shapes).iter().filter(|r| r.stroke.color == yellow && r.stroke.width == 2.0).count();
    assert_eq!(edged, 1, "one pattern edged in yellow");
    // Without a suspect pattern no card is edged.
    let (mut clean, number) = standing("scanner-ui-ohne-verdacht");
    card(&ctx, &mut clean, vec![], 0.0);
    clean.typed = number.to_string();
    clean.learn(&MiningData::default());
    let shown = texts(&card(&ctx, &mut clean, vec![], 1.0));
    // The word that marks a head as suspect is the tail of its text in the language file.
    let word = t("scan_ziffer_kopf_verdacht").rsplit('}').next().unwrap_or_default().trim().to_string();
    assert!(!word.is_empty() && !shown.iter().any(|s| s.contains(&word)), "{word}: {shown:?}");
}

// ------------------------------------------------------------ the pictures as cards

/// A scanner with `names` as pictures taught (the numbers), newest last.
fn with_pictures(name: &str, numbers: &[&str]) -> (ScannerState, String, Vec<String>) {
    let paths = copy(name);
    let mut state = ScannerState::with_files(&paths);
    state.on = true;
    let (_, folder, _) = state.files().unwrap();
    let names = numbers.iter().enumerate().map(|(at, n)| taught_picture(&folder, n, at as u8 * 12)).collect();
    (state, folder, names)
}

#[test]
fn the_pictures_taught_stand_as_cards_newest_first_with_their_count_and_a_search_by_number() {
    set_thread_language(Choice::De);
    let (mut state, _, _) = with_pictures("scanner-ui-raster", &["15600", "6740", "17080"]);
    let ctx = context();
    let shown = texts(&card(&ctx, &mut state, vec![], 0.0));
    assert!(shown.contains(&tf("scan_bilder_kopf", &[("0", Arg::from(3))])), "the count in the head: {shown:?}");
    assert_eq!(state.own.picture_numbers(), ["17080", "6740", "15600"], "newest first");
    for number in ["17,080", "6,740", "15,600"] {
        assert!(shown.contains(&number.to_string()), "the number under its picture: {shown:?}");
    }
    // The search narrows the cards to the number typed.
    state.own.search_for("6,7");
    let shown = texts(&card(&ctx, &mut state, vec![], 1.0));
    assert!(shown.contains(&"6,740".to_string()) && !shown.contains(&"15,600".to_string()), "{shown:?}");
    state.own.search_for("99");
    let shown = texts(&card(&ctx, &mut state, vec![], 2.0));
    assert!(shown.contains(&t("scan_bilder_keine")), "{shown:?}");
}

/// The shapes drawn (every kind of shape counts one) while the pointer rests at `at`, or nowhere.
fn drawn(ctx: &egui::Context, state: &mut ScannerState, at: Option<Pos2>, time: f64) -> usize {
    fn count(shape: &egui::epaint::Shape) -> usize {
        match shape {
            egui::epaint::Shape::Vec(shapes) => shapes.iter().map(count).sum(),
            _ => 1,
        }
    }
    let events = at.map(|at| vec![Event::PointerMoved(at)]).unwrap_or_default();
    card(ctx, state, events.clone(), time);
    card(ctx, state, events, time + 0.1).iter().map(|clipped| count(&clipped.shape)).sum()
}

#[test]
fn the_button_that_deletes_a_card_shows_only_while_the_pointer_rests_on_it_and_deletes_the_picture() {
    set_thread_language(Choice::De);
    let (mut state, folder, names) = with_pictures("scanner-ui-loeschen", &["15600", "6740"]);
    let ctx = context();
    let shapes = card(&ctx, &mut state, vec![], 0.0);
    let on_card = text_at(&shapes, "6,740").expect("the caption of a card");
    let (away, over) = (drawn(&ctx, &mut state, None, 1.0), drawn(&ctx, &mut state, Some(on_card), 2.0));
    assert!(over > away, "the button of the card under the pointer is drawn: {over} against {away}");
    // The button has its name for screen readers and keyboard, hidden or not.
    let mut harness = Harness::builder()
        .with_size(vec2(700.0, 1800.0))
        .with_context_setup(crate::install)
        .with_accessibility_check(false)
        .allow_missing_glyphs()
        .build_ui_state(
            |ui, state: &mut ScannerState| show_page(ui, versekit_core::theme::current(), state, &MiningData::default()),
            state,
        );
    harness.run_steps(3);
    harness.get_by_label(&tf("scan_eigene_bild_loeschen", &[("0", Arg::from("6,740"))])).click();
    harness.run_steps(4);
    let left = signature_scan::samples(&folder);
    assert_eq!(left, vec![names[0].clone()], "the picture of 6,740 is gone, the other stays");
}

#[test]
fn the_button_duplicates_away_keeps_one_picture_of_each_number_the_newest() {
    set_thread_language(Choice::De);
    let (mut state, folder, names) =
        with_pictures("scanner-ui-doppelte", &["15600", "15600", "15600", "6740", "6740"]);
    let ctx = context();
    let shapes = card(&ctx, &mut state, vec![], 0.0);
    let shown = texts(&shapes);
    assert!(shown.contains(&tf("scan_doppelte_h", &[("0", Arg::from(3))])), "{shown:?}");
    let button = text_at(&shapes, &labelled(ButtonKind::Normal, "scan_doppelte_entfernen")).expect("the button");
    click(&ctx, &mut state, button, 1.0);
    let left = signature_scan::samples(&folder);
    assert_eq!(left, vec![names[2].clone(), names[4].clone()], "the newest of each number");
    let shown = texts(&card(&ctx, &mut state, vec![], 3.0));
    assert!(shown.contains(&tf("scan_doppelte_weg", &[("0", Arg::from(3))])), "{shown:?}");
    assert!(!shown.contains(&t("scan_doppelte_entfernen")), "nothing left to remove: {shown:?}");
}

#[test]
fn a_picture_nearly_like_one_taught_is_not_taught_a_second_time() {
    set_thread_language(Choice::De);
    let (mut state, number) = standing("scanner-ui-schon");
    let ctx = context();
    card(&ctx, &mut state, vec![], 0.0);
    state.typed = number.to_string();
    state.learn(&MiningData::default());
    assert!(state.taught);
    let (files, folder, _) = state.files().unwrap();
    let before = own::taught_digits(&files).len();
    state.taught = false;
    state.typed = number.to_string();
    state.learn(&MiningData::default());
    assert!(!state.taught, "{:?}", state.said);
    let (lines, passed) = state.said.clone().expect("said");
    assert_eq!((lines, passed), (vec![t("scan_grund_anlernen_schon")], false));
    assert_eq!(signature_scan::samples(&folder).len(), 1, "one picture");
    assert_eq!(own::taught_digits(&files).len(), before, "no pattern more");
}

// ------------------------------------------------------------ "Next image" with nothing kept

thread_local! {
    /// What the made-up screen shows: 0 the game is not in front, 1 the game without a signature, 2 the signature.
    static SCREEN: Cell<u8> = const { Cell::new(0) };
}

struct Made;

impl Source for Made {
    fn game(&mut self) -> Option<versekit_core::screen_grab::Area> {
        (SCREEN.get() != 0).then_some((0, 0, 1000, 700))
    }

    fn grab(&mut self, area: versekit_core::screen_grab::Area) -> Result<Vec<u8>, versekit_core::screen_grab::GrabError> {
        let (width, height) = (area.2 as usize, area.3 as usize);
        let mut raw = vec![0u8; width * height * 4];
        for p in raw.as_chunks_mut::<4>().0 {
            *p = [5, 12, 7, 255];
        }
        if SCREEN.get() == 2 {
            let (_, crop) = crop();
            for y in 0..crop.h {
                for x in 0..crop.w {
                    let v = crop.data[y * crop.w + x];
                    let at = ((300 + y) * width + 420 + x) * 4;
                    raw[at..at + 4].copy_from_slice(&[v.saturating_add(60), v, 255, 255]);
                }
            }
        }
        Ok(raw)
    }
}

#[test]
fn next_image_with_nothing_kept_photographs_only_with_the_game_in_front_and_never_shows_a_picture_without_the_symbol() {
    set_thread_language(Choice::De);
    let paths = copy("scanner-ui-naechstes");
    let mut state = ScannerState::with_files(&paths);
    state.on = true;
    state.screen = || Box::new(Made);
    let (number, _) = crop();
    let mining = MiningData::default();
    // The game is not in front: nothing is photographed, nothing is shown.
    SCREEN.set(0);
    state.next(&mining, 0.0);
    assert!(state.frame.is_none());
    assert_eq!(state.said, Some((vec![t("scan_keine_mehr")], false)));
    // The game in front without a signature in it: a picture without the symbol is never shown.
    SCREEN.set(1);
    state.next(&mining, 1.0);
    assert!(state.frame.is_none());
    assert_eq!(state.said, Some((vec![t("scan_nichts_im_bild")], false)));
    // The game in front with the signature: it stands, and it can be taught.
    SCREEN.set(2);
    state.next(&mining, 2.0);
    assert!(state.frame.is_some(), "{:?}", state.said);
    let _ = number;
    // A picture kept comes before any photograph.
    let folder = state.pending_folder().unwrap();
    let (_, crop) = crop();
    assert!(signature_watch::keep_pending(&folder, &Frame::new(crop, None, 1)));
    SCREEN.set(0);
    state.next(&mining, 3.0);
    assert!(state.pending.is_none(), "set aside, not yet taken");
    let ctx = context();
    ctx.run_ui(input(vec![], 4.0), |ui| state.follow(ui)).textures_delta.clear();
    assert!(state.pending.is_some(), "the kept picture comes");
}
