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

//! Changing the data folder and taking the data along, checked file by file
//! (`storage_status`, `move_storage`, `remove_old_storage` and `_move_storage`
//! of the Python version), and bringing own files that were dragged into the
//! data folder to their places ([`Paths::tidy_storage`]).
//!
//! The steps are separate so the window can ask in between:
//! [`Paths::plan_storage_move`] looks at the target and says which question
//! is due, [`Paths::apply_storage_move`] does what was chosen.
//!
//! ⚠⚠ Copy, check, and only then clear the old folder, and only what is
//! proven to lie at the new place. The old folder is the only way back.

use std::fs;
use std::io::{self, Read};

use ring::digest;

use crate::jsonio::{self, Value};
use crate::paths::{Paths, STORAGE_KEY, SUBFOLDERS, file_identity, subfolder};
use crate::{errors, legacy, writeguard};

/// Files at the top of a data folder that come along: only these endings, so
/// whoever points the data folder at a folder with other things in it keeps them.
pub const MOVE_EXTENSIONS: [&str; 2] = [".json", ".txt"];

/// The subfolders Verse-Kit makes itself. All of their content is ours,
/// whatever the type of the file.
pub const OWN_FOLDERS: [&str; 6] = ["Bauplaene", "Einstellungen", "Intern", "Diagnose", "Patches", "export"];

/// The name of the file written to find out whether a folder takes files.
const PROBE_NAME: &str = ".schreibprobe";

/// What a target folder allows, before anything is touched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageStatus {
    /// Whether anything can be created there.
    pub writable: bool,
    /// How many own files already lie there (a data folder of someone else).
    pub files: usize,
    /// The system's reason if it is not writable, else empty.
    pub reason: String,
}

/// Which question is due for a chosen folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Plan {
    /// The data folder is fixed from outside: nothing is chosen or moved.
    Given,
    /// It is the folder in use already.
    Same,
    /// It lies inside the folder in use, or the folder in use inside it.
    Nested,
    /// Nothing can be saved there; nothing is changed.
    NotWritable { reason: String },
    /// A data folder lies there already (`files` own files): ask whether to use it.
    Occupied { files: usize },
    /// Nothing to take along: switch without a question.
    NothingToTake,
    /// `own` files lie in the folder in use: ask whether to take them along.
    OfferMove { own: usize },
}

/// What the user chose for a [`Plan`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    /// Copy, check, switch, clear the old folder.
    TakeAlong,
    /// Switch and leave the data where they are.
    SwitchOnly,
    /// Switch to the data that lie at the target already.
    UseExisting,
}

/// How a switch ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The data folder is fixed from outside: nothing was done.
    Given,
    /// One folder lies inside the other: nothing was done.
    Nested,
    /// Switched without data.
    Switched,
    /// Switched to the data that lie at the target.
    UsedExisting,
    /// Moved: files copied and checked, `kept` stayed in the old folder
    /// (relative names).
    Moved { copied: usize, kept: Vec<String>, old: String },
    /// Not switched: `failed` files could not be copied safely, `copied` went through.
    Failed { failed: usize, copied: usize },
    /// Not switched: the new place could not be written into the pointer files.
    NotSaved,
}

/// Copied, skipped (already there), failed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Moved {
    pub copied: usize,
    pub skipped: usize,
    pub failed: usize,
}

/// SHA-256 of a file as hexadecimal text, or empty if it cannot be read.
pub fn checksum(path: &str) -> String {
    let Ok(mut file) = fs::File::open(path) else { return String::new() };
    let mut context = digest::Context::new(&digest::SHA256);
    let mut block = [0u8; 65536];
    loop {
        match file.read(&mut block) {
            Ok(0) => break,
            Ok(n) => context.update(&block[..n]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return String::new(),
        }
    }
    context.finish().as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

/// Both files can be read and have the same content. Two files that cannot
/// be read are not "the same".
fn same_content(a: &str, b: &str) -> bool {
    let first = checksum(a);
    !first.is_empty() && first == checksum(b)
}

/// `os.rmdir`, blocked like any write access.
fn remove_dir(path: &str) -> io::Result<()> {
    writeguard::check("rmdir", path)?;
    fs::remove_dir(path)
}

impl Paths {
    /// The entries of a folder, sorted by name: (name, is a folder, is a link).
    /// A link to a folder counts as a folder, but is not entered.
    fn walk_entries(&self, folder: &str) -> Vec<(String, bool, bool)> {
        let Ok(entries) = fs::read_dir(folder) else { return Vec::new() };
        let mut list: Vec<(String, bool, bool)> = entries
            .filter_map(|e| {
                let e = e.ok()?;
                let name = e.file_name().into_string().ok()?;
                let linked = e.file_type().ok()?.is_symlink();
                Some((name, e.path().is_dir(), linked))
            })
            .collect();
        list.sort();
        list
    }

    /// All files below `base`, the files of a folder before its subfolders.
    /// `rel` is the name of `base` relative to the data folder; it is carried
    /// along, never cut out of the full path (a folder given as `D:/Daten/`
    /// would not match its own prefix).
    fn walk_files(&self, base: &str, rel: &str, out: &mut Vec<(String, String)>) {
        let flavor = self.host.flavor;
        let entries = self.walk_entries(base);
        for (name, is_dir, _) in &entries {
            if !is_dir && !name.ends_with(".tmp") {
                out.push((flavor.join(base, name), flavor.join(rel, name)));
            }
        }
        for (name, is_dir, linked) in &entries {
            if *is_dir && !linked {
                self.walk_files(&flavor.join(base, name), &flavor.join(rel, name), out);
            }
        }
    }

    /// Do two paths lead to the same file: the same name, or the same file
    /// on the disk (a link, a junction, a mapped drive)?
    fn same_file(&self, a: &str, b: &str) -> bool {
        let f = self.host.flavor;
        if f.normcase(&f.abspath(a, &self.host.cwd)) == f.normcase(&f.abspath(b, &self.host.cwd)) {
            return true;
        }
        let (x, y) = (file_identity(a), file_identity(b));
        x != (0, 0) && x == y
    }

    /// All own files of a data folder: (full path, path relative to the folder).
    ///
    /// ⚠ In the own subfolders everything counts, at the top only `.json` and
    /// `.txt`; a loose file there goes to its place by `SUBFOLDERS`. Files
    /// ending in `.tmp` are half written and left out.
    pub fn storage_items(&self, folder: &str) -> Vec<(String, String)> {
        let mut result = Vec::new();
        if !fs::metadata(folder).is_ok_and(|m| m.is_dir()) {
            return result;
        }
        let flavor = self.host.flavor;
        for sub in OWN_FOLDERS {
            self.walk_files(&flavor.join(folder, sub), sub, &mut result);
        }
        let mut loose: Vec<String> = fs::read_dir(folder)
            .map(|entries| entries.filter_map(|e| e.ok()?.file_name().into_string().ok()).collect())
            .unwrap_or_default();
        loose.sort();
        for name in loose {
            let full = flavor.join(folder, &name);
            if fs::metadata(&full).is_ok_and(|m| m.is_file()) && MOVE_EXTENSIONS.iter().any(|e| name.ends_with(e)) {
                result.push((full, flavor.join(subfolder(&name), &name)));
            }
        }
        result
    }

    /// What is going on at the target, before anything else is touched.
    ///
    /// ⚠ Check first, not on failing: a switch that stops halfway leaves half
    /// of the data at each place. The usual case is a system disk mounted
    /// read-only in the other system of a dual boot: it looks like a valid
    /// folder and takes nothing.
    pub fn storage_status(&self, target: &str) -> StorageStatus {
        let failed = |e: io::Error| StorageStatus { writable: false, files: 0, reason: e.to_string() };
        if let Err(e) = jsonio::make_dirs(target) {
            return failed(e);
        }
        let probe = self.host.flavor.join(target, PROBE_NAME);
        if let Err(e) = jsonio::write_new(&probe, b"x").and_then(|()| jsonio::remove_file(&probe)) {
            return failed(e);
        }
        StorageStatus { writable: true, files: self.storage_items(target).len(), reason: String::new() }
    }

    /// Is one of the two folders inside the other, or are they the same?
    ///
    /// Then nothing is copied and nothing is cleared: a move into its own
    /// subfolder would copy itself, and clearing would hit the new data.
    pub fn nested(&self, a: &str, b: &str) -> bool {
        let f = self.host.flavor;
        let a = f.normcase(&f.abspath(a, &self.host.cwd));
        let b = f.normcase(&f.abspath(b, &self.host.cwd));
        let sep = f.sep();
        a == b || a.starts_with(&format!("{b}{sep}")) || b.starts_with(&format!("{a}{sep}"))
    }

    /// Copy the own files into the new folder, each one checked afterwards.
    ///
    /// ⚠⚠ Copy, do not move, and delete nothing: the old folder stays whole.
    /// Every file is compared by checksum after copying; a half copy (full
    /// disk, a share that breaks off) is removed and counts as failed.
    /// Whatever already exists at the target is not overwritten.
    pub fn move_storage(&self, source: &str, target: &str) -> Moved {
        self.move_storage_with(source, target, &jsonio::copy2)
    }

    /// [`Paths::move_storage`] with the way a file is copied given, so a test
    /// can break it.
    pub fn move_storage_with(&self, source: &str, target: &str, copy: &dyn Fn(&str, &str) -> io::Result<()>) -> Moved {
        let mut moved = Moved::default();
        if !fs::metadata(source).is_ok_and(|m| m.is_dir()) || self.nested(source, target) {
            return moved;
        }
        let flavor = self.host.flavor;
        for (from, rel) in self.storage_items(source) {
            let to = flavor.join(target, &rel);
            if self.same_file(&from, &to) {
                // The target is this very file (a link, a mapped drive): not a copy.
                moved.failed += 1;
                continue;
            }
            if fs::symlink_metadata(&to).is_ok() || fs::metadata(&to).is_ok() {
                moved.skipped += 1;
                continue;
            }
            let copied = jsonio::make_dirs(&flavor.dirname(&to)).and_then(|()| copy(&from, &to));
            if let Err(e) = copied {
                errors::record(&format!("paths.move_storage.{}", flavor.basename(&rel)), &e);
                // ⚠ A half copy would count as "already there" at the next try.
                let _ = jsonio::remove_file(&to);
                moved.failed += 1;
                continue;
            }
            if !same_content(&from, &to) {
                moved.failed += 1;
                let _ = jsonio::remove_file(&to);
                continue;
            }
            moved.copied += 1;
        }
        moved
    }

    /// Clear the old data folder: only what is proven to lie at the new place.
    /// Returns how many files went and which stayed (relative names).
    ///
    /// ⚠⚠ A file goes only if its counterpart has the same checksum. One that
    /// differs at the target (data of another computer, or the watcher wrote
    /// on) stays; one that is missing there is copied and checked first. What
    /// cannot be proven stays, and so does a path that leads to the same file.
    ///
    /// ⚠ The pointer file under Documents stays: the next start reads from it
    /// where the data lie now. It is cut to that one field. Foreign files stay;
    /// the folder itself goes only if it is empty afterwards.
    pub fn remove_old_storage(&self, source: &str, target: &str) -> (usize, Vec<String>) {
        let (mut removed, mut kept) = (0, Vec::new());
        if !fs::metadata(source).is_ok_and(|m| m.is_dir()) || self.nested(source, target) {
            return (0, kept);
        }
        let flavor = self.host.flavor;
        let norm = |p: &str| flavor.normcase(&flavor.abspath(p, &self.host.cwd));
        let pointer = norm(&self.pointer_file());
        for (from, rel) in self.storage_items(source) {
            if norm(&from) == pointer {
                continue;
            }
            let to = flavor.join(target, &rel);
            if self.same_file(&from, &to) {
                kept.push(rel);
                continue;
            }
            let exists = fs::metadata(&to).is_ok();
            let mut same = exists && same_content(&from, &to);
            if !same && exists {
                // Something else lies there (data of another computer, or the
                // watcher wrote on): the target counts, the old file stays as the way back.
                kept.push(rel);
                continue;
            }
            if !same {
                let copied = jsonio::make_dirs(&flavor.dirname(&to)).and_then(|()| jsonio::copy2(&from, &to));
                match copied {
                    Ok(()) => same = same_content(&from, &to),
                    Err(e) => errors::record("paths.remove_old_storage.kopie", &e),
                }
            }
            if !same {
                kept.push(rel);
                continue;
            }
            match jsonio::remove_file(&from) {
                Ok(()) => removed += 1,
                Err(e) => {
                    // Windows: the file is still open (a log, for example).
                    errors::record("paths.remove_old_storage", &e);
                    kept.push(rel);
                }
            }
        }
        // Cut the pointer to its purpose: only where the data lie.
        if norm(&flavor.dirname(&flavor.dirname(&self.pointer_file()))) == norm(source) {
            self.write_first_pointer(target);
        }
        // Empty folders from the bottom up, only the own subfolders and the
        // old folder itself, never one with content.
        for sub in OWN_FOLDERS {
            let mut folders = Vec::new();
            self.walk_folders(&flavor.join(source, sub), &mut folders);
            for folder in folders {
                let _ = remove_dir(&folder);
            }
        }
        let _ = remove_dir(source);
        (removed, kept)
    }

    /// Folders below `base` and `base` itself, deepest first.
    fn walk_folders(&self, base: &str, out: &mut Vec<String>) {
        if !fs::metadata(base).is_ok_and(|m| m.is_dir()) {
            return;
        }
        for (name, is_dir, linked) in self.walk_entries(base) {
            if is_dir && !linked {
                self.walk_folders(&self.host.flavor.join(base, &name), out);
            }
        }
        out.push(base.to_string());
    }

    /// Which question is due when `target` was chosen as the new data folder.
    ///
    /// ⚠ Four situations, four answers; none may pass silently:
    /// not writable (nothing is set), a data folder lies there already
    /// (ask, touch nothing there), nothing to take along (switch quietly),
    /// data to take along (ask).
    pub fn plan_storage_move(&self, target: &str) -> Plan {
        // ⚠ Before anything is looked at, let alone created: a folder given from
        // outside (a copy of the data, or data that must stay as they are) is never moved.
        if self.given().data.is_some() {
            return Plan::Given;
        }
        let f = self.host.flavor;
        let old = self.app_folder();
        if f.abspath(&old, &self.host.cwd) == f.abspath(target, &self.host.cwd) {
            return Plan::Same;
        }
        // ⚠ One inside the other: nothing could be copied, and the switch would
        // leave an empty data folder behind (4.12 said "0 files moved" and switched).
        if self.nested(&old, target) {
            return Plan::Nested;
        }
        let status = self.storage_status(target);
        if !status.writable {
            return Plan::NotWritable { reason: status.reason };
        }
        let own = self.storage_items(&old).len();
        if status.files > 0 {
            return Plan::Occupied { files: status.files };
        }
        if own == 0 {
            return Plan::NothingToTake;
        }
        Plan::OfferMove { own }
    }

    /// Do what was chosen for `target`.
    ///
    /// ⚠⚠ With [`Choice::TakeAlong`] a failed file means: not switched. Else
    /// the setting would point at a folder with gaps while the complete data
    /// lie at a place nobody looks at anymore.
    pub fn apply_storage_move(&self, target: &str, choice: Choice) -> Outcome {
        if self.given().data.is_some() {
            return Outcome::Given;
        }
        let old = self.app_folder();
        if self.nested(&old, target) {
            return Outcome::Nested;
        }
        let set = || self.set_setting(STORAGE_KEY, Value::str(target));
        match choice {
            Choice::SwitchOnly if set() => Outcome::Switched,
            Choice::UseExisting if set() => Outcome::UsedExisting,
            Choice::SwitchOnly | Choice::UseExisting => Outcome::NotSaved,
            Choice::TakeAlong => {
                let moved = self.move_storage(&old, target);
                if moved.failed > 0 {
                    return Outcome::Failed { failed: moved.failed, copied: moved.copied };
                }
                // ⚠ Clear the old folder only once the new place is saved: else the
                // next start would read the emptied one.
                if !set() {
                    return Outcome::NotSaved;
                }
                let (_, kept) = self.remove_old_storage(&old, target);
                Outcome::Moved { copied: moved.copied, kept, old }
            }
        }
    }

    /// Bring own files that lie loose at the top of the data folder, or inside an older data folder that sits
    /// in it, to their places. Returns how many were moved. Without `folder` the data folder in use; with the
    /// data folder named from outside and no `folder`, nothing (test runs want it flat).
    ///
    /// Loose at the top counts only what `SUBFOLDERS` names, so a data folder in a folder that others use too
    /// keeps their files. A folder in it counts as an older data folder if it holds one of `OWN_FOLDERS`; its
    /// whole content goes to the same place in this one, and it is removed where it is empty afterwards.
    ///
    /// ⚠ Moved only where nothing lies at the target: a file there is the one in use, the other one stays.
    /// ⚠ A loose file counts only if its folder is written exactly like `folder` (`os.path.dirname(...) ==
    /// folder`): given with a separator at the end, nothing loose is moved.
    pub fn tidy_storage(&self, folder: Option<&str>) -> usize {
        if folder.is_none() && self.host.var_set(legacy::ENV_HOME).is_some() {
            return 0;
        }
        let folder = match folder {
            Some(given) if !given.is_empty() => given.to_string(),
            _ => self.app_folder(),
        };
        let flavor = self.host.flavor;
        let mut sources: Vec<(String, String)> = self
            .storage_items(&folder)
            .into_iter()
            .filter(|(full, _)| {
                let name = flavor.basename(full);
                flavor.dirname(full) == folder && SUBFOLDERS.iter().any(|(own, _)| *own == name)
            })
            .collect();
        // In the order the system lists the folder: of two older folders with the same file, the first counts.
        let inner: Vec<String> = fs::read_dir(&folder)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok()?.file_name().into_string().ok())
                    .filter(|name| !OWN_FOLDERS.contains(&name.as_str()))
                    .map(|name| flavor.join(&folder, &name))
                    .filter(|full| fs::metadata(full).is_ok_and(|m| m.is_dir()))
                    .collect()
            })
            .unwrap_or_default();
        let nested: Vec<String> = inner
            .into_iter()
            .filter(|sub| OWN_FOLDERS.iter().any(|own| fs::metadata(flavor.join(sub, own)).is_ok_and(|m| m.is_dir())))
            .collect();
        for sub in &nested {
            sources.extend(self.storage_items(sub));
        }
        let mut moved = 0;
        for (source, rel) in sources {
            let target = flavor.join(&folder, &rel);
            if fs::metadata(&target).is_ok() {
                continue;
            }
            match jsonio::make_dirs(&flavor.dirname(&target)).and_then(|()| move_file(&source, &target)) {
                Ok(()) => moved += 1,
                Err(e) => errors::record("paths.tidy_storage", &e),
            }
        }
        // Older data folders that are empty now go, only empty folders.
        for sub in &nested {
            let mut folders = Vec::new();
            self.walk_folders(sub, &mut folders);
            for empty in folders {
                let _ = remove_dir(&empty);
            }
        }
        moved
    }
}

/// `shutil.move` for a file: renamed, or copied and the original removed where renaming fails (another drive).
fn move_file(source: &str, target: &str) -> io::Result<()> {
    writeguard::check("move", source)?;
    writeguard::check("move", target)?;
    if fs::rename(source, target).is_ok() {
        return Ok(());
    }
    jsonio::copy2(source, target)?;
    jsonio::remove_file(source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::given::{Given, GivenBy};
    use crate::paths::Host;
    use crate::pypath::Flavor;
    use std::path::Path;

    fn scratch(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("versekit-storage-{name}-{}", crate::testdir::run_tag()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.to_string_lossy().into_owned()
    }

    fn put(path: &str, text: &str) {
        fs::create_dir_all(Path::new(path).parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn join(a: &str, b: &str) -> String {
        Flavor::native().join(a, b)
    }

    /// A machine whose home, configuration and data folder lie in `root`
    /// (so nothing real is touched), not marked as given from outside.
    fn machine(root: &str) -> Paths {
        let config = join(root, "AppData");
        let vars = [("USERPROFILE", root), ("HOME", root), ("APPDATA", config.as_str()), ("XDG_CONFIG_HOME", &config)];
        Paths::new(Host::new(Flavor::native(), &vars, root))
    }

    /// The data folder of `machine` filled like a used installation.
    fn filled(p: &Paths) -> String {
        let folder = p.app_folder();
        put(&join(&folder, "Bauplaene/bestand.json"), "{\"a\": 1}");
        put(&join(&folder, "Bauplaene/Unter/tief.txt"), "tief");
        put(&join(&folder, "Einstellungen/phrasen.json"), "{}");
        put(&join(&folder, "Intern/bild.png"), "PNG");
        put(&join(&folder, "Diagnose/fehler.json"), "[]");
        put(&join(&folder, "export/liste.csv"), "a;b");
        put(&join(&folder, "merkliste.json"), "[]");
        put(&join(&folder, "notiz.txt"), "n");
        put(&join(&folder, "fremd.docx"), "nicht unser");
        put(&join(&folder, "Bauplaene/halb.json.tmp"), "halb");
        folder
    }

    fn rels(items: &[(String, String)]) -> Vec<String> {
        let mut out: Vec<String> = items.iter().map(|(_, r)| r.replace('\\', "/")).collect();
        out.sort();
        out
    }

    #[test]
    fn own_files_are_the_subfolders_and_loose_json_and_txt() {
        let root = scratch("items");
        let p = machine(&root);
        let folder = filled(&p);
        assert_eq!(
            rels(&p.storage_items(&folder)),
            [
                "Bauplaene/Unter/tief.txt",
                "Bauplaene/bestand.json",
                "Bauplaene/merkliste.json",
                "Diagnose/fehler.json",
                "Einstellungen/phrasen.json",
                "Intern/bild.png",
                "Intern/notiz.txt",
                "export/liste.csv",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
            "loose files go to their place, .tmp and foreign types are left out"
        );
    }

    #[test]
    fn a_target_that_takes_nothing_is_not_writable() {
        let root = scratch("status");
        let p = machine(&root);
        let file = join(&root, "ist-eine-datei");
        put(&file, "x");
        let status = p.storage_status(&join(&file, "unter"));
        assert!(!status.writable && !status.reason.is_empty(), "{status:?}");
        let fine = p.storage_status(&join(&root, "neu"));
        assert_eq!(fine, StorageStatus { writable: true, files: 0, reason: String::new() });
        assert!(!Path::new(&join(&join(&root, "neu"), PROBE_NAME)).exists(), "the probe is removed");
    }

    #[test]
    fn nested_folders_are_found_in_both_directions() {
        let root = scratch("nested");
        let p = machine(&root);
        let (a, b) = (join(&root, "x"), join(&join(&root, "x"), "y"));
        assert!(p.nested(&a, &b) && p.nested(&b, &a) && p.nested(&a, &a));
        assert!(!p.nested(&a, &join(&root, "xy")), "a longer name is not inside");
    }

    #[test]
    fn taking_along_copies_checks_and_clears() {
        let root = scratch("move");
        let p = machine(&root);
        let old = filled(&p);
        let target = join(&root, "Neu");
        assert_eq!(p.plan_storage_move(&target), Plan::OfferMove { own: 8 });
        let outcome = p.apply_storage_move(&target, Choice::TakeAlong);
        let Outcome::Moved { copied, kept, .. } = outcome else { panic!("{outcome:?}") };
        assert_eq!((copied, kept.len()), (8, 0));
        assert_eq!(p.app_folder(), target, "the new place is in effect");
        assert_eq!(fs::read_to_string(join(&target, "Bauplaene/bestand.json")).unwrap(), "{\"a\": 1}");
        assert_eq!(fs::read_to_string(join(&target, "Bauplaene/merkliste.json")).unwrap(), "[]");
        assert!(!Path::new(&join(&old, "Bauplaene/bestand.json")).exists(), "the old files are cleared");
        assert!(!Path::new(&join(&old, "Intern")).exists(), "and the old subfolders that are empty now");
        assert!(Path::new(&join(&old, "Bauplaene/halb.json.tmp")).exists(), "a half written file stays");
        assert!(Path::new(&join(&old, "fremd.docx")).exists(), "foreign files stay");
        assert!(Path::new(&old).exists(), "so the old folder stays too");
    }

    #[test]
    fn a_file_that_cannot_be_checked_keeps_everything_where_it_was() {
        let root = scratch("broken");
        let p = machine(&root);
        let old = filled(&p);
        let target = join(&root, "Neu");
        // A copy that cuts the content off: the check must catch it.
        let breaks = |from: &str, to: &str| -> io::Result<()> {
            let bytes = fs::read(from)?;
            fs::write(to, &bytes[..bytes.len().saturating_sub(1)])
        };
        let moved = p.move_storage_with(&old, &target, &breaks);
        assert_eq!((moved.copied, moved.failed), (0, 8), "{moved:?}");
        assert!(rels(&p.storage_items(&target)).is_empty(), "the half copies are removed");
        assert_eq!(p.storage_items(&old).len(), 8, "the old folder is whole");
    }

    #[test]
    fn a_failed_move_does_not_switch() {
        let root = scratch("failed");
        let p = machine(&root);
        let old = filled(&p);
        let target = join(&root, "Neu");
        // The folder `Bauplaene` at the target is a file: nothing can be copied into it.
        put(&join(&target, "Bauplaene"), "ich bin eine Datei");
        let outcome = p.apply_storage_move(&target, Choice::TakeAlong);
        let Outcome::Failed { failed, copied } = outcome else { panic!("{outcome:?}") };
        assert!(failed > 0 && copied > 0, "{failed} {copied}");
        assert_eq!(p.app_folder(), old, "not switched");
        assert!(Path::new(&join(&old, "Bauplaene/bestand.json")).exists(), "and not cleared");
    }

    #[test]
    fn existing_files_at_the_target_are_kept_and_the_differing_old_ones_stay() {
        let root = scratch("occupied");
        let p = machine(&root);
        let old = filled(&p);
        let target = join(&root, "Neu");
        put(&join(&target, "Bauplaene/bestand.json"), "{\"anders\": 2}");
        assert_eq!(p.plan_storage_move(&target), Plan::Occupied { files: 1 });
        let moved = p.move_storage(&old, &target);
        assert_eq!((moved.copied, moved.skipped, moved.failed), (7, 1, 0));
        assert_eq!(fs::read_to_string(join(&target, "Bauplaene/bestand.json")).unwrap(), "{\"anders\": 2}");
        let (removed, kept) = p.remove_old_storage(&old, &target);
        assert_eq!((removed, kept.len()), (7, 1), "{kept:?}");
        assert!(Path::new(&join(&old, "Bauplaene/bestand.json")).exists(), "the old file is the way back");
    }

    #[test]
    fn a_file_missing_at_the_target_is_copied_before_it_goes() {
        let root = scratch("changed");
        let p = machine(&root);
        let old = filled(&p);
        let target = join(&root, "Neu");
        p.move_storage(&old, &target);
        // Nothing at the target for this one: the clearing copies it first.
        fs::remove_file(join(&target, "Intern/bild.png")).unwrap();
        let (removed, kept) = p.remove_old_storage(&old, &target);
        assert_eq!((removed, kept.len()), (8, 0));
        assert_eq!(fs::read_to_string(join(&target, "Intern/bild.png")).unwrap(), "PNG");
    }

    #[test]
    fn a_folder_written_with_slashes_and_a_slash_at_the_end_moves_like_any_other() {
        // tkinter hands out `D:/Daten/` on Windows; the relative names must not become absolute.
        let root = scratch("slashes");
        let p = machine(&root);
        let old = filled(&p);
        let slashed = format!("{}/", old.replace('\\', "/"));
        let items = p.storage_items(&slashed);
        assert_eq!(items.len(), 8, "{items:?}");
        assert!(items.iter().all(|(_, rel)| !Flavor::native().isabs(rel)), "{items:?}");
        let target = join(&root, "Neu");
        let moved = p.move_storage(&slashed, &target);
        assert_eq!((moved.copied, moved.skipped, moved.failed), (8, 0, 0));
        let (removed, kept) = p.remove_old_storage(&slashed, &target);
        assert_eq!((removed, kept.len()), (8, 0), "{kept:?}");
        assert_eq!(fs::read_to_string(join(&target, "Bauplaene/bestand.json")).unwrap(), "{\"a\": 1}");
        assert!(!Path::new(&join(&old, "Bauplaene/bestand.json")).exists());
    }

    #[test]
    fn a_target_inside_the_old_folder_or_around_it_is_refused() {
        let root = scratch("around");
        let p = machine(&root);
        let old = filled(&p);
        for target in [join(&old, "Unter"), root.clone()] {
            assert_eq!(p.plan_storage_move(&target), Plan::Nested, "{target}");
            assert_eq!(p.apply_storage_move(&target, Choice::TakeAlong), Outcome::Nested);
            assert_eq!(p.app_folder(), old, "not switched");
        }
        assert!(!Path::new(&join(&old, "Unter")).exists(), "not even created");
    }

    #[test]
    fn a_half_copy_is_removed_and_the_next_try_copies_again() {
        let root = scratch("half");
        let p = machine(&root);
        let old = filled(&p);
        let target = join(&root, "Neu");
        let breaks = |from: &str, to: &str| -> io::Result<()> {
            fs::write(to, &fs::read(from)?[..1])?;
            Err(io::Error::other("Platte voll"))
        };
        let moved = p.move_storage_with(&old, &target, &breaks);
        assert_eq!((moved.copied, moved.failed), (0, 8), "{moved:?}");
        assert!(p.storage_items(&target).is_empty(), "no half copy is left");
        let again = p.move_storage(&old, &target);
        assert_eq!((again.copied, again.skipped, again.failed), (8, 0, 0), "{again:?}");
    }

    #[test]
    fn a_path_to_the_same_file_is_never_copied_over_or_cleared() {
        let root = scratch("alias");
        let p = machine(&root);
        let old = filled(&p);
        let target = join(&root, "Neu");
        let (from, to) = (join(&old, "Bauplaene/bestand.json"), join(&target, "Bauplaene/bestand.json"));
        fs::create_dir_all(Path::new(&to).parent().unwrap()).unwrap();
        fs::hard_link(&from, &to).expect("a hard link in the temp folder");
        let moved = p.move_storage(&old, &target);
        assert_eq!((moved.copied, moved.skipped, moved.failed), (7, 0, 1), "{moved:?}");
        let (removed, kept) = p.remove_old_storage(&old, &target);
        assert_eq!((removed, kept), (7, vec![join("Bauplaene", "bestand.json")]));
        assert_eq!(fs::read_to_string(&from).unwrap(), "{\"a\": 1}", "the file is still there");
    }

    #[test]
    fn no_data_means_a_quiet_switch() {
        let root = scratch("quiet");
        let p = machine(&root);
        let target = join(&root, "Neu");
        assert_eq!(p.plan_storage_move(&target), Plan::NothingToTake);
        assert_eq!(p.apply_storage_move(&target, Choice::SwitchOnly), Outcome::Switched);
        assert_eq!(p.app_folder(), target);
        assert_eq!(p.plan_storage_move(&target), Plan::Same);
    }

    #[test]
    fn a_fixed_data_folder_is_never_moved() {
        let root = scratch("given");
        let p = machine(&root);
        let old = filled(&p);
        p.set_given_data(&old, GivenBy::Named("Test".into()));
        let target = join(&root, "Neu");
        assert_eq!(p.plan_storage_move(&target), Plan::Given);
        for choice in [Choice::TakeAlong, Choice::SwitchOnly, Choice::UseExisting] {
            assert_eq!(p.apply_storage_move(&target, choice), Outcome::Given);
        }
        assert!(!Path::new(&target).exists(), "the target is not even created");
        assert_eq!((p.app_folder(), p.storage_items(&old).len()), (old, 8), "nothing changed");
    }

    #[test]
    fn the_environment_and_a_copy_fix_the_data_folder_too() {
        let root = scratch("fixed");
        let mut host = machine(&root).host.clone();
        host.set_var(crate::legacy::ENV_HOME, &join(&root, "Umgebung"));
        let given = Paths::new(host).given();
        assert_eq!(
            given.data.as_ref().map(|d| d.by.clone()),
            Some(GivenBy::Environment(crate::legacy::ENV_HOME.to_string())),
            "{given:?}"
        );
        assert_eq!(Paths::new(machine(&root).host.clone()).given(), Given::default(), "nothing fixed on its own");
        let copy = scratch("copy");
        fs::create_dir_all(join(&copy, crate::paths::COPY_DATA_FOLDER)).unwrap();
        let paths = Paths::for_copy(machine(&root).host.clone(), &copy).expect("copy");
        let fixed = paths.given().data.expect("fixed");
        assert_eq!((fixed.by, fixed.folder), (GivenBy::CommandLine, paths.app_folder()));
    }

    #[test]
    fn the_pointer_under_documents_is_cut_and_kept() {
        let root = scratch("pointer");
        let p = machine(&root);
        let old = p.app_folder();
        assert!(old.starts_with(&root), "{old}");
        put(&join(&old, "Bauplaene/bestand.json"), "{}");
        let pointer = p.pointer_file();
        // The settings file is the pointer file: it carries other fields, too.
        put(&pointer, "{\"sprache\": \"de\"}");
        let target = join(&root, "Neu");
        let Outcome::Moved { .. } = p.apply_storage_move(&target, Choice::TakeAlong) else { panic!() };
        assert_eq!(p.read_pointer(&pointer).as_deref(), Some(target.as_str()));
        let text = fs::read_to_string(&pointer).unwrap();
        assert_eq!(text.matches("sprache").count(), 0, "cut to the one field: {text}");
    }

    fn files(folder: &str) -> Vec<String> {
        let mut out = Vec::new();
        Paths::new(Host::current()).walk_files(folder, "", &mut out);
        let mut names: Vec<String> = out.into_iter().map(|(_, rel)| rel.replace('\\', "/")).collect();
        names.sort();
        names
    }

    #[test]
    fn loose_own_files_go_to_their_subfolders_and_foreign_ones_stay() {
        let root = scratch("tidy-loose");
        let p = machine(&root);
        let folder = join(&root, "Ablage");
        put(&join(&folder, "bestand.json"), "lose");
        put(&join(&folder, "einstellungen.json"), "{}");
        put(&join(&folder, "auftragslog.json"), "[]");
        put(&join(&folder, "merkliste.json"), "neu");
        put(&join(&folder, "Bauplaene/merkliste.json"), "in Gebrauch");
        put(&join(&folder, "notiz.txt"), "nicht in der Liste");
        put(&join(&folder, "fremd.json"), "nicht unser");
        assert_eq!(p.tidy_storage(Some(&folder)), 3);
        assert_eq!(
            files(&folder),
            [
                "Bauplaene/bestand.json",
                "Bauplaene/merkliste.json",
                "Einstellungen/einstellungen.json",
                "Intern/auftragslog.json",
                "fremd.json",
                "merkliste.json",
                "notiz.txt",
            ]
        );
        assert_eq!(fs::read_to_string(join(&folder, "Bauplaene/merkliste.json")).unwrap(), "in Gebrauch");
        assert_eq!(p.tidy_storage(Some(&folder)), 0, "nothing left to do");
        // Written with a separator at the end, nothing loose counts.
        put(&join(&folder, "phrasen.json"), "{}");
        assert_eq!(p.tidy_storage(Some(&format!("{folder}{}", Flavor::native().sep()))), 0);
    }

    #[test]
    fn an_older_data_folder_inside_is_emptied_into_this_one_and_removed_where_empty() {
        let root = scratch("tidy-nested");
        let p = machine(&root);
        let folder = join(&root, "Ablage");
        let old = join(&folder, crate::legacy::DOCUMENTS_FOLDER);
        put(&join(&old, "Bauplaene/bestand.json"), "{\"a\": 1}");
        put(&join(&old, "Bauplaene/halb.json.tmp"), "halb");
        put(&join(&old, "Intern/tief/bild.png"), "PNG");
        put(&join(&old, "merkliste.json"), "[]");
        put(&join(&old, "unbekannt.json"), "{}");
        let gone = join(&folder, "Alt2");
        put(&join(&gone, "Einstellungen/phrasen.json"), "{}");
        put(&join(&folder, "Fotos/urlaub.json"), "nicht unser");
        assert_eq!(p.tidy_storage(Some(&folder)), 5);
        assert_eq!(
            files(&folder),
            [
                "Bauplaene/bestand.json",
                "Bauplaene/merkliste.json",
                "Einstellungen/phrasen.json",
                "Fotos/urlaub.json",
                "Intern/tief/bild.png",
                "Intern/unbekannt.json",
            ]
        );
        assert!(!Path::new(&gone).exists(), "an emptied folder goes");
        assert!(Path::new(&join(&old, "Bauplaene/halb.json.tmp")).exists(), "a half written file stays");
        assert!(!Path::new(&join(&old, "Intern")).exists());
    }

    #[test]
    fn with_the_data_folder_named_from_outside_nothing_is_tidied() {
        let root = scratch("tidy-named");
        let mut host = machine(&root).host;
        let named = join(&root, "eigen");
        host.set_var(crate::legacy::ENV_HOME, &named);
        let p = Paths::new(host);
        put(&join(&named, "bestand.json"), "{}");
        assert_eq!(p.tidy_storage(None), 0);
        assert_eq!(files(&named), ["bestand.json"]);
        assert_eq!(p.tidy_storage(Some(&named)), 1, "a folder given counts");
    }

    #[test]
    fn a_locked_target_leaves_the_file_where_it_is_and_says_so() {
        let root = scratch("tidy-locked");
        let p = machine(&root);
        let folder = join(&root, "Ablage");
        put(&join(&folder, "bestand.json"), "{}");
        writeguard::deny(true);
        writeguard::allow_under(None);
        let moved = p.tidy_storage(Some(&folder));
        let denied = writeguard::take_denied();
        writeguard::deny(false);
        assert_eq!(moved, 0);
        assert!(!denied.is_empty());
        assert_eq!(files(&folder), ["bestand.json"]);
    }
}
