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

use versekit_core::language::{Choice, set_thread_language};

use super::*;

pub(crate) fn snapshot(step: Step, fraction: f64, waiting: bool) -> Snapshot {
    Snapshot { step, step_fraction: None, fraction, elapsed: 187, waiting }
}

#[test]
fn the_time_is_minutes_and_seconds() {
    assert_eq!(elapsed_text(0), "0:00");
    assert_eq!(elapsed_text(59), "0:59");
    assert_eq!(elapsed_text(187), "3:07");
    assert_eq!(elapsed_text(3600), "60:00");
}

#[test]
fn the_line_names_the_step_in_both_languages() {
    let seen = snapshot(Step::Contracts, 0.31, false);
    let mut lines = Vec::new();
    for language in [Choice::De, Choice::En] {
        set_thread_language(language);
        let line = step_line(&seen);
        assert!(line.contains(&t(Step::Contracts.key())), "the step is named: {line}");
        assert!(line.contains('4') && line.contains('9'), "the place in the row: {line}");
        lines.push(line);
    }
    assert_ne!(lines[0], lines[1], "one line per language");
    set_thread_language(Choice::De);
}

/// Every step has a text of its own in both languages, none of them the key itself.
#[test]
fn every_step_has_its_text() {
    for language in [Choice::De, Choice::En] {
        set_thread_language(language);
        let texts: Vec<String> = Step::ALL.iter().map(|step| t(step.key())).collect();
        for (step, text) in Step::ALL.iter().zip(&texts) {
            assert_ne!(text, step.key(), "{step:?} has no text");
        }
        let mut unique = texts.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), texts.len(), "two steps say the same: {texts:?}");
    }
    set_thread_language(Choice::De);
}

#[test]
fn the_foot_bar_shows_work_and_not_a_wait() {
    set_thread_language(Choice::De);
    let (line, done) = foot(&snapshot(Step::Places, 0.42, false));
    assert_eq!(line, step_line(&snapshot(Step::Places, 0.42, false)));
    assert!((done.expect("a bar") - 0.42).abs() < 1e-6);
    let (line, done) = foot(&snapshot(Step::Archive, 0.0, true));
    assert_eq!(line, t("kat_wartet_spiel"));
    assert_eq!(done, None, "waiting for the game is no work: no bar");
}

#[test]
fn the_end_of_a_build_stays_a_few_seconds() {
    let ctx = egui::Context::default();
    crate::install(&ctx);
    let scheme = versekit_core::theme::current();
    let drawn = |at: f64, time: f64, blueprints: Option<usize>| {
        let mut shown = false;
        let input = egui::RawInput { time: Some(time), ..egui::RawInput::default() };
        let _ = ctx.run_ui(input, |ui| {
            shown = ended(ui, scheme, &Ended { blueprints, at });
        });
        shown
    };
    assert!(drawn(10.0, 10.0, Some(540)), "at once");
    assert!(drawn(10.0, 10.0 + ENDED_SECONDS - 1.0, None), "a failure too");
    assert!(!drawn(10.0, 10.0 + ENDED_SECONDS, Some(540)), "then it goes");
    assert!(!drawn(10.0, 9.0, Some(540)), "a clock that went back shows nothing");
}
