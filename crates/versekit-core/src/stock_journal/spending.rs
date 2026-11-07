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

//! What was spent in the shops, summed from the purchases of the journal: today, in the last seven days, in
//! the last session with a purchase, and in all. Only spending: the log has no line for a sale, so there is
//! no income to set against it.

use super::Session;

/// A week, in seconds.
const WEEK: i64 = 7 * 24 * 3600;

/// The sums of the purchases.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Spending {
    /// All purchases.
    pub total: f64,
    pub purchases: usize,
    /// Of the calendar day of `now`.
    pub today: f64,
    /// Of the last seven days up to `now`.
    pub week: f64,
    /// Of the newest session with a purchase: its start (seconds since 1970, UTC) and its sum.
    pub last_session: Option<(i64, f64)>,
}

impl Spending {
    /// Nothing bought yet?
    pub fn is_empty(&self) -> bool {
        self.purchases == 0
    }
}

/// The sums of `sessions` at the moment `now`. `day` names the calendar day of a moment (local time), so that
/// "today" is the player's day and not the one of UTC.
pub fn spending(sessions: &[Session], now: i64, day: &dyn Fn(i64) -> String) -> Spending {
    let today = day(now);
    let mut out = Spending::default();
    for session in sessions {
        let mut sum = 0.0;
        for purchase in &session.purchases {
            let price = purchase.price_total;
            sum += price;
            out.total += price;
            out.purchases += 1;
            if day(purchase.time) == today {
                out.today += price;
            }
            if purchase.time <= now && now - purchase.time < WEEK {
                out.week += price;
            }
        }
        if !session.purchases.is_empty() && out.last_session.is_none_or(|(start, _)| session.start >= start) {
            out.last_session = Some((session.start, sum));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::Purchase;
    use super::*;

    fn buy(time: i64, price_total: f64) -> Purchase {
        Purchase {
            time,
            class: "x".into(),
            guid: "g".into(),
            quantity: 1,
            price_total,
            place: None,
            shop: "SCShop_Kiosk".into(),
        }
    }

    fn session(start: i64, purchases: Vec<Purchase>) -> Session {
        Session { start, purchases, sales: Vec::new(), movements: Vec::new() }
    }

    /// Days of 86 400 seconds from the epoch, as UTC days: the clock of the check.
    fn utc_day(seconds: i64) -> String {
        (seconds.div_euclid(86_400)).to_string()
    }

    const NOW: i64 = 20 * 86_400 + 12 * 3600;

    #[test]
    fn the_sums_of_today_the_week_the_last_session_and_all() {
        let sessions = [
            session(10 * 86_400, vec![buy(10 * 86_400 + 60, 1000.0)]),
            session(18 * 86_400, vec![buy(18 * 86_400 + 60, 200.0), buy(18 * 86_400 + 120, 300.0)]),
            session(20 * 86_400, vec![buy(20 * 86_400 + 60, 50.0)]),
        ];
        let sums = spending(&sessions, NOW, &utc_day);
        assert_eq!(sums.total, 1550.0);
        assert_eq!(sums.purchases, 4);
        assert_eq!(sums.today, 50.0);
        assert_eq!(sums.week, 550.0, "the purchase ten days ago is not in the week");
        assert_eq!(sums.last_session, Some((20 * 86_400, 50.0)));
        assert!(!sums.is_empty());
    }

    #[test]
    fn a_session_without_a_purchase_is_not_the_last_one_and_nothing_bought_is_empty() {
        let sessions = [session(10 * 86_400, vec![buy(10 * 86_400 + 60, 70.0)]), session(19 * 86_400, Vec::new())];
        let sums = spending(&sessions, NOW, &utc_day);
        assert_eq!(sums.last_session, Some((10 * 86_400, 70.0)));
        let none = spending(&[session(1, Vec::new())], NOW, &utc_day);
        assert!(none.is_empty());
        assert_eq!(none, Spending::default());
    }

    #[test]
    fn today_follows_the_day_the_caller_names_and_not_a_window_of_24_hours() {
        // Yesterday evening is within 24 hours of now but another day.
        let sessions = [session(19 * 86_400, vec![buy(19 * 86_400 + 23 * 3600, 400.0)])];
        let sums = spending(&sessions, NOW, &utc_day);
        assert_eq!(sums.today, 0.0);
        assert_eq!(sums.week, 400.0);
    }

    #[test]
    fn a_purchase_after_now_does_not_count_in_the_week() {
        let sessions = [session(21 * 86_400, vec![buy(21 * 86_400, 90.0)])];
        let sums = spending(&sessions, NOW, &utc_day);
        assert_eq!((sums.week, sums.total), (0.0, 90.0));
    }
}
