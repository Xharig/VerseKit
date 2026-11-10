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

//! The thread that reads the Game.log and keeps the collection, like the
//! watcher of the Python version: first the kept logs are read back
//! (quietly, at most ten rows), then the running log is followed in the
//! interval of the settings. Only this thread writes the collection; a
//! request to read everything again is handed to it.

use versekit_core::language::SpawnWithLanguage as _;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

use versekit_core::catalog_cache::{self, CatalogCache};
use versekit_core::catalog_watch::{Item, Watch};
use versekit_core::collection::{self, Catalog, Collection, Known};
use versekit_core::contracts::Patterns;
use versekit_core::errors;
use versekit_core::export::{FileExports, now_unix};
use versekit_core::game_start;
use versekit_core::given::GivenBy;
use versekit_core::graphics_apply;
use versekit_core::jsonio::{self, Map};
use versekit_core::language::{self, Arg, Phrase};
use versekit_core::live_contracts::{Check, Line, Live, Out, Section, SectionScan, StartLog, hidden_load};
use versekit_core::logscan::Extra;
use versekit_core::logsource::{ContractScan, Gap, LogSource, LogTail, ReadState};
use versekit_core::mission_log::{ContractBlueprints, ini_files};
use versekit_core::mission_scan::MissionScan;
use versekit_core::overlay::{CAUGHT_UP_ROWS, Event, Message, RescanResult, Row, keys};
use versekit_core::paths::{Host, Paths, Recovered, name_key};
use versekit_core::phrases::{self, BackRoute, NoArchive};
use versekit_core::refinery_jobs;
use versekit_core::shard::Joins;
use versekit_core::stock_journal;
use versekit_core::watchlist;
use versekit_core::writeguard;

/// What the thread tells the window.
#[derive(Debug)]
pub enum News {
    Status(Message),
    Row(Row),
    /// A new blueprint: open, sound.
    Found,
    /// A short sound that nothing opens for: a refinery job that is done.
    Beep,
    /// Something happened that may open the overlay, if the player chose it (no sound).
    Opens(Event),
    /// Writes the lock refused in this thread.
    Denied(Vec<String>),
    /// The running contracts: the whole state of the bar.
    Contracts(Vec<Line>),
    /// A contract is gone: the hint rows of it go too.
    ContractGone(String),
    /// Something newly craftable in the game; `loud` for what the watch list waits for.
    Catalog {
        row: Row,
        loud: bool,
    },
    /// A message for the system (title, text), shown by the icon next to the clock.
    System(Phrase, Phrase),
    /// The `Data.p4k` the catalog is read from, said once at the start.
    Archive(Option<String>),
    /// A way to start the game is there (or gone): the rocket and the entry of the menu follow.
    Starter(bool),
    /// The joins of a shard in the running session: the whole state of the line.
    Shard(Joins),
    /// New lines of the log for the log monitor of the info bar (deaths, loading screens).
    LogLines(Vec<String>),
    /// The handle the log names as logged in: a click on a line of the log monitor searches the other player.
    OwnHandle(String),
}

/// What the window asks of the thread.
#[derive(Debug)]
pub enum Ask {
    Rescan,
    /// The player took a contract out of the bar by hand.
    Hide(String),
    /// The player took the hiding of a contract back.
    Unhide(String),
    /// The game ended: the contracts that stayed silent go.
    GameEnded,
}

/// The catalog the collection matches names against.
struct CacheCatalog<'a>(Option<&'a CatalogCache>);

impl Catalog for CacheCatalog<'_> {
    fn known(&self) -> Result<Known, String> {
        Ok(Known::new(self.0.iter().flat_map(|c| c.blueprints.iter()).map(|b| (b.key.clone(), Some(b.name.clone())))))
    }
}

/// Kind and code (`Military · S2 · A`) of each catalog blueprint by name key.
fn meta_table(catalog: Option<&CatalogCache>) -> HashMap<String, (String, String)> {
    let Some(catalog) = catalog else { return HashMap::new() };
    catalog
        .blueprints
        .iter()
        .map(|b| {
            let grade = b
                .grade
                .and_then(|g| u8::try_from(g).ok())
                .filter(|g| (1..=4).contains(g))
                .map(|g| char::from(b'A' + g - 1));
            (
                name_key(&b.name),
                (versekit_core::blueprints::kind_readable(&b.kind), code(b.class.as_deref(), b.size, grade)),
            )
        })
        .collect()
}

/// The class as the language file writes it (`Military` → „Militär“).
fn class_word(class: &str) -> String {
    let key = format!("s_ld_kl_{}", class.to_lowercase());
    language::text_in(language::current(), &key).map_or_else(|| class.to_string(), str::to_string)
}

/// `Militär · S2 · A`; parts that are unknown are left out.
fn code(class: Option<&str>, size: Option<i64>, grade: Option<char>) -> String {
    let mut parts = Vec::new();
    if let Some(class) = class.filter(|c| !c.is_empty()) {
        parts.push(class_word(class));
    }
    if let Some(size) = size {
        parts.push(format!("S{size}"));
    }
    if let Some(grade) = grade {
        parts.push(grade.to_string());
    }
    parts.join(" · ")
}

/// The code from the suffix behind the name in the log, for an item no
/// catalog knows yet (after a patch).
fn code_from_log(extra: Option<&Extra>) -> String {
    let Some((abbrev, size, grade)) = extra else { return String::new() };
    let class = match abbrev.to_lowercase().as_str() {
        "mil" => Some("Military"),
        "civ" => Some("Civilian"),
        "ind" => Some("Industrial"),
        "stl" | "ste" => Some("Stealth"),
        "com" | "cmp" | "cpt" => Some("Competition"),
        _ => None,
    };
    let size = size.as_deref().and_then(|s| s.parse().ok());
    let grade = grade.as_deref().and_then(|g| g.chars().next()).filter(char::is_ascii_alphabetic);
    code(class, size, grade)
}

fn clock() -> String {
    // YYYY-MM-DD hh:mm:ss: the time of day is the end.
    let now = collection::now_text();
    now.get(11..).unwrap_or(&now).to_string()
}

/// The thread's state while it runs.
struct Run<'p> {
    paths: &'p Paths,
    collection: Collection<'p>,
    data: Map,
    /// Size and time of `bestand.json` as this thread read or wrote it last: another one has written it
    /// (the main window ticks a blueprint, removes foreign ones) when the file looks different now.
    stamp: Option<(std::time::SystemTime, u64)>,
    seen: HashSet<String>,
    meta: HashMap<String, (String, String)>,
    pattern: regex::Regex,
    news: Sender<News>,
    wake: Box<dyn Fn() + Send>,
    /// A sentence went into the status line this round: it stands until the next one, the count waits.
    said: bool,
}

impl Run<'_> {
    fn tell(&self, news: News) {
        let _ = self.news.send(news);
        (self.wake)();
    }

    /// A sentence for the status line that stands until the next round (4.12 puts the count back only then).
    fn say(&mut self, text: Message) {
        self.said = true;
        self.tell(News::Status(text));
    }

    /// The count at the end of a round, unless a sentence was said in it.
    fn tell_count(&mut self, log_found: bool) {
        if !std::mem::take(&mut self.said) {
            let count = self.status(log_found);
            self.tell(News::Status(count));
        }
    }

    /// Gives what the contracts say to the window, in the order they came.
    fn tell_all(&self, outs: Vec<Out>) {
        for out in outs {
            let hint = matches!(out, Out::Hint { .. });
            self.tell(match out {
                Out::Stand(lines) => News::Contracts(lines),
                Out::Gone(title) => News::ContractGone(title),
                Out::Hint { text, contract } => News::Row(Row::Hint { text, contract: Some(contract) }),
            });
            if hint {
                self.tell(News::Opens(Event::Contract));
            }
        }
    }

    fn found_row(&self, name: &str, extra: Option<&Extra>, caught_up: bool) -> Row {
        let (kind, code) =
            self.meta.get(&name_key(name)).cloned().unwrap_or_else(|| (String::new(), code_from_log(extra)));
        Row::Found { name: name.to_string(), kind, code, time: clock(), caught_up }
    }

    fn save(&mut self) {
        let saved = self.collection.save(&mut self.data);
        self.stamp = self.file_stamp();
        if !saved {
            self.tell(News::Row(Row::Hint { text: Phrase::new("ov_bestand_fehler").into(), contract: None }));
        }
    }

    /// How `bestand.json` looks now.
    fn file_stamp(&self) -> Option<(std::time::SystemTime, u64)> {
        let meta = std::fs::metadata(self.collection.path()).ok()?;
        Some((meta.modified().ok()?, meta.len()))
    }

    /// Reads the collection again if someone else has written it since: this thread keeps the collection in
    /// memory and writes it whole, so without this a later find would take back what the window did.
    fn refresh(&mut self) {
        if self.file_stamp() == self.stamp {
            return;
        }
        self.data = self.collection.load();
        self.seen = collection::keys_seen_in_game(&self.data).into_iter().collect();
        self.stamp = self.file_stamp();
    }

    /// Takes caught up finds into the collection; at most ten as rows.
    fn take_caught_up(&mut self, finds: &[(String, Option<Extra>)]) -> Vec<String> {
        // Reading the logs takes seconds, in which the window may have written: take that over first, and
        // write before anyone else can.
        let _lock = versekit_core::jsonio::lock_saves();
        self.refresh();
        let mut added = Vec::new();
        for (name, _) in finds {
            if self.collection.add(&mut self.data, name, "nachlese", None) {
                added.push(name.clone());
            }
        }
        if !added.is_empty() {
            self.save();
            self.seen = collection::keys_seen_in_game(&self.data).into_iter().collect();
            if added.len() <= CAUGHT_UP_ROWS {
                for name in &added {
                    let row = self.found_row(name, None, true);
                    self.tell(News::Row(row));
                }
                // One sound and one opening for all of them.
                self.tell(News::Found);
            }
        }
        added
    }

    fn status(&self, log_found: bool) -> Message {
        let key = if log_found { "ov_status" } else { "ov_status_ohne_log" };
        Phrase::new(key).with("0", collection::count(&self.data)).with("1", clock()).into()
    }
}

/// The running contracts of the thread: what is open, the catalog behind
/// their lines and the place the log reader leaves each section in.
struct Contracts {
    live: Live,
    blueprints: ContractBlueprints,
    patterns: Patterns,
    section: std::sync::Arc<std::sync::Mutex<Section>>,
}

impl Contracts {
    fn new(paths: &Paths, tail: &mut LogTail) -> Contracts {
        let ini = ini_files(paths);
        let patterns = Patterns::build(&ini);
        let (scan, section) = SectionScan::new(patterns.clone());
        tail.contracts = Some(Box::new(scan) as Box<dyn ContractScan + '_>);
        Contracts { live: Live::new(), blueprints: ContractBlueprints::with_ini(paths, &ini), patterns, section }
    }

    /// The catalog changed (a patch, or it was missing at the start): the lines are built anew from it.
    fn new_catalog(&mut self, run: &Run) {
        self.blueprints = ContractBlueprints::load(run.paths);
        self.reset(run);
    }

    /// What is open according to the running Game.log, read once as a whole, a piece at a time. Gives what was
    /// read, for the shard and the handle.
    fn at_start(&mut self, run: &Run) -> Option<StartLog> {
        // ⚠ Only the running file: after a restart of the game a fresh one begins, and
        // what the kept ones say may long be over.
        let file = run.paths.game_log(None)?;
        let log = match StartLog::read(&file) {
            Ok(log) => log,
            Err(e) => {
                versekit_core::errors::record("watcher.auftraege_start", &e);
                return None;
            }
        };
        let has = |n: &str| collection::contains(&run.data, n);
        let check = Check { blueprints: &self.blueprints, has: &has };
        let mut out = Vec::new();
        self.live.at_start_log(&log, &self.patterns, &check, &hidden_load(run.paths), &mut out);
        run.tell_all(out);
        Some(log)
    }

    /// The section the reader just left: accepted, ended, objectives, silence.
    fn report(&mut self, run: &Run, text: &str) {
        let section = SectionScan::taken(&self.section);
        let has = |n: &str| collection::contains(&run.data, n);
        let check = Check { blueprints: &self.blueprints, has: &has };
        let mut out = Vec::new();
        self.live.report(run.paths, &section, text, &check, &mut out);
        run.tell_all(out);
    }

    fn hide(&mut self, run: &Run, title: &str) {
        let mut out = Vec::new();
        self.live.hide(run.paths, title, &mut out);
        run.tell_all(out);
    }

    /// The hiding is taken back: the contract shows again if it still runs (read from the log once more).
    fn unhide(&mut self, run: &Run, title: &str) {
        self.live.unhide(run.paths, title);
        self.reset(run);
    }

    /// Everything once more from the log; what is no longer running is told.
    fn reset(&mut self, run: &Run) {
        let before = self.live.reset();
        let _ = self.at_start(run);
        let mut out = Vec::new();
        self.live.after_reset(&before, &mut out);
        run.tell_all(out);
    }
}

/// The shard of the running session, from the text the contracts read anyway: the whole running log at the start,
/// then each new section. Nothing of the log is read for it alone.
#[derive(Default)]
struct Shards(Joins);

impl Shards {
    /// The whole running log: the joins of its session.
    fn whole(&mut self, run: &Run, log: &str) {
        self.0.clear();
        self.0.take(log);
        run.tell(News::Shard(self.0.clone()));
    }

    /// A new section; `fresh` when it begins a new log (the game was started anew): the joins before it go.
    fn section(&mut self, run: &Run, text: &str, fresh: bool) {
        let cleared = fresh && !self.0.list().is_empty();
        if cleared {
            self.0.clear();
        }
        if self.0.take(text) || cleared {
            run.tell(News::Shard(self.0.clone()));
        }
    }

    /// The game ended: there is no shard any more.
    fn ended(&mut self, run: &Run) {
        if !self.0.list().is_empty() {
            self.0.clear();
            run.tell(News::Shard(self.0.clone()));
        }
    }
}

/// The lines of a new section the log monitor shows, and the handle when the section names a login; nothing when
/// it has neither. Only the new sections: the past of the running log is not shown again.
fn tell_log_lines(run: &Run, text: &str) {
    let lines = versekit_core::handle_query::log_monitor::lines_in(text);
    if !lines.is_empty() {
        run.tell(News::LogLines(lines));
    }
    tell_own_handle(run, text);
}

/// The handle the text names as logged in, if it names one.
fn tell_own_handle(run: &Run, text: &str) {
    if let Some(handle) = versekit_core::logscan::account_from_text(text) {
        run.tell(News::OwnHandle(handle));
    }
}

/// How often the refinery jobs are looked at, and the catalog.
const REFINERY_EVERY: Duration = Duration::from_secs(15);
const CATALOG_EVERY: Duration = Duration::from_secs(60);
/// How often it is looked whether the graphics settings kept for the end of the game can be written (4.12: 20 s).
const GRAPHICS_EVERY: Duration = Duration::from_secs(20);
/// How often the running log is read again for the stock journal (only when it grew).
const JOURNAL_EVERY: Duration = Duration::from_secs(30);
/// How often the way to start the game is looked for: the launcher can be installed or the start command entered while the overlay runs.
pub const STARTER_EVERY: Duration = Duration::from_secs(20);

/// The hint row for a test channel (PTU, EPTU, TECH-PREVIEW): its data are confidential by the terms of
/// service. `told` is the channel the hint was said for; it is said again only when the channel changes.
fn channel_hint(told: &mut Option<String>, folder: Option<&str>) -> Option<Row> {
    let channel = folder.and_then(game_start::test_channel);
    if channel == *told {
        return None;
    }
    let row = channel
        .as_deref()
        .map(|name| Row::Hint { text: Phrase::new("s_pf_kanal_hinweis").with("0", name).into(), contract: None });
    *told = channel;
    row
}

/// The quiet checks that come around now and then: a refinery job that is
/// over, a corrupt settings file, something newly craftable, a way to start the game.
struct Periodic {
    refinery_next: Instant,
    catalog_next: Instant,
    catalog_stamp: Option<std::time::SystemTime>,
    /// The first look at the catalog was done.
    looked: bool,
    watch: Watch,
    starter_next: Instant,
    starter_every: Duration,
    /// Whether a way to start the game was there at the last look; `None` before the first.
    starter_known: Option<bool>,
    /// The test channel the hint was said for.
    channel_told: Option<String>,
    graphics_next: Instant,
    journal_next: Instant,
}

impl Periodic {
    fn new(starter_every: Duration) -> Periodic {
        let now = Instant::now();
        Periodic {
            refinery_next: now,
            catalog_next: now,
            catalog_stamp: None,
            looked: false,
            watch: Watch::default(),
            starter_next: now,
            starter_every,
            starter_known: None,
            channel_told: None,
            graphics_next: now,
            journal_next: now,
        }
    }

    /// The purchases and moves of the running log go into the stock journal; looked at every [`JOURNAL_EVERY`],
    /// and a log that did not grow since is not read.
    fn journal(&mut self, source: &LogSource, log: Option<&str>) {
        if Instant::now() < self.journal_next {
            return;
        }
        self.journal_next = Instant::now() + JOURNAL_EVERY;
        if let Some(log) = log {
            stock_journal::catch_up(source, &[log.to_string()]);
        }
    }

    /// The graphics settings the page kept for the end of the game (the game ran when the player wrote them) are
    /// written as soon as the game is quiet; looked at every [`GRAPHICS_EVERY`], and the answer is told once.
    fn graphics(&mut self, run: &Run) {
        if Instant::now() < self.graphics_next {
            return;
        }
        self.graphics_next = Instant::now() + GRAPHICS_EVERY;
        let phrase = match graphics_apply::apply_queued(run.paths, None) {
            graphics_apply::Queued::Nothing | graphics_apply::Queued::NotYet => return,
            graphics_apply::Queued::Done(Ok(backups)) => {
                let named = if backups.is_empty() { "-".to_string() } else { backups.join(", ") };
                Phrase::new("s_gr_nach_spiel_ok").with("0", named)
            }
            graphics_apply::Queued::Done(Err(error)) => Phrase::new("s_gr_nach_spiel_f").with("0", error.reason()),
        };
        run.tell(News::Row(Row::Hint { text: phrase.clone().into(), contract: None }));
        run.tell(News::Status(phrase.into()));
    }

    /// Looks for the way to start the game and tells the window when it appears or goes (also once at the start).
    fn starter(&mut self, run: &Run) {
        if Instant::now() < self.starter_next {
            return;
        }
        self.starter_next = Instant::now() + self.starter_every;
        let known = game_start::can_start(run.paths);
        if self.starter_known != Some(known) {
            self.starter_known = Some(known);
            run.tell(News::Starter(known));
        }
        // Data of a test build (PTU, EPTU, TECH-PREVIEW) are confidential by the terms of service: said once
        // for each channel the overlay reads (a hint row, asked for by the community).
        if let Some(row) = channel_hint(&mut self.channel_told, run.paths.game_folder().as_deref()) {
            run.tell(News::Row(row));
        }
    }

    /// A damaged settings file is told once, at the start and when it breaks later.
    fn settings(run: &Run) {
        if let Some(hint) = recovered_hint(run.paths) {
            run.tell(News::Row(Row::Hint { text: hint.into(), contract: None }));
        }
    }

    /// Every refinery job and every job of the fabricator that became over is told once: a hint and a message of the
    /// system.
    fn refinery(&mut self, run: &Run) {
        if Instant::now() < self.refinery_next {
            return;
        }
        self.refinery_next = Instant::now() + REFINERY_EVERY;
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
        for job in refinery_jobs::announce_due(run.paths, now) {
            let text = Phrase::new("s_rf_fertig_hinweis").with("0", refinery_jobs::job_label(&job, 2));
            run.tell(News::Row(Row::Hint { text: text.clone().into(), contract: None }));
            run.tell(News::System(Phrase::new("ov_a_raffinerie_titel"), text));
            run.tell(News::Opens(Event::Refinery));
            // The sound of this job is the player's choice of its own, apart from the one for a new blueprint.
            if versekit_core::sound::refinery_on(run.paths) {
                run.tell(News::Beep);
            }
        }
        // The jobs of the fabricator, looked at as often: a hint and a message of the system each, once.
        for job in versekit_core::workbench_jobs::announce_due(run.paths, now) {
            let text = Phrase::new("s_wb_fertig_hinweis").with("0", versekit_core::workbench_jobs::label(&job));
            run.tell(News::Row(Row::Hint { text: text.clone().into(), contract: None }));
            run.tell(News::System(Phrase::new("ov_a_werkbank_titel"), text));
            run.tell(News::Opens(Event::Workbench));
        }
    }

    /// Is something newly craftable? Looked at when the catalog file changed, at most once a minute.
    ///
    /// Gives the catalog back when it changed since the last look (not for the first look, which
    /// finds the one the thread started with): the contracts and the kinds of the finds are built
    /// from it and must follow, as `contracts.forget()` does in 4.12 after a catalog update.
    fn catalog(&mut self, run: &Run) -> Option<CatalogCache> {
        if Instant::now() < self.catalog_next {
            return None;
        }
        self.catalog_next = Instant::now() + CATALOG_EVERY;
        // The first look is the one at the start: the thread started with what it finds then
        // (or with nothing, and a catalog that comes later is a change).
        let first = !std::mem::replace(&mut self.looked, true);
        let file = catalog_cache::current_file(run.paths);
        let stamp = std::fs::metadata(&file).and_then(|m| m.modified()).ok();
        if stamp.is_none() || stamp == self.catalog_stamp {
            return None;
        }
        let catalog = catalog_cache::load_current(run.paths)?;
        // Only a catalog that could be read counts as seen: a file half written is looked at again.
        self.catalog_stamp = stamp;
        let items: Vec<Item> = catalog
            .blueprints
            .iter()
            .map(|b| Item {
                key: b.key.clone(),
                name: b.name.clone(),
                kind: versekit_core::blueprints::kind_readable(&b.kind),
            })
            .collect();
        for found in self.watch.look(run.paths, &catalog.version, &items, &collection::now_text()) {
            let loud = found.title.is_some();
            let row = Row::Catalog { name: found.name, kind: found.kind, time: clock(), watch_title: found.title };
            run.tell(News::Catalog { row, loud });
            run.tell(News::Opens(Event::Catalog));
        }
        (!first).then_some(catalog)
    }
}

/// The hint for a damaged settings file, once; reading it again finds nothing more.
pub fn recovered_hint(paths: &Paths) -> Option<Phrase> {
    let recovered = paths.settings_recovered()?;
    paths.clear_settings_recovered();
    Some(recovered_phrase(recovered))
}

/// What the player reads about a damaged settings file.
pub fn recovered_phrase(recovered: Recovered) -> Phrase {
    Phrase::new(match recovered {
        Recovered::Backup => "ov_a_einst_bak",
        Recovered::Defaults => "ov_a_einst_neu",
    })
}

/// The hint for a gap in the kept logs.
fn gap_phrase(gap: &Gap) -> Phrase {
    gap.moments()
        .into_iter()
        .enumerate()
        .fold(Phrase::new(gap.key()), |p, (i, at)| p.with(&i.to_string(), Arg::moment(at)))
}

/// What the thread starts with.
pub struct Start {
    /// The host of the data folder.
    pub host: Host,
    pub version: &'static str,
    /// The write lock of the window, taken over by the thread.
    pub guard: writeguard::State,
    /// Counter check: try one write outside the data folder.
    pub counter_write: bool,
    /// The game's `Data.p4k` given from outside (`--archiv`), where it does
    /// not lie in the game folder.
    pub archive: Option<String>,
    /// Counter check: the archive given is passed over.
    pub counter_archive: bool,
    /// How often the way to start the game is looked for ([`STARTER_EVERY`]; the self-check looks faster).
    pub starter_every: Duration,
}

/// The paths the thread reads with: the data folder of `host`, and the
/// archive given from outside, if one is.
fn paths_for(host: Host, archive: Option<&str>) -> Paths {
    with_archive(Paths::new(host), archive)
}

/// The paths of a thread from those of the window (`Paths::working_copy`: the folders given from outside come
/// along, so a copy of the data stays one), with the archive given from outside, if one is.
pub(crate) fn thread_paths(window: &Paths, archive: Option<&str>) -> Paths {
    with_archive(window.working_copy(), archive)
}

fn with_archive(paths: Paths, archive: Option<&str>) -> Paths {
    if let Some(file) = archive {
        paths.set_given_archive(file, GivenBy::CommandLine);
    }
    paths
}

/// Keeps the file of the scan signatures current for the archive of `paths` with `keep` (the archive, the file):
/// the archive is read only when it changed. Without a game nothing happens. What went wrong is recorded.
pub(crate) fn keep_signatures_with(
    paths: &Paths,
    keep: impl FnOnce(&std::path::Path, &str) -> versekit_gamedata::mineables::Keep,
) -> Option<versekit_gamedata::mineables::Keep> {
    let archive = paths.archive_file()?;
    let file = paths.app_file(versekit_gamedata::mineables::SIGNATURES_FILE);
    let outcome = keep(std::path::Path::new(&archive), &file);
    if let versekit_gamedata::mineables::Keep::Failed(reason) = &outcome {
        errors::record("signatures.keep", reason);
    }
    Some(outcome)
}

/// Starts the keeping of the scan signatures, of the goods with a quality, of the names of the places and of the names
/// of the goods in a thread of its own, so that the scanner knows the asteroid types and the signatures of the game
/// data, the stock the goods of the patch, the location row the names of the patch and the stock journal the goods a
/// commodity terminal sold, without the main window having been open. Each is read
/// from the game data only when the archive changed. `paths` are those of the thread ([`thread_paths`]).
pub fn keep_signatures(paths: Paths, guard: writeguard::State) {
    let started = std::thread::Builder::new().name("signatures".into()).spawn_with_language(move || {
        writeguard::adopt(&guard);
        keep_signatures_with(&paths, versekit_gamedata::mineables::keep_current);
        versekit_gamedata::quality_goods::keep(&paths);
        versekit_gamedata::place_names::keep(&paths);
        versekit_gamedata::commodity_names::keep(&paths);
    });
    if let Err(error) = started {
        errors::record("signatures.thread", &error);
    }
}

/// How often the thread of the game's texts looks at its clocks.
pub const TEXTS_EVERY: Duration = Duration::from_secs(1);

/// What the watch of the game's texts tells, as the status line says it.
pub(crate) fn texts_phrase(notice: &versekit_core::game_texts::Notice) -> Phrase {
    use versekit_core::game_texts::Notice;
    match notice {
        Notice::TranslationUpdated(what) => Phrase::new("texte_erneuert").with("0", what.as_str()),
        Notice::GapsFilled(lines) => Phrase::new("texte_ergaenzt").with("0", *lines),
        Notice::LanguageRepaired(folder) => Phrase::new("spielsprache_repariert").with("0", folder.as_str()),
        Notice::FileRestored(channel) => Phrase::new("texte_wieder_eingesetzt").with("0", channel.as_str()),
        Notice::Applied(count) => Phrase::new("inj_aktiv").with("0", *count),
        Notice::CraftData { build, items } => Phrase::new("craftdaten_neu").with("0", build.as_str()).with("1", *items),
    }
}

/// How often the thread of the stock lists looks at the live game build and the goods list.
pub const STOCK_EVERY: Duration = Duration::from_secs(60);

/// One round of the thread of the stock lists ([`stock_fetch::keep_with`]); nothing without the network. Whether
/// the lock refused a write (a copy of the data): then asking again brings nothing.
///
/// [`stock_fetch::keep_with`]: versekit_core::stock_fetch::keep_with
pub(crate) fn keep_stock_once(
    paths: &Paths,
    now: std::time::SystemTime,
    fetch: versekit_core::stock_fetch::Fetch<'_>,
) -> bool {
    if versekit_core::net::offline_in(&paths.host) {
        return false;
    }
    versekit_core::stock_fetch::keep_with(paths, now, fetch).refused()
}

/// Keeps the live game build and the goods list current in a thread of its own, also while the main window is
/// closed: the build in every round (asked for anew every twelve hours), so that every list fetched after it carries
/// the build of the running game, and the goods list when it is due for that build. On a copy of the data, whose
/// lock refuses the writes, the thread ends after telling them. `paths` are those of the thread ([`thread_paths`]).
pub fn keep_stock(paths: Paths, guard: writeguard::State, news: Sender<News>, wake: impl Fn() + Send + 'static) {
    let started = std::thread::Builder::new().name("lagerlisten".into()).spawn_with_language(move || {
        writeguard::adopt(&guard);
        let user_agent = versekit_core::basetool::config::user_agent(env!("CARGO_PKG_VERSION"));
        let fetch = versekit_core::stock_fetch::live_fetcher(&user_agent);
        loop {
            let refused = keep_stock_once(&paths, std::time::SystemTime::now(), &fetch);
            let denied = writeguard::take_denied();
            if !denied.is_empty() {
                if news.send(News::Denied(denied)).is_err() {
                    return;
                }
                wake();
            }
            if refused {
                return;
            }
            std::thread::sleep(STOCK_EVERY);
        }
    });
    if let Err(error) = started {
        errors::record("watcher.lagerlisten", &error);
    }
}

/// Keeps the game's texts current in a thread of its own (`game_texts::watch`), also while the main window is
/// closed: the language line at the start, the translation, the notes after a patch, another tool or a new
/// blueprint, the side channels, the recipe file with the known data errors, the craft data of the catalog (said in
/// the status line) and the mining data after a patch. A run takes its seconds; the round of the log never waits for
/// it. The thread ends with the window (when the news cannot be told any more). `paths` are those of the thread
/// ([`thread_paths`]).
pub fn keep_texts(paths: Paths, guard: writeguard::State, news: Sender<News>, wake: impl Fn() + Send + 'static) {
    keep_texts_while(paths, guard, news, wake, || true);
}

/// Keeps the game's texts from the main window while no overlay runs (`overlay_runs` says whether one does), so
/// that switching the overlay off loses no update of the translation: the same watch as [`keep_texts`], which waits
/// while an overlay runs (that one keeps the texts then, no run happens twice). The main window has no status line
/// for what it tells; the notices go nowhere. `window` are the paths of the window.
pub fn keep_texts_alone(window: &Paths, guard: writeguard::State, overlay_runs: impl Fn() -> bool + Send + 'static) {
    let (news, told) = std::sync::mpsc::channel::<News>();
    // The receiver lives as long as the program: the thread of the texts ends when nobody takes its news.
    let drained = std::thread::Builder::new().name("spieltexte-fenster-meldungen".into()).spawn_with_language(move || {
        for _ in told {}
    });
    if let Err(error) = drained {
        errors::record("watcher.spieltexte", &error);
        return;
    }
    let mut gate = AloneGate::default();
    keep_texts_while(thread_paths(window, None), guard, news, || {}, move || gate.active(overlay_runs(), Instant::now()));
}

/// How long no overlay must have run before the main window keeps the texts: an overlay that is just starting has no
/// window to be found yet.
pub const OVERLAY_GONE_FOR: Duration = Duration::from_secs(30);

/// Whether the main window keeps the game's texts now: only once no overlay has run for [`OVERLAY_GONE_FOR`].
#[derive(Default)]
struct AloneGate {
    absent_since: Option<Instant>,
}

impl AloneGate {
    fn active(&mut self, overlay_runs: bool, now: Instant) -> bool {
        if overlay_runs {
            self.absent_since = None;
            return false;
        }
        now.duration_since(*self.absent_since.get_or_insert(now)) >= OVERLAY_GONE_FOR
    }
}

/// One look of the thread of the game's texts: the watch ticks only while it is `active`; what it told.
fn texts_step(
    watch: &mut versekit_core::game_texts::watch::Watch,
    paths: &std::sync::Arc<Paths>,
    now: f64,
    active: bool,
) -> Vec<versekit_core::game_texts::Notice> {
    if active { watch.tick(paths, now) } else { Vec::new() }
}

/// [`keep_texts`] that looks only while `active` says yes.
fn keep_texts_while(
    paths: Paths,
    guard: writeguard::State,
    news: Sender<News>,
    wake: impl Fn() + Send + 'static,
    mut active: impl FnMut() -> bool + Send + 'static,
) {
    use std::sync::Arc;
    use versekit_core::game_texts::watch::Watch as TextsWatch;
    use versekit_core::translation;
    let started = std::thread::Builder::new().name("spieltexte-wache".into()).spawn_with_language(move || {
        writeguard::adopt(&guard);
        let paths = Arc::new(paths);
        let net = Arc::new(versekit_core::net::Web::new(
            translation::USER_AGENT,
            Duration::from_secs(translation::TIMEOUT_SECONDS),
            200 * 1024 * 1024,
        ));
        let user_agent = versekit_core::basetool::config::user_agent(env!("CARGO_PKG_VERSION"));
        let recipes = Arc::new(versekit_core::recipe_file::Scmdb { user_agent });
        // After a patch, and when scmdb has new craft data for the catalog, the catalog of the contract data is built
        // anew; the round of the log reads it again when its file changed.
        let catalog: versekit_core::game_texts::watch::CatalogBuild = Arc::new(|paths: &Paths| {
            versekit_gamedata::list::rebuild_now(paths).run().map(|built| {
                versekit_core::game_texts::watch::CatalogBuilt {
                    version: built.version.clone(),
                    blueprints: built.blueprints.len(),
                }
            })
        });
        let behind: versekit_core::game_texts::watch::CraftCheck =
            Arc::new(|paths: &Paths, build: &str| versekit_gamedata::list::craft_data_behind(paths, build));
        // A catalog that lags behind the game data at the start (an update of VerseKit with a new layout, a patch
        // while VerseKit was closed) is built anew without a page open, as soon as the game is closed.
        let lags: versekit_core::game_texts::watch::CatalogCheck =
            Arc::new(|paths: &Paths| versekit_gamedata::list::catalog_behind(paths));
        let refresh: versekit_core::game_texts::watch::CatalogBuild = Arc::new(|paths: &Paths| {
            versekit_gamedata::list::refresh_catalog(paths).map(|built| {
                versekit_core::game_texts::watch::CatalogBuilt {
                    version: built.version.clone(),
                    blueprints: built.blueprints.len(),
                }
            })
        });
        let mut watch = TextsWatch::new(net, Arc::new(versekit_ui::game_texts_page::GameArchive), recipes)
            .with_catalog(catalog)
            .with_craft_check(behind)
            .with_catalog_refresh(lags, refresh);
        let start = Instant::now();
        loop {
            let mut told = Vec::new();
            told.extend(
                texts_step(&mut watch, &paths, start.elapsed().as_secs_f64(), active())
                    .iter()
                    .map(|n| News::Status(texts_phrase(n).into())),
            );
            let mut denied = writeguard::take_denied();
            denied.extend(watch.take_denied());
            if !denied.is_empty() {
                told.push(News::Denied(denied));
            }
            if !told.is_empty() {
                if told.into_iter().any(|one| news.send(one).is_err()) {
                    return;
                }
                wake();
            }
            std::thread::sleep(TEXTS_EVERY);
        }
    });
    if let Err(error) = started {
        errors::record("watcher.spieltexte", &error);
    }
}

/// Starts the thread on the data folder of `host`. It runs until the
/// window is gone.
pub fn start(start: Start, news: Sender<News>, asks: Receiver<Ask>, wake: impl Fn() + Send + 'static) {
    std::thread::Builder::new()
        .name("watcher".into())
        .spawn_with_language(move || {
            writeguard::adopt(&start.guard);
            if start.counter_write {
                let outside = std::env::temp_dir().join("versekit-gegenprobe-schreiben.txt");
                let _ = jsonio::write_text_atomic(&outside.to_string_lossy(), "x", jsonio::Newline::Keep);
            }
            let archive = start.archive.as_deref().filter(|_| !start.counter_archive);
            let paths = paths_for(start.host, archive);
            run(&paths, start.version, start.starter_every, news, asks, Box::new(wake));
        })
        .expect("watcher thread");
}

/// The line of the start trail when `data` holds fewer blueprints than ever before; `None` otherwise. A count as
/// high as the mark or higher becomes the new mark, with the data folder (`check_shrinkage`).
fn shrinkage_line(collection: &Collection<'_>, data: &Map) -> Option<String> {
    let shrink = collection.check_shrinkage(data)?;
    let folder = shrink.folder_text().unwrap_or_else(|| "?".to_string());
    Some(format!("Bestand: SCHWUND {} statt {} (frueher in {folder})", shrink.now, shrink.highest))
}

fn run(
    paths: &Paths,
    version: &'static str,
    starter_every: Duration,
    news: Sender<News>,
    asks: Receiver<Ask>,
    wake: Box<dyn Fn() + Send>,
) {
    let _ = news.send(News::Status(Phrase::new("ov_starte").into()));
    let _ = news.send(News::Archive(paths.archive_file()));
    wake();
    // The overlay does not wait for the network: the window brings scmdb's part.
    let catalog = versekit_gamedata::list::list_catalog_offline(paths);
    let cache_catalog = CacheCatalog(catalog.as_ref());
    let names = BackRoute { paths, archive: &NoArchive };
    let exports = FileExports { paths, version, clock: now_unix };
    let collection =
        Collection { paths, catalog: &cache_catalog, names: &names, exports: &exports, clock: collection::now_text };
    let sources = LogSource::sources_of(paths);
    // Before the pattern is built: the backlog is read with the phrase of this client.
    if let Some(row) = learn_language(paths, &sources, catalog.as_ref()) {
        let _ = news.send(News::Row(row));
        wake();
    }
    let pattern = phrases::pattern(&sources.collect().0);
    let source = LogSource::new(paths, sources);
    // Load, match and add under the write lock: the main window may be writing the same file (it may have
    // been open before the overlay), and the lock holds against it as a process.
    let start_lock = versekit_core::jsonio::lock_saves();
    let data = collection.load();
    let mut run = Run {
        paths,
        collection,
        data,
        stamp: None,
        seen: HashSet::new(),
        meta: meta_table(catalog.as_ref()),
        pattern: pattern.clone(),
        news,
        wake,
        said: false,
    };

    run.stamp = run.file_stamp();
    let mut periodic = Periodic::new(starter_every);
    Periodic::settings(&run);
    // Once at the start: names that carry the game's suffix are matched to
    // the catalog (`Balandin (S3 B Military)`).
    if run.collection.align(&mut run.data) > 0 {
        run.save();
    }
    // Fewer blueprints than ever before: only written down here, the main window shows it.
    if let Some(line) = shrinkage_line(&run.collection, &run.data) {
        versekit_core::trail::step(&line);
    }
    // The blueprints everybody has from the start stand in no log.
    let starters: Vec<String> =
        catalog.iter().flat_map(|c| c.blueprints.iter()).filter(|b| b.start).map(|b| b.name.clone()).collect();
    let mut added_starters = 0;
    for name in &starters {
        if run.collection.add(&mut run.data, name, "start", None) {
            added_starters += 1;
        }
    }
    if added_starters > 0 {
        run.save();
        run.tell(News::Status(Phrase::new("ov_start_eingetragen").with("0", added_starters).into()));
    }
    drop(start_lock);

    // Read back what came while nothing ran: quietly, the number in the status.
    let mut state = ReadState::open(paths);
    let (finds, report) = source.read_backlog(&mut state, &run.pattern, true, true);
    let added = run.take_caught_up(&finds);
    if !added.is_empty() {
        run.tell(News::Status(Phrase::new("nachgelesen").with("0", added.len()).with("1", report.files).into()));
    }
    if let Some(gap) = &report.gap {
        run.tell(News::Row(Row::Hint { text: gap_phrase(gap).into(), contract: None }));
    }
    run.seen = collection::keys_seen_in_game(&run.data).into_iter().collect();
    let mut tail = LogTail::new(&source, state, pattern);
    let mut contracts = Contracts::new(paths, &mut tail);
    // What this first look read belongs to nobody: the state comes from the whole log. One block at a time, to
    // the end of the log.
    loop {
        tail.new_names();
        SectionScan::taken(&contracts.section);
        if !tail.behind {
            break;
        }
    }
    let mut shards = Shards::default();
    if let Some(log) = contracts.at_start(&run) {
        shards.whole(&run, log.lines());
        if let Some(handle) = log.handle() {
            run.tell(News::OwnHandle(handle));
        }
    }
    run.tell(News::Status(run.status(tail.path.is_some())));
    let denied = writeguard::take_denied();
    if !denied.is_empty() {
        run.tell(News::Denied(denied));
    }

    let interval_of =
        |paths: &Paths| Duration::from_secs(paths.setting_int(keys::INTERVAL, 3, Some(1), Some(60)) as u64);
    let mut rescan_seen = run.paths.setting_int(keys::RESCAN_NR, 0, None, None);
    let mut began = Instant::now();
    let mut next = began;
    let mut looked = began;
    loop {
        // Wait in short steps so a request to read again is not kept waiting.
        while Instant::now() < next {
            match asks.recv_timeout(Duration::from_millis(200)) {
                Ok(ask) => answer(ask, &mut run, &source, &mut contracts, &mut periodic, &mut shards),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
            // The interval of the settings counts at once: a shorter one brings the next look forward.
            if looked.elapsed() >= Duration::from_secs(1) {
                looked = Instant::now();
                next = next.min(began + interval_of(run.paths));
            }
        }
        began = Instant::now();
        next = began + interval_of(run.paths);
        match asks.try_recv() {
            Ok(ask) => answer(ask, &mut run, &source, &mut contracts, &mut periodic, &mut shards),
            Err(TryRecvError::Disconnected) => return,
            Err(TryRecvError::Empty) => {}
        }
        // Where the reader stood: a log that begins anew (the game was started again) starts from the front.
        let (offset, path) = (tail.offset, tail.path.clone());
        // What the window wrote since the last round (a tick, foreign blueprints removed) is taken over first,
        // and the finds are written before anyone else can.
        {
            let _lock = versekit_core::jsonio::lock_saves();
            run.refresh();
            let mut changed = false;
            for (name, extra) in tail.new_names() {
                let key = name_key(&name);
                if !run.seen.insert(key) {
                    continue;
                }
                if run.collection.add(&mut run.data, &name, "log", None) {
                    changed = true;
                }
                let row = run.found_row(&name, extra.as_ref(), false);
                run.tell(News::Row(row));
                run.tell(News::Found);
                // A find the watch list waited for is ticked off, and the player is told once.
                if let Some(title) = watchlist::fulfill(run.paths, &name) {
                    run.tell(News::Row(Row::Hint {
                        text: Phrase::new("ov_a_merk").with("0", title).into(),
                        contract: None,
                    }));
                }
            }
            if changed {
                run.save();
            }
        }
        // A block was read and the log goes on: the next look comes at once, not after the interval.
        if tail.behind {
            next = Instant::now();
        }
        // After the blueprints: a blueprint just received stands in the collection when the contract is checked.
        contracts.report(&run, &tail.last_text);
        shards.section(&run, &tail.last_text, tail.offset < offset || tail.path != path);
        tell_log_lines(&run, &tail.last_text);
        periodic.refinery(&run);
        periodic.starter(&run);
        periodic.graphics(&run);
        periodic.journal(&source, tail.path.as_deref());
        Periodic::settings(&run);
        if rescan_requested(run.paths, &mut rescan_seen) {
            answer(Ask::Rescan, &mut run, &source, &mut contracts, &mut periodic, &mut shards);
        }
        take_catalog(&mut run, &mut periodic, &mut contracts);
        run.tell_count(tail.path.is_some());
        let denied = writeguard::take_denied();
        if !denied.is_empty() {
            run.tell(News::Denied(denied));
        }
    }
}

/// Works out the blueprint phrase of this client from the kept logs and the names of the catalog, and keeps it
/// in `phrasen.json` (`Sources::learn`). The hint row when a phrase was newly kept.
fn learn_language(paths: &Paths, sources: &phrases::Sources, catalog: Option<&CatalogCache>) -> Option<Row> {
    let names: Vec<String> = catalog.iter().flat_map(|c| c.blueprints.iter()).map(|b| b.name.clone()).collect();
    let found = sources.learn(&names, &paths.log_backups(None))?;
    Some(Row::Hint { text: Phrase::new("sprache_erkannt").with("0", found).into(), contract: None })
}

/// A catalog that came since the last look (built or fetched by the main window): the status line says so
/// (4.12 `katalog_geholt`), and the kinds of the finds and the lines of the contracts follow it.
fn take_catalog(run: &mut Run, periodic: &mut Periodic, contracts: &mut Contracts) {
    let Some(catalog) = periodic.catalog(run) else { return };
    let said = Phrase::new("katalog_geholt").with("0", catalog.blueprints.len()).with("1", catalog.version.as_str());
    run.say(said.into());
    run.meta = meta_table(Some(&catalog));
    contracts.new_catalog(run);
}

/// What the window asked for.
fn answer(
    ask: Ask,
    run: &mut Run,
    source: &LogSource,
    contracts: &mut Contracts,
    periodic: &mut Periodic,
    shards: &mut Shards,
) {
    run.refresh();
    match ask {
        Ask::Rescan => {
            rescan(run, source, contracts);
            // Reading everything again looks at the catalog too, without waiting for its minute.
            periodic.catalog_next = Instant::now();
        }
        Ask::Hide(title) => contracts.hide(run, &title),
        Ask::Unhide(title) => contracts.unhide(run, &title),
        Ask::GameEnded => {
            contracts.live.clear_unclear = true;
            shards.ended(run);
        }
    }
}

/// Did the main window ask for a new reading since `seen`? `seen` is brought up to date.
///
/// The main window raises the number `einlesen_nr` in the settings (the account was changed); a number
/// other than the last seen is the request, one that is lower after a reset counts too.
fn rescan_requested(paths: &Paths, seen: &mut i64) -> bool {
    let now = paths.setting_int(keys::RESCAN_NR, 0, None, None);
    std::mem::replace(seen, now) != now
}

/// All logs once more, also the known ones, and the mission log judged anew (4.12 `_alles_neu_einlesen`). The
/// answer always shows, also a zero: in the status line, as a row that stays, and kept in the settings, from
/// where the main window tells it as a notice.
fn rescan(run: &mut Run, source: &LogSource, contracts: &mut Contracts) {
    // The phrases may have changed since the start (a language file came, an own phrase was added).
    run.pattern = phrases::pattern(&source.sources.collect().0);
    let unix_now =
        || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
    let (finds, report) = match source.read_all(&run.pattern) {
        Ok(read) => read,
        Err(e) => {
            // The failure is told in the same three places as a result; the contracts stay as they are.
            errors::record("watcher.neu_einlesen", &e);
            let result = RescanResult::failure(unix_now());
            run.say(result.phrase().into());
            run.tell(News::Row(Row::Hint { text: result.phrase().into(), contract: None }));
            result.save(run.paths);
            return;
        }
    };
    let added = run.take_caught_up(&finds);
    // ⚠ The mission log belongs to it: a kept contract is never read again otherwise, so a better reading would
    // reach only the contracts to come. A failure here leaves the blueprints, which are saved already.
    let judged = MissionScan::new(source).reassess(run.paths.game_folder().as_deref(), None);
    let (contracts_new, contracts_corrected) = judged.map_or_else(
        |e| {
            errors::record("watcher.neu_einlesen_auftraege", &e);
            (0, 0)
        },
        |(_, new, corrected)| (new, corrected),
    );
    let result = RescanResult {
        at: unix_now(),
        files: report.files,
        blueprints: added.len(),
        contracts_new,
        contracts_corrected,
        failed: false,
    };
    let text = result.phrase();
    run.say(text.clone().into());
    run.tell(News::Row(Row::Hint { text: text.into(), contract: None }));
    result.save(run.paths);
    // And the state of the contracts with it: dropped, then read again from the log.
    contracts.reset(run);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The main window keeps the game's texts only once no overlay has run for a while, and stops when one comes.
    #[test]
    fn the_main_window_keeps_the_texts_only_once_no_overlay_ran_for_a_while() {
        let mut gate = AloneGate::default();
        let t0 = Instant::now();
        let at = |seconds: u64| t0 + Duration::from_secs(seconds);
        assert!(!gate.active(true, at(0)), "an overlay runs: it keeps them");
        assert!(!gate.active(false, at(1)), "just gone, or still starting");
        assert!(!gate.active(false, at(30)));
        assert!(gate.active(false, at(31)), "gone for half a minute: the window keeps them");
        assert!(gate.active(false, at(400)));
        assert!(!gate.active(true, at(401)), "an overlay again: the window waits");
        assert!(!gate.active(false, at(402)));
    }

    /// No scmdb: every look fails.
    struct NoScmdb;

    impl versekit_core::recipe_file::Supply for NoScmdb {
        fn file(&self, name: &str) -> Result<Vec<u8>, String> {
            Err(format!("{name}: missing"))
        }
    }

    /// While it is not active the watch of the main window writes nothing into the game; once it is, the same watch
    /// as the overlay's enters the language and writes the notes.
    #[test]
    fn the_watch_of_the_main_window_writes_only_while_it_is_active() {
        use std::sync::Arc;
        use versekit_core::game_texts::{self, watch::Watch as TextsWatch};
        let dir = std::env::temp_dir().join(format!("versekit-watcher-allein-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.to_string_lossy().replace('\\', "/");
        let live = format!("{root}/StarCitizen/LIVE");
        let data = format!("{root}/data");
        let ini = format!("{live}/data/Localization/german_(germany)/global.ini");
        std::fs::create_dir_all(format!("{live}/data/Localization/german_(germany)")).expect("folder");
        std::fs::create_dir_all(&data).expect("folder");
        std::fs::write(format!("{live}/Game.log"), "").expect("log");
        std::fs::write(&ini, "Bounty_title_001=Kopfgeld\nBounty_desc_001=Fang ihn.\n").expect("ini");
        let host = versekit_core::paths::Host::new(
            versekit_core::pypath::Flavor::Posix,
            &[("HOME", root.as_str()), ("SC_BP_HOME", data.as_str()), ("SC_INSTALL_DIR", live.as_str())],
            &root,
        );
        let paths = Arc::new(Paths::new(host));
        versekit_core::translation::note_set(&paths, "dymerz_de", Some("v1"), Some("german_(germany)"), None);
        paths.set_setting(versekit_core::game_ini::SETTING_SOURCE, versekit_core::jsonio::Value::str("dymerz_de"));
        let catalog = r#"{"geholt": "2026-10-22 04:00", "bauplaene": {}, "missionen": {"Bounty_title_001": {"bp": ["Atzkav Sniper Rifle"], "rep": 0, "titel_key": "Bounty_title_001", "text_key": "Bounty_desc_001"}}}"#;
        std::fs::write(paths.app_file(versekit_core::injection::sources::CATALOG_412), catalog).expect("catalog");
        let mut watch = TextsWatch::new(
            Arc::new(versekit_core::net::Fake::default()),
            Arc::new(game_texts::NoArchive),
            Arc::new(NoScmdb),
        );
        for now in [0.0, 1.0, 2.0, 300.0] {
            assert!(texts_step(&mut watch, &paths, now, false).is_empty());
        }
        assert!(!watch.busy(), "no run started");
        assert!(!versekit_core::injection::is_applied(&ini), "nothing written while an overlay keeps the texts");
        assert!(!std::path::Path::new(&format!("{live}/user.cfg")).exists());
        let start = Instant::now();
        let mut told = texts_step(&mut watch, &paths, 400.0, true);
        while watch.busy() {
            assert!(start.elapsed() < Duration::from_secs(60), "the run ends");
            std::thread::sleep(Duration::from_millis(5));
            told.extend(texts_step(&mut watch, &paths, 400.0, true));
        }
        assert!(versekit_core::injection::is_applied(&ini), "{told:?}");
        let cfg = std::fs::read_to_string(format!("{live}/user.cfg")).expect("user.cfg");
        assert!(cfg.contains("g_language = german_(germany)"), "{cfg}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_status_line_tells_the_watch_of_the_game_texts_in_the_words_of_the_pages() {
        use versekit_core::game_texts::Notice;
        use versekit_core::language::{Choice, Lang, set_thread_language};
        let notices = [
            Notice::TranslationUpdated("Deutsch (Luftwerft) · 4.3".to_string()),
            Notice::GapsFilled(12),
            Notice::LanguageRepaired("german_(germany)".to_string()),
            Notice::FileRestored("PTU · german_(germany)".to_string()),
            Notice::Applied(345),
            Notice::CraftData { build: "4.3.2-live.12345".to_string(), items: 1591 },
        ];
        for (choice, lang) in [(Choice::De, Lang::De), (Choice::En, Lang::En)] {
            set_thread_language(choice);
            for notice in &notices {
                let said = texts_phrase(notice).render(lang);
                assert_eq!(said, notice.text(), "{notice:?}");
                assert!(!said.contains('{'), "every value is filled in: {said}");
            }
        }
        set_thread_language(Choice::Auto);
    }

    /// A collection in a throwaway home: the thread and, with the same paths, the main window write the same file.
    fn home(name: &str) -> (String, Paths) {
        let dir = std::env::temp_dir().join(format!("versekit-watcher-{}-{name}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        let home = dir.to_string_lossy().replace('\\', "/");
        let paths = Paths::new(Host::new(
            versekit_core::pypath::Flavor::Posix,
            &[("SC_BP_HOME", home.as_str()), ("HOME", home.as_str())],
            &home,
        ));
        (home, paths)
    }

    #[test]
    fn the_start_keeps_the_signature_file_of_the_given_archive_beside_the_other_data_files() {
        use versekit_gamedata::mineables::{Keep, SIGNATURES_FILE};
        let (home, paths) = home("signaturen-halten");
        let archive = format!("{home}/Spiel/Data.p4k");
        paths.set_given_archive(&archive, GivenBy::CommandLine);
        let asked = std::cell::RefCell::new(Vec::new());
        let outcome = keep_signatures_with(&paths, |archive, file| {
            asked.borrow_mut().push((archive.to_path_buf(), file.to_string()));
            Keep::Written
        });
        assert_eq!(outcome, Some(Keep::Written));
        let asked = asked.into_inner();
        assert_eq!(asked.len(), 1, "asked once: {asked:?}");
        assert_eq!(asked[0].0, std::path::PathBuf::from(&archive));
        assert_eq!(asked[0].1, paths.app_file(SIGNATURES_FILE), "beside the mining data, where they are read");
        assert!(asked[0].1.ends_with(SIGNATURES_FILE));
        // A failure comes back as it is; the thread records it.
        let failed = keep_signatures_with(&paths, |_, _| Keep::Failed("no".into()));
        assert_eq!(failed, Some(Keep::Failed("no".into())));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_round_of_the_stock_lists_renews_the_build_asks_nothing_offline_and_ends_on_a_copy() {
        use versekit_core::scmdb::FetchError;
        use versekit_core::stock_fetch::{GAME_VERSIONS_URL, live_build};
        let (home, paths) = home("lagerlisten");
        let asked = std::sync::Mutex::new(Vec::<String>::new());
        let fetch = |url: &str| -> Result<Vec<u8>, FetchError> {
            asked.lock().unwrap().push(url.to_string());
            if url == GAME_VERSIONS_URL {
                Ok(br#"{"status": "ok", "data": {"live": "4.11.0", "ptu": null}}"#.to_vec())
            } else {
                Err(FetchError::Missing)
            }
        };
        let now = std::time::SystemTime::now();
        assert!(!keep_stock_once(&paths, now, &fetch), "a missing list is no refused write");
        assert_eq!(live_build(&paths), "4.11.0", "the build of the running game is kept");
        assert!(asked.lock().unwrap().iter().any(|url| url != GAME_VERSIONS_URL), "the goods list was due");
        // Without the network of the program nobody is asked.
        let mut off = paths.host.clone();
        off.set_var(versekit_core::legacy::ENV_NO_NET, "1");
        let before = asked.lock().unwrap().len();
        assert!(!keep_stock_once(&Paths::new(off), now, &fetch));
        assert_eq!(asked.lock().unwrap().len(), before);
        // On a copy of the data the lock refuses the writes: the round says so, and the thread ends.
        let (copy_home, copy) = self::home("lagerlisten-kopie");
        writeguard::deny(true);
        let refused = keep_stock_once(&copy, now, &fetch);
        writeguard::deny(false);
        let denied = writeguard::take_denied();
        assert!(refused);
        assert!(denied.iter().any(|write| write.contains("spielstand.json")), "{denied:?}");
        assert_eq!(live_build(&copy), "");
        let _ = std::fs::remove_dir_all(home);
        let _ = std::fs::remove_dir_all(copy_home);
    }

    #[test]
    fn what_the_window_wrote_into_the_collection_is_taken_over_before_the_next_write() {
        let (home, paths) = home("fenster-schreibt");
        let names = BackRoute { paths: &paths, archive: &NoArchive };
        let make = || Collection {
            paths: &paths,
            catalog: &collection::NoCatalog,
            names: &names,
            exports: &collection::NoExports,
            clock: collection::now_text,
        };
        let (news, _told) = std::sync::mpsc::channel();
        let collection = make();
        let data = collection.load();
        let mut run = Run {
            paths: &paths,
            collection,
            data,
            stamp: None,
            seen: HashSet::new(),
            meta: HashMap::new(),
            pattern: regex::Regex::new("x").expect("pattern"),
            news,
            wake: Box::new(|| {}),
            said: false,
        };
        run.stamp = run.file_stamp();
        assert!(run.collection.add(&mut run.data, "Alpha Rifle", "log", None));
        run.save();
        // The window ticks another one off, in the same file.
        let window = make();
        let mut stock = window.load();
        assert!(window.add(&mut stock, "Beta Helmet", "hand", None));
        assert!(window.save(&mut stock));
        // The thread takes it over before it writes its next find: nothing the window did is taken back.
        run.refresh();
        assert!(run.collection.add(&mut run.data, "Gamma Pistol", "log", None));
        run.save();
        let names_now = collection::names(&make().load());
        for name in ["Alpha Rifle", "Beta Helmet", "Gamma Pistol"] {
            assert!(names_now.iter().any(|n| n == name), "{name} is missing: {names_now:?}");
        }
        assert!(run.seen.contains(&name_key("Alpha Rifle")), "what stood in a log stays seen");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn what_the_window_wrote_while_the_logs_were_read_is_not_taken_back_by_the_caught_up_finds() {
        let (home, paths) = home("nachlese-lang");
        let names = BackRoute { paths: &paths, archive: &NoArchive };
        let make = || Collection {
            paths: &paths,
            catalog: &collection::NoCatalog,
            names: &names,
            exports: &collection::NoExports,
            clock: collection::now_text,
        };
        let (news, _told) = std::sync::mpsc::channel();
        let collection = make();
        let data = collection.load();
        let mut run = Run {
            paths: &paths,
            collection,
            data,
            stamp: None,
            seen: HashSet::new(),
            meta: HashMap::new(),
            pattern: regex::Regex::new("x").expect("pattern"),
            news,
            wake: Box::new(|| {}),
            said: false,
        };
        run.stamp = run.file_stamp();
        // The thread reads the logs (seconds); meanwhile the window ticks a blueprint off.
        let window = make();
        let mut stock = window.load();
        assert!(window.add(&mut stock, "Beta Helmet", "hand", None));
        assert!(window.save(&mut stock));
        // Then it takes its finds: the tick of the window must still be there.
        let added = run.take_caught_up(&[("Gamma Pistol".to_string(), None)]);
        assert_eq!(added, ["Gamma Pistol"]);
        let names_now = collection::names(&make().load());
        for name in ["Beta Helmet", "Gamma Pistol"] {
            assert!(names_now.iter().any(|n| n == name), "{name} is missing: {names_now:?}");
        }
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_start_of_the_thread_waits_for_a_writer_of_the_lock() {
        let (home, paths) = home("start-wartet");
        let (news, told) = std::sync::mpsc::channel();
        let (_ask, asks) = std::sync::mpsc::channel();
        let guard = jsonio::lock_saves();
        start(
            Start {
                host: paths.host.clone(),
                version: "5.0.0",
                guard: writeguard::State::default(),
                counter_write: false,
                archive: None,
                counter_archive: false,
                starter_every: Duration::from_secs(3600),
            },
            news,
            asks,
            || {},
        );
        // What comes before the loading of the collection arrives; the status that follows the start does not.
        let started = |told: &std::sync::mpsc::Receiver<News>, wait: Duration| {
            let end = Instant::now() + wait;
            while let Some(left) = end.checked_duration_since(Instant::now()) {
                if let Ok(News::Status(Message::Phrase(p))) = told.recv_timeout(left)
                    && p.key != "ov_starte"
                {
                    return true;
                }
            }
            false
        };
        assert!(!started(&told, Duration::from_millis(700)), "the start goes on while another one writes");
        drop(guard);
        assert!(started(&told, Duration::from_secs(60)), "the start goes on when the lock is free");
        let _ = std::fs::remove_dir_all(home);
    }

    /// A throwaway home with a game folder whose kept log carries two blueprints in a language no table knows.
    fn foreign_game(name: &str) -> (String, Paths) {
        let (home, _) = home(name);
        let game = format!("{home}/StarCitizen/LIVE");
        std::fs::create_dir_all(format!("{game}/logbackups")).expect("folder");
        std::fs::write(format!("{game}/Game.log"), "").expect("log");
        let line = |n: u8, text: &str| {
            format!(
                "<2026-10-30T10:00:0{n}.000Z> [Notice] <SHUDEvent_OnNotification> Added notification \"{text}: \" [{n}]\n"
            )
        };
        let body = [
            line(1, "Plan de construction reçu: Attrition-5 Repeater"),
            line(2, "Mission terminée: Irgendwas"),
            line(3, "Plan de construction reçu: Singe Cannon (S2)"),
        ]
        .concat();
        std::fs::write(format!("{game}/logbackups/alt.log"), body).expect("backup");
        let paths = Paths::new(Host::new(
            versekit_core::pypath::Flavor::Posix,
            &[
                ("SC_BP_HOME", home.as_str()),
                ("HOME", home.as_str()),
                (versekit_core::paths::ENV_INSTALL_DIR, game.as_str()),
                (game_start::ENV_NO_PROCESS, "1"),
            ],
            &home,
        ));
        (home, paths)
    }

    fn catalog_of(names: &[&str]) -> CatalogCache {
        let blueprints = names
            .iter()
            .map(|n| versekit_core::catalog_cache::Blueprint {
                key: n.to_lowercase(),
                name: n.to_string(),
                ..Default::default()
            })
            .collect();
        CatalogCache { version: "4.3".into(), blueprints, ..Default::default() }
    }

    #[test]
    fn the_phrase_of_an_unknown_client_is_learned_from_the_catalog_names_and_said_once() {
        language::set_thread_language(language::Choice::De);
        let (home, paths) = foreign_game("sprache-lernen");
        let catalog = catalog_of(&["Attrition-5 Repeater", "Singe Cannon (S2)"]);
        let sources = LogSource::sources_of(&paths);
        assert!(learn_language(&paths, &sources, None).is_none(), "without names nothing is learned");
        let Some(Row::Hint { text, contract: None }) = learn_language(&paths, &sources, Some(&catalog)) else {
            panic!("a hint row")
        };
        assert!(text.render().contains("„Plan de construction reçu\""), "{}", text.render());
        assert!(sources.collect().0.iter().any(|p| p == "Plan de construction reçu"), "kept in phrasen.json");
        assert!(learn_language(&paths, &sources, Some(&catalog)).is_none(), "certain now: said once");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_start_learns_the_phrase_before_it_reads_the_kept_logs() {
        language::set_thread_language(language::Choice::De);
        let (home, paths) = foreign_game("sprache-start");
        let catalog = catalog_of(&["Attrition-5 Repeater", "Singe Cannon (S2)", "10-Series Greatsword Cannon"]);
        let cache = paths.app_file(versekit_core::catalog_cache::CACHE_FILE);
        let text = jsonio::dumps(&versekit_core::catalog_cache::to_value(&catalog), None, false);
        std::fs::write(&cache, text).expect("catalog");
        let (news, told) = std::sync::mpsc::channel();
        let (ask, asks) = std::sync::mpsc::channel();
        start(
            Start {
                host: paths.host.clone(),
                version: "5.0.0",
                guard: writeguard::State::default(),
                counter_write: false,
                archive: None,
                counter_archive: false,
                starter_every: Duration::from_secs(3600),
            },
            news,
            asks,
            || {},
        );
        let (mut hint, mut caught_up) = (None, None);
        let end = Instant::now() + Duration::from_secs(60);
        while caught_up.is_none()
            && let Some(left) = end.checked_duration_since(Instant::now())
        {
            match told.recv_timeout(left) {
                Ok(News::Row(Row::Hint { text, .. })) if hint.is_none() => hint = Some(text.render()),
                Ok(News::Status(Message::Phrase(p))) if p.key == "nachgelesen" => {
                    caught_up = Some(Message::Phrase(p).render())
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        drop(ask);
        let hint = hint.expect("the language is said");
        assert!(hint.contains("Plan de construction reçu"), "{hint}");
        let caught_up = caught_up.expect("the kept log is read with the learned phrase");
        assert!(caught_up.starts_with("Nachgelesen: 2 Baupläne aus 1 "), "both blueprints: {caught_up}");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn fewer_blueprints_than_ever_give_a_line_and_more_raise_the_mark() {
        let (home, paths) = home("schwund-zeile");
        let names = BackRoute { paths: &paths, archive: &NoArchive };
        let collection = Collection {
            paths: &paths,
            catalog: &collection::NoCatalog,
            names: &names,
            exports: &collection::NoExports,
            clock: collection::now_text,
        };
        let mut data = collection.load();
        for name in ["Alpha Rifle", "Beta Helmet", "Gamma Pistol"] {
            assert!(collection.add(&mut data, name, "log", None));
        }
        // As many as never before: no line, the count and the folder become the mark.
        assert_eq!(shrinkage_line(&collection, &data), None);
        let mark = std::fs::read_to_string(collection.high_water_file()).expect("the mark is written");
        assert!(mark.contains("\"bauplaene\": 3"), "{mark}");
        // One gone: the line names both counts and the folder of back then.
        assert!(collection::remove(&mut data, "Beta Helmet"));
        let folder = paths.app_folder();
        assert_eq!(
            shrinkage_line(&collection, &data),
            Some(format!("Bestand: SCHWUND 2 statt 3 (frueher in {folder})"))
        );
        // A mark without a folder: a question mark stands in for it.
        std::fs::write(collection.high_water_file(), "{\"bauplaene\": 9}").expect("mark");
        assert_eq!(shrinkage_line(&collection, &data).as_deref(), Some("Bestand: SCHWUND 2 statt 9 (frueher in ?)"));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_start_of_the_thread_checks_the_collection_against_its_mark() {
        let (home, paths) = home("schwund-start");
        let names = BackRoute { paths: &paths, archive: &NoArchive };
        let collection = Collection {
            paths: &paths,
            catalog: &collection::NoCatalog,
            names: &names,
            exports: &collection::NoExports,
            clock: collection::now_text,
        };
        let mut data = collection.load();
        assert!(collection.add(&mut data, "Alpha Rifle", "log", None));
        assert!(collection.save(&mut data));
        let mark = collection.high_water_file();
        assert!(!std::path::Path::new(&mark).exists(), "no mark before the start");
        let (news, told) = std::sync::mpsc::channel();
        let (_ask, asks) = std::sync::mpsc::channel();
        start(
            Start {
                host: paths.host.clone(),
                version: "5.0.0",
                guard: writeguard::State::default(),
                counter_write: false,
                archive: None,
                counter_archive: false,
                starter_every: Duration::from_secs(3600),
            },
            news,
            asks,
            || {},
        );
        // The status that follows the start comes after the check.
        let end = Instant::now() + Duration::from_secs(60);
        let mut started = false;
        while let Some(left) = end.checked_duration_since(Instant::now()) {
            if let Ok(News::Status(Message::Phrase(p))) = told.recv_timeout(left)
                && p.key != "ov_starte"
            {
                started = true;
                break;
            }
        }
        assert!(started, "the start did not go on");
        let written = std::fs::read_to_string(&mark).expect("the start checked the collection and set the mark");
        assert!(written.contains("\"bauplaene\":"), "{written}");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn graphics_settings_kept_for_the_end_of_the_game_are_written_when_it_is_quiet_and_told_once() {
        language::set_thread_language(language::Choice::De);
        let (home, _) = home("grafik-vorgemerkt");
        let game = format!("{home}/StarCitizen/LIVE");
        let profile = format!("{game}/USER/Client/0/Profiles/default");
        std::fs::create_dir_all(&profile).expect("folder");
        std::fs::write(format!("{profile}/attributes.xml"), "<Attributes>\n</Attributes>\n").expect("file");
        let log = format!("{game}/Game.log");
        std::fs::write(&log, "x").expect("log");
        let paths = Paths::new(Host::new(
            versekit_core::pypath::Flavor::Posix,
            &[
                ("SC_BP_HOME", home.as_str()),
                ("HOME", home.as_str()),
                (versekit_core::paths::ENV_INSTALL_DIR, game.as_str()),
                (game_start::ENV_NO_PROCESS, "1"),
            ],
            &home,
        ));
        let names = BackRoute { paths: &paths, archive: &NoArchive };
        let collection = Collection {
            paths: &paths,
            catalog: &collection::NoCatalog,
            names: &names,
            exports: &collection::NoExports,
            clock: collection::now_text,
        };
        let (news, told) = std::sync::mpsc::channel();
        let data = collection.load();
        let run = Run {
            paths: &paths,
            collection,
            data,
            stamp: None,
            seen: HashSet::new(),
            meta: HashMap::new(),
            pattern: regex::Regex::new("x").expect("pattern"),
            news,
            wake: Box::new(|| {}),
            said: false,
        };
        let mut periodic = Periodic::new(Duration::from_secs(60));
        let changes =
            graphics_apply::Changes { attributes: vec![("VSync".to_string(), Some(0.0))], ..Default::default() };
        assert!(graphics_apply::set_queued(&paths, &changes));
        // The game wrote its log a moment ago: nothing is written, nothing told, the note stays.
        periodic.graphics(&run);
        assert!(told.try_recv().is_err());
        assert_eq!(graphics_apply::queued(&paths), changes);
        // Quiet for a long time: written, told once (a hint and the status), the note gone.
        let old = std::time::SystemTime::now() - Duration::from_secs(7200);
        std::fs::File::options().write(true).open(&log).and_then(|f| f.set_modified(old)).expect("age");
        periodic.graphics_next = Instant::now();
        periodic.graphics(&run);
        let mut said = Vec::new();
        while let Ok(item) = told.try_recv() {
            match item {
                News::Row(Row::Hint { text, .. }) | News::Status(text) => said.push(text.render()),
                _ => {}
            }
        }
        assert_eq!(said.len(), 2, "{said:?}");
        assert!(said.iter().all(|s| s.contains("attributes.xml.versekit-")), "{said:?}");
        assert!(graphics_apply::queued(&paths).is_empty());
        periodic.graphics_next = Instant::now();
        periodic.graphics(&run);
        assert!(told.try_recv().is_err(), "told once");
        let _ = std::fs::remove_dir_all(home);
    }

    /// A job that is over, kept as the main window keeps it.
    fn write_due_job(paths: &Paths, material: &str) {
        let text =
            format!(r#"{{"version": 1, "auftraege": [{{"materials": ["{material}"], "total": "12", "ends": 1.0}}]}}"#);
        let file = paths.app_file(refinery_jobs::FILE);
        std::fs::create_dir_all(std::path::Path::new(&file).parent().expect("folder")).expect("folder");
        std::fs::write(file, text).expect("jobs");
    }

    /// What `Periodic::refinery` told, in kinds: hint rows, system messages, opening and sounds.
    fn told_by_refinery(paths: &Paths) -> Vec<&'static str> {
        let names = BackRoute { paths, archive: &NoArchive };
        let collection = Collection {
            paths,
            catalog: &collection::NoCatalog,
            names: &names,
            exports: &collection::NoExports,
            clock: collection::now_text,
        };
        let (news, told) = std::sync::mpsc::channel();
        let data = collection.load();
        let run = Run {
            paths,
            collection,
            data,
            stamp: None,
            seen: HashSet::new(),
            meta: HashMap::new(),
            pattern: regex::Regex::new("x").expect("pattern"),
            news,
            wake: Box::new(|| {}),
            said: false,
        };
        let mut periodic = Periodic::new(Duration::from_secs(3600));
        periodic.refinery_next = Instant::now();
        periodic.refinery(&run);
        let mut kinds = Vec::new();
        while let Ok(item) = told.try_recv() {
            kinds.push(match item {
                News::Row(_) => "hint",
                News::System(..) => "system",
                News::Opens(Event::Workbench) => "opens-workbench",
                News::Opens(_) => "opens",
                News::Beep => "beep",
                _ => "other",
            });
        }
        kinds
    }

    /// A job of the fabricator that is done opens the overlay by its own event (not by the refinery's), once, and
    /// makes no sound of the refinery.
    #[test]
    fn a_fabricator_job_that_is_done_opens_the_overlay_by_its_own_event() {
        language::set_thread_language(language::Choice::De);
        let (home, paths) = home("werkbank-fertig");
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
        // Entered an hour ago for a minute: done.
        versekit_core::workbench_jobs::add(&paths, "Rüstung", 60, now - 3600.0).expect("entered");
        assert_eq!(told_by_refinery(&paths), ["hint", "system", "opens-workbench"]);
        assert!(told_by_refinery(&paths).is_empty(), "told once");
        // A job still running opens nothing.
        versekit_core::workbench_jobs::add(&paths, "Waffe", 3600, now).expect("entered");
        assert!(told_by_refinery(&paths).is_empty(), "not done yet");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_refinery_job_that_is_done_makes_a_sound_of_its_own_unless_it_is_switched_off() {
        language::set_thread_language(language::Choice::De);
        let (home, paths) = home("raffinerie-ton");
        write_due_job(&paths, "Gold");
        // On without a setting: hint, message of the system, opening, and the sound.
        assert_eq!(told_by_refinery(&paths), ["hint", "system", "opens", "beep"]);
        // Told once: the job is marked.
        assert!(told_by_refinery(&paths).is_empty());
        // Switched off: everything else is told, the sound is not. The sound for a new blueprint does not matter.
        assert!(paths.set_setting(versekit_core::sound::REFINERY_SETTING, versekit_core::jsonio::Value::Bool(false)));
        assert!(paths.set_setting(keys::SOUND, versekit_core::jsonio::Value::Bool(true)));
        write_due_job(&paths, "Quantainium");
        assert_eq!(told_by_refinery(&paths), ["hint", "system", "opens"]);
        // And the other way round: the sound of the refinery on, the one for a blueprint off.
        assert!(paths.set_setting(versekit_core::sound::REFINERY_SETTING, versekit_core::jsonio::Value::Bool(true)));
        assert!(paths.set_setting(keys::SOUND, versekit_core::jsonio::Value::Bool(false)));
        write_due_job(&paths, "Taranite");
        assert_eq!(told_by_refinery(&paths), ["hint", "system", "opens", "beep"]);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_new_number_in_the_settings_is_a_request_to_read_all_logs_again_once() {
        let (home, paths) = home("einlesen-nr");
        let mut seen = paths.setting_int(keys::RESCAN_NR, 0, None, None);
        assert!(!rescan_requested(&paths, &mut seen), "nothing asked at the start");
        let ask = |n: i64| paths.set_setting(keys::RESCAN_NR, versekit_core::jsonio::Value::int(n));
        assert!(ask(1));
        assert!(rescan_requested(&paths, &mut seen), "the window raised the number");
        assert!(!rescan_requested(&paths, &mut seen), "and the request counts once");
        assert!(ask(2));
        assert!(rescan_requested(&paths, &mut seen));
        // A number that is lower (the settings were reset and raised again) is a request too.
        assert!(ask(1));
        assert!(rescan_requested(&paths, &mut seen));
        let _ = std::fs::remove_dir_all(home);
    }

    /// A throwaway home with a game folder of its own (an empty Game.log, `logbackups`).
    fn home_with_game(name: &str) -> (String, String, Paths) {
        let (home, _) = home(name);
        let game = format!("{home}/StarCitizen/LIVE");
        std::fs::create_dir_all(format!("{game}/logbackups")).expect("folder");
        // ⚠ Without a Game.log the folder of the test is not taken, and the real installation would be read.
        std::fs::write(format!("{game}/Game.log"), "").expect("log");
        let paths = Paths::new(Host::new(
            versekit_core::pypath::Flavor::Posix,
            &[
                ("SC_BP_HOME", home.as_str()),
                ("HOME", home.as_str()),
                (versekit_core::paths::ENV_INSTALL_DIR, game.as_str()),
            ],
            &home,
        ));
        assert_eq!(
            paths.game_folder().as_deref(),
            Some(game.as_str()),
            "the game folder of the test, never the real one"
        );
        (home, game, paths)
    }

    /// A catalog the main window built while the overlay runs is said in the status line, with its count and version.
    #[test]
    fn a_catalog_that_came_is_said_in_the_status_line() {
        let (home, _, paths) = home_with_game("katalog-geholt");
        let names = BackRoute { paths: &paths, archive: &NoArchive };
        let collection = Collection {
            paths: &paths,
            catalog: &collection::NoCatalog,
            names: &names,
            exports: &collection::NoExports,
            clock: collection::now_text,
        };
        let (news, told) = std::sync::mpsc::channel();
        let data = collection.load();
        let source = LogSource::new(&paths, LogSource::sources_of(&paths));
        let pattern = phrases::pattern(&source.sources.collect().0);
        let mut run = Run {
            paths: &paths,
            collection,
            data,
            stamp: None,
            seen: HashSet::new(),
            meta: HashMap::new(),
            pattern: pattern.clone(),
            news,
            wake: Box::new(|| {}),
            said: false,
        };
        let mut tail = LogTail::new(&source, ReadState::open(&paths), pattern);
        let mut contracts = Contracts::new(&paths, &mut tail);
        let mut periodic = Periodic::new(Duration::from_secs(3600));
        // The first look finds no catalog: nothing to say.
        take_catalog(&mut run, &mut periodic, &mut contracts);
        assert!(!run.said);
        let blueprint = |name: &str| catalog_cache::Blueprint {
            key: name_key(name),
            name: name.to_string(),
            kind: "WeaponPersonal".into(),
            ..Default::default()
        };
        let catalog = CatalogCache {
            version: "4.3.2-LIVE.10567890".into(),
            blueprints: vec![blueprint("Arclight Pistol"), blueprint("Morozov-SH Helmet")],
            ..Default::default()
        };
        let file = catalog_cache::current_file(&paths);
        let text = jsonio::dumps(&catalog_cache::to_value(&catalog), None, false);
        jsonio::write_text_atomic(&file, &text, jsonio::Newline::Keep).expect("catalog");
        let _ = told.try_iter().count();
        language::set_thread_language(language::Choice::De);
        periodic.catalog_next = Instant::now();
        take_catalog(&mut run, &mut periodic, &mut contracts);
        let said: Vec<String> = told
            .try_iter()
            .filter_map(|news| match news {
                News::Status(Message::Phrase(p)) if p.key == "katalog_geholt" => Some(p.to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(said, ["Bauplan-Katalog geholt: 2 Baupläne (4.3.2-LIVE.10567890)"]);
        assert!(run.said, "it stands until the next round");
        language::set_thread_language(language::Choice::Auto);
        let _ = std::fs::remove_dir_all(home);
    }

    /// The shard comes from the text the contracts read anyway: the whole running log at the start, then each new
    /// section. A log that begins anew and the end of the game take the joins away; the address never comes along.
    #[test]
    fn the_shard_follows_the_text_the_contracts_read() {
        let (home, game, paths) = home_with_game("shard");
        let join = |time: &str, id: &str| {
            format!("<2026-10-30T{time}.000Z> [Notice] <Join PU> address[35.241.242.86] port[64388] shard[{id}]\n")
        };
        let first = "pub_euw1b_11112222_1";
        std::fs::write(format!("{game}/Game.log"), format!("{}nothing\n", join("16:00:00", first))).expect("log");
        let names = BackRoute { paths: &paths, archive: &NoArchive };
        let collection = Collection {
            paths: &paths,
            catalog: &collection::NoCatalog,
            names: &names,
            exports: &collection::NoExports,
            clock: collection::now_text,
        };
        let (news, told) = std::sync::mpsc::channel();
        let data = collection.load();
        let source = LogSource::new(&paths, LogSource::sources_of(&paths));
        let pattern = phrases::pattern(&source.sources.collect().0);
        let run = Run {
            paths: &paths,
            collection,
            data,
            stamp: None,
            seen: HashSet::new(),
            meta: HashMap::new(),
            pattern: pattern.clone(),
            news,
            wake: Box::new(|| {}),
            said: false,
        };
        let mut tail = LogTail::new(&source, ReadState::open(&paths), pattern);
        let mut contracts = Contracts::new(&paths, &mut tail);
        let said = |told: &std::sync::mpsc::Receiver<News>| -> Vec<Vec<String>> {
            told.try_iter()
                .filter_map(|news| {
                    assert!(!format!("{news:?}").contains("35.241.242.86"), "the address is handed on: {news:?}");
                    match news {
                        News::Shard(joins) => Some(joins.list().iter().map(|j| j.shard.id.clone()).collect()),
                        _ => None,
                    }
                })
                .collect()
        };
        let mut shards = Shards::default();
        let log = contracts.at_start(&run).expect("the running log");
        shards.whole(&run, log.lines());
        assert_eq!(said(&told), [vec![first.to_string()]]);
        // The same join once more and a new one: the line follows, each join once.
        let second = "pub_euw1b_33334444_2";
        shards.section(&run, &format!("{}{}", join("16:00:00", first), join("16:30:00", second)), false);
        assert_eq!(said(&told), [vec![first.to_string(), second.to_string()]]);
        // Nothing new: nothing is said.
        shards.section(&run, "nothing\n", false);
        assert!(said(&told).is_empty());
        // A log that begins anew: only its own joins count.
        let third = "pub_use1a_55556666_3";
        shards.section(&run, &join("18:00:00", third), true);
        assert_eq!(said(&told), [vec![third.to_string()]]);
        // The game ended: the line goes, once.
        shards.ended(&run);
        shards.ended(&run);
        assert_eq!(said(&told), [Vec::<String>::new()]);
        // The lines for the log monitor and the handle of a login go to the window; other text says nothing.
        let login = "<2026-10-31T18:01:00.000Z> [Notice] <Legacy login response> [CIG-net] User Login Success - \
                     Handle[Musterpilot] - Time[1]\n";
        let loading = "<2026-10-31T18:05:00.000Z> Loading screen for pu : SC_Frontend closed after 42.10 seconds\n";
        tell_log_lines(&run, &format!("{login}noise\n{loading}"));
        let news: Vec<News> = told.try_iter().collect();
        assert!(
            matches!(news.as_slice(), [News::LogLines(lines), News::OwnHandle(handle)]
                if lines.len() == 1 && lines[0].contains("42.10") && handle == "Musterpilot"),
            "{news:?}"
        );
        tell_log_lines(&run, "nothing\n");
        assert!(told.try_iter().next().is_none(), "nothing to show, nothing said");
        let _ = std::fs::remove_dir_all(home);
    }

    /// Reading all logs again takes the blueprints in, judges the mission log anew and keeps the four numbers for the
    /// main window; the sentence stands in the status line until the next round.
    #[test]
    fn reading_all_logs_again_judges_the_contracts_too_and_keeps_the_result() {
        let (home, game, paths) = home_with_game("alles-neu");
        let at = |second: u32| format!("<2026-10-01T10:00:{second:02}.000Z>");
        let session = [
            format!("{} start OnClientSpawned", at(0)),
            format!(
                "{} [Notice] <SHUDEvent_OnNotification> Added notification \"Auftrag angenommen: Hunt: \" [1] MissionId: [m1], ObjectiveId: []",
                at(1)
            ),
            format!("{} <EndMission> x MissionId[m1] Player[Spieler] CompletionType[Complete]", at(5)),
            format!(
                "{} [Notice] <SHUDEvent_OnNotification> Added notification \"Auftrag abgeschlossen: Hunt: \" [1] MissionId: [m1], ObjectiveId: []",
                at(5)
            ),
            format!(
                "{} <SHUDEvent_OnNotification> Added notification \"Bauplan erhalten: Arclight Pistol: \" [1]",
                at(6)
            ),
            format!("{} ende", at(59)),
        ];
        std::fs::write(format!("{game}/logbackups/a.log"), session.join("\n") + "\n").expect("backup");
        let names = BackRoute { paths: &paths, archive: &NoArchive };
        let collection = Collection {
            paths: &paths,
            catalog: &collection::NoCatalog,
            names: &names,
            exports: &collection::NoExports,
            clock: collection::now_text,
        };
        let (news, told) = std::sync::mpsc::channel();
        let data = collection.load();
        let source = LogSource::new(&paths, LogSource::sources_of(&paths));
        let pattern = phrases::pattern(&source.sources.collect().0);
        let mut run = Run {
            paths: &paths,
            collection,
            data,
            stamp: None,
            seen: HashSet::new(),
            meta: HashMap::new(),
            pattern: pattern.clone(),
            news,
            wake: Box::new(|| {}),
            said: false,
        };
        let mut tail = LogTail::new(&source, ReadState::open(&paths), pattern);
        let mut contracts = Contracts::new(&paths, &mut tail);
        rescan(&mut run, &source, &mut contracts);
        let result = RescanResult::load(&paths).expect("the result is kept for the main window");
        assert_eq!(
            (result.files, result.blueprints, result.contracts_new, result.contracts_corrected),
            (1, 1, 1, 0),
            "one log, one blueprint, one contract the mission log did not have"
        );
        let said: Vec<News> = told.try_iter().collect();
        let keyed = |news: &News| match news {
            News::Status(Message::Phrase(p)) => Some(("status", p.key.clone())),
            News::Row(Row::Hint { text: Message::Phrase(p), .. }) => Some(("row", p.key.clone())),
            _ => None,
        };
        let keys: Vec<(&str, String)> = said.iter().filter_map(keyed).collect();
        assert!(keys.contains(&("status", "neu_gelesen".into())), "{keys:?}");
        assert!(keys.contains(&("row", "neu_gelesen".into())), "a row that stays: {keys:?}");
        // The count of the round waits once, then comes back.
        run.tell_count(true);
        assert!(told.try_recv().is_err(), "the sentence stands until the next round");
        run.tell_count(true);
        assert!(matches!(told.try_recv(), Ok(News::Status(Message::Phrase(p))) if p.key == "ov_status"));
        let _ = std::fs::remove_dir_all(home);
    }

    /// Without a game folder the logs cannot be read again: the answer is the failure, not a result of zeros.
    #[test]
    fn reading_all_logs_again_without_the_game_folder_says_that_it_did_not_work() {
        let (home, _) = home("alles-neu-fehlt");
        let gone = format!("{home}/StarCitizen/LIVE");
        let paths = Paths::new(Host::new(
            versekit_core::pypath::Flavor::Posix,
            &[
                ("SC_BP_HOME", home.as_str()),
                ("HOME", home.as_str()),
                (versekit_core::paths::ENV_INSTALL_DIR, gone.as_str()),
            ],
            &home,
        ));
        assert_eq!(paths.game_folder(), None, "no folder of the test, and the real installation is not taken");
        let names = BackRoute { paths: &paths, archive: &NoArchive };
        let collection = Collection {
            paths: &paths,
            catalog: &collection::NoCatalog,
            names: &names,
            exports: &collection::NoExports,
            clock: collection::now_text,
        };
        let (news, told) = std::sync::mpsc::channel();
        let data = collection.load();
        let source = LogSource::new(&paths, LogSource::sources_of(&paths));
        let pattern = phrases::pattern(&source.sources.collect().0);
        let mut run = Run {
            paths: &paths,
            collection,
            data,
            stamp: None,
            seen: HashSet::new(),
            meta: HashMap::new(),
            pattern: pattern.clone(),
            news,
            wake: Box::new(|| {}),
            said: false,
        };
        let mut tail = LogTail::new(&source, ReadState::open(&paths), pattern);
        let mut contracts = Contracts::new(&paths, &mut tail);
        rescan(&mut run, &source, &mut contracts);
        let result = RescanResult::load(&paths).expect("the failure is kept for the main window");
        assert!(result.failed, "{result:?}");
        assert_eq!(result.files, 0);
        let said: Vec<News> = told.try_iter().collect();
        let keyed = |news: &News| match news {
            News::Status(Message::Phrase(p)) => Some(("status", p.key.clone())),
            News::Row(Row::Hint { text: Message::Phrase(p), .. }) => Some(("row", p.key.clone())),
            _ => None,
        };
        let keys: Vec<(&str, String)> = said.iter().filter_map(keyed).collect();
        assert!(keys.contains(&("status", "neu_gelesen_fehler".into())), "{keys:?}");
        assert!(keys.contains(&("row", "neu_gelesen_fehler".into())), "{keys:?}");
        assert!(!keys.iter().any(|(_, key)| key == "neu_gelesen"), "no result of zeros: {keys:?}");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_test_channel_gets_its_hint_once_and_again_after_a_change() {
        language::set_thread_language(language::Choice::De);
        let mut told = None;
        assert!(channel_hint(&mut told, None).is_none());
        assert!(channel_hint(&mut told, Some("C:\\RSI\\StarCitizen\\LIVE")).is_none(), "LIVE says nothing");
        let hint = channel_hint(&mut told, Some("C:\\RSI\\StarCitizen\\PTU")).expect("PTU is a test build");
        let Row::Hint { text, .. } = hint else { panic!("a hint row") };
        assert!(text.render().contains("PTU") && text.render().contains("vertraulich"), "{}", text.render());
        assert!(channel_hint(&mut told, Some("C:\\RSI\\StarCitizen\\PTU")).is_none(), "said once");
        assert!(channel_hint(&mut told, Some("C:\\RSI\\StarCitizen\\LIVE")).is_none());
        assert!(
            channel_hint(&mut told, Some("C:\\RSI\\StarCitizen\\EPTU")).is_some(),
            "another test channel: said again"
        );
    }

    #[test]
    fn the_code_says_class_size_and_grade() {
        language::set_thread_language(language::Choice::De);
        assert_eq!(code(Some("Military"), Some(2), Some('A')), "Militär · S2 · A");
        assert_eq!(code(None, Some(1), None), "S1");
        assert_eq!(code(Some("Unbekannt"), None, None), "Unbekannt");
        let extra: Extra = ("Civ".into(), Some("3".into()), Some("A".into()));
        assert_eq!(code_from_log(Some(&extra)), "Zivil · S3 · A");
        let dash: Extra = ("Ind".into(), Some("–".into()), Some("–".into()));
        assert_eq!(code_from_log(Some(&dash)), "Industrie");
        assert_eq!(code_from_log(None), "");
    }

    #[test]
    fn a_gap_names_its_moments() {
        let gap = Gap::Missing { last: 1_700_000_000.0, oldest: 1_700_100_000.0 };
        let phrase = gap_phrase(&gap);
        assert_eq!(phrase.key, "m_luecke_logs");
        assert_eq!(phrase.args.len(), 2);
        assert!(gap_phrase(&Gap::NoLogs).args.is_empty());
    }

    /// The archive given with `--archiv` is the one read, wherever the game folder is;
    /// without it the archive of the game folder stays.
    #[test]
    fn the_archive_given_from_outside_is_read() {
        // A made-up computer with a game folder of its own (a `Game.log` makes it one): the real
        // installation does not count.
        let root = std::env::temp_dir().join(format!("versekit-archiv-{}", versekit_core::testdir::run_tag()));
        let live = root.join("LIVE");
        std::fs::create_dir_all(&live).expect("game folder");
        std::fs::write(live.join("Game.log"), "").expect("Game.log");
        let game = live.to_string_lossy();
        let host = || {
            Host::new(versekit_core::pypath::Flavor::native(), &[(versekit_core::paths::ENV_INSTALL_DIR, &game)], "/")
        };
        let given = paths_for(host(), Some("X:/Archiv/Data.p4k"));
        assert_eq!(given.archive_file().as_deref(), Some("X:/Archiv/Data.p4k"));
        let plain = paths_for(host(), None);
        let own = versekit_core::pypath::Flavor::native().join(&game, "Data.p4k");
        assert_eq!(plain.archive_file(), Some(own), "the game folder's");
        let _ = std::fs::remove_dir_all(root);
    }

    /// The threads of the texts and the signatures keep the folders the window was given from outside: a copy of
    /// the data stays a copy for them (`testmode::on_copy`), a game folder given stays theirs.
    #[test]
    fn a_thread_keeps_the_folders_given_to_the_window() {
        let window = Paths::new(Host::new(versekit_core::pypath::Flavor::native(), &[], "/"));
        window.set_given_data("X:/Kopie/Daten", GivenBy::CommandLine);
        window.set_given_game("X:/Kopie/Spiel/LIVE", GivenBy::CommandLine);
        let thread = thread_paths(&window, Some("X:/Archiv/Data.p4k"));
        let given = thread.given();
        assert_eq!(given.data.map(|f| f.folder).as_deref(), Some("X:/Kopie/Daten"));
        assert_eq!(given.game.map(|f| f.folder).as_deref(), Some("X:/Kopie/Spiel/LIVE"));
        assert_eq!(thread.archive_file().as_deref(), Some("X:/Archiv/Data.p4k"));
        assert!(versekit_core::testmode::on_copy(&thread), "the thread knows it runs on a copy");
        assert!(window.given().archive.is_none(), "the window's own paths stay as they were");
    }
}
