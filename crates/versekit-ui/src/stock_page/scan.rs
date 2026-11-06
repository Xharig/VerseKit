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

//! The refinery terminal read from the screen into the field of the refinery yield (`_refinery_box` of 4.12:
//! `lesen`, `auftraege_zeigen`, `lese_ergebnis`, `aus_dem_spiel`). Nothing is booked here: the lines go into the
//! field, the player compares them with the terminal and books them with the button below it. What the player
//! typed over teaches the scanner the digits of the cells it could not read for sure.
//!
//! ⚠ The button waits up to 15 seconds for Star Citizen in front and never brings it forward itself; the reading
//! runs in a thread of its own. The hotkey of the overlay reads at once and hands its reading over
//! ([`take_handed_over`]).

use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use egui::{RichText, Ui};
use versekit_core::jsonio::Value;
use versekit_core::language::{Arg, t, tf};
use versekit_core::paths::Paths;
use versekit_core::refinery_jobs;
use versekit_core::refinery_scan::digits::{self, Cell};
use versekit_core::refinery_scan::{Reason, ScannedJob, screen};
use versekit_core::theme::Scheme;
use versekit_core::writeguard;

use super::StockState;
use crate::color;
use crate::widgets::{self, ButtonKind};

/// How the message under the buttons is coloured (`lese_zeigen`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tone {
    Plain,
    Good,
    Warn,
    Bad,
}

/// What the thread of a reading tells.
enum Event {
    /// The game came to the front: now it reads.
    Reading,
    NoGame,
    Read(Vec<ScannedJob>, bool),
    Failed(Reason),
}

/// The reading of the screen of the page.
#[derive(Default)]
pub(super) struct Scan {
    /// The folders of the player for the thread; `None` on the sample contents.
    paths: Option<Paths>,
    events: Option<Receiver<Event>>,
    pub(super) reading: bool,
    pub(super) message: Option<(String, Tone)>,
    /// Several cards on the screen: one button each, the one chosen goes into the field.
    pub(super) choice: Vec<ScannedJob>,
    kept: bool,
    /// The cells of the unsure rows of the reading in the field; booking teaches the scanner from them.
    pub(super) cells: Vec<Cell>,
    /// The block is to be scrolled into view once (the tile asked for a reading).
    pub(super) scroll: bool,
}

impl Scan {
    pub(super) fn new(paths: &Paths) -> Scan {
        Scan { paths: Some(paths.working_copy()), ..Scan::default() }
    }
}

fn say(state: &mut StockState, text: String, tone: Tone) {
    state.scan.message = (!text.is_empty()).then_some((text, tone));
}

/// `lesen`: waits for the game and reads in a thread of its own; one reading at a time.
pub(super) fn start(state: &mut StockState, ctx: &egui::Context) {
    if state.scan.reading {
        return;
    }
    if !screen::supported() {
        say(state, t("s_rf_lesen_nur_windows"), Tone::Warn);
        return;
    }
    let Some(paths) = state.scan.paths.as_ref().map(Paths::working_copy) else {
        say(state, t("s_rf_lesen_fehler"), Tone::Bad);
        return;
    };
    state.scan.reading = true;
    say(state, t("s_rf_lesen_warten"), Tone::Plain);
    let (tx, rx) = mpsc::channel();
    state.scan.events = Some(rx);
    let (lock, ctx) = (writeguard::state(), ctx.clone());
    let started = std::thread::Builder::new().name("raffinerie-lesen".into()).spawn(move || {
        writeguard::adopt(&lock);
        let event =
            match screen::wait_for_game(Duration::from_secs(15), Duration::from_millis(250), &std::thread::sleep) {
                None => Event::NoGame,
                Some(rect) => {
                    let _ = tx.send(Event::Reading);
                    ctx.request_repaint();
                    match screen::read_screen(&paths, rect) {
                        Ok((jobs, kept)) => Event::Read(jobs, kept),
                        Err(error) => {
                            if error.reason != Reason::NoLanguage {
                                versekit_core::errors::record("pages.raffinerie_lesen", &error);
                            }
                            Event::Failed(error.reason)
                        }
                    }
                }
            };
        let _ = tx.send(event);
        ctx.request_repaint();
    });
    if started.is_err() {
        state.scan.reading = false;
        say(state, t("s_rf_lesen_fehler"), Tone::Bad);
    }
}

/// Takes what the thread told since the last frame.
pub(super) fn poll(state: &mut StockState) {
    let Some(events) = &state.scan.events else { return };
    let told: Vec<Event> = events.try_iter().collect();
    for event in told {
        match event {
            Event::Reading => say(state, t("s_rf_lesen_laeuft"), Tone::Plain),
            Event::NoGame => {
                state.scan.reading = false;
                say(state, t("s_rf_lesen_kein_spiel"), Tone::Warn);
            }
            Event::Read(jobs, kept) => {
                state.counted.push("lager_scan");
                show_jobs(state, jobs, kept);
            }
            Event::Failed(reason) => {
                state.scan.reading = false;
                let key = if reason == Reason::NoLanguage { "s_rf_lesen_keine_sprache" } else { "s_rf_lesen_fehler" };
                say(state, t(key), Tone::Bad);
            }
        }
    }
}

/// `auftraege_zeigen`: one job goes into the field at once; several are offered one button each.
pub(super) fn show_jobs(state: &mut StockState, jobs: Vec<ScannedJob>, kept: bool) {
    state.scan.choice.clear();
    if jobs.len() <= 1 {
        let job = jobs.into_iter().next();
        let (text, unsure, cells) = job.map_or_else(Default::default, |j| (j.text, j.unsure, j.cells));
        result(state, &text, &unsure, kept, cells);
        return;
    }
    state.scan.reading = false;
    say(state, tf("s_rf_auftraege", &[("0", Arg::Int(jobs.len() as i64))]), Tone::Warn);
    state.scan.kept = kept;
    state.scan.choice = jobs;
}

/// `lese_ergebnis`: what was read goes into the field; nothing is booked here.
fn result(state: &mut StockState, text: &str, unsure: &[String], kept: bool, cells: Vec<Cell>) {
    state.scan.reading = false;
    state.scan.cells = cells;
    let mut parts = Vec::new();
    if !text.is_empty() {
        state.refinery_text = text.to_string();
        parts.push(tf("s_rf_lesen_fertig", &[("0", Arg::Int(text.lines().count() as i64))]));
    } else if unsure.is_empty() {
        parts.push(t("s_rf_lesen_nichts"));
    }
    if !unsure.is_empty() {
        parts.push(tf("s_rf_lesen_unsicher", &[("0", Arg::from(unsure.join(", ")))]));
    }
    if kept {
        parts.push(t("s_rf_lesen_bild"));
    }
    let tone = if !text.is_empty() && unsure.is_empty() { Tone::Good } else { Tone::Warn };
    say(state, parts.join("\n"), tone);
}

/// `aus_dem_spiel`: a reading of the hotkey in the game that waits for the page, into the open block.
pub(super) fn take_handed_over(state: &mut StockState) -> bool {
    let Some((jobs, kept)) = state.scan.paths.as_ref().and_then(screen::take_reading) else { return false };
    state.refinery_open = true;
    state.counted.push("lager_scan");
    show_jobs(state, jobs, kept);
    true
}

/// The label of the button of a job (`refinery_scan.job_label`).
pub(super) fn job_label(job: &ScannedJob) -> String {
    let materials = Value::Array(job.materials.iter().map(|m| Value::str(m.clone())).collect());
    let map = vec![
        ("materials".to_string(), materials),
        ("total".to_string(), Value::str(job.total.clone())),
        ("state".to_string(), Value::str(job.state.word())),
    ];
    refinery_jobs::job_label(&map, 2)
}

/// Why the button "Bildschirm lesen" cannot be pressed now, or `None`.
pub(super) fn blocked_reason(state: &StockState) -> Option<String> {
    if state.scan.reading {
        Some(t("s_rf_lesen_laeuft"))
    } else if !screen::supported() {
        Some(t("s_rf_lesen_nur_windows"))
    } else {
        None
    }
}

/// The button "Bildschirm lesen", first in the row of the buttons; blocked with its reason while a reading runs
/// and where the screen cannot be read.
pub(super) fn read_button(ui: &mut Ui, scheme: &Scheme, state: &mut StockState) {
    let (reason, label) = (blocked_reason(state), t("s_rf_lesen"));
    let button = widgets::Button::new(&label, ButtonKind::Normal, scheme)
        .icon("scan-search")
        .blocked(reason.as_deref());
    if ui.add(button).clicked() && reason.is_none() {
        start(state, ui.ctx());
    }
}

/// The message under the row of the buttons (with the spinner while it reads) and the buttons of the jobs to
/// choose from, all as wide as the widest, numbered for a screen reader.
pub(super) fn notes(ui: &mut Ui, scheme: &Scheme, state: &mut StockState) {
    if let Some((text, tone)) = &state.scan.message {
        let tone = match tone {
            Tone::Plain => scheme.sub,
            Tone::Good => scheme.accent,
            Tone::Warn => scheme.gold,
            Tone::Bad => scheme.red,
        };
        let reading = state.scan.reading;
        ui.horizontal(|row| {
            if reading {
                row.add(egui::Spinner::new().color(color(scheme.accent)));
            }
            row.add(egui::Label::new(RichText::new(text).small().color(color(tone))).wrap());
        });
    }
    let mut chosen = None;
    for (i, job) in state.scan.choice.iter().enumerate() {
        let label = job_label(job);
        let spoken = format!("{}: {label}", i + 1);
        let button = widgets::Button::new(&label, ButtonKind::Normal, scheme).wide().wrapping().spoken(&spoken);
        if ui.add(button).clicked() {
            chosen = Some(i);
        }
    }
    if let Some(i) = chosen {
        let job = state.scan.choice.swap_remove(i);
        state.scan.choice.clear();
        let kept = state.scan.kept;
        result(state, &job.text, &job.unsure, kept, job.cells);
    }
}

/// After booking: the scanner learns the digits of the cells it could not read for sure from what the field
/// holds now. The terminal shows cSCU, goods in pieces as a count.
pub(super) fn learn_from_booking(state: &mut StockState, entries: &[(String, f64, i64)]) {
    let cells = std::mem::take(&mut state.scan.cells);
    state.scan.message = None;
    let Some(paths) = &state.scan.paths else { return };
    if cells.is_empty() {
        return;
    }
    let typed: Vec<digits::Typed> = entries
        .iter()
        .map(|(name, amount, quality)| {
            let shown = if state.data.is_piece(name) { *amount } else { amount * 100.0 };
            (name.clone(), Some(*quality as f64), Some(versekit_core::pyfloat::round(shown, 0)))
        })
        .collect();
    digits::learn(paths, &cells, &typed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use versekit_core::paths::Host;
    use versekit_core::pypath::Flavor;
    use versekit_core::refinery_scan::State;
    use versekit_core::refinery_scan::digits::{Digits, Gray};

    fn page(name: &str) -> (StockState, Paths, std::path::PathBuf) {
        language_de();
        let folder = versekit_core::testdir::fresh(name);
        let home = folder.to_string_lossy().into_owned();
        let paths = Paths::new(Host::new(Flavor::native(), &[(versekit_core::legacy::ENV_HOME, home.as_str())], &home));
        let mut state = StockState::sample_on(&paths);
        state.scan = Scan::new(&paths);
        (state, paths, folder)
    }

    fn language_de() {
        versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
    }

    fn job(text: &str, unsure: &[&str], materials: &[&str], total: &str) -> ScannedJob {
        ScannedJob {
            text: text.into(),
            unsure: unsure.iter().map(|s| s.to_string()).collect(),
            materials: materials.iter().map(|s| s.to_string()).collect(),
            total: total.into(),
            state: State::Running,
            remaining: None,
            cells: Vec::new(),
        }
    }

    #[test]
    fn one_job_goes_into_the_field_and_the_message_says_what_to_check() {
        let (mut state, _paths, folder) = page("lager-scan-einer");
        show_jobs(
            &mut state,
            vec![job("Gold 553 122 cSCU\nIron 208 1458 cSCU", &[], &["Gold", "Iron"], "12.76")],
            false,
        );
        assert_eq!(state.refinery_text, "Gold 553 122 cSCU\nIron 208 1458 cSCU");
        let (text, tone) = state.scan.message.clone().expect("message");
        assert_eq!(tone, Tone::Good);
        assert!(text.starts_with("2 Zeilen gelesen"), "{text}");
        // Unsure rows and a kept picture are said; the tone asks for a look.
        show_jobs(&mut state, vec![job("Gold ? 122 cSCU", &["Gold"], &["Gold"], "")], true);
        let (text, tone) = state.scan.message.clone().expect("message");
        assert_eq!(tone, Tone::Warn);
        assert!(text.contains("unsicher gelesen: Gold") && text.contains("aufgehoben"), "{text}");
        // Nothing read: the field keeps what it held.
        show_jobs(&mut state, Vec::new(), false);
        assert_eq!(state.refinery_text, "Gold ? 122 cSCU");
        assert_eq!(state.scan.message.as_ref().map(|m| m.0.as_str()), Some(t("s_rf_lesen_nichts").as_str()));
        assert!(state.counted.is_empty(), "showing counts nothing; the reading does");
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn several_jobs_are_offered_one_button_each_and_nothing_goes_into_the_field_before() {
        let (mut state, _paths, folder) = page("lager-scan-mehrere");
        let jobs = vec![
            job("Gold 553 122 cSCU", &[], &["Gold", "Tungsten"], "12.76"),
            job("Ouratite 553 122 cSCU", &[], &["Ouratite", "Agricium"], "7.33"),
        ];
        show_jobs(&mut state, jobs, false);
        assert!(state.refinery_text.is_empty());
        assert_eq!(state.scan.choice.len(), 2);
        assert_eq!(
            state.scan.message.as_ref().map(|m| m.0.as_str()),
            Some("2 Aufträge auf dem Bildschirm. Welchen eintragen?")
        );
        assert_eq!(job_label(&state.scan.choice[1]), "Läuft noch: Ouratite, Agricium · 7,33 cSCU");
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn the_reading_of_the_hotkey_is_taken_once_into_the_open_block() {
        let (mut state, paths, folder) = page("lager-scan-uebergabe");
        assert!(!state.refinery_open);
        assert!(!take_handed_over(&mut state), "nothing waits");
        assert!(screen::hand_over(&paths, &[job("Gold 553 122 cSCU", &[], &["Gold"], "")], false));
        assert!(take_handed_over(&mut state));
        assert!(state.refinery_open, "the block opens");
        assert_eq!(state.refinery_text, "Gold 553 122 cSCU");
        assert_eq!(state.take_counted(), ["lager_scan"]);
        assert!(!take_handed_over(&mut state), "taken once");
        let _ = std::fs::remove_dir_all(folder);
    }

    /// A cell with three bright strokes apart: three digits for the patterns.
    fn three_digits() -> Gray {
        let (width, height) = (30usize, 14usize);
        let mut gray = vec![20u8; width * height];
        for x in [3usize, 4, 13, 14, 23, 24] {
            for y in 2..12 {
                gray[y * width + x] = 230;
            }
        }
        Gray { width, height, gray }
    }

    #[test]
    fn booking_the_lines_teaches_the_scanner_the_digits_of_the_unsure_cells() {
        let (mut state, paths, folder) = page("lager-scan-lernen");
        let mut read = job("Titanium 553 ? cSCU", &["Titanium"], &["Titanium"], "");
        read.cells = vec![Cell { material: "Titanium".into(), column: 1, read: "l22".into(), crop: three_digits() }];
        show_jobs(&mut state, vec![read], false);
        // The player types the amount over: 122 cSCU.
        state.refinery_text = "Titanium 553 122 cSCU".into();
        super::super::book_refinery(&mut state);
        assert!(state.scan.cells.is_empty(), "the cells are used once");
        assert_eq!(state.scan.message, None, "the message of the reading goes");
        let learned = Digits::load(&paths);
        assert_eq!((learned.get("1").len(), learned.get("2").len()), (1, 2), "122 from 1.22 SCU back in cSCU");
        // Counter-check: booking without cells of a reading teaches nothing.
        state.refinery_text = "Titanium 553 122 cSCU".into();
        super::super::book_refinery(&mut state);
        assert_eq!(Digits::load(&paths).get("2").len(), 2);
        let _ = std::fs::remove_dir_all(folder);
    }
}
