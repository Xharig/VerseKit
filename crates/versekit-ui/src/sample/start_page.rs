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

//! The start page on real data: the tiles of 4.12 "at a glance" and the pinned
//! pages, laid out as the settings keep them, and every change written back
//! under the keys of 4.12 (the size and the single area under keys of v5).

use versekit_core::jsonio::Value;
use versekit_core::overview;
use versekit_core::paths::Paths;

use super::SamplePage;
use crate::tiles::{Tile, TileSize};

/// What the settings held that the board does not show: the whole order of
/// the tiles, the hidden ones, the pinned pages (some may be hidden pages) and
/// the sizes of tiles not on the board.
#[derive(Clone, Debug, Default)]
pub(super) struct Kept {
    order: Vec<String>,
    hidden: Vec<String>,
    pinned: Vec<String>,
    /// The pinned pages whose page was hidden at the last look.
    away: Vec<String>,
    sizes: Vec<(String, String)>,
}

/// The name of a size in the settings.
pub(super) fn size_name(size: TileSize) -> &'static str {
    match size {
        TileSize::Small => "klein",
        TileSize::Wide => "breit",
        TileSize::Tall => "hoch",
        TileSize::Large => "gross",
    }
}

fn size_of(name: &str) -> Option<TileSize> {
    TileSize::ALL.into_iter().find(|size| size_name(*size) == name)
}

/// The icon of a tile of 4.12.
pub(super) fn tile_icon(tile: &str) -> &'static str {
    match tile {
        "spiel" => "gamepad-2",
        "standort" => "plane-landing",
        "bauplaene" => "clipboard-list",
        "uebersetzung" => "languages",
        "spielzeit" => "chart-spline",
        "rechner" => "monitor",
        "dienste" => "server",
        "raffinerie" => "factory",
        "lager" => "package",
        "auftraege" => "notebook-pen",
        "neu_patch" => "sparkles",
        "merkliste" => "star",
        "bedarf" => "heart-handshake",
        "funde" => "scan-eye",
        "notizen" => "sticky-note",
        "executive_hangar" => "timer",
        _ => "archive-restore",
    }
}

impl SamplePage {
    /// Whether a tile of 4.12 is there now (`overview.available`): the refinery
    /// only where the terminal can be read, the unit demand only while its page is.
    pub(super) fn tile_available(&self, tile: &str) -> bool {
        match tile {
            "raffinerie" => cfg!(windows),
            "bedarf" => !self.page_hidden("hf_bedarf"),
            _ => true,
        }
    }

    /// Lays the start page out as the settings keep it.
    pub(super) fn load_start_page(&mut self, paths: &Paths) {
        let order = overview::tile_order(paths);
        let visible = overview::visible_tiles(paths);
        let sizes = overview::sizes(paths);
        let size = |key: &str| {
            sizes.iter().find(|(id, _)| id == key).and_then(|(_, name)| size_of(name)).unwrap_or(TileSize::Small)
        };
        let mut tiles: Vec<Tile> = order
            .iter()
            .filter(|id| self.tile_available(id))
            .map(|id| Tile { key: id.clone(), size: size(id), hidden: !visible.contains(id), pinned: false })
            .collect();
        let pinned = overview::pinned(paths);
        let mut away = Vec::new();
        for page in &pinned {
            let Some(key) = overview::page_key(page) else { continue };
            if self.page_hidden(key) {
                away.push(page.clone());
            } else {
                tiles.push(Tile { size: size(&page_size_id(page)), ..Tile::pinned(key) });
            }
        }
        self.tiles = tiles;
        self.overview_two = !paths.setting_bool(overview::ONE_AREA_SETTING, false);
        self.mine_first = overview::cards_first(paths);
        self.start = Some(Kept { order, hidden: overview::hidden_tiles(paths), pinned, away, sizes });
    }

    /// The pinned pages now (`cards.pinned`): the cards on the board, and the pinned pages whose
    /// page is hidden at their places; a page pinned anew comes at the end (`cards.set_pinned`).
    fn pinned_now(&self, kept: &Kept) -> Vec<String> {
        let shown: Vec<String> = self
            .tiles
            .iter()
            .filter(|t| t.pinned)
            .filter_map(|t| overview::page_name(&t.key).map(str::to_string))
            .collect();
        let hidden = |page: &String| overview::page_key(page).is_some_and(|key| self.page_hidden(key));
        let still: Vec<String> =
            kept.pinned.iter().filter(|p| shown.contains(p) || hidden(p) || kept.away.contains(p)).cloned().collect();
        let in_place: Vec<String> = shown.iter().filter(|p| still.contains(p)).cloned().collect();
        let mut pinned = overview::merged_order(&still, &in_place);
        pinned.extend(shown.iter().filter(|p| !still.contains(p)).cloned());
        pinned
    }

    /// A tile or a pinned page that came or went with its condition (the unit demand and the unit
    /// storage with the connection) joins the board at its place or leaves it.
    pub(super) fn follow_available_tiles(&mut self) {
        let Some(kept) = self.start.clone() else { return };
        let pinned = self.pinned_now(&kept);
        let mut away = Vec::new();
        for page in &pinned {
            let Some(key) = overview::page_key(page) else { continue };
            let on_board = self.tiles.iter().position(|t| t.pinned && t.key == key);
            match (self.page_hidden(key), on_board) {
                (true, Some(i)) => {
                    let tile = self.tiles.remove(i);
                    if let Some(kept) = &mut self.start {
                        kept.sizes.retain(|(k, _)| *k != page_size_id(page));
                        kept.sizes.push((page_size_id(page), size_name(tile.size).to_string()));
                    }
                    away.push(page.clone());
                }
                (true, None) => away.push(page.clone()),
                (false, None) => {
                    let size = kept
                        .sizes
                        .iter()
                        .find(|(k, _)| *k == page_size_id(page))
                        .and_then(|(_, n)| size_of(n))
                        .unwrap_or(TileSize::Small);
                    self.tiles.push(Tile { size, ..Tile::pinned(key) });
                }
                (false, Some(_)) => {}
            }
        }
        if let Some(kept) = &mut self.start {
            kept.pinned = pinned;
            kept.away = away;
        }
        let Some(kept) = self.start.clone() else { return };
        let order = self.tile_order_now(&kept);
        for (id, _) in overview::TILES {
            let shown = self.tiles.iter().position(|t| !t.pinned && t.key == id);
            match (self.tile_available(id), shown) {
                (true, None) => {
                    // Before the first shown tile that follows it in the order, else after the last tile.
                    let after: Vec<&String> = order.iter().skip_while(|o| *o != id).skip(1).collect();
                    let at = self
                        .tiles
                        .iter()
                        .position(|t| !t.pinned && after.contains(&&t.key))
                        .unwrap_or_else(|| self.tiles.iter().filter(|t| !t.pinned).count());
                    let size = kept
                        .sizes
                        .iter()
                        .find(|(k, _)| k == id)
                        .and_then(|(_, n)| size_of(n))
                        .unwrap_or(TileSize::Small);
                    let hidden = kept.hidden.iter().any(|h| h == id);
                    self.tiles.insert(at, Tile { key: id.to_string(), size, hidden, pinned: false });
                }
                (false, Some(i)) => {
                    let tile = self.tiles.remove(i);
                    if let Some(kept) = &mut self.start {
                        kept.order = order.clone();
                        kept.hidden.retain(|h| h != id);
                        if tile.hidden {
                            kept.hidden.push(tile.key.clone());
                        }
                        kept.sizes.retain(|(k, _)| k != id);
                        kept.sizes.push((tile.key, size_name(tile.size).to_string()));
                    }
                }
                _ => {}
            }
        }
    }

    /// The whole order of the tiles now: the board's sequence at the places of the tiles on it.
    fn tile_order_now(&self, kept: &Kept) -> Vec<String> {
        let shown: Vec<String> = self.tiles.iter().filter(|t| !t.pinned).map(|t| t.key.clone()).collect();
        overview::merged_order(&kept.order, &shown)
    }

    /// The settings of the start page as they stand now (nothing before the
    /// settings were read).
    pub(super) fn start_page_values(&self) -> Vec<(&'static str, Value)> {
        let Some(kept) = &self.start else { return Vec::new() };
        let hidden: Vec<String> = overview::TILES
            .iter()
            .map(|(id, _)| id.to_string())
            .filter(|id| match self.tiles.iter().find(|t| !t.pinned && t.key == *id) {
                Some(tile) => tile.hidden,
                None => kept.hidden.contains(id),
            })
            .collect();
        let pinned = self.pinned_now(kept);
        let mut sizes: Vec<(String, String)> =
            kept.sizes.iter().filter(|(id, _)| !self.tiles.iter().any(|t| tile_id(t) == *id)).cloned().collect();
        sizes.extend(self.tiles.iter().map(|t| (tile_id(t), size_name(t.size).to_string())));
        sizes.retain(|(_, name)| name != size_name(TileSize::Small));
        vec![
            (overview::ORDER_SETTING, overview::list_value(&self.tile_order_now(kept))),
            (overview::HIDDEN_SETTING, overview::list_value(&hidden)),
            (overview::PINNED_SETTING, overview::list_value(&pinned)),
            (overview::FIRST_SETTING, overview::first_value(self.mine_first)),
            (overview::SIZE_SETTING, overview::sizes_value(&sizes)),
            (overview::ONE_AREA_SETTING, Value::Bool(!self.overview_two)),
        ]
    }
}

/// The id a tile's size is kept under: the tile of 4.12, or `seite:` and the page name of a pinned
/// page (a tile and a page may share a name, as `uebersetzung`).
fn tile_id(tile: &Tile) -> String {
    if tile.pinned { page_size_id(overview::page_name(&tile.key).unwrap_or(&tile.key)) } else { tile.key.clone() }
}

/// The id the size of a pinned page is kept under.
fn page_size_id(page: &str) -> String {
    format!("seite:{page}")
}
