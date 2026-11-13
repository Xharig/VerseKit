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

//! Armistice zones in the Game.log, in every game language: the HUD tells "Entering Armistice Zone" and "Leaving
//! Armistice Zone" as a notification, `Added notification "<text>: "`, the way the injuries come.
//!
//! The text is translated; its language-neutral keys stand in the `global.ini` of the game (`hud_enter_green_zone`
//! and `hud_leave_green_zone`). The phrases are taken from the unpacked `global.ini` files of the installation (a
//! translation lies there), then from the bundled table of the wordings seen in game logs.
//!
//! The lines, from a log of 18.10.2026 (German client):
//! `[Notice] <SHUDEvent_OnNotification> Added notification "Betrete Waffenstillstandszone - Kampf verboten: " [2] to
//! queue. …` and `… Added notification "Waffenstillstandszone wird verlassen - Vorsicht geboten: " [3] to
//! queue. …`.
//! The lines `<UpdateNotificationItem> Notification "…"` that follow are not read.
//!
//! ⚠ Nothing here reads the log: the reader of the log hands over the text it reads anyway.

use std::fs;
use std::path::Path;

use crate::logtime::{seconds, time_of};
use crate::phrases::Sources;
use crate::text::{decode_ignore, lines_universal, py_strip};

/// The notification of the HUD, the same in every language.
pub use crate::injuries::NOTIFICATION;

/// The key of the text "Entering Armistice Zone" in the `global.ini`.
pub const KEY_ENTER: &str = "hud_enter_green_zone";
/// The key of the text "Leaving Armistice Zone" in the `global.ini`.
pub const KEY_LEAVE: &str = "hud_leave_green_zone";

/// A notice older than this (seconds, by the stamp of its line) is history, not news: nothing is told of it.
pub const STALE_AFTER: i64 = 120;

/// What the HUD told.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    /// The player entered an armistice zone: combat is prohibited.
    Entered,
    /// The player left it: weapons are free again.
    Left,
}

impl Zone {
    /// The key of the text in the `global.ini`.
    pub fn ini_key(self) -> &'static str {
        match self {
            Zone::Entered => KEY_ENTER,
            Zone::Left => KEY_LEAVE,
        }
    }
}

/// The wordings seen, for when no `global.ini` of the installation holds them: the English and the German ones of
/// the game files, and the two of the community translation found in game logs of June 2026 (the game's own
/// German took over from them in the logs of September).
pub const TABLE: [(Zone, &str); 6] = [
    (Zone::Entered, "Entering Armistice Zone - Combat Prohibited"),
    (Zone::Left, "Leaving Armistice Zone - Caution Advised"),
    (Zone::Entered, "Betrete Waffenstillstandszone - Kampf verboten"),
    (Zone::Left, "Waffenstillstandszone wird verlassen - Vorsicht geboten"),
    (Zone::Entered, "Schutzzone - Kampfhandlung untersagt!"),
    (Zone::Left, "Schutzzone verlassen"),
];

/// The phrases of the two notices, longest first, so that no phrase takes the line of a longer one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Phrases(Vec<(Zone, String)>);

impl Phrases {
    /// The bundled table only.
    pub fn table() -> Phrases {
        Phrases::from_list(TABLE.iter().map(|(zone, text)| (*zone, (*text).to_string())).collect())
    }

    /// The phrases of the `global.ini` files of `sources`, then the table.
    pub fn of(sources: &Sources) -> Phrases {
        let mut list: Vec<(Zone, String)> = Vec::new();
        for file in sources.ini_files() {
            list.extend(from_ini(&file));
        }
        list.extend(TABLE.iter().map(|(zone, text)| (*zone, (*text).to_string())));
        Phrases::from_list(list)
    }

    fn from_list(list: Vec<(Zone, String)>) -> Phrases {
        let mut kept: Vec<(Zone, String)> = Vec::new();
        for (zone, text) in list {
            let text = py_strip(&text).trim_end_matches(':').trim_end().to_string();
            if !text.is_empty() && !kept.iter().any(|(_, known)| *known == text) {
                kept.push((zone, text));
            }
        }
        kept.sort_by_key(|(_, text)| std::cmp::Reverse(text.chars().count()));
        Phrases(kept)
    }

    /// The zone notice the line tells of: a notification whose text begins with a phrase.
    pub fn zone(&self, line: &str) -> Option<Zone> {
        let (_, after) = line.split_once(NOTIFICATION)?;
        let text = after.split('"').next().unwrap_or("").trim();
        self.0.iter().find(|(_, phrase)| text.starts_with(phrase.as_str())).map(|(zone, _)| *zone)
    }

    /// The last zone notice of `text` (the newest one counts: several in a row are the player passing a door),
    /// with the moment its line names (seconds, UTC; `None` without a stamp). `None` when `text` holds none.
    pub fn last_in(&self, text: &str) -> Option<(Zone, Option<i64>)> {
        if !text.contains(NOTIFICATION) {
            return None;
        }
        text.split(['\n', '\r']).rev().find_map(|line| {
            let zone = self.zone(line)?;
            Some((zone, seconds(time_of(line)).filter(|s| *s != 0)))
        })
    }
}

/// Whether a notice with the stamp `at` is still news at `now` (both seconds, UTC). A notice without a stamp is.
pub fn is_news(at: Option<i64>, now: i64) -> bool {
    at.is_none_or(|at| now - at <= STALE_AFTER)
}

/// The phrases of one `global.ini`: the value of each of the two keys, a key with `,P` behind its name too.
pub fn from_ini(path: &Path) -> Vec<(Zone, String)> {
    let Ok(bytes) = fs::read(path) else { return Vec::new() };
    let text = decode_ignore(&bytes);
    let mut found = Vec::new();
    for line in lines_universal(text.trim_start_matches('\u{feff}')) {
        let Some((key, value)) = line.split_once('=') else { continue };
        let key = key.split(',').next().unwrap_or("").trim();
        for zone in [Zone::Entered, Zone::Left] {
            if zone.ini_key().eq_ignore_ascii_case(key) {
                found.push((zone, value.to_string()));
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lines of the logs of 18.10.2026 (German client), as the game wrote them; no name and no account stands in them.
    const ENTER_DE: &str = "<2026-10-18T13:26:41.747Z> [Notice] <SHUDEvent_OnNotification> Added notification \
        \"Betrete Waffenstillstandszone - Kampf verboten: \" [2] to queue. New queue size: 1, MissionId: \
        [00000000-0000-0000-0000-000000000000], ObjectiveId: [] [Team_CoreGameplayFeatures][Missions][Comms]";
    const LEAVE_DE: &str = "<2026-10-18T13:27:27.618Z> [Notice] <SHUDEvent_OnNotification> Added notification \
        \"Waffenstillstandszone wird verlassen - Vorsicht geboten: \" [3] to queue. New queue size: 1, MissionId: \
        [00000000-0000-0000-0000-000000000000], ObjectiveId: [] [Team_CoreGameplayFeatures][Missions][Comms]";
    /// The lines that follow each notice and must not count again.
    const FOLLOW_UP: [&str; 3] = [
        "<2026-10-18T13:27:27.618Z>    \"Waffenstillstandszone wird verlassen - Vorsicht geboten: \" [3]",
        "<2026-10-18T13:27:27.620Z> [Notice] <UpdateNotificationItem> Notification \"Waffenstillstandszone wird \
         verlassen - Vorsicht geboten: \" [3], Action: Next [Team_CoreGameplayFeatures][Missions][Comms]",
        "<2026-10-18T13:27:32.620Z> [Notice] <UpdateNotificationItem> Notification \"Waffenstillstandszone wird \
         verlassen - Vorsicht geboten: \" [3], Action: Remove [Team_CoreGameplayFeatures][Missions][Comms]",
    ];
    /// The wordings of the community translation in the logs of 10.06.2026.
    const ENTER_OLD: &str = "<2026-06-10T13:33:20.134Z> [Notice] <SHUDEvent_OnNotification> Added notification \
        \"Schutzzone - Kampfhandlung untersagt!: \" [3] to queue. New queue size: 3, MissionId: \
        [00000000-0000-0000-0000-000000000000], ObjectiveId: [] [Team_CoreGameplayFeatures][Missions][Comms]";
    const LEAVE_OLD: &str = "<2026-06-09T13:57:14.395Z> [Notice] <SHUDEvent_OnNotification> Added notification \
        \"Schutzzone verlassen: \" [21] to queue. New queue size: 4, MissionId: \
        [00000000-0000-0000-0000-000000000000], ObjectiveId: [] [Team_CoreGameplayFeatures][Missions][Comms]";

    fn english(text: &str) -> String {
        format!(
            "<2026-10-18T13:27:27.618Z> [Notice] <SHUDEvent_OnNotification> Added notification \"{text}: \" [3] to \
             queue."
        )
    }

    #[test]
    fn the_notices_of_the_real_logs_are_told_in_the_wordings_seen() {
        let phrases = Phrases::table();
        assert_eq!(phrases.zone(ENTER_DE), Some(Zone::Entered));
        assert_eq!(phrases.zone(LEAVE_DE), Some(Zone::Left));
        assert_eq!(phrases.zone(ENTER_OLD), Some(Zone::Entered));
        assert_eq!(phrases.zone(LEAVE_OLD), Some(Zone::Left));
        assert_eq!(phrases.zone(&english("Entering Armistice Zone - Combat Prohibited")), Some(Zone::Entered));
        assert_eq!(phrases.zone(&english("Leaving Armistice Zone - Caution Advised")), Some(Zone::Left));
    }

    #[test]
    fn the_lines_that_follow_a_notice_and_other_lines_tell_nothing() {
        let phrases = Phrases::table();
        for line in FOLLOW_UP {
            assert_eq!(phrases.zone(line), None, "{line}");
        }
        // Another notification, a removed one, a mention outside a notification: nothing.
        assert_eq!(phrases.zone(&english("Rechtsgebiet von UEE betreten")), None);
        assert_eq!(phrases.zone(&english("Bauplan erhalten: Helm")), None);
        assert_eq!(phrases.zone("Removed notification \"Schutzzone verlassen: \""), None);
        assert_eq!(phrases.zone("<Debug> Schutzzone verlassen"), None);
        assert_eq!(phrases.zone(""), None);
    }

    #[test]
    fn the_phrases_of_a_global_ini_come_by_their_keys_in_any_language() {
        let dir = crate::testdir::fresh("schutzzone-ini");
        let folder = dir.join("Localization").join("french_(france)");
        fs::create_dir_all(&folder).unwrap();
        let ini = "\u{feff}hud_enter_green_zone=Entrée en zone d'armistice - Combat interdit\n\
                   hud_leave_green_zone,P=Sortie de la zone d'armistice - Prudence\n\
                   hud_law_GreenZoneWarning=Zone d'armistice\n\
                   autre_cle=Sortie\n";
        fs::write(folder.join("global.ini"), ini).unwrap();
        let sources = Sources { phrases_file: None, localization_folder: Some(dir.join("Localization")) };
        let phrases = Phrases::of(&sources);
        let notice = |text: &str| english(text);
        assert_eq!(phrases.zone(&notice("Entrée en zone d'armistice - Combat interdit")), Some(Zone::Entered));
        assert_eq!(
            phrases.zone(&notice("Sortie de la zone d'armistice - Prudence")),
            Some(Zone::Left),
            "a key with ,P"
        );
        assert_eq!(phrases.zone(&notice("Zone d'armistice")), None, "another key of the file is none");
        assert_eq!(phrases.zone(&notice("Sortie")), None, "a word of another key is none");
        assert_eq!(phrases.zone(LEAVE_DE), Some(Zone::Left), "the table stays");
        // Counter check: without the file the French wording is not known.
        assert_eq!(Phrases::table().zone(&notice("Entrée en zone d'armistice - Combat interdit")), None);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn of_a_whole_section_the_newest_notice_counts_with_its_moment() {
        let phrases = Phrases::table();
        let text = format!("<x> noise\n{ENTER_DE}\r\n{}\n{LEAVE_DE}\n{}\nmore noise", FOLLOW_UP[0], FOLLOW_UP[1]);
        let (zone, at) = phrases.last_in(&text).expect("a notice");
        assert_eq!(zone, Zone::Left);
        assert_eq!(at, seconds("2026-10-18T13:27:27.618Z"), "the moment of the log comes along");
        // Counter check: the order decides, not the kind.
        let (zone, _) = phrases.last_in(&format!("{LEAVE_DE}\n{ENTER_DE}")).expect("a notice");
        assert_eq!(zone, Zone::Entered);
        assert_eq!(phrases.last_in("<x> [Notice] <Join PU> shard[pub_euw1b_1_2]"), None);
        assert_eq!(phrases.last_in(&FOLLOW_UP.join("\n")), None);
    }

    /// A stretch of the log of 18.10.2026 as the game wrote it (name and numbers of the player blacked out): the
    /// walk through a door of a station, in and out, with the follow-up lines and the traffic between them.
    const WALK: &str = "\
<2026-10-18T13:26:05.736Z> [Notice] <Inventory Request Completed> Request[13] Player[Spieler] Result[succeed] Elapsed[0.265357] PendingMoves[1] [Team_CoreGameplayFeatures][Inventory]
<2026-10-18T13:26:41.747Z> [Notice] <SHUDEvent_OnNotification> Added notification \"Betrete Waffenstillstandszone - Kampf verboten: \" [2] to queue. New queue size: 1, MissionId: [00000000-0000-0000-0000-000000000000], ObjectiveId: [] [Team_CoreGameplayFeatures][Missions][Comms]
<2026-10-18T13:26:41.747Z>    \"Betrete Waffenstillstandszone - Kampf verboten: \" [2]
<2026-10-18T13:26:41.749Z> [Notice] <UpdateNotificationItem> Notification \"Betrete Waffenstillstandszone - Kampf verboten: \" [2], Action: Next [Team_CoreGameplayFeatures][Missions][Comms]
<2026-10-18T13:26:41.757Z> [Notice] <AttachmentReceived> Player[Spieler] Attachment[behr_lmg_ballistic_01_000000000000, behr_lmg_ballistic_01, 000000000000] Status[persistent] Port[wep_stocked_2] Elapsed[36.286381] [Team_CoreGameplayFeatures][Inventory]
<2026-10-18T13:26:46.750Z> [Notice] <UpdateNotificationItem> Notification \"Betrete Waffenstillstandszone - Kampf verboten: \" [2], Action: StartFade [Team_CoreGameplayFeatures][Missions][Comms]
<2026-10-18T13:26:46.750Z> [Notice] <UpdateNotificationItem> Notification \"Betrete Waffenstillstandszone - Kampf verboten: \" [2], Action: Remove [Team_CoreGameplayFeatures][Missions][Comms]
<2026-10-18T13:27:27.618Z> [Notice] <SHUDEvent_OnNotification> Added notification \"Waffenstillstandszone wird verlassen - Vorsicht geboten: \" [3] to queue. New queue size: 1, MissionId: [00000000-0000-0000-0000-000000000000], ObjectiveId: [] [Team_CoreGameplayFeatures][Missions][Comms]
<2026-10-18T13:27:27.618Z>    \"Waffenstillstandszone wird verlassen - Vorsicht geboten: \" [3]
<2026-10-18T13:27:27.620Z> [Notice] <UpdateNotificationItem> Notification \"Waffenstillstandszone wird verlassen - Vorsicht geboten: \" [3], Action: Next [Team_CoreGameplayFeatures][Missions][Comms]
<2026-10-18T13:27:32.620Z> [Notice] <UpdateNotificationItem> Notification \"Waffenstillstandszone wird verlassen - Vorsicht geboten: \" [3], Action: Remove [Team_CoreGameplayFeatures][Missions][Comms]
";

    #[test]
    fn a_walk_through_a_door_of_the_real_log_holds_exactly_two_notices_in_the_order_they_came() {
        let phrases = Phrases::table();
        let told: Vec<Zone> = WALK.lines().filter_map(|line| phrases.zone(line)).collect();
        assert_eq!(told, [Zone::Entered, Zone::Left], "the follow-up lines of each notice count for nothing");
        // Read in two sections, as the reader of the log hands it over: each section tells its own newest notice.
        let (first, second) = WALK.split_at(WALK.find("<2026-10-18T13:27:27").expect("the second notice"));
        assert_eq!(phrases.last_in(first).map(|(zone, _)| zone), Some(Zone::Entered));
        assert_eq!(phrases.last_in(second).map(|(zone, _)| zone), Some(Zone::Left));
        assert_eq!(phrases.last_in(WALK).map(|(zone, _)| zone), Some(Zone::Left), "in one section the newest counts");
    }

    #[test]
    fn an_old_notice_is_history_and_one_without_a_stamp_is_news() {
        let at = seconds("2026-10-18T13:27:27.618Z");
        let now = at.expect("a stamp");
        assert!(is_news(at, now + 5));
        assert!(is_news(at, now + STALE_AFTER));
        assert!(!is_news(at, now + STALE_AFTER + 1), "a notice of the day before is no news");
        assert!(is_news(None, now + 1_000_000));
    }
}
