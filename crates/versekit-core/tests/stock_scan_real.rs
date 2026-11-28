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

//! Reads screenshots of the inventory and the freight manager with the text recognition of Windows, as the scan in
//! the game will, and counts the outcome against what each picture should give. The pictures (JPEG or PNG) lie in
//! a folder of their own, beside a list `lager-scan-erwartet.json`:
//!
//! ```json
//! [{"datei": "de/ScreenShot-….jpg", "zeiger": [2742, 678], "name": "Sovereign IP-10", "menge": 1,
//!   "einheit": "stueck", "qualitaet": null, "art": "komponente"},
//!  {"datei": "de/leer.jpg", "zeiger": [2400, 700], "name": null}]
//! ```
//!
//! `zeiger` is where the mouse pointer stood, `name` the stock name the card shows (`null`: no card, or a card with
//! the text key `schluessel` in place of a name), `einheit` `scu` or `stueck`, `art` one of `ware`, `gegenstand`,
//! `komponente`, `schiffswaffe`, `munition`; `gruppe` (optional) counts a picture apart from the folder it lies in.
//! The pictures, the catalog and the texts of the game are only read.
//!
//! Run with `VERSEKIT_STOCK_PICTURES=<folder> cargo test -p versekit-core --release --test stock_scan_real --
//! --ignored --nocapture`; with `VERSEKIT_CATALOG` (`katalog-spieldaten.json`) the tags of components are checked,
//! with `VERSEKIT_INI_EN` (and `VERSEKIT_INI_GAME`) text keys and translated names are resolved.

#![cfg(windows)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use versekit_core::blueprint_scan::{Certainty, Names};
use versekit_core::refinery_scan::Area;
use versekit_core::stock_scan::{
    CardOutcome, GameTexts, Unit, card_frames, read_card, read_card_where, stock_names, translation_pairs,
};
use versekit_core::stock_scan::important::{Group, add_important_names, important_names};
use versekit_core::win_ocr::{Bitmap, Recognition};

/// A screenshot as BGRA.
fn bitmap_of(path: &Path) -> Bitmap {
    let picture = image::open(path).expect("the picture").to_rgba8();
    let (width, height) = picture.dimensions();
    let mut raw = picture.into_raw();
    for pixel in raw.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Bitmap { width: i64::from(width), height: i64::from(height), raw }
}

/// The pictures of a folder and its folders, sorted; with `VERSEKIT_STOCK_PREFIX` only those whose name begins with
/// it.
fn pictures(folder: &Path) -> Vec<PathBuf> {
    let prefix = std::env::var("VERSEKIT_STOCK_PREFIX").unwrap_or_default();
    let mut files = Vec::new();
    for entry in std::fs::read_dir(folder).expect("the folder").flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(pictures(&path));
        } else if path.file_name().is_some_and(|n| n.to_string_lossy().starts_with(&prefix))
            && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("png"))
        {
            files.push(path);
        }
    }
    files.sort();
    files
}

/// The names of the kinds that matter (blueprint items, components, armour, weapons of a person), as
/// [`names_and_texts`] found them, under their comparison form (`plain`).
static IMPORTANT: std::sync::OnceLock<BTreeMap<String, Group>> = std::sync::OnceLock::new();

/// The comparison set of the stock (the bundled list of the stock, the catalog where given) and the texts of the
/// game where given.
fn names_and_texts() -> (Names, Option<GameTexts>) {
    let nowhere = std::env::temp_dir().join("versekit-stock-scan-test-has-no-files");
    let data = versekit_core::stock_data::StockData::load(|file| nowhere.join(file).to_string_lossy().into_owned());
    let catalog = std::env::var("VERSEKIT_CATALOG").ok().map(|path| {
        versekit_core::catalog_cache::parse(&versekit_core::jsonio::read_json(&path).expect("the catalog"))
    });
    let mut names = stock_names(&data, catalog.as_ref());
    let read = |path: String| GameTexts::parse(&String::from_utf8_lossy(&std::fs::read(path).expect("global.ini")));
    let mut english = std::env::var("VERSEKIT_INI_EN").ok().map(read);
    // As the context of the scan has them: the items of the kinds that matter, by the English texts.
    let important = important_names(english.as_ref(), catalog.as_ref());
    let added = add_important_names(&mut names, &important);
    println!("{} names of the kinds that matter, {added} of them new", important.len());
    // Looked up by the comparison form: the stock may spell a name in another case than the texts of the game.
    let _ = IMPORTANT
        .set(important.into_iter().map(|(name, group)| (versekit_core::blueprint_scan::plain(&name), group)).collect());
    if let (Some(english), Some(game)) = (&mut english, std::env::var("VERSEKIT_INI_GAME").ok().map(read)) {
        let taken = names.add_translations(&translation_pairs(english, &game));
        println!("{taken} names of the language of the game");
        // As the context of the scan has them.
        english.add_missing(&game);
        english.add_names_of(&game);
    }
    println!("{} names, {} texts", names.len(), english.as_ref().map_or(0, GameTexts::len));
    (names, english)
}

/// The card frame of a picture away from the debug text at the top right, and where the pointer stood: a little
/// above and left of the card, where the game puts it beside the pointer, when that lies in the frame of the tile
/// under the pointer; else in the middle of that frame (the inventory shows the card at a fixed place).
fn card_and_pointer(bitmap: &Bitmap) -> Option<(Area, (i64, i64))> {
    let unit = bitmap.height as f64;
    let frames: Vec<_> = card_frames(bitmap, (0, 0, bitmap.width, bitmap.height))
        .into_iter()
        .filter(|f| (f.0 as f64) < 0.85 * bitmap.width as f64)
        .collect();
    let squarish = |f: &(i64, i64, i64, i64)| {
        let ratio = f.2 as f64 / f.3.max(1) as f64;
        (0.55..1.8).contains(&ratio) && (0.06 * unit..0.22 * unit).contains(&(f.3 as f64))
    };
    let tile = frames.iter().find(|f| squarish(f)).copied();
    let frame = *frames.iter().find(|f| !squarish(f))?;
    let step = (unit / 1440.0 * 8.0) as i64;
    let corner = (frame.0 - step, frame.1 - step);
    let inside =
        |t: (i64, i64, i64, i64), p: (i64, i64)| p.0 >= t.0 && p.0 < t.0 + t.2 && p.1 >= t.1 && p.1 < t.1 + t.3;
    let pointer = match tile {
        Some(t) if !inside(t, corner) => (t.0 + t.2 / 2, t.1 + t.3 / 2),
        _ => corner,
    };
    Some((frame, pointer))
}

/// Finds the card of every picture of `VERSEKIT_STOCK_SURVEY`, writes the parts around them, nine to a sheet, into
/// `VERSEKIT_STOCK_SHEETS`, reads each card and writes a draft of the list of expected values beside the sheets
/// (`entwurf.json`), to be checked by eye against the sheets.
#[test]
#[ignore = "reads test pictures of the game"]
fn survey_of_cards() {
    let (Some(folder), Some(out)) =
        (std::env::var_os("VERSEKIT_STOCK_SURVEY"), std::env::var_os("VERSEKIT_STOCK_SHEETS"))
    else {
        return;
    };
    let (names, texts) = names_and_texts();
    let (cell_w, cell_h) = (800u32, 460u32);
    let mut sheet = image::RgbaImage::new(cell_w * 3, cell_h * 3);
    let mut index = 0usize;
    let mut draft = Vec::new();
    for path in pictures(Path::new(&folder)) {
        let bitmap = bitmap_of(&path);
        let file = path.strip_prefix(&folder).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        let Some((frame, pointer)) = card_and_pointer(&bitmap) else {
            println!("-- {file}: no card");
            continue;
        };
        let outcome =
            read_card(&bitmap, pointer, &mut Recognition::new(&bitmap), &names, texts.as_ref()).expect("read");
        if std::env::var_os("VERSEKIT_STOCK_MARKS").is_some() {
            use versekit_core::stock_scan::{count_area, count_box_at, count_boxes, count_marks};
            let area = count_area(pointer, (bitmap.width, bitmap.height));
            let boxes = count_boxes(&bitmap, area);
            let at = (pointer.0 as f64, pointer.1 as f64);
            if let Some((b, pitch)) = count_box_at(&boxes, at, bitmap.height as f64) {
                let marks: Vec<String> =
                    count_marks(&bitmap, b).iter().map(|m| format!("{}x{}@{}", m.2, m.3, m.0 - b.0)).collect();
                println!("   marks box {b:?} pitch {pitch:?}: {marks:?}");
            } else {
                println!("   marks: no box ({} boxes)", boxes.len());
            }
        }
        if std::env::var_os("VERSEKIT_STOCK_COUNT_WORDS").is_some() {
            use versekit_core::refinery_scan::{Job, Mode, Ocr};
            let area = versekit_core::stock_scan::count_area(pointer, (bitmap.width, bitmap.height));
            let jobs: Vec<Job> = versekit_core::stock_scan::COUNT_PASSES
                .iter()
                .map(|(scale, mode)| Job { area, scale: *scale, mode: *mode })
                .chain([Job { area, scale: 3.0, mode: Mode::Gray }])
                .collect();
            for words in Recognition::new(&bitmap).read(&jobs).expect("read") {
                let texts: Vec<String> = words.iter().map(|w| format!("{}@{},{}", w.t, w.x, w.y)).collect();
                println!("   count words: {texts:?}");
            }
        }
        println!(
            "{:02}/{} {file}: {:?} {:?} {} {:?} q{:?} {:?} {:?} tile {:?} lines {:?}",
            index / 9,
            index % 9,
            outcome.certainty,
            outcome.name,
            outcome.quantity,
            outcome.unit,
            outcome.quality,
            outcome.kind,
            outcome.candidates,
            outcome.tile_rect,
            outcome.lines
        );
        // Where the card and the count box of the tile stood, for sheets to check the draft by eye.
        let count_box = {
            use versekit_core::stock_scan::{count_area, count_box_at, count_boxes};
            let area = count_area(pointer, (bitmap.width, bitmap.height));
            count_box_at(&count_boxes(&bitmap, area), (pointer.0 as f64, pointer.1 as f64), bitmap.height as f64)
                .map(|(b, _)| b)
        };
        draft.push(serde_json::json!({
            "datei": file,
            "zeiger": [pointer.0, pointer.1],
            "name": outcome.name,
            "menge": outcome.quantity,
            "einheit": if outcome.unit == Unit::Scu { "scu" } else { "stueck" },
            "qualitaet": outcome.quality,
            "art": format!("{:?}", outcome.kind).to_lowercase(),
            "sicher": format!("{:?}", outcome.certainty),
            "angeboten": outcome.candidates,
            "rahmen": outcome.frame.map(|f| [f.0, f.1, f.2, f.3]),
            "kaestchen": count_box.map(|f| [f.0, f.1, f.2, f.3]),
        }));
        let x0 = (frame.0 - 300).max(0);
        let y0 = (frame.1 - 260).max(0);
        let (cx, cy) = ((index % 3) as u32 * cell_w, (index / 3 % 3) as u32 * cell_h);
        for dy in 0..i64::from(cell_h) {
            for dx in 0..i64::from(cell_w) {
                let (x, y) = (x0 + dx, y0 + dy);
                if x >= bitmap.width || y >= bitmap.height {
                    continue;
                }
                let at = ((y * bitmap.width + x) * 4) as usize;
                let pointer_mark = (x - pointer.0).abs() <= 4 && (y - pointer.1).abs() <= 4;
                // The frame read as the card, in magenta.
                let read_mark = outcome.frame.is_some_and(|f| {
                    let on_x = (x == f.0 || x == f.0 + f.2 - 1) && y >= f.1 && y < f.1 + f.3;
                    let on_y = (y == f.1 || y == f.1 + f.3 - 1) && x >= f.0 && x < f.0 + f.2;
                    on_x || on_y
                });
                let px = if pointer_mark {
                    image::Rgba([0, 255, 255, 255])
                } else if read_mark {
                    image::Rgba([255, 0, 255, 255])
                } else {
                    image::Rgba([bitmap.raw[at + 2], bitmap.raw[at + 1], bitmap.raw[at], 255])
                };
                sheet.put_pixel(cx + dx as u32, cy + dy as u32, px);
            }
        }
        index += 1;
        if index.is_multiple_of(9) {
            sheet.save(Path::new(&out).join(format!("sheet{:02}.png", index / 9 - 1))).expect("save");
            sheet = image::RgbaImage::new(cell_w * 3, cell_h * 3);
        }
    }
    if !index.is_multiple_of(9) {
        sheet.save(Path::new(&out).join(format!("sheet{:02}.png", index / 9))).expect("save");
    }
    let text = serde_json::to_string_pretty(&draft).expect("json");
    std::fs::write(Path::new(&out).join("entwurf.json"), text).expect("write the draft");
}

/// Sheets of small parts of pictures, enlarged, eighty to a sheet, one series of sheets per group.
struct Sheets {
    out: PathBuf,
    groups: BTreeMap<String, (image::RgbaImage, u32, u32)>,
}

impl Sheets {
    const CELL: (u32, u32) = (240, 100);
    const ACROSS: u32 = 8;
    const DOWN: u32 = 10;

    fn new(out: &Path) -> Sheets {
        Sheets { out: out.to_path_buf(), groups: BTreeMap::new() }
    }

    /// Puts `part` of the picture on the next place of the sheet of `group`; gives the sheet and the place.
    fn put(&mut self, group: &str, bitmap: &Bitmap, part: Area) -> String {
        let group = &group.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
        let (cw, ch) = Self::CELL;
        let zoom = (i64::from(cw) / (part.2 + 4).max(1)).min(i64::from(ch) / (part.3 + 4).max(1)).max(1);
        let entry = self
            .groups
            .entry(group.to_string())
            .or_insert_with(|| (image::RgbaImage::new(cw * Self::ACROSS, ch * Self::DOWN), 0, 0));
        let at = entry.1 % (Self::ACROSS * Self::DOWN);
        let (cx, cy) = ((at % Self::ACROSS) * cw, (at / Self::ACROSS) * ch);
        for dy in 0..i64::from(ch) {
            for dx in 0..i64::from(cw) {
                let (x, y) = (part.0 - 2 + dx / zoom, part.1 - 2 + dy / zoom);
                if x < 0 || y < 0 || x >= bitmap.width || y >= bitmap.height || x > part.0 + part.2 + 1 || y > part.1 + part.3 + 1
                {
                    continue;
                }
                let p = ((y * bitmap.width + x) * 4) as usize;
                entry.0.put_pixel(cx + dx as u32, cy + dy as u32, image::Rgba([bitmap.raw[p + 2], bitmap.raw[p + 1], bitmap.raw[p], 255]));
            }
        }
        let place = format!("{group}{:02}/{at:02}", entry.2);
        entry.1 += 1;
        if entry.1.is_multiple_of(Self::ACROSS * Self::DOWN) {
            let done = std::mem::replace(&mut entry.0, image::RgbaImage::new(cw * Self::ACROSS, ch * Self::DOWN));
            done.save(self.out.join(format!("{group}{:02}.png", entry.2))).expect("save");
            entry.2 += 1;
        }
        place
    }

    fn finish(self) {
        for (group, (sheet, n, page)) in self.groups {
            if !n.is_multiple_of(Self::ACROSS * Self::DOWN) {
                sheet.save(self.out.join(format!("{group}{page:02}.png"))).expect("save");
            }
        }
    }
}

/// The inside of a count box, where its writing stands.
fn box_inside(b: Area) -> Area {
    versekit_core::stock_scan::box_writing(b)
}

/// What the text recognition of Windows reads in a box of the ammunition drawn anew on a strip: `<number>/<number>`
/// where two of three readings agree.
fn windows_ammo(bitmap: &Bitmap, b: Area) -> Option<String> {
    use versekit_core::refinery_scan::{Job, Mode, Ocr};
    let (strip, _) = versekit_core::stock_scan::count_strip(bitmap, &[b])?;
    let jobs: Vec<Job> =
        [0.5, 0.75, 1.0].iter().map(|&scale| Job { area: (0, 0, strip.width, strip.height), scale, mode: Mode::Plain }).collect();
    let readings: Vec<String> = Recognition::new(&strip)
        .read(&jobs)
        .ok()?
        .iter()
        .filter_map(|words| {
            let text: String = words.iter().map(|w| w.t.as_str()).collect::<String>().replace(' ', "");
            // The strip writes the box three times: the first of its copies.
            let parts: Vec<&str> = text.split('/').collect();
            let [a, b, ..] = parts[..] else { return None };
            let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
            let b = &b[..b.len().min(a.len().max(1) + 1)];
            (digits(a) && digits(b)).then(|| format!("{a}/{b}"))
        })
        .collect();
    let first = readings.first()?;
    (readings.iter().filter(|r| *r == first).count() >= 2).then(|| first.clone())
}

/// The patterns of the digits in the file `VERSEKIT_GLYPHS` where it is given, in place of the ones built in.
fn patterns_given() -> Option<Vec<versekit_core::stock_scan::glyphs::Font>> {
    let path = std::env::var_os("VERSEKIT_GLYPHS")?;
    Some(versekit_core::stock_scan::glyphs::parse_fonts(&std::fs::read_to_string(path).expect("the patterns")))
}

/// Whether the middle colour of `part` is a grey of middle brightness, as the ground of a count box is.
fn greyish(bitmap: &Bitmap, part: Area) -> bool {
    let mut channels: [Vec<i32>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for y in part.1.max(0)..(part.1 + part.3).min(bitmap.height) {
        for x in part.0.max(0)..(part.0 + part.2).min(bitmap.width) {
            let p = ((y * bitmap.width + x) * 4) as usize;
            for (c, list) in channels.iter_mut().enumerate() {
                list.push(i32::from(bitmap.raw[p + c]));
            }
        }
    }
    let middle: Vec<i32> = channels
        .iter_mut()
        .map(|l| {
            l.sort_unstable();
            l.get(l.len() / 2).copied().unwrap_or(0)
        })
        .collect();
    let (low, high) = (*middle.iter().min().unwrap_or(&0), *middle.iter().max().unwrap_or(&0));
    high - low <= 25 && (50..=160).contains(&high)
}

/// Writes every count box of every picture of `VERSEKIT_GLYPH_SURVEY` (with `VERSEKIT_STOCK_PREFIX`) into
/// `VERSEKIT_STOCK_SHEETS`: the boxes enlarged on sheets, one series per reading by the patterns of the digits
/// (`box-x1-…`, `box-unread-…`), and a list of where each came from with the cells read (`boxes.txt`): the place,
/// the file, the box and per cell the best character, its likeness and lead.
#[test]
#[ignore = "reads test pictures of the game"]
fn survey_of_count_boxes() {
    use versekit_core::stock_scan::glyphs::{Ink, cells, font, judge, read};
    use versekit_core::stock_scan::{count_boxes, light_boxes};
    let (Some(folder), Some(out)) = (std::env::var_os("VERSEKIT_GLYPH_SURVEY"), std::env::var_os("VERSEKIT_STOCK_SHEETS"))
    else {
        return;
    };
    let fonts = patterns_given();
    let digits = fonts.as_ref().map_or_else(|| font("box"), |f| f.iter().find(|f| f.name == "box"));
    let mut sheets = Sheets::new(Path::new(&out));
    let mut list = String::new();
    let mut guesses = String::new();
    for path in pictures(Path::new(&folder)) {
        let bitmap = bitmap_of(&path);
        let file = path.file_name().unwrap().to_string_lossy().into_owned();
        // With `VERSEKIT_GLYPH_WIDE` the boxes of the ammunition (`369 / 369`), written in the same font.
        let whole = (0, 0, bitmap.width, bitmap.height);
        let wide = std::env::var_os("VERSEKIT_GLYPH_WIDE").is_some();
        let boxes = if wide {
            light_boxes(&bitmap, whole, (0.09, 6.0)).into_iter().filter(|b| b.2 * 4 > b.3 * 7).collect()
        } else {
            count_boxes(&bitmap, whole)
        };
        for b in boxes {
            // Count boxes are grey; the symbols of the filters and the pictures of items are not.
            if !greyish(&bitmap, b) {
                continue;
            }
            let ink = Ink::of(&bitmap, box_inside(b), true);
            let advance = digits.map_or(0.75, |f| f.advance);
            let found = ink.as_ref().map(|ink| cells(ink, advance)).unwrap_or_default();
            let read = match (digits, &ink) {
                (Some(f), Some(ink)) => read(f, ink).unwrap_or_else(|| "unread".into()),
                _ => format!("{}cells", found.len()),
            };
            // A box of the ammunition grouped by what the text recognition of Windows reads in it, where two of its
            // readings agree on `<number>/<number>`.
            let read = if wide { windows_ammo(&bitmap, b).map_or(read.clone(), |t| format!("{read}-{}", t.replace('/', "_"))) } else { read };
            let place = sheets.put(&format!("box-{read}-"), &bitmap, b);
            let judged: Vec<String> = found
                .iter()
                .map(|c| match digits.and_then(|f| judge(f, &c.grid)) {
                    Some((ch, score, lead)) => format!("{ch}:{score:.2}:{lead:.2}"),
                    None => "?".into(),
                })
                .collect();
            list.push_str(&format!("{place} {file} {} {} {} {} {}\n", b.0, b.1, b.2, b.3, judged.join(" ")));
            // Labels by what was read, to be checked on the sheets: a first guess without patterns is `x1`.
            let guess = if digits.is_none() && found.len() == 2 { "x1".to_string() } else { read.clone() };
            if guess.starts_with('x') {
                guesses.push_str(&format!("{file} {} {} {} {} {guess}\n", b.0, b.1, b.2, b.3));
            }
        }
    }
    sheets.finish();
    std::fs::write(Path::new(&out).join("boxes.txt"), list).expect("write the list");
    std::fs::write(Path::new(&out).join("box-guess.txt"), guesses).expect("write the guesses");
}

/// Writes the field of the quality of every card of goods in the pictures of `VERSEKIT_GLYPH_SURVEY` (with
/// `VERSEKIT_STOCK_PREFIX`) into `VERSEKIT_STOCK_SHEETS`: the fields on sheets, one series per number the text
/// recognition of Windows reads in them (`quality-588-…`, `quality-none-…`), and their list (`fields.txt`: place, file,
/// field, the numbers read, what the patterns read).
#[test]
#[ignore = "reads test pictures of the game"]
fn survey_of_quality_fields() {
    use versekit_core::refinery_scan::{Job, Ocr};
    use versekit_core::stock_scan::glyphs::{Ink, cells, font, judge, read_whole};
    use versekit_core::stock_scan::{CARD_PASSES, card_lines, field_number, quality_field, read_lines};
    let (Some(folder), Some(out)) = (std::env::var_os("VERSEKIT_GLYPH_SURVEY"), std::env::var_os("VERSEKIT_STOCK_SHEETS"))
    else {
        return;
    };
    let (names, texts) = names_and_texts();
    let fonts = patterns_given();
    let digits = fonts.as_ref().map_or_else(|| font("quality"), |f| f.iter().find(|f| f.name == "quality"));
    let mut sheets = Sheets::new(Path::new(&out));
    let mut list = String::new();
    let mut amounts = String::new();
    for path in pictures(Path::new(&folder)) {
        let bitmap = bitmap_of(&path);
        let file = path.file_name().unwrap().to_string_lossy().into_owned();
        let Some((frame, _)) = card_and_pointer(&bitmap) else { continue };
        let scale = |s: f64| (s * 1440.0 / bitmap.height as f64).clamp(1.0, 6.0);
        let inner = (frame.0 + 4, frame.1 + 4, frame.2 - 8, frame.3 - 8);
        let jobs: Vec<Job> = CARD_PASSES.iter().map(|(s, mode)| Job { area: inner, scale: scale(*s), mode: *mode }).collect();
        let line = Recognition::new(&bitmap)
            .read(&jobs)
            .expect("read")
            .iter()
            .find_map(|words| read_lines(&card_lines(words), &names, texts.as_ref()).goods_frame);
        // The field of the amount: found or not, and what its marks give.
        if let Some(line) = line {
            use versekit_core::stock_scan::glyphs::{Mark, marks, read_number, read_number_confirming};
            use versekit_core::stock_scan::{amount_field, shaped_amount};
            match amount_field(&bitmap, line, frame) {
                Some(a) => {
                    let part = (a.0 - 2, a.1, a.2 + 4, a.3);
                    let ink = Ink::of(&bitmap, part, false);
                    let font = fonts
                        .as_ref()
                        .map_or_else(|| font("amount"), |f| f.iter().find(|f| f.name == "amount"))
                        .expect("the font");
                    let shown: Vec<String> = ink
                        .as_ref()
                        .and_then(|ink| marks(ink, font.advance))
                        .map(|(marks, height)| {
                            let mut out = vec![format!("h{height:.0}")];
                            out.extend(marks.iter().map(|m| match m {
                                Mark::Char(cell, (l, r)) => match judge(font, &cell.grid) {
                                    Some((c, s, lead)) => format!("{l}-{r}:{c}:{s:.2}:{lead:.2}"),
                                    None => format!("{l}-{r}:?"),
                                },
                                Mark::Speck { columns, low } => format!("{}-{}:{}", columns.0, columns.1, if *low { "." } else { "'" }),
                            }));
                            out
                        })
                        .unwrap_or_default();
                    let sure = ink.as_ref().and_then(|ink| read_number(font, ink));
                    let loose = ink.as_ref().and_then(|ink| read_number_confirming(font, ink));
                    let place = sheets.put("amount-", &bitmap, part);
                    amounts.push_str(&format!(
                        "{place} {file} {a:?} sure {sure:?} loose {loose:?} amount {:?} {}\n",
                        shaped_amount(&bitmap, a),
                        shown.join(" ")
                    ));
                }
                None => amounts.push_str(&format!("{file} no field\n")),
            }
        }
        let Some(field) = line.and_then(|line| quality_field(&bitmap, line, frame)) else { continue };
        let windows = field_number(&bitmap, field);
        let ink = Ink::of(&bitmap, field, false);
        let advance = digits.map_or(0.75, |f| f.advance);
        let found = ink.as_ref().map(|ink| cells(ink, advance)).unwrap_or_default();
        let patterns = match (digits, &ink) {
            (Some(f), Some(ink)) => read_whole(f, ink).unwrap_or_else(|| "unread".into()),
            _ => format!("{}cells", found.len()),
        };
        let agreed = windows.first().filter(|n| windows.iter().filter(|m| m == n).count() >= 2);
        let group = format!("quality-{}-{patterns}-", agreed.map_or("none".to_string(), u32::to_string));
        let place = sheets.put(&group, &bitmap, field);
        let judged: Vec<String> = found
            .iter()
            .map(|c| match digits.and_then(|f| judge(f, &c.grid)) {
                Some((ch, score, lead)) => format!("{ch}:{score:.2}:{lead:.2}"),
                None => "?".into(),
            })
            .collect();
        list.push_str(&format!(
            "{place} {file} {} {} {} {} {windows:?} {patterns} {}\n",
            field.0,
            field.1,
            field.2,
            field.3,
            judged.join(" ")
        ));
    }
    sheets.finish();
    std::fs::write(Path::new(&out).join("fields.txt"), list).expect("write the list");
    std::fs::write(Path::new(&out).join("amounts.txt"), amounts).expect("write the amounts");
}

/// The labelled samples of a font: lines `<file> <left> <top> <width> <height> <writing>` of the part of a picture
/// of `folder` and what it shows (`x3`, `588`); `#` begins a note.
fn labelled(list: &Path) -> Vec<(String, Area, String)> {
    std::fs::read_to_string(list)
        .expect("the list of labels")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let w: Vec<&str> = l.split_whitespace().collect();
            let n = |i: usize| w[i].parse::<i64>().expect("a number");
            (w[0].to_string(), (n(1), n(2), n(3), n(4)), w[5].to_string())
        })
        .collect()
}

/// Writes the patterns of the digits (`glyphs.txt`) from the labelled samples of every font: `VERSEKIT_GLYPH_LABELS`
/// is a folder with one list per font (`box.txt`: count boxes, dark writing; `quality.txt`: fields of the quality,
/// light writing), as [`labelled`] reads them, the pictures lie in `VERSEKIT_GLYPH_SURVEY`; the patterns go to
/// `VERSEKIT_GLYPH_OUT`. A sample whose writing does not cut into as many cells as it has characters is left out and
/// named.
#[test]
#[ignore = "reads test pictures of the game"]
fn patterns_of_the_digits() {
    use versekit_core::stock_scan::glyphs::{Ink, cells, pattern_text};
    let (Some(labels), Some(folder), Some(out)) = (
        std::env::var_os("VERSEKIT_GLYPH_LABELS"),
        std::env::var_os("VERSEKIT_GLYPH_SURVEY"),
        std::env::var_os("VERSEKIT_GLYPH_OUT"),
    ) else {
        return;
    };
    let mut text = String::from(
        "# Patterns of the digits of the game, written by the test `stock_scan_real::patterns_of_the_digits` from labelled\n\
         # pictures of the freight elevator. Do not edit by hand.\n",
    );
    let mut cache: BTreeMap<String, Bitmap> = BTreeMap::new();
    for (name, dark, advance) in [("box", true, 0.9), ("quality", false, 0.85), ("amount", false, 0.85)] {
        let list = Path::new(&labels).join(format!("{name}.txt"));
        if !list.exists() {
            continue;
        }
        let mut samples: BTreeMap<char, Vec<Vec<f64>>> = BTreeMap::new();
        let mut steps = Vec::new();
        for (file, part, writing) in labelled(&list) {
            let bitmap = cache.entry(file.clone()).or_insert_with(|| bitmap_of(&Path::new(&folder).join(&file)));
            let inside = if dark { box_inside(part) } else { (part.0 - 2, part.1, part.2 + 4, part.3) };
            let Some(ink) = Ink::of(bitmap, inside, dark) else {
                println!("{name}: {file} {part:?} no writing");
                continue;
            };
            let found = cells(&ink, advance);
            if found.len() != writing.chars().count() {
                let places: Vec<String> = found.iter().map(|c| format!("{:.0}+{:.0}", c.place.0, c.place.2)).collect();
                println!("{name}: {file} {part:?} {writing}: {} cells {places:?}", found.len());
                continue;
            }
            for pair in found.windows(2) {
                steps.push((pair[1].place.0 - pair[0].place.0) / pair[0].place.3);
            }
            for (cell, c) in found.iter().zip(writing.chars()) {
                samples.entry(c).or_default().push(cell.grid.to_vec());
            }
        }
        steps.sort_by(f64::total_cmp);
        println!("{name}: steps between cells {:?}", steps.iter().map(|s| format!("{s:.2}")).collect::<Vec<_>>());
        text.push_str(&format!("\nfont {name} {advance}\nweights\n{}", pattern_text(&weights_of(&samples))));
        for (c, all) in &samples {
            for (mean, n) in groups_of(all) {
                println!("{name}: {c} from {n} of {} samples", all.len());
                text.push_str(&format!("# {n} samples\nchar {c}\n{}", pattern_text(&mean)));
            }
        }
    }
    std::fs::write(&out, text).expect("write the patterns");
}

/// How much each cell of the grid tells the characters apart: the spread of the means of the characters there,
/// against the spread of the samples of one character around its mean (every character counted alike, however many
/// samples it has); scaled to at most 1, at least a tenth.
fn weights_of(samples: &BTreeMap<char, Vec<Vec<f64>>>) -> Vec<f64> {
    let cells = samples.values().next().map_or(0, |s| s[0].len());
    let means: Vec<(Vec<f64>, &Vec<Vec<f64>>)> = samples
        .values()
        .map(|all| ((0..cells).map(|i| all.iter().map(|s| s[i]).sum::<f64>() / all.len() as f64).collect(), all))
        .collect();
    let k = means.len() as f64;
    let raw: Vec<f64> = (0..cells)
        .map(|i| {
            let overall = means.iter().map(|(m, _)| m[i]).sum::<f64>() / k;
            let between = means.iter().map(|(m, _)| (m[i] - overall).powi(2)).sum::<f64>() / k;
            let within = means
                .iter()
                .map(|(m, all)| all.iter().map(|s| (s[i] - m[i]).powi(2)).sum::<f64>() / all.len() as f64)
                .sum::<f64>()
                / k;
            between / (within + 0.005)
        })
        .collect();
    let most = raw.iter().copied().fold(1e-9, f64::max);
    raw.iter().map(|r| (r / most).max(0.1)).collect()
}

/// The samples of one character in groups of alike ones (the same character seen at another terminal, under
/// another light): up to [`GROUPS`] groups of at least [`GROUP_LEAST`] samples, each as its mean and its size. The
/// groups begin at the samples farthest apart and take every sample to the group it is most alike to, ten times.
fn groups_of(samples: &[Vec<f64>]) -> Vec<(Vec<f64>, usize)> {
    use versekit_core::stock_scan::glyphs::likeness;
    const GROUPS: usize = 4;
    const GROUP_LEAST: usize = 3;
    let mean_of = |members: &[&Vec<f64>]| -> Vec<f64> {
        let mut sum = vec![0.0; members[0].len()];
        for m in members {
            for (s, v) in sum.iter_mut().zip(m.iter()) {
                *s += v;
            }
        }
        sum.iter().map(|s| s / members.len() as f64).collect()
    };
    let all: Vec<&Vec<f64>> = samples.iter().collect();
    let k = (samples.len() / (2 * GROUP_LEAST)).clamp(1, GROUPS);
    let mut centres = vec![mean_of(&all)];
    while centres.len() < k {
        let far = samples
            .iter()
            .min_by(|a, b| {
                let near = |s: &Vec<f64>| centres.iter().map(|c| likeness(s, c)).fold(f64::MIN, f64::max);
                near(a).total_cmp(&near(b))
            })
            .expect("samples");
        centres.push(far.clone());
    }
    let mut members: Vec<Vec<&Vec<f64>>> = Vec::new();
    for _ in 0..10 {
        members = vec![Vec::new(); centres.len()];
        for s in samples {
            let best = (0..centres.len()).max_by(|&a, &b| likeness(s, &centres[a]).total_cmp(&likeness(s, &centres[b]))).expect("a centre");
            members[best].push(s);
        }
        centres = members.iter().zip(&centres).map(|(m, c)| if m.is_empty() { c.clone() } else { mean_of(m) }).collect();
    }
    let kept: Vec<(Vec<f64>, usize)> =
        members.iter().zip(centres).filter(|(m, _)| m.len() >= GROUP_LEAST).map(|(m, c)| (c, m.len())).collect();
    if kept.is_empty() { vec![(mean_of(&all), all.len())] } else { kept }
}

/// Reads every labelled sample of `VERSEKIT_GLYPH_LABELS` (lists as for [`patterns_of_the_digits`], pictures in
/// `VERSEKIT_GLYPH_SURVEY`) with the patterns built in (or those of `VERSEKIT_GLYPHS`) and counts right, unread and
/// wrong per font; every wrong one is named. Fails on a wrong one.
#[test]
#[ignore = "reads test pictures of the game"]
fn labelled_digits_are_read_right_or_not_at_all() {
    use versekit_core::stock_scan::glyphs::{Ink, font, read, read_whole};
    let (Some(labels), Some(folder)) = (std::env::var_os("VERSEKIT_GLYPH_LABELS"), std::env::var_os("VERSEKIT_GLYPH_SURVEY"))
    else {
        return;
    };
    let fonts = patterns_given();
    let mut cache: BTreeMap<String, Bitmap> = BTreeMap::new();
    let mut wrong = Vec::new();
    let mut astray: Vec<(f64, String)> = Vec::new();
    for (name, dark) in [("box", true), ("quality", false), ("amount", false)] {
        let list = Path::new(&labels).join(format!("{name}.txt"));
        if !list.exists() {
            continue;
        }
        let digits = fonts.as_ref().map_or_else(|| font(name), |f| f.iter().find(|f| f.name == name)).expect("the font");
        let amount = name == "amount";
        let mut by_label: BTreeMap<String, (usize, usize, usize)> = BTreeMap::new();
        for (file, part, writing) in labelled(&list) {
            let bitmap = cache.entry(file.clone()).or_insert_with(|| bitmap_of(&Path::new(&folder).join(&file)));
            // As the scan reads them ([`shaped_count`], [`shaped_quality`]): the field a little wider.
            let inside = if dark { box_inside(part) } else { (part.0 - 2, part.1, part.2 + 4, part.3) };
            let ink = Ink::of(bitmap, inside, dark);
            // An amount is labelled without its point (`0509SCU`): the point is no cell.
            let got = ink.as_ref().and_then(|ink| {
                if amount {
                    versekit_core::stock_scan::glyphs::read_number(digits, ink).map(|n| format!("{}SCU", n.replace('.', "")))
                } else if dark {
                    read(digits, ink)
                } else {
                    read_whole(digits, ink)
                }
            });
            // Every cell whose best pattern is another character than the label: how far it led.
            if let Some(ink) = &ink {
                use versekit_core::stock_scan::glyphs::{cells, judge};
                let found = cells(ink, digits.advance);
                if found.len() == writing.chars().count() {
                    for (cell, c) in found.iter().zip(writing.chars()) {
                        if let Some((best, score, lead)) = judge(digits, &cell.grid)
                            && best != c
                        {
                            astray.push((lead, format!("{name}: {file} {part:?} {c} looks like {best} {score:.2} lead {lead:.2}")));
                        }
                    }
                }
            }
            let entry = by_label.entry(writing.clone()).or_default();
            match got {
                Some(g) if g == writing => entry.0 += 1,
                None => {
                    entry.1 += 1;
                    if writing.len() <= 3 || amount {
                        use versekit_core::stock_scan::glyphs::{cells, judge};
                        let judged: Vec<String> = Ink::of(bitmap, inside, dark)
                            .map(|ink| cells(&ink, digits.advance))
                            .unwrap_or_default()
                            .iter()
                            .filter_map(|c| judge(digits, &c.grid))
                            .map(|(c, s, l)| format!("{c}:{s:.2}:{l:.2}"))
                            .collect();
                        println!("unread {name}: {file} {part:?} {writing}: {judged:?}");
                    }
                }
                Some(g) => {
                    entry.2 += 1;
                    wrong.push(format!("{name}: {file} {part:?} {writing} read {g}"));
                }
            }
        }
        for (label, (right, unread, bad)) in &by_label {
            println!("{name} {label}: right {right}, unread {unread}, wrong {bad}");
        }
        let total = by_label.values().fold((0, 0, 0), |a, v| (a.0 + v.0, a.1 + v.1, a.2 + v.2));
        println!("{name} all: right {}, unread {}, wrong {}", total.0, total.1, total.2);
    }
    // The cells that looked most like another character: how close a wrong digit came to being taken.
    astray.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (_, line) in astray.iter().take(8) {
        println!("astray {line}");
    }
    assert!(wrong.is_empty(), "read wrong: {wrong:#?}");
}

/// The mean brightness of every row of `band` (left, right, top, bottom), the pixels in `skip` left out; `None`
/// for a row left out whole.
fn row_profile(bitmap: &Bitmap, band: (i64, i64, i64, i64), skip: Option<(i64, i64, i64, i64)>) -> Vec<Option<f64>> {
    (band.2..band.3)
        .map(|y| {
            let (mut sum, mut n) = (0.0, 0.0);
            for x in (band.0..band.1).step_by(2) {
                if x < 0 || y < 0 || x >= bitmap.width || y >= bitmap.height {
                    continue;
                }
                if skip.is_some_and(|c| x >= c.0 && x < c.0 + c.2 && y >= c.1 && y < c.1 + c.3) {
                    continue;
                }
                let at = ((y * bitmap.width + x) * 4) as usize;
                let (b, g, r) =
                    (f64::from(bitmap.raw[at]), f64::from(bitmap.raw[at + 1]), f64::from(bitmap.raw[at + 2]));
                sum += (2.0 * r + 5.0 * g + b) / 8.0;
                n += 1.0;
            }
            (n > 0.0).then(|| sum / n)
        })
        .collect()
}

/// How far the list moved down from `before` to `after` (rows): the shift with the best correlation of the two
/// profiles, within `most` rows either way.
fn shift_between(before: &[Option<f64>], after: &[Option<f64>], most: i64) -> (i64, f64) {
    let mut best = (0, f64::MIN);
    for shift in -most..=most {
        let pairs: Vec<(f64, f64)> = (0..before.len() as i64)
            .filter_map(|y| {
                let a = (*before.get(y as usize)?)?;
                let b = (*after.get(usize::try_from(y + shift).ok()?)?)?;
                Some((a, b))
            })
            .collect();
        if pairs.len() < before.len() / 3 {
            continue;
        }
        let n = pairs.len() as f64;
        let (ma, mb) = (pairs.iter().map(|p| p.0).sum::<f64>() / n, pairs.iter().map(|p| p.1).sum::<f64>() / n);
        let cov: f64 = pairs.iter().map(|(a, b)| (a - ma) * (b - mb)).sum();
        let va: f64 = pairs.iter().map(|(a, _)| (a - ma).powi(2)).sum();
        let vb: f64 = pairs.iter().map(|(_, b)| (b - mb).powi(2)).sum();
        let r = if va > 0.0 && vb > 0.0 { cov / (va * vb).sqrt() } else { 0.0 };
        if r > best.1 {
            best = (shift, r);
        }
    }
    best
}

/// Reads every picture of a delivery (`VERSEKIT_STOCK_DELIVERY`, pictures in the order they were taken, the pointer
/// over one tile after the other, the list scrolled between them) and counts the different tiles of every item: a
/// tile is the same where its place in the list (its place on the screen plus how far the list was scrolled) and
/// its print agree. Prints the count per item, to be checked against the delivery.
#[test]
#[ignore = "reads test pictures of the game"]
fn tiles_of_a_delivery_are_counted_once() {
    use versekit_core::stock_scan::TilePrint;
    let Some(folder) = std::env::var_os("VERSEKIT_STOCK_DELIVERY") else { return };
    let (names, texts) = names_and_texts();
    struct Seen {
        file: String,
        name: Option<String>,
        certainty: Certainty,
        tile: Option<(i64, i64, i64, i64)>,
        print: Option<TilePrint>,
        card: Option<(i64, i64, i64, i64)>,
    }
    let files = pictures(Path::new(&folder));
    let mut seen = Vec::new();
    let mut height = 1440;
    for path in &files {
        let bitmap = bitmap_of(path);
        height = bitmap.height;
        let file = path.file_name().unwrap().to_string_lossy().into_owned();
        let Some((_, pointer)) = card_and_pointer(&bitmap) else {
            println!("-- {file}: no card");
            seen.push(Seen { file, name: None, certainty: Certainty::None, tile: None, print: None, card: None });
            continue;
        };
        let outcome =
            read_card(&bitmap, pointer, &mut Recognition::new(&bitmap), &names, texts.as_ref()).expect("read");
        println!(
            "{file}: {:?} {:?} x{} tile {:?} print {}",
            outcome.certainty,
            outcome.name,
            outcome.quantity,
            outcome.tile_rect,
            outcome.tile_print.is_some()
        );
        seen.push(Seen {
            file,
            name: outcome.name,
            certainty: outcome.certainty,
            tile: outcome.tile_rect,
            print: outcome.tile_print,
            card: outcome.frame,
        });
    }
    // The band of the list: where the tiles stood.
    let tiles: Vec<_> = seen.iter().filter_map(|s| s.tile).collect();
    assert!(!tiles.is_empty(), "no tile found");
    let band = (
        tiles.iter().map(|t| t.0).min().unwrap(),
        tiles.iter().map(|t| t.0 + t.2).max().unwrap(),
        tiles.iter().map(|t| t.1).min().unwrap(),
        tiles.iter().map(|t| t.1 + t.3).max().unwrap(),
    );
    let most = (0.4 * height as f64) as i64;
    let mut scrolled = 0i64;
    let mut before: Option<Vec<Option<f64>>> = None;
    // Every tile seen: name, place across, place down in the list, print, pictures.
    let mut found: Vec<(String, i64, i64, TilePrint, Vec<String>)> = Vec::new();
    let near = (0.05 * height as f64) as i64;
    // The upper bound: every picture a tile of its own, but the one before at the same place without scrolling.
    let mut pictures_per_item: BTreeMap<String, usize> = BTreeMap::new();
    let mut last: Option<(String, (i64, i64), i64)> = None;
    for (path, s) in files.iter().zip(&seen) {
        let bitmap = bitmap_of(path);
        let profile = row_profile(&bitmap, band, s.card);
        if let Some(before) = &before {
            let (shift, r) = shift_between(before, &profile, most);
            if shift != 0 {
                println!("{}: list moved {shift} rows (r {r:.2})", s.file);
            }
            scrolled -= shift;
        }
        before = Some(profile);
        if let (Some(name), true) = (&s.name, s.certainty > Certainty::None) {
            let place = s.tile.map_or((-1, -1), |t| (t.0, t.1));
            let again = last.as_ref().is_some_and(|(n, p, sc)| {
                n == name && *sc == scrolled && (p.0 - place.0).abs() <= near && (p.1 - place.1).abs() <= near
            });
            if !again {
                *pictures_per_item.entry(name.clone()).or_default() += 1;
            }
            last = Some((name.clone(), place, scrolled));
        }
        let (Some(name), Some(tile), Some(print)) = (&s.name, s.tile, &s.print) else { continue };
        if s.certainty == Certainty::None {
            continue;
        }
        let down = tile.1 + scrolled;
        match found.iter_mut().find(|f| f.0 == *name && (f.1 - tile.0).abs() <= near && (f.2 - down).abs() <= near) {
            Some(same) => {
                if !same.3.same(print) {
                    println!("{}: same place as {:?}, print differs ({:.2})", s.file, same.4, same.3.likeness(print));
                }
                same.4.push(s.file.clone());
            }
            None => found.push((name.clone(), tile.0, down, print.clone(), vec![s.file.clone()])),
        }
    }
    let mut per_item: BTreeMap<String, usize> = BTreeMap::new();
    for f in &found {
        *per_item.entry(f.0.clone()).or_default() += 1;
        println!("tile {} at {}/{}: {:?}", f.0, f.1, f.2, f.4);
    }
    for (name, count) in &per_item {
        println!("{count} x {name}");
    }
    for (name, count) in &pictures_per_item {
        println!("at most {count} x {name}");
    }
}

/// Two pictures of `VERSEKIT_STOCK_PICTURES` read and booked as the hotkey books them, into an own stock in an empty
/// folder at Orison: a component in pieces into the trade stock, ore with its quality into the material stock.
/// Needs the same variables as [`every_card_gives_its_stock_or_asks`].
#[test]
#[ignore = "reads test pictures of the game"]
fn a_picture_read_is_booked() {
    use versekit_core::stock_scan_book::{Outcome, Reading, Run, Target, run};
    let Some(folder) = std::env::var_os("VERSEKIT_STOCK_PICTURES") else { return };
    let folder = Path::new(&folder);
    let (names, texts) = names_and_texts();
    let list: serde_json::Value =
        serde_json::from_slice(&std::fs::read(folder.join("lager-scan-erwartet.json")).expect("the list"))
            .expect("a list");
    let pointer_of = |file: &str| {
        let item = list.as_array().expect("a list").iter().find(|i| i["datei"] == file).expect("the picture listed");
        (item["zeiger"][0].as_i64().expect("x"), item["zeiger"][1].as_i64().expect("y"))
    };
    let dir = versekit_core::testdir::Dir::new("lagerscan-lesen-buchen");
    let home = dir.path().to_string_lossy().into_owned();
    let vars = [(versekit_core::envvars::ENV_HOME, home.as_str()), (versekit_core::envvars::ENV_NO_NET, "1")];
    let paths = versekit_core::paths::Paths::new(versekit_core::paths::Host::new(
        versekit_core::pypath::Flavor::native(),
        &vars,
        &home,
    ));
    for (at, file) in ["de/ScreenShot-2026-11-21_03-16-19-603.jpg", "de/ScreenShot-2026-11-21_03-18-34-52A.jpg"]
        .into_iter()
        .enumerate()
    {
        let bitmap = bitmap_of(&folder.join(file));
        let outcome =
            read_card(&bitmap, pointer_of(file), &mut Recognition::new(&bitmap), &names, texts.as_ref()).expect("read");
        let reading = outcome.reading();
        assert!(
            matches!(reading, Reading::Sure(_)),
            "{file}: {:?} {:?} {} read {}",
            outcome.certainty,
            outcome.name,
            outcome.quantity,
            outcome.quantity_read
        );
        let ctx = Run {
            paths: &paths,
            target: Target::Own,
            place: Some("Orison".into()),
            place_since: None,
            unit: None,
            now: 1_000 * (at as i64 + 1),
        };
        let booked = run(&ctx, reading);
        assert!(matches!(booked, Outcome::Booked(_)), "{file}: {booked:?}");
    }
    let amount = |file: &str, find: &dyn Fn(&[serde_json::Value]) -> Option<usize>| {
        let text = std::fs::read_to_string(paths.app_file(file)).expect("the stock file");
        let entries: Vec<serde_json::Value> = match serde_json::from_str::<serde_json::Value>(&text).expect("json") {
            serde_json::Value::Array(entries) => entries,
            serde_json::Value::Object(map) => map.values().find_map(|v| v.as_array().cloned()).unwrap_or_default(),
            _ => Vec::new(),
        };
        find(&entries).map(|i| entries[i]["menge"].as_f64().expect("menge"))
    };
    let trade = amount(versekit_core::stock::TRADE_FILE, &|entries| {
        entries.iter().position(|e| e["name"] == "Sovereign IP-10" || e["ware"] == "Sovereign IP-10")
    });
    assert_eq!(trade, Some(1.0), "the component in the trade stock");
    let raw = amount(versekit_core::stock::RAW_FILE, &|entries| {
        entries.iter().position(|e| e["material"] == "Aslarite" && e["qualitaet"] == 575)
    });
    assert_eq!(raw, Some(0.017), "the ore in the material stock");
}

/// Prints the lines of writing around the pointer, as the card is looked for by its writing, and the words read
/// where the count of the tile stands.
fn show_what_was_read(bitmap: &Bitmap, pointer: (i64, i64), names: &Names) {
    use versekit_core::refinery_scan::{Job, Mode, Ocr};
    use versekit_core::stock_scan::{COUNT_PASSES, WRITING_SCALE, count_area, search_area, text_lines};
    let size = (bitmap.width, bitmap.height);
    let scale = |s: f64| (s * 1440.0 / bitmap.height as f64).clamp(1.0, 6.0);
    let writing = std::env::var("VERSEKIT_WRITING_SCALE").ok().and_then(|s| s.parse().ok()).unwrap_or(WRITING_SCALE);
    let job = Job { area: search_area(pointer, size), scale: scale(writing), mode: Mode::Plain };
    let words = Recognition::new(bitmap).read(&[job]).expect("read").remove(0);
    let lines = text_lines(&words);
    for line in &lines {
        println!("   line {:.0},{:.0}-{:.0},{:.0}: {}", line.left, line.top, line.right, line.bottom, line.text);
    }
    let card = versekit_core::stock_scan::card_by_writing(&lines, pointer, bitmap.height as f64);
    println!("   card by writing: {card:?}; hovered tile: {:?}", versekit_core::stock_scan::hovered_tile(bitmap, pointer));
    if let Some(card) = card {
        use versekit_core::stock_scan::{CARD_PASSES, card_lines, read_lines};
        let jobs: Vec<Job> =
            CARD_PASSES.iter().map(|(s, mode)| Job { area: card, scale: scale(*s), mode: *mode }).collect();
        for words in Recognition::new(bitmap).read(&jobs).expect("read") {
            let lines = card_lines(&words);
            let r = read_lines(&lines, names, None);
            let shown: Vec<&String> = lines.iter().map(|(t, _)| t).take(3).collect();
            println!("   card pass: {:?} {:?} labelled {} {:?}", r.reading.certainty, r.reading.name, r.labelled, shown);
        }
    }
    if let Some(tile) = versekit_core::stock_scan::hovered_tile(bitmap, pointer) {
        use versekit_core::stock_scan::{STACK_PASSES, corner_written, stack_corner};
        let corner = stack_corner(tile);
        let jobs: Vec<Job> = STACK_PASSES.iter().map(|(s, mode)| Job { area: corner, scale: scale(*s), mode: *mode }).collect();
        let read: Vec<Vec<String>> = Recognition::new(bitmap)
            .read(&jobs)
            .expect("read")
            .iter()
            .map(|words| words.iter().map(|w| w.t.clone()).collect())
            .collect();
        println!("   corner {corner:?} written {} read {read:?}", corner_written(bitmap, corner));
        let around = (corner.0 - corner.2 / 4, corner.1 - corner.3 / 3, corner.2 * 3 / 2, corner.3 * 5 / 3);
        if let Some((strip, _)) = versekit_core::stock_scan::count_strip_of(bitmap, &[around], true) {
            let jobs: Vec<Job> = [0.35, 0.5, 0.75, 1.0, 1.5, 2.0]
                .iter()
                .map(|&scale| Job { area: (0, 0, strip.width, strip.height), scale, mode: Mode::Plain })
                .collect();
            let read: Vec<Vec<String>> = Recognition::new(&strip)
                .read(&jobs)
                .expect("read")
                .iter()
                .map(|words| words.iter().map(|w| w.t.clone()).collect())
                .collect();
            println!("   strip {}x{} read {read:?}", strip.width, strip.height);
            if let Some(out) = std::env::var_os("VERSEKIT_STOCK_SHEETS") {
                let mut raw = strip.raw.clone();
                for pixel in raw.as_chunks_mut::<4>().0 {
                    pixel.swap(0, 2);
                    pixel[3] = 255;
                }
                let picture = image::RgbaImage::from_raw(strip.width as u32, strip.height as u32, raw).expect("strip");
                picture.save(Path::new(&out).join(format!("strip-{}-{}.png", pointer.0, pointer.1))).expect("save");
            }
        }
    }
    let area = count_area(pointer, size);
    {
        use versekit_core::stock_scan::{count_box_at, count_boxes, count_marks};
        let boxes = count_boxes(bitmap, area);
        match count_box_at(&boxes, (pointer.0 as f64, pointer.1 as f64), bitmap.height as f64) {
            Some((b, pitch)) => {
                let marks: Vec<String> =
                    count_marks(bitmap, b).iter().map(|m| format!("{}x{}@{},{}", m.2, m.3, m.0 - b.0, m.1 - b.1)).collect();
                println!("   box {b:?} pitch {pitch:?} of {} boxes: marks {marks:?}", boxes.len());
                if let Some((strip, _)) = versekit_core::stock_scan::count_strip(bitmap, &[b]) {
                    let jobs: Vec<Job> = [0.35, 0.5, 0.75, 1.0, 1.5, 2.0]
                        .iter()
                        .map(|&scale| Job { area: (0, 0, strip.width, strip.height), scale, mode: Mode::Plain })
                        .collect();
                    let raw: Vec<String> = Recognition::new(&strip)
                        .read(&jobs)
                        .expect("read")
                        .iter()
                        .map(|words| words.iter().map(|w| w.t.as_str()).collect::<Vec<_>>().join(" "))
                        .collect();
                    println!("   box strip read {raw:?}");
                }
            }
            None => println!("   no box of {} boxes {boxes:?}", boxes.len()),
        }
    }
    let jobs: Vec<Job> = COUNT_PASSES.iter().map(|(s, mode)| Job { area, scale: scale(*s), mode: *mode }).collect();
    for words in Recognition::new(bitmap).read(&jobs).expect("read") {
        let texts: Vec<String> = words.iter().map(|w| format!("{}@{},{}", w.t, w.x, w.y)).collect();
        println!("   count words: {texts:?}");
    }
}

/// Prints the line of goods of every reading of the card in `frame` and of every reading of that line alone.
fn show_goods_passes(bitmap: &Bitmap, frame: Area, names: &Names, texts: Option<&GameTexts>) {
    use versekit_core::refinery_scan::{Job, Ocr};
    use versekit_core::stock_scan::{CARD_PASSES, GOODS_PASSES, card_lines, goods_line, read_lines};
    let scale = |s: f64| (s * 1440.0 / bitmap.height as f64).clamp(1.0, 6.0);
    let inner = (frame.0 + 4, frame.1 + 4, frame.2 - 8, frame.3 - 8);
    let jobs: Vec<Job> = CARD_PASSES.iter().map(|(s, mode)| Job { area: inner, scale: scale(*s), mode: *mode }).collect();
    let mut line_frame = None;
    for words in Recognition::new(bitmap).read(&jobs).expect("read") {
        let r = read_lines(&card_lines(&words), names, texts);
        line_frame = line_frame.or(r.goods_frame);
        println!("   card pass goods {:?} capacity {:?} lines {:?}", r.goods, r.capacity_value, r.lines.iter().skip(1).collect::<Vec<_>>());
    }
    let Some(f) = line_frame else { return };
    let field = versekit_core::stock_scan::quality_field(bitmap, f, frame);
    let numbers = field.map(|field| versekit_core::stock_scan::field_number(bitmap, field));
    println!("   quality field {field:?} read {numbers:?}");
    if let (Some(field), Some(out)) = (field, std::env::var_os("VERSEKIT_STOCK_SHEETS")) {
        let around = (field.0 - field.2 / 4, field.1 - field.3 / 4, field.2 * 3 / 2, field.3 * 3 / 2);
        if let Some((strip, _)) = versekit_core::stock_scan::count_strip_of(bitmap, &[around], true) {
            let mut raw = strip.raw.clone();
            for pixel in raw.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
                pixel[3] = 255;
            }
            let picture = image::RgbaImage::from_raw(strip.width as u32, strip.height as u32, raw).expect("strip");
            picture.save(Path::new(&out).join(format!("field-{}-{}.png", field.0, field.1))).expect("save");
            use versekit_core::refinery_scan::{Job, Mode, Ocr};
            let jobs: Vec<Job> = [0.5, 0.75, 1.0, 1.5]
                .iter()
                .map(|&scale| Job { area: (0, 0, strip.width, strip.height), scale, mode: Mode::Plain })
                .collect();
            let raw: Vec<Vec<String>> = Recognition::new(&strip)
                .read(&jobs)
                .expect("read")
                .iter()
                .map(|words| words.iter().map(|w| w.t.clone()).collect())
                .collect();
            println!("   field strip read {raw:?}");
        }
    }
    let margin = 0.008 * bitmap.height as f64;
    let right = (f.2 + margin).max((frame.0 + frame.2) as f64 - margin / 2.0);
    let area = ((f.0 - margin) as i64, (f.1 - margin) as i64, (right - f.0 + margin) as i64, (f.3 - f.1 + 2.0 * margin) as i64);
    let jobs: Vec<Job> = GOODS_PASSES.iter().map(|(s, mode)| Job { area, scale: scale(*s), mode: *mode }).collect();
    for words in Recognition::new(bitmap).read(&jobs).expect("read") {
        let text = words.iter().map(|w| w.t.as_str()).collect::<Vec<_>>().join(" ");
        println!("   line pass {text:?} -> {:?}", goods_line(&text));
    }
}

/// Prints, for every reading of the card in `frame`, each line above the first label with its width per character
/// in heights of the line (`density`): a word the recognition lost leaves a wide gap.
fn show_name_density(bitmap: &Bitmap, frame: Area) {
    use versekit_core::refinery_scan::{Job, Ocr};
    use versekit_core::stock_scan::{CARD_PASSES, card_lines, is_label};
    let scale = |s: f64| (s * 1440.0 / bitmap.height as f64).clamp(1.0, 6.0);
    let inner = (frame.0 + 4, frame.1 + 4, frame.2 - 8, frame.3 - 8);
    let jobs: Vec<Job> = CARD_PASSES.iter().map(|(s, mode)| Job { area: inner, scale: scale(*s), mode: *mode }).collect();
    for words in Recognition::new(bitmap).read(&jobs).expect("read") {
        for (text, f) in card_lines(&words).iter().take_while(|(t, _)| !is_label(t)) {
            let chars = text.chars().filter(|c| !c.is_whitespace()).count().max(1) as f64;
            println!("   density {:.2} {text:?}", (f.2 - f.0) / (chars * (f.3 - f.1).max(1.0)));
        }
    }
}

/// How the readings of one kind of card came out.
#[derive(Default, Debug, Clone)]
struct Count {
    /// Sure, with the right name and quantity.
    right: usize,
    /// Sure, with a wrong name or quantity: the error that must not happen.
    wrong: usize,
    /// Asked, with the right name among the ones offered.
    asked_right: usize,
    /// Asked, without the right name.
    asked_wrong: usize,
    /// Nothing read.
    missed: usize,
}

impl Count {
    fn total(&self) -> usize {
        self.right + self.wrong + self.asked_right + self.asked_wrong + self.missed
    }
}

fn field<'a>(item: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    item.get(key).filter(|v| !v.is_null())
}

/// Whether a sure outcome has the quantity, unit and quality the list expects.
fn quantity_right(outcome: &CardOutcome, item: &serde_json::Value) -> bool {
    let amount = field(item, "menge").and_then(serde_json::Value::as_f64).unwrap_or(0.0);
    let unit = if field(item, "einheit").and_then(serde_json::Value::as_str) == Some("scu") {
        Unit::Scu
    } else {
        Unit::Pieces
    };
    let quality = field(item, "qualitaet").and_then(serde_json::Value::as_u64).map(|q| q as u32);
    (outcome.quantity - amount).abs() < 0.0005
        && outcome.unit == unit
        && (unit == Unit::Pieces || outcome.quality == quality)
}

#[test]
#[ignore = "reads test pictures of the game"]
fn every_card_gives_its_stock_or_asks() {
    let Some(folder) = std::env::var_os("VERSEKIT_STOCK_PICTURES") else { return };
    let folder = Path::new(&folder);
    let (names, texts) = names_and_texts();
    let list_path =
        std::env::var_os("VERSEKIT_STOCK_LIST").map_or_else(|| folder.join("lager-scan-erwartet.json"), PathBuf::from);
    let list: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&list_path).expect("the list of expected values")).expect("a list");
    let items = list.as_array().expect("a list");
    assert!(!items.is_empty(), "no pictures");
    let (_, wrong, _) = count_list(folder, items, &names, texts.as_ref(), true);
    assert!(wrong.is_empty(), "sure and wrong: {wrong:?}");
}

/// The sets of pictures of the game with their lists of expected values (in `tests/data/lager-scan`), and the least
/// share of their pictures each must read sure and right. The pictures themselves stay in the folder of screenshots
/// of the game.
/// The last number is the least share of the tiles of the kinds that matter (blueprint items, components, armour,
/// weapons of a person, by the names of the game) each must read sure and right: the measured one, 22.11.; the
/// aim of 95 % is reached at the inventory terminal at Levski only.
/// The sets of the freight elevator are read as the product reads them, those of the inventory terminal as the test
/// build does ([`read_card_where`]).
const SETS: [(&str, f64, bool, f64); 7] = [
    // The inventory terminal at Levski: weapons and components, the faint card with the lines of VerseKit.
    ("terminal-levski.json", 0.93, true, 0.95),
    // The freight elevator at Levski, a delivery from Argon: every tile `X1`.
    ("frachtaufzug-levski.json", 0.83, false, 0.85),
    // The freight elevator at Nyx: components, weapons, ammunition, ores.
    ("frachtaufzug-nyx.json", 0.66, false, 0.65),
    // The inventory terminal at Nyx: ores and goods in SCU, items in boxes and stacks.
    ("terminal-nyx.json", 0.6, true, 0.75),
    // The freight elevator at Levski again (22.11.): ship weapons and components, every tile `x1`.
    ("frachtaufzug-levski-2.json", 0.7, false, 0.73),
    // The freight elevator at Station Alpha (22.11.): the whole stock, items, ores and goods in boxes.
    ("frachtaufzug-alpha.json", 0.63, false, 0.62),
    // The freight elevator at Station Alpha with weapons and their attachments in stacks (`x2` to `x17`, 22.11.).
    ("frachtaufzug-alpha-stapel.json", 0.62, false, 0.66),
];

/// Reads every set of [`SETS`] in the screenshots of `VERSEKIT_STOCK_SCREENSHOTS` and counts it against its list:
/// no picture may be read sure and wrong, and each set must read its share sure and right.
#[test]
#[ignore = "reads test pictures of the game"]
fn every_set_holds_its_share() {
    let Some(folder) = std::env::var_os("VERSEKIT_STOCK_SCREENSHOTS") else { return };
    let folder = Path::new(&folder);
    let (names, texts) = names_and_texts();
    let lists = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("lager-scan");
    let mut failed = Vec::new();
    // With `VERSEKIT_STOCK_ONLY` only the sets whose file name contains it (a quick check of one set).
    let only = std::env::var("VERSEKIT_STOCK_ONLY").unwrap_or_default();
    for (file, least, inventory, least_matter) in SETS {
        if !file.contains(&only) {
            continue;
        }
        let list: serde_json::Value =
            serde_json::from_slice(&std::fs::read(lists.join(file)).expect("the list")).expect("a list");
        let items = list.as_array().expect("a list");
        let (counts, wrong, important) = count_list(folder, items, &names, texts.as_ref(), inventory);
        let matter = important.get("all").cloned().unwrap_or_default();
        let matter_share = matter.right as f64 / matter.total().max(1) as f64;
        println!(
            "SET {file}: kinds that matter {} of {} sure and right ({:.0} %)",
            matter.right,
            matter.total(),
            matter_share * 100.0
        );
        if matter_share < least_matter {
            failed.push(format!(
                "{file}: kinds that matter {:.0} % sure and right, at least {:.0} %",
                matter_share * 100.0,
                least_matter * 100.0
            ));
        }
        let right: usize = counts.values().map(|c| c.right).sum();
        let share = right as f64 / items.len().max(1) as f64;
        println!("SET {file}: {right} of {} sure and right ({:.0} %), sure and wrong {wrong:?}", items.len(), share * 100.0);
        if !wrong.is_empty() {
            failed.push(format!("{file}: sure and wrong {wrong:?}"));
        }
        if share < least {
            failed.push(format!("{file}: {:.0} % sure and right, at least {:.0} %", share * 100.0, least * 100.0));
        }
    }
    assert!(failed.is_empty(), "{failed:?}");
}

/// What the names of the game make of each line of `VERSEKIT_NAME_TRY` (lines split by `|`): the name read, how sure
/// and the candidates, as [`versekit_core::stock_scan::match_name`] gives them, and the reading of the line as a
/// whole card ([`versekit_core::stock_scan::read_lines`]).
#[test]
#[ignore = "reads the names of the game"]
fn names_of_lines() {
    use versekit_core::stock_scan::{match_name, read_lines};
    let Some(lines) = std::env::var_os("VERSEKIT_NAME_TRY") else { return };
    let (names, texts) = names_and_texts();
    for line in lines.to_string_lossy().split('|') {
        let r = match_name(line, &names);
        println!("{line:?}: {:?} {:?} {:?} {:?}", r.certainty, r.name, r.how, r.candidates);
        let card = read_lines(&[(line.to_string(), (0.0, 0.0, 300.0, 20.0))], &names, texts.as_ref());
        println!("  card: {:?} {:?}", card.reading.certainty, card.reading.name);
    }
}

/// Reads the sets of the inventory terminal of [`SETS`] as the product build reads them: until the reading there
/// has been accepted in the game, every picture gives nothing.
#[test]
#[ignore = "reads test pictures of the game"]
fn the_product_reads_nothing_at_the_inventory_terminal() {
    let Some(folder) = std::env::var_os("VERSEKIT_STOCK_SCREENSHOTS") else { return };
    let folder = Path::new(&folder);
    let (names, texts) = names_and_texts();
    let lists = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("lager-scan");
    let mut read = Vec::new();
    for file in ["terminal-levski.json", "terminal-nyx.json"] {
        let list: serde_json::Value =
            serde_json::from_slice(&std::fs::read(lists.join(file)).expect("the list")).expect("a list");
        for item in list.as_array().expect("a list") {
            let name = item["datei"].as_str().expect("datei");
            let pointer = (item["zeiger"][0].as_i64().expect("x"), item["zeiger"][1].as_i64().expect("y"));
            let bitmap = bitmap_of(&folder.join(name));
            let outcome = read_card_where(&bitmap, pointer, &mut Recognition::new(&bitmap), &names, texts.as_ref(), false)
                .expect("read");
            if outcome.certainty != Certainty::None {
                read.push(format!("{name}: {:?} {:?}", outcome.certainty, outcome.name));
            }
        }
    }
    assert!(read.is_empty(), "read at the inventory terminal: {read:?}");
}

/// Counts the outcome of one tile against what the list expects of it.
fn tally(count: &mut Count, expected: Option<&str>, outcome: &CardOutcome, item: &serde_json::Value) {
    // A card that shows a text key in place of a name: the key must come out; a sure name would be wrong.
    let key = field(item, "schluessel").and_then(serde_json::Value::as_str);
    match (expected, outcome.certainty) {
        (None, Certainty::Sure) => count.wrong += 1,
        (None, _) if key.is_some() && outcome.key.as_deref() != key => count.missed += 1,
        (None, _) => count.right += 1,
        (Some(name), Certainty::Sure) if outcome.name.as_deref() == Some(name) && quantity_right(outcome, item) => {
            count.right += 1;
        }
        (Some(_), Certainty::Sure) => count.wrong += 1,
        (Some(_), Certainty::None) => count.missed += 1,
        (Some(name), _) if outcome.candidates.iter().any(|c| c == name) => count.asked_right += 1,
        (Some(_), _) => count.asked_wrong += 1,
    }
}

/// Reads every picture of a list in `folder` and counts the outcome per group and kind; with the pictures read sure
/// and wrong. The inventory terminal is read where `inventory` ([`read_card_where`]).
fn count_list(
    folder: &Path,
    items: &[serde_json::Value],
    names: &Names,
    texts: Option<&GameTexts>,
    inventory: bool,
) -> (BTreeMap<String, Count>, Vec<String>, BTreeMap<String, Count>) {
    let mut counts: BTreeMap<String, Count> = BTreeMap::new();
    let mut important: BTreeMap<String, Count> = BTreeMap::new();
    let mut wrong = Vec::new();
    for item in items {
        let file = field(item, "datei").and_then(serde_json::Value::as_str).expect("datei");
        let xy = field(item, "zeiger").and_then(serde_json::Value::as_array).expect("zeiger");
        let pointer = (xy[0].as_i64().expect("x"), xy[1].as_i64().expect("y"));
        let expected = field(item, "name").and_then(serde_json::Value::as_str);
        let kind = field(item, "art").and_then(serde_json::Value::as_str).unwrap_or("keine");
        let language = field(item, "gruppe")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| file.split('/').next().unwrap_or(""));
        // A file may lie elsewhere: an absolute path is taken as it is.
        let bitmap = bitmap_of(&folder.join(file));
        let started = std::time::Instant::now();
        let outcome =
            read_card_where(&bitmap, pointer, &mut Recognition::new(&bitmap), names, texts, inventory).expect("read");
        println!(
            "== {file}: {:?} {:?} {} {:?} q{:?} key {:?} (expected {expected:?}) offered {:?} in {} ms frame {:?} \
             tile {:?} lines {:?}",
            outcome.certainty,
            outcome.name,
            outcome.quantity,
            outcome.unit,
            outcome.quality,
            outcome.key,
            outcome.candidates,
            started.elapsed().as_millis(),
            outcome.frame,
            outcome.tile_rect,
            outcome.lines.iter().take(4).collect::<Vec<_>>()
        );
        if std::env::var_os("VERSEKIT_STOCK_DEBUG").is_some() {
            show_what_was_read(&bitmap, pointer, names);
        }
        if let (Some(frame), true) = (outcome.frame, std::env::var_os("VERSEKIT_STOCK_GOODS").is_some()) {
            show_goods_passes(&bitmap, frame, names, texts);
        }
        if let (Some(frame), true) = (outcome.frame, std::env::var_os("VERSEKIT_STOCK_DENSITY").is_some()) {
            show_name_density(&bitmap, frame);
        }
        let count = counts.entry(format!("{language} {kind}")).or_default();
        let before = count.wrong;
        tally(count, expected, &outcome, item);
        if count.wrong > before {
            wrong.push(file.to_string());
        }
        // The tiles of the kinds that matter (blueprint items, components, armour, weapons of a person): the
        // expected name is one of the names of the game of those kinds.
        if let Some(group) = expected.and_then(|name| IMPORTANT.get().and_then(|all| all.get(&versekit_core::blueprint_scan::plain(name)))) {
            tally(important.entry(group.label().to_string()).or_default(), expected, &outcome, item);
            tally(important.entry("all".to_string()).or_default(), expected, &outcome, item);
        }
    }
    for (group, count) in &important {
        let share = 100.0 * count.right as f64 / count.total().max(1) as f64;
        println!(
            "KINDS THAT MATTER {group}: {} tiles, sure and right {} ({share:.0} %), asked with the right name {}, \
             asked without {}, nothing {}, sure and wrong {}",
            count.total(),
            count.right,
            count.asked_right,
            count.asked_wrong,
            count.missed,
            count.wrong
        );
    }
    for (kind, count) in &counts {
        let share = 100.0 * count.right as f64 / count.total().max(1) as f64;
        println!(
            "{kind}: {} pictures, sure and right {} ({share:.0} %), asked with the right name {}, asked without {}, \
             nothing {}, sure and wrong {}",
            count.total(),
            count.right,
            count.asked_right,
            count.asked_wrong,
            count.missed,
            count.wrong
        );
    }
    (counts, wrong, important)
}

/// A picture of 3840 × 2160 taken at a scaling of 150 % (`4k/ScreenShot-2026-11-26_21-01-36-083.jpg` in
/// `VERSEKIT_STOCK_PICTURES`): the card at the pointer 2240,1280 gives its name and the amount 1 for sure, also where
/// the pointer came in scaled points (1493,853) and is taken in physical ones as the scan does.
#[test]
#[ignore = "reads test pictures of the game"]
fn a_card_on_a_screen_of_4k_at_150_percent_is_read_at_the_pointer_in_physical_points() {
    let Some(folder) = std::env::var_os("VERSEKIT_STOCK_PICTURES") else { return };
    let file = Path::new(&folder).join("4k").join("ScreenShot-2026-11-26_21-01-36-083.jpg");
    let bitmap = bitmap_of(&file);
    assert_eq!((bitmap.width, bitmap.height), (3840, 2160));
    let (names, texts) = names_and_texts();
    let physical = versekit_core::screen_grab::physical_of((1493, 853), Some((2240, 1280)), Some(144));
    for pointer in [(2240, 1280), physical] {
        let at = (i64::from(pointer.0), i64::from(pointer.1));
        let outcome = read_card(&bitmap, at, &mut Recognition::new(&bitmap), &names, texts.as_ref()).expect("read");
        assert_eq!(
            (outcome.certainty, outcome.name.as_deref(), outcome.quantity, outcome.quantity_read),
            (Certainty::Sure, Some("CF-227 Badger Repeater"), 1.0, true),
            "{pointer:?}: {:?}",
            outcome.lines
        );
    }
}
