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

//! The crafting page (`pages._crafting` of 4.12), laid out like the crafting
//! terminal of the game: on the left the list to choose from (search,
//! categories of the terminal, filters, the items under their
//! subcategories), then the recipe of the chosen item (count, autofill, one
//! block per slot with the quality of its material) and the item itself with
//! the values that follow the qualities. Narrow windows stack them, the
//! narrowest show the list or the chosen item with a way back.
//!
//! It answers one question, "I want to craft that, what do I need?", and it
//! knows what scmdb.net cannot: which blueprints one has and what lies in the
//! own stock. It never says "you cannot craft this": the stock is kept by
//! hand and may be behind, so it says what is there and what is missing.
//!
//! The values come from the game data (`versekit_gamedata::values_store`):
//! the ones read last are shown at once, new ones are read in a thread.

mod list;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::SystemTime;

use egui::accesskit::Role;
use egui::{RichText, Ui, WidgetInfo};
use versekit_core::catalog_cache::CatalogCache;
use versekit_core::erkul_slots::{self, FitAnswer};
use versekit_core::fabricator::{self, Names, Place};
use versekit_core::formats;
use versekit_core::hangar;
use versekit_core::jsonio::{Map, Value};
use versekit_core::language::{self, Arg, t, tf};
use versekit_core::notepad;
use versekit_core::paths::{Paths, name_key};
use versekit_core::quality_mix::{self, Lot};
use versekit_core::raw_prices::{BUY_QUALITY, RawPrices};
use versekit_core::ships::{ERKUL_FILE, ErkulShips};
use versekit_core::stock::{self, py_round};
use versekit_core::stock_data::StockData;
use versekit_core::stock_use::{self, Ingredient, Order};
use versekit_core::theme::{Color, Scheme};
use versekit_gamedata::craft_book::{self, CraftBook, CraftItem, Have};
use versekit_gamedata::product_table::{self, Line, Rating};
use versekit_gamedata::quality;
use versekit_gamedata::values::{GameValues, Recipe, Slot};
use versekit_gamedata::values_store::{self, Pending, Progress};

use crate::color;
use crate::icons;
use crate::layout;
use crate::widgets::{self, ButtonKind};

use list::{Sort, View};

/// How often the page looks whether its files changed.
const LOOK_SECONDS: f64 = 1.0;
/// The setting that remembers which end of the qualities the stock is taken from.
const SETTING_FILL: &str = "herstellung_fuellen";
/// The setting that remembers whether the list shows tiles or rows.
const SETTING_VIEW: &str = "herstellung_ansicht";
/// The farm notes live in the hangar file.
const NOTES_FILE: &str = "hangar.json";
/// The least height of a row of the list.
const ROW_HEIGHT: f32 = 24.0;
/// The quality a material starts at that is not in the stock: the quality of bought goods.
const START_QUALITY: f64 = 500.0;
/// The highest quality.
const BEST_QUALITY: f64 = 1000.0;
/// The widths the sides of the page need, in lines of body text: the list,
/// the recipe and the item.
const LIST_EM: f32 = 19.0;
const SIDE_EM: f32 = 21.0;

/// What a click on the page asks of the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Show where the blueprint of this item comes from.
    ToBlueprint(String),
    /// Show where this material is found.
    ToMining(String),
    /// Show the shops.
    ToShops,
}

/// The picks of the filters.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Picks {
    /// The category of the terminal (index into [`fabricator::CATEGORIES`]).
    category: Option<usize>,
    /// Its subcategory; `None` is all of the category.
    sub: Option<&'static str>,
    /// The blueprint: empty, `habe` or `fehlt`.
    state: String,
    /// The material in the stock: empty, `reicht` or `fehlt`.
    material: String,
    /// The name of the hangar ship whose slots the items have to fit; empty is every item.
    ship: String,
}

/// What the ship of the filter lets through, worked out once for the list.
#[derive(Clone, Debug, Default, PartialEq)]
enum ShipFit {
    /// No ship is chosen.
    #[default]
    Off,
    /// The slots of the chosen ship are not known: nothing can be said, so nothing passes.
    NoData,
    /// The items that fit a slot of the ship, with the number of slots each fits.
    Parts(HashMap<usize, i64>),
}

/// Which feedback line stands under the buttons.
#[derive(Clone, Debug, PartialEq)]
enum Feedback {
    /// Taken out of the stock this many times.
    Deducted(i64),
    /// Nothing was taken: what is missing, as (material, amount).
    Missing(Vec<(String, f64)>),
    /// The stock could not be written.
    NotSaved,
}

/// What belongs to the chosen item and is lost with it.
#[derive(Clone, Debug)]
struct Detail {
    /// The index in [`CraftBook::items`].
    item: usize,
    count: String,
    /// The quality set per material; for a mixed material the quality of its mix.
    stand: HashMap<String, f64>,
    /// The lots per material that is mixed from more than one, else absent.
    mix: HashMap<String, Vec<Part>>,
    /// The materials whose quality follows the stock, not a hand.
    auto: HashSet<String>,
    /// The quality the terminal would take from the stock per material.
    from_stock: HashMap<String, f64>,
    /// The lots per piece the stock fills each material with.
    from_stock_lots: HashMap<String, Vec<Lot>>,
    /// The quality of what the stock does not cover, per material.
    rest: HashMap<String, f64>,
    /// Whether a material stood in the stock when the quality was last taken.
    in_stock: HashMap<String, bool>,
    feedback: Option<Feedback>,
    /// The typed text of a quality field per material, while one is typed.
    typed: HashMap<String, String>,
    /// The stock the qualities were taken from (its change number).
    stock_version: u64,
    /// The count the qualities were taken for.
    counted: i64,
    /// The materials whose quality field holds something that is no quality.
    bad: HashSet<String>,
}

/// One lot of a mixed material: amount per piece and quality, with the typed
/// text of both.
#[derive(Clone, Debug, PartialEq)]
struct Part {
    amount: f64,
    quality: f64,
    amount_text: String,
    quality_text: String,
    /// Whether the lot lies in the stock as it is.
    stocked: bool,
}

impl Part {
    fn new(amount: f64, quality: f64, stocked: bool) -> Part {
        Part { amount, quality, amount_text: g(rounded3(amount)), quality_text: py_g_text(quality), stocked }
    }

    fn lot(&self) -> Lot {
        Lot { amount: self.amount, quality: self.quality }
    }
}

/// How the qualities are taken anew.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Refill {
    /// The materials that follow the stock follow it; what was set by hand stays.
    Follow,
    /// Everything from the stock again; what is not in it starts at [`START_QUALITY`].
    Stock,
    /// Everything from the stock again; what is not in it gets the best quality
    /// or the lowest the recipe takes, after the fill.
    Fill,
}

/// A material the search text names and the names of the items that need it.
type MaterialHit = (String, Vec<String>);

/// Why the game values are not there (see [`CraftingState::values_wait`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValuesWait {
    Reading,
    Failed,
    None,
}

/// Class, size and grade of every item of `book` from the blueprint catalog,
/// as the blueprint list writes them: size and grade only for kinds where
/// they differ. Beside it the size of every item where the catalog knows one,
/// for the order of the list.
fn spec_lines(book: &CraftBook, paths: &Paths) -> (Vec<String>, Vec<Option<i64>>) {
    use versekit_core::{blueprints as bp, catalog_cache};
    let Some(catalog) = catalog_cache::load_current(paths) else {
        return (vec![String::new(); book.items.len()], vec![None; book.items.len()]);
    };
    let size_kinds = bp::kinds_with_spread(&catalog, |b| b.size);
    let grade_kinds = bp::kinds_with_spread(&catalog, |b| b.grade);
    let by_name: HashMap<String, &catalog_cache::Blueprint> =
        catalog.blueprints.iter().map(|b| (b.name.to_lowercase(), b)).collect();
    let sizes = book.items.iter().map(|item| by_name.get(&item.basis.to_lowercase()).and_then(|b| b.size)).collect();
    let lines = book
        .items
        .iter()
        .map(|item| {
            let Some(blueprint) = by_name.get(&item.basis.to_lowercase()) else { return String::new() };
            let kind = bp::kind_id(&blueprint.kind);
            let mut parts: Vec<String> = blueprint.class.iter().cloned().collect();
            if let Some(size) = blueprint.size.filter(|_| size_kinds.contains(kind)) {
                parts.push(format!("S{size}"));
            }
            if let Some(letter) =
                blueprint.grade.filter(|_| grade_kinds.contains(kind)).and_then(catalog_cache::grade_letter)
            {
                parts.push(letter.to_string());
            }
            parts.join(" · ")
        })
        .collect();
    (lines, sizes)
}

/// What the crafting page holds.
pub struct CraftingState {
    paths: Option<Paths>,
    values: Option<Arc<GameValues>>,
    book: CraftBook,
    data: StockData,
    /// The place of every item in the categories of the terminal.
    places: Vec<Place>,
    /// Class, size and grade of every item as a short line, empty where the
    /// catalog knows none of them.
    specs: Vec<String>,
    /// The size of every item where the catalog knows one: the list goes by it before the name.
    sizes: Vec<Option<i64>>,
    /// The names of the categories as the game writes them.
    names: Names,
    /// The reading of those names from the game files, while it runs.
    names_job: Option<std::sync::mpsc::Receiver<Names>>,
    pending: Option<Pending>,
    /// Where the shown values come from: `archive`, `scmdb` or empty.
    source: String,
    /// The reading ended without values.
    failed: bool,
    stock: Vec<Value>,
    stock_version: u64,
    stock_seen: Option<SystemTime>,
    notes: Map,
    notes_seen: Option<SystemTime>,
    /// The slots of the ships (erkul), for "fits into your ship".
    ships: ErkulShips,
    ships_seen: Option<SystemTime>,
    looked_at: f64,
    prices: RawPrices,
    pub search: String,
    picks: Picks,
    sort: Sort,
    view: View,
    /// The chosen item.
    open: Option<usize>,
    detail: Option<Detail>,
    fill: Order,
    /// Per item whether the stock covers its first tier, until the stock changes.
    covers: HashMap<usize, bool>,
    covers_version: u64,
    /// The items whose recipe uses a material the search names, for the search text.
    by_material: Option<(String, Vec<MaterialHit>)>,
    /// An item another page asked to show when this page is entered.
    jump: Option<String>,
    /// The item the list scrolls to once.
    scroll_to: Option<usize>,
    /// The fetch of the raw material prices, while it runs.
    prices_job: Option<std::sync::mpsc::Receiver<bool>>,
    /// The keys of the blueprints that have a recipe here, for the blueprint list.
    craftable: Arc<HashSet<String>>,
    /// How far the content of the scrolling parts sticks out of them to the right (0 = nothing cut off).
    list_overflow: f32,
    /// The cheapest shop of the chosen item.
    shop: crate::crafting_shop::ShopLine,
    /// What a new reading of the values needs, when the first one failed.
    retry: Option<Retry>,
    /// When the reading failed (`ui.input(|i| i.time)`), for the count to the next try.
    failed_at: Option<f64>,
    /// The fetches of the slots from erkul, for a blueprint without data.
    sync: crate::slot_sync::SlotSync,
    /// What the ship of the filter lets through, and what it was worked out from: the ship, the number of
    /// blueprints in the catalog and [`CraftingState::ship_version`].
    ship_fit: (ShipFit, Option<(String, usize, u64)>),
    /// Counts up whenever the recipes, the hangar or the slots of the ships are read anew.
    ship_version: u64,
    /// A ship another page asked to filter by when this page is entered.
    jump_ship: Option<String>,
}

/// Where the values are read from, kept for another try.
struct Retry {
    folder: Option<std::path::PathBuf>,
    cache: String,
    supply: Arc<dyn values_store::Supply>,
}

fn sample_values() -> GameValues {
    use versekit_gamedata::values::{ItemType, ItemValues, MaterialKind, Modifier, RecipeTier, Slot, SlotOption, Stat};
    let mut values = GameValues::default();
    let make = |guid: &str, name: &str, maker: &str, kind: &str, size: i64| ItemValues {
        guid: guid.into(),
        class_name: name.replace(' ', "_"),
        name: Some(name.into()),
        manufacturer: Some(maker.into()),
        item_type: ItemType::ShipComponent,
        attach_type: Some(kind.into()),
        cg_item_type: Some(kind.into()),
        size: Some(size),
        stats: [(Stat::Health, 1000.0)].into_iter().collect(),
        ..ItemValues::default()
    };
    // The frame of each part gives its integrity: from × 0.9 at quality 0 to × 1.1 at 1000.
    let integrity = Modifier {
        property_key: "health_maxhealth".into(),
        property_name: Some("Integrity".into()),
        start_quality: 0.0,
        end_quality: 1000.0,
        at_start: 0.9,
        at_end: 1.1,
        ..Modifier::default()
    };
    let slot = |name: &str, material: &str, amount: f64, modifiers: Vec<Modifier>| Slot {
        name: name.into(),
        options: vec![SlotOption {
            kind: MaterialKind::Resource,
            material: Some(material.into()),
            quantity: Some(amount),
            min_quality: Some(0),
            ..SlotOption::default()
        }],
        modifiers,
    };
    for (guid, name, maker, kind, tag, material) in [
        ("a", "Frostline", "Aegis Dynamics", "Cooler", "BP_CRAFT_COOL_AEGS_S02_Frostline", "Iron"),
        ("b", "Arc Shield", "Basilisk", "Shield", "BP_CRAFT_SHLD_BASL_S02_Arc", "Agricium"),
        ("c", "Grand Core", "Greycat Industrial", "PowerPlant", "BP_CRAFT_POWR_GRIN_S01_Core", "Taranite"),
    ] {
        values.items.insert(guid.into(), make(guid, name, maker, kind, 2));
        values.recipes.push(Recipe {
            guid: tag.to_lowercase(),
            tag: tag.into(),
            product: Some(guid.into()),
            kind: kind.to_lowercase(),
            tiers: vec![RecipeTier {
                craft_time_seconds: Some(900),
                slots: vec![
                    slot("Frame", material, 0.5, vec![integrity.clone()]),
                    slot("Core", "Gold", 0.2, Vec::new()),
                ],
            }],
            ..Recipe::default()
        });
    }
    values
}

impl Default for CraftingState {
    /// The sample: three made-up items, nothing read or written.
    fn default() -> Self {
        let mut state = CraftingState::empty(None);
        state.take_values(Arc::new(sample_values()), "archive");
        state
    }
}

impl CraftingState {
    fn empty(paths: Option<Paths>) -> CraftingState {
        CraftingState {
            paths,
            values: None,
            book: CraftBook::default(),
            data: StockData::default(),
            places: Vec::new(),
            specs: Vec::new(),
            sizes: Vec::new(),
            names: Names::default(),
            names_job: None,
            pending: None,
            source: String::new(),
            failed: false,
            stock: Vec::new(),
            stock_version: 0,
            stock_seen: None,
            notes: hangar::empty(),
            notes_seen: None,
            ships: ErkulShips::default(),
            ships_seen: None,
            looked_at: f64::NEG_INFINITY,
            prices: RawPrices::default(),
            search: String::new(),
            picks: Picks::default(),
            sort: Sort::default(),
            view: View::default(),
            open: None,
            detail: None,
            fill: Order::Best,
            covers: HashMap::new(),
            covers_version: 0,
            by_material: None,
            jump: None,
            scroll_to: None,
            prices_job: None,
            craftable: Arc::default(),
            list_overflow: 0.0,
            shop: crate::crafting_shop::ShopLine::new(None),
            retry: None,
            failed_at: None,
            sync: crate::slot_sync::global().clone(),
            ship_fit: (ShipFit::Off, None),
            ship_version: 0,
            jump_ship: None,
        }
    }

    /// The keys of the blueprints that have a recipe here.
    pub fn craftable(&self) -> Arc<HashSet<String>> {
        self.craftable.clone()
    }

    /// With own data: the values read last, the stock, the notes and the
    /// prices of the data folder; the new values are read in a thread.
    pub fn with_files(paths: &Paths, supply: Arc<dyn values_store::Supply>) -> CraftingState {
        let mut state = CraftingState::empty(Some(Paths::new(paths.host.clone())));
        state.fill = if paths.setting(SETTING_FILL).as_deref() == Some("worst") { Order::Worst } else { Order::Best };
        state.view = View::from_word(paths.setting(SETTING_VIEW).as_deref());
        state.prices = RawPrices::load(paths);
        state.shop = crate::crafting_shop::ShopLine::new(Some(paths));
        let folder = paths.game_folder().map(std::path::PathBuf::from);
        let cache = paths.app_file(values_store::CACHE_FILE);
        let reads = supply.reads_archive();
        let start = values_store::start(folder.as_deref(), &cache, supply.clone());
        if let Some(values) = start.values {
            state.take_values(values, &start.source);
        }
        if reads {
            state.pending = start.rebuild.and_then(values_store::Rebuild::spawn);
            state.fetch_prices(paths);
            if let Some(folder) = &folder {
                state.read_names(folder.to_string_lossy().into_owned());
            }
            state.retry = Some(Retry { folder, cache, supply });
        }
        state.reload_stock();
        state.reload_notes();
        state.reload_ships();
        state
    }

    /// Reads the names of the categories from the `global.ini` of the game
    /// in a thread of its own (the files are large).
    fn read_names(&mut self, folder: String) {
        let (sender, receiver) = std::sync::mpsc::channel();
        let started = std::thread::Builder::new().name("crafting-names".into()).spawn(move || {
            let de = versekit_core::controls::ini_texts(&folder, language::Lang::De);
            let en = versekit_core::controls::ini_texts(&folder, language::Lang::En);
            let _ = sender.send(Names::from_ini(&de, &en));
        });
        if started.is_ok() {
            self.names_job = Some(receiver);
        }
    }

    /// The game values with what is made of them, for the pages that calculate
    /// with the recipes too; `None` while there are none.
    pub fn parts(&self) -> Option<(&GameValues, &CraftBook, &StockData)> {
        self.values.as_deref().map(|values| (values, &self.book, &self.data))
    }

    /// Why there are no game values: read right now, reading failed, or there are none.
    pub fn values_wait(&self) -> Option<ValuesWait> {
        if self.values.is_some() {
            None
        } else if self.pending.is_some() {
            Some(ValuesWait::Reading)
        } else if self.failed {
            Some(ValuesWait::Failed)
        } else {
            Some(ValuesWait::None)
        }
    }

    /// The settings this page keeps in the settings file.
    pub fn setting_values(&self) -> Vec<(&'static str, Value)> {
        if self.paths.is_none() {
            return Vec::new();
        }
        vec![
            (SETTING_FILL, Value::str(if self.fill == Order::Worst { "worst" } else { "best" })),
            (SETTING_VIEW, Value::str(self.view.word())),
        ]
    }

    /// Something narrows the list: search or a filter.
    pub fn is_narrowed(&self) -> bool {
        !self.search.trim().is_empty() || self.picks != Picks::default()
    }

    /// Clears search, filters and the chosen item.
    pub fn reset(&mut self) {
        self.clear_filters();
        self.open = None;
        self.detail = None;
    }

    /// Clears search and filters; the chosen item stays.
    fn clear_filters(&mut self) {
        self.search.clear();
        self.picks = Picks::default();
    }

    /// Chooses an item of the list: its recipe and values show.
    fn choose(&mut self, index: usize) {
        if self.open != Some(index) {
            self.open = Some(index);
            self.detail = None;
        }
    }

    /// Chooses the first item of the book, as a click on it in the list does. Whether there is one.
    pub fn choose_first(&mut self) -> bool {
        if self.book.items.is_empty() {
            return false;
        }
        self.choose(0);
        true
    }

    /// The page is entered again: it starts without filters and choice, but
    /// only when something was set.
    pub fn enter(&mut self) {
        // A ship to filter by needs no recipes yet: the filter stands until they are there.
        if let Some(ship) = self.jump_ship.take() {
            self.reset();
            self.picks.ship = ship;
            return;
        }
        // While the recipes are still being read the jump waits for them.
        if self.jump.is_some() && !self.book.items.is_empty() {
            let name = self.jump.take().unwrap_or_default();
            self.reset();
            self.search = name.clone();
            let wanted = name_key(&name);
            self.open = self
                .book
                .items
                .iter()
                .position(|item| item.name == name)
                .or_else(|| self.book.items.iter().position(|item| name_key(&item.basis) == wanted));
            self.scroll_to = self.open;
        } else if self.jump.is_none() && (self.is_narrowed() || self.open.is_some()) {
            self.reset();
        }
    }

    /// A jump from another page: the name stands in the search and the item
    /// is chosen once the page is entered.
    pub fn jump_to(&mut self, name: &str) {
        self.jump = Some(name.to_string());
    }

    /// A jump from another page: the list shows the items that fit this hangar ship once the page is entered.
    pub fn jump_to_ship(&mut self, name: &str) {
        self.jump = None;
        self.jump_ship = Some(name.to_string());
    }

    /// The names of the ships in the hangar, each once, in the order of the hangar.
    fn ship_names(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for entry in hangar::ships(&self.notes) {
            let name = hangar::field_text(entry, "name").trim();
            if !name.is_empty() && !names.iter().any(|n| n == name) {
                names.push(name.to_string());
            }
        }
        names
    }

    /// Works out what the ship of the filter lets through, when ship, catalog, recipes, hangar or slots changed
    /// since the last time. A ship that is no longer in the hangar takes the filter away.
    fn refresh_ship_fit(&mut self, catalog: &CatalogCache) {
        if self.picks.ship.is_empty() {
            self.ship_fit = (ShipFit::Off, None);
            return;
        }
        let key = (self.picks.ship.clone(), catalog.blueprints.len(), self.ship_version);
        if self.ship_fit.1.as_ref() == Some(&key) {
            return;
        }
        let ships = hangar::ships(&self.notes);
        let Some(entry) = ships.iter().find(|e| hangar::field_text(e, "name").trim() == self.picks.ship) else {
            self.picks.ship.clear();
            self.ship_fit = (ShipFit::Off, None);
            return;
        };
        let fit = match erkul_slots::SlotTable::of(&self.ships, erkul_slots::ship_id(entry)) {
            None => ShipFit::NoData,
            Some(table) => {
                // The blueprint of an item as "fits into your ship" finds it: by the comparable name.
                let by_key: HashMap<&str, &versekit_core::catalog_cache::Blueprint> =
                    catalog.blueprints.iter().map(|b| (b.key.as_str(), b)).collect();
                let parts = self
                    .book
                    .items
                    .iter()
                    .enumerate()
                    .filter_map(|(index, item)| {
                        let blueprint = by_key.get(name_key(&item.basis).as_str())?;
                        let slots = table.takes(&blueprint.kind, blueprint.size);
                        let maker = blueprint.maker.as_deref();
                        let allowed = erkul_slots::maker_allows(&blueprint.kind, maker, &blueprint.name, entry);
                        (slots > 0 && allowed).then_some((index, slots))
                    })
                    .collect();
                ShipFit::Parts(parts)
            }
        };
        self.ship_fit = (fit, Some(key));
    }

    fn take_values(&mut self, values: Arc<GameValues>, source: &str) {
        // The chosen item stays chosen, though the list may be sorted anew.
        let open_name = self.open.and_then(|i| self.book.items.get(i)).map(|item| item.name.clone());
        self.book = CraftBook::build(&values);
        self.places = self
            .book
            .items
            .iter()
            .map(|item| fabricator::place_or_misc(&item.catalog_kind, &item.kind, &item.subtype))
            .collect();
        (self.specs, self.sizes) =
            self.paths.as_ref().map_or_else(|| (Vec::new(), Vec::new()), |paths| spec_lines(&self.book, paths));
        self.craftable = Arc::new(self.book.items.iter().map(|item| name_key(&item.basis)).collect());
        let mut data = StockData::default();
        data.add_recipe_materials(craft_book::material_lines(&values));
        self.data = data;
        self.values = Some(values);
        self.source = source.to_string();
        self.covers.clear();
        self.by_material = None;
        self.open = open_name.and_then(|name| self.book.items.iter().position(|item| item.name == name));
        self.detail = None;
        self.ship_version += 1;
    }

    /// Fetches the raw material prices in a thread of their own when they
    /// are due (at most once a day, `SC_BP_NO_NET` switches it off).
    fn fetch_prices(&mut self, paths: &Paths) {
        if !versekit_core::stock_fetch::prices_due(paths, SystemTime::now()) {
            return;
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        let own = Paths::new(paths.host.clone());
        let lock = versekit_core::writeguard::state();
        let started = std::thread::Builder::new().name("raw-prices".into()).spawn(move || {
            versekit_core::writeguard::adopt(&lock);
            let agent = versekit_core::basetool::config::user_agent(env!("CARGO_PKG_VERSION"));
            let outcome = versekit_core::stock_fetch::update_prices(&own, &agent);
            let _ = sender.send(matches!(outcome, versekit_core::stock_fetch::Outcome::Fresh));
        });
        if started.is_ok() {
            self.prices_job = Some(receiver);
        }
    }

    /// What goes on in the background: takes in what the reading threads bring and
    /// looks whether the stock or the notes changed. The window calls it every frame,
    /// so the values are there for the pages that only borrow them (the farm list,
    /// the button of the blueprint list) without a visit to this page.
    pub fn tick(&mut self, ctx: &egui::Context) {
        self.poll();
        self.follow(ctx);
        if self.pending.is_some() || self.prices_job.is_some() || self.names_job.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }

    /// The game values, or, while there are none, what stands in their place on
    /// the page: that they are read, that reading failed with the count to the
    /// next try, or that there are none. `None` then.
    pub fn values_or_wait(&mut self, ui: &mut Ui, scheme: &Scheme) -> Option<Arc<GameValues>> {
        self.tick(ui.ctx());
        if let Some(values) = self.values.clone() {
            return Some(values);
        }
        if self.pending.is_some() {
            widgets::loading(ui, scheme, &t("s_p9_werte_lesen"));
        } else if self.failed {
            let now = ui.input(|i| i.time);
            let failed_at = *self.failed_at.get_or_insert(now);
            let left = widgets::retry_in(failed_at, now);
            if left <= 0.0 && self.retry.is_some() {
                self.try_again();
                ui.ctx().request_repaint();
            } else {
                let status = tf("s_mu_naechster", &[("seconds", Arg::from(left.ceil() as i64))]);
                widgets::load_failed(ui, scheme, &t("s_p9_werte_fehlen"), &status);
            }
        } else {
            widgets::empty_state(ui, scheme, "clock", &t("s_he_keine_daten"));
        }
        None
    }

    /// Reads the values again after a failure that left the page without any.
    fn try_again(&mut self) {
        let Some(retry) = &self.retry else { return };
        let start = values_store::start(retry.folder.as_deref(), &retry.cache, retry.supply.clone());
        if let Some(values) = start.values {
            self.take_values(values, &start.source);
            self.failed = false;
        }
        self.pending = start.rebuild.and_then(values_store::Rebuild::spawn);
        self.failed = self.values.is_none() && self.pending.is_none();
        self.failed_at = None;
    }

    /// Takes over what the reading threads bring.
    fn poll(&mut self) {
        if let Some(job) = &self.prices_job
            && let Ok(fresh) = job.try_recv()
        {
            self.prices_job = None;
            if fresh && let Some(paths) = &self.paths {
                self.prices = RawPrices::load(paths);
            }
        }
        if let Some(job) = &self.names_job {
            match job.try_recv() {
                Ok(names) => {
                    self.names = names;
                    self.names_job = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.names_job = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        let Some(pending) = &self.pending else { return };
        if let Progress::Done(built) = pending.poll() {
            self.pending = None;
            match built {
                Some(built) => {
                    let source = if built.source == versekit_gamedata::value_source::Source::Archive {
                        "archive"
                    } else {
                        "scmdb"
                    };
                    self.take_values(built.values, source);
                    self.failed = false;
                }
                None => self.failed = self.values.is_none(),
            }
        }
    }

    fn file_time(file: &str) -> Option<SystemTime> {
        std::fs::metadata(file).and_then(|m| m.modified()).ok()
    }

    fn reload_stock(&mut self) {
        let Some(paths) = &self.paths else { return };
        let file = paths.app_file(stock::RAW_FILE);
        self.stock_seen = Self::file_time(&file);
        self.stock = stock::load(&file, "materials.load");
        self.stock_version += 1;
    }

    fn reload_notes(&mut self) {
        let Some(paths) = &self.paths else { return };
        let file = paths.app_file(NOTES_FILE);
        self.notes_seen = Self::file_time(&file);
        self.notes = hangar::read(&file).unwrap_or_else(hangar::empty);
        self.ship_version += 1;
    }

    fn reload_ships(&mut self) {
        let Some(paths) = &self.paths else { return };
        let file = paths.app_file(ERKUL_FILE);
        self.ships_seen = Self::file_time(&file);
        self.ships = ErkulShips::load(&file);
        self.ship_version += 1;
    }

    /// Reads the stock, the notes and the slots of the ships again when they changed on disk.
    fn follow(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if now - self.looked_at < LOOK_SECONDS {
            return;
        }
        self.looked_at = now;
        let Some(paths) = &self.paths else { return };
        let (stock_changed, notes_changed, ships_changed) = (
            Self::file_time(&paths.app_file(stock::RAW_FILE)) != self.stock_seen,
            Self::file_time(&paths.app_file(NOTES_FILE)) != self.notes_seen,
            Self::file_time(&paths.app_file(ERKUL_FILE)) != self.ships_seen,
        );
        if stock_changed {
            self.reload_stock();
        }
        if notes_changed {
            self.reload_notes();
        }
        if ships_changed {
            self.reload_ships();
        }
    }

    fn stock_file(&self) -> Option<String> {
        self.paths.as_ref().map(|p| p.app_file(stock::RAW_FILE))
    }

    /// Does the stock cover the first tier of the item, once?
    fn covers(&mut self, item: usize) -> bool {
        if self.covers_version != self.stock_version {
            self.covers.clear();
            self.covers_version = self.stock_version;
        }
        if let Some(known) = self.covers.get(&item) {
            return *known;
        }
        let covered = match (&self.values, self.book.items.get(item)) {
            (Some(values), Some(entry)) => values.recipes.get(entry.recipe).is_some_and(|recipe| {
                craft_book::ingredients(recipe).first().is_some_and(|tier| covered(&self.stock, &recipe_parts(tier), 1))
            }),
            _ => false,
        };
        self.covers.insert(item, covered);
        covered
    }

    /// The names of the items that need a material the search text names.
    fn by_material(&mut self, text: &str) -> &[MaterialHit] {
        if self.by_material.as_ref().is_none_or(|(known, _)| known != text) {
            let mut found = Vec::new();
            if let Some(values) = &self.values {
                for material in self.data.storable() {
                    if versekit_core::stock_data::lower(material).contains(text) {
                        found.push((material.clone(), craft_book::blueprints_with(values, material)));
                    }
                }
            }
            self.by_material = Some((text.to_string(), found));
        }
        self.by_material.as_ref().map_or(&[], |(_, found)| found.as_slice())
    }
}

// ------------------------------------------------------------ small helpers

fn tn(key: &str, args: &[Arg]) -> String {
    let named: Vec<(String, Arg)> = args.iter().enumerate().map(|(i, a)| (i.to_string(), a.clone())).collect();
    let refs: Vec<(&str, Arg)> = named.iter().map(|(n, a)| (n.as_str(), a.clone())).collect();
    tf(key, &refs)
}

/// A whole amount with the thousands mark of the language: 22700 as "22.700" or "22,700".
pub(crate) fn money(amount: f64) -> String {
    formats::count(language::current(), amount.round() as i64)
}

/// A number as Python's `%g` writes it, with the decimal mark of the language.
pub(crate) fn g(x: f64) -> String {
    let text = language::py_g(x, false);
    match language::current() {
        language::Lang::De => text.replace('.', ","),
        language::Lang::En => text,
    }
}

/// A number with `places` decimals and the decimal mark of the language.
pub(crate) fn fixed(x: f64, places: usize) -> String {
    formats::fixed(language::current(), x, places)
}

/// Like [`fixed`], with a plus sign where the number is not negative.
fn signed(x: f64, places: usize) -> String {
    let text = fixed(x, places);
    if text.starts_with('-') { text } else { format!("+{text}") }
}

/// How long crafting takes: "45 s", "16 min", "2 h 30 min".
pub(crate) fn duration(seconds: i64) -> String {
    if seconds < 60 {
        tn("s_he_sekunden", &[Arg::Int(seconds)])
    } else if seconds < 3600 {
        tn("s_he_minuten", &[Arg::Int(((seconds as f64) / 60.0).round_ties_even() as i64)])
    } else {
        tn("s_he_std_min", &[Arg::Int(seconds / 3600), Arg::Int((seconds % 3600) / 60)])
    }
}

fn rounded3(x: f64) -> f64 {
    py_round(x, 3)
}

fn sub(scheme: &Scheme, text: impl Into<String>) -> RichText {
    RichText::new(text.into()).small().color(color(scheme.sub))
}

fn rating_color(scheme: &Scheme, rating: Rating) -> Color {
    match rating {
        Rating::Good => widgets::change_color(scheme, true),
        Rating::Bad => widgets::change_color(scheme, false),
        Rating::Neutral => scheme.sub,
    }
}

/// The most pieces at once: more only come from a slip of the finger and would
/// make every frame count for a long time.
const COUNT_MAX: i64 = 9999;

/// The count typed when it is a number from one to [`COUNT_MAX`].
fn count_checked(text: &str) -> Option<i64> {
    let x = stock::parse_number(text)?;
    (x.is_finite() && (1.0..=COUNT_MAX as f64).contains(&x)).then(|| x.trunc() as i64)
}

/// Whether the field may stand as it is: empty counts as one.
fn count_valid(text: &str) -> bool {
    text.trim().is_empty() || count_checked(text).is_some()
}

/// The count typed: anything else than a number from one to [`COUNT_MAX`]
/// counts as one.
fn count_of(text: &str) -> i64 {
    count_checked(text).unwrap_or(1)
}

/// The least height of the list: six rows.
fn list_min(ui: &Ui) -> f32 {
    6.0 * list::row_height(ui)
}

// ------------------------------------------------------------ the page

/// What the page needs from the window besides its own state.
pub struct Shared<'a> {
    /// The comparable names of the blueprints one has.
    pub have: &'a HashSet<String>,
    /// The catalog, for class, size and grade and for whether a source is known.
    pub catalog: &'a CatalogCache,
}

/// How many sides stand next to each other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Columns {
    /// List, recipe and item side by side.
    Three,
    /// The list and beside it recipe and item, one under the other.
    Two,
    /// The list, or the chosen item with the way back.
    One,
}

impl Columns {
    fn for_width(width: f32, em: f32) -> Columns {
        let gap = layout::GAP;
        if width >= (LIST_EM + 2.0 * SIDE_EM) * em + 2.0 * gap {
            Columns::Three
        } else if width >= (LIST_EM + SIDE_EM) * em + gap {
            Columns::Two
        } else {
            Columns::One
        }
    }
}

/// Draws the page; what a click asks of the window comes back.
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut CraftingState, shared: &Shared<'_>) -> Option<Action> {
    let values = state.values_or_wait(ui, scheme)?;
    // A jump that came while the recipes were still read is kept until they are there.
    if state.jump.is_some() && !state.book.items.is_empty() {
        state.enter();
    }
    if state.book.items.is_empty() {
        widgets::empty_state(ui, scheme, "scan-search", &t("s_p9_keine_rezepte"));
        return None;
    }
    let marks = state.book.with_collection(shared.have);
    state.refresh_ship_fit(shared.catalog);
    if state.ship_fit.0 == ShipFit::NoData
        && let Some(paths) = &state.paths
    {
        // As for "fits into your ship": the slots are fetched, whichever page asks first.
        state.sync.wake(ui.ctx());
        state.sync.request(paths, crate::slot_sync::Trigger::BlueprintNoData);
    }
    let (sure, all, unclear) = state.book.counts(shared.have);
    head(ui, scheme, sure, all, unclear);
    if let Some(line) = values.crafter_limits.map(crafter_line) {
        ui.label(sub(scheme, line));
        ui.add_space(6.0);
    }
    if state.pending.is_some() {
        widgets::loading(ui, scheme, &t("s_p9_werte_neu"));
    }
    state.list_overflow = 0.0;
    let em = ui.text_style_height(&egui::TextStyle::Body);
    let width = ui.available_width();
    // The page itself does not scroll: every side scrolls in the room down to the bottom of the window.
    let height = (ui.clip_rect().bottom() - ui.cursor().min.y - 8.0).max(list_min(ui) * 2.0);
    let mut action = None;
    match Columns::for_width(width, em) {
        Columns::Three => {
            let free = width - 2.0 * layout::GAP;
            let list_width = (free * 0.34).floor();
            let side = ((free - list_width) / 2.0).floor();
            ui.horizontal_top(|row| {
                row.spacing_mut().item_spacing.x = layout::GAP;
                column(row, list_width, |ui| list::show(ui, scheme, state, &marks, height));
                column(row, side, |ui| {
                    side_scroll(ui, scheme, state, "herstellung-rezept", height, |ui, state| {
                        recipe_side(ui, scheme, state, &values)
                    });
                });
                column(row, side, |ui| {
                    let asked = side_scroll(ui, scheme, state, "herstellung-teil", height, |ui, state| {
                        item_side(ui, scheme, state, &values, shared, &marks)
                    });
                    action = action.take().or(asked);
                });
            });
        }
        Columns::Two => {
            let free = width - layout::GAP;
            let list_width = (free * 0.45).floor();
            let side = (free - list_width).floor();
            ui.horizontal_top(|row| {
                row.spacing_mut().item_spacing.x = layout::GAP;
                column(row, list_width, |ui| list::show(ui, scheme, state, &marks, height));
                column(row, side, |ui| {
                    action = side_scroll(ui, scheme, state, "herstellung-auswahl", height, |ui, state| {
                        stacked(ui, scheme, state, &values, shared, &marks)
                    });
                });
            });
        }
        Columns::One => {
            if state.open.is_some() {
                let top = ui.cursor().min.y;
                if ui
                    .add(widgets::Button::new(&t("s_ld_zurueck_liste"), ButtonKind::Normal, scheme).icon("arrow-left"))
                    .clicked()
                {
                    state.open = None;
                    state.detail = None;
                    return None;
                }
                ui.add_space(4.0);
                let rest = (height - (ui.cursor().min.y - top)).max(list_min(ui));
                action = side_scroll(ui, scheme, state, "herstellung-auswahl", rest, |ui, state| {
                    stacked(ui, scheme, state, &values, shared, &marks)
                });
            } else {
                list::show(ui, scheme, state, &marks, height);
            }
        }
    }
    action
}

/// A side of the page, `width` wide, its content from the top.
fn column(ui: &mut Ui, width: f32, add: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(egui::vec2(width, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.set_width(width);
        add(ui);
    });
}

/// A side that scrolls on its own in `height` points; what sticks out of it to
/// the right is measured.
fn side_scroll<R>(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &mut CraftingState,
    id: &str,
    height: f32,
    add: impl FnOnce(&mut Ui, &mut CraftingState) -> R,
) -> R {
    let area = egui::ScrollArea::vertical()
        .id_salt(id)
        .max_height(height)
        .min_scrolled_height(height)
        .auto_shrink([false, false]);
    let shown = widgets::scroll_area(ui, scheme, area, |ui| {
        ui.set_max_width(ui.available_width() - layout::SAFETY);
        add(ui, state)
    });
    state.list_overflow = state.list_overflow.max((shown.content_size.x - shown.inner_rect.width()).max(0.0));
    shown.inner
}

/// Recipe and item one under the other (the narrower windows).
fn stacked(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &mut CraftingState,
    values: &Arc<GameValues>,
    shared: &Shared<'_>,
    marks: &[Have],
) -> Option<Action> {
    if state.open.is_none() {
        choose_hint(ui, scheme);
        return None;
    }
    let first = item_head(ui, scheme, state, values, shared, marks);
    ui.add_space(layout::GAP);
    let second = recipe_side(ui, scheme, state, values);
    ui.add_space(layout::GAP);
    values_card(ui, scheme, state, values);
    first.or(second)
}

/// What stands where no item is chosen yet.
fn choose_hint(ui: &mut Ui, scheme: &Scheme) {
    ui.add_space(24.0);
    widgets::empty_state(ui, scheme, "hammer", &t("s_fb_waehlen"));
}

fn head(ui: &mut Ui, scheme: &Scheme, sure: usize, all: usize, unclear: usize) {
    ui.horizontal_wrapped(|row| {
        row.label(RichText::new(sure.to_string()).heading().color(color(scheme.accent)));
        row.label(sub(scheme, tn("s_he_von", &[Arg::from(all)]).trim_start().to_string()));
        if unclear > 0 {
            row.label(sub(scheme, tn("s_he_dazu_unklar", &[Arg::from(unclear)]).trim_start().to_string()));
        }
    });
    let fraction = sure as f32 / all.max(1) as f32;
    let bar_width = ui.available_width();
    widgets::progress(ui, scheme, fraction, bar_width);
    ui.add_space(6.0);
}

/// The limits of the fabricator in one line: "2 in progress, 4 waiting, 10 in the history · Dismantling 4/4".
fn crafter_line(limits: versekit_gamedata::values::CrafterLimits) -> String {
    let mut line =
        tn("s_p9_werkbank", &[Arg::Int(limits.in_progress), Arg::Int(limits.waiting), Arg::Int(limits.history)]);
    if let Some((progress, waiting)) = limits.dismantle {
        line = format!("{line}  ·  {}", tn("s_p9_werkbank_zerlegen", &[Arg::Int(progress), Arg::Int(waiting)]));
    }
    line
}

/// Class, size and grade written out ("Militär · Größe 4 · Güte A"), empty
/// for armour and hand weapons, whose size and grade mean nothing.
fn specs(catalog: &CatalogCache, item: &CraftItem) -> String {
    let Some(entry) = catalog.blueprints.iter().find(|b| b.key == name_key(&item.basis)) else { return String::new() };
    if entry.kind.starts_with("Char_") || matches!(entry.kind.as_str(), "WeaponPersonal" | "WeaponAttachment") {
        return String::new();
    }
    let mut parts = Vec::new();
    if let Some(class) = entry.class.as_deref() {
        let key = format!("s_ld_kl_{}", class.to_lowercase());
        if language::text_in(language::current(), &key).is_some() {
            parts.push(t(&key));
        }
    }
    if let Some(size) = entry.size.filter(|s| *s != 0) {
        parts.push(tn("s_ld_groesse", &[Arg::Int(size)]));
    }
    if let Some(grade) = entry
        .grade
        .and_then(|g| "abcd".chars().nth((g - 1).max(0) as usize))
        .filter(|_| entry.grade.is_some_and(|g| (1..=4).contains(&g)))
    {
        parts.push(tn("s_ld_guete", &[Arg::from(grade.to_ascii_uppercase().to_string())]));
    }
    parts.join("  ·  ")
}

/// Whether the catalog knows the blueprint and a way to get it.
fn has_source(catalog: &CatalogCache, item: &CraftItem) -> bool {
    catalog.blueprints.iter().any(|b| b.key == name_key(&item.basis) && !b.ways.is_empty())
}

// ------------------------------------------------------------ the chosen item

fn unique_materials(ingredients: &[Ingredient]) -> Vec<String> {
    let mut seen = Vec::new();
    for i in ingredients {
        if !i.material.is_empty() && !seen.contains(&i.material) {
            seen.push(i.material.clone());
        }
    }
    seen
}

fn ensure_detail(state: &mut CraftingState, index: usize, ingredients: &[Ingredient]) {
    if state.detail.as_ref().is_some_and(|d| d.item == index) {
        return;
    }
    let mut detail = Detail {
        item: index,
        count: "1".into(),
        stand: HashMap::new(),
        mix: HashMap::new(),
        auto: HashSet::new(),
        from_stock: HashMap::new(),
        from_stock_lots: HashMap::new(),
        rest: HashMap::new(),
        in_stock: HashMap::new(),
        feedback: None,
        typed: HashMap::new(),
        stock_version: state.stock_version,
        counted: 1,
        bad: HashSet::new(),
    };
    take_stock_qualities(state, &mut detail, ingredients);
    for (material, portion) in portions(ingredients) {
        detail.auto.extend(detail.from_stock.contains_key(&material).then(|| material.clone()));
        detail.in_stock.insert(material.clone(), detail.from_stock.contains_key(&material));
        detail.rest.insert(material.clone(), START_QUALITY);
        set_from_stock(&mut detail, &material, portion);
    }
    state.detail = Some(detail);
}

/// The amount per piece of every material, over all its slots, in the order of the recipe.
fn portions(ingredients: &[Ingredient]) -> Vec<(String, f64)> {
    let mut found: Vec<(String, f64)> = Vec::new();
    for i in ingredients.iter().filter(|i| !i.material.is_empty()) {
        match found.iter_mut().find(|(material, _)| *material == i.material) {
            Some((_, amount)) => *amount += i.amount,
            None => found.push((i.material.clone(), i.amount)),
        }
    }
    found
}

/// The quality of `material` from the lots the stock fills it with: one
/// quality alone, several as a mix. What the stock lacks of `portion` is a
/// lot at the quality for the rest.
fn set_from_stock(detail: &mut Detail, material: &str, portion: f64) {
    let rest = detail.rest.get(material).copied().unwrap_or(START_QUALITY);
    let lots = detail.from_stock_lots.get(material).cloned().unwrap_or_default();
    let mut rows: Vec<Part> = lots.iter().map(|lot| Part::new(lot.amount, lot.quality, true)).collect();
    let lacking = portion - quality_mix::total(&lots);
    if lacking > 1e-9 {
        rows.push(Part::new(lacking, rest, false));
    }
    let all: Vec<Lot> = rows.iter().map(Part::lot).collect();
    detail.stand.insert(material.to_string(), quality_mix::mixed_quality(&all).unwrap_or(rest));
    if rows.len() > 1 {
        detail.mix.insert(material.to_string(), rows);
    } else {
        detail.mix.remove(material);
    }
}

/// The lowest quality the recipe takes per material.
fn lowest_qualities(ingredients: &[Ingredient]) -> HashMap<&str, f64> {
    let mut lowest: HashMap<&str, f64> = HashMap::new();
    for i in ingredients.iter().filter(|i| !i.material.is_empty()) {
        let slot = lowest.entry(&i.material).or_insert(0.0);
        *slot = slot.max(i.quality);
    }
    lowest
}

/// The lots the stock fills each material with for the count typed, the best
/// or the worst first, from as many entries as it takes.
fn take_stock_qualities(state: &CraftingState, detail: &mut Detail, ingredients: &[Ingredient]) {
    let pieces = count_of(&detail.count);
    let lowest = lowest_qualities(ingredients);
    detail.from_stock.clear();
    detail.from_stock_lots.clear();
    for (material, portion) in portions(ingredients) {
        let there = quality_mix::in_stock(&state.stock, &material, lowest[material.as_str()]);
        let lots = quality_mix::fill(&there, portion * pieces as f64, state.fill);
        if let Some(q) = quality_mix::mixed_quality(&lots) {
            let each = lots.iter().map(|lot| Lot { amount: lot.amount / pieces as f64, quality: lot.quality });
            detail.from_stock.insert(material.clone(), q);
            detail.from_stock_lots.insert(material, each.collect());
        }
    }
    detail.counted = pieces;
    detail.stock_version = state.stock_version;
}

/// The lots per piece each material is taken from the stock as: its mix,
/// else the whole portion at its quality. No lot counts below the lowest
/// quality of the recipe, and what the mix leaves of the portion counts at it.
fn parts_now(ingredients: &[Ingredient], detail: &Detail) -> Vec<(String, Vec<Lot>)> {
    let lowest = lowest_qualities(ingredients);
    portions(ingredients)
        .into_iter()
        .map(|(material, portion)| {
            let floor = lowest.get(material.as_str()).copied().unwrap_or(0.0);
            let mut parts: Vec<Lot> = match detail.mix.get(&material) {
                Some(rows) => rows.iter().map(|row| Lot { quality: row.quality.max(floor), ..row.lot() }).collect(),
                None => {
                    let quality = detail.stand.get(&material).copied().unwrap_or(START_QUALITY).max(floor);
                    vec![Lot { amount: portion, quality }]
                }
            };
            let rest = portion - quality_mix::total(&parts);
            if rest > 1e-9 {
                parts.push(Lot { amount: rest, quality: floor });
            }
            (material, parts)
        })
        .collect()
}

/// Each ingredient of a recipe as a lot at its lowest quality, per material.
fn recipe_parts(ingredients: &[Ingredient]) -> Vec<(String, Vec<Lot>)> {
    let mut found: Vec<(String, Vec<Lot>)> = Vec::new();
    for i in ingredients.iter().filter(|i| !i.material.is_empty()) {
        let lot = Lot { amount: i.amount, quality: i.quality };
        match found.iter_mut().find(|(material, _)| *material == i.material) {
            Some((_, lots)) => lots.push(lot),
            None => found.push((i.material.clone(), vec![lot])),
        }
    }
    found
}

/// How every material stands against the stock for `count` pieces.
fn standings(stock: &[Value], parts: &[(String, Vec<Lot>)], count: i64) -> Vec<stock_use::Standing> {
    parts.iter().map(|(material, lots)| quality_mix::standing(stock, material, lots, count)).collect()
}

/// Whether the stock covers `count` pieces.
fn covered(stock: &[Value], parts: &[(String, Vec<Lot>)], count: i64) -> bool {
    standings(stock, parts, count).iter().all(|s| s.missing <= 0.0)
}

/// The most pieces the stock covers with the qualities set now; 0 for none.
fn max_count(state: &CraftingState, ingredients: &[Ingredient]) -> i64 {
    let Some(detail) = &state.detail else { return 0 };
    let now = parts_now(ingredients, detail);
    let covers = |n: i64| covered(&state.stock, &now, n);
    if !covers(1) {
        return 0;
    }
    if covers(COUNT_MAX) {
        return COUNT_MAX;
    }
    let (mut low, mut high) = (1, COUNT_MAX);
    while high - low > 1 {
        let middle = (low + high) / 2;
        if covers(middle) {
            low = middle;
        } else {
            high = middle;
        }
    }
    low
}

fn on_notes(state: &CraftingState, item: &CraftItem) -> bool {
    notepad::contains(&state.notes, &item.name)
}

/// The chosen item with its recipe and ingredients, the detail made ready and
/// following the count and the stock; `None` without a choice.
fn chosen(state: &mut CraftingState, values: &GameValues) -> Option<(usize, CraftItem, Recipe, Vec<Ingredient>)> {
    let index = state.open?;
    let item = state.book.items.get(index)?.clone();
    let recipe = values.recipes.get(item.recipe)?.clone();
    let ingredients: Vec<Ingredient> = craft_book::ingredients(&recipe).first().cloned().unwrap_or_default();
    ensure_detail(state, index, &ingredients);
    // The count or the stock changed since the qualities were taken: the
    // materials that follow the stock follow it.
    if let Some(detail) = &state.detail {
        let count = count_of(&detail.count);
        if detail.counted != count || detail.stock_version != state.stock_version {
            refresh_qualities(state, &ingredients, Refill::Follow);
        }
    }
    Some((index, item, recipe, ingredients))
}

/// The side of the item: name, maker, category, tier, time, sources, then the
/// table of its values (in the widest window).
fn item_side(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &mut CraftingState,
    values: &Arc<GameValues>,
    shared: &Shared<'_>,
    marks: &[Have],
) -> Option<Action> {
    // Without a choice the side of the recipe says what to do; this one stays empty.
    state.open?;
    let action = item_head(ui, scheme, state, values, shared, marks);
    ui.add_space(layout::GAP);
    values_card(ui, scheme, state, values);
    action
}

/// What says which item it is and where to get it.
fn item_head(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &mut CraftingState,
    values: &Arc<GameValues>,
    shared: &Shared<'_>,
    marks: &[Have],
) -> Option<Action> {
    let (index, item, recipe, _) = chosen(state, values)?;
    let mark = marks.get(index).copied().unwrap_or(Have::No);
    let mut action = None;
    widgets::card(ui, scheme, |ui| {
        ui.add(
            egui::Label::new(widgets::bold(ui, &item.name, egui::TextStyle::Heading).color(color(scheme.accent)))
                .wrap(),
        );
        let mut line = Vec::new();
        if !item.manufacturer.is_empty() {
            line.push(item.manufacturer.clone());
        }
        let spec = specs(shared.catalog, &item);
        if !spec.is_empty() {
            line.push(spec);
        }
        if !line.is_empty() {
            ui.add(egui::Label::new(sub(scheme, line.join("  ·  "))).wrap());
        }
        if let Some(place) = state.places.get(index) {
            ui.add(egui::Label::new(RichText::new(state.names.path(*place)).color(color(scheme.fg))).wrap());
        }
        ui.horizontal_wrapped(|row| {
            let tier = if item.tiers > 1 {
                tf("s_fb_stufe_von", &[("n", Arg::Int(1)), ("von", Arg::from(item.tiers))])
            } else {
                tf("s_fb_stufe", &[("n", Arg::Int(1))])
            };
            row.label(sub(scheme, tier));
            if let Some(seconds) = recipe.tiers.first().and_then(|tier| tier.craft_time_seconds).filter(|s| *s != 0) {
                row.label(sub(scheme, format!("·  {}", t("s_he_zeit"))));
                row.label(RichText::new(duration(seconds)).small());
            }
        });
        if mark == Have::Unclear {
            ui.label(sub(scheme, t("s_he_unklar")));
        }
        if let Some(raw) = &item.raw_product {
            ui.label(sub(scheme, tn("s_cig_produkt", &[Arg::from(raw.as_str())])));
        }
        if mark != Have::Yes && has_source(shared.catalog, &item) {
            ui.add_space(4.0);
            let word = t("s_he_woher_bp");
            if ui.add(widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("info")).clicked() {
                action = Some(Action::ToBlueprint(item.basis.clone()));
            }
        }
        if state.shop.show(ui, scheme, &item.entity, &item.basis).is_some() {
            action = Some(Action::ToShops);
        }
        fits_block(ui, scheme, state, shared.catalog, &item);
    });
    action
}

/// "Fits into your ship": the answer nobody else can give, because erkul knows the slots of every ship but not
/// your hangar, and the watcher knows your hangar but not the slots. ⚠ Bold and in colour: grey under a long
/// block of recipe is overlooked, and both answers (fits, fits nowhere) deserve to be seen.
fn fits_block(ui: &mut Ui, scheme: &Scheme, state: &CraftingState, catalog: &CatalogCache, item: &CraftItem) {
    let Some(blueprint) = catalog.blueprints.iter().find(|b| b.key == name_key(&item.basis)) else { return };
    let ships = hangar::ships(&state.notes);
    let part = (blueprint.maker.as_deref(), blueprint.name.as_str());
    let (text, tint, muted) = match erkul_slots::fit_answer(&state.ships, &blueprint.kind, blueprint.size, part, &ships)
    {
        FitAnswer::Silent => return,
        FitAnswer::EmptyHangar => (t("s_hg_passt_leer"), scheme.sub, true),
        FitAnswer::NoData => {
            // The data is missing, so it is fetched, whichever page asks first; at most once per program run.
            if let Some(paths) = &state.paths {
                state.sync.wake(ui.ctx());
                state.sync.request(paths, crate::slot_sync::Trigger::BlueprintNoData);
            }
            (t("s_hg_passt_unbekannt"), scheme.gold, false)
        }
        FitAnswer::In(found) => {
            let names: Vec<String> = found
                .iter()
                .map(|(name, count)| {
                    if *count > 1 {
                        tf("s_hg_passt_mehrfach", &[("name", Arg::from(name.as_str())), ("n", Arg::Int(*count))])
                    } else {
                        name.clone()
                    }
                })
                .collect();
            (tf("s_hg_passt_in", &[("schiffe", Arg::from(names.join(", ")))]), scheme.accent, false)
        }
        FitAnswer::Nowhere => (t("s_hg_passt_nirgends"), scheme.gold, false),
    };
    ui.add_space(6.0);
    let label =
        if muted { sub(scheme, text) } else { widgets::bold(ui, &text, egui::TextStyle::Body).color(color(tint)) };
    ui.add(egui::Label::new(label).wrap());
}

/// The side of the recipe: count, crafting, autofill, then one block per slot.
fn recipe_side(ui: &mut Ui, scheme: &Scheme, state: &mut CraftingState, values: &Arc<GameValues>) -> Option<Action> {
    let Some((_, item, recipe, ingredients)) = chosen(state, values) else {
        choose_hint(ui, scheme);
        return None;
    };
    let mut action = None;
    widgets::card(ui, scheme, |ui| {
        widgets::card_title(ui, scheme, &t("s_fb_rezept"));
        count_row(ui, scheme, state, &ingredients);
        craft_buttons(ui, scheme, state, &item, &ingredients);
        fill_row(ui, scheme, state, &ingredients);
        dismantle_note(ui, scheme, values, &ingredients);
    });
    if !unique_materials(&ingredients).is_empty() {
        ui.add_space(layout::GAP);
        ui.label(widgets::bold(ui, t("s_he_regler_kopf"), egui::TextStyle::Body));
        ui.add(egui::Label::new(sub(scheme, tn("s_he_kauf_q", &[Arg::Int(BUY_QUALITY)]))).wrap());
        ui.add_space(4.0);
    }
    if let Some(asked) = slot_blocks(ui, scheme, state, &recipe, &item, &ingredients) {
        action = Some(asked);
    }
    action
}

/// The count with one less, one more and as many as the stock covers.
fn count_row(ui: &mut Ui, scheme: &Scheme, state: &mut CraftingState, ingredients: &[Ingredient]) {
    let valid = state.detail.as_ref().is_none_or(|d| count_valid(&d.count));
    let most = max_count(state, ingredients);
    let salt = state.open;
    ui.add_space(4.0);
    ui.horizontal_wrapped(|row| {
        row.label(sub(scheme, t("s_lg_anzahl")));
        let Some(detail) = state.detail.as_mut() else { return };
        let now = count_of(&detail.count);
        let less = widgets::icon_button(row, scheme, "minus", &t("s_fb_weniger"));
        let less_word = t("s_fb_weniger");
        less.widget_info(|| WidgetInfo::labeled(Role::Button, true, &less_word));
        if less.clicked() {
            detail.count = (now - 1).max(1).to_string();
        }
        red_if(row, scheme, !valid, |ui| {
            ui.add(
                widgets::text_field(&mut detail.count, &t("s_pl_anzahl"), scheme)
                    .id_salt(("herstellung-anzahl", salt))
                    .desired_width(60.0),
            )
        });
        let more = widgets::icon_button(row, scheme, "plus", &t("s_fb_mehr"));
        let more_word = t("s_fb_mehr");
        more.widget_info(|| WidgetInfo::labeled(Role::Button, true, &more_word));
        if more.clicked() {
            detail.count = (now + 1).min(COUNT_MAX).to_string();
        }
        let none = t("s_fb_max_keins");
        let word = t("s_fb_max");
        let max =
            row.add(widgets::Button::new(&word, ButtonKind::Normal, scheme).blocked((most == 0).then_some(&none)));
        if most > 0 {
            if max.clicked() {
                detail.count = most.to_string();
            }
            max.on_hover_text(t("s_fb_max_tip"));
        }
    });
    if !valid {
        ui.label(RichText::new(t("s_p9_anzahl_falsch")).small().color(color(scheme.red)));
    }
}

fn craft_buttons(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &mut CraftingState,
    item: &CraftItem,
    ingredients: &[Ingredient],
) {
    let mut crafted = false;
    let mut note = false;
    ui.add_space(4.0);
    let valid = state.detail.as_ref().is_none_or(|d| count_valid(&d.count));
    let quality_ok = state.detail.as_ref().is_none_or(|d| d.bad.is_empty());
    let noted = on_notes(state, item);
    ui.horizontal_wrapped(|row| {
        let word = t("s_lg_bauen");
        let reason = t("s_p9_anzahl_falsch");
        let reason_q = t("s_p9_qualitaet_falsch");
        let why_not = if !valid {
            Some(reason.as_str())
        } else if !quality_ok {
            Some(reason_q.as_str())
        } else {
            None
        };
        let made = row.add(widgets::Button::new(&word, ButtonKind::Strong, scheme).blocked(why_not));
        if made.clicked() {
            crafted = true;
        }
        if why_not.is_none() {
            made.on_hover_text(t("s_lg_bauen_hilfe"));
        }
        let word = t("s_mz_knopf");
        let marked = row.add(
            widgets::Button::new(&word, ButtonKind::Normal, scheme)
                .icon("diamond")
                .blocked((!valid).then_some(&reason)),
        );
        if marked.clicked() {
            note = true;
        }
        if valid {
            marked.on_hover_text(t("s_mz_hilfe"));
        }
        if noted {
            if let Some(image) = icons::image("check", scheme.accent, 14.0) {
                row.add(image);
            }
            row.label(RichText::new(t("s_p9_vorgemerkt")).small().color(color(scheme.accent)));
        }
    });
    if let Some(feedback) = state.detail.as_ref().and_then(|d| d.feedback.clone()) {
        let (text, tone) = match feedback {
            Feedback::Deducted(1) => (t("s_lg_abgezogen"), scheme.accent),
            Feedback::Deducted(n) => (tn("s_lg_abgezogen_n", &[Arg::Int(n)]), scheme.accent),
            Feedback::Missing(missing) => {
                let pairs: Vec<String> = missing
                    .iter()
                    .map(|(name, amount)| {
                        tn("s_p9_fehlt_paar", &[Arg::from(name.as_str()), Arg::from(g(rounded3(*amount)))])
                    })
                    .collect();
                (tn("s_lg_teilweise", &[Arg::from(pairs.join(", "))]), scheme.gold)
            }
            Feedback::NotSaved => (t("s_p9_lager_nicht_gespeichert"), scheme.red),
        };
        ui.add(egui::Label::new(RichText::new(text).small().color(color(tone))).wrap());
    }
    if crafted {
        craft(state, item, ingredients);
    }
    if note {
        add_note(state, item);
    }
}

/// "Fill with best / worst material" and "set from the stock".
fn fill_row(ui: &mut Ui, scheme: &Scheme, state: &mut CraftingState, ingredients: &[Ingredient]) {
    if unique_materials(ingredients).is_empty() {
        return;
    }
    let mut refill = None;
    ui.add_space(6.0);
    ui.horizontal_wrapped(|row| {
        row.label(sub(scheme, t("s_he_fuellen")));
        for (order, key) in [(Order::Best, "s_he_bestes"), (Order::Worst, "s_he_schlechtestes")] {
            let kind = if state.fill == order { ButtonKind::Strong } else { ButtonKind::Normal };
            if row.add(widgets::Button::new(&t(key), kind, scheme)).clicked() {
                refill = Some((Some(order), Refill::Fill));
            }
        }
    });
    let any_stock = state.detail.as_ref().is_some_and(|d| !d.from_stock.is_empty());
    if any_stock {
        let word = t("s_he_zurueck_lager");
        if ui.add(widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("rotate-cw")).clicked() {
            refill = Some((None, Refill::Stock));
        }
    }
    if let Some((order, how)) = refill {
        if let Some(order) = order {
            state.fill = order;
        }
        refresh_qualities(state, ingredients, how);
    }
}

/// "Crafted": the ingredients leave the stock, the farm notes go down.
fn craft(state: &mut CraftingState, item: &CraftItem, ingredients: &[Ingredient]) {
    let Some(detail) = state.detail.clone() else { return };
    let count = count_of(&detail.count);
    let now = parts_now(ingredients, &detail);
    // Reading, taking out and writing the stock is one step under the lock of the stock files: the sync must
    // not write them in between. `None` where nothing is missing but the file could not be written.
    let outcome: Result<(), Option<Vec<(String, f64)>>> = {
        let _lock = stock::lock();
        // The stock is read fresh, as 4.12 does: what another page wrote a moment
        // ago must not be lost by writing the copy of the memory back.
        let mut work = match state.stock_file() {
            Some(file) => stock::load(&file, "materials.load"),
            None => state.stock.clone(),
        };
        let mut missing: Vec<(String, f64)> = Vec::new();
        for (material, lots) in &now {
            let short = quality_mix::take(&mut work, material, lots, count);
            if short > 1e-9 {
                missing.push((material.clone(), py_round(short, 6)));
            }
        }
        work.retain(|entry| stock_use::amount_of(entry) > 1e-9);
        if !missing.is_empty() {
            Err(Some(missing))
        } else if state.stock_file().is_none_or(|file| stock::save(&file, &work, "materials.save")) {
            state.stock = work;
            state.stock_version += 1;
            Ok(())
        } else {
            Err(None)
        }
    };
    let feedback = match outcome {
        Err(Some(missing)) => Feedback::Missing(missing),
        Err(None) => Feedback::NotSaved,
        Ok(()) => {
            if let Some(paths) = &state.paths {
                let file = paths.app_file(NOTES_FILE);
                let _lock = hangar::lock();
                let mut held = hangar::load_held(&file);
                if notepad::crafted(&mut held.data, &item.name, count) {
                    hangar::save_held(&file, &mut held);
                }
                state.notes = held.data;
            }
            Feedback::Deducted(count)
        }
    };
    let deducted = matches!(feedback, Feedback::Deducted(_));
    if let Some(detail) = state.detail.as_mut() {
        detail.feedback = Some(feedback);
        if deducted {
            detail.count = "1".into();
        }
    }
    // The stock changed: the qualities follow it.
    refresh_qualities(state, ingredients, Refill::Follow);
}

fn add_note(state: &mut CraftingState, item: &CraftItem) {
    let Some(paths) = &state.paths else { return };
    let count = state.detail.as_ref().map_or(1, |d| count_of(&d.count));
    let file = paths.app_file(NOTES_FILE);
    let _lock = hangar::lock();
    let mut held = hangar::load_held(&file);
    notepad::add(&mut held.data, &item.name, "", count);
    hangar::save_held(&file, &mut held);
    state.notes = held.data;
}

/// Takes the qualities of the stock again after the count, the fill or the
/// stock changed (see [`Refill`]).
fn refresh_qualities(state: &mut CraftingState, ingredients: &[Ingredient], how: Refill) {
    let Some(mut detail) = state.detail.take() else { return };
    take_stock_qualities(state, &mut detail, ingredients);
    let lowest = lowest_qualities(ingredients);
    for (material, portion) in portions(ingredients) {
        let from_stock = detail.from_stock.get(&material).copied();
        let without_stock = match (how, state.fill) {
            (Refill::Fill, Order::Best) => BEST_QUALITY,
            (Refill::Fill, Order::Worst) => lowest.get(material.as_str()).copied().unwrap_or(0.0),
            _ => START_QUALITY,
        };
        if how == Refill::Follow {
            if detail.auto.contains(&material) && from_stock.is_some() {
                set_from_stock(&mut detail, &material, portion);
            }
            continue;
        }
        detail.rest.insert(material.clone(), without_stock);
        set_from_stock(&mut detail, &material, portion);
        if from_stock.is_some() {
            detail.auto.insert(material.clone());
        } else {
            detail.auto.remove(&material);
        }
        detail.typed.remove(&material);
        detail.in_stock.insert(material, from_stock.is_some());
    }
    state.detail = Some(detail);
}

fn dismantle_note(ui: &mut Ui, scheme: &Scheme, values: &GameValues, ingredients: &[Ingredient]) {
    let (lost, efficiency) = craft_book::dismantle_lost(values);
    let lost_keys: HashSet<String> = lost.iter().map(|n| versekit_core::stock_data::norm_material(n)).collect();
    let mut affected: Vec<&str> = Vec::new();
    for i in ingredients {
        if !i.material.is_empty()
            && lost_keys.contains(&versekit_core::stock_data::norm_material(&i.material))
            && !affected.contains(&i.material.as_str())
        {
            affected.push(&i.material);
        }
    }
    if !affected.is_empty() {
        ui.add_space(4.0);
        ui.add(
            egui::Label::new(sub(
                scheme,
                tn("s_p9_zerlegen", &[Arg::from(fixed(efficiency * 100.0, 0)), Arg::from(affected.join(", "))]),
            ))
            .wrap(),
        );
    }
}

// ------------------------------------------------------------ the slots

/// The ingredients per slot of the recipe, in the order of the recipe.
fn by_slot(ingredients: &[Ingredient]) -> Vec<(String, Vec<&Ingredient>)> {
    let mut slots: Vec<(String, Vec<&Ingredient>)> = Vec::new();
    for ingredient in ingredients {
        match slots.iter_mut().find(|(name, _)| *name == ingredient.slot) {
            Some((_, list)) => list.push(ingredient),
            None => slots.push((ingredient.slot.clone(), vec![ingredient])),
        }
    }
    slots
}

/// One block per slot: its material with amount and stock, the quality of the
/// material and what that quality does in this slot.
fn slot_blocks(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &mut CraftingState,
    recipe: &Recipe,
    item: &CraftItem,
    ingredients: &[Ingredient],
) -> Option<Action> {
    let detail = state.detail.clone()?;
    let count = count_of(&detail.count);
    let now = parts_now(ingredients, &detail);
    let by_material: HashMap<String, stock_use::Standing> =
        standings(&state.stock, &now, count).into_iter().map(|s| (s.material.clone(), s)).collect();
    let portion_of: HashMap<String, f64> = portions(ingredients).into_iter().collect();
    let laser = item.kind == "mininglaser";
    let lowest = lowest_qualities(ingredients);
    let slots_of_recipe: Vec<&Slot> = recipe.tiers.first().map(|tier| tier.slots.iter().collect()).unwrap_or_default();
    let mut action = None;
    // A material in several slots has one quality: its field stands in the first of them.
    let mut with_field: HashSet<String> = HashSet::new();
    for (n, (slot_name, members)) in by_slot(ingredients).into_iter().enumerate() {
        let slot = slots_of_recipe.iter().find(|s| s.name == slot_name).copied();
        let effective = slot.is_some_and(|s| !s.modifiers.is_empty());
        widgets::card(ui, scheme, |ui| {
            ui.label(widgets::bold(ui, slot_name.to_uppercase(), egui::TextStyle::Small).color(color(scheme.sub)));
            for ingredient in members {
                if let Some(asked) =
                    ingredient_line(ui, scheme, state, ingredient, count, by_material.get(&ingredient.material))
                {
                    action = Some(asked);
                }
                if ingredient.material.is_empty() {
                    continue;
                }
                let field = with_field.insert(ingredient.material.clone());
                let portion = portion_of.get(&ingredient.material).copied().unwrap_or(ingredient.amount);
                let floor = lowest.get(ingredient.material.as_str()).copied().unwrap_or(0.0);
                if field && floor > 1.0 {
                    ui.label(
                        RichText::new(tn("s_he_mindestens", &[Arg::from(py_g_text(floor))]))
                            .small()
                            .color(color(scheme.gold)),
                    );
                }
                let mixed = state.detail.as_ref().is_some_and(|d| d.mix.contains_key(&ingredient.material));
                if mixed && field {
                    mix_controls(ui, scheme, state, &ingredient.material, portion, n, floor);
                } else if mixed {
                    let q = state.detail.as_ref().and_then(|d| d.stand.get(&ingredient.material).copied());
                    let text = tn("s_he_mix_guete", &[Arg::from(py_g_text(q.unwrap_or(START_QUALITY)))]);
                    ui.label(RichText::new(text).small());
                } else if quality_controls(ui, scheme, state, &ingredient.material, effective, field, n, floor)
                    && let Some(detail) = state.detail.as_mut()
                {
                    let q = detail.stand.get(&ingredient.material).copied().unwrap_or(START_QUALITY);
                    let rows = vec![Part::new(portion, q, false), Part::new(0.0, q, false)];
                    detail.mix.insert(ingredient.material.clone(), rows);
                    detail.auto.remove(&ingredient.material);
                    detail.typed.remove(&ingredient.material);
                }
                let q = state
                    .detail
                    .as_ref()
                    .and_then(|d| d.stand.get(&ingredient.material).copied())
                    .unwrap_or(START_QUALITY);
                if let Some(slot) = slot {
                    effects(ui, scheme, slot, q, laser);
                }
            }
        });
        ui.add_space(4.0);
    }
    action
}

/// The material of a slot: link to where it is found, amount, what the stock has of it and what buying costs.
fn ingredient_line(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &CraftingState,
    ingredient: &Ingredient,
    count: i64,
    standing: Option<&stock_use::Standing>,
) -> Option<Action> {
    let piece = state.data.is_piece(&ingredient.material);
    let needed = ingredient.amount * count as f64;
    let mut action = None;
    ui.horizontal_wrapped(|row| {
        let link = row.add(egui::Link::new(RichText::new(&ingredient.material).color(color(scheme.accent))));
        if link.on_hover_text(t("s_fl_zum_bergbau")).clicked() {
            action = Some(Action::ToMining(ingredient.material.clone()));
        }
        let (whole, each) = (Arg::from(g(needed)), Arg::from(g(ingredient.amount)));
        let amount_text = match (piece, count) {
            (false, 1) => tn("s_p9_menge", &[whole]),
            (false, n) => tn("s_p9_menge_n", &[whole, each, Arg::Int(n)]),
            (true, 1) => tn("s_p9_menge_stk", &[whole]),
            (true, n) => tn("s_p9_menge_stk_n", &[whole, each, Arg::Int(n)]),
        };
        row.label(sub(scheme, amount_text));
        let (there, missing, too_low) = standing.map_or((0.0, 0.0, 0.0), |s| (s.there, s.missing, s.too_low));
        if missing > 0.0 {
            let text = if there > 0.0 {
                tn(
                    "s_p9_teil",
                    &[Arg::from(g(rounded3(there))), Arg::from(g(rounded3(needed))), Arg::from(g(rounded3(missing)))],
                )
            } else {
                tn("s_p9_fehlt", &[Arg::from(g(rounded3(missing)))])
            };
            row.label(RichText::new(text).small().color(color(scheme.gold)));
        } else if there > 0.0 {
            row.label(
                RichText::new(tn("s_p9_da", &[Arg::from(g(rounded3(there)))])).small().color(color(scheme.accent)),
            );
        }
        if too_low > 0.0 {
            let minimum = standing.map_or(0.0, |s| s.quality);
            let key = if piece { "s_p9_zu_schlecht_stk" } else { "s_p9_zu_schlecht" };
            row.label(sub(scheme, tn(key, &[Arg::from(g(rounded3(too_low))), Arg::from(g(minimum))])));
        }
        if missing > 0.0
            && let Some(price) = state.prices.price(&ingredient.material)
        {
            if price.buy > 0.0 && ingredient.quality > BUY_QUALITY as f64 {
                row.label(RichText::new(t("s_he_kauf_zu_schlecht")).small().color(color(scheme.gold)));
            } else if price.buy > 0.0 {
                row.label(sub(
                    scheme,
                    tn("s_he_kaufen", &[Arg::from(money(price.buy * missing)), Arg::Int(BUY_QUALITY)]),
                ));
            } else {
                row.label(RichText::new(t("s_he_nur_abbau")).small().color(color(scheme.gold)));
            }
        }
    });
    action
}

/// The quality of a material: slider and, in the first slot of the material,
/// the field to type it and the button to mix in another lot, with where the
/// value comes from. True when that button was clicked. `floor` is the
/// lowest quality the recipe takes; nothing below it can be set.
#[allow(clippy::too_many_arguments)]
fn quality_controls(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &mut CraftingState,
    material: &str,
    effective: bool,
    field: bool,
    slot: usize,
    floor: f64,
) -> bool {
    let item_salt = state.open;
    let Some(detail) = state.detail.as_mut() else { return false };
    let stood = detail.stand.get(material).copied().unwrap_or(START_QUALITY);
    let current = stood.max(floor);
    if stood < floor {
        detail.stand.insert(material.to_string(), floor);
        detail.typed.insert(material.to_string(), py_g_text(floor));
    }
    let mut set: Option<f64> = None;
    let mut invalid = false;
    let mut mix = false;
    ui.horizontal_wrapped(|row| {
        let mut value = current.round() as i64;
        let width = (row.available_width() - 90.0).clamp(80.0, 200.0);
        let slider = row.push_id(("herstellung-regler", slot, material), |row| {
            row.add(widgets::Slider::new(&mut value, floor as i64, 1000, scheme).width(width))
        });
        let slider = slider.inner;
        slider.widget_info(|| {
            let mut info = WidgetInfo::labeled(Role::Slider, true, format!("{material}, Q"));
            info.value = Some(current);
            info
        });
        if slider.changed() {
            set = Some(value as f64);
        }
        if field {
            let typed = detail.typed.entry(material.to_string()).or_insert_with(|| py_g_text(current));
            let before = typed.clone();
            let wrong =
                !typed.trim().is_empty() && stock::parse_number(typed).is_none_or(|q| !(floor..=1000.0).contains(&q));
            let response = red_if(row, scheme, wrong, |ui| {
                ui.add(
                    widgets::text_field(typed, &t("s_pl_q_bereich"), scheme)
                        .id_salt(("herstellung-q", item_salt, material))
                        .desired_width(60.0),
                )
            });
            let parsed = stock::parse_number(typed).filter(|q| (floor..=1000.0).contains(q));
            invalid = !typed.trim().is_empty() && parsed.is_none();
            if !response.has_focus() && set.is_some() {
                *typed = py_g_text(set.unwrap_or(current));
                invalid = false;
            } else if *typed != before
                && let Some(q) = parsed
            {
                set = Some(q);
            }
            if !response.has_focus() && set.is_none() && !invalid {
                *typed = py_g_text(current);
            }
        } else {
            row.label(RichText::new(py_g_text(current)).small());
        }
        let source = if !effective {
            t("s_he_ohne_wirkung")
        } else if detail.in_stock.get(material) != Some(&true) {
            t("s_he_regler_ohne")
        } else if detail.from_stock.get(material).is_some_and(|q| (current - q).abs() < 0.5) {
            t("s_he_regler_lager")
        } else {
            String::new()
        };
        if !source.is_empty() {
            row.add(egui::Label::new(sub(scheme, source)).wrap_mode(egui::TextWrapMode::Extend));
        }
        if field {
            let word = t("s_he_mischen");
            let button = widgets::icon_button(row, scheme, "plus", &word);
            let name = format!("{material}: {word}");
            button.widget_info(|| WidgetInfo::labeled(Role::Button, true, &name));
            mix = button.clicked();
        }
    });
    if field {
        if invalid {
            detail.bad.insert(material.to_string());
            ui.label(RichText::new(t("s_p9_qualitaet_falsch")).small().color(color(scheme.red)));
        } else {
            detail.bad.remove(material);
        }
    }
    if let Some(q) = set {
        let q = q.clamp(0.0, 1000.0);
        detail.stand.insert(material.to_string(), q);
        detail.auto.remove(material);
        if !field {
            detail.typed.remove(material);
        }
    }
    mix
}

/// A typed number: the field, red while it holds no number `fits` takes.
/// Gives the number when the text is one and differs from `shown` (the
/// value as written), whether the text is wrong, and whether the field has
/// the focus.
fn number_field(
    row: &mut Ui,
    scheme: &Scheme,
    text: &mut String,
    shown: &str,
    hint: &str,
    salt: impl egui::AsIdSalt,
    fits: impl Fn(f64) -> bool,
) -> (Option<f64>, bool, bool) {
    let parse = |text: &str| stock::parse_number(text).filter(|x| x.is_finite() && fits(*x));
    let wrong = !text.trim().is_empty() && parse(text).is_none();
    let response = red_if(row, scheme, wrong, |ui| {
        ui.add(widgets::text_field(text, hint, scheme).id_salt(salt).desired_width(52.0))
    });
    let parsed = parse(text);
    let changed = if text.as_str() != shown { parsed } else { None };
    (changed, !text.trim().is_empty() && parsed.is_none(), response.has_focus())
}

/// The lots of a mixed material, one row each with amount and quality, a
/// button for one more and the quality of the mix.
/// `floor` is the lowest quality the recipe takes for the material.
#[allow(clippy::too_many_arguments)]
fn mix_controls(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &mut CraftingState,
    material: &str,
    portion: f64,
    slot: usize,
    floor: f64,
) {
    let item_salt = state.open;
    let piece = state.data.is_piece(material);
    let Some(detail) = state.detail.as_mut() else { return };
    let Some(mut rows) = detail.mix.get(material).cloned() else { return };
    let auto = detail.auto.contains(material);
    let (mut changed, mut removed, mut added) = (false, None, false);
    let (mut wrong_amount, mut wrong_quality) = (false, false);
    for (n, part) in rows.iter_mut().enumerate() {
        ui.horizontal_wrapped(|row| {
            let salt = ("herstellung-mix-menge", item_salt, material, n);
            let shown = g(rounded3(part.amount));
            let (amount, wrong, focus) =
                number_field(row, scheme, &mut part.amount_text, &shown, &t("s_pl_menge"), salt, |x| x >= 0.0);
            if let Some(amount) = amount {
                part.amount = amount;
                changed = true;
            }
            wrong_amount |= wrong;
            if !focus && !wrong {
                part.amount_text = g(rounded3(part.amount));
            }
            row.label(sub(scheme, t(if piece { "s_he_mix_einheit_stk" } else { "s_he_mix_einheit" })));
            let mut value = part.quality.round() as i64;
            // Room for the quality field and its spacing after the slider.
            let width = (row.available_width() - 150.0).clamp(40.0, 160.0);
            let slider = row
                .push_id(("herstellung-mix-regler", slot, material, n), |row| {
                    row.add(widgets::Slider::new(&mut value, floor as i64, 1000, scheme).width(width))
                })
                .inner;
            let name = format!("{material} {}, Q", n + 1);
            let shown = part.quality;
            slider.widget_info(|| {
                let mut info = WidgetInfo::labeled(Role::Slider, true, &name);
                info.value = Some(shown);
                info
            });
            let slid = slider.changed();
            if slid {
                part.quality = value as f64;
                changed = true;
            }
            let salt = ("herstellung-mix-q", item_salt, material, n);
            let written = py_g_text(shown);
            let (quality, wrong, focus) =
                number_field(row, scheme, &mut part.quality_text, &written, &t("s_pl_q_bereich"), salt, |q| {
                    (floor..=1000.0).contains(&q)
                });
            if let Some(q) = quality.filter(|_| !slid) {
                part.quality = q;
                changed = true;
            }
            if slid || (!focus && !wrong) {
                part.quality_text = py_g_text(part.quality);
            }
            wrong_quality |= wrong && !slid;
            let source = match (auto, part.stocked) {
                (true, true) => t("s_he_regler_lager"),
                (true, false) => t("s_he_mix_rest"),
                _ => String::new(),
            };
            if !source.is_empty() {
                row.add(egui::Label::new(sub(scheme, source)).wrap_mode(egui::TextWrapMode::Extend));
            }
            let word = t("s_he_mix_weg");
            let button = widgets::icon_button(row, scheme, "x", &word);
            let name = format!("{material} {}: {word}", n + 1);
            button.widget_info(|| WidgetInfo::labeled(Role::Button, true, &name));
            if button.clicked() {
                removed = Some(n);
            }
        });
    }
    if wrong_amount {
        ui.label(RichText::new(t("s_he_mix_menge_falsch")).small().color(color(scheme.red)));
    }
    if wrong_quality {
        ui.label(RichText::new(t("s_p9_qualitaet_falsch")).small().color(color(scheme.red)));
    }
    let lots: Vec<Lot> = rows.iter().map(Part::lot).collect();
    let together = quality_mix::total(&lots);
    ui.horizontal_wrapped(|row| {
        let word = t("s_he_mix_dazu");
        if row.add(widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("plus")).clicked() {
            added = true;
        }
        if let Some(q) = quality_mix::mixed_quality(&lots) {
            row.label(widgets::bold(row, tn("s_he_mix_guete", &[Arg::from(py_g_text(q))]), egui::TextStyle::Body));
        }
    });
    if (together - portion).abs() > 1e-6 {
        let amount = |x: f64| {
            let key = if piece { "s_p9_menge_stk" } else { "s_p9_menge" };
            tn(key, &[Arg::from(g(rounded3(x)))])
        };
        let text = tn("s_he_mix_summe", &[Arg::from(amount(together)), Arg::from(amount(portion))]);
        ui.add(egui::Label::new(RichText::new(text).small().color(color(scheme.gold))).wrap());
    }
    if wrong_amount || wrong_quality {
        detail.bad.insert(material.to_string());
    } else {
        detail.bad.remove(material);
    }
    if added {
        let q = rows.last().map_or(START_QUALITY, |part| part.quality);
        rows.push(Part::new(0.0, q, false));
        changed = true;
    }
    if let Some(n) = removed {
        rows.remove(n);
        changed = true;
    }
    if changed {
        detail.auto.remove(material);
        rows.iter_mut().for_each(|part| part.stocked = false);
    }
    let lots: Vec<Lot> = rows.iter().map(Part::lot).collect();
    if let Some(q) = quality_mix::mixed_quality(&lots) {
        detail.stand.insert(material.to_string(), q);
    }
    if rows.len() > 1 {
        detail.mix.insert(material.to_string(), rows);
    } else {
        if let Some(last) = rows.first() {
            detail.stand.insert(material.to_string(), last.quality);
        }
        detail.mix.remove(material);
        detail.typed.remove(material);
    }
}

/// A field whose input is wrong gets the red border (like `widgets::checked_field`).
fn red_if(ui: &mut Ui, scheme: &Scheme, wrong: bool, add: impl FnOnce(&mut Ui) -> egui::Response) -> egui::Response {
    ui.scope(|ui| {
        if wrong {
            let red = egui::Stroke::new(1.0, color(scheme.red));
            let w = &mut ui.visuals_mut().widgets;
            (w.inactive.bg_stroke, w.hovered.bg_stroke) = (red, red);
            ui.visuals_mut().selection.stroke.color = color(scheme.red);
        }
        add(ui)
    })
    .inner
}

fn py_g_text(x: f64) -> String {
    language::py_g(x, false)
}

/// What the quality `q` of the material in this slot does to each property it touches.
fn effects(ui: &mut Ui, scheme: &Scheme, slot: &Slot, q: f64, laser: bool) {
    let mut groups: Vec<((String, String), Vec<&versekit_gamedata::values::Modifier>)> = Vec::new();
    for modifier in &slot.modifiers {
        let key = (modifier.property_name.clone().unwrap_or_default(), modifier.property_key.clone());
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, list)) => list.push(modifier),
            None => groups.push((key, vec![modifier])),
        }
    }
    for ((name, key), ranges) in groups {
        let Some(factor) = quality::factor(&ranges, q) else { continue };
        let higher = quality::higher_is_better(&ranges);
        let property = language::property_name(&name, Some(&key), laser);
        let (text, tone, percent) = if quality::is_absolute(&ranges) {
            let text = if factor == 0.0 {
                t("s_he_absolut_null")
            } else if factor < 0.0 {
                g(factor)
            } else {
                format!("+{}", g(factor))
            };
            let tone = if factor > 0.0 {
                widgets::change_color(scheme, true)
            } else if factor < 0.0 {
                widgets::change_color(scheme, false)
            } else {
                scheme.sub
            };
            (text, tone, String::new())
        } else {
            let good = if higher { factor >= 1.0 } else { factor <= 1.0 };
            (
                tn("s_p9_faktor", &[Arg::from(fixed(factor, 3))]),
                widgets::change_color(scheme, good),
                tn("s_p9_prozent", &[Arg::from(signed((factor - 1.0) * 100.0, 2))]),
            )
        };
        ui.horizontal_wrapped(|row| {
            row.add_space(12.0);
            row.label(sub(scheme, property));
            row.label(RichText::new(text).color(color(tone)));
            if !percent.is_empty() {
                row.label(RichText::new(percent).color(color(tone)));
            }
        });
        let mut span = String::new();
        if let Some(reach) = quality::range_of(&ranges) {
            span = match reach.base {
                Some(base) => tn(
                    "s_p9_spanne",
                    &[
                        Arg::from(g(reach.quality_from)),
                        Arg::from(g(reach.quality_to)),
                        Arg::from(g(reach.factor_from)),
                        Arg::from(g(reach.factor_to)),
                        Arg::from(g(base.round_ties_even())),
                    ],
                ),
                None => tn(
                    "s_p9_spanne_ohne",
                    &[
                        Arg::from(g(reach.quality_from)),
                        Arg::from(g(reach.quality_to)),
                        Arg::from(g(reach.factor_from)),
                        Arg::from(g(reach.factor_to)),
                    ],
                ),
            };
        }
        if !higher {
            span = format!("{} · {span}", t("s_he_weniger_gut"));
        }
        if let Some(raw) = ranges.iter().find_map(|m| m.corrected.iter().find(|c| c.field == "modifierAtStart"))
            && let Ok(raw) = raw.raw.parse::<f64>()
        {
            span = format!("{span} · {}", tn("s_p9_cig_wert", &[Arg::from(g(raw))]));
        }
        ui.horizontal_wrapped(|row| {
            row.add_space(12.0);
            row.label(sub(scheme, span));
        });
    }
}

// ------------------------------------------------------------ the values

/// A cell of the table; numbers stand right-aligned.
fn numeric(grid: &mut Ui, right: bool, text: RichText) {
    if right {
        grid.with_layout(egui::Layout::right_to_left(egui::Align::Center), |cell| cell.label(text));
    } else {
        grid.label(text);
    }
}

/// The table of the values of the product in a box: base, as crafted with the
/// qualities set now, and the change.
fn values_card(ui: &mut Ui, scheme: &Scheme, state: &mut CraftingState, values: &Arc<GameValues>) {
    let Some((_, _, recipe, ingredients)) = chosen(state, values) else { return };
    if unique_materials(&ingredients).is_empty() {
        return;
    }
    widgets::card(ui, scheme, |ui| values_table(ui, scheme, state, &recipe, &ingredients));
}

fn values_table(ui: &mut Ui, scheme: &Scheme, state: &mut CraftingState, recipe: &Recipe, ingredients: &[Ingredient]) {
    let Some(values) = state.values.clone() else { return };
    let Some(detail) = &state.detail else { return };
    let materials = unique_materials(ingredients);
    let tweaked = materials.iter().any(|m| {
        let stand = detail.stand.get(m).copied().unwrap_or(START_QUALITY);
        (stand - detail.from_stock.get(m).copied().unwrap_or(START_QUALITY)).abs() > 0.5
            || detail.in_stock.get(m) != Some(&true)
    });
    let head = if !tweaked && !detail.from_stock.is_empty() { t("s_he_werte") } else { t("s_he_werte_probe_je") };
    ui.label(widgets::bold(ui, head, egui::TextStyle::Body)).on_hover_text(t("s_he_werte_hinweis"));
    ui.add(egui::Label::new(sub(scheme, t("s_he_werte_hinweis"))).wrap());
    let product = values.product(recipe);
    let lines = product.map_or_else(Vec::new, |product| {
        let factors = quality::combined(recipe, |_, slot| detail.stand.get(slot.material()).copied());
        product_table::table(product, &factors)
    });
    if !lines.iter().any(|l| matches!(l, Line::Row(_))) {
        ui.label(sub(scheme, t("s_ps_keine")));
        return;
    }
    // Info lines before the first row stand above the grid and wrap there, so
    // a long one does not widen the first column and push the rest out.
    let lead = lines.iter().take_while(|l| matches!(l, Line::Info(_))).count();
    for line in &lines[..lead] {
        if let Line::Info(text) = line
            && !text.is_empty()
        {
            ui.add(egui::Label::new(RichText::new(text).small().color(color(scheme.gold))).wrap());
        }
    }
    egui::Grid::new(("herstellung-werte", state.open)).num_columns(4).striped(true).show(ui, |grid| {
        for (n, title) in ["s_ps_sp_wert", "s_ps_sp_grund", "s_ps_sp_gebaut", "s_ps_sp_diff"].into_iter().enumerate() {
            numeric(grid, n > 0, widgets::bold(grid, t(title), egui::TextStyle::Small).color(color(scheme.sub)));
        }
        grid.end_row();
        for line in &lines[lead..] {
            match line {
                Line::Info(text) => {
                    if !text.is_empty() {
                        grid.label(RichText::new(text).small().color(color(scheme.gold)));
                        grid.end_row();
                    }
                }
                Line::Section(title) => {
                    grid.label(widgets::bold(grid, title, egui::TextStyle::Small).color(color(scheme.gold)));
                    grid.end_row();
                }
                Line::Row(row) => {
                    let shown = product_table::formatted_in(row, language::current());
                    let tone = rating_color(scheme, shown.rating);
                    let highlight = row.better == product_table::Better::Dps;
                    let label = if highlight {
                        widgets::bold(grid, &row.label, egui::TextStyle::Body)
                    } else {
                        RichText::new(&row.label)
                    };
                    grid.label(label);
                    numeric(grid, true, RichText::new(shown.base).small());
                    numeric(grid, true, RichText::new(shown.made).small().color(color(tone)));
                    numeric(grid, true, RichText::new(shown.change).small().color(color(tone)));
                    grid.end_row();
                }
            }
        }
    });
}

/// The tests: without a window, on the three made-up recipes of the sample, with a data folder of their own.
#[cfg(test)]
mod tests {
    use egui::{Pos2, Rect, vec2};
    use versekit_core::jsonio;
    use versekit_core::language::{Choice, set_language};
    use versekit_core::paths::Host;
    use versekit_core::pypath::Flavor;

    use super::list::{self, Line as ListLine};
    use super::*;

    #[test]
    fn a_long_menu_stays_inside_the_window_and_scrolls() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let scheme = versekit_core::theme::by_id("original").expect("original");
        let mut height = 0.0;
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 400.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                let shown = ui.scope(|ui| {
                    list::menu_entries(ui, scheme, |menu| {
                        for n in 0..60 {
                            menu.label(format!("Schiff {n}"));
                        }
                    });
                });
                height = shown.response.rect.height();
            });
            // Headless: no renderer takes the font texture.
            output.textures_delta.clear();
        }
        assert!(height > 200.0 && height <= 400.0 - 2.0 * list::MENU_MARGIN, "60 ships in a 400 high window: {height}");
    }

    #[test]
    fn it_says_why_the_game_values_are_not_there() {
        // The sample holds values.
        assert_eq!(CraftingState::default().values_wait(), None);
        let mut state = CraftingState::empty(None);
        assert_eq!(state.values_wait(), Some(ValuesWait::None), "no values, nothing read, nothing failed");
        state.failed = true;
        assert_eq!(state.values_wait(), Some(ValuesWait::Failed));
    }

    fn paths(name: &str) -> Paths {
        let dir =
            std::env::temp_dir().join(format!("versekit-herstellung-{}-{name}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let text = dir.to_string_lossy().into_owned();
        Paths::new(Host::new(Flavor::native(), &[(versekit_core::legacy::ENV_HOME, &text)], &text))
    }

    fn entries(text: &str) -> Vec<Value> {
        match jsonio::loads(text).unwrap() {
            Value::Array(items) => items,
            _ => panic!("array expected"),
        }
    }

    /// The page on the sample recipes with a data folder of its own and a stock.
    fn state(name: &str, stock_json: &str) -> CraftingState {
        let paths = paths(name);
        let mut state = CraftingState { paths: Some(paths), ..CraftingState::default() };
        let rows = entries(stock_json);
        assert!(stock::save(&state.stock_file().unwrap(), &rows, "test"));
        state.reload_stock();
        state
    }

    fn texts(shapes: &[egui::epaint::ClippedShape]) -> Vec<(String, Rect)> {
        fn collect(shape: &egui::epaint::Shape, out: &mut Vec<(String, Rect)>) {
            match shape {
                egui::epaint::Shape::Text(text) => {
                    out.push((text.galley.text().to_string(), Rect::from_min_size(text.pos, text.galley.size())))
                }
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut out));
        out
    }

    /// One frame with the made-up blueprints held as the window runs it.
    fn frame(ctx: &egui::Context, state: &mut CraftingState, held: &[&str], width: f32) -> Vec<String> {
        frame_with(ctx, state, held, width, &CatalogCache::default())
    }

    fn frame_with(
        ctx: &egui::Context,
        state: &mut CraftingState,
        held: &[&str],
        width: f32,
        catalog: &CatalogCache,
    ) -> Vec<String> {
        let have: HashSet<String> = held.iter().map(|n| name_key(n)).collect();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 2000.0))),
            ..Default::default()
        };
        let scheme = versekit_core::theme::current();
        let mut output = ctx.run_ui(input, |ui| {
            show_page(ui, scheme, state, &Shared { have: &have, catalog });
        });
        output.textures_delta.clear();
        texts(&output.shapes).into_iter().map(|(text, _)| text).collect()
    }

    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx
    }

    const STOCK: &str = r#"[{"material":"Iron","menge":2.0,"qualitaet":700,"ort":"Levski"},
                            {"material":"Gold","menge":0.1,"qualitaet":700,"ort":"Levski"}]"#;

    /// Enough for Frostline (Iron 0.5 and Gold 0.2 a piece), nothing for the others.
    const RICH: &str = r#"[{"material":"Iron","menge":2.0,"qualitaet":700,"ort":"Levski"},
                           {"material":"Gold","menge":1.0,"qualitaet":700,"ort":"Levski"}]"#;

    /// The page driven like with a mouse: frames with events, the texts and the
    /// named elements of the last frame with their places.
    struct Bench {
        ctx: egui::Context,
        page: CraftingState,
        catalog: CatalogCache,
        have: HashSet<String>,
        width: f32,
        time: f64,
        texts: Vec<(String, Rect)>,
        nodes: Vec<(egui::accesskit::Role, String, Rect)>,
    }

    impl Bench {
        fn new(name: &str, stock_json: &str, width: f32) -> Bench {
            let ctx = context();
            ctx.enable_accesskit();
            Bench {
                ctx,
                page: state(name, stock_json),
                catalog: CatalogCache::default(),
                have: HashSet::new(),
                width,
                time: 10.0,
                texts: Vec::new(),
                nodes: Vec::new(),
            }
        }

        fn frame_with(&mut self, events: Vec<egui::Event>) -> Vec<String> {
            self.time += 0.05;
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(self.width, 2000.0))),
                time: Some(self.time),
                events,
                ..Default::default()
            };
            let scheme = versekit_core::theme::current();
            let (page, have, catalog) = (&mut self.page, &self.have, &self.catalog);
            let mut output = self.ctx.run_ui(input, |ui| {
                show_page(ui, scheme, page, &Shared { have, catalog });
            });
            output.textures_delta.clear();
            self.texts = texts(&output.shapes);
            self.nodes = output
                .platform_output
                .accesskit_update
                .map(|update| {
                    update
                        .nodes
                        .iter()
                        .filter_map(|(_, node)| {
                            let b = node.bounds()?;
                            let corner = |x: f64, y: f64| Pos2::new(x as f32, y as f32);
                            Some((
                                node.role(),
                                node.label().unwrap_or_default().to_string(),
                                Rect::from_min_max(corner(b.x0, b.y0), corner(b.x1, b.y1)),
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default();
            self.texts.iter().map(|(t, _)| t.clone()).collect()
        }

        fn frame(&mut self) -> Vec<String> {
            self.frame_with(Vec::new())
        }

        /// Presses and lets go at a point, then two quiet frames.
        fn press_at(&mut self, at: Pos2) -> Vec<String> {
            let button = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            self.frame_with(vec![egui::Event::PointerMoved(at)]);
            self.frame_with(vec![button(true)]);
            self.frame_with(vec![button(false)]);
            // The pointer leaves, so no hover stays open.
            self.frame_with(vec![egui::Event::PointerGone]);
            self.frame()
        }

        /// Clicks the first text that is `label`.
        fn click_text(&mut self, label: &str) -> Vec<String> {
            let shown = self.frame();
            let at = self
                .texts
                .iter()
                .find(|(text, _)| text == label)
                .map(|(_, rect)| rect.center())
                .unwrap_or_else(|| panic!("{label} is not on the screen: {shown:?}"));
            self.press_at(at)
        }

        fn place(&mut self, role: egui::accesskit::Role, name: &str) -> Rect {
            self.frame();
            self.nodes
                .iter()
                .find(|(r, n, _)| *r == role && n == name)
                .map(|(_, _, rect)| *rect)
                .unwrap_or_else(|| panic!("no {role:?} {name}: {:?}", self.nodes))
        }

        fn click_node(&mut self, role: egui::accesskit::Role, name: &str) -> Vec<String> {
            let at = self.place(role, name).center();
            self.press_at(at)
        }

        fn click_button(&mut self, name: &str) -> Vec<String> {
            self.click_node(egui::accesskit::Role::Button, name)
        }

        /// Puts the named slider to `units` with one press at the place of that value.
        fn set_slider(&mut self, name: &str, units: i64) -> Vec<String> {
            let rect = self.place(egui::accesskit::Role::Slider, name);
            let x = rect.min.x + widgets::slider_x(rect.width(), 0, 1000, units);
            self.press_at(Pos2::new(x, rect.center().y))
        }

        fn index_of(&self, name: &str) -> usize {
            self.page.book.items.iter().position(|i| i.name == name).unwrap()
        }
    }

    fn has(shown: &[String], text: &str) -> bool {
        shown.iter().any(|s| s == text)
    }

    #[test]
    fn without_a_choice_the_list_shows_everything_and_the_other_side_asks_for_one() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut page = state("ask", STOCK);
        let shown = frame(&context(), &mut page, &["Frostline"], 1300.0);
        assert!(shown.iter().any(|s| s.contains("von 3 herstellbar")), "{shown:?}");
        for name in ["Frostline", "Arc Shield", "Grand Core"] {
            assert!(has(&shown, name), "{name} is in the list: {shown:?}");
        }
        assert!(has(&shown, &t("s_fb_waehlen")), "{shown:?}");
        // The headings of the subcategories stand over the items.
        assert!(has(&shown, "Vehikel  ·  Kühler"), "{shown:?}");
        assert!(!has(&shown, &t("s_fb_rezept")), "no recipe without a choice");
    }

    /// The page with these limits of the fabricator in its values.
    fn with_limits(name: &str, limits: Option<versekit_gamedata::values::CrafterLimits>) -> CraftingState {
        let mut page = state(name, STOCK);
        let mut values = page.values.as_deref().cloned().expect("values");
        values.crafter_limits = limits;
        page.take_values(Arc::new(values), "archive");
        page
    }

    /// The texts of one frame with their places.
    fn frame_rects(page: &mut CraftingState, width: f32) -> Vec<(String, Rect)> {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 2000.0))),
            ..Default::default()
        };
        let scheme = versekit_core::theme::current();
        let (have, catalog) = (HashSet::new(), CatalogCache::default());
        let mut output = context().run_ui(input, |ui| {
            show_page(ui, scheme, page, &Shared { have: &have, catalog: &catalog });
        });
        output.textures_delta.clear();
        texts(&output.shapes)
    }

    #[test]
    fn the_limits_of_the_fabricator_stand_under_the_head_and_nothing_is_cut_off() {
        use versekit_gamedata::values::CrafterLimits;
        let _language = crate::language_lock();
        let both = CrafterLimits { in_progress: 2, waiting: 4, history: 10, dismantle: Some((4, 4)) };
        let lines = [
            (Choice::De, "Fabrikator: 2 in Arbeit, 4 wartend, 10 im Verlauf  ·  Zerlegen 4/4"),
            (Choice::En, "Fabricator: 2 in progress, 4 waiting, 10 in the history  ·  Dismantling 4/4"),
        ];
        for (language, line) in lines {
            set_language(language);
            for width in [260.0, 380.0, 640.0, 900.0, 1300.0] {
                let mut page = with_limits("grenzen", Some(both));
                let shown = frame_rects(&mut page, width);
                let (_, rect) = shown
                    .iter()
                    .find(|(text, _)| text == line)
                    .unwrap_or_else(|| panic!("{width}: no {line:?} in {shown:?}"));
                assert!(rect.min.x >= 0.0 && rect.max.x <= width, "{width}: {line:?} sticks out: {rect:?}");
                assert!(rect.height() > 0.0 && rect.min.y >= 0.0, "{width}: {rect:?}");
                assert_eq!(page.list_overflow, 0.0, "{width}: nothing sticks out of the page");
            }
        }
        // Without a queue of dismantling the line ends after the history.
        set_language(Choice::De);
        let some = CrafterLimits { dismantle: None, ..both };
        let shown = frame_rects(&mut with_limits("grenzen-ohne-zerlegen", Some(some)), 900.0);
        assert!(shown.iter().any(|(text, _)| text == "Fabrikator: 2 in Arbeit, 4 wartend, 10 im Verlauf"), "{shown:?}");
        assert!(!shown.iter().any(|(text, _)| text.contains("Zerlegen 4/4")), "{shown:?}");
    }

    #[test]
    fn without_game_data_that_state_the_limits_no_line_is_shown() {
        let _language = crate::language_lock();
        for language in [Choice::De, Choice::En] {
            set_language(language);
            let shown = frame_rects(&mut with_limits("ohne-grenzen", None), 900.0);
            assert!(!shown.is_empty(), "the page is there");
            assert!(
                shown.iter().all(|(text, _)| !text.starts_with("Fabrikator") && !text.starts_with("Fabricator:")),
                "{shown:?}"
            );
        }
    }

    #[test]
    fn the_search_finds_by_name_maker_category_and_ingredient() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut page = state("search", STOCK);
        let ctx = context();
        // The maker: "Greycat" is in no name, so only the maker can find Grand Core.
        page.search = "greycat".into();
        let shown = frame(&ctx, &mut page, &[], 1300.0);
        assert!(has(&shown, "Grand Core") && !has(&shown, "Frostline"), "{shown:?}");
        page.search = "basilisk".into();
        assert!(frame(&ctx, &mut page, &[], 1300.0).iter().any(|s| s == "Arc Shield"));
        // The subcategory as the game names it.
        page.search = "kühler".into();
        let shown = frame(&ctx, &mut page, &[], 1300.0);
        assert!(has(&shown, "Frostline") && !has(&shown, "Arc Shield"), "{shown:?}");
        page.search = "agri".into();
        let shown = frame(&ctx, &mut page, &[], 1300.0);
        assert!(shown.contains(&tn("s_p9_aus_eins", &[Arg::from("Agricium")])), "{shown:?}");
        assert!(has(&shown, "Arc Shield"), "{shown:?}");
        page.search = "xyz".into();
        assert!(frame(&ctx, &mut page, &[], 1300.0).contains(&t("s_he_nichts")));
    }

    /// Counter-probe of the maker search: the maker is what finds the item.
    #[test]
    fn without_the_maker_in_the_search_the_maker_finds_nothing() {
        let page = state("maker", STOCK);
        let index = page.book.items.iter().position(|i| i.name == "Grand Core").unwrap();
        let item = page.book.items[index].clone();
        let names = Names::default();
        let none = HashSet::new();
        assert!(list::found(&item, page.places[index], &names, "greycat", &none));
        let without = CraftItem { manufacturer: String::new(), ..item };
        assert!(!list::found(&without, page.places[index], &names, "greycat", &none));
    }

    #[test]
    fn a_material_that_is_in_no_recipe_says_so() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut page = state("none", STOCK);
        // Hadanite is not in the made-up recipes but in the closed list of the stock data.
        page.data.add_recipe_materials([("Hadanite".to_string(), true, 0.0, 0.0)]);
        page.search = "hadan".into();
        let shown = frame(&context(), &mut page, &[], 1300.0);
        assert!(shown.contains(&tn("s_he_aus_keine", &[Arg::from("Hadanite")])), "{shown:?}");
    }

    #[test]
    fn a_chosen_item_says_what_is_there_and_what_is_missing() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        for width in [380.0, 900.0, 1300.0] {
            let mut page = state("open", STOCK);
            page.open = page.book.items.iter().position(|i| i.name == "Frostline");
            let shown = frame(&context(), &mut page, &[], width);
            // Iron 0.5 is covered by the crate of 2; of Gold 0.2 only 0.1 is there.
            let gold = tn("s_p9_teil", &[Arg::from("0,1"), Arg::from("0,2"), Arg::from("0,1")]);
            assert!(shown.contains(&tn("s_p9_da", &[Arg::from("2")])), "{width}: {shown:?}");
            assert!(shown.contains(&gold), "{width}: {shown:?}");
            assert!(shown.contains(&t("s_lg_bauen")), "{width}: {shown:?}");
            assert!(has(&shown, "FRAME") && has(&shown, "CORE"), "one block per slot: {shown:?}");
            assert!(has(&shown, "Vehikel  ·  Kühler"), "the category path: {shown:?}");
            assert!(has(&shown, &tf("s_fb_stufe", &[("n", Arg::Int(1))])), "{shown:?}");
            assert!(has(&shown, &duration(900)), "the time: {shown:?}");
            // The English page writes the decimal point.
            set_language(Choice::En);
            let shown = frame(&context(), &mut page, &[], width);
            let gold = tn("s_p9_teil", &[Arg::from("0.1"), Arg::from("0.2"), Arg::from("0.1")]);
            assert!(shown.contains(&gold), "{shown:?}");
            set_language(Choice::De);
        }
    }

    #[test]
    fn choosing_an_item_shows_its_recipe_and_values() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut bench = Bench::new("choose", STOCK, 1300.0);
        let shown = bench.frame();
        assert!(has(&shown, &t("s_fb_waehlen")) && !has(&shown, "Agricium"), "{shown:?}");
        let shown = bench.click_text("Arc Shield");
        assert_eq!(bench.page.open, Some(bench.index_of("Arc Shield")));
        assert!(!has(&shown, &t("s_fb_waehlen")), "{shown:?}");
        assert!(has(&shown, &t("s_fb_rezept")) && has(&shown, "Agricium"), "{shown:?}");
        assert!(has(&shown, &t("s_ps_sp_gebaut")), "the table of values: {shown:?}");
        // The list stays beside it.
        assert!(has(&shown, "Frostline") && has(&shown, "Grand Core"), "{shown:?}");
    }

    #[test]
    fn the_category_menu_filters_and_a_ticked_entry_takes_the_filter_away() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut bench = Bench::new("category", STOCK, 1300.0);
        let shown = bench.click_node(egui::accesskit::Role::Button, "Vehikel");
        assert!(
            has(&shown, "Alle Kategorien (3)") && has(&shown, "Kühler (1)") && has(&shown, "Radar (0)"),
            "{shown:?}"
        );
        let shown = bench.click_text("Kühler (1)");
        let vehicle = fabricator::category_index("vehikel");
        assert_eq!((bench.page.picks.category, bench.page.picks.sub), (vehicle, Some("kuehler")));
        assert!(has(&shown, "Frostline") && !has(&shown, "Arc Shield") && !has(&shown, "Grand Core"), "{shown:?}");
        // The same entry again: the filter goes.
        bench.click_node(egui::accesskit::Role::Button, "Vehikel");
        let shown = bench.click_text("Kühler (1)");
        assert_eq!(bench.page.picks.category, None);
        assert!(has(&shown, "Arc Shield"), "{shown:?}");
        // A category without subcategories filters with one click: no item is ammunition.
        let shown = bench.click_node(egui::accesskit::Role::Button, "Munition");
        assert!(has(&shown, &t("s_he_nichts")), "{shown:?}");
        let shown = bench.click_button(&t("s_fb_filter_leeren"));
        assert!(!bench.page.is_narrowed() && has(&shown, "Frostline"), "{shown:?}");
    }

    #[test]
    fn a_pick_in_a_menu_switches_and_the_same_pick_again_clears() {
        let mut picks = Picks::default();
        list::pick(&mut picks, 5, Some("kuehler"));
        assert_eq!((picks.category, picks.sub), (Some(5), Some("kuehler")));
        list::pick(&mut picks, 5, None);
        assert_eq!((picks.category, picks.sub), (Some(5), None), "all of the category");
        list::pick(&mut picks, 5, None);
        assert_eq!((picks.category, picks.sub), (None, None));
    }

    #[test]
    fn the_switches_show_what_is_affordable_and_what_is_owned() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut bench = Bench::new("switches", RICH, 1300.0);
        bench.have = ["Grand Core"].iter().map(|n| name_key(n)).collect();
        let (owned, enough) = ("Bauplan vorhanden", "Material reicht");
        assert_eq!((t("s_fb_besitz"), t("s_fb_erschwinglich")), (owned.to_string(), enough.to_string()));
        let shown = bench.click_node(egui::accesskit::Role::CheckBox, owned);
        assert_eq!(bench.page.picks.state, "habe");
        assert!(has(&shown, "Grand Core") && !has(&shown, "Frostline"), "{shown:?}");
        let shown = bench.click_node(egui::accesskit::Role::CheckBox, owned);
        assert!(bench.page.picks.state.is_empty() && has(&shown, "Frostline"), "{shown:?}");
        // Only Frostline is covered by the stock.
        let shown = bench.click_node(egui::accesskit::Role::CheckBox, enough);
        assert_eq!(bench.page.picks.material, "reicht");
        assert!(has(&shown, "Frostline") && !has(&shown, "Arc Shield") && !has(&shown, "Grand Core"), "{shown:?}");
        // Both: nothing is affordable and owned.
        let shown = bench.click_node(egui::accesskit::Role::CheckBox, owned);
        assert!(has(&shown, &t("s_he_nichts")), "{shown:?}");
        // The drop-downs of the blueprint and the material are gone: the switches say the same.
        bench.page.picks = Picks::default();
        let shown = bench.frame();
        assert!(has(&shown, "Frostline"), "{shown:?}");
        assert!(!has(&shown, &t("ff_alle_zustaende")) && !has(&shown, &t("ff_alle_material")), "{shown:?}");
    }

    /// The value of the integrity of Frostline as the table writes it when the iron has quality `q`.
    fn integrity_at(page: &CraftingState, q: f64) -> (String, String) {
        let values = page.values.clone().unwrap();
        let item = &page.book.items[page.open.unwrap()];
        let recipe = &values.recipes[item.recipe];
        let product = values.product(recipe).unwrap();
        let factors = quality::combined(recipe, |_, slot| Some(if slot.material() == "Iron" { q } else { 500.0 }));
        let row = product_table::table(product, &factors)
            .into_iter()
            .find_map(|line| match line {
                Line::Row(row)
                    if row.label == language::property_name("Integrity", Some("health_maxhealth"), false) =>
                {
                    Some(row)
                }
                _ => None,
            })
            .expect("the table has the integrity");
        let shown = product_table::formatted_in(&row, language::current());
        (shown.made, shown.change)
    }

    #[test]
    fn the_values_follow_the_slider_at_once() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut bench = Bench::new("values", STOCK, 1300.0);
        bench.click_text("Frostline");
        let shown = bench.frame();
        // Iron comes from the stock at 700, gold is not usable there and stands at 500.
        let (made_700, change_700) = integrity_at(&bench.page, 700.0);
        assert!(has(&shown, &made_700) && has(&shown, &change_700), "{made_700} {change_700}: {shown:?}");
        let shown = bench.set_slider("Iron, Q", 1000);
        assert_eq!(bench.page.detail.as_ref().unwrap().stand["Iron"], 1000.0);
        let (made_1000, change_1000) = integrity_at(&bench.page, 1000.0);
        assert_ne!(made_700, made_1000, "the value must change with the quality");
        assert!(has(&shown, &made_1000) && has(&shown, &change_1000), "{made_1000} {change_1000}: {shown:?}");
        assert!(!has(&shown, &made_700), "the old value is gone: {shown:?}");
    }

    #[test]
    fn autofill_takes_the_stock_and_else_the_best_or_the_lowest_quality() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut bench = Bench::new("fill", STOCK, 1300.0);
        bench.click_text("Frostline");
        let stand = |b: &Bench, m: &str| b.page.detail.as_ref().unwrap().stand[m];
        // Opened: iron from the stock; of the gold 0.1 lies in the stock at 700, the other 0.1
        // counts at the quality of bought goods: 600 together.
        assert_eq!((stand(&bench, "Iron"), stand(&bench, "Gold")), (700.0, 600.0));
        bench.click_button(&t("s_he_schlechtestes"));
        assert_eq!(bench.page.fill, Order::Worst);
        // The rest at the lowest the recipe takes (0).
        assert_eq!((stand(&bench, "Iron"), stand(&bench, "Gold")), (700.0, 350.0));
        bench.click_button(&t("s_he_bestes"));
        assert_eq!((stand(&bench, "Iron"), stand(&bench, "Gold")), (700.0, 850.0), "the rest at the best");
        // Counter-probe: set from the stock again puts the rest back to the quality of bought goods.
        bench.click_button(&t("s_he_zurueck_lager"));
        assert_eq!(stand(&bench, "Gold"), 600.0);
    }

    /// Frostline (Iron 0.5 a piece) open on the bench with `stock_json`.
    fn mix_bench(name: &str, stock_json: &str) -> Bench {
        let mut bench = Bench::new(name, stock_json, 1300.0);
        bench.click_text("Frostline");
        bench
    }

    fn iron_rows(bench: &Bench) -> Vec<(f64, f64)> {
        let detail = bench.page.detail.as_ref().unwrap();
        detail.mix.get("Iron").map_or_else(Vec::new, |rows| rows.iter().map(|r| (r.amount, r.quality)).collect())
    }

    const TWO_IRONS: &str = r#"[{"material":"Iron","menge":0.3,"qualitaet":865,"ort":"Levski"},
                                {"material":"Iron","menge":2.0,"qualitaet":797,"ort":"Area18"},
                                {"material":"Gold","menge":1.0,"qualitaet":700,"ort":"Levski"}]"#;

    #[test]
    fn the_best_fill_mixes_the_best_lots_and_crafting_takes_them() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut bench = mix_bench("mix-best", TWO_IRONS);
        bench.click_button(&t("s_he_bestes"));
        // 0.3 at 865 and 0.2 at 797: (259.5 + 159.4) / 0.5 = 837.8.
        assert_eq!(iron_rows(&bench), vec![(0.3, 865.0), (0.2, 797.0)]);
        let shown = bench.frame();
        assert_eq!(bench.page.detail.as_ref().unwrap().stand["Iron"], 838.0);
        assert!(has(&shown, &tn("s_he_mix_guete", &[Arg::from("838")])), "{shown:?}");
        assert!(has(&shown, &tn("s_p9_da", &[Arg::from("2,3")])), "both crates count: {shown:?}");
        let (made, _) = integrity_at(&bench.page, 838.0);
        assert!(has(&shown, &made), "{made}: {shown:?}");
        // Crafting takes exactly these lots.
        let index = bench.page.open.unwrap();
        let item = bench.page.book.items[index].clone();
        let recipe = bench.page.values.clone().unwrap().recipes[item.recipe].clone();
        let ingredients = craft_book::ingredients(&recipe).remove(0);
        craft(&mut bench.page, &item, &ingredients);
        assert_eq!(bench.page.detail.as_ref().unwrap().feedback, Some(Feedback::Deducted(1)));
        let after = stock::load(&bench.page.stock_file().unwrap(), "test");
        let iron: Vec<(f64, f64)> = after
            .iter()
            .filter(|e| e.get("material").and_then(Value::as_str) == Some("Iron"))
            .map(|e| (stock::number_of(e, "menge"), stock::number_of(e, "qualitaet")))
            .collect();
        assert_eq!(iron, vec![(1.8, 797.0)], "{after:?}");
        // Counter-probe: the worst fill takes one quality and needs no mix.
        let mut bench = mix_bench("mix-worst", TWO_IRONS);
        bench.click_button(&t("s_he_schlechtestes"));
        assert!(iron_rows(&bench).is_empty());
        assert_eq!(bench.page.detail.as_ref().unwrap().stand["Iron"], 797.0);
    }

    #[test]
    fn lots_mixed_by_hand_give_the_weighted_quality_and_what_is_missing() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut bench = mix_bench("mix-hand", TWO_IRONS);
        // The worst fill takes one quality: the normal case of one slider; the plus beside it
        // adds a second lot.
        bench.click_button(&t("s_he_schlechtestes"));
        assert!(iron_rows(&bench).is_empty());
        bench.click_button(&format!("Iron: {}", t("s_he_mischen")));
        assert_eq!(iron_rows(&bench).len(), 2);
        assert!(!bench.page.detail.as_ref().unwrap().auto.contains("Iron"));
        // 0.3 SCU at 800 and 0.2 SCU at 900, typed: 840.
        let rows = bench.page.detail.as_mut().unwrap().mix.get_mut("Iron").unwrap();
        (rows[0].amount_text, rows[0].quality_text) = ("0,3".into(), "800".into());
        (rows[1].amount_text, rows[1].quality_text) = ("0,2".into(), "900".into());
        bench.frame();
        let shown = bench.frame();
        assert_eq!(iron_rows(&bench), vec![(0.3, 800.0), (0.2, 900.0)]);
        assert!(has(&shown, &tn("s_he_mix_guete", &[Arg::from("840")])), "{shown:?}");
        // Nothing lies at 900: 0.3 from the 865, the 0.2 at 900 is missing.
        let missing = tn("s_p9_teil", &[Arg::from("0,3"), Arg::from("0,5"), Arg::from("0,2")]);
        assert!(has(&shown, &missing), "{shown:?}");
        assert!(has(&shown, &tn("s_p9_zu_schlecht", &[Arg::from("2"), Arg::from("800")])), "{shown:?}");
        // Less than the recipe needs is said.
        let rows = bench.page.detail.as_mut().unwrap().mix.get_mut("Iron").unwrap();
        rows[1].amount_text = "0,1".into();
        let shown = bench.frame();
        let summe = tn(
            "s_he_mix_summe",
            &[Arg::from(tn("s_p9_menge", &[Arg::from("0,4")])), Arg::from(tn("s_p9_menge", &[Arg::from("0,5")]))],
        );
        assert!(has(&shown, &summe), "{shown:?}");
        // A wrong amount is said and blocks crafting.
        let rows = bench.page.detail.as_mut().unwrap().mix.get_mut("Iron").unwrap();
        rows[1].amount_text = "-1".into();
        let shown = bench.frame();
        assert!(has(&shown, &t("s_he_mix_menge_falsch")), "{shown:?}");
        assert!(bench.page.detail.as_ref().unwrap().bad.contains("Iron"));
        // Removing a lot down to one gives the slider back.
        let rows = bench.page.detail.as_mut().unwrap().mix.get_mut("Iron").unwrap();
        rows[1].amount_text = "0,2".into();
        bench.frame();
        bench.click_button(&format!("Iron 1: {}", t("s_he_mix_weg")));
        assert!(iron_rows(&bench).is_empty());
        assert_eq!(bench.page.detail.as_ref().unwrap().stand["Iron"], 900.0);
        assert!(bench.page.detail.as_ref().unwrap().bad.is_empty());
        bench.place(egui::accesskit::Role::Slider, "Iron, Q");
    }

    #[test]
    fn the_count_multiplies_the_amounts_and_max_takes_what_the_stock_covers() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut bench = Bench::new("count-buttons", RICH, 1300.0);
        bench.click_text("Frostline");
        let shown = bench.click_node(egui::accesskit::Role::Button, &t("s_fb_mehr"));
        assert_eq!(bench.page.detail.as_ref().unwrap().count, "2");
        let doubled = tn("s_p9_menge_n", &[Arg::from(g(1.0)), Arg::from(g(0.5)), Arg::Int(2)]);
        assert!(has(&shown, &doubled), "{doubled}: {shown:?}");
        bench.click_node(egui::accesskit::Role::Button, &t("s_fb_weniger"));
        bench.click_node(egui::accesskit::Role::Button, &t("s_fb_weniger"));
        assert_eq!(bench.page.detail.as_ref().unwrap().count, "1", "never below one");
        bench.click_button(&t("s_fb_max"));
        let index = bench.page.open.unwrap();
        let recipe = bench.page.values.clone().unwrap().recipes[bench.page.book.items[index].recipe].clone();
        let ingredients = craft_book::ingredients(&recipe).remove(0);
        let most = max_count(&bench.page, &ingredients);
        assert!(most > 1, "the stock covers more than one: {most}");
        assert_eq!(bench.page.detail.as_ref().unwrap().count, most.to_string());
        // Counter-probe: one more than the most is not covered.
        let detail = bench.page.detail.clone().unwrap();
        let now = parts_now(&ingredients, &detail);
        assert!(!covered(&bench.page.stock, &now, most + 1), "{now:?}");
    }

    #[test]
    fn max_does_nothing_where_the_stock_covers_no_piece() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut bench = Bench::new("count-none", "[]", 1300.0);
        bench.click_text("Frostline");
        bench.click_button(&t("s_fb_max"));
        assert_eq!(bench.page.detail.as_ref().unwrap().count, "1");
    }

    #[test]
    fn a_narrow_window_shows_the_list_or_the_choice_with_the_way_back() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut bench = Bench::new("narrow", STOCK, 380.0);
        let shown = bench.frame();
        assert!(has(&shown, "Frostline") && has(&shown, "Arc Shield"), "{shown:?}");
        assert!(!has(&shown, &t("s_fb_waehlen")), "no empty side beside the list: {shown:?}");
        let shown = bench.click_text("Arc Shield");
        assert!(has(&shown, &t("s_ld_zurueck_liste")) && has(&shown, &t("s_fb_rezept")), "{shown:?}");
        assert!(!has(&shown, "Frostline"), "the list gives its room to the choice: {shown:?}");
        let shown = bench.click_button(&t("s_ld_zurueck_liste"));
        assert_eq!(bench.page.open, None);
        assert!(has(&shown, "Frostline") && !has(&shown, &t("s_fb_rezept")), "{shown:?}");
        // In between: list beside the choice, recipe and values one under the other.
        bench.width = 900.0;
        bench.click_text("Arc Shield");
        let shown = bench.frame();
        assert!(has(&shown, "Frostline") && has(&shown, &t("s_fb_rezept")), "{shown:?}");
        assert!(!has(&shown, &t("s_ld_zurueck_liste")), "{shown:?}");
    }

    #[test]
    fn the_sides_stand_as_the_width_allows() {
        let em = 17.0;
        assert_eq!(Columns::for_width(380.0, em), Columns::One);
        assert_eq!(Columns::for_width(900.0, em), Columns::Two);
        assert_eq!(Columns::for_width(1300.0, em), Columns::Three);
        // A larger font needs more room for the same.
        assert_eq!(Columns::for_width(1300.0, 24.0), Columns::Two);
    }

    #[test]
    fn the_list_groups_under_the_subcategories_and_sorts_within() {
        let mut page = state("sort", STOCK);
        let marks = vec![Have::No; page.book.items.len()];
        let (shown, _) = list::matching(&mut page, &marks);
        let lines = list::lines(&page.places, &shown, 4);
        let heads = lines.iter().filter(|l| matches!(l, ListLine::Head(_))).count();
        assert_eq!(heads, 3, "three subcategories: {lines:?}");
        // The cooler comes first: Kühler stands before Kraftwerke and Schilde in the terminal.
        assert_eq!(page.book.items[shown[0]].name, "Frostline");
        page.sort = Sort::Maker;
        let (by_maker, _) = list::matching(&mut page, &marks);
        assert_eq!(by_maker.len(), shown.len());
        assert!(list::lines(&page.places, &shown, 1).iter().all(|l| match l {
            ListLine::Items(items) => items.len() == 1,
            ListLine::Head(_) => true,
        }));
        assert_eq!(View::from_word(Some(View::List.word())), View::List);
        assert_eq!(View::from_word(None), View::Grid);
    }

    #[test]
    fn the_list_goes_by_size_before_the_name_within_a_subcategory() {
        let mut page = state("groesse", STOCK);
        let n = page.book.items.len();
        assert!(n >= 3, "{n}");
        // Three of them in one subcategory, named as the book sorts them (by name), sized against it.
        let place = page.places[0];
        for (i, name) in ["Omnisky III Cannon", "Omnisky IX Cannon", "Omnisky VI Cannon"].iter().enumerate() {
            page.book.items[i].name = (*name).into();
            page.places[i] = place;
        }
        page.sizes = vec![None; n];
        page.sizes[..3].copy_from_slice(&[Some(1), Some(3), Some(2)]);
        let marks = vec![Have::No; n];
        let first_three = |page: &CraftingState, shown: &[usize]| -> Vec<String> {
            shown.iter().filter(|&&i| i < 3).map(|&i| page.book.items[i].name.clone()).collect()
        };
        let (shown, _) = list::matching(&mut page, &marks);
        assert_eq!(first_three(&page, &shown), ["Omnisky III Cannon", "Omnisky VI Cannon", "Omnisky IX Cannon"]);
        // A sort the player chose wins: names descending leave the size aside.
        page.sort = Sort::NameDown;
        let (down, _) = list::matching(&mut page, &marks);
        assert_eq!(first_three(&page, &down), ["Omnisky VI Cannon", "Omnisky IX Cannon", "Omnisky III Cannon"]);
    }

    #[test]
    fn the_list_as_rows_shows_the_same_items() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut page = state("rows", STOCK);
        page.view = View::List;
        let shown = frame(&context(), &mut page, &[], 1300.0);
        for name in ["Frostline", "Arc Shield", "Grand Core"] {
            assert!(has(&shown, name), "{name}: {shown:?}");
        }
    }

    /// An open item with the typed text of its count and one quality field.
    fn open_frostline(name: &str) -> (CraftingState, CraftItem, Vec<Ingredient>) {
        let mut page = state(name, STOCK);
        let index = page.book.items.iter().position(|i| i.name == "Frostline").unwrap();
        page.search = "frost".into();
        page.open = Some(index);
        let item = page.book.items[index].clone();
        let recipe = page.values.clone().unwrap().recipes[item.recipe].clone();
        let ingredients = craft_book::ingredients(&recipe).remove(0);
        (page, item, ingredients)
    }

    /// The open Frostline (a cooler of size 2) against a hangar and the stored slots of the ships.
    fn fits_page(name: &str, hangar_json: &str, ships_json: Option<&str>) -> (CraftingState, CatalogCache) {
        let (mut page, item, _) = open_frostline(name);
        page.notes = match jsonio::loads(hangar_json).unwrap() {
            Value::Object(map) => map,
            _ => panic!("object expected"),
        };
        if let Some(stored) = ships_json {
            let file = page.paths.as_ref().unwrap().app_file(ERKUL_FILE);
            std::fs::write(&file, stored).unwrap();
            page.reload_ships();
        }
        let mut catalog = CatalogCache::default();
        catalog.blueprints.push(versekit_core::catalog_cache::Blueprint {
            key: name_key(&item.basis),
            name: item.name.clone(),
            kind: "Cooler".into(),
            size: Some(2),
            ..Default::default()
        });
        (page, catalog)
    }

    const SHIPS: &str = r#"{"format": 4, "spielversion": "4.1", "hersteller": {"aegisdynamics": "aegs"},
        "schiffe": {"aegiseclipse": {"id": "aegs_eclipse", "name": "Eclipse",
            "plaetze": [{"art": "Cooler", "groesse": 2, "anzahl": 3}]},
          "aegisgladius": {"id": "aegs_gladius", "name": "Gladius",
            "plaetze": [{"art": "Cooler", "groesse": 1, "anzahl": 1}]}}}"#;

    #[test]
    fn an_open_part_says_into_which_of_my_ships_it_goes_and_when_it_cannot_say() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let ctx = context();
        let eclipse = r#"{"schiffe": [{"name": "Eclipse", "hersteller": "Aegis Dynamics"}]}"#;
        let (mut page, catalog) = fits_page("fits-in", eclipse, Some(SHIPS));
        let shown = frame_with(&ctx, &mut page, &[], 900.0, &catalog);
        // Written out: the names of the values must be those of the text, or the braces would stand there.
        assert!(shown.contains(&"Passt in deine Schiffe: Eclipse (3×)".to_string()), "{shown:?}");
        set_language(Choice::En);
        let shown = frame_with(&ctx, &mut page, &[], 900.0, &catalog);
        assert!(shown.contains(&"Fits your ships: Eclipse (3×)".to_string()), "{shown:?}");
        assert!(!shown.iter().any(|s| s.contains('{') && s.contains('}')), "{shown:?}");
        set_language(Choice::De);

        let gladius = r#"{"schiffe": [{"name": "Gladius", "hersteller": "Aegis Dynamics"}]}"#;
        let (mut page, catalog) = fits_page("fits-nowhere", gladius, Some(SHIPS));
        let shown = frame_with(&ctx, &mut page, &[], 900.0, &catalog);
        assert!(shown.contains(&t("s_hg_passt_nirgends")), "{shown:?}");

        // No ship in the hangar is not "fits nowhere": the player is asked to enter the ships.
        let (mut page, catalog) = fits_page("fits-empty", r#"{"schiffe": []}"#, Some(SHIPS));
        let shown = frame_with(&ctx, &mut page, &[], 900.0, &catalog);
        assert!(shown.contains(&t("s_hg_passt_leer")), "{shown:?}");
        assert!(!shown.contains(&t("s_hg_passt_nirgends")), "{shown:?}");

        // No stored slots is not "fits nowhere" either.
        let (mut page, catalog) = fits_page("fits-nodata", eclipse, None);
        let shown = frame_with(&ctx, &mut page, &[], 900.0, &catalog);
        assert!(shown.contains(&t("s_hg_passt_unbekannt")), "{shown:?}");
        assert!(!shown.contains(&t("s_hg_passt_nirgends")), "{shown:?}");

        // A blueprint the catalog does not know gets no answer at all.
        let (mut page, _) = fits_page("fits-silent", eclipse, Some(SHIPS));
        let shown = frame_with(&ctx, &mut page, &[], 900.0, &CatalogCache::default());
        assert!(!shown.iter().any(|s| s.contains("Eclipse")), "{shown:?}");
    }

    #[test]
    fn a_blueprint_without_slot_data_fetches_them_once_and_says_so() {
        use crate::slot_sync::tests::{Counter, WAIT};
        let _language = crate::language_lock();
        set_language(Choice::De);
        let ctx = context();
        let arrow = r#"{"format": 1, "schiffe": [{"name": "Arrow", "hersteller": "Anvil", "hkurz": "ANVL"}]}"#;
        let (mut page, catalog) = fits_page("fits-fetch", arrow, None);
        std::fs::write(page.paths.as_ref().unwrap().app_file(hangar::FILE), arrow).unwrap();
        let counter = Counter::new("4.3");
        page.sync = counter.sync();
        let shown = frame_with(&ctx, &mut page, &[], 900.0, &catalog);
        assert!(shown.contains(&t("s_hg_passt_unbekannt")), "{shown:?}");
        assert!(t("s_hg_passt_unbekannt").contains("gerade geholt"), "{}", t("s_hg_passt_unbekannt"));
        assert!(page.sync.wait_idle(WAIT));
        assert_eq!(counter.catalogs(), 1, "the first frame fetched");
        // Shown again without data, in the same program run: the blueprint does not ask a second time.
        std::fs::remove_file(page.paths.as_ref().unwrap().app_file(ERKUL_FILE)).unwrap();
        page.ships = ErkulShips::default();
        let _ = frame_with(&ctx, &mut page, &[], 900.0, &catalog);
        // A frame may draw the page more than once; every drawing asks.
        assert!(page.sync.asked(crate::slot_sync::Trigger::BlueprintNoData) >= 2);
        assert!(page.sync.wait_idle(WAIT));
        assert_eq!((page.sync.runs(), counter.catalogs()), (1, 1));
        set_language(Choice::En);
        assert!(t("s_hg_passt_unbekannt").contains("being fetched"), "{}", t("s_hg_passt_unbekannt"));
        set_language(Choice::De);
    }

    /// Eclipse: coolers and a shield of size 2, a power plant of size 1. Gladius: a power plant of size 2.
    const FLEET: &str = r#"{"format": 4, "spielversion": "4.1", "hersteller": {"aegisdynamics": "aegs"},
        "schiffe": {"aegiseclipse": {"id": "aegs_eclipse", "name": "Eclipse",
            "plaetze": [{"art": "Cooler", "groesse": 2, "anzahl": 3}, {"art": "Shield", "groesse": 2, "anzahl": 1},
                        {"art": "PowerPlant", "groesse": 1, "anzahl": 1}]},
          "aegisgladius": {"id": "aegs_gladius", "name": "Gladius",
            "plaetze": [{"art": "PowerPlant", "groesse": 2, "anzahl": 1}]}}}"#;

    const ECLIPSE_AND_GLADIUS: &str = r#"{"format": 1, "schiffe": [
        {"name": "Eclipse", "hersteller": "Aegis Dynamics"}, {"name": "Gladius", "hersteller": "Aegis Dynamics"},
        {"name": "Eclipse", "hersteller": "Aegis Dynamics"}]}"#;

    /// The sample recipes against a hangar (written to its file), the stored slots of the ships and a
    /// catalog that knows all three items at size 2, each with a contract that gives it.
    fn ship_page(name: &str, hangar_json: &str, ships_json: Option<&str>) -> (CraftingState, CatalogCache) {
        let mut page = state(name, STOCK);
        std::fs::write(page.paths.as_ref().unwrap().app_file(NOTES_FILE), hangar_json).unwrap();
        page.reload_notes();
        if let Some(stored) = ships_json {
            std::fs::write(page.paths.as_ref().unwrap().app_file(ERKUL_FILE), stored).unwrap();
            page.reload_ships();
        }
        let mut catalog = CatalogCache::default();
        for (item, kind) in [("Frostline", "Cooler"), ("Arc Shield", "Shield"), ("Grand Core", "PowerPlant")] {
            let basis = page.book.items.iter().find(|i| i.name == item).unwrap().basis.clone();
            catalog.blueprints.push(versekit_core::catalog_cache::Blueprint {
                key: name_key(&basis),
                name: item.into(),
                kind: kind.into(),
                size: Some(2),
                ways: vec![versekit_core::catalog_cache::Way { contract: "A job".into(), ..Default::default() }],
                ..Default::default()
            });
        }
        (page, catalog)
    }

    const ITEMS: [&str; 3] = ["Frostline", "Arc Shield", "Grand Core"];

    fn listed(shown: &[String]) -> Vec<&'static str> {
        ITEMS.into_iter().filter(|name| has(shown, name)).collect()
    }

    #[test]
    fn the_ship_filter_shows_the_parts_of_a_ship_with_and_without_the_blueprint() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let ctx = context();
        let (mut page, catalog) = ship_page("ship-filter", ECLIPSE_AND_GLADIUS, Some(FLEET));
        assert_eq!(page.ship_names(), ["Eclipse", "Gladius"], "each ship once, in the order of the hangar");
        let shown = frame_with(&ctx, &mut page, &["Frostline"], 1300.0, &catalog);
        assert_eq!(listed(&shown), ITEMS, "without a ship every item stands");

        // The cooler and the shield go into the Eclipse; the power plant of size 2 does not. The shield stands
        // although its blueprint is missing.
        page.picks.ship = "Eclipse".into();
        let shown = frame_with(&ctx, &mut page, &["Frostline"], 1300.0, &catalog);
        assert_eq!(listed(&shown), ["Frostline", "Arc Shield"], "{shown:?}");
        assert!(has(&shown, "Passt in Eclipse · Bauplan vorhanden: 1 von 2"), "{shown:?}");
        assert!(has(&shown, "Für Eclipse"), "the button names the ship: {shown:?}");
        set_language(Choice::En);
        let shown = frame_with(&ctx, &mut page, &["Frostline"], 1300.0, &catalog);
        assert!(has(&shown, "Fits Eclipse · blueprint owned: 1 of 2"), "{shown:?}");
        assert!(has(&shown, "For Eclipse"), "{shown:?}");
        assert!(!shown.iter().any(|s| s.contains('{') && s.contains('}')), "{shown:?}");
        set_language(Choice::De);

        // A category on top narrows to the parts of the ship in it: the cooler stays under its own category and
        // subcategory, alone or with the ship.
        let frostline = page.book.items.iter().position(|i| i.name == "Frostline").unwrap();
        let place = page.places[frostline];
        (page.picks.category, page.picks.sub) = (Some(place.category), place.sub);
        let shown = frame_with(&ctx, &mut page, &["Frostline"], 1300.0, &catalog);
        assert_eq!(listed(&shown), ["Frostline"], "ship and category: {shown:?}");
        let ship = std::mem::take(&mut page.picks.ship);
        let shown = frame_with(&ctx, &mut page, &["Frostline"], 1300.0, &catalog);
        assert!(listed(&shown).contains(&"Frostline"), "category alone: {shown:?}");
        page.picks.ship = ship;
        (page.picks.category, page.picks.sub) = (None, None);

        // The switch "blueprint owned" still narrows.
        page.picks.state = "habe".into();
        let shown = frame_with(&ctx, &mut page, &["Frostline"], 1300.0, &catalog);
        assert_eq!(listed(&shown), ["Frostline"]);
        page.picks.state.clear();

        // The part without the blueprint opens with the way to it, and says that it fits the Eclipse.
        page.open = page.book.items.iter().position(|i| i.name == "Arc Shield");
        let shown = frame_with(&ctx, &mut page, &["Frostline"], 1300.0, &catalog);
        assert!(has(&shown, &t("s_he_woher_bp")), "{shown:?}");
        assert!(shown.iter().any(|s| s.starts_with("Passt in deine Schiffe: Eclipse")), "{shown:?}");

        // The Gladius takes only the power plant.
        page.open = None;
        page.picks.ship = "Gladius".into();
        let shown = frame_with(&ctx, &mut page, &["Frostline"], 1300.0, &catalog);
        assert_eq!(listed(&shown), ["Grand Core"], "{shown:?}");
        assert!(has(&shown, "Passt in Gladius · Bauplan vorhanden: 0 von 1"), "{shown:?}");

        // Both directions say the same: an item stands for a ship exactly when "fits into your ship" names it.
        let notes = page.notes.clone();
        for ship in hangar::ships(&notes) {
            page.picks.ship = hangar::field_text(ship, "name").to_string();
            page.refresh_ship_fit(&catalog);
            let ShipFit::Parts(parts) = &page.ship_fit.0 else { panic!("the ship has slots") };
            for (index, item) in page.book.items.iter().enumerate() {
                let blueprint = catalog.blueprints.iter().find(|b| b.key == name_key(&item.basis)).unwrap();
                let part = (blueprint.maker.as_deref(), blueprint.name.as_str());
                let answer = erkul_slots::fit_answer(&page.ships, &blueprint.kind, blueprint.size, part, &[ship]);
                assert_eq!(
                    parts.contains_key(&index),
                    matches!(answer, FitAnswer::In(_)),
                    "{} {}",
                    item.name,
                    page.picks.ship
                );
            }
        }

        // "Clear filters" takes the ship away too.
        page.picks.ship = "Eclipse".into();
        assert!(page.is_narrowed());
        page.clear_filters();
        let shown = frame_with(&ctx, &mut page, &[], 1300.0, &catalog);
        assert_eq!(listed(&shown), ITEMS);

        // A ship that leaves the hangar takes the filter with it instead of emptying the list.
        page.picks.ship = "Eclipse".into();
        let gladius = r#"{"format": 1, "schiffe": [{"name": "Gladius", "hersteller": "Aegis Dynamics"}]}"#;
        std::fs::write(page.paths.as_ref().unwrap().app_file(NOTES_FILE), gladius).unwrap();
        page.reload_notes();
        let shown = frame_with(&ctx, &mut page, &[], 1300.0, &catalog);
        assert_eq!(listed(&shown), ITEMS, "{shown:?}");
        assert!(page.picks.ship.is_empty());
    }

    #[test]
    fn a_ship_without_slot_data_says_so_and_fetches_them_instead_of_saying_nothing_fits() {
        use crate::slot_sync::tests::{Counter, WAIT};
        let _language = crate::language_lock();
        set_language(Choice::De);
        let ctx = context();
        let (mut page, catalog) = ship_page("ship-nodata", ECLIPSE_AND_GLADIUS, None);
        let counter = Counter::new("4.3");
        page.sync = counter.sync();
        page.picks.ship = "Eclipse".into();
        let shown = frame_with(&ctx, &mut page, &[], 1300.0, &catalog);
        assert!(has(&shown, "Zu Eclipse fehlen noch die Steckplatz-Daten. Sie werden gerade geholt."), "{shown:?}");
        assert!(listed(&shown).is_empty(), "{shown:?}");
        assert!(!has(&shown, &t("s_he_nichts")), "not \"nothing found\": {shown:?}");
        assert!(page.sync.asked(crate::slot_sync::Trigger::BlueprintNoData) >= 1);
        assert!(page.sync.wait_idle(WAIT));
        set_language(Choice::En);
        let shown = frame_with(&ctx, &mut page, &[], 1300.0, &catalog);
        assert!(has(&shown, "The slot data for Eclipse is still missing. It is being fetched now."), "{shown:?}");
        set_language(Choice::De);
        assert!(page.sync.wait_idle(WAIT));
    }

    #[test]
    fn the_ship_is_chosen_in_the_menu_and_an_empty_hangar_says_what_to_do() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut bench = Bench::new("ship-menu", STOCK, 1300.0);
        let (page, catalog) = ship_page("ship-menu-page", ECLIPSE_AND_GLADIUS, Some(FLEET));
        (bench.page, bench.catalog) = (page, catalog);
        let shown = bench.click_button("Für Schiff");
        assert!(has(&shown, "Alle Schiffe") && has(&shown, "Gladius"), "the menu is open: {shown:?}");
        let shown = bench.click_text("Gladius");
        assert_eq!(bench.page.picks.ship, "Gladius");
        assert_eq!(listed(&shown), ["Grand Core"], "{shown:?}");
        // "All ships" takes the filter away again.
        bench.click_button("Für Gladius");
        let shown = bench.click_text("Alle Schiffe");
        assert!(bench.page.picks.ship.is_empty());
        assert_eq!(listed(&shown), ITEMS);

        let (page, catalog) = ship_page("ship-menu-empty", r#"{"format": 1, "schiffe": []}"#, Some(FLEET));
        (bench.page, bench.catalog) = (page, catalog);
        let shown = bench.click_button("Für Schiff");
        assert!(has(&shown, &t("s_he_schiff_leer")), "{shown:?}");
        assert!(t("s_he_schiff_leer").contains("Mein Hangar"));
    }

    #[test]
    fn a_jump_to_a_ship_sets_the_filter_and_nothing_else() {
        let (mut page, _) = ship_page("ship-jump", ECLIPSE_AND_GLADIUS, Some(FLEET));
        page.search = "frost".into();
        page.picks.state = "habe".into();
        page.jump_to("Frostline");
        page.jump_to_ship("Eclipse");
        page.enter();
        assert_eq!(page.picks, Picks { ship: "Eclipse".into(), ..Picks::default() });
        assert!(page.search.is_empty() && page.open.is_none() && page.jump.is_none());
        // Entered again without a jump, the page starts clean as before.
        page.enter();
        assert_eq!(page.picks, Picks::default());
    }

    #[test]
    fn a_count_that_is_no_count_is_said_and_blocks_crafting() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let ctx = context();
        for (typed, fine) in [
            ("1", true),
            ("9999", true),
            ("", true),
            ("0", false),
            ("10000", false),
            ("99999999999", false),
            ("inf", false),
            ("nan", false),
            ("abc", false),
        ] {
            let (mut page, _, _) = open_frostline("count");
            frame(&ctx, &mut page, &[], 900.0);
            page.detail.as_mut().unwrap().count = typed.into();
            let shown = frame(&ctx, &mut page, &[], 900.0);
            assert_eq!(!shown.contains(&t("s_p9_anzahl_falsch")), fine, "{typed:?}: {shown:?}");
            assert_eq!(count_valid(typed), fine, "{typed:?}");
        }
        // Whatever stands there, the count used for the sum stays within bounds.
        assert_eq!(count_of("99999999999"), 1);
        assert_eq!(count_of("inf"), 1);
        assert_eq!(count_of("9999"), 9999);
    }

    #[test]
    fn a_quality_that_is_no_quality_is_said_and_blocks_crafting() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let ctx = context();
        let (mut page, _, _) = open_frostline("badq");
        frame(&ctx, &mut page, &[], 900.0);
        let shown = frame(&ctx, &mut page, &[], 900.0);
        assert!(!shown.contains(&t("s_p9_qualitaet_falsch")), "{shown:?}");
        assert!(page.detail.as_ref().unwrap().bad.is_empty());
        page.detail.as_mut().unwrap().typed.insert("Iron".into(), "1001".into());
        let shown = frame(&ctx, &mut page, &[], 900.0);
        assert!(shown.contains(&t("s_p9_qualitaet_falsch")), "{shown:?}");
        assert!(page.detail.as_ref().unwrap().bad.contains("Iron"));
        // Typed right again, the hint goes and the button works.
        page.detail.as_mut().unwrap().typed.insert("Iron".into(), "640".into());
        let shown = frame(&ctx, &mut page, &[], 900.0);
        assert!(!shown.contains(&t("s_p9_qualitaet_falsch")), "{shown:?}");
        assert!(page.detail.as_ref().unwrap().bad.is_empty());
    }

    #[test]
    fn crafting_reads_the_stock_fresh_and_keeps_what_another_page_wrote() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let (mut page, item, ingredients) = open_frostline("fresh");
        let file = page.stock_file().unwrap();
        // Enough for one piece: Iron 0.5, Gold 0.2 (one crate each, whole).
        let rows = entries(RICH);
        assert!(stock::save(&file, &rows, "test"));
        page.reload_stock();
        let index = page.open.unwrap();
        ensure_detail(&mut page, index, &ingredients);
        // Another page adds a crate after the page has read the file.
        let mut more = rows.clone();
        more.extend(entries(r#"[{"material":"Copper","menge":4.0,"qualitaet":500,"ort":"Levski"}]"#));
        assert!(stock::save(&file, &more, "test"));
        craft(&mut page, &item, &ingredients);
        assert_eq!(page.detail.as_ref().unwrap().feedback, Some(Feedback::Deducted(1)));
        let after = stock::load(&file, "test");
        assert!(
            after.iter().any(|e| e.get("material").and_then(Value::as_str) == Some("Copper")),
            "the crate another page wrote is gone: {after:?}"
        );
    }

    /// The sync reads the stock under the lock, waits and writes it back with a crate more: crafting meanwhile
    /// waits for the lock, so neither the crate nor the deduction is lost.
    #[test]
    fn crafting_waits_while_the_sync_holds_the_stock_and_keeps_what_it_wrote() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let (mut page, item, ingredients) = open_frostline("lock");
        let file = page.stock_file().unwrap();
        assert!(stock::save(&file, &entries(RICH), "test"));
        page.reload_stock();
        let index = page.open.unwrap();
        ensure_detail(&mut page, index, &ingredients);
        let (held, holding) = std::sync::mpsc::channel();
        let sync = {
            let file = file.clone();
            std::thread::spawn(move || {
                let _lock = stock::lock();
                let mut rows = stock::load(&file, "test");
                held.send(()).expect("the test waits");
                std::thread::sleep(std::time::Duration::from_millis(400));
                rows.extend(entries(r#"[{"material":"Copper","menge":4.0,"qualitaet":500,"ort":"Levski"}]"#));
                assert!(stock::save(&file, &rows, "test"));
            })
        };
        holding.recv().expect("the sync holds the lock");
        craft(&mut page, &item, &ingredients);
        sync.join().expect("the sync ends");
        assert_eq!(page.detail.as_ref().unwrap().feedback, Some(Feedback::Deducted(1)));
        let after = stock::load(&file, "test");
        let amount = |name: &str| {
            after
                .iter()
                .filter(|e| e.get("material").and_then(Value::as_str) == Some(name))
                .map(|e| stock::number_of(e, "menge"))
                .sum::<f64>()
        };
        assert_eq!(amount("Copper"), 4.0, "the crate of the sync is kept: {after:?}");
        assert!((amount("Iron") - 1.5).abs() < 1e-9, "the deduction is kept: {after:?}");
    }

    #[test]
    fn crafting_lowers_the_notes_and_the_item_stays_while_pieces_are_left() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let (mut page, item, ingredients) = open_frostline("notes");
        let file = page.stock_file().unwrap();
        let rows = entries(
            r#"[{"material":"Iron","menge":9.0,"qualitaet":700,"ort":"Levski"},
                {"material":"Gold","menge":9.0,"qualitaet":700,"ort":"Levski"}]"#,
        );
        assert!(stock::save(&file, &rows, "test"));
        page.reload_stock();
        let index = page.open.unwrap();
        ensure_detail(&mut page, index, &ingredients);
        page.detail.as_mut().unwrap().count = "3".into();
        add_note(&mut page, &item);
        assert!(notepad::contains(&page.notes, "Frostline"));
        assert_eq!(notepad::count_of_item(&page.notes, "Frostline"), 3);
        page.detail.as_mut().unwrap().count = "1".into();
        craft(&mut page, &item, &ingredients);
        assert_eq!(page.detail.as_ref().unwrap().feedback, Some(Feedback::Deducted(1)));
        assert_eq!(notepad::count_of_item(&page.notes, "Frostline"), 2, "one piece off the notes");
    }

    #[test]
    fn a_jump_waits_for_the_recipes_and_chooses_the_item_when_they_are_there() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut page = CraftingState::empty(None);
        page.jump_to("Frostline");
        page.enter();
        assert!(page.open.is_none() && page.jump.is_some(), "the jump must not be lost while nothing is read");
        page.take_values(Arc::new(sample_values()), "archive");
        // The window draws a frame; it does not call `enter` a second time.
        frame(&context(), &mut page, &[], 1300.0);
        assert!(page.jump.is_none());
        let open = page.open.expect("the item is chosen");
        assert_eq!(page.book.items[open].name, "Frostline");
        assert_eq!(page.search, "Frostline");
        assert!(page.scroll_to.is_none(), "the list scrolled to it once");
    }

    #[test]
    fn new_values_keep_the_chosen_item() {
        let mut page = CraftingState::default();
        page.open = page.book.items.iter().position(|i| i.name == "Grand Core");
        let before = page.book.items[page.open.unwrap()].name.clone();
        page.take_values(Arc::new(sample_values()), "archive");
        assert_eq!(page.open.map(|i| page.book.items[i].name.clone()), Some(before));
    }

    #[test]
    fn crafting_takes_the_ingredients_out_of_the_stock_file_and_lowers_the_notes() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut page = state("craft", RICH);
        let index = page.book.items.iter().position(|i| i.name == "Frostline").unwrap();
        page.open = Some(index);
        let item = page.book.items[index].clone();
        let recipe = page.values.clone().unwrap().recipes[item.recipe].clone();
        let ingredients = craft_book::ingredients(&recipe).remove(0);
        ensure_detail(&mut page, index, &ingredients);
        page.detail.as_mut().unwrap().count = "2".into();
        add_note(&mut page, &item);
        craft(&mut page, &item, &ingredients);
        assert_eq!(page.detail.as_ref().unwrap().feedback, Some(Feedback::Deducted(2)));
        let after = stock::load(&page.stock_file().unwrap(), "test");
        let amount = |name: &str| {
            after
                .iter()
                .filter(|e| e.get("material").and_then(Value::as_str) == Some(name))
                .map(|e| stock::number_of(e, "menge"))
                .sum::<f64>()
        };
        assert!((amount("Iron") - 1.0).abs() < 1e-9, "{after:?}");
        assert!((amount("Gold") - 0.6).abs() < 1e-9, "{after:?}");
        // The note was set with the count 2, crafting 2 strikes it.
        assert!(!notepad::contains(&page.notes, "Frostline"), "{:?}", notepad::notes(&page.notes));
        assert_eq!(page.detail.as_ref().unwrap().count, "1");
    }

    #[test]
    fn nothing_is_taken_when_anything_is_missing() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut page = state("short", r#"[{"material":"Iron","menge":2.0,"qualitaet":700,"ort":"Levski"}]"#);
        let index = page.book.items.iter().position(|i| i.name == "Frostline").unwrap();
        page.open = Some(index);
        let item = page.book.items[index].clone();
        let recipe = page.values.clone().unwrap().recipes[item.recipe].clone();
        let ingredients = craft_book::ingredients(&recipe).remove(0);
        ensure_detail(&mut page, index, &ingredients);
        let before = std::fs::read(page.stock_file().unwrap()).unwrap();
        craft(&mut page, &item, &ingredients);
        match page.detail.as_ref().unwrap().feedback.clone() {
            Some(Feedback::Missing(missing)) => assert_eq!(missing, vec![("Gold".to_string(), 0.2)]),
            other => panic!("expected a miss, got {other:?}"),
        }
        assert_eq!(std::fs::read(page.stock_file().unwrap()).unwrap(), before, "the file stays as it was");
    }

    #[test]
    fn the_quality_follows_the_stock_until_it_is_set_by_hand() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut page = state("quality", STOCK);
        let index = page.book.items.iter().position(|i| i.name == "Frostline").unwrap();
        let recipe = page.values.clone().unwrap().recipes[page.book.items[index].recipe].clone();
        let ingredients = craft_book::ingredients(&recipe).remove(0);
        ensure_detail(&mut page, index, &ingredients);
        let detail = page.detail.as_ref().unwrap();
        assert_eq!(detail.stand["Iron"], 700.0, "from the stock");
        // Half the gold lies in the stock, the other half counts at 500: a mix of both.
        assert_eq!(detail.stand["Gold"], 600.0);
        assert!(detail.auto.contains("Iron") && detail.auto.contains("Gold"));
        assert_eq!(
            detail.mix["Gold"].iter().map(|p| (p.amount, p.quality, p.stocked)).collect::<Vec<_>>(),
            [(0.1, 700.0, true), (0.1, 500.0, false)]
        );
        // A material that is not in the stock starts at 500.
        let mut empty = state("quality-empty", "[]");
        ensure_detail(&mut empty, index, &ingredients);
        assert_eq!(empty.detail.as_ref().unwrap().stand["Iron"], 500.0);
        assert!(!empty.detail.as_ref().unwrap().auto.contains("Iron"));
    }

    /// A recipe that takes iron of quality 800 at the least, like the Enhanced Ore Pod of MISC:
    /// the quality stands at 800 at the lowest, the recipe says so, and buying at 500 is no way.
    #[test]
    fn the_lowest_quality_of_the_recipe_binds_the_quality_and_rules_out_buying() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut values = sample_values();
        values.recipes[0].tiers[0].slots[0].options[0].min_quality = Some(800);
        let mut bench = Bench::new("floor", STOCK, 1300.0);
        bench.page.take_values(Arc::new(values), "archive");
        // Both can be bought at a terminal; only the gold, which takes any quality, is offered.
        let file = bench.page.paths.as_ref().unwrap().app_file(versekit_core::stock_fetch::PRICES_FILE);
        let prices = r#"{"format":1,"waren":{"iron":[{"name":"Iron","kauf":2643,"verkauf":2000}],
                                              "gold":[{"name":"Gold","kauf":5000,"verkauf":4000}]}}"#;
        std::fs::write(&file, prices).unwrap();
        bench.page.prices = RawPrices::load(bench.page.paths.as_ref().unwrap());
        assert!(bench.page.prices.price("Iron").is_some_and(|p| p.buy > 0.0), "the prices are read");
        bench.click_text("Frostline");
        let stand = |b: &Bench| b.page.detail.as_ref().unwrap().stand["Iron"];
        assert_eq!(stand(&bench), 800.0, "the stock holds iron at 700 only, which the recipe does not take");
        // A value below the lowest, set before, is lifted when the page is drawn.
        let detail = bench.page.detail.as_mut().unwrap();
        detail.stand.insert("Iron".into(), 431.0);
        detail.typed.insert("Iron".into(), "431".into());
        detail.auto.remove("Iron");
        let shown = bench.frame();
        assert_eq!(stand(&bench), 800.0, "{shown:?}");
        assert_eq!(bench.page.detail.as_ref().unwrap().typed["Iron"], "800");
        let lowest = tn("s_he_mindestens", &[Arg::from("800")]);
        assert_eq!(shown.iter().filter(|s| **s == lowest).count(), 1, "{lowest}: {shown:?}");
        // The iron cannot be bought at 500, the gold can.
        assert!(has(&shown, &t("s_he_kauf_zu_schlecht")), "{shown:?}");
        let offers = shown.iter().filter(|s| s.starts_with("kaufen: ")).count();
        assert_eq!(offers, 1, "only the gold is offered: {shown:?}");
    }

    #[test]
    fn the_filters_narrow_the_list() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut page = state("filters", STOCK);
        let ctx = context();
        page.picks.state = "habe".into();
        let held = frame(&ctx, &mut page, &["Frostline"], 1300.0);
        assert!(held.iter().any(|s| s == "Frostline") && !held.iter().any(|s| s == "Arc Shield"), "{held:?}");
        page.picks.state = "fehlt".into();
        let missing = frame(&ctx, &mut page, &["Frostline"], 1300.0);
        assert!(missing.iter().any(|s| s == "Arc Shield") && !missing.iter().any(|s| s == "Frostline"), "{missing:?}");
        page.picks = Picks { material: "reicht".into(), ..Picks::default() };
        // Nothing is covered: there is too little gold.
        assert!(frame(&ctx, &mut page, &[], 1300.0).contains(&t("s_he_nichts")));
        assert!(page.is_narrowed());
        page.reset();
        assert!(!page.is_narrowed());
    }

    /// The measure of the list sees what is wider than the list, which the edge check of the
    /// page cannot (it cuts the content): a label that is too wide must show.
    #[test]
    fn the_list_measure_sees_content_wider_than_the_list() {
        let ctx = context();
        let mut seen = 0.0;
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(300.0, 400.0))),
            ..Default::default()
        };
        let mut done = ctx.run_ui(input, |ui| {
            let output = egui::ScrollArea::vertical().show(ui, |inner| {
                inner.add(egui::Label::new("x".repeat(200)).extend());
            });
            seen = (output.content_size.x - output.inner_rect.width()).max(0.0);
        });
        done.textures_delta.clear();
        assert!(seen > 50.0, "{seen}");
    }

    #[test]
    fn nothing_sticks_out_at_any_width_size_and_language() {
        let _language = crate::language_lock();
        let mut findings = Vec::new();
        for lang in [Choice::De, Choice::En] {
            set_language(lang);
            for points in versekit_core::font_size::POINTS_RANGE.0..=versekit_core::font_size::POINTS_RANGE.1 {
                for width in [380.0, 520.0, 900.0, 1300.0] {
                    for chosen in [false, true] {
                        for view in [View::Grid, View::List] {
                            let ctx = context();
                            crate::apply_size(&ctx, points);
                            let mut page = state("fit", STOCK);
                            page.view = view;
                            let index = page.book.items.iter().position(|i| i.name == "Frostline").unwrap();
                            // The hardest case: very long names, a count and a quality that are wrong.
                            page.book.items[index].name = "Frostline Ultra Heavy Duty Cooling Array Mk IV".repeat(2);
                            page.book.items[index].manufacturer = "Aegis Dynamics Interplanetary Works".into();
                            page.open = chosen.then_some(index);
                            let mut over = 0.0;
                            for round in 0..4 {
                                if round == 2
                                    && let Some(detail) = page.detail.as_mut()
                                {
                                    detail.count = "abc".into();
                                    detail.typed.insert("Iron".into(), "1001".into());
                                    detail.feedback = Some(Feedback::Missing(vec![("Iron".into(), 1234.5678)]));
                                }
                                let have: HashSet<String> = HashSet::new();
                                let catalog = CatalogCache::default();
                                let input = egui::RawInput {
                                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 2000.0))),
                                    ..Default::default()
                                };
                                // The page runs inside the page frame of the window, which keeps the safety margin.
                                let scheme = versekit_core::theme::current();
                                let mut output = ctx.run_ui(input, |ui| {
                                    let shown = crate::layout::page(ui, scheme, "herstellung-test", |ui| {
                                        show_page(ui, scheme, &mut page, &Shared { have: &have, catalog: &catalog })
                                    });
                                    over = shown.overflow.max(page.list_overflow);
                                });
                                output.textures_delta.clear();
                            }
                            if over > 0.5 {
                                findings.push(format!(
                                    "{lang:?} Größe {points} Breite {width} gewählt {chosen} {view:?}: {over:.0} zu breit"
                                ));
                            }
                            if chosen {
                                assert!(
                                    page.detail.as_ref().is_some_and(|d| !d.bad.is_empty() && !count_valid(&d.count)),
                                    "the wrong input was in the frames"
                                );
                            }
                        }
                    }
                }
            }
        }
        set_language(Choice::De);
        assert!(findings.is_empty(), "{}", findings.join("\n"));
    }

    /// Every text of a frame with the place it really covers: a right-aligned text has its position at its right
    /// edge, so the rectangle comes from the galley and not from the size laid on the position.
    fn covered_texts(shapes: &[egui::epaint::ClippedShape]) -> Vec<(String, Rect)> {
        fn collect(shape: &egui::epaint::Shape, out: &mut Vec<(String, Rect)>) {
            match shape {
                egui::epaint::Shape::Text(text) => {
                    out.push((text.galley.text().to_string(), text.galley.rect.translate(text.pos.to_vec2())))
                }
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut out));
        out
    }

    /// The open item with every line it can have: maker, class, size and grade, the cheapest shop with a long
    /// place, and the ships it fits into, many and with long names.
    fn full_page(name: &str) -> (CraftingState, CatalogCache) {
        let names =
            ["Gladius Valiant", "Hornet Tracker", "Cutlass Black Steel", "Avenger Titan Renegade", "Sabre Raven"];
        let slim = |n: &str| n.to_lowercase().replace(' ', "");
        let hangar = format!(
            r#"{{"schiffe": [{}]}}"#,
            names
                .iter()
                .map(|n| format!(r#"{{"name": "{n}", "hersteller": "Aegis Dynamics"}}"#))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let ships = format!(
            r#"{{"format": 4, "spielversion": "4.1", "hersteller": {{"aegisdynamics": "aegs"}}, "schiffe": {{{}}}}}"#,
            names
                .iter()
                .map(|n| format!(
                    r#""aegis{0}": {{"id": "aegs_{0}", "name": "{n}", "plaetze": [{{"art": "Cooler", "groesse": 1, "anzahl": 2}}]}}"#,
                    slim(n)
                ))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let (mut page, mut catalog) = fits_page(name, &hangar, Some(&ships));
        let index = page.open.expect("open");
        page.book.items[index].manufacturer = "Lightning Power Ltd.".into();
        page.book.items[index].entity = "abc".into();
        let paths = page.paths.as_ref().expect("paths");
        std::fs::write(
            paths.app_file(versekit_core::shops::CACHE),
            r#"{"format": 1, "geholt": 1.0, "teile": {"abc": {"geholt": 1.0, "zeilen": [
                {"laden": "Platinum Bay", "ort": "ARC-L1 Wide Forest Station", "system": "Stanton", "preis": 6884.0, "zustand": 100}]}}}"#,
        )
        .unwrap();
        page.shop = crate::crafting_shop::ShopLine::new(page.paths.as_ref());
        let blueprint = &mut catalog.blueprints[0];
        (blueprint.size, blueprint.grade, blueprint.class) = (Some(1), Some(3), Some("Civilian".into()));
        (page, catalog)
    }

    /// In every size and both languages, from the smallest window up: nothing of the chosen item, its lines and
    /// its cards stands wider than the window, and what runs over is told by the measure of the side too.
    #[test]
    fn the_chosen_item_with_every_line_is_cut_off_nowhere_even_in_the_smallest_window() {
        let _language = crate::language_lock();
        let mut findings = Vec::new();
        for lang in [Choice::De, Choice::En] {
            set_language(lang);
            for points in versekit_core::font_size::POINTS_RANGE.0..=versekit_core::font_size::POINTS_RANGE.1 {
                let smallest = versekit_core::font_size::min_window(points).0;
                for width in [smallest * 0.5, smallest * 0.65, smallest * 0.8, smallest, smallest * 1.3] {
                    let ctx = context();
                    crate::apply_size(&ctx, points);
                    let (mut page, catalog) = full_page(&format!("voll-{points}-{width}"));
                    let have: HashSet<String> = HashSet::new();
                    let scheme = versekit_core::theme::current();
                    let (mut over, mut wide, mut drawn) = (0.0_f32, Vec::new(), Vec::new());
                    for _ in 0..4 {
                        let input = egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 2000.0))),
                            ..Default::default()
                        };
                        let mut output = ctx.run_ui(input, |ui| {
                            let shown = crate::layout::page(ui, scheme, "herstellung-voll", |ui| {
                                show_page(ui, scheme, &mut page, &Shared { have: &have, catalog: &catalog })
                            });
                            over = shown.overflow.max(page.list_overflow);
                        });
                        output.textures_delta.clear();
                        let placed = covered_texts(&output.shapes);
                        wide = placed
                            .iter()
                            .filter(|(_, rect)| rect.max.x > width + 0.5)
                            .map(|(text, rect)| format!("{text:?} bis {:.0}", rect.max.x))
                            .collect();
                        drawn = placed.into_iter().map(|(text, _)| text).collect();
                    }
                    let which = format!("{lang:?} Größe {points} Breite {width:.0}");
                    if over > 0.5 {
                        findings.push(format!("{which}: {over:.0} zu breit"));
                    }
                    if !wide.is_empty() {
                        findings.push(format!("{which}: Text über den Rand: {wide:?}"));
                    }
                    // The lines this check is about were drawn at all.
                    for line in ["Platinum Bay", "Gladius Valiant", "Lightning Power Ltd."] {
                        if !drawn.iter().any(|text| text.contains(line)) {
                            findings.push(format!("{which}: die Zeile mit {line} fehlt"));
                        }
                    }
                }
            }
        }
        set_language(Choice::De);
        assert!(findings.is_empty(), "{} findings:\n{}", findings.len(), findings[..findings.len().min(25)].join("\n"));
    }
}
