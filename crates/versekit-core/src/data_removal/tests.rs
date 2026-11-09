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

use std::path::{Path, PathBuf};

use super::*;
use crate::paths::Host;
use crate::pypath::Flavor;
use crate::storage_move::OWN_FOLDERS;
use crate::testdir::Dir;

/// A machine of its own below `root`: home with Documents, `APPDATA`, `LOCALAPPDATA`, no registry.
fn machine(root: &Path) -> Paths {
    let text = |p: PathBuf| p.to_string_lossy().into_owned();
    let home = text(root.join("home"));
    let appdata = text(root.join("appdata"));
    let local = text(root.join("local"));
    fs::create_dir_all(root.join("home").join("Dokumente")).unwrap();
    let vars = [
        ("USERPROFILE", home.as_str()),
        ("HOME", home.as_str()),
        ("APPDATA", appdata.as_str()),
        ("XDG_CONFIG_HOME", appdata.as_str()),
        ("LOCALAPPDATA", local.as_str()),
    ];
    Paths::new(Host::new(Flavor::native(), &vars, &text(root.to_path_buf())))
}

fn put(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn pointer_to(path: &str, folder: &str) {
    let value = serde_json::json!({ "ablage_ordner": folder });
    put(Path::new(path), &value.to_string());
}

/// Both pointers at `folder`.
fn point_at(paths: &Paths, folder: &Path) {
    pointer_to(&paths.pointer_file(), &folder.to_string_lossy());
    pointer_to(&paths.second_pointer(), &folder.to_string_lossy());
}

/// The mark of the program in a data folder: its settings file as the setup writes it.
fn mark(folder: &Path) {
    let settings = serde_json::json!({ "sprache": "de", crate::setup::VERSION_KEY: 5 });
    put(&folder.join("Einstellungen").join(SETTINGS_FILE), &settings.to_string());
}

/// A link to a folder that needs no rights: a junction under Windows, a symbolic link elsewhere.
fn link_folder(link: &Path, target: &Path) {
    #[cfg(windows)]
    {
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "junction made");
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).unwrap();
}

/// Everything below `root`, relative, with `/`, sorted; a folder ends in `/`, a link in `@`.
fn listing(root: &Path) -> Vec<String> {
    fn walk(base: &Path, folder: &Path, out: &mut Vec<String>) {
        for entry in fs::read_dir(folder).unwrap().flatten() {
            let path = entry.path();
            let rel = path.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
            let kind = entry.file_type().unwrap();
            if kind.is_symlink() {
                out.push(format!("{rel}@"));
            } else if kind.is_dir() {
                out.push(format!("{rel}/"));
                walk(base, &path, out);
            } else {
                out.push(rel);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

/// Files and folders with the names of the program, or nearly its names, that are not the program's.
const LOOK_ALIKES: [&str; 27] = [
    "bericht.txt",
    "watchlist.json",
    "gesehen.json",
    ".schreibprobe",
    "Einstellungen/einstellungen.ini",
    "Intern/geheim.txt",
    "Intern/routen.privat.json",
    "Intern/preise.2025.json",
    "Intern/notizen.privat.json",
    "Intern/basetool-meins.json",
    "Intern/injektion-urtext@meins.json",
    "Intern/vor-update/meins.zip",
    "Intern/signatur-bilder/Urlaub.png",
    "Intern/handle-bilder/0123456789abcdef.png",
    "Bauplaene/Hausplan.pdf",
    "Bauplaene/bestand.privat.json",
    "Bauplaene/fehler.json",
    "Diagnose/Befund.pdf",
    "Diagnose/bericht.2026.txt",
    "export/patch-liste.json",
    "export/scmdb-import.json",
    "export/Ältere/scmdb-import-2026-09-13.json",
    "Patches/patch-1.json",
    "Sicherungen/Verse-Kit-Sicherung-vor-VerseKit-5-Kopie.zip",
    "vor-update/vor-update-2026-11-01-120000-5.2.0.zip",
    "Leer/",
    "Intern/Leer/",
];

fn put_look_alikes(folder: &Path) {
    for rel in LOOK_ALIKES {
        match rel.strip_suffix('/') {
            Some(dir) => fs::create_dir_all(folder.join(dir)).unwrap(),
            None => put(&folder.join(rel), "fremd"),
        }
    }
}

#[test]
fn the_own_data_go_at_every_place_and_everything_else_stays() {
    let dir = Dir::new("daten-loeschen");
    let root = dir.path();
    let paths = machine(root);
    let documents = root.join("home").join("Dokumente");

    // The data folder the user chose, marked as the program's, in a folder that holds other things too.
    let chosen = root.join("Spiele").join("Daten");
    mark(&chosen);
    for own in [
        "Bauplaene/bestand.json",
        "Bauplaene/bestand.bak.json",
        "Intern/katalog-cache.json",
        "Intern/vor-update/vor-update-2026-11-01-120000-5.2.0.zip",
        "Diagnose/fehler.json",
        "Sicherungen/Verse-Kit-Sicherung-vor-VerseKit-5-2026-11-01-120000.zip",
        "Sicherungen/Verse-Kit-Sicherung-vor-VerseKit-5-2026-11-01-120000-2.zip",
        "Einstellungen/einstellungen.bak.json",
        "Einstellungen/einstellungen.kaputt-20261101-120000.json",
        "Einstellungen/einstellungen.kaputt-20261101-120000-2.json",
        "Bauplaene/bestand.json.4711-1.tmp",
        "Intern/injektion-urtext@PTU.json",
        "Intern/signatur-bilder/123_20261101-120000-042.png",
        "Intern/signatur-gemerkt/0001762000000000.bin",
        "Intern/handle-bilder/0123456789abcdef.bild",
        "Intern/raffinerie-bilder/raffinerie_20261101-120000-042_kopf.png",
    ] {
        put(&chosen.join(own), "x");
    }
    let export = serde_json::json!({ "exportSchemaVersion": 1, "productName": "VerseKit" }).to_string();
    put(&chosen.join("export").join("scmdb-import.json"), &export);
    put(&chosen.join("export").join("Ältere").join("scmdb-import-2026-09-13.json"), &export);
    put(&chosen.join("Patches").join("patch-4.3.json"), "{\"version\": \"4.3\", \"quelle\": \"erkul.games\"}");
    put(&chosen.join("Intern").join("basetool-inst_7.json"), "{\"format\": 1, \"installation_id\": \"inst_7\"}");
    // Foreign files beside them, also in the own subfolders, and loose files with the program's names at the top
    // (the program writes none there).
    for foreign in [
        "Urlaub.jpg",
        "notizen.txt",
        "bestand.json",
        "export/meine-liste.xlsx",
        "Patches/Lies mich.md",
        "Fotos/a.png",
        "Intern/geheim.txt",
        "Bauplaene/Hausplan.pdf",
        "Sicherungen/Steuer 2026.zip",
    ] {
        put(&chosen.join(foreign), "fremd");
    }
    fs::create_dir_all(chosen.join("export").join("Eigenes")).unwrap();
    // A file marked read-only is the program's too.
    let locked = chosen.join("Bauplaene").join("merkliste.json");
    put(&locked, "x");
    let mut permissions = fs::metadata(&locked).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&locked, permissions).unwrap();
    // A link in an own folder: what it leads to is not the program's.
    let elsewhere = root.join("Woanders");
    put(&elsewhere.join("wichtig.txt"), "fremd");
    link_folder(&chosen.join("Intern").join("verknuepft"), &elsewhere);

    // The default folders under Documents: the one of older installations (it holds the visible pointer) and the
    // new one.
    fs::create_dir_all(documents.join(legacy::DOCUMENTS_FOLDER)).unwrap();
    point_at(&paths, &chosen);
    put(&documents.join(NEW_FOLDER_NAME).join("Bauplaene").join("bestand.json"), "x");
    put(&documents.join("Brief.docx"), "fremd");

    // The configuration folder, the secrets, the fetched installers.
    let config = Path::new(&paths.second_pointer()).parent().unwrap().to_path_buf();
    put(&config.join("hochwasser.json"), "{}");
    put(&config.join("sperre-abc.lock"), "");
    put(&root.join("local").join("VerseKit").join("basetool").join("schluessel.bin"), "x");
    put(&root.join("local").join("VerseKit").join("Update").join("VerseKit-Setup.exe"), "x");
    // The program folder of an installation beside them is not touched.
    put(&root.join("local").join("Programs").join("VerseKit").join(legacy::PROGRAM_FILE), "x");
    put(&root.join("appdata").join("Anderes").join("x.json"), "fremd");

    let removal = remove(&paths).expect("not refused");
    assert!(removal.failed.is_empty(), "{:?}", removal.failed);
    assert_eq!(removal.exit_code(), 0);

    let program = format!("local/Programs/VerseKit/{}", legacy::PROGRAM_FILE);
    assert_eq!(
        listing(root),
        [
            "Spiele/",
            "Spiele/Daten/",
            "Spiele/Daten/Bauplaene/",
            "Spiele/Daten/Bauplaene/Hausplan.pdf",
            "Spiele/Daten/Fotos/",
            "Spiele/Daten/Fotos/a.png",
            "Spiele/Daten/Intern/",
            "Spiele/Daten/Intern/geheim.txt",
            "Spiele/Daten/Intern/verknuepft@",
            "Spiele/Daten/Patches/",
            "Spiele/Daten/Patches/Lies mich.md",
            "Spiele/Daten/Sicherungen/",
            "Spiele/Daten/Sicherungen/Steuer 2026.zip",
            "Spiele/Daten/Urlaub.jpg",
            "Spiele/Daten/bestand.json",
            "Spiele/Daten/export/",
            "Spiele/Daten/export/Eigenes/",
            "Spiele/Daten/export/meine-liste.xlsx",
            "Spiele/Daten/notizen.txt",
            "Woanders/",
            "Woanders/wichtig.txt",
            "appdata/",
            "appdata/Anderes/",
            "appdata/Anderes/x.json",
            "home/",
            "home/Dokumente/",
            "home/Dokumente/Brief.docx",
            "local/",
            "local/Programs/",
            "local/Programs/VerseKit/",
            program.as_str(),
        ],
        "own data gone, everything else as it was"
    );
    assert_eq!(fs::read_to_string(chosen.join("Intern").join("verknuepft").join("wichtig.txt")).unwrap(), "fremd");
}

#[test]
fn a_folder_the_pointer_names_without_a_mark_is_not_touched_whatever_lies_in_it() {
    let dir = Dir::new("daten-loeschen-fremd");
    let root = dir.path();
    let paths = machine(root);
    // The desktop, and the root of another disk, as a pointer may name them: files and folders with the names of
    // the program, none of them its own.
    for folder in [root.join("home").join("Desktop"), root.join("E")] {
        put_look_alikes(&folder);
        // A settings file that is not the program's: one key of the template is not enough.
        put(&folder.join("Einstellungen").join(SETTINGS_FILE), "{\"sprache\": \"de\"}");
        point_at(&paths, &folder);
        let before = listing(&folder);
        assert!(!marked(&paths, &folder.to_string_lossy()));
        let removal = remove(&paths).unwrap();
        assert!(removal.places.iter().all(|p| !same(&paths, p, &folder.to_string_lossy())), "{:?}", removal.places);
        assert_eq!(listing(&folder), before, "nothing of {folder:?} touched");
    }
}

#[test]
fn in_a_marked_folder_only_the_files_of_the_program_go_and_the_look_alikes_stay() {
    let dir = Dir::new("daten-loeschen-gewaehlt");
    let root = dir.path();
    let paths = machine(root);
    // The root of a disk the user chose as the data folder: the program's mark, and beside it files and folders
    // with the program's folder names that are not its own.
    let chosen = root.join("E");
    put_look_alikes(&chosen);
    put(&chosen.join("Urlaub.jpg"), "fremd");
    let before = listing(&chosen);
    mark(&chosen);
    point_at(&paths, &chosen);
    remove(&paths).unwrap();
    // Only the settings file that is the mark goes: every look-alike stays, by name, by place or by content, and so
    // do the folders it never makes and the data folder itself.
    assert_eq!(listing(&chosen), before);
}

#[test]
fn the_files_that_need_their_content_go_only_with_the_programs_content() {
    let dir = Dir::new("daten-loeschen-inhalt");
    let root = dir.path();
    let paths = machine(root);
    let chosen = root.join("E");
    mark(&chosen);
    let export = serde_json::json!({ "exportSchemaVersion": 1, "productName": "VerseKit", "blueprints": [] });
    let full = serde_json::json!({ "werkzeug": legacy::EXPORT_TOOL_NAME, "bauplaene": [] });
    let full_name = format!("export/{}.json", legacy::EXPORT_FULL);
    let own = [
        ("Patches/patch-4.3.0-LIVE.1.json", serde_json::json!({ "version": "4.3.0-LIVE.1", "quelle": "erkul.games" })),
        ("export/scmdb-import.json", export.clone()),
        ("export/Ältere/scmdb-import-2026-09-13.json", export),
        (full_name.as_str(), full),
        ("export/SC-Blueprints-Basetool.json", serde_json::json!({ "blueprints": [] })),
        ("Intern/basetool-inst_7.json", serde_json::json!({ "format": 1, "installation_id": "inst_7" })),
    ];
    for (rel, value) in &own {
        put(&chosen.join(rel), &value.to_string());
    }
    let contracts_name = format!("export/{}.json", legacy::EXPORT_CONTRACTS);
    let foreign = [
        // A patch of another source, and one whose version is not its name.
        ("Patches/patch-1.json", serde_json::json!({ "version": "1", "quelle": "meine-notizen" })),
        ("Patches/patch-2.json", serde_json::json!({ "version": "3", "quelle": "erkul.games" })),
        // An export name with content of somebody else.
        (contracts_name.as_str(), serde_json::json!({ "liste": [] })),
        // A state of the Basetool that names another installation.
        ("Intern/basetool-inst_8.json", serde_json::json!({ "format": 1, "installation_id": "inst_7" })),
    ];
    for (rel, value) in &foreign {
        put(&chosen.join(rel), &value.to_string());
    }
    point_at(&paths, &chosen);
    remove(&paths).unwrap();
    for (rel, _) in &own {
        assert!(!chosen.join(rel).exists(), "{rel} goes");
    }
    for (rel, _) in &foreign {
        assert!(chosen.join(rel).is_file(), "{rel} stays");
    }
}

#[test]
fn the_mark_needs_the_key_of_the_setup_two_keys_of_the_template_or_a_whole_collection() {
    let dir = Dir::new("daten-loeschen-merkmal");
    let root = dir.path();
    let paths = machine(root);
    let folder = root.join("Daten");
    let text = folder.to_string_lossy().into_owned();
    let settings = folder.join("Einstellungen").join(SETTINGS_FILE);
    let collection = folder.join("Bauplaene").join(crate::collection::FILE_NAME);
    assert!(!marked(&paths, &text), "nothing there");
    for (content, expected) in [
        ("{\"sprache\": \"de\"}", false),
        ("{\"sprache\": \"de\", \"farbe\": 1}", false),
        // One key twice is still one key.
        ("{\"sprache\": \"de\", \"sprache\": \"en\"}", false),
        ("[\"sprache\", \"signalton\"]", false),
        ("{kaputt", false),
        ("{\"sprache\": \"de\", \"signalton\": true}", true),
        ("{\"einrichtung_version\": 5}", true),
    ] {
        put(&settings, content);
        assert_eq!(marked(&paths, &text), expected, "{content}");
    }
    fs::remove_file(&settings).unwrap();
    for (content, expected) in [("{\"bauplaene\": {}}", false), ("{\"version\": 3, \"bauplaene\": {}}", true)] {
        put(&collection, content);
        assert_eq!(marked(&paths, &text), expected, "{content}");
    }
    assert_eq!(crate::setup::VERSION_KEY, "einrichtung_version");
}

#[test]
fn the_known_files_are_the_names_of_the_program_and_the_copies_it_makes_beside_them() {
    for name in [
        "bestand.json",
        "Bestand.JSON",
        "bestand.bak.json",
        "bestand.json.tmp",
        "bestand.json.4711-12.tmp",
        "bestand.bak.json.4711-1.tmp",
        "einstellungen.kaputt-20261101-120000.json",
        "einstellungen.kaputt-20261101-120000-3.json",
        "update-helfer.1.txt",
        "injektion-urtext@PTU.json",
        "absturz-overlay.txt",
    ] {
        assert!(known_file(name), "{name}");
    }
    for name in [
        "bestandsliste.json",
        "fehler-alt.json",
        "einstellungen.ini",
        "Hausplan.pdf",
        "orte.txt",
        "x.json",
        // Nearly a copy of the program's files, but not in a form it writes.
        "routen.meine-liste.json",
        "routen.privat.json",
        "preise.2025.json",
        "notizen.privat.json",
        "bericht.2026.txt",
        "bestand.privat.json",
        "einstellungen.kaputt-gestern.json",
        "bestand.json.alt.tmp",
        // The state of the Basetool needs its content; a channel that is none.
        "basetool-inst_7.json",
        "injektion-urtext@meins.json",
    ] {
        assert!(!known_file(name), "{name}");
    }
}

/// What the question of the uninstaller says: in the program's own subfolders a file with the program's exact
/// name goes whatever is in it (the program made those folders and writes those names); everything else stays.
#[test]
fn an_exact_name_of_the_program_in_its_own_subfolder_goes_whatever_its_content() {
    let dir = Dir::new("daten-loeschen-genauer-name");
    let root = dir.path();
    let paths = machine(root);
    let chosen = root.join("Dokumente-gewaehlt");
    mark(&chosen);
    put(&chosen.join("Intern").join("preise.json"), "meine eigene Preisliste");
    put(&chosen.join("Intern").join("preise-meine.json"), "meine eigene Preisliste");
    point_at(&paths, &chosen);
    remove(&paths).unwrap();
    assert!(!chosen.join("Intern").join("preise.json").exists(), "the exact name of the program goes");
    assert!(chosen.join("Intern").join("preise-meine.json").is_file(), "another name stays");
}

#[test]
fn a_file_too_large_to_be_the_programs_is_not_read_and_stays() {
    let dir = Dir::new("daten-loeschen-gross");
    let root = dir.path();
    let paths = machine(root);
    let chosen = root.join("E");
    mark(&chosen);
    // The content of a state of the Basetool, padded beyond what is read.
    let padding = "x".repeat(usize::try_from(LARGEST_READ).unwrap());
    let large = serde_json::json!({ "format": 1, "installation_id": "inst_9", "rest": padding }).to_string();
    put(&chosen.join("Intern").join("basetool-inst_9.json"), &large);
    let small = serde_json::json!({ "format": 1, "installation_id": "inst_10" }).to_string();
    put(&chosen.join("Intern").join("basetool-inst_10.json"), &small);
    point_at(&paths, &chosen);
    remove(&paths).unwrap();
    assert!(chosen.join("Intern").join("basetool-inst_9.json").is_file(), "over the limit: not read, stays");
    assert!(!chosen.join("Intern").join("basetool-inst_10.json").exists(), "within it: read, the program's, goes");
}

#[test]
fn temporary_files_of_exports_and_patches_go_and_the_export_keys_count_in_any_order() {
    let nowhere = Path::new("gibt-es-nicht");
    let full_temporary = format!("{}.json.9-1.tmp", legacy::EXPORT_FULL);
    for name in ["scmdb-import.json.tmp", "scmdb-import.json.4711-2.tmp", full_temporary.as_str()] {
        assert!(owned_in_common(crate::export::FOLDER, false, name, nowhere), "{name}");
    }
    for name in ["patch-4.3.0-LIVE.1.json.tmp", "patch-4.3.json.4711-1.tmp"] {
        assert!(owned_in_common(crate::patch_changes::FOLDER, false, name, nowhere), "{name}");
    }
    for name in ["meine-liste.json.tmp", "scmdb-import.tmp", "patch-liste.txt.tmp"] {
        assert!(!owned_in_common(crate::export::FOLDER, false, name, nowhere), "{name}");
        assert!(!owned_in_common(crate::patch_changes::FOLDER, false, name, nowhere), "{name}");
    }
    let dir = Dir::new("daten-loeschen-reihenfolge");
    let contracts = format!("{}.json", legacy::EXPORT_CONTRACTS);
    let file = dir.path().join(&contracts);
    for (content, expected) in [
        ("{\"auftraege\": [], \"format\": 2}", true),
        ("{\"format\": 2, \"auftraege\": []}", true),
        ("{\"blueprints\": []}", true),
        ("{\"format\": 2, \"auftraege\": [], \"mehr\": 1}", false),
        ("{\"auftraege\": []}", false),
    ] {
        fs::write(&file, content).unwrap();
        assert_eq!(owned_in_common(crate::export::FOLDER, false, &contracts, &file), expected, "{content}");
    }
}

#[test]
fn a_known_name_goes_only_in_the_subfolder_the_program_puts_it_in() {
    let nowhere = Path::new("gibt-es-nicht");
    assert!(owned_named("Bauplaene", "bestand.json", nowhere));
    assert!(owned_named("Diagnose", "fehler.json", nowhere));
    assert!(owned_named("Intern", "katalog-cache.json", nowhere));
    assert!(owned_named("Intern", "injektion-urtext@LIVE.json", nowhere));
    assert!(!owned_named("Bauplaene", "fehler.json", nowhere));
    assert!(!owned_named("Intern", "bestand.json", nowhere));
    assert!(!owned_named("Diagnose", "katalog-cache.json", nowhere));
}

/// Every file name the source of the program names in a constant is either a file of the program in its data
/// folder, known to the removal, or named here as none: a new file of the program cannot be forgotten.
#[test]
fn every_file_name_of_the_program_is_known_or_named_as_no_data() {
    // Names that are no data of the program in its own subfolders, each with where it lies.
    const NO_DATA: [&str; 6] = [
        // The attachment inside a backup file.
        "sicherung.txt",
        // Left in the temporary folder by the update of 4.12.
        legacy::UPDATE_SETUP_LOG,
        // The checksums beside a release.
        "SHA256SUMS.txt",
        // Shipped with the program, read from its own folder.
        "signatur-ziffern.json",
        // The note of the original texts beside a translated file in the game folder.
        ".urtext.json",
        // A file of the parity checks in their scratch folders.
        "contracts.json",
    ];
    let pattern =
        regex::Regex::new(r#"const [A-Z_0-9]+: &(?:'static )?str = "([^"]+\.(?:json|txt|bin|log|zip))""#).unwrap();
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut unknown = Vec::new();
    let mut seen = 0;
    let mut stack: Vec<PathBuf> = fs::read_dir(crates).unwrap().flatten().map(|e| e.path().join("src")).collect();
    while let Some(folder) = stack.pop() {
        for (path, kind) in entries(&folder) {
            if kind.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = fs::read_to_string(&path).unwrap();
                for found in pattern.captures_iter(&text) {
                    let name = &found[1];
                    seen += 1;
                    if !name.contains("://") && !known_file(name) && !NO_DATA.contains(&name) {
                        unknown.push(format!("{}: {name}", path.display()));
                    }
                }
            }
        }
    }
    assert!(seen > 100, "the source was read ({seen} names)");
    assert!(unknown.is_empty(), "add these to KNOWN_FILES or NO_DATA: {unknown:#?}");
}

#[test]
fn a_data_folder_on_the_network_is_named_but_not_looked_into_or_touched() {
    let dir = Dir::new("daten-loeschen-netz");
    let root = dir.path();
    let paths = machine(root);
    let share = r"\\server-den-es-nicht-gibt\freigabe\VerseKit";
    pointer_to(&paths.pointer_file(), share);
    assert!(on_network(share) && on_network("//server/freigabe"));
    assert!(!on_network(&root.to_string_lossy()));
    let found = places(&paths).unwrap();
    assert!(found.contains(&Place { path: share.to_string(), kind: Kind::Network }), "{found:?}");
    assert!(places_text(&found).lines().any(|line| line == format!("{NETWORK_MARK}{share}")));
    let removal = remove(&paths).unwrap();
    assert!(!removal.places.iter().any(|p| p == share));
}

#[test]
fn an_empty_chosen_data_folder_stays_and_documents_and_the_home_always_stay() {
    let dir = Dir::new("daten-loeschen-leer");
    let root = dir.path();
    let paths = machine(root);
    let documents = root.join("home").join("Dokumente");
    let chosen = root.join("Daten");
    mark(&chosen);
    point_at(&paths, &chosen);
    remove(&paths).unwrap();
    assert!(chosen.is_dir(), "a folder the user chose stays, even empty");
    assert_eq!(listing(&chosen), Vec::<String>::new());
    // The user picked Documents itself.
    mark(&documents);
    point_at(&paths, &documents);
    remove(&paths).unwrap();
    assert!(documents.is_dir() && root.join("home").is_dir());
    assert!(!documents.join("Einstellungen").exists());
}

#[test]
fn a_pointer_that_is_relative_holds_a_variable_is_broken_or_leads_to_a_link_names_no_place() {
    let dir = Dir::new("daten-loeschen-zeiger");
    let root = dir.path();
    let paths = machine(root);
    let real = root.join("Echt");
    mark(&real);
    put(&real.join("Bauplaene").join("bestand.json"), "{\"bauplaene\": {}}");
    let linked = root.join("Verknuepft");
    link_folder(&linked, &real);
    let first = paths.pointer_file();
    for value in ["Echt", "%USERPROFILE%\\Echt", &linked.to_string_lossy()] {
        pointer_to(&first, value);
        let found = places(&paths).unwrap();
        assert!(found.iter().all(|p| p.kind != Kind::ChosenData), "{value}: {found:?}");
    }
    put(Path::new(&first), "{kaputt");
    assert!(places(&paths).unwrap().iter().all(|p| p.kind != Kind::ChosenData));
    let before = listing(&real);
    remove(&paths).unwrap();
    assert_eq!(listing(&real), before, "the folder behind no usable pointer stays as it is");
}

#[test]
fn the_places_are_the_ones_cleared_and_the_mark_of_a_collection_counts_too() {
    let dir = Dir::new("daten-loeschen-orte");
    let root = dir.path();
    let paths = machine(root);
    let chosen = root.join("Sammlung");
    put(&chosen.join("Bauplaene").join("bestand.json"), "{\"version\": 3, \"bauplaene\": {}}");
    point_at(&paths, &chosen);
    let config = Path::new(&paths.second_pointer()).parent().unwrap().to_path_buf();
    put(&root.join("local").join("VerseKit").join("Update").join("VerseKit-Setup.exe"), "x");
    let found = places(&paths).unwrap();
    // The visible pointer lies in the default folder under Documents, which comes first.
    let default = root.join("home").join("Dokumente").join(NEW_FOLDER_NAME);
    let expected = vec![
        Place { path: default.to_string_lossy().into_owned(), kind: Kind::NamedData },
        Place { path: chosen.to_string_lossy().into_owned(), kind: Kind::ChosenData },
        Place { path: config.to_string_lossy().into_owned(), kind: Kind::Program },
        Place {
            path: root.join("local").join("VerseKit").join("Update").to_string_lossy().into_owned(),
            kind: Kind::Program,
        },
    ];
    assert_eq!(found, expected);
    assert_eq!(places_text(&found).lines().count(), 4);
    assert_eq!(remove(&paths).unwrap().places.len(), 4);
}

#[test]
fn a_folder_of_secrets_named_by_the_variable_is_not_touched() {
    let dir = Dir::new("daten-loeschen-geheim");
    let root = dir.path();
    let mut paths = machine(root);
    let own_choice = root.join("Tresor").join("basetool");
    put(&own_choice.join("schluessel.bin"), "x");
    paths.host.set_var(legacy::ENV_SECRETS, &own_choice.to_string_lossy());
    let removal = remove(&paths).unwrap();
    assert!(own_choice.join("schluessel.bin").is_file());
    assert!(removal.places.is_empty(), "{:?}", removal.places);
}

#[cfg(windows)]
#[test]
fn a_file_that_cannot_be_removed_is_told_by_the_exit_code() {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = Dir::new("daten-loeschen-offen");
    let root = dir.path();
    let paths = machine(root);
    let folder = root.join("home").join("Dokumente").join(NEW_FOLDER_NAME);
    let open = folder.join("Diagnose").join("fehler.json");
    put(&open, "x");
    put(&folder.join("Bauplaene").join("bestand.json"), "x");
    // Held open without the right to delete it, as the running program holds its log.
    let holder = fs::OpenOptions::new().read(true).share_mode(0).open(&open).unwrap();
    let removal = remove(&paths).unwrap();
    assert_eq!(removal.failed, [open.to_string_lossy().into_owned()]);
    assert_eq!(removal.exit_code(), 1);
    assert!(!folder.join("Bauplaene").exists(), "the rest went");
    drop(holder);
}

#[test]
fn a_data_folder_named_from_outside_is_refused_and_nothing_is_touched() {
    let dir = Dir::new("daten-loeschen-benannt");
    let root = dir.path();
    let mut paths = machine(root);
    let named = root.join("benannt");
    put(&named.join("bestand.json"), "x");
    put(&root.join("home").join("Dokumente").join(NEW_FOLDER_NAME).join("Bauplaene").join("bestand.json"), "x");
    let before = listing(root);
    paths.host.set_var(legacy::ENV_HOME, &named.to_string_lossy());
    assert_eq!(remove(&paths), Err(Refused::NamedFromOutside));
    assert_eq!(places(&paths), Err(Refused::NamedFromOutside));
    assert_eq!(listing(root), before);
}

#[test]
fn without_data_nothing_is_created() {
    let dir = Dir::new("daten-loeschen-nichts");
    let root = dir.path();
    let paths = machine(root);
    let before = listing(root);
    assert!(places(&paths).unwrap().is_empty());
    let removal = remove(&paths).unwrap();
    assert_eq!((removal.removed, removal.failed.len()), (0, 0));
    assert_eq!(listing(root), before, "no folder made, no file written");
}

#[test]
fn on_a_copy_of_the_data_only_the_copy_is_touched() {
    let dir = Dir::new("daten-loeschen-kopie");
    let root = dir.path();
    let copy = root.join("kopie");
    put(&copy.join(COPY_DATA_FOLDER).join("Bauplaene").join("bestand.json"), "x");
    // The machine the copy runs on: its secrets and installers lie outside the copy.
    let local = root.join("local");
    put(&local.join("VerseKit").join("basetool").join("schluessel.bin"), "x");
    put(&local.join("VerseKit").join("Update").join("VerseKit-Setup.exe"), "x");
    let base = Host::new(Flavor::native(), &[("LOCALAPPDATA", &local.to_string_lossy())], &root.to_string_lossy());
    writeguard::deny(true);
    writeguard::allow_under(Some(&copy.to_string_lossy()));
    let paths = Paths::for_copy(base, &copy.to_string_lossy()).expect("a copy");
    let removal = remove(&paths).unwrap();
    writeguard::deny(false);
    let denied = writeguard::take_denied();
    assert!(denied.is_empty(), "nothing tried outside the copy: {denied:?}");
    assert!(removal.failed.is_empty(), "{:?}", removal.failed);
    assert!(!copy.join(COPY_DATA_FOLDER).exists(), "the data of the copy are gone");
    assert!(local.join("VerseKit").join("basetool").join("schluessel.bin").is_file(), "the machine's secrets stay");
    assert!(local.join("VerseKit").join("Update").join("VerseKit-Setup.exe").is_file());
}

#[test]
fn the_folders_emptied_are_exactly_the_own_folders_of_the_data_folder() {
    let mut named: Vec<&str> = NAMED_FOLDERS.iter().chain(COMMON_FOLDERS.iter()).copied().collect();
    // The own subfolders a move takes along, and the folder of the backup of the first start.
    let mut own = OWN_FOLDERS.to_vec();
    own.push(crate::first_backup::FOLDER);
    named.sort_unstable();
    own.sort_unstable();
    assert_eq!(named, own, "a new own folder must be named here too");
}
