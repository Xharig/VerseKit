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

//! The transfers of aUEC between players, from the notification the game shows to the receiver and to the sender.
//!
//! ⚠ The notification is a text of several lines: its first line (`Added notification "…:`) tells who and which
//! way, its second line the amount (`1,200,000 aUEC`) with the stamp of the first. The wording is the one of the
//! language files of the game (`notification_PlayerTrade_Notification_*`) in German and English, and the form
//! the game used in the patches before (`Überweisung erhalten von:`, `Sie haben … gesendet:`). Another language
//! gives no transfer: nothing is guessed.
//!
//! ⚠ The game writes the same notification twice in one millisecond (two queue numbers); the same direction and
//! amount in the same second is one transfer. ⚠ The name of the other player is read to tell the way and is
//! never kept.

use std::sync::LazyLock;

use regex::Regex;

use crate::jsonio::Value;

/// A transfer the player received or sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transfer {
    /// Seconds since 1970 (UTC), the stamp of the notification.
    pub time: i64,
    /// Received (true) or sent (false).
    pub incoming: bool,
    /// aUEC.
    pub amount: i64,
}

static INCOMING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"Added notification "(?:Überweisung erhalten von: .+|.+ hat dir gesendet:|.+ has sent you:)\s*$"#)
        .expect("pattern")
});
static OUTGOING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"Added notification "(?:Du hast .+ gesendet:|Sie haben .+ gesendet:|You sent .+:)\s*$"#)
        .expect("pattern")
});
static AMOUNT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^<[^>]+>\s+(\d{1,3}(?:,\d{3})*|\d+) aUEC\s*$").expect("pattern"));

/// The first line of a transfer notification: `Some(true)` for one received, `Some(false)` for one sent.
pub(super) fn head(line: &str) -> Option<bool> {
    if INCOMING.is_match(line) {
        Some(true)
    } else if OUTGOING.is_match(line) {
        Some(false)
    } else {
        None
    }
}

/// The amount of the second line of a transfer notification; at least 1.
pub(super) fn amount(line: &str) -> Option<i64> {
    let text = AMOUNT.captures(line)?[1].replace(',', "");
    text.parse().ok().filter(|n| *n > 0)
}

impl Transfer {
    /// The entry of the journal: `zeit`, `richtung` (`erhalten` or `gesendet`), `betrag`.
    pub fn to_value(&self) -> Value {
        Value::Object(vec![
            ("zeit".into(), Value::int(self.time)),
            ("richtung".into(), Value::str(if self.incoming { "erhalten" } else { "gesendet" })),
            ("betrag".into(), Value::int(self.amount)),
        ])
    }

    /// The transfer of an entry of the journal; `None` when a field is missing or the way is unknown.
    pub fn from_value(value: &Value) -> Option<Transfer> {
        let incoming = match value.get("richtung")?.as_str()? {
            "erhalten" => true,
            "gesendet" => false,
            _ => return None,
        };
        let number = |key: &str| match value.get(key)? {
            Value::Int(text) => text.parse::<i64>().ok(),
            _ => None,
        };
        Some(Transfer { time: number("zeit")?, incoming, amount: number("betrag").filter(|n| *n > 0)? })
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Event, parse_text};
    use super::*;

    fn notification(stamp: &str, first: &str, amount: &str, id: u32) -> String {
        format!(
            "<{stamp}> [Notice] <SHUDEvent_OnNotification> Added notification \"{first}\n<{stamp}> {amount} aUEC\n\
             <{stamp}>  : \" [{id}] to queue. New queue size: 1, MissionId: [0000], ObjectiveId: [] [Team][Missions][Comms]\n"
        )
    }

    fn transfers(text: &str) -> Vec<Transfer> {
        parse_text(text, None)
            .into_iter()
            .filter_map(|e| match e {
                Event::Transfer(t) => Some(t),
                _ => None,
            })
            .collect()
    }

    const STAMP: &str = "2026-11-15T18:12:28.942Z";

    #[test]
    fn a_received_and_a_sent_transfer_in_the_forms_of_the_game() {
        let text = [
            notification(STAMP, "Überweisung erhalten von: Alice", "20,000", 1),
            notification("2026-11-15T18:13:00.000Z", "Alice hat dir gesendet:", "1,200", 2),
            notification("2026-11-15T18:14:00.000Z", "Du hast Bob gesendet:", "1,240,000", 3),
            notification("2026-11-15T18:15:00.000Z", "Sie haben Bob gesendet:", "400,000", 4),
            notification("2026-11-15T18:16:00.000Z", "Bob has sent you:", "5", 5),
            notification("2026-11-15T18:17:00.000Z", "You sent Bob:", "7,500", 6),
        ]
        .concat();
        let found = transfers(&text);
        let way = |t: &Transfer| (t.incoming, t.amount);
        assert_eq!(
            found.iter().map(way).collect::<Vec<_>>(),
            [(true, 20_000), (true, 1_200), (false, 1_240_000), (false, 400_000), (true, 5), (false, 7_500)]
        );
    }

    #[test]
    fn the_same_notification_twice_in_one_moment_is_one_transfer_but_a_later_one_is_another() {
        let text = [
            notification(STAMP, "Du hast Bob gesendet:", "25,000", 27),
            notification(STAMP, "Du hast Bob gesendet:", "25,000", 28),
            notification("2026-11-15T18:12:50.000Z", "Du hast Bob gesendet:", "25,000", 29),
            notification("2026-11-15T18:12:50.000Z", "Bob hat dir gesendet:", "25,000", 30),
        ]
        .concat();
        assert_eq!(transfers(&text).len(), 3);
    }

    #[test]
    fn what_is_no_transfer_gives_none() {
        let update = format!(
            "<{STAMP}> [Notice] <UpdateNotificationItem> Notification \"Überweisung erhalten von: Alice\n<{STAMP}> 9,000 aUEC\n"
        );
        let failed = notification(STAMP, "Die Überweisung von  ist fehlgeschlagen.", "", 3).replace(" aUEC", "");
        let channel = notification(STAMP, "Du hast Kanal [ Aegis : Bob ] verlassen", "", 4).replace(" aUEC", "");
        let unknown_language = notification(STAMP, "Transfert reçu de: Bob", "9,000", 5);
        let no_amount = notification(STAMP, "Du hast Bob gesendet:", "viel", 6);
        let zero = notification(STAMP, "Du hast Bob gesendet:", "0", 7);
        for text in [update, failed, channel, unknown_language, no_amount, zero] {
            assert!(transfers(&text).is_empty(), "{text}");
        }
    }

    #[test]
    fn an_amount_line_of_another_moment_does_not_complete_a_notification() {
        let text = format!(
            "<{STAMP}> [Notice] <SHUDEvent_OnNotification> Added notification \"Du hast Bob gesendet:\n\
             <2026-11-15T18:20:00.000Z> 9,000 aUEC\n"
        );
        assert!(transfers(&text).is_empty());
    }

    #[test]
    fn the_name_of_the_other_player_is_not_kept() {
        let text = notification(STAMP, "Du hast GeheimerName gesendet:", "9,000", 1);
        let entry = super::super::read_log_text(&text, None).expect("session");
        let written = crate::jsonio::dumps(&Value::Object(entry), None, false);
        assert!(written.contains("gesendet") && !written.contains("GeheimerName"), "{written}");
    }

    #[test]
    fn a_transfer_survives_the_journal() {
        let moved = Transfer { time: 1_792_000_000, incoming: false, amount: 1_200_000 };
        assert_eq!(Transfer::from_value(&moved.to_value()), Some(moved));
        let broken = Value::Object(vec![("zeit".into(), Value::int(1)), ("richtung".into(), Value::str("quer"))]);
        assert_eq!(Transfer::from_value(&broken), None);
    }
}
