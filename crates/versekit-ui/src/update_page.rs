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

//! Updating from the window of the user's own data (4.12 `main_window._update_click`, `pages._about`,
//! `pages._fetch_version`): the version that is on offer for the button in the head and the bar, the question
//! before it is installed, the progress, and the page „Update & Über“.
//!
//! The releases are read from the remembered list (`versionen.json`, written by the thread of the overlay) once a
//! minute; GitHub is asked for by this window itself only while the page is open, at most every five minutes.
//! The fetching is [`update_flow::Runner`]: a thread of its own, its state read here every frame.

use versekit_core::language::SpawnWithLanguage as _;
use std::sync::{Arc, Mutex, PoisonError};

use egui::{Frame, Margin, RichText, Sense, Stroke, Ui, UiBuilder};
use versekit_core::language::{Arg, t, tf};
use versekit_core::net::Fake;
use versekit_core::paths::Paths;
use versekit_core::theme::Scheme;
use versekit_core::update_flow::{Env, Runner, State};
use versekit_core::update_run;
use versekit_core::update_swap::Started;
use versekit_core::updater::{self, Packaging, Release};
use versekit_core::version::is_prerelease;

use crate::layout::GAP;
use crate::widgets::{self, ButtonKind};
use crate::{color, control_radius};

/// How often the remembered list is read again for the button in the head, in seconds.
const READ_SECONDS: f64 = 60.0;
/// How often the open page asks GitHub, in seconds: with the look of the thread of the overlay (every ten
/// minutes) at most eighteen of the sixty requests an hour GitHub allows.
const LOOK_SECONDS: f64 = 5.0 * 60.0;
/// Below this width the two boxes of the channels stand one below the other.
const NARROW: f32 = 620.0;
/// The picture of the program on the card.
const LOGO: f32 = 48.0;
/// The inner margin of the box of a channel.
const BOX_MARGIN: f32 = 12.0;

/// How the newest version of a channel stands to the running one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Relation {
    /// Nothing is known of the channel.
    #[default]
    Unknown,
    /// It is the running version: nothing to fetch.
    Same,
    /// It is newer.
    Newer,
    /// It is older: a way back.
    Older,
    /// It is newer, but the running program is a build of the source: `git pull` is its way, so the box only
    /// names the version.
    Source,
    /// It is older than the running build of the source: nothing to pull, the box only names the version.
    Ahead,
}

/// What the box of a channel offers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Channel {
    /// The newest version of the channel, as the tag says.
    pub version: Option<String>,
    pub relation: Relation,
}

impl Channel {
    /// The newest release of the channel in `releases`, whatever runs: from a test version the way back to the
    /// finished one stays open. `source`: the running program is a build of the source, which no fetch replaces.
    pub fn of(releases: &[Release], with_prerelease: bool, own: &str, source: bool) -> Channel {
        let Some(release) = updater::latest(releases, with_prerelease) else { return Channel::default() };
        let relation = if versekit_core::update_flow::same_version(&release.version, own) {
            Relation::Same
        } else if source {
            if versekit_core::version::is_newer(&release.version, own) { Relation::Source } else { Relation::Ahead }
        } else if versekit_core::version::is_newer(&release.version, own) {
            Relation::Newer
        } else {
            Relation::Older
        };
        Channel { version: Some(release.version.clone()), relation }
    }

    /// Is there something behind the button? ⚠ Not for the running version (it would be installed again) and
    /// not for a channel nothing is known of.
    pub fn can_fetch(&self) -> bool {
        matches!(self.relation, Relation::Newer | Relation::Older)
    }

    /// The words on the button, or the quiet statement that stands in its place.
    pub fn label(&self) -> String {
        let shown = self.version.as_deref().map(updater::shown_version).unwrap_or_default();
        let key = match self.relation {
            Relation::Unknown => return t("s_ub_keine_bekannt"),
            Relation::Same => "s_ub_holen_gleich",
            Relation::Newer => "s_ub_holen",
            Relation::Older => "s_ub_holen_zurueck",
            Relation::Source => "s_ub_neueste_quellcode",
            Relation::Ahead => "s_ub_neueste",
        };
        tf(key, &[("0", Arg::from(shown))])
    }
}

/// The head button „get latest version“ fetches from the test channel: the newest of all, test versions included.
/// Whether it is greyed out follows the same channel.
const HEAD_FETCH_PRERELEASE: bool = true;

/// What the remembered list says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct View {
    /// The release the head button and the bar name: newer than the running one, in the channel it is on.
    pub newer: Option<String>,
    pub stable: Channel,
    pub test: Channel,
}

impl View {
    /// The view of `releases` for the running version `own`. `with_prerelease`: the channel of the button in the
    /// head (a test version runs, or the setting asks for them). `source`: a build of the source runs.
    pub fn of(releases: &[Release], own: &str, with_prerelease: bool, source: bool) -> View {
        View {
            newer: updater::best_newer(releases, own, with_prerelease).map(|r| r.version.clone()),
            stable: Channel::of(releases, false, own, source),
            test: Channel::of(releases, true, own, source),
        }
    }

    /// Does the newest release of the channel the head button fetches from run already? Then it has nothing to
    /// fetch.
    pub fn runs_newest(&self) -> bool {
        let channel = if HEAD_FETCH_PRERELEASE { &self.test } else { &self.stable };
        channel.relation == Relation::Same
    }
}

/// What a look at GitHub found.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Look {
    /// `Some(false)`: asking failed; `None`: not asked.
    pub fetched: Option<bool>,
    pub rate_limited: bool,
}

impl Look {
    /// The line for the page when the look went wrong. ⚠ A failed look is never „up to date“: it says so.
    pub fn note(&self) -> Option<String> {
        match (self.fetched, self.rate_limited) {
            (Some(false), true) => Some(t("s_ub_grenze")),
            (Some(false), false) => Some(t("s_ub_sucht_fehler")),
            _ => None,
        }
    }
}

#[derive(Default)]
struct LookSlot {
    running: bool,
    result: Option<Look>,
}

/// The question before a version is installed.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Question {
    version: String,
    /// The game runs: the window closes for a moment, the game goes on, and „after the game“ is the other way.
    playing: bool,
    with_prerelease: bool,
}

/// What opening the question led to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Begin {
    /// There is no version to install.
    Nothing,
    /// A build of the source is updated by `git pull`, not by this window.
    Source,
    /// The question stands.
    Asked,
}

/// What the page asks the window to do.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageOut {
    /// Fetch the newest version; `true`: test versions count too.
    pub fetch: Option<bool>,
    /// A box was chosen: test versions on or off.
    pub channel: Option<bool>,
    pub news: bool,
    pub setup: bool,
    pub copy: bool,
    pub site: bool,
}

/// What the card of the page shows besides the version.
pub struct Facts<'a> {
    pub version: &'a str,
    /// Blueprints known, and how many of them the player has.
    pub known: usize,
    pub have: usize,
    pub folder: &'a str,
    /// The setting: test versions on.
    pub test_on: bool,
}

/// The state of updating in the window.
pub struct UpdateUi {
    env: Env,
    runner: Runner,
    view: View,
    releases: Vec<Release>,
    read_at: Option<f64>,
    slot: Arc<Mutex<LookSlot>>,
    next_look: f64,
    note: Option<Look>,
    question: Option<Question>,
    /// Seconds a click still waits after a failed download, as of the last frame of the page.
    manual_wait: Option<f64>,
}

/// An environment that fetches nothing: the pages of a window without an updater (and every check of this
/// crate) work on it, and no click can reach the net or replace a file.
fn offline_env(version: &str) -> Env {
    let net: Arc<dyn versekit_core::net::Net> = Arc::new(Fake::default());
    Env {
        net: net.clone(),
        download_net: net,
        own_version: version.to_string(),
        appimage: None,
        program: std::path::PathBuf::new(),
        temp: std::env::temp_dir(),
        start: Arc::new(|_| -> std::io::Result<Box<dyn Started>> { Err(std::io::Error::other("offline")) }),
        game_running: Arc::new(|| false),
        alive_wait: std::time::Duration::ZERO,
    }
}

impl UpdateUi {
    /// A window that updates nothing, running `version`.
    pub fn offline(version: &str) -> UpdateUi {
        UpdateUi::with_env(offline_env(version))
    }

    pub fn with_env(env: Env) -> UpdateUi {
        UpdateUi {
            env,
            runner: Runner::default(),
            view: View::default(),
            releases: Vec::new(),
            read_at: None,
            slot: Arc::default(),
            next_look: 0.0,
            note: None,
            question: None,
            manual_wait: None,
        }
    }

    /// Why a click cannot fetch right now: a fetch is under way, or a download failed a moment ago (in any process
    /// of the program). `None` when it can.
    fn held(&self) -> Option<String> {
        if self.is_busy() {
            return Some(t("up_laeuft_schon"));
        }
        self.manual_wait.map(|left| tf("s_up_warte", &[("0", Arg::Int(left.ceil() as i64))]))
    }

    /// The version that runs, as the tag would write it.
    pub fn own_version(&self) -> &str {
        &self.env.own_version
    }

    /// The channel of the button in the head: a test version runs, or the setting asks for test versions
    /// (`test_on`).
    pub fn wants_prerelease(&self, test_on: bool) -> bool {
        is_prerelease(&self.env.own_version) || test_on
    }

    /// The remembered list, read again when a minute has passed (or `force`). The repaint comes by itself.
    pub fn refresh(&mut self, ctx: &egui::Context, paths: &Paths, now: f64, force: bool, test_on: bool) {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(READ_SECONDS));
        if !force && self.read_at.is_some_and(|at| now - at < READ_SECONDS) {
            return;
        }
        self.read_at = Some(now);
        self.releases = updater::cache_read(paths).releases;
        let source = updater::packaging(self.env.appimage.as_deref(), &self.env.own_version) == Packaging::Source;
        self.view = View::of(&self.releases, &self.env.own_version, self.wants_prerelease(test_on), source);
    }

    /// The remembered releases, for the page „Was ist neu“.
    pub fn releases(&self) -> &[Release] {
        &self.releases
    }

    /// The version on offer without the `v` of the tag; `None` when nothing newer is known.
    pub fn newer(&self) -> Option<&str> {
        self.view.newer.as_deref().map(updater::shown_version)
    }

    pub fn view(&self) -> &View {
        &self.view
    }

    pub fn is_busy(&self) -> bool {
        self.runner.is_busy()
    }

    /// What the foot bar shows while a version is fetched: the words and how far it is (none while unknown).
    pub fn activity(&self) -> Option<(String, Option<f32>)> {
        let State::Busy { version, percent } = self.runner.state() else { return None };
        // The foot bar writes the percentage beside its track itself.
        let words = if version.is_empty() {
            t("s_ub_sucht")
        } else {
            tf("s_ub_holen_laeuft", &[("0", Arg::from(updater::shown_version(&version)))])
        };
        Some((words, percent.map(|percent| f32::from(percent) / 100.0)))
    }

    /// Once a frame: the sentences the fetching has for the player, and whether the program has to end now
    /// (the helper takes over).
    pub fn poll(&mut self, ctx: &egui::Context) -> (Vec<String>, bool) {
        if self.runner.is_busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        (self.runner.take_said(), self.runner.state() == State::Quit)
    }

    /// Fetches the newest version of the channel. Otherwise the sentence that says why not: a fetch is under way,
    /// or a download failed a moment ago.
    pub fn start(&mut self, paths: &Paths, with_prerelease: bool) -> Result<(), String> {
        self.manual_wait = update_run::manual_wait_left(paths, updater::now());
        if let Some(why) = self.held() {
            return Err(why);
        }
        if self.runner.start(&self.env, paths.host.clone(), with_prerelease) { Ok(()) } else { Err(t("up_laeuft_schon")) }
    }

    /// The click on „update“ in the bar: the question before the version is installed. A build of the source
    /// asks nothing (`git pull` is its way).
    pub fn begin(&mut self, test_on: bool) -> Begin {
        let Some(version) = self.view.newer.as_deref().map(updater::shown_version).map(str::to_string) else {
            return Begin::Nothing;
        };
        if updater::packaging(self.env.appimage.as_deref(), &self.env.own_version) == Packaging::Source {
            return Begin::Source;
        }
        let playing = (self.env.game_running)();
        self.question = Some(Question { version, playing, with_prerelease: self.wants_prerelease(test_on) });
        Begin::Asked
    }

    pub fn asking(&self) -> bool {
        self.question.is_some()
    }

    /// The question, over the dimmed window. ⚠ Escape and a click beside it do nothing: the two ways are equal,
    /// and whoever closes the question must not have chosen one. Returns a sentence for the player.
    pub fn dialogs(&mut self, ctx: &egui::Context, scheme: &Scheme, paths: &Paths) -> Option<String> {
        let question = self.question.clone()?;
        let version = Arg::from(question.version.as_str());
        let mut text =
            tf(if question.playing { "hf_update_frage_spiel" } else { "hf_update_frage" }, &[("0", version.clone())]);
        if question.playing {
            text.push_str("\n\n");
            text.push_str(&t("hf_update_frage_danach"));
        }
        let title = tf("hf_update_titel", &[("0", version.clone())]);
        let other = t(if question.playing { "hf_update_danach" } else { "hf_update_spaeter" });
        let now_word = t("hf_update_jetzt");
        let ((other_clicked, now_clicked), _closed) = widgets::dialog(ctx, scheme, "update-frage", |ui| {
            widgets::heading(ui, scheme, &title);
            ui.add_space(4.0);
            ui.label(text);
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                let other = ui.add(widgets::Button::new(&other, ButtonKind::Normal, scheme)).clicked();
                let now = ui.add(widgets::Button::new(&now_word, ButtonKind::Strong, scheme)).clicked();
                (other, now)
            })
            .inner
        });
        if now_clicked {
            self.question = None;
            if let Err(why) = self.start(paths, question.with_prerelease) {
                return Some(why);
            }
        } else if other_clicked {
            self.question = None;
            // „After the game“ only says that it will come by itself: the thread of the overlay installs it.
            return question.playing.then(|| tf("hf_update_danach_ok", &[("0", version)]));
        }
        None
    }

    /// Looks at GitHub from the open page: at once when the page opens, then every five minutes, in a thread of
    /// its own. The result shows in the boxes (the list is read again) and, if it went wrong, in a line.
    pub fn look(&mut self, ctx: &egui::Context, paths: &Paths, now: f64, test_on: bool) {
        self.manual_wait = update_run::manual_wait_left(paths, updater::now());
        if self.manual_wait.is_some() {
            // The seconds on the hint count down.
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
        let finished = {
            let mut slot = self.slot.lock().unwrap_or_else(PoisonError::into_inner);
            slot.result.take()
        };
        if let Some(look) = finished {
            self.note = Some(look);
            self.refresh(ctx, paths, now, true, test_on);
        }
        let starting = {
            let mut slot = self.slot.lock().unwrap_or_else(PoisonError::into_inner);
            let due = now >= self.next_look && !slot.running;
            if due {
                slot.running = true;
            }
            due
        };
        if !starting {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(
                (self.next_look - now).clamp(1.0, LOOK_SECONDS),
            ));
            return;
        }
        self.next_look = now + LOOK_SECONDS;
        let (net, own, host) = (self.env.net.clone(), self.env.own_version.clone(), paths.host.clone());
        let (slot, ctx) = (self.slot.clone(), ctx.clone());
        let spawned = std::thread::Builder::new().name("update-look".into()).spawn_with_language(move || {
            let paths = Paths::new(host);
            let checked = updater::check(&*net, &paths, &own, true, updater::now());
            let look = Look { fetched: checked.fetched, rate_limited: checked.rate_limited };
            let mut slot = slot.lock().unwrap_or_else(PoisonError::into_inner);
            slot.running = false;
            slot.result = Some(look);
            ctx.request_repaint();
        });
        if spawned.is_err() {
            self.slot.lock().unwrap_or_else(PoisonError::into_inner).running = false;
        }
    }

    /// The page „Update & Über“ on the user's own data.
    pub fn page(&mut self, ui: &mut Ui, scheme: &Scheme, facts: &Facts<'_>) -> PageOut {
        let mut out = PageOut::default();
        let packaging = updater::packaging(self.env.appimage.as_deref(), &self.env.own_version);
        about_card(ui, scheme, facts);
        ui.add_space(GAP);
        let held = self.held();
        let fetch_blocked = held.clone().or_else(|| self.view.runs_newest().then(|| t("s_up_sofort_aktuell")));
        widgets::card(ui, scheme, |ui| {
            ui.set_width(ui.available_width());
            widgets::card_title(ui, scheme, &t("hf_update"));
            small(ui, &t("s_up_sofort_h"), color(scheme.sub));
            ui.add_space(4.0);
            let fetch_word = t("s_up_sofort");
            let fetch = widgets::Button::new(&fetch_word, ButtonKind::Strong, scheme)
                .icon("download")
                .blocked(fetch_blocked.as_deref());
            if ui.add(fetch).clicked() {
                out.fetch = Some(HEAD_FETCH_PRERELEASE);
            }
            if packaging == Packaging::Source {
                ui.add_space(4.0);
                small(ui, &t("update_quellcode"), color(scheme.sub));
            }
            ui.add_space(GAP);
            ui.horizontal_wrapped(|row| {
                let (copy_word, news_word, setup_word) =
                    (t("s_mu_version_kopieren"), t("hf_wasistneu"), t("s_ub_einrichtung"));
                let copy = widgets::Button::new(&copy_word, ButtonKind::Normal, scheme).icon("copy");
                out.copy = row.add(copy).clicked();
                let news = widgets::Button::new(&news_word, ButtonKind::Normal, scheme).icon("sparkles");
                out.news = row.add(news).clicked();
                let setup = widgets::Button::new(&setup_word, ButtonKind::Normal, scheme).icon("wand-sparkles");
                out.setup = row.add(setup).clicked();
            });
            ui.add_space(4.0);
            out.site = widgets::external_link(ui, scheme, &t("s_mu_projektseite")).clicked();
        });
        ui.add_space(GAP);
        widgets::card(ui, scheme, |ui| {
            ui.set_width(ui.available_width());
            self.channels(ui, scheme, facts.test_on, held.as_deref(), &mut out);
            if let Some(note) = self.note.and_then(|look| look.note()) {
                ui.add_space(GAP);
                small(ui, &note, color(scheme.yellow));
            }
        });
        out
    }

    fn channels(&self, ui: &mut Ui, scheme: &Scheme, on: bool, held: Option<&str>, out: &mut PageOut) {
        widgets::card_title(ui, scheme, &t("s_ub_kanal"));
        small(ui, &t("s_ub_kanal_h"), color(scheme.sub));
        ui.add_space(6.0);
        let stable = BoxTexts { title: t("s_ub_fertig"), hint: t("s_ub_fertig_h"), mark: None };
        let test = BoxTexts { title: t("s_ub_test"), hint: t("s_ub_test_h"), mark: Some("rc") };
        let mut boxes = [(stable, &self.view.stable, !on, false), (test, &self.view.test, on, true)];
        // Both boxes are as tall as the taller one was in the frame before: the height its content asks for,
        // frame and stroke included, never the height that was drawn (that would grow by the frame every frame).
        let height_id = ui.scope_id().with("kasten-hoehe");
        let height: f32 = ui.data(|d| d.get_temp(height_id)).unwrap_or(0.0);
        let mut taller = 0.0_f32;
        if ui.available_width() >= NARROW {
            ui.scope(|ui| {
                // egui widens the row by what the widest column takes beyond its share; one point spare keeps a
                // rounded box inside the card.
                ui.set_max_width(ui.available_width() - 1.0);
                ui.columns(2, |columns| {
                    for (column, (texts, channel, chosen, with_prerelease)) in columns.iter_mut().zip(&mut boxes) {
                        let shown =
                            channel_box(column, scheme, texts, channel, *chosen, held, *with_prerelease, height, out);
                        taller = taller.max(shown);
                    }
                });
            });
        } else {
            for (i, (texts, channel, chosen, with_prerelease)) in boxes.iter_mut().enumerate() {
                if i > 0 {
                    ui.add_space(GAP);
                }
                let shown = channel_box(ui, scheme, texts, channel, *chosen, held, *with_prerelease, height, out);
                taller = taller.max(shown);
            }
        }
        ui.data_mut(|d| d.insert_temp(height_id, taller));
        if (taller - height).abs() > 0.5 {
            ui.ctx().request_repaint();
        }
    }
}

/// The words of the box of a channel.
struct BoxTexts {
    title: String,
    hint: String,
    mark: Option<&'static str>,
}

fn small(ui: &mut Ui, text: &str, tone: egui::Color32) {
    ui.add(egui::Label::new(RichText::new(text).small().color(tone)).wrap());
}

/// A value that may be a long path: broken where the line ends, never cut off.
fn value(ui: &mut Ui, text: &str, tone: egui::Color32) {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let mut job = egui::text::LayoutJob::simple(text.to_string(), font, tone, ui.available_width());
    job.wrap.break_anywhere = true;
    ui.add(egui::Label::new(job).wrap());
}

/// The card on top: the picture, the name, the version, and what the program knows.
fn about_card(ui: &mut Ui, scheme: &Scheme, facts: &Facts<'_>) {
    widgets::card(ui, scheme, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|row| {
            row.add(
                egui::Image::from_bytes("bytes://versekit-icon.png", include_bytes!("../../../assets/icon.png"))
                    .fit_to_exact_size(egui::vec2(LOGO, LOGO)),
            );
            row.vertical(|column| {
                column.label(widgets::bold(column, t("hf_titel"), egui::TextStyle::Heading));
                column.label(RichText::new(facts.version).strong().color(color(scheme.accent)));
            });
        });
        ui.add_space(6.0);
        let known = facts.known.to_string();
        let have = facts.have.to_string();
        for (label, shown) in [(t("s_ub_bekannt"), known.as_str()), (t("s_ub_davon"), have.as_str())] {
            ui.horizontal_wrapped(|row| {
                row.label(RichText::new(label).color(color(scheme.sub)));
                row.label(shown);
            });
        }
        ui.horizontal_wrapped(|row| {
            row.label(RichText::new(t("b_ordner")).color(color(scheme.sub)));
            value(row, facts.folder, color(scheme.fg));
        });
    });
}

/// The room the frame of a box takes around its content: the inner margin and the stroke, on both sides.
fn box_frame_height(stroke: f32) -> f32 {
    2.0 * (BOX_MARGIN + stroke)
}

/// The box of one channel. A click on it chooses the channel; the button in it fetches the version at once
/// (and only if there is something to fetch: a statement that looks like a button must not act like one).
/// `min_height`: the height the box is to have at least, frame included. Returns the height its own content asks
/// for, frame included, so that the caller can bring both boxes to the taller one.
#[allow(clippy::too_many_arguments)]
fn channel_box(
    ui: &mut Ui,
    scheme: &Scheme,
    texts: &BoxTexts,
    channel: &Channel,
    chosen: bool,
    held: Option<&str>,
    with_prerelease: bool,
    min_height: f32,
    out: &mut PageOut,
) -> f32 {
    let id = ui.scope_id().with(("kasten", &texts.title));
    let hovered = was_hovered(ui, id);
    let stroke = Stroke::new(if chosen { 2.0 } else { 1.0 }, color(if chosen { scheme.accent } else { scheme.line }));
    let scope = ui.scope_builder(UiBuilder::new().sense(Sense::click()), |ui| {
        // A label that can be selected takes the click away from the box.
        ui.style_mut().interaction.selectable_labels = false;
        Frame::new()
            .fill(color(if hovered { scheme.hover } else { scheme.surface }))
            .stroke(stroke)
            .corner_radius(control_radius(scheme))
            .inner_margin(Margin::same(BOX_MARGIN as i8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let content = ui.scope(|ui| {
                    ui.horizontal_wrapped(|row| {
                        row.label(RichText::new(&texts.title).strong().color(color(if chosen {
                            scheme.accent
                        } else {
                            scheme.fg
                        })));
                        if let Some(mark) = texts.mark {
                            widgets::badge(row, scheme, scheme.yellow, mark);
                        }
                    });
                    small(ui, &texts.hint, color(scheme.sub));
                    ui.add_space(6.0);
                    if channel.can_fetch() {
                        let label = channel.label();
                        let button = widgets::Button::new(&label, ButtonKind::Normal, scheme)
                            .icon("download")
                            .wrapping()
                            .blocked(held);
                        if ui.add(button).clicked() {
                            out.fetch = Some(with_prerelease);
                        }
                    } else {
                        small(ui, &channel.label(), color(scheme.sub));
                    }
                });
                // The content is measured on its own: the room that is added below must not count as content.
                let asked = content.response.rect.height();
                let frame = box_frame_height(stroke.width);
                ui.add_space((min_height - frame - asked).max(0.0));
                asked + frame
            })
            .inner
    });
    let response = scope.response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, true, chosen, &texts.title));
    remember_rect(ui, id, response.rect);
    widgets::focus_ring(ui, &response, scheme, control_radius(scheme));
    if response.clicked() && out.fetch.is_none() && !chosen {
        out.channel = Some(with_prerelease);
    }
    #[cfg(test)]
    tests::DRAWN_BOXES.with(|boxes| boxes.borrow_mut().push(response.rect.height()));
    scope.inner
}

/// Whether the pointer lay over the thing `id` named in the frame before (a container knows its place only
/// after it is drawn, but its fill is drawn first).
pub(crate) fn was_hovered(ui: &Ui, id: egui::Id) -> bool {
    let rect: Option<egui::Rect> = ui.data(|d| d.get_temp(id));
    rect.is_some_and(|rect| ui.input(|i| i.pointer.hover_pos()).is_some_and(|at| rect.contains(at)))
}

/// Remembers where the thing `id` stood, for [`was_hovered`].
pub(crate) fn remember_rect(ui: &Ui, id: egui::Id, rect: egui::Rect) {
    ui.data_mut(|d| d.insert_temp(id, rect));
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use versekit_core::language::{Choice, set_thread_language};
    use versekit_core::theme;

    use super::*;

    thread_local! {
        /// The heights of the boxes of the channels drawn in the frame that ran last.
        pub(super) static DRAWN_BOXES: RefCell<Vec<f32>> = const { RefCell::new(Vec::new()) };
    }

    fn release(version: &str, prerelease: bool) -> Release {
        Release { version: version.into(), prerelease, ..Release::default() }
    }

    /// The page for `frames` frames in a window `width` wide: the height of the page and of both boxes, per frame.
    fn frames(ui_state: &mut UpdateUi, width: f32, frames: usize) -> Vec<(f32, Vec<f32>)> {
        let ctx = egui::Context::default();
        let facts = Facts { version: "5.0.0-dev", known: 1602, have: 444, folder: "E:\\Daten", test_on: false };
        let mut drawn = Vec::new();
        for _ in 0..frames {
            DRAWN_BOXES.with(|boxes| boxes.borrow_mut().clear());
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 900.0))),
                ..Default::default()
            };
            let mut page = 0.0;
            let mut output = ctx.run_ui(input, |ui| {
                ui_state.page(ui, theme::current(), &facts);
                page = ui.min_rect().height();
            });
            output.textures_delta.clear();
            drawn.push((page, DRAWN_BOXES.with(|boxes| boxes.borrow().clone())));
        }
        drawn
    }

    fn stocked(own: &str) -> UpdateUi {
        let mut ui = UpdateUi::offline(own);
        let releases = vec![release("v4.12.3", false), release("v4.12.2", false)];
        let source = updater::packaging(None, own) == Packaging::Source;
        ui.view = View::of(&releases, own, false, source);
        ui
    }

    /// The boxes of the channels have the height of their content and keep it: a page drawn again and again does
    /// not grow, in a wide window (side by side) and in a narrow one (one below the other).
    #[test]
    fn the_boxes_of_the_channels_keep_their_height_frame_after_frame() {
        let _language = crate::language_lock();
        for choice in [Choice::De, Choice::En] {
            set_thread_language(choice);
            // 650 is about what the page has in the smallest window, close to where the boxes go one below the other.
            for width in [1200.0, 650.0, 500.0] {
                let drawn = frames(&mut stocked("5.0.0-dev"), width, 60);
                let first = &drawn[0];
                assert_eq!(first.1.len(), 2, "both boxes are drawn at {width}");
                // The first frame knows no height from before: each box is as tall as its own content.
                let content = first.1.iter().copied().fold(0.0_f32, f32::max);
                assert!(content > 40.0, "a box has content: {first:?}");
                for (number, (page, boxes)) in drawn.iter().enumerate().skip(2) {
                    assert!(
                        (page - drawn[2].0).abs() < 0.01,
                        "{choice:?} frame {number} at {width}: the page is {page} high, in frame 2 it was {}",
                        drawn[2].0
                    );
                    assert!(
                        boxes.iter().all(|height| (height - content).abs() < 0.5),
                        "{choice:?} frame {number} at {width}: boxes {boxes:?}, the taller content asks for {content}"
                    );
                }
            }
        }
        set_thread_language(Choice::De);
    }

    /// A build of the source is updated by `git pull`: its boxes name the newest version and offer no fetch, and
    /// say no „back to“.
    #[test]
    fn a_build_of_the_source_offers_no_way_back_and_names_the_newest_version() {
        let _language = crate::language_lock();
        let releases = vec![release("v4.12.3", false), release("v4.12.2", false)];
        // A build of the source that is ahead of the newest release: the version is named, nothing to pull.
        let ahead = Channel::of(&releases, false, "5.0.0-dev", true);
        assert_eq!(ahead.version.as_deref(), Some("v4.12.3"));
        assert_eq!(ahead.relation, Relation::Ahead);
        assert!(!ahead.can_fetch(), "no button, and no „back to“");
        assert_eq!(ahead.label(), tf("s_ub_neueste", &[("0", Arg::from("4.12.3"))]));
        // One behind it: `git pull` is the way, and it says so.
        let source = Channel::of(&releases, false, "4.12.0-dev", true);
        assert_eq!(source.relation, Relation::Source);
        assert!(!source.can_fetch(), "no button: a click would fetch nothing");
        assert_eq!(source.label(), tf("s_ub_neueste_quellcode", &[("0", Arg::from("4.12.3"))]));
        // A build that is not of the source keeps the way back, and the running version stays „installed“.
        let packaged = Channel::of(&releases, false, "5.0.0", false);
        assert_eq!((packaged.relation, packaged.can_fetch()), (Relation::Older, true));
        assert_eq!(Channel::of(&releases, false, "4.12.3", true).relation, Relation::Same);
    }

    /// A test version written in capitals is the same as the running one in small letters; the head button is
    /// greyed out by the channel it fetches from.
    #[test]
    fn a_test_version_in_capitals_is_the_running_one_and_the_head_button_follows_its_channel() {
        let releases = vec![release("v5.1.0-RC2", true), release("v5.0.1", false)];
        let view = View::of(&releases, "5.1.0-rc2", true, false);
        assert_eq!(view.test.relation, Relation::Same);
        assert!(view.runs_newest());
        // The newest finished version runs, a test version is newer: the head button fetches it.
        let stable = View::of(&releases, "5.0.1", false, false);
        assert_eq!((stable.stable.relation, stable.test.relation), (Relation::Same, Relation::Newer));
        assert!(!stable.runs_newest(), "the head button fetches the test version");
    }
}
