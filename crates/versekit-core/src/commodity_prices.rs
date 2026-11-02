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

//! Prices of the raw materials (`prices.py` of 4.12): buy or mine?
//!
//! The list `preise.json` comes from the UEX endpoint `commodities` and is
//! fetched at most once a day. Every material stands twice at UEX, refined
//! (`Iron`) and as ore (`Iron (Ore)`); both forms are kept under the same
//! key and the question decides which one counts.
//!
//! A good bought at a terminal always has quality 500.

use crate::jsonio::Value;
use crate::ordered::OrderedMap;
use crate::paths::Paths;
use crate::stock_data::{norm_material, py_float_value, text_of};
use crate::stock_fetch::{Fetch, switched_off};
use crate::uex::{self, DAY, Store};

/// The endpoint of the commodities.
pub const SOURCE: &str = "https://api.uexcorp.uk/2.0/commodities";
/// The file in the data folder.
pub const CACHE: &str = "preise.json";
/// The format number.
pub const FORMAT: i64 = 1;
/// The list counts as fresh for a day.
pub const SHELF_LIFE: f64 = DAY;
/// The quality of every good bought at a terminal.
pub const BUY_QUALITY: i64 = 500;

const STORE: Store = Store::new(CACHE, FORMAT, SHELF_LIFE);

/// One form of a material in the list.
#[derive(Clone, Debug, PartialEq)]
pub struct Form {
    /// The name as UEX writes it: `Iron` or `Iron (Ore)`.
    pub name: String,
    /// What the terminal asks per SCU; `0` means: not for sale.
    pub buy: f64,
    /// What the terminal pays per SCU.
    pub sell: f64,
}

/// The stored list.
#[derive(Clone, Debug, Default)]
pub struct Prices {
    goods: OrderedMap<Vec<Form>>,
}

/// What a material costs and brings.
#[derive(Clone, Debug, PartialEq)]
pub struct Price {
    /// The price to buy per SCU; `0` means: only to mine, not for sale.
    pub buy: f64,
    pub sell: f64,
    /// The form the price is of.
    pub form: String,
}

/// How old the stored list is in seconds, or `None`.
pub fn age(paths: &Paths, now: f64) -> Option<f64> {
    STORE.age(paths, now)
}

/// Fetches the list when it is missing or older than a day. `Ok(true)` when it
/// was fetched, `Ok(false)` when it was fresh, `Err` where nothing could be
/// fetched; a failed fetch is no loud failure (the page works without prices).
pub fn update(paths: &Paths, now: f64, fetch: Fetch<'_>) -> bool {
    if switched_off(&paths.host) {
        return false;
    }
    if !STORE.stale(paths, now) {
        return true;
    }
    let Some(data) = uex::fetch("prices", SOURCE, fetch) else { return false };
    let rows = uex::rows(&data);
    if rows.is_empty() {
        return false;
    }
    // Both forms stay: whoever overwrites gets one of them by chance.
    let mut slim: OrderedMap<Vec<Form>> = OrderedMap::new();
    for row in rows {
        let name = uex::stripped(row, "name");
        if name.is_empty() {
            continue;
        }
        let (Some(buy), Some(sell)) = (uex::number(row, "price_buy"), uex::number(row, "price_sell")) else {
            return false;
        };
        slim.entry_or_default(&norm_material(&name)).push(Form { name, buy, sell });
    }
    let waren = Value::Object(
        slim.iter()
            .map(|(key, forms)| {
                let forms = forms
                    .iter()
                    .map(|form| {
                        Value::Object(vec![
                            uex::field("name", Value::Str(form.name.clone())),
                            uex::field("kauf", uex::float(form.buy)),
                            uex::field("verkauf", uex::float(form.sell)),
                        ])
                    })
                    .collect();
                (key.to_string(), Value::Array(forms))
            })
            .collect(),
    );
    STORE.save(paths, vec![uex::field("waren", waren)], false, now).is_ok()
}

impl Prices {
    /// Reads the stored file; without a usable one the list is empty.
    pub fn load(paths: &Paths) -> Prices {
        STORE.load(paths).map_or_else(Prices::default, |data| Prices::from_value(&data))
    }

    /// The list of a file already read.
    pub fn from_value(data: &Value) -> Prices {
        let mut goods = OrderedMap::new();
        if let Some(Value::Object(entries)) = data.get("waren") {
            for (key, forms) in entries {
                let Value::Array(forms) = forms else { continue };
                let forms = forms
                    .iter()
                    .map(|form| Form {
                        name: text_of(form.get("name")).to_string(),
                        buy: form.get("kauf").and_then(py_float_value).unwrap_or(0.0),
                        sell: form.get("verkauf").and_then(py_float_value).unwrap_or(0.0),
                    })
                    .collect();
                goods.set(key, forms);
            }
        }
        Prices { goods }
    }

    /// Is the list empty?
    pub fn is_empty(&self) -> bool {
        self.goods.is_empty()
    }

    /// What this raw material costs and brings, or `None` without data for it.
    ///
    /// A price to buy of `0` does not mean "free" but "not for sale": whoever
    /// needs it has to mine it. Of the two forms the cheapest one that is
    /// really for sale counts; if none is, the best price to sell comes back
    /// with `buy == 0`.
    pub fn price(&self, material: &str) -> Option<Price> {
        if self.goods.is_empty() {
            return None;
        }
        let forms = self.goods.get(&norm_material(material))?;
        if forms.is_empty() {
            return None;
        }
        let mut buyable = forms.iter().filter(|form| form.buy != 0.0);
        if let Some(first) = buyable.next() {
            let best = buyable.fold(first, |best, form| if form.buy < best.buy { form } else { best });
            return Some(Price { buy: best.buy, sell: best.sell, form: best.name.clone() });
        }
        let best = forms.iter().fold(&forms[0], |best, form| if form.sell > best.sell { form } else { best });
        Some(Price { buy: 0.0, sell: best.sell, form: best.name.clone() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prices() -> Prices {
        let data = crate::jsonio::loads(
            r#"{"waren": {"iron": [{"name": "Iron (Ore)", "kauf": 0.0, "verkauf": 1200.0},
                                   {"name": "Iron", "kauf": 2643.0, "verkauf": 2500.0}],
                          "borase": [{"name": "Borase (Ore)", "kauf": 900.0, "verkauf": 800.0},
                                     {"name": "Borase", "kauf": 0.0, "verkauf": 700.0}],
                          "quantainium": [{"name": "Quantainium (Ore)", "kauf": 0.0, "verkauf": 10.0},
                                          {"name": "Quantainium", "kauf": 0.0, "verkauf": 20.0}]}}"#,
        )
        .unwrap();
        Prices::from_value(&data)
    }

    #[test]
    fn the_cheapest_form_that_is_for_sale_counts() {
        let prices = prices();
        let iron = prices.price("Iron").unwrap();
        assert_eq!((iron.buy, iron.form.as_str()), (2643.0, "Iron"));
        let borase = prices.price("Borase").unwrap();
        assert_eq!(borase.form, "Borase (Ore)");
    }

    #[test]
    fn a_material_nobody_sells_gives_the_best_price_to_sell() {
        let quantainium = prices().price("Quantainium").unwrap();
        assert_eq!((quantainium.buy, quantainium.sell, quantainium.form.as_str()), (0.0, 20.0, "Quantainium"));
        assert!(prices().price("Nothing").is_none());
    }
}
