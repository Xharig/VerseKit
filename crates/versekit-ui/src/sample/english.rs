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

//! The interface in English, without a window: every page and every tile is
//! drawn with an egui context, every drawn text and every name a screen
//! reader gets is collected, and each one that is German is reported.
//!
//! The scan of the source (`tests/standards.rs`) reads literals and misses
//! what is built at run time: a label in a table or a tuple, a constant, a
//! `format!`. This check does not read the code. It looks at the finished
//! page, in English, and asks of every text:
//!
//! * is it,  for word, a text of `lang/de.json` that `en.json` does not
//!   write the same way (a German text that came from the file but through the
//!   wrong language, or was copied into the code),
//! * is it a German text with something filled in (`{name}` in the template),
//! * does it carry an umlaut, an `ß` or a typical German word.
//!
//! What reads the same in both languages (names from the game, paths) is
//! listed with its reason in `tests/same_in_both_languages.txt`.
//!
//! The language is set for the thread of the test only, so no lock is needed.

use std::collections::HashSet;

use versekit_core::language::{self, Choice, Lang};
use versekit_core::theme;

use super::{NAV, SamplePage};

/// Texts that read the same in both languages, one per line, with `#` reasons.
const SAME_IN_BOTH: &str = include_str!("../../tests/same_in_both_languages.txt");

/// Words that are German and nothing else. A text with one of them is not English.
const GERMAN_WORDS: &[&str] = &[
    "aber",
    "alle",
    "alles",
    "auch",
    "auf",
    "aus",
    "bei",
    "bitte",
    "dann",
    "das",
    "dein",
    "deine",
    "deinen",
    "deiner",
    "dem",
    "den",
    "der",
    "des",
    "die",
    "dir",
    "dich",
    "eine",
    "einen",
    "einem",
    "einer",
    "für",
    "gibt",
    "hier",
    "ihr",
    "ist",
    "jetzt",
    "kein",
    "keine",
    "keinen",
    "mit",
    "nach",
    "nicht",
    "noch",
    "nur",
    "oder",
    "sich",
    "sind",
    "und",
    "unter",
    "vom",
    "von",
    "wenn",
    "wird",
    "werden",
    "zum",
    "zur",
    "über",
    "läuft",
    "fehlt",
    "fehlen",
    "gefunden",
    "offen",
    "geladen",
    "seite",
    "spiel",
    "bauplan",
    "baupläne",
    "aufträge",
    "auftrag",
    "einstellung",
    "einstellungen",
    "einrichtung",
    "fenster",
    "knopf",
    "sicherung",
    "neustart",
    "größe",
    "zeilen",
    "töne",
    "datei",
    "ordner",
    "fehler",
    "farbschemata",
    "farbschema",
    "wärmekarte",
    "wochentag",
    "heute",
    "gestern",
    "diese",
    "woche",
    "stunde",
    "stunden",
    "minuten",
    "tage",
    "monat",
    "jahr",
    "weniger",
    "mehr",
];

/// The two tables of the language files, for the look-ups of the check.
struct Dictionary {
    german: HashSet<String>,
    english: HashSet<String>,
    /// Texts with placeholders: (German, English) of one key.
    templates: Vec<(String, String)>,
    /// The texts that read the same in both languages: whole texts and templates.
    same: Vec<String>,
}

impl Dictionary {
    fn new() -> Dictionary {
        let [de, en] = language::tables();
        let values = |table: &language::Table| table.texts.values().cloned().collect::<HashSet<String>>();
        // A template that ends or starts with a placeholder fits too much to say anything: it needs
        // enough fixed text, and a text of the other language must not fit its English twin.
        let templates = de
            .texts
            .iter()
            .filter(|(_, text)| fixed_length(text) >= 8 && text.contains('{') && !text.contains("{{"))
            .filter_map(|(key, text)| Some((text.clone(), en.texts.get(key)?.clone())))
            .collect();
        let same = SAME_IN_BOTH.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).map(str::to_string);
        Dictionary { german: values(de), english: values(en), templates, same: same.collect() }
    }

    /// Whether a text with something filled in is German: it fits a German template
    /// whose English twin it does not fit, and no English text fits it either.
    fn fits_only_german(&self, text: &str) -> bool {
        let german = self.templates.iter().any(|(de, en)| fits(de, text) && !fits(en, text));
        // Only an English template with fixed text at both ends can claim a text.
        let anchored = |template: &str| {
            let pieces = parts(template);
            pieces.first().is_some_and(|p| !p.is_empty()) && pieces.last().is_some_and(|p| !p.is_empty())
        };
        german && !self.templates.iter().any(|(_, en)| anchored(en) && fits(en, text))
    }
}

/// How many characters of `template` are not placeholder.
fn fixed_length(template: &str) -> usize {
    parts(template).iter().map(|p| p.chars().count()).sum()
}

/// The pieces of `template` between its `{…}` placeholders.
fn parts(template: &str) -> Vec<&str> {
    let mut pieces = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        pieces.push(&rest[..open]);
        match rest[open..].find('}') {
            Some(close) => rest = &rest[open + close + 1..],
            None => return pieces,
        }
    }
    pieces.push(rest);
    pieces
}

/// Whether `text` is `template` with something filled in at each placeholder.
fn fits(template: &str, text: &str) -> bool {
    fits_with(template, text, false)
}

/// Like [`fits`]; with `one_word` every placeholder holds a single word or number (a version, a date).
fn fits_with(template: &str, text: &str, one_word: bool) -> bool {
    let pieces = parts(template);
    if pieces.len() < 2 {
        return template == text;
    }
    let (first, last) = (pieces[0], pieces[pieces.len() - 1]);
    if text.len() < first.len() + last.len() || !text.starts_with(first) || !text.ends_with(last) {
        return false;
    }
    let mut rest = &text[first.len()..text.len() - last.len()];
    for piece in &pieces[1..pieces.len() - 1] {
        match rest.find(piece) {
            Some(at) if one_word && rest[..at].contains(char::is_whitespace) => return false,
            Some(at) => rest = &rest[at + piece.len()..],
            None => return false,
        }
    }
    !(one_word && rest.contains(char::is_whitespace))
}

/// Why `text` is not English, or `None`. `text` has no spaces around it.
fn german_because(text: &str, dictionary: &Dictionary) -> Option<&'static str> {
    // A placeholder that was not filled in shows its name on the screen.
    if let Some(open) = text.find('{')
        && text[open..].find('}').is_some_and(|close| {
            close > 1 && text[open + 1..open + close].chars().all(|c| c.is_alphanumeric() || c == '_')
        })
    {
        return Some("a placeholder was not filled in");
    }
    // Names and paths that read the same in both languages are cut out first.
    if dictionary.same.iter().any(|entry| entry == text || (entry.contains('{') && fits_with(entry, text, true))) {
        return None;
    }
    // Names of settings (`farbschema=original`) are identifiers of the files, not wording.
    let mut cleaned = text.split(' ').filter(|word| !word.contains('=')).collect::<Vec<_>>().join(" ");
    for entry in dictionary.same.iter().filter(|entry| !entry.contains('{')) {
        cleaned = cleaned.replace(entry.as_str(), " ");
    }
    if !cleaned.chars().any(char::is_alphabetic) {
        return None;
    }
    if !dictionary.english.contains(text) {
        if dictionary.german.contains(text) {
            return Some("a text of de.json, word for word");
        }
        if dictionary.fits_only_german(text) {
            return Some("a text of de.json with something filled in");
        }
    }
    if cleaned.chars().any(|c| "äöüÄÖÜß".contains(c)) {
        return Some("has an umlaut or ß");
    }
    let has_german_word =
        cleaned.split(|c: char| !c.is_alphanumeric()).any(|word| GERMAN_WORDS.contains(&word.to_lowercase().as_str()));
    has_german_word.then_some("has a German word")
}

/// Every text `page` draws at this size, in three frames, and every name a screen reader would hear.
pub(super) fn drawn_texts(page: &mut SamplePage, width: f32, height: f32) -> Vec<String> {
    fn collect(shape: &egui::Shape, into: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(text) => into.push(text.galley.text().to_string()),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, into)),
            _ => {}
        }
    }
    thread_local! {
        static CTX: egui::Context = {
            let ctx = egui::Context::default();
            crate::install(&ctx);
            ctx.enable_accesskit();
            ctx
        };
    }
    let mut texts = Vec::new();
    CTX.with(|ctx| {
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height))),
                ..Default::default()
            };
            texts.clear();
            let mut output = ctx.run_ui(input, |ui| {
                page.show(ui);
            });
            output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
            if let Some(update) = &output.platform_output.accesskit_update {
                for (_, node) in &update.nodes {
                    texts.extend(node.label().map(str::to_string));
                    texts.extend(node.description().map(str::to_string));
                }
            }
            output.textures_delta.clear();
        }
    });
    texts
}

/// The pages of the window: the overview, every page of the navigation and the ones only a jump leads to.
pub(super) fn pages() -> Vec<&'static str> {
    let mut pages = vec![""];
    pages.extend(NAV.iter().flat_map(|(_, pages)| pages.iter().map(|(page, _)| *page)));
    pages.extend(["hf_wasistneu", "hf_diagnose"]);
    pages
}

/// Runs `check` with the language of this thread set to English.
fn in_english<R>(check: impl FnOnce() -> R) -> R {
    language::set_thread_language(Choice::En);
    theme::set_current("original");
    let result = check();
    language::set_thread_language(Choice::Auto);
    result
}

/// A small dictionary of made-up words, so that no real text and no word of the list helps the check.
fn made_up() -> Dictionary {
    let german = ["Zahnrad"].map(str::to_string).into_iter().collect();
    let english = ["Cogwheel"].map(str::to_string).into_iter().collect();
    Dictionary {
        german,
        english,
        templates: vec![("Zahnrad {n} gedreht".to_string(), "Cogwheel {n} turned".to_string())],
        same: vec!["Zahnrad-Typ A".to_string(), "Gear {} mark".to_string()],
    }
}

/// The check names the right reason for each way a German text can get into a page.
#[test]
fn the_english_check_finds_german_texts() {
    let dictionary = made_up();
    let offences = [
        ("Text aus de.json", "Zahnrad", "a text of de.json, word for word"),
        ("Vorlage gefüllt", "Zahnrad 3 gedreht", "a text of de.json with something filled in"),
        ("Umlaut", "Tür offen", "has an umlaut or ß"),
        ("deutsches Wort", "Open und close", "has a German word"),
        ("Großbuchstaben", "MIT", "has a German word"),
        ("Platzhalter", "Turned {count} times", "a placeholder was not filled in"),
    ];
    for (name, text, reason) in offences {
        assert_eq!(german_because(text, &dictionary), Some(reason), "{name}: {text}");
    }
    let english =
        ["Cogwheel", "Cogwheel 3 turned", "Zahnrad-Typ A", "Gear 7 mark", "Nothing here", "42 %", "Cutlass Black"];
    for text in english {
        assert_eq!(german_because(text, &dictionary), None, "false alarm: {text}");
    }
    // A fill-in of two words is no version or number: the entry does not cover it.
    assert_eq!(german_because("Gear in und mark", &dictionary), Some("has a German word"));
}

/// The real dictionary reads the two language files: a text of `de.json` the check flags, its English twin it lets by.
#[test]
fn the_english_check_reads_the_language_files() {
    let dictionary = Dictionary::new();
    let german = language::t_in(Lang::De, "s_ei_gpu");
    assert_eq!(german, "Grafikkarte nutzen");
    assert_eq!(german_because(&german, &dictionary), Some("a text of de.json, word for word"));
    assert_eq!(german_because(&language::t_in(Lang::En, "s_ei_gpu"), &dictionary), None);
    let filled = language::tf_in(Lang::De, "s_mu_gefunden_in", &[("place", language::Arg::from("Lorville"))]);
    assert!(german_because(&filled, &dictionary).is_some(), "{filled}");
}

/// A page really is drawn, in English, with the check reading what it draws.
#[test]
fn the_english_check_reads_a_drawn_page() {
    let dictionary = Dictionary::new();
    let drawn = in_english(|| {
        let mut page = SamplePage::preview();
        page.go_to("hf_ueber");
        drawn_texts(&mut page, 1280.0, 900.0)
    });
    assert!(drawn.iter().any(|t| t == &language::t_in(Lang::En, "s_up_sofort")), "{drawn:?}");
    let found: Vec<&String> = drawn.iter().filter(|t| german_because(t.trim(), &dictionary).is_some()).collect();
    assert!(found.is_empty(), "{found:?}");
    // The same page in German is full of what the check calls German: it would fail it.
    language::set_thread_language(Choice::De);
    let mut page = SamplePage::preview();
    page.go_to("hf_ueber");
    let german = drawn_texts(&mut page, 1280.0, 900.0);
    language::set_thread_language(Choice::Auto);
    assert!(german.iter().filter(|t| german_because(t.trim(), &dictionary).is_some()).count() > 3, "{german:?}");
}

/// Every text in the table of the settings is a key of the language file, or one of the
/// few that read the same in both languages: a bare German text would be shown in English as it is.
#[test]
fn the_settings_table_holds_only_keys() {
    let reads_the_same = ["English", "Deutsch", "LIVE", "HOTFIX", "PTU", "EPTU", "TECH-PREVIEW", "Alt", "s", "UHD 125 %", "UHD 150 %"];
    let mut bare = Vec::new();
    for (page, groups) in super::PAGE_SETTINGS {
        for (title, rows) in groups {
            let mut texts = vec![*title];
            for (name, hint, choices) in rows.iter() {
                texts.extend([*name, *hint]);
                texts.extend(choices.iter().copied());
            }
            for text in texts {
                let is_key = language::text_in(Lang::De, text).is_some();
                if !text.is_empty() && !is_key && !reads_the_same.contains(&text) {
                    bare.push(format!("{page}: {text}"));
                }
            }
        }
    }
    assert!(bare.is_empty(), "text in the table that is no language key:\n{}", bare.join("\n"));
}
