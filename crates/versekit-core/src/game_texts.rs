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

//! The game's texts as a whole: what the pages and the background do with
//! the translation and the blueprint notes, port of the parts of
//! `settings_window.py`, `pages.py` and the main module of 4.12 that drive
//! `translation` and `injection`.
//!
//! Every translation update and every game patch rewrites the `global.ini`,
//! and the notes are silently gone. So [`Upkeep`] looks on start, after
//! every game session and every five minutes, writes the boxes anew when the collection changes (every 30
//! seconds) and puts the notes back at once when another tool rewrites the
//! file (every 5 seconds). Only what the player set up himself is touched.
//!
//! Reading the `Data.p4k` (the original English texts, the names, the gaps
//! of an old translation) belongs to `versekit-gamedata`; it comes in
//! through [`Archive`].

use std::collections::BTreeSet;

use crate::errors;
use crate::game_ini;
use crate::injection::{self, Inputs, Outcome, item_info, sources};
use crate::jsonio::{self, Value};
use crate::net::Net;
use crate::paths::Paths;
use crate::translation::{self, CUSTOM, ORIGINAL, SOURCES};

/// Writing notes at all (`inj_an`).
pub const SETTING_ON: &str = "inj_an";
/// Keeping them current by itself (`inj_auto`).
pub const SETTING_AUTO: &str = "inj_auto";
/// The fingerprint of the last writing (`inj_bestand`).
pub const SETTING_STOCK_MARK: &str = "inj_bestand";
/// The catalog stand of the last writing (`inj_katalog`).
pub const SETTING_CATALOG: &str = "inj_katalog";
/// Sources the player confirmed once (`inj_bestaetigt`, comma separated).
pub const SETTING_CONFIRMED: &str = "inj_bestaetigt";
/// The `Data.p4k` stamp at the last patch seen (`p4k_stand`).
pub const P4K_SETTING: &str = "p4k_stand";

/// How often the translation is looked at for a newer version: every five minutes. A look at a release on
/// GitHub carries the `ETag` of the last one and costs nothing against GitHub's hourly limit while the release
/// is unchanged (`translation::latest`).
pub const TEXTS_POLL_SECONDS: f64 = 300.0;
pub const STOCK_POLL_SECONDS: f64 = 30.0;
pub const FILE_POLL_SECONDS: f64 = 5.0;
pub const PATCH_QUIET_SECONDS: f64 = 30.0;
/// How often the translation is looked at in the window after a patch: every five minutes as well.
pub const POST_PATCH_POLL_SECONDS: f64 = 300.0;
pub const POST_PATCH_WINDOW_SECONDS: f64 = 12.0 * 3600.0;

/// What only the game data can do.
pub trait Archive: Send + Sync {
    /// Size and time of the `Data.p4k` of the main installation as text.
    fn stamp(&self, paths: &Paths) -> Option<String>;
    /// Renew the placed original texts after a patch; whether it wrote.
    fn refresh_original(&self, paths: &Paths) -> bool;
    /// Fill what the translation lacks with English; the lines added.
    fn fill_gaps(&self, paths: &Paths, target: &str) -> usize;
    /// Place the English texts of the archive into a channel folder
    /// (`gametext.fetch`); what it said and whether it wrote (a foreign file
    /// is kept), or the problem.
    fn fetch_original(&self, paths: &Paths, root: &str, set_language: bool) -> Result<Placed, String>;
    /// The stored original names, renewed from the archive when needed.
    fn refresh_names(&self, paths: &Paths);
    /// The stored rank thresholds ([`injection::ranks::refresh`]), read from the archive when they do not fit `game_version`.
    fn refresh_ranks(&self, _paths: &Paths, _game_version: &str) {}
}

/// What placing the original texts did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    pub said: String,
    /// The file was written; `false` when a foreign file was kept.
    pub written: bool,
}

/// No game data at hand: nothing of the archive happens.
pub struct NoArchive;

impl Archive for NoArchive {
    fn stamp(&self, _: &Paths) -> Option<String> {
        None
    }
    fn refresh_original(&self, _: &Paths) -> bool {
        false
    }
    fn fill_gaps(&self, _: &Paths, _: &str) -> usize {
        0
    }
    fn fetch_original(&self, _: &Paths, _: &str, _: bool) -> Result<Placed, String> {
        Err(crate::language::t("m_kein_p4k"))
    }
    fn refresh_names(&self, _: &Paths) {}
}

/// `name (identifier), 1.2 MB` for a fetched source.
fn fetched_text(source: &str, what: &str, bytes: usize) -> String {
    use crate::language::{Arg, tf};
    let megabytes = crate::language::py_fixed(bytes as f64 / 1_048_576.0, 1, false);
    tf(
        "s_tq_geholt",
        &[("0", Arg::from(translation::display_name(source))), ("1", Arg::from(what)), ("2", Arg::from(megabytes))],
    )
}

/// What the background tells the player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    /// `texte_erneuert`: a translation (or the original texts) was renewed.
    TranslationUpdated(String),
    /// `texte_ergaenzt`: lines of an old translation filled in English.
    GapsFilled(usize),
    /// `spielsprache_repariert`: the language line is back in the user.cfg.
    LanguageRepaired(String),
    /// `texte_wieder_eingesetzt`: another program had replaced the text file of a channel (or it was gone); the
    /// chosen one is placed again.
    FileRestored(String),
    /// `inj_aktiv`: the notes are written.
    Applied(usize),
    /// `craftdaten_neu`: the craft data (kind, size, grade, class) were built anew for a new build of the game, with
    /// this many blueprints.
    CraftData { build: String, items: usize },
}

impl Notice {
    /// The status line text.
    pub fn text(&self) -> String {
        use crate::language::{Arg, tf};
        match self {
            Notice::TranslationUpdated(what) => tf("texte_erneuert", &[("0", Arg::from(what.as_str()))]),
            Notice::GapsFilled(n) => tf("texte_ergaenzt", &[("0", Arg::from(*n as i64))]),
            Notice::LanguageRepaired(folder) => tf("spielsprache_repariert", &[("0", Arg::from(folder.as_str()))]),
            Notice::FileRestored(channel) => tf("texte_wieder_eingesetzt", &[("0", Arg::from(channel.as_str()))]),
            Notice::Applied(n) => tf("inj_aktiv", &[("0", Arg::from(*n as i64))]),
            Notice::CraftData { build, items } => {
                tf("craftdaten_neu", &[("0", Arg::from(build.as_str())), ("1", Arg::from(*items as i64))])
            }
        }
    }
}

/// The version the menu line names.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// The catalog file the contracts come from.
pub fn catalog_file(paths: &Paths) -> String {
    crate::catalog_cache::current_file(paths)
}

/// `catalog.load()['geholt']`.
pub fn catalog_stand(paths: &Paths) -> String {
    match jsonio::read_json(&catalog_file(paths)).ok().and_then(|d| d.get("geholt").cloned()) {
        Some(value) if injection::texts::truthy(Some(&value)) => injection::texts::py_str(&value),
        _ => String::new(),
    }
}

/// Everything the writing reads, the rank thresholds read when they do
/// not fit the game version. The reputation and terms of the contracts only for the build they were read from: after a patch, until the
/// catalog is built anew, the notes name none rather than those of the build before.
pub fn prepare(paths: &Paths, archive: &dyn Archive, game_version: &str) -> Inputs {
    archive.refresh_ranks(paths, game_version);
    archive.refresh_names(paths);
    let mut inputs = sources::inputs(paths, &catalog_file(paths), version());
    let table = &inputs.reputation;
    if !game_version.is_empty()
        && !table.version.is_empty()
        && !injection::reputation::same_build(&table.version, game_version)
    {
        inputs.reputation = injection::reputation::Table::default();
    }
    inputs
}

/// `injection.ini_file` as the pages show it.
pub fn choice(paths: &Paths) -> game_ini::IniChoice {
    game_ini::ini_choice(paths)
}

/// `injection.setup`: write the notes into the file that counts, and into the text file of every other channel whose
/// translation VerseKit keeps ([`side_channels`]). How the file of the main installation went decides, the spots of
/// all files count together; without a file there, the other channels decide.
pub fn apply(paths: &Paths, archive: &dyn Archive, game_version: &str) -> Outcome {
    let choice = choice(paths);
    let sides = side_files(paths);
    if choice.path.is_none() && sides.is_empty() {
        return Outcome::NoFile;
    }
    let inputs = prepare(paths, archive, game_version);
    let mut outcome = choice
        .path
        .as_deref()
        .map_or(Outcome::NoFile, |path| injection::apply_texts(paths, path, &choice.language, &inputs, false));
    for (target, language) in &sides {
        outcome = together(outcome, injection::apply_texts(paths, target, language, &inputs, false));
    }
    outcome
}

/// `injection.remove_texts` on the file that counts and on the files of the other channels that hold notes.
pub fn remove(paths: &Paths) -> Outcome {
    let choice = choice(paths);
    let mut outcome =
        choice.path.as_deref().map_or(Outcome::NoFile, |path| injection::remove_texts(paths, path, &choice.language));
    for (target, language) in side_files(paths) {
        if injection::is_applied(&target) {
            outcome = together(outcome, injection::remove_texts(paths, &target, &language));
        }
    }
    // A channel left alone is no longer tended, but notes may still stand in its files: they go as well.
    for root in game_roots(paths) {
        for (target, language) in files_with_notes(paths, &root) {
            outcome = together(outcome, injection::remove_texts(paths, &target, &language));
        }
    }
    outcome
}

/// The main installation and every installed channel, each folder once.
fn game_roots(paths: &Paths) -> Vec<String> {
    let flavor = paths.host.flavor;
    let key = |p: &str| flavor.normcase(&flavor.normpath(p));
    let mut roots: Vec<String> = Vec::new();
    for root in paths.game_folder().into_iter().chain(paths.installed_channels().into_iter().map(|(_, folder, _)| folder)) {
        if !roots.iter().any(|seen| key(seen) == key(&root)) {
            roots.push(root);
        }
    }
    roots
}

/// The text files below a game folder that still hold notes of ours, with their language folders. Nothing on a copy
/// of the data.
fn files_with_notes(paths: &Paths, root: &str) -> Vec<(String, String)> {
    if translation::only_looks(paths, root) {
        return Vec::new();
    }
    let flavor = paths.host.flavor;
    let Some(folder) = ["data", "Data"]
        .iter()
        .map(|spelling| flavor.join_all(root, &[spelling, "Localization"]))
        .find(|folder| std::path::Path::new(folder).is_dir())
    else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut languages: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    languages.sort();
    languages
        .into_iter()
        .filter_map(|language| {
            let target = game_ini::target_ini_in(paths, root, &language)?;
            (game_ini::is_file(&target) && injection::is_applied(&target)).then_some((target, language))
        })
        .collect()
}

/// A game folder is left alone from now on ("Nicht anfassen"): first VerseKit's notes leave its text files and the
/// original wording stands again, then the choice is stored as "nothing". `channel` is `None` for the main
/// installation. How many spots went.
pub fn leave_alone(paths: &Paths, channel: Option<&str>, root: &str) -> usize {
    let mut count = 0;
    for (target, language) in files_with_notes(paths, root) {
        if let Outcome::Done(spots) = injection::remove_texts(paths, &target, &language) {
            count += spots;
        }
    }
    match channel {
        Some(channel) => {
            translation::set_channel_source(paths, channel, "");
        }
        None => {
            paths.set_setting(game_ini::SETTING_SOURCE, Value::str(""));
        }
    }
    count
}

/// The outcome of the main file and one of another channel as one: the spots count together, a failure of the main
/// file stays, one of the other channel goes to the error log; without a main file the other one decides.
fn together(main: Outcome, side: Outcome) -> Outcome {
    match (main, side) {
        (Outcome::Done(a), Outcome::Done(b)) => Outcome::Done(a + b),
        (Outcome::NoFile, side) => side,
        (main, Outcome::Done(_) | Outcome::NoFile) => main,
        (main, side) => {
            errors::record("injection.kanal", &format!("{side:?}"));
            main
        }
    }
}

/// A channel whose text file VerseKit keeps: `None` for the main installation, its folder and the source chosen
/// there, which is set up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kept {
    pub channel: Option<String>,
    pub root: String,
    pub source: String,
}

impl Kept {
    /// The language folder of its source.
    pub fn language(&self, paths: &Paths) -> String {
        translation::language_folder(paths, &self.source, self.channel.as_deref())
    }

    /// Where its text file belongs.
    pub fn target(&self, paths: &Paths) -> Option<String> {
        game_ini::target_ini_in(paths, &self.root, &self.language(paths))
    }

    /// What a notice names: the folder for the main installation (as 4.12 says it), `CHANNEL · folder` else.
    fn label(&self, paths: &Paths) -> String {
        match &self.channel {
            None => self.language(paths),
            Some(channel) => format!("{channel} · {}", self.language(paths)),
        }
    }
}

/// The source the main installation keeps: the chosen one when it is set up ([`active_source`]), or the original
/// texts when they are chosen and placed.
fn main_source(paths: &Paths) -> Option<String> {
    active_source(paths).or_else(|| {
        (paths.setting(game_ini::SETTING_SOURCE).as_deref() == Some(ORIGINAL)
            && translation::installed(paths, ORIGINAL, None).is_some())
        .then(|| ORIGINAL.to_string())
    })
}

/// The other channels besides the main installation (`main`) whose translation VerseKit keeps like the one of the
/// main installation: every installed channel with an archive whose chosen source is set up there. A channel left
/// alone is not among them.
pub fn side_channels(paths: &Paths, main: Option<&str>) -> Vec<Kept> {
    let flavor = paths.host.flavor;
    let same = |a: &str, b: &str| flavor.normcase(&flavor.normpath(a)) == flavor.normcase(&flavor.normpath(b));
    let sources = translation::channel_sources(paths);
    paths
        .installed_channels()
        .into_iter()
        .filter(|(name, folder, p4k)| {
            *p4k && translation::CHANNELS.contains(&name.as_str()) && !main.is_some_and(|main| same(main, folder))
        })
        .filter_map(|(name, folder, _)| {
            let source = sources.iter().find(|(k, _)| *k == name).and_then(|(_, v)| v.as_str())?.to_string();
            (!source.is_empty() && translation::installed(paths, &source, Some(&name)).is_some())
                .then_some(Kept { channel: Some(name), root: folder, source })
        })
        .collect()
}

/// Every channel whose translation VerseKit keeps: the main installation first, when its source is set up, then
/// [`side_channels`].
pub fn kept_channels(paths: &Paths) -> Vec<Kept> {
    let main = paths.game_folder();
    let mut kept: Vec<Kept> = main
        .as_ref()
        .and_then(|root| main_source(paths).map(|source| Kept { channel: None, root: root.clone(), source }))
        .into_iter()
        .collect();
    kept.extend(side_channels(paths, main.as_deref()));
    kept
}

/// The text files of the [`side_channels`] that are there, with their language folders.
fn side_files(paths: &Paths) -> Vec<(String, String)> {
    side_channels(paths, paths.game_folder().as_deref())
        .into_iter()
        .filter_map(|kept| kept.target(paths).filter(|t| game_ini::is_file(t)).map(|t| (t, kept.language(paths))))
        .collect()
}

/// Put the language of the chosen translation back into the `user.cfg` of a channel when another program changed
/// it or it is missing; only when its file is there, never on a copy of the data.
fn language_line(paths: &Paths, kept: &Kept) -> Option<Notice> {
    let folder = kept.language(paths);
    let target = kept.target(paths)?;
    if !game_ini::is_file(&target)
        || game_ini::game_language_in(paths, &kept.root).as_deref() == Some(folder.as_str())
        || translation::only_looks(paths, &kept.root)
    {
        return None;
    }
    let audio = translation::source(&kept.source).and_then(|q| q.audio);
    translation::set_user_cfg(paths, &kept.root, &folder, audio).then(|| Notice::LanguageRepaired(kept.label(paths)))
}

/// The language line of every kept channel ([`language_line`]), without the net: at the start and right before
/// the game is started.
pub fn check_game_languages(paths: &Paths) -> Vec<Notice> {
    kept_channels(paths).iter().filter_map(|kept| language_line(paths, kept)).collect()
}

/// Place the text file of a kept channel again when it is gone or another program put its own file there
/// ([`translation::replaced_by_other`]); the file that was there stays as a copy beside it, the language goes into
/// the `user.cfg`. A file placed by an older version, which has no version box noted, is taken as it is and noted.
/// Nothing on a copy of the data. What it told, if it placed the file.
fn put_back(paths: &Paths, net: &dyn Net, archive: &dyn Archive, kept: &Kept) -> Option<Notice> {
    if translation::only_looks(paths, &kept.root) {
        return None;
    }
    let channel = kept.channel.as_deref();
    let target = kept.target(paths)?;
    let key = translation::note_key(&kept.source, channel);
    if game_ini::is_file(&target) && !translation::replaced_by_other(paths, &kept.source, channel, &target) {
        if !translation::head_noted(paths, &kept.source, channel) {
            translation::note_head_of_file(paths, &key, &target);
        }
        return None;
    }
    let placed = if kept.source == ORIGINAL {
        archive.fetch_original(paths, &kept.root, true).map(|placed| placed.written)
    } else {
        translation::fetch(net, paths, &kept.source, &kept.root, channel, &|_| {}).map(|_| true).map_err(|p| p.text())
    };
    match placed {
        Ok(true) => {
            if kept.source == ORIGINAL {
                translation::note_head_of_file(paths, &key, &target);
            }
            Some(Notice::FileRestored(kept.label(paths)))
        }
        Ok(false) => None,
        Err(problem) => {
            errors::record("watcher.textdatei", &problem);
            None
        }
    }
}

/// The state for the pages and the error report (`injection.status`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub file: Option<String>,
    pub applied: bool,
    pub source: Option<String>,
    pub language: String,
}

/// `injection.status`.
pub fn status(paths: &Paths) -> Status {
    let choice = choice(paths);
    let exists = choice.path.as_deref().is_some_and(game_ini::is_file);
    let applied = exists && choice.path.as_deref().is_some_and(injection::is_applied);
    Status { file: choice.path, applied, source: choice.source, language: choice.language }
}

/// Whether the player confirmed a source once (`inj_bestaetigt`).
pub fn confirmed(paths: &Paths, source: &str) -> bool {
    paths.setting(SETTING_CONFIRMED).unwrap_or_default().split(',').any(|s| s == source)
}

/// Remember that the player confirmed a source.
pub fn confirm(paths: &Paths, source: &str) {
    if confirmed(paths, source) {
        return;
    }
    let mut list: Vec<String> = paths
        .setting(SETTING_CONFIRMED)
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    list.push(source.to_string());
    paths.set_setting(SETTING_CONFIRMED, Value::str(list.join(",")));
}

/// What a switch of the source did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Switched {
    /// The message of the fetch.
    pub fetched: String,
    /// The old file that was reset first, with its count.
    pub cleaned: Option<(String, usize)>,
    /// How the writing ended; `None` while writing notes is switched off.
    pub applied: Option<Outcome>,
}

/// `settings_window._inj_switch_now`: the main installation takes another
/// source. Fetch it (or the original texts), reset the old file, write.
///
/// # Errors
///
/// The text of the problem when nothing could be placed.
pub fn switch_source(
    paths: &Paths,
    net: &dyn Net,
    archive: &dyn Archive,
    source: &str,
    game_version: &str,
    progress: &dyn Fn(translation::Progress),
) -> Result<Switched, String> {
    paths.set_setting(game_ini::SETTING_SOURCE, Value::str(source));
    let root = paths.game_folder().ok_or_else(|| translation::Problem::NoGameFolder.text())?;
    if source != ORIGINAL && translation::only_looks(paths, &root) {
        // A copy of the data only looks: nothing is downloaded, nothing goes into the game folder.
        let looked = translation::look_on_copy(net, paths, source, None).map_err(|p| p.text())?;
        return Ok(Switched { fetched: looked.text_in(crate::language::current()), cleaned: None, applied: None });
    }
    let fetched = if source == ORIGINAL {
        let placed = archive.fetch_original(paths, &root, true)?;
        // A foreign file that was kept is no original of ours: the
        // background must not renew it after a patch.
        if placed.written {
            translation::note_source(paths, ORIGINAL, "Data.p4k", None);
            if let Some(target) = game_ini::target_ini_in(paths, &root, "english") {
                translation::note_head_of_file(paths, ORIGINAL, &target);
            }
        }
        placed.said
    } else {
        let placed = translation::fetch(net, paths, source, &root, None, progress).map_err(|p| p.text())?;
        fetched_text(source, &placed.ident, placed.bytes)
    };
    let target = choice(paths).path;
    // The notes of the old file reset it; the new file is a fresh base then.
    let cleaned = injection::clean_leftover(paths, target.as_deref());
    if cleaned.is_some() {
        injection::discard_origtext(paths);
    }
    let applied = paths.setting_bool(SETTING_ON, true).then(|| apply(paths, archive, game_version));
    Ok(Switched { fetched, cleaned, applied })
}

/// A side channel takes another source (`pages._fetch_side`): the text file
/// with the language in its `user.cfg`, and the notes as in the main
/// installation (while they are switched on). An empty source leaves the
/// channel alone.
///
/// # Errors
///
/// The text of the problem.
pub fn switch_side(
    paths: &Paths,
    net: &dyn Net,
    archive: &dyn Archive,
    channel: &str,
    root: &str,
    source: &str,
    progress: &dyn Fn(translation::Progress),
) -> Result<String, String> {
    if source.is_empty() {
        leave_alone(paths, Some(channel), root);
        return Ok(String::new());
    }
    translation::set_channel_source(paths, channel, source);
    let said = if source == ORIGINAL {
        let placed = archive.fetch_original(paths, root, true)?;
        if placed.written {
            let key = translation::note_key(ORIGINAL, Some(channel));
            translation::note_set(paths, &key, Some("Data.p4k"), Some("english"), None);
            if let Some(target) = game_ini::target_ini_in(paths, root, "english") {
                translation::note_head_of_file(paths, &key, &target);
            }
        }
        placed.said
    } else if translation::only_looks(paths, root) {
        let looked = translation::look_on_copy(net, paths, source, Some(channel)).map_err(|p| p.text())?;
        return Ok(looked.text_in(crate::language::current()));
    } else {
        let placed = translation::fetch(net, paths, source, root, Some(channel), progress).map_err(|p| p.text())?;
        fetched_text(source, channel, placed.bytes)
    };
    let kept = Kept { channel: Some(channel.to_string()), root: root.to_string(), source: source.to_string() };
    let Some(applied) = side_notes(paths, archive, &kept, &crate::stock_fetch::live_build(paths)) else {
        return Ok(said);
    };
    use crate::language::{Arg, tf};
    Ok(match applied {
        Outcome::Done(count) => format!("{said} · {}", tf("inj_aktiv", &[("0", Arg::from(count as i64))])),
        other => {
            errors::record("injection.kanal", &format!("{other:?}"));
            said
        }
    })
}

/// The notes in the text file of a side channel after its source changed: the old file of the channel (another
/// language folder) gets its wording back first, then the new one is written. `None` while notes are switched off
/// or there is no file.
fn side_notes(paths: &Paths, archive: &dyn Archive, kept: &Kept, version: &str) -> Option<Outcome> {
    if !paths.setting_bool(SETTING_ON, true) {
        return None;
    }
    let target = kept.target(paths).filter(|t| game_ini::is_file(t))?;
    if injection::clean_leftover(paths, Some(&target)).is_some() {
        injection::discard_origtext_for(paths, &target);
    }
    let inputs = prepare(paths, archive, version);
    Some(injection::apply_texts(paths, &target, &kept.language(paths), &inputs, false))
}

/// `_aktive_quelle` of the 4.12 watcher: the chosen source of the main installation when
/// it is set up. A choice that is set counts as it is, also the original
/// texts and „no translation“: then there is no source to fetch or to enter in
/// the `user.cfg` (4.12 fell back to the first source with a note and so
/// undid such a choice). Only without any choice the first set up counts.
pub fn active_source(paths: &Paths) -> Option<String> {
    let all: Vec<&str> = SOURCES.iter().map(|s| s.id).chain([CUSTOM]).collect();
    let settings = paths.settings();
    match settings.iter().find(|(k, _)| k == game_ini::SETTING_SOURCE).map(|(_, v)| v) {
        Some(Value::Str(chosen)) => {
            let chosen = crate::text::py_strip(chosen);
            (all.contains(&chosen) && translation::installed(paths, chosen, None).is_some()).then(|| chosen.to_string())
        }
        _ => all.into_iter().find(|s| translation::installed(paths, s, None).is_some()).map(str::to_string),
    }
}

/// `_spielsprache_pruefen` of the 4.12 watcher: put the language of the chosen
/// translation back into the user.cfg when it is missing. Only for a source
/// the player chose, and only when its file is there.
pub fn check_game_language(paths: &Paths, source: Option<&str>) -> Option<Notice> {
    let source = source.map(str::to_string).or_else(|| main_source(paths))?;
    let root = paths.game_folder()?;
    language_line(paths, &Kept { channel: None, root, source })
}

/// `_inj_mark` of the 4.12 watcher: collection, version and the shop catalog stand.
pub fn stock_mark(paths: &Paths, owned: &BTreeSet<String>) -> String {
    let mut mark = format!("{}@{}", injection::stock_mark(owned), version());
    if item_info::enabled(paths) {
        let fetched = jsonio::read_json(&paths.app_file(sources::SHOPS_FILE))
            .ok()
            .filter(|d| d.get("format") == Some(&Value::int(sources::SHOPS_FORMAT)))
            .and_then(|d| d.get("geholt").cloned())
            .filter(|v| injection::texts::truthy(Some(v)))
            .map(|v| injection::texts::py_str(&v))
            .unwrap_or_default();
        mark.push_str(&format!("@{fetched}"));
    }
    mark
}

/// Size and time of the files another program rewrites: the text file the notes go into and the `user.cfg` of the
/// main installation, and the same of every side channel VerseKit keeps; `None` for a file that is not there.
type FileStamp = Vec<(String, Option<(u64, std::time::SystemTime)>)>;

fn file_stamp(paths: &Paths) -> Option<FileStamp> {
    let folder =
        active_source(paths).map_or_else(|| "english".to_string(), |s| translation::language_folder(paths, &s, None));
    let main = paths.game_folder();
    let mut files: Vec<String> = game_ini::target_ini(paths, &folder).into_iter().collect();
    files.extend(main.as_deref().map(|root| paths.host.flavor.join(root, "user.cfg")));
    for kept in side_channels(paths, main.as_deref()) {
        files.extend(kept.target(paths));
        files.push(paths.host.flavor.join(&kept.root, "user.cfg"));
    }
    if files.is_empty() {
        return None;
    }
    let stamp = |file: &str| std::fs::metadata(file).ok().and_then(|info| Some((info.len(), info.modified().ok()?)));
    Some(
        files
            .into_iter()
            .map(|file| {
                let at = stamp(&file);
                (file, at)
            })
            .collect(),
    )
}

/// One run of the background, decided by [`Upkeep::tick`].
#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    /// The side channels to look at (timed runs only).
    pub channels: Vec<(String, String)>,
    pub source: Option<String>,
    pub own_original: bool,
    /// Only the collection changed: no look at foreign changes.
    pub stock_only: bool,
}

/// The clocks of the background (`texte_*` of the 4.12 watcher).
#[derive(Clone, Debug, Default)]
pub struct Upkeep {
    pub texts_next: f64,
    pub stock_next: f64,
    pub file_next: f64,
    pub p4k_next: f64,
    p4k_stamp: Option<String>,
    quiet_until: f64,
    pub patch_at: f64,
    file_stamp: Option<FileStamp>,
    /// A run is going on; no second one starts.
    pub running: bool,
}

impl Upkeep {
    /// A game session ended: look at once.
    pub fn game_ended(&mut self) {
        self.texts_next = 0.0;
    }

    fn texts_interval(&self, now: f64) -> f64 {
        if self.patch_at > 0.0 && now - self.patch_at < POST_PATCH_WINDOW_SECONDS {
            POST_PATCH_POLL_SECONDS
        } else {
            TEXTS_POLL_SECONDS
        }
    }

    /// `_patch_finished` of the 4.12 watcher.
    fn patch_finished(&mut self, paths: &Paths, archive: &dyn Archive, now: f64) -> bool {
        if now < self.p4k_next {
            return false;
        }
        self.p4k_next = now + FILE_POLL_SECONDS;
        let Some(stamp) = archive.stamp(paths) else { return false };
        let Some(known_now) = self.p4k_stamp.clone() else {
            self.p4k_stamp = Some(stamp.clone());
            match paths.setting(P4K_SETTING) {
                None => {
                    paths.set_setting(P4K_SETTING, Value::str(&stamp));
                }
                Some(known) if known != stamp => self.quiet_until = now + PATCH_QUIET_SECONDS,
                Some(_) => {}
            }
            return false;
        };
        if stamp != known_now {
            self.p4k_stamp = Some(stamp);
            self.quiet_until = now + PATCH_QUIET_SECONDS;
            return false;
        }
        if self.quiet_until > 0.0 && now >= self.quiet_until {
            self.quiet_until = 0.0;
            self.patch_at = now;
            paths.set_setting(P4K_SETTING, Value::str(&stamp));
            return true;
        }
        false
    }

    /// `_textdatei_geaendert` of the 4.12 watcher.
    fn file_changed(&mut self, paths: &Paths) -> bool {
        let Some(stamp) = file_stamp(paths) else { return false };
        match &self.file_stamp {
            None => {
                self.file_stamp = Some(stamp);
                false
            }
            Some(known) => *known != stamp,
        }
    }

    /// `_texte_tick` of the 4.12 watcher: whether a run is due, and which. `owned` is the
    /// collection as it is now.
    pub fn tick(&mut self, paths: &Paths, archive: &dyn Archive, owned: &BTreeSet<String>, now: f64) -> Option<Job> {
        if crate::net::offline_in(&paths.host) || self.running {
            return None;
        }
        if !paths.setting_bool(SETTING_ON, true) || !paths.setting_bool(SETTING_AUTO, true) {
            self.texts_next = now + TEXTS_POLL_SECONDS;
            return None;
        }
        let mut due = now >= self.texts_next;
        let mut stock_new = false;
        if !due && now >= self.stock_next {
            self.stock_next = now + STOCK_POLL_SECONDS;
            stock_new = paths.setting(SETTING_STOCK_MARK).as_deref() != Some(stock_mark(paths, owned).as_str());
        }
        let mut file_new = false;
        if !due && !stock_new && now >= self.file_next {
            self.file_next = now + FILE_POLL_SECONDS;
            file_new = self.file_changed(paths);
        }
        if !due && !stock_new && !file_new {
            due = self.patch_finished(paths, archive, now);
        }
        if !due && !stock_new && !file_new {
            return None;
        }
        let source = active_source(paths);
        let own_original = translation::installed(paths, ORIGINAL, None).is_some();
        // The side channels in every run: their files change with the collection and with another program too.
        let channels: Vec<(String, String)> = side_channels(paths, paths.game_folder().as_deref())
            .into_iter()
            .filter_map(|kept| kept.channel.map(|channel| (channel, kept.source)))
            .collect();
        if source.is_none() && !own_original && channels.is_empty() {
            return None;
        }
        if due {
            self.texts_next = now + self.texts_interval(now);
        }
        self.running = true;
        Some(Job { channels, source, own_original, stock_only: !(due || file_new) })
    }

    /// The run is over; remember the text file as it is now, so that the
    /// own writing does not count as another tool's.
    pub fn finished(&mut self, paths: &Paths, translation_renewed: bool) {
        self.running = false;
        if translation_renewed {
            self.patch_at = 0.0;
        }
        self.file_stamp = file_stamp(paths);
    }
}

/// What a run did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ran {
    pub notices: Vec<Notice>,
    /// A new version of the translation of the main installation came.
    pub renewed: bool,
}

/// `_kanaele_abgleichen` of the 4.12 watcher and `_texte_abgleichen`: carry out a run.
/// Never fails; problems go to the error log.
pub fn run(job: &Job, paths: &Paths, net: &dyn Net, archive: &dyn Archive, game_version: &str) -> Ran {
    let mut ran = Ran::default();
    let mut inputs: Option<Inputs> = None;
    let mut side_written = None;
    if !job.channels.is_empty() {
        // Whether the collection or the catalog changed since the last writing; the main installation notes it anew.
        let mark = stock_mark(paths, &sources::owned(paths));
        let stand = catalog_stand(paths);
        let marks_new = paths.setting(SETTING_STOCK_MARK).as_deref() != Some(mark.as_str())
            || (!stand.is_empty() && paths.setting(SETTING_CATALOG).as_deref() != Some(stand.as_str()));
        let sides = side_channels(paths, paths.game_folder().as_deref());
        for (channel, source) in &job.channels {
            let Some(kept) = sides.iter().find(|k| k.channel.as_deref() == Some(channel) && k.source == *source) else {
                continue;
            };
            // On a copy of the data the run only looks and notes what it found; the page says it calmly.
            if translation::only_looks(paths, &kept.root) {
                if !job.stock_only && source != ORIGINAL {
                    let _ = translation::look_on_copy(net, paths, source, Some(channel));
                }
                continue;
            }
            let mut fresh = false;
            if !job.stock_only {
                if let Some(notice) = put_back(paths, net, archive, kept) {
                    ran.notices.push(notice);
                    fresh = true;
                } else if source != ORIGINAL {
                    fresh = renew_side(paths, net, kept, &mut ran);
                }
                ran.notices.extend(language_line(paths, kept));
            }
            let Some(target) = kept.target(paths).filter(|t| game_ini::is_file(t)) else { continue };
            if !paths.setting_bool(SETTING_ON, true) || !(fresh || marks_new || !injection::is_applied(&target)) {
                continue;
            }
            let inputs = inputs.get_or_insert_with(|| prepare(paths, archive, game_version));
            match injection::apply_texts(paths, &target, &kept.language(paths), inputs, false) {
                Outcome::Done(_) => side_written = Some((mark.clone(), stand.clone())),
                other => errors::record("watcher.kanaele", &format!("{other:?}")),
            }
        }
    }
    if job.source.is_some() || job.own_original {
        reconcile(job, paths, net, archive, game_version, &mut ran);
    } else if let Some((mark, stand)) = side_written {
        // Without notes in the main installation the side channels keep the marks.
        paths.set_setting(SETTING_STOCK_MARK, Value::str(mark));
        if !stand.is_empty() {
            paths.set_setting(SETTING_CATALOG, Value::str(stand));
        }
    }
    ran
}

/// Look for a newer version of the source of a side channel and fetch it; whether it came.
fn renew_side(paths: &Paths, net: &dyn Net, kept: &Kept, ran: &mut Ran) -> bool {
    let (channel, source) = (kept.channel.as_deref().unwrap_or_default(), kept.source.as_str());
    let (newer, ident) = translation::update_available(net, paths, source, Some(channel));
    if !newer {
        return false;
    }
    match translation::fetch(net, paths, source, &kept.root, Some(channel), &|_| {}) {
        Ok(_) => {
            let said =
                translation::status_text(paths, source, Some(channel), crate::language::current(), &translation::today());
            let said = if said.is_empty() { ident.unwrap_or_default() } else { said };
            ran.notices.push(Notice::TranslationUpdated(format!("{channel} · {said}")));
            true
        }
        Err(problem) => {
            errors::record("watcher.kanaele", &problem.text());
            false
        }
    }
}

/// `_texte_abgleichen` of the 4.12 watcher.
fn reconcile(job: &Job, paths: &Paths, net: &dyn Net, archive: &dyn Archive, game_version: &str, ran: &mut Ran) {
    let source = job.source.as_deref();
    let folder = source.map_or_else(|| "english".to_string(), |s| translation::language_folder(paths, s, None));
    let Some(target) = game_ini::target_ini(paths, &folder) else { return };
    let mut needed = false;
    let mut renewed = false;
    // The file another program put in place of the chosen one goes first; the language follows it.
    let kept = paths.game_folder().zip(source.map(str::to_string).or_else(|| main_source(paths))).map(
        |(root, source)| Kept { channel: None, root, source },
    );
    if !job.stock_only
        && let Some(kept) = &kept
    {
        if let Some(notice) = put_back(paths, net, archive, kept) {
            ran.notices.push(notice);
            needed = true;
            renewed = true;
        }
        ran.notices.extend(language_line(paths, kept));
    }
    let looks_only = paths.game_folder().is_some_and(|root| translation::only_looks(paths, &root));
    if renewed {
        // Just placed anew: the newest version, nothing to look for.
    } else if let Some(source) = source.filter(|_| !job.stock_only && looks_only) {
        // On a copy of the data only a look: the note says whether a newer version is out.
        let _ = translation::look_on_copy(net, paths, source, None);
    } else if let Some(source) = source.filter(|_| !job.stock_only) {
        let (newer, ident) = translation::update_available(net, paths, source, None);
        if newer && let Some(root) = paths.game_folder() {
            match translation::fetch(net, paths, source, &root, None, &|_| {}) {
                Ok(_) => {
                    let said = translation::status_text(
                        paths,
                        source,
                        None,
                        crate::language::current(),
                        &translation::today(),
                    );
                    ran.notices.push(Notice::TranslationUpdated(if said.is_empty() {
                        ident.unwrap_or_default()
                    } else {
                        said
                    }));
                    needed = true;
                    renewed = true;
                    ran.renewed = true;
                }
                Err(problem) => errors::record("watcher.texte", &problem.text()),
            }
        }
    }
    if !job.stock_only && job.own_original && archive.refresh_original(paths) {
        ran.notices.push(Notice::TranslationUpdated(translation::display_name(ORIGINAL)));
        // The version box of the game changed with the patch: the new file is the one placed.
        if let Some(english) = game_ini::target_ini(paths, "english") {
            translation::note_head_of_file(paths, ORIGINAL, &english);
        }
        needed = true;
    }
    if !job.stock_only && source.is_some() && !renewed {
        let filled = archive.fill_gaps(paths, &target);
        if filled > 0 {
            ran.notices.push(Notice::GapsFilled(filled));
            needed = true;
        }
    }
    let mut stand = String::new();
    if !job.stock_only {
        stand = catalog_stand(paths);
        if !stand.is_empty() && paths.setting(SETTING_CATALOG).as_deref() != Some(stand.as_str()) {
            needed = true;
        }
    }
    if !needed && !job.stock_only && !injection::is_applied(&target) {
        needed = true;
    }
    let owned = sources::owned(paths);
    let mut mark = None;
    if !needed {
        let now = stock_mark(paths, &owned);
        needed = paths.setting(SETTING_STOCK_MARK).as_deref() != Some(now.as_str());
        mark = Some(now);
    }
    if needed && game_ini::is_file(&target) {
        let inputs = prepare(paths, archive, game_version);
        if let Outcome::Done(count) = injection::apply_texts(paths, &target, &folder, &inputs, false) {
            ran.notices.push(Notice::Applied(count));
            paths.set_setting(SETTING_STOCK_MARK, Value::str(mark.unwrap_or_else(|| stock_mark(paths, &owned))));
            if !stand.is_empty() {
                paths.set_setting(SETTING_CATALOG, Value::str(stand));
            }
        }
    }
}

pub mod watch;

#[cfg(test)]
mod tests;
