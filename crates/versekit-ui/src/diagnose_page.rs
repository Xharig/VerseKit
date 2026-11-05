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

//! The page "Fehler melden": the report as text for the player to read, a
//! name and a sentence of the player's own, up to four images, and three
//! ways out: send it, open it as a GitHub issue, copy it.
//!
//! Nothing leaves by itself. Sending needs the box ticked and a click; what
//! goes out is the text in the box, with the names and paths taken out
//! (`versekit_core::report`).
//!
//! The report is put together on a thread of its own, when the page opens and
//! a moment after the player stops typing, never on every showing of the page.
//! The box shows the last report finished; an action that finds it out of
//! date builds a fresh one first (sending then waits for the player to read
//! it).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{RichText, Role, TextStyle, Ui, WidgetInfo};
use versekit_core::basetool::http::{HttpTransport, Transport};
use versekit_core::basetool::sync::SyncHandle;
use versekit_core::jsonio::Value;
use versekit_core::language::{self, Arg, Lang, t, tf};
use versekit_core::paths::Paths;
use versekit_core::redact::Redactor;
use versekit_core::report::{self, ERRORS_SHOWN};
use versekit_core::report_facts::{self as facts_of, Environment, Request};
use versekit_core::report_send::{self as send, Attachment, ImageKind, SendFailure, Target};
use versekit_core::theme::Scheme;
use versekit_core::{browser, formats, writeguard};

use crate::layout::GAP;
use crate::widgets::{self, ButtonKind};
use crate::{color, file_dialog};

/// The setting that remembers the box is ticked, so nobody has to tick it for
/// every report.
pub const CONSENT_SETTING: &str = "bericht_senden_bestaetigt";
/// How long after the last keystroke the report is put together again.
const DEBOUNCE_SECONDS: f64 = 0.4;
/// A gap this long between two drawings of the page counts as opening it again.
const REOPEN_SECONDS: f64 = 1.0;
/// So many lines the report box shows without scrolling.
const BOX_ROWS: f32 = 16.0;
/// The interface library the report names (exactly pinned in the workspace).
const RUNTIME: &str = "Rust · egui 0.37";

/// The sharp rendering for the screen line (`b_scharf` of 4.12): the window draws in the pixels of the screen,
/// with the factor of the system scaling; at 100 % (or unknown) there is nothing to sharpen.
fn sharp_text(native: Option<f32>) -> String {
    match native.filter(|factor| (factor - 1.0).abs() > 0.001) {
        Some(factor) => tf("b_scharf", &[("0", Arg::Float(f64::from(factor)))]),
        None => t("b_scharf_ohne"),
    }
}

/// How a message under the buttons is colored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Good,
    Bad,
    Quiet,
}

/// What the sending uses: the real network or a transport given (tests, previews).
#[derive(Clone)]
pub enum SendVia {
    Network,
    Transport(Arc<dyn Transport>),
}

/// A receiver on this computer for previews: it answers every report with
/// "ok" and writes down how much came. Nothing it receives leaves the
/// computer.
pub struct TestReceiver {
    address: String,
    seen: Arc<Mutex<Vec<(usize, usize)>>>,
}

impl TestReceiver {
    /// Listens on a free port of `127.0.0.1`; `None` if that fails.
    pub fn start() -> Option<TestReceiver> {
        let listener = TcpListener::bind("127.0.0.1:0").ok()?;
        let address = format!("http://127.0.0.1:{}/bericht", listener.local_addr().ok()?.port());
        let seen: Arc<Mutex<Vec<(usize, usize)>>> = Arc::new(Mutex::new(Vec::new()));
        let record = Arc::clone(&seen);
        std::thread::Builder::new()
            .name("test-empfaenger".into())
            .spawn(move || {
                for stream in listener.incoming().filter_map(Result::ok) {
                    if let Some(request) = Self::read_request(stream)
                        && let Ok(mut list) = record.lock()
                    {
                        list.push(request);
                    }
                }
            })
            .ok()?;
        Some(TestReceiver { address, seen })
    }

    /// Reads one request, answers it with 200; (bytes of the body, files in it).
    fn read_request(mut stream: TcpStream) -> Option<(usize, usize)> {
        stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
        let mut data = Vec::new();
        let mut chunk = [0u8; 8192];
        let (mut start, mut wanted) = (None, 0usize);
        while let Ok(read) = stream.read(&mut chunk) {
            if read == 0 {
                break;
            }
            data.extend_from_slice(&chunk[..read]);
            if start.is_none()
                && let Some(at) = data.windows(4).position(|w| w == b"\r\n\r\n")
            {
                start = Some(at + 4);
                let head = String::from_utf8_lossy(&data[..at]).to_lowercase();
                wanted = head
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:").map(|v| v.trim().parse().unwrap_or(0)))
                    .unwrap_or(0);
            }
            if start.is_some_and(|s| data.len() >= s + wanted) {
                break;
            }
        }
        let body = &data[start?..];
        let files = body.windows(12).filter(|w| *w == b"name=\"files[").count();
        let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        Some((body.len(), files))
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    /// (bytes, files) of every report that arrived.
    pub fn received(&self) -> Vec<(usize, usize)> {
        self.seen.lock().map(|l| l.clone()).unwrap_or_default()
    }
}

/// An image the player attached.
#[derive(Clone, Debug)]
pub struct Picture {
    pub name: String,
    data: Vec<u8>,
    kind: ImageKind,
}

/// What the report was made from; a change of one of them makes it out of date.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Inputs {
    reporter: String,
    message: String,
    lang: Lang,
}

/// A finished report.
struct Shown {
    text: String,
    inputs: Inputs,
    millis: u128,
}

pub struct DiagnoseState {
    paths: Arc<Paths>,
    guard: writeguard::State,
    env: Environment,
    target: Target,
    via: SendVia,
    open_url: fn(&str) -> bool,
    reporter: String,
    /// The name the field got by itself (the game account); only a different
    /// one is stored.
    suggested: String,
    message: String,
    pictures: Vec<Picture>,
    consent: bool,
    shown: Option<Shown>,
    job: Option<Receiver<Shown>>,
    last_edit: f64,
    sending: Option<Receiver<Result<(), SendFailure>>>,
    notice: Option<(String, Tone)>,
    dialog: Option<file_dialog::Pending>,
    receiver: Option<TestReceiver>,
    /// When the page was last drawn: a long gap means it was opened anew.
    last_shown: f64,
    /// The page was opened anew: the report in the box may be old and is made
    /// again (the log and the files may have changed meanwhile).
    stale: bool,
    /// The taught images of the signature scanner go along with the report: asked for each time, never kept.
    scan_consent: bool,
    /// How many taught images there are, and when they were counted.
    scan_count: usize,
    scan_counted: f64,
    /// The page runs on a copy of the data (`--daten`): why nothing is sent is said for the copy.
    copy: bool,
    /// The sync of the program, for the Basetool line of the report.
    basetool: Option<SyncHandle>,
    /// "Send" was clicked once with an empty sentence: the player was asked for one, the next click sends.
    asked_for_sentence: bool,
}

impl DiagnoseState {
    /// The page for a data folder. `target` decides where "send" goes:
    /// [`Target::from_host`] in the program, [`Target::off`] or
    /// [`Target::local`] in previews and tests.
    pub fn new(paths: Arc<Paths>, env: Environment, target: Target, via: SendVia) -> DiagnoseState {
        let reporter = facts_of::reporter_name(&paths);
        let consent = paths.setting_bool(CONSENT_SETTING, false);
        DiagnoseState {
            guard: writeguard::state(),
            suggested: reporter.clone(),
            reporter,
            consent,
            paths,
            env,
            target,
            via,
            open_url: browser::open_in_browser,
            message: String::new(),
            pictures: Vec::new(),
            shown: None,
            job: None,
            last_edit: f64::NEG_INFINITY,
            sending: None,
            notice: None,
            dialog: None,
            receiver: None,
            last_shown: f64::NEG_INFINITY,
            stale: false,
            scan_consent: false,
            scan_count: 0,
            scan_counted: f64::NEG_INFINITY,
            copy: false,
            basetool: None,
            asked_for_sentence: false,
        }
    }

    /// The page runs on a copy of the data (`--daten`).
    pub fn on_copy(mut self, copy: bool) -> DiagnoseState {
        self.copy = copy;
        self
    }

    /// Why nothing can be sent: on a copy of the data the copy, else the preview.
    fn off_text(&self) -> String {
        t(if self.copy { "s_div_ziel_kopie" } else { "s_div_ziel_aus" })
    }

    /// Sends to a test receiver on this computer instead of the target given
    /// (previews).
    pub fn with_receiver(mut self, receiver: TestReceiver) -> DiagnoseState {
        self.target = Target::local(receiver.address());
        self.receiver = Some(receiver);
        self
    }

    /// Replaces what opens an address (tests do not open a browser).
    pub fn with_opener(mut self, open_url: fn(&str) -> bool) -> DiagnoseState {
        self.open_url = open_url;
        self
    }

    /// Where "send" goes from now on; a test receiver set before stays.
    pub fn set_target(&mut self, target: Target) {
        if self.receiver.is_none() {
            self.target = target;
        }
    }

    /// Where "send" goes.
    pub fn target(&self) -> &Target {
        &self.target
    }

    /// The page was opened: the name follows the chosen game account as long
    /// as the player did not enter one of their own.
    pub fn opened(&mut self) {
        self.stale = true;
        if facts_of::own_reporter(&self.paths).is_some() || self.reporter.trim() != self.suggested {
            return;
        }
        self.suggested = facts_of::reporter_name(&self.paths);
        self.reporter.clone_from(&self.suggested);
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn set_message(&mut self, text: &str) {
        self.message = text.to_string();
    }

    pub fn set_reporter(&mut self, name: &str) {
        self.reporter = name.to_string();
    }

    pub fn pictures(&self) -> &[Picture] {
        &self.pictures
    }

    /// The report as the box shows it, once there is one.
    pub fn shown_text(&self) -> Option<&str> {
        self.shown.as_ref().map(|s| s.text.as_str())
    }

    pub fn notice(&self) -> Option<&(String, Tone)> {
        self.notice.as_ref()
    }

    pub fn is_sending(&self) -> bool {
        self.sending.is_some()
    }

    /// How many reports the test receiver got (tests of the window).
    #[cfg(test)]
    pub(crate) fn test_receiver_count(&self) -> usize {
        self.receiver.as_ref().map_or(0, |r| r.received().len())
    }

    /// Ticks the box (tests of the window).
    #[cfg(test)]
    pub(crate) fn set_consent_for_test(&mut self) {
        self.consent = true;
    }

    fn inputs(&self) -> Inputs {
        Inputs {
            reporter: self.reporter.trim().to_string(),
            message: self.message.trim().to_string(),
            lang: language::current(),
        }
    }

    fn say(&mut self, text: String, tone: Tone) {
        self.notice = Some((text, tone));
    }

    // ---------------------------------------------------------- the report

    fn environment(&self, ctx: &egui::Context) -> Environment {
        let mut env = self.env.clone();
        env.runtime.get_or_insert_with(|| RUNTIME.to_string());
        env.packaging.get_or_insert_with(|| t(if cfg!(debug_assertions) { "b_v_quellcode" } else { "b_v_exe" }));
        let lang = language::current();
        if env.overlay.is_none() {
            env.overlay = Some(facts_of::overlay_line(&self.paths, lang));
        }
        if env.basetool.is_none() {
            env.basetool = facts_of::basetool_line(&self.paths, lang, self.basetool.as_ref());
        }
        let native = ctx.native_pixels_per_point();
        env.scale = env.scale.or(native.map(f64::from));
        if env.screen.is_none() {
            let (monitor, window) = ctx.input(|i| (i.viewport().monitor_size, i.viewport().inner_rect));
            // Sizes in pixels, as the screen has them: egui counts in points.
            let pixels = ctx.pixels_per_point();
            let system_scale = (native.unwrap_or(1.0) * 100.0).round() as i64;
            env.screen = monitor.map(|size| {
                let (width, height) = ((size.x * pixels).round() as i64, (size.y * pixels).round() as i64);
                let mut line = tf(
                    "b_skalierung",
                    &[("0", Arg::Int(width)), ("1", Arg::Int(height)), ("2", Arg::Int(system_scale))],
                );
                if let Some(window) = window.filter(|w| w.width() > 50.0 && w.height() > 50.0) {
                    let smallest = crate::base_size() * pixels;
                    line.push_str(&tf(
                        "b_fenstermass",
                        &[
                            ("0", Arg::Int((window.width() * pixels).round() as i64)),
                            ("1", Arg::Int((window.height() * pixels).round() as i64)),
                            ("2", Arg::Int(smallest.x.round() as i64)),
                            ("3", Arg::Int(smallest.y.round() as i64)),
                        ],
                    ));
                    if smallest.y.round() as i64 > height {
                        line.push_str(&t("bv_fenster_zu_hoch"));
                    }
                }
                line.push_str(&sharp_text(native));
                line
            });
        }
        env
    }

    /// The sync of the program the Basetool line of the report is read from; none: not connected.
    pub fn set_basetool(&mut self, sync: Option<SyncHandle>) {
        self.basetool = sync;
    }

    /// Puts the report together: gathers the facts and sets them together.
    fn make(paths: &Paths, env: &Environment, inputs: Inputs) -> Shown {
        let started = Instant::now();
        // The name the player gives as reporter is private for the streamer mode; the report keeps it.
        versekit_core::streamer::learn(&inputs.reporter);
        let request = Request {
            paths,
            lang: inputs.lang,
            env,
            reporter: Some(&inputs.reporter),
            message: &inputs.message,
            error_count: ERRORS_SHOWN,
        };
        let facts = facts_of::collect(&request);
        let text = report::build(&facts, &Redactor::new(paths));
        Shown { text, inputs, millis: started.elapsed().as_millis() }
    }

    fn start_build(&mut self, ctx: &egui::Context) {
        let (paths, env, guard, inputs) =
            (Arc::clone(&self.paths), self.environment(ctx), self.guard.clone(), self.inputs());
        let (sender, receiver) = channel();
        let repaint = ctx.clone();
        self.stale = false;
        let started = std::thread::Builder::new().name("bericht-bauen".into()).spawn(move || {
            writeguard::adopt(&guard);
            let _ = sender.send(Self::make(&paths, &env, inputs));
            repaint.request_repaint();
        });
        if started.is_ok() {
            self.job = Some(receiver);
        }
    }

    /// The text an action works with: the box, or a fresh report when the box
    /// is out of date. The second value says the box was out of date.
    fn fresh_text(&mut self, ctx: &egui::Context) -> (String, bool) {
        let wanted = self.inputs();
        if let Some(shown) = self.shown.as_ref().filter(|s| !self.stale && s.inputs == wanted) {
            return (shown.text.clone(), false);
        }
        let shown = Self::make(&self.paths, &self.environment(ctx), wanted);
        let text = shown.text.clone();
        self.shown = Some(shown);
        self.job = None;
        self.stale = false;
        (text, true)
    }

    // ---------------------------------------------------------- frame upkeep

    fn tick(&mut self, ctx: &egui::Context) {
        if let Some(job) = &self.job {
            match job.try_recv() {
                Ok(shown) => {
                    self.shown = Some(shown);
                    self.job = None;
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(100)),
                Err(TryRecvError::Disconnected) => self.job = None,
            }
        }
        if let Some(sending) = &self.sending {
            match sending.try_recv() {
                Ok(result) => {
                    self.sending = None;
                    self.sent(result);
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(100)),
                Err(TryRecvError::Disconnected) => self.sending = None,
            }
        }
        if let Some(answer) = self.dialog.as_ref().and_then(file_dialog::Pending::take) {
            self.dialog = None;
            if let Some(path) = answer {
                self.attach_path(&path);
            }
        }
        let now = ctx.input(|i| i.time);
        let up_to_date = !self.stale && self.shown.as_ref().is_some_and(|s| s.inputs == self.inputs());
        if self.job.is_none() && !up_to_date {
            let wait = DEBOUNCE_SECONDS - (now - self.last_edit);
            if wait <= 0.0 {
                self.start_build(ctx);
            } else {
                ctx.request_repaint_after(Duration::from_secs_f64(wait));
            }
        }
    }

    // ---------------------------------------------------------- name, images

    /// Stores the name when it differs from the one the field got by itself;
    /// an emptied field is stored as emptied on purpose.
    fn commit_reporter(&mut self) {
        let name = self.reporter.trim().to_string();
        if name != self.suggested {
            // The mark first: if writing the name fails, the old state holds,
            // never an empty name without the mark.
            if name.is_empty() {
                self.paths.set_setting(facts_of::REPORTER_CLEARED, Value::Bool(true));
            }
            self.paths.set_setting(facts_of::REPORTER_SETTING, Value::str(&name));
            self.suggested = name;
        }
    }

    /// Takes an image the player chose: only real PNG and JPEG, hidden
    /// additions removed, at most four, all together within the budget.
    pub fn attach_path(&mut self, path: &str) {
        if self.pictures.len() >= send::IMAGE_LIMIT {
            let text = tf("s_di_bild_voll", &[("0", Arg::from(send::IMAGE_LIMIT))]);
            self.say(text, Tone::Bad);
            return;
        }
        let used: usize = self.pictures.iter().map(|p| p.data.len()).sum();
        let too_large = || tf("s_div_bild_zugross", &[("0", Arg::from(send::IMAGE_BUDGET / (1024 * 1024)))]);
        match send::load_image(path) {
            Err("s_div_bild_zugross") => self.say(too_large(), Tone::Bad),
            Err(key) => self.say(t(key), Tone::Bad),
            Ok((data, _)) if used + data.len() > send::IMAGE_BUDGET => self.say(too_large(), Tone::Bad),
            Ok((data, kind)) => {
                let name = std::path::Path::new(path)
                    .file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                let text = tf("s_di_bild_dabei", &[("0", Arg::from(name.as_str()))]);
                self.pictures.push(Picture { name, data, kind });
                // Choosing an image is the wish to send it: the switch turns on where the player sees it.
                if !self.consent {
                    self.consent = true;
                    self.paths.set_setting(CONSENT_SETTING, Value::Bool(true));
                }
                self.say(text, Tone::Good);
            }
        }
    }

    fn remove_picture(&mut self, index: usize) {
        if index < self.pictures.len() {
            let gone = self.pictures.remove(index);
            let text = tf("s_di_bild_weg", &[("0", Arg::from(gone.name.as_str()))]);
            self.say(text, Tone::Quiet);
        }
    }

    // ---------------------------------------------------------- the three ways out

    /// The sentence is spent once the report went out by any way; the box
    /// stays as it is, so a second copy holds the same text.
    fn message_used(&mut self) {
        if self.message.trim().is_empty() {
            return;
        }
        self.message.clear();
        if let Some(shown) = &mut self.shown {
            shown.inputs.message.clear();
        }
    }

    pub fn copy(&mut self, ctx: &egui::Context) {
        self.commit_reporter();
        let (text, _) = self.fresh_text(ctx);
        ctx.copy_text(text);
        self.say(t("s_di_kopiert"), Tone::Good);
        self.message_used();
    }

    pub fn open_issue(&mut self, ctx: &egui::Context) {
        self.commit_reporter();
        let (text, _) = self.fresh_text(ctx);
        let lang = language::current();
        if (self.open_url)(&send::issue_url(&text, "", send::template_for(lang), lang)) {
            self.say(t("s_di_browser_ok"), Tone::Good);
            self.message_used();
        } else {
            self.say(t("s_di_browser_weg"), Tone::Bad);
        }
    }

    /// The folder of the taught images of the signature scanner and the files of the templates taught.
    fn scan_files(&self) -> Option<(String, versekit_core::signature_scan::OwnFiles)> {
        use versekit_core::signature_scan as scan;
        let own = scan::OwnFiles::in_folder(|name| self.paths.app_file(name));
        let folder =
            std::path::Path::new(&own.shared).parent()?.join(scan::SAMPLE_FOLDER).to_string_lossy().into_owned();
        Some((folder, own))
    }

    /// How many images there are to send: the taught ones of the signature scanner and the kept ones of the
    /// refinery scanner; counted again at most every few seconds.
    fn scan_images(&mut self, now: f64) -> usize {
        if now - self.scan_counted >= 5.0 {
            self.scan_counted = now;
            self.scan_count =
                self.scan_files().map_or(0, |(folder, _)| versekit_core::signature_scan::samples(&folder).len())
                    + versekit_core::refinery_scan::screen::kept_images(&self.paths).len();
        }
        self.scan_count
    }

    /// Sends the report on a click: only with the box ticked, only the text
    /// the player has seen, and only to the target given.
    pub fn send(&mut self, ctx: &egui::Context) {
        if !self.consent {
            self.say(t("s_di_erst_bestaetigen"), Tone::Bad);
            return;
        }
        if self.sending.is_some() {
            return;
        }
        if !self.target.available() {
            self.say(SendFailure::NoTarget.text(language::current()), Tone::Bad);
            return;
        }
        // Without a sentence the first click asks for one; the second sends all the same.
        if self.message.trim().is_empty() && !self.asked_for_sentence {
            self.asked_for_sentence = true;
            self.say(t("s_di_ohne_satz"), Tone::Bad);
            return;
        }
        self.asked_for_sentence = false;
        self.commit_reporter();
        let (text, was_stale) = self.fresh_text(ctx);
        if was_stale {
            // The box did not show this text yet: let the player read it first.
            self.say(t("s_div_aktualisiert"), Tone::Quiet);
            return;
        }
        let images: Vec<(Vec<u8>, ImageKind)> = self.pictures.iter().map(|p| (p.data.clone(), p.kind)).collect();
        let scans = self.scan_consent.then(|| self.scan_files().map(|files| (self.reporter.clone(), files))).flatten();
        let refinery = versekit_core::refinery_scan::screen::keep_folder(&self.paths);
        let (target, via, version, guard) =
            (self.target.clone(), self.via.clone(), self.env.version.clone(), self.guard.clone());
        let host = self.paths.host.clone();
        let (sender, receiver) = channel();
        let repaint = ctx.clone();
        let started = std::thread::Builder::new().name("bericht-senden".into()).spawn(move || {
            writeguard::adopt(&guard);
            let transport: Result<Arc<dyn Transport>, SendFailure> = match via {
                SendVia::Transport(transport) => Ok(transport),
                SendVia::Network => HttpTransport::new("", &host)
                    .map(|t| Arc::new(t) as Arc<dyn Transport>)
                    .map_err(|_| SendFailure::Unreachable),
            };
            let mut attachments: Vec<Attachment> = send::image_files(&images);
            if let Some((who, (folder, own))) = scans
                && let Some(data) = versekit_core::signature_scan::sample_archive(&who, &folder, &own, &refinery)
            {
                attachments.push(Attachment {
                    name: versekit_core::signature_scan::archive_name(&who),
                    data,
                    mime: "application/zip".to_string(),
                });
            }
            let result = transport.and_then(|transport| {
                send::submit(transport.as_ref(), &target, &text, &version, &stamp_now(), &attachments)
            });
            let _ = sender.send(result);
            repaint.request_repaint();
        });
        if started.is_ok() {
            self.sending = Some(receiver);
            self.say(t("s_di_ab_laeuft"), Tone::Quiet);
        } else {
            self.say(SendFailure::Unreachable.text(language::current()), Tone::Bad);
        }
    }

    fn sent(&mut self, result: Result<(), SendFailure>) {
        // The tick belongs to this one report, sent or not: the next one asks again.
        self.scan_consent = false;
        match result {
            Ok(()) => {
                self.say(t("s_di_ab_ok"), Tone::Good);
                // Only after success: if sending fails the text stays, the
                // player may want to try again.
                self.message_used();
                self.pictures.clear();
            }
            Err(failure) => {
                let reason = failure.text(language::current());
                self.say(tf("s_di_ab_weg", &[("0", Arg::from(reason.as_str()))]), Tone::Bad);
            }
        }
    }
}

/// `YYYY-MM-DD-HHMM` of now, for the file name of the report.
fn stamp_now() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0);
    formats::local_time(seconds).map(|time| formats::strftime("%Y-%m-%d-%H%M", &time)).unwrap_or_default()
}

// ------------------------------------------------------------ the page

fn small(ui: &mut Ui, text: &str, tone: egui::Color32) {
    ui.add(egui::Label::new(RichText::new(text).small().color(tone)).wrap());
}

/// A row with name and hint above, the control full width below.
fn wide_row<R>(ui: &mut Ui, scheme: &Scheme, name: &str, hint: &str, control: impl FnOnce(&mut Ui) -> R) -> R {
    ui.add(egui::Label::new(name).wrap());
    small(ui, hint, color(scheme.sub));
    ui.add_space(4.0);
    let inner = control(ui);
    ui.add_space(GAP);
    inner
}

/// The page.
pub fn show(ui: &mut Ui, scheme: &Scheme, state: &mut DiagnoseState) {
    let now = ui.input(|i| i.time);
    if now - state.last_shown > REOPEN_SECONDS {
        state.opened();
    }
    state.last_shown = now;
    state.tick(ui.ctx());
    let sub = color(scheme.sub);
    small(ui, &t("s_div_lead"), sub);
    match (state.target.address(), state.target.available()) {
        (_, false) => small(ui, &state.off_text(), sub),
        (Some(address), true) if address.starts_with("http://") => small(ui, &t("s_div_ziel_lokal"), sub),
        _ => {}
    }
    if let Some(receiver) = &state.receiver {
        let seen = receiver.received();
        let line = match seen.last() {
            None => t("s_div_empfaenger_leer"),
            Some(&(bytes, files)) => {
                let args = [("0", Arg::from(seen.len())), ("1", Arg::from(bytes)), ("2", Arg::from(files))];
                tf("s_div_empfaenger", &args)
            }
        };
        small(ui, &line, sub);
        ui.ctx().request_repaint_after(Duration::from_millis(500));
    }
    ui.add_space(GAP);

    // The three ways out stay at the top, in sight without scrolling.
    ui.horizontal_wrapped(|row| {
        let send_word = t("s_di_absenden");
        // No target: the button is locked, with the reason, instead of answering only after a click.
        let off = (!state.target.available()).then(|| state.off_text());
        let send = widgets::Button::new(&send_word, ButtonKind::Danger, scheme).icon("send").blocked(off.as_deref());
        if row.add(send).clicked() {
            state.send(row.ctx());
        }
        let issue_word = t("s_di_melden");
        if row.add(widgets::Button::new(&issue_word, ButtonKind::Strong, scheme).icon("external-link")).clicked() {
            state.open_issue(row.ctx());
        }
        let copy_word = t("s_di_kopieren");
        if row.add(widgets::Button::new(&copy_word, ButtonKind::Normal, scheme).icon("copy")).clicked() {
            state.copy(row.ctx());
        }
    });
    ui.add_space(4.0);
    let consent_word = t("s_di_zustimmung");
    if widgets::switch_row(ui, scheme, &consent_word, &mut state.consent).changed() {
        state.paths.set_setting(CONSENT_SETTING, Value::Bool(state.consent));
    }
    // The images of the scanners (taught signatures, kept refinery readings): only where there are some, and
    // asked each time.
    let scans = state.scan_images(ui.input(|i| i.time));
    if scans > 0 {
        let word = tf("s_di_zustimmung_bilder", &[("0", Arg::from(scans))]);
        widgets::switch_row(ui, scheme, &word, &mut state.scan_consent);
    }
    if let Some((text, tone)) = &state.notice {
        let tone = match tone {
            Tone::Good => scheme.accent,
            Tone::Bad => scheme.red,
            Tone::Quiet => scheme.sub,
        };
        small(ui, text, color(tone));
    }
    ui.add_space(GAP);

    // The name.
    let now = ui.input(|i| i.time);
    let hint = t("s_pl_melder");
    wide_row(ui, scheme, &t("s_melder"), &t("s_melderv_h"), |ui| {
        let before = state.reporter.clone();
        let response = ui.add(widgets::text_field(&mut state.reporter, &hint, scheme).desired_width(f32::INFINITY));
        if response.changed() && state.reporter != before {
            state.last_edit = now;
        }
        if response.lost_focus() {
            state.commit_reporter();
        }
    });

    // The sentence of the player.
    let message_hint = t("s_meldung");
    wide_row(ui, scheme, &message_hint, &t("s_meldungv_h"), |ui| {
        let edit =
            widgets::text_area(&mut state.message, &message_hint, scheme).desired_rows(4).desired_width(f32::INFINITY);
        if ui.add(edit).changed() {
            state.last_edit = now;
        }
        if state.asked_for_sentence && state.message.trim().is_empty() {
            small(ui, &t("s_di_ohne_satz_feld"), color(scheme.red));
        }
    });

    // The images.
    let budget_mb = send::IMAGE_BUDGET / (1024 * 1024);
    let images_hint = tf("s_bilderv_h", &[("0", Arg::from(send::IMAGE_LIMIT)), ("1", Arg::from(budget_mb))]);
    wide_row(ui, scheme, &t("s_bilder"), &images_hint, |ui| {
        let word = t("s_di_bild_waehlen");
        if ui.add(widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("image-plus")).clicked() {
            if state.pictures.len() >= send::IMAGE_LIMIT {
                let text = tf("s_di_bild_voll", &[("0", Arg::from(send::IMAGE_LIMIT))]);
                state.say(text, Tone::Bad);
            } else {
                let filters: [(&str, &[&str]); 1] = [("PNG / JPG", &["png", "jpg", "jpeg"])];
                state.dialog = Some(file_dialog::open_file(ui.ctx(), &t("s_di_bild_titel"), &filters, None));
            }
        }
        let mut gone = None;
        for (index, picture) in state.pictures.iter().enumerate() {
            ui.horizontal(|row| {
                row.label(&picture.name);
                let meaning = tf("s_di_bild_weg", &[("0", Arg::from(picture.name.as_str()))]);
                let remove = widgets::icon_button(row, scheme, "x", &meaning);
                remove.widget_info(|| WidgetInfo::labeled(Role::Button, true, &meaning));
                if remove.clicked() {
                    gone = Some(index);
                }
            });
        }
        if let Some(index) = gone {
            state.remove_picture(index);
        }
    });

    // The report.
    ui.add(egui::Label::new(RichText::new(t("s_div_bericht_name")).strong()));
    report_box(ui, scheme, state);
    ui.add_space(GAP);
    widgets::status_box(ui, scheme, scheme.accent, "shield-check", &t("s_di_sicher"), &t("s_div_sicher_h"));
    ui.add_space(GAP);
    small(ui, &t("s_div_ohne_github"), sub);
    ui.add_space(GAP);
}

/// The report in a read-only box that scrolls both ways.
fn report_box(ui: &mut Ui, scheme: &Scheme, state: &DiagnoseState) {
    let font = TextStyle::Monospace.resolve(ui.style());
    let height = ui.fonts_mut(|f| f.row_height(&font)) * BOX_ROWS + 24.0;
    let name = t("s_div_bericht_name");
    let Some(shown) = &state.shown else {
        small(ui, &t("s_div_wird_gebaut"), color(scheme.sub));
        return;
    };
    let mut text: &str = &shown.text;
    egui::ScrollArea::both()
        .id_salt("bericht-kasten")
        .max_height(height)
        .min_scrolled_height(height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut text)
                    .font(TextStyle::Monospace)
                    .hint_text(RichText::new(&name))
                    .desired_width(f32::INFINITY)
                    .desired_rows(BOX_ROWS as usize)
                    .lock_focus(false),
            );
        });
    small(ui, &tf("s_div_fertig_in", &[("0", Arg::Int(i64::try_from(shown.millis).unwrap_or(0)))]), color(scheme.sub));
}

#[cfg(test)]
mod tests {
    use super::*;
    use versekit_core::basetool::error::ApiError;
    use versekit_core::basetool::http::{Request as HttpRequest, Response};
    use versekit_core::paths::Host;
    use versekit_core::pypath::Flavor;

    /// The language is one for the whole process: German and held, so the
    /// texts the page shows are the ones the test asks for.
    fn german() -> std::sync::MutexGuard<'static, ()> {
        let guard = crate::language_lock();
        language::set_language(language::Choice::De);
        guard
    }

    /// A transport that only records and never reaches a network.
    struct Recording {
        seen: std::sync::Mutex<Vec<HttpRequest>>,
        status: u16,
    }

    impl Transport for Recording {
        fn send(&self, request: &HttpRequest) -> Result<Response, ApiError> {
            self.seen.lock().map(|mut s| s.push(request.clone())).ok();
            Ok(Response { status: self.status, ..Default::default() })
        }
    }

    fn state_in(name: &str, status: u16) -> (DiagnoseState, Arc<Recording>, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("versekit-fehlerseite-{name}-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("Ordner");
        let value = dir.to_string_lossy().to_string();
        let vars = [("SC_BP_HOME", value.as_str()), ("HOME", "/home/spieler"), ("USERPROFILE", "/home/spieler")];
        let paths = Arc::new(Paths::new(Host::new(Flavor::native(), &vars, "/")));
        let transport = Arc::new(Recording { seen: std::sync::Mutex::new(Vec::new()), status });
        let env = Environment { version: "5.0.0-test".into(), ..Default::default() };
        let mut state = DiagnoseState::new(
            paths,
            env,
            Target::local("http://127.0.0.1:9/bericht"),
            SendVia::Transport(transport.clone()),
        )
        .with_opener(|_| true);
        // A sentence of the player, so that one click sends; the empty case has a test of its own.
        state.message = "Probe".into();
        (state, transport, dir)
    }

    #[test]
    fn without_a_sentence_the_first_click_asks_and_the_second_sends() {
        let _german = german();
        let (mut state, transport, dir) = state_in("ohne-satz", 200);
        state.message.clear();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        wait_for_report(&ctx, &mut state);
        state.consent = true;
        state.send(&ctx);
        assert!(transport.seen.lock().map(|s| s.is_empty()).unwrap_or(false), "the first click only asks");
        assert_eq!(state.notice().map(|n| n.0.clone()), Some(t("s_di_ohne_satz")));
        state.send(&ctx);
        for _ in 0..200 {
            run_frame(&ctx, &mut state, Vec::new());
            if !state.is_sending() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(transport.seen.lock().map(|s| s.len()).unwrap_or(0), 1, "the second click sends");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn run_frame(ctx: &egui::Context, state: &mut DiagnoseState, events: Vec<egui::Event>) -> egui::FullOutput {
        let scheme = versekit_core::theme::current();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 3000.0))),
            time: Some(f64::from(events.len() as u32)),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| show(ui, scheme, state));
        output.textures_delta.clear();
        output
    }

    /// Frames until the report is there.
    fn wait_for_report(ctx: &egui::Context, state: &mut DiagnoseState) {
        for _ in 0..200 {
            run_frame(ctx, state, Vec::new());
            if state.shown_text().is_some() && state.job.is_none() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the report did not come");
    }

    #[test]
    fn the_report_is_made_on_a_thread_and_shown() {
        let _german = german();
        let (mut state, _, dir) = state_in("bauen", 200);
        let ctx = egui::Context::default();
        crate::install(&ctx);
        run_frame(&ctx, &mut state, Vec::new());
        assert!(state.shown_text().is_none() || state.job.is_some() || state.shown.is_some());
        wait_for_report(&ctx, &mut state);
        let text = state.shown_text().unwrap_or_default();
        assert!(text.starts_with("VerseKit 5.0.0-test · "), "{text}");
        assert!(text.lines().any(|l| l.starts_with("Von ")), "{text}");
        for heading in
            ["## System", "## Spiel", "## Bestand", "## Ordner und Einstellungen", "## Letzte Fehler", "## Schwärzung"]
        {
            assert!(text.lines().any(|l| l == heading), "section {heading} missing:\n{text}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_report_gets_the_overlay_the_basetool_the_scaling_and_the_sharp_rendering_from_the_page() {
        let _german = german();
        let (mut state, _, dir) = state_in("umgebung", 200);
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 700.0))),
            ..Default::default()
        };
        let info = egui::ViewportInfo {
            native_pixels_per_point: Some(1.25),
            monitor_size: Some(egui::vec2(1536.0, 864.0)),
            ..Default::default()
        };
        input.viewports.insert(egui::ViewportId::ROOT, info);
        let mut output = ctx.run_ui(input, |_| {});
        output.textures_delta.clear();
        let env = state.environment(&ctx);
        assert_eq!(env.scale, Some(1.25));
        let screen = env.screen.clone().unwrap_or_default();
        assert!(
            screen.contains("Skalierung 125 %") && screen.ends_with(" · scharfe Darstellung an (Faktor 1.25)"),
            "{screen}"
        );
        let overlay = env.overlay.clone().unwrap_or_default();
        assert!(overlay.starts_with("gemerkter Platz — · Ecke frei"), "{overlay}");
        assert_eq!(env.basetool, None, "no area switched on: no line");
        assert!(state.paths.set_setting(versekit_core::basetool::scopes::SETTING_SHIPS, Value::Bool(true)));
        state.set_basetool(None);
        assert_eq!(
            state.environment(&ctx).basetool.as_deref(),
            Some("Verbindung aus · Rechte — · zuletzt abgeglichen — · letzte Absage —")
        );
        assert_eq!(sharp_text(Some(1.5)), " · scharfe Darstellung an (Faktor 1.50)");
        assert_eq!(sharp_text(Some(1.0)), t("b_scharf_ohne"));
        assert_eq!(sharp_text(None), t("b_scharf_ohne"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_report_is_made_again_when_the_page_is_opened_anew() {
        let _german = german();
        let (mut state, _, dir) = state_in("neu-geoeffnet", 200);
        let ctx = egui::Context::default();
        crate::install(&ctx);
        wait_for_report(&ctx, &mut state);
        let marker = "Zwischenfall-nach-dem-Bauen";
        assert!(!state.shown_text().unwrap_or_default().contains(marker));
        let log = versekit_core::error_log::ErrorLog::new(&state.paths, "5.0.0-test");
        let problem = versekit_core::error_log::Problem { kind: "ValueError", message: marker, trace: None };
        assert!(log.record_at("test", Some(problem), "", "2026-01-01 10:00:00"));
        // Nothing changed in the inputs: without opening, the box stays.
        wait_for_report(&ctx, &mut state);
        assert!(!state.shown_text().unwrap_or_default().contains(marker));
        state.opened();
        wait_for_report(&ctx, &mut state);
        assert!(state.shown_text().unwrap_or_default().contains(marker));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_new_sentence_makes_the_report_out_of_date_and_it_is_made_again() {
        let _german = german();
        let (mut state, _, dir) = state_in("neu", 200);
        let ctx = egui::Context::default();
        crate::install(&ctx);
        wait_for_report(&ctx, &mut state);
        state.set_message("Die Liste öffnet nicht");
        state.last_edit = f64::NEG_INFINITY;
        wait_for_report(&ctx, &mut state);
        for _ in 0..50 {
            if state.shown_text().is_some_and(|t| t.contains("Die Liste öffnet nicht")) {
                break;
            }
            run_frame(&ctx, &mut state, Vec::new());
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(state.shown_text().unwrap_or_default().contains("Die Liste öffnet nicht"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Where the text `label` is drawn in the frame `output`.
    fn text_at(output: &egui::FullOutput, label: &str) -> Option<egui::Pos2> {
        fn walk(shape: &egui::Shape, label: &str) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(text) if text.galley.text() == label => Some(text.pos),
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| walk(s, label)),
                _ => None,
            }
        }
        output.shapes.iter().find_map(|clipped| walk(&clipped.shape, label))
    }

    /// A click on the text `label` of the page, as a person makes it.
    fn click_on(ctx: &egui::Context, state: &mut DiagnoseState, label: &str) {
        run_frame(ctx, state, Vec::new());
        let seen = run_frame(ctx, state, Vec::new());
        let at = text_at(&seen, label).unwrap_or_else(|| panic!("{label} is on the page")) + egui::vec2(3.0, 3.0);
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run_frame(ctx, state, vec![egui::Event::PointerMoved(at)]);
        run_frame(ctx, state, vec![button(true)]);
        run_frame(ctx, state, vec![button(false)]);
    }

    #[test]
    fn the_send_button_is_locked_while_there_is_no_target() {
        let _german = german();
        let (mut state, transport, dir) = state_in("ohne-ziel", 200);
        let ctx = egui::Context::default();
        crate::install(&ctx);
        wait_for_report(&ctx, &mut state);
        // Counter-check: with a target the click answers (the box is not ticked yet).
        click_on(&ctx, &mut state, &t("s_di_absenden"));
        assert_eq!(
            state.notice().map(|n| n.0.clone()),
            Some(t("s_di_erst_bestaetigen")),
            "the button works with a target"
        );
        // Without a target: the ticked box makes no difference, the locked button does nothing.
        state.target = Target::off();
        state.consent = true;
        state.notice = None;
        // A new window: the clock of the frames of a context only goes forward.
        let ctx = egui::Context::default();
        crate::install(&ctx);
        click_on(&ctx, &mut state, &t("s_di_absenden"));
        assert!(state.notice().is_none(), "a locked button says nothing after the click: {:?}", state.notice());
        assert!(transport.seen.lock().map(|s| s.is_empty()).unwrap_or(false));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sending_needs_the_box_and_sends_exactly_the_box() {
        let _german = german();
        let (mut state, transport, dir) = state_in("senden", 200);
        let ctx = egui::Context::default();
        crate::install(&ctx);
        wait_for_report(&ctx, &mut state);
        // Without the tick nothing goes out.
        state.send(&ctx);
        assert!(transport.seen.lock().map(|s| s.is_empty()).unwrap_or(false), "nothing may go without the tick");
        assert_eq!(state.notice().map(|n| n.0.clone()), Some(t("s_di_erst_bestaetigen")));
        // With the tick the text of the box goes, and only after the click.
        state.consent = true;
        state.send(&ctx);
        for _ in 0..200 {
            run_frame(&ctx, &mut state, Vec::new());
            if !state.is_sending() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let seen = transport.seen.lock().map(|s| s.clone()).unwrap_or_default();
        assert_eq!(seen.len(), 1);
        let body = String::from_utf8_lossy(seen[0].body.as_deref().unwrap_or_default()).into_owned();
        assert!(body.contains(state.shown_text().unwrap_or("?")), "the body holds the text of the box");
        assert_eq!(state.notice().map(|n| n.0.clone()), Some(t("s_di_ab_ok")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_taught_images_of_the_scanner_go_along_only_with_their_own_tick() {
        use versekit_core::signature_scan as scan;
        let _german = german();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let mut bodies = Vec::new();
        for ticked in [false, true] {
            let (mut state, transport, dir) = state_in(if ticked { "scan-ja" } else { "scan-nein" }, 200);
            wait_for_report(&ctx, &mut state);
            let (folder, _) = state.scan_files().expect("folder");
            assert!(scan::save_sample(&folder, &scan::Gray::new(4, 4, vec![9; 16]), "2000"));
            assert_eq!(state.scan_images(1000.0), 1);
            state.consent = true;
            state.scan_consent = ticked;
            state.send(&ctx);
            for _ in 0..200 {
                run_frame(&ctx, &mut state, Vec::new());
                if !state.is_sending() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            let seen = transport.seen.lock().map(|s| s.clone()).unwrap_or_default();
            assert_eq!(seen.len(), 1);
            bodies.push(String::from_utf8_lossy(seen[0].body.as_deref().unwrap_or_default()).into_owned());
            assert!(!state.scan_consent, "the tick is used up by the report it was given for");
            let _ = std::fs::remove_dir_all(&dir);
        }
        assert!(!bodies[0].contains("scan-bilder"), "without the tick no images go");
        assert!(bodies[1].contains("scan-bilder") && bodies[1].contains("application/zip"), "with it they do");
        assert!(bodies[1].contains("bilder/2000_"), "the right number stands in the name of the image");
    }

    #[test]
    fn the_tick_for_the_taught_images_is_used_up_whether_the_report_went_or_not() {
        let _german = german();
        for result in [Ok(()), Err(SendFailure::Unreachable)] {
            let (mut state, _transport, dir) = state_in("scan-verbraucht", 200);
            state.scan_consent = true;
            state.sent(result);
            assert!(!state.scan_consent);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn a_report_out_of_date_is_not_sent_but_shown_first() {
        let _german = german();
        let (mut state, transport, dir) = state_in("alt", 200);
        let ctx = egui::Context::default();
        crate::install(&ctx);
        wait_for_report(&ctx, &mut state);
        state.consent = true;
        state.set_message("neu getippt");
        state.send(&ctx);
        assert!(transport.seen.lock().map(|s| s.is_empty()).unwrap_or(false), "not sent while the box was behind");
        assert!(state.shown_text().unwrap_or_default().contains("neu getippt"), "the box now shows it");
        assert_eq!(state.notice().map(|n| n.0.clone()), Some(t("s_div_aktualisiert")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_send_keeps_the_sentence_and_the_images() {
        let _german = german();
        let (mut state, _, dir) = state_in("fehl", 429);
        let ctx = egui::Context::default();
        crate::install(&ctx);
        state.set_message("bleibt stehen");
        wait_for_report(&ctx, &mut state);
        for _ in 0..50 {
            if state.shown_text().is_some_and(|t| t.contains("bleibt stehen")) {
                break;
            }
            run_frame(&ctx, &mut state, Vec::new());
            std::thread::sleep(Duration::from_millis(10));
        }
        state.consent = true;
        state.send(&ctx);
        for _ in 0..200 {
            run_frame(&ctx, &mut state, Vec::new());
            if !state.is_sending() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(state.message(), "bleibt stehen");
        let reason = SendFailure::TooMany.text(language::current());
        assert_eq!(state.notice().map(|n| n.0.clone()), Some(tf("s_di_ab_weg", &[("0", Arg::from(reason.as_str()))])));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_target_given_later_takes_over_but_never_from_a_test_receiver() {
        let _german = german();
        let (mut state, _, dir) = state_in("ziel-spaeter", 200);
        state.set_target(Target::off());
        assert!(!state.target().available());
        state.set_target(Target::local("http://127.0.0.1:9/neu"));
        assert_eq!(state.target().address(), Some("http://127.0.0.1:9/neu"));
        // A test receiver asked for at the start stays where it is.
        let Some(receiver) = TestReceiver::start() else { panic!("receiver") };
        let address = receiver.address().to_string();
        let mut state = state.with_receiver(receiver);
        state.set_target(Target::off());
        assert_eq!(state.target().address(), Some(address.as_str()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn on_a_copy_of_the_data_the_page_says_the_copy_sends_nothing() {
        let _german = german();
        let (state, _, dir) = state_in("kopie-text", 200);
        let mut state = state.on_copy(true);
        state.set_target(Target::off());
        assert_eq!(state.off_text(), t("s_div_ziel_kopie"));
        let state = state.on_copy(false);
        assert_eq!(state.off_text(), t("s_div_ziel_aus"), "the preview keeps its own sentence");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn without_a_target_nothing_can_be_sent() {
        let _german = german();
        let (mut state, transport, dir) = state_in("ziel", 200);
        state.target = Target::off();
        state.consent = true;
        let ctx = egui::Context::default();
        state.send(&ctx);
        assert!(transport.seen.lock().map(|s| s.is_empty()).unwrap_or(false));
        assert_eq!(state.notice().map(|n| n.0.clone()), Some(t("m_bericht_kein_ziel")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn images_are_checked_by_content_and_limited() {
        let _german = german();
        let (mut state, _, dir) = state_in("bilder", 200);
        let folder = dir.join("bilder");
        std::fs::create_dir_all(&folder).expect("Ordner");
        let mut png = send::PNG_SIGNATURE.to_vec();
        for (kind, body) in [(*b"IHDR", vec![0u8; 13]), (*b"tEXt", b"Ort=Zuhause".to_vec()), (*b"IEND", Vec::new())] {
            png.extend((body.len() as u32).to_be_bytes());
            png.extend(kind);
            png.extend(&body);
            png.extend([0, 0, 0, 0]);
        }
        let good = folder.join("so-gut.png");
        std::fs::write(&good, &png).expect("Bild");
        let text = folder.join("kein-bild.png");
        std::fs::write(&text, b"kein Bild").expect("Datei");
        state.attach_path(&text.to_string_lossy());
        assert!(state.pictures().is_empty());
        assert_eq!(state.notice().map(|n| n.0.clone()), Some(t("s_di_bild_art")));
        assert!(!state.consent, "a refused file does not turn the switch on");
        state.attach_path(&good.to_string_lossy());
        assert!(state.consent, "an attached image turns the switch on");
        assert!(state.paths.setting_bool(CONSENT_SETTING, false), "and it is kept like a click on the switch");
        assert!(!state.scan_consent, "the scan images still need their own switch");
        for _ in 0..4 {
            state.attach_path(&good.to_string_lossy());
        }
        assert_eq!(state.pictures().len(), send::IMAGE_LIMIT);
        assert_eq!(
            state.notice().map(|n| n.0.clone()),
            Some(tf("s_di_bild_voll", &[("0", Arg::from(send::IMAGE_LIMIT))]))
        );
        assert!(!state.pictures()[0].data.windows(4).any(|w| w == b"tEXt"), "the hidden text chunk is gone");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_name_is_stored_only_when_it_differs_and_an_emptied_one_is_marked() {
        let _german = german();
        let (mut state, _, dir) = state_in("name", 200);
        state.set_reporter("Tester");
        state.commit_reporter();
        assert_eq!(facts_of::own_reporter(&state.paths), Some("Tester".to_string()));
        state.set_reporter("");
        state.commit_reporter();
        assert_eq!(facts_of::own_reporter(&state.paths), Some(String::new()), "emptied on purpose");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Nothing sticks out of the page, in both languages, in the plain and the
    /// square scheme, at the smallest and the biggest size, narrow and wide,
    /// with an image attached and a long sentence typed.
    #[test]
    fn the_page_fits_at_every_size() {
        let _german = german();
        let (mut state, _, dir) = state_in("passt", 200);
        state.set_message(&"Ein langer Satz ohne Ende, der umbrechen muss. ".repeat(12));
        let mut findings = Vec::new();
        for choice in [language::Choice::De, language::Choice::En] {
            language::set_language(choice);
            for id in ["original", "krt"] {
                versekit_core::theme::set_thread_current(id);
                for points in [0, 3, 7] {
                    for width in [400.0_f32, 880.0, 1800.0] {
                        let ctx = egui::Context::default();
                        crate::install(&ctx);
                        crate::apply_size(&ctx, points);
                        let scheme = versekit_core::theme::current();
                        let mut overflow = 0.0_f32;
                        for _ in 0..3 {
                            let input = egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    egui::vec2(width, 900.0),
                                )),
                                ..Default::default()
                            };
                            let mut output = ctx.run_ui(input, |ui| {
                                let shown = egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                                    ui.set_max_width(ui.available_width() - crate::layout::SAFETY);
                                    show(ui, scheme, &mut state);
                                });
                                overflow = (shown.content_size.x - shown.inner_rect.width()).max(0.0);
                            });
                            output.textures_delta.clear();
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        if overflow > 0.5 {
                            findings
                                .push(format!("{choice:?} {id} Größe {points} Breite {width}: {overflow:.0} zu breit"));
                        }
                    }
                }
            }
        }
        language::set_language(language::Choice::De);
        versekit_core::theme::set_thread_current(versekit_core::theme::DEFAULT);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(findings.is_empty(), "abgeschnitten ({}):\n{}", findings.len(), findings.join("\n"));
    }

    /// Every control of the page has a name for screen readers.
    #[test]
    fn the_controls_have_names() {
        let _german = german();
        let (mut state, _, dir) = state_in("namen", 200);
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        wait_for_report(&ctx, &mut state);
        let output = run_frame(&ctx, &mut state, Vec::new());
        let update = output.platform_output.accesskit_update.expect("accesskit is on");
        let labels: Vec<(Role, String)> = update
            .nodes
            .iter()
            .filter_map(|(_, n)| n.label().or(n.value()).map(|l| (n.role(), l.to_owned())))
            .collect();
        let has = |role: Role, name: &str| labels.iter().any(|(r, l)| *r == role && l.contains(name));
        assert!(has(Role::Button, &t("s_di_absenden")), "{labels:?}");
        assert!(has(Role::Button, &t("s_di_melden")), "{labels:?}");
        assert!(has(Role::Button, &t("s_di_kopieren")), "{labels:?}");
        assert!(has(Role::Button, &t("s_di_bild_waehlen")), "{labels:?}");
        assert!(has(Role::CheckBox, &t("s_di_zustimmung")), "{labels:?}");
        assert!(!labels.iter().any(|(_, l)| l.contains(&t("s_di_mit"))), "no switch for recording: {labels:?}");
        let fields =
            update.nodes.iter().filter(|(_, n)| n.role() == Role::TextInput || n.role() == Role::MultilineTextInput);
        for (_, node) in fields {
            let name = node.label().or(node.placeholder()).or(node.description()).unwrap_or_default();
            assert!(!name.is_empty(), "a text field without a name: {node:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
