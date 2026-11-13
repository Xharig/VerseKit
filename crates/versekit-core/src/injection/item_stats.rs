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

//! The stock values of a weapon or component under its description: below a
//! rule and the heading `VERSEKIT: WERTE AB WERK` stand at most four lines
//! with what matters for comparing the item at a shop terminal (`DPS 1.266 ·
//! Alpha 63 · 1.200 Schuss/min`).
//!
//! The values come from the catalog of the game data
//! ([`CATALOG_KEY`]): per item its name and description key, its kind and its
//! values by their German name. A description several items share gets the
//! block only when it reads the same for all of them.
//!
//! ⚠ The words are game file content in the language of the game's text file,
//! like those of [`super::texts`]; the headings are in
//! [`super::form::HEADINGS`], so that an earlier block is found again.

use std::collections::{HashMap, HashSet};

use crate::jsonio::Value;
use crate::text::py_strip;

/// The setting „Werte in den Beschreibungen“.
pub const SETTING: &str = "werte_in_beschreibung";

/// The key of the values in the catalog file of the game data.
pub const CATALOG_KEY: &str = "werte";

/// The kinds of item the catalog names (`art`).
pub mod kind {
    pub const SHIP_WEAPON: &str = "schiffswaffe";
    pub const PERSONAL_WEAPON: &str = "handwaffe";
    pub const SHIELD: &str = "schild";
    pub const COOLER: &str = "kuehler";
    pub const POWER_PLANT: &str = "kraftwerk";
    pub const QUANTUM_DRIVE: &str = "quantenantrieb";
    pub const MISSILE: &str = "rakete";
    pub const ARMOR: &str = "ruestung";
}

/// The names of the values in the catalog file (`w`).
pub mod value {
    pub const DPS: &str = "dps";
    pub const ALPHA: &str = "alpha";
    pub const FIRE_RATE: &str = "feuerrate";
    pub const RANGE: &str = "reichweite";
    pub const SPEED: &str = "tempo";
    pub const SPREAD: &str = "streuung";
    pub const AMMO: &str = "munition";
    pub const PENETRATION: &str = "durchschlag";
    pub const HEALTH: &str = "hp";
    pub const POWER: &str = "strom";
    pub const EM: &str = "em";
    pub const IR: &str = "ir";
    pub const DISTORTION_HP: &str = "distortion";
    pub const SHIELD_HP: &str = "schild_hp";
    pub const REGEN: &str = "regeneration";
    pub const DAMAGED_DELAY: &str = "verzoegerung_treffer";
    pub const DOWNED_DELAY: &str = "verzoegerung_ausfall";
    pub const RESIST_PHYSICAL: &str = "widerstand_physisch";
    pub const RESIST_ENERGY: &str = "widerstand_energie";
    pub const RESIST_DISTORTION: &str = "widerstand_distortion";
    pub const ABSORB_PHYSICAL: &str = "absorption_physisch";
    pub const ABSORB_ENERGY: &str = "absorption_energie";
    pub const ABSORB_DISTORTION: &str = "absorption_distortion";
    pub const COOLING: &str = "kuehlung";
    pub const OUTPUT: &str = "leistung";
    pub const DRIVE_SPEED: &str = "quantentempo";
    pub const FUEL: &str = "treibstoff";
    pub const SPOOL: &str = "aufladen";
    pub const COOLDOWN: &str = "abkuehlen";
    pub const DAMAGE: &str = "schaden";
    pub const RADIUS: &str = "radius";
    pub const LOCK_TIME: &str = "erfassung";
    pub const LOCK_RANGE: &str = "erfassungsweite";
    pub const TEMP_MIN: &str = "temperatur_min";
    pub const TEMP_MAX: &str = "temperatur_max";
    pub const RADIATION: &str = "strahlung";
}

/// One item of the catalog: its text keys, its kind and its values.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemStats {
    /// The key of the name; may be empty.
    pub name: String,
    /// The key of the description.
    pub text: String,
    /// One of [`kind`].
    pub kind: String,
    /// The values by their name ([`value`]).
    pub values: HashMap<String, f64>,
}

impl ItemStats {
    fn get(&self, name: &str) -> Option<f64> {
        self.values.get(name).copied().filter(|v| v.is_finite())
    }

    /// A value only when it is above zero.
    fn positive(&self, name: &str) -> Option<f64> {
        self.get(name).filter(|v| *v > 0.0)
    }
}

/// The items of the parsed catalog file of the game data; empty for a catalog
/// without values. An item without kind has no values and only keeps others
/// on its description from getting a block.
pub fn from_catalog(data: &Value) -> Vec<ItemStats> {
    let Some(Value::Object(map)) = data.get(CATALOG_KEY) else { return Vec::new() };
    let mut found = Vec::new();
    for (_, entry) in map {
        let field = |name: &str| entry.get(name).and_then(Value::as_str).map(py_strip).unwrap_or("").to_string();
        let text = field("text");
        let kind = field("art");
        if text.is_empty() {
            continue;
        }
        let mut values = HashMap::new();
        if let Some(Value::Object(numbers)) = entry.get("w") {
            for (name, number) in numbers {
                let number = match number {
                    Value::Float(x) => Some(*x),
                    Value::Int(n) => n.parse::<f64>().ok(),
                    _ => None,
                };
                if let Some(number) = number.filter(|n| n.is_finite()) {
                    values.insert(name.clone(), number);
                }
            }
        }
        found.push(ItemStats { name: field("name"), text, kind, values });
    }
    found
}

/// The words of the block in one language.
#[derive(Clone, Debug)]
pub struct StatWords {
    pub heading: &'static str,
    pub group: &'static str,
    pub decimal: &'static str,
    pub per_minute: &'static str,
    pub range: &'static str,
    pub speed: &'static str,
    pub spread: &'static str,
    pub penetration: &'static str,
    pub power: &'static str,
    pub ammo: &'static str,
    pub magazine: &'static str,
    pub durability: &'static str,
    pub shield: &'static str,
    pub regen: &'static str,
    pub restart_hit: &'static str,
    pub restart_down: &'static str,
    pub resistance: &'static str,
    pub absorption: &'static str,
    pub physical: &'static str,
    pub energy: &'static str,
    pub cooling: &'static str,
    pub output: &'static str,
    pub pips: &'static str,
    pub fuel: &'static str,
    pub spool: &'static str,
    pub cooldown: &'static str,
    pub damage: &'static str,
    pub radius: &'static str,
    pub lock: &'static str,
    pub lock_up_to: &'static str,
    pub temperature: &'static str,
    pub temperature_to: &'static str,
    pub radiation: &'static str,
}

pub const GERMAN: StatWords = StatWords {
    heading: "VERSEKIT: WERTE AB WERK",
    group: ".",
    decimal: ",",
    per_minute: "Schuss/min",
    range: "Reichweite",
    speed: "Tempo",
    spread: "Streuung",
    penetration: "Durchschlag",
    power: "Strom",
    ammo: "Munition",
    magazine: "Magazin",
    durability: "Haltbarkeit",
    shield: "Schild",
    regen: "Regeneration",
    restart_hit: "Neustart nach Treffer",
    restart_down: "nach Ausfall",
    resistance: "Widerstand",
    absorption: "Absorption",
    physical: "Phys",
    energy: "Energie",
    cooling: "Kühlleistung",
    output: "Leistung",
    pips: "Pips",
    fuel: "Treibstoff",
    spool: "Aufladen",
    cooldown: "Abkühlen",
    damage: "Schaden",
    radius: "Radius",
    lock: "Zielerfassung",
    lock_up_to: "bis",
    temperature: "Temperatur",
    temperature_to: "bis",
    radiation: "Strahlung",
};

pub const ENGLISH: StatWords = StatWords {
    heading: "VERSEKIT: STOCK STATS",
    group: ",",
    decimal: ".",
    per_minute: "rpm",
    range: "Range",
    speed: "Speed",
    spread: "Spread",
    penetration: "Penetration",
    power: "Power",
    ammo: "Ammo",
    magazine: "Magazine",
    durability: "Durability",
    shield: "Shield",
    regen: "Regen",
    restart_hit: "Restart after hit",
    restart_down: "after down",
    resistance: "Resistance",
    absorption: "Absorption",
    physical: "Phys",
    energy: "Energy",
    cooling: "Cooling",
    output: "Output",
    pips: "pips",
    fuel: "Fuel",
    spool: "Spool",
    cooldown: "Cooldown",
    damage: "Damage",
    radius: "Radius",
    lock: "Lock",
    lock_up_to: "up to",
    temperature: "Temperature",
    temperature_to: "to",
    radiation: "Radiation",
};

/// The longest line of the block in characters: the rule above it is 57, and
/// a line a little longer still reads as one line under it. The width of the
/// description box in the game itself is not known; the game breaks a longer
/// line.
pub const MAX_LINE: usize = 60;

/// The headings of the block in both languages, for [`super::form::HEADINGS`].
pub const HEADINGS: [&str; 2] = [GERMAN.heading, ENGLISH.heading];

/// The words for a language folder (`german_(germany)` German, else English).
pub fn words_for(language: &str) -> &'static StatWords {
    if super::texts::lang_code(language) == "de" { &GERMAN } else { &ENGLISH }
}

/// `value` with at most `decimals` places (trailing zeros cut), the digits
/// grouped and the decimal sign of `words`.
pub fn number(value: f64, decimals: usize, words: &StatWords) -> String {
    // Half rounds up, as people round; `format!` alone rounds half to even.
    let scale = 10f64.powi(decimals as i32);
    let text = format!("{:.*}", decimals, (value.abs() * scale).round() / scale);
    let (whole, part) = text.split_once('.').unwrap_or((&text, ""));
    let part = part.trim_end_matches('0');
    let whole: i128 = whole.parse().unwrap_or(0);
    let mut out = super::texts::group_digits(whole, words.group);
    if !part.is_empty() {
        out.push_str(words.decimal);
        out.push_str(part);
    }
    // A value that rounds to zero shows no sign.
    if value < 0.0 && out.chars().any(|c| c.is_ascii_digit() && c != '0') {
        out.insert(0, '-');
    }
    out
}

/// `value` with as many places as its size needs: none from 100, one from 10,
/// else two.
pub fn auto(value: f64, words: &StatWords) -> String {
    let size = value.abs();
    let decimals = if size >= 100.0 {
        0
    } else if size >= 10.0 {
        1
    } else {
        2
    };
    number(value, decimals, words)
}

/// The parts of one line joined with ` · `; `None` parts are left out, an
/// empty line is empty.
fn line(parts: &[Option<String>]) -> String {
    parts.iter().flatten().filter(|p| !p.is_empty()).cloned().collect::<Vec<_>>().join(" · ")
}

/// Shares of physical, energy and distortion damage (0 to 1) in percent behind
/// `label`; empty when the item has none of them.
fn shares_line(item: &ItemStats, w: &StatWords, label: &str, names: [&str; 3]) -> String {
    let part = |kind: &str, name: &str| item.get(name).map(|share| format!("{kind} {} %", number(share * 100.0, 0, w)));
    let parts = line(&[part(w.physical, names[0]), part(w.energy, names[1]), part("Distortion", names[2])]);
    if parts.is_empty() { String::new() } else { format!("{label}: {parts}") }
}

/// The resistances (the share of damage held off).
fn resistance_line(item: &ItemStats, w: &StatWords) -> String {
    shares_line(item, w, w.resistance, [value::RESIST_PHYSICAL, value::RESIST_ENERGY, value::RESIST_DISTORTION])
}

/// The absorption of a shield (the share of damage the shield takes instead of the hull).
fn absorption_line(item: &ItemStats, w: &StatWords) -> String {
    shares_line(item, w, w.absorption, [value::ABSORB_PHYSICAL, value::ABSORB_ENERGY, value::ABSORB_DISTORTION])
}

/// Power, EM and IR of a component, as far as it has them.
fn signature_parts(item: &ItemStats, w: &StatWords) -> Vec<Option<String>> {
    vec![
        item.positive(value::POWER).map(|v| format!("{} {}", w.power, auto(v, w))),
        item.positive(value::EM).map(|v| format!("EM {}", number(v, 0, w))),
        item.positive(value::IR).map(|v| format!("IR {}", number(v, 0, w))),
    ]
}

fn durability(item: &ItemStats, w: &StatWords) -> Option<String> {
    item.positive(value::HEALTH).map(|v| format!("{} {} HP", w.durability, number(v, 0, w)))
}

fn distortion(item: &ItemStats, w: &StatWords) -> Option<String> {
    item.positive(value::DISTORTION_HP).map(|v| format!("Distortion {}", number(v, 0, w)))
}

fn weapon_lines(item: &ItemStats, w: &StatWords, ship: bool) -> Vec<String> {
    let first = line(&[
        item.positive(value::DPS).map(|v| format!("DPS {}", number(v, 0, w))),
        item.positive(value::ALPHA).map(|v| format!("Alpha {}", auto(v, w))),
        item.positive(value::FIRE_RATE).map(|v| format!("{} {}", number(v, 0, w), w.per_minute)),
    ]);
    let second = line(&[
        item.positive(value::RANGE).map(|v| format!("{} {} m", w.range, number(v, 0, w))),
        item.positive(value::SPEED).map(|v| format!("{} {} m/s", w.speed, number(v, 0, w))),
        item.positive(value::PENETRATION).map(|v| format!("{} {} m", w.penetration, auto(v, w))),
    ]);
    let spread = item.get(value::SPREAD).filter(|v| *v >= 0.0).map(|v| format!("{} {}°", w.spread, auto(v, w)));
    if !ship {
        let third = line(&[item.positive(value::AMMO).map(|v| format!("{} {}", w.magazine, number(v, 0, w))), spread]);
        return vec![first, second, third];
    }
    let mut third = vec![spread];
    third.extend(signature_parts(item, w));
    let fourth =
        line(&[item.positive(value::AMMO).map(|v| format!("{} {}", w.ammo, number(v, 0, w))), durability(item, w)]);
    vec![first, second, line(&third), fourth]
}

fn shield_lines(item: &ItemStats, w: &StatWords) -> Vec<String> {
    let first = line(&[
        item.positive(value::SHIELD_HP).map(|v| format!("{} {} HP", w.shield, number(v, 0, w))),
        item.positive(value::REGEN).map(|v| format!("{} {} HP/s", w.regen, auto(v, w))),
    ]);
    let hit = item.get(value::DAMAGED_DELAY).filter(|v| *v >= 0.0);
    let down = item.get(value::DOWNED_DELAY).filter(|v| *v >= 0.0);
    let second = match (hit, down) {
        (Some(hit), Some(down)) => {
            format!("{} {} s · {} {} s", w.restart_hit, auto(hit, w), w.restart_down, auto(down, w))
        }
        (Some(hit), None) => format!("{} {} s", w.restart_hit, auto(hit, w)),
        _ => String::new(),
    };
    // Power, EM and integrity only where resistance or absorption leave room.
    let mut rest = signature_parts(item, w);
    rest.push(durability(item, w));
    vec![first, second, resistance_line(item, w), absorption_line(item, w), line(&rest)]
}

/// The lines of a cooler or power plant: its own value first, then power and
/// signatures, integrity with the distortion it takes before it fails, and the
/// resistances.
fn component_lines(first: String, item: &ItemStats, w: &StatWords) -> Vec<String> {
    vec![
        first,
        line(&signature_parts(item, w)),
        line(&[durability(item, w), distortion(item, w)]),
        resistance_line(item, w),
    ]
}

fn cooler_lines(item: &ItemStats, w: &StatWords) -> Vec<String> {
    let first = line(&[item.positive(value::COOLING).map(|v| format!("{} {}", w.cooling, auto(v, w)))]);
    component_lines(first, item, w)
}

fn power_plant_lines(item: &ItemStats, w: &StatWords) -> Vec<String> {
    let first = line(&[item.positive(value::OUTPUT).map(|v| format!("{} {} {}", w.output, auto(v, w), w.pips))]);
    component_lines(first, item, w)
}

fn quantum_lines(item: &ItemStats, w: &StatWords) -> Vec<String> {
    let first =
        line(&[item.positive(value::DRIVE_SPEED).map(|v| format!("{} {} km/s", w.speed, number(v / 1000.0, 0, w)))]);
    let second = line(&[
        // The fuel per Gm is a small number; two places would make most drives look alike.
        item.positive(value::FUEL).map(|v| format!("{} {}", w.fuel, number(v, 4, w))),
        item.positive(value::SPOOL).map(|v| format!("{} {} s", w.spool, auto(v, w))),
        item.positive(value::COOLDOWN).map(|v| format!("{} {} s", w.cooldown, auto(v, w))),
    ]);
    vec![first, second, line(&signature_parts(item, w)), line(&[durability(item, w), distortion(item, w)])]
}

fn missile_lines(item: &ItemStats, w: &StatWords) -> Vec<String> {
    let first = line(&[
        item.positive(value::DAMAGE).map(|v| format!("{} {}", w.damage, number(v, 0, w))),
        item.positive(value::RADIUS).map(|v| format!("{} {} m", w.radius, auto(v, w))),
    ]);
    let lock = item.positive(value::LOCK_TIME).map(|v| format!("{} s", auto(v, w)));
    let range = item.positive(value::LOCK_RANGE).map(|v| format!("{} {} m", w.lock_up_to, number(v, 0, w)));
    let second = match (lock, range) {
        (None, None) => String::new(),
        (lock, range) => [Some(w.lock.to_string()), lock, range].into_iter().flatten().collect::<Vec<_>>().join(" "),
    };
    let third = line(&[item.positive(value::SPEED).map(|v| format!("{} {} m/s", w.speed, number(v, 0, w)))]);
    vec![first, second, third]
}

fn armor_lines(item: &ItemStats, w: &StatWords) -> Vec<String> {
    let temperature = match (item.get(value::TEMP_MIN), item.get(value::TEMP_MAX)) {
        (Some(low), Some(high)) if low != 0.0 || high != 0.0 => {
            format!("{} {} {} {} °C", w.temperature, number(low, 0, w), w.temperature_to, number(high, 0, w))
        }
        _ => String::new(),
    };
    let radiation =
        line(&[item.positive(value::RADIATION).map(|v| format!("{} {} REM", w.radiation, number(v, 0, w)))]);
    vec![resistance_line(item, w), temperature, radiation]
}

/// The lines of an item, at most four, without empty ones.
pub fn lines(item: &ItemStats, w: &StatWords) -> Vec<String> {
    let all = match item.kind.as_str() {
        kind::SHIP_WEAPON => weapon_lines(item, w, true),
        kind::PERSONAL_WEAPON => weapon_lines(item, w, false),
        kind::SHIELD => shield_lines(item, w),
        kind::COOLER => cooler_lines(item, w),
        kind::POWER_PLANT => power_plant_lines(item, w),
        kind::QUANTUM_DRIVE => quantum_lines(item, w),
        kind::MISSILE => missile_lines(item, w),
        kind::ARMOR => armor_lines(item, w),
        _ => Vec::new(),
    };
    all.into_iter().filter(|l| !l.is_empty()).take(4).collect()
}

/// The block from its lines: a blank line, the rule, a blank line, the heading
/// and the lines; empty without lines.
pub fn block(rows: &[String], w: &StatWords) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let mut parts = vec![String::new(), super::texts::rule(), String::new(), format!("<EM4>{}</EM4>", w.heading)];
    parts.extend(rows.iter().cloned());
    format!("\\n{}", parts.join("\\n"))
}

/// The description key the game shows for an item, in lower case: its
/// description key, or where the catalog names the name key as description
/// the `_Desc` key beside it (see `item_info`).
fn shown_key(item: &ItemStats, file_keys: &HashMap<String, Vec<String>>) -> Option<String> {
    let key = shown_text_key(&item.name, &item.text);
    (!key.is_empty() && file_keys.contains_key(&key)).then_some(key)
}

/// The description key the game shows for an item with the name key `name`
/// and the description key `text`, in lower case: `text`, or where `text` is
/// the name key the `_Desc` key beside it; empty where there is none. The
/// reading of the game data groups the items by it as well.
pub fn shown_text_key(name: &str, text: &str) -> String {
    let text = text.to_lowercase();
    if text != name.to_lowercase() {
        return text;
    }
    text.strip_suffix("_name").map(|stem| format!("{stem}_desc")).unwrap_or_default()
}

/// Description key of the file, in its own spelling → the block, for every
/// description whose items all read the same. A key the file lacks gets
/// nothing.
pub fn build<S: AsRef<str>>(lines_of_file: &[S], items: &[ItemStats], w: &StatWords) -> HashMap<String, String> {
    let mut file_keys: HashMap<String, Vec<String>> = HashMap::new();
    for line in lines_of_file {
        let line = line.as_ref();
        let Some(sep) = line.find('=').filter(|sep| *sep > 0) else { continue };
        let key = line[..sep].split(',').next().unwrap_or("");
        let spellings = file_keys.entry(key.to_lowercase()).or_default();
        if !spellings.iter().any(|k| k == key) {
            spellings.push(key.to_string());
        }
    }
    // Description key → the lines of every item that shows it; `None` once two differ.
    let mut shown: Vec<(String, Option<Vec<String>>)> = Vec::new();
    let mut at: HashMap<String, usize> = HashMap::new();
    for item in items {
        let Some(key) = shown_key(item, &file_keys) else { continue };
        let rows = lines(item, w);
        match at.get(&key) {
            Some(&i) => {
                if shown[i].1.as_ref() != Some(&rows) {
                    shown[i].1 = None;
                }
            }
            None => {
                at.insert(key.clone(), shown.len());
                shown.push((key, Some(rows)));
            }
        }
    }
    let mut result = HashMap::new();
    let mut done: HashSet<String> = HashSet::new();
    for (key, rows) in shown {
        let Some(rows) = rows.filter(|rows| !rows.is_empty()) else { continue };
        if !done.insert(key.clone()) {
            continue;
        }
        let text = block(&rows, w);
        for spelling in &file_keys[&key] {
            result.insert(spelling.clone(), text.clone());
        }
    }
    result
}

/// The block of the values and the blueprint block of the same description as
/// one: the values first, the blueprint part after them below the same rule.
pub fn joined(stats: &str, blueprint: &str) -> String {
    let lead = format!("\\n\\n{}", super::texts::rule());
    match blueprint.strip_prefix(&lead) {
        Some(rest) => format!("{stats}{rest}"),
        None => format!("{stats}{blueprint}"),
    }
}

#[cfg(test)]
mod tests;
