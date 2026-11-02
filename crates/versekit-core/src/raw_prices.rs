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

//! What a raw material costs: buy it or mine it (`prices.price` of 4.12),
//! read from the file [`stock_fetch::PRICES_FILE`](crate::stock_fetch::PRICES_FILE).
//!
//! ⚠ A buying price of 0 means "cannot be bought", not "free". A terminal
//! sells every purchase at quality 500, the point where quality changes
//! nothing; better than that only comes from ore one mined oneself.

use std::collections::HashMap;

use crate::jsonio::{self, Value};
use crate::paths::Paths;
use crate::stock_data::{format_is, norm_material, py_float_value, text_of};
use crate::stock_fetch::PRICES_FILE;

/// The quality of everything bought at a terminal.
pub const BUY_QUALITY: i64 = 500;

/// What one raw material costs and brings.
#[derive(Clone, Debug, PartialEq)]
pub struct Price {
    /// aUEC per SCU a terminal asks; 0 where it cannot be bought.
    pub buy: f64,
    /// aUEC per SCU a terminal pays.
    pub sell: f64,
    /// The name UEX gives that form (`Iron` or `Iron (Ore)`).
    pub form: String,
}

/// The prices read from the file; empty without a usable file.
#[derive(Clone, Debug, Default)]
pub struct RawPrices {
    goods: HashMap<String, Vec<Price>>,
}

impl RawPrices {
    /// Reads the file of the data folder.
    pub fn load(paths: &Paths) -> RawPrices {
        let Ok(store) = jsonio::read_json(&paths.app_file(PRICES_FILE)) else { return RawPrices::default() };
        if !format_is(&store, 1) {
            return RawPrices::default();
        }
        let mut goods = HashMap::new();
        if let Some(Value::Object(map)) = store.get("waren") {
            for (key, forms) in map {
                let Value::Array(forms) = forms else { continue };
                let prices = forms
                    .iter()
                    .map(|form| Price {
                        buy: form.get("kauf").and_then(py_float_value).unwrap_or(0.0),
                        sell: form.get("verkauf").and_then(py_float_value).unwrap_or(0.0),
                        form: text_of(form.get("name")).to_string(),
                    })
                    .collect();
                goods.insert(key.clone(), prices);
            }
        }
        RawPrices { goods }
    }

    /// Whether any price is known.
    pub fn is_empty(&self) -> bool {
        self.goods.is_empty()
    }

    /// What `material` costs: of the forms the cheapest that can be bought,
    /// else the best selling price with a buying price of 0. `None` without
    /// prices for the material.
    pub fn price(&self, material: &str) -> Option<Price> {
        let forms = self.goods.get(&norm_material(material))?;
        let cheapest = forms.iter().filter(|form| form.buy != 0.0).fold(None::<&Price>, |best, form| match best {
            Some(best) if best.buy <= form.buy => Some(best),
            _ => Some(form),
        });
        if let Some(form) = cheapest {
            return Some(Price { buy: form.buy, sell: form.sell, form: form.form.clone() });
        }
        let best = forms.iter().fold(None::<&Price>, |best, form| match best {
            Some(best) if best.sell >= form.sell => Some(best),
            _ => Some(form),
        })?;
        Some(Price { buy: 0.0, sell: best.sell, form: best.form.clone() })
    }

    /// Prices made up for a test.
    #[cfg(test)]
    pub fn from_forms(forms: &[(&str, f64, f64)]) -> RawPrices {
        let mut goods: HashMap<String, Vec<Price>> = HashMap::new();
        for (name, buy, sell) in forms {
            goods.entry(norm_material(name)).or_default().push(Price {
                buy: *buy,
                sell: *sell,
                form: (*name).to_string(),
            });
        }
        RawPrices { goods }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn of_two_forms_the_cheapest_that_can_be_bought_counts() {
        let prices = RawPrices::from_forms(&[("Iron", 2643.0, 2000.0), ("Iron (Ore)", 0.0, 1500.0)]);
        let price = prices.price("iron").unwrap();
        assert_eq!((price.buy, price.form.as_str()), (2643.0, "Iron"));
    }

    #[test]
    fn a_material_that_cannot_be_bought_has_a_buying_price_of_zero() {
        let prices = RawPrices::from_forms(&[("Quantainium", 0.0, 100.0), ("Quantainium (Ore)", 0.0, 140.0)]);
        let price = prices.price("Quantainium").unwrap();
        assert_eq!((price.buy, price.sell, price.form.as_str()), (0.0, 140.0, "Quantainium (Ore)"));
        assert!(prices.price("Gold").is_none());
    }
}
