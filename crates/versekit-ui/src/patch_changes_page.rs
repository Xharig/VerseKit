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

//! The page "Geänderte Spielwerte" (`pages._patch_changes`): what a game patch changed in ships, weapons,
//! components and paints, field by field with old and new value. The patches stand in a list of their own
//! height with their counters; a patch without changed values is not listed, but counted below the list.
//! "Nach neuen Patches sehen" fetches in a thread of its own what erkul offers and keeps it for good.
//!
//! What the page shows is built again only when the patch files changed, another patch or area was chosen or
//! erkul offers something else: a visit that finds the same files keeps what it built.

use versekit_core::language::SpawnWithLanguage as _;
use std::collections::BTreeSet;
use std::sync::mpsc::{Receiver, TryRecvError};

use egui::{RichText, ScrollArea, Ui};
use versekit_core::catalog_cache::CatalogCache;
use versekit_core::erkul_fetch::FnFeed;
use versekit_core::language::{Arg, pa_field, t, tf};
use versekit_core::net::{Net, Web};
use versekit_core::patch_changes::{
    self, Change, Fate, FieldChange, Move, Offered, Patch, SHOWN_AT_MOST, Summary, Tone, Unreachable, Verdict,
};
use versekit_core::paths::Paths;
use versekit_core::theme::{Color, Scheme};

use crate::color;
use crate::layout::GAP;
use crate::widgets::{self, ButtonKind, RankTone};

/// How many patches the list shows before it scrolls in itself.
const LIST_ROWS: f32 = 5.0;
/// How many of the numbers that rose most, and of those that fell most, the card of the biggest changes names.
const MOVES_SHOWN: usize = 3;
/// How long a fetch from erkul may take.
const ERKUL_SECONDS: u64 = 30;
/// The largest file of erkul read.
const ERKUL_LIMIT: u64 = 64 * 1024 * 1024;

/// What fetching said.
#[derive(Clone, Debug, PartialEq)]
enum Said {
    Running,
    New(usize),
    Nothing,
    NoNet,
}

/// Where the patches come from.
enum Source {
    /// The data folder, and erkul over the net.
    Files(Box<Paths>),
    /// The preview: made-up patches in memory with how many items stayed the same, nothing asked.
    Sample(Vec<(String, Vec<Change>, Option<i64>)>),
}

/// What fetching brought: the versions kept anew (or why none), and what erkul offers.
type Fetched = (Result<Vec<String>, Unreachable>, Option<Vec<Offered>>);

/// What the page keeps.
pub struct PatchChangesState {
    source: Source,
    /// What erkul offered at the last look; `None` before it answered.
    offered: Option<Vec<Offered>>,
    list: Vec<Patch>,
    /// The patch files the list was built from.
    stamp: Vec<(String, u64, u128)>,
    chosen: Option<String>,
    /// The changes of the patch chosen, and its areas, largest first.
    changes: Vec<Change>,
    /// For each of `changes` the key its name has in the blueprint catalog.
    keys: Vec<String>,
    areas: Vec<(String, usize)>,
    /// 0: all areas; else the area at that place of `areas` plus one.
    area: usize,
    /// The numbers of the patch chosen that rose most and that fell most.
    moves: (Vec<Move>, Vec<Move>),
    /// How many items of the patch chosen stayed the same, where its file says so.
    unchanged: Option<i64>,
    /// The patch chosen is kept here (its file read once per choice, not in every frame).
    chosen_kept: bool,
    /// What is typed in the search of the changes.
    search: String,
    /// The patches already looked at; `None` until the list was first seen with what the source offers.
    seen: Option<BTreeSet<String>>,
    said: Option<Said>,
    sync_job: Option<Receiver<Fetched>>,
    offer_job: Option<Receiver<Option<Vec<Offered>>>>,
    /// The pass the page was drawn in last: a gap means it was entered again.
    last_pass: Option<u64>,
    /// How often the list was built; for the checks.
    builds: usize,
}

/// Asks erkul over the net; in the tests of this crate never.
fn erkul_get(path: &str) -> Option<Vec<u8>> {
    if cfg!(test) {
        return None;
    }
    let web = Web::new(&crate::trade::user_agent(), std::time::Duration::from_secs(ERKUL_SECONDS), ERKUL_LIMIT);
    web.get(&versekit_core::erkul_fetch::address(path)).ok()
}

impl PatchChangesState {
    /// The patches of the data folder; erkul is asked over the net.
    pub fn with_files(paths: &Paths) -> PatchChangesState {
        PatchChangesState::with_source(Source::Files(Box::new(Paths::new(paths.host.clone()))))
    }

    fn with_source(source: Source) -> PatchChangesState {
        let mut state = PatchChangesState {
            source,
            offered: None,
            list: Vec::new(),
            stamp: Vec::new(),
            chosen: None,
            changes: Vec::new(),
            keys: Vec::new(),
            areas: Vec::new(),
            area: 0,
            moves: (Vec::new(), Vec::new()),
            unchanged: None,
            chosen_kept: false,
            search: String::new(),
            seen: None,
            said: None,
            sync_job: None,
            offer_job: None,
            last_pass: None,
            builds: 0,
        };
        state.build();
        state
    }

    /// The preview: two made-up patches, one of them no longer at the source, and two without changes.
    pub fn sample() -> PatchChangesState {
        let field = |path: &str, old: Option<f64>, new: Option<f64>| FieldChange {
            path: path.to_string(),
            old: old.map(versekit_core::jsonio::Value::Float),
            new: new.map(versekit_core::jsonio::Value::Float),
        };
        let item = |kind: &str, fate: Fate, name: &str, size: Option<i64>, fields: Vec<FieldChange>| Change {
            kind: kind.to_string(),
            fate,
            id: String::new(),
            name: name.to_string(),
            size: size.map(versekit_core::jsonio::Value::int),
            fields,
        };
        let newest = vec![
            item(
                "ships",
                Fate::Changed,
                "Drake Cutter",
                None,
                vec![
                    field("precomputed.fuel.hydrogenCapacity", Some(30_000.0), Some(36_000.0)),
                    field("precomputed.fuel.burnRate.main", Some(0.00000125), Some(0.000000267)),
                ],
            ),
            item("ships", Fate::New, "MISC Starlancer TAC", None, Vec::new()),
            item(
                "weapons",
                Fate::Changed,
                "Behring M5A Cannon",
                Some(5),
                vec![
                    field("weapon.ammo.speed", Some(1600.0), Some(1450.0)),
                    field("weapon.ammo.impactRadius", Some(2.5), None),
                ],
            ),
        ];
        let older = vec![item("weapons", Fate::Gone, "Klaus & Werner Lumin V", Some(1), Vec::new())];
        let mut state = PatchChangesState::with_source(Source::Sample(vec![
            ("4.10.0-LIVE.12519617".to_string(), newest, Some(9_120)),
            ("4.9.0-LIVE.12232306".to_string(), older, None),
        ]));
        // The older patch was looked at, the newer one not yet.
        state.seen = Some(BTreeSet::from(["4.9.0-LIVE.12232306".to_string()]));
        let offered = |version: &str, date: &str, added, removed, modified| Offered {
            version: version.to_string(),
            date: date.to_string(),
            summary: Summary { added, removed, modified },
            path: String::new(),
        };
        state.offered = Some(vec![
            offered("4.10.2-LIVE.12700000", "2026-10-16", 0, 0, 0),
            offered("4.10.1-LIVE.12600000", "2026-10-09", 0, 0, 0),
            offered("4.10.0-LIVE.12519617", "2026-10-02", 1, 0, 2),
        ]);
        state.build();
        state.choose("4.10.0-LIVE.12519617");
        state
    }

    /// The patches as the list shows them.
    pub fn list(&self) -> &[Patch] {
        &self.list
    }

    /// How often the list was built.
    pub fn builds(&self) -> usize {
        self.builds
    }

    /// The versions kept, newest first.
    fn stored(&self) -> Vec<String> {
        match &self.source {
            Source::Files(paths) => patch_changes::stored(paths),
            Source::Sample(patches) => patches.iter().map(|(v, ..)| v.clone()).collect(),
        }
    }

    fn kept(&self, version: &str) -> bool {
        match &self.source {
            Source::Files(paths) => patch_changes::load(paths, version).is_some(),
            Source::Sample(patches) => patches.iter().any(|(v, ..)| v == version),
        }
    }

    fn load_changes(&self, version: &str) -> Vec<Change> {
        match &self.source {
            Source::Files(paths) => patch_changes::changes(paths, version, None),
            Source::Sample(patches) => {
                patches.iter().find(|(v, ..)| v == version).map(|(_, c, _)| c.clone()).unwrap_or_default()
            }
        }
    }

    fn load_unchanged(&self, version: &str) -> Option<i64> {
        match &self.source {
            Source::Files(paths) => patch_changes::unchanged(paths, version),
            Source::Sample(patches) => patches.iter().find(|(v, ..)| v == version).and_then(|(.., n)| *n),
        }
    }

    /// Loads the changes of `version`, its areas, its biggest changes and what stayed the same.
    fn load_chosen(&mut self, version: &str) {
        self.changes = self.load_changes(version);
        // The key a blueprint of the catalog has for each name, worked out once per choice, not in every frame.
        self.keys = self.changes.iter().map(|c| versekit_core::paths::name_key(&c.name)).collect();
        self.areas = patch_changes::categories(&self.changes);
        self.moves = patch_changes::largest_moves(&self.changes, MOVES_SHOWN);
        self.unchanged = self.load_unchanged(version);
        self.chosen_kept = self.kept(version);
    }

    /// The patches with changes not looked at yet, newest first.
    fn unread(&self) -> Vec<String> {
        self.seen.as_ref().map(|seen| patch_changes::unread(&self.list, seen)).unwrap_or_default()
    }

    /// Reads the patches looked at and joins them with those marked in this run (a write that failed loses none).
    /// The first time the list holds what the source offers and there is no file, everything in it counts as looked
    /// at: only what comes after that is news. A file that is there but cannot be read changes nothing.
    fn refresh_seen(&mut self) {
        let Source::Files(paths) = &self.source else { return };
        if let Some(from_file) = patch_changes::seen(paths) {
            self.seen.get_or_insert_with(BTreeSet::new).extend(from_file);
            return;
        }
        if self.seen.is_none() && self.offered.is_some() && !patch_changes::seen_exists(paths) {
            let all: BTreeSet<String> = self.list.iter().filter(|p| !p.empty).map(|p| p.version.clone()).collect();
            keep_seen(paths, &all);
            self.seen = Some(all);
        }
    }

    /// Marks `versions` as looked at and keeps that.
    fn mark_read(&mut self, versions: &[String]) {
        let Some(seen) = &mut self.seen else { return };
        let before = seen.len();
        seen.extend(versions.iter().cloned());
        if seen.len() != before
            && let Source::Files(paths) = &self.source
        {
            keep_seen(paths, seen);
        }
    }

    /// Builds the list and the changes of the patch chosen anew.
    fn build(&mut self) {
        let offered = self.offered.clone().unwrap_or_default();
        self.list = match &self.source {
            Source::Files(paths) => {
                self.stamp = patch_changes::stamp(paths);
                patch_changes::overview(paths, &offered)
            }
            Source::Sample(patches) => sample_overview(patches, &offered),
        };
        if let Some(version) = self.chosen.clone() {
            self.load_chosen(&version);
            self.area = self.area.min(self.areas.len());
        }
        self.refresh_seen();
        self.builds += 1;
    }

    /// Chooses a patch; the largest area stands chosen, not all of them.
    fn choose(&mut self, version: &str) {
        self.chosen = Some(version.to_string());
        self.load_chosen(version);
        self.area = usize::from(!self.areas.is_empty());
    }

    /// Fetches the new patches of erkul and what it offers, in a thread of its own.
    fn search(&mut self, ctx: &egui::Context) {
        let Source::Files(paths) = &self.source else { return };
        if self.sync_job.is_some() {
            return;
        }
        let own = Paths::new(paths.host.clone());
        let (sender, receiver) = std::sync::mpsc::channel();
        let lock = versekit_core::writeguard::state();
        let wake = ctx.clone();
        let started = std::thread::Builder::new().name("patch-aenderungen".into()).spawn_with_language(move || {
            versekit_core::writeguard::adopt(&lock);
            let feed = FnFeed(erkul_get);
            let new = patch_changes::sync(&own, &feed);
            let offer = patch_changes::offered(&feed);
            let _ = sender.send((new, offer));
            wake.request_repaint();
        });
        if started.is_ok() {
            self.sync_job = Some(receiver);
            self.said = Some(Said::Running);
        }
    }

    /// Asks erkul in a thread of its own what it offers; the list is built again when that changed.
    fn look_at_offer(&mut self, ctx: &egui::Context) {
        let Source::Files(paths) = &self.source else { return };
        if self.offer_job.is_some() || versekit_core::net::offline_in(&paths.host) {
            return;
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        let wake = ctx.clone();
        let started = std::thread::Builder::new().name("patch-angebot".into()).spawn_with_language(move || {
            let _ = sender.send(patch_changes::offered(&FnFeed(erkul_get)));
            wake.request_repaint();
        });
        if started.is_ok() {
            self.offer_job = Some(receiver);
        }
    }

    /// Takes in what the threads brought; at a new visit the list is built again only when the files changed.
    fn tick(&mut self, ctx: &egui::Context) {
        let pass = ctx.cumulative_pass_nr();
        let entered = self.last_pass.is_none_or(|last| pass > last + 1);
        self.last_pass = Some(pass);
        if entered {
            if let Source::Files(paths) = &self.source
                && patch_changes::stamp(paths) != self.stamp
            {
                self.build();
            }
            self.look_at_offer(ctx);
        }
        if let Some(job) = &self.offer_job {
            match job.try_recv() {
                Ok(offer) => {
                    self.offer_job = None;
                    if offer.is_some() && offer != self.offered {
                        self.offered = offer;
                        self.build();
                    }
                }
                Err(TryRecvError::Disconnected) => self.offer_job = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        if let Some(job) = &self.sync_job {
            match job.try_recv() {
                Ok((new, offer)) => {
                    self.sync_job = None;
                    self.said = Some(match &new {
                        Err(_) => Said::NoNet,
                        Ok(list) if list.is_empty() => Said::Nothing,
                        Ok(list) => Said::New(list.len()),
                    });
                    if offer.is_some() {
                        self.offered = offer;
                    }
                    self.build();
                }
                Err(TryRecvError::Disconnected) => {
                    self.sync_job = None;
                    self.said = Some(Said::NoNet);
                }
                Err(TryRecvError::Empty) => {}
            }
        }
    }
}

/// Keeps the patches looked at; a write the guard refuses is left, any other failure noted.
fn keep_seen(paths: &Paths, seen: &BTreeSet<String>) {
    match patch_changes::set_seen(paths, seen) {
        Ok(()) => {}
        Err(error) if versekit_core::writeguard::refused(&error) => {}
        Err(error) => versekit_core::errors::record("patch_changes_page.seen", &error),
    }
}

/// The list of the preview: what is offered and what is kept, as `overview` puts them together.
fn sample_overview(patches: &[(String, Vec<Change>, Option<i64>)], offered: &[Offered]) -> Vec<Patch> {
    let mut list: Vec<Patch> = offered
        .iter()
        .map(|o| Patch {
            version: o.version.clone(),
            short: o.version.split('-').next().unwrap_or_default().to_string(),
            date: o.date.clone(),
            summary: o.summary.clone(),
            empty: !o.summary.has_content(),
            at_source: true,
            kept: patches.iter().any(|(v, ..)| *v == o.version),
        })
        .collect();
    for (version, changes, _) in patches {
        if list.iter().any(|p| &p.version == version) {
            continue;
        }
        let count = |fate: Fate| changes.iter().filter(|c| c.fate == fate).count() as i64;
        list.push(Patch {
            version: version.clone(),
            short: version.split('-').next().unwrap_or_default().to_string(),
            date: "2026-08-01".to_string(),
            summary: Summary { added: count(Fate::New), removed: count(Fate::Gone), modified: count(Fate::Changed) },
            empty: false,
            at_source: false,
            kept: true,
        });
    }
    list
}

fn small(ui: &mut Ui, text: &str, tone: Color) {
    widgets::reading(ui, RichText::new(text).small().color(color(tone)));
}

/// The colour of a tone of a changed field: better and worse in the blue and yellow of [`widgets::change_color`]
/// (apart for those who tell red from green badly, never red and green), a field that came in the accent, one that
/// went, the same value and a change without a judgement muted (gold is too close to the yellow of worse in some
/// schemes, so a field that went says so in words and muted; the legend calls the muted colour "no verdict").
fn tone_colour(scheme: &Scheme, tone: &Tone) -> Color {
    match tone {
        Tone::Better => widgets::change_color(scheme, true),
        Tone::Worse => widgets::change_color(scheme, false),
        Tone::Up => scheme.accent,
        Tone::Gone | Tone::Same | Tone::Plain => scheme.sub,
    }
}

/// The bar of a change in the card of the biggest changes: blue for the better, yellow for the worse, muted where
/// the field has no judgement.
fn bar_tone(verdict: Verdict) -> RankTone {
    match verdict {
        Verdict::Better => RankTone::Better,
        Verdict::Worse => RankTone::Worse,
        Verdict::Neutral => RankTone::Muted,
    }
}

/// The line that says what the colours mean, in the two colours themselves.
fn legend(ui: &mut Ui, scheme: &Scheme) {
    ui.horizontal_wrapped(|row| {
        row.spacing_mut().item_spacing.x = 4.0;
        let part = |row: &mut Ui, text: String, tone: Color| {
            row.add(egui::Label::new(RichText::new(text).small().color(color(tone))));
        };
        part(row, t("s_pag_farbe_besser"), widgets::change_color(scheme, true));
        part(row, "·".to_string(), scheme.sub);
        part(row, t("s_pag_farbe_schlechter"), widgets::change_color(scheme, false));
        part(row, "·".to_string(), scheme.sub);
        part(row, t("s_pag_farbe_offen"), scheme.sub);
    });
}

/// What a click on the page asks of the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Open the blueprint list on the blueprint of the catalog with this name.
    ToBlueprint(String),
}

/// Draws the page; `catalog` is the blueprint catalog, which tells the parts that have a blueprint.
pub fn show_page(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &mut PatchChangesState,
    catalog: &CatalogCache,
) -> Option<Action> {
    state.tick(ui.ctx());
    small(ui, &t("s_pav_lead"), scheme.sub);
    ui.add_space(GAP);
    patch_list(ui, scheme, state);
    ui.add_space(GAP);
    if biggest(ui, scheme, state) {
        ui.add_space(GAP);
    }
    area_choice(ui, scheme, state);
    ui.add_space(GAP);
    items(ui, scheme, state, catalog)
}

/// The card of the patches: the collection, the button for new patches with its line, the patches with changes in
/// a list of its own height, the empty ones counted below it.
fn patch_list(ui: &mut Ui, scheme: &Scheme, state: &mut PatchChangesState) {
    let shown: Vec<Patch> = state.list.iter().filter(|p| !p.empty).cloned().collect();
    let left_out = state.list.len() - shown.len();
    let row_height = ui.spacing().interact_size.y + ui.spacing().item_spacing.y + 6.0;
    let unread = state.unread();
    let mut picked = None;
    let mut all_read = false;
    widgets::card(ui, scheme, |ui| {
        ui.set_width(ui.available_width());
        widgets::card_title(ui, scheme, &t("s_pa_patches_kopf"));
        ui.add_space(4.0);
        small(ui, &t("s_pav_sammlung"), scheme.sub);
        ui.add_space(6.0);
        ui.horizontal_wrapped(|row| {
            let word = t("s_pa_suchen");
            let busy = state.sync_job.is_some();
            if row.add_enabled(!busy, widgets::Button::new(&word, ButtonKind::Normal, scheme)).clicked() {
                state.search(row.ctx());
            }
            let said = match &state.said {
                None => None,
                Some(Said::Running) => Some(t("s_pa_laeuft")),
                Some(Said::New(n)) => Some(tf("s_pa_neu", &[("n", Arg::from(*n))])),
                Some(Said::Nothing) => Some(t("s_pav_keine_neuen")),
                Some(Said::NoNet) => Some(t("s_pa_kein_netz")),
            };
            if let Some(said) = said {
                row.add(egui::Label::new(RichText::new(said).small().color(color(scheme.sub))).wrap());
            }
        });
        // What came since the last visit, with the way to take it as read.
        if !unread.is_empty() {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|row| {
                let line = tf("s_pag_seit_besuch", &[("n", Arg::from(unread.len()))]);
                row.add(egui::Label::new(RichText::new(line).small().color(color(scheme.accent))).wrap());
                let word = t("s_pag_alle_gelesen");
                let button = widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("check-check");
                all_read = row.add(button).clicked();
            });
        }
        if !shown.is_empty() {
            ui.add_space(6.0);
            let area = ScrollArea::vertical()
                .id_salt("patch-liste")
                .max_height(LIST_ROWS * row_height)
                .min_scrolled_height(LIST_ROWS * row_height)
                .auto_shrink([false, true]);
            widgets::scroll_area(ui, scheme, area, |ui| {
                for patch in &shown {
                    let on = state.chosen.as_deref() == Some(patch.version.as_str());
                    ui.horizontal_wrapped(|row| {
                        let kind = if on { ButtonKind::Strong } else { ButtonKind::Normal };
                        if row.add(widgets::Button::new(&patch.version, kind, scheme)).clicked() {
                            picked = Some(patch.version.clone());
                        }
                        if !patch.date.is_empty() {
                            row.add(egui::Label::new(RichText::new(&patch.date).small().color(color(scheme.sub))));
                        }
                        row.add(egui::Label::new(RichText::new(patch_changes::counter_text(&patch.summary)).small()));
                        if !patch.at_source {
                            widgets::badge(row, scheme, scheme.accent, &t("s_pa_nur_hier"));
                        }
                        if unread.contains(&patch.version) {
                            widgets::badge(row, scheme, scheme.gold, &t("s_pag_ungelesen"));
                        }
                    });
                }
            });
        }
        if left_out > 0 {
            ui.add_space(4.0);
            small(ui, &tf("s_pav_leere_weg", &[("n", Arg::from(left_out))]), scheme.sub);
        }
    });
    if all_read {
        state.mark_read(&unread);
    }
    if let Some(version) = picked {
        state.choose(&version);
        // A patch not fetched yet shows no values: it stays unread until it can be looked at.
        if state.chosen_kept {
            state.mark_read(std::slice::from_ref(&version));
        }
    }
}

/// The card of the biggest changes of the patch chosen: its counter with what stayed the same, the numbers that
/// rose most and those that fell most, each as a bar measured at the largest of its kind. Drawn only for a patch
/// kept here; returns whether it was drawn.
fn biggest(ui: &mut Ui, scheme: &Scheme, state: &PatchChangesState) -> bool {
    let Some(version) = state.chosen.as_deref() else { return false };
    if !state.chosen_kept {
        return false;
    }
    let summary = state.list.iter().find(|p| p.version == version).map(|p| p.summary.clone()).unwrap_or_default();
    let counter = match state.unchanged {
        Some(same) => tf(
            "s_pag_zaehler",
            &[
                ("plus", Arg::from(summary.added)),
                ("minus", Arg::from(summary.removed)),
                ("tilde", Arg::from(summary.modified)),
                ("gleich", Arg::from(same)),
            ],
        ),
        None => patch_changes::counter_text(&summary),
    };
    let (rises, falls) = &state.moves;
    widgets::card(ui, scheme, |ui| {
        ui.set_width(ui.available_width());
        widgets::card_title(ui, scheme, &t("s_pag_kopf"));
        ui.add_space(4.0);
        small(ui, &counter, scheme.sub);
        if rises.is_empty() && falls.is_empty() {
            ui.add_space(4.0);
            small(ui, &t("s_pag_keine_zahl"), scheme.sub);
            return;
        }
        ui.add_space(2.0);
        legend(ui, scheme);
        let groups = [("s_pag_gestiegen", rises), ("s_pag_gefallen", falls)];
        for (key, moves) in groups {
            if moves.is_empty() {
                continue;
            }
            ui.add_space(GAP / 2.0);
            ui.label(RichText::new(t(key)).strong());
            ui.add_space(2.0);
            let largest = moves.iter().map(|m| m.share.abs()).fold(0.0, f64::max).max(f64::MIN_POSITIVE);
            for change in moves {
                let name = format!("{} · {}", change.name, pa_field(&change.path));
                let (before, after) = patch_changes::pair(Some(&change.old), Some(&change.new));
                let share = patch_changes::share_text(Some(&change.old), Some(&change.new));
                let number = format!("{before} → {after}  {share}");
                let share = (change.share.abs() / largest) as f32;
                widgets::ranking_row_tone(ui, scheme, &name, &number, share, bar_tone(change.verdict));
                ui.add_space(2.0);
            }
        }
    });
    true
}

/// The areas of the patch chosen with their counts, all of them first, in a card of their own.
fn area_choice(ui: &mut Ui, scheme: &Scheme, state: &mut PatchChangesState) {
    if state.chosen.is_none() || state.areas.is_empty() {
        return;
    }
    let all_word = t("s_pa_alle");
    let total = state.changes.len();
    let mut choices: Vec<(&str, usize)> = vec![(all_word.as_str(), total)];
    choices.extend(state.areas.iter().map(|(kind, n)| (kind.as_str(), *n)));
    let mut chosen = state.area;
    let changed = widgets::card(ui, scheme, |ui| {
        ui.set_width(ui.available_width());
        widgets::card_title(ui, scheme, &t("s_pa_kategorie"));
        ui.add_space(4.0);
        widgets::count_toggles(ui, scheme, &choices, &mut chosen)
    })
    .inner;
    if changed {
        state.area = chosen;
    }
}

/// The card of the changes: the items of the area chosen, at most [`SHOWN_AT_MOST`], the rest counted; or the line
/// that says why there are none.
fn items(ui: &mut Ui, scheme: &Scheme, state: &mut PatchChangesState, catalog: &CatalogCache) -> Option<Action> {
    let mut action = None;
    widgets::card(ui, scheme, |ui| {
        ui.set_width(ui.available_width());
        widgets::card_title(ui, scheme, &t("s_pa_aenderungen_kopf"));
        ui.add_space(4.0);
        let Some(version) = state.chosen.as_deref() else {
            let knopf = t("s_pa_suchen");
            let text = if state.stored().is_empty() {
                tf("s_pav_erst_holen", &[("knopf", Arg::from(knopf))])
            } else {
                t("s_pav_waehlen")
            };
            small(ui, &text, scheme.sub);
            return;
        };
        if !state.chosen_kept {
            let entry = state.list.iter().find(|p| p.version == version);
            let text = match entry {
                Some(patch) if !patch.empty => tf(
                    "s_pav_nicht_geholt",
                    &[
                        ("plus", Arg::from(patch.summary.added)),
                        ("minus", Arg::from(patch.summary.removed)),
                        ("tilde", Arg::from(patch.summary.modified)),
                        ("knopf", Arg::from(t("s_pa_suchen"))),
                    ],
                ),
                _ => t("s_pav_leer_klick"),
            };
            small(ui, &text, scheme.sub);
            return;
        }
        let wanted = state.area.checked_sub(1).and_then(|i| state.areas.get(i)).map(|(kind, _)| kind.as_str());
        let in_area: Vec<(usize, &Change)> =
            state.changes.iter().enumerate().filter(|(_, c)| wanted.is_none_or(|w| c.kind == w)).collect();
        if in_area.is_empty() {
            small(ui, &t("s_pa_nichts_hier"), scheme.sub);
            return;
        }
        // The search runs inside the area chosen, like the areas of the other pages narrow their search.
        let width = ui.available_width();
        widgets::search_field(ui, &mut state.search, &t("s_pag_suche_platz"), scheme, width);
        ui.add_space(4.0);
        let shown: Vec<(usize, &Change)> =
            in_area.into_iter().filter(|(_, c)| patch_changes::matches(c, &state.search)).collect();
        if shown.is_empty() {
            small(ui, &tf("s_pag_suche_nichts", &[("suche", Arg::from(state.search.trim()))]), scheme.sub);
            return;
        }
        legend(ui, scheme);
        ui.add_space(4.0);
        widgets::list_frame(ui, scheme, |list| {
            list.set_width(list.available_width());
            for (k, (at, change)) in shown.iter().take(SHOWN_AT_MOST).enumerate() {
                if k > 0 {
                    list.separator();
                }
                // The chip only for what is new or changed: a part that is gone is no news for a blueprint.
                let blueprint = (change.fate != Fate::Gone)
                    .then(|| state.keys.get(*at).and_then(|key| catalog.blueprints.iter().find(|b| &b.key == key)))
                    .flatten();
                if item(list, scheme, change, blueprint.map(|b| b.name.as_str())) {
                    action = blueprint.map(|b| Action::ToBlueprint(b.name.clone()));
                }
            }
        });
        if shown.len() > SHOWN_AT_MOST {
            ui.add_space(4.0);
            small(ui, &tf("s_pa_mehr", &[("n", Arg::from(shown.len() - SHOWN_AT_MOST))]), scheme.sub);
        }
    });
    action
}

/// One item, an entry of the list in the card of the changes: what happened to it, its name and size, the chip to
/// its blueprint where `blueprint` names one of the catalog, and each changed field. Whether the chip was clicked.
fn item(ui: &mut Ui, scheme: &Scheme, change: &Change, blueprint: Option<&str>) -> bool {
    let mut jump = false;
    ui.vertical(|ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|row| {
            let (word, tone) = match change.fate {
                Fate::New => (t("s_pa_zustand_neu"), scheme.accent),
                Fate::Gone => (t("s_pa_zustand_weg"), scheme.gold),
                Fate::Changed => (t("s_pa_zustand_geae"), scheme.sub),
            };
            widgets::badge(row, scheme, tone, &word);
            row.add(egui::Label::new(RichText::new(&change.name).strong()).wrap());
            if let Some(size) = &change.size {
                let size = patch_changes::number_text(Some(size), 4);
                row.add(egui::Label::new(
                    RichText::new(tf("s_pa_groesse", &[("n", Arg::from(size))])).small().color(color(scheme.sub)),
                ));
            }
            // The way to the blueprint list, with the blueprint as search and open.
            if let Some(name) = blueprint {
                let word = t("s_pag_bauplan");
                let chip = row.add(widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("clipboard-list"));
                jump = chip.on_hover_text(tf("s_pag_bauplan_h", &[("name", Arg::from(name))])).clicked();
            }
        });
        for field in &change.fields {
            let (text, tone) = patch_changes::field_text(field);
            ui.horizontal_wrapped(|row| {
                row.add_space(12.0);
                row.add(egui::Label::new(RichText::new(pa_field(&field.path)).small().color(color(scheme.sub))).wrap());
                row.add(egui::Label::new(RichText::new(text).small().color(color(tone_colour(scheme, &tone)))).wrap());
            });
        }
    });
    jump
}

#[cfg(test)]
mod tests;
