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

//! What the tiles of 4.12 show on real data (`overview.py`, `_tile_*`), each
//! as its key line and detail lines; a pinned page shows its short
//! explanation. A tile whose source v5 does not have yet is left out, and
//! the overview says it comes later. The files are read anew for every count,
//! as 4.12 reads them whenever the page is shown.

use versekit_core::language::SpawnWithLanguage as _;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use versekit_core::jsonio::{self, Value};
use versekit_core::language::{Arg, Lang, relative_age_in, t_in, tf_in};
use versekit_core::paths::Paths;
use versekit_core::{
    blueprints, collection, formats, location, machine_info, net, overview, playtime, refinery_jobs, serverstatus,
    stats_report, stock, translation,
};

use super::SamplePage;
use super::glance::{Glance, TileAction, Tone};

/// The entries of the collection (`bestand.json`); empty without one.
pub(super) fn own_blueprints(paths: &Paths) -> Vec<(String, Value)> {
    match jsonio::read_json(&paths.app_file(collection::FILE_NAME)) {
        Ok(Value::Object(data)) => collection::blueprints(&data).to_vec(),
        _ => Vec::new(),
    }
}

fn glance(line: String, details: Vec<String>) -> Glance {
    Glance::new(line, details)
}

/// The Executive Hangar at `now` (Unix seconds) for the start the page of the Contested Zones keeps: the phase
/// as the line in its colour (closed red, open in the accent, resetting gold), the five lamps, the time to the
/// next change and when it opens, or until when it stays open. The same calculation as the page; without a
/// start only the hint where to get one.
pub(super) fn hangar_tile(start: Option<i64>, now: f64, lang: Lang) -> Glance {
    use versekit_core::contested::{self, Phase};
    let Some(start) = start else {
        return glance(t_in(lang, "s_uv_hangar_ohne"), Vec::new()).line_tone(Tone::Warn);
    };
    let hangar = contested::hangar_at(start, now);
    let (line, until, tone) = match hangar.phase {
        Phase::Closed => ("s_cz_geschlossen", "ov_hangar_oeffnet", Tone::Bad),
        Phase::Open => ("s_cz_offen", "ov_hangar_schliesst", Tone::Good),
        Phase::Resetting => ("s_cz_reset", "ov_hangar_reset", Tone::Warn),
    };
    let left = tf_in(lang, until, &[("0", Arg::from(contested::countdown(hangar.to_change)))]);
    let when = match hangar.phase {
        Phase::Open => tf_in(
            lang,
            "s_uv_hangar_offen_bis",
            &[("0", Arg::from(contested::clock(now.floor() as i64 + hangar.to_change)))],
        ),
        _ => {
            let opens = contested::next_openings(start, now, 1).first().map_or(0, |opening| opening.opens);
            tf_in(lang, "s_uv_hangar_oeffnet", &[("0", Arg::from(contested::clock(opens)))])
        }
    };
    glance(t_in(lang, line), vec![left, when]).line_tone(tone).lamps(hangar.lamps)
}

/// The play sessions as saved, with the running one, merged where they overlap, oldest first
/// (`playtime.total`); a session without length is none.
pub(super) fn played_spans(paths: &Paths) -> Vec<(i64, i64)> {
    let mut spans = playtime::spans(paths);
    spans.extend(playtime::running_span(paths));
    spans.retain(|(from, to)| to > from);
    playtime::merge_spans(&spans)
}

/// The least time between two readings of the game log by one tile.
const LOOK_EVERY: std::time::Duration = std::time::Duration::from_secs(15);

/// What a tile reads from the game log in a thread of its own (it may read many megabytes): again when the
/// log changed and [`LOOK_EVERY`] has passed since the last reading, the last answer in between.
///
/// ⚠ While the game runs its log changes all the time; without the pause the next reading would start as soon
/// as the last one ended.
#[derive(Clone)]
pub(super) struct LogLook<T> {
    /// The last answer: `None` before the first.
    found: Arc<Mutex<Option<T>>>,
    busy: Arc<AtomicBool>,
    /// Size and time of the log when it was read last.
    stamp: Arc<Mutex<Option<(u64, std::time::SystemTime)>>>,
    /// When the last reading started.
    started: Arc<Mutex<Option<std::time::Instant>>>,
}

impl<T> Default for LogLook<T> {
    fn default() -> Self {
        LogLook { found: Arc::default(), busy: Arc::default(), stamp: Arc::default(), started: Arc::default() }
    }
}

/// Where the player is: the place, or none in the log.
pub(super) type PlaceLook = LogLook<Option<location::Place>>;
/// How the game rated the computer: the values, or none without a log.
pub(super) type MachineLook = LogLook<Option<machine_info::Info>>;

impl<T: Clone + Send + 'static> LogLook<T> {
    /// The last answer, and a new reading by `read` started when the log changed since and the pause is over.
    /// The reading works on a copy of `paths` with the folders given from outside (a game folder of the
    /// command line).
    fn poll(&self, paths: &Paths, read: fn(&Paths) -> T) -> Option<T> {
        self.poll_every(paths, read, LOOK_EVERY)
    }

    fn poll_every(&self, paths: &Paths, read: fn(&Paths) -> T, pause: std::time::Duration) -> Option<T> {
        let stamp = paths
            .game_log(None)
            .and_then(|log| std::fs::metadata(log).ok())
            .and_then(|m| Some((m.len(), m.modified().ok()?)));
        let due = {
            let mut seen = self.stamp.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut started = self.started.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let changed = *seen != stamp || stamp.is_none();
            let rested = started.is_none_or(|at| at.elapsed() >= pause);
            let due = changed && rested && !self.busy.load(Ordering::Acquire);
            if due {
                *seen = stamp;
                *started = Some(std::time::Instant::now());
            }
            due
        };
        if due && !self.busy.swap(true, Ordering::AcqRel) {
            let (found, busy, own) = (self.found.clone(), self.busy.clone(), paths.working_copy());
            let started = std::thread::Builder::new().name("uebersicht-spielprotokoll".into()).spawn_with_language(move || {
                let answer = read(&own);
                *found.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(answer);
                busy.store(false, Ordering::Release);
            });
            if started.is_err() {
                self.busy.store(false, Ordering::Release);
            }
        }
        self.found.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }
}

/// The state of the servers of CIG for the services tile: the one kept, and the page asked again in a
/// thread of its own when it is older than five minutes (`serverstatus.state`).
#[derive(Clone, Default)]
pub(super) struct StatusLook {
    busy: Arc<AtomicBool>,
    /// When the page was asked last (seconds), so that a page that cannot be reached is not asked at
    /// every count.
    tried: Arc<Mutex<f64>>,
}

impl StatusLook {
    /// The state kept; a new asking started when it is old and the last try lies five minutes back.
    pub(super) fn poll(&self, paths: &Paths, now: f64) -> Option<serverstatus::Overall> {
        let kept = serverstatus::stored(paths);
        let stale = kept.as_ref().is_none_or(|k| now - k.fetched >= serverstatus::FRESH_SECONDS);
        let due = {
            let mut tried = self.tried.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let due = stale && now - *tried >= serverstatus::FRESH_SECONDS && !net::offline_in(&paths.host);
            if due {
                *tried = now;
            }
            due
        };
        if due && !self.busy.swap(true, Ordering::AcqRel) {
            let (busy, own) = (self.busy.clone(), paths.working_copy());
            let agent = versekit_core::basetool::config::user_agent(env!("CARGO_PKG_VERSION"));
            let started = std::thread::Builder::new().name("uebersicht-serverstatus".into()).spawn_with_language(move || {
                let web = net::Web::new(&agent, std::time::Duration::from_secs(15), 1 << 20);
                serverstatus::refresh(&own, &web, now);
                busy.store(false, Ordering::Release);
            });
            if started.is_err() {
                self.busy.store(false, Ordering::Release);
            }
        }
        kept
    }
}

/// A moment for a tile (`_clock`): the time when it was today, else day and time.
pub(super) fn clock(lang: Lang, seconds: i64, now: i64) -> String {
    let day = |at: i64| formats::local_time(at).map(|t| formats::strftime("%Y-%m-%d", &t));
    match formats::local_time(seconds) {
        Some(at) if day(seconds) == day(now) => formats::strftime_shown("%H:%M", &at),
        _ => stats_report::local_moment(lang, seconds),
    }
}

/// How many detail lines a tile offers at most where 4.12 showed three; the tile shows as many as it has
/// room for, so a bigger one shows more.
const MOST: usize = 12;
/// The lines after the title that the notes tile shows of the memo edited last.
const NOTE_LINES: usize = 3;

/// The blueprint tile: the number owned as its key figure, the share owned as a bar, and what came this week (the
/// first detail line) in the accent where there is something (4.12).
fn blueprint_tile(line: String, details: Vec<String>, mine: usize, total: usize, week: usize) -> Glance {
    let tile = glance(line, details).bar((mine as f64 / total.max(1) as f64) as f32).figure(mine);
    if week > 0 { tile.tone_at(0, Tone::Good) } else { tile }
}

/// The blueprint tile with the blueprints of another account that did not count: their number as the first
/// detail line, in gold (the tile leads to the own account then). Nothing changes without any.
fn with_foreign(mut tile: Glance, foreign: usize, lang: Lang) -> Glance {
    if foreign == 0 {
        return tile;
    }
    let key = if foreign == 1 { "s_uv_bp_fremd_1" } else { "s_uv_bp_fremd" };
    tile.details.insert(0, tf_in(lang, key, &[("0", Arg::from(foreign))]));
    let tones: Vec<Tone> = (0..tile.details.len() - 1).map(|at| tile.tone_of(at)).collect();
    tile.tones = std::iter::once(Tone::Warn).chain(tones).collect();
    tile
}

/// The color of the state of the servers: the accent where all runs, gold where something is limited, red where
/// they are down (4.12 knew the accent and gold).
fn cig_tone(word: &str) -> Tone {
    match word {
        "operational" => Tone::Good,
        "major_outage" => Tone::Bad,
        _ => Tone::Warn,
    }
}

/// The word of CIG for the state of the servers in the language of the program; one it does not know
/// stands as CIG wrote it.
fn cig_word(lang: Lang, word: &str) -> String {
    let key = match word {
        "operational" => "s_kt_cig_operational",
        "degraded_performance" => "s_kt_cig_degraded_performance",
        "partial_outage" => "s_kt_cig_partial_outage",
        "major_outage" => "s_kt_cig_major_outage",
        "under_maintenance" | "maintenance" => "s_kt_cig_under_maintenance",
        _ => return word.to_string(),
    };
    t_in(lang, key)
}

/// The location tile (`_tile_location`): the place and since when, or travelling and from where.
pub(super) fn place_tile(
    place: Option<Option<location::Place>>,
    running: bool,
    paths: &Paths,
    lang: Lang,
    now: i64,
) -> Glance {
    let names = || location::names(paths);
    match place {
        None => glance(t_in(lang, "s_uv_lese"), Vec::new()),
        Some(None) | Some(Some(location::Place { number: None, .. })) => {
            glance(t_in(lang, "s_uv_ort_leer"), Vec::new())
        }
        Some(Some(p)) if p.travelling => {
            let details = match (&p.before, p.since) {
                (Some(before), Some(since)) => {
                    let from = Arg::from(location::display_name(before, &names()));
                    vec![tf_in(lang, "s_uv_ort_ab", &[("0", from), ("1", Arg::from(clock(lang, since, now)))])]
                }
                _ => Vec::new(),
            };
            glance(t_in(lang, "s_uv_unterwegs"), details)
        }
        Some(Some(location::Place { key: None, .. })) => {
            glance(t_in(lang, "s_uv_ort_unbekannt"), vec![t_in(lang, "s_uv_ort_lernen")])
        }
        Some(Some(location::Place { key: Some(key), since, .. })) => {
            let when = since.map(|at| {
                let word = if running { "s_uv_ort_seit" } else { "s_uv_ort_zuletzt" };
                tf_in(lang, word, &[("0", Arg::from(clock(lang, at, now)))])
            });
            glance(location::display_name(&key, &names()), when.into_iter().collect())
        }
    }
}

/// The computer tile (`_tile_machine`): how the game rated it, and its performance index; after it the course of
/// the index on the days noted before `today` (`YYYY-MM-DD`), newest first, muted.
pub(super) fn machine_tile(
    info: Option<Option<machine_info::Info>>,
    history: &[machine_info::history::Day],
    today: &str,
    lang: Lang,
) -> Glance {
    let Some(read) = info else { return glance(t_in(lang, "s_uv_lese"), Vec::new()) };
    let Some((class, info)) = read.and_then(|info| info.machine_class.map(|class| (class, info))) else {
        return glance(t_in(lang, "s_gr_klasse_leer"), Vec::new());
    };
    let index = |value: Option<f64>| value.map_or_else(|| "?".to_string(), |v| format!("{v:.0}"));
    let mut details: Vec<String> = info
        .cpu_index
        .map(|cpu| {
            tf_in(lang, "s_uv_index", &[("0", Arg::from(index(Some(cpu)))), ("1", Arg::from(index(info.gpu_index)))])
        })
        .into_iter()
        .collect();
    let now_lines = details.len();
    details.extend(
        history.iter().rev().filter(|day| day.day.as_str() < today).take(MOST).map(|day| history_line(day, lang)),
    );
    // The class as the five segments of 4.12, lit from red to the accent.
    let class = class.clamp(1, machine_info::CLASSES);
    let tile = glance(t_in(lang, &format!("s_gr_stufe_{class}")), details)
        .bar(class as f32 / machine_info::CLASSES as f32)
        .steps(machine_info::CLASSES as u8);
    let shown = tile.details.len();
    (now_lines..shown).fold(tile, |tile, at| tile.tone_at(at, Tone::Muted))
}

/// One day of the course of the index: the date as the language writes it, both indices with one decimal.
fn history_line(day: &machine_info::history::Day, lang: Lang) -> String {
    let decimal = formats::decimal_mark(lang).to_string();
    let number =
        |value: Option<f64>| value.map_or_else(|| "?".to_string(), |v| format!("{v:.1}").replace('.', &decimal));
    let date = match (lang, day.day.split('-').collect::<Vec<_>>().as_slice()) {
        (Lang::De, [year, month, date]) => format!("{date}.{month}.{year}"),
        _ => day.day.clone(),
    };
    tf_in(
        lang,
        "s_uv_verlauf_tag",
        &[("0", Arg::from(date)), ("1", Arg::from(number(Some(day.cpu)))), ("2", Arg::from(number(day.gpu)))],
    )
}

/// The refinery tile with its link "Scannen" where the screen can be read: the jobs, or without any the short
/// explanation that the hotkey at the terminal starts the time (the combination as the settings hold it).
pub(super) fn refinery_tile_with_scan(paths: &Paths, lang: Lang, now: i64) -> Glance {
    let tile = refinery_tile(paths, lang, now).unwrap_or_else(|| {
        let combination = paths
            .setting(versekit_core::overlay::keys::HOTKEY_SCAN)
            .unwrap_or_else(|| versekit_core::overlay::DEFAULT_SCAN_HOTKEY.to_string());
        let shown = hotkey_in(lang, &combination);
        glance(tf_in(lang, "s_uv_raff_leer", &[("0", Arg::from(shown))]), Vec::new())
    });
    let action = versekit_core::refinery_scan::screen::supported().then_some(TileAction::RefineryScan);
    Glance { action, ..tile }
}

/// A combination of keys with the words of the keys in `lang` (`Strg` is `Ctrl` in English); one the overlay would
/// not take stays as it is written.
pub(super) fn hotkey_in(lang: Lang, text: &str) -> String {
    let Some(hotkey) = versekit_core::overlay::Hotkey::parse(text) else { return text.to_string() };
    hotkey
        .text()
        .split('+')
        .map(|part| match part {
            "Strg" => t_in(lang, "s_ei_taste_strg"),
            "Umschalt" => t_in(lang, "s_ei_taste_umschalt"),
            other => super::hotkey_word(other).map_or_else(|| other.to_string(), |word| t_in(lang, word)),
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// The refinery tile: the jobs running and done, with the time left; `None` without any job.
pub(super) fn refinery_tile(paths: &Paths, lang: Lang, now: i64) -> Option<Glance> {
    let (running, done) = overview::refinery_current(&refinery_jobs::load(paths), now as f64);
    if running.is_empty() && done.is_empty() {
        return None;
    }
    // "N laufen · M fertig" has no singular: the numbers stand after their words.
    let count = tf_in(lang, "s_kt_raff_zahl", &[("0", Arg::from(running.len())), ("1", Arg::from(done.len()))]);
    // 4.12 showed three jobs; the tile shows as many as its size has room for.
    let shown = running.iter().take(MOST).chain(done.iter().take(MOST.saturating_sub(running.len())));
    let shown: Vec<&versekit_core::jsonio::Map> = shown.collect();
    // A job that is done and waits to be fetched stands in the accent (4.12).
    let tones: Vec<Tone> =
        shown.iter().map(|job| if overview::job_ends(job) > now as f64 { Tone::Plain } else { Tone::Good }).collect();
    let details = shown
        .into_iter()
        .map(|job| {
            let ends = overview::job_ends(job);
            let state = if ends > now as f64 {
                let at =
                    formats::local_time(ends as i64).map_or_else(String::new, |t| formats::strftime_shown("%H:%M", &t));
                let left = Arg::from(overview::countdown_text(ends - now as f64));
                tf_in(lang, "s_uv_raff_noch", &[("0", left), ("1", Arg::from(at))])
            } else {
                t_in(lang, "s_uv_raff_fertig")
            };
            // Every material by name: the label cuts a long list with "…", and a tile cuts no text.
            format!("{} · {state}", refinery_jobs::job_label(job, 12))
        })
        .collect();
    // The jobs running are the key figure (4.12).
    Some(Glance { tones, ..glance(count, details).figure(running.len()) })
}

/// The storage tile: how much of each kind lies there, and the `most` largest stocks with their place.
/// 4.12 showed three; the tile of the page shows as many as the size of the tile has room for.
pub(super) fn storage_tile(rows: &[stock::Row], lang: Lang, most: usize) -> Glance {
    if rows.is_empty() {
        return glance(t_in(lang, "s_uv_lager_leer"), Vec::new());
    }
    let (materials, goods, items) = overview::storage_counts(rows);
    // "1 Rohstoffe" of 4.12 had no singular: the numbers stand after their words.
    let line = tf_in(
        lang,
        "s_kt_lager_posten",
        &[("0", Arg::from(materials)), ("1", Arg::from(goods)), ("2", Arg::from(items))],
    );
    let decimal = formats::decimal_mark(lang);
    let details = overview::storage_top(rows, most)
        .into_iter()
        .map(|(name, amount, place, piece)| {
            let unit = if piece { t_in(lang, "s_lg_stueck") } else { "SCU".to_string() };
            let amount = stock::amount_text(amount, piece, decimal);
            let text =
                tf_in(lang, "s_uv_menge", &[("0", Arg::from(name)), ("1", Arg::from(amount)), ("2", Arg::from(unit))]);
            if place.is_empty() { text } else { format!("{text} · {place}") }
        })
        .collect();
    // All that is different in storage is the key figure (4.12).
    glance(line, details).figure(materials + goods + items)
}

/// The tile of the open contracts that give blueprints (`_tile_contracts`): each running title once, with the
/// blueprints its mission gives and how many of them are missing, found by the title as 4.12 asks
/// (`contracts.check` without the contract id), the most missing first. `owned` tells a key of the collection.
pub(super) fn contracts_tile(
    running: &[String],
    blueprints: &versekit_core::mission_log::ContractBlueprints,
    owned: &dyn Fn(&str) -> bool,
    lang: Lang,
) -> Glance {
    let has = |name: &str| owned(&versekit_core::paths::name_key(name));
    let check = |title: &str| blueprints.check(title, &has, None).map(|(total, missing)| (total, missing.len()));
    let open = overview::open_contracts(running, &check);
    if open.is_empty() {
        return glance(t_in(lang, "s_uv_auftraege_leer"), Vec::new());
    }
    // What still misses blueprints stands in the accent (4.12).
    let tones: Vec<Tone> =
        open.iter().take(MOST).map(|(_, _, m)| if *m > 0 { Tone::Good } else { Tone::Plain }).collect();
    let details = open
        .iter()
        .take(MOST)
        .map(|(name, total, missing)| {
            let name = Arg::from(name.as_str());
            // The numbers stand after their words: "1 von 3 fehlen" has no singular.
            if *missing > 0 {
                tf_in(lang, "s_kt_auftrag_fehlt", &[("0", name), ("1", Arg::from(*missing)), ("2", Arg::from(*total))])
            } else {
                tf_in(lang, "s_kt_auftrag_alle", &[("0", name), ("1", Arg::from(*total))])
            }
        })
        .collect();
    let line = tf_in(lang, "s_uv_auftraege_zahl", &[("0", Arg::from(open.len()))]);
    Glance { tones, ..glance(line, details).figure(open.len()) }
}

impl SamplePage {
    /// The tiles of 4.12 and the pinned pages, counted now; `now` in seconds of the clock.
    pub(super) fn tiles_of_4_12(&self, paths: &Paths, lang: Lang, now: i64) -> HashMap<&'static str, Glance> {
        let mut out: HashMap<&'static str, Glance> = HashMap::new();
        let ago = |at: i64| relative_age_in(lang, (now - at) as f64);
        let one = |key: &str, value: Arg| tf_in(lang, key, &[("0", value)]);
        let two = |key: &str, a: Arg, b: Arg| tf_in(lang, key, &[("0", a), ("1", b)]);
        // Without a catalog the list holds samples: nothing of them is counted here.
        let real_catalog = !self.blueprints.sample;
        let catalog = &self.blueprints.catalog;
        let own = own_blueprints(paths);
        let own_keys: HashSet<&str> = own.iter().map(|(key, _)| key.as_str()).collect();
        let owned = |key: &str| own_keys.contains(key);

        // Game: running or not (the process list, else the log), the channel, the version of the game data.
        let running = versekit_core::overlay::game_running(
            versekit_core::game_start::game_process(),
            versekit_core::overlay::log_age(paths),
        );
        let mut details = Vec::new();
        if let Some(folder) = paths.game_folder() {
            let flavor = paths.host.flavor;
            details.push(one("s_uv_kanal", Arg::from(flavor.basename(folder.trim_end_matches(flavor.sep())))));
        }
        if real_catalog && !catalog.version.is_empty() {
            details.push(one("s_uv_stand", Arg::from(catalog.version.as_str())));
        }
        out.insert("spiel", glance(t_in(lang, if running { "s_uv_laeuft" } else { "s_uv_laeuft_nicht" }), details));

        // Location: the last landing place in the game log, read beside the window.
        let place = self.place_look.poll(paths, |own| location::place(own, None));
        out.insert("standort", place_tile(place, running, paths, lang, now));

        // Computer: the rating of the game from its log (`machine_info`), read beside the window; each reading
        // with an index is noted for the course over the days.
        let machine = self.machine_look.poll(paths, |own| {
            let info = machine_info::read(own, None);
            if let Some(found) = &info {
                machine_info::history::note_now(own, found);
            }
            info
        });
        let today = machine_info::history::day_of(now).unwrap_or_default();
        out.insert("rechner", machine_tile(machine, &machine_info::history::load(paths), &today, lang));

        // Blueprints: owned of the catalog, this week, the newest.
        if real_catalog {
            let keys: HashSet<&str> = catalog.blueprints.iter().map(|b| b.key.as_str()).collect();
            let total = catalog.blueprints.len();
            let numbers = overview::blueprint_numbers(&own, &|key| keys.contains(key), total, now);
            let percent = numbers.mine as f64 * 100.0 / total.max(1) as f64;
            let line = format!("{} {}", numbers.mine, two("s_uv_von", Arg::from(total), Arg::from(percent)));
            let mut details = vec![one("s_uv_woche", Arg::from(numbers.week))];
            if let Some(at) = numbers.newest_at.filter(|_| !numbers.newest.is_empty()) {
                details.push(two("s_uv_neuester", Arg::from(numbers.newest.as_str()), Arg::from(ago(at))));
            }
            // The larger tile also tells today and the month, counted as the list's period filter does.
            let came = overview::received(&own);
            details.push(one("s_uv_heute", Arg::from(overview::count_in(&came, overview::Period::Today, now))));
            details.push(one("s_uv_monat", Arg::from(overview::count_in(&came, overview::Period::Month, now))));
            // The share as a bar; what came this week in the accent where there is something (4.12).
            let tile = blueprint_tile(line, details, numbers.mine, total, numbers.week);
            let foreign = versekit_core::logsource::foreign_tally(paths).blueprints;
            out.insert("bauplaene", with_foreign(tile, foreign, lang));
        }

        // Translation: the source of each channel and how it stands.
        let today = translation::today();
        let source_line = |name: &str, source: &str, channel: Option<&str>| {
            if source.is_empty() {
                return (format!("{name}: {}", t_in(lang, "s_uv_keine_quelle")), String::new());
            }
            let status = translation::status_text(paths, source, channel, lang, &today);
            (format!("{name}: {}", translation::display_name(source)), status)
        };
        let main = paths.game_folder();
        let flavor = paths.host.flavor;
        let main_name = main
            .as_deref()
            .map(|f| flavor.basename(f.trim_end_matches(flavor.sep())))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "LIVE".to_string());
        let (line, status) =
            source_line(&main_name, &paths.setting(translation::SETTING_SOURCE).unwrap_or_default(), None);
        let mut details: Vec<String> = Some(status).filter(|s| !s.is_empty()).into_iter().collect();
        let sources = translation::channel_sources(paths);
        for (name, folder, p4k) in paths.installed_channels() {
            if !p4k || !translation::CHANNELS.contains(&name.as_str()) || main.as_deref() == Some(folder.as_str()) {
                continue;
            }
            let source = sources.iter().find(|(k, _)| *k == name).and_then(|(_, v)| v.as_str()).unwrap_or("");
            let (text, status) = source_line(&name, source, Some(&name));
            details.push(if status.is_empty() { text } else { format!("{text} · {status}") });
        }
        // "Jetzt nach neuer Übersetzung sehen" where a channel has a source to look at (4.12).
        let check = (!crate::game_texts_page::checkable(paths).is_empty()).then_some(TileAction::TranslationCheck);
        out.insert("uebersetzung", Glance { action: check, ..glance(line, details) });

        // Play time: all of it, and the last seven days.
        let spans = played_spans(paths);
        let total: i64 = spans.iter().map(|(from, to)| to - from).sum();
        let week = overview::playtime_week(&spans, now);
        // 4.12: the total and the last seven days; beside them today and the longest session.
        let midnight = formats::local_time(now).map_or(now - 86_400, |t| {
            now - (i64::from(t.hour) * 3600 + i64::from(t.minute) * 60 + i64::from(t.second))
        });
        let today: i64 = spans.iter().map(|(from, to)| ((*to).min(now) - (*from).max(midnight)).max(0)).sum();
        let longest = spans.iter().map(|(from, to)| to - from).max().unwrap_or(0);
        // All of it is the key figure, as the big number of 4.12.
        out.insert(
            "spielzeit",
            glance(
                playtime::as_text(total),
                vec![
                    one("s_uv_sieben_tage", Arg::from(playtime::as_text(week))),
                    one("s_kt_heute", Arg::from(playtime::as_text(today))),
                    tf_in(lang, "s_uv_laengste", &[("time", Arg::from(formats::duration(longest as f64)))]),
                ],
            )
            .figure(playtime::as_text(total)),
        );

        // Services: the servers of CIG (asked beside the window at most every five minutes), the Basetool,
        // the update.
        // The colors of 4.12: the accent where all is well, gold where something asks for a look.
        let (cig, cig_tone) = match self.status_look.poll(paths, now as f64) {
            Some(state) => (one("s_uv_cig", Arg::from(cig_word(lang, &state.word))), cig_tone(&state.word)),
            None => (t_in(lang, "s_uv_cig_unbekannt"), Tone::Plain),
        };
        let mut details = Vec::new();
        let mut tones = Vec::new();
        let (basetool, linked) = match self.connection.wire() {
            Some(wire) if wire.sync.linked() => match wire.sync.status().get().1.and_then(|s| s.last_sync) {
                Some(at) => (one("s_uv_basetool", Arg::from(relative_age_in(lang, now as f64 - at))), true),
                None => (t_in(lang, "s_uv_basetool_neu"), true),
            },
            _ => (t_in(lang, "s_uv_basetool_aus"), false),
        };
        details.push(basetool);
        tones.push(if linked { Tone::Good } else { Tone::Plain });
        let own_version = env!("CARGO_PKG_VERSION");
        let on_offer = self.update_name();
        let newer = !on_offer.is_empty() && versekit_core::version::is_newer(&on_offer, own_version);
        details.push(if newer {
            one("s_uv_update_da", Arg::from(on_offer.as_str()))
        } else {
            one("s_uv_update_aktuell", Arg::from(own_version))
        });
        tones.push(if newer { Tone::Warn } else { Tone::Good });
        out.insert("dienste", Glance { tones, line_tone: cig_tone, ..glance(cig, details) });

        out.insert("raffinerie", refinery_tile_with_scan(paths, lang, now));

        // Storage: how much different lies there, and the largest materials.
        out.insert("lager", storage_tile(&self.stock.rows(), lang, MOST));

        // Open contracts that give blueprints, the most missing first.
        if real_catalog && !self.contracts.sample {
            let tile = match self.contracts.lookups() {
                None => glance(t_in(lang, "s_uv_lese"), Vec::new()),
                Some(lookups) => {
                    let running: Vec<String> = self
                        .contracts
                        .missions
                        .iter()
                        .filter(|m| m.state.is_empty() || m.state == versekit_core::mission_log::RUNNING)
                        .map(|m| m.name.clone())
                        .collect();
                    contracts_tile(&running, &lookups.blueprints, &owned, lang)
                }
            };
            out.insert("auftraege", tile);
        } else {
            // Why there is nothing: no catalog says which blueprints a contract gives, or no contracts were read.
            let why = if real_catalog { "s_kt_auftraege_leer" } else { "s_kt_katalog_leer" };
            out.insert("auftraege", glance(t_in(lang, why), Vec::new()));
        }

        // New in the patch: the blueprints that came with the catalog's version.
        if real_catalog {
            let (new, mine) = overview::patch_numbers(catalog, &owned);
            let short = blueprints::version_short(&catalog.version);
            let version_word = t_in(lang, "s_kt_diese_version");
            let short = if short.is_empty() { version_word.as_str() } else { short };
            // The new blueprints the player does not have yet come first, then the ones he has.
            let mut names: Vec<(bool, &str)> = catalog
                .blueprints
                .iter()
                .filter(|b| !catalog.version.is_empty() && b.since.as_deref() == Some(catalog.version.as_str()))
                .map(|b| (owned(&b.key), b.name.as_str()))
                .collect();
            names.sort();
            out.insert(
                "neu_patch",
                if new == 0 {
                    glance(one("s_uv_neu_patch_leer", Arg::from(short)), Vec::new())
                } else {
                    let mut details = vec![one("s_uv_neu_patch_hast", Arg::from(mine))];
                    details.extend(names.iter().take(MOST).map(|(_, name)| name.to_string()));
                    let line = format!("{new} {}", one("s_uv_neu_patch_zahl", Arg::from(short)));
                    let tile = glance(line, details).figure(new);
                    if mine > 0 { tile.tone_at(0, Tone::Good) } else { tile }
                },
            );
        } else {
            out.insert("neu_patch", glance(t_in(lang, "s_kt_katalog_leer"), Vec::new()));
            out.insert("bauplaene", glance(t_in(lang, "s_kt_katalog_leer"), Vec::new()));
        }

        // Watch list: found of all, the open ones.
        let (found, total, open) = overview::watch_numbers(&versekit_core::watchlist::load(paths), &owned);
        out.insert(
            "merkliste",
            if total == 0 {
                glance(t_in(lang, "s_uv_merkliste_leer"), Vec::new())
            } else {
                glance(
                    two("s_uv_merkliste_zahl", Arg::from(found), Arg::from(total)),
                    open.into_iter().take(MOST).collect(),
                )
                .bar(found as f32 / total as f32)
                .figure(found)
            },
        );

        // Unit demand: what is wanted, and how many materials lie in storage complete.
        let demand = self.connection.wire().and_then(|wire| wire.demand.current(now as f64).1);
        out.insert(
            "bedarf",
            match demand {
                None => glance(t_in(lang, "s_uv_bedarf_leer"), Vec::new()),
                Some(demand) => {
                    let entries = stock::load(&paths.app_file(stock::RAW_FILE), "overview.demand");
                    // What is still missing, by material: what is wanted less what lies in storage.
                    let decimal = formats::decimal_mark(lang);
                    let mut missing: Vec<(String, f64)> = Vec::new();
                    let mut complete = 0usize;
                    for m in &demand.materials {
                        let (have, _) =
                            versekit_core::stock_use::amount_with_quality(&entries, &m.name, m.min_quality as f64);
                        if have > 0.0 && have >= m.amount {
                            complete += 1;
                        } else {
                            missing.push((m.name.clone(), (m.amount - have.max(0.0)).max(0.0)));
                        }
                    }
                    let mut details = vec![one("s_kt_bedarf_da", Arg::from(complete))];
                    let done_tone = if complete > 0 { Tone::Good } else { Tone::Plain };
                    details.extend(missing.into_iter().take(MOST).map(|(name, short)| {
                        let gap = format!("{} SCU", stock::amount_text(short, false, decimal));
                        two("s_kt_bedarf_fehlt", Arg::from(name), Arg::from(gap))
                    }));
                    glance(
                        tf_in(
                            lang,
                            "s_kt_bedarf_zahl",
                            &[("0", Arg::from(demand.materials.len())), ("1", Arg::from(demand.items.len()))],
                        ),
                        details,
                    )
                    .tone_at(0, done_tone)
                    // All that is wanted is the key figure (4.12).
                    .figure(demand.materials.len() + demand.items.len())
                }
            },
        );

        // Latest finds in the game.
        let finds = overview::latest_finds(&own, MOST);
        let mut lines = finds.iter().map(|(name, at)| two("s_uv_fund", Arg::from(name.as_str()), Arg::from(ago(*at))));
        out.insert(
            "funde",
            match lines.next() {
                None => glance(t_in(lang, "s_uv_funde_leer"), Vec::new()),
                Some(first) => glance(first, lines.collect()),
            },
        );

        // Backup: when the last one was made.
        let last = paths.setting("letzte_sicherung").and_then(|text| versekit_core::export::local_stamp(&text));
        let backup = match last.and_then(|at| formats::local_time(at).map(|t| (at, t))) {
            // Never backed up asks for a look: gold, as in 4.12.
            None => glance(t_in(lang, "s_uv_sicherung_nie"), Vec::new()).line_tone(Tone::Warn),
            Some((at, time)) => {
                let format = if lang == Lang::De { "%d.%m.%Y %H:%M" } else { "%Y-%m-%d %H:%M" };
                glance(ago(at), vec![one("s_uv_sicherung_zuletzt", Arg::from(formats::strftime_shown(format, &time)))])
            }
        };
        // "Back up now" on the tile itself, as in 4.12: the link that makes the backup without a trip to the page.
        out.insert("sicherung", Glance { action: Some(TileAction::BackupNow), ..backup });

        // Notes: the memo edited last, its title as the line and the lines after it, muted.
        out.insert(
            "notizen",
            match self.memo_page.memos().latest() {
                None => glance(t_in(lang, "s_uv_notizen_leer"), Vec::new()),
                Some(memo) => {
                    let line = match memo.title() {
                        "" => t_in(lang, "s_nz_ohne_titel"),
                        title => title.to_string(),
                    };
                    let details: Vec<String> = memo.following(NOTE_LINES).into_iter().map(str::to_string).collect();
                    (0..details.len()).fold(glance(line, details), |tile, at| tile.tone_at(at, Tone::Muted))
                }
            },
        );

        // The Executive Hangar: the same on its tile and on the pinned page.
        let hangar = hangar_tile(self.contested.start(), versekit_core::contested::now(), lang);
        out.insert("executive_hangar", hangar.clone());

        // A pinned page shows what its page has to show; one without data of its own, its short explanation.
        let mut pages = self.page_glances(paths, &own, lang, now, &super::glance::today_key(now), &out);
        pages.insert("hf_contested", hangar);
        for tile in self.tiles.iter().filter(|tile| tile.pinned) {
            if let Some((page, key)) = overview::PAGES.iter().find(|(_, key)| *key == tile.key) {
                let shown = pages
                    .get(key)
                    .cloned()
                    .unwrap_or_else(|| glance(t_in(lang, &overview::description_key(page)), Vec::new()));
                out.insert(key, shown);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use versekit_core::language::Lang;

    const DE: Lang = Lang::De;

    /// The moment now and its time of day on the local clock. Nothing here counts the seconds since
    /// midnight: a day with a change of summer time is an hour longer or shorter.
    fn now_and_clock() -> (i64, String) {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
        let clock = formats::strftime_shown("%H:%M", &formats::local_time(now).unwrap());
        (now, clock)
    }

    /// The combination on the refinery tile speaks the language of the page; one the overlay would not take stays.
    #[test]
    fn the_hotkey_on_the_tile_names_its_keys_in_the_language_of_the_page() {
        assert_eq!(hotkey_in(DE, "Strg+Alt+R"), "Strg+Alt+R");
        assert_eq!(hotkey_in(Lang::En, "Strg+Alt+R"), "Ctrl+Alt+R");
        assert_eq!(hotkey_in(Lang::En, "umschalt+strg+f9"), "Ctrl+Shift+F9");
        assert_eq!(hotkey_in(Lang::En, "R"), "R");
    }

    /// Blueprints of another account that did not count stand first on the blueprint tile, in gold, and the line
    /// of the week keeps its accent below them; without any the tile stays as it was.
    #[test]
    fn the_blueprint_tile_names_the_blueprints_of_another_account_first_in_gold() {
        let week = tf_in(DE, "s_uv_woche", &[("0", Arg::from(2_i64))]);
        let plain = blueprint_tile("5 von 10".into(), vec![week.clone()], 5, 10, 2);
        assert_eq!(with_foreign(plain.clone(), 0, DE), plain);
        let told = with_foreign(plain, 3, DE);
        assert_eq!(told.details, [tf_in(DE, "s_uv_bp_fremd", &[("0", Arg::from(3_i64))]), week]);
        assert_eq!((told.tone_of(0), told.tone_of(1)), (Tone::Warn, Tone::Good));
        let one = with_foreign(blueprint_tile("1 von 10".into(), Vec::new(), 1, 10, 0), 1, DE);
        assert_eq!(one.details, [tf_in(DE, "s_uv_bp_fremd_1", &[("0", Arg::from(1_i64))])]);
    }

    /// The Executive Hangar at the moments a player saw and the page measured: the phase, the lamps, the
    /// countdown and the time of the opening, in the colours of the phase; a second on it is a second less.
    #[test]
    fn the_hangar_tile_has_the_phase_the_lamps_the_countdown_and_the_time_of_the_opening() {
        use versekit_core::contested::{self, Lamp};
        const START: i64 = 1_792_860_059;
        let left = |text: &str| tf_in(DE, "ov_hangar_oeffnet", &[("0", Arg::from(text))]);
        let at_opening = START + contested::CLOSED;
        // 14:39 before it opens, four lamps green.
        let closed = hangar_tile(Some(START), (at_opening - (14 * 60 + 39)) as f64, DE);
        assert_eq!((closed.line.as_str(), closed.line_tone), (t_in(DE, "s_cz_geschlossen").as_str(), Tone::Bad));
        assert_eq!(closed.lamps, Some([Lamp::Green, Lamp::Green, Lamp::Green, Lamp::Green, Lamp::Red]));
        assert_eq!(
            closed.details,
            [left("14:39"), tf_in(DE, "s_uv_hangar_oeffnet", &[("0", Arg::from(contested::clock(at_opening)))])]
        );
        let a_second_on = hangar_tile(Some(START), (at_opening - (14 * 60 + 38)) as f64, DE);
        assert_eq!(a_second_on.details[0], left("14:38"));
        // Open: the accent, until when it stays open.
        let open = hangar_tile(Some(START), (at_opening + 600) as f64, DE);
        assert_eq!((open.line.as_str(), open.line_tone), (t_in(DE, "s_cz_offen").as_str(), Tone::Good));
        assert_eq!(open.details[0], tf_in(DE, "ov_hangar_schliesst", &[("0", Arg::from("50:00"))]));
        let closes = contested::clock(at_opening + contested::OPEN);
        assert_eq!(open.details[1], tf_in(DE, "s_uv_hangar_offen_bis", &[("0", Arg::from(closes))]));
        // Resetting: gold, no lamp on, and the next opening of the next cycle.
        let reset = hangar_tile(Some(START), 1_792_870_884.999, DE);
        assert_eq!((reset.line.as_str(), reset.line_tone), (t_in(DE, "s_cz_reset").as_str(), Tone::Warn));
        assert_eq!(reset.lamps, Some([Lamp::Dark; 5]));
        assert_eq!(reset.details[0], tf_in(DE, "ov_hangar_reset", &[("0", Arg::from("04:35"))]));
        let next = contested::next_openings(START, 1_792_870_884.999, 1)[0].opens;
        assert_eq!(reset.details[1], tf_in(DE, "s_uv_hangar_oeffnet", &[("0", Arg::from(contested::clock(next)))]));
        // No start: the hint, in the colour of what asks for a look, and nothing else.
        let none = hangar_tile(None, 0.0, DE);
        assert_eq!((none.line.as_str(), none.line_tone), (t_in(DE, "s_uv_hangar_ohne").as_str(), Tone::Warn));
        assert!(none.details.is_empty() && none.lamps.is_none());
    }

    #[test]
    fn the_location_says_since_when_and_from_where_as_4_12() {
        let dir = std::env::temp_dir().join(format!("versekit-standort-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let paths = Paths::new(super::super::real_data::host_apart(&dir));
        let (now, clock) = now_and_clock();
        let at = now;
        let here = location::Place {
            number: Some("4711".into()),
            key: Some("Stanton2_Orison".into()),
            since: Some(at),
            ..location::Place::default()
        };
        let running = place_tile(Some(Some(here.clone())), true, &paths, DE, now);
        assert_eq!(running.details, [tf_in(DE, "s_uv_ort_seit", &[("0", Arg::from(clock.as_str()))])]);
        let quiet = place_tile(Some(Some(here.clone())), false, &paths, DE, now);
        assert_eq!(quiet.details, [tf_in(DE, "s_uv_ort_zuletzt", &[("0", Arg::from(clock.as_str()))])]);
        // Another day (two days back: one day back can be the same date on a day with 25 hours): day and time.
        let earlier = location::Place { since: Some(at - 2 * 86_400), ..here.clone() };
        let day = place_tile(Some(Some(earlier)), false, &paths, DE, now);
        let moment = stats_report::local_moment(DE, at - 2 * 86_400);
        assert_eq!(day.details, [tf_in(DE, "s_uv_ort_zuletzt", &[("0", Arg::from(moment))])]);
        // Travelling: from where and when; without the time no line.
        let away = location::Place {
            number: Some("0".into()),
            travelling: true,
            before: Some("Stanton1_Lorville".into()),
            since: Some(at),
            ..location::Place::default()
        };
        let travelling = place_tile(Some(Some(away.clone())), true, &paths, DE, now);
        assert_eq!(travelling.line, t_in(DE, "s_uv_unterwegs"));
        let from = location::display_name("Stanton1_Lorville", &location::names(&paths));
        assert_eq!(
            travelling.details,
            [tf_in(DE, "s_uv_ort_ab", &[("0", Arg::from(from)), ("1", Arg::from(clock.as_str()))])]
        );
        let unknown_time = location::Place { since: None, ..away };
        assert!(place_tile(Some(Some(unknown_time)), true, &paths, DE, now).details.is_empty());
        assert!(
            place_tile(Some(Some(location::Place { since: None, ..here })), true, &paths, DE, now).details.is_empty()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The tile „Übersetzung“ tells the same true state as the page: found but not fetched, being fetched only
    /// while a fetch runs, and locked on a copy of the data, whose lock keeps the game folder untouched.
    #[test]
    fn the_translation_tile_says_being_fetched_only_while_it_is_and_locked_on_a_copy_of_the_data() {
        use versekit_core::net::Fake;
        let dir =
            std::env::temp_dir().join(format!("versekit-kachel-uebersetzung-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        // The data folder and the game folder side by side, as on a copy of the data beside the real game.
        let (data, game) = (dir.join("daten"), dir.join("LIVE"));
        for folder in [&data, &game] {
            std::fs::create_dir_all(folder).unwrap();
        }
        std::fs::write(game.join("Game.log"), "").unwrap();
        let (data_text, live) = (data.to_string_lossy().into_owned(), game.to_string_lossy().into_owned());
        let host = versekit_core::paths::Host::new(
            versekit_core::pypath::Flavor::native(),
            &[("SC_BP_HOME", data_text.as_str()), ("HOME", data_text.as_str()), ("SC_INSTALL_DIR", live.as_str())],
            &data_text,
        );
        let paths = Paths::new(host.clone());
        paths.set_setting(translation::SETTING_SOURCE, versekit_core::jsonio::Value::str("luftwerft_de"));
        translation::note_set(&paths, "luftwerft_de", Some("etag:a"), Some("german_(germany)"), None);
        let url = "https://www.luftwerft.com/download/resources/localizations/LIVE/german_(germany)/global.ini";
        let mut file = String::from("Frontend_PU_Version=LW\nvehicle_NameAEGS_Gladius=Gladius\n");
        for i in 0..5001 {
            file.push_str(&format!("key_{i}=Text {i}\n"));
        }
        let net = Fake::default().with_head(url, &[("ETag", "b")]).with(url, file.into_bytes());
        let page = SamplePage::with_settings(Paths::new(host));
        let tile = |page: &SamplePage| {
            let glance = page.tiles_of_4_12(&paths, DE, 0).remove("uebersetzung").expect("the tile");
            format!("{} · {}", glance.line, glance.details.join(" · "))
        };

        assert!(translation::update_available(&net, &paths, "luftwerft_de", None).0);
        let found = tile(&page);
        assert!(found.starts_with("LIVE: ") && found.contains("noch nicht geholt"), "{found}");
        assert!(!found.contains("wird geholt"), "nothing fetches it: {found}");

        // A copy of the data: writing only in its data folder; the game folder of this test lies outside.
        versekit_core::writeguard::deny(true);
        versekit_core::writeguard::allow_under(Some(&data_text));
        let refused = translation::fetch(&net, &paths, "luftwerft_de", &live, None, &|_| {});
        versekit_core::writeguard::deny(false);
        versekit_core::writeguard::allow_under(None);
        assert_eq!(refused.err(), Some(translation::Problem::TestRun));
        let locked = tile(&page);
        assert!(locked.contains("gesperrt: Im Testbetrieb schreibt VerseKit nicht in den Spielordner"), "{locked}");
        assert!(!locked.contains("wird geholt") && !locked.contains("makedirs"), "{locked}");

        // Counter-check: while a fetch really runs, the tile says it is being fetched.
        assert!(translation::update_available(&net, &paths, "luftwerft_de", None).0);
        let seen = std::cell::RefCell::new(Vec::new());
        translation::fetch(&net, &paths, "luftwerft_de", &live, None, &|_| seen.borrow_mut().push(tile(&page)))
            .unwrap();
        let seen = seen.into_inner();
        assert!(!seen.is_empty() && seen.iter().all(|t| t.contains("wird geholt")), "{seen:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_location_is_read_from_the_game_folder_given_on_the_command_line() {
        let dir = std::env::temp_dir().join(format!("versekit-standort-ordner-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // The game folder of the machine holds an empty log; the one given holds the change.
        let paths = Paths::new(super::super::real_data::host_apart(&dir));
        let given = dir.join("GIVEN");
        std::fs::create_dir_all(&given).unwrap();
        std::fs::write(
            given.join("Game.log"),
            "<2026-10-21T10:05:00.000Z> <Update Inventory Location> Player [Xy] is changing location. Landing [815] -> [4711]\n",
        )
        .unwrap();
        paths.set_given_game(&given.to_string_lossy(), versekit_core::given::GivenBy::CommandLine);
        let look = PlaceLook::default();
        let mut found = None;
        for _ in 0..200 {
            found = look.poll(&paths, |own| location::place(own, None));
            if found.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let place = found.flatten().expect("read from the given folder");
        assert_eq!((place.number.as_deref(), place.since), (Some("4711"), Some(1_792_577_100)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_log_that_keeps_changing_is_read_again_only_after_the_pause() {
        use std::sync::atomic::AtomicUsize;
        static READS: AtomicUsize = AtomicUsize::new(0);
        let dir =
            std::env::temp_dir().join(format!("versekit-spielprotokoll-pause-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let paths = Paths::new(super::super::real_data::host_apart(&dir));
        let given = dir.join("GIVEN");
        std::fs::create_dir_all(&given).unwrap();
        let log = given.join("Game.log");
        std::fs::write(&log, "<2026-10-21T10:05:00.000Z> a line\n").unwrap();
        paths.set_given_game(&given.to_string_lossy(), versekit_core::given::GivenBy::CommandLine);
        let look: LogLook<usize> = LogLook::default();
        let read: fn(&Paths) -> usize = |_| READS.fetch_add(1, Ordering::SeqCst) + 1;
        let settled = |look: &LogLook<usize>, pause, want: usize| {
            for _ in 0..200 {
                if look.poll_every(&paths, read, pause) == Some(want) && !look.busy.load(Ordering::Acquire) {
                    return true;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            false
        };
        assert!(settled(&look, LOOK_EVERY, 1), "the first reading starts at once");
        // The game writes on: within the pause nothing is read again.
        for n in 0..20 {
            std::fs::write(&log, format!("<2026-10-21T10:05:00.000Z> a line\n{}", "more\n".repeat(n + 1))).unwrap();
            look.poll(&paths, read);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(READS.load(Ordering::SeqCst), 1, "read again within the pause");
        // Once the pause is over, the change is read.
        assert!(settled(&look, std::time::Duration::ZERO, 2), "read again after the pause");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_services_tile_shows_the_state_of_cig_as_kept_and_asks_nothing_without_the_net() {
        let dir = std::env::temp_dir().join(format!("versekit-dienste-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // The throwaway host has SC_BP_NO_NET set.
        let paths = Paths::new(super::super::real_data::host_apart(&dir));
        let look = StatusLook::default();
        assert_eq!(look.poll(&paths, 1000.0), None, "nothing fetched yet");
        let target = paths.app_file(serverstatus::CACHE);
        std::fs::create_dir_all(std::path::Path::new(&target).parent().unwrap()).unwrap();
        std::fs::write(&target, r#"{"etag": "", "lage": {"gesamt": "partial_outage", "geholt": 10.0}}"#).unwrap();
        let state = look.poll(&paths, 1000.0).expect("the state kept");
        assert_eq!(state.word, "partial_outage", "the word of CIG, untranslated");
        assert!(!look.busy.load(Ordering::Acquire), "SC_BP_NO_NET: the page is not asked");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_state_of_the_servers_is_told_in_the_language_of_the_program_and_an_unknown_word_stays_as_cig_wrote_it() {
        assert_eq!(cig_word(DE, "partial_outage"), "teilweise gestört");
        assert_eq!(cig_word(Lang::En, "partial_outage"), "partly down");
        assert_eq!(cig_word(DE, "operational"), "läuft");
        assert_eq!(cig_word(DE, "something_new"), "something_new");
    }

    #[test]
    fn the_blueprint_tile_draws_the_share_and_colors_the_week_only_when_something_came() {
        let tile = blueprint_tile("318 von 722".into(), vec!["Diese Woche: +4".into()], 318, 722, 4);
        assert_eq!(tile.bar, Some((318.0f64 / 722.0) as f32));
        assert_eq!(tile.figure.as_deref(), Some("318"), "the number owned is the key figure, at the start of the line");
        assert_eq!(tile.tone_of(0), Tone::Good);
        let quiet = blueprint_tile("0 von 0".into(), vec!["Diese Woche: +0".into()], 0, 0, 0);
        assert_eq!(quiet.tone_of(0), Tone::Plain, "nothing came, nothing to color");
        assert_eq!(quiet.bar, Some(0.0), "an empty catalog is an empty bar, no division by zero");
    }

    #[test]
    fn the_state_of_the_servers_has_the_color_of_its_news() {
        assert_eq!(cig_tone("operational"), Tone::Good);
        assert_eq!(cig_tone("degraded_performance"), Tone::Warn);
        assert_eq!(cig_tone("partial_outage"), Tone::Warn);
        assert_eq!(cig_tone("major_outage"), Tone::Bad);
        assert_eq!(cig_tone("something_new"), Tone::Warn, "what is not known asks for a look");
    }

    #[test]
    fn the_storage_tile_names_as_many_stocks_as_it_is_asked_for_and_4_12_named_three() {
        use versekit_core::stock::{Row, Store};
        let rows: Vec<Row> = (1..=8)
            .map(|n| Row {
                store: Store::Raw,
                index: 0,
                name: format!("Material {n}"),
                amount: f64::from(100 - n),
                piece: false,
                quality: None,
                place: "Area18".into(),
                stolen: false,
                cost: None,
                item: false,
                leftover: false,
            })
            .collect();
        assert_eq!(storage_tile(&rows, DE, 3).details.len(), 3);
        assert_eq!(storage_tile(&rows, DE, MOST).details.len(), 8, "a bigger tile has more to show");
        assert_eq!(storage_tile(&rows, DE, MOST).line, "Rohstoffe: 8 · Waren: 0 · Items: 0");
        assert_eq!(storage_tile(&rows, DE, MOST).figure.as_deref(), Some("8"), "all that is different, as 4.12");
    }

    /// The open contracts are found by their title as 4.12 finds them: the blueprints the mission of the title
    /// gives, counted by name whether the catalog lists them or not, each title once, the most missing first; a
    /// contract without blueprints and an unknown title are left out.
    #[test]
    fn the_open_contracts_count_the_blueprints_of_their_mission_by_the_title_as_4_12() {
        use versekit_core::mission_log::{ContractBlueprints, ContractIndex};
        let dir = std::env::temp_dir().join(format!("versekit-kachel-auftraege-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let ini = dir.join("global.ini");
        std::fs::write(
            &ini,
            "Farm_Help_Title=Hilf den Bauern\nRival_Stop_Title=Halte die Rivalen auf\nEmpty_Title=Bring das Paket\n",
        )
        .unwrap();
        let missions: HashMap<String, Vec<String>> = [
            ("Farm_Help_Title", vec!["Arclight Pistol", "Karna Rifle", "Kein Katalogname"]),
            ("Rival_Stop_Title", vec!["P4-AR Rifle"]),
            ("Empty_Title", vec![]),
        ]
        .into_iter()
        .map(|(key, names)| (key.to_string(), names.into_iter().map(str::to_string).collect()))
        .collect();
        let known: HashSet<String> = missions.keys().cloned().collect();
        let blueprints =
            ContractBlueprints { index: ContractIndex::build(&[ini], &known), missions, definitions: HashMap::new() };
        let owned_keys = [versekit_core::paths::name_key("Arclight Pistol")];
        let owned = |key: &str| owned_keys.iter().any(|k| k == key);
        let running: Vec<String> =
            ["Halte die Rivalen auf", "Hilf den Bauern", "Bring das Paket", "Unbekannt", "Hilf den Bauern"]
                .map(str::to_string)
                .to_vec();
        let tile = contracts_tile(&running, &blueprints, &owned, DE);
        let missing = |name: &str, missing: usize, total: usize| {
            tf_in(
                DE,
                "s_kt_auftrag_fehlt",
                &[("0", Arg::from(name)), ("1", Arg::from(missing)), ("2", Arg::from(total))],
            )
        };
        assert_eq!(tile.details, [missing("Hilf den Bauern", 2, 3), missing("Halte die Rivalen auf", 1, 1)]);
        assert_eq!(tile.line, tf_in(DE, "s_uv_auftraege_zahl", &[("0", Arg::from(2_i64))]));
        assert_eq!(tile.figure.as_deref(), Some("2"));
        assert!(tile.line.starts_with("2"), "the key figure begins the line: {}", tile.line);
        assert_eq!((tile.tone_of(0), tile.tone_of(1)), (Tone::Good, Tone::Good), "what is missing in the accent");
        // All owned: the title stays, plain, with all it gives.
        let all = |key: &str| key == versekit_core::paths::name_key("P4-AR Rifle");
        let done = contracts_tile(&["Halte die Rivalen auf".to_string()], &blueprints, &all, DE);
        assert_eq!(
            done.details,
            [tf_in(DE, "s_kt_auftrag_alle", &[("0", Arg::from("Halte die Rivalen auf")), ("1", Arg::from(1_i64))])]
        );
        assert_eq!(done.tone_of(0), Tone::Plain);
        // Nothing that gives blueprints runs: the sentence of 4.12.
        let none = contracts_tile(&["Bring das Paket".to_string()], &blueprints, &owned, DE);
        assert_eq!((none.line, none.figure), (t_in(DE, "s_uv_auftraege_leer"), None));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The tile of the farm list counts on the recipes as the program loads them: from the game values kept in the
    /// data folder, read at the start of the page. What is noted to build counts its pieces and names the missing
    /// materials, as many pieces as noted; without loaded values it says there are none, so its figures come from
    /// what was loaded.
    #[test]
    fn the_farm_list_tile_counts_on_the_recipes_loaded_from_the_data_folder() {
        use std::sync::Arc;
        use versekit_core::{hangar, notepad};
        use versekit_gamedata::values::GameValues;
        use versekit_gamedata::values_store;

        /// Hands over the given values as the ones of scmdb, without the net.
        struct Given(GameValues);
        impl values_store::Supply for Given {
            fn scmdb_values(&self, _build: &str) -> Result<GameValues, String> {
                Ok(self.0.clone())
            }
            fn current_build(&self) -> Result<String, String> {
                Ok("4.0.0-test".to_string())
            }
            fn corrections(
                &self,
                _build: Option<&str>,
            ) -> (Vec<versekit_gamedata::corrections::Correction>, Option<String>) {
                (Vec::new(), None)
            }
            fn reads_archive(&self) -> bool {
                false
            }
        }

        let dir = std::env::temp_dir().join(format!("versekit-kachel-farm-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let host = super::super::real_data::host_apart(&dir);
        let paths = Paths::new(host.clone());
        let note = |items: &[(&str, i64)]| {
            let mut data = hangar::empty();
            for (name, count) in items {
                notepad::add(&mut data, name, "", *count);
            }
            assert!(hangar::save(&paths.app_file(hangar::FILE), &mut data));
        };
        let farm_tile = || {
            let mut page = SamplePage::with_settings(Paths::new(host.clone()));
            page.tiles.push(crate::tiles::Tile::pinned("hf_farmliste"));
            page.tiles_of_4_12(&paths, DE, 0).remove("hf_farmliste").expect("the tile of the pinned farm list")
        };
        note(&[("Frostline", 2)]);

        // Before any values are kept the tile says there are none.
        let before = farm_tile();
        assert_eq!(before.line, t_in(DE, "s_he_keine_daten"), "{before:?}");

        // The recipes of the sample, kept in the data folder as a reading keeps them.
        let sample = crate::crafting_page::CraftingState::default();
        let values = sample.parts().expect("the sample values").0.clone();
        let cache = paths.app_file(values_store::CACHE_FILE);
        std::fs::create_dir_all(std::path::Path::new(&cache).parent().expect("the data folder")).unwrap();
        let started = values_store::start(None, &cache, Arc::new(Given(values)));
        started.rebuild.expect("nothing is kept yet").run().expect("the values are read");
        assert!(std::path::Path::new(&cache).is_file(), "kept in the data folder");

        let tile = farm_tile();
        assert_eq!(tile.line_tone, Tone::Warn, "{tile:?}");
        assert!(tile.line.starts_with("Rohstoffe, die fehlen: "), "{tile:?}");
        assert_eq!(tile.details[0], "Zu bauen: 2");
        assert!(tile.details.len() > 1 && tile.details[1..].iter().all(|d| d.contains(" · fehlt: ")), "{tile:?}");

        // The count of the note is what is built.
        note(&[("Frostline", 5)]);
        assert_eq!(farm_tile().details[0], "Zu bauen: 5");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The course of the index: the days noted before today, newest first, muted, below the index of now; today
    /// itself is the line of now. The date and the decimal mark follow the language.
    #[test]
    fn the_computer_tile_shows_the_course_of_the_index_over_the_days_noted_before() {
        use machine_info::history::Day;
        let day = |day: &str, cpu: f64, gpu: Option<f64>| Day { day: day.into(), cpu, gpu, class: Some(4) };
        let history =
            [day("2026-10-28", 8.3, Some(12.5)), day("2026-10-30", 9.0, None), day("2026-10-31", 9.6, Some(13.0))];
        let rated = machine_info::Info {
            machine_class: Some(4),
            cpu_index: Some(9.6),
            gpu_index: Some(13.0),
            ..machine_info::Info::default()
        };
        let tile = machine_tile(Some(Some(rated.clone())), &history, "2026-10-31", DE);
        assert_eq!(
            tile.details,
            [
                tf_in(DE, "s_uv_index", &[("0", Arg::from("10")), ("1", Arg::from("13"))]),
                "Am 30.10.2026: CPU 9,0 · GPU ?".to_string(),
                "Am 28.10.2026: CPU 8,3 · GPU 12,5".to_string(),
            ],
            "{tile:?}"
        );
        assert_eq!((tile.tone_of(0), tile.tone_of(1), tile.tone_of(2)), (Tone::Plain, Tone::Muted, Tone::Muted));
        let english = machine_tile(Some(Some(rated.clone())), &history, "2026-10-31", Lang::En);
        assert_eq!(english.details[1], "On 2026-10-30: CPU 9.0 · GPU ?");
        // Nothing noted before today: the tile stays as it was.
        assert_eq!(machine_tile(Some(Some(rated)), &history[2..], "2026-10-31", DE).details.len(), 1);
    }

    /// Recording: the tile reads the rating from the game log beside the window, and a reading with an index is
    /// noted for today in the data folder.
    #[test]
    fn a_reading_of_the_rating_by_the_tile_is_noted_in_the_data_folder() {
        let dir = std::env::temp_dir().join(format!("versekit-rechner-verlauf-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let host = super::super::real_data::host_apart(&dir);
        std::fs::write(
            dir.join("LIVE").join("Game.log"),
            "<2026-10-22T10:00:02.002Z> Performance Index: 8.5 (CPU), 12.75 (GPU)\n\
             <2026-10-22T10:00:02.003Z> Final rating: Machine class 4\n",
        )
        .unwrap();
        let paths = Paths::new(host.clone());
        let page = SamplePage::with_settings(Paths::new(host));
        let (now, _) = now_and_clock();
        let mut days = Vec::new();
        for _ in 0..500 {
            let _ = page.tiles_of_4_12(&paths, DE, now);
            days = machine_info::history::load(&paths);
            if !days.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(days.len(), 1, "noted once");
        assert_eq!((days[0].cpu, days[0].gpu, days[0].class), (8.5, Some(12.75), Some(4)));
        assert_eq!(Some(days[0].day.clone()), machine_info::history::day_of(now));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The link "Jetzt nach neuer Übersetzung sehen" stands on the tile only where a channel has a source to look
    /// at: none chosen and the original texts have nothing to look for.
    #[test]
    fn the_translation_tile_offers_the_look_only_with_a_source_chosen() {
        use versekit_core::jsonio::Value;
        let dir = std::env::temp_dir().join(format!("versekit-kachel-nachsehen-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let host = super::super::real_data::host_apart(&dir);
        let paths = Paths::new(host.clone());
        let page = SamplePage::with_settings(Paths::new(host));
        let action = |page: &SamplePage| page.tiles_of_4_12(&paths, DE, 0).remove("uebersetzung").expect("tile").action;
        assert_eq!(action(&page), None, "no source chosen");
        paths.set_setting(translation::SETTING_SOURCE, Value::str(translation::ORIGINAL));
        assert_eq!(action(&page), None, "the original texts");
        paths.set_setting(translation::SETTING_SOURCE, Value::str("luftwerft_de"));
        assert_eq!(action(&page), Some(TileAction::TranslationCheck));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_computer_tile_names_the_class_and_the_index_or_says_it_is_not_rated() {
        let machine_tile = |info, lang| machine_tile(info, &[], "", lang);
        assert_eq!(machine_tile(None, DE).line, t_in(DE, "s_uv_lese"));
        assert_eq!(machine_tile(Some(None), DE).line, t_in(DE, "s_gr_klasse_leer"));
        let unrated = machine_info::Info::default();
        assert_eq!(machine_tile(Some(Some(unrated)), DE).line, t_in(DE, "s_gr_klasse_leer"));
        let rated = machine_info::Info {
            machine_class: Some(3),
            cpu_index: Some(1234.4),
            gpu_index: Some(987.6),
            ..machine_info::Info::default()
        };
        let tile = machine_tile(Some(Some(rated.clone())), DE);
        assert_eq!(tile.line, t_in(DE, "s_gr_stufe_3"));
        assert_eq!(tile.bar, Some(0.6), "class 3 of 5");
        assert_eq!(tile.steps, Some(5), "in the five segments of 4.12");
        let top = machine_info::Info { machine_class: Some(9), ..rated.clone() };
        assert_eq!(machine_tile(Some(Some(top)), DE).bar, Some(1.0), "never more than five");
        assert_eq!(tile.details, [tf_in(DE, "s_uv_index", &[("0", Arg::from("1234")), ("1", Arg::from("988"))])]);
        // Without the index the class stands alone.
        let bare = machine_info::Info { cpu_index: None, ..rated };
        assert!(machine_tile(Some(Some(bare)), DE).details.is_empty());
    }
}
