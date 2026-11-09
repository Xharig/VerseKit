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

//! What is written into a contract: the title addition and the block under
//! the description (`injection._title_suffix`, `_build_block`).
//!
//! ⚠ The words here are game file content in the language of the game's
//! text file, not interface texts: they follow the language folder of the
//! `global.ini`, not the interface language, and must stay byte for byte as
//! 4.12 wrote them, because the fallback finds earlier blocks by them.
//!
//! A contract entry is the JSON of the catalog (`missionen`), read with
//! Python's truth rules, so that every quirk of the format stays.

use crate::jsonio::{Map, Value, float_text};

use super::form::{BOX_HAVE, BOX_MISSING};

/// The words of one language (`injection.TEXTS`).
#[derive(Clone, Debug)]
pub struct Words {
    pub short: &'static str,
    pub rep_short: &'static str,
    pub heading: &'static str,
    pub chance: &'static str,
    pub rep_min: &'static str,
    pub rep_max: &'static str,
    pub payout: &'static str,
    pub gain: &'static str,
    pub gain_at: &'static str,
    pub minutes: &'static str,
    pub minute: &'static str,
    pub cooldown: &'static str,
    pub cooldown_abandoned: &'static str,
    pub buy_in: &'static str,
    /// `Wieder annehmbar: nach Abbruch Ja, nach Fehlschlag Nein`.
    pub again: &'static str,
    pub after_abandoning: &'static str,
    pub after_failing: &'static str,
    /// The heading of the block of a contract without blueprints.
    pub contract_heading: &'static str,
    /// The last line of that block, by where its terms come from.
    pub terms_from_game: &'static str,
    pub terms_from_scmdb: &'static str,
    pub shareable: &'static str,
    pub region: &'static str,
    pub yes: &'static str,
    pub no: &'static str,
    pub list: &'static str,
    pub from_rank: &'static str,
    /// With two `%d`: empty tiers and all tiers.
    pub empty_tiers: &'static str,
    pub separator: &'static str,
    pub it_heading: &'static str,
    pub it_blueprint: &'static str,
    pub it_have: &'static str,
    pub it_missing: &'static str,
    pub it_from: &'static str,
    pub it_from_rank: &'static str,
    pub it_more: &'static str,
    pub it_no_contract: &'static str,
    pub it_crafting: &'static str,
    pub it_ingredient: &'static str,
    pub it_ingredients: &'static str,
    pub it_shop: &'static str,
    pub it_not_sold: &'static str,
    /// `N variants owned`, for a description several items share.
    pub it_variants_have: &'static str,
    /// `N variants missing`, for a description several items share.
    pub it_variants_missing: &'static str,
    /// The setting „Rufschwelle im Auftragstitel“, read once per run.
    pub rep_on: bool,
}

pub const GERMAN: Words = Words {
    short: "BP",
    rep_short: "Rep",
    heading: "MÖGLICHE BAUPLÄNE FÜR DIESEN MISSIONSTYP",
    chance: "Chance auf Bauplan",
    rep_min: "Min. Reputation",
    rep_max: "Max. Reputation",
    payout: "Belohnung",
    gain: "Rufpunkte",
    gain_at: "Ruf",
    minutes: "Minuten",
    minute: "Minute",
    cooldown: "Wartezeit",
    cooldown_abandoned: "Wartezeit nach Abbruch",
    buy_in: "Buy-in",
    again: "Wieder annehmbar",
    after_abandoning: "nach Abbruch",
    after_failing: "nach Fehlschlag",
    contract_heading: "AUFTRAGSANGABEN",
    terms_from_game: "Angaben aus den Spieldaten · eingefügt von VerseKit",
    terms_from_scmdb: "Angaben von scmdb.net · eingefügt von VerseKit",
    shareable: "Mission teilbar",
    region: "Region",
    yes: "Ja",
    no: "Nein",
    list: "Baupläne — angehakt ist, was du hast",
    from_rank: "erst ab",
    empty_tiers: "Achtung: %d der %d Stufen dieses Auftrags geben gar keine Baupläne.",
    separator: ".",
    it_heading: "VERSEKIT: BAUPLAN & HERSTELLUNG",
    it_blueprint: "Bauplan",
    it_have: "vorhanden",
    it_missing: "fehlt",
    it_from: "aus",
    it_from_rank: "ab Rang",
    it_more: "weitere",
    it_no_contract: "nicht aus Aufträgen",
    it_crafting: "Herstellen",
    it_ingredient: "Zutat",
    it_ingredients: "Zutaten",
    it_shop: "Im Laden",
    it_not_sold: "nicht erhältlich, nur Herstellen",
    it_variants_have: "Varianten vorhanden",
    it_variants_missing: "Varianten fehlen",
    rep_on: true,
};

pub const ENGLISH: Words = Words {
    short: "BP",
    rep_short: "Rep",
    heading: "POSSIBLE BLUEPRINTS FOR THIS MISSION TYPE",
    chance: "Blueprint chance",
    rep_min: "Min. reputation",
    rep_max: "Max. reputation",
    payout: "Payout",
    gain: "Reputation gain",
    gain_at: "Reputation",
    minutes: "minutes",
    minute: "minute",
    cooldown: "Cooldown",
    cooldown_abandoned: "Cooldown after abandoning",
    buy_in: "Buy-in",
    again: "Can be taken again",
    after_abandoning: "after abandoning",
    after_failing: "after failing",
    contract_heading: "CONTRACT DETAILS",
    terms_from_game: "Data from the game files · added by VerseKit",
    terms_from_scmdb: "Data from scmdb.net · added by VerseKit",
    shareable: "Shareable",
    region: "Region",
    yes: "Yes",
    no: "No",
    list: "Blueprints — ticked means you have it",
    from_rank: "needs",
    empty_tiers: "Note: %d of the %d tiers of this contract give no blueprints at all.",
    separator: ",",
    it_heading: "VERSEKIT: BLUEPRINT & CRAFTING",
    it_blueprint: "Blueprint",
    it_have: "owned",
    it_missing: "missing",
    it_from: "from",
    it_from_rank: "needs rank",
    it_more: "more",
    it_no_contract: "not from contracts",
    it_crafting: "Crafting",
    it_ingredient: "ingredient",
    it_ingredients: "ingredients",
    it_shop: "In shops",
    it_not_sold: "not sold, crafting only",
    it_variants_have: "variants owned",
    it_variants_missing: "variants missing",
    rep_on: true,
};

/// The rule between blocks (57 dashes).
pub fn rule() -> String {
    "-".repeat(57)
}

/// `injection._lang_code`: `german_(germany)` is `de`, everything else `en`.
pub fn lang_code(language: &str) -> &'static str {
    if language.to_lowercase().starts_with("german") { "de" } else { "en" }
}

/// The words for a language folder.
pub fn words_for(language: &str, rep_on: bool) -> Words {
    let base = if lang_code(language) == "de" { GERMAN } else { ENGLISH };
    Words { rep_on, ..base }
}

/// Python's truth value of a JSON value.
pub fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Bool(true)) => true,
        Some(Value::Int(n)) => n != "0",
        Some(Value::Float(x)) => *x != 0.0,
        Some(Value::Str(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

/// Python's `str()` of a JSON value (`'%s' % value`).
pub fn py_str(value: &Value) -> String {
    match value {
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Int(n) => n.clone(),
        Value::Float(x) => float_text(*x),
        Value::Str(s) => s.clone(),
        other => crate::jsonio::dumps(other, None, false),
    }
}

/// Python's `int()` of a JSON number (floats toward zero); `None` else.
pub fn py_int(value: Option<&Value>) -> Option<i128> {
    match value? {
        Value::Int(n) => n.parse().ok(),
        Value::Float(x) if x.is_finite() => Some(x.trunc() as i128),
        Value::Bool(b) => Some(i128::from(*b)),
        _ => None,
    }
}

/// `format(n, ',d')` with the separator of the language.
pub fn group_digits(n: i128, separator: &str) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push_str(separator);
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// `injection._group_digits(value, words)`, with `value or 0` already done.
fn digits_of(value: Option<&Value>, words: &Words) -> String {
    group_digits(py_int(value).unwrap_or(0), words.separator)
}

fn get<'a>(entry: &'a Map, key: &str) -> Option<&'a Value> {
    entry.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn has(entry: &Map, key: &str) -> bool {
    entry.iter().any(|(k, _)| k == key)
}

/// `'%d' % round(x)` of Python for a JSON number: ties to even.
fn percent(chance: &Value) -> String {
    match chance {
        Value::Int(n) => match n.parse::<i128>() {
            Ok(n) => (n * 100).to_string(),
            Err(_) => n.clone(),
        },
        Value::Float(x) => format!("{}", (x * 100.0).round_ties_even() as i128),
        Value::Bool(b) => (i128::from(*b) * 100).to_string(),
        other => py_str(other),
    }
}

/// `injection._rep_part`: ` 10.000 Rep` inside our mark.
fn rep_part(entry: &Map, words: &Words) -> String {
    if !words.rep_on || !truthy(get(entry, "rep_eindeutig")) {
        return String::new();
    }
    let positive = match get(entry, "rep") {
        Some(Value::Int(n)) => n.parse::<i128>().is_ok_and(|n| n > 0),
        Some(Value::Float(x)) => *x > 0.0,
        _ => false,
    };
    if !positive {
        return String::new();
    }
    format!(" {} {}", digits_of(get(entry, "rep"), words), words.rep_short)
}

/// `injection._title_suffix`: `[BP]`, `[BP!]`, `[BP 10.000 Rep]`.
pub fn title_suffix(entry: &Map, words: &Words) -> String {
    let note = get(entry, "bpnote").and_then(Value::as_str).unwrap_or("");
    let bang = if crate::text::py_strip(note).is_empty() { "" } else { "!" };
    format!(" <EM4>[{}{bang}{}]</EM4>", words.short, rep_part(entry, words))
}

fn blue(text: &str) -> String {
    format!("<EM4>{text}</EM4>")
}

fn names(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(list)) => list.iter().map(py_str).collect(),
        _ => Vec::new(),
    }
}

/// What the contract block needs besides the entry.
pub struct BlockContext<'a> {
    pub words: &'a Words,
    /// Names of the player's blueprints in comparison form.
    pub owned: &'a std::collections::BTreeSet<String>,
    /// `# Ruf: …` and `# Region: …` of this contract, or empty.
    pub reputation_line: String,
    pub region_line: String,
    /// The terms the block does not name otherwise (see [`super::terms::lines`]).
    pub terms_lines: Vec<String>,
    /// The last line: where the notes come from (`terms_from_game` or `terms_from_scmdb` of [`Words`]).
    pub source: &'a str,
}

/// The block appended to the description of a contract without blueprints: its terms, reputation and region
/// (`lines`, each `# …` or marked), and where they come from (`source`).
pub fn contract_block(lines: &[String], words: &Words, source: &str) -> String {
    let mut z: Vec<String> = vec![String::new(), rule(), String::new(), blue(words.contract_heading), String::new()];
    z.extend(lines.iter().cloned());
    z.push(String::new());
    z.push(source.to_string());
    format!("\\n{}", z.join("\\n"))
}

/// `injection._build_block`: the block appended to a description.
pub fn build_block(entry: &Map, ctx: &BlockContext<'_>) -> String {
    let words = ctx.words;
    let mut z: Vec<String> = vec![String::new(), rule(), String::new(), blue(words.heading), String::new()];
    if let Some(chance) = get(entry, "chance").filter(|v| truthy(Some(v))) {
        z.push(format!("# {}: {}%", words.chance, percent(chance)));
    }
    if let Some(rank) = get(entry, "rang").filter(|v| truthy(Some(v))) {
        z.push(blue(&format!("# {}: {} ({} XP)", words.rep_min, py_str(rank), digits_of(get(entry, "rep"), words))));
    }
    if let Some(rank) = get(entry, "rang_max").filter(|v| truthy(Some(v))) {
        z.push(blue(&format!(
            "# {}: {} ({} XP)",
            words.rep_max,
            py_str(rank),
            digits_of(get(entry, "rep_max"), words)
        )));
    }
    if truthy(get(entry, "uec")) {
        z.push(format!("# {}: {} aUEC", words.payout, digits_of(get(entry, "uec"), words)));
    }
    if truthy(get(entry, "ruf")) {
        z.push(blue(&format!("# {}: {} XP", words.gain, digits_of(get(entry, "ruf"), words))));
    }
    if !ctx.reputation_line.is_empty() {
        z.push(blue(&ctx.reputation_line));
    }
    if truthy(get(entry, "cooldown")) {
        z.push(format!("# {}: {} {}", words.cooldown, digits_of(get(entry, "cooldown"), words), words.minutes));
    }
    if has(entry, "teilbar") {
        let said = if truthy(get(entry, "teilbar")) { words.yes } else { words.no };
        z.push(format!("# {}: {said}", words.shareable));
    }
    z.extend(ctx.terms_lines.iter().cloned());
    if !ctx.region_line.is_empty() {
        z.push(ctx.region_line.clone());
    }
    z.push(String::new());
    z.push(format!("# {}:", words.list));

    let all = names(get(entry, "bp"));
    let from = get(entry, "ab").and_then(Value::as_object);
    let width = all.iter().map(|n| n.chars().count()).max().unwrap_or(0);
    let box_line = |name: &str| {
        let inside = ctx.owned.contains(&crate::paths::name_key(name));
        let mut line = format!("   {} {name}", if inside { BOX_HAVE } else { BOX_MISSING });
        if let Some(cond) = from.and_then(|f| f.iter().find(|(k, _)| k == name)).map(|(_, v)| v)
            && truthy(Some(cond))
        {
            let pad = " ".repeat(width.saturating_sub(name.chars().count()));
            let rank = cond.get("rang").map(py_str).unwrap_or_else(|| "None".into());
            line.push_str(&format!("{pad}  {} {rank} ({} XP)", words.from_rank, digits_of(cond.get("rep"), words)));
        }
        line
    };
    match get(entry, "je_system").and_then(Value::as_object).filter(|m| !m.is_empty()) {
        Some(per_system) => {
            for (system, list) in per_system {
                z.push(String::new());
                z.push(blue(&format!("# {system}:")));
                z.extend(names(Some(list)).iter().map(|n| box_line(n)));
            }
        }
        None => z.extend(all.iter().map(|n| box_line(n))),
    }
    if truthy(get(entry, "leer")) {
        let said = words.empty_tiers.replacen("%d", &py_int(get(entry, "leer")).unwrap_or(0).to_string(), 1).replacen(
            "%d",
            &py_int(get(entry, "stufen")).unwrap_or(0).to_string(),
            1,
        );
        z.push(String::new());
        z.push(format!("# {said}"));
    }
    z.push(String::new());
    z.push(ctx.source.to_string());
    format!("\\n{}", z.join("\\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsonio;

    fn entry(json: &str) -> Map {
        jsonio::loads(json).unwrap().as_object().unwrap().clone()
    }

    #[test]
    fn digits_like_python() {
        assert_eq!(group_digits(1234567, "."), "1.234.567");
        assert_eq!(group_digits(-1234, ","), "-1,234");
        assert_eq!(group_digits(999, "."), "999");
        assert_eq!(group_digits(0, "."), "0");
    }

    #[test]
    fn title_suffix_forms() {
        let w = words_for("german_(germany)", true);
        assert_eq!(title_suffix(&entry(r#"{"rep": 10000, "rep_eindeutig": true}"#), &w), " <EM4>[BP 10.000 Rep]</EM4>");
        assert_eq!(title_suffix(&entry(r#"{"rep": 10000}"#), &w), " <EM4>[BP]</EM4>");
        assert_eq!(
            title_suffix(&entry(r#"{"rep": 0, "rep_eindeutig": true, "bpnote": "x"}"#), &w),
            " <EM4>[BP!]</EM4>"
        );
        let off = words_for("english", false);
        assert_eq!(title_suffix(&entry(r#"{"rep": 10000, "rep_eindeutig": true}"#), &off), " <EM4>[BP]</EM4>");
    }

    #[test]
    fn chance_rounds_half_to_even() {
        assert_eq!(percent(&Value::Float(0.125)), "12");
        assert_eq!(percent(&Value::Float(0.135)), "14");
        assert_eq!(percent(&Value::int(1)), "100");
    }

    #[test]
    fn block_with_ranks_and_systems() {
        let e = entry(
            r#"{"bp": ["Abc", "Longer Name"], "chance": 0.5, "rang": "Jr", "rep": 800, "teilbar": false,
                "ab": {"Longer Name": {"rang": "Sr", "rep": 1500}},
                "je_system": {"Stanton": ["Abc"], "Pyro": ["Longer Name"]}, "leer": 1, "stufen": 3}"#,
        );
        let owned = [crate::paths::name_key("Abc")].into_iter().collect();
        let w = words_for("english", true);
        let ctx = BlockContext {
            words: &w,
            owned: &owned,
            reputation_line: String::new(),
            region_line: String::new(),
            terms_lines: Vec::new(),
            source: w.terms_from_game,
        };
        let block = build_block(&e, &ctx);
        let lines: Vec<&str> = block.split("\\n").collect();
        assert_eq!(lines[2], "-".repeat(57));
        assert!(lines.contains(&"# Blueprint chance: 50%"));
        assert!(lines.contains(&"<EM4># Min. reputation: Jr (800 XP)</EM4>"));
        assert!(lines.contains(&"# Shareable: No"));
        assert!(lines.contains(&"   <EM4>[x]</EM4> Abc"));
        assert!(lines.contains(&"   [  ] Longer Name  needs Sr (1,500 XP)"));
        assert!(lines.contains(&"# Note: 1 of the 3 tiers of this contract give no blueprints at all."));
        assert_eq!(lines.last(), Some(&"Data from the game files · added by VerseKit"));
    }
}
