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

//! The page "Erkennung" (`_detection` and `_account_rows` of 4.12): how Verse-Kit notices a blueprint. The
//! phrase found, the catalog to fetch again, the switch for blueprints with no known source. An entry of its
//! own in the settings group of the menu. The own account, the blueprints of another account that did not
//! count and those that ended up in the collection stand at the top of the page "Allgemein" ([`show_account`]),
//! drawn from the same state.
//!
//! The logs are read in a thread of their own (accounts and foreign blueprints together: 180 logs take
//! seconds), the catalog is built in one too. What the logic needs lives in `versekit_core::detection`.
//!
//! What 4.12 did badly is done better here: the catalog button says whether it worked (4.12 always said
//! "refreshed"), a failed search is not shown as "clean", and the removal of foreign blueprints is asked in
//! the row (no window) and tells when it could not write.

use std::sync::mpsc::{self, Receiver, TryRecvError};

use egui::{RichText, Ui};
use versekit_core::catalog_cache::CatalogCache;
use versekit_core::detection::{self, Accounts};
use versekit_core::language::{Arg, t, tf};
use versekit_core::logsource::ALL_ACCOUNTS;
use versekit_core::paths::Paths;
use versekit_core::stock_journal;
use versekit_core::theme::Scheme;
use versekit_gamedata::list;

use crate::color;
use crate::settings::{self, Said};
use crate::widgets::{self, ButtonKind};

/// How the catalog is built on the button (the check puts a stand-in in its place: no network).
pub type BuildCatalog = fn(&Paths) -> Option<CatalogCache>;

/// So many blueprints are named in the line, so many in the question.
const NAMED_IN_LINE: usize = 8;
const NAMED_IN_QUESTION: usize = 25;

/// The foreign blueprints: where the search and the question are.
enum Foreign {
    /// The logs are being read.
    Searching(Receiver<(Accounts, Vec<String>)>),
    /// None found.
    Clean,
    /// The search did not work.
    Failed,
    /// Found: the player may take them out.
    Found(Vec<String>),
    /// Asked: the player is to confirm.
    Asking(Vec<String>),
    /// Being taken out.
    Removing(Receiver<Option<usize>>),
}

/// What the page keeps.
pub struct DetectionState {
    loaded: bool,
    phrases: Vec<String>,
    all: bool,
    /// The switch "Lager aus dem Log lesen".
    stock_log: bool,
    accounts: Option<Accounts>,
    foreign: Foreign,
    catalog: Option<Receiver<Option<usize>>>,
    build: BuildCatalog,
    notice: Option<Said>,
    all_changed: bool,
}

impl Default for DetectionState {
    fn default() -> DetectionState {
        DetectionState::with_build(build_real)
    }
}

/// The catalog built now, asking the network as the program does (not in a check).
fn build_real(paths: &Paths) -> Option<CatalogCache> {
    list::rebuild_now(paths).run()
}

impl DetectionState {
    pub fn with_build(build: BuildCatalog) -> DetectionState {
        DetectionState {
            loaded: false,
            phrases: Vec::new(),
            all: false,
            stock_log: true,
            accounts: None,
            foreign: Foreign::Clean,
            catalog: None,
            build,
            notice: None,
            all_changed: false,
        }
    }

    /// The settings were read again: the page reads its values again at the next frame.
    pub fn invalidate(&mut self) {
        self.loaded = false;
    }

    /// The line for the foot, once.
    pub fn take_notice(&mut self) -> Option<Said> {
        self.notice.take()
    }

    /// Whether the switch for blueprints with no known source was flipped since the last ask, once: the lists
    /// follow it.
    pub fn take_all_changed(&mut self) -> bool {
        std::mem::take(&mut self.all_changed)
    }

    fn load(&mut self, paths: &Paths) {
        self.phrases = detection::phrases_found(paths);
        self.all = detection::all_blueprints(paths);
        self.stock_log = stock_journal::enabled(paths);
        self.loaded = true;
        self.search(paths);
    }

    /// Reads the logs: the accounts and the foreign blueprints.
    fn search(&mut self, paths: &Paths) {
        let (sender, receiver) = mpsc::channel();
        let copy = paths.working_copy();
        std::thread::spawn(move || {
            let accounts = detection::accounts(&copy);
            let _ = sender.send((accounts, detection::foreign(&copy)));
        });
        self.foreign = Foreign::Searching(receiver);
    }
}

/// The first `most` names in a line, then an ellipsis.
fn preview(names: &[String], most: usize) -> String {
    let mut line = names.iter().take(most).map(String::as_str).collect::<Vec<_>>().join(", ");
    if names.len() > most {
        line.push_str(" …");
    }
    line
}

/// The names as lines of the question, then an ellipsis.
fn listing(names: &[String], most: usize) -> String {
    let mut lines: Vec<String> = names.iter().take(most).map(|name| format!("· {name}")).collect();
    if names.len() > most {
        lines.push("…".to_string());
    }
    lines.join("\n")
}

/// The words of an account in the list.
pub(crate) fn account_word(name: &str, count: usize) -> String {
    tf("s_er_acc_n", &[("0", Arg::from(name)), ("1", Arg::from(count as i64))])
}

/// A block of the page: the name, below it the hint in small muted text, then what the block holds.
fn block<R>(
    ui: &mut Ui,
    scheme: &Scheme,
    name: &str,
    hint: &str,
    add: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<R> {
    let drawn = widgets::card(ui, scheme, |ui| {
        widgets::card_title_label(ui, scheme, name);
        if !hint.is_empty() {
            ui.label(RichText::new(hint).small().color(color(scheme.sub)));
        }
        ui.add_space(6.0);
        add(ui)
    });
    // A jump of the menu filter to the block frames it.
    crate::highlight::mark(ui, scheme, drawn.response.rect, &[name, hint]);
    drawn
}

/// Draws the page and moves the jobs on.
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut DetectionState, paths: &Paths) {
    if !state.loaded {
        state.load(paths);
    }
    poll(ui, state, paths);
    ui.label(RichText::new(t("s_er_lead")).small().color(color(scheme.sub)));
    ui.add_space(8.0);

    // The phrase the game writes.
    block(ui, scheme, &t("s_er_satz"), &t("s_er_satz_h"), |ui| {
        let text = if state.phrases.is_empty() { t("s_er_satz_leer") } else { state.phrases.join(" · ") };
        ui.label(text);
    });
    ui.add_space(8.0);

    // The catalog.
    widgets::card(ui, scheme, |ui| {
        widgets::card_title(ui, scheme, &t("s_er_kat_kopf"));
        ui.add_space(4.0);
        let busy = state.catalog.is_some();
        let reason = busy.then(|| t("s_er_kat_laeuft"));
        let mut clicked = false;
        let word = t("s_er_kat_jetzt");
        settings::setting_row(ui, scheme, &t("s_er_kat"), &t("s_er_kat_h"), false, |row| {
            let button = widgets::Button::new(&word, ButtonKind::Normal, scheme).blocked(reason.as_deref());
            clicked = row.add(button).clicked();
        });
        if busy {
            widgets::loading(ui, scheme, &t("s_er_kat_holt"));
        }
        if clicked && !busy {
            let (sender, receiver) = mpsc::channel();
            let (copy, build) = (paths.working_copy(), state.build);
            let all = detection::all_blueprints(paths);
            std::thread::spawn(move || {
                let counted =
                    build(&copy).map(|catalog| catalog.blueprints.iter().filter(|b| all || !b.no_way).count());
                let _ = sender.send(counted);
            });
            state.catalog = Some(receiver);
        }
    });
    ui.add_space(8.0);

    // Blueprints with no known source and the stock from the log, in one card.
    widgets::card(ui, scheme, |ui| {
        widgets::card_title(ui, scheme, &t("s_er_listen_kopf"));
        ui.add_space(4.0);
        let name = t("s_er_alle");
        settings::setting_row(ui, scheme, &name, &t("s_er_alle_h"), false, |row| {
            let switch = row.add(widgets::Toggle::new(&mut state.all, scheme));
            let (enabled, on) = (switch.enabled(), state.all);
            switch.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, enabled, on, &name));
            if switch.changed() {
                state.notice = Some(if detection::set_all_blueprints(paths, state.all) {
                    state.all_changed = true;
                    Said::note(t(if state.all { "s_er_alle_hin" } else { "s_er_alle_weg" }))
                } else {
                    state.all = !state.all;
                    Said::fault(t("s_hl_fehler"))
                });
            }
        });

        // The stock from the log.
        let name = t("s_lj_schalter");
        settings::setting_row(ui, scheme, &name, &t("s_lj_schalter_h"), false, |row| {
            let switch = row.add(widgets::Toggle::new(&mut state.stock_log, scheme));
            let (enabled, on) = (switch.enabled(), state.stock_log);
            switch.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, enabled, on, &name));
            if switch.changed() {
                state.notice = Some(if stock_journal::set_enabled(paths, state.stock_log) {
                    Said::note(t(if state.stock_log { "s_lj_schalter_an" } else { "s_lj_schalter_aus" }))
                } else {
                    state.stock_log = !state.stock_log;
                    Said::fault(t("s_hl_fehler"))
                });
            }
        });
    });
}

/// The own account and the blueprints of other accounts, at the top of the page "Allgemein": the choice, the
/// blueprints of another account that did not count, and those that ended up in the collection. Moves the jobs on.
pub fn show_account(ui: &mut Ui, scheme: &Scheme, state: &mut DetectionState, paths: &Paths) {
    if !state.loaded {
        state.load(paths);
    }
    poll(ui, state, paths);
    let mut chosen: Option<String> = None;
    widgets::card(ui, scheme, |ui| {
        widgets::card_title(ui, scheme, &t("s_er_acc_kopf"));
        ui.add_space(4.0);
        let name = t("s_er_acc");
        settings::setting_row(ui, scheme, &name, &t("s_er_acc_h"), false, |row| match &state.accounts {
            Some(accounts) if !accounts.list.is_empty() || accounts.own.is_some() => {
                let own = accounts.own.clone().unwrap_or_else(|| ALL_ACCOUNTS.to_string());
                let mut entries = vec![(ALL_ACCOUNTS.to_string(), t("s_er_acc_alle"))];
                entries.extend(accounts.list.iter().map(|(who, count)| (who.clone(), account_word(who, *count))));
                let shown =
                    entries.iter().find(|(value, _)| *value == own).map(|(_, word)| word.clone()).unwrap_or_default();
                settings::choice_box(row, "erkennung-konto", &name, &shown, |list| {
                    for (value, word) in &entries {
                        if list.selectable_label(*value == own, word).clicked() && *value != own {
                            chosen = Some(value.clone());
                        }
                    }
                });
            }
            Some(_) => {
                row.label(RichText::new(t("s_er_acc_keiner")).small().color(color(scheme.sub)));
            }
            None => {
                widgets::loading(row, scheme, &t("s_er_acc_laedt"));
            }
        });
        // Blueprints of another account that did not count: how many, and the way to count them.
        if let Some(accounts) = &state.accounts
            && accounts.foreign.blueprints > 0
            && accounts.own.as_deref() != Some(ALL_ACCOUNTS)
        {
            let tally = accounts.foreign;
            let line = tf(
                if tally.blueprints == 1 { "s_er_acc_fremd_1" } else { "s_er_acc_fremd_n" },
                &[("0", Arg::from(tally.blueprints as i64))],
            );
            let last = tally.last.map_or(Arg::from("—"), |at| Arg::moment(at as f64));
            let hint = tf(
                if tally.sessions == 1 { "s_er_acc_fremd_h1" } else { "s_er_acc_fremd_h" },
                &[("0", Arg::from(tally.sessions as i64)), ("1", last)],
            );
            let word = t("s_er_acc_fremd_knopf");
            settings::setting_row(ui, scheme, &line, &hint, false, |row| {
                if row.add(widgets::Button::new(&word, ButtonKind::Normal, scheme)).clicked() {
                    chosen = Some(ALL_ACCOUNTS.to_string());
                }
            });
        }
    });
    if let Some(value) = chosen
        && detection::set_account(paths, &value)
    {
        let word = if value == ALL_ACCOUNTS { t("s_er_acc_alle") } else { value };
        state.notice = Some(Said::note(tf("s_er_acc_sagen", &[("0", Arg::from(word.as_str()))])));
        state.accounts = None;
        state.search(paths);
    }
    ui.add_space(8.0);

    // Blueprints of other accounts in the collection.
    foreign_block(ui, scheme, state, paths);
}

/// Moves the jobs on that finished.
fn poll(ui: &mut Ui, state: &mut DetectionState, paths: &Paths) {
    if let Some(receiver) = &state.catalog {
        match receiver.try_recv() {
            Ok(counted) => {
                state.catalog = None;
                state.notice = Some(match counted {
                    Some(count) => Said::note(tf("s_er_kat_da", &[("0", Arg::from(count as i64))])),
                    None => Said::fault(t("s_er_kat_weg")),
                });
            }
            Err(TryRecvError::Empty) => ui.ctx().request_repaint_after(std::time::Duration::from_millis(200)),
            Err(TryRecvError::Disconnected) => {
                state.catalog = None;
                state.notice = Some(Said::fault(t("s_er_kat_weg")));
            }
        }
    }
    match &state.foreign {
        Foreign::Searching(receiver) => match receiver.try_recv() {
            Ok((accounts, names)) => {
                state.accounts = Some(accounts);
                state.foreign = if names.is_empty() { Foreign::Clean } else { Foreign::Found(names) };
            }
            Err(TryRecvError::Empty) => ui.ctx().request_repaint_after(std::time::Duration::from_millis(200)),
            Err(TryRecvError::Disconnected) => {
                state.accounts = Some(Accounts::default());
                state.foreign = Foreign::Failed;
            }
        },
        Foreign::Removing(receiver) => match receiver.try_recv() {
            Ok(removed) => {
                state.notice = Some(match removed {
                    Some(count) => Said::note(tf("s_er_fremd_ok", &[("0", Arg::from(count as i64))])),
                    None => Said::fault(t("s_hl_fehler")),
                });
                // The lists and the progress follow (the collection changed), and the search runs again.
                state.all_changed = true;
                state.search(paths);
            }
            Err(TryRecvError::Empty) => ui.ctx().request_repaint_after(std::time::Duration::from_millis(200)),
            Err(TryRecvError::Disconnected) => {
                state.notice = Some(Said::fault(t("s_hl_fehler")));
                state.foreign = Foreign::Failed;
            }
        },
        _ => {}
    }
}

/// The block "Baupläne anderer Accounts".
fn foreign_block(ui: &mut Ui, scheme: &Scheme, state: &mut DetectionState, paths: &Paths) {
    let (mut ask, mut cancel, mut confirm) = (false, false, false);
    block(ui, scheme, &t("s_er_fremd"), &t("s_er_fremd_h"), |ui| match &state.foreign {
        Foreign::Searching(_) | Foreign::Removing(_) => {
            widgets::loading(ui, scheme, &t("s_er_fremd_sucht"));
        }
        Foreign::Clean => {
            ui.label(t("s_er_fremd_keine"));
        }
        Foreign::Failed => {
            ui.label(RichText::new(t("s_er_fremd_fehler")).color(color(scheme.yellow)));
        }
        Foreign::Found(names) => {
            let line = tf(
                "s_er_fremd_n",
                &[("0", Arg::from(names.len() as i64)), ("1", Arg::from(preview(names, NAMED_IN_LINE).as_str()))],
            );
            ui.label(line);
            ui.add_space(6.0);
            ask = ui.add(widgets::Button::new(&t("s_er_fremd_weg"), ButtonKind::Danger, scheme)).clicked();
        }
        // The question stands in the row, no window of its own.
        Foreign::Asking(names) => {
            let question = tf(
                "s_er_fremd_frage",
                &[("0", Arg::from(names.len() as i64)), ("1", Arg::from(listing(names, NAMED_IN_QUESTION).as_str()))],
            );
            ui.label(question);
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                cancel = ui.add(widgets::Button::new(&t("abbrechen"), ButtonKind::Normal, scheme)).clicked();
                confirm = ui.add(widgets::Button::new(&t("s_mlg_bestaetigen"), ButtonKind::Danger, scheme)).clicked();
            });
        }
    });
    if ask && let Foreign::Found(names) = std::mem::replace(&mut state.foreign, Foreign::Clean) {
        state.foreign = Foreign::Asking(names);
    }
    if cancel && let Foreign::Asking(names) = std::mem::replace(&mut state.foreign, Foreign::Clean) {
        state.foreign = Foreign::Found(names);
    }
    if confirm && let Foreign::Asking(names) = std::mem::replace(&mut state.foreign, Foreign::Clean) {
        let (sender, receiver) = mpsc::channel();
        let copy = paths.working_copy();
        std::thread::spawn(move || {
            let _ = sender.send(detection::remove_foreign(&copy, &names));
        });
        state.foreign = Foreign::Removing(receiver);
    }
}

#[cfg(test)]
mod tests {
    use egui::{Event, Pos2, Rect};
    use versekit_core::catalog_cache::Blueprint;
    use versekit_core::collection::{self, Collection, NoCatalog};
    use versekit_core::export::{self, FileExports};
    use versekit_core::jsonio::Value;
    use versekit_core::language::{self, Choice};
    use versekit_core::logsource::ACCOUNT_SETTING;
    use versekit_core::overlay::keys;
    use versekit_core::paths::{ENV_INSTALL_DIR, Host};
    use versekit_core::phrases::{BackRoute, NoArchive};
    use versekit_core::pypath::Flavor;
    use versekit_core::theme;

    use super::*;

    fn scratch(name: &str) -> String {
        let dir =
            std::env::temp_dir().join(format!("versekit-detectpage-{}-{name}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        dir.to_string_lossy().replace('\\', "/")
    }

    /// A home with a game folder whose logs hold two accounts: Alice (two logs) and Bob (one).
    fn world(name: &str) -> (String, Paths) {
        let home = scratch(name);
        let game = format!("{home}/StarCitizen/LIVE");
        let log = |account: &str, find: &str| {
            format!(
                "<Legacy login response> [CIG-net] User Login Success - Handle[{account}] - Time\n\
                 <SHUDEvent_OnNotification> Added notification \"Received Blueprint: {find}: \" [1]\n"
            )
        };
        std::fs::create_dir_all(format!("{game}/logbackups")).expect("folder");
        std::fs::write(format!("{game}/logbackups/1.log"), log("Alice", "Mine Rifle")).unwrap();
        std::fs::write(format!("{game}/logbackups/2.log"), log("Alice", "Mine Helmet")).unwrap();
        std::fs::write(format!("{game}/logbackups/3.log"), log("Bob", "Other Pistol")).unwrap();
        std::fs::write(format!("{game}/Game.log"), "x\n").unwrap();
        let paths = Paths::new(Host::new(
            Flavor::Posix,
            &[("SC_BP_HOME", home.as_str()), ("HOME", home.as_str()), (ENV_INSTALL_DIR, game.as_str())],
            &home,
        ));
        (home, paths)
    }

    fn with_collection<R>(paths: &Paths, work: impl FnOnce(&Collection) -> R) -> R {
        let names = BackRoute { paths, archive: &NoArchive };
        let exports = FileExports { paths, version: "test", clock: export::now_unix };
        let collection =
            Collection { paths, catalog: &NoCatalog, names: &names, exports: &exports, clock: collection::now_text };
        work(&collection)
    }

    fn built(_: &Paths) -> Option<CatalogCache> {
        let blueprint = |name: &str, no_way: bool| Blueprint {
            key: name.to_lowercase(),
            name: name.into(),
            no_way,
            ..Blueprint::default()
        };
        Some(CatalogCache {
            blueprints: vec![blueprint("Aa One", false), blueprint("Bb Two", false), blueprint("Cc Three", true)],
            ..CatalogCache::default()
        })
    }

    fn not_built(_: &Paths) -> Option<CatalogCache> {
        None
    }

    type Texts = Vec<(String, Pos2)>;

    struct Bench {
        ctx: egui::Context,
        state: DetectionState,
        paths: Paths,
        home: String,
    }

    impl Bench {
        fn new(home: String, paths: Paths) -> Bench {
            let ctx = egui::Context::default();
            crate::install(&ctx);
            ctx.enable_accesskit();
            Bench { ctx, state: DetectionState::with_build(built), paths, home }
        }

        fn frame(&mut self, events: Vec<Event>) -> (Texts, Vec<(egui::Role, String, Rect)>) {
            let scheme = theme::by_id("original").expect("original");
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 1800.0))),
                events,
                ..Default::default()
            };
            let (state, paths) = (&mut self.state, &self.paths);
            // The page Erkennung and, below it, the part of the page Allgemein: one state for both.
            let mut output = self.ctx.run_ui(input, |ui| {
                show_page(ui, scheme, state, paths);
                show_account(ui, scheme, state, paths);
            });
            output.textures_delta.clear();
            let mut texts = Vec::new();
            fn collect(shape: &egui::Shape, into: &mut Texts) {
                match shape {
                    egui::Shape::Text(text) => into.push((text.galley.text().to_string(), text.pos)),
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, into)),
                    _ => {}
                }
            }
            output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
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
                                Pos2::new(b.x0 as f32, b.y0 as f32),
                                Pos2::new(b.x1 as f32, b.y1 as f32),
                            );
                            Some((node.role(), node.label().unwrap_or_default().to_string(), rect))
                        })
                        .collect()
                })
                .unwrap_or_default();
            (texts, nodes)
        }

        fn settle(&mut self) -> Texts {
            let mut last = Vec::new();
            for _ in 0..4 {
                last = self.frame(Vec::new()).0;
            }
            last
        }

        /// Frames until `done`, at most five seconds (the logs are read in a thread).
        fn until(&mut self, done: impl Fn(&Texts) -> bool) -> Texts {
            let mut texts = self.settle();
            for _ in 0..100 {
                if done(&texts) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
                texts = self.settle();
            }
            texts
        }

        fn press_at(&mut self, at: Pos2) {
            let button = |pressed| Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            self.frame(vec![Event::PointerMoved(at)]);
            self.frame(vec![button(true)]);
            self.frame(vec![button(false)]);
            self.settle();
        }

        fn click(&mut self, label: &str) {
            let texts = self.settle();
            let at = texts
                .iter()
                .find(|(text, _)| text == label)
                .map(|(_, at)| *at + egui::vec2(3.0, 3.0))
                .unwrap_or_else(|| panic!("{label} is not on the screen: {texts:?}"));
            self.press_at(at);
        }

        fn click_named(&mut self, role: egui::Role, name: &str) {
            self.settle();
            let nodes = self.frame(Vec::new()).1;
            let rect = nodes
                .iter()
                .find(|(r, n, _)| *r == role && n == name)
                .map(|(_, _, rect)| *rect)
                .unwrap_or_else(|| panic!("no {role:?} `{name}`: {nodes:?}"));
            self.press_at(rect.center());
        }
    }

    fn shows(texts: &Texts, part: &str) -> bool {
        texts.iter().any(|(text, _)| text.contains(part))
    }

    fn setup() -> std::sync::MutexGuard<'static, ()> {
        let guard = crate::language_lock();
        theme::set_current("original");
        language::set_language(Choice::De);
        guard
    }

    #[test]
    fn the_page_shows_the_phrases_and_the_switch_is_stored_told_and_passed_on() {
        let _language = setup();
        let home = scratch("schalter");
        let paths =
            Paths::new(Host::new(Flavor::Posix, &[("SC_BP_HOME", home.as_str()), ("HOME", home.as_str())], &home));
        let mut bench = Bench::new(home, paths);
        let texts = bench.until(|texts| shows(texts, &t("s_er_fremd_keine")));
        assert!(shows(&texts, "Bauplan erhalten") && shows(&texts, "Received Blueprint"), "{texts:?}");
        assert!(shows(&texts, &t("s_er_acc_keiner")), "no account yet says what to do: {texts:?}");
        bench.click_named(egui::Role::CheckBox, &t("s_er_alle"));
        assert!(detection::all_blueprints(&bench.paths), "stored at once");
        assert!(bench.state.take_all_changed(), "the lists are told");
        assert_eq!(bench.state.take_notice(), Some(Said::note(t("s_er_alle_hin"))));
        bench.click_named(egui::Role::CheckBox, &t("s_er_alle"));
        assert!(!detection::all_blueprints(&bench.paths));
        assert_eq!(bench.state.take_notice(), Some(Said::note(t("s_er_alle_weg"))));
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_switch_for_the_stock_from_the_log_is_on_says_what_is_kept_and_is_stored() {
        let _language = setup();
        let home = scratch("lagerlog");
        let paths =
            Paths::new(Host::new(Flavor::Posix, &[("SC_BP_HOME", home.as_str()), ("HOME", home.as_str())], &home));
        let mut bench = Bench::new(home, paths);
        let texts = bench.until(|texts| shows(texts, &t("s_er_fremd_keine")));
        assert!(shows(&texts, &t("s_lj_schalter")), "{texts:?}");
        // The hint, with what is kept, on hover behind the symbol `info`, never as a line under the name.
        let hint = t("s_lj_schalter_h");
        let first = hint.split_once(". ").map_or(hint.clone(), |(head, _)| format!("{head}."));
        assert!(!shows(&texts, &first), "the hint stands on hover: {texts:?}");
        assert!(
            hint.contains("Speichert nur Klasse, Menge, Preis, Zeit, Ort und Laden, nie Namen oder Spielerkennungen."),
            "the hint names what is kept: {hint}"
        );
        assert!(stock_journal::enabled(&bench.paths), "on by default");
        assert_eq!(bench.paths.setting("lager_aus_log"), None, "nothing stored yet");
        bench.click_named(egui::Role::CheckBox, &t("s_lj_schalter"));
        assert!(!stock_journal::enabled(&bench.paths), "stored at once");
        assert!(!bench.paths.setting_bool("lager_aus_log", true), "under the key `lager_aus_log`");
        assert_eq!(bench.state.take_notice(), Some(Said::note(t("s_lj_schalter_aus"))));
        bench.click_named(egui::Role::CheckBox, &t("s_lj_schalter"));
        assert!(stock_journal::enabled(&bench.paths));
        assert_eq!(bench.state.take_notice(), Some(Said::note(t("s_lj_schalter_an"))));
        // The page reads the stored value again after the settings were read anew.
        stock_journal::set_enabled(&bench.paths, false);
        bench.state.invalidate();
        bench.until(|texts| shows(texts, &t("s_er_fremd_keine")));
        assert!(!bench.state.stock_log, "the switch shows the stored value");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_accounts_are_listed_with_their_logs_and_the_choice_is_stored_and_asks_the_overlay() {
        let _language = setup();
        let (home, paths) = world("konten");
        let mut bench = Bench::new(home, paths);
        let alice = account_word("Alice", 2);
        let texts = bench.until(|texts| shows(texts, &alice));
        assert!(shows(&texts, &alice), "the own account (most logs) is chosen: {texts:?}");
        // Open the list: the others are in it.
        bench.click(&alice);
        let texts = bench.settle();
        assert!(shows(&texts, &account_word("Bob", 1)) && shows(&texts, &t("s_er_acc_alle")), "{texts:?}");
        bench.click(&account_word("Bob", 1));
        assert_eq!(bench.paths.setting(ACCOUNT_SETTING).as_deref(), Some("Bob"));
        assert_eq!(bench.paths.setting_int(keys::RESCAN_NR, 0, None, None), 1, "the overlay is asked to read again");
        let said = bench.state.take_notice().expect("told");
        assert_eq!(said, Said::note(tf("s_er_acc_sagen", &[("0", Arg::from("Bob"))])));
        // The logs are read again: the accounts come back (not "reading the logs" for good).
        let loading = t("s_er_acc_laedt");
        let bob = account_word("Bob", 1);
        let texts = bench.until(|texts| shows(texts, &bob) && !shows(texts, &loading));
        assert!(shows(&texts, &bob) && !shows(&texts, &loading), "the accounts are searched again: {texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn foreign_blueprints_are_offered_asked_in_the_row_and_only_after_the_yes_taken_out() {
        let _language = setup();
        let (home, paths) = world("fremd");
        assert!(paths.set_setting(ACCOUNT_SETTING, Value::str("Alice")));
        with_collection(&paths, |collection| {
            let mut stock = collection.load();
            assert!(collection.add(&mut stock, "Other Pistol", "log", None));
            assert!(collection.add(&mut stock, "Mine Rifle", "log", None));
            assert!(collection.save(&mut stock));
        });
        let mut bench = Bench::new(home, paths);
        let texts = bench.until(|texts| shows(texts, "Other Pistol"));
        assert!(shows(&texts, "1 Baupläne"), "{texts:?}");
        assert!(shows(&texts, &t("s_er_fremd_weg")));
        // The question stands in the row; cancelling takes nothing out.
        bench.click(&t("s_er_fremd_weg"));
        let texts = bench.settle();
        assert!(shows(&texts, "Aus dem Bestand entfernen?") && shows(&texts, "· Other Pistol"), "{texts:?}");
        bench.click(&t("abbrechen"));
        assert!(with_collection(&bench.paths, |c| collection::contains(&c.load(), "Other Pistol")));
        assert!(shows(&bench.settle(), &t("s_er_fremd_weg")), "back at the offer");
        // And the yes takes it out.
        bench.click(&t("s_er_fremd_weg"));
        bench.settle();
        bench.click(&t("s_mlg_bestaetigen"));
        let texts = bench.until(|texts| shows(texts, &t("s_er_fremd_keine")));
        assert!(shows(&texts, &t("s_er_fremd_keine")), "searched again, nothing left: {texts:?}");
        assert!(with_collection(&bench.paths, |c| !collection::contains(&c.load(), "Other Pistol")), "gone");
        assert!(with_collection(&bench.paths, |c| collection::contains(&c.load(), "Mine Rifle")), "the own one stays");
        assert!(bench.state.take_all_changed(), "the lists are told");
        assert_eq!(bench.state.take_notice(), Some(Said::note(tf("s_er_fremd_ok", &[("0", Arg::from(1i64))]))));
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn blueprints_of_another_account_that_did_not_count_are_told_at_the_account_and_all_accounts_counts_them() {
        use versekit_core::logsource::{self, LogSource, ReadState};
        let _language = setup();
        let (home, paths) = world("nicht-gezaehlt");
        assert!(paths.set_setting(ACCOUNT_SETTING, Value::str("Alice")));
        // The overlay read the logs: Bob's blueprint did not count and is tallied.
        let source = LogSource::new(&paths, LogSource::sources_of(&paths));
        let pattern = versekit_core::phrases::pattern(&source.sources.collect().0);
        source.read_backlog(&mut ReadState::open(&paths), &pattern, true, true);
        assert_eq!(logsource::foreign_tally(&paths).blueprints, 1);
        let mut bench = Bench::new(home, paths);
        let line = tf("s_er_acc_fremd_1", &[("0", Arg::from(1i64))]);
        let texts = bench.until(|texts| shows(texts, &line));
        assert!(shows(&texts, &line), "the number stands at the account: {texts:?}");
        // The way to count them is a button with its name for screen readers.
        let word = t("s_er_acc_fremd_knopf");
        bench.click_named(egui::Role::Button, &word);
        assert_eq!(bench.paths.setting(ACCOUNT_SETTING).as_deref(), Some(ALL_ACCOUNTS));
        assert_eq!(bench.paths.setting_int(keys::RESCAN_NR, 0, None, None), 1, "the overlay reads all logs again");
        assert_eq!(logsource::foreign_tally(&bench.paths).blueprints, 0, "the tally starts anew");
        let texts = bench.until(|texts| !shows(texts, &t("s_er_acc_laedt")) && !shows(texts, &line));
        assert!(!shows(&texts, &line), "{texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_catalog_button_says_how_many_it_got_or_that_it_could_not() {
        let _language = setup();
        let home = scratch("katalog");
        let paths =
            Paths::new(Host::new(Flavor::Posix, &[("SC_BP_HOME", home.as_str()), ("HOME", home.as_str())], &home));
        let mut bench = Bench::new(home, paths);
        bench.click(&t("s_er_kat_jetzt"));
        let mut said = None;
        for _ in 0..100 {
            bench.settle();
            said = bench.state.take_notice().or(said);
            if said.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        // Three blueprints, one without a known way: off, two count.
        assert_eq!(said, Some(Said::note(tf("s_er_kat_da", &[("0", Arg::from(2i64))]))));
        assert!(detection::set_all_blueprints(&bench.paths, true));
        bench.state.build = built;
        bench.click(&t("s_er_kat_jetzt"));
        let mut said = None;
        for _ in 0..100 {
            bench.settle();
            said = bench.state.take_notice().or(said);
            if said.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert_eq!(said, Some(Said::note(tf("s_er_kat_da", &[("0", Arg::from(3i64))]))), "all: three");
        // A build that gives nothing is told as what it is, not as a success (4.12 always said "refreshed").
        bench.state.build = not_built;
        bench.click(&t("s_er_kat_jetzt"));
        let mut said = None;
        for _ in 0..100 {
            bench.settle();
            said = bench.state.take_notice().or(said);
            if said.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert_eq!(said, Some(Said::fault(t("s_er_kat_weg"))));
        let _ = std::fs::remove_dir_all(&bench.home);
    }
}
