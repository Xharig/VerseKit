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

//! The cards of a page stand one below the other and are all the same width: the corner brackets at the bottom
//! right then stand on one line. A card used to be as wide as its content unless the content spread over the
//! whole width itself (a row with a control on the right does), so a page showed narrow and wide cards in one
//! column (Settings › Erkennung).
//!
//! The check draws every page of the window in a few sizes, on the sample, on real data in an empty folder, on
//! real data with files and on real data with two sticks, never on the game of the computer it runs on, reads the places of the cards the last frame drew ([`crate::widgets::drawn_cards`]) and
//! compares the cards that start at the same left edge.

use egui::Rect;
use versekit_core::theme;

use super::docking::tests::draw_cards;
use super::real_data::{folder, host_apart, host_in};
use super::{NAV, SamplePage};

/// The windows the pages are drawn in: wide, the smallest window of the program (880 wide) and one narrower than
/// the program allows (a harder case).
const SIZES: [(f32, f32); 4] = [(1280.0, 900.0), (900.0, 700.0), (880.0, 700.0), (700.0, 700.0)];
/// Two widths are the same when they differ by less than this (points).
const SAME: f32 = 0.6;

/// The pages of the window: the overview, every page of the navigation and the ones only a jump leads to.
fn pages() -> Vec<&'static str> {
    let mut pages = vec![""];
    pages.extend(NAV.iter().flat_map(|(_, pages)| pages.iter().map(|(page, _)| *page)));
    pages.extend(["hf_wasistneu", "hf_diagnose"]);
    // `CARD_WIDTH_PAGE=<page>` looks at one page only, for the search of a finding.
    match std::env::var("CARD_WIDTH_PAGE") {
        Ok(only) => pages.into_iter().filter(|page| *page == only).collect(),
        Err(_) => pages,
    }
}

/// The widest gap between two cards that stand side by side in a row.
const ROW_GAP: f32 = 40.0;

/// The cards that are neither the full width of the page nor part of a row of cards side by side: `(left edge,
/// width, full width)` of each. A card is fine when it is as wide as the widest card of the page (the full width),
/// or when another card stands beside it, to its right or left, in the same stretch of the page (the areas of the
/// Basetool stand in two columns below a card of the full width). Whatever is neither leaves a corner bracket
/// at the bottom right that stands elsewhere than those of the cards above and below.
pub(super) fn uneven(cards: &[Rect]) -> Vec<(f32, f32, f32)> {
    uneven_rects(cards).into_iter().map(|(card, full)| (card.min.x, card.width(), full)).collect()
}

/// [`uneven`] with the whole place of each card.
pub(super) fn uneven_rects(cards: &[Rect]) -> Vec<(Rect, f32)> {
    // A card inside another card is narrower than it by the margin of the outer one, whatever it does: it is
    // measured among the cards of the same outer card (the narrowest one that holds it), not among the cards of
    // the page.
    let inside =
        |outer: &Rect, card: &Rect| outer.width() - card.width() >= SAME && outer.expand(SAME).contains_rect(*card);
    let holder = |card: &Rect| {
        cards
            .iter()
            .enumerate()
            .filter(|(_, outer)| inside(outer, card))
            .min_by(|a, b| a.1.width().total_cmp(&b.1.width()))
            .map(|(n, _)| n)
    };
    let holders: Vec<Option<usize>> = cards.iter().map(holder).collect();
    let mut found = Vec::new();
    for group in std::iter::once(None).chain((0..cards.len()).map(Some)) {
        let same_holder: Vec<Rect> =
            cards.iter().zip(&holders).filter(|(_, h)| **h == group).map(|(card, _)| *card).collect();
        found.extend(uneven_among(&same_holder));
    }
    found
}

/// [`uneven_rects`] for cards that stand in the same place: the page or one card.
fn uneven_among(cards: &[Rect]) -> Vec<(Rect, f32)> {
    let full = cards.iter().map(Rect::width).fold(0.0, f32::max);
    cards
        .iter()
        .filter(|card| (full - card.width()).abs() >= SAME)
        .filter(|card| {
            !cards.iter().any(|other| {
                let beside = other.min.x >= card.max.x - SAME && other.min.x - card.max.x <= ROW_GAP
                    || card.min.x >= other.max.x - SAME && card.min.x - other.max.x <= ROW_GAP;
                beside && other.min.y < card.max.y + ROW_GAP && other.max.y > card.min.y - ROW_GAP
            })
        })
        .map(|card| (*card, full))
        .collect()
}

/// Draws `key` in every size and notes the pages whose cards differ in width.
fn look(case: &str, page: &mut SamplePage, key: &str, findings: &mut Vec<String>) {
    page.go_to(key);
    let name = if key.is_empty() { "overview" } else { key };
    for (width, height) in SIZES {
        let cards = draw_cards(page, width, height);
        let odd = uneven_rects(&cards);
        for (card, full) in &odd {
            findings.push(format!(
                "{case}: {name} at {width}: the card at x={:.0}, y={:.0} is {:.0} wide, the page {full:.0}",
                card.min.x,
                card.min.y,
                card.width()
            ));
        }
        if !odd.is_empty() {
            let all: Vec<String> =
                cards.iter().map(|c| format!("(x={:.0} y={:.0} w={:.0})", c.min.x, c.min.y, c.width())).collect();
            findings.push(format!("    all cards of {name} at {width}: {}", all.join(" ")));
        }
    }
}

/// Files for a window on real data that has something to show: stock, trade stock, a hangar.
const FILES: [(&str, &str); 3] = [
    (
        versekit_core::stock::RAW_FILE,
        r#"{"format": 1, "posten": [{"material": "Iron", "menge": 4.5, "qualitaet": 420, "ort": "Orison", "gestohlen": true}]}"#,
    ),
    (
        versekit_core::stock::TRADE_FILE,
        r#"{"format": 1, "posten": [{"ware": "Laranite", "menge": 32.0, "ort": "Seraphim Station", "gestohlen": false}]}"#,
    ),
    (
        versekit_core::hangar::FILE,
        r#"{"format": 1, "schiffe": [{"name": "Carrack", "hersteller": "Anvil Aerospace", "herkunft": "pledge", "belegung": {}}]}"#,
    ),
];

/// The program on real data in `dir`, with a game folder of its own there: it finds no installation on the drives of
/// this computer.
fn apart(dir: &std::path::Path) -> versekit_core::paths::Paths {
    versekit_core::paths::Paths::new(host_apart(dir))
}

/// A game folder with two sticks in the log and in the action maps, and the setting that points the program at it.
fn game_with_sticks(dir: &std::path::Path) -> String {
    let (left, right) = ("1234ABCD-0000-0000-0000-504944564944", "3344ABCD-0000-0000-0000-504944564944");
    let game = dir.join("LIVE");
    let profile = game.join("USER").join("Client").join("0").join("Profiles").join("default");
    std::fs::create_dir_all(&profile).expect("the profile folder");
    let line = |slot: u8, name: &str, id: &str| format!("Connected joystick{slot}:  {name}  {{{id}}}\n");
    std::fs::write(game.join("Game.log"), line(0, "Left Stick", left) + &line(1, "Right Stick", right))
        .expect("the log");
    std::fs::write(
        profile.join("actionmaps.xml"),
        format!(
            "<ActionMaps><ActionProfiles><deviceoptions name=\"Left Stick  {{{left}}}\"><option input=\"x\" deadzone=\"0.15\"/><option input=\"slider1\" saturation=\"0.9\"/></deviceoptions><deviceoptions name=\"Right Stick  {{{right}}}\"><option input=\"x\" deadzone=\"0.2\"/></deviceoptions><options type=\"joystick\" instance=\"1\" Product=\"Left Stick  {{{left}}}\"><flight_move_pitch exponent=\"1.5\"/></options><options type=\"joystick\" instance=\"2\" Product=\"Right Stick  {{{right}}}\"/><actionmap name=\"spaceship_general\"><action name=\"v_pitch\"><rebind input=\"js1_x\"/></action><action name=\"v_roll\"><rebind input=\"js1_x\"/></action></actionmap></ActionProfiles></ActionMaps>"
        ),
    )
    .expect("the action maps");
    game.to_string_lossy().into_owned()
}

/// All pages, in the four ways the window is shown, in German and English. None of them sees the game of this
/// computer: the program would find an installation on the drives and show its sticks, which differ from one
/// computer to the next and from one day to the next.
fn all_findings() -> Vec<String> {
    use versekit_core::language::{Choice, set_thread_language};
    let mut findings = Vec::new();
    theme::set_current("original");
    let dir = folder("kartenbreite");
    let files = folder("kartenbreite-dateien");
    let sticks = folder("kartenbreite-sticks");
    for (name, content) in FILES {
        std::fs::write(files.join(name), content).expect("a file of the copy");
    }
    let game = game_with_sticks(&sticks);
    for (choice, case) in [(Choice::De, "de"), (Choice::En, "en")] {
        set_thread_language(choice);
        let mut sample = SamplePage::preview();
        for key in pages() {
            look(&format!("{case} sample"), &mut sample, key, &mut findings);
        }
        let mut real = SamplePage::with_settings(apart(&dir));
        for key in pages() {
            look(&format!("{case} real data"), &mut real, key, &mut findings);
        }
        let mut full = SamplePage::with_settings(apart(&files));
        for key in pages() {
            look(&format!("{case} real data with files"), &mut full, key, &mut findings);
        }
        let vars = [(versekit_core::paths::ENV_INSTALL_DIR, game.as_str())];
        let mut with_sticks = SamplePage::with_settings(versekit_core::paths::Paths::new(host_in(&sticks, &vars)));
        for key in pages() {
            look(&format!("{case} real data with sticks"), &mut with_sticks, key, &mut findings);
        }
    }
    set_thread_language(Choice::De);
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(files);
    let _ = std::fs::remove_dir_all(sticks);
    findings
}

/// The cards of one page are the same width wherever they stand one below the other.
#[test]
fn the_cards_of_a_page_are_all_the_same_width() {
    let _language = crate::language_lock();
    let findings = all_findings();
    assert!(findings.is_empty(), "{} page(s) with cards of different widths:\n{}", findings.len(), findings.join("\n"));
}

/// The pages of the check see the game folder they are given and the installation of no computer: what a page
/// shows of the sticks and their axes must not depend on who runs the test.
#[test]
fn the_pages_of_the_check_do_not_see_the_game_of_this_computer() {
    let dir = folder("kartenbreite-spiel");
    let found = apart(&dir).game_folder();
    let found = found.expect("the folder of the check holds a game folder of its own");
    let inside = std::path::Path::new(&found).starts_with(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(inside, "the game folder of the check is {found}, outside its own folder");
}

#[test]
fn the_check_tells_a_narrow_card_among_wide_ones_from_cards_in_columns() {
    let at = |x: f32, y: f32, w: f32| Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, 40.0));
    // All the full width, one below the other.
    assert!(uneven(&[at(10.0, 0.0, 500.0), at(10.0, 50.0, 500.0), at(10.3, 100.0, 500.2)]).is_empty());
    // One of them narrower, with nothing beside it: found, and only that one.
    let found = uneven(&[at(10.0, 0.0, 500.0), at(10.0, 50.0, 300.0), at(10.0, 100.0, 500.0)]);
    assert_eq!(found, [(10.0, 300.0, 500.0)]);
    // Just a little narrower is found too (the corner marks would not line up).
    assert_eq!(uneven(&[at(10.0, 0.0, 500.0), at(10.0, 50.0, 490.0)]).len(), 1);
    // A card of the full width above two columns: the columns have each other beside them.
    let columns = [at(10.0, 0.0, 500.0), at(10.0, 50.0, 245.0), at(265.0, 50.0, 245.0), at(10.0, 100.0, 245.0)];
    assert!(uneven(&columns).is_empty());
    // A card that stands alone in the left half with the right half empty is not a column.
    assert_eq!(uneven(&[at(10.0, 0.0, 500.0), at(10.0, 50.0, 245.0)]).len(), 1);
    // Beside, but far away from the card (another stretch of the page): no neighbour.
    assert_eq!(uneven(&[at(10.0, 0.0, 500.0), at(10.0, 50.0, 200.0), at(300.0, 900.0, 200.0)]).len(), 2);
}

#[test]
fn cards_inside_a_card_are_measured_among_themselves() {
    let at = |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h));
    // A card of the page with two cards one below the other inside it, 26 narrower: nothing uneven.
    let nested = [
        at(10.0, 0.0, 500.0, 40.0),
        at(10.0, 50.0, 500.0, 300.0),
        at(23.0, 80.0, 474.0, 100.0),
        at(23.0, 200.0, 474.0, 100.0),
    ];
    assert!(uneven(&nested).is_empty());
    // Inside, one of them narrower than the other: found, and it is the inner one that is named.
    let odd = [nested[0], nested[1], nested[2], at(23.0, 200.0, 400.0, 100.0)];
    assert_eq!(uneven(&odd), [(23.0, 400.0, 474.0)]);
    // On the page a narrow card is still found next to nested ones.
    let page = [nested[0], nested[1], nested[2], nested[3], at(10.0, 400.0, 300.0, 40.0)];
    assert_eq!(uneven(&page), [(10.0, 300.0, 500.0)]);
}
