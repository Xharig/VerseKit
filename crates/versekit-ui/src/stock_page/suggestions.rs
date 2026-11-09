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

//! The block "Laut Log neu" on top of the tab Items: the purchases and sales the game log knows that are not
//! decided yet. A purchase can be taken over (booked as an item with its price per piece), a sale taken out of
//! the stock ("Abbuchen"), each one discarded; "all" does that for every one shown, and a message takes the last
//! step back. Nothing is booked without a click.
//! The block folds shut by its head and keeps that; the filters and the search of the list below narrow it too.
//!
//! ⚠ The decisions go to `lagervorschlaege.json` before the stock file is written: a crash in between loses
//! one suggestion, it never books one twice. The read of the journal and of the names happens when a file
//! changed, not with every frame.
//!
//! ⚠ A purchase at a place the list of places does not know adds the place to `orte-eigene.json` before the
//! stock is written; the undo takes it out again when no entry of the stock uses it any more.

use versekit_core::places;
use versekit_core::stats_report as report;
use versekit_core::stock_journal::names::{Items, PlaceName, Places};
use versekit_core::stock_journal::suggestions::{self, Decision, Decisions, OwnPlace, Proposal, Proposals};
use versekit_core::stock_journal::{self, Proof, Session};

use super::*;

/// How many suggestions show before "show all".
const ROWS_SHOWN: usize = 5;

/// What the block keeps.
pub(super) struct Suggestions {
    paths: Option<Paths>,
    decisions: Decisions,
    sessions: Vec<Session>,
    items: Items,
    places: Places,
    found: Proposals,
    seen: [Option<SystemTime>; 2],
    looked_at: f64,
    all: bool,
    /// The block is folded open (`lager_vorschlaege_offen`).
    pub(super) open: bool,
}

impl Default for Suggestions {
    fn default() -> Suggestions {
        Suggestions {
            paths: None,
            decisions: Decisions::default(),
            sessions: Vec::new(),
            items: Items::default(),
            places: Places::default(),
            found: Proposals::default(),
            seen: [None, None],
            looked_at: f64::NEG_INFINITY,
            all: false,
            open: true,
        }
    }
}

/// What "Rückgängig" takes back after a suggestion was taken over or discarded.
#[derive(Clone, Debug)]
pub(super) struct SuggestionUndo {
    /// The trade file before and after a take-over; none for a discard.
    stock: Option<(Vec<Value>, Vec<Value>)>,
    keys: Vec<String>,
    /// The places the take-over added to the list.
    places: Vec<String>,
}

/// What a click in the block asks for.
enum Ask {
    Adopt(Vec<String>),
    Discard(Vec<String>),
    ShowAll,
    ShowFewer,
    Fold,
}

fn place_text(place: &PlaceName) -> String {
    match place {
        PlaceName::Listed(name) => name.clone(),
        PlaceName::Shown(name) => tf("s_lj_ort_fremd", &[("0", Arg::from(name.as_str()))]),
        PlaceName::Unknown(number) if number.is_empty() => t("s_lj_ort_offen"),
        PlaceName::Unknown(number) => tf("s_lj_ort_nr", &[("0", Arg::from(number.as_str()))]),
    }
}

/// Why a suggestion cannot be taken over into or out of this stock; `None` when it can.
fn blocked_reason(proposal: &Proposal, trade: &[Value]) -> Option<String> {
    if proposal.stock_name.is_none() {
        Some(t(match (proposal.goods, proposal.sold) {
            (true, _) => "s_lj_grund_ware_verkauf",
            (false, true) => "s_lj_grund_item_verkauf",
            (false, false) => "s_lj_grund_item",
        }))
    } else if proposal.quantity == 0 {
        Some(t("s_lj_grund_menge"))
    } else if !proposal.bookable(trade) {
        let name = Arg::from(proposal.name.as_str());
        Some(match proposal.place_to_book() {
            "" => tf("s_lj_grund_lager_ohne_ort", &[("name", name)]),
            place => tf("s_lj_grund_lager", &[("name", name), ("ort", Arg::from(place))]),
        })
    } else {
        None
    }
}

/// What "Abbuchen" of a sale does: how many pieces (SCU for goods) it takes, whether some come from stacks without a
/// place, and whether the stock holds fewer than were sold; a sale of goods the stock holds none of is only marked.
/// A sale at a commodity terminal also says what the log shows of it.
fn take_hint(proposal: &Proposal, trade: &[Value]) -> String {
    let lang = language::current();
    let plan = suggestions::take_plan(trade, proposal);
    let pieces = |n: f64| Arg::from(amount_with_unit(lang, n, !proposal.goods));
    let mut lines = Vec::new();
    if proposal.goods && plan.pieces <= 0.0 {
        lines.push(t("s_lj_tipp_nur_vermerken"));
    } else {
        lines.push(tf("s_lj_tipp_abbuchen", &[("n", pieces(plan.pieces))]));
        if plan.without_place {
            lines.push(t("s_lj_tipp_abbuchen_ohne_ort"));
        }
        if plan.pieces + 1e-9 < f64::from(proposal.quantity) {
            lines.push(tf("s_lj_tipp_abbuchen_rest", &[("n", pieces(f64::from(proposal.quantity)))]));
        }
    }
    if proposal.goods {
        lines.push(t(if proposal.proof.confirmed() { "s_lj_tipp_bestaetigt" } else { "s_lj_tipp_angefragt" }));
    }
    lines.join("\n")
}

/// The word for what the log shows of a sale at a commodity terminal.
fn proof_word(proof: Proof) -> String {
    t(if proof.confirmed() { "s_lj_bestaetigt" } else { "s_lj_angefragt" })
}

/// What the button "Übernehmen" or "Abbuchen" of a suggestion says on hover: why it is blocked, or what it does
/// besides booking.
fn adopt_hint(proposal: &Proposal, trade: &[Value]) -> Option<String> {
    if let Some(reason) = blocked_reason(proposal, trade) {
        return Some(reason);
    }
    if proposal.sold {
        return Some(take_hint(proposal, trade));
    }
    if let Some(place) = proposal.new_place() {
        return Some(tf("s_lj_tipp_neuer_ort", &[("0", Arg::from(place.name.as_str()))]));
    }
    proposal.without_place().then(|| t("s_lj_tipp_ohne_ort"))
}

/// The names of places, joined for a sentence.
fn joined(places: &[OwnPlace]) -> String {
    places.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")
}

/// What "Alle buchen" does besides booking: the places it adds, the purchases it books without a place and
/// the sales it takes out of the stock.
fn all_hint(shown: &[Proposal], trade: &[Value]) -> Vec<String> {
    let new = suggestions::new_places(shown);
    let mut lines = Vec::new();
    match new.len() {
        0 => {}
        1 => lines.push(tf("s_lj_alle_neue_orte_eins", &[("orte", Arg::from(joined(&new)))])),
        n => lines.push(tf("s_lj_alle_neue_orte_n", &[("n", Arg::from(n)), ("orte", Arg::from(joined(&new)))])),
    }
    match shown.iter().filter(|p| p.adoptable() && p.without_place()).count() {
        0 => {}
        1 => lines.push(t("s_lj_alle_ohne_ort_eins")),
        n => lines.push(tf("s_lj_alle_ohne_ort_n", &[("n", Arg::from(n))])),
    }
    match suggestions::batch(trade, shown).iter().filter(|p| p.sold).count() {
        0 => {}
        1 => lines.push(t("s_lj_alle_verkaeufe_eins")),
        n => lines.push(tf("s_lj_alle_verkaeufe_n", &[("n", Arg::from(n))])),
    }
    lines
}

/// The small line under the name: amount, price per piece (per SCU for goods), place and moment; a sale says so in
/// front, a sale at a commodity terminal also whether the log shows it only as a request.
fn detail_line(lang: Lang, proposal: &Proposal) -> String {
    let key = match (proposal.goods, proposal.sold) {
        (true, _) => "s_lj_zeile_verkauf_ware",
        (false, true) => "s_lj_zeile_verkauf",
        (false, false) => "s_lj_zeile",
    };
    tf(
        key,
        &[
            ("menge", Arg::from(amount_with_unit(lang, f64::from(proposal.quantity), !proposal.goods))),
            ("preis", Arg::from(formats::money(lang, proposal.unit_price))),
            ("ort", Arg::from(place_text(&proposal.place))),
            ("zeit", Arg::from(report::local_moment(lang, proposal.time))),
            ("nachweis", Arg::from(proof_word(proposal.proof))),
        ],
    )
}

/// The message after a take-over: what was booked in, what was taken out (`taken` pieces or SCU of sales) and the
/// places that were added.
fn adopted_text(done: &[&Proposal], added: &[String], taken: f64) -> String {
    let (sales, purchases): (Vec<&Proposal>, Vec<&Proposal>) = done.iter().copied().partition(|p| p.sold);
    if purchases.is_empty() {
        return match sales.as_slice() {
            [one] if one.goods && taken <= 0.0 => tf("s_lj_vermerkt_eins", &[("name", Arg::from(one.name.as_str()))]),
            [one] => tf(
                "s_lj_abgebucht_eins",
                &[
                    ("name", Arg::from(one.name.as_str())),
                    ("menge", Arg::from(amount_with_unit(language::current(), taken, !one.goods))),
                ],
            ),
            many => tf("s_lj_abgebucht_n", &[("n", Arg::from(many.len()))]),
        };
    }
    if !sales.is_empty() {
        let (k, v) = (Arg::from(purchases.len()), Arg::from(sales.len()));
        return match added {
            [] => tf("s_lj_gebucht_gemischt", &[("k", k), ("v", v)]),
            [place] => tf("s_lj_gebucht_gemischt_ort", &[("k", k), ("v", v), ("ort", Arg::from(place.as_str()))]),
            places => tf("s_lj_gebucht_gemischt_orte", &[("k", k), ("v", v), ("o", Arg::from(places.len()))]),
        };
    }
    match (purchases.as_slice(), added) {
        ([one], []) => tf("s_lj_uebernommen_eins", &[("name", Arg::from(one.name.as_str()))]),
        ([one], [place]) => tf(
            "s_lj_uebernommen_eins_neu",
            &[("name", Arg::from(one.name.as_str())), ("ort", Arg::from(place.as_str()))],
        ),
        (many, []) => tf("s_lj_uebernommen_n", &[("n", Arg::from(many.len()))]),
        (many, [place]) => {
            tf("s_lj_uebernommen_n_neu_eins", &[("n", Arg::from(many.len())), ("ort", Arg::from(place.as_str()))])
        }
        (many, places) => tf("s_lj_uebernommen_n_neu", &[("n", Arg::from(many.len())), ("k", Arg::from(places.len()))]),
    }
}

fn journal_file(paths: &Paths) -> String {
    paths.app_file(stock_journal::FILE)
}

fn decisions_file(paths: &Paths) -> String {
    paths.app_file(suggestions::FILE)
}

impl StockState {
    /// Lets the block read the journal of these paths (a copy of them is kept).
    pub(super) fn connect_journal(&mut self, paths: &Paths) {
        self.sugg.paths = Some(paths.working_copy());
    }

    /// The suggestions open now (tests).
    #[cfg(test)]
    pub(crate) fn open_suggestions(&self) -> &[Proposal] {
        &self.sugg.found.list
    }

    /// Reads what changed on disk: the journal (with the names) and the decisions. Called while the tab is
    /// shown, at most once a second; `force` reads at once.
    pub(super) fn look_at_suggestions(&mut self, now: f64, force: bool) {
        if !force && now - self.sugg.looked_at < LOOK_SECONDS {
            return;
        }
        self.sugg.looked_at = now;
        let Some(paths) = &self.sugg.paths else { return };
        let seen = [modified(&journal_file(paths)), modified(&decisions_file(paths))];
        if !force && seen == self.sugg.seen {
            return;
        }
        let journal_changed = force || seen[0] != self.sugg.seen[0];
        self.sugg.seen = seen;
        self.sugg.decisions = Decisions::load(paths);
        if journal_changed {
            self.sugg.sessions = stock_journal::sessions(&stock_journal::load(paths));
            self.sugg.items = Items::load(paths);
            self.sugg.places = Places::load(paths);
        }
        self.recompute_suggestions();
    }

    fn recompute_suggestions(&mut self) {
        let s = &mut self.sugg;
        s.found = suggestions::proposals(&s.sessions, &s.decisions, &s.items, &s.places, &self.data);
    }

    /// The open suggestions of the tab shown: goods on the tab of the materials and goods, items on the tab Items.
    fn suggestions_of_tab(&self) -> impl Iterator<Item = &Proposal> {
        self.sugg.found.list.iter().filter(|p| p.goods != self.items)
    }

    /// Is the block there at all: the switch is on and something of this tab is open.
    fn suggestions_shown(&self) -> bool {
        self.sugg.paths.as_ref().is_some_and(stock_journal::enabled) && self.suggestions_of_tab().next().is_some()
    }

    /// The kind a suggestion has in the filter of the list, as its row would have it.
    fn suggestion_kind(&self, proposal: &Proposal) -> Kind {
        if proposal.goods { self.data.material_kind(&proposal.name, true) } else { self.data.item_kind(&proposal.name) }
    }

    /// The open suggestions of the tab the filters and the search of the list let through, the same way they narrow
    /// the list.
    fn filtered_suggestions(&self) -> Vec<Proposal> {
        let search = lower(self.search.trim());
        self.suggestions_of_tab()
            .filter(|p| self.kind.is_none_or(|k| self.suggestion_kind(p) == k))
            .filter(|p| self.place.is_empty() || lower(p.place_to_book()) == lower(&self.place))
            .filter(|p| {
                search.is_empty() || lower(&p.name).contains(&search) || lower(p.place_to_book()).contains(&search)
            })
            .cloned()
            .collect()
    }

    /// Is a place used by an entry of the stock?
    fn place_in_use(&self, place: &str) -> bool {
        let wanted = lower(place.trim());
        self.raw
            .iter()
            .chain(self.trade.iter())
            .any(|e| e.get("ort").and_then(Value::as_str).is_some_and(|o| lower(o.trim()) == wanted))
    }

    /// Takes added places out of the list again (file, list of the forms, names of the block).
    fn forget_places(&mut self, paths: &Paths, names: &[String]) -> bool {
        if names.is_empty() {
            return true;
        }
        let done = places::remove_own(paths, names);
        if done {
            for name in names {
                self.data.remove_place(name);
            }
        }
        self.sugg.places = Places::load(paths);
        done
    }

    /// Takes the suggestions with these keys into the stock as items, or a sale out of it (`Übernehmen`,
    /// `Abbuchen`, `Alle buchen`); a place the list does not know yet is added to it.
    fn adopt_suggestions(&mut self, keys: &[String]) {
        let Some(paths) = self.sugg.paths.as_ref().map(Paths::working_copy) else { return };
        if !self.fresh() {
            return;
        }
        // Oldest first: a piece bought and then sold is booked in before it is taken out.
        let mut chosen: Vec<Proposal> =
            self.sugg.found.list.iter().filter(|p| keys.contains(&p.key) && p.adoptable()).cloned().collect();
        chosen.sort_by(|a, b| a.time.cmp(&b.time).then_with(|| a.key.cmp(&b.key)));
        let before = self.trade.clone();
        let mut done: Vec<&Proposal> = Vec::new();
        let mut taken = 0.0;
        for proposal in &chosen {
            let pieces = suggestions::take_plan(&self.trade, proposal).pieces;
            if suggestions::adopt(&mut self.trade, proposal).is_ok() {
                done.push(proposal);
                taken += pieces;
            }
        }
        if done.is_empty() {
            self.trade = before;
            self.say(t("s_lj_nicht_gebucht"));
            return;
        }
        let booked: Vec<String> = done.iter().map(|p| p.key.clone()).collect();
        let new = suggestions::new_places(done.iter().copied());
        // First the decision, then the places, then the stock: a failure in between leaves a suggestion lost,
        // not booked twice.
        self.sugg.decisions.set(&booked, Decision::Adopted);
        if !self.sugg.decisions.save(&paths) {
            self.sugg.decisions.forget(&booked);
            self.trade = before;
            self.say(t("s_hl_fehler"));
            return;
        }
        let Some(added) = places::add_own(&paths, &new) else {
            self.sugg.decisions.forget(&booked);
            self.sugg.decisions.save(&paths);
            self.trade = before;
            self.say(t("s_hl_fehler"));
            return;
        };
        let added: Vec<String> = added.into_iter().map(|p| p.name).collect();
        if !self.write(Store::Trade) {
            self.sugg.decisions.forget(&booked);
            self.sugg.decisions.save(&paths);
            self.forget_places(&paths, &added);
            self.recompute_suggestions();
            return;
        }
        for name in &added {
            self.data.add_place(name);
        }
        if !added.is_empty() {
            self.sugg.places = Places::load(&paths);
        }
        self.sugg.seen[1] = modified(&decisions_file(&paths));
        let text = adopted_text(&done, &added, taken);
        let undo = SuggestionUndo { stock: Some((before, self.trade.clone())), keys: booked, places: added };
        self.message = Some(Message { text, since: None, undo: Some(Undo::Suggestions(undo)) });
        self.recompute_suggestions();
    }

    /// Throws the suggestions with these keys away (`Verwerfen`, `Alle verwerfen`).
    fn discard_suggestions(&mut self, keys: &[String]) {
        let Some(paths) = self.sugg.paths.as_ref().map(Paths::working_copy) else { return };
        let chosen: Vec<&Proposal> = self.sugg.found.list.iter().filter(|p| keys.contains(&p.key)).collect();
        if chosen.is_empty() {
            return;
        }
        let thrown: Vec<String> = chosen.iter().map(|p| p.key.clone()).collect();
        let text = match chosen.as_slice() {
            [one] => tf("s_lj_verworfen_eins", &[("name", Arg::from(one.name.as_str()))]),
            many => tf("s_lj_verworfen_n", &[("n", Arg::from(many.len()))]),
        };
        self.sugg.decisions.set(&thrown, Decision::Discarded);
        if !self.sugg.decisions.save(&paths) {
            self.sugg.decisions.forget(&thrown);
            self.say(t("s_hl_fehler"));
            return;
        }
        self.sugg.seen[1] = modified(&decisions_file(&paths));
        let undo = SuggestionUndo { stock: None, keys: thrown, places: Vec::new() };
        self.message = Some(Message { text, since: None, undo: Some(Undo::Suggestions(undo)) });
        self.recompute_suggestions();
    }

    /// Takes the last take-over or discard back: the stock as it was (when nothing else changed it since), the
    /// places it added out of the list when no entry uses them, and the purchases open again.
    pub(super) fn undo_suggestions(&mut self, undo: SuggestionUndo) {
        let Some(paths) = self.sugg.paths.as_ref().map(Paths::working_copy) else { return };
        self.reload();
        if let Some((before, after)) = undo.stock {
            if self.trade != after {
                self.say(t("s_lj_zurueck_veraendert"));
                return;
            }
            self.stop_editing();
            self.trade = before;
            if !self.write(Store::Trade) {
                return;
            }
        }
        let unused: Vec<String> = undo.places.iter().filter(|p| !self.place_in_use(p)).cloned().collect();
        if !self.forget_places(&paths, &unused) {
            self.say(t("s_hl_fehler"));
            return;
        }
        self.sugg.decisions.forget(&undo.keys);
        if !self.sugg.decisions.save(&paths) {
            self.say(t("s_hl_fehler"));
            return;
        }
        self.sugg.seen[1] = modified(&decisions_file(&paths));
        self.recompute_suggestions();
        self.say(if unused.is_empty() { t("s_lj_zurueck") } else { t("s_lj_zurueck_orte") });
    }

    /// Applies what a click in the block asked for.
    fn answer(&mut self, ask: Ask) {
        match ask {
            Ask::Adopt(keys) => self.adopt_suggestions(&keys),
            Ask::Discard(keys) => self.discard_suggestions(&keys),
            Ask::ShowAll => self.sugg.all = true,
            Ask::ShowFewer => self.sugg.all = false,
            Ask::Fold => self.sugg.open = !self.sugg.open,
        }
    }
}

/// The icon of "Alle buchen" for what it books: purchases only adds, sales only takes out, both checks off.
fn book_all_icon(batch: &[&Proposal]) -> &'static str {
    match (batch.iter().any(|p| !p.sold), batch.iter().any(|p| p.sold)) {
        (true, true) => "check-check",
        (false, true) => "package-minus",
        _ => "package-plus",
    }
}

/// The rows, the buttons for all and the hints of an open block.
fn block_body(ui: &mut Ui, scheme: &Scheme, state: &StockState, list: &[Proposal], total: usize) -> Option<Ask> {
    let lang = language::current();
    let mut ask = None;
    ui.add(egui::Label::new(RichText::new(t("s_lj_hilfe")).small().color(color(scheme.sub))).wrap());
    if list.len() < total {
        let line = tf("s_lj_gefiltert", &[("m", Arg::from(list.len())), ("n", Arg::from(total))]);
        ui.add(egui::Label::new(RichText::new(line).small().color(color(scheme.gold))).wrap());
    }
    ui.add_space(6.0);
    let trade = &state.trade;
    let batch = suggestions::batch(trade, list);
    let adoptable: Vec<String> = batch.iter().map(|p| p.key.clone()).collect();
    let all_icon = book_all_icon(&batch);
    let every: Vec<String> = list.iter().map(|p| p.key.clone()).collect();
    // "Übernehmen" and "Abbuchen" in one width, so the main button of every row starts at the same edge.
    let take_width = ["s_lj_uebernehmen", "s_lj_abbuchen"]
        .map(|key| widgets::icon_button_size(ui, scheme, ButtonKind::Strong, &t(key)))
        .into_iter()
        .fold(egui::Vec2::ZERO, |widest, size| widest.max(size));
    let shown = if state.sugg.all { list.len() } else { ROWS_SHOWN.min(list.len()) };
    for proposal in list.iter().take(shown) {
        ui.push_id(&proposal.key, |ui| {
            let reason = blocked_reason(proposal, trade);
            let hint = adopt_hint(proposal, trade);
            let (take_key, take_icon) =
                if proposal.sold { ("s_lj_abbuchen", "package-minus") } else { ("s_lj_uebernehmen", "package-plus") };
            let (take_word, drop_word) = (t(take_key), t("s_lj_verwerfen"));
            let (take_name, drop_name) =
                (format!("{take_word}: {}", proposal.name), format!("{drop_word}: {}", proposal.name));
            ui.horizontal(|line| {
                line.with_layout(Layout::right_to_left(Align::Center), |row| {
                    let discard =
                        widgets::Button::new(&drop_word, ButtonKind::Normal, scheme).icon("x").spoken(&drop_name);
                    if row.add(discard).clicked() {
                        ask = Some(Ask::Discard(vec![proposal.key.clone()]));
                    }
                    let adopt = widgets::Button::new(&take_word, ButtonKind::Strong, scheme)
                        .icon(take_icon)
                        .spoken(&take_name)
                        .blocked(reason.as_deref())
                        .wide();
                    let (cell, _) = row.allocate_exact_size(take_width, Sense::hover());
                    let mut spot = row.new_child(
                        UiBuilder::new()
                            .max_rect(cell)
                            .layout(Layout::centered_and_justified(egui::Direction::LeftToRight)),
                    );
                    let mut response = spot.add(adopt);
                    if reason.is_none()
                        && let Some(hint) = &hint
                    {
                        response = response.on_hover_text(hint);
                    }
                    if response.clicked() {
                        ask = Some(Ask::Adopt(vec![proposal.key.clone()]));
                    }
                    row.with_layout(Layout::left_to_right(Align::Center), |name| {
                        name.add(egui::Label::new(RichText::new(&proposal.name).strong()).truncate());
                    });
                });
            });
            ui.add(
                egui::Label::new(RichText::new(detail_line(lang, proposal)).small().color(color(scheme.sub))).wrap(),
            );
            ui.add_space(6.0);
        });
    }
    let hints = all_hint(list, trade);
    ui.horizontal_wrapped(|row| {
        if !adoptable.is_empty() {
            let word = tf("s_lj_alle_buchen", &[("n", Arg::from(adoptable.len()))]);
            let mut response = row.add(widgets::Button::new(&word, ButtonKind::Strong, scheme).icon(all_icon));
            if !hints.is_empty() {
                response = response.on_hover_text(hints.join("\n"));
            }
            if response.clicked() {
                ask = Some(Ask::Adopt(adoptable.clone()));
            }
        }
        if !every.is_empty() {
            let word = tf("s_lj_alle_verwerfen", &[("n", Arg::from(every.len()))]);
            if row.add(widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("x")).clicked() {
                ask = Some(Ask::Discard(every.clone()));
            }
        }
        if list.len() > ROWS_SHOWN {
            let (word, next) = if state.sugg.all {
                (t("s_lj_weniger"), Ask::ShowFewer)
            } else {
                (tf("s_lj_alle_zeigen", &[("n", Arg::from(list.len()))]), Ask::ShowAll)
            };
            if row.add(widgets::Button::new(&word, ButtonKind::Normal, scheme)).clicked() {
                ask = Some(next);
            }
        }
    });
    if !adoptable.is_empty() {
        for line in &hints {
            ui.add_space(2.0);
            ui.add(egui::Label::new(RichText::new(line).small().color(color(scheme.sub))).wrap());
        }
    }
    let unnamed = state.sugg.found.unnamed;
    if unnamed > 0 {
        ui.add_space(4.0);
        let line = tf("s_lj_ohne_namen", &[("n", Arg::from(unnamed))]);
        ui.add(egui::Label::new(RichText::new(line).small().color(color(scheme.sub))).wrap());
    }
    ask
}

/// The block on top of the list of either tab (items there, goods sold at a terminal on the other), folded open or
/// shut by its head.
pub(super) fn show_block(ui: &mut Ui, scheme: &Scheme, state: &mut StockState) {
    if !state.suggestions_shown() {
        return;
    }
    let total = state.suggestions_of_tab().count();
    let list = state.filtered_suggestions();
    let title = if list.len() < total {
        tf("s_lj_titel_gefiltert", &[("m", Arg::from(list.len())), ("n", Arg::from(total))])
    } else {
        tf("s_lj_titel_n", &[("n", Arg::from(total))])
    };
    let ask = widgets::card(ui, scheme, |ui| {
        let shown = widgets::collapsing(&title)
            .id_salt("laut-log-neu")
            .open(Some(state.sugg.open))
            .show(ui, |ui| block_body(ui, scheme, state, &list, total));
        if shown.header_response.clicked() { Some(Ask::Fold) } else { shown.body_returned.flatten() }
    })
    .inner;
    ui.add_space(GAP);
    if let Some(ask) = ask {
        state.answer(ask);
    }
}

#[cfg(test)]
mod tests {
    use egui::{Event, Pos2, Rect};
    use versekit_core::jsonio::Map;
    use versekit_core::language::Choice;
    use versekit_core::legacy;
    use versekit_core::paths::Host;
    use versekit_core::pypath::Flavor;
    use versekit_core::stock_journal::Purchase;
    use versekit_core::theme;

    use super::*;

    type Texts = Vec<(String, Pos2)>;
    type Nodes = Vec<(egui::Role, String, Rect)>;

    /// A button as a screen reader knows it.
    #[derive(Clone, Debug)]
    struct Node {
        name: String,
        description: String,
        disabled: bool,
    }

    const NAMES: &str = r#"{"item_Namemag": "Venture Arms", "item_Namemult": "FBL-8a Arms Arctic Digital", "item_Nameodd": "Odd Thing"}"#;

    fn purchase(time: i64, class: &str, quantity: u32, price_total: f64, place: Option<&str>) -> Purchase {
        Purchase {
            time,
            class: class.into(),
            guid: format!("guid-{class}"),
            quantity,
            price_total,
            place: place.map(str::to_string),
            shop: "SCShop_Kiosk".into(),
            proof: versekit_core::stock_journal::Proof::Answer,
        }
    }

    /// Two purchases of `Venture Arms` at Levski (4 pieces for 1660), one `FBL-8a Arms Arctic Digital` at Levski,
    /// one item the stock list does not know (`Odd Thing`), one `Venture Arms` at a place outside the list, and one
    /// class nobody names.
    fn standard() -> Vec<Purchase> {
        vec![
            purchase(1050, "mag", 1, 100.0, Some("1")),
            purchase(1100, "mag", 3, 1560.0, Some("1")),
            purchase(1200, "mult", 1, 800.0, Some("1")),
            purchase(1300, "odd", 1, 5.0, Some("1")),
            purchase(1400, "mag", 1, 50.0, Some("2")),
            purchase(1500, "blank", 1, 9.0, Some("1")),
        ]
    }

    fn write_journal(paths: &Paths, purchases: &[Purchase]) {
        write_deals(paths, purchases, &[]);
    }

    fn write_deals(paths: &Paths, purchases: &[Purchase], sales: &[Purchase]) {
        let bought: Vec<Value> = purchases.iter().map(Purchase::to_value).collect();
        let sold: Vec<Value> = sales.iter().map(Purchase::to_value).collect();
        let session = Value::Object(vec![
            ("von".into(), Value::int(1000)),
            ("bis".into(), Value::int(2000)),
            ("kaeufe".into(), Value::Array(bought)),
            ("verkaeufe".into(), Value::Array(sold)),
            ("bewegungen".into(), Value::Array(Vec::new())),
        ]);
        let mut data: Map = vec![
            ("sitzungen".into(), Value::Object(vec![("1000".into(), session)])),
            ("gelesen".into(), Value::Object(Vec::new())),
        ];
        assert!(stock_journal::save(paths, &mut data));
    }

    fn world(tag: &str, purchases: &[Purchase]) -> (String, Paths) {
        let home = std::env::temp_dir()
            .join(format!("versekit-lagervorschlag-{tag}-{}", versekit_core::testdir::run_tag()))
            .to_string_lossy()
            .replace('\\', "/");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("folder");
        let paths = Paths::new(Host::new(Flavor::native(), &[(legacy::ENV_HOME, home.as_str())], &home));
        let write = |name: &str, text: &str| std::fs::write(paths.app_file(name), text).expect("file");
        write("orte.json", r#"{"format": 1, "orte": ["Levski", "Area 18"]}"#);
        write("orte-gelernt.json", r#"{"orte": {"1": "levski", "2": "outpost"}, "gelesen": {}}"#);
        write("namen-original.json", NAMES);
        write("rohstoffe.json", r#"{"format": 1, "posten": []}"#);
        write(
            "handelslager.json",
            r#"{"format": 1, "posten": [{"ware": "TS-2", "menge": 2.0, "ort": "Levski", "gestohlen": false, "stueck": true}]}"#,
        );
        write_journal(&paths, purchases);
        (home, paths)
    }

    fn setup() -> std::sync::MutexGuard<'static, ()> {
        let guard = crate::language_lock();
        theme::set_current("original");
        language::set_thread_language(Choice::De);
        guard
    }

    fn trade_on_disk(paths: &Paths) -> Vec<Value> {
        stock::load(&paths.app_file(stock::TRADE_FILE), "test")
    }

    struct Bench {
        ctx: egui::Context,
        state: StockState,
    }

    impl Bench {
        fn new(paths: &Paths) -> Bench {
            let ctx = egui::Context::default();
            crate::install(&ctx);
            ctx.enable_accesskit();
            let mut state = StockState::sample_on(paths);
            state.items = true;
            Bench { ctx, state }
        }

        /// The program is closed and opened again: everything the page held is gone, the files stay.
        fn restart(&mut self, paths: &Paths) {
            self.state = StockState::sample_on(paths);
            self.state.items = true;
        }

        fn frame(&mut self, events: Vec<Event>) -> (Texts, Nodes) {
            let scheme = theme::by_id("original").expect("original");
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 2400.0))),
                events,
                ..Default::default()
            };
            let state = &mut self.state;
            let mut output = self.ctx.run_ui(input, |ui| show_page(ui, scheme, state));
            output.textures_delta.clear();
            let mut texts = Vec::new();
            fn collect(shape: &egui::Shape, into: &mut Texts) {
                match shape {
                    egui::Shape::Text(text) => into.push((text.galley.text().to_string(), text.pos)),
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, into)),
                    _ => {}
                }
            }
            output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
            let nodes = output
                .platform_output
                .accesskit_update
                .map(|update| {
                    update
                        .nodes
                        .iter()
                        .filter_map(|(_, node)| {
                            let b = node.bounds()?;
                            let rect = Rect::from_min_max(
                                Pos2::new(b.x0 as f32, b.y0 as f32),
                                Pos2::new(b.x1 as f32, b.y1 as f32),
                            );
                            Some((node.role(), node.label().unwrap_or_default().to_string(), rect))
                        })
                        .collect()
                })
                .unwrap_or_default();
            (texts, nodes)
        }

        fn settle(&mut self) -> Texts {
            let mut last = Vec::new();
            for _ in 0..4 {
                last = self.frame(Vec::new()).0;
            }
            last
        }

        fn press_at(&mut self, at: Pos2) {
            let button = |pressed| Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            self.frame(vec![Event::PointerMoved(at)]);
            self.frame(vec![button(true)]);
            self.frame(vec![button(false)]);
            self.settle();
        }

        /// Every button of the page with its name, its hover text (the description) and whether it is blocked.
        fn buttons(&mut self) -> Vec<Node> {
            self.settle();
            let scheme = theme::by_id("original").expect("original");
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 2400.0))),
                ..Default::default()
            };
            let state = &mut self.state;
            let mut output = self.ctx.run_ui(input, |ui| show_page(ui, scheme, state));
            output.textures_delta.clear();
            let Some(update) = output.platform_output.accesskit_update else { return Vec::new() };
            update
                .nodes
                .iter()
                .filter(|(_, node)| node.role() == egui::Role::Button)
                .map(|(_, node)| Node {
                    name: node.label().unwrap_or_default().to_string(),
                    description: node.description().unwrap_or_default().to_string(),
                    disabled: node.is_disabled(),
                })
                .collect()
        }

        /// The button whose name is `name` exactly.
        fn button(&mut self, name: &str) -> Node {
            let all = self.buttons();
            all.iter().find(|b| b.name == name).cloned().unwrap_or_else(|| panic!("no button `{name}`: {all:?}"))
        }

        /// The button named `name` whose hover text contains `part`.
        fn button_at(&mut self, name: &str, part: &str) -> Node {
            let all = self.buttons();
            all.iter()
                .find(|b| b.name == name && b.description.contains(part))
                .cloned()
                .unwrap_or_else(|| panic!("no button `{name}` with `{part}`: {all:?}"))
        }

        /// Presses the button whose name (the spoken one) contains `part`, without regard to case.
        fn press_button(&mut self, part: &str) {
            self.settle();
            let nodes = self.frame(Vec::new()).1;
            let wanted = part.to_lowercase();
            let rect = nodes
                .iter()
                .find(|(role, name, _)| *role == egui::Role::Button && name.to_lowercase().contains(&wanted))
                .map(|(_, _, rect)| *rect)
                .unwrap_or_else(|| panic!("no button `{part}`: {nodes:?}"));
            self.press_at(rect.center());
        }
    }

    fn shows(texts: &Texts, part: &str) -> bool {
        texts.iter().any(|(text, _)| text.contains(part))
    }

    fn at(texts: &Texts, part: &str) -> Pos2 {
        texts
            .iter()
            .find(|(text, _)| text.contains(part))
            .map(|(_, at)| *at)
            .unwrap_or_else(|| panic!("{part}: {texts:?}"))
    }

    fn title(n: usize) -> String {
        tf("s_lj_titel_n", &[("n", Arg::from(n))])
    }

    fn open_names(bench: &Bench) -> Vec<String> {
        bench.state.open_suggestions().iter().map(|p| format!("{}:{}", p.name, p.quantity)).collect()
    }

    #[test]
    fn the_block_stands_on_top_of_the_items_tab_with_what_the_log_knows() {
        let _language = setup();
        let (home, paths) = world("block", &standard());
        let mut bench = Bench::new(&paths);
        let texts = bench.settle();
        assert!(shows(&texts, &title(5)), "{texts:?}");
        assert_eq!(
            open_names(&bench),
            ["Venture Arms:1", "Odd Thing:1", "FBL-8a Arms Arctic Digital:1", "Venture Arms:3", "Venture Arms:1"],
            "the newest first, the class nobody names left out"
        );
        // Amount, price per piece (1560 for 3), the place by the log and the moment.
        assert!(shows(&texts, "3 Stück · 520 aUEC je Stück · laut Log an Levski · "), "{texts:?}");
        assert!(shows(&texts, "laut Log an outpost (noch nicht in deiner Ortsliste)"), "{texts:?}");
        assert!(shows(&texts, &tf("s_lj_ohne_namen", &[("n", Arg::from(1))])), "{texts:?}");
        // Under the tabs and above the list of the stock; the filters and the search between block and list.
        let (tabs, block, list) = (at(&texts, "Items ("), at(&texts, &title(5)), at(&texts, "TS-2"));
        assert!(tabs.y < block.y && block.y < list.y, "tabs {tabs:?} block {block:?} list {list:?}");
        let (kinds, search) = (at(&texts, &t("s_ml_alle_arten")), at(&texts, &t("s_mlg_suche")));
        let last_row =
            texts.iter().filter(|(text, _)| text.contains("laut Log an")).map(|(_, p)| p.y).fold(0.0, f32::max);
        assert!(last_row < kinds.y && kinds.y < search.y && search.y < list.y, "{texts:?}");
        // Not on the tab of the materials.
        bench.state.items = false;
        let texts = bench.settle();
        assert!(!shows(&texts, &title(5)), "the block belongs to the items");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_block_is_gone_while_the_switch_is_off_or_nothing_was_bought() {
        let _language = setup();
        let (home, paths) = world("aus", &standard());
        assert!(stock_journal::set_enabled(&paths, false));
        let mut bench = Bench::new(&paths);
        assert!(!shows(&bench.settle(), "Laut Log neu"));
        assert!(stock_journal::set_enabled(&paths, true));
        bench.restart(&paths);
        assert!(shows(&bench.settle(), &title(5)));
        write_journal(&paths, &[]);
        bench.restart(&paths);
        assert!(!shows(&bench.settle(), "Laut Log neu"), "no purchase, no block");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_purchase_is_taken_over_once_with_price_place_and_number() {
        let _language = setup();
        let (home, paths) = world("uebernehmen", &standard());
        let mut bench = Bench::new(&paths);
        bench.settle();
        let key = bench.state.sugg.found.list.iter().find(|p| p.class == "mult").expect("offered").key.clone();
        bench.press_button("Übernehmen: FBL-8a Arms Arctic Digital");
        let trade = trade_on_disk(&paths);
        assert_eq!(trade.len(), 2, "{trade:?}");
        let booked = &trade[1];
        assert_eq!(booked.get("ware"), Some(&Value::str("FBL-8a Arms Arctic Digital")));
        assert_eq!(booked.get("ort"), Some(&Value::str("Levski")));
        assert_eq!(booked.get("uex"), Some(&Value::int(261)));
        assert_eq!(stock::number_of(booked, "menge"), 1.0);
        assert_eq!(stock::cost_of(booked), Some(800.0));
        assert!(stock::is_pieces(booked));
        assert_eq!(Decisions::load(&paths).len(), 1, "the decision is on disk");
        let texts = bench.settle();
        assert!(shows(&texts, &title(4)), "{texts:?}");
        let message = bench.state.message.as_ref().expect("a message");
        assert_eq!(message.text, tf("s_lj_uebernommen_eins", &[("name", Arg::from("FBL-8a Arms Arctic Digital"))]));
        assert!(matches!(message.undo, Some(Undo::Suggestions(_))), "with an undo");
        // The same purchase again, by hand: nothing is booked a second time.
        assert_eq!(Decisions::load(&paths).get(&key), Some(Decision::Adopted));
        bench.state.adopt_suggestions(&[key]);
        assert_eq!(trade_on_disk(&paths).len(), 2);
        assert_eq!(stock::number_of(&trade_on_disk(&paths)[1], "menge"), 1.0);
        // After a restart: still 4 open, still one booking.
        bench.restart(&paths);
        assert!(shows(&bench.settle(), &title(4)));
        assert_eq!(trade_on_disk(&paths).len(), 2);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_purchase_that_does_not_fit_cannot_be_taken_over() {
        let _language = setup();
        let (home, paths) = world("sperre", &standard());
        let mut bench = Bench::new(&paths);
        bench.settle();
        let before = trade_on_disk(&paths);
        // An item the stock list does not know: the button does nothing and says why on hover.
        let node = bench.button("Übernehmen: Odd Thing");
        assert!(node.disabled, "{node:?}");
        assert_eq!(node.description, t("s_lj_grund_item"));
        bench.press_button("Übernehmen: Odd Thing");
        assert!(bench.state.message.is_none(), "nothing was done, nothing is said");
        assert_eq!(trade_on_disk(&paths), before);
        assert!(!paths_exist(&paths, suggestions::FILE), "no decision written");
        // Even asked by hand, the item the list does not know is not booked.
        let outside: Vec<String> =
            bench.state.sugg.found.list.iter().filter(|p| !p.adoptable()).map(|p| p.key.clone()).collect();
        assert_eq!(outside.len(), 1);
        bench.state.adopt_suggestions(&outside);
        assert_eq!(trade_on_disk(&paths), before);
        assert_eq!(bench.state.message.as_ref().map(|m| m.text.clone()), Some(t("s_lj_nicht_gebucht")));
        let _ = std::fs::remove_dir_all(home);
    }

    fn paths_exist(paths: &Paths, file: &str) -> bool {
        std::path::Path::new(&paths.app_file(file)).exists()
    }

    #[test]
    fn a_discarded_purchase_stays_discarded_after_a_restart() {
        let _language = setup();
        let (home, paths) = world("verwerfen", &standard());
        let mut bench = Bench::new(&paths);
        bench.settle();
        let before = trade_on_disk(&paths);
        bench.press_button("Verwerfen: Odd Thing");
        assert_eq!(trade_on_disk(&paths), before, "a discard books nothing");
        assert!(shows(&bench.settle(), &title(4)));
        assert!(!open_names(&bench).contains(&"Odd Thing:1".to_string()));
        bench.restart(&paths);
        let texts = bench.settle();
        assert!(shows(&texts, &title(4)) && !shows(&texts, "Odd Thing"), "{texts:?}");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn take_over_all_books_only_what_fits_and_mixes_the_price_of_one_place() {
        let _language = setup();
        let (home, paths) = world("alle", &standard());
        let mut bench = Bench::new(&paths);
        let texts = bench.settle();
        // Before the click the block says which place it adds, beside the button and on hover.
        let hint = tf("s_lj_alle_neue_orte_eins", &[("orte", Arg::from("outpost"))]);
        assert!(shows(&texts, &hint), "{texts:?}");
        assert_eq!(bench.button("Alle buchen (4)").description, hint);
        bench.press_button("Alle buchen (4)");
        let trade = trade_on_disk(&paths);
        assert_eq!(trade.len(), 4, "TS-2 and three new stacks: {trade:?}");
        let arms_at = |place: &str| {
            trade
                .iter()
                .find(|e| {
                    e.get("ware") == Some(&Value::str("Venture Arms")) && e.get("ort") == Some(&Value::str(place))
                })
                .unwrap_or_else(|| panic!("Venture Arms at {place}: {trade:?}"))
        };
        assert_eq!(stock::number_of(arms_at("Levski"), "menge"), 4.0, "3 and 1 pieces at Levski are one stack");
        assert_eq!(stock::cost_of(arms_at("Levski")), Some(415.0), "1660 for 4 pieces");
        assert_eq!(stock::number_of(arms_at("outpost"), "menge"), 1.0, "one piece at the new place");
        assert_eq!(places::own(&paths).iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["outpost"]);
        let texts = bench.settle();
        assert!(shows(&texts, &title(1)), "the unlisted item stays: {texts:?}");
        assert_eq!(open_names(&bench), ["Odd Thing:1"]);
        assert_eq!(
            bench.state.message.as_ref().map(|m| m.text.clone()),
            Some(tf("s_lj_uebernommen_n_neu_eins", &[("n", Arg::from(4)), ("ort", Arg::from("outpost"))]))
        );
        // The list of the items shows what was paid for one piece.
        assert!(shows(&texts, &t("s_lj_bezahlt")), "{texts:?}");
        assert!(
            shows(&texts, "342 aUEC") && shows(&texts, "800 aUEC"),
            "1710 for 5 pieces over both places: {texts:?}"
        );
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_undo_brings_the_stock_back_and_opens_the_purchases_again() {
        let _language = setup();
        let (home, paths) = world("rueckgaengig", &standard());
        let mut bench = Bench::new(&paths);
        bench.settle();
        let before = trade_on_disk(&paths);
        bench.press_button("Alle buchen (4)");
        assert_ne!(trade_on_disk(&paths), before);
        assert!(bench.state.data.places().iter().any(|p| p == "outpost"), "the forms know the new place");
        let Some(Message { undo: Some(back), .. }) = bench.state.message.take() else { panic!("no undo") };
        bench.state.take_back(back);
        assert_eq!(trade_on_disk(&paths), before, "the file as it was");
        assert!(Decisions::load(&paths).is_empty(), "nothing decided any more");
        assert!(places::own(&paths).is_empty(), "the new place is out of the list again");
        assert!(!bench.state.data.places().iter().any(|p| p == "outpost"));
        assert!(shows(&bench.settle(), &title(5)));
        assert_eq!(bench.state.message.as_ref().map(|m| m.text.clone()), Some(t("s_lj_zurueck_orte")));
        // Taking one over at a listed place and back says nothing of places.
        bench.press_button("Übernehmen: FBL-8a Arms Arctic Digital");
        let Some(Message { undo: Some(back), .. }) = bench.state.message.take() else { panic!("no undo") };
        bench.state.take_back(back);
        assert_eq!(bench.state.message.as_ref().map(|m| m.text.clone()), Some(t("s_lj_zurueck")));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_purchase_at_a_new_place_is_taken_over_and_the_place_is_added() {
        let _language = setup();
        let (home, paths) = world("neuer-ort", &standard());
        let mut bench = Bench::new(&paths);
        bench.settle();
        let node = bench.button_at("Übernehmen: Venture Arms", "outpost");
        assert!(!node.disabled, "{node:?}");
        assert_eq!(node.description, tf("s_lj_tipp_neuer_ort", &[("0", Arg::from("outpost"))]));
        let key = bench.state.sugg.found.list.iter().find(|p| p.place_to_book() == "outpost").unwrap().key.clone();
        bench.state.adopt_suggestions(&[key]);
        let trade = trade_on_disk(&paths);
        assert_eq!(trade.len(), 2);
        assert_eq!(trade[1].get("ort"), Some(&Value::str("outpost")));
        assert_eq!(places::own(&paths), vec![OwnPlace { name: "outpost".into(), system: None }]);
        assert_eq!(
            bench.state.message.as_ref().map(|m| m.text.clone()),
            Some(tf(
                "s_lj_uebernommen_eins_neu",
                &[("name", Arg::from("Venture Arms")), ("ort", Arg::from("outpost"))]
            ))
        );
        // The next purchase there finds the place in the list.
        assert!(bench.state.sugg.places.place("2").is_listed());
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_undo_keeps_a_new_place_that_an_entry_uses() {
        let _language = setup();
        let (home, paths) = world("ort-bleibt", &standard());
        let mut bench = Bench::new(&paths);
        bench.settle();
        bench.press_button("Alle buchen (4)");
        let Some(Message { undo: Some(back), .. }) = bench.state.message.take() else { panic!("no undo") };
        // A material is booked at the new place before the undo.
        std::fs::write(
            paths.app_file(stock::RAW_FILE),
            r#"{"format": 1, "posten": [{"material": "Iron", "menge": 1.0, "qualitaet": 500, "ort": "Outpost"}]}"#,
        )
        .unwrap();
        bench.state.take_back(back);
        assert_eq!(trade_on_disk(&paths).len(), 1, "the purchases are taken back");
        assert_eq!(places::own(&paths).len(), 1, "the place stays, the material uses it");
        assert_eq!(bench.state.message.as_ref().map(|m| m.text.clone()), Some(t("s_lj_zurueck")));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_purchase_without_a_place_is_taken_over_without_one() {
        let _language = setup();
        let bought = vec![purchase(1100, "mag", 2, 100.0, None), purchase(1200, "mult", 1, 800.0, Some("1"))];
        let (home, paths) = world("ohne-ort", &bought);
        let mut bench = Bench::new(&paths);
        let texts = bench.settle();
        assert!(shows(&texts, "laut Log an unbekannter Ort"), "{texts:?}");
        let node = bench.button("Übernehmen: Venture Arms");
        assert!(!node.disabled);
        assert_eq!(node.description, t("s_lj_tipp_ohne_ort"));
        assert!(shows(&texts, &t("s_lj_alle_ohne_ort_eins")), "{texts:?}");
        bench.press_button("Alle buchen (2)");
        let trade = trade_on_disk(&paths);
        let arms = trade.iter().find(|e| e.get("ware") == Some(&Value::str("Venture Arms"))).expect("booked");
        assert_eq!(arms.get("ort"), Some(&Value::str("")));
        assert!(places::own(&paths).is_empty(), "no place is made up");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn no_button_of_the_block_is_blocked_without_a_reason() {
        let _language = setup();
        let mut bought = standard();
        bought.push(purchase(1600, "mult", 0, 0.0, Some("1")));
        let (home, paths) = world("gruende", &bought);
        let mut bench = Bench::new(&paths);
        bench.press_button(&tf("s_lj_alle_zeigen", &[("n", Arg::from(6))]));
        let buttons = bench.buttons();
        let blocked: Vec<&Node> = buttons.iter().filter(|b| b.disabled).collect();
        assert_eq!(blocked.len(), 2, "Odd Thing and the purchase of no piece: {buttons:?}");
        for node in &blocked {
            assert!(!node.description.is_empty(), "blocked without a reason: {node:?}");
        }
        assert!(blocked.iter().any(|b| b.description == t("s_lj_grund_menge")));
        let _ = std::fs::remove_dir_all(home);
    }

    /// A stock of items: `(name, pieces, place)`, beside TS-2 at Levski.
    fn stock_of(paths: &Paths, entries: &[(&str, f64, &str)]) {
        let mut trade = trade_on_disk(paths);
        for (name, amount, place) in entries {
            let pieces = amount.to_string();
            assert_eq!(stock::trade_add_item(&mut trade, name, &pieces, place, false, None, None), Ok(()));
        }
        assert!(stock::save(&paths.app_file(stock::TRADE_FILE), &trade, "test"));
    }

    fn pieces_of(paths: &Paths, name: &str, place: &str) -> Option<f64> {
        trade_on_disk(paths)
            .iter()
            .find(|e| e.get("ware") == Some(&Value::str(name)) && e.get("ort") == Some(&Value::str(place)))
            .map(|e| stock::number_of(e, "menge"))
    }

    fn pieces(n: f64) -> String {
        amount_with_unit(Lang::De, n, true)
    }

    const GOLD: &str = "21825507-7923-4683-9bf3-9cfe316940e3";
    const STIMS: &str = "f4c68747-fa77-4c42-aff5-c3d72b8c8388";

    /// A sale at a commodity terminal at Levski.
    fn terminal_sale(time: i64, guid: &str, scu: u32, price_total: f64, proof: Proof) -> Purchase {
        Purchase { class: String::new(), guid: guid.into(), proof, ..purchase(time, "", scu, price_total, Some("1")) }
    }

    /// The names of the goods as the game data give them, and goods in the stock: `(name, SCU, place)`.
    fn goods_world(tag: &str, goods: &[(&str, f64, &str)]) -> (String, Paths) {
        let (home, paths) = world(tag, &[]);
        let names = format!(r#"{{"format": 1, "stand": "x", "waren": {{"{GOLD}": "Gold", "{STIMS}": "Stims"}}}}"#);
        std::fs::write(paths.app_file(stock_journal::names::COMMODITY_NAMES_FILE), names).expect("file");
        let mut trade = trade_on_disk(&paths);
        for (name, amount, place) in goods {
            assert_eq!(stock::trade_add(&mut trade, name, &amount.to_string(), place, true, ""), Ok(()));
        }
        assert!(stock::save(&paths.app_file(stock::TRADE_FILE), &trade, "test"));
        (home, paths)
    }

    fn scu(n: f64) -> String {
        amount_with_unit(Lang::De, n, false)
    }

    #[test]
    fn a_terminal_sale_stands_on_the_goods_tab_with_its_proof_and_takes_scu_out() {
        let _language = setup();
        let (home, paths) = goods_world("terminal", &[("Gold", 36.0, "Levski"), ("Gold", 50.5, "")]);
        write_deals(
            &paths,
            &[],
            &[
                terminal_sale(1700, GOLD, 68, 1_810_296.0, Proof::BoxGone),
                terminal_sale(1720, STIMS, 154, 664_687.0, Proof::Requested),
            ],
        );
        let mut bench = Bench::new(&paths);
        assert!(!shows(&bench.settle(), "Laut Log neu"), "no item to suggest on the tab Items");
        bench.state.items = false;
        let texts = bench.settle();
        assert!(shows(&texts, &title(2)), "{texts:?}");
        let gold_line = format!("Verkauft am Terminal, bestätigt: {} · ", scu(68.0));
        let stims_line = format!("Verkauft am Terminal, angefragt: {} · ", scu(154.0));
        assert!(shows(&texts, &gold_line) && shows(&texts, &stims_line), "{texts:?}");
        assert!(shows(&texts, "je SCU · laut Log an Levski"), "{texts:?}");
        let gold = bench.button("Abbuchen: Gold");
        assert!(!gold.disabled, "{gold:?}");
        let expected = [
            tf("s_lj_tipp_abbuchen", &[("n", Arg::from(scu(68.0)))]),
            t("s_lj_tipp_abbuchen_ohne_ort"),
            t("s_lj_tipp_bestaetigt"),
        ]
        .join("\n");
        assert_eq!(gold.description, expected);
        let stims = bench.button("Abbuchen: Stims");
        assert!(!stims.disabled, "a sale of goods the stock does not hold is still taken over: {stims:?}");
        assert_eq!(stims.description, [t("s_lj_tipp_nur_vermerken"), t("s_lj_tipp_angefragt")].join("\n"));
        bench.press_button("Abbuchen: Gold");
        assert_eq!(pieces_of(&paths, "Gold", "Levski"), None, "the place first, emptied");
        assert_eq!(pieces_of(&paths, "Gold", ""), Some(18.5), "the rest without a place");
        let message = bench.state.message.take().expect("a message");
        assert_eq!(message.text, tf("s_lj_abgebucht_eins", &[("name", Arg::from("Gold")), ("menge", Arg::from(scu(68.0)))]));
        let before = trade_on_disk(&paths);
        bench.press_button("Abbuchen: Stims");
        assert_eq!(trade_on_disk(&paths), before, "noted only, the stock stays");
        assert_eq!(bench.state.message.as_ref().map(|m| m.text.clone()), Some(tf("s_lj_vermerkt_eins", &[("name", Arg::from("Stims"))])));
        assert_eq!(Decisions::load(&paths).len(), 2);
        assert!(!shows(&bench.settle(), "Laut Log neu"), "nothing open any more");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_terminal_sale_reads_in_english_too() {
        let _language = setup();
        language::set_thread_language(Choice::En);
        let (home, paths) = goods_world("terminal-en", &[]);
        write_deals(&paths, &[], &[terminal_sale(1720, STIMS, 154, 664_687.0, Proof::Requested)]);
        let mut bench = Bench::new(&paths);
        bench.state.items = false;
        let texts = bench.settle();
        let line = format!("Sold at a terminal, requested: {} · ", amount_with_unit(Lang::En, 154.0, false));
        assert!(shows(&texts, &line), "{texts:?}");
        assert_eq!(
            bench.button("Take out: Stims").description,
            [t("s_lj_tipp_nur_vermerken"), t("s_lj_tipp_angefragt")].join("\n")
        );
        language::set_thread_language(Choice::De);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_sale_is_taken_out_of_the_stock_with_one_click_and_the_undo_brings_it_back() {
        let _language = setup();
        let (home, paths) = world("verkauf", &[]);
        stock_of(&paths, &[("Venture Arms", 3.0, "Levski"), ("Venture Arms", 4.0, "")]);
        write_deals(&paths, &[], &[purchase(1700, "mag", 2, 124.0, Some("1"))]);
        let before = trade_on_disk(&paths);
        let mut bench = Bench::new(&paths);
        let texts = bench.settle();
        assert!(shows(&texts, &title(1)), "{texts:?}");
        assert!(shows(&texts, "Verkauft: 2 Stück · 62 aUEC je Stück · laut Log an Levski · "), "{texts:?}");
        let node = bench.button("Abbuchen: Venture Arms");
        assert!(!node.disabled, "{node:?}");
        assert_eq!(node.description, tf("s_lj_tipp_abbuchen", &[("n", Arg::from(pieces(2.0)))]));
        assert!(!bench.buttons().iter().any(|b| b.name.starts_with("Übernehmen")), "a sale is no take-over");
        assert!(shows(&texts, &t("s_lj_alle_verkaeufe_eins")), "{texts:?}");
        bench.press_button("Abbuchen: Venture Arms");
        assert_eq!(pieces_of(&paths, "Venture Arms", "Levski"), Some(1.0), "taken at the place of the sale");
        assert_eq!(pieces_of(&paths, "Venture Arms", ""), Some(4.0), "the stack without a place stays");
        assert_eq!(Decisions::load(&paths).len(), 1);
        assert!(!shows(&bench.settle(), "Laut Log neu"), "nothing open any more");
        let message = bench.state.message.take().expect("a message");
        assert_eq!(
            message.text,
            tf("s_lj_abgebucht_eins", &[("name", Arg::from("Venture Arms")), ("menge", Arg::from(pieces(2.0)))])
        );
        let Some(back) = message.undo else { panic!("no undo") };
        bench.state.take_back(back);
        assert_eq!(trade_on_disk(&paths), before, "the stock as it was");
        assert!(Decisions::load(&paths).is_empty());
        assert!(shows(&bench.settle(), &title(1)));
        assert_eq!(bench.state.message.as_ref().map(|m| m.text.clone()), Some(t("s_lj_zurueck")));
        // The same sale after a restart: still open, nothing taken out by itself.
        bench.restart(&paths);
        assert!(shows(&bench.settle(), &title(1)));
        assert_eq!(trade_on_disk(&paths), before);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_sale_takes_the_rest_without_a_place_and_says_what_is_missing() {
        let _language = setup();
        let (home, paths) = world("verkauf-rest", &[]);
        stock_of(&paths, &[("Venture Arms", 1.0, "Levski"), ("Venture Arms", 1.0, "")]);
        write_deals(&paths, &[], &[purchase(1700, "mag", 3, 90.0, Some("1"))]);
        let mut bench = Bench::new(&paths);
        bench.settle();
        let node = bench.button("Abbuchen: Venture Arms");
        let expected = [
            tf("s_lj_tipp_abbuchen", &[("n", Arg::from(pieces(2.0)))]),
            t("s_lj_tipp_abbuchen_ohne_ort"),
            tf("s_lj_tipp_abbuchen_rest", &[("n", Arg::from(pieces(3.0)))]),
        ]
        .join("\n");
        assert_eq!(node.description, expected);
        bench.press_button("Abbuchen: Venture Arms");
        assert_eq!(pieces_of(&paths, "Venture Arms", "Levski"), None, "an emptied stack is gone");
        assert_eq!(pieces_of(&paths, "Venture Arms", ""), None);
        assert_eq!(trade_on_disk(&paths).len(), 1, "TS-2 stays");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_sale_the_stock_does_not_hold_is_blocked_with_its_reason_and_can_be_discarded() {
        let _language = setup();
        let (home, paths) = world("verkauf-sperre", &[]);
        stock_of(&paths, &[("Venture Arms", 2.0, "Area 18")]);
        let sales = [
            purchase(1700, "mag", 1, 50.0, Some("1")),
            purchase(1710, "odd", 1, 5.0, Some("1")),
            purchase(1720, "mult", 1, 9.0, None),
        ];
        write_deals(&paths, &[], &sales);
        let before = trade_on_disk(&paths);
        let mut bench = Bench::new(&paths);
        bench.settle();
        let buttons = bench.buttons();
        let blocked: Vec<&Node> = buttons.iter().filter(|b| b.disabled).collect();
        assert_eq!(blocked.len(), 3, "{buttons:?}");
        assert!(blocked.iter().all(|b| !b.description.is_empty()), "blocked without a reason: {blocked:?}");
        let arms = bench.button("Abbuchen: Venture Arms");
        assert_eq!(
            arms.description,
            tf("s_lj_grund_lager", &[("name", Arg::from("Venture Arms")), ("ort", Arg::from("Levski"))])
        );
        assert_eq!(bench.button("Abbuchen: Odd Thing").description, t("s_lj_grund_item_verkauf"));
        assert_eq!(
            bench.button("Abbuchen: FBL-8a Arms Arctic Digital").description,
            tf("s_lj_grund_lager_ohne_ort", &[("name", Arg::from("FBL-8a Arms Arctic Digital"))])
        );
        assert!(!buttons.iter().any(|b| b.name.starts_with("Alle buchen")), "nothing to take over: {buttons:?}");
        // Asked by hand anyway: nothing is booked, nothing decided.
        let keys: Vec<String> = bench.state.sugg.found.list.iter().map(|p| p.key.clone()).collect();
        bench.state.adopt_suggestions(&keys);
        assert_eq!(trade_on_disk(&paths), before);
        assert!(Decisions::load(&paths).is_empty());
        assert_eq!(bench.state.message.as_ref().map(|m| m.text.clone()), Some(t("s_lj_nicht_gebucht")));
        bench.press_button("Verwerfen: Venture Arms");
        assert_eq!(trade_on_disk(&paths), before, "a discard books nothing");
        assert_eq!(Decisions::load(&paths).len(), 1);
        assert!(shows(&bench.settle(), &title(2)));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn take_over_all_books_the_purchases_first_and_takes_the_sales_out_after() {
        let _language = setup();
        let (home, paths) = world("gemischt", &[]);
        // `mult` is bought at Levski and sold again later; nothing of it was in the stock before.
        write_deals(&paths, &standard(), &[purchase(1700, "mult", 1, 700.0, Some("1"))]);
        let mut bench = Bench::new(&paths);
        let texts = bench.settle();
        assert!(shows(&texts, &title(6)), "{texts:?}");
        assert!(shows(&texts, &t("s_lj_alle_verkaeufe_eins")), "{texts:?}");
        let hint = bench.button("Alle buchen (5)").description;
        assert!(hint.contains(&t("s_lj_alle_verkaeufe_eins")), "{hint}");
        // The sale alone finds nothing yet: its own button is blocked.
        assert!(bench.button("Abbuchen: FBL-8a Arms Arctic Digital").disabled);
        bench.press_button("Alle buchen (5)");
        assert_eq!(pieces_of(&paths, "FBL-8a Arms Arctic Digital", "Levski"), None, "bought and sold again");
        assert_eq!(pieces_of(&paths, "Venture Arms", "Levski"), Some(4.0));
        assert_eq!(
            bench.state.message.as_ref().map(|m| m.text.clone()),
            Some(tf(
                "s_lj_gebucht_gemischt_ort",
                &[("k", Arg::from(4)), ("v", Arg::from(1)), ("ort", Arg::from("outpost"))]
            ))
        );
        assert_eq!(open_names(&bench), ["Odd Thing:1"]);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn take_over_and_take_out_are_one_width_at_one_edge_in_every_scheme_size_and_language() {
        let _language = setup();
        let (home, paths) = world("breite", &[]);
        stock_of(&paths, &[("Venture Arms", 2.0, "Levski")]);
        write_deals(&paths, &standard(), &[purchase(1700, "mag", 1, 50.0, Some("1"))]);
        let schemes = theme::schemes();
        assert!(schemes.iter().any(|s| s.filled_buttons), "a scheme with filled buttons, in capitals");
        let mut findings = Vec::new();
        for scheme in schemes {
            for choice in [Choice::De, Choice::En] {
                language::set_thread_language(choice);
                for points in versekit_core::font_size::POINTS_RANGE.0..=versekit_core::font_size::POINTS_RANGE.1 {
                    let ctx = egui::Context::default();
                    crate::install(&ctx);
                    crate::apply_size(&ctx, points);
                    ctx.enable_accesskit();
                    let mut state = StockState::sample_on(&paths);
                    state.items = true;
                    let mut nodes: Nodes = Vec::new();
                    for _ in 0..3 {
                        let input = egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(380.0, 2400.0))),
                            ..Default::default()
                        };
                        let mut output = ctx.run_ui(input, |ui| show_page(ui, scheme, &mut state));
                        output.textures_delta.clear();
                        let Some(update) = output.platform_output.accesskit_update else { continue };
                        nodes = update
                            .nodes
                            .iter()
                            .filter_map(|(_, node)| {
                                let b = node.bounds()?;
                                let rect = Rect::from_min_max(
                                    Pos2::new(b.x0 as f32, b.y0 as f32),
                                    Pos2::new(b.x1 as f32, b.y1 as f32),
                                );
                                Some((node.role(), node.label().unwrap_or_default().to_string(), rect))
                            })
                            .collect();
                    }
                    let rects_of = |key: &str| -> Vec<Rect> {
                        let word = format!("{}: ", t(key));
                        nodes
                            .iter()
                            .filter(|(role, name, _)| *role == egui::Role::Button && name.starts_with(&word))
                            .map(|(_, _, rect)| *rect)
                            .collect()
                    };
                    let (take, out) = (rects_of("s_lj_uebernehmen"), rects_of("s_lj_abbuchen"));
                    let at = format!("{} {choice:?} size {points}", scheme.id);
                    assert!(!take.is_empty() && !out.is_empty(), "{at}: {nodes:?}");
                    let first = take[0];
                    for rect in take.iter().chain(&out) {
                        if (rect.width() - first.width()).abs() > 0.5 || (rect.left() - first.left()).abs() > 0.5 {
                            findings.push(format!("{at}: {first:?} against {rect:?}"));
                        }
                    }
                }
            }
        }
        language::set_thread_language(Choice::De);
        let _ = std::fs::remove_dir_all(home);
        assert!(findings.is_empty(), "{findings:#?}");
    }

    #[test]
    fn book_all_shows_what_it_books_with_its_icon() {
        let _language = setup();
        let (home, paths) = world("symbol", &[]);
        write_deals(&paths, &standard(), &[purchase(1700, "mag", 1, 50.0, Some("1"))]);
        let mut bench = Bench::new(&paths);
        bench.settle();
        let list = &bench.state.sugg.found.list;
        let bought = list.iter().find(|p| !p.sold).expect("a purchase");
        let sold = list.iter().find(|p| p.sold).expect("a sale");
        assert_eq!(book_all_icon(&[bought]), "package-plus");
        assert_eq!(book_all_icon(&[sold]), "package-minus");
        assert_eq!(book_all_icon(&[bought, sold]), "check-check");
        assert_eq!(book_all_icon(&[]), "package-plus");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_block_folds_shut_and_keeps_that() {
        let _language = setup();
        let (home, paths) = world("zu", &standard());
        let mut bench = Bench::new(&paths);
        let texts = bench.settle();
        assert!(bench.state.sugg.open, "open from the start");
        let head = at(&texts, &title(5));
        bench.press_at(head + egui::vec2(4.0, 4.0));
        let texts = bench.settle();
        assert!(shows(&texts, &title(5)), "the head stays");
        assert!(!shows(&texts, "laut Log an"), "the rows are gone: {texts:?}");
        assert!(!bench.state.sugg.open);
        // The list of the stock moved up into the room of the block.
        let list = at(&texts, "TS-2");
        bench.press_at(at(&texts, &title(5)) + egui::vec2(4.0, 4.0));
        let texts = bench.settle();
        assert!(at(&texts, "TS-2").y > list.y);
        // Shut, the setting goes to the file and comes back with it.
        bench.press_at(at(&texts, &title(5)) + egui::vec2(4.0, 4.0));
        let kept = bench.state.setting_values();
        let open = kept.iter().find(|(k, _)| *k == "lager_vorschlaege_offen").map(|(_, v)| v.clone());
        assert_eq!(open, Some(Value::Bool(false)));
        std::fs::write(paths.app_file("einstellungen.json"), r#"{"lager_vorschlaege_offen": false}"#).unwrap();
        assert!(!StockState::with_files(&paths).sugg.open);
        std::fs::write(paths.app_file("einstellungen.json"), "{}").unwrap();
        assert!(StockState::with_files(&paths).sugg.open, "open when it was never shut");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_filters_of_the_list_narrow_the_block_too() {
        let _language = setup();
        let (home, paths) = world("filter", &standard());
        let mut bench = Bench::new(&paths);
        bench.settle();
        bench.state.search = "fbl".into();
        let texts = bench.settle();
        let narrowed = tf("s_lj_titel_gefiltert", &[("m", Arg::from(1)), ("n", Arg::from(5))]);
        assert!(shows(&texts, &narrowed), "{texts:?}");
        assert!(shows(&texts, &tf("s_lj_gefiltert", &[("m", Arg::from(1)), ("n", Arg::from(5))])), "{texts:?}");
        assert!(!shows(&texts, "Odd Thing"), "{texts:?}");
        bench.press_button("Alle buchen (1)");
        let trade = trade_on_disk(&paths);
        assert_eq!(trade.len(), 2, "only what the filter shows: {trade:?}");
        assert_eq!(trade[1].get("ware"), Some(&Value::str("FBL-8a Arms Arctic Digital")));
        // A filter that hides every suggestion leaves the head and says so.
        bench.state.search = "nichts dergleichen".into();
        let texts = bench.settle();
        assert!(shows(&texts, &tf("s_lj_titel_gefiltert", &[("m", Arg::from(0)), ("n", Arg::from(4))])), "{texts:?}");
        assert!(!bench.buttons().iter().any(|b| b.name.contains("Alle")), "nothing to do for all");
        // The kind narrows it the same way as the list.
        bench.state.search.clear();
        bench.state.kind = Some(bench.state.data.item_kind("Venture Arms"));
        let shown = bench.state.filtered_suggestions();
        assert!(
            !shown.is_empty() && shown.iter().all(|p| bench.state.data.item_kind(&p.name) == bench.state.kind.unwrap())
        );
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_undo_leaves_a_stock_alone_that_changed_in_the_meantime() {
        let _language = setup();
        let (home, paths) = world("veraendert", &standard());
        let mut bench = Bench::new(&paths);
        bench.settle();
        bench.press_button("Alle buchen (4)");
        let Some(Message { undo: Some(back), .. }) = bench.state.message.take() else { panic!("no undo") };
        // Something else books an item before the undo.
        let mut trade = trade_on_disk(&paths);
        assert_eq!(stock::trade_add_item(&mut trade, "FR-66", "1", "Levski", false, None, None), Ok(()));
        assert!(stock::save(&paths.app_file(stock::TRADE_FILE), &trade, "test"));
        bench.state.take_back(back);
        assert_eq!(trade_on_disk(&paths), trade, "nothing taken back");
        assert_eq!(Decisions::load(&paths).len(), 4, "the decisions stay");
        assert_eq!(places::own(&paths).len(), 1, "and so does the new place");
        assert_eq!(bench.state.message.as_ref().map(|m| m.text.clone()), Some(t("s_lj_zurueck_veraendert")));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn discard_all_and_its_undo_leave_the_stock_alone() {
        let _language = setup();
        let (home, paths) = world("alle-weg", &standard());
        let mut bench = Bench::new(&paths);
        bench.settle();
        let before = trade_on_disk(&paths);
        bench.press_button("Alle verwerfen (5)");
        assert!(!shows(&bench.settle(), "Laut Log neu"), "everything decided, no block");
        assert_eq!(Decisions::load(&paths).len(), 5);
        assert_eq!(trade_on_disk(&paths), before);
        let Some(Message { undo: Some(back), .. }) = bench.state.message.take() else { panic!("no undo") };
        bench.state.take_back(back);
        assert!(shows(&bench.settle(), &title(5)));
        assert!(Decisions::load(&paths).is_empty());
        assert_eq!(trade_on_disk(&paths), before);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn more_than_five_show_five_and_a_button_shows_the_rest() {
        let _language = setup();
        let many: Vec<Purchase> = (0..8).map(|n| purchase(1000 + n, "mag", 1, 10.0, Some("1"))).collect();
        let (home, paths) = world("viele", &many);
        let mut bench = Bench::new(&paths);
        let texts = bench.settle();
        assert!(shows(&texts, &title(8)));
        let rows = |texts: &Texts| texts.iter().filter(|(text, _)| text.contains("laut Log an Levski")).count();
        assert_eq!(rows(&texts), 5);
        bench.press_button(&tf("s_lj_alle_zeigen", &[("n", Arg::from(8))]));
        assert_eq!(rows(&bench.settle()), 8);
        bench.press_button(&t("s_lj_weniger"));
        assert_eq!(rows(&bench.settle()), 5);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_block_fits_every_size_and_both_languages() {
        let _language = setup();
        let (home, paths) = world("passt", &standard());
        // Sales on top: one that can be taken out, one the stock does not hold, one of an unlisted item.
        stock_of(&paths, &[("Venture Arms", 1.0, "Levski")]);
        let sales = [
            purchase(1700, "mag", 2, 124.0, Some("1")),
            purchase(1710, "mult", 1, 9.0, None),
            purchase(1720, "odd", 1, 5.0, Some("2")),
        ];
        write_deals(&paths, &standard(), &sales);
        let mut findings = Vec::new();
        for choice in [Choice::De, Choice::En] {
            language::set_thread_language(choice);
            for points in versekit_core::font_size::POINTS_RANGE.0..=versekit_core::font_size::POINTS_RANGE.1 {
                // Open, open and narrowed by the search (the longest head), and folded shut.
                for (width, (open, search)) in [380.0, 520.0, 900.0]
                    .into_iter()
                    .flat_map(|w| [(true, ""), (true, "venture"), (false, "")].map(|v| (w, v)))
                {
                    let scheme = theme::current();
                    let ctx = egui::Context::default();
                    crate::install(&ctx);
                    crate::apply_size(&ctx, points);
                    let mut state = StockState::sample_on(&paths);
                    state.items = true;
                    state.sugg.open = open;
                    state.search = search.into();
                    let (mut over, mut block_seen) = (0.0, false);
                    for _ in 0..3 {
                        let input = egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 2400.0))),
                            ..Default::default()
                        };
                        let mut output = ctx.run_ui(input, |ui| {
                            show_page(ui, scheme, &mut state);
                            over = (ui.min_rect().right() - ui.ctx().content_rect().right()).max(0.0);
                        });
                        block_seen |= !state.open_suggestions().is_empty();
                        output.textures_delta.clear();
                    }
                    assert!(block_seen, "the block is there");
                    if over > 0.5 {
                        findings.push(format!(
                            "{choice:?} size {points} width {width} open {open} search {search:?}: {over:.0} too wide"
                        ));
                    }
                }
            }
        }
        assert!(findings.is_empty(), "{}", findings.join("\n"));
        let _ = std::fs::remove_dir_all(home);
    }

    /// Every file of a folder with its size and time of change.
    fn snapshot(folder: &std::path::Path, into: &mut Vec<(String, u64, Option<std::time::SystemTime>)>) {
        let Ok(entries) = std::fs::read_dir(folder) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.metadata() {
                Ok(meta) if meta.is_dir() => snapshot(&path, into),
                Ok(meta) => into.push((path.to_string_lossy().into_owned(), meta.len(), meta.modified().ok())),
                Err(_) => {}
            }
        }
    }

    /// The real data of a copy (`VERSEKIT_KOPIE`) and the real logs (`VERSEKIT_LOGS`, the folder `logbackups`),
    /// read only: what the block would offer for every purchase in them, and that the copy stays as it was.
    #[test]
    #[ignore = "needs a copy of real data and its logs"]
    fn real_copy_and_logs_give_the_suggestions_and_change_nothing() {
        let (Ok(root), Ok(logs)) = (std::env::var("VERSEKIT_KOPIE"), std::env::var("VERSEKIT_LOGS")) else { return };
        let mut before = Vec::new();
        snapshot(std::path::Path::new(&root), &mut before);
        versekit_core::writeguard::deny(true);
        versekit_core::writeguard::allow_under(None);
        let paths = Paths::for_copy(Host::current(), &root).expect("copy");
        let data = StockData::load(|name| paths.app_file(name));
        let (items, places) = (Items::load(&paths), Places::load(&paths));
        let mut recorded: Map = Vec::new();
        let mut files: Vec<_> = std::fs::read_dir(&logs)
            .expect("logs")
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "log"))
            .collect();
        files.sort();
        for file in &files {
            if let Some(entry) = stock_journal::read_log(&file.to_string_lossy(), None) {
                let key = entry.iter().find(|(k, _)| k == "von").map(|(_, v)| jsonio::dumps(v, None, false));
                recorded.push((key.unwrap_or_default(), Value::Object(entry)));
            }
        }
        let journal: Map = vec![("sitzungen".into(), Value::Object(recorded))];
        let sessions = stock_journal::sessions(&journal);
        let found = suggestions::proposals(&sessions, &Decisions::default(), &items, &places, &data);
        let adoptable = found.list.iter().filter(|p| p.adoptable()).count();
        let item_unlisted = found.list.iter().filter(|p| p.stock_name.is_none()).count();
        let place_new = found.list.iter().filter(|p| p.new_place().is_some()).count();
        let place_none = found.list.iter().filter(|p| p.without_place()).count();
        println!(
            "Logs {} · Sitzungen {} · Vorschläge {} (übernehmbar {adoptable}, Gegenstand nicht in der Liste {item_unlisted}, \
             Ort neu für die Liste {place_new}, ohne Ort {place_none}) · ohne Namen {}",
            files.len(),
            sessions.len(),
            found.list.len(),
            found.unnamed
        );
        println!("neue Orte: {:?}", suggestions::new_places(&found.list));
        let mut matched: Vec<String> = found
            .list
            .iter()
            .filter_map(|p| match &p.place {
                PlaceName::Listed(name) => Some(name.clone()),
                _ => None,
            })
            .collect();
        matched.sort();
        matched.dedup();
        println!("Orte der Liste: {matched:?}");
        for p in found.list.iter().take(12) {
            println!(
                "  {} · {}× · {:.2} je Stück · {:?} · {}",
                p.name,
                p.quantity,
                p.unit_price,
                p.place,
                p.adoptable()
            );
        }
        // Booked in memory on a copy of the trade list: what the stock would hold.
        let mut trade = stock::load(&paths.app_file(stock::TRADE_FILE), "kopie");
        let sold = found.list.iter().filter(|p| p.sold).count();
        let takeable = found.list.iter().filter(|p| p.sold && p.bookable(&trade)).count();
        println!("Verkäufe {sold}, davon jetzt abbuchbar {takeable}");
        for session in &sessions {
            for sale in session.sales.iter().filter(|s| s.goods()) {
                println!(
                    "  Terminal: {} · {} SCU · {:.2} je SCU · {:?} · Ort {:?} · {:?}",
                    sale.guid,
                    sale.quantity,
                    sale.unit_price(),
                    sale.shop,
                    sale.place,
                    sale.proof
                );
            }
        }
        for p in found.list.iter().filter(|p| p.goods) {
            println!("  Vorschlag Ware: {} · {:?} · {:?} · abbuchbar {}", p.name, p.proof, p.place, p.bookable(&trade));
        }
        let order = suggestions::batch(&trade, &found.list);
        let booked = order.iter().filter(|p| suggestions::adopt(&mut trade, p).is_ok()).count();
        println!("im Speicher gebucht: {booked}, Posten danach {}", trade.len());
        // The page itself, drawn on the copy.
        let mut state = StockState::with_files(&paths);
        state.items = true;
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let scheme = theme::current();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 2000.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| show_page(ui, scheme, &mut state));
            output.textures_delta.clear();
        }
        let denied = versekit_core::writeguard::take_denied();
        versekit_core::writeguard::deny(false);
        assert!(denied.is_empty(), "the page wanted to write: {denied:?}");
        let mut after = Vec::new();
        snapshot(std::path::Path::new(&root), &mut after);
        assert_eq!(before, after, "the copy was changed");
    }
}
