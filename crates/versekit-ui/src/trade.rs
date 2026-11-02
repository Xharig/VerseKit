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

//! What the three trade pages (buy and sell, routes, shops) share: how money,
//! age and distance are written, a pair of toggle buttons, a job that runs
//! beside the page and tells it when it is done.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{SystemTime, UNIX_EPOCH};

use egui::{Align, Layout, Rect, RichText, Sense, Ui, vec2};
use versekit_core::erkul_slots::ErkulData;
use versekit_core::formats;
use versekit_core::jsonio::Map;
use versekit_core::language::{Arg, Lang, relative_age, tf};
use versekit_core::net::{Net, Web};
use versekit_core::paths::Paths;
use versekit_core::raw_prices::RawPrices;
use versekit_core::ship_market::Market;
use versekit_core::shops::{self, Catalog, Shops};
use versekit_core::theme::Scheme;
use versekit_core::{cart, erkul_fetch, hangar, ships, stock, stock_fetch};
use versekit_gamedata::cart_data::CartData;
use versekit_gamedata::craft_book::CraftBook;
use versekit_gamedata::values_store;

use crate::color;
use crate::widgets::{self, ButtonKind};

/// Seconds since 1970 now.
pub fn now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

/// A sum without unit: `12.345`.
pub fn plain(lang: Lang, value: f64) -> String {
    // Half to even like the format of 4.12, and nothing odd for values that are no numbers.
    let whole = if value.is_finite() { stock::py_round_int(value) } else { 0 };
    formats::count(lang, whole)
}

/// A sum with its unit: `12.345 aUEC`.
pub fn auec(lang: Lang, value: f64) -> String {
    format!("{} aUEC", plain(lang, value))
}

/// How old a report is, as the pages write it (`vor 3 Std.`); an unknown age says nothing.
pub fn age_text(seconds: Option<f64>) -> String {
    seconds.map_or_else(String::new, relative_age)
}

/// The name of the program for the requests to UEX.
pub fn user_agent() -> String {
    versekit_core::basetool::config::user_agent(env!("CARGO_PKG_VERSION"))
}

/// The paths for a job in another thread: a second set on the same host.
pub fn job_paths(paths: &Paths) -> Arc<Paths> {
    Arc::new(Paths::new(paths.host.clone()))
}

/// A job beside the page: it fills a slot and wakes the page.
pub struct Job<T> {
    slot: Receiver<T>,
}

impl<T: Send + 'static> Job<T> {
    /// Runs `work` in a thread of its own; the page is woken when it is done.
    pub fn spawn(ctx: &egui::Context, name: &str, work: impl FnOnce() -> T + Send + 'static) -> Job<T> {
        let (send, slot) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        let started = std::thread::Builder::new().name(name.to_string()).spawn(move || {
            let result = work();
            let _ = send.send(result);
            ctx.request_repaint();
        });
        if started.is_err() {
            // No thread: the slot stays empty and closed; `poll` reports it as done without a result.
            return Job { slot: std::sync::mpsc::channel().1 };
        }
        Job { slot }
    }

    /// How the job stands.
    pub fn poll(&self) -> Polled<T> {
        match self.slot.try_recv() {
            Ok(result) => Polled::Done(result),
            Err(TryRecvError::Empty) => Polled::Running,
            Err(TryRecvError::Disconnected) => Polled::Lost,
        }
    }
}

/// How a job stands.
pub enum Polled<T> {
    Running,
    Done(T),
    /// The thread ended without a result.
    Lost,
}

/// The function object of requests, boxed.
pub type Fetcher = Box<dyn Fn(&str) -> Result<Vec<u8>, versekit_core::scmdb::FetchError> + Sync + Send>;

/// The function object of the real requests. The tests of this crate never
/// ask the network: theirs says "off".
pub fn fetcher() -> Fetcher {
    if cfg!(test) {
        return Box::new(|_| Err(versekit_core::scmdb::FetchError::Offline));
    }
    Box::new(stock_fetch::live_fetcher(&user_agent()))
}

/// How long erkul.games gets for one file, and the most one file may be.
const ERKUL_SECONDS: u64 = 30;
const ERKUL_LIMIT: u64 = 64 * 1024 * 1024;

/// The feed of erkul.games that asks the net. The tests of this crate never ask it: theirs gets nothing.
/// A file that fails goes into the error log (`erkul.holen.<katalog|index|hersteller|schiff|teile>`), the
/// switched-off net not. One feed serves one run: it keeps the catalog of the run for the labels.
pub fn erkul_feed() -> impl erkul_fetch::Feed {
    let web = Web::new(&user_agent(), std::time::Duration::from_secs(ERKUL_SECONDS), ERKUL_LIMIT);
    let catalog = std::cell::RefCell::new(None);
    erkul_fetch::FnFeed(move |path: &str| {
        if cfg!(test) {
            return None;
        }
        erkul_get(&web, path, &catalog, &|label, error| versekit_core::errors::record(label, error))
    })
}

/// One file of erkul over `net`, or `None`. A failure is handed to `record` with its label (`erkul._fetch`):
/// no answer, and an answer that is no packed file. The switched-off net is no failure. The catalog that came
/// is kept in `catalog`, so the files after it are labelled by it.
pub(crate) fn erkul_get(
    net: &dyn Net,
    path: &str,
    catalog: &std::cell::RefCell<Option<versekit_core::jsonio::Value>>,
    record: &dyn Fn(&str, &dyn std::fmt::Display),
) -> Option<Vec<u8>> {
    let label = || format!("erkul.holen.{}", erkul_fetch::fetch_label(path, catalog.borrow().as_ref()));
    match net.get(&erkul_fetch::address(path)) {
        Ok(bytes) => {
            let unpacked = erkul_fetch::unpack(&bytes);
            if unpacked.is_none() {
                record(&label(), &"no packed JSON");
                return None;
            }
            if erkul_fetch::fetch_label(path, None) == "katalog" {
                *catalog.borrow_mut() = unpacked;
            }
            Some(bytes)
        }
        Err(versekit_core::net::FetchError::Offline) => None,
        Err(error) => {
            record(&label(), &error);
            None
        }
    }
}

/// `fleet.fetch_missing`: the slots of the ships of the hangar and of the wishlist that erkul has and the store
/// lacks, fetched and stored. Gives how many ships were fetched.
pub fn fetch_slots(paths: &Paths, feed: &dyn erkul_fetch::Feed) -> usize {
    erkul_fetch::add_missing(paths, feed, &slot_rows(paths), now())
}

/// The ships of the hangar and of the wishlist whose slots are fetched (`fleet.id_sets`, `wishlist_id_sets`).
pub fn slot_rows(paths: &Paths) -> Vec<erkul_fetch::ShipRow> {
    let fleet = hangar::read(&paths.app_file(hangar::FILE)).unwrap_or_else(hangar::empty);
    let uex = ships::UexShips::load(&paths.app_file(ships::UEX_FILE));
    erkul_fetch::ship_rows(&fleet, &|name| uex.manufacturer(name))
}

/// The shop prices the shopping list still lacks, fetched one after the other (the preloading of 4.12 after the
/// raw material prices). The recipes come from the kept game values; without them the list has no lines yet.
pub fn fetch_missing_shop_prices(paths: &Paths, fetch: &Fetcher) -> usize {
    let fleet = hangar::read(&paths.app_file(hangar::FILE)).unwrap_or_else(hangar::empty);
    let values = values_store::kept(&paths.app_file(values_store::CACHE_FILE)).unwrap_or_default();
    let book = CraftBook::build(&values);
    let (mut book_of_shops, raw, market) = (Shops::load(paths), RawPrices::load(paths), Market::load(paths));
    let erkul = ErkulData::load(paths);
    let slots = |ship: &Map, chosen: &Map| erkul.cart_slots(ship, chosen);
    let missing = {
        let data = CartData {
            values: &values,
            book: &book,
            shops: &book_of_shops,
            raw: &raw,
            market: &market,
            slots: Some(&slots),
        };
        cart::missing_prices(&cart::invoice(&fleet, &data).lines)
    };
    let mut catalog = Catalog::load(paths);
    let mut fetched = 0;
    for (code, name) in &missing {
        if !book_of_shops.known(code)
            && shops::fetch_part(paths, &mut book_of_shops, &mut catalog, (code, name), false, now(), fetch)
        {
            fetched += 1;
        }
    }
    fetched
}

/// Starts the quiet fetches in the background (once per process): the lists
/// of the trade pages are there when a page is opened. First the slots of the
/// own ships (only then is known which parts are needed), then the raw
/// material prices with the first pass, then the shop prices the shopping
/// list lacks. The slots go the way of every other request ([`crate::slot_sync`]).
/// Tests start nothing.
pub fn start_background(paths: &Paths, ctx: &egui::Context) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static STARTED: AtomicBool = AtomicBool::new(false);
    if cfg!(test) || STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let (paths, ctx) = (job_paths(paths), ctx.clone());
    let slots = crate::slot_sync::global();
    slots.wake(&ctx);
    let _ = std::thread::Builder::new().name("trade-sync".to_string()).spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(5));
        slots.request(&paths, crate::slot_sync::Trigger::Start);
        // The shop prices need the slots: they wait for the run, at most as long as a slow net takes.
        slots.wait_idle(std::time::Duration::from_secs(10 * 60));
        let mut first = true;
        loop {
            versekit_core::trade_sync::pass(&paths, now(), &fetcher());
            if first {
                first = false;
                fetch_missing_shop_prices(&paths, &fetcher());
            }
            ctx.request_repaint();
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    });
}

/// Two buttons next to each other, the chosen one filled. Returns whether
/// the choice changed.
pub fn toggle_pair(ui: &mut Ui, scheme: &Scheme, words: [&str; 2], chosen: &mut usize) -> bool {
    let before = *chosen;
    ui.horizontal_wrapped(|row| {
        for (i, word) in words.iter().enumerate() {
            let kind = if *chosen == i { ButtonKind::Strong } else { ButtonKind::Normal };
            if row.add(widgets::Button::new(word, kind, scheme)).clicked() {
                *chosen = i;
            }
        }
    });
    before != *chosen
}

/// A quiet small line of help or state.
pub fn quiet(ui: &mut Ui, scheme: &Scheme, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).small().color(color(scheme.sub))).wrap());
}

/// A text in a color of the scheme.
pub fn colored(ui: &mut Ui, tone: versekit_core::theme::Color, text: &str) {
    ui.label(RichText::new(text).color(color(tone)));
}

/// The countdown in a button: `m:ss`.
pub fn countdown(seconds: i64) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// The colour of the countdown of a button blocked after a fetch that worked: it ripens from muted (over 30
/// minutes) over gold (down to 5 minutes) to the accent. Never red: red says failure, and the fetch worked.
pub fn cooldown_color(scheme: &Scheme, seconds: i64) -> versekit_core::theme::Color {
    if seconds > 30 * 60 {
        scheme.sub
    } else if seconds > 5 * 60 {
        scheme.gold
    } else {
        scheme.accent
    }
}

/// A row that can be clicked as a whole: striped, light under the mouse.
/// Returns the rectangle of the row and whether it was clicked.
pub fn clickable_row(ui: &mut Ui, scheme: &Scheme, stripe: bool, height: f32) -> (Rect, bool) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, 0.0, color(scheme.hover));
    } else if stripe {
        ui.painter().rect_filled(rect, 0.0, color(scheme.bar));
    }
    (rect, response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked())
}

/// How far the content of a scroll area sticks out at the right: it would be
/// cut off unseen, so the edge check counts it.
pub fn overflow_of<R>(shown: &egui::containers::scroll_area::ScrollAreaOutput<R>) -> f32 {
    (shown.content_size.x - shown.inner_rect.width()).max(0.0)
}

/// The inside of a clickable row: a child area at the left in which the labels
/// neither take the click nor the mouse from the row (they cannot be selected)
/// and are cut at its edge, and the text at the right drawn first so the left
/// part leaves room for it.
pub fn row_content(ui: &mut Ui, rect: Rect, right: Option<RichText>) -> Ui {
    let area = rect.shrink2(vec2(8.0, 0.0));
    let mut left_edge = area.right();
    if let Some(right) = right {
        let galley = egui::WidgetText::from(right).into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            egui::TextStyle::Body,
        );
        let at = egui::pos2((area.right() - galley.size().x).max(area.left()), area.center().y - galley.size().y / 2.0);
        left_edge = (at.x - 8.0).max(area.left());
        ui.painter().galley(at, galley, ui.visuals().text_color());
    }
    let left = Rect::from_min_max(area.min, egui::pos2(left_edge, area.bottom()));
    let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(left).layout(Layout::left_to_right(Align::Center)));
    inner.set_clip_rect(left.intersect(ui.clip_rect()));
    inner.style_mut().interaction.selectable_labels = false;
    inner
}

/// A row of a list that is a link: the text in the accent, the whole row clickable.
pub fn link_row_striped(ui: &mut Ui, scheme: &Scheme, stripe: bool, text: &str) -> bool {
    let height = ui.text_style_height(&egui::TextStyle::Body) + 10.0;
    let (rect, clicked) = clickable_row(ui, scheme, stripe, height);
    let mut inner = row_content(ui, rect, None);
    inner.add(egui::Label::new(RichText::new(text).color(color(scheme.accent))).truncate());
    clicked
}

/// [`link_row_striped`] without a stripe.
pub fn link_row(ui: &mut Ui, scheme: &Scheme, text: &str) -> bool {
    link_row_striped(ui, scheme, false, text)
}

/// A line with the text at the left and a second text at the right.
pub fn two_sided(ui: &mut Ui, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui)) {
    ui.horizontal_wrapped(|row| {
        left(row);
        row.with_layout(Layout::right_to_left(Align::Center), right);
    });
}

/// `{name}` texts of the language file with one argument.
pub fn with(key: &str, name: &str, value: impl Into<Arg>) -> String {
    tf(key, &[(name, value.into())])
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use versekit_core::erkul_fetch::{FnFeed, pack};
    use versekit_core::net::{Fake, FetchError, Headers};

    /// A net that is switched off.
    struct Off;

    impl Net for Off {
        fn get(&self, _: &str) -> Result<Vec<u8>, FetchError> {
            Err(FetchError::Offline)
        }

        fn head(&self, _: &str) -> Result<Headers, FetchError> {
            Err(FetchError::Offline)
        }
    }

    /// One run of the slots over `net`; gives what went into the error log.
    fn run(net: &dyn Net, paths: &Paths) -> Vec<(String, String)> {
        let logged = RefCell::new(Vec::new());
        let catalog = RefCell::new(None);
        let record = |label: &str, error: &dyn std::fmt::Display| {
            logged.borrow_mut().push((label.to_string(), error.to_string()));
        };
        let feed = FnFeed(|path: &str| erkul_get(net, path, &catalog, &record));
        fetch_slots(paths, &feed);
        logged.into_inner()
    }

    fn url(path: &str) -> String {
        erkul_fetch::address(&format!("LIVE/{path}"))
    }

    #[test]
    fn a_file_that_fails_is_logged_under_its_kind_and_the_switched_off_net_is_not() {
        let hangar = r#"{"format": 1, "schiffe": [{"name": "Arrow", "hersteller": "Anvil", "hkurz": "ANVL"}]}"#;
        let (paths, dir) = crate::slot_sync::tests::folder("fehlerspur", hangar);
        let catalog = r#"{"dataVersion": "4.3", "groups": [{"indexPath": "i1.bin"}, {"indexPath": "i2.bin"}],
            "families": [{"kind": "manufacturers", "path": "manufacturers.m.bin"},
                         {"kind": "paints", "path": "paints.p.bin"}]}"#;
        let net = Fake::default()
            .with(&url("catalog.bin"), pack(catalog))
            .with(&url("i1.bin"), pack(r#"{"blobs": [{"id": "anvl_arrow", "path": "ships/a.bin"}]}"#))
            .with(&url("paints.p.bin"), b"not packed".to_vec());
        let logged = run(&net, &paths);
        let labels: Vec<&str> = logged.iter().map(|(label, _)| label.as_str()).collect();
        for wanted in ["erkul.holen.teile", "erkul.holen.hersteller", "erkul.holen.index", "erkul.holen.schiff"] {
            assert!(labels.contains(&wanted), "{wanted}: {logged:?}");
        }
        assert!(!labels.contains(&"erkul.holen.katalog"), "{logged:?}");
        assert!(logged.iter().any(|(_, text)| text == &FetchError::Missing.to_string()), "{logged:?}");
        // No catalog: that is said, and nothing after it is asked.
        let logged = run(&Fake::default(), &paths);
        assert_eq!(logged, [("erkul.holen.katalog".to_string(), FetchError::Missing.to_string())]);
        // The net switched off: nothing in the log.
        assert!(run(&Off, &paths).is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The stores of a test copy (`VERSEKIT_INTERN`, its internal data folder) copied into a folder of its own:
    /// the hangar, its slots and the parts reach the shopping list, and a run without the net changes nothing.
    /// `VERSEKIT_INTERN=<…\VerseKit-Daten\Intern> cargo test -p versekit-ui testkopie -- --ignored --nocapture`.
    #[test]
    #[ignore = "needs the internal data folder of a test copy"]
    fn testkopie_goes_from_the_hangar_to_the_shopping_list() {
        use crate::slot_sync::{Answer, SlotSync, Trigger};
        use versekit_core::jsonio::Value;
        let Ok(intern) = std::env::var("VERSEKIT_INTERN") else { return };
        let (paths, dir) = crate::slot_sync::tests::folder("testkopie", "{}");
        let files = [ships::ERKUL_FILE, versekit_core::erkul_slots::PARTS_FILE, hangar::FILE, ships::UEX_FILE];
        for file in files {
            std::fs::copy(std::path::Path::new(&intern).join(file), dir.join(file)).expect(file);
        }
        let known = ships::ErkulShips::load(&paths.app_file(ships::ERKUL_FILE));
        let rows = slot_rows(&paths);
        let unknown: Vec<String> = rows
            .iter()
            .filter(|r| {
                !known.knows(ships::ShipId {
                    name: &r.name,
                    maker: &r.maker,
                    short: &r.short,
                    maker_short: &r.maker_short,
                })
            })
            .map(|r| r.name.clone())
            .collect();
        println!(
            "erkul {} · {} Zeilen (Hangar und Wunsch) · ohne Steckplätze {}: {}",
            known.game_version(),
            rows.len(),
            unknown.len(),
            unknown.join(", ")
        );
        assert!(!known.game_version().is_empty() && rows.len() > unknown.len(), "slots for some ships");
        assert_eq!(erkul_fetch::any_unknown(&paths, &rows, now()), !unknown.is_empty() || ships_stale(&paths));

        // Entering the page without the net: a run, nothing written.
        let before: Vec<Vec<u8>> = files.iter().map(|f| std::fs::read(dir.join(f)).unwrap()).collect();
        let sync = SlotSync::new(std::sync::Arc::new(|| Box::new(FnFeed(|_: &str| None))));
        let answer = sync.request(&paths, Trigger::PageOpen);
        assert!(sync.wait_idle(crate::slot_sync::tests::WAIT));
        println!("Seite betreten ohne Netz: {answer:?}, Läufe {}", sync.runs());
        assert_eq!(answer == Answer::Started, sync.runs() == 1);
        let after: Vec<Vec<u8>> = files.iter().map(|f| std::fs::read(dir.join(f)).unwrap()).collect();
        assert!(before == after, "a run without the net writes nothing");

        // The shopping list from the hangar, with the stored slots and parts.
        let fleet = hangar::read(&paths.app_file(hangar::FILE)).expect("hangar");
        let planned: Vec<String> = hangar::ships(&fleet)
            .into_iter()
            .filter(|s| matches!(s.iter().find(|(k, _)| k == "belegung"), Some((_, Value::Object(m))) if !m.is_empty()))
            .map(|s| hangar::field_text(s, "name").to_string())
            .collect();
        let values = versekit_gamedata::values::GameValues::default();
        let book = CraftBook::build(&values);
        let (shops, raw, market) = (Shops::load(&paths), RawPrices::load(&paths), Market::load(&paths));
        let erkul = ErkulData::load(&paths);
        let slots = |ship: &Map, chosen: &Map| erkul.cart_slots(ship, chosen);
        let data =
            CartData { values: &values, book: &book, shops: &shops, raw: &raw, market: &market, slots: Some(&slots) };
        let invoice = cart::invoice(&fleet, &data);
        let parts = invoice.lines.iter().filter(|l| l.kind != cart::Kind::Ship).count();
        println!(
            "Schiffe mit Plan {} · Zeilen {} (Teile {}) · Schiffe beteiligt {} · ohne Steckplatz-Daten {:?} · Preise offen {}",
            planned.len(),
            invoice.lines.len(),
            parts,
            invoice.ships,
            invoice.without_slot_data,
            cart::missing_prices(&invoice.lines).len()
        );
        // A ship with a plan and slots has lines; only a ship erkul does not know stands without slot data.
        for name in &invoice.without_slot_data {
            assert!(unknown.contains(name), "{name} has slots and still stands without");
        }
        if planned.iter().any(|p| !unknown.contains(p)) {
            assert!(parts > 0, "a plan on a ship with slots gives lines");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    fn ships_stale(paths: &Paths) -> bool {
        erkul_fetch::SHIPS_STORE.stale(paths, now())
    }
}
