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

//! The page "Kaufen & Verkaufen" (`pages._selling` of 4.12): where a load is
//! sold for the most, or where a good is bought cheapest.
//!
//! * Selling: one or more goods, each with the amount (or the amount of Mein
//!   Lager); the places that take the most of them stand on top, with the
//!   price per SCU, the proceeds and, with a price paid, the profit.
//! * Buying: one good; the terminals that sell it, the cheapest first, with
//!   the stock in seven steps.
//! * Without a choice a list of what pays best (or what is cheapest) at the
//!   moment, all goods or only what can be mined.
//!
//! The prices come from `verkauf.json` (see `versekit_core::selling`); the
//! button fetches them, at most once an hour.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::SystemTime;

use egui::{Align, Layout, RichText, Ui};
use versekit_core::formats;
use versekit_core::jsonio::Value;
use versekit_core::language::{self, Arg, Lang, t, tf};
use versekit_core::mining_data::MiningData;
use versekit_core::paths::Paths;
use versekit_core::selling::{self, Place, Selling, Update};
use versekit_core::stock::{self, TRADE_FILE};
use versekit_core::stock_data::{MINING_FILE, StockData, norm_material, py_float};
use versekit_core::stock_fetch;
use versekit_core::theme::{Color, Scheme};

use crate::color;
use crate::layout::{self, GAP};
use crate::trade::{self, Job, Polled};
use crate::widgets::{self, ButtonKind};

const SETTING_MODE: &str = "verkauf_modus";
const SETTING_TOP: &str = "verkauf_spitze";
/// The largest amount in SCU the page counts with (a typed 1e30 is no amount).
const MAX_AMOUNT: f64 = 1.0e9;
/// Places shown at most.
const MAX_PLACES: usize = 40;
/// Goods in the lists of what pays best.
const TOP_GOODS: usize = 12;
const TOP_PIECES: usize = 8;
/// How often the page looks whether its files changed.
const LOOK_SECONDS: f64 = 1.0;
const LIST_ID: &str = "handel-verkauf-liste";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Sell,
    Buy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Top {
    All,
    Mining,
}

/// What the last fetch said, for the notice.
struct Message {
    text: String,
    icon: &'static str,
    since: Option<f64>,
}

/// What the page holds.
pub struct SellingState {
    paths: Option<Arc<Paths>>,
    data: Selling,
    cargo: Vec<Value>,
    stock: StockData,
    /// The mining data: the key of a good goes through its aliases.
    mining: MiningData,
    mineable: HashSet<String>,
    sizes: HashMap<String, f64>,
    seen: [Option<SystemTime>; 2],
    looked_at: f64,
    mode: Mode,
    top: Top,
    search: String,
    picks: Vec<String>,
    amounts: HashMap<String, String>,
    nqa: bool,
    buy_good: Option<String>,
    message: Option<Message>,
    job: Option<Job<Update>>,
    /// The seconds the button is still locked, read from the file at most once a second.
    wait_value: i64,
    wait_checked: f64,
    /// The live game build, read with the lists.
    live: String,
    /// How far the list sticks out at the right (the edge check reads it).
    list_overflow: f32,
}

impl Default for SellingState {
    fn default() -> Self {
        SellingState {
            paths: None,
            data: Selling::default(),
            cargo: Vec::new(),
            stock: StockData::default(),
            mining: MiningData::default(),
            mineable: HashSet::new(),
            sizes: HashMap::new(),
            seen: [None, None],
            looked_at: f64::NEG_INFINITY,
            mode: Mode::Sell,
            top: Top::All,
            search: String::new(),
            picks: Vec::new(),
            amounts: HashMap::new(),
            nqa: false,
            buy_good: None,
            message: None,
            job: None,
            wait_value: 0,
            wait_checked: f64::NEG_INFINITY,
            live: String::new(),
            list_overflow: 0.0,
        }
    }
}

fn modified(file: &str) -> Option<SystemTime> {
    std::fs::metadata(file).and_then(|m| m.modified()).ok()
}

impl SellingState {
    /// With own data: the price list and the trade stock of the data folder.
    pub fn with_files(paths: &Paths) -> SellingState {
        let mut state = SellingState { paths: Some(trade::job_paths(paths)), ..SellingState::default() };
        state.mode = if paths.setting(SETTING_MODE).as_deref() == Some("kaufen") { Mode::Buy } else { Mode::Sell };
        state.top = if paths.setting(SETTING_TOP).as_deref() == Some("bergbau") { Top::Mining } else { Top::All };
        state.reload();
        state
    }

    /// How far the list stuck out at the right in the last frame.
    pub fn list_overflow(&self) -> f32 {
        self.list_overflow
    }

    /// The settings this page keeps in the settings file.
    pub fn setting_values(&self) -> Vec<(&'static str, Value)> {
        if self.paths.is_none() {
            return Vec::new();
        }
        vec![
            (SETTING_MODE, Value::str(if self.mode == Mode::Buy { "kaufen" } else { "verkaufen" })),
            (SETTING_TOP, Value::str(if self.top == Top::Mining { "bergbau" } else { "alle" })),
        ]
    }

    fn files(&self) -> Option<[String; 2]> {
        let paths = self.paths.as_ref()?;
        Some([paths.app_file(selling::CACHE), paths.app_file(TRADE_FILE)])
    }

    fn reload(&mut self) {
        let Some(paths) = self.paths.clone() else { return };
        if let Some(files) = self.files() {
            self.seen = [modified(&files[0]), modified(&files[1])];
        }
        self.data = Selling::load(&paths);
        self.cargo = stock::load(&paths.app_file(TRADE_FILE), "trade_cargo.load");
        self.stock = StockData::load(|name| paths.app_file(name));
        self.mining = MiningData::load(&paths.app_file(MINING_FILE));
        self.mineable = selling::mineable_keys(&self.mining);
        self.sizes = selling::piece_sizes(&paths);
        self.live = stock_fetch::live_build(&paths);
        self.wait_checked = f64::NEG_INFINITY;
    }

    /// The seconds the update button is locked, asked of the file once a second.
    fn wait(&mut self, now: f64) -> i64 {
        if now - self.wait_checked >= 1.0 || now < self.wait_checked {
            self.wait_value = self.paths.as_ref().map_or(0, |paths| selling::wait_time(paths, now));
            self.wait_checked = now;
        }
        (self.wait_value - (now - self.wait_checked) as i64).max(0)
    }

    fn follow(&mut self, ui: &Ui) {
        let now = ui.input(|i| i.time);
        if now - self.looked_at < LOOK_SECONDS {
            return;
        }
        self.looked_at = now;
        if let Some(files) = self.files()
            && self.seen != [modified(&files[0]), modified(&files[1])]
        {
            self.reload();
        }
    }

    fn poll_job(&mut self, ui: &Ui) {
        let Some(job) = &self.job else { return };
        let done = match job.poll() {
            Polled::Running => return,
            Polled::Done(result) => result,
            Polled::Lost => Update::Network,
        };
        self.job = None;
        let (key, icon) = match done {
            Update::Done => ("s_vk_geholt", "check"),
            Update::Locked => ("s_hd_gesperrt", "clock"),
            Update::Off => ("s_vk_kein_netz_aus", "shield-alert"),
            Update::Network | Update::Empty | Update::NotSaved => ("s_vk_fehler", "shield-alert"),
        };
        self.message = Some(Message { text: t(key), icon, since: Some(ui.input(|i| i.time)) });
        self.reload();
    }

    fn now(&self) -> f64 {
        trade::now()
    }

    /// The size of one piece, when the good is counted in pieces and the size is known.
    fn piece_size(&self, good: &str) -> Option<f64> {
        if !self.stock.is_piece(good) {
            return None;
        }
        self.sizes.get(&norm_material(good)).copied()
    }

    fn mining_filter(&self, name: &str) -> bool {
        self.mineable.contains(&selling::good_key(&self.mining, name))
    }

    fn choose(&mut self, name: &str) {
        match self.mode {
            Mode::Sell => {
                if !self.picks.iter().any(|p| p == name) {
                    self.picks.push(name.to_string());
                }
            }
            Mode::Buy => self.buy_good = Some(name.to_string()),
        }
        self.search.clear();
    }

    /// Buying or selling, without a good chosen (for the tests of the page).
    #[cfg(test)]
    pub(crate) fn set_buying(&mut self, buying: bool) {
        self.mode = if buying { Mode::Buy } else { Mode::Sell };
        self.buy_good = None;
    }

    /// Switches to buying and chooses a good (for the tests of the page).
    #[cfg(test)]
    pub(crate) fn buy(&mut self, good: &str) {
        self.mode = Mode::Buy;
        self.buy_good = Some(good.to_string());
    }

    /// Takes every good of Mein Lager that has prices and is not chosen yet.
    pub(crate) fn take_from_stock(&mut self) {
        let mut taken = 0;
        for name in selling::goods_in_stock(&self.cargo, None) {
            if self.data.known(&name) && !self.picks.contains(&name) {
                self.picks.push(name);
                taken += 1;
            }
        }
        if selling::has_stolen(&self.cargo) {
            self.nqa = true;
        }
        if taken == 0 {
            self.message = Some(Message { text: t("s_vk_lager_leer"), icon: "package", since: None });
        }
    }

    /// The amounts the places are counted with: Mein Lager, overwritten by what is typed.
    fn own_amounts(&self) -> HashMap<String, f64> {
        let mut own = HashMap::new();
        for (name, text) in &self.amounts {
            if let Some(value) = py_float(&text.replace(',', ".")) {
                let whole = value.trunc();
                if whole > 0.0 && whole <= MAX_AMOUNT {
                    own.insert(name.clone(), whole);
                }
            }
        }
        own
    }
}

// ------------------------------------------------------------ the page

/// The page.
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut SellingState) {
    state.follow(ui);
    state.poll_job(ui);
    trade::quiet(ui, scheme, &t("s_hd_lead_vk"));
    ui.add_space(4.0);
    head(ui, scheme, state);
    ui.add_space(GAP);
    picker(ui, scheme, state);
    ui.add_space(GAP);
    show_message(ui, scheme, state);
    results(ui, scheme, state);
}

fn show_message(ui: &Ui, scheme: &Scheme, state: &mut SellingState) {
    let Some(message) = &mut state.message else { return };
    let now = ui.input(|i| i.time);
    let since = *message.since.get_or_insert(now);
    let shown = widgets::notice(ui.ctx(), scheme, since, message.icon, &message.text, None);
    if !shown.shown {
        state.message = None;
    }
}

fn head(ui: &mut Ui, scheme: &Scheme, state: &mut SellingState) {
    let mut chosen = usize::from(state.mode == Mode::Buy);
    let words = [t("s_vk_m_verkaufen"), t("s_vk_m_kaufen")];
    if trade::toggle_pair(ui, scheme, [&words[0], &words[1]], &mut chosen) {
        let mode = if chosen == 1 { Mode::Buy } else { Mode::Sell };
        if mode != state.mode {
            state.mode = mode;
            state.buy_good = None;
            state.search.clear();
        }
    }
    ui.add_space(4.0);
    let now = state.now();
    let wait = state.wait(now);
    let running = state.job.is_some();
    ui.horizontal_wrapped(|row| {
        let (text, reason) = if running {
            (t("s_vk_holt"), Some(t("s_hd_holt_hint")))
        } else if wait > 0 {
            (trade::countdown(wait), Some(trade::with("s_hd_gesperrt_hint", "zeit", trade::countdown(wait))))
        } else if state.paths.is_none() {
            (t("s_vk_holen"), Some(t("s_hd_ohne_daten")))
        } else {
            (t("s_vk_holen"), None)
        };
        let mut button =
            widgets::Button::new(&text, ButtonKind::Normal, scheme).icon("rotate-cw").blocked(reason.as_deref());
        if wait > 0 && !running {
            button = button.tint(trade::cooldown_color(scheme, wait));
        }
        if row.add(button).clicked()
            && let Some(paths) = state.paths.clone()
        {
            let ctx = row.ctx().clone();
            state.job = Some(Job::spawn(&ctx, "selling-update", move || {
                selling::update(&paths, trade::now(), true, &trade::fetcher())
            }));
        }
        stand_line(row, scheme, state, now);
    });
    if wait > 0 && !running {
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
    }
}

/// "Stand: vor 3 Std." or the hint that a patch lies between the prices and the game.
fn stand_line(ui: &mut Ui, scheme: &Scheme, state: &SellingState, now: f64) {
    let Some(age) = state.data.age(now) else {
        trade::quiet(ui, scheme, &t("s_vk_kein_stand"));
        return;
    };
    let live = state.live.clone();
    if !live.is_empty() && !state.data.build.is_empty() && live != state.data.build {
        let text =
            tf("s_hd_patch", &[("alt", Arg::from(state.data.build.as_str())), ("neu", Arg::from(live.as_str()))]);
        trade::colored(ui, scheme.yellow, &text);
    } else {
        trade::quiet(ui, scheme, &tf("s_vk_stand", &[("alter", Arg::from(trade::age_text(Some(age))))]));
    }
}

fn picker(ui: &mut Ui, scheme: &Scheme, state: &mut SellingState) {
    let entries = match state.mode {
        Mode::Sell => state.data.goods(),
        Mode::Buy => state.data.buy_goods(),
    };
    let more = |n: usize| tf("s_mlg_mehr", &[("n", Arg::from(n))]);
    let hint = t("s_hd_ware_ph");
    let mut search = std::mem::take(&mut state.search);
    let width = ui.available_width() - layout::SAFETY;
    let found =
        widgets::SuggestField::new("handel-ware", &hint, scheme, &more).width(width).show(ui, &mut search, &entries);
    state.search = search;
    if let Some(index) = found.picked
        && let Some(name) = entries.get(index)
    {
        let name = name.clone();
        state.choose(&name);
    }
    if state.mode == Mode::Buy {
        return;
    }
    chips(ui, scheme, state);
    trade::quiet(ui, scheme, &t("s_hd_menge_hint"));
    ui.add_space(4.0);
    widgets::switch_row(ui, scheme, &t("s_vk_nur_nqa"), &mut state.nqa);
    ui.add_space(4.0);
    if ui.add(widgets::Button::new(&t("s_vk_aus_lager"), ButtonKind::Normal, scheme).icon("package")).clicked() {
        state.take_from_stock();
    }
}

/// The chosen goods side by side, each in a lighter box: name, field for the
/// amount, the unit and a cross. Boxes wrap to the next line when the row is full.
fn chips(ui: &mut Ui, scheme: &Scheme, state: &mut SellingState) {
    let mut remove = None;
    let amount_hint = t("s_hd_menge_ph");
    let unit = t("s_vk_scu_kurz");
    // A frame inside a wrapping row squeezes instead of wrapping: the lines are split here by measured width.
    const FIELD: f32 = 80.0;
    let gap = ui.spacing().item_spacing.x;
    let width_of = |ui: &Ui, text: &str| {
        ui.painter()
            .layout_no_wrap(text.to_string(), egui::TextStyle::Body.resolve(ui.style()), color(scheme.fg))
            .size()
            .x
    };
    let unit_w = width_of(ui, &unit);
    let cross_w = ui.spacing().interact_size.y + 4.0;
    let chip_w: Vec<f32> = state
        .picks
        .iter()
        .map(|name| width_of(ui, name) + FIELD + unit_w + cross_w + 3.0 * gap + 2.0 * 8.0 + 2.0)
        .collect();
    let room = ui.available_width();
    let mut lines: Vec<Vec<usize>> = vec![Vec::new()];
    let mut used = 0.0;
    for (i, w) in chip_w.iter().enumerate() {
        let need = if lines.last().is_some_and(|l| l.is_empty()) { *w } else { used + gap + w };
        if need > room && lines.last().is_some_and(|l| !l.is_empty()) {
            lines.push(Vec::new());
            used = *w;
        } else {
            used = need;
        }
        lines.last_mut().expect("one line").push(i);
    }
    for line in lines.iter().filter(|l| !l.is_empty()) {
        ui.horizontal(|row| {
            for &i in line {
                let name = &state.picks[i];
                // The id follows the good, not the place in the list: a typed amount stays with its good.
                row.push_id(("handel-menge", name), |row| {
                    egui::Frame::new()
                        .fill(color(scheme.hover))
                        .stroke(egui::Stroke::new(1.0, color(scheme.accent)))
                        .corner_radius(egui::CornerRadius::same(6))
                        .inner_margin(egui::Margin::symmetric(8, 4))
                        .show(row, |chip| {
                            chip.horizontal(|chip| {
                                chip.label(RichText::new(name).strong().color(color(scheme.accent)));
                                let text = state.amounts.entry(name.clone()).or_default();
                                chip.add(
                                    widgets::text_field(text, &amount_hint, scheme)
                                        .desired_width(FIELD)
                                        .align(egui::Align2::RIGHT_CENTER),
                                );
                                chip.label(RichText::new(&unit).color(color(scheme.sub)));
                                if widgets::icon_button(chip, scheme, "x", &t("s_hd_ware_weg")).clicked() {
                                    remove = Some(i);
                                }
                            });
                        });
                });
            }
        });
        ui.add_space(4.0);
    }
    if let Some(i) = remove {
        let name = state.picks.remove(i);
        state.amounts.remove(&name);
    }
}

// ------------------------------------------------------------ results

fn results(ui: &mut Ui, scheme: &Scheme, state: &mut SellingState) {
    if state.data.is_empty() {
        widgets::empty_state(ui, scheme, "coins", &t("s_vk_kein_stand"));
        return;
    }
    let row_height = ui.text_style_height(&egui::TextStyle::Body) + 10.0;
    let room = layout::list_room(ui, row_height) - widgets::LIST_FRAME_EXTRA;
    let area = egui::ScrollArea::vertical()
        .id_salt(LIST_ID)
        .max_height(room)
        .min_scrolled_height(room)
        .auto_shrink([false, true]);
    let mut jump = None;
    let shown = widgets::list_frame_marked(ui, scheme, |ui| widgets::scroll_area(ui, scheme, area, |ui| {
        ui.set_max_width(ui.available_width() - layout::SAFETY);
        jump = match state.mode {
            Mode::Sell if state.picks.is_empty() => sell_overview(ui, scheme, state),
            Mode::Sell => {
                sell_places(ui, scheme, state);
                None
            }
            Mode::Buy => buy_view(ui, scheme, state),
        };
    }))
    .inner;
    state.list_overflow = trade::overflow_of(&shown);
    if let Some(name) = jump {
        state.choose(&name);
    }
}

fn top_toggle(ui: &mut Ui, scheme: &Scheme, state: &mut SellingState) {
    let mut chosen = usize::from(state.top == Top::Mining);
    let words = [t("s_vk_alle_waren"), t("s_vk_aus_bergbau")];
    if trade::toggle_pair(ui, scheme, [&words[0], &words[1]], &mut chosen) {
        state.top = if chosen == 1 { Top::Mining } else { Top::All };
    }
    ui.add_space(4.0);
}

/// The best prices per SCU of what is asked for; goods that no terminal asks are left out.
fn sell_overview(ui: &mut Ui, scheme: &Scheme, state: &mut SellingState) -> Option<String> {
    let lang = language::current();
    trade::quiet(ui, scheme, &t("s_hd_leer_hint"));
    ui.add_space(4.0);
    top_toggle(ui, scheme, state);
    let mut scu: Vec<(f64, String)> = Vec::new();
    let mut pieces: Vec<(f64, String)> = Vec::new();
    for name in state.data.goods() {
        if !selling::in_top_list(&name) || (state.top == Top::Mining && !state.mining_filter(&name)) {
            continue;
        }
        let price = state.data.best_price(&name, false);
        if price <= 0.0 {
            continue;
        }
        match state.piece_size(&name) {
            Some(size) => pieces.push((price * size, name)),
            None => scu.push((price, name)),
        }
    }
    let by_price_then_name = |a: &(f64, String), b: &(f64, String)| b.0.total_cmp(&a.0).then_with(|| b.1.cmp(&a.1));
    scu.sort_by(by_price_then_name);
    pieces.sort_by(by_price_then_name);
    if state.top == Top::All {
        scu.truncate(TOP_GOODS);
        pieces.truncate(TOP_PIECES);
    }
    if scu.is_empty() && pieces.is_empty() {
        widgets::empty_state(ui, scheme, "hammer", &t("s_hd_keine_bergbau"));
        return None;
    }
    let mut chosen = None;
    for (title, rows, each) in [("s_hd_spitze", &scu, "s_vk_je_scu"), ("s_hd_spitze_stueck", &pieces, "s_vk_je_stueck")]
    {
        if rows.is_empty() {
            continue;
        }
        ui.label(RichText::new(t(title)).strong());
        for (n, (price, name)) in rows.iter().enumerate() {
            let text = tf(each, &[("preis", Arg::from(trade::auec(lang, *price)))]);
            if top_row(ui, scheme, n % 2 == 1, &text, name, None) {
                chosen = Some(name.clone());
            }
        }
        ui.add_space(GAP);
    }
    chosen
}

/// A row of a top list: the price at the left, the good, perhaps a text at the right.
fn top_row(ui: &mut Ui, scheme: &Scheme, stripe: bool, price: &str, name: &str, right: Option<&str>) -> bool {
    let height = ui.text_style_height(&egui::TextStyle::Body) + 10.0;
    let (rect, clicked) = trade::clickable_row(ui, scheme, stripe, height);
    let mut inner = trade::row_content(ui, rect, right.map(|r| RichText::new(r).color(color(scheme.sub))));
    inner.label(RichText::new(price).color(color(scheme.accent)));
    inner.add_space(12.0);
    inner.add(egui::Label::new(name).truncate());
    clicked
}

fn sell_places(ui: &mut Ui, scheme: &Scheme, state: &mut SellingState) {
    let lang = language::current();
    let now = state.now();
    let places = state.data.places_for(&state.picks, state.nqa, now);
    if places.is_empty() {
        trade::quiet(ui, scheme, &t("s_vk_keine_orte"));
        return;
    }
    let own = state.own_amounts();
    let mut amounts: HashMap<String, f64> = HashMap::new();
    for (name, value) in selling::cargo_amounts(&state.cargo, None).iter() {
        amounts.insert(name.to_string(), *value);
    }
    amounts.extend(own.iter().map(|(k, v)| (k.clone(), *v)));
    // A profit only for goods of Mein Lager with a price paid: a typed amount has none.
    let paid = selling::cargo_costs(&state.cargo, None);
    let costed = selling::cargo_costed_amounts(&state.cargo, None);
    let mut bought: HashMap<String, (f64, f64)> = HashMap::new();
    for (name, cost) in paid.iter() {
        if let Some(quantity) = costed.get(name)
            && !own.contains_key(name)
        {
            bought.insert(name.to_string(), (*cost, *quantity));
        }
    }
    if state.picks.len() == 1 {
        let name = &state.picks[0];
        if let Some(size) = state.piece_size(name) {
            let best = state.data.best_price(name, false);
            if best > 0.0 {
                let text = tf(
                    "s_hd_stueck_hint",
                    &[
                        ("groesse", Arg::from(formats::amount(lang, size, "").trim_end().to_string())),
                        ("preis", Arg::from(trade::auec(lang, best * size))),
                    ],
                );
                trade::colored(ui, scheme.yellow, &text);
                ui.add_space(4.0);
            }
        }
    }
    let wanted = state.picks.len();
    for (n, place) in places.iter().take(MAX_PLACES).enumerate() {
        place_card(ui, scheme, lang, n, place, wanted, &amounts, &bought);
        ui.add_space(GAP);
    }
}

#[allow(clippy::too_many_arguments)]
fn place_card(
    ui: &mut Ui,
    scheme: &Scheme,
    lang: Lang,
    number: usize,
    place: &Place,
    wanted: usize,
    amounts: &HashMap<String, f64>,
    bought: &HashMap<String, (f64, f64)>,
) {
    widgets::list_frame(ui, scheme, |ui| {
        ui.set_min_width(ui.available_width() - layout::SAFETY);
        ui.horizontal_wrapped(|row| {
            let tone = if place.count >= wanted { scheme.accent } else { scheme.sub };
            row.label(RichText::new(format!("{}/{}", place.count, wanted)).strong().color(color(tone)));
            row.label(RichText::new(&place.terminal).strong());
            let location: Vec<&str> =
                [place.place.as_str(), place.system.as_str()].into_iter().filter(|s| !s.is_empty()).collect();
            if !location.is_empty() {
                row.label(RichText::new(location.join(" · ")).color(color(scheme.sub)));
            }
            if place.nqa {
                row.label(RichText::new(t("s_vk_nqa_marke")).color(color(scheme.yellow)));
            }
            if let Some(age) = place.age {
                let tone = if age > selling::OLD { scheme.yellow } else { scheme.sub };
                row.label(RichText::new(trade::age_text(Some(age))).small().color(color(tone)));
            }
        });
        let mut proceeds = 0.0;
        let mut profit: Option<f64> = None;
        let has_amounts = place.hits.iter().any(|hit| amounts.get(&hit.good).is_some_and(|m| *m != 0.0));
        egui::Grid::new(("handel-ort", number)).num_columns(4).spacing([14.0, 2.0]).show(ui, |grid| {
            if number == 0 {
                grid.label("");
                right(grid, |g| {
                    if has_amounts {
                        g.label(RichText::new(t("s_hl_sp_menge")).strong());
                    }
                });
                right(grid, |g| {
                    g.label(RichText::new(t("s_hl_sp_je_scu")).strong());
                });
                right(grid, |g| {
                    if has_amounts {
                        g.label(RichText::new(t("s_hl_sp_gesamt")).strong());
                    }
                });
                grid.end_row();
            }
            for hit in &place.hits {
                let amount = amounts.get(&hit.good).copied().filter(|m| *m != 0.0);
                let total = amount.unwrap_or(0.0) * hit.price;
                proceeds += total;
                if let Some((cost, quantity)) = bought.get(&hit.good) {
                    profit = Some(profit.unwrap_or(0.0) + (hit.price - cost) * quantity);
                }
                let (text, tone) = match hit.fill {
                    Some((key, warning)) => {
                        (format!("{}  {}", hit.good, t(key)), if warning { scheme.red_text() } else { scheme.yellow })
                    }
                    None => (hit.good.clone(), scheme.sub),
                };
                grid.label(RichText::new(text).color(color(tone)));
                right(grid, |g| {
                    if let Some(amount) = amount {
                        g.label(formats::amount(lang, amount, ""));
                    }
                });
                right(grid, |g| {
                    g.label(trade::plain(lang, hit.price));
                });
                right(grid, |g| {
                    if amount.is_some() {
                        g.label(trade::plain(lang, total));
                    }
                });
                grid.end_row();
            }
        });
        if proceeds != 0.0 {
            totals(ui, &tf("s_vk_erloes", &[("summe", Arg::from(trade::plain(lang, proceeds)))]), color(scheme.accent));
        }
        if let Some(profit) = profit {
            let sign = if profit > 0.0 { "+" } else { "" };
            let tone = if profit >= 0.0 { scheme.accent } else { scheme.red_text() };
            totals(
                ui,
                &tf("s_vk_gewinn", &[("summe", Arg::from(format!("{sign}{}", trade::plain(lang, profit))))]),
                color(tone),
            );
        }
    });
}

fn right(grid: &mut Ui, add: impl FnOnce(&mut Ui)) {
    grid.with_layout(Layout::right_to_left(Align::Center), add);
}

fn totals(ui: &mut Ui, text: &str, tone: egui::Color32) {
    ui.with_layout(Layout::right_to_left(Align::Min), |side| {
        side.label(RichText::new(text).small().color(tone));
    });
}

// ------------------------------------------------------------ buying

fn buy_view(ui: &mut Ui, scheme: &Scheme, state: &mut SellingState) -> Option<String> {
    if let Some(name) = state.buy_good.clone()
        && state.data.buy_known(&name)
    {
        buy_places(ui, scheme, state, &name);
        return None;
    }
    state.buy_good = None;
    buy_overview(ui, scheme, state)
}

fn buy_overview(ui: &mut Ui, scheme: &Scheme, state: &mut SellingState) -> Option<String> {
    let lang = language::current();
    trade::quiet(ui, scheme, &t("s_hd_k_leer_hint"));
    ui.add_space(4.0);
    top_toggle(ui, scheme, state);
    // (cheapest price, name, best price somebody pays)
    let mut rows: Vec<(f64, String, f64)> = Vec::new();
    for name in state.data.buy_goods() {
        if !selling::in_top_list(&name) || (state.top == Top::Mining && !state.mining_filter(&name)) {
            continue;
        }
        let price = state.data.cheapest_price(&name, false);
        if price > 0.0 {
            rows.push((price, name.clone(), state.data.best_price(&name, false)));
        }
    }
    // The widest margin first; goods nobody buys last, the cheapest of them first.
    rows.sort_by(|a, b| {
        let margin = |r: &(f64, String, f64)| (r.2 > 0.0).then_some(r.2 - r.0);
        let (ma, mb) = (margin(a), margin(b));
        ma.is_none()
            .cmp(&mb.is_none())
            .then_with(|| mb.unwrap_or(0.0).total_cmp(&ma.unwrap_or(0.0)))
            .then_with(|| a.0.total_cmp(&b.0))
            .then_with(|| a.1.cmp(&b.1))
    });
    if state.top == Top::All {
        rows.truncate(TOP_GOODS);
    }
    if rows.is_empty() {
        let key = if state.top == Top::Mining { "s_hd_keine_bergbau" } else { "s_vk_kein_stand" };
        widgets::empty_state(ui, scheme, "hammer", &t(key));
        return None;
    }
    ui.label(RichText::new(t("s_hd_k_spitze")).strong());
    let mut chosen = None;
    for (n, (price, name, best)) in rows.iter().enumerate() {
        let text = tf("s_vk_je_scu", &[("preis", Arg::from(trade::auec(lang, *price)))]);
        let right = (*best > 0.0).then(|| tf("s_vk_k_ankauf_bis", &[("preis", Arg::from(trade::auec(lang, *best)))]));
        if top_row(ui, scheme, n % 2 == 1, &text, name, right.as_deref()) {
            chosen = Some(name.clone());
        }
    }
    chosen
}

fn buy_places(ui: &mut Ui, scheme: &Scheme, state: &mut SellingState, name: &str) {
    let lang = language::current();
    let now = state.now();
    trade::two_sided(
        ui,
        |left| {
            left.label(RichText::new(tf("s_hd_k_orte", &[("ware", Arg::from(name))])).strong());
        },
        |side| {
            if side.add(widgets::Button::new(&t("s_vk_k_uebersicht"), ButtonKind::Normal, scheme)).clicked() {
                state.buy_good = None;
            }
        },
    );
    ui.add_space(4.0);
    let places = state.data.buy_places(name, false, now);
    if places.is_empty() {
        trade::quiet(ui, scheme, &t("s_vk_k_keine_orte"));
        return;
    }
    for (n, place) in places.iter().take(MAX_PLACES).enumerate() {
        let price = tf("s_vk_je_scu", &[("preis", Arg::from(trade::auec(lang, place.price)))]);
        widgets::list_frame(ui, scheme, |ui| {
            ui.set_min_width(ui.available_width() - layout::SAFETY);
            ui.horizontal_wrapped(|row| {
                row.label(RichText::new(price).color(color(scheme.accent)));
                row.label(&place.terminal);
                let location: Vec<&str> =
                    [place.place.as_str(), place.system.as_str()].into_iter().filter(|s| !s.is_empty()).collect();
                if !location.is_empty() {
                    row.label(RichText::new(location.join(" · ")).color(color(scheme.sub)));
                }
                if place.nqa {
                    row.label(RichText::new(t("s_vk_nqa_marke")).color(color(scheme.yellow)));
                }
            });
            ui.horizontal_wrapped(|row| {
                if let Some((key, grade)) = place.level {
                    let mut text = t(key);
                    if place.stock > 0 {
                        text = format!(
                            "{text} · {}",
                            tf(
                                "s_vk_k_vorrat_scu",
                                &[(
                                    "n",
                                    Arg::from(formats::amount(lang, place.stock as f64, "").trim_end().to_string())
                                )]
                            )
                        );
                    }
                    let tone: Color = match grade {
                        "warnung" => scheme.red_text(),
                        "hinweis" => scheme.yellow,
                        _ => scheme.sub,
                    };
                    row.label(RichText::new(text).small().color(color(tone)));
                }
                if let Some(age) = place.age {
                    let tone = if age > selling::OLD { scheme.yellow } else { scheme.sub };
                    row.label(RichText::new(trade::age_text(Some(age))).small().color(color(tone)));
                }
                let sizes: Vec<&str> = place.boxes.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
                if !sizes.is_empty() {
                    let text = tf("s_vk_k_kisten", &[("groessen", Arg::from(sizes.join(", ")))]);
                    row.label(RichText::new(text).small().color(color(scheme.sub)));
                }
            });
        });
        ui.add_space(if n + 1 < places.len().min(MAX_PLACES) { 4.0 } else { 0.0 });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The colour the countdown text is drawn in, with the button locked for `seconds`.
    fn countdown_colour(seconds: i64) -> Option<egui::Color32> {
        fn walk(shape: &egui::Shape, wanted: &str, found: &mut Option<egui::Color32>) {
            match shape {
                egui::Shape::Text(text) if text.galley.text() == wanted => *found = Some(text.fallback_color),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, wanted, found)),
                _ => {}
            }
        }
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let scheme = versekit_core::theme::current();
        let mut state = SellingState { wait_value: seconds, wait_checked: trade::now(), ..SellingState::default() };
        let wanted = trade::countdown(seconds);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| head(ui, scheme, &mut state));
        output.textures_delta.clear();
        let mut found = None;
        output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &wanted, &mut found));
        found
    }

    #[test]
    fn the_countdown_ripens_from_muted_over_gold_to_green_and_is_never_red() {
        let scheme = versekit_core::theme::current();
        assert_eq!(countdown_colour(35 * 60), Some(color(scheme.sub)), "far away");
        assert_eq!(countdown_colour(20 * 60), Some(color(scheme.gold)), "something happens");
        assert_eq!(countdown_colour(3 * 60), Some(color(scheme.accent)), "back soon");
        for seconds in [1, 299, 301, 1799, 1801, 7200] {
            assert_ne!(trade::cooldown_color(scheme, seconds), scheme.red, "{seconds}");
        }
    }
}
