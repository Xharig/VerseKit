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

//! The page "Finances": the account (the balance the player entered, then every booking the Game.log shows) and
//! the evaluation of what came in and went out in aUEC in a period, by source.
//!
//! ⚠ The log has no balance and no amount for a completed contract. What it shows is booked: the sales of goods
//! at a terminal (a sale from the ship shows only its request, with the exact amount: it counts in full and is
//! marked as a request), the purchases and sales in the shops, the transfers between players. A completed
//! contract is always counted; it has an amount only where the catalog states one for its title
//! (`contract_rewards`: the amount VerseKit writes into the text of the contract). It is booked with it as income
//! and as a line of the statement; where variants of the title pay differently it is booked with the lowest amount
//! and marked (`≈`). What it costs to take a contract (its buy-in) is a spending, booked when it was taken, and
//! never income. Nothing else is guessed.
//!
//! ⚠ The balance the player enters (the count) sets the account. The difference of the balance entered to the
//! balance computed is booked as a check: it holds contract rewards, repairs, fuel, fines and sales the game
//! turned down.

use crate::contract_rewards::Reward;
use crate::jsonio::{self, Map, Value, map_set};
use crate::logtime::days_from_civil;
use crate::paths::Paths;
use crate::playtime::{merge_spans, write_beside};
use crate::stock_journal::Session;

/// A day, in seconds.
pub const DAY: i64 = 86_400;
/// The file of the counts.
pub const FILE: &str = "kassensturz.json";
/// The format of the file.
pub const FORMAT: i64 = 1;
/// The least play time (seconds) of a period for which an amount per hour is given.
pub const MIN_PLAY_SECONDS: i64 = 600;

/// The periods the page offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Period {
    /// The calendar day of now.
    Today,
    /// From the start of the newest session of the game.
    Session,
    /// The last seven days up to now.
    Week,
    /// The calendar month of now, up to now.
    Month,
    /// Days the player named.
    Custom,
}

impl Period {
    pub const ALL: [Period; 5] = [Period::Today, Period::Session, Period::Week, Period::Month, Period::Custom];

    /// The word of the setting that keeps the choice.
    pub fn word(self) -> &'static str {
        match self {
            Period::Today => "heute",
            Period::Session => "sitzung",
            Period::Week => "woche",
            Period::Month => "monat",
            Period::Custom => "frei",
        }
    }

    pub fn from_word(word: &str) -> Option<Period> {
        Period::ALL.into_iter().find(|p| p.word() == word)
    }
}

/// What the period before is: for the text that names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Before {
    Yesterday,
    PreviousSession,
    PreviousWeek,
    PreviousMonth,
    /// A period of the same length directly before.
    Same,
}

/// A stretch of time, from `from` (included) to `to` (excluded), in seconds since 1970 (UTC).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub from: i64,
    pub to: i64,
}

impl Window {
    pub fn contains(&self, moment: i64) -> bool {
        moment >= self.from && moment < self.to
    }
}

/// How far the local time of this computer is ahead of UTC at a moment, in seconds.
pub fn local_offset(moment: i64) -> i64 {
    crate::formats::local_time(moment).map_or(0, |t| {
        let local = days_from_civil(t.year, i64::from(t.month), i64::from(t.day)) * DAY
            + i64::from(t.hour) * 3600
            + i64::from(t.minute) * 60
            + i64::from(t.second);
        local - moment
    })
}

/// The calendar day (days since 1970) of a moment, by the offset of the time zone at that moment.
fn day_number(moment: i64, offset: &dyn Fn(i64) -> i64) -> i64 {
    (moment + offset(moment)).div_euclid(DAY)
}

/// The start (UTC seconds) of a local calendar day.
fn start_of_day(day: i64, offset: &dyn Fn(i64) -> i64) -> i64 {
    let guess = day * DAY - offset(day * DAY);
    day * DAY - offset(guess)
}

/// The start of the local calendar day of `moment`.
pub fn day_start(moment: i64, offset: &dyn Fn(i64) -> i64) -> i64 {
    start_of_day(day_number(moment, offset), offset)
}

/// Year, month and day of a day number (days since 1970).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// The start of the local calendar month of `moment`, or of the month `back` months before it.
pub fn month_start(moment: i64, back: i64, offset: &dyn Fn(i64) -> i64) -> i64 {
    let (year, month, _) = civil_from_days(day_number(moment, offset));
    let index = year * 12 + (month - 1) - back;
    let (year, month) = (index.div_euclid(12), index.rem_euclid(12) + 1);
    start_of_day(days_from_civil(year, month, 1), offset)
}

/// The start of the local calendar day of a date (year, month, day); `None` for a date that does not exist.
pub fn day_of_date(year: i64, month: i64, day: i64, offset: &dyn Fn(i64) -> i64) -> Option<i64> {
    let days = days_from_civil(year, month, day);
    (civil_from_days(days) == (year, month, day)).then(|| start_of_day(days, offset))
}

/// The date a text names: `2026-11-21` or `21.11.2026`; `None` for anything else.
pub fn parse_date(text: &str) -> Option<(i64, i64, i64)> {
    let text = text.trim();
    let number = |part: &str| part.trim().parse::<i64>().ok();
    let found = if let [year, month, day] = text.split('-').collect::<Vec<_>>()[..] {
        (number(year)?, number(month)?, number(day)?)
    } else if let [day, month, year] = text.trim_end_matches('.').split('.').collect::<Vec<_>>()[..] {
        (number(year)?, number(month)?, number(day)?)
    } else {
        return None;
    };
    (found.0 >= 2000 && civil_from_days(days_from_civil(found.0, found.1, found.2)) == found).then_some(found)
}

/// What a period is asked with.
#[derive(Clone, Copy, Debug)]
pub struct Ask<'a> {
    pub period: Period,
    pub now: i64,
    /// The starts of the sessions of the game, oldest first (the one running now included).
    pub sessions: &'a [i64],
    /// The days of the free period (first and last, included), as the starts of these days.
    pub custom: Option<(i64, i64)>,
}

/// The window of a period and the window before it; `None` while the period has no data to name it (no session
/// yet, no days chosen).
pub fn windows(ask: &Ask<'_>, offset: &dyn Fn(i64) -> i64) -> Option<(Window, Window, Before)> {
    let now = ask.now;
    let window = |from: i64, to: i64| Window { from, to };
    Some(match ask.period {
        Period::Today => {
            let from = day_start(now, offset);
            (window(from, now + 1), window(day_start(from - 1, offset), from), Before::Yesterday)
        }
        Period::Week => {
            let from = now + 1 - 7 * DAY;
            (window(from, now + 1), window(from - 7 * DAY, from), Before::PreviousWeek)
        }
        Period::Month => {
            let from = month_start(now, 0, offset);
            (window(from, now + 1), window(month_start(now, 1, offset), from), Before::PreviousMonth)
        }
        Period::Session => {
            let mut starts: Vec<i64> = ask.sessions.iter().copied().filter(|s| *s <= now).collect();
            starts.sort_unstable();
            let from = *starts.last()?;
            let before = match starts.len().checked_sub(2).map(|i| starts[i]) {
                Some(earlier) => window(earlier, from),
                None => window(from, from),
            };
            (window(from, now + 1), before, Before::PreviousSession)
        }
        Period::Custom => {
            let (first, last) = ask.custom.filter(|(first, last)| first <= last)?;
            let to = day_start(last + DAY / 2 * 3, offset);
            let len = to - first;
            (window(first, to), window(first - len, first), Before::Same)
        }
    })
}

/// A sum of bookings; the ones the log shows only as a request are in it, and named apart.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Flow {
    /// Everything the log shows, the requests included.
    pub sum: f64,
    /// How many of these.
    pub count: usize,
    /// Of the sum, what the log shows only as a request (a sale from the ship has no answer in the log).
    pub requested: f64,
    /// How many of these.
    pub requested_count: usize,
}

impl Flow {
    fn add(&mut self, amount: f64, proven: bool) {
        self.sum += amount;
        self.count += 1;
        if !proven {
            self.requested += amount;
            self.requested_count += 1;
        }
    }
}

/// What a window holds, by source.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Totals {
    /// Goods sold at a commodity terminal.
    pub trade: Flow,
    /// Items sold in a shop.
    pub shop_sales: Flow,
    /// Items bought in a shop.
    pub shop_purchases: Flow,
    /// Transfers received from other players.
    pub transfers_in: Flow,
    /// Transfers sent to other players.
    pub transfers_out: Flow,
    /// Contracts completed.
    pub contracts: usize,
    /// Of them, those whose catalog entry states a reward, and what these add up to.
    pub contracts_priced: usize,
    pub contracts_reward: f64,
    /// Of these, those that stand only for "at least" (variants that pay differently), and their sum.
    pub contracts_approx: usize,
    pub contracts_approx_reward: f64,
    /// Contracts that cost something to take (their buy-in), taken in the window, and the sum: spending, never income.
    pub contracts_cost_count: usize,
    pub contracts_cost: f64,
    /// Of that sum, what is only "at least" (variants with another buy-in).
    pub contracts_cost_approx: f64,
}

impl Totals {
    /// What was earned: goods and items sold, contracts that state their reward. Transfers are no earnings.
    pub fn income(&self) -> f64 {
        self.trade.sum + self.shop_sales.sum + self.contracts_reward
    }

    /// Of the income, what the log shows only as a request.
    pub fn income_requested(&self) -> f64 {
        self.trade.requested + self.shop_sales.requested
    }

    /// Of the income, what is only a "at least" of a contract.
    pub fn income_approx(&self) -> f64 {
        self.contracts_approx_reward
    }

    /// What was spent: bought in the shops, paid to take contracts.
    pub fn spent(&self) -> f64 {
        self.shop_purchases.sum + self.contracts_cost
    }

    /// Income less spending.
    pub fn result(&self) -> f64 {
        self.income() - self.spent()
    }

    /// Everything the log shows going into the account less everything going out, transfers included.
    pub fn movement(&self) -> f64 {
        self.result() + self.transfers_in.sum - self.transfers_out.sum
    }

    /// Does the window hold anything at all?
    pub fn is_empty(&self) -> bool {
        *self == Totals::default()
    }
}

/// A contract completed: when, its displayed title and what the catalog states it pays, if anything.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Contract {
    /// Seconds since 1970 (UTC).
    pub time: i64,
    pub name: String,
    pub reward: Option<Reward>,
    /// When it was taken (seconds since 1970, UTC); 0 where the log does not say. What it cost to take it is paid then.
    pub taken: i64,
    /// What it cost to take it (its buy-in), if the terms state one.
    pub cost: Option<Reward>,
}

impl Contract {
    /// The moment the cost was paid: when it was taken, else when it ended.
    fn paid_at(&self) -> i64 {
        if self.taken > 0 { self.taken } else { self.time }
    }
}

/// The contracts completed.
pub type Contracts<'a> = &'a [Contract];

/// What the sessions of the journal and the completed contracts hold in a window.
pub fn totals(sessions: &[Session], contracts: Contracts<'_>, window: Window) -> Totals {
    let mut out = Totals::default();
    for session in sessions {
        for deal in session.sales.iter().filter(|deal| window.contains(deal.time)) {
            let flow = if deal.goods() { &mut out.trade } else { &mut out.shop_sales };
            flow.add(deal.price_total, deal.proof.confirmed());
        }
        for deal in session.purchases.iter().filter(|deal| window.contains(deal.time)) {
            out.shop_purchases.add(deal.price_total, deal.proof.confirmed());
        }
        for transfer in session.transfers.iter().filter(|t| window.contains(t.time)) {
            let flow = if transfer.incoming { &mut out.transfers_in } else { &mut out.transfers_out };
            flow.add(transfer.amount as f64, true);
        }
    }
    for contract in contracts.iter().filter(|contract| window.contains(contract.time)) {
        out.contracts += 1;
        if let Some(reward) = contract.reward {
            out.contracts_priced += 1;
            out.contracts_reward += reward.amount as f64;
            if reward.approx {
                out.contracts_approx += 1;
                out.contracts_approx_reward += reward.amount as f64;
            }
        }
    }
    for cost in contracts.iter().filter(|c| window.contains(c.paid_at())).filter_map(|c| c.cost) {
        out.contracts_cost_count += 1;
        out.contracts_cost += cost.amount as f64;
        if cost.approx {
            out.contracts_cost_approx += cost.amount as f64;
        }
    }
    out
}

/// The seconds of play in a window: the sessions (`from`, `to`) with overlaps melted, clipped to the window.
pub fn play_seconds(spans: &[(i64, i64)], window: Window) -> i64 {
    merge_spans(spans).into_iter().map(|(from, to)| (to.min(window.to) - from.max(window.from)).max(0)).sum()
}

/// aUEC per hour of play; `None` below [`MIN_PLAY_SECONDS`] of play, where the quotient says nothing.
pub fn per_hour(amount: f64, play_seconds: i64) -> Option<f64> {
    (play_seconds >= MIN_PLAY_SECONDS).then(|| amount / (play_seconds as f64 / 3600.0))
}

// ---------------------------------------------------------------------------
// The count of the balance
// ---------------------------------------------------------------------------

/// A balance the player entered.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Count {
    /// Seconds since 1970 (UTC) of the entry.
    pub time: i64,
    /// The balance, aUEC.
    pub balance: i64,
    /// What the log shows going in and out since the count before ([`Totals::movement`]); `None` for the first.
    pub logged: Option<f64>,
    /// What the balance moved beyond what the log shows: income (or loss) the log does not see; `None` for the
    /// first count.
    pub unseen: Option<f64>,
    /// Of the time between, what the log shows only as a request.
    pub requested: f64,
}

/// A new count: the balance `balance` entered at `now`, against `before` and the journal.
pub fn count(before: Option<&Count>, balance: i64, now: i64, sessions: &[Session]) -> Count {
    let Some(before) = before else {
        return Count { time: now, balance, logged: None, unseen: None, requested: 0.0 };
    };
    let held = totals(sessions, &[], Window { from: before.time + 1, to: now + 1 });
    let logged = held.movement();
    Count {
        time: now,
        balance,
        logged: Some(logged),
        unseen: Some((balance - before.balance) as f64 - logged),
        requested: held.income_requested(),
    }
}

impl Count {
    pub fn to_value(&self) -> Value {
        let mut fields = vec![("zeit".into(), Value::int(self.time)), ("stand".into(), Value::int(self.balance))];
        if let Some(logged) = self.logged {
            fields.push(("log".into(), Value::Float(logged)));
        }
        if let Some(unseen) = self.unseen {
            fields.push(("unsichtbar".into(), Value::Float(unseen)));
        }
        fields.push(("angefragt".into(), Value::Float(self.requested)));
        Value::Object(fields)
    }

    pub fn from_value(value: &Value) -> Option<Count> {
        let whole = |key: &str| match value.get(key)? {
            Value::Int(text) => text.parse::<i64>().ok(),
            _ => None,
        };
        let float = |key: &str| match value.get(key)? {
            Value::Float(n) => Some(*n),
            Value::Int(text) => text.parse::<f64>().ok(),
            _ => None,
        };
        Some(Count {
            time: whole("zeit")?,
            balance: whole("stand")?,
            logged: float("log"),
            unseen: float("unsichtbar"),
            requested: float("angefragt").unwrap_or(0.0),
        })
    }
}

/// The counts of the file, oldest first.
pub fn counts(paths: &Paths) -> Vec<Count> {
    let valid = |d: &Value| matches!(d, Value::Object(_));
    let Some(Value::Object(map)) = jsonio::load_json(&paths.app_file(FILE), Some(&valid), "finance.counts") else {
        return Vec::new();
    };
    let Some(Value::Array(list)) = map.iter().find(|(key, _)| key == "eintraege").map(|(_, v)| v) else {
        return Vec::new();
    };
    let mut found: Vec<Count> = list.iter().filter_map(Count::from_value).collect();
    found.sort_by_key(|c| c.time);
    found
}

/// Writes the counts; false when the file could not be written.
pub fn save_counts(paths: &Paths, counts: &[Count]) -> bool {
    let mut data: Map = Vec::new();
    map_set(&mut data, "eintraege", Value::Array(counts.iter().map(Count::to_value).collect()));
    write_beside(paths, FILE, &mut data, FORMAT, "finance.save_counts")
}

/// The counts without the newest one: a balance typed wrong is taken back.
pub fn withdraw_last(counts: &[Count]) -> Vec<Count> {
    counts.split_last().map(|(_, rest)| rest.to_vec()).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// The account
// ---------------------------------------------------------------------------

/// What a line of the statement is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Booking {
    /// The first balance the player entered.
    Opening,
    /// Goods sold at a commodity terminal.
    GoodsSold,
    /// An item sold in a shop.
    ItemSold,
    /// An item bought in a shop.
    ItemBought,
    TransferIn,
    TransferOut,
    /// A contract completed that states a reward.
    ContractDone,
    /// What it cost to take a contract (its buy-in): a spending.
    ContractCost,
    /// A balance the player entered later: the difference to the computed balance.
    Check,
}

/// One line of the statement.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// Seconds since 1970 (UTC).
    pub time: i64,
    pub kind: Booking,
    /// The booking with its sign (for an opening, the balance; for a check, the difference).
    pub amount: f64,
    /// The balance after the booking.
    pub balance: f64,
    /// Pieces; for goods SCU; 0 for what has no count.
    pub quantity: u32,
    /// The name of the good or the item; `None` where nothing names it.
    pub what: Option<String>,
    /// The name of the place; `None` where nothing names it.
    pub place: Option<String>,
    /// The log shows only the request of this sale.
    pub requested: bool,
    /// The amount is the lowest a contract of this title pays.
    pub approx: bool,
}

/// The statement of the account, oldest line first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Account {
    pub entries: Vec<Entry>,
}

impl Account {
    /// The balance now; `None` while the player has entered none.
    pub fn balance(&self) -> Option<f64> {
        self.entries.last().map(|entry| entry.balance)
    }
}

/// The names the statement is given: of a good or an item by `(class, guid)`, of a place by its number.
pub struct Namer<'a> {
    pub item: &'a dyn Fn(&str, &str) -> Option<String>,
    pub place: &'a dyn Fn(&str) -> Option<String>,
}

/// The account: the first count is the opening balance, every booking of the journal after it moves the balance,
/// every later count sets it and books the difference as a check.
///
/// ⚠ A booking at the very moment of a count belongs to the balance of that count; a booking after it is new.
pub fn account(counts: &[Count], sessions: &[Session], namer: &Namer<'_>) -> Account {
    account_with(counts, sessions, &[], namer)
}

/// [`account`] with the contracts completed: every one that states a reward is a booking of its own.
pub fn account_with(counts: &[Count], sessions: &[Session], contracts: Contracts<'_>, namer: &Namer<'_>) -> Account {
    let mut counts = counts.to_vec();
    counts.sort_by_key(|c| c.time);
    let Some((first, later)) = counts.split_first() else { return Account::default() };
    let mut lines: Vec<Entry> = Vec::new();
    let line = |time: i64, kind: Booking, amount: f64| Entry {
        time,
        kind,
        amount,
        balance: 0.0,
        quantity: 0,
        what: None,
        place: None,
        requested: false,
        approx: false,
    };
    for contract in contracts.iter().filter(|c| c.time > first.time) {
        let Some(reward) = contract.reward else { continue };
        lines.push(Entry {
            what: Some(contract.name.clone()).filter(|name| !name.is_empty()),
            approx: reward.approx,
            ..line(contract.time, Booking::ContractDone, reward.amount as f64)
        });
    }
    for contract in contracts.iter().filter(|c| c.paid_at() > first.time) {
        let Some(cost) = contract.cost else { continue };
        lines.push(Entry {
            what: Some(contract.name.clone()).filter(|name| !name.is_empty()),
            approx: cost.approx,
            ..line(contract.paid_at(), Booking::ContractCost, -(cost.amount as f64))
        });
    }
    for session in sessions {
        for deal in session.sales.iter().filter(|d| d.time > first.time) {
            let (kind, what) = if deal.goods() {
                (Booking::GoodsSold, (namer.item)("", &deal.guid))
            } else {
                (Booking::ItemSold, (namer.item)(&deal.class, &deal.guid))
            };
            lines.push(Entry {
                quantity: deal.quantity,
                what: what.or_else(|| Some(deal.class.clone()).filter(|c| !c.is_empty())),
                place: deal.place.as_deref().and_then(|number| (namer.place)(number)),
                requested: !deal.proof.confirmed(),
                ..line(deal.time, kind, deal.price_total)
            });
        }
        for deal in session.purchases.iter().filter(|d| d.time > first.time) {
            lines.push(Entry {
                quantity: deal.quantity,
                what: (namer.item)(&deal.class, &deal.guid).or_else(|| Some(deal.class.clone()).filter(|c| !c.is_empty())),
                place: deal.place.as_deref().and_then(|number| (namer.place)(number)),
                ..line(deal.time, Booking::ItemBought, -deal.price_total)
            });
        }
        for transfer in session.transfers.iter().filter(|t| t.time > first.time) {
            let (kind, sign) = if transfer.incoming { (Booking::TransferIn, 1.0) } else { (Booking::TransferOut, -1.0) };
            lines.push(line(transfer.time, kind, sign * transfer.amount as f64));
        }
    }
    // Stable: what the journal lists first stays first at one moment.
    lines.sort_by_key(|entry| entry.time);
    let mut entries = vec![Entry { balance: first.balance as f64, ..line(first.time, Booking::Opening, first.balance as f64) }];
    let mut running = first.balance as f64;
    let mut later = later.iter().peekable();
    let check = |entries: &mut Vec<Entry>, running: &mut f64, count: &Count| {
        let difference = count.balance as f64 - *running;
        *running = count.balance as f64;
        entries.push(Entry { balance: *running, ..line(count.time, Booking::Check, difference) });
    };
    for mut entry in lines {
        while let Some(count) = later.next_if(|count| count.time < entry.time) {
            check(&mut entries, &mut running, count);
        }
        running += entry.amount;
        entry.balance = running;
        entries.push(entry);
    }
    for count in later {
        check(&mut entries, &mut running, count);
    }
    Account { entries }
}

#[cfg(test)]
mod tests;
