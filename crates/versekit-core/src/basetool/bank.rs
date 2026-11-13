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

//! The Kartellbank of the Basetool (`/bank`, rights `exchange.bank.read`, `.request`, `.queue`): the accounts the
//! member views, the own booking requests, the queue of a bank employee, and raising, correcting and cancelling an
//! own request.
//!
//! ⚠⚠ **In memory only.** Nothing of the bank goes to disk: not into the sync state, not into a file, not into the
//! error log beyond the code. What was read is thrown away after [`KEEP`] (seven days), when the area is switched
//! off or the connection ends. The feed cursor of the own requests lives here too; after a start the snapshot is
//! read anew.
//!
//! **Isolated.** A refusal of the bank stops no other area; `BANK_EXCHANGE_DISABLED` (the bank switched off for
//! connected applications) waits for the next pass.
//!
//! **No staff action.** The queue is read only; confirming, rejecting and approving happen in the Basetool, through
//! the link each entry brings.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::basetool::connection::{Connection, get, get_str, new_idempotency_key};
use crate::basetool::error::{ApiError, py_str, truthy};
use crate::basetool::feed::{self, list};
use crate::basetool::scopes::{SCOPES_BANK_QUEUE, SETTING_BANK, SETTING_BANK_QUEUE};
use crate::basetool::sync::{Area, Pass};
use crate::jsonio::{Map, Value};

pub const ACCOUNTS: &str = "/bank/accounts";
pub const REQUESTS: &str = "/bank/requests";
pub const QUEUE: &str = "/bank/queue";
/// Foreign bank data may be kept in memory for at most seven days.
pub const KEEP: f64 = 7.0 * 86_400.0;
/// The most pending own requests while the Basetool names none.
pub const OPEN_CAP: i64 = 20;
/// The bank is switched off for connected applications.
pub const DISABLED: &str = "BANK_EXCHANGE_DISABLED";
/// The member is no bank employee and has nothing to approve.
pub const NOT_STAFF: &str = "NOT_BANK_STAFF";
/// The member holds the most pending requests.
pub const CAP: &str = "BANK_OPEN_REQUEST_CAP";
/// How many bookings the history of an account shows.
pub const BOOKINGS_SHOWN: usize = 100;
/// The history of an account read once is shown again for this long; after it a click reads it anew.
pub const HISTORY_FRESH: f64 = 300.0;
/// Days of the balance curve of an account.
pub const SERIES_DAYS: i64 = 90;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn text(item: &Value, key: &str) -> String {
    match item.get(key) {
        Some(value) if truthy(value) => crate::text::py_strip(&py_str(value)).to_string(),
        _ => String::new(),
    }
}

fn optional(item: &Value, key: &str) -> Option<String> {
    Some(text(item, key)).filter(|t| !t.is_empty())
}

fn money(value: Option<&Value>) -> Option<i64> {
    match value {
        Some(Value::Int(n)) => n.parse().ok(),
        Some(Value::Float(x)) if x.is_finite() => Some(x.round() as i64),
        _ => None,
    }
}

fn flag(item: &Value, key: &str) -> bool {
    item.get(key).is_some_and(truthy)
}

/// An account in a request or a queue entry: `{id, no, name, orgUnitShorthand}`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AccountRef {
    pub id: String,
    pub no: String,
    pub name: String,
    pub shorthand: String,
}

impl AccountRef {
    fn parse(value: Option<&Value>) -> Option<AccountRef> {
        let value = value.filter(|v| matches!(v, Value::Object(_)))?;
        Some(AccountRef {
            id: text(value, "id"),
            no: text(value, "no"),
            name: text(value, "name"),
            shorthand: text(value, "orgUnitShorthand"),
        })
    }

    /// `KB-0042 Staffel Nord`.
    pub fn label(&self) -> String {
        [self.no.as_str(), self.name.as_str()].iter().filter(|p| !p.is_empty()).copied().collect::<Vec<_>>().join(" ")
    }
}

/// An account the member views.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Account {
    pub id: String,
    pub no: String,
    pub name: String,
    /// `ORG_UNIT`, `AREA`, `CARTEL`, `CARTEL_BANK`, `SPECIAL`; another stays as it came.
    pub kind: String,
    pub unit: String,
    pub balance: i64,
    pub target: Option<i64>,
    pub delta_30d: i64,
    /// The end-of-day balances of the last 30 days, oldest first.
    pub sparkline: Vec<i64>,
    /// The member may raise a withdrawal or transfer from it.
    pub requestable: bool,
    pub own_limit: Option<i64>,
    pub approval_exempt: bool,
}

impl Account {
    pub fn parse(item: &Value) -> Option<Account> {
        let id = text(item, "accountId");
        if id.is_empty() {
            return None;
        }
        let sparkline = match item.get("sparkline") {
            Some(Value::Array(points)) => points.iter().filter_map(|p| money(Some(p))).collect(),
            _ => Vec::new(),
        };
        let unit = item.get("orgUnit").map(|u| text(u, "shorthand")).unwrap_or_default();
        Some(Account {
            id,
            no: text(item, "accountNo"),
            name: text(item, "name"),
            kind: text(item, "type"),
            unit,
            balance: money(item.get("balance")).unwrap_or(0),
            target: money(item.get("balanceTarget")),
            delta_30d: money(item.get("delta30d")).unwrap_or(0),
            sparkline,
            requestable: flag(item, "requestable"),
            own_limit: money(item.get("ownLimit")),
            approval_exempt: flag(item, "approvalExempt"),
        })
    }

    pub fn label(&self) -> String {
        AccountRef { id: self.id.clone(), no: self.no.clone(), name: self.name.clone(), shorthand: self.unit.clone() }
            .label()
    }
}

/// An own booking request.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Request {
    pub id: String,
    pub version: i64,
    /// `PENDING`, `CONFIRMED`, `REJECTED`, `CANCELLED`; another stays as it came.
    pub status: String,
    pub created_at: String,
    pub decided_at: Option<String>,
    /// `DEPOSIT`, `WITHDRAWAL`, `TRANSFER`.
    pub kind: String,
    pub amount: i64,
    pub note: String,
    pub justification: String,
    pub account: Option<AccountRef>,
    pub target: Option<AccountRef>,
    pub needs_approval: bool,
    pub approver: Option<String>,
    pub approved: bool,
    pub reject_reason: Option<String>,
    /// The application that raised it; `None` for the Basetool itself.
    pub client: Option<String>,
}

impl Request {
    pub fn parse(item: &Value) -> Option<Request> {
        let id = text(item, "id");
        if id.is_empty() {
            return None;
        }
        Some(Request {
            id,
            version: money(item.get("version")).unwrap_or(0),
            status: text(item, "status"),
            created_at: text(item, "createdAt"),
            decided_at: optional(item, "decidedAt"),
            kind: text(item, "type"),
            amount: money(item.get("amount")).unwrap_or(0),
            note: text(item, "note"),
            justification: text(item, "justification"),
            account: AccountRef::parse(item.get("account")),
            target: AccountRef::parse(item.get("targetAccount")),
            needs_approval: flag(item, "requiresOwnerApproval"),
            approver: optional(item, "requiredApprover"),
            approved: flag(item, "ownerApprovalGranted"),
            reject_reason: optional(item, "rejectReason"),
            client: optional(item, "client"),
        })
    }

    pub fn pending(&self) -> bool {
        self.status == "PENDING"
    }

    /// A pending request that is not approved yet may still be corrected.
    pub fn correctable(&self) -> bool {
        self.pending() && !self.approved
    }
}

/// A request of the queue of a bank employee or approver.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct QueueEntry {
    pub id: String,
    pub kind: String,
    pub amount: i64,
    pub created_at: String,
    pub age_hours: i64,
    pub account: Option<AccountRef>,
    pub target: Option<AccountRef>,
    /// `None` when the requester withholds the name from connected applications.
    pub requester: Option<String>,
    pub justification: String,
    pub note: String,
    pub may_confirm: bool,
    /// The request in the Basetool; the only way to act on it.
    pub link: String,
}

impl QueueEntry {
    pub fn parse(item: &Value) -> Option<QueueEntry> {
        let id = text(item, "id");
        if id.is_empty() {
            return None;
        }
        Some(QueueEntry {
            id,
            kind: text(item, "type"),
            amount: money(item.get("amount")).unwrap_or(0),
            created_at: text(item, "createdAt"),
            age_hours: money(item.get("ageHours")).unwrap_or(0),
            account: AccountRef::parse(item.get("account")),
            target: AccountRef::parse(item.get("targetAccount")),
            requester: optional(item, "requesterHandle").map(crate::streamer::private),
            justification: text(item, "justification"),
            note: text(item, "note"),
            may_confirm: flag(item, "mayConfirm"),
            link: text(item, "link"),
        })
    }
}

/// The queue: what waits for the member's confirmation and for the member's approval.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Queue {
    pub open: Vec<QueueEntry>,
    pub to_approve: Vec<QueueEntry>,
    pub oldest_overdue: bool,
}

impl Queue {
    pub fn ids(&self) -> BTreeSet<String> {
        self.open.iter().chain(&self.to_approve).map(|e| e.id.clone()).collect()
    }

    pub fn len(&self) -> usize {
        self.open.len() + self.to_approve.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// What the queue part of a pass found.
#[derive(Clone, Debug, PartialEq)]
pub enum QueueState {
    /// Not asked for (switch off or right not granted).
    Off,
    /// The member is no bank employee and has nothing to approve: the queue is not shown.
    NotStaff,
    Read(Queue),
}

/// One booking of an account.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Booking {
    pub id: String,
    pub kind: String,
    pub amount: i64,
    pub created_at: String,
    pub note: String,
    pub counter_account: Option<String>,
    /// `None` when none was recorded or the member withholds the name.
    pub counterparty: Option<String>,
}

impl Booking {
    pub fn parse(item: &Value) -> Option<Booking> {
        let id = text(item, "postingId");
        if id.is_empty() {
            return None;
        }
        let counter = item.get("counterAccount").filter(|v| matches!(v, Value::Object(_))).map(|c| {
            let no = text(c, "no");
            let name = text(c, "name");
            [no, name].into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" ")
        });
        Some(Booking {
            id,
            kind: text(item, "type"),
            amount: money(item.get("amount")).unwrap_or(0),
            created_at: text(item, "createdAt"),
            note: text(item, "note"),
            counter_account: counter.filter(|c| !c.is_empty()),
            counterparty: optional(item, "counterpartyHandle").map(crate::streamer::private),
        })
    }
}

/// The history of an account: its last bookings and the end-of-day balances.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct History {
    pub bookings: Vec<Booking>,
    /// Day (`YYYY-MM-DD`) and balance, oldest first.
    pub series: Vec<(String, i64)>,
    pub target: Option<i64>,
}

/// What a pass read of the bank.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bank {
    pub accounts: Vec<Account>,
    /// The own requests, newest first.
    pub requests: Vec<Request>,
    pub open_cap: i64,
    pub queue: Option<Queue>,
    /// The member is no bank employee: the queue does not appear.
    pub not_staff: bool,
}

impl Bank {
    pub fn pending(&self) -> usize {
        self.requests.iter().filter(|r| r.pending()).count()
    }

    /// The member holds the most pending requests: no new one ("Neuer Antrag" is hidden).
    pub fn cap_reached(&self) -> bool {
        self.pending() as i64 >= self.open_cap
    }
}

#[derive(Default)]
struct Inner {
    data: Option<Arc<Bank>>,
    at: f64,
    generation: u64,
    /// The own requests by id and the feed cursor after them.
    requests: BTreeMap<String, Request>,
    cursor: Option<String>,
    open_cap: i64,
    /// The queue entries seen since the queue was first read, `None` before that (the first reading is no news).
    seen: Option<BTreeSet<String>>,
    /// The bank is switched off over there (`BANK_EXCHANGE_DISABLED`) since a pass found it so.
    disabled: bool,
    histories: BTreeMap<String, (f64, Arc<History>)>,
}

/// What lives longer than the data: the round of the queue news never goes back (the overlay counts on it), the
/// write held for its answer and the wait a `Retry-After` of a write asked for.
#[derive(Default)]
struct Lasting {
    news: u64,
    news_count: usize,
    held: Option<Held>,
    write_wait: Option<f64>,
}

/// The bank in memory, shared by the area, the page and the overlay handover.
#[derive(Default)]
pub struct BankStore {
    inner: Mutex<Inner>,
    lasting: Mutex<Lasting>,
}

/// New entries of the queue: a counter that grows with every pass that found some, and how many.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct News {
    pub round: u64,
    pub count: usize,
}

impl BankStore {
    pub fn new() -> Arc<BankStore> {
        Arc::new(BankStore::default())
    }

    /// Generation and data; data older than [`KEEP`] are gone.
    pub fn current(&self, now: f64) -> (u64, Option<Arc<Bank>>) {
        let mut inner = lock(&self.inner);
        if inner.data.is_some() && now - inner.at > KEEP {
            *inner = Inner { generation: inner.generation + 1, ..Inner::default() };
        }
        inner.histories.retain(|_, (at, _)| now - *at <= KEEP);
        (inner.generation, inner.data.clone())
    }

    pub fn generation(&self) -> u64 {
        lock(&self.inner).generation
    }

    /// Whether the last pass found the bank switched off for connected applications.
    pub fn disabled(&self) -> bool {
        lock(&self.inner).disabled
    }

    pub fn news(&self) -> News {
        let lasting = lock(&self.lasting);
        News { round: lasting.news, count: lasting.news_count }
    }

    /// Throws the bank data away: switched off, right missing, disconnected. The round of the news stays (it never
    /// goes back); a write held for its answer stays as well, so that it is never sent under a new key.
    pub fn clear(&self) {
        let mut inner = lock(&self.inner);
        if inner.data.is_some() || !inner.requests.is_empty() || !inner.histories.is_empty() || inner.disabled {
            *inner = Inner { generation: inner.generation + 1, ..Inner::default() };
        }
    }

    fn mark_disabled(&self, on: bool) {
        let mut inner = lock(&self.inner);
        if inner.disabled != on {
            inner.disabled = on;
            inner.generation += 1;
        }
    }

    /// The history of an account read last, while it is younger than [`HISTORY_FRESH`].
    pub fn history(&self, account: &str, now: f64) -> Option<Arc<History>> {
        lock(&self.inner).histories.get(account).filter(|(at, _)| now - *at <= HISTORY_FRESH).map(|(_, h)| h.clone())
    }

    /// The write that waits for its answer, if one does.
    pub fn held(&self) -> Option<Held> {
        lock(&self.lasting).held.clone()
    }

    /// Keeps a write until its answer comes (or lets it go with `None`).
    pub fn set_held(&self, held: Option<Held>) {
        lock(&self.lasting).held = held;
        lock(&self.inner).generation += 1;
    }

    /// Marks the held write with `key` as of unknown outcome (no answer, or an answer that asks to try again).
    pub fn mark_unclear(&self, key: &str) {
        let mut lasting = lock(&self.lasting);
        if let Some(held) = lasting.held.as_mut().filter(|h| h.key == key) {
            held.unclear = true;
        }
    }

    /// Lets the held write with `key` go: its answer came.
    pub fn answered(&self, key: &str) {
        let mut lasting = lock(&self.lasting);
        if lasting.held.as_ref().is_some_and(|h| h.key == key) {
            lasting.held = None;
        }
    }

    /// Until when the writes wait after a `Retry-After`, while that lies ahead.
    pub fn write_wait(&self, now: f64) -> Option<f64> {
        lock(&self.lasting).write_wait.filter(|until| *until > now)
    }

    pub fn set_write_wait(&self, until: f64) {
        lock(&self.lasting).write_wait = Some(until);
    }

    /// Reads the history of an account (bookings and the balance curve of [`SERIES_DAYS`] days) and keeps it.
    pub fn read_history(&self, conn: &Connection, account: &str, now: f64) -> Result<Arc<History>, ApiError> {
        let history = Arc::new(fetch_history(conn, account, now)?);
        let mut inner = lock(&self.inner);
        inner.histories.insert(account.to_string(), (now, history.clone()));
        inner.generation += 1;
        Ok(history)
    }

    /// One pass: the accounts, the own requests from the cursor on, the queue when `queue` is asked for.
    pub fn refresh(&self, conn: &Connection, queue: bool, now: f64) -> Result<Map, ApiError> {
        let result = self.read(conn, queue, now);
        // Switched off is said once a pass finds it, and taken back only by a pass that went through.
        match &result {
            Ok(_) => self.mark_disabled(false),
            Err(error) if error.code == DISABLED => self.mark_disabled(true),
            Err(_) => {}
        }
        result
    }

    fn read(&self, conn: &Connection, queue: bool, now: f64) -> Result<Map, ApiError> {
        let accounts: Vec<Account> =
            feed::fetch_all(conn, ACCOUNTS, None)?.items().into_iter().filter_map(Account::parse).collect();
        let (cursor, mut requests) = {
            let inner = lock(&self.inner);
            (inner.cursor.clone(), inner.requests.clone())
        };
        let (fetched, fresh) = feed::fetch_resuming(conn, REQUESTS, cursor.as_deref())?;
        if cursor.is_none() || fresh {
            requests.clear();
        }
        let mut open_cap = OPEN_CAP;
        for page in &fetched.pages {
            for stone in list(page, "removed") {
                for key in ["key", "id"] {
                    if let Some(id) = stone.get(key).and_then(Value::as_str) {
                        requests.remove(id);
                    }
                }
            }
            for request in list(page, "items").iter().filter_map(Request::parse) {
                requests.insert(request.id.clone(), request);
            }
            if let Some(cap) = money(get(page, "openRequestCap")).filter(|c| *c > 0) {
                open_cap = cap;
            }
        }
        // A queue that cannot be read now costs neither the accounts nor the own requests.
        let mut queue_error = None;
        let queue_state = if queue {
            fetch_queue(conn).unwrap_or_else(|error| {
                queue_error = Some(error.code);
                QueueState::Off
            })
        } else {
            QueueState::Off
        };
        let mut ordered: Vec<Request> = requests.values().cloned().collect();
        ordered.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| a.id.cmp(&b.id)));
        let (queue, not_staff) = match &queue_state {
            QueueState::Read(queue) => (Some(queue.clone()), false),
            QueueState::NotStaff => (None, true),
            QueueState::Off => (None, false),
        };
        let bank = Bank { accounts, requests: ordered, open_cap, queue, not_staff };
        let mut counts = vec![
            ("accounts".to_string(), Value::int(bank.accounts.len() as i64)),
            ("requests".to_string(), Value::int(bank.requests.len() as i64)),
            ("pending".to_string(), Value::int(bank.pending() as i64)),
        ];
        if let Some(code) = queue_error {
            counts.push(("queue_error".to_string(), Value::str(code)));
        }
        // A raise whose answer was lost is answered once its request stands in the feed.
        {
            let mut lasting = lock(&self.lasting);
            if let Some(held) = &lasting.held
                && held.found_in(&bank.requests)
            {
                lasting.held = None;
            }
        }
        let mut inner = lock(&self.inner);
        if let Some(queue) = &bank.queue {
            let ids = queue.ids();
            // The first reading after a start is the stand, no news.
            let new = inner.seen.as_ref().map_or(0, |seen| ids.difference(seen).count());
            if new > 0 {
                let mut lasting = lock(&self.lasting);
                lasting.news += 1;
                lasting.news_count = new;
            }
            inner.seen = Some(ids);
            counts.push(("queue".to_string(), Value::int(queue.len() as i64)));
            counts.push(("queue_new".to_string(), Value::int(new as i64)));
        }
        let changed = inner.data.as_deref() != Some(&bank);
        inner.requests = requests;
        inner.cursor = fetched.cursor;
        inner.open_cap = open_cap;
        inner.at = now;
        if changed {
            inner.data = Some(Arc::new(bank));
            inner.generation += 1;
        }
        Ok(counts)
    }
}

/// The queue to its end; `NOT_BANK_STAFF` is no error but no queue.
fn fetch_queue(conn: &Connection) -> Result<QueueState, ApiError> {
    let mut queue = Queue::default();
    let mut cursor: Option<String> = None;
    loop {
        let mut query: Vec<(&str, &str)> = vec![("limit", "500")];
        if let Some(c) = &cursor {
            query.push(("cursor", c));
        }
        let page = match conn.request("GET", QUEUE, None, &query, None) {
            Ok(page) => page,
            Err(error) if error.code == NOT_STAFF => return Ok(QueueState::NotStaff),
            Err(error) => return Err(error),
        };
        queue.open.extend(list(&page, "open").iter().filter_map(QueueEntry::parse));
        queue.to_approve.extend(list(&page, "toApprove").iter().filter_map(QueueEntry::parse));
        queue.oldest_overdue |= get(&page, "oldestOverdue").is_some_and(truthy);
        let next = get_str(&page, "nextCursor").filter(|c| !c.is_empty()).map(str::to_string);
        if !get(&page, "hasMore").is_some_and(truthy) {
            return Ok(QueueState::Read(queue));
        }
        match next {
            Some(n) if Some(&n) != cursor.as_ref() => cursor = Some(n),
            _ => {
                let mut error = ApiError::new("FEED_STUCK", 0);
                error.path = format!("GET {QUEUE}");
                return Err(error);
            }
        }
    }
}

/// `YYYY-MM-DD` of a time in seconds since 1970 (UTC).
fn day_of(seconds: f64) -> String {
    crate::export::utc_text(seconds.floor() as i64, false)[..10].to_string()
}

/// An id as one segment of a path: everything but the unreserved characters percent-encoded.
pub fn path_segment(id: &str) -> String {
    let mut out = String::new();
    for byte in id.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn fetch_history(conn: &Connection, account: &str, now: f64) -> Result<History, ApiError> {
    let limit = BOOKINGS_SHOWN.to_string();
    let account = path_segment(account);
    let path = format!("{ACCOUNTS}/{account}/bookings");
    let page = conn.request("GET", &path, None, &[("limit", &limit)], None)?;
    let bookings = list(&page, "items").iter().filter_map(Booking::parse).collect();
    let (from, to) = (day_of(now - (SERIES_DAYS - 1) as f64 * 86_400.0), day_of(now));
    let (from, to) = (format!("{from}T00:00:00Z"), format!("{to}T23:59:59Z"));
    let path = format!("{ACCOUNTS}/{account}/balance-series");
    let answer = conn.request("GET", &path, None, &[("from", &from), ("to", &to)], None)?;
    let series = list(&answer, "points")
        .iter()
        .filter_map(|p| Some((text(p, "date"), money(p.get("balance"))?)))
        .filter(|(day, _)| !day.is_empty())
        .collect();
    Ok(History { bookings, series, target: money(get(&answer, "balanceTarget")) })
}

/// The text of a refusal of a bank write: the bank's own codes, else the general text of the connection.
pub fn error_text(code: &str) -> String {
    let key = match code {
        CAP => "s_bank_f_deckel",
        "BANK_JUSTIFICATION_REQUIRED" => "s_bank_f_begruendung",
        "BANK_SELF_TRANSFER" => "s_bank_f_selbst",
        "BANK_REQUEST_NOT_PENDING" => "s_bank_f_entschieden",
        "BANK_REQUEST_ALREADY_APPROVED" => "s_bank_f_freigegeben",
        "VERSION_CONFLICT" => "s_bank_f_veraltet",
        "NOT_FOUND" => "s_bank_f_unbekannt",
        "SCOPE_MISSING" => "s_bank_f_recht",
        _ => return crate::basetool::error::error_text(code),
    };
    crate::language::t(key)
}

/// The kind of a new request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Deposit,
    Withdrawal,
    Transfer,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Deposit, Kind::Withdrawal, Kind::Transfer];

    pub fn code(self) -> &'static str {
        match self {
            Kind::Deposit => "DEPOSIT",
            Kind::Withdrawal => "WITHDRAWAL",
            Kind::Transfer => "TRANSFER",
        }
    }

    pub fn from_code(code: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.code() == code)
    }
}

/// The longest note or justification.
pub const TEXT_MAX: usize = 500;
/// The largest amount of a request.
pub const AMOUNT_MAX: i64 = 1_000_000_000_000;

/// A new request as the member typed it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Raise {
    pub kind: Kind,
    /// The target of a deposit, the source of a withdrawal or transfer.
    pub account: String,
    /// The destination of a transfer.
    pub target: Option<String>,
    pub amount: i64,
    pub note: String,
    pub justification: String,
}

/// Why a typed request cannot go out, as a text key.
pub fn check(raise: &Raise) -> Option<&'static str> {
    if raise.account.is_empty() {
        return Some("s_bank_f_konto");
    }
    if !(1..=AMOUNT_MAX).contains(&raise.amount) {
        return Some("s_bank_f_betrag");
    }
    if raise.note.chars().count() > TEXT_MAX || raise.justification.chars().count() > TEXT_MAX {
        return Some("s_bank_f_lang");
    }
    match raise.kind {
        Kind::Transfer => match &raise.target {
            None => Some("s_bank_f_ziel"),
            Some(target) if target.is_empty() => Some("s_bank_f_ziel"),
            Some(target) if *target == raise.account => Some("s_bank_f_selbst"),
            _ => None,
        },
        _ => None,
    }
}

/// The body of a raise.
pub fn raise_body(raise: &Raise) -> Value {
    // The account of every kind is `sourceAccountId`, also the one a deposit goes to.
    let mut pairs: Map = vec![
        ("type".into(), Value::str(raise.kind.code())),
        ("sourceAccountId".into(), Value::str(&raise.account)),
    ];
    if raise.kind == Kind::Transfer
        && let Some(target) = &raise.target
    {
        pairs.push(("targetAccountId".into(), Value::str(target)));
    }
    pairs.push(("amount".into(), Value::int(raise.amount)));
    let note = crate::text::py_strip(&raise.note);
    if !note.is_empty() {
        pairs.push(("note".into(), Value::str(note)));
    }
    let justification = crate::text::py_strip(&raise.justification);
    if raise.kind != Kind::Deposit && !justification.is_empty() {
        pairs.push(("justification".into(), Value::str(justification)));
    }
    Value::Object(pairs)
}

/// A write to the bank with its own `Idempotency-Key`, held until the Basetool answered it: sent again after a lost
/// answer it goes under the same key and gives the same result, never a second request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Write {
    Raise(Raise),
    Correct(Correction),
    Cancel { id: String, version: i64 },
}

/// A write held for its answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Held {
    pub write: Write,
    pub key: String,
    /// The own requests known when a raise was first sent: a new one that matches it is its request.
    pub known: BTreeSet<String>,
    /// It went out and no clear answer came back: its outcome is unknown.
    pub unclear: bool,
}

impl Held {
    /// A new write under a new key; `known` are the ids of the own requests read so far.
    pub fn new(write: Write, known: BTreeSet<String>) -> Held {
        Held { write, key: new_idempotency_key(), known, unclear: false }
    }

    /// Whether the feed shows the request a raise made: a new own request of the same kind, account and amount.
    pub fn found_in(&self, requests: &[Request]) -> bool {
        let Write::Raise(raise) = &self.write else { return false };
        requests.iter().any(|r| {
            !self.known.contains(&r.id)
                && r.kind == raise.kind.code()
                && r.amount == raise.amount
                && r.account.as_ref().is_some_and(|a| a.id == raise.account)
        })
    }
}

/// Whether an error is a clear refusal: the write did not happen and never will under this key, so it may go.
/// No answer, an answer of the server side and one that asks to try again (`IDEMPOTENCY_IN_PROGRESS`,
/// `RATE_LIMITED` …) leave the outcome open.
pub fn settled(error: &ApiError) -> bool {
    (400..500).contains(&error.status) && error.action() == crate::basetool::error::Action::Stop
}

/// What a write answered: the request's id, status and version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Written {
    pub id: String,
    pub status: String,
    pub version: i64,
}

fn written(answer: &Map) -> Written {
    let value = Value::Object(answer.clone());
    Written { id: text(&value, "id"), status: text(&value, "status"), version: money(get(answer, "version")).unwrap_or(0) }
}

/// A correction of an own pending request: every field replaces the stored one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Correction {
    pub id: String,
    pub version: i64,
    pub amount: i64,
    pub note: String,
    pub justification: String,
    pub target: Option<String>,
}

fn correction_body(correction: &Correction) -> Value {
    let mut pairs: Map = vec![
        ("amount".into(), Value::int(correction.amount)),
        ("note".into(), Value::str(crate::text::py_strip(&correction.note))),
        ("justification".into(), Value::str(crate::text::py_strip(&correction.justification))),
        ("version".into(), Value::int(correction.version)),
    ];
    if let Some(target) = &correction.target {
        pairs.push(("targetAccountId".into(), Value::str(target)));
    }
    Value::Object(pairs)
}

/// Sends a held write under its key: a raise (`POST /bank/requests`), a correction
/// (`POST /bank/requests/{id}/correction`) or a cancel (`POST /bank/requests/{id}/cancel`).
pub fn send(conn: &Connection, held: &Held) -> Result<Written, ApiError> {
    let (path, body) = match &held.write {
        Write::Raise(raise) => (REQUESTS.to_string(), raise_body(raise)),
        Write::Correct(correction) => {
            (format!("{REQUESTS}/{}/correction", path_segment(&correction.id)), correction_body(correction))
        }
        Write::Cancel { id, version } => (
            format!("{REQUESTS}/{}/cancel", path_segment(id)),
            Value::Object(vec![("version".into(), Value::int(*version))]),
        ),
    };
    Ok(written(&conn.request("POST", &path, Some(&body), &[], Some(&held.key))?))
}

// The area: runs with the bank switched on; the queue only with its own switch and right.
pub struct BankArea {
    pub store: Arc<BankStore>,
}

impl Area for BankArea {
    fn name(&self) -> &'static str {
        "bank"
    }

    fn runs(&self, ready: &[&'static str], _capabilities: &[String]) -> bool {
        ready.contains(&SETTING_BANK)
    }

    fn isolated(&self) -> bool {
        true
    }

    fn private(&self) -> bool {
        true
    }

    /// The bank switched off over there or a server in maintenance: what was read stays (at most seven days).
    fn forget_after(&self, action: crate::basetool::error::Action) -> bool {
        action != crate::basetool::error::Action::Retry
    }

    fn run(&mut self, pass: &mut Pass<'_>) -> Result<Map, ApiError> {
        let switches: &dyn crate::basetool::scopes::Switches = pass.paths;
        let queue = switches.on(SETTING_BANK_QUEUE) && pass.granted(SCOPES_BANK_QUEUE);
        self.store.refresh(pass.conn, queue, pass.now)
    }

    fn forget(&mut self) {
        self.store.clear();
    }
}

/// What the overlay shows of the bank, handed over in memory: the accounts with their balance, the pending own
/// requests, the queue and the news of the queue for its notice. The overlay only shows; requests are raised on
/// the bank page of the main window.
pub fn handover(bank: Option<&Bank>, news: News, now: f64) -> Vec<u8> {
    let Some(bank) = bank else {
        let empty = vec![("zeit".into(), Value::Float(now)), ("neu_runde".into(), Value::int(news.round as i64))];
        return crate::jsonio::dumps(&Value::Object(empty), None, false).into_bytes();
    };
    let accounts: Vec<Value> = bank
        .accounts
        .iter()
        .map(|a| {
            Value::Object(vec![
                ("id".into(), Value::str(&a.id)),
                ("name".into(), Value::str(a.label())),
                ("stand".into(), Value::int(a.balance)),
            ])
        })
        .collect();
    let mut pairs: Map = vec![
        ("konten".into(), Value::Array(accounts)),
        ("offen".into(), Value::int(bank.pending() as i64)),
        ("neu_runde".into(), Value::int(news.round as i64)),
        ("neu_anzahl".into(), Value::int(news.count as i64)),
        ("zeit".into(), Value::Float(now)),
    ];
    if let Some(queue) = &bank.queue {
        pairs.push(("warteschlange".into(), Value::int(queue.len() as i64)));
    }
    crate::jsonio::dumps(&Value::Object(pairs), None, false).into_bytes()
}

/// The bank as the overlay took it over.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Handed {
    /// Id, label and balance of each account.
    pub accounts: Vec<(String, String, i64)>,
    pub pending: i64,
    pub queue: Option<i64>,
    pub news: News,
    /// When the main window handed it over (seconds since 1970); `None` while nothing came.
    pub at: Option<f64>,
}

impl Handed {
    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty() && self.queue.is_none() && self.pending == 0
    }

    /// What may still be shown at `now`: nothing once it is older than [`KEEP`] (the main window that handed it
    /// over is long gone).
    pub fn shown_at(&self, now: f64) -> Handed {
        match self.at {
            Some(at) if now - at <= KEEP => self.clone(),
            _ => Handed { news: self.news, at: self.at, ..Handed::default() },
        }
    }
}

/// What the overlay makes of [`handover`]; anything unreadable is an empty bank.
pub fn taken_over(bytes: &[u8]) -> Handed {
    let Some(Value::Object(map)) = std::str::from_utf8(bytes).ok().and_then(|t| crate::jsonio::loads(t).ok()) else {
        return Handed::default();
    };
    let accounts = list(&map, "konten")
        .iter()
        .filter_map(|a| {
            let id = text(a, "id");
            (!id.is_empty()).then(|| (id, text(a, "name"), money(a.get("stand")).unwrap_or(0)))
        })
        .collect();
    Handed {
        accounts,
        pending: money(get(&map, "offen")).unwrap_or(0),
        queue: money(get(&map, "warteschlange")),
        news: News {
            round: money(get(&map, "neu_runde")).unwrap_or(0).max(0) as u64,
            count: money(get(&map, "neu_anzahl")).unwrap_or(0).max(0) as usize,
        },
        at: match get(&map, "zeit") {
            Some(Value::Float(x)) => Some(*x),
            Some(Value::Int(n)) => n.parse().ok(),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests;
