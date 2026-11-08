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

//! The note, once, that the data of the older version were backed up at the first start
//! (`versekit_core::first_backup`): where the file lies, and a button that opens its folder. It waits while the
//! assistant or a backup is open, and it counts as told when it is drawn the first time.

use versekit_core::first_backup;
use versekit_core::language::{Arg, t, tf};
use versekit_core::theme::Scheme;

use super::SamplePage;
use crate::widgets::{self, ButtonKind};

impl SamplePage {
    /// Takes the note about the backup of the first start when the player was not told yet (the main window
    /// asks at its start); true when there is one.
    pub fn open_first_backup_hint(&mut self) -> bool {
        let Some(store) = &self.store else { return false };
        self.first_backup_hint = first_backup::hint(&store.paths);
        self.first_backup_hint.is_some()
    }

    /// Is the note about the backup of the first start waiting or open?
    pub fn first_backup_hint_open(&self) -> bool {
        self.first_backup_hint.is_some()
    }

    /// Draws the note above the window (nothing without one, nothing while the assistant or a backup is open).
    pub(super) fn show_first_backup_hint(&mut self, ctx: &egui::Context, scheme: &Scheme) {
        let Some(file) = self.first_backup_hint.clone() else { return };
        if self.setup_open() || self.backup_open() {
            return;
        }
        // Told once: written down as soon as the player sees it, so that it does not come at the next start.
        if !self.first_backup_told {
            self.first_backup_told = true;
            if let Some(store) = &self.store {
                first_backup::hint_shown(&store.paths);
            }
        }
        let ((open, done), close) = widgets::dialog(ctx, scheme, "erststart-sicherung", |ui| {
            widgets::heading(ui, scheme, &t("erst_sich_hinweis_titel"));
            ui.add(egui::Label::new(tf("erst_sich_hinweis", &[("0", Arg::from(file.as_str()))])).wrap());
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let open = ui
                    .add(widgets::Button::new(&t("erst_sich_ordner"), ButtonKind::Normal, scheme).icon("folder"))
                    .clicked();
                let done = ui.add(widgets::Button::new(&t("hf_schliessen"), ButtonKind::Strong, scheme)).clicked();
                (open, done)
            })
            .inner
        });
        if open {
            let folder = std::path::Path::new(&file).parent().map(|p| p.to_string_lossy().into_owned());
            if !folder.is_some_and(|folder| self.open_folder(&folder)) {
                self.pin_fault = Some(t("erst_sich_ordner_fehlt"));
                self.pin_notice = Some((ctx.input(|i| i.time), t("erst_sich_ordner_fehlt")));
            }
        }
        if done || close {
            self.first_backup_hint = None;
        }
    }

    /// Opens a folder in the file manager; a test only notes it.
    fn open_folder(&mut self, folder: &str) -> bool {
        #[cfg(test)]
        {
            self.folders_opened.push(folder.to_string());
            std::path::Path::new(folder).is_dir()
        }
        #[cfg(not(test))]
        {
            versekit_core::browser::show_folder(folder)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use egui::Pos2;
    use versekit_core::first_backup::Outcome;
    use versekit_core::language;
    use versekit_core::paths::Paths;

    use super::super::controls::tests::Bench;
    use super::super::real_data::{folder, host_apart};
    use super::*;

    fn shows(texts: &[(String, Pos2)], part: &str) -> bool {
        texts.iter().any(|(text, _)| text.contains(part))
    }

    /// A data folder of the older version in a throwaway folder, backed up at the first start; the page on it and
    /// the file of the backup.
    fn backed_up(name: &str) -> (std::path::PathBuf, String) {
        let dir = folder(name);
        std::fs::write(dir.join("bestand.json"), "[1]").unwrap();
        let paths = Paths::new(host_apart(&dir));
        let Ok(Outcome::Written { file, .. }) = first_backup::ensure(&paths, "5.0.0", None) else {
            panic!("the backup of the first start");
        };
        (dir, file)
    }

    fn page_on(dir: &Path) -> SamplePage {
        versekit_core::theme::set_current("original");
        language::set_thread_language(language::Choice::De);
        SamplePage::with_settings(Paths::new(host_apart(dir)))
    }

    #[test]
    fn the_note_names_the_file_once_and_the_next_start_has_none() {
        let _language = crate::language_lock();
        let (dir, file) = backed_up("erststart-hinweis");
        let mut page = page_on(&dir);
        assert!(page.open_first_backup_hint());
        let mut bench = Bench::new(page);
        let texts = bench.settle();
        assert!(shows(&texts, &language::t("erst_sich_hinweis_titel")), "{texts:?}");
        assert!(shows(&texts, &file), "the path is named: {texts:?}");
        assert!(shows(&texts, &language::t("erst_sich_ordner")));
        bench.click(&language::t("erst_sich_ordner"));
        let parent = Path::new(&file).parent().unwrap().to_string_lossy().into_owned();
        assert_eq!(bench.page.folders_opened, [parent], "the folder of the file");
        assert!(bench.page.first_backup_hint_open(), "opening the folder keeps the note");
        bench.click(&language::t("hf_schliessen"));
        assert!(!bench.page.first_backup_hint_open());
        // The next start of the window: no note.
        let mut again = page_on(&dir);
        assert!(!again.open_first_backup_hint(), "once");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_note_waits_for_the_assistant_and_is_not_used_up_meanwhile() {
        let _language = crate::language_lock();
        let (dir, _file) = backed_up("erststart-hinweis-warten");
        let mut page = page_on(&dir);
        page.open_setup();
        assert!(page.open_first_backup_hint());
        let mut bench = Bench::new(page);
        let texts = bench.settle();
        assert!(!shows(&texts, &language::t("erst_sich_hinweis_titel")), "not above the assistant");
        // Not told yet: a window closed now brings it at the next start.
        assert!(page_on(&dir).open_first_backup_hint());
        bench.page.setup = None;
        let texts = bench.settle();
        assert!(shows(&texts, &language::t("erst_sich_hinweis_titel")), "after the assistant: {texts:?}");
        assert!(!page_on(&dir).open_first_backup_hint(), "now it is told");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_data_folder_without_a_backup_has_no_note() {
        let _language = crate::language_lock();
        let dir = folder("erststart-hinweis-leer");
        // The game folder outside the data folder, which stays empty.
        let nowhere = std::env::temp_dir().join("versekit-erststart-kein-spiel").to_string_lossy().into_owned();
        let host = super::super::real_data::host_in(&dir, &[(versekit_core::paths::ENV_INSTALL_DIR, &nowhere)]);
        let paths = Paths::new(host);
        assert_eq!(first_backup::ensure(&paths, "5.0.0", None), Ok(Outcome::Nothing));
        assert!(!page_on(&dir).open_first_backup_hint());
        // Nor a page without data of its own.
        assert!(!SamplePage::preview().open_first_backup_hint());
        let _ = std::fs::remove_dir_all(dir);
    }
}
