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

//! The matching of names read at the crafting terminal. The readings here are made up (marked `made up` in their
//! names): words as the recognition would give them, no pictures of the game.

use super::*;
use crate::refinery_scan::Reason;

fn entry(name: &str, class: Option<&str>, size: Option<i64>, grade: Option<char>) -> Entry {
    Entry { name: name.into(), class: class.map(str::to_string), size, grade }
}

/// Names of the catalog with their neighbours: colours of one piece, a piece and its coloured kinds, short names,
/// names in quotes and with brackets of their own.
fn names() -> Names {
    let mut names = Names::new();
    for e in [
        entry("Sedulity", Some("Ind"), Some(2), Some('B')),
        entry("GammaMax", Some("Sth"), Some(2), Some('B')),
        entry("ZeroRush", Some("Cmp"), Some(1), Some('B')),
        entry("7CA 'Nargun'", Some("Civ"), Some(3), Some('A')),
        entry("5CA 'Akura'", Some("Civ"), Some(3), Some('C')),
        entry("Antium Helmet", None, Some(1), Some('A')),
        entry("Antium Helmet Jet", None, Some(1), Some('A')),
        entry("Antium Helmet Sand", None, Some(1), Some('A')),
        entry("ADP Legs Blue", None, Some(1), Some('A')),
        entry("ADP Legs Black", None, Some(1), Some('A')),
        entry("ADP Legs Green", None, Some(1), Some('A')),
        entry("Scalpel Sniper Rifle Magazine (12 cap)", None, Some(1), Some('A')),
        entry("ORC-mkV Legs (Modified)", None, Some(1), Some('A')),
        entry("Corundum Ore Pod", None, None, None),
        entry("G-2 Helmet Red", None, Some(1), Some('A')),
        entry("HEX", None, None, None),
        entry("Bolt", None, None, None),
    ] {
        names.add(e);
    }
    names
}

#[test]
fn the_comparison_form_drops_case_accents_and_signs() {
    assert_eq!(plain("7CA 'Nargun'"), "7ca nargun");
    assert_eq!(plain("  Kühler – Größe 2!  "), "kuehler groesse 2");
    assert_eq!(plain("ORC-mkV Legs (Modified)"), "orc mkv legs modified");
    assert_eq!(fold_lookalikes("corundurn 0re"), "corundum ore");
}

#[test]
fn folding_puts_the_mistakable_characters_together() {
    assert_eq!(fold_lookalikes("corundurn"), fold_lookalikes("corundum"));
    assert_eq!(fold_lookalikes("seduiity"), fold_lookalikes("sedulity"));
    assert_eq!(fold_lookalikes("gammamax 5"), fold_lookalikes("gammamax s"));
    assert_ne!(fold_lookalikes("antium helmet jet"), fold_lookalikes("antium helmet"));
    assert_eq!(edit_distance("kitten", "sitting"), 3);
    assert!((likeness("abcdefghij", "abcdefghix") - 0.9).abs() < 1e-9);
}

#[test]
fn the_tag_is_split_off_and_read_with_its_faults() {
    let (name, tag) = split_tag("Sedulity (Ind/2/B)");
    assert_eq!(name, "Sedulity");
    assert_eq!(tag, Some(Tag { class: Some("Ind".into()), size: Some(2), grade: Some('B') }));
    let (_, tag) = split_tag("Sedulity (Ind/Z/8)");
    assert_eq!(tag, Some(Tag { class: Some("Ind".into()), size: Some(2), grade: Some('B') }));
    let (name, tag) = split_tag("Scalpel Sniper Rifle Magazine (12 cap)");
    assert_eq!((name.as_str(), tag), ("Scalpel Sniper Rifle Magazine (12 cap)", None));
    let (_, tag) = split_tag("Antium Helmet (–/1/A)");
    assert_eq!(tag, Some(Tag { class: None, size: Some(1), grade: Some('A') }));
    // Weapons carry only their kind, missiles their seeker and size.
    let (name, tag) = split_tag("S-38 Pistol (Bal)");
    assert_eq!((name.as_str(), tag), ("S-38 Pistol", Some(Tag { class: Some("Bal".into()), size: None, grade: None })));
    let (_, tag) = split_tag("Ignite II Missile (IR2)");
    assert_eq!(tag, Some(Tag { class: Some("IR".into()), size: Some(2), grade: None }));
    assert_eq!(split_tag("Magazine (12 cap)").1, None);
    assert_eq!(split_tag("Legs (Modified)").1, None);
}

/// Cases of the pictures of the game: a weapon with its kind behind it is not its magazine (magazines have no class),
/// a radar with its size is not the variant without one, a kind or a tag read with a fault still counts, and a tag
/// that fits the longer name too decides nothing.
#[test]
fn the_tag_tells_a_name_from_the_longer_ones_where_they_differ() {
    let mut names = Names::new();
    names.add(entry("BR-2 Shotgun", Some("Bal"), Some(3), Some('A')));
    names.add(entry("BR-2 Shotgun Magazine (12 cap)", None, Some(1), Some('A')));
    names.add(entry("BroadSpec", Some("Ind"), Some(2), Some('B')));
    names.add(entry("BroadSpec-Go", Some("Ind"), None, Some('B')));
    names.add(entry("BroadSpec-Max", Some("Ind"), Some(4), Some('B')));
    names.add(entry("PowerBolt", Some("Civ"), Some(1), Some('C')));
    names.add(entry("Sens", Some("Mil"), None, Some('B')));
    names.add(entry("FS-9 LMG", Some("Bal"), Some(4), Some('A')));
    names.add(entry("FS-9 Magazine (75 cap)", None, Some(1), Some('A')));
    names.add(entry("Monde Core", None, Some(1), Some('A')));
    names.add(entry("Monde Core Hiro", None, Some(1), Some('A')));
    for (read, name) in [
        ("BR-2 Shotgun (Bal)", "BR-2 Shotgun"),
        ("BR-2 Shotgun (Bat)", "BR-2 Shotgun"),
        ("BroadSpec (Ind/2/B)", "BroadSpec"),
        ("PowerBolt (civ/l/c)", "PowerBolt"),
        ("PowerBolt tiv/1/C", "PowerBolt"),
        // Size 0 is what the catalog keeps as none; a name in capitals with its tag is a hint.
        ("Sens (Mil/O/B)", "Sens"),
        ("FS-9 LMG (Bat)", "FS-9 LMG"),
    ] {
        let reading = match_line(read, &names);
        assert_eq!((reading.certainty, reading.name.as_deref()), (Certainty::Sure, Some(name)), "{read}: {reading:?}");
    }
    // Without a tag, or with one that fits the longer names as well: asked.
    assert_eq!(match_line("BR-2 Shotgun", &names).certainty, Certainty::Likely);
    assert_eq!(match_line("Monde Core", &names).certainty, Certainty::Likely);
    assert_eq!(match_line("Monde Core (–/1/A)", &names).certainty, Certainty::Likely);
    // A kind that is none of the known ones, or two of them a character away, decides nothing.
    assert_eq!(class_meant("Bat"), "Bal");
    assert_eq!(class_meant("tiv"), "Civ");
    assert_eq!(class_meant("Xyz"), "Xyz");
    assert_eq!(split_tag("PowerBolt tiv/1/O").1.map(|t| (t.size, t.grade)), Some((Some(1), None)));
    assert_eq!(split_tag("Antium Helmet Jet").1, None);
}

#[test]
fn a_weapon_with_its_kind_behind_it_is_sure_and_a_wrong_kind_asks() {
    let mut names = names();
    names.add(entry("S-38 Pistol", Some("Bal"), Some(1), Some('A')));
    let reading = match_line("S-38 Pistol (Bal)", &names);
    assert_eq!((reading.certainty, reading.name.as_deref()), (Certainty::Sure, Some("S-38 Pistol")));
    let reading = match_line("S-38 Pistol (Las)", &names);
    assert_eq!((reading.certainty, reading.tag), (Certainty::Likely, TagCheck::Contradicts));
}

#[test]
fn the_tag_agrees_or_contradicts_the_catalog() {
    let sedulity = entry("Sedulity", Some("Ind"), Some(2), Some('B'));
    let tag = |class: &str, size, grade| Some(Tag { class: Some(class.into()), size: Some(size), grade: Some(grade) });
    assert_eq!(check_tag(tag("Ind", 2, 'B').as_ref(), &sedulity), TagCheck::Agrees);
    assert_eq!(check_tag(tag("ind", 2, 'B').as_ref(), &sedulity), TagCheck::Agrees);
    assert_eq!(check_tag(tag("Ind", 3, 'B').as_ref(), &sedulity), TagCheck::Contradicts);
    assert_eq!(check_tag(tag("Mil", 2, 'B').as_ref(), &sedulity), TagCheck::Contradicts);
    assert_eq!(check_tag(None, &sedulity), TagCheck::Unread);
    // A single part compared is not enough to agree.
    let size_only = Tag { class: None, size: Some(2), grade: None };
    assert_eq!(check_tag(Some(&size_only), &sedulity), TagCheck::Unread);
}

#[test]
fn a_name_read_as_it_is_is_sure() {
    let names = names();
    let reading = match_line("Sedulity (Ind/2/B)", &names);
    assert_eq!(
        (reading.certainty, reading.how, reading.name.as_deref()),
        (Certainty::Sure, How::Exact, Some("Sedulity"))
    );
    assert_eq!(reading.tag, TagCheck::Agrees);
    let reading = match_line("7CA 'Nargun' (Civ/3/A)", &names);
    assert_eq!((reading.certainty, reading.name.as_deref()), (Certainty::Sure, Some("7CA 'Nargun'")));
    let reading = match_line("Scalpel Sniper Rifle Magazine (12 cap)", &names);
    assert_eq!(reading.name.as_deref(), Some("Scalpel Sniper Rifle Magazine (12 cap)"));
    // A symbol in front read as a letter does not matter.
    let reading = match_line("e Antium Helmet Jet", &names);
    assert_eq!((reading.certainty, reading.name.as_deref()), (Certainty::Sure, Some("Antium Helmet Jet")));
    // A name that begins with single characters keeps them.
    let reading = match_line("G-2 Helmet Red", &names);
    assert_eq!((reading.certainty, reading.name.as_deref()), (Certainty::Sure, Some("G-2 Helmet Red")));
    let reading = match_line("G-2 Helmet Rcd", &names);
    assert_eq!((reading.certainty, reading.name.as_deref()), (Certainty::Likely, Some("G-2 Helmet Red")));
}

#[test]
fn a_tag_that_says_otherwise_turns_a_sure_name_into_a_question() {
    let reading = match_line("Sedulity (Mil/3/A)", &names());
    assert_eq!((reading.certainty, reading.name.as_deref()), (Certainty::Likely, Some("Sedulity")));
    assert_eq!(reading.tag, TagCheck::Contradicts);
}

#[test]
fn mistakable_characters_still_give_the_name_sure() {
    let names = names();
    for read in ["SeduIity (Ind/2/B)", "Sedu1ity", "ZeroRu5h", "GarnmaMax (Sth/2/B)"] {
        let reading = match_line(read, &names);
        assert_eq!(reading.certainty, Certainty::Sure, "{read}: {reading:?}");
        assert_eq!(reading.how, How::Lookalike, "{read}");
    }
}

#[test]
fn a_name_with_a_fault_is_only_likely_and_never_sure() {
    let names = names();
    let reading = match_line("Sedulty (Ind/2/B)", &names);
    assert_eq!(
        (reading.certainty, reading.how, reading.name.as_deref()),
        (Certainty::Likely, How::Close, Some("Sedulity"))
    );
    let reading = match_line("Corundum Ore Pcd", &names);
    assert_eq!((reading.certainty, reading.name.as_deref()), (Certainty::Likely, Some("Corundum Ore Pod")));
}

#[test]
fn two_names_alike_to_the_reading_make_it_unclear() {
    // `Blxe` lies one letter from `Blue` and from `Black`… but `Black` is one longer: two letters off.
    let reading = match_line("ADP Legs Blxck", &names());
    assert_eq!((reading.certainty, reading.name.as_deref()), (Certainty::Likely, Some("ADP Legs Black")));
    let reading = match_line("ADP Legs Bl", &names());
    assert!(reading.certainty <= Certainty::Likely, "{reading:?}");
    assert_ne!(reading.certainty, Certainty::Sure);
    // `Antium Helmet Sxnd` and `Antium Helmet Jxt`: a word lost to faults leaves the piece and its kinds alike.
    let reading = match_line("Antium Helmet Xxxx", &names());
    assert_ne!(reading.certainty, Certainty::Sure, "{reading:?}");
}

#[test]
fn a_fault_in_the_last_word_never_gives_the_shorter_name() {
    let reading = match_line("Antium Helmet Jot", &names());
    assert_eq!(reading.certainty, Certainty::Unclear, "{reading:?}");
    let offered: Vec<&str> = reading.candidates.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(offered.first(), Some(&"Antium Helmet Jet"));
    assert!(offered.contains(&"Antium Helmet"));
}

#[test]
fn a_name_with_a_rest_behind_it_is_never_sure() {
    let names = names();
    // The brackets of the tag lost: it is read as the tag still, and agrees.
    let reading = match_line("Sedulity Ind/2/B", &names);
    assert_eq!((reading.certainty, reading.tag), (Certainty::Sure, TagCheck::Agrees), "{reading:?}");
    // A rest that is no tag.
    let reading = match_line("Sedulity Ind 2", &names);
    assert_eq!((reading.certainty, reading.how), (Certainty::Likely, How::WithTag), "{reading:?}");
    let reading = match_line("Bolt foo", &names);
    assert_ne!(reading.certainty, Certainty::Sure);
}

/// A heading of the terminal (`ARMOR` above the pictures of an English game) and a label in the world
/// (`AVALANCHE` on a carried item) were read as blueprints in pictures of the game; the hints are never in capitals.
#[test]
fn a_line_in_capitals_is_never_sure() {
    let mut names = names();
    names.add(entry("ARMOR", Some("Civ"), Some(3), Some('B')));
    names.add(entry("Avalanche", Some("Mil"), Some(2), Some('A')));
    for read in ["ARMOR", "I ARMOR", "AVALANCHE", "HEX"] {
        let reading = match_line(read, &names);
        assert_eq!(reading.certainty, Certainty::Likely, "{read}: {reading:?}");
    }
    assert_eq!(match_line("Avalanche", &names).certainty, Certainty::Sure);
    assert_eq!(match_line("Sedulity (Ind/2/B)", &names).certainty, Certainty::Sure);
    // A hint in capitals carries its tag (`FS-9 LMG (Bal)`): headings and labels never do.
    assert_eq!(match_line("SEDULITY (IND/2/B)", &names).certainty, Certainty::Sure);
    assert_eq!(match_line("ARMOR (Civ/3/B)", &names).certainty, Certainty::Sure);
    assert!(in_capitals("7CA 'NARGUN'") && !in_capitals("7CA 'Nargun'") && !in_capitals("S1"));
}

#[test]
fn short_names_count_only_exactly() {
    let names = names();
    assert_eq!(match_line("Hex", &names).certainty, Certainty::Sure);
    assert_eq!(match_line("HEY", &names).certainty, Certainty::None);
    assert_eq!(match_line("Bo1t", &names).certainty, Certainty::Sure);
    assert_eq!(match_line("Boot", &names).certainty, Certainty::None);
    assert_eq!(match_line("KRAFTWERKE", &names).certainty, Certainty::None);
    assert_eq!(match_line("", &names).certainty, Certainty::None);
}

#[test]
fn names_of_the_language_of_the_game_lead_to_the_catalog_name() {
    let mut names = names();
    let pairs = translation_pairs(
        "item_NameCorundumPod=Corundum Ore Pod\nitem_NameHelm=Antium Helmet\nitem_NameSame=Sedulity\n",
        "item_NameCorundumPod=Korund-Erzbehälter\nitem_NameHelm=Antium-Helm (–/1/A)\nitem_NameSame=Sedulity\n",
    );
    assert_eq!(
        pairs,
        vec![
            ("Antium-Helm (–/1/A)".to_string(), "Antium Helmet".to_string()),
            ("Korund-Erzbehälter".to_string(), "Corundum Ore Pod".to_string()),
        ]
    );
    assert_eq!(names.add_translations(&pairs), 2);
    assert!(!names.add_spelling("Unbekannt", "Not In The Catalog"));
    let reading = match_line("Korund-Erzbehälter", &names);
    assert_eq!((reading.certainty, reading.name.as_deref()), (Certainty::Sure, Some("Corundum Ore Pod")));
    let reading = match_line("Antium-Helm (–/1/A)", &names);
    assert_eq!((reading.certainty, reading.name.as_deref()), (Certainty::Sure, Some("Antium Helmet")));
}

#[test]
fn one_spelling_of_two_blueprints_is_settled_by_the_tag_or_asked() {
    let mut names = Names::new();
    names.add(entry("Bracer", Some("Mil"), Some(1), Some('A')));
    names.add(entry("Bracer Two", Some("Mil"), Some(2), Some('A')));
    assert!(names.add_spelling("Armschiene", "Bracer"));
    assert!(names.add_spelling("Armschiene", "Bracer Two"));
    let reading = match_line("Armschiene", &names);
    assert_eq!(reading.certainty, Certainty::Unclear);
    assert_eq!(reading.candidates.len(), 2);
    let reading = match_line("Armschiene (Mil/2/A)", &names);
    assert_eq!((reading.certainty, reading.name.as_deref()), (Certainty::Sure, Some("Bracer Two")));
}

fn sure(name: &str) -> Reading {
    Reading {
        text: name.into(),
        certainty: Certainty::Sure,
        how: How::Exact,
        name: Some(name.into()),
        candidates: vec![(name.into(), 1.0)],
        tag: TagCheck::Unread,
    }
}

fn likely(name: &str) -> Reading {
    Reading { certainty: Certainty::Likely, how: How::Close, ..sure(name) }
}

#[test]
fn only_two_sure_readings_alike_are_taken() {
    let none = Reading::none("");
    let outcome = combine(&[sure("Sedulity"), sure("Sedulity"), none.clone()]);
    assert_eq!((outcome.certainty, outcome.name.as_deref()), (Certainty::Sure, Some("Sedulity")));
    let outcome = combine(&[sure("Sedulity"), none.clone(), none.clone()]);
    assert_eq!(outcome.certainty, Certainty::Likely);
    let outcome = combine(&[sure("Sedulity"), likely("Sedulity"), none.clone()]);
    assert_eq!(outcome.certainty, Certainty::Likely);
    let outcome = combine(&[sure("Sedulity"), sure("Sedulity"), likely("GammaMax")]);
    assert_eq!(outcome.certainty, Certainty::Unclear);
    assert_eq!(outcome.candidates, ["Sedulity", "GammaMax"]);
    assert_eq!(combine(&[none.clone(), none]).certainty, Certainty::None);
}

#[test]
fn a_name_broken_onto_two_lines_is_not_taken_by_its_first_line() {
    let names = names();
    let lines = with_joined(&["Antium Helmet".to_string(), "Jet".to_string()]);
    let reading = best_of_lines(&lines, &names);
    assert_eq!(
        (reading.certainty, reading.name.as_deref()),
        (Certainty::Sure, Some("Antium Helmet Jet")),
        "{reading:?}"
    );
    // Two whole names on two lines, both sure, make it unclear.
    let lines = with_joined(&["Sedulity".to_string(), "GammaMax".to_string()]);
    assert_eq!(best_of_lines(&lines, &names).certainty, Certainty::Unclear);
}

#[test]
fn a_name_that_begins_longer_ones_is_asked_unless_the_tag_decides() {
    let names = names();
    let reading = match_line("Antium Helmet", &names);
    assert_eq!((reading.certainty, reading.name.as_deref()), (Certainty::Likely, Some("Antium Helmet")));
    let offered: Vec<&str> = reading.candidates.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(offered, ["Antium Helmet", "Antium Helmet Jet", "Antium Helmet Sand"]);
    // The same tag for all of them decides nothing.
    assert_eq!(match_line("Antium Helmet (–/1/A)", &names).certainty, Certainty::Likely);
    let mut names = Names::new();
    names.add(entry("Arrow", Some("Mil"), Some(1), Some('A')));
    names.add(entry("Arrow Head", Some("Mil"), Some(2), Some('A')));
    assert_eq!(names.longer_than("arrow", 0), [1]);
    assert!(names.longer_than("arrow head", 1).is_empty());
    assert_eq!(match_line("Arrow (Mil/1/A)", &names).certainty, Certainty::Sure);
    assert_eq!(match_line("Arrow", &names).certainty, Certainty::Likely);
    assert_eq!(match_line("Arrow Head", &names).certainty, Certainty::Sure);
}

/// A name that longer ones begin with, read whole in every pass and none of the longer ones read: taken without a
/// question. Fewer passes, one pass with a longer name, one that read nothing, or a line in capitals still ask.
#[test]
fn a_name_that_begins_longer_ones_read_whole_in_every_pass_is_sure() {
    let names = names();
    let read = |line: &str| match_line(line, &names);
    let every = vec![read("Antium Helmet"), read("Antium Helmet"), read("Antium Helmet")];
    let outcome = combine(&every);
    assert_eq!((outcome.certainty, outcome.name.as_deref()), (Certainty::Sure, Some("Antium Helmet")));
    assert_eq!(decide(&outcome, |_| false), Action::Add("Antium Helmet".into()));

    assert_eq!(combine(&every[..2]).certainty, Certainty::Likely, "two passes only");
    let longer = vec![read("Antium Helmet"), read("Antium Helmet Jet"), read("Antium Helmet")];
    assert_eq!(combine(&longer).certainty, Certainty::Unclear, "a pass read a longer name");
    let one_empty = vec![read("Antium Helmet"), read("Antium Helmet"), read("")];
    assert_eq!(combine(&one_empty).certainty, Certainty::Likely, "a pass read nothing");
    let capitals = vec![read("ANTIUM HELMET"), read("ANTIUM HELMET"), read("ANTIUM HELMET")];
    assert_ne!(combine(&capitals).certainty, Certainty::Sure, "a line in capitals");
    // A name read close (one letter off) is no whole name.
    let close = vec![read("Antium Helmot"), read("Antium Helmot"), read("Antium Helmot")];
    assert_ne!(combine(&close).certainty, Certainty::Sure, "read with a fault");
}

/// `SO Helix (-/O/B)` read three times at the terminal against the catalog's `S0 Helix` (a zero, no size, grade A):
/// the name is the whole of one blueprint and the size agrees, only the grade letter differs. That is no reason to
/// ask; a different size or class, a line in capitals, a pass that read nothing, or a longer name still do.
#[test]
fn a_whole_name_read_in_every_pass_with_only_the_grade_off_is_sure() {
    let mut names = Names::new();
    names.add(entry("S0 Helix", None, None, Some('A')));
    names.add(entry("Helix I Mining Laser", None, Some(1), Some('A')));
    names.add(entry("Helix II Mining Laser", None, Some(2), Some('A')));
    names.add(entry("Arrow", Some("Mil"), Some(1), Some('A')));
    let read = |line: &str| match_line(line, &names);

    let every = vec![read("SO Helix (-/O/B)"), read("SO Helix (-/O/B)"), read("SO Helix (-/O/B)")];
    let outcome = combine(&every);
    assert_eq!((outcome.certainty, outcome.name.as_deref()), (Certainty::Sure, Some("S0 Helix")), "{every:?}");
    assert_eq!(decide(&outcome, |_| false), Action::Add("S0 Helix".into()));

    // The same reading with the grade of the catalog was sure all along.
    let agreeing = vec![read("SO Helix (-/O/A)"), read("SO Helix (-/O/A)"), read("SO Helix (-/O/A)")];
    assert_eq!(combine(&agreeing).certainty, Certainty::Sure);

    // What still asks.
    assert_eq!(combine(&every[..2]).certainty, Certainty::Likely, "two passes only");
    let one_empty = vec![read("SO Helix (-/O/B)"), read("SO Helix (-/O/B)"), read("")];
    assert_eq!(combine(&one_empty).certainty, Certainty::Likely, "a pass read nothing");
    let size = vec![read("SO Helix (-/2/B)"), read("SO Helix (-/2/B)"), read("SO Helix (-/2/B)")];
    assert_eq!(combine(&size).certainty, Certainty::Likely, "another size");
    let capitals = vec![read("SO HELIX (-/O/B)"), read("SO HELIX (-/O/B)"), read("SO HELIX (-/O/B)")];
    assert_ne!(combine(&capitals).certainty, Certainty::Sure, "a line in capitals");
    let close = vec![read("SO Helox (-/O/B)"), read("SO Helox (-/O/B)"), read("SO Helox (-/O/B)")];
    assert_ne!(combine(&close).certainty, Certainty::Sure, "read with a fault");
    let wrong_class = vec![read("Arrow (Ind/1/B)"), read("Arrow (Ind/1/B)"), read("Arrow (Ind/1/B)")];
    assert_eq!(combine(&wrong_class).certainty, Certainty::Likely, "another class");
    let mixed = vec![read("SO Helix (-/O/B)"), read("SO Helix (-/O/C)"), read("SO Helix (-/O/B)")];
    assert_eq!(combine(&mixed).certainty, Certainty::Likely, "the grade changes from pass to pass");
}

/// Questions wait one behind the other, oldest first; the same candidates twice in a row are asked once; a file of
/// the former kind with one question is read as one.
#[test]
fn the_questions_of_the_scan_wait_in_order() {
    let dir = crate::testdir::Dir::new("bauplan-fragen");
    let home = dir.path().to_string_lossy().into_owned();
    let vars = [(crate::envvars::ENV_HOME, home.as_str()), (crate::envvars::ENV_NO_NET, "1")];
    let paths = crate::paths::Paths::new(crate::paths::Host::new(crate::pypath::Flavor::native(), &vars, &home));
    let (a, b) = (vec!["Arrowhead".to_string()], vec!["Sedulity".to_string(), "Sedulity Mk2".to_string()]);
    let read = Outcome {
        certainty: Certainty::Unclear,
        name: None,
        candidates: b.clone(),
        lines: vec!["Sedulty".into(), "Sedul1ty".into()],
        picture: Some(vec![0x89, b'P', b'N', b'G']),
    };
    let names = |all: Vec<Question>| all.into_iter().map(|q| q.candidates).collect::<Vec<_>>();
    assert!(questions(&paths).is_empty());
    ask(&paths, Question { candidates: a.clone(), at: 10, ..Question::default() }).unwrap();
    ask(&paths, Question { candidates: a.clone(), at: 11, ..Question::default() }).unwrap();
    ask(&paths, Question::of(&read, &b, 20)).unwrap();
    assert_eq!(names(questions(&paths)), [a.clone(), b.clone()]);
    assert_eq!(question(&paths).map(|q| (q.candidates, q.at)), Some((a.clone(), 10)));
    answer(&paths, None).unwrap();
    // The question keeps when it was, what was read and the picture.
    let kept = question(&paths).expect("the second");
    assert_eq!((kept.at, kept.read.as_str()), (20, "Sedulty | Sedul1ty"));
    assert_eq!(kept.picture, read.picture);
    answer(&paths, None).unwrap();
    assert!(questions(&paths).is_empty());
    assert!(!std::path::Path::new(&paths.app_file(QUESTION_FILE)).exists(), "no question, no file");
    std::fs::write(paths.app_file(QUESTION_FILE), r#"{"kandidaten": ["Arrowhead"]}"#).unwrap();
    assert_eq!(names(questions(&paths)), [a]);
}

/// One pass read the name sure, the others read a numeral with a stroke lost and name it among their candidates:
/// sure. A pass that read nothing, or one whose candidates leave it out, keeps the question.
#[test]
fn a_sure_pass_backed_by_unclear_ones_naming_it_is_sure() {
    let mut names = Names::new();
    for name in ["Omnisky III Cannon", "Omnisky IX Cannon", "Omnisky VI Cannon"] {
        names.add(entry(name, Some("Bal"), Some(3), Some('A')));
    }
    let read = |line: &str| match_line(line, &names);
    let sure = read("Omnisky III Cannon");
    assert_eq!(sure.certainty, Certainty::Sure);
    let lost = read("Omnisky II Cannon");
    assert_eq!(lost.certainty, Certainty::Unclear, "{lost:?}");
    assert!(lost.candidates.iter().any(|(c, _)| c == "Omnisky III Cannon"));
    let outcome = combine(&[sure.clone(), lost.clone(), lost.clone()]);
    assert_eq!((outcome.certainty, outcome.name.as_deref()), (Certainty::Sure, Some("Omnisky III Cannon")));
    // A pass that read nothing: asked.
    assert_eq!(combine(&[sure.clone(), lost.clone(), Reading::none("")]).certainty, Certainty::Likely);
    // A pass whose candidates leave the sure name out: asked.
    let other = read("Omnisky VI Cannon");
    assert_ne!(combine(&[sure, lost, other]).certainty, Certainty::Sure);
}

/// The kind of a personal weapon in its hint (`(Las)`, `(Pla)`, `(Ele)`) agrees with a catalog that keeps it as
/// energy, `(Bal)` with ballistic; a kind of the other sort still contradicts.
#[test]
fn the_kind_of_a_personal_weapon_agrees_with_energy_and_ballistic() {
    use crate::catalog_cache::Blueprint;
    let weapon = |name: &str, class: &str| {
        entry_of(&Blueprint { name: name.into(), class: Some(class.into()), size: Some(2), ..Blueprint::default() })
    };
    let mut names = Names::new();
    names.add(weapon("Gallant Rifle", "Energy"));
    names.add(weapon("Karna Rifle", "Energy"));
    names.add(weapon("P8-AR Rifle", "Ballistic"));
    for line in ["Gallant Rifle (Las)", "Karna Rifle (Pla)", "P8-AR Rifle (Bal)"] {
        let reading = match_line(line, &names);
        assert_eq!(reading.certainty, Certainty::Sure, "{line}: {reading:?}");
    }
    assert_ne!(match_line("Gallant Rifle (Bal)", &names).certainty, Certainty::Sure, "ballistic is no energy");
    // Broken before its kind, beside a coloured one that begins alike: the whole line read wins.
    names.add(weapon("Parallax Energy Assault Rifle", "Energy"));
    names.add(weapon("Parallax \"Sunstone\" Energy Assault Rifle", "Energy"));
    let lines = with_joined(&["Parallax Energy Assault Rifle".to_string(), "(Ele)".to_string()]);
    let reading = best_of_lines(&lines, &names);
    assert_eq!((reading.certainty, reading.name.as_deref()), (Certainty::Sure, Some("Parallax Energy Assault Rifle")));
}

/// The crossbows are personal weapons the catalog keeps without a class (`Novian "Nighthunter" Crossbow`, recipe
/// `crossbow_ballistic`): the kind their hint names does not speak against them. A kind on an item that is no
/// personal weapon, or a class that is no kind of weapon, still does.
#[test]
fn the_kind_of_a_crossbow_without_a_class_does_not_contradict() {
    use crate::catalog_cache::Blueprint;
    let item = |name: &str, kind: &str| {
        entry_of(&Blueprint { name: name.into(), kind: kind.into(), size: Some(3), ..Blueprint::default() })
    };
    let mut names = Names::new();
    names.add(item("Novian \"Nighthunter\" Crossbow", "WeaponPersonal"));
    names.add(item("Novian \"Ghostmaker\" Crossbow", "WeaponPersonal"));
    names.add(item("Novian Crossbow", "WeaponPersonal"));
    names.add(item("Arclight Pistol Battery (30 cap)", "WeaponAttachment"));
    for line in ["Novian \"Nighthunter\" Crossbow (Bal)", "Novian \"Nighthunter\" Crossbow (Ele)"] {
        let reading = match_line(line, &names);
        assert_eq!(
            (reading.certainty, reading.name.as_deref()),
            (Certainty::Sure, Some("Novian \"Nighthunter\" Crossbow")),
            "{line}"
        );
    }
    let lines = with_joined(&["Novian \"Nighthunter\"".to_string(), "Crossbow (Bal)".to_string()]);
    assert_eq!(best_of_lines(&lines, &names).certainty, Certainty::Sure, "broken onto two lines");
    assert_ne!(match_line("Novian Crossbow (Ind)", &names).certainty, Certainty::Sure, "no kind of weapon");
    let magazine = match_line("Arclight Pistol Battery (30 cap) (Bal)", &names);
    assert_ne!(magazine.certainty, Certainty::Sure, "a magazine has no kind: {magazine:?}");
}

/// `ADP-mk4 Arms Woodland` read sure in two passes and `ADP Arms Woodland` (the same with `mk4` lost) in the third:
/// the whole name is taken, the recognition does not add words. The other way round, or the whole name sure in one
/// pass only, it is still asked.
#[test]
fn a_pass_that_lost_a_word_does_not_speak_against_the_whole_name() {
    let mut names = Names::new();
    for name in ["ADP-mk4 Arms Woodland", "ADP Arms Woodland", "ADP-mk4 Legs Woodland"] {
        names.add(entry(name, None, Some(1), Some('A')));
    }
    let read = |line: &str| match_line(line, &names);
    let lost = vec![read("ADP-mk4 Arms Woodland"), read("ADP- Arms Woodland"), read("ADP-mk4 Arms Woodland")];
    assert_eq!(lost[1].name.as_deref(), Some("ADP Arms Woodland"), "the pass alone reads the shorter name");
    let outcome = combine(&lost);
    assert_eq!((outcome.certainty, outcome.name.as_deref()), (Certainty::Sure, Some("ADP-mk4 Arms Woodland")));
    assert_eq!(decide(&outcome, |_| false), Action::Add("ADP-mk4 Arms Woodland".into()));

    let once = vec![read("ADP-mk4 Arms Woodland"), read("ADP Arms Woodland"), Reading::none("")];
    assert_ne!(combine(&once).certainty, Certainty::Sure, "the whole name sure in one pass only");
    let added = vec![read("ADP Arms Woodland"), read("ADP-mk4 Arms Woodland"), read("ADP Arms Woodland")];
    assert_eq!(combine(&added).certainty, Certainty::Unclear, "a pass with more words than the others");
    let other = vec![read("ADP-mk4 Arms Woodland"), read("ADP-mk4 Legs Woodland"), read("ADP-mk4 Arms Woodland")];
    assert_eq!(combine(&other).certainty, Certainty::Unclear, "another word is no lost one");
}

/// Hits that share no word (`Comer Scraper Module`, `Lindstrom`) are text around the pointer that is no hint: no
/// question, nothing read. Hits alike (`Monde Arms Hiro`, `Monde Arms Purgatory Camo`), or one of them read sure in
/// two passes, are still asked.
#[test]
fn hits_that_have_nothing_to_do_with_one_another_are_nothing_read() {
    let none = Reading::none("");
    let unrelated = combine(&[sure("Comer Scraper Module"), likely("Lindstrom"), none.clone()]);
    assert_eq!(unrelated.certainty, Certainty::None, "{unrelated:?}");
    assert_eq!(decide(&unrelated, |_| false), Action::NotFound);
    // Both names on two lines of one pass: unclear in that pass, nothing read in the end.
    let mut names = Names::new();
    names.add(entry("Comer Scraper Module", None, Some(1), Some('A')));
    names.add(entry("Lindstrom", None, Some(1), Some('A')));
    let pass = best_of_lines(&with_joined(&["Comer Scraper Module".to_string(), "Lindstrom".to_string()]), &names);
    assert_eq!(pass.certainty, Certainty::Unclear);
    assert_eq!(combine(&[pass.clone(), pass.clone(), pass]).certainty, Certainty::None);

    let alike = combine(&[likely("Monde Arms Hiro"), likely("Monde Arms Purgatory Camo"), none.clone()]);
    assert_eq!(alike.certainty, Certainty::Unclear, "names alike are asked");
    let backed = combine(&[sure("Sedulity"), sure("Sedulity"), likely("GammaMax")]);
    assert_eq!(backed.certainty, Certainty::Unclear, "one read sure in two passes is asked");
    let three = combine(&[likely("Monde Arms Hiro"), likely("Monde Arms Delta Camo"), likely("Lindstrom")]);
    assert_eq!(three.certainty, Certainty::Unclear, "two of three alike are asked");
}

/// Brackets that belong to the name of the catalog (`Singe Cannon (S1)`) are no tag: the name read whole is sure.
#[test]
fn brackets_of_the_name_itself_are_no_tag() {
    let mut names = Names::new();
    for (name, size) in [("Singe Cannon (S1)", 1), ("Singe Cannon (S2)", 2), ("Singe Cannon (S3)", 3)] {
        names.add(entry(name, None, Some(size), None));
    }
    for name in ["Singe Cannon (S1)", "Singe Cannon (S3)"] {
        let reading = match_line(name, &names);
        assert_eq!((reading.certainty, reading.name.as_deref()), (Certainty::Sure, Some(name)));
    }
    // A name with its brackets broken onto a second line is read whole and sure, before the nearest magazines.
    let mut ammo = Names::new();
    for name in ["Skarnov Rifle Battery (30 cap)", "S-38 Magazine (20 cap)", "Scalpel Sniper Rifle Magazine (12 cap)"] {
        ammo.add(entry(name, None, Some(1), None));
    }
    let lines = with_joined(&["Skarnov Rifle Battery (30".to_string(), "cap)".to_string()]);
    let reading = best_of_lines(&lines, &ammo);
    assert_eq!((reading.certainty, reading.name.as_deref()), (Certainty::Sure, Some("Skarnov Rifle Battery (30 cap)")));
    // A tag that is no part of the name is still checked.
    let reading = match_line("Sedulity (Mil/2/B)", &self::names());
    assert_ne!(reading.certainty, Certainty::Sure, "a tag that contradicts");
}

/// A name broken at its hyphen onto a second line is read whole, without a blank in the middle.
#[test]
fn a_name_broken_at_a_hyphen_is_read_whole() {
    let mut names = Names::new();
    names.add(entry("CF-337 Panther \"Hazard-Zone\" Repeater", Some("Bal"), Some(3), Some('A')));
    names.add(entry("CF-337 Panther Repeater", Some("Bal"), Some(3), Some('A')));
    let lines = with_joined(&["CF-337 Panther \"Hazard-".to_string(), "Zone\" Repeater".to_string()]);
    assert!(lines.contains(&"CF-337 Panther \"Hazard-Zone\" Repeater".to_string()), "{lines:?}");
    let reading = best_of_lines(&lines, &names);
    assert_eq!(reading.name.as_deref(), Some("CF-337 Panther \"Hazard-Zone\" Repeater"));
    assert!(reading.certainty >= Certainty::Likely);
    assert_eq!(joined("Antium Helmet", "Jet"), "Antium Helmet Jet", "without a hyphen: a blank");
}

/// The hint beside the pointer wins over a name further away in the part (the heading of the blueprint chosen on
/// the right); two names about as near stay a question.
#[test]
fn the_name_beside_the_pointer_wins_over_one_further_away() {
    let names = names();
    let at = |text: &str, x: f64, y: f64| {
        let mut left = x;
        text.split(' ')
            .map(|part| {
                let word = Word::new(part, left, y, part.chars().count() as f64 * 9.0, 18.0);
                left += word.w + 6.0;
                word
            })
            .collect::<Vec<Word>>()
    };
    let pointer = (1000.0, 520.0);
    // The hint right of the pointer, the name of the chosen one far right and higher up.
    let mut words = at("Sedulity (Ind/2/B)", 1020.0, 515.0);
    words.extend(at("GammaMax", 1900.0, 380.0));
    let reading = nearest_of_lines(&words, pointer, &names);
    assert_eq!((reading.certainty, reading.name.as_deref()), (Certainty::Sure, Some("Sedulity")));
    // Without the rule both would be asked.
    let lines = with_joined(&lines_of(&words));
    assert_eq!(best_of_lines(&lines, &names).certainty, Certainty::Unclear);
    // Two names about as near: still asked.
    let mut close = at("Sedulity (Ind/2/B)", 1020.0, 515.0);
    close.extend(at("GammaMax (Sth/2/B)", 1020.0, 545.0));
    assert_eq!(nearest_of_lines(&close, pointer, &names).certainty, Certainty::Unclear);
}

#[test]
fn words_far_apart_in_one_row_are_two_lines() {
    let words = [
        Word::new("S1", 100.0, 200.0, 20.0, 18.0),
        Word::new("Sedulity", 300.0, 201.0, 80.0, 18.0),
        Word::new("(Ind/2/B)", 386.0, 201.0, 70.0, 18.0),
        Word::new("Kraftwerke", 100.0, 260.0, 90.0, 18.0),
    ];
    assert_eq!(lines_of(&words), ["S1", "Sedulity (Ind/2/B)", "Kraftwerke"]);
}

#[test]
fn the_hint_area_lies_around_the_pointer_and_within_the_screen() {
    assert_eq!(hint_area((1000, 500), (5120, 1440)), (884, 327, 1196, 432));
    let (x, y, w, h) = hint_area((5100, 10), (5120, 1440));
    assert!(x >= 0 && y == 0 && x + w <= 5120 && y + h <= 1440);
}

/// The picture of a question shows only the lines of the name in question with a margin, not the whole part around
/// the pointer: other text read there stays out, a name broken onto two lines stays whole.
#[test]
fn the_picture_of_a_question_shows_the_lines_of_the_name() {
    let size = (2560, 1440);
    let words = [
        Word::new("Kraftwerke", 100.0, 100.0, 90.0, 18.0),
        Word::new("Antium", 1000.0, 500.0, 60.0, 18.0),
        Word::new("Helmet", 1066.0, 500.0, 60.0, 18.0),
        Word::new("Ruestung", 1000.0, 700.0, 80.0, 18.0),
    ];
    let asked = ["Antium Helmet".to_string(), "Antium Helmet Jet".to_string()];
    let margin = NAME_MARGIN * 1440.0;
    let less = |at: f64| (at - margin) as i64;
    let more = |at: f64| (at + margin) as i64;
    let (x, y, w, h) = name_area(&words, &asked, &names(), size).expect("the name was read");
    assert_eq!((x, y), (less(1000.0), less(500.0)));
    assert_eq!((x + w, y + h), (more(1126.0), more(518.0)));

    // Broken onto two lines: both lines are in the picture.
    let broken = [
        Word::new("Scalpel", 1000.0, 500.0, 60.0, 18.0),
        Word::new("Sniper", 1066.0, 500.0, 60.0, 18.0),
        Word::new("Rifle", 1132.0, 500.0, 40.0, 18.0),
        Word::new("Magazine", 1000.0, 522.0, 80.0, 18.0),
        Word::new("(12", 1086.0, 522.0, 30.0, 18.0),
        Word::new("cap)", 1122.0, 522.0, 40.0, 18.0),
    ];
    let asked = ["Scalpel Sniper Rifle Magazine (12 cap)".to_string()];
    let (_, y, _, h) = name_area(&broken, &asked, &names(), size).expect("the name was read");
    assert_eq!((y, y + h), (less(500.0), more(540.0)));

    // No line names a blueprint in question: no part, the window keeps the part around the pointer.
    assert_eq!(name_area(&words, &["Bolt".to_string()], &names(), size), None);
}

/// A made-up recognition: per pass the words it gives, or a failure.
struct MadeUp(Result<Vec<Vec<Word>>, ScanError>, Vec<Job>);

impl Ocr for MadeUp {
    fn read(&mut self, jobs: &[Job]) -> Result<Vec<Vec<Word>>, ScanError> {
        self.1 = jobs.to_vec();
        self.0.clone()
    }
}

fn hint(text: &str) -> Vec<Word> {
    let mut x = 1050.0;
    text.split(' ')
        .map(|part| {
            let word = Word::new(part, x, 520.0, part.chars().count() as f64 * 9.0, 18.0);
            x += word.w + 6.0;
            word
        })
        .collect()
}

#[test]
fn made_up_reading_at_the_pointer_gives_the_name_and_what_to_do() {
    let names = names();
    let mut ocr =
        MadeUp(Ok(vec![hint("Sedulity (Ind/2/B)"), hint("SeduIity (Ind/2/B)"), hint("Sedulty (Ind/2/B)")]), vec![]);
    let outcome = read_at((5120, 1440), (1000, 500), &mut ocr, &names).expect("read");
    assert_eq!(ocr.1.len(), PASSES.len());
    assert!(ocr.1.iter().all(|job| job.area == hint_area((1000, 500), (5120, 1440))));
    assert_eq!((outcome.certainty, outcome.name.as_deref()), (Certainty::Sure, Some("Sedulity")));
    assert_eq!(decide(&outcome, |_| false), Action::Add("Sedulity".into()));
    assert_eq!(decide(&outcome, |n| n == "Sedulity"), Action::AlreadyThere("Sedulity".into()));

    let mut ocr = MadeUp(Ok(vec![hint("Sedulty"), vec![], vec![]]), vec![]);
    let outcome = read_at((5120, 1440), (1000, 500), &mut ocr, &names).expect("read");
    assert_eq!(decide(&outcome, |_| false), Action::Confirm(vec!["Sedulity".into()]));

    let mut ocr = MadeUp(Ok(vec![hint("Kraftwerke"), vec![], vec![]]), vec![]);
    let outcome = read_at((5120, 1440), (1000, 500), &mut ocr, &names).expect("read");
    assert_eq!(decide(&outcome, |_| false), Action::NotFound);

    let mut ocr = MadeUp(Err(ScanError::new(Reason::NoLanguage, "")), vec![]);
    assert_eq!(read_at((5120, 1440), (1000, 500), &mut ocr, &names).map_err(|e| e.reason), Err(Reason::NoLanguage));
}

/// A sure name goes into the collection under the source `scan` once; a second scan finds it there; an unsure one
/// writes nothing. In a made-up home folder, never the player's.
#[test]
fn only_a_sure_name_is_written_into_the_collection_and_only_once() {
    use crate::paths::{Host, Paths};
    use crate::pypath::Flavor;
    let home = std::env::temp_dir().join(format!("versekit-bauplanscan-{}", crate::testdir::run_tag()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("home");
    let home = home.to_string_lossy().into_owned();
    let (appdata, config) = (Flavor::native().join(&home, "appdata"), Flavor::native().join(&home, "config"));
    let vars = [
        (crate::envvars::ENV_HOME, home.as_str()),
        ("APPDATA", appdata.as_str()),
        ("XDG_CONFIG_HOME", config.as_str()),
    ];
    let paths = Paths::new(Host::new(Flavor::native(), &vars, &home));
    let sure = combine(&[sure("Sedulity"), sure("Sedulity")]);
    assert_eq!(record(&paths, &sure), Ok(Action::Add("Sedulity".into())));
    assert_eq!(record(&paths, &sure), Ok(Action::AlreadyThere("Sedulity".into())));
    let unsure = combine(&[likely("GammaMax")]);
    assert_eq!(record(&paths, &unsure), Ok(Action::Confirm(vec!["GammaMax".into()])));
    let collection = crate::collection::Collection {
        paths: &paths,
        catalog: &crate::collection::NoCatalog,
        names: &Untranslated,
        exports: &crate::collection::NoExports,
        clock: crate::collection::now_text,
    };
    let data = collection.load();
    assert!(crate::collection::contains(&data, "Sedulity"));
    assert!(!crate::collection::contains(&data, "GammaMax"), "unsure: not written");
    assert_eq!(crate::collection::by_source(&data), vec![(SOURCE.to_string(), 1)]);
    // The unsure one waits as a question; the player's choice writes it, and the question is gone.
    assert_eq!(question(&paths).map(|q| q.candidates), Some(vec!["GammaMax".to_string()]));
    assert_eq!(answer(&paths, Some("GammaMax")), Ok(()));
    assert!(question(&paths).is_none());
    assert!(crate::collection::contains(&collection.load(), "GammaMax"));
    // None of them: nothing written, the question gone.
    let legs = vec!["ADP Legs Blue".to_string(), "ADP Legs Black".to_string()];
    ask(&paths, Question { candidates: legs, ..Question::default() }).expect("ask");
    assert_eq!(question(&paths).map(|q| q.candidates.len()), Some(2));
    assert_eq!(answer(&paths, None), Ok(()));
    assert!(question(&paths).is_none());
    assert!(!crate::collection::contains(&collection.load(), "ADP Legs Blue"));
    assert_eq!(answer(&paths, None), Ok(()), "no question: nothing to do");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn every_action_has_its_message_in_both_languages() {
    let actions = [
        Action::Add("Sedulity".into()),
        Action::AlreadyThere("Sedulity".into()),
        Action::Confirm(vec!["Sedulity".into()]),
        Action::Confirm(vec!["ADP Legs Blue".into(), "ADP Legs Black".into()]),
        Action::NotFound,
    ];
    for lang in [Lang::De, Lang::En] {
        for action in &actions {
            let text = message_in(lang, action);
            assert!(!text.starts_with("s_bpscan_") && !text.contains('{'), "{lang:?} {action:?}: {text}");
        }
    }
    for lang in [Lang::De, Lang::En] {
        for action in &actions {
            let notice = notice_in(lang, action);
            assert!(!notice.title.starts_with("s_bpscan_"), "{lang:?} {action:?}: {notice:?}");
        }
    }
    use crate::scan_notice::NoticeKind;
    let notice = notice_in(Lang::De, &actions[0]);
    assert_eq!(
        (notice.kind, notice.shown_title().as_str(), notice.detail.as_str()),
        (NoticeKind::Done, "BAUPLAN ERFASST", "Sedulity")
    );
    let notice = notice_in(Lang::En, &actions[1]);
    assert_eq!((notice.kind, notice.title.as_str()), (NoticeKind::AlreadyThere, "Already captured"));
    let notice = notice_in(Lang::En, &actions[3]);
    assert_eq!((notice.kind, notice.detail.as_str()), (NoticeKind::Unsure, "ADP Legs Blue · ADP Legs Black"));
    assert_eq!(notice_in(Lang::De, &Action::NotFound).shown_title(), "KEIN BAUPLAN ERKANNT");
    assert_eq!(message_in(Lang::De, &actions[0]), "Bauplan erfasst: Sedulity");
    assert_eq!(message_in(Lang::En, &actions[3]), "Not clear which blueprint: ADP Legs Blue · ADP Legs Black");
}

#[test]
fn the_tally_counts_a_wrong_sure_name_apart() {
    let mut tally = Tally::default();
    tally.count(&combine(&[sure("Sedulity"), sure("Sedulity")]), Some("Sedulity"));
    tally.count(&combine(&[sure("GammaMax"), sure("GammaMax")]), Some("Sedulity"));
    tally.count(&combine(&[likely("Sedulity")]), Some("Sedulity"));
    tally.count(&combine(&[likely("GammaMax")]), Some("Sedulity"));
    tally.count(&combine(&[Reading::none("")]), Some("Sedulity"));
    tally.count(&combine(&[likely("GammaMax")]), None);
    tally.count(&combine(&[Reading::none("")]), None);
    assert_eq!(tally, Tally { right: 2, wrong: 1, asked_right: 1, asked_wrong: 1, missed: 1, false_alarm: 1 });
    assert_eq!(tally.total(), 7);
    assert!(tally.to_string().contains("1 sure and WRONG"));
}
