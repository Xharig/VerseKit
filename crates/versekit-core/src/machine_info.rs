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

//! What Star Citizen writes about the computer into `Game.log` at its start (`machine_info.py` of 4.12).
//!
//! | Line | Content |
//! |---|---|
//! | `Host CPU: …`, `Logical CPU Count: …` | processor, threads |
//! | `…MB physical memory installed` | memory |
//! | `[VK] Chosen Vulkan GPU Device (…) Driver Version (…) Vulkan API (…)` | graphics card, driver |
//! | `GPU: DedicatedVidMemMB = …` | graphics memory |
//! | `CPU benchmark: … ms (int+mem), … ms (fp+mem)` / `GPU benchmark: … ms` | times |
//! | `Performance Index: … (CPU), … (GPU)` | index of the game |
//! | `Final rating: Machine class N` | the class of the game, 1 to 5 |
//!
//! The class only arises when the game runs its automatic detection (first start, choosing a preset), so it
//! is also looked for in the kept logs, newest first.
//!
//! The classes 1 to 5 are the levels of the graphics templates in the archive (Low, Medium, High, Very High,
//! Ultra).
//!
//! The newest class found is kept in `rechnerklasse.json` in the data folder
//! (`{"format": 1, "klasse": 4, "gefunden_am": 1761127200}`, time in Unix seconds); where no log has a class
//! any more, that one is taken.

pub mod history;

use std::collections::HashMap;
use std::io::Read;
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

use regex::Regex;

use crate::errors;
use crate::jsonio::{self, Value};
use crate::paths::Paths;

/// The classes there are.
pub const CLASSES: i64 = 5;
/// The file in the data folder that keeps the newest class found.
pub const NOTE_FILE: &str = "rechnerklasse.json";
/// The format of [`NOTE_FILE`].
const NOTE_FORMAT: i64 = 1;
/// Up to here the start block is read; the hardware stands in it.
const HEAD_BYTES: usize = 600_000;
/// So many kept logs are searched for the class.
const BACKUPS_SEARCHED: usize = 40;

/// What the log says about the computer; what it does not say is `None`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Info {
    pub cpu: Option<String>,
    pub threads: Option<i64>,
    pub ram_mb: Option<i64>,
    pub gpu: Option<String>,
    pub driver: Option<String>,
    pub vulkan: Option<String>,
    pub vram_mb: Option<i64>,
    /// Width and height of the display mode.
    pub display: Option<(i64, i64)>,
    pub dlss: Option<bool>,
    /// Time of the integer and the float test in ms.
    pub cpu_bench: Option<(f64, f64)>,
    pub gpu_bench: Option<f64>,
    pub cpu_index: Option<f64>,
    pub gpu_index: Option<f64>,
    pub machine_class: Option<i64>,
    /// Unix seconds of the rating: of its line in the log (of the file where the line has no time), or when
    /// the class was noted.
    pub class_at: Option<i64>,
    /// The class comes from the note file, no log has it any more.
    pub class_remembered: bool,
}

struct Patterns {
    cpu: Regex,
    threads: Regex,
    ram: Regex,
    gpu_vk: Regex,
    vram: Regex,
    display: Regex,
    dlss: Regex,
    cpu_bench: Regex,
    gpu_bench: Regex,
    index: Regex,
    class: Regex,
    first_adapter: Regex,
}

fn patterns() -> &'static Patterns {
    static PATTERNS: OnceLock<Patterns> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        let re = |text: &str| Regex::new(text).expect("pattern");
        Patterns {
            cpu: re(r"Host CPU: (.+)"),
            threads: re(r"Logical CPU Count: (\d+)"),
            ram: re(r"(\d+)MB physical memory installed"),
            gpu_vk: re(r"\[VK\] Chosen Vulkan GPU Device \((.+?)\) Driver Version \((.+?)\) Vulkan API \((.+?)\)"),
            vram: re(r"GPU: DedicatedVidMemMB = (\d+)"),
            display: re(r"Current display mode is (\d+)x(\d+)"),
            dlss: re(r"GPU: DLSS Support = (\w+)"),
            cpu_bench: re(r"CPU benchmark: ([\d.]+) ms \(int\+mem\), ([\d.]+) ms \(fp\+mem\)"),
            gpu_bench: re(r"GPU benchmark: ([\d.]+) ms"),
            index: re(r"Performance Index: ([\d.]+) \(CPU\), ([\d.]+) \(GPU\)"),
            class: re(r"(?:<(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})[^>]*> )?Final rating: Machine class (\d+)"),
            first_adapter: re(r"(?m)^<[^>]*> - (.+?) \(vendor = "),
        }
    })
}

/// `str.strip` of Python.
fn strip(text: &str) -> String {
    crate::text::py_strip(text).to_string()
}

/// A class with the time of its line (UTC) where the line has one.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Rating {
    class: i64,
    at: Option<i64>,
}

/// Unix seconds of a UTC date and time.
fn unix_seconds(year: i64, month: i64, day: i64, hour: i64, minute: i64, second: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    days * 86_400 + hour * 3600 + minute * 60 + second
}

/// The rating a match of the class pattern says; `None` for a class outside 1 to 5.
fn rating_of(caps: &regex::Captures) -> Option<Rating> {
    let class: i64 = caps[7].parse().ok()?;
    if !(1..=CLASSES).contains(&class) {
        return None;
    }
    let field = |n: usize| caps.get(n).and_then(|m| m.as_str().parse::<i64>().ok());
    let at = (|| Some(unix_seconds(field(1)?, field(2)?, field(3)?, field(4)?, field(5)?, field(6)?)))();
    Some(Rating { class, at })
}

/// The last rating in the text, if the very last one is a valid class (1 to 5).
fn last_rating(text: &str) -> Option<Rating> {
    rating_of(&patterns().class.captures_iter(text).last()?)
}

/// Seconds of a time of change in ns.
fn seconds_of(nanos: u128) -> Option<i64> {
    i64::try_from(nanos / 1_000_000_000).ok()
}

/// The values of a log text; what is missing stays `None`.
pub fn parse(text: &str) -> Info {
    let p = patterns();
    let mut out = Info::default();
    let number = |caps: &regex::Captures, n: usize| caps[n].parse::<i64>().ok();
    if let Some(c) = p.cpu.captures(text) {
        out.cpu = Some(strip(&c[1]));
    }
    if let Some(c) = p.threads.captures(text) {
        out.threads = number(&c, 1);
    }
    if let Some(c) = p.ram.captures(text) {
        out.ram_mb = number(&c, 1);
    }
    if let Some(c) = p.gpu_vk.captures(text) {
        out.gpu = Some(strip(&c[1]));
        out.driver = Some(strip(&c[2]));
        out.vulkan = Some(strip(&c[3]));
    } else if let Some(c) = p.first_adapter.captures(text) {
        out.gpu = Some(strip(&c[1]));
    }
    if let Some(c) = p.vram.captures(text) {
        out.vram_mb = number(&c, 1);
    }
    if let Some(c) = p.display.captures(text) {
        out.display = number(&c, 1).zip(number(&c, 2));
    }
    if let Some(c) = p.dlss.captures(text) {
        out.dlss = Some(&c[1] == "Available");
    }
    if let Some(c) = p.cpu_bench.captures(text) {
        out.cpu_bench = c[1].parse::<f64>().ok().zip(c[2].parse::<f64>().ok());
    }
    if let Some(c) = p.gpu_bench.captures(text) {
        out.gpu_bench = c[1].parse().ok();
    }
    if let Some(c) = p.index.captures(text) {
        out.cpu_index = c[1].parse().ok();
        out.gpu_index = c[2].parse().ok();
    }
    if let Some(rating) = last_rating(text) {
        out.machine_class = Some(rating.class);
        out.class_at = rating.at;
    }
    out
}

/// The state of a file: path, time of change in ns and size. The key under which what was read is kept.
type Stamp = (String, u128, u64);

fn stamp(path: &str) -> std::io::Result<Stamp> {
    let meta = std::fs::metadata(path)?;
    let nanos = meta.modified()?.duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    Ok((path.to_string(), nanos, meta.len()))
}

/// What was read of the running log, and the classes of the kept ones, by the state of the file: a changed
/// file is read again. (The page reads in threads of its own; whole logs are searched rarely.)
struct Memo {
    log: Option<(Stamp, Info)>,
    classes: HashMap<Stamp, Option<Rating>>,
}

fn memo() -> &'static Mutex<Memo> {
    static MEMO: OnceLock<Mutex<Memo>> = OnceLock::new();
    MEMO.get_or_init(|| Mutex::new(Memo { log: None, classes: HashMap::new() }))
}

fn lock() -> std::sync::MutexGuard<'static, Memo> {
    memo().lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn text_of(path: &str, limit: Option<usize>) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    match limit {
        Some(limit) => file.take(limit as u64).read_to_end(&mut bytes)?,
        None => file.read_to_end(&mut bytes)?,
    };
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// The last valid rating in a kept log, `None` where there is none or it cannot be read. A line without a
/// time takes the time of change of the file.
fn class_in(path: &str) -> std::io::Result<Option<Rating>> {
    let key = stamp(path)?;
    if let Some(found) = lock().classes.get(&key) {
        return Ok(*found);
    }
    let text = text_of(path, None)?;
    let found = patterns()
        .class
        .captures_iter(&text)
        .collect::<Vec<_>>()
        .iter()
        .rev()
        .find_map(rating_of)
        .map(|rating| Rating { at: rating.at.or_else(|| seconds_of(key.1)), ..rating });
    lock().classes.insert(key, found);
    Ok(found)
}

/// The newest rating of the kept logs.
fn class_from_backups(paths: &Paths, folder: Option<&str>) -> Option<Rating> {
    let mut backups = paths.log_backups(folder);
    backups.reverse();
    backups.truncate(BACKUPS_SEARCHED);
    // Only what is still searched is kept in mind, or the memo would grow with every start of the game.
    lock().classes.retain(|(path, _, _), _| backups.contains(path));
    for path in &backups {
        // A log that cannot be read is passed over.
        if let Ok(Some(found)) = class_in(path) {
            return Some(found);
        }
    }
    None
}

/// The class kept in [`NOTE_FILE`] and when it was found.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Note {
    class: i64,
    at: i64,
}

/// The note in the data folder; `None` where the file is missing, cannot be read or says nothing usable.
fn note_read(paths: &Paths) -> Option<Note> {
    let data = jsonio::read_json(&paths.app_file(NOTE_FILE)).ok()?;
    if data.get("format") != Some(&Value::int(NOTE_FORMAT)) {
        return None;
    }
    let number = |key: &str| match data.get(key) {
        Some(Value::Int(text)) => text.parse::<i64>().ok(),
        _ => None,
    };
    let class = number("klasse").filter(|class| (1..=CLASSES).contains(class))?;
    Some(Note { class, at: number("gefunden_am")? })
}

fn note_write(paths: &Paths, note: Note) {
    let value = Value::Object(vec![
        ("format".to_string(), Value::int(NOTE_FORMAT)),
        ("klasse".to_string(), Value::int(note.class)),
        ("gefunden_am".to_string(), Value::int(note.at)),
    ]);
    let text = jsonio::dumps(&value, Some(2), false);
    if let Err(error) = jsonio::write_text_atomic(&paths.app_file(NOTE_FILE), &text, jsonio::Newline::Native) {
        errors::record("machine_info.note", &error);
    }
}

/// Brings the class found in the logs and the note together: a class found that is not older than the note is
/// noted (written only when it differs); without one, or with an older one, the class of the note is taken and
/// marked as remembered.
fn settle(paths: &Paths, info: &mut Info) {
    let kept = note_read(paths);
    if let Some(class) = info.machine_class {
        let found = Note { class, at: info.class_at.unwrap_or(0) };
        if kept.is_none_or(|kept| found.at >= kept.at) {
            if kept != Some(found) {
                note_write(paths, found);
            }
            return;
        }
    }
    if let Some(kept) = kept {
        info.machine_class = Some(kept.class);
        info.class_at = Some(kept.at);
        info.class_remembered = true;
    }
}

/// The values from the running `Game.log` (the hardware from the start block, the class from the whole log or
/// the kept ones). `None` without a log.
pub fn read(paths: &Paths, folder: Option<&str>) -> Option<Info> {
    let log = paths.game_log(folder)?;
    let result = (|| -> std::io::Result<Info> {
        let key = stamp(&log)?;
        if let Some((seen, info)) = &lock().log
            && *seen == key
        {
            return Ok(info.clone());
        }
        let mut info = parse(&text_of(&log, Some(HEAD_BYTES))?);
        if info.machine_class.is_none() && key.2 > HEAD_BYTES as u64 {
            let whole = parse(&text_of(&log, None)?);
            info.machine_class = whole.machine_class;
            info.class_at = whole.class_at;
        }
        if info.machine_class.is_some() && info.class_at.is_none() {
            info.class_at = seconds_of(key.1);
        }
        lock().log = Some((key, info.clone()));
        Ok(info)
    })();
    let mut info = match result {
        Ok(info) => info,
        Err(error) => {
            errors::record("machine_info.read", &error);
            return None;
        }
    };
    if info.machine_class.is_none()
        && let Some(rating) = class_from_backups(paths, folder)
    {
        info.machine_class = Some(rating.class);
        info.class_at = rating.at;
    }
    settle(paths, &mut info);
    Some(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    const START: &str = "<2026-10-22T10:00:00.000Z> Host CPU: AMD Ryzen 7 5800X3D 8-Core Processor    \n\
<2026-10-22T10:00:00.001Z> Logical CPU Count: 16\n\
<2026-10-22T10:00:00.002Z> 32678MB physical memory installed\n\
<2026-10-22T10:00:01.000Z> [VK] Chosen Vulkan GPU Device (NVIDIA GeForce RTX 4070) Driver Version (560.94) Vulkan API (1.3.280)\n\
<2026-10-22T10:00:01.001Z> GPU: DedicatedVidMemMB = 12282\n\
<2026-10-22T10:00:01.002Z> Current display mode is 2560x1440\n\
<2026-10-22T10:00:01.003Z> GPU: DLSS Support = Available\n\
<2026-10-22T10:00:02.000Z> CPU benchmark: 12.5 ms (int+mem), 9.25 ms (fp+mem)\n\
<2026-10-22T10:00:02.001Z> GPU benchmark: 3.5 ms\n\
<2026-10-22T10:00:02.002Z> Performance Index: 8.5 (CPU), 12.75 (GPU)\n\
<2026-10-22T10:00:02.003Z> Final rating: Machine class 4\n";

    #[test]
    fn the_start_block_is_read_value_by_value() {
        let info = parse(START);
        assert_eq!(info.cpu.as_deref(), Some("AMD Ryzen 7 5800X3D 8-Core Processor"), "blanks at the end go");
        assert_eq!((info.threads, info.ram_mb, info.vram_mb), (Some(16), Some(32678), Some(12282)));
        assert_eq!(info.gpu.as_deref(), Some("NVIDIA GeForce RTX 4070"));
        assert_eq!((info.driver.as_deref(), info.vulkan.as_deref()), (Some("560.94"), Some("1.3.280")));
        assert_eq!(info.display, Some((2560, 1440)));
        assert_eq!(info.dlss, Some(true));
        assert_eq!(info.cpu_bench, Some((12.5, 9.25)));
        assert_eq!(info.gpu_bench, Some(3.5));
        assert_eq!((info.cpu_index, info.gpu_index), (Some(8.5), Some(12.75)));
        assert_eq!(info.machine_class, Some(4));
    }

    #[test]
    fn what_the_log_does_not_say_stays_empty_and_the_card_comes_from_the_adapter_list_without_vulkan() {
        assert_eq!(parse(""), Info::default());
        let info = parse(
            "<2026-10-22T10:00:00Z> - NVIDIA GeForce GTX 1060 (vendor = 0x10de)\nGPU: DLSS Support = Unsupported\n",
        );
        assert_eq!(info.gpu.as_deref(), Some("NVIDIA GeForce GTX 1060"));
        assert_eq!(info.driver, None);
        assert_eq!(info.dlss, Some(false));
    }

    #[test]
    fn only_the_last_rating_counts_and_only_when_it_is_one_to_five() {
        assert_eq!(parse("Final rating: Machine class 2\nFinal rating: Machine class 5\n").machine_class, Some(5));
        assert_eq!(
            parse("Final rating: Machine class 3\nFinal rating: Machine class 9\n").machine_class,
            None,
            "the last is invalid"
        );
        assert_eq!(parse("Final rating: Machine class 0\n").machine_class, None);
    }

    fn scratch(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("versekit-machine-{}-{name}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        dir.to_string_lossy().replace('\\', "/")
    }

    fn paths_in(home: &str, game: &str) -> Paths {
        use crate::paths::{ENV_INSTALL_DIR, Host};
        Paths::new(Host::new(
            crate::pypath::Flavor::Posix,
            &[("SC_BP_HOME", home), ("HOME", home), (ENV_INSTALL_DIR, game)],
            home,
        ))
    }

    #[test]
    fn the_class_is_searched_in_the_whole_log_and_then_in_the_kept_ones_newest_first() {
        let home = scratch("klasse");
        let game = format!("{home}/StarCitizen/LIVE");
        let kept = format!("{game}/logbackups");
        std::fs::create_dir_all(&kept).expect("folder");
        let head_without_class = START.replace("Final rating: Machine class 4\n", "");
        // The class stands behind the start block, past what is read first.
        let filler = "x".repeat(HEAD_BYTES);
        std::fs::write(
            format!("{game}/Game.log"),
            format!("{head_without_class}{filler}\nFinal rating: Machine class 3\n"),
        )
        .unwrap();
        let paths = paths_in(&home, &game);
        let info = read(&paths, None).expect("a log");
        assert_eq!(info.machine_class, Some(3), "from the whole log");
        assert_eq!(info.cpu.as_deref(), Some("AMD Ryzen 7 5800X3D 8-Core Processor"));
        // Without a class in the running log the kept ones are asked, the newest first.
        std::fs::write(format!("{game}/Game.log"), &head_without_class).unwrap();
        std::fs::write(format!("{kept}/Game Build(1) 22 Oct 26 (10 00 00).log"), "Final rating: Machine class 2\n")
            .unwrap();
        std::fs::write(format!("{kept}/Game Build(1) 23 Oct 26 (10 00 00).log"), "Final rating: Machine class 5\n")
            .unwrap();
        assert_eq!(read(&paths, None).expect("a log").machine_class, Some(5));
        assert!(read(&paths_in(&home, &format!("{home}/nowhere")), None).is_none(), "no log: nothing");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_date_is_turned_into_unix_seconds() {
        assert_eq!(unix_seconds(1970, 1, 1, 0, 0, 0), 0);
        assert_eq!(unix_seconds(1970, 1, 2, 0, 0, 1), 86_401);
        assert_eq!(unix_seconds(2000, 1, 1, 0, 0, 0), 946_684_800);
        assert_eq!(unix_seconds(2000, 3, 1, 0, 0, 0), 951_868_800, "after the leap day of 2000");
        assert_eq!(unix_seconds(2026, 10, 22, 10, 0, 0), 1_792_663_200);
        assert_eq!(parse(START).class_at, Some(1_792_663_202), "the time of the line of the rating (10:00:02)");
    }

    /// A game folder with the start block of a log, with or without the rating; returns home, game folder, paths.
    fn rated_log(name: &str, with_class: bool) -> (String, String, Paths) {
        let home = scratch(name);
        let game = format!("{home}/StarCitizen/LIVE");
        std::fs::create_dir_all(&game).expect("folder");
        let text = if with_class { START.to_string() } else { START.replace("Final rating: Machine class 4\n", "") };
        std::fs::write(format!("{game}/Game.log"), text).unwrap();
        let paths = paths_in(&home, &game);
        (home, game, paths)
    }

    fn note_text(paths: &Paths) -> String {
        std::fs::read_to_string(paths.app_file(NOTE_FILE)).unwrap_or_default().replace("\r\n", "\n")
    }

    #[test]
    fn a_class_found_in_the_log_is_noted_and_comes_back_when_no_log_has_one() {
        let (home, game, paths) = rated_log("klasse-gemerkt", true);
        assert!(note_text(&paths).is_empty(), "nothing noted before the first reading");
        let info = read(&paths, None).expect("a log");
        assert_eq!((info.machine_class, info.class_remembered), (Some(4), false), "fresh from the log");
        assert_eq!(
            note_text(&paths),
            "{\n  \"format\": 1,\n  \"klasse\": 4,\n  \"gefunden_am\": 1792663202\n}",
            "the class and the time of its line"
        );
        // The game starts anew and the log has no rating: the noted one is taken.
        std::fs::write(format!("{game}/Game.log"), START.replace("Final rating: Machine class 4\n", "")).unwrap();
        let info = read(&paths, None).expect("a log");
        assert_eq!(info.machine_class, Some(4));
        assert_eq!(info.class_at, Some(1_792_663_202));
        assert!(info.class_remembered);
        assert_eq!(info.cpu.as_deref(), Some("AMD Ryzen 7 5800X3D 8-Core Processor"), "the rest is still read");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_class_found_later_replaces_the_note_and_an_older_one_does_not() {
        let (home, game, paths) = rated_log("klasse-neuer", false);
        std::fs::write(paths.app_file(NOTE_FILE), r#"{"format": 1, "klasse": 2, "gefunden_am": 1792000000}"#).unwrap();
        let newer = "<2026-10-22T10:00:02.003Z> Final rating: Machine class 5\n";
        std::fs::write(format!("{game}/Game.log"), format!("{START}{newer}")).unwrap();
        let info = read(&paths, None).expect("a log");
        assert_eq!((info.machine_class, info.class_remembered), (Some(5), false));
        assert!(note_text(&paths).contains("\"klasse\": 5"), "{}", note_text(&paths));
        // A rating of the log that is older than the note does not take its place.
        let older = "<2026-10-01T10:00:02.003Z> Final rating: Machine class 1\n";
        std::fs::write(format!("{game}/Game.log"), older).unwrap();
        let info = read(&paths, None).expect("a log");
        assert_eq!((info.machine_class, info.class_remembered), (Some(5), true));
        assert!(note_text(&paths).contains("\"klasse\": 5"), "the note stays");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn without_a_usable_note_and_without_a_rating_the_class_stays_empty() {
        let (home, _game, paths) = rated_log("klasse-leer", false);
        let unrated = |paths: &Paths| {
            let info = read(paths, None).expect("a log");
            (info.machine_class, info.class_at, info.class_remembered)
        };
        assert_eq!(unrated(&paths), (None, None, false), "no file");
        let file = paths.app_file(NOTE_FILE);
        for broken in [
            "not json at all",
            "",
            r#"{"format": 2, "klasse": 3, "gefunden_am": 1792000000}"#,
            r#"{"format": 1, "klasse": 9, "gefunden_am": 1792000000}"#,
            r#"{"format": 1, "klasse": 0, "gefunden_am": 1792000000}"#,
            r#"{"format": 1, "klasse": "3", "gefunden_am": 1792000000}"#,
            r#"{"format": 1, "klasse": 3}"#,
        ] {
            std::fs::write(&file, broken).unwrap();
            assert_eq!(unrated(&paths), (None, None, false), "{broken}");
        }
        std::fs::write(&file, r#"{"format": 1, "klasse": 3, "gefunden_am": 1792000000}"#).unwrap();
        assert_eq!(unrated(&paths), (Some(3), Some(1_792_000_000), true), "a usable file is taken");
        let _ = std::fs::remove_dir_all(home);
    }
}
