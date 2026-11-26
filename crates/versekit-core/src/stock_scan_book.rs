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

//! What a stock scan does with what the reader took from an info card: it **sets** the amount of the item at the
//! place the player stands (never adds), into the own stock or into the stock of the unit, writes down what it did
//! so that it can be taken back, and says in a notice which stock it went to.
//!
//! The reader of the card gives a [`Reading`]; the hotkey asks [`run`] to book it. Everything a scan leaves is in
//! [`FILE`]: the scans (with the value before, for [`undo`]) and the open questions (a reading that was not sure,
//! the player picks the item in the list of the scans).
//!
//! * The own stock: an item counted in pieces and goods without a quality go into the trade stock, goods with a
//!   quality into the material stock (`stock`). The place is the one of the Game.log ([`current_place`]); without
//!   one nothing is booked.
//! * The stock of the unit goes through [`UnitStock`] ([`org`] books at the Basetool); it is allowed only with the
//!   setting "Book as a logistician" ([`crate::overlay::keys::LOGISTICIAN`]) and with the right to write there.

use crate::errors;
use crate::jsonio::{self, Value};
use crate::language::{self, Arg, Lang};
use crate::paths::{self, Paths};
use crate::scan_notice::{Notice, NoticeKind};
use crate::stock;
use crate::stock_journal::names::Places;
use crate::text::py_strip;

pub mod org;

pub use crate::scan_notice::Target;

/// The file of the scans and the open questions in the data folder.
pub const FILE: &str = "lagerscans.json";
/// The most scans the file keeps; the oldest go first.
pub const MAX_SCANS: usize = 200;
/// A scan round at a place ends after this many seconds without a scan of its stock; the next scan sets anew.
pub const ROUND_PAUSE: i64 = 600;
/// The most open questions the file keeps; the oldest go first.
pub const MAX_QUESTIONS: usize = 30;

/// How an item is counted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Measure {
    /// In SCU, with decimals.
    Scu,
    /// In whole pieces.
    Pieces,
}

/// What the reader took from the card: the name the game shows, its key, the amount and how it counts, the quality
/// where the card names one.
#[derive(Clone, Debug, PartialEq)]
pub struct ScannedItem {
    pub name: String,
    pub key: String,
    pub quantity: f64,
    pub measure: Measure,
    pub quality: Option<u32>,
    /// The tile on the screen (left, top, width, height in pixels), where the reader gives it.
    pub tile_rect: Option<(i64, i64, i64, i64)>,
    /// The fingerprint of the picture of the tile as the reader stores it (`TilePrint::to_bytes`), where it gives
    /// one. Compared only by [`tile_prints_same`], never by equality.
    pub tile_print: Option<Vec<u8>>,
}

/// Whether two fingerprints of tiles show the same tile: by likeness ([`crate::stock_scan::TilePrint::same`]), as
/// the info card hides another part of the tile with every mouse position. Bytes that are no fingerprint show none.
pub fn tile_prints_same(a: &[u8], b: &[u8]) -> bool {
    use crate::stock_scan::TilePrint;
    match (TilePrint::from_bytes(a), TilePrint::from_bytes(b)) {
        (Some(a), Some(b)) => a.same(&b),
        _ => false,
    }
}

/// How far two places of a tile on the screen may differ and still be the same place, in pixels: the reader finds
/// the edges of a tile a few pixels apart from picture to picture.
pub const TILE_PLACE_TOLERANCE: i64 = 8;

/// Whether two places of tiles on the screen are the same place ([`TILE_PLACE_TOLERANCE`]).
fn same_place(a: (i64, i64, i64, i64), b: (i64, i64, i64, i64)) -> bool {
    let near = |x: i64, y: i64| (x - y).abs() <= TILE_PLACE_TOLERANCE;
    near(a.0, b.0) && near(a.1, b.1) && near(a.2, b.2) && near(a.3, b.3)
}

impl ScannedItem {
    pub fn new(name: &str, quantity: f64, measure: Measure, quality: Option<u32>) -> ScannedItem {
        ScannedItem {
            name: py_strip(name).to_string(),
            key: paths::name_key(name),
            quantity,
            measure,
            quality,
            tile_rect: None,
            tile_print: None,
        }
    }

    /// The same item (key, quality, how it counts).
    fn same_item(&self, other: &ScannedItem) -> bool {
        self.key == other.key && self.quality == other.quality && self.measure == other.measure
    }

    /// The same tile on the screen: the same place and, where both have a picture, the same fingerprint (a scrolled
    /// list puts another tile there); without a place, the same fingerprint alone.
    fn same_tile(&self, other: &ScannedItem) -> bool {
        let prints = match (&self.tile_print, &other.tile_print) {
            (Some(a), Some(b)) => Some(tile_prints_same(a, b)),
            _ => None,
        };
        match (self.tile_rect, other.tile_rect) {
            (Some(a), Some(b)) => same_place(a, b) && prints.unwrap_or(true),
            (None, None) => prints == Some(true),
            _ => false,
        }
    }
}

/// What the reader made of the card.
#[derive(Clone, Debug, PartialEq)]
pub enum Reading {
    /// Read sure.
    Sure(ScannedItem),
    /// Not sure: `item` holds the amount and the best name read (empty where none), `candidates` the names in
    /// question, `picture` the part of the screen as PNG.
    Unsure { item: ScannedItem, candidates: Vec<String>, picture: Option<Vec<u8>> },
    /// Nothing read.
    Nothing,
}

/// Why the stock of the unit did not book.
#[derive(Clone, Debug, PartialEq)]
pub enum UnitError {
    /// No connection to the unit's stock, no right to write, or not a logistician there.
    NoRight,
    /// The item is not known there (nothing, or several): it is not booked.
    Unknown(String),
    /// The Basetool held the change or refused it; its reasons.
    Refused(Vec<String>),
    /// The request failed (the code of the error).
    Failed(String),
    /// The amount asked for is below what orders and missions have set aside (the amount set aside): nothing was
    /// booked; with `force` it is.
    Reserved(f64),
}

/// The stock of the unit, as far as a scan needs it.
pub trait UnitStock {
    /// Sets the amount of `item` at `place` in the stock of the unit to `amount` (`None` takes it out), whoever
    /// booked what lies there. Gives the amount that was there before (`None` where there was none). Taking what is
    /// set aside for orders is [`UnitError::Reserved`] unless `force`.
    fn set(&self, item: &ScannedItem, place: &str, amount: Option<f64>, force: bool) -> Result<Option<f64>, UnitError>;
}

/// Where the player is, by the name the stock files use; `None` while he is on the way, or the place has no name.
pub fn current_place(paths: &Paths) -> Option<String> {
    current_place_since(paths).map(|(name, _)| name)
}

/// [`current_place`] with the moment the player came there (Unix seconds, from the stamp of the log), where it is
/// known: a scan round that began before it is over.
pub fn current_place_since(paths: &Paths) -> Option<(String, Option<i64>)> {
    let place = crate::location::place(paths, None)?;
    if place.travelling {
        return None;
    }
    let name = Places::load(paths).place(place.number.as_deref()?).name().map(str::to_string)?;
    Some((name, place.since))
}

/// What a booking into the own stock came to: the amount there before and now.
#[derive(Clone, Debug, PartialEq)]
pub struct Booked {
    pub before: Option<f64>,
    pub after: f64,
}

/// Rounds an amount as the stock keeps it: pieces whole, SCU to six places.
fn rounded(amount: f64, measure: Measure) -> f64 {
    match measure {
        Measure::Pieces => stock::py_round_int(amount) as f64,
        Measure::Scu => stock::py_round(amount, 6),
    }
}

/// Sets the amount of `item` at `place` in the own stock to `amount` (`None` takes the entry out). The amount is
/// replaced, never added to what lies there. A failed write is `Err`.
pub fn set_own(paths: &Paths, item: &ScannedItem, place: &str, amount: Option<f64>) -> Result<Booked, String> {
    let _held = stock::lock();
    set_own_held(paths, item, place, amount)
}

/// [`set_own`] for a caller that holds the lock of the stock files already.
fn set_own_held(paths: &Paths, item: &ScannedItem, place: &str, amount: Option<f64>) -> Result<Booked, String> {
    debug_assert!(stock::held_here(), "the lock of the stock files is held");
    let amount = amount.map(|a| rounded(a, item.measure)).filter(|a| *a > 0.0);
    // Goods with a quality lie in the material stock, everything else in the trade stock.
    let raw = item.measure == Measure::Scu && item.quality.is_some();
    let (file, report) = if raw {
        (paths.app_file(stock::RAW_FILE), "stock_scan_book.rohstoffe")
    } else {
        (paths.app_file(stock::TRADE_FILE), "stock_scan_book.handelslager")
    };
    let Some(mut entries) = stock::load_checked(&file, report) else {
        return Err("the stock file cannot be read".into());
    };
    let pieces = item.measure == Measure::Pieces;
    let quality = item.quality.map(i64::from);
    let at = if raw {
        stock::find_raw(&entries, &item.name, quality, place)
    } else {
        stock::find_trade(&entries, &item.name, place, pieces)
    };
    let before = at.map(|i| stock::number_of(&entries[i], "menge"));
    match (at, amount) {
        (Some(i), Some(a)) => {
            stock::set_amount(&mut entries, i, a);
        }
        (Some(i), None) => {
            entries.remove(i);
        }
        (None, Some(a)) if raw => stock::push_raw(&mut entries, &item.name, a, quality, place),
        (None, Some(a)) => {
            let text = format!("{a}");
            let booked = if pieces {
                stock::trade_add_item(&mut entries, &item.name, &text, place, false, None, None)
            } else {
                stock::trade_add(&mut entries, &item.name, &text, place, false, "")
            };
            if booked.is_err() {
                return Err("the amount cannot be booked".into());
            }
        }
        (None, None) => {}
    }
    if before.is_none() && amount.is_none() || before == amount {
        return Ok(Booked { before, after: amount.unwrap_or(0.0) });
    }
    if !stock::save(&file, &entries, report) {
        return Err("the stock file cannot be written".into());
    }
    Ok(Booked { before, after: amount.unwrap_or(0.0) })
}

// ------------------------------------------------------------ the journal

/// What a scan came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum What {
    /// The amount was set.
    Set,
    /// The amount was the same already.
    Same,
    /// Another tile of the same item in the round: its amount was added.
    Added,
}

impl What {
    fn word(self) -> &'static str {
        match self {
            What::Set => "gesetzt",
            What::Same => "gleich",
            What::Added => "dazu",
        }
    }

    fn from_word(word: &str) -> What {
        match word {
            "gleich" => What::Same,
            "dazu" => What::Added,
            _ => What::Set,
        }
    }
}

/// One scan that was booked.
#[derive(Clone, Debug, PartialEq)]
pub struct Scan {
    /// Counts up, to name a scan for [`undo`].
    pub id: i64,
    /// Unix seconds.
    pub at: i64,
    pub target: Target,
    pub item: ScannedItem,
    pub place: String,
    /// The scan round it belongs to (a round is one stock at one place; its scans count up).
    pub round: i64,
    /// The amount at the place before; `None` where there was no entry.
    pub before: Option<f64>,
    /// The amount at the place after it.
    pub after: f64,
    pub what: What,
    /// The scan was taken back.
    pub undone: bool,
}

/// A reading that was not sure, waiting for the player to pick the item.
#[derive(Clone, Debug, PartialEq)]
pub struct Question {
    pub at: i64,
    pub target: Target,
    pub place: String,
    /// The amount and how it counts, the name read in `name`.
    pub item: ScannedItem,
    pub candidates: Vec<String>,
    /// The part of the screen as PNG.
    pub picture: Option<Vec<u8>>,
}

fn target_word(target: Target) -> &'static str {
    match target {
        Target::Own => "eigen",
        Target::Unit => "einheit",
    }
}

fn target_of(word: &str) -> Target {
    if word == "einheit" { Target::Unit } else { Target::Own }
}

fn item_fields(item: &ScannedItem) -> Vec<(String, Value)> {
    let mut fields = vec![
        ("name".to_string(), Value::str(&item.name)),
        ("schluessel".to_string(), Value::str(&item.key)),
        ("menge".to_string(), Value::Float(item.quantity)),
        ("stueck".to_string(), Value::Bool(item.measure == Measure::Pieces)),
    ];
    if let Some(quality) = item.quality {
        fields.push(("qualitaet".to_string(), Value::int(i64::from(quality))));
    }
    if let Some((x, y, w, h)) = item.tile_rect {
        fields.push(("kachel".to_string(), Value::Array([x, y, w, h].map(Value::int).to_vec())));
    }
    if let Some(print) = &item.tile_print {
        use base64::Engine as _;
        fields.push(("abdruck".to_string(), Value::str(base64::engine::general_purpose::STANDARD.encode(print))));
    }
    fields
}

fn text_field(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn number_of(value: &Value) -> Option<f64> {
    match value {
        Value::Int(n) => n.parse().ok(),
        Value::Float(x) if x.is_finite() => Some(*x),
        _ => None,
    }
}

fn number_field(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(number_of)
}

fn item_of(value: &Value) -> ScannedItem {
    let name = text_field(value, "name");
    let key = Some(text_field(value, "schluessel")).filter(|k| !k.is_empty()).unwrap_or_else(|| paths::name_key(&name));
    ScannedItem {
        name,
        key,
        quantity: number_field(value, "menge").unwrap_or(0.0),
        measure: if value.get("stueck") == Some(&Value::Bool(true)) { Measure::Pieces } else { Measure::Scu },
        quality: number_field(value, "qualitaet").map(|q| q as u32),
        tile_rect: match value.get("kachel") {
            Some(Value::Array(parts)) if parts.len() == 4 => {
                let n = |i: usize| number_of(&parts[i]);
                match (n(0), n(1), n(2), n(3)) {
                    (Some(x), Some(y), Some(w), Some(h)) => Some((x as i64, y as i64, w as i64, h as i64)),
                    _ => None,
                }
            }
            _ => None,
        },
        tile_print: {
            use base64::Engine as _;
            Some(text_field(value, "abdruck"))
                .filter(|b| !b.is_empty())
                .and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok())
        },
    }
}

impl Scan {
    fn to_value(&self) -> Value {
        let mut fields = vec![
            ("id".to_string(), Value::int(self.id)),
            ("zeit".to_string(), Value::int(self.at)),
            ("ziel".to_string(), Value::str(target_word(self.target))),
            ("ort".to_string(), Value::str(&self.place)),
            ("was".to_string(), Value::str(self.what.word())),
            ("zurueck".to_string(), Value::Bool(self.undone)),
            ("runde".to_string(), Value::int(self.round)),
            ("danach".to_string(), Value::Float(self.after)),
        ];
        fields.extend(item_fields(&self.item));
        if let Some(before) = self.before {
            fields.push(("vorher".to_string(), Value::Float(before)));
        }
        Value::Object(fields)
    }

    fn from_value(value: &Value) -> Option<Scan> {
        let item = item_of(value);
        if item.name.is_empty() {
            return None;
        }
        Some(Scan {
            id: number_field(value, "id")? as i64,
            at: number_field(value, "zeit").unwrap_or(0.0) as i64,
            target: target_of(&text_field(value, "ziel")),
            place: text_field(value, "ort"),
            round: number_field(value, "runde").unwrap_or(0.0) as i64,
            after: number_field(value, "danach").unwrap_or(item.quantity),
            item,
            before: number_field(value, "vorher"),
            what: What::from_word(&text_field(value, "was")),
            undone: value.get("zurueck") == Some(&Value::Bool(true)),
        })
    }
}

impl Question {
    fn to_value(&self) -> Value {
        use base64::Engine as _;
        let mut fields = vec![
            ("zeit".to_string(), Value::int(self.at)),
            ("ziel".to_string(), Value::str(target_word(self.target))),
            ("ort".to_string(), Value::str(&self.place)),
            ("kandidaten".to_string(), Value::Array(self.candidates.iter().map(Value::str).collect())),
        ];
        fields.extend(item_fields(&self.item));
        if let Some(picture) = &self.picture {
            fields.push(("bild".to_string(), Value::str(base64::engine::general_purpose::STANDARD.encode(picture))));
        }
        Value::Object(fields)
    }

    fn from_value(value: &Value) -> Question {
        use base64::Engine as _;
        let candidates = match value.get("kandidaten") {
            Some(Value::Array(names)) => names.iter().filter_map(|n| n.as_str().map(str::to_string)).collect(),
            _ => Vec::new(),
        };
        let picture = Some(text_field(value, "bild"))
            .filter(|b| !b.is_empty())
            .and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok());
        Question {
            at: number_field(value, "zeit").unwrap_or(0.0) as i64,
            target: target_of(&text_field(value, "ziel")),
            place: text_field(value, "ort"),
            item: item_of(value),
            candidates,
            picture,
        }
    }
}

/// The journal as stored: the scans oldest first, the questions oldest first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Journal {
    pub scans: Vec<Scan>,
    pub questions: Vec<Question>,
    /// Bookings into the stock of the unit that wait for the player: they would take what is set aside for orders.
    pub confirms: Vec<Confirm>,
}

/// A booking worked out: the item, where, the amount it is to stand at, whether it adds to the round, its round.
#[derive(Clone, Debug, PartialEq)]
pub struct Pending {
    pub item: ScannedItem,
    pub place: String,
    pub wanted: f64,
    pub added: bool,
    pub round: i64,
}

/// A booking that waits for the player's word because it would take what orders and missions have set aside.
#[derive(Clone, Debug, PartialEq)]
pub struct Confirm {
    pub at: i64,
    pub target: Target,
    /// The amount set aside for orders and missions at the place.
    pub reserved: f64,
    pub pending: Pending,
}

impl Confirm {
    fn to_value(&self) -> Value {
        let mut fields = vec![
            ("zeit".to_string(), Value::int(self.at)),
            ("ziel".to_string(), Value::str(target_word(self.target))),
            ("ort".to_string(), Value::str(&self.pending.place)),
            ("vorgemerkt".to_string(), Value::Float(self.reserved)),
            ("soll".to_string(), Value::Float(self.pending.wanted)),
            ("dazu".to_string(), Value::Bool(self.pending.added)),
            ("runde".to_string(), Value::int(self.pending.round)),
        ];
        fields.extend(item_fields(&self.pending.item));
        Value::Object(fields)
    }

    fn from_value(value: &Value) -> Confirm {
        Confirm {
            at: number_field(value, "zeit").unwrap_or(0.0) as i64,
            target: target_of(&text_field(value, "ziel")),
            reserved: number_field(value, "vorgemerkt").unwrap_or(0.0),
            pending: Pending {
                item: item_of(value),
                place: text_field(value, "ort"),
                wanted: number_field(value, "soll").unwrap_or(0.0),
                added: value.get("dazu") == Some(&Value::Bool(true)),
                round: number_field(value, "runde").unwrap_or(0.0) as i64,
            },
        }
    }
}

impl Journal {
    /// Reads the file; a missing or unreadable one is an empty journal.
    pub fn load(paths: &Paths) -> Journal {
        let Ok(data) = jsonio::read_json(&paths.app_file(FILE)) else { return Journal::default() };
        let list = |key: &str| match data.get(key) {
            Some(Value::Array(items)) => items.clone(),
            _ => Vec::new(),
        };
        Journal {
            scans: list("scans").iter().filter_map(Scan::from_value).collect(),
            questions: list("fragen")
                .iter()
                .map(Question::from_value)
                .filter(|q| !q.candidates.is_empty())
                .collect(),
            confirms: list("bestaetigungen").iter().map(Confirm::from_value).collect(),
        }
    }

    fn save(&mut self, paths: &Paths) -> bool {
        if self.scans.len() > MAX_SCANS {
            self.scans.drain(..self.scans.len() - MAX_SCANS);
        }
        if self.questions.len() > MAX_QUESTIONS {
            self.questions.drain(..self.questions.len() - MAX_QUESTIONS);
        }
        if self.confirms.len() > MAX_QUESTIONS {
            self.confirms.drain(..self.confirms.len() - MAX_QUESTIONS);
        }
        let data = Value::Object(vec![
            ("scans".to_string(), Value::Array(self.scans.iter().map(Scan::to_value).collect())),
            ("fragen".to_string(), Value::Array(self.questions.iter().map(Question::to_value).collect())),
            ("bestaetigungen".to_string(), Value::Array(self.confirms.iter().map(Confirm::to_value).collect())),
        ]);
        match jsonio::save_json(&paths.app_file(FILE), &data, 1, false) {
            Ok(()) => true,
            Err(error) => {
                errors::record("stock_scan_book.journal", &error);
                false
            }
        }
    }

    fn next_id(&self) -> i64 {
        self.scans.iter().map(|s| s.id).max().unwrap_or(0) + 1
    }

    /// The scans, newest first.
    pub fn newest_first(&self) -> Vec<&Scan> {
        self.scans.iter().rev().collect()
    }
}

/// Whether the two scans are of the same item at the same place in the same stock.
fn same_spot(a: &Scan, b: &Scan) -> bool {
    a.target == b.target
        && a.item.key == b.item.key
        && a.item.quality == b.item.quality
        && a.item.measure == b.item.measure
        && stock_place(&a.place) == stock_place(&b.place)
}

fn stock_place(place: &str) -> String {
    crate::stock_data::lower(py_strip(place))
}

// ------------------------------------------------------------ the scan

/// What a scan by hotkey came to, for its notice.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// Booked: the amount is set.
    Booked(Scan),
    /// The amount was the same already; nothing changed.
    Same(Scan),
    /// Another tile of the same item in the round: its amount was added.
    Added(Scan),
    /// The tile was counted already in this round: nothing changed.
    Counted(Target, String),
    /// Not sure: a question waits in the list of the scans.
    Asked { target: Target, candidates: usize },
    /// The booking would take what orders and missions have set aside (`reserved`): it waits for the player's word
    /// in the list of the scans.
    Reserved { target: Target, name: String, reserved: f64 },
    /// Nothing read.
    Nothing(Target),
    /// The game was not in front, or the pointer stood outside it.
    NoGame(Target),
    /// The place of the player is not known: nothing is booked.
    NoPlace(Target),
    /// The scan into the stock of the unit needs "Book as a logistician".
    NotLogistician,
    /// The stock of the unit cannot be written: no connection, no right.
    NoRight,
    /// The item is not known in the stock of the unit.
    UnknownInUnit(String),
    /// Booking failed.
    Failed(Target),
}

impl Outcome {
    /// The stock the scan was for.
    pub fn target(&self) -> Target {
        match self {
            Outcome::Booked(scan) | Outcome::Same(scan) | Outcome::Added(scan) => scan.target,
            Outcome::Counted(target, _)
            | Outcome::Asked { target, .. }
            | Outcome::Reserved { target, .. }
            | Outcome::Nothing(target)
            | Outcome::NoGame(target)
            | Outcome::NoPlace(target)
            | Outcome::Failed(target) => *target,
            Outcome::NotLogistician | Outcome::NoRight | Outcome::UnknownInUnit(_) => Target::Unit,
        }
    }

    /// The notice: the title is always the stock, the line below says what came of it.
    pub fn notice_in(&self, lang: Lang) -> Notice {
        let target = self.target();
        let title = language::t_in(lang, target_title_key(target));
        let say = |key: &str| language::t_in(lang, key);
        let item_line = |scan: &Scan| {
            let amount = amount_in(lang, &scan.item);
            language::tf_in(
                lang,
                "s_lsc_zeile",
                &[("0", Arg::from(amount)), ("1", Arg::from(scan.item.name.as_str())), ("2", Arg::from(scan.place.as_str()))],
            )
        };
        // The comparison with what lay there: "9 (passt)" where the scan found the amount right, "war 9, jetzt 7"
        // where it set another.
        let compared = |scan: &Scan| {
            let now = amount_in(lang, &scan.item);
            let place = Arg::from(scan.place.as_str());
            let name = Arg::from(scan.item.name.as_str());
            match (scan.what, scan.before) {
                (What::Set, Some(before)) => {
                    let mut was = scan.item.clone();
                    was.quantity = before;
                    language::tf_in(
                        lang,
                        "s_lsc_zeile_war",
                        &[("0", Arg::from(now)), ("1", name), ("2", place), ("3", Arg::from(amount_in(lang, &was)))],
                    )
                }
                (What::Same, _) => {
                    language::tf_in(lang, "s_lsc_zeile_passt", &[("0", Arg::from(now)), ("1", name), ("2", place)])
                }
                _ => item_line(scan),
            }
        };
        let (kind, detail) = match self {
            Outcome::Booked(scan) => (NoticeKind::Done, compared(scan)),
            Outcome::Same(scan) => (NoticeKind::AlreadyThere, compared(scan)),
            Outcome::Reserved { name, reserved, .. } => {
                (
                    NoticeKind::Unsure,
                    language::tf_in(
                        lang,
                        "s_lsc_reserviert",
                        &[("0", Arg::from(stock::amount_text(*reserved, true, ','))), ("1", Arg::from(name.as_str()))],
                    ),
                )
            }
            Outcome::Added(scan) => {
                let mut total = scan.item.clone();
                total.quantity = scan.after;
                (
                    NoticeKind::Done,
                    language::tf_in(
                        lang,
                        "s_lsc_zeile_dazu",
                        &[
                            ("0", Arg::from(amount_in(lang, &total))),
                            ("1", Arg::from(scan.item.name.as_str())),
                            ("2", Arg::from(scan.place.as_str())),
                        ],
                    ),
                )
            }
            Outcome::Counted(_, name) => {
                (NoticeKind::Counted, language::tf_in(lang, "s_lsc_gezaehlt", &[("0", Arg::from(name.as_str()))]))
            }
            Outcome::Asked { candidates, .. } => (
                NoticeKind::Unsure,
                language::tf_in(lang, "s_lsc_frage", &[("0", Arg::from(*candidates))]),
            ),
            Outcome::Nothing(_) => (NoticeKind::Nothing, say("s_lsc_nichts")),
            Outcome::NoGame(_) => {
                // Marked as the notice of the game away, so it goes once the game is in front again; it still names
                // the stock.
                let away = language::t_in(lang, "s_bpscan_kein_spiel");
                return Notice { away: true, ..Notice::for_target(target, NoticeKind::Nothing, title, away) };
            }
            Outcome::NoPlace(_) => (NoticeKind::Nothing, say("s_lsc_kein_ort")),
            Outcome::NotLogistician => (NoticeKind::Nothing, say("s_lsc_kein_logistiker")),
            Outcome::NoRight => (NoticeKind::Nothing, say("s_lsc_kein_recht")),
            Outcome::UnknownInUnit(name) => (
                NoticeKind::Nothing,
                language::tf_in(lang, "s_lsc_unbekannt_einheit", &[("0", Arg::from(name.as_str()))]),
            ),
            Outcome::Failed(_) => (NoticeKind::Nothing, say("s_lsc_fehler")),
        };
        Notice::for_target(target, kind, title, detail)
    }
}

/// The text key of the name of a stock in a notice.
pub fn target_title_key(target: Target) -> &'static str {
    match target {
        Target::Own => "s_lsc_ziel_eigen",
        Target::Unit => "s_lsc_ziel_einheit",
    }
}

/// `3×` for a piece count, `3,5 SCU` for goods.
pub fn amount_in(lang: Lang, item: &ScannedItem) -> String {
    let decimal = if lang == Lang::De { ',' } else { '.' };
    let pieces = item.measure == Measure::Pieces;
    if pieces {
        return format!("{}\u{d7}", stock::amount_text(item.quantity, true, decimal));
    }
    // Goods on up to three places without trailing zeros: "3,5", not "3,500" (which reads as three thousand).
    let rounded = stock::py_round(item.quantity, 3);
    let text = format!("{rounded:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{} SCU", text.replace('.', &decimal.to_string()))
}

/// Everything a scan needs besides the reading.
pub struct Run<'a> {
    pub paths: &'a Paths,
    /// The stock the key was for.
    pub target: Target,
    /// The place of the player ([`current_place`]).
    pub place: Option<String>,
    /// When the player came to the place (Unix seconds), where known: a scan round from before is over.
    pub place_since: Option<i64>,
    /// The stock of the unit; `None` where there is no connection to it.
    pub unit: Option<&'a dyn UnitStock>,
    /// Unix seconds.
    pub now: i64,
}

/// The scan round a scan of `target` at `place` belongs to, `None` where there is none open: the last scan of that
/// stock at that place is not older than [`ROUND_PAUSE`] and not from before the player came to the place.
fn open_round(journal: &Journal, run: &Run<'_>, place: &str) -> Option<i64> {
    let last = journal.scans.iter().rev().find(|s| s.target == run.target && stock_place(&s.place) == stock_place(place))?;
    let fresh = run.now - last.at <= ROUND_PAUSE;
    let here_since = run.place_since.is_none_or(|since| last.at >= since);
    (fresh && here_since).then_some(last.round)
}

/// Books a sure item into the stock of the run, at the place of the run. Nothing is booked where the target is the
/// stock of the unit without the setting for it.
///
/// The first scan of an item in a round sets the amount at the place; each further scan of the item on another tile
/// in the same round adds to it; the same tile again changes nothing. In the stock of the unit "the amount at the
/// place" is what lies there in all lots, whoever booked them.
fn book(run: &Run<'_>, item: ScannedItem, place: &str) -> Outcome {
    // The stock files first, then the saves: the sync takes them in this order.
    let _stock = stock::lock();
    let _lock = jsonio::lock_saves();
    book_held(run, item, place)
}

/// [`book`] for a caller that holds the lock of the stock files and the saves already.
fn book_held(run: &Run<'_>, item: ScannedItem, place: &str) -> Outcome {
    let target = run.target;
    let mut journal = Journal::load(run.paths);
    let round = open_round(&journal, run, place);
    let before_in_round: Vec<&Scan> = journal
        .scans
        .iter()
        .filter(|s| Some(s.round) == round && s.target == target && !s.undone && s.item.same_item(&item))
        .collect();
    if before_in_round.iter().any(|s| s.item.same_tile(&item)) {
        return Outcome::Counted(target, item.name);
    }
    // What the item stands at in this round so far; without a scan of it in the round the amount is set.
    let wanted = match before_in_round.last() {
        Some(last) => rounded(last.after + item.quantity, item.measure),
        None => rounded(item.quantity, item.measure),
    };
    let added = !before_in_round.is_empty();
    let round = round.unwrap_or_else(|| journal.scans.iter().map(|s| s.round).max().unwrap_or(0) + 1);
    settle(run, &mut journal, Pending { item, place: place.to_string(), wanted, added, round }, false)
}

/// Brings the stock to `pending.wanted` and writes the scan down. In the stock of the unit what is set aside for
/// orders is not taken without `force`: the booking waits for the player's confirmation instead.
fn settle(run: &Run<'_>, journal: &mut Journal, pending: Pending, force: bool) -> Outcome {
    let target = run.target;
    let Pending { item, place, wanted, added, round } = pending;
    let set = match target {
        Target::Own => set_own_held(run.paths, &item, &place, Some(wanted)).map_err(|error| {
            errors::record("stock_scan_book.eigen", &error);
            UnitError::Failed(error)
        }),
        Target::Unit => match run.unit {
            Some(unit) => unit.set(&item, &place, Some(wanted), force).map(|before| Booked { before, after: wanted }),
            None => Err(UnitError::NoRight),
        },
    };
    let booked = match set {
        Ok(booked) => booked,
        Err(UnitError::NoRight) => return Outcome::NoRight,
        Err(UnitError::Unknown(name)) => return Outcome::UnknownInUnit(name),
        Err(UnitError::Reserved(reserved)) => {
            let name = item.name.clone();
            let pending = Pending { item, place, wanted, added, round };
            journal.confirms.push(Confirm { at: run.now, target, reserved, pending });
            journal.save(run.paths);
            return Outcome::Reserved { target, name, reserved };
        }
        Err(UnitError::Refused(reasons)) => {
            errors::record("stock_scan_book.einheit", &reasons.join(", "));
            return Outcome::Failed(target);
        }
        Err(UnitError::Failed(_)) => return Outcome::Failed(target),
    };
    let same = !added && booked.before == Some(booked.after);
    let what = if added {
        What::Added
    } else if same {
        What::Same
    } else {
        What::Set
    };
    let scan = Scan {
        id: journal.next_id(),
        at: run.now,
        target,
        item: ScannedItem { quantity: rounded(item.quantity, item.measure), ..item },
        place,
        round,
        before: booked.before,
        after: booked.after,
        what,
        undone: false,
    };
    journal.scans.push(scan.clone());
    journal.save(run.paths);
    match what {
        What::Added => Outcome::Added(scan),
        What::Same => Outcome::Same(scan),
        What::Set => Outcome::Booked(scan),
    }
}

/// The player confirms the booking that waits at `index` (oldest first): it is made now, taking what is set aside
/// for orders. `None` where there is no such confirmation.
pub fn confirm_reserved(paths: &Paths, unit: Option<&dyn UnitStock>, index: usize, now: i64) -> Option<Outcome> {
    let _stock = stock::lock();
    let _lock = jsonio::lock_saves();
    let mut journal = Journal::load(paths);
    if index >= journal.confirms.len() {
        return None;
    }
    let waiting = journal.confirms.remove(index);
    if let Some(stopped) = check(paths, waiting.target) {
        journal.save(paths);
        return Some(stopped);
    }
    let run = Run { paths, target: waiting.target, place: Some(waiting.pending.place.clone()), place_since: None, unit, now };
    Some(settle(&run, &mut journal, waiting.pending, true))
}

/// The player lets the booking that waits at `index` go: nothing is booked.
pub fn drop_confirm(paths: &Paths, index: usize) -> bool {
    let _lock = jsonio::lock_saves();
    let mut journal = Journal::load(paths);
    if index >= journal.confirms.len() {
        return false;
    }
    journal.confirms.remove(index);
    journal.save(paths)
}

/// Does what a scan by hotkey does with `reading`: books a sure one at the place, keeps an unsure one as a question,
/// and says what came of it. The checks that stop a scan before anything is read ([`check`]) belong to the caller.
pub fn run(run: &Run<'_>, reading: Reading) -> Outcome {
    let target = run.target;
    let Some(place) = run.place.clone().filter(|p| !py_strip(p).is_empty()) else { return Outcome::NoPlace(target) };
    match reading {
        Reading::Nothing => Outcome::Nothing(target),
        Reading::Sure(item) if item.name.is_empty() || !(item.quantity.is_finite() && item.quantity > 0.0) => {
            Outcome::Nothing(target)
        }
        // Goods are told apart by their quality: without one read for sure the stack is not known, and a wrong stack
        // is a wrong booking. The player decides (the name stands as the one choice).
        Reading::Sure(item) if item.measure == Measure::Scu && item.quality.is_none() => {
            let candidates = vec![item.name.clone()];
            run_reading(run, Reading::Unsure { item, candidates, picture: None }, place)
        }
        Reading::Sure(item) => book(run, item, &place),
        other => run_reading(run, other, place),
    }
}

/// Keeps a reading that is not sure as a question.
fn run_reading(run: &Run<'_>, reading: Reading, place: String) -> Outcome {
    let target = run.target;
    match reading {
        Reading::Nothing | Reading::Sure(_) => Outcome::Nothing(target),
        Reading::Unsure { item, candidates, picture } => {
            if candidates.is_empty() {
                return Outcome::Nothing(target);
            }
            let _lock = jsonio::lock_saves();
            let mut journal = Journal::load(run.paths);
            let asked = candidates.len();
            journal.questions.push(Question { at: run.now, target, place, item, candidates, picture });
            journal.save(run.paths);
            Outcome::Asked { target, candidates: asked }
        }
    }
}

/// The checks before a key reads anything: the stock of the unit takes a scan only from a logistician. `None` where
/// the scan may go on.
pub fn check(paths: &Paths, target: Target) -> Option<Outcome> {
    (target == Target::Unit && !crate::overlay::is_logistician(paths)).then_some(Outcome::NotLogistician)
}

/// The player picks an item of the question at `index` (oldest first), or none of them (`None`). The chosen item is
/// booked as the reading would have been; the question goes only once the booking went through (or once none was
/// picked): where the booking fails, the question stays open and nothing is lost.
pub fn answer(paths: &Paths, unit: Option<&dyn UnitStock>, index: usize, chosen: Option<&str>, now: i64) -> Option<Outcome> {
    // The stock files first, then the saves: the sync takes them in this order. Both are held until the question
    // is gone, so nobody else changes the journal in between.
    let _stock = stock::lock();
    let _lock = jsonio::lock_saves();
    let question = Journal::load(paths).questions.get(index).cloned()?;
    let Some(name) = chosen else {
        drop_question(paths, index, &question);
        return None;
    };
    // The tile of the question stays with the item, so the same tile is not counted twice later.
    let named = ScannedItem::new(name, question.item.quantity, question.item.measure, question.item.quality);
    let item = ScannedItem { tile_rect: question.item.tile_rect, tile_print: question.item.tile_print.clone(), ..named };
    let target = question.target;
    let outcome = if let Some(stopped) = check(paths, target) {
        stopped
    } else if item.name.is_empty() || !(item.quantity.is_finite() && item.quantity > 0.0) {
        Outcome::Nothing(target)
    } else {
        // The player picked: the pick books, the question is not asked again (goods without a quality included).
        let context = Run { paths, target, place: Some(question.place.clone()), place_since: None, unit, now };
        book_held(&context, item, &question.place)
    };
    let failed = matches!(
        outcome,
        Outcome::Failed(_) | Outcome::NoRight | Outcome::NotLogistician | Outcome::UnknownInUnit(_)
    );
    if !failed {
        drop_question(paths, index, &question);
    }
    Some(outcome)
}

/// Takes the question at `index` out of the journal, if it is still `question`.
fn drop_question(paths: &Paths, index: usize, question: &Question) {
    let mut journal = Journal::load(paths);
    if journal.questions.get(index) == Some(question) {
        journal.questions.remove(index);
        journal.save(paths);
    }
}

/// Drops every open question; nothing is booked.
pub fn discard_questions(paths: &Paths) -> bool {
    let _lock = jsonio::lock_saves();
    let mut journal = Journal::load(paths);
    journal.questions.clear();
    journal.save(paths)
}

/// Why a scan cannot be taken back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UndoError {
    /// No such scan, or it was taken back already.
    Gone,
    /// A newer scan of the same item at the same place stands on it: that one goes first.
    Newer,
    /// The stock of the unit is not to be written now.
    NoRight,
    /// The write failed.
    Failed,
}

/// Why a scan cannot be taken back just now, `None` where it can: the scan is not gone and no newer scan of the
/// same item at the same place stands on it.
pub fn undo_blocked(journal: &Journal, id: i64) -> Option<UndoError> {
    let Some(scan) = journal.scans.iter().find(|s| s.id == id && !s.undone) else { return Some(UndoError::Gone) };
    journal
        .scans
        .iter()
        .any(|newer| newer.id > id && !newer.undone && same_spot(newer, scan))
        .then_some(UndoError::Newer)
}

/// Takes scan `id` back: the amount at the place goes back to what it was before (the entry goes where there was
/// none). In the stock of the unit that is a booking against it, so that stock must be writable.
pub fn undo(paths: &Paths, unit: Option<&dyn UnitStock>, id: i64) -> Result<(), UndoError> {
    let _stock = stock::lock();
    let _lock = jsonio::lock_saves();
    let mut journal = Journal::load(paths);
    if let Some(blocked) = undo_blocked(&journal, id) {
        return Err(blocked);
    }
    let Some(scan) = journal.scans.iter().find(|s| s.id == id).cloned() else { return Err(UndoError::Gone) };
    match scan.target {
        Target::Own => {
            set_own_held(paths, &scan.item, &scan.place, scan.before).map_err(|error| {
                errors::record("stock_scan_book.zurueck", &error);
                UndoError::Failed
            })?;
        }
        Target::Unit => {
            if !crate::overlay::is_logistician(paths) {
                return Err(UndoError::NoRight);
            }
            let unit = unit.ok_or(UndoError::NoRight)?;
            unit.set(&scan.item, &scan.place, scan.before, false).map_err(|error| match error {
                UnitError::NoRight => UndoError::NoRight,
                _ => UndoError::Failed,
            })?;
        }
    }
    if let Some(entry) = journal.scans.iter_mut().find(|s| s.id == id) {
        entry.undone = true;
    }
    if journal.save(paths) { Ok(()) } else { Err(UndoError::Failed) }
}

#[cfg(test)]
mod tests;
