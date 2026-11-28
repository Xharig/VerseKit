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

//! The plugin folder `com.xharig.versekit.sdPlugin` and the file `VerseKit.streamDeckPlugin` (the folder as a zip
//! archive), made from the catalogue of the actions so that the manifest and the program cannot drift apart.
//!
//! The folder holds the manifest, a translation of it for German and English, the symbols of the actions and the
//! keys, the settings page of a key, this program, the license of the Lucide symbols, and the shipped profiles that
//! lie in `streamdeck-plugin/profiles` of the repository.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use versekit_core::language::{self, Lang};
use versekit_core::streamdeck::{self, Action};

use crate::paint;
use crate::plugin::{PROFILE_DEVICES, PROFILE_EDITION, profile_base, profile_name, shipped_profile};
use crate::render::{Look, Tone, list_symbol};

/// The licence of Lato, whose letters the key pictures carry.
const LATO_LICENSE: &str = include_str!("../../versekit-ui/fonts/OFL.txt");

/// The name of the plugin folder.
pub const FOLDER: &str = "com.xharig.versekit.sdPlugin";
/// The name of the program in the folder.
pub const PROGRAM: &str = "versekit-streamdeck.exe";
const INSPECTOR: &str = include_str!("inspector.html");
const LUCIDE_LICENSE: &str = "ISC License\n\nCopyright (c) for portions of Lucide are held by Cole Bemis 2013-2022 as part of Feather (MIT). All other copyright (c) for Lucide are held by Lucide Contributors 2022.\n\nPermission to use, copy, modify, and/or distribute this software for any purpose with or without fee is hereby granted, provided that the above copyright notice and this permission notice appear in all copies.\n\nTHE SOFTWARE IS PROVIDED \"AS IS\" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.\n";

/// The layout of a dial on the touch strip: one picture over the whole field, drawn by the plugin.
pub const DIAL_LAYOUT_FILE: &str = "layouts/regler.json";
/// The key of that picture in `setFeedback`.
pub const DIAL_PICTURE: &str = "bild";

/// The layout file of the dials.
pub fn dial_layout() -> Value {
    json!({
        "id": format!("{}.regler", streamdeck::PLUGIN_UUID),
        "items": [{
            "key": DIAL_PICTURE,
            "type": "pixmap",
            "rect": [0, 0, paint::DIAL_WIDTH, paint::DIAL_HEIGHT],
            "zOrder": 0,
        }],
    })
}

/// Actions with a settings page.
fn has_inspector(action: Action) -> bool {
    matches!(action, Action::Page | Action::GameAction | Action::Emote)
}

/// Actions that follow the key down and up, which a multi action does not pass on.
fn follows_the_key(action: Action) -> bool {
    matches!(action, Action::GameAction | Action::Emote)
}

/// The numbers of a version without a pre-release or build part: `5.7.0-beta.1` gives `5.7.0`.
pub fn numbers_of(version: &str) -> &str {
    version.split(['-', '+']).next().unwrap_or(version)
}

/// The version of the plugin: the numbers of the version of VerseKit with the time of its commit as the fourth number
/// (see `build.rs`), so the Stream Deck program takes every newer build as an update. The manifest takes numbers only;
/// a pre-release and its release share the first three, and the later commit counts higher.
pub fn version() -> String {
    format!("{}.{}", numbers_of(env!("CARGO_PKG_VERSION")), env!("VERSEKIT_PLUGIN_BUILD"))
}

/// A profile file of an edition (see [`PROFILE_EDITION`]): its name without extension, the kind of device and the
/// language.
fn profile_of(name: &str, edition: u32) -> Option<(u64, Lang)> {
    PROFILE_DEVICES.iter().find_map(|(kind, base, _)| {
        [Lang::De, Lang::En].into_iter().find(|lang| name == shipped_profile(base, *lang, edition)).map(|lang| (*kind, lang))
    })
}

/// The name a source profile (`streamdeck-plugin/profiles`, always named as the first edition) has in the package of
/// an edition; `None` for a file that is no profile of a known device.
pub fn packed_profile(source: &str, edition: u32) -> Option<String> {
    let (kind, lang) = profile_of(source, 1)?;
    Some(shipped_profile(profile_base(kind)?, lang, edition))
}

/// The device model written in a profile file (`Device.Model` of the manifest at the top of the archive).
pub fn profile_model(file: &[u8]) -> Option<String> {
    let entries = versekit_core::zip::entries(file).ok()?;
    let top = entries.iter().find(|e| {
        let mut parts = e.name.split('/');
        matches!((parts.next(), parts.next(), parts.next()), (Some(folder), Some("manifest.json"), None) if folder.ends_with(".sdProfile"))
    })?;
    let text = versekit_core::zip::read(file, top).ok()?;
    let value: Value = serde_json::from_slice(&text).ok()?;
    value["Device"]["Model"].as_str().map(String::from)
}

/// Is `text` a UUID (`8-4-4-4-12` hex digits)?
fn is_uuid(text: &[u8]) -> bool {
    text.len() == 36
        && text.iter().enumerate().all(|(i, b)| if [8, 13, 18, 23].contains(&i) { *b == b'-' } else { b.is_ascii_hexdigit() })
}

/// The UUID a shipped profile carries for `old`: derived from it (SHA-256 with a fixed word), so every build gives
/// the same and never the one of the source. Upper or lower case as `old`.
pub fn shipped_uuid(old: &str) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, format!("versekit-plugin-profil:{}", old.to_ascii_lowercase()).as_bytes());
    let mut b = [0u8; 16];
    b.copy_from_slice(&digest.as_ref()[..16]);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    let uuid = format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..]);
    if old.bytes().any(|c| c.is_ascii_uppercase()) { uuid.to_ascii_uppercase() } else { uuid }
}

/// Every UUID in `text` replaced by [`shipped_uuid`].
fn replace_uuids(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        if i + 36 <= bytes.len()
            && is_uuid(&bytes[i..i + 36])
            && (i == 0 || !bytes[i - 1].is_ascii_hexdigit())
            && bytes.get(i + 36).is_none_or(|b| !b.is_ascii_hexdigit())
        {
            out.push_str(&shipped_uuid(&text[i..i + 36]));
            i += 36;
        } else {
            let c = text[i..].chars().next().unwrap_or('\0');
            out.push(c);
            i += c.len_utf8();
        }
    }
    out
}

/// The profile file with UUIDs of its own (profile, pages, actions; in the names of its files and in its JSON): a
/// profile of the same source installed before (the testers' ZIP) must not meet the same UUIDs when the plugin
/// installs its profile.
pub fn with_own_uuids(file: &[u8]) -> std::io::Result<Vec<u8>> {
    let entries = versekit_core::zip::entries(file).map_err(|e| std::io::Error::other(e.0))?;
    let mut zip = versekit_core::zip::Builder::new((2026, 1, 1, 0, 0, 0));
    for entry in &entries {
        let bytes = versekit_core::zip::read(file, entry).map_err(|e| std::io::Error::other(e.0))?;
        let bytes = if entry.name.ends_with(".json") {
            replace_uuids(&String::from_utf8_lossy(&bytes)).into_bytes()
        } else {
            bytes
        };
        zip.add(&replace_uuids(&entry.name), &bytes).map_err(|e| std::io::Error::other(e.0))?;
    }
    zip.finish().map_err(|e| std::io::Error::other(e.0))
}

/// Why a source profile file (named as the first edition) may not be shipped; `None` when it may.
pub fn profile_problem(name: &str, file: &[u8]) -> Option<String> {
    let Some((kind, _)) = profile_of(name, 1) else { return Some(format!("{name}: no known kind of device")) };
    let wanted = PROFILE_DEVICES.iter().find(|(k, _, _)| *k == kind).and_then(|(_, _, model)| *model);
    let found = profile_model(file);
    match (wanted, found) {
        (Some(wanted), Some(found)) if wanted == found => None,
        (wanted, found) => Some(format!("{name}: device model {found:?}, kind {kind} wants {wanted:?}")),
    }
}

/// The manifest for the profiles `profiles` (file names without extension).
pub fn manifest(profiles: &[String]) -> Value {
    manifest_of(profiles, PROFILE_EDITION)
}

/// The manifest for the profiles `profiles` of an edition (file names without extension, as [`packed_profile`] gives).
fn manifest_of(profiles: &[String], edition: u32) -> Value {
    let en = |key: &str| language::t_in(Lang::En, key);
    let actions: Vec<Value> = Action::ALL
        .iter()
        .map(|action| {
            let id = action.id();
            let mut entry = json!({
                "UUID": action.uuid(),
                "Name": en(&action.label_key()),
                "Tooltip": en(&action.tooltip_key()),
                "Icon": format!("images/actions/{id}"),
                "States": [{"Image": format!("images/keys/{id}"), "ShowTitle": false}],
                "Controllers": match action {
                    Action::GameAction | Action::Emote | Action::Contested | Action::Wallet | Action::Health => {
                        json!(["Keypad", "Encoder"])
                    }
                    Action::Objective | Action::Status | Action::Ship => json!(["Encoder"]),
                    _ => json!(["Keypad"]),
                },
                "SupportedInMultiActions": !follows_the_key(*action),
            });
            if has_inspector(*action) {
                entry["PropertyInspectorPath"] = json!("ui/inspector.html");
            }
            // The circle of a dial in the editor shows its own symbol, without writing: else the program takes the
            // picture of the key, with the English name.
            if let Some(texts) = trigger_description(*action, Lang::En) {
                entry["Encoder"] =
                    json!({"layout": DIAL_LAYOUT_FILE, "TriggerDescription": texts, "Icon": format!("images/dials/{id}")});
            }
            entry
        })
        .collect();
    let profiles: Vec<Value> = profiles
        .iter()
        .filter_map(|name| {
            let (kind, _) = profile_of(name, edition)?;
            Some(json!({
                "Name": profile_name(name),
                "DeviceType": kind,
                "Readonly": false,
                "DontAutoSwitchWhenInstalled": true,
                "AutoInstall": false,
            }))
        })
        .collect();
    json!({
        "Name": "VerseKit",
        "Version": version(),
        "Author": "Xharig",
        "Description": en("sd_m_beschreibung"),
        "URL": "https://versekit.xharig.com",
        "UUID": streamdeck::PLUGIN_UUID,
        "Icon": "images/plugin",
        "Category": "VerseKit",
        "CategoryIcon": "images/category",
        "CodePathWin": PROGRAM,
        "SDKVersion": 2,
        "OS": [{"Platform": "windows", "MinimumVersion": "10"}],
        // `AutoInstall` of the profiles needs 6.6.
        "Software": {"MinimumVersion": "6.6"},
        "Actions": actions,
        "Profiles": profiles,
    })
}

/// The translation of the manifest into one language.
pub fn localization(lang: Lang) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("Name".into(), json!("VerseKit"));
    out.insert("Description".into(), json!(language::t_in(lang, "sd_m_beschreibung")));
    out.insert("Category".into(), json!("VerseKit"));
    for action in Action::ALL {
        let mut entry = json!({
            "Name": language::t_in(lang, &action.label_key()),
            "Tooltip": language::t_in(lang, &action.tooltip_key()),
        });
        // The texts of the turn and the press of a dial: the Stream Deck program takes them from here, by action.
        if let Some(texts) = trigger_description(action, lang) {
            entry["Encoder"] = json!({ "TriggerDescription": texts });
        }
        out.insert(action.uuid(), entry);
    }
    // Every English text of the manifest with its word in the language, for the texts the plugin translates itself;
    // the Stream Deck program takes the entries of the actions above.
    let mut words = serde_json::Map::new();
    let keys = Action::ALL.iter().flat_map(|a| [a.label_key(), a.tooltip_key()]).chain(trigger_text_keys().into_iter().map(String::from));
    for key in keys.chain(std::iter::once("sd_m_beschreibung".to_string())) {
        words.insert(language::t_in(Lang::En, &key), json!(language::t_in(lang, &key)));
    }
    out.insert("Localization".into(), Value::Object(words));
    Value::Object(out)
}

/// The text keys of the turn (where turning does something) and the press of a dial of `action`; `None` for an action
/// that sits on keys only.
fn trigger_keys(action: Action) -> Option<(Option<&'static str>, &'static str)> {
    Some(match action {
        Action::GameAction | Action::Emote => (Some("sd_m_drehen"), "sd_m_druecken"),
        Action::Objective => (Some("sd_m_auftrag_drehen"), "sd_m_auftrag_druecken"),
        Action::Contested => (None, "sd_m_contested_druecken"),
        Action::Wallet => (None, "sd_m_konto_druecken"),
        Action::Ship => (None, "sd_m_schiff_druecken"),
        Action::Health => (None, "sd_m_nur_anzeige"),
        Action::Status => (None, "sd_m_status_druecken"),
        _ => return None,
    })
}

/// The texts of the turn and the press of a dial of `action` in `lang`, as `TriggerDescription` wants them.
fn trigger_description(action: Action, lang: Lang) -> Option<Value> {
    let (rotate, push) = trigger_keys(action)?;
    let mut texts = json!({ "Push": language::t_in(lang, push) });
    if let Some(rotate) = rotate {
        texts["Rotate"] = json!(language::t_in(lang, rotate));
    }
    Some(texts)
}

/// Every text key of the turns and the presses of the dials.
fn trigger_text_keys() -> Vec<&'static str> {
    let mut keys: Vec<&'static str> = Action::ALL
        .iter()
        .filter_map(|a| trigger_keys(*a))
        .flat_map(|(rotate, push)| rotate.into_iter().chain(std::iter::once(push)))
        .collect();
    keys.sort_unstable();
    keys.dedup();
    keys
}

/// The picture a key shows before the plugin draws it, or when the plugin does not run: the house style with the
/// name in English, as the manifest is English.
fn key_picture(action: Action) -> Vec<u8> {
    let look = Look { icon: action.icon(), icon_tone: Tone::Accent, label: language::t_in(Lang::En, &action.label_key()), badge: None, colour: None, hold: None };
    paint::png(&look, action)
}

/// Every file of the folder except the program and the profiles: (path in the folder, content).
pub fn files(profiles: &[String]) -> Vec<(String, Vec<u8>)> {
    let pretty = |v: &Value| serde_json::to_string_pretty(v).unwrap_or_default().into_bytes();
    let mut out = vec![
        ("manifest.json".to_string(), pretty(&manifest(profiles))),
        ("de.json".to_string(), pretty(&localization(Lang::De))),
        ("en.json".to_string(), pretty(&localization(Lang::En))),
        ("ui/inspector.html".to_string(), INSPECTOR.as_bytes().to_vec()),
        (DIAL_LAYOUT_FILE.to_string(), pretty(&dial_layout())),
        ("images/plugin.png".to_string(), key_picture(Action::Overlay)),
        ("images/category.svg".to_string(), list_symbol("layers").into_bytes()),
        ("LICENSE-lucide.txt".to_string(), LUCIDE_LICENSE.as_bytes().to_vec()),
        ("LICENSE-lato.txt".to_string(), LATO_LICENSE.as_bytes().to_vec()),
    ];
    for action in Action::ALL {
        out.push((format!("images/actions/{}.svg", action.id()), list_symbol(action.icon()).into_bytes()));
        out.push((format!("images/keys/{}.png", action.id()), key_picture(action)));
        if trigger_keys(action).is_some() {
            let c = crate::paint::style().colour_of(action);
            let colour = format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b);
            out.push((format!("images/dials/{}.svg", action.id()), crate::render::dial_symbol(action.icon(), &colour).into_bytes()));
        }
    }
    out
}

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// Writes the folder into `target` and the installable file beside it; gives the file.
///
/// # Errors
///
/// What the writing reports.
pub fn write(target: &Path) -> std::io::Result<PathBuf> {
    let folder = target.join(FOLDER);
    if folder.exists() {
        std::fs::remove_dir_all(&folder)?;
    }
    let source_profiles = repository().join("streamdeck-plugin").join("profiles");
    let mut profiles: Vec<(String, Vec<u8>)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&source_profiles) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(stem) = name.strip_suffix(".streamDeckProfile") {
                let bytes = std::fs::read(entry.path())?;
                // A profile for the wrong device would be offered to the wrong device: no package then.
                if let Some(problem) = profile_problem(stem, &bytes) {
                    return Err(std::io::Error::other(problem));
                }
                // The edition goes into the name the manifest and the switches use (see PROFILE_EDITION).
                let packed = packed_profile(stem, PROFILE_EDITION).ok_or_else(|| std::io::Error::other(format!("{stem}: no name")))?;
                profiles.push((packed, with_own_uuids(&bytes)?));
            }
        }
    }
    profiles.sort();
    let names: Vec<String> = profiles.iter().map(|(n, _)| n.clone()).collect();
    let mut all = files(&names);
    all.push((PROGRAM.to_string(), std::fs::read(std::env::current_exe()?)?));
    for (name, bytes) in profiles {
        all.push((format!("profiles/{name}.streamDeckProfile"), bytes));
    }
    // A fixed time: the same sources give the same file.
    let mut zip = versekit_core::zip::Builder::new((2026, 1, 1, 0, 0, 0));
    for (name, bytes) in &all {
        let path = folder.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, bytes)?;
        zip.add(&format!("{FOLDER}/{name}"), bytes).map_err(|e| std::io::Error::other(e.0))?;
    }
    let file = target.join("VerseKit.streamDeckPlugin");
    std::fs::write(&file, zip.finish().map_err(|e| std::io::Error::other(e.0))?)?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_manifest_names_every_action_once_with_its_files() {
        let manifest = manifest(&[]);
        let actions = manifest["Actions"].as_array().expect("actions");
        assert_eq!(actions.len(), Action::ALL.len());
        let files: Vec<String> = files(&[]).into_iter().map(|(n, _)| n).collect();
        for action in Action::ALL {
            let entry = actions.iter().find(|a| a["UUID"] == action.uuid()).expect("in the manifest");
            let icon = format!("{}.svg", entry["Icon"].as_str().expect("path"));
            assert!(files.contains(&icon), "{icon}");
            // A dial has a symbol of its own in the package, as an SVG without any writing.
            if let Some(dial) = entry["Encoder"]["Icon"].as_str() {
                let file = format!("{dial}.svg");
                assert!(files.contains(&file), "{file}");
                let (_, bytes) = super::files(&[]).into_iter().find(|(n, _)| *n == file).expect("in the package");
                let svg = String::from_utf8(bytes).expect("svg");
                assert!(svg.starts_with("<svg") && !svg.contains("<text"), "{file}: no writing");
            } else {
                assert!(entry["Encoder"].is_null(), "{}: a dial without its own symbol", action.id());
            }
            let image = format!("{}.png", entry["States"][0]["Image"].as_str().expect("image"));
            assert!(files.contains(&image), "{image}");
            if let Some(page) = entry["PropertyInspectorPath"].as_str() {
                assert!(files.contains(&page.to_string()), "{page}");
            }
            if entry["Controllers"].as_array().expect("controllers").iter().any(|c| c == "Encoder") {
                assert_eq!(entry["Encoder"]["layout"], DIAL_LAYOUT_FILE, "{}: a dial is drawn by the plugin", action.id());
                assert!(entry["Encoder"]["TriggerDescription"]["Push"].is_string(), "{}: what a press does", action.id());
            }
        }
        let dials: Vec<&str> = actions.iter().filter(|a| a["Encoder"].is_object()).map(|a| a["UUID"].as_str().expect("uuid")).collect();
        for action in [
            Action::GameAction,
            Action::Emote,
            Action::Objective,
            Action::Contested,
            Action::Status,
            Action::Wallet,
            Action::Ship,
            Action::Health,
        ] {
            assert!(dials.contains(&action.uuid().as_str()), "{} can sit on a dial", action.id());
        }
        for uuid in actions.iter().map(|a| a["UUID"].as_str().expect("uuid")) {
            assert!(Action::from_uuid(uuid).is_some(), "an action the program does not know: {uuid}");
        }
        assert_eq!(manifest["CodePathWin"], PROGRAM);
        assert_eq!(manifest["UUID"], streamdeck::PLUGIN_UUID);
        let version = manifest["Version"].as_str().expect("version");
        let parts: Vec<&str> = version.split('.').collect();
        assert!(parts.len() == 4 && parts.iter().all(|p| p.parse::<u16>().is_ok()), "four numbers: {version}");
    }

    #[test]
    fn a_pre_release_gives_the_numbers_of_its_release() {
        assert_eq!(numbers_of("5.7.0-beta.1"), "5.7.0");
        assert_eq!(numbers_of("5.7.0-rc.2+build.7"), "5.7.0");
        assert_eq!(numbers_of("5.7.0+abc"), "5.7.0");
        assert_eq!(numbers_of("5.7.0"), "5.7.0");
    }

    fn version_start() -> Value {
        let file = repository().join("streamdeck-plugin").join("version-start.json");
        serde_json::from_str(&std::fs::read_to_string(file).expect("version-start.json")).expect("json")
    }

    #[test]
    fn every_commit_gives_a_higher_plugin_version() {
        // The Stream Deck program installs over a plugin only with a higher version: the fourth number is the time of
        // the commit in steps from the start of this version, not a fixed .0.
        let version = version();
        let parts: Vec<&str> = version.split('.').collect();
        assert_eq!(parts.len(), 4, "{version}");
        assert_eq!(parts[..3].join("."), numbers_of(env!("CARGO_PKG_VERSION")));
        let build: i64 = parts[3].parse().expect("a number");
        let out = std::process::Command::new("git").args(["log", "-1", "--format=%ct", "HEAD"]).output().expect("git");
        let time: i64 = String::from_utf8_lossy(&out.stdout).trim().parse().expect("commit time");
        let start = version_start();
        let step = start["minuten_je_schritt"].as_i64().expect("step") * 60;
        if std::env::var("VERSEKIT_PLUGIN_BUILD").is_err() {
            assert_eq!(build, (time - start["start_unix"].as_i64().expect("start")) / step, "the time of the commit HEAD");
        }
        assert!(build > 0, "a build after the start of the version counts above 0: {version}");
        // Many readers of versions take at most 65535 for one part.
        assert!(build <= 65535, "{version}");
    }

    /// The day `days` after 1970-01-01 as `YYYY-MM-DD` (the civil calendar).
    fn day_of(days: i64) -> String {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = yoe + era * 400 + i64::from(month <= 2);
        format!("{year:04}-{month:02}-{day:02}")
    }

    #[test]
    fn version_start_matches_the_version_of_verse_kit() {
        let start = version_start();
        assert_eq!(
            start["version"].as_str(),
            Some(numbers_of(env!("CARGO_PKG_VERSION"))),
            "the numbers of the version of VerseKit changed: move version and start in streamdeck-plugin/version-start.json along"
        );
        let unix = start["start_unix"].as_i64().expect("start_unix");
        assert_eq!(unix % 86_400, 0, "the start is a midnight");
        let text = start["start"].as_str().expect("start");
        assert_eq!(text, format!("{}T00:00:00Z", day_of(unix / 86_400)), "start and start_unix say the same day");
        let step = start["minuten_je_schritt"].as_i64().expect("minuten_je_schritt");
        assert!(step > 0, "a step of minutes");
        assert_eq!(day_of(0), "1970-01-01");
        assert_eq!(day_of(20_454), "2026-01-01");
    }

    #[test]
    fn the_manifest_is_translated_into_german_and_english() {
        for lang in Lang::ALL {
            let local = localization(lang);
            for action in Action::ALL {
                assert_eq!(local[action.uuid()]["Name"], language::t_in(lang, &action.label_key()));
            }
        }
        assert_ne!(localization(Lang::De)["Description"], localization(Lang::En)["Description"]);
    }

    #[test]
    fn every_english_text_of_the_manifest_has_its_german_word() {
        let manifest = manifest(&[]);
        let de = localization(Lang::De);
        let words = de["Localization"].as_object().expect("words");
        let mut texts: Vec<&str> = vec![manifest["Description"].as_str().expect("description")];
        for action in manifest["Actions"].as_array().expect("actions") {
            texts.push(action["Name"].as_str().expect("name"));
            texts.push(action["Tooltip"].as_str().expect("tooltip"));
            for trigger in action["Encoder"]["TriggerDescription"].as_object().into_iter().flat_map(|t| t.values()) {
                texts.push(trigger.as_str().expect("trigger"));
            }
        }
        for text in texts {
            assert!(words.contains_key(text), "no German word for „{text}“");
        }
        assert_eq!(words["Objective"], "Auftrag");
        assert_eq!(words["Executive hangar cycle"], "Executive-Hangar-Zyklus");
        assert_eq!(words["Readiness"], "Bereitschaft");
        assert_eq!(words["Switch contract"], "Auftrag wechseln");
        let en = localization(Lang::En);
        assert_eq!(en["Localization"]["Objective"], "Objective", "English stays English");
    }

    #[test]
    fn the_turn_and_the_press_of_every_dial_are_translated_in_the_entry_of_its_action() {
        let manifest = manifest(&[]);
        let de = localization(Lang::De);
        let mut dials = 0;
        for action in manifest["Actions"].as_array().expect("actions") {
            let english = &action["Encoder"]["TriggerDescription"];
            let uuid = action["UUID"].as_str().expect("uuid");
            let german = &de[uuid]["Encoder"]["TriggerDescription"];
            if english.is_null() {
                assert!(german.is_null(), "{uuid}: no dial, no texts");
                continue;
            }
            dials += 1;
            let fields = |v: &Value| v.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()).unwrap_or_default();
            assert_eq!(fields(german), fields(english), "{uuid}: the same fields as the manifest");
            for (field, text) in german.as_object().expect("texts") {
                assert!(text.as_str().is_some_and(|t| !t.is_empty()), "{uuid}.{field}");
            }
        }
        assert!(dials >= 8, "{dials} actions on dials");
        let status = &de["com.xharig.versekit.status"]["Encoder"]["TriggerDescription"];
        assert_eq!(status["Push"], "Tasten & Stream Deck öffnen");
        let game = &de["com.xharig.versekit.spielaktion"]["Encoder"]["TriggerDescription"];
        assert_eq!((game["Rotate"].clone(), game["Push"].clone()), (json!("Aktion links oder rechts"), json!("Aktion beim Drücken")));
        assert_eq!(
            localization(Lang::En)["com.xharig.versekit.status"]["Encoder"]["TriggerDescription"],
            manifest["Actions"].as_array().expect("actions").iter().find(|a| a["UUID"] == "com.xharig.versekit.status").expect("status")["Encoder"]["TriggerDescription"],
            "English is the manifest"
        );
    }

    #[test]
    fn only_profiles_of_known_devices_go_into_the_manifest() {
        let manifest = manifest(&[
            "VerseKit-StreamDeck-Mobile-DE".into(),
            "VerseKit-StreamDeck-Neo-EN".into(),
            "VerseKit-Mobile-DE".into(),
            "Fremd".into(),
        ]);
        let profiles = manifest["Profiles"].as_array().expect("profiles");
        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[0]["DeviceType"], 3);
        assert_eq!(profiles[0]["Name"], "profiles/VerseKit-StreamDeck-Mobile-DE");
        assert_eq!(profiles[1]["DeviceType"], 9);
        assert!(profiles.iter().all(|p| p["AutoInstall"] == false));
    }

    #[test]
    fn a_new_edition_of_the_profiles_gives_every_shipped_profile_a_new_name_and_the_first_keeps_it() {
        let sources: Vec<String> = shipped().into_iter().map(|(n, _)| n).collect();
        assert!(!sources.is_empty());
        for edition in [1, 2] {
            let packed: Vec<String> = sources.iter().map(|s| packed_profile(s, edition).expect("known")).collect();
            let manifest = manifest_of(&packed, edition);
            let names: Vec<&str> = manifest["Profiles"].as_array().expect("profiles").iter().filter_map(|p| p["Name"].as_str()).collect();
            assert_eq!(names.len(), sources.len(), "edition {edition}: every profile in the manifest");
            for (source, name) in sources.iter().zip(&names) {
                let wanted = if edition == 1 { format!("profiles/{source}") } else { format!("profiles/{source}-{edition}") };
                assert_eq!(*name, wanted);
            }
            // The plugin switches to the same names as the manifest gives.
            for (kind, base, _) in PROFILE_DEVICES {
                for lang in [Lang::De, Lang::En] {
                    let name = profile_name(&shipped_profile(base, lang, edition));
                    assert!(names.contains(&name.as_str()) || !sources.iter().any(|s| packed_profile(s, 1) == Some(shipped_profile(base, lang, 1))), "{kind} {name}");
                }
            }
        }
        // Off today: the package keeps the names the installed profiles have.
        assert_eq!(packed_profile("VerseKit-StreamDeck-MK2-DE", PROFILE_EDITION).as_deref(), Some("VerseKit-StreamDeck-MK2-DE"));
        assert_eq!(packed_profile("VerseKit-StreamDeck-MK2-DE-2", 2), None, "sources are named as the first edition");
    }

    fn shipped() -> Vec<(String, Vec<u8>)> {
        let folder = repository().join("streamdeck-plugin").join("profiles");
        let mut out: Vec<(String, Vec<u8>)> = std::fs::read_dir(&folder)
            .expect("the folder of the shipped profiles")
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().strip_suffix(".streamDeckProfile")?.to_string();
                Some((name, std::fs::read(e.path()).ok()?))
            })
            .collect();
        out.sort();
        out
    }

    #[test]
    fn every_shipped_profile_belongs_to_the_device_its_name_says() {
        let profiles = shipped();
        assert!(!profiles.is_empty());
        for (name, bytes) in &profiles {
            assert_eq!(profile_problem(name, bytes), None);
        }
        // Each device with a profile has it in both languages.
        let names: Vec<&str> = profiles.iter().map(|(n, _)| n.as_str()).collect();
        for (kind, base, model) in PROFILE_DEVICES {
            let de = names.contains(&format!("{base}-DE").as_str());
            let en = names.contains(&format!("{base}-EN").as_str());
            assert_eq!(de, en, "kind {kind}: German and English go together");
            assert!(de || model.is_none(), "kind {kind}: a model is known but no profile is shipped");
        }
    }

    /// Every key of this plugin in a profile file: (action UUID, settings).
    fn plugin_keys(file: &[u8]) -> Vec<(String, Value)> {
        fn walk(value: &Value, out: &mut Vec<(String, Value)>) {
            match value {
                Value::Object(map) => {
                    if let Some(uuid) = map.get("UUID").and_then(Value::as_str)
                        && uuid.starts_with(&format!("{}.", streamdeck::PLUGIN_UUID))
                    {
                        out.push((uuid.to_string(), map.get("Settings").cloned().unwrap_or(Value::Null)));
                    }
                    map.values().for_each(|v| walk(v, out));
                }
                Value::Array(list) => list.iter().for_each(|v| walk(v, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        for entry in versekit_core::zip::entries(file).expect("zip") {
            if entry.name.ends_with("manifest.json") {
                let text = versekit_core::zip::read(file, &entry).expect("entry");
                walk(&serde_json::from_slice(&text).expect("json"), &mut out);
            }
        }
        out
    }

    #[test]
    fn the_profiles_use_only_actions_and_pages_the_plugin_knows() {
        let mut seen = 0;
        for (name, bytes) in shipped() {
            for (uuid, settings) in plugin_keys(&bytes) {
                seen += 1;
                let action = Action::from_uuid(&uuid).unwrap_or_else(|| panic!("{name}: unknown action {uuid}"));
                if action == Action::Page {
                    let page = settings["seite"].as_str().unwrap_or("");
                    assert!(streamdeck::page_allowed(page), "{name}: page {page:?}");
                }
            }
        }
        assert!(seen > 0, "the profiles carry keys of the plugin");
    }

    /// The size of a key picture: the height of its symbol and that of the first line of its writing (a row band of
    /// 20 pixels from the top of the writing, so one line and two lines compare alike). Drawn pixels by luminance:
    /// any colour, dimmed or not.
    fn sizes(png: &[u8]) -> (u32, u32) {
        let (sx, sy) = paint::SYMBOL_PLACE;
        let (wx, wy) = paint::WRITING_PLACE;
        let symbol = paint::drawn_box(png, sx, sy).1;
        let (_, height, top) = paint::drawn_box_at(png, wx.clone(), wy.clone());
        let line = if height == 0 { 0 } else { paint::drawn_box(png, wx, top..(top + 20).min(wy.end)).1 };
        (symbol, line)
    }

    /// A card picture: dark card, a filled square of `side` pixels in `colour` where the symbol stands.
    fn card(side: u32, colour: [u8; 3]) -> Vec<u8> {
        let mut picture = image::RgbaImage::from_pixel(144, 144, image::Rgba([22, 28, 40, 255]));
        let start = 46 - side / 2;
        for y in start..start + side {
            for x in 72 - side / 2..72 + side / 2 {
                picture.put_pixel(x, y, image::Rgba([colour[0], colour[1], colour[2], 255]));
            }
        }
        let mut out = std::io::Cursor::new(Vec::new());
        picture.write_to(&mut out, image::ImageFormat::Png).expect("png");
        out.into_inner()
    }

    #[test]
    fn the_measure_of_a_key_counts_size_not_colour() {
        let (green, _) = sizes(&card(52, [156, 228, 48]));
        // A darker colour than the green, an orange, the green dimmed into the card at 45 % (as aUEC).
        for colour in [[58, 107, 255], [255, 154, 60], [82, 118, 44]] {
            let (other, _) = sizes(&card(52, colour));
            assert!(other.abs_diff(green) <= 1, "same size, colour {colour:?}: {other} against {green}");
            let (small, _) = sizes(&card(36, colour));
            assert!(small + 10 < green, "a smaller symbol measures smaller, colour {colour:?}: {small} against {green}");
        }
    }

    /// The page keys (`seite`, `contested`) and the reserved aUEC key of one profile file whose picture is clearly
    /// smaller than the one the plugin draws for it (whose measures follow the Anti-AFK key of the profile, see
    /// `paint`): symbol more than 6 pixels lower, first line of writing more than 3. The number of keys measured
    /// comes along. A key of the plugin without a picture of its own is drawn by the plugin and not measured here.
    fn small_page_keys(file: &[u8]) -> (usize, Vec<String>) {
        let entries = versekit_core::zip::entries(file).expect("zip");
        let read = |name: &str| entries.iter().find(|e| e.name == name).and_then(|e| versekit_core::zip::read(file, e).ok());
        let page_keys = [Action::Page.uuid(), Action::Contested.uuid()];
        let (mut measured, mut problems) = (0, Vec::new());
        for entry in entries.iter().filter(|e| e.name.split('/').count() == 4 && e.name.ends_with("/manifest.json")) {
            let folder = entry.name.trim_end_matches("manifest.json");
            let page: Value = serde_json::from_slice(&versekit_core::zip::read(file, entry).expect("page")).expect("json");
            for controller in page["Controllers"].as_array().into_iter().flatten() {
                for (place, key) in controller["Actions"].as_object().into_iter().flatten() {
                    let uuid = key["UUID"].as_str().unwrap_or("");
                    // The reserved aUEC key is dimmed and measured like the others.
                    let reserved = key["States"][0]["Title"].as_str().is_some_and(|t| t.contains("aUEC"));
                    let page_key = page_keys.iter().any(|u| u == uuid);
                    if !page_key && !reserved {
                        continue;
                    }
                    let Some(own) = key["States"][0]["Image"].as_str().filter(|p| !p.is_empty()) else { continue };
                    let Some(picture) = read(&format!("{folder}{own}")) else { continue };
                    let (look, action) = if reserved {
                        let badge = crate::render::Badge { word: "ab SC 5.0".into(), filled: false, tone: Tone::Muted };
                        (Look { icon: Action::Wallet.icon(), icon_tone: Tone::Muted, label: "aUEC".into(), badge: Some(badge), colour: None, hold: None }, Action::Wallet)
                    } else {
                        let action = Action::from_uuid(uuid).expect("page key");
                        let seite = key["Settings"]["seite"].as_str().unwrap_or("");
                        let icon = if action == Action::Page { crate::plugin::page_icon(seite) } else { action.icon() };
                        let label = key["Name"].as_str().unwrap_or("").to_string();
                        (Look { icon, icon_tone: Tone::Accent, label, badge: None, colour: None, hold: None }, action)
                    };
                    measured += 1;
                    let (symbol, line) = sizes(&picture);
                    let (want_symbol, want_line) = sizes(&paint::png(&look, action));
                    if symbol + 6 < want_symbol || line + 3 < want_line {
                        let name = key["States"][0]["Title"].as_str().unwrap_or("").replace('\n', " ");
                        problems.push(format!(
                            "{place} {name}: symbol {symbol} (plugin {want_symbol}), first line {line} (plugin {want_line})"
                        ));
                    }
                }
            }
        }
        (measured, problems)
    }

    #[test]
    fn the_page_keys_of_the_profiles_are_as_large_as_the_keys_the_plugin_draws() {
        let mut problems = Vec::new();
        let mut measured = 0;
        for (name, bytes) in shipped() {
            let (count, found) = small_page_keys(&bytes);
            measured += count;
            problems.extend(found.into_iter().map(|p| format!("{name} {p}")));
        }
        // aUEC carries a picture of its own in every profile, so something is always measured.
        assert!(measured > 0, "page keys were measured");
        assert!(problems.is_empty(), "page keys with a small symbol or small writing:\n{}", problems.join("\n"));
    }

    /// Every UUID in the names and the JSON of a profile file, lower case.
    fn uuids_of(file: &[u8]) -> std::collections::BTreeSet<String> {
        let mut out = std::collections::BTreeSet::new();
        let mut take = |text: &str| {
            let b = text.as_bytes();
            for i in 0..b.len().saturating_sub(35) {
                if is_uuid(&b[i..i + 36]) {
                    out.insert(text[i..i + 36].to_ascii_lowercase());
                }
            }
        };
        for entry in versekit_core::zip::entries(file).expect("zip") {
            take(&entry.name);
            if entry.name.ends_with(".json") {
                take(&String::from_utf8_lossy(&versekit_core::zip::read(file, &entry).expect("entry")));
            }
        }
        out
    }

    #[test]
    fn a_shipped_profile_carries_uuids_of_its_own_and_stays_whole() {
        for (name, source) in shipped() {
            let shipped = with_own_uuids(&source).expect("own uuids");
            let (old, new) = (uuids_of(&source), uuids_of(&shipped));
            assert!(!old.is_empty(), "{name}: the source has UUIDs");
            assert!(old.is_disjoint(&new), "{name}: a UUID of the source stayed: {:?}", old.intersection(&new).next());
            assert_eq!(old.len(), new.len(), "{name}: one new UUID for each old one");
            // The same device, the same keys of the plugin, the same pages, and every build the same file.
            assert_eq!(profile_model(&shipped), profile_model(&source), "{name}");
            assert_eq!(plugin_keys(&shipped).len(), plugin_keys(&source).len(), "{name}");
            assert_eq!(versekit_core::zip::entries(&shipped).expect("zip").len(), versekit_core::zip::entries(&source).expect("zip").len());
            assert_eq!(with_own_uuids(&source).expect("again"), shipped, "{name}: the same every build");
        }
        // The default page of the profile is still one of its pages.
        let (_, mobile) = shipped().into_iter().find(|(n, _)| n == "VerseKit-StreamDeck-Mobile-DE").expect("mobile");
        let file = with_own_uuids(&mobile).expect("own");
        let entries = versekit_core::zip::entries(&file).expect("zip");
        let top = entries.iter().find(|e| e.name.split('/').count() == 2 && e.name.ends_with("manifest.json")).expect("top");
        let manifest: Value = serde_json::from_slice(&versekit_core::zip::read(&file, top).expect("top")).expect("json");
        let default = manifest["Pages"]["Default"].as_str().expect("default").to_ascii_uppercase();
        assert!(entries.iter().any(|e| e.name.contains(&format!("/Profiles/{default}/"))), "default page {default} has its folder");
    }

    #[test]
    fn a_profile_of_another_device_under_a_wrong_name_is_refused() {
        let profiles = shipped();
        let (_, mini) = profiles.iter().find(|(n, _)| n == "VerseKit-StreamDeck-Mini-DE").expect("mini");
        assert_eq!(profile_model(mini).as_deref(), Some("20GAI9901"));
        assert!(profile_problem("VerseKit-StreamDeck-XL-DE", mini).is_some());
        assert!(profile_problem("VerseKit-StreamDeck-Mini-DE", b"kein zip").is_some());
    }

    #[test]
    fn the_settings_page_has_no_text_of_its_own_and_no_foreign_address() {
        // The texts come from the plugin in the language of VerseKit; the page loads nothing from outside.
        assert!(!INSPECTOR.contains("http://") && !INSPECTOR.contains("https://"));
        assert!(!INSPECTOR.contains("<script src"));
        for word in ["Seite", "Spielaktion", "Taste"] {
            assert!(!INSPECTOR.contains(&format!(">{word}<")), "{word} written into the page");
        }
    }
}
