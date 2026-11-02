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

//! What the overview tiles and the play time of the head bar show on real
//! data: counted from the data the window has open. A tile whose page has
//! no real data in this version yet says so instead of showing a sample.

use std::collections::{BTreeMap, HashMap};

use versekit_core::formats::{self, LocalTime};
use versekit_core::language::{self, Arg, Lang, tf_in};
use versekit_core::mission_log as log;
use versekit_core::playtime;

use super::SamplePage;
use crate::blueprint_list::ListData;
use crate::contract_log::LogData;

/// How often the counts are taken again (seconds).
pub(super) const GLANCE_SECONDS: f64 = 10.0;

/// The line of a tile whose page has no real data in this version yet.
pub(super) fn not_yet() -> String {
    language::t("s_uv_kommt")
}

/// What a tile says that has no counts on real data: why, where the reason is known (no catalog, no
/// contracts read, no refinery job), else that its page comes later.
pub(super) fn missing(key: &str) -> String {
    match key {
        "bauplaene" | "neu_patch" | "hf_liste" | "hf_fortschritt" => language::t("s_kt_katalog_leer"),
        "auftraege" | "hf_auftragslog" => language::t("s_kt_auftraege_leer"),
        "raffinerie" => language::t("s_kt_raff_leer"),
        _ => not_yet(),
    }
}

/// The German line of [`not_yet`], for the checks that draw in German.
#[cfg(test)]
pub(super) use tests::NOT_YET;

/// How a line of a tile is colored (4.12 colored what is good news or needs a look): plain text, the accent
/// for what is fine or done, gold for what asks for a look, red for what is wrong, and the muted color of
/// the secondary text for what only accompanies the line above.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Tone {
    #[default]
    Plain,
    Muted,
    Good,
    Warn,
    Bad,
}

/// What a tile offers to do on its own besides opening its page: a small link at its end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TileAction {
    /// Make a backup now (4.12: "Jetzt sichern" on the tile of the backup).
    BackupNow,
    /// Read the refinery terminal from the screen (4.12: "Scannen" on the tile of the refinery).
    RefineryScan,
    /// Look for a newer translation of every channel and fetch it (4.12: "Jetzt nach neuer Übersetzung sehen").
    TranslationCheck,
}

/// What one tile shows: its line and its detail lines, most important first.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Glance {
    pub line: String,
    pub details: Vec<String>,
    /// A share between 0 and 1 drawn as a bar under the line (the blueprints owned, the watch list found,
    /// the class of the computer).
    pub bar: Option<f32>,
    /// The color of the line and of the detail lines (a shorter list means plain for the rest).
    pub line_tone: Tone,
    pub tones: Vec<Tone>,
    /// Five lamps drawn as dots under the line (the Executive Hangar).
    pub lamps: Option<[versekit_core::contested::Lamp; versekit_core::contested::LAMPS]>,
    /// A link at the end of the tile; a click on it does that and not what a click on the tile does.
    pub action: Option<TileAction>,
    /// The key figure of the tile, drawn large and bold (4.12's big number): at the start of the line where the
    /// line begins with it, else before the line.
    pub figure: Option<String>,
    /// The bar is drawn in this many segments, lit from red to the accent (the class of the computer of 4.12).
    pub steps: Option<u8>,
}

impl Glance {
    pub(super) fn new(line: String, details: Vec<String>) -> Glance {
        Glance { line, details, ..Glance::default() }
    }

    pub(super) fn bar(mut self, share: f32) -> Glance {
        self.bar = Some(share.clamp(0.0, 1.0));
        self
    }

    pub(super) fn figure(mut self, figure: impl ToString) -> Glance {
        self.figure = Some(figure.to_string());
        self
    }

    /// The bar in `steps` segments.
    pub(super) fn steps(mut self, steps: u8) -> Glance {
        self.steps = Some(steps.max(1));
        self
    }

    pub(super) fn line_tone(mut self, tone: Tone) -> Glance {
        self.line_tone = tone;
        self
    }

    pub(super) fn lamps(mut self, lamps: [versekit_core::contested::Lamp; versekit_core::contested::LAMPS]) -> Glance {
        self.lamps = Some(lamps);
        self
    }

    /// Colors the detail line `at`.
    pub(super) fn tone_at(mut self, at: usize, tone: Tone) -> Glance {
        if self.tones.len() <= at {
            self.tones.resize(at + 1, Tone::Plain);
        }
        self.tones[at] = tone;
        self
    }

    /// The color of detail line `at`.
    pub(super) fn tone_of(&self, at: usize) -> Tone {
        self.tones.get(at).copied().unwrap_or_default()
    }
}

/// The counts of the overview on real data.
#[derive(Clone, Debug, Default)]
pub(super) struct Overview {
    /// When they were taken (seconds of the window's clock).
    pub at: f64,
    /// The language their texts are written in: a switch takes them again.
    pub lang: Option<Lang>,
    pub tiles: HashMap<&'static str, Glance>,
    /// Hours played on each of the last seven days, today last: (weekday, 0 for
    /// Monday, hours).
    pub week: Vec<(usize, f32)>,
    /// The head bar: all play time (and the running session), and its hover text.
    pub head: Option<(String, String)>,
    /// A refinery job is running: its time left is counted every second.
    pub ticking: bool,
    /// The tile of the Executive Hangar is on the overview: its countdown is counted every second.
    pub hangar_ticking: bool,
    /// When everything was counted last (`at` follows the refinery's second).
    pub full_at: f64,
    /// The tiles of 4.12 were counted (only while the overview is open).
    pub with_tiles: bool,
}

impl Overview {
    /// The tile `key` on real data: its counts, or that its page comes later.
    pub(super) fn tile(&self, key: &str) -> Glance {
        self.tiles.get(key).cloned().unwrap_or_else(|| Glance::new(missing(key), Vec::new()))
    }
}

impl SamplePage {
    /// Takes the counts of the overview again when they are older than ten seconds
    /// (only on real data). The tiles of 4.12 are counted only while the overview
    /// is open, at once when it opens; between two counts a running refinery job
    /// counts its time down every second.
    pub(super) fn refresh_overview(&mut self, now: f64) {
        if self.store.is_none() {
            return;
        }
        let lang = language::current();
        let tiles_wanted = self.start.is_some() && self.page.is_empty();
        let now_s = now_seconds();
        let hangar_start = self.contested.start();
        if let (Some(o), Some(store)) = (&mut self.overview, &self.store)
            && o.lang == Some(lang)
            && now - o.full_at < GLANCE_SECONDS
            && (o.with_tiles || !tiles_wanted)
        {
            if (o.ticking || o.hangar_ticking) && tiles_wanted && now - o.at >= TICK_SECONDS {
                if o.ticking {
                    o.tiles
                        .insert("raffinerie", super::glance_tiles::refinery_tile_with_scan(&store.paths, lang, now_s));
                }
                if o.hangar_ticking {
                    let tile = super::glance_tiles::hangar_tile(hangar_start, versekit_core::contested::now(), lang);
                    o.tiles.insert("hf_contested", tile.clone());
                    o.tiles.insert("executive_hangar", tile);
                }
                o.at = now;
            }
            return;
        }
        // The storage as the files hold it now, not as the storage page last read it.
        if tiles_wanted {
            self.stock.follow_files();
            // And the unit storage as the sync brought it, not as the page saw it last.
            self.org_stock.follow_store();
        }
        let Some(store) = &self.store else { return };
        let mut tiles = HashMap::new();
        if self.start.is_none() {
            if let Some(glance) = blueprints(&self.blueprints, lang) {
                tiles.insert("hf_liste", glance.0);
                tiles.insert("hf_fortschritt", glance.1);
            }
            tiles.insert("hf_meinlager", stock(&self.stock.rows(), lang));
            if let Some(glance) = contracts(&self.contracts, today_key(now_s), lang) {
                tiles.insert("hf_auftragslog", glance);
            }
        }
        let sessions = super::glance_tiles::played_spans(&store.paths);
        let week = week(&sessions, now_s);
        tiles.insert("spielzeit", play_tile(&sessions, &week, lang));
        let head = Some(head(&sessions, playtime::session_now(&store.paths), lang));
        // On real data, while the overview is open, the tiles of 4.12 and the pinned pages.
        let mut ticking = false;
        let mut hangar_ticking = false;
        if tiles_wanted {
            tiles.extend(self.tiles_of_4_12(&store.paths, lang, now_s));
            let jobs = versekit_core::refinery_jobs::load(&store.paths);
            ticking = !versekit_core::overview::refinery_current(&jobs, now_s as f64).0.is_empty();
            // The tile of the Executive Hangar counts while it stands on the board, shown or pinned as a page.
            hangar_ticking = self
                .tiles
                .iter()
                .any(|tile| !tile.hidden && (tile.key == "executive_hangar" || tile.key == "hf_contested"));
        }
        self.overview = Some(Overview {
            at: now,
            lang: Some(lang),
            tiles,
            week,
            head,
            ticking,
            hangar_ticking,
            full_at: now,
            with_tiles: tiles_wanted,
        });
    }
}

/// How often the counts are taken while a refinery job runs (seconds).
pub(super) const TICK_SECONDS: f64 = 1.0;

fn now_seconds() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// `2026-10-21` for the local day of `seconds`.
pub(super) fn today_key(seconds: i64) -> String {
    formats::local_time(seconds).map_or_else(String::new, |t| day_key(&t))
}

fn day_key(t: &LocalTime) -> String {
    format!("{:04}-{:02}-{:02}", t.year, t.month, t.day)
}

/// Monday 0 … Sunday 6 (days since 1970-01-01, a Thursday).
fn weekday(t: &LocalTime) -> usize {
    let (y, m, d) = (t.year - i64::from(t.month <= 2), i64::from(t.month), i64::from(t.day));
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    (days + 3).rem_euclid(7) as usize
}

/// The list: owned of all, and the share of each kind.
fn blueprints(data: &ListData, lang: Lang) -> Option<(Glance, Glance)> {
    if data.sample || data.catalog.blueprints.is_empty() {
        return None;
    }
    let progress = data.progress();
    let (have, total) = progress.iter().fold((0, 0), |sum, (_, _, h, t)| (sum.0 + h, sum.1 + t));
    let shares: Vec<String> = progress
        .iter()
        .filter(|(_, _, _, t)| *t > 0)
        .map(|(_, name, h, t)| format!("{name} {} %", h * 100 / t))
        .collect();
    let mut list = vec![tf_in(lang, "s_uv_fehlen", &[("count", Arg::from(total - have))])];
    list.extend(shares.iter().take(2).cloned());
    let of = |key: &str| tf_in(lang, key, &[("have", Arg::from(have)), ("total", Arg::from(total))]);
    let list = Glance::new(of("s_kd_von"), list);
    let progress = Glance::new(of("s_uv_von_bp"), shares);
    Some((list, progress))
}

/// The goods in SCU and the places they lie at, the fullest first; pieces are not SCU.
fn stock(rows: &[versekit_core::stock::Row], lang: Lang) -> Glance {
    let mut places: BTreeMap<&str, f64> = BTreeMap::new();
    for row in rows.iter().filter(|r| !r.piece && r.amount > 0.0) {
        *places.entry(row.place.trim()).or_default() += row.amount;
    }
    if places.is_empty() {
        return Glance::new(language::t_in(lang, "s_uv_lager_leer"), Vec::new());
    }
    let total: f64 = places.values().sum();
    let mut sorted: Vec<(&str, f64)> = places.into_iter().collect();
    sorted.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(b.0)));
    let scu = |amount: f64| format!("{} SCU", formats::fixed(lang, amount, if amount.fract() == 0.0 { 0 } else { 2 }));
    let nowhere = language::t_in(lang, "s_uv_ohne_ort");
    let details = sorted
        .iter()
        .map(|(place, amount)| {
            let place = if place.is_empty() { nowhere.as_str() } else { place };
            format!("{place}: {}", scu(*amount))
        })
        .collect();
    let line = if sorted.len() == 1 {
        tf_in(lang, "s_uv_lager_ein", &[("scu", Arg::from(scu(total)))])
    } else {
        tf_in(lang, "s_uv_lager_viele", &[("scu", Arg::from(scu(total))), ("places", Arg::from(sorted.len()))])
    };
    Glance::new(line, details)
}

/// Running contracts, and how many ended well today.
fn contracts(data: &LogData, today: String, lang: Lang) -> Option<Glance> {
    if data.sample {
        return None;
    }
    let open = data.missions.iter().filter(|m| m.state == log::RUNNING).count();
    let done = data.missions.iter().filter(|m| m.state == log::COMPLETED && m.until.starts_with(&today)).count();
    let line = if open == 1 {
        language::t_in(lang, "s_uv_offen_eins")
    } else {
        tf_in(lang, "s_uv_offen", &[("open", Arg::from(open))])
    };
    Some(Glance::new(line, vec![tf_in(lang, "s_uv_heute_fertig", &[("done", Arg::from(done))])]))
}

/// Seconds played on each local day, split at midnight.
fn per_day(sessions: &[(i64, i64)]) -> HashMap<String, i64> {
    let mut days: HashMap<String, i64> = HashMap::new();
    for &(from, to) in sessions {
        let mut at = from;
        while at < to {
            let Some(t) = formats::local_time(at) else { break };
            let into_day = i64::from(t.hour) * 3600 + i64::from(t.minute) * 60 + i64::from(t.second);
            let end = to.min(at + 86_400 - into_day);
            *days.entry(day_key(&t)).or_default() += end - at;
            at = end.max(at + 1);
        }
    }
    days
}

/// Hours on each of the last seven local days, today last.
fn week(sessions: &[(i64, i64)], now: i64) -> Vec<(usize, f32)> {
    let days = per_day(sessions);
    (0..7)
        .rev()
        .filter_map(|back| {
            let t = formats::local_time(now - back * 86_400)?;
            let seconds = days.get(&day_key(&t)).copied().unwrap_or(0);
            Some((weekday(&t), seconds as f32 / 3600.0))
        })
        .collect()
}

fn play_tile(sessions: &[(i64, i64)], week: &[(usize, f32)], lang: Lang) -> Glance {
    if sessions.is_empty() {
        return Glance::new(language::t_in(lang, "s_uv_keine_zeit"), Vec::new());
    }
    let hours_total: f32 = week.iter().map(|(_, h)| h).sum();
    let today = week.last().map_or(0.0, |(_, h)| *h);
    let longest = sessions.iter().map(|(from, to)| to - from).max().unwrap_or(0);
    let hours = |value: f32| Arg::from(formats::fixed(lang, f64::from(value), 1));
    Glance::new(
        tf_in(lang, "s_uv_sieben_h", &[("hours", hours(hours_total))]),
        vec![
            tf_in(lang, "s_uv_heute_h", &[("hours", hours(today))]),
            tf_in(lang, "s_uv_laengste", &[("time", Arg::from(formats::duration(longest as f64)))]),
        ],
    )
}

/// The play time of the head bar and its hover (4.12 `_playtime_display`): all play time as `3 h 05 min`, the
/// running session (`session` seconds) in brackets while the game runs, and since when it is counted. Without a
/// session recorded it says `0 min` and that counting begins with the first session.
pub(super) fn head(sessions: &[(i64, i64)], session: i64, lang: Lang) -> (String, String) {
    let total: i64 = sessions.iter().map(|(from, to)| to - from).sum();
    let mut text = playtime::as_text(total);
    if session > 0 {
        text.push_str(&format!("  (+{})", playtime::as_text(session)));
    }
    let hover = match sessions.first() {
        Some((first, _)) => {
            let since = formats::moment(lang, *first as f64, "m_erster_datum");
            tf_in(lang, "hf_zeit_h", &[("0", Arg::from(since))])
        }
        None => language::t_in(lang, "hf_zeit_h_leer"),
    };
    (text, hover)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The line of [`not_yet`] in German, as the checks that draw in German look for it.
    pub(in super::super) const NOT_YET: &str = "Kommt mit dem Bau dieser Seite.";

    #[test]
    fn the_line_for_a_page_that_comes_later_is_the_german_text_of_its_key() {
        assert_eq!(language::t_in(Lang::De, "s_uv_kommt"), NOT_YET);
    }

    #[test]
    fn the_play_time_counts_in_hours_not_days() {
        let all = [(1_000, 1_000 + 417 * 3600 + 12 * 60 + 59)];
        assert_eq!(head(&all, 0, Lang::De).0, "417 h 12 min");
        assert_eq!(head(&[(0, 2 * 3600)], 0, Lang::De).0, "2 h 00 min");
        assert_eq!(head(&[(0, 59)], 0, Lang::De).0, "0 min");
    }

    #[test]
    fn the_brackets_of_the_head_bar_hold_the_running_session_not_the_day() {
        // Two sessions today, the second one running for 42 minutes.
        let sessions = [(10_000, 10_000 + 3 * 3600 + 5 * 60), (30_000, 30_000 + 42 * 60)];
        let (text, hover) = head(&sessions, 42 * 60, Lang::De);
        assert_eq!(text, "3 h 47 min  (+42 min)");
        assert!(hover.starts_with("Deine Spielzeit seit "), "{hover}");
        assert_eq!(head(&sessions, 3 * 3600 + 5 * 60, Lang::De).0, "3 h 47 min  (+3 h 05 min)");
        // The game does not run: no brackets.
        assert_eq!(head(&sessions, 0, Lang::De).0, "3 h 47 min");
    }

    #[test]
    fn without_a_recorded_session_the_head_bar_says_zero_and_why() {
        let (text, hover) = head(&[], 0, Lang::De);
        assert_eq!(text, "0 min");
        assert_eq!(hover, language::t_in(Lang::De, "hf_zeit_h_leer"));
        assert_eq!(head(&[], 0, Lang::En).1, language::t_in(Lang::En, "hf_zeit_h_leer"));
    }

    fn at(year: i64, month: u32, day: u32) -> LocalTime {
        LocalTime { year, month, day, hour: 12, minute: 0, second: 0 }
    }

    #[test]
    fn weekdays_count_from_monday() {
        assert_eq!(weekday(&at(2026, 10, 21)), 2, "Wednesday");
        assert_eq!(weekday(&at(2026, 10, 25)), 6, "Sunday");
        assert_eq!(weekday(&at(2024, 2, 29)), 3, "Thursday");
        assert_eq!(weekday(&at(2000, 1, 1)), 5, "Saturday");
    }

    #[test]
    fn a_session_over_midnight_counts_on_both_days() {
        // 23:00 to 01:00 local, wherever this runs.
        let Some(noon) = formats::local_time(1_761_040_800) else { return };
        let into = i64::from(noon.hour) * 3600 + i64::from(noon.minute) * 60 + i64::from(noon.second);
        let eleven = 1_761_040_800 - into + 23 * 3600;
        let days = per_day(&[(eleven, eleven + 2 * 3600)]);
        assert_eq!(days.values().copied().collect::<Vec<_>>().iter().sum::<i64>(), 7200);
        assert_eq!(days.len(), 2, "{days:?}");
        assert!(days.values().all(|s| *s == 3600), "{days:?}");
    }

    #[test]
    fn the_stock_counts_scu_per_place_and_leaves_pieces_out() {
        use versekit_core::stock::{Row, Store};
        let row = |place: &str, amount: f64, piece: bool| Row {
            store: Store::Raw,
            index: 0,
            name: "Titanium".into(),
            amount,
            piece,
            quality: None,
            place: place.into(),
            stolen: false,
            cost: None,
            item: false,
            leftover: false,
        };
        let glance = stock(
            &[
                row("Area18", 100.0, false),
                row("Lorville", 12.5, false),
                row("Area18", 80.0, false),
                row("Area18", 3.0, true),
            ],
            Lang::De,
        );
        assert_eq!(glance.line, "192,50 SCU an 2 Orten");
        assert_eq!(glance.details, vec!["Area18: 180 SCU".to_string(), "Lorville: 12,50 SCU".to_string()]);
        assert_eq!(stock(&[], Lang::De).line, "Noch nichts eingelagert");
        assert_eq!(stock(&[], Lang::En).line, "Nothing stored yet");
    }
}
