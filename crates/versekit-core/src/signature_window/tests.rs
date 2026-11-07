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
use crate::jsonio;

const GAME: Area = (0, 0, 1920, 1080);

fn mining() -> MiningData {
    let data = jsonio::loads(
        r#"{"format": 1, "elemente": {
            "g1": {"name": "Quantainium", "scanSignature": 3170, "rarity": "legendary"},
            "g2": {"name": "Bexalite", "scanSignature": 1750, "rarity": "uncommon"},
            "g3": {"name": "Taranite", "scanSignature": 6340, "rarity": "uncommon"}}}"#,
    )
    .unwrap();
    MiningData::of(&data)
}

#[test]
fn the_number_is_written_with_the_thousands_separator_of_the_hud() {
    assert_eq!(with_thousands(0), "0");
    assert_eq!(with_thousands(999), "999");
    assert_eq!(with_thousands(1000), "1,000");
    assert_eq!(with_thousands(12680), "12,680");
    assert_eq!(with_thousands(1234567), "1,234,567");
}

#[test]
fn the_material_behind_a_signature_is_named_with_the_chunks_up_to_three() {
    let m = mining();
    // 6340 is two Quantainium and one Taranite.
    let text = describe(6340, &m);
    assert_eq!(text.number, "6,340");
    assert!(text.material.contains("2× Quantainium") && text.material.contains("1× Taranite"), "{text:?}");
    assert_eq!(text.material.split(" · ").count(), 2);
    assert!(text.known);
    assert_eq!(text.line(), format!("6,340 → {}", text.material));
    // As 4.12 wrote it: a number no material has is only the number.
    let none = describe_with(1700, &m, false);
    assert_eq!(none.material, "");
    assert_eq!(none.line(), "1,700");
}

#[test]
fn the_ground_the_hand_mining_and_the_salvage_have_a_scan_signature_of_their_own() {
    use crate::language::{Lang, with_language};
    let m = mining();
    // 10,000 is five times the signature of the salvage; 12,000 is six times that, three times the ROC (4,000)
    // and four times the hand mining (3,000).
    assert_eq!(materials(10_000, &m), ["salvage"]);
    assert_eq!(materials(12_000, &m), ["fps", "roc", "salvage"]);
    for (lang, five, twelve, four_thousand, seven) in [
        (Lang::De, "5× Bergung", "4× Handabbau · 3× ROC/Boden · 6× Bergung", "1× ROC/Boden · 2× Bergung", "7× Bergung"),
        (
            Lang::En,
            "5× Salvage",
            "4× Hand mining · 3× ROC/ground · 6× Salvage",
            "1× ROC/ground · 2× Salvage",
            "7× Salvage",
        ),
    ] {
        with_language(lang, || {
            let ten = describe(10_000, &m);
            assert_eq!((ten.material.as_str(), ten.known), (five, true), "{ten:?}");
            assert_eq!(ten.number, "10,000", "the number is shown all the same");
            assert_eq!(ten.line(), format!("10,000 → {five}"));
            assert_eq!(describe(12_000, &m).material, twelve);
            assert_eq!(describe(4_000, &m).material, four_thousand);
            assert_eq!(describe(14_000, &m).material, seven);
        });
    }
    with_language(Lang::De, || {
        let hand = describe(9_000, &m);
        assert_eq!((hand.material.as_str(), hand.known), ("3× Handabbau", true), "{hand:?}");
        assert_eq!(describe(3_000, &m).material, "1× Handabbau");
        assert_eq!(describe(28_000, &m).material, "7× ROC/Boden · 14× Bergung");
        assert_eq!(describe(30_000, &m).material, "10× Handabbau · 15× Bergung");
    });
    // Nothing fits: the number is there, no ore is named; the words do not say the number is unknown. Also beyond
    // the most chunks of the ground deposits (7) and of the hand mining (10).
    for (lang, none) in [(Lang::De, "Kein Erz erkannt"), (Lang::En, "No ore recognised")] {
        with_language(lang, || {
            for number in [1_700, 32_000, 33_000] {
                let nothing = describe(number, &m);
                assert_eq!((nothing.material.as_str(), nothing.known), (none, false), "{nothing:?}");
                assert_eq!(nothing.number, with_thousands(number));
            }
        });
    }
    assert_eq!(display_name("Quantainium"), "Quantainium");
}

#[test]
fn ores_come_before_the_salvage_and_a_real_scan_of_a_game_is_named_as_seen() {
    use crate::language::{Lang, with_language};
    let data = jsonio::loads(
        r#"{"format": 1, "elemente": {
            "g8": {"name": "Corundum (Raw)", "scanSignature": 4225, "rarity": "common"},
            "g9": {"name": "zeta", "scanSignature": 1000, "rarity": "common"}}}"#,
    )
    .unwrap();
    let m = MiningData::of(&data);
    with_language(Lang::De, || {
        // 21,125 is five Corundum.
        assert_eq!(describe(21_125, &m).material, "5× Corundum (Raw)");
        // 2,000 is two of the ore and one salvage: the data list the salvage first (a tie in the deviation goes by
        // the name), the window puts the ore before it.
        assert_eq!(materials(2_000, &m), ["salvage", "zeta"]);
        assert_eq!(describe(2_000, &m).material, "2× zeta · 1× Bergung");
        // 4,000 is four of the ore, one ROC and two salvage: the ore stands first, the ground deposits after it.
        assert_eq!(materials(4_000, &m), ["roc", "salvage", "zeta"]);
        assert_eq!(describe(4_000, &m).material, "4× zeta · 1× ROC/Boden · 2× Bergung");
    });
}

#[test]
fn the_sizes_grow_with_the_picture_and_are_read_from_numbers_and_the_old_words() {
    assert!(Size::MIN.font_px(1080) < Size::DEFAULT.font_px(1080));
    assert!(Size::DEFAULT.font_px(1080) < Size::MAX.font_px(1080));
    assert!(Size::DEFAULT.font_px(2160) > Size::DEFAULT.font_px(1080));
    assert_eq!(Size::MIN.font_px(100), 8, "never smaller than eight points");
    // The default is about half of the former medium, and the slider reaches below the former small.
    assert_eq!(Size::DEFAULT, Size(11));
    assert!(Size::MIN.0 < Size::read(Some(&Value::str("klein"))).0);
    // The words of the three fixed sizes keep the size they stood for.
    assert_eq!(Size::read(Some(&Value::str("klein"))), Size(16));
    assert_eq!(Size::read(Some(&Value::str("mittel"))), Size(22));
    assert_eq!(Size::read(Some(&Value::str("gross"))), Size(32));
    assert_eq!(Size::read(Some(&Value::str("riesig"))), Size::DEFAULT);
    assert_eq!(Size::read(None), Size::DEFAULT);
    assert_eq!(Size::read(Some(&Size(14).value())), Size(14), "a number comes back as it was written");
    assert_eq!(Size::read(Some(&Value::int(2))), Size::MIN);
    assert_eq!(Size::read(Some(&Value::int(99))), Size::MAX);
}

#[test]
fn the_place_and_the_time_left_come_from_the_settings_with_their_limits() {
    for place in Place::ALL {
        assert_eq!(Place::from_setting(Some(place.setting())), place);
    }
    assert_eq!(Place::from_setting(None), Place::InfoBar, "as before: beside the info bar no window");
    let layout = Layout::default();
    assert_eq!((layout.place, layout.linger, layout.size), (Place::InfoBar, 5, Size::DEFAULT));
}

#[test]
fn without_an_offset_the_window_stands_centred_above_the_pill() {
    let pill = (900, 500, 120, 60);
    let (x, y) = place(pill, GAME, (200, 50), &Layout::default());
    assert_eq!(x, 900 + 60 - 100);
    // The gap is four per mille of the height.
    assert_eq!(y, 500 - 4 - 50);
}

#[test]
fn the_offset_moves_by_a_share_of_the_game_window_and_follows_the_pill() {
    let layout = Layout { dx: 100, dy: -50, ..Layout::default() };
    let a = place((900, 500, 120, 60), GAME, (200, 50), &layout);
    let b = place((1000, 560, 120, 60), GAME, (200, 50), &layout);
    assert_eq!(a, (860 + 192, 446 - 54));
    assert_eq!((b.0 - a.0, b.1 - a.1), (100, 60));
}

#[test]
fn the_window_never_leaves_the_game_window() {
    let layout = Layout::default();
    // Pill at the left edge: the window stays at the left edge.
    assert_eq!(place((0, 500, 120, 60), GAME, (400, 50), &layout).0, 0);
    // Pill at the top: the window stays at the top.
    assert_eq!(place((900, 10, 120, 60), GAME, (200, 50), &layout).1, 0);
    // Pill at the right edge.
    assert_eq!(place((1900, 500, 120, 60), GAME, (400, 50), &layout).0, 1920 - 400);
    // A game window on a second screen.
    let game = (1920, 0, 1920, 1080);
    assert_eq!(place((1925, 500, 120, 60), game, (400, 50), &layout).0, 1920);
}

#[test]
fn an_offset_taken_from_a_moved_window_gives_that_window_back() {
    let pill = (900, 500, 120, 60);
    let window = (200, 50);
    for target in [(700, 300), (1200, 100), (860, 446), (50, 900)] {
        let (dx, dy) = offset_for(target, pill, GAME, window);
        let back = place(pill, GAME, window, &Layout { dx, dy, ..Layout::default() });
        // A per mille of the window is 2 points wide, 1 high.
        assert!((back.0 - target.0).abs() <= 1 && (back.1 - target.1).abs() <= 1, "{target:?} -> {back:?}");
    }
    assert_eq!(offset_for((10, 10), pill, (0, 0, 0, 0), window), (0, 0));
}

#[test]
fn the_size_of_the_window_follows_the_letters_and_the_text() {
    let (w1, h1) = window_size(24, 12, true);
    let (w2, h2) = window_size(24, 24, true);
    assert!(w2 > w1 && h1 == h2);
    let (_, h_without) = window_size(24, 12, false);
    assert!(h_without < h1);
    let (w_small, _) = window_size(12, 12, true);
    assert!(w_small < w1);
}

// ------------------------------------------------------------ where the scanner reads

fn copy(name: &str) -> Paths {
    use crate::paths::{COPY_DATA_FOLDER, Host};
    let root = crate::testdir::fresh(name);
    std::fs::create_dir_all(root.join(COPY_DATA_FOLDER)).expect("data folder");
    let host = Host::new(crate::pypath::Flavor::native(), &[], &root.to_string_lossy());
    Paths::for_copy(host, &root.to_string_lossy()).expect("a copy")
}

#[test]
fn the_line_in_the_overlay_is_on_until_the_player_switches_it_off() {
    let paths = copy("signatur-zeile");
    assert!(Layout::default().overlay_line, "the layout without settings");
    assert!(Layout::read(&paths).overlay_line, "no setting written");
    assert!(paths.set_setting(keys::OVERLAY_LINE, Value::Bool(false)));
    assert!(!Layout::read(&paths).overlay_line, "switched off by the player");
}

#[test]
fn the_size_of_the_window_is_the_name_of_its_place() {
    assert_eq!(size_key((0, 0, 2560, 1440)), "2560x1440");
    assert_eq!(size_key((1920, 0, 3440, 1440)), "3440x1440");
}

#[test]
fn without_a_frame_and_a_place_nothing_narrows_the_search() {
    assert_eq!(Hint::default().narrow(GAME), None);
    // A place for another size does not count.
    let hint = Hint::default().with_place("1280x720", [100, 100, 120, 60]);
    assert_eq!(hint.narrow(GAME), None);
}

#[test]
fn a_place_found_is_read_with_a_margin_and_kept_inside_the_window() {
    let hint = Hint::default().with_place("1920x1080", [800, 120, 200, 60]);
    let (area, user) = hint.narrow(GAME).expect("narrow");
    assert!(!user);
    // Margin: a quarter of the width (at least 24) to the sides, half the height (at least 16) up and down.
    assert_eq!(area, (750, 90, 300, 120));
    // At the edge of the window it is cut.
    let edge = Hint::default().with_place("1920x1080", [0, 0, 200, 60]);
    assert_eq!(edge.narrow(GAME).unwrap().0, (0, 0, 250, 90));
    // On a window on a second screen the place counts from its corner.
    let second = (1920, 0, 1920, 1080);
    let moved = Hint::default().with_place("1920x1080", [800, 120, 200, 60]);
    assert_eq!(moved.narrow(second).unwrap().0, (1920 + 750, 90, 300, 120));
}

#[test]
fn the_frame_of_the_player_wins_over_the_place_found_and_is_marked_as_his() {
    let hint = Hint { frame: Some([100, 200, 300, 100]), places: vec![("1920x1080".into(), [800, 120, 200, 60])] };
    let (area, user) = hint.narrow(GAME).expect("frame");
    assert!(user);
    assert_eq!(area, (192, 216, 576, 108));
    // A frame beyond the window leaves nothing to read.
    let outside = Hint { frame: Some([995, 995, 5, 5]), places: Vec::new() };
    assert_eq!(outside.narrow(GAME), None);
}

#[test]
fn a_place_counts_as_another_only_when_it_moved_by_more_than_a_third_of_its_size() {
    let hint = Hint::default().with_place("k", [800, 120, 300, 60]);
    assert!(!hint.differs("k", [800, 120, 300, 60]));
    assert!(!hint.differs("k", [850, 130, 300, 60]), "50 of 300 is under a third");
    assert!(hint.differs("k", [920, 120, 300, 60]));
    assert!(hint.differs("k", [800, 160, 300, 60]));
    assert!(hint.differs("other", [800, 120, 300, 60]), "no place kept for that size yet");
    assert_eq!(hint.with_place("k", [1, 2, 3, 4]).place_for("k"), Some([1, 2, 3, 4]));
    assert_eq!(hint.with_place("k", [1, 2, 3, 4]).places.len(), 1);
}

#[test]
fn the_frame_and_the_places_go_through_the_settings_and_come_back() {
    let paths = copy("signatur-hinweis");
    assert_eq!(Hint::read(&paths), Hint::default());
    let hint = Hint { frame: Some([100, 200, 300, 100]), places: vec![("1920x1080".into(), [800, 120, 200, 60])] };
    assert!(paths.set_setting(keys::FRAME, hint.frame_value()));
    assert!(paths.set_setting(keys::PLACES, hint.places_value()));
    assert_eq!(Hint::read(&paths), hint);
    // A removed frame is stored as nothing; a broken one counts as none.
    assert!(paths.set_setting(keys::FRAME, Hint::default().frame_value()));
    assert_eq!(Hint::read(&paths).frame, None);
    assert!(paths.set_setting(keys::FRAME, Value::Array(vec![Value::int(1), Value::int(2)])));
    assert_eq!(Hint::read(&paths).frame, None);
    assert!(
        paths.set_setting(keys::FRAME, Value::Array(vec![Value::int(0), Value::int(0), Value::int(3), Value::int(3)]))
    );
    assert_eq!(Hint::read(&paths).frame, None, "a frame under a hundredth of the window is none");
}

#[test]
fn the_place_kept_last_is_given_in_per_mille_of_the_window_it_was_found_in() {
    assert_eq!(Hint::default().remembered(), None);
    let hint = Hint::default().with_place("1280x720", [10, 20, 100, 50]).with_place("1920x1080", [960, 540, 192, 108]);
    assert_eq!(hint.remembered(), Some([500, 500, 100, 100]), "the last one");
    // Found again in the first size, the place is still the last of the list: its own entry changes in place.
    assert_eq!(hint.with_place("1280x720", [640, 360, 128, 72]).remembered(), Some([500, 500, 100, 100]));
    for key in ["gross", "0x0", "1920x", "x1080"] {
        assert_eq!(Hint::default().with_place(key, [1, 2, 3, 4]).remembered(), None, "{key}");
    }
}
