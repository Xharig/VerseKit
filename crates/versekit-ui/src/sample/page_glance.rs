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

//! What a pinned page shows on its tile (real data): the most important state of the page, so that
//! the tile is more than the way to it. The key line says how it stands, the detail lines follow in
//! the order of importance and the tile shows as many as its size has room for, so a bigger tile
//! shows more and never an empty area. 4.12 showed only the short explanation on its cards; the
//! tiles "at a glance" of 4.12 (`glance_tiles`) are the least a page shows here.

use std::collections::HashMap;

use versekit_core::hangar;
use versekit_core::jsonio::{Map, Value};
use versekit_core::language::{Arg, Lang, relative_age_in, t_in, tf_in};
use versekit_core::mission_log as log;
use versekit_core::notepad::{self, Note};
use versekit_core::paths::Paths;
use versekit_core::{formats, overview, playtime, stock};

use super::SamplePage;
use super::glance::{Glance, Tone};
use super::glance_tiles::storage_tile;

/// How many detail lines a page offers at most; the tile cuts at its height.
const MOST: usize = 12;

fn glance(line: String, details: Vec<String>) -> Glance {
    Glance::new(line, details)
}

/// The stock page: the key line of the storage tile, then the largest stocks of the raw materials, the
/// goods and the items, each with its place.
pub(super) fn storage_page(rows: &[stock::Row], lang: Lang) -> Glance {
    let mut tile = storage_tile(rows, lang, 6);
    if rows.is_empty() {
        return tile;
    }
    // What the raw top did not name: goods and items by name, the most first.
    let decimal = formats::decimal_mark(lang);
    let mut named: Vec<(String, f64, bool, String)> = Vec::new();
    for row in rows.iter().filter(|r| r.store == stock::Store::Trade) {
        let name = row.name.trim();
        if name.is_empty() {
            continue;
        }
        match named.iter_mut().find(|(n, ..)| n.eq_ignore_ascii_case(name)) {
            Some(entry) => entry.1 += row.amount,
            None => named.push((name.to_string(), row.amount, row.piece, row.place.trim().to_string())),
        }
    }
    named.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.to_lowercase().cmp(&b.0.to_lowercase())));
    for (name, amount, piece, place) in named {
        if tile.details.len() >= 10 {
            break;
        }
        let unit = if piece { t_in(lang, "s_lg_stueck") } else { "SCU".to_string() };
        let amount = stock::amount_text(amount, piece, decimal);
        let text =
            tf_in(lang, "s_uv_menge", &[("0", Arg::from(name)), ("1", Arg::from(amount)), ("2", Arg::from(unit))]);
        tile.details.push(if place.is_empty() { text } else { format!("{text} · {place}") });
    }
    // Then the places: how much of the measured stock lies where, the fullest first.
    let mut places: Vec<(String, f64)> = Vec::new();
    for row in rows.iter().filter(|r| !r.piece && r.amount > 0.0) {
        let place = row.place.trim();
        match places.iter_mut().find(|(p, _)| p == place) {
            Some(entry) => entry.1 += row.amount,
            None => places.push((place.to_string(), row.amount)),
        }
    }
    places.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    let nowhere = t_in(lang, "s_uv_ohne_ort");
    for (place, amount) in places {
        if tile.details.len() >= MOST + 4 {
            break;
        }
        let place = if place.is_empty() { nowhere.clone() } else { place };
        let scu = format!("{} SCU", stock::amount_text(amount, false, decimal));
        tile.details.push(tf_in(lang, "s_kt_wert", &[("0", Arg::from(place)), ("1", Arg::from(scu))]));
    }
    tile
}

/// The blueprint list and its progress: owned of the catalog and the share, then what came this week and
/// the newest, what is missing and the progress of every kind. `None` while the page has only samples.
pub(super) fn blueprint_pages(
    page: &SamplePage,
    own: &[(String, Value)],
    lang: Lang,
    now: i64,
) -> Option<(Glance, Glance)> {
    let data = &page.blueprints;
    if data.sample || data.catalog.blueprints.is_empty() {
        return None;
    }
    let keys: std::collections::HashSet<&str> = data.catalog.blueprints.iter().map(|b| b.key.as_str()).collect();
    let total = data.catalog.blueprints.len();
    let numbers = overview::blueprint_numbers(own, &|key| keys.contains(key), total, now);
    let percent = numbers.mine as f64 * 100.0 / total.max(1) as f64;
    let line =
        format!("{} {}", numbers.mine, tf_in(lang, "s_uv_von", &[("0", Arg::from(total)), ("1", Arg::from(percent))]));
    let kinds: Vec<String> = data
        .progress()
        .into_iter()
        .filter(|(_, _, _, t)| *t > 0)
        .map(|(_, name, h, t)| {
            let share = Arg::from(h * 100 / t);
            tf_in(
                lang,
                "s_kt_anteil",
                &[("0", Arg::from(name)), ("1", Arg::from(h)), ("2", Arg::from(t)), ("3", share)],
            )
        })
        .collect();
    let mut list = vec![tf_in(lang, "s_uv_woche", &[("0", Arg::from(numbers.week))])];
    if let Some(at) = numbers.newest_at.filter(|_| !numbers.newest.is_empty()) {
        let ago = relative_age_in(lang, (now - at) as f64);
        list.push(tf_in(lang, "s_uv_neuester", &[("0", Arg::from(numbers.newest.as_str())), ("1", Arg::from(ago))]));
    }
    let came = overview::received(own);
    for (key, period) in [("s_uv_heute", overview::Period::Today), ("s_uv_monat", overview::Period::Month)] {
        list.push(tf_in(lang, key, &[("0", Arg::from(overview::count_in(&came, period, now)))]));
    }
    list.push(tf_in(lang, "s_uv_fehlen", &[("count", Arg::from(total.saturating_sub(numbers.mine)))]));
    list.extend(kinds.iter().cloned());
    let share = (numbers.mine as f64 / total.max(1) as f64) as f32;
    let mut list = glance(line.clone(), list).bar(share);
    if numbers.week > 0 {
        list = list.tone_at(0, Tone::Good);
    }
    Some((list, glance(line, kinds).bar(share)))
}

/// The contract log: the running contracts with their objectives, then the ones done today.
pub(super) fn contract_page(page: &SamplePage, today: &str, lang: Lang) -> Option<Glance> {
    let data = &page.contracts;
    if data.sample {
        return None;
    }
    let running: Vec<&log::Mission> =
        data.missions.iter().filter(|m| m.state.is_empty() || m.state == log::RUNNING).collect();
    let done: Vec<&log::Mission> =
        data.missions.iter().filter(|m| m.state == log::COMPLETED && m.until.starts_with(today)).collect();
    let line = if running.len() == 1 {
        t_in(lang, "s_uv_offen_eins")
    } else {
        tf_in(lang, "s_uv_offen", &[("open", Arg::from(running.len()))])
    };
    let mut details = vec![tf_in(lang, "s_uv_heute_fertig", &[("done", Arg::from(done.len()))])];
    for mission in running.iter().take(MOST) {
        let name = Arg::from(mission.name.as_str());
        details.push(if mission.goals_total > 0 {
            tf_in(
                lang,
                "s_kt_auftrag_ziele",
                &[("0", name), ("1", Arg::from(mission.goals_done)), ("2", Arg::from(mission.goals_total))],
            )
        } else {
            tf_in(lang, "s_kt_auftrag_laeuft", &[("0", name)])
        });
    }
    for mission in done.iter().take(MOST) {
        details.push(tf_in(lang, "s_kt_auftrag_fertig", &[("0", Arg::from(mission.name.as_str()))]));
    }
    Some(glance(line, details))
}

fn number_of(value: Option<&Value>) -> Option<f64> {
    match value {
        Some(Value::Int(text)) => text.parse().ok(),
        Some(Value::Float(x)) if x.is_finite() => Some(*x),
        _ => None,
    }
}

/// What the hangar holds, counted from its file (read only, nothing is mended or written here).
pub(super) fn hangar_numbers(data: &Map) -> HangarNumbers {
    let ships = hangar::ships(data);
    let mut out = HangarNumbers {
        ships: ships.len(),
        planned: hangar::wishlist(data).iter().map(|w| hangar::field_text(w, "name").to_string()).collect(),
        ..HangarNumbers::default()
    };
    for ship in &ships {
        if matches!(ship.iter().find(|(k, _)| k == "lti").map(|(_, v)| v), Some(Value::Bool(true))) {
            out.lti += 1;
        }
        if matches!(ship.iter().find(|(k, _)| k == "einsatzbereit").map(|(_, v)| v), Some(Value::Bool(true))) {
            out.ready += 1;
        }
        let months = number_of(ship.iter().find(|(k, _)| k == "versicherung").map(|(_, v)| v)).filter(|m| *m > 0.0);
        if let Some(months) = months {
            out.timed += 1;
            out.shortest = Some(out.shortest.map_or(months, |known| known.min(months)));
        }
        let name = hangar::field_text(ship, "name");
        if !name.is_empty() {
            out.names.push(name.to_string());
        }
        let open = hangar::open_count(ship);
        out.open_parts += open;
        if open > 0 {
            out.with_open.push((name.to_string(), open));
        }
    }
    out
}

#[derive(Debug, Default, PartialEq)]
pub(super) struct HangarNumbers {
    pub ships: usize,
    pub lti: usize,
    pub ready: usize,
    /// Ships insured for a time, and the shortest time left in months.
    pub timed: usize,
    pub shortest: Option<f64>,
    pub names: Vec<String>,
    /// Planned parts not ticked off yet, in all and by ship.
    pub open_parts: usize,
    pub with_open: Vec<(String, usize)>,
    /// The ships on the list of planned ships, A to Z.
    pub planned: Vec<String>,
}

/// The hangar page: how many ships, how many with lifetime insurance and ready for use, the shortest
/// insurance, then the names, and the ships planned.
pub(super) fn hangar_glance(numbers: &HangarNumbers, lang: Lang) -> Glance {
    let planned: Vec<String> = numbers
        .planned
        .iter()
        .take(MOST)
        .map(|name| tf_in(lang, "s_kt_hangar_geplant_schiff", &[("0", Arg::from(name.as_str()))]))
        .collect();
    let planned_line = tf_in(lang, "s_kt_hangar_geplant", &[("0", Arg::from(numbers.planned.len()))]);
    if numbers.ships == 0 {
        return if numbers.planned.is_empty() {
            glance(t_in(lang, "s_kt_hangar_leer"), Vec::new())
        } else {
            glance(t_in(lang, "s_kt_hangar_leer"), std::iter::once(planned_line).chain(planned).collect())
        };
    }
    let line = if numbers.ships == 1 {
        t_in(lang, "s_kt_hangar_eins")
    } else {
        tf_in(lang, "s_kt_hangar_zahl", &[("0", Arg::from(numbers.ships))])
    };
    let mut details = Vec::new();
    if numbers.lti > 0 {
        details.push(tf_in(lang, "s_kt_hangar_lti", &[("0", Arg::from(numbers.lti))]));
    }
    if numbers.ready > 0 {
        details.push(tf_in(lang, "s_kt_hangar_bereit", &[("0", Arg::from(numbers.ready))]));
    }
    if let Some(shortest) = numbers.shortest {
        let months = Arg::from(formats::fixed(lang, shortest, if shortest.fract() == 0.0 { 0 } else { 1 }));
        // One month is "1 Monat", not "1 Monate".
        let key = if shortest == 1.0 { "s_kt_hangar_zeit_eins" } else { "s_kt_hangar_zeit" };
        details.push(tf_in(lang, key, &[("0", Arg::from(numbers.timed)), ("1", months)]));
    }
    if numbers.open_parts > 0 {
        details.push(tf_in(lang, "s_hg_offen", &[("n", Arg::from(numbers.open_parts))]));
    }
    if !numbers.planned.is_empty() {
        details.push(planned_line);
    }
    details.extend(numbers.names.iter().take(MOST).cloned());
    details.extend(planned);
    glance(line, details)
}

/// The shopping list: the parts planned on the ships and not ticked off, in all and ship by ship.
pub(super) fn shopping_glance(numbers: &HangarNumbers, lang: Lang) -> Glance {
    if numbers.open_parts == 0 {
        return glance(t_in(lang, "s_kt_teile_keine"), Vec::new());
    }
    let details = numbers
        .with_open
        .iter()
        .take(MOST)
        .map(|(ship, open)| {
            tf_in(lang, "s_kt_schiff_teile", &[("0", Arg::from(ship.as_str())), ("1", Arg::from(*open))])
        })
        .collect();
    glance(tf_in(lang, "s_hg_offen", &[("n", Arg::from(numbers.open_parts))]), details)
}

/// The crafting plan: the items noted for crafting or buying, with how many and what is to be done.
pub(super) fn plan_glance(notes: &[Note], lang: Lang) -> Glance {
    if notes.is_empty() {
        return glance(t_in(lang, "s_kt_plan_leer"), Vec::new());
    }
    let mut tones = Vec::new();
    let details = notes
        .iter()
        .take(MOST)
        .map(|note| {
            let (word, tone) = if note.done {
                ("s_kt_note_fertig", Tone::Good)
            } else if note.way == "kaufen" {
                ("s_kt_note_kaufen", Tone::Plain)
            } else {
                ("s_kt_note_bauen", Tone::Plain)
            };
            tones.push(tone);
            tf_in(
                lang,
                "s_kt_note",
                &[
                    ("0", Arg::from(note.name.as_str())),
                    ("1", Arg::from(note.count)),
                    ("2", Arg::from(t_in(lang, word))),
                ],
            )
        })
        .collect();
    Glance { tones, ..glance(tf_in(lang, "s_kt_plan_zahl", &[("0", Arg::from(notes.len()))]), details) }
}

/// What to farm: how many pieces are to be crafted and which materials are missing for them, the largest gap
/// first. Where the recipes are not there yet a line says so.
pub(super) fn farm_glance(page: &SamplePage, paths: &Paths, notes: &[Note], lang: Lang) -> Glance {
    use versekit_gamedata::farm;
    if notes.is_empty() {
        return glance(t_in(lang, "s_fl_nichts_geplant"), Vec::new());
    }
    use crate::crafting_page::ValuesWait;
    let Some((values, book, data)) = page.crafting.parts() else {
        // What the page says in the same case: the values are read, reading failed, or there are none.
        let (key, tone) = match page.crafting.values_wait() {
            Some(ValuesWait::Reading) => ("s_p9_werte_lesen", Tone::Plain),
            Some(ValuesWait::Failed) => ("s_p9_werte_fehlen", Tone::Warn),
            _ => ("s_he_keine_daten", Tone::Plain),
        };
        return glance(t_in(lang, key), Vec::new()).line_tone(tone);
    };
    let planned = farm::posts_of(values, book, notes);
    let stock = stock::load(&paths.app_file(stock::RAW_FILE), "overview.farm");
    let list = farm::farm_list(values, book, &planned, &stock, data);
    if list.posts == 0 {
        // As the page: only where no item lacks its recipe is everything bought or ticked off.
        if list.without_recipe.is_empty() {
            return glance(t_in(lang, "s_p9_alles_gekauft"), Vec::new());
        }
        let key = if list.without_recipe.len() == 1 { "s_p9_ohne_rezept_eins" } else { "s_fl_ohne_rezept" };
        let said = tf_in(
            lang,
            key,
            &[("n", Arg::from(list.without_recipe.len())), ("teile", Arg::from(list.without_recipe.join(", ")))],
        );
        return glance(said, Vec::new()).line_tone(Tone::Warn);
    }
    let decimal = formats::decimal_mark(lang);
    let pieces = tf_in(lang, "s_kt_farm_stuecke", &[("0", Arg::from(list.posts))]);
    if list.missing.is_empty() {
        return glance(t_in(lang, "s_fl_alles_da"), vec![pieces]).line_tone(Tone::Good);
    }
    let mut details = vec![pieces];
    details.extend(list.missing.iter().take(MOST).map(|need| {
        // The unit of the material: goods counted in pieces say pieces, as the page does.
        let piece = data.is_piece(&need.material);
        let unit = if piece { t_in(lang, "s_lg_stueck") } else { "SCU".to_string() };
        let gap = format!("{} {unit}", stock::amount_text(need.missing, piece, decimal));
        tf_in(lang, "s_kt_bedarf_fehlt", &[("0", Arg::from(need.material.as_str())), ("1", Arg::from(gap))])
    }));
    // A material in two qualities is one material that is missing.
    let materials: std::collections::BTreeSet<String> =
        list.missing.iter().map(|need| need.material.to_lowercase()).collect();
    glance(tf_in(lang, "s_kt_farm_fehlen", &[("0", Arg::from(materials.len()))]), details).line_tone(Tone::Warn)
}

/// The unit storage as it was fetched: how many lots in how many units, how old, then the largest lots.
pub(super) fn org_stock_glance(page: &SamplePage, lang: Lang) -> Glance {
    let Some(data) = page.org_stock.data() else {
        return glance(t_in(lang, "s_kt_staffel_leer"), Vec::new());
    };
    let decimal = formats::decimal_mark(lang);
    let mut lots: Vec<&versekit_core::basetool::orgstock::Lot> = data.lots.iter().collect();
    lots.sort_by(|a, b| b.amount.total_cmp(&a.amount).then(a.name.cmp(&b.name)));
    let mut details: Vec<String> = page
        .org_stock
        .age()
        .map(|age| tf_in(lang, "s_kt_stand", &[("0", Arg::from(relative_age_in(lang, age)))]))
        .into_iter()
        .collect();
    details.extend(lots.iter().take(MOST).map(|lot| {
        let piece = lot.piece();
        let unit = if piece { t_in(lang, "s_lg_stueck") } else { "SCU".to_string() };
        let amount = Arg::from(stock::amount_text(lot.amount, piece, decimal));
        let text =
            tf_in(lang, "s_uv_menge", &[("0", Arg::from(lot.name.as_str())), ("1", amount), ("2", Arg::from(unit))]);
        let place = lot.place.trim();
        let text = if place.is_empty() { text } else { format!("{text} · {place}") };
        if lot.unit_name.is_empty() { text } else { format!("{text} · {}", lot.unit_name) }
    }));
    glance(
        tf_in(lang, "s_kt_staffel_zahl", &[("0", Arg::from(data.lots.len())), ("1", Arg::from(data.units.len()))]),
        details,
    )
}

/// The prices for selling: how many goods have a price, and how old the list is.
pub(super) fn selling_page(paths: &Paths, lang: Lang, now: i64) -> Glance {
    let selling = versekit_core::selling::Selling::load(paths);
    let goods = selling.goods().len();
    if goods == 0 {
        return glance(t_in(lang, "s_kt_preise_leer"), Vec::new());
    }
    let details = selling
        .age(now as f64)
        .map(|age| tf_in(lang, "s_kt_stand", &[("0", Arg::from(relative_age_in(lang, age)))]))
        .into_iter()
        .collect();
    glance(tf_in(lang, "s_kt_preise", &[("0", Arg::from(goods))]), details)
}

/// The trade routes: from how many starting points trips are saved.
pub(super) fn routes_page(paths: &Paths, lang: Lang) -> Glance {
    let starts = versekit_core::routes::Routes::load(paths).known_starts(&[]);
    if starts == 0 {
        return glance(t_in(lang, "s_kt_routen_leer"), Vec::new());
    }
    glance(tf_in(lang, "s_kt_routen", &[("0", Arg::from(starts))]), Vec::new())
}

/// The shops: how many parts the catalog knows.
pub(super) fn shops_page(paths: &Paths, lang: Lang) -> Glance {
    let catalog = versekit_core::shops::Catalog::load(paths);
    if !catalog.ready() {
        return glance(t_in(lang, "s_kt_laeden_leer"), Vec::new());
    }
    glance(tf_in(lang, "s_kt_laeden", &[("0", Arg::from(catalog.items().len()))]), Vec::new())
}

/// The pages of the statistics, from the saved evaluation: the key figure first, the others under it.
pub(super) fn statistics_glance(key: &str, paths: &Paths, lang: Lang) -> Option<Glance> {
    use crate::statistics_page::{Page, Report};
    let page = Page::of(key)?;
    let report = Report::build(paths, None, None);
    let value = |label: &str, value: String| {
        tf_in(lang, "s_kt_wert", &[("0", Arg::from(t_in(lang, label))), ("1", Arg::from(value))])
    };
    let count = |n: i64| formats::fixed(lang, n as f64, 0);
    let times = |name: &str, n: i64| tf_in(lang, "s_kt_mal", &[("0", Arg::from(name)), ("1", Arg::from(n))]);
    if report.evaluation.sessions == 0 && !matches!(page, Page::Overview) {
        return Some(glance(t_in(lang, "s_kt_leer"), Vec::new()));
    }
    let last = report.evaluation.last.map_or_else(|| t_in(lang, "s_kt_nie"), |at| formats::date_time(lang, at));
    let tile = match page {
        // Nothing recorded yet: the state of the evaluation is what there is to tell.
        Page::Overview if report.evaluation.sessions == 0 => glance(
            value("s_sa_zuletzt", last),
            vec![
                value("s_sa_sitzungen", count(report.evaluation.sessions as i64)),
                value("s_sa_dateien", count(report.evaluation.files as i64)),
            ],
        ),
        Page::Overview => {
            let s = &report.summary;
            let mut details = vec![
                value("s_sx_sitzungen", count(s.sessions)),
                value("s_sx_auftraege", count(s.contracts)),
                value("s_sx_spruenge", count(s.jumps)),
                value("s_sx_tode", count(s.deaths)),
                value("s_sx_verloren", count(s.lost)),
            ];
            if let Some(since) = s.since {
                details.push(tf_in(lang, "s_sx_seit", &[("0", Arg::from(formats::date_time(lang, since as f64)))]));
            }
            details.push(value("s_sa_zuletzt", last));
            glance(value("s_sx_spielzeit", playtime::as_text(s.playtime)), details)
        }
        Page::Ships => {
            let s = &report.ships;
            let line = match &s.most {
                Some((name, _)) => value("s_ss_meist", name.clone()),
                None => t_in(lang, "s_kt_leer"),
            };
            let mut details = vec![
                value("s_ss_sitzungen_mit", count(s.sessions_with_ship)),
                value("s_ss_schiffe", count(s.ships as i64)),
            ];
            details.extend(s.used.iter().take(MOST).map(|(name, n)| times(name, *n)));
            glance(line, details)
        }
        Page::Missions => {
            let s = &report.missions;
            let mut details = vec![value("s_sm_fehl", count(s.failed)), value("s_sm_abbruch", count(s.abandoned))];
            details.extend(s.frequent.iter().take(MOST).map(|(name, n)| times(name, *n)));
            glance(value("s_sm_fertig", count(s.completed)), details)
        }
        Page::Quantum => {
            let s = &report.quantum;
            glance(value("s_sq_angekommen", count(s.jumps)), vec![value("s_sq_wahl", count(s.selections))])
        }
        Page::Stability => {
            let s = &report.stability;
            glance(
                value("s_sb_sauber", count(s.clean)),
                vec![value("s_sb_absturz", count(s.crashed)), value("s_sb_offen", count(s.open))],
            )
        }
    };
    Some(tile)
}

impl SamplePage {
    /// The tiles of the pinned pages that have data of their own; `base` holds the tiles of 4.12 already
    /// counted, which a page of the same subject shares. Pages without one keep their short explanation.
    pub(super) fn page_glances(
        &self,
        paths: &Paths,
        own: &[(String, Value)],
        lang: Lang,
        now: i64,
        today: &str,
        base: &HashMap<&'static str, Glance>,
    ) -> HashMap<&'static str, Glance> {
        let mut out: HashMap<&'static str, Glance> = HashMap::new();
        // Only what is pinned is worked out: the files are read every ten seconds while the overview is open.
        let pinned = |key: &str| self.tiles.iter().any(|tile| tile.pinned && tile.key == key);
        if pinned("hf_meinlager") {
            out.insert("hf_meinlager", storage_page(&self.stock.rows(), lang));
        }
        if (pinned("hf_liste") || pinned("hf_fortschritt"))
            && let Some((list, progress)) = blueprint_pages(self, own, lang, now)
        {
            out.insert("hf_liste", list);
            out.insert("hf_fortschritt", progress);
        }
        if pinned("hf_auftragslog")
            && let Some(contracts) = contract_page(self, today, lang)
        {
            out.insert("hf_auftragslog", contracts);
        }
        // The hangar file is read once for the three pages that are made of it.
        if pinned("hf_hangar") || pinned("hf_einkaufsliste") || pinned("hf_herstellung") || pinned("hf_farmliste") {
            let data = hangar::read(&paths.app_file(hangar::FILE));
            let numbers = data.as_ref().map(hangar_numbers).unwrap_or_default();
            out.insert("hf_hangar", hangar_glance(&numbers, lang));
            out.insert("hf_einkaufsliste", shopping_glance(&numbers, lang));
            // The notes of the crafting plan live in the same file.
            let notes = data.as_ref().map(notepad::notes).unwrap_or_default();
            out.insert("hf_herstellung", plan_glance(&notes, lang));
            if pinned("hf_farmliste") {
                out.insert("hf_farmliste", farm_glance(self, paths, &notes, lang));
            }
        }
        if pinned("hf_staffellager") {
            out.insert("hf_staffellager", org_stock_glance(self, lang));
        }
        if pinned("hf_verkauf") {
            out.insert("hf_verkauf", selling_page(paths, lang, now));
        }
        if pinned("hf_routen") {
            out.insert("hf_routen", routes_page(paths, lang));
        }
        if pinned("hf_laeden") {
            out.insert("hf_laeden", shops_page(paths, lang));
        }
        for tile in self.tiles.iter().filter(|tile| tile.pinned) {
            if let Some(glance) = statistics_glance(&tile.key, paths, lang)
                && let Some((_, key)) = overview::PAGES.iter().find(|(_, key)| *key == tile.key)
            {
                out.insert(key, glance);
            }
        }
        // The same subject as a tile of 4.12: the tile is the page's.
        for (page, tile) in [
            ("hf_bedarf", "bedarf"),
            ("hf_raffinerien", "raffinerie"),
            ("hf_uebersetzung", "uebersetzung"),
            ("hf_sichern", "sicherung"),
            ("hf_serverstatus", "dienste"),
            ("k_basetool_verbindung", "dienste"),
            ("hf_patchaenderungen", "neu_patch"),
            ("hf_spiel", "spiel"),
            ("hf_ordner", "spiel"),
            ("hf_grafik", "rechner"),
        ] {
            if let Some(shared) = base.get(tile) {
                out.insert(page, shared.clone());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ship(text: &str) -> Map {
        match versekit_core::jsonio::loads(text).expect("json") {
            Value::Object(map) => map,
            _ => panic!("object"),
        }
    }

    #[test]
    fn the_hangar_counts_lifetime_insurance_ready_ships_and_the_shortest_insurance() {
        let data = ship(
            r#"{"format": 1, "schiffe": [
            {"name": "Carrack", "lti": true, "belegung": {}},
            {"name": "Cutlass Black", "versicherung": 120, "einsatzbereit": true, "belegung": {}},
            {"name": "Freelancer MAX", "versicherung": 6, "belegung": {}},
            {"name": "Avenger Titan", "belegung": {}}]}"#,
        );
        let numbers = hangar_numbers(&data);
        assert_eq!((numbers.ships, numbers.lti, numbers.ready, numbers.timed), (4, 1, 1, 2));
        assert_eq!(numbers.shortest, Some(6.0));
        let tile = hangar_glance(&numbers, Lang::De);
        assert_eq!(tile.line, "4 Schiffe im Hangar");
        assert_eq!(tile.details[0], "1 mit LTI");
        assert_eq!(tile.details[1], "1 einsatzbereit");
        assert_eq!(tile.details[2], "2 mit Versicherung auf Zeit, die kürzeste noch 6 Monate");
        assert_eq!(&tile.details[3..], ["Carrack", "Cutlass Black", "Freelancer MAX", "Avenger Titan"]);
        assert_eq!(hangar_glance(&HangarNumbers::default(), Lang::En).line, "No ships in the hangar yet");
        let one = HangarNumbers { ships: 1, names: vec!["Carrack".into()], ..HangarNumbers::default() };
        assert_eq!(hangar_glance(&one, Lang::En).line, "1 ship in the hangar");
    }

    #[test]
    fn the_shopping_list_counts_the_parts_still_to_get_ship_by_ship() {
        let data = ship(
            r#"{"format": 1, "schiffe": [
            {"name": "Carrack", "belegung": {"a": {"ref": "x", "name": "X", "weg": "kaufen"}, "b": {"ref": "y", "erledigt": true}, "c": {"ref": "z", "weg": "kaufen"}}},
            {"name": "Gladius", "belegung": {"a": {"ref": "x", "erledigt": true}}},
            {"name": "Avenger Titan", "belegung": {"a": {"ref": "x", "weg": "kaufen"}}}]}"#,
        );
        let numbers = hangar_numbers(&data);
        assert_eq!(numbers.open_parts, 3);
        let tile = shopping_glance(&numbers, Lang::De);
        assert_eq!(tile.line, "3 noch zu besorgen");
        assert_eq!(tile.details, ["Carrack · 2 noch zu besorgen", "Avenger Titan · 1 noch zu besorgen"]);
        assert_eq!(shopping_glance(&HangarNumbers::default(), Lang::En).line, "Nothing left to get");
        // The hangar says it too, between the readiness and the names.
        let hangar = hangar_glance(&numbers, Lang::De);
        assert!(hangar.details.contains(&"3 noch zu besorgen".to_string()), "{hangar:?}");
    }

    #[test]
    fn pages_without_any_data_say_so_instead_of_staying_blank() {
        let dir = std::env::temp_dir().join(format!("versekit-seitenkachel-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let paths = Paths::new(super::super::real_data::host_apart(&dir));
        // Each says what is missing and where it comes from.
        assert!(selling_page(&paths, Lang::De, 1000).line.starts_with("Noch keine Preise geholt."));
        assert!(routes_page(&paths, Lang::De).line.starts_with("Noch keine Routen nachgesehen."));
        assert!(shops_page(&paths, Lang::En).line.starts_with("No shop catalog yet."));
        for key in ["hf_st_schiffe", "hf_st_auftraege", "hf_st_quantum", "hf_st_stabil"] {
            let tile = statistics_glance(key, &paths, Lang::De).expect("a page of the statistics");
            assert!(tile.line.starts_with("Noch nichts erfasst."), "{key}");
        }
        // The overview of an empty folder tells the state of the evaluation, as the page does.
        let evaluation = statistics_glance("hf_st_uebersicht", &paths, Lang::De).expect("the overview");
        assert_eq!(evaluation.line, "Zuletzt ausgewertet: noch nie");
        assert!(statistics_glance("hf_liste", &paths, Lang::De).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_crafting_plan_names_what_is_noted_and_what_is_to_be_done_with_it() {
        let note = |name: &str, count: i64, way: &str, done: bool| Note {
            name: name.into(),
            reference: String::new(),
            count,
            way: way.into(),
            done,
        };
        let notes =
            [note("Frostline", 2, "", false), note("Arc Shield", 1, "kaufen", false), note("Helm", 1, "bauen", true)];
        let tile = plan_glance(&notes, Lang::De);
        assert_eq!(tile.line, "Geplante Gegenstände: 3");
        assert_eq!(tile.details, ["Frostline ×2 · bauen", "Arc Shield ×1 · kaufen", "Helm ×1 · fertig"]);
        assert_eq!(tile.tone_of(2), Tone::Good, "what is done stands in the accent");
        assert_eq!(tile.tone_of(0), Tone::Plain);
        assert_eq!(plan_glance(&[], Lang::En).line, "Nothing planned yet. Note items in Crafting.");
    }

    #[test]
    fn the_farm_list_and_the_unit_storage_say_what_is_missing_before_they_have_figures() {
        let page = SamplePage::preview();
        assert_eq!(
            org_stock_glance(&page, Lang::De).line,
            "Noch nichts geholt. Klick im Lager der Einheit auf Jetzt abgleichen."
        );
        let dir = std::env::temp_dir().join(format!("versekit-farm-kachel-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let paths = Paths::new(super::super::real_data::host_apart(&dir));
        // Nothing noted: the sentence of the page.
        let none = farm_glance(&page, &paths, &[], Lang::De);
        assert!(none.line.starts_with("Nichts vorgemerkt."), "{none:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_farm_list_counts_the_pieces_and_names_the_materials_that_are_missing_as_the_page_does() {
        let note = |name: &str, count: i64, way: &str| Note {
            name: name.into(),
            reference: String::new(),
            count,
            way: way.into(),
            done: false,
        };
        // The sample page holds sample recipes (Frostline is one) and the folder holds no stock.
        let page = SamplePage::preview();
        let dir = std::env::temp_dir().join(format!("versekit-farm-werte-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let paths = Paths::new(super::super::real_data::host_apart(&dir));
        let tile = farm_glance(&page, &paths, &[note("Frostline", 2, "")], Lang::De);
        assert_eq!(tile.line_tone, Tone::Warn, "{tile:?}");
        assert!(tile.line.starts_with("Rohstoffe, die fehlen: "), "{tile:?}");
        assert_eq!(tile.details[0], "Zu bauen: 2");
        assert!(tile.details.len() > 1 && tile.details[1..].iter().all(|d| d.contains(" · fehlt: ")), "{tile:?}");
        assert!(tile.details[1].ends_with("SCU") || tile.details[1].ends_with("Stück"), "{tile:?}");
        // What is bought is not built: nothing left to do.
        let bought = farm_glance(&page, &paths, &[note("Frostline", 2, "kaufen")], Lang::De);
        assert_eq!(bought.line, "Nichts mehr zu bauen: alles gekauft oder abgehakt.");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_stock_page_names_raw_materials_goods_and_items_with_their_place() {
        use versekit_core::stock::{Row, Store};
        let row = |store: Store, name: &str, amount: f64, place: &str, piece: bool| Row {
            store,
            index: 0,
            name: name.into(),
            amount,
            piece,
            quality: None,
            place: place.into(),
            stolen: false,
            cost: None,
            item: false,
            leftover: false,
        };
        let rows = [
            row(Store::Raw, "Titanium", 100.0, "Area18", false),
            row(Store::Raw, "Gold", 12.0, "Lorville", false),
            row(Store::Trade, "Agricium", 40.0, "Orison", false),
            row(Store::Trade, "Medpen", 6.0, "Orison", true),
        ];
        let tile = storage_page(&rows, Lang::De);
        assert_eq!(tile.line, "Rohstoffe: 2 · Waren: 2 · Items: 0");
        // Two raw materials, two goods, then the places with their measured stock, the fullest first.
        assert_eq!(tile.details.len(), 7, "{:?}", tile.details);
        assert!(tile.details[0].starts_with("Titanium"), "{:?}", tile.details);
        assert!(tile.details[2].starts_with("Agricium") && tile.details[2].ends_with("Orison"), "{:?}", tile.details);
        assert_eq!(&tile.details[4..], ["Area18: 100 SCU", "Orison: 40 SCU", "Lorville: 12 SCU"]);
        // 4.12 named three; a tile of the page names every stock it has room for.
        assert_eq!(storage_tile(&rows, Lang::De, 3).details.len(), 2);
        assert_eq!(storage_page(&[], Lang::De).line, "Noch nichts eingelagert");
    }
}
