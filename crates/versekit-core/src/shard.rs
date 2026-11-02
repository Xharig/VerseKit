// Verse-Kit — companion for Star Citizen.
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

//! The shard the player is on, from the Game.log: every time the game puts the player on a server it writes a
//! `<Join PU>` line, a change of shard brings a new one.
//!
//! `<…> [Notice] <Join PU> address[…] port[…] shard[pub_euw1b_12881860_420] locationId[…] …`
//!
//! ⚠ The line names the address of the server too. Only `shard[…]` is taken: the address is not shown, not kept
//! and does not reach the error report.
//!
//! Nothing here reads a file. The overlay hands over what its reader of the log reads anyway (the whole running
//! log at the start, then every new section), the statistics take the joins into the session they evaluate.

use std::sync::LazyLock;

use regex::Regex;

use crate::fps_scan::Reading;
use crate::language::{Lang, t_in, tf_in};
use crate::logtime::{seconds, time_of};

/// The mark of the line of a join.
pub const JOIN_MARK: &str = "<Join PU>";

/// Only letters, digits and `_`: nothing else of the line can come along.
static SHARD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bshard\[([A-Za-z0-9_]+)\]").expect("pattern"));

/// The readable names of the regions by the letters in front of their number (`euw1b` → `euw`), longest first.
const REGIONS: [(&str, &str); 9] = [
    ("use", "s_shard_r_use"),
    ("usw", "s_shard_r_usw"),
    ("usc", "s_shard_r_usc"),
    ("euw", "s_shard_r_euw"),
    ("euc", "s_shard_r_euc"),
    ("eun", "s_shard_r_eun"),
    ("au", "s_shard_r_au"),
    ("ap", "s_shard_r_ap"),
    ("as", "s_shard_r_ap"),
];

/// A shard as the log names it: `pub_euw1b_12881860_420`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shard {
    pub id: String,
}

impl Shard {
    /// The channel (`pub`, `ptu` …): the first part.
    pub fn channel(&self) -> &str {
        self.id.split('_').next().unwrap_or("")
    }

    /// The region with its zone (`euw1b`): the second part; empty where the name has no more parts.
    pub fn region(&self) -> &str {
        let parts: Vec<&str> = self.id.split('_').collect();
        if parts.len() < 3 { "" } else { parts[1] }
    }

    /// The number of the shard (`12881860-420`): the parts after the region; the whole name where it has no region.
    pub fn number(&self) -> String {
        if self.region().is_empty() {
            return self.id.clone();
        }
        self.id.split('_').skip(2).collect::<Vec<_>>().join("-")
    }

    /// The longest part of the number: what tells this shard from every other one.
    pub fn main_number(&self) -> Option<&str> {
        self.id.split('_').skip(2).max_by_key(|part| part.len()).filter(|part| part.len() >= 4)
    }

    /// The text key of the region, `None` where its letters are not known.
    pub fn region_key(&self) -> Option<&'static str> {
        let letters: String = self.region().chars().take_while(char::is_ascii_alphabetic).collect();
        let letters = letters.to_lowercase();
        REGIONS.iter().find(|(start, _)| letters.starts_with(start)).map(|(_, key)| *key)
    }

    /// The region readable in `lang` (`EU-West`), its code in capitals where it is not known; a channel other than
    /// `pub` stands in front (`PTU EU-West`).
    pub fn region_name(&self, lang: Lang) -> String {
        let region = match self.region_key() {
            Some(key) => t_in(lang, key),
            None if self.region().is_empty() => String::new(),
            None => self.region().to_uppercase(),
        };
        let channel = self.channel();
        if channel.eq_ignore_ascii_case("pub") || channel.is_empty() || region.is_empty() {
            region
        } else {
            format!("{} {region}", channel.to_uppercase())
        }
    }

    /// The short name for the first line of the overlay: region and the last part of the number (`euw1b · 420`); a
    /// channel other than `pub` in front (`PTU euw1b · 420`), the whole name where it has no region.
    pub fn short(&self) -> String {
        if self.region().is_empty() {
            return self.id.clone();
        }
        let last = self.id.rsplit('_').next().unwrap_or("");
        let channel = self.channel();
        if channel.eq_ignore_ascii_case("pub") {
            format!("{} · {last}", self.region())
        } else {
            format!("{} {} · {last}", channel.to_uppercase(), self.region())
        }
    }

    /// The region for a table: readable with its code (`EU-West (euw1b)`), the code alone where it is not known.
    pub fn region_label(&self, lang: Lang) -> String {
        let name = self.region_name(lang);
        if self.region_key().is_some() { format!("{name} ({})", self.region()) } else { name }
    }
}

/// One join: when (seconds, UTC, from the stamp of the line; `None` without one) and which shard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Join {
    pub at: Option<i64>,
    pub shard: Shard,
}

/// The join of a log line, `None` for every other line.
pub fn join_in(line: &str) -> Option<Join> {
    if !line.contains(JOIN_MARK) {
        return None;
    }
    let shard = SHARD.captures(line)?[1].to_string();
    Some(Join { at: seconds(time_of(line)).filter(|s| *s != 0), shard: Shard { id: shard } })
}

/// The joins of a text, in the order of the log.
pub fn joins_in(text: &str) -> Vec<Join> {
    if !text.contains(JOIN_MARK) {
        return Vec::new();
    }
    text.split(['\n', '\r']).filter_map(join_in).collect()
}

/// The joins of the running session, as the log has them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Joins(Vec<Join>);

impl Joins {
    /// Takes the joins of `text`. One with the same moment and shard as one already taken is not taken again (the
    /// reader of the log may hand the same lines twice); true when one was new.
    pub fn take(&mut self, text: &str) -> bool {
        let mut added = false;
        for join in joins_in(text) {
            let known = match join.at {
                Some(_) => self.0.contains(&join),
                None => self.0.last() == Some(&join),
            };
            if !known {
                keep_private(&join.shard);
                self.0.push(join);
                added = true;
            }
        }
        added
    }

    /// A new session: nothing of the old one counts.
    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn list(&self) -> &[Join] {
        &self.0
    }

    /// The shard the player is on now: the last join.
    pub fn current(&self) -> Option<&Join> {
        self.0.last()
    }

    /// How often the shard changed: a join on another shard than the one before.
    pub fn changes(&self) -> usize {
        changes(&self.0)
    }
}

/// The number of a shard is private in streamer mode: whoever knows it can follow the player onto the server.
pub fn keep_private(shard: &Shard) {
    if let Some(number) = shard.main_number() {
        crate::streamer::learn(number);
    }
}

/// How often the shard changed in `joins`.
pub fn changes(joins: &[Join]) -> usize {
    joins.windows(2).filter(|pair| pair[0].shard != pair[1].shard).count()
}

/// The time of day of a moment in the local time (`17:26`), empty without one.
fn clock(at: Option<i64>) -> String {
    at.and_then(crate::formats::local_time).map(|t| crate::formats::strftime("%H:%M", &t)).unwrap_or_default()
}

/// Which parts of the reading the player switched on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Show {
    pub own: bool,
    pub server: bool,
    pub players: bool,
}

impl Show {
    pub const ALL: Show = Show { own: true, server: true, players: true };
}

/// The frame rates switched on as one part: `FPS 41 · Server-FPS 19,5`; `None` without one to show.
pub fn fps_part(reading: &Reading, show: Show, lang: Lang) -> Option<String> {
    let own = show.own.then(|| tf_in(lang, "ov_fps_allein", &[("0", format!("{:.0}", reading.fps).into())]));
    let server = reading
        .server
        .filter(|_| show.server)
        .map(|server| tf_in(lang, "ov_fps_server", &[("0", crate::formats::fixed(lang, server, 1).into())]));
    let parts: Vec<String> = own.into_iter().chain(server).collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// The players as one part: `Shard 482 · Server 37`; `None` when they are off or not read.
pub fn players_part(reading: &Reading, show: Show, lang: Lang) -> Option<String> {
    let players = reading.players.filter(|_| show.players)?;
    let shard = i64::from(players.shard);
    Some(match players.server {
        Some(server) => tf_in(lang, "ov_spieler", &[("0", shard.into()), ("1", i64::from(server).into())]),
        None => tf_in(lang, "ov_spieler_shard", &[("0", shard.into())]),
    })
}

/// The part at the right of the first line of the overlay: the shard now in short, then the frame rates and the
/// players switched on where they are read. `None` when there is nothing.
pub fn status_part(joins: &Joins, fps: Option<&Reading>, show: Show, lang: Lang) -> Option<String> {
    let read = fps.into_iter().flat_map(|r| [fps_part(r, show, lang), players_part(r, show, lang)]).flatten();
    let parts: Vec<String> = joins.current().map(|join| join.shard.short()).into_iter().chain(read).collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// The hint over the shard in the overlay: the whole name with the region readable, the time of the join and the
/// changes of the session. `None` without a shard.
pub fn hint(joins: &Joins, lang: Lang) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(join) = joins.current() {
        let shard = &join.shard;
        let name = [shard.region_name(lang), shard.number()].into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>();
        parts.push(tf_in(lang, "ov_shard", &[("0", name.join(" ").into())]));
        let time = clock(join.at);
        if !time.is_empty() {
            parts.push(tf_in(lang, "ov_shard_seit", &[("0", time.into())]));
        }
        let changed = joins.changes();
        if changed > 0 {
            parts.push(tf_in(lang, "ov_shard_wechsel", &[("0", changed.into())]));
        }
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINE: &str = "<2026-10-30T16:26:47.302Z> [Notice] <Join PU> address[35.241.242.86] port[64388] \
                        shard[pub_euw1b_12881860_420] locationId[562954248454145] [Team_GameServices][GIM][Matchmaking]";

    fn shard(id: &str) -> Shard {
        Shard { id: id.to_string() }
    }

    #[test]
    fn a_join_gives_the_shard_and_its_moment_and_nothing_of_the_address() {
        let join = join_in(LINE).expect("a join");
        assert_eq!(join.shard.id, "pub_euw1b_12881860_420");
        // 2026-10-30 16:26:47 UTC.
        assert_eq!(join.at, Some(1_793_377_607));
        let kept = format!("{join:?}");
        for part in ["35.241.242.86", "64388", "562954248454145"] {
            assert!(!kept.contains(part), "{part} must not be kept: {kept}");
        }
        // Another line with a shard in it is no join; a join without a stamp knows no moment.
        assert_eq!(join_in("<x> <Other> shard[pub_euw1b_1_2]"), None);
        assert_eq!(join_in("<Join PU> address[1.2.3.4] shard[pub_use1a_7_8]").map(|j| j.at), Some(None));
        // Something that is not a plain name does not count.
        assert_eq!(join_in("<Join PU> shard[] port[1]"), None);
    }

    #[test]
    fn the_name_is_cut_into_channel_region_and_number() {
        let s = shard("pub_euw1b_12881860_420");
        assert_eq!((s.channel(), s.region(), s.number()), ("pub", "euw1b", "12881860-420".to_string()));
        assert_eq!(s.main_number(), Some("12881860"));
        assert_eq!(s.region_key(), Some("s_shard_r_euw"));
        assert_eq!(s.region_name(Lang::De), "EU-West");
        assert_eq!(s.region_name(Lang::En), "EU West");
        assert_eq!(shard("ptu_use1a_5_1").region_name(Lang::De), "PTU US-Ost");
        assert_eq!(shard("pub_xyz9_5_1").region_name(Lang::De), "XYZ9", "an unknown region keeps its code");
        assert_eq!(s.region_label(Lang::De), "EU-West (euw1b)");
        assert_eq!(shard("pub_xyz9_5_1").region_label(Lang::En), "XYZ9");
        let odd = shard("einzeln");
        assert_eq!((odd.region(), odd.number(), odd.region_name(Lang::De)), ("", "einzeln".to_string(), String::new()));
    }

    #[test]
    fn the_session_keeps_each_join_once_and_counts_only_real_changes() {
        let at = |time: &str, id: &str| format!("<2026-10-30T{time}.000Z> <Join PU> address[1.2.3.4] shard[{id}]\n");
        let mut joins = Joins::default();
        let first =
            format!("{}nothing\n{}", at("16:00:00", "pub_euw1b_73100011_1"), at("16:30:00", "pub_euw1b_73100022_1"));
        assert!(joins.take(&first));
        // The same lines handed again: nothing new.
        assert!(!joins.take(&at("16:30:00", "pub_euw1b_73100022_1")));
        assert_eq!(joins.list().len(), 2);
        // Back on the same shard after a lost connection: a join, but no change.
        assert!(joins.take(&at("17:00:00", "pub_euw1b_73100022_1")));
        assert_eq!((joins.list().len(), joins.changes()), (3, 1));
        assert_eq!(joins.current().map(|j| j.shard.id.as_str()), Some("pub_euw1b_73100022_1"));
        joins.clear();
        assert_eq!(joins.current(), None);
    }

    #[test]
    fn the_number_of_a_shard_taken_is_covered_in_streamer_mode() {
        let mut joins = Joins::default();
        joins.take("<2026-10-30T16:00:00.000Z> <Join PU> shard[pub_euw1b_98765432_17]\n");
        crate::streamer::set_thread_active(Some(true));
        let covered = crate::streamer::scrub("Shard: EU-West 98765432-17");
        let other = crate::streamer::scrub("Shard: EU-West 55554444-17");
        crate::streamer::set_thread_active(None);
        assert!(!covered.contains("98765432"), "{covered}");
        assert!(other.contains("55554444"), "a number never taken stays: {other}");
    }

    #[test]
    fn the_first_line_names_the_shard_short_and_the_hint_names_it_whole() {
        let lines = "<2026-10-30T16:00:00.000Z> <Join PU> shard[pub_euw1b_73100033_7]\n\
                     <2026-10-30T16:26:47.302Z> <Join PU> shard[pub_euw1b_12881860_420]\n";
        let mut joins = Joins::default();
        joins.take(lines);
        let reading = Reading { fps: 40.6, server: Some(19.5), players: None };
        assert_eq!(status_part(&joins, None, Show::ALL, Lang::De).as_deref(), Some("euw1b · 420"));
        assert_eq!(
            status_part(&joins, Some(&reading), Show::ALL, Lang::De).as_deref(),
            Some("euw1b · 420 · FPS 41 · Server-FPS 19,5")
        );
        let time = clock(Some(1_793_377_607));
        assert_eq!(
            hint(&joins, Lang::De).as_deref(),
            Some(format!("Shard: EU-West 12881860-420 · seit {time} · Wechsel: 1").as_str())
        );
        assert_eq!(
            hint(&joins, Lang::En).as_deref(),
            Some(format!("Shard: EU West 12881860-420 · since {time} · changes: 1").as_str())
        );
        // The short name keeps a test channel and a name without a region whole.
        assert_eq!(shard("ptu_use1a_5_17").short(), "PTU use1a · 17");
        assert_eq!(shard("einzeln").short(), "einzeln");
        // Only the frame rates, without the server: the part is theirs alone; nothing at all gives no part.
        let own = Reading { fps: 59.4, server: None, players: None };
        assert_eq!(status_part(&Joins::default(), Some(&own), Show::ALL, Lang::De).as_deref(), Some("FPS 59"));
        // The players follow the frame rates, the server count where it was read.
        let players = crate::fps_scan::Players { shard: 482, server: Some(37) };
        let full = Reading { players: Some(players), ..reading };
        assert_eq!(fps_part(&full, Show::ALL, Lang::De).as_deref(), Some("FPS 41 · Server-FPS 19,5"));
        assert_eq!(fps_part(&full, Show::ALL, Lang::En).as_deref(), Some("FPS 41 · Server FPS 19.5"));
        assert_eq!(players_part(&full, Show::ALL, Lang::De).as_deref(), Some("Shard 482 · Server 37"));
        let shard_only = Reading { players: Some(crate::fps_scan::Players { server: None, ..players }), ..own };
        assert_eq!(players_part(&shard_only, Show::ALL, Lang::De).as_deref(), Some("Shard 482"));
        assert_eq!(status_part(&Joins::default(), None, Show::ALL, Lang::De), None);
        // Each part on its own switch: only what is on shows, nothing switched on gives no part.
        let only_server = Show { server: true, ..Show::default() };
        assert_eq!(fps_part(&full, only_server, Lang::De).as_deref(), Some("Server-FPS 19,5"));
        assert_eq!(players_part(&full, only_server, Lang::De), None);
        let only_players = Show { players: true, ..Show::default() };
        assert_eq!(fps_part(&full, only_players, Lang::De), None);
        assert_eq!(
            status_part(&Joins::default(), Some(&full), only_players, Lang::De).as_deref(),
            Some("Shard 482 · Server 37")
        );
        assert_eq!(status_part(&Joins::default(), Some(&full), Show::default(), Lang::De), None);
        assert_eq!(hint(&Joins::default(), Lang::De), None);
    }
}
