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

use super::*;
use crate::jsonio;

fn value(text: &str) -> Value {
    jsonio::loads(text).expect("Beispiel")
}

#[test]
fn an_account_of_the_documentation() {
    let account = Account::parse(&value(
        r#"{"accountId": "5d0e", "accountNo": "KB-0042", "name": "Staffel Nord", "type": "ORG_UNIT",
            "orgUnit": {"id": "0c2f", "name": "Staffel Nord", "shorthand": "NORD", "kind": "SQUADRON"},
            "balance": 1250000, "balanceTarget": 2000000, "delta30d": 250000,
            "sparkline": [1000000, 1000000, 1250000], "requestable": true, "ownLimit": null, "approvalExempt": true}"#,
    ))
    .expect("account");
    assert_eq!(account.label(), "KB-0042 Staffel Nord");
    assert_eq!((account.balance, account.target, account.delta_30d), (1_250_000, Some(2_000_000), 250_000));
    assert_eq!(account.sparkline, [1_000_000, 1_000_000, 1_250_000]);
    assert!(account.requestable && account.approval_exempt);
    assert_eq!(account.own_limit, None);
    assert_eq!(account.unit, "NORD");
    // An account of a type not known yet stays; one without id is worthless.
    assert_eq!(Account::parse(&value(r#"{"accountId": "x", "type": "NEU"}"#)).map(|a| a.kind), Some("NEU".into()));
    assert!(Account::parse(&value(r#"{"accountNo": "KB-1"}"#)).is_none());
}

#[test]
fn a_request_and_what_may_still_be_done_with_it() {
    let request = Request::parse(&value(
        r#"{"id": "7f3e", "version": 1, "status": "PENDING", "createdAt": "2026-11-08T20:15:00Z",
            "decidedAt": null, "type": "WITHDRAWAL", "amount": 15000, "note": "Refuel",
            "justification": "Fuel", "account": {"id": "5d0e", "no": "KB-0042", "name": "Staffel Nord",
            "orgUnitShorthand": "NORD"}, "targetAccount": null, "requiresOwnerApproval": true,
            "requiredApprover": "RESPONSIBLE_HOLDER", "ownerApprovalGranted": false, "rejectReason": null,
            "client": "versekit"}"#,
    ))
    .expect("request");
    assert!(request.pending() && request.correctable());
    assert_eq!(request.account.as_ref().map(AccountRef::label).as_deref(), Some("KB-0042 Staffel Nord"));
    assert_eq!(request.target, None);
    assert_eq!(request.approver.as_deref(), Some("RESPONSIBLE_HOLDER"));
    let approved = Request { approved: true, ..request.clone() };
    assert!(!approved.correctable(), "approved: only cancelling is left");
    let done = Request { status: "CONFIRMED".into(), ..request };
    assert!(!done.pending());
}

#[test]
fn a_queue_entry_keeps_a_withheld_name_withheld() {
    let entry = QueueEntry::parse(&value(
        r#"{"id": "7a1b", "type": "WITHDRAWAL", "amount": 2500, "ageHours": 52, "requesterHandle": null,
            "mayConfirm": true, "link": "https://profit-base.online/bank/requests/7a1b"}"#,
    ))
    .expect("entry");
    assert_eq!(entry.requester, None);
    assert_eq!(entry.link, "https://profit-base.online/bank/requests/7a1b");
    let booking = Booking::parse(&value(
        r#"{"postingId": "9b8a", "type": "TRANSFER", "amount": -5000, "createdAt": "2026-11-08T19:30:00Z",
            "counterAccount": {"no": "KB-0001", "name": "KRT"}, "counterpartyHandle": null}"#,
    ))
    .expect("booking");
    assert_eq!(booking.counter_account.as_deref(), Some("KB-0001 KRT"));
    assert_eq!(booking.counterparty, None);
}

fn raise(kind: Kind) -> Raise {
    Raise {
        kind,
        account: "a1".into(),
        target: None,
        amount: 15_000,
        note: " Tanken ".into(),
        justification: "Freitag".into(),
    }
}

#[test]
fn a_typed_request_is_checked_and_becomes_the_body_of_the_contract() {
    assert_eq!(check(&raise(Kind::Withdrawal)), None);
    assert_eq!(check(&Raise { account: String::new(), ..raise(Kind::Deposit) }), Some("s_bank_f_konto"));
    assert_eq!(check(&Raise { amount: 0, ..raise(Kind::Deposit) }), Some("s_bank_f_betrag"));
    assert_eq!(check(&Raise { amount: AMOUNT_MAX + 1, ..raise(Kind::Deposit) }), Some("s_bank_f_betrag"));
    assert_eq!(check(&Raise { note: "x".repeat(501), ..raise(Kind::Deposit) }), Some("s_bank_f_lang"));
    assert_eq!(check(&raise(Kind::Transfer)), Some("s_bank_f_ziel"));
    assert_eq!(check(&Raise { target: Some("a1".into()), ..raise(Kind::Transfer) }), Some("s_bank_f_selbst"));
    assert_eq!(
        jsonio::dumps(&raise_body(&raise(Kind::Deposit)), None, false),
        r#"{"type": "DEPOSIT", "sourceAccountId": "a1", "amount": 15000, "note": "Tanken"}"#,
        "a deposit carries no justification"
    );
    assert_eq!(
        jsonio::dumps(&raise_body(&Raise { target: Some("a2".into()), ..raise(Kind::Transfer) }), None, false),
        r#"{"type": "TRANSFER", "sourceAccountId": "a1", "targetAccountId": "a2", "amount": 15000, "note": "Tanken", "justification": "Freitag"}"#
    );
}

#[test]
fn the_overlay_takes_over_what_the_main_window_hands() {
    let bank = Bank {
        accounts: vec![Account { id: "a1".into(), no: "KB-1".into(), name: "KRT".into(), balance: 900, ..Account::default() }],
        requests: (0..20).map(|i| Request { id: format!("r{i}"), status: "PENDING".into(), ..Request::default() }).collect(),
        open_cap: 20,
        queue: Some(Queue { open: vec![QueueEntry { id: "q".into(), ..QueueEntry::default() }], ..Queue::default() }),
        not_staff: false,
    };
    let handed = taken_over(&handover(Some(&bank), News { round: 3, count: 1 }, 1000.0));
    assert_eq!(handed.accounts, [("a1".to_string(), "KB-1 KRT".to_string(), 900)]);
    assert_eq!((handed.pending, handed.queue, handed.at), (20, Some(1), Some(1000.0)));
    assert_eq!(handed.news, News { round: 3, count: 1 });
    // Old: nothing is shown any more, the round stays.
    let old = handed.shown_at(1000.0 + KEEP + 1.0);
    assert!(old.is_empty());
    assert_eq!(old.news.round, 3);
    assert!(!handed.shown_at(1000.0 + 60.0).is_empty());
    let none = taken_over(&handover(None, News::default(), 5.0));
    assert!(none.is_empty());
    assert_eq!(none.at, Some(5.0), "an empty bank handed over is still a handing over");
    assert!(taken_over(b"kaputt").is_empty());
    assert_eq!(taken_over(b"kaputt").at, None);
}

#[test]
fn only_a_clear_refusal_settles_a_write() {
    use crate::basetool::error::ApiError;
    assert!(settled(&ApiError::new(CAP, 409)));
    assert!(settled(&ApiError::new("VERSION_CONFLICT", 409)));
    assert!(!settled(&ApiError::new("IDEMPOTENCY_IN_PROGRESS", 409)), "try again");
    assert!(!settled(&ApiError::new("RATE_LIMITED", 429)));
    assert!(!settled(&ApiError::new("UNAUTHENTICATED", 401)), "a new login, then again");
    assert!(!settled(&ApiError::network("timeout")));
    assert!(!settled(&ApiError::new("SERVICE_UNAVAILABLE", 503)));
}

#[test]
fn a_bank_error_goes_into_the_log_without_ids_and_details() {
    use crate::basetool::error::ApiError;
    let mut error = ApiError::new("NOT_FOUND", 404)
        .with_problem(vec![("detail".into(), Value::str("Konto KB-7 Geheim")), ("correlationId".into(), Value::str("c-1"))]);
    error.path = "GET /bank/accounts/5d0e9c1a/bookings".into();
    let line = error.private().describe();
    assert_eq!(line, "NOT_FOUND · HTTP 404 · GET /bank/accounts/{id}/bookings");
    let mut cancel = ApiError::new("BANK_REQUEST_NOT_PENDING", 409);
    cancel.path = "POST /bank/requests/r-9/cancel".into();
    assert_eq!(cancel.private().path, "POST /bank/requests/{id}/cancel");
}
