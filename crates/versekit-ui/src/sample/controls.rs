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

//! What the buttons of the window do beyond drawing: the links of the sidebar,
//! the folder dialogs of the game folders, the restart, the count of the errors
//! and the mark of what is new. Everything that reaches outside the window (the
//! browser, a dialog, a new process) goes through a function the page holds, so
//! a test can put its own in its place and see what was asked for.

use versekit_core::language::SpawnWithLanguage as _;
use versekit_core::jsonio::{self, Value};
use versekit_core::language::{Arg, t, tf};
use versekit_core::paths::{Host, Paths};

use super::SamplePage;
use crate::close_writes::WritesOnClose;
use crate::file_dialog::Pending;

/// Where the Discord button leads.
pub(super) const DISCORD_URL: &str = "https://xharig.com/discord";
/// Where the donate button leads.
pub(super) const KOFI_URL: &str = "https://ko-fi.com/xharig";

/// Pages that are no more in the menu and the page that took over: the hangar delivery settings
/// sit in the fold "Kopplung verwalten" of Mein Hangar, the wishlist is the section "Planned" of it, the
/// dismantle calculation is a view of the page "Dismantle & Salvage", and the evaluation of the statistics is
/// part of their overview, the relations are a tab of the handle search, and the shader cache is the last card of
/// the graphics page.
const REMOVED_PAGES: [(&str, &str); 6] = [
    ("hf_beziehungen", "hf_handlesuche"),
    ("hf_hangar_einlieferung", "hf_hangar"),
    ("hf_wunschliste", "hf_hangar"),
    ("hf_zerlegen", "hf_bergung"),
    ("hf_st_auswertung", "hf_st_uebersicht"),
    ("hf_shader", "hf_grafik"),
];

/// The file of what the user has seen, in the data folder, as 4.12 keeps it.
const SEEN_FILE: &str = "gesehen.json";
/// Which area came with which version (the names as `gesehen.json` keeps them, 4.12 `NEW_SINCE`). The page of an
/// area is `hf_<area>`; an area without such a page carries no mark. A new area: one line here.
pub(super) const NEW_SINCE: [(&str, &str); 28] = [
    ("statistik", "3.58.0"),
    ("darstellung", "3.58.0"),
    ("basetool", "3.61.0"),
    ("meinlager", "4.10.0"),
    ("staffellager", "4.8.0"),
    ("module", "3.58.0"),
    ("startprogramme", "3.58.0"),
    ("uebersetzung", "3.59.0"),
    ("shader", "3.97.0"),
    ("grafik", "3.97.0"),
    ("uebersicht", "3.98.0"),
    ("patchaenderungen", "3.24.0"),
    ("hangar", "3.78.0"),
    ("verkauf", "3.92.0"),
    ("wunschliste", "3.19.0"),
    ("bergung", "3.19.0"),
    ("auftragslog", "3.12.0"),
    ("herstellung", "3.3.0"),
    ("bergbau", "3.3.0"),
    ("liste", "3.0.0"),
    ("fortschritt", "3.0.0"),
    ("bestand", "3.0.0"),
    ("wasistneu", "3.0.0"),
    ("ueber", "3.0.0"),
    ("diagnose", "3.0.0"),
    ("serverstatus", "3.0.0"),
    ("danke", "3.0.0"),
    ("farmliste", "5.0.0"),
];

/// "2.2.0" as (2, 2, 0); what cannot be read counts as 0 (4.12 `_parts`).
fn version_parts(version: &str) -> (u64, u64, u64) {
    let mut numbers = version.split('-').next().unwrap_or("").split('.').map(|p| p.trim().parse::<u64>().unwrap_or(0));
    (numbers.next().unwrap_or(0), numbers.next().unwrap_or(0), numbers.next().unwrap_or(0))
}

/// The page of an area, where the menu has one.
fn page_of_area(area: &str) -> Option<&'static str> {
    super::NAV
        .iter()
        .flat_map(|(_, pages)| pages.iter())
        .map(|(page, _)| *page)
        .find(|page| page.strip_prefix("hf_") == Some(area))
}

/// The pages that carry the mark now: an area that came with a version up to the own one and was not seen since
/// (4.12 `is_new`). `seen` gives the version an area was last seen with.
pub(super) fn new_pages(own: &str, seen: impl Fn(&str) -> Option<String>) -> Vec<&'static str> {
    NEW_SINCE
        .iter()
        .filter(|(_, since)| version_parts(since) <= version_parts(own))
        .filter(|(area, since)| seen(area).is_none_or(|was| version_parts(since) > version_parts(&was)))
        .filter_map(|(area, _)| page_of_area(area))
        .collect()
}

/// The areas of the newest version up to the own one: what the sample page marks, what the tool of the
/// developers brings back.
fn newest_areas(own: &str) -> Vec<&'static str> {
    let up_to: Vec<&(&str, &str)> =
        NEW_SINCE.iter().filter(|(_, since)| version_parts(since) <= version_parts(own)).collect();
    let newest = up_to.iter().map(|(_, since)| version_parts(since)).max();
    up_to.iter().filter(|(_, since)| Some(version_parts(since)) == newest).map(|(area, _)| *area).collect()
}

/// The pages the sample page marks: those of the newest version.
pub(super) fn sample_new_pages() -> Vec<&'static str> {
    newest_areas(env!("CARGO_PKG_VERSION")).into_iter().filter_map(page_of_area).collect()
}
/// How long a restart waits for the settings thread to write what is still on its way.
const WRITE_WAIT: std::time::Duration = std::time::Duration::from_secs(3);
/// How often the error log is looked at for its count, in seconds.
const ERRORS_LOOK_SECONDS: f64 = 2.0;

/// Starts this program again with these arguments.
pub(super) fn restart_process(args: &[String]) -> std::io::Result<()> {
    std::process::Command::new(std::env::current_exe()?).args(args).spawn().map(|_| ())
}

/// A folder dialog that does not exist: the game folder cannot be chosen.
pub(super) type ChooseFolder = fn(&egui::Context, &str, Option<&str>) -> Pending;

impl SamplePage {
    /// A link of the sidebar: says that the browser opens, then opens `url`; when nothing
    /// could open it, says so with the address.
    pub(super) fn follow_link(&mut self, now: f64, url: &str, opening_key: &str) {
        let text = if (self.open_url)(url) { t(opening_key) } else { tf("s_ub_auf_nein", &[("0", Arg::from(url))]) };
        self.pin_notice = Some((now, text));
    }

    /// A page that was taken out of the menu and may still be named (a stored last page, a pinned
    /// tile): the page that took over opens instead, and the setting that was on the old page
    /// opens with it.
    pub(super) fn follow_removed_page(&mut self, now: f64) {
        if let Some((old, to)) = REMOVED_PAGES.iter().find(|(old, _)| *old == self.page) {
            self.page = (*to).to_string();
            if *old == "hf_shader" {
                // The card of the shader cache is framed and scrolled into view, as a hit of the menu filter.
                self.highlight = Some((t("s_sc_titel"), now));
            } else if *old == "hf_beziehungen" {
                self.handles.tab = crate::handle_page::Tab::Relations;
            } else if *old == "hf_wunschliste" {
                self.hangar.show_planned();
            } else if *to == "hf_hangar" {
                self.hangar.point_to_pairing();
            }
            if *to == "hf_bergung" {
                self.salvage.focus_parts();
            }
        }
    }

    /// Writes what the pages hold back ([`Self::write_held_back`]) and every setting that changed, and
    /// waits (a few seconds at most) until the settings thread has it on disk: the program that starts
    /// next reads the file.
    pub(super) fn save_and_wait(&mut self) {
        self.write_held_back();
        self.save_settings_and_wait();
    }

    /// Writes every setting that changed and waits (a few seconds at most) until the settings thread
    /// has it on disk.
    pub(super) fn save_settings_and_wait(&mut self) {
        self.save_changes();
        let until = std::time::Instant::now() + WRITE_WAIT;
        while self.writes_pending() && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// Every part of the window that writes after a pause: the pages that do, then the parts signed up
    /// while the window runs ([`Self::write_on_close`]).
    fn close_writers(&mut self) -> Vec<&mut dyn WritesOnClose> {
        let mut writers: Vec<&mut dyn WritesOnClose> =
            vec![&mut self.memo_page, &mut self.start_programs, &mut self.hangar];
        writers.extend(self.close_writes.iter_mut());
        writers
    }

    /// Writes what every part in [`Self::close_writers`] holds back, now.
    pub(super) fn write_held_back(&mut self) {
        let store = self.store.take();
        let paths = store.as_ref().map(|store| &store.paths);
        for writer in self.close_writers() {
            writer.write_held_back(paths);
        }
        self.store = store;
    }

    /// Signs up a part that writes after a pause: what it holds back is written when the window closes
    /// or starts anew.
    pub fn write_on_close(&mut self, writer: Box<dyn WritesOnClose>) {
        self.close_writes.sign_up(writer);
    }

    /// Starts the program again as it was started and closes this window. The settings are written
    /// first, all of them (the new window reads the file); a refused start is said and the window
    /// stays. True when the new program was started.
    pub(super) fn restart_now(&mut self, ctx: &egui::Context) -> bool {
        self.save_and_wait();
        let args: Vec<String> = std::env::args().skip(1).collect();
        match (self.restart_with)(&args) {
            Ok(()) => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                self.pin_notice = Some((ctx.input(|i| i.time), t("s_pf_startet_neu")));
                true
            }
            Err(e) => {
                self.pin_notice =
                    Some((ctx.input(|i| i.time), tf("s_bs_neustart_nein", &[("0", Arg::from(e.to_string()))])));
                false
            }
        }
    }

    /// "Durchsuchen …" of the game folder `channel`: asks for a folder, starting at the one set.
    pub(super) fn browse_game_folder(&mut self, ctx: &egui::Context, channel: usize) {
        if self.folder_dialog.is_some() {
            return;
        }
        let start = self.folders[channel].clone();
        let title = t("spielordner");
        self.folder_dialog = Some((channel, (self.choose_folder)(ctx, &title, Some(&start))));
    }

    /// The answer of a folder dialog: the folder goes into its field and is the one set. A folder
    /// that was not taken is said, once.
    pub(super) fn settle_folders(&mut self, now: f64) {
        if let Some(said) = self.folder_refused.take() {
            self.pin_notice = Some((now, said));
        }
        let Some((channel, dialog)) = &self.folder_dialog else { return };
        let channel = *channel;
        let Some(answer) = dialog.take() else { return };
        self.folder_dialog = None;
        if let Some(folder) = answer {
            self.folders[channel] = folder;
            self.commit_game_folder(channel);
            if let Some(said) = self.folder_refused.take() {
                self.pin_notice = Some((now, said));
            }
        }
    }

    /// A folder that is not there, said as the notice does.
    fn refuse_missing(&mut self, folder: &str) {
        self.folder_refused = Some(tf("s_bs_ordner_nein", &[("0", Arg::from(folder))]));
    }

    /// The folder typed in a field counts from now on: the field was left or Enter was pressed.
    /// A folder that is not there is not taken (the field keeps the text to correct, and the
    /// notice says so); an empty field means "find it yourself".
    pub(super) fn commit_game_folder(&mut self, channel: usize) {
        if channel > 0 {
            self.commit_channel_folder(channel);
            return;
        }
        if self.folders[0] == self.live_folder_saved {
            self.folder_search = None;
            return;
        }
        let folder = self.folders[0].trim().to_string();
        if !folder.is_empty() && !std::path::Path::new(&folder).is_dir() {
            self.folder_search = None;
            self.refuse_missing(&folder);
            return;
        }
        // A folder with its Game.log, or an empty field, counts at once. Any other folder is searched in a thread
        // of its own as the assistant does (the channel below it, a Wine prefix, up to five levels deeper); the
        // answer is taken in `settle_folder_search`, and the field shows the channel found.
        if folder.is_empty() || std::path::Path::new(&folder).join("Game.log").is_file() {
            self.folder_search = None;
            self.take_game_folder(folder);
            return;
        }
        if self.folder_search.as_ref().is_some_and(|search| search.typed == folder) {
            return;
        }
        let host = self.store.as_ref().map_or_else(Host::current, |store| store.paths.host.clone());
        self.folder_search = Some(FolderSearch::start(host, folder));
    }

    /// Takes the answer of the search for the game folder once it is there; a field typed into since then keeps
    /// its text. True while the search still runs.
    pub(super) fn settle_folder_search(&mut self) -> bool {
        let Some(search) = &self.folder_search else { return false };
        let found = match search.answer.try_recv() {
            Ok(found) => found,
            Err(std::sync::mpsc::TryRecvError::Empty) => return true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => None,
        };
        let typed = search.typed.clone();
        self.folder_search = None;
        if self.folders[0].trim() != typed {
            return false;
        }
        let folder = found.unwrap_or(typed);
        self.folders[0].clone_from(&folder);
        self.take_game_folder(folder);
        false
    }

    /// The LIVE folder counts for the settings from now on, and the box above the fields says whether the game
    /// is there.
    fn take_game_folder(&mut self, folder: String) {
        if folder == self.live_folder_saved {
            return;
        }
        self.live_folder_saved.clone_from(&folder);
        // The box above the fields tells what the program finds there now (an empty field leaves
        // the search to the program, which the box has shown since the start).
        if !folder.is_empty() {
            self.game_found = std::path::Path::new(&folder).join("Game.log").is_file().then_some(folder);
        }
    }

    /// The folder of another channel (row 1: PTU, 2: TECH-PREVIEW), new in v5: 4.12 only finds it next
    /// to LIVE. A chosen folder is kept in `kanal_ordner` (other entries there stay) and counts
    /// wherever the channels are read instead of the one next to LIVE: the translation of the PTU,
    /// the channel question. The folder next to LIVE or an empty field takes the choice back. A
    /// folder that is already the game folder or another channel's is not taken: that channel would
    /// be lost, and writing for one would hit the other.
    fn commit_channel_folder(&mut self, channel: usize) {
        let name = super::CHANNEL_ROWS[channel];
        let folder = self.folders[channel].trim().to_string();
        let found = self.channels_found[channel].clone();
        let chosen = self.chosen_channel(channel);
        if chosen.as_deref() == Some(folder.as_str()) || (chosen.is_none() && folder == found) {
            return;
        }
        if !folder.is_empty() && !std::path::Path::new(&folder).is_dir() {
            self.refuse_missing(&folder);
            return;
        }
        let taken = (0..super::CHANNEL_ROWS.len())
            .filter(|other| *other != channel)
            .find(|other| !folder.is_empty() && same_folder(&self.folders[*other], &folder));
        if let Some(other) = taken {
            let owner = super::CHANNEL_ROWS[other];
            self.folder_refused = Some(tf("s_pf_kanal_doppelt", &[("0", Arg::from(owner))]));
            self.folders[channel] = chosen.unwrap_or(found);
            return;
        }
        self.channels_saved.retain(|(key, _)| key != name);
        if folder.is_empty() || folder == found {
            self.folders[channel] = found;
        } else {
            self.channels_saved.push((name.to_string(), Value::str(&folder)));
        }
    }

    /// The own start command is taken over (4.12: the button "Übernehmen"); an empty field removes it and
    /// the way that was found counts again. The button in the foot follows within the next lookup,
    /// which is started at once.
    pub(super) fn commit_start_command(&mut self, now: f64) {
        let text = self.start_command.trim().to_string();
        self.start_command.clone_from(&text);
        if let Some(store) = &self.store {
            if !store.paths.set_setting(versekit_core::game_start::STARTER_SETTING, Value::str(text.as_str())) {
                self.pin_notice = Some((now, t("s_bp_speichern_fehler")));
                return;
            }
            self.start_look.poll(&store.paths, true);
        }
        self.pin_notice = Some((now, t(if text.is_empty() { "s_or_start_weg" } else { "s_or_start_ok" })));
    }

    /// Looks at the error log from time to time and takes its count: the report button follows
    /// what is written down while the window is open. Only the stamp of the file is asked, not
    /// its content, until it changes.
    pub(super) fn follow_errors(&mut self, now: f64) {
        let Some(store) = &self.store else { return };
        if self.errors_looked.is_some_and(|(at, _)| now - at < ERRORS_LOOK_SECONDS) {
            return;
        }
        let stamp = std::fs::metadata(store.paths.app_file(versekit_core::error_log::FILE))
            .and_then(|m| m.modified().map(|time| (time, m.len())))
            .ok();
        let changed = self.errors_looked.is_none_or(|(_, before)| before != stamp);
        self.errors_looked = Some((now, stamp));
        if changed {
            let log = versekit_core::error_log::ErrorLog::new(&store.paths, env!("CARGO_PKG_VERSION"));
            self.error_count = u32::try_from(log.unseen()).unwrap_or(u32::MAX);
            // The error page is open: what turns up now is looked at right away.
            if self.error_count > 0 && self.page == "hf_diagnose" && log.mark_seen() {
                self.error_count = 0;
            }
        }
    }

    /// The error page is open: what is written so far is seen, the count starts again at nothing.
    pub(super) fn mark_errors_seen(&mut self) {
        let Some(store) = &self.store else { return };
        if self.error_count == 0 {
            return;
        }
        let log = versekit_core::error_log::ErrorLog::new(&store.paths, env!("CARGO_PKG_VERSION"));
        if log.mark_seen() {
            self.error_count = 0;
        }
    }

    /// Takes what the file of the seen areas says. A fresh install marks nothing; whoever comes
    /// from an older version sees the mark until the area was opened once.
    pub(super) fn load_seen(&mut self, paths: &Paths) {
        let file = paths.app_file(SEEN_FILE);
        let own = env!("CARGO_PKG_VERSION");
        match jsonio::read_json(&file) {
            Ok(data) if data.get("zuletzt").is_some() => {
                let areas = data.get("bereiche").cloned();
                let last = data.get("zuletzt").and_then(Value::as_str).unwrap_or_default().to_string();
                // An area missing from the list counts as seen with the version started last: that one had it.
                let seen = |area: &str| {
                    let noted =
                        areas.as_ref().and_then(|a| a.get(area)).map(|v| v.as_str().unwrap_or_default().to_string());
                    noted.or_else(|| Some(last.clone()))
                };
                self.new_pages = new_pages(own, seen);
            }
            // The first start notes every area as seen with this version (4.12 `first_start`).
            _ => {
                self.new_pages = Vec::new();
                write_seen(&file, &NEW_SINCE.map(|(area, _)| area), true);
            }
        }
    }

    /// The area of `page` was opened: its mark is gone for good.
    pub(super) fn mark_seen(&mut self, page: &str) {
        let Some(at) = self.new_pages.iter().position(|p| *p == page) else { return };
        self.new_pages.remove(at);
        if let (Some(store), Some(area)) = (&self.store, page.strip_prefix("hf_")) {
            write_seen(&store.paths.app_file(SEEN_FILE), &[area], true);
        }
    }

    /// The marks of the newest version come back (a tool of the developers): their areas count as not seen.
    pub(super) fn forget_seen(&mut self) {
        let areas = newest_areas(env!("CARGO_PKG_VERSION"));
        for page in areas.iter().filter_map(|area| page_of_area(area)) {
            if !self.new_pages.contains(&page) {
                self.new_pages.push(page);
            }
        }
        if let Some(store) = &self.store {
            write_seen(&store.paths.app_file(SEEN_FILE), &areas, false);
        }
    }
}

/// Writes the file of the seen areas: `areas` noted as seen with this version (`seen`), or as never seen (a
/// missing area would count as seen with the version started last). Other areas stay; the version of the last
/// start is this one.
fn write_seen(file: &str, areas: &[&str], seen: bool) {
    let mut map = match jsonio::read_json(file) {
        Ok(Value::Object(map)) => map,
        _ => Vec::new(),
    };
    let version = env!("CARGO_PKG_VERSION");
    jsonio::map_set(&mut map, "zuletzt", Value::str(version));
    let mut kept = match map.iter().find(|(k, _)| k == "bereiche").map(|(_, v)| v.clone()) {
        Some(Value::Object(kept)) => kept,
        _ => Vec::new(),
    };
    for area in areas {
        jsonio::map_set(&mut kept, area, Value::str(if seen { version } else { "0.0.0" }));
    }
    jsonio::map_set(&mut map, "bereiche", Value::Object(kept));
    let _ = jsonio::save_json(file, &Value::Object(map), 1, false);
}

/// The search for the game folder below a typed or chosen folder (`Paths::resolve_game_folder`, as in the
/// assistant), in a thread of its own: a deep folder tree must not hold up the window.
pub(super) struct FolderSearch {
    /// The folder as it stood in the field.
    pub(super) typed: String,
    answer: std::sync::mpsc::Receiver<Option<String>>,
}

impl FolderSearch {
    fn start(host: Host, typed: String) -> FolderSearch {
        let (send, answer) = std::sync::mpsc::channel();
        let chosen = typed.clone();
        // Without a thread the channel is closed at once: the answer is "nothing found", the folder counts as typed.
        let _ = std::thread::Builder::new().name("spielordner-suche".into()).spawn_with_language(move || {
            let _ = send.send(Paths::new(host).resolve_game_folder(&chosen));
        });
        FolderSearch { typed, answer }
    }
}

/// Whether two folders are the same: the same place on the disk where both are there, else the same
/// text without a separator at the end and, on Windows, without regard to case.
fn same_folder(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim(), b.trim());
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if let (Ok(a), Ok(b)) = (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        return a == b;
    }
    let plain = |f: &str| {
        let f = f.trim_end_matches(['\\', '/']).to_string();
        if cfg!(windows) { f.to_lowercase() } else { f }
    };
    plain(a) == plain(b)
}

#[cfg(test)]
pub(in crate::sample) mod tests {
    use std::cell::RefCell;

    use egui::{Event, Pos2, Shape};
    use versekit_core::{font_size, language, theme};

    use versekit_core::storage_move::Outcome;

    use super::super::hotkey_text;
    use super::super::real_data::{folder, host_apart, real_page};
    use super::*;

    thread_local! {
        /// What the page asked the world for, in this test's thread.
        static OPENED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
        static ASKED: RefCell<Vec<Option<String>>> = const { RefCell::new(Vec::new()) };
        static STARTED: RefCell<Vec<Vec<String>>> = const { RefCell::new(Vec::new()) };
    }

    fn opens(url: &str) -> bool {
        OPENED.with(|list| list.borrow_mut().push(url.to_string()));
        true
    }

    fn opens_not(url: &str) -> bool {
        OPENED.with(|list| list.borrow_mut().push(url.to_string()));
        false
    }

    fn chooses(_: &egui::Context, _: &str, start: Option<&str>) -> Pending {
        ASKED.with(|list| list.borrow_mut().push(start.map(str::to_string)));
        Pending::answered(Some(chosen_folder()))
    }

    /// The folder every folder dialog of the tests answers with: empty, so no search below it finds a game.
    fn chosen_folder() -> String {
        let dir = std::env::temp_dir().join(format!("versekit-echt-gewaehlt-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::create_dir_all(&dir);
        dir.to_string_lossy().into_owned()
    }

    fn starts(args: &[String]) -> std::io::Result<()> {
        STARTED.with(|list| list.borrow_mut().push(args.to_vec()));
        Ok(())
    }

    fn starts_not(_: &[String]) -> std::io::Result<()> {
        Err(std::io::Error::other("verweigert"))
    }

    fn opened() -> Vec<String> {
        OPENED.with(|list| list.borrow().clone())
    }

    fn asked() -> Vec<Option<String>> {
        ASKED.with(|list| list.borrow().clone())
    }

    fn started() -> Vec<Vec<String>> {
        STARTED.with(|list| list.borrow().clone())
    }

    /// A window without a screen: frames with input, the texts drawn with their places.
    pub(in crate::sample) struct Bench {
        ctx: egui::Context,
        pub(in crate::sample) page: SamplePage,
        closed: bool,
        /// The time of the next frames in seconds; without one the window counts its own.
        pub(in crate::sample) time: Option<f64>,
        /// The height of the window in points.
        pub(in crate::sample) height: f32,
    }

    impl Bench {
        pub(in crate::sample) fn new(page: SamplePage) -> Bench {
            let ctx = egui::Context::default();
            crate::install(&ctx);
            Bench { ctx, page, closed: false, time: None, height: font_size::BASE_MIN_SIZE.1.max(900.0) }
        }

        pub(in crate::sample) fn frame(&mut self, events: Vec<Event>) -> Vec<(String, Pos2)> {
            fn collect(shape: &Shape, into: &mut Vec<(String, Pos2)>) {
                match shape {
                    Shape::Text(text) => into.push((text.galley.text().to_string(), text.pos)),
                    Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, into)),
                    _ => {}
                }
            }
            let width = font_size::BASE_MIN_SIZE.0;
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(width.max(1300.0), self.height))),
                events,
                time: self.time,
                ..Default::default()
            };
            let page = &mut self.page;
            let mut output = self.ctx.run_ui(input, |ui| {
                page.show(ui);
            });
            let mut texts = Vec::new();
            output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
            output.textures_delta.clear();
            if output
                .viewport_output
                .values()
                .any(|v| v.commands.iter().any(|c| matches!(c, egui::ViewportCommand::Close)))
            {
                self.closed = true;
            }
            texts
        }

        /// One frame in which the system asks the window to close.
        fn close_request(&mut self) {
            let mut input = egui::RawInput::default();
            input.viewports.entry(egui::ViewportId::ROOT).or_default().events.push(egui::ViewportEvent::Close);
            let page = &mut self.page;
            let mut output = self.ctx.run_ui(input, |ui| {
                page.show(ui);
            });
            output.textures_delta.clear();
        }

        /// A few frames so that the page has settled.
        pub(in crate::sample) fn settle(&mut self) -> Vec<(String, Pos2)> {
            let mut texts = Vec::new();
            for _ in 0..4 {
                texts = self.frame(Vec::new());
            }
            texts
        }

        /// Where the `nth` text that reads `label` stands, counted from the top.
        fn place(&mut self, label: &str, nth: usize) -> Pos2 {
            let texts = self.settle();
            let mut found: Vec<Pos2> = texts.iter().filter(|(text, _)| text == label).map(|(_, at)| *at).collect();
            found.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
            *found.get(nth).unwrap_or_else(|| {
                let shown: Vec<&str> = texts.iter().map(|(text, _)| text.as_str()).collect();
                panic!("{label} #{nth} is not on the page {}: {shown:?}", self.page.page_key())
            })
        }

        /// A click as a person makes it: the pointer comes, presses, lets go.
        pub(in crate::sample) fn click_at(&mut self, at: Pos2) {
            let at = at + egui::vec2(3.0, 3.0);
            let button = |pressed| Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            self.frame(vec![Event::PointerMoved(at)]);
            self.frame(vec![button(true)]);
            self.frame(vec![button(false)]);
            self.frame(Vec::new());
        }

        pub(in crate::sample) fn click(&mut self, label: &str) {
            self.click_nth(label, 0);
        }

        pub(in crate::sample) fn click_nth(&mut self, label: &str, nth: usize) {
            let at = self.place(label, nth);
            self.click_at(at);
        }

        /// Where the controls named `label` for a screen reader stand, from one frame.
        pub(in crate::sample) fn named(&mut self, label: &str) -> Vec<egui::Rect> {
            self.ctx.enable_accesskit();
            let width = font_size::BASE_MIN_SIZE.0;
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(width.max(1300.0), self.height))),
                time: self.time,
                ..Default::default()
            };
            let page = &mut self.page;
            let mut output = self.ctx.run_ui(input, |ui| {
                page.show(ui);
            });
            output.textures_delta.clear();
            let Some(update) = output.platform_output.accesskit_update else { return Vec::new() };
            update
                .nodes
                .iter()
                .filter(|(_, node)| node.label().is_some_and(|name| name == label))
                .filter_map(|(_, node)| {
                    let b = node.bounds()?;
                    let (min, max) = (Pos2::new(b.x0 as f32, b.y0 as f32), Pos2::new(b.x1 as f32, b.y1 as f32));
                    Some(egui::Rect::from_min_max(min, max))
                })
                .collect()
        }

        pub(in crate::sample) fn notice(&self) -> Option<String> {
            self.page.pin_notice.as_ref().map(|(_, text)| text.clone())
        }
    }

    /// Draws frames until `done` says yes (the lookup of the way to start runs in a thread), at most three seconds.
    fn until(bench: &mut Bench, mut done: impl FnMut(&mut Bench, &[(String, Pos2)]) -> bool) -> Vec<(String, Pos2)> {
        let mut texts = bench.settle();
        for _ in 0..60 {
            if done(bench, &texts) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
            texts = bench.settle();
        }
        texts
    }

    #[test]
    fn the_start_button_comes_with_a_way_to_start_and_starts_through_the_spawner() {
        use versekit_core::game_start::{Recorder, Spawner, Started};
        struct Broken;
        impl Spawner for Broken {
            fn spawn(&self, _: &[String], _: &[(String, String)]) -> Result<(), String> {
                Err("kaputt".into())
            }
        }
        let _language = crate::language_lock();
        // A game folder of its own: where the program looks for the game on the drives of the computer, a test
        // does not find the installation that happens to be there, and so not its launcher either.
        let game = folder("start-knopf-spiel");
        std::fs::create_dir_all(game.join("LIVE")).expect("game folder");
        std::fs::write(game.join("LIVE").join("Game.log"), "").expect("log");
        let live = game.join("LIVE").to_string_lossy().into_owned();
        let (page, dir) = real_page("start-knopf", "", &[(versekit_core::paths::ENV_INSTALL_DIR, live.as_str())]);
        let recorder = std::sync::Arc::new(Recorder::default());
        let mut bench = Bench::new(page);
        bench.page.spawner = recorder.clone();
        let word = language::t("s_sp_start_knopf");
        let has = |texts: &[(String, Pos2)]| texts.iter().any(|(text, _)| *text == word);
        // No way to start the game here: no button, once the lookup has run.
        let texts = until(&mut bench, |bench, _| {
            let paths = bench.page.store.as_ref().map(|s| s.paths.working_copy()).expect("real data");
            bench.page.start_look.poll(&paths, false).looked
        });
        assert!(!has(&texts), "no way to start, no button");
        // An own command is entered on the folder page: the button comes.
        bench.page.start_command = "  lutris rungame/x ".into();
        bench.page.commit_start_command(0.0);
        assert_eq!(bench.notice(), Some(language::t("s_or_start_ok")));
        assert_eq!(bench.page.start_command, "lutris rungame/x", "the field shows what was taken over, without blanks");
        let texts = until(&mut bench, |_, texts| has(texts));
        assert!(has(&texts), "the command makes the button");
        // A click hands the split command to the spawner, and says that it starts.
        bench.click(&word);
        until(&mut bench, |_, _| !recorder.calls().is_empty());
        assert_eq!(recorder.calls(), [Started { command: vec!["lutris".into(), "rungame/x".into()], env: Vec::new() }]);
        assert_eq!(bench.notice(), Some(language::t("s_sp_start_lauft")));
        // A spawner that fails: the reason stands in the foot.
        bench.page.spawner = std::sync::Arc::new(Broken);
        bench.click(&word);
        until(&mut bench, |bench, _| bench.notice() != Some(language::t("s_sp_start_lauft")));
        assert_eq!(bench.notice(), Some(tf("s_sp_start_nein", &[("0", Arg::from("kaputt"))])));
        // The command removed: the way found counts again, here none, so no button.
        bench.page.start_command.clear();
        bench.page.commit_start_command(0.0);
        assert_eq!(bench.notice(), Some(language::t("s_or_start_weg")));
        let texts = until(&mut bench, |_, texts| !has(texts));
        assert!(!has(&texts), "the button goes with the command");
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(game);
    }

    #[test]
    fn the_answer_of_the_channel_question_becomes_the_game_folder() {
        use crate::channel_question::Mismatch;
        let _language = crate::language_lock();
        let game = folder("kanal-frage-spiel");
        let hotfix = game.join("HOTFIX");
        std::fs::create_dir_all(&hotfix).expect("game folder");
        std::fs::write(hotfix.join("Game.log"), "").expect("log");
        let hotfix = hotfix.to_string_lossy().into_owned();
        // The folder entered is gone; LIVE was renamed to HOTFIX.
        let gone = game.join("LIVE").to_string_lossy().into_owned();
        let settings = format!(
            "{{\n  \"spiel_ordner\": {}\n}}\n",
            versekit_core::jsonio::dumps(&Value::str(gone.as_str()), None, false)
        );
        let (page, dir) = real_page("kanal-frage", &settings, &[]);
        let mut bench = Bench::new(page);
        bench.page.channel_question.ask(Mismatch {
            entered: gone.clone(),
            channels: vec![("HOTFIX".into(), hotfix.clone(), 1_700_000_000.0)],
        });
        let texts = bench.settle();
        assert!(texts.iter().any(|(text, _)| *text == language::t("s_kn_titel")), "the question stands there");
        bench.click(&language::t("e_ja"));
        let stored = bench.page.store.as_ref().and_then(|s| s.paths.setting("spiel_ordner"));
        assert_eq!(stored.as_deref(), Some(hotfix.as_str()), "the answer is the setting now");
        assert_eq!(bench.page.folders[0], hotfix, "the folder page shows it");
        // Not now: nothing is stored.
        let (page, dir2) = real_page("kanal-frage-spaeter", &settings, &[]);
        let mut bench = Bench::new(page);
        bench
            .page
            .channel_question
            .ask(Mismatch { entered: gone.clone(), channels: vec![("HOTFIX".into(), hotfix, 1_700_000_000.0)] });
        bench.click(&language::t("s_kn_spaeter"));
        let stored = bench.page.store.as_ref().and_then(|s| s.paths.setting("spiel_ordner"));
        assert_eq!(stored.as_deref(), Some(gone.as_str()), "not now leaves the setting alone");
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(dir2);
        let _ = std::fs::remove_dir_all(game);
    }

    pub(in crate::sample) fn preview() -> Bench {
        theme::set_current("original");
        // This thread only: tests of other pages run side by side without a lock.
        language::set_thread_language(language::Choice::De);
        Bench::new(SamplePage::preview())
    }

    #[test]
    fn discord_and_donate_open_their_addresses_and_say_when_they_cannot() {
        let _language = crate::language_lock();
        let mut bench = preview();
        bench.page.open_url = opens;
        bench.click(&language::t("hf_discord"));
        assert_eq!(opened(), ["https://xharig.com/discord"], "Discord leads to the address of 4.12");
        assert_eq!(bench.notice(), Some(language::t("hf_discord_auf")), "it says that the browser opens");
        bench.click(&language::t("hf_kofi"));
        assert_eq!(opened(), ["https://xharig.com/discord", "https://ko-fi.com/xharig"]);
        assert_eq!(bench.notice(), Some(language::t("hf_kofi_auf")));
        // Nothing could open it: the address is named in the notice.
        bench.page.open_url = opens_not;
        bench.click(&language::t("hf_discord"));
        let text = bench.notice().expect("a notice");
        assert!(
            text.contains(DISCORD_URL) && text.contains(&language::t("s_ub_auf_nein").replace("{0}", "")),
            "{text}"
        );
    }

    #[test]
    fn what_is_new_in_the_update_bar_opens_the_gallery() {
        let _language = crate::language_lock();
        let mut bench = preview();
        assert!(bench.page.update_waits(), "the bar is there");
        // The bar is above the foot bar, which has a button of the same kind.
        bench.click_nth("Was ist neu", 0);
        assert_eq!(bench.page.page, "hf_wasistneu");
    }

    #[test]
    fn the_first_and_the_data_folder_restart_actually_restart_and_a_refusal_is_said() {
        let _language = crate::language_lock();
        let (page, dir) = real_page("neustart", "", &[]);
        let mut bench = Bench::new(page);
        bench.page.restart_with = starts;
        bench.page.data_folder.pretend_switched(&Outcome::Switched);
        bench.page.go_to("hf_ordner");
        bench.click(&language::t("s_da_neustart_knopf"));
        assert_eq!(started().len(), 1, "the program was started again");
        assert!(bench.closed, "and this window closes");
        // A refused start: said, and the window stays.
        let (page, dir2) = real_page("neustart-nein", "", &[]);
        let mut bench = Bench::new(page);
        bench.page.restart_with = starts_not;
        bench.page.data_folder.pretend_switched(&Outcome::Switched);
        bench.page.go_to("hf_ordner");
        bench.click(&language::t("s_da_neustart_knopf"));
        assert!(!bench.closed, "a window that could not restart stays open");
        assert!(bench.notice().is_some_and(|t| t.contains("verweigert")), "{:?}", bench.notice());
        // And the bar asks again: the restart that is needed is not forgotten.
        let texts = bench.settle();
        assert!(
            texts.iter().any(|(t, _)| *t == language::t("s_da_neustart_knopf")),
            "the restart button is still there"
        );
        let _ = (std::fs::remove_dir_all(dir), std::fs::remove_dir_all(dir2));
    }

    thread_local! {
        /// The settings file as the restarted program would find it.
        static FILE_AT_START: RefCell<Option<String>> = const { RefCell::new(None) };
        static SETTINGS_FILE: RefCell<Option<std::path::PathBuf>> = const { RefCell::new(None) };
    }

    fn starts_reading(_: &[String]) -> std::io::Result<()> {
        let path = SETTINGS_FILE.with(|p| p.borrow().clone()).expect("a file");
        FILE_AT_START.with(|f| *f.borrow_mut() = Some(std::fs::read_to_string(path).unwrap_or_default()));
        Ok(())
    }

    /// The usage report is on from the factory, its switch is on the page General in the group Privacy and is
    /// stored under the key of 4.12, so that an old choice counts.
    #[test]
    fn the_usage_switch_is_on_from_the_factory_stands_in_privacy_and_is_stored() {
        let _language = crate::language_lock();
        let (mut page, dir) = real_page("nutzung-schalter", "{\n  \"nutzung_melden\": false\n}\n", &[]);
        // An old choice of 4.12 is read.
        assert!(!page.usage_on, "the stored 'off' counts");
        let paths = Paths::new(super::super::real_data::host_in(&dir, &[]));
        assert!(!versekit_core::usage::enabled(&paths));
        // The row stands in the group of privacy.
        let (title, rows) = super::super::settings_of("hf_allgemein")
            .iter()
            .find(|(title, _)| *title == "s_ei_datenschutz")
            .copied()
            .expect("the group Datenschutz");
        assert!(rows.iter().any(|(name, hint, _)| *name == "s_nutzung" && *hint == "s_nutzung_h"), "{title}: {rows:?}");
        // The first saving only takes the state as it is; then "on" again is stored with the other settings.
        page.save_changes();
        page.usage_on = true;
        assert!(
            page.setting_values()
                .iter()
                .any(|(key, value)| *key == versekit_core::usage::SETTING && *value == Value::Bool(true))
        );
        page.save_changes();
        assert!(versekit_core::usage::enabled(&Paths::new(super::super::real_data::host_in(&dir, &[]))));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_restart_waits_until_the_settings_are_written() {
        let _language = crate::language_lock();
        let (page, dir) = real_page("neustart-schreiben", "", &[]);
        let mut bench = Bench::new(page);
        bench.page.write_in_background();
        bench.page.restart_with = starts_reading;
        SETTINGS_FILE.with(|p| *p.borrow_mut() = Some(dir.join("einstellungen.json")));
        bench.settle();
        // A write takes its time; a setting changed just before the restart is still on its way.
        bench.page.slow_writes(300);
        bench.page.overlay_rows = 44;
        bench.page.data_folder.pretend_switched(&Outcome::Switched);
        bench.page.go_to("hf_ordner");
        bench.click(&language::t("s_da_neustart_knopf"));
        bench.page.slow_writes(0);
        let seen = FILE_AT_START.with(|f| f.borrow().clone()).expect("the program was started");
        assert!(seen.contains("44"), "the new program finds the setting written: {seen}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn browsing_a_game_folder_puts_the_choice_in_the_field_and_saves_it() {
        let _language = crate::language_lock();
        let (page, dir) = real_page("durchsuchen", "", &[]);
        let mut bench = Bench::new(page);
        bench.page.choose_folder = chooses;
        bench.page.go_to("hf_ordner");
        // The first "Durchsuchen …" of the page is the LIVE folder.
        bench.click_nth("Durchsuchen …", 0);
        bench.settle();
        assert_eq!(asked().len(), 1, "the dialog was asked for once");
        searched(&mut bench.page);
        bench.settle();
        let chosen = chosen_folder();
        assert_eq!(bench.page.folders[0], chosen, "the choice stands in the field");
        let stored = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap_or_default();
        assert!(stored.contains(&chosen.replace('\\', "\\\\")), "and is saved: {stored}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The overlay page: „Anzeige“ and „Platz“ side by side and about equally tall, „Wann sichtbar“ in one line and
    /// „Tastenkürzel“ under them over the whole width, all four in the window without scrolling.
    #[test]
    fn the_overlay_page_shows_display_and_place_side_by_side_and_fits_without_scrolling() {
        let _language = crate::language_lock();
        let (page, dir) = real_page("overlay-raster", "", &[]);
        let mut bench = Bench::new(page);
        bench.page.go_to("hf_overlay");
        bench.settle();
        crate::widgets::drawn_cards::record(true);
        bench.frame(Vec::new());
        let (cards, _) = crate::widgets::drawn_cards::take_split();
        crate::widgets::drawn_cards::record(false);
        let cards: Vec<egui::Rect> = cards.into_iter().map(|(card, _)| card).collect();
        assert!(cards.len() >= 4, "{cards:?}");
        // The first card and the one beside it, then the ones right under each of them.
        let display = cards[0];
        let beside = |card: egui::Rect| {
            cards.iter().copied().find(|c| (c.top() - card.top()).abs() < 1.0 && c.left() > card.right())
        };
        let under = |card: egui::Rect| {
            cards
                .iter()
                .copied()
                .filter(|c| (c.left() - card.left()).abs() < 1.0 && c.top() > card.bottom())
                .min_by(|a, b| a.top().total_cmp(&b.top()))
        };
        let place = beside(display).expect("„Platz“ beside „Anzeige“");
        let (low, high) = (display.height().min(place.height()), display.height().max(place.height()));
        assert!(high - low < 0.2 * high, "about equally tall: {display:?} {place:?}");
        let (when, keys) = (under(display).expect("„Wann sichtbar“"), under(place).expect("„Tastenkürzel“"));
        // Side by side: their tops apart by less than a title and a row (the columns of cards even out their ends).
        assert!(
            (when.top() - keys.top()).abs() < 60.0,
            "„Wann sichtbar“ and „Tastenkürzel“ side by side: {when:?} {keys:?}"
        );
        assert!((when.width() - display.width()).abs() < 1.0, "in the columns of cards: {when:?}");
        assert!(keys.bottom() <= bench.height, "without scrolling: {keys:?} in a window {} tall", bench.height);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The hotkey of the refinery scan on the overlay page: changed in its picker, said as registered while the
    /// overlay runs, refused without a modifier key, kept in `hotkey_scan`.
    #[test]
    fn the_hotkey_of_the_refinery_scan_is_changed_said_and_refused_without_a_modifier() {
        let _language = crate::language_lock();
        let (page, dir) = real_page("scan-taste", "", &[]);
        let mut bench = Bench::new(page);
        // Tall enough for the picker of the scan, the second one of the page.
        bench.height = 1800.0;
        bench.page.overlay_runs = || true;
        bench.page.go_to("hf_overlay");
        let texts = bench.settle();
        let shown = |texts: &[(String, Pos2)], text: &str| texts.iter().any(|(t, _)| t == text);
        assert!(shown(&texts, &language::t("s_hk_scan")), "the row: {texts:?}");
        if !versekit_core::refinery_scan::screen::supported() {
            assert!(shown(&texts, &language::t("s_nicht_moegl")), "where the screen cannot be read, the reason");
            let _ = std::fs::remove_dir_all(dir);
            return;
        }
        assert_eq!(hotkey_text(bench.page.scan_modifiers, bench.page.scan_key).as_deref(), Some("Alt+Umschalt+R"));
        // The second picker of the page is the one of the scan: Strg on.
        bench.click_nth(&language::t("s_ei_taste_strg"), 1);
        bench.settle();
        assert_eq!(hotkey_text(bench.page.scan_modifiers, bench.page.scan_key).as_deref(), Some("Strg+Alt+Umschalt+R"));
        let said = bench.page.pin_notice.as_ref().map(|(_, text)| text.clone());
        assert_eq!(said, Some(language::tf("s_hk_ok", &[("0", language::Arg::from("Strg+Alt+Umschalt+R"))])));
        let stored = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap_or_default();
        assert!(stored.contains("\"hotkey_scan\": \"Strg+Alt+Umschalt+R\""), "{stored}");
        // All three off: no hotkey, the saved one stays and the line says why.
        for word in ["s_ei_taste_strg", "s_ei_taste_umschalt"] {
            bench.click_nth(&language::t(word), 1);
            bench.settle();
        }
        bench.click_nth("Alt", 1);
        bench.settle();
        assert_eq!(
            hotkey_text(bench.page.scan_modifiers, bench.page.scan_key).as_deref(),
            Some("Alt+R"),
            "the last one stays"
        );
        let said = bench.page.pin_notice.as_ref().map(|(_, text)| text.clone());
        assert_eq!(said, Some(language::tf("s_bs_hotkey_nein", &[("0", language::Arg::from("Alt+R"))])));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The hotkey of the handle search on the overlay page: on at first with its picker showing the default; switched
    /// off, the picker goes and `hotkey_suche_ein` keeps `false`; on again, a change is said while the overlay runs and
    /// kept in `hotkey_suche`, and without a modifier key the saved one stays.
    #[test]
    fn the_hotkey_of_the_handle_search_is_on_at_first_changed_and_refused_without_a_modifier() {
        let _language = crate::language_lock();
        let (page, dir) = real_page("such-taste", "", &[]);
        let mut bench = Bench::new(page);
        bench.page.overlay_runs = || true;
        // The hotkey of the picture of the game below it without its picker: the one of the search is the last.
        bench.page.picture_hotkey_on = false;
        bench.page.go_to("hf_overlay");
        // A little down the page, so that the picker below the last row of the first group stands in the window.
        bench.frame(vec![egui::Event::PointerMoved(Pos2::new(800.0, 450.0))]);
        bench.frame(vec![egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -240.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
        }]);
        let texts = bench.settle();
        let strg = language::t("s_ei_taste_strg");
        let pickers = |texts: &[(String, Pos2)]| texts.iter().filter(|(t, _)| *t == strg).count();
        assert!(texts.iter().any(|(t, _)| *t == language::t("s_hk_suche")), "the row: {texts:?}");
        assert!(bench.page.search_hotkey_on, "on at first");
        let on = pickers(&texts);
        let current = |page: &SamplePage| hotkey_text(page.search_modifiers, page.search_key);
        assert_eq!(current(&bench.page).as_deref(), Some("Alt+Umschalt+H"));
        bench.page.search_hotkey_on = false;
        let texts = bench.settle();
        let off = pickers(&texts);
        assert_eq!(off + 1, on, "switched off: its picker, the last on the page, goes");
        let stored = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap_or_default();
        assert!(stored.contains("\"hotkey_suche_ein\": false"), "{stored}");
        bench.page.search_hotkey_on = true;
        let texts = bench.settle();
        assert_eq!(pickers(&texts), on, "on again: its picker is back");
        let stored = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap_or_default();
        assert!(stored.contains("\"hotkey_suche_ein\": true"), "{stored}");
        bench.click_nth(&strg, off);
        bench.settle();
        assert_eq!(current(&bench.page).as_deref(), Some("Strg+Alt+Umschalt+H"));
        let said = bench.page.pin_notice.as_ref().map(|(_, text)| text.clone());
        assert_eq!(said, Some(language::tf("s_hk_ok", &[("0", language::Arg::from("Strg+Alt+Umschalt+H"))])));
        let stored = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap_or_default();
        assert!(stored.contains("\"hotkey_suche\": \"Strg+Alt+Umschalt+H\""), "{stored}");
        for word in [strg.clone(), "Alt".to_string(), language::t("s_ei_taste_umschalt")] {
            bench.click_nth(&word, off);
            bench.settle();
        }
        assert_eq!(current(&bench.page).as_deref(), Some("Umschalt+H"), "the last one stays");
        let said = bench.page.pin_notice.as_ref().map(|(_, text)| text.clone());
        assert_eq!(said, Some(language::tf("s_bs_hotkey_nein", &[("0", language::Arg::from("Umschalt+H"))])));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The hotkey of the picture of the game on the overlay page: on at first with the default; a change is said while
    /// the overlay runs and kept in `hotkey_bild`, without a modifier key the saved one stays, and switched off
    /// `hotkey_bild_an` keeps `false`. Where the screen cannot be read, the reason.
    #[test]
    fn the_hotkey_of_the_picture_of_the_game_is_on_at_first_changed_and_refused_without_a_modifier() {
        let _language = crate::language_lock();
        let (page, dir) = real_page("bild-taste", "", &[]);
        let mut bench = Bench::new(page);
        bench.height = 3000.0;
        bench.page.overlay_runs = || true;
        // The handle search without its picker: the one of the picture is the third of the page.
        bench.page.search_hotkey_on = false;
        bench.page.go_to("hf_overlay");
        let texts = bench.settle();
        let shown = |texts: &[(String, Pos2)], text: &str| texts.iter().any(|(t, _)| t == text);
        assert!(shown(&texts, &language::t("s_hk_bild")), "the row: {texts:?}");
        assert!(bench.page.picture_hotkey_on, "on at first");
        let current = |page: &SamplePage| hotkey_text(page.picture_modifiers, page.picture_key);
        assert_eq!(current(&bench.page).as_deref(), Some("Alt+Umschalt+B"));
        if !versekit_core::screen_grab::supported() {
            assert!(shown(&texts, &language::t("s_nicht_moegl")), "where the screen cannot be read, the reason");
            let _ = std::fs::remove_dir_all(dir);
            return;
        }
        let strg = language::t("s_ei_taste_strg");
        bench.click_nth(&strg, 2);
        bench.settle();
        assert_eq!(current(&bench.page).as_deref(), Some("Strg+Alt+Umschalt+B"));
        let said = bench.page.pin_notice.as_ref().map(|(_, text)| text.clone());
        assert_eq!(said, Some(language::tf("s_hk_ok", &[("0", language::Arg::from("Strg+Alt+Umschalt+B"))])));
        let stored = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap_or_default();
        assert!(stored.contains("\"hotkey_bild\": \"Strg+Alt+Umschalt+B\""), "{stored}");
        // The hint under the button of the picture names it in the language of the window.
        let shown_key = crate::sample::glance_tiles::hotkey_in(language::current(), "Strg+Alt+Umschalt+B");
        assert_eq!(bench.page.mining.scanner.hotkey.as_deref(), Some(shown_key.as_str()));
        for word in [strg.clone(), "Alt".to_string(), language::t("s_ei_taste_umschalt")] {
            bench.click_nth(&word, 2);
            bench.settle();
        }
        assert_eq!(current(&bench.page).as_deref(), Some("Umschalt+B"), "the last one stays");
        let said = bench.page.pin_notice.as_ref().map(|(_, text)| text.clone());
        assert_eq!(said, Some(language::tf("s_bs_hotkey_nein", &[("0", language::Arg::from("Umschalt+B"))])));
        bench.page.picture_hotkey_on = false;
        bench.settle();
        let stored = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap_or_default();
        assert!(stored.contains("\"hotkey_bild_an\": false"), "{stored}");
        assert_eq!(bench.page.mining.scanner.hotkey, None, "switched off: the hint names no hotkey");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The seconds before the picture of the game, in the card of the signature window: ten at first, a click on
    /// another choice keeps it in `spielbild_wartezeit`.
    #[test]
    fn the_seconds_before_the_picture_of_the_game_are_chosen_and_kept() {
        if !versekit_core::screen_grab::supported() {
            return;
        }
        let _language = crate::language_lock();
        let (page, dir) = real_page("bild-wartezeit", "", &[]);
        let mut bench = Bench::new(page);
        bench.height = 9000.0;
        bench.page.go_to("hf_overlay");
        let texts = bench.settle();
        assert!(texts.iter().any(|(t, _)| *t == language::t("s_sg_bild_wartezeit")), "the row: {texts:?}");
        assert_eq!(bench.page.mining.scanner.delay, 10, "ten seconds at first");
        // The last "30 s" of the page is the choice of the row, the last row of the page.
        let thirty = texts.iter().filter(|(t, _)| t == "30 s").count();
        assert!(thirty >= 1, "{texts:?}");
        bench.click_nth("30 s", thirty - 1);
        bench.settle();
        assert_eq!(bench.page.mining.scanner.delay, 30);
        let stored = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap_or_default();
        assert!(stored.contains("\"spielbild_wartezeit\": 30"), "{stored}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The gallery in the window: "Ansehen" opens the page of the highlight, and the gallery counts as shown for
    /// this version.
    #[test]
    fn the_gallery_in_the_window_opens_the_page_of_its_highlight_and_counts_as_shown() {
        let _language = crate::language_lock();
        let (page, dir) = real_page("galerie-fenster", "", &[]);
        let mut bench = Bench::new(page);
        bench.settle();
        bench.page.gallery = crate::gallery_page::GalleryState::new("3.97.0", Vec::new());
        assert!(bench.page.gallery.is_some(), "3.97 has highlights");
        let texts = bench.settle();
        assert!(texts.iter().any(|(t, _)| *t == language::t("gal_grafik_t")), "the first highlight: {texts:?}");
        bench.click_nth(&language::t("gal_ansehen"), 0);
        bench.settle();
        assert!(bench.page.gallery.is_none(), "closed");
        assert_eq!(bench.page.page_key(), "hf_grafik", "the page of the highlight");
        let stored = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap_or_default();
        let shown = versekit_core::gallery::SETTING;
        assert!(stored.contains(&format!("\"{shown}\"")), "noted as shown: {stored}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Waits for the search for the game folder (ten seconds at most) and takes its answer.
    fn searched(page: &mut SamplePage) {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while page.settle_folder_search() {
            assert!(std::time::Instant::now() < until, "the search ends");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    /// A Wine prefix, the folder above the channel or a folder further up is enough: the search as in the assistant
    /// runs in a thread of its own, and the channel found is taken and shown.
    #[test]
    fn a_wine_prefix_typed_as_the_game_folder_finds_the_channel_below_it() {
        let _language = crate::language_lock();
        let (page, dir) = real_page("wine-praefix", "", &[]);
        let mut bench = Bench::new(page);
        // The first frames take note of the settings as they are.
        bench.settle();
        let live = dir
            .join("prefix")
            .join("drive_c")
            .join("Program Files")
            .join("Roberts Space Industries")
            .join("StarCitizen")
            .join("LIVE");
        std::fs::create_dir_all(&live).unwrap();
        std::fs::write(live.join("Game.log"), "").unwrap();
        let live = live.to_string_lossy().into_owned();
        let prefix = dir.join("prefix").to_string_lossy().into_owned();
        bench.page.folders[0].clone_from(&prefix);
        bench.page.commit_game_folder(0);
        assert!(bench.page.folder_search.is_some(), "searched in a thread, the window goes on");
        assert_eq!(bench.page.folders[0], prefix, "nothing taken before the answer");
        searched(&mut bench.page);
        assert_eq!(bench.page.folders[0], live, "the field shows the channel");
        assert_eq!(bench.page.game_found.as_deref(), Some(live.as_str()), "and the box finds the game there");
        bench.settle();
        let stored = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap_or_default();
        assert!(stored.contains(&live.replace('\\', "\\\\")), "saved: {stored}");
        // The folder above LIVE works the same.
        let above = std::path::Path::new(&live).parent().unwrap().to_string_lossy().into_owned();
        bench.page.folders[0] = above;
        bench.page.commit_game_folder(0);
        searched(&mut bench.page);
        assert_eq!(bench.page.folders[0], live, "the folder above the channel");
        // LIVE two levels below the folder typed: the deep search of the assistant finds it.
        let deep = dir.join("tief").join("StarCitizen").join("LIVE");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("Game.log"), "").unwrap();
        let deep = deep.to_string_lossy().into_owned();
        bench.page.folders[0] = dir.join("tief").to_string_lossy().into_owned();
        bench.page.commit_game_folder(0);
        searched(&mut bench.page);
        assert_eq!(bench.page.folders[0], deep, "the channel two levels down");
        assert_eq!(bench.page.game_found.as_deref(), Some(deep.as_str()));
        // A folder without any Game.log below it counts as typed, and the box finds nothing.
        let empty = dir.join("leer").join("a");
        std::fs::create_dir_all(&empty).unwrap();
        let empty = dir.join("leer").to_string_lossy().into_owned();
        bench.page.folders[0].clone_from(&empty);
        bench.page.commit_game_folder(0);
        searched(&mut bench.page);
        assert_eq!(bench.page.folders[0], empty);
        assert_eq!(bench.page.game_found, None);
        // An answer for a field typed into since then is not taken.
        bench.page.folders[0] = prefix;
        bench.page.commit_game_folder(0);
        bench.page.folders[0] = "anders".to_string();
        searched(&mut bench.page);
        assert_eq!(bench.page.folders[0], "anders", "the field keeps what was typed since");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Not found: the page names the places it looked at.
    #[test]
    fn a_game_not_found_shows_where_it_was_looked_for() {
        let _language = crate::language_lock();
        let (page, dir) = real_page("gesucht", "", &[]);
        let mut bench = Bench::new(page);
        // The places come with the settings; on a computer without the game the page reads them itself.
        if super::super::real_data::game_on_this_computer() {
            bench.page.game_found = None;
            bench.page.game_searched = vec!["X:\\Spiele\\StarCitizen\\LIVE".to_string()];
        } else {
            assert!(bench.page.game_found.is_none() && !bench.page.game_searched.is_empty(), "read with the settings");
        }
        let place = bench.page.game_searched[0].clone();
        bench.page.go_to("hf_ordner");
        let texts = bench.settle();
        let shown = |text: &str| texts.iter().any(|(t, _)| t == text);
        assert!(shown(&language::t("gesucht_wurde_hier")) && shown(&place), "{texts:?}");
        // Found: no list.
        bench.page.game_found = Some(place.clone());
        let texts = bench.settle();
        assert!(!texts.iter().any(|(t, _)| *t == language::t("gesucht_wurde_hier")), "{texts:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A start page on real data in a throwaway folder with a game folder of its own, from the
    /// settings `settings` and the further files `files` (name, content) of the data folder.
    fn start_page_in(name: &str, settings: &str, files: &[(&str, &str)]) -> (SamplePage, std::path::PathBuf) {
        let dir = folder(name);
        std::fs::write(dir.join("einstellungen.json"), settings).unwrap();
        for (file, text) in files {
            std::fs::write(dir.join(file), text).unwrap();
        }
        theme::set_current("original");
        language::set_thread_language(language::Choice::De);
        (SamplePage::with_settings(Paths::new(host_apart(&dir))), dir)
    }

    /// Real data: the tiles of 4.12 count from the files, a tile without a source in this version
    /// says it comes later, the pinned pages show their short explanation (4.12 "Deine Karten"),
    /// and "Öffnen" leads to the page.
    #[test]
    fn the_tiles_of_4_12_count_real_data_and_pinned_pages_explain_themselves() {
        let _language = crate::language_lock();
        let settings = r#"{"karten_angeheftet": ["hangar", "bergbau"], "startseite_zuerst": "karten"}"#;
        let raw = r#"{"format": 1, "posten": [{"material": "Gold", "menge": 12.5, "ort": "Area18"}]}"#;
        // "Arclight Pistol Black" is owned in the samples of the list: without a catalog nothing of them counts.
        let watch = r#"{"namen": ["Arclight Pistol", "Arclight Pistol Black"], "eintraege": []}"#;
        let ships = r#"{"format": 1, "schiffe": [{"name": "Carrack", "lti": true, "belegung": {}}, {"name": "Gladius", "einsatzbereit": true, "belegung": {}}]}"#;
        let (page, dir) = start_page_in(
            "kacheln-echt",
            settings,
            &[
                (versekit_core::stock::RAW_FILE, raw),
                (versekit_core::watchlist::FILE, watch),
                (versekit_core::hangar::FILE, ships),
            ],
        );
        let mut bench = Bench::new(page);
        bench.page.open_overview();
        let hangar = t("k_hangar");
        let ships_line = tf("s_kt_hangar_zahl", &[("0", versekit_core::language::Arg::from(2))]);
        let texts = until(&mut bench, |_, texts| texts.iter().any(|(text, _)| *text == ships_line));
        assert!(texts.iter().any(|(text, _)| *text == ships_line), "{texts:?}");
        assert!(texts.iter().any(|(text, _)| *text == t("k_bergbau")), "{texts:?}");
        let overview = bench.page.overview.clone().expect("counted");
        // A pinned page with data of its own shows them, not its explanation (which 4.12 showed on its cards).
        let tile = overview.tile("hf_hangar");
        assert_eq!(tile.line, ships_line);
        assert_ne!(tile.line, hangar);
        assert!(
            tile.details.contains(&"Carrack".to_string()) && tile.details.contains(&"Gladius".to_string()),
            "{tile:?}"
        );
        // One without data of its own keeps the short explanation, and is no tile that comes later.
        assert_eq!(overview.tile("hf_bergbau").line, t("k_bergbau"));
        assert_eq!(overview.tile("lager").line, "Rohstoffe: 1 · Waren: 0 · Items: 0");
        // SCU with three places, as in the game and in 4.12.
        assert_eq!(overview.tile("lager").details, ["Gold · 12,500 SCU · Area18"]);
        assert_eq!(overview.tile("merkliste").line, "0 von 2 gefunden");
        assert_eq!(overview.tile("merkliste").details, ["Arclight Pistol", "Arclight Pistol Black"]);
        assert_eq!(overview.tile("dienste").line, t("s_uv_cig_unbekannt"));
        let no_catalog = t("s_kt_katalog_leer");
        // The computer is read from the log beside the window; the empty log of the test rates nothing.
        let computer = overview.tile("rechner").line;
        assert!([t("s_uv_lese"), t("s_gr_klasse_leer")].contains(&computer), "{computer}");
        assert_eq!(
            overview.tile("raffinerie").line,
            "Kein laufender Auftrag bekannt. Drück am Terminal Alt+Umschalt+R.",
            "no job known: the hotkey at the terminal starts the time"
        );
        assert_eq!(
            overview.tile("raffinerie").action,
            cfg!(windows).then_some(super::super::glance::TileAction::RefineryScan),
            "the link reads the terminal where the screen can be read"
        );
        // An empty catalog counts as in 4.12, and names no version.
        assert_eq!(overview.tile("bauplaene").line, "0 von 0 · 0 %");
        assert_eq!(overview.tile("spiel").details, ["Kanal: LIVE"], "{:?}", overview.tile("spiel"));
        // Without any catalog the list holds samples: nothing of them is counted.
        let real_list = std::mem::replace(&mut bench.page.blueprints, crate::blueprint_list::ListData::sample());
        bench.page.overview = None;
        bench.settle();
        let samples = bench.page.overview.clone().expect("counted");
        assert_eq!(samples.tile("bauplaene").line, no_catalog);
        assert_eq!(samples.tile("neu_patch").line, no_catalog);
        assert_eq!(samples.tile("spiel").details, ["Kanal: LIVE"], "no version of the samples");
        assert_eq!(samples.tile("merkliste").line, "0 von 2 gefunden", "the samples own nothing");
        bench.page.blueprints = real_list;
        bench.page.overview = None;
        let keys: Vec<&str> = bench.page.tiles.iter().map(|tile| tile.key.as_str()).collect();
        assert!(!keys.contains(&"bedarf"), "the unit demand needs the connection: {keys:?}");
        assert_eq!(keys.contains(&"raffinerie"), cfg!(windows), "the refinery where the terminal can be read");
        // A click on the hangar tile itself opens the page (the menu at the left names the page too). A tile of
        // one grid row has no room for a button below its lines: the click is the way in.
        let menu = bench.place("Bauplan-Liste", 0).x + 100.0;
        let title = texts.iter().find(|(text, at)| *text == t("hf_hangar") && at.x > menu).expect("hangar card").1;
        bench.click_at(title + egui::vec2(6.0, 6.0));
        assert_eq!(bench.page.page, "hf_hangar");
        // The same for the storage tile of 4.12: it leads to my storage (`overview.TILES`, target `meinlager`).
        bench.page.open_overview();
        let texts = bench.settle();
        let title = texts.iter().find(|(text, at)| *text == t("s_uv_lager") && at.x > menu).expect("storage tile").1;
        bench.click_at(title + egui::vec2(6.0, 6.0));
        assert_eq!(bench.page.page, "hf_meinlager");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The notes tile shows the memo edited last: its title as the line, the three lines after it muted. Without
    /// a memo it says where to make one; a click on it opens the page.
    #[test]
    fn the_notes_tile_shows_the_memo_edited_last_and_opens_the_page() {
        use super::super::glance::Tone;
        let _language = crate::language_lock();
        let notes = r#"{"format": 1, "zettel": [
            {"text": "Alt\nA1", "bearbeitet": 10},
            {"text": "\n Titanium holen \n\nLorville\nArea18\nGold\nZu viel", "bearbeitet": 50},
            {"text": "Mittel", "bearbeitet": 30}]}"#;
        let (page, dir) = start_page_in("kacheln-notizen", "{}", &[(versekit_core::memo::FILE, notes)]);
        let mut bench = Bench::new(page);
        bench.page.open_overview();
        let texts = until(&mut bench, |_, texts| texts.iter().any(|(text, _)| *text == "Titanium holen"));
        let tile = bench.page.overview.clone().expect("counted").tile("notizen");
        assert_eq!(tile.line, "Titanium holen");
        assert_eq!(tile.details, ["Lorville", "Area18", "Gold"], "three lines after the title, not the fourth");
        assert!((0..3).all(|at| tile.tone_of(at) == Tone::Muted), "{tile:?}");
        // The tile stands in the grid with its title; a click on it leads to the page.
        let menu = bench.place("Bauplan-Liste", 0).x + 100.0;
        let at = texts.iter().find(|(text, at)| *text == "Titanium holen" && at.x > menu).expect("notes tile").1;
        bench.click_at(at + egui::vec2(6.0, 6.0));
        assert_eq!(bench.page.page, "hf_notizen");
        let _ = std::fs::remove_dir_all(dir);

        // No memo: the hint. A memo without text is named as such.
        let (page, dir) = start_page_in("kacheln-notizen-leer", "{}", &[]);
        let mut bench = Bench::new(page);
        bench.page.open_overview();
        until(&mut bench, |_, texts| texts.iter().any(|(text, _)| *text == t("s_uv_notizen_leer")));
        assert_eq!(bench.page.overview.clone().expect("counted").tile("notizen").line, t("s_uv_notizen_leer"));
        bench.page.memo_page.add(0.0);
        bench.page.overview = None;
        bench.settle();
        assert_eq!(bench.page.overview.clone().expect("counted").tile("notizen").line, t("s_nz_ohne_titel"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The tile of the Executive Hangar shows the phase in its colour, the lamps, the countdown and when it
    /// opens; the countdown is counted anew every second, and a click on the tile opens the page.
    #[test]
    fn the_executive_hangar_tile_counts_every_second_and_opens_the_page() {
        use super::super::glance::Tone;
        use versekit_core::contested::Lamp;
        let _language = crate::language_lock();
        // A cycle that began 3000 seconds ago: closed, with two lamps green.
        let start = versekit_core::export::now_unix() - 3_000;
        let file = format!(r#"{{"format": 1, "start": {start}, "geholt": 1.0, "hand": null}}"#);
        let (page, dir) = start_page_in("kacheln-hangar", "{}", &[(versekit_core::contested::FILE, &file)]);
        let mut bench = Bench::new(page);
        bench.page.open_overview();
        let texts = until(&mut bench, |_, texts| texts.iter().any(|(text, _)| *text == t("s_cz_geschlossen")));
        let overview = bench.page.overview.clone().expect("counted");
        let tile = overview.tile("executive_hangar");
        assert_eq!((tile.line.as_str(), tile.line_tone), (t("s_cz_geschlossen").as_str(), Tone::Bad), "{tile:?}");
        assert_eq!(tile.lamps.map(|l| l.iter().filter(|lamp| **lamp == Lamp::Green).count()), Some(2));
        assert!(tile.details[0].starts_with("öffnet in "), "{tile:?}");
        assert!(tile.details[1].starts_with("Öffnet um "), "{tile:?}");
        assert!(overview.hangar_ticking, "the tile counts every second");
        // The tile stands in the grid with its phase; a click on it leads to the page.
        let menu = bench.place("Bauplan-Liste", 0).x + 100.0;
        let at = texts.iter().find(|(text, at)| *text == t("s_cz_geschlossen") && at.x > menu).expect("tile").1;
        bench.click_at(at + egui::vec2(6.0, 6.0));
        assert_eq!(bench.page.page, "hf_contested");
        // Back on the overview, a second later the countdown is a second less.
        bench.page.open_overview();
        until(&mut bench, |bench, _| {
            bench.page.overview.as_ref().is_some_and(|o| o.tiles.contains_key("executive_hangar"))
        });
        let first = bench.page.overview.clone().expect("counted").tile("executive_hangar").details[0].clone();
        let mut later = first.clone();
        for _ in 0..80 {
            std::thread::sleep(std::time::Duration::from_millis(50));
            bench.settle();
            later = bench.page.overview.clone().expect("counted").tile("executive_hangar").details[0].clone();
            if later != first {
                break;
            }
        }
        assert_ne!(later, first, "counted anew without waiting for the ten seconds");
        let _ = std::fs::remove_dir_all(dir);

        // Without a start the tile says where to get one.
        let (page, dir) = start_page_in("kacheln-hangar-leer", "{}", &[]);
        let mut bench = Bench::new(page);
        bench.page.open_overview();
        until(&mut bench, |_, texts| texts.iter().any(|(text, _)| *text == t("s_uv_hangar_ohne")));
        let tile = bench.page.overview.clone().expect("counted").tile("executive_hangar");
        assert_eq!((tile.line.as_str(), tile.lamps), (t("s_uv_hangar_ohne").as_str(), None));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A click on the blueprint tile, with something received this week, opens the list at "7 days"; the list
    /// shows as many as the tile counts "this week" (the rows plus the note of what the catalog lacks), and the
    /// larger tile tells today and the month too. A week without a find opens the list as before.
    #[test]
    fn the_blueprint_tile_opens_the_list_at_the_week_it_counts() {
        let _language = crate::language_lock();
        let catalog = crate::blueprint_list::ListData::sample().catalog;
        let now = versekit_core::export::now_unix();
        let text_of = |seconds: i64| {
            let time = versekit_core::formats::local_time(seconds).expect("a local time");
            versekit_core::formats::strftime("%Y-%m-%d %H:%M:%S", &time)
        };
        let entry = |name: &str, ago: i64, source: &str| {
            format!(
                r#""{}": {{"name": "{name}", "quelle": "{source}", "zeit": "{}"}}"#,
                versekit_core::paths::name_key(name),
                text_of(now - ago)
            )
        };
        let names: Vec<&str> = catalog.blueprints.iter().map(|b| b.name.as_str()).collect();
        let day = 86_400;
        let entries = [
            entry(names[0], 600, "log"),
            entry(names[1], 2 * day, "basetool"),
            entry(names[2], 6 * day, "hand"),
            entry(names[3], 9 * day, "import"),
            entry("Unbekannter Entwurf", 1_200, "log"),
        ];
        let bestand = format!(r#"{{"bauplaene": {{{}}}}}"#, entries.join(","));
        let (page, dir) =
            start_page_in("kacheln-bauplan-woche", "{}", &[(versekit_core::collection::FILE_NAME, &bestand)]);
        let mut bench = Bench::new(page);
        bench.page.blueprints =
            crate::blueprint_list::ListData::load_with(&Paths::new(host_apart(&dir)), catalog.clone());
        bench.page.open_overview();
        let texts = until(&mut bench, |bench, _| {
            bench.page.overview.as_ref().is_some_and(|o| o.tiles.contains_key("bauplaene"))
        });
        let tile = bench.page.overview.clone().expect("counted").tile("bauplaene");
        assert_eq!(tile.details[0], tf("s_uv_woche", &[("0", Arg::from(4))]), "{tile:?}");
        // Today and the month by the calendar, so the clock may stand anywhere in the day or the month.
        let came_ago = [600, 2 * day, 6 * day, 9 * day, 1_200];
        let since = |period: versekit_core::overview::Period| period.since(now).expect("a start");
        let today = came_ago.iter().filter(|ago| now - **ago >= since(versekit_core::overview::Period::Today)).count();
        let month = came_ago.iter().filter(|ago| now - **ago >= since(versekit_core::overview::Period::Month)).count();
        assert_eq!(tile.details.last(), Some(&tf("s_uv_monat", &[("0", Arg::from(month))])), "{tile:?}");
        assert!(tile.details.contains(&tf("s_uv_heute", &[("0", Arg::from(today))])), "{tile:?}");
        let menu = bench.place("Bauplan-Liste", 0).x + 100.0;
        let at = texts.iter().find(|(text, at)| *text == tile.line && at.x > menu).expect("the line of the tile").1;
        bench.click_at(at + egui::vec2(6.0, 6.0));
        assert_eq!(bench.page.page, "hf_liste");
        let view = &bench.page.blueprint_view;
        assert_eq!(view.period, versekit_core::overview::Period::Days7);
        let rows = crate::blueprint_list::shown_keys(&bench.page.blueprints, view).len();
        let outside = {
            let since = view.period.since(versekit_core::export::now_unix()).expect("a start");
            bench.page.blueprints.received.iter().filter(|(_, (at, _))| *at >= since).count() - rows
        };
        assert_eq!((rows, outside), (3, 1));
        assert_eq!(rows + outside, 4, "the list holds what the tile counts");
        // The list names the period it was opened at.
        assert_eq!(view.jump.as_deref(), Some(t(crate::blueprint_list::period_key(view.period)).as_str()));
        let _ = std::fs::remove_dir_all(dir);

        // Nothing this week: the click leads to the progress page as before, the list stays unfiltered.
        let old = format!(r#"{{"bauplaene": {{{}}}}}"#, entry(names[0], 20 * day, "log"));
        let (page, dir) = start_page_in("kacheln-bauplan-alt", "{}", &[(versekit_core::collection::FILE_NAME, &old)]);
        let mut bench = Bench::new(page);
        bench.page.blueprints =
            crate::blueprint_list::ListData::load_with(&Paths::new(host_apart(&dir)), catalog.clone());
        bench.page.open_overview();
        let texts = until(&mut bench, |bench, _| {
            bench.page.overview.as_ref().is_some_and(|o| o.tiles.contains_key("bauplaene"))
        });
        let tile = bench.page.overview.clone().expect("counted").tile("bauplaene");
        let menu = bench.place("Bauplan-Liste", 0).x + 100.0;
        let at = texts.iter().find(|(text, at)| *text == tile.line && at.x > menu).expect("the line of the tile").1;
        bench.click_at(at + egui::vec2(6.0, 6.0));
        assert_eq!(bench.page.page, "hf_fortschritt");
        assert_eq!(bench.page.blueprint_view.period, versekit_core::overview::Period::All);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Every page that can be pinned has a tile that says something, in both languages: its figures, why it
    /// has none, or its short explanation; never nothing and never "comes later". The pages with data of their
    /// own show it, not their explanation.
    #[test]
    fn every_pinned_page_has_a_tile_that_says_something() {
        use versekit_core::overview::PAGES;
        let _language = crate::language_lock();
        let names: Vec<String> = PAGES.iter().map(|(name, _)| format!("\"{name}\"")).collect();
        let settings = format!("{{\"karten_angeheftet\": [{}], \"startseite_zuerst\": \"karten\"}}", names.join(", "));
        let raw = r#"{"format": 1, "posten": [{"material": "Gold", "menge": 12.5, "ort": "Area18"}]}"#;
        let ships = r#"{"format": 1, "schiffe": [{"name": "Carrack", "lti": true, "belegung": {"a": {"ref": "x", "name": "X", "weg": "kaufen"}}}],
            "wunsch": [{"name": "Polaris", "hersteller": "RSI", "belegung": {}}, {"name": "Idris", "hersteller": "Aegis", "belegung": {}}],
            "merkzettel": [{"name": "Frostline", "anzahl": 2}, {"name": "Arc Shield", "anzahl": 1, "weg": "kaufen"}, {"name": "Done Item", "anzahl": 1, "erledigt": true}]}"#;
        let (page, dir) = start_page_in(
            "kacheln-alle",
            &settings,
            &[(versekit_core::stock::RAW_FILE, raw), (versekit_core::hangar::FILE, ships)],
        );
        let mut bench = Bench::new(page);
        bench.page.open_overview();
        bench.settle();
        let pinned: Vec<String> = bench.page.tiles.iter().filter(|t| t.pinned).map(|t| t.key.clone()).collect();
        assert!(pinned.len() >= 30, "most pages can be pinned: {pinned:?}");
        assert!(pinned.iter().any(|key| key == "hf_handlesuche"), "the handle search has a pin: {pinned:?}");
        assert!(!pinned.iter().any(|key| key == "hf_beziehungen"), "the relations have no pin of their own");
        for choice in [language::Choice::De, language::Choice::En] {
            language::set_thread_language(choice);
            bench.page.overview = None;
            bench.settle();
            let overview = bench.page.overview.clone().expect("counted");
            for key in &pinned {
                let tile = overview.tile(key);
                assert!(!tile.line.trim().is_empty(), "{choice:?} {key}: no line");
                assert_ne!(tile.line, super::super::glance::NOT_YET, "{choice:?} {key} comes later");
                assert!(!tile.line.contains('{'), "{choice:?} {key}: {}", tile.line);
            }
            // Pages with data of their own show it.
            // The stock of Gold, and the place it lies at.
            assert_eq!(overview.tile("hf_meinlager").details.len(), 2, "{:?}", overview.tile("hf_meinlager"));
            assert!(overview.tile("hf_hangar").details.iter().any(|d| d == "Carrack"));
            // The planned ships of the hangar page are no watch list of blueprints: its tile names them
            // after the own ships, A to Z, and says how many there are.
            let hangar = overview.tile("hf_hangar");
            let planned = |name: &str| tf("s_kt_hangar_geplant_schiff", &[("0", Arg::from(name))]);
            assert!(hangar.details.contains(&tf("s_kt_hangar_geplant", &[("0", Arg::from(2))])), "{hangar:?}");
            let named: Vec<&String> =
                hangar.details.iter().filter(|d| d.contains("Idris") || d.contains("Polaris")).collect();
            assert_eq!(named, [&planned("Idris"), &planned("Polaris")], "{hangar:?}");
            assert_ne!(hangar.line, overview.tile("merkliste").line);
            // The page the wishlist was has no card of its own any more.
            assert!(!bench.page.tiles.iter().any(|t| t.key == "hf_wunschliste"));
            // The crafting plan: what is noted and what is to be done with it.
            let plan = overview.tile("hf_herstellung");
            assert_eq!(plan.details.len(), 3, "{plan:?}");
            assert!(plan.line.contains('3'), "{plan:?}");
            assert!(plan.details.iter().any(|d| d.starts_with("Frostline ×2")), "{plan:?}");
            // The farm list needs the recipes, which load beside the window: until then it says so.
            assert!(!overview.tile("hf_farmliste").line.is_empty());
            assert!(overview.tile("hf_einkaufsliste").line.contains('1'), "{:?}", overview.tile("hf_einkaufsliste"));
            for key in ["hf_st_uebersicht", "hf_verkauf", "hf_routen", "hf_laeden", "hf_serverstatus", "hf_spiel"] {
                assert_ne!(
                    overview.tile(key).line,
                    t(&format!("k_{}", &key[3..])),
                    "{key} shows its data, not its explanation"
                );
            }
        }
        language::set_thread_language(language::Choice::De);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The usage counters of 4.12 (`page_usage`) count what the window does: the start, the way to
    /// every page, the clicks on the way including a folded group, the gallery, and the close.
    #[test]
    fn the_window_counts_pages_ways_and_clicks_as_4_12() {
        use versekit_core::jsonio::{self, Value};
        use versekit_core::usage::PageUsage;
        let _language = crate::language_lock();
        let settings = r#"{"karten_angeheftet": ["hangar"], "startseite_zuerst": "karten"}"#;
        let (mut page, dir) = start_page_in("nutzung", settings, &[]);
        assert!(page.usage.is_some(), "real data count");
        // Counting needs the network allowed; nothing here sends.
        let host = || super::super::real_data::host_in(&dir, &[(versekit_core::legacy::ENV_NO_NET, "0")]);
        assert!(!PageUsage::new(Paths::new(host()), "5.0.0-dev").active(), "a build of the source counts nothing");
        let counter = std::sync::Arc::new(PageUsage::new(Paths::new(host()), "5.0.0"));
        page.usage = Some(std::sync::Arc::clone(&counter));
        let mut bench = Bench::new(page);
        bench.settle();
        let menu = bench.place(&t("hf_liste"), 0);
        bench.click_at(menu);
        assert_eq!(bench.page.page, "hf_liste");
        // A group of the side bar folded on the way, then back to the overview by the side bar.
        bench.click(&t("hf_gruppe_handel").to_uppercase());
        bench.click_nth(&t("hf_uebersicht"), 0);
        assert_eq!(bench.page.page, "");
        // A click on the pinned hangar card.
        let texts = bench.settle();
        let title =
            texts.iter().find(|(text, at)| *text == t("hf_hangar") && at.x > menu.x + 100.0).expect("hangar card").1;
        bench.click_at(title);
        assert_eq!(bench.page.page, "hf_hangar");
        bench.settle();
        // The gallery opened by the program counts as opened, its page not as a way.
        bench.page.go_to("hf_wasistneu");
        bench.settle();
        bench.page.count_window_end(Some(1.0e6));
        let stored = counter.stored();
        let field = |name: &str| {
            let value = Value::Object(stored.clone());
            jsonio::dumps(value.get(name).expect("field"), None, false)
        };
        assert_eq!(field("pages"), r#"{"uebersicht": 2, "liste": 1, "hangar": 1}"#);
        assert_eq!(
            field("entry"),
            r#"{"uebersicht": {"start": 1, "seitenleiste": 1}, "liste": {"seitenleiste": 1}, "hangar": {"karte": 1}}"#
        );
        assert_eq!(field("actions"), r#"{"galerie_offen": 1}"#);
        // Four clicks to the hangar (list, group, overview, card), however they split into targets.
        let Some(Value::Object(clicks)) = Value::Object(stored.clone()).get("clicks").cloned() else { panic!() };
        let steps: i64 = clicks
            .iter()
            .flat_map(|(_, inner)| inner.as_object().cloned().unwrap_or_default())
            .map(|(step, n)| step.parse::<i64>().unwrap() * jsonio::dumps(&n, None, false).parse::<i64>().unwrap())
            .sum();
        assert_eq!(steps, 4, "{clicks:?}");
        assert!(clicks.iter().any(|(page, _)| page == "hangar"), "the page open last is a target: {clicks:?}");
        // A new window starts on the page it shows; the window asked to close ends the way, and a
        // page opened after it counts no more.
        bench.page.usage_started = false;
        bench.settle();
        bench.close_request();
        bench.click_at(menu);
        assert_eq!(bench.page.page, "hf_liste");
        let pages = jsonio::dumps(Value::Object(counter.stored()).get("pages").expect("pages"), None, false);
        assert_eq!(pages, r#"{"uebersicht": 2, "liste": 1, "hangar": 1, "wasistneu": 1}"#);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The menu filter counts once per search as used and, when the menu shows nothing, once as without hits; a page
    /// opened from it counts the way „suche“. The word searched for is never kept.
    #[test]
    fn the_menu_filter_counts_each_search_once_and_its_pages_by_the_way_of_the_search() {
        use versekit_core::jsonio::{self, Value};
        use versekit_core::usage::PageUsage;
        let _language = crate::language_lock();
        let (mut page, dir) = start_page_in("suche-zaehlen", "", &[]);
        let host = || super::super::real_data::host_in(&dir, &[(versekit_core::legacy::ENV_NO_NET, "0")]);
        let counter = std::sync::Arc::new(PageUsage::new(Paths::new(host()), "5.0.0"));
        page.usage = Some(std::sync::Arc::clone(&counter));
        let mut bench = Bench::new(page);
        bench.settle();
        // One search: a word comes, the hit is clicked, then a word that finds nothing, all without emptying.
        bench.page.nav_filter = "Hangar".into();
        bench.settle();
        bench.click_nth(&t("hf_hangar"), 0);
        assert_eq!(bench.page.page, "hf_hangar");
        bench.page.nav_filter = "Hangarqqq".into();
        bench.settle();
        bench.settle();
        // Emptied, then a second search that finds nothing.
        bench.page.nav_filter.clear();
        bench.settle();
        bench.page.nav_filter = "zzqqx".into();
        bench.settle();
        bench.page.count_window_end(Some(1.0e6));
        let stored = Value::Object(counter.stored());
        let field = |name: &str| jsonio::dumps(stored.get(name).expect("field"), None, false);
        assert_eq!(field("actions"), r#"{"suche_benutzt": 2, "suche_ohne_treffer": 2}"#);
        assert!(field("entry").contains(r#""hangar": {"suche": 1}"#), "{}", field("entry"));
        let all = jsonio::dumps(&stored, None, false);
        assert!(!all.contains("Hangarqqq") && !all.contains("zzqqx"), "never the word: {all}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The pages "Was steckt drin?" and "Lohnt das Zerlegen?" are one page now: every old way leads to it, and
    /// the usage counters keep telling the wreck and the calculation of a part apart.
    #[test]
    fn the_old_routes_to_the_two_pages_land_on_the_merged_page_and_count_by_view() {
        use versekit_core::jsonio::{self, Value};
        use versekit_core::usage::PageUsage;
        let _language = crate::language_lock();
        // Pinned under both old names: one card.
        let settings = r#"{"karten_angeheftet": ["zerlegen", "bergung"], "startseite_zuerst": "karten"}"#;
        let (mut page, dir) = start_page_in("zerlegen-bergen", settings, &[]);
        let host = || super::super::real_data::host_in(&dir, &[(versekit_core::legacy::ENV_NO_NET, "0")]);
        let counter = std::sync::Arc::new(PageUsage::new(Paths::new(host()), "5.0.0"));
        page.usage = Some(std::sync::Arc::clone(&counter));
        let mut bench = Bench::new(page);
        bench.page.open_overview();
        bench.settle();
        let paths = Paths::new(bench.page.store.as_ref().expect("real data").paths.host.clone());
        assert_eq!(versekit_core::overview::pinned(&paths), ["bergung"], "both old names are the one page");
        let cards = |key: &str| bench.page.tiles.iter().filter(|t| t.key == key && t.pinned).count();
        assert_eq!((cards("hf_bergung"), cards("hf_zerlegen")), (1, 0));
        // What a page was counted as: pages, and the way it was reached.
        let counted = |counter: &PageUsage, field: &str, page: &str| -> i64 {
            let stored = Value::Object(counter.stored());
            let inner = stored.get(field).and_then(|f| f.get(page)).cloned().unwrap_or(Value::Null);
            jsonio::dumps(&inner, None, false).parse().unwrap_or(0)
        };
        let ways = |counter: &PageUsage, page: &str| -> i64 {
            let stored = Value::Object(counter.stored());
            let inner = stored.get("entry").and_then(|f| f.get(page)).and_then(|f| f.get("sprung")).cloned();
            jsonio::dumps(&inner.unwrap_or(Value::Null), None, false).parse().unwrap_or(0)
        };
        // The wreck: counted as "bergung".
        bench.page.usage_route = Some("sprung");
        bench.page.go_to("hf_bergung");
        bench.settle();
        assert_eq!(bench.page.page, "hf_bergung");
        assert_eq!((counted(&counter, "pages", "bergung"), counted(&counter, "pages", "zerlegen")), (1, 0));
        // Frames without a change count nothing more.
        bench.settle();
        assert_eq!(counted(&counter, "pages", "bergung"), 1);
        // A part opens on the same page: the other view, counted as a jump to "zerlegen".
        bench.page.salvage.open_part("Frostline");
        bench.settle();
        assert_eq!(bench.page.page, "hf_bergung");
        assert_eq!((counted(&counter, "pages", "bergung"), counted(&counter, "pages", "zerlegen")), (1, 1));
        assert_eq!(ways(&counter, "zerlegen"), 1);
        bench.settle();
        assert_eq!(counted(&counter, "pages", "zerlegen"), 1);
        // The old page by its old key (a stored last page, a pinned card, a jump): the merged page opens, the
        // view that stands is the one counted.
        bench.page.salvage.clear();
        bench.page.usage_route = Some("sprung");
        bench.page.go_to("hf_laeden");
        bench.settle();
        bench.page.usage_route = Some("karte");
        bench.page.go_to("hf_zerlegen");
        bench.settle();
        assert_eq!(bench.page.page, "hf_bergung", "no page of its own any more");
        assert_eq!(bench.page.salvage.usage_view(), "zerlegen");
        assert_eq!(counted(&counter, "pages", "zerlegen"), 2, "the old route counts as the calculation");
        assert_eq!(counted(&counter, "pages", "bergung"), 1);
        // The old names find the page in the menu filter.
        for typed in ["was steckt drin", "Lohnt das Zerlegen", "Wrack", "worth dismantling"] {
            let hits = crate::sample::nav_hits(typed, "hf_gruppe_abbau", "hf_bergung");
            assert!(hits.contains(&t("hf_bergung")), "{typed}: {hits:?}");
        }
        assert!(crate::sample::nav_hits("Wrack", "hf_gruppe_abbau", "hf_bergbau").is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The layout of the start page as 4.12 keeps it: read at the start, nothing written by reading,
    /// every change written under the keys of 4.12 (the size and the single area under keys of v5),
    /// and the next start shows the same.
    #[test]
    fn the_start_page_keeps_its_layout_under_the_keys_of_4_12() {
        use crate::tiles;
        use versekit_core::overview;
        let _language = crate::language_lock();
        // The unit storage is hidden without a connection: its pin stays where it is. The tile and the
        // page "uebersetzung" keep sizes of their own.
        let settings = r#"{"uebersicht_reihenfolge": ["lager", "spiel"], "uebersicht_aus": ["rechner"], "karten_angeheftet": ["hangar", "staffellager", "uebersetzung"], "startseite_zuerst": "karten", "uebersicht_groessen": {"lager": "breit", "uebersetzung": "hoch", "seite:uebersetzung": "gross"}}"#;
        let (page, dir) = start_page_in("kacheln-merken", settings, &[]);
        let mut bench = Bench::new(page);
        bench.page.open_overview();
        bench.settle();
        let file = dir.join("einstellungen.json");
        let read = || std::fs::read_to_string(&file).unwrap();
        let before = read();
        assert!(
            before.contains("\"uebersicht_reihenfolge\": [\"lager\", \"spiel\"]"),
            "reading writes nothing: {before}"
        );
        assert!(!before.contains("uebersicht_ein_bereich"), "{before}");
        let tiles = &bench.page.tiles;
        assert_eq!((tiles[0].key.as_str(), tiles[0].size), ("lager", tiles::TileSize::Wide));
        assert_eq!(tiles[1].key, "spiel");
        assert!(tiles.iter().any(|t| t.key == "rechner" && t.hidden));
        assert!(tiles.iter().any(|t| t.key == "hf_hangar" && t.pinned));
        assert!(!tiles.iter().any(|t| t.key == "hf_staffellager"), "its page is hidden");
        assert!(tiles.iter().any(|t| t.key == "uebersetzung" && t.size == tiles::TileSize::Tall));
        assert!(tiles.iter().any(|t| t.key == "hf_uebersetzung" && t.pinned && t.size == tiles::TileSize::Large));
        assert!(bench.page.mine_first);

        // Change all of it, as the board, the pin and the settings page do.
        let at = |tiles: &[tiles::Tile], key: &str| tiles.iter().position(|t| t.key == key).unwrap();
        let (lager, spiel) = (at(&bench.page.tiles, "lager"), at(&bench.page.tiles, "spiel"));
        tiles::swap_tiles(&mut bench.page.tiles, lager, spiel);
        let funde = at(&bench.page.tiles, "funde");
        bench.page.tiles[funde].hidden = true;
        let spiel = at(&bench.page.tiles, "spiel");
        bench.page.tiles[spiel].size = tiles::TileSize::Large;
        bench.page.tiles.push(tiles::Tile::pinned("hf_routen"));
        bench.page.mine_first = false;
        bench.page.overview_two = false;
        bench.settle();
        let paths = Paths::new(bench.page.store.as_ref().unwrap().paths.host.clone());
        assert_eq!(&overview::tile_order(&paths)[..2], ["spiel", "lager"]);
        assert_eq!(overview::hidden_tiles(&paths), ["rechner", "funde"]);
        assert_eq!(overview::pinned(&paths), ["hangar", "staffellager", "uebersetzung", "routen"]);
        assert!(!overview::cards_first(&paths));
        assert_eq!(paths.setting(overview::FIRST_SETTING).as_deref(), Some("kacheln"));
        let mut sizes = overview::sizes(&paths);
        sizes.sort();
        let size = |id: &str, name: &str| (id.to_string(), name.to_string());
        assert_eq!(
            sizes,
            [
                size("lager", "breit"),
                size("seite:uebersetzung", "gross"),
                size("spiel", "gross"),
                size("uebersetzung", "hoch")
            ]
        );
        assert!(paths.setting_bool(overview::ONE_AREA_SETTING, false));

        // The next start lays the page out the same.
        let again = SamplePage::with_settings(Paths::new(paths.host.clone()));
        assert_eq!(again.tiles, bench.page.tiles);
        assert!(!again.mine_first && !again.overview_two);

        // Loosened and pinned again, a page comes at the end (`cards.set_pinned`).
        let hangar = at(&bench.page.tiles, "hf_hangar");
        bench.page.tiles.remove(hangar);
        bench.settle();
        bench.page.tiles.push(tiles::Tile::pinned("hf_hangar"));
        bench.settle();
        assert_eq!(overview::pinned(&paths), ["staffellager", "uebersetzung", "routen", "hangar"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// From the factory the play time tile is hidden (it is off in the head bar too); hiding another
    /// tile writes both, as 4.12 does.
    #[test]
    fn the_hidden_play_time_of_the_factory_is_written_with_the_first_hidden_tile() {
        let _language = crate::language_lock();
        let (page, dir) = start_page_in("kacheln-werk", "{}", &[]);
        let mut bench = Bench::new(page);
        bench.page.open_overview();
        bench.settle();
        assert!(bench.page.tiles.iter().any(|t| t.key == "spielzeit" && t.hidden));
        let funde = bench.page.tiles.iter().position(|t| t.key == "funde").unwrap();
        bench.page.tiles[funde].hidden = true;
        bench.settle();
        let paths = Paths::new(bench.page.store.as_ref().unwrap().paths.host.clone());
        assert_eq!(versekit_core::overview::hidden_tiles(&paths), ["spielzeit", "funde"]);
        // Written, not only derived: with the play time in the head bar it stays hidden.
        paths.set_setting(versekit_core::overview::PLAYTIME_HEAD_SETTING, versekit_core::jsonio::Value::Bool(true));
        assert_eq!(versekit_core::overview::hidden_tiles(&paths), ["spielzeit", "funde"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The "Durchsuchen …" of the game folders, top to bottom: LIVE, PTU, TECH-PREVIEW (the one of
    /// the data folder stands in the other column, further right; the order they are drawn in is
    /// another one).
    fn game_folder_buttons(bench: &mut Bench) -> Vec<Pos2> {
        let column = bench.place("LIVE", 0).x + 100.0;
        let mut browse: Vec<Pos2> = bench
            .settle()
            .into_iter()
            .filter(|(text, at)| text == "Durchsuchen …" && at.x < column)
            .map(|(_, at)| at)
            .collect();
        browse.sort_by(|a, b| a.y.total_cmp(&b.y));
        // One for every channel: LIVE, HOTFIX, PTU, EPTU, TECH-PREVIEW.
        assert_eq!(browse.len(), versekit_core::paths::CHANNELS.len(), "{browse:?}");
        browse
    }

    /// The folder of another channel can be chosen (new in v5: 4.12 only finds it next to LIVE): it is
    /// kept in `kanal_ordner` and counts for that channel; an empty field takes the choice back.
    #[test]
    fn another_channel_can_be_given_its_own_folder() {
        let _language = crate::language_lock();
        // An entry for another channel, whose folder is not there, stays as it is.
        let (page, dir) = real_page("kanaele", r#"{"kanal_ordner": {"HOTFIX": "Z:/Spiele/HOTFIX"}}"#, &[]);
        let mut bench = Bench::new(page);
        bench.page.choose_folder = chooses;
        bench.page.go_to("hf_ordner");
        let browse = game_folder_buttons(&mut bench);
        // The rows in the order of the channels: PTU is the third.
        bench.click_at(browse[2]);
        bench.settle();
        assert_eq!(asked().len(), 1, "the dialog was asked for the PTU folder");
        let chosen = chosen_folder();
        assert_eq!(bench.page.folders[2], chosen, "the choice stands in the field");
        let paths = Paths::new(bench.page.store.as_ref().unwrap().paths.host.clone());
        // Read back without the separator at the end, as every folder of a channel.
        let saved = chosen.trim_end_matches(['\\', '/']).to_string();
        assert_eq!(paths.chosen_channel_folders(), [("PTU".to_string(), saved.clone())], "and is saved");
        assert!(paths.installed_channels().iter().any(|c| c.0 == "PTU" && c.1 == saved), "and counts");
        let stored = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap_or_default();
        assert!(stored.contains("Z:/Spiele/HOTFIX"), "the other entry stays: {stored}");

        // A folder that is not there is not taken, and said.
        let gone = dir.join("gibt-es-nicht").to_string_lossy().into_owned();
        bench.page.folders[2].clone_from(&gone);
        bench.page.commit_game_folder(2);
        bench.settle();
        assert_eq!(bench.notice(), Some(tf("s_bs_ordner_nein", &[("0", Arg::from(gone.as_str()))])));
        assert_eq!(paths.chosen_channel_folders(), [("PTU".to_string(), saved.clone())], "the choice stays");
        // Nor a folder that is the game folder already: the field shows the choice again.
        let live = dir.to_string_lossy().into_owned();
        bench.page.folders[0].clone_from(&live);
        bench.page.folders[2].clone_from(&live);
        bench.page.commit_game_folder(2);
        bench.settle();
        assert_eq!(bench.notice(), Some(tf("s_pf_kanal_doppelt", &[("0", Arg::from("LIVE"))])));
        assert_eq!(bench.page.folders[2], chosen);
        assert_eq!(paths.chosen_channel_folders(), [("PTU".to_string(), saved.clone())], "the choice stays");

        // An empty field: the folder next to LIVE again, the choice gone.
        bench.page.folders[2].clear();
        bench.page.commit_game_folder(2);
        bench.settle();
        assert!(paths.chosen_channel_folders().is_empty());
        assert_eq!(bench.page.folders[2], bench.page.channels_found[2]);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// With a game folder given from outside (a test copy) the other channels are the copy's: their
    /// rows are locked with the reason, and no dialog opens.
    #[test]
    fn the_channels_of_a_given_copy_are_not_chosen() {
        let _language = crate::language_lock();
        let copy = folder("kanaele-kopie-spiel");
        let live = copy.join("LIVE");
        std::fs::create_dir_all(&live).unwrap();
        std::fs::write(live.join("Game.log"), "").unwrap();
        let live = live.to_string_lossy().into_owned();
        let (page, dir) = real_page("kanaele-kopie", "", &[("SC_INSTALL_DIR", &live)]);
        let mut bench = Bench::new(page);
        bench.page.choose_folder = chooses;
        bench.page.go_to("hf_ordner");
        let browse = game_folder_buttons(&mut bench);
        bench.click_at(browse[1]);
        bench.click_at(browse[2]);
        assert!(asked().is_empty(), "no dialog for a channel of the copy");
        bench.frame(vec![Event::PointerMoved(browse[1] + egui::vec2(3.0, 3.0))]);
        let reason = t("s_pf_gesperrt");
        let texts = until(&mut bench, |_, texts| texts.iter().any(|(text, _)| *text == reason));
        assert!(texts.iter().any(|(text, _)| *text == reason), "{texts:?}");
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(copy);
    }

    #[test]
    fn a_folder_typed_is_saved_when_the_field_is_left_and_not_with_every_key() {
        let _language = crate::language_lock();
        let (page, dir) = real_page("tippen", "", &[]);
        let mut bench = Bench::new(page);
        bench.page.go_to("hf_ordner");
        bench.settle();
        // The game folder as the settings file has it. Other keys come and go (the account found in
        // the logs of an installation on this computer is written with the whole file), and the
        // field starts with the folder found there, so emptying it saves an empty folder.
        let settings = || {
            let text = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap_or_default();
            let data = versekit_core::jsonio::loads(&text).ok()?;
            let folder = data.as_object()?.iter().find(|(key, _)| key == "spiel_ordner")?.1.as_str()?.to_string();
            Some(folder).filter(|folder| !folder.is_empty())
        };
        // An empty field, so that what is typed is the whole path.
        bench.page.folders[0].clear();
        bench.page.live_folder_saved.clear();
        let enter = |pressed| Event::Key {
            key: egui::Key::Enter,
            physical_key: Some(egui::Key::Enter),
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let type_in = |bench: &mut Bench, text: &str| {
            // A click puts the cursor in the field (it shows "Ordner" while empty), keys type.
            let at = bench.place("Ordner", 0);
            bench.click_at(at);
            bench.frame(vec![Event::Text(text.to_string())]);
            bench.settle();
        };
        // A folder that is there: typing changes the field and nothing else, Enter makes it count.
        let there = dir.join("ohne-spiel");
        std::fs::create_dir_all(&there).unwrap();
        let there = there.to_string_lossy().into_owned();
        type_in(&mut bench, &there);
        assert_eq!(bench.page.folders[0], there, "the keys went into the field");
        assert_eq!(settings(), None, "a half typed path is not saved");
        bench.frame(vec![enter(true)]);
        bench.frame(vec![enter(false)]);
        searched(&mut bench.page);
        bench.settle();
        assert!(settings().is_some_and(|folder| folder.replace('\\', "/") == there.replace('\\', "/")));
        // The box above says what the program finds there now: no Game.log in it.
        assert_eq!(bench.page.game_found, None);
        // A folder that is not there is not taken, and the notice says so.
        bench.page.folders[0].clear();
        let before = settings();
        type_in(&mut bench, "/gibt/es/nicht");
        bench.frame(vec![enter(true)]);
        bench.frame(vec![enter(false)]);
        bench.settle();
        assert_eq!(settings(), before, "a folder that is not there is not saved");
        assert!(bench.notice().is_some_and(|t| t.contains("/gibt/es/nicht")), "{:?}", bench.notice());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_folder_with_a_game_log_shows_as_found_once_it_is_set() {
        let _language = crate::language_lock();
        let (mut page, dir) = real_page("gefunden", "", &[]);
        let game = dir.join("StarCitizen").join("LIVE");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("Game.log"), "x").unwrap();
        page.folders[0] = game.to_string_lossy().into_owned();
        page.commit_game_folder(0);
        assert_eq!(page.game_found.as_deref(), Some(&*game.to_string_lossy()), "the box says found");
        // A folder without a Game.log in it or below it: the box does not say found.
        let elsewhere = dir.join("woanders");
        std::fs::create_dir_all(&elsewhere).unwrap();
        page.folders[0] = elsewhere.to_string_lossy().into_owned();
        page.commit_game_folder(0);
        searched(&mut page);
        assert_eq!(page.game_found, None, "no Game.log in it");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_count_of_the_errors_follows_the_error_log_while_the_window_is_open() {
        let _language = crate::language_lock();
        let (mut page, dir) = real_page("fehlerzahl", "", &[]);
        assert_eq!(page.error_count, 0);
        let paths = Paths::new(super::super::real_data::host_in(&dir, &[]));
        let log = versekit_core::error_log::ErrorLog::new(&paths, env!("CARGO_PKG_VERSION"));
        let problem = |message| Some(versekit_core::error_log::Problem { kind: "Test", message, trace: None });
        // An old entry of the file and one of an earlier version are no new errors.
        assert!(log.record_at("test.alt", problem("alt"), "", "2026-01-01 00:00:01"));
        assert!(versekit_core::error_log::ErrorLog::new(&paths, "4.0.0").record_at(
            "test.frueher",
            problem("frueher"),
            "",
            "2999-01-01 00:00:01"
        ));
        assert!(log.record_at("test.eins", problem("eins"), "", "2026-01-01 00:00:02"));
        page.follow_errors(10.0);
        assert_eq!(page.error_count, 2, "the entries of this version are counted, not the earlier version");
        assert!(log.record_at("test.zwei", problem("zwei"), "", "2026-01-01 00:00:03"));
        page.follow_errors(10.5);
        assert_eq!(page.error_count, 2, "not looked at again within two seconds");
        page.follow_errors(13.0);
        assert_eq!(page.error_count, 3);
        // Opening the error page counts as looking: nothing is new any more.
        page.go_to("hf_diagnose");
        let mut bench = Bench::new(page);
        bench.frame(Vec::new());
        assert_eq!(bench.page.error_count, 0, "after looking the count is zero");
        bench.page.errors_looked = None;
        bench.frame(Vec::new());
        assert_eq!(bench.page.error_count, 0, "and it stays zero when the file is read again");
        // Only what happens later counts again.
        assert!(log.record_at("test.drei", problem("drei"), "", "2999-01-01 00:00:04"));
        bench.page.errors_looked = None;
        bench.frame(Vec::new());
        assert_eq!(bench.page.error_count, 0, "the page is still open and has seen it");
        bench.page.go_to("hf_ueber");
        bench.page.errors_looked = None;
        bench.frame(Vec::new());
        assert_eq!(bench.page.error_count, 1, "an error written after the look is counted");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_hotkey_without_a_modifier_key_is_refused_with_the_reason_and_the_saved_one_stays() {
        let _language = crate::language_lock();
        let (page, dir) = real_page("kuerzel", "{\n  \"hotkey\": \"Strg+Alt+B\"\n}\n", &[]);
        let mut bench = Bench::new(page);
        bench.page.go_to("hf_overlay");
        let before = bench.page.hotkey_modifiers;
        assert_eq!(before, [true, true, false], "Strg+Alt as stored");
        // Strg off: Alt is left, fine. Alt off as well: nothing would be left.
        bench.click("Strg");
        assert_eq!(bench.page.hotkey_modifiers, [false, true, false]);
        bench.click("Alt");
        assert_eq!(bench.page.hotkey_modifiers, [false, true, false], "the saved shortcut stays");
        let text = bench.notice().expect("the reason is said");
        assert!(text.contains("Alt+") || text.contains("Umschalt") || text.contains("Strg"), "{text}");
        assert!(text.contains(&hotkey_text([false, true, false], bench.page.hotkey_key).unwrap()), "{text}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// F13 needs no modifier (Kuehlwagen): the last one switched off leaves F13 by itself, which is kept, saved and
    /// said as registered; switched on again, it is a combination as before.
    #[test]
    fn a_function_key_by_itself_is_kept_saved_and_registered() {
        let _language = crate::language_lock();
        let (page, dir) = real_page("kuerzel-allein", "{\n  \"hotkey\": \"Alt+F13\"\n}\n", &[]);
        let mut bench = Bench::new(page);
        bench.page.overlay_runs = || true;
        bench.page.go_to("hf_overlay");
        assert_eq!(hotkey_text(bench.page.hotkey_modifiers, bench.page.hotkey_key).as_deref(), Some("Alt+F13"));
        bench.click("Alt");
        bench.settle();
        assert_eq!(bench.page.hotkey_modifiers, [false; 3], "nothing pressed, and it stays so");
        assert_eq!(hotkey_text(bench.page.hotkey_modifiers, bench.page.hotkey_key).as_deref(), Some("F13"));
        let said = bench.notice();
        assert_eq!(
            said,
            Some(language::tf("s_hk_ok", &[("0", language::Arg::from("F13"))])),
            "registered, not refused"
        );
        let stored = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap_or_default();
        assert!(stored.contains("\"hotkey\": \"F13\""), "{stored}");
        bench.click(&language::t("s_ei_taste_strg"));
        bench.settle();
        assert_eq!(hotkey_text(bench.page.hotkey_modifiers, bench.page.hotkey_key).as_deref(), Some("Strg+F13"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_parts_shown_in_the_overlay_stand_on_the_page_are_saved_and_reach_the_overlay() {
        use versekit_core::overlay::{Section, Sections};
        let _language = crate::language_lock();
        let (page, dir) = real_page("anzeigen", "", &[]);
        let mut bench = Bench::new(page);
        bench.page.go_to("hf_overlay");
        assert_eq!(bench.page.overlay_sections, Sections::default(), "nothing written: everything shows");
        for lang in [language::Choice::De, language::Choice::En] {
            language::set_thread_language(lang);
            let wheel = |dy: f32| egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, dy),
                modifiers: egui::Modifiers::NONE,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
            };
            bench.frame(vec![egui::Event::PointerMoved(Pos2::new(800.0, 450.0))]);
            for _ in 0..60 {
                bench.frame(vec![wheel(120.0)]);
            }
            let mut texts = bench.settle();
            for _ in 0..12 {
                for _ in 0..5 {
                    bench.frame(vec![wheel(-120.0)]);
                }
                texts.extend(bench.settle());
            }
            for key in [
                "s_ov_anzeigen",
                "s_ov_zeige_server",
                "s_ov_zeige_ort",
                "s_ov_zeige_auftraege",
                "s_ov_zeige_raffinerie",
                "s_ov_zeige_werkbank",
                "s_ov_zeige_update",
                "s_ov_auftraege_max",
            ] {
                let text = language::t(key).to_lowercase();
                assert!(texts.iter().any(|(t, _)| t.to_lowercase() == text), "{key} ({text}) is not on the page");
            }
        }
        language::set_thread_language(language::Choice::De);
        // The contracts off: written to the file, read back by a new page and by the overlay.
        bench.page.overlay_sections = Sections::default().with(Section::Contracts, false);
        bench.page.overlay_contracts_max = 12;
        bench.settle();
        let file = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap();
        assert!(file.contains("\"overlay_zeige_auftraege\": false"), "{file}");
        assert!(file.contains("\"overlay_auftraege_hoechstzahl\": 12"), "{file}");
        assert!(!file.contains("overlay_zeige_ort"), "only what changed is written: {file}");
        let again = SamplePage::with_settings(Paths::new(super::super::real_data::host_in(&dir, &[])));
        assert_eq!(again.overlay_sections, bench.page.overlay_sections);
        let read = versekit_core::overlay::Settings::read(&Paths::new(super::super::real_data::host_in(&dir, &[])));
        assert!(!read.sections.contains(Section::Contracts) && read.sections.contains(Section::Location));
        assert_eq!((again.overlay_contracts_max, read.contracts_max), (12, 12));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_events_that_open_the_overlay_stand_on_the_page_are_saved_and_come_back() {
        use versekit_core::overlay::{Event, Events};
        let _language = crate::language_lock();
        let (page, dir) = real_page("ereignisse", "", &[]);
        let mut bench = Bench::new(page);
        bench.page.go_to("hf_overlay");
        assert_eq!(bench.page.overlay_events, Events::default(), "nothing written: only the blueprint");
        // Every event has its switch on the page, in German and in English.
        for lang in [language::Choice::De, language::Choice::En] {
            language::set_thread_language(lang);
            // The group stands below the visible part: from the top, scrolled down step by step, what showed up is
            // collected.
            let wheel = |dy: f32| egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, dy),
                modifiers: egui::Modifiers::NONE,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
            };
            bench.frame(vec![egui::Event::PointerMoved(Pos2::new(800.0, 450.0))]);
            for _ in 0..60 {
                bench.frame(vec![wheel(120.0)]);
            }
            let mut texts = bench.settle();
            // Steps shorter than the window, so that no part of the page is scrolled past unseen.
            for _ in 0..12 {
                for _ in 0..5 {
                    bench.frame(vec![wheel(-120.0)]);
                }
                texts.extend(bench.settle());
            }
            for key in [
                "s_ov_ereignisse",
                "s_ov_bei_bauplan",
                "s_ov_bei_scanner",
                "s_ov_bei_auftrag",
                "s_ov_bei_raffinerie",
                "s_ov_bei_katalog",
            ] {
                let text = language::t(key).to_lowercase();
                assert!(texts.iter().any(|(t, _)| t.to_lowercase() == text), "{key} ({text}) is not on the page");
            }
        }
        language::set_thread_language(language::Choice::De);
        // Switch the scanner on and the blueprint off: written to the file, and read back by a new page.
        bench.page.overlay_events = Events::default().with(Event::Scanner, true).with(Event::Blueprint, false);
        bench.settle();
        let file = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap();
        assert!(file.contains("\"overlay_bei_scanner\": true"), "{file}");
        assert!(file.contains("\"overlay_bei_bauplan\": false"), "{file}");
        assert!(!file.contains("overlay_bei_raffinerie"), "only what changed is written: {file}");
        let again = SamplePage::with_settings(Paths::new(super::super::real_data::host_in(&dir, &[])));
        assert_eq!(again.overlay_events, bench.page.overlay_events);
        // And it is the choice the overlay reads.
        let read = versekit_core::overlay::Settings::read(&Paths::new(super::super::real_data::host_in(&dir, &[])));
        assert!(read.events.contains(Event::Scanner) && !read.events.contains(Event::Blueprint));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_switch_of_the_frame_rates_stands_on_the_page_is_off_at_first_and_reaches_the_overlay() {
        let _language = crate::language_lock();
        let (page, dir) = real_page("fps-zeile", "", &[]);
        let mut bench = Bench::new(page);
        bench.page.go_to("hf_overlay");
        assert!(!bench.page.overlay_fps, "off by default");
        for lang in [language::Choice::De, language::Choice::En] {
            language::set_thread_language(lang);
            // From the top, in steps shorter than the window, so that no part of the page is scrolled past unseen.
            let wheel = |dy: f32| egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, dy),
                modifiers: egui::Modifiers::NONE,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
            };
            // Over the left column of cards: on the overlay page the right one holds the picture of the signature
            // window, which takes the wheel for itself.
            bench.frame(vec![egui::Event::PointerMoved(Pos2::new(400.0, 450.0))]);
            for _ in 0..80 {
                bench.frame(vec![wheel(120.0)]);
            }
            let mut texts = bench.settle();
            for _ in 0..16 {
                for _ in 0..5 {
                    bench.frame(vec![wheel(-120.0)]);
                }
                texts.extend(bench.settle());
            }
            // The switch stands with the info bar, where the frame rates show.
            for key in ["s_ov_infoleiste_titel", "s_ov_fps"] {
                let text = language::t(key).to_lowercase();
                assert!(texts.iter().any(|(t, _)| t.to_lowercase() == text), "{key} ({text}) is not on the page");
            }
        }
        language::set_thread_language(language::Choice::De);
        let file = dir.join("einstellungen.json");
        let before = std::fs::read_to_string(&file).unwrap_or_default();
        assert!(!before.contains("overlay_fps_zeile"), "nothing written yet");
        bench.page.overlay_fps = true;
        bench.settle();
        let written = std::fs::read_to_string(&file).unwrap();
        assert!(written.contains("\"overlay_fps_zeile\": true"), "{written}");
        let again = SamplePage::with_settings(Paths::new(super::super::real_data::host_in(&dir, &[])));
        assert!(again.overlay_fps);
        let read = versekit_core::overlay::Settings::read(&Paths::new(super::super::real_data::host_in(&dir, &[])));
        assert!(read.fps_line, "the overlay reads the switch");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The switch of the info bar stands on the page Overlay in both languages, is on at first while its companion
    /// "without the game" is off, is written when switched off and is what the overlay reads.
    #[test]
    fn the_switch_of_the_info_bar_stands_on_the_page_is_on_at_first_and_reaches_the_overlay() {
        let _language = crate::language_lock();
        let (page, dir) = real_page("infoleiste", "", &[]);
        let mut bench = Bench::new(page);
        bench.page.go_to("hf_overlay");
        assert!(bench.page.overlay_info_bar, "on by default");
        assert!(!bench.page.overlay_info_bar_always, "without the game off by default");
        for lang in [language::Choice::De, language::Choice::En] {
            language::set_thread_language(lang);
            // From the top, in steps shorter than the window, so that no part of the page is scrolled past unseen.
            let wheel = |dy: f32| egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, dy),
                modifiers: egui::Modifiers::NONE,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
            };
            // Over the left column of cards: on the overlay page the right one holds the picture of the signature
            // window, which takes the wheel for itself.
            bench.frame(vec![egui::Event::PointerMoved(Pos2::new(400.0, 450.0))]);
            for _ in 0..80 {
                bench.frame(vec![wheel(120.0)]);
            }
            let mut texts = bench.settle();
            for _ in 0..16 {
                for _ in 0..5 {
                    bench.frame(vec![wheel(-120.0)]);
                }
                texts.extend(bench.settle());
            }
            for key in ["s_ov_infoleiste_titel", "s_ov_infoleiste"] {
                let text = language::t(key).to_lowercase();
                assert!(texts.iter().any(|(t, _)| t.to_lowercase() == text), "{key} ({text}) is not on the page");
            }
        }
        language::set_thread_language(language::Choice::De);
        let file = dir.join("einstellungen.json");
        let before = std::fs::read_to_string(&file).unwrap_or_default();
        assert!(!before.contains("\"infoleiste\""), "nothing written yet");
        let host = || Paths::new(super::super::real_data::host_in(&dir, &[]));
        let read = versekit_core::overlay::Settings::read(&host());
        assert!(read.info_bar && !read.info_bar_always, "the overlay reads it on, without the game off");
        bench.page.overlay_info_bar = false;
        bench.settle();
        let written = std::fs::read_to_string(&file).unwrap();
        assert!(written.contains("\"infoleiste\": false"), "{written}");
        assert!(!written.contains("infoleiste_ohne_spiel"), "the untouched companion is not written: {written}");
        assert!(!SamplePage::with_settings(host()).overlay_info_bar, "a stored choice wins over the default");
        assert!(!versekit_core::overlay::Settings::read(&host()).info_bar, "the overlay reads the switch");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The switch "without the game" of the overlay stands in the first group of the page Overlay in both languages,
    /// right under the row about the game, is on at first, is written only when touched and is what the overlay reads.
    #[test]
    fn the_switch_of_the_overlay_without_the_game_stands_on_the_page_and_reaches_the_overlay() {
        let _language = crate::language_lock();
        let (page, dir) = real_page("overlay-ohne-spiel", "", &[]);
        let mut bench = Bench::new(page);
        bench.page.go_to("hf_overlay");
        assert!(bench.page.overlay_always, "on by default");
        for lang in [language::Choice::De, language::Choice::En] {
            language::set_thread_language(lang);
            let texts = bench.settle();
            let at = |key: &str| texts.iter().find(|(t, _)| *t == language::t(key)).map(|(_, at)| at.y);
            let (game, switch) = (at("s_ei_ov_spiel"), at("s_ov_ohne_spiel"));
            assert!(game.is_some() && switch.is_some(), "{lang:?}: the rows are on the page: {texts:?}");
            assert!(game < switch, "{lang:?}: under the row about the game");
            assert!(texts.iter().any(|(t, _)| *t == language::t("s_ei_ov_durch")), "{lang:?}: the rows after it stay");
        }
        language::set_thread_language(language::Choice::De);
        let file = dir.join("einstellungen.json");
        let host = || Paths::new(super::super::real_data::host_in(&dir, &[]));
        assert!(versekit_core::overlay::Settings::read(&host()).overlay_always, "the overlay reads it on");
        bench.page.overlay_info_bar = false;
        bench.settle();
        let written = std::fs::read_to_string(&file).unwrap();
        assert!(!written.contains("overlay_ohne_spiel"), "untouched, it is not written: {written}");
        bench.page.overlay_always = false;
        bench.settle();
        let written = std::fs::read_to_string(&file).unwrap();
        assert!(written.contains("\"overlay_ohne_spiel\": false"), "{written}");
        assert!(!SamplePage::with_settings(host()).overlay_always, "a stored choice wins over the default");
        assert!(!versekit_core::overlay::Settings::read(&host()).overlay_always, "the overlay reads the switch");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The rows of the overlay go down to two: a stored three is read as three, the minus of the field on the page
    /// Overlay takes it to two and no further, two is written and is what the overlay reads; a stored one is two.
    #[test]
    fn the_rows_of_the_overlay_go_down_to_two() {
        let _language = crate::language_lock();
        language::set_thread_language(language::Choice::De);
        let (page, dir) = real_page("zeilen-zwei", "{\n  \"max_zeilen\": 3\n}\n", &[]);
        let host = || Paths::new(super::super::real_data::host_in(&dir, &[]));
        assert_eq!(page.overlay_rows, 3, "three is kept as it stands");
        assert_eq!(versekit_core::overlay::Settings::read(&host()).rows, 3);
        let mut bench = Bench::new(page);
        bench.page.go_to("hf_overlay");
        let texts = bench.settle();
        let label = language::t("s_zeilen");
        let (_, at) = texts.iter().find(|(t, _)| *t == label).unwrap_or_else(|| panic!("{label} is not on the page"));
        let row_y = at.y + 8.0;
        let minus = bench
            .named(&language::t("s_ff_weniger"))
            .into_iter()
            .min_by(|a, b| (a.center().y - row_y).abs().total_cmp(&(b.center().y - row_y).abs()))
            .filter(|rect| (rect.center().y - row_y).abs() < 20.0)
            .unwrap_or_else(|| panic!("no minus in the row of {label}"));
        bench.click_at(minus.center() - egui::vec2(3.0, 3.0));
        bench.settle();
        assert_eq!(bench.page.overlay_rows, 2, "three less one");
        bench.click_at(minus.center() - egui::vec2(3.0, 3.0));
        bench.settle();
        assert_eq!(bench.page.overlay_rows, 2, "two is the fewest");
        let written = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap();
        assert!(written.contains("\"max_zeilen\": 2"), "{written}");
        assert_eq!(versekit_core::overlay::Settings::read(&host()).rows, 2, "the overlay reads two");
        std::fs::write(dir.join("einstellungen.json"), "{\n  \"max_zeilen\": 1\n}\n").unwrap();
        assert_eq!(versekit_core::overlay::Settings::read(&host()).rows, 2, "a stored one is read as two");
        assert_eq!(SamplePage::with_settings(host()).overlay_rows, 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The sizes of the overlay and of the info bar stand on the page Overlay in both languages, each with its
    /// slider, are 100 % at first, are written each on its own and are what the overlay reads.
    #[test]
    fn the_sizes_of_overlay_and_info_bar_stand_on_the_page_and_reach_the_overlay() {
        let _language = crate::language_lock();
        let (page, dir) = real_page("fenstergroessen", "", &[]);
        let mut bench = Bench::new(page);
        bench.page.go_to("hf_overlay");
        assert_eq!((bench.page.overlay_size, bench.page.overlay_info_bar_size), (100, 100), "as the main window");
        for lang in [language::Choice::De, language::Choice::En] {
            language::set_thread_language(lang);
            // From the top, in steps shorter than the window, so that no part of the page is scrolled past unseen.
            let wheel = |dy: f32| egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, dy),
                modifiers: egui::Modifiers::NONE,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
            };
            // Over the left column of cards: on the overlay page the right one holds the picture of the signature
            // window, which takes the wheel for itself.
            bench.frame(vec![egui::Event::PointerMoved(Pos2::new(400.0, 450.0))]);
            for _ in 0..80 {
                bench.frame(vec![wheel(120.0)]);
            }
            let mut texts = bench.settle();
            for _ in 0..16 {
                for _ in 0..5 {
                    bench.frame(vec![wheel(-120.0)]);
                }
                texts.extend(bench.settle());
            }
            for key in ["s_ov_groesse", "s_ov_infoleiste_groesse"] {
                let text = language::t(key).to_lowercase();
                assert!(texts.iter().any(|(t, _)| t.to_lowercase() == text), "{key} ({text}) is not on the page");
            }
            // Each slider shows its percent beside it.
            let percents: Vec<&(String, Pos2)> = texts.iter().filter(|(t, _)| t.ends_with('%')).collect();
            assert!(texts.iter().filter(|(t, _)| t == "100 %").count() >= 2, "both at 100 %: {percents:?}");
        }
        language::set_thread_language(language::Choice::De);
        let file = dir.join("einstellungen.json");
        let host = || Paths::new(super::super::real_data::host_in(&dir, &[]));
        let read = || {
            let s = versekit_core::overlay::Settings::read(&host());
            (s.size_percent, s.info_bar_size_percent)
        };
        assert!(!std::fs::read_to_string(&file).unwrap_or_default().contains("groesse_prozent"), "nothing written yet");
        assert_eq!(read(), (100, 100));
        bench.page.overlay_info_bar_size = 200;
        bench.settle();
        let written = std::fs::read_to_string(&file).unwrap();
        assert!(written.contains("\"infoleiste_groesse_prozent\": 200"), "{written}");
        assert!(!written.contains("overlay_groesse_prozent"), "only the bar's: {written}");
        assert_eq!(read(), (100, 200), "the overlay reads the bar's size alone");
        bench.page.overlay_size = 150;
        bench.settle();
        assert_eq!(read(), (150, 200));
        let again = SamplePage::with_settings(host());
        assert_eq!((again.overlay_size, again.overlay_info_bar_size), (150, 200), "and the page shows them again");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_mark_of_what_is_new_stays_gone_once_the_area_was_opened() {
        let _language = crate::language_lock();
        // Someone who comes from an older version has a file of seen areas without this one.
        let (page, dir) = real_page("neu-marke", "", &[]);
        drop(page);
        std::fs::write(dir.join("gesehen.json"), "{\"zuletzt\": \"4.12.0\", \"bereiche\": {}}").unwrap();
        let paths = Paths::new(super::super::real_data::host_in(&dir, &[]));
        // The gallery of this version was closed already: it would take the click below.
        versekit_core::gallery::mark_shown(&paths, env!("CARGO_PKG_VERSION"));
        let page = SamplePage::with_settings(paths);
        assert_eq!(page.new_pages, ["hf_farmliste"], "the mark shows for someone who updates, only on what is new");
        let mut bench = Bench::new(page);
        bench.click_nth(&language::t("hf_farmliste"), 0);
        assert!(bench.page.new_pages.is_empty(), "the area was opened");
        let stored = std::fs::read_to_string(dir.join("gesehen.json")).unwrap();
        assert!(stored.contains("\"farmliste\""), "and it is written down: {stored}");
        // The next start: the mark does not come back.
        let paths = Paths::new(super::super::real_data::host_in(&dir, &[]));
        assert!(SamplePage::with_settings(paths).new_pages.is_empty(), "gone for good");
        // A tool of the developers brings it back.
        let mut again = SamplePage::with_settings(Paths::new(super::super::real_data::host_in(&dir, &[])));
        again.forget_seen();
        assert_eq!(again.new_pages, ["hf_farmliste"]);
        assert_eq!(
            SamplePage::with_settings(Paths::new(super::super::real_data::host_in(&dir, &[]))).new_pages,
            ["hf_farmliste"]
        );
        // A fresh install marks nothing and writes down every area as seen.
        let fresh = folder("neu-marke-frisch");
        let page = SamplePage::with_settings(Paths::new(super::super::real_data::host_in(&fresh, &[])));
        assert!(page.new_pages.is_empty(), "nothing is marked on a fresh install");
        let stored = std::fs::read_to_string(fresh.join("gesehen.json")).unwrap();
        assert!(stored.contains("\"liste\"") && stored.contains("\"farmliste\""), "{stored}");
        let _ = (std::fs::remove_dir_all(dir), std::fs::remove_dir_all(fresh));
    }

    /// The table of areas and versions: one version can bring several areas, each with its own mark, and an area
    /// counts as seen with the version it was last seen in.
    #[test]
    fn a_version_marks_every_area_it_brought_and_only_until_each_was_seen() {
        let seen_with = |pairs: &'static [(&'static str, &'static str)], last: &'static str| {
            move |area: &str| Some(pairs.iter().find(|(a, _)| *a == area).map_or(last, |(_, v)| *v).to_string())
        };
        // Coming from 3.90: Mein Lager (4.10), Grafik (3.97), the overview (3.98), Kaufen & Verkaufen (3.92) and the
        // farm list (5.0) are new. The shader cache has no page of its own any more (a card of Grafik): no mark.
        let marked = new_pages("5.0.0", seen_with(&[], "3.90.0"));
        for page in ["hf_meinlager", "hf_grafik", "hf_verkauf", "hf_farmliste"] {
            assert!(marked.contains(&page), "{page} in {marked:?}");
        }
        assert!(!marked.contains(&"hf_shader"), "{marked:?}");
        assert!(!marked.contains(&"hf_hangar") && !marked.contains(&"hf_liste"), "older areas: {marked:?}");
        // One of them seen with 5.0: its mark alone is gone.
        let marked = new_pages("5.0.0", seen_with(&[("grafik", "5.0.0")], "3.90.0"));
        assert!(!marked.contains(&"hf_grafik") && marked.contains(&"hf_meinlager"), "{marked:?}");
        // An area of a later version than the own one stays unmarked.
        assert!(!new_pages("4.12.0", seen_with(&[], "3.0.0")).contains(&"hf_farmliste"));
        assert_eq!(version_parts("5.0.0-rc2"), (5, 0, 0));
        assert_eq!(version_parts("x"), (0, 0, 0));
    }

    #[test]
    fn the_update_buttons_load_nothing_by_a_timer_on_real_data_and_run_the_stand_in_on_the_sample_page() {
        let _language = crate::language_lock();
        // Real data: a version named from outside shows the bar, but a click asks the question and loads nothing
        // by a timer; the page is the real one, without the made-up button of the sample page.
        let (mut page, dir) = real_page("updater", "", &[]);
        page.update_version = "5.1".to_string();
        let mut bench = Bench::new(page);
        bench.click("Jetzt aktualisieren");
        assert!(bench.page.loading.is_none(), "no stand-in is timed on real data");
        bench.page.go_to("hf_ueber");
        let texts = bench.settle();
        assert!(texts.iter().any(|(t, _)| t == &language::t("s_up_sofort")), "the real page has its button");
        // Counter-check: the sample page keeps its stand-in, so the click does load.
        let mut sample = preview();
        sample.click("Jetzt aktualisieren");
        assert!(sample.page.loading.is_some(), "the sample page runs the stand-in");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_general_page_has_no_row_for_the_usage_and_keeps_the_one_for_the_errors() {
        let _language = crate::language_lock();
        language::set_thread_language(language::Choice::De);
        let usage = "Nutzung wird gezählt";
        let errors = "Fehler werden mitgeschrieben";
        // Real data and the sample page: the row is neither drawn nor found by the menu filter.
        let (mut page, dir) = real_page("usage-row", "", &[]);
        page.go_to("hf_allgemein");
        let mut real = Bench::new(page);
        let mut sample = preview();
        sample.page.go_to("hf_allgemein");
        for (what, bench) in [("real data", &mut real), ("sample page", &mut sample)] {
            assert_eq!(bench.page.page, "hf_allgemein");
            let texts = bench.settle();
            assert!(!texts.iter().any(|(t, _)| t == usage), "{what}: the usage row is not drawn");
            assert!(texts.iter().any(|(t, _)| t == errors), "{what}: the errors row stays: {texts:?}");
        }
        for filter in ["Nutzung wird", "gezählt", "Nur Zahlen"] {
            for sample_data in [true, false] {
                let hits = super::super::nav_hits_in(filter, "", "hf_allgemein", sample_data);
                assert!(
                    !hits.iter().any(|hit| hit.ends_with(usage)),
                    "the filter {filter:?} does not find the usage row: {hits:?}"
                );
            }
        }
        // Counter-check: the row that stays is found by the same way of asking.
        let hits = super::super::nav_hits_in("mitgeschrieben", "", "hf_allgemein", true);
        assert!(hits.iter().any(|hit| hit.ends_with(errors)), "{hits:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_hangar_delivery_has_no_page_of_its_own_and_the_filter_leads_into_the_fold() {
        let _language = crate::language_lock();
        // The names below are asked for before any page is drawn: in the language of the window.
        language::set_thread_language(language::Choice::De);
        // Not in the menu any more.
        assert!(
            super::super::NAV.iter().all(|(_, pages)| pages.iter().all(|(page, _)| *page != "hf_hangar_einlieferung")),
            "the page is gone from the menu"
        );
        for setting in ["s_hgl_an", "s_hgl_code", "s_hgl_erneuern"] {
            let name = language::t(setting);
            let mut bench = preview();
            bench.page.nav_filter = name.clone();
            let hit = format!("{} › {name}", language::t("hf_hangar"));
            // The filter finds the setting on Mein Hangar, and on no page of its own.
            let texts = bench.settle();
            assert!(texts.iter().any(|(t, _)| *t == hit), "{hit} is a hit of the filter");
            assert!(!texts.iter().any(|(t, _)| t.starts_with(&language::t("hf_hangar_einlieferung"))));
            bench.click(&hit);
            assert_eq!(bench.page.page, "hf_hangar", "{name} opens Mein Hangar");
            assert!(bench.page.hangar.pairing_unfolded(), "and unfolds the pairing");
            // The fold is seen, with the hint that no data are open: nothing leads into the void.
            let texts = bench.settle();
            assert!(texts.iter().any(|(t, _)| *t == language::t("s_hgl_ohne_daten")), "{texts:?}");
            assert_eq!(
                bench.page.highlight.as_ref().map(|(n, _)| n.as_str()),
                Some(name.as_str()),
                "and lights the row"
            );
        }
        // A stored last page or a pinned tile with the old key falls back on Mein Hangar.
        let mut bench = preview();
        bench.page.go_to("hf_hangar_einlieferung");
        bench.settle();
        assert_eq!(bench.page.page, "hf_hangar");
        assert!(bench.page.hangar.pairing_unfolded());
        // Counter-check: an ordinary page named is left alone and nothing unfolds.
        let mut other = preview();
        other.page.go_to("hf_allgemein");
        other.settle();
        assert_eq!(other.page.page, "hf_allgemein");
        assert!(!other.page.hangar.pairing_unfolded());
    }

    #[test]
    fn the_evaluation_of_the_statistics_is_part_of_the_overview_and_its_old_key_lands_there() {
        let _language = crate::language_lock();
        language::set_thread_language(language::Choice::De);
        // One entry less in the group: the evaluation has no page of its own any more.
        let group = super::super::NAV.iter().find(|(group, _)| *group == "hf_gruppe_statistik").expect("the group");
        let keys: Vec<&str> = group.1.iter().map(|(page, _)| *page).collect();
        assert_eq!(keys, ["hf_st_uebersicht", "hf_st_schiffe", "hf_st_auftraege", "hf_st_quantum", "hf_st_stabil"]);
        // A stored last page, a pinned tile or a jump with the old key opens the merged page, which has the buttons.
        let mut bench = preview();
        bench.page.go_to("hf_st_auswertung");
        let texts = bench.settle();
        assert_eq!(bench.page.page, "hf_st_uebersicht");
        for key in ["s_sa_jetzt", "s_sa_neu", "s_sx_spielzeit"] {
            let name = language::t(key);
            assert!(texts.iter().any(|(text, _)| *text == name), "{name} on the merged page");
        }
        // Counter-check: another page named is left alone.
        let mut other = preview();
        other.page.go_to("hf_st_schiffe");
        other.settle();
        assert_eq!(other.page.page, "hf_st_schiffe");
    }

    #[test]
    fn the_real_basetool_logs_in_through_the_keychain_and_a_test_environment_books_nothing() {
        use crate::extension::Command;
        let _language = crate::language_lock();
        let dir = folder("echtes-basetool");
        let given = |dir: &std::path::Path| {
            let paths = Paths::new(super::super::real_data::host_in(dir, &[]));
            // A data folder given from outside, as the developer tools give a copy.
            paths.set_given_data(&paths.app_folder(), versekit_core::given::GivenBy::Named("Test".into()));
            SamplePage::with_settings(paths)
        };
        let mut page = given(&dir);
        let before = page.connection.wire().expect("the connection of the program").file_store.clone();
        assert!(cfg!(windows) || before.is_some(), "a given folder keeps the credentials inside itself");
        let ctx = egui::Context::default();
        let reason = "In der Testumgebung wird beim echten Basetool nichts gebucht.".to_string();
        page.apply_commands(
            &ctx,
            vec![Command::UseRealBasetool { keyname: "VerseKit Pruefung".into(), book_lock: Some(reason.clone()) }],
        );
        let wire = page.connection.wire().expect("the real connection");
        assert!(wire.demo.is_none(), "no made-up Basetool");
        // Elsewhere than on Windows a folder of its own beside the one of the installation: not in
        // the data folder, not the place of the user's own login.
        if !cfg!(windows) {
            let own = wire.file_store.clone().expect("a folder of its own");
            let theirs = versekit_core::basetool::secrets::folder(&versekit_core::paths::Host::current());
            assert!(own.ends_with("-versekitpruefung"), "a name of its own: {own}");
            assert_eq!(own, format!("{theirs}-versekitpruefung"));
            assert!(!own.starts_with(&*dir.to_string_lossy()), "the credentials never lie in the data folder: {own}");
        }
        assert_eq!(page.org_stock.bookings_locked(), Some(reason.as_str()), "a test environment books nothing");
        assert!(!page.storage_given, "the storage page follows the real sync");
        // The tool follows another game folder: the connection is made again, the same way.
        let other = folder("echtes-basetool-zwei");
        let paths = Paths::new(super::super::real_data::host_in(&other, &[]));
        paths.set_given_data(&paths.app_folder(), versekit_core::given::GivenBy::Named("Test".into()));
        page.apply_commands(&ctx, vec![Command::UsePaths(Box::new(paths))]);
        assert_eq!(
            page.org_stock.bookings_locked(),
            Some(reason.as_str()),
            "the lock survives a change of the folders"
        );
        if !cfg!(windows) {
            let own = page.connection.wire().and_then(|w| w.file_store.clone()).expect("a folder of its own");
            assert!(own.ends_with("-versekitpruefung"), "the name of its own survives too: {own}");
        }
        // Back to the made-up Basetool: it books, and the real connection is forgotten.
        page.apply_commands(&ctx, vec![Command::UseStorageLink(Box::new(crate::org_stock_page::Link::demo()))]);
        assert_eq!(page.org_stock.bookings_locked(), None);
        assert!(page.real_basetool.is_none());
        let _ = std::fs::remove_dir_all(other);
        // Live data: the same connection, bookings allowed.
        let mut live = given(&dir);
        live.apply_commands(
            &ctx,
            vec![Command::UseRealBasetool { keyname: "VerseKit Pruefung".into(), book_lock: None }],
        );
        assert!(live.connection.wire().is_some_and(|w| w.demo.is_none()));
        assert_eq!(live.org_stock.bookings_locked(), None, "with live data the bookings go");
        // The sample page has no data of its own: the command changes nothing there.
        let mut sample = SamplePage::preview();
        sample.apply_commands(&ctx, vec![Command::UseRealBasetool { keyname: "x".into(), book_lock: None }]);
        assert!(
            sample.connection.wire().is_some_and(|w| w.demo.is_some()),
            "the sample page keeps its made-up Basetool"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The main window hands the demand of the unit to the overlay at the first look, again when the overlay asks
    /// and when it changed, none while the page of the demand is not there, and nothing of the made-up Basetool.
    #[test]
    fn the_main_window_hands_the_demand_to_the_overlay_when_asked_or_changed_and_never_the_made_up_one() {
        use versekit_core::basetool::demand;
        use versekit_core::window_signal::{Role, Signal};
        let _language = crate::language_lock();
        let dir = folder("bedarf-ans-overlay");
        let page = SamplePage::with_settings(Paths::new(super::super::real_data::host_in(&dir, &[])));
        let wire = page.connection.wire().expect("the connection of the program").clone();
        assert!(wire.demo.is_none(), "the real Basetool");
        let signals = super::super::MainSignals::default();
        let sent = RefCell::new(Vec::new());
        let hand = |signals: &super::super::MainSignals, page: &SamplePage| {
            signals.hand_demand(
                page,
                |role| (role == Role::Overlay).then_some(7),
                |overlay, bytes| {
                    sent.borrow_mut().push((overlay, bytes.to_vec()));
                    true
                },
            );
        };
        let none = demand::handover(&[]);
        hand(&signals, &page);
        assert_eq!(*sent.borrow(), [(7, none.clone())], "the first look hands over what there is: nothing");
        hand(&signals, &page);
        assert_eq!(sent.borrow().len(), 1, "unchanged: not again");
        signals.note(Signal::DemandWanted);
        hand(&signals, &page);
        assert_eq!(sent.borrow().len(), 2, "the overlay asked: again");
        // A demand in memory while its page is not there (switched off, not connected): none goes over.
        let read = jsonio::loads(r#"{"materials": [{"material": {"name": "Gold"}, "openQuantity": {"amount": 2}}]}"#);
        wire.demand.take(demand::parse(&read.expect("json")), wire.sync.now());
        signals.note(Signal::DemandWanted);
        hand(&signals, &page);
        assert_eq!(sent.borrow().last(), Some(&(7, none)), "not visible: nothing of it");
        // The preview has a made-up Basetool: its demand never reaches a running overlay.
        let preview = SamplePage::preview();
        signals.note(Signal::DemandWanted);
        hand(&signals, &preview);
        assert_eq!(sent.borrow().len(), 3, "nothing of the preview");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A change goes over at once; while no overlay takes it, it is tried again only after a pause.
    #[test]
    fn a_changed_demand_is_tried_again_only_after_a_pause_while_no_overlay_takes_it() {
        let mut handed = super::super::DemandHanded::default();
        let start = std::time::Instant::now();
        assert!(handed.due(b"a", false, start), "the first look");
        assert!(!handed.due(b"a", false, start), "not taken, just tried: waits");
        let later = start + super::super::DEMAND_RETRY;
        assert!(handed.due(b"a", false, later), "after the pause");
        handed.handed(b"a".to_vec());
        assert!(!handed.due(b"a", false, later + super::super::DEMAND_RETRY), "taken and unchanged: no more");
        assert!(handed.due(b"a", true, later), "asked: at once");
        assert!(handed.due(b"b", false, later + super::super::DEMAND_RETRY * 2), "changed");
    }

    /// „In die Mitte holen“ puts everything back as 4.12 does: size and place forgotten (the default size in the
    /// middle), the corner free and the bar on top.
    #[test]
    fn bringing_the_overlay_back_forgets_size_and_place_frees_the_corner_and_puts_the_bar_on_top() {
        use versekit_core::overlay::{self, Rect};
        let _language = crate::language_lock();
        let settings = "{\n  \"overlay_ecke\": \"unten-rechts\",\n  \"overlay_leiste\": \"unten\"\n}\n";
        let (page, dir) = real_page("lage-zurueck", settings, &[]);
        let mut bench = Bench::new(page);
        let paths = bench.page.store.as_ref().map(|s| s.paths.working_copy()).expect("real data");
        assert!(overlay::save_place(&paths, Rect::new(1500, 900, 600, 400)));
        bench.page.go_to("hf_overlay");
        bench.click(&language::t("ov_s_mitte_knopf"));
        bench.settle();
        assert_eq!(overlay::load_remembered(&paths), None, "the default size, in the middle of the main screen");
        assert_eq!(paths.setting("overlay_ecke").as_deref(), Some("frei"));
        assert_eq!(paths.setting("overlay_leiste").as_deref(), Some("oben"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Without the game the overlay waits hidden: „In die Mitte holen“ then says why it still cannot be seen and
    /// how to show it; with the game running, or with the overlay shown without the game, it says nothing of it.
    #[test]
    fn bringing_the_overlay_back_without_the_game_says_it_waits_hidden() {
        let _language = crate::language_lock();
        let said = |settings: &str, running: bool| {
            let (mut page, dir) = real_page("mitte-wartet", settings, &[]);
            page.game_process = if running { |_| Some(true) } else { |_| Some(false) };
            let mut bench = Bench::new(page);
            bench.page.go_to("hf_overlay");
            // The game state is given: a real game on this machine writes its log and would count as running.
            bench.time = Some(1.0);
            bench.settle();
            bench.page.game_seen = Some((1.0, running));
            bench.click(&language::t("ov_s_mitte_knopf"));
            // The state, not the drawn text: another test may switch the global language meanwhile.
            let says = bench.page.place_reset_at.is_some() && bench.page.place_reset_waits;
            let _ = std::fs::remove_dir_all(dir);
            says
        };
        let hidden = "{\n  \"overlay_ohne_spiel\": false\n}\n";
        assert!(said(hidden, false), "without the game the button says the overlay waits hidden");
        assert!(!said(hidden, true), "with the game running it says nothing of it");
        assert!(!said("{}\n", false), "shown without the game (the default): nothing to say");
    }

    /// A note typed just before the window closes is written before it goes (the pause after typing is not over).
    #[test]
    fn closing_the_window_writes_the_note_typed_a_moment_before() {
        let _language = crate::language_lock();
        let (page, dir) = real_page("notiz-beim-schliessen", "", &[]);
        let mut bench = Bench::new(page);
        let paths = bench.page.store.as_ref().map(|s| s.paths.working_copy()).expect("real data");
        bench.page.go_to("hf_notizen");
        bench.settle();
        bench.page.memo_page.add(0.0);
        bench.page.memo_page.typed("Merken: Morozov-Helm", 1.0e6);
        assert!(versekit_core::memo::load(&paths).items.iter().all(|m| m.text.is_empty()), "not written yet");
        bench.close_request();
        let texts: Vec<String> = versekit_core::memo::load(&paths).items.into_iter().map(|m| m.text).collect();
        assert!(texts.iter().any(|text| text == "Merken: Morozov-Helm"), "{texts:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A write held back by a part that signed up: the count of its writes, and what it writes into the settings.
    struct HeldBack {
        written: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl crate::close_writes::WritesOnClose for HeldBack {
        fn write_held_back(&mut self, paths: Option<&Paths>) {
            let paths = paths.expect("the settings of the window");
            let n = self.written.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            assert!(paths.set_setting("probe_zurueckgehalten", Value::int(i64::try_from(n).unwrap_or(0))));
        }
    }

    /// A part that signed up writes what it holds back when the window closes, and again before a restart.
    #[test]
    fn closing_the_window_writes_what_a_signed_up_part_holds_back() {
        use std::sync::atomic::Ordering;
        let _language = crate::language_lock();
        let (page, dir) = real_page("zurueckgehalten", "", &[]);
        let mut bench = Bench::new(page);
        let paths = bench.page.store.as_ref().map(|s| s.paths.working_copy()).expect("real data");
        let written = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        bench.page.write_on_close(Box::new(HeldBack { written: std::sync::Arc::clone(&written) }));
        bench.settle();
        assert_eq!(written.load(Ordering::SeqCst), 0, "nothing is written while the window stays");
        bench.close_request();
        assert_eq!(written.load(Ordering::SeqCst), 1, "written once when the window closes");
        assert_eq!(paths.setting_int("probe_zurueckgehalten", 0, None, None), 1, "on disk, with the settings");
        let before = written.load(Ordering::SeqCst);
        bench.page.save_and_wait();
        assert_eq!(written.load(Ordering::SeqCst), before + 1, "a restart writes it too");
        let _ = std::fs::remove_dir_all(dir);
    }
}
