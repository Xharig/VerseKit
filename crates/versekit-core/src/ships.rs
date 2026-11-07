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

//! All ships of the game from the stored data files, as `ships.py` and
//! `erkul.py` read them: the UEX ship list (`schiffe.json`: names, concept)
//! and the slots from erkul.games (`erkul-schiffe.json`). Nothing is
//! fetched here; a missing file or one of another format counts as empty.
//!
//! ⚠ Matching a hangar ship to erkul (`ErkulShips::slot_count`) goes by
//! code, then by name, then word by word (`erkul.ident`). Accents are taken
//! off like Python's NFKD for Latin letters (`FOLD`); other compatibility
//! forms (full-width letters, superscript digits) stay as they are.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use crate::hangar::{ShipNames, get, text, truthy};
use crate::jsonio::{self, Map, Value, py_eq};

pub const UEX_FILE: &str = "schiffe.json";
const UEX_FORMAT: i64 = 5;
pub const ERKUL_FILE: &str = "erkul-schiffe.json";
const ERKUL_FORMAT: i64 = 4;

/// The root object of a stored file when its `format` is `format`.
pub(crate) fn stored(path: &str, format: i64) -> Map {
    match jsonio::read_json(path) {
        Ok(Value::Object(root)) if py_eq(get(&root, "format"), Some(&Value::int(format))) => root,
        _ => Map::new(),
    }
}

// ------------------------------------------------------------ UEX

/// One ship of the UEX list.
#[derive(Clone, Debug, Default)]
pub struct UexShip {
    pub name: String,
    /// `anbau`: a module, not a ship of its own.
    addon: bool,
    /// `konzept`: not in the game yet, as UEX says.
    concept: bool,
}

/// The UEX ship list (`ships.load`).
#[derive(Clone, Debug, Default)]
pub struct UexShips {
    ships: Vec<UexShip>,
    names: ShipNames,
}

impl UexShips {
    pub fn load(path: &str) -> UexShips {
        let root = stored(path, UEX_FORMAT);
        let ships: Vec<UexShip> = match get(&root, "schiffe") {
            Some(Value::Object(items)) => items
                .iter()
                .filter_map(|(_, s)| s.as_object())
                .map(|s| UexShip {
                    name: text(s, "name").to_string(),
                    addon: truthy(get(s, "anbau")),
                    concept: truthy(get(s, "konzept")),
                })
                .collect(),
            _ => Vec::new(),
        };
        let names = ShipNames(ships.iter().map(|s| s.name.clone()).collect());
        UexShips { ships, names }
    }

    /// The names, for reading exports without a maker.
    pub fn names(&self) -> &ShipNames {
        &self.names
    }

    /// `ships.all_names`: every ship and vehicle, also without cargo, by
    /// name without case.
    pub fn all_names(&self) -> Vec<String> {
        let mut names: Vec<String> =
            self.ships.iter().filter(|s| !s.name.is_empty() && !s.addon).map(|s| s.name.clone()).collect();
        names.sort_by_key(|n| n.to_lowercase());
        names
    }

    /// The names for a field to pick a ship from: `all_names` with each name once, without regard to case or
    /// outer spaces. UEX lists a few ships twice under one name (the Sabre Raven EX under two ids); a list to
    /// choose from shows such a name only once.
    pub fn choice_names(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        self.all_names().into_iter().filter(|n| seen.insert(n.trim().to_lowercase())).collect()
    }

    fn find(&self, name: &str) -> Option<&UexShip> {
        self.names.position(name).map(|i| &self.ships[i])
    }

    /// `ships.knows`: does UEX list a ship of this name?
    pub fn knows(&self, name: &str) -> bool {
        self.find(name).is_some()
    }

    /// The name as UEX writes it, for a name `knows` accepts.
    pub fn name_of(&self, name: &str) -> Option<&str> {
        self.find(name).map(|s| s.name.as_str())
    }

    /// `ships.manufacturer`: the part of the UEX name before the name asked for (`Mirai` for `Fury LX`,
    /// `Aegis Dynamics` for `Sabre`), or `""`. UEX writes the maker into the name, the player does not.
    pub fn manufacturer(&self, name: &str) -> String {
        let Some(full) = self.name_of(name) else { return String::new() };
        let (full, wanted) = (crate::text::py_strip(full), crate::text::py_strip(name));
        if !full.to_lowercase().ends_with(&format!(" {}", wanted.to_lowercase())) {
            return String::new();
        }
        let keep = full.chars().count().saturating_sub(wanted.chars().count());
        crate::text::py_strip(&full.chars().take(keep).collect::<String>()).to_string()
    }

    /// `ships.is_concept`: a concept by UEX, not in the game yet.
    pub fn is_concept(&self, name: &str) -> bool {
        self.find(name).is_some_and(|s| s.concept)
    }
}

// ------------------------------------------------------------ erkul

/// Latin letters with marks → the letters NFKD leaves after the marks are
/// dropped; per group the replacement first, then the letters it replaces.
const FOLD: &str = "AÀÁÂÃÄÅĀĂĄǍǞǠǺȀȂȦ CÇĆĈĊČ DĎ DZǄǱ Dzǅǲ EÈÉÊËĒĔĖĘĚȄȆȨ GĜĞĠĢǦǴ HĤȞ IÌÍÎÏĨĪĬĮİǏȈȊ IJĲ JĴ KĶǨ \
                    LĹĻĽ LJǇ Ljǈ NÑŃŅŇǸ NJǊ Njǋ OÒÓÔÕÖŌŎŐƠǑǪǬȌȎȪȬȮȰ RŔŖŘȐȒ SŚŜŞŠȘ TŢŤȚ UÙÚÛÜŨŪŬŮŰŲƯǓǕǗǙǛȔȖ WŴ YÝŶŸȲ \
                    ZŹŻŽ aàáâãäåāăąǎǟǡǻȁȃȧ cçćĉċč dď dzǆǳ eèéêëēĕėęěȅȇȩ gĝğġģǧǵ hĥȟ iìíîïĩīĭįǐȉȋ ijĳ jĵǰ kķǩ \
                    lĺļľ ljǉ nñńņňǹ njǌ oòóôõöōŏőơǒǫǭȍȏȫȭȯȱ rŕŗřȑȓ sśŝşšſș tţťț uùúûüũūŭůűųưǔǖǘǚǜȕȗ wŵ yýÿŷȳ zźżž";

fn fold_table() -> &'static HashMap<char, &'static str> {
    static TABLE: OnceLock<HashMap<char, &'static str>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = HashMap::new();
        for group in FOLD.split_whitespace() {
            let cut = group.find(|c: char| !c.is_ascii()).unwrap_or(group.len());
            let (to, from) = group.split_at(cut);
            table.extend(from.chars().map(|c| (c, to)));
        }
        table
    })
}

/// `unicodedata.normalize('NFKD', …)` without the combining marks, then
/// lower case.
fn without_accents(text_: &str) -> String {
    if text_.is_ascii() {
        return text_.to_ascii_lowercase();
    }
    let table = fold_table();
    let mut out = String::new();
    for c in text_.chars() {
        if ('\u{300}'..='\u{36f}').contains(&c) {
            continue;
        }
        match table.get(&c) {
            Some(to) => out.push_str(to),
            None => out.push(c),
        }
    }
    out.to_lowercase()
}

/// `erkul._slim`: letters and digits only, accents taken off.
fn erkul_slim(text_: &str) -> String {
    without_accents(text_).chars().filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit()).collect()
}

const ROMAN: [(&str, &str); 5] = [("i", "1"), ("ii", "2"), ("iii", "3"), ("iv", "4"), ("v", "5")];

fn roman(word: &str) -> Option<&'static str> {
    ROMAN.iter().find(|(r, _)| *r == word).map(|(_, n)| *n)
}

/// `erkul._words`: the words of a name, `Mk II` as `mk2`.
fn words(text_: &str) -> Vec<String> {
    let t = without_accents(text_).replace(['.', '-'], "");
    let mut out: Vec<String> = Vec::new();
    for word in t.split(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit())).filter(|w| !w.is_empty()) {
        if let Some(number) = word.strip_prefix("mk")
            && !number.is_empty()
            && (number.chars().all(|c| "ivx".contains(c)) || number.chars().all(|c| c.is_ascii_digit()))
        {
            out.push(format!("mk{}", roman(number).unwrap_or(number)));
            continue;
        }
        if let Some(n) = roman(word)
            && out.last().is_some_and(|w| w == "mk")
        {
            if let Some(last) = out.last_mut() {
                *last = format!("mk{n}");
            }
            continue;
        }
        out.push(word.to_string());
    }
    out
}

/// `erkul._is_abbrev`: are the letters of `short` in `long`, in order, with
/// the same first letter? Only for short codes.
fn is_abbrev(short: &str, long: &str) -> bool {
    if short.is_empty() || long.is_empty() || short.len() > 5 || short.len() >= long.len() {
        return false;
    }
    if short.as_bytes()[0] != long.as_bytes()[0] {
        return false;
    }
    let short = short.as_bytes();
    let mut at = 0;
    for &c in long.as_bytes() {
        if at < short.len() && c == short[at] {
            at += 1;
        }
    }
    at == short.len()
}

/// `erkul._matches_wordwise`: does `wanted` cover every word of this erkul
/// code? The score, `0` when not.
fn matches_wordwise(own: &[String], wanted: &HashSet<String>, chain: &str) -> i64 {
    let mut without_maker = false;
    let mut score = 0;
    for (at, word) in own.iter().enumerate() {
        if wanted.contains(word) {
            score += 3;
        } else if wanted.iter().any(|g| g.starts_with(word.as_str())) {
            score += 2;
        } else if wanted.iter().any(|g| is_abbrev(word, g)) || (!chain.is_empty() && chain.contains(word.as_str())) {
            // A code without a vowel inside (`aegs`), or words erkul writes together (`alphawolf`).
            score += 1;
        } else if at == 0 && own.len() > 1 && word.len() <= 5 {
            without_maker = true;
        } else {
            return 0;
        }
    }
    if without_maker && score < 3 {
        return 0;
    }
    score
}

/// `erkul.MANUAL_MAP`: names no rule finds.
const MANUAL_MAP: [(&str, &str); 6] = [
    ("aegishammerhead", "aegs_hammerhead_gs"),
    ("aegishammerheadbestinshowedition", "aegs_hammerhead_gs"),
    ("aegisidrisp", "aegs_idris_p"),
    ("aegisidrism", "aegs_idris_m"),
    ("aopoasantokyai", "xnaa_santokyai"),
    ("rsiursa", "rsi_ursa_rover"),
];

/// One original erkul code for the word-by-word search: the code, the
/// stored entry it stands for, and its words.
#[derive(Clone, Debug)]
struct Code {
    code: String,
    index: usize,
    words: Vec<String>,
}

/// The slots from erkul.games as stored (`erkul.load`).
#[derive(Clone, Debug, Default)]
pub struct ErkulShips {
    version: String,
    /// Slim key → stored entry.
    ships: Vec<(String, Map)>,
    /// Slim key → place in `ships`.
    keys: HashMap<String, usize>,
    /// The original codes (`id`), as a dict of them keeps them.
    codes: Vec<Code>,
    /// Slim maker name → erkul's code.
    makers: Vec<(String, String)>,
}

/// Which hangar ship: name, maker, ship code and maker code.
#[derive(Clone, Copy, Debug)]
pub struct ShipId<'a> {
    pub name: &'a str,
    pub maker: &'a str,
    pub short: &'a str,
    pub maker_short: &'a str,
}

impl ErkulShips {
    pub fn load(path: &str) -> ErkulShips {
        let root = stored(path, ERKUL_FORMAT);
        let ships = match get(&root, "schiffe") {
            Some(Value::Object(items)) => {
                items.iter().filter_map(|(k, v)| v.as_object().map(|m| (k.clone(), m.clone()))).collect()
            }
            _ => Vec::new(),
        };
        let makers = match get(&root, "hersteller") {
            Some(Value::Object(items)) => {
                items.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect()
            }
            _ => Vec::new(),
        };
        let version = text(&root, "spielversion").to_string();
        ErkulShips::from_parts(version, ships, makers)
    }

    /// The slots of the game data, with erkul's where the game data have nothing: a ship the game data lack comes
    /// from erkul whole; a slot of the game data without what fits (`passt`) or without a size takes them from
    /// erkul's slot of the same path, where erkul knows the ship. The ports right on a ship are such slots:
    /// p4kx gives them no kind and size yet.
    pub fn merged(game: ErkulShips, erkul: ErkulShips) -> ErkulShips {
        if game.is_empty() {
            return erkul;
        }
        let ErkulShips { version, ships: mut own, makers: mut own_makers, .. } = game;
        for (key, entry) in &mut own {
            let Some(&at) = erkul.keys.get(key) else { continue };
            let theirs: Vec<&Map> = match get(&erkul.ships[at].1, "slots") {
                Some(Value::Array(items)) => items.iter().filter_map(Value::as_object).collect(),
                _ => Vec::new(),
            };
            let Some((_, Value::Array(slots))) = entry.iter_mut().find(|(k, _)| k == "slots") else { continue };
            for slot in slots.iter_mut() {
                let Value::Object(fields) = slot else { continue };
                let path = text(fields, "pfad").to_lowercase();
                let Some(other) = theirs.iter().find(|s| text(s, "pfad").to_lowercase() == path) else { continue };
                for field in ["passt", "groesse"] {
                    let missing = !matches!(get(fields, field), Some(v) if !matches!(v, Value::Null));
                    if missing && let Some(value) = get(other, field).filter(|v| !matches!(v, Value::Null)) {
                        jsonio::map_set(fields, field, value.clone());
                    }
                }
            }
        }
        let known: std::collections::HashSet<String> = own.iter().map(|(k, _)| k.clone()).collect();
        own.extend(erkul.ships.into_iter().filter(|(k, _)| !known.contains(k)));
        for (name, code) in erkul.makers {
            if !own_makers.iter().any(|(k, _)| *k == name) {
                own_makers.push((name, code));
            }
        }
        let version = if version.is_empty() { erkul.version } else { version };
        ErkulShips::from_parts(version, own, own_makers)
    }

    fn from_parts(version: String, ships: Vec<(String, Map)>, makers: Vec<(String, String)>) -> ErkulShips {
        let mut keys = HashMap::new();
        for (i, (key, _)) in ships.iter().enumerate() {
            keys.entry(key.clone()).or_insert(i);
        }
        // `dict((id or key, key) …)`: a code seen again keeps its place, takes the later entry.
        let mut codes: Vec<Code> = Vec::new();
        let mut place: HashMap<String, usize> = HashMap::new();
        for (i, (key, entry)) in ships.iter().enumerate() {
            let code = if truthy(get(entry, "id")) { text(entry, "id").to_string() } else { key.clone() };
            match place.get(&code) {
                Some(&at) => codes[at].index = i,
                None => {
                    place.insert(code.clone(), codes.len());
                    codes.push(Code { words: words(&code), code, index: i });
                }
            }
        }
        ErkulShips { version, ships, keys, codes, makers }
    }

    /// The stored entry of a ship (`erkul.load()['schiffe'][ident]`), `None` when erkul does not know it.
    pub fn entry(&self, id: ShipId) -> Option<&Map> {
        Some(&self.ships[self.ident(id)?].1)
    }

    /// `erkul.knows`: is there slot data for this ship at all?
    pub fn knows(&self, id: ShipId) -> bool {
        self.ident(id).is_some()
    }

    /// `erkul.game_version`: the game version of the stored slots, or `""`.
    pub fn game_version(&self) -> &str {
        &self.version
    }

    pub fn is_empty(&self) -> bool {
        self.ships.is_empty()
    }

    /// `erkul.candidates`: every spelling erkul could list the ship under.
    fn candidates(id: ShipId, maker_short: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for attempt in [
            id.short.to_string(),
            format!("{maker_short} {}", id.name),
            id.name.to_string(),
            format!("{} {}", id.maker, id.name),
        ] {
            let slimmed = erkul_slim(&attempt);
            if !slimmed.is_empty() && !out.contains(&slimmed) {
                out.push(slimmed);
            }
        }
        out
    }

    /// `erkul.ident`: the stored key of a ship, or `None`.
    fn ident(&self, id: ShipId) -> Option<usize> {
        let maker_short = if id.maker_short.is_empty() {
            let maker = erkul_slim(id.maker);
            self.makers.iter().find(|(k, _)| *k == maker).map_or("", |(_, v)| v.as_str())
        } else {
            id.maker_short
        };
        for slimmed in Self::candidates(id, maker_short) {
            if let Some(&i) = self.keys.get(&slimmed) {
                return Some(i);
            }
        }
        // Word by word against the original codes (`id`).
        search_wordwise(&self.codes, id.name, id.maker, maker_short).map(|c| c.index)
    }

    /// `erkul.slot_counts` summed: the number of slots, `None` when erkul
    /// does not know the ship.
    pub fn slot_count(&self, id: ShipId) -> Option<i64> {
        let entry = &self.ships[self.ident(id)?].1;
        match get(entry, "plaetze") {
            Some(Value::Array(slots)) if !slots.is_empty() => {
                Some(slots.iter().filter_map(Value::as_object).map(|p| crate::hangar::field_int(p, "anzahl")).sum())
            }
            _ => None,
        }
    }
}

/// The code of erkul's index (`blobs[].id`) that names this ship, word by word as `erkul._search_wordwise`
/// finds it for the salvage page; `None` where none fits or two fit equally.
pub fn erkul_code(codes: &[String], name: &str, maker: &str) -> Option<String> {
    let listing: Vec<Code> =
        codes.iter().enumerate().map(|(index, code)| Code { words: words(code), code: code.clone(), index }).collect();
    search_wordwise(&listing, name, maker, "").map(|c| c.code.clone())
}

/// `erkul._slim` for the pages: the comparable key of a ship name.
pub fn slim_name(name: &str) -> String {
    erkul_slim(name)
}

/// `erkul.candidates`: every slim spelling erkul could list the ship under, the best first.
pub fn erkul_candidates(id: ShipId) -> Vec<String> {
    ErkulShips::candidates(id, id.maker_short)
}

/// `erkul._search_wordwise` over the original codes of erkul's index: the one that names this ship, `None`
/// where none fits or two fit equally.
pub fn erkul_code_wordwise(codes: &[String], name: &str, maker: &str, maker_short: &str) -> Option<String> {
    let listing: Vec<Code> =
        codes.iter().enumerate().map(|(index, code)| Code { words: words(code), code: code.clone(), index }).collect();
    search_wordwise(&listing, name, maker, maker_short).map(|c| c.code.clone())
}

/// The format of the stored slots (`erkul.FORMAT`).
pub const ERKUL_FILE_FORMAT: i64 = ERKUL_FORMAT;

/// `erkul._search_wordwise`: the best word-by-word match, `None` on a tie.
fn search_wordwise<'a>(listing: &'a [Code], name: &str, maker: &str, maker_short: &str) -> Option<&'a Code> {
    let slim_name = erkul_slim(name);
    if let Some((_, manual)) = MANUAL_MAP.iter().find(|(k, _)| *k == slim_name)
        && let Some(hit) = listing.iter().find(|c| c.code == *manual)
    {
        return Some(hit);
    }
    let joined: Vec<&str> = [maker_short, maker, name].into_iter().filter(|x| !x.is_empty()).collect();
    let seq = words(&joined.join(" "));
    let wanted: HashSet<String> = seq.iter().cloned().collect();
    if wanted.is_empty() {
        return None;
    }
    let chain = seq.concat();
    let scored: Vec<(i64, &Code)> = listing.iter().map(|c| (matches_wordwise(&c.words, &wanted, &chain), c)).collect();
    let top = scored.iter().map(|(s, _)| *s).max().filter(|s| *s > 0)?;
    let mut best = scored.iter().filter(|(s, _)| *s == top);
    match (best.next(), best.next()) {
        (Some((_, code)), None) => Some(code),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_and_accents() {
        assert_eq!(words("F7C-M Super Hornet Mk II"), vec!["f7cm", "super", "hornet", "mk2"]);
        assert_eq!(words("anvl_hornet_f7cm_mk2"), vec!["anvl", "hornet", "f7cm", "mk2"]);
        assert_eq!(erkul_slim("San tok.Yāi"), "santokyai");
        assert!(is_abbrev("aegs", "aegis"));
        assert!(!is_abbrev("pir", "xpirate"));
    }

    fn erkul(keys: &[(&str, &str, i64)]) -> ErkulShips {
        let ships = keys
            .iter()
            .map(|(key, id, n)| {
                let slot = vec![("anzahl".to_string(), Value::int(*n))];
                let entry = vec![
                    ("id".to_string(), Value::str(*id)),
                    ("plaetze".to_string(), Value::Array(vec![Value::Object(slot)])),
                ];
                (key.to_string(), entry)
            })
            .collect();
        ErkulShips::from_parts("4.10".into(), ships, vec![("anvilaerospace".into(), "anvl".into())])
    }

    #[test]
    fn slots_by_code_name_and_words() {
        let e = erkul(&[
            ("anvlarrow", "anvl_arrow", 7),
            ("drakironcladassault", "drak_ironclad_assault", 9),
            ("anvlhornetf7cmmk2", "anvl_hornet_f7cm_mk2", 11),
            ("anvlhornetf7cmk2", "anvl_hornet_f7c_mk2", 10),
        ]);
        let id = |name, maker, short, maker_short| ShipId { name, maker, short, maker_short };
        assert_eq!(e.slot_count(id("Arrow", "Anvil Aerospace", "", "")), Some(7));
        assert_eq!(e.slot_count(id("Ironclad Assault", "Drake Interplanetary", "", "")), Some(9));
        assert_eq!(e.slot_count(id("F7C-M Super Hornet Mk II", "", "ANVL_F7C_M_Super_Hornet_Mk_I", "ANVL")), Some(11));
        assert_eq!(e.slot_count(id("Galaxy", "RSI", "", "")), None);
    }

    #[test]
    fn uex_names_concept_and_addons() {
        let ships = UexShips {
            ships: vec![
                UexShip { name: "RSI Galaxy".into(), addon: false, concept: true },
                UexShip { name: "RSI Galaxy Cargo Module".into(), addon: true, concept: false },
                UexShip { name: "aegis Gladius".into(), addon: false, concept: false },
            ],
            names: ShipNames(vec!["RSI Galaxy".into(), "RSI Galaxy Cargo Module".into(), "aegis Gladius".into()]),
        };
        assert!(ships.is_concept("Galaxy"));
        assert!(ships.knows("gladius"));
        assert_eq!(ships.all_names(), vec!["aegis Gladius".to_string(), "RSI Galaxy".to_string()]);
    }

    #[test]
    fn the_slots_of_the_game_data_come_first_and_erkul_fills_only_what_they_lack() {
        let ships = |text: &str| -> Vec<(String, Map)> {
            match jsonio::loads(text) {
                Ok(Value::Object(root)) => {
                    root.into_iter().filter_map(|(k, v)| v.as_object().cloned().map(|m| (k, m))).collect()
                }
                _ => panic!("ships"),
            }
        };
        let game = ErkulShips::from_parts(
            "game".into(),
            ships(
                r#"{"anvlarrow": {"name": "Anvil Arrow", "slots": [
                    {"pfad": "hardpoint_power_plant", "art": "PowerPlant", "groesse": null, "werk": {"ref": "pp", "name": "PP"}},
                    {"pfad": "hardpoint_weapon_left/hardpoint_class_2", "art": "WeaponGun", "groesse": 3,
                     "passt": {"typen": [["WeaponGun", []]], "min": 3, "max": 3, "porttags": [], "verlangt": []}}]}}"#,
            ),
            vec![("anvilaerospace".into(), "anvl".into())],
        );
        let erkul = ErkulShips::from_parts(
            "erkul".into(),
            ships(
                r#"{"anvlarrow": {"name": "Anvil Arrow", "slots": [
                    {"pfad": "HARDPOINT_POWER_PLANT", "art": "PowerPlant", "groesse": 1,
                     "passt": {"typen": [["PowerPlant", []]], "min": 1, "max": 1, "porttags": [], "verlangt": []}},
                    {"pfad": "hardpoint_weapon_left/hardpoint_class_2", "art": "WeaponGun", "groesse": 2,
                     "passt": {"typen": [["WeaponGun", []]], "min": 2, "max": 2, "porttags": [], "verlangt": []}},
                    {"pfad": "hardpoint_only_erkul", "art": "Cooler", "groesse": 1}]},
                   "aegsgladius": {"name": "Aegis Gladius", "slots": [{"pfad": "hardpoint_cooler", "art": "Cooler", "groesse": 1}]}}"#,
            ),
            vec![("aegisdynamics".into(), "aegs".into())],
        );
        let merged = ErkulShips::merged(game, erkul);
        assert_eq!(merged.game_version(), "game");
        let id = |name, short| ShipId { name, maker: "", short, maker_short: "" };
        let arrow = merged.entry(id("Arrow", "ANVL_Arrow")).expect("the game data's Arrow");
        let slots = match get(arrow, "slots") {
            Some(Value::Array(items)) => items.iter().filter_map(Value::as_object).cloned().collect::<Vec<_>>(),
            _ => panic!("slots"),
        };
        assert_eq!(slots.len(), 2, "a slot erkul alone has does not come in: the game data hold");
        // The top port without kind and size takes erkul's of the same path (case does not matter).
        assert_eq!(get(&slots[0], "groesse"), Some(&Value::int(1)));
        assert!(get(&slots[0], "passt").is_some());
        // What the game data say stays theirs.
        assert_eq!(get(&slots[1], "groesse"), Some(&Value::int(3)));
        // A ship only erkul knows comes from erkul, and erkul's makers come along.
        assert!(merged.knows(id("Gladius", "AEGS_Gladius")));
        assert!(merged.knows(ShipId { name: "Gladius", maker: "Aegis Dynamics", short: "", maker_short: "" }));
        // Counter-check: without game data it is erkul alone.
        let erkul_only = ErkulShips::merged(ErkulShips::default(), ErkulShips::from_parts("e".into(), ships(r#"{"x": {"slots": []}}"#), vec![]));
        assert_eq!(erkul_only.game_version(), "e");
    }

    #[test]
    fn a_ship_that_uex_lists_twice_under_one_name_is_offered_once() {
        let named = |names: &[&str]| UexShips {
            ships: names.iter().map(|n| UexShip { name: n.to_string(), addon: false, concept: false }).collect(),
            names: ShipNames(names.iter().map(|n| n.to_string()).collect()),
        };
        let ships = named(&["Aegis Sabre Raven EX", "Aegis Sabre", "aegis sabre raven ex ", "Aegis Sabre Raven EX"]);
        // The list as UEX gives it keeps every entry: the comparison with Python reads it like that.
        assert_eq!(ships.all_names().len(), 4);
        assert_eq!(ships.choice_names(), vec!["Aegis Sabre".to_string(), "Aegis Sabre Raven EX".to_string()]);
        // Names that only start alike stay apart.
        let ships = named(&["Aegis Sabre", "Aegis Sabre Comet", "Aegis Sabre Firebird"]);
        assert_eq!(ships.choice_names().len(), 3);
    }
}
