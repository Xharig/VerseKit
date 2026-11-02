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

//! The Verse-Kit 5 window with sample contents: the functions (navigation,
//! menu filter, tiles, settings, color schemes, size) can be tried without
//! own data.
//!
//! `cargo run -p versekit-ui --example vorschau`
//!
//! Without arguments it writes nothing at all: every write is locked.
//!
//! `vorschau --daten <folder>` reads and writes the settings in a copy of
//! the data: `<folder>` stands in for the user folder (`Dokumente`, the
//! pointer under Documents) and `<folder>\AppData` for the configuration
//! folder (the second pointer). If the pointers lead outside the copy (it
//! was taken on another machine), they are pointed at `<folder>\VerseKit-Daten`.
//! Writing is allowed inside `<folder>` only; if the settings file lies
//! anywhere else, nothing is written.
//!
//! `--test-empfaenger` (with `--daten`) lets the page "Fehler melden" send
//! to a receiver on this computer; without it nothing can be sent.
//!
//! The window marks itself as the main window: the button "Übersicht
//! öffnen" of the overlay brings it to the front on the overview, and
//! closing the overlay closes it too. The overlay is the way in: it starts
//! this window when its button finds none.

use versekit_core::paths::{Host, Paths};
use versekit_core::{font_size, screen, theme, writeguard};
use versekit_ui::sample::{MainSignals, SamplePage};

struct Preview {
    page: SamplePage,
    /// What the overlay asked for.
    signals: MainSignals,
}

impl eframe::App for Preview {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // File dialogs open above this window.
        versekit_ui::file_dialog::remember_parent(frame);
        self.signals.apply(ui.ctx(), &mut self.page);
        self.page.show(ui);
    }
}

/// The folder after `--daten`, if given.
fn data_folder() -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--daten" {
            return args.next();
        }
    }
    None
}

fn main() -> eframe::Result {
    let root = data_folder();
    // The window belongs to the run of its copy of the data (the overlay started on it finds it), or to the
    // samples.
    let key = root.as_deref().map_or_else(
        || versekit_core::window_signal::SAMPLE.to_string(),
        |root| versekit_core::window_signal::key_for(Some(root), None),
    );
    versekit_core::window_signal::bind(&key);
    // The whole run writes inside the copy only, or nowhere.
    writeguard::deny(true);
    writeguard::allow_under(root.as_deref());
    // The scheme is decided here, not from the user's own settings.
    theme::set_current(theme::DEFAULT);
    let paths = root.as_deref().and_then(|root| Paths::for_copy(Host::current(), root));
    if let Some(paths) = &paths {
        paths.share_saves();
    }
    let page = paths.map(SamplePage::with_settings);
    let title = match (&root, &page) {
        (None, _) => "VerseKit 5 · Vorschau (Beispielinhalte, nichts wird gespeichert)",
        (Some(_), Some(_)) => "VerseKit 5 · Vorschau (Einstellungen in der Testkopie)",
        (Some(_), None) => "VerseKit 5 · Vorschau (Testkopie nicht gefunden, nichts wird gespeichert)",
    };
    let mut page = page.unwrap_or_else(SamplePage::preview);
    // The settings file may lie on a network share: written in a thread of its own.
    page.write_in_background();
    // With own data the browser extension can send the hangar (Mein Hangar › Kopplung verwalten).
    page.start_hangar_delivery();
    // The error report is sent nowhere; with this flag only to a test receiver
    // on this computer.
    if std::env::args().any(|arg| arg == "--test-empfaenger") {
        page.use_test_receiver();
    }
    // The mission log reads the game logs in the background.
    page.start_contract_scan();
    // Once a day "the program runs" with the page counters (own data, not for a build of the
    // source). The preview runs on copies of data: it reports only to a receiver named in
    // SC_BP_NUTZUNG_ZIEL, never to the built-in one.
    if std::env::var_os(versekit_core::usage::ENV_TARGET).is_some() {
        page.start_usage_ping();
    }
    let (width, height) = font_size::BASE_MIN_SIZE;
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(title)
        .with_inner_size([width.max(1280.0), height.max(820.0)])
        .with_min_inner_size([width, height]);
    // Where the window was: size and place in points of the system scaling,
    // the place only when it still lies on a screen.
    if let Some(place) = page.window_place() {
        let opening =
            versekit_ui::sample::opening(&place, screen::system_scale(), (width, height), screen::on_a_screen);
        viewport = viewport.with_inner_size(opening.size).with_maximized(opening.maximized);
        if let Some(position) = opening.position {
            viewport = viewport.with_position(position);
        }
    }
    let mut options = eframe::NativeOptions { viewport, centered: true, ..Default::default() };
    versekit_ui::renderer::apply(&mut options);
    let language = page.start_language();
    eframe::run_native(
        "versekit-vorschau",
        options,
        Box::new(move |cc| {
            // The stored language on real data, German on the sample page; without
            // this the names from the language file follow the system language.
            versekit_core::language::set_language(language);
            versekit_ui::install(&cc.egui_ctx);
            // The overlay finds this window as the main window.
            let signals = MainSignals::serve(cc, &cc.egui_ctx);
            Ok(Box::new(Preview { page, signals }))
        }),
    )
}
