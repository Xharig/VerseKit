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

//! The course of the performance index over the days: each reading of the rating with an index is noted with
//! the local day in `rechner-verlauf.json` in the data folder, one entry per day (the last reading of the day
//! counts), the newest [`KEEP`] days.
//!
//! The file: `{"format": 1, "tage": [{"tag": "2026-10-31", "cpu": 8.5, "gpu": 12.75, "klasse": 4}, …]}`, oldest
//! first; `gpu` and `klasse` may be missing.

use crate::jsonio::{self, Value};
use crate::paths::Paths;

use super::Info;

/// The file in the data folder.
pub const FILE: &str = "rechner-verlauf.json";
/// The format of the file.
pub const FORMAT: i64 = 1;
/// So many days are kept.
pub const KEEP: usize = 90;

/// The rating of one day.
#[derive(Clone, Debug, PartialEq)]
pub struct Day {
    /// The local day, `YYYY-MM-DD`.
    pub day: String,
    pub cpu: f64,
    pub gpu: Option<f64>,
    pub class: Option<i64>,
}

fn number(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Float(x) => Some(*x),
        Value::Int(text) => text.parse().ok(),
        _ => None,
    }
}

/// The days noted, oldest first; none without a readable file of the format.
pub fn load(paths: &Paths) -> Vec<Day> {
    let Ok(data) = jsonio::read_json(&paths.app_file(FILE)) else { return Vec::new() };
    if data.get("format") != Some(&Value::int(FORMAT)) {
        return Vec::new();
    }
    let Some(Value::Array(days)) = data.get("tage") else { return Vec::new() };
    days.iter()
        .filter_map(|entry| {
            let day = entry.get("tag")?.as_str()?.to_string();
            let cpu = number(entry.get("cpu"))?;
            let class = match entry.get("klasse") {
                Some(Value::Int(text)) => text.parse().ok(),
                _ => None,
            };
            Some(Day { day, cpu, gpu: number(entry.get("gpu")), class })
        })
        .collect()
}

fn to_value(days: &[Day]) -> Value {
    let entries = days
        .iter()
        .map(|d| {
            let mut entry =
                vec![("tag".to_string(), Value::str(d.day.clone())), ("cpu".to_string(), Value::Float(d.cpu))];
            if let Some(gpu) = d.gpu {
                entry.push(("gpu".to_string(), Value::Float(gpu)));
            }
            if let Some(class) = d.class {
                entry.push(("klasse".to_string(), Value::int(class)));
            }
            Value::Object(entry)
        })
        .collect();
    Value::Object(vec![("format".to_string(), Value::int(FORMAT)), ("tage".to_string(), Value::Array(entries))])
}

/// Notes the rating `info` for `day` (`YYYY-MM-DD`): a day already noted takes the new values, an older day than
/// the newest noted is left alone. Without a CPU index nothing is noted. Writes only when something changed;
/// returns whether the file was written.
pub fn note(paths: &Paths, info: &Info, day: &str) -> bool {
    let Some(cpu) = info.cpu_index else { return false };
    // A class taken from the note file is no reading of the day.
    let class = info.machine_class.filter(|_| !info.class_remembered);
    let today = Day { day: day.to_string(), cpu, gpu: info.gpu_index, class };
    let mut days = load(paths);
    match days.last() {
        Some(last) if last.day.as_str() > day => return false,
        Some(last) if *last == today => return false,
        Some(last) if last.day == day => {
            days.pop();
        }
        _ => {}
    }
    days.push(today);
    let surplus = days.len().saturating_sub(KEEP);
    days.drain(..surplus);
    match jsonio::save_json(&paths.app_file(FILE), &to_value(&days), 2, false) {
        Ok(()) => true,
        Err(error) => {
            crate::errors::record("machine_info.history", &error);
            false
        }
    }
}

/// The local day of `seconds` (since 1970) as the file writes it.
pub fn day_of(seconds: i64) -> Option<String> {
    crate::formats::local_time(seconds).map(|time| crate::formats::strftime("%Y-%m-%d", &time))
}

/// [`note`] for the local day of now.
pub fn note_now(paths: &Paths, info: &Info) -> bool {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| i64::try_from(since.as_secs()).unwrap_or(0));
    day_of(now).is_some_and(|day| note(paths, info, &day))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Host;
    use crate::pypath::Flavor;
    use crate::testdir::Dir;

    fn paths_in(name: &str) -> (Dir, Paths) {
        let folder = Dir::new(name);
        let text = folder.path().to_string_lossy().into_owned();
        let host = Host::new(Flavor::native(), &[("SC_BP_HOME", &text)], &text);
        (folder, Paths::new(host))
    }

    fn rated(cpu: f64, gpu: f64, class: i64) -> Info {
        Info { cpu_index: Some(cpu), gpu_index: Some(gpu), machine_class: Some(class), ..Info::default() }
    }

    #[test]
    fn one_entry_per_day_the_last_reading_counts_and_an_older_day_is_left_alone() {
        let (_folder, paths) = paths_in("rechner-verlauf");
        assert!(load(&paths).is_empty(), "nothing noted yet");
        assert!(!note(&paths, &Info::default(), "2026-10-29"), "without an index nothing");
        assert!(note(&paths, &rated(8.5, 12.75, 4), "2026-10-29"));
        assert!(!note(&paths, &rated(8.5, 12.75, 4), "2026-10-29"), "the same again writes nothing");
        assert!(note(&paths, &rated(8.0, 12.5, 4), "2026-10-29"), "a new reading of the same day");
        assert!(note(&paths, &rated(9.25, 13.0, 5), "2026-10-31"));
        assert!(!note(&paths, &rated(1.0, 1.0, 1), "2026-10-30"), "a day before the newest");
        let days = load(&paths);
        assert_eq!(
            days,
            [
                Day { day: "2026-10-29".into(), cpu: 8.0, gpu: Some(12.5), class: Some(4) },
                Day { day: "2026-10-31".into(), cpu: 9.25, gpu: Some(13.0), class: Some(5) },
            ]
        );
    }

    #[test]
    fn a_remembered_class_is_not_noted_as_the_class_of_the_day() {
        let (_folder, paths) = paths_in("rechner-verlauf-gemerkt");
        let remembered = Info { class_remembered: true, ..rated(8.5, 12.75, 4) };
        assert!(note(&paths, &remembered, "2026-10-29"));
        assert_eq!(load(&paths), [Day { day: "2026-10-29".into(), cpu: 8.5, gpu: Some(12.75), class: None }]);
    }

    #[test]
    fn only_the_newest_days_are_kept() {
        let (_folder, paths) = paths_in("rechner-verlauf-grenze");
        for n in 0..KEEP + 5 {
            let day = format!("2026-{:02}-{:02}", 1 + n / 28, 1 + n % 28);
            assert!(note(&paths, &rated(n as f64, 1.0, 3), &day), "{day}");
        }
        let days = load(&paths);
        assert_eq!(days.len(), KEEP);
        assert_eq!(days[0].cpu, 5.0, "the five oldest are gone");
        assert_eq!(days.last().map(|d| d.cpu), Some((KEEP + 4) as f64));
    }

    #[test]
    fn a_file_of_another_format_counts_as_none() {
        let (_folder, paths) = paths_in("rechner-verlauf-format");
        std::fs::write(paths.app_file(FILE), r#"{"format": 2, "tage": [{"tag": "2026-10-31", "cpu": 1}]}"#).unwrap();
        assert!(load(&paths).is_empty());
        std::fs::write(paths.app_file(FILE), r#"{"format": 1, "tage": [{"tag": "2026-10-31", "cpu": 7}]}"#).unwrap();
        assert_eq!(load(&paths), [Day { day: "2026-10-31".into(), cpu: 7.0, gpu: None, class: None }]);
    }
}
