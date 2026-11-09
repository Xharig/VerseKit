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

//! What was spent in the shops, summed from the purchases of the journal, and what came in, summed from the sales:
//! today, in the last seven days, in the last session with such a deal, and in all. The sales also by good and by
//! place ([`sales_by`]), and any moments counted the same way ([`counted`]).
//!
//! ⚠ The income counts the sales at a commodity terminal that the log shows only as a request
//! ([`super::Proof::Requested`]); `requested` says how many of them there are.

use super::{Deal, Session};

/// A week, in seconds.
const WEEK: i64 = 7 * 24 * 3600;

/// The sums of the purchases (or of the sales).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Spending {
    /// All purchases.
    pub total: f64,
    /// How many deals.
    pub purchases: usize,
    /// Of them, how many the log shows only as a request.
    pub requested: usize,
    /// What those requested ones add up to: in all, today and in the last seven days.
    pub requested_total: f64,
    pub requested_today: f64,
    pub requested_week: f64,
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
    sums(sessions, now, day, |session| session.purchases.as_slice())
}

/// The sums of the sales of `sessions` at the moment `now`, the same way as [`spending`].
pub fn income(sessions: &[Session], now: i64, day: &dyn Fn(i64) -> String) -> Spending {
    sums(sessions, now, day, |session| session.sales.as_slice())
}

fn sums(sessions: &[Session], now: i64, day: &dyn Fn(i64) -> String, deals: fn(&Session) -> &[Deal]) -> Spending {
    let today = day(now);
    let mut out = Spending::default();
    for session in sessions {
        let mut sum = 0.0;
        for deal in deals(session) {
            let price = deal.price_total;
            sum += price;
            out.total += price;
            out.purchases += 1;
            let requested = !deal.proof.confirmed();
            if requested {
                out.requested += 1;
                out.requested_total += price;
            }
            if day(deal.time) == today {
                out.today += price;
                if requested {
                    out.requested_today += price;
                }
            }
            if deal.time <= now && now - deal.time < WEEK {
                out.week += price;
                if requested {
                    out.requested_week += price;
                }
            }
        }
        if !deals(session).is_empty() && out.last_session.is_none_or(|(start, _)| session.start >= start) {
            out.last_session = Some((session.start, sum));
        }
    }
    out
}

/// The sales of one good or of one place.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Share {
    /// What the sales are grouped by: the name of the good or of the place; empty when unknown.
    pub key: String,
    /// All of them.
    pub total: f64,
    /// Of the last seven days up to `now`.
    pub week: f64,
    /// How many sales.
    pub deals: usize,
    /// Of them, how many the log shows only as a request.
    pub requested: usize,
}

/// The sales of `sessions` grouped by `key`, the largest sum first and of equal sums the keys in order.
pub fn sales_by(sessions: &[Session], now: i64, key: &dyn Fn(&Deal) -> String) -> Vec<Share> {
    let mut shares: Vec<Share> = Vec::new();
    for deal in sessions.iter().flat_map(|session| &session.sales) {
        let name = key(deal);
        let at = match shares.iter().position(|share| share.key == name) {
            Some(at) => at,
            None => {
                shares.push(Share { key: name, ..Share::default() });
                shares.len() - 1
            }
        };
        let share = &mut shares[at];
        share.total += deal.price_total;
        share.deals += 1;
        share.requested += usize::from(!deal.proof.confirmed());
        if deal.time <= now && now - deal.time < WEEK {
            share.week += deal.price_total;
        }
    }
    shares.sort_by(|a, b| b.total.total_cmp(&a.total).then_with(|| a.key.cmp(&b.key)));
    shares
}

/// How many moments there are: today, in the last seven days up to `now` and in all.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counted {
    pub today: usize,
    pub week: usize,
    pub total: usize,
}

/// Counts `moments` (seconds since 1970, UTC) like [`spending`] sums: today by the day `day` names.
pub fn counted(moments: impl IntoIterator<Item = i64>, now: i64, day: &dyn Fn(i64) -> String) -> Counted {
    summed(moments.into_iter().map(|moment| (moment, 0.0)), now, day).count
}

/// Amounts at their moments, counted and summed today, in the last seven days up to `now` and in all.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Summed {
    pub count: Counted,
    pub today: f64,
    pub week: f64,
    pub total: f64,
}

/// Counts and sums `amounts` (moment in seconds since 1970 UTC, amount) like [`spending`].
pub fn summed(amounts: impl IntoIterator<Item = (i64, f64)>, now: i64, day: &dyn Fn(i64) -> String) -> Summed {
    let today = day(now);
    let mut out = Summed::default();
    for (moment, amount) in amounts {
        out.count.total += 1;
        out.total += amount;
        if day(moment) == today {
            out.count.today += 1;
            out.today += amount;
        }
        if moment <= now && now - moment < WEEK {
            out.count.week += 1;
            out.week += amount;
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
            proof: super::super::Proof::Answer,
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
    fn the_income_sums_the_sales_and_counts_the_ones_only_requested() {
        use super::super::Proof;
        let sell = |time: i64, price: f64, proof: Proof| Purchase { proof, ..buy(time, price) };
        let sessions = [
            Session {
                start: 18 * 86_400,
                purchases: vec![buy(18 * 86_400 + 10, 999.0)],
                sales: vec![sell(18 * 86_400 + 60, 100.0, Proof::Answer)],
                movements: Vec::new(),
            },
            Session {
                start: 20 * 86_400,
                purchases: Vec::new(),
                sales: vec![
                    sell(20 * 86_400 + 60, 1_810_296.0, Proof::BoxGone),
                    sell(20 * 86_400 + 80, 664_687.0, Proof::Requested),
                ],
                movements: Vec::new(),
            },
        ];
        let got = income(&sessions, NOW, &utc_day);
        assert_eq!(got.total, 2_475_083.0);
        assert_eq!((got.purchases, got.requested, got.requested_total), (3, 1, 664_687.0));
        assert_eq!((got.requested_today, got.requested_week), (664_687.0, 664_687.0));
        assert_eq!(got.today, 2_474_983.0);
        assert_eq!(got.last_session, Some((20 * 86_400, 2_474_983.0)));
        let spent = spending(&sessions, NOW, &utc_day);
        assert_eq!((spent.total, spent.purchases, spent.requested), (999.0, 1, 0), "sales are no spending");
        assert_eq!(spent.last_session, Some((18 * 86_400, 999.0)));
    }

    #[test]
    fn the_sales_are_grouped_by_their_key_the_largest_first_and_purchases_stay_out() {
        use super::super::Proof;
        let sell = |time: i64, class: &str, price: f64, proof: Proof| Purchase {
            proof,
            class: class.into(),
            ..buy(time, price)
        };
        let sessions = [
            Session {
                start: 5 * 86_400,
                purchases: vec![buy(5 * 86_400, 5_000.0)],
                sales: vec![sell(5 * 86_400 + 10, "gold", 300.0, Proof::BoxGone)],
                movements: Vec::new(),
            },
            Session {
                start: 20 * 86_400,
                purchases: Vec::new(),
                sales: vec![
                    sell(20 * 86_400 + 10, "stims", 700.0, Proof::Requested),
                    sell(20 * 86_400 + 20, "gold", 100.0, Proof::Answer),
                    sell(20 * 86_400 + 30, "", 50.0, Proof::Answer),
                ],
                movements: Vec::new(),
            },
        ];
        let shares = sales_by(&sessions, NOW, &|deal| deal.class.clone());
        let rows: Vec<(&str, f64, f64, usize, usize)> =
            shares.iter().map(|s| (s.key.as_str(), s.total, s.week, s.deals, s.requested)).collect();
        assert_eq!(rows, [("stims", 700.0, 700.0, 1, 1), ("gold", 400.0, 100.0, 2, 0), ("", 50.0, 50.0, 1, 0)]);
        assert!(sales_by(&[session(1, vec![buy(1, 9.0)])], NOW, &|d| d.class.clone()).is_empty());
    }

    #[test]
    fn moments_are_counted_today_in_the_week_and_in_all() {
        let moments = [10 * 86_400, 18 * 86_400, 20 * 86_400 + 60, 20 * 86_400 + 600, 21 * 86_400];
        let got = counted(moments, NOW, &utc_day);
        assert_eq!(got, Counted { today: 2, week: 3, total: 5 }, "a moment after now is not in the week");
        assert_eq!(counted([], NOW, &utc_day), Counted::default());
        let paid = summed([(10 * 86_400, 1000.0), (20 * 86_400 + 60, 250.0), (21 * 86_400, 5.0)], NOW, &utc_day);
        assert_eq!(paid.count, Counted { today: 1, week: 1, total: 3 });
        assert_eq!((paid.today, paid.week, paid.total), (250.0, 250.0, 1255.0));
    }

    #[test]
    fn a_purchase_after_now_does_not_count_in_the_week() {
        let sessions = [session(21 * 86_400, vec![buy(21 * 86_400, 90.0)])];
        let sums = spending(&sessions, NOW, &utc_day);
        assert_eq!((sums.week, sums.total), (0.0, 90.0));
    }
}
