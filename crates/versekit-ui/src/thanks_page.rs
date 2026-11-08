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

//! The page "Danke & Lizenzen": who built the program, what it bundles, what
//! it fetches from others, and the people whose feedback became something
//! that would not exist otherwise, then the fan content notice.
//!
//! The lists below are the source of the page: every source and every person
//! the program names stands here, with the licence, the address and the texts.
//! `tools/parity_thanks.py` and `versekit-parity thanks` check them against
//! the lists of the Python version, so nothing named there goes missing.

use std::collections::HashSet;

use egui::{RichText, Role, Sense, Ui, WidgetInfo};
use versekit_core::browser;
use versekit_core::credits::{
    BUNDLED, Credit, FAN_CONTENT, FAN_CONTENT_HEAD, FETCHED, LICENCE_TEXT_HEAD, LICENCE_TEXT_NAME, Licence, PEOPLE,
    Person, plain,
};
use versekit_core::language::{Arg, t, tf};
use versekit_core::theme::Scheme;

use crate::color;
use crate::layout::GAP;
use crate::widgets;

/// Where the page links to for the project itself.
const SITE: (&str, &str) = ("versekit.xharig.com", "https://versekit.xharig.com/");
const SOURCE: (&str, &str) = ("github.com/Xharig/VerseKit", "https://github.com/Xharig/VerseKit");
const RSI_PROFILE: &str = "https://robertsspaceindustries.com/en/citizens/Xharig";

/// What the page keeps: which people and which licence texts are unfolded, and the last link's answer.
pub struct ThanksState {
    open: HashSet<&'static str>,
    licences: HashSet<&'static str>,
    version: String,
    open_url: fn(&str) -> bool,
    answer: Option<String>,
}

impl ThanksState {
    pub fn new(version: &str) -> ThanksState {
        ThanksState {
            open: HashSet::new(),
            licences: HashSet::new(),
            version: version.to_string(),
            open_url: browser::open_in_browser,
            answer: None,
        }
    }

    /// Replaces what opens an address (tests do not open a browser).
    pub fn with_opener(mut self, open_url: fn(&str) -> bool) -> ThanksState {
        self.open_url = open_url;
        self
    }

    /// The names of the people who are unfolded.
    pub fn unfolded(&self) -> Vec<&'static str> {
        PEOPLE.iter().map(|p| p.name).filter(|n| self.open.contains(n)).collect()
    }

    /// The names of the sources whose licence text is unfolded.
    pub fn unfolded_licences(&self) -> Vec<&'static str> {
        BUNDLED.iter().map(|c| c.name).filter(|n| self.licences.contains(n)).collect()
    }

    fn follow(&mut self, url: &str) {
        let key = if (self.open_url)(url) { "s_ub_auf" } else { "s_ub_auf_nein" };
        self.answer = Some(tf(key, &[("0", Arg::from(url))]));
    }
}

fn licence_text(licence: Licence) -> String {
    match licence {
        Licence::Named(name) => name.to_string(),
        Licence::None => t("s_dk_keine_lizenz"),
    }
}

fn small(ui: &mut Ui, text: &str, tone: egui::Color32) {
    ui.add(egui::Label::new(RichText::new(text).small().color(tone)).wrap());
}

/// A group of the page: a card with its title, below it the hint in small muted text, then what the group holds.
fn group<R>(ui: &mut Ui, scheme: &Scheme, title: &str, hint: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.add_space(GAP);
    widgets::card(ui, scheme, |ui| {
        ui.set_width(ui.available_width());
        widgets::card_title(ui, scheme, title);
        ui.add_space(2.0);
        small(ui, hint, color(scheme.sub));
        ui.add_space(GAP);
        add(ui)
    })
    .inner
}

/// The entries of a group one below the other in a list, a line between two.
fn entries<T>(ui: &mut Ui, scheme: &Scheme, items: &[T], mut entry: impl FnMut(&mut Ui, &T)) {
    widgets::list_frame(ui, scheme, |list| {
        list.set_width(list.available_width());
        for (k, item) in items.iter().enumerate() {
            if k > 0 {
                list.separator();
            }
            entry(list, item);
        }
    });
}

/// A link that opens in the browser; the answer stands at the top of the first card.
/// A long address is broken where the line ends, never cut off or pushed
/// out of the page.
fn link(ui: &mut Ui, scheme: &Scheme, state: &mut ThanksState, shown: &str, url: &str) {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let mut job = egui::text::LayoutJob::simple(shown.to_string(), font, color(scheme.accent), ui.available_width());
    job.wrap.break_anywhere = true;
    let response = ui.add(egui::Link::new(job)).on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| WidgetInfo::labeled(Role::Link, true, shown));
    if response.clicked() {
        state.follow(url);
    }
}

/// One source, an entry of the list of its group: the name with its licence, the text, the address, the licence
/// text to fold out.
fn credit_entry(ui: &mut Ui, scheme: &Scheme, state: &mut ThanksState, credit: &Credit) {
    ui.vertical(|ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|row| {
            row.label(RichText::new(credit.name).strong());
            widgets::badge(row, scheme, scheme.accent, &licence_text(credit.licence));
        });
        small(ui, &plain(&t(credit.text)), color(scheme.sub));
        ui.add_space(4.0);
        link(ui, scheme, state, credit.url, credit.url);
        if let Some(text) = credit.licence_text {
            ui.add_space(4.0);
            licence_fold(ui, scheme, state, credit.name, text);
        }
    });
}

/// The full licence text of a bundled source, folded under a head that opens and closes it.
fn licence_fold(ui: &mut Ui, scheme: &Scheme, state: &mut ThanksState, name: &'static str, text: &str) {
    let open = state.licences.contains(name);
    let head = ui.horizontal(|row| {
        if let Some(image) = crate::icons::themed(
            if open { "chevron-down" } else { "chevron-right" },
            scheme,
            versekit_core::theme::IconTint::Normal,
            14.0,
        ) {
            row.add(image);
        }
        row.label(RichText::new(t(LICENCE_TEXT_HEAD)).small().color(color(scheme.sub)));
    });
    let click = ui.interact(head.response.rect, ui.scope_id().with(("licence", name)), Sense::click());
    let spoken = tf(LICENCE_TEXT_NAME, &[("0", Arg::from(name))]);
    click.widget_info(|| WidgetInfo::selected(Role::Button, true, open, &spoken));
    if open {
        ui.add_space(4.0);
        ui.add(egui::Label::new(RichText::new(text.trim_end()).small().monospace().color(color(scheme.sub))).wrap());
    }
    if click.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !state.licences.remove(name) {
        state.licences.insert(name);
    }
}

/// One person, an entry of the list of the people: the head opens and closes what came of the feedback.
fn person_entry(ui: &mut Ui, scheme: &Scheme, state: &mut ThanksState, person: &Person) {
    let open = state.open.contains(person.name);
    let shown = ui.vertical(|ui| {
        ui.set_width(ui.available_width());
        let head = ui.horizontal(|row| {
            if let Some(image) = crate::icons::themed(
                if open { "chevron-down" } else { "chevron-right" },
                scheme,
                versekit_core::theme::IconTint::Normal,
                16.0,
            ) {
                row.add(image);
            }
            row.label(RichText::new(person.name).strong());
            if !person.group.is_empty() {
                widgets::badge(row, scheme, scheme.accent, person.group);
            }
        });
        let click = ui.interact(head.response.rect, ui.scope_id().with(("person", person.name)), Sense::click());
        click.widget_info(|| WidgetInfo::selected(Role::Button, true, open, person.name));
        if open {
            ui.add_space(4.0);
            for key in person.ideas {
                ui.add(egui::Label::new(plain(&t(key))).wrap());
                ui.add_space(4.0);
            }
            for key in person.found {
                small(ui, &plain(&t(key)), color(scheme.sub));
                ui.add_space(4.0);
            }
        }
        click
    });
    if shown.inner.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !state.open.remove(person.name) {
        state.open.insert(person.name);
    }
}

/// The page.
pub fn show(ui: &mut Ui, scheme: &Scheme, state: &mut ThanksState) {
    small(ui, &t("s_dkv_lead"), color(scheme.sub));

    // Who built it: first, with the picture; the page names other people's
    // work and this belongs before it. The answer of the last link stands at the top of it.
    group(ui, scheme, &t("hf_wer"), &t("s_ub_wer_h"), |ui| {
        if let Some(answer) = state.answer.clone() {
            small(ui, &answer, color(scheme.sub));
            ui.add_space(4.0);
        }
        ui.horizontal(|row| {
            row.add(
                egui::Image::from_bytes("bytes://xharig.png", include_bytes!("../../../assets/xharig.png"))
                    .fit_to_exact_size(egui::vec2(64.0, 64.0)),
            );
            row.vertical(|col| {
                // The name opens the profile on the RSI website.
                let name = col
                    .add(egui::Link::new(RichText::new("Xharig").strong().color(color(scheme.accent))))
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text(RSI_PROFILE);
                name.widget_info(|| WidgetInfo::labeled(Role::Link, true, "Xharig"));
                if name.clicked() {
                    state.follow(RSI_PROFILE);
                }
                small(col, &format!("{} {} · GPL-3.0-only", t("hf_titel"), state.version), color(scheme.sub));
                let (site, site_url) = SITE;
                link(col, scheme, state, site, site_url);
                let (source, source_url) = SOURCE;
                link(col, scheme, state, source, source_url);
            });
        });
        ui.add_space(4.0);
        small(ui, &t("s_dkv_selbst_h"), color(scheme.sub));
    });

    group(ui, scheme, &t("s_dk_dabei"), &t("s_dk_dabei_h"), |ui| {
        entries(ui, scheme, &BUNDLED, |ui, entry| credit_entry(ui, scheme, state, entry));
    });

    group(ui, scheme, &t("s_dk_extern"), &t("s_dkv_extern_h"), |ui| {
        entries(ui, scheme, &FETCHED, |ui, entry| credit_entry(ui, scheme, state, entry));
    });

    group(ui, scheme, &t("s_dk_leute"), &t("s_dkv_leute_h"), |ui| {
        small(ui, &t("s_dk_aufklappen"), color(scheme.sub));
        ui.add_space(GAP);
        entries(ui, scheme, &PEOPLE, |ui, entry| person_entry(ui, scheme, state, entry));
    });

    // The marks and the notice of the fan kit, in a card under the head of the notice.
    ui.add_space(GAP);
    widgets::card(ui, scheme, |ui| {
        ui.set_width(ui.available_width());
        widgets::card_title(ui, scheme, &t(FAN_CONTENT_HEAD));
        ui.add_space(2.0);
        small(ui, &t("s_dkv_marken"), color(scheme.sub));
        ui.add_space(GAP);
        small(ui, &t(FAN_CONTENT), color(scheme.sub));
    });
    ui.add_space(GAP);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use versekit_core::language::{Choice, set_thread_language};

    static OPENED: Mutex<Vec<String>> = Mutex::new(Vec::new());

    fn record(url: &str) -> bool {
        OPENED.lock().map(|mut list| list.push(url.to_string())).is_ok()
    }

    fn refuse(_url: &str) -> bool {
        false
    }

    /// German and held: the language is one for the whole process.
    fn german() -> std::sync::MutexGuard<'static, ()> {
        let guard = crate::language_lock();
        set_thread_language(Choice::De);
        guard
    }

    /// One frame; the accesskit nodes with role, label and the middle of their place.
    fn frame(
        ctx: &egui::Context,
        state: &mut ThanksState,
        events: Vec<egui::Event>,
    ) -> Vec<(Role, String, egui::Pos2)> {
        let scheme = versekit_core::theme::current();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 12000.0))),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| show(ui, scheme, state));
        output.textures_delta.clear();
        let Some(update) = output.platform_output.accesskit_update else { return Vec::new() };
        update
            .nodes
            .iter()
            .filter_map(|(_, n)| {
                let text = n.label().or(n.value())?.to_owned();
                let b = n.bounds()?;
                Some((n.role(), text, egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32)))
            })
            .collect()
    }

    fn click(at: egui::Pos2) -> Vec<egui::Event> {
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        };
        vec![egui::Event::PointerMoved(at), press(true), press(false)]
    }

    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        ctx
    }

    #[test]
    fn every_source_and_every_person_is_drawn() {
        let _german = german();
        let ctx = context();
        let mut state = ThanksState::new("5.0.0").with_opener(record);
        frame(&ctx, &mut state, Vec::new());
        let nodes = frame(&ctx, &mut state, Vec::new());
        let has = |name: &str| nodes.iter().any(|(_, text, _)| text.contains(name));
        for credit in BUNDLED.iter().chain(FETCHED.iter()) {
            assert!(has(credit.name), "Quelle fehlt: {}", credit.name);
        }
        for person in &PEOPLE {
            assert!(has(person.name), "Person fehlt: {}", person.name);
        }
        assert!(has("Xharig") && has("GPL-3.0-only"));
        assert!(has(&t(FAN_CONTENT_HEAD)));
        assert!(has("SC Launch Configurator (Luftwerft)") && has("Hangar-Exporter (Drachennarr)"));
    }

    #[test]
    fn a_click_on_the_author_opens_the_rsi_profile() {
        let _german = german();
        let ctx = context();
        let mut state = ThanksState::new("5.0.0").with_opener(record);
        frame(&ctx, &mut state, Vec::new());
        let nodes = frame(&ctx, &mut state, Vec::new());
        let at = nodes
            .iter()
            .find(|(role, text, _)| *role == Role::Link && text == "Xharig")
            .map(|(_, _, at)| *at)
            .expect("the author's name is a link");
        let before = OPENED.lock().map(|list| list.len()).unwrap_or(0);
        frame(&ctx, &mut state, click(at));
        let opened = OPENED.lock().map(|list| list[before..].to_vec()).unwrap_or_default();
        assert!(opened.iter().any(|url| url == RSI_PROFILE), "opened: {opened:?}");
    }

    #[test]
    fn a_click_unfolds_a_person_and_a_second_folds_it_again() {
        let _german = german();
        let ctx = context();
        let mut state = ThanksState::new("5.0.0").with_opener(record);
        let find = |nodes: &[(Role, String, egui::Pos2)], name: &str| {
            nodes.iter().find(|(role, text, _)| *role == Role::Button && text == name).map(|(_, _, at)| *at)
        };
        frame(&ctx, &mut state, Vec::new());
        let nodes = frame(&ctx, &mut state, Vec::new());
        let at = find(&nodes, "Haldjas").expect("the head of the person is a button with the name");
        assert!(state.unfolded().is_empty());
        let first = plain(&t("s_dkv_haldjas_idee")).chars().take(30).collect::<String>();
        assert!(!nodes.iter().any(|(_, text, _)| text.starts_with(&first)), "folded: no text yet");
        frame(&ctx, &mut state, click(at));
        let nodes = frame(&ctx, &mut state, Vec::new());
        assert_eq!(state.unfolded(), vec!["Haldjas"]);
        assert!(nodes.iter().any(|(_, text, _)| text.starts_with(&first)), "unfolded: the text of the idea");
        let at = find(&nodes, "Haldjas").expect("still there");
        frame(&ctx, &mut state, click(at));
        frame(&ctx, &mut state, Vec::new());
        assert!(state.unfolded().is_empty());
    }

    #[test]
    fn every_bundled_box_with_a_licence_folds_out_its_own_licence_text() {
        let _german = german();
        let ctx = context();
        let mut state = ThanksState::new("5.0.0").with_opener(record);
        let with_text: Vec<&Credit> = BUNDLED.iter().filter(|c| c.licence_text.is_some()).collect();
        assert_eq!(with_text.len(), 5, "Lucide, flag-icons, DPoP, Fleetmanifest, Handle Query");
        let first_line = |credit: &Credit| credit.licence_text.and_then(|t| t.lines().nth(2)).unwrap_or_default();
        let head = |credit: &Credit| tf(LICENCE_TEXT_NAME, &[("0", Arg::from(credit.name))]);
        let find = |nodes: &[(Role, String, egui::Pos2)], name: &str| {
            nodes.iter().find(|(role, text, _)| *role == Role::Button && text == name).map(|(_, _, at)| *at)
        };
        frame(&ctx, &mut state, Vec::new());
        for credit in &with_text {
            let nodes = frame(&ctx, &mut state, Vec::new());
            let at = find(&nodes, &head(credit)).unwrap_or_else(|| panic!("{}: no head for its licence", credit.name));
            let copyright = first_line(credit);
            assert!(copyright.starts_with("Copyright"), "{}: {copyright}", credit.name);
            assert!(!nodes.iter().any(|(_, text, _)| text.contains(copyright)), "{}: folded at first", credit.name);
            frame(&ctx, &mut state, click(at));
            let nodes = frame(&ctx, &mut state, Vec::new());
            assert_eq!(state.unfolded_licences(), vec![credit.name], "only this one unfolds");
            assert!(
                nodes.iter().any(|(_, text, _)| text.contains(copyright) && text.contains("permission notice")),
                "{}: its own text is shown",
                credit.name
            );
            let at = find(&nodes, &head(credit)).expect("still there");
            frame(&ctx, &mut state, click(at));
            frame(&ctx, &mut state, Vec::new());
            assert!(state.unfolded_licences().is_empty(), "{}: folded again", credit.name);
        }
        let nodes = frame(&ctx, &mut state, Vec::new());
        for credit in BUNDLED.iter().chain(FETCHED.iter()).filter(|c| c.licence_text.is_none()) {
            assert!(find(&nodes, &head(credit)).is_none(), "{}: a fold without a text", credit.name);
        }
    }

    #[test]
    fn every_person_unfolded_shows_every_text() {
        let _german = german();
        let ctx = context();
        let mut state = ThanksState::new("5.0.0").with_opener(record);
        state.open.extend(PEOPLE.iter().map(|p| p.name));
        frame(&ctx, &mut state, Vec::new());
        let nodes = frame(&ctx, &mut state, Vec::new());
        assert_eq!(state.unfolded().len(), PEOPLE.len());
        for person in &PEOPLE {
            for key in person.ideas.iter().chain(person.found.iter()) {
                let text = plain(&t(key));
                assert!(!text.trim().is_empty(), "{}: text {key} is empty", person.name);
                let start: String = text.chars().take(30).collect();
                assert!(
                    nodes.iter().any(|(_, shown, _)| shown.starts_with(&start)),
                    "{}: text {key} is not on the page",
                    person.name
                );
            }
        }
    }

    #[test]
    fn a_link_opens_its_address_and_the_page_says_so() {
        let _german = german();
        let ctx = context();
        let mut state = ThanksState::new("5.0.0").with_opener(record);
        frame(&ctx, &mut state, Vec::new());
        let nodes = frame(&ctx, &mut state, Vec::new());
        let url = versekit_core::credits::HANGAR_EXPORTER;
        let at = nodes
            .iter()
            .find(|(role, text, _)| *role == Role::Link && text == url)
            .map(|(_, _, at)| *at)
            .expect("the address of the exporter is a link");
        OPENED.lock().map(|mut list| list.clear()).ok();
        frame(&ctx, &mut state, click(at));
        let nodes = frame(&ctx, &mut state, Vec::new());
        assert_eq!(OPENED.lock().map(|l| l.clone()).unwrap_or_default(), vec![url.to_string()]);
        let said = tf("s_ub_auf", &[("0", Arg::from(url))]);
        assert!(nodes.iter().any(|(_, text, _)| *text == said), "{said}");
        // A browser that does not start: the page names the address instead.
        let mut state = ThanksState::new("5.0.0").with_opener(refuse);
        frame(&ctx, &mut state, Vec::new());
        let nodes = frame(&ctx, &mut state, Vec::new());
        let at = nodes
            .iter()
            .find(|(role, text, _)| *role == Role::Link && text == url)
            .map(|(_, _, at)| *at)
            .expect("link");
        frame(&ctx, &mut state, click(at));
        let nodes = frame(&ctx, &mut state, Vec::new());
        let said = tf("s_ub_auf_nein", &[("0", Arg::from(url))]);
        assert!(nodes.iter().any(|(_, text, _)| *text == said), "{said}");
    }

    #[test]
    fn all_links_and_buttons_have_names() {
        let _german = german();
        let ctx = context();
        let mut state = ThanksState::new("5.0.0").with_opener(record);
        let scheme = versekit_core::theme::current();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 12000.0))),
            ..Default::default()
        };
        let mut first = ctx.run_ui(input.clone(), |ui| show(ui, scheme, &mut state));
        first.textures_delta.clear();
        let mut output = ctx.run_ui(input, |ui| show(ui, scheme, &mut state));
        output.textures_delta.clear();
        let update = output.platform_output.accesskit_update.expect("accesskit is on");
        let links = update.nodes.iter().filter(|(_, n)| n.role() == Role::Link).count();
        assert_eq!(
            links,
            3 + BUNDLED.len() + FETCHED.len(),
            "one link per source, three for the project (profile, site, source)"
        );
        for (_, node) in update.nodes.iter().filter(|(_, n)| matches!(n.role(), Role::Link | Role::Button)) {
            assert!(node.label().is_some_and(|l| !l.is_empty()), "no name: {node:?}");
        }
    }

    #[test]
    fn the_notice_of_the_fan_kit_is_shown_as_it_stands() {
        let _german = german();
        let ctx = context();
        let mut state = ThanksState::new("5.0.0");
        frame(&ctx, &mut state, Vec::new());
        let nodes = frame(&ctx, &mut state, Vec::new());
        let notice = t(FAN_CONTENT);
        assert!(nodes.iter().any(|(_, text, _)| *text == notice), "the whole notice, unchanged");
        assert!(notice.contains("Cloud Imperium Rights LLC") && notice.contains("Fankit Agreement"));
    }
}
