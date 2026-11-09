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

//! Removes the user's own data when the program is uninstalled ([`SWITCH`]), and names the places first
//! ([`PLACES_SWITCH`]) so the uninstaller can show them in its question.
//!
//! The places:
//!
//! 1. the data folders the program named itself: the default folders under Documents (the one of older
//!    installations and the new one) and, on a copy of the data, its data folder;
//! 2. the data folder a pointer file names, only when it holds a mark of the program ([`marked`]) and does not lie
//!    on the network (that one is only named, [`Kind::Network`]);
//! 3. the configuration folder next to the second pointer, the folder of the stored secrets (not when it is named
//!    by the environment variable) and the folder the installers of updates are fetched to.
//!
//! ⚠⚠ Only what is known to be the program's goes, also in a data folder the user chose (Documents, the desktop,
//! the root of a disk). A folder a pointer names without a mark is left as it is. In a data folder only files
//! in the exact names and forms the program gives them go ([`owned_named`], [`owned_in_common`], the files of
//! [`INNER_FOLDERS`]), each only in the subfolder the program puts it in, and where a name alone proves nothing,
//! only with the program's content; loose files at the top always stay. A link or a
//! junction is never followed and never removed. A folder goes only when it is empty afterwards and the program
//! named it: an own subfolder, a default folder, a folder of the program; never a data folder the user chose.
//!
//! Nothing here creates a folder or writes into the data: the data folder is found without `Paths::app_folder`,
//! which would create it.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::LazyLock;

use crate::basetool::secrets;
use crate::jsonio::Value;
use crate::paths::{COPY_DATA_FOLDER, NEW_FOLDER_NAME, Paths, SETTINGS_FILE};
use crate::{jsonio, legacy, updater, writeguard};

/// The switch of the program that removes the own data and ends; the uninstaller passes it.
pub const SWITCH: &str = "--eigene-daten-loeschen";
/// The switch that writes the places [`SWITCH`] would clear into the file named after it, one per line, and ends.
pub const PLACES_SWITCH: &str = "--eigene-daten-orte";

/// The own subfolders of a data folder that hold the program's files by name ([`owned_named`]) and the folders it
/// makes in `Intern` ([`INNER_FOLDERS`]). Everything else in them stays.
pub const NAMED_FOLDERS: [&str; 4] = ["Bauplaene", "Einstellungen", "Intern", "Diagnose"];

/// The own subfolders with a common name. In each only the files the program writes there go
/// ([`owned_in_common`]); everything else stays.
pub const COMMON_FOLDERS: [&str; 3] =
    [crate::patch_changes::FOLDER, crate::export::FOLDER, crate::first_backup::FOLDER];

/// The subfolder of `Intern` every name the program writes there lies in.
const INNER_PARENT: &str = "Intern";

/// The folders the program makes in `Intern`, each with the form of the names of the files it writes into it
/// (lower case): the backups before an update (`backup::write_before_update`), the taught images of the
/// signature scanner (`signature_scan::save_sample`), the pictures kept for later (`signature_watch::keep_pending`),
/// the pictures of handles (`handle_query::pictures::file_name`) and the kept pictures of the refinery
/// (`refinery_scan::screen::keep`).
pub const INNER_FOLDERS: [(&str, &str); 5] = [
    (crate::backup::BEFORE_UPDATE_FOLDER, r"^vor-update-\d{4}-\d{2}-\d{2}-\d{6}-[^\\/]+\.zip(\.teil)?$"),
    (crate::signature_scan::SAMPLE_FOLDER, r"^\d+_\d{8}-\d{6}-\d{3}\.png$"),
    (crate::signature_watch::PENDING_FOLDER, r"^\d{16}\.bin$"),
    (crate::handle_query::pictures::FOLDER, r"^[0-9a-f]{16}\.bild$"),
    (crate::refinery_scan::screen::KEEP_FOLDER, r"^raffinerie_\d{8}-\d{6}-\d{3}_[a-z0-9-]+\.png$"),
];

/// The files the program keeps in its own subfolders ([`NAMED_FOLDERS`]), in lower case. With each go the copies
/// it makes beside it (`<stem>.bak.json`, `<stem>.kaputt-<time>.json`, `<name>.tmp`, see [`known_file`]). The
/// test `every_file_name_of_the_program_is_known_or_named_as_no_data` holds every file name of the source to it.
pub const KNOWN_FILES: &[&str] = &[
    "absturz-anlernen-fassung.txt",
    "absturz-anlernen-letzter-fassung.txt",
    "absturz-anlernen-letzter.txt",
    "absturz-anlernen.txt",
    "absturz-fassung.txt",
    "absturz-letzter-fassung.txt",
    "absturz-letzter.txt",
    "absturz-overlay-fassung.txt",
    "absturz-overlay-letzter-fassung.txt",
    "absturz-overlay-letzter.txt",
    "absturz-overlay.txt",
    "absturz.txt",
    "aktionsnamen.json",
    "anti-afk.json",
    "asop.json",
    "auftraege-ausgeblendet.json",
    "auftragslog.bak.json",
    "auftragslog.json",
    "auftragsruf.json",
    "basetool-anfragen.log",
    "bauplaene-gesehen.json",
    "bergung.json",
    "bericht.txt",
    "bestand.bak.json",
    "bestand.json",
    "beziehungen.json",
    "bp-contracts-de.json",
    "bp-contracts-en.json",
    "bp-overrides.json",
    "catalog-seen.json",
    "cig-datenfehler.json",
    "contested.json",
    "crafting-blueprints.json",
    "einkaufskorb.json",
    "einstellungen.json",
    "erkul-schiffe.json",
    "erkul-teile.json",
    "erststart-sicherung.json",
    "fehler.json",
    "fremde-funde.json",
    "gesehen.json",
    "grafik.json",
    "handelslager.json",
    "handle-cache.json",
    "hangar.json",
    "hochwasser.json",
    "injektion-urtext.json",
    "joystick-saetze.json",
    "katalog-cache.json",
    "katalog-spieldaten.json",
    "konten-je-log.json",
    "laeden-katalog.json",
    "laeden.json",
    "lager-katalog.json",
    "lager-umzug.json",
    "lagerjournal.json",
    "lagervorschlaege.json",
    "logstand.json",
    "merkliste.json",
    "mining-data.json",
    "namen-original.json",
    "namen-rueckweg.json",
    "neustart-ausgabe.txt",
    "notizen.json",
    "org-mitglieder-cache.json",
    "orte-eigene.json",
    "orte-gelernt.json",
    "orte-namen.json",
    "orte-original.json",
    "orte.json",
    "overlay-steuerung.txt",
    "patch-historie.json",
    "patches-gelesen.json",
    "phrasen.json",
    "preise.json",
    "raffinerie-auftraege.json",
    "raffinerie-lesung.json",
    "raffinerie-verfahren-spieldaten.json",
    "raffinerie-ziffern.json",
    "rechner-verlauf.json",
    "rechnerklasse.json",
    "rohstoffe.json",
    "routen.json",
    "rufstufen.json",
    "schiffe.json",
    "scmdb-items.json",
    "seitennutzung.json",
    "serverstatus.json",
    "sicherung-laeuft.json",
    "signatur-graubilder.json",
    "signatur-letztes-bild.bin",
    "signatur-spielbild.bin",
    "signatur-ziffern-eigene-gruen.json",
    "signatur-ziffern-eigene-hell.json",
    "signatur-ziffern-eigene.json",
    "signaturen-spieldaten.json",
    "spiel-schiffe.json",
    "spiel-teile.json",
    "spielstand.json",
    "spielzeit.json",
    "start-spur-anlernen.txt",
    "start-spur-overlay.txt",
    "start-spur.txt",
    "statistik.json",
    "titel-original.json",
    "uebersetzung.json",
    "uebersetzung_abfragen.json",
    "update-ergebnis.txt",
    "update-helfer.1.txt",
    "update-helfer.txt",
    "update-lauf.json",
    "update-pause.json",
    "update-setup.txt",
    "update-sperre.json",
    "verkauf.json",
    "versionen.json",
    "waren-guete.json",
    "waren-namen.json",
    "watcher.json",
    "watchlist.json",
    "werkbank-auftraege.json",
    "werte-spieldaten.json",
];

/// The forms in which the program writes a copy beside one of its files (lower case), each with how the name of
/// the file comes back from it: the temporary file of an atomic write (`jsonio::temp_name`, `<name>.<process>-
/// <thread>.tmp`), the plain temporary file (`<name>.tmp`), the previous version (`jsonio::backup_name`,
/// `<stem>.bak.json`) and a broken file set aside (`<stem>.kaputt-<YYYYMMDD-HHMMSS>[-<n>].json`).
const COPY_FORMS: [(&str, &str); 4] = [
    (r"^(.+)\.\d+-\d+\.tmp$", "$1"),
    (r"^(.+)\.tmp$", "$1"),
    (r"^(.+)\.bak\.json$", "$1.json"),
    (r"^(.+)\.kaputt-\d{8}-\d{6}(?:-\d+)?\.json$", "$1.json"),
];

fn regex(pattern: &str) -> regex::Regex {
    regex::Regex::new(pattern).expect("a fixed expression")
}

/// The names of the program's files `name` may be: itself, and what each copy form gives back, one form after
/// another (a temporary file of a previous version too). In lower case.
fn originals(name: &str) -> Vec<String> {
    static FORMS: LazyLock<Vec<(regex::Regex, &'static str)>> =
        LazyLock::new(|| COPY_FORMS.iter().map(|(pattern, back)| (regex(pattern), *back)).collect());
    let mut found = vec![name.to_lowercase()];
    let mut at = 0;
    while at < found.len() {
        let current = found[at].clone();
        for (form, back) in FORMS.iter() {
            if form.is_match(&current) {
                let original = form.replace(&current, *back).into_owned();
                if !found.contains(&original) {
                    found.push(original);
                }
            }
        }
        at += 1;
    }
    found
}

/// Is `name` a name the program gives a file in its own subfolders: one of [`KNOWN_FILES`], the original texts of
/// a channel (`injection::origtext_path`: `injektion-urtext@<CHANNEL>.json`), or a copy of one of them in a form
/// the program writes ([`COPY_FORMS`])? Without regard to case. The state files of the Basetool need their content
/// too ([`owned_named`]).
pub fn known_file(name: &str) -> bool {
    known_original(name).is_some()
}

/// The name of the program's file `name` stands for, if it is one ([`known_file`]).
fn known_original(name: &str) -> Option<String> {
    let channels: Vec<String> =
        crate::paths::CHANNELS.iter().map(|c| format!("injektion-urtext@{}.json", c.to_lowercase())).collect();
    originals(name).into_iter().find(|original| KNOWN_FILES.contains(&original.as_str()) || channels.contains(original))
}

/// Is the file `path` (named `name`, in the own subfolder `folder`) one the program writes there: a known name
/// ([`known_file`]) in the subfolder the program puts it in (`paths::subfolder`), or the state file of an
/// installation of the Basetool in `Intern` whose content names that installation (`basetool::state`)?
pub fn owned_named(folder: &str, name: &str, path: &Path) -> bool {
    if let Some(original) = known_original(name) {
        let home =
            if original.starts_with("injektion-urtext@") { INNER_PARENT } else { crate::paths::subfolder(&original) };
        return home.eq_ignore_ascii_case(folder);
    }
    // The name first, the content only then: no foreign file is read for nothing.
    static STATE: LazyLock<regex::Regex> = LazyLock::new(|| regex(r"^basetool-[a-z0-9_.-]+\.json$"));
    folder.eq_ignore_ascii_case(INNER_PARENT)
        && STATE.is_match(&name.to_lowercase())
        && matches!(read_object(path), Some(map) if map.iter().any(|(key, value)| key == "installation_id"
            && value.as_str().is_some_and(|id| crate::basetool::state::file_name(id).eq_ignore_ascii_case(name))))
}

/// The name of the file a temporary file of the program stands for (`<name>.<process>-<thread>.tmp` or
/// `<name>.tmp`), in lower case; `None` for a name that is no temporary file.
fn temporary_of(lower: &str) -> Option<String> {
    static TEMP: LazyLock<regex::Regex> = LazyLock::new(|| regex(r"^(.+?)(?:\.\d+-\d+)?\.tmp$"));
    TEMP.captures(lower).map(|c| c[1].to_string())
}

/// Is the file `path` (named `name`) one the program writes into the own subfolder with a common name `folder`
/// ([`COMMON_FOLDERS`]), or into the folder of older exports in it (`inner`)?
///
/// - `Patches`: `patch-<version>.json` whose content is a patch of erkul.games with that version
///   (`patch_changes::file`), and its temporary files ([`temporary_of`]);
/// - `export`: the four export files by their names, their temporary files ([`temporary_of`]), and in `Ältere` the
///   dated files of older versions (`<stem>-<YYYY-MM-DD>.json`); a file with content must carry the program's
///   mark (its tool name, or the exact keys of the Basetool and the contract export);
/// - the backups of the first start: `Verse-Kit-Sicherung-vor-VerseKit-5-<YYYY-MM-DD-HHMMSS>[-<n>].zip` and
///   their part file (`first_backup::file_name`).
pub fn owned_in_common(folder: &str, inner: bool, name: &str, path: &Path) -> bool {
    let lower = name.to_lowercase();
    let temporary = temporary_of(&lower);
    if folder == crate::patch_changes::FOLDER {
        static PATCH: LazyLock<regex::Regex> = LazyLock::new(|| regex(r"^patch-([a-z0-9._-]+)\.json$"));
        if inner {
            return false;
        }
        // A temporary file of a patch file has no whole content to look at: its exact form is enough.
        if temporary.as_deref().is_some_and(|base| PATCH.is_match(base)) {
            return true;
        }
        let Some(version) = PATCH.captures(&lower).map(|c| c[1].to_string()) else { return false };
        return matches!(read_object(path), Some(map)
                if text_of(&map, "quelle") == Some("erkul.games")
                    && text_of(&map, "version")
                        .is_some_and(|v| crate::patch_changes::safe_name(v).to_lowercase() == version));
    }
    if folder == crate::export::FOLDER {
        let stems: Vec<String> = crate::export::Kind::ALL.iter().map(|k| k.stem().to_lowercase()).collect();
        if inner {
            static DATED: LazyLock<regex::Regex> = LazyLock::new(|| regex(r"^(.+)-\d{4}-\d{2}-\d{2}\.json$"));
            let dated = DATED.captures(&lower).is_some_and(|c| stems.contains(&c[1].to_string()));
            return dated && export_mark(path);
        }
        if temporary.is_some_and(|base| stems.iter().any(|stem| base == format!("{stem}.json"))) {
            return true;
        }
        return stems.iter().any(|stem| lower == format!("{stem}.json")) && export_mark(path);
    }
    static FIRST: LazyLock<regex::Regex> =
        LazyLock::new(|| regex(r"^verse-kit-sicherung-vor-versekit-5-\d{4}-\d{2}-\d{2}-\d{6}(-\d+)?\.zip(\.teil)?$"));
    folder == crate::first_backup::FOLDER && !inner && FIRST.is_match(&lower)
}

/// The mark of an export file of the program: the tool name under `werkzeug` or `productName` (also the one of
/// earlier versions), or exactly the keys of the Basetool export or of the contract export.
fn export_mark(path: &Path) -> bool {
    let Some(map) = read_object(path) else { return false };
    let names = [crate::export::TOOL_NAME, legacy::EXPORT_TOOL_NAME];
    let tool = ["werkzeug", "productName"].iter().any(|key| text_of(&map, key).is_some_and(|v| names.contains(&v)));
    let keys: HashSet<&str> = map.iter().map(|(k, _)| k.as_str()).collect();
    let exactly = |wanted: &[&str]| keys == wanted.iter().copied().collect::<HashSet<_>>();
    tool || exactly(&["blueprints"]) || exactly(&["format", "auftraege"])
}

/// The largest file whose content is looked at, in bytes: a larger one is no file of the program that needs it and
/// stays.
pub const LARGEST_READ: u64 = 1 << 20;

/// The JSON object in `path`, if it holds one (a link is not read, nor a file over [`LARGEST_READ`]).
fn read_object(path: &Path) -> Option<crate::jsonio::Map> {
    let text = path.to_string_lossy();
    if !is_real_file(&text) || fs::symlink_metadata(path).map_or(true, |m| m.len() > LARGEST_READ) {
        return None;
    }
    match jsonio::read_json(&text) {
        Ok(Value::Object(map)) => Some(map),
        _ => None,
    }
}

/// The text under `key` in an object.
fn text_of<'a>(map: &'a crate::jsonio::Map, key: &str) -> Option<&'a str> {
    map.iter().find(|(k, _)| k == key).and_then(|(_, v)| v.as_str())
}

/// The last part of the folder of the stored secrets.
const SECRETS_LEAF: &str = "basetool";

/// The exit code of the switches when they refused.
pub const EXIT_REFUSED: i32 = 2;

/// Why nothing was removed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// The data folder is named by the environment variable for test runs and special cases: its layout is flat
    /// and mixed, and it is not the user's installation.
    NamedFromOutside,
}

/// What kind of place a [`Place`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A data folder the program named itself: it goes when it is empty afterwards.
    NamedData,
    /// A data folder the user chose, with a mark of the program: it stays.
    ChosenData,
    /// A folder of the program (configuration, secrets, installers): it goes when it is empty afterwards.
    Program,
    /// A data folder a pointer names on the network: not looked into and not touched, only named.
    Network,
}

/// One place with data of the program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    pub path: String,
    pub kind: Kind,
}

/// What a removal did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Removal {
    /// The places that were cleared.
    pub places: Vec<String>,
    /// How many files went.
    pub removed: usize,
    /// Own files that could not be removed (still open, no right), full paths.
    pub failed: Vec<String>,
    /// Folders that went because they were empty afterwards.
    pub folders_removed: Vec<String>,
}

impl Removal {
    /// The exit code of [`SWITCH`]: 0 everything own went, 1 some own files stayed.
    pub fn exit_code(&self) -> i32 {
        if self.failed.is_empty() { 0 } else { 1 }
    }
}

/// The places with data of the program ([module](self)), each once.
///
/// # Errors
///
/// [`Refused::NamedFromOutside`] when the data folder is named by the environment variable.
pub fn places(paths: &Paths) -> Result<Vec<Place>, Refused> {
    if paths.host.var_set(legacy::ENV_HOME).is_some() {
        return Err(Refused::NamedFromOutside);
    }
    let f = paths.host.flavor;
    let mut found: Vec<Place> = Vec::new();
    let mut network: Vec<Place> = Vec::new();
    let mut add = |path: String, kind: Kind| {
        // On a copy of the data only what lies in the copy.
        let in_reach = paths.host.copy.as_deref().is_none_or(|root| under(paths, root, &path));
        if in_reach && is_real_dir(&path) && !found.iter().any(|known| same(paths, &known.path, &path)) {
            found.push(Place { path, kind });
        }
    };
    let documents = paths.documents();
    if on_network(&documents) {
        // Documents moved to a server: the default folders there are only named, as a chosen folder on the network.
        for name in [legacy::DOCUMENTS_FOLDER, NEW_FOLDER_NAME] {
            network.push(Place { path: f.join(&documents, name), kind: Kind::Network });
        }
    } else {
        add(f.join(&documents, legacy::DOCUMENTS_FOLDER), Kind::NamedData);
        add(f.join(&documents, NEW_FOLDER_NAME), Kind::NamedData);
    }
    if let Some(root) = paths.host.copy.as_deref() {
        add(f.join(root, COPY_DATA_FOLDER), Kind::NamedData);
    }
    for pointer in [paths.pointer_file(), paths.second_pointer()] {
        let Some(value) = paths.read_pointer(&pointer) else { continue };
        let folder = paths.host.expanduser(&value);
        // A relative path or one with a variable would be read against wherever the uninstaller runs.
        if !Path::new(&folder).is_absolute() || folder.contains('%') {
            continue;
        }
        // A folder on the network is not looked into: an unreachable share can stall the uninstaller. It is named
        // so the user knows it stays.
        if on_network(&folder) {
            let in_reach = paths.host.copy.as_deref().is_none_or(|root| under(paths, root, &folder));
            if in_reach && !network.iter().any(|known: &Place| same(paths, &known.path, &folder)) {
                network.push(Place { path: folder, kind: Kind::Network });
            }
            continue;
        }
        if marked(paths, &folder) {
            add(folder, Kind::ChosenData);
        }
    }
    let config = f.dirname(&paths.second_pointer());
    if last_part_is(&config, legacy::CONFIG_FOLDER) {
        add(config, Kind::Program);
    }
    // A folder of secrets named by the variable is somebody's choice, not the program's folder.
    let secrets_folder = secrets::folder(&paths.host);
    if paths.host.var_set(legacy::ENV_SECRETS).is_none() && last_part_is(&secrets_folder, SECRETS_LEAF) {
        add(secrets_folder, Kind::Program);
    }
    if let Some(local) = paths.host.var_set("LOCALAPPDATA") {
        add(f.join_all(local, &updater::INSTALLER_FOLDER), Kind::Program);
    }
    found.extend(network);
    Ok(found)
}

/// Does `folder` hold a mark of the program? Its settings file as an object with the key the setup writes
/// (`setup::VERSION_KEY`) or with at least two keys of the template, or its collection as an object with both the
/// version and the blueprints.
pub fn marked(paths: &Paths, folder: &str) -> bool {
    let f = paths.host.flavor;
    let keys_of = |file: String| -> Vec<String> {
        match is_real_file(&file).then(|| jsonio::read_json(&file)) {
            Some(Ok(Value::Object(map))) => map.into_iter().map(|(key, _)| key).collect(),
            _ => Vec::new(),
        }
    };
    let settings = keys_of(f.join_all(folder, &["Einstellungen", SETTINGS_FILE]));
    let template: Vec<String> = paths.template().into_iter().map(|(key, _)| key).collect();
    let of_template = settings.iter().filter(|key| template.contains(key)).collect::<HashSet<_>>().len();
    if settings.iter().any(|key| key == crate::setup::VERSION_KEY) || of_template >= 2 {
        return true;
    }
    let collection = keys_of(f.join_all(folder, &["Bauplaene", crate::collection::FILE_NAME]));
    ["version", "bauplaene"].iter().all(|needed| collection.iter().any(|key| key == needed))
}

/// Does `path` lie on the network: a share (`\\server\…`) or, under Windows, a drive letter of a network drive?
pub fn on_network(path: &str) -> bool {
    if path.starts_with("\\\\") || path.starts_with("//") {
        return true;
    }
    network_drive(path)
}

#[cfg(windows)]
fn network_drive(path: &str) -> bool {
    use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
    let mut chars = path.chars();
    let (Some(letter), Some(':')) = (chars.next(), chars.next()) else { return false };
    if !letter.is_ascii_alphabetic() {
        return false;
    }
    let root: Vec<u16> = format!("{letter}:\\").encode_utf16().chain([0]).collect();
    // SAFETY: the text ends with a zero and lives until the call returns. DRIVE_REMOTE is 4.
    unsafe { GetDriveTypeW(root.as_ptr()) == 4 }
}

#[cfg(not(windows))]
fn network_drive(_path: &str) -> bool {
    false
}

/// Removes the own data of the program at all its places ([module](self)).
///
/// # Errors
///
/// [`Refused::NamedFromOutside`] when the data folder is named by the environment variable; nothing is touched
/// then.
pub fn remove(paths: &Paths) -> Result<Removal, Refused> {
    let mut out = Removal::default();
    for place in places(paths)? {
        if place.kind == Kind::Network {
            continue;
        }
        out.places.push(place.path.clone());
        match place.kind {
            Kind::Network => {}
            Kind::NamedData | Kind::ChosenData => clear_data_folder(paths, &place, &mut out),
            Kind::Program => {
                clear_whole(Path::new(&place.path), &mut out);
                remove_if_empty(paths, &place.path, &mut out);
                // The folder of the program that holds the secrets and the installers, once both are gone.
                let parent = paths.host.flavor.dirname(&place.path);
                if !last_part_is(&place.path, legacy::CONFIG_FOLDER) && last_part_is(&parent, "VerseKit") {
                    remove_if_empty(paths, &parent, &mut out);
                }
            }
        }
    }
    Ok(out)
}

/// The mark at the start of a line of [`places_text`] for a place on the network that stays.
pub const NETWORK_MARK: &str = "-";

/// The places as text, one per line, for the uninstaller's question; a place on the network that stays begins
/// with [`NETWORK_MARK`].
pub fn places_text(places: &[Place]) -> String {
    places
        .iter()
        .map(
            |p| {
                if p.kind == Kind::Network {
                    format!("{NETWORK_MARK}{}\r\n", p.path)
                } else {
                    format!("{}\r\n", p.path)
                }
            },
        )
        .collect()
}

/// Empties the own parts of one data folder ([module](self)); a folder the program named goes if it is empty
/// afterwards.
fn clear_data_folder(paths: &Paths, place: &Place, out: &mut Removal) {
    let f = paths.host.flavor;
    let folder = &place.path;
    for sub in NAMED_FOLDERS {
        let path = f.join(folder, sub);
        if !is_real_dir(&path) {
            continue;
        }
        for (entry, kind) in entries(Path::new(&path)) {
            let name = file_name(&entry);
            if kind.is_file() && owned_named(sub, &name, &entry) {
                remove_one(&entry, out);
                continue;
            }
            let inner = INNER_FOLDERS.iter().find(|(known, _)| name.eq_ignore_ascii_case(known));
            if let (true, true, Some((_, form))) = (kind.is_dir(), sub == INNER_PARENT, inner) {
                let form = regex(form);
                clear_matching(&entry, |file, _| form.is_match(&file.to_lowercase()), out);
                remove_if_empty(paths, &entry.to_string_lossy(), out);
            }
        }
        remove_if_empty(paths, &path, out);
    }
    for common in COMMON_FOLDERS {
        let path = f.join(folder, common);
        if !is_real_dir(&path) {
            continue;
        }
        clear_matching(Path::new(&path), |name, file| owned_in_common(common, false, name, file), out);
        if common == crate::export::FOLDER {
            let older = f.join(&path, crate::export::OLD_FOLDER);
            if is_real_dir(&older) {
                clear_matching(Path::new(&older), |name, file| owned_in_common(common, true, name, file), out);
                remove_if_empty(paths, &older, out);
            }
        }
        remove_if_empty(paths, &path, out);
    }
    // Loose files at the top stay: the program writes none there.
    if place.kind == Kind::NamedData {
        remove_if_empty(paths, folder, out);
    }
}

/// Removes every file below `folder`, deepest first, and the folders below it that are empty then. Links and
/// junctions stay as they are.
fn clear_whole(folder: &Path, out: &mut Removal) {
    for (path, kind) in entries(folder) {
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            clear_whole(&path, out);
            if is_empty(&path) && remove_dir(&path).is_ok() {
                out.folders_removed.push(path.to_string_lossy().into_owned());
            }
        } else {
            remove_one(&path, out);
        }
    }
}

/// Removes the files directly in `folder` that `owned` takes as the program's (by name and path); nothing else,
/// no subfolder, no link.
fn clear_matching(folder: &Path, owned: impl Fn(&str, &Path) -> bool, out: &mut Removal) {
    for (path, kind) in entries(folder) {
        if kind.is_file() && owned(&file_name(&path), &path) {
            remove_one(&path, out);
        }
    }
}

/// The last part of a path as text.
fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// The entries of a folder with their kind (a link is not followed), sorted by name.
fn entries(folder: &Path) -> Vec<(std::path::PathBuf, fs::FileType)> {
    let Ok(read) = fs::read_dir(folder) else { return Vec::new() };
    let mut list: Vec<_> = read.flatten().filter_map(|e| Some((e.path(), e.file_type().ok()?))).collect();
    list.sort_by(|a, b| a.0.cmp(&b.0));
    list
}

/// Removes one file (under Windows also one marked read-only: the standard library deletes it as it is). Counted,
/// or listed as failed.
fn remove_one(path: &Path, out: &mut Removal) {
    let text = path.to_string_lossy().into_owned();
    match jsonio::remove_file(&text) {
        Ok(()) => out.removed += 1,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(_) => out.failed.push(text),
    }
}

/// Removes `folder` if it is empty, a real folder (no link) and none the system or the user keeps (a root, the
/// home folder, Documents, the folders of the system the places lie in).
fn remove_if_empty(paths: &Paths, folder: &str, out: &mut Removal) {
    let f = paths.host.flavor;
    let is_root = f.dirname(folder) == folder || f.dirname(folder).is_empty();
    if is_root || protected_folders(paths).iter().any(|p| same(paths, p, folder)) || !is_real_dir(folder) {
        return;
    }
    if is_empty(Path::new(folder)) && remove_dir(Path::new(folder)).is_ok() {
        out.folders_removed.push(folder.to_string());
    }
}

/// Folders that never go, even empty.
fn protected_folders(paths: &Paths) -> Vec<String> {
    let mut list = vec![paths.host.home(), paths.documents()];
    for name in ["USERPROFILE", "HOME", "APPDATA", "LOCALAPPDATA", "XDG_CONFIG_HOME", "XDG_DATA_HOME"] {
        if let Some(value) = paths.host.var_set(name) {
            list.push(value.to_string());
        }
    }
    list
}

fn is_empty(folder: &Path) -> bool {
    fs::read_dir(folder).is_ok_and(|mut e| e.next().is_none())
}

/// `rmdir`, blocked like any write access.
fn remove_dir(path: &Path) -> io::Result<()> {
    writeguard::check("rmdir", &path.to_string_lossy())?;
    fs::remove_dir(path)
}

/// A folder that is no link and no junction.
fn is_real_dir(path: &str) -> bool {
    !path.is_empty() && fs::symlink_metadata(path).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
}

/// A file that is no link.
fn is_real_file(path: &str) -> bool {
    !path.is_empty() && fs::symlink_metadata(path).is_ok_and(|m| m.is_file())
}

/// Is the last part of `path` exactly `name` (without regard to case under Windows)?
fn last_part_is(path: &str, name: &str) -> bool {
    let leaf = path.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or("");
    if cfg!(windows) { leaf.eq_ignore_ascii_case(name) } else { leaf == name }
}

/// The full, case-folded form of a path, without a separator at the end.
fn full(paths: &Paths, path: &str) -> String {
    let f = paths.host.flavor;
    f.normcase(&f.abspath(path.trim_end_matches(['/', '\\']), &paths.host.cwd))
}

/// Do two paths name the same place?
fn same(paths: &Paths, a: &str, b: &str) -> bool {
    full(paths, a) == full(paths, b)
}

/// Is `path` the folder `root` or below it?
fn under(paths: &Paths, root: &str, path: &str) -> bool {
    let (root, path) = (full(paths, root), full(paths, path));
    path == root || path.starts_with(&format!("{root}{}", paths.host.flavor.sep()))
}

#[cfg(test)]
mod tests;
