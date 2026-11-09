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

//! The sample page: every control in every color scheme and size, the
//! reference for how Verse-Kit's pages look and behave. Shown by the example
//! `farbschemata`; the edge check below builds it without a window in every
//! size and scheme and demands that nothing is cut off.
//!
//! Every text comes from the language file, in both languages.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use crate::channel_question;
use crate::detection_page;
use crate::modules_page;
use crate::scanner_ui;
use crate::shader_page;
use crate::startprograms_page;
use egui::RichText;
use versekit_core::autostart;
use versekit_core::game_start;
use versekit_core::jsonio::Value;
use versekit_core::language::{Arg, t, tf};
use versekit_core::paths::Paths;
use versekit_core::signature_window::{Place, Size};
use versekit_core::theme::{self, Color, IconTint, Scheme};
use versekit_core::window_signal::{self, Role, Signal};
use versekit_core::{font_size, language};

mod backup_flow;
#[cfg(test)]
mod basetool_secrets;
#[cfg(test)]
mod card_width;
#[cfg(test)]
mod control_column;
mod catch_up;
mod channel_states;
mod controls;
mod desktop_rows;
mod docking;
#[cfg(test)]
mod english;
#[cfg(test)]
mod silent_buttons;
#[cfg(test)]
mod back_bar;
#[cfg(test)]
mod filter_jump;
#[cfg(test)]
mod handle_tabs;
#[cfg(test)]
mod nav_groups;
mod crash_hint;
mod first_backup_hint;
mod glance;
mod glance_tiles;
mod layout_probe;
#[cfg(test)]
mod overview_clip;
mod page_glance;
mod preview;
#[cfg(test)]
mod real_data;
mod setup_wizard;
pub use setup_wizard::SetupEnd;
#[cfg(test)]
mod ship_plan;
mod start_page;
#[cfg(test)]
mod streamer_mode;
#[cfg(test)]
mod trade_pages;
#[cfg(test)]
mod update_ui;

use crate::layout::{self, GAP};
use crate::widgets::{self, ButtonKind};
use crate::{
    apply, axes_page, basetool_page, blickwinkel_page, blueprint_list, blueprint_progress, color, contested_page,
    contract_log, crafting_page, data_folder, demand_page, diagnose_page, equipment_box, extension, farm_page,
    game_texts_page, grafik_page, hangar_page, highlight, icons, import_page, joysticks_page, mining_page, notes_page,
    org_stock_page, refineries_page, routes_page, salvage_page, selling_page, settings, shopping_page, shops_page,
    statistics_page, stock_page, stock_share, thanks_page, tiles, trade, update_page,
};

const SYSTEMS: [&str; 4] = ["Stanton", "Pyro", "Nyx", "Castra"];
/// The made-up version the update bar of the sample page names: newer than the program itself.
const SAMPLE_UPDATE: &str = "9.9";
/// How often the foot bar looks at the process list and the game log on real data (seconds).
const GAME_LOOK_SECONDS: f64 = 2.0;

/// Whether the game is in the process list (`game_start::game_process`); `None` when it cannot be read or a
/// made-up machine of a test asks not to look (`game_start::ENV_NO_PROCESS`).
fn game_process_of(paths: &Paths) -> Option<bool> {
    use versekit_core::game_start;
    if paths.host.var(game_start::ENV_NO_PROCESS).is_some() { None } else { game_start::game_process() }
}

/// What the foot bar says about the game, as the overlay and the tile „Spiel“ do (`overlay::game_running`): the
/// process list decides, a log still written in the last moments of the end counts; without the list the log
/// alone. Only words: what may write into the game asks `game_start::run_state`.
fn game_running_for_foot(paths: &Paths, process: Option<bool>) -> bool {
    versekit_core::overlay::game_running(process, versekit_core::overlay::log_age(paths))
}
/// The blueprints of the sample: (name, kind), both language keys.
const BLUEPRINTS: [(&str, &str); 5] = [
    ("s_mu_n_arclight", "s_mu_art_waffe"),
    ("s_mu_n_morozov", "s_mu_art_ruestung"),
    ("s_mu_n_fr66", "s_mu_art_schiffsteil"),
    ("s_mu_n_centurion", "s_mu_art_waffe"),
    ("s_mu_n_pembroke", "s_mu_art_ruestung"),
];
/// The own stock in the sample: (material, SCU, worth in aUEC, badges: 0
/// stolen, 1 remainder, 2 three in the unit).
const STOCK: [(&str, f64, f64, &[u8]); 4] = [
    ("Titanium", 188.5, 75_400.0, &[0, 1]),
    ("Quantainium", 40.0, 352_000.0, &[]),
    ("Laranite", 12.125, 10_900.0, &[2]),
    ("Gold", 3.0, 18_600.0, &[]),
];
/// The best routes of the sample: (name as on the tile, aUEC).
const ROUTES: [(&str, f32); 3] = [
    ("Laranite: Lorville → Area18", 41_200.0),
    ("Agricium: Orison → New Babbage", 28_900.0),
    ("Gold: CRU-L1 → ARC-L1", 17_300.0),
];
/// Sample names for the field with suggestions, enough starting with `Bu` to
/// show the limit of eight and the line with the further matches. A language
/// key stands for its text, a name from the game for itself (see [`label`]).
const NAMES: [&str; 34] = [
    "Buzzard",
    "Buccaneer",
    "s_mu_n_bulwark",
    "s_mu_n_burst",
    "Bumblebee",
    "Buran",
    "Bushwhacker",
    "Butterfly",
    "Buoy",
    "s_mu_n_bulldog",
    "Super Hornet",
    "Hornet Ghost",
    "s_mu_n_arclight",
    "s_mu_n_morozov",
    "s_mu_n_fr66",
    "s_mu_n_centurion",
    "s_mu_n_pembroke",
    "Cutlass Black",
    "Cutlass Red",
    "Caterpillar",
    "Constellation Andromeda",
    "Freelancer",
    "Prospector",
    "Vulture",
    "Reclaimer",
    "Carrack",
    "Hammerhead",
    "Mercury Star Runner",
    "Gladius",
    "Sabre",
    "Arrow",
    "Avenger Titan",
    "Aurora MR",
    "Titanium",
];
/// Name, amount in SCU, quality, for the sample table.
const ORES: [(&str, i64, i64); 5] = [
    ("Titanium", 188, 295),
    ("Quantainium", 40, 612),
    ("Laranite", 96, 480),
    ("Agricium", 12, 701),
    ("Gold", 230, 155),
];
/// Play time per weekday in hours, Monday first, for the line chart.
const HOURS: [f32; 7] = [1.5, 0.0, 2.5, 1.0, 4.0, 6.5, 5.0];
/// Entries of the sample list with row buttons, as language keys.
const ENTRIES: [&str; 3] = ["s_mu_n_arclight", "s_mu_n_morozov", "s_mu_n_fr66"];

/// The short names of the weekdays, Monday first, in the language in force.
fn days() -> Vec<String> {
    t("s_sx_tage").split(',').map(str::to_string).collect()
}

/// [`HOURS`] with the names of the weekdays.
fn hours() -> Vec<(String, f32)> {
    days().into_iter().zip(HOURS).collect()
}

/// A number with `places` decimals, written the way the language in force does.
fn number(value: f64, places: usize) -> String {
    versekit_core::formats::fixed(language::current(), value, places)
}
/// Narrowest width of one of the two sections before they stack.
const SECTION_MIN_WIDTH: f32 = 460.0;
/// Quick choices of the interface size: (name in `font_size::PRESETS`, label as
/// a language key or a technical name that reads the same in both languages).
const SIZE_PRESETS: [(&str, &str); 5] = [
    ("auto", "s_gr_auto"),
    ("fullhd", "s_gr_fullhd"),
    ("wqhd", "s_gr_wqhd"),
    ("uhd125", "UHD 125 %"),
    ("uhd150", "UHD 150 %"),
];
/// The labels of [`SIZE_PRESETS`], for the setting row and the menu filter.
const SIZE_LABELS: [&str; 5] = ["s_gr_auto", "s_gr_fullhd", "s_gr_wqhd", "UHD 125 %", "UHD 150 %"];

/// The interface size of a quick choice on this screen.
fn preset_points(ctx: &egui::Context, name: &str) -> i64 {
    let points = font_size::PRESETS.iter().find(|(n, _)| *n == name).and_then(|(_, p)| *p);
    let points = points.unwrap_or_else(|| crate::screen_height(ctx).map_or(1, font_size::auto_points));
    points.min(crate::max_size(ctx))
}

/// The slider of the interface size with its percent; it takes the value on
/// release (a size that changes while dragging moves the slider away).
fn size_slider(ui: &mut egui::Ui, scheme: &Scheme, size: &mut i64) {
    let largest = crate::max_size(ui.ctx());
    let range = (font_size::POINTS_RANGE.0, largest);
    settings::slider_with_value(ui, scheme, &t("hf_schrift"), size, range, true, |shown| {
        format!("{} %", font_size::font_percent(shown))
    });
}

/// The size of a window of its own (overlay, info bar) as a slider over the steps of
/// `versekit_core::overlay::SIZE_STEPS`, the percent beside it; taken when the slider is let go.
fn window_size_slider(ui: &mut egui::Ui, scheme: &Scheme, name: &str, percent: &mut i64) {
    let steps = versekit_core::overlay::SIZE_STEPS;
    let last = steps.len() as i64 - 1;
    let at = |step: i64| steps[step.clamp(0, last) as usize];
    let mut step = steps.iter().position(|s| *s == *percent).unwrap_or(0) as i64;
    settings::slider_with_value(ui, scheme, name, &mut step, (0, last), true, |shown| format!("{} %", at(shown)));
    *percent = at(step);
}

/// The quick choices as a row of buttons; the one the size matches now is
/// filled. Below: why it cannot get bigger, when the screen is too small.
fn size_presets(ui: &mut egui::Ui, scheme: &Scheme, size: &mut i64) {
    ui.horizontal_wrapped(|ui| {
        for (name, text) in SIZE_PRESETS {
            let points = preset_points(ui.ctx(), name);
            let kind = if points == *size { ButtonKind::Strong } else { ButtonKind::Normal };
            if ui.add(widgets::Button::new(&label(text), kind, scheme)).clicked() {
                *size = points;
            }
        }
    });
    if crate::max_size(ui.ctx()) < font_size::POINTS_RANGE.1 {
        ui.label(RichText::new(t("s_ei_groesse_max")).small().color(color(scheme.sub)));
    }
}

/// Where the community mark leads: Star Citizen with Xharig's referral code.
const REFERRAL_URL: &str = "https://robertsspaceindustries.com/enlist?referral=STAR-YC7L-S4W2";
/// Lowest height of the window model; in the preview it fills the window.
const WINDOW_MODEL_HEIGHT: f32 = 460.0;

/// The two ends of the channel on which a start of the game reports how it went.
type StartDone = (std::sync::mpsc::Sender<Result<(), String>>, std::sync::mpsc::Receiver<Result<(), String>>);

/// State of the sample page.
pub struct SamplePage {
    applied: Option<&'static str>,
    /// Only the window model, to pass on and try out (no catalog below).
    preview: bool,
    /// Height of the window model: the room the window has in the preview.
    frame_height: f32,
    search: String,
    picked: Option<usize>,
    /// The value of the searchable select field of the sample.
    choice: String,
    dialog_open: bool,
    text: String,
    notes: String,
    toggle: bool,
    value: i64,
    system: usize,
    sort: usize,
    switches: [bool; 2],
    tab: usize,
    row: usize,
    quality: String,
    /// Sorted table column and whether ascending.
    table_sort: (usize, bool),
    /// When the sample notice came (`ui.input(|i| i.time)`).
    notice_at: Option<f64>,
    /// When a page was pinned or loosened, and what to say.
    pin_notice: Option<(f64, String)>,
    /// Where the start with the system lives and what it runs. In memory unless the program on the user's own
    /// data hands over the real one ([`SamplePage::use_autostart`]): a copy of the data or a test never writes
    /// the registry or the autostart folder.
    autostart: Box<dyn autostart::Store>,
    autostart_command: String,
    /// What the switch shows: the entry as it was last read.
    autostart_on: bool,
    /// The rows of the page General that differ by system (icon, message of the system, menu entry).
    desktop: desktop_rows::DesktopRows,
    /// The text of a `pin_notice` that tells of a fault: that line shows with the warning symbol instead
    /// of the pin (any other line replacing it shows with the pin).
    pin_fault: Option<String>,
    heat: Vec<Vec<f64>>,
    /// Size of the whole interface in points (0 … 7, normal = 1).
    pub size: i64,
    size_applied: Option<i64>,
    entries: Vec<&'static str>,
    /// The deleted entry for undo: when, where, which.
    undo: Option<(f64, usize, &'static str)>,
    row_note: Option<String>,
    /// When the sample loading last failed.
    failed_at: Option<f64>,
    /// Chosen page of the window model; whether the page with the new sign was opened.
    page: String,
    nav_filter: String,
    /// Enter was pressed in the filter of the menu: the first hit opens in this frame.
    nav_enter: bool,
    history: widgets::History,
    /// How far a tile's content stuck out in the last frame.
    tile_overflow: f32,
    /// How far the open page of the window model stuck out in the last frame.
    frame_overflow: f32,
    /// What the window model needs besides sidebar and page (bars, borders),
    /// measured on the frame before.
    frame_extra: f32,
    /// How far the preview stuck out at the bottom in the last frame.
    frame_overflow_y: f32,
    /// Graphics card: as set, and as Verse-Kit started with.
    gpu: bool,
    gpu_started: bool,
    /// The 24-hour clock (`formats::CLOCK_24_SETTING`); off, the times of day are written with AM and PM.
    clock_24: bool,
    /// The later choice in the restart bar: hidden until the next change.
    restart_later: bool,
    mangohud: bool,
    /// Streamer mode: private details are hidden on the screen (`versekit_core::streamer`).
    streamer: bool,
    /// The daily usage report and the counting for it are on (`usage::SETTING`).
    usage_on: bool,
    /// The page "Shader-Cache".
    shader: shader_page::ShaderState,
    grafik: grafik_page::GrafikState,
    fov: blickwinkel_page::FovState,
    joysticks: joysticks_page::JoystickState,
    axes: axes_page::AxesState,
    start_programs: startprograms_page::StartProgramsState,
    modules: modules_page::ModulesState,
    detection: detection_page::DetectionState,
    /// The pages whose module is switched off (read with the settings, again when a switch is flipped).
    module_hidden: Vec<&'static str>,
    /// The question after the game folder moved (LIVE became HOTFIX).
    channel_question: channel_question::ChannelQuestion,
    /// The own start command as typed on the folder page (stored with its button).
    start_command: String,
    /// Whether a way to start the game is there and MangoHud is set up, looked up in a thread of its own.
    start_look: game_start::Look,
    /// What starts the game: the real start; the tests put a recorder in its place.
    spawner: std::sync::Arc<dyn game_start::Spawner>,
    /// How a start went (the lookup and the start run in a thread of their own).
    start_done: StartDone,
    /// When the copy-version button was clicked.
    copied_at: Option<f64>,
    /// The blueprints the list shows: the user's catalog or the sample.
    blueprints: blueprint_list::ListData,
    /// On real data: there is no catalog and none is being built (no game
    /// data, no scmdb); the list and the progress say so.
    blueprints_none: bool,
    /// What the blueprint list is set to.
    blueprint_view: blueprint_list::ListState,
    /// What the blueprint progress is set to.
    progress_view: blueprint_progress::ProgressState,
    /// The own ships and the hangar delivery from the browser extension.
    hangar: hangar_page::HangarState,
    /// The game's texts: blueprint notes, translation per channel, and the
    /// background keeping both current.
    game_texts: game_texts_page::GameTextsState,
    /// The own stock: materials, goods and items.
    stock: stock_page::StockState,
    /// Reading in a blueprint stand (page "Backup & reset").
    import: import_page::ImportState,
    /// Everything craftable, with the recipe and the stock against it.
    crafting: crafting_page::CraftingState,
    /// What is still to be farmed to craft what is planned.
    farm: farm_page::FarmState,
    /// Where which ore lies, the scan signature and the refineries: one state for both pages.
    mining: mining_page::MiningState,
    /// What a wreck has on board and what it is worth in a shop; what the fabricator gives back for a part.
    salvage: salvage_page::SalvageState,
    /// The view of that page the usage counters last counted.
    salvage_counted: Option<&'static str>,
    /// What the ships still lack, as a bill with the sum and the route.
    shopping: shopping_page::ShoppingState,
    /// The memo pages.
    memo_page: notes_page::NotesState,
    /// Writes held back by parts that signed up while the window runs, written when it closes.
    close_writes: crate::close_writes::SignedUp,
    /// The cycle of the Executive Hangar and the self timers of the Contested Zones.
    contested: contested_page::ContestedState,
    server_status: crate::serverstatus_page::ServerStatusState,
    patch_changes: crate::patch_changes_page::PatchChangesState,
    /// The handle search with the list of the handles and organizations marked.
    handles: crate::handle_page::HandlePageState,
    /// A self timer ran out: when, and what the notice says.
    contested_notice: Option<(f64, String)>,
    /// The page shown in the last frame, to notice that one is entered.
    shown_page: String,
    /// The counting of pages and actions for the daily usage report (`page_usage`), on real data only.
    usage: Option<std::sync::Arc<versekit_core::usage::PageUsage>>,
    /// How the page asked for since the last frame was opened (`page_usage.ROUTES`); `None` is a
    /// page the program changed by itself, which counts not.
    usage_route: Option<&'static str>,
    /// Whether this search in the menu filter (from a word typed into the empty field until it is empty again)
    /// was counted as used (`suche_benutzt`) and as without hits (`suche_ohne_treffer`), each once.
    search_counted: (bool, bool),
    /// Whether the opening of the window was counted.
    usage_started: bool,
    /// Where goods are sold or bought (Kaufen & Verkaufen).
    selling: selling_page::SellingState,
    /// The trade routes.
    routes: routes_page::RoutesState,
    /// The shops: where a part stands and what it costs.
    shops: shops_page::ShopsState,
    /// The unit storage, against the made-up Basetool of the preview.
    org_stock: org_stock_page::OrgStockState,
    /// What the units still need (in memory only), on the sync of the connection.
    demand: demand_page::DemandState,
    /// The connection to the made-up Basetool; the unit storage is there only
    /// with it connected, its switch on and its right granted.
    connection: basetool_page::ConnectionState,
    /// The sync button in the head of the blueprint list, My stock and hangar.
    head_sync: crate::head_sync::HeadSync,
    /// The mission log: the user's saved log or the sample, and what the page is set to.
    contracts: contract_log::LogData,
    contract_view: contract_log::LogState,
    /// The six statistics pages: the numbers of the data folder, or made-up ones.
    stats: statistics_page::StatsState,
    /// Errors written down so far: the report button pale or strong.
    error_count: u32,
    /// The error report page; only with own data (it needs a data folder).
    diagnose: Option<diagnose_page::DiagnoseState>,
    /// The thanks page.
    thanks: thanks_page::ThanksState,
    /// All sidebar groups folded by the button next to the menu filter.
    nav_folded: bool,
    /// The game folder per channel: LIVE, PTU, TECH-PREVIEW.
    folders: [String; CHANNEL_COUNT],
    /// The game folder the program really finds (a Game.log lies in it), on real data.
    game_found: Option<String>,
    /// Where the game was looked for, shown while it is not found (read with the settings, not every frame).
    game_searched: Vec<String>,
    /// The data folder on the page "Installation & Pfade": questions, move, report.
    data_folder: data_folder::DataFolderState,
    /// The state of the user.cfg of every game channel, at the foot of the page Installation & Pfade.
    channel_states: channel_states::ChannelStates,
    /// View range, and whether a change waits for the next game start.
    view_range: i64,
    view_pending: bool,
    /// The question before restoring a backup, and its switch.
    restore_open: bool,
    backup_first: bool,
    /// The hotkey of the overlay: Strg, Alt, Umschalt and the key.
    hotkey_modifiers: [bool; 3],
    hotkey_key: usize,
    /// The hotkey of the refinery scan (`hotkey_scan`) and whether it is used (`hotkey_scan_an`).
    scan_modifiers: [bool; 3],
    scan_key: usize,
    scan_hotkey_on: bool,
    /// The overlay's own hotkey is registered (`hotkey_an`, only a literal `false` switches it off; no switch here).
    hotkey_on: bool,
    /// The hotkey of the handle search in the info bar (`hotkey_suche`) and whether it is used (`hotkey_suche_ein`).
    search_modifiers: [bool; 3],
    search_key: usize,
    search_hotkey_on: bool,
    /// The hotkey of the picture of the game for the signature window (`hotkey_bild`) and whether it is used
    /// (`hotkey_bild_an`).
    picture_modifiers: [bool; 3],
    picture_key: usize,
    picture_hotkey_on: bool,
    /// The hotkey of the main window (`hotkey_hauptfenster`) and whether it is used (`hotkey_hauptfenster_an`).
    main_modifiers: [bool; 3],
    main_key: usize,
    main_hotkey_on: bool,
    /// Whether the overlay runs: it registers the hotkeys, so only then a new one counts as registered.
    overlay_runs: fn() -> bool,
    /// The window shows a test build (sample page: a click on the version).
    test_build: bool,
    /// The highlight shown in the what's-new gallery.
    news_index: usize,
    /// The menu filter hit clicked last: only it lights up.
    nav_hit: String,
    overview_two: bool,
    /// Order of the navigation groups (indices into `NAV`); Basetool order first.
    group_order: Vec<usize>,
    /// The group being dragged (its place in the order).
    dragging_group: Option<usize>,
    mine_first: bool,
    /// The sample update was loaded: the bar goes.
    update_loaded: bool,
    /// The version the update bar names: made up on the sample page, empty on real data
    /// until an embedding program names one. Empty: no bar.
    update_version: String,
    /// Whether the game ran at the last look on real data, and when that was (seconds).
    game_seen: Option<(f64, bool)>,
    /// How the foot bar asks the process list for the game ([`game_process_of`]; a stand-in in tests).
    game_process: fn(&Paths) -> Option<bool>,
    /// What the overview tiles and the head bar count on real data; `None` on the sample page.
    overview: Option<glance::Overview>,
    /// What loads right now in the foot bar: since, what, seconds it takes.
    loading: Option<(f64, String, f64)>,
    /// What loads is the new version (then the what is new gallery opens by itself).
    loading_update: bool,
    /// The start demo of the foot bar ran.
    loading_shown: bool,
    /// Update page: offer test versions; look for versions, install them by itself.
    update_test: bool,
    /// Updating from the window on real data: the version on offer, the question, the fetching.
    updates: update_page::UpdateUi,
    /// The page „Was ist neu“ on real data.
    news: crate::news_page::NewsState,
    /// An update was handed over: the program has to end (the main program closes the window).
    update_quit: bool,
    /// A setting found by the menu filter and when.
    highlight: Option<(String, f64)>,
    /// The click-through switch, the only switch of the overlay; it comes and folds with the game.
    overlay_clickable: bool,
    /// The switches of "Allgemein" on the sample page: start with the system (0), 24-hour
    /// clock (2). None is stored; on real data both say they come later.
    general_switches: [bool; 4],
    general_english: bool,
    /// The language follows the system (`sprache` = `auto`); a click on a language of the page ends that.
    language_auto: bool,
    /// The play time stands in the head bar (`spielzeit_zeigen`); the sample page shows it.
    play_head: bool,
    /// Seconds between two looks at Game.log (`pruefintervall_sekunden`, 1 to 60).
    check_interval: i64,
    /// After a jump out of a page: (the page it came from, the page it led to). The way back stands
    /// above the page it led to while that page is open.
    came_from: Option<(String, String)>,
    /// Giving out and resetting the blueprint collection on the page "Sichern".
    collection_tools: crate::collection_tools::CollectionTools,
    /// The result of reading all logs again, told as a notice that stays.
    rescan_notice: crate::collection_tools::RescanNotice,
    /// Which groups of the menu are folded, in the order of [`GROUP_SETTINGS`].
    groups_closed: [bool; GROUP_SETTINGS.len()],
    /// The language the folded groups were last handed to the menu in (their state is kept by the shown name).
    groups_language: Option<language::Lang>,
    /// The setup assistant, while it is open.
    setup: Option<setup_wizard::Wizard>,
    /// The button "Sicherung": the choice, the dialogs and the work behind it.
    backup: backup_flow::BackupFlow,
    /// The file of the backup of the first start, while the note about it waits or is open.
    first_backup_hint: Option<String>,
    /// The note was drawn and written down as told.
    first_backup_told: bool,
    /// The crash of the last run, while the note about it waits or is open, and whether it was drawn and written
    /// down as told.
    crash_hint: Option<versekit_core::crash::Untold>,
    crash_told: bool,
    /// The folders a test asked to open (nothing opens in a test).
    #[cfg(test)]
    folders_opened: Vec<String>,
    overlay_place: settings::OverlayPlace,
    bar_side: settings::BarSide,
    wizard: settings::WizardState,
    wizard_english: bool,
    wizard_overlay: bool,
    tiles: Vec<tiles::Tile>,
    /// On real data: what the settings keep of the start page beyond the board.
    start: Option<start_page::Kept>,
    /// Where the player is, for the location tile, read beside the window.
    place_look: glance_tiles::PlaceLook,
    /// How the game rated the computer, for its tile, read beside the window.
    machine_look: glance_tiles::MachineLook,
    /// The state of the servers of CIG, for the services tile, asked beside the window.
    status_look: glance_tiles::StatusLook,
    /// The pages whose area carries the mark "NEU" (`controls::NEW_SINCE`, gone once the page was open).
    new_pages: Vec<&'static str>,
    /// The gallery of what is new while it is open (once after an update).
    gallery: Option<crate::gallery_page::GalleryState>,
    overlay_rows: i64,
    /// Clicks pass through while the game hides the pointer.
    overlay_auto: bool,
    /// Background opacity of the overlay in percent.
    overlay_opacity: i64,
    /// The overlay last started without transparency (`renderer::overlay_opaque`): the opacity has a hint of its own.
    overlay_opaque: bool,
    /// The size of the overlay in percent of the main window's size (`versekit_core::overlay::SIZE_STEPS`).
    overlay_size: i64,
    /// Only on a new blueprint (pop-up mode), and for how many seconds.
    overlay_popup: bool,
    overlay_seconds: i64,
    /// What opens the folded or waiting overlay (blueprint, scanner, contract, refinery, catalog).
    overlay_events: versekit_core::overlay::Events,
    /// What the open overlay shows besides its rows (server, place, contracts, update), all on by default.
    overlay_sections: versekit_core::overlay::Sections,
    /// The most contracts the game allows at a time, for the head of the bar in the overlay (`3/10`).
    overlay_contracts_max: i64,
    /// The frame rates beside the shard in the overlay (read from the picture of the game, off by default).
    overlay_fps: bool,
    /// The frame rate of the server and the players from the same picture, each on its own switch.
    overlay_server_fps: bool,
    overlay_players: bool,
    /// The info bar, a small window of its own over the game (off by default).
    overlay_info_bar: bool,
    /// The info bar shows while the game does not run too (off by default).
    overlay_info_bar_always: bool,
    overlay_always: bool,
    /// Where the info bar starts at the top while it was not dragged.
    overlay_info_bar_corner: versekit_core::info_bar::Corner,
    /// The size of the info bar in percent of the main window's size, in the same steps.
    overlay_info_bar_size: i64,
    /// The overlay place was reset, and when.
    place_reset_at: Option<f64>,
    /// The overlay brought back waits hidden without the game: the notice of `place_reset_at` says so.
    place_reset_waits: bool,
    sounds: [bool; 3],
    volume: i64,
    saved_at: Option<f64>,
    selection: widgets::Selection,
    revealed: bool,
    /// Where the settings are written; none in the sample and the preview.
    store: Option<SettingsStore>,
    /// Another window changed the settings; read again once own writes are done.
    reload_waiting: bool,
    /// The window place as last written, and one seen since with the time it was seen.
    window: Option<WindowPlace>,
    window_seen: Option<(WindowPlace, f64)>,
    /// The pages an embedding program added to the sidebar, if any.
    extension: Option<extension::Slot>,
    /// The page of the last frame, to open the extension's pages when one is chosen.
    extension_page: String,
    /// An embedding program gave the unit storage its own connection: the page is there
    /// whatever the sync of this window says.
    storage_given: bool,
    /// What reaches outside the window goes through these, so a test can ask what was asked for:
    /// open an address, ask for a folder, start the program again.
    open_url: fn(&str) -> bool,
    choose_folder: controls::ChooseFolder,
    restart_with: fn(&[String]) -> std::io::Result<()>,
    /// The folder dialog that is open: for which game folder.
    folder_dialog: Option<(usize, crate::file_dialog::Pending)>,
    /// The LIVE folder as it counts for the settings: the field is typed into, and this follows
    /// when the field is left or Enter is pressed.
    live_folder_saved: String,
    /// The folders chosen for the other channels as they count for the settings
    /// (`kanal_ordner`, other entries of it kept); a channel without one is found next to LIVE.
    channels_saved: versekit_core::jsonio::Map,
    /// The folder of each channel next to LIVE, shown while none is chosen.
    channels_found: [String; CHANNEL_COUNT],
    /// When the error log was looked at for its count, and its stamp (time, size) then.
    errors_looked: Option<(f64, Option<(std::time::SystemTime, u64)>)>,
    /// A tool connected the real Basetool under its own name: the name and the lock of the bookings.
    real_basetool: Option<(String, Option<String>)>,
    /// Why a typed or chosen game folder was not taken (not there, or already another channel's), to
    /// be said once.
    folder_refused: Option<String>,
    /// The search for the game folder below a typed or chosen folder, while it runs.
    folder_search: Option<controls::FolderSearch>,
    /// The window has no frame of the system: the head bar is its title bar, with buttons of its own, and the
    /// edges can be dragged to resize.
    own_frame: bool,
}

/// Where the window was, in screen pixels: its inner size, the top left
/// corner of its frame, and whether it was maximized. Size and place are the
/// ones before maximizing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowPlace {
    pub size: (i64, i64),
    pub position: Option<(i64, i64)>,
    pub maximized: bool,
}

/// Settings of the window place; the size as 4.12 writes it (`1229x1186`).
const WINDOW_SIZE_KEY: &str = "fenster_groesse";
const WINDOW_POSITION_KEY: &str = "fenster_lage";
const WINDOW_MAXIMIZED_KEY: &str = "fenster_maximiert";
/// How long the window must stay still before its place is written.
const WINDOW_SETTLE_SECONDS: f64 = 0.5;

/// A text like `1229x1186` or `-1920,40` as two numbers.
fn two_numbers(text: &str, separator: char) -> Option<(i64, i64)> {
    let (a, b) = text.trim().split_once(separator)?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

impl WindowPlace {
    /// The remembered place from the settings; `None` without a size.
    pub fn from_settings(paths: &Paths) -> Option<WindowPlace> {
        let size = two_numbers(&paths.setting(WINDOW_SIZE_KEY)?.to_lowercase(), 'x')?;
        let position = paths.setting(WINDOW_POSITION_KEY).and_then(|p| two_numbers(&p, ','));
        Some(WindowPlace { size, position, maximized: paths.setting_bool(WINDOW_MAXIMIZED_KEY, false) })
    }

    fn values(&self) -> [(&'static str, Value); 3] {
        let position = self.position.map_or_else(String::new, |(x, y)| format!("{x},{y}"));
        [
            (WINDOW_SIZE_KEY, Value::str(format!("{}x{}", self.size.0, self.size.1))),
            (WINDOW_POSITION_KEY, Value::str(position)),
            (WINDOW_MAXIMIZED_KEY, Value::Bool(self.maximized)),
        ]
    }
}

/// The handle of the system for `window` as `window_signal::serve` takes it: the window under Windows, 0 elsewhere
/// (there the mark belongs to the process).
pub fn system_handle(window: &impl raw_window_handle::HasWindowHandle) -> isize {
    match window.window_handle().map(|h| h.as_raw()) {
        Ok(raw_window_handle::RawWindowHandle::Win32(h)) => h.hwnd.get(),
        _ => 0,
    }
}

/// What the overlay asked of the main window, kept until the next frame.
#[derive(Clone, Default)]
pub struct MainSignals {
    overview: Arc<AtomicBool>,
    /// Shown on the page it shows (the hotkey of the main window).
    show: Arc<AtomicBool>,
    settings: Arc<AtomicBool>,
    close: Arc<AtomicBool>,
    /// The window stays open whatever the overlay says (a restore is starting the program anew: the overlay
    /// closes first and asks this window to go with it, which would end the restart).
    keep_open: Arc<AtomicBool>,
    /// The overlay closed and asked this window to end for good: its closing is no hiding any more.
    ended: Arc<AtomicBool>,
    /// The page the overlay asked for (the code of its signal), 0 for none: the last request counts.
    page: Arc<AtomicU8>,
    /// The overlay asked for the demand of the unit (it started).
    demand_wanted: Arc<AtomicBool>,
    /// The demand last handed to the overlay, and when it was last tried.
    demand_handed: Arc<std::sync::Mutex<DemandHanded>>,
}

/// What the main window last handed the overlay of the demand of the unit.
#[derive(Default)]
struct DemandHanded {
    bytes: Option<Vec<u8>>,
    tried: Option<std::time::Instant>,
}

/// A change of the demand is handed to the overlay at most this often while no overlay takes it.
const DEMAND_RETRY: std::time::Duration = std::time::Duration::from_secs(2);

impl DemandHanded {
    /// Whether `bytes` go to the overlay now: when it asked, or when they changed and the last try is long enough
    /// ago. Marks them as handed when they go.
    fn due(&mut self, bytes: &[u8], asked: bool, now: std::time::Instant) -> bool {
        let changed = self.bytes.as_deref() != Some(bytes);
        let waited = self.tried.is_none_or(|at| now.duration_since(at) >= DEMAND_RETRY);
        if asked || (changed && waited) {
            self.tried = Some(now);
            return true;
        }
        false
    }

    fn handed(&mut self, bytes: Vec<u8>) {
        self.bytes = Some(bytes);
    }
}

impl MainSignals {
    /// Marks `window` as the main window: the overlay finds it, brings it to
    /// the front on the overview, says when it changed the settings and
    /// closes it when the overlay closes. `ctx` is woken for every signal.
    pub fn serve(window: &impl raw_window_handle::HasWindowHandle, ctx: &egui::Context) -> MainSignals {
        let signals = MainSignals::default();
        let (noted, ctx) = (signals.clone(), ctx.clone());
        // The own title bar answers for its maximize button, so that Windows 11 shows the snap layouts there.
        let button = versekit_core::snap_layouts::MaxButton::new();
        let woken = ctx.clone();
        if versekit_core::snap_layouts::install(system_handle(window), button.clone(), move || woken.request_repaint())
        {
            ctx.data_mut(|d| d.insert_temp(egui::Id::unique(SNAP_BUTTON_ID), button));
        }
        window_signal::serve(system_handle(window), Role::Main, move |signal| {
            noted.note(signal);
            ctx.request_repaint();
        });
        signals
    }

    /// Keeps `signal` for the next frame.
    pub fn note(&self, signal: Signal) {
        let flag = match signal {
            Signal::Overview => &self.overview,
            Signal::Show => &self.show,
            Signal::Settings => &self.settings,
            Signal::Close => &self.close,
            Signal::OpenGeneral
            | Signal::OpenTranslation
            | Signal::OpenAbout
            | Signal::OpenMining
            | Signal::OpenStockScan => {
                self.page.store(signal as u8, Ordering::Relaxed);
                return;
            }
            Signal::DemandWanted => &self.demand_wanted,
            // Anti-AFK is switched in the overlay; the main window is the one who asks.
            Signal::AntiAfkOn | Signal::AntiAfkOff | Signal::ShowFrame => return,
        };
        flag.store(true, Ordering::Relaxed);
    }

    /// Does what was asked since the last frame: reads the settings again,
    /// shows the overview, closes the window. Called at the start of a frame.
    pub fn apply(&self, ctx: &egui::Context, page: &mut SamplePage) {
        if self.settings.swap(false, Ordering::Relaxed) {
            page.reload_settings();
        }
        self.hand_demand(page, window_signal::find, |overlay, bytes| {
            window_signal::send_data(overlay, window_signal::DataKind::Demand, bytes)
        });
        let shown = self.overview.swap(false, Ordering::Relaxed);
        if shown {
            page.open_overview();
            page.usage_route = Some("overlay");
        }
        // After the overview: a page asked for in the same breath wins.
        let code = self.page.swap(0, Ordering::Relaxed);
        // The hotkey of the overlay read the refinery terminal: the reading waits for the page.
        if code == Signal::OpenStockScan as u8 {
            page.stock.take_refinery_reading();
        }
        let (key, group) = match code {
            code if code == Signal::OpenGeneral as u8 => ("hf_allgemein", "hf_gruppe_einst"),
            code if code == Signal::OpenTranslation as u8 => ("hf_uebersetzung", "hf_gruppe_spiel"),
            code if code == Signal::OpenAbout as u8 => ("hf_ueber", "hf_gruppe_info"),
            code if code == Signal::OpenMining as u8 => ("hf_bergbau", "hf_gruppe_abbau"),
            code if code == Signal::OpenStockScan as u8 => ("hf_meinlager", "hf_gruppe_logistik"),
            _ => ("", ""),
        };
        if !key.is_empty() {
            page.open_page(ctx, key, group);
            page.usage_route = Some("overlay");
        }
        // The hotkey of the main window shows it without changing its page.
        let only_shown = self.show.swap(false, Ordering::Relaxed);
        // A window the player closed is hidden: whatever asks for a page shows it again.
        if shown || only_shown || !key.is_empty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        }
        if self.close.swap(false, Ordering::Relaxed) && !self.keep_open.load(Ordering::Relaxed) {
            self.ended.store(true, Ordering::Relaxed);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Hands the demand of the unit to the running overlay (`find` gives it, `send` hands the bytes over) when the
    /// overlay asked for it or it changed: all wanted materials while the page "Bedarf der Einheit" is there, none
    /// otherwise. Only the real Basetool: the made-up one of the preview stays in its window. In memory only.
    fn hand_demand(
        &self,
        page: &SamplePage,
        find: impl Fn(Role) -> Option<isize>,
        send: impl Fn(isize, &[u8]) -> bool,
    ) {
        use versekit_core::basetool::{demand, scopes::SETTING_DEMAND};
        let asked = self.demand_wanted.swap(false, Ordering::Relaxed);
        let Some(wire) = page.connection.wire().filter(|wire| wire.demo.is_none()) else { return };
        let wanted = wire.demand.for_overlay(wire.sync.area_visible(SETTING_DEMAND), wire.sync.now());
        let bytes = demand::handover(&wanted);
        let mut handed = self.demand_handed.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if !handed.due(&bytes, asked, std::time::Instant::now()) {
            return;
        }
        if let Some(overlay) = find(Role::Overlay)
            && send(overlay, &bytes)
        {
            handed.handed(bytes);
        }
    }

    /// Whether the overlay asked this window to end for good (so that closing it is not hiding it).
    pub fn ended(&self) -> bool {
        self.ended.load(Ordering::Relaxed)
    }

    /// From now on the window ignores the request of the overlay to close (or takes it again).
    pub fn keep_open(&self, on: bool) {
        self.keep_open.store(on, Ordering::Relaxed);
    }
}

/// How a window opens at a remembered place, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Opening {
    pub size: [f32; 2],
    pub position: Option<[f32; 2]>,
    pub maximized: bool,
}

/// Size and place of a window opening at `place`: screen pixels divided by
/// the system `scale`, never below `smallest` (points); the place only when
/// `on_screen` finds its title bar on a screen, otherwise it opens centered.
pub fn opening(
    place: &WindowPlace,
    scale: f32,
    smallest: (f32, f32),
    on_screen: impl Fn(i32, i32, i32) -> bool,
) -> Opening {
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let size = [(place.size.0 as f32 / scale).max(smallest.0), (place.size.1 as f32 / scale).max(smallest.1)];
    let position = place
        .position
        .filter(|&(x, y)| on_screen(x as i32, y as i32, place.size.0 as i32))
        .map(|(x, y)| [x as f32 / scale, y as f32 / scale]);
    Opening { size, position, maximized: place.maximized }
}

/// The opening limited to `work`, the work area of its screen in pixels (`x`, `y`, width, height): a window
/// remembered larger than the screen (another scaling, another interface size) shrinks to it, and its place moves
/// so that the whole window, the title bar first, lies on it. Unknown work area: unchanged.
pub fn fit_to_work_area(mut open: Opening, work: Option<(i32, i32, i32, i32)>, scale: f32) -> Opening {
    let Some((x, y, w, h)) = work else { return open };
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let (left, top, width, height) = (x as f32 / scale, y as f32 / scale, w as f32 / scale, h as f32 / scale);
    if width <= 0.0 || height <= 0.0 {
        return open;
    }
    open.size = [open.size[0].min(width), open.size[1].min(height)];
    if let Some([px, py]) = open.position {
        let px = px.min(left + width - open.size[0]).max(left);
        let py = py.min(top + height - open.size[1]).max(top);
        open.position = Some([px, py]);
    }
    open
}

/// Where the window has to go so that its whole top edge lies in `work`, the work area of its screen, or `None` where
/// it does. `outer` and `work` are (x, y, width, height) in screen pixels; the window is moved, not changed in size.
/// The top edge is the title bar: above the work area it can not be reached, and below or beside it there is nothing to
/// grab.
pub fn top_correction(outer: (i32, i32, i32, i32), work: (i32, i32, i32, i32)) -> Option<(i32, i32)> {
    let ((x, y, w, h), (wx, wy, ww, wh)) = (outer, work);
    if ww <= 0 || wh <= 0 {
        return None;
    }
    // A bar of 40 pixels at least is kept on the screen sideways.
    let grab = w.min(40);
    let new_x = x.max(wx + grab - w).min(wx + ww - grab);
    let new_y = y.max(wy).min((wy + wh - 40).max(wy));
    let _ = h;
    ((new_x, new_y) != (x, y)).then_some((new_x, new_y))
}

/// Where the rectangles of the three buttons of the window frame (minimize, maximize, close, from left to right) of
/// the last frame are kept, for the checks.
pub const FRAME_BUTTONS_ID: &str = "fensterknoepfe";
/// Where the state of the maximize button for the snap layouts of Windows 11 is kept in the context.
pub const SNAP_BUTTON_ID: &str = "fenster-schnappen";

/// The free part of the head bar and the three buttons of the window frame at its right end: minimize, maximize or
/// restore, close. A drag on the free part moves the window (so the system's snapping and the keys with the Windows key
/// work), a double click maximizes or restores it. Closing asks as the frame of the system does. Gives the left edge
/// of the buttons.
fn frame_buttons(ui: &mut egui::Ui, scheme: &Scheme, bar: egui::Rect) -> f32 {
    use versekit_core::language::t;
    let maximized = ui.input(|i| i.viewport().maximized == Some(true));
    let buttons = egui::Rect::from_min_max(
        egui::pos2(bar.right() - 3.0 * widgets::CAPTION_WIDTH, bar.top()),
        bar.right_bottom(),
    );
    // Made first, so that what stands on the bar takes its own clicks.
    let grab = egui::Rect::from_min_max(bar.left_top(), egui::pos2(buttons.left(), bar.bottom()));
    let drag = ui.interact(grab, ui.scope_id().with("kopfleiste-ziehen"), egui::Sense::click_and_drag());
    if drag.drag_started_by(egui::PointerButton::Primary) {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    if drag.double_clicked() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
    let mut row =
        ui.new_child(egui::UiBuilder::new().max_rect(buttons).layout(egui::Layout::right_to_left(egui::Align::Center)));
    row.spacing_mut().item_spacing.x = 0.0;
    // Windows 11: the system takes the maximize button for its own (snap layouts) and says so through the state.
    let snap = ui.ctx().data(|d| d.get_temp::<std::sync::Arc<versekit_core::snap_layouts::MaxButton>>(egui::Id::unique(SNAP_BUTTON_ID)));
    let close = widgets::caption_button(&mut row, scheme, "x", &t("s_fk_schliessen"), true, false);
    if close.clicked() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
    }
    let (icon, word) = if maximized { ("copy", "s_fk_wieder") } else { ("square", "s_fk_gross") };
    let bigger = widgets::caption_button(&mut row, scheme, icon, &t(word), false, snap.as_ref().is_some_and(|s| s.hot()));
    let toggled = snap.as_ref().is_some_and(|s| s.take_toggle());
    if bigger.clicked() || toggled {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
    if let Some(snap) = &snap {
        // The system asks where the button is in pixels of the client area.
        let scale = ui.ctx().pixels_per_point();
        let px = |value: f32| (value * scale).round() as i32;
        snap.set_rect(px(bigger.rect.left()), px(bigger.rect.top()), px(bigger.rect.right()), px(bigger.rect.bottom()));
    }
    let smaller = widgets::caption_button(&mut row, scheme, "minus", &t("s_fk_klein"), false, false);
    if smaller.clicked() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Minimized(true));
    }
    let rects = vec![smaller.rect, bigger.rect, close.rect];
    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique(FRAME_BUTTONS_ID), rects));
    buttons.left()
}

/// How wide the edge of the window is where it can be dragged to resize it, and how far along the edge a corner
/// reaches (points).
const EDGE: f32 = 4.0;
const CORNER: f32 = 10.0;

/// The edge or corner of `window` at `pos`, if `pos` lies within [`EDGE`] of it.
pub fn edge_at(pos: egui::Pos2, window: egui::Rect) -> Option<egui::viewport::ResizeDirection> {
    use egui::viewport::ResizeDirection as D;
    let (left, right) = (pos.x - window.left(), window.right() - pos.x);
    let (top, bottom) = (pos.y - window.top(), window.bottom() - pos.y);
    if left < 0.0 || right < 0.0 || top < 0.0 || bottom < 0.0 {
        return None;
    }
    let corner = |side: f32, other: f32| (side < EDGE && other < CORNER) || (other < EDGE && side < CORNER);
    Some(if corner(top, left) {
        D::NorthWest
    } else if corner(top, right) {
        D::NorthEast
    } else if corner(bottom, left) {
        D::SouthWest
    } else if corner(bottom, right) {
        D::SouthEast
    } else if top < EDGE {
        D::North
    } else if bottom < EDGE {
        D::South
    } else if left < EDGE {
        D::West
    } else if right < EDGE {
        D::East
    } else {
        return None;
    })
}

/// The edges of a window without a frame of the system: the mouse pointer shows the way to resize and a press
/// there lets the system resize. Not while the window is maximized or fills the screen.
fn resize_edges(ctx: &egui::Context) {
    use egui::viewport::ResizeDirection as D;
    let (pos, pressed, fixed) = ctx.input(|i| {
        let v = i.viewport();
        (i.pointer.latest_pos(), i.pointer.primary_pressed(), v.maximized == Some(true) || v.fullscreen == Some(true))
    });
    let Some(direction) = pos.filter(|_| !fixed).and_then(|pos| edge_at(pos, ctx.content_rect())) else { return };
    ctx.set_cursor_icon(match direction {
        D::North => egui::CursorIcon::ResizeNorth,
        D::South => egui::CursorIcon::ResizeSouth,
        D::East => egui::CursorIcon::ResizeEast,
        D::West => egui::CursorIcon::ResizeWest,
        D::NorthEast => egui::CursorIcon::ResizeNorthEast,
        D::NorthWest => egui::CursorIcon::ResizeNorthWest,
        D::SouthEast => egui::CursorIcon::ResizeSouthEast,
        D::SouthWest => egui::CursorIcon::ResizeSouthWest,
    });
    if pressed {
        ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
    }
}

/// Brings a window whose top edge lies off its screen back onto it (a position that was stored wrongly, a screen that
/// went away, a window larger than the screen). Not while it is maximized, full screen or minimized, and not while a
/// mouse button is held (the player may be moving it). Asks at most twice a second.
fn keep_on_screen(ctx: &egui::Context) {
    let (outer, off, scale, now, held) = ctx.input(|i| {
        let v = i.viewport();
        let off = v.maximized == Some(true) || v.fullscreen == Some(true) || v.minimized == Some(true);
        (v.outer_rect, off, i.pixels_per_point, i.time, i.pointer.any_down())
    });
    let Some(outer) = outer else { return };
    if off || held || scale <= 0.0 {
        return;
    }
    let id = egui::Id::unique("fenster-auf-schirm");
    if ctx.data(|d| d.get_temp::<f64>(id)).is_some_and(|at| now - at < 0.5) {
        return;
    }
    let px = |v: f32| (v * scale).round() as i32;
    let (x, y, w, h) = (px(outer.min.x), px(outer.min.y), px(outer.width()), px(outer.height()));
    let Some(work) = versekit_core::screen::monitor_area(x + w / 2, y + 20.min(h), true) else { return };
    if let Some((new_x, new_y)) = top_correction((x, y, w, h), work) {
        ctx.data_mut(|d| d.insert_temp(id, now));
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
            new_x as f32 / scale,
            new_y as f32 / scale,
        )));
    }
}

/// The settings written in a thread of its own: on a network share a write
/// can take long, and the window stood still meanwhile. The overlay is told
/// once a batch is written.
struct SettingsWriter {
    jobs: std::sync::mpsc::Sender<Vec<(&'static str, Value)>>,
    /// Batches sent and batches written.
    sent: u64,
    done: std::sync::Arc<std::sync::atomic::AtomicU64>,
    failed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Under test: how long each write of this thread takes in addition. Per writer, not for the whole program:
    /// the tests run side by side, and a slow writer of one must not slow down the others.
    #[cfg(test)]
    delay_ms: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl SettingsWriter {
    fn start(paths: Paths) -> SettingsWriter {
        use std::sync::atomic::Ordering;
        let (jobs, batches) = std::sync::mpsc::channel::<Vec<(&'static str, Value)>>();
        let done = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let failed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (done_here, failed_here) = (done.clone(), failed.clone());
        #[cfg(test)]
        let delay_ms = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        #[cfg(test)]
        let delay_here = delay_ms.clone();
        // The thread writes under the same lock as the window.
        let guard = versekit_core::writeguard::state();
        std::thread::Builder::new()
            .name("einstellungen".into())
            .spawn(move || {
                versekit_core::writeguard::adopt(&guard);
                while let Ok(batch) = batches.recv() {
                    #[cfg(test)]
                    std::thread::sleep(std::time::Duration::from_millis(delay_here.load(Ordering::Relaxed)));
                    if batch.into_iter().any(|(key, value)| !paths.set_setting(key, value)) {
                        failed_here.store(true, Ordering::Relaxed);
                    }
                    done_here.fetch_add(1, Ordering::Relaxed);
                    // The overlay takes the change over at once, not at its next look.
                    window_signal::send_to(Role::Overlay, Signal::Settings);
                }
            })
            .expect("settings thread");
        SettingsWriter {
            jobs,
            sent: 0,
            done,
            failed,
            #[cfg(test)]
            delay_ms,
        }
    }

    fn pending(&self) -> bool {
        self.done.load(std::sync::atomic::Ordering::Relaxed) < self.sent
    }
}

/// The settings file behind the window model.
struct SettingsStore {
    paths: Paths,
    /// Writes in a thread of its own; `None` writes here and now.
    writer: Option<SettingsWriter>,
    /// The values as last written (or read); `None` until the first frame.
    saved: Option<Vec<(&'static str, Value)>>,
    /// A write failed.
    failed: bool,
}

impl Default for SamplePage {
    fn default() -> Self {
        Self::new()
    }
}

impl SamplePage {
    /// The hangar page; once the recipes are read, every ship has its box "Loadout & cart".
    fn show_hangar(&mut self, ui: &mut egui::Ui, scheme: &Scheme) {
        if self.hangar.needs_extra()
            && let Some((values, book, _)) = self.crafting.parts()
            && !self.blueprints.catalog.blueprints.is_empty()
        {
            let recipes =
                book.items
                    .iter()
                    .filter_map(|item| {
                        let recipe = book.recipe_of(values, item)?;
                        Some(equipment_box::RecipeInfo {
                            entity: item.entity.clone(),
                            basis: item.basis.clone(),
                            blueprint: recipe.guid.clone(),
                            slots: recipe.tiers.first().map_or_else(Vec::new, |tier| {
                                tier.slots.iter().map(|slot| slot.name.clone()).collect()
                            }),
                        })
                    })
                    .collect();
            let craft = book
                .items
                .iter()
                .map(|item| versekit_core::part_choices::CraftEntry {
                    entity: item.entity.clone(),
                    basis: item.basis.clone(),
                    name: item.name.clone(),
                    maker: item.manufacturer.clone(),
                    recipe_kind: item.kind.clone(),
                    recipe_subtype: item.subtype.clone(),
                })
                .collect();
            let traits = self
                .blueprints
                .catalog
                .blueprints
                .iter()
                .map(|b| {
                    (
                        b.key.clone(),
                        versekit_core::part_choices::Traits {
                            kind: b.kind.clone(),
                            size: b.size,
                            maker: b.maker.clone().unwrap_or_default(),
                            grade: b.grade,
                            class: b.class.clone().unwrap_or_default(),
                        },
                    )
                })
                .collect();
            let ids = versekit_core::equipment_share::PartIds::new(
                values.items.values().map(|item| (item.guid.clone(), item.class_name.clone())),
            );
            self.hangar.set_extra(equipment_box::Extra { craft, traits, recipes, ids });
        }
        match self.crafting.parts() {
            Some((values, book, _)) => {
                let stores = self.hangar.stores();
                let slots = |ship: &versekit_core::jsonio::Map, chosen: &versekit_core::jsonio::Map| {
                    stores.erkul.cart_slots(ship, chosen)
                };
                let data = versekit_gamedata::cart_data::CartData {
                    values,
                    book,
                    shops: &stores.shops,
                    raw: &stores.raw,
                    market: &stores.market,
                    slots: Some(&slots),
                };
                hangar_page::show_page_with(ui, scheme, &mut self.hangar, Some(&data));
            }
            None => hangar_page::show_page(ui, scheme, &mut self.hangar),
        }
    }

    pub fn new() -> Self {
        // Made-up play times: evenings and weekends, some empty hours.
        let heat = (0..7)
            .map(|day| {
                (0..24)
                    .map(|hour| {
                        let evening = if (18..24).contains(&hour) { 3.0 } else { 0.0 };
                        let weekend = if day >= 5 && (12..24).contains(&hour) { 2.0 } else { 0.0 };
                        let noise = ((day * 7 + hour * 3) % 5) as f64 * 0.3;
                        let total = evening + weekend + noise;
                        if total < 0.5 { 0.0 } else { total * 600.0 }
                    })
                    .collect()
            })
            .collect();
        SamplePage {
            applied: None,
            preview: false,
            frame_height: WINDOW_MODEL_HEIGHT,
            search: String::new(),
            picked: None,
            choice: String::new(),
            dialog_open: false,
            text: String::new(),
            notes: String::new(),
            toggle: true,
            value: 40,
            system: 0,
            sort: 0,
            switches: [true, false],
            tab: 0,
            row: 1,
            quality: "1200".to_string(),
            table_sort: (0, true),
            notice_at: None,
            pin_notice: None,
            autostart: Box::new(autostart::Memory::default()),
            autostart_command: String::new(),
            autostart_on: false,
            desktop: desktop_rows::DesktopRows::default(),
            pin_fault: None,
            heat,
            size: 1,
            size_applied: None,
            entries: ENTRIES.to_vec(),
            undo: None,
            row_note: None,
            failed_at: None,
            page: String::new(),
            nav_filter: String::new(),
            nav_enter: false,
            history: widgets::History::new(""),
            tile_overflow: 0.0,
            frame_overflow: 0.0,
            frame_extra: 2.0 * widgets::BAR_HEIGHT + 16.0,
            frame_overflow_y: 0.0,
            gpu: true,
            gpu_started: true,
            clock_24: true,
            restart_later: false,
            mangohud: false,
            streamer: false,
            usage_on: true,
            shader:shader_page::ShaderState::default(),
            grafik: grafik_page::GrafikState::default(),
            fov: blickwinkel_page::FovState::default(),
            joysticks: joysticks_page::JoystickState::default(),
            axes: axes_page::AxesState::default(),
            start_programs: startprograms_page::StartProgramsState::default(),
            modules: modules_page::ModulesState::default(),
            detection: detection_page::DetectionState::default(),
            module_hidden: Vec::new(),
            channel_question: channel_question::ChannelQuestion::default(),
            start_command: String::new(),
            start_look: game_start::Look::default(),
            spawner: std::sync::Arc::new(game_start::System),
            start_done: std::sync::mpsc::channel(),
            copied_at: None,
            blueprints: blueprint_list::ListData::sample(),
            blueprints_none: false,
            blueprint_view: blueprint_list::ListState::default(),
            progress_view: blueprint_progress::ProgressState::default(),
            hangar: hangar_page::HangarState::default(),
            game_texts: game_texts_page::GameTextsState::default(),
            stock: stock_page::StockState::default(),
            import: import_page::ImportState::default(),
            crafting: crafting_page::CraftingState::default(),
            farm: farm_page::FarmState::default(),
            mining: mining_page::MiningState::default(),
            salvage: salvage_page::SalvageState::default(),
            salvage_counted: None,
            shopping: shopping_page::ShoppingState::default(),
            memo_page: notes_page::NotesState::default(),
            close_writes: crate::close_writes::SignedUp::default(),
            contested: contested_page::ContestedState::sample(versekit_core::contested::now(), 2_500),
            server_status: crate::serverstatus_page::ServerStatusState::sample(versekit_core::contested::now()),
            patch_changes: crate::patch_changes_page::PatchChangesState::sample(),
            handles: crate::handle_page::HandlePageState::sample(versekit_core::contested::now()),
            contested_notice: None,
            shown_page: String::new(),
            usage: None,
            usage_route: None,
            search_counted: (false, false),
            usage_started: false,
            selling: selling_page::SellingState::default(),
            routes: routes_page::RoutesState::default(),
            shops: shops_page::ShopsState::default(),
            org_stock: org_stock_page::OrgStockState::new(Some(org_stock_page::Link::demo()), None),
            demand: demand_page::DemandState::default(),
            connection: basetool_page::ConnectionState::new(None),
            head_sync: crate::head_sync::HeadSync::default(),
            contracts: contract_log::LogData::sample(),
            contract_view: contract_log::LogState::default(),
            stats: statistics_page::StatsState::sample(),
            error_count: 0,
            diagnose: None,
            thanks: thanks_page::ThanksState::new(env!("CARGO_PKG_VERSION")),
            nav_folded: false,
            folders: FOLDERS.map(str::to_string),
            game_found: None,
            game_searched: Vec::new(),
            data_folder: data_folder::DataFolderState::new(),
            channel_states: channel_states::ChannelStates::default(),
            view_range: 60,
            view_pending: false,
            restore_open: false,
            backup_first: true,
            hotkey_modifiers: parse_hotkey(versekit_core::overlay::DEFAULT_HOTKEY).map_or([true, true, false], |h| h.0),
            hotkey_key: parse_hotkey(versekit_core::overlay::DEFAULT_HOTKEY).map_or(1, |h| h.1),
            scan_modifiers: parse_hotkey(versekit_core::overlay::DEFAULT_SCAN_HOTKEY).map_or([true, true, false], |h| h.0),
            scan_key: parse_hotkey(versekit_core::overlay::DEFAULT_SCAN_HOTKEY).map_or(0, |h| h.1),
            scan_hotkey_on: true,
            hotkey_on: true,
            search_modifiers: parse_hotkey(versekit_core::overlay::DEFAULT_SEARCH_HOTKEY)
                .map_or([false, true, true], |h| h.0),
            search_key: parse_hotkey(versekit_core::overlay::DEFAULT_SEARCH_HOTKEY).map_or(7, |h| h.1),
            search_hotkey_on: true,
            picture_modifiers: parse_hotkey(versekit_core::overlay::DEFAULT_PICTURE_HOTKEY)
                .map_or([false, true, true], |h| h.0),
            picture_key: parse_hotkey(versekit_core::overlay::DEFAULT_PICTURE_HOTKEY).map_or(1, |h| h.1),
            picture_hotkey_on: true,
            main_modifiers: parse_hotkey(versekit_core::overlay::DEFAULT_MAIN_HOTKEY)
                .map_or([false, true, true], |h| h.0),
            main_key: parse_hotkey(versekit_core::overlay::DEFAULT_MAIN_HOTKEY).map_or(5, |h| h.1),
            main_hotkey_on: true,
            overlay_runs: || versekit_core::window_signal::find(versekit_core::window_signal::Role::Overlay).is_some(),
            test_build: false,
            news_index: 0,
            nav_hit: String::new(),
            overview_two: false,
            group_order: (0..NAV.len()).collect(),
            dragging_group: None,
            mine_first: false,
            update_loaded: false,
            update_version: SAMPLE_UPDATE.to_string(),
            game_seen: None,
            game_process: game_process_of,
            overview: None,
            loading: None,
            loading_update: false,
            loading_shown: false,
            update_test: false,
            updates: update_page::UpdateUi::offline(env!("CARGO_PKG_VERSION")),
            news: crate::news_page::NewsState::default(),
            update_quit: false,
            highlight: None,
            overlay_clickable: false,
            general_switches: [true, true, true, true],
            general_english: false,
            language_auto: false,
            play_head: true,
            check_interval: 3,
            came_from: None,
            collection_tools: Default::default(),
            rescan_notice: Default::default(),
            groups_closed: [false; GROUP_SETTINGS.len()],
            groups_language: None,
            setup: None,
            backup: backup_flow::BackupFlow::default(),
            first_backup_hint: None,
            first_backup_told: false,
            crash_hint: None,
            crash_told: false,
            #[cfg(test)]
            folders_opened: Vec::new(),
            overlay_place: settings::OverlayPlace::TopRight,
            bar_side: settings::BarSide::Top,
            wizard: settings::WizardState::new(4),
            wizard_english: false,
            wizard_overlay: true,
            tiles: [
                ("hf_raffinerien", tiles::TileSize::Wide),
                ("hf_meinlager", tiles::TileSize::Small),
                ("hf_liste", tiles::TileSize::Small),
                ("spielzeit", tiles::TileSize::Large),
                ("hf_hangar", tiles::TileSize::Tall),
                ("hf_serverstatus", tiles::TileSize::Small),
                ("hf_routen", tiles::TileSize::Small),
            ]
            .iter()
            .map(|(key, size)| tiles::Tile::new(key, *size))
            .collect(),
            start: None,
            place_look: glance_tiles::PlaceLook::default(),
            machine_look: glance_tiles::MachineLook::default(),
            status_look: glance_tiles::StatusLook::default(),
            new_pages: controls::sample_new_pages(),
            gallery: None,
            overlay_rows: versekit_core::overlay::ROWS_DEFAULT,
            overlay_auto: true,
            overlay_opacity: 93,
            overlay_opaque: false,
            overlay_size: versekit_core::overlay::SIZE_STEPS[0],
            overlay_popup: false,
            overlay_seconds: 6,
            overlay_events: versekit_core::overlay::Events::default(),
            overlay_sections: versekit_core::overlay::Sections::default(),
            overlay_contracts_max: versekit_core::overlay::CONTRACTS_MAX_DEFAULT,
            overlay_fps: false,
            overlay_server_fps: false,
            overlay_players: false,
            overlay_info_bar: versekit_core::overlay::INFO_BAR_DEFAULT,
            overlay_info_bar_always: versekit_core::overlay::INFO_BAR_ALWAYS_DEFAULT,
            overlay_always: versekit_core::overlay::OVERLAY_ALWAYS_DEFAULT,
            overlay_info_bar_corner: versekit_core::info_bar::Corner::default(),
            overlay_info_bar_size: versekit_core::overlay::SIZE_STEPS[0],
            place_reset_at: None,
            place_reset_waits: false,
            sounds: [true, true, false],
            volume: 70,
            saved_at: None,
            selection: widgets::Selection::new(ORES.len()),
            revealed: false,
            store: None,
            reload_waiting: false,
            window: None,
            window_seen: None,
            extension: None,
            extension_page: String::new(),
            storage_given: false,
            open_url: versekit_core::browser::open_in_browser,
            choose_folder: crate::file_dialog::choose_folder,
            restart_with: controls::restart_process,
            folder_dialog: None,
            live_folder_saved: FOLDERS[0].to_string(),
            channels_saved: Vec::new(),
            channels_found: FOLDERS.map(String::from),
            errors_looked: None,
            real_basetool: None,
            folder_refused: None,
            folder_search: None,
            own_frame: false,
        }
    }

    /// The window has no frame of the system: the head bar takes its place (title bar with the buttons to minimize,
    /// maximize and close, a place to drag the window by) and the edges resize it.
    pub fn use_own_frame(&mut self, on: bool) {
        self.own_frame = on;
    }

    /// Only the window model with sample contents, to pass on: people try
    /// the functions without their own data. Nothing is saved.
    pub fn preview() -> Self {
        // The hotkey of the handle search stands switched on, so that its picker is drawn and measured too.
        let mut page = SamplePage { preview: true, search_hotkey_on: true, ..Self::new() };
        page.connect_preview(true);
        page
    }

    /// [`Self::preview`] without the made-up update bar and the made-up loading in the foot bar: for pictures of
    /// the window that stand for the program as it is.
    pub fn preview_still() -> Self {
        let mut page = Self::preview();
        page.update_version = String::new();
        page.loading_shown = true;
        page
    }

    /// Connects the pages that talk to the Basetool to the real one (the sync
    /// of the program, see [`basetool_page::Wire::system`]): nothing goes out
    /// before a switch is on and someone connects. Where the connection cannot
    /// be made the pages say so; no made-up Basetool stands in.
    fn connect_system(&mut self, paths: &Paths) {
        self.connect_system_as(paths, None);
    }

    /// [`Self::connect_system`] with credentials of their own under `keyname`, even on a data
    /// folder given from outside: for a tool that logs in to the real Basetool from a copy of
    /// the data, the copy never holds a credential and the login of the user's installation
    /// stays untouched.
    fn connect_system_as(&mut self, paths: &Paths, keyname: Option<&str>) {
        use versekit_core::legacy::{ENV_BASETOOL_KEYNAME, ENV_SECRETS, ENV_SECRETS_FILE};
        // The old connection's worker ends; a new one takes over (a worker left running would
        // go on writing into the same files).
        if let Some(old) = self.connection.wire() {
            old.sync.stop();
        }
        let mut sync_paths = paths.working_copy();
        if let Some(name) = keyname {
            // A folder of its own next to the one of the user's installation, worked out from the
            // real environment: the host of a copy of the data points its configuration folder into
            // the copy. Windows keeps the key under the name given (and the file in that folder);
            // elsewhere the credentials go into a file in that folder, never into the Secret Service,
            // whose entries have one name for everybody and would replace the user's own login.
            let slug: String = name.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
            let real = versekit_core::basetool::secrets::folder(&versekit_core::paths::Host::current());
            sync_paths.host.set_var(ENV_BASETOOL_KEYNAME, name);
            sync_paths.host.set_var(ENV_SECRETS, &format!("{real}-{slug}"));
            sync_paths.host.set_var(ENV_SECRETS_FILE, "1");
        }
        // A data folder fixed from outside (a copy to try things on): the credentials stay inside it,
        // never those of the real installation.
        if let Some(fixed) = paths.given().data
            && keyname.is_none()
            && sync_paths.host.var_set(ENV_SECRETS).is_none()
        {
            let folder = sync_paths.host.flavor.join(&fixed.folder, "basetool-geheim");
            sync_paths.host.set_var(ENV_SECRETS, &folder);
            sync_paths.host.set_var(ENV_SECRETS_FILE, "1");
        }
        let sync_paths = std::sync::Arc::new(sync_paths);
        let own: versekit_core::basetool::sync::OwnAccount = {
            let paths = sync_paths.clone();
            std::sync::Arc::new(move || {
                use versekit_core::logsource::LogSource;
                LogSource::new(&paths, LogSource::sources_of(&paths)).own_account(None)
            })
        };
        let wire = basetool_page::Wire::system(sync_paths, own, env!("CARGO_PKG_VERSION"));
        let link = wire.as_ref().map(org_stock_page::Link::from_wire);
        self.org_stock = org_stock_page::OrgStockState::new(link, Some(paths));
        self.stock.connect(wire.as_ref().map(stock_share::Link::from_wire));
        self.hangar.connect(wire.as_ref().map(stock_share::Link::from_wire));
        self.demand = demand_page::DemandState::new(wire.as_ref().map(demand_page::Link::from_wire), Some(paths));
        self.connection = basetool_page::ConnectionState::new(wire);
    }

    /// Wires the connection page and the unit storage to a made-up Basetool
    /// with settings in a throwaway folder. `worker` starts the sync in the
    /// background.
    pub fn connect_preview(&mut self, worker: bool) {
        self.connect_wire(basetool_page::Wire::preview(worker));
    }

    /// Wires the connection page and the unit storage to `wire`.
    pub fn connect_wire(&mut self, wire: basetool_page::Wire) {
        self.org_stock = org_stock_page::OrgStockState::new(Some(org_stock_page::Link::from_wire(&wire)), None);
        self.stock.connect(Some(stock_share::Link::from_wire(&wire)));
        self.hangar.connect(Some(stock_share::Link::from_wire(&wire)));
        self.demand = demand_page::DemandState::new(Some(demand_page::Link::from_wire(&wire)), None);
        self.connection = basetool_page::ConnectionState::new(Some(wire));
    }

    /// The pages that are only there under a condition: the unit storage with
    /// its switch on, its right granted and the connection standing. Without
    /// a connection of the preview (the sample page) every page is there.
    fn page_hidden(&self, page: &str) -> bool {
        use versekit_core::basetool::scopes::{SETTING_DEMAND, SETTING_ORGSTOCK};
        // A module that is switched off hides its pages (the page "Module" itself is never one of them).
        if self.module_hidden.contains(&page) {
            return true;
        }
        // A connection an embedding program gave the storage shows the pages of the Basetool as they are.
        if self.storage_given && matches!(page, "hf_staffellager" | "hf_bedarf") {
            return false;
        }
        match (page, self.connection.wire()) {
            ("hf_staffellager", Some(wire)) => !wire.sync.area_visible(SETTING_ORGSTOCK),
            // Real data without a connection to the Basetool: no storage to show.
            ("hf_staffellager", None) => self.real(),
            // The unit demand is there with the connection standing, its switch on and its right granted.
            ("hf_bedarf", Some(wire)) => !wire.sync.area_visible(SETTING_DEMAND),
            ("hf_bedarf", None) => self.real(),
            _ => false,
        }
    }

    /// Switches to the overview, as the overview entry of the menu does.
    pub fn open_overview(&mut self) {
        self.page.clear();
    }

    /// The gallery of what is new while it is open: closed, it counts as shown for this version; "Ansehen" opens
    /// the page of its highlight (the card view of 4.12 has no switch in v5, so it leads to the overview).
    fn show_gallery(&mut self, ctx: &egui::Context, scheme: &Scheme) {
        use versekit_core::gallery::Target;
        let Some(gallery) = &mut self.gallery else { return };
        let outcome = gallery.show(ctx, scheme);
        if !outcome.closed {
            return;
        }
        self.gallery = None;
        if let Some(store) = &self.store {
            versekit_core::gallery::mark_shown(&store.paths, env!("CARGO_PKG_VERSION"));
        }
        match outcome.open {
            Some(Target::Page(key)) => match group_of(key) {
                Some(group) => self.open_page(ctx, key, group),
                None => self.page = key.to_string(),
            },
            Some(Target::Overview | Target::CardView) => self.page.clear(),
            None => {}
        }
    }

    /// Sets the blueprint list to the blueprint called `name`: it stands in the search and its row is open.
    fn open_blueprint(&mut self, name: &str) {
        self.blueprint_view.reset();
        self.blueprint_view.search = name.to_string();
        self.blueprint_view.open =
            self.blueprints.catalog.blueprints.iter().position(|b| b.key == versekit_core::paths::name_key(name));
    }

    /// Switches to the page `key` of the navigation group `group`, as a jump from another page does.
    pub fn open_page(&mut self, ctx: &egui::Context, key: &str, group: &str) {
        self.page = key.to_string();
        widgets::set_nav_section_open(ctx, &t(group), false);
    }

    /// Switches to the page of a page name of 4.12 (`main_window.open_page`): an old name leads to its new page
    /// with the choice it brings (`overview::resolve`). `false` when the name is no page.
    pub fn open_page_named(&mut self, ctx: &egui::Context, name: &str) -> bool {
        let (page, preset) = versekit_core::overview::resolve(name);
        let Some(key) = versekit_core::overview::page_key(page) else { return false };
        if let Some(preset) = preset {
            self.stock.apply_preset(preset);
        }
        if name == "beziehungen" {
            self.handles.tab = crate::handle_page::Tab::Relations;
        }
        match group_of(key) {
            Some(group) => self.open_page(ctx, key, group),
            None => self.page = key.to_string(),
        }
        true
    }

    /// Takes over the settings file again after another window changed it
    /// (the overlay moved by hand is free now). Nothing goes back: the next
    /// frame takes the values as written once they are applied, a text size
    /// this screen caps included.
    pub fn reload_settings(&mut self) {
        // Own changes still on their way would be read back as they were.
        if self.writes_pending() {
            self.reload_waiting = true;
            return;
        }
        self.reload_waiting = false;
        let Some(mut store) = self.store.take() else { return };
        self.load_settings(&store.paths);
        self.start_programs.invalidate();
        self.detection.invalidate();
        self.grafik.invalidate();
        self.fov.invalidate();
        self.joysticks.invalidate();
        self.axes.invalidate();
        self.size_applied = None;
        store.saved = None;
        self.store = Some(store);
    }

    /// Draws the page; returns how far anything sticks out to the right
    /// (0 = nothing cut off).
    pub fn show(&mut self, ui: &mut egui::Ui) -> f32 {
        if self.own_frame {
            resize_edges(ui.ctx());
        }
        // The window closes: the page open last counts (`page_usage.window_closed`), and a size or place that has
        // not settled yet counts too.
        let closing = ui.input(|i| i.viewport().close_requested());
        if closing {
            self.count_window_end(None);
            if let Some((place, _)) = self.window_seen.take() {
                self.window = Some(place);
            }
            // What the pages hold back for a moment is written now (4.12 `before_close`), before any page draws.
            self.write_held_back();
        }
        self.extension_frame(ui, true);
        // How a start of the game went: the reason it did not work stands in the foot like in 4.12.
        while let Ok(result) = self.start_done.1.try_recv() {
            if let Err(reason) = result {
                let said = tf("s_sp_start_nein", &[("0", Arg::from(reason.as_str()))]);
                self.pin_notice = Some((ui.input(|i| i.time), said));
            }
        }
        if self.reload_waiting {
            self.reload_settings();
            if self.reload_waiting {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
            }
        }
        let scheme = theme::current();
        if self.applied != Some(scheme.id) {
            apply(ui.ctx(), scheme);
            self.applied = Some(scheme.id);
            ui.ctx().request_repaint();
        }
        if self.size_applied != Some(self.size) {
            self.size = crate::apply_size(ui.ctx(), self.size);
            self.size_applied = Some(self.size);
        }
        crate::fit_window(ui.ctx());
        // The steps of the pages of the game's texts; the overlay keeps the texts current.
        if self.real() {
            self.game_texts.poll();
            if self.game_texts.busy() {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
            }
        }
        // A catalog built in the background replaces the one shown, before anything is drawn.
        if self.blueprints.poll(ui.ctx(), &mut self.blueprint_view) {
            self.contract_view.forget_jump();
        }
        // The game values are read in the background whatever page is open: the farm list and the
        // button of the blueprint list need them without a visit to the crafting page.
        self.crafting.tick(ui.ctx());
        // The self timers run whatever page is open, and tell when one has run out.
        self.contested.tick(ui.ctx());
        if let Some(name) = self.contested.take_ran_out().pop() {
            let said = tf("s_cz_timer_aus", &[("0", Arg::from(name))]);
            self.contested_notice = Some((ui.input(|i| i.time), said));
        }
        // The sync wakes this window whatever page is open.
        if let Some(wire) = self.connection.wire() {
            wire.remember(ui.ctx());
        }
        self.update_frame(ui.ctx(), scheme);
        // The measuring of the screen is the whole window (it goes full screen): nothing else is drawn.
        if self.fov.measuring() {
            match &self.store {
                Some(store) => blickwinkel_page::show_measure(ui, scheme, &mut self.fov, &store.paths),
                // Nothing to measure with and nothing to draw it with: the window must not stay full screen.
                None => self.fov.abort_measuring(ui.ctx()),
            }
            if let Some(said) = self.fov.take_notice() {
                self.pin_fault = said.fault.then(|| said.text.clone());
                self.pin_notice = Some((ui.input(|i| i.time), said.text));
            }
            return 0.0;
        }
        let mut overflow = 0.0;
        // The preview is the window itself: no margin around it.
        let margin = if self.preview { 0.0 } else { 16.0 };
        egui::Frame::NONE.fill(color(scheme.bg)).inner_margin(margin).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            if self.preview {
                // The preview is exactly as tall as the window: the sidebar and
                // the page scroll inside, the window itself never. What the frame needs besides
                // sidebar and page (bars, borders) is measured on the frame before.
                let room = ui.available_height();
                self.frame_height = (room - self.frame_extra).max(0.0);
                let top = ui.cursor().min.y;
                self.content(ui, scheme);
                let drawn = ui.min_rect().bottom() - top;
                self.frame_extra = (drawn - self.frame_height).max(0.0);
                self.frame_overflow_y = (drawn - room).max(0.0);
                overflow = (ui.min_rect().width() - ui.max_rect().width()).max(0.0);
            } else {
                self.frame_height = WINDOW_MODEL_HEIGHT;
                overflow = layout::page(ui, scheme, "musterseite", |ui| self.content(ui, scheme)).overflow;
            }
        });
        // A step that cannot be undone: ask, with the switch to back up first.
        if self.restore_open {
            let backup_first = &mut self.backup_first;
            let (answer, close) = widgets::dialog(ui.ctx(), scheme, "sicherung-einspielen", |ui| {
                widgets::heading(ui, scheme, &t("s_mu_rest_titel"));
                ui.label(t("s_mu_rest_text"));
                ui.add_space(4.0);
                widgets::switch_row(ui, scheme, &t("s_mu_rest_vorher"), backup_first);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let cancel = ui.add(widgets::Button::new(&t("abbrechen"), ButtonKind::Normal, scheme)).clicked();
                    let restore =
                        ui.add(widgets::Button::new(&t("s_mu_rest_einspielen"), ButtonKind::Danger, scheme)).clicked();
                    (cancel, restore)
                })
                .inner
            });
            if answer.1 {
                let said = t(if self.backup_first { "s_mu_rest_ok2" } else { "s_mu_rest_ok" });
                self.pin_notice = Some((ui.input(|i| i.time), said));
            }
            if answer.0 || answer.1 || close {
                self.restore_open = false;
            }
        }
        // The folder entered is gone but a channel lies beside it (LIVE became HOTFIX): asked once for each
        // start, a moment after it (4.12: `_kanal_pruefen`). The look runs in a thread of its own.
        let mut chosen_channel = None;
        if let Some(store) = &self.store {
            let now = ui.input(|i| i.time);
            self.channel_question.poll(&store.paths, now);
            if self.channel_question.waiting() {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
            }
            chosen_channel = self.channel_question.show(ui.ctx(), scheme);
        }
        if let Some(folder) = chosen_channel
            && self.store.as_ref().is_some_and(|s| s.paths.set_setting("spiel_ordner", Value::str(folder.as_str())))
        {
            self.reload_settings();
        }
        // The setup assistant above the window, and the questions of the data folder above it.
        self.show_setup(ui.ctx(), scheme);
        self.show_backup(ui.ctx(), scheme);
        self.show_first_backup_hint(ui.ctx(), scheme);
        self.show_crash_hint(ui.ctx(), scheme);
        self.data_folder.dialogs(ui.ctx(), scheme, self.store.as_ref().map(|store| &store.paths));
        if self.dialog_open {
            let (answered, close) = widgets::dialog(ui.ctx(), scheme, "musterdialog", |ui| {
                widgets::heading(ui, scheme, &t("s_mu_dlg_titel"));
                ui.label(t("s_mu_dlg_text"));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let cancel = ui.add(widgets::Button::new(&t("abbrechen"), ButtonKind::Normal, scheme)).clicked();
                    let clear =
                        ui.add(widgets::Button::new(&t("s_mlg_aktion_leeren"), ButtonKind::Danger, scheme)).clicked();
                    cancel || clear
                })
                .inner
            });
            if answered || close {
                self.dialog_open = false;
            }
        }
        if let Some((since, said)) = &self.pin_notice {
            let icon = if self.pin_fault.as_ref() == Some(said) { "triangle-alert" } else { "pin" };
            if !widgets::notice(ui.ctx(), scheme, *since, icon, said, None).shown {
                self.pin_notice = None;
                self.pin_fault = None;
            }
        }
        if let Some((since, said)) = &self.contested_notice
            && !widgets::notice(ui.ctx(), scheme, *since, "timer", said, None).shown
        {
            self.contested_notice = None;
        }
        if let Some(since) = self.copied_at
            && !widgets::notice(ui.ctx(), scheme, since, "check", &t("s_bt_kopiert"), None).shown
        {
            self.copied_at = None;
        }
        if let Some(since) = self.notice_at {
            let text = t("s_mu_raff_fertig");
            if !widgets::notice(ui.ctx(), scheme, since, "circle-check", &text, None).shown {
                self.notice_at = None;
            }
        }
        // Brought back while it waits hidden without the game: the notice says why nothing shows.
        let reset_text = language::t(if self.place_reset_waits { "ov_s_mitte_wartet" } else { "ov_lage_weg_2" });
        if let Some(since) = self.place_reset_at
            && !widgets::notice(ui.ctx(), scheme, since, "check", &reset_text, None).shown
        {
            self.place_reset_at = None;
        }
        if let Some((since, index, name)) = self.undo {
            let text = tf("s_mu_geloescht", &[("name", Arg::from(label(name)))]);
            let undo_word = t("s_mlg_rueckgaengig");
            let shown = widgets::notice(ui.ctx(), scheme, since, "trash-2", &text, Some(undo_word.as_str()));
            if shown.acted {
                self.entries.insert(index.min(self.entries.len()), name);
            }
            if !shown.shown {
                self.undo = None;
            }
        }
        if self.store.is_some() {
            self.follow_window(ui.ctx());
            keep_on_screen(ui.ctx());
        }
        if let Some(store) = &self.store {
            trade::start_background(&store.paths, ui.ctx());
        }
        let now = ui.input(|i| i.time);
        self.settle_folders(now);
        if self.settle_folder_search() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
        self.follow_errors(now);
        // The result of reading all logs again, from the overlay: a notice that stays until it is closed.
        if self.collection_tools.take_asked() {
            self.rescan_notice.expect(now);
        }
        if let Some(store) = &self.store {
            let unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
            if self.rescan_notice.follow(&store.paths, now, unix) {
                let look = std::time::Duration::from_secs_f64(crate::collection_tools::RESCAN_LOOK);
                ui.ctx().request_repaint_after(look);
            }
        }
        self.rescan_notice.show(ui.ctx(), scheme);
        self.show_gallery(ui.ctx(), theme::current());
        // Closing: the settings are on disk before the window goes (what the pages held back is written above).
        if closing {
            self.save_settings_and_wait();
        } else {
            self.save_changes();
        }
        let overflow = overflow.max(self.tile_overflow).max(self.frame_overflow);
        self.extension_frame(ui, false);
        overflow
    }

    /// The window model with the settings of a settings file: read once,
    /// and every change written back at once, in the format of 4.12.
    pub fn with_settings(paths: Paths) -> Self {
        // The tests of this crate never ask the network.
        #[cfg(test)]
        let supply: std::sync::Arc<dyn versekit_gamedata::list::Supply> =
            std::sync::Arc::new(versekit_gamedata::list::Offline);
        #[cfg(not(test))]
        let supply: std::sync::Arc<dyn versekit_gamedata::list::Supply> =
            std::sync::Arc::new(versekit_gamedata::list::Runtime::default());
        Self::with_settings_using(paths, supply)
    }

    /// Like [`SamplePage::with_settings`], with the `supply` the blueprint
    /// catalog takes from the network.
    pub fn with_settings_using(paths: Paths, supply: std::sync::Arc<dyn versekit_gamedata::list::Supply>) -> Self {
        // Not `preview()`: its made-up Basetool must not stand in on real data.
        let mut page = SamplePage { preview: true, ..Self::new() };
        // Real data name no made-up version: the bar waits for one from outside.
        page.update_version = String::new();
        // Nor a made-up load at start: the foot bar shows what really happens.
        page.loading_shown = true;
        page.load_settings(&paths);
        // The gallery of what is new, once after an update: asked before a first start makes the file of the seen
        // areas, which would make every install look like an update.
        if versekit_core::gallery::due(&paths, env!("CARGO_PKG_VERSION")) {
            page.gallery = crate::gallery_page::GalleryState::new(env!("CARGO_PKG_VERSION"), Vec::new());
        }
        page.load_seen(&paths);
        match blueprint_list::ListData::load_with_supply(&paths, supply) {
            Some(blueprints) => page.blueprints = blueprints,
            None => page.blueprints_none = true,
        }
        page.hangar = hangar_page::HangarState::with_file(&paths);
        page.game_texts = game_texts_page::GameTextsState::with_paths(&paths);
        page.stock = stock_page::StockState::with_files(&paths);
        // The tests of this crate never ask the network for game values either.
        #[cfg(test)]
        let values_supply: std::sync::Arc<dyn versekit_gamedata::values_store::Supply> =
            std::sync::Arc::new(versekit_gamedata::values_store::Offline);
        #[cfg(not(test))]
        let values_supply: std::sync::Arc<dyn versekit_gamedata::values_store::Supply> =
            std::sync::Arc::new(versekit_gamedata::values_store::Runtime::for_host(&paths.host));
        page.crafting = crafting_page::CraftingState::with_files(&paths, values_supply.clone());
        page.farm = farm_page::FarmState::with_files(&paths);
        page.mining = mining_page::MiningState::with_files(&paths).with_supply(values_supply);
        page.salvage = salvage_page::SalvageState::with_files(&paths);
        page.shopping = shopping_page::ShoppingState::with_files(&paths);
        page.memo_page = notes_page::NotesState::with_files(&paths);
        page.contested = contested_page::ContestedState::with_files(&paths);
        page.server_status = crate::serverstatus_page::ServerStatusState::with_files(&paths);
        page.patch_changes = crate::patch_changes_page::PatchChangesState::with_files(&paths);
        page.handles = crate::handle_page::HandlePageState::with_files(&paths);
        page.selling = selling_page::SellingState::with_files(&paths);
        page.routes = routes_page::RoutesState::with_files(&paths);
        page.shops = shops_page::ShopsState::with_files(&paths);
        page.connect_system(&paths);
        page.contracts = contract_log::LogData::load(&paths);
        page.stats = statistics_page::StatsState::of(&paths);
        // The report page reads the same data folder; nothing can be sent
        // from the preview unless a test receiver on this computer is asked for.
        let report_paths = std::sync::Arc::new(Paths::new(paths.host.clone()));
        let env = versekit_core::report_facts::Environment {
            version: env!("CARGO_PKG_VERSION").to_string(),
            ..Default::default()
        };
        page.error_count =
            u32::try_from(versekit_core::error_log::ErrorLog::new(&report_paths, &env.version).count()).unwrap_or(0);
        page.diagnose = Some(
            diagnose_page::DiagnoseState::new(
                report_paths,
                env,
                versekit_core::report_send::Target::off(),
                diagnose_page::SendVia::Network,
            )
            .on_copy(versekit_core::testmode::on_copy(&paths)),
        );
        // After the connection and on real data: whether the unit demand and the unit storage are
        // there decides their tile and their pinned card.
        let start_paths = Paths::new(paths.host.clone());
        // A working copy, so the counting knows a copy of the data (the test mode) as the sync does.
        page.usage = Some(std::sync::Arc::new(versekit_core::usage::PageUsage::new(
            paths.working_copy(),
            env!("CARGO_PKG_VERSION"),
        )));
        page.store = Some(SettingsStore { paths, writer: None, saved: None, failed: false });
        page.load_start_page(&start_paths);
        page
    }

    /// Reports once a day in the background that the program runs, with the
    /// page counters (`usage_ping.start`): at once, then hourly, ten minutes
    /// after a failed try. Own data only; off with `SC_BP_NO_NET` and for a
    /// version the receiver does not take.
    pub fn start_usage_ping(&self) {
        let (Some(store), Some(usage)) = (&self.store, &self.usage) else { return };
        if !usage.active() {
            return;
        }
        versekit_core::usage::spawn_ping(store.paths.working_copy(), std::sync::Arc::clone(usage));
    }

    /// The window counts its opening and the page it starts with (`page_usage.window_opened`).
    fn count_window_start(&mut self) {
        if self.usage_started {
            return;
        }
        self.usage_started = true;
        self.salvage_counted = self.salvage_view_open();
        if let Some(usage) = &self.usage {
            usage.window_opened();
            if let Some(id) = self.usage_id() {
                usage.page_opened(id, Some("start"), None);
            }
        }
    }

    /// The page id the usage counters name the open page by. The page "Dismantle & Salvage" is counted by the
    /// view it shows: `bergung` for a wreck, `zerlegen` for the calculation of a part, so the report keeps
    /// telling the two uses apart.
    fn usage_id(&self) -> Option<&'static str> {
        if self.page == SALVAGE_PAGE {
            return Some(self.salvage.usage_view());
        }
        usage_page_id(&self.page)
    }

    /// The view of the page "Dismantle & Salvage" while it is open, else `None`.
    fn salvage_view_open(&self) -> Option<&'static str> {
        (self.page == SALVAGE_PAGE).then(|| self.salvage.usage_view())
    }

    /// A page was entered: counted with its way; the gallery counts as opened however it was reached.
    fn count_page(&mut self, route: Option<&str>) {
        self.salvage_counted = self.salvage_view_open();
        let Some(usage) = &self.usage else { return };
        if let Some(id) = self.usage_id() {
            usage.page_opened(id, route, None);
        }
        if self.page == "hf_wasistneu" {
            usage.action("galerie_offen");
        }
    }

    /// The other view of the page "Dismantle & Salvage" came up on the page itself (a click on a part, the way
    /// back, a search for the other kind): counted as a jump to the page id of that view.
    fn count_salvage_view(&mut self) {
        if !self.salvage.showing_result() {
            return;
        }
        let Some(view) = self.salvage_view_open() else { return };
        if self.salvage_counted.is_some_and(|counted| counted != view)
            && let Some(usage) = &self.usage
        {
            usage.page_opened(view, Some("sprung"), None);
        }
        self.salvage_counted = Some(view);
    }

    /// A group of the side bar opened or closed by a click.
    fn count_group_toggle(&self) {
        if let Some(usage) = &self.usage {
            usage.group_toggled();
        }
    }

    /// An action of `page_usage.ACTIONS`, only how often.
    fn count_action(&self, key: &str) {
        if let Some(usage) = &self.usage {
            usage.action(key);
        }
    }

    /// The window closes (`page_usage.window_closed`); `clock` for the checks. What follows in
    /// this window counts no more.
    fn count_window_end(&self, clock: Option<f64>) {
        if let Some(usage) = &self.usage {
            usage.window_closed(clock);
        }
    }

    /// Lets the report page send to a test receiver on this computer (the
    /// preview never sends anywhere else). Own data only.
    pub fn use_test_receiver(&mut self) {
        // Start the receiver first: if it cannot start, the page stays as it is.
        let Some(receiver) = diagnose_page::TestReceiver::start() else { return };
        if let Some(state) = self.diagnose.take() {
            self.diagnose = Some(state.with_receiver(receiver));
        }
    }

    /// Where the report page sends to; without this call nothing can be sent.
    pub fn use_report_target(&mut self, target: versekit_core::report_send::Target) {
        if let Some(state) = &mut self.diagnose {
            state.set_target(target);
        }
    }

    /// The key of the open page; empty for the overview.
    pub fn current_page(&self) -> &str {
        &self.page
    }

    /// Starts the hangar delivery for the browser extension (own data only).
    pub fn start_hangar_delivery(&mut self) {
        if let Some(store) = &self.store {
            self.hangar.start_receiver(&store.paths);
        }
    }

    /// Reads the game logs for the mission log in the background (own data
    /// only). The page reads them again whenever
    /// it is shown.
    pub fn start_contract_scan(&self) {
        if self.store.is_some() {
            self.contracts.rescan(None);
        }
    }

    /// Takes over what the settings file says.
    /// Hands over the start with the system of this system and the command it runs (see
    /// [`versekit_core::autostart`]): the switch in the settings then writes it. Only the program on the user's
    /// own data does this.
    pub fn use_autostart(&mut self, store: Box<dyn autostart::Store>, command: String) {
        self.autostart_on = autostart::is_on(store.as_ref());
        self.autostart = store;
        self.autostart_command = command;
    }

    /// On a copy of the data (`Host::copy`): the switch of the start with the system writes into the copy
    /// (`autostart::CopyFolder`), with a command that starts `program` (or the AppImage) on the copy. Never the
    /// real start. False when the data are no copy.
    pub fn use_copy_autostart(&mut self, program: &std::path::Path, appimage: Option<&str>) -> bool {
        let Some(host) = self.store.as_ref().map(|s| &s.paths.host).filter(|host| host.copy.is_some()) else {
            return false;
        };
        let arguments = versekit_core::desktop_entry::run_arguments(host);
        let command = autostart::command(program, appimage, &arguments);
        let store = autostart::CopyFolder(autostart::copy_folder(host));
        self.use_autostart(Box::new(store), command);
        true
    }

    /// The switch of the start with the system was clicked: the entry is written, and what the entry says
    /// afterwards is what the switch shows and what is said (not what was wished for).
    fn change_autostart(&mut self, wanted: bool) -> String {
        let written = autostart::set_on(self.autostart.as_ref(), wanted, &self.autostart_command);
        self.autostart_on = autostart::is_on(self.autostart.as_ref());
        match written {
            Ok(()) => {
                let word = t(if self.autostart_on { "e_an" } else { "e_aus" });
                tf("s_al_autostart", &[("0", Arg::from(word.as_str()))])
            }
            Err(_) => t("ov_as_fehler"),
        }
    }

    /// Streamer mode follows the switch: on for the whole program, and what the program knows about
    /// the person at the machine (home, user name, folders) is hidden from then on.
    fn apply_streamer(&self, paths: &Paths) {
        versekit_core::streamer::set_active(self.streamer);
        if self.streamer {
            versekit_core::streamer::learn_paths(paths);
        }
    }

    fn load_settings(&mut self, paths: &Paths) {
        self.module_hidden = versekit_core::modules::hidden_pages(paths);
        let data = paths.settings();
        if data.iter().any(|(key, _)| key == theme::SETTING) {
            theme::set_current(theme::choice_of(&data).id);
        }
        self.size = font_size::font_points(&data, &paths.host);
        self.general_english = match language::choice_of(&data) {
            language::Choice::En => true,
            language::Choice::De => false,
            language::Choice::Auto => language::system_language(&paths.host) == language::Lang::En,
        };
        self.language_auto = language::choice_of(&data) == language::Choice::Auto;
        self.play_head = paths.setting_bool(versekit_core::overview::PLAYTIME_HEAD_SETTING, false);
        self.check_interval = paths.setting_int(versekit_core::overlay::keys::INTERVAL, 3, Some(1), Some(60));
        self.groups_closed = GROUP_SETTINGS.map(|(_, key)| paths.setting_bool(key, false));
        self.group_order = group_order_from(paths.settings().iter().find(|(k, _)| k == GROUP_ORDER_SETTING).map(|(_, v)| v));
        self.groups_language = None;
        self.sounds[0] = paths.setting_bool("signalton", true);
        self.sounds[1] = versekit_core::sound::refinery_on(paths);
        self.volume = versekit_core::sound::volume(paths);
        // The card as the program started with it is the stored choice: only a change in this run asks for a restart.
        self.gpu = crate::renderer::use_card(paths);
        self.gpu_started = self.gpu;
        self.clock_24 = versekit_core::formats::apply_clock(paths);
        self.desktop.load(paths);
        self.mangohud = paths.setting_bool("mangohud", false);
        self.streamer = versekit_core::streamer::load(paths);
        self.apply_streamer(paths);
        self.usage_on = versekit_core::usage::enabled(paths);
        self.start_command = paths.setting(game_start::STARTER_SETTING).unwrap_or_default();
        self.overlay_clickable = paths.setting_bool("durchklickbar", false);
        self.overlay_rows = paths.setting_int(
            versekit_core::overlay::keys::ROWS,
            versekit_core::overlay::ROWS_DEFAULT,
            Some(OVERLAY_ROWS.0),
            Some(OVERLAY_ROWS.1),
        );
        self.overlay_auto = paths.setting_bool("durchklick_auto", true);
        self.overlay_opacity = paths.setting_int("deckkraft_prozent", 93, Some(30), Some(100));
        self.overlay_opaque = crate::renderer::overlay_opaque(paths);
        self.overlay_popup = paths.setting("overlay_modus").as_deref() == Some("popup");
        self.overlay_seconds = paths.setting_int("popup_sekunden", 6, Some(2), Some(60));
        self.overlay_events = versekit_core::overlay::Events::read(paths);
        self.overlay_sections = versekit_core::overlay::Sections::read(paths);
        self.overlay_contracts_max = versekit_core::overlay::Settings::read(paths).contracts_max;
        self.overlay_fps = paths.setting_bool(versekit_core::overlay::keys::FPS_LINE, false);
        // Read like the overlay reads them: without their own key they follow the frame rates.
        let overlay = versekit_core::overlay::Settings::read(paths);
        (self.overlay_server_fps, self.overlay_players) = (overlay.server_fps, overlay.players);
        self.overlay_info_bar =
            paths.setting_bool(versekit_core::overlay::keys::INFO_BAR, versekit_core::overlay::INFO_BAR_DEFAULT);
        self.overlay_info_bar_always = paths.setting_bool(
            versekit_core::overlay::keys::INFO_BAR_ALWAYS,
            versekit_core::overlay::INFO_BAR_ALWAYS_DEFAULT,
        );
        self.overlay_always = paths.setting_bool(
            versekit_core::overlay::keys::OVERLAY_ALWAYS,
            versekit_core::overlay::OVERLAY_ALWAYS_DEFAULT,
        );
        self.overlay_info_bar_corner = versekit_core::info_bar::Corner::read(paths);
        self.overlay_size = versekit_core::overlay::size_setting(paths, versekit_core::overlay::keys::SIZE);
        self.overlay_info_bar_size = versekit_core::overlay::size_setting(paths, versekit_core::overlay::keys::INFO_BAR_SIZE);
        if let Some((modifiers, key)) = paths.setting("hotkey").as_deref().and_then(parse_hotkey) {
            (self.hotkey_modifiers, self.hotkey_key) = (modifiers, key);
        }
        {
            use versekit_core::overlay::{self, DEFAULT_SCAN_HOTKEY, HotkeyChoice, keys};
            // As the overlay registers them: one left at its default whose key an earlier one holds shows the spare
            // it takes instead.
            let [_, scan, search, picture, main] =
                HotkeyChoice::read_all(paths).map(|c| c.combination().map(|h| h.text()));
            let stored = scan
                .or_else(|| paths.setting(keys::HOTKEY_SCAN))
                .unwrap_or_else(|| DEFAULT_SCAN_HOTKEY.to_string());
            if let Some((modifiers, key)) = parse_hotkey(&stored) {
                (self.scan_modifiers, self.scan_key) = (modifiers, key);
            }
            self.scan_hotkey_on = overlay::scan_hotkey_on(paths);
            self.hotkey_on = overlay::overlay_hotkey_on(paths);
            let stored = search
                .or_else(|| paths.setting(keys::HOTKEY_SEARCH))
                .unwrap_or_else(|| overlay::DEFAULT_SEARCH_HOTKEY.to_string());
            if let Some((modifiers, key)) = parse_hotkey(&stored) {
                (self.search_modifiers, self.search_key) = (modifiers, key);
            }
            self.search_hotkey_on = overlay::search_hotkey_on(paths);
            let stored = picture
                .or_else(|| paths.setting(keys::HOTKEY_PICTURE))
                .unwrap_or_else(|| overlay::DEFAULT_PICTURE_HOTKEY.to_string());
            if let Some((modifiers, key)) = parse_hotkey(&stored) {
                (self.picture_modifiers, self.picture_key) = (modifiers, key);
            }
            self.picture_hotkey_on = overlay::picture_hotkey_on(paths);
            let stored = main
                .or_else(|| paths.setting(keys::HOTKEY_MAIN))
                .unwrap_or_else(|| overlay::DEFAULT_MAIN_HOTKEY.to_string());
            if let Some((modifiers, key)) = parse_hotkey(&stored) {
                (self.main_modifiers, self.main_key) = (modifiers, key);
            }
            self.main_hotkey_on = overlay::main_hotkey_on(paths);
        }
        if let Some(place) = paths.setting("overlay_ecke").as_deref().and_then(corner_of) {
            self.overlay_place = place;
        }
        self.bar_side = match paths.setting("overlay_leiste").as_deref() {
            Some("unten") => settings::BarSide::Bottom,
            _ => settings::BarSide::Top,
        };
        self.update_test = paths.setting_bool("vorabversionen", false);
        self.mining.scanner.load(paths);
        self.progress_view.watch_only = paths.setting_bool(blueprint_progress::WATCHLIST_SETTING, false);
        self.window = WindowPlace::from_settings(paths);
        // The game folder as set, or as found; a game folder given from outside wins over the
        // setting. The other channels follow below.
        let live = paths
            .given()
            .game
            .map(|fixed| fixed.folder)
            .or_else(|| paths.setting("spiel_ordner"))
            .or_else(|| paths.game_folder())
            .unwrap_or_default();
        self.game_found =
            paths.game_folder().or_else(|| paths.given().game.map(|f| f.folder).filter(|f| folder_there(f)));
        self.game_searched = if self.game_found.is_none() { paths.searched_game_locations(4) } else { Vec::new() };
        let flavor = paths.host.flavor;
        let base = flavor.dirname(live.trim_end_matches(flavor.sep()));
        self.live_folder_saved.clone_from(&live);
        self.folders[0] = live;
        // A folder chosen for a channel, else the one next to LIVE. With a game folder given from
        // outside only the copy counts, so the choice is left aside then.
        let key = versekit_core::paths::SETTING_CHANNEL_FOLDERS;
        self.channels_saved = match paths.settings().into_iter().find(|(k, _)| k == key) {
            Some((_, Value::Object(chosen))) if paths.given().game.is_none() => chosen,
            _ => Vec::new(),
        };
        for (k, channel) in CHANNEL_ROWS.into_iter().enumerate().skip(1) {
            let folder = flavor.join(&base, channel);
            self.channels_found[k] =
                if !base.is_empty() && std::path::Path::new(&folder).is_dir() { folder } else { String::new() };
            self.folders[k] = self.chosen_channel(k).unwrap_or_else(|| self.channels_found[k].clone());
        }
    }

    /// The folder chosen for the channel of row `k` (`CHANNEL_ROWS[k]`, 1 and up), if any.
    fn chosen_channel(&self, k: usize) -> Option<String> {
        let (_, folder) = self.channels_saved.iter().find(|(name, _)| name == CHANNEL_ROWS[k])?;
        folder.as_str().map(|f| f.trim().to_string()).filter(|f| !f.is_empty())
    }

    /// The settings this window shares with the settings file, as stored there.
    /// A hotkey without a modifier key is left out: 4.12 would not take it.
    fn setting_values(&self) -> Vec<(&'static str, Value)> {
        let mut values = vec![(theme::SETTING, Value::str(theme::current().id))];
        values.extend(font_size::settings_for(self.size));
        values.extend([
            (
                "sprache",
                Value::str(if self.language_auto {
                    "auto"
                } else if self.general_english {
                    "en"
                } else {
                    "de"
                }),
            ),
            ("signalton", Value::Bool(self.sounds[0])),
            (versekit_core::sound::REFINERY_SETTING, Value::Bool(self.sounds[1])),
            (versekit_core::sound::VOLUME_SETTING, Value::int(self.volume)),
            (crate::renderer::GPU_SETTING, Value::Bool(self.gpu)),
            (versekit_core::formats::CLOCK_24_SETTING, Value::Bool(self.clock_24)),
            ("mangohud", Value::Bool(self.mangohud)),
            (versekit_core::streamer::SETTING, Value::Bool(self.streamer)),
            (versekit_core::usage::SETTING, Value::Bool(self.usage_on)),
            ("durchklickbar", Value::Bool(self.overlay_clickable)),
            ("max_zeilen", Value::int(self.overlay_rows)),
            ("durchklick_auto", Value::Bool(self.overlay_auto)),
            ("deckkraft_prozent", Value::int(self.overlay_opacity)),
            ("overlay_modus", Value::str(if self.overlay_popup { "popup" } else { "immer" })),
            ("popup_sekunden", Value::int(self.overlay_seconds)),
            (versekit_core::overlay::keys::INTERVAL, Value::int(self.check_interval)),
        ]);
        values.extend(GROUP_SETTINGS.iter().zip(self.groups_closed).map(|((_, key), closed)| (*key, Value::Bool(closed))));
        values.push((GROUP_ORDER_SETTING, group_order_value(&self.group_order)));
        values.extend([
            ("overlay_ecke", Value::str(corner_name(self.overlay_place))),
            ("overlay_leiste", Value::str(if self.bar_side == settings::BarSide::Bottom { "unten" } else { "oben" })),
            ("vorabversionen", Value::Bool(self.update_test)),
            (blueprint_progress::WATCHLIST_SETTING, Value::Bool(self.progress_view.watch_only)),
            ("spiel_ordner", Value::str(&self.live_folder_saved)),
            (versekit_core::paths::SETTING_CHANNEL_FOLDERS, Value::Object(self.channels_saved.clone())),
        ]);
        values.extend(
            versekit_core::overlay::Event::ALL.map(|e| (e.key(), Value::Bool(self.overlay_events.contains(e)))),
        );
        values.extend(
            versekit_core::overlay::Section::ALL.map(|s| (s.key(), Value::Bool(self.overlay_sections.contains(s)))),
        );
        values.push((versekit_core::overlay::keys::CONTRACTS_MAX, Value::int(self.overlay_contracts_max)));
        values.push((versekit_core::overlay::keys::FPS_LINE, Value::Bool(self.overlay_fps)));
        values.push((versekit_core::overlay::keys::SERVER_FPS, Value::Bool(self.overlay_server_fps)));
        values.push((versekit_core::overlay::keys::PLAYERS, Value::Bool(self.overlay_players)));
        values.push((versekit_core::overlay::keys::INFO_BAR, Value::Bool(self.overlay_info_bar)));
        values.push((versekit_core::overlay::keys::INFO_BAR_ALWAYS, Value::Bool(self.overlay_info_bar_always)));
        values.push((versekit_core::overlay::keys::OVERLAY_ALWAYS, Value::Bool(self.overlay_always)));
        values.push((versekit_core::info_bar::keys::CORNER, Value::str(self.overlay_info_bar_corner.word())));
        values.push((versekit_core::overlay::keys::SIZE, Value::int(self.overlay_size)));
        values.push((versekit_core::overlay::keys::INFO_BAR_SIZE, Value::int(self.overlay_info_bar_size)));
        values.extend(self.desktop.setting_values());
        if let Some(hotkey) = hotkey_text(self.hotkey_modifiers, self.hotkey_key) {
            values.push(("hotkey", Value::str(hotkey)));
        }
        if let Some(hotkey) = hotkey_text(self.scan_modifiers, self.scan_key) {
            values.push((versekit_core::overlay::keys::HOTKEY_SCAN, Value::str(hotkey)));
        }
        values.push((versekit_core::overlay::keys::HOTKEY_SCAN_ON, Value::Bool(self.scan_hotkey_on)));
        if let Some(hotkey) = hotkey_text(self.search_modifiers, self.search_key) {
            values.push((versekit_core::overlay::keys::HOTKEY_SEARCH, Value::str(hotkey)));
        }
        values.push((versekit_core::overlay::keys::HOTKEY_SEARCH_ON, Value::Bool(self.search_hotkey_on)));
        if let Some(hotkey) = hotkey_text(self.picture_modifiers, self.picture_key) {
            values.push((versekit_core::overlay::keys::HOTKEY_PICTURE, Value::str(hotkey)));
        }
        values.push((versekit_core::overlay::keys::HOTKEY_PICTURE_ON, Value::Bool(self.picture_hotkey_on)));
        if let Some(hotkey) = hotkey_text(self.main_modifiers, self.main_key) {
            values.push((versekit_core::overlay::keys::HOTKEY_MAIN, Value::str(hotkey)));
        }
        values.push((versekit_core::overlay::keys::HOTKEY_MAIN_ON, Value::Bool(self.main_hotkey_on)));
        if let Some(window) = &self.window {
            values.extend(window.values());
        }
        values.extend(self.hangar.setting_values());
        values.extend(self.stock.setting_values());
        values.extend(self.crafting.setting_values());
        values.extend(self.selling.setting_values());
        values.extend(self.org_stock.setting_values());
        values.extend(self.contested.setting_values());
        values.extend(self.mining.scanner.setting_values());
        values.extend(self.start_page_values());
        values
    }

    /// The remembered window place, to open the window there.
    pub fn window_place(&self) -> Option<WindowPlace> {
        self.window
    }

    /// Follows the window: once it stood still for `WINDOW_SETTLE_SECONDS`,
    /// its place counts and is written with the other settings. While
    /// maximized the size and place from before stay. Minimized counts not.
    fn follow_window(&mut self, ctx: &egui::Context) {
        let (inner, outer, maximized, minimized, scale, now) = ctx.input(|i| {
            let v = i.viewport();
            (v.inner_rect, v.outer_rect, v.maximized, v.minimized, i.pixels_per_point, i.time)
        });
        if minimized == Some(true) {
            return;
        }
        let Some(inner) = inner else { return };
        let maximized = maximized == Some(true);
        let place = match self.window.filter(|_| maximized) {
            Some(before) => WindowPlace { maximized: true, ..before },
            None => WindowPlace {
                size: ((inner.width() * scale).round() as i64, (inner.height() * scale).round() as i64),
                position: outer.map(|o| ((o.min.x * scale).round() as i64, (o.min.y * scale).round() as i64)),
                maximized,
            },
        };
        if self.window == Some(place) {
            self.window_seen = None;
            return;
        }
        match self.window_seen {
            Some((seen, since)) if seen == place => {
                if now - since >= WINDOW_SETTLE_SECONDS {
                    self.window = Some(place);
                    self.window_seen = None;
                } else {
                    ctx.request_repaint_after(std::time::Duration::from_secs_f64(WINDOW_SETTLE_SECONDS));
                }
            }
            _ => {
                self.window_seen = Some((place, now));
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(WINDOW_SETTLE_SECONDS));
            }
        }
    }

    /// Writes every setting that changed since the last frame. The first
    /// frame only takes note: a size the screen capped is no change.
    fn save_changes(&mut self) {
        let Some(store) = &self.store else { return };
        let now = self.setting_values();
        let Some(saved) = &store.saved else {
            if let Some(store) = &mut self.store {
                store.saved = Some(now);
            }
            return;
        };
        let changed: Vec<(&'static str, Value)> =
            now.iter().filter(|(key, value)| !saved.iter().any(|(k, v)| k == key && v == value)).cloned().collect();
        if changed.is_empty() {
            return;
        }
        let Some(store) = &mut self.store else { return };
        if let Some(writer) = &mut store.writer {
            writer.sent += 1;
            if writer.jobs.send(changed).is_err() {
                store.failed = true;
            }
            store.saved = Some(now);
            return;
        }
        let failed = changed.iter().any(|(key, value)| !store.paths.set_setting(key, value.clone()));
        store.failed |= failed;
        store.saved = Some(now);
        // The overlay takes the change over at once, not at its next look.
        window_signal::send_to(Role::Overlay, Signal::Settings);
    }

    /// Writes the settings in a thread of its own from now on (the window;
    /// the tests write here and now).
    pub fn write_in_background(&mut self) {
        if let Some(store) = &mut self.store
            && store.writer.is_none()
        {
            store.writer = Some(SettingsWriter::start(Paths::new(store.paths.host.clone())));
        }
    }

    /// Under test: every write of this window's settings thread takes `ms` longer (only this window's).
    #[cfg(test)]
    fn slow_writes(&self, ms: u64) {
        if let Some(writer) = self.store.as_ref().and_then(|s| s.writer.as_ref()) {
            writer.delay_ms.store(ms, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Under test: the extra time of every write of this window's settings thread.
    #[cfg(test)]
    fn write_delay(&self) -> Option<u64> {
        let writer = self.store.as_ref().and_then(|s| s.writer.as_ref())?;
        Some(writer.delay_ms.load(std::sync::atomic::Ordering::Relaxed))
    }

    /// Settings sent to the settings thread and not written yet.
    fn writes_pending(&self) -> bool {
        self.store.as_ref().and_then(|s| s.writer.as_ref()).is_some_and(SettingsWriter::pending)
    }

    /// Whether this window writes its settings, and whether a write failed.
    pub fn saving(&self) -> Option<bool> {
        self.store.as_ref().map(|store| {
            let thread_failed =
                store.writer.as_ref().is_some_and(|w| w.failed.load(std::sync::atomic::Ordering::Relaxed));
            !store.failed && !thread_failed
        })
    }

    fn content(&mut self, ui: &mut egui::Ui, scheme: &Scheme) {
        // The preview shows only the window; scheme and size are in its settings.
        if self.preview {
            self.window_frame(ui, scheme);
            return;
        }
        widgets::heading(ui, scheme, &t("s_mu_farbschemata"));
        ui.horizontal_wrapped(|ui| {
            for s in theme::schemes() {
                if widgets::radio(ui, scheme, s.id == scheme.id, s.id).clicked() {
                    theme::set_current(s.id);
                }
            }
        });
        ui.add_space(4.0);
        self.size_row(ui, scheme);
        ui.add_space(GAP);
        widgets::card(ui, scheme, |ui| self.window_and_behavior(ui, scheme));
        ui.add_space(GAP);
        widgets::card(ui, scheme, |ui| self.wizard(ui, scheme));
        ui.add_space(GAP);
        widgets::card(ui, scheme, |ui| self.inputs(ui, scheme));
        ui.add_space(GAP);
        layout::columns_or_stack(ui, SECTION_MIN_WIDTH, 2, |ui, i| {
            if i == 0 {
                widgets::card(ui, scheme, |ui| self.own_controls(ui, scheme));
            } else {
                widgets::card(ui, scheme, |ui| self.built_in_controls(ui, scheme));
            }
        });
        ui.add_space(GAP);
        widgets::card(ui, scheme, |ui| self.data(ui, scheme));
        ui.add_space(GAP);
        widgets::card(ui, scheme, |ui| {
            widgets::card_title(ui, scheme, &t("s_mu_waerme"));
            ui.add_space(4.0);
            let days = days();
            let days: Vec<&str> = days.iter().map(String::as_str).collect();
            widgets::heat_map(ui, scheme, &days, &self.heat);
        });
        ui.add_space(GAP);
        Self::colors(ui, scheme);
    }

    fn swatch(ui: &mut egui::Ui, name: &str, c: Color) {
        ui.allocate_ui(egui::vec2(250.0, 20.0), |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(28.0, 16.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 2.0, color(c));
                ui.label(format!("{name}  {}", c.to_hex()));
            });
        });
    }

    fn colors(ui: &mut egui::Ui, s: &Scheme) {
        let all = [
            ("bg", s.bg),
            ("surface", s.surface),
            ("bar", s.bar),
            ("fg", s.fg),
            ("sub", s.sub),
            ("accent", s.accent),
            ("accent_dark", s.accent_dark),
            ("line", s.line),
            ("field", s.field),
            ("gold", s.gold),
            ("yellow", s.yellow),
            ("blue", s.blue),
            ("red", s.red),
            ("red_pale", s.red_pale),
            ("hover", s.hover),
            ("selected", s.selected),
            ("danger_fill", s.danger_fill),
            ("track", s.track),
            ("track_on", s.track_on),
            ("scroll_groove", s.scroll_groove),
            ("scroll_grip", s.scroll_grip),
            ("scroll_grip_light", s.scroll_grip_light),
            ("control", s.control()),
            ("control_border", s.control_border()),
            ("hover_fg", s.hover_fg()),
        ];
        ui.horizontal_wrapped(|ui| {
            for (name, c) in all {
                Self::swatch(ui, name, c);
            }
        });
    }

    /// Size of the whole interface: presets per screen and a stepless slider,
    /// capped to what the screen can hold.
    fn size_row(&mut self, ui: &mut egui::Ui, scheme: &Scheme) {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(t("s_ei_groesse")).strong());
            size_slider(ui, scheme, &mut self.size);
        });
        size_presets(ui, scheme, &mut self.size);
    }

    /// The setup assistant: the steps at the left, each one clickable,
    /// a check mark only for confirmed steps; finishing stays
    /// blocked while a step is open.
    fn wizard(&mut self, ui: &mut egui::Ui, scheme: &Scheme) {
        const STEPS: [&str; 4] = ["s_ei_spielordner", "sprache", "hf_overlay", "fertig"];
        let names: Vec<String> = STEPS.iter().map(|key| t(key)).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        ui.label(RichText::new(t("s_mu_wiz_titel")).strong());
        ui.add_space(4.0);
        widgets::list_frame(ui, scheme, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_top(|ui| {
                let list = ui.allocate_ui_with_layout(
                    egui::vec2(180.0_f32.min(ui.available_width() * 0.4), 200.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| settings::step_list(ui, scheme, &names, self.wizard.step, &self.wizard.done),
                );
                if let Some(step) = list.inner {
                    self.wizard.step = step;
                }
                ui.add_space(GAP);
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), 200.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        let step = self.wizard.step;
                        let last = step + 1 == STEPS.len();
                        widgets::heading(ui, scheme, names[step]);
                        match step {
                            0 => {
                                let found = tf(
                                    "s_mu_wiz_gefunden",
                                    &[("path", Arg::from("C:\\Program Files\\Roberts Space Industries"))],
                                );
                                ui.label(RichText::new(found).color(color(scheme.sub)));
                            }
                            1 => {
                                ui.horizontal_wrapped(|ui| {
                                    if widgets::radio(ui, scheme, !self.wizard_english, "Deutsch").clicked() {
                                        self.wizard_english = false;
                                    }
                                    if widgets::radio(ui, scheme, self.wizard_english, "English").clicked() {
                                        self.wizard_english = true;
                                    }
                                });
                            }
                            2 => {
                                widgets::switch_row(ui, scheme, &t("s_mu_wiz_overlay"), &mut self.wizard_overlay);
                            }
                            _ => {
                                let open: Vec<&str> = self.wizard.open().iter().map(|&k| names[k]).collect();
                                let (text, tone) = if self.wizard.finished() {
                                    (tf("s_mu_wiz_fertig", &[("name", Arg::from(t("hf_einrichtung")))]), scheme.sub)
                                } else if open.is_empty() {
                                    (t("s_mu_wiz_alle"), scheme.sub)
                                } else {
                                    (tf("s_mu_wiz_offen", &[("steps", Arg::from(open.join(", ")))]), scheme.yellow)
                                };
                                ui.label(RichText::new(text).color(color(tone)));
                            }
                        }
                        ui.add_space(GAP);
                        ui.horizontal(|ui| {
                            let start = t("s_mu_wiz_anfang");
                            let first = (step == 0).then_some(start.as_str());
                            if ui
                                .add(widgets::Button::new(&t("zurueck"), ButtonKind::Normal, scheme).blocked(first))
                                .clicked()
                            {
                                self.wizard.step -= 1;
                            }
                            let (done, first_all) = (t("s_mu_wiz_schon"), t("s_mu_wiz_erst"));
                            let blocked = if !last {
                                None
                            } else if self.wizard.finished() {
                                Some(done.as_str())
                            } else if !self.wizard.open().is_empty() {
                                Some(first_all.as_str())
                            } else {
                                None
                            };
                            let name = t(if last { "fertig" } else { "s_mlg_bestaetigen" });
                            if ui
                                .add(widgets::Button::new(&name, ButtonKind::Strong, scheme).blocked(blocked))
                                .clicked()
                            {
                                self.wizard.confirm();
                            }
                        });
                    },
                );
            });
        });
    }

    /// Settings that save at once, sounds, chart, choosing several rows,
    /// motion. The window model is its own program
    /// (`examples/vorschau.rs`).
    fn window_and_behavior(&mut self, ui: &mut egui::Ui, scheme: &Scheme) {
        layout::columns_or_stack(ui, SECTION_MIN_WIDTH, 2, |ui, i| {
            if i == 0 {
                ui.label(RichText::new(t("s_mu_sofort")).strong());
                let before = (self.overlay_rows, self.sounds, self.volume);
                ui.horizontal(|ui| {
                    ui.label(t("s_zeilen"));
                    ui.add(widgets::Slider::new(&mut self.overlay_rows, 3, 15, scheme).width(120.0));
                    ui.label(self.overlay_rows.to_string());
                });
                ui.label(RichText::new(t("s_ei_toene")).strong());
                for (k, name) in ["s_ei_ton_bp", "s_ei_ton_raff", "s_ei_ton_fehler"].iter().enumerate() {
                    widgets::switch_row(ui, scheme, &t(name), &mut self.sounds[k]);
                }
                ui.horizontal(|ui| {
                    ui.label(t("s_ei_lautstaerke"));
                    ui.add(widgets::Slider::new(&mut self.volume, 0, 100, scheme).width(120.0));
                    ui.label(format!("{} %", self.volume));
                });
                if (self.overlay_rows, self.sounds, self.volume) != before {
                    self.saved_at = Some(ui.input(|t| t.time));
                }
                widgets::saved_hint(ui, scheme, self.saved_at, &t("s_mu_gespeichert"));
            } else {
                ui.label(RichText::new(t("s_mu_wochentag")).strong());
                let format = |h: f32| format!("{} h", number(f64::from(h), 1));
                let week = hours();
                let points: Vec<(&str, f32)> = week.iter().map(|(day, hours)| (day.as_str(), *hours)).collect();
                widgets::line_chart(ui, scheme, &points, egui::vec2(340.0, widgets::CHART_HEIGHT), &format);
            }
        });
        ui.add_space(GAP);
        layout::columns_or_stack(ui, SECTION_MIN_WIDTH, 2, |ui, i| {
            if i == 0 {
                ui.label(RichText::new(t("s_mu_mehrere")).strong());
                let modifiers = ui.input(|t| t.modifiers);
                widgets::list_frame(ui, scheme, |ui| {
                    ui.set_min_width(ui.available_width());
                    for (row, (name, ..)) in ORES.iter().enumerate() {
                        if widgets::list_row(ui, scheme, self.selection.is_chosen(row), name).clicked() {
                            self.selection.click(row, modifiers);
                        }
                    }
                });
                let count = self.selection.count();
                if count > 1 {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(tf("s_mu_gewaehlt", &[("count", Arg::from(count))]));
                        if ui.add(widgets::Button::new(&t("s_mu_auswahl_weg"), ButtonKind::Normal, scheme)).clicked() {
                            self.selection.clear();
                        }
                    });
                }
            } else {
                ui.label(RichText::new(t("s_mu_einblenden")).strong());
                let text = t(if self.revealed { "s_mu_verbergen" } else { "s_mu_zeigen" });
                if ui.add(widgets::Button::new(&text, ButtonKind::Normal, scheme)).clicked() {
                    self.revealed = !self.revealed;
                }
                widgets::reveal(ui, "einblenden", self.revealed, |ui| {
                    widgets::card(ui, scheme, |ui| {
                        ui.label(tf("s_mu_neuer_bp", &[("name", Arg::from(t("s_mu_n_arclight")))]));
                        let place = tf("s_mu_gefunden_in", &[("place", Arg::from("Lorville"))]);
                        ui.label(RichText::new(place).color(color(scheme.sub)));
                    });
                });
            }
        });
    }

    /// The head bar of the window model (4.12): name, version and play time
    /// at the left, the setup button at the right. No update button:
    /// the update bar takes its place.
    fn head_bar(&mut self, ui: &mut egui::Ui, scheme: &Scheme) {
        use versekit_core::language::t;
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), widgets::BAR_HEIGHT), egui::Sense::hover());
        ui.painter().rect_filled(rect, 0.0, color(scheme.bar));
        if let Some(line) = head_line(rect, scheme) {
            ui.painter().rect_filled(line, 0.0, color(scheme.accent));
        }
        let mut inner = rect.shrink2(egui::vec2(10.0, 0.0));
        if self.own_frame {
            inner.max.x = frame_buttons(ui, scheme, rect) - 6.0;
        }
        let mut right = ui
            .new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::right_to_left(egui::Align::Center)));
        // The update button at the far right like 4.12: always the way to the update page;
        // in the accent with the number while a new version waits.
        let (word, hint, alarm) = if self.update_waits() {
            let name = [("0", Arg::from(self.update_name().as_str()))];
            (tf("hf_update_da", &name), tf("hf_hinweis_update_da", &name), widgets::Alarm::News)
        } else {
            (t("hf_update"), t("hf_hinweis_update"), widgets::Alarm::No)
        };
        if widgets::bar_action(&mut right, scheme, "download", &word, &hint, alarm).clicked() {
            self.page = "hf_ueber".to_string();
            self.usage_route = Some("sprung");
        }
        right.add_space(8.0);
        if widgets::bar_action(&mut right, scheme, "wand-sparkles", &t("hf_einrichtung"), &t("hf_hinweis_einr"), false)
            .clicked()
        {
            self.open_setup();
        }
        // The play time at the right, before the setup button (like 4.12);
        // laid out from the right, so the text comes before its clock. On real
        // data the counted one with the running session, `0 min` while nothing is counted.
        right.add_space(16.0);
        self.follow_available_tiles();
        self.refresh_overview(ui.input(|i| i.time));
        let played = match &self.overview {
            _ if !self.play_head => None,
            Some(overview) => overview.head.clone(),
            None => Some(("412 h 30 min  (+1 h 05 min)".to_string(), t("hf_zeit_h").replace("{0}", "04.08.2026"))),
        };
        if let Some((time, hover)) = played {
            let label = egui::Label::new(RichText::new(time).small().color(color(scheme.sub))).selectable(false);
            right.add(label).on_hover_text(hover);
            if let Some(image) = crate::icons::image("clock", scheme.sub, 14.0) {
                right.add(image);
            }
        }
        let left_rect = egui::Rect::from_min_max(inner.min, egui::pos2(right.min_rect().left() - 12.0, inner.max.y));
        let mut left = ui.new_child(
            egui::UiBuilder::new().max_rect(left_rect).layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        // The icon of the program before its name, as tall as the line.
        let icon = crate::window_look::program_texture(left.ctx());
        let side = left.text_style_height(&egui::TextStyle::Body).max(16.0);
        left.add(egui::Image::new((icon.id(), egui::vec2(side, side))));
        // The name one step larger than the text of the bar, for the room the title bar gives; it is no text to select,
        // so a drag on it moves the window.
        let name_size = egui::TextStyle::Body.resolve(left.style()).size + 2.0;
        left.add(egui::Label::new(RichText::new(t("hf_titel")).strong().size(name_size)).selectable(false));
        let test_run =
            self.real() && self.store.as_ref().is_some_and(|store| versekit_core::language::test_run(&store.paths.host));
        let word_key = head_word(self.real(), self.test_build, test_run);
        let word = word_key.map(t);
        let version = if !self.real() && self.test_build {
            concat!(env!("CARGO_PKG_VERSION"), "-rc2")
        } else {
            env!("CARGO_PKG_VERSION")
        };
        let mark = widgets::version_mark(&mut left, scheme, version, word.as_deref());
        let mark = if test_run { mark.on_hover_text(t("s_kopf_testfassung_h")) } else { mark };
        // Sample page only: a click on the version switches stable and test build.
        if !self.real() {
            let click = left.interact(mark.rect, left.scope_id().with("version"), egui::Sense::click());
            if click.on_hover_text(t("s_mu_version_h")).clicked() {
                self.test_build = !self.test_build;
            }
        }
        // Streamer mode: the mark stays in the head bar as long as it is on; a click leads to the switch.
        if versekit_core::streamer::active() && left.available_width() > 120.0 {
            left.add_space(8.0);
            if widgets::bar_action(
                &mut left,
                scheme,
                "eye-off",
                &t("streamer_kopf"),
                &t("streamer_kopf_h"),
                widgets::Alarm::News,
            )
            .clicked()
            {
                self.page = "hf_allgemein".to_string();
                self.usage_route = Some("sprung");
            }
        }
        // An embedding program may mark the head bar; a click on its mark can open a page.
        if let Some(slot) = &mut self.extension {
            left.add_space(8.0);
            if let Some(page) = slot.extension.head(&mut left, scheme) {
                self.page = page;
            }
        }
    }

    /// The short notes above a page that read the game data: that a reading waits for the game to close, and that
    /// the data of a test channel are preliminary.
    fn archive_notes(&self, page: &mut egui::Ui, scheme: &Scheme) {
        use versekit_core::language::t;
        let note = |page: &mut egui::Ui, text: String| {
            page.add(egui::Label::new(RichText::new(text).small().color(color(scheme.sub))).wrap());
            page.add_space(4.0);
        };
        // Only the page whose data the waiting reading is for says it: data are missing or old there.
        let waits = TOPIC_PAGES
            .iter()
            .any(|(topic, pages)| pages.contains(&self.page.as_str()) && versekit_core::archive_lock::waiting_for(topic));
        if waits {
            note(page, t("s_ak_wartet"));
            page.ctx().request_repaint_after(std::time::Duration::from_secs(2));
        }
        if self.store.is_some()
            && ARCHIVE_PAGES.contains(&self.page.as_str())
            && let Some(channel) = self.test_archive(page.ctx())
        {
            for key in versekit_core::archive_lock::test_channel_notes(channel) {
                note(page, versekit_core::language::tf(key, &[("0", Arg::from(channel))]));
            }
        }
    }

    /// The channel of test builds the archive in use lies in, if any; looked at every few seconds, not every frame.
    fn test_archive(&self, ctx: &egui::Context) -> Option<&'static str> {
        let store = self.store.as_ref()?;
        let id = egui::Id::unique("archiv-testkanal");
        let now = ctx.input(|i| i.time);
        if let Some((at, answer)) = ctx.data(|d| d.get_temp::<(f64, Option<&'static str>)>(id))
            && now - at < 5.0
        {
            return answer;
        }
        let answer = store
            .paths
            .archive_file()
            .and_then(|archive| versekit_core::archive_lock::test_channel_of(std::path::Path::new(&archive)));
        ctx.data_mut(|d| d.insert_temp(id, (now, answer)));
        answer
    }

    /// Whether real data are open (not the sample page).
    fn real(&self) -> bool {
        self.store.is_some()
    }

    /// The language to start in: the stored one on real data, German on the sample
    /// page (its sample texts are German). The program sets it once at start.
    pub fn start_language(&self) -> language::Choice {
        match (self.real(), self.general_english) {
            (true, true) => language::Choice::En,
            _ => language::Choice::De,
        }
    }

    /// The version the head button and the update bar name: on real data the one on offer in the remembered list
    /// of releases, else the one an embedding program named (the sample page names a made-up one).
    fn update_name(&self) -> String {
        match self.updates.newer() {
            Some(newer) if self.real() => newer.to_string(),
            _ => self.update_version.clone(),
        }
    }

    /// Whether the update bar shows: not loaded yet, and a version to name.
    fn update_waits(&self) -> bool {
        !self.update_loaded && !self.update_name().is_empty()
    }

    /// The page „Update & Über“ on real data; what the player asked for in it.
    fn show_about(&mut self, ui: &mut egui::Ui, scheme: &Scheme) -> Option<update_page::PageOut> {
        let store = self.store.as_ref()?;
        let now = ui.input(|i| i.time);
        self.updates.look(ui.ctx(), &store.paths, now, self.update_test);
        let folder = store.paths.app_folder();
        let version = self.updates.own_version().to_string();
        let facts = update_page::Facts {
            version: &version,
            known: self.blueprints.catalog.blueprints.len(),
            have: self.blueprints.have.len(),
            folder: &folder,
            test_on: self.update_test,
        };
        Some(self.updates.page(ui, scheme, &facts))
    }

    /// Lets the window update the program: how it is started, where it runs, and what it fetches with.
    pub fn use_updates(&mut self, env: versekit_core::update_flow::Env) {
        self.updates = update_page::UpdateUi::with_env(env);
    }

    /// An update was handed over: the program has to end now. Asked once; the main program closes the window and
    /// the overlay.
    pub fn take_update_quit(&mut self) -> bool {
        std::mem::take(&mut self.update_quit)
    }

    /// The click on „update“ in the bar on real data: the question before it is installed. A build of the source
    /// is updated by `git pull`; it is told on the page, not asked.
    fn ask_update(&mut self, now: f64) {
        match self.updates.begin(self.update_test) {
            update_page::Begin::Source => {
                self.pin_notice = Some((now, t("update_quellcode")));
                self.page = "hf_ueber".to_string();
                self.usage_route = Some("sprung");
            }
            update_page::Begin::Asked | update_page::Begin::Nothing => {}
        }
    }

    /// What updating does every frame on real data: the list of releases is read again now and then, the fetching
    /// tells its sentences, and the question stands over the window.
    fn update_frame(&mut self, ctx: &egui::Context, scheme: &Scheme) {
        let Some(store) = &self.store else { return };
        let now = ctx.input(|i| i.time);
        self.updates.refresh(ctx, &store.paths, now, false, self.update_test);
        let (said, quit) = self.updates.poll(ctx);
        let asked = self.updates.dialogs(ctx, scheme, &store.paths);
        for text in said.into_iter().chain(asked) {
            self.pin_notice = Some((now, text));
        }
        self.update_quit |= quit;
    }

    /// The left of the foot bar while nothing loads: on real data whether the
    /// game runs ([`game_running_for_foot`]), looked at every two seconds; on the
    /// sample page a made-up game state.
    fn foot_idle(&mut self, now: f64) -> String {
        use versekit_core::language::t;
        let Some(store) = &self.store else {
            return format!("{}  ·  {}", t("s_mu_spiel_laeuft"), t("hf_sofort"));
        };
        let running = match self.game_seen {
            Some((at, running)) if now - at < GAME_LOOK_SECONDS => running,
            _ => {
                let running = game_running_for_foot(&store.paths, (self.game_process)(&store.paths));
                self.game_seen = Some((now, running));
                running
            }
        };
        let game = if running { t("ov_spiel_an") } else { t("ov_spiel_aus") };
        // The graphics page keeps its changes until the button takes them: its foot says so (4.12 `FOOTER_KEYS`).
        let saving = if self.page == "hf_grafik" { t("hf_per_knopf") } else { t("hf_sofort") };
        format!("{game}  ·  {saving}")
    }

    /// The foot bar of the window model: what loads right now (at start a
    /// short demo; the update button loads the named version), otherwise the
    /// game state; at the right the backup, what's-new and report buttons.
    fn foot(&mut self, ui: &mut egui::Ui, scheme: &Scheme) {
        use versekit_core::language::t;
        let now = ui.input(|i| i.time);
        if !self.loading_shown {
            self.loading_shown = true;
            self.loading = Some((now, t("s_mu_basetool_laedt"), 6.0));
            self.loading_update = false;
        }
        let mut activity = None;
        if let Some((since, text, seconds)) = &self.loading {
            let done = ((now - since) / seconds) as f32;
            if done >= 1.0 {
                let update = self.loading_update;
                self.loading = None;
                self.loading_update = false;
                if update {
                    self.update_loaded = true;
                    let notice = tf("s_ul_geladen", &[("version", Arg::from(self.update_version.as_str()))]);
                    self.pin_notice = Some((now, notice));
                    // Once after an update: the what's-new gallery opens by itself.
                    self.page = "hf_wasistneu".to_string();
                    self.news_index = 0;
                }
            } else {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
                activity = Some((text.clone(), done));
            }
        }
        if self.store.is_some() {
            // The game may start or end while nothing is clicked.
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(GAME_LOOK_SECONDS));
        }
        let idle = self.foot_idle(now);
        // A version being fetched shows its real progress, before anything else that loads.
        let fetching = if self.real() { self.updates.activity() } else { None };
        let activity = match (&fetching, &activity) {
            (Some((text, done)), _) => Some(widgets::Activity { text, done: *done }),
            (None, Some((text, done))) => Some(widgets::Activity { text, done: Some(*done) }),
            (None, None) => None,
        };
        let mut open = None;
        let _ = widgets::foot_bar(ui, scheme, &idle, activity.as_ref(), |bar| {
            // Pale red while nothing is written down, strong red with the count once there is.
            let (word, alarm) = if self.error_count == 0 {
                (t("hf_diagnose"), widgets::Alarm::Quiet)
            } else {
                (format!("{} ({})", t("hf_diagnose"), self.error_count), widgets::Alarm::Loud)
            };
            if widgets::bar_action(bar, scheme, "bug", &word, &t("hf_diagnose"), alarm).clicked() {
                open = Some("hf_diagnose");
            }
            if widgets::bar_action(bar, scheme, "sparkles", &t("hf_wasistneu"), &t("hf_hinweis_neu"), false).clicked() {
                open = Some("hf_wasistneu");
            }
            if widgets::bar_action(bar, scheme, "archive-restore", &t("hf_sicherung"), &t("hf_hinweis_sich"), false)
                .clicked()
            {
                if self.real() {
                    self.open_backup();
                } else {
                    self.pin_notice = Some((now, t("s_mu_sicherung_angelegt")));
                }
            }
        });
        if let Some(page) = open {
            self.page = page.to_string();
            self.usage_route = Some("sprung");
        }
    }

    /// A model of the window as in Verse-Kit 4.12: the overview alone on top,
    /// the menu filter, the folding groups that scroll on their own, Ko-fi and
    /// Discord fixed at the foot; at the right the page, below the status bar.
    /// Names come from the language file.
    fn window_frame(&mut self, ui: &mut egui::Ui, scheme: &Scheme) {
        use versekit_core::language::t;
        let height = self.frame_height;
        // The foot of the navigation: two rows of buttons and the community
        // mark, about four and a half lines tall like in 4.12.
        let line = ui.text_style_height(&egui::TextStyle::Body);
        let mark_wanted = line * 4.5;
        let mark_height = widgets::community_height(mark_wanted, ui.ctx().pixels_per_point());
        let button_height = line.max(widgets::BUTTON_ICON_SIZE) + 16.0;
        let foot = 2.0 * button_height + mark_height + 24.0;
        let filter = self.nav_filter.clone();
        let filtering = !filter.trim().is_empty();
        // A search is counted once when a word comes into the empty filter, never the word; emptied, the next
        // word is a new search.
        if !filtering {
            self.search_counted = (false, false);
        } else if !self.search_counted.0 {
            self.search_counted.0 = true;
            self.count_action("suche_benutzt");
        }
        // The way of a page opened from the menu: the search while the filter holds a word.
        let menu_route = if filtering { "suche" } else { "seitenleiste" };
        // A page that is no more (a stored last page, a pinned tile): its successor takes its place.
        self.follow_removed_page(ui.input(|i| i.time));
        // The mark goes once the area was open, whichever way it was reached.
        if self.new_pages.contains(&self.page.as_str()) {
            let page = self.page.clone();
            self.mark_seen(&page);
        }
        // A page whose module is switched off (a jump to it, a stored last page): the page "Module" opens
        // and says what is off, instead of a page without an entry in the menu.
        if self.module_hidden.iter().any(|hidden| *hidden == self.page) {
            if let Some(group) = versekit_core::modules::group_of(&self.page) {
                let said =
                    tf("s_mo_aus_hinweis", &[("0", Arg::from(t(versekit_core::modules::label(group)).as_str()))]);
                self.pin_notice = Some((ui.input(|i| i.time), said));
            }
            // A filter in the menu would hide the entry of the page that opens now.
            self.nav_filter.clear();
            self.page = "hf_module".to_string();
        }
        // A page that went away while it was open (disconnected, switched off):
        // the blueprint list takes its place.
        if self.page_hidden(&self.page) {
            self.page = "hf_liste".to_string();
        }
        // After the redirects above: the page the window really starts with counts.
        self.count_window_start();
        widgets::list_frame(ui, scheme, |ui| {
            ui.set_min_width(ui.available_width());
            self.head_bar(ui, scheme);
            let side = 220.0_f32.min(ui.available_width() * 0.45);
            let top = ui.cursor().min;
            let mut nav =
                ui.new_child(egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(top, egui::vec2(side, height))));
            let overview = t("hf_uebersicht");
            // The overview stays pinned on top, the filter only acts below it.
            if widgets::nav_entry(&mut nav, scheme, "layout-dashboard", &overview, self.page.is_empty(), None).clicked()
            {
                self.page.clear();
                self.usage_route = Some("seitenleiste");
            }
            let (top_page, top_icon) = TOP_PAGE;
            let top_name = t(top_page);
            if widgets::nav_entry(&mut nav, scheme, top_icon, &top_name, self.page == top_page, None).clicked() {
                self.page = top_page.to_string();
                self.usage_route = Some("seitenleiste");
            }
            let extension_found = self.extension_entry(&mut nav, scheme, &filter);
            nav.add_space(4.0);
            // The filter, at its right one button folds or opens all groups.
            nav.horizontal(|row| {
                row.spacing_mut().item_spacing.x = 4.0;
                let width = side - widgets::ICON_BUTTON_SIZE - 4.0;
                // Enter opens the first hit of this frame's filter and empties the field, Escape empties it.
                let field =
                    widgets::search_field_with_target(row, &mut self.nav_filter, &t("hf_menue_filtern"), scheme, width);
                self.nav_enter = field.entered.is_some();
                let (icon, hint) = if self.nav_folded {
                    ("chevrons-up-down", t("s_ke_alle_auf"))
                } else {
                    ("chevrons-down-up", t("s_ke_alle_zu"))
                };
                // Not a click on the way to a page for the usage counters: 4.12 has no such button.
                if widgets::icon_button(row, scheme, icon, &hint).clicked() {
                    self.nav_folded = !self.nav_folded;
                    for (group, _) in NAV {
                        widgets::set_nav_section_open(row.ctx(), &t(group), !self.nav_folded);
                    }
                }
            });
            nav.add_space(2.0);
            // The folded groups: handed to the menu at the start and after a change of the language (the menu keeps
            // them by the shown name), otherwise taken from it, with every click, the button for all and a jump.
            let ctx = nav.ctx().clone();
            if self.groups_language == Some(language::current()) {
                for ((group, _), closed) in GROUP_SETTINGS.iter().zip(self.groups_closed.iter_mut()) {
                    *closed = !widgets::nav_section_open(&ctx, &t(group));
                }
            } else {
                self.groups_language = Some(language::current());
                for ((group, _), closed) in GROUP_SETTINGS.iter().zip(self.groups_closed) {
                    if closed {
                        widgets::set_nav_section_open(&ctx, &t(group), false);
                    }
                }
            }
            let rest = (nav.available_height() - foot).max(60.0);
            let area = egui::ScrollArea::vertical().id_salt("navigation").max_height(rest).auto_shrink([false, false]);
            let real = self.real();
            widgets::scroll_area(&mut nav, scheme, area, |nav| {
                let mut found = extension_found;
                // Heads of the groups shown: (place in the order, rect).
                let mut heads: Vec<(usize, egui::Rect)> = Vec::new();
                for (place, &g) in self.group_order.clone().iter().enumerate() {
                    let (group, pages) = NAV[g];
                    // (page, icon, shown name): the page itself, or a setting or info on it.
                    let mut hits: Vec<(&str, &str, String)> = Vec::new();
                    for (page, icon) in pages.iter() {
                        if self.page_hidden(page) {
                            continue;
                        }
                        for shown in nav_hits_in(&filter, group, page, !real) {
                            hits.push((page, icon, shown));
                        }
                    }
                    if hits.is_empty() {
                        continue;
                    }
                    found = true;
                    // Enter in the filter: the first hit of the menu opens.
                    if self.nav_enter && filtering {
                        self.nav_enter = false;
                        self.page = hits[0].0.to_string();
                        self.usage_route = Some(menu_route);
                    }
                    let holds_page = hits.iter().any(|(page, ..)| self.page == *page);
                    // What the open page shows in this group: of several hits only one lights up.
                    let open_hits: Vec<String> =
                        hits.iter().filter(|(page, ..)| self.page == *page).map(|(.., shown)| shown.clone()).collect();
                    let section = widgets::nav_section(nav, scheme, &t(group), filtering, holds_page, |nav, full| {
                        for (page, icon, shown) in hits {
                            if !full && self.page != page {
                                continue;
                            }
                            let new_word = t("hf_neu").to_uppercase();
                            let new = self.new_pages.contains(&page).then_some(new_word.as_str());
                            let lit = self.page == page && nav_lit(&shown, &open_hits, &self.nav_hit);
                            if widgets::nav_entry(nav, scheme, icon, &shown, lit, new).clicked() {
                                self.page = page.to_string();
                                self.usage_route = Some(menu_route);
                                self.nav_hit = shown.clone();
                                // A setting found by the filter lights up on its page.
                                if let Some((_, setting)) = shown.split_once(" › ") {
                                    let now = nav.input(|i| i.time);
                                    self.highlight = Some((setting.to_string(), now));
                                    // The pairing settings lie in a fold of the hangar page: it opens.
                                    if page == "hf_hangar" && hangar_page::HangarState::is_pairing_setting(setting) {
                                        self.hangar.point_to_pairing();
                                    }
                                    // A section of a page found by the filter: the page scrolls to it.
                                    if page == "hf_hangar" && is_section_hit(page, setting) {
                                        self.hangar.show_planned();
                                    }
                                }
                                self.mark_seen(page);
                            }
                        }
                    });
                    // A group opened or closed by a click is a click on the way to a page.
                    if section.head.clicked() {
                        self.count_group_toggle();
                    }
                    let head = section.head.on_hover_text(t("s_ke_gruppe_h"));
                    // While filtering all groups are open: no moving then.
                    if !filtering && head.drag_started() {
                        self.dragging_group = Some(place);
                    }
                    heads.push((place, head.rect));
                }
                // A dragged group: its name follows the mouse, an accent line
                // shows where it lands, release puts it there.
                if let (Some(from), Some(pointer)) = (self.dragging_group, nav.ctx().pointer_interact_pos()) {
                    let before = heads
                        .iter()
                        .find(|(_, rect)| pointer.y < rect.center().y)
                        .map_or(self.group_order.len(), |(place, _)| *place);
                    let y = heads
                        .iter()
                        .find(|(place, _)| *place == before)
                        .map_or(nav.min_rect().bottom() + 2.0, |(_, rect)| rect.top() - 2.0);
                    let x = nav.min_rect().x_range();
                    nav.painter().hline(x, y, egui::Stroke::new(3.0, color(scheme.accent)));
                    let name = t(NAV[self.group_order[from]].0).to_uppercase();
                    egui::Area::new(egui::Id::unique("gezogene Gruppe"))
                        .order(egui::Order::Foreground)
                        .fixed_pos(pointer + egui::vec2(12.0, -10.0))
                        .interactable(false)
                        .show(nav.ctx(), |area| {
                            egui::Frame::new()
                                .fill(color(scheme.surface))
                                .stroke(egui::Stroke::new(2.0, color(scheme.accent)))
                                .corner_radius(crate::control_radius(scheme))
                                .inner_margin(egui::Margin::symmetric(10, 4))
                                .show(area, |area| area.label(RichText::new(name).small().strong()));
                        });
                    if !nav.input(|i| i.pointer.any_down()) {
                        widgets::move_before(&mut self.group_order, from, before);
                        self.dragging_group = None;
                    }
                }
                if !found {
                    nav.label(RichText::new(t("hf_menue_leer")).color(color(scheme.sub)));
                    // Nothing left in the menu: once per search.
                    if filtering && !self.search_counted.1 {
                        self.search_counted.1 = true;
                        self.count_action("suche_ohne_treffer");
                    }
                }
            });
            nav.add_space(4.0);
            // On every page, in the accent: the launcher, below it Discord and
            // Ko-fi sharing the width, then the community mark (4.12).
            // On real data only while a way to start the game is known (looked up in a thread of its own):
            // a button that does nothing is worse than none. The sample page shows it as an example.
            let found = self.store.as_ref().map(|store| self.start_look.poll(&store.paths, false));
            if found.as_ref().is_some_and(|f| !f.looked) {
                nav.ctx().request_repaint_after(std::time::Duration::from_millis(500));
            }
            if found.as_ref().is_none_or(|f| f.starter) {
                let start = t("s_sp_start_knopf");
                let launch = widgets::Button::new(&start, ButtonKind::Strong, scheme).icon("rocket").wide();
                if nav.add(launch).clicked() && self.loading.is_none() {
                    let now = nav.input(|i| i.time);
                    if let Some(store) = &self.store {
                        // The lookup of the way and the start run in a thread of their own.
                        self.pin_notice = Some((now, t("s_sp_start_lauft")));
                        let (spawner, done, paths) = (
                            std::sync::Arc::clone(&self.spawner),
                            self.start_done.0.clone(),
                            store.paths.working_copy(),
                        );
                        versekit_core::language::spawn_with_language(move || {
                            let _ = done.send(game_start::start_game(&paths, &spawner));
                        });
                    } else {
                        self.loading = Some((now, t("s_sp_start_lauft"), 3.0));
                        self.loading_update = false;
                    }
                }
            }
            nav.add_space(4.0);
            // Two links, as in 4.12: the notice that the browser opens, then the address.
            let (mut discord_clicked, mut donate_clicked) = (false, false);
            nav.columns(2, |halves| {
                let discord = t("hf_discord");
                discord_clicked = halves[0]
                    .add(widgets::Button::new(&discord, ButtonKind::Normal, scheme).icon("discord").wide())
                    .clicked();
                let donate = t("hf_kofi");
                donate_clicked = halves[1]
                    .add(widgets::Button::new(&donate, ButtonKind::Normal, scheme).icon("coffee").wide())
                    .clicked();
            });
            let now = nav.input(|i| i.time);
            if discord_clicked {
                self.follow_link(now, controls::DISCORD_URL, "hf_discord_auf");
            }
            if donate_clicked {
                self.follow_link(now, controls::KOFI_URL, "hf_kofi_auf");
            }
            nav.add_space(6.0);
            // The mark leads to Star Citizen with Xharig's referral code.
            let mark = nav.vertical_centered(|nav| widgets::community_mark(nav, scheme, mark_wanted)).inner;
            if mark.on_hover_text(t("s_ke_empfehlung")).clicked() {
                nav.ctx().open_url(egui::OpenUrl::new_tab(REFERRAL_URL));
            }
            let line_x = top.x + side + 6.0;
            ui.painter().vline(line_x, top.y..=top.y + height, egui::Stroke::new(1.0, color(scheme.line)));
            let page_rect = egui::Rect::from_min_max(
                egui::pos2(line_x + 8.0, top.y),
                egui::pos2(top.x + ui.available_width(), top.y + height),
            );
            let mut page = ui.new_child(egui::UiBuilder::new().max_rect(page_rect));
            let name = if self.page.is_empty() { overview.clone() } else { t(&self.page) };
            // Pages opened by the navigation or a tile go into the history;
            // the arrows (and Alt+←/→, the mouse side buttons) walk it.
            self.history.visit(&self.page);
            // The way of the page asked for counts once; a page the program changed by itself counts not.
            let route = self.usage_route.take();
            // A page entered anew starts without the filters it was left with.
            if self.shown_page != self.page {
                self.count_page(route);
                // The notes written but not yet saved go to the file when their page is left.
                if self.shown_page == "hf_notizen" {
                    self.memo_page.flush(None);
                }
                if self.page == "hf_herstellung" {
                    self.crafting.enter();
                }
                if self.page == "hf_contested" {
                    self.contested.enter();
                }
                // What the crafting page or the farm list put on the notes since shows in the list.
                if self.page == "hf_liste" {
                    self.blueprints.reload_noted();
                    // What came since (the scanner writes the collection beside the window) shows with its time.
                    if let Some(store) = &self.store {
                        self.blueprints.reload_have(&store.paths);
                    }
                }
                self.shown_page.clone_from(&self.page);
            }
            // A find the overlay wrote into the collection shows in the open list and progress, once a second.
            if self.store.is_some() && matches!(self.page.as_str(), "hf_liste" | "hf_fortschritt") {
                self.blueprints.follow_collection(page.input(|i| i.time));
                page.ctx().request_repaint_after(std::time::Duration::from_secs_f64(blueprint_list::COLLECTION_LOOK));
            }
            if self.update_waits() {
                let name = self.update_name();
                let version = [("version", Arg::from(name.as_str()))];
                let message = tf("s_ul_da", &version);
                let (update_word, news_word) = (t("s_ul_jetzt"), t("was_ist_neu"));
                // While a version is being fetched the button waits (on real data the fetching is the one in
                // the thread; the sample page times a stand-in).
                let blocked = (self.real() && self.updates.is_busy()).then(|| t("up_laeuft_schon"));
                let texts = settings::UpdateTexts {
                    message: &message,
                    update: &update_word,
                    news: &news_word,
                    update_blocked: blocked.as_deref(),
                };
                match settings::update_bar(&mut page, scheme, &texts) {
                    Some(settings::UpdateChoice::Update) if self.real() => {
                        let now = page.input(|i| i.time);
                        self.ask_update(now);
                    }
                    Some(settings::UpdateChoice::Update) if self.loading.is_none() => {
                        let loading = tf("s_ul_laedt", &version);
                        self.loading = Some((page.input(|i| i.time), loading, 5.0));
                        self.loading_update = true;
                    }
                    // "Was ist neu" leads to the gallery of what the version brings.
                    Some(settings::UpdateChoice::News) => {
                        self.page = "hf_wasistneu".to_string();
                        self.usage_route = Some("sprung");
                    }
                    _ => {}
                }
                page.add_space(4.0);
            }
            // The way back after a jump, above the head; gone once another page is open.
            if self.came_from.as_ref().is_some_and(|(_, to)| *to != self.page) {
                self.came_from = None;
            }
            if let Some((from, _)) = self.came_from.clone() {
                let from_name = if from.is_empty() { overview.clone() } else { t(&from) };
                let text = tf("hf_zurueck_seite", &[("0", Arg::from(from_name.as_str()))]);
                let button = widgets::Button::new(&text, ButtonKind::Normal, scheme).icon("arrow-left").wrapping();
                if page.add(button).clicked() {
                    // A step back when the page came right before, so forward leads to the jump again.
                    if self.history.previous() == Some(from.as_str()) {
                        let _ = self.history.back();
                    }
                    self.page = from;
                    self.usage_route = Some("sprung");
                    self.came_from = None;
                }
                page.add_space(4.0);
            }
            self.archive_notes(&mut page, scheme);
            let texts = widgets::HistoryTexts {
                back: &t("hf_seite_zurueck"),
                forward: &t("hf_seite_vor"),
                no_back: &t("hf_seite_zurueck_leer"),
                no_forward: &t("hf_seite_vor_leer"),
            };
            // The sync button of the page: next to the heading on the three pages that have an area, only
            // while the area is switched on and the connection stands; in the test mode with the switch on,
            // blocked, saying why.
            let sync_area = crate::head_sync::setting_of(&self.page)
                .filter(|_| self.real())
                .zip(self.connection.wire().cloned())
                .filter(|(setting, wire)| {
                    crate::head_sync::HeadSync::ready(wire, setting)
                        || crate::head_sync::HeadSync::locked(wire, setting)
                });
            // A part open on Shops: a step back leads to the list of results first.
            let inner = if self.page == "hf_laeden" && self.shops.detail_open() {
                Some(t("s_ld_zurueck_liste"))
            } else if self.page == SALVAGE_PAGE && self.salvage.can_go_back() {
                Some(t("s_zb_zurueck_wrack"))
            } else {
                None
            };
            page.horizontal(|head| {
                match widgets::history_buttons_inner(head, scheme, &mut self.history, &texts, inner.as_deref()) {
                    Some(widgets::HistoryStep::Page(target)) => {
                        self.page = target;
                        self.usage_route = Some("sprung");
                    }
                    Some(widgets::HistoryStep::Inner) if self.page == SALVAGE_PAGE => self.salvage.back_to_wreck(),
                    Some(widgets::HistoryStep::Inner) => self.shops.close_detail(),
                    None => {}
                }
                head.add_space(6.0);
                widgets::heading(head, scheme, &name);
                if self.page == "hf_staffellager" {
                    // The real age of the fetched storage, none before the first fetch.
                    if let Some(age) = self.org_stock.age() {
                        let at = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_or(0.0, |d| d.as_secs_f64())
                            - age;
                        let exact = versekit_core::formats::date_time(versekit_core::language::current(), at);
                        widgets::data_age(head, scheme, age, &exact);
                    }
                } else if let Some((age, exact)) = data_age_of(&self.page).filter(|_| !self.real()) {
                    widgets::data_age(head, scheme, age, &exact);
                }
                // The pin (like 4.12): pins the page to the overview or loosens it.
                let extra_page = self.extension.as_ref().is_some_and(|slot| slot.has(&self.page));
                // On real data only a page with a card of its own can be pinned (`cards.PAGES`).
                let pinnable = !self.real() || versekit_core::overview::page_name(&self.page).is_some();
                if !self.page.is_empty() && !extra_page && pinnable {
                    // On the overview: a tile of this page that is not hidden.
                    let index = self.tiles.iter().position(|tile| tile.key == self.page);
                    let pinned = index.is_some_and(|i| !self.tiles[i].hidden);
                    let hints = (t("k_nadel_anheften"), t("k_nadel_loesen"));
                    if widgets::pin_toggle(head, scheme, pinned, (&hints.0, &hints.1)).clicked() {
                        match index {
                            // A page pinned from here goes; a fixed tile is only hidden.
                            Some(i) if pinned && self.tiles[i].pinned => {
                                self.tiles.remove(i);
                            }
                            Some(i) => self.tiles[i].hidden = pinned,
                            None => self.tiles.push(tiles::Tile::pinned(&self.page)),
                        }
                        let said = t(if pinned { "k_geloest" } else { "k_angeheftet" });
                        self.pin_notice = Some((head.input(|i| i.time), said));
                        // The card shows its text at once, not after the next count.
                        self.overview = None;
                    }
                }
                // Resetting the filters stays at the top right, so it takes no line of the page.
                let narrowed = match self.page.as_str() {
                    "hf_liste" => self.blueprint_view.is_narrowed(),
                    "hf_auftragslog" => self.contract_view.is_narrowed(),
                    "hf_meinlager" => self.stock.is_narrowed(),
                    "hf_herstellung" => self.crafting.is_narrowed(),
                    "hf_bergbau" => self.mining.is_narrowed(),
                    "hf_staffellager" => self.org_stock.is_narrowed(),
                    "hf_routen" if self.real() => self.routes.is_narrowed(),
                    "hf_laeden" if self.real() => self.shops.is_narrowed(),
                    _ => false,
                };
                if narrowed || sync_area.is_some() {
                    head.with_layout(egui::Layout::right_to_left(egui::Align::Center), |right| {
                        // The sync button sits at the right edge and stays there; the filter reset, which comes
                        // and goes, stands to its left.
                        if let Some((setting, wire)) = &sync_area {
                            self.head_sync.button(right, scheme, wire, setting);
                        }
                        if narrowed {
                            let word = t("ff_zuruecksetzen");
                            if right.add(widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("x")).clicked() {
                                match self.page.as_str() {
                                    "hf_liste" => self.blueprint_view.reset(),
                                    "hf_meinlager" => self.stock.reset(),
                                    "hf_herstellung" => self.crafting.reset(),
                                    "hf_bergbau" => self.mining.reset(),
                                    "hf_routen" => self.routes.reset(),
                                    "hf_laeden" => self.shops.reset(),
                                    "hf_staffellager" => self.org_stock.reset(),
                                    _ => self.contract_view.reset(),
                                }
                            }
                        }
                    });
                }
            });
            // How the last pass ended, under the head: the texts are long and wrap.
            if let Some((setting, wire)) = &sync_area {
                self.head_sync.line(&mut page, scheme, wire, setting);
            }
            // What stays above the scrolling area comes first (4.12: the bar of the graphics page).
            if self.page == "hf_grafik"
                && self.real()
                && let Some(store) = &self.store
            {
                grafik_page::show_head(&mut page, scheme, &mut self.grafik, &store.paths);
            }
            // The overview: how the tiles are handled stays above them, only the tiles scroll.
            if self.page.is_empty() {
                page.label(RichText::new(t("s_ke_hinweis")).color(color(scheme.sub)));
            }
            // The page Achsen & Kurven keeps its foot ("write", "discard") below the scrolling area, always in reach.
            let foot = if self.page == "hf_achsen" && self.real() { self.axes.foot_height() } else { 0.0 };
            // Every page scrolls on its own below its heading.
            let rest = (page.available_height() - 4.0 - foot).max(60.0);
            let area = egui::ScrollArea::vertical()
                .id_salt(("seite", self.page.as_str()))
                .max_height(rest)
                .auto_shrink([false, false]);
            let mut jump: Option<(String, &str)> = None;
            // How the jump was asked for: a pinned card is `karte`, every other jump `sprung`.
            let mut jump_route = "sprung";
            // The widgets of the page frame the element the menu filter found.
            highlight::publish(page.ctx(), self.highlight.as_ref());
            let shown = widgets::scroll_area(&mut page, scheme, area, |page| {
                // Like `layout::page`: a little room so rounding cannot push a full-width row out.
                page.set_max_width(page.available_width() - layout::SAFETY);
                let blueprints_missing = self.blueprints.sample;
                match self.page.as_str() {
                    // No catalog on real data: say so, not that the page comes later.
                    "hf_liste" | "hf_fortschritt" if self.real() && self.blueprints_none => {
                        blueprint_list::no_catalog(page, scheme);
                    }
                    // The progress has nothing to count while the catalog is empty.
                    "hf_fortschritt" if self.real() && self.blueprints.catalog.blueprints.is_empty() => {
                        blueprint_list::empty_notice(page, scheme, &self.blueprints);
                    }
                    // The backup and reading in a stand are real; the rest of the page comes later.
                    "hf_sichern" if self.real() => {
                        widgets::card(page, scheme, |card| self.backup_section(card, scheme));
                        page.add_space(GAP);
                        let (tools, import) = (&mut self.collection_tools, &mut self.import);
                        let paths = self.store.as_ref().map(|store| &store.paths);
                        if let Some(paths) = paths {
                            widgets::card(page, scheme, |card| {
                                crate::collection_tools::show_export(card, scheme, tools, paths);
                            });
                            page.add_space(GAP);
                            widgets::card(page, scheme, |card| import_page::show(card, scheme, import, &paths.host));
                            page.add_space(GAP);
                        }
                        let mut setup = false;
                        widgets::card(page, scheme, |card| {
                            settings::setting_group(card, scheme, &t("s_si_protokolle"));
                            setup = Self::repeat_setup_line(card, scheme);
                            if let Some(paths) = paths {
                                // Right above the reset: reading again adds what is missing, resetting throws away.
                                crate::collection_tools::show_rescan(card, scheme, tools, paths);
                                crate::collection_tools::show_reset(card, scheme, tools, paths);
                            }
                        });
                        if setup {
                            self.open_setup();
                        }
                        if let Some(said) = self.collection_tools.take_notice() {
                            self.pin_fault = said.fault.then(|| said.text.clone());
                            self.pin_notice = Some((page.input(|i| i.time), said.text));
                        }
                    }
                    // On real data without blueprints the list and the progress say they come later.
                    name if self.real() && blueprints_missing && ["hf_liste", "hf_fortschritt"].contains(&name) => {
                        not_yet(page, scheme);
                    }
                    "" => {
                        // The overview: tiles the user arranges. The open button opens the
                        // page and folds its group, so only that page shows in it.
                        let loosen = t("k_nadel_loesen");
                        let (swap, size, hide, hidden) =
                            (t("s_ke_tausch"), t("s_ke_ecke"), t("s_ke_ausblenden"), t("s_ke_ausgeblendet"));
                        let keys = t("s_kt_tasten");
                        let sizes = ["s_ke_klein", "s_ke_breit", "s_ke_hoch", "s_ke_gross"].map(t);
                        let tile_texts = tiles::TileTexts {
                            unpin: &loosen,
                            swap: &swap,
                            size: &size,
                            hide: &hide,
                            sizes: [&sizes[0], &sizes[1], &sizes[2], &sizes[3]],
                            hidden: &hidden,
                            keys: &keys,
                        };
                        let real = self.overview.clone();
                        // A running refinery job counts its time down every second.
                        if real.as_ref().is_some_and(|o| o.ticking || o.hangar_ticking) {
                            page.ctx().request_repaint_after(std::time::Duration::from_secs_f64(glance::TICK_SECONDS));
                        }
                        let mut backup_now = false;
                        let mut refinery_now = false;
                        let mut translation_now = false;
                        let mut add = |tile: &mut egui::Ui, key: &str| {
                            let (target, _, action) = overview_tile_content(tile, scheme, key, real.as_ref());
                            if let Some(target) = target {
                                jump = Some(target);
                                if !versekit_core::overview::TILES.iter().any(|(id, _)| *id == key) {
                                    jump_route = "karte";
                                }
                            }
                            // "Back up now": the backup, not the page (the tile's own click is not taken then).
                            backup_now |= action == Some(glance::TileAction::BackupNow);
                            // "Scannen": Mein Lager with the refinery block open, and the reading starts.
                            refinery_now |= action == Some(glance::TileAction::RefineryScan);
                            translation_now |= action == Some(glance::TileAction::TranslationCheck);
                        };
                        let title = |key: &str| tile_of(key).1;
                        let mut opened: Option<String> = None;
                        if self.overview_two {
                            // Two areas like 4.12: what the program shows, and what
                            // the user pinned; the user picks which comes first.
                            let (mut mine, mut glance): (Vec<_>, Vec<_>) =
                                std::mem::take(&mut self.tiles).into_iter().partition(|tile| tile.pinned);
                            let mut overflow: f32 = 0.0;
                            for is_mine in [self.mine_first, !self.mine_first] {
                                let (name, list, id) = if is_mine {
                                    (t("k_bereich_deine"), &mut mine, "deine")
                                } else {
                                    (t("k_bereich_blick"), &mut glance, "blick")
                                };
                                settings::setting_group(page, scheme, &name);
                                if list.is_empty() {
                                    page.label(RichText::new(t("k_deine_leer")).color(color(scheme.sub)));
                                }
                                overflow =
                                    overflow.max(tiles::board(page, scheme, id, list, &tile_texts, &mut add, &title));
                                opened = opened.or_else(|| tiles::clicked_tile(page, id));
                                page.add_space(GAP);
                            }
                            self.tile_overflow = overflow;
                            self.tiles = glance;
                            self.tiles.extend(mine);
                        } else {
                            self.tile_overflow = tiles::board(
                                page,
                                scheme,
                                "uebersicht",
                                &mut self.tiles,
                                &tile_texts,
                                &mut add,
                                &title,
                            );
                            opened = tiles::clicked_tile(page, "uebersicht");
                        }
                        // The link "Back up now" of a tile makes the backup; it does not open the page behind the tile.
                        if backup_now {
                            opened = None;
                            self.open_backup_write(page.ctx());
                        }
                        if refinery_now {
                            opened = None;
                            jump = Some(("hf_meinlager".to_string(), "hf_gruppe_logistik"));
                            self.stock.read_refinery_screen(page.ctx());
                        }
                        // "Jetzt nach neuer Übersetzung sehen": the page of the translation, where the look runs.
                        if translation_now {
                            opened = None;
                            jump = Some(("hf_uebersetzung".to_string(), "hf_gruppe_spiel"));
                            self.game_texts.check_all();
                        }
                        // A click on a tile itself leads where its button leads.
                        if let Some(key) = opened {
                            let mut target = real
                                .as_ref()
                                .and_then(|_| versekit_core::overview::tile_target(&key))
                                .unwrap_or(key.as_str())
                                .to_string();
                            // Blueprints of another account did not count: the tile of the blueprints leads to the
                            // own account, and the row lights up.
                            if key == "bauplaene"
                                && let Some(store) = &self.store
                                && real.is_some()
                                && versekit_core::logsource::foreign_tally(&store.paths).blueprints > 0
                            {
                                target = "hf_allgemein".to_string();
                                self.highlight = Some((t("s_er_acc"), page.input(|i| i.time)));
                            }
                            // Else the blueprint tile with something received this week opens the list at that
                            // week (the progress page when nothing came).
                            else if key == "bauplaene"
                                && real.is_some()
                                && let Some(store) = &self.store
                            {
                                self.blueprints.reload_have(&store.paths);
                                let now = versekit_core::export::now_unix();
                                let came =
                                    versekit_core::overview::received(&glance_tiles::own_blueprints(&store.paths));
                                let period = versekit_core::overview::Period::Days7;
                                if versekit_core::overview::count_in(&came, period, now) > 0 {
                                    self.blueprint_view.jump(
                                        blueprint_list::ListJump::Received(period),
                                        t(blueprint_list::period_key(period)),
                                    );
                                    target = "hf_liste".to_string();
                                }
                            }
                            if let Some(group) = group_of(&target) {
                                jump = Some((target, group));
                                if !versekit_core::overview::TILES.iter().any(|(id, _)| *id == key) {
                                    jump_route = "karte";
                                }
                            }
                        }
                    }
                    "hf_allgemein" => {
                        let rows = settings_of("hf_allgemein");
                        let groups = group_sizes(rows);
                        // A setting that needs a restart: the bar at the top of the page.
                        if self.gpu != self.gpu_started && !self.restart_later {
                            let message = tf("s_ul_neustart_noetig", &[("name", Arg::from(label("s_ei_gpu")))]);
                            let (restart_word, later_word) = (t("s_ub_neustart"), t("hf_update_spaeter"));
                            let texts = settings::UpdateTexts {
                                message: &message,
                                update: &restart_word,
                                news: &later_word,
                                update_blocked: None,
                            };
                            match settings::restart_bar(page, scheme, &texts) {
                                Some(settings::UpdateChoice::Update) => {
                                    if self.real() {
                                        // The window starts again and closes; a refused start is said and the bar stays.
                                        if self.restart_now(page.ctx()) {
                                            self.gpu_started = self.gpu;
                                        }
                                    } else {
                                        self.gpu_started = self.gpu;
                                        let now = page.input(|i| i.time);
                                        self.pin_notice = Some((now, t("s_pf_startet_neu")));
                                    }
                                }
                                Some(settings::UpdateChoice::News) => self.restart_later = true,
                                None => {}
                            }
                            page.add_space(GAP);
                        }
                        let real = self.real();
                        // The own account comes first: which logs count decides what the collection holds.
                        if real && let Some(store) = &self.store {
                            detection_page::show_account(page, scheme, &mut self.detection, &store.paths);
                            if self.detection.take_all_changed() {
                                self.blueprints.follow_all_setting(&store.paths, &mut self.blueprint_view);
                            }
                            if let Some(said) = self.detection.take_notice() {
                                self.pin_fault = said.fault.then(|| said.text.clone());
                                self.pin_notice = Some((page.input(|i| i.time), said.text));
                            }
                            page.add_space(GAP);
                        }
                        // MangoHud is set up on this system? Looked up in a thread of its own.
                        let mango_installed =
                            self.store.as_ref().is_none_or(|s| self.start_look.poll(&s.paths, false).mangohud);
                        let mut mango_said: Option<bool> = None;
                        // The start with the system: does this system have one, and what the entry says now.
                        let autostart_possible = self.autostart.possible();
                        let mut autostart_wanted: Option<bool> = None;
                        let autostart_on = &mut self.autostart_on;
                        let (switches, sounds) = (&mut self.general_switches, &mut self.sounds);
                        let (english, volume) = (&mut self.general_english, &mut self.volume);
                        let language_auto = &mut self.language_auto;
                        let (gpu, later, mangohud) = (&mut self.gpu, &mut self.restart_later, &mut self.mangohud);
                        let clock_24 = &mut self.clock_24;
                        let mut clock_changed = false;
                        let streamer = &mut self.streamer;
                        let usage_on = &mut self.usage_on;
                        let mut streamer_changed = false;
                        let play_head = &mut self.play_head;
                        let mut play_head_changed = false;
                        let check_interval = &mut self.check_interval;
                        let mut interval_changed = false;
                        let desktop = &mut self.desktop;
                        let mut desktop_wish = None;
                        settings::settings_page(page, scheme, "allgemein", &group_refs(&groups), &mut |ui, g, r| {
                            let (name, hint, choices) = rows[g].1[r];
                            // The same row for every system: its name says Windows or the login.
                            let name =
                                label(if name == "autostart_win" && !cfg!(windows) { "autostart_linux" } else { name });
                            // Without an icon next to the clock (off Windows) closing ends the window: the hint says so.
                            let hint = match hint {
                                "s_tray_h" if !cfg!(windows) => "s_tray_h_ohne",
                                // The message of the system is a Windows message on Windows, a desktop one elsewhere.
                                "s_systemmeldung_h" if !cfg!(windows) => "s_systemmeldung_h_linux",
                                other => other,
                            };
                            let hint = label(hint);
                            let choices: Vec<String> = choices.iter().map(|c| label(c)).collect();
                            settings::setting_row(ui, scheme, &name, &hint, false, |row| {
                                match (g, r) {
                                    // Icon next to the clock, message of the system, menu entry: by system.
                                    (0, r) if r >= desktop_rows::FIRST_ROW => {
                                        if let Some(wish) = desktop.control(row, scheme, r - desktop_rows::FIRST_ROW) {
                                            desktop_wish = Some(wish);
                                        }
                                    }
                                    // The bridge to scmdb.net, in the group of what leaves the computer.
                                    (3, 2) => {
                                        if let Some(wish) = desktop.control(row, scheme, desktop_rows::SCMDB_ROW) {
                                            desktop_wish = Some(wish);
                                        }
                                    }
                                    // The start with the system is the entry itself, written at once.
                                    (0, 0) if real && !autostart_possible => {
                                        row.label(RichText::new(language::t("s_nicht_moegl")).color(color(scheme.sub)));
                                    }
                                    (0, 0) if real => {
                                        if row.add(widgets::Toggle::new(autostart_on, scheme)).changed() {
                                            autostart_wanted = Some(*autostart_on);
                                        }
                                    }
                                    // The clock of every time of day: at once, in the whole program.
                                    (1, 1) => {
                                        if row.add(widgets::Toggle::new(clock_24, scheme)).changed() {
                                            clock_changed = true;
                                        }
                                    }
                                    // MangoHud starts the game with an overlay: only where the game is started that way.
                                    (4, 0) if real && !cfg!(target_os = "linux") => {
                                        row.label(RichText::new(choices[0].as_str()).color(color(scheme.sub)));
                                    }
                                    (0, 1) => {
                                        if row.add(widgets::Toggle::new(gpu, scheme)).changed() {
                                            *later = false;
                                        }
                                    }
                                    // Not on this system: the reason instead of the switch.
                                    (4, 0) if cfg!(windows) => {
                                        row.label(RichText::new(choices[0].as_str()).color(color(scheme.sub)));
                                    }
                                    // Linux, real data, MangoHud not set up: the reason, a stored switch would do nothing.
                                    (4, 0) if real && !mango_installed => {
                                        row.label(RichText::new(language::t("s_mango_fehlt")).color(color(scheme.sub)));
                                    }
                                    (4, 0) => {
                                        if row.add(widgets::Toggle::new(mangohud, scheme)).changed() {
                                            mango_said = Some(*mangohud);
                                        }
                                    }
                                    // Seconds between two looks at Game.log; the watcher reads it before each look.
                                    (4, 1) => {
                                        let unit = choices[0].trim();
                                        if widgets::number_field(row, scheme, "pruefintervall", check_interval, 1..=60, unit)
                                        {
                                            interval_changed = true;
                                        }
                                    }
                                    // The play time in the head bar, at once.
                                    (1, 2) => {
                                        if row.add(widgets::Toggle::new(play_head, scheme)).changed() {
                                            play_head_changed = true;
                                        }
                                    }
                                    (1, 0) => {
                                        // The choice takes effect at once (the hint says so): the texts switch.
                                        let before = (*english, *language_auto);
                                        // The two languages with their flags on one line, as in 4.12, the language of the
                                        // system below them.
                                        let chosen = !*language_auto;
                                        let auto_word = language::t("e_sprache_auto");
                                        row.vertical(|column| {
                                            column.horizontal(|flags| {
                                                if widgets::radio_flag(flags, scheme, chosen && !*english, &choices[1], "de")
                                                    .clicked()
                                                {
                                                    *english = false;
                                                    *language_auto = false;
                                                }
                                                if widgets::radio_flag(flags, scheme, chosen && *english, &choices[0], "gb")
                                                    .clicked()
                                                {
                                                    *english = true;
                                                    *language_auto = false;
                                                }
                                            });
                                            if widgets::radio(column, scheme, !chosen, &auto_word).clicked() {
                                                *language_auto = true;
                                            }
                                        });
                                        if (*english, *language_auto) != before {
                                            let choice = match (*language_auto, *english) {
                                                (true, _) => language::Choice::Auto,
                                                (false, true) => language::Choice::En,
                                                (false, false) => language::Choice::De,
                                            };
                                            language::set_language(choice);
                                            // Following the system, the texts are in its language.
                                            if *language_auto {
                                                *english = language::current() == language::Lang::En;
                                            }
                                            row.ctx().request_repaint();
                                        }
                                    }
                                    (2, 2) => {
                                        settings::slider_with_value(row, scheme, &name, volume, (0, 100), false, |n| {
                                            format!("{n} %")
                                        });
                                    }
                                    (2, k) => {
                                        row.add(widgets::Toggle::new(&mut sounds[k], scheme));
                                    }
                                    // Errors are always written down: no switch, only the line that says so.
                                    (3, 0) => {}
                                    // The daily usage report: stored at the end of the frame with the other settings.
                                    (3, 3) => {
                                        row.add(widgets::Toggle::new(usage_on, scheme));
                                    }
                                    // Streamer mode: takes effect at once, in the whole program.
                                    (3, 1) => {
                                        if row.add(widgets::Toggle::new(streamer, scheme)).changed() {
                                            streamer_changed = true;
                                        }
                                    }
                                    (g, r) => {
                                        // Start with the system: 0, the 24-hour clock: 2.
                                        let k = match g {
                                            0 => r,
                                            1 => 2,
                                            _ => 3 + r,
                                        };
                                        row.add(widgets::Toggle::new(&mut switches[k], scheme));
                                    }
                                }
                            });
                        });
                        if clock_changed {
                            versekit_core::formats::set_clock_24(self.clock_24);
                            page.ctx().request_repaint();
                        }
                        if play_head_changed && let Some(store) = &self.store {
                            store.paths.set_setting(
                                versekit_core::overview::PLAYTIME_HEAD_SETTING,
                                Value::Bool(self.play_head),
                            );
                        }
                        // Written with the other settings at the end of the frame.
                        if interval_changed {
                            let said = tf("s_takt_sagen", &[("0", Arg::from(self.check_interval))]);
                            self.pin_notice = Some((page.input(|i| i.time), said));
                        }
                        if streamer_changed && let Some(store) = &self.store {
                            self.apply_streamer(&store.paths);
                        } else if streamer_changed {
                            versekit_core::streamer::set_active(self.streamer);
                        }
                        if let Some(wanted) = autostart_wanted {
                            let said = self.change_autostart(wanted);
                            self.pin_notice = Some((page.input(|i| i.time), said));
                        }
                        if let Some(wish) = desktop_wish {
                            let host = self.store.as_ref().map(|s| &s.paths.host);
                            if let Some(said) = self.desktop.apply(wish, host) {
                                self.pin_notice = Some((page.input(|i| i.time), said));
                            }
                        }
                        // The switch of MangoHud says what it does at the next start (4.12).
                        if let Some(on) = mango_said {
                            let word = t(if on { "e_an" } else { "e_aus" });
                            let said = tf("s_mango_sagen", &[("0", Arg::from(word.as_str()))]);
                            self.pin_notice = Some((page.input(|i| i.time), said));
                        }
                    }
                    "hf_ueber" if self.real() => {
                        if let Some(out) = self.show_about(page, scheme) {
                            if out.copy {
                                page.ctx().copy_text(format!("{} {}", t("hf_titel"), self.updates.own_version()));
                                self.copied_at = Some(page.input(|i| i.time));
                            }
                            if out.site {
                                page.ctx().open_url(egui::OpenUrl::new_tab("https://versekit.xharig.com"));
                            }
                            if out.setup {
                                self.open_setup();
                            }
                            // The gallery has no row in the menu: it is jumped to without a group, as from the
                            // update bar.
                            if out.news {
                                jump = Some(("hf_wasistneu".to_string(), ""));
                            }
                            if let Some(on) = out.channel {
                                self.update_test = on;
                                let word = t(if on { "e_an" } else { "e_aus" });
                                let said = format!("{}: {word}", t("e_vorab"));
                                self.pin_notice = Some((page.input(|i| i.time), said));
                                if let Some(store) = &self.store {
                                    let now = page.input(|i| i.time);
                                    self.updates.refresh(page.ctx(), &store.paths, now, true, on);
                                }
                            }
                            if let Some(with_prerelease) = out.fetch
                                && let Some(store) = &self.store
                                && let Err(why) = self.updates.start(&store.paths, with_prerelease)
                            {
                                self.pin_notice = Some((page.input(|i| i.time), why));
                            }
                        }
                    }
                    "hf_ueber" => {
                        let infos = page_infos("hf_ueber");
                        // The buttons and the link in a card, as on the page with own data.
                        widgets::card(page, scheme, |page| {
                            page.set_width(page.available_width());
                            widgets::card_title(page, scheme, &t("hf_update"));
                            page.horizontal_wrapped(|row| {
                                let button =
                                    widgets::Button::new(&infos[0], ButtonKind::Strong, scheme).icon("download");
                                if row.add(button).clicked() && self.loading.is_none() && self.update_waits() {
                                    let version = [("version", Arg::from(self.update_version.as_str()))];
                                    let loading = tf("s_ul_laedt", &version);
                                    self.loading = Some((row.input(|i| i.time), loading, 5.0));
                                    self.loading_update = true;
                                }
                                // Copied: the notice at the top right.
                                let copy = widgets::Button::new(&infos[1], ButtonKind::Normal, scheme).icon("copy");
                                if row.add(copy).clicked() {
                                    row.ctx().copy_text(format!("{} 5.0.0", t("hf_titel")));
                                    self.copied_at = Some(row.input(|i| i.time));
                                }
                                // The way back into the assistant, as in 4.12 beside the buttons of the page.
                                let label = t("s_ub_einrichtung");
                                let again = widgets::Button::new(&label, ButtonKind::Normal, scheme);
                                if row.add(again.icon("wand-sparkles")).clicked() {
                                    self.open_setup();
                                }
                            });
                            page.add_space(4.0);
                            // A link out of Verse-Kit carries the symbol.
                            if widgets::external_link(page, scheme, &infos[2]).clicked() {
                                page.ctx().open_url(egui::OpenUrl::new_tab("https://versekit.xharig.com"));
                            }
                        });
                        page.add_space(GAP);
                        let rows = settings_of("hf_ueber");
                        let groups = group_sizes(rows);
                        let test = &mut self.update_test;
                        settings::settings_page(page, scheme, "ueber", &group_refs(&groups), &mut |ui, g, r| {
                            let (name, hint, choices) = rows[g].1[r];
                            let name = label(name);
                            let hint = label(hint);
                            let choices: Vec<String> = choices.iter().map(|c| label(c)).collect();
                            let found_by_name = highlight::pointed(ui, &[&name]);
                            settings::setting_row(ui, scheme, &name, &hint, found_by_name, |row| {
                                // Updating has no switch: only test versions yes or no.
                                if g == 0 {
                                    if widgets::radio(row, scheme, *test, &choices[0]).clicked() {
                                        *test = true;
                                    }
                                    if widgets::radio(row, scheme, !*test, &choices[1]).clicked() {
                                        *test = false;
                                    }
                                }
                            });
                        });
                    }
                    "hf_overlay" => {
                        let rows = settings_of("hf_overlay");
                        let groups = group_sizes(rows);
                        let choices: Vec<String> = rows[1].1[0].2.iter().map(|c| label(c)).collect();
                        let texts = settings::OverlayTexts {
                            corners: [&choices[0], &choices[1], &choices[2], &choices[3]],
                            free: &choices[4],
                            bar_top: &choices[5],
                            bar_bottom: &choices[6],
                        };
                        let (clickable, place, bar) =
                            (&mut self.overlay_clickable, &mut self.overlay_place, &mut self.bar_side);
                        let lines = &mut self.overlay_rows;
                        let (modifiers, key) = (&mut self.hotkey_modifiers, &mut self.hotkey_key);
                        let hotkey_before = hotkey_text(*modifiers, *key);
                        let (scan_modifiers, scan_key, scan_on) =
                            (&mut self.scan_modifiers, &mut self.scan_key, &mut self.scan_hotkey_on);
                        let scan_before = hotkey_text(*scan_modifiers, *scan_key);
                        let mut scan_refused = false;
                        let (search_modifiers, search_key, search_on) =
                            (&mut self.search_modifiers, &mut self.search_key, &mut self.search_hotkey_on);
                        let search_before = search_on.then(|| hotkey_text(*search_modifiers, *search_key)).flatten();
                        let mut search_refused = false;
                        let (picture_modifiers, picture_key, picture_on) =
                            (&mut self.picture_modifiers, &mut self.picture_key, &mut self.picture_hotkey_on);
                        let picture_before =
                            picture_on.then(|| hotkey_text(*picture_modifiers, *picture_key)).flatten();
                        let mut picture_refused = false;
                        let (main_modifiers, main_key, main_on) =
                            (&mut self.main_modifiers, &mut self.main_key, &mut self.main_hotkey_on);
                        let main_before = main_on.then(|| hotkey_text(*main_modifiers, *main_key)).flatten();
                        let mut main_refused = false;
                        // The refinery scan reads the screen: only where that works.
                        let scan_possible = versekit_core::refinery_scan::screen::supported();
                        // The picture of the game grabs the screen: only where that works.
                        let picture_possible = versekit_core::screen_grab::supported();
                        let (auto, opacity) = (&mut self.overlay_auto, &mut self.overlay_opacity);
                        let opaque = self.overlay_opaque;
                        let (popup, seconds) = (&mut self.overlay_popup, &mut self.overlay_seconds);
                        let events = &mut self.overlay_events;
                        let sections = &mut self.overlay_sections;
                        let contracts_max = &mut self.overlay_contracts_max;
                        let fps = &mut self.overlay_fps;
                        let server_fps = &mut self.overlay_server_fps;
                        let players = &mut self.overlay_players;
                        let info_bar = &mut self.overlay_info_bar;
                        let info_bar_always = &mut self.overlay_info_bar_always;
                        let overlay_always = &mut self.overlay_always;
                        let info_bar_corner = &mut self.overlay_info_bar_corner;
                        let (overlay_size, info_bar_size) = (&mut self.overlay_size, &mut self.overlay_info_bar_size);
                        let handles = &mut self.handles;
                        let scanner = &mut self.mining.scanner;
                        let mut reset = false;
                        let mut hotkey_refused = false;
                        // A key another shortcut holds: the row that holds it.
                        let mut hotkey_taken: Option<usize> = None;
                        // Switched off in the settings file, the overlay's own hotkey holds no key either.
                        let overlay_on = self.hotkey_on;
                        let (desktop, mut shortcut_copied) = (&self.desktop, false);
                        settings::settings_page_wide(
                            page,
                            scheme,
                            "overlay",
                            &group_refs(&groups),
                            // Cards of rows in the columns of cards; the signature window, an editor on pictures of
                            // the game, over the whole width under them.
                            &[8],
                            &mut |ui, g, r| {
                                let (name, hint, choices) = rows[g].1[r];
                                let name = label(name);
                                // The opacity of an overlay drawn without transparency says why it has no effect.
                                let hint = if (g, r, opaque) == (0, 5, true) { "ov_s_deck_ohne_h" } else { hint };
                                let hint = label(hint);
                                let hint = hint.as_str();
                                if (g, r) == (0, 0) {
                                    // Comes and folds with the game, no switch: a small heading over the two switches
                                    // that change it, its hint on hover.
                                    subheading(ui, scheme, &name, hint);
                                } else if g == 0 {
                                    // What the overlay shows and how: one line each, the hint on hover.
                                    settings::setting_line(ui, scheme, &name, hint, false, |row| match r {
                                        1 => {
                                            row.add(widgets::Toggle::new(overlay_always, scheme));
                                        }
                                        2 => {
                                            row.add(widgets::Toggle::new(clickable, scheme));
                                        }
                                        3 => {
                                            row.add(widgets::Toggle::new(auto, scheme));
                                        }
                                        // A number: − field + with its unit.
                                        4 => {
                                            widgets::number_field(
                                                row,
                                                scheme,
                                                "overlay-zeilen",
                                                lines,
                                                OVERLAY_ROWS.0..=OVERLAY_ROWS.1,
                                                &label(choices[0]),
                                            );
                                        }
                                        5 => {
                                            settings::slider_with_value(row, scheme, &name, opacity, (30, 100), false, |n| {
                                                format!("{n} %")
                                            });
                                        }
                                        _ => window_size_slider(row, scheme, &name, overlay_size),
                                    });
                                } else if g == 2 && r == 0 {
                                    // Always, or only on news.
                                    settings::setting_line(ui, scheme, &name, hint, false, |row| {
                                        if widgets::radio(row, scheme, !*popup, &label(choices[0])).clicked() {
                                            *popup = false;
                                        }
                                        if widgets::radio(row, scheme, *popup, &label(choices[1])).clicked() {
                                            *popup = true;
                                        }
                                    });
                                } else if g == 2 {
                                    // The seconds belong to „only on news“: grey while the overlay is always there.
                                    let on_news = *popup;
                                    settings::setting_line(ui, scheme, &name, hint, false, |row| {
                                        row.add_enabled_ui(on_news, |row| {
                                            widgets::number_field(
                                                row,
                                                scheme,
                                                "overlay-sekunden",
                                                seconds,
                                                2..=60,
                                                &label(choices[0]),
                                            );
                                        });
                                    });
                                } else if g == 3 {
                                    // A shortcut in one line: its switch (the overlay's own is always on, its switch
                                    // stands on and grey), its name, the modifier keys and the key.
                                    let names: Vec<String> = choices.iter().map(|w| label(w)).collect();
                                    let keys = hotkey_names();
                                    let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
                                    // The five as the overlay registers them, before this frame's change.
                                    let rows_before = [
                                        (*modifiers, *key, overlay_on),
                                        (*scan_modifiers, *scan_key, *scan_on && scan_possible),
                                        (*search_modifiers, *search_key, *search_on),
                                        (*picture_modifiers, *picture_key, *picture_on && picture_possible),
                                        (*main_modifiers, *main_key, *main_on),
                                    ];
                                    let (switch, held, chosen, id): (Option<&mut bool>, &mut [bool; 3], &mut usize, &str) =
                                        match r {
                                            0 => (None, &mut *modifiers, &mut *key, "overlay"),
                                            1 => (Some(&mut *scan_on), &mut *scan_modifiers, &mut *scan_key, "overlay-scan"),
                                            2 => (Some(&mut *search_on), &mut *search_modifiers, &mut *search_key, "overlay-suche"),
                                            3 => (Some(&mut *picture_on), &mut *picture_modifiers, &mut *picture_key, "overlay-bild"),
                                            _ => (Some(&mut *main_on), &mut *main_modifiers, &mut *main_key, "overlay-haupt"),
                                        };
                                    // The refinery scan and the picture of the game read the screen: only where that
                                    // works.
                                    let possible = match r {
                                        1 => scan_possible,
                                        3 => picture_possible,
                                        _ => true,
                                    };
                                    let before = (*held, *chosen);
                                    // The keys only while the shortcut is on.
                                    let switch = possible.then_some(switch).flatten();
                                    shortcut_line(ui, scheme, &name, hint, switch, r == 0, |line| {
                                        if !possible {
                                            let reason = language::t("s_nicht_moegl");
                                            line.label(RichText::new(reason).color(color(scheme.sub)));
                                            return;
                                        }
                                        let mut words: Vec<(&str, bool)> =
                                            names.iter().zip(held.iter()).map(|(w, on)| (w.as_str(), *on)).collect();
                                        widgets::hotkey_picker(line, scheme, id, &mut words, &keys, chosen, &name);
                                        for (on, (_, pressed)) in held.iter_mut().zip(words) {
                                            *on = pressed;
                                        }
                                    });
                                    // A letter or a digit without Strg, Alt or Umschalt is no hotkey: the saved one
                                    // stays, and the reason is said below the page. F13 by itself is one (Kuehlwagen).
                                    let refused = keep_hotkey(held, chosen, before);
                                    match r {
                                        0 => hotkey_refused |= refused,
                                        1 => scan_refused |= refused,
                                        2 => search_refused |= refused,
                                        3 => picture_refused |= refused,
                                        _ => main_refused |= refused,
                                    }
                                    // A key another shortcut that is on holds already: the change goes back (a switch
                                    // turned on goes off again), and the reason is said below the page. One press must
                                    // not do two things.
                                    let rows_now = [
                                        (*modifiers, *key, overlay_on),
                                        (*scan_modifiers, *scan_key, *scan_on && scan_possible),
                                        (*search_modifiers, *search_key, *search_on),
                                        (*picture_modifiers, *picture_key, *picture_on && picture_possible),
                                        (*main_modifiers, *main_key, *main_on),
                                    ];
                                    let changed = (rows_now[r].0, rows_now[r].1) != before;
                                    let turned_on = rows_now[r].2 && !rows_before[r].2;
                                    let holder = any_holder(r, &active_hotkeys(rows_now));
                                    if (changed || turned_on)
                                        && let Some(by) = holder
                                    {
                                        let (held, chosen, on) = match r {
                                            0 => (&mut *modifiers, &mut *key, None),
                                            1 => (&mut *scan_modifiers, &mut *scan_key, Some(&mut *scan_on)),
                                            2 => (&mut *search_modifiers, &mut *search_key, Some(&mut *search_on)),
                                            3 => (&mut *picture_modifiers, &mut *picture_key, Some(&mut *picture_on)),
                                            _ => (&mut *main_modifiers, &mut *main_key, Some(&mut *main_on)),
                                        };
                                        if changed {
                                            (*held, *chosen) = before;
                                        } else if let Some(on) = on {
                                            *on = false;
                                        }
                                        hotkey_taken = Some(by);
                                    } else if let Some(by) = earlier_holder(r, &active_hotkeys(rows_now)) {
                                        // Read so from the settings: not registered, and the line says by which.
                                        let by = language::t(versekit_core::overlay::Shortcut::ALL[by].label_key());
                                        let text = language::tf("s_hk_belegt_durch", &[("0", language::Arg::from(by))]);
                                        ui.horizontal(|line| {
                                            line.add_space(widgets::TOGGLE_SIZE.x + line.spacing().item_spacing.x);
                                            line.add(egui::Label::new(RichText::new(text).color(color(scheme.yellow))).wrap());
                                        });
                                    }
                                    if r == 4 {
                                        // Under Wayland: the way through a shortcut of the desktop, whose command
                                        // shows the main window of a running program.
                                        shortcut_copied |= desktop.shortcut_hint(ui, scheme);
                                    }
                                } else if g == 7 {
                                    // The settings of the handle search, which shows in the bar: what they are for
                                    // as the small heading, then their rows.
                                    subheading(ui, scheme, hint, "");
                                    crate::handle_page::show_settings(ui, scheme, handles);
                                } else if g == 8 {
                                    // The window with the scanned material: its place and the search frame on two
                                    // models side by side or one below the other (the frame has no row of its own), its
                                    // size and the extra line in the overlay.
                                    if r == 0 {
                                        // The hotkey of the picture as the hint under its button names it, while it is
                                        // on and can be had.
                                        scanner.hotkey = (*picture_on && picture_possible)
                                            .then(|| hotkey_text(*picture_modifiers, *picture_key))
                                            .flatten()
                                            .map(|text| glance_tiles::hotkey_in(language::current(), &text));
                                        let frame_row = rows[g].1[3];
                                        let (frame_name, frame_hint) = (label(frame_row.0), label(frame_row.1));
                                        scanner_ui::model_cards(
                                            ui,
                                            scheme,
                                            scanner,
                                            [&name, hint, &label(choices[0])],
                                            [&frame_name, &frame_hint, &label(frame_row.2[0])],
                                        );
                                    } else if r != 3 {
                                        settings::setting_row(ui, scheme, &name, hint, false, |row| match r {
                                            // The size in per mille of the game window's height.
                                            1 => {
                                                let mut size = i64::from(scanner.layout.size.0);
                                                let range = (i64::from(Size::MIN.0), i64::from(Size::MAX.0));
                                                settings::slider_with_value(row, scheme, &name, &mut size, range, false, |n| {
                                                    format!("{n} ‰")
                                                });
                                                scanner.layout.size = Size(size as i32);
                                            }
                                            2 => {
                                                row.add(widgets::Toggle::new(&mut scanner.layout.overlay_line, scheme));
                                            }
                                            // Above the signature or in the info bar.
                                            4 => {
                                                for (place, word) in Place::ALL.into_iter().zip(choices) {
                                                    if widgets::radio(row, scheme, scanner.layout.place == place, &label(word))
                                                        .clicked()
                                                    {
                                                        scanner.layout.place = place;
                                                    }
                                                }
                                            }
                                            5 => {
                                                let range = versekit_core::signature_window::LINGER_RANGE;
                                                let linger = &mut scanner.layout.linger;
                                                settings::slider_with_value(row, scheme, &name, linger, range, false, |n| {
                                                    format!("{n} s")
                                                });
                                            }
                                            // The seconds between "Spielbild holen" and the picture.
                                            6 if !picture_possible => {
                                                let reason = language::t("s_nicht_moegl");
                                                row.label(RichText::new(reason).color(color(scheme.sub)));
                                            }
                                            6 => {
                                                for seconds in versekit_core::game_picture::DELAYS {
                                                    let chosen = scanner.delay == seconds;
                                                    if widgets::radio(row, scheme, chosen, &format!("{seconds} s")).clicked()
                                                    {
                                                        scanner.delay = seconds;
                                                    }
                                                }
                                            }
                                            _ => {}
                                        });
                                    }
                                } else if g == 4 {
                                    // The events that open the overlay: one switch each.
                                    settings::setting_row(ui, scheme, &name, hint, false, |row| {
                                        let event = versekit_core::overlay::Event::ALL[r];
                                        let mut on = events.contains(event);
                                        if row.add(widgets::Toggle::new(&mut on, scheme)).changed() {
                                            *events = events.with(event, on);
                                        }
                                    });
                                } else if g == 5 && r == versekit_core::overlay::Section::ALL.len() {
                                    // After the switches: the most contracts the game allows, for the head of the bar.
                                    settings::setting_row(ui, scheme, &name, hint, false, |row| {
                                        let range = versekit_core::overlay::CONTRACTS_MAX_RANGE;
                                        let id = "auftraege-hoechstzahl";
                                        widgets::number_field(row, scheme, id, contracts_max, range.0..=range.1, "");
                                    });
                                } else if g == 5 {
                                    // What the open overlay shows: one switch each.
                                    settings::setting_row(ui, scheme, &name, hint, false, |row| {
                                        let section = versekit_core::overlay::Section::ALL[r];
                                        let mut on = sections.contains(section);
                                        if row.add(widgets::Toggle::new(&mut on, scheme)).changed() {
                                            *sections = sections.with(section, on);
                                        }
                                    });
                                } else if g == 6 && r == 3 {
                                    // The size of the bar with its card and lines, apart from the overlay's.
                                    settings::setting_row(ui, scheme, &name, hint, false, |row| {
                                        window_size_slider(row, scheme, &name, info_bar_size);
                                    });
                                } else if g == 6 && r == 2 {
                                    // Where the bar starts: top left, middle or right.
                                    settings::setting_row(ui, scheme, &name, hint, false, |row| {
                                        let corners = versekit_core::info_bar::Corner::ALL;
                                        for (corner, word) in corners.into_iter().zip(choices) {
                                            if widgets::radio(row, scheme, *info_bar_corner == corner, &label(word))
                                                .clicked()
                                            {
                                                *info_bar_corner = corner;
                                            }
                                        }
                                    });
                                } else if g == 6 {
                                    // The bar itself, whether it shows without the game too, and the frame rates
                                    // (in the bar while it is on, else beside the shard in the overlay).
                                    let switch = match r {
                                        0 => &mut *info_bar,
                                        1 => &mut *info_bar_always,
                                        4 => &mut *fps,
                                        5 => &mut *server_fps,
                                        _ => &mut *players,
                                    };
                                    settings::setting_row(ui, scheme, &name, hint, false, |row| {
                                        row.add(widgets::Toggle::new(switch, scheme));
                                    });
                                } else if r == 0 {
                                    // The place: its name as the small heading (a row without a control is one), under
                                    // it the field of the four corners with free movement in the middle row and the bar
                                    // beside it.
                                    subheading(ui, scheme, &name, hint);
                                    // The field right under its heading, at the left of the card: no space before it.
                                    settings::overlay_place(ui, scheme, place, bar, &texts);
                                    ui.add_space(6.0);
                                } else {
                                    // Forget the place: back to the middle, free, bar on top.
                                    settings::setting_line(ui, scheme, &name, hint, false, |row| {
                                        if row
                                            .add(widgets::Button::new(&label(choices[0]), ButtonKind::Normal, scheme))
                                            .clicked()
                                        {
                                            reset = true;
                                        }
                                    });
                                }
                            },
                        );
                        if shortcut_copied {
                            self.copied_at = Some(page.input(|i| i.time));
                        }
                        // A letter or a digit left without a modifier: why, and which combination stays.
                        for (refused, modifiers, key) in [
                            (hotkey_refused, self.hotkey_modifiers, self.hotkey_key),
                            (scan_refused, self.scan_modifiers, self.scan_key),
                            (search_refused, self.search_modifiers, self.search_key),
                            (picture_refused, self.picture_modifiers, self.picture_key),
                            (main_refused, self.main_modifiers, self.main_key),
                        ] {
                            if refused {
                                let saved = hotkey_text(modifiers, key).unwrap_or_default();
                                let text = language::tf("s_bs_hotkey_nein", &[("0", language::Arg::from(saved))]);
                                self.pin_notice = Some((page.input(|i| i.time), text));
                            }
                        }
                        // A key another shortcut holds: which one, and that the key stays as it was.
                        if let Some(by) = hotkey_taken {
                            let rows = [
                                (self.hotkey_modifiers, self.hotkey_key),
                                (self.scan_modifiers, self.scan_key),
                                (self.search_modifiers, self.search_key),
                                (self.picture_modifiers, self.picture_key),
                                (self.main_modifiers, self.main_key),
                            ];
                            let (modifiers, key) = rows[by];
                            let combination = hotkey_text(modifiers, key).unwrap_or_default();
                            let by = language::t(versekit_core::overlay::Shortcut::ALL[by].label_key());
                            let text = language::tf(
                                "s_hk_doppelt",
                                &[("0", language::Arg::from(combination)), ("1", language::Arg::from(by))],
                            );
                            self.pin_notice = Some((page.input(|i| i.time), text));
                        }
                        // A new hotkey: the running overlay takes it with the settings (it registers all of them). The
                        // ones of the handle search and of the picture count only while they are on, so switching
                        // one on says it too.
                        let search_now = self
                            .search_hotkey_on
                            .then(|| hotkey_text(self.search_modifiers, self.search_key))
                            .flatten();
                        let picture_now = self
                            .picture_hotkey_on
                            .then(|| hotkey_text(self.picture_modifiers, self.picture_key))
                            .flatten();
                        let main_now =
                            self.main_hotkey_on.then(|| hotkey_text(self.main_modifiers, self.main_key)).flatten();
                        let now_hotkeys = [
                            hotkey_text(self.hotkey_modifiers, self.hotkey_key),
                            hotkey_text(self.scan_modifiers, self.scan_key),
                            search_now,
                            picture_now,
                            main_now,
                        ];
                        let before_hotkeys = [hotkey_before, scan_before, search_before, picture_before, main_before];
                        for (before, now) in before_hotkeys.into_iter().zip(now_hotkeys) {
                            if let Some(now) = now.filter(|now| before.as_ref() != Some(now))
                                && (self.overlay_runs)()
                            {
                                let said = language::tf("s_hk_ok", &[("0", language::Arg::from(now))]);
                                self.pin_notice = Some((page.input(|i| i.time), said));
                            }
                        }
                        // Everything back (4.12 `reset_position`): the middle of the main screen in the default
                        // size, the corner freed and the bar on top, or a fixed corner would send it back to where
                        // it could not be reached. The settings go with the others at the end of the frame.
                        if reset {
                            self.overlay_place = settings::OverlayPlace::Free;
                            self.bar_side = settings::BarSide::Top;
                            let mut waits_hidden = false;
                            if let Some(store) = &self.store {
                                versekit_core::overlay::forget_place(&store.paths);
                                // Without the game the overlay waits hidden: brought back, it still cannot be
                                // seen, so the button says why and how to show it.
                                // The game state the foot bar looked at last, while it is fresh; else asked anew.
                                let now = page.input(|i| i.time);
                                let running = match self.game_seen {
                                    Some((at, running)) if now - at < GAME_LOOK_SECONDS => running,
                                    _ => game_running_for_foot(&store.paths, (self.game_process)(&store.paths)),
                                };
                                waits_hidden = !self.overlay_always && !running;
                            }
                            self.place_reset_at = Some(page.ctx().input(|i| i.time));
                            self.place_reset_waits = waits_hidden;
                        }
                    }
                    "hf_darstellung" => {
                        let rows = settings_of("hf_darstellung");
                        let mut groups = group_sizes(rows);
                        // The on-top choice only counts with two areas.
                        if !self.overview_two {
                            groups[2].1 = 1;
                        }
                        let basetool: Vec<usize> = (0..NAV.len()).collect();
                        let same = t("s_ei_steht_schon");
                        let already = (self.group_order == basetool).then_some(same.as_str());
                        let (order, two, mine_first, size) =
                            (&mut self.group_order, &mut self.overview_two, &mut self.mine_first, &mut self.size);
                        settings::settings_page(page, scheme, "darstellung", &group_refs(&groups), &mut |ui, g, r| {
                            let (name, hint, choices) = rows[g].1[r];
                            let name = label(name);
                            let choices: Vec<String> = choices.iter().map(|c| label(c)).collect();
                            let hint = label(hint);
                            if (g, r) == (0, 0) {
                                // The scheme: its name as the small heading over its cards (a row without a control
                                // is one).
                                subheading(ui, scheme, &name, &hint);
                                // A small picture of the window per scheme, in its colors (like 4.12).
                                // The names in the order of `theme::schemes()` (a test keeps them alike).
                                if let Some(id) = widgets::scheme_cards(ui, scheme, theme::schemes(), &choices) {
                                    theme::set_current(id);
                                }
                                return;
                            }
                            settings::setting_row(ui, scheme, &name, &hint, false, |row| {
                                match (g, r) {
                                    // The size: slider in the column of controls, the quick choices below the row.
                                    (0, _) => size_slider(row, scheme, size),
                                    (1, _) => {
                                        let button = widgets::Button::new(&choices[0], ButtonKind::Normal, scheme);
                                        if row.add(button.blocked(already)).clicked() {
                                            *order = basetool.clone();
                                        }
                                    }
                                    (2, 0) => {
                                        if widgets::radio(row, scheme, *two, &choices[0]).clicked() {
                                            *two = true;
                                        }
                                        if widgets::radio(row, scheme, !*two, &choices[1]).clicked() {
                                            *two = false;
                                        }
                                    }
                                    _ => {
                                        if widgets::radio(row, scheme, *mine_first, &choices[0]).clicked() {
                                            *mine_first = true;
                                        }
                                        if widgets::radio(row, scheme, !*mine_first, &choices[1]).clicked() {
                                            *mine_first = false;
                                        }
                                    }
                                }
                            });
                            if (g, r) == (0, 1) {
                                size_presets(ui, scheme, size);
                            }
                        });
                    }
                    "hf_wasistneu" if self.real() => {
                        self.news.show(page, scheme, self.updates.releases());
                    }
                    "hf_wasistneu" => {
                        let title = tf("s_mu_neu_in", &[("version", Arg::from(SAMPLE_UPDATE))]);
                        let (of, back, next, open, close) = (
                            t("s_mu_von"),
                            t("s_mu_galerie_zurueck"),
                            t("s_mu_galerie_weiter"),
                            t("s_mu_galerie_seite"),
                            t("hf_schliessen"),
                        );
                        let texts = widgets::GalleryTexts {
                            title: &title,
                            of: &of,
                            back: &back,
                            next: &next,
                            open: &open,
                            close: &close,
                        };
                        let words: Vec<(String, String)> =
                            NEWS.iter().map(|(_, head, text, _)| (t(head), t(text))).collect();
                        let items: Vec<widgets::NewsItem> = NEWS
                            .iter()
                            .zip(&words)
                            .map(|((icon, _, _, page), (head, text))| widgets::NewsItem { icon, head, text, page })
                            .collect();
                        match widgets::news_gallery(page, scheme, &items, &mut self.news_index, &texts) {
                            Some(widgets::GalleryAction::Open(i)) => {
                                if let Some(group) = group_of(NEWS[i].3) {
                                    jump = Some((NEWS[i].3.to_string(), group));
                                    self.count_action("galerie_sprung");
                                }
                            }
                            Some(widgets::GalleryAction::Close) => {
                                self.page.clear();
                                self.usage_route = Some("sprung");
                            }
                            None => {}
                        }
                        // Below: every version, newest first; the skipped ones open.
                        page.add_space(GAP);
                        settings::setting_group(page, scheme, &t("s_mu_aeltere"));
                        let lines: Vec<Vec<String>> =
                            HISTORY.iter().map(|(_, _, keys)| keys.iter().map(|key| t(key)).collect()).collect();
                        let line_refs: Vec<Vec<&str>> =
                            lines.iter().map(|l| l.iter().map(String::as_str).collect()).collect();
                        let notes: Vec<widgets::VersionNote> = HISTORY
                            .iter()
                            .zip(&line_refs)
                            .map(|((version, date, _), lines)| widgets::VersionNote { version, date, lines })
                            .collect();
                        widgets::version_history(page, scheme, &notes, "5.0");
                    }
                    "hf_ordner" => {
                        // A switch of the data folder waits for a restart: the bar on top.
                        if self.data_folder.restart_bar(page, scheme) {
                            if self.real() {
                                // The window starts again and closes; a refused start is said and the bar stays.
                                if !self.restart_now(page.ctx()) {
                                    self.data_folder.restart_again();
                                }
                            } else {
                                let now = page.input(|i| i.time);
                                self.pin_notice = Some((now, language::t("s_pf_startet_neu")));
                            }
                        }
                        let real = self.real();
                        if real {
                            // The folder the program really reads the game from, or the honest "not found".
                            match &self.game_found {
                                Some(folder) => {
                                    let head = language::t("s_sc_da");
                                    widgets::status_box(page, scheme, scheme.accent, "circle-check", &head, folder);
                                    // A test build (PTU, EPTU, TECH-PREVIEW): its data are confidential by the terms of service.
                                    if let Some(channel) = game_start::test_channel(folder) {
                                        let said = tf("s_pf_kanal_hinweis", &[("0", Arg::from(channel.as_str()))]);
                                        page.add_space(4.0);
                                        widgets::status_box(page, scheme, scheme.yellow, "shield-alert", &said, "");
                                    }
                                }
                                None => {
                                    let (head, line) = (language::t("s_sc_weg"), language::t("s_pf_sc_weg_h"));
                                    widgets::status_box(page, scheme, scheme.yellow, "shield-alert", &head, &line);
                                    // Where it was looked for: the question that comes next.
                                    if !self.game_searched.is_empty() {
                                        page.add_space(4.0);
                                        let caption = language::t("gesucht_wurde_hier");
                                        page.label(RichText::new(caption).small().color(color(scheme.sub)));
                                        for place in &self.game_searched {
                                            page.add(egui::Label::new(RichText::new(place).small()).wrap());
                                        }
                                    }
                                }
                            }
                        } else {
                            let infos = page_infos("hf_ordner");
                            widgets::status_box(page, scheme, scheme.accent, "circle-check", &infos[0], &infos[1]);
                        }
                        page.add_space(GAP);
                        // One folder per channel, below the data folder.
                        let rows = settings_of("hf_ordner");
                        let titles: Vec<String> = rows.iter().map(|(title, _)| label(title)).collect();
                        let mut groups: Vec<(&str, usize)> =
                            titles.iter().zip(rows).map(|(title, (_, group))| (title.as_str(), group.len())).collect();
                        // The states of the channels exist on real data only.
                        if !real {
                            groups.truncate(3);
                        }
                        let paths = self.store.as_ref().map(|store| &store.paths);
                        let given_game = paths.and_then(|p| p.given().game);
                        let (folders, data) = (&mut self.folders, &mut self.data_folder);
                        let start_command = &mut self.start_command;
                        let channel_states = &mut self.channel_states;
                        let mut start_commit = false;
                        // What happened at the folder fields: handled once the page is drawn.
                        let mut events: Vec<(usize, widgets::FolderEvents)> = Vec::new();
                        settings::settings_page(page, scheme, "ordner", &group_refs(&groups), &mut |ui, g, r| {
                            let (name, hint, words) = rows[g].1[r];
                            if g == 3 {
                                channel_states.rows(ui, scheme, if real { paths } else { None });
                                return;
                            }
                            if g == 1 {
                                let name = label(name);
                                settings::setting_row(ui, scheme, &name, &label(hint), false, |_| {});
                                data.show(ui, scheme, paths);
                                ui.add_space(4.0);
                                return;
                            }
                            if g == 2 {
                                let name = label(name);
                                settings::setting_row(ui, scheme, &name, &label(hint), false, |_| {});
                                ui.horizontal(|ui| {
                                    let field = widgets::text_field(start_command, &t("s_pl_startbefehl"), scheme)
                                        .id_salt("startbefehl")
                                        .desired_width(
                                            (ui.available_width() - 130.0).clamp(120.0, widgets::FIELD_MAX_WIDTH),
                                        );
                                    ui.add(field);
                                    let word = label(words[0]);
                                    let take = widgets::Button::new(&word, ButtonKind::Normal, scheme);
                                    if ui.add(take).clicked() {
                                        start_commit = true;
                                    }
                                });
                                ui.label(RichText::new(t("s_or_start_bsp")).small().color(color(scheme.sub)));
                                ui.add_space(4.0);
                                return;
                            }
                            // Real data: what the folder is, not what the sample says.
                            let hint = if real {
                                language::t(match (folder_there(&folders[r]), r) {
                                    (true, _) => "s_pf_kanal_da",
                                    // The main installation missing is a fault; another channel is not installed.
                                    (false, 0) => "s_pf_kanal_weg",
                                    (false, _) => "s_ei_ordner_nicht",
                                })
                            } else {
                                label(hint)
                            };
                            let name = label(name);
                            settings::setting_row(ui, scheme, &name, &hint, false, |_| {});
                            if let (0, Some(fixed)) = (r, &given_game) {
                                // Given from outside: to read, not to change.
                                let reason = language::t("s_pf_gesperrt");
                                let clicks = data_folder::folder_row(
                                    ui,
                                    scheme,
                                    "spielordner-vorgabe",
                                    &folders[0],
                                    Some(&reason),
                                );
                                if clicks.open {
                                    versekit_core::browser::show_folder(&folders[0]);
                                }
                                data_folder::given_line(ui, scheme, fixed);
                            } else if given_game.is_some() && r > 0 {
                                // With a game folder given from outside only the channels of that copy count.
                                let reason = language::t("s_pf_gesperrt");
                                let clicks = data_folder::folder_row(
                                    ui,
                                    scheme,
                                    &format!("kanal-{r}"),
                                    &folders[r],
                                    Some(&reason),
                                );
                                if clicks.open && !folders[r].is_empty() {
                                    versekit_core::browser::show_folder(&folders[r]);
                                }
                            } else {
                                let seen = widgets::folder_field_events(
                                    ui,
                                    scheme,
                                    &mut folders[r],
                                    (&label(words[0]), &label(words[1])),
                                );
                                if seen.open && !folders[r].is_empty() {
                                    versekit_core::browser::show_folder(&folders[r]);
                                }
                                events.push((r, seen));
                            }
                            ui.add_space(4.0);
                        });
                        for (channel, seen) in events {
                            if seen.typed {
                                self.commit_game_folder(channel);
                            }
                            if seen.browse {
                                self.browse_game_folder(page.ctx(), channel);
                            }
                        }
                        if start_commit {
                            self.commit_start_command(page.input(|i| i.time));
                        }
                    }
                    "hf_grafik" if !self.real() => {
                        let rows = settings_of("hf_grafik");
                        let groups = group_sizes(rows);
                        let (range, pending) = (&mut self.view_range, &mut self.view_pending);
                        settings::settings_page(page, scheme, "grafik", &group_refs(&groups), &mut |ui, g, r| {
                            let (name, hint, _) = rows[g].1[r];
                            let (name, hint) = (label(name), label(hint));
                            settings::setting_row(ui, scheme, &name, &hint, false, |row| {
                                let response = settings::slider_with_value(row, scheme, &name, range, (20, 100), true, |n| {
                                    format!("{n} %")
                                });
                                if response.changed() {
                                    *pending = true;
                                }
                            });
                            // The game runs: noted at once, applied at the next game start.
                            if *pending {
                                ui.horizontal(|ui| {
                                    if let Some(image) = icons::themed("check", scheme, IconTint::Highlight, 14.0) {
                                        ui.add(image);
                                    }
                                    ui.label(RichText::new(t("s_ei_vorgemerkt")).small());
                                });
                            }
                        });
                    }
                    "hf_sichern" => {
                        let infos = page_infos("hf_sichern");
                        widgets::card(page, scheme, |card| {
                            settings::setting_group(card, scheme, &t("s_si_sicherung"));
                            let restore = widgets::Button::new(&infos[0], ButtonKind::Normal, scheme);
                            if card.add(restore.icon("archive-restore")).clicked() {
                                self.restore_open = true;
                            }
                        });
                        page.add_space(GAP);
                        widgets::card(page, scheme, |card| {
                            settings::setting_group(card, scheme, &t("s_si_protokolle"));
                            self.repeat_setup_row(card, scheme);
                        });
                    }
                    "k_basetool_verbindung" if self.connection.wire().is_some() => {
                        basetool_page::show_page(page, scheme, &mut self.connection)
                    }
                    "k_basetool_verbindung" if self.real() => {
                        // The connection could not be made: say so instead of a made-up state.
                        let (title, text) = (language::t("s_btv_ohne"), language::t("s_btv_ohne_h"));
                        widgets::status_box(page, scheme, scheme.red_text(), "shield-alert", &title, &text);
                    }
                    "k_basetool_verbindung" => {
                        let infos = page_infos("k_basetool_verbindung");
                        widgets::status_box(page, scheme, scheme.yellow, "shield-alert", &infos[0], &infos[1]);
                    }
                    "hf_liste" => {
                        self.blueprint_view.craftable = self.crafting.craftable();
                        blueprint_list::show(page, scheme, &mut self.blueprints, &mut self.blueprint_view);
                        // The crafting page of the blueprint: the item stands open there.
                        if let Some(name) = self.blueprint_view.craft_jump.take() {
                            self.crafting.jump_to(&name);
                            jump = Some(("hf_herstellung".to_string(), "hf_gruppe_herst"));
                        }
                    }
                    "hf_spiel" if self.real() => {
                        if let Some(game_texts_page::Jump::Translation) =
                            game_texts_page::show_game(page, scheme, &mut self.game_texts)
                        {
                            jump = Some(("hf_uebersetzung".to_string(), "hf_gruppe_spiel"));
                        }
                    }
                    "hf_uebersetzung" if self.real() => {
                        game_texts_page::show_translation(page, scheme, &mut self.game_texts);
                    }
                    // The shader cache is the last card of the graphics page (its own page until 5.1.3).
                    "hf_grafik" if self.real() => {
                        if let Some(store) = &self.store {
                            let shader = &mut self.shader;
                            let mut card =
                                |ui: &mut egui::Ui| shader_page::show_body(ui, scheme, shader, &store.paths);
                            grafik_page::show_page(page, scheme, &mut self.grafik, &store.paths, &mut card);
                        }
                        for said in [self.grafik.take_notice(), self.shader.take_notice()].into_iter().flatten() {
                            self.pin_fault = said.fault.then(|| said.text.clone());
                            self.pin_notice = Some((page.input(|i| i.time), said.text));
                        }
                    }
                    "hf_blickwinkel" if self.real() => {
                        if let Some(store) = &self.store {
                            blickwinkel_page::show_page(page, scheme, &mut self.fov, &store.paths);
                        }
                        if let Some(said) = self.fov.take_notice() {
                            self.pin_fault = said.fault.then(|| said.text.clone());
                            self.pin_notice = Some((page.input(|i| i.time), said.text));
                        }
                    }
                    "hf_joysticks" if self.real() => {
                        if let Some(store) = &self.store {
                            joysticks_page::show_page(page, scheme, &mut self.joysticks, &store.paths);
                        }
                        if let Some(said) = self.joysticks.take_notice() {
                            self.pin_fault = said.fault.then(|| said.text.clone());
                            self.pin_notice = Some((page.input(|i| i.time), said.text));
                        }
                    }
                    "hf_achsen" if self.real() => {
                        if let Some(store) = &self.store {
                            axes_page::show_page(page, scheme, &mut self.axes, &store.paths);
                        }
                        if let Some(said) = self.axes.take_notice() {
                            self.pin_fault = said.fault.then(|| said.text.clone());
                            self.pin_notice = Some((page.input(|i| i.time), said.text));
                        }
                    }
                    "hf_erkennung" if self.real() => {
                        if let Some(store) = &self.store {
                            detection_page::show_page(page, scheme, &mut self.detection, &store.paths);
                            // The switch for blueprints with no known source, or the collection, changed:
                            // the list and the progress follow at once.
                            if self.detection.take_all_changed() {
                                self.blueprints.follow_all_setting(&store.paths, &mut self.blueprint_view);
                            }
                        }
                        if let Some(said) = self.detection.take_notice() {
                            self.pin_fault = said.fault.then(|| said.text.clone());
                            self.pin_notice = Some((page.input(|i| i.time), said.text));
                        }
                    }
                    "hf_module" if self.real() => {
                        if let Some(store) = &self.store {
                            modules_page::show_page(page, scheme, &mut self.modules, &store.paths);
                            if self.modules.take_changed() {
                                self.module_hidden = versekit_core::modules::hidden_pages(&store.paths);
                            }
                        }
                        if let Some(said) = self.modules.take_notice() {
                            self.pin_fault = said.fault.then(|| said.text.clone());
                            self.pin_notice = Some((page.input(|i| i.time), said.text));
                        }
                    }
                    "hf_startprogramme" if self.real() => {
                        if let Some(store) = &self.store {
                            startprograms_page::show_page(
                                page,
                                scheme,
                                &mut self.start_programs,
                                &store.paths,
                                &self.spawner,
                            );
                        }
                        if let Some(said) = self.start_programs.take_notice() {
                            self.pin_fault = said.fault.then(|| said.text.clone());
                            self.pin_notice = Some((page.input(|i| i.time), said.text));
                        }
                    }
                    "hf_hangar" => {
                        self.show_hangar(page, scheme);
                        // The parts that can be crafted for a ship: Crafting with the ship in its filter.
                        if let Some(ship) = self.hangar.take_craft_jump() {
                            self.crafting.jump_to_ship(&ship);
                            jump = Some(("hf_herstellung".to_string(), "hf_gruppe_herst"));
                        }
                    }
                    // The page without an entry in the menu: the names moved into the hangar, a button leads there.
                    ASOP_PAGE => {
                        widgets::hint(page, scheme, &t("s_as_umgezogen_2"));
                        page.add_space(12.0);
                        let word = t("s_as_zum_hangar");
                        if page.add(widgets::Button::new(&word, ButtonKind::Strong, scheme).icon("plane")).clicked() {
                            jump = Some(("hf_hangar".to_string(), "hf_gruppe_logistik"));
                        }
                    }
                    "hf_meinlager" => stock_page::show_page(page, scheme, &mut self.stock),
                    "hf_farmliste" => match farm_page::show_page(page, scheme, &mut self.farm, &mut self.crafting) {
                        // The item opens in Crafting with its recipe.
                        Some(farm_page::Action::ToCrafting(name)) => {
                            self.crafting.jump_to(&name);
                            jump = Some(("hf_herstellung".to_string(), "hf_gruppe_herst"));
                        }
                        Some(farm_page::Action::ToMining(search)) => {
                            self.mining.jump_to(&search);
                            jump = Some(("hf_bergbau".to_string(), "hf_gruppe_abbau"));
                        }
                        None => {}
                    },
                    "hf_einkaufsliste" => {
                        shopping_page::show_page(page, scheme, &mut self.shopping, &mut self.crafting)
                    }
                    "hf_notizen" => notes_page::show_page(page, scheme, &mut self.memo_page),
                    "hf_contested" => contested_page::show_page(page, scheme, &mut self.contested),
                    "hf_serverstatus" => {
                        crate::serverstatus_page::show_page(page, scheme, &mut self.server_status);
                    }
                    "hf_patchaenderungen" => {
                        let catalog = &self.blueprints.catalog;
                        // The part of the patch that has a blueprint leads to it in the blueprint list.
                        if let Some(crate::patch_changes_page::Action::ToBlueprint(name)) =
                            crate::patch_changes_page::show_page(page, scheme, &mut self.patch_changes, catalog)
                        {
                            self.open_blueprint(&name);
                            jump = Some(("hf_liste".to_string(), "hf_gruppe_bp"));
                        }
                    }
                    "hf_handlesuche" => crate::handle_page::show_page(page, scheme, &mut self.handles),
                    "hf_bergbau" => {
                        if let Some(mining_page::Action::ToRefineries) =
                            mining_page::show_page(page, scheme, &mut self.mining)
                        {
                            jump = Some(("hf_raffinerien".to_string(), "hf_gruppe_abbau"));
                        }
                    }
                    "hf_raffinerien" => refineries_page::show_page(page, scheme, &mut self.mining),
                    "hf_bergung" => salvage_page::show_page(page, scheme, &mut self.salvage, &mut self.crafting),
                    "hf_herstellung" => {
                        let shared =
                            crafting_page::Shared { have: &self.blueprints.have, catalog: &self.blueprints.catalog };
                        match crafting_page::show_page(page, scheme, &mut self.crafting, &shared) {
                            // Where the blueprint comes from: the list, with the item as search and open.
                            Some(crafting_page::Action::ToBlueprint(name)) => {
                                self.open_blueprint(&name);
                                jump = Some(("hf_liste".to_string(), "hf_gruppe_bp"));
                            }
                            Some(crafting_page::Action::ToMining(material)) => {
                                self.mining.jump_to(&material);
                                jump = Some(("hf_bergbau".to_string(), "hf_gruppe_abbau"));
                            }
                            Some(crafting_page::Action::ToShops) => {
                                jump = Some(("hf_laeden".to_string(), "hf_gruppe_handel"));
                            }
                            None => {}
                        }
                    }
                    "hf_verkauf" => selling_page::show_page(page, scheme, &mut self.selling),
                    "hf_staffellager" => org_stock_page::show_page(page, scheme, &mut self.org_stock),
                    "hf_bedarf" => demand_page::show_page(page, scheme, &mut self.demand),
                    "hf_fortschritt" => {
                        // A jump that brings its filter along.
                        if let Some((to, from)) =
                            blueprint_progress::show(page, scheme, &self.blueprints, &mut self.progress_view)
                        {
                            self.blueprint_view.jump(to, from);
                            jump = Some(("hf_liste".to_string(), "hf_gruppe_bp"));
                        }
                    }
                    "hf_auftragslog" => {
                        // The open row leads to its blueprints, with the filter as a label.
                        let catalog = &self.blueprints.catalog;
                        if let Some(contract_log::LogAction::Jump(to, from)) =
                            contract_log::show(page, scheme, &mut self.contracts, catalog, &mut self.contract_view)
                        {
                            self.blueprint_view.jump(to, from);
                            jump = Some(("hf_liste".to_string(), "hf_gruppe_bp"));
                        }
                    }
                    "hf_routen" if self.real() => routes_page::show_page(page, scheme, &mut self.routes),
                    "hf_laeden" if self.real() => {
                        // A planned ship leads to the section "Planned" of the hangar page.
                        if let Some(shops_page::Action::ToPlanned) =
                            shops_page::show_page(page, scheme, &mut self.shops, &mut self.hangar)
                        {
                            self.hangar.show_planned();
                            jump = Some(("hf_hangar".to_string(), "hf_gruppe_logistik"));
                        }
                    }
                    "hf_routen" => {
                        // A ranking: the bar behind each line.
                        let best = ROUTES.iter().map(|(_, value)| *value).fold(0.0, f32::max);
                        for (name, value) in ROUTES {
                            let written =
                                format!("{} aUEC", versekit_core::formats::count(language::current(), value as i64));
                            widgets::ranking_row(page, scheme, name, &written, value / best);
                            page.add_space(2.0);
                        }
                    }
                    "hf_danke" => thanks_page::show(page, scheme, &mut self.thanks),
                    "hf_diagnose" if self.real() => {
                        self.mark_errors_seen();
                        if let Some(state) = &mut self.diagnose {
                            state.set_basetool(self.connection.wire().map(|wire| wire.sync.clone()));
                            diagnose_page::show(page, scheme, state);
                        }
                    }
                    "hf_diagnose" => {
                        let name = Arg::from(t("hf_diagnose"));
                        let said = match self.error_count {
                            0 => tf("s_mu_diag_nichts", &[("name", name)]),
                            1 => tf("s_mu_diag_fehler_eins", &[("name", name)]),
                            count => tf("s_mu_diag_fehler", &[("count", Arg::from(count)), ("name", name)]),
                        };
                        page.label(said);
                        page.horizontal_wrapped(|row| {
                            let raise_word = t("s_mu_diag_ausloesen");
                            let raise = widgets::Button::new(&raise_word, ButtonKind::Normal, scheme);
                            if row.add(raise).clicked() {
                                self.error_count += 1;
                            }
                            if self.error_count > 0
                                && row
                                    .add(widgets::Button::new(&t("s_mu_diag_gemeldet"), ButtonKind::Strong, scheme))
                                    .clicked()
                            {
                                self.error_count = 0;
                            }
                        });
                        page.add_space(GAP);
                        if let Some(state) = &mut self.diagnose {
                            state.set_basetool(self.connection.wire().map(|wire| wire.sync.clone()));
                            diagnose_page::show(page, scheme, state);
                        } else {
                            let texts = [t("hf_diagnose"), t("s_div_ohne_daten")];
                            widgets::status_box(page, scheme, scheme.sub, "info", &texts[0], &texts[1]);
                        }
                    }
                    name if statistics_page::Page::of(name).is_some() => {
                        if let Some(statistics) = statistics_page::Page::of(name)
                            && let Some(said) = statistics_page::show(page, scheme, &mut self.stats, statistics)
                        {
                            self.pin_notice = Some((page.input(|i| i.time), said));
                        }
                    }
                    name if self.extension.as_ref().is_some_and(|slot| slot.has(name)) => {
                        let context = extension::Ctx {
                            scheme,
                            paths: self.store.as_ref().map(|store| &store.paths),
                            time: page.input(|i| i.time),
                            size: self.size,
                            english: self.general_english,
                            update_bar: self.update_waits(),
                            new_mark: !self.new_pages.is_empty(),
                            test_build: self.test_build,
                            errors: self.error_count,
                            popup: self.overlay_popup,
                            popup_seconds: self.overlay_seconds,
                            overflow: [self.frame_overflow, self.frame_overflow_y, self.tile_overflow],
                        };
                        if let Some(slot) = &mut self.extension {
                            slot.extension.show(page, name, &context);
                        }
                    }
                    _ if self.real() => not_yet(page, scheme),
                    _ => {
                        page.label(RichText::new(t("s_mu_seite")).color(color(scheme.sub)));
                    }
                }
            });
            if self.page == "hf_achsen"
                && self.real()
                && let Some(store) = &self.store
            {
                axes_page::show_foot(&mut page, scheme, &mut self.axes, &store.paths);
                if let Some(said) = self.axes.take_notice() {
                    self.pin_fault = said.fault.then(|| said.text.clone());
                    self.pin_notice = Some((page.input(|i| i.time), said.text));
                }
            }
            // The page scrolls on its own and clips: what sticks out to the right
            // would be cut off unseen, so it counts for the edge check.
            self.frame_overflow = (shown.content_size.x - shown.inner_rect.width()).max(0.0);
            // Rows of the mission log are clipped by their own list: measured there.
            if self.page == "hf_auftragslog" {
                self.frame_overflow = self.frame_overflow.max(self.contract_view.row_overflow);
            }
            // The lists of the trade pages sit in scroll areas of their own.
            match self.page.as_str() {
                "hf_verkauf" => self.frame_overflow = self.frame_overflow.max(self.selling.list_overflow()),
                "hf_routen" if self.real() => {
                    self.frame_overflow = self.frame_overflow.max(self.routes.list_overflow())
                }
                "hf_laeden" if self.real() => self.frame_overflow = self.frame_overflow.max(self.shops.list_overflow()),
                "hf_bergung" => self.frame_overflow = self.frame_overflow.max(self.salvage.list_overflow()),
                _ => {}
            }
            self.count_salvage_view();
            for key in self.stock.take_counted() {
                self.count_action(key);
            }
            if let Some((target, group)) = jump {
                // A jump out of a page with a row in the menu offers the way back; the overview, the pages
                // outside the menu and a pinned card offer none.
                if jump_route == "sprung" && group_of(&self.page).is_some() && target != self.page {
                    self.came_from = Some((self.page.clone(), target.clone()));
                }
                self.page = target;
                self.usage_route = Some(jump_route);
                if !group.is_empty() {
                    widgets::set_nav_section_open(ui.ctx(), &t(group), false);
                }
            }
            ui.allocate_rect(
                egui::Rect::from_min_size(top, egui::vec2(ui.available_width(), height)),
                egui::Sense::hover(),
            );
            self.foot(ui, scheme);
        });
    }

    /// Search field with suggestions, an empty list and a dialog.
    fn inputs(&mut self, ui: &mut egui::Ui, scheme: &Scheme) {
        ui.label(RichText::new(t("s_mu_suchfeld")).strong());
        ui.add_space(4.0);
        let more = |n: usize| tf("s_mlg_mehr", &[("n", Arg::from(n))]);
        let hint = t("s_mu_suche_ph");
        let names: Vec<String> = NAMES.iter().map(|name| label(name)).collect();
        let found = widgets::SuggestField::new("suche", &hint, scheme, &more).searching().show(ui, &mut self.search, &names);
        if found.picked.is_some() {
            self.picked = found.picked;
        } else if found.response.changed() {
            self.picked = None;
        }
        if let Some(name) = self.picked.and_then(|i| names.get(i)) {
            let chosen = tf("s_mu_gewaehlt_name", &[("name", Arg::from(name.as_str()))]);
            ui.label(RichText::new(chosen).color(color(scheme.sub)));
        }
        ui.add_space(GAP);
        ui.label(RichText::new(t("s_mu_auswahlfeld")).strong());
        ui.add_space(4.0);
        let (name, hint, empty) = (t("s_mu_auswahl_name"), t("s_mu_auswahl_ph"), t("s_mu_auswahl_leer"));
        widgets::SelectField::new("auswahl", &name, &hint, &empty, scheme).show(ui, &mut self.choice, &names);
        ui.add_space(GAP);
        layout::columns_or_stack(ui, SECTION_MIN_WIDTH, 2, |ui, i| {
            if i == 0 {
                ui.label(RichText::new(t("s_mu_leere_liste")).strong());
                widgets::list_frame(ui, scheme, |ui| {
                    ui.set_min_width(ui.available_width());
                    widgets::empty_state(ui, scheme, "bookmark", &t("s_mu_merkliste_leer"));
                });
            } else {
                ui.label(RichText::new(t("s_mu_dialog")).strong());
                ui.label(RichText::new(t("s_mu_dialog_mitte")).color(color(scheme.sub)));
                ui.add_space(4.0);
                if ui.add(widgets::Button::new(&t("s_mu_dialog_oeffnen"), ButtonKind::Normal, scheme)).clicked() {
                    self.dialog_open = true;
                }
            }
        });
    }

    fn own_controls(&mut self, ui: &mut egui::Ui, scheme: &Scheme) {
        ui.label(RichText::new(t("s_mu_eigene")).strong());
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.add(widgets::Toggle::new(&mut self.toggle, scheme));
            ui.label(t("s_mu_schalter"));
            ui.add_space(16.0);
            ui.add(widgets::Slider::new(&mut self.value, 0, 100, scheme));
            ui.label(format!("{} %", self.value));
        });
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.add(widgets::Button::new(&t("s_mu_knopf"), ButtonKind::Normal, scheme));
            ui.add(widgets::Button::new(&t("speichern"), ButtonKind::Strong, scheme));
            ui.add(widgets::Button::new(&t("s_zuruecksetzen"), ButtonKind::Danger, scheme));
            ui.add(widgets::Button::new("Basetool", ButtonKind::Brand(theme::KRT_ORANGE), scheme));
        });
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            for (tint, word) in [
                (IconTint::Normal, "s_mu_tint_normal"),
                (IconTint::Highlight, "s_mu_tint_hervor"),
                (IconTint::Hover, "s_mu_tint_darueber"),
            ] {
                for name in ["settings", "activity", "archive-restore"] {
                    if let Some(image) = icons::themed(name, scheme, tint, 18.0) {
                        ui.add(image);
                    }
                }
                ui.label(RichText::new(t(word)).color(color(scheme.sub)));
                ui.add_space(10.0);
            }
        });
        ui.add_space(6.0);
        ui.label(RichText::new(t("s_mu_gelb")).color(color(scheme.yellow)));
        ui.label(RichText::new(t("s_mu_blau")).color(color(scheme.blue)));
        ui.label(RichText::new(t("s_mu_rot")).color(color(scheme.red_text())));
        ui.label(RichText::new(t("s_mu_gedaempft")).color(color(scheme.sub)));
    }

    fn built_in_controls(&mut self, ui: &mut egui::Ui, scheme: &Scheme) {
        ui.label(RichText::new(t("s_mu_eingebaut")).strong());
        ui.add_space(4.0);
        let tabs = [t("hf_uebersicht"), t("bauplaene"), t("s_uv_lager")];
        widgets::tab_bar(ui, scheme, &[&tabs[0], &tabs[1], &tabs[2]], &mut self.tab);
        ui.add_space(GAP);
        let field_width = (ui.available_width() - 140.0).clamp(120.0, 280.0);
        egui::Grid::new("eingebaut").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            ui.label(t("s_mu_aufklappliste"));
            let combo = egui::ComboBox::from_id_salt("system").selected_text(SYSTEMS[self.system]).show_ui(ui, |ui| {
                for (i, name) in SYSTEMS.iter().enumerate() {
                    ui.selectable_value(&mut self.system, i, *name);
                }
            });
            widgets::name_combo(&combo.response, &t("s_mu_aufklappliste"), SYSTEMS[self.system]);
            ui.end_row();

            ui.label(t("s_mu_auswahlknoepfe"));
            ui.horizontal_wrapped(|ui| {
                for (i, name) in ["s_mu_sort_name", "s_mu_sort_art", "s_mu_sort_neueste"].iter().enumerate() {
                    if widgets::radio(ui, scheme, self.sort == i, &t(name)).clicked() {
                        self.sort = i;
                    }
                }
            });
            ui.end_row();

            ui.label(t("s_mu_eingabefeld"));
            widgets::clearable_field(ui, &mut self.text, &t("s_mu_eingabe_ph"), scheme, field_width);
            ui.end_row();

            ui.label(t("s_mu_mehrzeilig"));
            ui.add(
                widgets::text_area(&mut self.notes, &t("s_mlg_rf_feld"), scheme)
                    .desired_rows(2)
                    .desired_width(field_width),
            );
            ui.end_row();

            ui.label(t("s_mu_pruefung"));
            ui.vertical(|ui| {
                let error_text = quality_error(&self.quality).map(label);
                let error = error_text.as_deref();
                widgets::checked_field(ui, scheme, &mut self.quality, &t("s_mu_q_ph"), field_width, error);
                let first = t("s_mu_q_erst");
                let reason = error.map(|_| first.as_str());
                ui.add(widgets::Button::new(&t("s_lg_eintragen"), ButtonKind::Strong, scheme).blocked(reason));
            });
            ui.end_row();

            ui.label(t("s_mu_fortschritt"));
            widgets::progress(ui, scheme, self.value as f32 / 100.0, field_width);
            ui.end_row();

            ui.label(t("s_mu_hinweis"));
            ui.label(RichText::new(t("s_mu_maus_hierher")).underline()).on_hover_text(t("s_mu_hinweis_text"));
            ui.end_row();
        });
        ui.add_space(6.0);
        for (i, name) in ["s_mu_nur_fehlende", "s_mu_mit_fundorten"].iter().enumerate() {
            widgets::switch_row(ui, scheme, &t(name), &mut self.switches[i]);
        }
        ui.add_space(6.0);
        widgets::collapsing(&t("s_mu_aufklappbar")).default_open(false).show(ui, |ui| {
            ui.label(t("s_mu_aufklapp_inhalt"));
        });
        ui.add_space(6.0);
        ui.label(t("s_mu_liste_auswahl"));
        widgets::list_frame(ui, scheme, |ui| {
            let area = egui::ScrollArea::vertical().id_salt("liste").max_height(90.0).auto_shrink([false, true]);
            widgets::scroll_area(ui, scheme, area, |ui| {
                for (i, (name, kind)) in BLUEPRINTS.iter().enumerate() {
                    let text = format!("{}   ·   {}", t(name), t(kind));
                    if widgets::list_row(ui, scheme, self.row == i, &text).clicked() {
                        self.row = i;
                    }
                }
            });
        });
    }

    /// Table, notice and loading indicator.
    fn data(&mut self, ui: &mut egui::Ui, scheme: &Scheme) {
        ui.label(RichText::new(t("s_mu_tabelle")).strong());
        ui.add_space(4.0);
        let (column, ascending) = self.table_sort;
        let mut rows = ORES.to_vec();
        rows.sort_by(|a, b| {
            let order = match column {
                0 => a.0.cmp(b.0),
                1 => a.1.cmp(&b.1),
                _ => a.2.cmp(&b.2),
            };
            if ascending { order } else { order.reverse() }
        });
        let mut clicked = None;
        egui_extras::TableBuilder::new(ui)
            .id_salt("tabelle")
            .striped(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(egui_extras::Column::remainder().at_least(90.0))
            .column(egui_extras::Column::auto().at_least(90.0))
            .column(egui_extras::Column::auto().at_least(70.0))
            .header(24.0, |mut header| {
                for (i, name) in ["s_mu_sort_name", "s_lg_menge", "s_sl_qualitaet"].iter().enumerate() {
                    header.col(|ui| {
                        let sorted = (column == i).then_some(ascending);
                        if widgets::sort_head(ui, scheme, &t(name), sorted, i > 0) {
                            clicked = Some(i);
                        }
                    });
                }
            })
            .body(|mut body| {
                for (name, amount, quality) in rows {
                    body.row(22.0, |mut row| {
                        row.col(|ui| widgets::cell(ui, name, false));
                        row.col(|ui| widgets::cell(ui, &amount.to_string(), true));
                        row.col(|ui| widgets::cell(ui, &quality.to_string(), true));
                    });
                }
            });
        if let Some(i) = clicked {
            self.table_sort = if column == i { (i, !ascending) } else { (i, true) };
        }
        ui.add_space(GAP);
        ui.horizontal_wrapped(|ui| {
            if ui.add(widgets::Button::new(&t("s_mu_meldung_zeigen"), ButtonKind::Normal, scheme)).clicked() {
                self.notice_at = Some(ui.input(|i| i.time));
            }
            ui.label(RichText::new(t("s_mu_meldung_oben")).color(color(scheme.sub)));
        });
        ui.add_space(GAP);
        widgets::loading(ui, scheme, &t("s_mu_laedt_daten"));
        ui.add_space(GAP);
        layout::columns_or_stack(ui, SECTION_MIN_WIDTH, 2, |ui, i| {
            if i == 0 {
                self.rows_with_actions(ui, scheme);
            } else {
                self.failed_loading(ui, scheme);
            }
        });
    }

    /// Rows with buttons on hover; deleting at once with undo.
    fn rows_with_actions(&mut self, ui: &mut egui::Ui, scheme: &Scheme) {
        ui.label(RichText::new(t("s_mu_zeilen_knoepfe")).strong());
        widgets::list_frame(ui, scheme, |ui| {
            ui.set_min_width(ui.available_width());
            let mut deleted = None;
            let (keep, delete) = (t("s_mu_merken"), t("s_gs_loeschen"));
            for (i, name) in self.entries.iter().enumerate() {
                let shown = label(name);
                match widgets::action_row(
                    ui,
                    scheme,
                    &shown,
                    &[("bookmark", keep.as_str()), ("trash-2", delete.as_str())],
                ) {
                    Some(0) => self.row_note = Some(tf("s_mu_gemerkt", &[("name", Arg::from(shown.as_str()))])),
                    Some(_) => deleted = Some(i),
                    None => {}
                }
            }
            if let Some(i) = deleted {
                let name = self.entries.remove(i);
                self.undo = Some((ui.input(|t| t.time), i, name));
            }
            if self.entries.is_empty() {
                let all_gone = tf("s_mu_alles_weg", &[("name", Arg::from(t("s_mlg_rueckgaengig")))]);
                widgets::empty_state(ui, scheme, "bookmark", &all_gone);
            }
        });
        ui.horizontal_wrapped(|ui| {
            for (icon, meaning) in
                [("pencil", "s_ml_bearbeiten"), ("bookmark", "s_mu_merken"), ("trash-2", "s_gs_loeschen")]
            {
                widgets::icon_button(ui, scheme, icon, &t(meaning));
            }
            let default_note = t("s_mu_symbol_knoepfe");
            let note = self.row_note.as_deref().unwrap_or(&default_note);
            ui.label(RichText::new(note).color(color(scheme.sub)));
        });
        if self.entries.is_empty()
            && self.undo.is_none()
            && ui.add(widgets::Button::new(&t("s_mu_liste_zurueck"), ButtonKind::Normal, scheme)).clicked()
        {
            self.entries = ENTRIES.to_vec();
        }
    }

    /// Loading that failed and retries by itself.
    fn failed_loading(&mut self, ui: &mut egui::Ui, scheme: &Scheme) {
        ui.label(RichText::new(t("s_mu_laden_fehl")).strong());
        let now = ui.input(|t| t.time);
        let failed_at = *self.failed_at.get_or_insert(now);
        let left = widgets::retry_in(failed_at, now);
        if left <= 0.0 {
            // The sample fails every time.
            self.failed_at = Some(now);
        }
        let status = tf("s_mu_naechster", &[("seconds", Arg::from(left.ceil() as i64))]);
        widgets::load_failed(ui, scheme, &t("s_mu_nicht_erreichbar"), &status);
    }
}

/// The page under the overview at the top of the menu that belongs to no group, with its Lucide icon.
const TOP_PAGE: (&str, &str) = ("hf_notizen", "sticky-note");

/// (group, [(page, Lucide icon)]), order and icons of 4.12.
/// The setting that keeps each group of the menu folded, with the names of 4.12 (`gruppe_zu_<id>`).
/// The order of the groups of the menu, as a list of the ids of [`GROUP_SETTINGS`] (`bauplaene`, `logistik`, …).
const GROUP_ORDER_SETTING: &str = "menue_gruppen_reihenfolge";

/// The id of a group in the settings: its `gruppe_zu_<id>` without the prefix.
fn group_id(g: usize) -> &'static str {
    GROUP_SETTINGS[g].1.trim_start_matches("gruppe_zu_")
}

/// The stored order of the groups; an unknown id is left out, a group missing from it follows in the order of
/// [`NAV`], so the result always holds every group once.
fn group_order_from(stored: Option<&Value>) -> Vec<usize> {
    let mut order: Vec<usize> = Vec::new();
    if let Some(Value::Array(ids)) = stored {
        for id in ids.iter().filter_map(Value::as_str) {
            if let Some(g) = (0..NAV.len()).find(|&g| group_id(g) == id)
                && !order.contains(&g)
            {
                order.push(g);
            }
        }
    }
    order.extend((0..NAV.len()).filter(|g| !order.contains(g)).collect::<Vec<_>>());
    order
}

fn group_order_value(order: &[usize]) -> Value {
    Value::Array(order.iter().map(|&g| Value::str(group_id(g))).collect())
}

const GROUP_SETTINGS: [(&str, &str); 10] = [
    ("hf_gruppe_bp", "gruppe_zu_bauplaene"),
    ("hf_gruppe_logistik", "gruppe_zu_logistik"),
    ("hf_gruppe_herst", "gruppe_zu_werkstatt"),
    ("hf_gruppe_abbau", "gruppe_zu_bergung"),
    ("hf_gruppe_handel", "gruppe_zu_handel"),
    ("hf_gruppe_beschaffung", "gruppe_zu_beschaffung"),
    ("hf_gruppe_statistik", "gruppe_zu_statistiken"),
    ("hf_gruppe_einst", "gruppe_zu_einstellungen"),
    ("hf_gruppe_spiel", "gruppe_zu_spiel_einst"),
    ("hf_gruppe_info", "gruppe_zu_info"),
];

/// The pages that show what a reading of the game archive is for (`archive_lock::reading_for`): a reading that waits
/// for the game to close is told on these pages.
const TOPIC_PAGES: [(&str, &[&str]); 5] = [
    ("texts", &["hf_uebersetzung"]),
    ("catalog", &["hf_liste", "hf_fortschritt", "hf_herstellung", "hf_farmliste"]),
    ("signatures", &["hf_bergbau", "hf_erkennung"]),
    ("values", &["hf_hangar"]),
    ("controls", &["hf_joysticks"]),
];

/// The pages whose data come out of the game archive: where the data of a test channel are marked as preliminary.
const ARCHIVE_PAGES: [&str; 9] = [
    "hf_liste",
    "hf_fortschritt",
    "hf_herstellung",
    "hf_farmliste",
    "hf_bergbau",
    "hf_uebersetzung",
    "hf_joysticks",
    "hf_erkennung",
    "hf_hangar",
];

const NAV: [(&str, &[(&str, &str)]); 10] = [
    (
        "hf_gruppe_bp",
        &[("hf_liste", "clipboard-list"), ("hf_fortschritt", "chart-column"), ("hf_auftragslog", "notebook-pen")],
    ),
    ("hf_gruppe_logistik", &[("hf_hangar", "plane"), ("hf_meinlager", "package"), ("hf_staffellager", "boxes")]),
    ("hf_gruppe_herst", &[("hf_herstellung", "zap"), ("hf_farmliste", "diamond")]),
    ("hf_gruppe_abbau", &[("hf_bergbau", "compass"), ("hf_raffinerien", "factory"), ("hf_bergung", "wrench")]),
    ("hf_gruppe_handel", &[("hf_laeden", "store"), ("hf_verkauf", "coins"), ("hf_routen", "route")]),
    (
        "hf_gruppe_beschaffung",
        &[("hf_bedarf", "arrow-left-right"), ("hf_einkaufsliste", "receipt"), ("hf_contested", "timer")],
    ),
    (
        "hf_gruppe_statistik",
        &[
            ("hf_st_uebersicht", "chart-no-axes-combined"),
            ("hf_st_schiffe", "plane-takeoff"),
            ("hf_st_auftraege", "crosshair"),
            ("hf_st_quantum", "orbit"),
            ("hf_st_stabil", "activity"),
        ],
    ),
    (
        "hf_gruppe_einst",
        &[
            ("hf_allgemein", "settings"),
            ("hf_overlay", "app-window"),
            ("hf_darstellung", "palette"),
            ("hf_ordner", "folder"),
            ("hf_module", "puzzle"),
            // 4.12 hid it in a folded block "Für Fortgeschrittene"; v5 gives it an entry of its own (decision 21.10.).
            ("hf_erkennung", "scan-search"),
            ("hf_startprogramme", "circle-play"),
            ("k_basetool_verbindung", "arrow-left-right"),
            ("hf_sichern", "archive-restore"),
        ],
    ),
    (
        "hf_gruppe_spiel",
        &[
            ("hf_spiel", "message-square-text"),
            ("hf_uebersetzung", "languages"),
            ("hf_grafik", "monitor"),
            ("hf_blickwinkel", "eye"),
            ("hf_joysticks", "gamepad-2"),
            ("hf_achsen", "chart-spline"),
        ],
    ),
    (
        "hf_gruppe_info",
        // The update page under Info.
        &[
            ("hf_ueber", "info"),
            ("hf_patchaenderungen", "clock"),
            ("hf_serverstatus", "server"),
            ("hf_handlesuche", "users"),
            ("hf_danke", "heart-handshake"),
        ],
    ),
];

/// One setting row: name, the hint below it, the words of its choices and
/// buttons. A text that is a language key stands for its text.
type Row = (&'static str, &'static str, &'static [&'static str]);

/// The setting groups of a page: (group title, rows).
type Groups = &'static [(&'static str, &'static [Row])];

/// The settings of the pages: (page, groups). The pages draw their rows
/// from here and the menu filter searches the same table, so whatever a
/// page shows can be found.
const PAGE_SETTINGS: [(&str, Groups); 7] = [
    (
        "k_basetool_verbindung",
        &[
            (
                "s_btv_verbindung",
                &[
                    ("s_btv_name", "", &[]),
                    ("s_btv_verbinden", "", &[]),
                    ("s_btv_trennen", "", &[]),
                    ("s_btv_jetzt", "", &[]),
                    ("s_btv_konto", "", &[]),
                ],
            ),
            (
                "s_ei_bereiche",
                &[
                    ("s_btv_bauplaene", "", &[]),
                    ("s_btv_lager", "", &[]),
                    ("s_btv_hangar", "", &[]),
                    ("s_btv_bedarf", "", &[]),
                    ("s_btv_staffellager", "", &[]),
                    ("s_btv_staffellager_buchen", "", &[]),
                ],
            ),
        ],
    ),
    ("hf_allgemein", GENERAL),
    (
        "hf_overlay",
        &[
            // What the overlay shows and how: one line each, the hint on hover.
            (
                "s_ov_gruppe_anzeige",
                &[
                    ("s_ei_ov_spiel", "s_ei_ov_spiel_h", &[]),
                    ("s_ov_ohne_spiel", "s_ov_ohne_spiel_h", &[]),
                    ("s_ei_ov_durch", "s_ei_ov_durch_h", &[]),
                    ("s_ov_durch_auto", "ov_s_durch_auto_h", &[]),
                    ("s_zeilen", "s_ei_ov_zeilen_h", &["s_ei_zeilen_einheit"]),
                    ("e_deckkraft", "ov_s_deck_h", &[]),
                    ("s_ov_groesse", "s_ov_groesse_h", &[]),
                ],
            ),
            (
                "s_ei_platz",
                &[
                    (
                        "s_ov_ecke",
                        "s_ei_platz_h",
                        &[
                            "s_ov_ecke_ol",
                            "s_ov_ecke_or",
                            "s_ov_ecke_ul",
                            "s_ov_ecke_ur",
                            "s_ov_ecke_frei",
                            "s_ei_leiste_oben",
                            "s_ei_leiste_unten",
                        ],
                    ),
                    ("ov_s_mitte", "ov_s_mitte_h_2", &["ov_s_mitte_knopf"]),
                ],
            ),
            // Always or only on news, with the seconds, in one line (the seconds row is drawn in it).
            (
                "s_ov_gruppe_wann",
                &[
                    ("s_ov_modus", "ov_s_modus_h", &["s_ov_immer", "s_ov_popup"]),
                    ("s_ov_dauer", "ov_s_dauer_h", &["s_ov_sek_sichtbar"]),
                ],
            ),
            // The five shortcuts, one line each: switch, name, modifier keys, key.
            (
                "s_ov_gruppe_kuerzel",
                &[
                    ("s_ei_ov_taste", "s_ei_ov_taste_h", &["s_ei_taste_strg", "Alt", "s_ei_taste_umschalt"]),
                    ("s_hk_scan", "s_hk_scan_h_2", &["s_ei_taste_strg", "Alt", "s_ei_taste_umschalt"]),
                    ("s_hk_suche", "s_hk_suche_h", &["s_ei_taste_strg", "Alt", "s_ei_taste_umschalt"]),
                    ("s_hk_bild", "s_hk_bild_h", &["s_ei_taste_strg", "Alt", "s_ei_taste_umschalt"]),
                    ("s_hk_haupt", "s_hk_haupt_h", &["s_ei_taste_strg", "Alt", "s_ei_taste_umschalt"]),
                ],
            ),
            // The events that open the folded or waiting overlay, one switch each (the order of
            // `versekit_core::overlay::Event::ALL`).
            (
                "s_ov_ereignisse",
                &[
                    ("s_ov_bei_bauplan", "s_ov_bei_bauplan_h", &[]),
                    ("s_ov_bei_scanner", "s_ov_bei_scanner_h", &[]),
                    ("s_ov_bei_auftrag", "s_ov_bei_auftrag_h", &[]),
                    ("s_ov_bei_raffinerie", "s_ov_bei_raffinerie_h", &[]),
                    ("s_ov_bei_werkbank", "s_ov_bei_werkbank_h", &[]),
                    ("s_ov_bei_katalog", "s_ov_bei_katalog_h", &[]),
                ],
            ),
            // What the open overlay shows besides its rows, one switch each (the order of
            // `versekit_core::overlay::Section::ALL`).
            (
                "s_ov_anzeigen",
                &[
                    ("s_ov_zeige_server", "s_ov_zeige_server_h", &[]),
                    ("s_ov_zeige_ort", "s_ov_zeige_ort_h", &[]),
                    ("s_ov_zeige_auftraege", "s_ov_zeige_auftraege_h", &[]),
                    ("s_ov_zeige_raffinerie", "s_ov_zeige_raffinerie_h", &[]),
                    ("s_ov_zeige_werkbank", "s_ov_zeige_werkbank_h", &[]),
                    ("s_ov_zeige_update", "s_ov_zeige_update_h", &[]),
                    // After the switches: the most contracts for the count in the head of the bar.
                    ("s_ov_auftraege_max", "s_ov_auftraege_max_h", &[]),
                ],
            ),
            // The info bar: a small window of its own over the game, with what it shows (the frame rates too).
            (
                "s_ov_infoleiste_titel",
                &[
                    ("s_ov_infoleiste", "s_ov_infoleiste_h", &[]),
                    ("s_ov_infoleiste_ohne_spiel", "s_ov_infoleiste_ohne_spiel_h", &[]),
                    (
                        "s_ov_infoleiste_ecke",
                        "s_ov_infoleiste_ecke_h",
                        &["s_ov_infoleiste_links", "s_ov_infoleiste_mitte", "s_ov_infoleiste_rechts"],
                    ),
                    ("s_ov_infoleiste_groesse", "s_ov_infoleiste_groesse_h", &[]),
                    ("s_ov_fps", "s_ov_fps_h", &[]),
                    ("s_ov_server_fps", "s_ov_server_fps_h", &[]),
                    ("s_ov_spieler", "s_ov_spieler_h", &[]),
                ],
            ),
            // The settings of the handle search, which shows in the bar: a card of its own.
            ("s_hqs_einstellungen", &[("s_hqs_einstellungen", "s_hqs_einstellungen_h", &[])]),
            // The window of the signature scanner, the part for mining, after the overlay itself.
            (
                "s_sg_titel",
                &[
                    ("s_sg_lage", "s_sg_lage_h", &["s_sg_lage_mitte"]),
                    ("s_sg_groesse", "s_sg_groesse_h", &[]),
                    ("s_sg_zeile", "s_sg_zeile_h", &[]),
                    ("s_sg_rahmen", "s_sg_rahmen_h", &["s_sg_rahmen_weg"]),
                    // From here on rows of their own after the frame (row 3 is drawn inside the model).
                    ("s_sg_ort", "s_sg_ort_h", &["s_sg_ort_signatur", "s_sg_ort_leiste"]),
                    ("s_sg_nachlauf", "s_sg_nachlauf_h", &[]),
                    ("s_sg_bild_wartezeit", "s_sg_bild_wartezeit_h", &[]),
                ],
            ),
        ],
    ),
    (
        "hf_darstellung",
        &[
            (
                "s_ei_aussehen",
                &[
                    (
                        "s_da_schema",
                        "s_ei_sofort_h",
                        &["s_da_original", "s_da_krt", "s_da_eis", "s_da_nebel", "s_da_glut", "s_da_kontrast"],
                    ),
                    ("s_ei_groesse", "s_ei_groesse_h", &SIZE_LABELS),
                ],
            ),
            ("s_ei_seitenleiste", &[("s_ei_reihenfolge", "s_ei_reihenfolge_h", &["s_ei_wie_basetool"])]),
            (
                "hf_uebersicht",
                &[
                    ("s_ei_aufbau", "s_ei_aufbau_h", &["s_ei_zwei", "s_ei_ein"]),
                    ("s_ei_oben", "", &["k_bereich_deine", "k_bereich_blick"]),
                ],
            ),
        ],
    ),
    (
        "hf_ueber",
        &[
            ("s_ub_version", &[("s_ei_testversionen", "s_ei_testversionen_h", &["s_ul_testversion", "s_ul_stabil"])]),
            ("s_ei_aktualisieren", &[("s_ei_updates", "s_ei_updates_h", &[])]),
        ],
    ),
    (
        "hf_ordner",
        &[
            (
                "s_ei_spielordner",
                &[
                    // The names of the channels are the game's own, in the order of `paths::CHANNELS`.
                    ("LIVE", "s_pf_kanal_da", &["s_durchsuchen", "s_oeffnen"]),
                    ("HOTFIX", "s_ei_ordner_nicht", &["s_durchsuchen", "s_oeffnen"]),
                    ("PTU", "s_pf_kanal_da", &["s_durchsuchen", "s_oeffnen"]),
                    ("EPTU", "s_ei_ordner_nicht", &["s_durchsuchen", "s_oeffnen"]),
                    ("TECH-PREVIEW", "s_ei_ordner_nicht", &["s_durchsuchen", "s_oeffnen"]),
                ],
            ),
            ("s_eigene", &[("s_pf_datenordner", "s_pf_daten_h", &["s_durchsuchen", "s_oeffnen"])]),
            // The own command that starts the game (Lutris, Heroic, Flatpak or a script); empty: the way found.
            ("s_pf_start_gruppe", &[("s_or_start", "s_or_start_h", &["s_or_uebernehmen"])]),
            // The state of each channel found, beside the start of the game.
            ("s_uc_kanaele", &[("s_uc_kanaele", "s_uc_kanaele_h", &[])]),
        ],
    ),
    ("hf_grafik", &[("s_ei_bild", &[("s_ei_sichtweite", "s_ei_sichtweite_h", &[])])]),
];

/// Whether a game folder is set and lies there.
fn folder_there(folder: &str) -> bool {
    !folder.is_empty() && std::path::Path::new(folder).is_dir()
}

/// The channels of the rows of game folders: the one list of the program (LIVE, HOTFIX, PTU, EPTU, TECH-PREVIEW).
const CHANNEL_ROWS: [&str; CHANNEL_COUNT] = versekit_core::paths::CHANNELS;
const CHANNEL_COUNT: usize = versekit_core::paths::CHANNELS.len();

/// The game folders of the sample, per channel.
const FOLDERS: [&str; CHANNEL_COUNT] = [
    "C:\\Program Files\\Roberts Space Industries\\StarCitizen\\LIVE",
    "",
    "C:\\Program Files\\Roberts Space Industries\\StarCitizen\\PTU",
    "",
    "",
];

/// The version history of the sample, newest first: (version, date, lines as
/// language keys).
const HISTORY: [(&str, &str, &[&str]); 3] = [
    ("5.1", "02.11.2026", &["s_mu_news1_k", "s_mu_news2_k", "s_mu_news3_k"]),
    ("5.0.1", "26.10.2026", &["s_mu_hist_overlay", "s_mu_hist_pfade"]),
    ("5.0", "20.10.2026", &["s_mu_hist_rust", "s_mu_hist_schemata", "s_mu_hist_halb"]),
];
/// Keys a hotkey can use, as stored: a letter, a digit, F1 … F24 and the named keys of
/// `overlay::NAMED_KEYS` (`overlay::Hotkey::parse`).
const HOTKEYS: [&str; 67] = [
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W",
    "X", "Y", "Z", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8",
    "F9", "F10", "F11", "F12", "F13", "F14", "F15", "F16", "F17", "F18", "F19", "F20", "F21", "F22", "F23", "F24",
    "Pause", "Rollen", "Einfg", "NumGeteilt", "NumMal", "NumMinus", "NumPlus",
];
/// The words the picker and the texts show for the named keys of `HOTKEYS` (`Rollen` is `Scroll Lock` in English).
const HOTKEY_WORDS: [(&str, &str); 7] = [
    ("Pause", "s_ei_taste_pause"),
    ("Rollen", "s_ei_taste_rollen"),
    ("Einfg", "s_ei_taste_einfg"),
    ("NumGeteilt", "s_ei_taste_num_geteilt"),
    ("NumMal", "s_ei_taste_num_mal"),
    ("NumMinus", "s_ei_taste_num_minus"),
    ("NumPlus", "s_ei_taste_num_plus"),
];
/// The modifier keys of a hotkey as stored, in the order of the picker.
const HOTKEY_MODIFIERS: [&str; 3] = ["Strg", "Alt", "Umschalt"];
/// Fewest and most rows of the overlay, as the overlay reads them.
const OVERLAY_ROWS: (i64, i64) = versekit_core::overlay::ROWS_RANGE;

/// About the width of the keys of a shortcut (three modifier keys, „+“ and the key), for the column of names before
/// them.
const HOTKEY_PICKER_WIDTH: f32 = 340.0;

use crate::settings::subheading;

/// A shortcut in one line: its switch (`None`: a grey switch, on when the shortcut is `always` on, else off: it
/// cannot be had here), its name with the hint on hover behind the symbol `info`, then `keys` while it is on. The
/// names and the keys stand in line in every row. Framed after a jump from the menu filter to its name or hint.
fn shortcut_line(
    ui: &mut egui::Ui,
    scheme: &Scheme,
    name: &str,
    hint: &str,
    switch: Option<&mut bool>,
    always: bool,
    keys: impl FnOnce(&mut egui::Ui),
) {
    let spacing = ui.spacing().item_spacing.x;
    // Too narrow for the name and the keys side by side: the keys stand in a line of their own under the name.
    let stacked = ui.available_width() < widgets::TOGGLE_SIZE.x + spacing + 200.0 + HOTKEY_PICKER_WIDTH;
    let block = ui.vertical(|ui| {
        let mut show_keys = false;
        let mut keys = Some(keys);
        ui.horizontal(|line| {
            line.set_min_height(widgets::TOGGLE_SIZE.y);
            // (shown on, cannot be had here)
            let (on, impossible) = match switch {
                Some(on) => {
                    line.add(widgets::Toggle::new(on, scheme));
                    (*on, false)
                }
                None => {
                    let mut fixed = always;
                    line.add_enabled(false, widgets::Toggle::new(&mut fixed, scheme));
                    (always, !always)
                }
            };
            // Where it cannot be had, the keys say why.
            show_keys = on || impossible;
            // The keys begin at the same place in every line, after a name column as in the other rows.
            let names = if stacked {
                line.available_width()
            } else {
                widgets::name_column(line.available_width() + widgets::TOGGLE_SIZE.x, HOTKEY_PICKER_WIDTH)
                    - widgets::TOGGLE_SIZE.x
                    - spacing
            };
            let size = egui::vec2(names, widgets::TOGGLE_SIZE.y);
            let left = egui::Layout::left_to_right(egui::Align::Center);
            line.allocate_ui_with_layout(size, left, |cell| {
                if !stacked {
                    cell.set_min_width(names);
                }
                cell.add(egui::Label::new(name).truncate());
                let icon = cell.text_style_height(&egui::TextStyle::Small);
                if let Some(image) = crate::icons::themed("info", scheme, versekit_core::theme::IconTint::Normal, icon) {
                    cell.add(image);
                }
            })
            .response
            .on_hover_text(hint);
            if show_keys && !stacked && let Some(keys) = keys.take() {
                keys(line);
            }
        });
        if show_keys && let Some(keys) = keys.take() {
            ui.horizontal(|line| {
                line.add_space(widgets::TOGGLE_SIZE.x + spacing);
                keys(line);
            });
        }
    });
    crate::highlight::mark(ui, scheme, block.response.rect, &[name, hint]);
    ui.add_space(6.0);
}

/// The language key of the word for a key as stored, `None` for a letter, a digit or F1 … F24 (they read the same in
/// every language).
fn hotkey_word(stored: &str) -> Option<&'static str> {
    HOTKEY_WORDS.iter().find(|(name, _)| *name == stored).map(|(_, word)| *word)
}

/// The keys of `HOTKEYS` as the picker shows them, in the language of the window.
fn hotkey_names() -> Vec<String> {
    HOTKEYS.iter().map(|name| hotkey_word(name).map_or_else(|| (*name).to_string(), language::t)).collect()
}

/// A stored hotkey such as `Strg+Alt+B`: which modifiers, which key (index
/// into `HOTKEYS`). Read by the same parser as the overlay
/// (`overlay::Hotkey::parse`); `None` for anything it would not take.
fn parse_hotkey(text: &str) -> Option<([bool; 3], usize)> {
    let hotkey = versekit_core::overlay::Hotkey::parse(text)?;
    let key = HOTKEYS.iter().position(|name| versekit_core::overlay::key_code(name) == Some(hotkey.key))?;
    Some(([hotkey.ctrl, hotkey.alt, hotkey.shift], key))
}

/// The hotkey as stored, `Strg+Alt+B`; `None` without a modifier key, unless the key stands alone (F1 … F24 and
/// the named keys, `overlay::Hotkey::stands_alone`): a letter or a digit by itself would catch the typing in the game.
fn hotkey_text(modifiers: [bool; 3], key: usize) -> Option<String> {
    let mut parts: Vec<&str> = HOTKEY_MODIFIERS.iter().zip(modifiers).filter(|(_, on)| *on).map(|(m, _)| *m).collect();
    let name = HOTKEYS.get(key)?;
    let alone = versekit_core::overlay::key_code(name).is_some_and(versekit_core::overlay::Hotkey::stands_alone);
    if parts.is_empty() && !alone {
        return None;
    }
    parts.push(name);
    Some(parts.join("+"))
}

/// After a change in a hotkey picker: a combination [`hotkey_text`] would not take (a letter or a digit left without
/// a modifier) goes back to the one `before`, which stays saved. Returns whether it went back.
fn keep_hotkey(modifiers: &mut [bool; 3], key: &mut usize, before: ([bool; 3], usize)) -> bool {
    let refused = hotkey_text(*modifiers, *key).is_none() && hotkey_text(before.0, before.1).is_some();
    if refused {
        (*modifiers, *key) = before;
    }
    refused
}

/// The combinations of the five shortcuts of the overlay (the order of `overlay::Shortcut::ALL`: modifiers, key and
/// whether it is on and can be had here) that are registered; `None` for one that is not.
fn active_hotkeys(rows: [([bool; 3], usize, bool); 5]) -> [Option<String>; 5] {
    rows.map(|(modifiers, key, on)| on.then(|| hotkey_text(modifiers, key)).flatten())
}

/// The earlier shortcut that holds the combination of row `r`: the overlay does not register a later one on it.
fn earlier_holder(r: usize, active: &[Option<String>; 5]) -> Option<usize> {
    let mine = active.get(r)?.as_ref()?;
    active[..r].iter().position(|other| other.as_ref() == Some(mine))
}

/// Any other shortcut that holds the combination of row `r`: the picker gives no key to two of them.
fn any_holder(r: usize, active: &[Option<String>; 5]) -> Option<usize> {
    let mine = active.get(r)?.as_ref()?;
    (0..active.len()).find(|&i| i != r && active[i].as_ref() == Some(mine))
}

/// The overlay corners as stored.
const CORNERS: [(settings::OverlayPlace, &str); 5] = [
    (settings::OverlayPlace::Free, "frei"),
    (settings::OverlayPlace::TopLeft, "oben-links"),
    (settings::OverlayPlace::TopRight, "oben-rechts"),
    (settings::OverlayPlace::BottomLeft, "unten-links"),
    (settings::OverlayPlace::BottomRight, "unten-rechts"),
];

fn corner_of(name: &str) -> Option<settings::OverlayPlace> {
    CORNERS.iter().find(|(_, n)| *n == name).map(|(place, _)| *place)
}

fn corner_name(place: settings::OverlayPlace) -> &'static str {
    CORNERS.iter().find(|(p, _)| *p == place).map_or("frei", |(_, name)| name)
}

/// The settings of a page, empty for a page without any.
fn settings_of(page: &str) -> Groups {
    // Where the system has no application menu, the page General has no row for an entry in it; on Windows the row
    // stays and says that the installer makes the entry.
    if page == "hf_allgemein" && !versekit_core::desktop_entry::available() && !cfg!(windows) {
        return GENERAL_WITHOUT_MENU;
    }
    PAGE_SETTINGS.iter().find(|(p, _)| *p == page).map_or(&[], |(_, groups)| *groups)
}

/// The group "Start" of the page General. From the third row on the rows of `desktop_rows`
/// (`desktop_rows::FIRST_ROW`); the entry in the application menu is the last one.
const GENERAL_START: &[Row] = &[
    ("autostart_win", "s_ei_autostart_h", &[]),
    // No "look for updates": v5 always looks quietly and installs on a click or
    // once the game has ended, so nobody runs an old version.
    ("s_ei_gpu", "s_ei_gpu_h", &[]),
    ("s_tray", "s_tray_h", &[]),
    ("s_systemmeldung", "s_systemmeldung_h", &[]),
    ("s_menue", "s_menue_h", &[]),
];

/// The groups of the page General after "Start".
const GENERAL_LANGUAGE: &[Row] = &[
    // The names of the languages are written in their own language.
    ("sprache", "s_ei_sofort_h", &["English", "Deutsch"]),
    ("s_ei_uhr24", "s_ei_uhr24_h", &[]),
    ("s_zeit", "s_zeit_h_2", &[]),
];
const GENERAL_SOUNDS: &[Row] = &[
    ("s_ei_ton_bp", "s_ei_ton_bp_h", &[]),
    ("s_ei_ton_raff", "s_ei_ton_raff_h", &[]),
    ("s_ei_lautstaerke", "s_ei_lautstaerke_h", &[]),
];
// The errors, the streamer mode, what goes to scmdb.net (`desktop_rows::SCMDB_ROW`) and the switch of the daily
// usage report (`usage::SETTING`, as in 4.12).
const GENERAL_PRIVACY: &[Row] = &[
    ("s_ei_fehler", "s_ei_fehler_h", &[]),
    ("s_ei_streamer", "s_ei_streamer_h", &[]),
    ("s_scmdb", "s_scmdb_h", &[]),
    ("s_nutzung", "s_nutzung_h", &[]),
];
const GENERAL_IN_GAME: &[Row] =
    &[("s_ei_mango", "s_ei_mango_h", &["s_ei_nur_linux"]), ("s_takt", "s_takt_h", &["s_sek"])];

/// The page General.
const GENERAL: Groups = &[
    ("s_ei_start", GENERAL_START),
    ("sprache", GENERAL_LANGUAGE),
    ("s_ei_toene", GENERAL_SOUNDS),
    ("s_ei_datenschutz", GENERAL_PRIVACY),
    ("s_ei_im_spiel", GENERAL_IN_GAME),
];

/// [`GENERAL`] without the row of the entry in the application menu.
const GENERAL_WITHOUT_MENU: Groups = &[
    ("s_ei_start", GENERAL_START.split_at(GENERAL_START.len() - 1).0),
    ("sprache", GENERAL_LANGUAGE),
    ("s_ei_toene", GENERAL_SOUNDS),
    ("s_ei_datenschutz", GENERAL_PRIVACY),
    ("s_ei_im_spiel", GENERAL_IN_GAME),
];

/// What the real page Grafik shows as settings, for the menu filter: the same names, hints and choices as
/// `grafik_page` draws (the sample page has its one made-up row, which the real page does not have).
const GRAFIK_REAL: Groups = &[
    (
        "hf_grafik",
        &[
            ("s_gr_renderer", "s_gr_renderer_h", &["s_gr_renderer_1", "s_gr_renderer_0"]),
            ("s_gr_fenstermodus", "", &["s_gr_fenster_0", "s_gr_fenster_1", "s_gr_fenster_2"]),
            ("s_gr_vsync", "s_gr_vsync_h", &[]),
            ("s_gr_upscaling", "s_gr_upscaling_h", &["s_gr_up_0", "s_gr_up_1", "s_gr_up_2", "s_gr_up_3", "s_gr_up_4"]),
            ("s_gr_technik", "", &["s_gr_tech_0", "s_gr_tech_1", "s_gr_tech_2"]),
            ("s_gr_modell", "s_gr_modell_h", &["s_gr_modell_0", "s_gr_modell_1"]),
            ("s_gr_unschaerfe", "", &[]),
            ("s_gr_koernung", "", &[]),
            ("s_gr_schaerfen", "", &[]),
            ("s_gr_aberration", "s_gr_aberration_h", &[]),
        ],
    ),
    (
        "s_gr_q_titel",
        &[
            (
                "s_gr_q_gesamt",
                "s_gr_q_gesamt_h",
                &["s_gr_q_auto", "s_gr_stufe_1", "s_gr_stufe_2", "s_gr_stufe_3", "s_gr_stufe_4", "s_gr_stufe_5"],
            ),
            ("s_gr_q_objektdetails", "", &[]),
            ("s_gr_q_sichtweite", "", &[]),
            ("s_gr_q_texturqualitaet", "", &[]),
            ("s_gr_q_texturdetails", "", &[]),
            ("s_gr_q_bodentexturen", "", &[]),
            ("s_gr_q_texturfilter", "", &[]),
            ("s_gr_q_schatten", "", &[]),
            ("s_gr_q_schatten_bild", "", &[]),
            ("s_gr_q_wolken", "", &[]),
            ("s_gr_q_gaswolken", "", &[]),
            ("s_gr_q_nebel", "", &[]),
            ("s_gr_q_wasserkaustik", "", &[]),
            ("s_gr_q_wassersim", "", &[]),
            ("s_gr_q_shader", "", &[]),
            ("s_gr_q_nachbearbeitung", "", &[]),
            ("s_gr_q_video", "", &[]),
            ("s_gr_q_partikel", "", &[]),
            ("s_gr_q_planetentexturen", "", &[]),
        ],
    ),
    (
        "s_gr_cfg_titel",
        &[
            ("s_gr_maxfps", "s_gr_maxfps_h", &[]),
            ("s_gr_idlefps", "s_gr_idlefps_h", &[]),
            ("s_gr_info", "s_gr_info_h", &["s_gr_info_0", "s_gr_info_1", "s_gr_info_2", "s_gr_info_3", "s_gr_info_4"]),
        ],
    ),
];

/// The settings of a page as the menu filter searches them: on real data the real rows of the page Grafik.
fn searched_settings(page: &str, sample: bool) -> Groups {
    if page == "hf_grafik" && !sample { GRAFIK_REAL } else { settings_of(page) }
}

/// (group title, number of rows) as [`settings::settings_page`] takes them
/// once the titles are written out (see [`group_refs`]).
fn group_sizes(groups: Groups) -> Vec<(String, usize)> {
    groups.iter().map(|(title, rows)| (label(title), rows.len())).collect()
}

/// The groups as [`settings::settings_page`] takes them.
fn group_refs<S: AsRef<str>>(groups: &[(S, usize)]) -> Vec<(&str, usize)> {
    groups.iter().map(|(title, rows)| (title.as_ref(), *rows)).collect()
}

/// The text of a language key, any other text as it is.
fn label(text: &str) -> String {
    let lang = versekit_core::language::current();
    if versekit_core::language::show_keys() && versekit_core::language::text_in(lang, text).is_some() {
        return format!("[{text}]");
    }
    versekit_core::language::text_in(lang, text).map_or_else(|| text.to_string(), str::to_string)
}

/// The infos a page shows besides its settings: buttons, its tile line and
/// the short version on its tile.
fn page_infos(page: &str) -> Vec<String> {
    let mut infos = base_infos(page);
    if let Some((_, _, line)) = overview_tile(page) {
        infos.push(line);
    }
    if overview_tile(page).is_some() {
        infos.extend(tile_details(page));
    }
    infos
}

/// What the menu filter finds on a page besides its name: on the sample page
/// everything it shows, on real data only its buttons and labels, never sample data.
fn search_infos(page: &str, sample: bool) -> Vec<String> {
    let found: Vec<String> = match page {
        _ if sample => page_infos(page),
        "hf_meinlager" => Vec::new(),
        // The game channels at the foot of the page, with the state of their user.cfg.
        "hf_ordner" => ["s_uc_kanaele", "s_uc_kanaele_h"].iter().map(|key| t(key)).collect(),
        // The block user.cfg at the foot of the real page Spiel.
        "hf_spiel" => [
            "aafk_titel",
            "aafk_h",
            "aafk_warten",
            "aafk_warten_h",
            "aafk_nur_windows",
            "aafk_hinweis",
            "aafk_hinweis_h",
            "s_uc_titel",
            "s_uc_lead",
            "s_uc_kanal",
            "s_uc_eigene",
            "s_uc_eigene_h",
            "s_uc_speichern",
            "s_uc_neu",
            "s_uc_ganz",
        ]
        .iter()
        .map(|key| t(key))
        .chain(base_infos(page))
        .collect(),
        // What the real page Blickwinkel shows.
        "hf_blickwinkel" => [
            "s_fv_lead",
            "s_fv_erklaerung",
            "s_fv_messen",
            "s_fv_neu_messen",
            "s_fv_nicht_gemessen",
            "s_fv_breite",
            "s_fv_abstand",
            "s_fv_abstand_hilfe",
            "s_fv_neutral",
            "s_fv_neutral_hilfe",
            "s_fv_im_spiel",
            "s_fv_optimalpunkt",
            "s_fv_optimalpunkt_hilfe",
            "s_fv_hinweis_deutung",
        ]
        .iter()
        .map(|key| t(key))
        .collect(),
        // What the real page Steuerung shows besides the sticks themselves.
        "hf_joysticks" => [
            "s_js_lead",
            "s_js_hinweis",
            "s_gh_titel",
            "s_gh_lead",
            "s_gh_was_tun",
            "s_js_geraete",
            "s_gh_bereit",
            "s_gh_ohne_nummer",
            "s_gh_abgesteckt",
            "s_gh_unbekannt",
            "s_gh_alles_gut",
            "s_gh_kein_geraet",
            "s_js_leer",
            "s_js_keine_datei",
            "s_js_passt",
            "s_js_ersetzt",
            "s_js_ohne",
            "s_js_nicht_erkannt",
            // The table of the bindings and the buttons above it.
            "s_js_belegt",
            "s_js_datei",
            "s_js_q_werk",
            "s_js_gruppe",
            "s_js_nur_fav",
            "s_js_nur_eigene",
            "s_js_nur_frei",
            "s_js_k_tasten",
            "s_js_k_stick",
            "s_pl_belegung",
            "s_js_b_hinweis",
            "s_js_zurueck",
            "s_js_profil",
            "s_js_ausgeben",
            "s_js_ausgeben_csv",
            "s_js_einlesen",
            "s_gh_tausch_knopf",
        ]
        .iter()
        .map(|key| t(key))
        .collect(),
        // What the real page Achsen & Kurven shows besides the sticks themselves.
        "hf_achsen" => [
            "s_ac_lead",
            "s_ac_hinweis",
            "s_ac_doppelt",
            "s_ac_keine",
            "s_ac_keine_werte",
            "s_kv_totzone",
            "s_kv_saettigung",
            "s_kv_ganz",
            "s_kv_quadrant",
            "s_ac_gross",
            "s_ac_geaendert",
            "s_ac_kopf_empf",
            "s_ac_lead_empf",
            "s_ac_keine_funktion",
            "s_ac_kopf_vor",
            "s_ac_lead_vor",
            "s_ac_vor_standard",
            "s_ac_vor_balanced",
            "s_ac_vor_fine",
            "s_ac_vor_direct",
            "s_ac_vor_pedals",
            "s_ac_kopf_uebertragen",
            "s_ac_lead_uebertragen",
            "s_ac_alle_achsen",
            "s_ac_uebertragen",
            "s_ac_speichern",
            "s_ac_verwerfen",
            "s_ac_kopf_angleichen",
            "s_ac_lead_angleichen",
            "s_ac_kopf_tauschen",
            "s_ac_lead_tauschen",
            "s_ac_befund",
            "s_ac_aufraeumen",
            "s_gs_titel",
            "s_gs_lead",
            "s_gs_speichern",
            "s_gs_anwenden",
            "s_gs_loeschen",
            "s_gs_keine",
            "s_pl_satzname",
        ]
        .iter()
        .map(|key| t(key))
        .collect(),
        // The switch of every area on the real page Module.
        "hf_module" => versekit_core::modules::SWITCHABLE
            .iter()
            .map(|(group, _)| t(versekit_core::modules::label(group)))
            .collect(),
        // The switch of the real page Startprogramme and the button of the real page Shader.
        "hf_startprogramme" => ["s_pg_aktiv", "s_pg_aktiv_h"].iter().map(|key| t(key)).collect(),
        // The box with the computer and the buttons of the bar on the real page Grafik, and its card of the shader
        // cache.
        "hf_grafik" => [
            "s_sc_titel",
            "s_sc_knopf",
            "s_sc_knopf_alt",
            "s_gr_rechner",
            "s_gr_cpu",
            "s_gr_gpu",
            "s_gr_ram",
            "s_gr_aufloesung",
            "s_gr_index",
            "s_gr_schreiben",
            "s_gr_verwerfen",
        ]
        .iter()
        .map(|key| t(key))
        .collect(),
        // The rows of each card of a channel on the real page Übersetzung.
        "hf_uebersetzung" => base_infos(page)
            .into_iter()
            .chain(["s_tq_sprache", "s_tq_quelle"].iter().map(|key| t(key)))
            .chain(std::iter::once(tf("s_tq_wie", &[("0", Arg::from("LIVE"))])))
            .collect(),
        _ => base_infos(page),
    };
    found.into_iter().chain(language_infos(page)).collect()
}

/// The buttons of a page whose texts come from the language file, in the language
/// in force: the menu filter finds them like the fixed ones.
fn language_infos(page: &str) -> Vec<String> {
    let keys: &[&str] = match page {
        "hf_st_uebersicht" => &[
            "s_sx_lead",
            "s_sa_zuletzt",
            "s_sa_sitzungen",
            "s_sa_dateien",
            "s_sa_groesse",
            "s_sx_spielzeit",
            "s_sx_sitzungen",
            "s_sx_auftraege",
            "s_sx_spruenge",
            "s_sx_tode",
            "s_sx_verloren",
            "s_sx_wann",
            "hf_st_auswertung",
            "s_sa_jetzt",
            "s_sa_neu",
            "s_sa_auto",
            "s_sx_export",
        ],
        "hf_st_schiffe" => &[
            "s_ss_lead",
            "s_ss_meist",
            "s_ss_sitzungen_mit",
            "s_ss_schiffe",
            "s_ss_anteil",
            "s_ss_genutzt",
            "s_ss_verloren",
            "s_ss_ruecken",
            "s_ss_seite",
            "s_ss_hand",
        ],
        "hf_st_auftraege" => &["s_sm_lead", "s_sm_fertig", "s_sm_fehl", "s_sm_abbruch", "s_sm_haeufig", "s_sm_letzte"],
        "hf_st_quantum" => &["s_sq_lead", "s_sq_angekommen", "s_sq_wahl", "s_sq_quote", "s_sq_ziele", "s_sq_starts"],
        "hf_st_stabil" => &["s_sb_lead", "s_sb_sauber", "s_sb_absturz", "s_sb_offen", "s_sb_gruende", "s_sb_letzte"],
        // The settings of the pairing lie in the fold "Kopplung verwalten" of the hangar page.
        "hf_hangar" => &hangar_page::PAIRING_SETTINGS,
        _ => &[],
    };
    // "Jetzt abgleichen" stands in the heads of the three pages with a Basetool area (while it is switched on).
    let sync = crate::head_sync::setting_of(page).map(|_| "s_bt_jetzt");
    keys.iter().copied().chain(sync).map(versekit_core::language::t).collect()
}

/// The buttons and labels of a page, and on the sample page its sample data.
fn base_infos(page: &str) -> Vec<String> {
    let texts = |keys: &[&str]| keys.iter().map(|key| t(key)).collect();
    match page {
        "hf_ueber" => texts(&[
            "s_up_sofort",
            "s_mu_version_kopieren",
            "s_mu_projektseite",
            "s_ub_kanal",
            "s_ub_fertig",
            "s_ub_test",
            "s_ub_einrichtung",
        ]),
        "hf_liste" => texts(&["s_bp_suche_platz", "filter_deckel", "s_bp_leg_start"]),
        "hf_meinlager" => STOCK.iter().map(|(name, ..)| name.to_string()).collect(),
        "hf_sichern" => {
            let mut words: Vec<String> = texts(&[
                "s_mu_sicherung_einspielen",
                "s_si_einrichtung",
                "s_si_einrichtung_h",
                "hf_einrichtung",
                "s_be_aus",
                "s_be_art_basetool",
                "s_be_voll",
                "s_be_alle",
                "s_be_ablage",
                "s_be_neu",
                "s_be_reset",
            ]);
            // The format of scmdb is named by its site in every language.
            words.push("scmdb.net".to_string());
            words
        }
        // What the crafting page and the farm list show, so that the menu filter finds it.
        "hf_herstellung" => texts(&["s_pl_herstellung", "s_lg_anzahl", "s_lg_bauen", "s_mz_knopf", "s_he_regler_kopf"]),
        "hf_farmliste" => texts(&["s_mz_titel", "s_mz_weg", "s_fl_sammel_kopf", "s_fl_alles_da"]),
        "hf_bedarf" => texts(&["s_bd_rohstoffe", "s_bd_gegenstaende", "s_bd_auftraege", "s_bt_jetzt"]),
        "hf_einkaufsliste" => texts(&[
            "s_ek_lead",
            "s_wk_route",
            "s_wk_kaufen",
            "s_wk_bauen",
            "s_wk_bauzeit",
            "s_ek_aus_hangar",
            "s_ek_aus_wunsch",
            "s_ek_das_schiff",
            "s_p9_ek_abhaken",
            "s_p9_ek_haken_weg",
            "s_p9_ek_hoch",
            "s_p9_ek_runter",
            "s_p9_ek_ziehen",
        ]),
        "hf_notizen" => texts(&["s_nz_lead", "s_nz_neu", "s_nz_loeschen", "s_nz_schreib_hier"]),
        "hf_handlesuche" => texts(&[
            "s_hqt_reiter_suche",
            "s_hqt_reiter_beziehungen",
            "s_hqt_karte_suche",
            "s_hqt_karte_austausch",
            "s_hqs_lead",
            "s_hq_verlauf",
            "s_hqb_lead",
            "s_hqs_liste",
            "s_hq_export",
            "s_hq_import",
            "s_hq_freundlich",
            "s_hq_neutral",
            "s_hq_feindlich",
            "s_hq_beobachten",
            "s_hqs_unbekannt",
        ]),
        "hf_contested" => texts(&[
            "s_cz_hangar",
            "s_cz_naechste",
            "s_cz_hand_kopf",
            "s_cz_hand_auf",
            "s_cz_hand_reset",
            "s_cz_fortgeschritten",
            "s_cz_overlay",
            "s_cz_overlay_h",
            "s_cz_timer",
            "s_cz_timer_lead",
            "s_cz_start",
            "s_cz_stopp",
            "s_cz_zurueck",
        ]),
        "hf_bergung" => texts(&[
            "s_zb_suche",
            "s_wr_such_hilfe",
            "s_wr_npc_kopf",
            "s_wr_nachsehen",
            "s_wr_leeren",
            "s_wr_vergessen",
            "s_zb_zurueck_wrack",
        ]),
        "hf_ordner" => vec![t("s_mu_ordner_gefunden"), FOLDERS[0].to_string()],
        "k_basetool_verbindung" => texts(&["s_uv_basetool_aus", "s_mu_basetool_anmelden"]),
        _ => Vec::new(),
    }
}

/// The line of a page or tile whose real data are not in this version yet.
fn not_yet(ui: &mut egui::Ui, scheme: &Scheme) {
    ui.label(RichText::new(glance::not_yet()).color(color(scheme.sub)));
}

/// A text as the menu filter compares it: lower case, `ß` as `ss` and the
/// umlauts as `ae`, `oe`, `ue`, so `GRÖSSE` and `groesse` find `Größe`.
fn search_form(text: &str) -> String {
    text.to_lowercase().replace('ß', "ss").replace('ä', "ae").replace('ö', "oe").replace('ü', "ue")
}

/// Words besides the name by which the menu filter finds a page: the pages of
/// the Basetool connection answer to "basetool".
fn search_words(page: &str) -> &'static [&'static str] {
    match page {
        "k_basetool_verbindung" | "hf_staffellager" | "hf_bedarf" => &["basetool"],
        // The two pages this one took over, by their old names and by what they were about.
        "hf_bergung" => &[
            "wrack",
            "wreck",
            "fabricator",
            "was steckt drin",
            "what's inside",
            "lohnt das zerlegen",
            "worth dismantling",
        ],
        // The page that took the shader cache in.
        "hf_grafik" => &["shader"],
        _ => &[],
    }
}

/// Parts of a page the menu filter leads to by their name, each found by more names than its own:
/// (page, text key of the hit, text keys of the words that find it). The section "Planned" of the hangar
/// page is also found by the name of the page it replaced, and so is the card of the shader cache on the graphics page.
const SECTION_HITS: [(&str, &str, &[&str]); 2] = [
    ("hf_hangar", "s_hg_geplant_titel", &["hf_wunschliste"]),
    ("hf_grafik", "s_sc_titel", &["hf_shader", "s_sc_knopf", "s_sc_knopf_alt"]),
];

/// The section of `page` whose hit text is `setting`, when there is one.
fn is_section_hit(page: &str, setting: &str) -> bool {
    SECTION_HITS.iter().any(|(p, hit, _)| *p == page && t(hit) == setting)
}

/// Settings a page draws from a module of its own, not from [`PAGE_SETTINGS`], that the menu filter finds on real
/// data (only there they stand): (page, text key of the name, of the hint, of more words that find it; empty: none).
const MODULE_SETTINGS: [(&str, &str, &str, &str); 20] = [
    // The settings of the handle search, on its page and in the card of the info bar of the overlay page.
    ("hf_overlay", "s_hq_einst_tage", "s_hq_einst_tage_h", ""),
    ("hf_overlay", "s_hq_einst_aff", "s_hq_einst_aff_h", ""),
    ("hf_overlay", "s_hq_einst_verborgen", "s_hq_einst_verborgen_h", ""),
    ("hf_overlay", "s_hq_einst_leeren", "s_hq_einst_leeren_h", ""),
    ("hf_overlay", "s_hq_einst_log", "s_hq_einst_log_h", ""),
    ("hf_overlay", "s_hq_einst_log_sek", "s_hq_einst_log_sek_h", ""),
    ("hf_handlesuche", "s_hq_einst_tage", "s_hq_einst_tage_h", ""),
    ("hf_handlesuche", "s_hq_einst_aff", "s_hq_einst_aff_h", ""),
    ("hf_handlesuche", "s_hq_einst_verborgen", "s_hq_einst_verborgen_h", ""),
    ("hf_handlesuche", "s_hq_einst_leeren", "s_hq_einst_leeren_h", ""),
    ("hf_handlesuche", "s_hq_einst_log", "s_hq_einst_log_h", ""),
    ("hf_handlesuche", "s_hq_einst_log_sek", "s_hq_einst_log_sek_h", ""),
    ("hf_allgemein", "s_er_acc", "s_er_acc_h", "s_er_acc_suche"),
    ("hf_allgemein", "s_er_fremd", "s_er_fremd_h", "s_er_acc_suche"),
    ("hf_erkennung", "s_er_satz", "s_er_satz_h", ""),
    ("hf_erkennung", "s_er_kat", "s_er_kat_h", ""),
    ("hf_erkennung", "s_er_alle", "s_er_alle_h", ""),
    ("hf_erkennung", "s_lj_schalter", "s_lj_schalter_h", ""),
    ("hf_spiel", "aafk_taste", "aafk_taste_eigene", ""),
    ("hf_spiel", "aafk_makro", "aafk_makro_h", ""),
];

/// Rows of [`PAGE_SETTINGS`] the real page shows only now and then, not as a setting: the menu filter does not
/// lead there on real data. (page, text key of the row.) "Welcher Account?" stands on the connection page only
/// while the check of the account stopped the sync.
const SITUATIONAL_ROWS: [(&str, &str); 1] = [("k_basetool_verbindung", "s_btv_konto")];

/// What the menu filter shows for `page` of `group`: the page itself when
/// its name or its group matches, and `Page › text` for every setting
/// (by name, hint, group title or a choice) and every info that matches.
/// With an empty filter: just the page.
#[cfg(test)]
fn nav_hits(filter: &str, group: &str, page: &str) -> Vec<String> {
    nav_hits_in(filter, group, page, true)
}

/// The hits of the menu filter, with the sample data of a page only when `sample`.
fn nav_hits_in(filter: &str, group: &str, page: &str, sample: bool) -> Vec<String> {
    use versekit_core::language::t;
    let filter = search_form(filter.trim());
    let name = t(page);
    if filter.is_empty() {
        return vec![name];
    }
    let matches = |text: &str| search_form(text).contains(&filter);
    let mut hits = Vec::new();
    if matches(&name) || matches(&t(group)) || search_words(page).iter().any(|word| matches(word)) {
        hits.push(name.clone());
    }
    for (title, rows) in searched_settings(page, sample) {
        for (row, hint, choices) in rows.iter() {
            if !sample && SITUATIONAL_ROWS.contains(&(page, *row)) {
                continue;
            }
            let row = label(row);
            if matches(&row)
                || matches(&label(hint))
                || matches(&label(title))
                || choices.iter().any(|c| matches(&label(c)))
            {
                hits.push(format!("{name} › {row}"));
            }
        }
    }
    for (_, row, hint, words) in MODULE_SETTINGS.iter().filter(|(p, ..)| *p == page && !sample) {
        let row = t(row);
        let hit = format!("{name} › {row}");
        let found = matches(&row) || matches(&t(hint)) || (!words.is_empty() && matches(&t(words)));
        if found && !hits.contains(&hit) {
            hits.push(hit);
        }
    }
    for info in search_infos(page, sample) {
        let hit = format!("{name} › {info}");
        if matches(&info) && !hits.contains(&hit) {
            hits.push(hit);
        }
    }
    for (_, shown, words) in SECTION_HITS.iter().filter(|(p, ..)| *p == page) {
        let hit = format!("{name} › {}", t(shown));
        if std::iter::once(shown).chain(words.iter()).any(|key| matches(&t(key))) && !hits.contains(&hit) {
            hits.push(hit);
        }
    }
    hits
}

/// The highlights of the sample update in the what's-new gallery: (icon, head
/// and text as language keys, page).
const NEWS: [(&str, &str, &str, &str); 3] = [
    ("sparkles", "s_mu_news1_k", "s_mu_news1_t", "hf_allgemein"),
    ("layout-grid", "s_mu_news2_k", "s_mu_news2_t", "hf_darstellung"),
    ("move", "s_mu_news3_k", "s_mu_news3_t", "hf_darstellung"),
];

/// How old the data of a page is in the sample: (seconds, date and time).
fn data_age_of(page: &str) -> Option<(f64, String)> {
    let today = |time: &str| tf("s_sp_heute", &[("0", Arg::from(time))]);
    Some(match page {
        "hf_meinlager" => (300.0, today("08:42")),
        "hf_raffinerien" => (90.0, today("08:45")),
        "hf_laeden" | "hf_routen" => (3.0 * 3_600.0, today("05:47")),
        "hf_serverstatus" => (2.0 * 86_400.0, "18.10. 14:05".to_string()),
        _ => return None,
    })
}

/// Whether an entry of the open page lights up: of several entries the
/// filter shows for it only the one clicked last, or the first when none of
/// them was clicked. `open_hits` are the entries of the open page.
fn nav_lit(shown: &str, open_hits: &[String], clicked: &str) -> bool {
    if open_hits.iter().any(|hit| hit == clicked) {
        shown == clicked
    } else {
        open_hits.first().is_some_and(|first| first == shown)
    }
}

/// The page id of 4.12 the usage counters name a page by; `None` for a page 4.12 does not
/// know (a page of an extension).
fn usage_page_id(key: &str) -> Option<&'static str> {
    match key {
        "" => Some("uebersicht"),
        "hf_wasistneu" => Some("wasistneu"),
        "hf_ueber" => Some("ueber"),
        "hf_diagnose" => Some("diagnose"),
        ASOP_PAGE => Some("asop"),
        // Pages the receiver of the counters does not know yet are not counted, even where they can be pinned.
        _ => versekit_core::overview::page_name(key).filter(|id| versekit_core::usage::PAGES.contains(id)),
    }
}

/// The merged page of the wreck and the dismantle calculation.
const SALVAGE_PAGE: &str = "hf_bergung";
/// The page "Schiffe benennen" of 4.12: no entry in the menu, it says where the names went.
const ASOP_PAGE: &str = "hf_asop";

/// The group a page belongs to; empty for the page at the top of the menu, which has none.
/// The menu group of the page `page`, for tools that open a page by its key.
pub fn group_of_page(page: &str) -> Option<&'static str> {
    group_of(page)
}

fn group_of(page: &str) -> Option<&'static str> {
    if page == TOP_PAGE.0 {
        return Some("");
    }
    NAV.iter().find(|(_, pages)| pages.iter().any(|(p, _)| *p == page)).map(|(g, _)| *g)
}

/// What a tile shows: its own text, or for a pinned page its icon, name
/// and the pinned mark.
fn tile_of(key: &str) -> (&'static str, String, String) {
    if let Some(sample) = overview_tile(key) {
        return sample;
    }
    // A tile of 4.12 (real data): its icon and title; its lines come from the counts.
    if let Some(title) = versekit_core::overview::tile_title(key) {
        return (start_page::tile_icon(key), t(title), String::new());
    }
    let icon = NAV.iter().flat_map(|(_, pages)| pages.iter()).chain([&TOP_PAGE]).find(|(p, _)| *p == key);
    let mut name = versekit_core::language::t(key);
    // The pages of the folded part "Fortgeschritten" say so on their card (`cards.ADVANCED`).
    if matches!(versekit_core::overview::page_name(key), Some("erkennung" | "startprogramme")) {
        name = format!("{name} · {}", t("k_fortgeschritten"));
    }
    (icon.map_or("pin", |(_, icon)| *icon), name, t("s_ke_geheftet"))
}

/// Draws a tile of the overview: the bigger, the more it shows, and no text is
/// ever cut: title and key line wrap (the row of the tile grows for them), and a
/// detail stands in full or not at all. Narrow: the key line, as many details as
/// there is room for, the button. Wide: the details (or the chart)
/// stand next to it. Returns the page and group when the open button was clicked,
/// and how much it showed (detail lines; a chart counts three). On real data
/// (`real`) line, details and chart come from the counts, not the samples.
fn overview_tile_content(
    tile: &mut egui::Ui,
    scheme: &Scheme,
    key: &str,
    real: Option<&glance::Overview>,
) -> (Option<(String, &'static str)>, usize, Option<glance::TileAction>) {
    let (icon, title, sample_line) = tile_of(key);
    let counted = real.map(|overview| overview.tile(key));
    let line = counted.as_ref().map_or(sample_line.as_str(), |glance| glance.line.as_str());
    let counted_details: Vec<(&str, glance::Tone)> = counted
        .as_ref()
        .map(|glance| glance.details.iter().enumerate().map(|(at, d)| (d.as_str(), glance.tone_of(at))).collect())
        .unwrap_or_default();
    let sample_details = tile_details(key);
    let sample_refs: Vec<(&str, glance::Tone)> =
        sample_details.iter().map(|d| (d.as_str(), glance::Tone::Plain)).collect();
    let details = if counted.is_some() { &counted_details[..] } else { &sample_refs[..] };
    let (line_tone, bar) =
        counted.as_ref().map_or((glance::Tone::Plain, None), |glance| (glance.line_tone, glance.bar));
    let (figure, steps) = counted.as_ref().map_or((None, None), |glance| (glance.figure.as_deref(), glance.steps));
    let weekdays = days();
    let points: Vec<(&str, f32)> = match real {
        Some(overview) => overview.week.iter().map(|(day, hours)| (weekdays[*day].as_str(), *hours)).collect(),
        None => HOURS.iter().enumerate().map(|(day, hours)| (weekdays[day].as_str(), *hours)).collect(),
    };
    tile.horizontal(|head| {
        if let Some(image) = icons::themed(icon, scheme, IconTint::Highlight, 18.0) {
            head.add(image);
        }
        // The hide button shows over the top right corner: the title keeps clear of it.
        head.scope(|name| {
            name.set_max_width((name.available_width() - tiles::HIDE_ROOM).max(40.0));
            name.add(egui::Label::new(RichText::new(title).strong()).wrap());
        });
    });
    let chart = key == "spielzeit";
    let room = tile.available_size();
    // A tile has no open button: a click on the tile opens its page (`tiles::clicked_tile`), dragging moves it.
    // A tile of one grid row gives its room to the lines.
    let short = room.y < SHORT_TILE;
    let mut shown = 0;
    let lamps = counted.as_ref().and_then(|glance| glance.lamps);
    let key_line = |ui: &mut egui::Ui| {
        // The key figure large and bold, as the big number of 4.12.
        tiles::key_line(ui, line, figure, tone_color(scheme, line_tone));
        if let Some(lamps) = &lamps {
            contested_page::lamp_dots(ui, scheme, lamps);
        }
        // A tile of one grid row gives its room to the lines: the share stands in the text as well.
        if let Some(share) = bar
            && !short
        {
            match steps {
                Some(steps) => tiles::step_bar(ui, scheme, share, steps),
                None => tiles::share_bar(ui, scheme, share),
            }
        }
    };
    if room.x >= WIDE_TILE {
        let half = ((room.x - GAP) / 2.0).max(1.0);
        // Each column is top-down: the layout inherited from the row is left to
        // right, which keeps long lines from being cut and runs them out of the tile.
        let column = egui::Layout::top_down(egui::Align::Min);
        tile.horizontal_top(|row| {
            let left = row.allocate_ui_with_layout(egui::vec2(half, room.y), column, |left| {
                left.set_width(half);
                key_line(left);
                // The chart takes the right half, the details of the play time stand under the line.
                detail_lines(left, scheme, details)
            });
            shown += left.inner;
            row.allocate_ui_with_layout(egui::vec2(half, room.y), column, |right| {
                right.set_width(half);
                if chart {
                    shown += chart_if_room(right, scheme, &points, 0.0);
                } else {
                    shown += detail_lines(right, scheme, &details[left.inner..]);
                }
            });
        });
    } else {
        key_line(tile);
        shown += detail_lines(tile, scheme, details);
        if chart {
            shown += chart_if_room(tile, scheme, &points, 0.0);
        }
    }
    // The link at the end of the tile, where the tile has one: it lies on the tile, so a click on it is its own.
    let mut asked = None;
    if let Some(action) = counted.as_ref().and_then(|glance| glance.action)
        && tile_action(tile, scheme, action)
    {
        asked = Some(action);
    }
    (None, shown, asked)
}

/// The link of a tile that does something on its own (accent text, the hand on hover, a ring on focus); true when
/// it was clicked.
fn tile_action(ui: &mut egui::Ui, scheme: &Scheme, action: glance::TileAction) -> bool {
    let text = match action {
        glance::TileAction::BackupNow => t("s_uv_sicherung_jetzt"),
        glance::TileAction::RefineryScan => t("s_uv_scannen"),
        glance::TileAction::TranslationCheck => t("s_uv_nachsehen"),
    };
    ui.add_space(2.0);
    let mut link = ui.add(egui::Link::new(RichText::new(text).color(color(scheme.accent))));
    // "Scannen" alone does not say what it reads, nor that it waits for the game.
    if action == glance::TileAction::RefineryScan {
        link = link.on_hover_text(t("s_uv_scannen_h"));
    }
    link.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// A tile with less room than this (below its head) is one grid row high.
const SHORT_TILE: f32 = 120.0;

/// From this width on a tile puts its details next to its key line.
const WIDE_TILE: f32 = 380.0;

/// As many of `details` as fit, each in full over as many lines as it takes (never cut). Returns how many it
/// drew.
fn detail_lines(ui: &mut egui::Ui, scheme: &Scheme, details: &[(&str, glance::Tone)]) -> usize {
    let room = ui.available_height();
    let (width, gap) = (ui.available_width(), ui.spacing().item_spacing.y);
    let mut used = 0.0;
    let mut count = 0;
    for (detail, tone) in details {
        let text = egui::WidgetText::from(RichText::new(format!("· {detail}")).color(tone_color(scheme, *tone)));
        let galley = text.into_galley(ui, Some(egui::TextWrapMode::Wrap), width, egui::TextStyle::Body);
        let height = galley.size().y + gap;
        if used + height > room {
            break;
        }
        ui.add(egui::Label::new(galley));
        used += height;
        count += 1;
    }
    count
}

/// The color of a line of a tile: plain text, the accent for what is fine, gold for what asks for a look, red for
/// what is wrong.
fn tone_color(scheme: &Scheme, tone: glance::Tone) -> egui::Color32 {
    match tone {
        glance::Tone::Plain => color(scheme.fg),
        glance::Tone::Muted => color(scheme.sub),
        glance::Tone::Good => color(scheme.accent),
        glance::Tone::Warn => color(scheme.gold),
        glance::Tone::Bad => color(scheme.red_text()),
    }
}

/// Height of the line under the head bar in the square scheme.
const HEAD_LINE_HEIGHT: f32 = 2.0;

/// The word of the yellow mark next to the version in the head bar, as a text key; `None` for the plain version.
/// On real data a run for testing (`language::test_run`: a test build, a copy of the data) says "Testfassung"
/// before anything else, else a version of the test channel of the updates says so; the sample page shows the
/// test version by its switch.
fn head_word(real: bool, test_build: bool, test_run: bool) -> Option<&'static str> {
    match (real, test_run, test_build) {
        (true, true, _) => Some("s_kopf_testfassung"),
        (_, _, true) => Some("s_ul_testversion"),
        _ => None,
    }
}

/// The accent line along the lower edge of the head bar `bar`, in the square scheme only; it lies inside the
/// bar, so the bar keeps its height.
fn head_line(bar: egui::Rect, scheme: &Scheme) -> Option<egui::Rect> {
    scheme
        .square
        .then(|| egui::Rect::from_min_max(egui::pos2(bar.left(), bar.bottom() - HEAD_LINE_HEIGHT), bar.right_bottom()))
}

/// The chart of the play time in the room left (less `keep` for what comes after it), if it is readable there.
fn chart_if_room(ui: &mut egui::Ui, scheme: &Scheme, points: &[(&str, f32)], keep: f32) -> usize {
    let mut room = ui.available_size();
    room.y -= keep;
    if room.y < widgets::CHART_MIN_HEIGHT || points.is_empty() {
        return 0;
    }
    // Short on the axis, and never `0 h` for a short time: 0,4 h is `24 min`.
    let lang = language::current();
    let format = move |h: f32| {
        if h < 1.0 {
            format!("{} min", (h * 60.0).round())
        } else {
            let digits = if (h - h.round()).abs() < 0.05 { 0 } else { 1 };
            format!("{} h", versekit_core::formats::fixed(lang, f64::from(h), digits))
        }
    };
    widgets::line_chart(ui, scheme, points, room, &format);
    3
}

/// The short version of a page for its tile, in the order it is cut:
/// sample texts here; each real page brings its own.
fn tile_details(key: &str) -> Vec<String> {
    // A name and its share, as `Rüstung 61 %`.
    let share = |name: &str, percent: u32| format!("{} {percent} %", t(name));
    let count = |key: &str, name: Arg, count: i64| tf(key, &[("name", name), ("count", Arg::from(count))]);
    let named = |key: &str, name: &str| tf(key, &[("name", Arg::from(name))]);
    let week = || tf("s_kd_diese_woche", &[("count", Arg::from(12))]);
    let texts = |keys: &[&str]| -> Vec<String> { keys.iter().map(|key| t(key)).collect() };
    match key {
        "hf_raffinerien" => texts(&["s_kd_raff1", "s_kd_raff2", "s_kd_raff3", "s_kd_raff4"]),
        "hf_meinlager" => {
            ["Area18: 180 SCU", "Lorville: 120 SCU", "Orison: 60 SCU", "GrimHEX: 52 SCU"].map(str::to_string).to_vec()
        }
        "hf_liste" => vec![
            week(),
            named("s_kd_neueste", &t("s_mu_n_arclight")),
            format!("{}, {}", share("s_mu_art_ruestung", 61), share("s_mu_art_waffen", 48)),
            tf("s_uv_fehlen", &[("count", Arg::from(404))]),
        ],
        "hf_fortschritt" => vec![
            share("s_mu_art_ruestung", 61),
            share("s_mu_art_waffen", 48),
            share("s_mu_art_schiffsteile", 33),
            share("s_mu_art_werkzeuge", 72),
            week(),
        ],
        "hf_auftragslog" => vec![
            named("s_kd_auftrag_fertig", "Retake the Outpost"),
            named("s_kd_auftrag_offen", "Protect Fuel Tanks"),
            named("s_kd_bauplan", "Arclight Pistol"),
            tf("s_kd_gestern", &[("count", Arg::from(7))]),
        ],
        "hf_farmliste" => vec![
            count("s_kd_fehlt_scu", Arg::from("Titanium"), 40),
            count("s_kd_fehlt_scu", Arg::from("Quantainium"), 8),
            count("s_kd_fehlt_scu", Arg::from("Gold"), 3),
        ],
        "hf_hangar" => vec![
            tf("s_kt_hangar_lti", &[("0", Arg::from(3))]),
            tf("s_kt_hangar_bereit", &[("0", Arg::from(2))]),
            tf("s_kt_hangar_geplant", &[("0", Arg::from(4))]),
        ],
        "hf_serverstatus" => texts(&["s_kd_srv_spiel", "s_kd_srv_login", "s_kd_srv_basetool", "s_kd_srv_update"]),
        "hf_routen" => ROUTES.iter().map(|(name, _)| name.to_string()).collect(),
        "spielzeit" => vec![
            tf("s_uv_heute_h", &[("hours", Arg::from(number(2.5, 1)))]),
            tf("s_uv_laengste", &[("time", Arg::from("4 h 10 min"))]),
        ],
        _ => texts(&["s_kd_kurzfassung"]),
    }
}

/// What a tile of the overview shows: (icon, title, line).
fn overview_tile(key: &str) -> Option<(&'static str, String, String)> {
    let of = |key: &str, have: i64, total: i64| tf(key, &[("have", Arg::from(have)), ("total", Arg::from(total))]);
    Some(match key {
        "hf_raffinerien" => ("factory", t(key), t("s_kd_raff_zeile")),
        "hf_meinlager" => {
            ("package", t(key), tf("s_kd_lager_zeile", &[("scu", Arg::from(412)), ("places", Arg::from(6))]))
        }
        "hf_liste" => ("clipboard-list", t(key), of("s_kd_von", 318, 722)),
        "hf_hangar" => ("plane", t(key), tf("s_kt_hangar_zahl", &[("0", Arg::from(7))])),
        "hf_serverstatus" => ("server", t(key), t("s_kd_srv_alle")),
        "hf_routen" => {
            let best = versekit_core::formats::count(language::current(), 41_200);
            ("route", t(key), tf("s_kd_beste", &[("value", Arg::from(best))]))
        }
        "spielzeit" => {
            ("chart-spline", t("s_uv_spielzeit"), tf("s_uv_sieben_h", &[("hours", Arg::from(number(20.5, 1)))]))
        }
        "hf_fortschritt" => ("chart-column", t(key), of("s_uv_von_bp", 318, 722)),
        "hf_auftragslog" => ("notebook-pen", t(key), tf("s_uv_offen", &[("open", Arg::from(3))])),
        "hf_farmliste" => ("diamond", t(key), tf("s_kd_rohstoffe", &[("count", Arg::from(3))])),
        _ => return None,
    })
}

/// The check of the quality field, as in the refinery window.
fn quality_error(text: &str) -> Option<&'static str> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    match text.parse::<i64>() {
        Ok(q) if (0..=1000).contains(&q) => None,
        Ok(_) => Some("s_rf_qualitaet"),
        Err(_) => Some("s_mu_q_zahl"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A run for testing says "Testfassung" in the head bar before the test channel of the updates, and only on
    /// real data; the sample page keeps its switch.
    #[test]
    fn the_head_bar_says_test_build_for_a_run_for_testing() {
        assert_eq!(head_word(true, false, false), None, "the player's own data: the plain version");
        assert_eq!(head_word(true, false, true), Some("s_kopf_testfassung"));
        assert_eq!(head_word(true, true, true), Some("s_kopf_testfassung"), "the test run comes first");
        assert_eq!(head_word(true, true, false), Some("s_ul_testversion"));
        assert_eq!(head_word(false, true, false), Some("s_ul_testversion"), "the sample page and its switch");
        assert_eq!(head_word(false, false, true), None);
        for key in ["s_kopf_testfassung", "s_kopf_testfassung_h"] {
            for lang in [versekit_core::language::Lang::De, versekit_core::language::Lang::En] {
                assert!(versekit_core::language::text_in(lang, key).is_some_and(|t| !t.is_empty()), "{key} {lang:?}");
            }
        }
    }

    /// The overlay closes the main window with itself, and a signal is done
    /// once: the frames after it close nothing.
    #[test]
    fn the_main_window_closes_when_the_overlay_asks_once() {
        let ctx = egui::Context::default();
        let mut page = SamplePage::preview();
        let signals = MainSignals::default();
        let closes = |signals: &MainSignals, page: &mut SamplePage| {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| signals.apply(ui.ctx(), page));
            output.textures_delta.clear();
            let root = output.viewport_output.get(&egui::ViewportId::ROOT);
            root.is_some_and(|v| v.commands.contains(&egui::ViewportCommand::Close))
        };
        signals.note(Signal::Settings);
        signals.note(Signal::Overview);
        assert!(!closes(&signals, &mut page), "settings and overview leave the window open");
        assert!(!signals.ended());
        signals.note(Signal::Close);
        assert!(closes(&signals, &mut page), "the overlay closed: the main window closes");
        assert!(signals.ended(), "and its closing is no hiding");
        assert!(!closes(&signals, &mut page), "done once");
        // A restore is starting the program anew: the overlay closes first and asks the window to go with it,
        // and the window stays (it starts the program again), and the request is gone afterwards.
        signals.keep_open(true);
        signals.note(Signal::Close);
        assert!(!closes(&signals, &mut page), "the window that restarts the program does not close");
        signals.keep_open(false);
        assert!(!closes(&signals, &mut page), "and the request is not kept for later");
    }

    /// A window the player closed is only hidden: the overview and the pages the overlay asks for show it again,
    /// a change of the settings does not.
    #[test]
    fn what_the_overlay_asks_for_shows_a_hidden_window_again() {
        let ctx = egui::Context::default();
        let mut page = SamplePage::preview();
        let signals = MainSignals::default();
        let shows = |signals: &MainSignals, page: &mut SamplePage| {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| signals.apply(ui.ctx(), page));
            output.textures_delta.clear();
            let root = output.viewport_output.get(&egui::ViewportId::ROOT);
            root.is_some_and(|v| v.commands.contains(&egui::ViewportCommand::Visible(true)))
        };
        signals.note(Signal::Settings);
        assert!(!shows(&signals, &mut page), "the settings changed: the window stays as it is");
        signals.note(Signal::Overview);
        assert!(shows(&signals, &mut page), "the gear, the icon and a second start ask for the overview");
        assert!(!shows(&signals, &mut page), "done once");
        for signal in
            [Signal::OpenGeneral, Signal::OpenTranslation, Signal::OpenAbout, Signal::OpenMining, Signal::OpenStockScan]
        {
            signals.note(signal);
            assert!(shows(&signals, &mut page), "{signal:?}");
        }
    }

    /// The hotkey of the main window shows a hidden window again and leaves it on the page it shows, once.
    #[test]
    fn the_hotkey_of_the_main_window_shows_it_on_its_page() {
        let ctx = egui::Context::default();
        let mut page = SamplePage::preview();
        let signals = MainSignals::default();
        let shows = |signals: &MainSignals, page: &mut SamplePage| {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| signals.apply(ui.ctx(), page));
            output.textures_delta.clear();
            let root = output.viewport_output.get(&egui::ViewportId::ROOT);
            root.is_some_and(|v| v.commands.contains(&egui::ViewportCommand::Visible(true)))
        };
        signals.note(Signal::OpenAbout);
        assert!(shows(&signals, &mut page));
        assert_eq!(page.page, "hf_ueber");
        page.usage_route = None;
        signals.note(Signal::Show);
        assert!(shows(&signals, &mut page), "the hidden window is shown");
        assert_eq!(page.page, "hf_ueber", "on the page it showed, not the overview");
        assert_eq!(page.usage_route, None, "no page opened, none counted");
        assert!(!shows(&signals, &mut page), "done once");
    }

    /// The entries of the icon's menu in the overlay open their page in the main window, once.
    #[test]
    fn the_overlay_opens_a_page_of_the_main_window() {
        let ctx = egui::Context::default();
        let mut page = SamplePage::preview();
        let signals = MainSignals::default();
        let apply = |signals: &MainSignals, page: &mut SamplePage| {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| signals.apply(ui.ctx(), page));
            output.textures_delta.clear();
        };
        for (signal, key) in [
            (Signal::OpenGeneral, "hf_allgemein"),
            (Signal::OpenTranslation, "hf_uebersetzung"),
            (Signal::OpenAbout, "hf_ueber"),
            (Signal::OpenMining, "hf_bergbau"),
            (Signal::OpenStockScan, "hf_meinlager"),
        ] {
            signals.note(signal);
            apply(&signals, &mut page);
            assert_eq!(page.page, key, "{signal:?}");
            page.open_overview();
            apply(&signals, &mut page);
            assert_eq!(page.page, "", "{signal:?} is done once");
        }
        // Two requests in one frame: the last one counts.
        signals.note(Signal::OpenAbout);
        signals.note(Signal::OpenTranslation);
        apply(&signals, &mut page);
        assert_eq!(page.page, "hf_uebersetzung");
        // For the usage counters the page came by the overlay; nothing asked, no way.
        assert_eq!(page.usage_route.take(), Some("overlay"));
        apply(&signals, &mut page);
        assert_eq!(page.usage_route, None);
        signals.note(Signal::Overview);
        apply(&signals, &mut page);
        assert_eq!(page.usage_route, Some("overlay"));
    }

    /// The old page names of the storage open my storage; the cargo hold brings its choice along, the goods for sale
    /// in the view Material (4.12 `my_stock_page.PRESETS`).
    #[test]
    fn the_old_cargo_hold_opens_my_storage_on_the_goods_for_sale() {
        let ctx = egui::Context::default();
        let mut page = SamplePage::preview();
        page.stock.apply_preset(versekit_core::overview::Preset { purpose: "craft", view: "items" });
        assert_eq!(page.stock.view_and_purpose(), (true, 1));
        assert!(page.open_page_named(&ctx, "lager"));
        assert_eq!(page.page, "hf_meinlager");
        assert_eq!(page.stock.view_and_purpose(), (true, 1), "the old material storage brings no choice");
        page.open_overview();
        assert!(page.open_page_named(&ctx, "handelslager"));
        assert_eq!(page.page, "hf_meinlager");
        assert_eq!(page.stock.view_and_purpose(), (false, 2), "view Material, purpose for sale");
        assert!(page.open_page_named(&ctx, "hangar"));
        assert_eq!(page.page, "hf_hangar");
        assert!(!page.open_page_named(&ctx, "gibtsnicht"));
        assert_eq!(page.page, "hf_hangar", "a name that is no page changes nothing");
    }

    /// The rows of game folders are every channel of the one list, in its order: LIVE, HOTFIX, PTU, EPTU,
    /// TECH-PREVIEW.
    #[test]
    fn the_rows_of_game_folders_are_the_channels_of_the_one_list() {
        let rows: Vec<&str> = settings_of("hf_ordner")[0].1.iter().map(|(name, ..)| *name).collect();
        assert_eq!(rows, versekit_core::paths::CHANNELS);
        assert_eq!(CHANNEL_ROWS, versekit_core::paths::CHANNELS);
    }

    /// The usage counters name the pages by the ids of 4.12, also those without an entry there.
    #[test]
    fn the_usage_counters_name_pages_as_4_12() {
        assert_eq!(usage_page_id(""), Some("uebersicht"));
        assert_eq!(usage_page_id("hf_wasistneu"), Some("wasistneu"));
        assert_eq!(usage_page_id("hf_ueber"), Some("ueber"));
        assert_eq!(usage_page_id("hf_diagnose"), Some("diagnose"));
        assert_eq!(usage_page_id("hf_meinlager"), Some("meinlager"));
        assert_eq!(usage_page_id("eigene_seite"), None);
        // Every page of the side bar has an id the receiver takes, the handle search included.
        for (_, pages) in NAV {
            for (page, _) in pages.iter() {
                let id = usage_page_id(page).unwrap_or_else(|| panic!("{page} has no id"));
                assert!(versekit_core::usage::PAGES.contains(&id), "{page} → {id}");
            }
        }
    }

    /// A window opens where it was: pixels back to points of the system
    /// scaling, never below the smallest window, centered when its place is
    /// on no screen any more.
    #[test]
    fn the_window_opens_where_it_was() {
        let place = WindowPlace { size: (1500, 1125), position: Some((-1920, 40)), maximized: true };
        let open = opening(&place, 1.25, (880.0, 380.0), |_, _, _| true);
        assert_eq!(open.size, [1200.0, 900.0]);
        assert_eq!(open.position, Some([-1536.0, 32.0]));
        assert!(open.maximized);
        let gone = opening(&place, 1.25, (880.0, 380.0), |_, _, _| false);
        assert_eq!(gone.position, None, "no screen there: centered");
        let tiny = WindowPlace { size: (300, 200), position: None, maximized: false };
        assert_eq!(opening(&tiny, 1.0, (880.0, 380.0), |_, _, _| true).size, [880.0, 380.0]);
        assert_eq!(two_numbers(" 1229x1186", 'x'), Some((1229, 1186)));
        assert_eq!(two_numbers("-1920,40", ','), Some((-1920, 40)));
        assert_eq!(two_numbers("breit", 'x'), None);
    }

    /// A window remembered larger than its screen opens at most as large as the work area, with the whole window
    /// and its title bar on it; one that fits stays as it was.
    #[test]
    fn a_window_larger_than_its_screen_opens_inside_the_work_area() {
        // The screen of the report: 3844 × 2221 pixels at 150 %, the task bar below.
        let work = Some((0, 0, 3844, 2161));
        let big = Opening { size: [2800.0, 1600.0], position: Some([-10.0, -40.0]), maximized: false };
        let fit = fit_to_work_area(big, work, 1.5);
        assert!(fit.size[0] <= 3844.0 / 1.5 && fit.size[1] <= 2161.0 / 1.5, "{fit:?}");
        let [x, y] = fit.position.expect("a place");
        assert!(x >= 0.0 && y >= 0.0, "the title bar on the screen: {fit:?}");
        assert!(x + fit.size[0] <= 3844.0 / 1.5 + 0.01 && y + fit.size[1] <= 2161.0 / 1.5 + 0.01, "{fit:?}");
        let small = Opening { size: [1200.0, 800.0], position: Some([100.0, 80.0]), maximized: false };
        assert_eq!(fit_to_work_area(small, work, 1.5), small, "a window that fits stays as it was");
        assert_eq!(fit_to_work_area(big, None, 1.5), big, "no screen known: unchanged");
    }

    /// A place from another machine on a Retina screen (scale 2.0): kept where its title bar
    /// lies on the screen, centered where it lies on none (a monitor that is not there).
    #[test]
    fn a_place_from_another_machine_opens_only_where_a_screen_is() {
        use versekit_core::screen::{self, Monitor, Setup};
        let area = (0, 0, 3420, 2214);
        let _screens = screen::fixed(Setup {
            scale: 2.0,
            primary: 0,
            monitors: vec![Monitor { area, work: (0, 70, 3420, 2034) }],
        });
        let smallest = (880.0, 380.0);
        let seen = WindowPlace { size: (2000, 1200), position: Some((600, 300)), maximized: false };
        let open = opening(&seen, screen::system_scale(), smallest, screen::on_a_screen);
        assert_eq!((open.size, open.position), ([1000.0, 600.0], Some([300.0, 150.0])));
        // A second PC monitor to the left, and one far to the right: neither exists here.
        for x in [-3000, 5000] {
            let lost = WindowPlace { position: Some((x, 40)), ..seen };
            let open = opening(&lost, screen::system_scale(), smallest, screen::on_a_screen);
            assert_eq!(open.position, None, "{x}: centered");
            assert_eq!(open.size, [1000.0, 600.0], "the size stays");
        }
    }

    /// The window place is written once the window stood still, in screen
    /// pixels like 4.12, and the size before maximizing stays.
    #[test]
    fn the_window_place_is_remembered() {
        use versekit_core::paths::Host;
        use versekit_core::pypath::Flavor;

        let dir = std::env::temp_dir().join(format!("versekit-fenster-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dir_text = dir.to_string_lossy().into_owned();
        let file = dir.join("einstellungen.json");
        std::fs::write(&file, "{\n  \"fenster_groesse\": \"1229x1186\"\n}\n").unwrap();
        let host = Host::new(Flavor::native(), &[("SC_BP_HOME", &dir_text)], &dir_text);
        let mut page = SamplePage::with_settings(Paths::new(host));
        assert_eq!(page.window_place().map(|p| p.size), Some((1229, 1186)));
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let frame = |time: f64, inner: egui::Rect, maximized: bool| {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, inner.size())),
                time: Some(time),
                ..Default::default()
            };
            let info = input.viewports.entry(egui::ViewportId::ROOT).or_default();
            info.inner_rect = Some(inner);
            info.outer_rect = Some(inner.translate(egui::vec2(-8.0, -31.0)));
            info.maximized = Some(maximized);
            info.native_pixels_per_point = Some(1.0);
            input
        };
        let moved = egui::Rect::from_min_size(egui::pos2(108.0, 131.0), egui::vec2(1400.0, 900.0));
        for (k, time) in [0.0, 0.1, 0.2, 0.8, 0.9].into_iter().enumerate() {
            let mut output = ctx.run_ui(frame(time, moved, false), |ui| {
                page.show(ui);
            });
            output.textures_delta.clear();
            if k == 1 {
                assert_eq!(page.window_place().map(|p| p.size), Some((1229, 1186)), "not yet still");
            }
        }
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("\"fenster_groesse\": \"1400x900\""), "{text}");
        assert!(text.contains("\"fenster_lage\": \"100,100\""), "{text}");
        // Maximized: the size from before stays, only the mark is written.
        let full = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(2560.0, 1400.0));
        for time in [1.0, 1.1, 1.7, 1.8] {
            let mut output = ctx.run_ui(frame(time, full, true), |ui| {
                page.show(ui);
            });
            output.textures_delta.clear();
        }
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(
            text.contains("\"fenster_groesse\": \"1400x900\"") && text.contains("\"fenster_maximiert\": true"),
            "{text}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_window_whose_top_edge_is_off_its_screen_is_brought_back() {
        let work = (0, 0, 1920, 1040);
        // Above the screen (the title bar is out of reach): down to the top of the work area, sideways as it was.
        assert_eq!(top_correction((100, -70, 1400, 900), work), Some((100, 0)));
        assert_eq!(top_correction((100, -600, 1400, 1200), work), Some((100, 0)));
        // On the screen: left alone, also when it is taller than the screen or sticks out below.
        assert_eq!(top_correction((100, 0, 1400, 900), work), None);
        assert_eq!(top_correction((100, 40, 1400, 1200), work), None);
        // Below the screen only a bar of the title stays in reach: up to it.
        assert_eq!(top_correction((100, 1500, 1400, 900), work), Some((100, 1000)));
        // Far to the side: at least a bar of 40 pixels stays on the screen.
        assert_eq!(top_correction((-1500, 50, 1400, 900), work), Some((-1360, 50)));
        assert_eq!(top_correction((2500, 50, 1400, 900), work), Some((1880, 50)));
        // A screen left of the main one, with negative places.
        assert_eq!(top_correction((-1800, -20, 1400, 900), (-1920, 0, 1920, 1040)), Some((-1800, 0)));
        assert_eq!(top_correction((-1800, 10, 1400, 900), (-1920, 0, 1920, 1040)), None);
        // No work area known: nothing is moved.
        assert_eq!(top_correction((100, -70, 1400, 900), (0, 0, 0, 0)), None);
    }

    /// Storing the place and opening there again, over and over, does not move the window: it comes back where it
    /// was, and one that was stored above the screen is on it after the first start and stays.
    #[test]
    fn the_place_of_the_window_does_not_wander_when_stored_and_opened_again_and_again() {
        let work = (0, 0, 1920, 1040);
        for scale in [1.0_f32, 1.25, 1.5, 2.0] {
            for stored in [(100, 100), (0, 0), (-8, -31), (300, -70), (1500, 900)] {
                let mut place = WindowPlace { size: (1400, 900), position: Some(stored), maximized: false };
                let mut seen = Vec::new();
                for _ in 0..5 {
                    let open = opening(&place, scale, (800.0, 600.0), |_, _, _| true);
                    let open = fit_to_work_area(open, Some(work), scale);
                    let [x, y] = open.position.expect("a place");
                    // The window stands there; its frame is stored as `follow_window` does.
                    place.position = Some(((x * scale).round() as i64, (y * scale).round() as i64));
                    seen.push(place.position);
                }
                assert!(seen.iter().all(|p| *p == seen[0]), "scale {scale}, stored {stored:?}: {seen:?}");
                let (_, y) = seen[0].expect("a place");
                assert!(y >= 0, "scale {scale}, stored {stored:?}: the top edge is on the screen: {seen:?}");
            }
        }
    }

    /// One frame of the preview page at `time` with `events` and the window maximized or not: the commands it gives
    /// the window.
    fn frame_commands(
        ctx: &egui::Context,
        page: &mut SamplePage,
        time: f64,
        maximized: bool,
        events: Vec<egui::Event>,
    ) -> Vec<egui::ViewportCommand> {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1300.0, 800.0))),
            time: Some(time),
            events,
            ..Default::default()
        };
        input.viewports.entry(egui::ViewportId::ROOT).or_default().maximized = Some(maximized);
        let mut output = ctx.run_ui(input, |ui| {
            page.show(ui);
        });
        output.textures_delta.clear();
        output.viewport_output.get(&egui::ViewportId::ROOT).map(|v| v.commands.clone()).unwrap_or_default()
    }

    fn press(at: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// A page with the frame of its own after two quiet frames, and the rectangles of the buttons it drew
    /// (minimize, maximize, close).
    fn framed_page() -> (egui::Context, SamplePage, Vec<egui::Rect>) {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let mut page = SamplePage::preview();
        page.use_own_frame(true);
        frame_commands(&ctx, &mut page, 0.0, false, Vec::new());
        frame_commands(&ctx, &mut page, 0.1, false, Vec::new());
        let rects = ctx.data(|d| d.get_temp::<Vec<egui::Rect>>(egui::Id::unique(FRAME_BUTTONS_ID))).unwrap_or_default();
        (ctx, page, rects)
    }

    /// A click at `at`: the commands of the frame of the release.
    fn click_at(
        ctx: &egui::Context,
        page: &mut SamplePage,
        at: egui::Pos2,
        time: f64,
        maximized: bool,
    ) -> Vec<egui::ViewportCommand> {
        frame_commands(ctx, page, time, maximized, vec![egui::Event::PointerMoved(at), press(at, true)]);
        frame_commands(ctx, page, time + 0.05, maximized, vec![press(at, false)])
    }

    #[test]
    fn the_head_bar_is_the_title_bar_with_three_buttons_that_do_what_they_say() {
        use egui::ViewportCommand as C;
        let _language = crate::language_lock();
        language::set_thread_language(language::Choice::De);
        let (ctx, mut page, rects) = framed_page();
        assert_eq!(rects.len(), 3, "minimize, maximize, close");
        // As wide as asked and as high as the bar, side by side to the right edge of the window.
        for rect in &rects {
            assert!((rect.width() - widgets::CAPTION_WIDTH).abs() < 0.5 && (rect.height() - widgets::BAR_HEIGHT).abs() < 0.5);
        }
        assert!(rects[0].right() <= rects[1].left() + 0.5 && rects[1].right() <= rects[2].left() + 0.5);
        // The bar stands in the frame of the page, a few points from the edge.
        assert!((1300.0 - rects[2].right()).abs() < 8.0, "the close button ends at the edge: {:?}", rects[2]);
        // Each click gives its own command.
        assert_eq!(click_at(&ctx, &mut page, rects[0].center(), 1.0, false), vec![C::Minimized(true)]);
        assert_eq!(click_at(&ctx, &mut page, rects[1].center(), 2.0, false), vec![C::Maximized(true)]);
        assert_eq!(click_at(&ctx, &mut page, rects[2].center(), 3.0, false), vec![C::Close]);
        // Maximized, the same button restores.
        assert_eq!(click_at(&ctx, &mut page, rects[1].center(), 4.0, true), vec![C::Maximized(false)]);
    }

    /// Windows 11 asks the window where its maximize button is (snap layouts): the frame tells it in pixels of the
    /// client area at every frame, a click that arrives as a click on the title bar maximizes and restores like
    /// the click on the button, and the hover said by the system lights the button up.
    #[test]
    fn the_maximize_button_tells_the_system_where_it_is_and_takes_its_click() {
        use egui::ViewportCommand as C;
        let _language = crate::language_lock();
        language::set_thread_language(language::Choice::De);
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let button = versekit_core::snap_layouts::MaxButton::new();
        ctx.data_mut(|d| d.insert_temp(egui::Id::unique(SNAP_BUTTON_ID), button.clone()));
        let mut page = SamplePage::preview();
        page.use_own_frame(true);
        frame_commands(&ctx, &mut page, 0.0, false, Vec::new());
        frame_commands(&ctx, &mut page, 0.1, false, Vec::new());
        let rects = ctx.data(|d| d.get_temp::<Vec<egui::Rect>>(egui::Id::unique(FRAME_BUTTONS_ID))).unwrap_or_default();
        let (maximize, close) = (rects[1], rects[2]);
        let scale = ctx.pixels_per_point();
        let at = |p: egui::Pos2| ((p.x * scale).round() as i32, (p.y * scale).round() as i32);
        let (x, y) = at(maximize.center());
        assert!(button.contains(x, y), "the middle of the maximize button is the button");
        let (x, y) = at(close.center());
        assert!(!button.contains(x, y), "the close button is not");
        let (x, y) = at(rects[0].center());
        assert!(!button.contains(x, y), "the minimize button is not");
        // A click the system took for the title bar: the frame maximizes, and restores when it is maximized.
        button.request_toggle();
        assert_eq!(frame_commands(&ctx, &mut page, 0.2, false, Vec::new()), vec![C::Maximized(true)]);
        assert!(frame_commands(&ctx, &mut page, 0.3, false, Vec::new()).is_empty(), "once");
        button.request_toggle();
        assert_eq!(frame_commands(&ctx, &mut page, 0.4, true, Vec::new()), vec![C::Maximized(false)]);
    }

    #[test]
    fn a_drag_on_the_free_bar_moves_the_window_and_one_on_a_button_does_not() {
        use egui::ViewportCommand as C;
        let _language = crate::language_lock();
        language::set_thread_language(language::Choice::De);
        let (ctx, mut page, rects) = framed_page();
        let y = rects[0].center().y;
        let drag = |ctx: &egui::Context, page: &mut SamplePage, from: egui::Pos2, time: f64| {
            frame_commands(ctx, page, time, false, vec![egui::Event::PointerMoved(from), press(from, true)]);
            let mut commands = frame_commands(ctx, page, time + 0.05, false, vec![egui::Event::PointerMoved(from + egui::vec2(25.0, 0.0))]);
            commands.extend(frame_commands(ctx, page, time + 0.1, false, vec![press(from + egui::vec2(25.0, 0.0), false)]));
            commands
        };
        // The free part between the name and the buttons on the right.
        let free = egui::pos2(rects[0].left() - 250.0, y);
        assert!(drag(&ctx, &mut page, free, 1.0).contains(&C::StartDrag), "the free bar moves the window");
        // Not from a button: the maximize button is no handle.
        let on_button = rects[1].center();
        assert!(!drag(&ctx, &mut page, on_button, 2.0).contains(&C::StartDrag), "a button is no handle");
        // A double click on the free bar maximizes, restores when it is maximized; on a button it does not.
        let double = |ctx: &egui::Context, page: &mut SamplePage, at: egui::Pos2, time: f64, maximized: bool| {
            let mut commands = click_at(ctx, page, at, time, maximized);
            commands.extend(click_at(ctx, page, at, time + 0.12, maximized));
            commands
        };
        assert!(double(&ctx, &mut page, free, 3.0, false).contains(&C::Maximized(true)));
        assert!(double(&ctx, &mut page, free, 4.0, true).contains(&C::Maximized(false)));
        assert!(!double(&ctx, &mut page, rects[0].center(), 5.0, false).iter().any(|c| matches!(c, C::Maximized(_))));
    }

    #[test]
    fn the_edges_of_the_window_resize_it_unless_it_is_maximized_or_has_a_frame_of_the_system() {
        use egui::ViewportCommand as C;
        use egui::viewport::ResizeDirection as D;
        let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1300.0, 800.0));
        for (at, direction) in [
            (egui::pos2(650.0, 1.0), Some(D::North)),
            (egui::pos2(650.0, 798.0), Some(D::South)),
            (egui::pos2(1.0, 400.0), Some(D::West)),
            (egui::pos2(1298.0, 400.0), Some(D::East)),
            (egui::pos2(1.0, 1.0), Some(D::NorthWest)),
            (egui::pos2(1298.0, 2.0), Some(D::NorthEast)),
            (egui::pos2(2.0, 798.0), Some(D::SouthWest)),
            (egui::pos2(1298.0, 798.0), Some(D::SouthEast)),
            (egui::pos2(8.0, 1.0), Some(D::NorthWest)),
            (egui::pos2(650.0, 400.0), None),
            (egui::pos2(6.0, 400.0), None),
            (egui::pos2(-3.0, 400.0), None),
        ] {
            assert_eq!(edge_at(at, window), direction, "{at:?}");
        }
        let (ctx, mut page, _) = framed_page();
        let west = egui::pos2(1.0, 400.0);
        let commands = frame_commands(&ctx, &mut page, 1.0, false, vec![egui::Event::PointerMoved(west), press(west, true)]);
        assert!(commands.contains(&C::BeginResize(D::West)), "{commands:?}");
        // Maximized: nothing to resize.
        let commands = frame_commands(&ctx, &mut page, 2.0, true, vec![egui::Event::PointerMoved(west), press(west, true)]);
        assert!(!commands.iter().any(|c| matches!(c, C::BeginResize(_))), "{commands:?}");
        // The frame of the system stays where the page does not draw its own.
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let mut system = SamplePage::preview();
        let commands = frame_commands(&ctx, &mut system, 1.0, false, vec![egui::Event::PointerMoved(west), press(west, true)]);
        assert!(!commands.iter().any(|c| matches!(c, C::BeginResize(_) | C::StartDrag)));
        assert!(ctx.data(|d| d.get_temp::<Vec<egui::Rect>>(egui::Id::unique(FRAME_BUTTONS_ID))).is_none(), "no buttons");
    }

    /// The texts one frame of the preview page draws.
    fn preview_texts(page: &mut SamplePage, ctx: &egui::Context) -> Vec<String> {
        fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => out.push(text.galley.text().to_string()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let mut texts = Vec::new();
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1300.0, 800.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                page.show(ui);
            });
            output.textures_delta.clear();
            texts.clear();
            output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut texts));
        }
        texts
    }

    /// While a reading of the game archive waits for the game to close, the page says so, and not otherwise.
    #[test]
    fn a_page_says_that_the_reading_waits_for_the_game_to_close() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::time::Duration;

        let _language = crate::language_lock();
        language::set_thread_language(language::Choice::De);
        let mut page = SamplePage::preview();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let said = t("s_ak_wartet");
        page.page = "hf_uebersetzung".to_string();
        assert!(!preview_texts(&mut page, &ctx).contains(&said), "nothing waits");
        let closed = Arc::new(AtomicBool::new(false));
        let reader = {
            let closed = Arc::clone(&closed);
            std::thread::spawn(move || {
                let _topic = versekit_core::archive_lock::reading_for("texts");
                versekit_core::archive_lock::wait_with(&|| closed.load(Ordering::SeqCst), Duration::from_millis(5));
            })
        };
        let started = std::time::Instant::now();
        while !versekit_core::archive_lock::waiting_for("texts") && started.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(preview_texts(&mut page, &ctx).contains(&said), "a reading for this page waits");
        // The page of another reading, and a page that reads nothing, stay quiet.
        for other in ["hf_joysticks", "hf_liste", "hf_notizen"] {
            page.page = other.to_string();
            assert!(!preview_texts(&mut page, &ctx).contains(&said), "{other}");
        }
        page.page = "hf_uebersetzung".to_string();
        closed.store(true, Ordering::SeqCst);
        reader.join().expect("the reading ended");
        assert!(!preview_texts(&mut page, &ctx).contains(&said), "the reading went on");
    }

    /// The head bar shows the icon of the program before its name.
    #[test]
    fn the_head_bar_shows_the_icon_of_the_program_before_the_name() {
        let _language = crate::language_lock();
        let mut page = SamplePage::preview();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let icon = crate::window_look::program_texture(&ctx).id();
        let (mut icon_at, mut name_at) = (None, None);
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1300.0, 800.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                page.show(ui);
            });
            output.textures_delta.clear();
            fn walk(shape: &egui::Shape, icon: egui::TextureId, found: &mut (Option<egui::Rect>, Option<egui::Pos2>)) {
                match shape {
                    // A picture is a rectangle painted with its texture.
                    egui::Shape::Rect(rect) if rect.brush.as_ref().is_some_and(|b| b.fill_texture_id == icon) => {
                        found.0 = Some(rect.rect);
                    }
                    egui::Shape::Mesh(mesh) if mesh.texture_id == icon => {
                        found.0 = Some(egui::Rect::from_points(&mesh.vertices.iter().map(|v| v.pos).collect::<Vec<_>>()));
                    }
                    egui::Shape::Text(text) if found.1.is_none() && text.galley.text() == t("hf_titel") => {
                        found.1 = Some(text.pos);
                    }
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, icon, found)),
                    _ => {}
                }
            }
            let mut found = (None, None);
            output.shapes.iter().for_each(|clipped| walk(&clipped.shape, icon, &mut found));
            (icon_at, name_at) = found;
        }
        let icon_at = icon_at.expect("the icon of the program is drawn");
        let name_at = name_at.expect("the name is drawn");
        assert!(icon_at.right() <= name_at.x + 0.5, "the icon stands before the name: {icon_at:?} {name_at:?}");
        assert!(icon_at.height() >= 16.0 && icon_at.top() < 60.0, "in the head bar, not too small: {icon_at:?}");
    }

    /// A size that has not settled yet when the window closes is written before it goes.
    #[test]
    fn the_window_size_of_the_last_moment_counts_when_the_window_closes() {
        use versekit_core::paths::Host;
        use versekit_core::pypath::Flavor;

        let dir = std::env::temp_dir().join(format!("versekit-fenster-zu-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dir_text = dir.to_string_lossy().into_owned();
        let file = dir.join("einstellungen.json");
        std::fs::write(&file, "{\n  \"fenster_groesse\": \"1229x1186\"\n}\n").unwrap();
        let host = Host::new(Flavor::native(), &[("SC_BP_HOME", &dir_text)], &dir_text);
        let mut page = SamplePage::with_settings(Paths::new(host));
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let frame = |time: f64, inner: egui::Rect, close: bool| {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, inner.size())),
                time: Some(time),
                ..Default::default()
            };
            let info = input.viewports.entry(egui::ViewportId::ROOT).or_default();
            info.inner_rect = Some(inner);
            info.outer_rect = Some(inner.translate(egui::vec2(-8.0, -31.0)));
            info.maximized = Some(false);
            info.native_pixels_per_point = Some(1.0);
            if close {
                info.events.push(egui::ViewportEvent::Close);
            }
            input
        };
        let first = egui::Rect::from_min_size(egui::pos2(108.0, 131.0), egui::vec2(1229.0, 1186.0));
        let pulled = egui::Rect::from_min_size(egui::pos2(108.0, 131.0), egui::vec2(1500.0, 950.0));
        // Pulled to a new size and closed at once, long before it would count as still.
        for (time, inner, close) in [(0.0, first, false), (0.1, first, false), (0.2, pulled, false), (0.25, pulled, true)] {
            let mut output = ctx.run_ui(frame(time, inner, close), |ui| {
                page.show(ui);
            });
            output.textures_delta.clear();
        }
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("\"fenster_groesse\": \"1500x950\""), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The name of the desktop of this thread (`Default` is the one the user sees).
    #[cfg(windows)]
    fn desktop_name() -> String {
        use windows_sys::Win32::System::StationsAndDesktops::{GetThreadDesktop, GetUserObjectInformationW, UOI_NAME};
        use windows_sys::Win32::System::Threading::GetCurrentThreadId;
        let mut buffer = [0u16; 256];
        let mut needed = 0u32;
        // SAFETY: asks for the name of the thread's own desktop into a buffer of the size given.
        let ok = unsafe {
            let desktop = GetThreadDesktop(GetCurrentThreadId());
            !desktop.is_null()
                && GetUserObjectInformationW(
                    desktop,
                    UOI_NAME,
                    buffer.as_mut_ptr().cast(),
                    (buffer.len() * 2) as u32,
                    &raw mut needed,
                ) != 0
        };
        let length = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        if ok { String::from_utf16_lossy(&buffer[..length]) } else { String::new() }
    }

    /// A page of a test writes its settings and tells the overlay, as the main window does. The signal reaches
    /// no window of a running program: hidden windows of this test stand in for the overlay of the user's own
    /// data and for one of a copy, marked as those runs mark them. (On the desktop the user sees, a key of its
    /// own stands in for the user's own data: a running program there must not find the stand-in either.)
    #[cfg(windows)]
    #[test]
    fn a_settings_signal_of_a_test_reaches_no_window_of_a_running_program() {
        use std::sync::atomic::AtomicU32;
        use versekit_core::paths::Host;
        use versekit_core::pypath::Flavor;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, WS_POPUP,
        };

        let own_data = if desktop_name().eq_ignore_ascii_case("Default") { "pruefung-fremd" } else { "" };
        let copy = window_signal::folder_key(&std::env::temp_dir().join("versekit-fremde-kopie").to_string_lossy());
        let heard = Arc::new(AtomicU32::new(0));
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let mut windows = Vec::new();
        for key in [own_data, copy.as_str()] {
            // SAFETY: a plain window of a system class without parent, never shown; destroyed below.
            let window = unsafe {
                CreateWindowExW(
                    0,
                    class.as_ptr(),
                    std::ptr::null(),
                    WS_POPUP,
                    0,
                    0,
                    1,
                    1,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            };
            assert!(!window.is_null());
            let counter = Arc::clone(&heard);
            assert!(window_signal::serve_as(window as isize, Role::Overlay, key, move |_| {
                counter.fetch_add(1, Ordering::SeqCst);
            }));
            windows.push(window);
        }
        let pump = || {
            // SAFETY: takes the messages of this thread and hands them to their windows.
            unsafe {
                let mut message: MSG = std::mem::zeroed();
                while PeekMessageW(&raw mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                    DispatchMessageW(&raw const message);
                }
            }
        };
        // The page writes the place of its window: the way that tells the overlay at once.
        let dir = std::env::temp_dir().join(format!("versekit-fremdsignal-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dir_text = dir.to_string_lossy().into_owned();
        let file = dir.join("einstellungen.json");
        std::fs::write(&file, "{\n  \"fenster_groesse\": \"1229x1186\"\n}\n").unwrap();
        let mut page = SamplePage::with_settings(Paths::new(Host::new(
            Flavor::native(),
            &[(versekit_core::legacy::ENV_HOME, &dir_text)],
            &dir_text,
        )));
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let moved = egui::Rect::from_min_size(egui::pos2(108.0, 131.0), egui::vec2(1400.0, 900.0));
        for time in [0.0, 0.1, 0.2, 0.8, 0.9] {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, moved.size())),
                time: Some(time),
                ..Default::default()
            };
            let info = input.viewports.entry(egui::ViewportId::ROOT).or_default();
            info.inner_rect = Some(moved);
            info.outer_rect = Some(moved.translate(egui::vec2(-8.0, -31.0)));
            info.native_pixels_per_point = Some(1.0);
            let mut output = ctx.run_ui(input, |ui| {
                page.show(ui);
            });
            output.textures_delta.clear();
        }
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("\"fenster_groesse\": \"1400x900\""), "the setting was written: {text}");
        pump();
        assert_eq!(heard.load(Ordering::SeqCst), 0, "the signal of the test reached a window of another run");
        assert!(!window_signal::send_to(Role::Overlay, Signal::Settings), "no overlay is found from a test");
        pump();
        assert_eq!(heard.load(Ordering::SeqCst), 0);
        // The stand-ins hear what is sent to them: the silence above is not that of deaf windows.
        for window in &windows {
            assert!(window_signal::send(*window as isize, Signal::Settings));
        }
        pump();
        assert_eq!(heard.load(Ordering::SeqCst), 2);
        for window in windows {
            // SAFETY: the windows made above.
            unsafe { DestroyWindow(window) };
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Hotkeys are read and written as 4.12 takes them.
    #[test]
    fn hotkeys_like_4_12() {
        let b = HOTKEYS.iter().position(|k| *k == "B").unwrap();
        assert_eq!(parse_hotkey("strg + alt + b"), Some(([true, true, false], b)));
        let f12 = HOTKEYS.iter().position(|k| *k == "F12").unwrap();
        assert_eq!(parse_hotkey("Ctrl-Shift-F12"), Some(([true, false, true], f12)));
        // F13 to F24 stand in the list after F12 and are read back to the same place.
        assert_eq!(parse_hotkey("Alt+F13"), Some(([false, true, false], f12 + 1)));
        let f24 = HOTKEYS.iter().position(|k| *k == "F24").unwrap();
        assert_eq!(parse_hotkey("Strg+F24"), Some(([true, false, false], f24)));
        for key in f12 + 1..HOTKEYS.len() {
            let text = hotkey_text([false, true, true], key).unwrap();
            assert_eq!(parse_hotkey(&text), Some(([false, true, true], key)), "{text}");
        }
        for wrong in ["B", "Strg+A+B", "Strg+Ä", "Alt+F25", ""] {
            assert_eq!(parse_hotkey(wrong), None, "{wrong}");
        }
        assert_eq!(hotkey_text([true, true, false], b).as_deref(), Some("Strg+Alt+B"));
        assert_eq!(hotkey_text([false; 3], b), None);
        // Every key of the list is one the overlay reads; the named keys stand after F24.
        for (i, name) in HOTKEYS.iter().enumerate() {
            assert!(versekit_core::overlay::key_code(name).is_some(), "{name}");
            assert_eq!(hotkey_word(name).is_some(), i > f24, "{name}");
        }
        let stored: Vec<&str> = versekit_core::overlay::NAMED_KEYS.iter().map(|(name, _, _)| *name).collect();
        assert_eq!(HOTKEYS[f24 + 1..], stored[..]);
        for (modifiers, key) in [([true, false, false], 0), ([false, true, true], 40)] {
            assert_eq!(parse_hotkey(&hotkey_text(modifiers, key).unwrap()), Some((modifiers, key)));
        }
        // Without a setting the page shows the defaults, the same on every system and without Strg.
        let fresh = SamplePage::new();
        let shown = hotkey_text(fresh.hotkey_modifiers, fresh.hotkey_key);
        assert_eq!(shown.as_deref(), Some(versekit_core::overlay::DEFAULT_HOTKEY));
        let scan = hotkey_text(fresh.scan_modifiers, fresh.scan_key);
        assert_eq!(scan.as_deref(), Some(versekit_core::overlay::DEFAULT_SCAN_HOTKEY));
        assert_eq!((shown.as_deref(), scan.as_deref()), (Some("Alt+Umschalt+V"), Some("Alt+Umschalt+R")));
    }

    /// One key by itself (Kuehlwagen): F1 … F24 and the named keys need no modifier, a letter or a digit does. A
    /// letter picked while nothing is pressed goes back to the key before.
    #[test]
    fn a_key_that_is_never_typed_needs_no_modifier() {
        let at = |name: &str| HOTKEYS.iter().position(|k| *k == name).unwrap();
        assert_eq!(hotkey_text([false; 3], at("F13")).as_deref(), Some("F13"));
        assert_eq!(parse_hotkey("F13"), Some(([false; 3], at("F13"))));
        for (i, name) in HOTKEYS.iter().enumerate() {
            let alone = hotkey_text([false; 3], i);
            let letter_or_digit = name.len() == 1;
            assert_eq!(alone.is_none(), letter_or_digit, "{name}");
            if let Some(text) = alone {
                assert_eq!(parse_hotkey(&text), Some(([false; 3], i)), "{text}");
            }
        }
        // F13 by itself, and A picked from the list: back to F13, refused.
        let (mut modifiers, mut key) = ([false; 3], at("A"));
        assert!(keep_hotkey(&mut modifiers, &mut key, ([false; 3], at("F13"))));
        assert_eq!((modifiers, key), ([false; 3], at("F13")));
        // Alt let go of with B: refused; with F8: kept.
        let (mut modifiers, mut key) = ([false; 3], at("B"));
        assert!(keep_hotkey(&mut modifiers, &mut key, ([false, true, false], at("B"))));
        assert_eq!((modifiers, key), ([false, true, false], at("B")));
        let (mut modifiers, mut key) = ([false; 3], at("F8"));
        assert!(!keep_hotkey(&mut modifiers, &mut key, ([false, true, false], at("F8"))));
        assert_eq!((modifiers, key), ([false; 3], at("F8")));
        // Unchanged combinations with a modifier stay as they are.
        let (mut modifiers, mut key) = ([true, true, false], at("B"));
        assert!(!keep_hotkey(&mut modifiers, &mut key, ([true, true, false], at("B"))));
        // The words of the named keys come in the language of the window.
        let _language = crate::language_lock();
        let names = hotkey_names();
        assert_eq!(names[at("Einfg")], language::t("s_ei_taste_einfg"));
        assert_eq!(names[at("F13")], "F13");
    }

    /// The window reads the settings file of 4.12, writes only what changed,
    /// keeps everything else in it, and writes nothing while the write lock
    /// is on, as in the preview to pass on.
    #[test]
    fn settings_are_read_and_written_like_4_12() {
        use versekit_core::writeguard;

        let dir = std::env::temp_dir().join(format!("versekit-einstellungen-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("einstellungen.json");
        let original = "{\n  \"_hinweis\": \"Zeilen mit _ sind nur Erklaerung.\",\n  \"durchklickbar\": false,\n  \
                        \"max_zeilen\": 5,\n  \"hotkey\": \"strg + alt + b\",\n  \"overlay_ecke\": \"unten-rechts\",\n  \
                        \"overlay_leiste\": \"unten\",\n  \"fremd\": [1, 2]\n}\n";
        std::fs::write(&file, original).unwrap();
        // A game folder of its own: the account found in the logs of an installation on this
        // computer would be written into the file.
        let mut page = SamplePage::with_settings(Paths::new(super::real_data::host_apart(&dir)));
        assert_eq!(page.overlay_rows, 5);
        assert_eq!((page.hotkey_modifiers, HOTKEYS[page.hotkey_key]), ([true, true, false], "B"));
        assert_eq!(page.overlay_place, settings::OverlayPlace::BottomRight);
        assert_eq!(page.bar_side, settings::BarSide::Bottom);

        // Nothing changed: the file stays as it is, byte for byte.
        page.save_changes();
        page.save_changes();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), original);

        page.overlay_clickable = true;
        page.hotkey_key = HOTKEYS.iter().position(|k| *k == "F8").unwrap();
        page.save_changes();
        let text = std::fs::read_to_string(&file).unwrap();
        let data = versekit_core::jsonio::loads(&text).unwrap();
        let map = data.as_object().unwrap();
        let get = |key: &str| map.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
        assert_eq!(get("durchklickbar"), Some(Value::Bool(true)));
        assert_eq!(get("hotkey"), Some(Value::str("Strg+Alt+F8")));
        assert_eq!(get("max_zeilen"), Some(Value::int(5)));
        assert_eq!(map[0].0, "_hinweis", "the explanation stays at the top");
        assert!(get("fremd").is_some(), "unknown settings are kept");
        assert_eq!(page.saving(), Some(true));

        // With the write lock on, a change is not written and is reported.
        writeguard::deny(true);
        writeguard::allow_under(None);
        page.overlay_rows = 9;
        page.save_changes();
        writeguard::deny(false);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
        assert_eq!(page.saving(), Some(false));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// With its own settings thread a slow settings file does not hold the
    /// window: the frame goes on at once, the file follows, and a reload
    /// asked for meanwhile waits for it.
    #[test]
    fn a_slow_settings_file_does_not_hold_the_window() {
        use versekit_core::legacy::ENV_HOME;
        use versekit_core::paths::Host;
        use versekit_core::pypath::Flavor;

        let dir =
            std::env::temp_dir().join(format!("versekit-einstellungen-langsam-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dir_text = dir.to_string_lossy().into_owned();
        let file = dir.join("einstellungen.json");
        std::fs::write(&file, "{\n  \"max_zeilen\": 5\n}\n").unwrap();
        let host = Host::new(Flavor::native(), &[(ENV_HOME, &dir_text)], &dir_text);
        let mut page = SamplePage::with_settings(Paths::new(host));
        page.write_in_background();
        page.save_changes();
        page.slow_writes(400);
        page.overlay_rows = 9;
        let started = std::time::Instant::now();
        page.save_changes();
        let took = started.elapsed();
        page.reload_settings();
        assert!(page.reload_waiting, "the reload waits for the own write");
        assert_eq!(page.overlay_rows, 9, "nothing read back over the own change");
        let mut written = false;
        for _ in 0..60 {
            std::thread::sleep(std::time::Duration::from_millis(50));
            if std::fs::read_to_string(&file).unwrap().contains("\"max_zeilen\": 9") {
                written = true;
                break;
            }
        }
        page.slow_writes(0);
        assert!(took < std::time::Duration::from_millis(100), "the frame waited {took:?}");
        assert!(written, "the file follows");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The tests run side by side: a window made slow on purpose must not slow down the settings writing of another
    /// window (that made tests of other pages wait for it and fail now and then).
    #[test]
    fn a_slow_writer_of_one_window_leaves_the_writer_of_another_alone() {
        use versekit_core::legacy::ENV_HOME;
        use versekit_core::paths::Host;
        use versekit_core::pypath::Flavor;

        let make = |name: &str| {
            let dir = std::env::temp_dir().join(format!("versekit-schreiber-{name}-{}", versekit_core::testdir::run_tag()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let text = dir.to_string_lossy().into_owned();
            let host = Host::new(Flavor::native(), &[(ENV_HOME, &text)], &text);
            let mut page = SamplePage::with_settings(Paths::new(host));
            page.write_in_background();
            (page, dir)
        };
        let ((slow, dir_a), (quick, dir_b)) = (make("langsam"), make("schnell"));
        slow.slow_writes(400);
        assert_eq!(slow.write_delay(), Some(400));
        assert_eq!(quick.write_delay(), Some(0), "the other window writes without a delay");
        let _ = (std::fs::remove_dir_all(dir_a), std::fs::remove_dir_all(dir_b));
    }

    /// The overlay was moved by hand and wrote "frei": the window takes it
    /// over at once, writes nothing for the reload, and a later change of
    /// its own keeps the freed corner.
    #[test]
    fn a_corner_freed_by_the_overlay_is_taken_over() {
        use versekit_core::legacy::ENV_HOME;
        use versekit_core::paths::Host;
        use versekit_core::pypath::Flavor;

        let dir =
            std::env::temp_dir().join(format!("versekit-einstellungen-frei-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dir_text = dir.to_string_lossy().into_owned();
        let file = dir.join("einstellungen.json");
        std::fs::write(&file, "{\n  \"overlay_ecke\": \"unten-links\",\n  \"overlay_leiste\": \"unten\"\n}\n").unwrap();
        let host = Host::new(Flavor::native(), &[(ENV_HOME, &dir_text)], &dir_text);
        let mut page = SamplePage::with_settings(Paths::new(host.clone()));
        page.save_changes();
        assert_eq!(page.overlay_place, settings::OverlayPlace::BottomLeft);

        // The overlay, another program, frees the corner.
        assert!(Paths::new(host).set_setting("overlay_ecke", Value::str("frei")));
        let written = std::fs::read_to_string(&file).unwrap();
        page.reload_settings();
        assert_eq!(page.overlay_place, settings::OverlayPlace::Free);
        page.save_changes();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), written, "the reload writes nothing");

        // A later change of the window keeps the freed corner.
        page.bar_side = settings::BarSide::Top;
        page.save_changes();
        let data = versekit_core::jsonio::loads(&std::fs::read_to_string(&file).unwrap()).unwrap();
        let map = data.as_object().unwrap();
        let get = |key: &str| map.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
        assert_eq!(get("overlay_ecke"), Some(Value::str("frei")));
        assert_eq!(get("overlay_leiste"), Some(Value::str("oben")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Every text a page shows (setting name, hint, group title, choice,
    /// info) leads the menu filter to that page, in any case; a setting
    /// shows as `Page › name`, so the jump lights up its row.
    #[test]
    fn menu_filter_finds_everything_on_the_pages() {
        use versekit_core::language::t;
        // The typed words are German.
        language::set_thread_language(language::Choice::De);
        let mut checked = 0;
        for (group, pages) in NAV {
            for (page, _) in pages.iter() {
                let mut texts: Vec<String> = vec![t(page), t(group)];
                for (title, rows) in settings_of(page) {
                    for (row, hint, choices) in rows.iter() {
                        let row = label(row);
                        assert!(
                            nav_hits(&row.to_uppercase(), group, page).contains(&format!("{} › {row}", t(page))),
                            "{page}: setting {row} not shown as its own hit"
                        );
                        texts.extend([label(title), row, label(hint)]);
                        texts.extend(choices.iter().map(|c| label(c)));
                    }
                }
                texts.extend(page_infos(page).iter().map(|info| info.to_string()));
                texts.extend(language_infos(page));
                for text in texts.iter().filter(|text| !text.is_empty()) {
                    assert!(!nav_hits(text, group, page).is_empty(), "{page}: “{text}” not found");
                    checked += 1;
                }
            }
        }
        assert!(checked > 100, "only {checked} texts checked");
        assert_eq!(settings_of("hf_allgemein").len(), 5);
        assert!(page_infos("hf_raffinerien").iter().any(|info| info.contains("Titanium")));
        // Counter-check: a word on no page finds nothing anywhere.
        for (group, pages) in NAV {
            for (page, _) in pages.iter() {
                assert!(nav_hits("Gibt es nirgends", group, page).is_empty(), "{page}");
            }
        }
        // Without a filter every page shows once, just by its name.
        assert_eq!(nav_hits("  ", "hf_gruppe_info", "hf_ueber"), vec![t("hf_ueber")]);
        // Every page of a module has an entry in the menu, and no page of the core is one of them.
        for (module, pages) in
            versekit_core::modules::SWITCHABLE.iter().chain(versekit_core::modules::ALSO_HIDES.iter())
        {
            for page in *pages {
                let group = group_of(page).unwrap_or_else(|| panic!("{page} of {module} is not in the menu"));
                assert!(
                    !matches!(group, "hf_gruppe_bp" | "hf_gruppe_spiel" | "hf_gruppe_info")
                        && (group != "hf_gruppe_einst" || *page == "k_basetool_verbindung"),
                    "{page} stands in the core group {group} but belongs to the module {module}"
                );
            }
        }
        // The start programs sit in the settings group and the filter finds them by their name.
        assert_eq!(nav_hits("startprog", "hf_gruppe_einst", "hf_startprogramme"), vec![t("hf_startprogramme")]);
        assert!(group_of("hf_startprogramme") == Some("hf_gruppe_einst"));
        // `ß` and the umlauts may be typed either way.
        let size = format!("{} › Größe der Darstellung", t("hf_darstellung"));
        for typed in ["Größe", "GRÖSSE", "groesse", "grosse der"] {
            let found = nav_hits(typed, "hf_gruppe_einst", "hf_darstellung").contains(&size);
            assert_eq!(found, typed != "grosse der", "„{typed}“");
        }
    }

    /// "basetool" in the menu filter leads to the connection page and the unit
    /// storage, to no page else.
    #[test]
    fn the_menu_filter_knows_the_basetool_pages() {
        use versekit_core::language::t;
        // The page itself leads there, not only a text on it.
        let mut found = Vec::new();
        for (group, pages) in NAV {
            for (page, _) in pages.iter() {
                if nav_hits("basetool", group, page).contains(&t(page)) {
                    found.push(*page);
                }
            }
        }
        assert!(found.contains(&"k_basetool_verbindung") && found.contains(&"hf_staffellager"), "{found:?}");
        assert!(!found.contains(&"hf_liste") && !found.contains(&"hf_hangar"), "{found:?}");
        // The areas of the connection page are found by their names.
        let hits = nav_hits(&t("s_btv_hangar"), "hf_gruppe_einst", "k_basetool_verbindung");
        assert!(hits.contains(&format!("{} › {}", t("k_basetool_verbindung"), t("s_btv_hangar"))), "{hits:?}");
    }

    /// The unit demand is there with its switch on, its right granted and
    /// the connection standing, and the page shows what the sync brought.
    #[test]
    fn the_unit_demand_comes_with_its_switch_its_right_and_the_connection() {
        use versekit_core::basetool::demo::DemoLogin;
        use versekit_core::basetool::scopes::SETTING_DEMAND;
        let dir = std::env::temp_dir().join(format!("versekit-gate-bedarf-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        let wire = basetool_page::Wire::in_folder(&dir.to_string_lossy(), false);
        wire.demo.as_ref().expect("demo").script_login(DemoLogin::Approve, 1);
        let mut page = SamplePage::new();
        page.connect_wire(wire.clone());
        assert!(page.page_hidden("hf_bedarf"), "not connected, nothing switched on");
        assert!(wire.sync.set_setting(SETTING_DEMAND, Value::Bool(true)));
        assert!(page.page_hidden("hf_bedarf"), "switched on, but not connected");
        wire.sync.connect();
        // The login runs in a thread of its own: waits for it, on a busy machine that takes long.
        let end = std::time::Instant::now() + std::time::Duration::from_secs(90);
        while !wire.sync.connection().connected() && std::time::Instant::now() < end {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(wire.sync.connection().connected(), "the login did not end");
        wire.sync.pass_now().expect("pass");
        assert!(!page.page_hidden("hf_bedarf"));
        assert!(page.page_hidden("hf_staffellager"), "the unit storage has its own switch");
        let ctx = egui::Context::default();
        crate::install(&ctx);
        page.preview = true;
        page.page = "hf_bedarf".into();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0))),
                ..Default::default()
            },
            |ui| {
                page.show(ui);
            },
        );
        output.textures_delta.clear();
        fn collect(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
            match shape {
                egui::epaint::Shape::Text(text) => out.push(text.galley.text().to_string()),
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, out)),
                _ => {}
            }
        }
        let mut shown = Vec::new();
        output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut shown));
        assert!(shown.contains(&"Laranite".to_string()), "the demand of the sync stands on the page: {shown:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The unit storage is there only with its switch on, its right granted
    /// and the connection standing; the page that was open goes when the
    /// connection ends.
    #[test]
    fn the_unit_storage_comes_with_the_connection_and_goes_with_it() {
        use versekit_core::basetool::demo::DemoLogin;
        use versekit_core::basetool::scopes::SETTING_ORGSTOCK;
        let dir = std::env::temp_dir().join(format!("versekit-gate-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        let wire = basetool_page::Wire::in_folder(&dir.to_string_lossy(), false);
        wire.demo.as_ref().expect("demo").script_login(DemoLogin::Approve, 1);
        let mut page = SamplePage::new();
        assert!(!page.page_hidden("hf_staffellager"), "without a connection of the preview the page is as before");
        assert!(!page.page_hidden("hf_bedarf"));
        page.connect_wire(wire.clone());
        assert!(page.page_hidden("hf_staffellager"), "not connected, nothing switched on");
        assert!(page.page_hidden("hf_bedarf"), "the unit demand comes with its own switch and right");
        assert!(!page.page_hidden("hf_liste") && !page.page_hidden("k_basetool_verbindung"));
        assert!(wire.sync.set_setting(SETTING_ORGSTOCK, Value::Bool(true)));
        assert!(page.page_hidden("hf_staffellager"), "switched on, but not connected");
        wire.sync.connect();
        let end = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while !wire.sync.connection().connected() && std::time::Instant::now() < end {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        wire.sync.pass_now().expect("pass");
        assert!(!page.page_hidden("hf_staffellager"));
        // Open, then the connection ends: the next frame shows the blueprint list.
        page.preview = true;
        page.page = "hf_staffellager".into();
        wire.sync.disconnect().wait(std::time::Duration::from_secs(10));
        assert!(page.page_hidden("hf_staffellager"));
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0))),
                ..Default::default()
            },
            |ui| {
                page.show(ui);
            },
        );
        output.textures_delta.clear();
        assert_eq!(page.page, "hf_liste");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The choices of the color scheme setting are the names of all color
    /// schemes, in their order; the setting row and the menu filter use them.
    #[test]
    fn scheme_choices_follow_the_schemes() {
        let row = settings_of("hf_darstellung")[0].1[0];
        assert_eq!(row.0, "s_da_schema");
        let labels: Vec<&str> = theme::schemes().iter().map(|s| s.label).collect();
        assert_eq!(row.2, labels.as_slice());
        assert!(row.2.iter().all(|key| label(key) != *key), "a scheme name is missing in the language file");
    }

    /// Of several menu filter hits of the open page only the clicked one
    /// lights up; none clicked: the first.
    #[test]
    fn only_the_clicked_hit_lights_up() {
        let hits: Vec<String> = ["Overlay", "Overlay › Durchklickbar", "Overlay › Wo das Overlay sitzt"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let lit: Vec<bool> = hits.iter().map(|h| nav_lit(h, &hits, "Overlay › Durchklickbar")).collect();
        assert_eq!(lit, [false, true, false]);
        let lit: Vec<bool> = hits.iter().map(|h| nav_lit(h, &hits, "")).collect();
        assert_eq!(lit, [true, false, false]);
        assert!(nav_lit("Overlay", &hits[..1], "Darstellung"));
    }

    /// A tile colors its lines as it is told (the accent for what is fine, gold for a look, red for what is wrong)
    /// and draws a share as a bar of that width under the key line.
    #[test]
    fn a_tile_colors_its_lines_and_draws_the_share_as_a_bar() {
        use versekit_core::language::{Choice, set_thread_language};
        set_thread_language(Choice::De);
        let scheme = theme::schemes().first().expect("a scheme");
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let mut real = glance::Overview::default();
        let tile = glance::Glance::new(
            "ZEILE EINS".to_string(),
            vec!["gut".to_string(), "achtung".to_string(), "schlecht".to_string(), "schlicht".to_string()],
        )
        .line_tone(glance::Tone::Bad)
        .bar(0.5)
        .tone_at(0, glance::Tone::Good)
        .tone_at(1, glance::Tone::Warn)
        .tone_at(2, glance::Tone::Bad);
        real.tiles.insert("hf_meinlager", tile);
        let texts = tiles::TileTexts {
            swap: "Tauschen",
            unpin: "Lösen",
            size: "Ecke",
            hide: "Aus",
            sizes: ["K", "B", "H", "G"],
            hidden: "Aus:",
            keys: "Tasten",
        };
        let mut board = vec![tiles::Tile::new("hf_meinlager", tiles::TileSize::Large)];
        let (mut colors, mut bars) = (Vec::new(), Vec::new());
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(560.0, 3000.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                let mut add = |tile: &mut egui::Ui, key: &str| {
                    overview_tile_content(tile, scheme, key, Some(&real));
                };
                tiles::board(ui, scheme, "farben", &mut board, &texts, &mut add, &|k: &str| k.to_string());
            });
            output.textures_delta.clear();
            fn walk(
                shape: &egui::epaint::Shape,
                colors: &mut Vec<(String, egui::Color32)>,
                bars: &mut Vec<(egui::Rect, egui::Color32)>,
            ) {
                match shape {
                    egui::epaint::Shape::Text(text) => {
                        let color = text.galley.job.sections.first().map(|s| s.format.color);
                        if let Some(color) = color {
                            colors.push((text.galley.text().to_string(), color));
                        }
                    }
                    egui::epaint::Shape::Rect(rect) => bars.push((rect.rect, rect.fill)),
                    egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, colors, bars)),
                    _ => {}
                }
            }
            colors.clear();
            bars.clear();
            output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut colors, &mut bars));
        }
        let color_of = |text: &str| colors.iter().find(|(t, _)| t.contains(text)).map(|(_, c)| *c);
        assert_eq!(color_of("ZEILE EINS"), Some(color(scheme.red_text())));
        assert_eq!(color_of("gut"), Some(color(scheme.accent)));
        assert_eq!(color_of("achtung"), Some(color(scheme.gold)));
        assert_eq!(color_of("schlecht"), Some(color(scheme.red_text())));
        assert_eq!(color_of("schlicht"), Some(color(scheme.fg)));
        // The track and the filled part: the filled one is half as wide as the track.
        let height = tiles::BAR_HEIGHT;
        let track =
            bars.iter().find(|(r, c)| (r.height() - height).abs() < 0.1 && *c == color(tiles::bar_track(scheme)));
        let filled = bars.iter().find(|(r, c)| (r.height() - height).abs() < 0.1 && *c == color(scheme.accent));
        let (track, filled) = (track.expect("the track").0, filled.expect("the filled part").0);
        assert!((filled.width() / track.width() - 0.5).abs() < 0.02, "{} of {}", filled.width(), track.width());
        set_thread_language(Choice::Auto);
    }

    /// The hide button shows over the top right corner of a tile: a long title wraps before it and never runs
    /// under it, in the largest font and the narrowest window.
    #[test]
    fn the_title_of_a_tile_keeps_clear_of_the_hide_button() {
        use versekit_core::language::{Choice, set_thread_language};
        set_thread_language(Choice::De);
        let scheme = theme::schemes().first().expect("a scheme");
        let points = versekit_core::font_size::POINTS_RANGE.1;
        let ctx = egui::Context::default();
        crate::install(&ctx);
        crate::apply_size(&ctx, points);
        let texts = tiles::TileTexts {
            swap: "Tauschen",
            unpin: "Lösen",
            size: "Ecke",
            hide: "Aus",
            sizes: ["K", "B", "H", "G"],
            hidden: "Aus:",
            keys: "Tasten",
        };
        let width = 560.0;
        let columns = tiles::columns_for(width);
        let cell = (width - GAP * (columns - 1) as f32) / columns as f32;
        fn find(shape: &egui::epaint::Shape, title: &str, out: &mut Option<f32>) {
            match shape {
                egui::epaint::Shape::Text(text) if text.galley.text() == title => {
                    *out = Some(text.pos.x + text.galley.size().x);
                }
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| find(s, title, out)),
                _ => {}
            }
        }
        // Every title there is: the tiles of 4.12 and every page that can be pinned (one has "· Fortgeschritten").
        // Plus one name of a single long word, which wraps at the edge of its room wherever that is.
        let keys: Vec<&str> = versekit_core::overview::TILES
            .iter()
            .map(|(id, _)| *id)
            .chain(versekit_core::overview::PAGES.iter().map(|(_, key)| *key))
            .chain(["WWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWW"])
            .collect();
        let mut worst = (0.0f32, "");
        for key in keys {
            let title = tile_of(key).1;
            let mut board = vec![tiles::Tile::new(key, tiles::TileSize::Small)];
            let mut right_edge = 0.0;
            for _ in 0..3 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 3000.0))),
                    ..Default::default()
                };
                let mut output = ctx.run_ui(input, |ui| {
                    let mut add = |tile: &mut egui::Ui, key: &str| {
                        overview_tile_content(tile, scheme, key, None);
                    };
                    tiles::board(ui, scheme, "titel-test", &mut board, &texts, &mut add, &|k: &str| k.to_string());
                });
                output.textures_delta.clear();
                let mut found = None;
                output.shapes.iter().for_each(|clipped| find(&clipped.shape, &title, &mut found));
                right_edge = found.unwrap_or(0.0);
            }
            if right_edge > worst.0 {
                worst = (right_edge, key);
            }
        }
        // The button lies 30 points from the right edge of the tile (`tiles::board`).
        assert!(
            worst.0 <= cell - 30.0,
            "the title of {} reaches {}, the hide button starts at {}",
            worst.1,
            worst.0,
            cell - 30.0
        );
        set_thread_language(Choice::Auto);
    }

    /// Every tile of the overview in every grid size, every font size, every scheme, German and English, in a
    /// narrow to a wide window, with the sample texts and with long real texts: its content has to fit, and
    /// no line of text may be cut short ("…") or cut off at the edge of the tile.
    #[test]
    fn every_overview_tile_fits_every_size() {
        use versekit_core::language::{Choice, set_thread_language};
        // Every line of text of a frame that is shortened or reaches out of the room it is drawn in.
        fn cut_lines(output: &egui::FullOutput) -> Vec<String> {
            fn walk(shape: &egui::epaint::Shape, clip: egui::Rect, out: &mut Vec<String>) {
                match shape {
                    egui::epaint::Shape::Text(text) => {
                        if text.galley.elided {
                            out.push(format!("gekürzt: {:?}", text.galley.text()));
                        }
                        for row in &text.galley.rows {
                            let rect = row.rect_without_leading_space().translate(text.pos.to_vec2());
                            if !row.row.text().trim().is_empty() && !clip.expand(0.5).contains_rect(rect) {
                                out.push(format!("abgeschnitten: {:?}", row.row.text()));
                            }
                        }
                    }
                    egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, clip, out)),
                    _ => {}
                }
            }
            let mut out = Vec::new();
            output.shapes.iter().for_each(|clipped| walk(&clipped.shape, clipped.clip_rect, &mut out));
            out
        }
        // Only the tile of the backup has the link.
        let backup_link = |key: &str| (key == "sicherung").then_some(glance::TileAction::BackupNow);
        // Real counts with long texts, as the pages write them (the line of a pinned page is its description).
        let long_line =
            "Wo welches Erz liegt, nach Rohstoff oder nach Ort, mit Mengen und Qualität aus deinen Aufträgen";
        let long_detail =
            "neuere Fassung gefunden heute 08:05, wird gleich eingespielt, sobald das Spiel geschlossen ist";
        let keys = [
            "hf_raffinerien",
            "hf_meinlager",
            "hf_liste",
            "spielzeit",
            "hf_serverstatus",
            "hf_routen",
            // Pinned pages without their own tile, one with a long name.
            "hf_farmliste",
            "k_basetool_verbindung",
            // Pages that show their own figures, and the tiles of 4.12.
            "hf_herstellung",
            "hf_staffellager",
            "hf_fortschritt",
            "hf_auftragslog",
            "hf_hangar",
            "hf_einkaufsliste",
            "hf_st_uebersicht",
            "spiel",
            "standort",
            "bauplaene",
            "rechner",
            "dienste",
            "raffinerie",
            "lager",
            "auftraege",
            "neu_patch",
            "merkliste",
            "funde",
            "sicherung",
            "notizen",
            "executive_hangar",
        ];
        let lamps = [
            versekit_core::contested::Lamp::Green,
            versekit_core::contested::Lamp::Green,
            versekit_core::contested::Lamp::Red,
            versekit_core::contested::Lamp::Red,
            versekit_core::contested::Lamp::Dark,
        ];
        let texts = tiles::TileTexts {
            swap: "Tauschen",
            unpin: "Lösen",
            size: "Ecke",
            hide: "Aus",
            sizes: ["K", "B", "H", "G"],
            hidden: "Aus:",
            keys: "Tasten",
        };
        let mut real = glance::Overview::default();
        for key in keys.into_iter().chain(["uebersetzung"]) {
            // Twelve lines, as the pages offer them: ten of a usual length, then two long ones.
            let mut details: Vec<String> = (1..=10).map(|n| format!("Titanium {n} · 12,500 SCU · Area18")).collect();
            details.push(long_detail.to_string());
            details.push(format!("{long_detail} (2)"));
            // A large key figure before the long line: the room for it counts.
            let tile = glance::Glance::new(long_line.to_string(), details).figure("1234567");
            // The tile of the backup ends in the link "Back up now": the room for it counts, in both languages.
            let tile = glance::Glance { action: backup_link(key), ..tile };
            // The tile of the Executive Hangar draws its lamps under the line: the room for them counts.
            real.tiles.insert(key, if key == "executive_hangar" { tile.lamps(lamps) } else { tile });
        }
        // The same tiles with the texts they usually have: there no size may stay empty.
        let mut usual = glance::Overview::default();
        let mut flat = glance::Overview::default();
        for key in keys.into_iter().chain(["uebersetzung"]) {
            let details: Vec<String> = (1..=12).map(|n| format!("Titanium {n} · 12,500 SCU · Area18")).collect();
            // The key figure of most tiles begins their line.
            let tile = glance::Glance::new("12 · Rohstoffe: 12 · Waren: 3".to_string(), details).figure(12);
            let tile = glance::Glance { action: backup_link(key), ..tile };
            let tile = if key == "executive_hangar" { tile.lamps(lamps) } else { tile };
            flat.tiles.insert(key, tile.clone());
            // With a bar and a colored line, as several tiles have them: the bar never costs the smallest tile its
            // lines (the same tile without them shows as many). The computer draws its bar in five steps.
            let tile = tile.bar(0.5).line_tone(glance::Tone::Warn);
            usual.tiles.insert(key, if key == "rechner" { tile.steps(5) } else { tile });
        }
        let mut without_bar: std::collections::HashMap<String, [usize; 4]> = std::collections::HashMap::new();
        let mut findings = Vec::new();
        for language in [Choice::De, Choice::En] {
            set_thread_language(language);
            // Three of the six schemes (the colors cannot move text) and the sizes at both ends and between.
            for s in theme::schemes().iter().step_by(2) {
                for points in [versekit_core::font_size::POINTS_RANGE.0, 2, 4, versekit_core::font_size::POINTS_RANGE.1]
                {
                    let ctx = egui::Context::default();
                    crate::install(&ctx);
                    crate::apply_size(&ctx, points);
                    for (counted, plain, barred) in [
                        (None, false, false),
                        (Some(&real), false, false),
                        (Some(&flat), true, false),
                        (Some(&usual), true, true),
                    ] {
                        for key in keys.into_iter().chain(counted.map(|_| "uebersetzung")) {
                            // The narrowest real board: the smallest window less the side bar, two columns.
                            for width in [560.0, 900.0, 1800.0] {
                                // How much each size shows, in the order of `TileSize::ALL`.
                                let mut shown = [0usize; 4];
                                for (n, size) in tiles::TileSize::ALL.into_iter().enumerate() {
                                    let mut board = vec![tiles::Tile::new(key, size)];
                                    let salt = format!("test-{}-{plain}-{width}-{n}", counted.is_some());
                                    let (mut overflow, mut cut, mut button) = (0.0, Vec::new(), false);
                                    // The rows follow what the tiles needed in the frame before.
                                    for _ in 0..3 {
                                        let input = egui::RawInput {
                                            screen_rect: Some(egui::Rect::from_min_size(
                                                egui::Pos2::ZERO,
                                                egui::vec2(width, 6000.0),
                                            )),
                                            ..Default::default()
                                        };
                                        let mut output = ctx.run_ui(input, |ui| {
                                            let mut add = |tile: &mut egui::Ui, key: &str| {
                                                shown[n] = overview_tile_content(tile, s, key, counted).1;
                                            };
                                            overflow = tiles::board(ui, s, &salt, &mut board, &texts, &mut add, &|k| {
                                                k.to_string()
                                            });
                                        });
                                        output.textures_delta.clear();
                                        cut = cut_lines(&output);
                                        let open = t("s_oeffnen");
                                        button = output.shapes.iter().any(|clipped| {
                                            fn has(shape: &egui::epaint::Shape, text: &str) -> bool {
                                                match shape {
                                                    egui::epaint::Shape::Text(t) => t.galley.text() == text,
                                                    egui::epaint::Shape::Vec(shapes) => {
                                                        shapes.iter().any(|s| has(s, text))
                                                    }
                                                    _ => false,
                                                }
                                            }
                                            has(&clipped.shape, &open)
                                        });
                                    }
                                    let at = format!(
                                        "{language:?} {} Stufe {points} {key} {size:?} {width} echt {} gewöhnlich {plain}",
                                        s.id,
                                        counted.is_some()
                                    );
                                    if overflow > 0.5 {
                                        findings.push(format!("{at}: {overflow:.0} über"));
                                    }
                                    findings.extend(cut.into_iter().map(|line| format!("{at}: {line}")));
                                    // The click on the tile opens its page: an open button would only say it twice.
                                    if button {
                                        findings.push(format!("{at}: „Öffnen“ auf der Kachel"));
                                    }
                                }
                                // A bigger tile never shows less: small ≤ wide, tall ≤ large, small ≤ tall.
                                let [small, wide, tall, large] = shown;
                                // Where the page has lines to show, no size is left empty: even the smallest shows
                                // one in the usual sizes of the text, a tall or large one several.
                                if plain {
                                    let at = format!("{language:?}{}{points}{key}{width}", s.id);
                                    if barred {
                                        if let Some([flat_small, flat_wide, ..]) = without_bar.get(&at)
                                            && (small, wide) != (*flat_small, *flat_wide)
                                        {
                                            findings.push(format!(
                                                "{language:?} {} {key} {width}: der Balken kostet Zeilen, klein {small} statt {flat_small}, breit {wide} statt {flat_wide}",
                                                s.id
                                            ));
                                        }
                                    } else {
                                        without_bar.insert(at, shown);
                                    }
                                }
                                if plain && points <= 3 {
                                    let (first, many) = (small >= 1, tall >= 2 && large >= 2 && wide >= 1);
                                    if !first || !many {
                                        findings.push(format!(
                                            "{language:?} {} {key} {width}: leer, klein {small}, breit {wide}, hoch {tall}, groß {large}",
                                            s.id
                                        ));
                                    }
                                }
                                if small > wide || small > tall || tall > large || wide > large {
                                    findings.push(format!(
                                        "{language:?} {} {key} {width}: zeigt klein {small}, breit {wide}, hoch {tall}, groß {large}",
                                        s.id
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
        set_thread_language(Choice::Auto);
        let count = findings.len();
        findings.truncate(40);
        assert!(count == 0, "Kachelinhalt steht über oder ist gekürzt ({count}):\n{}", findings.join("\n"));
    }

    #[test]
    fn quality_check_like_the_refinery_window() {
        assert_eq!(quality_error(""), None);
        assert_eq!(quality_error("295"), None);
        assert_eq!(quality_error("1000"), None);
        assert_eq!(quality_error("1200"), Some("s_rf_qualitaet"));
        assert_eq!(quality_error("abc"), Some("s_mu_q_zahl"));
        let german = language::Lang::De;
        assert_eq!(language::t_in(german, "s_rf_qualitaet"), "Qualität: 0 bis 1000.");
        assert_eq!(language::t_in(german, "s_mu_q_zahl"), "Qualität muss eine Zahl sein.");
    }

    /// The edge check: the sample page and the preview in every size and
    /// every scheme, in the smallest window of that size (which is
    /// `BASE_MIN_SIZE` in interface points) and in a wide one: nothing may
    /// stick out.
    #[test]
    fn nothing_is_cut_off_at_any_size() {
        let (width, height) = font_size::BASE_MIN_SIZE;
        let mut findings = Vec::new();
        for (kind, make) in [("Musterseite", SamplePage::new as fn() -> SamplePage), ("Vorschau", SamplePage::preview)]
        {
            for s in theme::schemes() {
                for points in font_size::POINTS_RANGE.0..=font_size::POINTS_RANGE.1 {
                    for window_width in [width, 1800.0] {
                        theme::set_thread_current(s.id);
                        let ctx = egui::Context::default();
                        crate::install(&ctx);
                        let mut page = make();
                        page.size = points;
                        let mut overflow = 0.0;
                        for _ in 0..3 {
                            let input = egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    egui::vec2(window_width, height),
                                )),
                                ..Default::default()
                            };
                            let mut output = ctx.run_ui(input, |ui| overflow = page.show(ui));
                            output.textures_delta.clear();
                        }
                        if overflow > 0.5 {
                            findings.push(format!(
                                "{kind} {} Größe {points} Breite {window_width}: {overflow:.0} zu breit",
                                s.id
                            ));
                        }
                    }
                }
            }
        }
        assert!(findings.is_empty(), "abgeschnitten:\n{}", findings.join("\n"));
    }

    /// In the smallest window of each size and in a tall one the window model
    /// ends at the bottom of the window: sidebar and page scroll inside, the
    /// window never.
    #[test]
    fn the_preview_never_needs_a_second_scroll_bar() {
        let mut findings = Vec::new();
        for points in font_size::POINTS_RANGE.0..=font_size::POINTS_RANGE.1 {
            let (width, smallest) = font_size::min_window(points);
            for height in [smallest, 1400.0] {
                let ctx = egui::Context::default();
                crate::install(&ctx);
                let mut page = SamplePage::preview();
                page.size = points;
                for key in ["", "hf_darstellung", "hf_overlay"] {
                    page.page = key.to_string();
                    for _ in 0..4 {
                        let input = egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height))),
                            ..Default::default()
                        };
                        let mut output = ctx.run_ui(input, |ui| {
                            page.show(ui);
                        });
                        output.textures_delta.clear();
                    }
                    if page.frame_overflow_y > 0.5 {
                        findings
                            .push(format!("{key} Größe {points} Höhe {height}: {:.0} zu hoch", page.frame_overflow_y));
                    }
                }
            }
        }
        assert!(findings.is_empty(), "zweiter Rollbalken nötig:\n{}", findings.join("\n"));
    }

    /// The back up, read in and clear actions stand in the tab row of My
    /// Inventory, right of the tabs, always in sight; clear opens its panel
    /// with the clear buttons.
    #[test]
    fn my_inventory_shows_its_rare_actions_beside_the_tabs() {
        use versekit_core::language::t;
        let (width, height) = (1226.0, 548.0);
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        let mut page = SamplePage::preview();
        page.page = "hf_meinlager".to_string();
        let frame = |ctx: &egui::Context, page: &mut SamplePage, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height))),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                page.show(ui);
            });
            output.textures_delta.clear();
            output.platform_output.accesskit_update.map(|u| u.nodes).unwrap_or_default()
        };
        let find = |nodes: &[(egui::accesskit::NodeId, egui::accesskit::Node)], name: &str| {
            nodes.iter().find(|(_, n)| n.label() == Some(name)).and_then(|(_, n)| n.bounds())
        };
        let mut nodes = Vec::new();
        for _ in 0..3 {
            nodes = frame(&ctx, &mut page, Vec::new());
        }
        let tab = find(&nodes, &t("s_ml_items").to_string())
            .or_else(|| {
                nodes
                    .iter()
                    .find(|(_, n)| n.label().is_some_and(|l| l.starts_with(&t("s_ml_items"))))
                    .and_then(|(_, n)| n.bounds())
            })
            .expect("Reiter Items");
        let mut last_right = tab.x1;
        for key in ["s_mlg_aktion_sichern", "s_mlg_aktion_einlesen", "s_mlg_aktion_umziehen", "s_mlg_aktion_leeren"] {
            let b = find(&nodes, &t(key)).unwrap_or_else(|| panic!("{key} fehlt"));
            assert!(b.x0 > last_right, "{key} nicht rechts vom Vorigen");
            assert!((b.y0 + b.y1) / 2.0 > tab.y0 && (b.y0 + b.y1) / 2.0 < tab.y1, "{key} nicht in der Reiterzeile");
            last_right = b.x1;
        }
        assert!(last_right > width as f64 * 0.85, "nicht rechtsbündig: {last_right}");
        assert!(find(&nodes, &t("s_lg_leeren")).is_none(), "Leeren-Knöpfe ohne Klick sichtbar");
        let clear = find(&nodes, &t("s_mlg_aktion_leeren")).expect("Leeren");
        let at = egui::pos2(((clear.x0 + clear.x1) / 2.0) as f32, ((clear.y0 + clear.y1) / 2.0) as f32);
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(&ctx, &mut page, vec![egui::Event::PointerMoved(at), button(true)]);
        frame(&ctx, &mut page, vec![button(false)]);
        let nodes = frame(&ctx, &mut page, Vec::new());
        assert!(find(&nodes, &t("s_lg_leeren")).is_some(), "„Leeren“ öffnet keinen Bereich");
    }

    /// A narrowed My Inventory shows the reset button at the top right
    /// of the head, beside the heading, and a click there resets it.
    #[test]
    fn resetting_my_inventory_sits_at_the_top_right() {
        // The button is looked for by its German name: the language of this thread stays German whatever other
        // tests switch the one of the process to meanwhile.
        language::set_thread_language(language::Choice::De);
        let (width, height) = (1226.0, 548.0);
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        let mut page = SamplePage::preview();
        page.page = "hf_meinlager".to_string();
        page.stock.search = "gold".to_string();
        let mut place = None;
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                page.show(ui);
            });
            output.textures_delta.clear();
            let word = versekit_core::language::t("ff_zuruecksetzen");
            place = output.platform_output.accesskit_update.and_then(|update| {
                update.nodes.iter().find(|(_, n)| n.label() == Some(word.as_str())).and_then(|(_, n)| n.bounds())
            });
        }
        let rect = place.expect("Knopf „Auswahl zurücksetzen“ fehlt");
        assert!(rect.x1 > width as f64 * 0.85, "nicht am rechten Rand: {rect:?}");
        assert!(rect.y0 < height as f64 * 0.35, "nicht oben in der Kopfzeile: {rect:?}");
        let at = egui::pos2(((rect.x0 + rect.x1) / 2.0) as f32, ((rect.y0 + rect.y1) / 2.0) as f32);
        for (pressed, step) in [(true, 0), (false, 1), (false, 2)] {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height))),
                ..Default::default()
            };
            input.events.push(egui::Event::PointerMoved(at));
            if step < 2 {
                input.events.push(egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            let mut output = ctx.run_ui(input, |ui| {
                page.show(ui);
            });
            output.textures_delta.clear();
        }
        assert!(!page.stock.is_narrowed(), "Klick hat nicht zurückgesetzt");
    }

    /// My Inventory in the preview window with entry form and refinery open,
    /// scrolled to the top and to the bottom: the list keeps its share of the
    /// visible page.
    #[test]
    fn the_stock_list_keeps_its_room_under_open_forms() {
        let mut findings = Vec::new();
        for points in font_size::POINTS_RANGE.0..=font_size::POINTS_RANGE.1 {
            // The smallest window of this size, a laptop window, a tall one.
            let (narrow, low) = font_size::min_window(points);
            for (width, height) in [(narrow, low), (1000.0, 615.0), (1400.0, 1000.0)] {
                let ctx = egui::Context::default();
                crate::install(&ctx);
                let mut page = SamplePage::preview();
                page.size = points;
                page.page = "hf_meinlager".to_string();
                page.stock.open_forms();
                for wheel in [0.0, -5000.0] {
                    for step in 0..5 {
                        let mut input = egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height))),
                            ..Default::default()
                        };
                        if step == 1 && wheel != 0.0 {
                            input.events.push(egui::Event::PointerMoved(egui::pos2(width * 0.7, height * 0.5)));
                            input.events.push(egui::Event::MouseWheel {
                                unit: egui::MouseWheelUnit::Point,
                                delta: egui::vec2(0.0, wheel),
                                phase: egui::TouchPhase::Move,
                                source: egui::MouseWheelSource::Unknown,
                                modifiers: egui::Modifiers::NONE,
                            });
                        }
                        let mut output = ctx.run_ui(input, |ui| {
                            page.show(ui);
                        });
                        output.textures_delta.clear();
                    }
                    let Some((_, _, visible, drawn, row)) = stock_page::StockState::list_room_seen(&ctx) else {
                        findings.push(format!("Größe {points} {width}×{height}: Liste nicht gezeichnet"));
                        continue;
                    };
                    let rows = drawn / row;
                    if rows < layout::LIST_MIN_ROWS - 0.5 || drawn < visible * layout::LIST_SHARE - 12.0 {
                        findings.push(format!(
                            "Größe {points} {width}×{height} Rad {wheel}: {rows:.1} Zeilen, {drawn:.0} von {visible:.0}"
                        ));
                    }
                }
            }
        }
        assert!(findings.is_empty(), "Liste zu klein:\n{}", findings.join("\n"));
    }

    /// With own data the error report page stands in the window and, with the
    /// test receiver, a report is sent to this computer only: in the window,
    /// nothing sticks out, and the receiver gets exactly one request.
    #[test]
    fn the_report_page_works_in_the_window_with_own_data() {
        let _language = crate::language_lock();
        language::set_thread_language(language::Choice::De);
        let dir = std::env::temp_dir().join(format!("versekit-fenster-bericht-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("Ordner");
        // A made-up computer with a game folder of its own and the network off: nothing of the
        // shell the check runs in, nor a game installed on this computer, counts.
        let mut page = SamplePage::with_settings(Paths::new(super::real_data::host_apart(&dir)));
        page.use_test_receiver();
        // A sentence of the player before the report is made, so that one click sends: without one the first
        // click only asks for it, and a sentence typed after the report was shown makes the click refresh it.
        page.diagnose.as_mut().expect("the page has a data folder").set_message("Probe");
        page.page = "hf_diagnose".to_string();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let (width, height) = font_size::BASE_MIN_SIZE;
        let frame = |page: &mut SamplePage, ctx: &egui::Context| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height))),
                ..Default::default()
            };
            let mut overflow = 0.0;
            let mut output = ctx.run_ui(input, |ui| overflow = page.show(ui));
            output.textures_delta.clear();
            overflow
        };
        let mut worst = 0.0_f32;
        for _ in 0..40 {
            worst = worst.max(frame(&mut page, &ctx));
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        assert!(worst <= 0.5, "{worst:.0} zu breit");
        let state = page.diagnose.as_mut().expect("the page has a data folder");
        assert!(state.shown_text().is_some_and(|t| t.starts_with("VerseKit ")), "the report is there");
        state.set_consent_for_test();
        state.send(&ctx);
        for _ in 0..200 {
            frame(&mut page, &ctx);
            if page.diagnose.as_ref().is_some_and(|s| !s.is_sending()) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let state = page.diagnose.as_ref().expect("page");
        assert_eq!(state.test_receiver_count(), 1, "one report reached the test receiver: {:?}", state.notice());
        assert_eq!(state.notice().map(|n| n.0.clone()), Some(language::t("s_di_ab_ok")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Every page of the preview (the overview, every page of the sidebar,
    /// what's-new, the error report and "Schiffe benennen") in every size, in the smallest and a
    /// wide window, in the plain and the square scheme: nothing may stick out
    /// of the page.
    #[test]
    fn every_page_fits_at_every_size() {
        let (width, height) = font_size::BASE_MIN_SIZE;
        let mut pages: Vec<&str> = vec!["", "hf_wasistneu", "hf_diagnose", ASOP_PAGE, TOP_PAGE.0];
        pages.extend(NAV.iter().flat_map(|(_, entries)| entries.iter().map(|(page, _)| *page)));
        let mut findings = Vec::new();
        for id in ["original", "krt"] {
            theme::set_thread_current(id);
            for points in font_size::POINTS_RANGE.0..=font_size::POINTS_RANGE.1 {
                for window_width in [width, 1800.0] {
                    let ctx = egui::Context::default();
                    crate::install(&ctx);
                    let mut page = SamplePage::preview();
                    page.size = points;
                    for key in &pages {
                        page.page = key.to_string();
                        let mut overflow = 0.0;
                        for _ in 0..3 {
                            let input = egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    egui::vec2(window_width, height),
                                )),
                                ..Default::default()
                            };
                            let mut output = ctx.run_ui(input, |ui| overflow = page.show(ui));
                            output.textures_delta.clear();
                        }
                        if overflow > 0.5 {
                            let name = if key.is_empty() { "Übersicht" } else { key };
                            findings.push(format!(
                                "{id} {name} Größe {points} Breite {window_width}: {overflow:.0} zu breit"
                            ));
                        }
                    }
                }
            }
        }
        assert!(findings.is_empty(), "abgeschnitten ({}):\n{}", findings.len(), findings.join("\n"));
    }

    /// The accent rectangles two points high that the first frame of the preview paints, and the bar rectangle.
    fn head_lines(id: &str) -> (Vec<egui::Rect>, Option<egui::Rect>) {
        theme::set_thread_current(id);
        let scheme = theme::current();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let mut page = SamplePage::preview();
        let (width, height) = font_size::BASE_MIN_SIZE;
        let mut lines = Vec::new();
        let mut bar = None;
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                page.show(ui);
            });
            output.textures_delta.clear();
            lines.clear();
            bar = None;
            for clipped in &output.shapes {
                if let egui::Shape::Rect(rect) = &clipped.shape {
                    if rect.fill == color(scheme.accent) && (rect.rect.height() - HEAD_LINE_HEIGHT).abs() < 0.01 {
                        lines.push(rect.rect);
                    }
                    // The first one is the head bar, the topmost thing of the page.
                    if bar.is_none()
                        && rect.fill == color(scheme.bar)
                        && (rect.rect.height() - widgets::BAR_HEIGHT).abs() < 0.01
                    {
                        bar = Some(rect.rect);
                    }
                }
            }
        }
        (lines, bar)
    }

    /// The square scheme draws the orange line under the head bar, as wide as the bar, along its lower edge;
    /// the round schemes draw none.
    #[test]
    fn the_square_scheme_has_a_line_under_the_head_bar() {
        let (lines, bar) = head_lines("krt");
        let bar = bar.expect("the head bar is drawn");
        let at_the_edge: Vec<_> = lines
            .iter()
            .filter(|l| (l.bottom() - bar.bottom()).abs() < 0.01 && (l.left() - bar.left()).abs() < 0.01 && (l.width() - bar.width()).abs() < 0.01)
            .collect();
        assert_eq!(at_the_edge.len(), 1, "one line along the lower edge of the bar, got {lines:?}");
        let (lines, bar) = head_lines("original");
        let bar = bar.expect("the head bar is drawn");
        assert!(lines.iter().all(|l| (l.bottom() - bar.bottom()).abs() > 0.01), "no line in a round scheme");
    }

    /// The window takes a catalog built in the background over by itself: the
    /// frames of the page run the take-over, nothing else has to ask for it.
    #[test]
    fn the_page_takes_a_catalog_built_in_the_background_over() {
        use std::sync::mpsc;
        use std::time::Duration;

        let stale = blueprint_list::ListData::sample().catalog;
        let mut new = stale.clone();
        new.blueprints.pop();
        let (go, wait) = mpsc::channel::<()>();
        let copy = new.clone();
        // Builds when the test says so; a test that ends before drops `go`, and the build ends with it.
        let building = versekit_gamedata::list::Pending::spawn(move || {
            wait.recv().ok()?;
            Some(copy)
        });
        let mut page = SamplePage::preview();
        let dir = std::env::temp_dir().join(format!("versekit-seite-katalog-{}", versekit_core::testdir::run_tag()));
        std::fs::create_dir_all(&dir).unwrap();
        let text = dir.to_string_lossy().into_owned();
        let host = versekit_core::paths::Host::new(
            versekit_core::pypath::Flavor::native(),
            &[(versekit_core::legacy::ENV_HOME, &text)],
            &text,
        );
        let paths = versekit_core::paths::Paths::new(host);
        page.blueprints = blueprint_list::ListData::load_from(&paths, Some(stale.clone()), building).expect("a list");
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let frame = |page: &mut SamplePage| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                page.show(ui);
            });
            output.textures_delta.clear();
        };
        frame(&mut page);
        assert!(page.blueprints.is_building() && page.blueprints.catalog == stale);
        go.send(()).unwrap();
        for _ in 0..1000 {
            frame(&mut page);
            if !page.blueprints.is_building() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(page.blueprints.catalog, new);
    }

    /// The page "Installation & Pfade" with own data in its three states
    /// (folders given from outside, a question for a chosen folder, a switch
    /// that waits for a restart) in every size, both languages, the smallest
    /// and a wide window, two schemes: nothing may stick out.
    #[test]
    fn the_paths_page_fits_in_every_state() {
        use versekit_core::storage_move::Outcome;
        let _language = crate::language_lock();
        let (width, height) = font_size::BASE_MIN_SIZE;
        let root = std::env::temp_dir().join(format!("versekit-pfade-rand-{}", versekit_core::testdir::run_tag()));
        let root_text = root.to_string_lossy().into_owned();
        let long = root.join("ein sehr langer Ordnername ohne Leerzeichen und mit vielen Zeichen darin").join("Daten");
        std::fs::create_dir_all(&long).expect("Ordner");
        let mut findings = Vec::new();
        for state in ["given", "question", "restart"] {
            // A made-up computer: whatever the real environment fixes must not reach the real data.
            let home = [
                ("USERPROFILE", root_text.as_str()),
                ("HOME", root_text.as_str()),
                ("APPDATA", root_text.as_str()),
                ("XDG_CONFIG_HOME", root_text.as_str()),
            ];
            let mut host = versekit_core::paths::Host::new(versekit_core::pypath::Flavor::native(), &home, &root_text);
            if state == "given" {
                host.set_var(versekit_core::legacy::ENV_HOME, &long.to_string_lossy());
                host.set_var(versekit_core::paths::ENV_INSTALL_DIR, &long.to_string_lossy());
            }
            let paths = Paths::new(host);
            let ctx = egui::Context::default();
            crate::install(&ctx);
            let mut page = SamplePage::with_settings(paths);
            page.page = "hf_ordner".to_string();
            match state {
                "question" => {
                    let target = long.join("Ziel").to_string_lossy().into_owned();
                    let paths = page.store.as_ref().map(|store| &store.paths).expect("store");
                    let old = paths.app_folder();
                    std::fs::create_dir_all(std::path::Path::new(&old).join("Intern")).expect("Ordner");
                    std::fs::write(std::path::Path::new(&old).join("Intern").join("notiz.txt"), "n").expect("Datei");
                    page.data_folder.chosen(&ctx, paths, &target);
                    assert!(page.data_folder.question().is_some(), "the question is up");
                }
                "restart" => page.data_folder.pretend_switched(&Outcome::Moved {
                    copied: 12,
                    kept: vec!["a".into(), "b".into()],
                    old: long.to_string_lossy().into_owned(),
                }),
                _ => {}
            }
            for lang in [language::Choice::De, language::Choice::En] {
                language::set_thread_language(lang);
                for id in ["original", "krt"] {
                    theme::set_thread_current(id);
                    for points in font_size::POINTS_RANGE.0..=font_size::POINTS_RANGE.1 {
                        for window_width in [width, 1800.0] {
                            page.size = points;
                            let mut overflow = 0.0;
                            for _ in 0..3 {
                                let input = egui::RawInput {
                                    screen_rect: Some(egui::Rect::from_min_size(
                                        egui::Pos2::ZERO,
                                        egui::vec2(window_width, height),
                                    )),
                                    ..Default::default()
                                };
                                let mut output = ctx.run_ui(input, |ui| overflow = page.show(ui));
                                output.textures_delta.clear();
                            }
                            if overflow > 0.5 {
                                findings.push(format!(
                                    "{state} {id} {lang:?} Größe {points} Breite {window_width}: {overflow:.0} zu breit"
                                ));
                            }
                        }
                    }
                }
            }
        }
        language::set_thread_language(language::Choice::De);
        let _ = std::fs::remove_dir_all(&root);
        assert!(findings.is_empty(), "abgeschnitten ({}):\n{}", findings.len(), findings.join("\n"));
    }

    /// On real data without a catalog (no game data, no scmdb) the list and
    /// the progress say that the catalog is missing, not that the page comes
    /// with a later build.
    #[test]
    fn without_a_catalog_the_pages_say_so() {
        use versekit_core::language::t;

        let _language = crate::language_lock();
        let dir = std::env::temp_dir().join(format!("versekit-kein-katalog-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // A game folder of its own, without a `Data.p4k`: the archive of an installation on this
        // computer would be read, and the build would take its time.
        let mut page = SamplePage::with_settings(Paths::new(super::real_data::host_apart(&dir)));
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let shown = |ctx: &egui::Context, page: &mut SamplePage| -> Vec<String> {
            fn collect(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
                match shape {
                    egui::epaint::Shape::Text(text) => out.push(text.galley.text().to_string()),
                    egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, out)),
                    _ => {}
                }
            }
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1226.0, 700.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                page.show(ui);
            });
            output.textures_delta.clear();
            let mut texts = Vec::new();
            output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
            texts
        };
        for name in ["hf_liste", "hf_fortschritt"] {
            page.page = name.to_string();
            // The build without any source ends at once; wait until it has.
            let mut texts = Vec::new();
            for _ in 0..500 {
                texts = shown(&ctx, &mut page);
                if texts.iter().any(|x| *x == t("kein_katalog")) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert!(texts.iter().any(|x| *x == t("kein_katalog")), "{name}: {texts:?}");
            assert!(!texts.iter().any(|x| x.contains(glance::NOT_YET)), "{name}: {texts:?}");
        }
        // The case that no build is running at all says the same.
        page.blueprints = blueprint_list::ListData::sample();
        page.blueprints_none = true;
        let texts = shown(&ctx, &mut page);
        assert!(texts.iter().any(|x| *x == t("kein_katalog")), "{texts:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The community mark is chosen in screen pixels: with Windows scaling it
    /// takes a bigger prepared size and stays the same size on screen, never
    /// stretched.
    #[test]
    fn community_mark_stays_sharp_when_scaled() {
        assert_eq!(widgets::community_size(72.0), 64);
        assert_eq!(widgets::community_size(144.0), 128);
        assert_eq!(widgets::community_height(72.0, 1.0), 64.0);
        assert_eq!(widgets::community_height(72.0, 2.0), 64.0);
        assert_eq!(widgets::community_height(72.0, 1.5), 96.0 / 1.5);
        for pixels_per_point in [1.0, 1.25, 1.5, 1.75, 2.0, 2.5] {
            let points = widgets::community_height(72.0, pixels_per_point);
            let pixels = points * pixels_per_point;
            assert!(widgets::COMMUNITY_SIZES.contains(&(pixels.round() as u32)), "{pixels_per_point}: {pixels}");
            assert!(points <= 72.0 + 0.01, "{pixels_per_point}: {points}");
        }
    }

    /// Every page that waits for a reading of the game archive shows its data, so it marks those of a test channel.
    #[test]
    fn every_page_that_reads_the_archive_marks_the_data_of_a_test_channel() {
        for (topic, pages) in TOPIC_PAGES {
            for page in pages {
                assert!(ARCHIVE_PAGES.contains(page), "{page} ({topic}) reads the archive but says nothing of a test channel");
            }
        }
    }
}
