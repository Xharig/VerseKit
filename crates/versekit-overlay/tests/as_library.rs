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

//! Another program runs the overlay through the library: its entry is
//! there, and the switches for the main program arrive as written.

use std::path::PathBuf;

use versekit_overlay::Args;

fn parse(words: &[&str]) -> Args {
    Args::parse(words.iter().map(|w| w.to_string()))
}

#[test]
fn another_program_can_run_the_overlay_on_the_users_own_data() {
    let _: fn(Args) -> eframe::Result = versekit_overlay::run;
    assert!(parse(&["--echt"]).real, "the switch is there for a program that decides itself");
    assert!(!parse(&[]).real && !parse(&["--daten", r"D:\Kopie"]).real, "without it nothing is the user's own");
}

#[test]
fn another_program_hands_over_its_main_program() {
    let _: fn() -> eframe::Result = versekit_overlay::main;
    let args = parse(&[
        "--daten",
        r"D:\Kopie",
        "--hauptprogramm",
        r"C:\Werkzeug\haupt.exe",
        "--hauptprogramm-arg",
        "--quelle",
        "--unbekannt",
        "--hauptprogramm-arg",
        "test",
        "--archiv",
        r"C:\Spiel\Data.p4k",
    ]);
    assert_eq!(args.data.as_deref(), Some(r"D:\Kopie"));
    assert_eq!(args.archive.as_deref(), Some(r"C:\Spiel\Data.p4k"));
    assert_eq!(args.main_program, Some(PathBuf::from(r"C:\Werkzeug\haupt.exe")));
    assert_eq!(args.main_args, ["--quelle", "test"], "in order, switch-like values too");
    let plain = parse(&["--daten", r"D:\Kopie"]);
    assert_eq!((plain.main_program, plain.main_args.len()), (None, 0), "without them the preview");
}
