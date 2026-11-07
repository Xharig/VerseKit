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

//! The backup at the first start on a data folder (`versekit_core::first_backup`), the first thing a start does on
//! its data: before the settings are read for the overlay, the assistant asks whether the folder can be written,
//! the start with the system is taken over or anything else writes. When it does not work, a window says why and
//! the start goes on only once the backup is made (`versekit_ui::first_backup`); "Beenden" ends it.

use std::sync::{Arc, Mutex, PoisonError};

use versekit_core::first_backup::{self, Failure, Outcome};
use versekit_core::paths::{Host, Paths};
use versekit_core::window_signal::{self, Role};
use versekit_core::{font_size, language, theme, writeguard};
use versekit_ui::first_backup::{Answer, FailedBackup};

/// What the start does after the backup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gate {
    /// On; `written` when the backup was made right now (the player is to be told).
    Go { written: bool },
    /// The player ended the program: nothing more happens.
    Quit,
}

/// The size of the window, in points.
const SIZE: [f32; 2] = [560.0, 420.0];

/// Makes the backup of the first start for the data of the run (`data`: the copy after `--daten`; `None`: the
/// player's own), and the window when it does not work. The lock of writing of this thread is as before
/// afterwards.
pub fn gate(data: Option<&str>, version: &str) -> Gate {
    let before = writeguard::state();
    let paths = match data {
        Some(root) => {
            writeguard::deny(true);
            writeguard::allow_under(Some(root));
            Paths::for_copy(Host::current(), root)
        }
        None => Some(Paths::new(Host::current())),
    };
    let guard = writeguard::state();
    // A copy that is none (its settings lie outside) runs on nothing of its own: nothing to keep.
    let gate = paths.map_or(Gate::Go { written: false }, |paths| decide(&paths, version, guard, run_window));
    writeguard::adopt(&before);
    gate
}

/// The backup, and `window` when it does not work.
fn decide(
    paths: &Paths,
    version: &str,
    guard: writeguard::State,
    window: impl FnOnce(&Paths, &str, writeguard::State, Failure) -> Answer,
) -> Gate {
    let written = |outcome: &Outcome| Gate::Go { written: matches!(outcome, Outcome::Written { .. }) };
    match first_backup::ensure(paths, version, None) {
        Ok(outcome) => written(&outcome),
        Err(failure) => match window(paths, version, guard, failure) {
            Answer::Go(outcome) => written(&outcome),
            Answer::Quit => Gate::Quit,
        },
    }
}

struct Window {
    inner: FailedBackup,
    answer: Arc<Mutex<Option<Answer>>>,
    /// The title bar of the system, dark or light with the scheme.
    title_bar: versekit_ui::window_look::TitleBar,
}

impl eframe::App for Window {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.title_bar.follow(ui.ctx(), theme::current());
        self.inner.show(ui);
        if let Some(answer) = self.inner.answer() {
            *self.answer.lock().unwrap_or_else(PoisonError::into_inner) = Some(answer.clone());
        }
    }
}

/// The window as it opens: the class every window of the program carries.
pub(crate) fn viewport() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title(language::window_title(&language::t("erst_sich_fenster")))
        .with_app_id(versekit_core::paths::WM_CLASS)
        .with_inner_size(SIZE)
        .with_min_inner_size(SIZE)
}

/// Shows the window until the backup is made or the player ends the program. A window that cannot be shown
/// ends the program: it never goes on without the backup.
fn run_window(paths: &Paths, version: &str, guard: writeguard::State, failure: Failure) -> Answer {
    // Language and scheme from the settings, read under a lock that refuses every write of this thread.
    writeguard::deny(true);
    writeguard::allow_under(None);
    let data = paths.settings();
    theme::set_current(theme::choice_of(&data).id);
    language::set_language(language::choice_of(&data));
    let points = font_size::font_points(&data, &paths.host);
    let inner = FailedBackup::new(paths, version, guard, failure);
    let answer = Arc::new(Mutex::new(None));
    let shared = Arc::clone(&answer);
    let options = crate::main_window::native_options(viewport());
    let _ = versekit_ui::renderer::run_native(
        "versekit-erststart",
        options,
        versekit_ui::renderer::Look::Opaque,
        Box::new(move |cc| {
            versekit_ui::install(&cc.egui_ctx);
            versekit_ui::apply(&cc.egui_ctx, theme::current());
            versekit_ui::apply_size(&cc.egui_ctx, points);
            // Marks this window for a second start of the program, which brings it to the front
            // (`versekit_overlay::already_running`).
            window_signal::serve(versekit_ui::sample::system_handle(cc), Role::FirstBackup, |_| {});
            Ok(Box::new(Window { inner, answer: shared, title_bar: Default::default() }))
        }),
    );
    answer.lock().unwrap_or_else(PoisonError::into_inner).take().unwrap_or(Answer::Quit)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    /// A copy of the data as `--daten` takes it: `<root>/VerseKit-Daten` with data of the older version, and a
    /// made-up game folder in `<root>/spiel/LIVE` (a test never reads the controls of the real game).
    fn copy(name: &str) -> (versekit_core::testdir::Dir, String, std::path::PathBuf) {
        use versekit_core::jsonio::{self, Value};
        let dir = versekit_core::testdir::Dir::new(&format!("erststart-start-{name}"));
        let data = dir.path().join(versekit_core::paths::COPY_DATA_FOLDER);
        let game = dir.path().join("spiel").join("LIVE");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("Game.log"), "").unwrap();
        std::fs::create_dir_all(data.join("Bauplaene")).unwrap();
        std::fs::create_dir_all(data.join("Einstellungen")).unwrap();
        std::fs::write(data.join("Bauplaene").join("bestand.json"), "[\"Arrowhead\"]").unwrap();
        let settings = Value::Object(vec![("spiel_ordner".into(), Value::str(game.to_string_lossy()))]);
        std::fs::write(data.join("Einstellungen").join("einstellungen.json"), jsonio::dumps(&settings, Some(2), false))
            .unwrap();
        let root = dir.path().to_string_lossy().into_owned();
        (dir, root, data)
    }

    fn zips(folder: &Path) -> Vec<String> {
        std::fs::read_dir(folder)
            .map(|entries| entries.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default()
    }

    #[test]
    fn a_copy_of_the_data_is_backed_up_at_its_first_start_and_not_at_the_second() {
        let (_dir, root, data) = copy("kopie");
        assert_eq!(gate(Some(&root), "5.0.0"), Gate::Go { written: true });
        let backups = data.join(first_backup::FOLDER);
        assert_eq!(zips(&backups).len(), 1, "{:?}", zips(&backups));
        assert!(data.join("Intern").join(first_backup::MARK_FILE).is_file(), "the mark is written");
        assert_eq!(writeguard::state(), writeguard::State::default(), "the lock of the thread is as before");
        assert_eq!(gate(Some(&root), "5.0.0"), Gate::Go { written: false });
        assert_eq!(zips(&backups).len(), 1, "no second backup");
    }

    #[test]
    fn the_window_comes_only_when_the_backup_fails_and_quitting_writes_nothing() {
        let (_dir, root, data) = copy("fehler");
        writeguard::deny(true);
        writeguard::allow_under(Some(&root));
        let paths = Paths::for_copy(Host::current(), &root).expect("a copy");
        // A file where the folder of the backups should be: it cannot be made.
        std::fs::write(data.join(first_backup::FOLDER), "x").unwrap();
        let asked = std::cell::Cell::new(0);
        let quit = |_: &Paths, _: &str, _: writeguard::State, _: Failure| {
            asked.set(asked.get() + 1);
            Answer::Quit
        };
        assert_eq!(decide(&paths, "5.0.0", writeguard::state(), quit), Gate::Quit);
        assert_eq!(asked.get(), 1);
        assert!(first_backup::needed(&paths), "no mark: the next start tries again");
        assert!(!data.join("Intern").join(first_backup::MARK_FILE).exists());
        // The window made it (another folder): the start goes on.
        let elsewhere = Path::new(&root).join("anderswo").to_string_lossy().into_owned();
        let made = |paths: &Paths, version: &str, guard: writeguard::State, _: Failure| {
            writeguard::adopt(&guard);
            Answer::Go(first_backup::ensure(paths, version, Some(&elsewhere)).expect("works there"))
        };
        assert_eq!(decide(&paths, "5.0.0", writeguard::state(), made), Gate::Go { written: true });
        // Made once: no window again.
        let never = |_: &Paths, _: &str, _: writeguard::State, _: Failure| -> Answer { panic!("no window") };
        assert_eq!(decide(&paths, "5.0.0", writeguard::state(), never), Gate::Go { written: false });
        writeguard::deny(false);
        writeguard::allow_under(None);
        writeguard::take_denied();
    }

    #[test]
    fn the_window_carries_the_class_of_the_program() {
        let viewport = viewport();
        assert_eq!(viewport.app_id.as_deref(), Some("Versekit"));
        assert_eq!(viewport.inner_size, Some(egui::vec2(SIZE[0], SIZE[1])));
    }
}
