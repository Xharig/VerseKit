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

//! The files of the game in a backup, channel by channel: the active binding (`actionmaps.xml`), the settings of
//! the game beside it (`attributes.xml`), the saved profiles (`mappings/*.xml`), the `user.cfg` and the character
//! templates (`CustomCharacters/*.chf`) of every channel that is installed.
//!
//! In the archive they lie under `Steuerung/Kanaele/<channel>/`. The channel that is set up is written a second time
//! the old way, straight under `Steuerung/` ([`backup::bindings`]): older versions of the program read only that and
//! leave everything below `Steuerung/Kanaele/` alone (they take only `actionmaps.xml`, `attributes.xml` and
//! `mappings/` right under `Steuerung/`, and never write anything of `Steuerung/` into the data folder).
//!
//! ⚠⚠ The way back keeps the two groups of the old way apart. What only adds (the saved profiles and the character
//! templates: they lie there until the player loads one) comes back on a first yes; what the game works with now (the
//! active binding, the `attributes.xml`, the `user.cfg`) only with a second one. Every file that is replaced gets a
//! copy first (`game_files`), the `user.cfg` keeps the language lines of the program (`usercfg::write_own`), and
//! nothing is written while the game runs or nobody can tell.
//!
//! ⚠ Everything is read and checked first, then written: a damaged entry must not leave half of the files replaced.

use std::fs;
use std::path::Path;

use crate::backup::{self, Binding, BindingsError, Change, PreviewItem};
use crate::game_files::{self, WriteError};
use crate::paths::{self, Paths};
use crate::pypath::Flavor;
use crate::{errors, joysticks, usercfg, zip};

/// The prefix in the archive of the files of the game, channel by channel; the name of the channel follows.
pub const CHANNELS_PREFIX: &str = "Steuerung/Kanaele/";
/// The active binding, after the channel.
const ACTIVE: &str = "actionmaps.xml";
/// The settings of the game beside the active binding, after the channel.
const GAME_SETTINGS: &str = "attributes.xml";
/// The folder of the saved profiles, after the channel.
const PROFILES: &str = "mappings/";
/// The folder of the character templates, after the channel (and in the game folder below `USER/Client/0`).
pub const TEMPLATES: &str = "CustomCharacters/";
/// The ending of a character template.
const TEMPLATE_SUFFIX: &str = ".chf";
/// The largest character template that is put back; a real one is a few kilobytes.
const TEMPLATE_MAX: usize = 4 << 20;

/// The spellings of the folders above `0` (as in `joysticks`).
const TOPS: [&str; 2] = ["USER", "user"];
const MIDDLES: [&str; 2] = ["Client", "client"];

/// The installed channels, as (name, folder), each name once, in the order of [`paths::CHANNELS`].
pub fn channels(paths: &Paths) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = Vec::new();
    for (name, folder, _) in paths.installed_channels() {
        if !found.iter().any(|(known, _)| *known == name) {
            found.push((name, folder));
        }
    }
    found
}

/// The folders of the character templates in the channel `folder`, without the ones that are the same folder under
/// another spelling.
fn template_folders(flavor: Flavor, folder: &str) -> Vec<String> {
    let mut found: Vec<((u64, u64), String)> = Vec::new();
    for top in TOPS {
        for middle in MIDDLES {
            let path = flavor.join_all(folder, &[top, middle, "0", TEMPLATES.trim_end_matches('/')]);
            if !Path::new(&path).is_dir() {
                continue;
            }
            let id = paths::file_identity(&path);
            if id == (0, 0) || found.iter().any(|(known, _)| *known == id) {
                continue;
            }
            found.push((id, path));
        }
    }
    found.into_iter().map(|(_, path)| path).collect()
}

/// The character templates of the channel `folder`: name in the archive after the channel, full path. Of two files of
/// the same name (in two spellings of the folder) the newer one is taken.
fn templates_in(flavor: Flavor, folder: &str) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = Vec::new();
    for dir in template_folders(flavor, folder) {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let Ok(name) = entry.file_name().into_string() else { continue };
            if !name.to_lowercase().ends_with(TEMPLATE_SUFFIX) || !entry.path().is_file() {
                continue;
            }
            let full = entry.path().to_string_lossy().into_owned();
            let key = format!("{TEMPLATES}{name}");
            if let Some((_, previous)) = found.iter().find(|(known, _)| known.to_lowercase() == key.to_lowercase()) {
                let time = |path: &str| paths::mtime_seconds(path).unwrap_or(0.0);
                if time(previous) >= time(&full) {
                    continue;
                }
            }
            found.retain(|(known, _)| known.to_lowercase() != key.to_lowercase());
            found.push((key, full));
        }
    }
    found
}

/// The files of the game of one channel that go into a backup, named under [`CHANNELS_PREFIX`].
pub fn channel_bindings(paths: &Paths, channel: &str, folder: &str) -> Vec<Binding> {
    let flavor = paths.host.flavor;
    let prefix = format!("{CHANNELS_PREFIX}{channel}/");
    let mut list: Vec<Binding> = backup::bindings(paths, Some(folder))
        .into_iter()
        .filter_map(|binding| {
            let rest = binding.name.strip_prefix(backup::CONTROLS)?.to_string();
            Some(Binding { full: binding.full, name: format!("{prefix}{rest}") })
        })
        .collect();
    let cfg = Path::new(folder).join(usercfg::FILE);
    if cfg.is_file() {
        list.push(Binding { full: cfg.to_string_lossy().into_owned(), name: format!("{prefix}{}", usercfg::FILE) });
    }
    for (rest, full) in templates_in(flavor, folder) {
        list.push(Binding { full, name: format!("{prefix}{rest}") });
    }
    list.sort_by(|a, b| a.name.cmp(&b.name));
    list
}

/// Every file of the game that goes into a backup: the controls of the channel that is set up the old way (for older
/// versions of the program), then the files of every installed channel.
pub fn bindings_all(paths: &Paths) -> Vec<Binding> {
    let mut list = backup::bindings(paths, None);
    for (channel, folder) in channels(paths) {
        list.extend(channel_bindings(paths, &channel, &folder));
    }
    list
}

/// What a file of the game is.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Kind {
    Active,
    GameSettings,
    Profile(String),
    UserCfg,
    Template(String),
}

impl Kind {
    /// Does it belong to what the game works with now (asked about apart)?
    fn is_active(&self) -> bool {
        matches!(self, Kind::Active | Kind::GameSettings | Kind::UserCfg)
    }

    /// The name shown to the player, after the channel.
    fn shown(&self) -> String {
        match self {
            Kind::Active => ACTIVE.to_string(),
            Kind::GameSettings => GAME_SETTINGS.to_string(),
            Kind::Profile(name) => format!("{PROFILES}{name}"),
            Kind::UserCfg => usercfg::FILE.to_string(),
            Kind::Template(name) => format!("{TEMPLATES}{name}"),
        }
    }
}

/// A file of the game in the archive: its channel (`None`: the old way, the channel that is set up), what it is, and
/// its entry.
struct Member<'a> {
    channel: Option<String>,
    kind: Kind,
    entry: &'a zip::Entry,
}

/// May a template of this name be written into the folder of the templates?
fn template_name_ok(name: &str, flavor: Flavor) -> bool {
    let lower = name.to_lowercase();
    lower.len() > TEMPLATE_SUFFIX.len()
        && lower.ends_with(TEMPLATE_SUFFIX)
        && !name.contains([':', '/', '\\'])
        && !(flavor == Flavor::Windows && backup::windows_reads_differently(name))
}

/// The files of the game in the archive, each name once (the last entry of a name counts), and how many could not be
/// placed for their name. Where the archive holds files by channel, the old way (the same files once more) is left
/// aside.
fn members(entries: &[zip::Entry], flavor: Flavor) -> (Vec<Member<'_>>, usize) {
    let mut found: Vec<Member<'_>> = Vec::new();
    let mut skipped = 0;
    for entry in entries.iter().rev() {
        let Some(rest) = entry.name.strip_prefix(backup::CONTROLS) else { continue };
        if entry.name.ends_with('/') || found.iter().any(|m| m.entry.name == entry.name) {
            continue;
        }
        let (channel, rest) = match rest.strip_prefix(CHANNELS_PREFIX.strip_prefix(backup::CONTROLS).unwrap_or("")) {
            Some(below) => {
                let Some((channel, rest)) = below.split_once('/') else { continue };
                if channel.is_empty() || channel.contains('\\') {
                    skipped += 1;
                    continue;
                }
                (Some(channel.to_string()), rest)
            }
            None => (None, rest),
        };
        let kind = if rest == ACTIVE {
            Kind::Active
        } else if rest == GAME_SETTINGS {
            Kind::GameSettings
        } else if let Some(name) = rest.strip_prefix(PROFILES) {
            let name = flavor.basename(name);
            if !backup::profile_name_ok(&name, flavor) {
                skipped += 1;
                continue;
            }
            Kind::Profile(name)
        } else if channel.is_some() && rest == usercfg::FILE {
            Kind::UserCfg
        } else if let Some(name) = rest.strip_prefix(TEMPLATES).filter(|_| channel.is_some()) {
            if !template_name_ok(name, flavor) {
                skipped += 1;
                continue;
            }
            Kind::Template(name.to_string())
        } else {
            continue;
        };
        found.push(Member { channel, kind, entry });
    }
    found.reverse();
    if found.iter().any(|m| m.channel.is_some()) {
        found.retain(|m| m.channel.is_some());
    }
    (found, skipped)
}

/// What of the game lies in a backup for one channel.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChannelContents {
    pub channel: String,
    /// The active binding.
    pub active: bool,
    /// The `attributes.xml`.
    pub game_settings: bool,
    /// The `user.cfg`.
    pub user_cfg: bool,
    /// The names of the saved profiles (without `.xml`), sorted without regard to case.
    pub profiles: Vec<String>,
    /// The names of the character templates (without `.chf`), sorted without regard to case.
    pub templates: Vec<String>,
}

impl ChannelContents {
    /// Is there something that only adds (profiles, templates)?
    pub fn has_additive(&self) -> bool {
        !self.profiles.is_empty() || !self.templates.is_empty()
    }

    /// Is there something the game works with now (active binding, game settings, `user.cfg`)?
    pub fn has_active(&self) -> bool {
        self.active || self.game_settings || self.user_cfg
    }
}

/// What of the game lies in the backup, channel by channel, in the order of [`paths::CHANNELS`] (others after it).
/// Empty for a backup that holds the controls the old way only (then [`backup::bindings_in_archive`] tells them) or
/// cannot be read.
pub fn in_archive(source: &str) -> Vec<ChannelContents> {
    let Ok(data) = backup::read_archive(source) else { return Vec::new() };
    let Ok(entries) = zip::entries(&data) else { return Vec::new() };
    let (members, _) = members(&entries, Flavor::native());
    let mut out: Vec<ChannelContents> = Vec::new();
    for member in members {
        let Some(channel) = member.channel else { continue };
        let slot = match out.iter().position(|c| c.channel == channel) {
            Some(slot) => slot,
            None => {
                out.push(ChannelContents { channel, ..ChannelContents::default() });
                out.len() - 1
            }
        };
        let contents = &mut out[slot];
        match member.kind {
            Kind::Active => contents.active = true,
            Kind::GameSettings => contents.game_settings = true,
            Kind::UserCfg => contents.user_cfg = true,
            Kind::Profile(name) => contents.profiles.push(name[..name.len() - 4].to_string()),
            Kind::Template(name) => contents.templates.push(name[..name.len() - TEMPLATE_SUFFIX.len()].to_string()),
        }
    }
    for contents in &mut out {
        contents.profiles.sort_by_key(|name| name.to_lowercase());
        contents.templates.sort_by_key(|name| name.to_lowercase());
    }
    let rank = |name: &str| paths::CHANNELS.iter().position(|known| *known == name).unwrap_or(usize::MAX);
    out.sort_by(|a, b| rank(&a.channel).cmp(&rank(&b.channel)).then_with(|| a.channel.cmp(&b.channel)));
    out
}

/// A file of the game, read and checked, ready to be written: into which channel folder, what it is, the bytes (for
/// the `user.cfg`: its own lines).
struct Planned {
    channel: String,
    folder: Option<String>,
    kind: Kind,
    content: Vec<u8>,
    lines: Vec<String>,
}

/// The place of a file in a channel that has none yet (the spelling the game itself uses).
fn default_place(flavor: Flavor, folder: &str, rest: &[&str]) -> String {
    let mut parts = vec!["USER", "Client", "0"];
    parts.extend_from_slice(rest);
    flavor.join_all(folder, &parts)
}

/// Where a file of `kind` goes in the channel `folder`. With `old_way` (a backup with the controls of the channel that
/// is set up only) as before: no active binding there, no place for the active state.
fn target_of(paths: &Paths, folder: &str, kind: &Kind, old_way: bool) -> Option<String> {
    let flavor = paths.host.flavor;
    let found_active = joysticks::actionmaps_path(paths, Some(folder));
    let active = if old_way {
        found_active
    } else {
        Some(found_active.unwrap_or_else(|| default_place(flavor, folder, &["Profiles", "default", ACTIVE])))
    };
    match kind {
        Kind::Active => active,
        Kind::GameSettings => active.map(|path| flavor.join_all(&flavor.dirname(&path), &[GAME_SETTINGS])),
        Kind::Profile(name) => {
            let dir = joysticks::all_mapping_folders(paths, Some(folder)).into_iter().next().or_else(|| {
                let active = active.as_deref()?;
                let zero = flavor.dirname(&flavor.dirname(&flavor.dirname(active)));
                Some(flavor.join_all(&zero, &["controls", "mappings"]))
            });
            dir.map(|dir| flavor.join_all(&dir, &[name]))
        }
        Kind::UserCfg => Some(flavor.join_all(folder, &[usercfg::FILE])),
        Kind::Template(name) => {
            let dir = template_folders(flavor, folder)
                .into_iter()
                .next()
                .unwrap_or_else(|| default_place(flavor, folder, &[TEMPLATES.trim_end_matches('/')]));
            Some(flavor.join_all(&dir, &[name]))
        }
    }
}

/// Reads and checks every file of the game in the archive: what can be written (with the folder of its channel, or
/// `None` where the channel is not installed here), how many were left out as not fit, whether the archive holds them
/// the old way only.
///
/// # Errors
///
/// [`BindingsError::Failed`] where an entry cannot be read.
fn plan(paths: &Paths, data: &[u8], entries: &[zip::Entry]) -> Result<(Vec<Planned>, usize, bool), BindingsError> {
    let flavor = paths.host.flavor;
    let (members, mut skipped) = members(entries, flavor);
    let old_way = members.iter().all(|m| m.channel.is_none());
    let installed = channels(paths);
    let set_up = paths.game_folder();
    let mut planned = Vec::new();
    for member in members {
        let (channel, folder) = match &member.channel {
            Some(name) => (name.clone(), installed.iter().find(|(known, _)| known == name).map(|(_, f)| f.clone())),
            None => {
                let name = set_up
                    .as_deref()
                    .and_then(|folder| installed.iter().find(|(_, f)| same_folder(f, folder)).map(|(n, _)| n.clone()))
                    .or_else(|| set_up.as_deref().map(|folder| flavor.basename(folder)))
                    .unwrap_or_default();
                (name, set_up.clone())
            }
        };
        let content = zip::read(data, member.entry).map_err(|e| {
            errors::record("backup_game.plan", &e);
            BindingsError::Failed(e.to_string())
        })?;
        let mut lines = Vec::new();
        let fine = match &member.kind {
            Kind::GameSettings => backup::xml_of(&content, None),
            Kind::Active | Kind::Profile(_) => backup::xml_of(&content, Some("ActionMaps")),
            Kind::Template(_) => !content.is_empty() && content.len() <= TEMPLATE_MAX,
            Kind::UserCfg => {
                let text = String::from_utf8_lossy(&content);
                let text = usercfg::normalized(text.strip_prefix('\u{feff}').unwrap_or(&text));
                lines = usercfg::own_lines(&text);
                !lines.is_empty() && usercfg::check(&lines).is_empty()
            }
        };
        if !fine {
            skipped += 1;
            continue;
        }
        planned.push(Planned { channel, folder, kind: member.kind, content, lines });
    }
    Ok((planned, skipped, old_way))
}

/// Are the two folders the same (also under another spelling or as a link)?
fn same_folder(a: &str, b: &str) -> bool {
    let (x, y) = (paths::file_identity(a), paths::file_identity(b));
    (x != (0, 0) && x == y) || a == b
}

/// The look before the files of the game of one channel are put back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelPreview {
    pub channel: String,
    /// Is the channel installed here? Where it is not, every file says [`Change::NoPlace`].
    pub installed: bool,
    /// What only adds (profiles, templates): comes back on the first yes.
    pub additive: Vec<PreviewItem>,
    /// What the game works with now (active binding, `attributes.xml`, `user.cfg`): only on the second yes.
    pub active: Vec<PreviewItem>,
}

/// The look before the files of the game are put back, channel by channel, planned as [`restore`] does. Empty where the
/// backup holds no files of the game or cannot be read.
pub fn preview(paths: &Paths, source: &str) -> Vec<ChannelPreview> {
    let Ok(data) = backup::read_archive(source) else { return Vec::new() };
    let Ok(entries) = zip::entries(&data) else { return Vec::new() };
    let Ok((planned, _, old_way)) = plan(paths, &data, &entries) else { return Vec::new() };
    let mut out: Vec<ChannelPreview> = Vec::new();
    for item in planned {
        let slot = match out.iter().position(|c| c.channel == item.channel) {
            Some(slot) => slot,
            None => {
                out.push(ChannelPreview {
                    channel: item.channel.clone(),
                    installed: item.folder.is_some(),
                    additive: Vec::new(),
                    active: Vec::new(),
                });
                out.len() - 1
            }
        };
        let change = match item.folder.as_deref().and_then(|f| target_of(paths, f, &item.kind, old_way).map(|t| (f, t))) {
            None => Change::NoPlace,
            Some((folder, _)) if item.kind == Kind::UserCfg => {
                let old = usercfg::read(folder);
                // The own lines decide: the language lines of this computer stay whatever the backup says.
                match usercfg::own_text(old.as_deref().unwrap_or_default(), &item.lines) {
                    Err(_) => Change::NoPlace,
                    Ok(_) if old.is_none() => Change::New,
                    Ok(_) if old.as_deref().is_some_and(|old| usercfg::own_lines(old) == item.lines) => Change::Same,
                    Ok(_) => Change::Replaced,
                }
            }
            Some((_, target)) => backup::change_of(&target, &item.content),
        };
        let entry = PreviewItem { name: item.kind.shown(), change };
        if item.kind.is_active() {
            out[slot].active.push(entry);
        } else {
            out[slot].additive.push(entry);
        }
    }
    let rank = |name: &str| paths::CHANNELS.iter().position(|known| *known == name).unwrap_or(usize::MAX);
    out.sort_by(|a, b| rank(&a.channel).cmp(&rank(&b.channel)).then_with(|| a.channel.cmp(&b.channel)));
    out
}

/// What [`restore`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameRestored {
    /// The files written into the game folders.
    pub count: usize,
    /// The files left out: not fit, or no place for them here.
    pub skipped: usize,
    /// The channels of the backup that are not installed here (only names of [`paths::CHANNELS`]).
    pub missing: Vec<String>,
}

/// Puts the files of the game of a backup back: what only adds (saved profiles, character templates) always, what the
/// game works with now (active binding, `attributes.xml`, `user.cfg`) only `with_active`. A backup that holds the
/// controls the old way only goes the old way ([`backup::restore_bindings`]).
///
/// # Errors
///
/// [`BindingsError::Invalid`] where the file is no backup of this program, [`BindingsError::GameNotQuiet`] where the
/// game may run (nothing was written), [`BindingsError::Failed`] with the reason where reading or writing broke off.
pub fn restore(paths: &Paths, source: &str, with_active: bool) -> Result<GameRestored, BindingsError> {
    if backup::check(source).is_none() {
        return Err(BindingsError::Invalid);
    }
    let data = backup::read_archive(source).map_err(BindingsError::Failed)?;
    let entries = zip::entries(&data).map_err(|e| BindingsError::Failed(e.to_string()))?;
    let (planned, mut skipped, old_way) = plan(paths, &data, &entries)?;
    if old_way {
        return backup::restore_bindings(paths, source, with_active, None).map(|done| GameRestored {
            count: done.count,
            skipped: done.skipped,
            missing: Vec::new(),
        });
    }
    let mut missing: Vec<String> = Vec::new();
    let mut work: Vec<(String, Planned)> = Vec::new();
    for item in planned {
        if item.kind.is_active() && !with_active {
            continue;
        }
        let Some(folder) = item.folder.clone() else {
            if paths::CHANNELS.contains(&item.channel.as_str()) && !missing.contains(&item.channel) {
                missing.push(item.channel.clone());
            }
            skipped += 1;
            continue;
        };
        work.push((folder, item));
    }
    if work.is_empty() {
        return Ok(GameRestored { count: 0, skipped, missing });
    }
    // Before the first file is written, not in the middle.
    let refused = |error: WriteError| match error {
        WriteError::Failed(io) => BindingsError::Failed(io.to_string()),
        quiet => BindingsError::GameNotQuiet(quiet.reason()),
    };
    game_files::check_quiet(paths).map_err(refused)?;
    let mut count = 0;
    for (folder, item) in work {
        if item.kind == Kind::UserCfg {
            match usercfg::write_own(paths, &folder, &item.lines) {
                Ok(true) => count += 1,
                Ok(false) => {}
                Err(usercfg::SetError::Write(error)) => return Err(refused(error)),
                Err(usercfg::SetError::Bad(_)) => skipped += 1,
            }
            continue;
        }
        let Some(target) = target_of(paths, &folder, &item.kind, false) else {
            skipped += 1;
            continue;
        };
        if let Some(parent) = Path::new(&target).parent() {
            fs::create_dir_all(parent).map_err(|e| BindingsError::Failed(e.to_string()))?;
        }
        game_files::write(paths, &target, &item.content).map_err(|error| {
            if let WriteError::Failed(io) = &error {
                errors::record("backup_game.restore", io);
            }
            refused(error)
        })?;
        count += 1;
    }
    Ok(GameRestored { count, skipped, missing })
}

#[cfg(test)]
mod tests;
