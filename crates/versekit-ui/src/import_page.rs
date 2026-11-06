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

//! "Read in the stand" on the page "Backup & reset" (`pages._backup`,
//! `_show_preview`): pick a file, see what the import would do, take it over.
//!
//! The format is told from the content (`versekit_core::importer`). Nothing is
//! written before the button of the preview; what is there stays untouched, new
//! comes in. A file that is known but holds no blueprint that is owned (a wish
//! list) is said so and takes nothing over.
//!
//! The preview names the blueprints the catalog does not know; 4.12 asked its
//! preview without a catalog, so that column was always empty there.
//!
//! Reading, the preview and taking over run in a thread of their own: the
//! catalog may have to be read from its cache first.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

use egui::{RichText, Ui};
use versekit_core::collection::{self, Catalog, Collection, Known};
use versekit_core::export::{self, FileExports};
use versekit_core::importer::{self, Entry, Kind, Preview};
use versekit_core::language::{Arg, t, tf};
use versekit_core::logsource::LogSource;
use versekit_core::paths::{Host, Paths};
use versekit_core::phrases::{BackRoute, NoArchive};
use versekit_core::theme::Scheme;

use crate::widgets::{self, ButtonKind};
use crate::{color, file_dialog};

/// Names shown of the blueprints the catalog does not know.
const UNKNOWN_SHOWN: usize = 6;

/// Where the page stands.
#[derive(Clone, Debug, PartialEq)]
enum Stage {
    /// No file chosen.
    Empty,
    /// A file in no known format (or unreadable).
    Unknown,
    /// A known format without a blueprint that is owned.
    Wishlist,
    /// What the import would do.
    Preview { kind: Kind, entries: Arc<Vec<Entry>>, preview: Preview },
}

/// What a thread brings back.
enum Answer {
    Read(Stage),
    /// Blueprints taken over; `None` when the collection could not be written.
    Taken(Option<usize>),
}

/// The part of the page that reads in a stand.
pub struct ImportState {
    stage: Stage,
    dialog: Option<file_dialog::Pending>,
    working: Option<Receiver<Answer>>,
    /// The notice at the top right: text and when it came.
    notice: Option<(String, Option<f64>, &'static str)>,
}

impl Default for ImportState {
    fn default() -> Self {
        ImportState { stage: Stage::Empty, dialog: None, working: None, notice: None }
    }
}

/// The catalog the collection matches names against, as the program has it offline.
struct CacheCatalog(Option<versekit_core::catalog_cache::CatalogCache>);

impl Catalog for CacheCatalog {
    fn known(&self) -> Result<Known, String> {
        Ok(Known::new(self.0.iter().flat_map(|c| c.blueprints.iter()).map(|b| (b.key.clone(), Some(b.name.clone())))))
    }
}

/// Reads the file and works out the preview.
fn read_job(host: &Host, path: &str) -> Stage {
    let paths = Paths::new(host.clone());
    let sources = LogSource::sources_of(&paths);
    let (kind, entries) = importer::read(path, &|key| sources.resolve_key(key));
    let Some(kind) = kind else { return Stage::Unknown };
    // ⚠ Known and still empty happens without any error: wish lists (the DB's marked ones, scmdb's watched ones)
    // are not owned blueprints and take nothing over.
    if entries.is_empty() {
        return Stage::Wishlist;
    }
    let catalog = CacheCatalog(versekit_gamedata::list::list_catalog_offline(&paths));
    let names: Vec<String> = catalog.0.iter().flat_map(|c| c.blueprints.iter()).map(|b| b.name.clone()).collect();
    let back = BackRoute { paths: &paths, archive: &NoArchive };
    let exports = FileExports { paths: &paths, version: env!("CARGO_PKG_VERSION"), clock: export::now_unix };
    let collection =
        Collection { paths: &paths, catalog: &catalog, names: &back, exports: &exports, clock: collection::now_text };
    let preview = importer::preview(&entries, &collection.load(), &names);
    Stage::Preview { kind, entries: Arc::new(entries), preview }
}

/// Takes the entries over into the collection.
fn take_job(host: &Host, entries: &[Entry]) -> Option<usize> {
    let paths = Paths::new(host.clone());
    let catalog = CacheCatalog(versekit_gamedata::list::list_catalog_offline(&paths));
    let back = BackRoute { paths: &paths, archive: &NoArchive };
    let exports = FileExports { paths: &paths, version: env!("CARGO_PKG_VERSION"), clock: export::now_unix };
    let collection =
        Collection { paths: &paths, catalog: &catalog, names: &back, exports: &exports, clock: collection::now_text };
    // Read, add and write as one step: a find of the watcher or a tick of the window between them would be lost.
    let _lock = versekit_core::jsonio::lock_saves();
    let mut data = collection.load();
    let added = importer::merge(&collection, entries, &mut data, false);
    if added == 0 || collection.save(&mut data) { Some(added) } else { None }
}

fn kind_name(kind: Kind) -> String {
    match kind {
        Kind::Own => t("s_be_eigen"),
        Kind::Basetool => t("s_be_art_basetool"),
        // Both scmdb formats read the same to the player.
        Kind::ScmdbOld | Kind::ScmdbNew => "scmdb.net".to_string(),
        Kind::BlueprintDb => t("s_be_art_bpdb"),
    }
}

impl ImportState {
    fn spawn<T: Send + 'static>(&mut self, ctx: &egui::Context, work: impl FnOnce() -> T + Send + 'static)
    where
        Answer: From<T>,
    {
        let (send, answer) = mpsc::channel();
        let ctx = ctx.clone();
        let started = std::thread::Builder::new().name("bestand-einlesen".into()).spawn(move || {
            let _ = send.send(Answer::from(work()));
            ctx.request_repaint();
        });
        if started.is_ok() {
            self.working = Some(answer);
        } else {
            versekit_core::errors::record("import_page.thread", &"no thread");
        }
    }

    /// The answers that came in: the dialog, the thread.
    fn follow(&mut self, ctx: &egui::Context, host: &Host) {
        if let Some(answer) = self.dialog.as_ref().and_then(file_dialog::Pending::take) {
            self.dialog = None;
            // A cancelled dialog says nothing.
            if let Some(path) = answer {
                let host = host.clone();
                self.spawn(ctx, move || read_job(&host, &path));
            }
        }
        let Some(receiver) = &self.working else { return };
        match receiver.try_recv() {
            Ok(Answer::Read(stage)) => {
                self.stage = stage;
                self.working = None;
            }
            Ok(Answer::Taken(taken)) => {
                self.working = None;
                self.stage = Stage::Empty;
                self.notice = Some(match taken {
                    Some(n) => (tf("s_be_genommen", &[("0", Arg::Int(n as i64))]), None, "check"),
                    None => (t("s_be_nicht_geschrieben"), None, "triangle-alert"),
                });
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => self.working = None,
        }
    }

    /// Takes over what the preview showed.
    fn take(&mut self, ctx: &egui::Context, host: &Host) {
        let Stage::Preview { entries, .. } = &self.stage else { return };
        let (host, entries) = (host.clone(), entries.clone());
        self.spawn(ctx, move || take_job(&host, &entries));
    }

    /// Test seam: sets the answer of a read.
    #[cfg(test)]
    pub(crate) fn read_now(&mut self, host: &Host, path: &str) {
        self.stage = read_job(host, path);
    }

    #[cfg(test)]
    pub(crate) fn take_now(&mut self, host: &Host) -> Option<usize> {
        let Stage::Preview { entries, .. } = &self.stage else { return None };
        let taken = take_job(host, entries);
        self.stage = Stage::Empty;
        taken
    }

    #[cfg(test)]
    pub(crate) fn preview_of(&self) -> Option<(Kind, Preview)> {
        match &self.stage {
            Stage::Preview { kind, preview, .. } => Some((*kind, preview.clone())),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn says(&self) -> &'static str {
        match self.stage {
            Stage::Empty => "empty",
            Stage::Unknown => "unknown",
            Stage::Wishlist => "wishlist",
            Stage::Preview { .. } => "preview",
        }
    }
}

impl From<Stage> for Answer {
    fn from(stage: Stage) -> Answer {
        Answer::Read(stage)
    }
}

impl From<Option<usize>> for Answer {
    fn from(taken: Option<usize>) -> Answer {
        Answer::Taken(taken)
    }
}

/// The section "Read in the stand", for a card of its own: the title of the card, the button, and the preview of
/// the file below.
pub fn show(ui: &mut Ui, scheme: &Scheme, state: &mut ImportState, host: &Host) {
    state.follow(ui.ctx(), host);
    crate::settings::setting_group(ui, scheme, &t("s_be_ein"));
    ui.add(egui::Label::new(RichText::new(t("s_be_ein_h")).small().color(color(scheme.sub))).wrap());
    ui.add_space(8.0);
    let busy = state.working.is_some() || state.dialog.is_some();
    let word = t("s_be_waehlen");
    let choose = widgets::Button::new(&word, ButtonKind::Strong, scheme).icon("folder");
    if ui.add_enabled(!busy, choose).clicked() {
        let every = t("alle_dateien");
        let filters: file_dialog::Filters = &[("JSON", &["json"]), (every.as_str(), &["*"])];
        state.dialog = Some(file_dialog::open_file(ui.ctx(), &t("s_be_ein"), filters, None));
    }
    ui.add(egui::Label::new(RichText::new(t("s_be_erkannt")).small().color(color(scheme.sub))).wrap());
    ui.add_space(10.0);
    match state.stage.clone() {
        Stage::Empty => {
            ui.label(RichText::new(t("s_vorschau_leer")).strong());
            ui.add(egui::Label::new(RichText::new(t("s_vorschau_leer_h")).small().color(color(scheme.sub))).wrap());
        }
        Stage::Unknown => {
            widgets::status_box(ui, scheme, scheme.red, "circle-x", &t("s_be_unbekannt"), &t("s_be_unbekannt_h"));
        }
        Stage::Wishlist => {
            widgets::status_box(ui, scheme, scheme.gold, "triangle-alert", &t("s_be_leer"), &t("s_be_leer_h"));
        }
        Stage::Preview { kind, preview, .. } => show_preview(ui, scheme, state, host, kind, &preview),
    }
    if state.working.is_some() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
    }
    show_notice(ui, scheme, state);
}

fn show_preview(ui: &mut Ui, scheme: &Scheme, state: &mut ImportState, host: &Host, kind: Kind, v: &Preview) {
    let mut take = false;
    let mut cancel = false;
    ui.scope(|ui| {
        ui.horizontal(|row| {
            row.label(RichText::new(t("s_be_vorschau")).strong());
            row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |row| {
                row.label(RichText::new(kind_name(kind)).small().color(color(scheme.accent)));
            });
        });
        ui.add_space(6.0);
        ui.horizontal_wrapped(|row| {
            for (count, what, tone) in [
                (v.new.len(), t("s_be_dazu"), scheme.accent),
                (v.present.len(), t("s_be_schon"), scheme.fg),
                (v.unknown.len(), t("s_be_nicht_kat"), scheme.gold),
            ] {
                row.vertical(|cell| {
                    cell.label(RichText::new(count.to_string()).heading().color(color(tone)));
                    cell.label(RichText::new(what).small().color(color(scheme.sub)));
                });
                row.add_space(24.0);
            }
        });
        if !v.unknown.is_empty() {
            let shown = v.unknown.iter().take(UNKNOWN_SHOWN).cloned().collect::<Vec<_>>().join(" · ");
            let more = if v.unknown.len() > UNKNOWN_SHOWN { " …" } else { "" };
            ui.add_space(4.0);
            ui.add(
                egui::Label::new(
                    RichText::new(format!("{}{shown}{more}", t("s_be_nicht_kat_h"))).small().color(color(scheme.sub)),
                )
                .wrap(),
            );
        }
        ui.add_space(6.0);
        ui.add(egui::Label::new(RichText::new(t("s_be_merge")).small().color(color(scheme.sub))).wrap());
        ui.add_space(8.0);
        ui.horizontal_wrapped(|row| {
            let word = tf("s_be_nimm", &[("0", Arg::Int(v.new.len() as i64))]);
            let button = widgets::Button::new(&word, ButtonKind::Strong, scheme);
            take = row.add_enabled(state.working.is_none(), button).clicked();
            cancel = row.add(widgets::Button::new(&t("abbrechen"), ButtonKind::Normal, scheme)).clicked();
        });
    });
    if cancel {
        state.stage = Stage::Empty;
    } else if take {
        let ctx = ui.ctx().clone();
        state.take(&ctx, host);
    }
}

fn show_notice(ui: &Ui, scheme: &Scheme, state: &mut ImportState) {
    let Some((text, since, icon)) = &mut state.notice else { return };
    let now = ui.input(|i| i.time);
    let at = *since.get_or_insert(now);
    if !widgets::notice(ui.ctx(), scheme, at, icon, text, None).shown {
        state.notice = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use versekit_core::jsonio::Value;

    struct Bench {
        host: Host,
        dir: String,
    }

    impl Drop for Bench {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn bench(name: &str) -> Bench {
        let dir = std::env::temp_dir()
            .join(format!("versekit-bestand-{name}-{}", versekit_core::testdir::run_tag()))
            .to_string_lossy()
            .replace('\\', "/");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        let host = Host::new(
            versekit_core::pypath::Flavor::native(),
            &[(versekit_core::legacy::ENV_HOME, dir.as_str()), (versekit_core::paths::ENV_INSTALL_DIR, dir.as_str())],
            &dir,
        );
        Bench { host, dir }
    }

    impl Bench {
        fn file(&self, name: &str, text: &str) -> String {
            let path = format!("{}/{name}", self.dir);
            std::fs::write(&path, text).expect("file");
            path
        }

        fn names(&self) -> Vec<String> {
            let paths = Paths::new(self.host.clone());
            let back = BackRoute { paths: &paths, archive: &NoArchive };
            let exports = FileExports { paths: &paths, version: "5.0.0", clock: export::now_unix };
            let catalog = CacheCatalog(None);
            let c = Collection {
                paths: &paths,
                catalog: &catalog,
                names: &back,
                exports: &exports,
                clock: collection::now_text,
            };
            let mut names = collection::names(&c.load());
            names.sort();
            names
        }
    }

    const ENVELOPE: &str = r#"{"format": "basetool.blueprints", "formatVersion": "1.0", "items": [
        {"ref": {"name": "Karna Rifle"}, "acquiredAt": "2026-07-01T10:00:00Z"}, {"ref": {"name": "Bastion Helm"}}]}"#;

    #[test]
    fn a_file_is_read_shown_and_taken_over_without_touching_what_is_there() {
        let b = bench("einlesen");
        let mut state = ImportState::default();
        let path = b.file("basetool.json", ENVELOPE);
        state.read_now(&b.host, &path);
        let (kind, preview) = state.preview_of().expect("a preview");
        assert_eq!(kind, Kind::Basetool);
        assert_eq!((preview.new.len(), preview.present.len(), preview.total), (2, 0, 2));
        assert!(b.names().is_empty(), "nothing is written before the button");
        assert_eq!(state.take_now(&b.host), Some(2));
        assert_eq!(b.names(), ["Bastion Helm", "Karna Rifle"]);
        // Again: all of it is there already, so the preview says so and nothing new comes in.
        state.read_now(&b.host, &path);
        let (_, again) = state.preview_of().expect("a preview");
        assert_eq!((again.new.len(), again.present.len()), (0, 2));
        assert_eq!(state.take_now(&b.host), Some(0));
    }

    #[test]
    fn taking_over_waits_for_whoever_writes_the_collection_at_that_moment() {
        let b = bench("sperre");
        let sources = LogSource::sources_of(&Paths::new(b.host.clone()));
        let (_, entries) = importer::read(&b.file("basetool.json", ENVELOPE), &|key| sources.resolve_key(key));
        assert_eq!(entries.len(), 2);
        // Another writer holds the lock and writes a find while the import wants to start. Had the import
        // read the collection before it got the lock, that find would be gone after it.
        let guard = versekit_core::jsonio::lock_saves();
        let (host, taken) = (b.host.clone(), entries.clone());
        let worker = std::thread::spawn(move || take_job(&host, &taken));
        std::thread::sleep(std::time::Duration::from_millis(400));
        let paths = Paths::new(b.host.clone());
        let back = BackRoute { paths: &paths, archive: &NoArchive };
        let exports = FileExports { paths: &paths, version: "5.0.0", clock: export::now_unix };
        let catalog = CacheCatalog(None);
        let c = Collection { paths: &paths, catalog: &catalog, names: &back, exports: &exports, clock: collection::now_text };
        let mut data = c.load();
        assert!(c.add(&mut data, "Found Meanwhile", "log", None));
        assert!(c.save(&mut data));
        drop(guard);
        assert_eq!(worker.join().expect("worker"), Some(2));
        assert_eq!(b.names(), ["Bastion Helm", "Found Meanwhile", "Karna Rifle"]);
    }

    #[test]
    fn an_unknown_file_a_wish_list_and_a_missing_file_each_say_what_they_are() {
        let b = bench("grenzfaelle");
        let mut state = ImportState::default();
        state.read_now(&b.host, &b.file("zufall.json", r#"{"etwas": 1}"#));
        assert_eq!(state.says(), "unknown");
        state.read_now(&b.host, &b.file("kaputt.json", "{kaputt"));
        assert_eq!(state.says(), "unknown");
        state.read_now(&b.host, &format!("{}/gibt-es-nicht.json", b.dir));
        assert_eq!(state.says(), "unknown");
        // The DB with only wishes marked, scmdb with only watched ones: known, and nothing owned in them.
        let wishes = r#"{"blueprints": [{"key": "A", "isDone": false, "isMarked": true}, {"key": "B", "isDone": false}]}"#;
        state.read_now(&b.host, &b.file("wunsch.json", wishes));
        assert_eq!(state.says(), "wishlist");
        let watched = r#"{"version": 3, "blueprints": [{"tag": "t", "name": "C", "completed": false}]}"#;
        state.read_now(&b.host, &b.file("beobachtet.json", watched));
        assert_eq!(state.says(), "wishlist");
        assert!(b.names().is_empty());
    }

    /// Every stage draws, in both languages, with its own texts and the buttons named for screen readers.
    #[test]
    fn each_stage_shows_its_texts_in_both_languages() {
        use versekit_core::language::{Lang, with_language};
        let b = bench("zeigen");
        let scheme = versekit_core::theme::current();
        let unknown = b.file("x.json", r#"{"etwas": 1}"#);
        let wish = b.file("w.json", r#"{"blueprints": [{"key": "A", "isDone": false}]}"#);
        let envelope = b.file("e.json", ENVELOPE);
        // What is drawn, as lower case texts (the way the other pages are checked).
        let labels = |state: &mut ImportState| -> Vec<String> {
            fn collect(shape: &egui::Shape, out: &mut Vec<String>) {
                match shape {
                    egui::Shape::Text(text) => out.push(text.galley.text().to_lowercase()),
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out)),
                    _ => {}
                }
            }
            let ctx = egui::Context::default();
            crate::install(&ctx);
            let mut texts = Vec::new();
            for _ in 0..2 {
                texts.clear();
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 1200.0))),
                    ..Default::default()
                };
                let mut output = ctx.run_ui(input, |ui| show(ui, scheme, state, &b.host));
                output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
                output.textures_delta.clear();
            }
            texts
        };
        let has = |seen: &[String], text: String| seen.iter().any(|s| s.contains(&text.to_lowercase()));
        for language in [Lang::De, Lang::En] {
            with_language(language, || {
                let mut state = ImportState::default();
                let seen = labels(&mut state);
                assert!(has(&seen, t("s_be_waehlen")), "{seen:?}");
                assert!(has(&seen, t("s_vorschau_leer")), "{seen:?}");
                state.read_now(&b.host, &unknown);
                assert!(has(&labels(&mut state), t("s_be_unbekannt")));
                state.read_now(&b.host, &wish);
                assert!(has(&labels(&mut state), t("s_be_leer")));
                state.read_now(&b.host, &envelope);
                let seen = labels(&mut state);
                assert!(has(&seen, t("s_be_vorschau")), "{seen:?}");
                assert!(has(&seen, tf("s_be_nimm", &[("0", Arg::Int(2))])), "{seen:?}");
                assert!(has(&seen, t("abbrechen")), "{seen:?}");
                assert!(!has(&seen, "Karna".into()),"the names of new blueprints are not listed");
            });
        }
    }

    #[test]
    fn what_the_collection_holds_stays_and_only_the_new_ones_count_in_the_preview() {
        let b = bench("zusammenfuehren");
        let mut state = ImportState::default();
        let first = b.file("a.json", r#"{"werkzeug": "VerseKit", "bauplaene": [{"name": "Karna Rifle", "zeit": "2026-01-01 00:00:00"}]}"#);
        state.read_now(&b.host, &first);
        assert_eq!(state.take_now(&b.host), Some(1));
        state.read_now(&b.host, &b.file("b.json", ENVELOPE));
        let (_, preview) = state.preview_of().expect("preview");
        assert_eq!((preview.new, preview.present), (vec!["Bastion Helm".to_string()], vec!["Karna Rifle".to_string()]));
        assert_eq!(state.take_now(&b.host), Some(1));
        assert_eq!(b.names(), ["Bastion Helm", "Karna Rifle"]);
        let _ = Value::Null;
    }
}
