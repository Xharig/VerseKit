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

//! The tiles of the overview stand whole in their places, and the field of tiles scrolls below a head that stays.
//!
//! A tile is a box with a border and two corner brackets; its bottom edge, its rounding and the bracket at the
//! bottom right lie at the bottom of the box. When the box is taller than the place of its tile, the tile cuts it
//! off there and the box stands open at the bottom. The checks draw the overview in every size of the text, in
//! German and English, in windows 880, 1350 and 1800 wide, on the sample and on real data, and read the places
//! of tile and box ([`crate::tiles::drawn_tiles`]).

use egui::{Pos2, Rect};
use versekit_core::font_size;
use versekit_core::language::{self, Choice};

use super::SamplePage;
use super::real_data::{folder, host_in};
use crate::tiles::{self, TileSize, drawn_tiles::Drawn};

/// The widths of the window; where a size of the text needs a wider window, its smallest one.
const WIDTHS: [f32; 3] = [880.0, 1350.0, 1800.0];
/// Two places are the same when they differ by less than this (points).
const SAME: f32 = 0.5;

thread_local! {
    static CTX: egui::Context = {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx
    };
}

/// Draws one frame of `page` in a window of `size` with `events`; returns every text drawn with its place.
fn frame(page: &mut SamplePage, size: (f32, f32), events: Vec<egui::Event>) -> Vec<(String, Pos2)> {
    fn collect(shape: &egui::Shape, into: &mut Vec<(String, Pos2)>) {
        match shape {
            egui::Shape::Text(text) => into.push((text.galley.text().to_string(), text.pos)),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, into)),
            _ => {}
        }
    }
    CTX.with(|ctx| {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(size.0, size.1))),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            page.show(ui);
        });
        output.textures_delta.clear();
        let mut texts = Vec::new();
        output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
        texts
    })
}

/// Draws `page` until its layout has settled (the rows of the tiles follow their content a frame later, the
/// scroll bar another) and returns the tiles and texts of the last frame.
fn settled(page: &mut SamplePage, size: (f32, f32)) -> (Vec<Drawn>, Vec<(String, Pos2)>) {
    for _ in 0..8 {
        frame(page, size, Vec::new());
    }
    tiles::drawn_tiles::take();
    let texts = frame(page, size, Vec::new());
    (tiles::drawn_tiles::take(), texts)
}

/// The tiles whose box does not stand whole in the place of the tile (its bottom edge and the bracket at the
/// bottom right cut off), as text.
fn cut(tiles: &[Drawn]) -> Vec<String> {
    tiles
        .iter()
        .filter(|drawn| {
            let (tile, card) = (drawn.tile, drawn.card);
            card.min.x < tile.min.x - SAME
                || card.min.y < tile.min.y - SAME
                || card.max.x > tile.max.x + SAME
                || card.max.y > tile.max.y + SAME
        })
        .map(|drawn| {
            format!(
                "tile y {:.0}..{:.0}, its box y {:.0}..{:.0} (x {:.0}..{:.0} in {:.0}..{:.0})",
                drawn.tile.min.y,
                drawn.tile.max.y,
                drawn.card.min.y,
                drawn.card.max.y,
                drawn.card.min.x,
                drawn.card.max.x,
                drawn.tile.min.x,
                drawn.tile.max.x
            )
        })
        .collect()
}

/// A board of every size: a few pages are pinned (on real data the area of the cards of the user has tiles, and
/// the sample gets more tiles than the widest window shows at once), the tiles take the four sizes in turn and
/// one of the lowest board is hidden (the row of the hidden tiles shows below it, at the end of the page).
fn varied(page: &mut SamplePage) {
    for key in ["hf_meinlager", "hf_hangar", "hf_liste", "hf_bergbau", "hf_routen", "hf_laeden"] {
        if !page.tiles.iter().any(|tile| tile.key == key) {
            page.tiles.push(tiles::Tile::pinned(key));
        }
    }
    // With two areas the pinned pages stand below the other tiles unless they come first.
    let lowest_pinned = page.overview_two && !page.mine_first;
    let hide = page.tiles.iter().position(|tile| !page.overview_two || tile.pinned == lowest_pinned);
    for (at, tile) in page.tiles.iter_mut().enumerate() {
        tile.size = TileSize::ALL[at % TileSize::ALL.len()];
        tile.hidden = Some(at) == hide;
    }
}

/// The pages the checks draw: the sample, and real data in an empty folder (two areas, the tiles of 4.12).
fn pages(dir: &std::path::Path) -> Vec<(&'static str, SamplePage)> {
    let mut sample = SamplePage::preview();
    let mut real = SamplePage::with_settings(versekit_core::paths::Paths::new(host_in(dir, &[])));
    for page in [&mut sample, &mut real] {
        page.go_to("");
        varied(page);
    }
    vec![("sample", sample), ("real data", real)]
}

/// Every tile of the overview shows its box whole: bottom edge, rounding and the bracket at the bottom right lie
/// within the tile, in every size of the text, both languages and the three widths.
#[test]
fn every_tile_of_the_overview_shows_its_box_whole() {
    let _language = crate::language_lock();
    versekit_core::theme::set_current("original");
    let dir = folder("kachel-ganz");
    let mut findings = Vec::new();
    let mut whole = 0;
    for (choice, lang) in [(Choice::De, "de"), (Choice::En, "en")] {
        language::set_thread_language(choice);
        for (case, mut page) in pages(&dir) {
            for points in crate::test_sizes() {
                page.size = points;
                let (least, _) = font_size::min_window(points);
                for width in WIDTHS {
                    // Tall enough for every tile to show in full: what is cut is cut by the tile itself.
                    let size = (width.max(least), 6000.0);
                    let (drawn, _) = settled(&mut page, size);
                    assert!(drawn.len() > 3, "{lang} {case} {points} {width}: the board drew {} tiles", drawn.len());
                    whole += drawn.iter().filter(|d| (d.visible.intersect(d.tile)).height() >= d.tile.height()).count();
                    for finding in cut(&drawn) {
                        findings.push(format!("{lang} {case} size {points} width {width}: {finding}"));
                    }
                }
            }
        }
    }
    language::set_thread_language(Choice::De);
    let _ = std::fs::remove_dir_all(dir);
    assert!(whole > 0, "no tile was shown in full: the check saw nothing");
    assert!(
        findings.is_empty(),
        "{} tile(s) with a box cut off at the bottom:\n{}",
        findings.len(),
        findings[..findings.len().min(20)].join("\n")
    );
}

/// The check of the boxes tells a box cut off at the bottom from one that stands in its tile.
#[test]
fn the_check_finds_a_box_taller_than_its_tile() {
    let tile = Rect::from_min_max(egui::pos2(10.0, 100.0), egui::pos2(280.0, 210.0));
    let fits = Drawn { tile, visible: tile, card: tile };
    assert!(cut(&[fits]).is_empty());
    let taller = Drawn { card: Rect::from_min_max(tile.min, egui::pos2(280.0, 292.0)), ..fits };
    assert_eq!(cut(&[taller]).len(), 1);
    let wider = Drawn { card: Rect::from_min_max(tile.min, egui::pos2(282.0, 210.0)), ..fits };
    assert_eq!(cut(&[wider]).len(), 1);
}
