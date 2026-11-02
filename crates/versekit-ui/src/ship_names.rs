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

//! The in-game name on a ship of the hangar page (`pages._ship_names`,
//! `_asop_row(in_card=True)`): a field for the own name and a star switch,
//! stored in `asop.json` when the field is left, on Enter and on
//! the switch. Ships with the same key share the text.
//!
//! The table of the game's `global.ini` is read on its own thread and read
//! again when the file or the stored original texts change. A stored name
//! goes into the game's `global.ini` 0.9 seconds later (names typed in a row
//! are written once), on the button, and at the latest when the page state
//! goes (`pages._ship_names`, `spaeter_einspielen`).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Weak};
use std::time::{Duration, SystemTime};

use egui::{RichText, Role, Ui, WidgetInfo};
use versekit_core::asop::{self, Stored, Table};
use versekit_core::jsonio::Map;
use versekit_core::language::{Arg, t, tf};
use versekit_core::paths::{Host, Paths};
use versekit_core::theme::{Color, Scheme};

use crate::{color, widgets};

/// How often the thread looks whether the language file changed.
const LOOK_TABLE: Duration = Duration::from_secs(2);
/// Names stored within this time are written into the game once.
const WRITE_DELAY: Duration = Duration::from_millis(900);

/// Writes the names into the game: the text for the player and whether it
/// failed.
pub type Writer = Arc<dyn Fn() -> (String, bool) + Send + Sync>;

/// The writer of the program: the whole writing of the notes, which
/// carries the own names (`pages.einspielen`, `injection.refresh`).
fn game_writer(host: Host) -> Writer {
    Arc::new(move || {
        let paths = Paths::new(host.clone());
        // The names travel with the notes: while the notes are switched off,
        // writing would bring all of them back.
        if !paths.setting_bool(versekit_core::game_texts::SETTING_ON, true) {
            return (t("s_as_aus"), true);
        }
        let net = versekit_core::net::Web::new(
            versekit_core::translation::USER_AGENT,
            Duration::from_secs(versekit_core::translation::TIMEOUT_SECONDS),
            64 * 1024 * 1024,
        );
        let version = versekit_core::stock_fetch::live_build(&paths);
        let archive = crate::game_texts_page::GameArchive;
        match versekit_core::game_texts::apply(&paths, &net, &archive, &version) {
            versekit_core::injection::Outcome::Done(_) => (t("s_as_steht"), false),
            versekit_core::injection::Outcome::NoFile => (t("s_as_keine_ini"), true),
            versekit_core::injection::Outcome::NoMissions => {
                (tf("s_as_schief", &[("0", Arg::from(t("m_keine_missionen")))]), true)
            }
            versekit_core::injection::Outcome::ReadFailed(text)
            | versekit_core::injection::Outcome::WriteFailed(text) => {
                (tf("s_as_schief", &[("0", Arg::from(text))]), true)
            }
        }
    })
}

/// The writer of the program as a hook for the sync: names that came from the Basetool go into the game; a
/// failure goes into the error log.
pub fn names_hook(host: Host) -> Arc<dyn Fn() + Send + Sync> {
    let write = game_writer(host);
    Arc::new(move || {
        let (text, failed) = write();
        if failed {
            versekit_core::errors::record("basetool_sync.schiffsnamen_schreiben", &text);
        }
    })
}

/// The match of a hangar name: key and factory name, or none.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Found {
    key: String,
    factory: String,
}

/// What a name box says under the field.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Note {
    key: String,
    text: String,
    bad: bool,
}

/// The thread that reads the table, as the page sees it.
struct Watch {
    news: Receiver<Table>,
    /// Set while the page is shown; the thread only looks then. It stops
    /// once this is gone.
    wanted: Arc<AtomicBool>,
}

/// The own names and the matches of the hangar ships.
pub struct ShipNames {
    file: Option<String>,
    data: Map,
    seen: Option<SystemTime>,
    table: Option<Table>,
    /// Where the language file is looked for; the thread starts when the
    /// page is first shown.
    host: Option<Host>,
    watch: Option<Watch>,
    /// Hangar name and short name of every ship the matches belong to.
    matched_for: Option<Vec<(String, String)>>,
    found: HashMap<String, Found>,
    /// Ships per key in the whole hangar.
    per_key: HashMap<String, usize>,
    /// Renamable, all, and the names without a match (`asop::naming_summary`); `None` without a table.
    summary: Option<(usize, usize, Vec<String>)>,
    /// Text typed but not stored yet, by key.
    edits: HashMap<String, String>,
    /// Keys whose field has the cursor this frame.
    focused: HashSet<String>,
    note: Option<Note>,
    /// The order of the list as it was last set (`reorder`), by hangar name.
    ranks: HashMap<String, (u8, String)>,
    /// Writes the names into the game; `None` without a data folder.
    writer: Option<Writer>,
    /// When the stored names are due to be written.
    write_due: Option<std::time::Instant>,
    /// The writing going on, and its answer.
    writing: Option<Receiver<(String, bool)>>,
    /// What the last writing said.
    written: Option<(String, bool)>,
}

fn modified(file: &str) -> Option<SystemTime> {
    std::fs::metadata(file).and_then(|m| m.modified()).ok()
}

impl Default for ShipNames {
    fn default() -> Self {
        ShipNames {
            file: None,
            data: asop::empty(),
            seen: None,
            table: None,
            host: None,
            watch: None,
            matched_for: None,
            found: HashMap::new(),
            per_key: HashMap::new(),
            summary: None,
            edits: HashMap::new(),
            focused: HashSet::new(),
            note: None,
            ranks: HashMap::new(),
            writer: None,
            write_due: None,
            writing: None,
            written: None,
        }
    }
}

impl ShipNames {
    /// Without own data: a fixed table, changes stay in the window.
    pub fn detached(table: Table) -> ShipNames {
        let mut names = ShipNames::default();
        names.table = Some(table);
        names
    }

    /// On a file with a fixed table, without the thread.
    #[cfg(test)]
    pub(crate) fn on_file(file: &str, table: Table) -> ShipNames {
        let mut names = ShipNames::on(file);
        names.table = Some(table);
        names
    }

    fn on(file: &str) -> ShipNames {
        let mut names = ShipNames::default();
        names.data = asop::load(file);
        names.seen = modified(file);
        names.file = Some(file.to_string());
        names
    }

    #[cfg(test)]
    pub(crate) fn set_data_for_test(&mut self, data: Map) {
        self.data = data;
    }

    /// With own data: `asop.json` of the folder and the table of the
    /// game's language file, read on its own thread while the page is shown.
    pub fn with_paths(paths: &Paths) -> ShipNames {
        let mut names = ShipNames::on(&paths.app_file(asop::FILE));
        names.host = Some(paths.host.clone());
        names.writer = Some(game_writer(paths.host.clone()));
        names
    }

    /// With another writer (tests).
    #[cfg(test)]
    pub(crate) fn set_writer(&mut self, writer: Writer) {
        self.writer = Some(writer);
    }

    /// Whether the names can go into the game from here.
    pub fn can_write(&self) -> bool {
        self.writer.is_some()
    }

    /// Write the names into the game now, on a thread of its own.
    pub fn write_now(&mut self) {
        self.write_due = None;
        let Some(writer) = self.writer.clone() else { return };
        if self.writing.is_some() {
            self.write_due = Some(std::time::Instant::now() + WRITE_DELAY);
            return;
        }
        let (send, answer) = mpsc::channel();
        let lock = versekit_core::writeguard::state();
        let started = std::thread::Builder::new()
            .name("schiffsnamen-schreiben".into())
            .spawn(move || {
                versekit_core::writeguard::adopt(&lock);
                let _ = send.send(writer());
            })
            .is_ok();
        if started {
            self.writing = Some(answer);
            self.written = Some((t("s_as_laeuft"), false));
        }
    }

    /// Whether a writing runs or waits.
    pub fn writing(&self) -> bool {
        self.writing.is_some() || self.write_due.is_some()
    }

    /// What the last writing said.
    pub fn written(&self) -> Option<&(String, bool)> {
        self.written.as_ref()
    }

    /// Start a writing that is due, take the answer of one that ended.
    fn follow_writing(&mut self) {
        if let Some(answer) = &self.writing {
            match answer.try_recv() {
                Ok(said) => {
                    self.written = Some(said);
                    self.writing = None;
                }
                // The writing broke off without a word: say so, and let the
                // next name write again.
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.written = Some((tf("s_as_schief", &[("0", Arg::from(t("inj_abgebrochen")))]), true));
                    self.writing = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.write_due.is_some_and(|due| std::time::Instant::now() >= due) && self.writing.is_none() {
            self.write_now();
        }
    }

    /// Take a new table; read `asop.json` again when another place wrote
    /// it. Called while the page is shown.
    pub fn follow(&mut self) {
        self.follow_writing();
        if self.watch.is_none()
            && let Some(host) = self.host.take()
        {
            let wanted = Arc::new(AtomicBool::new(true));
            let (send, news) = mpsc::channel();
            let flag = Arc::downgrade(&wanted);
            let started = std::thread::Builder::new()
                .name("schiffsnamen".into())
                .spawn(move || watch_table(host, flag, send))
                .is_ok();
            self.watch = started.then_some(Watch { news, wanted });
        }
        if let Some(watch) = &self.watch {
            watch.wanted.store(true, Ordering::Relaxed);
            let mut latest = None;
            while let Ok(table) = watch.news.try_recv() {
                latest = Some(table);
            }
            if let Some(table) = latest {
                self.table = Some(table);
                self.matched_for = None;
            }
        }
        if let Some(file) = &self.file {
            let now = modified(file);
            if now != self.seen {
                self.data = asop::load(file);
                self.seen = now;
                self.ranks.clear();
            }
        }
    }

    /// Sort the list anew at the next frame, with the names as they are
    /// then. Own changes on the page do not move a ship until this.
    pub fn reorder(&mut self) {
        self.ranks.clear();
    }

    /// Match the hangar ships when the table or the ships changed.
    pub fn prepare(&mut self, ships: &[&Map]) {
        let names: Vec<(String, String)> = ships
            .iter()
            .map(|s| {
                let text = |k: &str| versekit_core::hangar::field_text(s, k).to_string();
                (text("name"), text("kurz"))
            })
            .collect();
        if self.matched_for.as_ref() == Some(&names) {
            return;
        }
        self.found.clear();
        self.per_key.clear();
        self.ranks.clear();
        self.summary = None;
        if let Some(table) = self.table.as_ref().filter(|t| !t.is_empty()) {
            let matches = asop::match_ships(ships, table);
            self.summary = Some(asop::naming_summary(&matches));
            for m in matches {
                self.found.insert(m.name.clone(), Found { key: m.key, factory: m.factory });
            }
            for (name, _) in &names {
                if let Some(found) = self.found.get(versekit_core::text::py_strip(name))
                    && !found.key.is_empty()
                {
                    *self.per_key.entry(found.key.clone()).or_default() += 1;
                }
            }
        }
        self.matched_for = Some(names);
    }

    /// Under the foot of the names: how many ships can be renamed, and which have no match. Only with the
    /// table of the language file.
    pub fn footer(&self, ui: &mut Ui, scheme: &Scheme) {
        if self.table.as_ref().is_none_or(Table::is_empty) {
            return;
        }
        let Some((renamable, all, without)) = &self.summary else { return };
        small(ui, &tf("s_as_stand", &[("0", Arg::from(*renamable)), ("1", Arg::from(*all))]), scheme.sub);
        if !without.is_empty() {
            small(ui, &tf("s_as_ohne", &[("0", Arg::from(without.join(", ")))]), scheme.sub);
        }
    }

    /// The table was read and holds nothing: no language file.
    pub fn without_language_file(&self) -> bool {
        self.table.as_ref().is_some_and(Table::is_empty)
    }

    fn found(&self, hangar_name: &str) -> Option<&Found> {
        self.found.get(versekit_core::text::py_strip(hangar_name))
    }

    /// The own name of a ship, for the search.
    pub fn own_name(&self, hangar_name: &str) -> String {
        match self.found(hangar_name).filter(|f| !f.key.is_empty()) {
            Some(found) => asop::entry(&self.data, &found.key).0,
            None => String::new(),
        }
    }

    /// Order as at the terminal (`_hangar_order`): 0 with the star, 1 with
    /// an own name, 2 without either; and the own name. Kept until
    /// `reorder`.
    pub fn rank(&mut self, hangar_name: &str) -> (u8, String) {
        if let Some(rank) = self.ranks.get(hangar_name) {
            return rank.clone();
        }
        let rank = match self.found(hangar_name).filter(|f| !f.key.is_empty()) {
            Some(found) => {
                let (name, star) = asop::entry(&self.data, &found.key);
                let level = if star {
                    0
                } else if name.is_empty() {
                    2
                } else {
                    1
                };
                (level, name)
            }
            None => (2, String::new()),
        };
        self.ranks.insert(hangar_name.to_string(), rank.clone());
        rank
    }

    /// Store a name and say how it went, under the field of that key. When
    /// writing fails, the name stays on the page.
    fn store(&mut self, key: &str, text: &str, star: bool) {
        self.edits.remove(key);
        let Some(file) = &self.file else {
            asop::set_name(&mut self.data, key, text, star);
            return;
        };
        let (done, data) = asop::store(file, key, text, star);
        match data {
            Some(data) => self.data = data,
            None => asop::set_name(&mut self.data, key, text, star),
        }
        self.seen = modified(file);
        let (text, bad) = match done {
            Stored::Unchanged => return,
            Stored::Written if self.writer.is_some() => {
                self.write_due = Some(std::time::Instant::now() + WRITE_DELAY);
                (t("s_as_gemerkt"), false)
            }
            Stored::Written => (t("s_hgn_gemerkt"), false),
            Stored::SetAside => (t("s_hgn_beiseite"), true),
            Stored::Failed => (t("s_as_nicht_gespeichert"), true),
        };
        self.note = Some(Note { key: key.to_string(), text, bad });
    }

    /// After the list: store every typed name whose field lost the cursor.
    pub fn finish_frame(&mut self) {
        self.store_left(false);
        self.focused.clear();
    }

    /// Store typed names: those without the cursor, or with `all` every one.
    fn store_left(&mut self, all: bool) {
        let left: Vec<(String, String)> = self
            .edits
            .iter()
            .filter(|(k, _)| all || !self.focused.contains(*k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (key, text) in left {
            let star = asop::entry(&self.data, &key).1;
            self.store(&key, &text, star);
        }
    }

    /// The in-game name box in the row of a ship. Nothing without a
    /// table; the hint when the ship is not in the language file. `salt`
    /// tells the field of this ship from the others.
    pub fn card(&mut self, ui: &mut Ui, scheme: &Scheme, hangar_name: &str, salt: &str) {
        if self.table.as_ref().is_none_or(Table::is_empty) {
            return;
        }
        let found = self.found(hangar_name).cloned().unwrap_or_default();
        // An inner area of the ship card: another shade and a thin border, so
        // it reads as part of the card and not as the head of the next one.
        egui::Frame::new()
            .fill(color(scheme.bar))
            .stroke(egui::Stroke::new(1.0, color(scheme.line)))
            .corner_radius(crate::control_radius(scheme))
            .inner_margin(egui::Margin::symmetric(8, 6))
            .show(ui, |b| {
                b.set_width(b.available_width());
                let label = b.label(RichText::new(t("s_hg_name_feld")).small().color(color(scheme.sub)));
                if found.key.is_empty() {
                    small(b, &t("s_as_zeile_ohne"), scheme.sub);
                    return;
                }
                self.field_line(b, scheme, &found.key, salt, label.id);
                let count = self.per_key.get(&found.key).copied().unwrap_or(1);
                if count > 1 {
                    let ship = if found.factory.is_empty() { hangar_name } else { found.factory.as_str() };
                    let text = tf("s_hgn_alle", &[("n", Arg::from(count)), ("schiff", Arg::from(ship))]);
                    small(b, &text, scheme.gold);
                }
            });
        // The answer to a stored name stands under the area, on the card itself:
        // red text does not reach the contrast on the area's shade.
        if !found.key.is_empty()
            && let Some(note) = self.note.as_ref().filter(|n| n.key == found.key)
        {
            small(ui, &note.text, if note.bad { scheme.red } else { scheme.sub });
        }
    }

    /// The field and the star switch to its right.
    fn field_line(&mut self, ui: &mut Ui, scheme: &Scheme, key: &str, salt: &str, label: egui::Id) {
        let (stored, mut star) = asop::entry(&self.data, key);
        let mut text = self.edits.get(key).cloned().unwrap_or_else(|| stored.clone());
        let mut toggled = false;
        ui.horizontal(|line| {
            line.with_layout(egui::Layout::right_to_left(egui::Align::Center), |line| {
                let name = t("s_as_stern");
                let switch = line.add(widgets::Toggle::new(&mut star, scheme));
                let (enabled, on) = (switch.enabled(), star);
                switch.widget_info(|| WidgetInfo::selected(Role::CheckBox, enabled, on, &name));
                toggled = switch.changed();
                line.label(RichText::new(&name).small());
                let width = line.available_width();
                let edit = widgets::text_field(&mut text, &t("s_hgn_platz"), scheme)
                    .id_salt(("hangar-name", salt))
                    .char_limit(asop::MAX_LENGTH)
                    .desired_width(width);
                let field = line.add(edit).labelled_by(label);
                if field.changed() {
                    self.edits.insert(key.to_string(), text.clone());
                }
                if field.has_focus() {
                    self.focused.insert(key.to_string());
                }
            });
        });
        if toggled {
            self.store(key, &text, star);
        }
    }
}

impl ShipNames {
    /// A name typed but not stored yet is stored, and a writing that waits is done now, in this thread.
    pub(crate) fn write_left(&mut self) {
        self.store_left(true);
        if self.write_due.take().is_some()
            && let Some(writer) = self.writer.clone()
        {
            writer();
        }
    }
}

/// A name typed but not stored yet (page left, window closed) is stored
/// when the page state goes, and a writing that waits is done now.
impl Drop for ShipNames {
    fn drop(&mut self) {
        self.write_left();
    }
}

fn small(ui: &mut Ui, text: &str, tone: Color) {
    ui.add(egui::Label::new(RichText::new(text).small().color(color(tone))).wrap());
}

/// The original names stored again from the game's `Data.p4k` when none are
/// stored or the archive changed (`game_text::names_or_fetch`).
fn refresh_names(paths: &Paths) {
    versekit_gamedata::game_text::names_or_fetch(paths, None);
}

/// The table of factory names, as 4.12 reads it: the original names are
/// `refresh`ed first, since without an English file next to the German one
/// the added ship names come from them.
fn fresh_table(paths: &Paths, refresh: &dyn Fn(&Paths)) -> Table {
    refresh(paths);
    asop::factory_table(paths)
}

/// The thread behind the table: reads it at the start and, while the page
/// is shown, whenever its sources changed; ends when the page state is gone.
fn watch_table(host: Host, wanted: Weak<AtomicBool>, send: mpsc::Sender<Table>) {
    let paths = Paths::new(host);
    let mut last = None;
    while let Some(flag) = wanted.upgrade() {
        if flag.swap(false, Ordering::Relaxed) {
            let stamp = asop::table_stamp(&paths);
            if last.as_ref() != Some(&stamp) {
                if send.send(fresh_table(&paths, &refresh_names)).is_err() {
                    return;
                }
                last = Some(stamp);
            }
        }
        drop(flag);
        std::thread::sleep(LOOK_TABLE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// While the notes are switched off, the names do not bring them back.
    #[test]
    fn switched_off_notes_stay_out_of_the_game() {
        let root = std::env::temp_dir().join(format!("versekit-shipnames-off-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&root);
        let root = root.to_string_lossy().into_owned();
        let live = format!("{root}/StarCitizen/LIVE");
        let folder = format!("{live}/data/Localization/german_(germany)");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::create_dir_all(format!("{root}/data")).unwrap();
        std::fs::write(format!("{live}/Game.log"), "").unwrap();
        let ini = format!("{folder}/global.ini");
        std::fs::write(&ini, "Bounty_title_001=Kopfgeld\n").unwrap();
        let data = format!("{root}/data");
        let host = Host::new(
            versekit_core::pypath::Flavor::Posix,
            &[("HOME", root.as_str()), ("SC_BP_HOME", data.as_str()), ("SC_INSTALL_DIR", live.as_str())],
            &root,
        );
        let paths = Paths::new(host.clone());
        paths.set_setting(versekit_core::game_texts::SETTING_ON, versekit_core::jsonio::Value::Bool(false));
        assert_eq!(game_writer(host)(), (t("s_as_aus"), true));
        assert_eq!(std::fs::read_to_string(&ini).unwrap(), "Bounty_title_001=Kopfgeld\n");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Without an English file next to the German one the ships the German
    /// file lacks come from the original names; the table fetches them before
    /// it reads, as 4.12 does.
    #[test]
    fn the_table_refreshes_the_original_names_before_it_reads() {
        let root = std::env::temp_dir().join(format!("versekit-shipnames-names-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&root);
        let root = root.to_string_lossy().into_owned();
        let live = format!("{root}/StarCitizen/LIVE");
        let folder = format!("{live}/data/Localization/german_(germany)");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::create_dir_all(format!("{root}/data")).unwrap();
        std::fs::write(format!("{live}/Game.log"), "").unwrap();
        std::fs::write(format!("{folder}/global.ini"), "vehicle_NameAEGS_Gladius=Gladius\n").unwrap();
        let data = format!("{root}/data");
        let host = Host::new(
            versekit_core::pypath::Flavor::Posix,
            &[("HOME", root.as_str()), ("SC_BP_HOME", data.as_str()), ("SC_INSTALL_DIR", live.as_str())],
            &root,
        );
        let paths = Paths::new(host);
        let names = paths.app_file(asop::NAMES_FILE);
        let fetched = |p: &Paths| {
            let file = p.app_file(asop::NAMES_FILE);
            std::fs::write(
                file,
                r#"{"vehicle_NameAEGS_Gladius": "Gladius", "vehicle_NameRSI_Ursa_Rover": "Ursa Rover"}"#,
            )
            .unwrap();
        };
        let table = fresh_table(&paths, &fetched);
        assert!(std::path::Path::new(&names).is_file(), "the names were stored first");
        assert_eq!(table.get("vehicle_NameRSI_Ursa_Rover").map(String::as_str), Some("Ursa Rover"), "{table:?}");
        assert_eq!(table.get("vehicle_NameAEGS_Gladius").map(String::as_str), Some("Gladius"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
