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

/// The tests of the arithmetic follow the community checker, by name: the model in use has tests of its own below.
fn assess(rock: &Rock, loadout: &Loadout) -> Verdict {
    assess_with(Model::Checker, rock, loadout)
}

fn laser(power: f64) -> Laser {
    Laser { power, min_share: 0.0, head: Factors::NONE, modules: Vec::new() }
}

fn rock(mass: f64, resistance: f64) -> Rock {
    Rock { mass: Some(mass), resistance: Some(resistance), instability: None }
}

fn known(verdict: Verdict) -> Assessment {
    match verdict {
        Verdict::Known(found) => found,
        Verdict::Unknown(why) => panic!("no answer: {why:?}"),
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.01
}

#[test]
fn the_checker_default_rock_needs_1800_and_a_3900_laser_cracks_it_at_half_throttle() {
    let found = known(assess(&rock(5000.0, 0.0), &Loadout { lasers: vec![laser(3900.0)], ..Loadout::default() }));
    assert!(close(found.required, 1800.0));
    assert!(close(found.delivered, 3900.0));
    assert_eq!(found.band, Band::Easy);
    assert_eq!(found.throttle, Some(50));
}

#[test]
fn a_2340_laser_on_the_default_rock_is_130_percent_and_good() {
    let found = known(assess(&rock(5000.0, 0.0), &Loadout { lasers: vec![laser(2340.0)], ..Loadout::default() }));
    assert!(close(found.ratio, 1.3));
    assert_eq!(found.band, Band::Good);
    // The heaviest rock it cracks at 0 % is 6500 kg.
    let edge = known(assess(&rock(6500.0, 0.0), &Loadout { lasers: vec![laser(2340.0)], ..Loadout::default() }));
    assert!(close(edge.ratio, 1.0));
    assert_eq!(edge.band, Band::Tight);
    assert_eq!(edge.throttle, Some(100));
}

#[test]
fn the_older_formula_gives_the_numbers_its_pages_show() {
    let required = |mass: f64, resistance: f64| Model::MassOverFive.powers(mass, resistance, 0.0).0;
    assert!(close(required(50_000.0, 25.0), 13_333.33));
    assert!(close(required(50_000.0, 25.0 * 1.25), 14_545.45));
    assert!(close(required(8000.0, 95.0), 32_000.0));
    assert!(close(required(8500.0, 28.0), 2361.11));
}

#[test]
fn resistance_lowers_the_power_delivered_not_the_power_required() {
    let found = known(assess(&rock(5000.0, 50.0), &Loadout { lasers: vec![laser(3900.0)], ..Loadout::default() }));
    assert!(close(found.required, 1800.0));
    assert!(close(found.delivered, 1950.0));
    assert_eq!(found.band, Band::Tight);
}

#[test]
fn lasers_sum_their_power_and_average_their_resistance_factors() {
    let one = Laser { power: 2000.0, min_share: 0.0, head: Factors { resistance: 0.7, ..Factors::NONE }, modules: Vec::new() };
    let two = Laser { power: 2000.0, min_share: 0.0, head: Factors::NONE, modules: vec![Factors { power: 1.5, ..Factors::NONE }] };
    let loadout = Loadout { lasers: vec![one, two], ..Loadout::default() };
    let all = loadout.combined().unwrap();
    assert!(close(all.power, 5000.0));
    assert!(close(all.resistance, 0.85));
    let found = known(assess(&rock(10_000.0, 40.0), &loadout));
    assert!(close(found.effective_resistance, 34.0));
    assert!(close(found.delivered, 3300.0));
    assert_eq!(found.band, Band::NeedsHelp);
}

#[test]
fn a_gadget_and_an_active_module_multiply_after_the_average() {
    let base = Loadout { lasers: vec![laser(3000.0)], ..Loadout::default() };
    let without = known(assess(&rock(8000.0, 60.0), &base));
    assert_eq!(without.band, Band::MoreLasers);
    let gadget = Loadout { gadget: Some(Factors { resistance: 0.5, ..Factors::NONE }), ..base.clone() };
    let with_gadget = known(assess(&rock(8000.0, 60.0), &gadget));
    assert!(close(with_gadget.effective_resistance, 30.0));
    assert_eq!(with_gadget.band, Band::NeedsHelp);
    let both = Loadout { active: Some(Factors { power: 1.5, ..Factors::NONE }), ..gadget };
    let with_both = known(assess(&rock(8000.0, 60.0), &both));
    assert!(close(with_both.delivered, 3150.0));
    assert_eq!(with_both.band, Band::Tight);
}

#[test]
fn an_unsure_reading_or_no_laser_gives_no_answer() {
    let lasers = Loadout { lasers: vec![laser(3900.0)], ..Loadout::default() };
    let no_mass = Rock { mass: None, ..rock(5000.0, 10.0) };
    assert_eq!(assess(&no_mass, &lasers), Verdict::Unknown(Missing::Reading));
    let no_resistance = Rock { resistance: None, ..rock(5000.0, 10.0) };
    assert_eq!(assess(&no_resistance, &lasers), Verdict::Unknown(Missing::Reading));
    assert_eq!(assess(&rock(5000.0, 120.0), &lasers), Verdict::Unknown(Missing::Reading));
    assert_eq!(assess(&rock(0.0, 10.0), &lasers), Verdict::Unknown(Missing::Reading));
    assert_eq!(assess(&rock(5000.0, 10.0), &Loadout::default()), Verdict::Unknown(Missing::Lasers));
}

#[test]
fn the_effective_resistance_stops_at_98_percent() {
    let strong = Loadout {
        lasers: vec![Laser { power: 100_000.0, min_share: 0.0, head: Factors { resistance: 2.0, ..Factors::NONE }, modules: vec![] }],
        ..Loadout::default()
    };
    let found = known(assess(&rock(1000.0, 90.0), &strong));
    assert!(close(found.effective_resistance, 98.0));
}

#[test]
fn the_bands_split_at_160_115_100_and_70_percent() {
    assert_eq!(Band::of(1.6), Band::Easy);
    assert_eq!(Band::of(1.59), Band::Good);
    assert_eq!(Band::of(1.15), Band::Good);
    assert_eq!(Band::of(1.0), Band::Tight);
    assert_eq!(Band::of(0.99), Band::NeedsHelp);
    assert_eq!(Band::of(0.7), Band::NeedsHelp);
    assert_eq!(Band::of(0.69), Band::MoreLasers);
}

#[test]
fn the_worth_leaves_out_the_inert_rest_and_needs_a_price_for_every_share() {
    let shares = vec![("Quantainium".to_string(), 30.0), ("Inert Materials".to_string(), 70.0)];
    let price = |name: &str| (name == "Quantainium").then_some(1000.0);
    assert!(close(worth(10.0, &shares, price).unwrap(), 3000.0));
    let unknown = vec![("Quantainium".to_string(), 30.0), ("Gold".to_string(), 10.0)];
    assert_eq!(worth(10.0, &unknown, price), None);
    assert_eq!(worth(0.0, &shares, price), None);
}

#[test]
fn the_lines_say_the_answer_in_both_languages() {
    use crate::language::Lang;
    let loadout = Loadout { lasers: vec![laser(3900.0)], ..Loadout::default() };
    let easy = assess(&rock(5000.0, 0.0), &loadout);
    assert_eq!(verdict_line_in(Lang::De, &easy), "Aufbrechbar · leicht · Gas 50 %");
    assert_eq!(verdict_line_in(Lang::En, &easy), "Crackable · easy · throttle 50 %");
    let hard = assess(&rock(50_000.0, 0.0), &loadout);
    assert_eq!(verdict_line_in(Lang::De, &hard), "Nicht aufbrechbar · mehr Laser nötig");
    let unsure = assess(&Rock::default(), &loadout);
    assert_eq!(verdict_line_in(Lang::En, &unsure), "Crackable: scan not read surely");
    assert_eq!(worth_line_in(Lang::De, 12_345.0), "Wert ca. 12.300 aUEC");
    assert_eq!(worth_line_in(Lang::En, 12_355.0), "Worth about 12,400 aUEC");
}

/// The HUD shows 61 % as 42 % in green once a head of factor 0.70 is armed: the rock's own lies between 60 and
/// 61.43 %, and the answer stands only where both ends agree.
#[test]
fn a_resistance_shown_with_the_head_is_taken_back_to_the_rocks_own() {
    let head = |resistance: f64| Laser { power: 3900.0, min_share: 0.0, head: Factors { resistance, ..Factors::NONE }, modules: vec![] };
    let one = Loadout { lasers: vec![head(0.7)], ..Loadout::default() };
    let shown = Rock { resistance: Some(42.0), ..rock(5000.0, 0.0) };
    let Verdict::Known(found) = assess_with_head_in(Model::Checker, &shown, &one) else { panic!("known") };
    let direct = known(assess(&rock(5000.0, 61.43 - 1e-6), &one));
    assert!(close(found.effective_resistance, direct.effective_resistance), "the higher end: {found:?}");
    // At the edge: 2240 W required, 2262 W delivered at 42 % effective, 2223 W at 43 %: tight at one end, short at
    // the other. No answer.
    let edge = Rock { mass: Some(2240.0 / POWER_PER_KG), ..shown };
    assert_eq!(known(assess(&Rock { resistance: Some(60.0), ..edge }, &one)).band, Band::Tight);
    assert_eq!(known(assess(&Rock { resistance: Some(61.42), ..edge }, &one)).band, Band::NeedsHelp);
    assert_eq!(assess_with_head_in(Model::Checker, &edge, &one), Verdict::Unknown(Missing::Reading));
    let mixed = Loadout { lasers: vec![head(0.7), head(1.0)], ..Loadout::default() };
    assert_eq!(assess_with_head_in(Model::Checker, &shown, &mixed), Verdict::Unknown(Missing::Reading), "which head the HUD used is not known");
    assert_eq!(assess_with_head_in(Model::Checker, &shown, &Loadout::default()), Verdict::Unknown(Missing::Lasers));
}

/// The head of turret 1 of the MOLE in the tests, with the values of the game data (LIVE 4.10, Helix II): 4930 W,
/// the lowest throttle at 30 % (the floor the HUD showed), resistance −30 %, Focus III and twice Rieger-C3 (power
/// ×0.95, ×1.25, ×1.25).
fn turret_one() -> Loadout {
    let rieger = Factors { power: 1.25, ..Factors::NONE };
    let focus = Factors { power: 0.95, ..Factors::NONE };
    Loadout {
        lasers: vec![Laser {
            power: 4930.0,
            min_share: 0.3,
            head: Factors { resistance: 0.7, ..Factors::NONE },
            modules: vec![focus, rieger, rieger],
        }],
        ..Loadout::default()
    }
}

/// In the game the rock of 16365 kg (resistance shown 38 % in green, with the head) began to heat from about 62 %
/// throttle on, not below; the one of 32975 kg at 61 % never did; a chunk of 1588 kg at 53 % was „easy“. The model
/// in use meets all three.
#[test]
fn the_model_in_use_meets_the_throttle_the_game_showed() {
    assert_eq!(Model::IN_USE, Model::MassOverFive);
    let loadout = turret_one();
    let all = loadout.combined().unwrap();
    let shown = Rock { mass: Some(16365.0), resistance: Some(38.0), instability: None };
    let Verdict::Known(found) = super::assess_with_head(&shown, &loadout) else { panic!("known") };
    // The throttle at which delivered meets required: lowest power plus that share of the rest.
    let edge = (found.required - all.min_power) / (all.power - all.min_power);
    assert!((0.58..=0.66).contains(&edge), "{edge}");
    assert!(found.band.cracks() && !found.too_strong);
    let checker = known(super::assess_with(Model::Checker, &Rock { resistance: Some(54.3), ..shown }, &loadout));
    assert!(!checker.band.cracks(), "the community checker would say no: {checker:?}");
    let big = Rock { mass: Some(32975.0), resistance: Some(61.0), instability: None };
    assert!(!known(super::assess(&big, &loadout)).band.cracks(), "never heated");
    let chunk = Rock { mass: Some(1588.0), resistance: Some(53.0), instability: None };
    let chunk = known(super::assess(&chunk, &loadout));
    assert!(chunk.band.cracks() && chunk.too_strong, "easy: already the lowest throttle is too much");
}

/// Turret 2 of the same MOLE with the values of the game data (Hofstede-S2): 4060 W, the lowest throttle at 10 %
/// (the floor the HUD showed), resistance −30 %, twice Focus III.
fn turret_two() -> Loadout {
    let focus = Factors { power: 0.95, ..Factors::NONE };
    Loadout {
        lasers: vec![Laser {
            power: 4060.0,
            min_share: 0.1,
            head: Factors { resistance: 0.7, ..Factors::NONE },
            modules: vec![focus, focus],
        }],
        ..Loadout::default()
    }
}

/// With turret 2 the rock of 3506 kg (44 % in green) began to heat from 31 % on: the throttle counted from the
/// lowest power meets it (27 %), a share of the highest (34 %) would not have heated at 31 %.
#[test]
fn the_throttle_from_the_lowest_meets_the_second_head_as_well() {
    let loadout = turret_two();
    let all = loadout.combined().unwrap();
    let shown = Rock { mass: Some(3506.0), resistance: Some(44.0), instability: None };
    let Verdict::Known(found) = super::assess_with_head(&shown, &loadout) else { panic!("known") };
    let from_lowest = (found.required - all.min_power) / (all.power - all.min_power);
    let share_of_highest = found.required / all.power;
    assert!((0.25..=0.31).contains(&from_lowest), "{from_lowest}");
    assert!(share_of_highest > 0.31, "{share_of_highest}");
    assert!(found.band.cracks() && !found.too_strong);
}

#[test]
fn the_throttle_counts_the_lowest_power_of_the_head() {
    let mut loadout = Loadout { lasers: vec![laser(4000.0)], ..Loadout::default() };
    loadout.lasers[0].min_share = 0.25;
    // Required 1000 W at 0 % (mass 5000 / 5): 1080 W wanted, 1000 W at the lowest, 3000 W more to the highest.
    let found = known(super::assess(&rock(5000.0, 0.0), &loadout));
    assert_eq!(found.throttle, Some(3));
    assert!(found.too_strong);
    let heavier = known(super::assess(&rock(15_000.0, 0.0), &loadout));
    assert_eq!(heavier.throttle, Some(75), "3240 W wanted: (3240 − 1000) / 3000");
    assert!(!heavier.too_strong);
}

/// The two meanings of the throttle, for 3240 W wanted with 1000 W at the lowest and 4000 W at the highest.
#[test]
fn the_throttle_from_the_lowest_and_as_a_share_of_the_highest() {
    assert!((Throttle::FromLowest.for_power(3240.0, 1000.0, 4000.0) - 74.67).abs() < 0.01);
    assert!((Throttle::ShareOfHighest.for_power(3240.0, 1000.0, 4000.0) - 81.0).abs() < 0.01);
    assert_eq!(Throttle::ShareOfHighest.for_power(500.0, 1000.0, 4000.0), 25.0, "not below the lowest power");
    assert_eq!(Throttle::FromLowest.for_power(500.0, 1000.0, 4000.0), 0.0);
    assert_eq!(Throttle::FromLowest.for_power(9000.0, 1000.0, 4000.0), 100.0);
}

#[test]
fn a_laser_too_strong_says_so() {
    use crate::language::Lang;
    let mut loadout = Loadout { lasers: vec![laser(4000.0)], ..Loadout::default() };
    loadout.lasers[0].min_share = 0.25;
    let verdict = super::assess(&rock(5000.0, 0.0), &loadout);
    assert_eq!(
        verdict_line_in(Lang::De, &verdict),
        "Aufbrechbar · leicht · schon das kleinste Gas ist zu viel: schwächeren Laser nehmen"
    );
}

#[test]
fn percent_changes_turn_into_factors() {
    let factors = Factors::from_percent(25.0, -30.0, 10.0);
    assert!(close(factors.power, 1.25) && close(factors.resistance, 0.7) && close(factors.instability, 1.1));
}
