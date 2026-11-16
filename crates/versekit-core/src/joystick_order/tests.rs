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

//! The tests of the order of the sticks: made-up `actionmaps.xml` of two and three sticks, shifted, unchanged,
//! with a stick missing, with identical sticks. Nothing real is touched: every file lies in a throwaway home.

use super::*;
use crate::game_start;
use crate::joysticks::bindings_from_text;
use crate::paths::{ENV_INSTALL_DIR, Host};
use crate::pypath::Flavor;

const A: &str = "1111AAAA-0000-0000-0000-504944564944";
const B: &str = "2222BBBB-0000-0000-0000-504944564944";
const C: &str = "3333CCCC-0000-0000-0000-504944564944";

/// A binding with the given heads `(number, name, id)` and lines `(action, input)`.
fn maps(heads: &[(i64, &str, &str)], lines: &[(&str, &str)]) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- kept -->\n<ActionMaps version=\"1\">\n <ActionProfiles version=\"1\" profileName=\"default\">\n",
    );
    for (number, name, id) in heads {
        out.push_str(&format!(
            "  <options type=\"joystick\" instance=\"{number}\" Product=\" {name}  {{{id}}}\">\n   <flight_move_pitch invert=\"1\"/>\n  </options>\n"
        ));
    }
    out.push_str("  <actionmap name=\"spaceship_general\">\n");
    for (action, input) in lines {
        out.push_str(&format!("   <action name=\"{action}\">\n    <rebind input=\"{input}\"/>\n   </action>\n"));
    }
    out.push_str("  </actionmap>\n </ActionProfiles>\n</ActionMaps>\n");
    out
}

fn order(slots: &[(&str, &str, i64)]) -> Remembered {
    Remembered {
        stand: "2026-11-14 01:00".into(),
        slots: slots.iter().map(|(id, name, number)| Slot { id: (*id).into(), name: (*name).into(), number: *number }).collect(),
    }
}

/// The action that lies on `input` of the device with this id, looked up the way the game would: head first.
fn owner(text: &str, action: &str) -> Option<String> {
    let heads = joysticks::assignment_from_text(text);
    for (device, list) in bindings_from_text(text) {
        if list.iter().any(|b| b.action == action && b.kind == joysticks::Kind::Joystick) {
            let number: i64 = device.trim_start_matches("js").parse().ok()?;
            return heads.iter().find(|h| h.number == number).map(|h| h.id.clone());
        }
    }
    None
}

#[test]
fn two_sticks_that_changed_places_are_found_and_every_action_goes_back_to_its_stick() {
    // Remembered: A is js1, B is js2. Now the heads say the opposite; the lines were made for the old order.
    let text = maps(
        &[(2, "Stick L", A), (1, "Stick R", B)],
        &[("v_eject", "js1_button5"), ("v_boost", "js2_button1"), ("v_pitch", "js1_y")],
    );
    let remembered = order(&[(A, "Stick L", 1), (B, "Stick R", 2)]);
    let plan = plan_from_text(&remembered, &text).expect("well formed");
    assert!(plan.is_shifted() && !plan.conflict);
    assert_eq!(
        plan.shifts.iter().map(|s| (s.id.as_str(), s.from, s.to, s.bindings)).collect::<Vec<_>>(),
        [(A, 1, 2, 2), (B, 2, 1, 1)],
        "A: 1 to 2 with two lines, B: 2 to 1 with one line"
    );
    assert_eq!(plan.lines(), 3);
    // Before: js1_button5 hangs on the head with number 1, which is B now: wrong.
    assert_eq!(owner(&text, "v_eject").as_deref(), Some(B));
    let (new, count) = rewrite_text(&text, &plan.shifts).expect("rewritten");
    assert_eq!(count, 3);
    // After: the action that was on A is on A again, and the other way round, in one pass (no turning back).
    assert_eq!(owner(&new, "v_eject").as_deref(), Some(A));
    assert_eq!(owner(&new, "v_pitch").as_deref(), Some(A));
    assert_eq!(owner(&new, "v_boost").as_deref(), Some(B));
    assert!(new.contains("js2_button5") && new.contains("js2_y") && new.contains("js1_button1"), "{new}");
    // Nothing but the three numbers changed: heads, settings of the sticks and the comment stay.
    let back = new.replace("js2_button5", "js1_button5").replace("js2_y", "js1_y").replace("js1_button1", "js2_button1");
    assert_eq!(back, text);
}

#[test]
fn an_order_that_is_as_remembered_asks_for_nothing() {
    let text = maps(&[(1, "Stick L", A), (2, "Stick R", B)], &[("v_eject", "js1_button5")]);
    let remembered = order(&[(A, "Stick L", 1), (B, "Stick R", 2)]);
    let plan = plan_from_text(&remembered, &text).expect("well formed");
    assert_eq!(plan, Plan::default());
    assert!(matches!(rewrite_text(&text, &plan.shifts), Err(Failure::Same)));
}

#[test]
fn of_three_sticks_only_the_two_that_changed_places_are_moved() {
    let text = maps(
        &[(1, "Stick L", A), (3, "Stick R", B), (2, "Pedals", C)],
        &[("a_one", "js1_x"), ("a_two", "js2_x"), ("a_three", "js3_x")],
    );
    let remembered = order(&[(A, "Stick L", 1), (B, "Stick R", 2), (C, "Pedals", 3)]);
    let plan = plan_from_text(&remembered, &text).expect("well formed");
    assert_eq!(plan.shifts.iter().map(|s| (s.id.as_str(), s.from, s.to)).collect::<Vec<_>>(), [(B, 2, 3), (C, 3, 2)]);
    let (new, count) = rewrite_text(&text, &plan.shifts).expect("rewritten");
    assert_eq!(count, 2);
    assert!(new.contains("js1_x") && new.contains("name=\"a_two\">\n    <rebind input=\"js3_x\""), "{new}");
    assert_eq!(owner(&new, "a_two").as_deref(), Some(B));
    assert_eq!(owner(&new, "a_three").as_deref(), Some(C));
    assert_eq!(owner(&new, "a_one").as_deref(), Some(A));
}

#[test]
fn a_chain_into_a_free_number_is_fine_and_a_number_that_carries_other_lines_is_refused() {
    // A: 1 to 2, B: 2 to 3, and nothing on js3: a clean move.
    let free = maps(&[(2, "Stick L", A), (3, "Stick R", B)], &[("a", "js1_x"), ("b", "js2_x")]);
    let remembered = order(&[(A, "Stick L", 1), (B, "Stick R", 2)]);
    let plan = plan_from_text(&remembered, &free).expect("well formed");
    assert!(!plan.conflict);
    let (new, _) = rewrite_text(&free, &plan.shifts).expect("rewritten");
    assert!(new.contains("input=\"js2_x\"") && new.contains("input=\"js3_x\""));
    // The same, but lines of something else sit on js3 already: they would be mixed with those of B.
    let taken = maps(&[(2, "Stick L", A), (3, "Stick R", B)], &[("a", "js1_x"), ("b", "js2_x"), ("c", "js3_x")]);
    let plan = plan_from_text(&remembered, &taken).expect("well formed");
    assert!(plan.conflict, "{plan:?}");
}

#[test]
fn a_stick_that_is_missing_is_named_and_does_not_hold_up_the_others() {
    let text = maps(&[(2, "Stick L", A), (1, "Stick R", B)], &[("a", "js1_x"), ("b", "js2_x"), ("c", "js3_x")]);
    // C is remembered as js3 but has no head in the file now.
    let remembered = order(&[(A, "Stick L", 1), (B, "Stick R", 2), (C, "Pedals", 3)]);
    let plan = plan_from_text(&remembered, &text).expect("well formed");
    assert_eq!(plan.not_found.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), [C]);
    assert_eq!(plan.shifts.len(), 2);
    assert!(!plan.conflict, "the lines of the missing stick stay where they are: nobody takes js3");
    let (new, _) = rewrite_text(&text, &plan.shifts).expect("rewritten");
    assert!(new.contains("input=\"js3_x\""), "the lines of the missing stick are not touched");
}

#[test]
fn identical_sticks_are_not_guessed() {
    // Two heads with one id: the file can not tell which is which.
    let text = maps(&[(1, "Twin", A), (2, "Twin", A), (3, "Pedals", C)], &[("a", "js1_x"), ("b", "js2_x")]);
    let remembered = order(&[(A, "Twin", 1), (C, "Pedals", 3)]);
    let plan = plan_from_text(&remembered, &text).expect("well formed");
    assert_eq!(plan.twins.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), [A]);
    assert!(plan.shifts.is_empty(), "{plan:?}");
    let captured = capture_text(&text);
    assert_eq!(captured.slots.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), [C], "twins are not written down");
}

#[test]
fn two_heads_under_one_number_are_no_order_and_a_broken_file_gives_no_plan() {
    let text = maps(&[(1, "Stick L", A), (1, "Stick R", B)], &[("a", "js1_x")]);
    let plan = plan_from_text(&order(&[(A, "Stick L", 1), (B, "Stick R", 2)]), &text).expect("well formed");
    assert!(plan.conflict);
    assert!(plan_from_text(&order(&[(A, "x", 1)]), "<ActionMaps><options").is_none());
    assert!(matches!(rewrite_text("<a", &[]), Err(Failure::Unreadable)));
}

#[test]
fn only_the_prefix_of_a_stick_is_renumbered_and_nothing_that_looks_like_it() {
    let text = maps(
        &[(2, "Stick L", A), (1, "Stick R", B)],
        &[("a", "js1_button1"), ("b", "kb1_js1_x"), ("c", "gp1_button1"), ("d", "mo1_mouse1"), ("e", "lalt+js2_button3")],
    );
    let plan = plan_from_text(&order(&[(A, "Stick L", 1), (B, "Stick R", 2)]), &text).expect("well formed");
    let (new, count) = rewrite_text(&text, &plan.shifts).expect("rewritten");
    assert_eq!(count, 2, "the keyboard line with a name that contains js1_ is no stick: {new}");
    assert!(new.contains("input=\"js2_button1\"") && new.contains("input=\"lalt+js1_button3\""));
    assert!(new.contains("input=\"kb1_js1_x\"") && new.contains("input=\"gp1_button1\"") && new.contains("input=\"mo1_mouse1\""));
}

// ------------------------------------------------------------------ over files

fn scratch(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("versekit-joyorder-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("folder");
    dir.to_string_lossy().replace('\\', "/")
}

/// A made-up installation under a throwaway home; the game is quiet for `age` seconds.
fn world(name: &str, age: u64, body: &[u8]) -> (Paths, String) {
    let home = scratch(name);
    let game = format!("{home}/StarCitizen/LIVE");
    let profile = format!("{game}/USER/Client/0/Profiles/default");
    std::fs::create_dir_all(&profile).expect("folder");
    std::fs::write(format!("{profile}/actionmaps.xml"), body).expect("maps");
    std::fs::write(format!("{game}/Game.log"), "x").expect("log");
    let at = std::time::SystemTime::now() - std::time::Duration::from_secs(age);
    std::fs::File::options().write(true).open(format!("{game}/Game.log")).and_then(|f| f.set_modified(at)).expect("age");
    let vars = [
        ("VERSEKIT_HOME", home.as_str()),
        ("HOME", home.as_str()),
        (ENV_INSTALL_DIR, game.as_str()),
        (game_start::ENV_NO_PROCESS, "1"),
    ];
    (Paths::new(Host::new(Flavor::native(), &vars, &home)), format!("{profile}/actionmaps.xml"))
}

fn shifted_text() -> String {
    maps(&[(2, "Stick L", A), (1, "Stick R", B)], &[("v_eject", "js1_button5"), ("v_boost", "js2_button1")])
}

#[test]
fn without_a_memory_nothing_is_corrected_and_the_memory_is_written_down_on_request() {
    let (paths, file) = world("gemerkt", 7200, shifted_text().as_bytes());
    assert!(remembered(&paths).is_none());
    assert_eq!(check(&paths, None, None), Check::default());
    assert!(matches!(correct(&paths, None, None), Err(Failure::OrderNothing)));
    assert_eq!(remember(&paths, None, None).expect("written down"), 2);
    let order = remembered(&paths).expect("remembered");
    assert_eq!(order.slots.iter().map(|s| (s.id.as_str(), s.number)).collect::<Vec<_>>(), [(B, 1), (A, 2)]);
    assert_eq!(check(&paths, None, None).plan, Plan::default(), "what was written down is as it is");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), shifted_text(), "writing down changes no file of the game");
}

#[test]
fn a_correction_writes_a_copy_first_renumbers_the_lines_and_the_memory_follows() {
    let (paths, file) = world("berichtigen", 7200, shifted_text().as_bytes());
    save(&paths, &order(&[(A, "Stick L", 1), (B, "Stick R", 2)])).expect("memory");
    let seen = check(&paths, None, None);
    assert_eq!(seen.plan.shifts.len(), 2, "{seen:?}");

    let applied = correct(&paths, None, None).expect("corrected");
    assert_eq!(applied.done.count, 2);
    let backup = applied.done.backup.clone().expect("a copy");
    assert_eq!(std::fs::read_to_string(&backup).unwrap(), shifted_text(), "the copy holds what was there");
    let now = std::fs::read_to_string(&file).unwrap();
    assert_eq!(owner(&now, "v_eject").as_deref(), Some(A));
    assert_eq!(owner(&now, "v_boost").as_deref(), Some(B));
    let order = remembered(&paths).expect("remembered");
    assert_eq!(order.slots.iter().map(|s| (s.id.as_str(), s.number)).collect::<Vec<_>>(), [(A, 2), (B, 1)]);
    assert_eq!(check(&paths, None, None).plan, Plan::default(), "the next look finds nothing: no turning back");
    assert!(matches!(correct(&paths, None, None), Err(Failure::Same)));
}

#[test]
fn the_way_back_restores_file_and_memory_but_only_while_the_file_is_as_written() {
    let (paths, file) = world("zurueck", 7200, shifted_text().as_bytes());
    let before = order(&[(A, "Stick L", 1), (B, "Stick R", 2)]);
    save(&paths, &before).expect("memory");
    let applied = correct(&paths, None, None).expect("corrected");
    undo(&paths, &applied, None, None).expect("taken back");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), shifted_text());
    assert_eq!(remembered(&paths).expect("remembered").slots, before.slots);
    assert_eq!(check(&paths, None, None).plan.shifts.len(), 2, "shifted again, as before");

    // Corrected again; then somebody else changes the file: the way back must not wipe that.
    let applied = correct(&paths, None, None).expect("corrected");
    let mut changed = std::fs::read_to_string(&file).unwrap();
    changed.push_str("<!-- later -->\n");
    std::fs::write(&file, &changed).expect("changed");
    assert!(matches!(undo(&paths, &applied, None, None), Err(Failure::Changed)));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), changed);
}

#[test]
fn while_the_game_runs_nothing_is_written_not_even_the_memory() {
    let (paths, file) = world("laeuft", 5, shifted_text().as_bytes());
    let before = order(&[(A, "Stick L", 1), (B, "Stick R", 2)]);
    save(&paths, &before).expect("memory");
    let failure = correct(&paths, None, None).expect_err("refused");
    assert!(failure.game_not_quiet(), "{failure:?}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), shifted_text());
    assert_eq!(remembered(&paths).expect("remembered").slots, before.slots);
    assert!(game_files::backups(&file).is_empty());
}

#[test]
fn a_conflict_changes_nothing_and_the_line_ends_and_the_mark_of_the_file_stay() {
    let taken = maps(&[(2, "Stick L", A), (3, "Stick R", B)], &[("a", "js1_x"), ("b", "js2_x"), ("c", "js3_x")]);
    let (paths, file) = world("widerspruch", 7200, taken.as_bytes());
    save(&paths, &order(&[(A, "Stick L", 1), (B, "Stick R", 2)])).expect("memory");
    assert!(matches!(correct(&paths, None, None), Err(Failure::OrderConflict)));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), taken);

    // A file with a mark of the byte order and Windows line ends keeps both.
    let mut bytes = vec![0xef, 0xbb, 0xbf];
    bytes.extend_from_slice(shifted_text().replace('\n', "\r\n").as_bytes());
    let (paths, file) = world("windows", 7200, &bytes);
    save(&paths, &order(&[(A, "Stick L", 1), (B, "Stick R", 2)])).expect("memory");
    correct(&paths, None, None).expect("corrected");
    let after = std::fs::read(&file).unwrap();
    assert_eq!(&after[..3], &[0xef, 0xbb, 0xbf]);
    let text = String::from_utf8(after[3..].to_vec()).unwrap();
    assert!(text.contains("\r\n") && !text.replace("\r\n", "").contains('\n'), "line ends kept");
    assert!(text.contains("js2_button5") && text.contains("js1_button1"));
}

#[test]
fn a_memory_that_is_broken_is_no_memory_and_the_id_is_read_without_regard_to_case() {
    let (paths, _) = world("kaputt", 7200, shifted_text().as_bytes());
    std::fs::write(paths.app_file(FILE), "{kaputt").expect("broken");
    assert!(remembered(&paths).is_none());
    save(&paths, &order(&[(&A.to_lowercase(), "Stick L", 1), (B, "Stick R", 2)])).expect("memory");
    let loaded = remembered(&paths).expect("remembered");
    assert_eq!(loaded.slots[0].id, A, "the id is kept in capital letters");
    assert_eq!(check(&paths, None, None).plan.shifts.len(), 2);
}
