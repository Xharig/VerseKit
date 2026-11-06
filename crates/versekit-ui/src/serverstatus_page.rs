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

//! The page "Serverstatus" (`pages._server_status`): what CIG reports on its status page. The state kept shows
//! at once, the page is asked in a thread of its own; while the page is open it looks every minute whether
//! something changed (with the tag of the last answer, so an unchanged page costs almost nothing) and redraws
//! only then. The latest incidents of two months, resolved ones too, follow in full below.
//!
//! ⚠ Names, conditions, titles and texts are the words of CIG and stay as they are; the colours of the lights
//! come from the colour scheme, so they read in every scheme.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};

use egui::{RichText, Role, Ui, WidgetInfo};
use versekit_core::browser;
use versekit_core::language::{self, Arg, t, tf};
use versekit_core::net::Net;
use versekit_core::paths::Paths;
use versekit_core::serverstatus::{self, Incident, State, System};
use versekit_core::theme::{Color, Scheme};

use crate::color;
use crate::layout::GAP;
use crate::widgets::{self, ButtonKind};

/// How many months of incidents the page shows.
const MONTHS: i64 = 2;
/// How many incidents at most.
const NOTICES: usize = 12;
/// The width of the names of the systems.
const NAME_WIDTH: f32 = 190.0;

/// What a fetch of the state brought, and whether it was the minute's look.
enum Fetched {
    State(State),
    Look(State, bool),
}

/// What the page keeps.
pub struct ServerStatusState {
    /// The data folder; none in the preview.
    paths: Option<Paths>,
    /// The way to the status page; none in the preview and in tests.
    net: Option<Arc<dyn Net>>,
    shown: State,
    /// The incidents in full; `None` while they are fetched.
    notices: Option<Vec<Incident>>,
    job: Option<Receiver<Fetched>>,
    notices_job: Option<Receiver<Vec<Incident>>>,
    /// A look the player asked for runs.
    forced: bool,
    /// When the next look is due (seconds); 0 before the page was open.
    next_look: f64,
    open_url: fn(&str) -> bool,
    /// What opening the last link said.
    answer: Option<String>,
}

fn now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

impl ServerStatusState {
    /// The state of the data folder; the page asks the real status page.
    pub fn with_files(paths: &Paths) -> ServerStatusState {
        #[cfg(not(test))]
        let net: Option<Arc<dyn Net>> = {
            let agent = versekit_core::basetool::config::user_agent(env!("CARGO_PKG_VERSION"));
            Some(Arc::new(versekit_core::net::Web::new(&agent, std::time::Duration::from_secs(15), 4 * 1024 * 1024)))
        };
        // The tests of this crate never ask the network.
        #[cfg(test)]
        let net: Option<Arc<dyn Net>> = None;
        ServerStatusState::with_net(paths, net)
    }

    /// Like [`ServerStatusState::with_files`], asking over `net`.
    pub fn with_net(paths: &Paths, net: Option<Arc<dyn Net>>) -> ServerStatusState {
        let own = Paths::new(paths.host.clone());
        ServerStatusState {
            shown: serverstatus::stored_state(&own),
            paths: Some(own),
            net,
            ..ServerStatusState::empty()
        }
    }

    fn empty() -> ServerStatusState {
        ServerStatusState {
            paths: None,
            net: None,
            shown: State::default(),
            notices: None,
            job: None,
            notices_job: None,
            forced: false,
            next_look: 0.0,
            open_url: browser::open_in_browser,
            answer: None,
        }
    }

    /// The preview: a made-up state with an open and a resolved incident, nothing asked.
    pub fn sample(now: f64) -> ServerStatusState {
        let incident = |title: &str, begun: f64, resolved: Option<f64>, lines: &[(&str, bool)]| Incident {
            title: title.to_string(),
            severity: "maintenance".to_string(),
            affected: vec!["Persistent Universe".to_string(), "Arena Commander".to_string()],
            begun: Some(begun as i64),
            resolved: resolved.map(|r| r as i64),
            file: String::new(),
            address: String::new(),
            lines: lines.iter().map(|(text, bold)| (text.to_string(), *bold)).collect(),
        };
        let open = incident(
            "Live Deployment",
            now - 3.0 * 3600.0,
            None,
            &[("1415 UTC - Initial Notice, servers are coming down.", false), ("Please stow your vehicles.", true)],
        );
        let system = |name: &str, status: &str, incidents: Vec<Incident>| {
            let light = serverstatus::light(status);
            System {
                name: name.to_string(),
                status: status.to_string(),
                light: light.to_string(),
                colour: String::new(),
                incidents,
            }
        };
        let shown = State {
            overall: "maintenance".to_string(),
            systems: vec![
                system("Platform", "operational", Vec::new()),
                system("Persistent Universe", "maintenance", vec![open.clone()]),
                system("Arena Commander", "operational", Vec::new()),
            ],
            fetched: now - 120.0,
            stand: Some(now as i64 - 3 * 3600),
            source: format!("{}/", serverstatus::BASE),
            no_net: false,
        };
        let resolved = incident(
            "Login Issues",
            now - 5.0 * 86_400.0,
            Some(now - 5.0 * 86_400.0 + 7200.0),
            &[("We are investigating reports of login failures.", false)],
        );
        ServerStatusState {
            shown,
            notices: Some(vec![open, resolved]),
            next_look: f64::INFINITY,
            ..ServerStatusState::empty()
        }
    }

    /// Replaces what opens an address (tests do not open a browser).
    pub fn with_opener(mut self, open_url: fn(&str) -> bool) -> ServerStatusState {
        self.open_url = open_url;
        self
    }

    /// What the page shows now.
    pub fn shown(&self) -> &State {
        &self.shown
    }

    /// Starts a fetch of the state in a thread of its own: `look` is the minute's look, `force` asks without the tag.
    fn fetch(&mut self, ctx: &egui::Context, look: bool, force: bool) {
        if self.job.is_some() {
            return;
        }
        let (Some(paths), Some(net)) = (&self.paths, &self.net) else { return };
        let (sender, receiver) = std::sync::mpsc::channel();
        let own = Paths::new(paths.host.clone());
        let net = Arc::clone(net);
        let lock = versekit_core::writeguard::state();
        let wake = ctx.clone();
        let started = std::thread::Builder::new().name("serverstatus".into()).spawn(move || {
            versekit_core::writeguard::adopt(&lock);
            let fetched = if look {
                let (state, changed) = serverstatus::ask(&own, net.as_ref(), now());
                Fetched::Look(state, changed)
            } else {
                Fetched::State(serverstatus::state(&own, net.as_ref(), now(), force, None))
            };
            let _ = sender.send(fetched);
            wake.request_repaint();
        });
        if started.is_ok() {
            self.job = Some(receiver);
            self.forced = force;
        }
    }

    /// Fetches the incidents in full in a thread of its own.
    fn fetch_notices(&mut self, ctx: &egui::Context) {
        if self.notices_job.is_some() {
            return;
        }
        let (Some(paths), Some(net)) = (&self.paths, &self.net) else { return };
        let (sender, receiver) = std::sync::mpsc::channel();
        let own = Paths::new(paths.host.clone());
        let net = Arc::clone(net);
        let lock = versekit_core::writeguard::state();
        let wake = ctx.clone();
        let started = std::thread::Builder::new().name("serverstatus-meldungen".into()).spawn(move || {
            versekit_core::writeguard::adopt(&lock);
            let _ = sender.send(serverstatus::messages(&own, net.as_ref(), now(), MONTHS, NOTICES));
            wake.request_repaint();
        });
        if started.is_ok() {
            self.notices = None;
            self.notices_job = Some(receiver);
        }
    }

    /// Takes in what the threads brought, fetches at the first visit and looks every minute.
    fn tick(&mut self, ctx: &egui::Context, at: f64) {
        if let Some(job) = &self.job {
            match job.try_recv() {
                Ok(Fetched::State(state)) => {
                    self.shown = state;
                    self.job = None;
                    self.forced = false;
                }
                Ok(Fetched::Look(state, changed)) => {
                    // Redrawn only when something changed; the time asked goes on anyway.
                    if changed || state.no_net != self.shown.no_net {
                        self.shown = state;
                    } else {
                        self.shown.fetched = state.fetched;
                    }
                    self.job = None;
                }
                Err(TryRecvError::Disconnected) => {
                    self.job = None;
                    self.forced = false;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if let Some(job) = &self.notices_job {
            match job.try_recv() {
                Ok(list) => {
                    self.notices = Some(list);
                    self.notices_job = None;
                }
                Err(TryRecvError::Disconnected) => {
                    self.notices = Some(Vec::new());
                    self.notices_job = None;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if self.next_look == 0.0 {
            self.fetch(ctx, false, false);
            self.fetch_notices(ctx);
            self.next_look = at + serverstatus::POLL_SECONDS;
        } else if at >= self.next_look {
            self.fetch(ctx, true, false);
            self.next_look = at + serverstatus::POLL_SECONDS;
        }
        if self.next_look.is_finite() {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64((self.next_look - at).clamp(1.0, 60.0)));
        }
    }

    fn follow(&mut self, url: &str) {
        let key = if (self.open_url)(url) { "s_ub_auf" } else { "s_ub_auf_nein" };
        self.answer = Some(tf(key, &[("0", Arg::from(url))]));
    }
}

/// The colour of a light from the colour scheme.
fn light_colour(scheme: &Scheme, light: &str) -> Color {
    match light {
        "ok" => scheme.accent,
        "hinweis" => scheme.blue,
        "aus" => scheme.red,
        _ => scheme.yellow,
    }
}

/// The colour of the whole: the one of the worst system.
fn whole_colour(scheme: &Scheme, state: &State) -> Color {
    let rank = |light: &str| match light {
        "ok" => 0,
        "hinweis" => 1,
        "aus" => 3,
        _ => 2,
    };
    state
        .systems
        .iter()
        .map(|s| s.light.as_str())
        .max_by_key(|l| rank(l))
        .map_or(scheme.fg, |l| light_colour(scheme, l))
}

fn small(ui: &mut Ui, text: &str, tone: Color) {
    widgets::reading(ui, RichText::new(text).small().color(color(tone)));
}

/// A coloured bar at the left of the row, as tall as the text.
fn bar(ui: &mut Ui, tone: Color) {
    let height = ui.text_style_height(&egui::TextStyle::Body);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(4.0, height), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, color(tone));
}

/// A link that opens in the browser; a long address breaks where the line ends.
fn link(ui: &mut Ui, scheme: &Scheme, state: &mut ServerStatusState, shown: &str, url: &str) {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let mut job = egui::text::LayoutJob::simple(shown.to_string(), font, color(scheme.accent), ui.available_width());
    job.wrap.break_anywhere = true;
    let response = ui.add(egui::Link::new(job)).on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| WidgetInfo::labeled(Role::Link, true, shown));
    if response.clicked() {
        state.follow(url);
    }
}

/// A label and its value in two columns.
fn value_row(ui: &mut Ui, scheme: &Scheme, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal_top(|row| {
        row.add_sized(
            egui::vec2(NAME_WIDTH, row.text_style_height(&egui::TextStyle::Body)),
            egui::Label::new(RichText::new(label).color(color(scheme.sub))).truncate(),
        );
        row.vertical(add);
    });
}

/// The time as the clock of the user shows it.
fn clock(stamp: Option<f64>) -> String {
    match stamp.filter(|s| *s != 0.0) {
        Some(stamp) => versekit_core::formats::date_time(language::current(), stamp),
        None => "—".to_string(),
    }
}

/// Draws the page.
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut ServerStatusState) {
    let at = now();
    state.tick(ui.ctx(), at);
    small(ui, &t("s_st_lead"), scheme.sub);
    ui.add_space(6.0);
    // The head card: the button with its loading line, the answer of the browser, then the whole in its colour
    // (or the line that there is nothing yet).
    let shown = widgets::card(ui, scheme, |ui| {
        ui.set_width(ui.available_width());
        widgets::card_title(ui, scheme, &t("s_st_gesamt"));
        ui.add_space(4.0);
        ui.horizontal_wrapped(|row| {
            let busy = state.forced && state.job.is_some();
            let word = t("s_st_nachsehen");
            let button = widgets::Button::new(&word, ButtonKind::Strong, scheme);
            if row.add_enabled(!busy, button).clicked() {
                state.fetch(row.ctx(), false, true);
                state.fetch_notices(row.ctx());
            }
            if busy {
                row.add(egui::Label::new(RichText::new(t("s_st_laedt")).small().color(color(scheme.sub))));
            }
        });
        if let Some(answer) = &state.answer {
            small(ui, answer, scheme.sub);
        }
        ui.add_space(6.0);
        let shown = state.shown.clone();
        if shown.is_empty() {
            small(ui, &t(if shown.no_net { "s_stv_kein_netz" } else { "s_st_leer" }), scheme.sub);
            return shown;
        }
        if shown.no_net {
            small(ui, &t("s_stv_alt_ohne_netz"), scheme.gold);
            ui.add_space(4.0);
        }
        banner(ui, scheme, &shown, at);
        shown
    })
    .inner;
    if shown.is_empty() {
        return;
    }
    ui.add_space(GAP);
    widgets::card(ui, scheme, |ui| {
        ui.set_width(ui.available_width());
        widgets::card_title(ui, scheme, &t("s_st_systeme_kopf"));
        ui.add_space(4.0);
        for system in &shown.systems {
            system_row(ui, scheme, system);
        }
    });
    ui.add_space(GAP);
    widgets::card(ui, scheme, |ui| {
        ui.set_width(ui.available_width());
        widgets::card_title(ui, scheme, &t("s_st_quelle_kopf"));
        ui.add_space(4.0);
        if let Some(stand) = shown.stand {
            value_row(ui, scheme, &t("s_st_stand"), |ui| {
                ui.label(clock(Some(stand as f64)));
            });
        }
        value_row(ui, scheme, &t("s_st_geholt"), |ui| {
            ui.label(clock(Some(shown.fetched)));
        });
        value_row(ui, scheme, &t("s_st_quelle"), |ui| {
            if shown.source.is_empty() {
                ui.label("—");
            } else {
                link(ui, scheme, state, &shown.source, &shown.source);
            }
        });
        ui.add_space(6.0);
        small(ui, &t("s_stv_hinweis"), scheme.sub);
    });
    ui.add_space(GAP);
    // The last incidents: one card, the incidents in a list one below the other, the link to all of them below.
    widgets::card(ui, scheme, |ui| {
        ui.set_width(ui.available_width());
        widgets::card_title(ui, scheme, &t("s_st_letzte"));
        ui.add_space(4.0);
        match state.notices.clone() {
            None => small(ui, &t("s_st_laedt"), scheme.sub),
            Some(list) if list.is_empty() => small(ui, &t("s_st_keine"), scheme.sub),
            Some(list) => {
                widgets::list_frame(ui, scheme, |list_ui| {
                    list_ui.set_width(list_ui.available_width());
                    for (k, incident) in list.iter().enumerate() {
                        if k > 0 {
                            list_ui.separator();
                        }
                        notice_entry(list_ui, scheme, incident, at);
                    }
                });
            }
        }
        if !shown.source.is_empty() {
            ui.add_space(4.0);
            if widgets::external_link(ui, scheme, &t("s_st_alle_zeigen")).clicked() {
                state.follow(&shown.source);
            }
        }
    });
}

/// The strip in the head card, as on the status page: how long ago it was asked, and the whole in its colour.
fn banner(ui: &mut Ui, scheme: &Scheme, shown: &State, at: f64) {
    let tone = whole_colour(scheme, shown);
    let age = tf("s_st_zuletzt", &[("0", Arg::from(serverstatus::relative_time(Some(shown.fetched), at)))]);
    let word = t(if shown.all_well() { "s_st_ok" } else { "s_st_stoerung" });
    ui.horizontal_wrapped(|row| {
        bar(row, tone);
        row.add(egui::Label::new(RichText::new(&word).strong().color(color(tone))));
        row.add(egui::Label::new(RichText::new(&age).small().color(color(scheme.sub))));
    });
}

/// One system: the bar in the colour of its light, its name, its condition in the word of CIG.
fn system_row(ui: &mut Ui, scheme: &Scheme, system: &System) {
    let tone = light_colour(scheme, &system.light);
    ui.horizontal(|row| {
        bar(row, tone);
        let name = if system.name.is_empty() { "?" } else { system.name.as_str() };
        row.add_sized(
            egui::vec2(NAME_WIDTH, row.text_style_height(&egui::TextStyle::Body)),
            egui::Label::new(name).truncate(),
        );
        let condition = if system.status.is_empty() { "—" } else { system.status.as_str() };
        row.add(egui::Label::new(RichText::new(condition).color(color(tone))).truncate());
    });
}

/// One incident as the status page shows it: title and open or resolved, the age, the severity apart from the
/// systems it touches, the text in the word of CIG with its highlighted lines bold. An entry of the list in the card
/// of the last incidents.
fn notice_entry(ui: &mut Ui, scheme: &Scheme, incident: &Incident, at: f64) {
    ui.vertical(|ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|row| {
            let title = if incident.title.is_empty() { "—" } else { incident.title.as_str() };
            row.add(egui::Label::new(RichText::new(title).strong()).wrap());
            let (word, tone) = if incident.resolved.is_some() {
                (t("s_st_erledigt_kurz"), scheme.accent)
            } else {
                (t("s_st_offen"), scheme.gold)
            };
            widgets::badge(row, scheme, tone, &word);
        });
        small(ui, &serverstatus::relative_time(incident.begun.map(|b| b as f64), at), scheme.sub);
        ui.add_space(2.0);
        ui.horizontal_wrapped(|row| {
            if !incident.severity.is_empty() {
                widgets::badge(row, scheme, scheme.blue, &incident.severity);
            }
            for system in &incident.affected {
                widgets::badge(row, scheme, scheme.sub, system);
            }
        });
        if !incident.lines.is_empty() {
            ui.add_space(4.0);
        }
        for (line, bold) in &incident.lines {
            if *bold {
                ui.add(egui::Label::new(RichText::new(line).strong()).wrap());
            } else {
                small(ui, line, scheme.sub);
            }
        }
    });
}

#[cfg(test)]
mod tests;
