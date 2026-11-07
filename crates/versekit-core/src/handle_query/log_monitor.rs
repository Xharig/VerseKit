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

//! The log monitor: what happens to player characters, from the Game.log, each line shown for the seconds set.
//!
//! `<…> [Notice] <Actor Death> CActor::Kill: 'Victim' [2001] in zone 'Zone' killed by 'Killer' [2002] using
//! 'weapon_s1_123' [Class weapon_s1] with damage type 'Bullet' from direction …`
//!
//! `<…> Loading screen for pu : SC_Frontend closed after 50.43 seconds`
//!
//! ⚠ The game writes the line of a death only in some versions; without it the monitor shows only the loading
//! screens. The numbers in brackets (the accounts) are never taken.
//!
//! Nothing here reads a file: the reader of the log hands over what it reads anyway, as for the shard.

use std::collections::VecDeque;
use std::sync::LazyLock;

use regex::Regex;

use crate::language::{Arg, Lang, t_in, tf_in};
use crate::logtime::{seconds, time_of};
use crate::paths::Paths;

/// Whether the monitor is shown.
pub const KEY_ON: &str = "logmonitor_an";
/// How long a line stays, in seconds.
pub const KEY_SECONDS: &str = "logmonitor_sekunden";
pub const SECONDS_DEFAULT: i64 = 60;
pub const SECONDS_MIN: i64 = 5;
pub const SECONDS_MAX: i64 = 3600;
/// The most lines kept at once; the oldest go first.
pub const LINES_MAX: usize = 50;
/// How many lines read are remembered, so that a section handed twice adds nothing.
const REMEMBERED: usize = 400;

const DEATH_MARK: &str = "<Actor Death>";
const LOADING_MARK: &str = "Loading screen for ";

static DEATH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"<Actor Death> CActor::Kill: '([^']*)' \[\d+\] in zone '[^']*' killed by '([^']*)' \[\d+\](?: using '([^']*)')?(?:[^']*? with damage type '([^']*)')?",
    )
    .expect("pattern")
});
static LOADING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Loading screen for ([A-Za-z0-9_]+) : [A-Za-z0-9_]+ closed after ([0-9]+(?:\.[0-9]+)?) seconds")
        .expect("pattern")
});
/// The number of an entity behind a name: `PU_Human_Enemy_123456789012`.
static SERIAL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"_\d{5,}$").expect("pattern"));

/// What happened.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// A character died; the killer is the victim itself when it died on its own.
    Kill { victim: String, killer: String, weapon: String, damage: String },
    /// A loading screen closed: where to, and after how many seconds.
    Loading { place: String, seconds: f64 },
}

/// Whether `name` is a player character: a handle, not a creature or a person of the game.
pub fn is_player(name: &str) -> bool {
    const GAME: [&str; 6] = ["PU_", "NPC_", "AIModule", "Kopion", "Quasi_Grazer", "unknown"];
    !name.is_empty()
        && crate::handle_query::page::valid_handle(name).as_deref() == Some(name)
        && !SERIAL.is_match(name)
        && !name.contains("_NPC_")
        && !GAME.iter().any(|start| name.starts_with(start))
}

/// A name without the number of the entity behind it.
fn without_serial(name: &str) -> String {
    SERIAL.replace(name, "").into_owned()
}

/// The event of one log line, `None` for every other line and for a death no player character has a part in.
pub fn event_in(line: &str) -> Option<Event> {
    if line.contains(DEATH_MARK) {
        let caps = DEATH.captures(line)?;
        let (victim, killer) = (caps[1].to_string(), caps[2].to_string());
        if !is_player(&victim) && !is_player(&killer) {
            return None;
        }
        let part = |i: usize| caps.get(i).map(|m| m.as_str().to_string()).unwrap_or_default();
        return Some(Event::Kill { victim, killer, weapon: without_serial(&part(3)), damage: part(4) });
    }
    if line.contains(LOADING_MARK) {
        let caps = LOADING.captures(line)?;
        let seconds = caps[2].parse::<f64>().ok().filter(|s| s.is_finite())?;
        return Some(Event::Loading { place: caps[1].to_string(), seconds });
    }
    None
}

/// The lines of `text` with an event, in their order: what the reader of the log hands to the window.
pub fn lines_in(text: &str) -> Vec<String> {
    if !text.contains(DEATH_MARK) && !text.contains(LOADING_MARK) {
        return Vec::new();
    }
    text.split(['\n', '\r']).filter(|line| event_in(line).is_some()).map(str::to_string).collect()
}

/// One line of the monitor.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub event: Event,
    /// The moment the log names (seconds, UTC), `None` without one.
    pub at: Option<i64>,
    /// When the monitor took it (seconds, the clock of the program).
    pub taken: f64,
}

impl Entry {
    /// The handle a click sends to the search: the other player, not `own`; the killer first.
    pub fn handle(&self, own: Option<&str>) -> Option<&str> {
        let Event::Kill { victim, killer, .. } = &self.event else { return None };
        let not_own = |name: &&String| own.is_none_or(|own| !own.eq_ignore_ascii_case(name));
        [killer, victim].into_iter().filter(|name| is_player(name)).find(not_own).map(String::as_str)
    }

    /// The handles of the line: the player characters it names.
    pub fn handles(&self) -> Vec<&str> {
        match &self.event {
            Event::Kill { victim, killer, .. } => {
                let mut list: Vec<&str> =
                    [killer.as_str(), victim.as_str()].into_iter().filter(|n| is_player(n)).collect();
                list.dedup();
                list
            }
            Event::Loading { .. } => Vec::new(),
        }
    }

    /// The line as the monitor writes it.
    pub fn text(&self, lang: Lang) -> String {
        match &self.event {
            Event::Kill { victim, killer, .. } if victim == killer => {
                tf_in(lang, "s_hq_log_tod", &[("opfer", Arg::Text(victim.clone()))])
            }
            Event::Kill { victim, killer, .. } => tf_in(
                lang,
                "s_hq_log_kill",
                &[("taeter", Arg::Text(without_serial(killer))), ("opfer", Arg::Text(without_serial(victim)))],
            ),
            Event::Loading { place, seconds } => {
                let number = format!("{seconds:.1}");
                let number = if lang == Lang::De { number.replace('.', ",") } else { number };
                tf_in(
                    lang,
                    "s_hq_log_laden",
                    &[("ort", Arg::Text(place_name(lang, place))), ("sekunden", Arg::Text(number))],
                )
            }
        }
    }
}

/// The place of a loading screen readable; an unknown one as the log names it.
pub fn place_name(lang: Lang, place: &str) -> String {
    match place.to_lowercase().as_str() {
        "pu" => t_in(lang, "s_hq_ort_pu"),
        "frontend_main" => t_in(lang, "s_hq_ort_menue"),
        _ => place.to_string(),
    }
}

/// The settings of the monitor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub on: bool,
    pub seconds: i64,
}

impl Settings {
    pub fn read(paths: &Paths) -> Settings {
        Settings {
            on: paths.setting_bool(KEY_ON, true),
            seconds: paths.setting_int(KEY_SECONDS, SECONDS_DEFAULT, Some(SECONDS_MIN), Some(SECONDS_MAX)),
        }
    }
}

/// The lines of the monitor.
#[derive(Clone, Debug, Default)]
pub struct Monitor {
    entries: Vec<Entry>,
    /// The lines read last: the reader of the log may hand the same section twice.
    remembered: VecDeque<String>,
}

impl Monitor {
    /// Takes the events of `text` at `now`; true when one was new.
    pub fn take(&mut self, text: &str, now: f64) -> bool {
        if !text.contains(DEATH_MARK) && !text.contains(LOADING_MARK) {
            return false;
        }
        let mut added = false;
        for line in text.split(['\n', '\r']) {
            let Some(event) = event_in(line) else { continue };
            if self.remembered.iter().any(|known| known == line) {
                continue;
            }
            if self.remembered.len() >= REMEMBERED {
                self.remembered.pop_front();
            }
            self.remembered.push_back(line.to_string());
            self.entries.push(Entry { event, at: seconds(time_of(line)).filter(|s| *s != 0), taken: now });
            added = true;
        }
        let skip = self.entries.len().saturating_sub(LINES_MAX);
        self.entries.drain(..skip);
        added
    }

    /// The lines still shown at `now` with `hold` seconds per line, the newest last.
    pub fn shown(&self, now: f64, hold: i64) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(move |entry| now - entry.taken < hold as f64 && entry.taken <= now + 1.0)
    }

    /// Drops the lines no longer shown.
    pub fn forget_old(&mut self, now: f64, hold: i64) {
        self.entries.retain(|entry| now - entry.taken < hold as f64 && entry.taken <= now + 1.0);
    }

    /// A new session: nothing of the old one stays.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.remembered.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KILL: &str = "<2026-10-27T20:03:38.726Z> [Notice] <Actor Death> CActor::Kill: 'Opferpilot' [200012345678] in zone \
                        'OOC_Stanton_1_Hurston' killed by 'Taeterpilot' [200087654321] using 'KLWE_LaserRepeater_S3_3374512012' \
                        [Class KLWE_LaserRepeater_S3] with damage type 'Bullet' from direction x: 0.1, y: 0.2, z: 0.3 \
                        [Team_ActorTech][Actor]";
    const NPC: &str = "<2026-10-27T20:04:00.000Z> [Notice] <Actor Death> CActor::Kill: 'PU_Human_Enemy_GroundCombat_NPC_Pilot_334455667788' \
                       [200011112222] in zone 'X' killed by 'NPC_Archetypes_Guard_998877665544' [200033334444] using 'gun_s1' \
                       [Class gun] with damage type 'Bullet'";
    const NPC_KILLS_PLAYER: &str = "<2026-10-27T20:05:00.000Z> [Notice] <Actor Death> CActor::Kill: 'Opferpilot' [2001] in zone \
                                    'X' killed by 'PU_Pilots_Human_Criminal_123456789012' [2002] using 'gun' [Class gun] with \
                                    damage type 'Bullet'";
    const SUICIDE: &str = "<2026-10-27T20:06:00.000Z> [Notice] <Actor Death> CActor::Kill: 'Opferpilot' [2001] in zone 'X' \
                           killed by 'Opferpilot' [2001] using 'unknown' [Class unknown] with damage type 'Suicide'";
    const LOADING_LINE: &str =
        "<2026-08-06T18:52:56.670Z> Loading screen for pu : SC_Frontend closed after 50.43 seconds";

    #[test]
    fn a_kill_between_players_is_read_without_their_numbers() {
        let event = event_in(KILL).expect("a kill");
        assert_eq!(
            event,
            Event::Kill {
                victim: "Opferpilot".into(),
                killer: "Taeterpilot".into(),
                weapon: "KLWE_LaserRepeater_S3".into(),
                damage: "Bullet".into(),
            }
        );
        assert!(!format!("{event:?}").contains("2000"), "no account number comes along");
    }

    #[test]
    fn deaths_without_a_player_are_left_out() {
        assert_eq!(event_in(NPC), None);
        // Counter-check: the same line with a player as victim is taken.
        assert!(event_in(&NPC.replace("PU_Human_Enemy_GroundCombat_NPC_Pilot_334455667788", "Opferpilot")).is_some());
        let Some(Event::Kill { killer, .. }) = event_in(NPC_KILLS_PLAYER) else { panic!("a kill") };
        assert!(!is_player(&killer));
    }

    #[test]
    fn players_and_people_of_the_game_are_told_apart() {
        for player in ["Musterpilot", "M_u_s_t_e_r", "pilot92", "Muster-20"] {
            assert!(is_player(player), "{player}");
        }
        for game in ["", "unknown", "PU_Human_Enemy_1", "NPC_Guard", "Kopion_Adult", "Pilot_123456789", "a b", "Ünl"] {
            assert!(!is_player(game), "{game}");
        }
    }

    #[test]
    fn a_loading_screen_is_read_with_its_seconds() {
        assert_eq!(event_in(LOADING_LINE), Some(Event::Loading { place: "pu".into(), seconds: 50.43 }));
        assert_eq!(event_in("Loading screen for pu : SC_Frontend closed after abc seconds"), None);
        assert_eq!(event_in("<…> [CLoadingScreenComponent::Enter]"), None);
    }

    #[test]
    fn the_click_sends_the_other_player() {
        let entry = Entry { event: event_in(KILL).expect("kill"), at: None, taken: 0.0 };
        assert_eq!(entry.handle(None), Some("Taeterpilot"));
        assert_eq!(entry.handle(Some("taeterpilot")), Some("Opferpilot"));
        assert_eq!(entry.handles(), ["Taeterpilot", "Opferpilot"]);
        let by_npc = Entry { event: event_in(NPC_KILLS_PLAYER).expect("kill"), at: None, taken: 0.0 };
        assert_eq!(by_npc.handle(Some("Opferpilot")), None, "nobody else to search");
        assert_eq!(by_npc.handle(None), Some("Opferpilot"));
        let loading = Entry { event: event_in(LOADING_LINE).expect("loading"), at: None, taken: 0.0 };
        assert_eq!(loading.handle(None), None);
    }

    #[test]
    fn lines_stay_for_the_seconds_set_and_come_only_once() {
        let mut monitor = Monitor::default();
        let text = format!("{KILL}\n{NPC}\r\n{LOADING_LINE}\n");
        assert!(monitor.take(&text, 100.0));
        assert_eq!(monitor.len(), 2);
        assert!(!monitor.take(&text, 101.0), "a section handed twice adds nothing");
        assert!(!monitor.take("nothing to see", 101.0));
        assert_eq!(monitor.shown(150.0, 60).count(), 2);
        assert_eq!(monitor.shown(160.0, 60).count(), 0);
        // Counter-check: a longer hold keeps them.
        assert_eq!(monitor.shown(160.0, 120).count(), 2);
        assert_eq!(monitor.entries[0].at, seconds("2026-10-27T20:03:38.726Z"), "the moment of the log comes along");
        monitor.forget_old(160.0, 60);
        assert!(monitor.is_empty());
    }

    #[test]
    fn only_the_lines_with_an_event_are_handed_on() {
        let text = format!("<x> noise\n{KILL}\r\n{NPC}\n{LOADING_LINE}\nmore noise");
        assert_eq!(lines_in(&text), [KILL, LOADING_LINE]);
        assert!(lines_in("<x> [Notice] <Join PU> shard[pub_euw1b_1_2]").is_empty());
        let mut monitor = Monitor::default();
        assert!(monitor.take(&lines_in(&text).join("\n"), 0.0));
        assert_eq!(monitor.len(), 2);
    }

    #[test]
    fn the_monitor_keeps_at_most_its_lines() {
        let mut monitor = Monitor::default();
        let text: String = (0..LINES_MAX + 7).map(|n| format!("{LOADING_LINE} #{n}\n")).collect();
        monitor.take(&text, 0.0);
        assert_eq!(monitor.len(), LINES_MAX);
        monitor.clear();
        assert!(monitor.is_empty());
        assert!(monitor.take(LOADING_LINE, 1.0), "after clearing the same line counts again");
    }

    #[test]
    fn the_lines_read_in_both_languages() {
        let kill = Entry { event: event_in(KILL).expect("kill"), at: None, taken: 0.0 };
        assert!(kill.text(Lang::De).contains("Taeterpilot") && kill.text(Lang::De).contains("Opferpilot"));
        assert_ne!(kill.text(Lang::De), kill.text(Lang::En));
        let suicide = Entry { event: event_in(SUICIDE).expect("death"), at: None, taken: 0.0 };
        assert!(suicide.text(Lang::En).contains("Opferpilot"));
        assert_ne!(suicide.text(Lang::En), kill.text(Lang::En));
        let loading = Entry { event: event_in(LOADING_LINE).expect("loading"), at: None, taken: 0.0 };
        assert!(loading.text(Lang::De).contains("50,4"), "{}", loading.text(Lang::De));
        assert!(loading.text(Lang::En).contains("50.4"), "{}", loading.text(Lang::En));
        assert_eq!(place_name(Lang::En, "Frontend_Main"), t_in(Lang::En, "s_hq_ort_menue"));
        assert_eq!(place_name(Lang::En, "Hangar_1"), "Hangar_1");
    }
}
