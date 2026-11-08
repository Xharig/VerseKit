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

//! Checks of the two language files, `lang/de.json` and `lang/en.json`.
//!
//! The files are the original of every text v5 shows; they are edited by hand,
//! German and English together. These tests keep them honest:
//!
//! * both files are valid JSON with the expected sections and a plain header,
//! * no key appears twice (a JSON reader silently keeps the last one),
//! * both languages have the same keys,
//! * no text is empty,
//! * every key has the same placeholders in both languages,
//! * every key the Rust code asks for with a literal exists.
//!
//! ⚠ Keys built at run time (`format!("art_{raw}")`, a key picked from a table
//! in another module) cannot be followed by a scan of the source. Literals
//! with the shape of a `s_…` key are checked wherever they stand, which covers
//! most tables; other families (`art_`, `gruppe_`, `ov_` …) are checked only
//! where a call such as `t("…")` names the key right away.
//!
//! Every check is a function over plain text, and `the_checks_find_their_defect`
//! feeds each one a broken input, so a check that stops looking turns red.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;
use versekit_core::jsonio::{self, Value};
use versekit_core::language::{placeholders, spec_supported};

/// Sections of a language file whose values are plain texts.
const SECTIONS: [&str; 4] = ["texte", "feldnamen", "eigenschaften", "eigenschaften_laser"];

/// One language file read for the checks.
struct LangFile {
    /// Name shown in messages (`de.json`).
    name: String,
    header: Option<String>,
    sections: BTreeMap<String, BTreeMap<String, String>>,
}

fn lang_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("lang")
}

fn read_lang(name: &str) -> String {
    let path = lang_dir().join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Parses a language file; the error says what is wrong with it.
fn parse(name: &str, text: &str) -> Result<LangFile, String> {
    let root = jsonio::loads(text).map_err(|e| format!("{name}: not valid JSON: {e:?}"))?;
    let Value::Object(entries) = &root else { return Err(format!("{name}: the top level is not an object")) };
    let mut file = LangFile { name: name.to_string(), header: None, sections: BTreeMap::new() };
    for (key, value) in entries {
        match (key.as_str(), value) {
            ("quelle", Value::Str(text)) => file.header = Some(text.clone()),
            ("quelle", _) => return Err(format!("{name}: `quelle` is not a text")),
            (section, Value::Object(map)) if SECTIONS.contains(&section) => {
                let mut texts = BTreeMap::new();
                for (k, v) in map {
                    let Value::Str(s) = v else { return Err(format!("{name}: {section}.{k} is not a text")) };
                    texts.insert(k.clone(), s.clone());
                }
                file.sections.insert(section.to_string(), texts);
            }
            (other, _) => return Err(format!("{name}: unexpected entry `{other}`")),
        }
    }
    for section in SECTIONS {
        if !file.sections.contains_key(section) {
            return Err(format!("{name}: section `{section}` is missing"));
        }
    }
    Ok(file)
}

fn load(name: &str) -> LangFile {
    let text = read_lang(name);
    parse(name, &text).unwrap_or_else(|e| panic!("{e}"))
}

fn both() -> (LangFile, LangFile) {
    (load("de.json"), load("en.json"))
}

/// The header must not send anyone to the Python version for the texts.
fn header_problems(file: &LangFile) -> Vec<String> {
    match &file.header {
        None => vec![format!("{}: `quelle` is missing", file.name)],
        Some(h) if h.trim().is_empty() => vec![format!("{}: `quelle` is empty", file.name)],
        Some(h) => ["language.py", "export_language"]
            .iter()
            .filter(|old| h.contains(**old))
            .map(|old| format!("{}: the header still points to `{old}`", file.name))
            .collect(),
    }
}

/// Keys that stand twice in one section, and a section that stands twice. The
/// files keep one entry per line (`  "key": "text",`); a line that does not
/// fit that layout is reported too, because this check reads lines.
fn duplicate_problems(name: &str, text: &str) -> Vec<String> {
    let entry = Regex::new(r#"^  "([^"\\]+)": "(?:[^"\\]|\\.)*",?$"#).expect("regex");
    let head = Regex::new(r#"^ "([a-z_]+)": \{$"#).expect("regex");
    let mut problems = Vec::new();
    let mut section = String::new();
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    let mut heads: BTreeSet<String> = BTreeSet::new();
    for (number, line) in text.lines().enumerate() {
        if let Some(caps) = head.captures(line) {
            section = caps[1].to_string();
            if !heads.insert(section.clone()) {
                problems.push(format!("{name}:{}: section `{section}` stands twice", number + 1));
            }
        } else if let Some(caps) = entry.captures(line) {
            if !seen.insert((section.clone(), caps[1].to_string())) {
                problems.push(format!("{name}:{}: `{}` stands twice in `{section}`", number + 1, &caps[1]));
            }
        } else if line.starts_with("  ") {
            problems.push(format!("{name}:{}: line does not fit the layout one entry per line", number + 1));
        }
    }
    problems
}

fn list(keys: impl Iterator<Item = String>) -> String {
    let all: Vec<String> = keys.collect();
    let mut names: Vec<String> = all.iter().take(8).cloned().collect();
    if all.len() > 8 {
        names.push(format!("… and {} more", all.len() - 8));
    }
    names.join(", ")
}

/// Both languages need the same keys. `eigenschaften` is the exception: English
/// keeps the game's name for most recipe properties, so it may hold fewer
/// entries, but none the German table lacks.
fn key_problems(de: &LangFile, en: &LangFile) -> Vec<String> {
    let mut problems = Vec::new();
    for section in SECTIONS {
        let a: BTreeSet<&String> = de.sections[section].keys().collect();
        let b: BTreeSet<&String> = en.sections[section].keys().collect();
        let only_de = list(a.difference(&b).map(|k| (*k).clone()));
        if section != "eigenschaften" && !only_de.is_empty() {
            problems.push(format!("{section}: missing in {}: {only_de}", en.name));
        }
        let only_en = list(b.difference(&a).map(|k| (*k).clone()));
        if !only_en.is_empty() {
            problems.push(format!("{section}: missing in {}: {only_en}", de.name));
        }
    }
    problems
}

fn empty_problems(file: &LangFile) -> Vec<String> {
    let mut problems = Vec::new();
    for (section, map) in &file.sections {
        for (key, text) in map {
            if text.trim().is_empty() {
                problems.push(format!("{}: {section}.{key} is empty", file.name));
            }
        }
    }
    problems
}

/// Returns the problems and how many placeholders were looked at.
fn placeholder_problems(de: &LangFile, en: &LangFile) -> (Vec<String>, usize) {
    let mut problems = Vec::new();
    let mut count = 0;
    let none = String::new();
    for (key, german) in &de.sections["texte"] {
        let english = en.sections["texte"].get(key).unwrap_or(&none);
        let mut a = placeholders(german);
        let mut b = placeholders(english);
        for (name, spec) in a.iter().chain(&b) {
            if !spec_supported(spec) {
                problems.push(format!("{key}: placeholder {{{name}:{spec}}} has a spec the reader does not know"));
            }
        }
        count += a.len();
        a.sort();
        b.sort();
        if a != b {
            problems.push(format!("{key}: placeholders differ, de {a:?} / en {b:?}"));
        }
    }
    (problems, count)
}

/// A `{` or `}` that is neither a complete `{name}` nor a doubled `{{`/`}}`:
/// the reader prints it as it stands, so a typo like `{0` would reach the user.
fn brace_problems(file: &LangFile) -> Vec<String> {
    let doubled = Regex::new(r"\{\{|\}\}").expect("regex");
    let complete = Regex::new(r"\{[^{}]*\}").expect("regex");
    let mut problems = Vec::new();
    for (key, text) in &file.sections["texte"] {
        let rest = doubled.replace_all(text, "");
        let rest = complete.replace_all(&rest, "");
        if rest.contains(['{', '}']) {
            problems.push(format!("{}: texte.{key} has a brace that is not a placeholder", file.name));
        }
    }
    problems
}

/// What a hand-written text can get wrong that the language comparison does
/// not see, because it is wrong the same way in both languages: a placeholder
/// name that is neither a number nor an identifier (`{ 0}`, `{}`, `{0 }`), and
/// leftovers of the 4.12 syntax (`%s`, `%d`, `%%`, `%.1f`) in a text that is
/// not a date format.
fn syntax_problems(file: &LangFile) -> Vec<String> {
    let old = Regex::new(r"%%|%[+]?(?:\.\d+f|[sdg])").expect("regex");
    let strftime = Regex::new(r"%[YmHMSaAbBpyj]").expect("regex");
    let name = Regex::new(r"^(?:\d+|[A-Za-z_][A-Za-z0-9_]*)$").expect("regex");
    let mut problems = Vec::new();
    for (key, text) in &file.sections["texte"] {
        for (field, _) in placeholders(text) {
            if !name.is_match(&field) {
                problems.push(format!("{}: texte.{key} has a placeholder with the name `{field}`", file.name));
            }
        }
        if old.is_match(text) && !strftime.is_match(text) {
            problems.push(format!("{}: texte.{key} still has the old `%` placeholder syntax", file.name));
        }
    }
    problems
}

/// The keys of the file `tests/same_text_keys.txt`: every line that is not a comment or empty.
fn same_text_keys(file_text: &str) -> BTreeSet<String> {
    file_text.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).map(|l| l.trim().to_string()).collect()
}

/// Whether a text has a word in it (three letters in a row once the placeholders are gone).
/// Units, numbers and ranges (`{0} s`, `5–100`) read the same in every language.
fn has_a_word(text: &str) -> bool {
    let mut run = 0;
    let mut in_placeholder = false;
    for c in text.chars() {
        match c {
            '{' => in_placeholder = true,
            '}' => in_placeholder = false,
            _ if in_placeholder => {}
            c if c.is_alphabetic() => {
                run += 1;
                if run >= 3 {
                    return true;
                }
            }
            _ => run = 0,
        }
    }
    false
}

/// Keys whose English text is the German one, copied: the translation was forgotten, or the
/// word reads the same in both languages and the key is listed in `known`. A listed key that
/// no longer reads the same, or no longer exists, is a problem too.
fn same_text_problems(de: &LangFile, en: &LangFile, known: &BTreeSet<String>) -> Vec<String> {
    let mut problems = Vec::new();
    let mut same = BTreeSet::new();
    for (key, german) in &de.sections["texte"] {
        if en.sections["texte"].get(key) == Some(german) && has_a_word(german) {
            same.insert(key.clone());
            if !known.contains(key) {
                problems.push(format!("{key}: the English text is the German one ({german}); translate it"));
            }
        }
    }
    for key in known.difference(&same) {
        problems.push(format!("{key}: listed in same_text_keys.txt but it does not read the same any more; remove it"));
    }
    problems
}

// ---------------------------------------------------------------------------
// Keys the code asks for
// ---------------------------------------------------------------------------

/// Source files of every crate (`crates/*/src/**/*.rs`) with their text. Of
/// `language.rs` only the part before its tests is read: they ask for keys
/// that must not exist.
fn sources() -> Vec<(String, String)> {
    fn walk(dir: &Path, out: &mut Vec<(String, String)>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let name = path.to_string_lossy().replace('\\', "/");
                if let Ok(text) = fs::read_to_string(&path) {
                    let text = if name.ends_with("versekit-core/src/language.rs") {
                        text.split("#[cfg(test)]").next().unwrap_or_default().to_string()
                    } else {
                        text
                    };
                    out.push((name, text));
                }
            }
        }
    }
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut out = Vec::new();
    let mut dirs: Vec<PathBuf> = fs::read_dir(&crates).expect("crates folder").flatten().map(|e| e.path()).collect();
    dirs.sort();
    for dir in dirs {
        walk(&dir.join("src"), &mut out);
    }
    out
}

/// Prefixes of the key families that are read as keys wherever a literal of
/// that shape stands (tables, constants, `if … { "k1" } else { "k2" }`), next
/// to `s_`. Families whose prefix is also a common word or a settings name
/// (`art_`, `gruppe_`, `ohne_`, `inj_`) are left out: they are found only where
/// a call names them.
const FAMILIES: &str = "hf|ov|bf|b|m|he|hk|ff|up|bv|kb|tray|gal|scan|hinweis|schritt|sich|ka|aafk";

/// Keys named by a literal, with the file: `t("k")`, `tf("k", …)`,
/// `Phrase::new("k")`, `t_in(lang, "k")`, `tf_in(lang, "k", …)`,
/// `text_in(lang, "k")`, the local wrappers `text(lang, "k")`, `t(lang, "k")`
/// and `n(lang, "k", …)`, any literal shaped like a key of a family in
/// [`FAMILIES`], and the fixed start of `format!("s_…_{…}")` (returned with a
/// trailing `_`).
fn used_keys(file: &str, text: &str) -> Vec<(String, String)> {
    let direct = Regex::new(r#"\b(?:t|tf|Phrase::new)\(\s*"([^"\\]+)""#).expect("regex");
    let with_lang = Regex::new(r#"\b(?:t_in|tf_in|text_in)\(\s*[^,()]+,\s*"([^"\\]+)""#).expect("regex");
    let wrapper = Regex::new(r#"\b(?:text|t|n)\(\s*\*?lang,\s*"([^"\\]+)""#).expect("regex");
    // `s_` keys always have two more parts; `"s_fa"` is a prefix, not a key.
    let shaped =
        Regex::new(&format!(r#""((?:s(?:_[a-z0-9]+){{2,}})|(?:(?:{FAMILIES})(?:_[a-z0-9]+)+))""#)).expect("regex");
    let built = Regex::new(r#"format!\(\s*"(s_[a-z0-9_]*_)\{"#).expect("regex");
    // Whole-line comments hold examples and old names, not calls.
    let code: String = text.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
    let mut found = Vec::new();
    for rx in [&direct, &with_lang, &wrapper, &shaped, &built] {
        for caps in rx.captures_iter(&code) {
            found.push((file.to_string(), caps[1].to_string()));
        }
    }
    found
}

fn used_key_problems(keys: &BTreeSet<&String>, sources: &[(String, String)]) -> (Vec<String>, usize) {
    let mut problems = BTreeSet::new();
    let mut count = 0;
    for (file, text) in sources {
        for (file, key) in used_keys(file, text) {
            count += 1;
            let known =
                if key.ends_with('_') { keys.iter().any(|k| k.starts_with(key.as_str())) } else { keys.contains(&key) };
            if !known {
                problems.insert(format!("{file}: key `{key}` is not in the language files"));
            }
        }
    }
    (problems.into_iter().collect(), count)
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

fn assert_clean(problems: &[String]) {
    assert!(problems.is_empty(), "{} problem(s):\n{}", problems.len(), problems.join("\n"));
}

#[test]
fn both_files_are_valid_json_with_all_sections() {
    let (de, en) = both();
    // Counted so an empty table cannot pass for a complete one.
    assert!(de.sections["texte"].len() > 3000, "{}", de.sections["texte"].len());
    assert!(!en.sections["feldnamen"].is_empty());
}

#[test]
fn the_header_does_not_point_to_python() {
    let (de, en) = both();
    assert_clean(&[header_problems(&de), header_problems(&en)].concat());
}

#[test]
fn no_key_stands_twice() {
    let problems: Vec<String> =
        ["de.json", "en.json"].iter().flat_map(|name| duplicate_problems(name, &read_lang(name))).collect();
    assert_clean(&problems);
}

#[test]
fn both_languages_have_the_same_keys() {
    let (de, en) = both();
    assert_clean(&key_problems(&de, &en));
}

#[test]
fn no_text_is_empty() {
    let (de, en) = both();
    assert_clean(&[empty_problems(&de), empty_problems(&en)].concat());
}

#[test]
fn no_text_has_a_stray_brace() {
    let (de, en) = both();
    assert_clean(&[brace_problems(&de), brace_problems(&en)].concat());
}

#[test]
fn no_text_has_a_bad_placeholder_name_or_old_syntax() {
    let (de, en) = both();
    assert_clean(&[syntax_problems(&de), syntax_problems(&en)].concat());
}

#[test]
fn placeholders_match_between_languages() {
    let (de, en) = both();
    let (problems, count) = placeholder_problems(&de, &en);
    assert_clean(&problems);
    // The scan really finds placeholders.
    assert!(count > 800, "only {count} placeholders found");
}

/// An English text that is the German one is an untranslated text, unless the word reads the
/// same in both languages and the key says so in `tests/same_text_keys.txt`.
#[test]
fn texts_that_read_the_same_are_known() {
    let (de, en) = both();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("same_text_keys.txt");
    let known = same_text_keys(&fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())));
    // The scan really finds keys.
    assert!(known.len() > 50, "only {} keys listed", known.len());
    assert_clean(&same_text_problems(&de, &en, &known));
}

#[test]
fn every_key_the_code_asks_for_exists() {
    let (de, _) = both();
    let keys: BTreeSet<&String> = de.sections["texte"].keys().collect();
    let sources = sources();
    assert!(sources.len() > 50, "only {} source files found", sources.len());
    let (problems, count) = used_key_problems(&keys, &sources);
    assert_clean(&problems);
    assert!(count > 500, "only {count} keys found in the code");
}

/// Each check gets an input with exactly the defect it is there to find.
#[test]
fn the_checks_find_their_defect() {
    let good = r#"{
 "quelle": "hand edited",
 "texte": {
  "a": "Eins {0}",
  "b": "Zwei"
 },
 "feldnamen": {
  "x.y": "Feld"
 },
 "eigenschaften": {
  "p": "Eigenschaft"
 },
 "eigenschaften_laser": {
  "p": "Laser"
 }
}
"#;
    let ok = parse("good.json", good).expect("good file parses");
    assert!(header_problems(&ok).is_empty());
    assert!(duplicate_problems("good.json", good).is_empty());
    assert!(empty_problems(&ok).is_empty());

    // Not valid JSON, wrong shape, missing section.
    assert!(parse("x", &good.replace("\"b\": \"Zwei\"", "\"b\": \"Zwei\",")).is_err());
    assert!(parse("x", &good.replace("\"Zwei\"", "2")).is_err());
    assert!(parse("x", &good.replace("\"feldnamen\"", "\"anderes\"")).is_err());
    assert!(parse("x", "[]").is_err());

    // Header.
    let old = good.replace("hand edited", "language.py (tools/export_language.py)");
    assert_eq!(header_problems(&parse("old", &old).expect("parses")).len(), 2);
    let no_header = good.replace(" \"quelle\": \"hand edited\",\n", "");
    assert_eq!(header_problems(&parse("none", &no_header).expect("parses")).len(), 1);

    // A key twice, in one section only (the same key in another section is fine).
    let twice = good.replace("  \"b\": \"Zwei\"", "  \"a\": \"Doppelt\",\n  \"b\": \"Zwei\"");
    assert_eq!(duplicate_problems("twice.json", &twice).len(), 1);
    let loose = good.replace("  \"b\": \"Zwei\"", "  \"b\":\"Zwei\"");
    assert_eq!(duplicate_problems("loose.json", &loose).len(), 1);
    // Two entries in one line hide the second key from a check that reads lines.
    let crowded = good.replace("  \"a\": \"Eins {0}\",", "  \"a\": \"Eins {0}\", \"a\": \"Doppelt\",");
    assert_eq!(duplicate_problems("crowded.json", &crowded).len(), 1);
    // A section twice: the reader would merge them without a word.
    let again = good.replace(" \"feldnamen\": {", " \"texte\": {\n  \"c\": \"Drei\"\n },\n \"feldnamen\": {");
    assert_eq!(duplicate_problems("again.json", &again).len(), 1);
    // Escaped quotes and a backslash inside a text are fine.
    let escaped = good.replace("\"Zwei\"", "\"Er sagte \\\"ja\\\" \\\\ nein\"");
    assert!(duplicate_problems("escaped.json", &escaped).is_empty());

    // A key in one language only, in either direction.
    let fewer = parse("fewer", &good.replace("  \"a\": \"Eins {0}\",\n", "")).expect("parses");
    assert_eq!(key_problems(&ok, &fewer).len(), 1);
    assert_eq!(key_problems(&fewer, &ok).len(), 1);
    // …except fewer recipe properties in English.
    let no_property = parse("p", &good.replace("  \"p\": \"Eigenschaft\"\n", "")).expect("parses");
    assert!(key_problems(&ok, &no_property).is_empty());
    assert_eq!(key_problems(&no_property, &ok).len(), 1);

    // An empty text.
    let blank = parse("blank", &good.replace("\"Zwei\"", "\"  \"")).expect("parses");
    assert_eq!(empty_problems(&blank).len(), 1);

    // Braces: unclosed, lone closing, but doubled ones are literal and fine.
    for (text, bad) in [("Eins {0", true), ("Eins 0}", true), ("Eins {{0}}", false), ("Eins {0} {{x}}", false)] {
        let braced = parse("braced", &good.replace("Eins {0}", text)).expect("parses");
        assert_eq!(brace_problems(&braced).len(), usize::from(bad), "{text}");
    }

    // Syntax: a bad name, 4.12 leftovers; a date format keeps its `%`.
    for (text, bad) in [
        ("Eins { 0}", true),
        ("Eins {}", true),
        ("Eins {0 }", true),
        ("Eins %s", true),
        ("Eins %d %%", true),
        ("Eins %.1f", true),
        ("Eins {0} 50 %", false),
        ("%d.%m.%Y", false),
    ] {
        let syntax = parse("syntax", &good.replace("Eins {0}", text)).expect("parses");
        assert_eq!(syntax_problems(&syntax).len(), usize::from(bad), "{text}");
    }

    // Placeholders: a different one, a missing one, an unknown spec.
    for other in ["Eins {1}", "Eins", "Eins {0:x}", "Eins {0} {1}"] {
        let changed = parse("changed", &good.replace("Eins {0}", other)).expect("parses");
        assert!(!placeholder_problems(&ok, &changed).0.is_empty(), "{other}");
    }
    let (clean, count) = placeholder_problems(&ok, &ok);
    assert!(clean.is_empty() && count == 1);

    // An English text copied from the German one: found, unless listed; a listed key must still read the same.
    let copied = parse("copied", &good.replace("\"Zwei\"", "\"Vier\"")).expect("parses");
    let listed = |keys: &[&str]| keys.iter().map(|k| k.to_string()).collect::<BTreeSet<String>>();
    assert_eq!(same_text_problems(&ok, &copied, &listed(&[])).len(), 1, "a copied text");
    assert!(same_text_problems(&ok, &copied, &listed(&["a"])).is_empty(), "a listed word");
    assert_eq!(same_text_problems(&ok, &ok, &listed(&[])).len(), 2, "two copied texts");
    assert_eq!(same_text_problems(&ok, &copied, &listed(&["a", "b"])).len(), 1, "a listed key that differs");
    assert_eq!(same_text_problems(&ok, &copied, &listed(&["a", "gone"])).len(), 1, "a listed key that is gone");
    // Numbers, units and ranges are no words.
    for text in ["{0} s", "5–100", "—", "Q {0:g}", "cm"] {
        assert!(!has_a_word(text), "{text}");
    }
    for text in ["Status", "Terminal", "{0} SCU", "Verse-Kit"] {
        assert!(has_a_word(text), "{text}");
    }

    // Keys the code asks for.
    let known: BTreeSet<&String> = ok.sections["texte"].keys().collect();
    let code = |text: &str| vec![("x.rs".to_string(), text.to_string())];
    for text in [
        r#"let w = t("zz");"#,
        r#"let w = tf("zz", &[]);"#,
        r#"let p = Phrase::new("zz");"#,
        r#"let w = t_in(*lang, "zz");"#,
        r#"let w = tf_in(Lang::De, "zz", &args);"#,
        r#"let w = language::text_in(lang, "zz");"#,
        r#"let w = text(lang, "zz");"#,
        r#"let w = n(*lang, "zz", 3);"#,
        r#"let k = if on { "ov_zz" } else { "plain" };"#,
        r#"const KEY: &str = "m_zz_zz";"#,
        r#"const KEYS: [&str; 1] = ["s_zz_zz"];"#,
        r#"let w = t(&format!("s_zz_{n}"));"#,
    ] {
        assert_eq!(used_key_problems(&known, &code(text)).0.len(), 1, "{text}");
    }
    for text in [r#"let w = t("a");"#, r#"// t("zz")"#, r#"let w = t(key);"#, r#"let w = "plain_text";"#] {
        assert!(used_key_problems(&known, &code(text)).0.is_empty(), "{text}");
    }
}
