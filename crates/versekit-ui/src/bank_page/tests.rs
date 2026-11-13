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

use std::time::{Duration, Instant};

use super::*;
use versekit_core::basetool::exchange_double::ExchangeDouble;
use versekit_core::language::{Choice, set_thread_language};

#[test]
fn an_amount_reads_only_with_whole_groups_of_thousands() {
    for (typed, read) in [
        ("15000", Some(15_000)),
        ("15.000", Some(15_000)),
        ("1,250,000", Some(1_250_000)),
        ("1 250 000", Some(1_250_000)),
        (" 900 ", Some(900)),
        ("12,5", None),
        ("1.250,000", None),
        ("1.25.000", None),
        ("1234.567", None),
        ("x", None),
        ("", None),
    ] {
        assert_eq!(parse_amount(typed), read, "{typed}");
    }
}

#[test]
fn amounts_handles_and_free_texts_hide_in_streamer_mode_and_a_withheld_name_stays_withheld() {
    set_thread_language(Choice::De);
    streamer::set_thread_active(Some(false));
    assert_eq!(money(1_250_000), "1.250.000 aUEC");
    assert_eq!(handle(Some("Cpt. Holder")), "Cpt. Holder");
    assert_eq!(free_text("Tanken"), "Tanken");
    assert_eq!(handle(None), t("s_bank_name_verborgen"));
    streamer::set_thread_active(Some(true));
    assert!(!money(1_250_000).contains("1.250"));
    assert!(!handle(Some("Cpt. Holder")).contains("Holder"));
    assert!(!free_text("Tanken").contains("Tanken"));
    assert_eq!(handle(None), t("s_bank_name_verborgen"));
    streamer::set_thread_active(None);
}

#[test]
fn unknown_kinds_and_states_are_named_and_not_shown_raw() {
    set_thread_language(Choice::De);
    assert_eq!(kind_text("REVERSAL"), t("s_bank_storno"));
    assert_eq!(kind_text("HOLDER_TRANSFER"), t("s_bank_halterwechsel"));
    assert_eq!(kind_text("WIPE_RESET"), t("s_bank_ruecksetzung"));
    assert_eq!(kind_text("NEU"), "Sonstiges (NEU)");
    assert_eq!(status_text("ARCHIVED"), "Sonstiges (ARCHIVED)");
}

/// Every text one frame of the page draws.
fn drawn(state: &mut BankState) -> Vec<String> {
    fn collect(shape: &egui::Shape, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(text) => out.push(text.galley.text().to_string()),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out)),
            _ => {}
        }
    }
    let ctx = egui::Context::default();
    crate::install(&ctx);
    let scheme = versekit_core::theme::current();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 1600.0))),
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| show_page(ui, scheme, state));
    output.textures_delta.clear();
    let mut out = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut out);
    }
    out
}

#[test]
fn the_new_request_button_goes_with_the_cap_and_the_queue_tab_comes_with_staff_rights() {
    let _language = crate::language_lock();
    set_thread_language(Choice::De);
    let mut state = BankState { tab: Tab::Requests, ..BankState::default() };
    let texts = drawn(&mut state);
    assert!(texts.iter().any(|t| *t == super::t("s_bank_neu")), "{texts:?}");
    assert!(texts.iter().any(|t| *t == super::t("s_bank_korrigieren")));
    assert!(!texts.iter().any(|t| *t == super::t("s_bank_warteschlange")), "no staff: no queue tab");

    let mut full = sample();
    full.requests = (0..20)
        .map(|i| Request { id: format!("r{i}"), status: "PENDING".into(), client: Some(OWN_CLIENT.into()), ..Request::default() })
        .collect();
    full.not_staff = false;
    full.queue = Some(bank::Queue {
        open: vec![QueueEntry { id: "q".into(), link: "https://beispiel.invalid/q".into(), ..QueueEntry::default() }],
        ..bank::Queue::default()
    });
    state.data = Some(Arc::new(full));
    let texts = drawn(&mut state);
    assert!(!texts.iter().any(|t| *t == super::t("s_bank_neu")), "twenty pending: no new request");
    assert!(texts.iter().any(|t| *t == super::t("s_bank_f_deckel")));
    assert!(texts.iter().any(|t| *t == super::t("s_bank_warteschlange")), "staff: the queue tab");
    state.tab = Tab::Queue;
    let texts = drawn(&mut state);
    assert!(texts.iter().any(|t| *t == super::t("s_bank_im_basetool")));
}

#[test]
fn a_deposit_is_not_offered_and_another_modules_request_keeps_its_amount() {
    assert!(!OFFERED.contains(&Kind::Deposit));
    let foreign = Request {
        id: "r".into(),
        kind: "WITHDRAWAL".into(),
        amount: 9_000,
        status: "PENDING".into(),
        client: None,
        ..Request::default()
    };
    let mut form = Form::correcting(&foreign);
    assert!(form.fixed);
    form.amount = "1".into();
    // The form shows no amount field for it; what it sends keeps what was read in it.
    let own = Form::correcting(&Request { client: Some(OWN_CLIENT.into()), ..foreign.clone() });
    assert!(!own.fixed);
}

/// A page on the stand-in of the Basetool, its bank read once, with one account to ask from.
struct Bench {
    double: Arc<ExchangeDouble>,
    state: BankState,
    store: Arc<BankStore>,
    account: String,
    _dir: versekit_core::testdir::Dir,
}

fn bench(name: &str) -> Bench {
    let dir = versekit_core::testdir::Dir::new(&format!("bank-seite-{name}"));
    let double = Arc::new(ExchangeDouble::new());
    double.set_rights(&["exchange.bank.read", "exchange.bank.request"]);
    let account = double.add_account("KB-1", "KRT", 900_000, true);
    double.add_account("KB-2", "Staffel", 10, true);
    let wire = crate::basetool_page::Wire::over_double(&double, &dir.path().to_string_lossy());
    let link = Link::from_wire(&wire);
    let store = link.store.clone();
    store.refresh(wire.sync.connection(), false, (link.clock)()).expect("read");
    let mut state = BankState::new(Some(link));
    state.follow();
    Bench { double, state, store, account, _dir: dir }
}

impl Bench {
    /// Waits until the write and the history jobs are answered.
    fn settle(&mut self) {
        let end = Instant::now() + Duration::from_secs(20);
        while (self.state.write_job.is_some() || self.state.history_job.is_some()) && Instant::now() < end {
            self.state.follow();
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(self.state.write_job.is_none() && self.state.history_job.is_none(), "answered in time");
    }

    fn fill(&mut self, amount: &str) {
        self.state.open_new();
        let form = self.state.form.as_mut().expect("form");
        form.kind = Kind::Withdrawal;
        form.account = self.account.clone();
        form.amount = amount.into();
        form.justification = "Freitag".into();
    }

    fn requests(&self) -> usize {
        self.double.bank_requests().len()
    }
}

#[test]
fn a_raise_keeps_its_key_through_a_busy_answer_a_lost_answer_closing_and_a_new_page() {
    set_thread_language(Choice::De);
    let mut b = bench("schluessel");
    // Asked to try again: the key stays, nothing made yet.
    b.fill("500");
    b.double.fail("/bank/requests", 409, "IDEMPOTENCY_IN_PROGRESS", None, 1);
    b.state.send();
    b.settle();
    let first = b.store.held().expect("held");
    assert!(first.unclear);
    assert_eq!(b.requests(), 0);
    b.state.send();
    b.settle();
    assert!(b.store.held().is_none(), "answered: the key goes");
    assert_eq!(b.requests(), 1);
    assert!(b.state.form.is_none(), "the form closes with its answer");

    // A lost answer, the form closed and the page made anew: the same raise goes under the same key.
    b.fill("600");
    b.double.lose_next_answer("/bank/requests");
    b.state.send();
    b.settle();
    let lost = b.store.held().expect("held after a lost answer");
    assert!(lost.unclear);
    assert_eq!(b.requests(), 2, "it came through over there");
    b.state.form = None;
    let link = b.state.link.clone();
    b.state = BankState::new(link);
    b.state.follow();
    b.state.open_new();
    assert_eq!(b.state.form.as_ref().map(|f| f.amount.as_str()), Some("600"), "the held raise comes back");
    b.state.send();
    b.settle();
    assert_eq!(b.requests(), 2, "the same key: the same request, no second one");
    assert!(b.store.held().is_none());
}

#[test]
fn a_changed_raise_after_an_unclear_outcome_is_warned_about_and_a_clear_refusal_lets_the_key_go() {
    set_thread_language(Choice::De);
    let mut b = bench("geaendert");
    b.fill("700");
    b.double.lose_next_answer("/bank/requests");
    b.state.send();
    b.settle();
    let held = b.store.held().expect("held");
    b.state.form.as_mut().expect("form").amount = "800".into();
    b.state.send();
    assert!(b.state.write_job.is_none(), "warned, not sent");
    assert_eq!(b.state.message.as_ref().map(|(m, _)| m.clone()), Some(t("s_bank_unklar_geaendert")));
    b.state.send();
    b.settle();
    assert_eq!(b.requests(), 2, "sent as a new request after the warning");
    assert_ne!(b.store.held().map(|h| h.key), Some(held.key));

    b.fill("900");
    b.double.fail("/bank/requests", 409, bank::CAP, None, 1);
    b.state.send();
    b.settle();
    assert!(b.store.held().is_none(), "a clear refusal: the key goes");
}

#[test]
fn a_withdrawal_leaves_the_open_form_and_retry_after_blocks_the_writes() {
    set_thread_language(Choice::De);
    let mut b = bench("zurueck");
    b.fill("100");
    b.state.send();
    b.settle();
    b.store.refresh(b.state.link.as_ref().expect("link").sync.connection(), false, b.state.now()).expect("read");
    b.state.follow();
    let request = b.state.data.as_ref().expect("data").requests[0].clone();
    b.fill("200");
    b.state.withdraw(&request);
    b.settle();
    assert!(b.state.form.is_some(), "the withdrawal of another request leaves the form");
    assert!(b.double.bank_requests().iter().any(|r| r.get("status").and_then(|s| s.as_str()) == Some("CANCELLED")));

    b.double.fail("/bank/requests", 503, "SERVICE_UNAVAILABLE", Some(600), 1);
    b.state.send();
    b.settle();
    assert!(b.state.write_wait().is_some(), "the wait is kept");
    let before = b.requests();
    b.state.send();
    assert!(b.state.write_job.is_none(), "nothing goes out while it stands");
    assert_eq!(b.requests(), before);
}

#[test]
fn the_history_of_an_account_opened_while_another_one_is_read_is_its_own() {
    let mut b = bench("verlauf");
    let other = b.state.data.as_ref().expect("data").accounts[1].id.clone();
    b.state.open_history(&b.account.clone());
    b.state.open_history(&other);
    b.settle();
    // The reading of the first account ended; the second one gets its own.
    b.state.follow();
    b.settle();
    let history = b.state.history.as_ref().expect("history");
    assert_eq!(history.bookings[0].note, other, "the bookings of the account open now");
}
