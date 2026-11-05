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

//! Rust side of the count of the stock lines in the Game.log (`tools/parity_lagerlog.py`).
//!
//! The Python side counts the logs with patterns of its own. The purchases, the moves and the places come from
//! `stock_journal::parse_text` and `location::learn_pairs`; only the raw line counts (requests, answers, queued
//! and completed lines by kind) are counted here from the text, the way the Python side counts them.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Value, json};
use versekit_core::location;
use versekit_core::stock_journal::{Endpoint, Event, Kind, Movement, parse_text};

static QUEUED_KIND: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Queued Request\[\d+\] Type\[(\w+)\]").expect("pattern"));
static COMPLETE_KIND: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<Player Inventory Request Complete> Request\[\d+\].*?Type\[(\w+)\] Result\[(\w+)\]").expect("pattern")
});
static ANSWER_TYPE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"type\[(\w+)\]").expect("pattern"));

fn end_name(end: &Endpoint) -> String {
    match end {
        Endpoint::Nowhere => "kein".into(),
        Endpoint::Container => "behaelter".into(),
        Endpoint::Location(number) => format!("ort:{number}"),
        Endpoint::Other => "anderes".into(),
    }
}

fn ok_name(ok: Option<bool>) -> &'static str {
    match ok {
        Some(true) => "True",
        Some(false) => "False",
        None => "None",
    }
}

fn move_line(m: &Movement) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}",
        m.time,
        m.kind.name(),
        m.class,
        end_name(&m.from),
        end_name(&m.to),
        m.amount,
        ok_name(m.ok)
    )
}

/// The counts of one log.
fn count_log(path: &str, gegenprobe: bool) -> Result<(Value, BTreeSet<u64>, Vec<Event>), String> {
    let bytes = fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let text = String::from_utf8_lossy(&bytes);
    let events = parse_text(&text, None);

    let mut asked = [0u64; 2];
    let mut answered = [0u64; 2];
    let mut sells = 0u64;
    let mut landings = BTreeSet::new();
    let mut raw_queued: BTreeMap<String, u64> = BTreeMap::new();
    let mut raw_done: BTreeMap<String, [u64; 2]> = BTreeMap::new();
    for line in text.split(['\n', '\r']) {
        if line.contains("SellRequest") {
            sells += 1;
        }
        if line.contains("<Update Inventory Location>")
            && let Some((_, now)) = location::change_in(line)
            && now != location::NOWHERE
            && let Ok(number) = now.parse()
        {
            landings.insert(number);
        }
        if line.contains("SendShopBuyRequest>") {
            asked[0] += 1;
        } else if line.contains("SendStandardItemBuyRequest>") {
            asked[1] += 1;
        } else if line.contains("RmShopFlowResponse>") {
            if ANSWER_TYPE.captures(line).is_some_and(|t| &t[1] != "Buying") {
                continue;
            }
            answered[usize::from(line.contains("ShoppingProvider::RmShopFlowResponse>"))] += 1;
        } else if (line.contains("<InventoryManagementRequest>") || line.contains("<Inventory Mgmt Request Queued>"))
            && let Some(c) = QUEUED_KIND.captures(line)
            && Kind::ALL.iter().any(|k| k.name() == &c[1])
        {
            *raw_queued.entry(c[1].to_string()).or_default() += 1;
        } else if line.contains("<Player Inventory Request Complete>")
            && let Some(c) = COMPLETE_KIND.captures(line)
            && Kind::ALL.iter().any(|k| k.name() == &c[1])
        {
            raw_done.entry(c[1].to_string()).or_default()[usize::from(&c[2] != "Succeed")] += 1;
        }
    }

    let purchases: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Purchase(p) => Some(p),
            Event::Sale(_) | Event::Movement(_) => None,
        })
        .collect();
    let moves: Vec<&Movement> = events
        .iter()
        .filter_map(|e| match e {
            Event::Movement(m) => Some(m),
            Event::Purchase(_) | Event::Sale(_) => None,
        })
        .collect();
    let mut classes: Vec<&str> = purchases.iter().map(|p| p.class.as_str()).collect();
    classes.sort_unstable();
    classes.dedup();
    let mut forms: BTreeMap<String, [u64; 4]> = BTreeMap::new();
    for m in &moves {
        let row = forms.entry(format!("{} {}>{}", m.kind.name(), end_name(&m.from), end_name(&m.to))).or_default();
        row[0] += 1;
        row[match m.ok {
            Some(true) => 1,
            Some(false) => 2,
            None => 3,
        }] += 1;
    }
    let mut lines: Vec<String> = moves.iter().map(|m| move_line(m)).collect();
    lines.sort_unstable();
    let mut move_classes: Vec<&str> = moves.iter().map(|m| m.class.as_str()).filter(|c| !c.is_empty()).collect();
    move_classes.sort_unstable();
    move_classes.dedup();
    let mut price_micro: i64 = purchases.iter().map(|p| (p.price_total * 1e6).round() as i64).sum();
    let mut move_count = moves.len();
    if gegenprobe {
        price_micro += 1_000_000;
        move_count += 1;
    }
    let counted = json!({
        "datei": path,
        "anfragen": asked,
        "antworten": answered,
        "verkauf": sells,
        "gebucht": purchases.len(),
        "preis_mikro": price_micro,
        "menge": purchases.iter().map(|p| u64::from(p.quantity)).sum::<u64>(),
        "klassen": classes,
        "kauf_orte": purchases.iter().map(|p| p.place.clone().unwrap_or_else(|| "0".into())).collect::<Vec<_>>(),
        "kauf_zeiten": purchases.iter().map(|p| p.time).collect::<Vec<_>>(),
        "formen": forms,
        "bewegungen": move_count,
        "bewegungen_zeilen": lines,
        "bewegungen_klassen": move_classes,
        "roh_queued": raw_queued,
        "roh_abschluss": raw_done,
    });
    Ok((counted, landings, events))
}

/// Tells how many items and places the stored names resolve, from the data folder named by `VERSEKIT_INTERN`
/// (the `Intern` folder of a test copy); reads only. Nothing is compared, there is no Python side for it.
fn coverage(folder: &str, events: &[Event], landings: &BTreeSet<u64>, learned: &location::Pairs) {
    use versekit_core::legacy;
    use versekit_core::paths::{Host, Paths};
    use versekit_core::pypath::Flavor;
    use versekit_core::stock_journal::names::{Items, PlaceName, Places};

    let paths = Paths::new(Host::new(Flavor::native(), &[(legacy::ENV_HOME, folder)], folder));
    let items = Items::load(&paths);
    let places = Places::load(&paths).with_learned(learned.iter().cloned().collect());
    let (mut bought, mut bought_named, mut moved, mut moved_named) = (0, 0, 0, 0);
    let mut classes: BTreeMap<(bool, String), bool> = BTreeMap::new();
    for event in events {
        let (class, guid, is_purchase) = match event {
            Event::Purchase(p) => (p.class.as_str(), p.guid.as_str(), true),
            Event::Movement(m) if !m.class.is_empty() => (m.class.as_str(), "", false),
            Event::Movement(_) | Event::Sale(_) => continue,
        };
        let named = items.item(class, guid).adoptable();
        classes.insert((is_purchase, class.to_string()), named);
        if is_purchase {
            bought += 1;
            bought_named += usize::from(named);
        } else {
            moved += 1;
            moved_named += usize::from(named);
        }
    }
    let count =
        |purchase: bool, named: bool| classes.iter().filter(|((p, _), n)| *p == purchase && **n == named).count();
    println!(
        "Namen: Käufe {bought_named}/{bought} benannt ({} von {} Klassen), Bewegungen {moved_named}/{moved} ({} von {} Klassen)",
        count(true, true),
        count(true, true) + count(true, false),
        count(false, true),
        count(false, true) + count(false, false)
    );
    let (mut listed, mut shown, mut unknown) = (0, 0, 0);
    for number in landings {
        match places.place(&number.to_string()) {
            PlaceName::Listed(_) => listed += 1,
            PlaceName::Shown(_) => shown += 1,
            PlaceName::Unknown(_) => unknown += 1,
        }
    }
    println!("Orte: {listed} in der Liste, {shown} nur angezeigt, {unknown} ohne Namen (von {})", landings.len());
}

/// `gegenprobe`: raises the price sum and the number of moves of the first log and takes one place and one
/// name away, so the comparison must report all of it.
pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let text = fs::read(python_json).map_err(|e| format!("{python_json}: {e}"))?;
    let input: Value = serde_json::from_slice(&text).map_err(|e| format!("{python_json}: {e}"))?;
    let mut files = Vec::new();
    for entry in input["dateien"].as_array().ok_or("dateien fehlt")? {
        files.push(entry["datei"].as_str().ok_or("datei fehlt")?.to_string());
    }
    let mut counted = Vec::new();
    let mut landings = BTreeSet::new();
    let mut all_events = Vec::new();
    for (index, path) in files.iter().enumerate() {
        let (one, found, events) = count_log(path, gegenprobe && index == 0)?;
        counted.push(one);
        landings.extend(found);
        all_events.extend(events);
    }
    let pairs = location::learn_pairs(&files);
    if let Ok(folder) = std::env::var("VERSEKIT_INTERN") {
        coverage(&folder, &all_events, &landings, &pairs);
    }
    let mut names: BTreeMap<u64, String> = BTreeMap::new();
    for (number, key) in &pairs {
        if let Ok(number) = number.parse() {
            names.insert(number, key.clone());
        }
    }
    if gegenprobe {
        landings.pop_first();
        names.pop_first();
    }
    let without: Vec<String> = landings.iter().filter(|n| !names.contains_key(n)).map(u64::to_string).collect();
    let mut conflicts: BTreeSet<u64> = BTreeSet::new();
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for path in &files {
        let bytes = fs::read(path).map_err(|e| format!("{path}: {e}"))?;
        let text = String::from_utf8_lossy(&bytes);
        for (number, key) in location::pairs_in(text.split(['\n', '\r'])) {
            if seen.get(&number).is_some_and(|earlier| *earlier != key)
                && let Ok(number) = number.parse()
            {
                conflicts.insert(number);
            }
            seen.insert(number, key);
        }
    }
    let result = json!({
        "dateien": counted,
        "orte": {
            "landing": landings.iter().map(u64::to_string).collect::<Vec<_>>(),
            "gelernt": names.len(),
            "ohne_namen": without,
            "widersprueche": conflicts.iter().map(u64::to_string).collect::<Vec<_>>(),
            "namen": names.iter().map(|(n, k)| format!("{n}={k}")).collect::<Vec<_>>(),
        },
    });
    fs::write(out, serde_json::to_string(&result).map_err(|e| e.to_string())?).map_err(|e| format!("{out}: {e}"))?;
    println!("{} Logs, {} Orts-IDs, {} Orte gelernt", files.len(), landings.len(), names.len());
    Ok(())
}
