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

//! The reading of the info card. Pictures and readings here are made up: frames drawn into an empty picture, words
//! as the recognition would give them.

use super::*;

fn names() -> Names {
    let mut names = Names::new();
    for entry in [
        Entry { name: "Sovereign IP-10".into(), class: Some("Mil".into()), size: Some(1), grade: Some('B') },
        Entry { name: "Titanium".into(), ..Entry::default() },
        Entry { name: "Titanium (Ore)".into(), ..Entry::default() },
        Entry { name: "Agricium".into(), ..Entry::default() },
        Entry { name: "Construction Pieces".into(), ..Entry::default() },
        Entry { name: "Ripper SMG Battery (60 Cap)".into(), ..Entry::default() },
        Entry { name: "Boomtube Rocket".into(), ..Entry::default() },
        Entry { name: "Boomtube Rocket Launcher".into(), ..Entry::default() },
        Entry { name: "Ksar Energy LMG Battery".into(), ..Entry::default() },
        Entry { name: "Mantis GT-220 Gatling".into(), ..Entry::default() },
    ] {
        names.add(entry);
    }
    names
}

const INI: &str = "\u{feff}item_Nameksar_lmg_energy_01_mag=Ksar Energy LMG Battery\r\n\
item_Descksar_lmg_energy_01_mag=A battery.\r\n\
items_commodities_titanium=Titanium\r\n\
item_NameMantis_GT220=Mantis GT-220 Gatling\r\n\
item_Name_unknown_thing=Something New\r\n\
ui_not_an_item=Titanium\r\n";

fn line(text: &str, top: f64) -> (String, Frame) {
    (text.to_string(), (100.0, top, 300.0, top + 18.0))
}

fn word(text: &str, x: f64, y: f64) -> Word {
    Word::new(text, x, y, 10.0 * text.chars().count() as f64, 16.0)
}

// ------------------------------------------------------------ Lines

#[test]
fn labels_below_the_name_are_known_in_german_and_english() {
    for label in [
        "Kapazität: 1.00SCU",
        "Capacity: 1.00SCU",
        "Auf dich registriert",
        "Registered To You",
        "Verbleibender Inhalt",
        "Munitionsanzahl: 80/0",
        "Ammo Count: 165/0",
        "Titanium 295 0.404 SCU",
        "Construction Pieces 0 8SCU",
        "Volumen 500 µSCU",
    ] {
        assert!(is_label(label), "{label}");
    }
    for name in ["Sovereign IP-10 (Mil/1/B)", "Ripper SMG Battery (60", "cap)", "Titanium", "＠item_Nameksar_lmg"] {
        assert!(!is_label(name), "{name}");
    }
}

#[test]
fn the_line_of_goods_gives_quality_and_amount() {
    assert_eq!(goods_line("Titanium 295 0.404 SCU"), Some((Some(295), 0.404)));
    assert_eq!(goods_line("Agricium 667 0,471 SCU"), Some((Some(667), 0.471)));
    assert_eq!(goods_line("Construction Pieces 0 8SCU"), Some((Some(0), 8.0)));
    assert_eq!(goods_line("Titanium 0.404 SCU"), Some((None, 0.404)));
    // Characters the recognition mistakes for digits.
    assert_eq!(goods_line("Titanium 29S O.4O4 SCU"), Some((Some(295), 0.404)));
    // A quality above the highest is no quality; the capacity and a volume are no line of goods.
    assert_eq!(goods_line("Titanium 2950 0.404 SCU"), Some((None, 0.404)));
    assert_eq!(goods_line("Kapazität: 1.00SCU"), None);
    assert_eq!(goods_line("Volumen 500 µSCU"), None);
    assert_eq!(goods_line("Titanium 295"), None);
    // A part of an SCU always has three decimals: a digit was lost.
    assert_eq!(goods_line("Agricium 588 0.14 SCU"), None);
    assert_eq!(goods_line("Agricium 588 0.1455 SCU"), None);
    assert_eq!(goods_line("SCU"), None);
    // A whole number of SCU stands behind a quality; alone behind the name it is the quality, the amount lost.
    assert_eq!(goods_line("Pressurized Ice 322 2 SCU"), Some((Some(322), 2.0)));
    assert_eq!(goods_line("Aslarite 602 SCU"), None);
    assert_eq!(goods_line("Aslarite 602SCU"), None);
    // A quality read mostly as letters is guessed: `SBO` of `588` must not become 580.
    assert_eq!(goods_line("Agricium SBO 0.020 SCU"), Some((None, 0.02)));
    assert_eq!(goods_line("Agricium SB8 0.020 SCU"), Some((None, 0.02)));
    assert_eq!(goods_line("Agricium 5B8 0.020 SCU"), Some((Some(588), 0.02)));
    // A lone round letter is the quality 0 of a good.
    assert_eq!(goods_line("Gold O 16 SCU"), Some((Some(0), 16.0)));
}

#[test]
fn the_capacity_and_the_ammunition_are_told_apart() {
    assert!(is_capacity_line("Kapazität: 1.00SCU"));
    assert!(is_capacity_line("Capacity: 8.00SCU"));
    assert!(!is_capacity_line("Titanium 295 0.404 SCU"));
    assert!(!is_capacity_line("Volumen: 500 µSCU"));
    assert!(!is_capacity_line("Volumen: 8000 pSCU"), "µ read as p");
    assert!(!is_capacity_line("Volumen: sono eSCU"), "µ read as e");
    assert_eq!(capacity_of("Kapazität: 1.00SCU"), Some(Some(1.0)));
    assert_eq!(capacity_of("Kapazitat: i.OOSCU"), Some(Some(1.0)));
    assert_eq!(capacity_of("Kapazitat: x.yzSCU"), Some(None));
    assert_eq!(capacity_of("Titanium 295 0.404 SCU"), None);
    assert!(is_ammo_line("Munitionsanzahl: 80/0"));
    assert!(is_ammo_line("Ammo Count: 4314/0"));
    assert!(!is_ammo_line("Auf dich registriert"));
}

#[test]
fn the_count_of_a_tile_is_the_number_behind_the_x() {
    assert_eq!(count_of("x3"), Some(3));
    assert_eq!(count_of("X1"), Some(1));
    assert_eq!(count_of("xl"), Some(1));
    assert_eq!(count_of("×12"), Some(12));
    assert_eq!(count_of("x 2"), Some(2));
    assert_eq!(count_of("2x"), Some(2), "the inventory writes the x behind");
    for not in ["x0", "x", "602Q", "x12345", "xa", "x2x", "2", ""] {
        assert_eq!(count_of(not), None, "{not}");
    }
}

#[test]
fn a_text_key_in_place_of_a_name_is_found() {
    assert_eq!(broken_key("＠item_Nameksar_lmg_energy_01_mag").as_deref(), Some("item_Nameksar_lmg_energy_01_mag"));
    assert_eq!(broken_key("@ item_Namefoo_bar").as_deref(), Some("item_Namefoo_bar"));
    assert_eq!(broken_key("item_Namefoo_bar").as_deref(), Some("item_Namefoo_bar"));
    assert_eq!(broken_key("Titanium"), None);
    assert_eq!(broken_key("@ab"), None);
}

// ------------------------------------------------------------ Texts of the game

#[test]
fn the_texts_of_the_game_resolve_keys_and_names() {
    let texts = GameTexts::parse(INI);
    assert_eq!(texts.len(), 5, "only keys beginning with item");
    assert_eq!(texts.text("ITEM_NAMEKSAR_LMG_ENERGY_01_MAG"), Some("Ksar Energy LMG Battery"));
    assert_eq!(texts.key_of("Ksar Energy LMG Battery"), Some("item_Nameksar_lmg_energy_01_mag"));
    assert_eq!(texts.key_of("titanium"), Some("items_commodities_titanium"));
    // The recognition reads the small l as a capital I: the only key it can be, not exact.
    let (key, text, exact) = texts.resolve("item_Nameksar_Img_energy_O1_mag").expect("folded");
    assert_eq!((key, text, exact), ("item_Nameksar_lmg_energy_01_mag", "Ksar Energy LMG Battery", false));
    assert_eq!(texts.resolve("item_Nameksar_lmg_energy_01_mag").map(|r| r.2), Some(true));
    assert_eq!(texts.resolve("item_Name_nothing"), None);
}

#[test]
fn translated_names_lead_back_to_the_english_ones() {
    let english = GameTexts::parse(INI);
    let german = GameTexts::parse("items_commodities_titanium=Titan\nitem_NameMantis_GT220=Mantis GT-220 Gatling\n");
    let pairs = translation_pairs(&english, &german);
    assert_eq!(pairs, vec![("Titan".to_string(), "Titanium".to_string())]);
    let mut set = names();
    assert_eq!(set.add_translations(&pairs), 1);
    assert_eq!(match_name("Titan", &set).name.as_deref(), Some("Titanium"));
}

// ------------------------------------------------------------ Names

#[test]
fn a_tag_behind_a_name_is_checked_where_the_set_knows_it() {
    let names = names();
    let agrees = match_name("Sovereign IP-10 (Mil/1/B)", &names);
    assert_eq!((agrees.certainty, agrees.tag), (Certainty::Sure, TagCheck::Agrees));
    assert_eq!(agrees.name.as_deref(), Some("Sovereign IP-10"));
    let contradicts = match_name("Sovereign IP-10 (Civ/1/B)", &names);
    assert_eq!((contradicts.certainty, contradicts.tag), (Certainty::Likely, TagCheck::Contradicts));
    // A name the set knows no class, size or grade of: the tag says nothing against it.
    let unknown = match_name("Agricium (Ind/2/A)", &names);
    assert_eq!((unknown.certainty, unknown.name.as_deref()), (Certainty::Sure, Some("Agricium")));
    // Brackets that belong to the name are no tag.
    let ore = match_name("Titanium (Ore)", &names);
    assert_eq!((ore.certainty, ore.name.as_deref()), (Certainty::Sure, Some("Titanium (Ore)")));
}

#[test]
fn every_blueprint_of_the_catalog_is_a_name_of_the_scan() {
    let data = crate::stock_data::StockData::sample();
    let catalog = crate::catalog_cache::parse(&crate::jsonio::loads(
        r#"{"bauplaene": {"zzq rifle": {"n": "ZZQ-9 Test Rifle", "a": "WeaponPersonal"}}}"#,
    )
    .expect("json"));
    assert!(stock_names(&data, None).entry("ZZQ-9 Test Rifle").is_none());
    let names = stock_names(&data, Some(&catalog));
    assert!(names.entry("ZZQ-9 Test Rifle").is_some(), "a blueprint the stock does not hold is a name too");
    assert_eq!(match_name("ZZQ-9 Test Rifle", &names).name.as_deref(), Some("ZZQ-9 Test Rifle"));
}

#[test]
fn the_names_of_the_stock_know_goods_from_items() {
    let data = crate::stock_data::StockData::sample();
    let names = stock_names(&data, None);
    assert!(names.len() > 1000, "items and goods of the bundled list: {}", names.len());
    assert!(is_goods(&names, "Titanium"));
    assert!(is_goods(&names, "Construction Pieces"));
    assert!(!is_goods(&names, "Sovereign IP-10"));
    assert!(!is_goods(&names, "No Such Thing"));
    // A good read whole is still sure; a tag behind a good says it is something else.
    assert_eq!(match_name("Titanium", &names).certainty, Certainty::Likely, "Titanium (Ore) begins with it");
    assert_eq!(match_name("Laranite (Mil/1/A)", &names).certainty, Certainty::Likely);
}

#[test]
fn a_card_of_goods_gives_name_quality_and_amount() {
    let lines = [line("Titanium", 10.0), line("Kapazität: 1.00SCU", 40.0), line("Titanium 295 0.404 SCU", 60.0)];
    let reading = read_lines(&lines, &names(), None);
    assert_eq!(reading.reading.name.as_deref(), Some("Titanium"));
    assert_eq!(reading.reading.certainty, Certainty::Sure);
    assert_eq!(reading.goods, Some((Some(295), 0.404)));
    assert!(reading.capacity && !reading.ammo);
    assert_eq!(reading.name_frame, Some((100.0, 10.0, 300.0, 28.0)));
    // An amount above the capacity of the box was read wrong.
    let over = [line("Titanium", 10.0), line("Kapazität: 1.00SCU", 40.0), line("Titanium 295 4 SCU", 60.0)];
    assert_eq!(read_lines(&over, &names(), None).goods, None);
    // Goods without a quality read are asked about.
    let bare = || {
        read_lines(
            &[line("Titanium", 10.0), line("Kapazität: 1.00SCU", 40.0), line("Titanium 0.4 SCU", 60.0)],
            &names(),
            None,
        )
    };
    assert_eq!(combine(&[bare(), bare(), bare()], &[], false, &[], &[]).certainty, Certainty::Likely);
}

#[test]
fn a_name_on_two_lines_is_read_whole() {
    let lines = [line("Ripper SMG Battery (60", 10.0), line("cap)", 30.0), line("Munitionsanzahl: 60/0", 55.0)];
    let reading = read_lines(&lines, &names(), None);
    assert_eq!(reading.reading.name.as_deref(), Some("Ripper SMG Battery (60 Cap)"));
    assert_eq!(reading.reading.certainty, Certainty::Sure);
    assert!(reading.ammo);
    assert_eq!(reading.name_frame, Some((100.0, 10.0, 300.0, 48.0)));
    // The beginning of a longer name alone is not sure; with the rest below it is.
    let short = read_lines(&[line("Boomtube Rocket", 10.0)], &names(), None);
    assert_eq!(short.reading.certainty, Certainty::Likely);
    let long = read_lines(&[line("Boomtube Rocket", 10.0), line("Launcher", 30.0)], &names(), None);
    assert_eq!(
        (long.reading.certainty, long.reading.name.as_deref()),
        (Certainty::Sure, Some("Boomtube Rocket Launcher"))
    );
}

#[test]
fn a_text_key_in_place_of_the_name_is_resolved_through_the_texts() {
    let texts = GameTexts::parse(INI);
    let lines = [line("＠item_Nameksar_lmg_energy_", 10.0), line("01_mag", 30.0), line("Munitionsanzahl: 80/0", 55.0)];
    let reading = read_lines(&lines, &names(), Some(&texts));
    assert_eq!(reading.key.as_deref(), Some("item_Nameksar_lmg_energy_01_mag"));
    assert_eq!(reading.reading.name.as_deref(), Some("Ksar Energy LMG Battery"));
    assert_eq!(reading.reading.certainty, Certainty::Sure);
    // Without the texts nothing can be said of it; a key of a name the stock lacks is asked about.
    let alone = read_lines(&lines, &names(), None);
    assert_eq!((alone.reading.certainty, alone.key.as_deref()), (Certainty::None, None));
    assert_eq!(alone.broken.as_deref(), Some("item_Nameksar_lmg_energy_01_mag"));
    let new = read_lines(&[line("＠item_Name_unknown_thing", 10.0)], &names(), Some(&texts));
    assert_eq!(new.reading.certainty, Certainty::Likely);
    assert_eq!(new.reading.name.as_deref(), Some("Something New"));
}

// ------------------------------------------------------------ The count on the tile

#[test]
fn the_count_of_the_tile_under_the_pointer_is_taken() {
    // Two rows of tiles 240 pixels apart, counts at their top right; the pointer in the second tile of the lower row.
    let mut words = Vec::new();
    for (row, y) in [100.0, 340.0].into_iter().enumerate() {
        for (column, x) in [300.0, 540.0, 780.0].into_iter().enumerate() {
            words.push(word(&format!("x{}", row * 3 + column + 1), x, y));
        }
    }
    words.push(word("602Q", 400.0, 450.0));
    assert_eq!(count_at(&words, (400.0, 420.0), 1440.0), Some(5));
    assert_eq!(count_at(&words, (520.0, 360.0), 1440.0), Some(5), "on the height of the count");
    assert_eq!(count_at(&words, (200.0, 200.0), 1440.0), Some(1));
    assert_eq!(count_at(&words, (900.0, 50.0), 1440.0), None, "nothing above");
}

// ------------------------------------------------------------ Putting readings together

fn goods_reading(amount: f64, quality: u32) -> CardReading {
    read_lines(
        &[
            line("Titanium", 10.0),
            line("Kapazität: 1.00SCU", 40.0),
            line(&format!("Titanium {quality} {amount} SCU"), 60.0),
        ],
        &names(),
        None,
    )
}

#[test]
fn goods_read_the_same_in_every_pass_are_sure() {
    let same = [goods_reading(0.404, 295), goods_reading(0.404, 295), goods_reading(0.404, 295)];
    let outcome = combine(&same, &[1, 1], false, &[], &[]);
    assert_eq!(outcome.certainty, Certainty::Sure);
    assert_eq!((outcome.unit, outcome.quantity, outcome.quality), (Unit::Scu, 0.404, Some(295)));
    assert_eq!((outcome.kind, outcome.quantity_read), (CardKind::Goods, true));
    let mixed = [goods_reading(0.404, 295), goods_reading(0.401, 295), goods_reading(0.404, 295)];
    let differ = combine(&mixed, &[], false, &[], &[]);
    assert_eq!((differ.certainty, differ.quantity), (Certainty::Likely, 0.404));
    // Read again alone and large, the line adds its readings: five against one is a clear majority, three against
    // three none.
    let again = combine(&mixed, &[], false, &[(Some(295), 0.404), (Some(295), 0.404), (Some(295), 0.404)], &[]);
    assert_eq!((again.certainty, again.quantity), (Certainty::Sure, 0.404));
    let split = combine(&mixed, &[], false, &[(Some(295), 0.401), (Some(295), 0.401), (Some(295), 0.404)], &[]);
    assert_eq!(split.certainty, Certainty::Likely, "three against three is no clear majority");
    // The quality read wrong in the large readings is outvoted, not taken.
    let quality_read = [(Some(50), 0.404), (Some(588), 0.404), (Some(50), 0.404)];
    let outvoted = combine(&same, &[], false, &quality_read, &[]);
    assert_eq!(outvoted.certainty, Certainty::Likely, "three against three");
    // The field of the quality read alone adds its readings.
    let field = combine(&same, &[], false, &quality_read, &[295, 295, 295]);
    assert_eq!((field.certainty, field.quality), (Certainty::Sure, Some(295)));
    assert_eq!(strip_number("588 588 588"), Some(588));
    assert_eq!(strip_number("588 Æ88 588"), Some(588));
    assert_eq!(strip_number("S88 Æ88 588"), None, "one copy is too little");
    let quality = combine(
        &[goods_reading(0.404, 295), goods_reading(0.404, 296), goods_reading(0.404, 295)],
        &[],
        false,
        &[],
        &[],
    );
    assert_eq!((quality.certainty, quality.quality), (Certainty::Likely, Some(295)));
    // Goods take no count of the tile, not even one for none.
    assert_eq!(combine(&same, &[], true, &[], &[]).quantity, 0.404);
}

#[test]
fn items_take_their_count_from_the_tile() {
    let item =
        || read_lines(&[line("Sovereign IP-10 (Mil/1/B)", 10.0), line("Auf dich registriert", 30.0)], &names(), None);
    let outcome = combine(&[item(), item(), item()], &[3, 3], false, &[], &[]);
    assert_eq!(outcome.certainty, Certainty::Sure);
    assert_eq!((outcome.unit, outcome.quantity, outcome.quality), (Unit::Pieces, 3.0, None));
    assert_eq!(outcome.kind, CardKind::Component);
    assert_eq!(outcome.name.as_deref(), Some("Sovereign IP-10"));
    let unread = combine(&[item(), item(), item()], &[], false, &[], &[]);
    assert_eq!((unread.certainty, unread.quantity, unread.quantity_read), (Certainty::Likely, 0.0, false));
    let once = combine(&[item(), item(), item()], &[3], false, &[], &[]);
    assert_eq!(once.certainty, Certainty::Likely);
    let differ = combine(&[item(), item(), item()], &[3, 8], false, &[], &[]);
    assert_eq!(differ.certainty, Certainty::Likely);
    // In the inventory a single item has no count.
    let single = combine(&[item(), item(), item()], &[], true, &[], &[]);
    assert_eq!((single.certainty, single.quantity, single.quantity_read), (Certainty::Sure, 1.0, true));
    assert_eq!(combine(&[item(), item(), item()], &[2, 2], true, &[], &[]).quantity, 2.0);
    // Goods whose capacity and amount were not read whole are no sure item.
    let unread_goods = || {
        read_lines(
            &[line("Titanium", 10.0), line("Kapazitat: IOOSCU", 30.0), line("Titanium 295", 50.0)],
            &names(),
            None,
        )
    };
    let asked = combine(&[unread_goods(), unread_goods(), unread_goods()], &[1, 1, 1], false, &[], &[]);
    assert_eq!((asked.unit, asked.certainty), (Unit::Pieces, Certainty::Likely));
}

// ------------------------------------------------------------ Pictures

/// An empty picture 1440 high in the colour of the freight manager.
fn ground(width: i64, height: i64) -> Bitmap {
    let mut raw = Vec::with_capacity((width * height * 4) as usize);
    for _ in 0..width * height {
        raw.extend_from_slice(&[22, 64, 99, 255]);
    }
    Bitmap { width, height, raw }
}

/// Draws the outline of `area`, `thick` pixels wide, in the colour BGR.
fn outline(bitmap: &mut Bitmap, area: Area, thick: i64, bgr: [u8; 3]) {
    let mut put = |x: i64, y: i64| {
        let at = ((y * bitmap.width + x) * 4) as usize;
        bitmap.raw[at..at + 3].copy_from_slice(&bgr);
    };
    for t in 0..thick {
        for x in area.0..area.0 + area.2 {
            put(x, area.1 + t);
            put(x, area.1 + area.3 - 1 - t);
        }
        for y in area.1..area.1 + area.3 {
            put(area.0 + t, y);
            put(area.0 + area.2 - 1 - t, y);
        }
    }
}

const CARD: Area = (2750, 686, 340, 68);

/// Whether a frame found is the one drawn, up to the width of its lines on every side.
fn close(found: Area, drawn: Area) -> bool {
    let near = |a: i64, b: i64| (a - b).abs() <= 3;
    near(found.0, drawn.0)
        && near(found.1, drawn.1)
        && near(found.0 + found.2, drawn.0 + drawn.2)
        && near(found.1 + found.3, drawn.1 + drawn.3)
}
const BORDER: [u8; 3] = [135, 165, 152];

#[test]
fn the_frame_of_the_card_is_found() {
    let mut picture = ground(3600, 1440);
    outline(&mut picture, CARD, 2, BORDER);
    // The frame of the tile under the pointer is a frame, but no card: it holds the pointer.
    const TILE: Area = (2610, 530, 230, 210);
    outline(&mut picture, TILE, 2, [57, 96, 124]);
    let mut frames = card_frames(&picture, (0, 0, 3600, 1440));
    frames.sort();
    assert_eq!(frames.len(), 2, "{frames:?}");
    assert!(close(frames[0], TILE) && close(frames[1], CARD), "{frames:?}");
    assert!(card_at(&picture, (2740, 676)).is_some_and(|f| close(f, CARD)));
    // A frame too faint is no frame.
    let mut faint = ground(3600, 1440);
    outline(&mut faint, CARD, 2, [40, 80, 110]);
    assert!(card_frames(&faint, (0, 0, 3600, 1440)).is_empty());
    // A card far from the pointer is none of it; a frame around the pointer neither.
    assert_eq!(card_at(&picture, (600, 300)), None);
    let mut around = ground(3600, 1440);
    outline(&mut around, (2600, 600, 400, 100), 2, BORDER);
    assert_eq!(card_at(&around, (2700, 650)), None);
}

#[test]
fn a_frame_open_on_one_side_or_too_small_is_no_card() {
    let mut picture = ground(3600, 1440);
    outline(&mut picture, CARD, 2, BORDER);
    // The right side painted over.
    for y in CARD.1 + 4..CARD.1 + CARD.3 - 4 {
        for x in CARD.0 + CARD.2 - 3..CARD.0 + CARD.2 {
            let at = ((y * 3600 + x) * 4) as usize;
            picture.raw[at..at + 3].copy_from_slice(&[22, 64, 99]);
        }
    }
    outline(&mut picture, (500, 500, 60, 40), 2, BORDER);
    assert!(card_frames(&picture, (0, 0, 3600, 1440)).is_empty());
}

/// Paints a made-up item into `area`: bright blocks on a pattern of `seed`, brighter by `light`.
fn item(bitmap: &mut Bitmap, area: Area, seed: u64, light: u8) {
    let mut state = seed;
    for by in 0..8 {
        for bx in 0..8 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let level = (state >> 56) as u8 / 2;
            for y in area.1 + area.3 * by / 8..area.1 + area.3 * (by + 1) / 8 {
                for x in area.0 + area.2 * bx / 8..area.0 + area.2 * (bx + 1) / 8 {
                    let at = ((y * bitmap.width + x) * 4) as usize;
                    let v = level.saturating_add(light);
                    bitmap.raw[at..at + 3].copy_from_slice(&[v, v, v]);
                }
            }
        }
    }
}

const TILE: Area = (2610, 530, 230, 210);

#[test]
fn the_tile_under_the_pointer_is_found_beside_the_card_over_its_corner() {
    let mut picture = ground(3600, 1440);
    item(&mut picture, (2640, 560, 170, 150), 7, 0);
    outline(&mut picture, TILE, 2, [57, 96, 124]);
    // The card stands over the lower right corner of the tile, as at a pointer inside it.
    for y in CARD.1..CARD.1 + CARD.3 {
        for x in CARD.0..CARD.0 + CARD.2 {
            let at = ((y * 3600 + x) * 4) as usize;
            picture.raw[at..at + 3].copy_from_slice(&[20, 40, 60]);
        }
    }
    outline(&mut picture, CARD, 2, BORDER);
    let card = card_at(&picture, (2740, 676));
    assert!(card.is_some_and(|f| close(f, CARD)), "{card:?}");
    let tile = tile_at(&picture, (2740, 676), card);
    assert!(tile.is_some_and(|t| close(t, TILE)), "{tile:?}");
    // No tile where the pointer stands on the ground.
    assert_eq!(tile_at(&picture, (1000, 300), None), None);
}

#[test]
fn a_tile_print_knows_the_same_item_under_other_light_and_another_card() {
    let tile = |seed: u64, light: u8| {
        let mut picture = ground(3600, 1440);
        item(&mut picture, (2640, 560, 170, 150), seed, light);
        picture
    };
    let first = TilePrint::of(&tile(7, 0), TILE, Some(CARD)).expect("a print");
    let brighter = TilePrint::of(&tile(7, 30), TILE, Some((2700, 650, 340, 68))).expect("a print");
    let other = TilePrint::of(&tile(8, 0), TILE, Some(CARD)).expect("a print");
    assert!(first.same(&brighter), "{}", first.likeness(&brighter));
    assert!(!first.same(&other), "{}", first.likeness(&other));
    assert_eq!(TilePrint::from_bytes(&first.to_bytes()), Some(first.clone()));
    assert_eq!(TilePrint::from_bytes(&[1, 2, 3]), None);
    // A tile covered almost whole has no print; a tile on the ground alone has nothing to compare.
    assert_eq!(TilePrint::of(&tile(7, 0), TILE, Some((2600, 520, 250, 200))), None);
    let empty = TilePrint::of(&ground(3600, 1440), TILE, None).expect("a print");
    assert_eq!(empty.likeness(&first), 0.0);
}

#[test]
fn an_outcome_becomes_what_the_booking_takes() {
    use crate::stock_scan_book::{Measure, Reading};
    let print = TilePrint { cells: (0..PRINT_CELLS * PRINT_CELLS).map(|i| Some((i % 50) as i8)).collect() };
    let sure = CardOutcome {
        name: Some("Titanium".into()),
        quantity: 0.404,
        unit: Unit::Scu,
        quality: Some(295),
        certainty: Certainty::Sure,
        candidates: vec!["Titanium".into()],
        quantity_read: true,
        tile_rect: Some((10, 20, 30, 40)),
        tile_print: Some(print.clone()),
        ..CardOutcome::nothing()
    };
    let Reading::Sure(item) = sure.reading() else { panic!("sure") };
    assert_eq!(
        (item.name.as_str(), item.quantity, item.measure, item.quality),
        ("Titanium", 0.404, Measure::Scu, Some(295))
    );
    assert_eq!(item.tile_rect, Some((10, 20, 30, 40)));
    assert_eq!(item.tile_print, Some(print.to_bytes()));
    // Sure of the name, but no quantity read: a question, as is anything not sure.
    let unread = CardOutcome { quantity_read: false, quantity: 0.0, ..sure.clone() };
    assert!(matches!(unread.reading(), Reading::Unsure { .. }));
    let asked = CardOutcome {
        name: None,
        certainty: Certainty::Unclear,
        candidates: vec!["Agricium".into(), "Aslarite".into()],
        picture: Some(vec![1, 2]),
        ..sure.clone()
    };
    let Reading::Unsure { item, candidates, picture } = asked.reading() else { panic!("a question") };
    assert_eq!((item.name.as_str(), candidates.len(), picture), ("Agricium", 2, Some(vec![1, 2])));
    assert_eq!(CardOutcome::nothing().reading(), Reading::Nothing);
}

/// The recognition, made up: the same words for every job of a kind.
struct Fake {
    card: Vec<Word>,
    count: Vec<Word>,
    jobs: Vec<Job>,
}

impl Ocr for Fake {
    fn read(&mut self, jobs: &[Job]) -> Result<Vec<Vec<Word>>, ScanError> {
        self.jobs.extend(jobs.iter().cloned());
        Ok(jobs
            .iter()
            .enumerate()
            .map(|(i, _)| if i < CARD_PASSES.len() { self.card.clone() } else { self.count.clone() })
            .collect())
    }
}

#[test]
fn the_card_at_the_pointer_is_read_with_the_count_of_its_tile() {
    let mut picture = ground(3600, 1440);
    outline(&mut picture, CARD, 2, BORDER);
    let mut ocr = Fake {
        card: vec![
            word("Sovereign", 2760.0, 696.0),
            word("IP-10", 2860.0, 696.0),
            word("(Mil/1/B)", 2920.0, 696.0),
            word("Auf", 2770.0, 725.0),
            word("dich", 2810.0, 725.0),
            word("registriert", 2860.0, 725.0),
        ],
        count: vec![word("x1", 2810.0, 540.0), word("x2", 3060.0, 540.0)],
        jobs: Vec::new(),
    };
    let texts = GameTexts::parse("item_NameSovereign_IP10=Sovereign IP-10\n");
    let outcome = read_card(&picture, (2740, 676), &mut ocr, &names(), Some(&texts)).expect("read");
    assert_eq!(outcome.certainty, Certainty::Sure);
    assert_eq!(outcome.name.as_deref(), Some("Sovereign IP-10"));
    assert_eq!(outcome.key.as_deref(), Some("item_NameSovereign_IP10"));
    assert_eq!((outcome.quantity, outcome.unit, outcome.kind), (1.0, Unit::Pieces, CardKind::Component));
    assert!(outcome.frame.is_some_and(|f| close(f, CARD)));
    assert!(outcome.picture.as_deref().is_some_and(|png| png.starts_with(b"\x89PNG")));
    // The card and the count, then the lines of the name alone.
    assert_eq!(ocr.jobs.len(), CARD_PASSES.len() + COUNT_PASSES.len() + NAME_PASSES.len());
    // The card is read inside its frame.
    let inner = ocr.jobs[0].area;
    assert!(
        inner.0 > CARD.0
            && inner.1 > CARD.1
            && inner.0 + inner.2 < CARD.0 + CARD.2
            && inner.1 + inner.3 < CARD.1 + CARD.3
    );
}

#[test]
fn the_writing_of_a_count_box_is_drawn_dark_on_white() {
    // A light box with a dark stroke inside, as the count of a tile.
    let mut picture = ground(400, 1440);
    for y in 100..124 {
        for x in 100..130 {
            let at = ((y * 400 + x) * 4) as usize;
            let v = if (112..118).contains(&x) && (105..119).contains(&y) { 60 } else { 160 };
            picture.raw[at..at + 3].copy_from_slice(&[v, v, v]);
        }
    }
    let boxes = count_boxes(&picture, (50, 50, 200, 200));
    assert_eq!(boxes, vec![(99, 99, 32, 26)], "the box with a pixel around it");
    // Its writing is one narrow stroke without an x: no `x1`.
    assert!(!box_reads_one(&count_marks(&picture, boxes[0]), boxes[0]));
    let (strip, starts) = count_strip(&picture, &boxes).expect("a strip");
    assert_eq!(starts, vec![(0, STRIP_HEIGHT)]);
    assert_eq!(strip.height, 3 * STRIP_HEIGHT);
    // White around, black writing in the middle of the first copy.
    assert_eq!(&strip.raw[0..3], &[255, 255, 255]);
    let middle = (((STRIP_HEIGHT + STRIP_HEIGHT / 2) * strip.width + STRIP_HEIGHT + 10) * 4) as usize;
    assert!(strip.raw[middle] < 80, "{}", strip.raw[middle]);
    // The marks of `x1`: an x and a narrow stroke higher than it; a wider digit, a broken stroke or a lone mark is
    // left to the recognition.
    let b = (0, 0, 30, 24);
    assert!(box_reads_one(&[(6, 6, 8, 9), (21, 4, 4, 12)], b));
    assert!(box_reads_one(&[(6, 6, 8, 9), (21, 4, 5, 12)], b));
    assert!(!box_reads_one(&[(6, 6, 8, 9), (21, 4, 8, 12)], b), "x4");
    assert!(!box_reads_one(&[(6, 6, 8, 9), (21, 4, 2, 3)], b), "a broken stroke");
    assert!(!box_reads_one(&[(21, 4, 4, 12)], b), "no x");
    assert!(!box_reads_one(&[(6, 6, 8, 9), (16, 4, 4, 12), (23, 4, 4, 12)], b), "x11");
    // The copies read as the count of the box; one copy alone is too little.
    assert_eq!(strip_count("xlxlxl"), Some(1));
    assert_eq!(strip_count("x I XIXI"), Some(1));
    assert_eq!(strip_count("x3 x8 x3"), Some(3));
    assert_eq!(strip_count("x1"), None);
    assert_eq!(strip_count("41"), None);
}

#[test]
fn without_a_card_nothing_is_read() {
    let picture = ground(3600, 1440);
    let mut ocr = Fake { card: vec![word("Titanium", 10.0, 10.0)], count: Vec::new(), jobs: Vec::new() };
    let outcome = read_card(&picture, (2740, 676), &mut ocr, &names(), None).expect("read");
    assert_eq!(outcome, CardOutcome::nothing());
    // Without a frame only the part around the pointer is read, for the writing of a card, at every enlargement.
    assert_eq!(ocr.jobs.len(), WRITING_SCALES.len());
    assert!(ocr.jobs.iter().all(|job| job.area == search_area((2740, 676), (3600, 1440))));
}

// ------------------------------------------------------------ The faint card of the inventory terminal

#[test]
fn a_stack_count_with_something_glued_to_its_front_counts_for_nothing() {
    assert_eq!(stack_strip_count("86x 86x 86x"), Some(86));
    assert_eq!(stack_strip_count("2x2x"), Some(2));
    // `86x` with its first digit read wrong is no `6x`.
    assert_eq!(stack_strip_count("Æ6x Æ6x Æ6x"), None);
    assert_eq!(stack_strip_count("2x"), None, "one copy is too little");
}

#[test]
fn a_misread_tag_is_written_as_meant() {
    assert_eq!(mend_tag("FR-76 (Mi112/A)").as_deref(), Some("FR-76 (Mil/2/A)"));
    assert_eq!(mend_tag("JS-400 (Milf2/A)").as_deref(), Some("JS-400 (Mil/2/A)"));
    assert_eq!(mend_tag("Polar (MillilB)").as_deref(), Some("Polar (Mil/1/B)"));
    assert_eq!(mend_tag("SuperDrive (Mi113/C)").as_deref(), Some("SuperDrive (Mil/3/C)"));
    // Two digits, a letter where the size stands, no grade: nothing to mend.
    assert_eq!(mend_tag("Polar (Mil/VB)"), None);
    assert_eq!(mend_tag("X (Mil/23/A)"), None);
    assert_eq!(mend_tag("Titanium (Ore)"), None);
}

#[test]
fn a_roman_numeral_read_exactly_picks_its_name() {
    let candidates = |names: &[&str]| names.iter().map(|n| (n.to_string(), 0.9)).collect::<Vec<(String, f64)>>();
    let deadbolts = candidates(&["Deadbolt V Cannon", "Deadbolt IV Cannon", "Deadbolt VI Cannon"]);
    assert_eq!(by_numeral("Oeadbolt V Cannon", &deadbolts).as_deref(), Some("Deadbolt V Cannon"));
    assert_eq!(by_numeral("Deadbolt VI Cannon", &deadbolts).as_deref(), Some("Deadbolt VI Cannon"));
    // `II` read as `11` is no numeral; a name that differs otherwise is not picked.
    let twos = candidates(&["Deadbolt II Cannon", "Deadbolt III Cannon"]);
    assert_eq!(by_numeral("Oeadbolt 11 Cannon", &twos), None);
    assert_eq!(by_numeral("Deadbolt V Repeater", &deadbolts), None);
}

#[test]
fn a_card_is_found_by_its_writing_beside_a_list() {
    // The card: a name, a label below it and more lines; beside it a list whose numbers stand in the same rows.
    let words = vec![
        word("JS-400", 2643.0, 402.0),
        word("(Mil/2/A)", 2713.0, 402.0),
        word("Volumen:", 2643.0, 421.0),
        word("252000", 2733.0, 421.0),
        word("Hersteller:", 2643.0, 458.0),
        word("Amon", 2763.0, 458.0),
        word("369/369", 3106.0, 421.0),
        word("Haltbarkeit", 2643.0, 520.0),
        word("980", 2763.0, 520.0),
    ];
    let lines = text_lines(&words);
    assert!(lines.iter().any(|l| l.text == "369/369"), "the list is a line of its own: {lines:?}");
    let cards = cards_by_writing(&lines, (3160, 440), 1440.0);
    let card = cards.first().copied().expect("a card");
    assert!(card.0 < 2643 && card.1 < 402 && card.0 + card.2 < 3106, "{card:?}");
    // A label further down the same card begins no card of its own.
    assert_eq!(cards.len(), 1, "{cards:?}");
}

#[test]
fn a_name_that_leaves_much_of_its_line_unexplained_is_in_question() {
    let mut names = names();
    for name in ["Sawbuck", "SW16BR2 “Sawbuck” Repeater", "TBF-4 Combat Knife", "Fresnel Energy LMG"] {
        names.add(Entry { name: name.into(), ..Entry::default() });
    }
    // The whole line names it: sure.
    assert_eq!(match_name("Sawbuck", &names).certainty, Certainty::Sure);
    // The left edge of the card cut off: a short name inside a long line.
    let cut = match_name("16BR2 “Sawbuck” Repeater", &names);
    assert!(cut.certainty < Certainty::Sure, "{cut:?}");
    // A nickname in quotes the name lacks makes it another item.
    let knife = match_name("TBF-4 \"Balefire\" Combat Knife (Nah)", &names);
    assert!(knife.certainty < Certainty::Sure, "{knife:?}");
    // A line above the label left out of the name: the name is in question.
    let lines = [line("Fresnel Energy LMG", 0.0), line("Battery cap)", 20.0), line("Munitionsanzahl: 165/0", 40.0)];
    let read = read_lines(&lines, &names, None);
    assert!(read.reading.certainty < Certainty::Sure, "{:?}", read.reading);
    assert!(read.name_frame.is_some_and(|f| f.3 >= 38.0), "both lines of the name read again: {:?}", read.name_frame);
    // A word lost inside the line of the name leaves a wide gap: `TBF-4 Combat` of `TBF-4 "Balefire" Combat`.
    names.add(Entry { name: "TBF-4 Combat Knife".into(), ..Entry::default() });
    let lost = [word("TBF-4", 100.0, 0.0), word("Combat", 230.0, 0.0), word("Knife", 100.0, 20.0)];
    let whole = [word("TBF-4", 100.0, 0.0), word("Combat", 156.0, 0.0), word("Knife", 100.0, 20.0)];
    let label = word("Munitionsanzahl:", 100.0, 40.0);
    let with = |w: &[Word]| {
        let mut all = w.to_vec();
        all.push(label.clone());
        all
    };
    assert_eq!(read_words(&with(&whole), &names, None).reading.certainty, Certainty::Sure);
    assert!(read_words(&with(&lost), &names, None).reading.certainty < Certainty::Sure);
}

/// The pattern of `c` in `font` (its first one).
fn pattern(font: &str, c: char) -> Vec<f64> {
    glyphs::font(font).expect("the font").patterns.iter().find(|p| p.0 == c).expect("the character").1.to_vec()
}

/// Draws `grid` (ink 0 to 1, [`glyphs::CELL_W`] × [`glyphs::CELL_H`]) into `area` of the picture: the ink mixes the
/// ground (`ground`) with the writing (`writing`), both as BGR.
fn draw(picture: &mut Bitmap, area: Area, grid: &[f64], ground: [u8; 3], writing: [u8; 3]) {
    for py in 0..area.3 {
        for px in 0..area.2 {
            let gx = (px * glyphs::CELL_W as i64 / area.2) as usize;
            let gy = (py * glyphs::CELL_H as i64 / area.3) as usize;
            let ink = grid[gy * glyphs::CELL_W + gx];
            let at = (((area.1 + py) * picture.width + area.0 + px) * 4) as usize;
            for c in 0..3 {
                picture.raw[at + c] = (f64::from(ground[c]) * (1.0 - ink) + f64::from(writing[c]) * ink).round() as u8;
            }
        }
    }
}

/// Fills `area` of the picture with one colour (BGR).
fn fill(picture: &mut Bitmap, area: Area, bgr: [u8; 3]) {
    for y in area.1..area.1 + area.3 {
        for x in area.0..area.0 + area.2 {
            let at = ((y * picture.width + x) * 4) as usize;
            picture.raw[at..at + 3].copy_from_slice(&bgr);
        }
    }
}

/// A count box at (100, 100) with `writing` dark on light grey, as the freight elevator draws it, each character a
/// cell 11 pixels wide and 12 high (the patterns of `cells`, mixed where a cell gives several).
fn count_box(cells: &[Vec<f64>]) -> (Bitmap, Area) {
    const BOX: [u8; 3] = [140, 150, 140];
    let mut picture = ground(400, 1440);
    let b = (100, 100, 14 + 12 * cells.len() as i64, 27);
    fill(&mut picture, b, BOX);
    for (i, grid) in cells.iter().enumerate() {
        draw(&mut picture, (106 + 12 * i as i64, 107, 11, 12), grid, BOX, [40, 45, 40]);
    }
    (picture, b)
}

#[test]
fn a_count_is_read_by_the_shape_of_its_digits() {
    let x = pattern("box", 'x');
    for d in 1..=9u32 {
        let digit = pattern("box", char::from_digit(d, 10).unwrap());
        let (picture, b) = count_box(&[x.clone(), digit]);
        assert_eq!(shaped_count(&picture, b), Some(d), "x{d}");
    }
    let (picture, b) = count_box(&[x.clone(), pattern("box", '1'), pattern("box", '7')]);
    assert_eq!(shaped_count(&picture, b), Some(17), "x17");
    // Writing without the x in front, or a box of the ammunition, is no count.
    let (picture, b) = count_box(&[pattern("box", '4'), pattern("box", '5')]);
    assert_eq!(shaped_count(&picture, b), None, "45");
    let (picture, b) = count_box(&[pattern("box", '0'), pattern("box", '/'), pattern("box", '0')]);
    assert_eq!(shaped_count(&picture, b), None, "0/0");
}

#[test]
fn a_digit_between_two_patterns_is_not_read() {
    // Half a 3, half an 8: like both, but leading neither by enough.
    let x = pattern("box", 'x');
    let (three, eight) = (pattern("box", '3'), pattern("box", '8'));
    let mixed: Vec<f64> = three.iter().zip(&eight).map(|(a, b)| (a + b) / 2.0).collect();
    let (picture, b) = count_box(&[x.clone(), mixed]);
    assert_eq!(shaped_count(&picture, b), None);
    let font = glyphs::font("box").unwrap();
    let ink = glyphs::Ink::of(&picture, box_writing(b), true).unwrap();
    let cells = glyphs::cells(&ink, font.advance);
    assert_eq!(cells.len(), 2);
    let (_, _, lead) = glyphs::judge(font, &cells[1].grid).unwrap();
    assert!(lead < glyphs::MIN_LEAD, "lead {lead}");
    // An even box (no writing at all) reads nothing.
    let (picture, b) = count_box(&[]);
    assert_eq!(shaped_count(&picture, b), None);
}

/// How wide the light writing of the line of goods draws a character beside the next, in pixels, for writing
/// [`glyphs::CELL_H`] pixels high: as the game does, side by side without a gap.
const LIGHT_W: i64 = 10;

/// A field of the quality at (100, 100) with `digits` light on teal, the brown card around it, each digit its
/// pattern drawn [`LIGHT_W`] wide.
fn quality_box(digits: &str) -> (Bitmap, Area) {
    quality_box_of(&digits.chars().map(|c| pattern("quality", c)).collect::<Vec<_>>())
}

/// [`quality_box`] with the cells drawn from `cells` (patterns, mixed where a cell gives several).
fn quality_box_of(cells: &[Vec<f64>]) -> (Bitmap, Area) {
    const TEAL: [u8; 3] = [77, 85, 75];
    let h = glyphs::CELL_H as i64;
    let mut picture = ground(400, 1440);
    let field = (100, 100, 6 + LIGHT_W * cells.len() as i64, h + 4);
    fill(&mut picture, field, TEAL);
    for (i, grid) in cells.iter().enumerate() {
        draw(&mut picture, (103 + LIGHT_W * i as i64, 102, LIGHT_W, h), grid, TEAL, [215, 225, 220]);
    }
    (picture, field)
}

/// A field of the amount at (100, 100) with `writing` light on olive: characters their patterns drawn [`LIGHT_W`]
/// wide, a point as a small square at the foot of the writing, a blank as a narrow gap.
fn amount_box(writing: &str) -> (Bitmap, Area) {
    const OLIVE: [u8; 3] = [20, 70, 82];
    const LIGHT: [u8; 3] = [200, 225, 230];
    let (w, h) = (LIGHT_W, glyphs::CELL_H as i64);
    let width = |c: char| match c {
        '.' => 3,
        ' ' => 4,
        _ => w,
    };
    let mut picture = ground(400, 1440);
    let field = (100, 100, 6 + writing.chars().map(width).sum::<i64>(), h + 4);
    fill(&mut picture, field, OLIVE);
    let mut x = 103;
    for c in writing.chars() {
        match c {
            '.' => fill(&mut picture, (x, 102 + h - 2, 2, 2), LIGHT),
            ' ' => {}
            _ => draw(&mut picture, (x, 102, w, h), &pattern("amount", c), OLIVE, LIGHT),
        }
        x += width(c);
    }
    (picture, field)
}

/// What the patterns of `font` make of each cell of the writing in `part` (light writing): place and judgement.
fn judged(font: &str, picture: &Bitmap, part: Area) -> Vec<String> {
    let font = glyphs::font(font).unwrap();
    glyphs::Ink::of(picture, (part.0 - 2, part.1, part.2 + 4, part.3), false)
        .map(|ink| glyphs::cells(&ink, font.advance))
        .unwrap_or_default()
        .iter()
        .map(|c| match glyphs::judge(font, &c.grid) {
            Some((ch, score, lead)) => format!("{:.0}:{ch}:{score:.2}:{lead:.2}", c.place.0),
            None => "?".into(),
        })
        .collect()
}

#[test]
fn an_amount_is_read_by_the_shape_of_its_digits_in_front_of_its_unit() {
    let (picture, field) = amount_box("0.217 SCU");
    assert_eq!(shaped_amount(&picture, field), Some(0.217), "{:?}", judged("amount", &picture, field));
    let (picture, field) = amount_box("0.020 SCU");
    assert_eq!(shaped_amount(&picture, field), Some(0.02), "{:?}", judged("amount", &picture, field));
    let (picture, field) = amount_box("17 SCU");
    assert_eq!(shaped_amount(&picture, field), Some(17.0), "{:?}", judged("amount", &picture, field));
    assert_eq!(checked_amount(&picture, field, &[17.0]), Some(17.0));
    // Against another amount read, the shapes give their own, so that the card is asked about; alone nothing.
    assert_eq!(checked_amount(&picture, field, &[1.7]), Some(17.0));
    assert_eq!(checked_amount(&picture, field, &[]), None);
    // Without the unit read sure behind it, the number may be cut: nothing.
    let (picture, field) = amount_box("17 SC");
    assert_eq!(shaped_amount(&picture, field), None);
    // A nought in front of a whole number, a part with other than three places, or more than a box holds (`1.500`
    // with its point lost) is no amount.
    let (picture, field) = amount_box("06 SCU");
    assert_eq!(shaped_amount(&picture, field), None);
    let (picture, field) = amount_box("0.50 SCU");
    assert_eq!(shaped_amount(&picture, field), None);
    let (picture, field) = amount_box("1.500 SCU");
    assert_eq!(shaped_amount(&picture, field), None);
}

#[test]
fn the_shapes_of_the_digits_check_a_quality_the_recognition_read() {
    // An `8` leads a `6` by too little to be read sure by its shape alone, but enough to check a reading.
    let (picture, field) = quality_box("588");
    assert_eq!(shaped_quality(&picture, field), None);
    assert_eq!(checked_quality(&picture, field, &[588]), Some(588));
    assert_eq!(checked_quality(&picture, field, &[580, 588]), Some(588));
    // Against another number read they give their own, so that the card is asked about; alone nothing.
    assert_eq!(checked_quality(&picture, field, &[568]), Some(588));
    assert_eq!(checked_quality(&picture, field, &[]), None);
    // What the best patterns speak for, however little they lead: only to put a reading in question.
    assert_eq!(best_goods(&picture, None, Some(field)), (None, Some(588)));
    // A digit half 6, half 8 leads neither by enough even to check one.
    let mixed: Vec<f64> = pattern("quality", '6').iter().zip(&pattern("quality", '8')).map(|(a, b)| (a + b) / 2.0).collect();
    let (picture, field) = quality_box_of(&[pattern("quality", '5'), mixed, pattern("quality", '8')]);
    assert_eq!(checked_quality(&picture, field, &[568, 588]), None, "{:?}", judged("quality", &picture, field));
    // Gold without quality shows a lone nought.
    let (picture, field) = quality_box("0");
    assert_eq!(checked_quality(&picture, field, &[0]), Some(0));
}

#[test]
fn shapes_against_the_recognition_make_a_question() {
    // Goods whose amount the recognition read alike every time, but the shapes of its digits read another: asked.
    let lines = [line("Titanium", 10.0), line("Kapazität: 1.00SCU", 40.0), line("Titanium 295 0.186 SCU", 60.0)];
    let card = read_lines(&lines, &names(), None);
    let cards = [card.clone(), card.clone(), card];
    let windows = [(Some(295), 0.186), (Some(295), 0.186), (Some(295), 0.186)];
    let sure = combine(&cards, &[], false, &windows, &[295, 295]);
    assert_eq!((sure.certainty, sure.quantity, sure.quality), (Certainty::Sure, 0.186, Some(295)));
    let mut same = sure.clone();
    against_shapes(&mut same, Some(0.186), Some(295));
    assert_eq!(same.certainty, Certainty::Sure);
    let mut other_amount = sure.clone();
    against_shapes(&mut other_amount, Some(0.166), None);
    assert_eq!(other_amount.certainty, Certainty::Likely);
    let mut other_quality = sure.clone();
    against_shapes(&mut other_quality, None, Some(298));
    assert_eq!(other_quality.certainty, Certainty::Likely);
    // Nothing from the shapes leaves the outcome as it is.
    let mut nothing = sure;
    against_shapes(&mut nothing, None, None);
    assert_eq!(nothing.certainty, Certainty::Sure);
}

#[test]
fn a_quality_is_read_by_the_shape_of_its_digits_where_it_stands_whole() {
    // Digits that lead their next pattern clearly; an `8` of the game leads a `6` by little and is read sure only
    // where its shape is clear.
    for q in ["527", "714", "301", "410", "320"] {
        let (picture, field) = quality_box(q);
        let font = glyphs::font("quality").unwrap();
        let part = (field.0 - 2, field.1, field.2 + 4, field.3);
        let judged: Vec<_> = glyphs::Ink::of(&picture, part, false)
            .map(|ink| glyphs::cells(&ink, font.advance))
            .unwrap_or_default()
            .iter()
            .map(|c| (c.place, glyphs::judge(font, &c.grid)))
            .collect();
        assert_eq!(shaped_quality(&picture, field), q.parse().ok(), "{q}: {judged:?}");
    }
    // A field found too narrow cuts off the first digit: `602` must not come out as `2` or `02`.
    let (picture, field) = quality_box("602");
    let cut = (field.0 + 8, field.1, field.2 - 8, field.3);
    assert_eq!(shaped_quality(&picture, cut), None);
    // A quality above the highest there is, or with a nought in front, is none.
    let (picture, field) = quality_box("0588");
    assert_eq!(shaped_quality(&picture, field), None);
}

#[test]
fn the_patterns_know_every_digit() {
    for name in ["box", "quality", "amount"] {
        let font = glyphs::font(name).expect(name);
        for d in '0'..='9' {
            assert!(font.patterns.iter().any(|p| p.0 == d), "{name} {d}");
        }
    }
    assert!(glyphs::font("box").unwrap().patterns.iter().any(|p| p.0 == 'x'));
    for c in glyphs::UNIT.chars() {
        assert!(glyphs::font("amount").unwrap().patterns.iter().any(|p| p.0 == c), "amount {c}");
    }
    // A pattern kept as text comes back as it was, in steps of a ninth.
    let grid = pattern("quality", '8');
    let text = format!("font t 0.7\nchar 8\n{}", glyphs::pattern_text(&grid));
    let back = glyphs::parse_fonts(&text);
    let again = &back[0].patterns[0].1;
    assert!(grid.iter().zip(again.iter()).all(|(a, b)| (a - b).abs() < 1e-9));
}
