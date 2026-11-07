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

//! Names from the previous version, without which existing data cannot be
//! found.
//!
//! ⚠ The only place with these names.
//!
//! Also the copying of the files an older version kept in the hidden folder ([`Paths::migrate`]).

use std::fs;

use crate::errors;
use crate::jsonio::{self, Map, Value};
use crate::paths::Paths;
use crate::pypath::Flavor;

/// The data folder under Documents, as older installations created it. If it
/// exists, it stays the default folder (including the pointer file).
pub const DOCUMENTS_FOLDER: &str = "SC BP Watcher";

/// The folder in the configuration directory (`%APPDATA%` or `~/.config`);
/// the second pointer file lives there.
pub const CONFIG_FOLDER: &str = "sc-bp-watcher";

/// The name 4.12 gave its autostart entry (the value in the `Run` key, and the name of the Linux autostart
/// file in lower case with hyphens). A start with the system that was set up then is taken over under the
/// new name (`autostart::migrate`).
pub const AUTOSTART_NAME: &str = "SC BP Watcher";

/// The file of the entry in the application menu 4.12 wrote on Linux, and the icon it put next to it
/// (`desktop_entry::take_over`).
pub const MENU_FILE: &str = "sc-bp-watcher.desktop";
pub const MENU_ICON: &str = "sc-bp-watcher.png";

/// The name 4.12 wrote as `werkzeug` into its export and backup files: a file with it is still read in as the
/// program's own (`importer::detect`).
pub const EXPORT_TOOL_NAME: &str = "SC BP Watcher";

/// The mark in the attachment of a backup of 4.12 (`sicherung.txt`): such a backup is still read in.
pub const BACKUP_MARKER: &str = "SC-BP-Watcher-Sicherung";

/// The name of the previous version's program (without `.exe`), as the
/// process list shows it.
pub const PROGRAM_PROCESS: &str = "SC-BP-Watcher";

/// The name of the previous version's program file. 5.0 installs itself under this name, because the update
/// helper of 4.12 starts exactly this file in the installation folder once the setup has run.
pub const PROGRAM_FILE: &str = "SC-BP-Watcher.exe";

/// What a file name of this program contains in the previous version (lower case), besides the new name. An
/// AppImage an update of 4.12 replaced in place keeps its old name.
pub const OWN_FILENAME: &str = "sc-bp-watcher";

/// The repository under its previous name; GitHub redirects it to the new one. Download addresses with it are
/// still the program's own.
pub const OLD_REPO: &str = "Xharig/SC-BP-Watcher";

/// What the update of 4.12 leaves in the temporary folder: its helper script and the second place of the setup
/// log.
pub const UPDATE_HELPER_SCRIPT: &str = "scbp-update-helfer.cmd";
pub const UPDATE_SETUP_LOG: &str = "scbp-update-setup.txt";

/// Environment variable for a custom data folder (test runs, special cases).
pub const ENV_HOME: &str = "SC_BP_HOME";

/// Environment variables of the Basetool connection. The test environment
/// (sandbox) is chosen only through these, never in the user interface:
/// another issuer, and only together with it another API address, client
/// id and certificate file.
pub const ENV_BASETOOL_ISSUER: &str = "SC_BP_BASETOOL_ISSUER";
pub const ENV_BASETOOL_API: &str = "SC_BP_BASETOOL_API";
pub const ENV_BASETOOL_CLIENT: &str = "SC_BP_BASETOOL_CLIENT";
pub const ENV_BASETOOL_CA: &str = "SC_BP_BASETOOL_CA";
/// Name of the Windows key, so test runs never touch the player's key.
pub const ENV_BASETOOL_KEYNAME: &str = "SC_BP_BASETOOL_KEYNAME";
/// No network: only the own computer (127.0.0.1) stays reachable.
pub const ENV_NO_NET: &str = "SC_BP_NO_NET";
/// Other address for error reports (test runs); anything that is not an
/// `https://` address, or `aus`, turns sending off.
pub const ENV_REPORT_TARGET: &str = "SC_BP_BERICHT_ZIEL";
/// Folder of the stored secrets, for test runs.
pub const ENV_SECRETS: &str = "SC_BP_SECRETS";
/// Secrets in a file even where the Secret Service is there, for test runs.
pub const ENV_SECRETS_FILE: &str = "SC_BP_SECRETS_FILE";
/// The file of the own blueprint corrections at another place; the new name comes first (`overrides::ENV_FILE`).
pub const ENV_OVERRIDES: &str = "SC_BP_OVERRIDES";
/// Another port of the bridge to scmdb.net; the new name comes first (`scmdb_bridge::ENV_PORT`).
pub const ENV_SCMDB_PORT: &str = "SC_BP_SCMDB_PORT";

/// Output files in the export folder: the full backup and the contract log.
/// Older versions put `-<date>` before `.json`.
pub const EXPORT_FULL: &str = "SC-BP-Watcher-Bestand";
pub const EXPORT_CONTRACTS: &str = "SC-BP-Watcher-Auftraege";

/// The block the previous version inserted into contract titles in the
/// game's texts; it can still stand in titles of the game log.
pub const TITLE_MARK: (&str, &str) = ("[SCBPW]", "[/SCBPW]");

/// `TITLE_MARK` as a pattern for the whole block.
pub const TITLE_BLOCK: &str = r"\[SCBPW\].*?\[/SCBPW\]";

/// The ending of the lock files this program keeps next to the second pointer (`Paths::save_lock_file`): they
/// are not data and are not copied.
const LOCK_ENDING: &str = ".lock";

/// The start of a settings key that only explains (`Paths::drop_explanation_lines`).
const EXPLANATION_MARK: char = '_';

impl Paths {
    /// The hidden folder (`%APPDATA%` or `~/.config`) older versions kept the data in, and where the second
    /// pointer lies: the source of the copying at the start.
    ///
    /// ⚠ Under Windows an `APPDATA` that is set but empty counts (`os.environ.get('APPDATA', home)`), unlike
    /// [`Paths::second_pointer`].
    pub fn legacy_app_folder(&self) -> String {
        let f = self.host.flavor;
        let base = match f {
            Flavor::Windows => self.host.var("APPDATA").map(String::from).unwrap_or_else(|| self.host.home()),
            Flavor::Posix => match self.host.var_set("XDG_CONFIG_HOME") {
                Some(v) => v.to_string(),
                None => self.host.expanduser("~/.config"),
            },
        };
        f.join(&base, CONFIG_FOLDER)
    }

    /// Do files lie in the hidden folder that the data folder lacks? A `.json` or `.txt` must lie there, and
    /// the data folder must not hold a collection yet. Never with the data folder named from outside.
    ///
    /// ⚠ Asking for the collection creates the data folder and its subfolder, as every `app_file` does.
    pub fn migration_needed(&self) -> bool {
        let old = self.legacy_app_folder();
        if !fs::metadata(&old).is_ok_and(|m| m.is_dir()) || self.host.var_set(ENV_HOME).is_some() {
            return false;
        }
        let Ok(entries) = fs::read_dir(&old) else { return false };
        let existing = entries
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .any(|name| name.ends_with(".json") || name.ends_with(".txt"));
        existing && fs::metadata(self.app_file("bestand.json")).is_err()
    }

    /// Copy the files of the hidden folder into the data folder, each to its subfolder. Returns how many were
    /// copied.
    ///
    /// ⚠⚠ Copy, never move: the hidden folder stays as it is, it is the way back. A file that lies in the data
    /// folder already is not overwritten; a file that cannot be copied is left out.
    pub fn migrate(&self) -> usize {
        let old = self.legacy_app_folder();
        let Ok(entries) = fs::read_dir(&old) else { return 0 };
        let mut names: Vec<String> = entries.filter_map(|e| e.ok()?.file_name().into_string().ok()).collect();
        names.sort();
        let f = self.host.flavor;
        let mut copied = 0;
        for name in names {
            let source = f.join(&old, &name);
            if !fs::metadata(&source).is_ok_and(|m| m.is_file()) || name.ends_with(LOCK_ENDING) {
                continue;
            }
            let target = self.app_file(&name);
            if fs::metadata(&target).is_ok() {
                continue;
            }
            if jsonio::make_dirs(&f.dirname(&target)).and_then(|()| jsonio::copy2(&source, &target)).is_ok() {
                copied += 1;
            }
        }
        copied
    }

    /// Removes every top-level key of the settings file that starts with an underscore: the explanatory lines
    /// older versions wrote between the real settings. Returns how many keys went.
    ///
    /// ⚠ Nothing is written when no such key exists, when the file is missing, or when it cannot be read right
    /// now; a failed write is recorded and leaves the file as it was. The remaining keys keep their values and
    /// their order, and the file goes through the same atomic write as `set_setting`.
    pub fn drop_explanation_lines(&self) -> usize {
        let _lock = jsonio::lock_saves();
        let (data, readable) = self.settings_read();
        if !readable {
            return 0;
        }
        let kept: Map = data.iter().filter(|(key, _)| !key.starts_with(EXPLANATION_MARK)).cloned().collect();
        let dropped = data.len() - kept.len();
        if dropped == 0 {
            return 0;
        }
        match jsonio::save_json(&self.settings_file(), &Value::Object(kept), 2, false) {
            Ok(()) => dropped,
            Err(e) => {
                errors::record("legacy.explanation_lines", &e);
                0
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Host;

    fn scratch(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("versekit-legacy-{name}-{}", crate::testdir::run_tag()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.to_string_lossy().into_owned()
    }

    /// A machine of its own below `root`: home, Documents and configuration folder, no registry.
    fn machine(root: &str) -> Paths {
        let f = Flavor::native();
        let home = f.join(root, "home");
        let config = f.join(root, "appdata");
        fs::create_dir_all(f.join(&home, "Dokumente")).unwrap();
        let vars = [
            ("USERPROFILE", home.as_str()),
            ("HOME", home.as_str()),
            ("APPDATA", &config),
            ("XDG_CONFIG_HOME", &config),
        ];
        Paths::new(Host::new(f, &vars, root))
    }

    fn put(path: &str, text: &str) {
        fs::create_dir_all(Flavor::native().dirname(path)).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn the_files_of_the_hidden_folder_are_copied_once_each_to_its_place() {
        let root = scratch("umzug");
        let p = machine(&root);
        let f = Flavor::native();
        let old = p.legacy_app_folder();
        assert_eq!(old, f.join_all(&root, &["appdata", CONFIG_FOLDER]));
        put(&f.join(&old, "bestand.json"), "{\"a\": 1}");
        put(&f.join(&old, "einstellungen.json"), "{}");
        put(&f.join(&old, "hochwasser.json"), "{}");
        put(&f.join(&old, "sperre-abc.lock"), "");
        put(&f.join_all(&old, &["Unter", "tief.json"]), "{}");
        assert!(p.migration_needed());
        assert_eq!(p.migrate(), 3);
        let data = f.join_all(&root, &["home", "Dokumente", crate::paths::NEW_FOLDER_NAME]);
        assert_eq!(fs::read_to_string(f.join_all(&data, &["Bauplaene", "bestand.json"])).unwrap(), "{\"a\": 1}");
        assert!(fs::metadata(f.join_all(&data, &["Einstellungen", "einstellungen.json"])).is_ok());
        assert!(fs::metadata(f.join_all(&data, &["Intern", "hochwasser.json"])).is_ok());
        assert!(fs::metadata(f.join_all(&data, &["Intern", "sperre-abc.lock"])).is_err(), "a lock file is no data");
        assert!(fs::metadata(f.join_all(&data, &["Intern", "tief.json"])).is_err(), "only files at the top");
        assert!(fs::metadata(f.join(&old, "bestand.json")).is_ok(), "copied, not moved");
        assert!(!p.migration_needed(), "the collection lies in the data folder now");
        assert_eq!(p.migrate(), 0, "nothing is overwritten");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn nothing_is_copied_without_data_files_with_a_collection_or_with_a_named_data_folder() {
        let root = scratch("kein-umzug");
        let p = machine(&root);
        let f = Flavor::native();
        assert!(!p.migration_needed(), "no hidden folder");
        let old = p.legacy_app_folder();
        put(&f.join(&old, "bild.png"), "x");
        put(&f.join(&old, "GROSS.JSON"), "x");
        assert!(!p.migration_needed(), "neither .json nor .txt");
        put(&f.join(&old, "bestand.json"), "{}");
        assert!(p.migration_needed());
        put(&p.app_file("bestand.json"), "{\"neu\": 1}");
        assert!(!p.migration_needed(), "a collection lies in the data folder");
        let mut named = p.host.clone();
        named.set_var(ENV_HOME, &f.join(&root, "eigen"));
        let _ = fs::remove_file(p.app_file("bestand.json"));
        assert!(p.migration_needed());
        assert!(!Paths::new(named).migration_needed(), "a data folder named from outside");
        let _ = fs::remove_dir_all(&root);
    }

    /// Puts a file's modification time far into the past, so it does not count as just written.
    fn age(path: &str) {
        let old = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
        fs::File::options().write(true).open(path).unwrap().set_modified(old).unwrap();
    }

    #[test]
    fn the_explanatory_lines_go_and_the_real_settings_keep_their_values_and_their_order() {
        let root = scratch("erklaerzeilen");
        let p = machine(&root);
        let file = p.settings_file();
        put(
            &file,
            "{\n  \"_hinweis\": \"Zeilen mit _ sind nur Erklaerung.\",\n  \"spiel_ordner\": \"C:/Spiel\",\n  \
             \"_spiel_ordner_gesucht_wird_hier\": [\"a\", \"b\"],\n  \"sprache\": \"de\",\n  \
             \"_sprache_moeglich\": \"auto, de, en\",\n  \"pruefintervall_sekunden\": 7,\n  \
             \"fremd_\": {\"_innen\": 1},\n  \"signalton\": false\n}\n",
        );
        age(&file);
        assert_eq!(p.drop_explanation_lines(), 3);
        let data = jsonio::read_json(&file).unwrap();
        let Value::Object(map) = data else { panic!("no object") };
        let keys: Vec<&str> = map.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["spiel_ordner", "sprache", "pruefintervall_sekunden", "fremd_", "signalton"]);
        assert_eq!(map[0].1, Value::str("C:/Spiel"));
        assert_eq!(map[1].1, Value::str("de"));
        assert_eq!(map[2].1, Value::int(7));
        assert_eq!(map[3].1.get("_innen"), Some(&Value::int(1)), "only the top level is cleaned");
        assert_eq!(map[4].1, Value::Bool(false));
        age(&file);
        assert_eq!(p.drop_explanation_lines(), 0, "a second start finds nothing");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_settings_file_without_explanatory_lines_is_not_touched() {
        let root = scratch("erklaerzeilen-keine");
        let p = machine(&root);
        let file = p.settings_file();
        // Written the way nobody but a person writes it: the program would format it differently.
        let text = "{\"sprache\":   \"en\",\n\t\"max_zeilen\": 5}";
        put(&file, text);
        age(&file);
        let before = fs::metadata(&file).unwrap().modified().unwrap();
        assert_eq!(p.drop_explanation_lines(), 0);
        assert_eq!(fs::read_to_string(&file).unwrap(), text);
        assert_eq!(fs::metadata(&file).unwrap().modified().unwrap(), before, "not rewritten");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_or_broken_settings_file_ends_without_a_crash_and_without_a_new_file() {
        let root = scratch("erklaerzeilen-kaputt");
        let p = machine(&root);
        let file = p.settings_file();
        assert_eq!(p.drop_explanation_lines(), 0);
        assert!(fs::metadata(&file).is_err(), "no file is created");

        // Just written and not complete: left alone.
        put(&file, "{\"_hinweis\": \"x\", \"spra");
        assert_eq!(p.drop_explanation_lines(), 0);
        assert_eq!(fs::read_to_string(&file).unwrap(), "{\"_hinweis\": \"x\", \"spra");

        // Broken for a while: set aside by the reading as always, nothing of it is written back.
        age(&file);
        assert_eq!(p.drop_explanation_lines(), 0);
        assert!(fs::metadata(&file).is_err(), "the broken file was set aside");

        // Not an object.
        put(&file, "[\"_hinweis\"]");
        age(&file);
        assert_eq!(p.drop_explanation_lines(), 0);
        let _ = fs::remove_dir_all(&root);
    }

    /// The name is what older files carry: a change would stop them from being read in.
    #[test]
    fn the_name_older_export_files_carry_is_the_one_they_were_written_with() {
        assert_eq!(EXPORT_TOOL_NAME, "SC BP Watcher");
        assert!(crate::importer::OWN_TOOL_NAMES.contains(&EXPORT_TOOL_NAME));
    }
}
