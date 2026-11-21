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

use egui::accesskit::Role;
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use versekit_core::language::{Choice, set_thread_language};
use versekit_core::theme;

use super::*;

fn finder() -> Finder {
    let gain = |who: &str, high: i64| Gain { who: who.into(), kind: "Standing".into(), low: high, high, unknown: false };
    Finder {
        contracts: vec![
            Contract {
                key: "alpha_title".into(),
                title: Some("Alpha Run".into()),
                giver: Some("Bit Zeros".into()),
                gains: vec![gain("Bit Zeros", 100)],
                cooldown: Some((45, 45)),
                cooldown_spread: Some(10),
                shareable: Some(true),
                region: Some("Stanton".into()),
                systems: vec!["Stanton".into()],
                rank: Some(Rank { name: "Contractor".into(), reputation: 2200 }),
                blueprints: Some(vec!["Arclight Pistol".into()]),
                ..Contract::default()
            },
            Contract {
                key: "beta_title".into(),
                title: Some("Beta Haul".into()),
                giver: Some("Headhunters".into()),
                gains: vec![gain("Headhunters", 300)],
                cooldown: Some((2880, 2880)),
                shareable: Some(false),
                rank: Some(Rank { name: "Neutral".into(), reputation: 0 }),
                blueprints: Some(Vec::new()),
                ..Contract::default()
            },
            Contract { key: "gamma_title".into(), ..Contract::default() },
        ],
        version: "4.10.3-live.1".into(),
    }
}

fn harness(state: ContractFinderState) -> Harness<'static, ContractFinderState> {
    let mut harness = Harness::builder()
        .with_size(vec2(1100.0, 2000.0))
        .with_context_setup(crate::install)
        .with_accessibility_check(false)
        .allow_missing_glyphs()
        .build_ui_state(|ui, state: &mut ContractFinderState| show_page(ui, theme::current(), state), state);
    harness.run_steps(3);
    harness
}

fn order(harness: &mut Harness<'static, ContractFinderState>) -> Vec<String> {
    harness.state_mut().arranged().into_iter().map(|c| c.key.clone()).collect()
}

/// A value the data do not state reads "unknown", in both languages; the times read in minutes, hours and days.
#[test]
fn the_values_of_a_contract_read_unknown_where_the_data_state_none() {
    set_thread_language(Choice::De);
    let all = finder().contracts;
    assert_eq!(
        facts(&all[0]),
        [
            "Bit Zeros +100 Standing",
            "45 Min (±10 Min)",
            "ja",
            "Stanton",
            "ab Contractor (2.200 Ruf)",
            "Arclight Pistol"
        ]
    );
    assert_eq!(facts(&all[1])[1..3], ["2 Tage".to_string(), "nein".to_string()]);
    assert_eq!(facts(&all[1])[5], "keiner");
    assert_eq!(facts(&all[2]), ["unbekannt"; 6]);
    let span = Contract {
        cooldown: Some((30, 90)),
        gains: vec![Gain { who: "X".into(), kind: String::new(), low: -50, high: 20, unknown: false }],
        ..Contract::default()
    };
    assert_eq!(facts(&span)[..2], ["X -50 … +20".to_string(), "30 Min – 1 Std 30 Min".to_string()]);
    let loss = Contract {
        gains: vec![Gain { who: "X".into(), kind: String::new(), low: -100, high: -50, unknown: false }],
        ..Contract::default()
    };
    assert_eq!(facts(&loss)[0], "X -100 … -50", "both ends keep their sign");
    let gain_only = Contract {
        gains: vec![Gain { who: "X".into(), kind: String::new(), low: 20, high: 50, unknown: false }],
        ..Contract::default()
    };
    assert_eq!(facts(&gain_only)[0], "X +20 … +50", "a positive low end carries its plus too");
    // A gain whose amount the data do not state is shown as such, in both languages, never left out.
    let not_known = Contract {
        gains: vec![
            Gain { who: "Shubin".into(), kind: "Standing".into(), low: 0, high: 0, unknown: true },
            Gain { who: "X".into(), kind: String::new(), low: 20, high: 20, unknown: false },
        ],
        ..Contract::default()
    };
    assert_eq!(facts(&not_known)[0], "Shubin (Menge unbekannt) Standing, X +20");
    set_thread_language(Choice::En);
    assert_eq!(facts(&not_known)[0], "Shubin (amount unknown) Standing, X +20");
    set_thread_language(Choice::De);
    let capped = Contract {
        rank: Some(Rank { name: "Contractor".into(), reputation: 2200 }),
        rank_max: Some(Rank { name: "Elite Contractor".into(), reputation: 95250 }),
        ..Contract::default()
    };
    assert_eq!(facts(&capped)[4], "ab Contractor bis Elite Contractor (2.200 bis 95.250 Ruf)");
    set_thread_language(Choice::En);
    assert_eq!(facts(&all[2])[0], "unknown");
    assert_eq!(duration(60), "1 h");
    assert_eq!(duration(1440), "1 day");
    set_thread_language(Choice::Auto);
}

/// The page lists every contract with its giver and values, counts them, filters by a switch and the text and sorts
/// by a click on a head (egui_kittest, no window).
#[test]
fn the_page_lists_filters_and_sorts_the_contracts() {
    set_thread_language(Choice::De);
    theme::set_thread_current("krt");
    let mut harness = harness(ContractFinderState::of(finder()));
    assert!(harness.query_by_label("Alpha Run").is_some(), "a title");
    assert!(harness.query_by_label("gamma_title").is_some(), "the key where no title is known");
    assert!(harness.query_by_label("Bit Zeros").is_some(), "a giver under its head");
    assert!(harness.query_all_by_label("unbekannt").count() > 0, "a value the data do not state");
    assert!(harness.query_by_label_contains("ab Contractor (2.200 Ruf)").is_some());
    assert!(harness.query_by_label("3 von 3 Aufträgen").is_some(), "the count");
    assert!(harness.query_by_label("Stand der Spieldaten: 4.10.3-live.1").is_some());
    assert_eq!(order(&mut harness), ["alpha_title", "beta_title", "gamma_title"], "by title at first");

    // A head sorts; a second click turns the direction, the contract without a gain stays last.
    harness.get_by_label("Ruf").click();
    harness.run_steps(2);
    assert_eq!(order(&mut harness), ["alpha_title", "beta_title", "gamma_title"]);
    harness.get_by_label("Ruf").click();
    harness.run_steps(2);
    assert_eq!(order(&mut harness), ["beta_title", "alpha_title", "gamma_title"]);

    // Only contracts with a blueprint: the empty list of Beta is none.
    harness.get_by_role_and_label(Role::CheckBox, "Nur Aufträge mit Bauplan").click();
    harness.run_steps(2);
    assert!(harness.state().view.with_blueprint);
    assert_eq!(order(&mut harness), ["alpha_title"]);
    assert!(harness.query_by_label("1 von 3 Aufträgen").is_some());
    assert!(harness.query_by_label("Beta Haul").is_none(), "a contract filtered out is not drawn");

    // The reset (a button of the page head) brings every contract back; the text finds a blueprint.
    assert!(harness.state().is_narrowed());
    harness.state_mut().reset();
    assert!(!harness.state().is_narrowed());
    harness.run_steps(2);
    assert_eq!(order(&mut harness).len(), 3);
    let field = harness.get_by_role(Role::TextInput);
    field.click();
    harness.run_steps(1);
    harness.get_by_role(Role::TextInput).type_text("arclight");
    harness.run_steps(2);
    assert_eq!(harness.state().view.text, "arclight");
    assert_eq!(order(&mut harness), ["alpha_title"]);
    assert!(harness.query_by_label("1 von 3 Aufträgen").is_some());
    set_thread_language(Choice::Auto);
}

/// The rank chosen shows what a holder of it can take, says how many contracts it leaves out for want of a known rank,
/// and a list read anew keeps the choice where it still offers it.
#[test]
fn a_rank_chosen_shows_what_its_holder_can_take() {
    set_thread_language(Choice::De);
    theme::set_thread_current("krt");
    let mut harness = harness(ContractFinderState::of(finder()));
    {
        let state = harness.state_mut();
        assert_eq!(state.choices.ranks.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["Neutral", "Contractor"]);
        state.filters[2] = 1;
        state.take_choices();
        state.rearrange();
    }
    harness.run_steps(2);
    assert_eq!(order(&mut harness), ["beta_title"], "Neutral takes only what starts at Neutral");
    assert!(harness.query_by_label("Ohne bekannten Rang ausgeblendet: 1").is_some());
    assert!(harness.query_by_label("1 von 3 Aufträgen").is_some());

    harness.state_mut().filters[2] = 2;
    harness.state_mut().take_choices();
    harness.state_mut().rearrange();
    harness.run_steps(2);
    assert_eq!(order(&mut harness), ["alpha_title", "beta_title"], "Contractor takes both");

    harness.state_mut().take_filters();
    assert_eq!(harness.state().filters[2], 2, "the choice stays through a list read anew");
    set_thread_language(Choice::Auto);
}

/// The words that the wrapping of the text shapes cut in the middle (a row ends with a letter or digit, the next one
/// begins with one), as `first|second`.
fn cut_words(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
    fn of(shape: &egui::epaint::Shape, found: &mut Vec<String>) {
        match shape {
            egui::epaint::Shape::Text(text) => {
                for (at, row) in text.galley.rows.iter().enumerate() {
                    let Some(next) = text.galley.rows.get(at + 1) else { continue };
                    let (Some(last), Some(first)) = (row.row.glyphs.last(), next.row.glyphs.first()) else { continue };
                    if !row.ends_with_newline && last.chr.is_alphanumeric() && first.chr.is_alphanumeric() {
                        found.push(format!("{}|{}", row.row.text(), next.row.text()));
                    }
                }
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|inner| of(inner, found)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    shapes.iter().for_each(|clipped| of(&clipped.shape, &mut found));
    found
}

/// Heads and values stand whole at the least width of the page: no word is broken in the middle in the narrowest
/// window, in both languages, and the least widths of the columns together fit into the room of the list.
#[test]
fn no_word_is_broken_in_the_middle_at_the_least_width() {
    theme::set_thread_current("krt");
    for language in [Choice::De, Choice::En] {
        set_thread_language(language);
        let state = ContractFinderState::of(finder());
        let mut harness = Harness::builder()
            .with_size(vec2(880.0, 700.0))
            .with_context_setup(crate::install)
            .with_accessibility_check(false)
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, (state, least): &mut (ContractFinderState, Vec<f32>)| {
                    let places = state.arranged_places().to_vec();
                    *least = state.least_widths(ui, &places);
                    show_page(ui, theme::current(), state);
                },
                (state, Vec::new()),
            );
        harness.run_steps(4);
        let cut = cut_words(&harness.output().shapes);
        assert!(cut.is_empty(), "{language:?}: {cut:?}");
        let least = harness.state().1.clone();
        assert_eq!(least.len(), COLUMNS.len());
        assert!(least.iter().all(|width| *width > 10.0), "{least:?}");
        // The list is the width of the window less the menu of the real window (about 270) and the margins of the
        // cards; the gaps between the columns stand off it.
        let room = 880.0 - 270.0 - 7.0 * 8.0;
        assert!(least.iter().sum::<f32>() <= room, "{language:?}: the least widths {least:?} do not fit into {room}");
    }
    set_thread_language(Choice::Auto);
}

/// The sorted column keeps room for the arrow of its head, the others do not.
#[test]
fn the_sorted_column_keeps_room_for_its_arrow() {
    set_thread_language(Choice::De);
    theme::set_thread_current("krt");
    let least_by = |sort: Column| {
        let mut state = ContractFinderState::of(finder());
        state.view.sort = sort;
        let mut harness = Harness::builder()
            .with_size(vec2(880.0, 700.0))
            .with_context_setup(crate::install)
            .with_accessibility_check(false)
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, (state, least): &mut (ContractFinderState, Vec<f32>)| {
                    let places = state.arranged_places().to_vec();
                    *least = state.least_widths(ui, &places);
                },
                (state, Vec::new()),
            );
        harness.run_steps(2);
        harness.state().1.clone()
    };
    let by_title = least_by(Column::Title);
    let by_gain = least_by(Column::Gain);
    assert!(by_title[0] > by_gain[0], "the title keeps room for the arrow only while it is sorted");
    assert!(by_gain[2] >= by_title[2], "the reputation column has the arrow room when sorted");
    // `column_widths` hands the room left over on by the shares, and never gives less than the least.
    let widths = column_widths(1000.0, &by_title);
    assert!(widths.iter().zip(&by_title).all(|(width, least)| width >= least));
    assert!((widths.iter().sum::<f32>() - 1000.0).abs() < 0.5, "{widths:?}");
    let narrow = column_widths(10.0, &by_title);
    assert_eq!(narrow, by_title, "where the room does not suffice every column keeps its least width");
    set_thread_language(Choice::Auto);
}

/// Without data the page says where they come from, and shows no filter.
#[test]
fn without_data_the_page_says_where_they_come_from() {
    set_thread_language(Choice::De);
    theme::set_thread_current("krt");
    let harness = harness(ContractFinderState::of(Finder::default()));
    assert!(harness.query_by_label("Noch keine Auftragsdaten.").is_some());
    assert!(harness.query_by_role_and_label(Role::CheckBox, "Nur Aufträge mit Bauplan").is_none());
    set_thread_language(Choice::Auto);
}

/// The preview shows its made-up contracts, nothing read.
#[test]
fn the_preview_has_contracts_of_its_own() {
    let mut state = ContractFinderState::sample();
    state.refresh(100.0);
    assert!(state.contracts().len() >= 3);
    assert!(state.contracts().iter().any(|c| c.title.is_none()), "one shows what unknown looks like");
}
