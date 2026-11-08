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

//! The page "Mining" (`pages._mining` of 4.12): which processing method suits the player, what a scanned
//! signature is, and where which ore lies: one search for both ways, a raw material for its places and a
//! place for what lies there, each place with the share of the ore per tool and below an ore the refinery
//! that makes the most of it.
//!
//! Unlike 4.12 the search and the filters stay when the page is left, and the page reads the mining data
//! again as soon as the file changes (4.12 built itself anew only when it had no data).

use std::time::SystemTime;

use egui::{RichText, Ui};
use versekit_core::archive_lock;
use versekit_core::jsonio::{self, Value};
use versekit_core::language::{Arg, fill, t, tf};
use versekit_core::mining_data::{self, MiningData, Ore, OrePlace, Place};
use versekit_core::paths::Paths;
use versekit_core::refining;
use versekit_core::stock_data::lower;
use versekit_core::stock_fetch::{self, Outcome, Reason};
use versekit_core::theme::{IconTint, Scheme};
use versekit_gamedata::values_store;

use crate::color;
use crate::farm_page::share_text;
use crate::trade::{self, Job, Polled};
use crate::widgets::{self, ButtonKind};

const LOOK_SECONDS: f64 = 1.0;
/// How often a page without the file of the processing methods looks whether it may ask for a reading.
const ASK_SECONDS: f64 = 5.0;
/// So many hits of the signature stand there; the count above names all.
const SHOWN_HITS: usize = 10;
/// The tools in the order of the page; the first choice of the filter means all.
const TOOLS: [&str; 3] = ["schiff", "fahrzeug", "fps"];
/// The ways of mining in the order they are named at a place.
const KINDS: [&str; 4] = ["fps", "fahrzeug", "schiff", "schiff_selten"];

/// What a click asks of the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Open the page of the refineries.
    ToRefineries,
}

/// The page "Mining".
pub struct MiningState {
    paths: Option<Paths>,
    mining: MiningData,
    seen: Option<SystemTime>,
    looked_at: f64,
    /// The text of the search.
    pub search: String,
    /// The raw material and the place picked in the filters (indexes into their lists, 0 = all), the tool.
    picked_ore: usize,
    picked_place: usize,
    tool: usize,
    /// The one row that is open: `erz:` or `ort:` and its name.
    open: Option<String>,
    /// The typed signature.
    pub signature: String,
    /// The axes chosen for the processing method (0 = none).
    first: usize,
    second: usize,
    /// The levels of the processing methods: read from the game data where they were kept, else the table.
    methods: refining::Methods,
    methods_seen: Option<SystemTime>,
    /// Where the reading of the game data comes from, to ask for the file of the methods when it is missing.
    supply: Option<std::sync::Arc<dyn values_store::Supply>>,
    /// The reading this page started for that file, and whether it was started (once for a page).
    methods_reading: Option<values_store::Pending>,
    methods_asked: bool,
    /// When the wish for the file was last looked at ([`ASK_SECONDS`]).
    methods_checked: f64,
    /// The fetch of the mining data that runs, and what the last one said (and whether it failed).
    fetch: Option<Job<Fetched>>,
    said: Option<(String, bool)>,
    /// The signature scanner: its switch, the teaching of the digits and the place of its window.
    pub scanner: crate::scanner_ui::ScannerState,
}

/// What a fetch of the mining data ended with: the sentence and whether it went wrong.
struct Fetched {
    text: String,
    failed: bool,
}

/// The sample: a made-up handful of places, ores and refineries, so that every part of the page shows.
const SAMPLE: &str = r#"{"format": 1, "build": "4.3.2-sample",
  "locations": [
    {"locationName": "Daymar", "system": "Stanton", "locationType": "Moon", "groups": [
      {"groupName": "SpaceShip_Mineables", "groupProbability": 1, "deposits": [
        {"compositionGuid": "q", "relativeProbability": 1}, {"compositionGuid": "t", "relativeProbability": 3}]},
      {"groupName": "FPS_Mineables", "deposits": [{"compositionGuid": "h", "relativeProbability": 1}]}]},
    {"locationName": "Aaron Halo", "system": "Stanton", "locationType": "Asteroid belt", "groups": [
      {"groupName": "SpaceShip_Mineables", "deposits": [
        {"compositionGuid": "q", "relativeProbability": 2}, {"compositionGuid": "t", "relativeProbability": 1}]}]},
    {"locationName": "Pyro IV", "system": "Pyro", "locationType": "Planet", "groups": [
      {"groupName": "GroundVehicle_Mineables", "deposits": [{"compositionGuid": "g", "relativeProbability": 1}]}]}],
  "compositions": {
    "q": {"name": "Quantanium rock", "parts": [{"elementName": "Quantanium (Raw)", "elementGuid": "eq", "probability": 0.6, "minPercent": 5, "maxPercent": 25},
                                               {"elementName": "Titanium (Ore)", "elementGuid": "et", "probability": 1, "minPercent": 30, "maxPercent": 60}]},
    "t": {"name": "Titanium rock", "parts": [{"elementName": "Titanium (Ore)", "elementGuid": "et", "probability": 1, "minPercent": 40, "maxPercent": 80}]},
    "h": {"name": "Hadanite", "parts": [{"elementName": "Hadanite", "elementGuid": "eh", "probability": 1, "minPercent": 100, "maxPercent": 100}]},
    "g": {"name": "Gold rock", "parts": [{"elementName": "Gold (Ore)", "elementGuid": "eg", "probability": 1, "minPercent": 10, "maxPercent": 30}]}},
  "refineries": [
    {"name": "ARC-L1 Wide Forest Station", "system": "Stanton", "profileId": "a"},
    {"name": "CRU-L1 Ambitious Dream Station", "system": "Stanton", "profileId": "b"},
    {"name": "Pyro Gateway (Stanton)", "system": "Stanton", "profileId": "a"},
    {"name": "Checkmate", "system": "Pyro", "profileId": "c"}],
  "refineryProfiles": {
    "a": {"Quantanium (Raw)": 9, "Titanium (Ore)": -4, "Gold (Ore)": 2},
    "b": {"Quantanium (Raw)": -3, "Titanium (Ore)": 7, "Gold (Ore)": 2},
    "c": {"Quantanium (Raw)": 4, "Gold (Ore)": 2}},
  "elemente": {
    "eq": {"name": "Quantanium (Raw)", "materialName": "Quantanium", "scanSignature": 3170, "rarity": "legendary"},
    "et": {"name": "Titanium (Ore)", "materialName": "Titanium", "scanSignature": 4270, "rarity": "common"},
    "eg": {"name": "Gold (Ore)", "materialName": "Gold", "scanSignature": 3585, "rarity": "uncommon"},
    "eh": {"name": "Hadanite", "scanSignature": 3000, "rarity": "rare"}}}"#;

/// The mining data of the sample.
pub(crate) fn sample_data() -> MiningData {
    MiningData::of(&jsonio::loads(SAMPLE).unwrap_or(Value::Null))
}

fn file_time(file: &str) -> Option<SystemTime> {
    std::fs::metadata(file).and_then(|m| m.modified()).ok()
}

impl Default for MiningState {
    fn default() -> Self {
        MiningState {
            paths: None,
            mining: sample_data(),
            seen: None,
            looked_at: f64::NEG_INFINITY,
            search: String::new(),
            picked_ore: 0,
            picked_place: 0,
            tool: 0,
            open: None,
            signature: String::new(),
            first: 0,
            second: 0,
            methods: refining::Methods::table(),
            methods_seen: None,
            supply: None,
            methods_reading: None,
            methods_asked: false,
            methods_checked: f64::NEG_INFINITY,
            fetch: None,
            said: None,
            scanner: crate::scanner_ui::ScannerState::default(),
        }
    }
}

impl MiningState {
    /// With own data: the mining data of the data folder.
    pub fn with_files(paths: &Paths) -> MiningState {
        let mut state = MiningState {
            paths: Some(Paths::new(paths.host.clone())),
            scanner: crate::scanner_ui::ScannerState::with_files(paths),
            ..MiningState::default()
        };
        state.reload();
        state
    }

    /// The page asks for the levels of the processing methods itself when their file is missing: it starts the
    /// reading of the game data with this `supply` ([`MiningState::ask_for_methods`]). Without a supply it never does.
    pub fn with_supply(mut self, supply: std::sync::Arc<dyn values_store::Supply>) -> MiningState {
        self.supply = Some(supply);
        self
    }

    /// The file of the processing methods is missing: starts the reading of the game data that keeps it, once for
    /// this page. Only with the archive free to read (the game is closed, [`archive_lock::may_read`]) and a supply
    /// that reads it, and not while a reading of the values already runs (the page Herstellung starts one at the
    /// start of the program): that one writes the file too.
    fn ask_for_methods(&mut self, ctx: &egui::Context, now: f64) {
        if self.methods_asked || self.methods_seen.is_some() || now - self.methods_checked < ASK_SECONDS {
            return;
        }
        // Whether the game runs is looked up in the process list and the logs: not at every look of the page.
        self.methods_checked = now;
        let (Some(paths), Some(supply)) = (&self.paths, &self.supply) else { return };
        if !supply.reads_archive() || values_store::reading() || !archive_lock::may_read(paths) {
            return;
        }
        let folder = paths.game_folder().map(std::path::PathBuf::from);
        let start = values_store::start(folder.as_deref(), &paths.app_file(values_store::CACHE_FILE), supply.clone());
        self.methods_asked = true;
        self.methods_reading = start.rebuild.and_then(values_store::Rebuild::spawn);
        if let Some(reading) = &self.methods_reading {
            let ctx = ctx.clone();
            reading.set_wake(move || ctx.request_repaint());
        }
    }

    fn reload(&mut self) {
        let Some(paths) = &self.paths else { return };
        let file = paths.app_file(mining_data::FILE);
        self.seen = file_time(&file);
        self.mining = MiningData::load(&file);
        self.reload_methods();
    }

    /// Reads the levels of the processing methods the reading of the game data keeps; without them, or when they do
    /// not fit, the table.
    fn reload_methods(&mut self) {
        let Some(paths) = &self.paths else { return };
        let file = paths.app_file(refining::GAME_FILE);
        self.methods_seen = file_time(&file);
        self.methods = refining::Methods::from_game_file(&file).unwrap_or_else(refining::Methods::table);
    }

    /// Reads the mining data again when the file changed on disk.
    pub(crate) fn follow(&mut self, ui: &Ui) {
        let now = ui.input(|i| i.time);
        if now - self.looked_at < LOOK_SECONDS {
            return;
        }
        self.looked_at = now;
        let Some(paths) = &self.paths else { return };
        if file_time(&paths.app_file(mining_data::FILE)) != self.seen {
            self.reload();
        } else if file_time(&paths.app_file(refining::GAME_FILE)) != self.methods_seen {
            self.reload_methods();
        }
        // A finished reading is let go; the file it wrote is found by its time above, at the next look.
        let over = self.methods_reading.as_ref().is_some_and(|r| matches!(r.poll(), values_store::Progress::Done(_)));
        if over {
            self.methods_reading = None;
        }
        // The thread wakes the window when it is done; a reading that ended before the wake was set is found too.
        if self.methods_reading.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
        }
        self.ask_for_methods(ui.ctx(), now);
    }

    /// The mining data the page shows.
    pub(crate) fn data(&self) -> &MiningData {
        &self.mining
    }

    /// Whether a search or a filter narrows the list.
    pub fn is_narrowed(&self) -> bool {
        !self.search.trim().is_empty() || self.picked_ore + self.picked_place + self.tool > 0
    }

    /// Takes back the search, the filters and what is open.
    pub fn reset(&mut self) {
        self.search.clear();
        self.picked_ore = 0;
        self.picked_place = 0;
        self.tool = 0;
        self.open = None;
    }

    /// Opens the row of the first raw material, as a click on it does. Whether there is one.
    pub fn choose_first(&mut self) -> bool {
        self.open = self.mining.ores().first().map(|ore| format!("erz:{}", ore.name));
        self.open.is_some()
    }

    /// Opens the page on a raw material or a place from another page (`jump_to_ore` of 4.12): an ore that
    /// matches by its key stands open under its own name; anything else stands in the search.
    pub fn jump_to(&mut self, name: &str) {
        self.reset();
        let wanted = self.mining.material_key(name);
        match self.mining.ores().iter().find(|o| self.mining.material_key(&o.name) == wanted) {
            Some(ore) => {
                self.search = ore.name.clone();
                self.open = Some(format!("erz:{}", ore.name));
            }
            None => self.search = name.to_string(),
        }
    }

    /// Starts the fetch of the mining data: the build of the game first, then the file for it.
    pub(crate) fn start_fetch(&mut self, ctx: &egui::Context) {
        let Some(paths) = &self.paths else { return };
        if self.fetch.is_some() {
            return;
        }
        let paths = trade::job_paths(paths);
        self.said = None;
        self.fetch = Some(Job::spawn(ctx, "mining-fetch", move || fetch(&paths)));
    }

    /// Takes in a fetch that ended; true when the data are new.
    pub(crate) fn poll_fetch(&mut self) -> bool {
        let Some(job) = &self.fetch else { return false };
        let result = match job.poll() {
            Polled::Running => return false,
            Polled::Done(result) => result,
            Polled::Lost => Fetched { text: t("s_bg_holen_weg"), failed: true },
        };
        self.fetch = None;
        self.said = Some((result.text, result.failed));
        self.reload();
        !result.failed
    }

    /// Whether a fetch runs.
    pub(crate) fn fetching(&self) -> bool {
        self.fetch.is_some()
    }

    /// What the last fetch said, and whether it failed.
    pub(crate) fn said(&self) -> Option<(&str, bool)> {
        self.said.as_ref().map(|(text, failed)| (text.as_str(), *failed))
    }

    /// How old the mining data are (seconds) and for which game build, where they come from a file.
    pub(crate) fn data_age(&self) -> Option<(f64, String)> {
        let seen = self.seen?;
        let age = SystemTime::now().duration_since(seen).map_or(0.0, |d| d.as_secs_f64());
        Some((age, self.mining.build().to_string()))
    }

    /// Whether the page has its own data (not the sample).
    pub(crate) fn real(&self) -> bool {
        self.paths.is_some()
    }
}

/// The fetch itself, in the thread of the job (`mining.update` of 4.12, with the build from scmdb).
fn fetch(paths: &Paths) -> Fetched {
    let offline = Fetched { text: t("m_h_kein_netz"), failed: true };
    let user_agent = trade::user_agent();
    let build = if cfg!(test) {
        Err(versekit_core::scmdb::FetchError::Offline)
    } else {
        versekit_gamedata::scmdb::current_build(&user_agent)
    };
    let build = match build {
        Ok(build) => build,
        Err(versekit_core::scmdb::FetchError::Offline) => return offline,
        Err(error) => {
            versekit_core::errors::record("pages.bergbau.holen", &error);
            return Fetched { text: t("s_bg_holen_weg"), failed: true };
        }
    };
    let places = || MiningData::load(&paths.app_file(mining_data::FILE)).places().len();
    match stock_fetch::update_mining(paths, &build, &user_agent) {
        Outcome::Fresh => Fetched { text: tf("m_b_geladen", &[("0", Arg::from(places()))]), failed: false },
        Outcome::Current => Fetched { text: tf("m_b_aktuell", &[("0", Arg::from(places()))]), failed: false },
        Outcome::Offline => offline,
        Outcome::Failed(Reason::Empty(_)) => Fetched { text: t("m_b_leer"), failed: true },
        Outcome::Refused | Outcome::Failed(Reason::Write(_)) => {
            Fetched { text: t("m_b_nicht_gespeichert"), failed: true }
        }
        Outcome::Failed(reason) => {
            versekit_core::errors::record("pages.bergbau.holen", &reason);
            Fetched { text: t("s_bg_holen_weg"), failed: true }
        }
    }
}

fn sub(scheme: &Scheme, text: impl Into<String>) -> RichText {
    RichText::new(text.into()).small().color(color(scheme.sub))
}

/// A row that is clicked as a whole and holds wrapping labels, `key` telling it from the others: the
/// background is painted below the labels once their height is known, the labels take neither the click nor
/// the mouse. A screen reader hears `spoken` and whether the row is open; Tab reaches it and shows the ring.
pub(crate) fn click_block(
    ui: &mut Ui,
    scheme: &Scheme,
    stripe: bool,
    (key, spoken, open): (&str, &str, bool),
    add: impl FnOnce(&mut Ui),
) -> bool {
    let background = ui.painter().add(egui::Shape::Noop);
    let shown = egui::Frame::new().inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |inner| {
        inner.set_width(inner.available_width());
        inner.style_mut().interaction.selectable_labels = false;
        add(inner);
    });
    let rect = shown.response.rect;
    let id = ui.make_persistent_id(("klickzeile", key));
    let response = ui.interact(rect, id, egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| {
        let mut info = egui::WidgetInfo::labeled(egui::accesskit::Role::Button, true, spoken);
        info.selected = Some(open);
        info
    });
    if response.hovered() {
        // On a striped row a lighter fill alone is hard to see: the edge in the accent says it too.
        ui.painter().set(background, egui::Shape::rect_filled(rect, 0.0, color(scheme.hover)));
        ui.painter().rect_stroke(rect, 0.0, egui::Stroke::new(1.0, color(scheme.accent)), egui::StrokeKind::Inside);
    } else if stripe {
        ui.painter().set(background, egui::Shape::rect_filled(rect, 0.0, color(scheme.bar)));
    }
    widgets::focus_ring(ui, &response, scheme, egui::CornerRadius::ZERO);
    response.clicked()
}

/// Text at the left and at the right of one line, the right one pushed to the edge; on a narrow page the
/// right one moves below.
fn sides(ui: &mut Ui, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui)) {
    ui.horizontal_wrapped(|row| {
        left(row);
        row.with_layout(egui::Layout::right_to_left(egui::Align::Center), right);
    });
}

/// The ways of mining at a place in a fixed order: `FPS · Fahrzeug`.
fn kinds_text(kinds: &std::collections::BTreeSet<String>) -> String {
    KINDS.iter().filter(|k| kinds.contains(**k)).map(|k| t(&format!("s_bg_art_{k}"))).collect::<Vec<_>>().join(" · ")
}

/// The share of a place with its level, or "the only one here"; nothing where the share is unknown.
fn share_words(share: f64, level: u8, alone: bool) -> Option<(String, u8)> {
    if share <= 0.0 {
        return None;
    }
    if alone {
        return Some((t("s_bg_einziges"), level));
    }
    Some((format!("{} · {}", share_text(share), t(&format!("s_bg_st_{level}"))), level))
}

fn level_color(scheme: &Scheme, level: u8) -> versekit_core::theme::Color {
    match level {
        5.. => scheme.accent,
        3 | 4 => scheme.fg,
        _ => scheme.sub,
    }
}

/// Whether an ore is mined with the tool somewhere.
fn ore_has_tool(ore: &Ore, tool: &str) -> bool {
    ore.places.iter().any(|p| p.per_tool.iter().any(|s| s.tool == tool))
}

fn place_has_tool(place: &Place, tool: &str) -> bool {
    place.per_tool.iter().any(|s| s.tool == tool)
}

/// The share, level and "alone" of an ore at a place, for the tool chosen or for all.
fn share_at(place: &OrePlace, tool: Option<&str>) -> (f64, u8, bool) {
    match tool {
        Some(tool) => place
            .per_tool
            .iter()
            .find(|s| s.tool == tool)
            .map_or((0.0, 1, false), |s| (s.share, s.level, s.ores_there <= 1)),
        None => match place.per_tool.as_slice() {
            [only] => (place.share, place.level, only.ores_there <= 1),
            _ => (place.share, place.level, false),
        },
    }
}

/// Draws the page.
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut MiningState) -> Option<Action> {
    state.follow(ui);
    state.poll_fetch();
    let mut action = None;
    ui.label(sub(scheme, t("s_bg_lead")));
    ui.add_space(8.0);
    // The methods need no mining data: they stand there without it.
    method_box(ui, scheme, state);
    ui.add_space(10.0);
    // The scanner needs no mining data either: the base signatures it knows by itself.
    if state.real() {
        crate::scanner_ui::show_box(ui, scheme, &mut state.scanner, &state.mining);
        ui.add_space(10.0);
    }
    if state.mining.is_empty() {
        widgets::card(ui, scheme, |ui| {
            widgets::card_title(ui, scheme, &t("s_bg_daten_kopf"));
            no_data(ui, scheme, state, ("compass", "s_bg_keine_daten"));
        });
        return None;
    }
    signature_box(ui, scheme, state);
    ui.add_space(10.0);
    search_head(ui, scheme, state);
    let ores = state.mining.ores().len();
    ui.label(sub(scheme, tf("s_bg_orte", &[("0", Arg::from(state.mining.places().len())), ("1", Arg::from(ores))])));
    ui.label(sub(scheme, t("s_bg_anteil_hilfe")));
    ui.add_space(4.0);
    if list(ui, scheme, state) {
        action = Some(Action::ToRefineries);
    }
    ui.add_space(8.0);
    stand(ui, scheme, state);
    ui.add(egui::Label::new(sub(scheme, t("s_bg_mehr_info"))).wrap());
    if widgets::external_link(ui, scheme, SCMDB_NAME).clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(SCMDB));
    }
    action
}

/// The site of the mining data, for the whole of them, and its name.
const SCMDB: &str = "https://scmdb.net";
const SCMDB_NAME: &str = "scmdb.net";

/// How old the mining data of the data folder are, with their game build on hover.
pub(crate) fn stand(ui: &mut Ui, scheme: &Scheme, state: &MiningState) {
    if let Some((age, build)) = state.data_age() {
        widgets::data_age(ui, scheme, age, &tf("s_bg_stand", &[("0", Arg::from(build))]));
    }
}

/// Without mining data: the sentence, the button to fetch them, the wait while they come and what the last
/// fetch said; a failure with the way to try again.
pub(crate) fn no_data(ui: &mut Ui, scheme: &Scheme, state: &mut MiningState, (icon, sentence): (&str, &str)) {
    widgets::empty_state(ui, scheme, icon, &t(sentence));
    if !state.real() {
        return;
    }
    let word = t("s_bg_jetzt_holen");
    let busy = t("s_bg_holt");
    let button =
        widgets::Button::new(&word, ButtonKind::Strong, scheme).blocked(state.fetching().then_some(busy.as_str()));
    if ui.add(button).clicked() && !state.fetching() {
        state.start_fetch(ui.ctx());
    }
    if state.fetching() {
        widgets::loading(ui, scheme, &busy);
        return;
    }
    match state.said() {
        Some((said, true)) => {
            widgets::load_failed(ui, scheme, said, &t("s_bg_nochmal"));
        }
        Some((said, false)) => {
            ui.label(sub(scheme, said.to_string()));
        }
        None => {}
    }
}

fn axis_word(axis: &str) -> String {
    t(&format!("s_rm_{axis}"))
}

/// The line of levels of a method: yield, speed, costs in words.
fn method_line(methods: &refining::Methods, key: &str) -> String {
    let size = |level: u8| t(["s_rm_s_gering", "s_rm_s_moderat", "s_rm_s_hoch"][usize::from(level.clamp(1, 3) - 1)]);
    let speed = t(["s_rm_t_sehr", "s_rm_t_langsam", "s_rm_t_mittel", "s_rm_t_schnell"]
        [usize::from(methods.level(key, "tempo").min(3))]);
    // A high cost advantage means low costs: the word is turned round.
    let costs = size(4 - methods.level(key, "kosten").clamp(1, 3));
    tf(
        "s_rm_zeile",
        &[("0", Arg::from(size(methods.level(key, "ertrag")))), ("1", Arg::from(speed)), ("2", Arg::from(costs))],
    )
}

/// Where the levels come from: the table with the patch it was read in, or the game data with their build, and
/// for a channel of test builds what the pages that read the archive say of it.
fn methods_origin(methods: &refining::Methods) -> Vec<String> {
    match &methods.origin {
        refining::Origin::Table => {
            vec![tf("s_rm_stand", &[("0", Arg::from(refining::PATCH)), ("1", Arg::from(refining::READ_ON))])]
        }
        refining::Origin::Game { build, channel } => {
            let notes = versekit_core::archive_lock::test_channel_notes(channel);
            let read = if build.is_empty() {
                t("s_rm_stand_spiel_ohne")
            } else {
                tf("s_rm_stand_spiel", &[("0", Arg::from(build.as_str()))])
            };
            std::iter::once(read)
                .chain(notes.iter().map(|key| tf(key, &[("0", Arg::from(channel.as_str()))])))
                .collect()
        }
    }
}

/// "Which processing method?": two axes to choose, the method that suits them and all nine in a fold.
fn method_box(ui: &mut Ui, scheme: &Scheme, state: &mut MiningState) {
    widgets::card(ui, scheme, |card| {
        widgets::card_title(card, scheme, &t("s_rm_kopf"));
        card.label(sub(scheme, t("s_rm_lead")));
        let first: Vec<String> =
            std::iter::once(t("s_rm_erste")).chain(refining::AXES.iter().map(|a| axis_word(a))).collect();
        let second: Vec<String> =
            std::iter::once(t("s_rm_zweite")).chain(refining::AXES.iter().map(|a| axis_word(a))).collect();
        let first: Vec<&str> = first.iter().map(String::as_str).collect();
        let second: Vec<&str> = second.iter().map(String::as_str).collect();
        let mut chosen = [state.first, state.second];
        widgets::filter_bar(card, "methode", &[&first, &second], &mut chosen);
        [state.first, state.second] = chosen;
        let axis = |i: usize| i.checked_sub(1).and_then(|i| refining::AXES.get(i)).copied();
        let methods = &state.methods;
        let (best, all) = methods.recommend(axis(state.first), axis(state.second));
        card.add_space(4.0);
        card.label(
            widgets::bold(card, tf("s_rm_nimm", &[("0", Arg::from(refining::name(best)))]), egui::TextStyle::Body)
                .color(color(scheme.accent)),
        );
        card.label(sub(scheme, method_line(methods, best)));
        if methods.level(best, "tempo") <= 1 {
            card.label(sub(scheme, t("s_rm_zeit_laeuft")));
        }
        widgets::collapsing(&t("s_rm_alle")).id_salt("methoden-vergleich").show(card, |inner| {
            for (k, key) in all.iter().enumerate() {
                let frame = egui::Frame::new()
                    .fill(if k % 2 == 1 { color(scheme.bar) } else { egui::Color32::TRANSPARENT })
                    .inner_margin(egui::Margin::symmetric(6, 2));
                frame.show(inner, |row| {
                    row.set_width(row.available_width());
                    let tone = if *key == best { scheme.accent } else { scheme.fg };
                    sides(
                        row,
                        |left| {
                            left.label(RichText::new(refining::name(key)).color(color(tone)));
                        },
                        |right| {
                            right.label(sub(scheme, method_line(methods, key)));
                        },
                    );
                });
            }
            let mut beaten = methods.dominated();
            beaten.sort_unstable();
            for (worse, better) in beaten {
                let said = tf(
                    "s_rm_unterlegen",
                    &[("0", Arg::from(refining::name(worse))), ("1", Arg::from(refining::name(better)))],
                );
                inner.label(sub(scheme, said));
            }
            for line in methods_origin(methods) {
                inner.label(sub(scheme, line));
            }
        });
    });
}

/// The calculator of the scan signature in a card of its own.
fn signature_box(ui: &mut Ui, scheme: &Scheme, state: &mut MiningState) {
    widgets::card(ui, scheme, |ui| {
        widgets::card_title(ui, scheme, &t("s_bg_sig_kopf"));
        signature_body(ui, scheme, state);
    });
}

/// What the card of the signature holds: a value gives the ores with their chunks, a name its signatures.
fn signature_body(ui: &mut Ui, scheme: &Scheme, state: &mut MiningState) {
    let width = ui.available_width() - crate::layout::SAFETY;
    widgets::search_field(ui, &mut state.signature, &t("s_pl_signatur"), scheme, width);
    ui.label(sub(scheme, t("s_bg_sig_hilfe")));
    let typed = state.signature.trim().to_string();
    if typed.is_empty() {
        return;
    }
    let whole = |value: f64| format!("{}", value.trunc() as i64);
    if typed.chars().any(char::is_alphabetic) {
        let rows = state.mining.signatures_of(&typed);
        if rows.is_empty() {
            ui.label(RichText::new(t("s_bg_sig_kein_erz")).color(color(scheme.yellow)));
        }
        for (name, _, series) in rows.iter().take(SHOWN_HITS) {
            let counts = series
                .iter()
                .enumerate()
                .map(|(n, value)| tf("s_bg_sig_vielfach", &[("0", Arg::from(n + 1)), ("1", Arg::from(whole(*value)))]))
                .collect::<Vec<_>>()
                .join("   ");
            sides(
                ui,
                |left| {
                    left.label(
                        RichText::new(versekit_core::signature_window::display_name(name)).color(color(scheme.accent)),
                    );
                },
                |right| {
                    right.label(sub(scheme, counts));
                },
            );
        }
        if rows.len() > SHOWN_HITS {
            ui.label(sub(scheme, tf("s_mlg_mehr", &[("n", Arg::from(rows.len() - SHOWN_HITS))])));
        }
        return;
    }
    let hits = state.mining.find_signature(&typed);
    if hits.is_empty() {
        ui.label(RichText::new(t("s_bg_sig_nichts")).color(color(scheme.yellow)));
        return;
    }
    let count =
        if hits.len() == 1 { t("s_bg_sig_anzahl_1") } else { tf("s_bg_sig_anzahl", &[("0", Arg::from(hits.len()))]) };
    ui.label(sub(scheme, count));
    for hit in hits.iter().take(SHOWN_HITS) {
        let off = if hit.deviation.abs() >= 0.05 {
            format!("{} %", fill("{0:+.1f}", &[("0", Arg::from(hit.deviation))]))
        } else {
            t("s_bg_sig_genau")
        };
        let shown = versekit_core::signature_window::display_name(&hit.name);
        let name = tf("s_bg_sig_treffer", &[("0", Arg::from(hit.chunks)), ("1", Arg::from(shown.as_str()))]);
        sides(
            ui,
            |left| {
                left.label(RichText::new(name).color(color(scheme.accent)));
            },
            |right| {
                right.label(whole(hit.total));
                right.label(sub(scheme, off));
            },
        );
    }
}

/// The names of the ores and the places for the filters, by name.
fn choices(mining: &MiningData) -> (Vec<String>, Vec<String>) {
    let mut ores: Vec<String> = mining.ores().iter().map(|o| o.name.clone()).collect();
    ores.sort_by_cached_key(|n| lower(n));
    let mut places: Vec<String> = mining.places().iter().map(|p| p.name.clone()).collect();
    places.sort_by_cached_key(|n| lower(n));
    (ores, places)
}

/// The search and the three filters: raw material, place and tool. A raw material or place picked stands in
/// the search and opens at once.
fn search_head(ui: &mut Ui, scheme: &Scheme, state: &mut MiningState) {
    let width = ui.available_width() - crate::layout::SAFETY;
    widgets::search_field(ui, &mut state.search, &t("s_pl_bergbau"), scheme, width);
    let (ores, places) = choices(&state.mining);
    let ore_words: Vec<String> = std::iter::once(t("s_bg_alle_erze")).chain(ores.iter().cloned()).collect();
    let place_words: Vec<String> = std::iter::once(t("s_bg_alle_orte")).chain(places.iter().cloned()).collect();
    let tool_words: Vec<String> =
        std::iter::once(t("s_bg_alle_geraete")).chain(TOOLS.iter().map(|k| t(&format!("s_bg_art_{k}")))).collect();
    // A filter with one choice at most is no choice and stays away.
    let lists: Vec<(usize, &Vec<String>)> = [(0, &ore_words), (1, &place_words), (2, &tool_words)]
        .into_iter()
        .filter(|(_, words)| words.len() > 2)
        .collect();
    if lists.is_empty() {
        return;
    }
    let refs: Vec<Vec<&str>> = lists.iter().map(|(_, words)| words.iter().map(String::as_str).collect()).collect();
    let slices: Vec<&[&str]> = refs.iter().map(Vec::as_slice).collect();
    let current = [state.picked_ore, state.picked_place, state.tool];
    let mut chosen: Vec<usize> = lists.iter().map(|(which, _)| current[*which]).collect();
    if widgets::filter_bar(ui, "bergbau", &slices, &mut chosen) {
        for ((which, _), value) in lists.iter().zip(chosen) {
            match which {
                0 if value != state.picked_ore => {
                    state.picked_ore = value;
                    if let Some(name) = value.checked_sub(1).and_then(|i| ores.get(i)) {
                        state.search = name.clone();
                        state.open = Some(format!("erz:{name}"));
                    }
                }
                1 if value != state.picked_place => {
                    state.picked_place = value;
                    if let Some(name) = value.checked_sub(1).and_then(|i| places.get(i)) {
                        state.search = name.clone();
                        state.open = Some(format!("ort:{name}"));
                    }
                }
                2 => state.tool = value,
                _ => {}
            }
        }
    }
}

/// The list: the raw materials first (all of them without a search), the places only for a search. Returns
/// whether the refineries were asked for.
fn list(ui: &mut Ui, scheme: &Scheme, state: &mut MiningState) -> bool {
    let text = lower(state.search.trim());
    let tool = state.tool.checked_sub(1).and_then(|i| TOOLS.get(i)).copied();
    let ores: Vec<&Ore> = state
        .mining
        .ores()
        .iter()
        .filter(|o| text.is_empty() || lower(&o.name).contains(&text))
        .filter(|o| tool.is_none_or(|t| ore_has_tool(o, t)))
        .collect();
    // Without a search the 48 places would stand below the ores: nobody reads them.
    let dashless = text.replace('-', " ");
    let places: Vec<&Place> = if text.is_empty() {
        Vec::new()
    } else {
        state
            .mining
            .places()
            .iter()
            .filter(|p| {
                let name = lower(&p.name);
                name.contains(&text)
                    || name.contains(&dashless)
                    || lower(&p.template).contains(&text)
                    || lower(&p.system).contains(&text)
            })
            .filter(|p| tool.is_none_or(|t| place_has_tool(p, t)))
            .collect()
    };
    if ores.is_empty() && places.is_empty() {
        widgets::empty_state(ui, scheme, "scan-search", &t("s_he_nichts"));
        return false;
    }
    let mut toggled: Option<String> = None;
    let mut to_refineries = false;
    widgets::list_frame_marked(ui, scheme, |inner| {
        inner.set_width(inner.available_width());
        let mut k = 0;
        for ore in &ores {
            let key = format!("erz:{}", ore.name);
            let open = state.open.as_deref() == Some(key.as_str());
            let count =
                ore.places.iter().filter(|p| tool.is_none_or(|t| p.per_tool.iter().any(|s| s.tool == t))).count();
            let places_word =
                if count == 1 { t("s_bg_nur_orte_1") } else { tf("s_bg_nur_orte", &[("0", Arg::from(count))]) };
            let spoken = format!("{}, {places_word}", ore.name);
            let clicked = click_block(inner, scheme, k % 2 == 1, (&key, &spoken, open), |row| {
                sides(
                    row,
                    |left| {
                        left.label(RichText::new(&ore.name).color(color(scheme.accent)));
                    },
                    |right| {
                        right.label(sub(scheme, places_word.clone()));
                    },
                );
            });
            if clicked {
                toggled = Some(key.clone());
            }
            if open {
                ore_places(inner, scheme, ore, tool);
                to_refineries |= refinery_box(inner, scheme, &state.mining, &ore.name);
            }
            k += 1;
        }
        for place in &places {
            let key = format!("ort:{}", place.name);
            let open = state.open.as_deref() == Some(key.as_str());
            let right = [place.system.as_str(), place.kind.as_str()]
                .iter()
                .filter(|s| !s.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" · ");
            let spoken = if right.is_empty() { place.name.clone() } else { format!("{}, {right}", place.name) };
            let clicked = click_block(inner, scheme, k % 2 == 1, (&key, &spoken, open), |row| {
                sides(
                    row,
                    |left| {
                        left.label(&place.name);
                    },
                    |right_side| {
                        right_side.label(sub(scheme, right.clone()));
                    },
                );
            });
            if clicked {
                toggled = Some(key.clone());
            }
            if open {
                place_ores(inner, scheme, place, tool);
            }
            k += 1;
        }
    });
    // One row is open; a second click shuts it.
    if let Some(key) = toggled {
        state.open = if state.open.as_deref() == Some(key.as_str()) { None } else { Some(key) };
    }
    to_refineries
}

/// The places of an open ore, the richest first; with a tool its own share.
fn ore_places(ui: &mut Ui, scheme: &Scheme, ore: &Ore, tool: Option<&str>) {
    let mut places: Vec<&OrePlace> =
        ore.places.iter().filter(|p| tool.is_none_or(|t| p.per_tool.iter().any(|s| s.tool == t))).collect();
    if tool.is_some() {
        places.sort_by(|a, b| {
            share_at(b, tool).0.total_cmp(&share_at(a, tool).0).then_with(|| lower(&a.name).cmp(&lower(&b.name)))
        });
    }
    egui::Frame::new().inner_margin(egui::Margin { left: 18, right: 8, top: 2, bottom: 4 }).show(ui, |inner| {
        inner.set_width(inner.available_width());
        for (n, place) in places.into_iter().enumerate() {
            let (share, level, alone) = share_at(place, tool);
            let words = share_words(share, level, alone);
            striped_line(inner, scheme, n % 2 == 1, |line| {
                sides(
                    line,
                    |left| {
                        left.label(&place.name);
                        left.label(sub(scheme, place.system.clone()));
                    },
                    |right| {
                        right.label(sub(scheme, kinds_text(&place.kinds)));
                        if let Some((words, level)) = words {
                            right.label(RichText::new(words).color(color(level_color(scheme, level))));
                        }
                    },
                );
            });
        }
    });
}

/// A line of an opened list, every second one on a stripe: the value at the right edge is read with the name at
/// the left of its own line.
fn striped_line(ui: &mut Ui, scheme: &Scheme, stripe: bool, add: impl FnOnce(&mut Ui)) {
    let background = ui.painter().add(egui::Shape::Noop);
    let rect = ui.scope(add).response.rect;
    if stripe {
        ui.painter().set(background, egui::Shape::rect_filled(rect.expand2(egui::vec2(4.0, 1.0)), 0.0, color(scheme.bar)));
    }
}

/// The ores of an open place, per tool, the most first.
fn place_ores(ui: &mut Ui, scheme: &Scheme, place: &Place, tool: Option<&str>) {
    let blocks: Vec<&mining_data::ToolOres> = TOOLS
        .iter()
        .filter(|t| tool.is_none_or(|chosen| chosen == **t))
        .filter_map(|t| place.per_tool.iter().find(|b| b.tool == *t))
        .collect();
    egui::Frame::new().inner_margin(egui::Margin { left: 18, right: 8, top: 2, bottom: 4 }).show(ui, |inner| {
        inner.set_width(inner.available_width());
        for block in &blocks {
            if blocks.len() > 1 {
                inner.label(sub(scheme, t(&format!("s_bg_art_{}", block.tool))));
            }
            let mut ores = block.ores.clone();
            ores.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| lower(&a.0).cmp(&lower(&b.0))));
            let alone = ores.len() <= 1;
            for (n, (name, share, level)) in ores.iter().enumerate() {
                let words = share_words(*share, *level, alone);
                striped_line(inner, scheme, n % 2 == 1, |line| {
                    sides(
                        line,
                        |left| {
                            left.label(name);
                        },
                        |right| {
                            if let Some((words, level)) = words {
                                right.label(RichText::new(words).color(color(level_color(scheme, level))));
                            }
                        },
                    );
                });
            }
        }
    });
}

/// The color of a bonus of a refinery: better and worse as everywhere (blue and yellow, also for the colour
/// blind), nothing as muted text.
pub(crate) fn bonus_color(scheme: &Scheme, bonus: f64) -> versekit_core::theme::Color {
    if bonus == 0.0 { scheme.sub } else { widgets::change_color(scheme, bonus > 0.0) }
}

/// Below an open ore: which refinery makes the most of it and how far apart the best and the worst lie.
/// Returns whether all refineries were asked for.
fn refinery_box(ui: &mut Ui, scheme: &Scheme, mining: &MiningData, ore: &str) -> bool {
    let rows = refining::for_ore(mining, ore);
    let (Some(best), Some(worst)) = (rows.first(), rows.last()) else { return false };
    let spread = best.2 - worst.2;
    // The closed header already names the best refinery, so the list below can stay shut.
    let title = if spread == 0.0 {
        tf("s_bg_raff_klappe_gleich", &[("0", Arg::from(t("s_bg_raff_kopf"))), ("1", Arg::from(best.2))])
    } else {
        tf(
            "s_bg_raff_klappe_beste",
            &[
                ("0", Arg::from(t("s_bg_raff_kopf"))),
                ("1", Arg::from(refining::box_name(&best.0).0)),
                ("2", Arg::from(best.2)),
            ],
        )
    };
    let mut clicked = false;
    egui::Frame::new().inner_margin(egui::Margin { left: 18, right: 8, top: 4, bottom: 8 }).show(ui, |inner| {
        inner.set_width(inner.available_width());
        // The open state lives in egui's memory under the ore: kept for the session, closed at the start.
        // A header whose text wraps (the title is long), with the same Lucide chevron as `widgets::collapsing`.
        let id = inner.make_persistent_id(("refinery-fold", ore));
        let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(inner.ctx(), id, false);
        let mut label_clicked = false;
        let head = inner.horizontal_top(|head| {
            state.show_toggle_button(head, |ui, openness, response| {
                let name = if openness > 0.5 { "chevron-down" } else { "chevron-right" };
                let size = response.rect.height().max(12.0) + 4.0;
                let tint = if response.hovered() { IconTint::Hover } else { IconTint::Normal };
                if let Some(image) = crate::icons::themed(name, versekit_core::theme::current(), tint, size) {
                    image.paint_at(ui, egui::Rect::from_center_size(response.rect.center(), egui::Vec2::splat(size)));
                }
            });
            let label = egui::Label::new(&title).wrap().sense(egui::Sense::click());
            label_clicked = head.add(label).on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
        });
        if label_clicked {
            state.toggle(inner);
        }
        let body = state.show_body_indented(&head.response, inner, |inner| {
            if spread == 0.0 {
                inner.label(sub(scheme, tf("s_bg_raff_egal", &[("0", Arg::from(best.2))])));
            } else {
                for (k, (names, system, bonus)) in rows.iter().enumerate() {
                    let (first, more) = refining::box_name(names);
                    let short = match more {
                        0 => first,
                        1 => tf("s_bg_raff_weitere_1", &[("0", Arg::from(first))]),
                        _ => tf("s_bg_raff_weitere", &[("0", Arg::from(first)), ("1", Arg::from(more))]),
                    };
                    let tone = bonus_color(scheme, *bonus);
                    let frame = egui::Frame::new()
                        .fill(if k % 2 == 1 { color(scheme.bar) } else { egui::Color32::TRANSPARENT })
                        .inner_margin(egui::Margin::symmetric(6, 1));
                    frame.show(inner, |row| {
                        row.set_width(row.available_width());
                        sides(
                            row,
                            |left| {
                                left.label(short.clone());
                                left.label(sub(scheme, system.clone()));
                            },
                            |right| {
                                right.label(
                                    RichText::new(tf("s_bg_raff_zeile", &[("0", Arg::from(*bonus))]))
                                        .color(color(tone)),
                                );
                            },
                        );
                    });
                }
                inner.label(sub(scheme, tf("s_bg_raff_spanne", &[("0", Arg::from(spread.trunc() as i64))])));
            }
            let word = t("s_bg_raff_alle");
            inner.add(widgets::Button::new(&word, ButtonKind::Normal, scheme)).clicked()
        });
        clicked = body.is_some_and(|body| body.inner);
    });
    clicked
}

#[cfg(test)]
#[path = "mining_page/tests.rs"]
mod bench;
