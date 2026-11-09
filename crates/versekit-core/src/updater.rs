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

//! Noticing new versions and fetching them (4.12 `updater`).
//!
//! Three steps: ask GitHub which releases there are (remembered in `versionen.json`), pick the one to offer,
//! and fetch the file that fits this packaging. ⚠ A file is fetched only from a release of this project on
//! `github.com`, and only with the checksum the release publishes in `SHA256SUMS.txt`: no checksum, no update.
//! What happens with the file afterwards is the business of `update_run` (the installer under Windows) and
//! `update_swap` (replacing the AppImage under Linux).

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use ring::digest;
use serde_json::{Value, json};

use crate::language::{self, Arg};
use crate::legacy;
use crate::net::{FetchError, Net};
use crate::paths::Paths;
use crate::text::{py_isspace, py_strip};
use crate::version::{is_newer, is_prerelease, parts, prerelease_number};
use crate::{errors, jsonio};

/// The repository the releases come from.
pub const REPO: &str = "Xharig/VerseKit";
/// ⚠ Not `/releases/latest`: GitHub leaves the pre-releases out there, and every test version is one.
/// 100 is the most one request returns; with 20 there was often not a single finished version among them.
pub const API: &str = "https://api.github.com/repos/Xharig/VerseKit/releases?per_page=100";
/// The repository of the test versions: binaries only, each release a normal one.
pub const TEST_REPO: &str = "Xharig/VerseKit-Testfassungen";
/// The releases of the test versions.
pub const TEST_API: &str = "https://api.github.com/repos/Xharig/VerseKit-Testfassungen/releases?per_page=100";

/// Where an own version asks for releases: a test version at the test versions and at the main repository, so
/// that the finished version reaches it too; every other version only at the main repository.
pub fn sources(own: &str) -> &'static [&'static str] {
    if is_prerelease(own) { &[TEST_API, API] } else { &[API] }
}
/// The page with all releases, for fetching by hand.
pub const PAGE: &str = "https://github.com/Xharig/VerseKit/releases";
/// What the servers see of the program.
pub const USER_AGENT: &str = crate::net::USER_AGENT;
/// The remembered releases, in `Intern`.
pub const CACHE: &str = "versionen.json";
/// How long one look at GitHub holds, in seconds: at most six requests an hour, far below the 60 GitHub allows
/// without signing in.
pub const MIN_INTERVAL: f64 = 600.0;
/// The setting that offers test versions too.
pub const PRERELEASE_SETTING: &str = "vorabversionen";
/// The file of a release with the checksums.
pub const CHECKSUM_FILE: &str = "SHA256SUMS.txt";
/// The largest checksum file read; it holds a few hundred bytes.
pub const CHECKSUM_LIMIT: usize = 64 * 1024;
/// The endings of the files an update may fetch at all.
pub const ALLOWED_SUFFIXES: [&str; 2] = [".appimage", ".exe"];
/// The installer under Windows. ⚠ `-setup.exe` stays: older versions look for exactly that ending.
pub const WINDOWS_INSTALLER: [&str; 3] = ["-setup.exe", "-installer.exe", "_setup.exe"];
/// What a file name of this program contains (lower case), the old name and the new one.
pub const OWN_FILENAMES: [&str; 2] = [legacy::OWN_FILENAME, "versekit"];

// ------------------------------------------------------------ releases

/// One file of a release.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: Option<u64>,
}

/// One release as remembered (the fields of `versionen.json`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Release {
    /// The tag, `v5.0.0` or `v5.0.0-rc1`.
    pub version: String,
    pub name: String,
    /// The day it was published, `2026-11-15`.
    pub date: String,
    /// The moment it was published, `2026-11-15T18:00:00Z`.
    pub time: String,
    pub text: String,
    pub prerelease: bool,
    pub assets: Vec<Asset>,
}

/// What `versionen.json` holds: when GitHub was asked last, and what it said.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cache {
    pub checked: f64,
    pub releases: Vec<Release>,
}

fn text_of(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

/// The releases in an answer of the GitHub API, drafts left out. `None` if it is no list.
pub fn releases_from_api(body: &[u8]) -> Option<Vec<Release>> {
    let list: Vec<Value> = serde_json::from_slice(body).ok()?;
    Some(
        list.iter()
            .filter(|r| !r.get("draft").and_then(Value::as_bool).unwrap_or(false))
            .map(|r| {
                let time = text_of(r, "published_at");
                Release {
                    version: text_of(r, "tag_name"),
                    name: text_of(r, "name"),
                    date: time.chars().take(10).collect(),
                    time,
                    text: text_of(r, "body"),
                    prerelease: r.get("prerelease").and_then(Value::as_bool).unwrap_or(false),
                    assets: r
                        .get("assets")
                        .and_then(Value::as_array)
                        .map(|list| {
                            list.iter()
                                .map(|a| Asset {
                                    name: text_of(a, "name"),
                                    url: text_of(a, "browser_download_url"),
                                    size: a.get("size").and_then(Value::as_u64),
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                }
            })
            .collect(),
    )
}

fn cache_from_value(value: &Value) -> Cache {
    let releases = value
        .get("freigaben")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .map(|r| Release {
                    version: text_of(r, "version"),
                    name: text_of(r, "name"),
                    date: text_of(r, "datum"),
                    time: text_of(r, "zeit"),
                    text: text_of(r, "text"),
                    prerelease: r.get("vorab").and_then(Value::as_bool).unwrap_or(false),
                    assets: r
                        .get("dateien")
                        .and_then(Value::as_array)
                        .map(|list| {
                            list.iter()
                                .map(|a| Asset {
                                    name: text_of(a, "name"),
                                    url: text_of(a, "url"),
                                    size: a.get("groesse").and_then(Value::as_u64),
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();
    Cache { checked: value.get("geprueft").and_then(Value::as_f64).unwrap_or(0.0), releases }
}

fn cache_to_value(cache: &Cache) -> Value {
    json!({
        "geprueft": cache.checked,
        "freigaben": cache.releases.iter().map(|r| json!({
            "version": r.version,
            "name": r.name,
            "datum": r.date,
            "zeit": r.time,
            "text": r.text,
            "vorab": r.prerelease,
            "dateien": r.assets.iter().map(|a| json!({"name": a.name, "url": a.url, "groesse": a.size})).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

/// The remembered releases; empty if there are none or the file is broken.
pub fn cache_read(paths: &Paths) -> Cache {
    fs::read(paths.app_file(CACHE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .map(|value| cache_from_value(&value))
        .unwrap_or_default()
}

/// Remember the releases.
///
/// # Errors
///
/// When the file cannot be written.
pub fn cache_write(paths: &Paths, cache: &Cache) -> io::Result<()> {
    let text = serde_json::to_string(&cache_to_value(cache)).map_err(io::Error::other)?;
    jsonio::write_text_atomic(&paths.app_file(CACHE), &text, jsonio::Newline::Keep)
}

/// Seconds since 1970.
pub fn now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

/// What a look at GitHub found.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Checked {
    /// The release to offer, if one is newer.
    pub offer: Option<Release>,
    /// Whether GitHub was asked this time: `None` not asked (the last look is recent enough, or no net),
    /// `Some(false)` asking failed.
    pub fetched: Option<bool>,
    /// Asking failed at the hourly limit of GitHub.
    pub rate_limited: bool,
}

/// Is there something new for `own`? GitHub is asked at most every [`MIN_INTERVAL`] seconds (`force` asks
/// anyway); in between, and without the net, what is remembered counts. ⚠ `SC_BP_NO_NET` forbids asking, not
/// showing what is already known.
pub fn check(net: &dyn Net, paths: &Paths, own: &str, force: bool, at: f64) -> Checked {
    let mut cache = cache_read(paths);
    let mut checked = Checked::default();
    let old_enough = at - cache.checked > MIN_INTERVAL;
    if !crate::net::offline_in(&paths.host) && (force || old_enough) {
        let mut found: Option<Vec<Release>> = None;
        let mut failed = None;
        for source in sources(own) {
            match net
                .get(source)
                .map_err(|e| e.to_string())
                .and_then(|body| releases_from_api(&body).ok_or_else(|| "the answer is no list of releases".to_string()))
            {
                Ok(releases) => found.get_or_insert_with(Vec::new).extend(releases),
                Err(error) => {
                    errors::record("updater.check", &error);
                    failed = Some(error);
                }
            }
        }
        if let Some(releases) = found {
            cache = Cache { checked: at, releases };
            if let Err(error) = cache_write(paths, &cache) {
                errors::record("updater.cache_write", &error);
            }
            checked.fetched = Some(true);
        } else if let Some(error) = failed {
            checked.fetched = Some(false);
            checked.rate_limited = error.contains("403")
                || error.to_lowercase().contains("rate limit")
                || error == FetchError::Refused.to_string();
        }
    }
    let with_prerelease = is_prerelease(own) || paths.setting_bool(PRERELEASE_SETTING, false);
    checked.offer = best_newer(&cache.releases, own, with_prerelease).cloned();
    checked
}

/// The remembered release newer than `own` that is offered, without asking GitHub (the update button asks this
/// often; the same choice as [`check`]).
pub fn known_newer(paths: &Paths, own: &str) -> Option<Release> {
    let with_prerelease = is_prerelease(own) || paths.setting_bool(PRERELEASE_SETTING, false);
    best_newer(&cache_read(paths).releases, own, with_prerelease).cloned()
}

/// The highest release newer than `own`; pre-releases only `with_prerelease`. ⚠ The highest, not the first:
/// GitHub sorts by the time of the tag, and `rc10` can stand behind `rc9`.
pub fn best_newer<'a>(releases: &'a [Release], own: &str, with_prerelease: bool) -> Option<&'a Release> {
    let mut best: Option<&Release> = None;
    for release in releases {
        if release.version.is_empty() || (is_test(release) && !with_prerelease) || !is_newer(&release.version, own) {
            continue;
        }
        if best.is_none_or(|b| is_newer(&release.version, &b.version)) {
            best = Some(release);
        }
    }
    best
}

/// A test version: marked as pre-release, or its tag says so. ⚠ The releases of the test repository are not
/// marked as pre-release, only their tag (`-rc3`) tells.
fn is_test(release: &Release) -> bool {
    release.prerelease || is_prerelease(&release.version)
}

/// The newest release of a channel, whatever runs: from a test version the way back to the finished one stays
/// open.
pub fn latest(releases: &[Release], with_prerelease: bool) -> Option<&Release> {
    let mut best: Option<&Release> = None;
    for release in releases {
        if release.version.is_empty() || (is_test(release) && !with_prerelease) {
            continue;
        }
        if best.is_none_or(|b| is_newer(&release.version, &b.version)) {
            best = Some(release);
        }
    }
    best
}

/// One key per version: `-rc1` and `-rc13` differ, the finished version stands above its pre-releases.
pub fn version_key(version: &str) -> (u64, u64, u64, u64) {
    let (major, minor, patch) = parts(version);
    let step = if is_prerelease(version) { prerelease_number(version) } else { 9999 };
    (major, minor, patch, step)
}

/// A version as the player reads it: without the `v` of the tag.
pub fn shown_version(version: &str) -> &str {
    version.strip_prefix(['v', 'V']).unwrap_or(version)
}

// ------------------------------------------------------------ the file

/// How this program runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Packaging {
    /// The program file under Windows; updated by the installer.
    Exe,
    /// An AppImage under Linux; replaced in place.
    AppImage,
    /// Built from the source: no update replaces it.
    Source,
}

/// Does this file belong to this program (by its name: the old one or the new one)?
pub fn is_ours(path: &str) -> bool {
    let name = path.replace('\\', "/");
    let name = name.rsplit('/').next().unwrap_or("").to_lowercase();
    OWN_FILENAMES.iter().any(|own| name.contains(own))
}

/// The AppImage this program runs from, from the variable `APPIMAGE`. ⚠ The variable alone is not enough: it is
/// in the environment of everything started from any AppImage. Only a file named after this program counts.
pub fn own_appimage(appimage: Option<&str>) -> Option<PathBuf> {
    let path = appimage.filter(|p| !p.is_empty())?;
    (Path::new(path).is_file() && is_ours(path)).then(|| PathBuf::from(path))
}

/// How this program runs: an own AppImage, else a release build (a program file), else a build of the source.
/// A build with a pre-release mark `-dev` counts as the source, which `git pull` updates.
pub fn packaging(appimage: Option<&str>, own_version: &str) -> Packaging {
    if own_appimage(appimage).is_some() {
        Packaging::AppImage
    } else if own_version.contains("-dev") || cfg!(not(windows)) {
        Packaging::Source
    } else {
        Packaging::Exe
    }
}

/// The file of a release that fits the packaging. Under Windows only the installer: without one, no update
/// rather than the wrong one.
pub fn matching_asset(release: &Release, kind: Packaging) -> Option<&Asset> {
    let endings: &[&str] = match kind {
        Packaging::Source => return None,
        Packaging::AppImage => &[".appimage"],
        Packaging::Exe => &WINDOWS_INSTALLER,
    };
    release.assets.iter().find(|asset| {
        let name = asset.name.to_lowercase();
        endings.iter().any(|ending| name.ends_with(ending)) && url_ok(&asset.url)
    })
}

/// Does the path of a release file fit: `/<repo>/releases/(download/)<tag>/<file>`, the repository under its
/// new or old name?
fn release_path(path: &str) -> bool {
    let rest = [REPO, TEST_REPO, legacy::OLD_REPO]
        .iter()
        .find_map(|repo| path.strip_prefix(&format!("/{}/releases/", repo.to_lowercase())));
    let Some(rest) = rest else { return false };
    let rest = rest.strip_prefix("download/").filter(|r| r.split('/').count() == 2).unwrap_or(rest);
    let mut pieces = rest.split('/');
    matches!((pieces.next(), pieces.next(), pieces.next()), (Some(tag), Some(file), None) if !tag.is_empty() && !file.is_empty())
}

/// Only release files of this project on `github.com`, whatever an answer or the cache claims. `..` and
/// percent signs could lead the path on the server into another repository.
pub fn url_ok(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once("://") else { return false };
    if !scheme.eq_ignore_ascii_case("https") {
        return false;
    }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, after) = rest.split_at(end);
    let host = authority.rsplit('@').next().unwrap_or("");
    let host = match host.rsplit_once(':') {
        Some((name, port)) if port.bytes().all(|b| b.is_ascii_digit()) => name,
        _ => host,
    };
    if !host.eq_ignore_ascii_case("github.com") {
        return false;
    }
    let path = after.split(['?', '#']).next().unwrap_or("").to_lowercase();
    !path.contains("..") && !path.contains('%') && release_path(&path)
}

/// A harmless file name from the name of a release file: the bare name, no separators, no dot names, and
/// only the endings this program fetches. ⚠ The name comes from the server's answer: as part of a path
/// `../../x.exe` would put the file somewhere else. `None` when nothing harmless is left; no stand-in name,
/// so that a safety check can never rest on one.
pub fn safe_filename(name: &str) -> Option<String> {
    let raw = py_strip(name).replace('\\', "/");
    let raw = raw.trim_end_matches('/');
    let raw = raw.rsplit('/').next().unwrap_or("");
    let lower = raw.to_lowercase();
    (!matches!(raw, "" | "." | "..") && ALLOWED_SUFFIXES.iter().any(|s| lower.ends_with(s))).then(|| raw.to_string())
}

/// The SHA-256 of a file in lower-case hex, read in pieces (an AppImage is large).
///
/// # Errors
///
/// When the file cannot be read.
pub fn checksum_of(path: &Path) -> io::Result<String> {
    checksum_of_reader(&mut fs::File::open(path)?)
}

/// The SHA-256 of what `reader` holds from where it stands, in lower-case hex.
///
/// # Errors
///
/// When it cannot be read.
pub fn checksum_of_reader(reader: &mut dyn Read) -> io::Result<String> {
    let mut context = digest::Context::new(&digest::SHA256);
    let mut block = vec![0u8; 1024 * 1024];
    loop {
        let count = reader.read(&mut block)?;
        if count == 0 {
            break;
        }
        context.update(&block[..count]);
    }
    Ok(context.finish().as_ref().iter().map(|b| format!("{b:02x}")).collect())
}

/// `SHA256SUMS.txt` as `{file name: checksum}`, in the format of `sha256sum`. ⚠ A path in a name or a name
/// twice makes the whole file invalid: an empty table, and empty means no installation.
pub fn parse_checksums(text: &str) -> BTreeMap<String, String> {
    let mut table = BTreeMap::new();
    for line in crate::text::py_splitlines(text) {
        let line = py_strip(line);
        let Some(split) = line.find(py_isspace) else { continue };
        let checksum = line[..split].to_lowercase();
        let name = py_strip(&line[split..]).trim_start_matches('*');
        if name.is_empty() || checksum.len() != 64 || !checksum.bytes().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        if name.contains(['/', '\\']) || table.contains_key(name) {
            return BTreeMap::new();
        }
        table.insert(name.to_string(), checksum);
    }
    table
}

/// Why the checksums of a release are not there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChecksumProblem {
    /// The release has no checksum file.
    Missing,
    /// The checksum file is to come from somewhere else than this project's releases.
    Foreign,
    /// It could not be fetched.
    Net,
    /// The server answered with an error of its own (HTTP 5xx, the status): GitHub is disturbed, the net is not.
    Server(u16),
}

/// The status of an answer that is an error of the server itself (HTTP 500 to 599), else `None`.
pub fn server_status(error: &FetchError) -> Option<u16> {
    let FetchError::Failed(text) = error else { return None };
    text.strip_prefix("HTTP ").and_then(|code| code.trim().parse::<u16>().ok()).filter(|code| (500..600).contains(code))
}

/// The checksums of a release.
///
/// # Errors
///
/// Why there are none: a missing file is something else than a missing net.
pub fn fetch_checksums(net: &dyn Net, release: &Release) -> Result<BTreeMap<String, String>, ChecksumProblem> {
    let Some(asset) = release.assets.iter().find(|a| a.name == CHECKSUM_FILE) else {
        return Err(ChecksumProblem::Missing);
    };
    if !url_ok(&asset.url) {
        return Err(ChecksumProblem::Foreign);
    }
    match net.get(&asset.url) {
        Ok(body) => Ok(parse_checksums(&String::from_utf8_lossy(&body[..body.len().min(CHECKSUM_LIMIT)]))),
        Err(error) => {
            errors::record("updater.fetch_checksums", &error);
            Err(server_status(&error).map_or(ChecksumProblem::Net, ChecksumProblem::Server))
        }
    }
}

/// Why no file was fetched. Each case has its own sentence for the player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateError {
    /// The file is to come from somewhere else than this project's releases.
    ForeignSource,
    /// The file name is not one an update fetches (the name).
    ForeignFile(String),
    /// No checksum for the file, and why.
    NoChecksum(ChecksumProblem),
    /// The fetched file does not match the published checksum; it is thrown away.
    ChecksumWrong,
    /// Fetching failed.
    Fetch(String),
    /// The server answered the file with an error of its own (HTTP 5xx, the status).
    Server(u16),
    /// Writing the file failed.
    Disk(String),
}

impl UpdateError {
    /// The sentence for the player, in the language of the window.
    pub fn message(&self) -> String {
        match self {
            Self::ForeignSource => language::t("up_fremde_quelle"),
            Self::ForeignFile(name) => language::tf("up_fremde_datei", &[("0", Arg::from(name.clone()))]),
            Self::NoChecksum(ChecksumProblem::Net) => language::t("up_summen_netz"),
            Self::NoChecksum(ChecksumProblem::Foreign) => language::t("up_summen_fremd"),
            Self::NoChecksum(ChecksumProblem::Missing) => language::t("up_keine_summen"),
            Self::NoChecksum(ChecksumProblem::Server(code)) | Self::Server(code) => {
                language::tf("up_github_gestoert", &[("0", Arg::Int(i64::from(*code)))])
            }
            Self::ChecksumWrong => language::t("up_summe_falsch"),
            Self::Fetch(what) | Self::Disk(what) => what.clone(),
        }
    }

    /// The reason in English, for the update log next to the installer's lines.
    pub fn log_text(&self) -> String {
        match self {
            Self::ForeignSource => "the file is to come from elsewhere".to_string(),
            Self::ForeignFile(name) => format!("file name not allowed: {name}"),
            Self::NoChecksum(ChecksumProblem::Missing) => "no checksum published".to_string(),
            Self::NoChecksum(ChecksumProblem::Foreign) => "checksum file from elsewhere".to_string(),
            Self::NoChecksum(ChecksumProblem::Net) => "checksums not loadable (no connection)".to_string(),
            Self::NoChecksum(ChecksumProblem::Server(code)) => format!("checksums: GitHub answered HTTP {code}"),
            Self::Server(code) => format!("GitHub answered HTTP {code}"),
            Self::ChecksumWrong => "checksum does not match".to_string(),
            Self::Fetch(what) => format!("download failed: {what}"),
            Self::Disk(what) => format!("file not writable: {what}"),
        }
    }
}

/// A fetched file and the published checksum it was checked against. The checksum goes on to the step that
/// installs: the file may be replaced in between, so it is compared again right before.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Downloaded {
    pub path: PathBuf,
    pub checksum: String,
}

/// The folder under `%LOCALAPPDATA%` the installer of an update is fetched to and started from.
pub const INSTALLER_FOLDER: [&str; 2] = ["VerseKit", "Update"];
/// The folder in the data folder that takes its place where `%LOCALAPPDATA%` is not set.
pub const INSTALLER_FOLDER_IN_DATA: &str = "Update";

/// The folder the installer of an update is fetched to and started from: [`INSTALLER_FOLDER`] under
/// `%LOCALAPPDATA%`, else [`INSTALLER_FOLDER_IN_DATA`] in the data folder. ⚠ Never the temporary folder: a
/// program file started from there is what virus scanners stop as malware.
pub fn installer_folder(paths: &Paths) -> PathBuf {
    match paths.host.var("LOCALAPPDATA").filter(|v| !v.is_empty()) {
        Some(local) => INSTALLER_FOLDER.iter().fold(PathBuf::from(local), |path, part| path.join(part)),
        None => Path::new(&paths.app_folder()).join(INSTALLER_FOLDER_IN_DATA),
    }
}

/// Where the new version is fetched to. Windows: in `installers` ([`installer_folder`]). Linux: next to the
/// running AppImage as `.<name>.neu`, so that replacing it stays within one file system and is atomic; the
/// temporary folder only if that folder cannot be written.
pub fn download_target(
    clean_name: &str,
    kind: Packaging,
    appimage: Option<&Path>,
    temp: &Path,
    installers: &Path,
) -> PathBuf {
    if kind == Packaging::AppImage
        && let Some(folder) = appimage.and_then(Path::parent)
        && writable(folder)
    {
        return folder.join(format!(".{clean_name}.neu"));
    }
    if kind == Packaging::Exe {
        return installers.join(clean_name);
    }
    temp.join(clean_name)
}

/// Can files be created in this folder? Tried with a file that goes again at once.
fn writable(folder: &Path) -> bool {
    let probe = folder.join(format!(".versekit-probe-{}", std::process::id()));
    let ok = fs::File::create(&probe).is_ok();
    let _ = fs::remove_file(&probe);
    ok
}

/// Fetch the file of `asset` from `release` to `target` and compare it with the published checksum.
/// `progress` hears the percentage.
///
/// ⚠ The checksums come first: without them the download would be in vain. From the first written byte to
/// the checksum that matches, every failure removes the file again: a fragment must never stay, under Linux
/// it would lie next to the running AppImage.
///
/// # Errors
///
/// See [`UpdateError`].
pub fn download(
    net: &dyn Net,
    release: &Release,
    asset: &Asset,
    target: &Path,
    progress: &mut dyn FnMut(u8),
) -> Result<Downloaded, UpdateError> {
    if !url_ok(&asset.url) {
        return Err(UpdateError::ForeignSource);
    }
    let Some(name) = safe_filename(&asset.name) else {
        return Err(UpdateError::ForeignFile(if asset.name.is_empty() { "?".into() } else { asset.name.clone() }));
    };
    let table = fetch_checksums(net, release).map_err(UpdateError::NoChecksum)?;
    let Some(expected) = table.get(&name).cloned() else {
        return Err(UpdateError::NoChecksum(ChecksumProblem::Missing));
    };
    let result = fetch_and_verify(net, &asset.url, target, &expected, progress);
    if result.is_err() {
        discard(target);
    }
    result.map(|()| Downloaded { path: target.to_path_buf(), checksum: expected })
}

fn fetch_and_verify(
    net: &dyn Net,
    url: &str,
    target: &Path,
    expected: &str,
    progress: &mut dyn FnMut(u8),
) -> Result<(), UpdateError> {
    {
        if let Some(folder) = target.parent() {
            fs::create_dir_all(folder).map_err(|e| UpdateError::Disk(e.to_string()))?;
        }
        let mut file = fs::File::create(target).map_err(|e| UpdateError::Disk(e.to_string()))?;
        let mut shown = None;
        // ⚠ The display must not end the download: a percentage is a side matter.
        let mut tell = |written: u64, total: Option<u64>| {
            if let Some(total) = total.filter(|t| *t > 0) {
                let percent = u8::try_from((written.saturating_mul(100) / total).min(100)).unwrap_or(100);
                if shown != Some(percent) {
                    shown = Some(percent);
                    progress(percent);
                }
            }
        };
        net.download(url, &mut file, &mut tell)
            .map_err(|e| server_status(&e).map_or_else(|| UpdateError::Fetch(e.to_string()), UpdateError::Server))?;
        file.flush().map_err(|e| UpdateError::Disk(e.to_string()))?;
    }
    let actual = checksum_of(target).map_err(|e| UpdateError::Disk(e.to_string()))?;
    if actual != expected.to_lowercase() {
        return Err(UpdateError::ChecksumWrong);
    }
    Ok(())
}

/// Remove an unchecked update file; never fails loudly.
pub fn discard(path: &Path) {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => errors::record("updater.discard", &error),
        _ => {}
    }
}

/// Does `path` still carry exactly the checksum it was checked against?
pub fn still_matches(path: &Path, checksum: &str) -> bool {
    !checksum.is_empty() && checksum_of(path).is_ok_and(|actual| actual == checksum.to_lowercase())
}

/// The environment for programs this one starts after an update: without the library paths of a running
/// AppImage (an `*_ORIG` value takes its place, as AppImage keeps the value of before there; without one, under
/// Linux the value of the user stays, less its folders inside the mount of the AppImage, and under Windows it
/// goes), without the variables of the AppImage itself and without the switches of the update helper of 5.2.2
/// and earlier and of the replacing start.
pub fn clean_environment(vars: impl IntoIterator<Item = (String, String)>) -> Vec<(String, String)> {
    clean_environment_with(vars, cfg!(unix))
}

/// [`clean_environment`]; `keep_user_paths`: a library path without `*_ORIG` keeps its folders outside the mount.
fn clean_environment_with(
    vars: impl IntoIterator<Item = (String, String)>,
    keep_user_paths: bool,
) -> Vec<(String, String)> {
    const LIBRARY_PATHS: [&str; 11] = [
        "LD_LIBRARY_PATH",
        "PYTHONHOME",
        "PYTHONPATH",
        "PYTHONDONTWRITEBYTECODE",
        "QT_PLUGIN_PATH",
        "GTK_PATH",
        "GDK_PIXBUF_MODULE_FILE",
        "GI_TYPELIB_PATH",
        "XDG_DATA_DIRS",
        "PERLLIB",
        "GSETTINGS_SCHEMA_DIR",
    ];
    const APPIMAGE: [&str; 4] = ["APPIMAGE", "APPDIR", "OWD", "ARGV0"];
    let vars: Vec<(String, String)> = vars.into_iter().collect();
    let find = |name: &str| vars.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.clone());
    let mount = find("APPDIR").filter(|v| !v.is_empty());
    let mut out: Vec<(String, String)> = vars
        .iter()
        .filter(|(k, _)| {
            let upper = k.to_uppercase();
            !upper.starts_with("_PYI_")
                && !upper.starts_with("_MEI")
                && !upper.starts_with(crate::update_run::ENV_PREFIX)
                && upper != crate::update_swap::REPLACES_PID
                && upper != "__COMPAT_LAYER"
                && !APPIMAGE.contains(&upper.as_str())
                && !LIBRARY_PATHS.iter().any(|n| upper == *n || upper == format!("{n}_ORIG"))
        })
        .cloned()
        .collect();
    for name in LIBRARY_PATHS {
        let value = match find(&format!("{name}_ORIG")) {
            Some(original) => original,
            None if keep_user_paths => find(name).map(|v| outside_mount(&v, mount.as_deref())).unwrap_or_default(),
            None => String::new(),
        };
        if !value.is_empty() {
            out.push((name.to_string(), value));
        }
    }
    out
}

/// The folders of the list `value` (separated by `:`) that do not lie inside the mount of a running AppImage:
/// `mount` (its `APPDIR`), or a folder `.mount_*` as the runtime of the AppImage makes it.
fn outside_mount(value: &str, mount: Option<&str>) -> String {
    let mount = mount.map(|m| m.trim_end_matches('/')).filter(|m| !m.is_empty());
    let inside = |folder: &str| {
        mount.is_some_and(|m| folder.trim_end_matches('/') == m || folder.starts_with(&format!("{m}/")))
            || folder.contains("/.mount_")
    };
    value.split(':').filter(|folder| !inside(folder)).collect::<Vec<_>>().join(":")
}

#[cfg(test)]
mod tests;
