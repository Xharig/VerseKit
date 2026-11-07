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

//! Lots of one material in different qualities put into one slot: the
//! quality of the mix, filling a slot from the best or worst lots of the
//! stock, and how such a mix stands against the stock.
//!
//! The quality of a mix is the mean of the lots weighted by their amount:
//! 2 SCU at 865 and 5 SCU at 797 give 816. Unlike [`crate::stock_use`], a
//! mix takes from as many entries of the stock as it needs.

use crate::jsonio::{Value, map_set};
use crate::stock::py_round;
use crate::stock_data::{norm_material, text_of};
use crate::stock_use::{Order, Standing, amount_of, quality_of};

/// An amount of a material at one quality.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lot {
    pub amount: f64,
    pub quality: f64,
}

/// The quality of the lots together: the mean weighted by amount, rounded
/// to a whole number half to even; `None` without any amount.
pub fn mixed_quality(lots: &[Lot]) -> Option<f64> {
    let total: f64 = lots.iter().map(|lot| lot.amount.max(0.0)).sum();
    if total <= 1e-9 {
        return None;
    }
    let weighted: f64 = lots.iter().map(|lot| lot.amount.max(0.0) * lot.quality).sum();
    Some((weighted / total).round_ties_even())
}

/// The sum of the amounts.
pub fn total(lots: &[Lot]) -> f64 {
    lots.iter().map(|lot| lot.amount.max(0.0)).sum()
}

fn same_material(entry: &Value, wanted: &str) -> bool {
    norm_material(text_of(entry.get("material"))) == wanted
}

/// The stock of `material` at `minimum` quality or better, one lot per
/// quality, in the order the qualities first appear.
pub fn in_stock(entries: &[Value], material: &str, minimum: f64) -> Vec<Lot> {
    let wanted = norm_material(material);
    let mut lots: Vec<Lot> = Vec::new();
    for entry in entries.iter().filter(|entry| same_material(entry, &wanted)) {
        let (amount, quality) = (amount_of(entry), quality_of(entry));
        if amount <= 0.0 || quality < minimum {
            continue;
        }
        match lots.iter_mut().find(|lot| lot.quality == quality) {
            Some(lot) => lot.amount += amount,
            None => lots.push(Lot { amount, quality }),
        }
    }
    lots
}

/// What fills `needed` from `lots`, the best or the worst first: one lot per
/// quality in the order taken. Less than `needed` when the lots run out.
pub fn fill(lots: &[Lot], needed: f64, order: Order) -> Vec<Lot> {
    let mut sorted: Vec<Lot> = lots.iter().copied().filter(|lot| lot.amount > 0.0).collect();
    sorted.sort_by(|a, b| match order {
        Order::Best => b.quality.total_cmp(&a.quality),
        Order::Worst => a.quality.total_cmp(&b.quality),
    });
    let mut taken: Vec<Lot> = Vec::new();
    let mut remaining = needed;
    for lot in sorted {
        if remaining <= 1e-9 {
            break;
        }
        let amount = lot.amount.min(remaining);
        match taken.iter_mut().find(|t| t.quality == lot.quality) {
            Some(t) => t.amount += amount,
            None => taken.push(Lot { amount, quality: lot.quality }),
        }
        remaining -= amount;
    }
    taken
}

/// Takes the `parts` of a mix `count` times out of `entries` and gives the
/// amount that could not be taken. A part takes any entry at its quality or
/// better, the lowest such quality first, so the better entries stay for the
/// better parts; the parts are served from the best down.
pub fn take(entries: &mut [Value], material: &str, parts: &[Lot], count: i64) -> f64 {
    let wanted = norm_material(material);
    let mut sorted: Vec<Lot> = parts.to_vec();
    sorted.sort_by(|a, b| b.quality.total_cmp(&a.quality));
    let mut missing = 0.0;
    for part in sorted {
        let mut remaining = part.amount.max(0.0) * count.max(1) as f64;
        let mut indices: Vec<usize> = (0..entries.len())
            .filter(|&i| same_material(&entries[i], &wanted) && quality_of(&entries[i]) >= part.quality)
            .collect();
        indices.sort_by(|&a, &b| {
            quality_of(&entries[a])
                .total_cmp(&quality_of(&entries[b]))
                .then(amount_of(&entries[a]).total_cmp(&amount_of(&entries[b])))
        });
        for i in indices {
            if remaining <= 1e-9 {
                break;
            }
            let there = amount_of(&entries[i]);
            let step = there.min(remaining);
            if step <= 0.0 {
                continue;
            }
            if let Value::Object(map) = &mut entries[i] {
                map_set(map, "menge", Value::Float(py_round(there - step, 6)));
            }
            remaining -= step;
        }
        if remaining > 1e-9 {
            missing += remaining;
        }
    }
    missing
}

/// How a mix stands against the stock for `count` pieces: there is what
/// lies at the lowest quality of the parts or better, too low what lies
/// below it, missing what [`take`] could not get.
pub fn standing(entries: &[Value], material: &str, parts: &[Lot], count: i64) -> Standing {
    let lowest = parts.iter().map(|part| part.quality).fold(f64::INFINITY, f64::min);
    let lowest = if lowest.is_finite() { lowest } else { 0.0 };
    let mut work = entries.to_vec();
    let missing = take(&mut work, material, parts, count);
    let wanted = norm_material(material);
    let (mut there, mut too_low) = (0.0, 0.0);
    for entry in entries.iter().filter(|entry| same_material(entry, &wanted)) {
        if quality_of(entry) >= lowest {
            there += amount_of(entry);
        } else {
            too_low += amount_of(entry);
        }
    }
    Standing {
        material: material.to_string(),
        needed: total(parts) * count.max(1) as f64,
        there,
        missing: py_round(missing, 6),
        too_low,
        quality: lowest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsonio::loads;

    fn stock(text: &str) -> Vec<Value> {
        match loads(text).unwrap() {
            Value::Array(items) => items,
            _ => panic!("array expected"),
        }
    }

    fn lot(amount: f64, quality: f64) -> Lot {
        Lot { amount, quality }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn the_quality_of_a_mix_is_weighted_by_amount() {
        assert_eq!(mixed_quality(&[lot(2.0, 865.0), lot(5.0, 797.0)]), Some(816.0));
        // 0.2 SCU at 800 and 0.1 SCU at 700: 766.67.
        assert_eq!(mixed_quality(&[lot(0.2, 800.0), lot(0.1, 700.0)]), Some(767.0));
        assert_eq!(mixed_quality(&[lot(1.0, 640.0)]), Some(640.0));
        // A lot without amount weighs nothing.
        assert_eq!(mixed_quality(&[lot(1.0, 640.0), lot(0.0, 100.0)]), Some(640.0));
        assert_eq!(mixed_quality(&[]), None);
        assert_eq!(mixed_quality(&[lot(0.0, 500.0)]), None);
    }

    #[test]
    fn for_pieces_the_fill_gives_what_the_terminal_quality_gives() {
        // Pieces were always counted over all entries, so `quality_for`
        // already mixes them: both ways must agree.
        let mut data = crate::stock_data::StockData::default();
        data.add_recipe_materials([("Hadanite".to_string(), true, 5.0, 1.0)]);
        let entries = stock(
            r#"[{"material":"Hadanite","menge":3,"qualitaet":800},{"material":"Hadanite","menge":2,"qualitaet":430},
                {"material":"Hadanite","menge":4,"qualitaet":610},{"material":"Hadanite","menge":1,"qualitaet":120}]"#,
        );
        for needed in [1.0, 4.0, 6.0, 9.0] {
            for (order, best) in [(Order::Best, true), (Order::Worst, false)] {
                let terminal =
                    crate::stock_use::quality_for(&entries, &data, "Hadanite", needed, 200.0, best, Some(needed));
                let mixed = mixed_quality(&fill(&in_stock(&entries, "Hadanite", 200.0), needed, order));
                assert_eq!(mixed, terminal, "{needed} {order:?}");
            }
        }
    }

    #[test]
    fn the_best_fill_takes_the_best_lots_until_the_amount_is_reached() {
        let lots = [lot(5.0, 797.0), lot(2.0, 865.0), lot(4.0, 610.0)];
        let best = fill(&lots, 3.0, Order::Best);
        assert_eq!(best, vec![lot(2.0, 865.0), lot(1.0, 797.0)]);
        assert_eq!(mixed_quality(&best), Some(842.0));
        let worst = fill(&lots, 3.0, Order::Worst);
        assert_eq!(worst, vec![lot(3.0, 610.0)]);
        // Not enough: everything that is there.
        let short = fill(&lots, 20.0, Order::Best);
        assert!(close(total(&short), 11.0));
    }

    #[test]
    fn the_stock_gives_one_lot_per_quality_at_the_minimum_or_better() {
        let entries = stock(
            r#"[{"material":"Iron","menge":1.0,"qualitaet":800},{"material":"Iron","menge":0.5,"qualitaet":800},
                {"material":"iron","menge":2.0,"qualitaet":300},{"material":"Gold","menge":9.0,"qualitaet":900},
                {"material":"Iron","menge":0,"qualitaet":950}]"#,
        );
        assert_eq!(in_stock(&entries, "Iron", 0.0), vec![lot(1.5, 800.0), lot(2.0, 300.0)]);
        assert_eq!(in_stock(&entries, "Iron", 500.0), vec![lot(1.5, 800.0)]);
    }

    #[test]
    fn a_mix_takes_from_several_entries_and_keeps_the_better_ones_for_the_better_parts() {
        let mut entries = stock(
            r#"[{"material":"Iron","menge":0.15,"qualitaet":820},{"material":"Iron","menge":0.1,"qualitaet":810},
                {"material":"Iron","menge":1.0,"qualitaet":700}]"#,
        );
        let parts = [lot(0.1, 700.0), lot(0.2, 800.0)];
        assert!(close(take(&mut entries, "Iron", &parts, 1), 0.0));
        // The part at 800 needs both entries above it; the one at 700 takes the 700.
        assert!(close(amount_of(&entries[0]), 0.05));
        assert!(close(amount_of(&entries[1]), 0.0));
        assert!(close(amount_of(&entries[2]), 0.9));
    }

    #[test]
    fn what_a_mix_cannot_get_is_missing() {
        let entries = stock(
            r#"[{"material":"Iron","menge":0.1,"qualitaet":800},{"material":"Iron","menge":0.5,"qualitaet":300}]"#,
        );
        let parts = [lot(0.2, 800.0), lot(0.1, 700.0)];
        let s = standing(&entries, "Iron", &parts, 2);
        assert!(close(s.needed, 0.6));
        assert!(close(s.missing, 0.5));
        assert!(close(s.there, 0.1));
        assert!(close(s.too_low, 0.5));
        assert_eq!(s.quality, 700.0);
    }
}
