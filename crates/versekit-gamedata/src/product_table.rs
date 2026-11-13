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

//! The table of product values of a crafted item: property, base value,
//! built value, change (`product_stats.rows` and `formatted` of 4.12).
//!
//! It is computed the way scmdb.net does it, so that both show the same
//! number. A property without a base value gets no row; a section without a
//! row is left out.

use versekit_core::language::{Arg, py_fixed, py_g, py_str_float, t, tf};

use crate::craft_book::{manufacturer_code, manufacturer_name};
use crate::quality;
use crate::values::{DAMAGE_KINDS, FireMode, ItemType, ItemValues};

/// How a row's change is rated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Better {
    /// More is better.
    High,
    /// Less is better.
    Low,
    /// The amount counts, recoil: less is better.
    Absolute,
    /// More is better, and the row is highlighted (damage per second).
    Dps,
}

/// One value: label, base value, built value, unit, decimals.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub label: String,
    pub base: f64,
    pub made: Option<f64>,
    pub unit: String,
    pub decimals: usize,
    pub better: Better,
}

/// A line of the table.
#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    /// The facts above the table: maker, mass, size, range.
    Info(String),
    Section(String),
    Row(Row),
}

/// How a changed value is rated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rating {
    Good,
    Bad,
    Neutral,
}

/// A row as text: base value, built value, change, rating.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub base: String,
    pub made: String,
    pub change: String,
    pub rating: Rating,
}

/// The values of a mining module in the order the game data lists them.
const MODULE_ORDER: [&str; 8] = [
    "instability",
    "shatterDamage",
    "resistance",
    "optimalChargeWindowSize",
    "optimalChargeRate",
    "clusterFactor",
    "catastrophicChargeRate",
    "filter",
];

struct Builder {
    lines: Vec<Line>,
}

impl Builder {
    fn info(&mut self, parts: &[String]) {
        self.lines.push(Line::Info(parts.join(" · ")));
    }

    fn section(&mut self, title: String) {
        self.lines.push(Line::Section(title));
    }

    /// No base value, no row: scmdb leaves it out as well.
    fn row(
        &mut self,
        label: String,
        base: Option<f64>,
        made: Option<f64>,
        unit: &str,
        decimals: usize,
        better: Better,
    ) {
        if let Some(base) = base {
            self.lines.push(Line::Row(Row { label, base, made, unit: unit.into(), decimals, better }));
        }
    }
}

fn info_parts(item: &ItemValues) -> Vec<String> {
    let mut parts = Vec::new();
    if let Some(maker) = item.manufacturer.as_deref().filter(|m| !m.is_empty()) {
        let code = item.extra.texts.get("manufacturerCode").map_or_else(String::new, |c| c.clone());
        let code = if code.is_empty() { manufacturer_code(&item.class_name) } else { code };
        parts.push(manufacturer_name(maker, &code));
    }
    if let Some(mass) = item.number("mass").filter(|m| *m > 0.0) {
        parts.push(format!("{} kg", py_g(mass, false)));
    }
    parts
}

fn plain(key: &str) -> String {
    t(key)
}

fn with(key: &str, args: &[Arg]) -> String {
    let named: Vec<(String, Arg)> = args.iter().enumerate().map(|(i, a)| (i.to_string(), a.clone())).collect();
    let refs: Vec<(&str, Arg)> = named.iter().map(|(n, a)| (n.as_str(), a.clone())).collect();
    tf(key, &refs)
}

fn size_part(item: &ItemValues) -> Option<String> {
    item.size.map(|size| with("s_ps_groesse", &[Arg::Int(size)]))
}

/// Python's `str.capitalize`.
fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars.flat_map(char::to_lowercase)).collect(),
        None => String::new(),
    }
}

/// `round(x)` to an integer, ties to even.
fn round_int(x: f64) -> i64 {
    x.round_ties_even() as i64
}

fn mining_laser(item: &ItemValues, made: &ItemValues, out: &mut Builder) {
    let mut info = info_parts(item);
    info.extend(size_part(item));
    out.info(&info);
    out.section(plain("s_ps_komponente"));
    out.row(plain("s_ps_integritaet"), item.number("health"), made.number("health"), " HP", 0, Better::High);
    out.row(plain("s_ps_strom"), item.number("powerDraw"), made.number("powerDraw"), "", 1, Better::Low);
    out.row(
        plain("s_ps_reparatur"),
        item.number("selfRepairTime"),
        made.number("selfRepairTime"),
        " s",
        1,
        Better::Low,
    );
    for (i, beam) in item.beams.iter().enumerate() {
        let info = item.extra.beams.get(i).cloned().unwrap_or_default();
        let made_beam = made.extra.beams.get(i).cloned().unwrap_or_default();
        let made_dps = made.beams.get(i).copied();
        out.section(match info.hit_type.as_str() {
            "ElectricArc" => plain("s_ps_laserkraft"),
            "Extraction" => plain("s_ps_extraktion"),
            other => other.to_string(),
        });
        let decimals = if *beam < 10.0 { 2 } else { 0 };
        out.row(plain("s_ps_dps"), Some(*beam), made_dps, "", decimals, Better::Dps);
        out.row(plain("s_ps_voll_bis"), info.full_range, made_beam.full_range, " m", 0, Better::High);
        out.row(plain("s_ps_null_ab"), info.zero_range, made_beam.zero_range, " m", 0, Better::Low);
    }
}

fn mode_title(mode: &FireMode) -> String {
    let name = mode.action.name.as_deref().unwrap_or_default();
    let kind = mode.action.kind.as_str();
    let mut title = capitalize(if name.is_empty() { kind } else { name });
    if !name.is_empty() && !kind.is_empty() && kind != name.to_lowercase() {
        title.push_str(&format!(" ({kind})"));
    }
    title
}

fn weapon(item: &ItemValues, made: &ItemValues, out: &mut Builder) {
    let mut info = info_parts(item);
    let weapon = item.weapon.clone().unwrap_or_default();
    let ammo = weapon.ammo.clone().unwrap_or_default();
    let gun = item.cg_item_type.as_deref() == Some("WeaponGun");
    if gun && ammo.speed.unwrap_or(0.0) > 0.0 && ammo.lifetime.unwrap_or(0.0) > 0.0 {
        let reach = round_int(ammo.speed.unwrap_or(0.0) * ammo.lifetime.unwrap_or(0.0));
        info.push(with("s_ps_reichweite_m", &[Arg::Int(reach)]));
    } else if !gun && let Some(category) = item.extra.texts.get("combatRange").filter(|c| !c.is_empty()) {
        info.push(with("s_ps_reichweite", &[Arg::from(category.as_str())]));
    }
    if let Some(count) = weapon.magazine.filter(|c| *c > 0) {
        info.push(with("s_ps_magazin", &[Arg::Int(count)]));
    }
    out.info(&info);
    // A scattergun counts all the pellets of a shot, as the values under the description of the item do.
    let all_pellets = |item: &ItemValues| quality::dps(item).map(|dps| dps * quality::pellets(item));
    let (base_dps, made_dps) = (all_pellets(item), all_pellets(made));
    let (base_damage, detonation) = quality::damage(weapon.ammo.as_ref());
    let (made_damage, _) = quality::damage(made.weapon.as_ref().and_then(|w| w.ammo.as_ref()));
    let mut label = match base_damage.as_ref().and_then(crate::values::Damage::single_kind) {
        Some(kind) => with("s_ps_schaden_art", &[Arg::from(t(&format!("s_ps_dmg_{kind}")))]),
        None => plain("s_ps_schaden"),
    };
    if detonation {
        label.push(' ');
        label.push_str(&plain("s_ps_detonation"));
    }
    let made_weapon = made.weapon.clone().unwrap_or_default();
    for (i, mode) in weapon.fire_modes.iter().enumerate() {
        let made_mode = made_weapon.fire_modes.get(i).unwrap_or(mode);
        let (e, me) = (mode.shown(), made_mode.shown());
        out.section(mode_title(mode));
        let rate = |a: &crate::values::FireAction| a.fire_rate.filter(|r| *r != 0.0).or(a.delay.filter(|d| *d != 0.0));
        out.row(plain("s_ps_feuerrate"), rate(e), rate(me), " RPM", 0, Better::High);
        out.row(
            label.clone(),
            Some(base_damage.map_or(0.0, |d| d.total())),
            Some(made_damage.map_or(0.0, |d| d.total())),
            "",
            1,
            Better::High,
        );
        if let (Some(base), 0) = (base_dps, i) {
            out.row(plain("s_ps_dps"), Some(base), made_dps, "", 1, Better::Dps);
        }
        if let Some(recoil) = e.recoil {
            let mrecoil = me.recoil.unwrap_or_default();
            out.row(plain("s_ps_rueck_pitch"), recoil.pitch_max_deg, mrecoil.pitch_max_deg, "°", 3, Better::Absolute);
            out.row(plain("s_ps_rueck_yaw"), recoil.yaw_max_deg, mrecoil.yaw_max_deg, "°", 3, Better::Absolute);
            out.row(plain("s_ps_rueck_glatt"), recoil.smooth_time, mrecoil.smooth_time, " s", 3, Better::Absolute);
        }
        if let Some((min, max)) = e.spread {
            out.row(
                plain("s_ps_streuung"),
                Some(min),
                Some(min),
                &format!("–{}°", py_fixed(max, 2, false)),
                2,
                Better::High,
            );
        }
        if let Some(speed) = ammo.speed.filter(|s| *s > 0.0) {
            out.row(plain("s_ps_geschoss"), Some(speed), Some(speed), " m/s", 0, Better::High);
        }
        if let Some(heat) = e.heat_per_shot.filter(|h| *h > 0.0)
            && let Some(overheat) = item.extra.numbers.get("overheatTemperature")
        {
            out.row(
                plain("s_ps_hitze"),
                Some(heat),
                Some(heat),
                &format!(" / {}", py_str_float(*overheat)),
                1,
                Better::High,
            );
        }
    }
}

fn ship_component(item: &ItemValues, made: &ItemValues, out: &mut Builder) {
    let attach = item.attach_type.clone().unwrap_or_default().to_lowercase();
    let mut info = info_parts(item);
    info.extend(size_part(item));
    if let Some(class) = item.extra.texts.get("componentClass").filter(|c| !c.is_empty()) {
        info.push(class.clone());
    }
    out.info(&info);
    out.section(plain("s_ps_komponente"));
    let n = |name: &str| (item.number(name), made.number(name));
    let mut row = |label: &str, name: &str, unit: &str, decimals: usize, better: Better| {
        let (base, built) = n(name);
        out.row(plain(label), base, built, unit, decimals, better);
    };
    row("s_ps_integritaet", "health", " HP", 0, Better::High);
    row("s_ps_strom", "powerDraw", "", 1, Better::Low);
    row("s_ps_em", "emSignature", "", 0, Better::Low);
    if item.number("selfRepairTime").is_some_and(|s| s > 0.0) {
        row("s_ps_reparatur", "selfRepairTime", " s", 1, Better::Low);
    }
    match attach.as_str() {
        "shield" => {
            out.section(plain("s_ps_schild"));
            let mut row = |label: &str, name: &str, unit: &str, decimals: usize, better: Better| {
                out.row(plain(label), item.number(name), made.number(name), unit, decimals, better);
            };
            row("s_ps_schild_hp", "shieldHealth", "", 0, Better::High);
            row("s_ps_regen", "shieldRegen", "/s", 1, Better::High);
            row("s_ps_down_delay", "downedRegenDelay", " s", 1, Better::Low);
            row("s_ps_dmg_delay", "damagedRegenDelay", " s", 1, Better::Low);
        }
        "radar" => {
            out.section(plain("s_ps_radar"));
            let mut row = |label: &str, name: &str, unit: &str, decimals: usize, better: Better| {
                out.row(plain(label), item.number(name), made.number(name), unit, decimals, better);
            };
            row("s_ps_empfindlich", "sensitivity", "", 2, Better::High);
            row("s_ps_durchdringung", "piercing", "", 2, Better::High);
            row("s_ps_ping", "pingCooldown", " s", 1, Better::Low);
            if item.number("minAimAssistRange").is_some() || item.number("maxAimAssistRange").is_some() {
                row("s_ps_hilfe_min", "minAimAssistRange", " m", 0, Better::High);
                row("s_ps_hilfe_max", "maxAimAssistRange", " m", 0, Better::High);
            } else {
                row("s_ps_hilfe", "aimAssistRange", " m", 0, Better::High);
            }
        }
        "powerplant" => {
            out.section(plain("s_ps_kraftwerk"));
            out.row(
                plain("s_ps_leistung"),
                item.number("powerOutput"),
                made.number("powerOutput"),
                " Pips",
                0,
                Better::High,
            );
        }
        "cooler" => {
            out.section(plain("s_ps_kuehler"));
            out.row(
                plain("s_ps_kuehlrate"),
                item.number("coolingRate"),
                made.number("coolingRate"),
                "",
                1,
                Better::High,
            );
            out.row(plain("s_ps_ir"), item.number("irSignature"), made.number("irSignature"), "", 0, Better::Low);
        }
        "tractorbeam" => {
            out.section(plain("s_ps_traktor"));
            let mut row = |label: &str, name: &str, unit: &str, decimals: usize| {
                out.row(plain(label), item.number(name), made.number(name), unit, decimals, Better::High);
            };
            row("s_ps_kraft_min", "minForce", " N", 0);
            row("s_ps_kraft_max", "maxForce", " N", 0);
            row("s_ps_voll_bis", "fullStrengthDistance", " m", 1);
            row("s_ps_max_dist", "maxDistance", " m", 1);
            row("s_ps_max_vol", "maxVolume", "", 0);
        }
        "salvagemodifier" => {
            out.section(plain("s_ps_bergung"));
            for (label, name) in [
                ("s_ps_tempo", "salvageSpeedMultiplier"),
                ("s_ps_radius", "radiusMultiplier"),
                ("s_ps_effizienz", "extractionEfficiency"),
            ] {
                out.row(
                    plain(label),
                    item.number(name).map(|v| v * 100.0),
                    made.number(name).map(|v| v * 100.0),
                    " %",
                    1,
                    Better::High,
                );
            }
        }
        _ => {}
    }
    if item.number("fuelRate").is_some() || item.number("quantumFuelRate").is_some() {
        out.section(plain("s_ps_duese"));
        out.row(plain("s_ps_wasserstoff"), item.number("fuelRate"), made.number("fuelRate"), "", 2, Better::Low);
        out.row(
            plain("s_ps_quantum_rate"),
            item.number("quantumFuelRate"),
            made.number("quantumFuelRate"),
            "",
            2,
            Better::Low,
        );
    }
    match attach.as_str() {
        "quantumdrive" => {
            out.section(plain("s_ps_quantum"));
            out.row(
                plain("s_ps_tempo_q"),
                item.number("driveSpeed").map(|v| v / 1e6),
                made.number("driveSpeed").map(|v| v / 1e6),
                " Mm/s",
                2,
                Better::High,
            );
            out.row(
                plain("s_ps_verbrauch"),
                item.number("quantumFuelRequirement"),
                made.number("quantumFuelRequirement"),
                "",
                4,
                Better::Low,
            );
            out.row(plain("s_ps_spool"), item.number("spoolUpTime"), made.number("spoolUpTime"), " s", 1, Better::Low);
            out.row(
                plain("s_ps_abkuehlen"),
                item.number("cooldownTime"),
                made.number("cooldownTime"),
                " s",
                1,
                Better::Low,
            );
        }
        "container" => {
            out.section(plain("s_ps_erzkapsel"));
            out.row(
                plain("s_ps_fracht"),
                item.number("cargoCapacitySCU"),
                made.number("cargoCapacitySCU"),
                " SCU",
                0,
                Better::High,
            );
        }
        "miningmodifier" => {
            out.section(plain("s_ps_modul"));
            out.row(plain("s_ps_ladungen"), item.number("charges"), made.number("charges"), "", 0, Better::High);
            // The data names them in the order of the game; a map keeps them
            // sorted, so the known ones are put back in that order.
            let mut shown: Vec<(&String, &f64)> = item.extra.modifiers.iter().collect();
            shown.sort_by_key(|(key, _)| {
                MODULE_ORDER.iter().position(|known| known.eq_ignore_ascii_case(key)).unwrap_or(MODULE_ORDER.len())
            });
            for (key, value) in shown {
                let label_key = format!("s_ps_mm_{}", key.to_lowercase());
                let label = t(&label_key);
                let label = if label == label_key { key.clone() } else { label };
                out.row(label, Some(*value), made.extra.modifiers.get(key).copied(), "", 1, Better::High);
            }
        }
        _ => {}
    }
}

fn armour(item: &ItemValues, made: &ItemValues, out: &mut Builder) {
    let mut info = info_parts(item);
    if let Some(sub) = item.extra.texts.get("attachSubType").filter(|s| !s.is_empty()) {
        info.push(sub.clone());
    }
    out.info(&info);
    let Some(armor) = &item.armor else { return };
    let made_armor = made.armor.clone().unwrap_or_default();
    let cells: Vec<usize> = (0..DAMAGE_KINDS.len()).filter(|i| armor.resistance[*i].is_some()).collect();
    if !cells.is_empty() {
        out.section(plain("s_ps_widerstand"));
        for i in cells {
            let base = armor.resistance[i].map(|m| (1.0 - m) * 100.0);
            let built = made_armor.resistance[i].map(|m| (1.0 - m) * 100.0);
            out.row(plain(&format!("s_ps_dmg_{}", DAMAGE_KINDS[i])), base, built, " %", 1, Better::High);
        }
    }
    if let Some((min, max)) = armor.temperature {
        let built = made_armor.temperature;
        out.section(plain("s_ps_temperatur"));
        out.row(plain("s_ps_temp_min"), Some(min), built.map(|(m, _)| m), " °C", 1, Better::Low);
        out.row(plain("s_ps_temp_max"), Some(max), built.map(|(_, m)| m), " °C", 1, Better::High);
    }
    if armor.radiation_capacity.is_some() || armor.radiation_dissipation.is_some() {
        out.section(plain("s_ps_strahlung"));
        out.row(
            plain("s_ps_rad_schutz"),
            armor.radiation_capacity,
            made_armor.radiation_capacity,
            " REM",
            0,
            Better::High,
        );
        out.row(
            plain("s_ps_rad_abbau"),
            armor.radiation_dissipation,
            made_armor.radiation_dissipation,
            " REM/s",
            0,
            Better::High,
        );
    }
}

/// A heading with no row below it falls away; the facts line does not count.
fn without_empty_sections(lines: Vec<Line>) -> Vec<Line> {
    let mut kept = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if matches!(line, Line::Section(_)) {
            let next = lines[i + 1..].iter().find(|l| !matches!(l, Line::Info(_)));
            if next.is_none_or(|n| matches!(n, Line::Section(_))) {
                continue;
            }
        }
        kept.push(line.clone());
    }
    kept
}

/// The lines of the table for `item` and the item as it is built (`rows`).
pub fn lines(item: &ItemValues, made: &ItemValues) -> Vec<Line> {
    let mut out = Builder { lines: Vec::new() };
    if item.cg_item_type.as_deref() == Some("WeaponMining") {
        mining_laser(item, made, &mut out);
        return without_empty_sections(out.lines);
    }
    match item.item_type {
        ItemType::Weapon => weapon(item, made, &mut out),
        ItemType::ShipComponent => ship_component(item, made, &mut out),
        _ => armour(item, made, &mut out),
    }
    without_empty_sections(out.lines)
}

/// [`formatted`] with the decimal mark of the language (German writes `0,85`, not `0.85`).
pub fn formatted_in(row: &Row, lang: versekit_core::language::Lang) -> Shown {
    let mut shown = formatted(row);
    if lang == versekit_core::language::Lang::De {
        shown.base = shown.base.replace('.', ",");
        shown.made = shown.made.replace('.', ",");
        shown.change = shown.change.replace('.', ",");
    }
    shown
}

/// A row as text (`formatted`): the built value is a dash when it reads the
/// same as the base value, the change is rated by its direction.
pub fn formatted(row: &Row) -> Shown {
    let base_text = py_fixed(row.base, row.decimals, false);
    let made_text = row.made.map(|m| py_fixed(m, row.decimals, false));
    let base_shown = format!("{base_text}{}", row.unit);
    let Some(made) = row.made else {
        return Shown { base: base_shown, made: "—".into(), change: String::new(), rating: Rating::Neutral };
    };
    let made_text = made_text.unwrap_or_default();
    if made_text == base_text {
        return Shown { base: base_shown, made: "—".into(), change: String::new(), rating: Rating::Neutral };
    }
    if row.base == 0.0 {
        return Shown {
            base: base_shown,
            made: format!("{made_text}{}", row.unit),
            change: String::new(),
            rating: Rating::Neutral,
        };
    }
    let pct = if row.better == Better::Absolute {
        (made.abs() - row.base.abs()) / row.base.abs() * 100.0
    } else {
        (made - row.base) / row.base.abs() * 100.0
    };
    let lower = matches!(row.better, Better::Low | Better::Absolute);
    let good = if lower { pct < -0.05 } else { pct > 0.05 };
    let bad = if lower { pct > 0.05 } else { pct < -0.05 };
    Shown {
        base: base_shown,
        made: format!("{made_text}{}", row.unit),
        change: format!("{} %", py_fixed(pct, 2, true)),
        rating: if good {
            Rating::Good
        } else if bad {
            Rating::Bad
        } else {
            Rating::Neutral
        },
    }
}

/// The table of an item built with the qualities of `factors`
/// (`crafting.product_table`): nothing where the item has no values.
pub fn table(item: &ItemValues, factors: &quality::Factors) -> Vec<Line> {
    let made = quality::crafted(item, factors);
    lines(item, &made)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::values::{Ammo, Damage, FireAction, FireMode, Weapon};

    /// A ship weapon firing `rate` shots a minute of `pellets` bullets with `damage` each.
    fn gun(rate: f64, pellets: i64, damage: f64) -> ItemValues {
        ItemValues {
            item_type: ItemType::Weapon,
            cg_item_type: Some("WeaponGun".into()),
            weapon: Some(Weapon {
                fire_modes: vec![FireMode {
                    action: FireAction {
                        name: Some("Single".into()),
                        kind: "single".into(),
                        fire_rate: Some(rate),
                        damage_multiplier: Some(1.0),
                        pellet_count: Some(pellets),
                        ..FireAction::default()
                    },
                    sequence: Vec::new(),
                }],
                ammo: Some(Ammo { damage: Some(Damage([damage, 0.0, 0.0, 0.0, 0.0, 0.0])), ..Ammo::default() }),
                magazine: None,
            }),
            ..ItemValues::default()
        }
    }

    fn dps_row(item: &ItemValues) -> Row {
        let label = t("s_ps_dps");
        lines(item, item)
            .into_iter()
            .find_map(|line| match line {
                Line::Row(row) if row.label == label => Some(row),
                _ => None,
            })
            .expect("the row with the damage per second")
    }

    #[test]
    fn the_table_counts_all_the_pellets_of_a_scattergun() {
        // Salvation Distortion Scattergun (S4): 8 pellets of 60, 60 shots a minute: 480, as erkul.games.
        let row = dps_row(&gun(60.0, 8, 60.0));
        assert_eq!(row.base, 480.0);
        assert_eq!(row.made, Some(480.0));
    }

    #[test]
    fn the_table_of_a_weapon_with_one_bullet_is_unchanged() {
        // M5A Cannon: one bullet of 410.184, 100 shots a minute.
        let row = dps_row(&gun(100.0, 1, 410.184));
        assert!((row.base - 100.0 / 60.0 * 410.184).abs() < 1e-9);
        let without_count = gun(100.0, 0, 410.184);
        assert!((dps_row(&without_count).base - 100.0 / 60.0 * 410.184).abs() < 1e-9);
    }
}
