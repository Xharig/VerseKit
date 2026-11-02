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

//! The recipe file `crafting-blueprints.json`: the recipes of the current build from scmdb, the materials that a
//! dismantling does not give back and the base values of the products. Port of `crafting.update` and
//! `product_stats.compact`.
//!
//! The readers (the notes in the contract texts, the tags for the Basetool, the materials of the stock) take the
//! file as it is; the known data errors of CIG are applied by them in memory, never written into it. The list of
//! those errors for the build ([`ISSUES_FILE`]) is fetched with the recipes and kept beside them. The name, the
//! format number and the keys stay as they are, or every file already there counts as outdated.
//!
//! Sparing: with the same build, recipes, dismantling and products already there, nothing is fetched (the files
//! are 4 and 1 MB). Nothing of scmdb is shipped: the files are fetched at run time on the user's computer.

use crate::errors;
use crate::jsonio::{self, Map, Value};
use crate::paths::Paths;

/// The name of the file in the data folder.
pub const FILE: &str = "crafting-blueprints.json";
/// The format number of the file; the readers ignore any other.
pub const FORMAT: i64 = 1;

/// Fields of a product that show nothing and only make the file bigger.
const DROP: [&str; 6] =
    ["acceptedResources", "acceptedResourceGroups", "knownIssue", "tags", "shieldResistance", "shieldAbsorption"];

/// Reference field, shared table of the item file and the field the resolved entry goes to.
const POOLS: [(&str, &str, &str); 5] = [
    ("fireModesIndex", "fireModesPools", "fireModes"),
    ("ammoIndex", "ammoPools", "ammo"),
    ("magazineIndex", "magazinePools", "magazine"),
    ("damageResistanceIndex", "damageResistancePools", "damageResistance"),
    ("signaturesIndex", "signaturesPools", "signatures"),
];

/// Where the files of scmdb come from: the net at run time, a stand-in in the checks.
pub trait Supply: Send + Sync {
    /// The file `name` of scmdb (e.g. `versions.json`).
    ///
    /// # Errors
    ///
    /// Why it could not be fetched.
    fn file(&self, name: &str) -> Result<Vec<u8>, String>;
}

/// scmdb: its mirror first, then scmdb.net (`scmdb::fetch_file`).
pub struct Scmdb {
    pub user_agent: String,
}

impl Supply for Scmdb {
    fn file(&self, name: &str) -> Result<Vec<u8>, String> {
        crate::scmdb::fetch_file(name, &self.user_agent).map_err(|error| error.to_string())
    }
}

/// What came of [`update`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Updated {
    /// The file was current already, with this many recipes; nothing was fetched.
    Current(usize),
    /// The recipes were fetched and written, this many.
    Written(usize),
    /// The file was current already, with this many recipes, but the list of the known data errors of its build
    /// changed: the readers correct them anew.
    Corrected(usize),
}

/// The list of the known data errors of CIG for one build (`cig_issues.py`), kept beside the recipe file.
pub const ISSUES_FILE: &str = "cig-datenfehler.json";
/// The format number of [`ISSUES_FILE`].
pub const ISSUES_FORMAT: i64 = 1;
/// Without a list for the current build scmdb is asked again only after this many seconds.
pub const ISSUES_RETRY_SECONDS: f64 = 6.0 * 3600.0;
/// The sections of the list whose entries count.
const ISSUE_SECTIONS: [&str; 4] = ["blueprintRecords", "blueprintPools", "entities", "contracts"];

/// The name of the list of `build` at scmdb.
pub fn issues_source(build: &str) -> String {
    format!("cig_data_issues-{build}.json")
}

/// The stored list, only in the format of [`ISSUES_FORMAT`] with an object as `daten`.
fn stored_issues(paths: &Paths) -> Option<Value> {
    jsonio::read_json(&paths.app_file(ISSUES_FILE))
        .ok()
        .filter(|data| data.get("format") == Some(&Value::int(ISSUES_FORMAT)))
        .filter(|data| matches!(data.get("daten"), Some(Value::Object(_))))
}

/// A value with the keys of every object in sorted order, so that two lists compare by content.
fn sorted(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut map: Map = map.iter().map(|(key, item)| (key.clone(), sorted(item))).collect();
            map.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(map)
        }
        Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
        other => other.clone(),
    }
}

/// The entries of the four sections without comments (`_…`) and without what is no object, the keys in lower case
/// except for the contracts (`cig_issues.fingerprint`).
fn issue_entries(issues: Option<&Value>) -> Vec<(&'static str, Value)> {
    ISSUE_SECTIONS
        .iter()
        .map(|section| {
            let entries = match issues.and_then(|issues| issues.get(section)) {
                Some(Value::Object(block)) => block
                    .iter()
                    .filter(|(key, value)| !key.starts_with('_') && matches!(value, Value::Object(_)))
                    .map(|(key, value)| {
                        (if *section == "contracts" { key.clone() } else { key.to_lowercase() }, value.clone())
                    })
                    .collect(),
                _ => Map::new(),
            };
            (*section, sorted(&Value::Object(entries)))
        })
        .collect()
}

/// `cig_issues.update`: fetch the list of the known data errors of `build` when it is missing or another build's.
/// A list that could not be had is kept as an empty one (or as the one of the same build there was) and asked for
/// again after [`ISSUES_RETRY_SECONDS`]. Whether the entries for `build` changed; a failed write is recorded and
/// counts as no change.
pub fn update_issues(paths: &Paths, supply: &dyn Supply, build: &str, now: f64) -> bool {
    if build.is_empty() {
        return false;
    }
    let old = stored_issues(paths);
    let same_build = old.as_ref().and_then(|data| text(data.get("build"))) == Some(build);
    let old_issues = old.as_ref().filter(|_| same_build).and_then(|data| data.get("daten"));
    let fetched_at =
        old.as_ref().and_then(|data| crate::stock_data::py_float_value(data.get("geholt")?)).unwrap_or(0.0);
    let has_entries = old_issues.and_then(Value::as_object).is_some_and(|map| !map.is_empty());
    if same_build && (has_entries || now - fetched_at < ISSUES_RETRY_SECONDS) {
        return false;
    }
    let fetched = match parse(supply, &issues_source(build)) {
        Ok(Value::Object(map)) => Some(Value::Object(map)),
        Ok(_) => None,
        Err(error) => {
            errors::record("cig_issues.update", &error);
            None
        }
    };
    let issues = fetched.or_else(|| old_issues.cloned()).unwrap_or_else(|| Value::Object(Map::new()));
    let before = issue_entries(old_issues);
    let data = Value::Object(vec![
        ("format".to_string(), Value::int(ISSUES_FORMAT)),
        ("build".to_string(), Value::str(build)),
        ("geholt".to_string(), Value::Float(now)),
        ("daten".to_string(), issues),
    ]);
    let target = paths.app_file(ISSUES_FILE);
    if let Err(error) = jsonio::write_text_atomic(&target, &jsonio::dumps(&data, None, false), jsonio::Newline::Keep) {
        if !crate::writeguard::refused(&error) {
            errors::record("cig_issues._save", &error);
        }
        return false;
    }
    issue_entries(data.get("daten")) != before
}

fn parse(supply: &dyn Supply, name: &str) -> Result<Value, String> {
    let bytes = supply.file(name)?;
    let text = String::from_utf8(bytes).map_err(|error| format!("{name}: {error}"))?;
    jsonio::loads(&text).map_err(|error| format!("{name}: {error:?}"))
}

fn list(value: Option<&Value>) -> &[Value] {
    match value {
        Some(Value::Array(items)) => items,
        _ => &[],
    }
}

fn text(value: Option<&Value>) -> Option<&str> {
    value.and_then(Value::as_str).filter(|text| !text.is_empty())
}

/// scmdb's current build of the LIVE channel: the first entry of `versions.json` that is no PTU.
///
/// # Errors
///
/// The fetch error, or when the list names none.
pub fn current_build(supply: &dyn Supply) -> Result<String, String> {
    let versions = parse(supply, "versions.json")?;
    list(Some(&versions))
        .iter()
        .filter_map(|entry| text(entry.get("version")))
        .find(|name| !name.to_ascii_lowercase().contains("ptu"))
        .map(str::to_string)
        .ok_or_else(|| "versions.json: no build".to_string())
}

/// `product_stats.compact`: of the items of the raw item file only those a recipe makes (its product, or the one it
/// was meant to make where CIG swapped it), without the fields of [`DROP`], the references into the shared tables
/// resolved. Keyed by the entity class.
pub fn compact(raw_items: &Value, blueprints: &[Value]) -> Map {
    let wanted: std::collections::HashSet<&str> = blueprints
        .iter()
        .flat_map(|blueprint| {
            ["productEntityClass", "suggestedProductEntityClass"].into_iter().filter_map(|key| text(blueprint.get(key)))
        })
        .collect();
    let mut result = Map::new();
    for item in list(raw_items.get("items")) {
        let Some(ident) = text(item.get("entityClass")).filter(|ident| wanted.contains(ident)) else { continue };
        let Some(fields) = item.as_object() else { continue };
        let mut entry: Map = fields.iter().filter(|(key, _)| !DROP.contains(&key.as_str())).cloned().collect();
        for (index_key, pool_key, target) in POOLS {
            let Some(at) = entry.iter().position(|(key, _)| key == index_key) else { continue };
            let (_, index) = entry.remove(at);
            let pool = list(raw_items.get(pool_key));
            if let Value::Int(number) = &index
                && let Ok(n) = number.parse::<usize>()
                && let Some(resolved) = pool.get(n)
            {
                jsonio::map_set(&mut entry, target, resolved.clone());
            }
        }
        jsonio::map_set(&mut result, ident, Value::Object(entry));
    }
    result
}

/// The file as it stands, only in the format of [`FORMAT`].
fn stored(paths: &Paths) -> Option<Value> {
    jsonio::read_json(&paths.app_file(FILE)).ok().filter(|data| data.get("format") == Some(&Value::int(FORMAT)))
}

/// The key that names the build whose products could not be fetched.
pub const WITHOUT_PRODUCTS: &str = "ohne_produkte";

/// Whether the file holds the recipes of `build` whole: recipes, the dismantling and the products, or the mark that
/// the products of this build could not be had. A file of an older writing without dismantling or products is
/// fetched once more.
fn current_for(data: &Value, build: &str) -> Option<usize> {
    let recipes = list(data.get("blueprints")).len();
    let dismantle = data.get("dismantle").is_some_and(|value| *value != Value::Null);
    let products = data.get("products").and_then(Value::as_object).is_some_and(|products| !products.is_empty())
        || text(data.get(WITHOUT_PRODUCTS)) == Some(build);
    (text(data.get("build")) == Some(build) && recipes > 0 && dismantle && products).then_some(recipes)
}

/// `crafting.update`: fetch the recipes of the current build when the file lacks them, or holds another build.
/// The products come from a fetch of their own: when it fails the recipes are written anyway with the products the
/// file held before (none without a file), and the build is marked as one without products, so it counts as
/// current until the next build. Nothing with `SC_BP_NO_NET`.
///
/// # Errors
///
/// Why nothing was written; the file stays as it was.
pub fn update(paths: &Paths, supply: &dyn Supply) -> Result<Updated, String> {
    if crate::net::offline_in(&paths.host) {
        return Err("offline".to_string());
    }
    let build = current_build(supply)?;
    update_for(paths, supply, &build)
}

/// [`update`] for the `build` already asked for: first the list of the known data errors of the build
/// ([`update_issues`]), with which the readers correct what they read, then the recipes.
///
/// # Errors
///
/// Why the recipes were not written; the file stays as it was.
pub fn update_for(paths: &Paths, supply: &dyn Supply, build: &str) -> Result<Updated, String> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
    let corrected = update_issues(paths, supply, build, now);
    let build = build.to_string();
    let before = stored(paths);
    if let Some(recipes) = before.as_ref().and_then(|data| current_for(data, &build)) {
        return Ok(if corrected { Updated::Corrected(recipes) } else { Updated::Current(recipes) });
    }
    let raw = parse(supply, &format!("crafting_blueprints-{build}.json"))?;
    let blueprints = list(raw.get("blueprints")).to_vec();
    if blueprints.is_empty() {
        return Err(format!("crafting_blueprints-{build}.json: no recipes"));
    }
    let (products, without) = match parse(supply, &format!("crafting_items-{build}.json")) {
        Ok(items) => (compact(&items, &blueprints), false),
        Err(error) => {
            errors::record("rezepte.produkte", &error);
            let kept = before.as_ref().and_then(|data| data.get("products")).and_then(Value::as_object);
            (kept.cloned().unwrap_or_default(), true)
        }
    };
    let dismantle = match raw.get("dismantle") {
        Some(value @ Value::Object(map)) if !map.is_empty() => value.clone(),
        _ => Value::Object(Map::new()),
    };
    let count = blueprints.len();
    let mut fields = vec![
        ("format".to_string(), Value::int(FORMAT)),
        ("build".to_string(), Value::str(&build)),
        ("blueprints".to_string(), Value::Array(blueprints)),
        ("dismantle".to_string(), dismantle),
        ("products".to_string(), Value::Object(products)),
    ];
    if without {
        fields.push((WITHOUT_PRODUCTS.to_string(), Value::str(&build)));
    }
    let data = Value::Object(fields);
    let target = paths.app_file(FILE);
    jsonio::write_text_atomic(&target, &jsonio::dumps(&data, None, false), jsonio::Newline::Native)
        .map_err(|error| format!("{target}: {error}"))?;
    Ok(Updated::Written(count))
}

#[cfg(test)]
mod tests;
