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

//! The stock page (`my_stock_page.py`): materials from `rohstoffe.json` and
//! goods and items from `handelslager.json` in one list, with tabs, filters,
//! search, sortable columns and groups that fold open; above it the forms
//! to add or change an entry and the refinery yield, below it clearing.
//!
//! Every change reads both files first, changes one entry and writes the
//! file; a failed write reads the file again. Without own data the page
//! shows a sample and writes nothing.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::SystemTime;

use egui::accesskit::Role;
use egui::{Align, Key, Layout, Rect, RichText, Sense, Ui, UiBuilder, WidgetInfo, vec2};
use versekit_core::basetool::stock_split::Kind as ShareKind;
use versekit_core::formats;
use versekit_core::jsonio::{self, Value};
use versekit_core::language::{self, Arg, Lang, t, tf};
use versekit_core::mining_data::MiningData;
use versekit_core::paths::Paths;
use versekit_core::stock::{self, Cost, Refused, Row, Store};
use versekit_core::stock_data::{ITEM_KINDS, Kind, MATERIAL_KINDS, StockData, lower, truthy};
use versekit_core::theme::{Color, IconTint, Scheme};

use crate::color;
use crate::layout::GAP;
use crate::widgets::{self, ButtonKind};

mod relocation;
mod scan;
mod suggestions;

/// How often the page looks whether one of its files changed.
const LOOK_SECONDS: f64 = 1.0;
/// A second delete this soon after the first is taken for a double click.
const DELETE_PAUSE: f64 = 0.5;
/// The narrowest the name column may get before rows break into lines.
const NAME_MIN: f32 = 150.0;
/// Room for the two row actions.
const ACTIONS_WIDTH: f32 = 2.0 * widgets::ICON_BUTTON_SIZE + 12.0;
/// The same with the buttons for sharing into a unit and taking back.
const ACTIONS_WIDTH_SPLIT: f32 = 4.0 * widgets::ICON_BUTTON_SIZE + 12.0;
/// Inner margin plus border of `widgets::list_frame`.
const FRAME_INSET: f32 = 5.0;
/// Where the list keeps (room left on screen, height asked for, visible page,
/// height drawn, row height) of the last frame.
const LIST_ROOM_ID: &str = "lager-liste-platz";
/// Refinery lines shown in the preview, and rejected or hinted ones.
const PREVIEW_ENTRIES: usize = 8;
const PREVIEW_PROBLEMS: usize = 4;

const SETTING_VIEW: &str = "meinlager_sicht";
const SETTING_FORM_OPEN: &str = "meinlager_eintragen_offen";
const SETTING_FORM_KIND: &str = "meinlager_eintragen_zweck";
const SETTING_REFINERY_OPEN: &str = "lager_raffinerie_offen";
/// A panel of the rare actions is open, and which one (`sichern`, `leeren`).
const SETTING_PANEL_OPEN: &str = "meinlager_sichern_offen";
const SETTING_PANEL: &str = "meinlager_sichern_teil";
const SETTING_RAW_PLACE: &str = "lager_ort";
/// The place of the last refinery yield that was booked.
const SETTING_REFINERY_PLACE: &str = "lager_raffinerie_ort";
const SETTING_UNIT: &str = "lager_einheit";
const SETTING_TRADE_PLACE: &str = "handel_ort";
const SETTING_SUGGESTIONS_OPEN: &str = "lager_vorschlaege_offen";
const SETTING_MOVED_SEEN: &str = "lager_umzug_gesehen";

const SAMPLE_RAW: &str = r#"{"format": 1, "posten": [
 {"material": "Agricium", "menge": 2.998, "qualitaet": 588, "ort": "Levski"},
 {"material": "Agricium", "menge": 0.079, "qualitaet": 346, "ort": "Levski"},
 {"material": "Taranite", "menge": 1.25, "qualitaet": 712, "ort": "Area18"},
 {"material": "Hadanite", "menge": 14.0, "qualitaet": 500, "ort": "Levski"},
 {"material": "Iron", "menge": 4.5, "qualitaet": 420, "ort": "Orison", "gestohlen": true}]}"#;
const SAMPLE_TRADE: &str = r#"{"format": 1, "posten": [
 {"ware": "Laranite", "menge": 32.0, "ort": "Seraphim Station", "gestohlen": false, "einkauf": 2650.0},
 {"ware": "Construction Materials", "menge": 72.0, "ort": "Seraphim Station", "gestohlen": false},
 {"ware": "FR-66", "menge": 1.0, "ort": "Levski", "gestohlen": false, "stueck": true},
 {"ware": "TS-2", "menge": 2.0, "ort": "Levski", "gestohlen": false, "stueck": true}]}"#;

/// Which form is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Form {
    Raw,
    Trade,
    Item,
}

impl Form {
    fn code(self) -> &'static str {
        match self {
            Form::Raw => "raw",
            Form::Trade => "trade",
            Form::Item => "item",
        }
    }
}

/// Columns the list sorts by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Column {
    Name,
    Place,
    Quality,
    Amount,
}

/// Columns besides the name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cell {
    Place,
    Quality,
    Amount,
    /// The price paid for one piece of an item.
    Paid,
    Price,
    Total,
    Profit,
}

/// What the clear buttons empty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Raw,
    Goods,
    Items,
}

/// One message per field, shown at the field in red.
#[derive(Clone, Debug, Default, PartialEq)]
struct Errors {
    name: Option<String>,
    amount: Option<String>,
    quality: Option<String>,
    cost: Option<String>,
    place: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct RawForm {
    material: String,
    amount: String,
    quality: String,
    place: String,
    cscu: bool,
    errors: Errors,
}

#[derive(Clone, Debug, Default)]
struct TradeForm {
    goods: String,
    amount: String,
    cost: String,
    place: String,
    stolen: bool,
    errors: Errors,
}

#[derive(Clone, Debug, Default)]
struct ItemForm {
    name: String,
    amount: String,
    place: String,
    stolen: bool,
    errors: Errors,
}

/// The entry in the form: where it lies, what it held when opened and which
/// form shows it.
#[derive(Clone, Debug)]
struct Editing {
    store: Store,
    index: usize,
    entry: Value,
    form: Form,
}

/// A deleted entry that the undo button puts back at its place.
#[derive(Clone, Debug)]
struct Deleted {
    store: Store,
    index: usize,
    entry: Value,
    name: String,
}

/// What the undo button in the message takes back.
#[derive(Clone, Debug)]
enum Undo {
    Deleted(Deleted),
    /// A read-in backup: the whole store as it was before.
    Replaced(Store, Vec<Value>),
    /// Suggestions of the log taken over or discarded.
    Suggestions(suggestions::SuggestionUndo),
    /// Both stores moved to one home place: back from the copies made before.
    Relocated,
}

/// A message at the top right.
#[derive(Clone, Debug)]
struct Message {
    text: String,
    since: Option<f64>,
    undo: Option<Undo>,
}

/// A group of rows: tab, name without case, piece goods among materials.
type GroupKey = (bool, String, bool);

/// One line of the list.
enum Line {
    Group(GroupKey, Vec<Row>, bool),
    Post(Row, bool),
}

/// What a click in the list asks for.
enum Act {
    Edit(Store, usize),
    Delete(Store, usize),
    Share(Store, usize, ShareKind),
    Toggle(GroupKey, bool),
}

/// Today's local date as the file names of 4.12 have it (`%Y-%m-%d`).
fn today() -> String {
    let seconds =
        SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
    formats::local_time(seconds).map(|t| formats::strftime("%Y-%m-%d", &t)).unwrap_or_default()
}

/// The panels of the rare actions right of the tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Panel {
    Backup,
    Move,
    Clear,
}

/// What an open file dialog is for: writing a store out (as CSV or not) or
/// reading a backup in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Job {
    Export(Store, bool),
    Import,
}

/// What the stock page holds.
pub struct StockState {
    files: Option<(String, String)>,
    raw: Vec<Value>,
    trade: Vec<Value>,
    data: StockData,
    /// The tag `(Mil/3/A)` of the items of the blueprint catalog, by name key.
    tags: HashMap<String, String>,
    /// The mining data: how a raw material is mined.
    mining: MiningData,
    seen: [Option<SystemTime>; 2],
    looked_at: f64,
    items: bool,
    kind: Option<Kind>,
    place: String,
    purpose: usize,
    /// The list shows only the rows the Basetool does not get.
    only_gaps: bool,
    pub search: String,
    sort: Column,
    descending: bool,
    open: HashSet<GroupKey>,
    closed: HashSet<GroupKey>,
    closed_for: String,
    form_open: bool,
    form: Form,
    raw_form: RawForm,
    trade_form: TradeForm,
    item_form: ItemForm,
    /// The places remembered after the last booking (`lager_ort`, `handel_ort`).
    raw_place: String,
    trade_place: String,
    editing: Option<Editing>,
    to_form: bool,
    refinery_open: bool,
    refinery_text: String,
    refinery_cscu: bool,
    refinery_place: String,
    refinery_error: Option<String>,
    /// The place of the last refinery yield booked (`lager_raffinerie_ort`); the field starts on it.
    refinery_last: String,
    /// The booking button was pressed with the place empty: the field got the focus and the next press books anyway.
    refinery_asked: bool,
    /// The place field of the refinery block takes the focus and scrolls into view in the next frame.
    refinery_focus: bool,
    /// The widget id of that field, as drawn last.
    refinery_field: Option<egui::Id>,
    /// The panel of a rare action open under the tab row.
    panel: Option<Panel>,
    confirm: Option<Part>,
    message: Option<Message>,
    dialog: Option<(crate::file_dialog::Pending, Job)>,
    /// Actions of the usage counters done since the window last asked (`page_usage.action`).
    counted: Vec<&'static str>,
    /// Personal and shared parts of the rows, with a connection.
    pub(crate) split: crate::stock_share::Split,
    /// The purchases and sales of the log offered on top of the tab Items.
    sugg: suggestions::Suggestions,
    /// The refinery terminal read from the screen into the field of the refinery yield.
    scan: scan::Scan,
    /// The card that says where the material storage and the cargo hold went: shown once to whoever has entries,
    /// gone for good with its cross (`lager_umzug_gesehen`).
    moved_note: bool,
    moved_seen: bool,
    /// Nobody to tell where the stores went: noted as seen once the page is shown.
    moved_unneeded: bool,
    /// The panel that moves both stores to one home place.
    reloc: relocation::Relocation,
}

fn entries_of(text: &str) -> Vec<Value> {
    match jsonio::loads(text) {
        Ok(data) => match data.get("posten") {
            Some(Value::Array(items)) => items.clone(),
            _ => Vec::new(),
        },
        Err(_) => Vec::new(),
    }
}

impl Default for StockState {
    fn default() -> Self {
        StockState {
            files: None,
            raw: entries_of(SAMPLE_RAW),
            trade: entries_of(SAMPLE_TRADE),
            data: StockData::sample(),
            tags: HashMap::new(),
            mining: MiningData::default(),
            seen: [None, None],
            looked_at: f64::NEG_INFINITY,
            items: false,
            kind: None,
            place: String::new(),
            purpose: 0,
            only_gaps: false,
            search: String::new(),
            sort: Column::Name,
            descending: false,
            open: HashSet::new(),
            closed: HashSet::new(),
            closed_for: String::new(),
            form_open: false,
            form: Form::Raw,
            raw_form: RawForm::default(),
            trade_form: TradeForm::default(),
            item_form: ItemForm::default(),
            raw_place: String::new(),
            trade_place: String::new(),
            editing: None,
            to_form: false,
            refinery_open: false,
            refinery_text: String::new(),
            refinery_cscu: true,
            refinery_place: String::new(),
            refinery_error: None,
            refinery_last: String::new(),
            refinery_asked: false,
            refinery_focus: false,
            refinery_field: None,
            panel: None,
            confirm: None,
            message: None,
            dialog: None,
            counted: Vec::new(),
            split: crate::stock_share::Split::default(),
            sugg: suggestions::Suggestions::default(),
            scan: scan::Scan::default(),
            moved_note: false,
            moved_seen: false,
            moved_unneeded: false,
            reloc: relocation::Relocation::default(),
        }
    }
}

fn modified(file: &str) -> Option<SystemTime> {
    std::fs::metadata(file).and_then(|m| m.modified()).ok()
}

fn unit_word(piece: bool) -> String {
    if piece { t("s_lg_stueck") } else { "SCU".to_string() }
}

fn amount_with_unit(lang: Lang, value: f64, piece: bool) -> String {
    formats::amount(lang, value, &unit_word(piece))
}

/// The number of an amount without its unit.
fn number_only(lang: Lang, value: f64) -> String {
    formats::amount(lang, value, "").trim_end().to_string()
}

/// A number for a field: up to four places, trailing zeros dropped, the
/// decimal mark of the language.
fn field_text(value: f64) -> String {
    let text = format!("{:.4}", stock::py_round(value, 4));
    let text = text.trim_end_matches('0').trim_end_matches('.');
    text.replace('.', &formats::decimal_mark(language::current()).to_string())
}

fn name_of(entry: &Value, key: &str) -> String {
    match entry.get(key) {
        Some(Value::Str(s)) if !s.trim().is_empty() => s.trim().to_string(),
        _ => "?".to_string(),
    }
}

impl StockState {
    /// The page on the stock files of `paths` with the sample lists of materials, places and goods (tests).
    #[cfg(test)]
    pub(crate) fn sample_on(paths: &Paths) -> StockState {
        let files = (paths.app_file(stock::RAW_FILE), paths.app_file(stock::TRADE_FILE));
        let mut state = StockState { files: Some(files), ..StockState::default() };
        state.connect_journal(paths);
        state.reloc = relocation::Relocation::new(paths);
        state.reload();
        state
    }

    /// The text of the message at the top right, taken away (tests).
    #[cfg(test)]
    pub(crate) fn take_message_text(&mut self) -> Option<String> {
        self.message.take().map(|m| m.text)
    }

    /// Books a material through the form (tests).
    #[cfg(test)]
    pub(crate) fn book_raw(&mut self, material: &str, amount: &str, quality: &str, place: &str) {
        self.raw_form = RawForm {
            material: material.into(),
            amount: amount.into(),
            quality: quality.into(),
            place: place.into(),
            ..RawForm::default()
        };
        self.save_raw();
    }

    /// Books goods for selling through the form (tests).
    #[cfg(test)]
    pub(crate) fn book_goods(&mut self, goods: &str, amount: &str, place: &str) {
        self.trade_form = TradeForm { goods: goods.into(), amount: amount.into(), place: place.into(), ..TradeForm::default() };
        self.save_trade();
    }

    /// The error at the place field of the material form (tests).
    #[cfg(test)]
    pub(crate) fn place_error(&self) -> Option<String> {
        self.raw_form.errors.place.clone()
    }

    /// The error at the place field of the goods form (tests).
    #[cfg(test)]
    pub(crate) fn trade_place_error(&self) -> Option<String> {
        self.trade_form.errors.place.clone()
    }

    /// The places the three forms start on: material, goods, items (tests).
    #[cfg(test)]
    pub(crate) fn form_places(&self) -> [String; 3] {
        [self.raw_form.place.clone(), self.trade_form.place.clone(), self.item_form.place.clone()]
    }

    /// Changes the amount of a material entry through the form (tests).
    #[cfg(test)]
    pub(crate) fn change_raw(&mut self, index: usize, amount: &str) {
        self.edit(Store::Raw, index);
        self.raw_form.amount = amount.into();
        self.save_raw();
    }

    /// Gives a material entry a place through the form (tests).
    #[cfg(test)]
    pub(crate) fn change_raw_place(&mut self, index: usize, place: &str) {
        self.edit(Store::Raw, index);
        self.raw_form.place = place.into();
        self.save_raw();
    }

    /// Books the lines of the refinery field at `place` (tests).
    #[cfg(test)]
    pub(crate) fn book_refinery_lines(&mut self, text: &str, place: &str) {
        self.refinery_text = text.into();
        self.refinery_place = place.into();
        book_refinery(self);
    }

    /// Types `text` and `place` into the refinery block and presses its booking button (tests).
    #[cfg(test)]
    pub(crate) fn press_refinery_with(&mut self, text: &str, place: &str) {
        self.refinery_text = text.into();
        self.refinery_place = place.into();
        self.press_refinery(&egui::Context::default());
    }

    /// Whether the last press of the booking button led to the place field (tests).
    #[cfg(test)]
    pub(crate) fn refinery_leads_to_place(&self) -> bool {
        self.refinery_focus
    }

    /// The hint under the place field while there is something to book (tests).
    #[cfg(test)]
    pub(crate) fn refinery_note_key(&self) -> Option<&'static str> {
        self.refinery_place_note(true)
    }

    /// The click on the line above the list (tests).
    #[cfg(test)]
    pub(crate) fn click_gap_line(&mut self) {
        let rows = self.rows();
        self.toggle_gaps(&rows);
    }

    /// Shows the tab of the items or of the materials (tests).
    #[cfg(test)]
    pub(crate) fn show_items_tab(&mut self, items: bool) {
        self.items = items;
    }

    /// Whether the tab of the items is shown (tests).
    #[cfg(test)]
    pub(crate) fn items_tab(&self) -> bool {
        self.items
    }

    /// Name and place of the rows the list lets through (tests).
    #[cfg(test)]
    pub(crate) fn shown_rows(&self) -> Vec<(String, String)> {
        visible(self, &self.rows()).into_iter().map(|r| (r.name, r.place)).collect()
    }

    /// Entry form and refinery open, for tests of the page around the list.
    #[cfg(test)]
    pub(crate) fn open_forms(&mut self) {
        self.form_open = true;
        self.refinery_open = true;
    }

    /// The (room left on screen, height asked for, visible page, height drawn, row height) of the last frame.
    #[cfg(test)]
    pub(crate) fn list_room_seen(ctx: &egui::Context) -> Option<(f32, f32, f32, f32, f32)> {
        ctx.data(|d| d.get_temp::<(f32, f32, f32, f32, f32)>(egui::Id::unique(LIST_ROOM_ID)))
    }

    /// The actions of the usage counters done since the last call (`lager_hand`, `lager_raffinerie`).
    pub fn take_counted(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.counted)
    }

    /// With own data: both stock files and the helper lists of the data folder.
    pub fn with_files(paths: &Paths) -> StockState {
        let raw = paths.app_file(stock::RAW_FILE);
        let trade = paths.app_file(stock::TRADE_FILE);
        let mut state = StockState {
            files: Some((raw, trade)),
            raw: Vec::new(),
            trade: Vec::new(),
            data: StockData::load(|name| paths.app_file(name)),
            tags: versekit_core::catalog_cache::load_current(paths)
                .map(|catalog| versekit_core::stock_data::item_tags(&catalog))
                .unwrap_or_default(),
            mining: MiningData::load(&paths.app_file(versekit_core::mining_data::FILE)),
            ..StockState::default()
        };
        state.items = paths.setting(SETTING_VIEW).as_deref() == Some("items");
        state.form_open = paths.setting_bool(SETTING_FORM_OPEN, false);
        state.form = match paths.setting(SETTING_FORM_KIND).as_deref() {
            Some("trade") => Form::Trade,
            Some("item") => Form::Item,
            _ => Form::Raw,
        };
        state.refinery_open = paths.setting_bool(SETTING_REFINERY_OPEN, false);
        state.panel =
            paths.setting_bool(SETTING_PANEL_OPEN, false).then(|| match paths.setting(SETTING_PANEL).as_deref() {
                Some("leeren") => Panel::Clear,
                Some("umziehen") => Panel::Move,
                _ => Panel::Backup,
            });
        state.raw_place = paths.setting(SETTING_RAW_PLACE).unwrap_or_default();
        state.raw_form.cscu = paths.setting(SETTING_UNIT).as_deref() == Some("cscu");
        state.trade_place = paths.setting(SETTING_TRADE_PLACE).unwrap_or_default();
        state.raw_form.place = state.raw_place.clone();
        state.trade_form.place = state.trade_place.clone();
        state.item_form.place = state.trade_place.clone();
        // The place of the last yield as long as the list still knows it, otherwise the one of the last material.
        let last = paths.setting(SETTING_REFINERY_PLACE).unwrap_or_default();
        if !last.trim().is_empty() && state.data.knows_place(&last) {
            state.refinery_last = last;
        }
        state.refinery_place =
            if state.refinery_last.is_empty() { state.raw_place.clone() } else { state.refinery_last.clone() };
        state.sugg.open = paths.setting_bool(SETTING_SUGGESTIONS_OPEN, true);
        state.scan = scan::Scan::new(paths);
        state.connect_journal(paths);
        state.reloc = relocation::Relocation::new(paths);
        state.reload();
        state.moved_seen = paths.setting_bool(SETTING_MOVED_SEEN, false);
        if !state.moved_seen {
            // Whoever used the two stores of before: entries in them, or one of them opened once (4.12 `is_old_user`).
            if !state.raw.is_empty() || !state.trade.is_empty() || opened_old_stores(paths) {
                state.moved_note = true;
            } else {
                // Nobody to tell: noted when the page is shown, so stock that comes later (a Basetool sync) brings
                // no card. Reading the settings at the start writes nothing.
                state.moved_unneeded = true;
            }
        }
        state
    }

    /// The reading of the refinery terminal the hotkey of the overlay handed over, into the open block of the
    /// refinery yield; `false` when none waits.
    pub fn take_refinery_reading(&mut self) -> bool {
        scan::take_handed_over(self)
    }

    /// Opens the block of the refinery yield, scrolls to it and starts reading the screen (the tile "Scannen").
    pub fn read_refinery_screen(&mut self, ctx: &egui::Context) {
        self.refinery_open = true;
        self.scan.scroll = true;
        scan::start(self, ctx);
    }

    /// Takes the choice an old page name brings (`overview::resolve`): the view and the purpose of the list.
    pub fn apply_preset(&mut self, preset: versekit_core::overview::Preset) {
        self.items = preset.view == "items";
        self.purpose = match preset.purpose {
            "craft" => 1,
            "sale" => 2,
            _ => 0,
        };
    }

    /// The view and the purpose the list shows: (items, 0 all, 1 to craft, 2 for sale).
    #[cfg(test)]
    pub(crate) fn view_and_purpose(&self) -> (bool, usize) {
        (self.items, self.purpose)
    }

    /// Connects the page to the sync: personal and shared parts, sharing into a unit.
    pub fn connect(&mut self, link: Option<crate::stock_share::Link>) {
        self.split = crate::stock_share::Split::new(link);
    }

    /// The settings this page keeps in the settings file.
    pub fn setting_values(&self) -> Vec<(&'static str, Value)> {
        if self.files.is_none() {
            return Vec::new();
        }
        vec![
            (SETTING_VIEW, Value::str(if self.items { "items" } else { "material" })),
            (SETTING_FORM_OPEN, Value::Bool(self.form_open)),
            (SETTING_FORM_KIND, Value::str(self.form.code())),
            (SETTING_REFINERY_OPEN, Value::Bool(self.refinery_open)),
            (SETTING_PANEL_OPEN, Value::Bool(self.panel.is_some())),
            (
                SETTING_PANEL,
                Value::str(match self.panel {
                    Some(Panel::Clear) => "leeren",
                    Some(Panel::Move) => "umziehen",
                    _ => "sichern",
                }),
            ),
            (SETTING_UNIT, Value::str(if self.raw_form.cscu { "cscu" } else { "scu" })),
            (SETTING_RAW_PLACE, Value::str(self.raw_place.clone())),
            (SETTING_TRADE_PLACE, Value::str(self.trade_place.clone())),
            (SETTING_REFINERY_PLACE, Value::str(self.refinery_last.clone())),
            (SETTING_SUGGESTIONS_OPEN, Value::Bool(self.sugg.open)),
            (SETTING_MOVED_SEEN, Value::Bool(self.moved_seen)),
        ]
    }

    fn reload(&mut self) {
        let Some((raw, trade)) = &self.files else { return };
        self.seen = [modified(raw), modified(trade)];
        self.raw = stock::load(raw, "materials.load");
        self.trade = stock::load(trade, "trade_cargo.load");
    }

    /// Reads the files again when they changed on disk; an entry in the form
    /// that changed meanwhile stops being edited.
    fn follow(&mut self, ui: &Ui) {
        let now = ui.input(|i| i.time);
        if now - self.looked_at < LOOK_SECONDS {
            return;
        }
        self.looked_at = now;
        self.follow_files();
    }

    /// Reads the files again when they changed on disk (the overview asks before it counts).
    pub(crate) fn follow_files(&mut self) {
        let Some((raw, trade)) = &self.files else { return };
        if [modified(raw), modified(trade)] != self.seen {
            self.reload();
            self.drop_stale_edit();
        }
    }

    /// Before a change: both files as they are on disk, and false when the
    /// entry in the form changed meanwhile (`drop_stale_edit` in 4.12).
    fn fresh(&mut self) -> bool {
        self.reload();
        !self.drop_stale_edit()
    }

    fn entries(&self, store: Store) -> &Vec<Value> {
        match store {
            Store::Raw => &self.raw,
            Store::Trade => &self.trade,
        }
    }

    fn entries_mut(&mut self, store: Store) -> &mut Vec<Value> {
        match store {
            Store::Raw => &mut self.raw,
            Store::Trade => &mut self.trade,
        }
    }

    /// Writes one file; without own data only the page holds it. A failed
    /// write reads both files again, so nothing unsaved stays behind.
    fn write(&mut self, store: Store) -> bool {
        let Some((raw, trade)) = self.files.clone() else { return true };
        let (path, label) = match store {
            Store::Raw => (raw, "materials.save"),
            Store::Trade => (trade, "trade_cargo.save"),
        };
        let done = stock::save(&path, self.entries(store), label);
        if done {
            self.seen = match &self.files {
                Some((raw, trade)) => [modified(raw), modified(trade)],
                None => [None, None],
            };
        } else {
            self.reload();
            self.say(t("s_hl_fehler"));
        }
        done
    }

    fn say(&mut self, text: String) {
        self.message = Some(Message { text, since: None, undo: None });
    }

    /// A message that does not push away a pending undo.
    fn tell(&mut self, text: String) {
        if !self.message.as_ref().is_some_and(|m| m.undo.is_some()) {
            self.say(text);
        }
    }

    fn still_editing(&self) -> bool {
        match &self.editing {
            None => true,
            Some(e) => self.entries(e.store).get(e.index) == Some(&e.entry),
        }
    }

    /// The entry in the form changed meanwhile: stop editing and say so.
    fn drop_stale_edit(&mut self) -> bool {
        if self.still_editing() {
            return false;
        }
        self.stop_editing();
        self.say(t("s_mlg_veraendert"));
        true
    }

    /// Empties the form that shows the entry being edited (`reset` in 4.12).
    fn stop_editing(&mut self) {
        let Some(editing) = self.editing.take() else { return };
        match editing.form {
            Form::Raw => self.reset_raw(),
            Form::Trade => self.reset_trade(),
            Form::Item => self.reset_item(),
        }
    }

    fn reset_raw(&mut self) {
        let f = &mut self.raw_form;
        (f.material, f.amount, f.quality, f.errors) = (String::new(), String::new(), String::new(), Errors::default());
        f.place = self.raw_place.clone();
    }

    fn reset_trade(&mut self) {
        let f = &mut self.trade_form;
        (f.goods, f.amount, f.cost, f.errors) = (String::new(), String::new(), String::new(), Errors::default());
        f.stolen = false;
        f.place = self.trade_place.clone();
    }

    fn reset_item(&mut self) {
        let f = &mut self.item_form;
        (f.name, f.amount, f.errors) = (String::new(), String::new(), Errors::default());
        f.stolen = false;
        f.place = self.trade_place.clone();
    }

    /// Both forms of the trade stock start on the place last booked there.
    fn remember_trade_place(&mut self, place: &str) {
        self.trade_place = place.to_string();
        self.trade_form.place = place.to_string();
        self.item_form.place = place.to_string();
    }

    pub(crate) fn rows(&self) -> Vec<Row> {
        stock::rows(&self.raw, &self.trade, &self.data)
    }

    fn kind_of(&self, row: &Row) -> Kind {
        if row.item {
            self.data.item_kind(&row.name)
        } else {
            self.data.material_kind(&row.name, row.store == Store::Trade)
        }
    }

    // ------------------------------------------------------------ editing

    /// Puts an entry into its form (`my_stock_page.edit`).
    fn edit(&mut self, store: Store, index: usize) {
        self.stop_editing();
        let Some(entry) = self.entries(store).get(index).cloned() else { return };
        let text = |key: &str| match entry.get(key) {
            Some(Value::Str(s)) => s.clone(),
            _ => String::new(),
        };
        let stolen = truthy(entry.get("gestohlen"));
        let form = match store {
            Store::Raw => Form::Raw,
            Store::Trade if stock::is_pieces(&entry) => Form::Item,
            Store::Trade => Form::Trade,
        };
        let name = match form {
            Form::Raw => {
                let name = text("material");
                let factor = self.raw_factor(&name);
                let quality = stock::number_of(&entry, "qualitaet");
                let f = &mut self.raw_form;
                f.material = name.clone();
                f.amount = field_text(stock::number_of(&entry, "menge") / factor);
                f.quality = if quality != 0.0 { language::py_g(quality, false) } else { String::new() };
                f.place = text("ort");
                name
            }
            Form::Trade => {
                let f = &mut self.trade_form;
                f.goods = text("ware");
                f.amount = field_text(stock::number_of(&entry, "menge"));
                f.cost = stock::cost_of(&entry).map(field_text).unwrap_or_default();
                f.place = text("ort");
                f.stolen = stolen;
                f.goods.clone()
            }
            Form::Item => {
                let f = &mut self.item_form;
                f.name = text("ware");
                f.amount = format!("{}", stock::number_of(&entry, "menge").trunc() as i64);
                f.place = text("ort");
                f.stolen = stolen;
                f.name.clone()
            }
        };
        let rows = self.rows();
        if let Some(row) = rows.iter().find(|r| r.store == store && r.index == index) {
            let key = group_key(row.item, row);
            self.closed.remove(&key);
        }
        self.form = form;
        self.editing = Some(Editing { store, index, entry, form });
        self.form_open = true;
        self.to_form = true;
        self.tell(tf("s_lg_bearbeite", &[("0", Arg::from(if name.is_empty() { "?".to_string() } else { name }))]));
    }

    fn raw_factor(&self, material: &str) -> f64 {
        if self.data.is_piece(material) || !self.raw_form.cscu { 1.0 } else { stock::CSCU }
    }

    fn editing_index(&self, store: Store) -> Option<usize> {
        self.editing.as_ref().filter(|e| e.store == store).map(|e| e.index)
    }

    /// Deletes an entry at once; the message offers to put it back.
    fn delete(&mut self, store: Store, index: usize) {
        let expected = self.entries(store).get(index).cloned();
        if !self.fresh() {
            return;
        }
        // The file changed under the click: nothing is deleted.
        if self.entries(store).get(index) != expected.as_ref() || expected.is_none() {
            self.say(t("s_mlg_veraendert"));
            return;
        }
        let entry = self.entries_mut(store).remove(index);
        let name = name_of(&entry, if store == Store::Raw { "material" } else { "ware" });
        self.stop_editing();
        if self.write(store) {
            let text = tf("s_mlg_geloescht", &[("name", Arg::from(name.as_str()))]);
            let deleted = Deleted { store, index, entry, name };
            self.message = Some(Message { text, since: None, undo: Some(Undo::Deleted(deleted)) });
        }
    }

    /// Takes back what the message offers to undo.
    fn take_back(&mut self, undo: Undo) {
        match undo {
            Undo::Deleted(deleted) => self.undo(deleted),
            Undo::Replaced(store, before) => {
                self.reload();
                self.stop_editing();
                *self.entries_mut(store) = before;
                if self.write(store) {
                    self.say(t("s_mlg_einlesen_zurueck"));
                }
            }
            Undo::Suggestions(undo) => self.undo_suggestions(undo),
            Undo::Relocated => relocation::take_back(self),
        }
    }

    /// Puts a deleted entry back at its place.
    fn undo(&mut self, deleted: Deleted) {
        self.reload();
        self.stop_editing();
        let entries = self.entries_mut(deleted.store);
        let at = deleted.index.min(entries.len());
        entries.insert(at, deleted.entry);
        if self.write(deleted.store) {
            self.say(tf("s_mlg_zurueck", &[("name", Arg::from(deleted.name.as_str()))]));
        }
    }

    /// Saves the material form (`my_stock_page._raw_form.save`).
    fn save_raw(&mut self) {
        if !self.fresh() {
            return;
        }
        self.raw_form.errors = Errors::default();
        let typed = self.raw_form.material.trim().to_string();
        if typed.is_empty() {
            self.raw_form.errors.name = Some(t("s_lg_kein_material"));
            return;
        }
        let Some(name) = self.data.storage_name(&typed) else {
            self.raw_form.errors.name = Some(tf("s_mlg_name_fremd", &[("name", Arg::from(typed.as_str()))]));
            return;
        };
        let corrected = name != typed && lower(&name) != lower(&typed);
        let piece = self.data.is_piece(&name);
        let factor = self.raw_factor(&name);
        let index = self.editing_index(Store::Raw);
        let before = index.and_then(|i| self.raw.get(i)).map_or(0.0, |e| stock::number_of(e, "menge"));
        let typed_amount = self.raw_form.amount.trim();
        let raw_text = if typed_amount.is_empty() { "0".to_string() } else { typed_amount.to_string() };
        let calculating = stock::is_calculation(&raw_text);
        let Some(value) = stock::calculate(&raw_text, before / factor) else {
            self.raw_form.errors.amount = Some(t("s_lg_keine_menge"));
            return;
        };
        if calculating && value < 0.0 {
            let error = match index {
                None => t("s_lg_nicht_negativ"),
                Some(_) => tf("s_lg_zu_wenig", &[("0", Arg::from(before)), ("1", Arg::from(unit_word(piece)))]),
            };
            self.raw_form.errors.amount = Some(error);
            return;
        }
        if calculating
            && value == 0.0
            && let Some(i) = index
        {
            self.raw.remove(i);
            self.editing = None;
            if self.write(Store::Raw) {
                self.reset_raw();
                self.say(tf("s_mlg_alles_weg", &[("name", Arg::from(name.as_str()))]));
            }
            return;
        }
        if value <= 0.0 {
            self.raw_form.errors.amount = Some(t("s_lg_keine_menge"));
            return;
        }
        let Some(place) = self.data.official_place(&self.raw_form.place) else {
            self.raw_form.errors.place = Some(self.unknown_place_text());
            return;
        };
        if self.lacks_place(&place) {
            self.raw_form.errors.place = Some(t("s_ml_ort_fehlt"));
            return;
        }
        self.raw_form.place = place.clone();
        let quality =
            stock::parse_number(&self.raw_form.quality).map(stock::py_round_int).filter(|q| (0..=1000).contains(q));
        let Some(quality) = quality else {
            self.raw_form.errors.quality = Some(t("s_lg_keine_guete"));
            return;
        };
        let value = stock::py_round(value * factor, 4);
        // A new booking may go into a unit at once ("For"); a wrong amount for it books nothing.
        let share = match index {
            None => match self.split.choose(value, piece) {
                Ok(share) => share,
                Err(_) => return,
            },
            Some(_) => None,
        };
        self.counted.push("lager_hand");
        let args = [("0", Arg::from(name.as_str())), ("1", Arg::from(value)), ("2", Arg::from(unit_word(piece)))];
        let mut text = match index {
            None => {
                stock::raw_add(&mut self.raw, &self.data, &name, value, Some(quality), &place);
                tf("s_lg_eingetragen", &args)
            }
            Some(i) => {
                stock::raw_change(&mut self.raw, i, &name, value, Some(quality), &place);
                tf("s_lg_geaendert", &args)
            }
        };
        if corrected {
            let fixed = tf("s_lg_berichtigt", &[("0", Arg::from(typed.as_str())), ("1", Arg::from(name.as_str()))]);
            text = format!("{fixed} {text}");
        }
        if self.write(Store::Raw) {
            self.editing = None;
            if let Some((unit, amount)) = share {
                let (name, place) = (name.clone(), place.clone());
                self.split.queue(crate::stock_share::For {
                    store: Store::Raw,
                    name,
                    place,
                    quality: Some(quality),
                    stolen: false,
                    unit,
                    amount,
                });
            }
            self.split.booked(Store::Raw, &name, &place, Some(quality), false);
            self.raw_place = place;
            self.reset_raw();
            self.say(text);
        }
    }

    /// Saves the goods form (`my_stock_page._trade_form.save`).
    fn save_trade(&mut self) {
        if !self.fresh() {
            return;
        }
        let index = self.editing_index(Store::Trade);
        let name_before = index
            .and_then(|i| self.trade.get(i))
            .and_then(|e| e.get("ware").and_then(Value::as_str).map(|s| s.trim().to_string()));
        self.trade_form.errors = Errors::default();
        let name = self.trade_form.goods.trim().to_string();
        if !self.data.knows_goods(&name) && Some(&name) != name_before.as_ref() {
            self.trade_form.errors.name = Some(t("s_hl_unbekannt"));
            return;
        }
        if !self.data.knows_place(&self.trade_form.place) {
            self.trade_form.errors.place = Some(self.unknown_place_text());
            return;
        }
        if self.lacks_place(&self.trade_form.place) {
            self.trade_form.errors.place = Some(t("s_ml_ort_fehlt"));
            return;
        }
        let f = self.trade_form.clone();
        let share = match index {
            None => match self.split.choose(stock::calculate(f.amount.trim(), 0.0).unwrap_or(0.0), false) {
                Ok(share) => share,
                Err(_) => return,
            },
            Some(_) => None,
        };
        let result = match index {
            None => stock::trade_add(&mut self.trade, &name, &f.amount, &f.place, f.stolen, &f.cost),
            Some(i) => {
                stock::trade_change(&mut self.trade, i, &name, &f.amount, &f.place, f.stolen, Cost::Text(&f.cost))
            }
        };
        if let Err(reason) = result {
            match reason {
                Refused::Goods => self.trade_form.errors.name = Some(t("s_hl_fehlt_ware")),
                Refused::Amount => self.trade_form.errors.amount = Some(t("s_hl_fehlt_menge")),
                Refused::Price => self.trade_form.errors.cost = Some(t("s_hl_fehlt_preis")),
                Refused::Gone => self.say(t("s_hl_fehler")),
            }
            return;
        }
        if self.write(Store::Trade) {
            self.editing = None;
            if let Some((unit, amount)) = share {
                self.split.queue(crate::stock_share::For {
                    store: Store::Trade,
                    name: name.clone(),
                    place: f.place.clone(),
                    quality: None,
                    stolen: f.stolen,
                    unit,
                    amount,
                });
            }
            self.split.booked(Store::Trade, &name, &f.place, None, f.stolen);
            self.remember_trade_place(&f.place);
            let form = &mut self.trade_form;
            (form.goods, form.amount, form.cost) = (String::new(), String::new(), String::new());
            self.say(t("s_hl_gebucht"));
        }
    }

    /// Saves the item form (`my_stock_page._item_form.save`).
    fn save_item(&mut self) {
        if !self.fresh() {
            return;
        }
        let index = self.editing_index(Store::Trade);
        let before = index.and_then(|i| self.trade.get(i)).cloned();
        let name_before =
            before.as_ref().and_then(|e| e.get("ware").and_then(Value::as_str)).map(|s| s.trim().to_string());
        self.item_form.errors = Errors::default();
        let typed = self.item_form.name.trim().to_string();
        let found = self.data.item_choice(&typed).cloned();
        if found.is_none() && Some(&typed) != name_before.as_ref() {
            self.item_form.errors.name = Some(tf("s_mlg_item_unbekannt", &[("name", Arg::from(typed.as_str()))]));
            return;
        }
        if !self.data.knows_place(&self.item_form.place) {
            self.item_form.errors.place = Some(self.unknown_place_text());
            return;
        }
        if self.lacks_place(&self.item_form.place) {
            self.item_form.errors.place = Some(t("s_ml_ort_fehlt"));
            return;
        }
        let number = |v: &Value| match v {
            Value::Int(n) => n.parse::<i64>().ok(),
            _ => None,
        };
        let uex = match &found {
            Some(choice) => choice.item.as_ref().and_then(number),
            None => before.as_ref().and_then(|e| e.get("uex")).and_then(number),
        };
        let name = found.map_or(typed, |c| c.name);
        let f = self.item_form.clone();
        let share = match index {
            None => match self.split.choose(stock::calculate(f.amount.trim(), 0.0).unwrap_or(0.0), true) {
                Ok(share) => share,
                Err(_) => return,
            },
            Some(_) => None,
        };
        let result = stock::trade_add_item(&mut self.trade, &name, &f.amount, &f.place, f.stolen, uex, index);
        if let Err(reason) = result {
            match reason {
                Refused::Amount => self.item_form.errors.amount = Some(t("s_ml_item_menge")),
                Refused::Goods => self.item_form.errors.name = Some(t("s_hl_fehlt_ware")),
                _ => self.say(t("s_hl_fehler")),
            }
            return;
        }
        if self.write(Store::Trade) {
            self.editing = None;
            if let Some((unit, amount)) = share {
                self.split.queue(crate::stock_share::For {
                    store: Store::Trade,
                    name: name.clone(),
                    place: f.place.clone(),
                    quality: None,
                    stolen: f.stolen,
                    unit,
                    amount,
                });
            }
            self.split.booked(Store::Trade, &name, &f.place, None, f.stolen);
            self.remember_trade_place(&f.place);
            (self.item_form.name, self.item_form.amount) = (String::new(), String::new());
            self.say(t("s_hl_gebucht"));
        }
    }

    fn clear(&mut self, part: Part) {
        self.reload();
        self.stop_editing();
        let (store, count) = match part {
            Part::Raw => {
                let n = self.raw.len();
                self.raw.clear();
                (Store::Raw, n)
            }
            Part::Goods => (Store::Trade, stock::clear_goods(&mut self.trade)),
            Part::Items => (Store::Trade, stock::clear_items(&mut self.trade)),
        };
        if self.write(store) {
            self.say(tf("s_mlg_geleert", &[("n", Arg::from(count))]));
        }
    }

    /// Whether `path` is one of the page's own stock files.
    fn is_own_file(&self, path: &str) -> bool {
        let Some((raw, trade)) = &self.files else { return false };
        let same = |own: &str| match (std::fs::canonicalize(own), std::fs::canonicalize(path)) {
            (Ok(a), Ok(b)) => a == b,
            _ => std::path::Path::new(own) == std::path::Path::new(path),
        };
        same(raw) || same(trade)
    }

    /// Opens the dialog for `job` unless one is open already; it starts in
    /// the system's default folder.
    fn ask_file(&mut self, ctx: &egui::Context, job: Job) {
        if self.dialog.is_some() {
            self.say(t("s_mlg_dialog_offen"));
            return;
        }
        let start: Option<String> = None;
        let pending = match job {
            Job::Export(store, csv) => {
                let stem = if store == Store::Raw { "lager" } else { "handelslager" };
                let ending = if csv { "csv" } else { "json" };
                let name = format!("{stem}-{}.{ending}", today());
                let filters: crate::file_dialog::Filters =
                    if csv { &[("CSV", &["csv"])] } else { &[("JSON", &["json"])] };
                crate::file_dialog::save_file(ctx, &t("s_lg_ausgeben"), filters, start.as_deref(), &name)
            }
            Job::Import => {
                let filters: crate::file_dialog::Filters = &[("JSON", &["json"])];
                crate::file_dialog::open_file(ctx, &t("s_lg_einlesen"), filters, start.as_deref())
            }
        };
        self.dialog = Some((pending, job));
    }

    /// The answer of the file dialog, once it was closed.
    fn take_dialog(&mut self) {
        let Some(answer) = self.dialog.as_ref().and_then(|(p, _)| p.take()) else { return };
        let Some((_, job)) = self.dialog.take() else { return };
        // A cancelled dialog says nothing.
        match (answer, job) {
            (None, _) => {}
            (Some(path), Job::Export(store, csv)) => self.export_to(store, csv, &path),
            (Some(path), Job::Import) => self.import_from(&path),
        }
    }

    /// Writes one store as JSON backup or CSV table (`_backup_section.write_out`).
    /// A name without ending gets one (kdialog and zenity add none), and the
    /// page's own stock files are refused as target.
    fn export_to(&mut self, store: Store, csv: bool, path: &str) {
        let ending = if csv { "csv" } else { "json" };
        let path = if std::path::Path::new(path).extension().is_none() {
            format!("{path}.{ending}")
        } else {
            path.to_string()
        };
        let path = path.as_str();
        if self.is_own_file(path) {
            self.say(t("s_mlg_eigene_datei"));
            return;
        }
        self.reload();
        let entries = self.entries(store);
        let content = match (store, csv) {
            (_, false) => Some(stock::as_json(entries)),
            (Store::Raw, true) => stock::raw_csv(entries),
            (Store::Trade, true) => stock::trade_csv(entries),
        };
        let Some(content) = content else {
            self.say(t("s_hl_fehler"));
            return;
        };
        match jsonio::write_text_atomic(path, &content, jsonio::Newline::Native) {
            Ok(()) => {
                let name = std::path::Path::new(path)
                    .file_name()
                    .map_or_else(|| path.to_string(), |n| n.to_string_lossy().into_owned());
                self.say(tf("s_lg_gespeichert", &[("0", Arg::from(name.as_str()))]));
            }
            Err(e) if versekit_core::writeguard::refused(&e) => self.say(t("s_mlg_nur_testkopie")),
            Err(e) => {
                versekit_core::errors::record("my_stock_page.ausgeben", &e);
                self.say(t("s_hl_fehler"));
            }
        }
    }

    /// Reads a backup back in: materials replace all materials, goods replace
    /// the goods and keep the items (`_backup_section.read_in`). Keeps the old
    /// store as `….vor-einlesen` and offers to undo.
    fn import_from(&mut self, path: &str) {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) => {
                versekit_core::errors::record("my_stock_page.einlesen", &e);
                self.say(t("s_mlg_datei_falsch"));
                return;
            }
        };
        let Some(found) = stock::detect_backup(&text) else {
            self.say(t("s_mlg_datei_falsch"));
            return;
        };
        self.reload();
        self.stop_editing();
        let (store, replaced, text) = match found {
            stock::Backup::Raw(entries) => {
                let n = entries.len();
                (Store::Raw, entries, tf("s_ml_eingelesen_raw", &[("0", Arg::from(n))]))
            }
            stock::Backup::Trade(entries) => match stock::restore_trade(&self.trade, &entries) {
                Some((all, goods, added)) => (
                    Store::Trade,
                    all,
                    tf("s_ml_eingelesen_trade", &[("0", Arg::from(goods)), ("1", Arg::from(added))]),
                ),
                None => {
                    self.say(t("s_hl_fehler"));
                    return;
                }
            },
        };
        if !self.keep_before(store) {
            return;
        }
        let before = std::mem::replace(self.entries_mut(store), replaced);
        if self.write(store) {
            self.message = Some(Message { text, since: None, undo: Some(Undo::Replaced(store, before)) });
        }
    }

    /// Copies a store file to `….vor-einlesen` before a read-in replaces it.
    /// Without own data or without the file yet there is nothing to keep.
    fn keep_before(&mut self, store: Store) -> bool {
        let Some((raw, trade)) = &self.files else { return true };
        let file = if store == Store::Raw { raw.clone() } else { trade.clone() };
        if !std::path::Path::new(&file).exists() {
            return true;
        }
        match jsonio::copy2(&file, &format!("{file}.vor-einlesen")) {
            Ok(()) => true,
            Err(e) if versekit_core::writeguard::refused(&e) => {
                self.say(t("s_mlg_nur_testkopie"));
                false
            }
            Err(e) => {
                versekit_core::errors::record("my_stock_page.einlesen", &e);
                self.say(t("s_hl_fehler"));
                false
            }
        }
    }

    /// Whether search or a filter narrows the list (for the reset button in
    /// the page head).
    pub fn is_narrowed(&self) -> bool {
        !self.search.trim().is_empty()
            || self.kind.is_some()
            || !self.place.is_empty()
            || self.purpose != 0
            || self.only_gaps
    }

    /// Back to the whole stock of the tab.
    pub fn reset(&mut self) {
        self.search.clear();
        self.kind = None;
        self.place.clear();
        self.purpose = 0;
        self.only_gaps = false;
    }

    /// The line above the list was clicked: only the rows the Basetool does not get, or all again.
    fn toggle_gaps(&mut self, rows: &[Row]) {
        if self.only_gaps {
            self.only_gaps = false;
            return;
        }
        self.reset();
        self.split.set_scope_index(0);
        self.only_gaps = true;
        if !rows.iter().any(|r| r.item == self.items && self.split.gap(r).is_some()) {
            self.items = !self.items;
        }
    }

    /// The error of a place that is not in the list: with the sync on, an empty field is no way out.
    fn unknown_place_text(&self) -> String {
        t(if self.split.place_required() { "s_ml_ort_fremd_pflicht" } else { "s_mlg_ort_fremd" })
    }

    /// Whether the sync needs a place that is missing in `place`.
    fn lacks_place(&self, place: &str) -> bool {
        self.split.place_required() && versekit_core::text::py_strip(place).is_empty()
    }

    /// Opens a panel, or closes it when it is open; an open question goes.
    fn toggle_panel(&mut self, panel: Panel) {
        self.panel = if self.panel == Some(panel) { None } else { Some(panel) };
        self.confirm = None;
    }

    fn count(&self, part: Part) -> usize {
        match part {
            Part::Raw => self.raw.len(),
            Part::Goods => self.trade.iter().filter(|e| !stock::is_pieces(e)).count(),
            Part::Items => self.trade.iter().filter(|e| stock::is_pieces(e)).count(),
        }
    }
}

// ------------------------------------------------------------ list logic

/// The rows tabs, filters and search let through (`visible_rows`).
fn visible(state: &StockState, rows: &[Row]) -> Vec<Row> {
    let search = lower(state.search.trim());
    rows.iter()
        .filter(|r| r.item == state.items)
        .filter(|r| state.kind.is_none_or(|k| state.kind_of(r) == k))
        .filter(|r| state.place.is_empty() || lower(&r.place) == lower(&state.place))
        .filter(|r| state.items || state.purpose != 1 || r.store == Store::Raw)
        .filter(|r| state.items || state.purpose != 2 || r.store == Store::Trade)
        .filter(|r| search.is_empty() || lower(&r.name).contains(&search) || lower(&r.place).contains(&search))
        .filter(|r| !state.only_gaps || state.split.gap(r).is_some())
        .filter_map(|r| state.split.scoped(r))
        .collect()
}

fn group_key(items: bool, row: &Row) -> GroupKey {
    (items, lower(&row.name), row.piece && !items)
}

/// The places of some rows, each spelling once, sorted without case.
fn places_of<'a>(rows: impl Iterator<Item = &'a Row>) -> Vec<String> {
    let set: BTreeSet<&str> = rows.map(|r| r.place.as_str()).filter(|p| !p.is_empty()).collect();
    let mut places: Vec<String> = set.into_iter().map(str::to_string).collect();
    places.sort_by_key(|p| lower(p));
    places
}

/// Groups in list order (`_draw_list`): sorted by the chosen column, pieces
/// first among materials.
fn groups(state: &StockState, shown: &[Row]) -> Vec<(GroupKey, Vec<Row>)> {
    let mut out: Vec<(GroupKey, Vec<Row>)> = Vec::new();
    for row in shown {
        let key = group_key(state.items, row);
        match out.iter_mut().find(|(k, _)| *k == key) {
            Some((_, rows)) => rows.push(row.clone()),
            None => out.push((key, vec![row.clone()])),
        }
    }
    let sort_text = |rows: &[Row]| -> String {
        match state.sort {
            Column::Place => {
                let mut places: Vec<String> = rows.iter().map(|r| lower(&r.place)).collect();
                places.sort();
                places.join(" ")
            }
            _ => lower(&rows[0].name),
        }
    };
    let sort_number = |rows: &[Row]| -> f64 {
        match state.sort {
            Column::Amount => rows.iter().map(|r| r.amount).sum(),
            _ => rows.iter().map(|r| r.quality.unwrap_or(0) as f64).fold(f64::MIN, f64::max),
        }
    };
    out.sort_by(|(_, a), (_, b)| {
        let order = match state.sort {
            Column::Amount | Column::Quality => sort_number(a).total_cmp(&sort_number(b)),
            _ => sort_text(a).cmp(&sort_text(b)),
        };
        if state.descending { order.reverse() } else { order }
    });
    if !state.items {
        out.sort_by_key(|(_, rows)| state.data.piece_first(&rows[0].name));
    }
    out
}

/// Is this group open? By hand, while one of its entries is edited, or when
/// the search hits one of its places; folded by hand it stays shut.
fn is_open(state: &StockState, key: &GroupKey, rows: &[Row]) -> bool {
    let search = lower(state.search.trim());
    let editing = state.editing.as_ref().map(|e| (e.store, e.index));
    let edited = rows.iter().any(|r| Some((r.store, r.index)) == editing);
    let found = !search.is_empty() && rows.iter().any(|r| lower(&r.place).contains(&search));
    state.open.contains(key) || ((edited || found) && !state.closed.contains(key))
}

fn lines(state: &StockState, shown: &[Row]) -> Vec<Line> {
    let mut out = Vec::new();
    for (key, mut rows) in groups(state, shown) {
        if rows.len() == 1 {
            out.push(Line::Post(rows.remove(0), false));
            continue;
        }
        let open = is_open(state, &key, &rows);
        rows.sort_by(|a, b| {
            lower(&a.place).cmp(&lower(&b.place)).then_with(|| b.quality.unwrap_or(0).cmp(&a.quality.unwrap_or(0)))
        });
        let nested: Vec<Row> = if open { rows.clone() } else { Vec::new() };
        out.push(Line::Group(key, rows, open));
        out.extend(nested.into_iter().map(|r| Line::Post(r, true)));
    }
    out
}

fn profit_of(rows: &[Row], data: &StockData) -> Option<f64> {
    let mut total = 0.0;
    let mut known = false;
    for row in rows {
        let price = data.best_price(&row.name);
        if let Some(cost) = row.cost
            && price != 0.0
        {
            total += (price - cost) * row.amount;
            known = true;
        }
    }
    known.then_some(total)
}

/// The price paid for one piece over the rows that know it (the average by pieces); empty where none does.
fn paid_text(lang: Lang, rows: &[Row]) -> String {
    let priced: Vec<(f64, f64)> = rows.iter().filter_map(|r| r.cost.map(|c| (c, r.amount))).collect();
    let pieces: f64 = priced.iter().map(|(_, amount)| amount).sum();
    if priced.is_empty() {
        return String::new();
    }
    let average = if pieces > 0.0 {
        priced.iter().map(|(cost, amount)| cost * amount).sum::<f64>() / pieces
    } else {
        priced[0].0
    };
    formats::money(lang, average)
}

/// The cells of one line besides the name, with their color; empty where
/// nothing is known.
fn cells(state: &StockState, scheme: &Scheme, lang: Lang, line: &Line, columns: &[Cell]) -> Vec<(String, Color)> {
    let rows: &[Row] = match line {
        Line::Group(_, rows, _) => rows,
        Line::Post(row, _) => std::slice::from_ref(row),
    };
    let first = &rows[0];
    columns
        .iter()
        .map(|cell| match cell {
            Cell::Place => {
                let places = places_of(rows.iter());
                let text = match places.len() {
                    0 => String::new(),
                    1 => places[0].clone(),
                    n => tf("s_lg_orte_n", &[("0", Arg::Int(n as i64))]),
                };
                (text, scheme.sub)
            }
            Cell::Quality => {
                let mut q: Vec<i64> = rows.iter().filter_map(|r| r.quality).collect();
                q.sort_unstable();
                let text = match (q.first(), q.last()) {
                    (Some(a), Some(b)) if a == b => a.to_string(),
                    (Some(a), Some(b)) => format!("{a}-{b}"),
                    _ => String::new(),
                };
                (text, scheme.sub)
            }
            Cell::Amount => {
                let sum = stock::py_round(rows.iter().map(|r| r.amount).sum(), 3);
                let tone = if matches!(line, Line::Post(r, _) if r.leftover) { scheme.gold } else { scheme.accent };
                (amount_with_unit(lang, sum, first.piece), tone)
            }
            Cell::Paid => (paid_text(lang, rows), scheme.sub),
            Cell::Price => {
                let price = state.data.best_price(&first.name);
                (if price != 0.0 { formats::money(lang, price) } else { String::new() }, scheme.sub)
            }
            Cell::Total => {
                let value = state.data.best_price(&first.name) * rows.iter().map(|r| r.amount).sum::<f64>();
                (if value != 0.0 { formats::money(lang, value) } else { String::new() }, scheme.fg)
            }
            Cell::Profit => match profit_of(rows, &state.data) {
                None => (String::new(), scheme.sub),
                Some(p) => {
                    let sign = if p > 0.0 { "+" } else { "" };
                    let tone = if p > 0.0 {
                        scheme.accent
                    } else if p < 0.0 {
                        scheme.red_text()
                    } else {
                        scheme.sub
                    };
                    (format!("{sign}{}", formats::money(lang, p)), tone)
                }
            },
        })
        .collect()
}

fn badges_of(state: &StockState, scheme: &Scheme, line: &Line, selling: bool) -> Vec<(String, Color)> {
    let mixed = !state.items && !selling;
    // What keeps a row from the Basetool comes first, so it stays among the two badges a row shows.
    let gap_mark = |rows: &[Row]| {
        rows.iter().find_map(|row| state.split.gap(row)).map(|gap| (crate::stock_share::gap_word(gap), scheme.gold))
    };
    match line {
        Line::Group(_, rows, _) => {
            let mut marks: Vec<(String, Color)> = gap_mark(rows).into_iter().collect();
            marks.push((t(&state.kind_of(&rows[0]).text_key()), scheme.sub));
            if mixed && rows.iter().any(|r| r.store == Store::Trade) {
                marks.push((t("s_ml_verkauf"), scheme.accent));
            }
            marks
        }
        Line::Post(row, nested) => {
            let mut marks: Vec<(String, Color)> = gap_mark(std::slice::from_ref(row)).into_iter().collect();
            if !nested {
                marks.push((t(&state.kind_of(row).text_key()), scheme.sub));
            }
            if mixed && row.store == Store::Trade {
                marks.push((t("s_ml_verkauf"), scheme.accent));
            }
            if row.stolen {
                marks.push((t("s_hl_marke"), scheme.gold));
            }
            if row.leftover {
                marks.push((t("s_ml_rest"), scheme.gold));
            }
            marks.extend(state.split.badges(scheme, row));
            marks
        }
    }
}

/// How a raw material is mined, `Hand · Fahrzeug · Schiff`, or empty when the mining data do not know it
/// (`my_stock_page.mining_text`).
fn mining_text(mining: &MiningData, name: &str) -> String {
    let kinds = mining.mining_kinds(name);
    [("fps", "s_lg_abbau_fps"), ("fahrzeug", "s_lg_abbau_fahrzeug"), ("schiff", "s_lg_abbau_schiff")]
        .iter()
        .filter(|(kind, _)| kinds.contains(*kind))
        .map(|(_, key)| t(key))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// What stands beside the name of a line: the tag of an item and how a raw material is mined; nothing in a
/// nested row (`_post_row`, `_group_row`).
fn name_extras(state: &StockState, line: &Line) -> (Option<String>, String) {
    let (row, nested) = match line {
        Line::Group(_, rows, _) => (&rows[0], false),
        Line::Post(row, nested) => (row, *nested),
    };
    if nested {
        return (None, String::new());
    }
    if state.items {
        let key = versekit_core::paths::name_key(versekit_core::text::py_strip(&row.name));
        (state.tags.get(&key).cloned(), String::new())
    } else {
        (None, mining_text(&state.mining, &row.name))
    }
}

fn text_width(ui: &Ui, text: &str, strong: bool) -> f32 {
    let rich = if strong { RichText::new(text).strong() } else { RichText::new(text) };
    egui::WidgetText::from(rich)
        .into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            egui::FontSelection::Style(egui::TextStyle::Body),
        )
        .size()
        .x
}

/// An icon button that also tells screen readers what it does.
fn named_icon(ui: &mut Ui, scheme: &Scheme, icon: &str, meaning: &str) -> egui::Response {
    let response = widgets::icon_button(ui, scheme, icon, meaning);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, meaning));
    response
}

// ------------------------------------------------------------ the page

/// Whether the file of the seen areas names the material storage or the cargo hold of before (4.12 `lager`,
/// `handelslager`): one of them was open once.
fn opened_old_stores(paths: &Paths) -> bool {
    let Ok(data) = jsonio::read_json(&paths.app_file(versekit_core::gallery::SEEN_FILE)) else { return false };
    data.get("bereiche").is_some_and(|areas| ["lager", "handelslager"].iter().any(|area| areas.get(area).is_some()))
}

/// The stock page.
/// Where the material storage and the cargo hold went, with a cross that hides the card for good.
fn moved_card(ui: &mut Ui, scheme: &Scheme, state: &mut StockState) {
    egui::Frame::new()
        .fill(color(scheme.surface))
        .stroke(egui::Stroke::new(1.0, color(scheme.line)))
        .corner_radius(crate::control_radius(scheme))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |card| {
            card.set_width(card.available_width());
            card.horizontal(|head| {
                // The title wraps in the room beside the cross, so the cross stays in the card at every size.
                let room =
                    (head.available_width() - widgets::ICON_BUTTON_SIZE - head.spacing().item_spacing.x).max(0.0);
                head.allocate_ui_with_layout(vec2(room, 0.0), Layout::top_down(Align::Min), |left| {
                    let title =
                        egui::Label::new(RichText::new(t("s_ml_umzug_titel")).strong().color(color(scheme.accent)));
                    left.add(title.wrap());
                });
                head.with_layout(egui::Layout::right_to_left(egui::Align::Center), |right| {
                    if widgets::icon_button(right, scheme, "x", &t("s_ml_umzug_weg")).clicked() {
                        state.moved_note = false;
                        state.moved_seen = true;
                    }
                });
            });
            card.add(egui::Label::new(RichText::new(t("s_ml_umzug")).small()).wrap());
        });
}

pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut StockState) {
    // The frame reads the files, changes an entry and writes: the sync takes over its own changes only between frames.
    let _lock = stock::lock();
    state.follow(ui);
    state.take_dialog();
    // 4.12 notes it on the first build of the page; the settings take it with the next change.
    if std::mem::take(&mut state.moved_unneeded) {
        state.moved_seen = true;
    }
    let lang = language::current();
    ui.add(egui::Label::new(RichText::new(t("s_mlg_lead")).small().color(color(scheme.sub))).wrap());
    ui.add_space(4.0);
    if state.moved_note {
        moved_card(ui, scheme, state);
        ui.add_space(GAP);
    }
    form_section(ui, scheme, state);
    refinery_section(ui, scheme, state);
    ui.add_space(GAP);
    let rows = state.rows();
    state.split.refresh(ui.ctx(), &rows, &state.raw, &state.trade);
    if let Some(text) = state.split.poll() {
        state.say(text);
    }
    tabs(ui, scheme, state, &rows);
    ui.add_space(GAP);
    gap_line(ui, scheme, state, &rows);
    if state.items {
        let now = ui.input(|i| i.time);
        state.look_at_suggestions(now, false);
        suggestions::show_block(ui, scheme, state);
    }
    state.split.show_form(ui, scheme);
    // Filters and search stand right above the table they narrow.
    filters(ui, scheme, state, &rows);
    ui.add_space(4.0);
    list(ui, scheme, state, &rows, lang);
    show_message(ui, scheme, state);
}

/// The line above the list that says how many rows the Basetool does not get; a click narrows the list to them.
fn gap_line(ui: &mut Ui, scheme: &Scheme, state: &mut StockState, rows: &[Row]) {
    let Some(text) = state.split.gaps(rows).text() else {
        state.only_gaps = false;
        return;
    };
    let word = t(if state.only_gaps { "s_ml_luecke_alle" } else { "s_ml_luecke_nur" });
    let kind = if state.only_gaps { ButtonKind::Strong } else { ButtonKind::Normal };
    let spoken = format!("{text}, {word}");
    let button = widgets::Button::new(&text, kind, scheme).icon("triangle-alert").wrapping().spoken(&spoken);
    if ui.add(button).on_hover_text(&word).clicked() {
        state.toggle_gaps(rows);
    }
    ui.add_space(GAP);
}

fn show_message(ui: &Ui, scheme: &Scheme, state: &mut StockState) {
    let Some(message) = &mut state.message else { return };
    let now = ui.input(|i| i.time);
    let since = *message.since.get_or_insert(now);
    let action = message.undo.as_ref().map(|_| t("s_mlg_rueckgaengig"));
    let icon = match message.undo {
        Some(Undo::Deleted(_)) => "trash-2",
        Some(Undo::Replaced(..)) => "folder",
        Some(Undo::Suggestions(_)) => "package-plus",
        Some(Undo::Relocated) => "house",
        None => "check",
    };
    let shown = widgets::notice(ui.ctx(), scheme, since, icon, &message.text, action.as_deref());
    if shown.acted {
        if let Some(Message { undo: Some(back), .. }) = state.message.take() {
            state.take_back(back);
        }
    } else if !shown.shown {
        state.message = None;
    }
}

/// A collapsing block whose open state the page keeps.
fn fold(ui: &mut Ui, title: &str, open: &mut bool, add: impl FnOnce(&mut Ui)) -> egui::Response {
    let shown = widgets::collapsing(title).open(Some(*open)).show(ui, add);
    if shown.header_response.clicked() {
        *open = !*open;
    }
    shown.header_response
}

fn field_label(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(text).strong());
}

fn field_error(ui: &mut Ui, scheme: &Scheme, error: &Option<String>) {
    if let Some(error) = error {
        ui.add(egui::Label::new(RichText::new(error).color(color(scheme.red_text()))).wrap());
    }
}

/// A field with suggestions from a closed list; red border and message
/// below while its input is wrong, gone as soon as the input changes.
/// Returns whether Enter was pressed in it.
fn suggest_field(
    ui: &mut Ui,
    scheme: &Scheme,
    salt: &str,
    hint: &str,
    text: &mut String,
    entries: &[String],
    error: &mut Option<String>,
) -> bool {
    suggest_field_marked(ui, scheme, salt, hint, text, entries, error, None).0
}

/// [`suggest_field`] that can also mark a field left empty: a border in the warning color and `note` below it
/// (an error shows red instead). Returns whether Enter was pressed in it, and the response of the field.
#[allow(clippy::too_many_arguments)]
fn suggest_field_marked(
    ui: &mut Ui,
    scheme: &Scheme,
    salt: &str,
    hint: &str,
    text: &mut String,
    entries: &[String],
    error: &mut Option<String>,
    note: Option<&str>,
) -> (bool, egui::Response) {
    let more = |n: usize| tf("s_mlg_mehr", &[("n", Arg::from(n))]);
    let shown_error = error.clone();
    let out = ui
        .scope(|ui| {
            let border = if shown_error.is_some() { Some(scheme.red) } else { note.map(|_| scheme.gold) };
            if let Some(tone) = border {
                let stroke = egui::Stroke::new(1.0, color(tone));
                let w = &mut ui.visuals_mut().widgets;
                (w.inactive.bg_stroke, w.hovered.bg_stroke) = (stroke, stroke);
            }
            widgets::SuggestField::new(salt, hint, scheme, &more).width(ui.available_width()).show(ui, text, entries)
        })
        .inner;
    if out.response.changed() {
        *error = None;
    }
    field_error(ui, scheme, error);
    if let (None, Some(note)) = (error.as_ref(), note) {
        ui.add(egui::Label::new(RichText::new(note).small().color(color(scheme.gold))).wrap());
    }
    let enter = out.picked.is_none() && out.response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
    (enter, out.response)
}

/// A plain field with an error below, gone as soon as the input changes.
/// Returns whether Enter was pressed in it.
fn plain_field(ui: &mut Ui, scheme: &Scheme, text: &mut String, hint: &str, error: &mut Option<String>) -> bool {
    let width = ui.available_width();
    let response = widgets::checked_field(ui, scheme, text, hint, width, error.as_deref());
    if response.changed() {
        *error = None;
    }
    response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))
}

fn form_section(ui: &mut Ui, scheme: &Scheme, state: &mut StockState) {
    let mut open = state.form_open;
    let header = fold(ui, &t("s_lg_hand_titel"), &mut open, |ui| {
        ui.horizontal_wrapped(|row| {
            for (form, key) in
                [(Form::Raw, "s_ml_herstellen"), (Form::Trade, "s_ml_verkaufen"), (Form::Item, "s_ml_item")]
            {
                if widgets::radio(row, scheme, state.form == form, &t(key)).clicked() && state.form != form {
                    state.form = form;
                    state.stop_editing();
                }
            }
        });
        match state.form {
            Form::Raw => raw_form(ui, scheme, state),
            Form::Trade => trade_form(ui, scheme, state),
            Form::Item => item_form(ui, scheme, state),
        }
    });
    state.form_open = open;
    if std::mem::take(&mut state.to_form) {
        header.scroll_to_me(Some(Align::Min));
    }
}

/// While an entry is edited: the buttons for sharing into a unit and taking back, the ones the
/// row offers (only where the row shows its whole amount).
fn edit_share_buttons(ui: &mut Ui, scheme: &Scheme, state: &mut StockState) {
    let Some((store, index)) = state.editing.as_ref().map(|e| (e.store, e.index)) else { return };
    let rows = state.rows();
    let Some(row) = rows.iter().find(|r| (r.store, r.index) == (store, index)) else { return };
    if !state.split.whole(row) {
        return;
    }
    let kinds = state.split.actions(row);
    if kinds.is_empty() {
        return;
    }
    ui.add_space(GAP);
    ui.horizontal_wrapped(|line| {
        for kind in kinds {
            let key = match kind {
                ShareKind::Share => "s_ml_form_teilen",
                ShareKind::Unshare => "s_ml_form_zurueck",
            };
            // An open form stays open; the list is above, nothing to scroll to.
            if line.add(widgets::Button::new(&t(key), ButtonKind::Normal, scheme)).clicked()
                && state.split.open_for(row) != Some(kind)
            {
                state.split.open(row, kind);
            }
        }
    });
}

/// Save, cancel and delete while editing; add otherwise. Returns the clicks.
fn form_buttons(ui: &mut Ui, scheme: &Scheme, editing: bool, raw: bool, no_place: bool) -> (bool, bool, bool) {
    let (mut save, mut cancel, mut delete) = (false, false, false);
    ui.add_space(GAP);
    let missing = t("s_ml_ort_fehlt");
    ui.horizontal_wrapped(|row| {
        let (save_key, cancel_key) =
            if raw { ("s_lg_speichern", "s_lg_abbrechen") } else { ("s_hl_speichern", "s_hl_abbrechen") };
        let key = if editing {
            save_key
        } else if raw {
            "s_lg_eintragen"
        } else {
            "s_hl_buchen"
        };
        let word = t(key);
        // Without a place the button stays grey and says why, next to it as well as on hover.
        save = row
            .add(widgets::Button::new(&word, ButtonKind::Strong, scheme).blocked(no_place.then_some(missing.as_str())))
            .clicked();
        if no_place {
            row.label(RichText::new(&missing).small().color(color(scheme.gold)));
        }
        if editing {
            cancel = row.add(widgets::Button::new(&t(cancel_key), ButtonKind::Normal, scheme)).clicked();
            row.add_space(16.0);
            delete = row.add(widgets::Button::new(&t("s_lg_posten_weg"), ButtonKind::Danger, scheme)).clicked();
        }
    });
    (save, cancel, delete)
}

/// What the amount field works out to while typing (`show_preview`), in
/// what lands in the stock.
fn raw_preview(state: &StockState) -> Option<(String, bool)> {
    let form = &state.raw_form;
    let raw = form.amount.trim();
    let piece = state.data.is_piece(&form.material);
    let factor = state.raw_factor(&form.material);
    let lang = language::current();
    if raw.is_empty() {
        return None;
    }
    if !stock::is_calculation(raw) {
        if piece {
            return None;
        }
        let value = stock::calculate(raw, 0.0).filter(|v| *v > 0.0)?;
        let number = number_only(lang, value * factor);
        return match stock::unit_hint(value, form.cscu) {
            Some("s_lg_hinweis_scu_gross") => {
                Some((tf("s_mlg_hinweis_scu_gross", &[("menge", Arg::from(number))]), true))
            }
            Some(_) => Some((tf("s_mlg_hinweis_cscu_komma", &[("menge", Arg::from(number))]), true)),
            None if form.cscu => Some((tf("s_lg_wird_zu", &[("0", Arg::from(number))]), false)),
            None => None,
        };
    }
    let index = state.editing_index(Store::Raw);
    let stored = index.and_then(|i| state.raw.get(i)).map_or(0.0, |e| stock::number_of(e, "menge"));
    let value = stock::calculate(raw, stored / factor)? * factor;
    let unit = unit_word(piece);
    Some(if value < 0.0 {
        (tf("s_lg_ergibt_minus", &[("0", Arg::from(stored)), ("1", Arg::from(unit))]), true)
    } else if value == 0.0 {
        (t("s_mlg_ergibt_null"), true)
    } else {
        (tf("s_lg_ergibt", &[("0", Arg::from(stock::py_round(value, 3))), ("1", Arg::from(unit))]), false)
    })
}

fn raw_form(ui: &mut Ui, scheme: &Scheme, state: &mut StockState) {
    let mut enter = false;
    let names = state.data.storable().to_vec();
    let places = state.data.places().to_vec();
    field_label(ui, &t("s_lg_material"));
    let f = &mut state.raw_form;
    enter |= suggest_field(
        ui,
        scheme,
        "lager-rohstoff",
        &t("s_mlg_pl_rohstoff"),
        &mut f.material,
        &names,
        &mut f.errors.name,
    );
    let piece = state.data.is_piece(&state.raw_form.material);
    let label = if piece {
        t("s_lg_menge_stueck")
    } else if state.raw_form.cscu {
        t("s_lg_menge_cscu")
    } else {
        t("s_lg_menge")
    };
    let preview = raw_preview(state);
    crate::layout::columns_or_stack(ui, 220.0, 2, |col, i| {
        let f = &mut state.raw_form;
        if i == 0 {
            field_label(col, &label);
            enter |= plain_field(col, scheme, &mut f.amount, &t("s_pl_lager_menge"), &mut f.errors.amount);
            if !piece {
                widgets::switch_row(col, scheme, &t("s_lg_cscu"), &mut f.cscu);
            }
            if let Some((text, warn)) = &preview {
                let tone = if *warn { scheme.gold } else { scheme.accent };
                col.add(egui::Label::new(RichText::new(text).small().color(color(tone))).wrap());
            }
        } else {
            field_label(col, &t("s_lg_qualitaet"));
            enter |= plain_field(col, scheme, &mut f.quality, &t("s_pl_qualitaet"), &mut f.errors.quality);
        }
    });
    field_label(ui, &t(if state.split.place_required() { "s_ml_ort" } else { "s_lg_ort" }));
    let f = &mut state.raw_form;
    enter |= suggest_field(ui, scheme, "lager-ort", &t("s_mlg_pl_ort"), &mut f.place, &places, &mut f.errors.place);
    let editing = state.editing_index(Store::Raw);
    if editing.is_some() {
        ui.add(egui::Label::new(RichText::new(t("s_mlg_rechnen")).small().color(color(scheme.sub))).wrap());
    }
    if editing.is_none() {
        state.split.show_for(ui, scheme);
    }
    edit_share_buttons(ui, scheme, state);
    let no_place = state.lacks_place(&state.raw_form.place);
    let (save, cancel, delete) = form_buttons(ui, scheme, editing.is_some(), true, no_place);
    form_actions(state, Form::Raw, save || enter, cancel, delete.then_some(editing).flatten().map(|i| (Store::Raw, i)));
}

/// Carries out the buttons of a form.
fn form_actions(state: &mut StockState, form: Form, save: bool, cancel: bool, delete: Option<(Store, usize)>) {
    if save {
        match form {
            Form::Raw => state.save_raw(),
            Form::Trade => state.save_trade(),
            Form::Item => state.save_item(),
        }
    } else if cancel {
        state.stop_editing();
    } else if let Some((store, index)) = delete {
        state.delete(store, index);
    }
}

fn trade_form(ui: &mut Ui, scheme: &Scheme, state: &mut StockState) {
    let mut enter = false;
    let goods = state.data.goods();
    let places = state.data.places().to_vec();
    let required = state.split.place_required();
    let piece = state.editing.as_ref().is_some_and(|e| stock::is_pieces(&e.entry));
    let f = &mut state.trade_form;
    field_label(ui, &t("s_hl_ware"));
    enter |= suggest_field(ui, scheme, "lager-ware", &t("s_mlg_pl_ware"), &mut f.goods, &goods, &mut f.errors.name);
    field_label(ui, &t(if piece { "s_hl_menge_stueck" } else { "s_hl_menge" }));
    enter |= plain_field(ui, scheme, &mut f.amount, &t("s_pl_lager_menge"), &mut f.errors.amount);
    let raw = f.amount.trim().to_string();
    if !raw.is_empty() && stock::is_calculation(&raw) {
        let before = state.editing.as_ref().map_or(0.0, |e| stock::number_of(&e.entry, "menge"));
        let (text, tone) = match stock::calculate(&raw, before) {
            None => (t("s_hl_rechnung_kaputt"), scheme.gold),
            Some(v) if v <= 0.0 => (t("s_mlg_unter_null"), scheme.red_text()),
            Some(v) => (tf("s_hl_ergibt", &[("menge", Arg::from(number_only(language::current(), v)))]), scheme.sub),
        };
        ui.label(RichText::new(text).small().color(color(tone)));
    }
    let f = &mut state.trade_form;
    field_label(ui, &t("s_mlg_einkauf"));
    enter |= plain_field(ui, scheme, &mut f.cost, &t("s_mlg_pl_einkauf"), &mut f.errors.cost);
    field_label(ui, &t(if required { "s_ml_ort" } else { "s_hl_ort" }));
    enter |=
        suggest_field(ui, scheme, "lager-handelsort", &t("s_mlg_pl_ort"), &mut f.place, &places, &mut f.errors.place);
    ui.add_space(4.0);
    widgets::switch_row(ui, scheme, &t("s_hl_gestohlen"), &mut f.stolen);
    let editing = state.editing.as_ref().filter(|e| e.form == Form::Trade).map(|e| e.index);
    if editing.is_none() {
        state.split.show_for(ui, scheme);
    }
    edit_share_buttons(ui, scheme, state);
    let no_place = state.lacks_place(&state.trade_form.place);
    let (save, cancel, delete) = form_buttons(ui, scheme, editing.is_some(), false, no_place);
    form_actions(
        state,
        Form::Trade,
        save || enter,
        cancel,
        delete.then_some(editing).flatten().map(|i| (Store::Trade, i)),
    );
}

fn item_form(ui: &mut Ui, scheme: &Scheme, state: &mut StockState) {
    let mut enter = false;
    let places = state.data.places().to_vec();
    let required = state.split.place_required();
    let f = &mut state.item_form;
    field_label(ui, &t("s_ml_item"));
    enter |= suggest_field(
        ui,
        scheme,
        "lager-item",
        &t("s_mlg_pl_item"),
        &mut f.name,
        state.data.item_names(),
        &mut f.errors.name,
    );
    field_label(ui, &t("s_hl_menge_stueck"));
    enter |= plain_field(ui, scheme, &mut f.amount, &t("s_ml_stueck_h"), &mut f.errors.amount);
    field_label(ui, &t(if required { "s_ml_ort" } else { "s_hl_ort" }));
    enter |= suggest_field(ui, scheme, "lager-itemort", &t("s_mlg_pl_ort"), &mut f.place, &places, &mut f.errors.place);
    ui.add_space(4.0);
    widgets::switch_row(ui, scheme, &t("s_hl_gestohlen"), &mut f.stolen);
    let editing = state.editing.as_ref().filter(|e| e.form == Form::Item).map(|e| e.index);
    if editing.is_none() {
        state.split.show_for(ui, scheme);
    }
    edit_share_buttons(ui, scheme, state);
    let no_place = state.lacks_place(&state.item_form.place);
    let (save, cancel, delete) = form_buttons(ui, scheme, editing.is_some(), false, no_place);
    form_actions(
        state,
        Form::Item,
        save || enter,
        cancel,
        delete.then_some(editing).flatten().map(|i| (Store::Trade, i)),
    );
}

fn refinery_section(ui: &mut Ui, scheme: &Scheme, state: &mut StockState) {
    scan::poll(state);
    let mut open = state.refinery_open;
    let mut book = false;
    if std::mem::take(&mut state.scan.scroll) {
        ui.scroll_to_cursor(Some(Align::Min));
    }
    fold(ui, &t("s_rf_titel"), &mut open, |ui| {
        ui.add(egui::Label::new(RichText::new(t("s_mlg_rf_hilfe")).small().color(color(scheme.sub))).wrap());
        ui.horizontal_wrapped(|row| {
            row.label(RichText::new(t("s_rf_einheit")).color(color(scheme.sub)));
            if widgets::radio(row, scheme, state.refinery_cscu, "cSCU").clicked() {
                state.refinery_cscu = true;
            }
            if widgets::radio(row, scheme, !state.refinery_cscu, "SCU").clicked() {
                state.refinery_cscu = false;
            }
        });
        field_label(ui, &t("s_rf_ort"));
        let places = state.data.places().to_vec();
        let readable =
            !stock::refinery_lines(&state.refinery_text, state.refinery_cscu, &state.data).entries.is_empty();
        let note = state.refinery_place_note(readable).map(t);
        let (enter, field) = suggest_field_marked(
            ui,
            scheme,
            "lager-raffinerieort",
            &t("s_mlg_pl_ort"),
            &mut state.refinery_place,
            &places,
            &mut state.refinery_error,
            note.as_deref(),
        );
        state.refinery_field = Some(field.id);
        // A place in the field ends the question; emptying it again starts it anew.
        if !state.refinery_place.trim().is_empty() {
            state.refinery_asked = false;
        }
        if std::mem::take(&mut state.refinery_focus) {
            field.request_focus();
            field.scroll_to_me(Some(Align::Center));
        }
        ui.add_space(4.0);
        let area = widgets::text_area(&mut state.refinery_text, &t("s_mlg_rf_feld"), scheme)
            .desired_rows(7)
            .desired_width(ui.available_width())
            .font(egui::TextStyle::Monospace);
        ui.add(area);
        let read = stock::refinery_lines(&state.refinery_text, state.refinery_cscu, &state.data);
        let lang = language::current();
        let mut parts = Vec::new();
        for y in read.entries.iter().take(PREVIEW_ENTRIES) {
            let piece = state.data.is_piece(&y.material);
            let key = if piece { "s_rf_wird_zu_stueck" } else { "s_rf_wird_zu" };
            let args = [
                ("0", Arg::from(y.material.as_str())),
                ("1", Arg::from(number_only(lang, y.amount))),
                ("2", Arg::Int(y.quality)),
            ];
            parts.push(tf(key, &args));
        }
        if read.entries.len() > PREVIEW_ENTRIES {
            parts.push("…".to_string());
        }
        let swap = |k: &'static str| match k {
            "s_rf_zu_kurz" => "s_mlg_rf_zu_kurz",
            "s_rf_stueck_ganz" => "s_mlg_rf_stueck_ganz",
            "s_rf_stueck_scu" => "s_mlg_rf_stueck_scu",
            other => other,
        };
        for (line, error) in read.errors.iter().take(PREVIEW_PROBLEMS) {
            parts.push(format!("⚠ {line}: {}", error.text_with(lang, swap)));
        }
        for (line, key, amount) in read.hints.iter().take(PREVIEW_PROBLEMS) {
            let hint_key = match *key {
                "s_lg_hinweis_scu_gross" => "s_mlg_hinweis_scu_gross",
                "s_lg_hinweis_cscu_komma" => "s_mlg_hinweis_cscu_komma",
                _ => "s_mlg_hinweis_stueck",
            };
            parts.push(format!("⚠ {line}: {}", tf(hint_key, &[("menge", Arg::from(number_only(lang, *amount)))])));
        }
        let warn = !read.errors.is_empty() || !read.hints.is_empty();
        let text = if parts.is_empty() { t("s_rf_nichts") } else { parts.join("\n") };
        let tone = if warn { scheme.gold } else { scheme.sub };
        ui.add(egui::Label::new(RichText::new(text).small().color(color(tone))).wrap());
        // "Bildschirm lesen" first and always in its place (as in 4.12), beside it the booking as soon as the
        // field holds something readable.
        ui.add_space(GAP);
        ui.horizontal_wrapped(|row| {
            scan::read_button(row, scheme, state);
            if !read.entries.is_empty() {
                let label = tf("s_rf_knopf", &[("0", Arg::Int(read.entries.len() as i64))]);
                // Never grey: with the place empty the press leads to the field (see `press_refinery`).
                book = row.add(widgets::Button::new(&label, ButtonKind::Strong, scheme)).clicked() || enter;
            }
        });
        scan::notes(ui, scheme, state);
    });
    state.refinery_open = open;
    if book {
        state.press_refinery(ui.ctx());
    }
}

impl StockState {
    /// The hint under the place field of the refinery block while it is empty and there is something to book:
    /// the text key, `None` when the field needs no mark.
    fn refinery_place_note(&self, readable: bool) -> Option<&'static str> {
        if !readable || !self.refinery_place.trim().is_empty() {
            return None;
        }
        Some(if self.split.place_required() {
            "s_rf_ort_leer_pflicht"
        } else if self.refinery_asked {
            "s_rf_ort_leer_nochmal"
        } else {
            "s_rf_ort_leer"
        })
    }

    /// The press of the booking button (or Enter in the place field). With the place empty the press leads to the
    /// field instead of booking: with the stock sync on every time, otherwise the first time; the next press books
    /// without a place.
    fn press_refinery(&mut self, ctx: &egui::Context) {
        let empty = self.refinery_place.trim().is_empty();
        let required = self.split.place_required();
        if empty && (required || !self.refinery_asked) {
            self.refinery_asked = !required;
            self.refinery_error = None;
            self.refinery_focus = true;
            ctx.request_repaint();
            return;
        }
        book_refinery(self);
    }
}

/// Books every readable line as a material entry (`_refinery_box.uebernehmen`).
fn book_refinery(state: &mut StockState) {
    state.refinery_error = None;
    if !state.data.knows_place(state.refinery_place.trim()) {
        state.refinery_error = Some(state.unknown_place_text());
        return;
    }
    if state.lacks_place(&state.refinery_place) {
        state.refinery_error = Some(t("s_ml_ort_fehlt"));
        return;
    }
    state.reload();
    let typed = state.refinery_place.trim().to_string();
    let place = state.data.official_place(&typed).unwrap_or(typed);
    state.counted.push("lager_raffinerie");
    let read = stock::refinery_lines(&state.refinery_text, state.refinery_cscu, &state.data);
    for y in &read.entries {
        stock::raw_add(&mut state.raw, &state.data, &y.material, y.amount, Some(y.quality), &place);
    }
    if state.write(Store::Raw) {
        for y in &read.entries {
            state.split.booked(Store::Raw, &y.material, &place, Some(y.quality), false);
        }
        let typed: Vec<(String, f64, i64)> = read.entries.iter().map(|y| (y.material.clone(), y.amount, y.quality)).collect();
        scan::learn_from_booking(state, &typed);
        // A place that was left empty remembers nothing: the field starts on the last real one.
        if !place.is_empty() {
            state.refinery_last = place.clone();
            state.refinery_place = place;
        }
        state.refinery_asked = false;
        let left: Vec<String> = read.errors.iter().map(|(line, _)| line.clone()).collect();
        state.refinery_text = left.join("\n");
        state.say(tf("s_rf_fertig", &[("0", Arg::Int(read.entries.len() as i64))]));
    }
}

/// Tabs with counts and the rare actions beside them (`_draw_controls`, first half).
fn tabs(ui: &mut Ui, scheme: &Scheme, state: &mut StockState, rows: &[Row]) {
    let materials = rows.iter().filter(|r| !r.item).count();
    let items = rows.len() - materials;
    let tabs = [format!("{} ({materials})", t("s_ml_material")), format!("{} ({items})", t("s_ml_items"))];
    let mut chosen = usize::from(state.items);
    let words =
        [t("s_mlg_aktion_sichern"), t("s_mlg_aktion_einlesen"), t("s_mlg_aktion_umziehen"), t("s_mlg_aktion_leeren")];
    let actions = [
        widgets::PageAction { icon: "download", text: &words[0], on: state.panel == Some(Panel::Backup) },
        widgets::PageAction {
            icon: "folder",
            text: &words[1],
            on: state.dialog.as_ref().is_some_and(|(_, j)| *j == Job::Import),
        },
        widgets::PageAction { icon: "house", text: &words[2], on: state.panel == Some(Panel::Move) },
        widgets::PageAction { icon: "trash-2", text: &words[3], on: state.panel == Some(Panel::Clear) },
    ];
    let (changed, action) =
        widgets::tab_bar_with_actions(ui, scheme, &[tabs[0].as_str(), tabs[1].as_str()], &mut chosen, &actions);
    if changed {
        state.items = chosen == 1;
        state.kind = None;
    }
    match action {
        Some(0) => state.toggle_panel(Panel::Backup),
        Some(1) => state.ask_file(ui.ctx(), Job::Import),
        Some(2) => state.toggle_panel(Panel::Move),
        Some(3) => state.toggle_panel(Panel::Clear),
        _ => {}
    }
    action_panel(ui, scheme, state);
}

/// Filters and the search of the list (`_draw_controls`, second half); they narrow the suggestions of the log too.
fn filters(ui: &mut Ui, scheme: &Scheme, state: &mut StockState, rows: &[Row]) {
    let here: Vec<&Row> = rows.iter().filter(|r| r.item == state.items).collect();
    let present: HashSet<Kind> = here.iter().map(|r| state.kind_of(r)).collect();
    let kinds: Vec<Kind> = (if state.items { &ITEM_KINDS[..] } else { &MATERIAL_KINDS[..] })
        .iter()
        .copied()
        .filter(|k| present.contains(k))
        .collect();
    if state.kind.is_some_and(|k| !kinds.contains(&k)) {
        state.kind = None;
    }
    let places = places_of(here.iter().copied());
    if !state.place.is_empty() && !places.contains(&state.place) {
        state.place.clear();
    }
    let kind_words: Vec<String> =
        std::iter::once(t("s_ml_alle_arten")).chain(kinds.iter().map(|k| t(&k.text_key()))).collect();
    let place_words: Vec<String> = std::iter::once(t("s_ml_alle_orte")).chain(places.iter().cloned()).collect();
    let purpose_words = [t("s_ml_alle_zwecke"), t("s_ml_herstellen"), t("s_ml_verkaufen")];
    let kind_refs: Vec<&str> = kind_words.iter().map(String::as_str).collect();
    let place_refs: Vec<&str> = place_words.iter().map(String::as_str).collect();
    let purpose_refs: Vec<&str> = purpose_words.iter().map(String::as_str).collect();
    // With a connection the first filter is the view: all, personal, shared.
    let scope_words = state.split.scope_words();
    let scope_refs: Vec<&str> = scope_words.iter().map(String::as_str).collect();
    let lead = usize::from(state.split.available());
    let mut chosen = Vec::new();
    let mut filters: Vec<&[&str]> = Vec::new();
    if lead == 1 {
        chosen.push(state.split.scope_index());
        filters.push(&scope_refs);
    }
    chosen.push(state.kind.and_then(|k| kinds.iter().position(|x| *x == k)).map_or(0, |p| p + 1));
    chosen.push(places.iter().position(|p| *p == state.place).map_or(0, |p| p + 1));
    filters.push(&kind_refs);
    filters.push(&place_refs);
    if !state.items {
        filters.push(&purpose_refs);
        chosen.push(state.purpose);
    }
    if widgets::filter_bar(ui, "bestand", &filters, &mut chosen) {
        if lead == 1 {
            state.split.set_scope_index(chosen[0]);
        }
        state.kind = chosen[lead].checked_sub(1).and_then(|i| kinds.get(i).copied());
        state.place = chosen[lead + 1].checked_sub(1).and_then(|i| places.get(i).cloned()).unwrap_or_default();
        if !state.items {
            state.purpose = chosen[lead + 2];
        }
    }
    ui.add_space(4.0);
    let width = ui.available_width();
    widgets::search_field(ui, &mut state.search, &t("s_mlg_suche"), scheme, width);
    let search = lower(state.search.trim());
    if state.closed_for != search {
        state.closed.clear();
        state.closed_for = search;
    }
}

/// How the table is laid out at this width.
struct Table {
    columns: Vec<Cell>,
    widths: Vec<f32>,
    compact: bool,
    selling: bool,
    /// Width of the row actions.
    actions: f32,
}

fn head_of(cell: &Cell) -> String {
    match cell {
        Cell::Place => t("s_ml_sp_ort"),
        Cell::Quality => t("s_ml_sp_q"),
        Cell::Amount => t("s_ml_sp_menge"),
        Cell::Paid => t("s_lj_bezahlt"),
        Cell::Price => t("s_hl_sp_je_scu"),
        Cell::Total => t("s_hl_sp_gesamt"),
        Cell::Profit => t("s_hl_sp_gewinn"),
    }
}

fn sortable(cell: &Cell) -> Option<Column> {
    match cell {
        Cell::Place => Some(Column::Place),
        Cell::Quality => Some(Column::Quality),
        Cell::Amount => Some(Column::Amount),
        _ => None,
    }
}

fn list(ui: &mut Ui, scheme: &Scheme, state: &mut StockState, rows: &[Row], lang: Lang) {
    let shown = visible(state, rows);
    if rows.is_empty() {
        widgets::empty_state(ui, scheme, "package", &t("s_ml_leer"));
        return;
    }
    if shown.is_empty() && state.items && !rows.iter().any(|r| r.item) {
        widgets::empty_state(ui, scheme, "package", &t("s_mlg_keine_items"));
        return;
    }
    if shown.is_empty() {
        widgets::empty_state(ui, scheme, "package", &t("s_lg_nichts_da"));
        return;
    }
    let selling = !state.items && state.purpose == 2;
    let mut columns = vec![Cell::Place];
    if !state.items && !selling {
        columns.push(Cell::Quality);
    }
    columns.push(Cell::Amount);
    if state.items && shown.iter().any(|r| r.cost.is_some()) {
        columns.push(Cell::Paid);
    }
    if selling {
        columns.extend([Cell::Price, Cell::Total, Cell::Profit]);
    }
    let all = lines(state, &shown);
    let texts: Vec<Vec<(String, Color)>> = all.iter().map(|l| cells(state, scheme, lang, l, &columns)).collect();
    let widths: Vec<f32> = columns
        .iter()
        .enumerate()
        .map(|(c, cell)| {
            let longest = texts.iter().map(|row| text_width(ui, &row[c].0, false)).fold(0.0, f32::max);
            longest.max(text_width(ui, &head_of(cell), true) + 18.0) + 16.0
        })
        .collect();
    let available = ui.available_width() - crate::layout::SAFETY - 2.0 * FRAME_INSET;
    let actions = if state.split.available() { ACTIONS_WIDTH_SPLIT } else { ACTIONS_WIDTH };
    let compact = available - widths.iter().sum::<f32>() - actions - 12.0 < NAME_MIN;
    let table = Table { columns, widths, compact, selling, actions };

    let sort_click = heads(ui, scheme, state, &table, available);
    if let Some(column) = sort_click {
        if state.sort == column {
            state.descending = !state.descending;
        } else {
            state.sort = column;
            state.descending = false;
        }
    }

    let text_height = ui.text_style_height(&egui::TextStyle::Body);
    let small_height = ui.text_style_height(&egui::TextStyle::Small);
    let row_height = if compact {
        text_height + 2.0 * small_height + 14.0
    } else {
        (text_height + 10.0).max(widgets::ICON_BUTTON_SIZE + 4.0)
    };
    let total_height = row_height * all.len() as f32;
    let left = ui.clip_rect().bottom() - ui.cursor().min.y - 12.0;
    let room = crate::layout::list_room(ui, row_height);
    let visible = ui.clip_rect().height();
    let editing = state.editing.as_ref().map(|e| (e.store, e.index));
    let now = ui.input(|i| i.time);
    let mut act = None;
    let drawn = widgets::list_frame_marked(ui, scheme, |frame| {
        frame.set_width(frame.available_width() - crate::layout::SAFETY);
        let area = egui::ScrollArea::vertical()
            .id_salt("lager-liste")
            .max_height(room)
            .min_scrolled_height(room)
            .auto_shrink([false, true]);
        area.show_viewport(frame, |rows_ui, viewport| {
            rows_ui.set_min_height(total_height);
            let origin = rows_ui.min_rect().min;
            let width = rows_ui.available_width();
            let first = (viewport.min.y / row_height).floor().max(0.0) as usize;
            for (n, line) in all.iter().enumerate().skip(first) {
                let top = row_height * n as f32;
                if top > viewport.max.y {
                    break;
                }
                let rect = Rect::from_min_size(origin + vec2(0.0, top), vec2(width, row_height));
                let post = match line {
                    Line::Post(row, _) => Some((row.store, row.index)),
                    Line::Group(..) => None,
                };
                if post.is_some() && post == editing {
                    // The entry in the form: like a chosen list row.
                    rows_ui.painter().rect_filled(rect, 0.0, color(scheme.accent_dark));
                    let bar = Rect::from_min_size(rect.min, vec2(widgets::LIST_BAR_WIDTH, rect.height()));
                    rows_ui.painter().rect_filled(bar, 0.0, color(scheme.accent));
                } else if rows_ui.rect_contains_pointer(rect) {
                    rows_ui.painter().rect_filled(rect, 0.0, color(scheme.hover));
                } else if n % 2 == 1 {
                    rows_ui.painter().rect_filled(rect, 0.0, color(scheme.bar));
                }
                if let Some(a) = draw_line(rows_ui, scheme, state, line, &texts[n], &table, rect) {
                    act = Some(a);
                }
            }
        })
        .inner_rect
        .height()
    })
    .inner;
    if cfg!(test) {
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique(LIST_ROOM_ID), (left, room, visible, drawn, row_height)));
    }
    summaries(ui, scheme, state, &shown, lang);
    match act {
        Some(Act::Edit(store, index)) => state.edit(store, index),
        Some(Act::Share(store, index, kind)) => {
            if let Some(row) = rows.iter().find(|r| (r.store, r.index) == (store, index)) {
                state.split.open(row, kind);
            }
        }
        Some(Act::Delete(store, index)) => {
            let just_deleted = state
                .message
                .as_ref()
                .is_some_and(|m| m.undo.is_some() && m.since.is_none_or(|s| now - s < DELETE_PAUSE));
            if !just_deleted {
                state.delete(store, index);
            }
        }
        Some(Act::Toggle(key, shown)) => {
            if shown {
                state.open.remove(&key);
                state.closed.insert(key);
            } else {
                state.closed.remove(&key);
                state.open.insert(key);
            }
        }
        None => {}
    }
}

/// The column heads; a click sorts, a second click turns the direction.
fn heads(ui: &mut Ui, scheme: &Scheme, state: &StockState, table: &Table, available: f32) -> Option<Column> {
    let mut sort_click = None;
    let sorted = |column: Column| -> Option<bool> { (state.sort == column).then_some(!state.descending) };
    if table.compact {
        ui.horizontal_wrapped(|row| {
            if widgets::sort_head(row, scheme, &t("s_ml_sp_bestand"), sorted(Column::Name), false) {
                sort_click = Some(Column::Name);
            }
            for cell in &table.columns {
                if let Some(column) = sortable(cell)
                    && widgets::sort_head(row, scheme, &head_of(cell), sorted(column), false)
                {
                    sort_click = Some(column);
                }
            }
        });
        return sort_click;
    }
    let height = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
    let (rect, _) = ui.allocate_exact_size(vec2(available + 2.0 * FRAME_INSET, height), Sense::hover());
    // Same inner edges as the rows inside the list frame.
    let inner = rect.shrink2(vec2(FRAME_INSET + 6.0, 0.0));
    let mut x = inner.max.x - table.actions;
    let mut spots = Vec::new();
    for (c, cell) in table.columns.iter().enumerate().rev() {
        x -= table.widths[c];
        spots.push((Rect::from_min_size(egui::pos2(x, rect.min.y), vec2(table.widths[c] - 8.0, height)), Some(*cell)));
    }
    spots.push((Rect::from_min_max(egui::pos2(inner.min.x, rect.min.y), egui::pos2(x, rect.max.y)), None));
    for (spot, cell) in spots {
        let mut child = ui.new_child(UiBuilder::new().max_rect(spot).layout(Layout::left_to_right(Align::Center)));
        match cell {
            None => {
                if widgets::sort_head(&mut child, scheme, &t("s_ml_sp_bestand"), sorted(Column::Name), false) {
                    sort_click = Some(Column::Name);
                }
            }
            Some(cell) => match sortable(&cell) {
                Some(column) => {
                    let numbers = cell != Cell::Place;
                    if widgets::sort_head(&mut child, scheme, &head_of(&cell), sorted(column), numbers) {
                        sort_click = Some(column);
                    }
                }
                None => {
                    child.with_layout(Layout::right_to_left(Align::Center), |c| {
                        c.label(RichText::new(head_of(&cell)).strong())
                    });
                }
            },
        }
    }
    sort_click
}

fn draw_line(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &StockState,
    line: &Line,
    texts: &[(String, Color)],
    table: &Table,
    rect: Rect,
) -> Option<Act> {
    let (name, id, nested) = match line {
        Line::Group(key, rows, _) => {
            (format!("{} ({})", rows[0].name, rows.len()), ui.make_persistent_id(("lager-gruppe", key.clone())), false)
        }
        Line::Post(row, nested) => {
            (row.name.clone(), ui.make_persistent_id(("lager-posten", row.store == Store::Raw, row.index)), *nested)
        }
    };
    let marks = badges_of(state, scheme, line, table.selling);
    let (tag, extra) = name_extras(state, line);
    let response = ui.interact(rect, id, Sense::click());
    // A row the Basetool refused says on hover what it answered.
    let refusal = match line {
        Line::Post(row, _) => state.split.refusal_text(row),
        Line::Group(..) => None,
    };
    if let Some(text) = &refusal {
        response.clone().on_hover_text(text);
    }
    let mut spoken: Vec<&str> = vec![name.as_str()];
    spoken.extend(tag.as_deref());
    spoken.extend(marks.iter().map(|(m, _)| m.as_str()));
    spoken.extend(Some(extra.as_str()).filter(|e| !e.is_empty()));
    spoken.extend(refusal.as_deref());
    spoken.extend(texts.iter().map(|(t, _)| t.as_str()).filter(|t| !t.is_empty()));
    let state_word = match line {
        Line::Group(_, _, true) => Some(t("s_mlg_offen")),
        Line::Group(_, _, false) => Some(t("s_mlg_zu")),
        Line::Post(..) => None,
    };
    if let Some(word) = &state_word {
        spoken.push(word.as_str());
    }
    let spoken = spoken.join(", ");
    response.widget_info(|| WidgetInfo::labeled(Role::Row, true, &spoken));
    let hovered = ui.rect_contains_pointer(rect);
    let inner = rect.shrink2(vec2(6.0, 2.0));
    let actions = Rect::from_min_max(egui::pos2(inner.max.x - table.actions, inner.min.y), inner.max);
    let indent = if nested { 24.0 } else { 0.0 };
    let line_height = if table.compact { (inner.height() - 4.0) / 3.0 } else { inner.height() };

    // Name and badges; long names end with an ellipsis and show in full on hover.
    let name_right = if table.compact { actions.min.x } else { actions.min.x - table.widths.iter().sum::<f32>() };
    let name_rect = Rect::from_min_max(inner.min, egui::pos2(name_right, inner.min.y + line_height + 2.0));
    let mut cell = ui.new_child(UiBuilder::new().max_rect(name_rect).layout(Layout::left_to_right(Align::Center)));
    cell.set_clip_rect(name_rect.intersect(ui.clip_rect()));
    cell.add_space(indent);
    let tag_width = tag.as_ref().map_or(0.0, |tag| text_width(&cell, tag, false) * 0.85 + 8.0);
    // As many badges as fit beside the name and the tag; the others go into the `+n` badge.
    let badge_width = |m: &str| text_width(&cell, m, false) * 0.85 + 16.0;
    let name_least = if nested { 0.0 } else { (text_width(&cell, &name, true) + 4.0).min(90.0) };
    let full = (0..=marks.len().min(2))
        .rev()
        .find(|&k| {
            let shown: f32 = marks.iter().take(k).map(|(m, _)| badge_width(m)).sum();
            let rest = if marks.len() > k { 30.0 } else { 0.0 };
            name_least + tag_width + shown + rest <= cell.available_width()
        })
        .unwrap_or(0);
    let marks_width: f32 = marks.iter().take(full).map(|(m, _)| badge_width(m)).sum::<f32>()
        + if marks.len() > full { 30.0 } else { 0.0 };
    if !nested {
        let room = (cell.available_width() - marks_width - tag_width).max(40.0);
        cell.scope(|c| {
            c.set_max_width(room);
            c.add(egui::Label::new(RichText::new(&name).strong()).truncate());
        });
    }
    // The tag right behind the name, how it is mined after the badges; the latter gives way first.
    if let Some(tag) = &tag {
        cell.label(RichText::new(tag).small().color(color(scheme.sub)));
    }
    let refs: Vec<(&str, Color)> = marks.iter().map(|(m, c)| (m.as_str(), *c)).collect();
    widgets::badges_within(&mut cell, scheme, &refs, full);
    if !extra.is_empty() {
        cell.add(egui::Label::new(RichText::new(&extra).small().color(color(scheme.sub))).truncate());
    }

    if table.compact {
        // Place, quality and amount on the second line, prices on the third.
        let split = texts.len().min(if table.selling { 2 } else { texts.len() });
        for (k, part) in [&texts[..split], &texts[split..]].into_iter().enumerate() {
            if part.is_empty() {
                continue;
            }
            let top = name_rect.max.y + k as f32 * line_height;
            let spot =
                Rect::from_min_max(egui::pos2(inner.min.x + indent, top), egui::pos2(actions.min.x, top + line_height));
            let mut sub = ui.new_child(UiBuilder::new().max_rect(spot).layout(Layout::left_to_right(Align::Center)));
            let font = egui::TextStyle::Small.resolve(sub.style());
            let mut job = egui::text::LayoutJob::default();
            for (text, tone) in part.iter().filter(|(text, _)| !text.is_empty()) {
                if !job.text.is_empty() {
                    job.append("  ·  ", 0.0, egui::TextFormat::simple(font.clone(), color(scheme.sub)));
                }
                job.append(text, 0.0, egui::TextFormat::simple(font.clone(), color(*tone)));
            }
            sub.add(egui::Label::new(job).truncate());
        }
    } else {
        let mut x = actions.min.x;
        for (c, (text, tone)) in texts.iter().enumerate().rev() {
            x -= table.widths[c];
            let spot = Rect::from_min_size(egui::pos2(x, inner.min.y), vec2(table.widths[c] - 8.0, inner.height()));
            // The place stands left, numbers right.
            let layout =
                if c > 0 { Layout::right_to_left(Align::Center) } else { Layout::left_to_right(Align::Center) };
            let mut child = ui.new_child(UiBuilder::new().max_rect(spot).layout(layout));
            child.add(egui::Label::new(RichText::new(text).color(color(*tone))).truncate());
        }
    }

    let mut buttons = ui.new_child(UiBuilder::new().max_rect(actions).layout(Layout::right_to_left(Align::Center)));
    match line {
        Line::Group(key, _, shown) => {
            let icon = if *shown { "chevron-down" } else { "chevron-right" };
            let tint = if hovered { IconTint::Hover } else { IconTint::Normal };
            if let Some(image) = crate::icons::themed(icon, scheme, tint, 16.0) {
                buttons.add(image);
            }
            if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                return Some(Act::Toggle(key.clone(), *shown));
            }
            None
        }
        Line::Post(row, _) => {
            let mut act = None;
            // Of a part of the lot only sharing and taking back count; edit and delete are for the whole row.
            let whole = state.split.whole(row);
            let pending = state.split.pending(row);
            let missing = state.split.share_missing(row);
            if hovered && whole {
                if named_icon(&mut buttons, scheme, "trash-2", &t("s_lg_weg")).clicked() {
                    act = Some(Act::Delete(row.store, row.index));
                }
                if named_icon(&mut buttons, scheme, "pencil", &t("s_ml_bearbeiten")).clicked() {
                    act = Some(Act::Edit(row.store, row.index));
                }
            } else if (pending || missing.is_some()) && whole {
                // The place of the edit and delete buttons, which show on hover.
                buttons.add_space(2.0 * (widgets::ICON_BUTTON_SIZE + buttons.spacing().item_spacing.x));
            }
            if hovered {
                for kind in state.split.actions(row).into_iter().rev() {
                    let (icon, key) = match kind {
                        ShareKind::Share => ("users", "s_ml_teilen"),
                        ShareKind::Unshare => ("undo-2", "s_ml_zuruecknehmen"),
                    };
                    if named_icon(&mut buttons, scheme, icon, &t(key)).clicked() {
                        act = Some(Act::Share(row.store, row.index, kind));
                    }
                }
            }
            // Sharing waits for the lot at the Basetool: the button is there from the start, greyed, and says so.
            if pending {
                widgets::icon_button_blocked(&mut buttons, scheme, "users", &t("s_ml_abgleich"));
            }
            // Without the button the row still says why it cannot go into the unit's storage.
            if let Some(reason) = &missing {
                widgets::icon_button_blocked(&mut buttons, scheme, "users", reason);
            }
            if act.is_none() && whole && response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                act = Some(Act::Edit(row.store, row.index));
            }
            act
        }
    }
}

/// Sale value and profit under the list, only for goods for sale.
fn summaries(ui: &mut Ui, scheme: &Scheme, state: &StockState, shown: &[Row], lang: Lang) {
    let trade: Vec<Row> = shown.iter().filter(|r| r.store == Store::Trade && !r.item).cloned().collect();
    if trade.is_empty() {
        return;
    }
    let total: f64 = trade.iter().map(|r| state.data.best_price(&r.name) * r.amount).sum();
    let profit = profit_of(&trade, &state.data);
    let right = |ui: &mut Ui, text: String, tone: Color| {
        ui.with_layout(Layout::right_to_left(Align::Min), |r| {
            r.add(egui::Label::new(RichText::new(text).small().color(color(tone))).wrap());
        });
    };
    if total != 0.0 {
        right(ui, tf("s_hl_gesamt", &[("summe", Arg::from(formats::money(lang, total)))]), scheme.accent);
    }
    if let Some(p) = profit {
        let sign = if p > 0.0 { "+" } else { "" };
        let tone = if p >= 0.0 { scheme.accent } else { scheme.red_text() };
        let sum = format!("{sign}{}", formats::money(lang, p));
        right(ui, tf("s_hl_gewinn_gesamt", &[("summe", Arg::from(sum))]), tone);
    }
    if total != 0.0 || profit.is_some() {
        right(ui, t("s_hl_gesamt_hinweis"), scheme.sub);
    }
}

/// The panel of the open action, framed under the tab row: writing each
/// part out, or clearing it after a question in the row.
fn action_panel(ui: &mut Ui, scheme: &Scheme, state: &mut StockState) {
    let Some(panel) = state.panel else { return };
    if panel == Panel::Move {
        ui.add_space(4.0);
        widgets::list_frame(ui, scheme, |ui| {
            ui.set_width(ui.available_width() - crate::layout::SAFETY);
            relocation::show(ui, scheme, state);
        });
        return;
    }
    let mut run = None;
    let mut ask = None;
    let busy = state.dialog.is_some();
    let open_text = t("s_mlg_dialog_offen");
    let blocked = busy.then_some(open_text.as_str());
    ui.add_space(4.0);
    widgets::list_frame(ui, scheme, |ui| {
        ui.set_width(ui.available_width() - crate::layout::SAFETY);
        for (store, label, parts) in [
            (Store::Raw, "s_ml_teil_herstellen", &[(Part::Raw, "s_lg_leeren")][..]),
            (
                Store::Trade,
                "s_mlg_teil_verkaufen",
                &[(Part::Goods, "s_ml_ware_leeren"), (Part::Items, "s_ml_items_leeren")][..],
            ),
        ] {
            ui.label(RichText::new(t(label)).small().color(color(scheme.sub)));
            ui.horizontal_wrapped(|row| match panel {
                Panel::Backup => {
                    for (csv, key) in [(false, "s_lg_aus_json"), (true, "s_lg_aus_csv")] {
                        let word = t(key);
                        let button =
                            widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("download").blocked(blocked);
                        if row.add(button).clicked() && !busy {
                            ask = Some(Job::Export(store, csv));
                        }
                    }
                }
                Panel::Clear => {
                    for (part, key) in parts {
                        let count = state.count(*part);
                        if state.confirm == Some(*part) {
                            let question = tf("s_mlg_frage", &[("n", Arg::from(count))]);
                            row.label(RichText::new(question).color(color(scheme.red_text())));
                            let sure = t("s_mlg_bestaetigen");
                            if row.add(widgets::Button::new(&sure, ButtonKind::Danger, scheme)).clicked() {
                                run = Some(*part);
                            }
                            if row.add(widgets::Button::new(&t("s_lg_abbrechen"), ButtonKind::Normal, scheme)).clicked()
                            {
                                state.confirm = None;
                            }
                        } else {
                            let empty = if *part == Part::Items { t("s_mlg_keine_items") } else { t("s_ml_leer") };
                            let word = t(key);
                            let button = widgets::Button::new(&word, ButtonKind::Danger, scheme)
                                .blocked((count == 0).then_some(empty.as_str()));
                            if row.add(button).clicked() {
                                state.confirm = Some(*part);
                            }
                        }
                    }
                }
                // Shown by its own module above.
                Panel::Move => {}
            });
            ui.add_space(4.0);
        }
        if panel == Panel::Backup {
            ui.add(egui::Label::new(RichText::new(t("s_mlg_sichern_hilfe")).small().color(color(scheme.sub))).wrap());
        }
        if panel == Panel::Clear {
            ui.add(egui::Label::new(RichText::new(t("s_mlg_patch_hilfe")).small().color(color(scheme.sub))).wrap());
        }
    });
    if let Some(job) = ask {
        state.ask_file(ui.ctx(), job);
    }
    if let Some(part) = run {
        state.confirm = None;
        state.clear(part);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows_named(state: &StockState) -> Vec<String> {
        let rows = state.rows();
        let shown = visible(state, &rows);
        lines(state, &shown)
            .iter()
            .map(|l| match l {
                Line::Group(_, rows, _) => format!("{} ({})", rows[0].name, rows.len()),
                Line::Post(row, nested) => format!("{}{}", if *nested { "  " } else { "" }, row.name),
            })
            .collect()
    }

    /// A scratch folder with both stock files, written like 4.12.
    fn with_scratch(name: &str, raw: &str, trade: &str) -> (StockState, String) {
        let folder = std::env::temp_dir().join(format!("versekit-lager-{name}-{}", versekit_core::testdir::run_tag()));
        let folder = folder.to_string_lossy().into_owned();
        jsonio::make_dirs(&folder).expect("Ordner");
        let raw_path = format!("{folder}/{}", stock::RAW_FILE);
        let trade_path = format!("{folder}/{}", stock::TRADE_FILE);
        jsonio::write_text_atomic(&raw_path, raw, jsonio::Newline::Native).expect("rohstoffe");
        jsonio::write_text_atomic(&trade_path, trade, jsonio::Newline::Native).expect("handelslager");
        let mut state = StockState { files: Some((raw_path, trade_path)), ..StockState::default() };
        state.reload();
        (state, folder)
    }

    /// A data folder with the stock files of 4.12 and the settings given.
    fn moved_world(name: &str, raw: &str, settings: &str) -> (Paths, String) {
        let home = std::env::temp_dir().join(format!("versekit-umzug-{name}-{}", versekit_core::testdir::run_tag()));
        let home = home.to_string_lossy().replace('\\', "/");
        let _ = std::fs::remove_dir_all(&home);
        jsonio::make_dirs(&home).expect("folder");
        let paths = Paths::new(versekit_core::paths::Host::new(
            versekit_core::pypath::Flavor::Posix,
            &[("SC_BP_HOME", home.as_str()), ("HOME", home.as_str())],
            &home,
        ));
        let write = |file: &str, text: &str| {
            jsonio::write_text_atomic(&paths.app_file(file), text, jsonio::Newline::Native).expect(file);
        };
        write(stock::RAW_FILE, raw);
        write(stock::TRADE_FILE, "{\"posten\": []}");
        write("einstellungen.json", settings);
        (paths, home)
    }

    /// The texts and the named controls of a few frames of the page.
    fn moved_frames(
        ctx: &egui::Context,
        state: &mut StockState,
        click: Option<egui::Pos2>,
    ) -> (Vec<String>, Vec<(String, egui::Rect)>) {
        let scheme = versekit_core::theme::by_id("original").expect("original");
        let mut steps: Vec<Vec<egui::Event>> = vec![vec![], vec![]];
        if let Some(at) = click {
            let button = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            steps.extend([vec![egui::Event::PointerMoved(at)], vec![button(true)], vec![button(false)], vec![]]);
        }
        let (mut texts, mut nodes) = (Vec::new(), Vec::new());
        for events in steps {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 1400.0))),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| show_page(ui, scheme, state));
            output.textures_delta.clear();
            texts.clear();
            fn collect(shape: &egui::Shape, into: &mut Vec<String>) {
                match shape {
                    egui::Shape::Text(text) => into.push(text.galley.text().to_string()),
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, into)),
                    _ => {}
                }
            }
            output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
            nodes = output
                .platform_output
                .accesskit_update
                .map(|update| {
                    update
                        .nodes
                        .iter()
                        .filter_map(|(_, node)| {
                            let b = node.bounds()?;
                            let rect = egui::Rect::from_min_max(
                                egui::pos2(b.x0 as f32, b.y0 as f32),
                                egui::pos2(b.x1 as f32, b.y1 as f32),
                            );
                            Some((node.label().unwrap_or_default().to_string(), rect))
                        })
                        .collect()
                })
                .unwrap_or_default();
        }
        (texts, nodes)
    }

    #[test]
    fn whoever_has_entries_sees_once_where_both_stores_went_and_the_cross_hides_it_for_good() {
        let _language = crate::language_lock();
        language::set_thread_language(language::Choice::De);
        let title = t("s_ml_umzug_titel");
        let (paths, home) = moved_world("einmal", SAMPLE_RAW, "{}");
        let mut state = StockState::with_files(&paths);
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        let (texts, nodes) = moved_frames(&ctx, &mut state, None);
        assert!(texts.contains(&title), "{texts:?}");
        let cross = nodes
            .iter()
            .find(|(label, _)| *label == t("s_ml_umzug_weg"))
            .map(|(_, rect)| rect.center())
            .unwrap_or_else(|| panic!("no cross: {nodes:?}"));
        let (texts, _) = moved_frames(&ctx, &mut state, Some(cross));
        assert!(!texts.contains(&title), "gone after the cross");
        assert!(
            state.setting_values().contains(&(SETTING_MOVED_SEEN, Value::Bool(true))),
            "kept in the settings: {:?}",
            state.setting_values()
        );
        let _ = std::fs::remove_dir_all(&home);

        // Already seen, or nothing entered yet: no card.
        for (name, raw, settings) in [
            ("gesehen", SAMPLE_RAW, "{\"lager_umzug_gesehen\": true}"),
            ("leer", "{\"posten\": []}", "{}"),
        ] {
            let (paths, home) = moved_world(name, raw, settings);
            let mut state = StockState::with_files(&paths);
            let (texts, _) = moved_frames(&ctx, &mut state, None);
            assert!(!texts.contains(&title), "{name}: {texts:?}");
            let _ = std::fs::remove_dir_all(&home);
        }

        // Nothing entered and none of the old stores opened: reading writes nothing, the page shown notes it, so
        // stock that comes later brings no card.
        let (paths, home) = moved_world("neu", "{\"posten\": []}", "{}");
        let mut state = StockState::with_files(&paths);
        let seen = (SETTING_MOVED_SEEN, Value::Bool(true));
        assert!(!state.setting_values().contains(&seen), "nothing noted before the page is shown");
        assert!(!paths.setting_bool(SETTING_MOVED_SEEN, false), "reading writes nothing");
        moved_frames(&ctx, &mut state, None);
        assert!(state.setting_values().contains(&seen), "noted for a fresh install: {:?}", state.setting_values());
        // As the window writes the changed setting.
        assert!(paths.set_setting(SETTING_MOVED_SEEN, Value::Bool(true)));
        jsonio::write_text_atomic(&paths.app_file(stock::RAW_FILE), SAMPLE_RAW, jsonio::Newline::Native).expect("raw");
        let mut state = StockState::with_files(&paths);
        let (texts, _) = moved_frames(&ctx, &mut state, None);
        assert!(!texts.contains(&title), "stock that came later: {texts:?}");
        let _ = std::fs::remove_dir_all(&home);

        // Nothing entered, but one of the old stores was open once: the card, and nothing noted before the cross.
        for area in ["lager", "handelslager"] {
            let (paths, home) = moved_world(area, "{\"posten\": []}", "{}");
            let seen = format!("{{\"zuletzt\": \"4.12.0\", \"bereiche\": {{\"{area}\": \"4.9.0\"}}}}");
            let file = paths.app_file(versekit_core::gallery::SEEN_FILE);
            jsonio::write_text_atomic(&file, &seen, jsonio::Newline::Native).expect("seen");
            let mut state = StockState::with_files(&paths);
            let (texts, _) = moved_frames(&ctx, &mut state, None);
            assert!(texts.contains(&title), "{area}: {texts:?}");
            assert!(!paths.setting_bool(SETTING_MOVED_SEEN, false), "{area}");
            let _ = std::fs::remove_dir_all(&home);
        }
    }

    /// The card about where both stores went keeps its cross inside the page and beside the title, also so
    /// narrow that the title has to wrap, in both languages.
    #[test]
    fn the_cross_of_the_card_stays_in_the_page_beside_the_title_at_every_width() {
        let _language = crate::language_lock();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        let scheme = versekit_core::theme::by_id("original").expect("original");
        let (paths, home) = moved_world("schmal", SAMPLE_RAW, "{}");
        for choice in [language::Choice::De, language::Choice::En] {
            language::set_thread_language(choice);
            let (title, cross) = (t("s_ml_umzug_titel"), t("s_ml_umzug_weg"));
            for width in [240.0, 420.0, 1000.0] {
                let mut state = StockState::with_files(&paths);
                let (mut title_rect, mut cross_rect) = (None, None);
                for _ in 0..3 {
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 1400.0))),
                        ..Default::default()
                    };
                    let mut output = ctx.run_ui(input, |ui| show_page(ui, scheme, &mut state));
                    output.textures_delta.clear();
                    fn find(shape: &egui::Shape, text: &str) -> Option<egui::Rect> {
                        match shape {
                            egui::Shape::Text(shown) if shown.galley.text() == text => {
                                Some(egui::Rect::from_min_size(shown.pos, shown.galley.size()))
                            }
                            egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, text)),
                            _ => None,
                        }
                    }
                    title_rect = output.shapes.iter().find_map(|clipped| find(&clipped.shape, &title));
                    cross_rect = output.platform_output.accesskit_update.and_then(|update| {
                        update.nodes.iter().find(|(_, node)| node.label() == Some(cross.as_str())).and_then(
                            |(_, node)| {
                                let b = node.bounds()?;
                                Some(egui::Rect::from_min_max(
                                    egui::pos2(b.x0 as f32, b.y0 as f32),
                                    egui::pos2(b.x1 as f32, b.y1 as f32),
                                ))
                            },
                        )
                    });
                }
                let (title_rect, cross_rect) = (title_rect.expect("title"), cross_rect.expect("cross"));
                assert!(cross_rect.max.x <= width + 0.5, "{width}: the cross {cross_rect:?} leaves the page");
                assert!(!cross_rect.intersects(title_rect), "{width}: cross {cross_rect:?} on title {title_rect:?}");
            }
        }
        language::set_thread_language(language::Choice::De);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// The part "clear" says that after a patch that wipes the stock a button empties one part in one go.
    #[test]
    fn the_part_clear_names_the_patch_that_wipes_the_stock() {
        let _language = crate::language_lock();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let (paths, home) = moved_world("patch", SAMPLE_RAW, "{\"lager_umzug_gesehen\": true}");
        for choice in [language::Choice::De, language::Choice::En] {
            language::set_thread_language(choice);
            let hint = t("s_mlg_patch_hilfe");
            let mut state = StockState::with_files(&paths);
            state.panel = Some(Panel::Clear);
            let (texts, _) = moved_frames(&ctx, &mut state, None);
            assert!(texts.contains(&hint), "{texts:?}");
            state.panel = Some(Panel::Backup);
            let (texts, _) = moved_frames(&ctx, &mut state, None);
            assert!(!texts.contains(&hint), "only with the part clear: {texts:?}");
        }
        language::set_thread_language(language::Choice::De);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// The real data of a copy (`VERSEKIT_KOPIE`), read only: what the page
    /// shows, drawn without a window in every tab and purpose.
    #[test]
    #[ignore = "needs a copy of real data"]
    fn real_copy_shows_the_stock() {
        let Ok(root) = std::env::var("VERSEKIT_KOPIE") else { return };
        versekit_core::writeguard::deny(true);
        versekit_core::writeguard::allow_under(None);
        let paths = Paths::for_copy(versekit_core::paths::Host::current(), &root).expect("copy");
        let mut state = StockState::with_files(&paths);
        let rows = state.rows();
        println!(
            "Posten {} (Rohstoffe {}, Handel {}) · Liste {} Rohstoffe, {} Orte, {} Waren, {} Items",
            rows.len(),
            state.raw.len(),
            state.trade.len(),
            state.data.storable().len(),
            state.data.places().len(),
            state.data.goods().len(),
            state.data.item_names().len()
        );
        let scheme = versekit_core::theme::current();
        for (items, purpose) in [(false, 0), (false, 1), (false, 2), (true, 0)] {
            state.items = items;
            state.purpose = purpose;
            let ctx = egui::Context::default();
            crate::install(&ctx);
            for _ in 0..3 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 2000.0))),
                    ..Default::default()
                };
                let mut output = ctx.run_ui(input, |ui| show_page(ui, scheme, &mut state));
                output.textures_delta.clear();
            }
            println!("Items {items} · Zweck {purpose}:");
            for line in rows_named(&state) {
                println!("  {line}");
            }
        }
        let denied = versekit_core::writeguard::take_denied();
        versekit_core::writeguard::deny(false);
        println!("nicht geschrieben (Sperre): {denied:?}");
        assert!(denied.is_empty(), "die Seite wollte schreiben: {denied:?}");
    }

    /// Open forms above must not squeeze the list: the page in a scroll area
    /// of a laptop-sized window, forms open, the list scrolled to its top.
    #[test]
    fn open_forms_leave_the_list_room() {
        let scheme = versekit_core::theme::current();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let mut state = StockState { form_open: true, refinery_open: true, ..StockState::default() };
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 800.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                egui::ScrollArea::vertical().id_salt("seite").show(ui, |page| show_page(page, scheme, &mut state));
            });
            output.textures_delta.clear();
        }
        let (left, room, visible, drawn, _) =
            ctx.data(|d| d.get_temp::<(f32, f32, f32, f32, f32)>(egui::Id::unique(LIST_ROOM_ID))).expect("list drawn");
        let share = crate::layout::LIST_SHARE;
        assert!(room >= visible * share, "list asks {room:.0} of {visible:.0} visible");
        // Inside a scrolling page egui shrinks a scroll area to what is left on
        // screen, down to 64 points; the drawn height is what counts.
        assert!(drawn >= visible * share - 12.0, "list drawn {drawn:.0} of {visible:.0} visible");
        // Counter check: this window really squeezes the list under the forms.
        assert!(left.max(160.0) < visible * share, "forms leave {left:.0}: the test must see the squeeze");
        assert!(crate::layout::room_for_list(10.0, 200.0, 40.0) >= crate::layout::LIST_MIN_ROWS * 40.0);
    }

    #[test]
    fn groups_fold_and_search_opens_them() {
        let mut state = StockState::default();
        assert_eq!(
            rows_named(&state),
            ["Hadanite", "Agricium (2)", "Construction Materials", "Iron", "Laranite", "Taranite"]
        );
        state.search = "levski".into();
        assert_eq!(rows_named(&state), ["Hadanite", "Agricium (2)", "  Agricium", "  Agricium"]);
        state.closed.insert((false, "agricium".into(), false));
        assert_eq!(rows_named(&state), ["Hadanite", "Agricium (2)"]);
    }

    #[test]
    fn a_group_open_for_editing_folds_on_click() {
        let mut state = StockState::default();
        state.edit(Store::Raw, 0);
        assert_eq!(rows_named(&state)[1..4], ["Agricium (2)", "  Agricium", "  Agricium"]);
        state.closed.insert((false, "agricium".into(), false));
        assert_eq!(rows_named(&state)[1], "Agricium (2)");
        assert_ne!(rows_named(&state)[2], "  Agricium");
    }

    #[test]
    fn tabs_purpose_and_sorting() {
        let mut state = StockState { items: true, ..StockState::default() };
        assert_eq!(rows_named(&state), ["FR-66", "TS-2"]);
        state.items = false;
        state.purpose = 2;
        assert_eq!(rows_named(&state), ["Construction Materials", "Laranite"]);
        state.purpose = 0;
        state.sort = Column::Amount;
        state.descending = true;
        // Pieces stay on top, the SCU goods follow by amount.
        assert_eq!(rows_named(&state)[..2], ["Hadanite", "Construction Materials"]);
    }

    #[test]
    fn undo_puts_the_entry_back_at_its_place() {
        let mut state = StockState::default();
        let before = state.raw.clone();
        state.delete(Store::Raw, 2);
        assert_eq!(state.raw.len(), before.len() - 1);
        let Some(Message { undo: Some(Undo::Deleted(deleted)), .. }) = state.message.take() else {
            panic!("kein Rückgängig")
        };
        state.undo(deleted);
        assert_eq!(state.raw, before);
    }

    #[test]
    fn editing_an_entry_that_changed_is_dropped() {
        let mut state = StockState::default();
        state.edit(Store::Trade, 0);
        assert_eq!(state.form, Form::Trade);
        assert_eq!(state.trade_form.goods, "Laranite");
        state.trade[0] = Value::Null;
        assert!(state.drop_stale_edit());
        assert!(state.editing.is_none());
        assert!(state.trade_form.goods.is_empty());
    }

    #[test]
    fn other_forms_keep_what_was_typed() {
        let mut state = StockState::default();
        state.item_form.name = "FR-66".into();
        state.edit(Store::Trade, 0);
        state.stop_editing();
        assert_eq!(state.item_form.name, "FR-66");
    }

    #[test]
    fn stolen_zero_stays_clean() {
        let mut state = StockState::default();
        state.trade[1] = jsonio::loads(r#"{"ware": "Construction Materials", "menge": 4, "ort": "", "gestohlen": 0}"#)
            .expect("Posten");
        state.edit(Store::Trade, 1);
        assert!(!state.trade_form.stolen);
    }

    #[test]
    fn clearing_items_keeps_the_goods() {
        let mut state = StockState::default();
        state.clear(Part::Items);
        assert_eq!(state.count(Part::Items), 0);
        assert_eq!(state.count(Part::Goods), 2);
    }

    #[test]
    fn booking_onto_cscu_counts_in_cscu() {
        let mut state = StockState::default();
        state.raw_form.cscu = true;
        state.edit(Store::Raw, 2);
        assert_eq!(state.raw_form.amount, field_text(125.0));
        state.raw_form.amount = "+50".into();
        state.save_raw();
        assert_eq!(stock::number_of(&state.raw[2], "menge"), 1.75);
    }

    /// The usage counters learn that a material was booked by hand or a refinery yield was taken,
    /// only how often, and only once the input was accepted (`page_usage.action`).
    #[test]
    fn a_booking_by_hand_and_a_refinery_yield_count_once_each() {
        let refused =
            RawForm { material: "Iron".into(), amount: "3-3".into(), quality: "500".into(), ..RawForm::default() };
        let mut state = StockState { raw_form: refused, ..StockState::default() };
        state.save_raw();
        assert!(state.take_counted().is_empty(), "a refused booking counts not");
        state.edit(Store::Raw, 2);
        state.raw_form.amount = "+50".into();
        state.save_raw();
        assert_eq!(state.take_counted(), vec!["lager_hand"]);
        assert!(state.take_counted().is_empty(), "taken once");
        state.refinery_place = "Nirgendwo am Rand".into();
        book_refinery(&mut state);
        assert!(state.refinery_error.is_some());
        assert!(state.take_counted().is_empty(), "an unknown place counts not");
        state.refinery_place = String::new();
        book_refinery(&mut state);
        assert_eq!(state.take_counted(), vec!["lager_raffinerie"], "an empty place is allowed");
    }

    use versekit_core::stock_data::PLACES_FILE;

    const EMPTY_STORE: &str = r#"{"format": 1, "posten": []}"#;

    /// Runs `frames` frames of the page on `ctx`; the texts painted in the last one.
    fn run_frames(ctx: &egui::Context, state: &mut StockState, frames: usize) -> Vec<String> {
        fn collect(shape: &egui::Shape, into: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => into.push(text.galley.text().to_string()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, into)),
                _ => {}
            }
        }
        let scheme = versekit_core::theme::current();
        let mut texts = Vec::new();
        for _ in 0..frames {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 2000.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| show_page(ui, scheme, state));
            output.textures_delta.clear();
            texts.clear();
            output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
        }
        texts
    }

    #[test]
    fn the_refinery_place_starts_on_the_last_one_booked_and_survives_a_restart() {
        let (mut state, folder) = with_scratch("raffinerie-ort-merken", EMPTY_STORE, EMPTY_STORE);
        state.raw_place = "Orison".into();
        state.book_refinery_lines("Iron 295 188", "levski");
        assert_eq!(state.refinery_last, "Levski", "the place as the list spells it");
        assert_eq!(state.refinery_place, "Levski");
        // A yield booked without a place remembers nothing and leaves the last place alone.
        state.book_refinery_lines("Iron 10 100", "");
        assert_eq!(state.refinery_last, "Levski");
        // An unknown place books nothing and is not remembered.
        state.book_refinery_lines("Iron 10 100", "Nirgendwo am Rand");
        assert_eq!(state.refinery_last, "Levski");
        assert!(state.setting_values().contains(&(SETTING_REFINERY_PLACE, Value::str("Levski"))));
        let stored = jsonio::dumps(
            &Value::Object(state.setting_values().into_iter().map(|(k, v)| (k.to_string(), v)).collect()),
            None,
            false,
        );
        let places = r#"{"format": 1, "orte": ["Levski", "Orison"]}"#;
        let (paths, home) =
            data_folder("raffinerie-ort-neustart", &[("einstellungen.json", &stored), (PLACES_FILE, places)]);
        let again = StockState::with_files(&paths);
        assert_eq!(again.refinery_place, "Levski", "the field starts on the last yield's place, not the material's");
        assert_eq!(again.raw_place, "Orison");
        // A place the list no longer knows is not offered: the field falls back to the material's place.
        let gone = r#"{"format": 1, "orte": ["Area18", "Orison"]}"#;
        let (paths2, home2) =
            data_folder("raffinerie-ort-weg", &[("einstellungen.json", &stored), (PLACES_FILE, gone)]);
        assert_eq!(StockState::with_files(&paths2).refinery_place, "Orison");
        // Nothing remembered yet (an old settings file): the place of the last material, as before.
        let old = r#"{"lager_ort": "Area18"}"#;
        let (paths3, home3) = data_folder("raffinerie-ort-alt", &[("einstellungen.json", old)]);
        assert_eq!(StockState::with_files(&paths3).refinery_place, "Area18");
        for dir in [&folder, &home, &home2, &home3] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn an_empty_refinery_place_is_led_to_first_and_booked_anyway_with_the_second_press() {
        let (mut state, folder) = with_scratch("raffinerie-ort-fuehren", EMPTY_STORE, EMPTY_STORE);
        assert!(!state.split.place_required());
        // Nothing to book yet, or a place typed: no mark.
        assert_eq!(state.refinery_place_note(false), None);
        state.refinery_place = "Levski".into();
        assert_eq!(state.refinery_place_note(true), None);
        state.refinery_place = String::new();
        assert_eq!(state.refinery_note_key(), Some("s_rf_ort_leer"));
        // First press: nothing booked, the field is asked for, the hint says what the next press does.
        state.press_refinery_with("Iron 295 188", "");
        assert!(state.raw.is_empty(), "the first press books nothing");
        assert!(state.refinery_leads_to_place());
        assert_eq!(state.refinery_note_key(), Some("s_rf_ort_leer_nochmal"));
        assert!(state.take_counted().is_empty());
        // Second press: booked without a place.
        state.refinery_focus = false;
        state.press_refinery_with("Iron 295 188", "");
        assert_eq!(state.raw.len(), 1, "the second press books");
        assert!(!state.refinery_leads_to_place());
        assert_eq!(state.take_counted(), vec!["lager_raffinerie"]);
        assert_eq!(state.refinery_last, "", "no place, nothing remembered");
        // With a place the first press books at once.
        state.press_refinery_with("Iron 100 150", "Levski");
        assert_eq!(state.raw.len(), 2);
        assert!(!state.refinery_leads_to_place());
        assert_eq!(state.refinery_last, "Levski");
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn typing_a_place_takes_back_the_first_press() {
        let (mut state, folder) = with_scratch("raffinerie-ort-tippen", EMPTY_STORE, EMPTY_STORE);
        let _language = crate::language_lock();
        language::set_thread_language(language::Choice::De);
        state.refinery_open = true;
        state.refinery_text = "Iron 295 188".into();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        state.press_refinery(&ctx);
        assert!(state.refinery_asked);
        state.refinery_place = "Levski".into();
        run_frames(&ctx, &mut state, 2);
        assert!(!state.refinery_asked, "a typed place ends the question");
        state.refinery_place = String::new();
        run_frames(&ctx, &mut state, 2);
        state.press_refinery(&ctx);
        assert!(state.raw.is_empty(), "emptied again: the first press leads to the field once more");
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn the_empty_refinery_place_is_marked_and_the_press_puts_the_cursor_into_it() {
        let _language = crate::language_lock();
        language::set_thread_language(language::Choice::De);
        let mut state = StockState { form_open: false, refinery_open: true, ..StockState::default() };
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        // Nothing typed: the empty field is not nagged.
        let texts = run_frames(&ctx, &mut state, 2);
        assert!(!texts.iter().any(|s| s.contains(&t("s_rf_ort_leer"))), "{texts:?}");
        // Something to book and no place: the hint stands below the field.
        state.refinery_text = "Iron 295 188".into();
        let texts = run_frames(&ctx, &mut state, 2);
        assert!(texts.iter().any(|s| s == &t("s_rf_ort_leer")), "{texts:?}");
        let field = state.refinery_field.expect("the field was drawn");
        assert!(!ctx.memory(|m| m.has_focus(field)));
        // The press: the hint changes, the field has the cursor.
        state.press_refinery(&ctx);
        let texts = run_frames(&ctx, &mut state, 3);
        assert!(texts.iter().any(|s| s == &t("s_rf_ort_leer_nochmal")), "{texts:?}");
        assert!(ctx.memory(|m| m.has_focus(field)), "the place field has the focus");
        assert!(!state.refinery_leads_to_place(), "the request is used up");
        // A place typed: the mark is gone.
        state.refinery_place = "Levski".into();
        let texts = run_frames(&ctx, &mut state, 2);
        assert!(!texts.iter().any(|s| s.starts_with(&t("s_rf_ort_leer"))), "{texts:?}");
    }

    #[test]
    fn a_new_entry_of_zero_is_refused() {
        let mut state = StockState::default();
        let before = state.raw.len();
        state.raw_form =
            RawForm { material: "Iron".into(), amount: "3-3".into(), quality: "500".into(), ..RawForm::default() };
        state.save_raw();
        assert_eq!(state.raw.len(), before);
        assert!(state.raw_form.errors.amount.is_some());
    }

    #[test]
    fn saving_reads_the_file_first_and_keeps_changes_from_outside() {
        let raw = r#"{"format": 1, "posten": [{"material": "Iron", "menge": 1.0, "qualitaet": 500, "ort": ""}]}"#;
        let (mut state, folder) = with_scratch("fremd", raw, r#"{"format": 1, "posten": []}"#);
        let outside = r#"{"format": 1, "posten": [{"material": "Iron", "menge": 1.0, "qualitaet": 500, "ort": ""},
 {"material": "Taranite", "menge": 2.0, "qualitaet": 600, "ort": ""}]}"#;
        let raw_path = format!("{folder}/{}", stock::RAW_FILE);
        jsonio::write_text_atomic(&raw_path, outside, jsonio::Newline::Native).expect("fremd");
        state.raw_form =
            RawForm { material: "Iron".into(), amount: "4".into(), quality: "400".into(), ..RawForm::default() };
        state.save_raw();
        let written = stock::load(&raw_path, "test");
        assert_eq!(written.len(), 3, "die fremde Änderung muss bleiben");
        assert!(state.raw_form.material.is_empty());
    }

    #[test]
    fn a_failed_write_leaves_the_disk_state_and_the_form() {
        let raw = r#"{"format": 1, "posten": [{"material": "Iron", "menge": 1.0, "qualitaet": 500, "ort": ""}]}"#;
        let (mut state, _folder) = with_scratch("gesperrt", raw, r#"{"format": 1, "posten": []}"#);
        versekit_core::writeguard::deny(true);
        versekit_core::writeguard::allow_under(None);
        state.raw_form =
            RawForm { material: "Taranite".into(), amount: "2".into(), quality: "600".into(), ..RawForm::default() };
        state.save_raw();
        state.delete(Store::Raw, 0);
        let denied = versekit_core::writeguard::take_denied();
        versekit_core::writeguard::deny(false);
        assert!(!denied.is_empty());
        assert_eq!(state.raw.len(), 1, "nichts Ungespeichertes bleibt im Speicher");
        assert_eq!(state.raw_form.material, "Taranite", "die Eingabe bleibt stehen");
    }

    /// Both stores out as JSON and back in: the same entries; the goods
    /// backup keeps the items that are here; the table is 4.12's CSV.
    #[test]
    fn a_backup_comes_back_as_it_went_out() {
        let (mut state, folder) = with_scratch("sicherung", SAMPLE_RAW, SAMPLE_TRADE);
        let (raw_before, trade_before) = (state.raw.clone(), state.trade.clone());
        let raw_out = format!("{folder}/lager.json");
        let trade_out = format!("{folder}/handelslager.json.aus");
        let table = format!("{folder}/lager.csv");
        state.export_to(Store::Raw, false, &raw_out);
        state.export_to(Store::Trade, false, &trade_out);
        state.export_to(Store::Raw, true, &table);
        let csv = std::fs::read_to_string(&table).expect("Tabelle");
        // 4.12 writes the table in text mode: the system's line ending.
        let eol = if cfg!(windows) { "\r\n" } else { "\n" };
        let head = format!("Material;Menge;Qualitaet;Lagerort{eol}Agricium;2,998;588;Levski{eol}");
        assert!(csv.starts_with(&head), "{csv:?}");

        state.clear(Part::Raw);
        state.clear(Part::Goods);
        state.import_from(&raw_out);
        assert_eq!(state.raw, raw_before, "Rohstoffe wie vorher");
        // An item added after the backup stays; the goods come back.
        state.trade.push(Value::Object(vec![
            ("ware".into(), Value::str("P4-AR")),
            ("menge".into(), Value::Float(1.0)),
            ("ort".into(), Value::str("Levski")),
            ("gestohlen".into(), Value::Bool(false)),
            ("stueck".into(), Value::Bool(true)),
        ]));
        assert!(state.write(Store::Trade));
        state.import_from(&trade_out);
        let names: Vec<_> = state.trade.iter().map(|e| name_of(e, "ware")).collect();
        assert_eq!(names, ["Laranite", "Construction Materials", "FR-66", "TS-2", "P4-AR"]);
        assert_eq!(state.trade[..4], trade_before[..], "Ware und Items wie in der Sicherung");
    }

    /// A file that is no backup changes nothing and says so; a write the
    /// preview's lock refuses names the test copy.
    #[test]
    fn a_wrong_file_and_a_refused_write_change_nothing() {
        let (mut state, folder) = with_scratch("falsch", SAMPLE_RAW, SAMPLE_TRADE);
        let before = (state.raw.clone(), state.trade.clone());
        let wrong = format!("{folder}/fremd.json");
        jsonio::write_text_atomic(
            &wrong,
            r#"{"format": 2, "posten": [{"material": "Gold"}]}"#,
            jsonio::Newline::Native,
        )
        .expect("Datei");
        state.import_from(&wrong);
        assert_eq!((state.raw.clone(), state.trade.clone()), before);
        assert_eq!(state.message.as_ref().map(|m| m.text.clone()), Some(t("s_mlg_datei_falsch")));

        versekit_core::writeguard::deny(true);
        versekit_core::writeguard::allow_under(Some(&folder));
        state.export_to(Store::Raw, false, "/elsewhere/lager.json");
        let denied = versekit_core::writeguard::take_denied();
        versekit_core::writeguard::deny(false);
        versekit_core::writeguard::allow_under(None);
        assert!(!denied.is_empty());
        assert_eq!(state.message.as_ref().map(|m| m.text.clone()), Some(t("s_mlg_nur_testkopie")));
    }

    /// A read-in keeps the old store beside its file and undo
    /// brings it back; a name without ending gets one; the page's own
    /// files are refused as backup target.
    #[test]
    fn a_read_in_can_be_taken_back() {
        let (mut state, folder) = with_scratch("zurueck", SAMPLE_RAW, SAMPLE_TRADE);
        let raw_file = state.files.as_ref().expect("Dateien").0.clone();
        let backed_up = state.raw.clone();
        state.export_to(Store::Raw, false, &format!("{folder}/sicherung"));
        let backup = format!("{folder}/sicherung.json");
        assert!(std::path::Path::new(&backup).exists(), "Endung angehängt");

        state.delete(Store::Raw, 0);
        let changed = state.raw.clone();
        assert_ne!(changed, backed_up);
        state.import_from(&backup);
        assert_eq!(state.raw, backed_up, "Sicherung eingelesen");
        let kept = std::fs::read_to_string(format!("{raw_file}.vor-einlesen")).expect("alter Stand daneben");
        assert_eq!(stock::raw_from_json(&kept), Some(changed.clone()), "alter Stand in der Datei");

        let Some(Message { undo: Some(back @ Undo::Replaced(..)), .. }) = state.message.take() else {
            panic!("kein Rückgängig nach dem Einlesen")
        };
        state.take_back(back);
        assert_eq!(state.raw, changed, "Rückgängig bringt den alten Stand");
        state.reload();
        assert_eq!(state.raw, changed, "auch in der Datei");

        let file_before = std::fs::read_to_string(&raw_file).expect("Lagerdatei");
        state.export_to(Store::Trade, false, &raw_file);
        assert_eq!(state.message.as_ref().map(|m| m.text.clone()), Some(t("s_mlg_eigene_datei")));
        assert_eq!(std::fs::read_to_string(&raw_file).expect("Lagerdatei"), file_before);
    }

    #[test]
    fn the_remembered_places_are_kept() {
        let (mut state, _folder) =
            with_scratch("orte", r#"{"format": 1, "posten": []}"#, r#"{"format": 1, "posten": []}"#);
        state.raw_form = RawForm {
            material: "Iron".into(),
            amount: "1".into(),
            quality: "1".into(),
            place: "levski".into(),
            ..RawForm::default()
        };
        state.save_raw();
        assert_eq!(state.raw_place, "Levski");
        assert_eq!(state.raw_form.place, "Levski");
        assert!(state.setting_values().contains(&(SETTING_RAW_PLACE, Value::str("Levski"))));
    }

    #[test]
    fn the_place_of_the_last_entry_starts_every_form_and_survives_cancelling_and_a_restart() {
        let (mut state, folder) =
            with_scratch("ort-bleibt", r#"{"format": 1, "posten": []}"#, r#"{"format": 1, "posten": []}"#);
        state.raw_form = RawForm {
            material: "Iron".into(),
            amount: "1".into(),
            quality: "1".into(),
            place: "Levski".into(),
            ..RawForm::default()
        };
        state.save_raw();
        assert_eq!(state.form_places()[0], "Levski", "the next material starts on the same place");
        // Goods booked at two places: the form starts on the one booked last, also for items.
        for place in ["Levski", "Seraphim Station"] {
            state.trade_form =
                TradeForm { goods: "Laranite".into(), amount: "5".into(), place: place.into(), ..TradeForm::default() };
            state.save_trade();
            assert_eq!(state.trade_form.errors, Errors::default(), "{place}");
        }
        assert_eq!(state.form_places()[1..], ["Seraphim Station", "Seraphim Station"], "goods and items share it");
        // Editing the first goods entry shows its place; cancelling brings the last one back.
        let first = state.rows().iter().find(|r| r.store == Store::Trade && r.place == "Levski").map(|r| r.index);
        state.edit(Store::Trade, first.expect("the goods at Levski"));
        assert_eq!(state.trade_form.place, "Levski");
        state.stop_editing();
        assert_eq!(state.form_places()[1..], ["Seraphim Station", "Seraphim Station"]);
        state.edit(Store::Raw, 0);
        state.raw_form.place = "Orison".into();
        state.stop_editing();
        assert_eq!(state.form_places()[0], "Levski", "a place typed and not booked is not remembered");
        // Written as the window writes the settings, read again at the next start.
        let stored = Value::Object(state.setting_values().into_iter().map(|(k, v)| (k.to_string(), v)).collect());
        let (paths, home) =
            data_folder("ort-neustart", &[("einstellungen.json", &jsonio::dumps(&stored, None, false))]);
        let again = StockState::with_files(&paths);
        assert_eq!(again.form_places(), ["Levski", "Seraphim Station", "Seraphim Station"]);
        let _ = std::fs::remove_dir_all(&folder);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// The page fits a narrow window at every size, also with goods for sale.
    #[test]
    fn nothing_sticks_out() {
        let scheme = versekit_core::theme::current();
        let mut findings = Vec::new();
        for points in versekit_core::font_size::POINTS_RANGE.0..=versekit_core::font_size::POINTS_RANGE.1 {
            for width in [380.0, 520.0, 900.0] {
                for (purpose, panel) in [(0, Some(Panel::Backup)), (2, Some(Panel::Clear)), (0, None)] {
                    let ctx = egui::Context::default();
                    crate::install(&ctx);
                    crate::apply_size(&ctx, points);
                    let mut state =
                        StockState { form_open: true, refinery_open: true, panel, purpose, ..StockState::default() };
                    state.refinery_text = "Titanium 295 188\nzu kurz".into();
                    let mut over = 0.0;
                    for _ in 0..3 {
                        let input = egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 2000.0))),
                            ..Default::default()
                        };
                        let mut output = ctx.run_ui(input, |ui| {
                            show_page(ui, scheme, &mut state);
                            over = (ui.min_rect().right() - ui.ctx().content_rect().right()).max(0.0);
                        });
                        output.textures_delta.clear();
                    }
                    if over > 0.5 {
                        findings.push(format!("Größe {points} Breite {width} Zweck {purpose}: {over:.0} zu breit"));
                    }
                }
            }
        }
        assert!(findings.is_empty(), "{}", findings.join("\n"));
    }

    /// A data folder of its own with the files given, for the page with own data.
    fn data_folder(name: &str, files: &[(&str, &str)]) -> (Paths, String) {
        let home =
            std::env::temp_dir().join(format!("versekit-lager-daten-{name}-{}", versekit_core::testdir::run_tag()));
        let home = home.to_string_lossy().replace('\\', "/");
        let _ = std::fs::remove_dir_all(&home);
        jsonio::make_dirs(&home).expect("folder");
        let paths = Paths::new(versekit_core::paths::Host::new(
            versekit_core::pypath::Flavor::Posix,
            &[(versekit_core::legacy::ENV_HOME, home.as_str()), ("HOME", home.as_str())],
            &home,
        ));
        for (file, text) in files {
            jsonio::write_text_atomic(&paths.app_file(file), text, jsonio::Newline::Native).expect(file);
        }
        (paths, home)
    }

    /// The texts painted in a few frames of the page.
    fn painted(state: &mut StockState) -> Vec<String> {
        let scheme = versekit_core::theme::current();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let mut texts = Vec::new();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 2000.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| show_page(ui, scheme, state));
            output.textures_delta.clear();
            fn collect(shape: &egui::Shape, into: &mut Vec<String>) {
                match shape {
                    egui::Shape::Text(text) => into.push(text.galley.text().to_string()),
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, into)),
                    _ => {}
                }
            }
            texts.clear();
            output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
        }
        texts
    }

    /// A long item name with its tag and its badges stays left of the place column, also in a narrow window:
    /// badges that do not fit go into the `+n` badge instead of being painted over the place.
    #[test]
    fn the_badges_of_an_item_never_reach_into_the_place_column() {
        let _language = crate::language_lock();
        language::set_thread_language(language::Choice::De);
        let catalog = r#"{"version": "4.3", "geholt": "", "bauplaene": {
          "snowblind": {"n": "Snowblind Shield Generator Deluxe", "a": "Shield", "s": 1, "g": 1, "c": "Stealth"}}}"#;
        let trade = r#"{"format": 1, "posten": [
          {"ware": "Snowblind Shield Generator Deluxe", "menge": 2.0, "ort": "Checkmate Station",
           "gestohlen": true, "stueck": true, "einkauf": 20190.0}]}"#;
        let (paths, home) = data_folder(
            "abzeichen-breite",
            &[(versekit_core::catalog_cache::CACHE_FILE, catalog), (stock::RAW_FILE, EMPTY_STORE), (stock::TRADE_FILE, trade)],
        );
        let mut state = StockState::with_files(&paths);
        state.items = true;
        let kind = t(&state.kind_of(&state.rows()[0]).text_key());
        let scheme = versekit_core::theme::current();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let mut found: Vec<(String, egui::Rect, egui::Rect)> = Vec::new();
        for width in [760.0, 800.0, 840.0, 880.0, 920.0, 1000.0, 1400.0] {
            for _ in 0..3 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 2000.0))),
                    ..Default::default()
                };
                let mut output = ctx.run_ui(input, |ui| show_page(ui, scheme, &mut state));
                output.textures_delta.clear();
                found = output
                    .shapes
                    .iter()
                    .filter_map(|clipped| match &clipped.shape {
                        egui::Shape::Text(text) => Some((
                            text.galley.text().to_string(),
                            text.galley.rect.translate(text.pos.to_vec2()),
                            clipped.clip_rect,
                        )),
                        _ => None,
                    })
                    .collect();
            }
            let place = found.iter().find(|(text, ..)| text == "Checkmate Station").map(|(_, rect, _)| *rect);
            let place = place.unwrap_or_else(|| panic!("place painted at {width}: {:?}", found));            for (text, rect, clip) in &found {
                let same_row = (rect.center().y - place.center().y).abs() < place.height();
                if !same_row || text == "Checkmate Station" || rect.min.x >= place.min.x {
                    continue;
                }
                let seen_right = rect.max.x.min(clip.max.x);
                assert!(seen_right <= place.min.x + 0.5, "{text:?} reaches into the place at {width}: {rect:?} {place:?}");
            }
            let shown = found.iter().any(|(text, ..)| *text == kind);
            let folded = found.iter().any(|(text, ..)| text.starts_with('+'));
            assert!(shown || folded, "the kind is shown or folded into +n at {width}");
        }
        let _ = std::fs::remove_dir_all(home);
    }

    /// Items show the tag of the blueprint catalog behind the name, raw materials how they are mined, both
    /// only in the first row of a group; read from the data folder.
    #[test]
    fn items_show_their_tag_and_raw_materials_how_they_are_mined() {
        let _language = crate::language_lock();
        language::set_thread_language(language::Choice::De);
        let catalog = r#"{"version": "4.3", "geholt": "", "bauplaene": {
          "fr-66": {"n": "FR-66", "a": "Shield", "s": 2, "g": 1, "c": "Military"},
          "p4-ar": {"n": "P4-AR", "a": "WeaponPersonal", "c": "Ballistic"},
          "ts-2": {"n": "TS-2", "a": "Cooler", "c": "Civilian"}}}"#;
        let mining = r#"{"format": 1, "locations": [
          {"locationName": "Daymar", "groups": [
            {"groupName": "FPS_Mineables", "deposits": [{"compositionGuid": "c1"}]},
            {"groupName": "SpaceShip_Mineables", "deposits": [{"compositionGuid": "c1"}]},
            {"groupName": "SpaceShip_Mineables_Rare", "deposits": [{"compositionGuid": "c2"}]}]},
          {"locationName": "Yela", "groups": [
            {"groupName": "GroundVehicle_Mineables", "deposits": [{"compositionGuid": "c1"}]}]}],
         "compositions": {
           "c1": {"parts": [{"elementName": "Iron", "minPercent": 10, "maxPercent": 20}]},
           "c2": {"parts": [{"elementName": "Gold", "minPercent": 10, "maxPercent": 20}]}}}"#;
        let raw = r#"{"format": 1, "posten": [
          {"material": "Iron", "menge": 1.0, "qualitaet": 500, "ort": "Levski"},
          {"material": "Iron", "menge": 2.0, "qualitaet": 600, "ort": "Area18"},
          {"material": "Gold", "menge": 1.0, "qualitaet": 500, "ort": "Levski"},
          {"material": "Taranite", "menge": 1.0, "qualitaet": 500, "ort": "Levski"}]}"#;
        let trade = r#"{"format": 1, "posten": [
          {"ware": "FR-66", "menge": 1.0, "ort": "Levski", "gestohlen": false, "stueck": true},
          {"ware": "P4-AR", "menge": 1.0, "ort": "Levski", "gestohlen": false, "stueck": true},
          {"ware": "TS-2", "menge": 1.0, "ort": "Levski", "gestohlen": false, "stueck": true}]}"#;
        let (paths, home) = data_folder(
            "kuerzel",
            &[
                (versekit_core::catalog_cache::CACHE_FILE, catalog),
                (versekit_core::mining_data::FILE, mining),
                (stock::RAW_FILE, raw),
                (stock::TRADE_FILE, trade),
            ],
        );
        let mut state = StockState::with_files(&paths);
        let extras = |state: &StockState| -> Vec<(String, Option<String>, String)> {
            let rows = state.rows();
            let shown = visible(state, &rows);
            lines(state, &shown)
                .iter()
                .map(|line| {
                    let name = match line {
                        Line::Group(_, rows, _) => format!("{} ({})", rows[0].name, rows.len()),
                        Line::Post(row, nested) => format!("{}{}", if *nested { "  " } else { "" }, row.name),
                    };
                    let (tag, extra) = name_extras(state, line);
                    (name, tag, extra)
                })
                .collect()
        };
        let line = |name: &str, tag: Option<&str>, extra: &str| (name.to_string(), tag.map(String::from), extra.into());
        state.items = true;
        assert_eq!(
            extras(&state),
            [line("FR-66", Some("(Mil/2/A)"), ""), line("P4-AR", Some("(Bal)"), ""), line("TS-2", None, "")]
        );
        let texts = painted(&mut state);
        assert!(texts.contains(&"(Mil/2/A)".to_string()) && texts.contains(&"(Bal)".to_string()), "{texts:?}");

        state.items = false;
        state.open.insert((false, "iron".into(), false));
        let all_ways = format!("{} · {} · {}", t("s_lg_abbau_fps"), t("s_lg_abbau_fahrzeug"), t("s_lg_abbau_schiff"));
        assert_eq!(
            extras(&state),
            [
                line("Gold", None, &t("s_lg_abbau_schiff")),
                line("Iron (2)", None, &all_ways),
                line("  Iron", None, ""),
                line("  Iron", None, ""),
                line("Taranite", None, ""),
            ]
        );
        let texts = painted(&mut state);
        assert!(texts.contains(&all_ways) && texts.contains(&t("s_lg_abbau_schiff")), "{texts:?}");
        assert!(
            !texts.iter().any(|text| text.starts_with('(') && text.ends_with(')')),
            "no tag at materials: {texts:?}"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    /// Whether a panel of the rare actions is open, and which, is read from the settings and written back.
    #[test]
    fn the_open_panel_of_the_rare_actions_is_kept_over_a_restart() {
        for (name, settings, want) in [
            ("zu", "{}", None),
            ("sichern", r#"{"meinlager_sichern_offen": true}"#, Some(Panel::Backup)),
            ("leeren", r#"{"meinlager_sichern_offen": true, "meinlager_sichern_teil": "leeren"}"#, Some(Panel::Clear)),
            ("nur-teil", r#"{"meinlager_sichern_offen": false, "meinlager_sichern_teil": "leeren"}"#, None),
        ] {
            let (paths, home) = data_folder(name, &[("einstellungen.json", settings)]);
            assert_eq!(StockState::with_files(&paths).panel, want, "{name}");
            let _ = std::fs::remove_dir_all(&home);
        }
        let (paths, home) = data_folder("schreiben", &[]);
        let mut state = StockState::with_files(&paths);
        let kept = |state: &StockState| -> (Option<Value>, Option<Value>) {
            let values = state.setting_values();
            let find = |key: &str| values.iter().find(|(k, _)| *k == key).map(|(_, v)| v.clone());
            (find(SETTING_PANEL_OPEN), find(SETTING_PANEL))
        };
        assert_eq!(kept(&state), (Some(Value::Bool(false)), Some(Value::str("sichern"))));
        state.toggle_panel(Panel::Clear);
        assert_eq!(kept(&state), (Some(Value::Bool(true)), Some(Value::str("leeren"))));
        // Written as the window writes the settings and read again: the same panel.
        let stored = Value::Object(state.setting_values().into_iter().map(|(k, v)| (k.to_string(), v)).collect());
        let (again, again_home) =
            data_folder("wieder", &[("einstellungen.json", &jsonio::dumps(&stored, None, false))]);
        assert_eq!(StockState::with_files(&again).panel, Some(Panel::Clear));
        state.toggle_panel(Panel::Clear);
        assert_eq!(kept(&state).0, Some(Value::Bool(false)));
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&again_home);
    }
}
