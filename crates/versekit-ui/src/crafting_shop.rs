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

//! "Buy it finished" on the crafting page (`pages._shop_row` of 4.12): the
//! cheapest shop of the opened item, looked up in the background.
//!
//! ⚠ Three states, three displays: not looked up yet shows nothing (the line
//! comes when the answer is there), a part UEX does not know shows nothing,
//! shops known show the cheapest with its place. "Nowhere in trade" is never
//! claimed: UEX has gaps, and a gap in foreign data says nothing about the game.

use std::collections::HashSet;

use egui::{RichText, Ui};
use versekit_core::formats;
use versekit_core::language::{self, Arg, t, tf};
use versekit_core::paths::Paths;
use versekit_core::shops::{self, Catalog, Shops};
use versekit_core::theme::Scheme;

use crate::color;
use crate::trade::{self, Job, Polled};

/// What a click on the line asks of the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Show the shops page.
    ToShops,
}

/// The shop prices of the items looked at, and the lookup that runs.
pub struct ShopLine {
    paths: Option<Paths>,
    shops: Shops,
    job: Option<(String, Job<bool>)>,
    /// The codes whose lookup ended without a result: not asked again in this session.
    failed: HashSet<String>,
}

impl ShopLine {
    /// With the stored prices of the data folder; without paths nothing is looked up.
    pub fn new(paths: Option<&Paths>) -> ShopLine {
        let paths = paths.map(|p| Paths::new(p.host.clone()));
        let shops = paths.as_ref().map(Shops::load).unwrap_or_default();
        ShopLine { paths, shops, job: None, failed: HashSet::new() }
    }

    /// Takes in a lookup that ended.
    fn poll(&mut self) {
        let Some((code, job)) = &self.job else { return };
        match job.poll() {
            Polled::Running => {}
            Polled::Done(true) => {
                self.job = None;
                if let Some(paths) = &self.paths {
                    self.shops = Shops::load(paths);
                }
            }
            Polled::Done(false) | Polled::Lost => {
                self.failed.insert(code.clone());
                self.job = None;
            }
        }
    }

    fn start(&mut self, ctx: &egui::Context, code: &str, name: &str) {
        let Some(paths) = self.paths.as_ref().map(|p| Paths::new(p.host.clone())) else { return };
        let (own_code, own_name) = (code.to_string(), name.to_string());
        self.job = Some((
            code.to_string(),
            Job::spawn(ctx, "crafting-shop", move || {
                let mut book = Shops::load(&paths);
                let mut catalog = Catalog::load(&paths);
                shops::fetch_part(
                    &paths,
                    &mut book,
                    &mut catalog,
                    (&own_code, &own_name),
                    false,
                    trade::now(),
                    &trade::fetcher(),
                )
            }),
        ));
    }

    /// The line for the item with this identifier; `name` is the fallback for a part UEX
    /// lists under another identifier.
    pub fn show(&mut self, ui: &mut Ui, scheme: &Scheme, code: &str, name: &str) -> Option<Action> {
        if code.is_empty() {
            return None;
        }
        self.poll();
        if !self.shops.known(code) {
            if self.job.is_none() && !self.failed.contains(code) {
                self.start(ui.ctx(), code, name);
            }
            return None;
        }
        let (price, shop, place) = self.shops.cheapest(code)?;
        let wo = [shop, place].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ");
        let text = tf(
            "s_he_fertig_kaufen",
            &[("0", Arg::from(formats::money(language::current(), price))), ("1", Arg::from(wo))],
        );
        let mut action = None;
        ui.horizontal_wrapped(|row| {
            row.label(RichText::new(text).small().color(color(scheme.sub)));
            if row.add(egui::Link::new(RichText::new(t("hf_laeden")).small())).clicked() {
                action = Some(Action::ToShops);
            }
        });
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use versekit_core::language::{Choice, set_language};
    use versekit_core::paths::Host;
    use versekit_core::pypath::Flavor;

    fn draw(state: &mut ShopLine, code: &str) -> Vec<String> {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let scheme = versekit_core::theme::current();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            state.show(ui, scheme, code, "Frostline");
        });
        output.textures_delta.clear();
        fn collect(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
            match shape {
                egui::epaint::Shape::Text(text) => out.push(text.galley.text().to_string()),
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut out));
        out
    }

    fn paths(name: &str) -> Paths {
        let dir = std::env::temp_dir().join(format!("versekit-fertig-{}-{name}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let text = dir.to_string_lossy().into_owned();
        Paths::new(Host::new(Flavor::native(), &[(versekit_core::legacy::ENV_HOME, &text)], &text))
    }

    #[test]
    fn the_cheapest_shop_stands_there_and_nothing_stands_where_nothing_is_known() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let paths = paths("known");
        let file = paths.app_file(shops::CACHE);
        std::fs::write(
            &file,
            r#"{"format": 1, "geholt": 1.0, "teile": {"abc": {"geholt": 1.0, "zeilen": [
                {"laden": "Dumper's Depot", "ort": "Area18", "system": "Stanton", "preis": 22700.0, "zustand": 100},
                {"laden": "Centermass", "ort": "Lorville", "system": "Stanton", "preis": 12500.0, "zustand": 100}]},
              "leer": {"geholt": 1.0, "zeilen": []}}}"#,
        )
        .unwrap();
        let mut line = ShopLine::new(Some(&paths));
        let shown = draw(&mut line, "abc");
        let expected =
            tf("s_he_fertig_kaufen", &[("0", Arg::from("12.500 aUEC")), ("1", Arg::from("Centermass · Lorville"))]);
        assert!(shown.contains(&expected), "{expected}: {shown:?}");
        assert!(shown.contains(&t("hf_laeden")), "{shown:?}");
        // A part UEX does not know shows nothing, and so does one without an identifier.
        assert!(draw(&mut line, "leer").is_empty());
        assert!(draw(&mut line, "").is_empty());
        // Without a data folder nothing is looked up.
        let mut none = ShopLine::new(None);
        assert!(draw(&mut none, "abc").is_empty() && none.job.is_none());
    }
}
