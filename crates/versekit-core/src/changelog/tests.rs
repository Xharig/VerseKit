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

use super::*;

fn release(version: &str, date: &str, text: &str) -> Release {
    Release { version: version.into(), date: date.into(), text: text.into(), ..Release::default() }
}

fn lines(points: &[(Kind, String)]) -> Vec<(Kind, &str)> {
    points.iter().map(|(k, l)| (*k, l.as_str())).collect()
}

#[test]
fn a_fenced_block_is_skipped_and_does_not_stick_to_a_point() {
    let text = "### Added\n- first point\n```\n- inside the fence\n  and more\n```\n- second point";
    assert_eq!(lines(&points_by_kind(text)), [(Kind::New, "first point"), (Kind::New, "second point")]);
}

#[test]
fn an_indented_line_without_a_bullet_continues_the_point_above_and_a_sub_point_is_none() {
    let text = "### Fixed\n- a long point\n  that goes on\n    and on\n  - a sub point\n- next";
    assert_eq!(lines(&points_by_kind(text)), [(Kind::Fix, "a long point that goes on and on"), (Kind::Fix, "next")]);
    // A continuation before any point has nothing to stick to.
    assert!(points_by_kind("  lonely line").is_empty());
}

#[test]
fn a_heading_sets_the_kind_and_an_unknown_one_keeps_the_kind_before() {
    let text = "- without a heading\n### Behoben\n- one\n### Something else\n- two\n## Geändert\n- three\n### Hinzugefügt\n- four";
    assert_eq!(
        lines(&points_by_kind(text)),
        [
            (Kind::New, "without a heading"),
            (Kind::Fix, "one"),
            (Kind::Fix, "two"),
            (Kind::Better, "three"),
            (Kind::New, "four"),
        ]
    );
}

#[test]
fn the_words_of_both_languages_name_the_kinds_and_the_stars_are_bullets_too() {
    for (heading, kind) in [
        ("Fixed", Kind::Fix),
        ("Korrigiert", Kind::Fix),
        ("Changed", Kind::Better),
        ("Verbessert", Kind::Better),
        ("Removed", Kind::Better),
        ("Bedienung", Kind::Better),
        ("Added", Kind::New),
        ("Neu", Kind::New),
    ] {
        let text = format!("### {heading}\n* a point");
        assert_eq!(lines(&points_by_kind(&text)), [(kind, "a point")], "{heading}");
    }
}

#[test]
fn the_lead_is_the_quote_block_and_an_empty_line_ends_it() {
    let text = "intro that is no quote\n> **First** part\n> of `the` lead\n\n> a second block\n- point";
    assert_eq!(intro(text), "First part of the lead");
    assert_eq!(intro("- point\n> late"), "late", "a quote after other lines still counts");
    assert_eq!(intro("no lead"), "");
    assert_eq!(intro("> one\nnot a quote\n> two"), "one", "a line that is no quote ends the block");
}

#[test]
fn marks_that_would_show_as_signs_are_removed_from_a_line() {
    assert_eq!(clean_row("**Bold** and `code` and [a link](https://x.y/z)"), "Bold and code and a link");
    assert_eq!(clean_row("half `fence  "), "half fence");
}

#[test]
fn the_release_text_of_github_is_cut_to_the_language() {
    let text = "English text\n\n---\n<details>\n<summary>Deutsch</summary>\n\nDeutscher Text\n\n</details>";
    assert_eq!(language_part(text, Lang::De), "Deutscher Text");
    assert_eq!(language_part(text, Lang::En), "English text");
    assert_eq!(language_part("plain", Lang::De), "plain", "without the block the text stays whole");
    let only_german = "<details><summary>x</summary>Nur deutsch</details>";
    assert_eq!(language_part(only_german, Lang::En), "Nur deutsch", "a missing part is made up by the other");
}

const LOG: &str = "# Changelog\n\nSome text above.\n\n\
## v5.0.0 - in Arbeit\n> Lead five\n### Added\n- five\n\n\
## v4.12.1 - 2026-10-20\n> Lead of the newest\n### Fixed\n- b\n\n\
## v4.12.0 - 2026-10-19\n### Added\n- a\n\n\
## v4.11.0-rc13 - 2026-10-18\n- rc thirteen\n\n\
## v4.11.0-rc1 - 2026-10-18\n- rc one\n\n\
## v4.10.0-rc2 - 2026-10-17\n- a\n\n\
## v4.10.0 - 2026-10-17\n- ten final\n\n\
## Unveröffentlicht\n- never shown\n\n\
## v4.12.0 - 2026-01-01\n- a double\n";

#[test]
fn the_blocks_of_a_log_are_read_by_version_and_the_unreleased_one_and_a_double_are_left_out() {
    let entries = parse(LOG);
    let names: Vec<_> = entries.iter().map(|e| (e.version.as_str(), e.date.as_str())).collect();
    assert_eq!(
        names,
        [
            ("v5.0.0", ""),
            ("v4.12.1", "2026-10-20"),
            ("v4.12.0", "2026-10-19"),
            ("v4.11.0-rc13", "2026-10-18"),
            ("v4.11.0-rc1", "2026-10-18"),
            ("v4.10.0-rc2", "2026-10-17"),
            ("v4.10.0", "2026-10-17"),
        ]
    );
    assert_eq!(entries[2].text, "### Added\n- a", "the double with the other date is not taken");
    assert!(entries.iter().all(|e| e.built_in));
}

#[test]
fn a_log_with_windows_line_ends_reads_like_one_with_unix_ones() {
    assert_eq!(parse(&LOG.replace('\n', "\r\n")), parse(LOG));
}

#[test]
fn the_history_puts_the_log_before_github_and_sorts_by_version_not_by_number() {
    let releases = [
        release("v4.12.1", "2026-10-21", "GitHub text that loses to the log"),
        release("v4.13.0", "2026-10-22", "- new on GitHub"),
        release("v4.13.0-rc2", "2026-10-21", "- rc two"),
        release("v4.13.0-rc10", "2026-10-21", "- rc ten"),
    ];
    let entries = history_of(LOG, Lang::De, &releases);
    let names: Vec<_> = entries.iter().map(|e| e.version.as_str()).collect();
    assert_eq!(
        names,
        [
            "v5.0.0",
            "v4.13.0",
            "v4.13.0-rc10",
            "v4.13.0-rc2",
            "v4.12.1",
            "v4.12.0",
            "v4.11.0-rc13",
            "v4.11.0-rc1",
            "v4.10.0",
            "v4.10.0-rc2"
        ]
    );
    let github = entries.iter().find(|e| e.version == "v4.13.0").unwrap();
    assert!(!github.built_in && github.text == "- new on GitHub");
    let log = entries.iter().find(|e| e.version == "v4.12.1").unwrap();
    assert_eq!(log.text, "> Lead of the newest\n### Fixed\n- b", "the log has the say");
}

#[test]
fn rc1_and_rc13_are_two_versions() {
    let releases = [release("v4.11.0-rc13", "", "x"), release("v4.11.0-rc1", "", "y")];
    let entries = history_of("", Lang::De, &releases);
    assert_eq!(entries.len(), 2);
}

#[test]
fn a_bilingual_release_text_shows_one_language() {
    let text = "English\n<details><summary>Deutsch</summary>Deutsch hier</details>";
    let releases = [release("v9.0.0", "2026-12-01", text)];
    assert_eq!(history_of("", Lang::De, &releases)[0].text, "Deutsch hier");
    assert_eq!(history_of("", Lang::En, &releases)[0].text, "English");
}

#[test]
fn a_series_becomes_one_entry_with_the_lead_of_the_newest_and_a_finished_version_hides_its_test_versions() {
    let entries = group(history_of(LOG, Lang::De, &[]));
    let names: Vec<_> = entries.iter().map(|e| e.version.as_str()).collect();
    // The test versions of 4.10.0 are gone, the ones of 4.11.0 stay (there is no finished 4.11.0).
    assert_eq!(names, ["v5.0", "v4.12", "v4.11.0-rc13", "v4.11.0-rc1", "v4.10"]);
    let series = &entries[1];
    assert_eq!(series.date, "2026-10-20", "the date of the newest");
    assert!(series.text.starts_with("> Lead of the newest"), "the lead of the newest");
    assert_eq!(
        lines(&points_by_kind(&series.text)),
        [(Kind::Fix, "b"), (Kind::New, "a")],
        "the points of every version stand below"
    );
}

#[test]
fn the_built_in_logs_have_the_same_versions_in_both_languages_and_a_current_block() {
    let german = parse(GERMAN);
    let english = parse(ENGLISH);
    assert!(german.len() > 50, "the German log is built in");
    let versions = |entries: &[Entry]| entries.iter().map(|e| e.version.clone()).collect::<Vec<_>>();
    assert_eq!(versions(&german), versions(&english));
    // The page has something to show for the newest version.
    assert!(!points_by_kind(&german[0].text).is_empty());
    assert!(!history(Lang::En, &[]).is_empty());
}
