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
use crate::selling_route::{self, Inputs, RouteState};
use crate::trade::{self, Job, Polled};
use crate::widgets::{self, ButtonKind};

const SETTING_VIEW: &str = "verkauf_ansicht";
const SETTING_MODE: &str = "verkauf_modus";
const SETTING_TOP: &str = "verkauf_spitze";
/// The unit of Mein Lager (`cscu` or `scu`), kept by its page.
const SETTING_UNIT: &str = "lager_einheit";
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
/// Room a line of chosen goods keeps free at its right.
const CHIP_SLACK: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Sell,
    Buy,
}

/// The unit an amount is typed in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unit {
    Scu,
    /// A hundredth of an SCU.
    Cscu,
    Piece,
}

/// How the goods chosen for selling are answered: the places that take them, or one route over a few stations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum View {
    Places,
    Route,
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
    view: View,
    route: RouteState,
    /// Counts the reloads of the prices, so that a route is calculated again when they changed.
    version: u64,
    top: Top,
    search: String,
    picks: Vec<String>,
    amounts: HashMap<String, String>,
    /// Is the amount of a good that counts in SCU typed in cSCU (the setting of Mein Lager)?
    cscu: bool,
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
            view: View::Places,
            route: RouteState::default(),
            version: 0,
            top: Top::All,
            search: String::new(),
            picks: Vec::new(),
            amounts: HashMap::new(),
            cscu: false,
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
        state.view = if paths.setting(SETTING_VIEW).as_deref() == Some("route") { View::Route } else { View::Places };
        state.top = if paths.setting(SETTING_TOP).as_deref() == Some("bergbau") { Top::Mining } else { Top::All };
        state.cscu = paths.setting(SETTING_UNIT).as_deref() == Some("cscu");
        state.reload();
        state
    }

    /// Prices and typed amounts for the previews and the layout check: an amount in a part of an SCU, a piece good
    /// with a known size, one without and one with a part of a piece typed.
    pub fn sample() -> SellingState {
        let now = trade::now() as i64;
        let offers = |price: f64| format!(r#"[{{"t":"7","n":"TDD","p":{price},"d":{now},"k":"","z":3}}]"#);
        let text = format!(
            r#"{{"terminals":{{"7":{{"o":"Area18","s":"Stanton","n":"TDD","q":false}}}},"geholt":{now},"waren":{{
                "Iron":{},"Kopion Horn":{},"Saldynium (Ore)":{},"Valakkar Pearl (Apex)":{}}}}}"#,
            offers(2_000.0),
            offers(45_000.0),
            offers(34_000_000.0),
            offers(9_000_000.0),
        );
        let mut state = SellingState { cscu: true, ..SellingState::default() };
        if let Ok(data) = versekit_core::jsonio::loads(&text) {
            state.data = Selling::from_value(&data);
        }
        let goods = [
            ("Iron", "10", false),
            ("Kopion Horn", "12", true),
            ("Saldynium (Ore)", "3", true),
            ("Valakkar Pearl (Apex)", "0,5", true),
        ];
        for (name, typed, piece) in goods {
            state.picks.push(name.to_string());
            state.amounts.insert(name.to_string(), typed.to_string());
            if piece {
                state.stock.add_recipe_materials([(name.to_string(), true, 0.0, 0.0)]);
            }
        }
        for name in ["Kopion Horn", "Valakkar Pearl (Apex)"] {
            state.sizes.insert(norm_material(name), 0.001);
        }
        state
    }

    /// The route view over two stations, for the layout check: each good sells best at the other one, and the
    /// stations lack a known distance.
    pub fn show_route_sample(&mut self) -> bool {
        let now = trade::now() as i64;
        let offer = |terminal: &str, name: &str, price: f64, level: i64| {
            format!(r#"{{"t":"{terminal}","n":"{name}","p":{price},"d":{now},"k":"","z":{level}}}"#)
        };
        let text = format!(
            r#"{{"terminals":{{
                "7":{{"o":"Area18","s":"Stanton","n":"TDD","q":false}},
                "8":{{"o":"Lorville","s":"Stanton","n":"Teasa Spaceport","q":true}}}},"geholt":{now},"waren":{{
                "Iron":[{},{}],"Kopion Horn":[{},{}],"Saldynium (Ore)":[{},{}],"Valakkar Pearl (Apex)":[{},{}]}}}}"#,
            offer("7", "TDD", 1_500.0, 3),
            offer("8", "Teasa Spaceport", 2_200.0, 3),
            offer("7", "TDD", 45_000.0, 3),
            offer("8", "Teasa Spaceport", 40_000.0, 3),
            offer("7", "TDD", 34_000_000.0, 3),
            offer("8", "Teasa Spaceport", 30_000_000.0, 3),
            offer("7", "TDD", 8_000_000.0, 3),
            offer("8", "Teasa Spaceport", 9_000_000.0, 5),
        );
        let Ok(data) = versekit_core::jsonio::loads(&text) else { return false };
        self.data = Selling::from_value(&data);
        self.version += 1;
        self.view = View::Route;
        true
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
            (SETTING_VIEW, Value::str(if self.view == View::Route { "route" } else { "orte" })),
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
        self.version += 1;
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
        let cscu = self.paths.as_ref().map(|paths| paths.setting(SETTING_UNIT).as_deref() == Some("cscu"));
        if let Some(cscu) = cscu {
            self.change_unit(cscu);
        }
        if let Some(files) = self.files()
            && self.seen != [modified(&files[0]), modified(&files[1])]
        {
            self.reload();
        }
    }

    fn poll_job(&mut self, ui: &Ui) {
        self.route.poll(ui);
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

    /// Switches between SCU and cSCU; what is typed in SCU goods is rewritten so it stays the same amount.
    fn change_unit(&mut self, cscu: bool) {
        if cscu == self.cscu {
            return;
        }
        let factor = if cscu { 1.0 / stock::CSCU } else { stock::CSCU };
        for (name, text) in &mut self.amounts {
            if self.stock.is_piece(name) || stock::is_calculation(text) {
                continue;
            }
            if let Some(value) = py_float(&text.replace(',', ".")) {
                *text = format!("{}", (value * factor * 1.0e6).round() / 1.0e6);
            }
        }
        self.cscu = cscu;
    }

    /// Enter in the field of a good: a calculation in it (`+8`, `30+2`) becomes its result, as in Mein Lager. A plain
    /// number stays as typed, and so does a calculation that gives no amount.
    fn settle(&mut self, good: &str) {
        let Some(text) = self.amounts.get(good) else { return };
        if !stock::is_calculation(text) {
            return;
        }
        let Some(value) = self.calculated(good, text).filter(|value| *value > 0.0) else { return };
        let decimal = if language::current() == Lang::De { ',' } else { '.' };
        let mut shown = stock::amount_text(value, self.unit_of(good) == Unit::Piece, decimal);
        if shown.contains(decimal) {
            shown.truncate(shown.trim_end_matches('0').trim_end_matches(decimal).len());
        }
        self.amounts.insert(good.to_string(), shown);
    }

    /// How the amount of a good is typed: pieces for a piece good, else SCU or cSCU as Mein Lager is set.
    fn unit_of(&self, good: &str) -> Unit {
        if self.stock.is_piece(good) {
            Unit::Piece
        } else if self.cscu {
            Unit::Cscu
        } else {
            Unit::Scu
        }
    }

    /// The word behind the field of a good.
    fn unit_word(&self, good: &str) -> String {
        match self.unit_of(good) {
            Unit::Piece => t("s_lg_stueck"),
            Unit::Cscu => t("s_lg_cscu"),
            Unit::Scu => t("s_vk_scu_kurz"),
        }
    }

    /// What Mein Lager holds of a good, in the unit of its field (pieces need the size of a piece).
    fn held(&self, good: &str, unit: Unit) -> f64 {
        let scu = selling::cargo_amounts(&self.cargo, None).get(good).copied().unwrap_or(0.0);
        match unit {
            Unit::Scu => scu,
            Unit::Cscu => scu / stock::CSCU,
            Unit::Piece => self.piece_size(good).map_or(0.0, |size| scu / size),
        }
    }

    /// What the field says as a number, calculated as in Mein Lager (`stock::calculate`): `4,5` is the amount,
    /// `30+2` is worked out, `+8` and `-2` book onto what Mein Lager holds.
    fn calculated(&self, good: &str, text: &str) -> Option<f64> {
        let value = stock::calculate(text, self.held(good, self.unit_of(good)))?;
        value.is_finite().then(|| stock::py_round(value, 6))
    }

    /// What is typed for a good: the amount in the unit of the field and, where it can be told, in SCU.
    fn typed(&self, good: &str, text: &str) -> Option<(f64, Option<f64>)> {
        let value = self.calculated(good, text)?;
        let unit = self.unit_of(good);
        let value = if unit == Unit::Piece { value.trunc() } else { value };
        let scu = match unit {
            Unit::Scu => Some(value),
            Unit::Cscu => Some(value * stock::CSCU),
            Unit::Piece => self.piece_size(good).map(|size| value * size),
        };
        let fits = value > 0.0 && value <= MAX_AMOUNT && scu.is_none_or(|scu| scu <= MAX_AMOUNT);
        fits.then_some((value, scu))
    }

    /// The amounts in SCU the places are counted with: Mein Lager, overwritten by what is typed.
    fn own_amounts(&self) -> HashMap<String, f64> {
        let typed = |(name, text): (&String, &String)| Some((name.clone(), self.typed(name, text)?.1?));
        self.amounts.iter().filter_map(typed).collect()
    }

    /// The amounts in SCU of the chosen goods the places and the route are counted with: Mein Lager, overwritten
    /// by what is typed; pieces without a known size of a piece stay without SCU.
    fn scu_amounts(&self) -> HashMap<String, f64> {
        let own = self.own_amounts();
        let mut amounts: HashMap<String, f64> = HashMap::new();
        for (name, value) in selling::cargo_amounts(&self.cargo, None).iter() {
            amounts.insert(name.to_string(), *value);
        }
        amounts.extend(own.iter().map(|(k, v)| (k.clone(), *v)));
        let counts = self.own_pieces();
        amounts.retain(|name, _| !counts.contains_key(name) || own.contains_key(name));
        amounts
    }

    /// The pieces typed for the goods that count in pieces.
    fn own_pieces(&self) -> HashMap<String, f64> {
        let typed = |(name, text): (&String, &String)| {
            (self.unit_of(name) == Unit::Piece).then_some(())?;
            Some((name.clone(), self.typed(name, text)?.0))
        };
        self.amounts.iter().filter_map(typed).collect()
    }

    /// The chosen piece goods with a typed amount but no known size of a piece: the SCU cannot be told.
    fn pieces_without_size(&self) -> Vec<String> {
        let own = self.own_amounts();
        let mut names: Vec<String> = self.own_pieces().into_keys().filter(|name| !own.contains_key(name)).collect();
        names.sort();
        names
    }

    /// The chosen piece goods with a part of a piece typed (a decimal): pieces count whole.
    fn parts_of_a_piece(&self) -> Vec<String> {
        self.picks
            .iter()
            .filter(|name| self.unit_of(name) == Unit::Piece)
            .filter(|name| {
                let text = self.amounts.get(*name).map_or("", String::as_str);
                self.calculated(name, text).is_some_and(|value| value > 0.0 && value.fract() != 0.0)
            })
            .cloned()
            .collect()
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
    ui.add_space(GAP);
    let mut chosen = usize::from(state.view == View::Route);
    let words = [t("s_vk_ro_ansicht_orte"), t("s_vk_ro_ansicht_route")];
    if trade::toggle_pair(ui, scheme, [&words[0], &words[1]], &mut chosen) {
        state.view = if chosen == 1 { View::Route } else { View::Places };
    }
    if state.view == View::Route {
        ui.add_space(4.0);
        let amounts = state.scu_amounts();
        let inputs = Inputs {
            data: &state.data,
            picks: &state.picks,
            amounts: &amounts,
            nqa: state.nqa,
            paths: state.paths.as_ref(),
            version: state.version,
        };
        selling_route::controls(ui, scheme, &mut state.route, &inputs);
    }
}

/// The chosen goods side by side, each in a lighter box: name, field for the
/// amount, the unit and a cross. Boxes wrap to the next line when the row is full.
fn chips(ui: &mut Ui, scheme: &Scheme, state: &mut SellingState) {
    let mut remove = None;
    let mut entered: Option<String> = None;
    let amount_hint = t("s_hd_menge_ph");
    let units: Vec<String> = state.picks.iter().map(|name| state.unit_word(name)).collect();
    // A frame inside a wrapping row squeezes instead of wrapping: the lines are split here by measured width.
    const FIELD: f32 = 80.0;
    let gap = ui.spacing().item_spacing.x;
    let width_of = |ui: &Ui, text: &str| {
        ui.painter()
            .layout_no_wrap(text.to_string(), egui::TextStyle::Body.resolve(ui.style()), color(scheme.fg))
            .size()
            .x
    };
    let cross_w = ui.spacing().interact_size.y + 4.0;
    let chip_w: Vec<f32> = state
        .picks
        .iter()
        .zip(&units)
        .map(|(name, unit)| width_of(ui, name) + FIELD + width_of(ui, unit) + cross_w + 3.0 * gap + 2.0 * 8.0 + 2.0)
        .collect();
    // The boxes are a little wider than the sum of their parts (the margins of the field); a line keeps that much free.
    let room = ui.available_width() - CHIP_SLACK;
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
                                let field = chip.add(
                                    widgets::text_field(text, &amount_hint, scheme)
                                        .desired_width(FIELD)
                                        .align(egui::Align2::RIGHT_CENTER),
                                );
                                if field.lost_focus() && chip.input(|input| input.key_pressed(egui::Key::Enter)) {
                                    entered = Some(name.clone());
                                }
                                chip.label(RichText::new(&units[i]).color(color(scheme.sub)));
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
    if let Some(good) = entered {
        state.settle(&good);
    }
    let parts = state.parts_of_a_piece();
    if !parts.is_empty() {
        trade::colored(ui, scheme.yellow, &tf("s_vk_nur_ganze_stuecke", &[("waren", Arg::from(parts.join(", ")))]));
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
    let shown = widgets::list_frame_marked(ui, scheme, |ui| {
        widgets::scroll_area(ui, scheme, area, |ui| {
            ui.set_max_width(ui.available_width() - layout::SAFETY);
            jump = match state.mode {
                Mode::Sell if state.picks.is_empty() => sell_overview(ui, scheme, state),
                Mode::Sell if state.view == View::Route => {
                    let amounts = state.scu_amounts();
                    let inputs = Inputs {
                        data: &state.data,
                        picks: &state.picks,
                        amounts: &amounts,
                        nqa: state.nqa,
                        paths: state.paths.as_ref(),
                        version: state.version,
                    };
                    selling_route::results(ui, scheme, &mut state.route, &inputs);
                    None
                }
                Mode::Sell => {
                    sell_places(ui, scheme, state);
                    None
                }
                Mode::Buy => buy_view(ui, scheme, state),
            };
        })
    })
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
    let amounts = state.scu_amounts();
    let counts = state.own_pieces();
    // A profit only for goods of Mein Lager with a price paid: a typed amount has none.
    let paid = selling::cargo_costs(&state.cargo, None);
    let costed = selling::cargo_costed_amounts(&state.cargo, None);
    let mut bought: HashMap<String, (f64, f64)> = HashMap::new();
    for (name, cost) in paid.iter() {
        if let Some(quantity) = costed.get(name)
            && !own.contains_key(name)
            && !counts.contains_key(name)
        {
            bought.insert(name.to_string(), (*cost, *quantity));
        }
    }
    let unsized_pieces = state.pieces_without_size();
    if !unsized_pieces.is_empty() {
        trade::colored(
            ui,
            scheme.yellow,
            &tf("s_vk_stueck_ohne_groesse", &[("waren", Arg::from(unsized_pieces.join(", ")))]),
        );
        ui.add_space(4.0);
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
        place_card(ui, scheme, lang, n, place, wanted, &Amounts { scu: &amounts, pieces: &counts }, &bought);
        ui.add_space(GAP);
    }
}

/// The amounts a place card counts with: in SCU for the sum, in pieces for the goods that count in pieces.
struct Amounts<'a> {
    scu: &'a HashMap<String, f64>,
    pieces: &'a HashMap<String, f64>,
}

#[allow(clippy::too_many_arguments)]
fn place_card(
    ui: &mut Ui,
    scheme: &Scheme,
    lang: Lang,
    number: usize,
    place: &Place,
    wanted: usize,
    amounts: &Amounts,
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
        let has_amounts = place
            .hits
            .iter()
            .any(|hit| amounts.scu.get(&hit.good).is_some_and(|m| *m != 0.0) || amounts.pieces.contains_key(&hit.good));
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
                let amount = amounts.scu.get(&hit.good).copied().filter(|m| *m != 0.0);
                let pieces = amounts.pieces.get(&hit.good).copied();
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
                    if let Some(pieces) = pieces {
                        g.label(formats::amount(lang, pieces, &t("s_lg_stueck")));
                    } else if let Some(amount) = amount {
                        g.label(formats::amount(lang, amount, &t("s_vk_scu_kurz")));
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

    /// A page state with one chosen good and the amount typed for it.
    fn typed(good: &str, text: &str, piece_size: Option<f64>, cscu: bool) -> SellingState {
        let mut state = SellingState::default();
        state.picks.push(good.to_string());
        state.amounts.insert(good.to_string(), text.to_string());
        state.cscu = cscu;
        if let Some(size) = piece_size {
            state.stock.add_recipe_materials([(good.to_string(), true, 0.0, 0.0)]);
            state.sizes.insert(norm_material(good), size);
        }
        state
    }

    fn scu_of(state: &SellingState, good: &str) -> Option<f64> {
        state.own_amounts().get(good).copied()
    }

    #[test]
    fn a_part_of_an_scu_is_kept_and_a_comma_reads_as_a_point() {
        assert_eq!(scu_of(&typed("Iron", "0,1", None, false), "Iron"), Some(0.1));
        assert_eq!(scu_of(&typed("Iron", "2.5", None, false), "Iron"), Some(2.5));
        assert_eq!(scu_of(&typed("Iron", "120", None, false), "Iron"), Some(120.0));
        for nothing in ["0", "-3", "", "abc", "1e30", "nan", "inf"] {
            assert_eq!(scu_of(&typed("Iron", nothing, None, false), "Iron"), None, "{nothing:?}");
        }
    }

    #[test]
    fn with_the_unit_cscu_the_typed_amount_counts_a_hundredth_of_an_scu() {
        let got = scu_of(&typed("Iron", "250", None, true), "Iron").expect("an amount");
        assert!((got - 2.5).abs() < 1e-9, "{got}");
        let got = scu_of(&typed("Iron", "5", None, true), "Iron").expect("an amount");
        assert!((got - 0.05).abs() < 1e-9, "{got}");
    }

    #[test]
    fn a_piece_good_counts_whole_pieces_and_its_size_gives_the_scu() {
        let state = typed("Saldynium (Ore)", "3", Some(0.001), false);
        let got = scu_of(&state, "Saldynium (Ore)").expect("an amount");
        assert!((got - 0.003).abs() < 1e-12, "{got}");
        let cscu = typed("Saldynium (Ore)", "3", Some(0.001), true);
        assert_eq!(scu_of(&cscu, "Saldynium (Ore)"), scu_of(&state, "Saldynium (Ore)"), "cSCU leaves pieces alone");
        let part = scu_of(&typed("Saldynium (Ore)", "3,7", Some(0.001), false), "Saldynium (Ore)").expect("an amount");
        assert!((part - 0.003).abs() < 1e-12, "a part of a piece is cut: {part}");
        assert_eq!(scu_of(&typed("Saldynium (Ore)", "0,1", Some(0.001), false), "Saldynium (Ore)"), None);
    }

    /// The texts a function draws, in one list.
    fn drawn(draw: impl FnMut(&mut Ui)) -> Vec<String> {
        fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => out.push(text.galley.text().to_string()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let mut draw = draw;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| draw(ui));
        output.textures_delta.clear();
        let mut out = Vec::new();
        output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut out));
        out
    }

    /// A price list with one terminal that pays `price` per SCU for the good.
    fn priced(good: &str, price: f64) -> Selling {
        let now = trade::now() as i64;
        let text = format!(
            r#"{{"terminals":{{"7":{{"o":"Area18","s":"Stanton","n":"TDD","q":false}}}},
               "waren":{{"{good}":[{{"t":"7","n":"TDD","p":{price},"d":{now},"k":"","z":3}}]}},"geholt":{now}}}"#
        );
        Selling::from_value(&versekit_core::jsonio::loads(&text).expect("json"))
    }

    #[test]
    fn the_proceeds_of_pieces_are_the_price_per_scu_times_the_scu_of_the_pieces() {
        let lang = language::current();
        let mut state = typed("Saldynium (Ore)", "3", Some(0.001), false);
        state.data = priced("Saldynium (Ore)", 34_000_000.0);
        let scheme = versekit_core::theme::current();
        let shown = drawn(|ui| sell_places(ui, scheme, &mut state));
        let wanted = trade::plain(lang, 34_000_000.0 * 0.003);
        let wrong = trade::plain(lang, 34_000_000.0 * 3.0);
        assert!(shown.iter().any(|s| s.contains(&wanted)), "{wanted} in {shown:?}");
        assert!(!shown.iter().any(|s| s.contains(&wrong)), "{wrong} in {shown:?}");
        let pieces = formats::amount(lang, 3.0, &t("s_lg_stueck"));
        assert!(shown.contains(&pieces), "the amount stands in pieces: {pieces:?} in {shown:?}");
    }

    #[test]
    fn pieces_without_a_known_size_show_the_pieces_no_sum_and_say_why() {
        let lang = language::current();
        let mut state = SellingState::default();
        state.picks.push("Saldynium (Ore)".to_string());
        state.amounts.insert("Saldynium (Ore)".to_string(), "3".to_string());
        state.stock.add_recipe_materials([("Saldynium (Ore)".to_string(), true, 0.0, 0.0)]);
        state.data = priced("Saldynium (Ore)", 34_000_000.0);
        assert!(state.own_amounts().is_empty(), "no SCU is guessed");
        assert_eq!(state.own_pieces().get("Saldynium (Ore)"), Some(&3.0));
        assert_eq!(state.pieces_without_size(), ["Saldynium (Ore)"]);
        let scheme = versekit_core::theme::current();
        let shown = drawn(|ui| sell_places(ui, scheme, &mut state));
        let hint = tf("s_vk_stueck_ohne_groesse", &[("waren", Arg::from("Saldynium (Ore)"))]);
        assert!(shown.iter().any(|s| s.contains(&hint)), "{hint:?} in {shown:?}");
        assert!(shown.contains(&formats::amount(lang, 3.0, &t("s_lg_stueck"))), "{shown:?}");
        for sum in [3.0, 0.003] {
            let text = trade::plain(lang, 34_000_000.0 * sum);
            assert!(!shown.iter().any(|s| s.contains(&text)), "no sum {text} in {shown:?}");
        }
    }

    #[test]
    fn a_part_of_a_piece_is_named_and_the_field_keeps_the_unit_of_the_good() {
        let state = typed("Saldynium (Ore)", "0,1", Some(0.001), false);
        assert_eq!(state.parts_of_a_piece(), ["Saldynium (Ore)"]);
        assert_eq!(state.unit_word("Saldynium (Ore)"), t("s_lg_stueck"));
        let more = typed("Saldynium (Ore)", "3,7", Some(0.001), false);
        assert_eq!(more.parts_of_a_piece(), ["Saldynium (Ore)"], "a part after whole pieces is named too");
        assert!(typed("Saldynium (Ore)", "2", Some(0.001), false).parts_of_a_piece().is_empty());
        assert!(typed("Iron", "0,1", None, false).parts_of_a_piece().is_empty(), "an SCU may be a part");
        assert_eq!(typed("Iron", "5", None, false).unit_word("Iron"), "SCU");
        assert_eq!(typed("Iron", "5", None, true).unit_word("Iron"), "cSCU");
    }

    #[test]
    fn the_sample_draws_every_line_of_the_amounts() {
        let lang = language::current();
        let mut state = SellingState::sample();
        let scheme = versekit_core::theme::current();
        let shown = drawn(|ui| {
            chips(ui, scheme, &mut state);
            sell_places(ui, scheme, &mut state);
        });
        let any = |text: &str| shown.iter().any(|s| s.contains(text));
        for key in ["s_lg_stueck", "s_vk_scu_kurz", "s_lg_cscu"] {
            assert!(any(&t(key)), "{key} in {shown:?}");
        }
        assert!(any(&tf("s_vk_stueck_ohne_groesse", &[("waren", Arg::from("Saldynium (Ore)"))])), "{shown:?}");
        assert!(any(&tf("s_vk_nur_ganze_stuecke", &[("waren", Arg::from("Valakkar Pearl (Apex)"))])), "{shown:?}");
        assert!(any(&formats::amount(lang, 12.0, &t("s_lg_stueck"))), "{shown:?}");
        assert!(any(&trade::plain(lang, 45_000.0 * 0.012 + 2_000.0 * 0.1)), "the sum of the sized goods: {shown:?}");
    }

    /// A page state with `amount` SCU of `good` in Mein Lager and `text` typed for it.
    fn in_stock(good: &str, amount: f64, text: &str, piece_size: Option<f64>, cscu: bool) -> SellingState {
        let mut state = typed(good, text, piece_size, cscu);
        let row = format!(r#"{{"ware": "{good}", "menge": {amount}, "ort": "Area18"}}"#);
        state.cargo = vec![versekit_core::jsonio::loads(&row).expect("row")];
        state
    }

    #[test]
    fn the_amount_field_calculates_like_mein_lager() {
        // A number is the new amount, a sum is worked out, a sign books onto what Mein Lager holds.
        for (text, expected) in [("30+2", 32.0), ("30+2,5", 32.5), ("+8", 40.0), ("-2", 30.0), ("−2", 30.0), ("4,5", 4.5)] {
            let state = in_stock("Iron", 32.0, text, None, false);
            let got = scu_of(&state, "Iron");
            assert!(got.is_some_and(|got| (got - expected).abs() < 1e-9), "{text}: {got:?} instead of {expected}");
        }
        for nothing in ["-40", "+", "3+", "abc", "5-5"] {
            assert_eq!(scu_of(&in_stock("Iron", 32.0, nothing, None, false), "Iron"), None, "{nothing:?}");
        }
        // Without anything in Mein Lager a sign books onto nothing.
        assert_eq!(scu_of(&typed("Iron", "+8", None, false), "Iron"), Some(8.0));
    }

    #[test]
    fn calculating_holds_for_cscu_and_pieces_too() {
        // cSCU: Mein Lager holds 0,32 SCU = 32 cSCU; "+8" makes 40 cSCU = 0,4 SCU.
        let cscu = in_stock("Iron", 0.32, "+8", None, true);
        assert!(scu_of(&cscu, "Iron").is_some_and(|got| (got - 0.40).abs() < 1e-9), "{:?}", scu_of(&cscu, "Iron"));
        // Pieces of 0,001 SCU: 32 pieces lie there; "+8" makes 40 pieces and stays whole.
        let pieces = in_stock("Kopion Horn", 0.032, "+8", Some(0.001), false);
        assert_eq!(pieces.own_pieces().get("Kopion Horn"), Some(&40.0));
        assert!(scu_of(&pieces, "Kopion Horn").is_some_and(|got| (got - 0.040).abs() < 1e-9));
        let sum = in_stock("Kopion Horn", 0.0, "30+2", Some(0.001), false);
        assert_eq!(sum.own_pieces().get("Kopion Horn"), Some(&32.0));
        let part = in_stock("Kopion Horn", 0.0, "30+2,5", Some(0.001), false);
        assert_eq!(part.own_pieces().get("Kopion Horn"), Some(&32.0), "pieces come out whole");
        assert_eq!(part.parts_of_a_piece(), ["Kopion Horn"], "and the page says so");
    }

    #[test]
    fn enter_in_the_amount_field_turns_a_calculation_into_its_result() {
        let mut state = in_stock("Iron", 32.0, "+8", None, false);
        state.settle("Iron");
        assert_eq!(state.amounts.get("Iron").map(String::as_str), Some("40"));
        state.amounts.insert("Iron".to_string(), "30+2,5".to_string());
        state.settle("Iron");
        assert!(["32,5", "32.5"].contains(&state.amounts["Iron"].as_str()), "{:?}", state.amounts["Iron"]);
        state.amounts.insert("Iron".to_string(), "7".to_string());
        state.settle("Iron");
        assert_eq!(state.amounts.get("Iron").map(String::as_str), Some("7"), "a plain number stays as it is");
        state.amounts.insert("Iron".to_string(), "-99".to_string());
        state.settle("Iron");
        assert_eq!(state.amounts.get("Iron").map(String::as_str), Some("-99"), "a result below nothing stays to be seen");
    }

    #[test]
    fn switching_the_unit_of_mein_lager_keeps_the_typed_amount_the_same_amount() {
        let mut state = typed("Iron", "0,5", None, false);
        state.stock.add_recipe_materials([("Kopion Horn".to_string(), true, 0.0, 0.0)]);
        state.amounts.insert("Kopion Horn".to_string(), "4".to_string());
        state.change_unit(true);
        assert_eq!(state.amounts.get("Iron").map(String::as_str), Some("50"));
        assert_eq!(scu_of(&state, "Iron"), Some(0.5), "still half an SCU");
        assert_eq!(state.amounts.get("Kopion Horn").map(String::as_str), Some("4"), "pieces stay");
        state.change_unit(false);
        assert_eq!(state.amounts.get("Iron").map(String::as_str), Some("0.5"));
        assert_eq!(scu_of(&state, "Iron"), Some(0.5));
        state.change_unit(false);
        assert_eq!(state.amounts.get("Iron").map(String::as_str), Some("0.5"), "no change, no rewrite");
    }

    #[test]
    fn taking_the_goods_of_mein_lager_leaves_what_is_typed() {
        let mut state = typed("Saldynium (Ore)", "3", Some(0.001), false);
        state.take_from_stock();
        assert_eq!(state.amounts.get("Saldynium (Ore)").map(String::as_str), Some("3"));
        assert_eq!(state.picks, ["Saldynium (Ore)"]);
    }

    #[test]
    fn a_part_of_an_scu_is_counted_in_the_proceeds() {
        let lang = language::current();
        let mut state = typed("Iron", "0,1", None, false);
        state.data = priced("Iron", 2_000.0);
        let scheme = versekit_core::theme::current();
        let shown = drawn(|ui| sell_places(ui, scheme, &mut state));
        let wanted = trade::plain(lang, 200.0);
        assert!(shown.iter().any(|s| s.contains(&wanted)), "{wanted} in {shown:?}");
    }

    #[test]
    fn the_route_list_never_sticks_out_at_the_right_in_any_size_or_language() {
        let mut found = Vec::new();
        for english in [false, true] {
            versekit_core::language::set_thread_language(if english {
                versekit_core::language::Choice::En
            } else {
                versekit_core::language::Choice::De
            });
            for points in versekit_core::font_size::POINTS_RANGE.0..=versekit_core::font_size::POINTS_RANGE.1 {
                let mut state = SellingState::sample();
                assert!(state.show_route_sample());
                let mut harness = egui_kittest::Harness::builder()
                    .with_size(egui::vec2(1280.0, 820.0))
                    .with_context_setup(crate::install)
                    .with_accessibility_check(false)
                    .allow_missing_glyphs()
                    .build_ui_state(|ui, state: &mut SellingState| show_page(ui, versekit_core::theme::current(), state), state);
                harness.ctx.options_mut(|options| options.zoom_factor = versekit_core::font_size::zoom_factor(points));
                harness.run_steps(8);
                let overflow = harness.state().list_overflow();
                if overflow > 0.0 {
                    found.push(format!("{} size {points}: {overflow}", if english { "EN" } else { "DE" }));
                }
            }
        }
        versekit_core::language::set_thread_language(versekit_core::language::Choice::Auto);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn the_route_page_in_the_window_sticks_out_nowhere_when_the_size_changes_on_one_page() {
        use versekit_core::font_size;
        use versekit_core::language::{Choice, set_thread_language};
        set_thread_language(Choice::En);
        let mut page = crate::sample::SamplePage::preview();
        page.go_to("hf_verkauf");
        assert!(page.choose_first_entry());
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(font_size::BASE_MIN_SIZE.0, font_size::BASE_MIN_SIZE.1))
            .with_context_setup(crate::install)
            .with_accessibility_check(false)
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, state: &mut (crate::sample::SamplePage, f32)| state.1 = state.0.show(ui),
                (page, 0.0),
            );
        let mut found = Vec::new();
        for points in font_size::POINTS_RANGE.0..=font_size::POINTS_RANGE.1 {
            let (smallest_width, smallest_height) = font_size::min_window(points);
            for window in [egui::vec2(smallest_width, smallest_height), egui::vec2(1280.0, 820.0), egui::vec2(1280.0, 1600.0)] {
                harness.state_mut().0.size = points;
                harness.ctx.options_mut(|options| options.zoom_factor = font_size::zoom_factor(points));
                for _ in 0..6 {
                    harness.set_size(window);
                    harness.step();
                }
                let overflow = harness.state().1;
                if overflow > 0.0 {
                    found.push(format!("size {points} window {window:?}: {overflow}"));
                }
            }
        }
        set_thread_language(Choice::Auto);
        assert!(found.is_empty(), "{found:?}");
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
