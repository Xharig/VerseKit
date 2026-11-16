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

//! The page "Auftragsfinder": every contract of the stored game data ([`versekit_gamedata::contract_finder`]) in a
//! list that filters by a text, the giver, the star system, the lowest rank, blueprints and sharing, and sorts by each
//! of its values. Each contract is a title line with its giver and a wrapped line of its values; a value the data do
//! not state reads "unknown".
//!
//! The page reads the files when it is first shown and again when one of them was written since (a new catalog
//! after a patch), at most once in [`LOOK_SECONDS`].

use egui::{RichText, Ui};
use versekit_core::language::{self, Arg, t, tf};
use versekit_core::paths::Paths;
use versekit_core::theme::Scheme;
use versekit_gamedata::contract_finder::{self, Choices, Column, Contract, Finder, Gain, Rank, Stamps, View};

use crate::color;
use crate::layout::GAP;
use crate::widgets::{self, ButtonKind};

/// How many contracts the list shows at first, and how many more each click on "show more" adds.
const PAGE_ROWS: usize = 100;
/// How often the files are looked at for a change, in seconds.
const LOOK_SECONDS: f64 = 5.0;
/// The space between two columns: the least widths of eight columns leave little room in the smallest window.
const COLUMN_GAP: f32 = 4.0;
/// The columns of the list in the order they stand: the column, its text key and its share of what is left after
/// the least widths (the shares add up to one).
const COLUMNS: [(Column, &str, f32); 8] = [
    (Column::Title, "s_af_sp_titel", 0.20),
    (Column::Giver, "s_af_sp_auftraggeber", 0.12),
    (Column::Gain, "s_af_sp_ruf", 0.13),
    (Column::Cooldown, "s_af_sp_wartezeit", 0.11),
    (Column::Shareable, "s_af_sp_teilbar", 0.07),
    (Column::Region, "s_af_sp_region", 0.11),
    (Column::Rank, "s_af_sp_rang", 0.14),
    (Column::Blueprints, "s_af_sp_bauplan", 0.12),
];

/// What the page keeps.
pub struct ContractFinderState {
    /// The data folder; `None` for the preview, which reads nothing.
    paths: Option<Paths>,
    finder: Finder,
    choices: Choices,
    pub view: View,
    /// The drop-down filters as the filter bar holds them: giver, star system, rank (0: all).
    filters: [usize; 3],
    /// The places of the contracts shown, as arranged last; `None` when something changed.
    arranged: Option<Vec<usize>>,
    shown_rows: usize,
    /// When the files were looked at last, and what they said.
    looked: f64,
    stamps: Option<Stamps>,
    /// The least width of each column, measured from the heads and the texts shown; kept while they stay the same.
    least: Option<(LeastKey, Vec<f32>)>,
}

/// What the least widths of the columns depend on: the words of the heads and "unknown" (the language), how many
/// rows are shown, the column sorted (its head carries an arrow), and the pixels of a point.
#[derive(PartialEq)]
struct LeastKey {
    words: String,
    rows: usize,
    sorted: Column,
    pixels: i64,
}

impl ContractFinderState {
    /// The page of the data folder; the contracts come with the first frame.
    pub fn with_files(paths: &Paths) -> ContractFinderState {
        ContractFinderState {
            paths: Some(Paths::new(paths.host.clone())),
            ..ContractFinderState::of(Finder::default())
        }
    }

    /// The page over `finder`, reading nothing.
    pub fn of(finder: Finder) -> ContractFinderState {
        ContractFinderState {
            paths: None,
            choices: Choices::of(&finder.contracts),
            finder,
            view: View::new(),
            filters: [0; 3],
            arranged: None,
            shown_rows: PAGE_ROWS,
            looked: f64::NEG_INFINITY,
            stamps: None,
            least: None,
        }
    }

    /// The least width of each column of [`COLUMNS`] for the contracts at `places` that are shown: the head with the
    /// arrow of its sort, and the longest word of any text in the column with the space behind it, so no word is
    /// broken in the middle and no head wraps. Measured again only when one of those changed.
    fn least_widths(&mut self, ui: &Ui, places: &[usize]) -> Vec<f32> {
        let shown = places.len().min(self.shown_rows);
        let key = LeastKey {
            words: COLUMNS.iter().map(|(_, key, _)| t(key)).chain([t("s_af_unbekannt")]).collect::<Vec<_>>().join("|"),
            rows: shown,
            sorted: self.view.sort,
            pixels: (ui.ctx().pixels_per_point() * 100.0).round() as i64,
        };
        if let Some((kept, widths)) = &self.least
            && *kept == key
        {
            return widths.clone();
        }
        let arrow = 14.0 + COLUMN_GAP;
        let longest_word = |text: &str, strong: bool| {
            text.split_inclusive(char::is_whitespace)
                .map(|word| {
                    let rich = if strong { RichText::new(word).strong() } else { RichText::new(word) };
                    widgets::one_line_width(ui, rich, egui::TextStyle::Body)
                })
                .fold(0.0, f32::max)
        };
        let mut widths: Vec<f32> = COLUMNS
            .iter()
            .map(|(column, key, _)| {
                let head = widgets::one_line_width(ui, RichText::new(t(key)).strong(), egui::TextStyle::Body);
                head + if *column == self.view.sort { arrow } else { 0.0 }
            })
            .collect();
        for &at in places.iter().take(shown) {
            let Some(contract) = self.finder.contracts.get(at) else { continue };
            let facts = facts(contract);
            let title = contract.title.as_deref().unwrap_or(&contract.key);
            let giver = contract.giver.clone().unwrap_or_else(|| t("s_af_unbekannt"));
            for (column, width) in widths.iter_mut().enumerate() {
                let (text, strong) = match column {
                    // The key shown where no title is known is an identifier that may break anywhere.
                    0 if contract.title.is_none() => continue,
                    0 => (title, true),
                    1 => (giver.as_str(), false),
                    _ => (facts.get(column - 2).map_or("", String::as_str), false),
                };
                *width = width.max(longest_word(text, strong));
            }
        }
        self.least = Some((key, widths.clone()));
        widths
    }

    /// The preview: made-up contracts, nothing read.
    pub fn sample() -> ContractFinderState {
        ContractFinderState::of(sample_finder())
    }

    /// The contracts the page knows.
    pub fn contracts(&self) -> &[Contract] {
        &self.finder.contracts
    }

    /// The contracts the list shows, all of them, in their order.
    pub fn arranged(&mut self) -> Vec<&Contract> {
        let places = self.arranged_places().to_vec();
        places.iter().filter_map(|&at| self.finder.contracts.get(at)).collect()
    }

    fn arranged_places(&mut self) -> &[usize] {
        if self.arranged.is_none() {
            self.arranged = Some(contract_finder::arrange(&self.finder.contracts, &self.view));
        }
        self.arranged.as_deref().unwrap_or_default()
    }

    /// Something of the view changed: the list is arranged anew and starts at its top.
    fn rearrange(&mut self) {
        self.arranged = None;
        self.least = None;
        self.shown_rows = PAGE_ROWS;
    }

    /// Reads the files anew when one of them was written since the last look; looks at most once in
    /// [`LOOK_SECONDS`].
    pub fn refresh(&mut self, now: f64) {
        if now - self.looked < LOOK_SECONDS {
            return;
        }
        self.looked = now;
        let Some(paths) = &self.paths else { return };
        let stamps = Stamps::of(paths);
        if self.stamps.as_ref() == Some(&stamps) {
            return;
        }
        self.finder = contract_finder::load(paths);
        self.choices = Choices::of(&self.finder.contracts);
        self.stamps = Some(stamps);
        self.take_filters();
    }

    /// The drop-down filters into the view, by name, so that a list read anew keeps what was chosen where it still
    /// offers it.
    fn take_filters(&mut self) {
        let keep = |chosen: &Option<String>, names: Vec<&String>| {
            chosen.as_ref().and_then(|name| names.iter().position(|n| *n == name)).map_or(0, |at| at + 1)
        };
        let held = self.view.rank.as_ref().map(|rank| rank.name.clone());
        self.filters = [
            keep(&self.view.giver, self.choices.givers.iter().collect()),
            keep(&self.view.system, self.choices.systems.iter().collect()),
            keep(&held, self.choices.ranks.iter().map(|r| &r.name).collect()),
        ];
        self.take_choices();
        self.rearrange();
    }

    /// The drop-down choices into the view.
    fn take_choices(&mut self) {
        self.view.giver = pick(&self.choices.givers, self.filters[0]);
        self.view.system = pick(&self.choices.systems, self.filters[1]);
        self.view.rank = self.filters[2].checked_sub(1).and_then(|at| self.choices.ranks.get(at)).cloned();
    }

    /// Sorts by `column`; the column sorted already turns its direction.
    pub fn sort_by(&mut self, column: Column) {
        self.view.sort_by(column);
        self.rearrange();
    }

    /// Whether any filter narrows the list.
    pub fn is_narrowed(&self) -> bool {
        let view = &self.view;
        !view.text.trim().is_empty()
            || view.giver.is_some()
            || view.system.is_some()
            || view.rank.is_some()
            || view.with_blueprint
            || view.shareable
    }

    /// Every filter back to all; the order stays.
    pub fn reset(&mut self) {
        self.view = View { sort: self.view.sort, ascending: self.view.ascending, ..View::new() };
        self.filters = [0; 3];
        self.rearrange();
    }
}

/// The choice `at` of a drop-down whose first entry means all: `None` for it.
fn pick(names: &[String], at: usize) -> Option<String> {
    at.checked_sub(1).and_then(|at| names.get(at)).cloned()
}

/// The made-up contracts of the preview.
fn sample_finder() -> Finder {
    let rank = |name: &str, reputation: i64| Some(Rank { name: name.into(), reputation });
    let gain = |who: &str, low: i64, high: i64| Gain { who: who.into(), kind: "Standing".into(), low, high };
    let contracts = vec![
        Contract {
            key: "sample_escort_title".into(),
            title: Some("Escort the Convoy to [LOCATION]".into()),
            giver: Some("Northwind Freight".into()),
            gains: vec![gain("Northwind Freight", 100, 150)],
            cooldown: Some((30, 45)),
            cooldown_spread: Some(10),
            shareable: Some(true),
            region: Some("Stanton (Crusader, Hurston)".into()),
            systems: vec!["Stanton".into()],
            rank: rank("Contractor", 2200),
            rank_max: rank("Elite Contractor", 95250),
            blueprints: Some(vec!["Halcyon Rifle".into(), "Halcyon Rifle Magazine".into()]),
        },
        Contract {
            key: "sample_salvage_title".into(),
            title: Some("Recover the Lost Cargo".into()),
            giver: Some("Deepwell Salvage".into()),
            gains: vec![gain("Deepwell Salvage", 50, 50)],
            cooldown: Some((2880, 2880)),
            shareable: Some(false),
            region: Some("Pyro".into()),
            systems: vec!["Pyro".into()],
            rank: rank("Neutral", 0),
            blueprints: Some(Vec::new()),
            ..Contract::default()
        },
        Contract {
            key: "sample_bounty_title".into(),
            title: Some("Bounty: [TARGET]".into()),
            giver: Some("Northwind Freight".into()),
            gains: vec![gain("Northwind Freight", 300, 300)],
            shareable: Some(true),
            rank: rank("Sr. Contractor", 5800),
            blueprints: Some(vec!["Quarry Helmet".into()]),
            ..Contract::default()
        },
        Contract { key: "sample_unknown_title".into(), ..Contract::default() },
    ];
    Finder { contracts, version: String::new() }
}

/// Minutes as the list shows them: minutes below an hour, hours below a day, days.
pub fn duration(minutes: i64) -> String {
    let n = |value: i64| Arg::Int(value);
    if minutes < 60 {
        tf("s_af_min", &[("n", n(minutes))])
    } else if minutes < 24 * 60 {
        let (hours, rest) = (minutes / 60, minutes % 60);
        if rest == 0 {
            tf("s_af_std", &[("n", n(hours))])
        } else {
            tf("s_af_std_min", &[("n", n(hours)), ("m", n(rest))])
        }
    } else {
        let days = (minutes as f64 / (24.0 * 60.0)).round() as i64;
        tf(if days == 1 { "s_af_tag" } else { "s_af_tage" }, &[("n", n(days))])
    }
}

fn count(value: i64) -> String {
    versekit_core::formats::count(language::current(), value)
}

/// The ranks of a contract as the list shows them: `from Contractor (2,200)`, with the highest rank where the data
/// name one: `from Contractor to Elite Contractor (2,200 to 95,250)`.
pub fn rank_text(rank: &Rank, highest: Option<&Rank>) -> String {
    let rep = Arg::Text(count(rank.reputation));
    let name = Arg::Text(rank.name.clone());
    match highest {
        None => tf("s_af_ab_rang", &[("rank", name), ("rep", rep)]),
        Some(top) => tf(
            "s_af_ab_bis_rang",
            &[
                ("rank", name),
                ("rep", rep),
                ("max", Arg::Text(top.name.clone())),
                ("rep_max", Arg::Text(count(top.reputation))),
            ],
        ),
    }
}

/// A rank as the rank filter offers it: `Contractor (2,200 reputation)`.
fn held_text(rank: &Rank) -> String {
    tf("s_af_rang_wahl", &[("rank", Arg::Text(rank.name.clone())), ("rep", Arg::Text(count(rank.reputation)))])
}

/// A gain with its sign: `Bit Zeros +50 … +200 Standing`.
fn gain_text(gain: &Gain) -> String {
    let signed = |value: i64| if value < 0 { format!("-{}", count(-value)) } else { format!("+{}", count(value)) };
    let amount = if gain.low == gain.high {
        signed(gain.high)
    } else {
        format!("{} … {}", signed(gain.low), signed(gain.high))
    };
    [gain.who.as_str(), amount.as_str(), gain.kind.as_str()]
        .iter()
        .filter(|p| !p.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The values of a contract in the columns after title and giver, a value the data do not state as "unknown".
pub fn facts(contract: &Contract) -> Vec<String> {
    let unknown = || t("s_af_unbekannt");
    let gains = if contract.gains.is_empty() {
        unknown()
    } else {
        contract.gains.iter().map(gain_text).collect::<Vec<_>>().join(", ")
    };
    let cooldown = contract.cooldown.map_or_else(unknown, |(low, high)| {
        let time = if low == high { duration(high) } else { format!("{} – {}", duration(low), duration(high)) };
        match contract.cooldown_spread {
            Some(spread) => format!("{time} (±{})", duration(spread)),
            None => time,
        }
    });
    let shareable = contract.shareable.map_or_else(unknown, |yes| t(if yes { "s_af_ja" } else { "s_af_nein" }));
    let blueprints = match &contract.blueprints {
        None => unknown(),
        Some(names) if names.is_empty() => t("s_af_keine"),
        Some(names) => names.join(", "),
    };
    let rank = contract.rank.as_ref().map_or_else(unknown, |rank| rank_text(rank, contract.rank_max.as_ref()));
    vec![gains, cooldown, shareable, contract.region.clone().unwrap_or_else(unknown), rank, blueprints]
}

fn sub(scheme: &Scheme, text: impl Into<String>) -> RichText {
    RichText::new(text.into()).small().color(color(scheme.sub))
}

/// The filters: the text, the giver, the star system and the rank, the two switches, and a line that says how many
/// contracts the rank filter leaves out. The button that resets them stands in the page head.
fn filters(ui: &mut Ui, scheme: &Scheme, state: &mut ContractFinderState) {
    let width = ui.available_width();
    if widgets::search_field(ui, &mut state.view.text, &t("s_af_suche"), scheme, width).changed() {
        state.rearrange();
    }
    ui.add_space(4.0);
    let givers: Vec<String> =
        std::iter::once(t("s_af_alle_auftraggeber")).chain(state.choices.givers.iter().cloned()).collect();
    let systems: Vec<String> =
        std::iter::once(t("s_af_alle_regionen")).chain(state.choices.systems.iter().cloned()).collect();
    let ranks: Vec<String> =
        std::iter::once(t("s_af_alle_raenge")).chain(state.choices.ranks.iter().map(held_text)).collect();
    fn refs(list: &[String]) -> Vec<&str> {
        list.iter().map(String::as_str).collect()
    }
    let (givers, systems, ranks) = (refs(&givers), refs(&systems), refs(&ranks));
    if widgets::filter_bar(ui, "auftragsfinder", &[&givers, &systems, &ranks], &mut state.filters) {
        state.take_choices();
        state.rearrange();
    }
    ui.add_space(4.0);
    if widgets::switch_row(ui, scheme, &t("s_af_nur_bauplan"), &mut state.view.with_blueprint).changed() {
        state.rearrange();
    }
    if widgets::switch_row(ui, scheme, &t("s_af_nur_teilbar"), &mut state.view.shareable).changed() {
        state.rearrange();
    }
    if state.view.rank.is_some() {
        let hidden = contract_finder::without_rank(&state.finder.contracts, &state.view);
        if hidden > 0 {
            ui.add_space(4.0);
            ui.label(sub(scheme, tf("s_af_ohne_rang", &[("n", Arg::Text(count(hidden as i64)))])));
        }
    }
}

/// The widths of the columns when `usable` points are there for them: each has its least width, and what is left
/// over is shared by the shares of [`COLUMNS`]. Where the least widths do not fit, each has its least width all the
/// same (the line then is wider than the page, which the check of the layout reports).
fn column_widths(usable: f32, least: &[f32]) -> Vec<f32> {
    let spare = (usable - least.iter().sum::<f32>()).max(0.0);
    COLUMNS.iter().zip(least).map(|((_, _, share), least)| least + spare * share).collect()
}

/// Runs `add` once for each column of [`COLUMNS`], each in a cell as wide as its least width and its part of
/// what is left; the line is as high as its highest cell, and the cells of the heads and of every row stand under
/// each other.
fn columns_line(ui: &mut Ui, least: &[f32], mut add: impl FnMut(&mut Ui, usize)) {
    let gap = COLUMN_GAP;
    let whole = ui.available_width();
    let usable = (whole - gap * (COLUMNS.len() - 1) as f32).max(0.0);
    let widths = column_widths(usable, least);
    let origin = ui.cursor().min;
    let clip = ui.clip_rect();
    let mut left = origin.x;
    let mut height = 0.0_f32;
    // Each cell has exactly its width and is clipped to it, so a wide content never pushes the line (and the card
    // around it) wider than the page.
    for (at, width) in widths.iter().copied().enumerate() {
        let place = egui::Rect::from_min_size(egui::pos2(left, origin.y), egui::vec2(width, 10_000.0));
        let mut cell = ui.new_child(
            egui::UiBuilder::new().max_rect(place).layout(egui::Layout::top_down(egui::Align::Min)),
        );
        cell.set_clip_rect(clip.intersect(place));
        add(&mut cell, at);
        height = height.max(cell.min_rect().height());
        left += width + gap;
    }
    ui.allocate_space(egui::vec2(whole, height));
}

/// The heads of the columns, each sorting on a click.
fn sort_heads(ui: &mut Ui, scheme: &Scheme, state: &mut ContractFinderState, least: &[f32]) {
    let mut clicked = None;
    columns_line(ui, least, |cell, at| {
        let (column, key, _) = COLUMNS[at];
        let sorted = (state.view.sort == column).then_some(state.view.ascending);
        if cell.horizontal_wrapped(|head| widgets::sort_head(head, scheme, &t(key), sorted, false)).inner {
            clicked = Some(column);
        }
    });
    if let Some(column) = clicked {
        state.sort_by(column);
    }
}

/// One contract: its title (the text key where no title is known), its giver and its values, each under its head.
fn row(ui: &mut Ui, scheme: &Scheme, contract: &Contract, least: &[f32]) {
    let facts = facts(contract);
    columns_line(ui, least, |cell, at| {
        let text = match at {
            0 => {
                match &contract.title {
                    Some(title) => {
                        cell.add(egui::Label::new(RichText::new(title).strong()).wrap());
                    }
                    None => {
                        // An identifier, not words: it may break anywhere.
                        let font = egui::TextStyle::Body.resolve(cell.style());
                        let format = egui::TextFormat::simple(font, color(scheme.sub));
                        let mut job = egui::text::LayoutJob::single_section(contract.key.clone(), format);
                        job.wrap.break_anywhere = true;
                        cell.add(egui::Label::new(job).wrap());
                    }
                }
                return;
            }
            1 => contract.giver.clone().unwrap_or_else(|| t("s_af_unbekannt")),
            _ => facts.get(at - 2).cloned().unwrap_or_default(),
        };
        cell.add(egui::Label::new(text).wrap());
    });
}

/// The list: how many contracts pass, the heads, the contracts shown and the button for more.
fn list(ui: &mut Ui, scheme: &Scheme, state: &mut ContractFinderState) {
    let total = state.finder.contracts.len();
    let places = state.arranged_places().to_vec();
    let line =
        tf("s_af_zahl", &[("shown", Arg::Text(count(places.len() as i64))), ("total", Arg::Text(count(total as i64)))]);
    ui.label(sub(scheme, line));
    ui.add_space(4.0);
    if places.is_empty() {
        widgets::hint(ui, scheme, &t("s_af_keine_treffer"));
        return;
    }
    let least = state.least_widths(ui, &places);
    widgets::list_frame(ui, scheme, |ui| {
        ui.set_min_width(ui.available_width());
        sort_heads(ui, scheme, state, &least);
        for &at in places.iter().take(state.shown_rows) {
            ui.separator();
            if let Some(contract) = state.finder.contracts.get(at) {
                row(ui, scheme, contract, &least);
            }
        }
    });
    let left = places.len().saturating_sub(state.shown_rows);
    if left > 0 {
        ui.add_space(4.0);
        let word = tf("s_af_mehr", &[("n", Arg::Int(left.min(PAGE_ROWS) as i64))]);
        if ui.add(widgets::Button::new(&word, ButtonKind::Normal, scheme)).clicked() {
            state.shown_rows += PAGE_ROWS;
        }
    }
}

/// A card with `title` over `add`.
fn titled_card(ui: &mut Ui, scheme: &Scheme, title: &str, add: impl FnOnce(&mut Ui)) {
    widgets::card(ui, scheme, |ui| {
        ui.set_min_width(ui.available_width());
        widgets::card_title(ui, scheme, title);
        ui.add_space(4.0);
        add(ui);
    });
}

/// Draws the page "Auftragsfinder".
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut ContractFinderState) {
    state.refresh(ui.input(|i| i.time));
    widgets::hint(ui, scheme, &t("s_af_lead"));
    ui.add_space(6.0);
    if state.finder.contracts.is_empty() {
        widgets::empty_state(ui, scheme, "target", &t("s_af_leer"));
        widgets::hint(ui, scheme, &t("s_af_leer_h"));
        return;
    }
    titled_card(ui, scheme, &t("s_af_filter"), |ui| filters(ui, scheme, state));
    ui.add_space(GAP);
    titled_card(ui, scheme, &t("s_af_liste"), |ui| list(ui, scheme, state));
    if !state.finder.version.is_empty() {
        ui.add_space(4.0);
        ui.label(sub(scheme, tf("s_af_stand", &[("version", Arg::Text(state.finder.version.clone()))])));
    }
}

#[cfg(test)]
mod tests;
