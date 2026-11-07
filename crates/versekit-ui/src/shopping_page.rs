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

//! The page "Shopping list" (`pages._shopping_list` of 4.12): everything the
//! ships still lack, as a bill over all ships and the wish list, with the sum,
//! the crafting time and the shopping route.
//!
//! ⚠ Only what belongs to ships stands here; what is planned to be built
//! without a ship stands on the farm list with its material. A ship stands on
//! the bill only where one does not have it yet. The way (buy or build) is
//! chosen on the card of the ship; here it is shown, and the tick says that a
//! part is fitted. The game does not write what sits in a ship, so nothing is
//! guessed.
//!
//! ⚠ The slots of a ship come with the equipment of the ships (P6). Until then
//! the bill says which ships it could not count ("no slot data") instead of
//! claiming that nothing is to be got.
//!
//! New in v5: the parts put in from the shops (`basket.rs`) stand here too,
//! with how many, every shop that sells them, and a tick for "bought"; the
//! sum and the route take them with the parts of the ships. Templates keep
//! parts with how many under a name: made empty or from the parts from the
//! shops, unfolded to change their parts (found with the search of the shops
//! page) and put on the list again. Every stop of the route is ticked off on
//! the way.

use std::collections::{HashMap, HashSet};
use std::time::SystemTime;

use egui::{RichText, Ui};
use versekit_core::basket::{self, Basket};
use versekit_core::cart::{self, Condition, Ingredient, Kind, Line, Offer, Origin, Recipe, Slot, Sources, Step, Stop};
use versekit_core::erkul_slots::ErkulData;
use versekit_core::hangar;
use versekit_core::jsonio::{self, Map, Value};
use versekit_core::language::{self, Arg, t, tf};
use versekit_core::part_choices::ShopPart;
use versekit_core::paths::Paths;
use versekit_core::raw_prices::RawPrices;
use versekit_core::ship_market::{self, Market};
use versekit_core::shops::{self, Catalog, CatalogItem, Shops};
use versekit_core::stock_fetch;
use versekit_core::theme::Scheme;
use versekit_gamedata::cart_data::CartData;

use crate::color;
use crate::crafting_page::{CraftingState, duration};
use crate::farm_page::row_button;
use crate::trade::{self, Job, Polled};
use crate::widgets::{self, ButtonKind};

const LOOK_SECONDS: f64 = 1.0;
/// So long a part whose shop prices could not be looked up is left alone.
const RETRY_SECONDS: f64 = 120.0;
/// The width of the position column, so that the names stand in a line.
const POSITION_WIDTH: f32 = 130.0;
const FLEET_FILE: &str = "hangar.json";
/// The setting that holds the order of the stops the player chose.
const ORDER_SETTING: &str = "route_reihenfolge";

fn file_time(file: &str) -> Option<SystemTime> {
    std::fs::metadata(file).and_then(|m| m.modified()).ok()
}

/// The stores the bill reads besides the game values.
pub(crate) struct Stores {
    pub(crate) shops: Shops,
    pub(crate) raw: RawPrices,
    pub(crate) market: Market,
    /// The slots of the ships and the parts of erkul.
    pub(crate) erkul: ErkulData,
    /// What UEX sells (the catalog of the shops page), for the choice of parts.
    pub(crate) catalog: Vec<ShopPart>,
    /// The same parts as the shops page lists them, for the search inside a template.
    pub(crate) parts: Vec<CatalogItem>,
    seen: [Option<SystemTime>; 8],
}

impl Stores {
    fn files(paths: &Paths) -> [String; 8] {
        let [game_ships, game_parts, ships, parts] = ErkulData::files(paths);
        [
            paths.app_file(shops::CACHE),
            paths.app_file(stock_fetch::PRICES_FILE),
            paths.app_file(ship_market::CACHE),
            ships,
            parts,
            paths.app_file(shops::CATALOG_CACHE),
            game_ships,
            game_parts,
        ]
    }

    /// Nothing loaded: the sample.
    pub(crate) fn empty() -> Stores {
        Stores {
            shops: Shops::default(),
            raw: RawPrices::default(),
            market: Market::default(),
            erkul: ErkulData::default(),
            catalog: Vec::new(),
            parts: Vec::new(),
            seen: [None; 8],
        }
    }

    pub(crate) fn load(paths: &Paths) -> Stores {
        let files = Stores::files(paths);
        let parts = Catalog::load(paths).items();
        let catalog = parts
            .iter()
            .map(|p| ShopPart {
                code: p.code.clone(),
                name: p.name.clone(),
                group: p.group.clone(),
                maker: p.maker.clone(),
                size: p.size.clone(),
                class: p.class.clone(),
                grade: p.grade.clone(),
            })
            .collect();
        Stores {
            shops: Shops::load(paths),
            raw: RawPrices::load(paths),
            market: Market::load(paths),
            erkul: ErkulData::load(paths),
            catalog,
            parts,
            seen: std::array::from_fn(|k| file_time(&files[k])),
        }
    }

    pub(crate) fn changed(&self, paths: &Paths) -> bool {
        let files = Stores::files(paths);
        (0..files.len()).any(|k| file_time(&files[k]) != self.seen[k])
    }
}

/// What the page holds besides the stores: the fleet file, the lookup that
/// runs and the order of the stops.
struct View {
    fleet: Map,
    fleet_seen: Option<SystemTime>,
    looked_at: f64,
    /// A write of the fleet file failed.
    save_failed: bool,
    /// The lookup of shop prices.
    lookup: PriceLookup,
    /// The order of the stops the player chose, as `stop_key`s.
    order: Vec<String>,
    /// The parts put in from the shops, the templates and the stops ticked off.
    basket: Basket,
    basket_seen: Option<SystemTime>,
    /// A write of the basket failed.
    basket_failed: bool,
    /// The name typed for a template.
    template_name: String,
    /// What the last action on the templates did.
    template_said: Option<String>,
    /// Which templates are unfolded, their searches, a rename and a deletion to take back.
    templates: TemplateView,
}

impl View {
    /// Changes the basket and writes it: read again first, so that a part put in on the shops page meanwhile
    /// stays. Nothing is written where `change` changed nothing.
    fn edit_basket(&mut self, paths: Option<&Paths>, change: impl FnOnce(&mut Basket) -> bool) {
        let Some(paths) = paths else {
            change(&mut self.basket);
            return;
        };
        let mut held = basket::load(paths);
        if !change(&mut held) {
            return;
        }
        match basket::save(paths, &held) {
            Ok(()) => self.basket_failed = false,
            Err(error) => {
                versekit_core::errors::record("shopping.basket", &error);
                self.basket_failed = true;
            }
        }
        self.basket = held;
        self.basket_seen = file_time(&paths.app_file(basket::FILE));
    }
}

/// The lookup of the shop prices a bill still lacks, in the background and one part after the other: a bill
/// with twelve parts is twelve rounds of the net. The shopping list and the boxes "Loadout & cart" of the
/// hangar both run one.
#[derive(Default)]
pub(crate) struct PriceLookup {
    /// The lookup that runs.
    job: Option<Job<bool>>,
    /// The identifiers asked for and when: asked again only after a pause, so that a part UEX
    /// could not be asked about (no net) is tried again later and not at every frame.
    asked: HashMap<String, f64>,
    /// A lookup ended: the stores are read again.
    stale: bool,
}

impl PriceLookup {
    /// Takes in a lookup that ended.
    pub(crate) fn poll(&mut self, ui: &Ui) {
        let Some(job) = &self.job else { return };
        match job.poll() {
            Polled::Running => {}
            Polled::Done(_) | Polled::Lost => {
                self.job = None;
                self.stale = true;
                ui.ctx().request_repaint();
            }
        }
    }

    /// Whether a lookup runs (for the tests of the pages that hold one).
    #[cfg(test)]
    pub(crate) fn job(&self) -> Option<&Job<bool>> {
        self.job.as_ref()
    }

    /// Looks up the shop prices the lines still lack.
    pub(crate) fn look_up(&mut self, ui: &Ui, paths: Option<&Paths>, lines: &[Line]) {
        self.ask(ui, paths, cart::missing_prices(lines));
    }

    /// Looks up the given parts, `(code, name)`, that no shop price is known for.
    pub(crate) fn ask(&mut self, ui: &Ui, paths: Option<&Paths>, wanted: Vec<(String, String)>) {
        self.poll(ui);
        let Some(paths) = paths else { return };
        if self.job.is_some() {
            return;
        }
        let now = trade::now();
        let missing: Vec<(String, String)> = wanted
            .into_iter()
            .filter(|(code, _)| self.asked.get(code).is_none_or(|at| now - at > RETRY_SECONDS))
            .collect();
        if missing.is_empty() {
            return;
        }
        for (code, _) in &missing {
            self.asked.insert(code.clone(), now);
        }
        let paths = trade::job_paths(paths);
        self.job = Some(Job::spawn(ui.ctx(), "cart-shops", move || {
            let mut book = Shops::load(&paths);
            let mut catalog = Catalog::load(&paths);
            let mut stored = false;
            for (code, name) in &missing {
                if book.known(code) {
                    continue;
                }
                stored |= shops::fetch_part(
                    &paths,
                    &mut book,
                    &mut catalog,
                    (code, name),
                    false,
                    trade::now(),
                    &trade::fetcher(),
                );
            }
            stored
        }));
    }
}

/// The page "Shopping list".
pub struct ShoppingState {
    paths: Option<Paths>,
    stores: Stores,
    view: View,
}

/// The sample: a made-up fleet with made-up shops, in a window without a data folder.
const SAMPLE_FLEET: &str = r#"{"format": 1,
  "schiffe": [{"name": "Cutlass Black", "hersteller": "Drake", "belegung": {
      "hardpoint_cooler_01": {"ref": "sample-cooler", "name": "Frostline Cooler", "weg": "kaufen"},
      "hardpoint_shield_01": {"ref": "sample-shield", "name": "Arc Shield", "weg": "bauen"},
      "hardpoint_quantum_drive": {"ref": "sample-drive", "name": "Quantum Drive Mk2", "weg": "kaufen", "erledigt": true},
      "hardpoint_radar_01": {"ref": "factory-radar", "name": "Stock Radar", "weg": "kaufen"}}}],
  "wunsch": [{"name": "Carrack", "hersteller": "Anvil", "belegung": {
      "hardpoint_cooler_01": {"ref": "sample-cooler", "name": "Frostline Cooler", "weg": "kaufen"}}}]}"#;

fn parse(text: &str) -> Map {
    match jsonio::loads(text) {
        Ok(Value::Object(map)) => map,
        _ => hangar::empty(),
    }
}

impl Default for ShoppingState {
    fn default() -> Self {
        ShoppingState {
            paths: None,
            stores: Stores { parts: sample_parts(), ..Stores::empty() },
            view: View {
                fleet: parse(SAMPLE_FLEET),
                fleet_seen: None,
                looked_at: f64::NEG_INFINITY,
                save_failed: false,
                lookup: PriceLookup::default(),
                order: Vec::new(),
                basket: Basket::default(),
                basket_seen: None,
                basket_failed: false,
                template_name: String::new(),
                template_said: None,
                templates: TemplateView::default(),
            },
        }
    }
}

/// The order of the stops saved in the settings.
pub(crate) fn saved_order(paths: &Paths) -> Vec<String> {
    match paths.settings().iter().find(|(k, _)| k == ORDER_SETTING) {
        Some((_, Value::Array(keys))) => keys.iter().filter_map(|k| k.as_str().map(str::to_string)).collect(),
        _ => Vec::new(),
    }
}

impl ShoppingState {
    /// With own data: the fleet file and the stores of the data folder.
    pub fn with_files(paths: &Paths) -> ShoppingState {
        let mut state = ShoppingState {
            paths: Some(Paths::new(paths.host.clone())),
            stores: Stores::load(paths),
            ..ShoppingState::default()
        };
        state.view.fleet = hangar::empty();
        state.view.order = saved_order(paths);
        state.reload_fleet();
        state.reload_basket();
        state
    }

    fn reload_basket(&mut self) {
        let Some(paths) = &self.paths else { return };
        self.view.basket_seen = file_time(&paths.app_file(basket::FILE));
        self.view.basket = basket::load(paths);
    }

    fn reload_fleet(&mut self) {
        let Some(paths) = &self.paths else { return };
        let file = paths.app_file(FLEET_FILE);
        self.view.fleet_seen = file_time(&file);
        self.view.fleet = hangar::read(&file).unwrap_or_else(hangar::empty);
    }

    /// Reads what changed on disk again.
    fn follow(&mut self, ui: &Ui) {
        let now = ui.input(|i| i.time);
        if !self.view.lookup.stale && now - self.view.looked_at < LOOK_SECONDS {
            return;
        }
        self.view.looked_at = now;
        let Some(paths) = &self.paths else { return };
        // The hangar keeps the order of the stops too: a move there shows here.
        self.view.order = saved_order(paths);
        let fleet_changed = file_time(&paths.app_file(FLEET_FILE)) != self.view.fleet_seen;
        // A part put in on the shops page shows here.
        let basket_changed = file_time(&paths.app_file(basket::FILE)) != self.view.basket_seen;
        let stores = (self.view.lookup.stale || self.stores.changed(paths)).then(|| Stores::load(paths));
        if fleet_changed {
            self.reload_fleet();
        }
        if basket_changed {
            self.reload_basket();
        }
        if let Some(stores) = stores {
            self.view.lookup.stale = false;
            self.stores = stores;
        }
    }
}

impl View {
    /// Changes the fleet file and writes it: reads it again first, so that
    /// another place's change stays. `change` says whether it changed
    /// something; nothing is written where it did not, so that a file that
    /// could not be read is never replaced by an empty one.
    fn edit(&mut self, paths: Option<&Paths>, change: impl FnOnce(&mut Map) -> bool) {
        let Some(paths) = paths else {
            change(&mut self.fleet);
            return;
        };
        let file = paths.app_file(FLEET_FILE);
        let _lock = hangar::lock();
        let mut held = hangar::load_held(&file);
        if !change(&mut held.data) {
            return;
        }
        let saved = hangar::save_held(&file, &mut held);
        self.save_failed = !saved.ok();
        // After a merge what is held is older than the file: the page shows the file.
        self.fleet = if saved == hangar::Saved::Merged { hangar::read(&file).unwrap_or(held.data) } else { held.data };
        self.fleet_seen = file_time(&file);
    }
}

// ------------------------------------------------------------ the sample

const SAMPLE_COOLER: &str = "sample-cooler";
const SAMPLE_SHIELD: &str = "sample-shield";
const SAMPLE_DRIVE: &str = "sample-drive";
const SAMPLE_RADAR: &str = "sample-radar";

fn sample_part(code: &str, name: &str, section: &str, group: &str, maker: &str) -> CatalogItem {
    CatalogItem {
        name: name.into(),
        code: code.into(),
        section: section.into(),
        group: group.into(),
        maker: maker.into(),
        size: String::new(),
        class: String::new(),
        grade: String::new(),
        places: Vec::new(),
    }
}

/// The made-up catalog of the sample, for the search inside a template.
fn sample_parts() -> Vec<CatalogItem> {
    vec![
        sample_part(SAMPLE_COOLER, "Frostline Cooler", "Systems", "Coolers", "Wen"),
        sample_part(SAMPLE_SHIELD, "Arc Shield", "Systems", "Shield Generators", "Gorgon"),
        sample_part(SAMPLE_DRIVE, "Quantum Drive Mk2", "Propulsion", "Quantum Drives", "Wei-Tek"),
        sample_part(SAMPLE_RADAR, "Radar X", "Avionics", "Radar", "Chimera"),
    ]
}

/// The made-up world of the sample: shops, one recipe, raw prices, a ship price and slots.
struct Sample;

fn offer(shop: &str, place: &str, price: f64) -> Offer {
    Offer { shop: shop.into(), place: place.into(), system: "Stanton".into(), price }
}

fn sample_slot(path: &str, kind: &str, size: i64, factory: &str) -> Slot {
    Slot {
        path: path.into(),
        kind: kind.into(),
        size: Some(size),
        factory_ref: factory.into(),
        factory_name: String::new(),
    }
}

impl Sources for Sample {
    fn shop_known(&self, reference: &str) -> bool {
        matches!(reference, SAMPLE_COOLER | SAMPLE_SHIELD | SAMPLE_DRIVE | SAMPLE_RADAR)
    }

    fn cheapest(&self, reference: &str) -> Option<(f64, String, String)> {
        let mut best: Option<Offer> = None;
        for row in self.shops_for(reference) {
            if best.as_ref().is_none_or(|b| row.price < b.price) {
                best = Some(row);
            }
        }
        best.map(|o| (o.price, o.shop, o.place))
    }

    fn shops_for(&self, reference: &str) -> Vec<Offer> {
        match reference {
            SAMPLE_COOLER => {
                vec![offer("Dumper's Depot", "Area18", 22_700.0), offer("Cubby Blast", "Lorville", 23_100.0)]
            }
            SAMPLE_SHIELD => {
                vec![offer("Centermass", "Lorville", 12_500.0), offer("Teasa Spaceport", "Lorville", 13_100.0)]
            }
            SAMPLE_RADAR => vec![offer("Tammany and Sons", "Port Tressler", 8_400.0)],
            _ => Vec::new(),
        }
    }

    fn raw_buy_price(&self, material: &str) -> f64 {
        match material {
            "Iron" => 10.0,
            "Gold" => 25.0,
            _ => 0.0,
        }
    }

    fn recipe_for(&self, reference: &str, _name: &str) -> Option<Recipe> {
        let ingredient = |material: &str, amount: f64| Ingredient { material: material.into(), amount };
        (reference == SAMPLE_SHIELD).then(|| Recipe {
            blueprint: "Arc Shield".into(),
            steps: vec![Step {
                seconds: 420,
                ingredients: vec![ingredient("Iron", 3.0), ingredient("Gold", 1.5), ingredient("Quantainium", 0.5)],
            }],
        })
    }

    fn ship_offers(&self, name: &str) -> Vec<Offer> {
        if name == "Carrack" { vec![offer("Pad A", "Area18", 4_600_000.0)] } else { Vec::new() }
    }

    fn slots(&self, ship: &Map, _chosen: &Map) -> Vec<Slot> {
        match hangar::field_text(ship, "name") {
            "Cutlass Black" | "Carrack" => vec![
                sample_slot("hardpoint_cooler_01", "Cooler", 2, "factory-cooler"),
                sample_slot("hardpoint_shield_01", "Shield", 1, "factory-shield"),
                sample_slot("hardpoint_quantum_drive", "Quantum Drive", 1, "factory-drive"),
                sample_slot("hardpoint_radar_01", "Radar", 1, "factory-radar"),
            ],
            _ => Vec::new(),
        }
    }
}

// ------------------------------------------------------------ drawing

pub(crate) fn sub(scheme: &Scheme, text: impl Into<String>) -> RichText {
    RichText::new(text.into()).small().color(color(scheme.sub))
}

pub(crate) fn auec(value: f64) -> String {
    trade::auec(language::current(), value)
}

pub(crate) fn plain(value: f64) -> String {
    trade::plain(language::current(), value)
}

/// The amount of a line and the tone it stands in.
pub(crate) fn amount_of(scheme: &Scheme, line: &Line) -> (String, versekit_core::theme::Color) {
    if line.done {
        return (t("s_ek_erledigt"), scheme.accent);
    }
    let buying = line.way == cart::BUY;
    let (condition, single) =
        if buying { (line.buy.condition, line.buy.price) } else { (line.craft.condition, line.craft.material) };
    match (condition, single) {
        (Condition::Known, Some(single)) => {
            let text = if line.count <= 1 {
                auec(single)
            } else {
                tf(
                    "s_wk_route_je",
                    &[("summe", Arg::from(auec(single * line.count as f64))), ("preis", Arg::from(auec(single)))],
                )
            };
            (text, scheme.fg)
        }
        (Condition::NotChecked, _) => (t("s_wk_nicht_geprueft"), scheme.sub),
        _ => (t("s_ek_kein_betrag"), scheme.gold),
    }
}

/// The width a text takes in one line.
pub(crate) fn text_width(ui: &Ui, text: &str, style: egui::TextStyle, bold: bool) -> f32 {
    let font = style.resolve(ui.style());
    let font = if bold { crate::fonts::bold(font.size) } else { font };
    ui.painter().layout_no_wrap(text.to_string(), font, egui::Color32::WHITE).size().x
}

/// A line with something at the left and something at the right edge. Where both do not fit side by side
/// (`left_width` and `right_width` as measured), the right one goes under the left one: a text that grows
/// leftwards from the edge would lie on the end of the one before it.
pub(crate) fn spread(
    ui: &mut Ui,
    left_width: f32,
    right_width: f32,
    indent: f32,
    left: impl FnOnce(&mut Ui),
    right: impl FnOnce(&mut Ui),
) {
    let gap = ui.spacing().item_spacing.x;
    if left_width + right_width + 3.0 * gap <= ui.available_width() {
        ui.horizontal_wrapped(|row| {
            left(row);
            row.with_layout(egui::Layout::right_to_left(egui::Align::Center), right);
        });
    } else {
        ui.horizontal_wrapped(left);
        ui.horizontal_wrapped(|row| {
            row.add_space(indent);
            right(row);
        });
    }
}

/// A cell of a fixed width at the left, so that the columns of the rows stand in a line.
pub(crate) fn cell(row: &mut Ui, width: f32, text: RichText) {
    row.allocate_ui_with_layout(
        egui::vec2(width, row.spacing().interact_size.y),
        egui::Layout::left_to_right(egui::Align::Center),
        |inner| {
            inner.add(egui::Label::new(text).wrap());
        },
    );
}

/// One position of the bill: the tick, where on the ship, the way, what, and at the right the amount.
fn row(ui: &mut Ui, scheme: &Scheme, k: usize, line: &Line, ticks: &mut Vec<(Line, bool)>) {
    let frame = egui::Frame::new()
        .fill(if k % 2 == 1 { color(scheme.bar) } else { egui::Color32::TRANSPARENT })
        .inner_margin(egui::Margin::symmetric(6, 3));
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        let gap = ui.spacing().item_spacing.x;
        let (amount, amount_tone) = amount_of(scheme, line);
        let way = t(if line.way == cart::BUY { "s_wk_kaufen" } else { "s_wk_bauen" });
        // The column is as wide as the wider of the two words, in this language.
        let way_width = [t("s_wk_kaufen"), t("s_wk_bauen")]
            .iter()
            .map(|word| text_width(ui, word, egui::TextStyle::Small, false))
            .fold(0.0, f32::max);
        let name = if line.count > 1 { format!("{} × {}", line.count, line.name) } else { line.name.clone() };
        let left_width = widgets::ICON_BUTTON_SIZE
            + POSITION_WIDTH
            + way_width
            + text_width(ui, &name, egui::TextStyle::Body, false)
            + 4.0 * gap;
        let right_width = text_width(ui, &amount, egui::TextStyle::Small, false);
        spread(
            ui,
            left_width,
            right_width,
            widgets::ICON_BUTTON_SIZE + gap,
            |row| {
                if line.kind == Kind::Part {
                    // The tick says "fitted": for both ways alike, since the tool cannot see what sits in the ship.
                    let (icon, meaning) = if line.done {
                        ("circle-check", t("s_p9_ek_haken_weg"))
                    } else {
                        ("circle", t("s_p9_ek_abhaken"))
                    };
                    let who = format!("{}, {}, {}", line.ship, line.position, line.name);
                    if row_button(row, scheme, icon, &meaning, &who, true) {
                        ticks.push((line.clone(), !line.done));
                    }
                } else {
                    // The ship has no tick; its place stays empty so that the columns keep their lines.
                    row.allocate_exact_size(egui::Vec2::splat(widgets::ICON_BUTTON_SIZE), egui::Sense::hover());
                }
                let position = if line.kind == Kind::Ship { t("s_ek_das_schiff") } else { line.position.clone() };
                cell(row, POSITION_WIDTH, sub(scheme, position));
                cell(row, way_width, sub(scheme, way));
                let tone = if line.done { scheme.sub } else { scheme.fg };
                row.label(RichText::new(name).color(color(tone)));
            },
            |right| {
                right.label(RichText::new(amount).small().color(color(amount_tone)));
            },
        );
    });
}

/// The positions of the ships, one list per ship with its heading and where the ship comes from.
fn ships(ui: &mut Ui, scheme: &Scheme, lines: &[Line], ticks: &mut Vec<(Line, bool)>) {
    let mut start = 0;
    while start < lines.len() {
        let ship = &lines[start];
        let end = lines[start..].iter().position(|l| l.ship != ship.ship).map_or(lines.len(), |n| start + n);
        ui.add_space(8.0);
        ui.horizontal_wrapped(|head| {
            head.label(widgets::bold(head, ship.ship.clone(), egui::TextStyle::Body));
            let origin = if ship.origin == Origin::Wishlist { "s_ek_aus_wunsch" } else { "s_ek_aus_hangar" };
            head.label(sub(scheme, t(origin)));
        });
        widgets::list_frame(ui, scheme, |list| {
            list.set_width(list.available_width());
            for (k, line) in lines[start..end].iter().enumerate() {
                row(list, scheme, k, line, ticks);
            }
        });
        start = end;
    }
}

/// What the cart costs, split by the way and with what is not counted.
pub(crate) fn total_block(ui: &mut Ui, scheme: &Scheme, total: &cart::Total) {
    ui.add_space(10.0);
    ui.label(widgets::bold(ui, total_sum(total), egui::TextStyle::Body).color(color(scheme.accent)));
    total_parts(ui, scheme, total);
}

/// The sum of the cart in words: the head of [`total_block`] and the title of the card of the sum.
fn total_sum(total: &cart::Total) -> String {
    tf("s_wk_summe", &[("preis", Arg::from(plain(total.total)))])
}

/// What [`total_block`] says under the sum: the split by the way, the time to build and what is not counted.
fn total_parts(ui: &mut Ui, scheme: &Scheme, total: &cart::Total) {
    if total.buy > 0.0 && total.craft > 0.0 {
        let split =
            tf("s_wk_summe_teil", &[("kaufen", Arg::from(plain(total.buy))), ("bauen", Arg::from(plain(total.craft)))]);
        ui.label(sub(scheme, split));
    }
    if total.seconds > 0 {
        ui.label(sub(scheme, tf("s_wk_bauzeit", &[("dauer", Arg::from(duration(total.seconds)))])));
    }
    // A sum with parts missing looks like a complete one.
    if total.open > 0 {
        let key = if total.open == 1 { "s_p9_ek_offen_eins" } else { "s_wk_summe_offen" };
        ui.label(RichText::new(tf(key, &[("n", Arg::from(total.open))])).small().color(color(scheme.gold)));
    }
    if total.incomplete {
        ui.label(RichText::new(t("s_p9_ek_untergrenze")).small().color(color(scheme.gold)));
    }
}

/// The ships whose slots are unknown: named, never left out silently.
fn no_data_note(ui: &mut Ui, scheme: &Scheme, missing: &[String]) {
    if missing.is_empty() {
        return;
    }
    let text = if missing.len() == 1 {
        tf("s_p9_ek_ohne_daten_eins", &[("schiffe", Arg::from(missing[0].clone()))])
    } else {
        tf("s_ek_ohne_daten", &[("n", Arg::from(missing.len())), ("schiffe", Arg::from(missing.join(", ")))])
    };
    ui.add_space(8.0);
    ui.label(RichText::new(text).small().color(color(scheme.gold)));
}

/// The parts no shop carries.
fn without_block(ui: &mut Ui, scheme: &Scheme, lines: &[Line], sources: &dyn Sources, count: usize) {
    if count == 0 {
        return;
    }
    ui.add_space(6.0);
    let said =
        if count == 1 { t("s_p9_ek_route_ohne_eins") } else { tf("s_wk_route_ohne", &[("n", Arg::from(count))]) };
    ui.label(RichText::new(said).small().color(color(scheme.gold)));
    let names: Vec<String> = cart::without_lines(lines, sources)
        .into_iter()
        .map(|(name, n)| if n > 1 { format!("{n} × {name}") } else { name })
        .collect();
    ui.label(sub(scheme, names.join("  ·  ")));
    ui.label(sub(scheme, t("s_wk_route_ohne_hilfe")));
}

/// The ticks of a route that is worked through (the shopping list); the boxes of the hangar show the route
/// without them.
pub(crate) struct StopMarks<'a> {
    /// The stops ticked off, as `stop_key`s.
    pub(crate) done: &'a [String],
    /// A stop ticked or unticked in this frame: its key and the new state.
    pub(crate) toggled: Option<(String, bool)>,
    /// All ticks of the route to be taken back.
    pub(crate) clear: bool,
}

/// What the stops tell during a frame: the move asked for by a button (from, to), which stop is being dragged
/// by its grip, which one was let go, and where every stop lies.
#[derive(Default)]
pub(crate) struct StopDrag {
    pub(crate) moves: Option<(usize, usize)>,
    pub(crate) dragging: Option<usize>,
    pub(crate) dropped: Option<usize>,
    pub(crate) rects: Vec<egui::Rect>,
}

/// The place a stop dragged from `from` lands when the mouse is let go at `y`: behind every other stop whose
/// middle lies above the mouse; above all of them it is the first, below all of them the last.
pub(crate) fn drop_target(middles: &[f32], from: usize, y: f32) -> usize {
    middles.iter().enumerate().filter(|(i, middle)| *i != from && **middle < y).count()
}

/// The height of the line that marks where a stop lands at `to` (as `drop_target` gives it) among the
/// stops lying at `rects`, with `from` taken out: above the stop it lands before, under the last one at the end.
pub(crate) fn drop_line_y(rects: &[egui::Rect], from: usize, to: usize) -> Option<f32> {
    let others: Vec<&egui::Rect> = rects.iter().enumerate().filter(|(i, _)| *i != from).map(|(_, r)| r).collect();
    match others.get(to) {
        Some(before) => Some(before.top()),
        None => others.last().map(|last| last.bottom()),
    }
}

/// One part of a stop: its name with the count and its price.
fn stop_part(line: &cart::RouteLine) -> (String, String) {
    let name = if line.count > 1 { format!("{} × {}", line.count, line.name) } else { line.name.clone() };
    let price = if line.count > 1 {
        tf("s_wk_route_je", &[("summe", Arg::from(auec(line.sum))), ("preis", Arg::from(auec(line.price)))])
    } else {
        auec(line.sum)
    };
    (name, price)
}

/// One stop: its tick on the way (with `marks`), the parts bought there in the first rows, below them the place
/// with its shops and, for more than one part, the sum.
fn stop_block(
    ui: &mut Ui,
    scheme: &Scheme,
    k: usize,
    count: usize,
    stop: &Stop,
    drag: &mut StopDrag,
    mut marks: Option<&mut StopMarks>,
) {
    let key = cart::stop_key(stop);
    let done = marks.as_ref().is_some_and(|m| m.done.contains(&key));
    let frame = egui::Frame::new()
        .fill(if k % 2 == 1 { color(scheme.bar) } else { egui::Color32::TRANSPARENT })
        .inner_margin(egui::Margin::symmetric(6, 3));
    let shown = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        let place =
            [stop.system.as_str(), stop.place.as_str()].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>();
        let lines = cart::route_lines(&stop.entries);
        let parts: Vec<(String, String)> = lines.iter().map(stop_part).collect();
        let (first_name, first_price) = parts.first().cloned().unwrap_or_default();
        let mut title = format!("{}  {first_name}", k + 1);
        if done {
            title = format!("{title}  ·  {}", t("s_ek_stopp_erledigt"));
        }
        // The name of the buttons for a screen reader: the stop with its number, not with the gap of the display.
        let who = format!("{}. {}", k + 1, place.join(" · "));
        let gap = ui.spacing().item_spacing.x;
        let tick_width = if marks.is_some() { widgets::ICON_BUTTON_SIZE + gap } else { 0.0 };
        // The grip and the two arrows.
        let arrows_width = tick_width + if count > 1 { 3.0 * (widgets::ICON_BUTTON_SIZE + gap) } else { 0.0 };
        let title_width = arrows_width + text_width(ui, &title, egui::TextStyle::Body, true);
        let tone = if done { scheme.sub } else { scheme.fg };
        spread(
            ui,
            title_width,
            text_width(ui, &first_price, egui::TextStyle::Body, false),
            arrows_width,
            |row| {
                // The grip at the left: the stop is dragged to its place with the mouse.
                if count > 1 {
                    let meaning = t("s_p9_ek_ziehen");
                    let grip = widgets::drag_handle(row, scheme, &meaning);
                    grip.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::accesskit::Role::Button, true, format!("{who}: {meaning}"))
                    });
                    if grip.dragged() {
                        drag.dragging = Some(k);
                    }
                    if grip.drag_stopped() {
                        drag.dropped = Some(k);
                    }
                }
                // On the way: the stop is ticked off once everything there is bought.
                if let Some(marks) = &mut marks {
                    let (icon, meaning) = if done {
                        ("circle-check", t("s_ek_stopp_offen"))
                    } else {
                        ("circle", t("s_ek_stopp_abhaken"))
                    };
                    if row_button(row, scheme, icon, &meaning, &who, true) {
                        marks.toggled = Some((key.clone(), !done));
                    }
                }
                // The order is the player's: besides the grip two buttons move a stop, since a drag cannot be done
                // with the keyboard.
                if count > 1 {
                    let up = t(if k > 0 { "s_p9_ek_hoch" } else { "s_p9_ek_erster" });
                    if row_button(row, scheme, "arrow-up", &up, &who, k > 0) {
                        drag.moves = Some((k, k - 1));
                    }
                    let down = t(if k + 1 < count { "s_p9_ek_runter" } else { "s_p9_ek_letzter" });
                    if row_button(row, scheme, "arrow-down", &down, &who, k + 1 < count) {
                        drag.moves = Some((k, k + 1));
                    }
                }
                row.label(widgets::bold(row, title.clone(), egui::TextStyle::Body).color(color(tone)));
            },
            |right| {
                right.label(RichText::new(first_price.clone()).color(color(tone)));
            },
        );
        // The further parts in the same weight as the first, under its name.
        let indent = arrows_width + text_width(ui, &format!("{}  ", k + 1), egui::TextStyle::Body, true);
        for (name, price) in parts.iter().skip(1) {
            spread(
                ui,
                indent + text_width(ui, name, egui::TextStyle::Body, true),
                text_width(ui, price, egui::TextStyle::Body, false),
                indent,
                |row| {
                    row.add_space(indent);
                    row.label(widgets::bold(row, name.clone(), egui::TextStyle::Body).color(color(tone)));
                },
                |right| {
                    right.label(RichText::new(price.clone()).color(color(tone)));
                },
            );
        }
        // Below the parts, small: where they are bought, and the sum of the stop when it holds more than one part.
        let mut shops: Vec<&str> = Vec::new();
        for line in &lines {
            if !line.shop.is_empty() && !shops.contains(&line.shop.as_str()) {
                shops.push(line.shop.as_str());
            }
        }
        let where_text = place.iter().copied().chain(shops).collect::<Vec<_>>().join(" · ");
        let sum = if parts.len() > 1 { auec(stop.sum) } else { String::new() };
        spread(
            ui,
            indent + text_width(ui, &where_text, egui::TextStyle::Small, false),
            text_width(ui, &sum, egui::TextStyle::Small, false),
            indent,
            |row| {
                row.add_space(indent);
                row.label(sub(scheme, where_text.clone()));
            },
            |right| {
                right.label(sub(scheme, sum.clone()));
            },
        );
    });
    drag.rects.push(shown.response.rect);
}

/// The shopping route for everything that is bought; with `marks` every stop gets its tick.
pub(crate) fn route_block(
    ui: &mut Ui,
    scheme: &Scheme,
    order: &mut Vec<String>,
    paths: Option<&Paths>,
    lines: &[Line],
    sources: &dyn Sources,
    marks: Option<&mut StopMarks>,
) {
    ui.add_space(12.0);
    ui.label(widgets::bold(ui, t("s_wk_route"), egui::TextStyle::Body));
    route_body(ui, scheme, order, paths, lines, sources, marks);
}

/// What [`route_block`] shows under its head: the stops, their ticks and the parts no shop carries.
fn route_body(
    ui: &mut Ui,
    scheme: &Scheme,
    order: &mut Vec<String>,
    paths: Option<&Paths>,
    lines: &[Line],
    sources: &dyn Sources,
    mut marks: Option<&mut StopMarks>,
) {
    let (stops, without) = cart::route(lines, sources);
    if stops.is_empty() {
        // No stop is not "everything is built": perhaps no shop carries the parts.
        if without.is_empty() {
            ui.label(sub(scheme, t("s_wk_route_leer")));
        }
        without_block(ui, scheme, lines, sources, without.len());
        return;
    }
    // "On this route", not "sum": the number above counts the cheapest shop of the whole verse, this one the shops that lie on the route.
    let sum = cart::route_total(&stops);
    let shops_word = tf(if sum.shops == 1 { "s_wk_laden" } else { "s_wk_laeden" }, &[("n", Arg::from(sum.shops))]);
    let stops_word = tf(if sum.stops == 1 { "s_wk_stopp" } else { "s_wk_stopps" }, &[("n", Arg::from(sum.stops))]);
    let head = tf("s_wk_route_summe", &[("preis", Arg::from(plain(sum.total)))]);
    ui.label(sub(scheme, format!("{head}  ·  {shops_word} · {stops_word}")));
    let ordered = cart::ordered_stops(&stops, order);
    let mut drag = StopDrag::default();
    widgets::list_frame(ui, scheme, |list| {
        list.set_width(list.available_width());
        for (k, stop) in ordered.iter().enumerate() {
            stop_block(list, scheme, k, ordered.len(), stop, &mut drag, marks.as_deref_mut());
        }
    });
    // The mouse decides where a dragged stop lands, as 4.12: behind every other stop whose middle lies above it.
    let middles: Vec<f32> = drag.rects.iter().map(|r| r.center().y).collect();
    let mouse = ui.input(|i| i.pointer.latest_pos());
    if let (Some(from), Some(at)) = (drag.dropped, mouse) {
        let to = drop_target(&middles, from, at.y);
        if to != from {
            drag.moves = Some((from, to));
        }
    } else if let (Some(from), Some(at)) = (drag.dragging, mouse)
        && let Some(y) = drop_line_y(&drag.rects, from, drop_target(&middles, from, at.y))
        && let Some(first) = drag.rects.first()
    {
        let line = egui::Rect::from_min_max(egui::pos2(first.left(), y - 1.0), egui::pos2(first.right(), y + 1.0));
        ui.painter().rect_filled(line, 0.0, color(scheme.accent));
    }
    let moves = drag.moves;
    if let Some(marks) = marks {
        let ticked = ordered.iter().filter(|stop| marks.done.contains(&cart::stop_key(stop))).count();
        if ticked > 0 {
            let said = tf("s_ek_stopps_erledigt", &[("0", Arg::from(ticked)), ("1", Arg::from(ordered.len()))]);
            ui.horizontal_wrapped(|row| {
                row.label(sub(scheme, said));
                if row.add(widgets::Button::new(&t("s_ek_stopps_zuruecksetzen"), ButtonKind::Normal, scheme)).clicked()
                {
                    marks.clear = true;
                }
            });
        }
    }
    if let Some((from, to)) = moves
        && let (_, Some(moved)) = cart::move_stop(&ordered, from, to, order)
    {
        if let Some(paths) = paths {
            paths.set_setting(ORDER_SETTING, Value::Array(moved.iter().map(|k| Value::str(k.as_str())).collect()));
        }
        *order = moved;
        ui.ctx().request_repaint();
    }
    without_block(ui, scheme, lines, sources, without.len());
}

/// What could not be saved (the fleet file, the file of the parts from the shops), said once: the first card that
/// calls it shows it, the later ones find it taken.
fn failures(ui: &mut Ui, scheme: &Scheme, failed: &mut [bool; 2]) {
    if std::mem::take(&mut failed[0]) {
        ui.label(RichText::new(t("s_bp_speichern_fehler")).color(color(scheme.red)));
    }
    if std::mem::take(&mut failed[1]) {
        ui.label(RichText::new(t("s_ld_korb_fehler")).color(color(scheme.red)));
    }
}

/// The page with the sources it computes from.
fn page(
    ui: &mut Ui,
    scheme: &Scheme,
    view: &mut View,
    paths: Option<&Paths>,
    sources: &dyn Sources,
    parts: &[CatalogItem],
) {
    ui.label(sub(scheme, t("s_ek_lead")));
    ui.add_space(6.0);
    // The failures stand in the first card drawn below.
    let mut failed = [view.save_failed, view.basket_failed];
    let has = |key: &str| matches!(view.fleet.iter().find(|(k, _)| k == key), Some((_, Value::Array(items))) if !items.is_empty());
    let has_ships = has("schiffe") || has("wunsch");
    let from_shops = !view.basket.items.is_empty();
    // Three situations, three sentences: nothing entered, nothing to get, and no data look the same in the code.
    if !has_ships && !from_shops {
        widgets::card(ui, scheme, |ui| {
            widgets::card_title(ui, scheme, &t("hf_gruppe_schiffe"));
            failures(ui, scheme, &mut failed);
            widgets::empty_state(ui, scheme, "receipt", &t("s_ek_kein_schiff"));
            ui.label(sub(scheme, t("s_ek_korb_leer")));
        });
        templates_block(ui, scheme, view, paths, sources, parts, &mut failed);
        return;
    }
    let mut ticks = Vec::new();
    let mut open: Vec<Line> = Vec::new();
    if has_ships {
        let bill = cart::invoice(&view.fleet, sources);
        let mut all: Vec<Line> = bill.lines.iter().filter(|l| l.origin != Origin::Notepad).cloned().collect();
        cart::bill_order(&mut all);
        let (done, still): (Vec<Line>, Vec<Line>) = all.iter().cloned().partition(|l| l.done);
        // The head counts what is still to do, not what was once planned; without anything open the card is
        // called after the ships.
        let head = if still.is_empty() {
            t("hf_gruppe_schiffe")
        } else {
            let ship_count = still.iter().map(|l| l.ship.as_str()).collect::<HashSet<_>>().len();
            match (still.len(), ship_count) {
                (1, _) => t("s_p9_ek_kopf_1_1"),
                (n, 1) => tf("s_p9_ek_kopf_n_1", &[("n", Arg::from(n))]),
                (n, m) => tf("s_ek_kopf", &[("n", Arg::from(n)), ("schiffe", Arg::from(m))]),
            }
        };
        widgets::card(ui, scheme, |ui| {
            widgets::card_title(ui, scheme, &head);
            failures(ui, scheme, &mut failed);
            if all.is_empty() {
                // Ships whose slots are unknown are no ships "as the factory built them": then only the note stands.
                if bill.without_slot_data.is_empty() {
                    if from_shops {
                        ui.label(sub(scheme, t("s_ek_nichts_offen")));
                    } else {
                        widgets::empty_state(ui, scheme, "circle-check", &t("s_ek_nichts_offen"));
                    }
                }
            } else if still.is_empty() {
                let said = if done.len() == 1 {
                    t("s_p9_ek_alles_erledigt_eins")
                } else {
                    tf("s_ek_alles_erledigt", &[("n", Arg::from(done.len()))])
                };
                ui.label(RichText::new(said).color(color(scheme.accent)));
                done_block(ui, scheme, &done, &mut ticks);
            } else {
                ships(ui, scheme, &cart::group_items(&still), &mut ticks);
                if !done.is_empty() {
                    ui.add_space(6.0);
                    done_block(ui, scheme, &done, &mut ticks);
                }
            }
            no_data_note(ui, scheme, &bill.without_slot_data);
        });
        open = still;
    }
    let basket_lines = basket::lines(&view.basket, sources);
    if from_shops {
        basket_block(ui, scheme, view, paths, sources, &basket_lines, &mut failed);
    }
    templates_block(ui, scheme, view, paths, sources, parts, &mut failed);
    // The sum and the route take the parts of the ships and of the shops together.
    open.extend(basket_lines.into_iter().filter(|l| !l.done));
    if !open.is_empty() {
        view.lookup.look_up(ui, paths, &open);
        let total = cart::total(&open);
        ui.add_space(10.0);
        widgets::card(ui, scheme, |ui| {
            widgets::card_title(ui, scheme, &total_sum(&total));
            total_parts(ui, scheme, &total);
        });
        let done_stops = view.basket.stops_done.clone();
        let mut marks = StopMarks { done: &done_stops, toggled: None, clear: false };
        ui.add_space(12.0);
        widgets::card(ui, scheme, |ui| {
            widgets::card_title(ui, scheme, &t("s_wk_route"));
            route_body(ui, scheme, &mut view.order, paths, &open, sources, Some(&mut marks));
        });
        if let Some((key, on)) = marks.toggled {
            view.edit_basket(paths, |b| b.set_stop_done(&key, on));
            ui.ctx().request_repaint();
        }
        if marks.clear {
            view.edit_basket(paths, |b| !std::mem::take(&mut b.stops_done).is_empty());
            ui.ctx().request_repaint();
        }
    }
    apply_ticks(ui, view, paths, ticks);
}

/// The parts put in from the shops: how many of each, the cheapest shop and every shop that sells it, a tick
/// for "bought" and the way out of the list; the bought ones folded below.
fn basket_block(
    ui: &mut Ui,
    scheme: &Scheme,
    view: &mut View,
    paths: Option<&Paths>,
    sources: &dyn Sources,
    lines: &[Line],
    failed: &mut [bool; 2],
) {
    let (done, open): (Vec<&Line>, Vec<&Line>) = lines.iter().partition(|l| l.done);
    ui.add_space(10.0);
    // One change per frame: the count, the tick or taking a part out.
    let mut count: Option<(String, i64)> = None;
    let mut tick: Option<(String, bool)> = None;
    let mut out: Option<String> = None;
    widgets::card(ui, scheme, |ui| {
        widgets::card_title(ui, scheme, &tf("s_ek_korb", &[("0", Arg::from(lines.len()))]));
        failures(ui, scheme, failed);
        widgets::list_frame(ui, scheme, |list| {
            list.set_width(list.available_width());
            for (k, line) in open.iter().enumerate() {
                basket_row(list, scheme, k, line, sources, &mut count, &mut tick, &mut out);
            }
        });
        if !done.is_empty() {
            let title = tf("s_ek_korb_gekauft", &[("0", Arg::from(done.len()))]);
            widgets::collapsing(&title).id_salt("basket-done").show(ui, |inner| {
                for (k, line) in done.iter().enumerate() {
                    basket_row(inner, scheme, k, line, sources, &mut count, &mut tick, &mut out);
                }
            });
        }
    });
    if let Some((reference, n)) = count {
        view.edit_basket(paths, |b| b.set_count(&reference, n));
    }
    if let Some((reference, on)) = tick {
        view.edit_basket(paths, |b| b.set_done(&reference, on));
    }
    if let Some(reference) = out {
        view.edit_basket(paths, |b| b.set_count(&reference, 0));
    }
}

/// One part from the shops: the tick, how many, the name and the amount; below it the cheapest shop and,
/// folded, every shop that sells it.
#[allow(clippy::too_many_arguments)]
fn basket_row(
    ui: &mut Ui,
    scheme: &Scheme,
    k: usize,
    line: &Line,
    sources: &dyn Sources,
    count: &mut Option<(String, i64)>,
    tick: &mut Option<(String, bool)>,
    out: &mut Option<String>,
) {
    let frame = egui::Frame::new()
        .fill(if k % 2 == 1 { color(scheme.bar) } else { egui::Color32::TRANSPARENT })
        .inner_margin(egui::Margin::symmetric(6, 3));
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        let (amount, tone) = amount_of(scheme, line);
        ui.horizontal_wrapped(|row| {
            let (icon, meaning) =
                if line.done { ("circle-check", t("s_p9_ek_haken_weg")) } else { ("circle", t("s_ek_korb_abhaken")) };
            if row_button(row, scheme, icon, &meaning, &line.name, true) {
                *tick = Some((line.reference.clone(), !line.done));
            }
            let mut n = line.count;
            if widgets::number_field(row, scheme, &format!("korb-{}", line.reference), &mut n, 1..=999, "") {
                *count = Some((line.reference.clone(), n));
            }
            let name_tone = if line.done { scheme.sub } else { scheme.fg };
            row.label(RichText::new(&line.name).color(color(name_tone)));
            row.label(RichText::new(amount).small().color(color(tone)));
            if row_button(row, scheme, "trash-2", &t("s_ek_korb_entfernen"), &line.name, true) {
                *out = Some(line.reference.clone());
            }
        });
        if line.buy.condition == Condition::Known && !line.buy.shop.is_empty() {
            let best = tf(
                "s_wk_kauf_preis",
                &[
                    ("preis", Arg::from(plain(line.buy.price.unwrap_or(0.0)))),
                    ("laden", Arg::from(line.buy.shop.as_str())),
                    ("ort", Arg::from(line.buy.place.as_str())),
                ],
            );
            ui.label(sub(scheme, best));
        }
        // Every shop that sells it, not only the cheapest: the route may pass another one.
        let offers = sources.shops_for(&line.reference);
        if !offers.is_empty() {
            let title = tf("s_ek_korb_orte", &[("0", Arg::from(offers.len()))]);
            widgets::collapsing(&title).id_salt(("basket-places", line.reference.as_str())).show(ui, |inner| {
                for offer in &offers {
                    let place: Vec<&str> =
                        [offer.place.as_str(), offer.system.as_str()].into_iter().filter(|s| !s.is_empty()).collect();
                    inner.label(sub(scheme, format!("{} · {} · {}", auec(offer.price), offer.shop, place.join(" · "))));
                }
            });
        }
    });
}

/// The most parts a search inside a template lists; below them how many more fit.
const SEARCH_SHOWN: usize = 8;
/// What a field with the X needs besides its text: its margins and the X.
const FIELD_MARGIN: f32 = 34.0;

/// What the player did to the templates in a frame.
enum SetAction {
    /// An empty template of this name.
    Create(String),
    /// The parts from the shops as a template of this name.
    FromList(String),
    Toggle(String),
    /// The parts of the template onto the list.
    Insert(String),
    StartRename(String),
    /// The template and its new name.
    Rename(String, String),
    StopRename,
    Delete(String),
    /// The template, the code of the part and how many; at zero the part goes.
    Count(String, String, i64),
    /// The template, the code of the part and its name: one more of it.
    Add(String, String, String),
}

/// What the templates show besides their parts: which are unfolded, what is typed into their searches,
/// the one being renamed and the one deleted a moment ago.
#[derive(Default)]
struct TemplateView {
    open: HashSet<String>,
    search: HashMap<String, String>,
    /// The template being renamed and the name typed for it.
    renaming: Option<(String, String)>,
    /// The field of the new name gets the cursor in the next frame.
    focus_rename: bool,
    /// A template deleted a moment ago: when, its place and the template, to be taken back.
    deleted: Option<(f64, usize, basket::Set)>,
}

/// How many parts a template holds, in words.
fn parts_word(set: &basket::Set) -> String {
    match set.items.iter().map(|i| i.count).sum::<i64>() {
        0 => t("s_ek_vorlage_ohne_teile"),
        1 => t("s_ek_vorlage_teil_eins"),
        n => tf("s_ek_vorlage_teile", &[("0", Arg::from(n))]),
    }
}

/// The templates: named lists of parts with how many, made empty or from the parts from the shops,
/// unfolded to see and change their parts, put on the list (instead of the parts from the shops),
/// renamed, or deleted (and taken back for a few seconds).
fn templates_block(
    ui: &mut Ui,
    scheme: &Scheme,
    view: &mut View,
    paths: Option<&Paths>,
    sources: &dyn Sources,
    parts: &[CatalogItem],
    failed: &mut [bool; 2],
) {
    ui.add_space(10.0);
    widgets::card(ui, scheme, |ui| {
        widgets::card_title(ui, scheme, &t("s_ek_vorlagen"));
        failures(ui, scheme, failed);
        templates_body(ui, scheme, view, paths, sources, parts);
    });
}

/// What the card of the templates holds under its title.
fn templates_body(
    ui: &mut Ui,
    scheme: &Scheme,
    view: &mut View,
    paths: Option<&Paths>,
    sources: &dyn Sources,
    parts: &[CatalogItem],
) {
    ui.label(sub(scheme, t("s_ek_vorlage_lead")));
    let can_keep = !view.basket.items.is_empty();
    let mut action: Option<SetAction> = None;
    ui.horizontal_wrapped(|row| {
        // The hint fits whole, and the field never reaches past the edge; the buttons wrap below it.
        let hint = t("s_ek_vorlage_name");
        let room = row.available_width() - 2.0;
        let width = (text_width(row, &hint, egui::TextStyle::Body, false) + FIELD_MARGIN).max(160.0).min(room);
        widgets::clearable_field(row, &mut view.template_name, &hint, scheme, width);
        let name = view.template_name.trim().to_string();
        let (empty_word, list_word) = (t("s_ek_vorlage_neu"), t("s_ek_vorlage_speichern"));
        // Without a name both buttons wait and say what is missing.
        let missing = name.is_empty().then(|| t("s_ek_vorlage_name_fehlt"));
        let empty =
            widgets::Button::new(&empty_word, ButtonKind::Normal, scheme).icon("plus").blocked(missing.as_deref());
        if row.add(empty).clicked() {
            action = Some(SetAction::Create(name.clone()));
        }
        if can_keep {
            let from_list = widgets::Button::new(&list_word, ButtonKind::Normal, scheme).blocked(missing.as_deref());
            if row.add(from_list).clicked() {
                action = Some(SetAction::FromList(name));
            }
        }
    });
    if !view.basket.sets.is_empty() {
        ui.add_space(4.0);
        widgets::list_frame(ui, scheme, |list| {
            list.set_width(list.available_width());
            for (k, set) in view.basket.sets.iter().enumerate() {
                template_row(list, scheme, k, set, &mut view.templates, sources, parts, &mut action);
            }
        });
    }
    if let Some(action) = action {
        apply_template_action(ui, view, paths, action);
        ui.ctx().request_repaint();
    }
    if let Some(said) = &view.template_said {
        ui.label(RichText::new(said).small().color(color(scheme.accent)));
    }
    // A deleted template can be taken back for a few seconds, to the place where it stood.
    if let Some((since, at, set)) = view.templates.deleted.take() {
        let said = tf("s_ek_vorlage_geloescht", &[("0", Arg::from(set.name.as_str()))]);
        let word = t("s_mlg_rueckgaengig");
        let shown = widgets::notice(ui.ctx(), scheme, since, "trash-2", &said, Some(&word));
        if shown.acted {
            view.edit_basket(paths, |b| b.restore_set(at, set));
            ui.ctx().request_repaint();
        } else if shown.shown {
            view.templates.deleted = Some((since, at, set));
        }
    }
}

/// Does what the player did to the templates.
fn apply_template_action(ui: &Ui, view: &mut View, paths: Option<&Paths>, action: SetAction) {
    let said = |key: &str, name: &str| Some(tf(key, &[("0", Arg::from(name))]));
    match action {
        SetAction::Create(name) => {
            if view.basket.set(&name).is_some() {
                view.template_said = said("s_ek_vorlage_gibt_es", &name);
            } else {
                view.edit_basket(paths, |b| b.new_set(&name));
                view.template_said = said("s_ek_vorlage_angelegt", &name);
                view.template_name.clear();
            }
            view.templates.open.insert(name);
        }
        SetAction::FromList(name) => {
            view.edit_basket(paths, |b| b.save_set(&name));
            view.template_said = said("s_ek_vorlage_gespeichert", &name);
            view.template_name.clear();
            view.templates.open.insert(name);
        }
        SetAction::Toggle(name) => {
            if !view.templates.open.remove(&name) {
                view.templates.open.insert(name);
            }
        }
        SetAction::Insert(name) => {
            view.edit_basket(paths, |b| b.load_set(&name));
            view.template_said = said("s_ek_vorlage_geladen", &name);
        }
        SetAction::StartRename(name) => {
            view.templates.renaming = Some((name.clone(), name));
            view.templates.focus_rename = true;
        }
        SetAction::Rename(from, to) => {
            if view.basket.set(&to).is_some() && to != from {
                view.template_said = said("s_ek_vorlage_gibt_es", &to);
                return;
            }
            view.edit_basket(paths, |b| b.rename_set(&from, &to));
            view.templates.renaming = None;
            if view.basket.set(&to).is_some() && to != from {
                if view.templates.open.remove(&from) {
                    view.templates.open.insert(to.clone());
                }
                if let Some(typed) = view.templates.search.remove(&from) {
                    view.templates.search.insert(to.clone(), typed);
                }
                view.template_said = Some(tf(
                    "s_ek_vorlage_umbenannt",
                    &[("0", Arg::from(from.as_str())), ("1", Arg::from(to.as_str()))],
                ));
            }
        }
        SetAction::StopRename => view.templates.renaming = None,
        SetAction::Delete(name) => {
            let mut taken = None;
            view.edit_basket(paths, |b| {
                taken = b.take_set(&name);
                taken.is_some()
            });
            if let Some((at, set)) = taken {
                view.templates.open.remove(&set.name);
                view.templates.search.remove(&set.name);
                view.templates.deleted = Some((ui.input(|i| i.time), at, set));
                view.template_said = None;
            }
        }
        SetAction::Count(name, reference, count) => {
            view.edit_basket(paths, |b| b.set_count_in_set(&name, &reference, count));
        }
        SetAction::Add(name, reference, part) => {
            view.edit_basket(paths, |b| b.add_to_set(&name, &reference, &part, 1));
        }
    }
}

/// One template: the arrow that unfolds it, its name and how many parts, and the buttons to put it on the
/// list, rename it and delete it; unfolded, its parts and the search that adds more.
#[allow(clippy::too_many_arguments)]
fn template_row(
    ui: &mut Ui,
    scheme: &Scheme,
    k: usize,
    set: &basket::Set,
    state: &mut TemplateView,
    sources: &dyn Sources,
    parts: &[CatalogItem],
    action: &mut Option<SetAction>,
) {
    let frame = egui::Frame::new()
        .fill(if k % 2 == 1 { color(scheme.bar) } else { egui::Color32::TRANSPARENT })
        .inner_margin(egui::Margin::symmetric(6, 3));
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        let open = state.open.contains(&set.name);
        let (icon, meaning) = if open {
            ("chevron-down", t("s_ek_vorlage_zuklappen"))
        } else {
            ("chevron-right", t("s_ek_vorlage_aufklappen"))
        };
        let gap = ui.spacing().item_spacing.x;
        match state.renaming.as_mut().filter(|(name, _)| *name == set.name) {
            Some((_, typed)) => {
                ui.horizontal_wrapped(|row| {
                    if row_button(row, scheme, icon, &meaning, &set.name, true) {
                        *action = Some(SetAction::Toggle(set.name.clone()));
                    }
                    // The field leaves room for its two buttons, so that the row never reaches past the edge.
                    let room = row.available_width() - 2.0 * (widgets::ICON_BUTTON_SIZE + gap) - FIELD_MARGIN;
                    let width = room.clamp(60.0, 320.0);
                    let hint = t("s_ek_vorlage_neuer_name");
                    let field = row
                        .push_id(("vorlage-name", set.name.as_str()), |inner| {
                            widgets::clearable_field(inner, typed, &hint, scheme, width)
                        })
                        .inner;
                    if std::mem::take(&mut state.focus_rename) {
                        field.request_focus();
                    }
                    let entered = field.lost_focus() && row.input(|i| i.key_pressed(egui::Key::Enter));
                    let to = typed.trim().to_string();
                    let apply = t("s_ek_vorlage_name_uebernehmen");
                    if (row_button(row, scheme, "check", &apply, &set.name, !to.is_empty()) || entered)
                        && !to.is_empty()
                    {
                        *action = Some(SetAction::Rename(set.name.clone(), to));
                    }
                    if row_button(row, scheme, "x", &t("s_ek_vorlage_name_abbrechen"), &set.name, true) {
                        *action = Some(SetAction::StopRename);
                    }
                });
            }
            None => {
                // The name with its count at the left, the three buttons at the right edge; where both do not
                // fit side by side, the buttons go under the name.
                let count = parts_word(set);
                let left_width = widgets::ICON_BUTTON_SIZE
                    + text_width(ui, &set.name, egui::TextStyle::Body, true)
                    + text_width(ui, &count, egui::TextStyle::Small, false)
                    + 2.0 * gap;
                let right_width = 3.0 * widgets::ICON_BUTTON_SIZE + 2.0 * gap;
                let (mut toggled, mut chosen) = (false, None);
                spread(
                    ui,
                    left_width,
                    right_width,
                    widgets::ICON_BUTTON_SIZE + gap,
                    |row| {
                        toggled = row_button(row, scheme, icon, &meaning, &set.name, true);
                        row.label(widgets::bold(row, set.name.clone(), egui::TextStyle::Body));
                        row.label(sub(scheme, count.clone()));
                    },
                    |right| {
                        // On screen always: put on the list, rename, delete. Laid out from the right edge,
                        // the buttons come in reverse.
                        let mut order = [0, 1, 2];
                        if right.layout().prefer_right_to_left() {
                            order.reverse();
                        }
                        let name = &set.name;
                        for button in order {
                            let (icon, key) = match button {
                                0 => ("archive-restore", "s_ek_vorlage_laden"),
                                1 => ("pencil", "s_ek_vorlage_umbenennen"),
                                _ => ("trash-2", "s_ek_vorlage_loeschen"),
                            };
                            if row_button(right, scheme, icon, &t(key), name, true) {
                                chosen = Some(match button {
                                    0 => SetAction::Insert(name.clone()),
                                    1 => SetAction::StartRename(name.clone()),
                                    _ => SetAction::Delete(name.clone()),
                                });
                            }
                        }
                    },
                );
                if toggled {
                    *action = Some(SetAction::Toggle(set.name.clone()));
                } else if chosen.is_some() {
                    *action = chosen;
                }
            }
        }
        if open {
            let indent = egui::Margin { left: 12, right: 0, top: 2, bottom: 2 };
            egui::Frame::new().inner_margin(indent).show(ui, |inner| {
                template_parts(inner, scheme, set, state, sources, parts, action);
            });
        }
    });
}

/// The parts of an unfolded template, each with how many, its price and where (as far as the shops are
/// known), and the way out; below them the search through the parts of the shops that adds one.
fn template_parts(
    ui: &mut Ui,
    scheme: &Scheme,
    set: &basket::Set,
    state: &mut TemplateView,
    sources: &dyn Sources,
    parts: &[CatalogItem],
    action: &mut Option<SetAction>,
) {
    ui.set_width(ui.available_width());
    if set.items.is_empty() {
        ui.label(sub(scheme, t("s_ek_vorlage_leer")));
    } else {
        widgets::list_frame(ui, scheme, |list| {
            list.set_width(list.available_width());
            for (k, item) in set.items.iter().enumerate() {
                template_part_row(list, scheme, k, set, item, sources, action);
            }
        });
    }
    ui.add_space(4.0);
    let typed = state.search.entry(set.name.clone()).or_default();
    let width = (ui.available_width() - 2.0).min(420.0);
    let hint = t("s_ek_vorlage_suche");
    ui.push_id(("vorlage-suche", set.name.as_str()), |inner| {
        widgets::search_field(inner, typed, &hint, scheme, width);
    });
    if parts.is_empty() {
        ui.label(sub(scheme, t("s_ek_vorlage_ohne_katalog")));
        return;
    }
    if typed.trim().chars().count() < 2 {
        return;
    }
    let needle = typed.clone();
    let found: Vec<&CatalogItem> = parts.iter().filter(|p| shops::search_finds(&p.search_text(), &needle)).collect();
    if found.is_empty() {
        ui.label(sub(scheme, t("s_ek_vorlage_kein_treffer")));
        return;
    }
    widgets::list_frame(ui, scheme, |list| {
        list.set_width(list.available_width());
        for (k, part) in found.iter().take(SEARCH_SHOWN).enumerate() {
            let frame = egui::Frame::new()
                .fill(if k % 2 == 1 { color(scheme.bar) } else { egui::Color32::TRANSPARENT })
                .inner_margin(egui::Margin::symmetric(6, 2));
            frame.show(list, |cell| {
                cell.set_width(cell.available_width());
                cell.horizontal_wrapped(|row| {
                    if row_button(row, scheme, "plus", &t("s_ek_vorlage_hinzufuegen"), &part.name, true) {
                        *action = Some(SetAction::Add(set.name.clone(), part.code.clone(), part.name.clone()));
                    }
                    row.label(&part.name);
                    let group = shops::group_label(&part.group);
                    let what: Vec<&str> =
                        [group.as_str(), part.maker.as_str()].into_iter().filter(|s| !s.trim().is_empty()).collect();
                    if !what.is_empty() {
                        row.label(sub(scheme, what.join(" · ")));
                    }
                });
            });
        }
    });
    if found.len() > SEARCH_SHOWN {
        ui.label(sub(scheme, tf("s_mlg_mehr", &[("n", Arg::from(found.len() - SEARCH_SHOWN))])));
    }
}

/// One part of a template: how many (− and +), its name, its price and where, and the way out.
fn template_part_row(
    ui: &mut Ui,
    scheme: &Scheme,
    k: usize,
    set: &basket::Set,
    item: &basket::Item,
    sources: &dyn Sources,
    action: &mut Option<SetAction>,
) {
    let frame = egui::Frame::new()
        .fill(if k % 2 == 1 { color(scheme.bar) } else { egui::Color32::TRANSPARENT })
        .inner_margin(egui::Margin::symmetric(6, 2));
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        let mut count = None;
        ui.horizontal_wrapped(|row| {
            let mut n = item.count;
            let id = format!("vorlage-{}-{}", set.name, item.reference);
            if widgets::number_field(row, scheme, &id, &mut n, 1..=999, "") {
                count = Some(n);
            }
            row.label(&item.name);
        });
        // The price and where below the name, the bin at the right edge (or under the price where both do
        // not fit side by side).
        let (price, tone) = part_price(scheme, &item.reference, sources);
        let mut out = false;
        spread(
            ui,
            text_width(ui, &price, egui::TextStyle::Small, false),
            widgets::ICON_BUTTON_SIZE,
            0.0,
            |row| {
                row.label(RichText::new(price.clone()).small().color(color(tone)));
            },
            |right| {
                out = row_button(right, scheme, "trash-2", &t("s_ek_vorlage_teil_entfernen"), &item.name, true);
            },
        );
        if out {
            count = Some(0);
        }
        if let Some(n) = count {
            *action = Some(SetAction::Count(set.name.clone(), item.reference.clone(), n));
        }
    });
}

/// The price of one piece of a part and the cheapest shop with its place, as far as the shops are known,
/// with the tone it stands in.
fn part_price(scheme: &Scheme, reference: &str, sources: &dyn Sources) -> (String, versekit_core::theme::Color) {
    let buy = cart::buy_option(reference, sources);
    match (buy.condition, buy.price) {
        (Condition::Known, Some(price)) if !buy.shop.is_empty() => (
            tf(
                "s_wk_kauf_preis",
                &[
                    ("preis", Arg::from(plain(price))),
                    ("laden", Arg::from(buy.shop.as_str())),
                    ("ort", Arg::from(buy.place.as_str())),
                ],
            ),
            scheme.sub,
        ),
        (Condition::Known, Some(price)) => (auec(price), scheme.sub),
        (Condition::NotChecked, _) => (t("s_wk_nicht_geprueft"), scheme.sub),
        _ => (t("s_ek_kein_betrag"), scheme.gold),
    }
}

/// What is ticked off: counted, and folded open to take a tick back.
fn done_block(ui: &mut Ui, scheme: &Scheme, done: &[Line], ticks: &mut Vec<(Line, bool)>) {
    let title = tf("s_ek_abgehakt", &[("n", Arg::from(done.len()))]);
    // The identifier is not the title: the count in it changes with every tick and would fold the list shut.
    widgets::collapsing(&title).id_salt("cart-done").show(ui, |inner| {
        ships(inner, scheme, &cart::group_items(done), ticks);
    });
}

fn apply_ticks(ui: &Ui, view: &mut View, paths: Option<&Paths>, ticks: Vec<(Line, bool)>) {
    if !ticks.is_empty() {
        // The change shows at once, not with the next movement of the mouse.
        ui.ctx().request_repaint();
    }
    write_ticks(view, paths, ticks);
}

/// Writes the ticks into the fleet file (or the fleet of the sample).
fn write_ticks(view: &mut View, paths: Option<&Paths>, ticks: Vec<(Line, bool)>) {
    if ticks.is_empty() {
        return;
    }
    view.edit(paths, |data| {
        let mut changed = false;
        for (line, on) in &ticks {
            changed |= cart::tick(data, line.origin, &line.ship, &line.paths, *on);
        }
        changed
    });
}

/// Draws the page.
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut ShoppingState, crafting: &mut CraftingState) {
    state.follow(ui);
    let ShoppingState { paths, stores, view } = state;
    if paths.is_none() {
        page(ui, scheme, view, None, &Sample, &stores.parts);
        return;
    }
    // The recipes come with the window, not with a visit to the crafting page.
    if crafting.values_or_wait(ui, scheme).is_none() {
        return;
    }
    let Some((values, book, _)) = crafting.parts() else { return };
    let slots = |ship: &Map, chosen: &Map| stores.erkul.cart_slots(ship, chosen);
    let data =
        CartData { values, book, shops: &stores.shops, raw: &stores.raw, market: &stores.market, slots: Some(&slots) };
    page(ui, scheme, view, paths.as_ref(), &data, &stores.parts);
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Rect, vec2};
    use versekit_core::language::{Choice, set_language};

    fn texts(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
        fn collect(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
            match shape {
                egui::epaint::Shape::Text(text) => out.push(text.galley.text().to_string()),
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut out));
        out
    }

    /// One frame without a window around it.
    fn frame(ctx: &egui::Context, add: impl FnMut(&mut Ui)) {
        let mut output = ctx.run_ui(egui::RawInput::default(), add);
        output.textures_delta.clear();
    }

    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx
    }

    /// One frame inside the page frame of the window: the texts and how far the page sticks out.
    fn draw(ctx: &egui::Context, state: &mut ShoppingState, width: f32) -> (Vec<String>, f32) {
        let scheme = versekit_core::theme::current();
        let mut crafting = CraftingState::default();
        let mut over = 0.0;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 2400.0))),
                ..Default::default()
            },
            |ui| {
                let shown =
                    crate::layout::page(ui, scheme, "cart-test", |ui| show_page(ui, scheme, state, &mut crafting));
                over = shown.overflow;
            },
        );
        output.textures_delta.clear();
        (texts(&output.shapes), over)
    }

    #[test]
    fn the_sample_bill_shows_ships_positions_sum_and_route() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut state = ShoppingState::default();
        let (shown, over) = draw(&context(), &mut state, 900.0);
        assert_eq!(over, 0.0);
        // Four open positions on two ships: the cooler on both, the shield and the wish ship itself; the drive is ticked off.
        assert!(shown.contains(&tf("s_ek_kopf", &[("n", Arg::Int(4)), ("schiffe", Arg::Int(2))])), "{shown:?}");
        assert!(shown.contains(&"Cutlass Black".to_string()) && shown.contains(&"Carrack".to_string()), "{shown:?}");
        assert!(shown.contains(&t("s_ek_aus_hangar")) && shown.contains(&t("s_ek_aus_wunsch")), "{shown:?}");
        assert!(shown.contains(&t("s_ek_das_schiff")), "the wish ship stands as a position itself: {shown:?}");
        assert!(shown.contains(&"Cooler S2".to_string()) && shown.contains(&"Shield S1".to_string()), "{shown:?}");
        // The shield is built: 3 Iron at 10 and 1,5 Gold at 25, the Quantainium has no price.
        assert!(shown.contains(&auec(67.5)), "{shown:?}");
        assert!(shown.contains(&auec(22_700.0)), "{shown:?}");
        assert!(shown.contains(&auec(4_600_000.0)), "{shown:?}");
        // The sum: ship + 2 coolers + shield material.
        let total = 4_600_000.0 + 2.0 * 22_700.0 + 67.5;
        assert!(shown.contains(&tf("s_wk_summe", &[("preis", Arg::from(plain(total)))])), "{shown:?}");
        assert!(shown.contains(&tf("s_wk_bauzeit", &[("dauer", Arg::from(duration(420)))])), "{shown:?}");
        assert!(shown.contains(&t("s_p9_ek_untergrenze")), "the Quantainium has no price: {shown:?}");
        // One stop: both coolers at the same shop are one line with the count.
        assert!(shown.contains(&t("s_wk_route")), "{shown:?}");
        assert!(shown.iter().any(|s| s == "1  2 × Frostline Cooler"), "the parts on top: {shown:?}");
        assert!(shown.iter().any(|s| s.ends_with("Dumper's Depot")), "the place below: {shown:?}");
        // The done position is folded under its count.
        assert!(shown.contains(&tf("s_ek_abgehakt", &[("n", Arg::Int(1))])), "{shown:?}");
        // A part that is the factory part is no position.
        assert!(!shown.contains(&"Stock Radar".to_string()), "{shown:?}");
    }

    /// A part line with the slots given, for the tests of the ticks.
    fn blank(ship: &str, origin: Origin, name: &str, slots: &[&str]) -> Line {
        Line {
            kind: Kind::Part,
            ship: ship.into(),
            origin,
            position: String::new(),
            name: name.into(),
            reference: String::new(),
            way: cart::BUY.into(),
            buy: cart::BuyOption {
                condition: Condition::NotChecked,
                price: None,
                shop: String::new(),
                place: String::new(),
            },
            craft: cart::CraftOption {
                condition: Condition::NoRecipe,
                material: None,
                seconds: None,
                blueprint: String::new(),
                unpriced: Vec::new(),
            },
            done: false,
            count: 1,
            path: String::new(),
            paths: slots.iter().map(|s| (*s).to_string()).collect(),
            slot_kind: String::new(),
            size: None,
            factory_ref: String::new(),
            factory_name: String::new(),
        }
    }

    #[test]
    fn a_tick_on_the_wish_ship_does_not_touch_the_hangar_ship_of_the_same_name() {
        let mut state = ShoppingState::default();
        state.view.fleet = parse(
            r#"{"schiffe": [{"name": "Carrack", "belegung": {"hardpoint_cooler_01": {"ref": "sample-cooler", "name": "C", "weg": "kaufen"}}}],
                "wunsch": [{"name": "Carrack", "belegung": {"hardpoint_cooler_01": {"ref": "sample-cooler", "name": "C", "weg": "kaufen"}}}]}"#,
        );
        write_ticks(
            &mut state.view,
            None,
            vec![(blank("Carrack", Origin::Wishlist, "C", &["hardpoint_cooler_01"]), true)],
        );
        let list = |key: &str| {
            let (_, value) = state.view.fleet.iter().find(|(k, _)| k == key).expect("list");
            jsonio::dumps(value, None, false)
        };
        assert!(list("wunsch").contains("erledigt"), "{}", list("wunsch"));
        assert!(!list("schiffe").contains("erledigt"), "{}", list("schiffe"));
    }

    #[test]
    fn a_ticked_position_says_done_instead_of_an_amount() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let done: Vec<Line> =
            cart::invoice(&parse(SAMPLE_FLEET), &Sample).lines.into_iter().filter(|l| l.done).collect();
        assert_eq!(done.len(), 1);
        let scheme = versekit_core::theme::current();
        let mut ticks = Vec::new();
        let mut output = context().run_ui(egui::RawInput::default(), |ui| ships(ui, scheme, &done, &mut ticks));
        output.textures_delta.clear();
        let shown = texts(&output.shapes);
        assert!(shown.contains(&t("s_ek_erledigt")), "{shown:?}");
        assert!(!shown.contains(&auec(0.0)) && !shown.iter().any(|s| s.contains("aUEC")), "{shown:?}");
    }

    #[test]
    fn a_part_without_a_price_and_one_not_yet_looked_up_are_told_apart() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut state = ShoppingState::default();
        // The drive is known to UEX, no shop carries it; the other part nobody has looked up.
        state.view.fleet = parse(
            r#"{"schiffe": [{"name": "Cutlass Black", "belegung": {
                "hardpoint_quantum_drive": {"ref": "sample-drive", "name": "Drive", "weg": "kaufen"},
                "hardpoint_radar_01": {"ref": "sample-new", "name": "New Radar", "weg": "kaufen"}}}]}"#,
        );
        let (shown, _) = draw(&context(), &mut state, 900.0);
        assert!(shown.contains(&t("s_ek_kein_betrag")), "{shown:?}");
        assert!(shown.contains(&t("s_wk_nicht_geprueft")), "{shown:?}");
        // Neither is on the route; the one no shop carries is named.
        assert!(shown.contains(&tf("s_wk_route_ohne", &[("n", Arg::Int(2))])), "{shown:?}");
    }

    #[test]
    fn the_route_takes_the_place_that_covers_most_before_the_cheaper_one() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut state = ShoppingState::default();
        // The cooler is cheaper in Area18, but Lorville carries the cooler and the shield: one stop.
        state.view.fleet = parse(
            r#"{"schiffe": [{"name": "Cutlass Black", "belegung": {
                "hardpoint_cooler_01": {"ref": "sample-cooler", "name": "Cooler", "weg": "kaufen"},
                "hardpoint_shield_01": {"ref": "sample-shield", "name": "Shield", "weg": "kaufen"}}}]}"#,
        );
        let (shown, _) = draw(&context(), &mut state, 900.0);
        assert!(shown.iter().any(|s| s.contains("Lorville")), "{shown:?}");
        assert!(!shown.iter().any(|s| s.contains("Area18")), "{shown:?}");
        assert!(shown.iter().any(|s| s.ends_with(&tf("s_wk_stopp", &[("n", Arg::Int(1))]))), "{shown:?}");
    }

    #[test]
    fn a_tick_goes_to_the_ship_of_its_origin_and_takes_the_position_off_the_bill() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut state = ShoppingState::default();
        let ctx = context();
        let (first, _) = draw(&ctx, &mut state, 900.0);
        assert!(first.contains(&"Shield S1".to_string()));
        // The shield of the Cutlass is ticked off like the page does it.
        let shield = vec!["hardpoint_shield_01".to_string()];
        write_ticks(
            &mut state.view,
            None,
            vec![(
                Line {
                    kind: Kind::Part,
                    ship: "Cutlass Black".into(),
                    origin: Origin::Hangar,
                    position: String::new(),
                    name: "Arc Shield".into(),
                    reference: SAMPLE_SHIELD.into(),
                    way: cart::CRAFT.into(),
                    buy: cart::BuyOption {
                        condition: Condition::NotChecked,
                        price: None,
                        shop: String::new(),
                        place: String::new(),
                    },
                    craft: cart::CraftOption {
                        condition: Condition::NoRecipe,
                        material: None,
                        seconds: None,
                        blueprint: String::new(),
                        unpriced: Vec::new(),
                    },
                    done: false,
                    count: 1,
                    path: String::new(),
                    paths: shield,
                    slot_kind: String::new(),
                    size: None,
                    factory_ref: String::new(),
                    factory_name: String::new(),
                },
                true,
            )],
        );
        let (second, _) = draw(&ctx, &mut state, 900.0);
        assert!(second.contains(&tf("s_ek_kopf", &[("n", Arg::Int(3)), ("schiffe", Arg::Int(2))])), "{second:?}");
        assert!(second.contains(&tf("s_ek_abgehakt", &[("n", Arg::Int(2))])), "{second:?}");
        // Nothing to build any more: no crafting time.
        assert!(!second.iter().any(|s| s.starts_with(&t("s_wk_bauzeit").replace("{dauer}", ""))), "{second:?}");
    }

    #[test]
    fn the_three_empty_situations_have_three_sentences() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut state = ShoppingState::default();
        state.view.fleet = hangar::empty();
        let (shown, _) = draw(&context(), &mut state, 900.0);
        assert!(shown.contains(&t("s_ek_kein_schiff")), "{shown:?}");
        // A ship that stands as the factory built it: nothing to get.
        state.view.fleet = parse(r#"{"schiffe": [{"name": "Gladius", "belegung": {}}]}"#);
        let (shown, _) = draw(&context(), &mut state, 900.0);
        assert!(shown.contains(&t("s_ek_nichts_offen")), "{shown:?}");
        // A plan on a ship without slot data is named, and not called "nothing to get".
        state.view.fleet = parse(r#"{"schiffe": [{"name": "Galaxy", "belegung": {"a": {"ref": "x", "name": "Y"}}}]}"#);
        let (shown, _) = draw(&context(), &mut state, 900.0);
        assert!(shown.contains(&tf("s_p9_ek_ohne_daten_eins", &[("schiffe", Arg::from("Galaxy"))])), "{shown:?}");
        // Everything ticked off is not the same as nothing planned.
        state.view.fleet = parse(
            r#"{"schiffe": [{"name": "Cutlass Black", "belegung": {
                "hardpoint_cooler_01": {"ref": "sample-cooler", "name": "Frostline Cooler", "weg": "kaufen", "erledigt": true}}}]}"#,
        );
        let (shown, _) = draw(&context(), &mut state, 900.0);
        assert!(shown.contains(&t("s_p9_ek_alles_erledigt_eins")), "{shown:?}");
        assert!(!shown.contains(&t("s_ek_nichts_offen")), "{shown:?}");
    }

    /// A data folder of its own with a fleet file.
    fn folder(name: &str, fleet: &str) -> Paths {
        use versekit_core::paths::Host;
        use versekit_core::pypath::Flavor;
        let dir = std::env::temp_dir().join(format!("versekit-einkauf-{}-{name}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        let text = dir.to_string_lossy().into_owned();
        let paths = Paths::new(Host::new(Flavor::native(), &[(versekit_core::legacy::ENV_HOME, &text)], &text));
        assert!(hangar::write(&paths.app_file(FLEET_FILE), &parse(fleet)));
        paths
    }

    #[test]
    fn a_tick_is_written_to_the_file_and_a_change_that_changes_nothing_writes_nothing() {
        let _language = crate::language_lock();
        let paths = folder(
            "tick",
            r#"{"format": 1, "schiffe": [{"name": "Cutlass Black", "belegung": {"a": {"ref": "x", "name": "Y"}}}]}"#,
        );
        let mut state = ShoppingState::with_files(&paths);
        let file = paths.app_file(FLEET_FILE);
        let before = std::fs::read(&file).expect("file");
        let line = |on_path: &str| Line {
            kind: Kind::Part,
            ship: "Cutlass Black".into(),
            origin: Origin::Hangar,
            position: String::new(),
            name: "Y".into(),
            reference: "x".into(),
            way: cart::BUY.into(),
            buy: cart::BuyOption {
                condition: Condition::NotChecked,
                price: None,
                shop: String::new(),
                place: String::new(),
            },
            craft: cart::CraftOption {
                condition: Condition::NoRecipe,
                material: None,
                seconds: None,
                blueprint: String::new(),
                unpriced: Vec::new(),
            },
            done: false,
            count: 1,
            path: String::new(),
            paths: vec![on_path.to_string()],
            slot_kind: String::new(),
            size: None,
            factory_ref: String::new(),
            factory_name: String::new(),
        };
        // A slot that is not there: nothing changes, so nothing is written.
        write_ticks(&mut state.view, Some(&paths), vec![(line("nowhere"), true)]);
        assert_eq!(std::fs::read(&file).expect("file"), before);
        assert!(!state.view.save_failed);
        write_ticks(&mut state.view, Some(&paths), vec![(line("a"), true)]);
        let written = std::fs::read_to_string(&file).expect("file");
        assert!(written.contains("erledigt"), "{written}");
        // The page sees it, and a change made elsewhere is taken up by the next look.
        let ship = state.view.fleet.iter().find(|(k, _)| k == "schiffe").map(|(_, v)| v.clone());
        assert!(format!("{ship:?}").contains("erledigt"), "{ship:?}");
        // ⚠ The page notices a change by the time of the file. The tick was written a moment ago, and a file
        // system that stamps coarsely gives the second write the same time: the page would not look, and the
        // test failed now and then in the full run. Written again until the time differs.
        let foreign = parse(r#"{"format": 1, "schiffe": [], "wunsch": [{"name": "Carrack"}]}"#);
        let seen = state.view.fleet_seen;
        let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            assert!(hangar::write(&file, &foreign));
            if file_time(&file) != seen || std::time::Instant::now() > end {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert_ne!(file_time(&file), seen, "the second write has a time of its own");
        let ctx = context();
        frame(&ctx, |ui| state.follow(ui));
        assert!(format!("{:?}", state.view.fleet).contains("Carrack"));
        let _ = std::fs::remove_dir_all(
            std::env::temp_dir().join(format!("versekit-einkauf-{}-tick", versekit_core::testdir::run_tag())),
        );
    }

    #[test]
    fn a_ship_with_a_plan_and_no_slot_data_is_not_called_factory_fitted() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut state = ShoppingState::default();
        state.view.fleet = parse(r#"{"schiffe": [{"name": "Galaxy", "belegung": {"a": {"ref": "x", "name": "Y"}}}]}"#);
        let (shown, _) = draw(&context(), &mut state, 900.0);
        assert!(!shown.contains(&t("s_ek_nichts_offen")), "{shown:?}");
        assert!(shown.contains(&tf("s_p9_ek_ohne_daten_eins", &[("schiffe", Arg::from("Galaxy"))])), "{shown:?}");
    }

    #[test]
    fn a_part_that_could_not_be_looked_up_is_asked_again_after_a_pause_and_not_at_every_frame() {
        let _language = crate::language_lock();
        let paths = folder("retry", r#"{"format": 1, "schiffe": []}"#);
        let mut state = ShoppingState::with_files(&paths);
        let mut line = Vec::new();
        let (reference, name) = ("sample-never", "Part");
        let mut base = cart::BuyOption {
            condition: Condition::NotChecked,
            price: None,
            shop: String::new(),
            place: String::new(),
        };
        base.condition = Condition::NotChecked;
        line.push(Line {
            kind: Kind::Part,
            ship: "S".into(),
            origin: Origin::Hangar,
            position: String::new(),
            name: name.into(),
            reference: reference.into(),
            way: cart::BUY.into(),
            buy: base,
            craft: cart::CraftOption {
                condition: Condition::NoRecipe,
                material: None,
                seconds: None,
                blueprint: String::new(),
                unpriced: Vec::new(),
            },
            done: false,
            count: 1,
            path: "p".into(),
            paths: Vec::new(),
            slot_kind: String::new(),
            size: None,
            factory_ref: String::new(),
            factory_name: String::new(),
        });
        let ctx = context();
        let view = &mut state.view.lookup;
        frame(&ctx, |ui| view.look_up(ui, Some(&paths), &line));
        assert!(view.job.is_some(), "the first frame starts the lookup");
        let asked = view.asked.get(reference).copied().expect("asked");
        // Wait for the lookup (the tests ask no net: it ends without a result).
        let end = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while view.job.is_some() && std::time::Instant::now() < end {
            frame(&ctx, |ui| view.poll(ui));
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(view.job.is_none() && view.stale);
        // Straight after: not asked again. After the pause: asked again.
        frame(&ctx, |ui| view.look_up(ui, Some(&paths), &line));
        assert!(view.job.is_none(), "no new lookup at the next frame");
        view.asked.insert(reference.to_string(), asked - RETRY_SECONDS - 1.0);
        frame(&ctx, |ui| view.look_up(ui, Some(&paths), &line));
        assert!(view.job.is_some(), "asked again after the pause");
        let _ = std::fs::remove_dir_all(
            std::env::temp_dir().join(format!("versekit-einkauf-{}-retry", versekit_core::testdir::run_tag())),
        );
    }

    #[test]
    fn the_order_of_the_stops_is_changed_and_kept() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut state = ShoppingState::default();
        // Two stops that cover one part each: the cheaper one comes first.
        state.view.fleet = parse(
            r#"{"schiffe": [{"name": "Cutlass Black", "belegung": {
                "hardpoint_cooler_01": {"ref": "sample-cooler", "name": "Frostline Cooler", "weg": "kaufen"},
                "hardpoint_radar_01": {"ref": "sample-radar", "name": "Radar X", "weg": "kaufen"}}}]}"#,
        );
        let (shown, _) = draw(&context(), &mut state, 900.0);
        let at = |shown: &[String], needle: &str| shown.iter().position(|s| s.contains(needle)).expect(needle);
        assert!(at(&shown, "Port Tressler") < at(&shown, "Area18"), "{shown:?}");
        let (stops, _) = cart::route(&cart::invoice(&state.view.fleet, &Sample).lines, &Sample);
        let ordered = cart::ordered_stops(&stops, &state.view.order);
        let (_, order) = cart::move_stop(&ordered, 0, 1, &state.view.order);
        state.view.order = order.expect("order");
        let (shown, _) = draw(&context(), &mut state, 900.0);
        assert!(at(&shown, "Area18") < at(&shown, "Port Tressler"), "{shown:?}");
    }

    /// Where the title of a stop lies after a frame (top left of the text): the one that starts with its
    /// number and names its first `part`.
    fn title_at(shapes: &[egui::epaint::ClippedShape], number: usize, part: &str) -> egui::Pos2 {
        fn find(shape: &egui::epaint::Shape, start: &str, part: &str) -> Option<egui::Pos2> {
            match shape {
                egui::epaint::Shape::Text(text)
                    if text.galley.text().starts_with(start) && text.galley.text().contains(part) =>
                {
                    Some(text.pos)
                }
                egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, start, part)),
                _ => None,
            }
        }
        let start = format!("{number}  ");
        shapes.iter().find_map(|clipped| find(&clipped.shape, &start, part)).expect(part)
    }

    /// One frame of the page with `events` coming in; the painted shapes.
    fn draw_events(ctx: &egui::Context, state: &mut ShoppingState, events: Vec<egui::Event>) -> Vec<egui::epaint::ClippedShape> {
        let scheme = versekit_core::theme::current();
        let mut crafting = CraftingState::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(900.0, 2400.0))),
                events,
                ..Default::default()
            },
            |ui| {
                crate::layout::page(ui, scheme, "cart-test", |ui| show_page(ui, scheme, state, &mut crafting));
            },
        );
        output.textures_delta.clear();
        output.shapes
    }

    /// A few frames without input, for the page to find its size; the shapes of the last one.
    fn settle(ctx: &egui::Context, state: &mut ShoppingState) -> Vec<egui::epaint::ClippedShape> {
        let mut shapes = Vec::new();
        for _ in 0..3 {
            shapes = draw_events(ctx, state, vec![]);
        }
        shapes
    }

    fn two_stops() -> ShoppingState {
        let mut state = ShoppingState::default();
        state.view.fleet = parse(
            r#"{"schiffe": [{"name": "Cutlass Black", "belegung": {
                "hardpoint_cooler_01": {"ref": "sample-cooler", "name": "Frostline Cooler", "weg": "kaufen"},
                "hardpoint_radar_01": {"ref": "sample-radar", "name": "Radar X", "weg": "kaufen"}}}]}"#,
        );
        state
    }

    /// The middle of the grip of the stop whose title text starts its line at `title`: the grip is the first
    /// thing in the row.
    fn grip_of(title: egui::Pos2) -> egui::Pos2 {
        title + vec2(widgets::ICON_BUTTON_SIZE, widgets::ICON_BUTTON_SIZE) / 2.0
    }

    /// The mouse takes the grip of the stop whose title lies at `title`, moves to `to` and lets go there.
    fn drag_grip(ctx: &egui::Context, state: &mut ShoppingState, title: egui::Pos2, to: egui::Pos2) {
        let grip = grip_of(title);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        draw_events(ctx, state, vec![egui::Event::PointerMoved(grip)]);
        draw_events(ctx, state, vec![button(grip, true)]);
        draw_events(ctx, state, vec![egui::Event::PointerMoved(grip + vec2(0.0, 6.0))]);
        draw_events(ctx, state, vec![egui::Event::PointerMoved(to)]);
        draw_events(ctx, state, vec![button(to, false)]);
        draw_events(ctx, state, vec![]);
    }

    #[test]
    fn a_stop_dragged_by_its_grip_lands_where_the_mouse_lets_go() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let ctx = context();
        let mut state = two_stops();
        let shapes = settle(&ctx, &mut state);
        let (first, second) = (title_at(&shapes, 1, "Radar X"), title_at(&shapes, 2, "Frostline Cooler"));
        assert!(first.y < second.y);
        // Let go above the middle of the first stop: nothing changes for the first one.
        drag_grip(&ctx, &mut state, first, egui::pos2(first.x, first.y - 4.0));
        assert!(state.view.order.is_empty(), "the first stop stays first: {:?}", state.view.order);
        // The first stop let go below the second one: it is the last now.
        drag_grip(&ctx, &mut state, first, egui::pos2(first.x, second.y + 40.0));
        let shapes = settle(&ctx, &mut state);
        assert!(title_at(&shapes, 1, "Frostline Cooler").y < title_at(&shapes, 2, "Radar X").y, "the order is turned");
        assert_eq!(state.view.order.len(), 2, "and kept: {:?}", state.view.order);
    }

    #[test]
    fn a_click_on_the_grip_moves_nothing() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let ctx = context();
        let mut state = two_stops();
        let shapes = settle(&ctx, &mut state);
        let first = title_at(&shapes, 1, "Radar X");
        let second = title_at(&shapes, 2, "Frostline Cooler");
        // Taken and let go on the spot, even though the mouse then lies under the second stop.
        let grip = grip_of(first);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        draw_events(&ctx, &mut state, vec![egui::Event::PointerMoved(grip), button(grip, true), button(grip, false)]);
        draw_events(&ctx, &mut state, vec![]);
        assert!(state.view.order.is_empty());
        assert!(second.y > first.y);
    }

    #[test]
    fn the_drop_lands_behind_the_stops_whose_middle_lies_above_the_mouse() {
        // Three stops 100 high; the dragged one is the first.
        let middles = [50.0, 150.0, 250.0];
        assert_eq!(drop_target(&middles, 0, -20.0), 0);
        assert_eq!(drop_target(&middles, 0, 60.0), 0);
        assert_eq!(drop_target(&middles, 0, 160.0), 1);
        assert_eq!(drop_target(&middles, 0, 900.0), 2);
        // The dragged stop does not count itself, wherever the mouse is.
        assert_eq!(drop_target(&middles, 2, 900.0), 2);
        assert_eq!(drop_target(&middles, 2, -900.0), 0);
        let rects: Vec<Rect> =
            (0..3).map(|i| Rect::from_min_size(egui::pos2(0.0, i as f32 * 100.0), vec2(200.0, 100.0))).collect();
        // The line stands above the stop it lands before, under the last one at the end.
        assert_eq!(drop_line_y(&rects, 0, 0), Some(100.0));
        assert_eq!(drop_line_y(&rects, 0, 1), Some(200.0));
        assert_eq!(drop_line_y(&rects, 0, 2), Some(300.0));
        assert_eq!(drop_line_y(&rects, 2, 2), Some(200.0));
    }

    /// The sample with no ship and the given parts from the shops.
    fn from_shops(parts: &[(&str, &str, i64)]) -> ShoppingState {
        let mut state = ShoppingState::default();
        state.view.fleet = hangar::empty();
        for (reference, name, count) in parts {
            assert!(state.view.basket.add(reference, name, *count));
        }
        state
    }

    #[test]
    fn the_parts_from_the_shops_stand_with_how_many_their_places_the_sum_and_the_route() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut state = from_shops(&[(SAMPLE_COOLER, "Frostline Cooler", 2), (SAMPLE_RADAR, "Radar X", 1)]);
        let (shown, over) = draw(&context(), &mut state, 900.0);
        assert_eq!(over, 0.0);
        assert!(!shown.contains(&t("s_ek_kein_schiff")), "parts from the shops are no empty list: {shown:?}");
        assert!(shown.contains(&tf("s_ek_korb", &[("0", Arg::from(2))])), "{shown:?}");
        assert!(shown.contains(&"Frostline Cooler".to_string()) && shown.contains(&"Radar X".to_string()), "{shown:?}");
        // The cheapest shop under the part, and every shop that sells it folded below.
        let best = tf(
            "s_wk_kauf_preis",
            &[
                ("preis", Arg::from(plain(22_700.0))),
                ("laden", Arg::from("Dumper's Depot")),
                ("ort", Arg::from("Area18")),
            ],
        );
        assert!(shown.contains(&best), "{shown:?}");
        assert!(shown.contains(&tf("s_ek_korb_orte", &[("0", Arg::from(2))])), "{shown:?}");
        assert!(shown.contains(&tf("s_ek_korb_orte", &[("0", Arg::from(1))])), "{shown:?}");
        // The sum counts how many: two coolers and one radar.
        let total = 2.0 * 22_700.0 + 8_400.0;
        assert!(shown.contains(&tf("s_wk_summe", &[("preis", Arg::from(plain(total)))])), "{shown:?}");
        assert!(shown.contains(&t("s_wk_route")), "{shown:?}");
        assert!(shown.iter().any(|s| s.ends_with("  2 × Frostline Cooler")), "{shown:?}");
        assert!(shown.iter().any(|s| s.ends_with("  Radar X")), "{shown:?}");
        assert!(shown.iter().any(|s| s.ends_with("Dumper's Depot")), "{shown:?}");
        assert!(shown.iter().any(|s| s.ends_with("Tammany and Sons")), "{shown:?}");
        assert!(shown.contains(&t("s_ek_vorlagen")), "a list with parts can be kept: {shown:?}");
    }

    #[test]
    fn the_parts_of_the_ships_and_of_the_shops_make_one_sum_and_one_route() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut state = from_shops(&[(SAMPLE_COOLER, "Frostline Cooler", 2)]);
        state.view.fleet = parse(
            r#"{"schiffe": [{"name": "Cutlass Black", "belegung": {
                "hardpoint_cooler_01": {"ref": "sample-cooler", "name": "Frostline Cooler", "weg": "kaufen"}}}]}"#,
        );
        let (shown, _) = draw(&context(), &mut state, 900.0);
        assert!(shown.contains(&"Cutlass Black".to_string()), "{shown:?}");
        assert!(shown.contains(&tf("s_ek_korb", &[("0", Arg::from(1))])), "{shown:?}");
        let total = 3.0 * 22_700.0;
        assert!(shown.contains(&tf("s_wk_summe", &[("preis", Arg::from(plain(total)))])), "{shown:?}");
        assert!(
            shown.iter().any(|s| s == "1  3 × Frostline Cooler"),
            "one stop for all three: {shown:?}"
        );
    }

    #[test]
    fn an_empty_list_points_to_the_shops_and_still_shows_the_templates() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut state = from_shops(&[]);
        let (shown, _) = draw(&context(), &mut state, 900.0);
        assert!(shown.contains(&t("s_ek_kein_schiff")) && shown.contains(&t("s_ek_korb_leer")), "{shown:?}");
        // No parts: an empty template can be made, none from the list.
        assert!(shown.contains(&t("s_ek_vorlagen")) && shown.contains(&t("s_ek_vorlage_neu")), "{shown:?}");
        assert!(!shown.contains(&t("s_ek_vorlage_speichern")), "nothing on the list to take: {shown:?}");
        // A kept list can be loaded while the list is empty.
        let mut kept = Basket::default();
        assert!(kept.add(SAMPLE_COOLER, "Frostline Cooler", 3) && kept.save_set("FPS-Set Schwer"));
        assert!(kept.new_set("Leer"));
        state.view.basket.sets = kept.sets;
        let (shown, _) = draw(&context(), &mut state, 900.0);
        assert!(shown.contains(&t("s_ek_vorlagen")) && shown.contains(&"FPS-Set Schwer".to_string()), "{shown:?}");
        assert!(shown.contains(&tf("s_ek_vorlage_teile", &[("0", Arg::from(3))])), "{shown:?}");
        assert!(shown.contains(&t("s_ek_vorlage_ohne_teile")), "an empty template says so: {shown:?}");
    }

    type Spots = Vec<(String, Rect)>;

    /// The sample page frame after frame, with clicks: the texts and the named controls with where they stand.
    struct Bench {
        ctx: egui::Context,
        state: ShoppingState,
        crafting: CraftingState,
    }

    impl Bench {
        fn new(state: ShoppingState) -> Bench {
            let ctx = context();
            ctx.enable_accesskit();
            Bench { ctx, state, crafting: CraftingState::default() }
        }

        fn frame(&mut self, events: Vec<egui::Event>) -> (Spots, Spots) {
            let scheme = versekit_core::theme::current();
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(900.0, 2400.0))),
                events,
                ..Default::default()
            };
            let (state, crafting) = (&mut self.state, &mut self.crafting);
            let mut output = self.ctx.run_ui(input, |ui| {
                crate::layout::page(ui, scheme, "cart-bench", |ui| show_page(ui, scheme, state, crafting));
            });
            output.textures_delta.clear();
            let nodes = output
                .platform_output
                .accesskit_update
                .map(|update| {
                    update
                        .nodes
                        .iter()
                        .filter_map(|(_, node)| {
                            let b = node.bounds()?;
                            let rect = Rect::from_min_max(
                                egui::pos2(b.x0 as f32, b.y0 as f32),
                                egui::pos2(b.x1 as f32, b.y1 as f32),
                            );
                            Some((node.label().unwrap_or_default().to_string(), rect))
                        })
                        .collect()
                })
                .unwrap_or_default();
            (placed(&output.shapes), nodes)
        }

        fn settle(&mut self) -> Vec<String> {
            let mut last = Vec::new();
            for _ in 0..3 {
                last = self.frame(Vec::new()).0;
            }
            last.into_iter().map(|(text, _)| text).collect()
        }

        fn press_at(&mut self, at: egui::Pos2) {
            let button = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            self.frame(vec![egui::Event::PointerMoved(at)]);
            self.frame(vec![button(true)]);
            self.frame(vec![button(false)]);
            self.settle();
        }

        /// Clicks the text that reads `label`.
        fn click(&mut self, label: &str) {
            self.settle();
            let texts = self.frame(Vec::new()).0;
            let at = texts
                .iter()
                .find(|(text, _)| text == label)
                .map(|(_, rect)| rect.center())
                .unwrap_or_else(|| panic!("{label} is not on the screen: {texts:?}"));
            self.press_at(at);
        }

        /// Clicks the control whose name for a screen reader holds every one of `words`.
        fn click_named(&mut self, words: &[&str]) {
            self.settle();
            let nodes = self.frame(Vec::new()).1;
            let rect = nodes
                .iter()
                .find(|(name, _)| words.iter().all(|w| name.contains(w)))
                .map(|(_, rect)| *rect)
                .unwrap_or_else(|| panic!("no control named with {words:?}: {nodes:?}"));
            self.press_at(rect.center());
        }
    }

    #[test]
    fn a_part_is_ticked_off_as_bought_and_taken_out_of_the_list() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut bench = Bench::new(from_shops(&[(SAMPLE_COOLER, "Frostline Cooler", 2), (SAMPLE_RADAR, "Radar X", 1)]));
        bench.click_named(&["Frostline Cooler", &t("s_ek_korb_abhaken")]);
        assert!(bench.state.view.basket.items.iter().any(|i| i.reference == SAMPLE_COOLER && i.done));
        let shown = bench.settle();
        assert!(shown.contains(&tf("s_ek_korb_gekauft", &[("0", Arg::from(1))])), "{shown:?}");
        // What is bought is no longer in the sum.
        assert!(shown.contains(&tf("s_wk_summe", &[("preis", Arg::from(plain(8_400.0)))])), "{shown:?}");
        bench.click_named(&["Radar X", &t("s_ek_korb_entfernen")]);
        let left: Vec<&str> = bench.state.view.basket.items.iter().map(|i| i.reference.as_str()).collect();
        assert_eq!(left, [SAMPLE_COOLER]);
    }

    #[test]
    fn a_list_is_kept_as_a_template_loaded_again_and_taken_away() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut bench = Bench::new(from_shops(&[(SAMPLE_COOLER, "Frostline Cooler", 2)]));
        // Without a name the button keeps nothing.
        bench.click(&t("s_ek_vorlage_speichern"));
        assert!(bench.state.view.basket.sets.is_empty());
        bench.state.view.template_name = "  FPS-Set Schwer ".into();
        bench.click(&t("s_ek_vorlage_speichern"));
        let names: Vec<&str> = bench.state.view.basket.sets.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["FPS-Set Schwer"]);
        let shown = bench.settle();
        assert!(shown.contains(&tf("s_ek_vorlage_gespeichert", &[("0", Arg::from("FPS-Set Schwer"))])), "{shown:?}");
        // Other parts meanwhile; loading brings back the kept ones in their place.
        assert!(bench.state.view.basket.set_count(SAMPLE_COOLER, 0));
        assert!(bench.state.view.basket.add(SAMPLE_RADAR, "Radar X", 4));
        bench.click_named(&["FPS-Set Schwer", &t("s_ek_vorlage_laden")]);
        let items: Vec<(&str, i64)> =
            bench.state.view.basket.items.iter().map(|i| (i.reference.as_str(), i.count)).collect();
        assert_eq!(items, [(SAMPLE_COOLER, 2)]);
        bench.click_named(&["FPS-Set Schwer", &t("s_ek_vorlage_loeschen")]);
        assert!(bench.state.view.basket.sets.is_empty());
        let shown = bench.settle();
        assert!(shown.contains(&tf("s_ek_vorlage_geloescht", &[("0", Arg::from("FPS-Set Schwer"))])), "{shown:?}");
        // Taken back from the notice: the template is there again with its parts.
        bench.click(&t("s_mlg_rueckgaengig"));
        let kept: Vec<(&str, usize)> =
            bench.state.view.basket.sets.iter().map(|s| (s.name.as_str(), s.items.len())).collect();
        assert_eq!(kept, [("FPS-Set Schwer", 1)]);
    }

    /// The parts of a template as `(code, count)`.
    fn parts_of(state: &ShoppingState, name: &str) -> Vec<(String, i64)> {
        let set = state.view.basket.set(name).unwrap_or_else(|| panic!("no template {name}"));
        set.items.iter().map(|i| (i.reference.clone(), i.count)).collect()
    }

    #[test]
    fn an_empty_template_is_filled_by_the_search_changed_part_by_part_and_put_on_the_list() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut bench = Bench::new(from_shops(&[]));
        let name = "Rüstung Mamba";
        bench.state.view.template_name = name.into();
        bench.click(&t("s_ek_vorlage_neu"));
        assert!(parts_of(&bench.state, name).is_empty(), "an empty template");
        assert!(bench.state.view.template_name.is_empty(), "the field is empty for the next one");
        let shown = bench.settle();
        assert!(shown.contains(&t("s_ek_vorlage_leer")), "made and unfolded at once: {shown:?}");
        assert!(shown.contains(&tf("s_ek_vorlage_angelegt", &[("0", Arg::from(name))])), "{shown:?}");
        // The same name again makes no second one.
        bench.state.view.template_name = name.into();
        bench.click(&t("s_ek_vorlage_neu"));
        assert_eq!(bench.state.view.basket.sets.len(), 1);
        assert!(bench.settle().contains(&tf("s_ek_vorlage_gibt_es", &[("0", Arg::from(name))])));
        // The search finds as the shops page does: one letter is nothing, the translated group is found.
        let search = |bench: &mut Bench, typed: &str| {
            bench.state.view.templates.search.insert(name.into(), typed.into());
            bench.settle()
        };
        // One letter: no list, and no "nothing found" either.
        let shown = search(&mut bench, "k");
        assert!(!shown.contains(&"Frostline Cooler".to_string()), "{shown:?}");
        assert!(!shown.contains(&t("s_ek_vorlage_kein_treffer")), "{shown:?}");
        let shown = search(&mut bench, "zzz");
        assert!(shown.contains(&t("s_ek_vorlage_kein_treffer")), "{shown:?}");
        let shown = search(&mut bench, "kühl");
        assert!(shown.contains(&"Frostline Cooler".to_string()), "{shown:?}");
        assert!(!shown.contains(&"Arc Shield".to_string()), "only what fits: {shown:?}");
        bench.click_named(&["Frostline Cooler", &t("s_ek_vorlage_hinzufuegen")]);
        bench.click_named(&["Frostline Cooler", &t("s_ek_vorlage_hinzufuegen")]);
        assert_eq!(parts_of(&bench.state, name), [(SAMPLE_COOLER.to_string(), 2)], "the same part adds up");
        // − and + change the count (the cooler is the only part with a count on the page).
        search(&mut bench, "");
        bench.click_named(&[&t("s_ff_mehr")]);
        assert_eq!(parts_of(&bench.state, name)[0], (SAMPLE_COOLER.to_string(), 3));
        bench.click_named(&[&t("s_ff_weniger")]);
        assert_eq!(parts_of(&bench.state, name)[0], (SAMPLE_COOLER.to_string(), 2));
        search(&mut bench, "shield gen");
        bench.click_named(&["Arc Shield", &t("s_ek_vorlage_hinzufuegen")]);
        // The parts stand in the unfolded template with their price and where; the head counts them.
        let shown = search(&mut bench, "");
        assert!(shown.contains(&cooler_line()), "{shown:?}");
        assert!(shown.contains(&"Arc Shield".to_string()), "{shown:?}");
        assert!(shown.contains(&tf("s_ek_vorlage_teile", &[("0", Arg::from(3))])), "{shown:?}");
        // The bin takes the part out.
        bench.click_named(&["Frostline Cooler", &t("s_ek_vorlage_teil_entfernen")]);
        assert_eq!(parts_of(&bench.state, name), [(SAMPLE_SHIELD.to_string(), 1)]);
        // Put on the list: exactly the parts of the template, a part on the list before is gone.
        assert!(bench.state.view.basket.add(SAMPLE_RADAR, "Radar X", 4));
        bench.click_named(&[name, &t("s_ek_vorlage_laden")]);
        let items: Vec<(&str, i64)> =
            bench.state.view.basket.items.iter().map(|i| (i.reference.as_str(), i.count)).collect();
        assert_eq!(items, [(SAMPLE_SHIELD, 1)]);
        assert_eq!(parts_of(&bench.state, name), [(SAMPLE_SHIELD.to_string(), 1)], "the template stays");
    }

    #[test]
    fn a_template_is_renamed_and_folded() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut bench = Bench::new(from_shops(&[(SAMPLE_COOLER, "Frostline Cooler", 2)]));
        bench.state.view.template_name = "F8C".into();
        bench.click(&t("s_ek_vorlage_speichern"));
        // The list is emptied, so that the price line of the cooler stands only in the template.
        assert!(bench.state.view.basket.set_count(SAMPLE_COOLER, 0));
        bench.state.view.template_name = "Rüstung Mamba".into();
        bench.click(&t("s_ek_vorlage_neu"));
        bench.click_named(&["F8C", &t("s_ek_vorlage_umbenennen")]);
        assert_eq!(bench.state.view.templates.renaming, Some(("F8C".to_string(), "F8C".to_string())));
        // A name another template has is refused.
        bench.state.view.templates.renaming = Some(("F8C".into(), "Rüstung Mamba".into()));
        bench.click_named(&["F8C", &t("s_ek_vorlage_name_uebernehmen")]);
        assert!(bench.state.view.basket.set("F8C").is_some());
        assert!(bench.settle().contains(&tf("s_ek_vorlage_gibt_es", &[("0", Arg::from("Rüstung Mamba"))])));
        bench.state.view.templates.renaming = Some(("F8C".into(), " F8C Ausstattung ".into()));
        bench.click_named(&["F8C", &t("s_ek_vorlage_name_uebernehmen")]);
        let names: Vec<&str> = bench.state.view.basket.sets.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["F8C Ausstattung", "Rüstung Mamba"]);
        assert_eq!(parts_of(&bench.state, "F8C Ausstattung"), [(SAMPLE_COOLER.to_string(), 2)]);
        let shown = bench.settle();
        let said = tf("s_ek_vorlage_umbenannt", &[("0", Arg::from("F8C")), ("1", Arg::from("F8C Ausstattung"))]);
        assert!(shown.contains(&said), "{shown:?}");
        // Unfolded under its old name, unfolded under its new one; the arrow folds it.
        assert!(bench.state.view.templates.open.contains("F8C Ausstattung"));
        bench.click_named(&["F8C Ausstattung", &t("s_ek_vorlage_zuklappen")]);
        assert!(!bench.state.view.templates.open.contains("F8C Ausstattung"));
        assert!(!bench.settle().contains(&cooler_line()), "folded: the part is not shown");
        bench.click_named(&["F8C Ausstattung", &t("s_ek_vorlage_aufklappen")]);
        assert!(bench.settle().contains(&cooler_line()));
    }

    /// The price line of the sample cooler.
    fn cooler_line() -> String {
        tf(
            "s_wk_kauf_preis",
            &[
                ("preis", Arg::from(plain(22_700.0))),
                ("laden", Arg::from("Dumper's Depot")),
                ("ort", Arg::from("Area18")),
            ],
        )
    }

    /// Every line of text of a frame that is shortened with "…" or reaches out of its room at the side.
    fn cut_lines(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
        fn walk(shape: &egui::epaint::Shape, clip: Rect, out: &mut Vec<String>) {
            match shape {
                egui::epaint::Shape::Text(text) => {
                    if text.galley.elided {
                        out.push(format!("gekürzt: {:?}", text.galley.text()));
                    }
                    for row in &text.galley.rows {
                        let rect = row.rect_without_leading_space().translate(text.pos.to_vec2());
                        let inside = rect.min.x >= clip.min.x - 0.5 && rect.max.x <= clip.max.x + 0.5;
                        if !row.row.text().trim().is_empty() && !inside {
                            out.push(format!("abgeschnitten: {:?}", row.row.text()));
                        }
                    }
                }
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, clip, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        shapes.iter().for_each(|clipped| walk(&clipped.shape, clipped.clip_rect, &mut out));
        out
    }

    #[test]
    fn the_cut_check_finds_a_shortened_and_a_cut_off_text() {
        let ctx = context();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.set_max_width(120.0);
            ui.add(egui::Label::new("A text far too long for its little room here").truncate());
            ui.allocate_ui(vec2(60.0, 20.0), |ui| {
                ui.set_clip_rect(ui.max_rect());
                ui.add(egui::Label::new("Wide words cut at the side").extend());
            });
        });
        output.textures_delta.clear();
        let cut = cut_lines(&output.shapes);
        assert!(cut.iter().any(|c| c.starts_with("gekürzt")), "{cut:?}");
        assert!(cut.iter().any(|c| c.starts_with("abgeschnitten")), "{cut:?}");
    }

    #[test]
    fn an_unfolded_template_shows_its_parts_and_cuts_nothing_at_every_size_and_width_in_both_languages() {
        let _language = crate::language_lock();
        let mut findings = Vec::new();
        let long = "Rüstung der Staffel Mamba mit einem sehr langen Namen für die Vorlage";
        for lang in [Choice::De, Choice::En] {
            set_language(lang);
            for points in versekit_core::font_size::POINTS_RANGE.0..=versekit_core::font_size::POINTS_RANGE.1 {
                for width in [380.0, 520.0, 900.0] {
                    let ctx = context();
                    crate::apply_size(&ctx, points);
                    let mut state = from_shops(&[(SAMPLE_COOLER, "Frostline Cooler", 2)]);
                    let basket = &mut state.view.basket;
                    assert!(basket.new_set(long) && basket.new_set("F8C") && basket.new_set("Leer"));
                    assert!(basket.add_to_set(
                        long,
                        SAMPLE_COOLER,
                        "Hurston Dynamics Predator Cooler Extra Long Name",
                        999
                    ));
                    assert!(basket.add_to_set(long, SAMPLE_SHIELD, "Arc Shield", 1));
                    assert!(basket.add_to_set(long, "uuid-unknown", "Part No Shop Knows", 3));
                    assert!(basket.add_to_set("F8C", SAMPLE_RADAR, "Radar X", 2));
                    let view = &mut state.view.templates;
                    view.open.extend([long.to_string(), "F8C".to_string(), "Leer".to_string()]);
                    view.search.insert("F8C".into(), "co".into());
                    view.renaming = Some(("Leer".into(), "Ein neuer, recht langer Name".into()));
                    let (shown, over) = (0..3).map(|_| draw(&ctx, &mut state, width)).last().expect("frames");
                    let at = format!("{lang:?} Größe {points} Breite {width}");
                    for wanted in [
                        "Hurston Dynamics Predator Cooler Extra Long Name",
                        "Arc Shield",
                        "Part No Shop Knows",
                        "Radar X",
                    ] {
                        if !shown.iter().any(|s| s.contains(wanted)) {
                            findings.push(format!("{at}: {wanted} fehlt"));
                        }
                    }
                    if !shown.contains(&t("s_wk_nicht_geprueft")) {
                        findings.push(format!("{at}: ein Teil ohne Laden sagt es nicht"));
                    }
                    if over > 0.5 {
                        findings.push(format!("{at}: {over:.0} zu breit"));
                    }
                    let scheme = versekit_core::theme::current();
                    let mut crafting = CraftingState::default();
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 2400.0))),
                            ..Default::default()
                        },
                        |ui| {
                            crate::layout::page(ui, scheme, "cart-test", |ui| {
                                show_page(ui, scheme, &mut state, &mut crafting)
                            });
                        },
                    );
                    output.textures_delta.clear();
                    findings.extend(cut_lines(&output.shapes).into_iter().map(|c| format!("{at}: {c}")));
                    if let Some(found) = first_overlap(&placed(&output.shapes)) {
                        findings.push(format!("{at}: {found}"));
                    }
                }
            }
        }
        set_language(Choice::De);
        assert!(findings.is_empty(), "{}", findings.join("\n"));
    }

    #[test]
    fn a_stop_is_ticked_off_on_the_way_and_the_ticks_of_the_route_are_taken_back() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut bench = Bench::new(from_shops(&[(SAMPLE_COOLER, "Frostline Cooler", 1), (SAMPLE_RADAR, "Radar X", 1)]));
        let shown = bench.settle();
        assert!(!shown.iter().any(|s| s.contains(&t("s_ek_stopps_zuruecksetzen"))), "nothing ticked yet: {shown:?}");
        bench.click_named(&["Area18", &t("s_ek_stopp_abhaken")]);
        assert_eq!(bench.state.view.basket.stops_done, ["Stanton|Area18"]);
        let shown = bench.settle();
        assert!(
            shown.iter().any(|s| s.contains("Frostline Cooler") && s.contains(&t("s_ek_stopp_erledigt"))),
            "{shown:?}"
        );
        let said = tf("s_ek_stopps_erledigt", &[("0", Arg::from(1)), ("1", Arg::from(2))]);
        assert!(shown.contains(&said), "{shown:?}");
        // The tick is taken back on the stop itself, and all at once below the route.
        bench.click_named(&["Area18", &t("s_ek_stopp_offen")]);
        assert!(bench.state.view.basket.stops_done.is_empty());
        bench.click_named(&["Area18", &t("s_ek_stopp_abhaken")]);
        bench.click_named(&["Port Tressler", &t("s_ek_stopp_abhaken")]);
        assert_eq!(bench.state.view.basket.stops_done.len(), 2);
        bench.click(&t("s_ek_stopps_zuruecksetzen"));
        assert!(bench.state.view.basket.stops_done.is_empty());
        // The ticks of the route are not the ticks of the parts.
        assert!(bench.state.view.basket.items.iter().all(|i| !i.done));
    }

    /// Where every text of a frame stands.
    fn placed(shapes: &[egui::epaint::ClippedShape]) -> Vec<(String, egui::Rect)> {
        fn collect(shape: &egui::epaint::Shape, out: &mut Vec<(String, egui::Rect)>) {
            match shape {
                // Row by row: a text that wraps has a box that holds the texts beside its first row.
                egui::epaint::Shape::Text(text) => {
                    for placed in &text.galley.rows {
                        let row_text = placed.row.text();
                        if !row_text.trim().is_empty() {
                            out.push((
                                row_text.to_string(),
                                placed.rect_without_leading_space().translate(text.pos.to_vec2()),
                            ));
                        }
                    }
                }
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut out));
        out
    }

    /// The first two texts that lie on each other, if any.
    fn first_overlap(texts: &[(String, egui::Rect)]) -> Option<String> {
        for (i, (a, ra)) in texts.iter().enumerate() {
            for (b, rb) in &texts[i + 1..] {
                let both = ra.intersect(*rb);
                if both.width() > 1.0 && both.height() > 1.0 {
                    return Some(format!("{a:?} {ra:?} liegt auf {b:?} {rb:?}"));
                }
            }
        }
        None
    }

    #[test]
    fn no_text_lies_on_another_with_long_names_and_big_amounts() {
        let _language = crate::language_lock();
        let mut findings = Vec::new();
        for lang in [Choice::De, Choice::En] {
            set_language(lang);
            for points in versekit_core::font_size::POINTS_RANGE.0..=versekit_core::font_size::POINTS_RANGE.1 {
                for width in (380..=900).step_by(65).map(|w| w as f32) {
                    let ctx = context();
                    crate::apply_size(&ctx, points);
                    let mut state = ShoppingState::default();
                    // Names of every length, so that some end near the amount at some width.
                    state.view.fleet = parse(
                        r#"{"schiffe": [{"name": "Constellation Andromeda Extra Long Edition", "belegung": {
                              "hardpoint_cooler_01": {"ref": "sample-cooler", "name": "Hurston Dynamics Predator Cooler Extra Long Name", "weg": "kaufen"},
                              "hardpoint_shield_01": {"ref": "sample-shield", "name": "Arc Shield of the Very Long Name Edition", "weg": "bauen"},
                              "hardpoint_quantum_drive": {"ref": "sample-drive", "name": "Quantum Drive Mk2 Long", "weg": "kaufen"},
                              "hardpoint_radar_01": {"ref": "sample-radar", "name": "Radar of Twelve", "weg": "kaufen"}}}],
                            "wunsch": [{"name": "Carrack", "belegung": {
                              "hardpoint_cooler_01": {"ref": "sample-cooler", "name": "Frostline Cooler Extended", "weg": "kaufen"},
                              "hardpoint_shield_01": {"ref": "sample-shield", "name": "Arc Shield Mk", "weg": "bauen"},
                              "hardpoint_radar_01": {"ref": "sample-radar", "name": "Hurston Dynamics Radar Array Extra Long Name Edition", "weg": "kaufen"}}}]}"#,
                    );
                    // Parts from the shops and a kept list as well, with a long name and a big count.
                    let basket = &mut state.view.basket;
                    assert!(basket.add(SAMPLE_COOLER, "Hurston Dynamics Predator Cooler From The Shop", 999));
                    assert!(basket.add(SAMPLE_SHIELD, "Arc Shield", 1));
                    assert!(basket.save_set("FPS-Set Schwer mit einem sehr langen Namen für die Vorlage"));
                    assert!(basket.set_done(SAMPLE_SHIELD, true));
                    let scheme = versekit_core::theme::current();
                    let mut crafting = CraftingState::default();
                    let mut last = Vec::new();
                    for _ in 0..3 {
                        let mut output = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 2400.0))),
                                ..Default::default()
                            },
                            |ui| {
                                crate::layout::page(ui, scheme, "cart-overlap", |ui| {
                                    show_page(ui, scheme, &mut state, &mut crafting)
                                });
                            },
                        );
                        output.textures_delta.clear();
                        last = placed(&output.shapes);
                    }
                    if let Some(found) = first_overlap(&last) {
                        findings.push(format!("{lang:?} Größe {points} Breite {width}: {found}"));
                    }
                }
            }
        }
        set_language(Choice::De);
        assert!(findings.is_empty(), "{}", findings.join("\n"));
    }

    #[test]
    fn the_overlap_check_finds_two_texts_on_each_other() {
        let a = ("a".to_string(), Rect::from_min_size(egui::Pos2::ZERO, vec2(50.0, 10.0)));
        let b = ("b".to_string(), Rect::from_min_size(egui::pos2(40.0, 0.0), vec2(50.0, 10.0)));
        let c = ("c".to_string(), Rect::from_min_size(egui::pos2(60.0, 0.0), vec2(50.0, 10.0)));
        assert!(first_overlap(&[a.clone(), b]).is_some());
        assert!(first_overlap(&[a, c]).is_none());
    }

    #[test]
    fn the_page_fits_at_every_size_and_width_in_both_languages() {
        let _language = crate::language_lock();
        let mut findings = Vec::new();
        for lang in [Choice::De, Choice::En] {
            set_language(lang);
            for points in versekit_core::font_size::POINTS_RANGE.0..=versekit_core::font_size::POINTS_RANGE.1 {
                for width in [380.0, 520.0, 900.0] {
                    let ctx = context();
                    crate::apply_size(&ctx, points);
                    let mut state = ShoppingState::default();
                    // The parts from the shops and the templates below the ships.
                    assert!(state.view.basket.add(SAMPLE_COOLER, "Frostline Cooler", 12));
                    assert!(state.view.basket.save_set("FPS-Set Schwer"));
                    let (_, over) = (0..3).map(|_| draw(&ctx, &mut state, width)).last().expect("frames");
                    if over > 0.5 {
                        findings.push(format!("{lang:?} Größe {points} Breite {width}: {over:.0} zu breit"));
                    }
                }
            }
        }
        set_language(Choice::De);
        assert!(findings.is_empty(), "{}", findings.join("\n"));
    }
}
