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

//! The page "Settings › Connect org": connect VerseKit to any org server of
//! the open org standard by its address, with a code the member confirms in
//! the org's panel; each connected org as a folding entry with its member,
//! rights, a check and disconnecting.
//!
//! ⚠ Only the code and the bare address of the confirmation page are shown,
//! copied and opened, never a link with the code built in. The page holds
//! no token: it draws the [`View`] of the [`Hub`].

use std::sync::Arc;

use egui::{Align, Key, Layout, Rect, RichText, Ui, UiBuilder, vec2};
use versekit_core::basetool::error::ApiError;
use versekit_core::basetool::http::{HttpTransport, Request, Response, Transport};
use versekit_core::language::{self, Arg, t, tf};
use versekit_core::orgstd::areas::Areas;
use versekit_core::orgstd::client::OrgClient;
use versekit_core::orgstd::discovery::{Discovery, Origin};
use versekit_core::orgstd::hub::{ConnectionView, Hub, Kind, Login, Outcome, View, system_clock};
use versekit_core::orgstd::store::{Connection, Store};
use versekit_core::orgstd::{self, OrgError};
use versekit_core::paths::Host;
use versekit_core::theme::{Color, Scheme};

use crate::color;
use crate::layout::{self, GAP};
use crate::org_data::OrgLink;
use crate::settings::SETTINGS_COLUMN_MIN;
use crate::widgets::{self, ButtonKind};

/// How long "copied" shows on the copy button.
const COPIED_SECONDS: f64 = 2.0;
/// The cards of the page: connect, the connected orgs, privacy.
const CARDS: usize = 3;
/// The widest a card gets: in a wide window the cards stand side by side at this width instead of stretching.
const CARD_MOST: f32 = 600.0;
/// The made-up org of the preview.
pub const SAMPLE_ADDRESS: &str = "https://orga.example.org";

/// A transport that never reaches the network: the preview.
struct Offline;

impl Transport for Offline {
    fn send(&self, _request: &Request) -> Result<Response, ApiError> {
        Err(ApiError::network("preview"))
    }
}

/// What the page keeps between frames.
pub struct OrgConnectState {
    hub: Hub,
    /// The switches per org and area.
    areas: Areas,
    /// The preview with a made-up org: nothing goes out.
    sample: bool,
    address: String,
    address_error: Option<String>,
    /// A login started from the field runs: when it connects, the field is emptied.
    connecting: bool,
    copied_at: Option<f64>,
    notice: Option<(String, f64)>,
}

pub(crate) fn sample_connection() -> Connection {
    let origin = Origin::parse(SAMPLE_ADDRESS).expect("a fixed address");
    let api = format!("{SAMPLE_ADDRESS}/versekit/v1");
    Connection {
        discovery: Discovery {
            origin,
            name: "Nova".into(),
            tag: "NOVA".into(),
            homepage: None,
            version: "1".into(),
            api_base: api.clone(),
            device_endpoint: format!("{api}/device/code"),
            token_endpoint: format!("{api}/token"),
            revocation_endpoint: Some(format!("{api}/revoke")),
            scopes: orgstd::SCOPES.iter().map(|s| s.to_string()).collect(),
        },
        member: "Vega".into(),
        // The rights of a member who reads and books but does not share: the pages of the made-up org show, and
        // the lines to share stay away from the pages of the stock and the blueprints.
        scopes: ["stock:read", "blueprints:read", "orders:read", "orders:write"].map(String::from).to_vec(),
        access_token: String::new(),
        access_expires: 0,
        refresh_token: "preview".into(),
        connected_at: 1_763_100_000,
        checked_at: Some(1_763_103_600),
    }
}

impl OrgConnectState {
    /// The page over `hub`; `sample` marks the preview.
    pub fn with_hub(hub: Hub, sample: bool) -> OrgConnectState {
        OrgConnectState {
            hub,
            areas: Areas::in_memory(),
            sample,
            address: String::new(),
            address_error: None,
            connecting: false,
            copied_at: None,
            notice: None,
        }
    }

    /// The preview: one made-up org in memory, no network.
    pub fn sample() -> OrgConnectState {
        let client = OrgClient::new(Arc::new(Offline), env!("CARGO_PKG_VERSION"));
        let hub = Hub::new(client, Store::memory(vec![sample_connection()]), system_clock());
        let state = OrgConnectState::with_hub(hub, true);
        for kind in Kind::ALL {
            state.areas.set(SAMPLE_ADDRESS, kind, true);
        }
        state.hub.preload(SAMPLE_ADDRESS, crate::org_data::sample_data((system_clock())()));
        state
    }

    /// The same page with the switches kept in `areas`.
    pub fn with_areas(mut self, areas: Areas) -> OrgConnectState {
        self.areas = areas;
        self
    }

    /// The hub and the switches, for the pages that show or share data of the orgs.
    pub fn link(&self) -> OrgLink {
        OrgLink { hub: self.hub.clone(), areas: self.areas.clone(), sample: self.sample }
    }

    /// The real connections of this machine, kept in `folder` (the user's own folder, or one inside a copy of
    /// the data).
    pub fn system(host: &Host, folder: &str) -> OrgConnectState {
        let transport: Arc<dyn Transport> = match HttpTransport::new("", host) {
            Ok(transport) => Arc::new(transport),
            Err(_) => Arc::new(Offline),
        };
        let client = OrgClient::new(transport, env!("CARGO_PKG_VERSION"));
        OrgConnectState::with_hub(Hub::new(client, Store::in_folder(folder), system_clock()), false)
    }

    pub fn hub(&self) -> &Hub {
        &self.hub
    }
}

/// The text of an error of the org server.
pub fn error_text(error: OrgError) -> String {
    let key = match error.code {
        orgstd::E_ADDRESS => "s_os_f_adresse",
        orgstd::E_INSECURE => "s_os_f_unsicher",
        orgstd::E_NETWORK => "s_os_f_netz",
        orgstd::E_NO_STANDARD => "s_os_f_kein_standard",
        orgstd::E_VERSION => "s_os_f_version",
        orgstd::E_FOREIGN => "s_os_f_fremd",
        orgstd::E_DENIED => "s_os_f_abgelehnt",
        orgstd::E_EXPIRED => "s_os_f_abgelaufen",
        orgstd::E_RELOGIN => "s_os_f_neu",
        orgstd::E_SCOPE => "s_os_f_recht",
        orgstd::E_RATE => "s_os_f_zu_oft",
        orgstd::E_STORE => "s_os_f_speichern",
        orgstd::E_GONE => "s_os_f_weg",
        orgstd::E_SERVER => return tf("s_os_f_server", &[("status", Arg::from(i64::from(error.status)))]),
        _ => "s_os_f_antwort",
    };
    t(key)
}

/// The text of a right of the standard.
pub fn scope_text(scope: &str) -> String {
    let key = match scope {
        "stock:read" => "s_os_r_bestand_lesen",
        "stock:write" => "s_os_r_bestand_teilen",
        "blueprints:read" => "s_os_r_bp_lesen",
        "blueprints:write" => "s_os_r_bp_teilen",
        "orders:read" => "s_os_r_auftraege_lesen",
        "orders:write" => "s_os_r_auftraege_buchen",
        other => return other.to_string(),
    };
    t(key)
}

fn small(ui: &mut Ui, text: &str, tone: Color) {
    widgets::reading(ui, RichText::new(text).small().color(color(tone)));
}

fn button<'a>(text: &'a str, kind: ButtonKind, scheme: &'a Scheme) -> widgets::Button<'a> {
    widgets::Button::new(text, kind, scheme)
}

fn when(seconds: u64) -> String {
    versekit_core::formats::date_time(language::current(), seconds as f64)
}

enum Act {
    Connect,
    Cancel,
    Open(String),
    Copy(String),
    Check(String),
    Disconnect(String),
    Area(String, Kind, bool),
}

/// The page.
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut OrgConnectState) {
    let ctx = ui.ctx().clone();
    let now = ui.input(|i| i.time);
    let view = state.hub.view();
    match &view.login {
        Some(Login::Connected { .. }) if state.connecting => {
            state.connecting = false;
            state.address.clear();
        }
        Some(Login::Failed(_)) | None => state.connecting = false,
        _ => {}
    }
    let mut acts = Vec::new();
    small(ui, &t("s_os_lead"), scheme.sub);
    ui.add_space(GAP);
    // One column below the other when two do not fit, else up to three side by side; no card wider than
    // CARD_MOST. In two columns the privacy card goes under the shorter one.
    let room = ui.available_width();
    let count =
        if room < 2.0 * SETTINGS_COLUMN_MIN + GAP { 1 } else { layout::columns_that_fit(room, SETTINGS_COLUMN_MIN, CARDS) };
    let width = ((room - (count - 1) as f32 * GAP) / count as f32).floor().min(CARD_MOST);
    let placed = if count == 2 { CARDS - 1 } else { CARDS };
    ui.scope(|ui| {
        let columns = ui
            .horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = GAP;
                (0..count)
                    .map(|column| {
                        let drawn = ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
                            ui.set_width(width);
                            for card in (column..placed).step_by(count) {
                                if card > column {
                                    ui.add_space(GAP);
                                }
                                match card {
                                    0 => new_card(ui, scheme, state, &view, now, &mut acts),
                                    1 => list_card(ui, scheme, &view, &state.areas, &mut acts),
                                    _ => privacy_card(ui, scheme),
                                }
                            }
                        });
                        drawn.response.rect
                    })
                    .collect::<Vec<Rect>>()
            })
            .inner;
        if let [left, right] = columns[..] {
            let shorter = if right.bottom() < left.bottom() { right } else { left };
            let rect = Rect::from_min_size(shorter.left_bottom() + vec2(0.0, GAP), vec2(width, f32::INFINITY));
            ui.scope_builder(UiBuilder::new().max_rect(rect).layout(Layout::top_down(Align::Min)), |ui| {
                ui.set_width(width);
                privacy_card(ui, scheme);
            });
        }
    });
    for act in acts {
        apply(state, act, &ctx, now);
    }
    if let Some((text, since)) = state.notice.clone()
        && !widgets::notice(&ctx, scheme, since, "check", &text, None).shown
    {
        state.notice = None;
    }
    if state.hub.working() || state.copied_at.is_some_and(|at| now - at < COPIED_SECONDS) {
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }
}

/// The card that says where the access data lie.
fn privacy_card(ui: &mut Ui, scheme: &Scheme) {
    widgets::card(ui, scheme, |ui| {
        ui.set_width(ui.available_width());
        widgets::card_title(ui, scheme, &t("s_ei_datenschutz"));
        ui.add_space(4.0);
        small(ui, &t("s_os_datenschutz"), scheme.sub);
    });
}

fn apply(state: &mut OrgConnectState, act: Act, ctx: &egui::Context, now: f64) {
    match act {
        Act::Connect => match Origin::parse(&state.address) {
            Ok(_) => {
                state.address_error = None;
                state.connecting = true;
                state.hub.connect(&state.address);
            }
            Err(error) => state.address_error = Some(error_text(error)),
        },
        Act::Cancel => state.hub.cancel_login(),
        Act::Open(url) => {
            if state.sample {
                state.notice = Some((t("s_sle_beispiel_browser"), now));
            } else {
                versekit_core::browser::open_in_browser(&url);
            }
        }
        Act::Copy(code) => {
            ctx.copy_text(code);
            state.copied_at = Some(now);
            state.notice = Some((t("s_os_kopiert"), now));
        }
        Act::Check(id) => state.hub.check(&id),
        Act::Disconnect(id) => {
            state.hub.disconnect(&id);
            state.areas.forget(&id);
        }
        Act::Area(id, kind, on) => {
            if !state.areas.set(&id, kind, on) {
                state.notice = Some((t("s_os_f_speichern"), now));
            }
        }
    }
}

/// The card to connect a new org: the address, then the code to confirm.
fn new_card(ui: &mut Ui, scheme: &Scheme, state: &mut OrgConnectState, view: &View, now: f64, acts: &mut Vec<Act>) {
    widgets::card(ui, scheme, |ui| {
        ui.set_width(ui.available_width());
        widgets::card_title(ui, scheme, &t("s_os_neu"));
        if state.sample {
            small(ui, &t("s_os_beispiel"), scheme.sub);
        }
        ui.add_space(4.0);
        match &view.login {
            Some(Login::Looking { .. }) => {
                small(ui, &t("s_os_suche"), scheme.sub);
                ui.add_space(6.0);
                if ui.add(button(&t("s_os_abbrechen"), ButtonKind::Normal, scheme)).clicked() {
                    acts.push(Act::Cancel);
                }
            }
            Some(Login::Code { org, user_code, verification_uri }) => {
                ui.label(RichText::new(org).strong());
                small(ui, &t("s_os_code_h"), scheme.sub);
                ui.add_space(4.0);
                ui.label(RichText::new(user_code).heading().color(color(scheme.accent)));
                ui.add_space(4.0);
                widgets::reading(ui, RichText::new(verification_uri).color(color(scheme.accent)));
                ui.add_space(4.0);
                small(ui, &t("s_os_code_warnung"), scheme.yellow);
                ui.add_space(6.0);
                ui.horizontal_wrapped(|row| {
                    let open = t("s_os_seite_oeffnen");
                    if row.add(button(&open, ButtonKind::Strong, scheme).icon("external-link")).clicked() {
                        acts.push(Act::Open(verification_uri.clone()));
                    }
                    let copied = state.copied_at.is_some_and(|at| now - at < COPIED_SECONDS);
                    let copy = t("s_os_code_kopieren");
                    if row.add(button(&copy, ButtonKind::Normal, scheme).icon(if copied { "check" } else { "copy" })).clicked()
                    {
                        acts.push(Act::Copy(user_code.clone()));
                    }
                    if row.add(button(&t("s_os_abbrechen"), ButtonKind::Normal, scheme)).clicked() {
                        acts.push(Act::Cancel);
                    }
                });
            }
            other => {
                if let Some(Login::Connected { org, member }) = other {
                    let head = if member.is_empty() {
                        tf("s_os_verbunden_mit_org", &[("org", Arg::from(org.as_str()))])
                    } else {
                        tf("s_os_verbunden_mit", &[("org", Arg::from(org.as_str())), ("name", Arg::from(member.as_str()))])
                    };
                    widgets::status_box_wrapping(ui, scheme, scheme.accent, "circle-check", &head, "");
                    ui.add_space(6.0);
                }
                if let Some(Login::Failed(error)) = other {
                    small(ui, &error_text(*error), scheme.yellow);
                    ui.add_space(6.0);
                }
                address_row(ui, scheme, state, acts);
            }
        }
    });
}

/// The field for the address with its hint and the button.
fn address_row(ui: &mut Ui, scheme: &Scheme, state: &mut OrgConnectState, acts: &mut Vec<Act>) {
    let name = t("s_os_adresse");
    ui.label(RichText::new(&name).small().color(color(scheme.sub)));
    let hint = t("s_os_adresse_beispiel");
    let width = ui.available_width().min(360.0);
    let response = widgets::checked_field(ui, scheme, &mut state.address, &hint, width, state.address_error.as_deref());
    let shown = state.address.clone();
    response.widget_info(|| {
        let mut info = egui::WidgetInfo::text_edit(true, "", &shown, &hint);
        info.label = Some(name.clone());
        info
    });
    if response.changed() {
        state.address_error = None;
    }
    let enter = response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
    small(ui, &t("s_os_adresse_h"), scheme.sub);
    ui.add_space(6.0);
    let word = t("s_os_verbinden");
    let clicked = ui.add(button(&word, ButtonKind::Strong, scheme).icon("share-2")).clicked();
    if clicked || enter {
        acts.push(Act::Connect);
    }
}

fn outcome_text(outcome: &Outcome) -> (String, bool) {
    match outcome {
        Outcome::Checked => (t("s_os_ok"), false),
        Outcome::Failed(error) => (error_text(*error), true),
        Outcome::Disconnected { told: true } => (t("s_os_getrennt"), false),
        Outcome::Disconnected { told: false } => (t("s_os_getrennt_lokal"), true),
    }
}

/// The card with every connected org, each folding.
fn list_card(ui: &mut Ui, scheme: &Scheme, view: &View, areas: &Areas, acts: &mut Vec<Act>) {
    widgets::card(ui, scheme, |ui| {
        ui.set_width(ui.available_width());
        widgets::card_title(ui, scheme, &t("s_os_verbundene"));
        ui.add_space(4.0);
        if view.store_failed {
            small(ui, &t("s_os_f_ablage"), scheme.yellow);
        }
        // What disconnecting left for orgs that are gone now.
        for (id, outcome) in &view.outcomes {
            if !view.connections.iter().any(|c| &c.id == id) {
                let (text, warn) = outcome_text(outcome);
                small(ui, &format!("{id}: {text}"), if warn { scheme.yellow } else { scheme.sub });
            }
        }
        if view.connections.is_empty() {
            small(ui, &t("s_os_keine"), scheme.sub);
            return;
        }
        let single = view.connections.len() == 1;
        for connection in &view.connections {
            entry(ui, scheme, view, areas, connection, single, acts);
        }
    });
}

/// The name and the help of the switch of an area.
fn area_words(kind: Kind) -> (String, String) {
    match kind {
        Kind::Stock => (t("s_ob_lager"), t("s_ob_lager_h")),
        Kind::Blueprints => (t("s_ob_bauplaene"), t("s_ob_bauplaene_h")),
        Kind::Orders => (t("s_ob_auftraege"), t("s_ob_auftraege_h")),
    }
}

/// The switches of the areas the member gave a right for; off from the start. Without a right there is no row.
fn area_rows(ui: &mut Ui, scheme: &Scheme, areas: &Areas, c: &ConnectionView, acts: &mut Vec<Act>) {
    let granted: Vec<Kind> = Kind::ALL.into_iter().filter(|kind| c.granted(*kind)).collect();
    if granted.is_empty() {
        return;
    }
    ui.add_space(4.0);
    ui.label(RichText::new(format!("{}:", t("s_ob_titel"))).color(color(scheme.sub)));
    for kind in granted {
        let mut on = areas.on(&c.id, kind);
        let (name, help) = area_words(kind);
        if widgets::switch_row_help(ui, scheme, &name, &help, &mut on).changed() {
            acts.push(Act::Area(c.id.clone(), kind, on));
        }
    }
}

fn line(ui: &mut Ui, scheme: &Scheme, name: &str, value: &str) {
    ui.horizontal_wrapped(|row| {
        row.label(RichText::new(format!("{name}:")).color(color(scheme.sub)));
        widgets::reading(row, value);
    });
}

/// One connected org.
fn entry(ui: &mut Ui, scheme: &Scheme, view: &View, areas: &Areas, c: &ConnectionView, open: bool, acts: &mut Vec<Act>) {
    let title = if c.tag.is_empty() { c.name.clone() } else { format!("{} [{}]", c.name, c.tag) };
    widgets::collapsing(&title).id_salt(("orga-verbindung", c.id.as_str())).default_open(open).show(ui, |ui| {
        line(ui, scheme, &t("s_os_adresse"), &c.id);
        let member = if c.member.is_empty() { t("s_os_unbekannt") } else { c.member.clone() };
        line(ui, scheme, &t("s_os_mitglied"), &member);
        let rights: Vec<String> = c.scopes.iter().map(|s| scope_text(s)).collect();
        line(ui, scheme, &t("s_os_rechte"), &rights.join(", "));
        line(ui, scheme, &t("s_os_seit"), &when(c.connected_at));
        let checked = c.checked_at.map_or_else(|| t("s_os_nie"), when);
        line(ui, scheme, &t("s_os_geprueft"), &checked);
        area_rows(ui, scheme, areas, c, acts);
        let busy = view.busy.contains(&c.id);
        if busy {
            small(ui, &t("s_os_laeuft"), scheme.sub);
        } else if let Some((_, outcome)) = view.outcomes.iter().find(|(id, _)| id == &c.id) {
            let (text, warn) = outcome_text(outcome);
            small(ui, &text, if warn { scheme.yellow } else { scheme.accent });
        }
        ui.add_space(6.0);
        // Each button names its org for a screen reader; while one runs, both say why they wait.
        let running = t("s_os_laeuft");
        let reason = busy.then_some(running.as_str());
        let org = [("org", Arg::from(c.name.as_str()))];
        let (check, check_name) = (t("s_os_pruefen"), tf("s_os_pruefen_name", &org));
        let (off, off_name) = (t("s_os_trennen"), tf("s_os_trennen_name", &org));
        ui.horizontal_wrapped(|row| {
            let check_button = button(&check, ButtonKind::Normal, scheme).icon("rotate-cw").spoken(&check_name);
            if row.add(check_button.blocked(reason)).clicked() {
                acts.push(Act::Check(c.id.clone()));
            }
            if row.add(button(&off, ButtonKind::Danger, scheme).spoken(&off_name).blocked(reason)).clicked() {
                acts.push(Act::Disconnect(c.id.clone()));
            }
        });
    });
}
