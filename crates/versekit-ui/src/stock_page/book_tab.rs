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

//! The tab "Lager-Scans" of the stock page: the open questions of the stock scan (a reading the reader was not
//! sure of: the player picks the item) and the last scans with the stock each went to, the place, the amount and
//! what it came to, a button "Rückgängig" in each row that puts the value before the scan back at the place.
//! Whatever lies in the stock of the unit is booked back through the connection to it, in a thread of its own.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::SystemTime;

use egui::{RichText, Ui};
use versekit_core::language::SpawnWithLanguage as _;
use versekit_core::language::{self, Arg, t, tf};
use versekit_core::overlay;
use versekit_core::paths::Paths;
use versekit_core::scan_notice::Target;
use versekit_core::stock_scan_book::org::OrgLink;
use versekit_core::stock_scan_book::{self, Journal, Scan, UndoError, UnitStock, What};
use versekit_core::theme::Scheme;

use super::modified;
use crate::widgets::{self, ButtonKind};
use crate::{color, layout};

/// The picture of a question in the table: scaled to fill this box in points, keeping its shape.
const QUESTION_PICTURE: egui::Vec2 = egui::vec2(360.0, 90.0);

/// The picture of a question while the pointer is on it: scaled to fill this box in points, keeping its shape.
const QUESTION_PICTURE_LARGE: egui::Vec2 = egui::vec2(900.0, 260.0);

/// The column of the text read, at most this wide in points.
const READ_COLUMN: f32 = 220.0;

/// How often the file is looked at, in seconds.
const LOOK_SECONDS: f64 = 1.0;

/// What the tab does on a click.
#[derive(Clone, Debug, PartialEq)]
enum Work {
    /// Answers the question at the index (oldest first) with an item, or with none.
    Answer(usize, Option<String>),
    /// Takes scan `id` back.
    Undo(i64),
    /// Makes the booking that waits at the index, taking what is set aside for orders.
    Confirm(usize),
    /// Lets the booking that waits at the index go.
    DropConfirm(usize),
}

/// What a piece of work came to, as a line for the player.
struct Done {
    text: String,
    good: bool,
}

/// What the tab keeps.
pub(super) struct BookTab {
    paths: Option<Paths>,
    journal: Journal,
    seen: Option<SystemTime>,
    looked_at: f64,
    /// The connection to the stock of the unit, where there is one.
    unit: Option<Arc<OrgLink>>,
    working: Option<Receiver<Done>>,
    note: Option<Done>,
}

impl Default for BookTab {
    fn default() -> BookTab {
        BookTab {
            paths: None,
            journal: Journal::default(),
            seen: None,
            looked_at: f64::NEG_INFINITY,
            unit: None,
            working: None,
            note: None,
        }
    }
}

fn file_of(paths: &Paths) -> String {
    paths.app_file(stock_scan_book::FILE)
}

impl BookTab {
    /// Lets the tab read the journal of these paths (a copy of them is kept).
    pub(super) fn connect(&mut self, paths: &Paths) {
        self.paths = Some(paths.working_copy());
        self.reload();
    }

    /// Lets the tab book into the stock of the unit through this connection.
    pub(super) fn connect_unit(&mut self, link: Option<Arc<OrgLink>>) {
        self.unit = link;
    }

    fn reload(&mut self) {
        let Some(paths) = &self.paths else { return };
        self.seen = modified(&file_of(paths));
        self.journal = Journal::load(paths);
    }

    /// The number of open questions, for the tab.
    pub(super) fn open_questions(&self) -> usize {
        self.journal.questions.len() + self.journal.confirms.len()
    }

    /// Reads the journal again when its file changed (the overlay writes it), at most once a second.
    pub(super) fn follow(&mut self, now: f64) {
        if now - self.looked_at < LOOK_SECONDS {
            return;
        }
        self.looked_at = now;
        let Some(paths) = &self.paths else { return };
        if modified(&file_of(paths)) != self.seen {
            self.reload();
        }
        if let Some(finished) = self.working.as_ref().map(Receiver::try_recv) {
            match finished {
                Ok(done) => {
                    self.note = Some(done);
                    self.working = None;
                    self.reload();
                }
                Err(mpsc::TryRecvError::Disconnected) => self.working = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
    }

    fn start(&mut self, ctx: &egui::Context, work: Work) {
        let Some(paths) = self.paths.as_ref().map(Paths::working_copy) else { return };
        let into_unit = match &work {
            Work::Undo(id) => self.journal.scans.iter().find(|s| s.id == *id).map(|s| s.target),
            Work::Answer(index, _) => self.journal.questions.get(*index).map(|q| q.target),
            Work::Confirm(_) => Some(Target::Unit),
            Work::DropConfirm(_) => None,
        } == Some(Target::Unit);
        let unit = self.unit.clone();
        if !into_unit {
            // The own stock is a file: done at once.
            self.note = Some(execute(&paths, None, &work));
            self.reload();
            return;
        }
        if self.working.is_some() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.working = Some(receiver);
        let ctx = ctx.clone();
        let started = std::thread::Builder::new().name("lagerscan-einheit".into()).spawn_with_language(move || {
            let link: Option<&dyn UnitStock> = unit.as_deref().map(|l| l as &dyn UnitStock);
            let _ = sender.send(execute(&paths, link, &work));
            ctx.request_repaint();
        });
        if started.is_err() {
            self.working = None;
        }
    }
}

/// Does the work against the stocks and says what came of it.
fn execute(paths: &Paths, unit: Option<&dyn UnitStock>, work: &Work) -> Done {
    let now = versekit_core::export::now_unix();
    match work {
        Work::Answer(index, chosen) => match stock_scan_book::answer(paths, unit, *index, chosen.as_deref(), now) {
            Some(outcome) => {
                let notice = outcome.notice_in(language::current());
                let good = matches!(
                    outcome,
                    stock_scan_book::Outcome::Booked(_)
                        | stock_scan_book::Outcome::Same(_)
                        | stock_scan_book::Outcome::Added(_)
                );
                Done { text: format!("{}: {}", notice.title, notice.detail), good }
            }
            None => Done { text: t("s_lsc_msg_verworfen"), good: true },
        },
        Work::Confirm(index) => match stock_scan_book::confirm_reserved(paths, unit, *index, now) {
            Some(outcome) => {
                let notice = outcome.notice_in(language::current());
                let good = matches!(
                    outcome,
                    stock_scan_book::Outcome::Booked(_)
                        | stock_scan_book::Outcome::Same(_)
                        | stock_scan_book::Outcome::Added(_)
                );
                Done { text: format!("{}: {}", notice.title, notice.detail), good }
            }
            None => Done { text: t("s_lsc_msg_verworfen"), good: true },
        },
        Work::DropConfirm(index) => {
            stock_scan_book::drop_confirm(paths, *index);
            Done { text: t("s_lsc_msg_verworfen"), good: true }
        }
        Work::Undo(id) => {
            let scan = Journal::load(paths).scans.into_iter().find(|s| s.id == *id);
            match stock_scan_book::undo(paths, unit, *id) {
                Ok(()) => Done { text: back_text(scan.as_ref()), good: true },
                Err(error) => Done { text: undo_error_text(&error), good: false },
            }
        }
    }
}

fn back_text(scan: Option<&Scan>) -> String {
    let Some(scan) = scan else { return t("s_lsc_msg_zurueck_ohne") };
    let before = match scan.before {
        Some(amount) => {
            let mut item = scan.item.clone();
            item.quantity = amount;
            stock_scan_book::amount_in(language::current(), &item)
        }
        None => t("s_lsc_kein_eintrag"),
    };
    tf(
        "s_lsc_msg_zurueck",
        &[("0", Arg::from(scan.item.name.as_str())), ("1", Arg::from(scan.place.as_str())), ("2", Arg::from(before))],
    )
}

fn undo_error_text(error: &UndoError) -> String {
    t(match error {
        UndoError::Gone => "s_lsc_zurueck_schon",
        UndoError::Newer => "s_lsc_zurueck_neuer",
        UndoError::NoRight => "s_lsc_zurueck_recht",
        UndoError::Failed => "s_lsc_zurueck_fehler",
    })
}

/// `13:53` for a Unix time, empty where it is not known.
fn clock(at: i64) -> String {
    versekit_core::formats::local_time(at).map_or(String::new(), |time| format!("{:02}:{:02}", time.hour, time.minute))
}

/// The name of a stock with its colour: the own stock in the accent, the stock of the unit in gold, as the notice
/// over the game shows them.
fn target_label(ui: &mut Ui, scheme: &Scheme, target: Target) {
    let tint = match target {
        Target::Own => scheme.accent,
        Target::Unit => scheme.gold,
    };
    ui.label(RichText::new(t(stock_scan_book::target_title_key(target))).strong().color(color(tint)));
}

fn column_head(ui: &mut Ui, scheme: &Scheme, key: &str) {
    ui.label(RichText::new(t(key)).strong().color(color(scheme.sub)));
}

/// Why the button of a scan is blocked, `None` where it works.
fn blocked_reason(tab: &BookTab, scan: &Scan) -> Option<String> {
    if let Some(error) = stock_scan_book::undo_blocked(&tab.journal, scan.id) {
        return Some(undo_error_text(&error));
    }
    if scan.target == Target::Unit {
        let logistician = tab.paths.as_ref().is_some_and(overlay::is_logistician);
        if !logistician || tab.unit.is_none() {
            return Some(undo_error_text(&UndoError::NoRight));
        }
    }
    None
}

fn result_text(scan: &Scan) -> String {
    if scan.undone {
        return t("s_lsc_e_zurueck");
    }
    match (scan.what, scan.before) {
        (What::Same, _) => t("s_lsc_e_gleich"),
        (What::Added, before) => {
            let mut item = scan.item.clone();
            item.quantity = before.unwrap_or(0.0);
            tf("s_lsc_e_dazu", &[("0", Arg::from(stock_scan_book::amount_in(language::current(), &item)))])
        }
        (What::Set, None) => t("s_lsc_e_neu"),
        (What::Set, Some(before)) => {
            let mut item = scan.item.clone();
            item.quantity = before;
            tf("s_lsc_e_gesetzt", &[("0", Arg::from(stock_scan_book::amount_in(language::current(), &item)))])
        }
    }
}

/// The tab: the questions, then the last scans.
pub(super) fn show(ui: &mut Ui, scheme: &Scheme, tab: &mut BookTab) {
    let now = ui.input(|i| i.time);
    tab.follow(now);
    let mut work: Option<Work> = None;
    let mut discard = false;
    ui.add(egui::Label::new(RichText::new(t("s_lsc_lead")).small().color(color(scheme.sub))).wrap());
    ui.add_space(4.0);
    if tab.working.is_some() {
        widgets::loading(ui, scheme, &t("s_lsc_arbeitet"));
        ui.add_space(4.0);
    }
    if let Some(note) = &tab.note {
        let tint = if note.good { scheme.accent } else { scheme.red_text() };
        ui.add(egui::Label::new(RichText::new(&note.text).color(color(tint))).wrap());
        ui.add_space(4.0);
    }
    egui::ScrollArea::vertical().id_salt("lager-scans").auto_shrink([false, false]).show(ui, |ui| {
        if !tab.journal.confirms.is_empty() {
            widgets::card_title(ui, scheme, &t("s_lsc_bestaetigen_kopf"));
            ui.label(RichText::new(t("s_lsc_bestaetigen_text")).color(color(scheme.sub)));
            ui.add_space(4.0);
            egui::Grid::new("lager-bestaetigen").num_columns(7).striped(true).spacing([14.0, 8.0]).show(ui, |grid| {
                for key in [
                    "s_lsc_sp_zeit",
                    "s_lsc_sp_ziel",
                    "s_lsc_sp_gegenstand",
                    "s_lsc_sp_ort",
                    "s_lsc_sp_soll",
                    "s_lsc_sp_vorgemerkt",
                ] {
                    column_head(grid, scheme, key);
                }
                grid.label("");
                grid.end_row();
                for (i, waiting) in tab.journal.confirms.iter().enumerate() {
                    let item = &waiting.pending.item;
                    grid.label(clock(waiting.at));
                    target_label(grid, scheme, waiting.target);
                    grid.add(egui::Label::new(RichText::new(&item.name)).wrap());
                    grid.add(egui::Label::new(RichText::new(&waiting.pending.place).color(color(scheme.sub))).wrap());
                    let mut after = item.clone();
                    after.quantity = waiting.pending.wanted;
                    grid.label(stock_scan_book::amount_in(language::current(), &after));
                    let mut held = item.clone();
                    held.quantity = waiting.reserved;
                    grid.label(stock_scan_book::amount_in(language::current(), &held));
                    grid.horizontal(|cell| {
                        let word = t("s_lsc_trotzdem");
                        let spoken = tf("s_lsc_trotzdem_n", &[("0", Arg::from(item.name.as_str()))]);
                        let book = widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("check").spoken(&spoken);
                        if cell.add(book).clicked() {
                            work = Some(Work::Confirm(i));
                        }
                        let word = t("s_lsc_verwerfen");
                        let spoken = tf("s_lsc_verwerfen_n", &[("0", Arg::from(item.name.as_str()))]);
                        let drop = widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("x").spoken(&spoken);
                        if cell.add(drop).clicked() {
                            work = Some(Work::DropConfirm(i));
                        }
                    });
                    grid.end_row();
                }
            });
            ui.add_space(layout::GAP + 6.0);
        }
        widgets::card_title(ui, scheme, &t("s_lsc_fragen_kopf"));
        if tab.journal.questions.is_empty() {
            ui.label(RichText::new(t("s_lsc_keine_fragen")).color(color(scheme.sub)));
        } else {
            ui.label(RichText::new(t("s_lsc_frage_text")).color(color(scheme.sub)));
            ui.add_space(4.0);
            egui::Grid::new("lager-fragen").num_columns(6).striped(true).spacing([14.0, 8.0]).show(ui, |grid| {
                for key in ["s_lsc_sp_zeit", "s_lsc_sp_ziel", "s_lsc_sp_bild", "s_lsc_sp_gelesen", "s_lsc_sp_ort", "s_lsc_sp_wahl"]
                {
                    column_head(grid, scheme, key);
                }
                grid.end_row();
                for (i, asked) in tab.journal.questions.iter().enumerate() {
                    grid.label(clock(asked.at));
                    target_label(grid, scheme, asked.target);
                    match &asked.picture {
                        Some(picture) => {
                            let uri = format!("bytes://lager-frage/{}-{i}-{}.png", asked.at, picture.len());
                            let image = egui::Image::from_bytes(uri, picture.clone());
                            grid.add(image.clone().fit_to_exact_size(QUESTION_PICTURE)).on_hover_ui(|tip| {
                                tip.add(image.fit_to_exact_size(QUESTION_PICTURE_LARGE));
                            });
                        }
                        None => {
                            grid.label("–");
                        }
                    }
                    grid.vertical(|cell| {
                        cell.set_max_width(READ_COLUMN);
                        let read = stock_scan_book::amount_in(language::current(), &asked.item);
                        let text = if asked.item.name.is_empty() { read } else { format!("{read} {}", asked.item.name) };
                        cell.add(egui::Label::new(RichText::new(text).color(color(scheme.sub))).wrap());
                    });
                    grid.add(egui::Label::new(RichText::new(&asked.place).color(color(scheme.sub))).wrap());
                    grid.vertical(|cell| {
                        for name in &asked.candidates {
                            if cell.add(widgets::Button::new(name, ButtonKind::Normal, scheme)).clicked() {
                                work = Some(Work::Answer(i, Some(name.clone())));
                            }
                        }
                        let word = t("s_lsc_keiner_davon");
                        if cell.add(widgets::Button::new(&word, ButtonKind::Normal, scheme)).clicked() {
                            work = Some(Work::Answer(i, None));
                        }
                    });
                    grid.end_row();
                }
            });
            ui.add_space(6.0);
            let word = t("s_lsc_alle_verwerfen");
            discard = ui.add(widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("x")).clicked();
        }
        ui.add_space(layout::GAP + 6.0);
        widgets::card_title(ui, scheme, &t("s_lsc_zuletzt"));
        if tab.journal.scans.is_empty() {
            ui.label(RichText::new(t("s_lsc_keine_scans")).color(color(scheme.sub)));
            return;
        }
        egui::Grid::new("lager-scans-liste").num_columns(7).striped(true).spacing([14.0, 4.0]).show(ui, |grid| {
            for key in [
                "s_lsc_sp_zeit",
                "s_lsc_sp_ziel",
                "s_lsc_sp_gegenstand",
                "s_lsc_sp_menge",
                "s_lsc_sp_ort",
                "s_lsc_sp_ergebnis",
            ] {
                column_head(grid, scheme, key);
            }
            grid.label("");
            grid.end_row();
            for scan in tab.journal.newest_first() {
                grid.label(clock(scan.at));
                target_label(grid, scheme, scan.target);
                grid.add(egui::Label::new(RichText::new(&scan.item.name)).wrap());
                grid.label(stock_scan_book::amount_in(language::current(), &scan.item));
                grid.add(egui::Label::new(RichText::new(&scan.place).color(color(scheme.sub))).wrap());
                grid.add(egui::Label::new(RichText::new(result_text(scan)).color(color(scheme.sub))).wrap());
                if scan.undone {
                    grid.label("");
                } else {
                    let reason = blocked_reason(tab, scan);
                    let word = t("s_lsc_rueckgaengig");
                    let spoken = tf("s_lsc_rueckgaengig_n", &[("0", Arg::from(scan.item.name.as_str()))]);
                    let button = widgets::Button::new(&word, ButtonKind::Normal, scheme)
                        .icon("undo-2")
                        .spoken(&spoken)
                        .blocked(reason.as_deref());
                    let hover = tf(
                        "s_lsc_rueckgaengig_h",
                        &[("0", Arg::from(scan.place.as_str())), ("1", Arg::from(t(stock_scan_book::target_title_key(scan.target))))],
                    );
                    // A blocked button says why on hover by itself.
                    let response = grid.add(button);
                    let response = if reason.is_none() { response.on_hover_text(hover) } else { response };
                    if response.clicked() && reason.is_none() {
                        work = Some(Work::Undo(scan.id));
                    }
                }
                grid.end_row();
            }
        });
    });
    if discard {
        if let Some(paths) = &tab.paths {
            stock_scan_book::discard_questions(paths);
        }
        tab.reload();
    } else if let Some(work) = work {
        tab.start(ui.ctx(), work);
    }
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;
    use versekit_core::scan_notice::Target;
    use versekit_core::stock;
    use versekit_core::stock_scan_book::{Measure, Reading, Run, ScannedItem};
    use versekit_core::testdir::Dir;

    use super::*;

    fn paths_in(dir: &Dir) -> Paths {
        let home = dir.path().to_string_lossy().into_owned();
        let vars = [(versekit_core::envvars::ENV_HOME, home.as_str()), (versekit_core::envvars::ENV_NO_NET, "1")];
        Paths::new(versekit_core::paths::Host::new(versekit_core::pypath::Flavor::native(), &vars, &home))
    }

    fn scan(paths: &Paths, target: Target, name: &str, quantity: f64, now: i64) {
        let item = ScannedItem::new(name, quantity, Measure::Pieces, None);
        let context = Run { paths, target, place: Some("Orison".into()), place_since: None, unit: None, now };
        stock_scan_book::run(&context, Reading::Sure(item));
    }

    fn harness(tab: BookTab) -> egui_kittest::Harness<'static, BookTab> {
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1100.0, 700.0))
            .with_context_setup(crate::install)
            .with_accessibility_check(false)
            .allow_missing_glyphs()
            .build_ui_state(|ui, tab: &mut BookTab| show(ui, versekit_core::theme::current(), tab), tab);
        harness.run_steps(3);
        harness
    }

    fn open_tab(paths: &Paths) -> BookTab {
        let mut tab = BookTab::default();
        tab.connect(paths);
        tab
    }

    fn has(harness: &egui_kittest::Harness<'static, BookTab>, text: &str) -> bool {
        harness.query_all_by_label_contains(text).next().is_some()
    }

    /// The buttons that take a scan back, newest row first (their name for a screen reader).
    fn undo_buttons<'a>(
        harness: &'a egui_kittest::Harness<'static, BookTab>,
    ) -> Vec<egui_kittest::Node<'a>> {
        harness.query_all_by_label_contains("rückgängig machen").collect()
    }

    /// The tab names the stock of each scan, the amount and the place, and the button takes the scan back: the
    /// value before stands at the place again, and the row says so.
    #[test]
    fn the_button_takes_a_scan_back_and_the_rows_name_their_stock() {
        let dir = Dir::new("lagerscan-reiter");
        let paths = paths_in(&dir);
        language::set_thread_language(language::Choice::De);
        scan(&paths, Target::Own, "Parallax", 5.0, 1);
        // A new round: the amount is set, not added.
        scan(&paths, Target::Own, "Parallax", 3.0, 5_000);
        let mut harness = harness(open_tab(&paths));
        assert!(has(&harness, "Mein Lager"), "the stock stands in the row");
        assert!(has(&harness, "Parallax") && has(&harness, "Orison"));
        assert!(has(&harness, "gesetzt, vorher 5\u{d7}"), "the value before is told");
        assert!(has(&harness, "Keine offenen Rückfragen"));
        // The older scan stands under the newer one: its button is blocked, the newer one is not.
        assert_eq!(undo_buttons(&harness).len(), 2);
        assert!(harness.state().journal.scans.iter().all(|s| !s.undone));
        undo_buttons(&harness).first().expect("newest row first").click();
        harness.run_steps(3);
        let entries = stock::load(&paths.app_file(stock::TRADE_FILE), "test");
        assert_eq!(stock::number_of(&entries[0], "menge"), 5.0, "back to the value before the scan");
        assert!(has(&harness, "zurückgenommen"), "the row says it was taken back");
        assert!(has(&harness, "Zurückgenommen: Parallax"), "the line above says what came back");
    }

    /// While the booking into the stock of the unit runs on its thread, the tab says so where the result will stand.
    #[test]
    fn the_tab_says_that_a_booking_into_the_unit_runs() {
        let dir = Dir::new("lagerscan-reiter-arbeit");
        let paths = paths_in(&dir);
        language::set_thread_language(language::Choice::De);
        let mut tab = open_tab(&paths);
        let said = t("s_lsc_arbeitet");
        let (sender, receiver) = mpsc::channel::<Done>();
        tab.working = Some(receiver);
        let mut harness = harness(tab);
        assert!(has(&harness, &said), "the booking runs");
        // The answer came: the line is gone.
        sender.send(Done { text: "fertig".into(), good: true }).unwrap();
        harness.state_mut().looked_at = f64::NEG_INFINITY;
        harness.run_steps(3);
        assert!(!has(&harness, &said), "the booking ended");
        assert!(has(&harness, "fertig"));
    }

    /// An open question shows its candidates and takes the pick as a booking; none of them books nothing.
    #[test]
    fn a_question_is_answered_in_the_tab() {
        let dir = Dir::new("lagerscan-reiter-frage");
        let paths = paths_in(&dir);
        language::set_thread_language(language::Choice::De);
        let item = ScannedItem::new("Parallx", 4.0, Measure::Pieces, None);
        let context = Run {
            paths: &paths,
            target: Target::Own,
            place: Some("Orison".into()),
            place_since: None,
            unit: None,
            now: 1,
        };
        let unsure = Reading::Unsure {
            item,
            candidates: vec!["Parallax".into(), "Parallel".into()],
            picture: None,
        };
        stock_scan_book::run(&context, unsure);
        let mut harness = harness(open_tab(&paths));
        assert!(has(&harness, "Welcher Gegenstand?"));
        assert!(has(&harness, "Keiner davon") && has(&harness, "Parallel"));
        harness.get_by_label("Parallax").click();
        harness.run_steps(3);
        let entries = stock::load(&paths.app_file(stock::TRADE_FILE), "test");
        assert_eq!(stock::number_of(&entries[0], "menge"), 4.0);
        assert!(has(&harness, "Keine offenen Rückfragen"), "the question is gone");
        assert!(has(&harness, "Mein Lager"), "and the booking stands in the list");
    }

    /// A scan into the stock of the unit is told as such, in its own name, and its button needs the setting and the
    /// connection: without them it is blocked and says why.
    #[test]
    fn a_scan_of_the_unit_stock_needs_the_logistician_to_be_taken_back() {
        struct Pool;
        impl UnitStock for Pool {
            fn set(
                &self,
                _: &stock_scan_book::ScannedItem,
                _: &str,
                _: Option<f64>,
                _: bool,
            ) -> Result<Option<f64>, stock_scan_book::UnitError> {
                Ok(None)
            }
        }
        let dir = Dir::new("lagerscan-reiter-einheit");
        let paths = paths_in(&dir);
        language::set_thread_language(language::Choice::De);
        paths.set_setting(overlay::keys::LOGISTICIAN, versekit_core::jsonio::Value::Bool(true));
        let item = ScannedItem::new("Parallax", 3.0, Measure::Pieces, None);
        let context =
            Run {
                paths: &paths,
                target: Target::Unit,
                place: Some("Orison".into()),
                place_since: None,
                unit: Some(&Pool),
                now: 1,
            };
        stock_scan_book::run(&context, Reading::Sure(item));
        let harness = harness(open_tab(&paths));
        assert!(has(&harness, "Staffellager"), "the row names the stock of the unit");
        assert!(!has(&harness, "Mein Lager"));
        let tab = harness.state();
        let scan = &tab.journal.scans[0];
        // The setting is on, but there is no connection to the unit: blocked, and it says what is missing.
        assert_eq!(blocked_reason(tab, scan), Some(t("s_lsc_zurueck_recht")));
        paths.set_setting(overlay::keys::LOGISTICIAN, versekit_core::jsonio::Value::Bool(false));
        assert_eq!(blocked_reason(tab, scan), Some(t("s_lsc_zurueck_recht")), "and without the setting too");
        // An own scan has no such condition.
        scan_own_free(&paths);
    }

    fn scan_own_free(paths: &Paths) {
        scan(paths, Target::Own, "Scrap", 2.0, 5);
        let tab = open_tab(paths);
        let own = tab.journal.scans.iter().find(|s| s.target == Target::Own).expect("own scan");
        assert_eq!(blocked_reason(&tab, own), None);
    }
}
