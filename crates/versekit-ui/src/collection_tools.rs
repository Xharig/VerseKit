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

//! Three parts of the page "Sichern" (`_backup` of 4.12): giving out the blueprint collection, reading all logs
//! again and resetting the collection; and the notice with the result of the reading.
//!
//! Giving out writes the files of the export folder (`FileExports`): all at once, or one format to a file the
//! player picks. Reading again is the overlay's work (its watcher writes the collection), asked for through the
//! settings; its result comes back the same way and is told as a notice that stays. Resetting removes
//! `bestand.json` with its high-water mark after a question that names how many blueprints come back from the
//! logs on their own and how many are lost.

use egui::{RichText, Ui};
use versekit_core::collection::{self, Collection, NoCatalog};
use versekit_core::export::{self, FileExports, Kind};
use versekit_core::jsonio::{self, Map, Newline};
use versekit_core::language::{Arg, t, tf};
use versekit_core::overlay::{RESCAN_FRESH_SECONDS, RescanResult};
use versekit_core::paths::Paths;
use versekit_core::theme::{self, Scheme};

use crate::blueprint_list::NoEnglishNames;
use crate::color;
use crate::file_dialog::{self, Pending};
use crate::settings::{self, Said};
use crate::widgets::{self, ButtonKind};

/// The formats that can be saved one by one, in the order of their rows.
const SINGLE: [Kind; 3] = [Kind::Basetool, Kind::Scmdb, Kind::Full];

/// State of the three parts.
#[derive(Default)]
pub struct CollectionTools {
    /// The question before a reset is open: (blueprints now, blueprints that come back).
    asking: Option<(usize, usize)>,
    /// A save dialog is open for this format.
    saving: Option<(Kind, Pending)>,
    notice: Option<Said>,
    /// Whether an overlay runs that can read the logs; `None`: the window of the overlay is looked for.
    overlay_runs: Option<Box<dyn Fn() -> bool>>,
    /// The overlay was asked to read again: the window waits for the result.
    asked: bool,
    /// The folders a test run would have opened, in their order.
    #[cfg(test)]
    shown: Vec<String>,
}

impl CollectionTools {
    pub fn take_notice(&mut self) -> Option<Said> {
        self.notice.take()
    }

    fn say(&mut self, said: Said) {
        self.notice = Some(said);
    }

    /// Whether the overlay was asked to read all logs again since the last call.
    pub fn take_asked(&mut self) -> bool {
        std::mem::take(&mut self.asked)
    }

    /// Opens `folder` in the file manager; a test run only notes it.
    fn show_folder(&mut self, folder: &str) {
        #[cfg(not(test))]
        versekit_core::browser::show_folder(folder);
        #[cfg(test)]
        self.shown.push(folder.to_string());
    }
}

fn exports(paths: &Paths) -> FileExports<'_> {
    FileExports { paths, version: env!("CARGO_PKG_VERSION"), clock: export::now_unix }
}

/// The collection as stored.
fn load(paths: &Paths) -> Map {
    let exports = exports(paths);
    collection_of(paths, &exports).load()
}

fn collection_of<'a>(paths: &'a Paths, exports: &'a FileExports<'a>) -> Collection<'a> {
    Collection { paths, catalog: &NoCatalog, names: &NoEnglishNames, exports, clock: collection::now_text }
}

fn kind_name(kind: Kind) -> String {
    match kind {
        Kind::Basetool => t("s_be_art_basetool"),
        Kind::Scmdb => "scmdb.net".to_string(),
        _ => t("s_be_voll"),
    }
}

/// "Bestand ausgeben": one row per format with its own save button, below them all files at once and the
/// folder.
pub fn show_export(ui: &mut Ui, scheme: &Scheme, state: &mut CollectionTools, paths: &Paths) {
    take_saved(state, paths);
    settings::setting_group(ui, scheme, &t("s_be_aus"));
    ui.add(egui::Label::new(t("s_be_aus_h_2")).wrap());
    ui.add_space(6.0);
    let data = load(paths);
    let count = tf("s_be_n_bp", &[("0", Arg::from(collection::count(&data)))]);
    let save_word = t("s_be_speichern_kurz");
    let mut picked = None;
    for kind in SINGLE {
        let name = kind_name(kind);
        let what = if kind == Kind::Full { t("s_be_voll_h") } else { count.clone() };
        let busy = state.saving.is_some().then(|| t("s_be_speichern"));
        // How many a file holds is what the row is about: it stands beside the button; what the full backup holds
        // stands on hover.
        let hint = if kind == Kind::Full { what.as_str() } else { "" };
        settings::setting_row(ui, scheme, &name, hint, false, |row| {
            let button_kind =
                if kind == Kind::Basetool { ButtonKind::Brand(theme::KRT_ORANGE) } else { ButtonKind::Normal };
            let button =
                widgets::Button::new(&save_word, button_kind, scheme).icon("save").spoken(&name).blocked(busy.as_deref());
            if row.add(button).clicked() {
                picked = Some(kind);
            }
            if kind != Kind::Full {
                row.label(egui::RichText::new(&what).small().color(crate::color(scheme.sub)));
            }
        });
    }
    if let Some(kind) = picked {
        let folder = exports(paths).folder();
        let pending = file_dialog::save_file(
            ui.ctx(),
            &t("s_be_speichern"),
            &[("JSON", &["json"])],
            Some(&folder),
            &kind.file_name(),
        );
        state.saving = Some((kind, pending));
    }
    ui.add_space(6.0);
    let (mut all, mut open) = (false, false);
    ui.horizontal_wrapped(|row| {
        all = row.add(widgets::Button::new(&t("s_be_alle"), ButtonKind::Strong, scheme).icon("download")).clicked();
        open = row.add(widgets::Button::new(&t("s_be_ablage"), ButtonKind::Normal, scheme).icon("folder-open")).clicked();
    });
    ui.add(egui::Label::new(RichText::new(t("s_be_fort")).small().color(color(scheme.sub))).wrap());
    if all {
        match exports(paths).archive_files(&data) {
            Ok(done) => {
                state.say(Said::note(tf("s_be_geschrieben", &[("0", Arg::from(done.written.len()))])));
                // The files just written are shown in their folder.
                state.show_folder(&exports(paths).folder());
            }
            Err(e) => {
                versekit_core::errors::record("collection_tools.archive", &e);
                state.say(Said::fault(t("s_be_schiefging")));
            }
        }
    }
    if open {
        state.show_folder(&exports(paths).folder());
    }
}

/// The answer of an open save dialog: the format is written to the file picked.
fn take_saved(state: &mut CollectionTools, paths: &Paths) {
    let Some((kind, pending)) = &state.saving else { return };
    let Some(answer) = pending.take() else { return };
    let kind = *kind;
    state.saving = None;
    let Some(target) = answer else { return };
    let data = load(paths);
    let written = exports(paths)
        .text(kind, &data)
        .and_then(|text| text.ok_or_else(|| "nothing to write".to_string()))
        .and_then(|text| jsonio::write_text_atomic(&target, &text, Newline::Native).map_err(|e| e.to_string()));
    match written {
        Ok(()) => {
            let name = paths.host.flavor.basename(&target);
            state.say(Said::note(tf("s_be_gespeichert", &[("0", Arg::from(name))])));
        }
        Err(e) => {
            versekit_core::errors::record("collection_tools.single", &e);
            state.say(Said::fault(t("s_be_schiefging")));
        }
    }
}

/// "Protokolle erneut einlesen": the overlay reads every log again, also the known ones, and judges the mission log
/// anew (4.12 `neu_einlesen`). Not red: it only adds, nothing goes. Without a running overlay it says that VerseKit
/// has to run for it.
pub fn show_rescan(ui: &mut Ui, scheme: &Scheme, state: &mut CollectionTools, paths: &Paths) {
    let (name, word) = (t("s_be_neu"), t("s_be_neu_knopf"));
    let mut clicked = false;
    settings::setting_row(ui, scheme, &name, &t("s_be_neu_h"), false, |row| {
        let button = widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("rotate-cw").spoken(&name);
        clicked = row.add(button).clicked();
    });
    if !clicked {
        return;
    }
    if overlay_running(state) && versekit_core::detection::ask_for_rescan(paths) {
        state.asked = true;
        state.say(Said::note(t("s_be_neu_los")));
    } else {
        state.say(Said::fault(t("s_be_neu_kein")));
    }
}

/// Whether an overlay runs that can read the logs: the test's answer, otherwise the window of the overlay is looked for.
fn overlay_running(state: &CollectionTools) -> bool {
    match &state.overlay_runs {
        Some(runs) => runs(),
        None => versekit_core::window_signal::find(versekit_core::window_signal::Role::Overlay).is_some(),
    }
}

/// The result of a reading of all logs, told as a notice that stays until it is closed (4.12 `_bescheid_zeigen`):
/// the overlay keeps it in the settings, the window looks at it now and then. A result that was there at the
/// start, or that is older than [`RESCAN_FRESH_SECONDS`], is not told.
#[derive(Default)]
pub struct RescanNotice {
    /// The result last seen, by its time; `None` before the first look.
    seen: Option<Option<f64>>,
    looked: Option<f64>,
    /// The text of the notice while it is open.
    open: Option<String>,
    /// A reading was asked for from here: until then (time of the frames) the window looks without being touched.
    waiting_until: Option<f64>,
}

/// How often the window looks for a new result, in seconds of its frames.
pub const RESCAN_LOOK: f64 = 2.0;

impl RescanNotice {
    /// A reading was asked for at `now`: the result is waited for as long as it counts as fresh.
    pub fn expect(&mut self, now: f64) {
        self.waiting_until = Some(now + RESCAN_FRESH_SECONDS);
    }

    /// Looks for a new result at `now` (the time of the frame) and opens the notice for it; `unix` is the time of
    /// the clock, which the result carries. The first look only takes what is there. True while a result asked
    /// for from here is still awaited (the window then draws again after [`RESCAN_LOOK`]).
    pub fn follow(&mut self, paths: &Paths, now: f64, unix: f64) -> bool {
        let waiting = self.waiting_until.is_some_and(|until| now < until);
        if self.looked.is_some_and(|at| (0.0..RESCAN_LOOK).contains(&(now - at))) {
            return waiting;
        }
        self.looked = Some(now);
        let result = RescanResult::load(paths);
        let at = result.map(|r| r.at);
        let first = self.seen.is_none();
        if self.seen.replace(at) == Some(at) || first {
            return waiting;
        }
        if let Some(result) = result.filter(|r| unix - r.at < RESCAN_FRESH_SECONDS) {
            self.open = Some(result.phrase().to_string());
            self.waiting_until = None;
            return false;
        }
        waiting
    }

    pub fn text(&self) -> Option<&str> {
        self.open.as_deref()
    }

    /// Draws the notice while it is open; OK, Esc or a click beside it closes it.
    pub fn show(&mut self, ctx: &egui::Context, scheme: &Scheme) {
        let Some(text) = &self.open else { return };
        let (ok, close) = widgets::dialog(ctx, scheme, "einlesen-ergebnis", |d| {
            widgets::heading(d, scheme, &t("s_be_neu"));
            d.add(egui::Label::new(text.as_str()).wrap());
            d.add_space(8.0);
            d.add(widgets::Button::new(&t("e_ok"), ButtonKind::Strong, scheme)).clicked()
        });
        if ok || close {
            self.open = None;
        }
    }
}

/// "Bestand zurücksetzen": the row with the red button, the warning below it and the question with the numbers.
pub fn show_reset(ui: &mut Ui, scheme: &Scheme, state: &mut CollectionTools, paths: &Paths) {
    let (name, hint, word) = (t("s_be_reset"), t("s_be_reset_h"), t("s_zuruecksetzen"));
    let mut clicked = false;
    settings::setting_row(ui, scheme, &name, &hint, false, |row| {
        let button = widgets::Button::new(&word, ButtonKind::Danger, scheme).icon("rotate-ccw").spoken(&name);
        clicked = row.add(button).clicked();
    });
    ui.add_space(6.0);
    widgets::status_box(ui, scheme, scheme.gold, "triangle-alert", &t("s_be_reset_warn"), &t("s_be_reset_warn_h_2"));
    if clicked {
        let data = load(paths);
        state.asking = Some((collection::count(&data), collection::restorable(&data)));
    }
    let Some((total, back)) = state.asking else { return };
    let mut text = t("s_be_reset_frage");
    if total > 0 {
        let numbers = [("0", Arg::from(total)), ("1", Arg::from(back)), ("2", Arg::from(total.saturating_sub(back)))];
        text = format!("{}\n\n{text}", tf("s_be_reset_zahlen_2", &numbers));
    }
    let ((yes, no), close) = widgets::dialog(ui.ctx(), scheme, "bestand-zuruecksetzen", |d| {
        widgets::heading(d, scheme, &name);
        d.add(egui::Label::new(text.as_str()).wrap());
        d.add_space(8.0);
        d.horizontal(|row| {
            let no = row.add(widgets::Button::new(&t("e_nein"), ButtonKind::Normal, scheme)).clicked();
            let yes = row.add(widgets::Button::new(&t("e_ja"), ButtonKind::Danger, scheme)).clicked();
            (yes, no)
        })
        .inner
    });
    if no || close {
        state.asking = None;
        return;
    }
    if !yes {
        return;
    }
    state.asking = None;
    let exports = exports(paths);
    let failed = {
        let _lock = jsonio::lock_saves();
        collection_of(paths, &exports).reset()
    };
    match failed {
        // A running overlay builds it up again at once, otherwise the next start does.
        None if overlay_running(state) && versekit_core::detection::ask_for_rescan(paths) => {
            state.asked = true;
            state.say(Said::note(t("s_be_reset_neu")));
        }
        None => state.say(Said::note(t("s_be_reset_ok_2"))),
        Some(e) => {
            versekit_core::errors::record("collection_tools.reset", &e);
            state.say(Said::fault(tf("s_be_reset_fehler", &[("0", Arg::from(e.to_string()))])));
        }
    }
}

#[cfg(test)]
mod tests {
    use egui::{Event, Pos2, Rect};
    use versekit_core::language::{self, Choice};
    use versekit_core::paths::Host;
    use versekit_core::pypath::Flavor;

    use super::*;

    type Texts = Vec<(String, Pos2)>;

    fn scratch(name: &str) -> String {
        let dir =
            std::env::temp_dir().join(format!("versekit-bestand-{}-{name}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        dir.to_string_lossy().replace('\\', "/")
    }

    /// A data folder whose collection holds one blueprint from a log and one set by hand.
    fn world(name: &str) -> (String, Paths) {
        let home = scratch(name);
        let paths =
            Paths::new(Host::new(Flavor::Posix, &[("SC_BP_HOME", home.as_str()), ("HOME", home.as_str())], &home));
        let exports = exports(&paths);
        let collection = collection_of(&paths, &exports);
        let mut stock = collection.load();
        collection.add(&mut stock, "Arclight Pistol", "log", None);
        collection.add(&mut stock, "Pembroke Helmet", "hand", None);
        assert!(collection.save(&mut stock));
        (home, paths)
    }

    struct Bench {
        ctx: egui::Context,
        state: CollectionTools,
        paths: Paths,
    }

    impl Bench {
        fn new(paths: Paths) -> Bench {
            let ctx = egui::Context::default();
            crate::install(&ctx);
            Bench { ctx, state: CollectionTools::default(), paths }
        }

        fn frame(&mut self, events: Vec<Event>) -> Texts {
            let scheme = theme::by_id("original").expect("original");
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 1400.0))),
                events,
                ..Default::default()
            };
            let (state, paths) = (&mut self.state, &self.paths);
            let mut output = self.ctx.run_ui(input, |ui| {
                show_export(ui, scheme, state, paths);
                show_rescan(ui, scheme, state, paths);
                show_reset(ui, scheme, state, paths);
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
            texts
        }

        fn settle(&mut self) -> Texts {
            let mut last = Vec::new();
            for _ in 0..4 {
                last = self.frame(Vec::new());
            }
            last
        }

        /// A click on the first text `label`, as a player makes it; the texts after it settled.
        fn click(&mut self, label: &str) -> Texts {
            let texts = self.settle();
            let at = texts
                .iter()
                .find(|(text, _)| text == label)
                .map(|(_, at)| *at + egui::vec2(3.0, 3.0))
                .unwrap_or_else(|| panic!("{label} is not on the screen: {texts:?}"));
            let button = |pressed| Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            self.frame(vec![Event::PointerMoved(at)]);
            self.frame(vec![button(true)]);
            self.frame(vec![button(false)]);
            self.settle()
        }
    }

    fn shows(texts: &Texts, part: &str) -> bool {
        texts.iter().any(|(text, _)| text.contains(part))
    }

    fn setup() -> std::sync::MutexGuard<'static, ()> {
        let guard = crate::language_lock();
        theme::set_current("original");
        language::set_thread_language(Choice::De);
        guard
    }

    #[test]
    fn the_reset_asks_with_the_numbers_and_only_a_yes_removes_the_collection() {
        let _language = setup();
        let (home, paths) = world("zuruecksetzen");
        let file = collection_of(&paths, &exports(&paths)).path();
        let mut bench = Bench::new(paths);
        let texts = bench.click(&t("s_zuruecksetzen"));
        let numbers =
            tf("s_be_reset_zahlen_2", &[("0", Arg::from(2usize)), ("1", Arg::from(1usize)), ("2", Arg::from(1usize))]);
        assert!(shows(&texts, &numbers), "the question names how many come back: {texts:?}");
        let texts = bench.click(&t("e_nein"));
        assert!(!shows(&texts, &numbers), "a no closes the question: {texts:?}");
        assert!(std::path::Path::new(&file).is_file(), "a no keeps the collection");
        assert_eq!(bench.state.take_notice(), None, "a no says nothing");

        bench.click(&t("s_zuruecksetzen"));
        bench.click(&t("e_ja"));
        assert!(!std::path::Path::new(&file).exists(), "a yes removes the collection");
        assert_eq!(bench.state.take_notice(), Some(Said::note(t("s_be_reset_ok_2"))));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn reading_again_asks_the_running_overlay_and_says_when_none_runs() {
        use versekit_core::overlay::keys;
        let _language = setup();
        let (home, paths) = world("neu-einlesen");
        let mut bench = Bench::new(paths);
        let asked = |bench: &Bench| bench.paths.setting_int(keys::RESCAN_NR, 0, None, None);
        bench.state.overlay_runs = Some(Box::new(|| false));
        bench.click(&t("s_be_neu_knopf"));
        assert_eq!(bench.state.take_notice(), Some(Said::fault(t("s_be_neu_kein"))), "VerseKit has to run for it");
        assert_eq!(asked(&bench), 0, "nobody to ask");
        assert!(!bench.state.take_asked());
        bench.state.overlay_runs = Some(Box::new(|| true));
        bench.click(&t("s_be_neu_knopf"));
        assert_eq!(bench.state.take_notice(), Some(Said::note(t("s_be_neu_los"))));
        assert_eq!(asked(&bench), 1, "the overlay sees the number raised");
        assert!(bench.state.take_asked(), "the window waits for the result");
        // A reset with a running overlay builds the collection up again at once.
        bench.click(&t("s_zuruecksetzen"));
        bench.click(&t("e_ja"));
        assert_eq!(asked(&bench), 2, "read again after the reset");
        assert_eq!(bench.state.take_notice(), Some(Said::note(t("s_be_reset_neu"))));
        assert!(bench.state.take_asked());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn the_result_of_the_overlay_is_told_once_and_only_while_it_is_fresh() {
        let _language = setup();
        let (home, paths) = world("einlesen-bescheid");
        let result = |at: f64| RescanResult {
            at,
            files: 12,
            blueprints: 2,
            contracts_new: 3,
            contracts_corrected: 1,
            failed: false,
        };
        let mut notice = RescanNotice::default();
        assert!(result(1000.0).save(&paths));
        assert!(!notice.follow(&paths, 0.0, 1010.0));
        assert_eq!(notice.text(), None, "what was there at the start is not told");
        // Asked from here: the window looks again by itself until the result is there.
        notice.expect(0.5);
        assert!(notice.follow(&paths, 1.0, 1011.0), "waiting, looked at again soon");
        assert!(result(2000.0).save(&paths));
        assert!(notice.follow(&paths, 1.5, 2001.0), "looked at once in two seconds");
        assert_eq!(notice.text(), None);
        assert!(!notice.follow(&paths, 3.0, 2001.0), "the result came: nothing more to wait for");
        assert_eq!(notice.text(), Some(result(2000.0).phrase().to_string().as_str()));
        notice.open = None;
        assert!(!notice.follow(&paths, 6.0, 2010.0));
        assert_eq!(notice.text(), None, "told once");
        // One that came while the window was out of sight for long is not told.
        assert!(result(3000.0).save(&paths));
        notice.follow(&paths, 9.0, 3000.0 + RESCAN_FRESH_SECONDS + 1.0);
        assert_eq!(notice.text(), None, "too old to be told");
        // A reading that failed is told with the sentence of the failure.
        assert!(RescanResult::failure(4000.0).save(&paths));
        notice.follow(&paths, 12.0, 4001.0);
        assert_eq!(notice.text(), Some(t("neu_gelesen_fehler").as_str()));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn all_files_go_into_the_export_folder_and_say_how_many() {
        let _language = setup();
        let (home, paths) = world("ablage");
        let folder = exports(&paths).folder();
        let mut bench = Bench::new(paths);
        let texts = bench.settle();
        assert!(shows(&texts, &tf("s_be_n_bp", &[("0", Arg::from(2usize))])), "the count of the rows: {texts:?}");
        assert!(bench.state.shown.is_empty(), "nothing is opened before a click");
        bench.click(&t("s_be_alle"));
        for kind in SINGLE {
            let file = format!("{folder}/{}", kind.file_name());
            assert!(std::path::Path::new(&file).is_file(), "{file}");
        }
        assert_eq!(bench.state.take_notice(), Some(Said::note(tf("s_be_geschrieben", &[("0", Arg::from(3usize))]))));
        assert_eq!(bench.state.shown, std::slice::from_ref(&folder), "the folder of the files opens after them");
        // The folder button opens the same folder.
        bench.click(&t("s_be_ablage"));
        assert_eq!(bench.state.shown, [folder.clone(), folder], "the folder button");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn one_format_is_saved_to_the_file_picked() {
        let _language = setup();
        let (home, paths) = world("einzeln");
        let mut bench = Bench::new(paths);
        crate::file_dialog::stand_in::start();
        bench.click(&t("s_be_speichern_kurz"));
        assert_eq!(crate::file_dialog::stand_in::stop(), vec!["save"], "the dialog to pick the file");
        assert!(matches!(bench.state.saving, Some((Kind::Basetool, _))), "the button of the first row");
        // The dialog answers with a file.
        let target = format!("{home}/mein-stand.json");
        bench.state.saving = Some((Kind::Basetool, Pending::answered(Some(target.clone()))));
        bench.settle();
        let text = std::fs::read_to_string(&target).expect("written");
        assert!(text.contains("Arclight"), "{text}");
        assert_eq!(bench.state.take_notice(), Some(Said::note(tf("s_be_gespeichert", &[("0", Arg::from("mein-stand.json"))]))));
        let _ = std::fs::remove_dir_all(&home);
    }
}
