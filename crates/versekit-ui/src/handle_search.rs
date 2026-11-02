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

//! The handle search as parts for a window: the slim field (for the middle of the info bar), the card of the
//! handle found below it, the lines of the log monitor and the settings. Every part draws into the `Ui` it is
//! given; the window around them belongs to whoever shows them.
//!
//! Enter searches, Ctrl+Enter fetches anew, Escape closes the card. The site is asked in a thread of its own,
//! never in the frame.
//!
//! A click on the name of an organization on the card opens its members, page by page; a click on a member
//! searches it. The empty field with the cursor shows the handles searched lately; a click searches one.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};

use egui::{Align, Layout, Response, RichText, Role, Sense, Stroke, Ui, WidgetInfo, pos2, vec2};
use versekit_core::browser;
use versekit_core::handle_query::cache::{self, KEY_AFFILIATIONS, KEY_CLEAR_AFTER, KEY_DAYS, KEY_HIDE_HIDDEN};
use versekit_core::handle_query::log_monitor::{self, Monitor};
use versekit_core::handle_query::members::{self, Member};
use versekit_core::handle_query::relations::{ImportError, Kind, Relations, Stance};
use versekit_core::handle_query::{Failure, Found, Membership, Org, Settings, page};
use versekit_core::jsonio::Value;
use versekit_core::language::{self, Arg, t, tf};
use versekit_core::net::Net;
use versekit_core::paths::Paths;
use versekit_core::theme::{Color, Scheme};

use crate::color;
use crate::file_dialog::{self, Pending};
use crate::handle_pictures::Pictures;
use crate::widgets::{self, ButtonKind};

/// The width of the field in the bar.
pub const FIELD_WIDTH: f32 = 180.0;
/// The width of the card and of the log lines: a window 375 wide with pictures of 70 as the model, half a
/// centimetre narrower, the pictures in the same proportion.
pub const CARD_WIDTH: f32 = 356.0;
/// The size of the marks of a relation.
const MARK_SIZE: f32 = 16.0;
/// The sizes of the profile picture, of a logo and of the title picture.
pub const AVATAR_SIZE: f32 = 66.0;
pub const LOGO_SIZE: f32 = 66.0;
const TITLE_SIZE: f32 = 15.0;
/// The size of one rank star and the space between two.
pub const STAR_SIZE: f32 = 15.0;
const STAR_GAP: f32 = 4.0;
/// The share of the text beside a picture its right column takes.
const RIGHT_SHARE: f32 = 0.40;
/// The names are this much larger than the text below them.
const NAME_SCALE: f32 = 4.0 / 3.0;
/// The highest the list of members gets on the card and on the page before it scrolls.
pub const MEMBERS_HEIGHT: f32 = 200.0;
pub const MEMBERS_HEIGHT_PAGE: f32 = 420.0;
/// The share of a member row the handle takes beside the stars; the rank takes the rest.
const MEMBER_HANDLE_SHARE: f32 = 0.55;
/// How many handles searched lately the empty field shows.
pub const HISTORY_MAX: usize = 8;

/// The members of one organization as the search shows them.
struct MembersView {
    sid: String,
    shown: Option<Result<members::Found, Failure>>,
    /// The way the answer of a page comes, and whether it is a further page.
    job: Option<(bool, Receiver<Result<members::Found, Failure>>)>,
    /// Why the last further page did not come.
    more_failed: Option<Failure>,
}

/// A cell `width` wide and `height` high in a row, its content from the left.
fn cell(ui: &mut Ui, width: f32, height: f32, add: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(vec2(width, height), Layout::left_to_right(Align::Center), |ui| {
        ui.set_width(width);
        add(ui);
    });
}

/// The icon of a stance.
pub fn stance_icon(stance: Stance) -> &'static str {
    match stance {
        Stance::Friendly => "shield-check",
        Stance::Neutral => "shield-half",
        Stance::Hostile => "shield-alert",
    }
}

/// The colour of a stance in `scheme`.
pub fn stance_color(scheme: &Scheme, stance: Stance) -> Color {
    match stance {
        Stance::Friendly => scheme.accent,
        Stance::Neutral => scheme.yellow,
        Stance::Hostile => scheme.red,
    }
}

/// The text of a failure.
pub fn failure_text(failure: &Failure) -> String {
    match failure {
        Failure::Invalid => t("s_hq_ungueltig"),
        Failure::NotFound => t("s_hq_nicht_gefunden"),
        Failure::Offline => t("s_hq_offline"),
        Failure::Failed(why) => tf("s_hq_fehler", &[("grund", Arg::Text(why.clone()))]),
        Failure::Busy => t("s_hq_belegt"),
    }
}

/// The clock of the search and the log monitor: seconds since 1970.
pub fn now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

/// The five rank stars of an organization as circles, `lit` of them filled.
pub fn stars(ui: &mut Ui, scheme: &Scheme, lit: u8) {
    let width = 5.0 * STAR_SIZE + 4.0 * STAR_GAP;
    let (rect, response) = ui.allocate_exact_size(vec2(width, MARK_SIZE), Sense::hover());
    let radius = STAR_SIZE / 2.0;
    for n in 0..5u8 {
        let center = pos2(rect.min.x + radius + f32::from(n) * (STAR_SIZE + STAR_GAP), rect.center().y);
        if n < lit {
            ui.painter().circle_filled(center, radius, color(scheme.fg));
        } else {
            ui.painter().circle_stroke(center, radius - 0.5, Stroke::new(1.0, color(scheme.sub)));
        }
    }
    let text = tf("s_hq_sterne", &[("n", Arg::Int(i64::from(lit.min(5))))]);
    response.widget_info(|| WidgetInfo::labeled(Role::Image, true, &text));
    response.on_hover_text(text);
}

/// What the search keeps between frames.
pub struct HandleSearch {
    paths: Option<Paths>,
    net: Option<Arc<dyn Net>>,
    /// The text in the field.
    pub query: String,
    /// The handle being asked and the way its answer comes.
    job: Option<(String, Receiver<Result<Found, Failure>>)>,
    shown: Option<Result<Found, Failure>>,
    /// Since when the card stands untouched ([`now`]); the card and the field empty themselves after the
    /// seconds set.
    shown_at: f64,
    settings: Settings,
    relations: Relations,
    monitor: Monitor,
    log: log_monitor::Settings,
    /// The handle of the player: a click on a log line sends the other one, and the empty field names it.
    own: Option<String>,
    open_url: fn(&str) -> bool,
    export: Option<(Pending, String)>,
    import: Option<Pending>,
    /// What the last export or import said.
    message: Option<String>,
    /// The field as drawn last.
    field_id: Option<egui::Id>,
    /// The pointer was on the field in the last frame.
    field_hovered: bool,
    /// A setting or a relation was written since [`HandleSearch::take_changed`] was asked last.
    changed: bool,
    /// The pictures of the card.
    pictures: Pictures,
    /// The organization whose members are open.
    members: Option<MembersView>,
    /// Whether the card shows the members itself; the page shows them below it with [`HandleSearch::members`].
    members_in_card: bool,
    /// The handles searched lately while the empty field has the cursor: handle and the SIDs of its
    /// organizations, the newest first.
    history: Option<Vec<(String, Vec<String>)>>,
    /// Where they were drawn last: they stay while the mouse is over them.
    history_rect: Option<egui::Rect>,
}

impl HandleSearch {
    /// The search of the data folder, asking the real site.
    pub fn with_files(paths: &Paths) -> HandleSearch {
        #[cfg(not(test))]
        let net: Option<Arc<dyn Net>> = {
            let agent = versekit_core::basetool::config::user_agent(env!("CARGO_PKG_VERSION"));
            Some(Arc::new(versekit_core::net::Web::new(&agent, std::time::Duration::from_secs(10), 2 * 1024 * 1024)))
        };
        // The tests of this crate never ask the network.
        #[cfg(test)]
        let net: Option<Arc<dyn Net>> = None;
        HandleSearch::with_net(paths, net)
    }

    /// Like [`HandleSearch::with_files`], asking over `net`.
    pub fn with_net(paths: &Paths, net: Option<Arc<dyn Net>>) -> HandleSearch {
        let own = Paths::new(paths.host.clone());
        HandleSearch {
            settings: Settings::read(&own),
            relations: Relations::load(&own),
            log: log_monitor::Settings::read(&own),
            pictures: Pictures::new(Some(&own), net.clone()),
            paths: Some(own),
            net,
            ..HandleSearch::empty()
        }
    }

    /// A search without a data folder and without the net (the preview, the self-check): it asks nothing.
    pub fn empty() -> HandleSearch {
        HandleSearch {
            settings: Settings::default(),
            relations: Relations::default(),
            log: log_monitor::Settings { on: true, seconds: log_monitor::SECONDS_DEFAULT },
            paths: None,
            net: None,
            query: String::new(),
            job: None,
            shown: None,
            shown_at: 0.0,
            monitor: Monitor::default(),
            own: None,
            open_url: browser::open_in_browser,
            export: None,
            import: None,
            message: None,
            field_id: None,
            field_hovered: false,
            changed: false,
            pictures: Pictures::none(),
            members: None,
            members_in_card: true,
            history: None,
            history_rect: None,
        }
    }

    /// Shows `found` and the members `roster` below it, without the net and kept until closed (the preview).
    pub fn with_sample(mut self, found: Found, roster: members::Roster) -> HandleSearch {
        self.settings.clear_after = 0;
        self.show(Ok(found));
        let sid = roster.sid.clone();
        let shown = Some(Ok(members::Found { roster, live: true, failed: None }));
        self.members = Some(MembersView { sid, shown, job: None, more_failed: None });
        self
    }

    /// Whether a picture of the card is still on its way.
    pub fn pictures_waiting(&self) -> bool {
        self.pictures.waiting()
    }

    /// Replaces what opens an address (tests do not open a browser).
    pub fn with_opener(mut self, open_url: fn(&str) -> bool) -> HandleSearch {
        self.open_url = open_url;
        self
    }

    /// Starts with `relations` instead of the ones kept (the preview).
    pub fn with_relations(mut self, relations: Relations) -> HandleSearch {
        self.relations = relations;
        self
    }

    /// The data folder the search reads and writes; `None` in the preview.
    pub fn data_paths(&self) -> Option<&Paths> {
        self.paths.as_ref()
    }

    /// Whether a setting or a relation was written since the last call; asking resets it.
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    /// The answer shown, `None` before the first search and after closing.
    pub fn shown(&self) -> Option<&Result<Found, Failure>> {
        self.shown.as_ref()
    }

    /// Whether a search runs.
    pub fn searching(&self) -> bool {
        self.job.is_some()
    }

    pub fn relations(&self) -> &Relations {
        &self.relations
    }

    /// The handle of the player, from the log.
    pub fn set_own_handle(&mut self, handle: Option<String>) {
        self.own = handle;
    }

    /// What the empty field says: the handle of the player once the log knows it.
    fn field_hint(&self) -> String {
        match &self.own {
            Some(own) => tf("s_hq_feld_hinweis_eigen", &[("handle", Arg::Text(own.clone()))]),
            None => t("s_hq_feld_hinweis"),
        }
    }

    /// Closes the card and the members below it.
    pub fn close(&mut self) {
        self.shown = None;
        self.members = None;
    }

    /// Shows `answer` from now on.
    fn show(&mut self, answer: Result<Found, Failure>) {
        self.shown = Some(answer);
        self.shown_at = now();
    }

    /// Empties the card and the field once the card stood untouched for the seconds set; `busy` (typing, a
    /// search running) keeps it and counts anew. The seconds left until then, `None` when nothing is to go.
    pub fn expire(&mut self, now: f64, busy: bool) -> Option<f64> {
        if self.shown.is_none() || self.settings.clear_after <= 0 {
            return None;
        }
        if busy {
            self.shown_at = now;
        } else if self.settings.clear_due(self.shown_at, now) {
            self.close();
            self.query.clear();
            return None;
        }
        Some(self.shown_at + self.settings.clear_after as f64 - now)
    }

    /// Searches `handle` in a thread of its own; `force` fetches anew. A search running goes on, a second one
    /// waits for it.
    pub fn search(&mut self, ctx: &egui::Context, handle: &str, force: bool) {
        self.history = None;
        let Some(handle) = page::valid_handle(handle) else {
            self.show(Err(Failure::Invalid));
            return;
        };
        self.query.clone_from(&handle);
        if self.job.is_some() {
            return;
        }
        let (Some(paths), Some(net)) = (&self.paths, &self.net) else {
            self.show(Err(Failure::Offline));
            return;
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        let own = Paths::new(paths.host.clone());
        let net = Arc::clone(net);
        let lock = versekit_core::writeguard::state();
        let wake = ctx.clone();
        let asked = handle.clone();
        let started = std::thread::Builder::new().name("handle-suche".into()).spawn(move || {
            versekit_core::writeguard::adopt(&lock);
            let _ = sender.send(cache::search(&own, net.as_ref(), &asked, force, now()));
            wake.request_repaint();
        });
        if started.is_ok() {
            self.job = Some((handle, receiver));
        }
    }

    /// The SID of the organization whose members are open.
    pub fn members_sid(&self) -> Option<&str> {
        self.members.as_ref().map(|m| m.sid.as_str())
    }

    /// Whether a page of members is on its way.
    pub fn members_loading(&self) -> bool {
        self.members.as_ref().is_some_and(|m| m.job.is_some())
    }

    /// The members as shown, `None` while none are open or the first page is on its way.
    pub fn members_shown(&self) -> Option<&Result<members::Found, Failure>> {
        self.members.as_ref().and_then(|m| m.shown.as_ref())
    }

    /// Whether the card shows the members itself (the info bar) or the page shows them below it.
    pub fn set_members_in_card(&mut self, in_card: bool) {
        self.members_in_card = in_card;
    }

    /// Opens the members of `sid` and loads their first page; on the members of `sid` already open it closes them.
    pub fn open_members(&mut self, ctx: &egui::Context, sid: &str) {
        if self.members_sid().is_some_and(|open| open.eq_ignore_ascii_case(sid)) {
            self.members = None;
            return;
        }
        self.members = Some(MembersView { sid: sid.to_string(), shown: None, job: None, more_failed: None });
        let sid = sid.to_string();
        self.members_job(ctx, false, move |paths, net| members::load(paths, net, &sid, false, now()));
    }

    /// Loads the next page of the members open.
    pub fn more_members(&mut self, ctx: &egui::Context) {
        let Some(Some(Ok(found))) = self.members.as_ref().map(|m| m.shown.clone()) else { return };
        if !found.roster.more() || self.members_loading() {
            return;
        }
        let roster = found.roster;
        self.members_job(ctx, true, move |paths, net| members::more(paths, net, &roster));
    }

    /// Asks `ask` in a thread of its own for the members open; without a data folder or the net it says so at once.
    fn members_job(
        &mut self,
        ctx: &egui::Context,
        more: bool,
        ask: impl FnOnce(&Paths, &dyn Net) -> Result<members::Found, Failure> + Send + 'static,
    ) {
        let Some(view) = self.members.as_mut() else { return };
        let (Some(paths), Some(net)) = (&self.paths, &self.net) else {
            if more {
                view.more_failed = Some(Failure::Offline);
            } else {
                view.shown = Some(Err(Failure::Offline));
            }
            return;
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        let own = Paths::new(paths.host.clone());
        let net = Arc::clone(net);
        let lock = versekit_core::writeguard::state();
        let wake = ctx.clone();
        let started = std::thread::Builder::new().name("org-mitglieder".into()).spawn(move || {
            versekit_core::writeguard::adopt(&lock);
            let _ = sender.send(ask(&own, net.as_ref()));
            wake.request_repaint();
        });
        if started.is_ok() {
            view.job = Some((more, receiver));
            view.more_failed = None;
        }
    }

    /// Takes what the thread of the members brought.
    fn tick_members(&mut self) {
        let Some(view) = &mut self.members else { return };
        let Some((more, job)) = &view.job else { return };
        let more = *more;
        match job.try_recv() {
            Ok(answer) => {
                match answer {
                    Ok(found) => view.shown = Some(Ok(found)),
                    Err(failure) if more => view.more_failed = Some(failure),
                    Err(failure) => view.shown = Some(Err(failure)),
                }
                view.job = None;
            }
            Err(TryRecvError::Disconnected) => view.job = None,
            Err(TryRecvError::Empty) => {}
        }
    }

    /// Takes what the thread of the search and the dialogs brought.
    fn tick(&mut self) {
        self.tick_members();
        if let Some((_, job)) = &self.job {
            match job.try_recv() {
                Ok(answer) => {
                    self.show(answer);
                    self.job = None;
                }
                Err(TryRecvError::Disconnected) => self.job = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        if let Some((pending, text)) = &self.export
            && let Some(answer) = pending.take()
        {
            if let Some(path) = answer {
                let written = std::fs::write(&path, text).is_ok();
                let count = Arg::Int(self.relations.list().len() as i64);
                self.message =
                    Some(if written { tf("s_hq_export_ok", &[("n", count)]) } else { t("s_hq_export_fehler") });
            }
            self.export = None;
        }
        if let Some(pending) = &self.import
            && let Some(answer) = pending.take()
        {
            if let Some(path) = answer {
                self.message = Some(match self.relations.import_from(&path) {
                    Ok(count) => {
                        self.save_relations();
                        self.changed |= count > 0;
                        tf("s_hq_import_ok", &[("n", Arg::Int(count as i64))])
                    }
                    Err(ImportError::NotJson | ImportError::WrongForm) => t("s_hq_import_fehler"),
                });
            }
            self.import = None;
        }
    }

    fn save_relations(&self) {
        if let Some(paths) = &self.paths {
            self.relations.save(paths);
        }
    }

    /// Sets the stance of a handle or an organization; the same stance again takes it away.
    pub fn mark(&mut self, kind: Kind, name: &str, stance: Stance) {
        let next = (self.relations.of(kind, name) != Some(stance)).then_some(stance);
        self.set_stance(kind, name, next);
    }

    /// Sets the stance of a handle or an organization, or takes it away with `None`; written at once.
    pub fn set_stance(&mut self, kind: Kind, name: &str, stance: Option<Stance>) {
        if self.relations.set(kind, name, stance) {
            self.save_relations();
            self.changed = true;
        }
        self.shown_at = now();
    }

    /// The stance that counts for the handle of `found`, what it was set on.
    fn stance_of(&self, found: &Found) -> Option<(Stance, Kind, String)> {
        let sids: Vec<&str> = found.card.orgs.shown().map(|o| o.sid.as_str()).collect();
        self.relations.for_handle(&found.card.profile.handle, sids)
    }

    /// The field for the bar: a search line with the magnifier. Enter searches, Ctrl+Enter fetches anew,
    /// Escape closes the card.
    pub fn field(&mut self, ui: &mut Ui, scheme: &Scheme) -> Response {
        self.tick();
        let hint = self.field_hint();
        let had_text = !self.query.trim().is_empty();
        let response = widgets::search_field(ui, &mut self.query, &hint, scheme, FIELD_WIDTH);
        // The field emptied (deleted or its clear mark): the card of the handle goes with it.
        if had_text && self.query.trim().is_empty() {
            self.close();
        }
        self.field_id = Some(response.id);
        self.field_hovered = response.hovered();
        self.follow_history(ui, &response);
        let name = t("s_hq_feld_name");
        response.widget_info(|| WidgetInfo::labeled(Role::SearchInput, true, &name));
        // Enter alone ends the input of the field; with Ctrl the field keeps the cursor.
        let typing = response.has_focus() || response.lost_focus();
        let (enter, command, escape) = ui.input(|i| {
            let enter = i.events.iter().find_map(|event| match event {
                egui::Event::Key { key: egui::Key::Enter, pressed: true, modifiers, .. } => Some(modifiers.command),
                _ => None,
            });
            (enter.is_some(), enter == Some(true), i.key_pressed(egui::Key::Escape))
        });
        if typing && enter && !self.query.trim().is_empty() {
            let query = self.query.clone();
            self.search(ui.ctx(), &query, command);
        } else if typing && escape {
            self.close();
        }
        // No tooltip: over the game it would cover the whole bar; the bar shows the short help below itself
        // ([`HandleSearch::field_hovered`]) and the name stays for screen readers.
        response
    }

    /// The pointer was on the field in the last frame.
    pub fn field_hovered(&self) -> bool {
        self.field_hovered
    }

    /// Opens the handles searched lately while the field is empty and has the cursor. They stay while the mouse is
    /// over them, so that a click on one reaches it although the field gave up the cursor.
    fn follow_history(&mut self, ui: &Ui, field: &Response) {
        let empty = self.query.trim().is_empty();
        let over = self.history_rect.is_some_and(|rect| ui.rect_contains_pointer(rect));
        if field.has_focus() && empty {
            if self.history.is_none() {
                self.history = Some(self.recent_handles());
            }
        } else if !empty || !over {
            self.history = None;
            self.history_rect = None;
        }
    }

    /// The handles kept, the newest first, with the SIDs of their organizations.
    fn recent_handles(&self) -> Vec<(String, Vec<String>)> {
        let Some(paths) = &self.paths else { return Vec::new() };
        cache::read(paths)
            .into_iter()
            .rev()
            .take(HISTORY_MAX)
            .map(|(_, card, _)| {
                let sids = card.orgs.shown().map(|o| o.sid.clone()).collect();
                (card.profile.handle, sids)
            })
            .collect()
    }

    /// Whether the handles searched lately are shown.
    pub fn history_open(&self) -> bool {
        self.history.as_ref().is_some_and(|list| !list.is_empty())
    }

    /// The handles searched lately in a card, each in the colour of its stance; a click searches one.
    fn history_card(&mut self, ui: &mut Ui, scheme: &Scheme) {
        let Some(list) = self.history.clone() else { return };
        ui.set_max_width(CARD_WIDTH);
        let mut send = None;
        let drawn = widgets::card(ui, scheme, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(RichText::new(t("s_hq_verlauf")).small().color(color(scheme.sub)));
            for (handle, sids) in &list {
                let stance = self.relations.for_handle(handle, sids.iter().map(String::as_str)).map(|s| s.0);
                let tone = stance.map_or(scheme.fg, |s| stance_color(scheme, s));
                let label = egui::Label::new(RichText::new(handle).color(color(tone))).truncate();
                let response = ui.add(label.sense(Sense::click()));
                response.widget_info(|| WidgetInfo::labeled(Role::Button, true, handle));
                let hint = tf("s_hq_log_klick", &[("handle", Arg::Text(handle.clone()))]);
                if response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(hint).clicked() {
                    send = Some(handle.clone());
                }
            }
        });
        self.history_rect = Some(drawn.response.rect);
        if let Some(handle) = send {
            self.history_rect = None;
            self.search(ui.ctx(), &handle, false);
        }
    }

    /// Puts the cursor into the field, e.g. for a key that opens the search.
    pub fn focus_field(&self, ctx: &egui::Context) {
        if let Some(id) = self.field_id {
            ctx.memory_mut(|m| m.request_focus(id));
        }
    }

    /// Whether the field has the cursor: the window around it then needs the keyboard.
    pub fn typing(&self, ctx: &egui::Context) -> bool {
        self.field_id.is_some_and(|id| ctx.memory(|m| m.has_focus(id)))
    }

    /// Takes the cursor out of the field.
    pub fn stop_typing(&self, ctx: &egui::Context) {
        if let Some(id) = self.field_id {
            ctx.memory_mut(|m| m.surrender_focus(id));
        }
    }

    /// The card of the handle found, or why there is none; nothing before the first search.
    pub fn card(&mut self, ui: &mut Ui, scheme: &Scheme) {
        self.tick();
        if self.history_open() {
            self.history_card(ui, scheme);
            return;
        }
        if let Some((handle, _)) = &self.job {
            widgets::loading(ui, scheme, &tf("s_hq_sucht", &[("handle", Arg::Text(handle.clone()))]));
            return;
        }
        if let Some(left) = self.expire(now(), self.typing(ui.ctx())) {
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(left.max(0.1)));
        }
        let Some(shown) = self.shown.clone() else { return };
        ui.set_max_width(CARD_WIDTH);
        let mut close = false;
        widgets::card(ui, scheme, |ui| {
            ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                // The text beside the picture leaves the place of the close button free.
                let gap = ui.spacing().item_spacing.x;
                let text_width = ui.available_width() - widgets::ICON_BUTTON_SIZE - gap;
                match &shown {
                    Ok(found) => self.head(ui, scheme, found, text_width),
                    Err(failure) => {
                        ui.allocate_ui_with_layout(vec2(text_width, 0.0), Layout::top_down(Align::Min), |ui| {
                            ui.set_width(text_width);
                            ui.label(RichText::new(failure_text(failure)).color(color(scheme.yellow)));
                        });
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                    close = widgets::icon_button(ui, scheme, "x", &t("s_hq_schliessen")).clicked();
                });
            });
            if let Ok(found) = &shown {
                self.body(ui, scheme, found);
                if self.members_in_card && self.members.is_some() {
                    ui.add(egui::Separator::default().spacing(6.0));
                    self.members(ui, scheme, false);
                }
            }
        });
        // The mouse over the card reads it: the time counts anew.
        if ui.ui_contains_pointer() {
            self.shown_at = now();
        }
        if close {
            self.close();
        }
    }

    /// The profile picture, beside it handle, name, title, number, date of enlisting and languages; the handle in
    /// the colour of the stance that counts. The text takes at most `width` with the picture.
    fn head(&mut self, ui: &mut Ui, scheme: &Scheme, found: &Found, width: f32) {
        let profile = &found.card.profile;
        let stance = self.stance_of(found);
        let tone = stance.as_ref().map_or(scheme.fg, |(s, _, _)| stance_color(scheme, *s));
        let body = egui::TextStyle::Body.resolve(ui.style()).size;
        let (big, bold) = (crate::fonts::bold(body * NAME_SCALE), crate::fonts::bold(body));
        self.pictures.show(ui, scheme, &profile.image, AVATAR_SIZE);
        let gap = ui.spacing().item_spacing.x;
        let width = (width - AVATAR_SIZE - gap).max(1.0);
        let right = (width * RIGHT_SHARE).floor();
        let middle = (width - right - gap).max(1.0);
        ui.allocate_ui_with_layout(vec2(middle, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(middle);
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.horizontal_wrapped(|ui| {
                if let Some((stance, _, _)) = &stance
                    && let Some(image) = crate::icons::image(stance_icon(*stance), tone, MARK_SIZE)
                {
                    ui.add(image);
                }
                ui.label(RichText::new(&profile.handle).font(big.clone()).color(color(tone)));
            });
            if !profile.name.is_empty() {
                ui.label(RichText::new(tf("s_hq_cm", &[("name", Arg::Text(profile.name.clone()))])).font(bold.clone()));
            }
            if !profile.title.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    if !profile.title_image.is_empty() {
                        self.pictures.show(ui, scheme, &profile.title_image, TITLE_SIZE);
                    }
                    ui.label(&profile.title);
                });
            }
            if !profile.fluency.is_empty() {
                ui.label(tf("s_hq_sprachen", &[("liste", Arg::Text(profile.fluency.join(", ")))]));
            }
            if let Some((stance, kind, on)) = &stance {
                let text =
                    tf("s_hq_markiert", &[("haltung", Arg::Text(t(stance.label()))), ("ziel", Arg::Text(on.clone()))]);
                if *kind == Kind::Org {
                    ui.label(RichText::new(text).small().color(color(tone)));
                }
            }
        });
        ui.allocate_ui_with_layout(vec2(right, 0.0), Layout::top_down(Align::Max), |ui| {
            ui.set_width(right);
            ui.spacing_mut().item_spacing.y = 1.0;
            if !profile.record.is_empty() {
                ui.label(RichText::new(&profile.record).font(big.clone()));
            }
            if !profile.enlisted.is_empty() {
                ui.label(RichText::new(&profile.enlisted).font(bold.clone()));
            }
        });
    }

    /// The organizations, the age of the data, the links and the marks.
    fn body(&mut self, ui: &mut Ui, scheme: &Scheme, found: &Found) {
        let orgs = &found.card.orgs;
        let line = |ui: &mut Ui| {
            ui.add(egui::Separator::default().spacing(6.0));
        };
        let hidden = |ui: &mut Ui, key: &str| {
            ui.label(RichText::new(t(key)).color(color(scheme.sub)));
        };
        line(ui);
        match &orgs.main {
            Some(Membership::Shown(org)) => self.org_row(ui, scheme, org, true),
            Some(Membership::Hidden) => hidden(ui, "s_hq_verborgen"),
            None => hidden(ui, "s_hq_keine_org"),
        }
        for membership in self.settings.affiliations(orgs) {
            line(ui);
            match membership {
                Membership::Shown(org) => self.org_row(ui, scheme, org, false),
                Membership::Hidden => hidden(ui, "s_hq_verborgen"),
            }
        }
        line(ui);
        let age = if found.live {
            t("s_hq_frisch")
        } else {
            tf("s_hq_stand", &[("alter", Arg::Text(language::relative_age(now() - found.fetched)))])
        };
        ui.label(RichText::new(age).small().color(color(scheme.sub)));
        if let Some(failure) = &found.failed {
            ui.label(RichText::new(failure_text(failure)).small().color(color(scheme.yellow)));
        }
        if widgets::external_link(ui, scheme, &t("s_hq_profil")).clicked() {
            (self.open_url)(&found.card.url());
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new(t("s_hq_handle_markieren")).small().color(color(scheme.sub)));
            let handle = found.card.profile.handle.clone();
            self.marks(ui, scheme, Kind::Handle, &handle);
        });
    }

    /// One organization: its logo, beside it the name, the SID, the rank and the rank stars; on the right its marks
    /// and what it does, at the bottom right how much it asks (or that it is an affiliation) and its members.
    /// A click on the name opens its members, on the members open it closes them.
    fn org_row(&mut self, ui: &mut Ui, scheme: &Scheme, org: &Org, main: bool) {
        let open = self.members_sid().is_some_and(|sid| sid.eq_ignore_ascii_case(&org.sid));
        let toggle = if open { t("s_hqm_zu") } else { tf("s_hqm_auf", &[("sid", Arg::Text(org.sid.clone()))]) };
        let mut clicked = false;
        let stance = self.relations.of(Kind::Org, &org.sid);
        let tone = stance.map_or(scheme.fg, |s| stance_color(scheme, s));
        let body = egui::TextStyle::Body.resolve(ui.style()).size;
        let (big, bold) = (crate::fonts::bold(body * NAME_SCALE), crate::fonts::bold(body));
        let kind = if !main {
            t("s_hq_partner")
        } else if org.commitment.is_empty() {
            t("s_hq_haupt")
        } else {
            org.commitment.clone()
        };
        let foot = match org.members {
            Some(members) => format!("{kind} / {}", tf("s_hq_mitglieder", &[("n", Arg::Int(members as i64))])),
            None => kind,
        };
        ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
            self.pictures.show(ui, scheme, &org.logo, LOGO_SIZE);
            let gap = ui.spacing().item_spacing.x;
            let width = ui.available_width();
            let right = (width * RIGHT_SHARE).floor();
            let middle = (width - right - gap).max(1.0);
            ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                    ui.allocate_ui_with_layout(vec2(middle, 0.0), Layout::top_down(Align::Min), |ui| {
                        ui.set_width(middle);
                        ui.spacing_mut().item_spacing.y = 1.0;
                        let mut name = RichText::new(&org.name).font(big.clone()).color(color(tone));
                        if open {
                            name = name.underline();
                        }
                        let response = ui.add(egui::Label::new(name).sense(Sense::click()));
                        response.widget_info(|| WidgetInfo::selected(Role::Button, true, open, &org.name));
                        clicked =
                            response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(&toggle).clicked();
                        ui.label(RichText::new(tf("s_hq_sid", &[("sid", Arg::Text(org.sid.clone()))])).font(bold));
                        if !org.rank.is_empty() {
                            ui.label(&org.rank);
                        }
                    });
                    ui.allocate_ui_with_layout(vec2(right, 0.0), Layout::top_down(Align::Max), |ui| {
                        ui.set_width(right);
                        ui.spacing_mut().item_spacing.y = 1.0;
                        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                            self.marks(ui, scheme, Kind::Org, &org.sid);
                        });
                        for text in [&org.activity, &org.second_activity] {
                            if !text.is_empty() {
                                ui.label(text);
                            }
                        }
                    });
                });
                // The stars on the left, the line of the members on the right; below the stars when it is too wide.
                let star_width = 5.0 * STAR_SIZE + 4.0 * STAR_GAP;
                let foot_text = RichText::new(foot).small().color(color(scheme.sub));
                let foot_width = egui::WidgetText::from(foot_text.clone())
                    .into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Small)
                    .size()
                    .x;
                if star_width + gap + foot_width <= width {
                    ui.horizontal(|ui| {
                        stars(ui, scheme, org.stars);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(foot_text);
                        });
                    });
                } else {
                    stars(ui, scheme, org.stars);
                    ui.with_layout(Layout::top_down(Align::Max), |ui| {
                        ui.label(foot_text);
                    });
                }
            });
        });
        if clicked {
            self.open_members(ui.ctx(), &org.sid);
        }
    }

    /// The members open: a head with the organization and a button that closes them, the members in a list that
    /// scrolls past a height, below it how many are loaded, a button for the next page and the page at RSI.
    /// `wide` draws them for the page: higher, with the marks of each handle. A click on a member searches it.
    pub fn members(&mut self, ui: &mut Ui, scheme: &Scheme, wide: bool) {
        self.tick();
        let Some(view) = &self.members else { return };
        let sid = view.sid.clone();
        let shown = view.shown.clone();
        let (loading_first, loading_more) = match &view.job {
            Some((more, _)) => (!*more, *more),
            None => (false, false),
        };
        let more_failed = view.more_failed.clone();
        let mut close = false;
        ui.horizontal(|ui| {
            let body = egui::TextStyle::Body.resolve(ui.style()).size;
            let title =
                RichText::new(tf("s_hqm_titel", &[("sid", Arg::Text(sid.clone()))])).font(crate::fonts::bold(body));
            let width = (ui.available_width() - widgets::ICON_BUTTON_SIZE - ui.spacing().item_spacing.x).max(1.0);
            cell(ui, width, widgets::ICON_BUTTON_SIZE, |ui| {
                ui.add(egui::Label::new(title).truncate());
            });
            close = widgets::icon_button(ui, scheme, "x", &t("s_hqm_zu")).clicked();
        });
        if close {
            self.members = None;
            return;
        }
        if loading_first {
            widgets::loading(ui, scheme, &tf("s_hqm_laedt", &[("sid", Arg::Text(sid))]));
            return;
        }
        let found = match shown {
            Some(Ok(found)) => found,
            Some(Err(failure)) => {
                ui.label(RichText::new(failure_text(&failure)).color(color(scheme.yellow)));
                if widgets::external_link(ui, scheme, &t("s_hqm_rsi")).clicked() {
                    (self.open_url)(&page::members_url(&sid));
                }
                return;
            }
            None => return,
        };
        let roster = &found.roster;
        if roster.members.is_empty() {
            widgets::hint(ui, scheme, &t("s_hqm_leer"));
        } else {
            let height = if wide { MEMBERS_HEIGHT_PAGE } else { MEMBERS_HEIGHT };
            let mut send = None;
            egui::ScrollArea::vertical()
                .id_salt(("org-mitglieder", wide))
                .max_height(height)
                .min_scrolled_height(height)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    for member in &roster.members {
                        if let Some(handle) = self.member_row(ui, scheme, member, &roster.sid, wide) {
                            send = Some(handle);
                        }
                    }
                });
            if let Some(handle) = send {
                self.search(ui.ctx(), &handle, false);
            }
        }
        let count = match roster.total {
            Some(total) => {
                tf("s_hqm_anzahl", &[("n", Arg::Int(roster.members.len() as i64)), ("gesamt", Arg::Int(total as i64))])
            }
            None => tf("s_hqm_anzahl_ohne", &[("n", Arg::Int(roster.members.len() as i64))]),
        };
        let age = if found.live {
            t("s_hq_frisch")
        } else {
            tf("s_hq_stand", &[("alter", Arg::Text(language::relative_age(now() - roster.fetched)))])
        };
        ui.label(RichText::new(format!("{count} · {age}")).small().color(color(scheme.sub)));
        for failure in [&found.failed, &more_failed].into_iter().flatten() {
            ui.label(RichText::new(failure_text(failure)).small().color(color(scheme.yellow)));
        }
        if loading_more {
            widgets::loading(ui, scheme, &t("s_hqm_laedt_mehr"));
        } else if roster.more() {
            if ui.add(widgets::Button::new(&t("s_hqm_mehr"), ButtonKind::Normal, scheme)).clicked() {
                self.more_members(ui.ctx());
            }
        } else if !roster.end {
            widgets::hint(ui, scheme, &t("s_hqm_grenze"));
        }
        if widgets::external_link(ui, scheme, &t("s_hqm_rsi")).clicked() {
            (self.open_url)(&page::members_url(&roster.sid));
        }
    }

    /// One member: the handle in the colour of its stance (or that it is hidden), the rank and the stars; on the
    /// page also the marks of the handle. The handle clicked, if it was.
    fn member_row(&mut self, ui: &mut Ui, scheme: &Scheme, member: &Member, sid: &str, wide: bool) -> Option<String> {
        let gap = ui.spacing().item_spacing.x;
        let star_width = 5.0 * STAR_SIZE + 4.0 * STAR_GAP;
        let marks = if wide && !member.hidden() { 3.0 * (MARK_SIZE + gap) } else { 0.0 };
        let rest = (ui.available_width() - star_width - marks - 3.0 * gap).max(2.0);
        let handle_width = (rest * MEMBER_HANDLE_SHARE).floor().max(1.0);
        let rank_width = (rest - handle_width).max(1.0);
        let height = MARK_SIZE.max(ui.text_style_height(&egui::TextStyle::Body));
        let mut send = None;
        ui.horizontal(|row| {
            cell(row, handle_width, height, |c| {
                if member.hidden() {
                    let text = RichText::new(t("s_hqm_verborgen")).italics().color(color(scheme.sub));
                    c.add(egui::Label::new(text).truncate());
                    return;
                }
                let stance = self.relations.for_handle(&member.handle, [sid]).map(|s| s.0);
                let tone = stance.map_or(scheme.fg, |s| stance_color(scheme, s));
                let label = egui::Label::new(RichText::new(&member.handle).strong().color(color(tone))).truncate();
                let response = c.add(label.sense(Sense::click()));
                response.widget_info(|| WidgetInfo::labeled(Role::Button, true, &member.handle));
                let hint = tf("s_hq_log_klick", &[("handle", Arg::Text(member.handle.clone()))]);
                if response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(hint).clicked() {
                    send = Some(member.handle.clone());
                }
            });
            cell(row, rank_width, height, |c| {
                c.add(egui::Label::new(RichText::new(&member.rank).small().color(color(scheme.sub))).truncate());
            });
            stars(row, scheme, member.stars);
            if wide && !member.hidden() {
                self.marks(row, scheme, Kind::Handle, &member.handle);
            }
        });
        send
    }

    /// The three marks of a stance for `name`; the one set in its colour. A click sets that stance, on the one
    /// set it takes it away.
    pub fn marks(&mut self, ui: &mut Ui, scheme: &Scheme, kind: Kind, name: &str) {
        let set = self.relations.of(kind, name);
        for stance in Stance::ALL {
            let on = set == Some(stance);
            let tone = if on { stance_color(scheme, stance) } else { scheme.sub };
            let meaning = if on {
                t("s_hq_markierung_weg")
            } else {
                let what = match kind {
                    Kind::Handle => "s_hq_handle_als",
                    Kind::Org => "s_hq_org_als",
                };
                tf(what, &[("haltung", Arg::Text(t(stance.label()))), ("org", Arg::Text(name.to_string()))])
            };
            let Some(image) = crate::icons::image(stance_icon(stance), tone, MARK_SIZE) else { continue };
            let response = ui.add(image.sense(Sense::click()));
            response.widget_info(|| WidgetInfo::selected(Role::CheckBox, true, on, &meaning));
            if response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(&meaning).clicked() {
                self.mark(kind, name, stance);
            }
        }
    }

    /// Takes what the reader of the log read at `now`.
    pub fn take_log(&mut self, text: &str, now: f64) -> bool {
        self.log.on && self.monitor.take(text, now)
    }

    /// A new session of the game: the lines of the old one go.
    pub fn clear_log(&mut self) {
        self.monitor.clear();
    }

    /// Whether the log monitor has lines to show at `now`.
    pub fn has_log_lines(&self, now: f64) -> bool {
        self.log.on && self.monitor.shown(now, self.log.seconds).next().is_some()
    }

    /// Whether there is something to show below the field: a card, a search running.
    pub fn has_card(&self) -> bool {
        self.shown.is_some() || self.job.is_some() || self.history_open()
    }

    /// Takes the settings anew, e.g. after they changed in the main window.
    pub fn reload_settings(&mut self) {
        if let Some(paths) = &self.paths {
            self.settings = Settings::read(paths);
            self.log = log_monitor::Settings::read(paths);
            self.relations = Relations::load(paths);
        }
    }

    /// The lines of the log monitor still shown at `now`; a click on one sends its handle to the search. Nothing
    /// when the monitor is off or empty.
    pub fn log_lines(&mut self, ui: &mut Ui, scheme: &Scheme, now: f64) {
        if !self.log.on {
            return;
        }
        let hold = self.log.seconds;
        self.monitor.forget_old(now, hold);
        let lang = language::current();
        let mut send = None;
        for entry in self.monitor.shown(now, hold) {
            let handle = entry.handle(self.own.as_deref());
            let stance = handle.and_then(|h| self.relations.of(Kind::Handle, h));
            let tone = stance.map_or(scheme.fg, |s| stance_color(scheme, s));
            let text = entry.text(lang);
            let label = egui::Label::new(RichText::new(&text).color(color(tone))).truncate();
            match handle {
                Some(handle) => {
                    let hint = tf("s_hq_log_klick", &[("handle", Arg::Text(handle.to_string()))]);
                    let response = ui.add(label.sense(Sense::click()));
                    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, &text));
                    if response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(hint).clicked() {
                        send = Some(handle.to_string());
                    }
                }
                None => {
                    ui.add(label);
                }
            }
        }
        // The next line to go: a frame then, so that it leaves on time.
        if let Some(next) = self.monitor.shown(now, hold).map(|e| e.taken + hold as f64 - now).reduce(f64::min) {
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(next.max(0.1)));
        }
        if let Some(handle) = send {
            self.search(ui.ctx(), &handle, false);
        }
    }

    /// The settings of the search and the log monitor, with export and import of the relations. Every change is
    /// written at once; without a data folder (the preview) it is only kept here.
    pub fn settings(&mut self, ui: &mut Ui, scheme: &Scheme) {
        self.tick();
        let mut written: Vec<(&'static str, Value)> = Vec::new();
        let mut days = self.settings.days;
        ui.horizontal(|ui| {
            ui.label(t("s_hq_einst_tage"));
            if widgets::number_field(ui, scheme, "handle-tage", &mut days, 0..=cache::DAYS_MAX, "") {
                written.push((KEY_DAYS, Value::int(days)));
            }
        });
        widgets::hint(ui, scheme, &t("s_hq_einst_tage_h"));
        // The main organization counts: one organization shown is no affiliation.
        let mut count = self.settings.affiliations as i64;
        ui.horizontal_wrapped(|ui| {
            ui.label(t("s_hq_einst_aff"));
            for affiliations in 0..=cache::AFFILIATIONS_MAX {
                let shown = (affiliations + 1).to_string();
                if widgets::radio(ui, scheme, count == affiliations, &shown).clicked() && count != affiliations {
                    count = affiliations;
                    written.push((KEY_AFFILIATIONS, Value::int(count)));
                }
            }
        });
        let mut hide = self.settings.hide_hidden;
        if widgets::switch_row(ui, scheme, &t("s_hq_einst_verborgen"), &mut hide).changed() {
            written.push((KEY_HIDE_HIDDEN, Value::Bool(hide)));
        }
        let mut clear_after = self.settings.clear_after;
        ui.horizontal(|ui| {
            ui.label(t("s_hq_einst_leeren"));
            if widgets::number_field(ui, scheme, "handle-leeren", &mut clear_after, 0..=cache::CLEAR_AFTER_MAX, "s") {
                written.push((KEY_CLEAR_AFTER, Value::int(clear_after)));
            }
        });
        widgets::hint(ui, scheme, &t("s_hq_einst_leeren_h"));
        let mut on = self.log.on;
        if widgets::switch_row(ui, scheme, &t("s_hq_einst_log"), &mut on).changed() {
            written.push((log_monitor::KEY_ON, Value::Bool(on)));
        }
        let mut seconds = self.log.seconds;
        ui.horizontal(|ui| {
            ui.label(t("s_hq_einst_log_sek"));
            let range = log_monitor::SECONDS_MIN..=log_monitor::SECONDS_MAX;
            if widgets::number_field(ui, scheme, "logmonitor-sek", &mut seconds, range, "s") {
                written.push((log_monitor::KEY_SECONDS, Value::int(seconds)));
            }
        });
        if !written.is_empty() {
            self.changed = true;
            match self.paths.as_ref() {
                Some(paths) => {
                    for (key, value) in written {
                        paths.set_setting(key, value);
                    }
                    self.settings = Settings::read(paths);
                    self.log = log_monitor::Settings::read(paths);
                }
                None => {
                    self.settings =
                        Settings { days, affiliations: count.max(0) as usize, hide_hidden: hide, clear_after };
                    self.log = log_monitor::Settings { on, seconds };
                }
            }
        }
        let paths = self.paths.as_ref().map(|p| Paths::new(p.host.clone()));
        let (clear, export, import) = (t("s_hq_cache_leeren"), t("s_hq_export"), t("s_hq_import"));
        let filters: file_dialog::Filters = &[("JSON", &["json"])];
        ui.horizontal_wrapped(|ui| {
            if ui.add(widgets::Button::new(&clear, ButtonKind::Normal, scheme)).clicked()
                && let Some(paths) = &paths
            {
                cache::clear(paths);
                members::clear(paths);
            }
            if ui.add(widgets::Button::new(&export, ButtonKind::Normal, scheme)).clicked() && self.export.is_none() {
                let pending = file_dialog::save_file(ui.ctx(), &export, filters, None, "versekit-beziehungen.json");
                self.export = Some((pending, self.relations.export_text()));
            }
            if ui.add(widgets::Button::new(&import, ButtonKind::Normal, scheme)).clicked() && self.import.is_none() {
                self.import = Some(file_dialog::open_file(ui.ctx(), &import, filters, None));
            }
        });
        if let Some(message) = &self.message {
            widgets::hint(ui, scheme, message);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;
    use versekit_core::handle_query::page::{orgs_url, profile_url};
    use versekit_core::language::{Choice, set_language};
    use versekit_core::net::Fake;

    static OPENED: Mutex<Vec<String>> = Mutex::new(Vec::new());

    fn record(url: &str) -> bool {
        OPENED.lock().map(|mut list| list.push(url.to_string())).is_ok()
    }

    /// Emptying the field takes the card of the handle away with it.
    #[test]
    fn emptying_the_field_closes_the_card() {
        let ctx = context();
        let mut search = HandleSearch::empty();
        search.query = "a/b".into();
        search.search(&ctx, "a/b", false);
        assert!(search.shown().is_some(), "a card stands");
        frame(&ctx, &mut search, Vec::new(), egui::Modifiers::NONE);
        search.focus_field(&ctx);
        frame(&ctx, &mut search, Vec::new(), egui::Modifiers::NONE);
        let key = |key, modifiers| egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers };
        let wipe = vec![key(egui::Key::A, egui::Modifiers::COMMAND), key(egui::Key::Backspace, egui::Modifiers::NONE)];
        frame(&ctx, &mut search, wipe, egui::Modifiers::NONE);
        assert!(search.query.is_empty(), "the field is empty: {:?}", search.query);
        assert!(search.shown().is_none(), "the card went with the text");
    }

    #[test]
    fn the_card_and_the_field_empty_themselves_after_the_seconds_set() {
        let ctx = egui::Context::default();
        let mut search = HandleSearch::empty();
        search.query = "a/b".into();
        search.search(&ctx, "a/b", false);
        assert!(search.shown().is_some(), "an invalid handle shows its card");
        let start = search.shown_at;
        // Typing keeps the card and counts anew from then.
        assert!(search.expire(start + 100.0, true).is_some());
        let left = search.expire(start + 129.0, false).expect("still there");
        assert!((left - 1.0).abs() < 1e-6, "{left}");
        assert_eq!(search.expire(start + 130.0, false), None);
        assert!(search.shown().is_none() && search.query.is_empty(), "card and field are empty");
        // Counter-check: with 0 the card stays.
        let mut kept = HandleSearch::empty();
        kept.settings.clear_after = 0;
        kept.search(&ctx, "a/b", false);
        assert_eq!(kept.expire(kept.shown_at + 1e6, false), None);
        assert!(kept.shown().is_some());
    }

    const PROFILE: &str = r#"<div class="profile left-col"><div class="thumb"><img src="/media/b.png" /></div>
        <div class="info"><p class="entry"><strong class="value">Muster</strong></p>
        <p class="entry"><span class="label">Handle name</span><strong class="value">Musterpilot</strong></p></div></div>"#;
    const ORGS: &str = r#"<div class="box-content org main visibility-V"><a href="/orgs/TEST"><img src="/media/l.png" /></a>
        <span class="members">12 members</span><a href="/orgs/TEST" class="value">Testgeschwader</a>
        <p class="entry"><span class="label">Organization rank</span><strong class="value">Pilot</strong></p></div>
        <div class="box-content org affiliation visibility-R"></div>"#;

    fn paths(dir: &versekit_core::testdir::Dir) -> Paths {
        let home = dir.path().to_string_lossy().into_owned();
        Paths::new(versekit_core::paths::Host::new(
            versekit_core::pypath::Flavor::native(),
            &[(versekit_core::legacy::ENV_HOME, home.as_str())],
            &home,
        ))
    }

    fn search_with_site(dir: &versekit_core::testdir::Dir) -> HandleSearch {
        let net = Fake::default().with(&profile_url("Musterpilot"), PROFILE).with(&orgs_url("Musterpilot"), ORGS);
        HandleSearch::with_net(&paths(dir), Some(Arc::new(net))).with_opener(record)
    }

    /// One frame with `events`; the accesskit nodes with role, label and the middle of their place.
    fn frame(
        ctx: &egui::Context,
        search: &mut HandleSearch,
        mut events: Vec<egui::Event>,
        modifiers: egui::Modifiers,
    ) -> Vec<(Role, String, egui::Pos2)> {
        let scheme = versekit_core::theme::current();
        events.insert(0, egui::Event::ModifiersChanged(modifiers));
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600.0, 800.0))),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            search.field(ui, scheme);
            search.card(ui, scheme);
            search.log_lines(ui, scheme, 100.0);
        });
        output.textures_delta.clear();
        let Some(update) = output.platform_output.accesskit_update else { return Vec::new() };
        update
            .nodes
            .iter()
            .filter_map(|(_, n)| {
                let text = n.label().or(n.value())?.to_owned();
                let b = n.bounds()?;
                Some((n.role(), text, egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32)))
            })
            .collect()
    }

    fn click(at: egui::Pos2) -> Vec<egui::Event> {
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        };
        vec![egui::Event::PointerMoved(at), press(true), press(false)]
    }

    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        ctx
    }

    /// Waits for the thread of the search and takes its answer. The pause between two pages of the site holds in
    /// the tests too and is shared by the tests running at the same time: up to half a minute.
    fn settle(ctx: &egui::Context, search: &mut HandleSearch) {
        for _ in 0..3000 {
            if !search.searching() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
            frame(ctx, search, Vec::new(), egui::Modifiers::NONE);
        }
        panic!("the search did not end");
    }

    fn german() -> std::sync::MutexGuard<'static, ()> {
        let guard = crate::language_lock();
        set_language(Choice::De);
        guard
    }

    #[test]
    fn a_handle_searched_shows_its_card_and_the_mark_colours_it() {
        let _lang = german();
        let dir = versekit_core::testdir::Dir::new("handle-suche-karte");
        let mut search = search_with_site(&dir);
        let ctx = context();
        search.search(&ctx, " Musterpilot ", false);
        settle(&ctx, &mut search);
        let Some(Ok(found)) = search.shown().cloned() else { panic!("found: {:?}", search.shown()) };
        assert!(found.live);
        assert_eq!(found.card.profile.name, "Muster");
        frame(&ctx, &mut search, Vec::new(), egui::Modifiers::NONE);
        let nodes = frame(&ctx, &mut search, Vec::new(), egui::Modifiers::NONE);
        let has = |text: &str| nodes.iter().any(|(_, label, _)| label == text);
        let sid = tf("s_hq_sid", &[("sid", Arg::Text("TEST".into()))]);
        assert!(has("Musterpilot") && has(&sid) && has("Pilot") && has("Testgeschwader"), "{nodes:?}");
        assert!(has(&t("s_hq_verborgen")), "the redacted affiliation shows: {nodes:?}");
        // A click on the hostile mark of the organization counts for the handle and is kept.
        let meaning =
            tf("s_hq_org_als", &[("haltung", Arg::Text(t("s_hq_feindlich"))), ("org", Arg::Text("TEST".into()))]);
        let mark =
            nodes.iter().find(|(role, label, _)| *role == Role::CheckBox && *label == meaning).expect("the mark");
        frame(&ctx, &mut search, click(mark.2), egui::Modifiers::NONE);
        assert_eq!(search.stance_of(&found).map(|s| s.0), Some(Stance::Hostile));
        assert_eq!(Relations::load(&paths(&dir)).of(Kind::Org, "test"), Some(Stance::Hostile));
        // The same mark again takes it away.
        search.mark(Kind::Org, "TEST", Stance::Hostile);
        assert_eq!(search.stance_of(&found), None);
        // The link of the profile opens the page of the handle.
        let link =
            nodes.iter().find(|(role, label, _)| *role == Role::Link && *label == t("s_hq_profil")).expect("link");
        frame(&ctx, &mut search, click(link.2), egui::Modifiers::NONE);
        assert!(OPENED.lock().map(|l| l.iter().any(|u| u.ends_with("/citizens/Musterpilot"))).unwrap_or(false));
    }

    const RICH_PROFILE: &str = r#"<p class="entry citizen-record"><span class="label">UEE Citizen Record</span>
        <strong class="value">#100001</strong></p>
        <div class="profile left-col"><div class="thumb"><img src="/media/b.png" /></div>
        <div class="info"><p class="entry"><strong class="value">Muster</strong></p>
        <p class="entry"><span class="label">Handle name</span><strong class="value">Musterpilot</strong></p></div></div>
        <p class="entry"><span class="label">Enlisted</span><strong class="value">Apr 8, 2015</strong></p>
        <p class="entry"><span class="label">Fluency</span><strong class="value"> German,   English </strong></p>"#;
    const RICH_ORGS: &str = r#"<div class="box-content org main visibility-V"><a href="/orgs/TEST"><img src="/media/l.png" /></a>
        <span class="members">3724 members</span><a href="/orgs/TEST" class="value">Testgeschwader</a>
        <p class="entry"><span class="label">Organization rank</span><strong class="value">Pilot</strong></p>
        <div class="ranking"><span class="active"></span><span class="active"></span><span class="active"></span>
        <span ></span><span ></span></div>
        <p class="entry"><span class="label">Prim. Activity</span><strong class="value">Security</strong></p>
        <p class="entry"><span class="label">Sec. Activity</span><strong class="value">Trading</strong></p>
        <p class="entry"><span class="label">Commitment</span><strong class="value">Regular</strong></p></div>
        <div class="box-content org affiliation visibility-V"><a href="/orgs/ZWEI"><img src="/media/z.png" /></a>
        <span class="members">5010 members</span><a href="/orgs/ZWEI" class="value">Zweite</a></div>"#;
    pub(crate) const LOGO_URL: &str = "https://robertsspaceindustries.com/media/l.png";

    /// The site with a card full of entries and its pictures: the profile picture and the logo of the main
    /// organization; the logo of the affiliation is missing.
    /// The handle is one of its own, so that tests running at the same time never find it busy.
    pub(crate) fn rich_site(handle: &str) -> Fake {
        let picture = crate::handle_pictures::tests::png(40, 40);
        Fake::default()
            .with(&profile_url(handle), RICH_PROFILE.replace("Musterpilot", handle))
            .with(&orgs_url(handle), RICH_ORGS)
            .with("https://robertsspaceindustries.com/media/b.png", picture.clone())
            .with(LOGO_URL, picture)
    }

    /// The boxes of `size` points in the shapes of a frame: (placeholders, pictures).
    pub(crate) fn boxes(shapes: &[egui::epaint::ClippedShape], size: f32) -> (usize, usize) {
        let scheme = versekit_core::theme::current();
        let square = |r: egui::Rect| (r.width() - size).abs() < 0.5 && (r.height() - size).abs() < 0.5;
        let rects = || {
            shapes.iter().filter_map(|c| match &c.shape {
                egui::Shape::Rect(rect) if square(rect.rect) => Some(rect),
                _ => None,
            })
        };
        let empty = rects()
            .filter(|r| r.fill_texture_id() == egui::TextureId::default() && r.fill == color(scheme.field))
            .count();
        let pictures = rects().filter(|r| r.fill_texture_id() != egui::TextureId::default()).count();
        (empty, pictures)
    }

    /// A site whose pictures wait until they are let through.
    struct Held {
        site: Fake,
        open: Arc<std::sync::atomic::AtomicBool>,
    }

    impl Net for Held {
        fn get(&self, url: &str) -> Result<Vec<u8>, versekit_core::net::FetchError> {
            while url.contains("/media/") && !self.open.load(std::sync::atomic::Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            self.site.get(url)
        }

        fn head(&self, url: &str) -> Result<versekit_core::net::Headers, versekit_core::net::FetchError> {
            self.site.head(url)
        }
    }

    #[test]
    fn the_card_shows_logos_once_they_came_with_stars_activities_and_members() {
        let _lang = german();
        let dir = versekit_core::testdir::Dir::new("handle-suche-bilder");
        let open = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let net = Held { site: rich_site("Bildpilot"), open: Arc::clone(&open) };
        let mut search = HandleSearch::with_net(&paths(&dir), Some(Arc::new(net))).with_opener(record);
        let ctx = context();
        search.search(&ctx, "Bildpilot", false);
        settle(&ctx, &mut search);
        let scheme = versekit_core::theme::current();
        let shapes = |ctx: &egui::Context, search: &mut HandleSearch| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600.0, 800.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| search.card(ui, scheme));
            output.textures_delta.clear();
            output.shapes
        };
        // The first frame asks for the pictures: quiet boxes in their place.
        let first = shapes(&ctx, &mut search);
        assert_eq!(AVATAR_SIZE, LOGO_SIZE, "profile picture and logos are the same size");
        assert_eq!(boxes(&first, LOGO_SIZE), (3, 0), "the profile picture and two logos waiting");
        assert!(search.pictures_waiting());
        open.store(true, std::sync::atomic::Ordering::SeqCst);
        for _ in 0..500 {
            shapes(&ctx, &mut search);
            if !search.pictures_waiting() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!search.pictures_waiting(), "the pictures came");
        let later = shapes(&ctx, &mut search);
        assert_eq!(boxes(&later, LOGO_SIZE), (1, 2), "picture and logo shown, the missing logo stays a quiet box");
        let picture = std::path::Path::new(&versekit_core::handle_query::pictures::folder(&paths(&dir)))
            .join(versekit_core::handle_query::pictures::file_name(LOGO_URL));
        assert!(picture.is_file(), "the logo is kept on disk");
        // Three of five stars filled, in the colour of the text.
        let filled = later
            .iter()
            .filter(|c| matches!(&c.shape, egui::Shape::Circle(circle)
                if (circle.radius - STAR_SIZE / 2.0).abs() < 0.01 && circle.fill == color(scheme.fg)))
            .count();
        assert_eq!(filled, 3);
        let nodes = frame(&ctx, &mut search, Vec::new(), egui::Modifiers::NONE);
        let has = |text: &str| nodes.iter().any(|(_, label, _)| label == text);
        let members = |n: i64| tf("s_hq_mitglieder", &[("n", Arg::Int(n))]);
        assert!(has(&tf("s_hq_sterne", &[("n", Arg::Int(3))])), "{nodes:?}");
        assert!(has("Security") && has("Trading"), "{nodes:?}");
        assert!(has(&format!("Regular / {}", members(3724))), "the main one: {nodes:?}");
        assert!(has(&format!("{} / {}", t("s_hq_partner"), members(5010))), "the affiliation: {nodes:?}");
        assert!(has("#100001") && has("Apr 8, 2015") && has(&tf("s_hq_cm", &[("name", Arg::Text("Muster".into()))])));
        assert!(has(&tf("s_hq_sprachen", &[("liste", Arg::Text("German, English".into()))])), "{nodes:?}");
    }

    /// An answer of the list of members: on the first page a hidden member and `Pilot0` on, on a further page
    /// `Pilot<from>` on; `count` members in all, of `total`.
    fn members_answer(from: usize, count: usize, total: u64) -> String {
        let mut items = String::new();
        let hidden = r#"<li class="member-item org-main org-visibility-H"><span class="rank">Recruit</span></li>"#;
        let shown = if from == 0 {
            items.push_str(hidden);
            count - 1
        } else {
            count
        };
        for n in from..from + shown {
            items.push_str(&format!(
                r#"<li class="member-item js-member-item org-main org-visibility-V"><a class="membercard" href="/citizens/Pilot{n}"><span class="stars" style="width: 40%;"></span><span class="rank">Rank{n}</span></a></li>"#
            ));
        }
        let html = versekit_core::jsonio::dumps(&Value::str(items), None, false);
        format!(r#"{{"success":1,"data":{{"totalrows":{total},"html":{html}}},"code":"OK","msg":"OK"}}"#)
    }

    /// The site of `search_with_site` with the members of TEST on two pages (32 and 8) and the pages of `Pilot1`.
    fn site_with_members() -> Fake {
        Fake::default()
            .with(&profile_url("Musterpilot"), PROFILE)
            .with(&orgs_url("Musterpilot"), ORGS)
            .with(&profile_url("Pilot1"), PROFILE.replace("Musterpilot", "Pilot1"))
            .with(&orgs_url("Pilot1"), ORGS)
            .with_post(members::API, members::request_body("TEST", 1), members_answer(0, 32, 40))
            .with_post(members::API, members::request_body("TEST", 2), members_answer(31, 8, 40))
    }

    /// Waits for the page of members on its way.
    fn settle_members(ctx: &egui::Context, search: &mut HandleSearch) {
        for _ in 0..3000 {
            if !search.members_loading() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
            frame(ctx, search, Vec::new(), egui::Modifiers::NONE);
        }
        panic!("the members did not come");
    }

    #[test]
    fn a_click_on_an_organization_opens_its_members_page_by_page_and_a_click_on_one_searches_it() {
        let _lang = german();
        let dir = versekit_core::testdir::Dir::new("handle-suche-mitglieder");
        let net = Arc::new(site_with_members());
        let mut search = HandleSearch::with_net(&paths(&dir), Some(net.clone())).with_opener(record);
        let ctx = context();
        search.search(&ctx, "Musterpilot", false);
        settle(&ctx, &mut search);
        frame(&ctx, &mut search, Vec::new(), egui::Modifiers::NONE);
        let nodes = frame(&ctx, &mut search, Vec::new(), egui::Modifiers::NONE);
        let org = nodes.iter().find(|(r, l, _)| *r == Role::Button && l == "Testgeschwader").expect("the org").2;
        frame(&ctx, &mut search, click(org), egui::Modifiers::NONE);
        assert_eq!(search.members_sid(), Some("TEST"), "the click opened the members");
        settle_members(&ctx, &mut search);
        let posts = || net.asked().into_iter().filter(|a| a.starts_with("POST")).collect::<Vec<_>>();
        assert_eq!(posts(), [format!("POST {} {}", members::API, members::request_body("TEST", 1))], "page 1 only");
        let Some(Ok(found)) = search.members_shown().cloned() else { panic!("members: {:?}", search.members_shown()) };
        assert_eq!((found.roster.members.len(), found.roster.total), (32, Some(40)));
        frame(&ctx, &mut search, Vec::new(), egui::Modifiers::NONE);
        let nodes = frame(&ctx, &mut search, Vec::new(), egui::Modifiers::NONE);
        let has = |nodes: &[(Role, String, egui::Pos2)], role: Role, text: &str| {
            nodes.iter().find(|(r, l, _)| *r == role && l == text).map(|n| n.2)
        };
        assert!(nodes.iter().any(|(_, l, _)| *l == t("s_hqm_verborgen")), "the hidden member: {nodes:?}");
        assert!(has(&nodes, Role::Image, &tf("s_hq_sterne", &[("n", Arg::Int(2))])).is_some(), "the stars");
        let more = has(&nodes, Role::Button, &t("s_hqm_mehr")).expect("load more");
        frame(&ctx, &mut search, click(more), egui::Modifiers::NONE);
        assert!(search.members_loading(), "the click asked the next page");
        settle_members(&ctx, &mut search);
        assert_eq!(posts().len(), 2);
        let Some(Ok(found)) = search.members_shown().cloned() else { panic!("members") };
        assert_eq!(found.roster.members.len(), 40, "both pages");
        assert!(!found.roster.more());
        // A click on a member searches it; the list stays open.
        frame(&ctx, &mut search, Vec::new(), egui::Modifiers::NONE);
        let nodes = frame(&ctx, &mut search, Vec::new(), egui::Modifiers::NONE);
        assert!(has(&nodes, Role::Button, &t("s_hqm_mehr")).is_none(), "nothing more to load");
        let pilot = has(&nodes, Role::Button, "Pilot1").unwrap_or_else(|| panic!("Pilot1: {nodes:?}"));
        frame(&ctx, &mut search, click(pilot), egui::Modifiers::NONE);
        assert!(search.searching(), "the click started a search");
        settle(&ctx, &mut search);
        assert!(matches!(search.shown(), Some(Ok(found)) if found.card.profile.handle == "Pilot1"));
        assert_eq!(search.members_sid(), Some("TEST"));
        // Counter-check: a click on the organization open closes its members.
        frame(&ctx, &mut search, Vec::new(), egui::Modifiers::NONE);
        let nodes = frame(&ctx, &mut search, Vec::new(), egui::Modifiers::NONE);
        let org = has(&nodes, Role::Button, "Testgeschwader").expect("the org");
        frame(&ctx, &mut search, click(org), egui::Modifiers::NONE);
        assert_eq!(search.members_sid(), None);
    }

    #[test]
    fn without_the_net_the_members_say_so_and_nothing_is_asked() {
        let _lang = german();
        let ctx = egui::Context::default();
        let mut search = HandleSearch::empty();
        search.open_members(&ctx, "TEST");
        assert!(!search.members_loading());
        assert_eq!(search.members_shown(), Some(&Err(Failure::Offline)));
    }

    #[test]
    fn the_empty_field_shows_the_handles_searched_lately_and_a_click_searches_one() {
        let _lang = german();
        let dir = versekit_core::testdir::Dir::new("handle-suche-verlauf");
        let mut search = search_with_site(&dir);
        let ctx = context();
        search.search(&ctx, "Musterpilot", false);
        settle(&ctx, &mut search);
        search.close();
        search.query.clear();
        frame(&ctx, &mut search, Vec::new(), egui::Modifiers::NONE);
        let heading = t("s_hq_verlauf");
        let shown = |nodes: &[(Role, String, egui::Pos2)]| nodes.iter().any(|(_, l, _)| *l == heading);
        assert!(!shown(&frame(&ctx, &mut search, Vec::new(), egui::Modifiers::NONE)), "not without the cursor");
        assert!(!search.has_card());
        search.focus_field(&ctx);
        frame(&ctx, &mut search, Vec::new(), egui::Modifiers::NONE);
        let nodes = frame(&ctx, &mut search, Vec::new(), egui::Modifiers::NONE);
        assert!(shown(&nodes) && search.has_card(), "the empty field with the cursor: {nodes:?}");
        // Counter-check: a text in the field closes them.
        search.query = "Mu".into();
        frame(&ctx, &mut search, Vec::new(), egui::Modifiers::NONE);
        assert!(!shown(&frame(&ctx, &mut search, Vec::new(), egui::Modifiers::NONE)) && !search.history_open());
        search.query.clear();
        search.focus_field(&ctx);
        frame(&ctx, &mut search, Vec::new(), egui::Modifiers::NONE);
        let nodes = frame(&ctx, &mut search, Vec::new(), egui::Modifiers::NONE);
        let entry = nodes
            .iter()
            .find(|(r, l, _)| *r == Role::Button && l == "Musterpilot")
            .unwrap_or_else(|| panic!("the handle: {nodes:?}"));
        frame(&ctx, &mut search, click(entry.2), egui::Modifiers::NONE);
        assert!(search.searching() || search.shown().is_some(), "the click searched");
        settle(&ctx, &mut search);
        assert!(matches!(search.shown(), Some(Ok(found)) if found.card.profile.handle == "Musterpilot"));
        assert!(!search.history_open());
    }

    #[test]
    fn enter_searches_from_the_file_and_ctrl_enter_fetches_anew() {
        let _lang = german();
        let dir = versekit_core::testdir::Dir::new("handle-suche-tasten");
        let mut search = search_with_site(&dir);
        let ctx = context();
        search.search(&ctx, "Musterpilot", false);
        settle(&ctx, &mut search);
        let enter = |modifiers| egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        let mut press = |modifiers: egui::Modifiers| {
            search.focus_field(&ctx);
            frame(&ctx, &mut search, Vec::new(), egui::Modifiers::NONE);
            frame(&ctx, &mut search, vec![enter(modifiers)], modifiers);
            settle(&ctx, &mut search);
            search.shown().cloned()
        };
        let Some(Ok(kept)) = press(egui::Modifiers::NONE) else { panic!("kept") };
        assert!(!kept.live, "Enter takes the handle kept");
        let Some(Ok(fresh)) = press(egui::Modifiers::COMMAND) else { panic!("fresh") };
        assert!(fresh.live, "Ctrl+Enter fetches anew");
        // Escape in the field closes the card.
        search.focus_field(&ctx);
        frame(&ctx, &mut search, Vec::new(), egui::Modifiers::NONE);
        let escape = egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        frame(&ctx, &mut search, vec![escape], egui::Modifiers::NONE);
        assert!(search.shown().is_none());
    }

    #[test]
    fn a_text_that_is_no_handle_asks_nothing() {
        let _lang = german();
        let dir = versekit_core::testdir::Dir::new("handle-suche-ungueltig");
        let mut search = search_with_site(&dir);
        let ctx = egui::Context::default();
        search.search(&ctx, "a/b", false);
        assert!(!search.searching());
        assert_eq!(search.shown(), Some(&Err(Failure::Invalid)));
        // Counter-check: without a way to the net nothing is asked either, and the card says so.
        let mut offline = HandleSearch::with_net(&paths(&dir), None);
        offline.search(&ctx, "Musterpilot", false);
        assert_eq!(offline.shown(), Some(&Err(Failure::Offline)));
    }

    #[test]
    fn a_click_on_a_log_line_sends_the_other_player_to_the_search() {
        let _lang = german();
        let dir = versekit_core::testdir::Dir::new("handle-suche-log");
        let mut search = search_with_site(&dir);
        search.set_own_handle(Some("Selbst".into()));
        let line = "<2026-10-27T20:03:38.726Z> [Notice] <Actor Death> CActor::Kill: 'Selbst' [2001] in zone 'X' \
                    killed by 'Musterpilot' [2002] using 'gun_s1' [Class gun] with damage type 'Bullet'";
        assert!(search.take_log(line, 90.0));
        let ctx = context();
        frame(&ctx, &mut search, Vec::new(), egui::Modifiers::NONE);
        let text = search.monitor.shown(100.0, 60).next().expect("a line").text(language::current());
        assert!(text.contains("Musterpilot") && text.contains("Selbst"), "{text}");
        let nodes = frame(&ctx, &mut search, Vec::new(), egui::Modifiers::NONE);
        let line_node =
            nodes.iter().find(|(role, label, _)| *role == Role::Button && *label == text).expect("the line");
        frame(&ctx, &mut search, click(line_node.2), egui::Modifiers::NONE);
        assert!(search.searching(), "the click started a search");
        settle(&ctx, &mut search);
        assert!(matches!(search.shown(), Some(Ok(found)) if found.card.profile.handle == "Musterpilot"));
        // Counter-check: switched off, the monitor takes nothing and draws nothing.
        search.log.on = false;
        assert!(!search.take_log(&line.replace("20:03", "20:04"), 95.0));
        let nodes = frame(&ctx, &mut search, Vec::new(), egui::Modifiers::NONE);
        assert!(!nodes.iter().any(|(_, label, _)| *label == text));
    }

    /// The full card with the members of its organization open at every text size in both languages: nothing
    /// reaches past its width.
    #[test]
    fn the_full_card_fits_its_width_at_every_size() {
        let _guard = crate::language_lock();
        let card = versekit_core::handle_query::Card {
            profile: page::parse_profile(RICH_PROFILE).expect("profile"),
            orgs: page::parse_orgs(RICH_ORGS),
        };
        let member = |handle: &str, rank: &str, stars: u8| Member { handle: handle.into(), rank: rank.into(), stars };
        let roster = members::Roster {
            sid: "TEST".into(),
            members: vec![
                member("Ein_sehr_langer_Handle_mit_vielen_Zeichen_0123456789", "Ein sehr langer Rang mit Worten", 5),
                member("", "Recruit", 0),
                member("Kurz", "Pilot", 2),
            ],
            total: Some(3724),
            pages: 1,
            end: false,
            fetched: 0.0,
        };
        let mut long = card.clone();
        long.profile.handle = "Ein_sehr_langer_Handle_mit_vielen_Zeichen_0123456789".into();
        let scheme = versekit_core::theme::current();
        let (low, high) = versekit_core::font_size::POINTS_RANGE;
        let mut findings = Vec::new();
        for lang in [Choice::De, Choice::En] {
            set_language(lang);
            for points in low..=high {
                for card in [&card, &long] {
                    let ctx = context();
                    crate::apply_size(&ctx, points);
                    let found = Found { card: card.clone(), fetched: 0.0, live: true, failed: None };
                    let mut search = HandleSearch::empty().with_sample(found, roster.clone());
                    let mut output = egui::FullOutput::default();
                    for _ in 0..2 {
                        let input = egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 2000.0))),
                            ..Default::default()
                        };
                        output = ctx.run_ui(input, |ui| search.card(ui, scheme));
                        output.textures_delta.clear();
                    }
                    let right = output
                        .shapes
                        .iter()
                        .filter_map(|c| match &c.shape {
                            egui::Shape::Text(text) => Some(text.pos.x + text.galley.rect.max.x),
                            egui::Shape::Rect(rect) => Some(rect.rect.max.x),
                            _ => None,
                        })
                        .fold(0.0f32, f32::max);
                    if right > CARD_WIDTH + 0.5 {
                        findings.push(format!("{lang:?} Stufe {points} {}: {right:.1}", card.profile.handle));
                    }
                }
            }
        }
        set_language(Choice::Auto);
        assert!(findings.is_empty(), "zu breit:\n{}", findings.join("\n"));
    }

    #[test]
    fn the_own_handle_stands_in_the_empty_field_and_not_in_the_log_monitor() {
        let _lang = german();
        let dir = versekit_core::testdir::Dir::new("handle-suche-eigen");
        let mut search = HandleSearch::with_net(&paths(&dir), Some(Arc::new(rich_site("Eigenpilot"))));
        assert_eq!(search.field_hint(), t("s_hq_feld_hinweis"), "before the log knows it");
        search.set_own_handle(Some("Eigenpilot".into()));
        assert_eq!(search.field_hint(), "Suchen · du: Eigenpilot");
        // No line of its own below the bar: the bar stays low.
        assert!(!search.has_log_lines(now()), "the own handle makes no line");
    }

    #[test]
    fn the_choice_of_organizations_counts_the_main_one() {
        let _lang = german();
        let dir = versekit_core::testdir::Dir::new("handle-suche-orgs");
        let mut search = search_with_site(&dir);
        assert_eq!(search.settings.affiliations, 2, "three organizations by default");
        let ctx = context();
        let scheme = versekit_core::theme::current();
        let run = |search: &mut HandleSearch, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 800.0))),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| search.settings(ui, scheme));
            output.textures_delta.clear();
            let update = output.platform_output.accesskit_update.expect("nodes");
            update
                .nodes
                .iter()
                .filter(|(_, n)| n.role() == Role::RadioButton)
                .filter_map(|(_, n)| {
                    let b = n.bounds()?;
                    Some((n.label()?.to_string(), egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32)))
                })
                .collect::<Vec<_>>()
        };
        run(&mut search, Vec::new());
        let mut radios = run(&mut search, Vec::new());
        radios.sort_by(|a, b| a.0.cmp(&b.0));
        let labels: Vec<&str> = radios.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(labels, ["1", "2", "3", "4"]);
        let one = radios[0].1;
        run(&mut search, click(one));
        assert_eq!(search.settings.affiliations, 0, "one organization: the main one alone");
        assert_eq!(Settings::read(&paths(&dir)).affiliations, 0, "written");
        let four = radios[3].1;
        run(&mut search, click(four));
        assert_eq!(Settings::read(&paths(&dir)).affiliations, 3);
    }

    #[test]
    fn every_text_key_of_the_search_exists_in_both_languages() {
        let keys = [
            "s_hq_feld_hinweis",
            "s_hq_feld_hinweis_eigen",
            "s_hq_feld_tipp",
            "s_hq_feld_name",
            "s_hq_sucht",
            "s_hq_ungueltig",
            "s_hq_nicht_gefunden",
            "s_hq_offline",
            "s_hq_fehler",
            "s_hq_belegt",
            "s_hq_stand",
            "s_hq_frisch",
            "s_hq_haupt",
            "s_hq_weitere",
            "s_hq_verborgen",
            "s_hq_keine_org",
            "s_hq_mitglieder",
            "s_hq_profil",
            "s_hq_cm",
            "s_hq_partner",
            "s_hq_sprachen",
            "s_hq_sid",
            "s_hq_sterne",
            "s_hq_schliessen",
            "s_hq_handle_als",
            "s_hq_org_als",
            "s_hq_markierung_weg",
            "s_hq_markiert",
            "s_hq_handle_markieren",
            "s_hq_export",
            "s_hq_import",
            "s_hq_export_ok",
            "s_hq_import_ok",
            "s_hq_import_fehler",
            "s_hq_export_fehler",
            "s_hq_log_klick",
            "s_hq_einst_tage",
            "s_hq_einst_tage_h",
            "s_hq_einst_aff",
            "s_hq_einst_verborgen",
            "s_hq_einst_leeren",
            "s_hq_einst_leeren_h",
            "s_hq_einst_log",
            "s_hq_einst_log_sek",
            "s_hq_cache_leeren",
            "s_hq_verlauf",
            "s_hqm_auf",
            "s_hqm_zu",
            "s_hqm_titel",
            "s_hqm_laedt",
            "s_hqm_laedt_mehr",
            "s_hqm_mehr",
            "s_hqm_anzahl",
            "s_hqm_anzahl_ohne",
            "s_hqm_grenze",
            "s_hqm_rsi",
            "s_hqm_leer",
            "s_hqm_verborgen",
        ];
        for key in keys {
            for lang in [language::Lang::De, language::Lang::En] {
                let text = language::text_in(lang, key).unwrap_or_else(|| panic!("{key} fehlt"));
                assert!(!text.contains('—') && !text.contains('–'), "{key}: Gedankenstrich");
                for sentence in text.split(['.', '!', '?']) {
                    assert!(sentence.split_whitespace().count() <= 10, "{key}: Satz zu lang: {sentence}");
                }
            }
        }
    }
}
