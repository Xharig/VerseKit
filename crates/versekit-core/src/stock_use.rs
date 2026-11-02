// Verse-Kit — companion for Star Citizen.
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

//! What a recipe takes from the own stock: whether the stock covers it,
//! which quality the crafting terminal would take, and taking the
//! ingredients out (`materials.check`, `deduct`, `quality_for`,
//! `best_quality`, `worst_quality`, `amount_with_quality` of 4.12).
//!
//! ⚠ An SCU entry is one crate. Every portion of a recipe comes out of
//! **one** crate that holds it whole; crates are never put together.
//! Pieces (gems) are counted over all entries. Nothing is taken when
//! anything is missing.

use crate::jsonio::{Value, map_set};
use crate::stock::py_round;
use crate::stock_data::{StockData, norm_material, py_float_value, text_of, truthy};

/// One ingredient of a recipe: slot, material, amount per piece, lowest quality.
#[derive(Clone, Debug, PartialEq)]
pub struct Ingredient {
    pub slot: String,
    pub material: String,
    pub amount: f64,
    pub quality: f64,
}

/// How an ingredient stands against the stock (`materials.check`).
#[derive(Clone, Debug, PartialEq)]
pub struct Standing {
    pub material: String,
    /// The amount needed, already multiplied by the number of pieces.
    pub needed: f64,
    /// The amount that is there in the quality asked for.
    pub there: f64,
    /// The amount that is missing, rounded to six places.
    pub missing: f64,
    /// The amount that is there but below the quality asked for.
    pub too_low: f64,
    pub quality: f64,
}

/// Which end of the qualities a material is taken from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    Best,
    Worst,
}

/// `float(entry.get(key) or 0)`, `0.0` where that fails.
fn num(entry: &Value, key: &str) -> f64 {
    let v = entry.get(key);
    if !truthy(v) {
        return 0.0;
    }
    v.and_then(py_float_value).unwrap_or(0.0)
}

/// The quality of a stock entry, 0 where it has none.
pub fn quality_of(entry: &Value) -> f64 {
    num(entry, "qualitaet")
}

/// The amount of a stock entry, 0 where it has none.
pub fn amount_of(entry: &Value) -> f64 {
    num(entry, "menge")
}

fn same_material(entry: &Value, wanted: &str) -> bool {
    norm_material(text_of(entry.get("material"))) == wanted
}

fn set_amount(entry: &mut Value, amount: f64) {
    if let Value::Object(map) = entry {
        map_set(map, "menge", Value::Float(amount));
    }
}

/// The crate the terminal would take a portion from (`materials._pick_crate`):
/// at least `minimum` quality, the whole portion in it. `Best` takes the
/// highest quality, otherwise the lowest; at equal quality the smallest
/// crate that is enough, then the older one.
pub fn pick_crate(entries: &[Value], wanted: &str, minimum: f64, portion: f64, order: Option<Order>) -> Option<usize> {
    let mut candidates: Vec<usize> = (0..entries.len())
        .filter(|&i| {
            let p = &entries[i];
            same_material(p, wanted) && quality_of(p) >= minimum && amount_of(p) + 1e-9 >= portion
        })
        .collect();
    if candidates.is_empty() {
        return None;
    }
    candidates.sort_by(|&a, &b| {
        let (qa, qb) = (quality_of(&entries[a]), quality_of(&entries[b]));
        let by_quality = if order == Some(Order::Best) { qb.total_cmp(&qa) } else { qa.total_cmp(&qb) };
        by_quality.then(amount_of(&entries[a]).total_cmp(&amount_of(&entries[b])))
    });
    candidates.first().copied()
}

/// What `take` could not get: (comparable name, name, missing amount) in the
/// order the first miss happened.
type Missing = Vec<(String, String, f64)>;

/// Takes the ingredients `count` times out of `entries` (`materials._take`);
/// changes `entries`.
fn take(
    entries: &mut [Value],
    ingredients: &[Ingredient],
    count: i64,
    order: &dyn Fn(&str) -> Option<Order>,
    data: &StockData,
) -> Missing {
    let mut missing: Missing = Vec::new();
    for _round in 0..count.max(1) {
        for ingredient in ingredients {
            let wanted = norm_material(&ingredient.material);
            let portion = ingredient.amount;
            let minimum = ingredient.quality;
            if portion <= 0.0 {
                continue;
            }
            let got;
            if data.is_piece(&ingredient.material) {
                let mut remaining = portion;
                let mut indices: Vec<usize> = (0..entries.len()).collect();
                if order(&wanted) == Some(Order::Best) {
                    indices.sort_by(|&a, &b| quality_of(&entries[b]).total_cmp(&quality_of(&entries[a])));
                } else {
                    indices.sort_by(|&a, &b| quality_of(&entries[a]).total_cmp(&quality_of(&entries[b])));
                }
                for i in indices {
                    if remaining <= 1e-9 {
                        break;
                    }
                    if !same_material(&entries[i], &wanted) || quality_of(&entries[i]) < minimum {
                        continue;
                    }
                    let step = amount_of(&entries[i]).min(remaining);
                    let left = py_round(amount_of(&entries[i]) - step, 6);
                    set_amount(&mut entries[i], left);
                    remaining -= step;
                }
                got = portion - remaining;
            } else {
                match pick_crate(entries, &wanted, minimum, portion, order(&wanted)) {
                    Some(i) => {
                        let left = py_round(amount_of(&entries[i]) - portion, 6);
                        set_amount(&mut entries[i], left);
                        got = portion;
                    }
                    None => got = 0.0,
                }
            }
            if portion - got > 1e-9 {
                let short = portion - got;
                match missing.iter_mut().find(|(key, _, _)| *key == wanted) {
                    Some(slot) => slot.2 += short,
                    None => missing.push((wanted.clone(), ingredient.material.clone(), short)),
                }
            }
        }
    }
    missing
}

/// How much of `material` is there: (in the quality asked for, below it)
/// (`materials.amount_with_quality`).
pub fn amount_with_quality(entries: &[Value], material: &str, min_quality: f64) -> (f64, f64) {
    let wanted = norm_material(material);
    let (mut suitable, mut too_low) = (0.0, 0.0);
    for p in entries.iter().filter(|p| same_material(p, &wanted)) {
        if quality_of(p) >= min_quality {
            suitable += amount_of(p);
        } else {
            too_low += amount_of(p);
        }
    }
    (suitable, too_low)
}

/// The sum of an entry's amount over all entries of the material
/// (`materials.amount_of`).
pub fn amount_total(entries: &[Value], material: &str) -> f64 {
    let wanted = norm_material(material);
    crate::text::py_sum(entries.iter().filter(|p| same_material(p, &wanted)).map(|p| num(p, "menge")))
}

fn usable_qualities<'a>(entries: &'a [Value], material: &str, min_quality: f64) -> impl Iterator<Item = f64> + 'a {
    let wanted = norm_material(material);
    entries
        .iter()
        .filter(move |p| same_material(p, &wanted) && num(p, "menge") > 0.0)
        .map(quality_of)
        .filter(move |q| *q >= min_quality)
}

/// The highest quality of the material that is usable (`best_quality`).
pub fn best_quality(entries: &[Value], material: &str, min_quality: f64) -> Option<f64> {
    usable_qualities(entries, material, min_quality).fold(None, |best, q| Some(best.map_or(q, |b: f64| b.max(q))))
}

/// The lowest quality of the material that is usable (`worst_quality`).
pub fn worst_quality(entries: &[Value], material: &str, min_quality: f64) -> Option<f64> {
    usable_qualities(entries, material, min_quality).fold(None, |worst, q| Some(worst.map_or(q, |w: f64| w.min(q))))
}

/// Python's `round(x)` of a non-negative average.
fn rounded(x: f64) -> f64 {
    x.round_ties_even()
}

/// The average quality the crafting terminal would take for `needed` of the
/// material (`materials.quality_for`). `portion` is the amount per piece;
/// without it `needed` is one portion. `None` without a stock to take from.
pub fn quality_for(
    entries: &[Value],
    data: &StockData,
    material: &str,
    needed: f64,
    min_quality: f64,
    best: bool,
    portion: Option<f64>,
) -> Option<f64> {
    let wanted = norm_material(material);
    let piece = data.is_piece(material);
    let portion = if piece { portion } else { Some(portion.filter(|p| *p != 0.0).unwrap_or(needed)) };
    if let Some(portion) = portion.filter(|p| *p > 0.0)
        && !piece
    {
        let mut work: Vec<Value> = entries.iter().filter(|p| same_material(p, &wanted)).cloned().collect();
        let pieces = ((needed / portion).round_ties_even() as i64).max(1);
        let (mut total, mut weighted) = (0.0, 0.0);
        for _ in 0..pieces {
            let order = Some(if best { Order::Best } else { Order::Worst });
            let Some(i) = pick_crate(&work, &wanted, min_quality, portion, order) else { break };
            let left = py_round(amount_of(&work[i]) - portion, 6);
            set_amount(&mut work[i], left);
            total += portion;
            weighted += portion * quality_of(&work[i]);
        }
        return (total > 0.0).then(|| rounded(weighted / total));
    }
    let mut per_quality: Vec<(f64, f64)> = Vec::new();
    for p in entries.iter().filter(|p| same_material(p, &wanted)) {
        let (amount, q) = (num(p, "menge"), quality_of(p));
        if amount <= 0.0 || q < min_quality {
            continue;
        }
        match per_quality.iter_mut().find(|(k, _)| *k == q) {
            Some(slot) => slot.1 += amount,
            None => per_quality.push((q, amount)),
        }
    }
    if per_quality.is_empty() {
        return None;
    }
    let mut remaining = needed;
    if remaining <= 1e-9 {
        let qualities = per_quality.iter().map(|(q, _)| *q);
        return Some(if best { qualities.fold(f64::MIN, f64::max) } else { qualities.fold(f64::MAX, f64::min) });
    }
    per_quality.sort_by(|a, b| if best { b.0.total_cmp(&a.0) } else { a.0.total_cmp(&b.0) });
    let (mut total, mut weighted) = (0.0, 0.0);
    for (q, amount) in per_quality {
        let taken = amount.min(remaining);
        total += taken;
        weighted += taken * q;
        remaining -= taken;
        if remaining <= 1e-9 {
            break;
        }
    }
    (total > 0.0).then(|| rounded(weighted / total))
}

/// How every ingredient stands against the stock for `count` pieces
/// (`materials.check`); every ingredient comes back, also those that are there.
pub fn check(entries: &[Value], data: &StockData, ingredients: &[Ingredient], count: i64) -> Vec<Standing> {
    let factor = count.max(1);
    let mut work: Vec<Value> = entries.to_vec();
    let missing = take(&mut work, ingredients, factor, &|_| None, data);
    ingredients
        .iter()
        .map(|ingredient| {
            let key = norm_material(&ingredient.material);
            let (mut suitable, too_low) = amount_with_quality(entries, &ingredient.material, ingredient.quality);
            if !data.is_piece(&ingredient.material) {
                suitable = crate::text::py_sum(
                    entries
                        .iter()
                        .filter(|p| same_material(p, &key) && quality_of(p) >= ingredient.quality)
                        .filter(|p| amount_of(p) + 1e-9 >= ingredient.amount)
                        .map(amount_of),
                );
            }
            let short = missing.iter().find(|(k, _, _)| *k == key).map_or(0.0, |(_, _, s)| *s);
            Standing {
                material: ingredient.material.clone(),
                needed: ingredient.amount * factor as f64,
                there: suitable,
                missing: py_round(short, 6),
                too_low,
                quality: ingredient.quality,
            }
        })
        .collect()
}

/// Takes the ingredients out of `entries` `count` times (`materials.deduct`).
///
/// Nothing is changed when anything is missing; the list of what is missing
/// (name, amount) comes back then. Empty entries are dropped when it worked.
/// `order` names the materials taken from the best or worst end.
pub fn deduct(
    entries: &mut Vec<Value>,
    data: &StockData,
    ingredients: &[Ingredient],
    count: i64,
    order: &[(String, Order)],
) -> Result<(), Vec<(String, f64)>> {
    let keyed: Vec<(String, Order)> = order.iter().map(|(m, o)| (norm_material(m), *o)).collect();
    let lookup = |wanted: &str| keyed.iter().rev().find(|(m, _)| m == wanted).map(|(_, o)| *o);
    let mut work = entries.clone();
    let missing = take(&mut work, ingredients, count, &lookup, data);
    if !missing.is_empty() {
        return Err(missing.into_iter().map(|(_, name, short)| (name, py_round(short, 6))).collect());
    }
    work.retain(|p| amount_of(p) > 1e-9);
    *entries = work;
    Ok(())
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

    fn ingredient(material: &str, amount: f64, quality: f64) -> Ingredient {
        Ingredient { slot: String::new(), material: material.into(), amount, quality }
    }

    #[test]
    fn two_small_crates_do_not_make_a_big_portion() {
        let data = StockData::default();
        let entries = stock(
            r#"[{"material":"Iron","menge":2.0,"qualitaet":600},{"material":"Iron","menge":2.0,"qualitaet":600}]"#,
        );
        let wanted = [ingredient("Iron", 3.0, 0.0)];
        let standing = check(&entries, &data, &wanted, 1);
        assert_eq!(standing[0].missing, 3.0);
        assert_eq!(standing[0].there, 0.0);
    }

    #[test]
    fn nothing_is_taken_when_anything_is_missing() {
        let data = StockData::default();
        let mut entries = stock(
            r#"[{"material":"Iron","menge":1.0,"qualitaet":600},{"material":"Gold","menge":0.1,"qualitaet":600}]"#,
        );
        let before = entries.clone();
        let wanted = [ingredient("Iron", 0.5, 0.0), ingredient("Gold", 0.5, 0.0)];
        let result = deduct(&mut entries, &data, &wanted, 1, &[]);
        assert_eq!(result, Err(vec![("Gold".to_string(), 0.5)]));
        assert_eq!(entries, before);
    }

    #[test]
    fn the_lowest_quality_that_is_enough_is_taken_first() {
        let data = StockData::default();
        let mut entries = stock(
            r#"[{"material":"Iron","menge":1.0,"qualitaet":900},{"material":"Iron","menge":1.0,"qualitaet":520}]"#,
        );
        deduct(&mut entries, &data, &[ingredient("Iron", 0.4, 500.0)], 1, &[]).unwrap();
        assert!((amount_of(&entries[0]) - 1.0).abs() < 1e-9);
        assert!((amount_of(&entries[1]) - 0.6).abs() < 1e-9);
    }

    #[test]
    fn best_order_takes_the_highest_quality() {
        let data = StockData::default();
        let mut entries = stock(
            r#"[{"material":"Iron","menge":1.0,"qualitaet":900},{"material":"Iron","menge":1.0,"qualitaet":520}]"#,
        );
        deduct(&mut entries, &data, &[ingredient("Iron", 0.4, 500.0)], 1, &[("Iron".into(), Order::Best)]).unwrap();
        assert!((amount_of(&entries[0]) - 0.6).abs() < 1e-9);
    }

    #[test]
    fn the_quality_the_terminal_takes_is_a_weighted_average_for_pieces() {
        let mut data = StockData::default();
        data.add_recipe_materials([("Hadanite".to_string(), true, 5.0, 1.0)]);
        let entries = stock(
            r#"[{"material":"Hadanite","menge":3,"qualitaet":800},{"material":"Hadanite","menge":3,"qualitaet":400}]"#,
        );
        let best = quality_for(&entries, &data, "Hadanite", 4.0, 0.0, true, Some(4.0));
        assert_eq!(best, Some(((3.0 * 800.0 + 1.0 * 400.0) / 4.0_f64).round()));
        let worst = quality_for(&entries, &data, "Hadanite", 4.0, 0.0, false, Some(4.0));
        assert_eq!(worst, Some(((3.0 * 400.0 + 1.0 * 800.0) / 4.0_f64).round()));
    }

    #[test]
    fn a_crate_below_the_quality_counts_as_too_low_not_as_there() {
        let data = StockData::default();
        let entries = stock(r#"[{"material":"Iron","menge":2.0,"qualitaet":300}]"#);
        let standing = check(&entries, &data, &[ingredient("Iron", 1.0, 500.0)], 1);
        assert_eq!((standing[0].there, standing[0].too_low, standing[0].missing), (0.0, 2.0, 1.0));
    }
}
