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

//! Can the own lasers crack a scanned rock, and what is it worth?
//!
//! The rock gives mass, resistance and instability; the equipment gives the power of each laser and the factors of
//! its head, its modules, a gadget on the rock and an active module. The factors are multipliers (`1.0` changes
//! nothing, `0.75` lowers by a quarter).
//!
//! | Step | Rule |
//! |---|---|
//! | Power of a laser | its power × the power factors of its modules; at throttle `x` its lowest power plus `x` of the rest |
//! | Power of the lasers together | summed |
//! | Resistance factor of a laser | head × its modules |
//! | Resistance factor of the lasers together | averaged, then × gadget × active module |
//! | Effective resistance | rock resistance × that factor, within 0–98 |
//! | Required and delivered power | by the [`Model`]; cracked when delivered ≥ required |
//!
//! The arithmetic of required and delivered power lives in [`Model::powers`] alone: it is a community formula, no
//! published one, and changing it is changing that one function.
//!
//! ⚠ A value that was not read sure is no value: an unknown mass or resistance gives [`Verdict::Unknown`], never a
//! guess.

/// The power the community checker asks per kilogram of rock.
pub const POWER_PER_KG: f64 = 0.36;
/// The effective resistance never rises above this.
pub const MAX_RESISTANCE: f64 = 98.0;
/// The throttle recommended is the share of power that gives this much more than required.
pub const THROTTLE_MARGIN: f64 = 1.08;
/// The throttle recommended stays within these percent.
pub const THROTTLE_RANGE: (f64, f64) = (0.0, 100.0);

/// The multipliers a head, a module or a gadget brings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Factors {
    pub power: f64,
    pub resistance: f64,
    pub instability: f64,
}

impl Default for Factors {
    fn default() -> Factors {
        Factors::NONE
    }
}

impl Factors {
    /// Changes nothing.
    pub const NONE: Factors = Factors { power: 1.0, resistance: 1.0, instability: 1.0 };

    /// Both applied one after the other.
    pub fn times(self, other: Factors) -> Factors {
        Factors {
            power: self.power * other.power,
            resistance: self.resistance * other.resistance,
            instability: self.instability * other.instability,
        }
    }

    /// From changes in percent as the game texts write them: `-25` resistance is the factor `0.75`.
    pub fn from_percent(power: f64, resistance: f64, instability: f64) -> Factors {
        Factors { power: 1.0 + power / 100.0, resistance: 1.0 + resistance / 100.0, instability: 1.0 + instability / 100.0 }
    }
}

/// One laser that fires at the rock.
#[derive(Clone, Debug, PartialEq)]
pub struct Laser {
    /// The highest power of the head.
    pub power: f64,
    /// The power at the lowest throttle as a share of the highest (0–1): the throttle `x` gives
    /// `min + (1 − min) × x` of the power.
    pub min_share: f64,
    /// The factors of the head (its power factor is not used: the power is the head's own).
    pub head: Factors,
    /// The passive modules in the head.
    pub modules: Vec<Factors>,
}

/// The lasers that fire together and what helps them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Loadout {
    pub lasers: Vec<Laser>,
    /// A gadget placed on the rock.
    pub gadget: Option<Factors>,
    /// An active module in use.
    pub active: Option<Factors>,
}

/// The lasers combined: summed power (at full and at the lowest throttle), averaged factors with gadget and
/// active module applied.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Combined {
    pub power: f64,
    pub min_power: f64,
    pub resistance: f64,
    pub instability: f64,
}

impl Loadout {
    /// `None` without a laser.
    pub fn combined(&self) -> Option<Combined> {
        if self.lasers.is_empty() {
            return None;
        }
        let mut power = 0.0;
        let mut min_power = 0.0;
        let mut resistance = 0.0;
        let mut instability = 0.0;
        for laser in &self.lasers {
            let own = laser.modules.iter().fold(Factors { power: 1.0, ..laser.head }, |all, m| all.times(*m));
            power += laser.power * own.power;
            min_power += laser.power * laser.min_share.clamp(0.0, 1.0) * own.power;
            resistance += own.resistance;
            instability += own.instability;
        }
        let count = self.lasers.len() as f64;
        let mut out = Combined { power, min_power, resistance: resistance / count, instability: instability / count };
        for extra in [self.gadget, self.active].into_iter().flatten() {
            out.power *= extra.power;
            out.min_power *= extra.power;
            out.resistance *= extra.resistance;
            out.instability *= extra.instability;
        }
        Some(out)
    }
}

/// The formula for required and delivered power.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Model {
    /// The community checker: required `0.36 × mass`, resistance lowers the power delivered.
    Checker,
    /// Required `mass / (1 − resistance) / 5`, the power delivered is the full power: the one that meets the
    /// throttle at which a rock began to heat in the game (with the lowest power of the head and the modules
    /// multiplied).
    #[default]
    MassOverFive,
}

/// What the throttle of the HUD means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Throttle {
    /// The throttle `x` gives the lowest power plus `x` of the rest.
    FromLowest,
    /// The throttle is the share of the highest power; it goes no lower than the lowest power.
    ShareOfHighest,
}

impl Throttle {
    /// The meaning the program uses.
    pub const IN_USE: Throttle = Throttle::FromLowest;

    /// The throttle in percent (0–100) at which `wanted` is delivered, from the power delivered at the lowest
    /// (`lowest`) and at the highest throttle (`highest`).
    pub fn for_power(self, wanted: f64, lowest: f64, highest: f64) -> f64 {
        if highest <= 0.0 {
            return 100.0;
        }
        let share = match self {
            Throttle::FromLowest if highest > lowest => (wanted - lowest) / (highest - lowest),
            Throttle::FromLowest => 1.0,
            Throttle::ShareOfHighest => wanted.max(lowest) / highest,
        };
        (share * 100.0).clamp(THROTTLE_RANGE.0, THROTTLE_RANGE.1)
    }
}

impl Model {
    /// The model the program uses.
    pub const IN_USE: Model = Model::MassOverFive;

    /// Required and delivered power for a rock of `mass` kg at `resistance` percent (already the effective one)
    /// against `power`.
    pub fn powers(self, mass: f64, resistance: f64, power: f64) -> (f64, f64) {
        let share = resistance / 100.0;
        match self {
            Model::Checker => (POWER_PER_KG * mass, power * (1.0 - share)),
            Model::MassOverFive => (mass / (1.0 - share) / 5.0, power),
        }
    }
}

/// How easily the rock cracks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Band {
    /// At least 160 % of the power required.
    Easy,
    /// At least 115 %.
    Good,
    /// At least 100 %.
    Tight,
    /// Below, but at least 70 %: a gadget or an active module may close the gap.
    NeedsHelp,
    /// Below 70 %.
    MoreLasers,
}

impl Band {
    pub fn of(ratio: f64) -> Band {
        if ratio >= 1.6 {
            Band::Easy
        } else if ratio >= 1.15 {
            Band::Good
        } else if ratio >= 1.0 {
            Band::Tight
        } else if ratio >= 0.7 {
            Band::NeedsHelp
        } else {
            Band::MoreLasers
        }
    }

    pub fn cracks(self) -> bool {
        matches!(self, Band::Easy | Band::Good | Band::Tight)
    }

    /// The text key of its word.
    pub fn label(self) -> &'static str {
        match self {
            Band::Easy => "s_stein_stufe_leicht",
            Band::Good => "s_stein_stufe_gut",
            Band::Tight => "s_stein_stufe_knapp",
            Band::NeedsHelp => "s_stein_stufe_hilfe",
            Band::MoreLasers => "s_stein_stufe_mehr",
        }
    }
}

/// What the rock gives, each value only where it was read sure.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rock {
    pub mass: Option<f64>,
    /// Percent.
    pub resistance: Option<f64>,
    pub instability: Option<f64>,
}

/// Why there is no answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Missing {
    /// Mass or resistance was not read sure.
    Reading,
    /// No laser chosen.
    Lasers,
}

/// The answer for one rock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Assessment {
    pub required: f64,
    pub delivered: f64,
    /// Delivered / required.
    pub ratio: f64,
    pub effective_resistance: f64,
    pub band: Band,
    /// The throttle in percent that cracks with a small margin, the lowest power of the heads counted; only where it
    /// cracks.
    pub throttle: Option<i32>,
    /// Already the lowest throttle delivers what is required: the rock heats too fast, a weaker laser fits better.
    pub too_strong: bool,
    /// The instability with the factors applied, where it was read.
    pub instability: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Verdict {
    Unknown(Missing),
    Known(Assessment),
}

/// The answer of `model` for `rock` and `loadout`.
pub fn assess_with(model: Model, rock: &Rock, loadout: &Loadout) -> Verdict {
    let (Some(mass), Some(resistance)) = (rock.mass, rock.resistance) else {
        return Verdict::Unknown(Missing::Reading);
    };
    if !mass.is_finite() || mass <= 0.0 || !resistance.is_finite() || !(0.0..=100.0).contains(&resistance) {
        return Verdict::Unknown(Missing::Reading);
    }
    let Some(all) = loadout.combined() else { return Verdict::Unknown(Missing::Lasers) };
    let effective = (resistance * all.resistance).clamp(0.0, MAX_RESISTANCE);
    let (required, delivered) = model.powers(mass, effective, all.power);
    let (_, at_least) = model.powers(mass, effective, all.min_power);
    let ratio = if required > 0.0 { delivered / required } else { 0.0 };
    let band = Band::of(ratio);
    let throttle = (band.cracks() && delivered > 0.0)
        .then(|| Throttle::IN_USE.for_power(THROTTLE_MARGIN * required, at_least, delivered).round() as i32);
    Verdict::Known(Assessment {
        required,
        delivered,
        ratio,
        effective_resistance: effective,
        band,
        throttle,
        too_strong: at_least >= required,
        instability: rock.instability.map(|value| (value * all.instability).clamp(0.0, 99.0)),
    })
}

/// The answer of the model in use.
pub fn assess(rock: &Rock, loadout: &Loadout) -> Verdict {
    assess_with(Model::IN_USE, rock, loadout)
}

/// The answer for a rock whose resistance the HUD shows with the factor of the laser in use applied (written in
/// green, whole percent cut down: 61 % × 0.70 shows 42 %). The rock's own lies between `shown / f` and
/// `(shown + 1) / f`, `f` the resistance factor of a laser (head × modules). Known only where every laser has the
/// same factor (else it is not known which one the HUD used) and both ends give the same band; the answer of the
/// higher end then.
pub fn assess_with_head(rock: &Rock, loadout: &Loadout) -> Verdict {
    assess_with_head_in(Model::IN_USE, rock, loadout)
}

/// [`assess_with_head`] by `model`.
pub fn assess_with_head_in(model: Model, rock: &Rock, loadout: &Loadout) -> Verdict {
    let assess = |rock: &Rock, loadout: &Loadout| assess_with(model, rock, loadout);
    let Some(shown) = rock.resistance else { return Verdict::Unknown(Missing::Reading) };
    let factors: Vec<f64> = loadout
        .lasers
        .iter()
        .map(|laser| laser.modules.iter().fold(laser.head.resistance, |all, m| all * m.resistance))
        .collect();
    let Some(&factor) = factors.first() else { return Verdict::Unknown(Missing::Lasers) };
    if factor <= 0.0 || factors.iter().any(|f| (f - factor).abs() > 1e-9) {
        return Verdict::Unknown(Missing::Reading);
    }
    let low = Rock { resistance: Some((shown / factor).min(100.0)), ..*rock };
    let high = Rock { resistance: Some(((shown + 1.0) / factor - 1e-6).min(100.0)), ..*rock };
    match (assess(&low, loadout), assess(&high, loadout)) {
        (Verdict::Known(a), Verdict::Known(b)) if a.band == b.band => Verdict::Known(b),
        (Verdict::Unknown(why), _) | (_, Verdict::Unknown(why)) => Verdict::Unknown(why),
        _ => Verdict::Unknown(Missing::Reading),
    }
}

/// The worthless rest of a rock (`INERT MATERIALS`).
pub fn is_inert(name: &str) -> bool {
    name.trim().to_lowercase().starts_with("inert")
}

/// The rough worth of a rock: `scu` × the share of each material × its price per SCU, the inert rest left out.
/// `None` where a share has no price (nothing would be half right) or nothing is known.
pub fn worth(scu: f64, shares: &[(String, f64)], price: impl Fn(&str) -> Option<f64>) -> Option<f64> {
    if !scu.is_finite() || scu <= 0.0 || shares.is_empty() {
        return None;
    }
    let mut sum = 0.0;
    for (name, percent) in shares {
        if *percent <= 0.0 || is_inert(name) {
            continue;
        }
        sum += scu * percent / 100.0 * price(name)?;
    }
    Some(sum)
}

/// The line of the answer as the window above the signature and the overlay show it:
/// `Aufbrechbar · gut · Gas 46 %`, `Nicht aufbrechbar · mehr Laser`, `Aufbrechbar: unsicher gelesen`.
pub fn verdict_line_in(lang: crate::language::Lang, verdict: &Verdict) -> String {
    use crate::language::{Arg, t_in, tf_in};
    match verdict {
        Verdict::Unknown(Missing::Reading) => t_in(lang, "s_stein_unsicher"),
        Verdict::Unknown(Missing::Lasers) => t_in(lang, "s_stein_kein_laser"),
        Verdict::Known(found) => match found.throttle {
            Some(_) if found.band.cracks() && found.too_strong => {
                tf_in(lang, "s_stein_zu_stark", &[("0", Arg::Text(t_in(lang, found.band.label())))])
            }
            Some(throttle) if found.band.cracks() => tf_in(
                lang,
                "s_stein_ja",
                &[("0", Arg::Text(t_in(lang, found.band.label()))), ("1", Arg::Int(i64::from(throttle)))],
            ),
            _ => tf_in(lang, "s_stein_nein", &[("0", Arg::Text(t_in(lang, found.band.label())))]),
        },
    }
}

/// The rough worth in a line, to the hundred: `ca. 12.300 aUEC`.
pub fn worth_line_in(lang: crate::language::Lang, worth: f64) -> String {
    let rounded = (worth / 100.0).round() * 100.0;
    crate::language::tf_in(lang, "s_stein_wert", &[("0", crate::language::Arg::Text(crate::formats::money(lang, rounded)))])
}

#[cfg(test)]
mod tests;
