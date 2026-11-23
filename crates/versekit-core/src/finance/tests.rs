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
use crate::stock_journal::{Deal, Proof, Transfer};

fn utc(_: i64) -> i64 {
    0
}

/// A zone one hour ahead of UTC all year.
fn ahead(_: i64) -> i64 {
    3600
}

/// 2026-11-21 12:00 UTC.
const NOW: i64 = 1_795_262_400;

fn at(year: i64, month: i64, day: i64) -> i64 {
    day_of_date(year, month, day, &utc).expect("date")
}

fn deal(time: i64, price: f64, proof: Proof, goods: bool) -> Deal {
    Deal {
        time,
        class: if goods { String::new() } else { "x".into() },
        guid: "g".into(),
        quantity: 1,
        price_total: price,
        place: None,
        shop: "SCShop".into(),
        proof: if goods { proof } else { Proof::Answer },
    }
}

fn session(start: i64) -> Session {
    Session { start, purchases: Vec::new(), sales: Vec::new(), movements: Vec::new(), transfers: Vec::new() }
}

fn ask(period: Period) -> Ask<'static> {
    Ask { period, now: NOW, sessions: &[], custom: None }
}

#[test]
fn the_date_of_the_test_is_the_one_it_says() {
    assert_eq!(at(2026, 11, 21) + 12 * 3600, NOW);
}

#[test]
fn the_local_offset_is_a_zone_of_the_world() {
    for moment in [NOW, NOW - 120 * DAY] {
        let offset = local_offset(moment);
        assert!((-12 * 3600..=14 * 3600).contains(&offset), "{offset}");
        assert_eq!(offset % 900, 0, "zones go by a quarter hour");
    }
}

#[test]
fn today_is_the_local_day_and_before_it_is_yesterday() {
    let (now, before, kind) = windows(&ask(Period::Today), &utc).expect("window");
    assert_eq!(now, Window { from: at(2026, 11, 21), to: NOW + 1 });
    assert_eq!(before, Window { from: at(2026, 11, 20), to: at(2026, 11, 21) });
    assert_eq!(kind, Before::Yesterday);
    // One hour ahead of UTC: the local day began an hour earlier in UTC.
    let (now, before, _) = windows(&ask(Period::Today), &ahead).expect("window");
    assert_eq!(now.from, at(2026, 11, 21) - 3600);
    assert_eq!(before.from, at(2026, 11, 20) - 3600);
    // Half past midnight local time is still the day that began a half hour ago.
    let late = Ask { now: at(2026, 11, 21) - 3600 + 1800, ..ask(Period::Today) };
    assert_eq!(windows(&late, &ahead).expect("window").0.from, at(2026, 11, 21) - 3600);
}

#[test]
fn the_week_is_seven_days_up_to_now_and_before_it_the_seven_before() {
    let (now, before, kind) = windows(&ask(Period::Week), &utc).expect("window");
    assert_eq!((now.to - now.from, before.to - before.from), (7 * DAY, 7 * DAY));
    assert_eq!((now.to, before.to), (NOW + 1, now.from));
    assert_eq!(kind, Before::PreviousWeek);
    assert!(now.contains(NOW) && !now.contains(NOW - 7 * DAY) && now.contains(NOW - 7 * DAY + 1));
}

#[test]
fn the_month_is_the_calendar_month_and_before_it_the_one_before_even_over_a_new_year() {
    let (now, before, kind) = windows(&ask(Period::Month), &utc).expect("window");
    assert_eq!(now, Window { from: at(2026, 11, 1), to: NOW + 1 });
    assert_eq!(before, Window { from: at(2026, 10, 1), to: at(2026, 11, 1) });
    assert_eq!(kind, Before::PreviousMonth);
    let january = Ask { now: at(2027, 1, 15), ..ask(Period::Month) };
    let (now, before, _) = windows(&january, &utc).expect("window");
    assert_eq!((now.from, before.from, before.to), (at(2027, 1, 1), at(2026, 12, 1), at(2027, 1, 1)));
    // February of a leap year has 29 days: the month before March reaches back to the first of February.
    let march = Ask { now: at(2028, 3, 10), ..ask(Period::Month) };
    let (_, before, _) = windows(&march, &utc).expect("window");
    assert_eq!((before.to - before.from) / DAY, 29);
}

#[test]
fn the_session_is_the_newest_started_and_before_it_the_one_before() {
    let starts = [at(2026, 11, 18), at(2026, 11, 20), at(2026, 11, 21) + 3600, NOW + 5000];
    let asked = Ask { sessions: &starts, ..ask(Period::Session) };
    let (now, before, kind) = windows(&asked, &utc).expect("window");
    // The start after now is no session yet.
    assert_eq!(now, Window { from: starts[2], to: NOW + 1 });
    assert_eq!(before, Window { from: starts[1], to: starts[2] });
    assert_eq!(kind, Before::PreviousSession);
    let one = Ask { sessions: &starts[..1], ..ask(Period::Session) };
    let (_, before, _) = windows(&one, &utc).expect("window");
    assert_eq!(before.from, before.to, "no session before: an empty window");
    assert!(windows(&ask(Period::Session), &utc).is_none(), "no session at all");
}

#[test]
fn a_free_period_takes_whole_days_and_before_it_the_same_length() {
    let custom = Ask { custom: Some((at(2026, 11, 10), at(2026, 11, 12))), ..ask(Period::Custom) };
    let (now, before, kind) = windows(&custom, &utc).expect("window");
    assert_eq!(now, Window { from: at(2026, 11, 10), to: at(2026, 11, 13) });
    assert_eq!(before, Window { from: at(2026, 11, 7), to: at(2026, 11, 10) });
    assert_eq!(kind, Before::Same);
    let backwards = Ask { custom: Some((at(2026, 11, 12), at(2026, 11, 10))), ..ask(Period::Custom) };
    assert!(windows(&backwards, &utc).is_none());
    assert!(windows(&ask(Period::Custom), &utc).is_none());
}

#[test]
fn a_date_is_read_as_iso_or_as_german_and_a_wrong_one_is_none() {
    assert_eq!(parse_date("2026-11-21"), Some((2026, 11, 21)));
    assert_eq!(parse_date(" 21.11.2026 "), Some((2026, 11, 21)));
    assert_eq!(parse_date("21.11.2026."), Some((2026, 11, 21)));
    for wrong in ["", "2026-02-30", "31.04.2026", "2026-13-01", "morgen", "1.1.1999", "2026-11"] {
        assert_eq!(parse_date(wrong), None, "{wrong}");
    }
    assert_eq!(day_of_date(2026, 2, 29, &utc), None);
}

#[test]
fn the_sources_are_kept_apart_and_a_request_is_never_added_to_the_proven_sum() {
    let mut one = session(NOW - 3600);
    one.sales = vec![
        deal(NOW - 100, 1000.0, Proof::BoxGone, true),
        deal(NOW - 90, 500.0, Proof::Requested, true),
        deal(NOW - 80, 200.0, Proof::Answer, false),
    ];
    one.purchases = vec![deal(NOW - 70, 300.0, Proof::Answer, false)];
    one.transfers = vec![
        Transfer { time: NOW - 60, incoming: true, amount: 40_000 },
        Transfer { time: NOW - 50, incoming: false, amount: 15_000 },
    ];
    let window = Window { from: NOW - 3600, to: NOW + 1 };
    let held = totals(&[one], &[done(NOW - 10, Some(exact(25_000))), done(NOW - 9, None)], window);
    // The sale from the ship (a request, with its exact amount) counts in full and is named apart.
    assert_eq!(
        (held.trade.sum, held.trade.count, held.trade.requested, held.trade.requested_count),
        (1500.0, 2, 500.0, 1)
    );
    assert_eq!((held.shop_sales.sum, held.shop_sales.requested), (200.0, 0.0));
    assert_eq!(held.shop_purchases.sum, 300.0);
    assert_eq!((held.transfers_in.sum, held.transfers_out.sum), (40_000.0, 15_000.0));
    // The contract with a reward is income; the other one has no amount, not even a guessed one.
    assert_eq!(held.income(), 1700.0 + 25_000.0);
    assert_eq!(held.income_requested(), 500.0);
    assert_eq!(held.income_approx(), 0.0);
    assert_eq!(held.result(), 1400.0 + 25_000.0);
    // Transfers are no earnings but move the account.
    assert_eq!(held.movement(), 1400.0 + 25_000.0 + 40_000.0 - 15_000.0);
    assert_eq!((held.contracts, held.contracts_priced, held.contracts_reward), (2, 1, 25_000.0));
}

fn exact(amount: i64) -> Reward {
    Reward { amount, approx: false }
}

fn done(time: i64, reward: Option<Reward>) -> Contract {
    Contract { time, name: "Hunt the Wolf".into(), reward, ..Contract::default() }
}

/// A contract with a buy-in and no reward, taken at `taken` and done at `time`.
fn costly(taken: i64, time: i64, cost: Reward) -> Contract {
    Contract { time, taken, name: "Goods for the Getting".into(), cost: Some(cost), ..Contract::default() }
}

#[test]
fn what_it_costs_to_take_a_contract_is_spending_and_never_income() {
    let contracts = [
        costly(10, 90, exact(33_000)),
        costly(20, 95, Reward { amount: 250, approx: true }),
        done(30, Some(exact(5_000))),
    ];
    let held = totals(&[], &contracts, Window { from: 0, to: 100 });
    assert_eq!((held.contracts_cost_count, held.contracts_cost, held.contracts_cost_approx), (2, 33_250.0, 250.0));
    assert_eq!(held.income(), 5_000.0, "a cost is no income");
    assert_eq!(held.spent(), 33_250.0);
    assert_eq!(held.result(), 5_000.0 - 33_250.0);
    // It is paid when it was taken, not when it ended.
    let early = totals(&[], &contracts, Window { from: 0, to: 15 });
    assert_eq!(early.contracts_cost, 33_000.0);
    let late = totals(&[], &contracts, Window { from: 50, to: 100 });
    assert_eq!(late.contracts_cost, 0.0, "taken before the window");
}

#[test]
fn a_cost_is_a_negative_line_of_the_statement_with_the_balance_after_it() {
    let contracts = [costly(150, 400, exact(33_000)), costly(50, 60, exact(1_000))];
    let account = account_with(&[opening(100_000, 100)], &[], &contracts, &namer());
    assert_eq!(
        amounts(&account),
        vec![(Booking::Opening, 100_000.0, 100_000.0), (Booking::ContractCost, -33_000.0, 67_000.0)],
        "before the opening count nothing is booked"
    );
    assert_eq!(account.entries[1].what.as_deref(), Some("Goods for the Getting"));
    assert_eq!(account.entries[1].time, 150, "booked when it was taken");
}

#[test]
fn a_contract_with_the_lowest_of_its_variants_is_income_and_named_approximate() {
    let both = [done(10, Some(exact(70_000))), done(20, Some(Reward { amount: 30_000, approx: true })), done(30, None)];
    let held = totals(&[], &both, Window { from: 0, to: 100 });
    assert_eq!((held.contracts, held.contracts_priced, held.contracts_approx), (3, 2, 1));
    assert_eq!(held.income(), 100_000.0);
    assert_eq!(held.income_approx(), 30_000.0);
    assert!(!held.is_empty());
}

#[test]
fn a_contract_with_a_reward_is_a_line_of_the_statement_with_its_name_and_the_balance_after_it() {
    let mut one = session(0);
    one.sales = vec![goods(200, 1000.0, Proof::BoxGone)];
    let contracts = [
        done(150, Some(exact(25_000))),
        Contract {
            time: 250,
            name: "Escort the Convoy".into(),
            reward: Some(Reward { amount: 82_750, approx: true }),
            ..Contract::default()
        },
        done(300, None),
        done(50, Some(exact(9_999))),
    ];
    let account = account_with(&[opening(10_000, 100)], &[one], &contracts, &namer());
    assert_eq!(
        amounts(&account),
        vec![
            (Booking::Opening, 10_000.0, 10_000.0),
            (Booking::ContractDone, 25_000.0, 35_000.0),
            (Booking::GoodsSold, 1000.0, 36_000.0),
            (Booking::ContractDone, 82_750.0, 118_750.0),
        ],
        "before the opening count and without a reward there is no line"
    );
    let line = &account.entries[3];
    assert_eq!((line.time, line.what.as_deref(), line.approx), (250, Some("Escort the Convoy"), true));
    assert!(!account.entries[1].approx);
    assert_eq!(account.entries[1].what.as_deref(), Some("Hunt the Wolf"));
}

#[test]
fn a_window_includes_its_start_and_excludes_its_end() {
    let mut one = session(0);
    one.transfers = vec![
        Transfer { time: 100, incoming: true, amount: 1 },
        Transfer { time: 199, incoming: true, amount: 10 },
        Transfer { time: 200, incoming: true, amount: 100 },
        Transfer { time: 99, incoming: true, amount: 1000 },
    ];
    let held = totals(&[one], &[done(100, None), done(200, None)], Window { from: 100, to: 200 });
    assert_eq!(held.transfers_in.sum, 11.0);
    assert_eq!(held.contracts, 1);
    assert!(totals(&[], &[], Window { from: 0, to: 1 }).is_empty());
}

#[test]
fn the_play_time_is_clipped_to_the_window_and_overlaps_count_once() {
    let spans = [(100, 400), (300, 500), (900, 1000)];
    assert_eq!(play_seconds(&spans, Window { from: 0, to: 2000 }), 400 + 100);
    assert_eq!(play_seconds(&spans, Window { from: 350, to: 950 }), 150 + 50);
    assert_eq!(play_seconds(&spans, Window { from: 2000, to: 3000 }), 0);
}

#[test]
fn an_amount_per_hour_needs_enough_play_time() {
    assert_eq!(per_hour(120_000.0, 3600), Some(120_000.0));
    assert_eq!(per_hour(60_000.0, 1800), Some(120_000.0));
    assert_eq!(per_hour(60_000.0, MIN_PLAY_SECONDS - 1), None);
    assert_eq!(per_hour(0.0, 7200), Some(0.0));
}

#[test]
fn the_first_count_has_no_difference_and_the_next_one_names_what_the_log_does_not_see() {
    let first = count(None, 1_000_000, 1000, &[]);
    assert_eq!((first.logged, first.unseen), (None, None));
    let mut one = session(1000);
    one.sales = vec![deal(1500, 40_000.0, Proof::BoxGone, true), deal(1600, 9_000.0, Proof::Requested, true)];
    one.purchases = vec![deal(1700, 10_000.0, Proof::Answer, false)];
    one.transfers = vec![Transfer { time: 1800, incoming: false, amount: 5_000 }];
    // Before the first count and after the second: not counted in between.
    one.transfers.push(Transfer { time: 1000, incoming: true, amount: 777_777 });
    one.transfers.push(Transfer { time: 3000, incoming: true, amount: 888_888 });
    let second = count(Some(&first), 1_200_000, 2000, &[one]);
    // The log shows +40 000 + 9 000 (the request) - 10 000 - 5 000 = +34 000; the balance rose by 200 000.
    assert_eq!(second.logged, Some(34_000.0));
    assert_eq!(second.unseen, Some(166_000.0));
    assert_eq!(second.requested, 9_000.0);
}

#[test]
fn a_balance_below_the_one_before_gives_a_negative_unseen_part() {
    let first = count(None, 500_000, 0, &[]);
    let second = count(Some(&first), 450_000, 10, &[]);
    assert_eq!((second.logged, second.unseen), (Some(0.0), Some(-50_000.0)));
}

#[test]
fn the_counts_survive_the_file_and_a_broken_entry_is_left_out() {
    let dir = crate::testdir::Dir::new("kassensturz");
    let host = crate::paths::Host::new(
        crate::pypath::Flavor::native(),
        &[(crate::envvars::ENV_HOME, dir.path().to_str().expect("path"))],
        "/",
    );
    let paths = Paths::new(host);
    assert!(counts(&paths).is_empty());
    let first = count(None, 1_000, 10, &[]);
    let second = count(Some(&first), 1_500, 20, &[]);
    assert!(save_counts(&paths, &[second, first]));
    assert_eq!(counts(&paths), vec![first, second], "oldest first");
    assert_eq!(Count::from_value(&Value::Object(vec![("zeit".into(), Value::int(1))])), None);
}

// ---------------------------------------------------------------------------
// The account
// ---------------------------------------------------------------------------

fn namer_item(class: &str, guid: &str) -> Option<String> {
    match (class, guid) {
        ("", "gold") => Some("Gold".into()),
        ("mag", _) => Some("Magazine".into()),
        _ => None,
    }
}

fn namer_place(number: &str) -> Option<String> {
    (number == "7").then(|| "New Babbage".to_string())
}

fn namer() -> Namer<'static> {
    Namer { item: &namer_item, place: &namer_place }
}

fn goods(time: i64, price: f64, proof: Proof) -> Deal {
    Deal { guid: "gold".into(), quantity: 96, place: Some("7".into()), ..deal(time, price, proof, true) }
}

fn opening(balance: i64, time: i64) -> Count {
    count(None, balance, time, &[])
}

fn amounts(account: &Account) -> Vec<(Booking, f64, f64)> {
    account.entries.iter().map(|e| (e.kind, e.amount, e.balance)).collect()
}

#[test]
fn without_a_count_there_is_no_account() {
    let mut one = session(0);
    one.sales = vec![goods(10, 1000.0, Proof::BoxGone)];
    let account = account(&[], &[one], &namer());
    assert!(account.entries.is_empty());
    assert_eq!(account.balance(), None);
}

#[test]
fn the_account_runs_from_the_opening_balance_through_every_booking() {
    let mut one = session(0);
    one.sales = vec![goods(200, 1000.0, Proof::BoxGone), deal(400, 250.0, Proof::Answer, false)];
    one.purchases = vec![deal(300, 100.0, Proof::Answer, false)];
    one.transfers = vec![
        Transfer { time: 500, incoming: true, amount: 4_000 },
        Transfer { time: 600, incoming: false, amount: 2_000 },
    ];
    let account = account(&[opening(10_000, 100)], &[one], &namer());
    assert_eq!(
        amounts(&account),
        vec![
            (Booking::Opening, 10_000.0, 10_000.0),
            (Booking::GoodsSold, 1000.0, 11_000.0),
            (Booking::ItemBought, -100.0, 10_900.0),
            (Booking::ItemSold, 250.0, 11_150.0),
            (Booking::TransferIn, 4_000.0, 15_150.0),
            (Booking::TransferOut, -2_000.0, 13_150.0),
        ]
    );
    assert_eq!(account.balance(), Some(13_150.0));
}

#[test]
fn half_of_an_incoming_sum_sent_on_brings_the_balance_down_again() {
    let mut one = session(0);
    one.transfers = vec![
        Transfer { time: 200, incoming: true, amount: 1_000_000 },
        Transfer { time: 300, incoming: false, amount: 500_000 },
    ];
    let account = account(&[opening(0, 100)], &[one], &namer());
    assert_eq!(account.balance(), Some(500_000.0));
}

#[test]
fn a_sale_from_the_ship_counts_in_full_and_is_marked_as_a_request() {
    let mut one = session(0);
    one.sales = vec![goods(200, 32_000_000.0, Proof::Requested), goods(300, 1.0, Proof::BoxGone)];
    let account = account(&[opening(1_000, 100)], &[one], &namer());
    assert_eq!(account.balance(), Some(32_001_001.0));
    let flags: Vec<bool> = account.entries.iter().map(|e| e.requested).collect();
    assert_eq!(flags, vec![false, true, false]);
}

#[test]
fn a_line_is_named_by_what_and_where_and_a_name_nobody_knows_stays_empty() {
    let mut one = session(0);
    let mut bought = deal(300, 5.0, Proof::Answer, false);
    bought.class = "mag".into();
    bought.quantity = 3;
    let mut odd = deal(400, 5.0, Proof::Answer, false);
    odd.class = "odd_item".into();
    one.sales = vec![goods(200, 1000.0, Proof::BoxGone), odd];
    one.purchases = vec![bought];
    let account = account(&[opening(0, 100)], &[one], &namer());
    let named: Vec<(Option<&str>, Option<&str>, u32)> =
        account.entries.iter().map(|e| (e.what.as_deref(), e.place.as_deref(), e.quantity)).collect();
    assert_eq!(
        named,
        vec![
            (None, None, 0),
            (Some("Gold"), Some("New Babbage"), 96),
            (Some("Magazine"), None, 3),
            // No source names it: the class of the log stands in.
            (Some("odd_item"), None, 1),
        ]
    );
}

#[test]
fn a_later_count_sets_the_balance_and_books_the_difference_as_a_check() {
    let mut one = session(0);
    one.sales = vec![goods(200, 1000.0, Proof::BoxGone), goods(600, 500.0, Proof::BoxGone)];
    // Computed 11 000 at moment 400; the player says 12 500: 1 500 the log does not see.
    let account = account(&[opening(10_000, 100), opening(12_500, 400)], &[one], &namer());
    assert_eq!(
        amounts(&account),
        vec![
            (Booking::Opening, 10_000.0, 10_000.0),
            (Booking::GoodsSold, 1000.0, 11_000.0),
            (Booking::Check, 1_500.0, 12_500.0),
            (Booking::GoodsSold, 500.0, 13_000.0),
        ]
    );
}

#[test]
fn a_balance_below_the_computed_one_books_a_negative_check_and_an_equal_one_books_zero() {
    let mut one = session(0);
    one.sales = vec![goods(200, 1000.0, Proof::BoxGone)];
    let lower = account(&[opening(10_000, 100), opening(10_400, 300)], &[one.clone()], &namer());
    assert_eq!(amounts(&lower)[2], (Booking::Check, -600.0, 10_400.0));
    let equal = account(&[opening(10_000, 100), opening(11_000, 300)], &[one], &namer());
    assert_eq!(amounts(&equal)[2], (Booking::Check, 0.0, 11_000.0));
}

#[test]
fn a_booking_at_the_moment_of_a_count_is_in_that_count_and_one_before_the_first_is_left_out() {
    let mut one = session(0);
    one.transfers = vec![
        Transfer { time: 50, incoming: true, amount: 777 },
        Transfer { time: 100, incoming: true, amount: 888 },
        Transfer { time: 300, incoming: true, amount: 1_000 },
        Transfer { time: 301, incoming: true, amount: 2_000 },
    ];
    let account = account(&[opening(5_000, 100), opening(6_000, 300)], &[one], &namer());
    assert_eq!(
        amounts(&account),
        vec![
            (Booking::Opening, 5_000.0, 5_000.0),
            (Booking::TransferIn, 1_000.0, 6_000.0),
            (Booking::Check, 0.0, 6_000.0),
            (Booking::TransferIn, 2_000.0, 8_000.0),
        ]
    );
}

#[test]
fn the_sessions_are_merged_by_time_and_the_counts_by_their_moment_not_their_order() {
    let (mut early, mut late) = (session(0), session(1000));
    early.transfers = vec![Transfer { time: 700, incoming: true, amount: 7 }];
    late.transfers = vec![Transfer { time: 200, incoming: true, amount: 2 }];
    let account = account(&[opening(10, 600), opening(0, 100)], &[early, late], &namer());
    let moments: Vec<i64> = account.entries.iter().map(|e| e.time).collect();
    assert_eq!(moments, vec![100, 200, 600, 700]);
    assert_eq!(account.entries[0].kind, Booking::Opening);
    assert_eq!(account.entries[2].kind, Booking::Check);
}

#[test]
fn taking_the_last_count_back_returns_to_the_account_before() {
    let (first, second) = (opening(1_000, 10), opening(2_000, 20));
    assert_eq!(withdraw_last(&[first, second]), vec![first]);
    assert_eq!(withdraw_last(&[first]), Vec::<Count>::new());
    assert_eq!(withdraw_last(&[]), Vec::<Count>::new());
    // Without the second count the account is the one of the first again.
    let mut one = session(0);
    one.transfers = vec![Transfer { time: 15, incoming: true, amount: 500 }];
    let both = account(&[first, second], &[one.clone()], &namer());
    let back = account(&withdraw_last(&[first, second]), &[one], &namer());
    assert_eq!((both.balance(), back.balance()), (Some(2_000.0), Some(1_500.0)));
}
