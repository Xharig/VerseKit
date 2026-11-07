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

//! Empty the shader caches of Star Citizen and of the graphics drivers (`shader_cache.py`, and
//! `sc_cache_roots`, `gpu_cache_dirs` of `paths.py`).
//!
//! The game puts a folder `starcitizen_(sc-alpha-…)_…` for each version under `AppData\Local\Star
//! Citizen`. In it:
//!
//! | folder | content | what happens |
//! |---|---|---|
//! | `shaders` | translated shaders | emptied |
//! | `vulkanshadercache` | Vulkan pipeline cache | emptied |
//! | `GraphicsSettings` | the player's graphics settings | **stays** |
//!
//! Beside them `crashes` (crash reports, emptied) and the caches of the graphics drivers
//! ([`gpu_cache_dirs`]). Only the **content** of these folders is deleted; the folders stay. Files that
//! are in use stay and are counted.
//!
//! The folders of a version are named after the branch of the game, e.g. `starcitizen_(sc-alpha-4.10.2)_cc9wt_0`.
//! The installed versions are read from the `build_manifest.id` of each channel ([`Branches`]); a folder of a
//! numbered version older than the newest installed one, and none that is installed, is [`Age::Old`]. A folder
//! whose version cannot be told is never taken for old. The player picks [`Place`]s: one version folder, one
//! maker of drivers, the crash reports.
//!
//! ⚠ This is deleting, final and without a copy: the page asks first and does not do it while the game
//! runs. Nothing here writes a file.

use std::fs;
use std::path::Path;

use crate::paths::Paths;
use crate::pypath::Flavor;

/// The folders of a version of the game start like this (any case).
const VERSION_PREFIX: &str = "starcitizen_";
/// The folders of a version whose content is emptied.
const SC_CACHE_SUBDIRS: [&str; 2] = ["shaders", "vulkanshadercache"];
/// The crash reports lie beside the versions.
const CRASH_DIR: &str = "crashes";

/// The driver caches on Windows below `AppData\Local`.
const GPU_CACHE_WINDOWS: [&[&str]; 7] = [
    &["NVIDIA", "DXCache"],
    &["NVIDIA", "GLCache"],
    &["AMD", "DxCache"],
    &["AMD", "DxcCache"],
    &["AMD", "VkCache"],
    &["Intel", "ShaderCache"],
    &["D3DSCache"],
];

/// The driver caches on Linux below `~/.cache`.
const GPU_CACHE_LINUX: [&[&str]; 3] = [&["nvidia", "GLCache"], &["mesa_shader_cache"], &["mesa_shader_cache_db"]];

/// What a folder holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Shaders of the game.
    Game,
    /// Crash reports of the game.
    Crash,
    /// Cache of a graphics driver (for all games; the driver builds it again by itself).
    Driver,
}

/// One folder that can be emptied: what it is, where, how many bytes it holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub kind: Kind,
    pub folder: String,
    pub bytes: u64,
}

/// The size of all files below `path` in bytes.
fn size_of(path: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(path) else { return 0 };
    entries
        .flatten()
        .map(|entry| match entry.metadata() {
            Ok(meta) if meta.is_dir() => size_of(&entry.path()),
            Ok(meta) => meta.len(),
            Err(_) => 0,
        })
        .sum()
}

fn names_of(folder: &str) -> Option<Vec<String>> {
    let mut names: Vec<String> =
        fs::read_dir(folder).ok()?.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    Some(names)
}

/// Every `AppData\Local` in which the game can put its caches: on Windows the own one, on Linux the one of
/// each user in each Wine prefix (also the prefix in which the game folder found lies).
pub fn local_appdata_dirs(paths: &Paths) -> Vec<String> {
    let f = paths.host.flavor;
    if f == Flavor::Windows {
        let base = match paths.host.var("LOCALAPPDATA").filter(|v| !v.is_empty()) {
            Some(var) => var.to_string(),
            None => f.join_all(&paths.host.home(), &["AppData", "Local"]),
        };
        return vec![base];
    }
    let mut prefixes = paths.wine_prefixes();
    if let Some(folder) = paths.game_folder()
        && let Some((head, _)) = folder.split_once("/drive_c/")
        && !prefixes.iter().any(|p| p == head)
    {
        prefixes.insert(0, head.to_string());
    }
    let mut found: Vec<String> = Vec::new();
    for prefix in prefixes {
        let users = f.join_all(&prefix, &["drive_c", "users"]);
        for user in names_of(&users).unwrap_or_default() {
            if user == "Public" {
                continue;
            }
            let local = f.join_all(&users, &[user.as_str(), "AppData", "Local"]);
            if Path::new(&local).is_dir() && !found.contains(&local) {
                found.push(local);
            }
        }
    }
    found
}

/// The folders `Star Citizen` below the `AppData\Local` folders. The name is searched without regard to
/// case: the game writes it once like this, once like that.
pub fn sc_cache_roots(paths: &Paths) -> Vec<String> {
    let f = paths.host.flavor;
    let mut roots = Vec::new();
    for local in local_appdata_dirs(paths) {
        for name in names_of(&local).unwrap_or_default() {
            let path = f.join(&local, &name);
            if name.to_lowercase() == "star citizen" && Path::new(&path).is_dir() {
                roots.push(path);
            }
        }
    }
    roots
}

/// The shader caches of the graphics drivers that exist. They hold for all games; the driver builds them
/// again at the next start by itself.
pub fn gpu_cache_dirs(paths: &Paths) -> Vec<String> {
    let f = paths.host.flavor;
    let mut candidates: Vec<String> = Vec::new();
    if f == Flavor::Windows {
        let local = local_appdata_dirs(paths).into_iter().next().unwrap_or_default();
        candidates.extend(GPU_CACHE_WINDOWS.iter().map(|parts| f.join_all(&local, parts)));
        let low = f.join(&f.dirname(&local), "LocalLow");
        candidates.push(f.join_all(&low, &["Intel", "ShaderCache"]));
    } else {
        let cache = match paths.host.var("XDG_CACHE_HOME").filter(|v| !v.is_empty()) {
            Some(var) => var.to_string(),
            None => f.join(&paths.host.home(), ".cache"),
        };
        candidates.extend(GPU_CACHE_LINUX.iter().map(|parts| f.join_all(&cache, parts)));
    }
    candidates.into_iter().filter(|p| Path::new(p).is_dir()).collect()
}

/// Is `path` a folder itself, and no link (symbolic link, junction) to a folder somewhere else? Emptying
/// follows no link: whatever lies behind it may be anything, and nothing here goes to a recycle bin.
fn is_real_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
}

/// Everything that can be emptied. Without the places given the real ones are searched through `paths`.
/// A folder that is a link to another place is passed over (see [`is_real_dir`]).
pub fn find(sc_roots: &[String], gpu_dirs: &[String]) -> Vec<Target> {
    let mut targets = Vec::new();
    for root in sc_roots {
        for name in names_of(root).unwrap_or_default() {
            let version = Path::new(root).join(&name);
            if !(name.to_lowercase().starts_with(VERSION_PREFIX) && is_real_dir(&version)) {
                continue;
            }
            for sub in SC_CACHE_SUBDIRS {
                let path = version.join(sub);
                if is_real_dir(&path) {
                    targets.push(Target {
                        kind: Kind::Game,
                        folder: path.to_string_lossy().into_owned(),
                        bytes: size_of(&path),
                    });
                }
            }
        }
        let crash = Path::new(root).join(CRASH_DIR);
        if is_real_dir(&crash) {
            targets.push(Target {
                kind: Kind::Crash,
                folder: crash.to_string_lossy().into_owned(),
                bytes: size_of(&crash),
            });
        }
    }
    for path in gpu_dirs {
        if is_real_dir(Path::new(path)) {
            targets.push(Target { kind: Kind::Driver, folder: path.clone(), bytes: size_of(Path::new(path)) });
        }
    }
    targets
}

/// Bytes for each kind: (game, crash reports, drivers).
pub fn total_by_kind(targets: &[Target]) -> (u64, u64, u64) {
    let sum = |kind| targets.iter().filter(|t| t.kind == kind).map(|t| t.bytes).sum();
    (sum(Kind::Game), sum(Kind::Crash), sum(Kind::Driver))
}

/// Empties `path` and leaves the folder itself. Gives (bytes freed, files skipped).
fn clear_dir(path: &Path) -> (u64, u64) {
    let (mut freed, mut skipped) = (0, 0);
    // The folder was a folder when it was found; between the question and the yes it may have become a
    // link (or vanished): then nothing is emptied.
    if !is_real_dir(path) {
        return (0, 0);
    }
    let Ok(entries) = fs::read_dir(path) else { return (0, 1) };
    for entry in entries.flatten() {
        let full = entry.path();
        let is_link = fs::symlink_metadata(&full).is_ok_and(|m| m.file_type().is_symlink());
        if full.is_dir() && !is_link {
            let (sub_freed, sub_skipped) = clear_dir(&full);
            freed += sub_freed;
            skipped += sub_skipped;
            // Only a folder that is empty now goes; one with a file in use stays.
            if sub_skipped == 0 && fs::remove_dir(&full).is_err() {
                let _ = fs::remove_dir_all(&full);
            }
            continue;
        }
        let size = fs::metadata(&full).map_or(0, |m| m.len());
        match fs::remove_file(&full) {
            Ok(()) => freed += size,
            Err(_) => skipped += 1,
        }
    }
    (freed, skipped)
}

/// Empties every folder of [`find`]. Gives (bytes freed, files skipped).
pub fn clear(targets: &[Target]) -> (u64, u64) {
    targets.iter().map(|t| clear_dir(Path::new(&t.folder))).fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1))
}

/// Where a cache lies, as the player picks it: one version of the game, the crash reports, or one maker of
/// graphics drivers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceKind {
    /// The caches of one version folder of the game.
    Game,
    /// The crash reports of the game.
    Crash,
    Nvidia,
    Amd,
    Intel,
    /// The cache Windows keeps for DirectX (`D3DSCache`).
    DirectX,
    /// The cache of the Mesa drivers on Linux.
    Mesa,
    /// A driver cache whose maker is not told by its name.
    OtherDriver,
}

impl PlaceKind {
    /// The order of the list: the game first, the drivers, the crash reports last.
    fn order(self) -> u8 {
        match self {
            PlaceKind::Game => 0,
            PlaceKind::Nvidia => 1,
            PlaceKind::Amd => 2,
            PlaceKind::Intel => 3,
            PlaceKind::DirectX => 4,
            PlaceKind::Mesa => 5,
            PlaceKind::OtherDriver => 6,
            PlaceKind::Crash => 7,
        }
    }
}

/// How old a version folder of the game is against the versions installed now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Age {
    /// The folder of a version that is installed: it keeps what the next start needs.
    Current,
    /// A version older than the newest one installed, and none that is installed.
    Old,
    /// Nobody can tell: the name is not a version, or the installed versions are not known for sure. Such a
    /// folder is never taken for old.
    Unclear,
}

/// The versions of the game installed now, from the `build_manifest.id` of each channel, and what they say about
/// a folder of the shader cache.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Branches {
    /// The branches of the channels installed (lower case), e.g. `sc-alpha-4.10.2`.
    names: Vec<String>,
    /// The newest of those that are numbered.
    newest: Option<Vec<u64>>,
    /// Every installed channel was read, and there is one: only then something can be called old.
    sure: bool,
}

/// The prefix of the branch of a numbered version.
const BRANCH_PREFIX: &str = "sc-alpha-";

/// The branch in a `build_manifest.id`, or `None` when the file is not there or says none.
fn branch_in_manifest(file: &Path) -> Option<String> {
    let text = fs::read_to_string(file).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let branch = value.get("Data")?.get("Branch")?.as_str()?.trim();
    (!branch.is_empty()).then(|| branch.to_lowercase())
}

/// A branch such as `sc-alpha-4.10.2` as numbers without the zeros at the end, so that `4.10` and `4.10.0` are
/// one version. `None` for a branch that is not numbered (`sc-patch-inc`).
fn rank(branch: &str) -> Option<Vec<u64>> {
    let numbers = branch.strip_prefix(BRANCH_PREFIX)?;
    let mut parts: Vec<u64> = numbers.split('.').map(|p| p.parse().ok()).collect::<Option<_>>()?;
    if parts.len() < 2 {
        return None;
    }
    while parts.last() == Some(&0) {
        parts.pop();
    }
    Some(parts)
}

/// The branch and the rest of the name of a version folder: `starcitizen_(sc-alpha-4.10.2)_cc9wt_0` gives
/// (`sc-alpha-4.10.2`, `cc9wt_0`). `None` when the name does not have this form.
fn split_name(name: &str) -> Option<(String, String)> {
    let lower = name.to_lowercase();
    let rest = lower.strip_prefix(VERSION_PREFIX)?.strip_prefix('(')?;
    let (branch, tail) = rest.split_once(')')?;
    (!branch.is_empty()).then(|| (branch.to_string(), tail.trim_start_matches('_').to_string()))
}

impl Branches {
    /// From the branches read: `complete` when every installed channel could be read.
    pub fn new(names: Vec<String>, complete: bool) -> Branches {
        let newest = names.iter().filter_map(|n| rank(n)).max();
        let sure = complete && !names.is_empty();
        Branches { names, newest, sure }
    }

    /// Nothing is known: no folder is old.
    pub fn unknown() -> Branches {
        Branches::new(Vec::new(), false)
    }

    /// Reads the `build_manifest.id` of every installed channel. A channel that holds a game (`Data.p4k`) and
    /// whose file cannot be read makes the answer unsure; a folder without game is not an installation.
    pub fn read(paths: &Paths) -> Branches {
        let f = paths.host.flavor;
        let (mut names, mut complete) = (Vec::new(), true);
        for (_, folder, has_game) in paths.installed_channels() {
            if !has_game {
                continue;
            }
            match branch_in_manifest(Path::new(&f.join(&folder, "build_manifest.id"))) {
                Some(branch) => {
                    if !names.contains(&branch) {
                        names.push(branch);
                    }
                }
                None => complete = false,
            }
        }
        Branches::new(names, complete)
    }

    /// Are the installed versions known for sure (the base of everything called old)?
    pub fn is_sure(&self) -> bool {
        self.sure
    }

    /// What the name of a version folder says. Only a folder of a numbered version that is older than the newest
    /// installed one and not installed itself is [`Age::Old`].
    pub fn classify(&self, folder_name: &str) -> Age {
        let Some((branch, _)) = split_name(folder_name) else { return Age::Unclear };
        if self.names.contains(&branch) {
            return Age::Current;
        }
        match (self.sure, rank(&branch), &self.newest) {
            (true, Some(this), Some(newest)) if this < *newest => Age::Old,
            _ => Age::Unclear,
        }
    }
}

/// One place the player can pick: a version folder, the crash reports or one maker of drivers, with everything
/// that is emptied there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    pub kind: PlaceKind,
    /// A version folder: its branch from the name, e.g. `sc-alpha-4.10.2`; else empty.
    pub branch: String,
    /// A version folder: the rest of its name after the brackets, e.g. `cc9wt_0`; else empty.
    pub suffix: String,
    /// A version folder: how old it is; else [`Age::Unclear`].
    pub age: Age,
    /// A version folder: its path; else empty.
    pub folder: String,
    pub targets: Vec<Target>,
    pub bytes: u64,
}

impl Place {
    /// The version in words, e.g. `4.10.2` (the branch without its prefix).
    pub fn version(&self) -> &str {
        self.branch.strip_prefix(BRANCH_PREFIX).unwrap_or(&self.branch)
    }
}

/// The maker a driver cache belongs to, from the last two parts of its path.
fn driver_kind(folder: &str) -> PlaceKind {
    let parts: Vec<String> = folder.split(['/', '\\']).filter(|p| !p.is_empty()).map(str::to_lowercase).collect();
    let tail = &parts[parts.len().saturating_sub(2)..];
    let has = |name: &str| tail.iter().any(|p| p == name);
    if has("nvidia") {
        PlaceKind::Nvidia
    } else if has("amd") {
        PlaceKind::Amd
    } else if has("intel") {
        PlaceKind::Intel
    } else if has("d3dscache") {
        PlaceKind::DirectX
    } else if tail.iter().any(|p| p.starts_with("mesa_shader")) {
        PlaceKind::Mesa
    } else {
        PlaceKind::OtherDriver
    }
}

/// [`find`] grouped into the places the player picks, with the version folders judged against `branches`. A
/// version folder holds its `shaders` and `vulkanshadercache` as one place; the caches of one maker, and all
/// crash reports, are one place each. Newest version first.
pub fn find_places(sc_roots: &[String], gpu_dirs: &[String], branches: &Branches) -> Vec<Place> {
    let mut places: Vec<Place> = Vec::new();
    for target in find(sc_roots, gpu_dirs) {
        let version_folder = match target.kind {
            Kind::Game => Path::new(&target.folder).parent().map(|p| p.to_string_lossy().into_owned()),
            _ => None,
        };
        let kind = match target.kind {
            Kind::Game => PlaceKind::Game,
            Kind::Crash => PlaceKind::Crash,
            Kind::Driver => driver_kind(&target.folder),
        };
        let same = |p: &Place| match &version_folder {
            Some(folder) => p.kind == PlaceKind::Game && p.folder == *folder,
            None => p.kind == kind,
        };
        let place = match places.iter().position(same) {
            Some(at) => &mut places[at],
            None => {
                let name =
                    version_folder.as_deref().and_then(|f| Path::new(f).file_name()).map(|n| n.to_string_lossy());
                let (branch, suffix) = name.as_deref().and_then(split_name).unwrap_or_default();
                let age = name.as_deref().map_or(Age::Unclear, |n| branches.classify(n));
                places.push(Place {
                    kind,
                    branch,
                    suffix,
                    age,
                    folder: version_folder.unwrap_or_default(),
                    targets: Vec::new(),
                    bytes: 0,
                });
                let last = places.len() - 1;
                &mut places[last]
            }
        };
        place.bytes += target.bytes;
        place.targets.push(target);
    }
    places.sort_by(|a, b| {
        let newest_first = |p: &Place| std::cmp::Reverse(rank(&p.branch));
        (a.kind.order(), newest_first(a), &a.suffix).cmp(&(b.kind.order(), newest_first(b), &b.suffix))
    });
    places
}

/// What emptying one place did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cleared {
    /// Bytes freed.
    pub freed: u64,
    /// Files that were in use and stayed.
    pub skipped: u64,
    /// The place was left alone: it was asked as an old one and is not (any more).
    pub kept: bool,
}

/// Empties the places `chosen` (indices into `places`) and tells, for each, what happened. With `only_old` given
/// a place is emptied **only** when it is a version folder that `only_old` calls [`Age::Old`] right now; whatever
/// else was chosen is left alone (`kept`), however it got into the choice.
pub fn clear_places(places: &[Place], chosen: &[usize], only_old: Option<&Branches>) -> Vec<(usize, Cleared)> {
    let mut done = Vec::new();
    for &at in chosen {
        let Some(place) = places.get(at) else { continue };
        if let Some(branches) = only_old {
            let name = Path::new(&place.folder).file_name().map(|n| n.to_string_lossy().into_owned());
            let old = place.kind == PlaceKind::Game && name.is_some_and(|n| branches.classify(&n) == Age::Old);
            if !old {
                done.push((at, Cleared { freed: 0, skipped: 0, kept: true }));
                continue;
            }
        }
        let (freed, skipped) = clear(&place.targets);
        done.push((at, Cleared { freed, skipped, kept: false }));
    }
    done
}

/// Bytes as a readable size: `740 MB`, `14,9 GB` (with `decimal` as the decimal sign).
pub fn format_size(size: u64, decimal: char) -> String {
    let gb = size as f64 / 1024f64.powi(3);
    if gb >= 1.0 {
        return format!("{gb:.1} GB").replace('.', &decimal.to_string());
    }
    // Python's `round` goes to the even number at a half.
    let mb = size as f64 / 1024f64.powi(2);
    let rounded = if (mb - mb.floor() - 0.5).abs() < f64::EPSILON { 2.0 * (mb / 2.0).round() } else { mb.round() };
    format!("{} MB", rounded as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Host;

    fn scratch(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("versekit-shader-{}-{name}", crate::testdir::run_tag()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("folder");
        dir.to_string_lossy().replace('\\', "/")
    }

    fn put(path: &str, bytes: usize) {
        fs::create_dir_all(Path::new(path).parent().expect("parent")).expect("folder");
        fs::write(path, vec![7u8; bytes]).expect("file");
    }

    /// A tree like the game makes it: two versions, the settings, crash reports.
    fn tree(home: &str) -> String {
        let root = format!("{home}/Star Citizen");
        let version = format!("{root}/starcitizen_(sc-alpha-4.1.0)_abc");
        put(&format!("{version}/shaders/a.bin"), 1000);
        put(&format!("{version}/vulkanshadercache/sub/b.bin"), 2000);
        put(&format!("{version}/GraphicsSettings/GraphicsSettings.json"), 50);
        put(&format!("{root}/STARCITIZEN_(sc-alpha-4.0.0)_old/shaders/c.bin"), 300);
        put(&format!("{root}/other/shaders/d.bin"), 9);
        put(&format!("{root}/crashes/e.dmp"), 400);
        root
    }

    #[test]
    fn what_can_be_emptied_is_found_and_the_settings_are_not_among_it() {
        let home = scratch("find");
        let root = tree(&home);
        let driver = format!("{home}/cache/nvidia/GLCache");
        put(&format!("{driver}/f.bin"), 5000);
        let targets = find(std::slice::from_ref(&root), std::slice::from_ref(&driver));
        let folders: Vec<&str> = targets.iter().map(|t| t.folder.rsplit('/').next().unwrap_or("")).collect();
        assert_eq!(targets.len(), 5, "{folders:?}");
        assert!(targets.iter().all(|t| !t.folder.contains("GraphicsSettings") && !t.folder.contains("/other/")));
        assert_eq!(total_by_kind(&targets), (3300, 400, 5000), "game, crash reports, driver");
        // A folder that is not there is passed over.
        assert!(find(&[format!("{home}/nirgends")], &[format!("{home}/auch-nicht")]).is_empty());
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn emptying_leaves_the_folders_and_the_settings_and_counts_what_it_freed() {
        let home = scratch("clear");
        let root = tree(&home);
        let targets = find(std::slice::from_ref(&root), &[]);
        assert_eq!(clear(&targets), (3700, 0));
        assert!(Path::new(&format!("{root}/starcitizen_(sc-alpha-4.1.0)_abc/shaders")).is_dir(), "the folder stays");
        assert!(!Path::new(&format!("{root}/starcitizen_(sc-alpha-4.1.0)_abc/shaders/a.bin")).exists());
        assert!(
            !Path::new(&format!("{root}/starcitizen_(sc-alpha-4.1.0)_abc/vulkanshadercache/sub")).exists(),
            "an empty subfolder goes"
        );
        assert!(
            Path::new(&format!("{root}/starcitizen_(sc-alpha-4.1.0)_abc/GraphicsSettings/GraphicsSettings.json"))
                .is_file()
        );
        assert!(Path::new(&format!("{root}/other/shaders/d.bin")).is_file(), "what is no version stays");
        assert_eq!(total_by_kind(&find(std::slice::from_ref(&root), &[])), (0, 0, 0));
        let _ = fs::remove_dir_all(home);
    }

    /// A link from `link` to the folder `target`: a symbolic link, on Windows a junction (no right needed).
    fn link_dir(link: &str, target: &str) -> bool {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link).is_ok()
        }
        #[cfg(windows)]
        {
            std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J", &link.replace('/', "\\"), &target.replace('/', "\\")])
                .output()
                .is_ok_and(|out| out.status.success())
        }
    }

    #[test]
    fn a_folder_that_is_a_link_is_not_found_and_one_that_became_a_link_is_not_emptied() {
        let home = scratch("link");
        let root = tree(&home);
        let version = format!("{root}/starcitizen_(sc-alpha-4.1.0)_abc");
        let outside = format!("{home}/anderswo");
        put(&format!("{outside}/keep.bin"), 11);
        // `shaders` is a link to a folder somewhere else: it is not offered.
        fs::remove_dir_all(format!("{version}/shaders")).expect("remove");
        assert!(link_dir(&format!("{version}/shaders"), &outside), "this machine makes no link: the check is void");
        let targets = find(std::slice::from_ref(&root), &[]);
        assert!(
            targets.iter().all(|t| !Path::new(&t.folder).ends_with("shaders") || t.folder.contains("4.0.0")),
            "{targets:?}"
        );
        assert!(targets.iter().any(|t| t.folder.ends_with("vulkanshadercache")), "the others are there: {targets:?}");
        assert_eq!(clear(&targets), (3700 - 1000, 0), "what was offered is emptied, the link is left alone");
        assert!(Path::new(&format!("{outside}/keep.bin")).is_file(), "nothing behind the link was touched");
        // Between the question and the yes a found folder is replaced by a link: nothing is emptied there.
        let vulkan = format!("{version}/vulkanshadercache");
        put(&format!("{vulkan}/c.bin"), 20);
        put(&format!("{root}/STARCITIZEN_(sc-alpha-4.0.0)_old/shaders/c.bin"), 300);
        put(&format!("{root}/crashes/e.dmp"), 400);
        let targets = find(std::slice::from_ref(&root), &[]);
        fs::remove_dir_all(&vulkan).expect("remove");
        assert!(link_dir(&vulkan, &outside));
        assert_eq!(clear(&targets), (300 + 400, 0), "the other two are emptied, the link is not");
        assert!(Path::new(&format!("{outside}/keep.bin")).is_file(), "still nothing behind the link");
        let _ = fs::remove_dir_all(home);
    }

    #[cfg(windows)]
    #[test]
    fn a_file_in_use_stays_and_is_counted() {
        let home = scratch("busy");
        let root = tree(&home);
        let busy = format!("{root}/starcitizen_(sc-alpha-4.1.0)_abc/shaders/a.bin");
        // Opened so that nobody else may delete it, as a game that uses it does.
        use std::os::windows::fs::OpenOptionsExt;
        let _open = fs::OpenOptions::new().read(true).share_mode(0).open(&busy).expect("open");
        let targets = find(std::slice::from_ref(&root), &[]);
        let (freed, skipped) = clear(&targets);
        assert_eq!(skipped, 1, "the open file");
        assert_eq!(freed, 3700 - 1000);
        assert!(Path::new(&busy).is_file());
        drop(_open);
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn the_places_on_linux_are_the_wine_prefixes_and_on_windows_the_own_folder() {
        let home = scratch("orte");
        // Linux: the prefix of the LUG Helper with two users, one of them the public one.
        let local = format!("{home}/Games/star-citizen/drive_c/users/ich/AppData/Local");
        fs::create_dir_all(format!("{local}/Star Citizen")).expect("folder");
        fs::create_dir_all(format!("{home}/Games/star-citizen/drive_c/users/Public/AppData/Local/Star Citizen"))
            .expect("folder");
        let linux = Paths::new(Host::new(Flavor::Posix, &[("HOME", home.as_str())], &home));
        assert_eq!(local_appdata_dirs(&linux), std::slice::from_ref(&local));
        assert_eq!(sc_cache_roots(&linux), [format!("{local}/Star Citizen")]);
        // The driver caches below ~/.cache or XDG_CACHE_HOME.
        fs::create_dir_all(format!("{home}/.cache/mesa_shader_cache")).expect("folder");
        assert_eq!(gpu_cache_dirs(&linux), [format!("{home}/.cache/mesa_shader_cache")]);
        let xdg = format!("{home}/xdg");
        fs::create_dir_all(format!("{xdg}/nvidia/GLCache")).expect("folder");
        let with_xdg =
            Paths::new(Host::new(Flavor::Posix, &[("HOME", home.as_str()), ("XDG_CACHE_HOME", xdg.as_str())], &home));
        assert_eq!(gpu_cache_dirs(&with_xdg), [format!("{xdg}/nvidia/GLCache")]);
        let _ = fs::remove_dir_all(home);
    }

    #[cfg(windows)]
    #[test]
    fn on_windows_the_local_appdata_of_the_user_is_the_place() {
        let home = scratch("win");
        let local = format!("{home}\\AppData\\Local");
        fs::create_dir_all(format!("{local}\\star citizen")).expect("folder");
        fs::create_dir_all(format!("{local}\\NVIDIA\\DXCache")).expect("folder");
        fs::create_dir_all(format!("{home}\\AppData\\LocalLow\\Intel\\ShaderCache")).expect("folder");
        let windows = Paths::new(Host::new(Flavor::Windows, &[("LOCALAPPDATA", local.as_str())], &home));
        assert_eq!(local_appdata_dirs(&windows), std::slice::from_ref(&local));
        assert_eq!(sc_cache_roots(&windows), [format!("{local}\\star citizen")], "the name in any case");
        assert_eq!(
            gpu_cache_dirs(&windows),
            [format!("{local}\\NVIDIA\\DXCache"), format!("{home}\\AppData\\LocalLow\\Intel\\ShaderCache")]
        );
        let _ = fs::remove_dir_all(home);
    }

    fn installed(names: &[&str]) -> Branches {
        Branches::new(names.iter().map(|n| n.to_string()).collect(), true)
    }

    #[test]
    fn a_folder_is_old_only_when_it_is_a_numbered_version_older_than_the_newest_installed_one() {
        let branches = installed(&["sc-alpha-4.10.2", "sc-patch-inc"]);
        assert_eq!(branches.classify("starcitizen_(sc-alpha-4.10.2)_cc9wt_0"), Age::Current);
        assert_eq!(branches.classify("StarCitizen_(SC-Alpha-4.10.2)_x"), Age::Current, "any case");
        assert_eq!(branches.classify("starcitizen_(sc-alpha-4.10.0)_6kj0hb_0"), Age::Old);
        assert_eq!(branches.classify("starcitizen_(sc-alpha-4.9.9)_x"), Age::Old, "numbers, not text: 9 is below 10");
        assert_eq!(
            branches.classify("starcitizen_(sc-patch-inc)_x"),
            Age::Current,
            "an installed branch without numbers"
        );
        // Not told: a newer version, a branch without numbers that is not installed, a name that is no version.
        assert_eq!(branches.classify("starcitizen_(sc-alpha-4.11.0)_x"), Age::Unclear);
        assert_eq!(branches.classify("starcitizen_(sc-patch-other)_x"), Age::Unclear);
        assert_eq!(branches.classify("starcitizen_sc-alpha-4.1.0_x"), Age::Unclear, "no brackets");
        assert_eq!(branches.classify("starcitizen_()_x"), Age::Unclear, "no branch");
        assert_eq!(branches.classify("other"), Age::Unclear);
        // 4.10 and 4.10.0 are one version.
        assert_eq!(installed(&["sc-alpha-4.10.0"]).classify("starcitizen_(sc-alpha-4.10)_x"), Age::Unclear);
    }

    #[test]
    fn when_the_installed_versions_are_not_known_for_sure_nothing_is_old() {
        // A channel with a game whose manifest could not be read.
        let unsure = Branches::new(vec!["sc-alpha-4.10.2".to_string()], false);
        assert!(!unsure.is_sure());
        assert_eq!(unsure.classify("starcitizen_(sc-alpha-4.10.0)_x"), Age::Unclear);
        assert_eq!(unsure.classify("starcitizen_(sc-alpha-4.10.2)_x"), Age::Current, "what is installed still is");
        // No channel at all.
        assert!(!Branches::new(Vec::new(), true).is_sure());
        assert_eq!(Branches::unknown().classify("starcitizen_(sc-alpha-4.0.0)_x"), Age::Unclear);
    }

    #[test]
    fn the_branches_are_read_from_the_manifest_of_every_channel_that_holds_a_game() {
        let home = scratch("branches");
        let game = format!("{home}/Games/star-citizen/drive_c/Program Files/Roberts Space Industries/StarCitizen");
        let manifest = |branch: &str| format!(r#"{{"Data": {{"Branch": "{branch}", "Version": "4.10.197.2878"}}}}"#);
        for (channel, branch) in [("LIVE", Some("sc-alpha-4.10.2")), ("PTU", Some("sc-alpha-4.11.0"))] {
            put(&format!("{game}/{channel}/Data.p4k"), 10);
            if let Some(branch) = branch {
                fs::write(format!("{game}/{channel}/build_manifest.id"), manifest(branch)).expect("manifest");
            }
        }
        // A folder without a game (a left-over) is no installation.
        fs::create_dir_all(format!("{game}/TECH-PREVIEW")).expect("folder");
        let live = format!("{game}/LIVE");
        let vars = [("HOME", home.as_str()), (crate::paths::ENV_INSTALL_DIR, live.as_str())];
        let paths = Paths::new(Host::new(Flavor::Posix, &vars, &home));
        let read = Branches::read(&paths);
        assert!(read.is_sure(), "{read:?}");
        assert_eq!(read.classify("starcitizen_(sc-alpha-4.10.2)_a"), Age::Current);
        assert_eq!(read.classify("starcitizen_(sc-alpha-4.11.0)_a"), Age::Current);
        assert_eq!(read.classify("starcitizen_(sc-alpha-4.10.5)_a"), Age::Old, "below the newest, installed in none");
        // A channel with a game and a manifest that says nothing: unsure.
        fs::write(format!("{game}/PTU/build_manifest.id"), "{}").expect("manifest");
        assert!(!Branches::read(&paths).is_sure());
        fs::remove_file(format!("{game}/PTU/build_manifest.id")).expect("remove");
        assert!(!Branches::read(&paths).is_sure(), "a manifest that is not there");
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn the_places_group_a_version_folder_a_maker_and_the_crash_reports() {
        let home = scratch("places");
        let root = tree(&home);
        put(&format!("{root}/starcitizen_(sc-alpha-4.2.0)_new/shaders/n.bin"), 70);
        let local = format!("{home}/AppData/Local");
        put(&format!("{local}/NVIDIA/DXCache/a.bin"), 100);
        put(&format!("{local}/NVIDIA/GLCache/b.bin"), 200);
        put(&format!("{local}/Intel/ShaderCache/c.bin"), 5);
        put(&format!("{home}/AppData/LocalLow/Intel/ShaderCache/d.bin"), 6);
        put(&format!("{local}/D3DSCache/e.bin"), 7);
        put(&format!("{local}/Fremd/Cache/f.bin"), 8);
        let gpu: Vec<String> = ["NVIDIA/DXCache", "NVIDIA/GLCache", "Intel/ShaderCache", "D3DSCache", "Fremd/Cache"]
            .iter()
            .map(|p| format!("{local}/{p}"))
            .chain([format!("{home}/AppData/LocalLow/Intel/ShaderCache")])
            .collect();
        let places = find_places(std::slice::from_ref(&root), &gpu, &installed(&["sc-alpha-4.2.0"]));
        let seen: Vec<(PlaceKind, &str, Age, u64)> =
            places.iter().map(|p| (p.kind, p.version(), p.age, p.bytes)).collect();
        assert_eq!(
            seen,
            [
                (PlaceKind::Game, "4.2.0", Age::Current, 70),
                (PlaceKind::Game, "4.1.0", Age::Old, 3000),
                (PlaceKind::Game, "4.0.0", Age::Old, 300),
                (PlaceKind::Nvidia, "", Age::Unclear, 300),
                (PlaceKind::Intel, "", Age::Unclear, 11),
                (PlaceKind::DirectX, "", Age::Unclear, 7),
                (PlaceKind::OtherDriver, "", Age::Unclear, 8),
                (PlaceKind::Crash, "", Age::Unclear, 400),
            ],
            "newest version first, then the makers, the crash reports last"
        );
        let old = &places[1];
        assert_eq!(old.targets.len(), 2, "shaders and vulkanshadercache are one place");
        assert_eq!(old.suffix, "abc");
        assert!(places[0].folder.ends_with("starcitizen_(sc-alpha-4.2.0)_new"));
        assert!(places.iter().all(|p| !p.folder.contains("/other")), "a folder that is no version is no place");
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn only_old_never_touches_the_installed_version_or_what_is_no_old_version() {
        let home = scratch("only-old");
        let root = tree(&home);
        let current = format!("{root}/starcitizen_(sc-alpha-4.1.0)_abc");
        put(&format!("{root}/starcitizen_(sc-alpha-4.5.0)_newer/shaders/n.bin"), 70);
        put(&format!("{root}/starcitizen_(odd-name)_x/shaders/o.bin"), 80);
        let driver = format!("{home}/cache/nvidia/GLCache");
        put(&format!("{driver}/f.bin"), 5000);
        let branches = installed(&["sc-alpha-4.1.0"]);
        let places = find_places(std::slice::from_ref(&root), std::slice::from_ref(&driver), &branches);
        // Everything is chosen, as a click on all of them or a mistake in the page would do.
        let every: Vec<usize> = (0..places.len()).collect();
        let done = clear_places(&places, &every, Some(&branches));
        let freed: u64 = done.iter().map(|(_, c)| c.freed).sum();
        assert_eq!(freed, 300, "only the folder of 4.0.0");
        for (at, cleared) in &done {
            let place = &places[*at];
            let is_old = place.kind == PlaceKind::Game && place.age == Age::Old;
            assert_eq!(cleared.kept, !is_old, "{place:?}");
        }
        assert!(Path::new(&format!("{current}/shaders/a.bin")).is_file(), "the installed version stays whole");
        assert!(Path::new(&format!("{current}/vulkanshadercache/sub/b.bin")).is_file());
        assert!(Path::new(&format!("{current}/GraphicsSettings/GraphicsSettings.json")).is_file());
        assert!(Path::new(&format!("{root}/starcitizen_(sc-alpha-4.5.0)_newer/shaders/n.bin")).is_file(), "newer");
        assert!(Path::new(&format!("{root}/starcitizen_(odd-name)_x/shaders/o.bin")).is_file(), "no version");
        assert!(Path::new(&format!("{driver}/f.bin")).is_file(), "no driver cache");
        assert!(Path::new(&format!("{root}/crashes/e.dmp")).is_file(), "no crash reports");
        assert!(
            !Path::new(&format!("{root}/STARCITIZEN_(sc-alpha-4.0.0)_old/shaders/c.bin")).exists(),
            "the old one is gone"
        );
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn only_old_keeps_every_installed_version_also_one_below_the_newest_and_empties_only_what_lies_between() {
        let home = scratch("two-installed");
        let root = tree(&home);
        put(&format!("{root}/starcitizen_(sc-alpha-4.2.0)_new/shaders/n.bin"), 70);
        // Two channels: 4.0.0 is installed and not the newest, 4.2.0 is the newest, 4.1.0 is installed in none.
        let branches = installed(&["sc-alpha-4.0.0", "sc-alpha-4.2.0"]);
        let places = find_places(std::slice::from_ref(&root), &[], &branches);
        let ages: Vec<(&str, Age)> =
            places.iter().filter(|p| p.kind == PlaceKind::Game).map(|p| (p.version(), p.age)).collect();
        assert_eq!(ages, [("4.2.0", Age::Current), ("4.1.0", Age::Old), ("4.0.0", Age::Current)]);
        let every: Vec<usize> = (0..places.len()).collect();
        let done = clear_places(&places, &every, Some(&branches));
        assert_eq!(done.iter().map(|(_, c)| c.freed).sum::<u64>(), 3000, "only the folder of 4.1.0");
        assert!(Path::new(&format!("{root}/STARCITIZEN_(sc-alpha-4.0.0)_old/shaders/c.bin")).is_file(), "installed");
        assert!(Path::new(&format!("{root}/starcitizen_(sc-alpha-4.2.0)_new/shaders/n.bin")).is_file(), "newest");
        assert!(!Path::new(&format!("{root}/starcitizen_(sc-alpha-4.1.0)_abc/shaders/a.bin")).exists(), "between");
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn a_folder_that_became_the_installed_version_after_the_question_is_not_emptied() {
        let home = scratch("patched");
        let root = tree(&home);
        // At the question 4.1.0 is installed and 4.0.0 is old. Then a patch brings 4.0.0 (a downgrade), and the
        // folder of 4.1.0 is no more the installed one: the answer is asked again before it is emptied.
        let places = find_places(std::slice::from_ref(&root), &[], &installed(&["sc-alpha-4.1.0"]));
        let now = installed(&["sc-alpha-4.0.0"]);
        let every: Vec<usize> = (0..places.len()).collect();
        let done = clear_places(&places, &every, Some(&now));
        assert!(done.iter().all(|(_, c)| c.kept && c.freed == 0), "{done:?}");
        assert!(Path::new(&format!("{root}/STARCITIZEN_(sc-alpha-4.0.0)_old/shaders/c.bin")).is_file());
        assert!(Path::new(&format!("{root}/starcitizen_(sc-alpha-4.1.0)_abc/shaders/a.bin")).is_file());
        // The same choice for everything (no `only_old`) empties what was chosen and only that.
        let done = clear_places(&places, &[1], None);
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].1.freed, 300, "{places:?}");
        assert!(Path::new(&format!("{root}/starcitizen_(sc-alpha-4.1.0)_abc/shaders/a.bin")).is_file(), "not chosen");
        let _ = fs::remove_dir_all(home);
    }

    /// Reads the real machine and writes nothing: how the folders of the player are judged. Run by hand with
    /// `cargo test -p versekit-core real_machine -- --ignored --nocapture`.
    #[test]
    #[ignore = "reads the real machine"]
    fn real_machine_read_only() {
        let paths = Paths::new(Host::current());
        let branches = Branches::read(&paths);
        let places = find_places(&sc_cache_roots(&paths), &gpu_cache_dirs(&paths), &branches);
        println!("{branches:?}");
        for p in &places {
            println!("{:?} {} {} {:?} {} bytes", p.kind, p.version(), p.suffix, p.age, p.bytes);
        }
    }

    #[test]
    fn sizes_read_as_megabytes_or_gigabytes_with_the_decimal_sign_given() {
        assert_eq!(format_size(740 * 1024 * 1024, ','), "740 MB");
        assert_eq!(format_size(15_990_000_000, ','), "14,9 GB");
        assert_eq!(format_size(15_990_000_000, '.'), "14.9 GB");
        assert_eq!(format_size(0, ','), "0 MB");
        // Python rounds half to even: 2.5 MB gives 2, 3.5 MB gives 4.
        assert_eq!(format_size(2 * 1024 * 1024 + 512 * 1024, ','), "2 MB");
        assert_eq!(format_size(3 * 1024 * 1024 + 512 * 1024, ','), "4 MB");
    }
}
