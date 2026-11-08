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

//! The note, once per crash, that the main window ended hard the last time (`versekit_core::crash`): what
//! happened in a line, and two ways on. "Bericht ansehen" opens the page "Fehler melden", where the report with the
//! crash at its top is read and sent only by the box and a click, as every report. "Nicht senden" removes the crash,
//! so no later report carries it. Closing the note keeps the crash for a report later. Nothing is sent from here.
//!
//! Only a crash a v5 version wrote down and that is recent comes as a note (`crash::untold`): the crash file of the
//! Python program of 4.x stays in the report only.
//!
//! The note waits while the assistant, a backup or the note about the backup of the first start is open, and it
//! counts as told when it is drawn the first time.

use egui::RichText;
use versekit_core::crash;
use versekit_core::language::{self, Arg, t, tf};
use versekit_core::redact::Redactor;
use versekit_core::theme::Scheme;

use super::SamplePage;
use crate::color;
use crate::widgets::{self, ButtonKind};

/// The first line of the crash is cut to this many characters.
const FIRST_LINE: usize = 160;

impl SamplePage {
    /// Takes the note about a crash of the last run when the player was not told of it yet (the main window asks at
    /// its start); true when there is one. Names and paths of the first line are taken out, as in the report.
    pub fn open_crash_hint(&mut self) -> bool {
        let Some(store) = &self.store else { return false };
        self.crash_hint = crash::untold(&store.paths).map(|mut untold| {
            let clean = Redactor::new(&store.paths).redact(&untold.first);
            untold.first = match clean.char_indices().nth(FIRST_LINE) {
                Some((cut, _)) => format!("{}…", &clean[..cut]),
                None => clean,
            };
            untold
        });
        self.crash_told = false;
        self.crash_hint.is_some()
    }

    /// Is the note about a crash waiting or open?
    pub fn crash_hint_open(&self) -> bool {
        self.crash_hint.is_some()
    }

    /// Draws the note above the window (nothing without one, nothing while another note or question is open).
    pub(super) fn show_crash_hint(&mut self, ctx: &egui::Context, scheme: &Scheme) {
        let Some(untold) = self.crash_hint.clone() else { return };
        if self.setup_open() || self.backup_open() || self.first_backup_hint_open() {
            return;
        }
        // Told once: written down as soon as the player sees it, so that it does not come at the next start.
        if !self.crash_told {
            self.crash_told = true;
            if let Some(store) = &self.store {
                crash::mark_told(&store.paths);
            }
        }
        let when = untold
            .at
            .map(|at| versekit_core::formats::date_time(language::current(), at))
            .filter(|when| !when.is_empty() && !untold.version.is_empty())
            .map(|when| tf("abs_wann", &[("0", Arg::from(untold.version.as_str())), ("1", Arg::from(when))]));
        let ((view, drop), close) = widgets::dialog(ctx, scheme, "absturz", |ui| {
            widgets::heading(ui, scheme, &t("abs_titel"));
            ui.add(egui::Label::new(t("abs_text")).wrap());
            if let Some(when) = &when {
                ui.add(egui::Label::new(RichText::new(when).color(color(scheme.sub))).wrap());
            }
            ui.add(egui::Label::new(RichText::new(&untold.first).monospace().color(color(scheme.sub))).wrap());
            ui.add_space(6.0);
            ui.add(egui::Label::new(t("abs_bericht")).wrap());
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                let view =
                    ui.add(widgets::Button::new(&t("abs_ansehen"), ButtonKind::Strong, scheme).icon("bug")).clicked();
                let drop = ui.add(widgets::Button::new(&t("abs_nicht"), ButtonKind::Normal, scheme)).clicked();
                (view, drop)
            })
            .inner
        });
        if view {
            // The page "Fehler melden" with the report: reading and sending stay with the player there.
            self.crash_hint = None;
            self.go_to("hf_diagnose");
        } else if drop {
            self.crash_hint = None;
            if let Some(store) = &self.store {
                crash::clear_crash(&store.paths);
            }
        } else if close {
            self.crash_hint = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use egui::Pos2;
    use versekit_core::language;
    use versekit_core::paths::Paths;

    use super::super::controls::tests::Bench;
    use super::super::real_data::{folder, host_apart};
    use super::*;

    fn shows(texts: &[(String, Pos2)], part: &str) -> bool {
        texts.iter().any(|(text, _)| text.contains(part))
    }

    /// A data folder whose main window ended hard in 5.2.0, with a path of the player in the first line; the
    /// crash is set aside as at the next start.
    fn crashed(name: &str) -> (std::path::PathBuf, Paths) {
        let dir = folder(name);
        let paths = Paths::new(host_apart(&dir));
        let home = dir.to_string_lossy().into_owned();
        let dump = format!("Fatal error: panic: kaputt in {home}/geheim.json\n\nCurrent thread 'main':\n");
        std::fs::write(paths.app_file(crash::FILE), dump).unwrap();
        std::fs::write(paths.app_file(crash::VERSION_FILE), "5.2.0").unwrap();
        assert!(crash::install(&paths, "5.2.1"), "the crash set aside");
        (dir, paths)
    }

    fn page_on(paths: &Paths) -> SamplePage {
        versekit_core::theme::set_current("original");
        language::set_thread_language(language::Choice::De);
        SamplePage::with_settings(Paths::new(paths.host.clone()))
    }

    /// The note comes once: it names the crash without the path of the player, and the next start has none.
    /// Closing it keeps the crash for a report later.
    #[test]
    fn the_note_tells_the_crash_once_without_private_paths() {
        let _language = crate::language_lock();
        let (dir, paths) = crashed("absturz-hinweis");
        let mut page = page_on(&paths);
        assert!(page.open_crash_hint());
        let mut bench = Bench::new(page);
        let texts = bench.settle();
        assert!(shows(&texts, &language::t("abs_titel")), "{texts:?}");
        assert!(shows(&texts, "Fatal error: panic: kaputt"), "what happened: {texts:?}");
        assert!(shows(&texts, "5.2.0"), "the version of the crash: {texts:?}");
        let home = dir.to_string_lossy().into_owned();
        assert!(!texts.iter().any(|(text, _)| text.contains(&home)), "no path of the player: {texts:?}");
        assert!(shows(&texts, &language::t("abs_ansehen")) && shows(&texts, &language::t("abs_nicht")));
        // The next start: no note, although nothing was clicked.
        assert!(!page_on(&paths).open_crash_hint(), "once per crash");
        assert!(!crash::last_crash(&paths).is_empty(), "the crash stays for a report");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// "Bericht ansehen" opens the page "Fehler melden" and sends nothing: the report there carries the crash, and
    /// only its box and a click send it.
    #[test]
    fn viewing_the_report_opens_the_page_and_sends_nothing() {
        let _language = crate::language_lock();
        let (dir, paths) = crashed("absturz-ansehen");
        let mut page = page_on(&paths);
        // A receiver on this computer stands in for the address: nothing leaves it.
        page.use_test_receiver();
        assert!(page.open_crash_hint());
        let mut bench = Bench::new(page);
        bench.settle();
        bench.click(&language::t("abs_ansehen"));
        let texts = bench.settle();
        assert!(!bench.page.crash_hint_open(), "the note is gone");
        assert_eq!(bench.page.page_key(), "hf_diagnose", "the page with the report");
        assert!(!shows(&texts, &language::t("abs_titel")), "{texts:?}");
        let sent = bench.page.diagnose.as_ref().map_or(0, |state| state.test_receiver_count());
        assert_eq!(sent, 0, "nothing sent without the box and a click");
        assert!(!crash::last_crash(&paths).is_empty(), "the crash goes into the report");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// "Nicht senden" removes the crash: no note at the next start, and no later report carries it.
    #[test]
    fn not_sending_clears_the_crash() {
        let _language = crate::language_lock();
        let (dir, paths) = crashed("absturz-nicht");
        let mut page = page_on(&paths);
        assert!(page.open_crash_hint());
        let mut bench = Bench::new(page);
        bench.settle();
        bench.click(&language::t("abs_nicht"));
        assert!(!bench.page.crash_hint_open());
        assert!(crash::last_crash(&paths).is_empty(), "cleared");
        assert_eq!(crash::untold(&paths), None);
        assert!(!page_on(&paths).open_crash_hint());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The crash file the Python program of 4.x left behind (no version next to it) is no news after the update to
    /// v5: no note, and the report keeps it (5.2.1 told a crash of September after the update).
    #[test]
    fn an_old_crash_of_the_python_program_gives_no_note() {
        let _language = crate::language_lock();
        let dir = folder("absturz-python");
        let paths = Paths::new(host_apart(&dir));
        std::fs::write(paths.app_file(crash::FILE), "Windows fatal exception: code 0xc0000374\n").unwrap();
        assert!(crash::install(&paths, "5.2.2"));
        assert!(!page_on(&paths).open_crash_hint());
        assert!(!crash::last_crash(&paths).is_empty(), "the report keeps it");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Without a crash, or without data of its own, there is no note.
    #[test]
    fn without_a_crash_there_is_no_note() {
        let _language = crate::language_lock();
        let dir = folder("absturz-keiner");
        let paths = Paths::new(host_apart(&dir));
        assert!(crash::install(&paths, "5.2.1"));
        assert!(!page_on(&paths).open_crash_hint());
        assert!(!SamplePage::preview().open_crash_hint());
        let _ = std::fs::remove_dir_all(dir);
    }
}
