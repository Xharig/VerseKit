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

//! Fixed standards every page keeps, checked in the source so no page has
//! to be checked by hand. Every scan is a function, and each counter check
//! sends a made-up offence through that same function.

use std::fs;
use std::path::{Path, PathBuf};

fn sources() -> Vec<(String, String)> {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files: Vec<PathBuf> = fs::read_dir(&folder)
        .expect("src")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "rs"))
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|p| {
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            // Only the program, not its test module (single test helpers stay in).
            let text = fs::read_to_string(&p).expect("Quelltext");
            let program = text.split("#[cfg(test)]\nmod tests").next().unwrap_or_default().to_string();
            (name, program)
        })
        .collect()
}

/// Line number of a byte position.
fn line_of(text: &str, at: usize) -> usize {
    text[..at].matches('\n').count() + 1
}

/// The arguments of the call whose `(` stands at `open`, split at the
/// commas of its own level.
fn arguments(text: &str, open: usize) -> Vec<&str> {
    let mut depth = 0usize;
    let mut quoted = false;
    let mut start = open + 1;
    let mut parts = Vec::new();
    for (i, c) in text[open..].char_indices().map(|(i, c)| (open + i, c)) {
        match c {
            '"' if text.as_bytes().get(i.wrapping_sub(1)) != Some(&b'\\') => quoted = !quoted,
            _ if quoted => {}
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    parts.push(text[start..i].trim());
                    return parts;
                }
            }
            ',' if depth == 1 => {
                parts.push(text[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    parts
}

/// The text of a string literal argument, without its quotes.
fn literal(argument: &str) -> Option<&str> {
    argument.strip_prefix('"')?.strip_suffix('"')
}

/// The scroll areas allowed a bare upper bound: the sidebar and the page
/// itself (no scrolling page around them) and a fixed-size sample in the
/// element catalog.
const OWN_SCROLLING: [&str; 3] = ["\"navigation\"", "(\"seite\"", "\"liste\""];

/// Every scroll area of `text` with an upper bound but without the same
/// value as lower bound. A scroll area is read from `ScrollArea::` up to
/// its `.show`, so a list built over several statements counts as one;
/// the next scroll area ends it at the latest.
fn bare_lists(file: &str, text: &str) -> Vec<String> {
    let mut findings = Vec::new();
    let mut from = 0;
    while let Some(found) = text[from..].find("ScrollArea::") {
        let start = from + found;
        let rest = &text[start + 1..];
        let next = rest.find("ScrollArea::").map_or(text.len(), |n| start + 1 + n);
        let end = rest.find(".show").map_or(next, |n| (start + 1 + n).min(next));
        let built = &text[start..end];
        from = start + 1;
        if OWN_SCROLLING.iter().any(|id| built.contains(&format!(".id_salt({id}"))) {
            continue;
        }
        let mut at = 0;
        while let Some(max) = built[at..].find(".max_height(") {
            let open = at + max + ".max_height".len();
            let argument = arguments(built, open).first().copied().unwrap_or_default();
            at = open;
            if !built.contains(&format!(".min_scrolled_height({argument})")) {
                findings
                    .push(format!("{file}:{}: max_height({argument}) ohne min_scrolled_height", line_of(text, start)));
            }
        }
    }
    findings
}

/// A list inside a scrolling page sets its height as upper and lower
/// bound: egui otherwise shrinks it to what is left on screen, down to 64
/// points (see `layout::list_room`).
#[test]
fn every_bounded_list_also_has_its_lower_bound() {
    let findings: Vec<String> = sources().iter().flat_map(|(file, text)| bare_lists(file, text)).collect();
    assert!(findings.is_empty(), "Liste ohne Untergrenze:\n{}", findings.join("\n"));
}

#[test]
fn the_list_scan_finds_bare_lists() {
    let one_go = "let a = egui::ScrollArea::vertical().id_salt(\"x\").max_height(room).show(ui, |ui| {});";
    let two_steps = "let mut area = ScrollArea::vertical().id_salt(\"x\");\n\
                     area = area.max_height(room);\n\
                     area.show(ui, |ui| {});";
    let formatted = "ScrollArea::vertical().id_salt(format!(\"x-{n}\")).max_height(room).show(ui, |ui| {});";
    let both_ways = "ScrollArea::both().max_height(h).show(ui, |ui| {});";
    for (name, text) in [("einzeilig", one_go), ("G1", two_steps), ("G2", formatted), ("both", both_ways)] {
        assert_eq!(bare_lists("x.rs", text).len(), 1, "{name} nicht gefunden");
    }
    let kept = "let mut area = ScrollArea::vertical().max_height(room);\n\
                area = area.min_scrolled_height(room);\n\
                area.show(ui, |ui| {});";
    let own = "ScrollArea::vertical().id_salt(\"navigation\").max_height(h).show(ui, |ui| {});";
    assert!(bare_lists("x.rs", kept).is_empty(), "Untergrenze in zwei Schritten übersehen");
    assert!(bare_lists("x.rs", own).is_empty(), "Navigation darf");
}

/// The language keys that say „Auswahl zurücksetzen“ in either language.
fn reset_keys() -> Vec<String> {
    let lang = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("versekit-core").join("lang");
    let texts = |name: &str| fs::read_to_string(lang.join(name)).expect("Sprachdatei");
    let (de, en) = (texts("de.json"), texts("en.json"));
    let value_of = |json: &str, key: &str| {
        let at = json.find(&format!("\"{key}\": \""))? + key.len() + 5;
        json[at..].split('"').next().map(str::to_owned)
    };
    let words = [value_of(&de, "ff_zuruecksetzen"), value_of(&en, "ff_zuruecksetzen")];
    let mut keys = Vec::new();
    for json in [&de, &en] {
        for line in json.lines() {
            let Some((key, value)) = line.trim().split_once("\": \"") else { continue };
            let value = value.trim_end_matches(',').trim_end_matches('"');
            if words.iter().flatten().any(|w| w == value) {
                let key = key.trim_start_matches('"').to_string();
                if !keys.contains(&key) {
                    keys.push(key);
                }
            }
        }
    }
    keys
}

/// Every place outside the page head that offers „Auswahl zurücksetzen“:
/// any key with that text, anywhere but the head of `sample.rs` (the block
/// that asks whether the page is narrowed).
fn reset_outside_head(sources: &[(String, String)], keys: &[String]) -> Vec<String> {
    let mut findings = Vec::new();
    for (file, text) in sources {
        for key in keys {
            let mut from = 0;
            while let Some(found) = text[from..].find(&format!("\"{key}\"")) {
                let at = from + found;
                from = at + 1;
                let before = &text[at.saturating_sub(600)..at];
                let in_head = file == "sample.rs" && before.contains("if narrowed {");
                if !in_head {
                    findings.push(format!("{file}:{}: {key}", line_of(text, at)));
                }
            }
        }
    }
    findings
}

/// „Auswahl zurücksetzen“ stands in one place only: at the top right of the
/// page head (`sample.rs`), never inside a page.
#[test]
fn resetting_the_selection_lives_only_in_the_page_head() {
    let keys = reset_keys();
    assert!(keys.contains(&"ff_zuruecksetzen".to_string()), "Schlüssel fehlt: {keys:?}");
    let sources = sources();
    let findings = reset_outside_head(&sources, &keys);
    assert!(findings.is_empty(), "„Auswahl zurücksetzen“ außerhalb der Kopfzeile:\n{}", findings.join("\n"));
    let in_head: usize =
        sources.iter().filter(|(f, _)| f == "sample.rs").map(|(_, t)| t.matches("\"ff_zuruecksetzen\"").count()).sum();
    assert_eq!(in_head, 1, "nicht genau einmal in der Kopfzeile");
}

#[test]
fn the_reset_scan_finds_a_second_place() {
    let keys = vec!["ff_zuruecksetzen".to_string(), "xx_auswahl_weg".to_string()];
    let head = "if narrowed {\n    let word = t(\"ff_zuruecksetzen\");\n}";
    let ok = vec![("sample.rs".to_string(), head.to_string())];
    assert!(reset_outside_head(&ok, &keys).is_empty(), "Kopfzeile selbst gemeldet");
    let in_page = vec![("stock_page.rs".to_string(), "ui.button(t(\"ff_zuruecksetzen\"));".to_string())];
    let other_key = vec![("stock_page.rs".to_string(), "ui.button(t(\"xx_auswahl_weg\"));".to_string())];
    let handed_in = vec![(
        "sample.rs".to_string(),
        "let reset = t(\"ff_zuruecksetzen\");\nstock_page::show(ui, &reset);".to_string(),
    )];
    for (name, sources) in [("Seite", in_page), ("G5 anderer Schlüssel", other_key), ("G5 hineingereicht", handed_in)]
    {
        assert_eq!(reset_outside_head(&sources, &keys).len(), 1, "{name} nicht gefunden");
    }
}

/// Every icon name written into the source: `.icon("…")`, `icon: "…"`,
/// `icons::themed("…")`, `icons::image("…")` and the third argument of
/// `named_icon(…)` / `icon_button(…)`, each with its line.
fn named_icons(file: &str, text: &str) -> Vec<(String, String)> {
    let mut names = Vec::new();
    for call in [".icon(\"", "icon: \"", "icons::themed(\"", "icons::image(\""] {
        let mut from = 0;
        while let Some(found) = text[from..].find(call) {
            let start = from + found + call.len();
            from = start;
            let name = text[start..].split('"').next().unwrap_or_default();
            names.push((format!("{file}:{}", line_of(text, start)), name.to_string()));
        }
    }
    for call in ["named_icon(", "icon_button("] {
        let mut from = 0;
        while let Some(found) = text[from..].find(call) {
            let open = from + found + call.len() - 1;
            from = open + 1;
            // The definition itself names no icon.
            if text[..open].ends_with(&format!("fn {}", &call[..call.len() - 1])) {
                continue;
            }
            if let Some(name) = arguments(text, open).get(2).and_then(|a| literal(a)) {
                names.push((format!("{file}:{}", line_of(text, open)), name.to_string()));
            }
        }
    }
    names
}

fn missing_icons(file: &str, text: &str, known: &[String]) -> Vec<String> {
    named_icons(file, text)
        .into_iter()
        .filter(|(_, name)| !known.iter().any(|k| k == name))
        .map(|(at, name)| format!("{at}: {name}"))
        .collect()
}

fn icon_set() -> Vec<String> {
    let set = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("tools").join("symbol-vorlagen");
    fs::read_dir(&set)
        .expect("Symbolsatz")
        .filter_map(Result::ok)
        .filter_map(|e| e.path().file_stem().map(|s| s.to_string_lossy().into_owned()))
        .collect()
}

/// Every icon a page asks for by name is in the Lucide set of the repo
/// (`tools/symbol-vorlagen/`); a missing one is drawn as nothing.
#[test]
fn every_named_icon_exists() {
    let known = icon_set();
    assert!(!known.is_empty(), "Symbolsatz leer");
    let sources = sources();
    let seen: usize = sources.iter().map(|(f, t)| named_icons(f, t).len()).sum();
    // Far fewer means the scan itself broke.
    assert!(seen >= 40, "Scan findet kaum Symbole: {seen}");
    let findings: Vec<String> = sources.iter().flat_map(|(f, t)| missing_icons(f, t, &known)).collect();
    assert!(findings.is_empty(), "Symbol fehlt im Satz:\n{}", findings.join("\n"));
}

#[test]
fn the_icon_scan_finds_a_wrong_name() {
    let known = vec!["download".to_string(), "pencil".to_string()];
    let offences = [
        ("Knopf", "Button::new(&w, kind, s).icon(\"downlaod\")"),
        ("G3 Feld", "PageAction { icon: \"downlaod\", text: &w, on: false }"),
        ("G4 Hilfsfunktion", "named_icon(ui, scheme, \"pencl\", &t(\"s_x\"))"),
        ("icon_button", "widgets::icon_button(ui, scheme, \"pencl\", \"Ändern\")"),
    ];
    for (name, text) in offences {
        assert_eq!(missing_icons("x.rs", text, &known).len(), 1, "{name} nicht gefunden");
    }
    let fine = "named_icon(ui, scheme, \"pencil\", &t(\"s_x\")); named_icon(ui, scheme, icon, &t(\"s_y\"));";
    assert!(missing_icons("x.rs", fine, &known).is_empty(), "Bedeutung als Symbol gelesen");
    let definition = "fn named_icon(ui: &mut Ui, scheme: &Scheme, icon: &str, meaning: &str) {}";
    assert!(named_icons("x.rs", definition).is_empty(), "Definition als Aufruf gelesen");
}

/// Every `.rs` file of the workspace, tests and examples included.
fn workspace_sources() -> Vec<(String, String)> {
    fn walk(dir: &Path, out: &mut Vec<(String, String)>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push((path.display().to_string(), fs::read_to_string(&path).expect("Quelltext")));
            }
        }
    }
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut out = Vec::new();
    walk(&crates, &mut out);
    out.sort();
    out
}

/// The names of the game channels.
const CHANNEL_NAMES: [&str; 5] = ["\"LIVE\"", "\"HOTFIX\"", "\"PTU\"", "\"EPTU\"", "\"TECH-PREVIEW\""];

/// The places that may write the channels out, each with its reason.
const OWN_CHANNEL_LISTS: [(&str, &str); 3] = [
    // The one list.
    ("versekit-core/src/paths.rs", "CHANNELS, TEST_CHANNELS und FOLLOWS_LIVE"),
    // Folder names to ids, the longer names first so that EPTU is not read as PTU.
    ("versekit-gamedata/src/lib.rs", "Zuordnung Ordnername zu Kennung"),
    // The rows of the menu filter, checked against the one list by a test of their own.
    ("versekit-ui/src/sample.rs", "Zeilen der Tabelle PAGE_SETTINGS"),
];

/// Lists of channels defined outside the one list: a `const` or `static` whose value names two channels or more.
fn own_channel_lists(file: &str, text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for keyword in ["const ", "static "] {
        for (at, _) in text.match_indices(keyword) {
            // The value after `=` up to its `;` (the type before it may hold one: `[&str; 3]`).
            let rest = &text[at..];
            let Some(equals) = rest.find('=') else { continue };
            let value = &rest[equals..];
            let value = &value[..value.find(';').unwrap_or(value.len())];
            if CHANNEL_NAMES.iter().filter(|name| value.contains(*name)).count() >= 2 {
                found.push(format!("{file}:{}", line_of(text, at)));
            }
        }
    }
    found
}

/// Every page and part takes the channels from `paths::CHANNELS`: no second list that could miss a channel.
#[test]
fn the_channels_come_from_one_list() {
    let mut found = Vec::new();
    for (file, text) in workspace_sources() {
        let unix = file.replace('\\', "/");
        let is_test = unix.contains("/tests/") || unix.ends_with("tests.rs") || unix.ends_with("/tests.rs");
        if is_test || OWN_CHANNEL_LISTS.iter().any(|(own, _)| unix.ends_with(own)) {
            continue;
        }
        found.extend(own_channel_lists(&unix, &text));
    }
    assert!(found.is_empty(), "Eigene Kanalliste statt paths::CHANNELS:\n{}", found.join("\n"));
}

#[test]
fn the_channel_scan_finds_a_second_list() {
    let second = "pub const CHANNEL_ROWS: [&str; 3] = [\"LIVE\", \"PTU\", \"TECH-PREVIEW\"];";
    assert_eq!(own_channel_lists("x.rs", second).len(), 1);
    let one = "const CHANNEL_ROWS: [&str; 5] = versekit_core::paths::CHANNELS;\nconst NAME: &str = \"LIVE\";";
    assert!(own_channel_lists("x.rs", one).is_empty(), "the one list or a single name is no list");
}

/// The first line of every file's licence header keeps its dash.
const HEADER: &str = "// Verse-Kit — companion for Star Citizen.";

/// The files of `sources` that do not begin with the licence head (GPL-3.0-only, the whole text).
fn without_licence_head(sources: &[(String, String)]) -> Vec<String> {
    const SPDX: &str = "// SPDX-License-Identifier: GPL-3.0-only";
    sources
        .iter()
        .filter(|(_, text)| {
            let head: Vec<&str> = text.lines().take(16).collect();
            head.first().copied() != Some(HEADER) || !head.contains(&SPDX) || !head.iter().any(|l| l.contains("GNU General Public License"))
        })
        .map(|(file, _)| file.clone())
        .collect()
}

/// Every source file of the workspace, test files included, begins with the licence head.
#[test]
fn every_source_file_begins_with_the_licence_head() {
    let sources = workspace_sources();
    assert!(sources.len() > 100, "only {} files found", sources.len());
    let missing = without_licence_head(&sources);
    assert!(missing.is_empty(), "Quelltext ohne Lizenzkopf:\n{}", missing.join("\n"));
}

#[test]
fn the_licence_scan_finds_a_file_without_the_head() {
    let head = "// Verse-Kit — companion for Star Citizen.\n// Copyright (C) 2026 Xharig\n//\n// SPDX-License-Identifier: GPL-3.0-only\n//\n// This program is free software: you can redistribute it and/or modify\n// it under the terms of the GNU General Public License as published by\n";
    let files = vec![
        ("ok.rs".to_string(), format!("{head}use x;")),
        ("none.rs".to_string(), "use super::*;\n".to_string()),
        ("other.rs".to_string(), head.replace("GPL-3.0-only", "MIT")),
        ("doc.rs".to_string(), format!("//! The tests.\n{head}")),
    ];
    assert_eq!(without_licence_head(&files), ["none.rs", "other.rs", "doc.rs"]);
}

/// Where a `//` comment starts in `line`, outside string literals.
fn comment_start(line: &str) -> Option<usize> {
    let mut quoted = false;
    let bytes = line.as_bytes();
    for i in 0..bytes.len() {
        match bytes[i] {
            b'"' if i == 0 || bytes[i - 1] != b'\\' => quoted = !quoted,
            b'/' if !quoted && bytes.get(i + 1) == Some(&b'/') => return Some(i),
            _ => {}
        }
    }
    None
}

/// Whether `text` names the author as a word, not the company „Roberts
/// Space Industries“.
fn names_a_person(text: &str) -> bool {
    text.match_indices("Robert").any(|(at, _)| {
        let rest = &text[at + "Robert".len()..];
        let before = text[..at].chars().next_back();
        !before.is_some_and(char::is_alphanumeric)
            && !rest.starts_with("s Space")
            && !rest.starts_with(char::is_alphanumeric)
    })
}

/// Comment lines with a long dash or a person's name.
fn comment_findings(file: &str, text: &str) -> Vec<String> {
    let mut findings = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if line.trim() == HEADER {
            continue;
        }
        let Some(at) = comment_start(line) else { continue };
        let comment = &line[at..];
        if comment.contains('—') || names_a_person(comment) {
            findings.push(format!("{file}:{}: {}", n + 1, comment.trim()));
        }
    }
    findings
}

/// Comments say in plain words what the code does: no long dashes, no
/// names of people.
#[test]
fn comments_read_plain() {
    let findings: Vec<String> = workspace_sources().iter().flat_map(|(f, t)| comment_findings(f, t)).collect();
    assert!(findings.is_empty(), "Kommentar mit „—“ oder Namen:\n{}", findings.join("\n"));
}

#[test]
fn the_comment_scan_finds_dash_and_name() {
    let offences = [
        ("Strich", "let a = 1; // takes the first — or none"),
        ("Name", "/// Shown at the top (Robert, 2026-10-21)."),
        ("Name mit s", "// Robert's rule, not a setting."),
    ];
    for (name, text) in offences {
        assert_eq!(comment_findings("x.rs", text).len(), 1, "{name} nicht gefunden");
    }
    let fine = [
        "let path = \"Roberts Space Industries\"; // the game folder",
        "let s = \"a — b\"; // a text, not a comment",
        HEADER,
    ]
    .join("\n");
    assert!(comment_findings("x.rs", &fine).is_empty(), "Fehlalarm: {:?}", comment_findings("x.rs", &fine));
}

/// Texts whose wording is set by others and stays as it is, dash included:
/// the notice on fan content, as the fan kit asks for it.
const SET_WORDING: [&str; 1] = ["s_dk_fankit"];

/// The texts of the language file that `sources` use, with the dash.
fn texts_with_dash(json: &str, sources: &[(String, String)]) -> Vec<String> {
    let mut findings = Vec::new();
    for line in json.lines() {
        let Some((key, value)) = line.trim().split_once("\": \"") else { continue };
        let key = key.trim_start_matches('"');
        if SET_WORDING.contains(&key) {
            continue;
        }
        if value.contains('—') && sources.iter().any(|(_, t)| t.contains(&format!("\"{key}\""))) {
            findings.push(format!("{key}: {}", value.trim_end_matches(',').trim_end_matches('"')));
        }
    }
    findings
}

/// The texts v5 shows are short and plain: no long dash, in either language.
#[test]
fn shown_texts_have_no_long_dash() {
    let lang = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("versekit-core").join("lang");
    let sources = workspace_sources();
    let mut findings = Vec::new();
    for name in ["de.json", "en.json"] {
        let json = fs::read_to_string(lang.join(name)).expect("Sprachdatei");
        findings.extend(texts_with_dash(&json, &sources).into_iter().map(|f| format!("{name} {f}")));
    }
    assert!(findings.is_empty(), "Text mit „—“:\n{}", findings.join("\n"));
}

#[test]
fn the_text_scan_finds_a_dash() {
    let json = "{\n  \"s_x\": \"Gelöscht — sofort\",\n  \"s_frei\": \"Alt — unbenutzt\"\n}";
    let sources = vec![("a.rs".to_string(), "t(\"s_x\")".to_string())];
    assert_eq!(texts_with_dash(json, &sources), vec!["s_x: Gelöscht — sofort".to_string()]);
}

/// Every program file of this crate (test modules and test-only files cut off),
/// with its path below `src`.
fn ui_program_sources() -> Vec<(String, String)> {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, String)>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, base, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let name = path.strip_prefix(base).unwrap_or(&path).to_string_lossy().replace('\\', "/");
                let text = fs::read_to_string(&path).expect("Quelltext");
                let test_only = [
                    "sample/real_data.rs",
                    "sample/english.rs",
                    "sample/trade_pages.rs",
                    "sample/ship_plan.rs",
                    "sample/update_ui.rs",
                    "sample/control_column.rs",
                    "sample/streamer_mode.rs",
                    "sample/silent_buttons.rs",
                ];
                if test_only.contains(&name.as_str()) {
                    continue;
                }
                let program = match text.find("mod tests {") {
                    Some(at) => text[..at].rsplit_once("#[cfg(test)]").map_or(&text[..at], |(head, _)| head),
                    None => &text,
                };
                out.push((name, program.to_string()));
            }
        }
    }
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    walk(&src, &src, &mut out);
    out.sort();
    out
}

/// The string literals of a line of code, comments left out.
fn line_literals(line: &str) -> Vec<String> {
    let code = comment_start(line).map_or(line, |at| &line[..at]);
    let mut found = Vec::new();
    let mut current: Option<String> = None;
    let mut escaped = false;
    for c in code.chars() {
        match current.as_mut() {
            Some(text) if escaped => {
                text.push(c);
                escaped = false;
            }
            Some(_) if c == '\\' => escaped = true,
            Some(_) if c == '"' => found.push(current.take().unwrap_or_default()),
            Some(text) => text.push(c),
            None if c == '"' => current = Some(String::new()),
            None => {}
        }
    }
    found
}

/// Whether a literal reads like a text for people: a German letter, or words
/// starting with a capital (format strings, keys and paths do not).
fn reads_as_text(literal: &str) -> bool {
    let german = literal.chars().any(|c| "äöüÄÖÜß".contains(c));
    let words = literal.contains(' ') && literal.chars().next().is_some_and(char::is_uppercase);
    german || words
}

/// Calls that show one of their arguments to people, with the places that
/// argument can have (counted from 0). The text of an icon button or a switch
/// row stands in the call, so it has to come from the language file too.
const SHOWING_CALLS: [(&str, &[usize]); 25] = [
    (".label", &[0]),
    ("Button::new", &[0]),
    ("RichText::new", &[0]),
    (".on_hover_text", &[0]),
    ("collapsing", &[0]),
    ("heading", &[2]),
    ("switch_row", &[2]),
    ("setting_group", &[2]),
    ("loading", &[2]),
    ("external_link", &[2]),
    ("load_failed", &[2, 3]),
    ("sort_head", &[2]),
    ("clearable_field", &[2]),
    ("empty_state", &[3]),
    ("icon_button", &[3]),
    ("radio", &[3]),
    ("saved_hint", &[3]),
    ("checked_field", &[3]),
    ("status_box", &[4, 5]),
    ("notice", &[4]),
    ("text_area", &[1]),
    ("name_combo", &[1]),
    ("text_field", &[1]),
    ("then_some", &[0]),
    ("Some", &[0]),
];

/// Whether `head` ends with the call `name` as a whole word.
fn ends_with_call(head: &str, name: &str) -> bool {
    head.strip_suffix(name)
        .is_some_and(|rest| name.starts_with('.') || !rest.ends_with(|c: char| c.is_alphanumeric() || c == '_'))
}

/// A literal that looks like a word or a sentence to read (not a key, a path or an identifier).
fn looks_shown(literal: &str) -> bool {
    literal.chars().count() >= 3
        && literal.chars().next().is_some_and(char::is_uppercase)
        && !literal.contains(['_', '/'])
        && !literal.contains("::")
}

/// The literals of a line that stand where a call shows its text to people.
fn call_literals(line: &str) -> Vec<String> {
    let code = comment_start(line).map_or(line, |at| &line[..at]);
    let chars: Vec<(usize, char)> = code.char_indices().collect();
    // The open brackets of calls: where the bracket stands, how many commas came after it.
    let mut open: Vec<(usize, usize)> = Vec::new();
    let mut found = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let (at, c) = chars[i];
        match c {
            '(' => open.push((at, 0)),
            ')' => {
                open.pop();
            }
            ',' => {
                if let Some(top) = open.last_mut() {
                    top.1 += 1;
                }
            }
            '"' => {
                let mut literal = String::new();
                i += 1;
                while i < chars.len() && chars[i].1 != '"' {
                    if chars[i].1 == '\\' {
                        i += 1;
                    }
                    if let Some(&(_, c)) = chars.get(i) {
                        literal.push(c);
                    }
                    i += 1;
                }
                if let Some(&(bracket, commas)) = open.last() {
                    let head = &code[..bracket];
                    let shown = SHOWING_CALLS
                        .iter()
                        .any(|(name, places)| ends_with_call(head, name) && places.contains(&commas));
                    if shown && looks_shown(&literal) {
                        found.push(literal);
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    found
}

/// Texts for people written into the code instead of the language file, as `file\ttext`:
/// the literals that read like a text, and those that stand where a call shows a text.
fn fixed_texts(file: &str, text: &str) -> Vec<String> {
    let lines = text.lines().filter(|line| !line.trim_start().starts_with("//"));
    let mut found = std::collections::BTreeSet::new();
    for line in lines {
        found.extend(line_literals(line).into_iter().filter(|literal| reads_as_text(literal)));
        found.extend(call_literals(line));
    }
    found.into_iter().map(|literal| format!("{file}\t{literal}")).collect()
}

/// The file of the texts that read the same in both languages, and the reason for each group.
const SAME_IN_BOTH: &str = "tests/same_in_both_languages.txt";

/// The entries of [`SAME_IN_BOTH`]: every line that is not a comment or empty.
fn same_in_both_entries(text: &str) -> Vec<String> {
    text.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).map(str::to_string).collect()
}

/// What is wrong with the file of [`SAME_IN_BOTH`]: a group without a reason on its first line,
/// an entry twice, an entry with spaces around it.
fn same_in_both_problems(text: &str) -> Vec<String> {
    let mut problems = Vec::new();
    for block in text.split("\n\n") {
        let first = block.lines().next().unwrap_or_default();
        if !first.starts_with('#') && !block.trim().is_empty() {
            problems.push(format!("group without a reason: {first}"));
        }
    }
    let entries = same_in_both_entries(text);
    for (n, entry) in entries.iter().enumerate() {
        if entry.trim() != entry {
            problems.push(format!("spaces around the entry: {entry:?}"));
        }
        if entries[..n].contains(entry) {
            problems.push(format!("entry twice: {entry}"));
        }
    }
    problems
}

fn same_in_both() -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(SAME_IN_BOTH)).expect("Ausnahmeliste")
}

/// The fixed texts still waiting to move into the language file; the list may only shrink.
const FIXED_TEXTS: &str = "tests/fixed_texts.txt";

/// Whether a `file\ttext` entry of the scan is a text that reads the same in both languages.
fn reads_the_same(entry: &str, same: &[String]) -> bool {
    entry.split_once('\t').is_some_and(|(_, text)| same.iter().any(|s| s == text))
}

/// New texts for people come from the language file (`lang/de.json`, `en.json`),
/// never written into the code. The texts that are still in the code stand in
/// `tests/fixed_texts.txt` and leave it when they move; nothing joins it. A text
/// that reads the same in both languages (a name from the game, a path) stands in
/// `tests/same_in_both_languages.txt` with its reason instead.
#[test]
fn shown_texts_come_from_the_language_file() {
    let same = same_in_both_entries(&same_in_both());
    let mut now: Vec<String> = ui_program_sources()
        .iter()
        .flat_map(|(f, t)| fixed_texts(f, t))
        .filter(|entry| !reads_the_same(entry, &same))
        .collect();
    now.sort();
    now.dedup();
    let list = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXED_TEXTS);
    if std::env::var_os("VERSEKIT_FIXED_TEXTS_WRITE").is_some() {
        fs::write(&list, now.join("\n") + "\n").expect("Liste schreiben");
        return;
    }
    let known: Vec<String> = fs::read_to_string(&list).expect("Liste").lines().map(str::to_string).collect();
    let new: Vec<&String> = now.iter().filter(|t| !known.contains(t)).collect();
    let gone: Vec<&String> = known.iter().filter(|t| !now.contains(t)).collect();
    assert!(
        new.is_empty(),
        "Neuer fester Text im Code, gehört in lang/de.json und en.json:\n{}",
        new.iter().map(|t| t.as_str()).collect::<Vec<_>>().join("\n")
    );
    assert!(
        gone.is_empty(),
        "Text ist umgezogen, bitte aus {FIXED_TEXTS} streichen:\n{}",
        gone.iter().map(|t| t.as_str()).collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn the_fixed_text_scan_finds_a_written_text() {
    let offences = [
        ("Umlaut", "ui.label(\"Größe ändern\");"),
        ("Satz", "notice(\"Kommt mit dem Bau dieser Seite.\");"),
        ("Knopf", "Button::new(\"Jetzt aktualisieren\", kind, scheme)"),
    ];
    for (name, text) in offences {
        assert_eq!(fixed_texts("x.rs", text).len(), 1, "{name} nicht gefunden");
    }
    let fine = [
        "ui.label(t(\"hf_uebersicht\"));",
        "format!(\"{} %\", volume)",
        "let id = \"navigation\";",
        "// Ein Kommentar mit Größe",
        "let path = \"crates/versekit-core/lang\";",
    ]
    .join("\n");
    assert!(fixed_texts("x.rs", &fine).is_empty(), "Fehlalarm: {:?}", fixed_texts("x.rs", &fine));
}

/// A word without umlaut or space still counts where a call shows it.
#[test]
fn the_fixed_text_scan_finds_a_word_in_a_showing_call() {
    let offences = [
        ("Etikett", "ui.label(\"Schalter\");"),
        ("Knopf", "ui.add(widgets::Button::new(\"Abbrechen\", kind, scheme))"),
        ("Hinweis", "response.on_hover_text(\"Verbergen\");"),
        ("dritte Stelle", "widgets::switch_row(ui, scheme, \"Gespeichert\", &mut on);"),
        ("vierte Stelle", "widgets::icon_button(ui, scheme, \"x\", \"Loesen\");"),
        ("nach einem Aufruf", "foo(a, (b, c)); ui.label(\"Zeigen\");"),
    ];
    for (name, text) in offences {
        assert_eq!(fixed_texts("x.rs", text).len(), 1, "{name} nicht gefunden");
    }
    let fine = [
        "ui.label(t(\"s_x_y\"));",
        "widgets::icon_button(ui, scheme, \"Aaa\", &t(\"s_x\"));",
        "ui.label(\"x\");",
        "let name = lookup(\"Titanium\");",
        "format!(\"{}\", ui.label(\"ab\"))",
        "ui.label(\"s_mu_dialog\");",
        "ui.label(\"crates/foo\");",
    ]
    .join("\n");
    assert!(fixed_texts("x.rs", &fine).is_empty(), "Fehlalarm: {:?}", fixed_texts("x.rs", &fine));
}

/// Every exception has a reason, and no entry stands twice.
#[test]
fn the_texts_that_read_the_same_have_a_reason() {
    let text = same_in_both();
    assert_eq!(same_in_both_problems(&text), Vec::<String>::new());
    assert!(same_in_both_entries(&text).len() > 20, "the file is read wrongly");
}

#[test]
fn the_exception_check_finds_its_defects() {
    let fine = "# header\n\n# reason\nOne\nTwo\n";
    assert!(same_in_both_problems(fine).is_empty());
    let offences = [
        ("ohne Grund", "# header\n\nOne\nTwo\n"),
        ("doppelt", "# header\n\n# reason\nOne\nOne\n"),
        ("Leerzeichen", "# header\n\n# reason\nOne \n"),
    ];
    for (name, text) in offences {
        assert_eq!(same_in_both_problems(text).len(), 1, "{name} nicht gefunden");
    }
}

/// Whether a text of both language files carries `entry`: the own name of a language stands only there.
fn in_both_language_files(entry: &str) -> bool {
    let lang = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("versekit-core").join("lang");
    ["de.json", "en.json"].iter().all(|name| {
        let json = fs::read_to_string(lang.join(name)).expect("Sprachdatei");
        json.lines().any(|line| line.split_once("\": \"").is_some_and(|(_, value)| value.contains(entry)))
    })
}

/// An exception that neither the code nor the language files have any more must go, or the file fills with leftovers.
#[test]
fn every_exception_still_stands_in_the_code() {
    // The interface and the core (the phrases of the game log stand there).
    let core: Vec<(String, String)> = workspace_sources()
        .into_iter()
        .filter(|(path, _)| path.replace('\\', "/").contains("versekit-core/src"))
        .collect();
    assert!(core.len() > 5, "the sources of the core were not found");
    let all: Vec<String> =
        ui_program_sources().iter().chain(core.iter()).flat_map(|(_, t)| t.lines().flat_map(line_literals)).collect();
    let stale: Vec<String> = same_in_both_entries(&same_in_both())
        .into_iter()
        .filter(|e| !all.contains(e) && !in_both_language_files(e))
        .collect();
    assert!(stale.is_empty(), "Ausnahme steht nicht mehr im Code, bitte streichen:\n{}", stale.join("\n"));
}

/// Only the texts with a set wording are let through, and only those.
#[test]
fn the_dash_stays_only_in_texts_with_a_set_wording() {
    let json = "{\n  \"s_dk_fankit\": \"Fankit — specifically\",\n  \"s_dk_anders\": \"Anders — nein\"\n}";
    let sources = vec![("a.rs".to_string(), "t(\"s_dk_fankit\") t(\"s_dk_anders\")".to_string())];
    assert_eq!(texts_with_dash(json, &sources), vec!["s_dk_anders: Anders — nein".to_string()]);
}

/// Words of the call that make a field a search line: the state it edits or the text it asks with.
const SEARCH_WORDS: [&str; 4] = ["search", "such", "filter", "signature"];

/// Every search line of `text` drawn as a plain field: a `clearable_field` over a search text, or a
/// `SuggestField` named after a search without `.searching()`.
fn plain_searches(file: &str, text: &str) -> Vec<String> {
    let mut findings = Vec::new();
    let mut from = 0;
    while let Some(found) = text[from..].find("clearable_field(") {
        let at = from + found;
        from = at + 1;
        if text[..at].ends_with("fn ") {
            continue;
        }
        // The call and the three lines before it: the text it asks with often stands in a variable.
        let first_line = text[..at].rfind('\n').map_or(0, |n| n + 1);
        let lines_before = text[..first_line].lines().rev().take(3).collect::<Vec<_>>().join("\n");
        let call = arguments(text, at + "clearable_field".len()).join(",");
        let context = format!("{lines_before}\n{call}").to_lowercase();
        if SEARCH_WORDS.iter().any(|word| context.contains(word)) {
            findings.push(format!("{file}:{}: clearable_field({call})", line_of(text, at)));
        }
    }
    from = 0;
    while let Some(found) = text[from..].find("SuggestField::new(") {
        let at = from + found;
        from = at + 1;
        let open = at + "SuggestField::new".len();
        let name = arguments(text, open).first().and_then(|a| literal(a)).unwrap_or_default().to_lowercase();
        let built = &text[at..text[at..].find(".show(").map_or(text.len(), |n| at + n)];
        if SEARCH_WORDS.iter().any(|word| name.contains(word)) && !built.contains(".searching()") {
            findings.push(format!("{file}:{}: SuggestField {name} without searching()", line_of(text, at)));
        }
    }
    findings
}

/// Every search line is drawn with `widgets::search_field` (or a `SuggestField` that is `.searching()`), so
/// the green border, the magnifier and the lighter ground stand in one place.
#[test]
fn every_search_line_is_drawn_with_the_search_field() {
    let sources = ui_program_sources();
    let findings: Vec<String> = sources.iter().flat_map(|(f, t)| plain_searches(f, t)).collect();
    assert!(findings.is_empty(), "Suchzeile als einfaches Feld:\n{}", findings.join("\n"));
    let uses: usize = sources.iter().map(|(_, t)| t.matches("search_field(").count()).sum();
    assert!(uses >= 10, "only {uses} search lines found");
}

#[test]
fn the_search_line_scan_finds_a_plain_search() {
    let plain = "widgets::clearable_field(ui, &mut state.search, &t(\"s_x\"), scheme, width);";
    let by_name = "widgets::clearable_field(ui, &mut state.text, &t(\"s_pl_suche\"), scheme, width);";
    let suggest = "widgets::SuggestField::new(\"bergung-suche\", &hint, scheme, &more).width(w).show(ui, t, e);";
    for (name, text) in [("Zustand", plain), ("Text", by_name), ("Vorschlag", suggest)] {
        assert_eq!(plain_searches("x.rs", text).len(), 1, "{name} nicht gefunden");
    }
    let in_a_variable = "let hint = t(\"s_ek_vorlage_suche\");\nwidgets::clearable_field(inner, typed, &hint, scheme, width);";
    assert_eq!(plain_searches("x.rs", in_a_variable).len(), 1, "Text in einer Variablen nicht gefunden");
    for fine in [
        "widgets::search_field(ui, &mut state.search, &t(\"s_x\"), scheme, width);",
        "widgets::clearable_field(ui, &mut state.set_name, &t(\"s_pl_satzname\"), scheme, width);",
        "pub fn clearable_field(ui: &mut Ui, text: &mut String, hint: &str, scheme: &Scheme, width: f32) {",
        "widgets::SuggestField::new(\"bergung-suche\", &hint, scheme, &more).searching().show(ui, t, e);",
        "widgets::SuggestField::new(\"hangar-eintragen\", &hint, scheme, &more).show(ui, t, e);",
    ] {
        assert!(plain_searches("x.rs", fine).is_empty(), "Fehlalarm: {fine}: {:?}", plain_searches("x.rs", fine));
    }
}

/// Files that may name a size of text as a number: the sizes themselves (`lib.rs`) and the picture of the game's
/// own screen in the sample, which copies what the game shows.
const OWN_TEXT_SIZES: [&str; 2] = ["lib.rs", "sample/preview.rs"];

/// Every size of text written as a number: `FontId::proportional(13.0)`, `fonts::bold(10.0)`,
/// `RichText::new(…).size(22.0)` and the like, each with its line.
fn literal_text_sizes(file: &str, text: &str) -> Vec<String> {
    let calls = ["FontId::proportional(", "FontId::monospace(", "FontId::new(", "fonts::bold(", ".size("];
    let mut found = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let code = comment_start(line).map_or(line, |at| &line[..at]);
        for call in calls {
            let mut rest = code;
            while let Some(at) = rest.find(call) {
                let after = &rest[at + call.len()..];
                let number = after.trim_start().starts_with(|c: char| c.is_ascii_digit());
                // `.size(` is a size of text only on rich text, not on a spinner or a picture.
                let text_size = call != ".size(" || code.contains("RichText");
                if number && text_size {
                    found.push(format!("{file}:{}: {}", n + 1, line.trim()));
                }
                rest = after;
            }
        }
    }
    found
}

/// Every text takes its size from the few sizes of the interface (`versekit_ui::text_styles`), never a number of
/// its own: a page of its own sizes is what makes the sizes drift apart.
#[test]
fn text_sizes_come_from_the_styles_of_the_interface() {
    let findings: Vec<String> = ui_program_sources()
        .iter()
        .filter(|(f, _)| !OWN_TEXT_SIZES.contains(&f.as_str()))
        .flat_map(|(f, t)| literal_text_sizes(f, t))
        .collect();
    assert!(findings.is_empty(), "Schriftgröße als Zahl:\n{}", findings.join("\n"));
}

#[test]
fn the_text_size_scan_finds_a_number() {
    for size in [
        "let font = egui::FontId::proportional(11.0);",
        "cell.label(RichText::new(count.to_string()).size(22.0).color(c));",
        "let galley = layout(text, crate::fonts::bold(10.0), c);",
    ] {
        assert_eq!(literal_text_sizes("x.rs", size).len(), 1, "nicht gefunden: {size}");
    }
    for fine in [
        "let font = egui::TextStyle::Small.resolve(ui.style());",
        "let spinner = egui::Spinner::new().size(14.0).color(c);",
        "let big = fonts::bold(egui::TextStyle::Heading.resolve(ui.style()).size);",
        "// FontId::proportional(11.0) in a comment",
    ] {
        assert!(literal_text_sizes("x.rs", fine).is_empty(), "Fehlalarm: {fine}");
    }
}
