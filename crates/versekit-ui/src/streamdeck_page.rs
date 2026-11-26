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

//! The page "Tasten & Stream Deck": the switch of the interface for the Stream Deck plugin with what became of it,
//! the setting up of the plugin, and the keys for the game actions on the Stream Deck that have none on the
//! keyboard ([`versekit_core::streamdeck_fill`]): a plan first, then the writing on a second click, afterwards the
//! way back.
//!
//! The interface itself runs in the overlay; this page reads its report over the interface (`/v1/verbindung`) in a
//! thread of its own, every few seconds while it is shown.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use egui::{Role, Ui, WidgetInfo};
use versekit_core::language::{Arg, t, tf};
use versekit_core::paths::Paths;
use versekit_core::streamdeck::{self, SETTING_ON};
use versekit_core::streamdeck_fill::{self, Item, Outcome, Record};
use versekit_core::theme::Scheme;
use versekit_core::jsonio::Value;

use crate::settings;
use crate::widgets::{self, ButtonKind};

const ASK_EVERY: Duration = Duration::from_secs(2);

/// What the overlay reports of the interface.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    /// The interface answered.
    pub reachable: bool,
    pub connected: bool,
    pub failures: u64,
    pub device_hint: Option<String>,
    pub in_use: Vec<String>,
    /// The version the installed plugin told, and the one of the plugin file beside VerseKit.
    pub plugin_version: Option<String>,
    pub shipped_version: Option<String>,
}

impl Report {
    /// Is the installed plugin older than the one VerseKit brings?
    pub fn plugin_old(&self) -> bool {
        match (&self.plugin_version, &self.shipped_version) {
            (Some(installed), Some(shipped)) => streamdeck::version_older(installed, shipped),
            _ => false,
        }
    }
}

/// The version of the plugin file beside VerseKit.
fn shipped_version() -> Option<String> {
    let file = streamdeck::plugin_file()?;
    streamdeck::plugin_file_version(&std::fs::read(file).ok()?)
}

/// The plan worked out in a thread: the items and the names of the actions.
type Planned = Option<Result<(Vec<Item>, Vec<String>), String>>;

#[derive(Default)]
pub struct StreamDeckState {
    paths: Option<Paths>,
    /// On a copy of the data the interface does not run.
    copy: bool,
    on: bool,
    /// The dials without their name (`streamdeck_regler_kompakt`).
    compact: bool,
    /// The page of the device follows the game (`streamdeck_seite_auto`).
    auto_page: bool,
    report: Arc<Mutex<Report>>,
    asked: Option<Instant>,
    asking: Arc<Mutex<bool>>,
    /// The keyboard lines the player emptied in the game stay empty (off: they get a key too).
    keep_deleted: bool,
    /// The actions of the plan that already have a key are shown (folded under one line until then).
    show_done: bool,
    planned: Arc<Mutex<Planned>>,
    planning: bool,
    record: Option<Record>,
    message: Option<(String, bool)>,
    /// When the plugin file was handed to the Stream Deck program.
    opened: Option<Instant>,
    /// The plan worked out by itself for the numbers beside the button, and whether the game is quiet.
    counted: Arc<Mutex<Option<Counted>>>,
    counted_at: Option<Instant>,
    counting: Arc<Mutex<bool>>,
    /// The player closed the preview the page opened by itself: it does not open again until something changes.
    preview_closed: Option<(Vec<String>, bool, String)>,
    /// „Später“ in the bar of the main window: hidden until the next start.
    bar_hidden: bool,
}

/// The plan of the game actions the plugin reported, worked out without a click, for the numbers beside the button.
#[derive(Clone, Debug)]
struct Counted {
    /// What it was worked out for: the actions, whether deleted lines stay, the state of the `actionmaps.xml`.
    key: (Vec<String>, bool, String),
    /// The items and the names of their actions.
    items: Result<(Vec<Item>, Vec<String>), String>,
    /// `Err` with the reason while the game runs or nobody can tell.
    quiet: Result<(), String>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Asks the interface for its report; `None` when it does not answer.
fn ask(paths: &Paths) -> Option<Report> {
    let key = streamdeck::read_key_file(paths)?;
    let address = std::net::SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, key.port));
    let mut stream = std::net::TcpStream::connect_timeout(&address, Duration::from_secs(1)).ok()?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n",
        streamdeck::PATH_REPORT,
        key.port,
        key.key
    );
    use std::io::{Read, Write};
    stream.write_all(request.as_bytes()).ok()?;
    let mut answer = Vec::new();
    stream.take(256 * 1024).read_to_end(&mut answer).ok()?;
    let text = String::from_utf8(answer).ok()?;
    let (head, body) = text.split_once("\r\n\r\n")?;
    if head.split(' ').nth(1) != Some("200") {
        return None;
    }
    let value = versekit_core::jsonio::loads(body).ok()?;
    Some(Report {
        reachable: true,
        connected: value.get("verbunden") == Some(&Value::Bool(true)),
        failures: match value.get("fehlversuche") {
            Some(Value::Int(n)) => n.parse().unwrap_or(0),
            _ => 0,
        },
        device_hint: value.get("geraet_ohne_profil").and_then(Value::as_str).map(String::from),
        in_use: match value.get("spielaktionen") {
            Some(Value::Array(list)) => list.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
            _ => Vec::new(),
        },
        plugin_version: value.get("plugin_version").and_then(Value::as_str).map(String::from),
        shipped_version: shipped_version(),
    })
}

/// Why the keys cannot be written while the game is not quiet, exactly ([`versekit_core::game_start::not_quiet`]);
/// a reason other than the last one goes to the error log.
fn game_not_quiet(paths: &Paths) -> String {
    static LAST: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
    let reason = versekit_core::game_start::not_quiet(paths);
    let said = format!("{reason:?}");
    let mut last = LAST.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if last.as_deref() != Some(said.as_str()) {
        versekit_core::errors::note("streamdeck.eintragen_gesperrt", &said);
        *last = Some(said);
    }
    not_quiet_text(reason.as_ref())
}

fn not_quiet_text(reason: Option<&versekit_core::game_start::NotQuiet>) -> String {
    use versekit_core::game_start::NotQuiet;
    match reason {
        Some(NotQuiet::Process) => t("sd_f_spiel_prozess"),
        Some(fresh @ NotQuiet::LogFresh { age, listed }) => {
            let minutes = fresh.minutes_left().unwrap_or(1) as usize;
            match (listed, minutes) {
                (true, 1) => t("sd_f_spiel_beendet_1"),
                (true, n) => tf("sd_f_spiel_beendet", &[("0", Arg::from(n))]),
                (false, n) => tf("sd_f_spiel_log", &[("0", Arg::from(*age as usize)), ("1", Arg::from(n))]),
            }
        }
        Some(NotQuiet::Unknown) => t("sd_f_spiel_unklar"),
        None => t("sd_f_spiel_laeuft"),
    }
}

/// The plan for the game actions of the plugin, with the bindings, the factory setting and the hotkeys of VerseKit.
fn work_out_plan(paths: &Paths, in_use: &[String], keep_deleted: bool) -> Result<(Vec<Item>, Vec<String>), String> {
    let profile = versekit_gamedata::control_profile::profile(paths);
    let own = match versekit_core::joysticks::actionmaps_path(paths, None) {
        Some(file) if std::path::Path::new(&file).is_file() => versekit_core::joysticks::bindings(paths, Some(&file), None),
        _ => return Err(t("s_js_f_datei")),
    };
    let hotkeys: Vec<versekit_core::overlay::Hotkey> =
        versekit_core::overlay::HotkeyChoice::read_all(paths).iter().filter_map(|c| c.hotkey()).collect();
    let mut items = streamdeck_fill::plan(in_use, &own, &profile, &hotkeys, keep_deleted);
    let lang = versekit_core::language::current();
    let folder = paths.game_folder();
    let labels = match &folder {
        Some(folder) => versekit_core::controls::labels_from_folder(&profile, folder, lang),
        None => Default::default(),
    };
    // As on the deck: the short name of the profiles where the game has no text in the language of the program.
    let without_text = versekit_core::controls::without_own_text(&profile, folder.as_deref(), lang);
    let name_of = |action: &str| {
        let plain = action.split_once('.').map_or(action, |(_, p)| p);
        let label = labels.get(plain);
        let game = label.map_or_else(|| versekit_core::controls::technical_readable(plain), |l| l.name.clone());
        let own_text = !without_text.contains(plain) && label.is_some_and(|l| l.real);
        streamdeck::game_action_name(plain, &game, own_text, lang)
    };
    // The action the key alone presses, by its name: the line of the plan says it.
    for item in &mut items {
        if let Outcome::Relocate { alone: Some(alone), .. } = &mut item.outcome {
            *alone = name_of(alone);
        }
    }
    let names = items.iter().map(|item| name_of(&item.action)).collect();
    Ok((items, names))
}

impl StreamDeckState {
    /// The page on these data; `copy` for a copy of the data, where the interface does not run.
    pub fn with_paths(paths: &Paths, copy: bool) -> StreamDeckState {
        let on = paths.setting_bool(SETTING_ON, false);
        let compact = paths.setting_bool(streamdeck::SETTING_DIAL_COMPACT, false);
        let auto_page = paths.setting_bool(streamdeck::SETTING_AUTO_PAGE, true);
        let record = streamdeck_fill::load_record(paths);
        StreamDeckState {
            paths: Some(Paths::new(paths.host.clone())),
            copy,
            on,
            compact,
            auto_page,
            record,
            ..StreamDeckState::default()
        }
    }

    fn set_compact(&mut self, on: bool) {
        self.compact = on;
        if let Some(paths) = &self.paths {
            paths.set_setting(streamdeck::SETTING_DIAL_COMPACT, Value::Bool(on));
        }
    }

    fn set_auto_page(&mut self, on: bool) {
        self.auto_page = on;
        if let Some(paths) = &self.paths {
            paths.set_setting(streamdeck::SETTING_AUTO_PAGE, Value::Bool(on));
        }
    }

    fn set_on(&mut self, on: bool) {
        self.on = on;
        if let Some(paths) = &self.paths {
            paths.set_setting(SETTING_ON, Value::Bool(on));
            // The overlay reads its settings again and starts or stops the interface.
            versekit_core::window_signal::send_to(versekit_core::window_signal::Role::Overlay, versekit_core::window_signal::Signal::Settings);
        }
    }

    /// Asks the interface for its report now and then, in a thread.
    fn keep_asking(&mut self, ui: &Ui) {
        let Some(paths) = &self.paths else { return };
        if !self.on || self.copy || self.asked.is_some_and(|at| at.elapsed() < ASK_EVERY) || *lock(&self.asking) {
            return;
        }
        self.asked = Some(Instant::now());
        *lock(&self.asking) = true;
        let (paths, report, asking, ctx) = (Paths::new(paths.host.clone()), self.report.clone(), self.asking.clone(), ui.ctx().clone());
        let spawned = std::thread::Builder::new().name("streamdeck-bericht".into()).spawn(move || {
            *lock(&report) = ask(&paths).unwrap_or_default();
            *lock(&asking) = false;
            ctx.request_repaint();
        });
        if spawned.is_err() {
            *lock(&self.asking) = false;
        }
        ui.ctx().request_repaint_after(ASK_EVERY);
    }

    /// Works the plan out again in a thread when the reported actions, the set or the `actionmaps.xml` changed, and
    /// looks whether the game is quiet; not more often than the report is asked for.
    fn keep_counting(&mut self, ui: &Ui) {
        let Some(paths) = &self.paths else { return };
        if !self.on || self.copy || self.counted_at.is_some_and(|at| at.elapsed() < ASK_EVERY) || *lock(&self.counting) {
            return;
        }
        let in_use = lock(&self.report).in_use.clone();
        if in_use.is_empty() {
            *lock(&self.counted) = None;
            return;
        }
        self.counted_at = Some(Instant::now());
        *lock(&self.counting) = true;
        let before = lock(&self.counted).clone();
        let (paths, counted, counting, keep, ctx) =
            (Paths::new(paths.host.clone()), self.counted.clone(), self.counting.clone(), self.keep_deleted, ui.ctx().clone());
        let lang = versekit_core::language::current();
        let spawned = std::thread::Builder::new().name("streamdeck-zaehlen".into()).spawn(move || {
            let key = (in_use, keep, versekit_core::anti_afk_key::stamp(&paths));
            let items = match before {
                Some(done) if done.key == key => done.items,
                _ => versekit_core::language::with_language(lang, || work_out_plan(&paths, &key.0, key.1)),
            };
            let quiet = versekit_core::game_files::check_quiet(&paths)
                .map_err(|_| versekit_core::language::with_language(lang, || game_not_quiet(&paths)));
            *lock(&counted) = Some(Counted { key, items, quiet });
            *lock(&counting) = false;
            ctx.request_repaint();
        });
        if spawned.is_err() {
            *lock(&self.counting) = false;
        }
        ui.ctx().request_repaint_after(ASK_EVERY);
    }

    /// How many game actions of the deck have no key on the keyboard, and whether the game runs (then nothing can
    /// be put in); `None` while nothing is counted or nothing is missing.
    pub fn missing(&self) -> Option<(usize, bool)> {
        if !self.on || self.copy {
            return None;
        }
        let counted = lock(&self.counted).clone()?;
        let (items, _) = counted.items.ok()?;
        let missing = streamdeck_fill::count(&items).without_key;
        (missing > 0).then_some((missing, counted.quiet.is_err()))
    }

    /// While keys are missing the preview stands open by itself, unless the player closed it for this state of the
    /// keys and the bindings.
    fn open_preview_by_itself(&mut self) {
        if self.planning || lock(&self.planned).is_some() || self.missing().is_none() {
            return;
        }
        let Some(counted) = lock(&self.counted).clone() else { return };
        if self.preview_closed.as_ref() == Some(&counted.key) {
            return;
        }
        if let Ok(plan) = counted.items {
            *lock(&self.planned) = Some(Ok(plan));
            self.planning = true;
        }
    }

    /// The preview is closed (cancelled or written): it does not open by itself again for the same state.
    fn close_preview(&mut self) {
        self.preview_closed = lock(&self.counted).as_ref().map(|c| c.key.clone());
        *lock(&self.planned) = None;
        self.planning = false;
    }

    /// The numbers beside the button, and why it cannot be pressed (`None` when it can).
    fn count_text(&self) -> (String, Option<String>) {
        if self.copy {
            return (t("sd_f_hinweis"), Some(t("sd_s_kopie")));
        }
        if !self.on {
            return (t("sd_f_hinweis"), Some(t("sd_s_aus")));
        }
        if lock(&self.report).in_use.is_empty() {
            return (t("sd_f_hinweis"), Some(t("sd_f_keine_tasten")));
        }
        let Some(counted) = lock(&self.counted).clone() else { return (t("sd_f_zaehlt"), Some(t("sd_f_zaehlt"))) };
        let items = match counted.items {
            Ok((items, _)) => items,
            Err(reason) => return (tf("sd_f_fehler_zaehlen", &[("0", Arg::from(reason.as_str()))]), Some(reason)),
        };
        let count = streamdeck_fill::count(&items);
        let mut text = tf("sd_f_zahl", &[("0", Arg::from(count.without_key))]);
        if count.deleted > 0 {
            text.push(' ');
            text.push_str(&tf("sd_f_zahl_geloescht", &[("0", Arg::from(count.deleted))]));
        }
        let blocked = if count.addable == 0 {
            Some(if count.without_key == 0 {
                t("sd_f_alle_da")
            } else if count.deleted == count.without_key {
                t("sd_f_nur_geloescht")
            } else {
                t("sd_f_nichts_frei")
            })
        } else {
            counted.quiet.err()
        };
        (text, blocked)
    }

    /// Why the keys cannot be written now; `None` when the game is quiet.
    fn write_blocked(&self) -> Option<String> {
        match lock(&self.counted).as_ref().map(|c| c.quiet.clone()) {
            Some(Ok(())) => None,
            Some(Err(why)) => Some(why),
            None => Some(t("sd_f_spiel_laeuft")),
        }
    }

    fn status_text(&self) -> String {
        if self.copy {
            return t("sd_s_kopie");
        }
        if !self.on {
            return t("sd_s_aus");
        }
        let report = lock(&self.report).clone();
        let mut text = if !report.reachable {
            t("sd_s_antwortet_nicht")
        } else if report.connected {
            t("sd_s_verbunden")
        } else {
            t("sd_s_wartet")
        };
        if report.failures > 0 {
            text.push_str("  ·  ");
            text.push_str(&tf("sd_s_fehlversuche", &[("0", Arg::from(report.failures as usize))]));
        }
        if report.connected
            && report.plugin_old()
            && let (Some(installed), Some(shipped)) = (&report.plugin_version, &report.shipped_version)
        {
            text.push_str("  ·  ");
            text.push_str(&tf("sd_s_veraltet", &[("0", Arg::from(installed.as_str())), ("1", Arg::from(shipped.as_str()))]));
        }
        text
    }

    fn set_up(&mut self) {
        let host_ok = self.paths.as_ref().is_some_and(|p| streamdeck::app_installed(&p.host));
        if !host_ok {
            self.message = Some((t("sd_s_keine_app"), false));
            return;
        }
        let Some(file) = streamdeck::plugin_file() else {
            self.message = Some((tf("sd_s_keine_datei", &[("0", Arg::from(streamdeck::PLUGIN_FILE))]), false));
            return;
        };
        if !self.on {
            self.set_on(true);
        }
        if versekit_core::browser::open_stream_deck_plugin(&file) {
            self.opened = Some(Instant::now());
            self.message = Some((t("sd_s_bestaetigen"), true));
        } else {
            self.message = Some((tf("sd_s_oeffnen_fehler", &[("0", Arg::from(file.as_str()))]), false));
        }
    }

    fn start_plan(&mut self, ui: &Ui) {
        let Some(paths) = &self.paths else { return };
        let in_use = lock(&self.report).in_use.clone();
        if in_use.is_empty() {
            self.message = Some((t("sd_f_keine_tasten"), false));
            return;
        }
        self.planning = true;
        *lock(&self.planned) = None;
        let (paths, planned, keep, ctx) =
            (Paths::new(paths.host.clone()), self.planned.clone(), self.keep_deleted, ui.ctx().clone());
        let lang = versekit_core::language::current();
        let spawned = std::thread::Builder::new().name("streamdeck-plan".into()).spawn(move || {
            let result = versekit_core::language::with_language(lang, || work_out_plan(&paths, &in_use, keep));
            *lock(&planned) = Some(result);
            ctx.request_repaint();
        });
        if spawned.is_err() {
            self.planning = false;
        }
    }

    fn write_plan(&mut self, items: &[Item]) {
        let Some(paths) = &self.paths else { return };
        let now = versekit_core::contested::now() as i64;
        match streamdeck_fill::apply(paths, items, now) {
            Ok(record) => {
                self.message = Some((tf("sd_f_fertig", &[("0", Arg::from(record.entries.len()))]), true));
                self.record = Some(record);
                self.close_preview();
            }
            Err(failure) => {
                let reason = if failure.game_not_quiet() { game_not_quiet(paths) } else { failure.text() };
                self.message = Some((tf("sd_f_fehler", &[("0", Arg::from(reason))]), false));
            }
        }
    }

    fn undo(&mut self, whole: bool) {
        let Some(paths) = &self.paths else { return };
        let result = if whole { streamdeck_fill::restore_copy(paths).map(|()| Vec::new()) } else { streamdeck_fill::undo(paths) };
        match result {
            Ok(found) => {
                // Nothing of the record stood in the file any more (changed in the game since): the way back is dropped.
                let text = if whole {
                    t("sd_f_ganz_fertig")
                } else if found.is_empty() {
                    t("sd_f_zurueck_veraltet")
                } else {
                    tf("sd_f_zurueck_fertig", &[("0", Arg::from(found.len()))])
                };
                self.message = Some((text, true));
                self.record = streamdeck_fill::load_record(paths);
            }
            Err(failure) => {
                let reason = if failure.game_not_quiet() { game_not_quiet(paths) } else { failure.text() };
                self.message = Some((tf("sd_f_fehler", &[("0", Arg::from(reason))]), false));
            }
        }
    }
}

/// The bar at the top of the main window while keys of the Stream Deck have no binding: how many, that the game
/// must be closed while it runs, „Jetzt eintragen“ and „Später“ (hidden until the next start). True when the
/// player wants to put them in: the page of the keys opens with the preview.
/// The text of the bar: how many keys of the deck have no binding (one key in the singular), and that the game
/// must be closed first while it runs.
fn missing_text(lang: versekit_core::language::Lang, missing: usize, game_runs: bool) -> String {
    use versekit_core::language::{t_in, tf_in};
    let mut message =
        if missing == 1 { t_in(lang, "sd_leiste_1") } else { tf_in(lang, "sd_leiste", &[("0", Arg::from(missing))]) };
    if game_runs {
        message.push(' ');
        message.push_str(&t_in(lang, "sd_leiste_spiel"));
    }
    message
}

pub fn missing_bar(ui: &mut Ui, scheme: &Scheme, state: &mut StreamDeckState) -> bool {
    if state.paths.is_none() || state.bar_hidden {
        return false;
    }
    state.keep_asking(ui);
    state.keep_counting(ui);
    let Some((missing, game_runs)) = state.missing() else { return false };
    let message = missing_text(versekit_core::language::current(), missing, game_runs);
    let (now_word, later_word) = (t("sd_leiste_jetzt"), t("hf_update_spaeter"));
    let texts =
        settings::UpdateTexts { message: &message, update: &now_word, news: &later_word, update_blocked: None, urgent: false };
    let choice = settings::keys_bar(ui, scheme, &texts);
    ui.add_space(4.0);
    match choice {
        Some(settings::UpdateChoice::Update) => true,
        Some(settings::UpdateChoice::News) => {
            state.bar_hidden = true;
            false
        }
        None => false,
    }
}

/// A row of the list of the plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlanRow {
    /// The item with this index.
    Item(usize),
    /// The line that folds the actions with a key already: how many.
    Done(usize),
}

/// The rows of the list of the plan: first what needs something (written, on the mouse, deleted, no free key),
/// then one line for the actions that already have a key, and these only when `show_done`.
fn plan_rows(items: &[Item], show_done: bool) -> Vec<PlanRow> {
    let (done, needs): (Vec<usize>, Vec<usize>) = (0..items.len()).partition(|&i| items[i].outcome == Outcome::HasKey);
    let mut rows: Vec<PlanRow> = needs.into_iter().map(PlanRow::Item).collect();
    if !done.is_empty() {
        rows.push(PlanRow::Done(done.len()));
        if show_done {
            rows.extend(done.into_iter().map(PlanRow::Item));
        }
    }
    rows
}

/// One line of the plan: the name of the action and what happens with it.
fn plan_line(item: &Item, name: &str) -> String {
    let what = match &item.outcome {
        Outcome::Add(key) => {
            let shown = versekit_core::controls::input_readable(key, Some(versekit_core::joysticks::Kind::Keyboard));
            tf("sd_f_neu", &[("0", Arg::from(shown))])
        }
        Outcome::Move { mouse, key } => {
            let kind = Some(versekit_core::joysticks::Kind::Keyboard);
            let (mouse, key) =
                (versekit_core::controls::input_readable(mouse, kind), versekit_core::controls::input_readable(key, kind));
            tf("sd_f_verschieben", &[("0", Arg::from(mouse)), ("1", Arg::from(key))])
        }
        Outcome::NotSendable(input) => {
            let shown = versekit_core::controls::input_readable(input, Some(versekit_core::joysticks::Kind::Keyboard));
            tf("sd_f_maus", &[("0", Arg::from(shown))])
        }
        // „Alt Gr + P wird Alt + Umschalt + B, weil Alt Gr im Spiel auch P allein auslöst: Energie Waffen“.
        Outcome::Relocate { from, key, alone, .. } => {
            let kind = Some(versekit_core::joysticks::Kind::Keyboard);
            let readable = |input: &str| versekit_core::controls::input_readable(input, kind);
            let base = readable(from.rsplit('+').next().unwrap_or(from));
            let args = [("0", Arg::from(readable(from))), ("1", Arg::from(readable(key))), ("2", Arg::from(base))];
            match alone {
                Some(alone) => {
                    let mut with = args.to_vec();
                    with.push(("3", Arg::from(alone.as_str())));
                    tf("sd_f_umziehen_mit", &with)
                }
                None => tf("sd_f_umziehen", &args),
            }
        }
        Outcome::Refill(key) => {
            let shown = versekit_core::controls::input_readable(key, Some(versekit_core::joysticks::Kind::Keyboard));
            tf("sd_f_wieder", &[("0", Arg::from(shown))])
        }
        other => t(other.text_key()),
    };
    format!("{name}: {what}")
}

/// Draws the page.
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut StreamDeckState) {
    state.keep_asking(ui);
    state.keep_counting(ui);
    // Keys are missing: the preview stands open at once (after setting up, after a press on such a key, from the bar).
    state.open_preview_by_itself();
    // Once the plugin asks, the request to confirm is done: the state row says "Connected".
    if state.opened.is_some() && lock(&state.report).connected {
        state.opened = None;
        state.message = None;
    }
    if let Some(result) = lock(&state.planned).as_ref()
        && state.planning
        && let Err(reason) = result
    {
        state.message = Some((tf("sd_f_fehler", &[("0", Arg::from(reason.as_str()))]), false));
    }
    if let Some((text, ok)) = state.message.clone() {
        let (tone, icon) = if ok { (scheme.accent, "circle-check") } else { (scheme.yellow, "triangle-alert") };
        widgets::status_box(ui, scheme, tone, icon, &t("hf_streamdeck"), &text);
        ui.add_space(6.0);
    }
    let plugin = t("sd_s_gruppe");
    let fill = t("sd_f_gruppe");
    let plan: Option<(Vec<Item>, Vec<String>)> = lock(&state.planned).as_ref().and_then(|r| r.as_ref().ok()).cloned();
    // The actions the player deleted in the game: a box of their own above the list, so they are not missed, and the
    // switch whether they get a key too.
    let deleted: Vec<String> = plan
        .as_ref()
        .map(|(items, names)| {
            let was_deleted = |o: &Outcome| matches!(o, Outcome::Deleted | Outcome::Refill(_));
            items.iter().zip(names).filter(|(i, _)| was_deleted(&i.outcome)).map(|(_, n)| n.clone()).collect()
        })
        .unwrap_or_default();
    let deleted_rows = if deleted.is_empty() { 0 } else { 2 };
    let mut deleted_switched = None;
    let rows_of_plan: Vec<PlanRow> = plan.as_ref().map(|(items, _)| plan_rows(items, state.show_done)).unwrap_or_default();
    let plan_rows = plan.as_ref().map_or(0, |_| deleted_rows + rows_of_plan.len() + 1);
    let mut fold_switched = false;
    let (count_text, check_blocked) = state.count_text();
    let write_blocked = state.write_blocked();
    let record_rows = state.record.as_ref().map_or(0, |r| r.entries.len() + 1);
    let fill_rows = 1 + plan_rows + record_rows;
    let mut switched = None;
    let mut auto_switched = None;
    let mut set_up = false;
    let mut elgato = false;
    let mut check = false;
    let mut write = false;
    let mut cancel = false;
    let mut undo = false;
    let mut whole = false;
    let hint = lock(&state.report).device_hint.clone();
    let copy = state.copy;
    let mut compact_switched = None;
    settings::settings_page(ui, scheme, "streamdeck", &[(plugin.as_str(), 6), (fill.as_str(), fill_rows)], &mut |ui, g, r| {
        match (g, r) {
            (0, 0) => {
                let name = t("sd_s_an");
                settings::setting_row(ui, scheme, &name, &t("sd_s_an_h"), false, |row| {
                    let mut on = state.on;
                    let switch = row.add_enabled(!copy, widgets::Toggle::new(&mut on, scheme));
                    let enabled = switch.enabled();
                    switch.widget_info(|| WidgetInfo::selected(Role::CheckBox, enabled, on, &name));
                    if switch.changed() {
                        switched = Some(on);
                    }
                });
            }
            (0, 1) => {
                let name = t("sd_s_seite_auto");
                settings::setting_row(ui, scheme, &name, &t("sd_s_seite_auto_h"), false, |row| {
                    let mut on = state.auto_page;
                    let switch = row.add_enabled(!copy, widgets::Toggle::new(&mut on, scheme));
                    let enabled = switch.enabled();
                    switch.widget_info(|| WidgetInfo::selected(Role::CheckBox, enabled, on, &name));
                    if switch.changed() {
                        auto_switched = Some(on);
                    }
                });
            }
            (0, 2) => {
                settings::setting_row(ui, scheme, &t("sd_s_status"), &state.status_text(), false, |_| {});
            }
            (0, 3) => {
                settings::setting_row(ui, scheme, &t("sd_s_einrichten"), &t("sd_s_einrichten_h"), false, |row| {
                    let word = t("sd_s_einrichten_knopf");
                    let reason = t("sd_s_kopie");
                    let button = widgets::Button::new(&word, ButtonKind::Strong, scheme)
                        .icon("plug-zap")
                        .blocked(copy.then_some(reason.as_str()));
                    set_up = row.add(button).clicked() && !copy;
                });
            }
            (0, 4) => {
                let name = t("sd_s_regler_kompakt");
                settings::setting_row(ui, scheme, &name, &t("sd_s_regler_kompakt_h"), false, |row| {
                    let mut on = state.compact;
                    let switch = row.add_enabled(!copy, widgets::Toggle::new(&mut on, scheme));
                    let enabled = switch.enabled();
                    switch.widget_info(|| WidgetInfo::selected(Role::CheckBox, enabled, on, &name));
                    if switch.changed() {
                        compact_switched = Some(on);
                    }
                });
            }
            (0, _) => {
                let text = hint.as_ref().map_or_else(
                    || t("sd_s_elgato_h"),
                    |device| tf("sd_geraet_kein_profil", &[("0", Arg::from(device.as_str()))]),
                );
                settings::setting_row(ui, scheme, &t("sd_s_programm"), &text, false, |row| {
                    let word = t("sd_s_elgato");
                    let button = widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("external-link");
                    elgato = row.add(button).clicked();
                });
            }
            (_, 0) => {
                settings::setting_row(ui, scheme, &t("sd_f_pruefen"), &count_text, false, |row| {
                    let word = t("sd_f_pruefen_knopf");
                    let blocked = check_blocked.as_deref();
                    let button = widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("keyboard").blocked(blocked);
                    check = row.add(button).clicked() && blocked.is_none();
                });
            }
            (_, r) => {
                let r = r - 1;
                if let Some((items, names)) = &plan {
                    if r == 0 && deleted_rows > 0 {
                        let head = tf("sd_f_geloescht_kopf", &[("0", Arg::from(deleted.len()))]);
                        let which = if state.keep_deleted { "sd_f_geloescht_zeile" } else { "sd_f_geloescht_mit" };
                        let line = tf(which, &[("0", Arg::from(deleted.join(", ").as_str()))]);
                        widgets::status_box(ui, scheme, scheme.yellow, "triangle-alert", &head, &line);
                        return;
                    }
                    if r == 1 && deleted_rows > 1 {
                        let name = t("sd_f_auch_geloeschte");
                        settings::setting_row(ui, scheme, &name, &t("sd_f_auch_geloeschte_h"), false, |row| {
                            let mut on = !state.keep_deleted;
                            let switch = row.add(widgets::Toggle::new(&mut on, scheme));
                            let enabled = switch.enabled();
                            switch.widget_info(|| WidgetInfo::selected(Role::CheckBox, enabled, on, &name));
                            if switch.changed() {
                                deleted_switched = Some(on);
                            }
                        });
                        return;
                    }
                    let r = r - deleted_rows;
                    match rows_of_plan.get(r) {
                        Some(PlanRow::Item(i)) => {
                            ui.label(plan_line(&items[*i], &names[*i]));
                            return;
                        }
                        Some(PlanRow::Done(count)) => {
                            let text = if *count == 1 {
                                t("sd_f_haben_taste_1")
                            } else {
                                tf("sd_f_haben_taste", &[("0", Arg::from(*count))])
                            };
                            ui.horizontal(|row| {
                                row.label(text);
                                let (word, icon) = if state.show_done {
                                    (t("sd_f_ausblenden"), "chevron-up")
                                } else {
                                    (t("sd_f_anzeigen"), "chevron-down")
                                };
                                fold_switched = row.add(widgets::Button::new(&word, ButtonKind::Normal, scheme).icon(icon)).clicked();
                            });
                            return;
                        }
                        None => {}
                    }
                    if r == rows_of_plan.len() {
                        let adds = items
                            .iter()
                            .any(|i| {
                                matches!(i.outcome, Outcome::Add(_) | Outcome::Move { .. } | Outcome::Relocate { .. } | Outcome::Refill(_))
                            });
                        let none = t("sd_f_nichts_frei");
                        let reason = if adds { write_blocked.as_deref() } else { Some(none.as_str()) };
                        ui.horizontal(|row| {
                            let (yes, no) = (t("sd_f_eintragen"), t("sd_f_abbrechen"));
                            let go = widgets::Button::new(&yes, ButtonKind::Strong, scheme).icon("check").blocked(reason);
                            write = row.add(go).clicked() && reason.is_none();
                            cancel = row.add(widgets::Button::new(&no, ButtonKind::Normal, scheme)).clicked();
                        });
                        return;
                    }
                }
                let r = r - plan_rows;
                if let Some(record) = &state.record {
                    if r < record.entries.len() {
                        let (action, _, key) = &record.entries[r];
                        let shown = versekit_core::controls::input_readable(key, Some(versekit_core::joysticks::Kind::Keyboard));
                        ui.label(format!("{}: {shown}", versekit_core::controls::technical_readable(action)));
                    } else {
                        ui.horizontal(|row| {
                            let (back_word, all_word) = (t("sd_f_rueckgaengig"), t("sd_f_ganz"));
                            let reason = write_blocked.as_deref();
                            let back = widgets::Button::new(&back_word, ButtonKind::Normal, scheme).icon("undo-2").blocked(reason);
                            undo = row.add(back).clicked() && reason.is_none();
                            let all = widgets::Button::new(&all_word, ButtonKind::Danger, scheme).blocked(reason);
                            whole = row.add(all).on_hover_text(t("sd_f_ganz_h")).clicked() && reason.is_none();
                        });
                    }
                }
            }
        }
    });
    if let Some(on) = switched {
        state.set_on(on);
    }
    if let Some(on) = compact_switched {
        state.set_compact(on);
    }
    if fold_switched {
        state.show_done = !state.show_done;
    }
    // The plan and the count are worked out again with the new choice.
    if let Some(on) = deleted_switched {
        state.keep_deleted = !on;
        state.counted_at = None;
        state.start_plan(ui);
    }
    if let Some(on) = auto_switched {
        state.set_auto_page(on);
    }
    if set_up {
        state.set_up();
    }
    if elgato {
        versekit_core::browser::open_in_browser(streamdeck::ELGATO_URL);
    }
    if check {
        state.message = None;
        state.start_plan(ui);
    }
    if write && let Some((items, _)) = &plan {
        state.write_plan(items);
    }
    if cancel {
        state.close_preview();
    }
    if undo {
        state.undo(false);
    }
    if whole {
        state.undo(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_needs_something_comes_first_and_the_keys_already_bound_fold_under_one_line() {
        let item = |action: &str, outcome: Outcome| Item { action: action.into(), group: "g".into(), outcome };
        let items = [
            item("a", Outcome::HasKey),
            item("b", Outcome::Add("ralt+b".into())),
            item("c", Outcome::HasKey),
            item("d", Outcome::NotSendable("mouse4".into())),
            item("e", Outcome::Deleted),
            item("f", Outcome::NoFreeKey),
        ];
        let folded = plan_rows(&items, false);
        assert_eq!(folded, [PlanRow::Item(1), PlanRow::Item(3), PlanRow::Item(4), PlanRow::Item(5), PlanRow::Done(2)]);
        let open = plan_rows(&items, true);
        assert_eq!(&open[5..], [PlanRow::Item(0), PlanRow::Item(2)], "unfolded: below the line");
        let all_bound = [item("a", Outcome::HasKey)];
        assert_eq!(plan_rows(&all_bound, false), [PlanRow::Done(1)]);
        assert_eq!(plan_rows(&items[1..2], false), [PlanRow::Item(0)], "nothing bound already: no line");
        assert_eq!(t_in_de("sd_f_haben_taste_1"), "1 Aktion hat schon eine Taste.");
    }

    fn t_in_de(key: &str) -> String {
        versekit_core::language::t_in(versekit_core::language::Lang::De, key)
    }

    #[test]
    fn the_bar_says_one_key_in_the_singular_and_more_in_the_plural() {
        use versekit_core::language::Lang;
        assert_eq!(missing_text(Lang::De, 1, false), "1 Stream-Deck-Taste ist im Spiel nicht belegt.");
        assert_eq!(missing_text(Lang::De, 3, false), "3 Stream-Deck-Tasten sind im Spiel nicht belegt.");
        assert_eq!(missing_text(Lang::En, 1, false), "1 Stream Deck key has no binding in the game.");
        assert_eq!(missing_text(Lang::En, 2, false), "2 Stream Deck keys have no binding in the game.");
        assert!(missing_text(Lang::De, 1, true).ends_with(&versekit_core::language::t_in(Lang::De, "sd_leiste_spiel")));
    }

    #[test]
    fn a_plan_line_names_the_action_and_what_happens() {
        let add = Item { action: "v_eject".into(), group: "g".into(), outcome: Outcome::Add("ralt+b".into()) };
        assert!(plan_line(&add, "Aussteigen").starts_with("Aussteigen: "));
        let kept = Item { action: "v_x".into(), group: "g".into(), outcome: Outcome::Deleted };
        assert_eq!(plan_line(&kept, "X"), format!("X: {}", t("sd_f_geloescht")));
        let filled = Item { action: "v_x".into(), group: "g".into(), outcome: Outcome::Refill("lalt+lshift+b".into()) };
        let key = versekit_core::controls::input_readable("lalt+lshift+b", Some(versekit_core::joysticks::Kind::Keyboard));
        assert_eq!(plan_line(&filled, "X"), format!("X: {}", tf("sd_f_wieder", &[("0", Arg::from(key))])));
        // Deleted lines get a key unless the player switches it off.
        assert!(!StreamDeckState::default().keep_deleted);
        let mouse = Item { action: "v_m".into(), group: "g".into(), outcome: Outcome::NotSendable("mouse4".into()) };
        let line = plan_line(&mouse, "Master-Modus");
        let button = versekit_core::controls::input_readable("mouse4", Some(versekit_core::joysticks::Kind::Keyboard));
        assert!(line.starts_with("Master-Modus: ") && line.contains(&button), "{line}");
        // Before the click the page says what moves: the mouse button stays, a key comes.
        let moved = Item {
            action: "v_m".into(),
            group: "g".into(),
            outcome: Outcome::Move { mouse: "mouse4".into(), key: "ralt+b".into() },
        };
        let line = plan_line(&moved, "Master-Modus");
        let key = versekit_core::controls::input_readable("ralt+b", Some(versekit_core::joysticks::Kind::Keyboard));
        assert_eq!(line, format!("Master-Modus: {}", tf("sd_f_verschieben", &[("0", Arg::from(button)), ("1", Arg::from(key))])));
        // A key with Alt Gr moves, and the line says why, with what the key alone does.
        let moved = Item {
            action: "zoom_in".into(),
            group: "player".into(),
            outcome: Outcome::Relocate {
                from: "ralt+p".into(),
                key: "lalt+lshift+b".into(),
                alone: Some("Energie Waffen".into()),
                factory: false,
            },
        };
        let line = plan_line(&moved, "Zoom");
        let kind = Some(versekit_core::joysticks::Kind::Keyboard);
        let readable = |i: &str| versekit_core::controls::input_readable(i, kind);
        assert!(line.starts_with("Zoom: ") && line.contains(&readable("ralt+p")) && line.contains(&readable("lalt+lshift+b")), "{line}");
        assert!(line.ends_with("Energie Waffen"), "{line}");
        let without = Item {
            outcome: Outcome::Relocate { from: "ralt+q".into(), key: "lalt+lshift+c".into(), alone: None, factory: true },
            ..moved
        };
        assert!(!plan_line(&without, "Quantum").contains(": :"), "no empty name at the end");
    }

    #[test]
    fn an_older_plugin_is_said_in_the_state_with_both_versions() {
        let report = Report {
            reachable: true,
            connected: true,
            plugin_version: Some("5.7.0.100".into()),
            shipped_version: Some("5.7.1.200".into()),
            ..Report::default()
        };
        let state = StreamDeckState { on: true, report: Arc::new(Mutex::new(report.clone())), ..StreamDeckState::default() };
        let text = state.status_text();
        let old = tf("sd_s_veraltet", &[("0", Arg::from("5.7.0.100")), ("1", Arg::from("5.7.1.200"))]);
        assert!(text.contains(&old), "{text}");
        // The same version, or no plugin file beside VerseKit: nothing said.
        for (installed, shipped) in [(Some("5.7.1.200"), Some("5.7.1.200")), (Some("5.7.0.100"), None)] {
            let same = Report { plugin_version: installed.map(String::from), shipped_version: shipped.map(String::from), ..report.clone() };
            let state = StreamDeckState { on: true, report: Arc::new(Mutex::new(same)), ..StreamDeckState::default() };
            assert!(!state.status_text().contains(&old), "{installed:?} {shipped:?}");
        }
    }

    #[test]
    fn a_game_ended_lately_says_when_the_keys_can_go_in_instead_of_asking_to_close_it() {
        use versekit_core::game_start::NotQuiet;
        let ended = |age: f64| NotQuiet::LogFresh { age, listed: true };
        assert_eq!(not_quiet_text(None), t("sd_f_spiel_laeuft"));
        assert_eq!(not_quiet_text(Some(&ended(70.0))), tf("sd_f_spiel_beendet", &[("0", Arg::from(4usize))]));
        assert_eq!(not_quiet_text(Some(&ended(250.0))), t("sd_f_spiel_beendet_1"));
        // Each reason says itself: the process, a fresh log without the process list, nothing to go by.
        let texts = [
            not_quiet_text(Some(&NotQuiet::Process)),
            not_quiet_text(Some(&NotQuiet::LogFresh { age: 70.0, listed: false })),
            not_quiet_text(Some(&NotQuiet::Unknown)),
            not_quiet_text(Some(&ended(70.0))),
        ];
        for (i, a) in texts.iter().enumerate() {
            assert!(!texts[i + 1..].contains(a) && *a != t("sd_f_spiel_laeuft"), "{a}");
        }
        assert!(texts[1].contains("70") && texts[1].contains('4'), "{}", texts[1]);
        // The reason the count found is what the button says.
        let state = counted_state(&[Outcome::Add("lalt+lshift+b".into())], false, "s");
        lock(&state.report).in_use = vec!["v_0".into()];
        if let Some(counted) = lock(&state.counted).as_mut() {
            counted.quiet = Err(texts[0].clone());
        }
        assert_eq!(state.write_blocked(), Some(texts[0].clone()));
        assert_eq!(state.count_text().1, Some(texts[0].clone()));
    }

    /// A state with the interface on and a counted plan of these outcomes, the game quiet or running.
    fn counted_state(outcomes: &[Outcome], quiet: bool, stamp: &str) -> StreamDeckState {
        let items: Vec<Item> = outcomes
            .iter()
            .enumerate()
            .map(|(n, outcome)| Item { action: format!("v_{n}"), group: "g".into(), outcome: outcome.clone() })
            .collect();
        let names = items.iter().map(|i| i.action.clone()).collect();
        let counted = Counted {
            key: (vec!["v_0".into()], false, stamp.into()),
            items: Ok((items, names)),
            quiet: if quiet { Ok(()) } else { Err("läuft".into()) },
        };
        StreamDeckState { on: true, counted: Arc::new(Mutex::new(Some(counted))), ..StreamDeckState::default() }
    }

    #[test]
    fn missing_keys_are_counted_for_the_bar_with_the_game_running_or_not() {
        let missing = [Outcome::Add("ralt+a".into()), Outcome::Deleted, Outcome::HasKey, Outcome::NoFreeKey];
        assert_eq!(counted_state(&missing, true, "1").missing(), Some((3, false)), "deleted and without a free key count");
        assert_eq!(counted_state(&missing, false, "1").missing(), Some((3, true)), "the bar says to close the game");
        assert_eq!(counted_state(&[Outcome::HasKey], true, "1").missing(), None, "nothing missing: no bar");
        let off = StreamDeckState { on: false, ..counted_state(&missing, true, "1") };
        assert_eq!(off.missing(), None, "the interface off: no bar");
    }

    #[test]
    fn the_preview_opens_by_itself_while_keys_are_missing_and_stays_closed_once_the_player_closed_it() {
        let mut state = counted_state(&[Outcome::Add("ralt+a".into())], true, "1");
        state.open_preview_by_itself();
        assert!(matches!(lock(&state.planned).as_ref(), Some(Ok((items, _))) if items.len() == 1), "opened at once");
        state.close_preview();
        state.open_preview_by_itself();
        assert!(lock(&state.planned).is_none(), "closed for this state of the keys");
        // The bindings changed (another stamp): it opens again.
        *lock(&state.counted) = lock(&counted_state(&[Outcome::Add("ralt+b".into())], true, "2").counted).clone();
        state.open_preview_by_itself();
        assert!(lock(&state.planned).is_some(), "a new state, the preview again");
        // Nothing missing: nothing opens.
        let mut full = counted_state(&[Outcome::HasKey], true, "1");
        full.open_preview_by_itself();
        assert!(lock(&full.planned).is_none());
    }

    #[test]
    fn without_data_the_page_says_the_interface_is_off() {
        let state = StreamDeckState::default();
        assert_eq!(state.status_text(), t("sd_s_aus"));
        let copy = StreamDeckState { copy: true, ..StreamDeckState::default() };
        assert_eq!(copy.status_text(), t("sd_s_kopie"));
    }
}
