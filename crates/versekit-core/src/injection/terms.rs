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

//! The terms of a contract the notes name for every contract: cooldowns, buy-in, whether it can be shared and taken
//! again. One [`Terms`] per contract of the game data; [`summarize`] joins the contracts of one title into the table
//! part `angaben` of `auftragsruf.json`, [`lines`] writes an entry of it.
//!
//! An entry holds a number as a number when every contract of the title that states it states the same, as
//! `[min, max]` when they differ; a contract without it adds no 0. A yes or no only when they all agree. A cooldown of
//! 0 is no cooldown.

use super::reputation::key;
use super::texts::{Words, group_digits};
use crate::jsonio::{Map, Value};

/// What the game data state of one contract.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Terms {
    /// The personal cooldown in minutes, `None` when the contract has none.
    pub cooldown: Option<i64>,
    /// How far the personal cooldown may vary either way, in minutes.
    pub cooldown_spread: Option<i64>,
    /// The cooldown after abandoning it, in minutes.
    pub abandoned: Option<i64>,
    /// How far that cooldown may vary either way, in minutes.
    pub abandoned_spread: Option<i64>,
    /// What it costs to take it (aUEC).
    pub buy_in: Option<i64>,
    pub shareable: Option<bool>,
    /// Whether it can be taken again after abandoning it, and after failing it.
    pub again_abandoned: Option<bool>,
    pub again_failed: Option<bool>,
    /// The text key of its description.
    pub description: Option<String>,
    /// The lowest and the highest rank that get it: name and the reputation it starts at.
    pub lowest: Option<(String, i64)>,
    pub highest: Option<(String, i64)>,
    /// The payout in aUEC where the game data state it as a fixed amount; `None` for a payout the game computes.
    pub reward: Option<i64>,
}

/// The entries of `contracts` (title key as the game data name it, terms) by title key without `@` in lower case, in
/// the order the titles first occur; every title has one, an empty one when nothing is known of it.
pub fn summarize(contracts: &[(String, Terms)]) -> Map {
    let mut grouped: Vec<(String, Vec<&Terms>)> = Vec::new();
    for (title, terms) in contracts {
        let title = key(title);
        if title.is_empty() {
            continue;
        }
        match grouped.iter_mut().find(|(known, _)| *known == title) {
            Some((_, list)) => list.push(terms),
            None => grouped.push((title, vec![terms])),
        }
    }
    grouped.into_iter().map(|(title, list)| (title, Value::Object(entry_of(&list)))).collect()
}

fn entry_of(list: &[&Terms]) -> Map {
    let mut entry = Map::new();
    // The rank of the easiest variant, and the highest rank any variant still takes.
    let lowest = list.iter().filter_map(|t| t.lowest.as_ref()).min_by_key(|(_, rep)| *rep);
    let highest = list.iter().filter_map(|t| t.highest.as_ref()).max_by_key(|(_, rep)| *rep);
    for ((rank_name, rep_name), found) in [(("rang", "rep"), lowest), (("rang_max", "rep_max"), highest)] {
        if let Some((rank, rep)) = found {
            entry.push((rank_name.to_string(), Value::str(rank)));
            entry.push((rep_name.to_string(), Value::int(*rep)));
        }
    }
    let numbers = [
        ("warte", list.iter().map(|t| t.cooldown).collect::<Vec<_>>()),
        ("warte_abbruch", list.iter().map(|t| t.abandoned).collect()),
        ("buyin", list.iter().map(|t| t.buy_in).collect()),
    ];
    for (name, values) in numbers {
        // Only the variants that state the value: one without it is no 0 in the span.
        let values: Vec<i64> = values.into_iter().flatten().filter(|v| *v > 0).collect();
        let (Some(&low), Some(&high)) = (values.iter().min(), values.iter().max()) else { continue };
        let value = if low == high { Value::int(high) } else { Value::Array(vec![Value::int(low), Value::int(high)]) };
        entry.push((name.to_string(), value));
        // The largest spread of the variants that have this cooldown at all.
        let spread = match name {
            "warte" => list.iter().filter(|t| t.cooldown.unwrap_or(0) > 0).filter_map(|t| t.cooldown_spread).max(),
            "warte_abbruch" => {
                list.iter().filter(|t| t.abandoned.unwrap_or(0) > 0).filter_map(|t| t.abandoned_spread).max()
            }
            _ => None,
        };
        if let Some(spread) = spread.filter(|s| *s > 0) {
            entry.push((format!("{name}_streuung"), Value::int(spread)));
        }
    }
    // The payout only where every contract of the title states a fixed one: a computed one has no amount.
    let rewards: Option<Vec<i64>> = list.iter().map(|t| t.reward).collect();
    if let Some(rewards) = rewards.filter(|r| !r.is_empty()) {
        let (low, high) = (rewards.iter().min().copied().unwrap_or(0), rewards.iter().max().copied().unwrap_or(0));
        let value = if low == high { Value::int(high) } else { Value::Array(vec![Value::int(low), Value::int(high)]) };
        entry.push(("belohnung".to_string(), value));
    }
    let flags = [
        ("teilbar", list.iter().map(|t| t.shareable).collect::<Vec<_>>()),
        ("wieder_abbruch", list.iter().map(|t| t.again_abandoned).collect()),
        ("wieder_fehlschlag", list.iter().map(|t| t.again_failed).collect()),
    ];
    for (name, values) in flags {
        if let Some(Some(first)) = values.first()
            && values.iter().all(|v| *v == Some(*first))
        {
            entry.push((name.to_string(), Value::Bool(*first)));
        }
    }
    let mut texts: Vec<Value> = Vec::new();
    for text in list.iter().filter_map(|t| t.description.as_deref()).map(key).filter(|t| !t.is_empty()) {
        if !texts.iter().any(|known| known.as_str() == Some(text.as_str())) {
            texts.push(Value::Str(text));
        }
    }
    texts.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
    if !texts.is_empty() {
        entry.push(("texte".to_string(), Value::Array(texts)));
    }
    entry
}

fn number(value: &Value) -> Option<i128> {
    super::texts::py_int(Some(value))
}

/// `45` or `30–45`, with the digit groups of the language.
fn amount(value: &Value, words: &Words) -> Option<String> {
    match value {
        Value::Array(pair) if pair.len() == 2 => {
            let (low, high) = (number(&pair[0])?, number(&pair[1])?);
            Some(format!("{}–{}", group_digits(low, words.separator), group_digits(high, words.separator)))
        }
        other => number(other).map(|n| group_digits(n, words.separator)),
    }
}

fn minutes(value: &Value, words: &Words) -> Option<String> {
    let unit = if matches!(number(value), Some(1)) { words.minute } else { words.minutes };
    Some(format!("{} {unit}", amount(value, words)?))
}

fn get<'a>(entry: &'a Map, name: &str) -> Option<&'a Value> {
    entry.iter().find(|(k, _)| k == name).map(|(_, v)| v)
}

fn flag(entry: &Map, name: &str) -> Option<bool> {
    match get(entry, name) {
        Some(Value::Bool(value)) => Some(*value),
        _ => None,
    }
}

/// The lines of an entry, each `# …` (the ranks marked like in the blueprint block). `skip_known` leaves out the
/// ranks, the cooldown and whether it can be shared, which the blueprint block names already. The lowest rank is
/// named only when it needs reputation.
pub fn lines(entry: &Map, words: &Words, skip_known: bool) -> Vec<String> {
    let mut out = Vec::new();
    let yes_no = |value: bool| if value { words.yes } else { words.no };
    let rank = |rank_name: &str, rep_name: &str, label: &str, needs_some: bool| {
        let rank = get(entry, rank_name).and_then(Value::as_str).filter(|r| !r.is_empty())?;
        let rep = get(entry, rep_name).and_then(number)?;
        (!needs_some || rep > 0)
            .then(|| format!("<EM4># {label}: {rank} ({} XP)</EM4>", group_digits(rep, words.separator)))
    };
    if !skip_known {
        out.extend(rank("rang", "rep", words.rep_min, true));
        out.extend(rank("rang_max", "rep_max", words.rep_max, false));
    }
    let spread = |name: &str| {
        get(entry, &format!("{name}_streuung"))
            .and_then(number)
            .map_or_else(String::new, |s| format!(" (±{})", group_digits(s, words.separator)))
    };
    if !skip_known && let Some(text) = get(entry, "warte").and_then(|v| minutes(v, words)) {
        out.push(format!("# {}: {text}{}", words.cooldown, spread("warte")));
    }
    if let Some(text) = get(entry, "warte_abbruch").and_then(|v| minutes(v, words)) {
        out.push(format!("# {}: {text}{}", words.cooldown_abandoned, spread("warte_abbruch")));
    }
    if let Some(text) = get(entry, "buyin").and_then(|v| amount(v, words)) {
        out.push(format!("# {}: {text} aUEC", words.buy_in));
    }
    if !skip_known && let Some(value) = flag(entry, "teilbar") {
        out.push(format!("# {}: {}", words.shareable, yes_no(value)));
    }
    let again: Vec<String> = [(words.after_abandoning, "wieder_abbruch"), (words.after_failing, "wieder_fehlschlag")]
        .into_iter()
        .filter_map(|(when, name)| flag(entry, name).map(|value| format!("{when} {}", yes_no(value))))
        .collect();
    if !again.is_empty() {
        out.push(format!("# {}: {}", words.again, again.join(", ")));
    }
    out
}

/// The entry of a title key (with or without `@`, in any case).
pub fn entry_for<'a>(table: &'a Map, title: &str) -> Option<&'a Map> {
    let wanted = key(title);
    table.iter().find(|(k, _)| *k == wanted).and_then(|(_, v)| v.as_object())
}

/// Description key (lower case) → every title key whose entry names it, in the order of the table.
pub fn by_description(table: &Map) -> std::collections::HashMap<String, Vec<String>> {
    let mut found: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for (title, entry) in table {
        if let Some(Value::Array(texts)) = entry.as_object().and_then(|e| get(e, "texte")) {
            for text in texts.iter().filter_map(Value::as_str) {
                found.entry(text.to_string()).or_default().push(title.clone());
            }
        }
    }
    found
}

/// What `of` gives for every title of `titles` when it gives the same for all, else the empty value: a description
/// several titles share tells only what holds for each of them.
pub fn agreed<T: PartialEq + Default>(titles: &[String], of: impl Fn(&str) -> T) -> T {
    let Some((first, rest)) = titles.split_first() else { return T::default() };
    let value = of(first);
    if rest.iter().all(|title| of(title) == value) { value } else { T::default() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::injection::texts::words_for;
    use crate::jsonio;

    fn terms(cooldown: Option<i64>, buy_in: Option<i64>, shareable: Option<bool>, text: &str) -> Terms {
        Terms {
            cooldown,
            cooldown_spread: None,
            abandoned: Some(1),
            abandoned_spread: None,
            buy_in,
            shareable,
            again_abandoned: Some(true),
            again_failed: Some(false),
            description: Some(text.to_string()),
            lowest: None,
            highest: None,
            reward: None,
        }
    }

    /// A variant without a cooldown adds no 0 to the span of those that have one.
    #[test]
    fn a_missing_value_is_no_zero_in_the_span() {
        let table = summarize(&[
            ("t".into(), Terms { cooldown: Some(45), buy_in: Some(500), ..Terms::default() }),
            ("t".into(), Terms { cooldown: None, buy_in: Some(900), ..Terms::default() }),
        ]);
        assert_eq!(
            jsonio::dumps(&Value::Object(entry_for(&table, "t").unwrap().clone()), None, false),
            r#"{"warte": 45, "buyin": [500, 900]}"#
        );
    }

    /// A fixed payout stands in the entry; a title with a computed one, or with both, has none.
    #[test]
    fn the_payout_stands_only_where_it_is_fixed() {
        let paid = |reward: Option<i64>| Terms { reward, ..Terms::default() };
        let table = summarize(&[
            ("fixed".into(), paid(Some(134_500))),
            ("computed".into(), paid(None)),
            ("mixed".into(), paid(Some(40_000))),
            ("mixed".into(), paid(None)),
            ("tiers".into(), paid(Some(1_000))),
            ("tiers".into(), paid(Some(3_000))),
        ]);
        let reward = |title: &str| entry_for(&table, title).and_then(|e| e.iter().find(|(k, _)| k == "belohnung")).map(|(_, v)| v.clone());
        assert_eq!(reward("fixed"), Some(Value::int(134_500)));
        assert_eq!(reward("computed"), None);
        assert_eq!(reward("mixed"), None);
        assert_eq!(reward("tiers"), Some(Value::Array(vec![Value::int(1_000), Value::int(3_000)])));
    }

    #[test]
    fn the_ranks_are_the_lowest_and_the_highest_of_the_variants() {
        let tier = |low: i64, high: i64| Terms {
            lowest: Some((format!("R{low}"), low)),
            highest: Some((format!("R{high}"), high)),
            ..Terms::default()
        };
        let table =
            summarize(&[("t".into(), tier(800, 5000)), ("t".into(), tier(0, 95250)), ("t".into(), tier(2200, 9000))]);
        let entry = entry_for(&table, "t").unwrap();
        assert_eq!(
            jsonio::dumps(&Value::Object(entry.clone()), None, false),
            r#"{"rang": "R0", "rep": 0, "rang_max": "R95250", "rep_max": 95250}"#
        );
        // A lowest rank without reputation is not named; the blueprint block names both itself.
        assert_eq!(
            lines(entry, &words_for("german_(germany)", true), false),
            ["<EM4># Max. Reputation: R95250 (95.250 XP)</EM4>"]
        );
        assert!(lines(entry, &words_for("english", true), true).is_empty());
        let needs = summarize(&[("n".into(), tier(800, 5000))]);
        assert_eq!(
            lines(entry_for(&needs, "n").unwrap(), &words_for("english", true), false),
            ["<EM4># Min. reputation: R800 (800 XP)</EM4>", "<EM4># Max. reputation: R5000 (5,000 XP)</EM4>"]
        );
    }

    #[test]
    fn the_contracts_of_a_title_are_one_entry() {
        let table = summarize(&[
            (
                "@TestGang_Raid_E_title_001".into(),
                terms(Some(45), Some(13500), Some(false), "@TestGang_Raid_E_desc_001"),
            ),
            ("other_title".into(), terms(Some(30), None, Some(true), "other_desc")),
            ("other_title".into(), terms(Some(45), None, Some(false), "other_desc_002")),
            ("".into(), terms(Some(5), None, None, "x")),
            ("nothing_title".into(), Terms::default()),
        ]);
        assert_eq!(
            jsonio::dumps(&Value::Object(table.clone()), None, false),
            concat!(
                r#"{"testgang_raid_e_title_001": {"warte": 45, "warte_abbruch": 1, "buyin": 13500, "teilbar": false, "#,
                r#""wieder_abbruch": true, "wieder_fehlschlag": false, "texte": ["testgang_raid_e_desc_001"]}, "#,
                r#""other_title": {"warte": [30, 45], "warte_abbruch": 1, "wieder_abbruch": true, "wieder_fehlschlag": false, "#,
                r#""texte": ["other_desc", "other_desc_002"]}, "nothing_title": {}}"#
            )
        );
        assert_eq!(by_description(&table).get("other_desc_002"), Some(&vec!["other_title".to_string()]));
        assert!(entry_for(&table, "@TESTGANG_RAID_E_TITLE_001").is_some());
    }

    #[test]
    fn the_lines_in_both_languages() {
        let table = summarize(&[("t_title".into(), terms(Some(45), Some(13500), Some(false), "t_desc"))]);
        let entry = entry_for(&table, "t_title").unwrap();
        assert_eq!(
            lines(entry, &words_for("german_(germany)", true), false),
            [
                "# Wartezeit: 45 Minuten",
                "# Wartezeit nach Abbruch: 1 Minute",
                "# Buy-in: 13.500 aUEC",
                "# Mission teilbar: Nein",
                "# Wieder annehmbar: nach Abbruch Ja, nach Fehlschlag Nein",
            ]
        );
        assert_eq!(
            lines(entry, &words_for("english", true), true),
            [
                "# Cooldown after abandoning: 1 minute",
                "# Buy-in: 13,500 aUEC",
                "# Can be taken again: after abandoning Yes, after failing No",
            ]
        );
        let range = summarize(&[
            ("r_title".into(), terms(Some(30), Some(1000), None, "r")),
            ("r_title".into(), terms(Some(1500), Some(2000), None, "r")),
        ]);
        assert_eq!(
            lines(entry_for(&range, "r_title").unwrap(), &words_for("english", true), false)[..2],
            ["# Cooldown: 30–1,500 minutes", "# Cooldown after abandoning: 1 minute",]
        );
    }
}
