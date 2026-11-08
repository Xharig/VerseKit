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

//! The harness draws in the thread that steps it, so the language of that
//! thread decides the texts of a drawn page: two tests in two languages can
//! run side by side.

use std::sync::{Arc, Barrier};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use versekit_core::language::{Choice, Lang, set_thread_language, t, t_in};

/// Draws a label with a text of the language file in a harness and returns what the harness shows.
fn drawn_text(choice: Choice, start: &Barrier) -> String {
    set_thread_language(choice);
    let mut harness = Harness::builder().with_accessibility_check(false).build_ui(|ui| {
        ui.label(t("bauplaene"));
    });
    start.wait();
    for _ in 0..200 {
        harness.step();
    }
    let drawn = t_in(if choice == Choice::De { Lang::De } else { Lang::En }, "bauplaene");
    assert!(harness.query_by_label(&drawn).is_some(), "the label is drawn in the language of its thread");
    drawn
}

#[test]
fn harnesses_in_two_threads_draw_in_their_own_language() {
    let start = Arc::new(Barrier::new(2));
    let german = {
        let start = Arc::clone(&start);
        std::thread::spawn(move || drawn_text(Choice::De, &start))
    };
    let english = {
        let start = Arc::clone(&start);
        std::thread::spawn(move || drawn_text(Choice::En, &start))
    };
    let german = german.join().expect("German harness");
    let english = english.join().expect("English harness");
    assert_ne!(german, english, "the two threads drew different texts");
}
