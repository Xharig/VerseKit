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

//! Getting the files of erkul (`erkul._fetch`, `ship_catalog`, `_maker_table` of 4.12): the way in, behind
//! an interface. The files lie packed on `https://cdn.erkul.games/LIVE/…` (`.bin`, raw deflate without the
//! zlib header); what asks the net is a [`Feed`], so everything here runs on stored answers in the tests and
//! in the checks: **no request leaves the process from this file.**
//!
//! Built: the way in ([`unpack`], [`fetch`]), the catalog, the table of makers and the reduction of one ship
//! file to its slots ([`fetch_ship`]), equal to 4.12 (`tools/parity_erkul.py`, section `fetch`).
//!
//! Also the parts ([`slim_part`], [`refresh_parts`]), equal to 4.12 the same way.
//!
//! And the choice of the ship files from the catalog with writing `erkul-schiffe.json` and `erkul-teile.json`
//! ([`add_missing`], [`store_parts`]). The [`Feed`] that really asks the net is built by the caller (the pages):
//! this file knows no address but [`address`].

use crate::hangar::slim;
use crate::jsonio::{self, Value};
use crate::paths::Paths;
use crate::uex::Store;

/// The address of the files.
pub const BASE: &str = "https://cdn.erkul.games";
/// The branch of the game the files are read for.
pub const BRANCH: &str = "LIVE";

/// Where the packed files come from: the bytes of `path` (relative to [`BASE`], e.g. `LIVE/catalog.bin`), or
/// `None` where they could not be had (no net, an answer that is no file). Never an error: without the net the
/// tool goes on as before, as it does with UEX.
pub trait Feed {
    fn get(&self, path: &str) -> Option<Vec<u8>>;
}

/// A feed made of a function, for the one that talks to the net and for the made-up ones.
pub struct FnFeed<F: Fn(&str) -> Option<Vec<u8>>>(pub F);

impl<F: Fn(&str) -> Option<Vec<u8>>> Feed for FnFeed<F> {
    fn get(&self, path: &str) -> Option<Vec<u8>> {
        (self.0)(path)
    }
}

/// The address of a file.
pub fn address(path: &str) -> String {
    format!("{BASE}/{}", path.trim_start_matches('/'))
}

/// Unpacks a file: raw deflate, then JSON. `None` for anything else.
/// ⚠ `decompress_to_vec` is the raw one; with a zlib header (`decompress_to_vec_zlib`) it fails at once, as
/// `zlib.decompress(raw)` does without `-15` in Python.
pub fn unpack(raw: &[u8]) -> Option<Value> {
    let bytes = miniz_oxide::inflate::decompress_to_vec_with_limit(raw, MAX_UNPACKED).ok()?;
    jsonio::loads(&String::from_utf8(bytes).ok()?).ok()
}

/// Packs a text the way erkul packs its files (raw deflate): for the made-up answers of the tests and checks.
pub fn pack(text: &str) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec(text.as_bytes(), 6)
}

/// More than this unpacked is no file of erkul (a zip bomb would otherwise fill the memory).
const MAX_UNPACKED: usize = 256 * 1024 * 1024;

/// `erkul._fetch`: one file, unpacked.
pub fn fetch(feed: &dyn Feed, path: &str) -> Option<Value> {
    unpack(&feed.get(path)?)
}

/// `erkul.ship_catalog`: the table of contents with the game version and the current file names.
pub fn ship_catalog(feed: &dyn Feed) -> Option<Value> {
    fetch(feed, &format!("{BRANCH}/catalog.bin"))
}

/// `erkul._maker_table`: the written-out name of a maker, slimmed, to the abbreviation of erkul (152 entries
/// in `manufacturers.<hash>.bin`). Without it a ship entered by hand stays without slots: `anvl` is no prefix
/// of `Anvil`.
pub fn maker_table(feed: &dyn Feed, catalog: &Value) -> Vec<(String, String)> {
    let path = match catalog.get("families") {
        Some(Value::Array(families)) => families
            .iter()
            .filter_map(|f| f.get("path").and_then(Value::as_str))
            .find(|p| p.starts_with("manufacturers"))
            .unwrap_or_default()
            .to_string(),
        _ => String::new(),
    };
    if path.is_empty() {
        return Vec::new();
    }
    let Some(Value::Array(rows)) = fetch(feed, &format!("{BRANCH}/{path}")) else { return Vec::new() };
    let mut out: Vec<(String, String)> = Vec::new();
    for entry in &rows {
        let abbreviation = entry.get("className").and_then(Value::as_str).unwrap_or_default().trim();
        let plain = entry.get("i18n").and_then(|i| i.get("name")).and_then(Value::as_str).unwrap_or_default().trim();
        if abbreviation.is_empty() || plain.is_empty() {
            continue;
        }
        let key = slim(plain);
        match out.iter_mut().find(|(k, _)| *k == key) {
            Some(found) => found.1 = abbreviation.to_string(),
            None => out.push((key, abbreviation.to_string())),
        }
    }
    out
}

// ------------------------------------------------------------------ one ship

/// The flags that hide a slot from the player (`erkul.HIDDEN`).
const HIDDEN: [&str; 2] = ["invisible", "uneditable"];

fn list(value: Option<&Value>) -> &[Value] {
    match value {
        Some(Value::Array(items)) => items,
        _ => &[],
    }
}

fn some(value: Option<&Value>) -> Option<&Value> {
    value.filter(|v| !matches!(v, Value::Null))
}

fn text_of(value: Option<&Value>) -> &str {
    value.and_then(Value::as_str).unwrap_or_default()
}

/// The slots of a ship as the hardpoints name them, recursively (`erkul._collect_slots`). ⚠ They stand under
/// `vehicle.hardpoints`, not under `slots`: those say what sits in them, not what fits. On a turret the weapon
/// slots hang on the turret part, not on the hull, so it goes down.
fn collect_slots(node: Option<&Value>, out: &mut Vec<(String, i64)>) {
    for slot in list(node).iter().filter(|s| matches!(s, Value::Object(_))) {
        one_slot(slot, out);
        for field in ["hardpoints", "ports", "children", "slots"] {
            collect_slots(slot.get(field), out);
        }
        if let Some(part @ Value::Object(_)) = slot.get("item") {
            for field in ["ports", "hardpoints", "slots"] {
                collect_slots(part.get(field), out);
            }
        }
    }
}

fn one_slot(slot: &Value, out: &mut Vec<(String, i64)>) {
    if let Some(flags @ Value::Object(_)) = slot.get("flags")
        && HIDDEN.iter().any(|f| crate::hangar::truthy(flags.get(f)))
    {
        return;
    }
    let kinds: Vec<&str> = list(slot.get("accepts"))
        .iter()
        .filter_map(|takes| takes.get("type").and_then(Value::as_str))
        .filter(|kind| crate::erkul_slots::INTERESTING.contains(kind))
        .collect();
    if kinds.is_empty() {
        return;
    }
    let size = some(slot.get("maxSize")).or_else(|| some(slot.get("minSize")));
    let Some(size) = crate::hangar::py_int(size) else { return };
    for kind in kinds {
        out.push((kind.to_string(), size));
    }
}

/// Every slot under its full path (`erkul._hardpoint_index`); the first one of a path stays. ⚠ The bare name is
/// no key: on the Cutlass Black `hardpoint_class_2` exists in each of the four gimbal mounts.
fn hardpoint_index<'a>(node: Option<&'a Value>, out: &mut Vec<(String, &'a Value)>, path: &str) {
    for slot in list(node).iter().filter(|s| matches!(s, Value::Object(_))) {
        let own =
            Some(text_of(slot.get("name"))).filter(|n| !n.is_empty()).unwrap_or_else(|| text_of(slot.get("portName")));
        let full = if path.is_empty() { own.to_string() } else { format!("{path}/{own}") };
        if !own.is_empty() && !out.iter().any(|(p, _)| *p == full) {
            out.push((full.clone(), slot));
        }
        for field in ["hardpoints", "ports", "children", "slots"] {
            hardpoint_index(slot.get(field), out, &full);
        }
        if let Some(part @ Value::Object(_)) = slot.get("item") {
            for field in ["ports", "hardpoints", "slots"] {
                hardpoint_index(part.get(field), out, &full);
            }
        }
    }
}

/// What a slot takes, in the stored form, or `None` (`erkul._constraints`). The tags a part must carry are
/// written with a leading `$` in erkul, which is no part of the name.
fn constraints(desc: Option<&Value>) -> Option<Value> {
    let desc = desc.filter(|d| matches!(d, Value::Object(_)))?;
    let takes: Vec<Value> = list(desc.get("accepts"))
        .iter()
        .filter(|node| matches!(node, Value::Object(_)) && crate::hangar::truthy(node.get("type")))
        .map(|node| {
            let subs = list(node.get("subTypes")).to_vec();
            Value::Array(vec![node.get("type").cloned().unwrap_or(Value::Null), Value::Array(subs)])
        })
        .collect();
    if takes.is_empty() {
        return None;
    }
    let required: Vec<Value> = list(desc.get("requiredTags"))
        .iter()
        .map(|x| Value::str(crate::basetool::error::py_str(x).trim_start_matches('$')))
        .collect();
    let or_null = |key: &str| desc.get(key).cloned().unwrap_or(Value::Null);
    Some(Value::Object(vec![
        ("typen".to_string(), Value::Array(takes)),
        ("min".to_string(), or_null("minSize")),
        ("max".to_string(), or_null("maxSize")),
        ("porttags".to_string(), Value::Array(list(desc.get("portTags")).to_vec())),
        ("verlangt".to_string(), Value::Array(required)),
    ]))
}

/// One slot in the stored form, or `None` (`erkul._one_hardpoint`). The kind and size come from the slot
/// (`hardpoints`) in the first place, not from the part that happens to sit in it; only where the slot has no
/// entry (a missile slot made by the rack that is built in) does the part help out.
fn one_hardpoint(slot: &Value, index: &[(String, &Value)], path: &str) -> Option<Value> {
    let desc = index.iter().find(|(p, _)| p == path).map(|(_, d)| *d);
    let part = slot.get("item").filter(|p| matches!(p, Value::Object(_)));
    let kind = list(desc.and_then(|d| d.get("accepts")))
        .iter()
        .filter_map(|n| n.get("type").and_then(Value::as_str))
        .find(|k| crate::erkul_slots::SWAPPABLE.contains(k))
        .or_else(|| part.and_then(|p| p.get("type")).and_then(Value::as_str))
        .unwrap_or_default();
    if !crate::erkul_slots::SWAPPABLE.contains(&kind) {
        return None;
    }
    let size = desc
        .and_then(|d| some(d.get("maxSize")))
        .or_else(|| desc.and_then(|d| some(d.get("minSize"))))
        .or_else(|| part.and_then(|p| some(p.get("size"))));
    // A slot without a size is no mishap: paints have none, and a paint is bought too.
    let size = crate::hangar::py_int(size).map_or(Value::Null, Value::int);
    let mut entry: crate::jsonio::Map = vec![
        ("pfad".to_string(), Value::str(path)),
        ("art".to_string(), Value::str(kind)),
        ("groesse".to_string(), size),
    ];
    if let Some(fit) = constraints(desc) {
        entry.push(("passt".to_string(), fit));
    }
    if let Some(part) = part
        && crate::hangar::truthy(part.get("ref"))
    {
        // The identifier is the matter, not the name: only through it does a shop price hang on the part.
        let name = [part.get("i18n").and_then(|i| i.get("name")), part.get("className")]
            .into_iter()
            .find(|v| crate::hangar::truthy(*v))
            .map_or_else(String::new, |v| crate::basetool::error::py_str(v.unwrap_or(&Value::Null)));
        entry.push((
            "werk".to_string(),
            Value::Object(vec![
                ("ref".to_string(), part.get("ref").cloned().unwrap_or(Value::Null)),
                ("name".to_string(), Value::str(name)),
            ]),
        ));
    }
    Some(Value::Object(entry))
}

/// All swappable slots with their factory fitting, recursively (`erkul._collect_loadout`). ⚠ The filter is
/// `kind`, not the flags; and it must not stop at a `fixed` slot: the turret is fixed, the gimbal mounts and
/// the guns in it are not.
fn collect_loadout(node: Option<&Value>, index: &[(String, &Value)], out: &mut Vec<Value>, path: &str) {
    for slot in list(node).iter().filter(|s| matches!(s, Value::Object(_))) {
        let own =
            Some(text_of(slot.get("portName"))).filter(|n| !n.is_empty()).unwrap_or_else(|| text_of(slot.get("name")));
        let full = if path.is_empty() { own.to_string() } else { format!("{path}/{own}") };
        if !own.is_empty()
            && text_of(slot.get("kind")) == "swappable"
            && let Some(entry) = one_hardpoint(slot, index, &full)
        {
            out.push(entry);
        }
        collect_loadout(slot.get("children"), index, out, &full);
    }
}

/// `erkul.fetch_ship`: one ship file, reduced to its slots. Two views of the same thing, both needed:
/// `plaetze` (two cooler slots of size 2: does my blueprint fit?) and `slots` (this slot carries ColdSnap at the
/// factory: the plan and the cart). `None` where the file is missing or no object.
pub fn fetch_ship(feed: &dyn Feed, erkul_id: &str, path: &str) -> Option<Value> {
    let raw = fetch(feed, &format!("{BRANCH}/{path}"))?;
    if !matches!(raw, Value::Object(_)) {
        return None;
    }
    let vehicle = raw.get("vehicle").filter(|v| matches!(v, Value::Object(_)));
    let hardpoints = vehicle.and_then(|v| v.get("hardpoints"));
    let mut found = Vec::new();
    collect_slots(hardpoints, &mut found);
    // Also the fitted slots: what sits in a turret brings its own weapon slots.
    collect_slots(raw.get("slots"), &mut found);
    let mut counted: std::collections::BTreeMap<(String, i64), i64> = std::collections::BTreeMap::new();
    for key in found {
        *counted.entry(key).or_insert(0) += 1;
    }
    let counts: Vec<Value> = counted
        .into_iter()
        .map(|((kind, size), n)| {
            Value::Object(vec![
                ("art".to_string(), Value::str(kind)),
                ("groesse".to_string(), Value::int(size)),
                ("anzahl".to_string(), Value::int(n)),
            ])
        })
        .collect();
    // The factory fitting stands in `slots` (as `item`, the deeper levels in `children`, not in `ports`).
    let mut index = Vec::new();
    hardpoint_index(hardpoints, &mut index, "");
    let mut slots = Vec::new();
    collect_loadout(raw.get("slots"), &index, &mut slots, "");
    let name = [vehicle.and_then(|v| v.get("vehicleDisplayName")), raw.get("i18n").and_then(|i| i.get("name"))]
        .into_iter()
        .find(|v| crate::hangar::truthy(*v))
        .map_or_else(|| Value::str(erkul_id), |v| v.cloned().unwrap_or(Value::Null));
    Some(Value::Object(vec![
        ("name".to_string(), name),
        ("plaetze".to_string(), Value::Array(counts)),
        ("slots".to_string(), Value::Array(slots)),
    ]))
}

// ------------------------------------------------------------------ the parts

/// The families of parts that are kept (`erkul.PART_FAMILIES`): racks, pods, missiles, bombs, paints, blades and
/// the components and weapons that neither a shop nor the blueprints list for ships from packages and events.
pub const PART_FAMILIES: [&str; 12] = [
    "missileracks",
    "rocketpods",
    "missiles",
    "bombs",
    "paints",
    "blades",
    "coolers",
    "powerplants",
    "shields",
    "quantumdrives",
    "radars",
    "weapons",
];

fn part_name(raw: &Value) -> String {
    [raw.get("i18n").and_then(|i| i.get("name")), raw.get("className")]
        .into_iter()
        .find(|v| crate::hangar::truthy(*v))
        .map_or_else(String::new, |v| crate::basetool::error::py_str(v.unwrap_or(&Value::Null)))
}

/// `value or ''`.
fn or_empty(value: Option<&Value>) -> Value {
    match value {
        Some(v) if crate::hangar::truthy(Some(v)) => v.clone(),
        _ => Value::str(""),
    }
}

fn tags_of(value: Option<&Value>) -> Value {
    Value::Array(
        list(value).iter().map(|x| Value::str(crate::basetool::error::py_str(x).trim_start_matches('$'))).collect(),
    )
}

/// `erkul._slim_part`: a part of a family of erkul reduced to what is needed, or `None`. A rack brings its OWN
/// slots (`plaetze`) and what sits in them at the factory (`werk`, slot name to part): after a rack swap the
/// new slots below it come from exactly that (3 × torpedo S9 become 20 × bomb S3).
pub fn slim_part(raw: &Value) -> Option<Value> {
    if !matches!(raw, Value::Object(_)) || !crate::hangar::truthy(raw.get("ref")) {
        return None;
    }
    // ⚠ erkul writes tags partly with a leading `$` (`$RSI_Mantis_Blade`), at the slot without.
    let mut part: crate::jsonio::Map = vec![
        ("ref".to_string(), raw.get("ref").cloned().unwrap_or(Value::Null)),
        ("name".to_string(), Value::str(part_name(raw))),
        ("typ".to_string(), or_empty(raw.get("type"))),
        ("untertyp".to_string(), or_empty(raw.get("subType"))),
        ("groesse".to_string(), raw.get("size").cloned().unwrap_or(Value::Null)),
        ("tags".to_string(), tags_of(raw.get("tags"))),
        ("verlangt".to_string(), tags_of(raw.get("requiredTags"))),
        ("guete".to_string(), or_empty(raw.get("grade"))),
    ];
    let mut ports = Vec::new();
    for port in list(raw.get("ports")) {
        if !matches!(port, Value::Object(_)) || !crate::hangar::truthy(port.get("name")) {
            continue;
        }
        if let Some(flags @ Value::Object(_)) = port.get("flags")
            && HIDDEN.iter().any(|f| crate::hangar::truthy(flags.get(f)))
        {
            continue;
        }
        let Some(fit) = constraints(Some(port)) else { continue };
        let takes_swappable = list(fit.get("typen")).iter().any(|pair| {
            list(Some(pair)).first().and_then(Value::as_str).is_some_and(|t| crate::erkul_slots::SWAPPABLE.contains(&t))
        });
        if takes_swappable {
            ports.push(Value::Object(vec![
                ("name".to_string(), port.get("name").cloned().unwrap_or(Value::Null)),
                ("passt".to_string(), fit),
            ]));
        }
    }
    if !ports.is_empty() {
        let mut stock: crate::jsonio::Map = Vec::new();
        for slot in list(raw.get("slots")) {
            let Some(item) = slot.get("item").filter(|i| matches!(i, Value::Object(_))) else { continue };
            if crate::hangar::truthy(item.get("ref")) && crate::hangar::truthy(slot.get("portName")) {
                let key = crate::basetool::error::py_str(slot.get("portName").unwrap_or(&Value::Null));
                let entry = Value::Object(vec![
                    ("ref".to_string(), item.get("ref").cloned().unwrap_or(Value::Null)),
                    ("name".to_string(), Value::str(part_name(item))),
                ]);
                crate::jsonio::map_set(&mut stock, &key, entry);
            }
        }
        part.push(("plaetze".to_string(), Value::Array(ports)));
        part.push(("werk".to_string(), Value::Object(stock)));
    }
    Some(Value::Object(part))
}

/// `erkul._refresh_parts`: the lists of parts, when the patch is new. Gives what is to be stored
/// (`{spielversion, teile}`), or `None` when nothing is to be: no version in the catalog, the stored one is the
/// same, a family did not come. ⚠ Only if ALL families came: a half store would look whole, and the paints
/// would be missing without a word. `stored_version` is the version of the store as it is.
pub fn refresh_parts(feed: &dyn Feed, catalog: &Value, stored_version: &str) -> Option<Value> {
    let version = match catalog.get("dataVersion") {
        Some(v) if crate::hangar::truthy(Some(v)) => crate::basetool::error::py_str(v),
        _ => String::new(),
    };
    if version.is_empty() || stored_version == version {
        return None;
    }
    let wanted: Vec<(&str, &str)> = list(catalog.get("families"))
        .iter()
        .filter(|f| matches!(f, Value::Object(_)))
        .map(|f| (text_of(f.get("kind")), text_of(f.get("path"))))
        .collect();
    let mut parts: crate::jsonio::Map = Vec::new();
    for family in PART_FAMILIES {
        let Some(path) =
            wanted.iter().rev().find(|(kind, _)| *kind == family).map(|(_, p)| *p).filter(|p| !p.is_empty())
        else {
            continue;
        };
        let Some(Value::Array(items)) = fetch(feed, &format!("{BRANCH}/{path}")) else { return None };
        for item in &items {
            if let Some(one) = slim_part(item) {
                let key = crate::basetool::error::py_str(one.get("ref").unwrap_or(&Value::Null));
                crate::jsonio::map_set(&mut parts, &key, one);
            }
        }
    }
    (!parts.is_empty()).then(|| {
        Value::Object(vec![
            ("spielversion".to_string(), Value::str(version)),
            ("teile".to_string(), Value::Object(parts)),
        ])
    })
}

// ------------------------------------------------------------------ the stores

/// The emergency limit of both stores; what counts is the game version of the catalog (`erkul.SHELF_LIFE`).
pub const SHELF_LIFE: f64 = 30.0 * crate::uex::DAY;
/// The slots of the ships (`erkul._store`), bound to the patch.
pub const SHIPS_STORE: Store =
    Store::new(crate::ships::ERKUL_FILE, crate::ships::ERKUL_FILE_FORMAT, SHELF_LIFE).bound();
/// The parts (`erkul._parts_store`), bound to the patch.
pub const PARTS_STORE: Store =
    Store::new(crate::erkul_slots::PARTS_FILE, crate::erkul_slots::PARTS_FORMAT, SHELF_LIFE).bound();

/// `erkul._refresh_parts` with the saving: the lists of parts when the catalog names a game version the store
/// does not have yet; saved only when all families came.
pub fn store_parts(paths: &Paths, feed: &dyn Feed, catalog: &Value, now: f64) {
    let stored = PARTS_STORE.load(paths);
    let stored_version = stored
        .as_ref()
        .and_then(|s| s.get("spielversion"))
        .filter(|v| crate::hangar::truthy(Some(v)))
        .map(crate::basetool::error::py_str)
        .unwrap_or_default();
    if let Some(Value::Object(fields)) = refresh_parts(feed, catalog, &stored_version) {
        let _ = PARTS_STORE.save(paths, fields, false, now);
    }
}

/// One ship to look for: as the hangar names it (`fleet.id_sets`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShipRow {
    pub name: String,
    pub maker: String,
    pub short: String,
    pub maker_short: String,
}

/// `fleet.id_sets` and `fleet.wishlist_id_sets`: the ships of the hangar with name, maker and both codes, then
/// those of the wishlist with name and maker. A wish without a maker gets the one `maker_of` names (the part of
/// the UEX name before it, `ships.manufacturer`).
pub fn ship_rows(fleet: &crate::jsonio::Map, maker_of: &dyn Fn(&str) -> String) -> Vec<ShipRow> {
    let field = |entry: &crate::jsonio::Map, key: &str| -> String {
        match entry.iter().find(|(k, _)| k == key).map(|(_, v)| v) {
            Some(v) if crate::hangar::truthy(Some(v)) => crate::basetool::error::py_str(v),
            _ => String::new(),
        }
    };
    let entries = |key: &str| -> Vec<&crate::jsonio::Map> {
        match fleet.iter().find(|(k, _)| k == key).map(|(_, v)| v) {
            Some(Value::Array(items)) => items.iter().filter_map(Value::as_object).collect(),
            _ => Vec::new(),
        }
    };
    let mut rows: Vec<ShipRow> = entries("schiffe")
        .into_iter()
        .filter(|s| !field(s, "name").is_empty())
        .map(|s| ShipRow {
            name: field(s, "name"),
            maker: field(s, "hersteller"),
            short: field(s, "kurz"),
            maker_short: field(s, "hkurz"),
        })
        .collect();
    for wish in entries("wunsch") {
        let name = field(wish, "name");
        if name.is_empty() {
            continue;
        }
        let maker = Some(field(wish, "hersteller")).filter(|m| !m.is_empty()).unwrap_or_else(|| maker_of(&name));
        rows.push(ShipRow { name, maker, ..ShipRow::default() });
    }
    rows
}

/// `fleet.unknown` for the hangar and the wishlist: whether a ship named lacks its slots. True as well when the
/// store is stale (too old, or a new game build is live): then every ship is worth asking for again. No ship
/// named: false, there is nothing to fetch.
pub fn any_unknown(paths: &Paths, rows: &[ShipRow], now: f64) -> bool {
    if rows.is_empty() {
        return false;
    }
    if SHIPS_STORE.stale(paths, now) {
        return true;
    }
    let known = crate::ships::ErkulShips::load(&paths.app_file(crate::ships::ERKUL_FILE));
    rows.iter().any(|row| {
        !known.knows(crate::ships::ShipId {
            name: &row.name,
            maker: &row.maker,
            short: &row.short,
            maker_short: &row.maker_short,
        })
    })
}

/// Which file of erkul a path names, as the error log labels it (`erkul.holen.<label>`): `katalog`, `index`,
/// `hersteller`, `teile` or `schiff`. `catalog` is the table of contents of the same run, when it came: it names
/// the index and family files exactly. Without it the name of the file decides.
pub fn fetch_label(path: &str, catalog: Option<&Value>) -> &'static str {
    let file = path.trim_start_matches('/');
    let file = file.strip_prefix(BRANCH).and_then(|f| f.strip_prefix('/')).unwrap_or(file);
    if file == "catalog.bin" {
        return "katalog";
    }
    if let Some(catalog) = catalog {
        if list(catalog.get("groups")).iter().any(|g| text_of(g.get("indexPath")) == file) {
            return "index";
        }
        if list(catalog.get("families")).iter().any(|f| text_of(f.get("path")) == file) {
            return if file.starts_with("manufacturers") { "hersteller" } else { "teile" };
        }
    }
    if file.starts_with("manufacturers") {
        "hersteller"
    } else if PART_FAMILIES.iter().any(|family| file.strip_prefix(family).is_some_and(|rest| rest.starts_with('.'))) {
        "teile"
    } else {
        "schiff"
    }
}

/// `erkul.add_missing`: fetches the slots of the ships named that are still missing and saves them. Gives how
/// many were fetched: 0 means all were there or there is no net, and both are fine: what is missing is fetched
/// the next time. A new game version in the catalog drops every stored ship (slots change with a patch).
pub fn add_missing(paths: &Paths, feed: &dyn Feed, rows: &[ShipRow], now: f64) -> usize {
    if rows.is_empty() {
        return 0;
    }
    let Some(catalog @ Value::Object(_)) = ship_catalog(feed) else { return 0 };
    store_parts(paths, feed, &catalog, now);
    let version = match catalog.get("dataVersion") {
        Some(v) if crate::hangar::truthy(Some(v)) => crate::basetool::error::py_str(v),
        _ => String::new(),
    };
    let data = SHIPS_STORE.load(paths);
    let same = data
        .as_ref()
        .and_then(|d| d.get("spielversion"))
        .is_some_and(|v| crate::jsonio::py_eq(Some(v), Some(&Value::str(version.as_str()))));
    let mut known: crate::jsonio::Map = match data.as_ref().and_then(|d| d.get("schiffe")) {
        Some(Value::Object(ships)) if same => ships.clone(),
        _ => Vec::new(),
    };
    // The table of makers is fetched once per patch, then kept with the ships.
    let makers: crate::jsonio::Map = match data.as_ref().and_then(|d| d.get("hersteller")) {
        Some(Value::Object(stored)) if same && !stored.is_empty() => stored.clone(),
        _ => maker_table(feed, &catalog).into_iter().map(|(k, v)| (k, Value::Str(v))).collect(),
    };
    // Two views of one index: the slim key for the letter comparison, the original code for the word-by-word
    // search, which needs the word boundaries.
    let mut listing: Vec<(String, String)> = Vec::new();
    let mut raw_ids: Vec<(String, String)> = Vec::new();
    for group in list(catalog.get("groups")) {
        let path = text_of(group.get("indexPath"));
        if path.is_empty() {
            continue;
        }
        let index = fetch(feed, &format!("{BRANCH}/{path}"));
        for entry in list(index.as_ref().and_then(|i| i.get("blobs"))) {
            let (id, file) = (text_of(entry.get("id")), text_of(entry.get("path")));
            if id.is_empty() || file.is_empty() {
                continue;
            }
            for (map, key) in [(&mut listing, crate::ships::slim_name(id)), (&mut raw_ids, id.to_string())] {
                match map.iter_mut().find(|(k, _)| *k == key) {
                    Some(found) => found.1 = file.to_string(),
                    None => map.push((key, file.to_string())),
                }
            }
        }
    }
    let codes: Vec<String> = raw_ids.iter().map(|(id, _)| id.clone()).collect();
    let mut fetched = 0;
    for row in rows {
        let maker_short = if row.maker_short.is_empty() {
            let maker = crate::ships::slim_name(&row.maker);
            makers.iter().find(|(k, _)| *k == maker).map(|(_, v)| crate::basetool::error::py_str(v)).unwrap_or_default()
        } else {
            row.maker_short.clone()
        };
        let id =
            crate::ships::ShipId { name: &row.name, maker: &row.maker, short: &row.short, maker_short: &maker_short };
        let possible = crate::ships::erkul_candidates(id);
        if possible.iter().any(|k| known.iter().any(|(key, _)| key == k)) {
            continue;
        }
        let (code, file) = match possible.iter().find_map(|k| listing.iter().find(|(key, _)| key == k)) {
            Some((key, file)) => (key.clone(), file.clone()),
            None => {
                let Some(code) = crate::ships::erkul_code_wordwise(&codes, &row.name, &row.maker, &maker_short) else {
                    continue;
                };
                let file = raw_ids.iter().find(|(id, _)| *id == code).map(|(_, f)| f.clone()).unwrap_or_default();
                (code, file)
            }
        };
        // Stored always under the slim key, so both ways are looked up alike.
        let key = crate::ships::slim_name(&code);
        if known.iter().any(|(k, _)| *k == key) {
            continue;
        }
        if let Some(Value::Object(mut one)) = fetch_ship(feed, &key, &file) {
            one.push(("id".to_string(), Value::str(code.as_str())));
            known.push((key, Value::Object(one)));
            fetched += 1;
        }
    }
    if fetched > 0 || !same {
        let fields = vec![
            ("spielversion".to_string(), Value::str(version.as_str())),
            ("schiffe".to_string(), Value::Object(known)),
            ("hersteller".to_string(), Value::Object(makers)),
        ];
        let _ = SHIPS_STORE.save(paths, fields, false, now);
    }
    fetched
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    fn packed(json: &str) -> Vec<u8> {
        miniz_oxide::deflate::compress_to_vec(json.as_bytes(), 6)
    }

    /// Stored answers by path; every path asked for is written down.
    struct Stored {
        files: Vec<(&'static str, Vec<u8>)>,
        asked: RefCell<Vec<String>>,
    }

    impl Feed for Stored {
        fn get(&self, path: &str) -> Option<Vec<u8>> {
            self.asked.borrow_mut().push(path.to_string());
            self.files.iter().find(|(p, _)| *p == path).map(|(_, bytes)| bytes.clone())
        }
    }

    #[test]
    fn a_packed_file_is_read_and_what_is_no_file_is_none() {
        let raw = packed(r#"{"version": "4.1", "families": []}"#);
        assert_eq!(
            unpack(&raw).and_then(|v| v.get("version").and_then(Value::as_str).map(str::to_string)),
            Some("4.1".into())
        );
        assert!(unpack(b"not packed at all").is_none());
        assert!(unpack(&packed("not json")).is_none());
        // With a zlib header the raw reading fails, as in Python without `-15`.
        let with_header = miniz_oxide::deflate::compress_to_vec_zlib(b"{}", 6);
        assert!(unpack(&with_header).is_none());
    }

    #[test]
    fn the_catalog_and_the_table_of_makers_come_from_stored_answers_and_nothing_else_is_asked() {
        let catalog = r#"{"families": [{"path": "ships.abc.bin"}, {"path": "manufacturers.def.bin"}]}"#;
        let makers = r#"[{"className": "ANVL", "i18n": {"name": "Anvil Aerospace"}},
            {"className": "AEGS", "i18n": {"name": " Aegis Dynamics "}},
            {"className": "", "i18n": {"name": "No Code"}}, {"className": "X", "i18n": {}}]"#;
        let feed = Stored {
            files: vec![("LIVE/catalog.bin", packed(catalog)), ("LIVE/manufacturers.def.bin", packed(makers))],
            asked: RefCell::new(Vec::new()),
        };
        let cat = ship_catalog(&feed).expect("catalog");
        let table = maker_table(&feed, &cat);
        assert_eq!(
            table,
            vec![("anvilaerospace".to_string(), "ANVL".to_string()), ("aegisdynamics".to_string(), "AEGS".to_string())]
        );
        assert_eq!(*feed.asked.borrow(), ["LIVE/catalog.bin", "LIVE/manufacturers.def.bin"]);
        // No net: nothing, not an error.
        let none = FnFeed(|_: &str| None);
        assert!(ship_catalog(&none).is_none());
        assert!(maker_table(&none, &cat).is_empty());
        // A catalog without the family: no second request.
        let bare = Stored { files: vec![], asked: RefCell::new(Vec::new()) };
        assert!(maker_table(&bare, &jsonio::loads(r#"{"families": [{"path": "ships.abc.bin"}]}"#).unwrap()).is_empty());
        assert!(bare.asked.borrow().is_empty());
        assert_eq!(address("/LIVE/catalog.bin"), "https://cdn.erkul.games/LIVE/catalog.bin");
    }

    fn paths_in(name: &str) -> (Paths, std::path::PathBuf) {
        let folder = std::env::temp_dir().join(format!("versekit-erkul-{name}-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let home = folder.to_string_lossy().into_owned();
        let host = crate::paths::Host::new(crate::pypath::Flavor::native(), &[(crate::legacy::ENV_HOME, &home)], &home);
        (Paths::new(host), folder)
    }

    /// erkul in small: a catalog of `version`, the makers, one index with three ships, two ship files, the paints.
    fn erkul_files(version: &str) -> Stored {
        let catalog = format!(
            r#"{{"dataVersion": "{version}", "groups": [{{"indexPath": "ships.i1.bin"}}, {{"name": "no index"}}],
                "families": [{{"kind": "manufacturers", "path": "manufacturers.m1.bin"}},
                             {{"kind": "paints", "path": "paints.p1.bin"}}]}}"#
        );
        let ship = |name: &str, kind: &str, size: i64| {
            format!(
                r#"{{"vehicle": {{"vehicleDisplayName": "{name}", "hardpoints": [
                    {{"name": "hardpoint_one", "accepts": [{{"type": "{kind}"}}], "maxSize": {size}}},
                    {{"name": "hardpoint_door", "accepts": [{{"type": "Door"}}], "maxSize": 1}}]}}, "slots": []}}"#
            )
        };
        let files: Vec<(&'static str, Vec<u8>)> = vec![
            ("LIVE/catalog.bin", packed(&catalog)),
            ("LIVE/manufacturers.m1.bin", packed(r#"[{"className": "ANVL", "i18n": {"name": "Anvil Aerospace"}}]"#)),
            ("LIVE/paints.p1.bin", packed(r#"[{"ref": "p1", "className": "Paint_Red", "type": "Paints", "size": 1}]"#)),
            (
                "LIVE/ships.i1.bin",
                packed(
                    r#"{"blobs": [{"id": "anvl_arrow", "path": "ships/arrow.bin"},
                    {"id": "drak_ironclad_assault", "path": "ships/ironclad.bin"}, {"id": "", "path": "x.bin"},
                    {"id": "misc_prospector", "path": "ships/prospector.bin"}]}"#,
                ),
            ),
            ("LIVE/ships/arrow.bin", packed(&ship("Anvil Arrow", "Cooler", 1))),
            ("LIVE/ships/ironclad.bin", packed(&ship("Ironclad Assault", "PowerPlant", 3))),
        ];
        Stored { files, asked: RefCell::new(Vec::new()) }
    }

    fn row(name: &str, maker: &str) -> ShipRow {
        ShipRow { name: name.into(), maker: maker.into(), ..ShipRow::default() }
    }

    #[test]
    fn the_missing_ships_are_found_by_code_or_word_by_word_fetched_and_stored_once() {
        let (paths, folder) = paths_in("add-missing");
        let rows =
            [row("Arrow", "Anvil Aerospace"), row("Ironclad Assault", "Drake Interplanetary"), row("Galaxy", "RSI")];
        let feed = erkul_files("4.3.0");
        // The Arrow by the maker table (`anvl`), the Ironclad word by word; erkul does not know the Galaxy.
        assert_eq!(add_missing(&paths, &feed, &rows, 1000.0), 2);
        assert!(!feed.asked.borrow().iter().any(|p| p.contains("prospector")), "no ship that was not asked for");
        let stored = SHIPS_STORE.load(&paths).expect("stored");
        let ship = |key: &str| stored.get("schiffe").and_then(|s| s.get(key)).cloned().expect(key);
        assert_eq!(ship("anvlarrow").get("id").and_then(Value::as_str), Some("anvlarrow"));
        assert_eq!(ship("drakironcladassault").get("id").and_then(Value::as_str), Some("drak_ironclad_assault"));
        assert_eq!(stored.get("spielversion").and_then(Value::as_str), Some("4.3.0"));
        assert_eq!(
            stored.get("hersteller").and_then(|h| h.get("anvilaerospace")).and_then(Value::as_str),
            Some("ANVL")
        );
        // What the pages read from it: the slots of both, the door counts not.
        let ships = crate::ships::ErkulShips::load(&paths.app_file(crate::ships::ERKUL_FILE));
        let id = |name, maker| crate::ships::ShipId { name, maker, short: "", maker_short: "" };
        assert_eq!(ships.slot_count(id("Arrow", "Anvil Aerospace")), Some(1));
        assert_eq!(ships.slot_count(id("Ironclad Assault", "Drake Interplanetary")), Some(1));
        let parts = PARTS_STORE.load(&paths).expect("parts");
        assert!(parts.get("teile").and_then(|t| t.get("p1")).is_some());

        // Asked again: nothing is fetched, neither ships nor makers nor parts; only catalog and index.
        let again = erkul_files("4.3.0");
        assert_eq!(add_missing(&paths, &again, &rows, 2000.0), 0);
        assert_eq!(*again.asked.borrow(), ["LIVE/catalog.bin", "LIVE/ships.i1.bin"]);

        // A new patch: everything stored counts no more and is fetched again.
        let patched = erkul_files("4.4.0");
        assert_eq!(add_missing(&paths, &patched, &rows, 3000.0), 2);
        assert_eq!(SHIPS_STORE.load(&paths).and_then(|s| s.get("spielversion").cloned()), Some(Value::str("4.4.0")));
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn a_ship_is_unknown_until_its_slots_are_stored_and_again_after_a_patch() {
        let (paths, folder) = paths_in("any-unknown");
        let build = |live: &str| {
            std::fs::write(
                folder.join("spielstand.json"),
                format!(r#"{{"live":"{live}","ptu":"","format":1,"geholt":1.0}}"#),
            )
            .unwrap();
        };
        build("4.3.0");
        let rows = [row("Arrow", "Anvil Aerospace"), row("Ironclad Assault", "Drake Interplanetary")];
        assert!(!any_unknown(&paths, &[], 1000.0), "no ship, nothing to fetch");
        assert!(any_unknown(&paths, &rows, 1000.0), "no store yet");
        assert_eq!(add_missing(&paths, &erkul_files("4.3.0"), &rows, 1000.0), 2);
        assert!(!any_unknown(&paths, &rows, 1001.0), "both stored");
        assert!(any_unknown(&paths, &[row("Arrow", "Anvil Aerospace"), row("Galaxy", "RSI")], 1001.0));
        // A new game build makes the store stale: every ship counts as unknown again.
        build("4.4.0");
        assert!(any_unknown(&paths, &rows, 1001.0), "a new patch asks again");
        // Too old counts too.
        build("4.3.0");
        assert!(any_unknown(&paths, &rows, 1000.0 + SHELF_LIFE + 1.0));
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn every_file_of_a_run_gets_the_label_of_its_kind() {
        let catalog = jsonio::loads(
            r#"{"groups": [{"indexPath": "ships.i1.bin"}],
                "families": [{"kind": "manufacturers", "path": "manufacturers.m1.bin"},
                             {"kind": "paints", "path": "paints.p1.bin"}]}"#,
        )
        .unwrap();
        let with = |path: &str| fetch_label(path, Some(&catalog));
        assert_eq!(with("LIVE/catalog.bin"), "katalog");
        assert_eq!(with("LIVE/ships.i1.bin"), "index");
        assert_eq!(with("LIVE/manufacturers.m1.bin"), "hersteller");
        assert_eq!(with("LIVE/paints.p1.bin"), "teile");
        assert_eq!(with("LIVE/ships/arrow.bin"), "schiff");
        // Without the catalog the name decides; what is no catalog, no maker and no family is a ship.
        assert_eq!(fetch_label("/LIVE/catalog.bin", None), "katalog");
        assert_eq!(fetch_label("LIVE/manufacturers.x.bin", None), "hersteller");
        assert_eq!(fetch_label("LIVE/missiles.ab12.bin", None), "teile");
        assert_eq!(fetch_label("LIVE/missilesx.bin", None), "schiff");
        assert_eq!(fetch_label("LIVE/ships/arrow.bin", None), "schiff");
    }

    #[test]
    fn without_net_or_without_ships_nothing_is_fetched_and_nothing_written() {
        let (paths, folder) = paths_in("add-missing-off");
        let none = FnFeed(|_: &str| None);
        assert_eq!(add_missing(&paths, &none, &[row("Arrow", "Anvil Aerospace")], 1000.0), 0);
        let feed = erkul_files("4.3.0");
        assert_eq!(add_missing(&paths, &feed, &[], 1000.0), 0);
        assert!(feed.asked.borrow().is_empty(), "no ship, no request");
        assert!(SHIPS_STORE.load(&paths).is_none() && PARTS_STORE.load(&paths).is_none());
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn the_rows_are_the_hangar_with_its_codes_then_the_wishlist_with_a_maker() {
        let fleet = jsonio::loads(
            r#"{"schiffe": [{"name": "Arrow", "hersteller": "Anvil Aerospace", "kurz": "ANVL_Arrow", "hkurz": "ANVL"},
                            {"name": ""}, {"hersteller": "x"}],
                "wunsch": [{"name": "Polaris"}, {"name": "Carrack", "hersteller": "Anvil"}, {"name": null}]}"#,
        )
        .unwrap();
        let Value::Object(fleet) = fleet else { panic!() };
        let rows = ship_rows(&fleet, &|name| if name == "Polaris" { "RSI".into() } else { String::new() });
        assert_eq!(
            rows,
            [
                ShipRow {
                    name: "Arrow".into(),
                    maker: "Anvil Aerospace".into(),
                    short: "ANVL_Arrow".into(),
                    maker_short: "ANVL".into()
                },
                row("Polaris", "RSI"),
                row("Carrack", "Anvil"),
            ]
        );
    }
}
