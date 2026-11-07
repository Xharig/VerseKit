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

//! The cycle of the hangar, the start and the self timers.

use super::*;
use crate::net::Fake;

/// The start the page published on 24 Oct 2026.
const START: i64 = 1_792_860_059;
/// A moment at which the page showed `00:04:35` and "Hangar Resetting".
const SEEN_AT: f64 = 1_792_870_884.999;

fn paths_in(name: &str, off: bool) -> (Paths, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("versekit-contested-{}-{name}", crate::testdir::run_tag()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let home = dir.to_string_lossy().into_owned();
    let mut vars = vec![(crate::legacy::ENV_HOME, home.as_str())];
    if off {
        vars.push((crate::legacy::ENV_NO_NET, "1"));
    }
    let host = crate::paths::Host::new(crate::pypath::Flavor::native(), &vars, &home);
    (Paths::new(host), dir)
}

/// The hangar `seconds` after the start of a cycle.
fn at(seconds: i64) -> Hangar {
    hangar_at(START, (START + seconds) as f64)
}

#[test]
fn the_seconds_the_page_showed_are_the_seconds_counted() {
    let hangar = hangar_at(START, SEEN_AT);
    assert_eq!(hangar.cycle_rest, 275, "4:35 as the page showed it");
    assert_eq!(hms(hangar.cycle_rest), "00:04:35");
    assert_eq!(hangar.phase, Phase::Resetting);
    assert_eq!(hangar.to_change, 275);
    assert_eq!(hangar.lamps, [Lamp::Dark; LAMPS]);
}

#[test]
fn the_red_phase_counts_to_the_opening_and_turns_a_lamp_green_every_24_minutes() {
    let hangar = at(3_000);
    assert_eq!(hangar.phase, Phase::Closed);
    assert_eq!(hangar.to_change, 4_200);
    assert_eq!(hangar.cycle_rest, 8_100);
    assert_eq!(hangar.lamps, [Lamp::Green, Lamp::Green, Lamp::Red, Lamp::Red, Lamp::Red]);
    assert_eq!(at(0).lamps, [Lamp::Red; LAMPS]);
    assert_eq!(at(1_439).lamps, [Lamp::Red; LAMPS], "the first lamp turns green after 24 minutes");
    assert_eq!(at(1_440).lamps[0], Lamp::Green);
    assert_eq!(at(7_199).lamps, [Lamp::Green, Lamp::Green, Lamp::Green, Lamp::Green, Lamp::Red]);
    assert_eq!(at(7_199).to_change, 1);
}

#[test]
fn the_green_phase_counts_to_the_reset_and_puts_a_lamp_out_every_12_minutes() {
    let open = at(7_200);
    assert_eq!((open.phase, open.to_change), (Phase::Open, 3_600));
    assert_eq!(open.lamps, [Lamp::Green; LAMPS]);
    let hangar = at(7_200 + 1_500);
    assert_eq!(hangar.phase, Phase::Open);
    assert_eq!(hangar.to_change, 2_100);
    assert_eq!(hangar.lamps, [Lamp::Dark, Lamp::Dark, Lamp::Green, Lamp::Green, Lamp::Green]);
    assert_eq!(at(7_200 + 719).lamps, [Lamp::Green; LAMPS]);
    let last = at(10_799);
    assert_eq!((last.phase, last.to_change), (Phase::Open, 1));
    assert_eq!(last.lamps, [Lamp::Dark, Lamp::Dark, Lamp::Dark, Lamp::Dark, Lamp::Green]);
    let reset = at(10_800);
    assert_eq!((reset.phase, reset.to_change, reset.cycle_rest), (Phase::Resetting, 300, 300));
}

#[test]
fn the_cycle_starts_again_after_11100_seconds() {
    let again = at(CYCLE);
    assert_eq!((again.phase, again.to_change, again.cycle_rest), (Phase::Closed, CLOSED, CYCLE));
    assert_eq!(
        hangar_at(START, SEEN_AT),
        hangar_at(START - 7 * CYCLE, SEEN_AT),
        "a start of an earlier cycle counts alike"
    );
    assert_eq!(hangar_at(START + 3 * CYCLE, SEEN_AT), hangar_at(START, SEEN_AT), "and so does one of a later cycle");
}

/// The proof that the numbers are not a lucky hit: a start that is off by a second, or the wrong second of the
/// moment, does not show the 4:35 the page showed.
#[test]
fn a_start_off_by_a_second_is_not_the_cycle_the_page_showed() {
    assert_eq!(hangar_at(START + 1, SEEN_AT).cycle_rest, 276);
    assert_eq!(hangar_at(START - 1, SEEN_AT).cycle_rest, 274);
    assert_ne!(hangar_at(START, SEEN_AT + 1.0).cycle_rest, 275);
    assert_eq!(hangar_at(START, SEEN_AT - 0.999).cycle_rest, 275, "the whole second counts: 884.0 and 884.999 alike");
}

/// The page's own way, written out as its script does it (`updateExecTimer`), against which every second of
/// several cycles is compared.
fn like_the_page(start: i64, now: f64) -> (Phase, i64, [Lamp; LAMPS]) {
    let elapsed = (now - start as f64).floor() as i64;
    let time_left = CYCLE - (elapsed % CYCLE);
    let (red, green, black) = (2 * 60 * 60, 60 * 60, 5 * 60);
    let mut lights = [Lamp::Dark; LAMPS];
    if time_left > green + black {
        let red_time = time_left - (green + black);
        let since = red - red_time;
        for (i, light) in lights.iter_mut().enumerate() {
            *light = if since >= (i as i64 + 1) * 24 * 60 { Lamp::Green } else { Lamp::Red };
        }
        (Phase::Closed, red_time, lights)
    } else if time_left > black {
        let green_time = time_left - black;
        let since = green - green_time;
        for (i, light) in lights.iter_mut().enumerate() {
            *light = if since >= (i as i64 + 1) * 12 * 60 { Lamp::Dark } else { Lamp::Green };
        }
        (Phase::Open, green_time, lights)
    } else {
        (Phase::Resetting, time_left, lights)
    }
}

#[test]
fn every_second_of_three_cycles_is_what_the_page_shows() {
    for fraction in [0.0, 0.5, 0.999] {
        for second in 0..3 * CYCLE + 200 {
            let now = (START + second) as f64 + fraction;
            let hangar = hangar_at(START, now);
            let (phase, to_change, lamps) = like_the_page(START, now);
            assert_eq!(
                (hangar.phase, hangar.to_change, hangar.lamps),
                (phase, to_change, lamps),
                "second {second} + {fraction}"
            );
        }
    }
}

#[test]
fn the_next_openings_are_the_ones_after_now() {
    let now = (START + 3_000) as f64;
    let openings = next_openings(START, now, 6);
    assert_eq!(openings.len(), 6);
    assert_eq!(openings[0], Opening { opens: START + 7_200, closes: START + 10_800 });
    for (k, opening) in openings.iter().enumerate() {
        assert_eq!(opening.opens, START + 7_200 + k as i64 * CYCLE);
        assert_eq!(opening.closes - opening.opens, OPEN);
        assert!(opening.opens as f64 > now);
    }
    // At the second the hangar opens it is open, so the next opening is the one of the next cycle.
    let at_opening = next_openings(START, (START + 7_200) as f64, 1);
    assert_eq!(at_opening[0].opens, START + 7_200 + CYCLE);
    let before = next_openings(START, (START + 7_199) as f64 + 0.9, 1);
    assert_eq!(before[0].opens, START + 7_200);
    // During the open time the next opening is the one after it.
    assert_eq!(next_openings(START, (START + 8_000) as f64, 1)[0].opens, START + 7_200 + CYCLE);
    assert!(next_openings(START, now, 0).is_empty());
    // Each opening is the moment `hangar_at` turns to open.
    for opening in &openings {
        assert_eq!(hangar_at(START, opening.opens as f64).phase, Phase::Open);
        assert_eq!(hangar_at(START, (opening.opens - 1) as f64).phase, Phase::Closed);
        assert_eq!(hangar_at(START, opening.closes as f64).phase, Phase::Resetting);
    }
}

#[test]
fn the_answer_of_the_page_is_a_whole_number_and_nothing_else() {
    assert_eq!(parse_start(b"1792860059"), Some(START));
    assert_eq!(parse_start(b" 1792860059\r\n"), Some(START));
    assert_eq!(parse_start(b""), None);
    assert_eq!(parse_start(b"12"), None, "not a time since 1970 anyone uses");
    assert_eq!(parse_start(b"1792860059.5"), None);
    assert_eq!(parse_start(b"1792860059 1792860060"), None);
    assert_eq!(parse_start(b"<html>404</html>"), None);
    assert_eq!(parse_start(b"-1792860059"), None);
}

#[test]
fn a_start_typed_by_hand_is_unix_seconds_or_a_time_of_day() {
    let now = START + 5_000;
    assert_eq!(parse_input(" 1792860059 ", now), Some(START));
    assert_eq!(parse_input("12", now), None);
    for bad in ["", "abc", "24:00", "12:60", "12:", ":30", "ab:cd", "1:2:3", "-1:30", "12.30"] {
        assert_eq!(parse_input(bad, now), None, "{bad:?}");
    }
    for text in ["00:00", "07:05", "14:32", "23:59"] {
        let start = parse_input(text, now).unwrap_or_else(|| panic!("{text}"));
        assert!(start <= now && now - start < 86_400, "{text}: the latest such moment");
        assert_eq!(clock(start), text);
    }
    // The time of day of now is now's own minute, not a day before.
    let here = clock(now);
    let start = parse_input(&here, now).unwrap();
    assert!(now - start < 60);
}

fn fresh(site: Option<i64>, fetched: f64, hand: Option<i64>) -> Kept {
    Kept { site, fetched, hand }
}

#[test]
fn the_hand_beats_the_page_and_the_page_is_asked_every_six_hours() {
    assert_eq!(fresh(Some(1), 0.0, Some(2)).start(), Some(2));
    assert_eq!(fresh(Some(1), 0.0, None).start(), Some(1));
    assert_eq!(Kept::default().start(), None);
    let kept = fresh(Some(START), 1_000.0, None);
    assert!(!kept.due(1_000.0 + FETCH_EVERY - 1.0));
    assert!(kept.due(1_000.0 + FETCH_EVERY));
    assert!(Kept::default().due(5.0), "never fetched");
    assert!(fresh(None, 1_000.0, Some(START)).due(1_001.0), "a hand does not stand in for the page");
}

#[test]
fn the_start_is_fetched_kept_and_kept_beside_the_hand() {
    let (paths, dir) = paths_in("holen", false);
    let net = Fake::default().with(START_URL, "1792860059\n");
    assert_eq!(load(&paths), Kept::default());
    let Outcome::Fresh(kept) = refresh(&paths, &net, 5_000.0) else { panic!("fetched") };
    assert_eq!(kept, fresh(Some(START), 5_000.0, None));
    assert_eq!(load(&paths), kept, "kept in the data folder");
    assert_eq!(net.asked(), [START_URL]);

    let by_hand = set_hand(&paths, Some(START + 100)).unwrap();
    assert_eq!(by_hand, fresh(Some(START), 5_000.0, Some(START + 100)));
    // A fetch later leaves the hand alone.
    let Outcome::Fresh(later) = refresh(&paths, &net, 90_000.0) else { panic!("fetched") };
    assert_eq!(later, fresh(Some(START), 90_000.0, Some(START + 100)));
    assert_eq!(later.start(), Some(START + 100));
    assert_eq!(set_hand(&paths, None).unwrap().start(), Some(START), "and forgetting it brings the page back");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_page_that_fails_or_lies_changes_nothing() {
    let (paths, dir) = paths_in("fehler", false);
    let good = Fake::default().with(START_URL, "1792860059");
    assert!(matches!(refresh(&paths, &good, 100.0), Outcome::Fresh(_)));
    let before = load(&paths);
    for body in ["", "oops", "5", "<html>"] {
        let net = Fake::default().with(START_URL, body);
        assert!(matches!(refresh(&paths, &net, 200.0), Outcome::Failed(_)), "{body:?}");
    }
    assert!(matches!(refresh(&paths, &Fake::default(), 200.0), Outcome::Failed(_)), "the page is not there");
    assert_eq!(load(&paths), before);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn nothing_is_asked_with_the_net_switched_off() {
    let (paths, dir) = paths_in("aus", true);
    let net = Fake::default().with(START_URL, "1792860059");
    assert_eq!(refresh(&paths, &net, 100.0), Outcome::Offline);
    assert!(net.asked().is_empty());
    assert_eq!(load(&paths), Kept::default());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_with_a_wrong_start_gives_no_start() {
    let (paths, dir) = paths_in("datei", true);
    let target = paths.app_file(FILE);
    std::fs::write(&target, r#"{"format": 1, "start": 12, "geholt": 7.5, "hand": null}"#).unwrap();
    assert_eq!(load(&paths), fresh(None, 7.5, None));
    std::fs::write(&target, "not json").unwrap();
    assert_eq!(load(&paths), Kept::default());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn times_are_written_with_the_page_s_digits() {
    assert_eq!(hms(0), "00:00:00");
    assert_eq!(hms(3_661), "01:01:01");
    assert_eq!(hms(-5), "00:00:00");
    assert_eq!(minutes_seconds(900), "15:00");
    assert_eq!(minutes_seconds(1_799), "29:59");
    assert_eq!(minutes_seconds(-1), "00:00");
}

#[test]
fn a_moment_is_written_by_day_only_when_it_is_not_today() {
    let now = START;
    let today = when(Lang::De, now + 60, now);
    assert!(today.contains(&clock(now + 60)), "{today}");
    let later = when(Lang::De, now + 3 * 86_400, now);
    assert_eq!(later.len(), "24.10. 22:15".len(), "{later}");
    assert_eq!(&later[2..3], ".");
    let english = when(Lang::En, now + 3 * 86_400, now);
    assert_eq!(&english[2..3], "-", "{english}");
}

#[test]
fn the_self_timers_are_those_of_the_page() {
    let timers: Vec<&TimerSpec> = GROUPS.iter().flat_map(|g| g.sections.iter()).flat_map(|s| s.timers.iter()).collect();
    assert_eq!(timers.len(), 17);
    let mut ids: Vec<&str> = timers.iter().map(|t| t.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 17, "every id once");
    let minutes = |id: &str| timers.iter().find(|t| t.id == id).map(|t| t.seconds / 60);
    assert_eq!(minutes("checkmate-terminal3"), Some(15));
    assert_eq!(minutes("checkmate-tablet1"), Some(30));
    assert_eq!(minutes("orbituary-tablet7"), Some(30));
    assert_eq!(minutes("ruinstation-lastresort"), Some(15));
    assert_eq!(minutes("ruinstation-tablet5"), Some(30));
    assert_eq!(minutes("pyam-3-5"), Some(30));
    assert_eq!(GROUPS.map(|g| g.sections.iter().map(|s| s.timers.len()).sum::<usize>()), [6, 4, 5, 2]);
}

#[test]
fn a_self_timer_counts_down_stops_goes_on_and_runs_out_once() {
    let mut timer = SelfTimer::new(900);
    assert_eq!((timer.state(), timer.left(0.0)), (TimerState::Idle, 900));
    timer.start(1_000.0);
    assert_eq!(timer.state(), TimerState::Running);
    assert_eq!(timer.left(1_000.0), 900, "a timer just started shows its full time");
    assert_eq!(timer.left(1_000.4), 900);
    assert_eq!(timer.left(1_001.0), 899);
    assert_eq!(timer.left(1_300.5), 600);
    timer.stop(1_300.5);
    assert_eq!((timer.state(), timer.left(9_999.0)), (TimerState::Paused, 600), "stopped: the time stands");
    timer.start(2_000.0);
    assert_eq!(timer.left(2_100.0), 500);
    assert!(!timer.tick(2_599.0));
    assert!(timer.tick(2_600.0), "runs out at 0");
    assert!(!timer.tick(2_601.0), "once");
    assert_eq!((timer.state(), timer.left(2_601.0)), (TimerState::Done, 0));
    assert!(timer.highlighted(2_600.0 + HIGHLIGHT - 0.1));
    assert!(!timer.highlighted(2_600.0 + HIGHLIGHT));
    timer.start(2_700.0);
    assert_eq!(timer.state(), TimerState::Done, "no time left: it does not start");
    timer.reset();
    assert_eq!((timer.state(), timer.left(0.0)), (TimerState::Idle, 900));
}

#[test]
fn the_minute_buttons_stay_between_nothing_and_the_full_time() {
    let mut timer = SelfTimer::new(900);
    timer.adjust(1, 0.0);
    assert_eq!(timer.left(0.0), 900, "not above the full time");
    timer.adjust(-3, 0.0);
    assert_eq!((timer.state(), timer.left(0.0)), (TimerState::Paused, 720));
    timer.adjust(-30, 0.0);
    assert_eq!(timer.left(0.0), 0);
    timer.adjust(1, 0.0);
    assert_eq!(timer.left(0.0), 60);
    // Running: the end moves.
    timer.reset();
    timer.start(100.0);
    timer.adjust(-5, 160.0);
    assert_eq!(timer.left(160.0), 900 - 60 - 300);
    assert_eq!(timer.left(170.0), 900 - 60 - 300 - 10);
    // Pulled to 0 while running: it runs out at once.
    timer.adjust(-60, 170.0);
    assert!(timer.tick(170.0));
    // A timer that ran out is alive again with a minute.
    timer.adjust(1, 171.0);
    assert_eq!((timer.state(), timer.left(171.0)), (TimerState::Paused, 60));
    assert_eq!(timer.wake_in(171.0), None);
    timer.start(171.0);
    assert_eq!(timer.wake_in(172.0), Some(59.0));
}

/// What a player saw at the hangar on 24 Oct 2026, about 22:25 local time: four lamps red, one green. That is
/// 39 minutes into a cycle that began at the start of the page plus one cycle.
#[test]
fn four_lamps_red_and_one_green_is_the_first_lamp_of_a_new_cycle() {
    let hangar = hangar_at(START, (START + CYCLE + 39 * 60) as f64);
    assert_eq!(hangar.phase, Phase::Closed);
    assert_eq!(hangar.green, 1);
    assert_eq!(hangar.lamps, [Lamp::Green, Lamp::Red, Lamp::Red, Lamp::Red, Lamp::Red]);
    assert_eq!(hangar.next_lamp, Some((2, 2 * 1_440 - 39 * 60)), "lamp 2 turns green after 48 minutes");
}

/// What a player saw at the hangar on 24 Oct 2026, 23:26:20 local time: four of five lamps green. The start of
/// the page and the moment give 6021 seconds into the cycle, so the hangar opens in 1179 seconds (23:46).
#[test]
fn four_of_five_lamps_green_at_23_26_is_what_the_cycle_says() {
    let hangar = hangar_at(START, 1_792_877_180.0);
    assert_eq!(hangar.phase, Phase::Closed);
    assert_eq!(hangar.phase_elapsed, 6_021);
    assert_eq!(hangar.green, 4);
    assert_eq!(hangar.lamps, [Lamp::Green, Lamp::Green, Lamp::Green, Lamp::Green, Lamp::Red]);
    assert_eq!(hangar.to_change, 1_179, "opens in about 19.7 minutes");
    assert_eq!(hangar.next_lamp, Some((5, 1_179)), "the fifth lamp turns green when it opens");
    assert_eq!(next_openings(START, 1_792_877_180.0, 1)[0].opens, 1_792_877_180 + 1_179);
}

#[test]
fn the_next_lamp_and_the_progress_of_the_phase_follow_the_cycle() {
    let closed = at(3_000);
    assert_eq!((closed.green, closed.next_lamp), (2, Some((3, 3 * 1_440 - 3_000))));
    assert_eq!((closed.phase_elapsed, closed.phase_length), (3_000, CLOSED));
    assert!((closed.progress() - 3_000.0 / 7_200.0).abs() < 1e-6);
    assert_eq!(at(7_199).next_lamp, Some((5, 1)), "the fifth lamp turns green when the hangar opens");
    let open = at(7_200 + 1_500);
    assert_eq!((open.green, open.next_lamp), (3, Some((3, 3 * 720 - 1_500))));
    assert!((open.progress() - 1_500.0 / 3_600.0).abs() < 1e-6);
    assert_eq!(at(7_200).green, 5);
    assert_eq!(at(10_799).next_lamp, Some((5, 1)));
    let reset = at(10_900);
    assert_eq!((reset.green, reset.next_lamp), (0, None));
    assert!((reset.progress() - 100.0 / 300.0).abs() < 1e-6);
    assert_eq!(at(0).progress(), 0.0);
}

#[test]
fn the_button_hangar_just_opened_sets_the_start_so_that_it_opens_now() {
    let now = START + 12_345;
    let start = start_just_opened(now);
    assert!(plausible_start(start));
    let hangar = hangar_at(start, now as f64);
    assert_eq!((hangar.phase, hangar.to_change, hangar.green), (Phase::Open, OPEN, LAMPS));
    // A second on, it is still the first lamp of the open time; a second before, it was closed.
    assert_eq!(hangar_at(start, (now + 1) as f64).to_change, OPEN - 1);
    assert_eq!(hangar_at(start, (now - 1) as f64).phase, Phase::Closed);
    // The list names the moment as the next opening of the following cycle.
    assert_eq!(next_openings(start, now as f64, 1)[0].opens, now + CYCLE);
}

#[test]
fn the_button_hangar_just_resets_sets_the_start_so_that_it_resets_now() {
    let now = START + 12_345;
    let start = start_just_reset(now);
    let hangar = hangar_at(start, now as f64);
    assert_eq!((hangar.phase, hangar.to_change, hangar.green), (Phase::Resetting, RESET, 0));
    assert_eq!(hangar_at(start, (now - 1) as f64).phase, Phase::Open);
    assert_eq!(next_openings(start, now as f64, 1)[0].opens, now + RESET + CLOSED, "two hours after the reset is over");
}

#[test]
fn the_source_of_the_start_is_the_hand_before_the_page() {
    assert_eq!(Kept::default().source(), None);
    assert_eq!(fresh(Some(START), 1.0, None).source(), Some(StartSource::Page));
    assert_eq!(fresh(Some(START), 1.0, Some(START + 5)).source(), Some(StartSource::Hand));
    assert_eq!(fresh(None, 0.0, Some(START)).source(), Some(StartSource::Hand));
}

#[test]
fn the_line_of_the_overlay_has_the_phase_the_lamps_and_the_countdown_of_the_page() {
    use crate::language::{Choice, set_thread_language};
    set_thread_language(Choice::De);
    // 14 minutes 39 seconds before the hangar opens, four lamps green.
    let now = (START + CLOSED - (14 * 60 + 39)) as f64;
    let line = overlay_line(Some(START), now);
    assert_eq!(line.label, "EXHANG");
    assert_eq!(line.phase, Some(Phase::Closed));
    assert_eq!(line.text, "öffnet in 14:39");
    let page = hangar_at(START, now);
    assert_eq!(line.lamps, page.lamps, "the same calculation as the page");
    assert_eq!(line.lamps.iter().filter(|lamp| **lamp == Lamp::Green).count(), 4);
    let open = overlay_line(Some(START), (START + CLOSED + 600) as f64);
    assert_eq!((open.phase, open.text.as_str()), (Some(Phase::Open), "schließt in 50:00"));
    let reset = overlay_line(Some(START), SEEN_AT);
    assert_eq!((reset.phase, reset.text.as_str()), (Some(Phase::Resetting), "setzt zurück in 04:35"));
    let none = overlay_line(None, now);
    assert_eq!((none.phase, none.text.as_str(), none.lamps), (None, "keine Startzeit", [Lamp::Dark; LAMPS]));
    set_thread_language(Choice::En);
    assert_eq!(overlay_line(Some(START), now).text, "opens in 14:39");
    set_thread_language(Choice::Auto);
}

#[test]
fn the_overlay_reads_the_switch_of_the_page_and_it_is_off_unless_set() {
    use crate::overlay::{Settings, keys};
    let (paths, dir) = paths_in("overlay", true);
    assert!(!Settings::read(&paths).hangar_timer, "off from the start");
    assert!(paths.set_setting(keys::HANGAR_TIMER, Value::Bool(true)));
    assert!(Settings::read(&paths).hangar_timer);
    assert!(paths.set_setting(keys::HANGAR_TIMER, Value::Bool(false)));
    assert!(!Settings::read(&paths).hangar_timer);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn days_are_told_from_today_and_countdowns_are_written_short() {
    let now = START;
    assert_eq!(day(Lang::De, now + 600, now), Some(Day::Today));
    assert_eq!(day(Lang::De, now + 86_400, now), Some(Day::Tomorrow));
    let Some(Day::Other(later)) = day(Lang::De, now + 3 * 86_400, now) else { panic!("a later day") };
    assert_eq!((later.len(), &later[2..3]), (6, "."), "{later}");
    let Some(Day::Other(later)) = day(Lang::En, now + 3 * 86_400, now) else { panic!("a later day") };
    assert_eq!(&later[2..3], "-", "{later}");
    assert_eq!(civil_days(1970, 1, 1), 0);
    assert_eq!(civil_days(2026, 3, 1) - civil_days(2026, 2, 28), 1);
    assert_eq!(civil_days(2028, 3, 1) - civil_days(2028, 2, 28), 2, "a leap year");
    assert_eq!(countdown(977), "16:17");
    assert_eq!(countdown(3_599), "59:59");
    assert_eq!(countdown(3_600), "1:00:00");
    assert_eq!(countdown(7_199), "1:59:59");
    assert_eq!(countdown(-3), "00:00");
}
