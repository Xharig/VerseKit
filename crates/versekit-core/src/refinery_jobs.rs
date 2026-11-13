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

//! Running refinery jobs with their end time (`refinery_jobs.py`), as far as
//! the overlay needs them: which jobs are over now, each reported exactly
//! once, also across restarts.
//!
//! The game writes nothing about a job into the log. The end time comes from
//! the scan of the terminal (the rest time of the card plus the moment of the
//! scan) and is kept in `raffinerie-auftraege.json`:
//! `{"version": 1, "auftraege": [{"materials": [..], "total": "..", "ends": 1.5e9}]}`.
//! A job is reported once it is over and does not carry `notified` yet.
//!
//! Beyond that a job may carry what the Basetool sync needs, all of it optional and kept when the job is read
//! again: the refinery read at the top of the terminal ([`PLACE`]), the yield lines of the card ([`GOODS`]), the
//! own key of the order over there ([`KEY`]), the id of an order taken over from the Basetool ([`ORDER`]) and
//! when the yield was booked into the own stock ([`PICKED_UP`]).

use crate::jsonio::{self, Map, Newline, Value, map_set};
use crate::language::{Lang, current, tf};
use crate::paths::Paths;

pub const FILE: &str = "raffinerie-auftraege.json";

/// The refinery the job runs at, as the list of places writes it.
pub const PLACE: &str = "ort";
/// The yield lines: `[{"material", "qualitaet", "menge"}]`, the amount in cSCU (100 to the SCU).
pub const GOODS: &str = "gueter";
/// The own key of the order in the Basetool (`clientKey`).
pub const KEY: &str = "schluessel";
/// The id of an order that came from the Basetool (created in the web or by another installation).
pub const ORDER: &str = "basetool";
/// When the yield was booked into the own stock (seconds since 1970): the order is picked up.
pub const PICKED_UP: &str = "abgeholt";

/// The fields a new reading of a job never brings and that stay with it.
const KEPT: [&str; 5] = [PLACE, GOODS, KEY, ORDER, PICKED_UP];

/// One yield line of a job.
#[derive(Clone, Debug, PartialEq)]
pub struct Good {
    pub material: String,
    pub quality: i64,
    /// cSCU, 100 to the SCU.
    pub amount: i64,
}

/// The yield lines of the card text (`Material Quality Amount [cSCU|SCU]` per line); lines that do not read so
/// are left out. An amount without unit is cSCU, as the terminal shows it.
pub fn goods_of_text(text: &str) -> Vec<Good> {
    let mut out: Vec<Good> = Vec::new();
    for line in text.lines() {
        let mut parts: Vec<&str> = line.split_whitespace().collect();
        let mut factor = 1.0;
        match parts.last().map(|p| p.to_lowercase()) {
            Some(unit) if unit == "cscu" => {
                parts.pop();
            }
            Some(unit) if unit == "scu" => {
                parts.pop();
                factor = 100.0;
            }
            _ => {}
        }
        if parts.len() < 3 {
            continue;
        }
        let number = |text: &str| text.replace(',', ".").parse::<f64>().ok().filter(|x| x.is_finite());
        let (Some(quality), Some(amount)) = (number(parts[parts.len() - 2]), number(parts[parts.len() - 1])) else {
            continue;
        };
        let amount = (amount * factor).round() as i64;
        let quality = quality.trunc() as i64;
        if amount <= 0 || !(0..=1000).contains(&quality) {
            continue;
        }
        let good = Good { material: parts[..parts.len() - 2].join(" "), quality, amount };
        if !out.contains(&good) {
            out.push(good);
        }
    }
    out
}

fn goods_value(goods: &[Good]) -> Value {
    Value::Array(
        goods
            .iter()
            .map(|g| {
                Value::Object(vec![
                    ("material".into(), Value::str(&g.material)),
                    ("qualitaet".into(), Value::int(g.quality)),
                    ("menge".into(), Value::int(g.amount)),
                ])
            })
            .collect(),
    )
}

/// The yield lines a job carries.
pub fn goods(job: &Map) -> Vec<Good> {
    let Some((_, Value::Array(items))) = job.iter().find(|(k, _)| k == GOODS) else { return Vec::new() };
    items
        .iter()
        .filter_map(|item| {
            let material = item.get("material").and_then(Value::as_str).filter(|m| !m.is_empty())?;
            let whole = |key: &str| item.get(key).and_then(number).map(|x| x as i64);
            Some(Good { material: material.to_string(), quality: whole("qualitaet")?, amount: whole("menge")? })
        })
        .collect()
}

/// A text field of a job, empty when it has none.
pub fn text(job: &Map, key: &str) -> String {
    job.iter().find(|(k, _)| k == key).and_then(|(_, v)| v.as_str()).unwrap_or_default().to_string()
}

/// When the job ends (seconds since 1970); infinity without a readable end.
pub fn ends(job: &Map) -> f64 {
    ends_of(job)
}

/// Whether the yield of a job was booked into the own stock.
pub fn picked_up(job: &Map) -> bool {
    job.iter().any(|(k, v)| k == PICKED_UP && number(v).is_some())
}

/// A number as Python counts one for `isinstance(x, (int, float))`.
fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        Value::Int(text) => text.parse().ok(),
        Value::Float(x) => Some(*x),
        _ => None,
    }
}

/// All kept jobs: objects with a number as `ends`.
pub fn load(paths: &Paths) -> Vec<Map> {
    let Ok(Value::Object(data)) = jsonio::read_json(&paths.app_file(FILE)) else { return Vec::new() };
    match data.iter().find(|(k, _)| k == "auftraege") {
        Some((_, Value::Array(jobs))) => jobs
            .iter()
            .filter_map(|j| match j {
                Value::Object(map) if map.iter().any(|(k, v)| k == "ends" && number(v).is_some()) => Some(map.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn save(paths: &Paths, jobs: &[Map]) -> bool {
    let data = Value::Object(vec![
        ("version".into(), Value::int(1)),
        ("auftraege".into(), Value::Array(jobs.iter().cloned().map(Value::Object).collect())),
    ]);
    let path = paths.app_file(FILE);
    match jsonio::write_text_atomic(&path, &jsonio::dumps(&data, Some(1), false), Newline::Native) {
        Ok(()) => {
            // A new reading or a pickup: the refinery sync follows soon (its own writes do not count).
            crate::basetool::sync::store_saved(&path);
            true
        }
        Err(e) => {
            crate::errors::record("refinery_jobs.save", &e);
            false
        }
    }
}

fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Int(n)) => n != "0",
        Some(Value::Float(x)) => *x != 0.0,
        Some(Value::Str(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

/// Done jobs fall out of the list after this many seconds.
pub const KEEP_DONE_SEC: f64 = 24.0 * 3600.0;

/// Two readings whose ends lie at most this far apart and that share two materials are one job, read before and
/// after scrolling its list (the terminal gives the time left in minutes).
pub const SAME_JOB_SEC: f64 = 120.0;

/// `_key`: a job is known by its materials (without case) and its sum.
fn key_of(materials: Option<&Value>, total: Option<&Value>) -> (Vec<String>, Value) {
    let materials = match materials {
        Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).map(str::to_lowercase).collect(),
        _ => Vec::new(),
    };
    let total = match total {
        Some(value) if truthy(Some(value)) => value.clone(),
        _ => Value::str(""),
    };
    (materials, total)
}

fn ends_of(job: &Map) -> f64 {
    job.iter().find(|(k, _)| k == "ends").and_then(|(_, v)| number(v)).unwrap_or(f64::INFINITY)
}

/// `merge`: kept jobs and a scan made into one, without the disk. Running jobs with a time left read get
/// `ends = now + time left`, jobs read as done `ends = now` (when they were known already) and `notified`: who sees
/// it done at the terminal needs no message any more. What is done longer than [`KEEP_DONE_SEC`] falls out.
pub fn merge(stored: &[Map], scanned: &[crate::refinery_scan::ScannedJob], now: f64) -> Vec<Map> {
    use crate::refinery_scan::State;
    let get = |job: &Map, key: &str| job.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
    let mut by_key: Vec<((Vec<String>, Value), Map)> = Vec::new();
    let slot = |by_key: &[((Vec<String>, Value), Map)], key: &(Vec<String>, Value)| {
        by_key.iter().position(|(k, _)| k.0 == key.0 && jsonio::py_eq_value(&k.1, &key.1))
    };
    for job in stored {
        let key = key_of(get(job, "materials").as_ref(), get(job, "total").as_ref());
        match slot(&by_key, &key) {
            Some(i) => by_key[i].1 = job.clone(),
            None => by_key.push((key, job.clone())),
        }
    }
    for job in scanned {
        let materials = Value::Array(job.materials.iter().map(|m| Value::str(m.clone())).collect());
        let key = key_of(Some(&materials), Some(&Value::str(job.total.clone())));
        if key.0.is_empty() {
            continue;
        }
        match (job.remaining, job.state) {
            (Some(remaining), state) if state != State::Done => {
                let ends = now + remaining as f64;
                // The same job read again after scrolling: it ends at the same time and shares at least two
                // materials (a list long enough to scroll holds several). Its materials are joined, its sum is the
                // newer one.
                let scrolled = slot(&by_key, &key).is_none().then(|| {
                    by_key.iter().position(|(k, known)| {
                        let shared = k.0.iter().filter(|m| key.0.contains(m)).count();
                        (ends_of(known) - ends).abs() <= SAME_JOB_SEC && shared >= 2
                    })
                });
                let mut all = job.materials.clone();
                if let Some(Some(i)) = scrolled
                    && let Some(Value::Array(known)) = get(&by_key[i].1, "materials")
                {
                    let before: Vec<String> = known.iter().filter_map(Value::as_str).map(str::to_string).collect();
                    all = before.clone();
                    all.extend(job.materials.iter().filter(|m| !before.iter().any(|b| b.eq_ignore_ascii_case(m))).cloned());
                }
                let materials = Value::Array(all.iter().map(|m| Value::str(m.clone())).collect());
                let mut entry = vec![
                    ("materials".to_string(), materials.clone()),
                    ("total".to_string(), Value::str(job.total.clone())),
                    ("ends".to_string(), Value::Float(ends)),
                ];
                let key = key_of(Some(&materials), Some(&Value::str(job.total.clone())));
                let known = slot(&by_key, &key).or(scrolled.flatten());
                // What the sync needs: the place and the lines read now, joined to those of the job read before,
                // and what the job carried for the Basetool. A reading without place or lines adds no field.
                let before = known.map(|i| by_key[i].1.clone()).unwrap_or_default();
                let mut lines = goods(&before);
                for good in goods_of_text(&job.text) {
                    if !lines.contains(&good) {
                        lines.push(good);
                    }
                }
                if !lines.is_empty() {
                    entry.push((GOODS.to_string(), goods_value(&lines)));
                }
                let place = crate::text::py_strip(&job.station).to_string();
                if !place.is_empty() {
                    entry.push((PLACE.to_string(), Value::str(place)));
                }
                for field in KEPT {
                    if !entry.iter().any(|(k, _)| k == field)
                        && let Some((_, value)) = before.iter().find(|(k, _)| k == field)
                    {
                        entry.push((field.to_string(), value.clone()));
                    }
                }
                match known {
                    Some(i) => by_key[i] = (key, entry),
                    None => by_key.push((key, entry)),
                }
            }
            (_, State::Done) => {
                if let Some(i) = slot(&by_key, &key) {
                    let known = &mut by_key[i].1;
                    if now < ends_of(known) {
                        map_set(known, "ends", Value::Float(now));
                    }
                    map_set(known, "notified", Value::Bool(true));
                }
            }
            _ => {}
        }
    }
    let mut kept: Vec<Map> =
        by_key.into_iter().map(|(_, job)| job).filter(|job| now - ends_of(job) <= KEEP_DONE_SEC).collect();
    kept.sort_by(|a, b| ends_of(a).total_cmp(&ends_of(b)));
    kept
}

/// `remember`: take a scan over and keep it. Gives the new list; a scan without a time left and without a job
/// done changes nothing.
pub fn remember(paths: &Paths, scanned: &[crate::refinery_scan::ScannedJob], now: f64) -> Vec<Map> {
    use crate::refinery_scan::State;
    if !scanned.iter().any(|j| j.remaining.is_some() || j.state == State::Done) {
        return load(paths);
    }
    // Read, merged and written under the lock of the saves: the watcher reports due jobs from the same file.
    let _saves = jsonio::lock_saves();
    let jobs = merge(&load(paths), scanned, now);
    save(paths, &jobs);
    jobs
}

/// `current`: `(running, done)`, the running ones by their end.
pub fn current_jobs(paths: &Paths, now: f64) -> (Vec<Map>, Vec<Map>) {
    let jobs = merge(&load(paths), &[], now);
    jobs.into_iter().partition(|job| ends_of(job) > now)
}

/// `(due, all)`: due are the jobs that are over and not yet `notified`; in
/// `all` they carry `notified: true` afterwards. No disk.
pub fn due(stored: &[Map], now: f64) -> (Vec<Map>, Vec<Map>) {
    let (mut found, mut jobs) = (Vec::new(), Vec::new());
    for job in stored {
        let mut job = job.clone();
        let ends = job.iter().find(|(k, _)| k == "ends").and_then(|(_, v)| number(v)).unwrap_or(f64::INFINITY);
        if ends <= now && !truthy(job.iter().find(|(k, _)| k == "notified").map(|(_, v)| v)) {
            map_set(&mut job, "notified", Value::Bool(true));
            found.push(job.clone());
        }
        jobs.push(job);
    }
    (found, jobs)
}

/// The jobs that became over since the last call, each exactly once, also
/// across a restart.
pub fn announce_due(paths: &Paths, now: f64) -> Vec<Map> {
    // Read, changed and written under the write lock: the main window writes the same file.
    let _lock = crate::jsonio::lock_saves();
    let (found, jobs) = due(&load(paths), now);
    if !found.is_empty() {
        save(paths, &jobs);
    }
    found
}

/// Changes the kept jobs under the write lock; `change` says whether it changed anything, and only then the file is
/// written. False when the write failed.
pub fn update(paths: &Paths, change: impl FnOnce(&mut Vec<Map>) -> bool) -> bool {
    let _lock = crate::jsonio::lock_saves();
    let mut jobs = load(paths);
    if !change(&mut jobs) {
        return true;
    }
    save(paths, &jobs)
}

/// The job whose yield was just booked into the own stock, by the materials booked: the one not yet picked up
/// that shares the most of its materials with them (without case), at least half of its own; of equal ones the one
/// with the larger part of its own booked, then the one that ended first. `None` when no job fits.
pub fn booked_job(jobs: &[Map], booked: &[String]) -> Option<usize> {
    let booked: Vec<String> = booked.iter().map(|m| m.to_lowercase()).collect();
    let mut best: Option<(usize, usize, f64, f64)> = None;
    for (index, job) in jobs.iter().enumerate() {
        if picked_up(job) {
            continue;
        }
        let Some((_, Value::Array(items))) = job.iter().find(|(k, _)| k == "materials") else { continue };
        let own: Vec<String> = items.iter().filter_map(Value::as_str).map(str::to_lowercase).collect();
        let shared = own.iter().filter(|m| booked.contains(m)).count();
        if own.is_empty() || shared == 0 || shared * 2 < own.len() {
            continue;
        }
        let (end, part) = (ends_of(job), shared as f64 / own.len() as f64);
        let better = match best {
            None => true,
            Some((_, most, larger, first)) => {
                shared > most || (shared == most && (part > larger || (part == larger && end < first)))
            }
        };
        if better {
            best = Some((index, shared, part, end));
        }
    }
    best.map(|(index, ..)| index)
}

/// The job a reading of the terminal stands for: the same materials (without case) and the same sum, not picked up.
pub fn read_job(jobs: &[Map], materials: &[String], total: &str) -> Option<usize> {
    let key = key_of(
        Some(&Value::Array(materials.iter().map(|m| Value::str(m.clone())).collect())),
        Some(&Value::str(total)),
    );
    jobs.iter().position(|job| {
        let get = |name: &str| job.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
        let own = key_of(get("materials").as_ref(), get("total").as_ref());
        !picked_up(job) && own.0 == key.0 && jsonio::py_eq_value(&own.1, &key.1)
    })
}

/// The yield of a job was booked into the own stock: the job is picked up (and needs no message any more). `read`
/// names the reading the field was filled from (materials and sum): that job counts first; without one, or when
/// it is gone, the job is found by the materials booked. Gives whether a job was found. The stock itself is booked
/// by the caller; the Basetool sync only closes the order.
pub fn mark_picked_up(paths: &Paths, booked: &[String], read: Option<(&[String], &str)>, now: f64) -> bool {
    let mut found = false;
    update(paths, |jobs| {
        let by_reading = read.and_then(|(materials, total)| read_job(jobs, materials, total));
        let Some(index) = by_reading.or_else(|| booked_job(jobs, booked)) else { return false };
        map_set(&mut jobs[index], PICKED_UP, Value::Float(now));
        map_set(&mut jobs[index], "notified", Value::Bool(true));
        found = true;
        true
    });
    found
}

/// The label of a job: the first materials and the sum (`refinery_scan.job_label`).
pub fn job_label(job: &Map, most: usize) -> String {
    let get = |key: &str| job.iter().find(|(k, _)| k == key).map(|(_, v)| v);
    let mut seen: Vec<&str> = Vec::new();
    if let Some(Value::Array(items)) = get("materials") {
        for name in items.iter().filter_map(Value::as_str) {
            if !seen.contains(&name) {
                seen.push(name);
            }
        }
    }
    let mut names = if seen.is_empty() { "?".to_string() } else { seen[..most.min(seen.len())].join(", ") };
    if seen.len() > most {
        names.push_str(" …");
    }
    if let Some(Value::Str(total)) = get("total").filter(|v| truthy(Some(v))) {
        let total = if current() == Lang::De { total.replace('.', ",") } else { total.clone() };
        names = tf("s_rf_auftrag_summe", &[("0", names.into()), ("1", total.into())]);
    }
    match get("state").and_then(Value::as_str) {
        Some("fertig") => tf("s_rf_auftrag_fertig", &[("0", names.into())]),
        Some("laeuft") => tf("s_rf_auftrag_laeuft", &[("0", names.into())]),
        _ => names,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(text: &str) -> Map {
        match jsonio::loads(text).expect("Beispiel") {
            Value::Object(map) => map,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_job_is_due_once_when_its_time_has_come() {
        let stored = [job(r#"{"materials": ["Gold"], "total": "12.5", "ends": 100.0}"#), job(r#"{"ends": 300}"#)];
        let (found, all) = due(&stored, 150.0);
        assert_eq!(found.len(), 1);
        assert!(all[0].iter().any(|(k, v)| k == "notified" && *v == Value::Bool(true)));
        assert!(!all[1].iter().any(|(k, _)| k == "notified"));
        // The second time nothing is due any more.
        assert!(due(&all, 150.0).0.is_empty());
    }

    fn scanned(
        materials: &[&str],
        total: &str,
        state: crate::refinery_scan::State,
        remaining: Option<i64>,
    ) -> crate::refinery_scan::ScannedJob {
        crate::refinery_scan::ScannedJob {
            text: String::new(),
            unsure: Vec::new(),
            materials: materials.iter().map(|m| m.to_string()).collect(),
            total: total.into(),
            state,
            remaining,
            cells: Vec::new(),
            station: String::new(),
        }
    }

    fn ends(job: &Map) -> f64 {
        super::ends_of(job)
    }

    #[test]
    fn a_job_read_before_and_after_scrolling_is_one_job_and_two_jobs_stay_two() {
        use crate::refinery_scan::State;
        let now = 1_000_000.0;
        let first = merge(&[], &[scanned(&["Iron", "Quantainium", "Laranite"], "3.63", State::Running, Some(19_800))], now);
        // Scrolled a minute later: one more material, a newer sum, the time left a minute less.
        let both = merge(
            &first,
            &[scanned(&["Iron", "Quantainium", "Laranite", "Tungsten"], "4.96", State::Running, Some(19_740))],
            now + 30.0,
        );
        assert_eq!(both.len(), 1, "{both:?}");
        let materials = both[0].iter().find(|(k, _)| k == "materials").map(|(_, v)| v.clone());
        assert_eq!(
            materials,
            Some(Value::Array(["Iron", "Quantainium", "Laranite", "Tungsten"].map(Value::str).to_vec()))
        );
        // A second job that ends at the same time with one material in common stays a job of its own.
        let other = merge(&both, &[scanned(&["Iron", "Gold"], "2.00", State::Running, Some(19_740))], now + 30.0);
        assert_eq!(other.len(), 2);
    }

    #[test]
    fn a_scan_sets_the_end_replaces_it_marks_done_and_forgets_after_a_day() {
        use crate::refinery_scan::State;
        let now = 1_000_000.0;
        let jobs = merge(&[], &[scanned(&["Gold", "Iron"], "12.76", State::Running, Some(600))], now);
        assert_eq!(jobs.len(), 1);
        assert_eq!(ends(&jobs[0]), now + 600.0);
        // The same job scanned again: the new time left counts, case does not matter.
        let again = merge(&jobs, &[scanned(&["gold", "IRON"], "12.76", State::Unknown, Some(60))], now + 100.0);
        assert_eq!(again.len(), 1);
        assert_eq!(ends(&again[0]), now + 160.0);
        // Read as done: over now and needs no message any more.
        let done = merge(&again, &[scanned(&["Gold", "Iron"], "12.76", State::Done, None)], now + 120.0);
        assert_eq!(ends(&done[0]), now + 120.0);
        assert!(done[0].iter().any(|(k, v)| k == "notified" && *v == Value::Bool(true)));
        // Done and unknown, or without materials: nothing new.
        assert_eq!(
            merge(
                &done,
                &[scanned(&["Quarz"], "", State::Done, None), scanned(&[], "1", State::Running, Some(5))],
                now
            )
            .len(),
            1
        );
        // A day after its end it falls out; the list stands by the end.
        assert!(merge(&done, &[], now + 120.0 + KEEP_DONE_SEC + 1.0).is_empty());
        let two = merge(
            &[],
            &[scanned(&["A"], "", State::Running, Some(900)), scanned(&["B"], "", State::Running, Some(30))],
            now,
        );
        assert_eq!(two.iter().map(ends).collect::<Vec<_>>(), [now + 30.0, now + 900.0]);
    }

    fn read(materials: &[&str], text: &str, station: &str, remaining: i64) -> crate::refinery_scan::ScannedJob {
        let mut job = scanned(materials, "", crate::refinery_scan::State::Running, Some(remaining));
        job.text = text.into();
        job.station = station.into();
        job
    }

    #[test]
    fn the_lines_and_the_place_of_a_reading_stay_with_the_job_and_join_after_scrolling() {
        let now = 1_000_000.0;
        let first = merge(&[], &[read(&["Gold", "Iron"], "Gold 512 1200\nIron 300 4,5 SCU\nkaputt", "CRU-L1", 600)], now);
        assert_eq!(text(&first[0], PLACE), "CRU-L1");
        assert_eq!(
            goods(&first[0]),
            [
                Good { material: "Gold".into(), quality: 512, amount: 1200 },
                Good { material: "Iron".into(), quality: 300, amount: 450 }
            ]
        );
        // The sync wrote its key; a new reading without place or lines keeps all of it.
        let mut keyed = first.clone();
        map_set(&mut keyed[0], KEY, Value::str("vk-rf-1"));
        let again = merge(&keyed, &[read(&["Gold", "Iron"], "", "", 540)], now + 60.0);
        assert_eq!(text(&again[0], KEY), "vk-rf-1");
        assert_eq!(text(&again[0], PLACE), "CRU-L1");
        assert_eq!(goods(&again[0]).len(), 2);
        // Scrolled: one more line joins.
        let more = merge(&again, &[read(&["Gold", "Iron", "Tin"], "Iron 300 4,5 SCU\nTin 10 80 cSCU", "", 540)], now + 60.0);
        assert_eq!(more.len(), 1);
        assert_eq!(goods(&more[0]).len(), 3);
        assert_eq!(text(&more[0], KEY), "vk-rf-1");
        // A reading without place and lines adds no field (as 4.12 writes it).
        let bare = merge(&[], &[read(&["Gold"], "", "", 60)], now);
        assert!(bare[0].iter().all(|(k, _)| !KEPT.contains(&k.as_str())), "{bare:?}");
    }

    #[test]
    fn the_booked_materials_find_the_job_they_came_from() {
        let jobs = [
            job(r#"{"materials": ["Gold", "Iron"], "ends": 100}"#),
            job(r#"{"materials": ["Gold", "Iron", "Tin", "Copper"], "ends": 50}"#),
            job(r#"{"materials": ["Quartz"], "ends": 10, "abgeholt": 5}"#),
        ];
        let booked = |names: &[&str]| booked_job(&jobs, &names.iter().map(|n| n.to_string()).collect::<Vec<_>>());
        assert_eq!(booked(&["gold", "IRON"]), Some(0), "both of the first, half of the second");
        assert_eq!(booked(&["Gold", "Iron", "Tin"]), Some(1), "the most shared");
        assert_eq!(booked(&["Quartz"]), None, "picked up already");
        assert_eq!(booked(&["Gold"]), Some(0), "half of the first, a quarter of the second");
        assert_eq!(booked(&["Laranite"]), None);
    }

    #[test]
    fn the_reading_the_field_came_from_names_its_job_before_the_materials_do() {
        let jobs = [
            job(r#"{"materials": ["Gold", "Iron"], "total": "15", "ends": 100}"#),
            job(r#"{"materials": ["Gold", "Iron"], "total": "9", "ends": 50}"#),
        ];
        let names = |list: &[&str]| list.iter().map(|n| n.to_string()).collect::<Vec<_>>();
        assert_eq!(read_job(&jobs, &names(&["gold", "IRON"]), "15"), Some(0));
        assert_eq!(read_job(&jobs, &names(&["Gold", "Iron"]), "9"), Some(1));
        assert_eq!(read_job(&jobs, &names(&["Gold"]), "15"), None);
        // By the materials alone both fit: the one that ended first is taken.
        assert_eq!(booked_job(&jobs, &names(&["Gold", "Iron"])), Some(1));
    }

    #[test]
    fn the_label_names_two_materials_and_the_sum() {
        crate::language::set_thread_language(crate::language::Choice::De);
        let label = job(r#"{"materials": ["Gold", "Gold", "Iron", "Quarz"], "total": "12.5"}"#);
        assert_eq!(job_label(&label, 2), "Gold, Iron … · 12,5 cSCU");
        assert_eq!(job_label(&job(r#"{"materials": []}"#), 2), "?");
        assert_eq!(job_label(&job(r#"{"materials": ["Gold"], "state": "fertig"}"#), 2), "Fertig: Gold");
    }
}
