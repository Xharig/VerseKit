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

//! The pages „Spiel“ (blueprint notes in the game's texts) and
//! „Übersetzung“ (the text file per channel), ports of `pages._game` and
//! `pages._translation_page`. The background that keeps both current runs in
//! the overlay (`game_texts::watch`); the pages show and take the steps.
//!
//! Fetching and writing take seconds, so they run on a thread of their own,
//! one at a time; the page shows what happens and what came of it.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use egui::{RichText, Ui};
use versekit_core::game_texts::{self, Archive, Placed};
use versekit_core::injection::{self, Outcome, item_info, ranks};
use versekit_core::jsonio::Value;
use versekit_core::language::{self, Arg, t, tf};
use versekit_core::net::Net;
use versekit_core::paths::Paths;
use versekit_core::theme::{Color, Scheme};
use versekit_core::translation::{self, CUSTOM, ORIGINAL};
use versekit_gamedata::game_text;

use crate::color;
use crate::layout::GAP;
use crate::widgets::{self, ButtonKind};

mod anti_afk;
mod user_cfg;

/// The `Data.p4k` of the player, through `versekit-gamedata`.
pub struct GameArchive;

impl Archive for GameArchive {
    fn stamp(&self, paths: &Paths) -> Option<String> {
        game_text::archive_stamp(&paths.game_folder()?)
    }
    fn refresh_original(&self, paths: &Paths) -> bool {
        game_text::refresh_original(paths, None)
    }
    fn fill_gaps(&self, paths: &Paths, target: &str) -> usize {
        game_text::fill_gaps(paths, target, None)
    }
    fn fetch_original(&self, paths: &Paths, root: &str, set_language: bool) -> Result<Placed, String> {
        let set = |language: &str| translation::set_user_cfg(paths, root, language, None);
        match game_text::fetch_original(paths, "english", Some(root), set_language, &set) {
            Ok(game_text::FetchOutcome::Written { bytes }) => Ok(Placed { said: megabytes(bytes), written: true }),
            Err(game_text::FetchError::Read(error)) => Err(error.text_key().map_or_else(|| error.to_string(), t)),
            Err(game_text::FetchError::UnknownTarget) => Err(t("s_tq_kein_spiel")),
            Err(other) => Err(other.to_string()),
        }
    }
    fn refresh_names(&self, paths: &Paths) {
        game_text::names_or_fetch(paths, None);
    }
}

fn megabytes(bytes: usize) -> String {
    tf("s_tq_mb", &[("0", Arg::from(language::py_fixed(bytes as f64 / 1_048_576.0, 1, false)))])
}

/// The net of the pages: a stand-in in the tests of this crate.
fn net() -> Arc<dyn Net> {
    #[cfg(test)]
    return Arc::new(versekit_core::net::Fake::default());
    #[cfg(not(test))]
    Arc::new(versekit_core::net::Web::new(
        translation::USER_AGENT,
        Duration::from_secs(translation::TIMEOUT_SECONDS),
        200 * 1024 * 1024,
    ))
}

/// What a thread tells the page.
enum Message {
    Progress(String),
    Done(String, bool),
}

/// A question before a step that changes the game: replace the text file,
/// or leave the main installation alone.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Question {
    Source { source: String, channel: Option<String>, root: String },
    LeaveAlone,
}

/// The own address being typed on a card.
#[derive(Clone, Debug, Default)]
struct Custom {
    open: bool,
    url: String,
    language: String,
}

/// The state of both pages and the background.
pub struct GameTextsState {
    paths: Option<Arc<Paths>>,
    archive: Arc<dyn Archive>,
    net: Arc<dyn Net>,
    messages: Option<Receiver<Message>>,
    /// A setting changed while a step ran: write the notes anew after it.
    apply_after: bool,
    /// The notes were switched on or off while a step ran.
    settle_after: bool,
    busy: bool,
    said: Option<(String, bool)>,
    question: Option<Question>,
    custom: Vec<(String, Custom)>,
    status: Option<(f64, game_texts::Status)>,
    /// The assistant only: "Keine Übersetzung" was chosen in this run. It writes nothing (as in 4.12, where doing
    /// nothing is the truth) and only marks the choice.
    left_alone: bool,
    /// The language picked in the list of a card (its key; "" for „Keine Übersetzung“), before a source of it is
    /// chosen: (key of the card, language).
    picked_language: Vec<(String, String)>,
    /// The language a card shows where nothing is chosen nor picked (the checks set it).
    default_language: Option<String>,
    /// The sources a test channel was given because it follows the main installation: (channel, source), tried once
    /// per change, also where it failed.
    followed: Vec<(String, String)>,
    /// The block "Anti-AFK" on the page Spiel.
    anti_afk: anti_afk::AfkBlock,
    /// The block "user.cfg" at the foot of the page Spiel.
    user_cfg: user_cfg::UserCfgBlock,
}

impl Default for GameTextsState {
    fn default() -> Self {
        GameTextsState {
            paths: None,
            archive: Arc::new(game_texts::NoArchive),
            net: net(),
            messages: None,
            apply_after: false,
            settle_after: false,
            busy: false,
            said: None,
            question: None,
            custom: Vec::new(),
            status: None,
            left_alone: false,
            picked_language: Vec::new(),
            default_language: None,
            followed: Vec::new(),
            anti_afk: anti_afk::AfkBlock::default(),
            user_cfg: user_cfg::UserCfgBlock::default(),
        }
    }
}

/// What a page asks the window to do.
pub enum Jump {
    Translation,
}

impl GameTextsState {
    /// On the data and game folder of the window.
    pub fn with_paths(paths: &Paths) -> Self {
        GameTextsState {
            paths: Some(Arc::new(Paths::new(paths.host.clone()))),
            archive: Arc::new(GameArchive),
            ..Self::default()
        }
    }

    /// With other stand-ins for net and archive (tests).
    #[cfg(test)]
    pub(crate) fn with_parts(paths: &Paths, net: Arc<dyn Net>, archive: Arc<dyn Archive>) -> Self {
        GameTextsState { paths: Some(Arc::new(Paths::new(paths.host.clone()))), archive, net, ..Self::default() }
    }

    /// A switch of the notes flipped in the setup assistant, with the same effect as on the page of the game: the
    /// setting is written, and where notes stand in the text file they are written anew or taken out.
    pub fn switch_note(&mut self, setting: &str, on: bool) {
        if setting == game_texts::SETTING_ON {
            self.switch_on(on);
        } else if setting == game_texts::SETTING_AUTO {
            let Some(paths) = self.paths.clone() else { return };
            paths.set_setting(game_texts::SETTING_AUTO, Value::Bool(on));
            self.say(tf("s_sp_auto_sagen", &[("0", Arg::from(t(if on { "e_an" } else { "e_aus" })))]), false);
        } else if setting == injection::SETTING_DETAILS {
            self.switch_detail(setting, on, "s_sp_angaben_sagen");
        } else if setting == ranks::SETTING {
            self.switch_detail(setting, on, "s_sp_rang_sagen");
        }
    }

    /// The assistant opens anew: "Keine Übersetzung" is not chosen yet in this run.
    pub fn forget_leaving_alone(&mut self) {
        self.left_alone = false;
    }

    /// Whether a step runs now.
    pub fn busy(&self) -> bool {
        self.busy
    }

    /// The last thing said on the pages, and whether it was a failure.
    pub fn said(&self) -> Option<&(String, bool)> {
        self.said.as_ref()
    }

    fn say(&mut self, text: String, bad: bool) {
        self.said = Some((text, bad));
    }

    /// Run `work` on a thread of its own; refused while another runs.
    fn start(&mut self, work: impl FnOnce(&Paths, &dyn Net, &dyn Archive, &Sender<Message>) + Send + 'static) {
        let Some(paths) = self.paths.clone() else { return };
        if self.busy {
            self.say(t("inj_laeuft"), false);
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.messages = Some(receiver);
        self.busy = true;
        self.say(t("inj_laeuft"), false);
        let (net, archive) = (self.net.clone(), self.archive.clone());
        let lock = versekit_core::writeguard::state();
        let started = std::thread::Builder::new()
            .name("spieltexte".into())
            .spawn(move || {
                versekit_core::writeguard::adopt(&lock);
                work(&paths, net.as_ref(), archive.as_ref(), &sender);
            })
            .is_ok();
        if !started {
            self.busy = false;
            self.say(tf("inj_fehler", &[("0", Arg::from(t("inj_abgebrochen")))]), true);
        }
    }

    /// Take what the threads said; called every frame.
    pub fn poll(&mut self) {
        self.take_messages();
        // What was switched while the step ran comes now.
        if !self.busy {
            let again = std::mem::take(&mut self.apply_after);
            if std::mem::take(&mut self.settle_after) || again {
                self.settle(again);
            }
        }
    }

    fn take_messages(&mut self) {
        let Some(receiver) = &self.messages else { return };
        let mut got = Vec::new();
        let gone = loop {
            match receiver.try_recv() {
                Ok(message) => got.push(message),
                Err(mpsc::TryRecvError::Empty) => break false,
                Err(mpsc::TryRecvError::Disconnected) => break true,
            }
        };
        let mut ended = false;
        for message in got {
            match message {
                Message::Progress(text) => self.say(text, false),
                Message::Done(text, bad) => {
                    self.say(text, bad);
                    self.finish();
                    ended = true;
                }
            }
        }
        // A thread that ended without a word (it broke off) must not leave
        // the pages waiting for ever.
        if gone && !ended && self.busy {
            self.say(tf("inj_fehler", &[("0", Arg::from(t("inj_abgebrochen")))]), true);
            self.finish();
        }
        if gone {
            self.messages = None;
        }
    }

    /// A step ended. The watch of the game's texts runs in the overlay: it sees this writing of the file as
    /// another tool's and looks once (`game_texts::watch`).
    fn finish(&mut self) {
        self.busy = false;
        self.status = None;
    }

    fn status(&mut self, now: f64) -> Option<game_texts::Status> {
        let paths = self.paths.clone()?;
        match &self.status {
            Some((at, status)) if now - at < 2.0 => Some(status.clone()),
            _ => {
                let status = game_texts::status(&paths);
                self.status = Some((now, status.clone()));
                Some(status)
            }
        }
    }

    // ------------------------------------------------------- the steps

    /// `_inj_refresh`: write the notes anew.
    fn apply_now(&mut self) {
        self.start(|paths, net, archive, out| {
            let version = versekit_core::stock_fetch::live_build(paths);
            let said = outcome_text(&game_texts::apply(paths, net, archive, &version));
            let _ = out.send(Message::Done(said.0, said.1));
        });
    }

    /// `_inj_remove`.
    fn remove_now(&mut self) {
        self.start(|paths, _, _, out| {
            let said = match game_texts::remove(paths) {
                Outcome::Done(count) => (tf("inj_entfernt", &[("0", Arg::from(count as i64))]), false),
                other => outcome_text(&other),
            };
            let _ = out.send(Message::Done(said.0, said.1));
        });
    }

    /// `_inj_check`: is there something new at the source in use?
    fn check_now(&mut self) {
        let Some(paths) = self.paths.clone() else { return };
        let status = game_texts::status(&paths);
        let applied = if status.applied { t("inj_steht") } else { t("inj_steht_nicht") };
        let Some(source) = status.source else {
            self.say(applied, false);
            return;
        };
        self.start(move |paths, net, _, out| {
            // A copy of the data only looks and says so calmly; it never fetches.
            if paths.game_folder().is_some_and(|root| translation::only_looks(paths, &root)) {
                let said = match translation::look_on_copy(net, paths, &source, None) {
                    Ok(looked) => (format!("{applied} · {}", looked.text_in(language::current())), false),
                    Err(problem) => (tf("inj_fehler", &[("0", Arg::from(problem.text()))]), true),
                };
                let _ = out.send(Message::Done(said.0, said.1));
                return;
            }
            let (newer, ident) = translation::update_available(net, paths, &source, None);
            let mut parts = vec![applied];
            let state = translation::status_text(paths, &source, None, language::current(), &translation::today());
            if !state.is_empty() {
                parts.push(state);
            }
            parts.push(if newer {
                tf("inj_update_da", &[("0", Arg::from(ident.unwrap_or_default().as_str()))])
            } else {
                t("inj_aktuell")
            });
            let _ = out.send(Message::Done(parts.join(" · "), false));
        });
    }

    /// Switch a setting of the notes and, where notes stand, write anew.
    fn switch_detail(&mut self, setting: &str, on: bool, said: &str) {
        let Some(paths) = self.paths.clone() else { return };
        paths.set_setting(setting, Value::Bool(on));
        self.say(tf(said, &[("0", Arg::from(t(if on { "e_an" } else { "e_aus" })))]), false);
        if paths.setting_bool(game_texts::SETTING_ON, true) && game_texts::status(&paths).applied {
            if self.busy {
                self.apply_after = true;
            } else {
                self.apply_now();
            }
        }
        self.status = None;
    }

    /// `inj_an_um`: off takes the notes out, on writes them.
    fn switch_on(&mut self, on: bool) {
        let Some(paths) = self.paths.clone() else { return };
        paths.set_setting(game_texts::SETTING_ON, Value::Bool(on));
        self.say(tf("s_sp_an_sagen", &[("0", Arg::from(t(if on { "e_an" } else { "e_aus" })))]), false);
        if self.busy {
            self.settle_after = true;
        } else {
            self.settle(false);
        }
        self.status = None;
    }

    /// Bring the file in line with the switch: on writes, off takes out;
    /// `again` writes also where notes stand.
    fn settle(&mut self, again: bool) {
        let Some(paths) = self.paths.clone() else { return };
        let on = paths.setting_bool(game_texts::SETTING_ON, true);
        let applied = game_texts::status(&paths).applied;
        if on && (again || !applied) {
            self.apply_now();
        } else if !on && applied {
            self.remove_now();
        }
    }

    /// Ask before a source replaces a text file, once per source.
    fn choose(&mut self, source: &str, channel: Option<&str>, root: &str) {
        let Some(paths) = self.paths.clone() else { return };
        if self.busy {
            self.say(t("inj_laeuft"), false);
            return;
        }
        if source != ORIGINAL && !game_texts::confirmed(&paths, source) {
            self.question = Some(Question::Source {
                source: source.into(),
                channel: channel.map(str::to_string),
                root: root.into(),
            });
            return;
        }
        self.fetch_source(source.to_string(), channel.map(str::to_string), root.to_string());
    }

    fn fetch_source(&mut self, source: String, channel: Option<String>, root: String) {
        self.start(move |paths, net, archive, out| {
            let _ =
                out.send(Message::Progress(tf("s_sp_hole", &[("0", Arg::from(translation::display_name(&source)))])));
            let progress = |step: translation::Progress| {
                let text = match step {
                    translation::Progress::Loading(name, mb) => {
                        tf("z_laedt", &[("0", Arg::from(name)), ("1", Arg::from(mb))])
                    }
                    translation::Progress::Placing => t("z_einsetzen"),
                };
                let _ = out.send(Message::Progress(text));
            };
            let said = match channel {
                None => match game_texts::switch_source(
                    paths,
                    net,
                    archive,
                    &source,
                    &versekit_core::stock_fetch::live_build(paths),
                    &progress,
                ) {
                    Ok(done) => {
                        if let Some((_, count)) = done.cleaned {
                            let _ = out
                                .send(Message::Progress(tf("inj_alt_aufgeraeumt", &[("0", Arg::from(count as i64))])));
                        }
                        switched_text(&done)
                    }
                    Err(problem) => (tf("inj_fehler", &[("0", Arg::from(problem))]), true),
                },
                Some(channel) => {
                    match game_texts::switch_side(paths, net, archive, &channel, &root, &source, &progress) {
                        Ok(message) => {
                            (tf("s_tq_fertig", &[("0", Arg::from(channel.as_str())), ("1", Arg::from(message))]), false)
                        }
                        Err(problem) => (tf("inj_fehler", &[("0", Arg::from(problem))]), true),
                    }
                }
            };
            let _ = out.send(Message::Done(said.0, said.1));
        });
    }

    /// `pages.check` on a card: look for a new version and fetch it.
    fn check_channel(&mut self, current: &str, channel: Option<&str>, root: &str) {
        if current.is_empty() || current == ORIGINAL {
            self.say(t("s_tq_nichts_zu_pruefen"), false);
            return;
        }
        let check = Check { source: current.to_string(), channel: channel.map(str::to_string), root: root.to_string() };
        self.start(move |paths, net, archive, out| {
            let said = check_one(paths, net, archive, &check).unwrap_or_else(|| (t("inj_aktuell"), false));
            let _ = out.send(Message::Done(said.0, said.1));
        });
    }

    /// The link of the overview tile (4.12 `translation_check`): every channel with a chosen source looks for a
    /// newer version and fetches it, one after the other on one thread. What was fetched is said, else that all
    /// is current.
    pub fn check_all(&mut self) {
        let Some(paths) = self.paths.clone() else { return };
        let checks = checkable(&paths);
        if checks.is_empty() {
            self.say(t("s_tq_nichts_zu_pruefen"), false);
            return;
        }
        self.start(move |paths, net, archive, out| {
            let mut said = Vec::new();
            let mut failed = false;
            for check in &checks {
                if let Some((text, bad)) = check_one(paths, net, archive, check) {
                    said.push(text);
                    failed |= bad;
                }
            }
            let text = if said.is_empty() { t("inj_aktuell") } else { said.join(" · ") };
            let _ = out.send(Message::Done(text, failed));
        });
    }

    /// Put the text file back as it was before the last replacement, with its note of original texts. The file put
    /// back counts as the one placed for the note `key`, so the background keeps it and does not replace it again.
    fn restore(&mut self, target: String, key: String) {
        self.start(move |paths, _, _, out| {
            let said = match translation::restore_backup(paths, &target, true) {
                Ok(_) => {
                    translation::note_head_of_file(paths, &key, &target);
                    (t("s_tq_zurueck_fertig"), false)
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (t("s_tq_zurueck_keine"), true),
                Err(error) => (tf("inj_fehler", &[("0", Arg::from(error.to_string()))]), true),
            };
            let _ = out.send(Message::Done(said.0, said.1));
        });
    }

    fn custom_of(&mut self, card: &str, paths: &Paths, channel: Option<&str>) -> &mut Custom {
        if !self.custom.iter().any(|(k, _)| k == card) {
            let stored = translation::custom_settings(paths, channel);
            let text = |key: &str| {
                stored.iter().find(|(k, _)| k == key).and_then(|(_, v)| v.as_str()).unwrap_or("").to_string()
            };
            let language = text("sprache");
            let custom = Custom {
                open: false,
                url: text("url"),
                language: if language.is_empty() { "english".into() } else { language },
            };
            self.custom.push((card.to_string(), custom));
        }
        let at = self.custom.iter().position(|(k, _)| k == card).unwrap_or(0);
        &mut self.custom[at].1
    }
}

/// The text of how a writing ended, and whether it failed.
fn outcome_text(outcome: &Outcome) -> (String, bool) {
    let failed = |text: String| (tf("inj_fehler", &[("0", Arg::from(text))]), true);
    match outcome {
        Outcome::Done(count) => (tf("inj_aktiv", &[("0", Arg::from(*count as i64))]), false),
        Outcome::NoFile => failed(t("m_keine_ini")),
        Outcome::NoMissions => failed(t("m_keine_missionen")),
        Outcome::ReadFailed(text) | Outcome::WriteFailed(text) => failed(text.clone()),
    }
}

/// One channel to look at: its chosen source, the channel (`None` for the main installation) and its folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    pub source: String,
    pub channel: Option<String>,
    pub root: String,
}

/// The channels with a chosen source other than the original texts, the main installation first, then the other
/// channels with an archive the page shows (4.12 `translation_rows`).
pub fn checkable(paths: &Paths) -> Vec<Check> {
    let flavor = paths.host.flavor;
    let same = |a: &str, b: &str| flavor.normcase(&flavor.normpath(a)) == flavor.normcase(&flavor.normpath(b));
    let main = paths.game_folder();
    let mut out = Vec::new();
    if let Some(root) = &main {
        let source = paths.setting(translation::SETTING_SOURCE).unwrap_or_default();
        out.push(Check { source, channel: None, root: root.clone() });
    }
    let sources = translation::channel_sources(paths);
    for (name, folder, p4k) in paths.installed_channels() {
        if !p4k || !translation::CHANNELS.contains(&name.as_str()) || main.as_deref().is_some_and(|m| same(m, &folder))
        {
            continue;
        }
        let source = sources.iter().find(|(k, _)| *k == name).and_then(|(_, v)| v.as_str()).unwrap_or("").to_string();
        out.push(Check { source, channel: Some(name), root: folder });
    }
    out.retain(|check| !check.source.is_empty() && check.source != ORIGINAL);
    out
}

/// Looks for a newer version of the source of `check` and fetches it: what came of it, and whether it failed;
/// `None` when there is nothing newer. A copy of the data only looks and says what it saw.
fn check_one(paths: &Paths, net: &dyn Net, archive: &dyn Archive, check: &Check) -> Option<(String, bool)> {
    let Check { source, channel, root } = check;
    if translation::only_looks(paths, root) {
        return Some(match translation::look_on_copy(net, paths, source, channel.as_deref()) {
            Ok(looked) => (looked.text_in(language::current()), false),
            Err(problem) => (tf("inj_fehler", &[("0", Arg::from(problem.text()))]), true),
        });
    }
    let (newer, _) = translation::update_available(net, paths, source, channel.as_deref());
    if !newer {
        return None;
    }
    let progress = |_: translation::Progress| {};
    Some(match channel {
        None => match game_texts::switch_source(
            paths,
            net,
            archive,
            source,
            &versekit_core::stock_fetch::live_build(paths),
            &progress,
        ) {
            Ok(done) => switched_text(&done),
            Err(problem) => (tf("inj_fehler", &[("0", Arg::from(problem))]), true),
        },
        Some(channel) => match game_texts::switch_side(paths, net, archive, channel, root, source, &progress) {
            Ok(message) => (tf("s_tq_fertig", &[("0", Arg::from(channel.as_str())), ("1", Arg::from(message))]), false),
            Err(problem) => (tf("inj_fehler", &[("0", Arg::from(problem))]), true),
        },
    })
}

/// The text of a switch of the source: what was placed, then how the
/// writing of the notes ended (none while the notes are switched off).
fn switched_text(done: &game_texts::Switched) -> (String, bool) {
    match &done.applied {
        None => (done.fetched.clone(), false),
        Some(outcome) => {
            let (text, bad) = outcome_text(outcome);
            (format!("{} · {text}", done.fetched), bad)
        }
    }
}

fn hint(ui: &mut Ui, scheme: &Scheme, text: &str) {
    widgets::hint(ui, scheme, text);
}

fn said_line(ui: &mut Ui, scheme: &Scheme, state: &GameTextsState) {
    if let Some((text, bad)) = state.said() {
        let tone: Color = if *bad { scheme.red } else { scheme.sub };
        ui.add(egui::Label::new(RichText::new(text).color(color(tone))).wrap());
    }
}

/// One switch with its help text; returns the new value when it changed.
fn switch(ui: &mut Ui, scheme: &Scheme, label: &str, help: &str, value: bool) -> Option<bool> {
    let mut on = value;
    let changed = widgets::switch_row_help(ui, scheme, label, help, &mut on).changed();
    ui.add_space(8.0);
    changed.then_some(on)
}

/// The page „Spiel“.
pub fn show_game(ui: &mut Ui, scheme: &Scheme, state: &mut GameTextsState) -> Option<Jump> {
    state.poll();
    let paths = state.paths.clone()?;
    let now = ui.input(|i| i.time);
    let mut jump = None;
    hint(ui, scheme, &t("s_sp_lead"));
    ui.add_space(8.0);
    let status = state.status(now);

    // The state of the notes in the game stands in the card of the source, under its title.
    widgets::card(ui, scheme, |ui| {
        crate::settings::setting_group(ui, scheme, &t("s_sp_quelle"));
        if let Some(status) = &status {
            if !paths.setting_bool(game_texts::SETTING_ON, true) {
                if status.applied {
                    widgets::status_box(
                        ui,
                        scheme,
                        scheme.sub,
                        "circle-dashed",
                        &t("s_sp_aus_rest"),
                        &t("s_sp_aus_rest_h"),
                    );
                } else {
                    widgets::status_box(ui, scheme, scheme.sub, "circle-dashed", &t("s_sp_aus_hinweis"), "");
                }
            } else if status.applied {
                let mut extra = Vec::new();
                if let Some(source) = &status.source {
                    extra.push(tf("s_sp_quelle_ist", &[("0", Arg::from(translation::display_name(source)))]));
                    let stand =
                        translation::status_text(&paths, source, None, language::current(), &translation::today());
                    if !stand.is_empty() {
                        extra.push(stand);
                    }
                }
                widgets::status_box(ui, scheme, scheme.accent, "circle-check", &t("s_sp_steht"), &extra.join(" · "));
            } else {
                widgets::status_box(ui, scheme, scheme.sub, "circle-dashed", &t("s_sp_nichts"), &t("s_sp_nichts_h"));
            }
            ui.add_space(6.0);
        }
        let chosen =
            paths.setting(game_ini_source()).map_or_else(|| t("s_uv_keine_quelle"), |s| translation::display_name(&s));
        hint(ui, scheme, &tf("s_sp_quelle_jetzt", &[("0", Arg::from(chosen))]));
        ui.add_space(4.0);
        if ui.add(widgets::Button::new(&t("s_sp_zur_uebersetzung"), ButtonKind::Normal, scheme)).clicked() {
            jump = Some(Jump::Translation);
        }
    });
    ui.add_space(GAP);

    widgets::card(ui, scheme, |ui| {
        crate::settings::setting_group(ui, scheme, &t("s_sp_schalter"));
        if let Some(on) =
            switch(ui, scheme, &t("s_sp_auto"), &t("s_sp_auto_h"), paths.setting_bool(game_texts::SETTING_AUTO, true))
        {
            paths.set_setting(game_texts::SETTING_AUTO, Value::Bool(on));
            state.say(tf("s_sp_auto_sagen", &[("0", Arg::from(t(if on { "e_an" } else { "e_aus" })))]), false);
        }
        if let Some(on) =
            switch(ui, scheme, &t("s_sp_an"), &t("s_sp_an_h"), paths.setting_bool(game_texts::SETTING_ON, true))
        {
            state.switch_on(on);
        }
        let details = [
            (injection::SETTING_DETAILS, "s_sp_angaben", "s_sp_angaben_h", "s_sp_angaben_sagen"),
            (ranks::SETTING, "s_sp_rang", "s_sp_rang_h", "s_sp_rang_sagen"),
            (injection::SETTING_TITLE_REP, "s_sp_titelruf", "s_sp_titelruf_h", "s_sp_titelruf_sagen"),
            (item_info::SETTING, "s_sp_itinfo", "s_sp_itinfo_h", "s_sp_itinfo_sagen"),
        ];
        for (setting, label, help, said) in details {
            if let Some(on) = switch(ui, scheme, &t(label), &t(help), paths.setting_bool(setting, true)) {
                state.switch_detail(setting, on, said);
            }
        }
    });
    ui.add_space(GAP);

    widgets::card(ui, scheme, |ui| {
        crate::settings::setting_group(ui, scheme, &t("s_sp_hand"));
        hint(ui, scheme, &t("s_sp_hand_h"));
        ui.add_space(4.0);
        let busy = state.busy().then(|| t("inj_laeuft"));
        let mut pressed = None;
        ui.horizontal_wrapped(|row| {
            let buttons = [
                ("s_sp_jetzt", ButtonKind::Strong),
                ("s_sp_pruefen", ButtonKind::Normal),
                ("s_sp_weg", ButtonKind::Danger),
            ];
            for (i, (label, kind)) in buttons.into_iter().enumerate() {
                if row.add(widgets::Button::new(&t(label), kind, scheme).blocked(busy.as_deref())).clicked() {
                    pressed = Some(i);
                }
            }
        });
        match pressed {
            Some(0) => state.apply_now(),
            Some(1) => state.check_now(),
            Some(2) => state.remove_now(),
            _ => {}
        }
        said_line(ui, scheme, state);
        ui.add_space(8.0);
        widgets::status_box(ui, scheme, scheme.gold, "shield-alert", &t("s_sp_warn"), &t("s_sp_warn_h"));
    });
    ui.add_space(GAP);
    widgets::card(ui, scheme, |ui| anti_afk::show(ui, scheme, state));
    ui.add_space(GAP);
    widgets::card(ui, scheme, |ui| user_cfg::show(ui, scheme, state));
    jump
}

fn game_ini_source() -> &'static str {
    translation::SETTING_SOURCE
}

/// The channel a folder name ends with after a space, `-` or `_` (`Spieldaten LIVE` → `LIVE`),
/// else the name itself.
fn channel_title(name: &str) -> String {
    let upper = name.trim().to_uppercase();
    versekit_core::paths::CHANNELS
        .iter()
        .find(|k| upper == **k || upper.strip_suffix(**k).is_some_and(|rest| rest.ends_with([' ', '-', '_'])))
        .map_or_else(|| name.to_string(), |k| k.to_string())
}

/// The page „Übersetzung“: one card per channel in the columns of cards (the main installation first, LIVE and PTU
/// side by side), a closing card with what happened last and the warning.
pub fn show_translation(ui: &mut Ui, scheme: &Scheme, state: &mut GameTextsState) {
    state.poll();
    let Some(paths) = state.paths.clone() else { return };
    hint(ui, scheme, &t("s_tq_lead"));
    ui.add_space(8.0);
    let main_folder = paths.game_folder();
    let channels: Vec<(String, String)> = paths
        .installed_channels()
        .into_iter()
        .filter(|(name, _, p4k)| *p4k && translation::CHANNELS.contains(&name.as_str()))
        .map(|(n, f, _)| (n, f))
        .collect();
    let same = |a: &str, b: &str| {
        let f = paths.host.flavor;
        f.normcase(&f.normpath(a)) == f.normcase(&f.normpath(b))
    };
    let main_name = main_folder
        .as_deref()
        .and_then(|main| channels.iter().find(|(_, folder)| same(folder, main)).map(|(n, _)| n.clone()));
    // The cards: (title, folder, main).
    let mut cards: Vec<(String, String, bool)> = Vec::new();
    if let Some(main_root) = main_folder.clone() {
        let title = main_name.clone().unwrap_or_else(|| channel_title(&paths.host.flavor.basename(&main_root)));
        cards.push((title, main_root, true));
    } else if channels.is_empty() {
        no_game(ui, scheme);
        return;
    }
    for (name, folder) in &channels {
        if Some(name) != main_name.as_ref() {
            cards.push((name.clone(), folder.clone(), false));
        }
    }
    // The main installation, as the test channels may follow it.
    let main_card = cards.iter().find(|(_, _, main)| *main).map(|(title, ..)| title.clone());
    let main_source = paths.setting(translation::SETTING_SOURCE).unwrap_or_default();
    let closing = main_folder.is_some();
    let closing_title = t("s_tq_abschluss_kopf");
    let mut groups: Vec<(&str, usize)> = cards.iter().map(|(title, ..)| (title.as_str(), 1)).collect();
    if closing {
        groups.push((closing_title.as_str(), 1));
    }
    crate::settings::settings_page(ui, scheme, "uebersetzung", &groups, &mut |c, g, _| {
        let Some((title, root, main)) = cards.get(g) else {
            // Under the cards of the channels: what happened last and the warning.
            said_line(c, scheme, state);
            c.add_space(8.0);
            widgets::status_box(c, scheme, scheme.gold, "shield-alert", &t("s_tq_warn"), &t("s_tq_warn_h"));
            return;
        };
        let follows = (!main && versekit_core::paths::FOLLOWS_LIVE.contains(&title.as_str()))
            .then(|| main_card.as_deref().map(|main| (main, main_source.as_str())))
            .flatten();
        card_body(c, scheme, state, &paths, title, root, *main, false, follows);
    });
    questions(ui, scheme, state);
}

/// The choice of the translation for the main installation alone, for the setup assistant: the card of the page
/// (the language, its source, what is chosen and its state), the line that tells what happened, and the question
/// before a source replaces the text file. Channels besides the main installation stay on the page of the
/// translation.
pub fn show_main_choice(ui: &mut Ui, scheme: &Scheme, state: &mut GameTextsState) {
    state.poll();
    let Some(paths) = state.paths.clone() else { return };
    let Some(root) = paths.game_folder() else {
        no_game(ui, scheme);
        return;
    };
    let f = paths.host.flavor;
    let same = |a: &str, b: &str| f.normcase(&f.normpath(a)) == f.normcase(&f.normpath(b));
    let title = paths
        .installed_channels()
        .into_iter()
        .filter(|(name, _, p4k)| *p4k && translation::CHANNELS.contains(&name.as_str()))
        .find(|(_, folder, _)| same(folder, &root))
        .map_or_else(|| channel_title(&f.basename(&root)), |(name, _, _)| name);
    // The line of what happened stands at the end of the card (`assistant`).
    widgets::card(ui, scheme, |c| {
        c.set_width(c.available_width());
        widgets::card_title(c, scheme, &title);
        card_body(c, scheme, state, &paths, &title, &root, true, true, None);
    });
    questions(ui, scheme, state);
}

/// The setting that says which test channels take the translation of the main installation: {channel: bool}.
pub const SETTING_FOLLOWS: &str = "kanal_wie_live";

/// Bavarian, a language of its own in the list though it lies in the German folder.
const BAVARIAN: &str = "luftwerft_bar";

/// A language of the list: its key (the language folder), its name, its flag, its sources in their order, and
/// whether it is the one for fun (Bavarian), which says that it is a work in progress.
struct Language {
    key: String,
    label: String,
    flag: Option<&'static str>,
    sources: Vec<String>,
    fun: bool,
}

/// The languages a channel can be given, from the sources it can be given (Bavarian apart from German).
fn languages(paths: &Paths, channel: Option<&str>) -> Vec<Language> {
    let mut out = Vec::new();
    for group in translation::grouped_sources(paths, channel, true) {
        let (fun, plain): (Vec<String>, Vec<String>) = group.into_iter().partition(|id| id == BAVARIAN);
        for (sources, fun) in [(plain, false), (fun, true)] {
            // The name of the language from a translation of it (StarStrings names no language).
            let Some(first) = sources.iter().find(|id| *id != "starstrings").or(sources.first()).cloned() else {
                continue;
            };
            let name = translation::display_name(&first);
            let mut label = name.split(" (").next().unwrap_or(&name).to_string();
            if fun {
                label.push_str(" [FUN]");
            }
            let key = if fun { BAVARIAN.to_string() } else { translation::language_folder(paths, &first, channel) };
            out.push(Language { key, label, flag: translation::flag_of(&first), sources, fun });
        }
    }
    out
}

/// The short name of a source on its chip: who makes it (Luftwerft, Dymerz, StarStrings) or „aus dem Spiel“.
fn provider(id: &str) -> String {
    if id == "starstrings" {
        return "StarStrings".to_string();
    }
    let name = translation::display_name(id);
    name.split_once('(').and_then(|(_, rest)| rest.split_once(')')).map_or(name.clone(), |(who, _)| who.to_string())
}

/// What a test channel does with the translation of the main installation: follows it (`Some(true)`), keeps its own
/// (`Some(false)`), or nothing stored yet (`None`: it follows where both are the same now).
fn follows_main(paths: &Paths, channel: &str) -> Option<bool> {
    let stored = paths.settings().into_iter().find(|(k, _)| k == SETTING_FOLLOWS).map(|(_, v)| v);
    match stored {
        Some(Value::Object(map)) => map.iter().find(|(k, _)| k == channel).and_then(|(_, v)| match v {
            Value::Bool(on) => Some(*on),
            _ => None,
        }),
        _ => None,
    }
}

/// Stores whether the test channel `channel` follows the main installation.
fn set_follows_main(paths: &Paths, channel: &str, on: bool) {
    let mut map = match paths.settings().into_iter().find(|(k, _)| k == SETTING_FOLLOWS).map(|(_, v)| v) {
        Some(Value::Object(map)) => map,
        _ => Vec::new(),
    };
    map.retain(|(k, _)| k != channel);
    map.push((channel.to_string(), Value::Bool(on)));
    paths.set_setting(SETTING_FOLLOWS, Value::Object(map));
}

/// The body of the card of a channel: what is chosen and its state, the language, its source, for a test channel
/// whether it takes the translation of the main installation (`follows`: its title and source), the buttons and
/// „Mehr …“ with the own address. `assistant`: it stands in the setup assistant, where „Keine Übersetzung“ asks
/// nothing and writes nothing (as in 4.12), nothing is marked before the player chose, and the line of what happened
/// stands at the end of the card.
#[allow(clippy::too_many_arguments)]
fn card_body(
    c: &mut Ui,
    scheme: &Scheme,
    state: &mut GameTextsState,
    paths: &Paths,
    title: &str,
    root: &str,
    main: bool,
    assistant: bool,
    follows: Option<(&str, &str)>,
) {
    let channel = (!main).then_some(title);
    let current = if main {
        paths.setting(translation::SETTING_SOURCE).unwrap_or_default()
    } else {
        translation::channel_sources(paths)
            .into_iter()
            .find(|(k, _)| k == title)
            .and_then(|(_, v)| v.as_str().map(str::to_string))
            .unwrap_or_default()
    };
    let card_key = format!("{title}{main}");
    let busy = state.busy().then(|| t("inj_laeuft"));
    // A test channel that follows the main installation takes its translation (once per change, also where it fails).
    let following = follows.map(|(main_title, source)| {
        let on = follows_main(paths, title).unwrap_or(current == source);
        (main_title, source, on)
    });
    if let Some((_, source, true)) = following
        && current != source
        && !state.busy()
        && state.question.is_none()
        && !state.followed.iter().any(|(k, s)| k == title && s == source)
    {
        state.followed.push((title.to_string(), source.to_string()));
        if source.is_empty() {
            game_texts::leave_alone(paths, Some(title), root);
        } else if source == CUSTOM || translation::available(paths, source, channel) {
            state.choose(source, channel, root);
        }
    }
    let led = following.is_some_and(|(_, _, on)| on);
    // What is chosen and how it stands, first: the card says at once what it holds.
    let shown = if assistant && state.left_alone {
        tf("s_tq_unberuehrt", &[("0", Arg::from(title))])
    } else if current.is_empty() {
        if main { t("s_tq_nichts_gewaehlt") } else { tf("s_tq_unberuehrt", &[("0", Arg::from(title))]) }
    } else {
        let stand = translation::status_text(paths, &current, channel, language::current(), &translation::today());
        if stand.is_empty() {
            translation::display_name(&current)
        } else {
            format!("{} · {stand}", translation::display_name(&current))
        }
    };
    hint(c, scheme, &shown);
    c.add_space(4.0);
    let langs = languages(paths, channel);
    let picked_language = state.picked_language.iter().find(|(k, _)| *k == card_key).map(|(_, l)| l.clone());
    let from_current = langs.iter().position(|l| l.sources.contains(&current));
    let language = match picked_language.as_deref() {
        Some("") => None,
        Some(key) => langs.iter().position(|l| l.key == key),
        None => from_current
            .or_else(|| state.default_language.as_deref().and_then(|key| langs.iter().position(|l| l.key == key))),
    };
    // In the assistant nothing is marked before the player chose.
    let none_chosen = if assistant { state.left_alone } else { current.is_empty() && language.is_none() };
    let mut picked: Option<String> = None;
    let mut new_language: Option<String> = None;
    c.add_enabled_ui(!led, |c| {
        // The language: a list with the flag of each, „Keine Übersetzung“ first.
        let nothing = t("s_tq_nichts");
        let selected = match (language, current.as_str()) {
            (Some(k), _) => langs[k].label.clone(),
            (None, CUSTOM) => t("s_sp_q_eigen"),
            _ if none_chosen || current.is_empty() => nothing.clone(),
            _ => translation::display_name(&current),
        };
        let name = t("s_tq_sprache");
        crate::settings::setting_row(c, scheme, &name, "", false, |row| {
            // The flag of the language in front of the list, on its line.
            row.horizontal(|row| {
                if let Some(code) = language.and_then(|k| langs[k].flag)
                    && let Some((image, size)) = widgets::flag(row, code)
                {
                    row.add(image.fit_to_exact_size(size));
                }
                crate::settings::choice_box(row, ("uebersetzung-sprache", &card_key), &name, &selected, |list| {
                    if list.selectable_label(none_chosen, &nothing).on_hover_text(t("s_tq_nichts_h")).clicked() {
                        picked = Some(String::new());
                    }
                    for (k, lang) in langs.iter().enumerate() {
                        let text = if lang.fun {
                            format!("{}  ·  {}", lang.label, t("s_tq_bayrisch_wip"))
                        } else {
                            lang.label.clone()
                        };
                        let on = language == Some(k);
                        list.horizontal(|item| {
                            if let Some(code) = lang.flag
                                && let Some((image, size)) = widgets::flag(item, code)
                            {
                                item.add(image.fit_to_exact_size(size));
                            }
                            if item.selectable_label(on, text).clicked() {
                                new_language = Some(lang.key.clone());
                            }
                        });
                    }
                });
            });
        });
        // The sources of that language: who makes it, the one chosen stands out.
        if let Some(k) = language {
            crate::settings::setting_row(c, scheme, &t("s_tq_quelle"), "", false, |row| {
                for id in &langs[k].sources {
                    let chosen = if assistant { !state.left_alone && *id == current } else { *id == current };
                    widgets::drawn_cards::note_choice(&format!("uebersetzung-quelle-{card_key}"), chosen);
                    let kind = if chosen { ButtonKind::Strong } else { ButtonKind::Normal };
                    let short = provider(id);
                    // Named by its whole name for screen readers: „Deutsch (Dymerz)“.
                    let spoken = translation::display_name(id);
                    let button = widgets::Button::new(&short, kind, scheme).spoken(&spoken).blocked(busy.as_deref());
                    if row.add(button).clicked() {
                        picked = Some(id.clone());
                    }
                }
            });
        }
    });
    // A test channel: whether it takes the translation of the main installation.
    if let Some((main_title, source, on)) = following {
        let mut follow = on;
        let name = tf("s_tq_wie", &[("0", Arg::from(main_title))]);
        let help = tf("s_tq_wie_h", &[("0", Arg::from(main_title))]);
        if widgets::switch_row_help(c, scheme, &name, &help, &mut follow).changed() {
            set_follows_main(paths, title, follow);
            if follow {
                // Taken over now, once.
                state.followed.retain(|(k, s)| !(k == title && s == source));
            }
        }
    }
    if let Some(key) = new_language {
        state.picked_language.retain(|(k, _)| *k != card_key);
        state.picked_language.push((card_key.clone(), key));
    }
    if let Some(id) = picked {
        if id.is_empty() {
            state.picked_language.retain(|(k, _)| *k != card_key);
            state.picked_language.push((card_key.clone(), String::new()));
        }
        if main && id.is_empty() && assistant {
            // As in 4.12: doing nothing is the truth. Nothing is asked, nothing written or deleted.
            state.left_alone = true;
        } else if main && id.is_empty() {
            state.question = Some(Question::LeaveAlone);
        } else if main {
            state.choose(&id, None, root);
            // A source that needs no question is chosen now; one that asks first changes the mark only
            // with the answer "Ja".
            if state.question.is_none() {
                state.left_alone = false;
            }
        } else if id.is_empty() {
            game_texts::leave_alone(paths, Some(title), root);
            state.say(tf("s_tq_unberuehrt", &[("0", Arg::from(title))]), false);
        } else {
            state.choose(&id, channel, root);
        }
    }
    c.add_space(4.0);
    let target = if current.is_empty() {
        None
    } else {
        versekit_core::game_ini::target_ini_in(paths, root, &translation::language_folder(paths, &current, channel))
    };
    let backups = target.as_deref().map_or(0, |t| translation::backups_of(t).len());
    let mut check = false;
    let mut restore = false;
    let mut own = false;
    c.horizontal_wrapped(|row| {
        // Without a source there is nothing to look for.
        let sourceless = (current.is_empty() || current == ORIGINAL).then(|| t("s_tq_pruefen_keine"));
        let reason = busy.clone().or(sourceless);
        check = row
            .add(widgets::Button::new(&t("s_tq_pruefen"), ButtonKind::Normal, scheme).blocked(reason.as_deref()))
            .clicked();
        let none = (backups == 0).then(|| t("s_tq_zurueck_keine"));
        let reason = busy.clone().or(none);
        restore = row
            .add(widgets::Button::new(&t("s_tq_zurueck"), ButtonKind::Normal, scheme).blocked(reason.as_deref()))
            .clicked();
        own = row
            .add(widgets::Button::new(&t("s_sp_q_eigen"), ButtonKind::Normal, scheme))
            .clicked();
    });
    if own {
        state.custom_of(&card_key, paths, channel).open = true;
    }
    if check {
        state.check_channel(&current, channel, root);
    }
    if restore && let Some(target) = target {
        state.restore(target, translation::note_key(&current, channel));
    }
    let open = state.custom_of(&card_key, paths, channel).open || current == CUSTOM;
    if open {
        c.add_space(4.0);
        custom_box(c, scheme, state, paths, &card_key, channel, root);
    }
    // In the assistant the card stands alone: the line that tells what happened closes it.
    if assistant {
        said_line(c, scheme, state);
    }
}

/// Without a game folder: the sentence that says where to set it, in a card of its own.
fn no_game(ui: &mut Ui, scheme: &Scheme) {
    widgets::card(ui, scheme, |c| {
        widgets::card_title(c, scheme, &t("s_ei_spielordner"));
        hint(c, scheme, &t("s_tq_kein_spiel"));
    });
}

fn custom_box(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &mut GameTextsState,
    paths: &Paths,
    card_key: &str,
    channel: Option<&str>,
    root: &str,
) {
    hint(ui, scheme, &t("s_tq_url_h"));
    let mut take = false;
    {
        let custom = state.custom_of(card_key, paths, channel);
        let width = ui.available_width();
        ui.add(
            widgets::text_field(&mut custom.url, &t("s_tq_url_platz"), scheme)
                .id_salt(("eigene-adresse", card_key))
                .desired_width(width),
        );
        hint(ui, scheme, &t("s_tq_sprache_h"));
        ui.horizontal_wrapped(|row| {
            for language in versekit_core::game_ini::GAME_LANGUAGES {
                let kind = if custom.language == language { ButtonKind::Strong } else { ButtonKind::Normal };
                if row.add(widgets::Button::new(language, kind, scheme)).clicked() {
                    custom.language = language.to_string();
                }
            }
        });
        if ui.add(widgets::Button::new(&t("s_tq_uebernehmen"), ButtonKind::Strong, scheme)).clicked() {
            take = true;
        }
    }
    if take {
        let custom = state.custom_of(card_key, paths, channel).clone();
        if translation::set_custom(paths, &custom.url, &custom.language, channel).is_none() {
            state.say(t("s_tq_url_falsch"), true);
        } else {
            state.choose(CUSTOM, channel, root);
        }
    }
}

/// The open question, as a dialog.
fn questions(ui: &mut Ui, scheme: &Scheme, state: &mut GameTextsState) {
    let Some(question) = state.question.clone() else { return };
    let (title, text) = match &question {
        Question::Source { source, .. } => {
            let mut text = tf("s_sp_warnung", &[("0", Arg::from(translation::display_name(source)))]);
            if let Some(hint) = translation::source(source).and_then(|q| q.hint) {
                text.push_str("\n\n");
                text.push_str(&t(hint));
            }
            (t("s_sp_warnung_titel"), text)
        }
        Question::LeaveAlone => (t("s_tq_nichts"), t("s_tq_nichts_warnung")),
    };
    // While the background writes, a yes would be refused and lost: it waits.
    let busy = state.busy().then(|| t("inj_laeuft"));
    let ((yes, no), close) = widgets::dialog(ui.ctx(), scheme, "spieltexte-frage", |d| {
        widgets::heading(d, scheme, &title);
        d.add(egui::Label::new(text.as_str()).wrap());
        d.add_space(8.0);
        d.horizontal(|row| {
            let no = row.add(widgets::Button::new(&t("e_nein"), ButtonKind::Normal, scheme)).clicked();
            let yes = row
                .add(widgets::Button::new(&t("e_ja"), ButtonKind::Strong, scheme).blocked(busy.as_deref()))
                .clicked();
            (yes, no)
        })
        .inner
    });
    if no || close {
        state.question = None;
        return;
    }
    if !yes {
        return;
    }
    state.question = None;
    let Some(paths) = state.paths.clone() else { return };
    match question {
        Question::Source { source, channel, root } => {
            // The player took a source after all: "Keine Übersetzung" no longer stands for the choice.
            state.left_alone = false;
            game_texts::confirm(&paths, &source);
            state.fetch_source(source, channel, root);
        }
        Question::LeaveAlone => {
            if let Some(root) = paths.game_folder() {
                game_texts::leave_alone(&paths, None, &root);
            } else {
                paths.set_setting(translation::SETTING_SOURCE, Value::str(""));
            }
            state.say(t("s_tq_nichts_ok"), false);
        }
    }
}

#[cfg(test)]
mod tests {
    // Every button and switch of both pages, clicked as a player would, and
    // the files it wrote: in a made-up game folder in a throwaway folder, with
    // a stand-in for the net.

    use std::fs;
    use std::sync::Mutex;

    use egui::accesskit::Role;
    use versekit_core::net::Fake;
    use versekit_core::paths::Host;
    use versekit_core::pypath::Flavor;

    use super::*;

    const GAME: &str = "Bounty_title_001=Kopfgeld\nBounty_desc_001=Fang ihn.\nitem_NameCOOL_A=Glacier\nitem_DescCOOL_A=Größe: 1\\nGütegrad: A\\nKlasse: Militär\n";
    /// The German source the tests take, with its newest release and the address of its file.
    const GERMAN: &str = "dymerz_de";
    const RELEASE: &str = "https://api.github.com/repos/Dymerz/StarCitizen-Localization/releases/latest";
    const RELEASE_JSON: &str = r#"{"tag_name": "v2", "published_at": "2026-10-20T10:00:00Z", "assets": [{"name": "german_.germany.zip", "browser_download_url": "https://dl/de.zip", "size": 10}]}"#;
    const DOWNLOAD: &str = "https://dl/de.zip";

    struct Game {
        root: String,
        paths: Paths,
        live: String,
        /// The language is one for the whole process and the answers of a step are written in its thread: a test
        /// that compares them with `t(..)` keeps the language still until it ends (last field: it drops after the folder
        /// is removed).
        _language: std::sync::MutexGuard<'static, ()>,
    }

    impl Game {
        fn new(name: &str) -> Game {
            let language = crate::language_lock();
            versekit_core::language::set_language(versekit_core::language::Choice::De);
            let root = std::env::temp_dir()
                .join(format!("versekit-gametexts-page-{name}-{}", versekit_core::testdir::run_tag()));
            let _ = fs::remove_dir_all(&root);
            let root = root.to_string_lossy().into_owned();
            let live = format!("{root}/StarCitizen/LIVE");
            let data = format!("{root}/data");
            fs::create_dir_all(format!("{live}/data/Localization/german_(germany)")).unwrap();
            fs::create_dir_all(&data).unwrap();
            fs::write(format!("{live}/Game.log"), "").unwrap();
            fs::write(format!("{live}/Data.p4k"), "").unwrap();
            let catalog = r#"{"geholt": "x", "bauplaene": {}, "missionen": {"Bounty_title_001": {"bp": ["Atzkav Sniper Rifle"], "rep": 0, "titel_key": "Bounty_title_001", "text_key": "Bounty_desc_001"}}}"#;
            fs::write(format!("{data}/katalog-cache.json"), catalog).unwrap();
            let host = Host::new(
                Flavor::Posix,
                &[("HOME", root.as_str()), ("SC_BP_HOME", &data), ("SC_INSTALL_DIR", &live)],
                &root,
            );
            Game { root, paths: Paths::new(host), live, _language: language }
        }

        fn ini(&self) -> String {
            format!("{}/data/Localization/german_(germany)/global.ini", self.live)
        }

        fn text(&self) -> String {
            fs::read_to_string(self.ini()).unwrap_or_default()
        }

        /// The German translation set up, notes not yet written.
        fn german(&self) {
            fs::write(self.ini(), GAME).unwrap();
            translation::note_set(&self.paths, GERMAN, Some("v-old"), Some("german_(germany)"), None);
            self.paths.set_setting(translation::SETTING_SOURCE, Value::str(GERMAN));
        }
    }

    impl Drop for Game {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    /// A stand-in for the German source's releases with a newer file.
    fn german_net() -> Arc<dyn Net> {
        let mut file =
            "Bounty_title_001=Kopfgeld neu\nBounty_desc_001=Fang ihn.\nvehicle_NameAEGS_Gladius=Gladius\n".to_string();
        for i in 0..5001 {
            file.push_str(&format!("k{i}=v\n"));
        }
        Arc::new(Fake::default().with(RELEASE, RELEASE_JSON).with(DOWNLOAD, file))
    }

    struct Page {
        ctx: egui::Context,
    }

    enum Which {
        Game,
        Translation,
    }

    impl Page {
        fn new() -> Page {
            let ctx = egui::Context::default();
            crate::install(&ctx);
            ctx.enable_accesskit();
            Page { ctx }
        }

        /// One frame; the labelled nodes with their middle.
        fn frame(
            &self,
            which: &Which,
            state: &mut GameTextsState,
            events: Vec<egui::Event>,
        ) -> (Vec<(Role, String, egui::Pos2)>, bool) {
            let (nodes, jumped) = self.frame_sized(which, state, 1000.0, events);
            let middles = nodes.into_iter().map(|(role, label, rect)| (role, label, rect.center())).collect();
            (middles, jumped)
        }

        /// One frame of a page `width` points wide; the labelled nodes with their rectangle.
        fn frame_sized(
            &self,
            which: &Which,
            state: &mut GameTextsState,
            width: f32,
            events: Vec<egui::Event>,
        ) -> (Vec<(Role, String, egui::Rect)>, bool) {
            let scheme = versekit_core::theme::current();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 5000.0))),
                events,
                ..Default::default()
            };
            let mut jumped = false;
            let mut output = self.ctx.run_ui(input, |ui| match which {
                Which::Game => jumped = show_game(ui, scheme, state).is_some(),
                Which::Translation => show_translation(ui, scheme, state),
            });
            output.textures_delta.clear();
            let Some(update) = output.platform_output.accesskit_update else { return (Vec::new(), jumped) };
            let nodes = update
                .nodes
                .iter()
                .filter_map(|(_, n)| {
                    // Texts of labels and boxes stand in the value, buttons and switches in the label.
                    let label = n.label().or_else(|| n.value())?.to_owned();
                    let b = n.bounds()?;
                    let rect = egui::Rect::from_min_max(
                        egui::pos2(b.x0 as f32, b.y0 as f32),
                        egui::pos2(b.x1 as f32, b.y1 as f32),
                    );
                    Some((n.role(), label, rect))
                })
                .collect();
            (nodes, jumped)
        }

        /// Click the first control of `role` named `name`; whether there was one.
        fn click(&self, which: &Which, state: &mut GameTextsState, role: Role, name: &str) -> bool {
            self.click_nth(which, state, role, name, 0)
        }

        /// Click the control of `role` named `name` that comes `nth` from the
        /// top (the cards repeat their buttons).
        fn click_nth(&self, which: &Which, state: &mut GameTextsState, role: Role, name: &str, nth: usize) -> bool {
            let (nodes, _) = self.frame(which, state, Vec::new());
            let mut found: Vec<egui::Pos2> =
                nodes.iter().filter(|(r, l, _)| *r == role && l == name).map(|(_, _, at)| *at).collect();
            // From the top, and on one height from the left (the cards stand side by side).
            found.sort_by(|a, b| {
                (a.y.round(), a.x).partial_cmp(&(b.y.round(), b.x)).unwrap_or(std::cmp::Ordering::Equal)
            });
            let Some(&at) = found.get(nth) else { return false };
            self.press(which, state, at);
            true
        }

        /// Click the first element of any role named `name` (an entry of an open list).
        fn click_any(&self, which: &Which, state: &mut GameTextsState, name: &str) -> bool {
            let (nodes, _) = self.frame(which, state, Vec::new());
            let Some(at) = nodes.iter().find(|(_, l, _)| l == name).map(|(_, _, at)| *at) else { return false };
            self.press(which, state, at);
            true
        }

        /// Picks `entry` in the list of languages of the card `card` (from the top and the left).
        fn pick(&self, which: &Which, state: &mut GameTextsState, card: usize, entry: &str) -> bool {
            self.click_nth(which, state, Role::ComboBox, &t("s_tq_sprache"), card)
                && self.click_any(which, state, entry)
        }

        /// Clicks „Eigene Adresse“ of the card `card`.
        fn own_address(&self, which: &Which, state: &mut GameTextsState, card: usize) -> bool {
            self.click_nth(which, state, Role::Button, &t("s_sp_q_eigen"), card)
        }

        fn press(&self, which: &Which, state: &mut GameTextsState, at: egui::Pos2) {
            let press = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::default(),
            };
            self.frame(which, state, vec![egui::Event::PointerMoved(at), press(true), press(false)]);
        }
    }

    /// The language folder of German, for the cards that show its sources.
    fn german_folder() -> String {
        "german_(germany)".to_string()
    }

    /// Wait until the step on its thread is done.
    fn settle(state: &mut GameTextsState) {
        for _ in 0..500 {
            state.poll();
            if !state.busy() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the step did not end");
    }

    #[test]
    fn a_folder_is_named_by_the_channel_it_ends_with() {
        for (folder, title) in [
            ("Spieldaten LIVE", "LIVE"),
            ("spieldaten-ptu", "PTU"),
            ("Kopie_TECH-PREVIEW", "TECH-PREVIEW"),
            ("Spieldaten EPTU", "EPTU"),
            ("EPTU", "EPTU"),
            ("Spieldaten-EPTU", "EPTU"),
            ("Spieldaten", "Spieldaten"),
            ("MEINLIVE", "MEINLIVE"),
        ] {
            assert_eq!(channel_title(folder), title, "{folder}");
        }
    }

    /// A test copy given from outside (`Spieldaten LIVE`, a PTU next to it):
    /// one card per channel of the copy, named by the channel, none for an
    /// installation in the usual places; every translation with its flag.
    #[test]
    fn a_given_copy_shows_its_own_channels_with_flags() {
        let root =
            std::env::temp_dir().join(format!("versekit-gametexts-page-copy-{}", versekit_core::testdir::run_tag()));
        let _ = fs::remove_dir_all(&root);
        let root = root.to_string_lossy().into_owned();
        let copy = format!("{root}/Testumgebung/Spieldaten LIVE");
        let real = format!("{root}/.wine/drive_c/Program Files/Roberts Space Industries/StarCitizen");
        let data = format!("{root}/data");
        for folder in [copy.clone(), format!("{root}/Testumgebung/PTU"), format!("{real}/LIVE"), format!("{real}/PTU")]
        {
            fs::create_dir_all(&folder).unwrap();
            fs::write(format!("{folder}/Data.p4k"), "").unwrap();
            fs::write(format!("{folder}/Game.log"), "").unwrap();
        }
        fs::create_dir_all(&data).unwrap();
        let host = Host::new(
            Flavor::Posix,
            &[("HOME", root.as_str()), ("SC_BP_HOME", &data), ("SC_INSTALL_DIR", &copy)],
            &root,
        );
        let paths = Paths::new(host);
        let mut state = GameTextsState::with_parts(&paths, Arc::new(Fake::default()), Arc::new(game_texts::NoArchive));
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        let scheme = versekit_core::theme::current();
        let (mut labels, mut flags) = (Vec::new(), 0);
        // The flag of the language beside the list of each card (the main one and the PTU).
        state.default_language = Some(german_folder());
        let sources = 2;
        // The flags are decoded next to the frames, each on its own: frames until all are there
        // (at most ten seconds, the whole test suite may be running beside it).
        for _ in 0..500 {
            std::thread::sleep(Duration::from_millis(20));
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 5000.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| show_translation(ui, scheme, &mut state));
            output.textures_delta.clear();
            let names = ctx.tex_manager();
            let names = names.read();
            flags = output
                .shapes
                .iter()
                .filter(|c| {
                    let id = c.shape.texture_id();
                    names.meta(id).is_some_and(|meta| meta.name.starts_with("bytes://flaggen/"))
                })
                .count();
            labels = output
                .shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_string()),
                    _ => None,
                })
                .collect();
            if flags >= sources {
                break;
            }
        }
        let count = |name: &str| labels.iter().filter(|l| *l == name).count();
        assert_eq!((count("LIVE"), count("PTU")), (1, 1), "{labels:?}");
        assert_eq!(count("Spieldaten LIVE"), 0, "the copy is named by its channel");
        assert_eq!(flags, sources, "the flag of the language on both cards");
        let _ = fs::remove_dir_all(&root);
    }

    /// All five channels installed: five cards, one per channel, each named by it.
    #[test]
    fn five_channels_give_five_cards() {
        let game = Game::new("fuenf");
        let base = game.live.trim_end_matches("/LIVE").to_string();
        for channel in versekit_core::paths::CHANNELS.iter().filter(|c| **c != "LIVE") {
            let folder = format!("{base}/{channel}");
            fs::create_dir_all(&folder).unwrap();
            fs::write(format!("{folder}/Data.p4k"), "").unwrap();
            fs::write(format!("{folder}/Game.log"), "").unwrap();
        }
        let mut state =
            GameTextsState::with_parts(&game.paths, Arc::new(Fake::default()), Arc::new(game_texts::NoArchive));
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let scheme = versekit_core::theme::current();
        let mut labels: Vec<String> = Vec::new();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 5000.0))),
                ..Default::default()
            };
            let output = ctx.run_ui(input, |ui| show_translation(ui, scheme, &mut state));
            labels = output
                .shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_string()),
                    _ => None,
                })
                .collect();
        }
        assert_eq!(versekit_core::paths::CHANNELS.len(), 5);
        for channel in versekit_core::paths::CHANNELS {
            assert_eq!(labels.iter().filter(|l| *l == channel).count(), 1, "one card {channel}: {labels:?}");
        }
        // Every channel beside LIVE can follow it, HOTFIX too: four switches.
        let follow = tf("s_tq_wie", &[("0", Arg::from("LIVE"))]);
        assert_eq!(labels.iter().filter(|l| **l == follow).count(), 4, "{labels:?}");
    }

    #[test]
    fn on_a_copy_of_the_data_looking_says_calmly_what_is_out_and_fetches_nothing() {
        let game = Game::new("testkopie");
        game.german();
        // As on a copy of the data: writing only into the data folder, the game folder lies outside. The page
        // builds its own paths; the lock is what its threads take over.
        versekit_core::writeguard::deny(true);
        versekit_core::writeguard::allow_under(Some(&format!("{}/data", game.root)));
        let mut state = GameTextsState::with_parts(&game.paths, german_net(), Arc::new(game_texts::NoArchive));
        state.check_now();
        settle(&mut state);
        let (said, bad) = state.said().cloned().expect("an answer");
        assert!(!bad, "a lock that is wanted is no failure: {said}");
        assert!(said.contains("Neue Fassung vom 20.10.2026 gibt es, im Testbetrieb nicht eingespielt"), "{said}");
        state.check_channel(GERMAN, None, &game.live);
        settle(&mut state);
        let (said, bad) = state.said().cloned().expect("an answer");
        versekit_core::writeguard::deny(false);
        versekit_core::writeguard::allow_under(None);
        let denied = versekit_core::writeguard::take_denied();
        assert!(!bad && said.contains("im Testbetrieb nicht eingespielt"), "{said}");
        assert_eq!(game.text(), GAME, "the text file stays as it was");
        assert!(denied.is_empty(), "nothing was even tried in the game folder: {denied:?}");
        let state_line = translation::status_text(
            &game.paths,
            GERMAN,
            None,
            versekit_core::language::Lang::De,
            &translation::today(),
        );
        assert!(!state_line.contains("gesperrt") && state_line.contains("nicht eingespielt"), "{state_line}");
    }

    #[test]
    fn insert_again_and_remove_again_write_and_restore_the_file() {
        let game = Game::new("hand");
        game.german();
        let mut state =
            GameTextsState::with_parts(&game.paths, Arc::new(Fake::default()), Arc::new(game_texts::NoArchive));
        let page = Page::new();
        assert!(page.click(&Which::Game, &mut state, Role::Button, &t("s_sp_jetzt")));
        settle(&mut state);
        assert!(game.text().contains("Kopfgeld <EM4>[BP]</EM4>"), "{}", game.text());
        assert!(game.text().contains("Glacier (Mil/1/A)"));
        assert!(page.click(&Which::Game, &mut state, Role::Button, &t("s_sp_weg")));
        settle(&mut state);
        assert_eq!(game.text(), GAME);
        // Title, description and item name went back.
        assert_eq!(state.said().cloned(), Some((tf("inj_entfernt", &[("0", Arg::from(3i64))]), false)));
    }

    #[test]
    fn the_main_switch_takes_the_notes_out_and_writes_them_again() {
        let game = Game::new("switch");
        game.german();
        let mut state =
            GameTextsState::with_parts(&game.paths, Arc::new(Fake::default()), Arc::new(game_texts::NoArchive));
        state.apply_now();
        settle(&mut state);
        let page = Page::new();
        assert!(page.click(&Which::Game, &mut state, Role::CheckBox, &t("s_sp_an")));
        settle(&mut state);
        assert!(!game.paths.setting_bool(game_texts::SETTING_ON, true));
        assert_eq!(game.text(), GAME, "off takes them out");
        assert!(page.click(&Which::Game, &mut state, Role::CheckBox, &t("s_sp_an")));
        settle(&mut state);
        assert!(game.text().contains("<EM4>[BP]</EM4>"), "on writes them again");
    }

    #[test]
    fn a_detail_switch_writes_the_notes_anew_without_that_detail() {
        let game = Game::new("detail");
        game.german();
        let mut state =
            GameTextsState::with_parts(&game.paths, Arc::new(Fake::default()), Arc::new(game_texts::NoArchive));
        state.apply_now();
        settle(&mut state);
        let page = Page::new();
        assert!(page.click(&Which::Game, &mut state, Role::CheckBox, &t("s_sp_angaben")));
        settle(&mut state);
        assert!(!game.paths.setting_bool(injection::SETTING_DETAILS, true));
        assert!(game.text().contains("item_NameCOOL_A=Glacier\n"), "{}", game.text());
        assert!(game.text().contains("<EM4>[BP]</EM4>"));
        for (label, setting) in [
            ("s_sp_rang", ranks::SETTING),
            ("s_sp_titelruf", injection::SETTING_TITLE_REP),
            ("s_sp_itinfo", item_info::SETTING),
        ] {
            assert!(page.click(&Which::Game, &mut state, Role::CheckBox, &t(label)), "{label}");
            settle(&mut state);
            assert!(!game.paths.setting_bool(setting, true), "{label}");
        }
        assert!(page.click(&Which::Game, &mut state, Role::CheckBox, &t("s_sp_auto")));
        assert!(!game.paths.setting_bool(game_texts::SETTING_AUTO, true));
    }

    #[test]
    fn a_switch_during_a_step_is_carried_out_after_it() {
        let game = Game::new("busy");
        game.german();
        let mut state =
            GameTextsState::with_parts(&game.paths, Arc::new(Fake::default()), Arc::new(game_texts::NoArchive));
        state.apply_now();
        settle(&mut state);
        // A detail switched while a writing runs: written anew afterwards.
        state.busy = true;
        state.switch_detail(injection::SETTING_DETAILS, false, "s_sp_angaben_sagen");
        assert!(state.apply_after);
        state.busy = false;
        state.poll();
        settle(&mut state);
        assert!(game.text().contains("item_NameCOOL_A=Glacier\n"), "{}", game.text());
        // The notes switched off during a step: taken out afterwards.
        state.busy = true;
        state.switch_on(false);
        state.busy = false;
        state.poll();
        settle(&mut state);
        assert_eq!(game.text(), GAME);
    }

    #[test]
    fn the_check_says_whether_something_new_is_there_and_the_way_to_the_translation() {
        let game = Game::new("check");
        game.german();
        let mut state = GameTextsState::with_parts(&game.paths, german_net(), Arc::new(game_texts::NoArchive));
        let page = Page::new();
        assert!(page.click(&Which::Game, &mut state, Role::Button, &t("s_sp_pruefen")));
        settle(&mut state);
        let said = state.said().cloned().unwrap_or_default().0;
        assert!(said.contains(&tf("inj_update_da", &[("0", Arg::from("v2"))])), "{said}");
        assert_eq!(game.text(), GAME, "looking changes nothing");
        let (nodes, _) = page.frame(&Which::Game, &mut state, Vec::new());
        let at = nodes.iter().find(|(r, l, _)| *r == Role::Button && *l == t("s_sp_zur_uebersetzung")).unwrap().2;
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        };
        let (_, jumped) =
            page.frame(&Which::Game, &mut state, vec![egui::Event::PointerMoved(at), press(true), press(false)]);
        assert!(jumped);
    }

    #[test]
    fn a_source_is_asked_once_fetched_placed_and_the_old_file_comes_back() {
        let game = Game::new("source");
        fs::write(game.ini(), "old=Text\n").unwrap();
        let mut state = GameTextsState::with_parts(&game.paths, german_net(), Arc::new(game_texts::NoArchive));
        let page = Page::new();
        // The language first: its sources come up as chips, named by their whole name.
        assert!(!page.click(&Which::Translation, &mut state, Role::Button, &t("s_sp_q_dymerz_de")), "no source before");
        assert!(page.pick(&Which::Translation, &mut state, 0, "Deutsch"), "the list of languages");
        assert!(state.question.is_none(), "a language alone asks nothing and writes nothing");
        assert!(page.click(&Which::Translation, &mut state, Role::Button, &t("s_sp_q_dymerz_de")));
        assert!(state.question.is_some(), "it asks before it replaces the text file");
        assert!(page.click(&Which::Translation, &mut state, Role::Button, &t("e_ja")));
        settle(&mut state);
        assert!(game.text().starts_with("Bounty_title_001=Kopfgeld neu <EM4>[BP]</EM4>\n"), "{}", &game.text()[..80]);
        assert_eq!(translation::installed(&game.paths, GERMAN, None).as_deref(), Some("v2"));
        assert!(game_texts::confirmed(&game.paths, GERMAN));
        assert!(
            fs::read_to_string(format!("{}/user.cfg", game.live)).unwrap().contains("g_language = german_(germany)")
        );
        assert_eq!(translation::backups_of(&game.ini()).len(), 1);

        assert!(page.click(&Which::Translation, &mut state, Role::Button, &t("s_tq_zurueck")));
        settle(&mut state);
        assert_eq!(game.text(), "old=Text\n", "the file as it was before");
        assert!(translation::backups_of(&game.ini()).is_empty());
        // Without a copy the button is locked: a click starts nothing.
        assert!(page.click(&Which::Translation, &mut state, Role::Button, &t("s_tq_zurueck")));
        assert!(!state.busy());
        assert_eq!(state.said().cloned(), Some((t("s_tq_zurueck_fertig"), false)));
        assert_eq!(game.text(), "old=Text\n");
    }

    #[test]
    fn looking_for_a_new_version_on_a_card_fetches_it() {
        let game = Game::new("cardcheck");
        game.german();
        let mut state = GameTextsState::with_parts(&game.paths, german_net(), Arc::new(game_texts::NoArchive));
        let page = Page::new();
        assert!(page.click(&Which::Translation, &mut state, Role::Button, &t("s_tq_pruefen")));
        settle(&mut state);
        assert!(game.text().starts_with("Bounty_title_001=Kopfgeld neu"), "{}", &game.text()[..60]);
    }

    /// Without a source there is nothing to look for: the button is grey and a click starts nothing.
    #[test]
    fn without_a_source_looking_for_a_new_version_is_locked() {
        let game = Game::new("cardnone");
        fs::write(game.ini(), GAME).unwrap();
        let mut state = GameTextsState::with_parts(&game.paths, german_net(), Arc::new(game_texts::NoArchive));
        let page = Page::new();
        assert!(page.click(&Which::Translation, &mut state, Role::Button, &t("s_tq_pruefen")));
        assert!(!state.busy(), "nothing is looked for");
        assert_eq!(game.text(), GAME, "the file stays");
    }

    /// The link of the overview tile: with nothing chosen it says so and starts nothing; else every channel with
    /// a source looks for a newer version and fetches it, the main installation and the PTU beside it, on one
    /// thread; looked again, all is current.
    #[test]
    fn the_look_of_the_overview_fetches_every_channel_with_a_newer_version() {
        let game = Game::new("allcheck");
        let mut state =
            GameTextsState::with_parts(&game.paths, Arc::new(Fake::default()), Arc::new(game_texts::NoArchive));
        state.check_all();
        assert!(!state.busy(), "nothing chosen starts nothing");
        assert_eq!(state.said().cloned(), Some((t("s_tq_nichts_zu_pruefen"), false)));

        game.german();
        let ptu = format!("{}/StarCitizen/PTU", game.root);
        fs::create_dir_all(&ptu).unwrap();
        fs::write(format!("{ptu}/Game.log"), "").unwrap();
        fs::write(format!("{ptu}/Data.p4k"), "").unwrap();
        assert!(translation::set_channel_source(&game.paths, "PTU", "luftwerft_de"));
        let checks = checkable(&game.paths);
        let shown: Vec<(&str, Option<&str>)> =
            checks.iter().map(|c| (c.source.as_str(), c.channel.as_deref())).collect();
        assert_eq!(shown, [(GERMAN, None), ("luftwerft_de", Some("PTU"))]);

        let mut german = "Bounty_title_001=Kopfgeld neu\nvehicle_NameAEGS_Gladius=Gladius\n".to_string();
        for i in 0..5001 {
            german.push_str(&format!("k{i}=v\n"));
        }
        let side = "https://www.luftwerft.com/download/resources/localizations/PTU/german_(germany)/global.ini";
        let net = Fake::default()
            .with(RELEASE, RELEASE_JSON)
            .with(DOWNLOAD, german)
            .with_head(side, &[("ETag", "\"ptu-1\"")])
            .with(side, direct_file());
        let mut state = GameTextsState::with_parts(&game.paths, Arc::new(net), Arc::new(game_texts::NoArchive));
        state.check_all();
        settle(&mut state);
        let (said, bad) = state.said().cloned().expect("an answer");
        assert!(!bad, "{said}");
        assert!(game.text().starts_with("Bounty_title_001=Kopfgeld neu"), "{}", &game.text()[..60]);
        let placed = fs::read_to_string(format!("{ptu}/data/Localization/german_(germany)/global.ini")).unwrap();
        assert!(placed.starts_with("Bounty_title_001=Kopfgeld PTU <EM4>[BP]</EM4>\n"), "the PTU got its own file");
        assert!(placed.contains("\nk5000=v\n"), "with the notes in it");
        assert!(said.contains("PTU"), "{said}");

        state.check_all();
        settle(&mut state);
        assert_eq!(state.said().cloned(), Some((t("inj_aktuell"), false)), "nothing newer the second time");
    }

    #[test]
    fn leaving_the_main_installation_alone_asks_and_forgets_the_source() {
        let game = Game::new("alone");
        game.german();
        let mut state =
            GameTextsState::with_parts(&game.paths, Arc::new(Fake::default()), Arc::new(game_texts::NoArchive));
        let page = Page::new();
        // „Keine Übersetzung“, the first entry of the list of languages.
        assert!(page.pick(&Which::Translation, &mut state, 0, &t("s_tq_nichts")));
        assert_eq!(state.question, Some(Question::LeaveAlone));
        assert!(page.click(&Which::Translation, &mut state, Role::Button, &t("e_ja")));
        assert_eq!(game.paths.setting(translation::SETTING_SOURCE), None);
        assert_eq!(game.text(), GAME, "the file stays as it is");
    }

    /// A text file as a direct address serves it: many lines and vehicles.
    fn direct_file() -> String {
        let mut file = "Bounty_title_001=Kopfgeld PTU\nvehicle_NameAEGS_Gladius=Gladius\n".to_string();
        for i in 0..5001 {
            file.push_str(&format!("k{i}=v\n"));
        }
        file
    }

    #[test]
    fn a_side_channel_asks_takes_no_for_an_answer_and_then_fetches_only_its_own_file() {
        let game = Game::new("side");
        game.german();
        let ptu = format!("{}/StarCitizen/PTU", game.root);
        fs::create_dir_all(&ptu).unwrap();
        fs::write(format!("{ptu}/Game.log"), "").unwrap();
        fs::write(format!("{ptu}/Data.p4k"), "").unwrap();
        let url = "https://www.luftwerft.com/download/resources/localizations/PTU/german_(germany)/global.ini";
        let net = Arc::new(Fake::default().with_head(url, &[("ETag", "\"ptu-1\"")]).with(url, direct_file()));
        let mut state = GameTextsState::with_parts(&game.paths, net.clone(), Arc::new(game_texts::NoArchive));
        // Both cards show the German sources: the PTU, which has none yet, too.
        state.default_language = Some(german_folder());
        let page = Page::new();
        let name = translation::display_name("luftwerft_de");
        // The second button of that name sits on the card of the PTU (beside the one of LIVE).
        assert!(page.click_nth(&Which::Translation, &mut state, Role::Button, &name, 1));
        assert!(matches!(&state.question, Some(Question::Source { channel: Some(c), .. }) if c == "PTU"));
        assert!(page.click(&Which::Translation, &mut state, Role::Button, &t("e_nein")));
        assert_eq!(state.question, None);
        // A step started by mistake would have asked the net by its end.
        settle(&mut state);
        assert!(net.asked().is_empty(), "no means nothing is fetched");

        assert!(page.click_nth(&Which::Translation, &mut state, Role::Button, &name, 1));
        assert!(page.click(&Which::Translation, &mut state, Role::Button, &t("e_ja")));
        settle(&mut state);
        // Its own file, with the notes and the language as in the main installation.
        let placed = fs::read_to_string(format!("{ptu}/data/Localization/german_(germany)/global.ini")).unwrap();
        assert!(placed.starts_with("Bounty_title_001=Kopfgeld PTU <EM4>[BP]</EM4>\n"), "{}", &placed[..80]);
        assert!(fs::read_to_string(format!("{ptu}/user.cfg")).unwrap().contains("g_language = german_(germany)"));
        assert_eq!(translation::installed(&game.paths, "luftwerft_de", Some("PTU")).as_deref(), Some("etag:ptu-1"));
        assert_eq!(game.text(), GAME, "the main installation stays as it is");
        assert_eq!(game.paths.setting(translation::SETTING_SOURCE).as_deref(), Some(GERMAN));
        assert_eq!(state.said().map(|s| s.1), Some(false), "{:?}", state.said());
    }

    #[test]
    fn an_own_address_is_fetched_after_the_question() {
        let game = Game::new("customok");
        let url = "https://example.org/files/global.ini";
        let net = Arc::new(Fake::default().with_head(url, &[("ETag", "\"own-1\"")]).with(url, direct_file()));
        let mut state = GameTextsState::with_parts(&game.paths, net, Arc::new(game_texts::NoArchive));
        let page = Page::new();
        // The own address stands in „Mehr …“.
        assert!(page.own_address(&Which::Translation, &mut state, 0));
        state.custom_of("LIVEtrue", &game.paths, None).url = url.into();
        assert!(page.click(&Which::Translation, &mut state, Role::Button, &t("s_tq_uebernehmen")));
        assert!(matches!(&state.question, Some(Question::Source { source, .. }) if source == CUSTOM));
        assert!(page.click(&Which::Translation, &mut state, Role::Button, &t("e_ja")));
        settle(&mut state);
        // Placed, and as the main installation it carries the notes at once.
        let english = fs::read_to_string(format!("{}/data/Localization/english/global.ini", game.live)).unwrap();
        assert!(english.starts_with("Bounty_title_001=Kopfgeld PTU <EM4>[BP]</EM4>\nvehicle_"), "{}", &english[..80]);
        assert!(english.contains("\nk5000=v\n"));
        assert_eq!(translation::installed(&game.paths, CUSTOM, None).as_deref(), Some("etag:own-1"));
        assert!(fs::read_to_string(format!("{}/user.cfg", game.live)).unwrap().contains("g_language = english"));
    }

    #[test]
    fn a_wrong_own_address_is_refused() {
        let game = Game::new("custom");
        let mut state =
            GameTextsState::with_parts(&game.paths, Arc::new(Fake::default()), Arc::new(game_texts::NoArchive));
        let page = Page::new();
        assert!(page.own_address(&Which::Translation, &mut state, 0));
        assert!(page.click(&Which::Translation, &mut state, Role::Button, &t("s_tq_uebernehmen")));
        assert_eq!(state.said().cloned(), Some((t("s_tq_url_falsch"), true)));
    }

    // The block Anti-AFK on the page Spiel.

    /// The block with an overlay that answers (or not) and records what it was asked.
    fn afk_state(
        game: &Game,
        answer: anti_afk::Asked,
        runs: bool,
        supported: bool,
    ) -> (GameTextsState, Arc<Mutex<Vec<bool>>>) {
        let asked = Arc::new(Mutex::new(Vec::new()));
        let record = asked.clone();
        let ask: anti_afk::Ask = Arc::new(move |on| {
            record.lock().unwrap().push(on);
            answer
        });
        let mut state = cfg_state(&game.paths);
        state.anti_afk = anti_afk::AfkBlock::with_parts(ask, Arc::new(move || runs), supported);
        (state, asked)
    }

    fn texts_of(nodes: &[(Role, String, egui::Pos2)]) -> Vec<&str> {
        nodes.iter().map(|(_, label, _)| label.as_str()).collect()
    }

    #[test]
    fn the_anti_afk_block_has_its_switch_off_at_first_the_explanation_and_the_note_of_risk() {
        let game = Game::new("afk-zeigt");
        let (mut state, asked) = afk_state(&game, anti_afk::Asked::Sent, true, true);
        let page = Page::new();
        let (nodes, _) = page.frame(&Which::Game, &mut state, Vec::new());
        let texts = texts_of(&nodes);
        // The help of the switch stands on hover behind the symbol `info`, not as a line; the note of risk stays.
        assert!(!texts.iter().any(|text| crate::widgets::shown_as_hint(text, &t("aafk_h"))), "aafk_h: {texts:?}");
        for key in ["aafk_hinweis", "aafk_hinweis_h"] {
            assert!(texts.iter().any(|text| crate::widgets::shown_as_hint(text, &t(key))), "{key}: {texts:?}");
        }
        assert!(has(&nodes, Role::CheckBox, &t("aafk_titel")), "the switch: {nodes:?}");
        assert!(!texts.contains(&t("aafk_nur_windows").as_str()), "where it can be done there is no such sentence");
        assert!(!state.anti_afk.shown_on(0.0), "off after every start");
        assert!(asked.lock().unwrap().is_empty(), "nobody was asked");
    }

    /// The wait stands under the switch with what the setting holds; without the setting five minutes.
    #[test]
    fn the_anti_afk_wait_shows_what_the_player_set() {
        let game = Game::new("afk-wartezeit");
        let (mut state, _) = afk_state(&game, anti_afk::Asked::Sent, true, true);
        let page = Page::new();
        let (nodes, _) = page.frame(&Which::Game, &mut state, Vec::new());
        let texts = texts_of(&nodes);
        // The name in its row, the hint on hover.
        assert!(
            texts.contains(&t("aafk_warten").as_str()) && !texts.contains(&t("aafk_warten_h").as_str()),
            "{texts:?}"
        );
        assert_eq!(state.anti_afk.seconds, 300);
        assert!(game.paths.set_setting(versekit_core::anti_afk::SETTING_INTERVAL, Value::int(20)));
        state.anti_afk.looked_at = None;
        page.frame(&Which::Game, &mut state, Vec::new());
        assert_eq!(state.anti_afk.seconds, 20, "the page shows the setting");
    }

    /// The page shows the key a press sends: the binding of "wipe visor" in the game, the factory setting
    /// without one, Alt + X with the reason where it cannot be read.
    #[test]
    fn the_anti_afk_block_shows_the_key_from_the_binding_of_the_game() {
        let game = Game::new("afk-taste");
        let (mut state, _) = afk_state(&game, anti_afk::Asked::Sent, true, true);
        let page = Page::new();
        let (nodes, _) = page.frame(&Which::Game, &mut state, Vec::new());
        let texts = texts_of(&nodes);
        assert!(texts.contains(&t("aafk_taste").as_str()), "{texts:?}");
        // Where the key comes from stands on hover over its row.
        use versekit_core::anti_afk_key::Origin;
        assert_eq!(state.anti_afk.chord.origin, Origin::Default, "no file: the factory setting {texts:?}");
        assert!(texts.contains(&"Alt links + X"), "{texts:?}");
        let profile = format!("{}/USER/Client/0/Profiles/default", game.live);
        fs::create_dir_all(&profile).unwrap();
        let maps = r#"<ActionMaps><ActionProfiles><actionmap name="vehicle_general"><action name="visor_wipe"><rebind input="kb1_lalt+k"/></action></actionmap></ActionProfiles></ActionMaps>"#;
        fs::write(format!("{profile}/actionmaps.xml"), maps).unwrap();
        state.anti_afk.looked_at = None;
        let (nodes, _) = page.frame(&Which::Game, &mut state, Vec::new());
        let texts = texts_of(&nodes);
        assert!(texts.contains(&"Alt links + K") && state.anti_afk.chord.origin == Origin::Own, "{texts:?}");
        fs::write(format!("{profile}/actionmaps.xml"), "<ActionMaps><kaputt").unwrap();
        state.anti_afk.looked_at = None;
        let (nodes, _) = page.frame(&Which::Game, &mut state, Vec::new());
        let texts = texts_of(&nodes);
        assert!(texts.contains(&"Alt links + X") && state.anti_afk.chord.origin == Origin::Fallback, "{texts:?}");
    }

    /// The switch for keys of macro software stands on without the setting, and a click stores the other way.
    #[test]
    fn the_switch_for_keys_of_macro_software_is_on_at_first_and_a_click_stores_it() {
        let game = Game::new("afk-makro");
        let (mut state, asked) = afk_state(&game, anti_afk::Asked::Sent, true, true);
        let page = Page::new();
        page.frame(&Which::Game, &mut state, Vec::new());
        assert!(state.anti_afk.count_injected, "they count without the setting");
        assert!(page.click(&Which::Game, &mut state, Role::CheckBox, &t("aafk_makro")));
        assert!(!versekit_core::anti_afk::injected_count(&game.paths), "stored");
        assert!(!state.anti_afk.count_injected);
        assert!(asked.lock().unwrap().is_empty(), "the overlay reads the setting itself");
        assert!(page.click(&Which::Game, &mut state, Role::CheckBox, &t("aafk_makro")));
        assert!(versekit_core::anti_afk::injected_count(&game.paths));
    }

    #[test]
    fn a_click_asks_the_overlay_says_so_and_shows_the_wish_until_the_file_confirms() {
        let game = Game::new("afk-klick");
        let (mut state, asked) = afk_state(&game, anti_afk::Asked::Sent, true, true);
        let page = Page::new();
        page.frame(&Which::Game, &mut state, Vec::new());
        assert!(page.click(&Which::Game, &mut state, Role::CheckBox, &t("aafk_titel")));
        assert_eq!(*asked.lock().unwrap(), [true]);
        assert_eq!(state.anti_afk.said, Some((tf("aafk_sagen", &[("0", Arg::from(t("e_an")))]), false)));
        assert!(state.anti_afk.shown_on(0.5), "the wish stands at once, before the overlay wrote anything");
        // The overlay switched and wrote it: the switch stands on because of the file.
        versekit_core::anti_afk::Snapshot { on: true, ..Default::default() }.write(&game.paths).unwrap();
        state.anti_afk.clicked = None;
        state.anti_afk.looked_at = None;
        let (nodes, _) = page.frame(&Which::Game, &mut state, Vec::new());
        assert!(has(&nodes, Role::CheckBox, &t("aafk_titel")));
        assert!(state.anti_afk.shown_on(100.0), "{:?}", state.anti_afk.snapshot);
        // A second click is the other wish.
        assert!(page.click(&Which::Game, &mut state, Role::CheckBox, &t("aafk_titel")));
        assert_eq!(*asked.lock().unwrap(), [true, false]);
        assert_eq!(state.anti_afk.said, Some((tf("aafk_sagen", &[("0", Arg::from(t("e_aus")))]), false)));
    }

    #[test]
    fn without_a_reachable_overlay_the_switch_stays_and_the_page_says_where_it_is_switched() {
        let game = Game::new("afk-ohne-overlay");
        let (mut state, asked) = afk_state(&game, anti_afk::Asked::NoOverlay, false, true);
        let page = Page::new();
        page.frame(&Which::Game, &mut state, Vec::new());
        assert!(page.click(&Which::Game, &mut state, Role::CheckBox, &t("aafk_titel")));
        assert_eq!(*asked.lock().unwrap(), [true], "it was tried");
        assert_eq!(state.anti_afk.said, Some((t("aafk_kein_overlay"), true)));
        assert!(!state.anti_afk.shown_on(0.5), "nothing was switched");
    }

    #[test]
    fn an_overlay_with_higher_rights_that_does_not_take_the_signal_is_told_apart_from_no_overlay() {
        let game = Game::new("afk-abgelehnt");
        let (mut state, asked) = afk_state(&game, anti_afk::Asked::Refused, true, true);
        let page = Page::new();
        page.frame(&Which::Game, &mut state, Vec::new());
        assert!(page.click(&Which::Game, &mut state, Role::CheckBox, &t("aafk_titel")));
        assert_eq!(*asked.lock().unwrap(), [true]);
        assert_eq!(state.anti_afk.said, Some((t("aafk_abgelehnt"), true)));
        assert!(!state.anti_afk.shown_on(0.5), "nothing was switched");
    }

    #[test]
    fn a_file_of_an_overlay_that_is_gone_does_not_make_the_switch_look_on() {
        let game = Game::new("afk-alte-datei");
        versekit_core::anti_afk::Snapshot { on: true, sent: 3, ..Default::default() }.write(&game.paths).unwrap();
        let (mut state, _) = afk_state(&game, anti_afk::Asked::Sent, false, true);
        let page = Page::new();
        page.frame(&Which::Game, &mut state, Vec::new());
        assert!(!state.anti_afk.shown_on(0.0), "the overlay does not run: off, whatever the file says");
        let (mut state, _) = afk_state(&game, anti_afk::Asked::Sent, true, true);
        page.frame(&Which::Game, &mut state, Vec::new());
        assert!(state.anti_afk.shown_on(0.0), "the overlay runs and the file says on");
    }

    /// The line under the switch says why it does not press, from what the overlay wrote, and stands on the
    /// page.
    #[test]
    fn the_line_under_the_switch_says_why_it_does_not_press_right_now() {
        versekit_core::language::with_language(versekit_core::language::Lang::De, the_line_says_why);
    }

    fn the_line_says_why() {
        use versekit_core::anti_afk::{Reason, Snapshot};
        use versekit_core::language::{Lang, with_language};
        use versekit_core::stick_input::Source;
        let game = Game::new("afk-grund");
        let (mut state, _) = afk_state(&game, anti_afk::Asked::Sent, true, true);
        let page = Page::new();
        let wall = 1_793_000_000;
        let mut line = |snapshot: Snapshot| {
            snapshot.write(&game.paths).unwrap();
            state.anti_afk.looked_at = None;
            page.frame(&Which::Game, &mut state, Vec::new());
            state.anti_afk.status(100.0, wall)
        };
        let on = Snapshot { on: true, joystick: true, ..Snapshot::default() };
        assert_eq!(line(on.clone()), ("Anti-AFK: an".to_string(), false), "before the first beat");
        let not_in_front = Snapshot { reason: Some(Reason::NotInFront), ..on.clone() };
        assert_eq!(line(not_in_front.clone()).0, "Anti-AFK an · Star Citizen ist nicht im Vordergrund");
        let typed = Snapshot {
            reason: Some(Reason::Waiting),
            input_at: Some(wall - 4),
            input_source: Some(Source::Keyboard),
            ..on.clone()
        };
        assert_eq!(line(typed.clone()).0, "Anti-AFK an · wartet: letzte Eingabe vor 4 s (Tastatur)");
        let flown = Snapshot { input_at: Some(wall - 9), input_source: Some(Source::Stick), ..typed.clone() };
        assert_eq!(line(flown).0, "Anti-AFK an · wartet: letzte Eingabe vor 9 s (Joystick)");
        let still = Snapshot { input_at: Some(wall - 30), input_source: None, ..typed };
        assert_eq!(line(still).0, "Anti-AFK an · wartet: seit 30 s keine Eingabe");
        let pressed = Snapshot { reason: Some(Reason::Pressed), pressed_at: Some(wall - 12), sent: 1, ..on.clone() };
        assert_eq!(line(pressed).0, "Anti-AFK an · gedrückt vor 12 s");
        let blind = line(Snapshot { reason: Some(Reason::Blind), ..on.clone() });
        assert_eq!(blind, ("Anti-AFK an · drückt nicht: Eingaben lassen sich nicht lesen".to_string(), true));
        // The page shows the line.
        not_in_front.write(&game.paths).unwrap();
        state.anti_afk.looked_at = None;
        let (nodes, _) = page.frame(&Which::Game, &mut state, Vec::new());
        let texts = texts_of(&nodes);
        assert!(texts.contains(&"Anti-AFK an · Star Citizen ist nicht im Vordergrund"), "{texts:?}");
        // In English.
        let english = with_language(Lang::En, || state.anti_afk.status(100.0, wall).0);
        assert_eq!(english, "Anti-AFK on · Star Citizen is not in the foreground");
    }

    /// The switch takes the state of what the overlay wrote: off, waiting, a press for a moment, a stop; a click
    /// is believed for a moment, and without an overlay it is off.
    #[test]
    fn the_switch_of_the_page_shows_the_state_of_anti_afk() {
        use versekit_core::anti_afk::{Look, Reason, Snapshot};
        let game = Game::new("afk-zustand");
        let (mut state, _) = afk_state(&game, anti_afk::Asked::Sent, true, true);
        let page = Page::new();
        let wall = 1_793_000_000;
        let mut look = |snapshot: Snapshot, wall: i64| {
            snapshot.write(&game.paths).unwrap();
            state.anti_afk.looked_at = None;
            page.frame(&Which::Game, &mut state, Vec::new());
            state.anti_afk.state(100.0, wall)
        };
        assert_eq!(look(Snapshot::default(), wall), Look::Off);
        let on = Snapshot { on: true, reason: Some(Reason::Waiting), ..Snapshot::default() };
        assert_eq!(look(on.clone(), wall), Look::Waiting);
        let pressed = Snapshot { reason: Some(Reason::Pressed), pressed_at: Some(wall), ..on.clone() };
        assert_eq!(look(pressed.clone(), wall), Look::Pressed, "the second of the press");
        assert_eq!(look(pressed.clone(), wall + 1), Look::Pressed);
        assert_eq!(look(pressed, wall + 2), Look::Waiting, "the yellow is over");
        assert_eq!(look(Snapshot { stop: Some("aafk_err_admin".into()), ..Snapshot::default() }, wall), Look::Stopped);
        assert_eq!(look(Snapshot { reason: Some(Reason::Blind), ..on.clone() }, wall), Look::Blind);
        // The colours: the scheme's, one per state.
        for scheme in versekit_core::theme::schemes() {
            let tint = |look| crate::overlay::Tone::of_anti_afk(look).tint(scheme, scheme.sub);
            assert_eq!(
                [Look::Off, Look::Waiting, Look::Pressed, Look::Stopped, Look::Blind].map(tint),
                [scheme.sub, scheme.accent, scheme.yellow, scheme.red, scheme.red],
                "{}",
                scheme.id
            );
        }
        // A click is believed for a moment; without an overlay the switch is off whatever the file says.
        let (mut gone, _) = afk_state(&game, anti_afk::Asked::NoOverlay, false, true);
        on.write(&game.paths).unwrap();
        page.frame(&Which::Game, &mut gone, Vec::new());
        assert_eq!(gone.anti_afk.state(100.0, wall), Look::Off);
        let (mut clicked, _) = afk_state(&game, anti_afk::Asked::Sent, true, true);
        Snapshot::default().write(&game.paths).unwrap();
        page.frame(&Which::Game, &mut clicked, Vec::new());
        assert!(page.click(&Which::Game, &mut clicked, Role::CheckBox, &t("aafk_titel")));
        assert_eq!(clicked.anti_afk.state(0.5, wall), Look::Waiting);
        assert_eq!(clicked.anti_afk.state(10.0, wall), Look::Off, "the file says off");
    }

    /// The switch and the line under it never contradict each other: what a click said stands for a moment,
    /// then the file speaks, also when the overlay switched off by itself.
    #[test]
    fn a_click_that_said_on_does_not_stay_next_to_a_switch_that_is_off() {
        use versekit_core::anti_afk::Snapshot;
        let game = Game::new("afk-widerspruch");
        let (mut state, _) = afk_state(&game, anti_afk::Asked::Sent, true, true);
        let page = Page::new();
        page.frame(&Which::Game, &mut state, Vec::new());
        assert!(page.click(&Which::Game, &mut state, Role::CheckBox, &t("aafk_titel")));
        let said_on = tf("aafk_sagen", &[("0", Arg::from(t("e_an")))]);
        assert_eq!(state.anti_afk.status(0.5, 0), (said_on.clone(), false), "the click is believed for a moment");
        // The overlay never wrote "on" (or switched off since): the switch is off, and so is the line.
        assert!(!state.anti_afk.shown_on(10.0));
        let said_off = tf("aafk_sagen", &[("0", Arg::from(t("e_aus")))]);
        assert_eq!(state.anti_afk.status(10.0, 0), (said_off, false));
        // A stop of the overlay: the line says why, in red.
        Snapshot {
            failure: Some(("aafk_err_admin".into(), "2026-10-24 00:19".into())),
            stop: Some("aafk_err_admin".into()),
            ..Snapshot::default()
        }
        .write(&game.paths)
        .unwrap();
        state.anti_afk.looked_at = None;
        page.frame(&Which::Game, &mut state, Vec::new());
        assert_eq!(state.anti_afk.status(10.0, 0), (t("aafk_err_admin"), true));
        // A stop of an earlier run that the switching on forgot: off, not red.
        Snapshot { failure: Some(("aafk_err_admin".into(), "2026-10-24 00:19".into())), ..Snapshot::default() }
            .write(&game.paths)
            .unwrap();
        state.anti_afk.looked_at = None;
        page.frame(&Which::Game, &mut state, Vec::new());
        assert!(!state.anti_afk.status(10.0, 0).1);
        // No overlay: the line says so, whatever the file says.
        let (mut gone, _) = afk_state(&game, anti_afk::Asked::NoOverlay, false, true);
        Snapshot { on: true, ..Snapshot::default() }.write(&game.paths).unwrap();
        page.frame(&Which::Game, &mut gone, Vec::new());
        assert_eq!(gone.anti_afk.status(10.0, 0), (t("aafk_kein_overlay"), false));
    }

    #[test]
    fn where_the_system_cannot_do_it_there_is_no_switch_but_the_sentence_and_the_note_of_risk() {
        let game = Game::new("afk-linux");
        let (mut state, asked) = afk_state(&game, anti_afk::Asked::Sent, true, false);
        let page = Page::new();
        let (nodes, _) = page.frame(&Which::Game, &mut state, Vec::new());
        let texts = texts_of(&nodes);
        assert!(texts.contains(&t("aafk_nur_windows").as_str()) && texts.contains(&t("aafk_h").as_str()), "{texts:?}");
        assert!(texts.contains(&t("aafk_hinweis").as_str()), "the note of risk stays: {texts:?}");
        assert!(!has(&nodes, Role::CheckBox, &t("aafk_titel")));
        assert!(asked.lock().unwrap().is_empty());
    }

    // The block user.cfg at the foot of the page Spiel.

    const CFG: &str = "g_language = german_(germany)\n\nsys_MaxFPS = 60\n; mine\ng_languageAudio = english\n";

    /// A made-up game with a user.cfg and a Game.log that was last written two hours ago: the game is quiet (a
    /// fresh log counts as a game that runs where the processes cannot decide it).
    fn quiet_game_with_cfg(name: &str, text: &str) -> Game {
        let mut game = Game::new(name);
        // The processes of the machine are not asked: a Star Citizen that runs while the tests do must not turn
        // them red (the log decides, and it is old).
        let data = format!("{}/data", game.root);
        let vars = [
            ("HOME", game.root.as_str()),
            ("SC_BP_HOME", data.as_str()),
            ("SC_INSTALL_DIR", game.live.as_str()),
            (versekit_core::game_start::ENV_NO_PROCESS, "1"),
        ];
        game.paths = Paths::new(Host::new(Flavor::Posix, &vars, &game.root));
        fs::write(format!("{}/user.cfg", game.live), text).unwrap();
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(7200);
        fs::File::options()
            .write(true)
            .open(format!("{}/Game.log", game.live))
            .and_then(|f| f.set_modified(old))
            .unwrap();
        game
    }

    fn read_cfg(game: &Game) -> String {
        fs::read_to_string(format!("{}/user.cfg", game.live)).unwrap().replace("\r\n", "\n")
    }

    fn cfg_state(paths: &Paths) -> GameTextsState {
        GameTextsState::with_parts(paths, Arc::new(Fake::default()), Arc::new(game_texts::NoArchive))
    }

    /// The paths of the same user while the game runs: the log is fresh and nobody can list the processes.
    fn paths_while_the_game_runs(game: &Game) -> Paths {
        let data = format!("{}/data", game.root);
        let vars = [
            ("HOME", game.root.as_str()),
            ("SC_BP_HOME", data.as_str()),
            ("SC_INSTALL_DIR", game.live.as_str()),
            (versekit_core::game_start::ENV_NO_PROCESS, "1"),
        ];
        fs::write(format!("{}/Game.log", game.live), "x").unwrap();
        Paths::new(Host::new(Flavor::Posix, &vars, &game.root))
    }

    fn has(nodes: &[(Role, String, egui::Pos2)], role: Role, name: &str) -> bool {
        nodes.iter().any(|(r, l, _)| *r == role && l == name)
    }

    #[test]
    fn the_user_cfg_block_shows_the_own_lines_without_the_language_lines_and_the_whole_file_below() {
        let game = quiet_game_with_cfg("cfg-zeigt", CFG);
        let mut state = cfg_state(&game.paths);
        let page = Page::new();
        let (nodes, _) = page.frame(&Which::Game, &mut state, Vec::new());
        for key in ["s_uc_titel", "s_uc_eigene", "s_uc_ganz"] {
            assert!(nodes.iter().any(|(_, label, _)| *label == t(key)), "{key}: {nodes:?}");
        }
        assert!(has(&nodes, Role::Button, &t("s_uc_speichern")) && has(&nodes, Role::Button, &t("s_uc_neu")));
        assert_eq!(state.user_cfg.own, "sys_MaxFPS = 60\n; mine", "no language lines, no empty line");
        assert_eq!(state.user_cfg.whole.as_deref(), Some(CFG), "the whole file as it is");
        // Every channel of the one list stands there; one not installed does nothing.
        for channel in versekit_core::paths::CHANNELS {
            assert!(has(&nodes, Role::Button, channel), "{channel}: {nodes:?}");
        }
        let live = state.user_cfg.folder.clone();
        page.click(&Which::Game, &mut state, Role::Button, "PTU");
        assert_eq!(state.user_cfg.folder, live, "PTU is not installed: nothing to choose");
        // The path of the file stands in the line above the box.
        let path = format!("{}/user.cfg", game.live).replace('\\', "/");
        assert!(nodes.iter().any(|(_, label, _)| label.replace('\\', "/").contains(&path)), "{path}: {nodes:?}");
    }

    #[test]
    fn what_is_typed_into_the_box_stands_in_the_state_and_saving_writes_it_with_the_language_lines_behind() {
        let game = quiet_game_with_cfg("cfg-speichert", CFG);
        let mut state = cfg_state(&game.paths);
        let page = Page::new();
        let (nodes, _) = page.frame(&Which::Game, &mut state, Vec::new());
        // Click into the first box (the one with the own lines) and type.
        let mut boxes: Vec<egui::Pos2> =
            nodes.iter().filter(|(r, ..)| *r == Role::MultilineTextInput).map(|(.., at)| *at).collect();
        // Side by side on a wide page: the own lines stand at the left.
        boxes.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
        assert_eq!(boxes.len(), 2, "the own lines and the whole file: {nodes:?}");
        let at = boxes[0];
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        };
        page.frame(&Which::Game, &mut state, vec![egui::Event::PointerMoved(at), press(true), press(false)]);
        page.frame(&Which::Game, &mut state, vec![egui::Event::Text("; typed".into())]);
        assert!(state.user_cfg.own.contains("; typed"), "{:?}", state.user_cfg.own);
        // Now a whole text of lines, then save.
        state.user_cfg.own = "r_DisplayInfo = 2\n\nsys_MaxFPS = 30\ng_language = x".into();
        assert!(page.click(&Which::Game, &mut state, Role::Button, &t("s_uc_speichern")));
        assert_eq!(state.user_cfg.said, Some((t("s_uc_gespeichert"), false)));
        assert_eq!(
            read_cfg(&game),
            "r_DisplayInfo = 2\nsys_MaxFPS = 30\ng_language = german_(germany)\ng_languageAudio = english\n",
            "the typed language line and the empty line are gone, the language lines of the file stand behind"
        );
        let before = fs::read_to_string(format!("{}/user.cfg.versekit-vorher", game.live)).unwrap();
        assert_eq!(before.replace("\r\n", "\n"), CFG, "the state before the first change");
        assert_eq!(state.user_cfg.said, Some((t("s_uc_gespeichert"), false)));
        assert_eq!(state.user_cfg.own, "r_DisplayInfo = 2\nsys_MaxFPS = 30", "the box shows the file again");
    }

    #[test]
    fn an_invalid_line_is_named_by_its_number_nothing_is_written_and_the_typed_text_stays() {
        let game = quiet_game_with_cfg("cfg-ungueltig", CFG);
        let mut state = cfg_state(&game.paths);
        let page = Page::new();
        page.frame(&Which::Game, &mut state, Vec::new());
        let typed = "good = 1\nfoo\nbar baz\n// no\nok = 2\na\nb\nc";
        state.user_cfg.own = typed.into();
        assert!(page.click(&Which::Game, &mut state, Role::Button, &t("s_uc_speichern")));
        // Five numbers at most: lines 2, 3, 4, 6, 7 (the eighth would be the sixth invalid one).
        let said = tf("s_uc_fehler", &[("0", Arg::from("2, 3, 4, 6, 7"))]);
        assert_eq!(state.user_cfg.said, Some((said, true)));
        assert_eq!(read_cfg(&game), CFG.replace("\r\n", "\n"), "the file is as it was");
        assert!(!std::path::Path::new(&format!("{}/user.cfg.versekit-vorher", game.live)).exists(), "no copy either");
        assert_eq!(state.user_cfg.own, typed, "what was typed stays");
    }

    #[test]
    fn while_the_game_runs_nothing_is_written_the_reason_is_said_and_the_typed_text_stays() {
        let game = quiet_game_with_cfg("cfg-laeuft", CFG);
        let running = paths_while_the_game_runs(&game);
        let mut state = cfg_state(&running);
        let page = Page::new();
        page.frame(&Which::Game, &mut state, Vec::new());
        state.user_cfg.own = "a = 1".into();
        assert!(page.click(&Which::Game, &mut state, Role::Button, &t("s_uc_speichern")));
        assert_eq!(state.user_cfg.said, Some((t("s_gf_spiel"), true)));
        assert_eq!(read_cfg(&game), CFG.replace("\r\n", "\n"));
        assert!(!std::path::Path::new(&format!("{}/user.cfg.versekit-vorher", game.live)).exists());
        assert_eq!(state.user_cfg.own, "a = 1");
    }

    #[test]
    fn reloading_throws_away_what_was_typed_and_a_missing_file_says_so() {
        let game = quiet_game_with_cfg("cfg-neu", CFG);
        let mut state = cfg_state(&game.paths);
        let page = Page::new();
        page.frame(&Which::Game, &mut state, Vec::new());
        state.user_cfg.own = "typed = 1".into();
        state.user_cfg.said = Some(("old".into(), true));
        assert!(page.click(&Which::Game, &mut state, Role::Button, &t("s_uc_neu")));
        assert_eq!(state.user_cfg.own, "sys_MaxFPS = 60\n; mine");
        assert_eq!(state.user_cfg.said, None);
        // The file goes: the box is empty and the lower box says there is none.
        fs::remove_file(format!("{}/user.cfg", game.live)).unwrap();
        assert!(page.click(&Which::Game, &mut state, Role::Button, &t("s_uc_neu")));
        assert_eq!(state.user_cfg.own, "");
        assert_eq!(state.user_cfg.whole, None);
        let (nodes, _) = page.frame(&Which::Game, &mut state, Vec::new());
        assert!(nodes.iter().any(|(_, label, _)| *label == t("s_uc_keine_datei")), "{nodes:?}");
        // Saving makes the file; there is nothing to keep a copy of.
        state.user_cfg.own = "new = 1".into();
        assert!(page.click(&Which::Game, &mut state, Role::Button, &t("s_uc_speichern")));
        assert_eq!(state.user_cfg.said, Some((t("s_uc_gespeichert"), false)));
        assert_eq!(read_cfg(&game), "new = 1\n");
        assert!(!std::path::Path::new(&format!("{}/user.cfg.versekit-vorher", game.live)).exists());
    }

    #[test]
    fn an_untouched_box_follows_a_change_of_the_file_and_an_edited_one_refuses_to_write_over_it() {
        let game = quiet_game_with_cfg("cfg-aussen", CFG);
        let mut state = cfg_state(&game.paths);
        let page = Page::new();
        page.frame(&Which::Game, &mut state, Vec::new());
        assert_eq!(state.user_cfg.own, "sys_MaxFPS = 60\n; mine");
        // The page Grafik (or the player's editor) changes the file: the untouched box follows.
        let outside = "g_language = german_(germany)\nsys_MaxFPS = 90\n";
        fs::write(format!("{}/user.cfg", game.live), outside).unwrap();
        state.user_cfg.checked_at = None;
        page.frame(&Which::Game, &mut state, Vec::new());
        assert_eq!(state.user_cfg.own, "sys_MaxFPS = 90", "the box shows the file as it is now");
        assert_eq!(state.user_cfg.said, None);
        // The box has typing in it, then the file changes again: the typing stays, the file is not written over.
        state.user_cfg.own = "mine = 1".into();
        let again = "g_language = german_(germany)\nsys_MaxFPS = 120\n";
        fs::write(format!("{}/user.cfg", game.live), again).unwrap();
        state.user_cfg.checked_at = None;
        page.frame(&Which::Game, &mut state, Vec::new());
        assert_eq!(state.user_cfg.own, "mine = 1", "what was typed stays");
        assert_eq!(state.user_cfg.said, Some((t("s_uc_geaendert"), true)));
        state.user_cfg.said = None;
        assert!(page.click(&Which::Game, &mut state, Role::Button, &t("s_uc_speichern")));
        assert_eq!(state.user_cfg.said, Some((t("s_uc_geaendert"), true)), "refused");
        assert_eq!(read_cfg(&game), again, "the file is the one of the other writer");
        assert!(!std::path::Path::new(&format!("{}/user.cfg.versekit-vorher", game.live)).exists());
        // After a reload the box is the file again and saving works.
        assert!(page.click(&Which::Game, &mut state, Role::Button, &t("s_uc_neu")));
        assert_eq!(state.user_cfg.own, "sys_MaxFPS = 120");
        state.user_cfg.own = "sys_MaxFPS = 120\nmine = 1".into();
        assert!(page.click(&Which::Game, &mut state, Role::Button, &t("s_uc_speichern")));
        assert_eq!(state.user_cfg.said, Some((t("s_uc_gespeichert"), false)));
    }

    #[test]
    fn a_file_that_cannot_be_read_is_not_taken_for_no_file_and_nothing_is_saved_over_it() {
        let game = quiet_game_with_cfg("cfg-gesperrt", CFG);
        // A folder where the file should be: reading it fails, but the file is "there".
        fs::remove_file(format!("{}/user.cfg", game.live)).unwrap();
        fs::create_dir(format!("{}/user.cfg", game.live)).unwrap();
        let mut state = cfg_state(&game.paths);
        let page = Page::new();
        let (nodes, _) = page.frame(&Which::Game, &mut state, Vec::new());
        assert!(state.user_cfg.unreadable.is_some(), "told apart from no file");
        assert!(
            !nodes.iter().any(|(_, label, _)| *label == t("s_uc_keine_datei")),
            "the lower box does not claim there is no file: {nodes:?}"
        );
        state.user_cfg.own = "typed = 1".into();
        assert!(page.click(&Which::Game, &mut state, Role::Button, &t("s_uc_speichern")));
        let said = state.user_cfg.said.clone().expect("said");
        assert!(said.1 && said.0.contains(t("s_uc_nicht_lesbar").split('{').next().unwrap_or("")), "{said:?}");
        assert!(std::path::Path::new(&format!("{}/user.cfg", game.live)).is_dir(), "nothing was written");
        assert_eq!(state.user_cfg.own, "typed = 1");
    }

    #[test]
    fn with_two_channels_one_is_chosen_at_a_time_and_a_folder_without_the_archive_is_no_channel() {
        let game = quiet_game_with_cfg("cfg-kanaele", CFG);
        let ptu = format!("{}/StarCitizen/PTU", game.root);
        fs::create_dir_all(&ptu).unwrap();
        fs::write(format!("{ptu}/Data.p4k"), "").unwrap();
        fs::write(format!("{ptu}/user.cfg"), "ptu_only = 1\n").unwrap();
        let eptu = format!("{}/StarCitizen/EPTU", game.root);
        fs::create_dir_all(&eptu).unwrap();
        let mut state = cfg_state(&game.paths);
        let page = Page::new();
        let (nodes, _) = page.frame(&Which::Game, &mut state, Vec::new());
        assert!(has(&nodes, Role::Button, "LIVE") && has(&nodes, Role::Button, "PTU"), "{nodes:?}");
        assert!(nodes.iter().any(|(_, label, _)| *label == t("s_uc_kanal")));
        assert_eq!(state.user_cfg.own, "sys_MaxFPS = 60\n; mine", "the first channel at first");
        // EPTU stands grey: no Data.p4k, no channel to choose.
        page.click(&Which::Game, &mut state, Role::Button, "EPTU");
        assert!(!state.user_cfg.folder.ends_with("EPTU"), "no Data.p4k, no channel");
        assert!(page.click(&Which::Game, &mut state, Role::Button, "PTU"));
        assert!(state.user_cfg.folder.ends_with("PTU"), "{}", state.user_cfg.folder);
        assert_eq!(state.user_cfg.own, "ptu_only = 1");
        // Saving goes to the channel shown and to that one only.
        state.user_cfg.own = "ptu_only = 2".into();
        assert!(page.click(&Which::Game, &mut state, Role::Button, &t("s_uc_speichern")));
        assert_eq!(state.user_cfg.said, Some((t("s_uc_gespeichert"), false)));
        assert_eq!(fs::read_to_string(format!("{ptu}/user.cfg")).unwrap().replace("\r\n", "\n"), "ptu_only = 2\n");
        assert_eq!(read_cfg(&game), CFG.replace("\r\n", "\n"), "LIVE is untouched");
        assert!(page.click(&Which::Game, &mut state, Role::Button, "LIVE"));
        assert_eq!(state.user_cfg.own, "sys_MaxFPS = 60\n; mine");
    }

    #[test]
    fn without_any_installation_the_block_says_where_to_set_the_folder() {
        let game = Game::new("cfg-leer");
        fs::remove_file(format!("{}/Data.p4k", game.live)).unwrap();
        let mut state = cfg_state(&game.paths);
        let page = Page::new();
        let (nodes, _) = page.frame(&Which::Game, &mut state, Vec::new());
        assert!(nodes.iter().any(|(_, label, _)| *label == t("s_uc_titel")), "{nodes:?}");
        assert!(nodes.iter().any(|(_, label, _)| *label == t("s_tq_kein_spiel")), "{nodes:?}");
        assert!(!has(&nodes, Role::Button, &t("s_uc_speichern")));
    }

    // The two texts of the block user.cfg side by side or under each other.

    /// A user.cfg with `n` lines of the player's own behind the language lines.
    fn long_cfg(n: usize) -> String {
        let mut text = String::from("g_language = german_(germany)\n\n");
        for i in 0..n {
            text.push_str(&format!("sys_Option{i} = {i}\n"));
        }
        text
    }

    /// Below this width of the page the two texts stand under each other, from this one on side by side
    /// (the threshold of the block lies between).
    const NARROW: f32 = 660.0;
    const WIDE: f32 = 760.0;

    /// What the block showed after a few frames at `width`: the nodes, the rectangles of both boxes, and where
    /// the block begins and ends.
    struct Seen {
        nodes: Vec<(Role, String, egui::Rect)>,
        own: egui::Rect,
        whole: egui::Rect,
        top: f32,
        bottom: f32,
    }

    fn seen(paths: &Paths, width: f32) -> Seen {
        let mut state = cfg_state(paths);
        let page = Page::new();
        let mut nodes = Vec::new();
        // The second frame knows how tall the heads of both columns are.
        for _ in 0..4 {
            nodes = page.frame_sized(&Which::Game, &mut state, width, Vec::new()).0;
        }
        let [Some(own), Some(whole)] = state.user_cfg.drawn else { panic!("both boxes were drawn: {nodes:?}") };
        let top = nodes
            .iter()
            .find(|(_, label, _)| *label == t("s_uc_titel"))
            .map(|(.., rect)| rect.top())
            .expect("the title of the block");
        let below = nodes.iter().filter(|(role, _, rect)| *role != Role::MultilineTextInput && rect.top() >= top);
        let bottom = below.map(|(.., rect)| rect.bottom()).fold(own.bottom().max(whole.bottom()), f32::max);
        Seen { nodes, own, whole, top, bottom }
    }

    /// The text of the made-up game does not reach beyond the page on either side.
    fn nothing_cut_off(seen: &Seen, width: f32, what: &str) {
        for (_, label, rect) in &seen.nodes {
            assert!(rect.left() >= -0.5 && rect.right() <= width + 0.5, "{what}: {label:?} {rect:?} of {width}");
        }
        for rect in [seen.own, seen.whole] {
            assert!(rect.left() >= -0.5 && rect.right() <= width + 0.5, "{what}: a box {rect:?} of {width}");
        }
    }

    #[test]
    fn the_two_user_cfg_texts_stand_side_by_side_on_a_wide_page_and_under_each_other_on_a_narrow_one() {
        let game = quiet_game_with_cfg("cfg-spalten", &long_cfg(60));
        // A second channel: its buttons stand above both texts.
        let ptu = format!("{}/StarCitizen/PTU", game.root);
        fs::create_dir_all(&ptu).unwrap();
        fs::write(format!("{ptu}/Data.p4k"), "").unwrap();
        for choice in [versekit_core::language::Choice::De, versekit_core::language::Choice::En] {
            versekit_core::language::set_language(choice);
            for width in [1500.0, 1000.0, WIDE] {
                let what = format!("{choice:?} {width}");
                let s = seen(&game.paths, width);
                nothing_cut_off(&s, width, &what);
                assert!(s.own.right() < s.whole.left(), "{what}: the own lines at the left, the file at the right");
                assert!((s.own.top() - s.whole.top()).abs() < 1.0, "{what}: both begin on one line {:?}", s.own);
                assert!((s.own.width() - s.whole.width()).abs() < 1.0, "{what}: equally wide");
                assert!((s.own.height() - s.whole.height()).abs() < 1.0, "{what}: equally tall");
            }
            for width in [NARROW, 520.0, 420.0] {
                let what = format!("{choice:?} {width}");
                let s = seen(&game.paths, width);
                nothing_cut_off(&s, width, &what);
                assert!((s.own.left() - s.whole.left()).abs() < 1.0, "{what}: one under the other");
                assert!(s.whole.top() > s.own.bottom(), "{what}: the file below the own lines");
                assert!((s.own.width() - s.whole.width()).abs() < 1.0, "{what}: equally wide");
            }
            // The channel buttons stand above both texts, wide and narrow alike.
            for width in [WIDE, NARROW] {
                let s = seen(&game.paths, width);
                for channel in ["LIVE", "PTU"] {
                    let (_, _, button) = s
                        .nodes
                        .iter()
                        .find(|(role, label, _)| *role == Role::Button && label == channel)
                        .unwrap_or_else(|| panic!("{channel} at {width}"));
                    assert!(button.bottom() <= s.own.top() && button.bottom() <= s.whole.top(), "{channel} at {width}");
                }
            }
        }
        versekit_core::language::set_language(versekit_core::language::Choice::De);
    }

    #[test]
    fn side_by_side_both_texts_are_taller_than_under_each_other_and_the_block_is_not_longer() {
        let game = quiet_game_with_cfg("cfg-hoehe", &long_cfg(60));
        let stacked = seen(&game.paths, NARROW);
        let side = seen(&game.paths, WIDE);
        for (name, now, before) in [("own", side.own, stacked.own), ("whole", side.whole, stacked.whole)] {
            assert!(now.height() > before.height() * 1.3, "{name}: {} against {}", now.height(), before.height());
        }
        // The page does not need more scrolling than before: the block is not longer than the two texts under each other.
        let (long, short) = (stacked.bottom - stacked.top, side.bottom - side.top);
        assert!(short <= long, "side by side {short}, under each other {long}");
        // A short file: the texts are as tall as ever or taller (a box under the other is 18 lines high however
        // short its text), and equally tall.
        // (the same game with a short file: a second game would wait for the language lock this one holds)
        fs::write(format!("{}/user.cfg", game.live), CFG).unwrap();
        let (stacked, side) = (seen(&game.paths, NARROW), seen(&game.paths, WIDE));
        assert!(side.own.height() > stacked.own.height(), "{} against {}", side.own.height(), stacked.own.height());
        assert!((side.own.height() - side.whole.height()).abs() < 1.0);
    }
}
