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

//! Blueprint notes under the description of a craftable item: port of
//! `item_info.py`.
//!
//! Below a rule and the heading `VERSE-KIT: BAUPLAN & HERSTELLUNG` stand
//! three lines: the blueprint with its box (had or missing, from which
//! contract, from which rank, how many more), crafting (`3 Zutaten · 4 min`)
//! and the cheapest shop (`Im Laden: New Babbage · 12.400 aUEC`).
//!
//! The block always holds a box: that is how an earlier block is told as
//! ours. Items whose ownership is unclear (one name, several items) get no
//! block. The craftable items come as [`Item`]s, from 4.12's
//! `crafting-blueprints.json` ([`items_from_412`]) or from the game data.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;

use crate::jsonio::{Map, Value};
use crate::paths::{Paths, name_key};
use crate::text::{PY_SPACE, py_strip};

use super::texts::{Words, py_int, py_str, truthy};

/// The setting „Bauplan-Angaben in den Beschreibungen“.
pub const SETTING: &str = "angaben_in_beschreibung";
const TAG_PREFIX: &str = "bp_craft_";

static PLACEHOLDER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"~\w+\([^)]*\)").expect("pattern"));
static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!("{PY_SPACE}{{2,}}")).expect("pattern"));

/// `item_info.enabled`.
pub fn enabled(paths: &Paths) -> bool {
    paths.setting_bool(SETTING, true)
}

/// One craftable item as `crafting.with_collection` gives it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Item {
    /// The product name the collection is compared with (`basis`).
    pub base: String,
    pub tag: String,
    pub tags: Vec<String>,
    /// The entity class (`entity`), the key to the shop prices.
    pub entity: String,
    /// Blueprint at hand: yes, no, or unclear (`habe`).
    pub have: Option<bool>,
    /// Slots of the first tier of the recipe of `tag`, and its crafting
    /// time in seconds; `None` without a recipe.
    pub first_tier: Option<(usize, Value)>,
}

/// The text keys the game shows for one crafted item, from the catalog of the
/// game data.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemKeys {
    /// The key of the name (`item_Name…`); empty where the catalog names none.
    pub name: String,
    /// The key of the description, which several items can share.
    pub text: String,
}

/// Record GUID of the item in lower case (the `entity` of an [`Item`]) → its
/// text keys.
pub type ItemTexts = HashMap<String, ItemKeys>;

/// The text keys of the parsed catalog file of the game data
/// (`catalog_cache::ITEM_TEXTS`); empty for a catalog without them.
pub fn item_texts_from_catalog(data: &Value) -> ItemTexts {
    let mut found = ItemTexts::new();
    if let Some(Value::Object(map)) = data.get(crate::catalog_cache::ITEM_TEXTS) {
        for (guid, keys) in map {
            let field = |name: &str| keys.get(name).and_then(Value::as_str).map(py_strip).unwrap_or("").to_string();
            let text = field("text");
            if !text.is_empty() {
                found.insert(guid.to_lowercase(), ItemKeys { name: field("name"), text });
            }
        }
    }
    found
}

/// The cheapest shop per entity and per name (`shops.cheapest_by_entity`).
#[derive(Clone, Debug, Default)]
pub struct Shops {
    pub by_entity: HashMap<String, (Value, Value)>,
    pub by_name: HashMap<String, (Value, Value)>,
    pub ids: HashSet<String>,
    pub names: HashSet<String>,
}

fn pair(value: &Value) -> Option<(Value, Value)> {
    match value {
        Value::Array(list) if list.len() == 2 => Some((list[0].clone(), list[1].clone())),
        _ => None,
    }
}

/// `shops.cheapest_by_entity` from the parsed `laeden-katalog.json`.
pub fn shops_from_catalog(data: &Value) -> Shops {
    let mut shops = Shops::default();
    if let Some(Value::Array(parts)) = data.get("teile") {
        for part in parts {
            if let (Some(id), Some(hit)) = (part.get("s"), part.get("p").and_then(pair))
                && truthy(Some(id))
            {
                shops.by_entity.insert(py_str(id), hit);
            }
        }
    }
    if let Some(Value::Object(prices)) = data.get("preise_namen") {
        for (name, price) in prices {
            if let Some(hit) = pair(price) {
                shops.by_name.insert(name.clone(), hit);
            }
        }
    }
    let strings = |value: Option<&Value>| -> HashSet<String> {
        match value {
            Some(Value::Array(list)) => list.iter().map(py_str).collect(),
            Some(Value::Object(map)) => map.iter().map(|(k, _)| k.clone()).collect(),
            _ => HashSet::new(),
        }
    };
    shops.ids = strings(data.get("kennungen"));
    shops.names = strings(data.get("namen"));
    shops
}

fn loose(stem: &str) -> String {
    super::specs::loose_stem(stem).to_string()
}

/// Loose stem → description keys in their own spelling.
type Descriptions = HashMap<String, Vec<String>>;

/// `item_info.desc_keys`: loose stem → description keys, name key → stem.
fn desc_keys<S: AsRef<str>>(lines: &[S]) -> (Descriptions, Vec<(String, String)>) {
    let mut descs: HashMap<String, Vec<String>> = HashMap::new();
    let mut names: Vec<(String, String)> = Vec::new();
    for line in lines {
        let line = line.as_ref();
        let Some(sep) = line.find('=') else { continue };
        if sep < 1 {
            continue;
        }
        let key = line[..sep].split(',').next().unwrap_or("");
        let lower = key.to_lowercase();
        if let Some(stem) = lower.strip_prefix("item_desc") {
            descs.entry(loose(stem)).or_default().push(key.to_string());
        } else if let Some(stem) = lower.strip_prefix("item_name") {
            match names.iter_mut().find(|(k, _)| k == key) {
                Some(slot) => slot.1 = loose(stem),
                None => names.push((key.to_string(), loose(stem))),
            }
        }
    }
    (descs, names)
}

/// `item_info._tag_stems`.
fn tag_stems(tag: &str) -> Vec<String> {
    let lower = tag.to_lowercase();
    let Some(rest) = lower.strip_prefix(TAG_PREFIX) else { return Vec::new() };
    let stem = loose(rest);
    let mut out = vec![stem.clone()];
    if let Some(short) = stem.strip_suffix("_temp") {
        out.push(short.to_string());
    }
    out
}

/// `item_info._name_index`: English name in comparison form → stem, only
/// unique ones.
fn name_index(names: &[(String, String)], original: &[(String, String)]) -> HashMap<String, String> {
    let mut found: HashMap<String, String> = HashMap::new();
    let mut double: HashSet<String> = HashSet::new();
    for (key, stem) in names {
        let Some(name) = original.iter().rev().find(|(k, _)| k == key).map(|(_, n)| n).filter(|n| !n.is_empty()) else {
            continue;
        };
        let k = name_key(name);
        if found.get(&k).is_some_and(|known| known != stem) {
            double.insert(k.clone());
        }
        found.insert(k, stem.clone());
    }
    for k in double {
        found.remove(&k);
    }
    found
}

/// `item_info._time_text`.
fn time_text(seconds: &Value) -> String {
    let seconds = py_int(Some(seconds)).unwrap_or(0);
    if seconds <= 0 {
        return String::new();
    }
    if seconds < 60 {
        return format!("{seconds} s");
    }
    let minutes = (seconds + 30).div_euclid(60);
    if minutes < 60 { format!("{minutes} min") } else { format!("{} h {} min", minutes / 60, minutes % 60) }
}

/// `item_info._clean_title`.
fn clean_title(text: &str) -> String {
    let text = PLACEHOLDER.replace_all(text, "");
    let text = SPACES.replace_all(&text, " ");
    text.trim_matches([' ', '-', ':', ',']).to_string()
}

/// One way to a blueprint: reputation (huge when unknown), minus reward,
/// title key, rank.
type Route = (f64, f64, String, String);

/// `item_info.origins`: name in comparison form → ways, easiest first.
fn origins(missions: &Map) -> HashMap<String, Vec<Route>> {
    let mut result: HashMap<String, Vec<Route>> = HashMap::new();
    for (_, entry) in missions {
        let Some(key) = entry.get("titel_key").filter(|v| truthy(Some(v))).map(py_str) else { continue };
        let per_bp = entry.get("ab").and_then(Value::as_object);
        let Some(Value::Array(blueprints)) = entry.get("bp") else { continue };
        for name in blueprints.iter().map(py_str) {
            let cond = per_bp
                .and_then(|m| m.iter().find(|(k, _)| *k == name))
                .map(|(_, v)| v)
                .filter(|c| c.as_object().is_some());
            // `cond.get('rep', entry.get('rep'))`: a key that is there wins, even as null.
            let rep = cond.and_then(|c| c.get("rep")).or_else(|| entry.get("rep"));
            let rank = cond.and_then(|c| c.get("rang")).or_else(|| entry.get("rang"));
            let rep = match rep {
                Some(Value::Int(n)) => n.parse::<f64>().unwrap_or(1e9),
                Some(Value::Float(x)) => *x,
                _ => 1e9,
            };
            let reward = match entry.get("uec") {
                Some(v) if truthy(Some(v)) => match v {
                    Value::Float(x) => *x,
                    other => py_int(Some(other)).unwrap_or(0) as f64,
                },
                _ => 0.0,
            };
            let rank = rank.filter(|r| truthy(Some(r))).map(py_str).unwrap_or_default();
            result.entry(name_key(&name)).or_default().push((rep, -reward, key.clone(), rank));
        }
    }
    for routes in result.values_mut() {
        routes.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    }
    result
}

/// What the item block needs from the injection.
pub struct ItemContext<'a> {
    pub words: &'a Words,
    /// The contract title in the language of the file, without our mark.
    pub title_of: &'a dyn Fn(&str) -> String,
    pub digits: &'a dyn Fn(&Value) -> String,
}

/// `item_info._blueprint_line`; with `name` the item is named before its
/// state (`Bauplan: [x] ADP-mk4 Arms Woodland (vorhanden)`), for a
/// description several items share.
fn blueprint_line(have: bool, routes: Option<&Vec<Route>>, name: Option<&str>, ctx: &ItemContext<'_>) -> String {
    let w = ctx.words;
    let state = |word: &str| match name {
        Some(name) => format!("{name} ({word})"),
        None => word.to_string(),
    };
    if have {
        return format!("{}: {} {}", w.it_blueprint, super::form::BOX_HAVE, state(w.it_have));
    }
    let line = format!("{}: {} {}", w.it_blueprint, super::form::BOX_MISSING, state(w.it_missing));
    line + &route_part(routes, ctx)
}

/// The tail of a missing blueprint line: from which contract and rank for
/// the easiest of `routes` and how many other contracts give it, or that no
/// contract gives it.
fn route_part(routes: Option<&Vec<Route>>, ctx: &ItemContext<'_>) -> String {
    let w = ctx.words;
    let named: Vec<(&Route, String)> = routes
        .into_iter()
        .flatten()
        .map(|r| (r, clean_title(&(ctx.title_of)(&r.2))))
        .filter(|(_, t)| !t.is_empty())
        .collect();
    let Some((first, title)) = named.first() else { return format!(" — {}", w.it_no_contract) };
    let mut part = format!(" — {} \"{title}\"", w.it_from);
    if !first.3.is_empty() {
        part.push_str(&format!(", {} {}", w.it_from_rank, first.3));
    }
    let others = named.iter().map(|(_, t)| t.as_str()).collect::<HashSet<_>>().len() - 1;
    if others > 0 {
        part.push_str(&format!(" (+{others} {})", w.it_more));
    }
    part
}

/// Named blueprint lines a shared description shows at most; with more
/// items, owned and missing ones are counted instead.
const NAMED_LINES: usize = 3;

/// The blueprint lines of a shared description with more than
/// [`NAMED_LINES`] items: owned and missing ones each as one counted line
/// (`Blueprint: [  ] 6 variants missing` and the contract), a group of one
/// by name.
fn counted_lines(rows: &[Rows], ctx: &ItemContext<'_>) -> Vec<String> {
    let w = ctx.words;
    let mut lines = Vec::new();
    for have in [true, false] {
        let group: Vec<&Rows> = rows.iter().filter(|r| r.have == have).collect();
        match group.len() {
            0 => {}
            1 => lines.push(group[0].named.clone()),
            count if have => {
                lines.push(format!("{}: {} {count} {}", w.it_blueprint, super::form::BOX_HAVE, w.it_variants_have))
            }
            count => {
                let mut routes: Vec<Route> = group.iter().flat_map(|r| r.routes.iter().cloned()).collect();
                routes.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
                lines.push(format!(
                    "{}: {} {count} {}{}",
                    w.it_blueprint,
                    super::form::BOX_MISSING,
                    w.it_variants_missing,
                    route_part(Some(&routes), ctx)
                ));
            }
        }
    }
    lines
}

/// `item_info._craft_line`.
fn craft_line(item: &Item, words: &Words) -> String {
    let Some((count, seconds)) = &item.first_tier else { return String::new() };
    if *count == 0 {
        return String::new();
    }
    let noun = if *count == 1 { words.it_ingredient } else { words.it_ingredients };
    let mut parts = vec![format!("{count} {noun}")];
    let span = time_text(seconds);
    if !span.is_empty() {
        parts.push(span);
    }
    format!("{}: {}", words.it_crafting, parts.join(" · "))
}

/// `item_info._shop_line`.
fn shop_line(item: &Item, shops: &Shops, ctx: &ItemContext<'_>) -> String {
    let w = ctx.words;
    let name = item.base.to_lowercase();
    let mut hit = shops.by_entity.get(&item.entity);
    if hit.is_none() && !shops.ids.contains(&item.entity) {
        hit = shops.by_name.get(&name);
    }
    if let Some((price, place)) = hit {
        return format!("{}: {} · {} aUEC", w.it_shop, py_str(place), (ctx.digits)(price));
    }
    if shops.ids.contains(&item.entity) || shops.names.contains(&name) {
        return format!("{}: {}", w.it_shop, w.it_not_sold);
    }
    String::new()
}

/// `item_info._names_in`: name key → name without tag, from the lines.
fn names_in<S: AsRef<str>>(lines: &[S], names: &[(String, String)]) -> Vec<(String, String)> {
    let wanted: HashSet<&str> = names.iter().map(|(k, _)| k.as_str()).collect();
    let mut found: Vec<(String, String)> = Vec::new();
    for line in lines {
        let line = line.as_ref();
        let Some(sep) = line.find('=') else { continue };
        if sep < 1 {
            continue;
        }
        let key = line[..sep].split(',').next().unwrap_or("");
        if wanted.contains(key) {
            let name = py_strip(&super::specs::strip_tag(&line[sep + 1..])).to_string();
            match found.iter_mut().find(|(k, _)| k == key) {
                Some(slot) => slot.1 = name,
                None => found.push((key.to_string(), name)),
            }
        }
    }
    found
}

/// The keys of the file that get the block of an item: its description key,
/// never its name key. Where the game data name the name key as description,
/// the `_Desc` key beside the `_Name` key is taken if the file has it, else
/// nothing; the game shows names in its cockpit displays.
fn description_keys<'a>(known: &ItemKeys, all_keys: &'a HashMap<String, Vec<String>>) -> Option<&'a Vec<String>> {
    let text = known.text.to_lowercase();
    if text != known.name.to_lowercase() {
        return all_keys.get(&text);
    }
    let stem = text.strip_suffix("_name")?;
    all_keys.get(&format!("{stem}_desc"))
}

/// Every key of the lines in lower case → the keys in their own spelling,
/// and the name keys of `texts` in lower case → the name without tag. Name
/// and description keys do not always start with `item_Name` and `item_Desc`
/// (`item_Mining_MiningLaser_Thermyte_2_S2_Desc`).
fn keys_in<S: AsRef<str>>(lines: &[S], texts: &ItemTexts) -> (HashMap<String, Vec<String>>, HashMap<String, String>) {
    let wanted: HashSet<String> = texts.values().map(|k| k.name.to_lowercase()).filter(|k| !k.is_empty()).collect();
    let mut keys: HashMap<String, Vec<String>> = HashMap::new();
    let mut names: HashMap<String, String> = HashMap::new();
    for line in lines {
        let line = line.as_ref();
        let Some(sep) = line.find('=') else { continue };
        if sep < 1 {
            continue;
        }
        let key = line[..sep].split(',').next().unwrap_or("");
        let lower = key.to_lowercase();
        if wanted.contains(&lower) {
            names.insert(lower.clone(), py_strip(&super::specs::strip_tag(&line[sep + 1..])).to_string());
        }
        let spellings = keys.entry(lower).or_default();
        if !spellings.iter().any(|k| k == key) {
            spellings.push(key.to_string());
        }
    }
    (keys, names)
}

/// A text line the file lacks for a crafted item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Missing {
    /// The key in the spelling of the catalog of the game data.
    pub key: String,
    /// The stand-in name for a name key; `None` for a description key, which
    /// gets only the block.
    pub name: Option<String>,
}

/// A stand-in name from a name key: the part after the last `_` of the class
/// name (`item_Name_RADR_WLOP_S03_Lephari` gives `Lephari`). A trailing
/// `_SCItem` is ignored, and a size part alone (`S1`) takes the part before it
/// (`LaserCannon S1`). `None` where the key leaves no part.
pub fn fallback_name(key: &str) -> Option<String> {
    let rest = ["item_name", "item_"]
        .iter()
        .find_map(|prefix| {
            key.get(..prefix.len()).filter(|head| head.eq_ignore_ascii_case(prefix)).map(|_| &key[prefix.len()..])
        })
        .unwrap_or(key);
    let mut parts: Vec<&str> = rest.split('_').filter(|p| !p.is_empty()).collect();
    if parts.last().is_some_and(|p| p.eq_ignore_ascii_case("scitem")) {
        parts.pop();
    }
    let last = *parts.last()?;
    let is_size = |part: &str| {
        let mut chars = part.chars();
        matches!(chars.next(), Some('S' | 's'))
            && !chars.as_str().is_empty()
            && chars.as_str().len() <= 2
            && chars.all(|c| c.is_ascii_digit())
    };
    match parts.len().checked_sub(2).map(|at| parts[at]) {
        Some(before) if is_size(last) && !before.chars().all(|c| c.is_ascii_digit()) => {
            Some(format!("{before} {last}"))
        }
        _ => Some(last.to_string()),
    }
}

/// Whether the line `key` holds `text` in the file is the text of a crafted
/// item that is not the one [`missing_texts`] writes for it: the game wrote
/// a text of its own there since. `false` for a key that is no text key of the
/// catalog and for a key without a line.
pub fn foreign_text(key: &str, text: Option<&str>, texts: &ItemTexts) -> bool {
    let Some(text) = text else { return false };
    if texts.values().any(|known| known.name == key) {
        return fallback_name(key).as_deref() != Some(text);
    }
    texts.values().any(|known| known.text == key) && !text.starts_with(&super::texts::rule())
}

/// The lines the file lacks for the crafted items whose text keys the game
/// data name: a stand-in name for every missing name key, and, with
/// `with_descriptions`, every missing description key (which only gets the
/// block). `present` holds the keys of the file in lower case, without lines
/// we added ourselves; a key with a text, however short, is never listed.
pub fn missing_texts(
    present: &HashSet<String>,
    items: &[Item],
    texts: &ItemTexts,
    with_descriptions: bool,
) -> Vec<Missing> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut found = Vec::new();
    for item in items {
        let Some(known) = texts.get(&item.entity.to_lowercase()) else { continue };
        let name = known.name.to_lowercase();
        if !name.is_empty()
            && !present.contains(&name)
            && let Some(stand_in) = fallback_name(&known.name)
            && seen.insert(name)
        {
            found.push(Missing { key: known.name.clone(), name: Some(stand_in) });
        }
        let text = known.text.to_lowercase();
        if with_descriptions && !text.is_empty() && !present.contains(&text) && seen.insert(text) {
            found.push(Missing { key: known.text.clone(), name: None });
        }
    }
    found
}

/// What the block says of one item: its name, the blueprint line without and
/// with the name, crafting and shop (empty when unknown).
struct Rows {
    label: String,
    have: bool,
    routes: Vec<Route>,
    plain: String,
    named: String,
    craft: String,
    shop: String,
}

/// The block from its rows.
fn block_of(rows: Vec<String>, words: &Words) -> String {
    let mut parts =
        vec![String::new(), super::texts::rule(), String::new(), format!("<EM4>{}</EM4>", words.it_heading)];
    parts.extend(rows.into_iter().filter(|row| !row.is_empty()));
    format!("\\n{}", parts.join("\\n"))
}

/// The block of a description the game shows for several items: one block as
/// for a single item when it reads the same for all of them, else one named
/// blueprint line per item (counted past [`NAMED_LINES`] items, see
/// [`counted_lines`]), with crafting and shop only where they are the same
/// for all.
fn shared_block(mut rows: Vec<Rows>, ctx: &ItemContext<'_>) -> String {
    rows.sort_by(|a, b| a.label.cmp(&b.label));
    let first = &rows[0];
    let same = |pick: fn(&Rows) -> &String| rows.iter().all(|r| pick(r) == pick(first));
    let craft = if same(|r| &r.craft) { first.craft.clone() } else { String::new() };
    let shop = if same(|r| &r.shop) { first.shop.clone() } else { String::new() };
    let mut lines: Vec<String> = if same(|r| &r.plain) {
        vec![first.plain.clone()]
    } else if rows.len() > NAMED_LINES {
        counted_lines(&rows, ctx)
    } else {
        rows.iter().map(|r| r.named.clone()).collect()
    };
    lines.push(craft);
    lines.push(shop);
    block_of(lines, ctx.words)
}

/// `item_info.build`: description key → block, for every craftable item.
/// `original_names` are the stored English names (`namen-original.json`);
/// `english` says the file itself is English.
///
/// Where `texts` (from the catalog of the game data) names the description
/// key of an item, that key gets the block, and only that one: the game shows
/// it, not one that merely looks like the item's name key (arms and helmet
/// of a set, colour variants and paints often share the description of
/// another item). A description several items share gets [`shared_block`].
/// Items `texts` does not know are found by their tag and name as 4.12 does.
#[allow(clippy::too_many_arguments)]
pub fn build<S: AsRef<str>>(
    lines: &[S],
    missions: &Map,
    items: &[Item],
    shops: &Shops,
    original_names: &[(String, String)],
    english: bool,
    texts: &ItemTexts,
    ctx: &ItemContext<'_>,
) -> HashMap<String, String> {
    let (descs, names) = desc_keys(lines);
    if descs.is_empty() && texts.is_empty() {
        return HashMap::new();
    }
    let merged: Vec<(String, String)>;
    let original: &[(String, String)] = if english {
        // The stored ones win: they come unchanged from the Data.p4k.
        let mut both = names_in(lines, &names);
        for (k, v) in original_names {
            match both.iter_mut().find(|(key, _)| key == k) {
                Some(slot) => slot.1 = v.clone(),
                None => both.push((k.clone(), v.clone())),
            }
        }
        merged = both;
        &merged
    } else {
        original_names
    };
    let by_name = name_index(&names, original);
    let routes_of = origins(missions);
    let (all_keys, file_names) = if texts.is_empty() { Default::default() } else { keys_in(lines, texts) };
    let mut result = HashMap::new();
    // Description keys the game data name, in the order first met → rows.
    let mut shared: Vec<(Vec<String>, Vec<Rows>)> = Vec::new();
    for item in items {
        let Some(have) = item.have else { continue };
        let routes = routes_of.get(&name_key(&item.base));
        if let Some(known) = texts.get(&item.entity.to_lowercase()) {
            // A key the file lacks shows nothing of ours; a guessed one would
            // be a description the game does not show for this item.
            let Some(keys) = description_keys(known, &all_keys) else { continue };
            let label = file_names
                .get(&known.name.to_lowercase())
                .filter(|name| !name.is_empty())
                .cloned()
                .unwrap_or_else(|| item.base.clone());
            let rows = Rows {
                plain: blueprint_line(have, routes, None, ctx),
                named: blueprint_line(have, routes, Some(&label), ctx),
                craft: craft_line(item, ctx.words),
                shop: shop_line(item, shops, ctx),
                label,
                have,
                routes: routes.cloned().unwrap_or_default(),
            };
            match shared.iter_mut().find(|(k, _)| k == keys) {
                Some((_, list)) => list.push(rows),
                None => shared.push((keys.clone(), vec![rows])),
            }
            continue;
        }
        let stem = item
            .tags
            .iter()
            .flat_map(|t| tag_stems(t))
            .find(|s| descs.contains_key(s))
            .or_else(|| by_name.get(&name_key(&item.base)).cloned());
        let Some(keys) = stem.as_ref().and_then(|s| descs.get(s)) else { continue };
        let block = block_of(
            vec![blueprint_line(have, routes, None, ctx), craft_line(item, ctx.words), shop_line(item, shops, ctx)],
            ctx.words,
        );
        for key in keys {
            result.insert(key.clone(), block.clone());
        }
    }
    for (keys, rows) in shared {
        let block = shared_block(rows, ctx);
        for key in keys {
            result.insert(key, block.clone());
        }
    }
    result
}

// ------------------------------------------------- 4.12's recipe file

/// `crafting._name_from_tag`.
fn name_from_tag(tag: &str) -> String {
    let raw = tag.replace("BP_CRAFT_", "").replace("_SCItem", "").replace("_TEMP", "").replace('_', " ");
    let raw = py_strip(&raw);
    if raw.is_empty() { "?".into() } else { raw.to_string() }
}

/// `crafting._ingredients` as the strings `_merge` compares.
fn recipe_key(blueprint: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(Value::Array(tiers)) = blueprint.get("tiers") {
        for tier in tiers {
            let Some(Value::Array(slots)) = tier.get("slots") else { continue };
            for slot in slots {
                let slot_name = slot.get("name").filter(|v| truthy(Some(v))).map(py_str).unwrap_or_default();
                let Some(Value::Array(options)) = slot.get("options") else { continue };
                for option in options {
                    let material = [option.get("resourceName"), option.get("itemName")]
                        .into_iter()
                        .find(|v| truthy(*v))
                        .flatten()
                        .map(py_str)
                        .unwrap_or_default();
                    if material.is_empty() {
                        continue;
                    }
                    let amount =
                        option.get("quantity").filter(|v| truthy(Some(v))).map(py_str).unwrap_or_else(|| "0".into());
                    out.push(format!("{slot_name}|{material}|{amount}"));
                }
            }
        }
    }
    out.sort();
    out
}

fn text_of(value: Option<&Value>) -> String {
    value.filter(|v| truthy(Some(v))).map(py_str).unwrap_or_default()
}

/// The name and entity corrections of `cig_issues.apply_blueprints` for a
/// blueprint, from the parsed `daten` of `cig-datenfehler.json` of the same
/// build: (product name, entity class) to use.
fn corrected(blueprint: &Value, issues: Option<&Value>) -> (Option<String>, Option<String>) {
    let guid = blueprint.get("guid").and_then(Value::as_str).map(|g| py_strip(g).to_lowercase());
    let entry = issues
        .and_then(|i| i.get("blueprintRecords"))
        .and_then(Value::as_object)
        .and_then(|records| {
            let guid = guid?;
            records.iter().find(|(k, v)| !k.starts_with('_') && v.as_object().is_some() && k.to_lowercase() == guid)
        })
        .map(|(_, v)| v);
    let Some(entry) = entry else { return (None, None) };
    let Some(expected) = entry.get("expected").filter(|e| e.as_object().is_some()) else { return (None, None) };
    if truthy(entry.get("slot")) && truthy(entry.get("propertyKey")) {
        return (None, None);
    }
    let name = expected.get("name").filter(|v| truthy(Some(v))).map(py_str);
    let entity = expected.get("entityClass").filter(|v| truthy(Some(v))).map(py_str);
    (name, entity)
}

/// `crafting.with_collection(owned)` from 4.12's parsed
/// `crafting-blueprints.json`, with the name and entity corrections of the
/// error list (`issues`, the `daten` of `cig-datenfehler.json` of the same
/// build). Ordered as 4.12 orders them; recipes are looked up by tag.
pub fn items_from_412(data: &Value, issues: Option<&Value>, owned: &BTreeSet<String>) -> Vec<Item> {
    let blueprints: Vec<Value> = match data.get("blueprints") {
        Some(Value::Array(list)) => list
            .iter()
            .map(|b| {
                let mut b = b.clone();
                let (name, entity) = corrected(&b, issues);
                if let Value::Object(map) = &mut b {
                    if let Some(entity) = entity {
                        crate::jsonio::map_set(map, "productEntityClass", Value::str(entity));
                    }
                    if let Some(name) = name {
                        crate::jsonio::map_set(map, "productName", Value::str(name));
                    }
                }
                b
            })
            .collect(),
        _ => Vec::new(),
    };
    let mut by_tag: HashMap<String, &Value> = HashMap::new();
    for b in &blueprints {
        let tag = text_of(b.get("tag"));
        if !tag.is_empty() {
            by_tag.insert(tag, b);
        }
    }
    let product = |b: &Value| {
        let name = text_of(b.get("productName"));
        if name.is_empty() { name_from_tag(&text_of(b.get("tag"))) } else { name }
    };
    let mut groups: Vec<(String, Vec<&Value>)> = Vec::new();
    for b in &blueprints {
        let k = name_key(&product(b));
        match groups.iter_mut().find(|(key, _)| *key == k) {
            Some((_, list)) => list.push(b),
            None => groups.push((k, vec![b])),
        }
    }
    let mut items: Vec<(String, Item)> = Vec::new();
    for (_, group) in &groups {
        let mut parts: Vec<(Vec<String>, Vec<&Value>)> = Vec::new();
        for b in group {
            let key = recipe_key(b);
            match parts.iter_mut().find(|(k, _)| *k == key) {
                Some((_, list)) => list.push(b),
                None => parts.push((key, vec![b])),
            }
        }
        let several = parts.len() > 1;
        for (_, part) in &parts {
            let b = part[0];
            let name = product(b);
            let display = if several {
                format!("{name} ({})", distinguisher(&text_of(b.get("tag")), &name))
            } else {
                name.clone()
            };
            let tag = text_of(b.get("tag"));
            let first_tier = by_tag.get(&tag).and_then(|recipe| match recipe.get("tiers") {
                Some(Value::Array(tiers)) if !tiers.is_empty() => {
                    let slots = match tiers[0].get("slots") {
                        Some(Value::Array(s)) => s.len(),
                        _ => 0,
                    };
                    Some((slots, tiers[0].get("craftTimeSeconds").cloned().unwrap_or(Value::Null)))
                }
                _ => None,
            });
            items.push((
                display.to_lowercase(),
                Item {
                    base: name,
                    tag,
                    tags: part.iter().map(|x| text_of(x.get("tag"))).collect(),
                    entity: text_of(b.get("productEntityClass")),
                    have: None,
                    first_tier,
                },
            ));
        }
    }
    // A stable sort by the shown name in lower case, like `sort(key=…)`.
    items.sort_by(|a, b| a.0.cmp(&b.0));
    let mut seen = HashSet::new();
    let mut ambiguous = HashSet::new();
    for (_, item) in &items {
        let k = name_key(&item.base);
        if !seen.insert(k.clone()) {
            ambiguous.insert(k);
        }
    }
    items
        .into_iter()
        .map(|(_, mut item)| {
            let k = name_key(&item.base);
            let owns = owned.contains(&k);
            item.have = if owns && ambiguous.contains(&k) { None } else { Some(owns) };
            item
        })
        .collect()
}

/// `crafting._distinguisher`.
fn distinguisher(tag: &str, name: &str) -> String {
    let raw = name_from_tag(tag);
    let in_name: HashSet<String> = name.split_whitespace().map(str::to_lowercase).collect();
    let parts: Vec<&str> = raw
        .split_whitespace()
        .filter(|w| !in_name.contains(&w.to_lowercase()) && !["BP", "CRAFT"].contains(&w.to_uppercase().as_str()))
        .collect();
    if parts.is_empty() { raw } else { parts[parts.len().saturating_sub(2)..].join(" ") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsonio;

    #[test]
    fn times_like_python() {
        assert_eq!(time_text(&Value::int(0)), "");
        assert_eq!(time_text(&Value::int(45)), "45 s");
        assert_eq!(time_text(&Value::int(89)), "1 min");
        assert_eq!(time_text(&Value::int(90)), "2 min");
        assert_eq!(time_text(&Value::int(3690)), "1 h 2 min");
    }

    #[test]
    fn titles_lose_placeholders() {
        assert_eq!(clean_title("Deliver ~mission(Location)  now - "), "Deliver now");
    }

    #[test]
    fn a_block_per_description() {
        let lines = [
            "item_DescCOOL_JSPN_S01_FrostStar=Cooler",
            "item_NameCOOL_JSPN_S01_FrostStar=FrostStar",
            "k_title=Wartungsauftrag ~mission(Location)",
        ];
        let missions =
            jsonio::loads(r#"{"k": {"titel_key": "k_title", "bp": ["FrostStar"], "rep": 0, "rang": "Junior"}}"#)
                .unwrap()
                .as_object()
                .unwrap()
                .clone();
        let item = Item {
            base: "FrostStar".into(),
            tag: "BP_CRAFT_COOL_JSPN_S01_FrostStar_SCItem".into(),
            tags: vec!["BP_CRAFT_COOL_JSPN_S01_FrostStar_SCItem".into()],
            entity: "e1".into(),
            have: Some(false),
            first_tier: Some((3, Value::int(240))),
        };
        let shops = Shops {
            by_entity: [("e1".to_string(), (Value::int(12400), Value::str("New Babbage")))].into_iter().collect(),
            ..Shops::default()
        };
        let words = super::super::texts::words_for("german_(germany)", true);
        let title =
            |key: &str| if key == "k_title" { "Wartungsauftrag ~mission(Location)".to_string() } else { String::new() };
        let digits = |v: &Value| super::super::texts::group_digits(py_int(Some(v)).unwrap_or(0), ".");
        let ctx = ItemContext { words: &words, title_of: &title, digits: &digits };
        let blocks = build(&lines, &missions, &[item], &shops, &[], false, &ItemTexts::new(), &ctx);
        let block = &blocks["item_DescCOOL_JSPN_S01_FrostStar"];
        assert!(block.ends_with(
            "VERSE-KIT: BAUPLAN & HERSTELLUNG</EM4>\\nBauplan: [  ] fehlt — aus \"Wartungsauftrag\", ab Rang Junior\\nHerstellen: 3 Zutaten · 4 min\\nIm Laden: New Babbage · 12.400 aUEC"
        ), "{block}");
    }

    fn armor(name: &str, tag: &str, entity: &str, have: bool) -> Item {
        Item {
            base: name.into(),
            tag: tag.into(),
            tags: vec![tag.into()],
            entity: entity.into(),
            have: Some(have),
            first_tier: Some((2, Value::int(240))),
        }
    }

    fn keys(name: &str, text: &str) -> ItemKeys {
        ItemKeys { name: name.into(), text: text.into() }
    }

    fn english_blocks(lines: &[&str], items: &[Item], texts: &ItemTexts) -> HashMap<String, String> {
        let missions = jsonio::loads(
            r#"{"cfp": {"titel_key": "cfp_title", "bp": ["ADP-mk4 Arms Woodland", "ADP-mk4 Core Woodland",
                "ADP-mk4 Helmet Woodland", "Gallant Rifle", "Gallant \"Midnight\" Rifle"], "rep": 0}}"#,
        )
        .unwrap()
        .as_object()
        .unwrap()
        .clone();
        let words = super::super::texts::words_for("english", true);
        let title = |key: &str| if key == "cfp_title" { "Destroy Stolen Data".to_string() } else { String::new() };
        let digits = |v: &Value| super::super::texts::group_digits(py_int(Some(v)).unwrap_or(0), ",");
        let ctx = ItemContext { words: &words, title_of: &title, digits: &digits };
        build(lines, &missions, items, &Shops::default(), &[], true, texts, &ctx)
    }

    /// The ADP-mk4 set of the game data: arms and helmet show the description
    /// `item_Desc_cds_heavy_armor_01_Shared`, core and legs one of their own.
    const ADP_MK4: [&str; 6] = [
        "item_Desc_cds_heavy_armor_01_Shared=Item Type: Heavy Armor",
        "item_Desc_cds_heavy_armor_01_core=Item Type: Heavy Armor",
        "item_Name_cds_heavy_armor_01_arms=ADP-mk4 Arms Woodland",
        "item_Name_cds_heavy_armor_01_core=ADP-mk4 Core Woodland",
        "item_Name_cds_heavy_armor_helmet_01=ADP-mk4 Helmet Woodland",
        "cfp_title=Destroy Stolen Data",
    ];

    fn adp_mk4(arms: bool, helmet: bool) -> Vec<Item> {
        vec![
            armor("ADP-mk4 Arms Woodland", "BP_CRAFT_cds_armor_heavy_arms_01_01_01", "bef9cd69", arms),
            armor("ADP-mk4 Core Woodland", "BP_CRAFT_cds_armor_heavy_core_01_01_01", "8eabeef4", true),
            armor("ADP-mk4 Helmet Woodland", "BP_CRAFT_cds_armor_heavy_helmet_01_01_01", "80FD0B3B", helmet),
        ]
    }

    fn adp_mk4_texts() -> ItemTexts {
        [
            ("bef9cd69", keys("item_Name_cds_heavy_armor_01_arms", "item_Desc_cds_heavy_armor_01_Shared")),
            ("8eabeef4", keys("item_Name_cds_heavy_armor_01_core", "item_Desc_cds_heavy_armor_01_core")),
            ("80fd0b3b", keys("item_Name_cds_heavy_armor_helmet_01", "item_Desc_cds_heavy_armor_01_Shared")),
        ]
        .into_iter()
        .map(|(guid, keys)| (guid.to_string(), keys))
        .collect()
    }

    #[test]
    fn arms_get_the_description_the_game_shows_them() {
        let blocks = english_blocks(&ADP_MK4, &adp_mk4(true, false), &adp_mk4_texts());
        let shared = &blocks["item_Desc_cds_heavy_armor_01_Shared"];
        assert!(shared.contains("\\nBlueprint: <EM4>[x]</EM4> ADP-mk4 Arms Woodland (owned)\\n"), "{shared}");
        assert!(
            shared.contains("\\nBlueprint: [  ] ADP-mk4 Helmet Woodland (missing) — from \"Destroy Stolen Data\"\\n"),
            "{shared}"
        );
        assert!(shared.ends_with("\\nCrafting: 2 ingredients · 4 min"), "the same for both: {shared}");
        assert!(blocks["item_Desc_cds_heavy_armor_01_core"].contains("\\nBlueprint: <EM4>[x]</EM4> owned\\n"));
        assert_eq!(blocks.len(), 2);
    }

    /// Mining modules whose game data name the name key as description.
    const MODULES: [&str; 4] = [
        "item_Mining_Modules_Passive_THCN_Power_BTGL_Name=Deluge Module",
        "item_Mining_Modules_Passive_THCN_Power_BTGL_Desc=Manufacturer: Greycat Industrial",
        "item_Mining_Modules_Active_GRIN_Charge_BTGL_Name=Clearcut Module",
        "cfp_title=Destroy Stolen Data",
    ];

    fn module(name: &str, entity: &str) -> Item {
        armor(name, "BP_CRAFT_Mining_Modules_Passive_Deluge", entity, true)
    }

    fn module_texts() -> ItemTexts {
        [
            ("d1", keys("item_Mining_Modules_Passive_THCN_Power_BTGL_Name", "item_Mining_Modules_Passive_THCN_Power_BTGL_Name")),
            ("c1", keys("item_Mining_Modules_Active_GRIN_Charge_BTGL_Name", "item_Mining_Modules_Active_GRIN_Charge_BTGL_Name")),
        ]
        .into_iter()
        .map(|(guid, keys)| (guid.to_string(), keys))
        .collect()
    }

    #[test]
    fn a_name_key_never_gets_a_block_the_description_beside_it_does() {
        let items = [module("Deluge Module", "d1"), module("Clearcut Module", "c1")];
        let blocks = english_blocks(&MODULES, &items, &module_texts());
        assert!(!blocks.contains_key("item_Mining_Modules_Passive_THCN_Power_BTGL_Name"), "{blocks:?}");
        assert!(!blocks.contains_key("item_Mining_Modules_Active_GRIN_Charge_BTGL_Name"), "{blocks:?}");
        assert!(
            blocks["item_Mining_Modules_Passive_THCN_Power_BTGL_Desc"].contains("Blueprint: <EM4>[x]</EM4>"),
            "{blocks:?}"
        );
        assert_eq!(blocks.len(), 1, "the module without a description in the file gets nothing: {blocks:?}");
    }

    #[test]
    fn without_the_keys_of_the_game_data_the_arms_stay_without_block_as_in_4_12() {
        let blocks = english_blocks(&ADP_MK4, &adp_mk4(true, false), &ItemTexts::new());
        assert!(blocks.contains_key("item_Desc_cds_heavy_armor_01_core"));
        assert!(!blocks.contains_key("item_Desc_cds_heavy_armor_01_Shared"), "the name key of the arms finds nothing");
    }

    #[test]
    fn a_shared_description_that_reads_the_same_for_all_items_gets_the_plain_block() {
        let blocks = english_blocks(&ADP_MK4, &adp_mk4(true, true), &adp_mk4_texts());
        let shared = &blocks["item_Desc_cds_heavy_armor_01_Shared"];
        assert!(shared.contains("\\nBlueprint: <EM4>[x]</EM4> owned\\n"), "{shared}");
        assert!(!shared.contains("Woodland"), "{shared}");
    }

    #[test]
    fn a_paint_gets_the_description_of_its_weapon_and_never_a_look_alike_key() {
        let lines = [
            "item_Descklwe_rifle_energy_01=Manufacturer: Klaus & Werner",
            "item_Descklwe_rifle_energy_01_black01=Manufacturer: Klaus & Werner",
            "item_Nameklwe_rifle_energy_01=Gallant Rifle",
            "item_Nameklwe_rifle_energy_01_black01=Gallant \"Midnight\" Rifle",
            "cfp_title=Destroy Stolen Data",
        ];
        let items = [
            armor("Gallant Rifle", "BP_CRAFT_klwe_rifle_energy_01", "aaaa", false),
            armor("Gallant \"Midnight\" Rifle", "BP_CRAFT_klwe_rifle_energy_01_black01", "bbbb", false),
        ];
        let texts: ItemTexts = [
            ("aaaa".to_string(), keys("item_Nameklwe_rifle_energy_01", "item_Descklwe_rifle_energy_01")),
            ("bbbb".to_string(), keys("item_Nameklwe_rifle_energy_01_black01", "item_Descklwe_rifle_energy_01")),
        ]
        .into_iter()
        .collect();
        let blocks = english_blocks(&lines, &items, &texts);
        assert_eq!(blocks.keys().collect::<Vec<_>>(), ["item_Descklwe_rifle_energy_01"], "{blocks:?}");
        assert!(blocks["item_Descklwe_rifle_energy_01"].contains("\\nBlueprint: [  ] missing — from \"Destroy"));
        let guessed = english_blocks(&lines, &items, &ItemTexts::new());
        assert!(guessed.contains_key("item_Descklwe_rifle_energy_01_black01"), "4.12 writes the look-alike key");
    }

    /// Seven paints of one pistol that all show the description of the
    /// pistol; `owned` lists the indexes of the owned ones.
    fn arclight_blocks(owned: &[usize], count: usize) -> String {
        let paints = ["", "_black01", "_blue01", "_desert01", "_exec01", "_red01", "_white01"];
        let mut lines = vec!["item_Descklwe_pistol_energy_01=Manufacturer: Klaus & Werner".to_string()];
        let mut items = Vec::new();
        let mut texts = ItemTexts::new();
        for (i, paint) in paints.iter().take(count).enumerate() {
            let name = if paint.is_empty() { "Gallant Rifle".to_string() } else { format!("Gallant \"P{i}\" Rifle") };
            let key = format!("item_Nameklwe_pistol_energy_01{paint}");
            lines.push(format!("{key}={name}"));
            let guid = format!("g{i}");
            items.push(armor(&name, &format!("BP_CRAFT_klwe_pistol_energy_01{paint}"), &guid, owned.contains(&i)));
            texts.insert(guid, keys(&key, "item_Descklwe_pistol_energy_01"));
        }
        lines.push("cfp_title=Destroy Stolen Data".into());
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        let blocks = english_blocks(&lines, &items, &texts);
        assert_eq!(blocks.len(), 1, "{blocks:?}");
        blocks["item_Descklwe_pistol_energy_01"].clone()
    }

    fn blueprint_rows(block: &str) -> Vec<&str> {
        block.split("\\n").filter(|row| row.starts_with("Blueprint:")).collect()
    }

    #[test]
    fn many_items_on_one_description_are_counted_not_listed() {
        let block = arclight_blocks(&[2], 7);
        assert_eq!(
            blueprint_rows(&block),
            [
                "Blueprint: <EM4>[x]</EM4> Gallant \"P2\" Rifle (owned)",
                "Blueprint: [  ] 6 variants missing — from \"Destroy Stolen Data\"",
            ],
            "{block}"
        );
        let block = arclight_blocks(&[0, 3], 7);
        assert_eq!(
            blueprint_rows(&block),
            ["Blueprint: <EM4>[x]</EM4> 2 variants owned", "Blueprint: [  ] 5 variants missing — not from contracts"],
            "{block}"
        );
    }

    #[test]
    fn up_to_three_items_on_one_description_stay_named() {
        let block = arclight_blocks(&[1], 3);
        assert_eq!(blueprint_rows(&block).len(), 3, "{block}");
        assert!(block.contains("Gallant \"P2\" Rifle (missing)"), "{block}");
    }

    #[test]
    fn keys_without_item_desc_in_front_are_found_through_the_game_data() {
        let lines = [
            "item_Mining_MiningLaser_Thermyte_2_S2=Helix II Bergbaulaser",
            "item_Mining_MiningLaser_Thermyte_2_S2_Desc=Hersteller: Thermyte Concern",
            "cfp_title=Destroy Stolen Data",
        ];
        let items = [armor("Helix II Mining Laser", "BP_CRAFT_Mining_Laser_THCN_Helix_S2", "3bcde69d", true)];
        let texts: ItemTexts = [(
            "3bcde69d".to_string(),
            keys("item_Mining_MiningLaser_Thermyte_2_S2", "item_Mining_MiningLaser_Thermyte_2_S2_Desc"),
        )]
        .into_iter()
        .collect();
        assert!(english_blocks(&lines, &items, &ItemTexts::new()).is_empty(), "4.12 finds no description");
        let blocks = english_blocks(&lines, &items, &texts);
        assert!(blocks["item_Mining_MiningLaser_Thermyte_2_S2_Desc"].contains("\\nBlueprint: <EM4>[x]</EM4> owned"));
    }

    #[test]
    fn a_shared_description_names_its_items_as_the_file_does() {
        let mut lines = ADP_MK4.to_vec();
        lines[2] = "item_Name_cds_heavy_armor_01_arms=ADP-mk4 Arme Woodland (Mil/1/A)";
        let blocks = english_blocks(&lines, &adp_mk4(false, true), &adp_mk4_texts());
        let shared = &blocks["item_Desc_cds_heavy_armor_01_Shared"];
        assert!(shared.contains("[  ] ADP-mk4 Arme Woodland (missing)"), "{shared}");
        let arms = shared.find("Arme Woodland").unwrap();
        assert!(arms < shared.find("Helmet Woodland").unwrap(), "ordered by name: {shared}");
    }

    #[test]
    fn a_description_key_the_file_lacks_gets_nothing_guessed() {
        let texts: ItemTexts = [("8eabeef4".to_string(), keys("item_Name_cds_heavy_armor_01_core", "item_Desc_gone"))]
            .into_iter()
            .collect();
        let blocks = english_blocks(&ADP_MK4, &adp_mk4(true, false)[1..2], &texts);
        assert!(blocks.is_empty(), "{blocks:?}");
    }

    #[test]
    fn the_keys_are_read_from_the_catalog_of_the_game_data() {
        let data = jsonio::loads(
            r#"{"bauplaene": {}, "beschreibungen": {
                "BEF9CD69": {"name": "item_Name_cds_heavy_armor_01_arms", "text": "item_Desc_cds_heavy_armor_01_Shared"},
                "empty": {"name": "item_Name_x", "text": " "}}}"#,
        )
        .unwrap();
        let texts = item_texts_from_catalog(&data);
        assert_eq!(texts.len(), 1);
        assert_eq!(texts["bef9cd69"], keys("item_Name_cds_heavy_armor_01_arms", "item_Desc_cds_heavy_armor_01_Shared"));
        assert!(item_texts_from_catalog(&jsonio::loads(r#"{"bauplaene": {}}"#).unwrap()).is_empty());
    }
}
