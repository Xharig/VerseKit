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

//! The panel "Umziehen" of the stock page: choose a home place, choose by category what is struck, then move
//! both stocks there in one step (`versekit_core::stock_relocation`). Both stock files are copied to
//! `….vor-umzug` first; the message and the panel take the move back from these copies. After writing, the
//! files are read and counted again, and the panel lists what was struck for comparing with the game.

use std::collections::{HashMap, HashSet};
use std::time::SystemTime;

use egui::{RichText, Ui};
use versekit_core::formats;
use versekit_core::jsonio::Value;
use versekit_core::language::{self, Arg, t, tf};
use versekit_core::paths::Paths;
use versekit_core::stock::{self, Row, Store};
use versekit_core::stock_relocation::{self as core, Category, Consumables, Report};
use versekit_core::theme::Scheme;

use super::{Message, StockState, Undo, amount_with_unit, field_label, fold, places_of, suggest_field};
use crate::color;
use crate::widgets::{self, ButtonKind};

/// The question open in the panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ask {
    Move,
    Back,
}

/// What the panel keeps between frames.
#[derive(Default)]
pub(super) struct Relocation {
    /// The original names of the game and the report of the last move; none for the sample page.
    files: Option<(String, String)>,
    consumables: Option<Consumables>,
    /// The report of the last move, read once the panel is shown.
    report: Option<Option<Report>>,
    /// The sample page keeps the stocks before its move in memory.
    before: Option<(Vec<Value>, Vec<Value>)>,
    pub(super) home: String,
    home_error: Option<String>,
    /// The choice per category where it differs from [`Category::struck_by_default`].
    chosen: HashMap<Category, bool>,
    ask: Option<Ask>,
    pub(super) preview_open: bool,
    pub(super) report_open: bool,
}

impl Relocation {
    /// On the data folder of `paths`.
    pub(super) fn new(paths: &Paths) -> Relocation {
        Relocation {
            files: Some((paths.app_file(core::NAMES_FILE), paths.app_file(core::REPORT_FILE))),
            report_open: true,
            ..Relocation::default()
        }
    }

    fn consumables(&mut self) -> &Consumables {
        let names = self.files.as_ref().map(|(names, _)| names.clone());
        self.consumables.get_or_insert_with(|| names.map(|file| Consumables::load(&file)).unwrap_or_default())
    }

    fn report(&mut self) -> Option<&Report> {
        let file = self.files.as_ref().map(|(_, report)| report.clone());
        self.report.get_or_insert_with(|| file.and_then(|file| Report::load(&file))).as_ref()
    }

    pub(super) fn struck(&self, category: Category) -> bool {
        self.chosen.get(&category).copied().unwrap_or_else(|| category.struck_by_default())
    }
}

fn now() -> i64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

/// The name of a category in the list.
fn category_text(category: Category) -> String {
    match category {
        Category::Consumable(c) => t(c.text_key()),
        Category::Material(kind) => format!("{}: {}", t("s_ml_material"), t(&kind.text_key())),
        Category::Item(kind) => format!("{}: {}", t("s_ml_items"), t(&kind.text_key())),
    }
}

/// Every row with its category.
fn categorized(state: &mut StockState) -> Vec<(Row, Category)> {
    let rows = state.rows();
    let consumables = state.reloc.consumables();
    rows.into_iter()
        .map(|row| {
            let category = Category::of_row(&row, &state.data, consumables);
            (row, category)
        })
        .collect()
}

/// The place the field names, as the list of places writes it; `None` when empty or unknown.
fn chosen_home(state: &StockState) -> Option<String> {
    state.data.official_place(&state.reloc.home).filter(|place| !place.is_empty())
}

/// One line of a struck entry: name, amount, the place it lay at.
fn struck_line(ui: &mut Ui, scheme: &Scheme, name: &str, amount: f64, piece: bool, place: &str) {
    let lang = language::current();
    let place = if place.is_empty() { t("s_mlu_ohne_ort") } else { place.to_string() };
    ui.horizontal_wrapped(|row| {
        row.label(RichText::new(name).strong());
        let rest = format!("{} · {place}", amount_with_unit(lang, amount, piece));
        row.add(egui::Label::new(RichText::new(rest).small().color(color(scheme.sub))).wrap());
    });
}

/// The panel under the tab row.
pub(super) fn show(ui: &mut Ui, scheme: &Scheme, state: &mut StockState) {
    widgets::hint(ui, scheme, &t("s_mlu_hilfe"));
    let rows = categorized(state);
    let mut present: Vec<(Category, usize)> = Vec::new();
    for (_, category) in &rows {
        match present.iter_mut().find(|(c, _)| c == category) {
            Some((_, n)) => *n += 1,
            None => present.push((*category, 1)),
        }
    }
    present.sort_by_key(|(c, _)| c.order());

    field_label(ui, &t("s_mlu_heimatort"));
    let places: Vec<String> = if state.data.places().is_empty() {
        places_of(rows.iter().map(|(r, _)| r))
    } else {
        state.data.places().to_vec()
    };
    let before = state.reloc.home.clone();
    let reloc = &mut state.reloc;
    suggest_field(
        ui,
        scheme,
        "lager-umzug-ort",
        &t("s_mlu_heimatort_ph"),
        &mut reloc.home,
        &places,
        &mut reloc.home_error,
    );
    if state.reloc.home != before {
        state.reloc.ask = None;
    }

    field_label(ui, &t("s_mlu_streichen"));
    if present.is_empty() {
        widgets::hint(ui, scheme, &t("s_ml_leer"));
    }
    for (category, count) in &present {
        let mut on = state.reloc.struck(*category);
        let label = format!("{} ({count})", category_text(*category));
        if widgets::switch_row(ui, scheme, &label, &mut on).changed() {
            state.reloc.chosen.insert(*category, on);
            state.reloc.ask = None;
        }
    }
    widgets::hint(ui, scheme, &t("s_mlu_streichen_hilfe"));

    let struck: Vec<&Row> = rows.iter().filter(|(_, c)| state.reloc.struck(*c)).map(|(r, _)| r).collect();
    let home = chosen_home(state);
    ui.add_space(6.0);
    if let Some(home) = &home {
        let text = tf(
            "s_mlu_vorschau",
            &[
                ("ort", Arg::from(home.as_str())),
                ("n", Arg::from(rows.len() - struck.len())),
                ("m", Arg::from(struck.len())),
            ],
        );
        ui.add(egui::Label::new(text).wrap());
    }
    if !struck.is_empty() {
        let title = tf("s_mlu_liste", &[("n", Arg::from(struck.len()))]);
        let mut open = state.reloc.preview_open;
        fold(ui, &title, &mut open, |ui| {
            for row in &struck {
                struck_line(ui, scheme, &row.name, row.amount, row.piece, &row.place);
            }
        });
        state.reloc.preview_open = open;
    }

    ui.add_space(4.0);
    // A typed place the list does not know says so at the button, an empty field asks for one.
    let missing =
        if state.reloc.home.trim().is_empty() { t("s_mlu_heimatort_fehlt") } else { t("s_mlu_ort_unbekannt") };
    let empty = t("s_ml_leer");
    let open_dialog = t("s_mlg_dialog_offen");
    let blocked = if rows.is_empty() {
        Some(empty.as_str())
    } else if state.dialog.is_some() {
        Some(open_dialog.as_str())
    } else if home.is_none() {
        Some(missing.as_str())
    } else {
        None
    };
    let mut run = None;
    ui.horizontal_wrapped(|row| {
        if state.reloc.ask == Some(Ask::Move)
            && blocked.is_none()
            && let Some(home) = &home
        {
            let question = tf("s_mlu_frage", &[("ort", Arg::from(home.as_str())), ("m", Arg::from(struck.len()))]);
            row.add(egui::Label::new(RichText::new(question).color(color(scheme.red))).wrap());
            if row.add(widgets::Button::new(&t("s_mlg_bestaetigen"), ButtonKind::Danger, scheme)).clicked() {
                run = Some(Ask::Move);
            }
            if row.add(widgets::Button::new(&t("s_lg_abbrechen"), ButtonKind::Normal, scheme)).clicked() {
                state.reloc.ask = None;
            }
        } else {
            let word = match &home {
                Some(home) => tf("s_mlu_umziehen", &[("ort", Arg::from(home.as_str()))]),
                None => t("s_mlg_aktion_umziehen"),
            };
            let button =
                widgets::Button::new(&word, ButtonKind::Strong, scheme).icon("house").wrapping().blocked(blocked);
            let clicked = row.add(button).clicked();
            if clicked && home.is_some() {
                state.reloc.ask = Some(Ask::Move);
            }
        }
    });
    report_section(ui, scheme, state, &mut run);
    match run {
        Some(Ask::Move) => move_home(state),
        Some(Ask::Back) => take_back(state),
        None => {}
    }
}

/// The last move: when and where, the struck entries to compare with the game, and taking it back.
fn report_section(ui: &mut Ui, scheme: &Scheme, state: &mut StockState, run: &mut Option<Ask>) {
    let Some(report) = state.reloc.report().cloned() else { return };
    ui.add_space(8.0);
    ui.separator();
    let lang = language::current();
    let head = tf(
        "s_mlu_letzter",
        &[
            ("zeit", Arg::from(formats::date_time(lang, report.time as f64))),
            ("ort", Arg::from(report.home.as_str())),
            ("n", Arg::from(report.moved)),
            ("m", Arg::from(report.struck.len())),
        ],
    );
    ui.add(egui::Label::new(RichText::new(head).strong()).wrap());
    if !report.struck.is_empty() {
        let title = tf("s_mlu_abgleich", &[("n", Arg::from(report.struck.len()))]);
        let mut open = state.reloc.report_open;
        fold(ui, &title, &mut open, |ui| {
            widgets::hint(ui, scheme, &t("s_mlu_abgleich_hilfe"));
            for (store, entry) in &report.struck {
                let key = if *store == Store::Raw { "material" } else { "ware" };
                let name = entry.get(key).and_then(Value::as_str).map_or("?", str::trim);
                let piece = match store {
                    Store::Raw => state.data.is_piece(name),
                    Store::Trade => stock::is_pieces(entry),
                };
                let place = entry.get("ort").and_then(Value::as_str).map_or("", str::trim);
                struck_line(ui, scheme, name, stock::number_of(entry, "menge"), piece, place);
            }
        });
        state.reloc.report_open = open;
    }
    ui.add_space(4.0);
    ui.horizontal_wrapped(|row| {
        if state.reloc.ask == Some(Ask::Back) {
            row.add(egui::Label::new(RichText::new(t("s_mlu_zurueck_frage")).color(color(scheme.red))).wrap());
            if row.add(widgets::Button::new(&t("s_mlg_bestaetigen"), ButtonKind::Danger, scheme)).clicked() {
                *run = Some(Ask::Back);
            }
            if row.add(widgets::Button::new(&t("s_lg_abbrechen"), ButtonKind::Normal, scheme)).clicked() {
                state.reloc.ask = None;
            }
        } else if row
            .add(widgets::Button::new(&t("s_mlu_zuruecknehmen"), ButtonKind::Normal, scheme).icon("undo-2"))
            .clicked()
        {
            state.reloc.ask = Some(Ask::Back);
        }
    });
}

/// Says why writing a copy failed.
fn failed(state: &mut StockState, error: &std::io::Error, what: &str) {
    if versekit_core::writeguard::refused(error) {
        state.say(t("s_mlg_nur_testkopie"));
    } else {
        versekit_core::errors::record(what, error);
        state.say(t("s_hl_fehler"));
    }
}

/// Moves both stocks to the chosen home place: copies first, then both files, then the report, then a count
/// of what the files hold now.
pub(super) fn move_home(state: &mut StockState) {
    state.reloc.ask = None;
    let Some(home) = chosen_home(state) else {
        state.reloc.home_error = Some(t("s_mlu_heimatort_fehlt"));
        return;
    };
    if let Some((raw, trade)) = &state.files
        && (stock::load_checked(raw, "materials.load").is_none()
            || stock::load_checked(trade, "trade_cargo.load").is_none())
    {
        state.say(t("s_mlu_unlesbar"));
        return;
    }
    state.reload();
    state.stop_editing();
    let rows = categorized(state);
    let strike: HashSet<(Store, usize)> =
        rows.iter().filter(|(_, c)| state.reloc.struck(*c)).map(|(r, _)| (r.store, r.index)).collect();
    let (raw_before, trade_before) = (state.raw.clone(), state.trade.clone());
    let moved =
        core::relocate(&raw_before, &trade_before, &home, &state.data, |store, index| strike.contains(&(store, index)));
    match state.files.clone() {
        Some((raw, trade)) => {
            for file in [raw, trade] {
                if let Err(error) = core::keep_before(&file) {
                    failed(state, &error, "stock_relocation.keep_before");
                    return;
                }
            }
        }
        None => state.reloc.before = Some((raw_before.clone(), trade_before.clone())),
    }
    state.raw = moved.raw.clone();
    if !state.write(Store::Raw) {
        return;
    }
    state.trade = moved.trade.clone();
    if !state.write(Store::Trade) {
        state.raw = raw_before;
        state.write(Store::Raw);
        return;
    }
    let report = Report::new(now(), &home, &moved);
    let mut saved = true;
    if let Some((_, file)) = &state.reloc.files
        && let Err(error) = report.save(file)
    {
        versekit_core::errors::record("stock_relocation.report", &error);
        saved = false;
    }
    state.reloc.report = Some(Some(report));
    state.reloc.report_open = true;
    state.reload();
    let check = core::check((&raw_before, &trade_before), &moved.struck, (&state.raw, &state.trade), &home);
    let mut text = tf(
        "s_mlu_fertig",
        &[("ort", Arg::from(home.as_str())), ("n", Arg::from(moved.moved)), ("m", Arg::from(moved.struck.len()))],
    );
    text.push(' ');
    if check.fine() {
        text.push_str(&t("s_mlu_geprueft"));
    } else {
        let mut names = check.elsewhere.clone();
        names.extend(check.differs.iter().cloned());
        names.dedup();
        text.push_str(&tf("s_mlu_abweichung", &[("namen", Arg::from(names.join(", ")))]));
    }
    if !saved {
        text.push(' ');
        text.push_str(&t("s_mlu_bericht_fehler"));
    }
    state.message = Some(Message { text, since: None, undo: Some(Undo::Relocated) });
}

/// Puts both stocks back as they were before the last move and takes the report away.
pub(super) fn take_back(state: &mut StockState) {
    state.reloc.ask = None;
    let before = match &state.files {
        Some((raw, trade)) => match (core::kept_before(raw), core::kept_before(trade)) {
            (Some(raw), Some(trade)) => (raw, trade),
            _ => {
                state.say(t("s_mlu_keine_sicherung"));
                return;
            }
        },
        None => match state.reloc.before.take() {
            Some(before) => before,
            None => {
                state.say(t("s_mlu_keine_sicherung"));
                return;
            }
        },
    };
    state.reload();
    state.stop_editing();
    let raw_now = state.raw.clone();
    state.raw = before.0;
    if !state.write(Store::Raw) {
        return;
    }
    state.trade = before.1;
    if !state.write(Store::Trade) {
        state.raw = raw_now;
        state.write(Store::Raw);
        return;
    }
    if let Some((_, file)) = &state.reloc.files
        && let Err(error) = Report::remove(file)
    {
        versekit_core::errors::record("stock_relocation.report", &error);
    }
    state.reloc.report = Some(None);
    state.say(t("s_mlu_zurueck"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Pos2, Rect};
    use versekit_core::language::Choice;
    use versekit_core::legacy;
    use versekit_core::paths::Host;
    use versekit_core::pypath::Flavor;
    use versekit_core::stock_relocation::Consumable;
    use versekit_core::theme;

    const NAMES: &str = r#"{"item_Namecrlf_consumable_healing_01": "MedPen (Hemozal)",
        "item_Namebehr_lmg_ballistic_01_mag": "FS-9 Magazine (75 cap)",
        "item_NameFood_bar_busters_01_a": "Buster's Chocolate Bar",
        "item_Namebehr_lmg_ballistic_01": "FS-9 LMG"}"#;
    const RAW: &str = r#"{"format": 1, "posten": [
        {"material": "Hadanite", "menge": 14.0, "qualitaet": 500, "ort": "Area18"},
        {"material": "Hadanite", "menge": 2.0, "qualitaet": 500, "ort": "Levski"},
        {"material": "Iron", "menge": 4.5, "qualitaet": 420, "ort": "Orison"}]}"#;
    const TRADE: &str = r#"{"format": 1, "posten": [
        {"ware": "MedPen (Hemozal)", "menge": 3.0, "ort": "Levski", "gestohlen": false, "stueck": true},
        {"ware": "FS-9 Magazine (75 Cap)", "menge": 474.0, "ort": "Orison", "gestohlen": false, "stueck": true},
        {"ware": "Buster's Chocolate Bar", "menge": 1.0, "ort": "Orison", "gestohlen": false, "stueck": true},
        {"ware": "FS-9 LMG", "menge": 2.0, "ort": "Orison", "gestohlen": false, "stueck": true},
        {"ware": "FS-9 LMG", "menge": 1.0, "ort": "Area18", "gestohlen": false, "stueck": true},
        {"ware": "Laranite", "menge": 32.0, "ort": "Seraphim Station", "gestohlen": false, "einkauf": 2650.0}]}"#;

    fn setup() -> std::sync::MutexGuard<'static, ()> {
        let guard = crate::language_lock();
        theme::set_current("original");
        language::set_language(Choice::De);
        guard
    }

    fn world(tag: &str) -> (String, Paths) {
        let home = std::env::temp_dir()
            .join(format!("versekit-lagerumzug-{tag}-{}", versekit_core::testdir::run_tag()))
            .to_string_lossy()
            .replace('\\', "/");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("folder");
        let paths = Paths::new(Host::new(Flavor::native(), &[(legacy::ENV_HOME, home.as_str())], &home));
        let write = |name: &str, text: &str| std::fs::write(paths.app_file(name), text).expect("file");
        write("namen-original.json", NAMES);
        write(stock::RAW_FILE, RAW);
        write(stock::TRADE_FILE, TRADE);
        (home, paths)
    }

    fn page(paths: &Paths) -> StockState {
        let mut state = StockState::sample_on(paths);
        state.reloc = Relocation::new(paths);
        state.panel = Some(super::super::Panel::Move);
        state
    }

    fn on_disk(paths: &Paths, file: &str) -> Vec<Value> {
        stock::load(&paths.app_file(file), "test")
    }

    fn place(entry: &Value) -> &str {
        entry.get("ort").and_then(Value::as_str).unwrap_or_default()
    }

    fn name(entry: &Value) -> &str {
        entry.get("ware").or_else(|| entry.get("material")).and_then(Value::as_str).unwrap_or_default()
    }

    #[test]
    fn the_move_brings_everything_home_strikes_the_consumables_and_keeps_a_copy() {
        let _language = setup();
        let (home, paths) = world("ziehen");
        let mut state = page(&paths);
        let raw_text = std::fs::read_to_string(paths.app_file(stock::RAW_FILE)).unwrap();
        let trade_text = std::fs::read_to_string(paths.app_file(stock::TRADE_FILE)).unwrap();
        state.reloc.home = "levski".into();
        move_home(&mut state);
        let (raw, trade) = (on_disk(&paths, stock::RAW_FILE), on_disk(&paths, stock::TRADE_FILE));
        assert!(raw.iter().chain(&trade).all(|e| place(e) == "Levski"), "{raw:?} {trade:?}");
        let names: Vec<&str> = trade.iter().map(name).collect();
        assert_eq!(names, vec!["FS-9 LMG", "Laranite"], "consumables struck, equal items one stack");
        assert_eq!(stock::number_of(&trade[0], "menge"), 3.0);
        assert_eq!(raw.len(), 2, "the gems add up, the iron crate stays");
        assert_eq!(stock::number_of(&raw[0], "menge"), 16.0);
        // The copies before are the files as they were.
        let kept = |file: &str| std::fs::read_to_string(format!("{}{}", paths.app_file(file), core::BACKUP_SUFFIX));
        assert_eq!(kept(stock::RAW_FILE).unwrap(), raw_text);
        assert_eq!(kept(stock::TRADE_FILE).unwrap(), trade_text);
        // The report lists the struck as they were, and the message offers to take it back.
        let report = Report::load(&paths.app_file(core::REPORT_FILE)).expect("report");
        assert_eq!(report.home, "Levski");
        let struck: Vec<(&str, &str)> = report.struck.iter().map(|(_, e)| (name(e), place(e))).collect();
        assert_eq!(
            struck,
            vec![
                ("MedPen (Hemozal)", "Levski"),
                ("FS-9 Magazine (75 Cap)", "Orison"),
                ("Buster's Chocolate Bar", "Orison")
            ]
        );
        let message = state.message.as_ref().expect("message");
        assert!(matches!(message.undo, Some(Undo::Relocated)));
        assert!(message.text.contains(&t("s_mlu_geprueft")), "{}", message.text);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn taking_back_restores_both_files_and_takes_the_report_away() {
        let _language = setup();
        let (home, paths) = world("zurueck");
        let mut state = page(&paths);
        let (raw, trade) = (on_disk(&paths, stock::RAW_FILE), on_disk(&paths, stock::TRADE_FILE));
        state.reloc.home = "New Babbage".into();
        move_home(&mut state);
        assert_eq!(on_disk(&paths, stock::RAW_FILE), raw, "an unknown place moves nothing");
        assert_eq!(state.reloc.home_error, Some(t("s_mlu_heimatort_fehlt")));
        state.reloc.home = "Orison".into();
        move_home(&mut state);
        assert_ne!(on_disk(&paths, stock::TRADE_FILE), trade);
        let Some(Message { undo: Some(undo), .. }) = state.message.take() else { panic!("no undo") };
        state.take_back(undo);
        assert_eq!(on_disk(&paths, stock::RAW_FILE), raw);
        assert_eq!(on_disk(&paths, stock::TRADE_FILE), trade);
        assert_eq!(Report::load(&paths.app_file(core::REPORT_FILE)), None);
        assert_eq!(state.message.as_ref().map(|m| m.text.clone()), Some(t("s_mlu_zurueck")));
        // Without a copy there is nothing to take back, and nothing changes.
        std::fs::remove_file(format!("{}{}", paths.app_file(stock::TRADE_FILE), core::BACKUP_SUFFIX)).unwrap();
        take_back(&mut state);
        assert_eq!(state.message.as_ref().map(|m| m.text.clone()), Some(t("s_mlu_keine_sicherung")));
        assert_eq!(on_disk(&paths, stock::RAW_FILE), raw);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_category_switched_off_stays_and_one_switched_on_goes() {
        let _language = setup();
        let (home, paths) = world("arten");
        let mut state = page(&paths);
        state.reloc.chosen.insert(Category::Consumable(Consumable::Pen), false);
        state.reloc.chosen.insert(Category::Material(versekit_core::stock_data::Kind::Commodity), true);
        state.reloc.home = "Levski".into();
        move_home(&mut state);
        let names: Vec<String> = on_disk(&paths, stock::TRADE_FILE).iter().map(|e| name(e).to_string()).collect();
        assert!(names.contains(&"MedPen (Hemozal)".to_string()), "{names:?}");
        assert!(!names.contains(&"Laranite".to_string()), "{names:?}");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn an_unreadable_stock_file_blocks_the_move() {
        let _language = setup();
        let (home, paths) = world("kaputt");
        let mut state = page(&paths);
        std::fs::write(paths.app_file(stock::TRADE_FILE), "{\"format\": 9, \"posten\": []}").unwrap();
        let raw = std::fs::read_to_string(paths.app_file(stock::RAW_FILE)).unwrap();
        state.reloc.home = "Levski".into();
        move_home(&mut state);
        assert_eq!(std::fs::read_to_string(paths.app_file(stock::RAW_FILE)).unwrap(), raw);
        assert_eq!(state.message.as_ref().map(|m| m.text.clone()), Some(t("s_mlu_unlesbar")));
        assert!(!std::path::Path::new(&format!("{}{}", paths.app_file(stock::RAW_FILE), core::BACKUP_SUFFIX)).exists());
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_sample_page_moves_in_memory_and_takes_it_back() {
        let _language = setup();
        let mut state = StockState::default();
        let (raw, trade) = (state.raw.clone(), state.trade.clone());
        state.reloc.home = "Levski".into();
        move_home(&mut state);
        assert!(state.raw.iter().chain(&state.trade).all(|e| place(e) == "Levski"));
        take_back(&mut state);
        assert_eq!((state.raw.clone(), state.trade.clone()), (raw, trade));
    }

    /// The texts and the named controls of a few frames of the page, with an optional click.
    fn frames(ctx: &egui::Context, state: &mut StockState, width: f32, click: Option<&str>) -> (Vec<String>, f32) {
        let scheme = theme::current();
        let mut target = None;
        let mut texts = Vec::new();
        let mut over = 0.0;
        for step in 0..6 {
            let mut events = Vec::new();
            if let (Some(at), 3) = (target, step) {
                events.push(egui::Event::PointerMoved(at));
            }
            if let (Some(at), 4) = (target, step) {
                for pressed in [true, false] {
                    events.push(egui::Event::PointerButton {
                        pos: at,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
            }
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 3000.0))),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                super::super::show_page(ui, scheme, state);
                over = (ui.min_rect().right() - ui.ctx().content_rect().right()).max(0.0f32);
            });
            output.textures_delta.clear();
            texts.clear();
            fn collect(shape: &egui::Shape, into: &mut Vec<String>) {
                match shape {
                    egui::Shape::Text(text) => into.push(text.galley.text().to_string()),
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, into)),
                    _ => {}
                }
            }
            output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
            if let (Some(label), None) = (click, target) {
                target = output.platform_output.accesskit_update.as_ref().and_then(|update| {
                    update.nodes.iter().find(|(_, node)| node.label() == Some(label)).and_then(|(_, node)| {
                        let b = node.bounds()?;
                        Some(egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32))
                    })
                });
            }
        }
        (texts, over)
    }

    #[test]
    fn the_panel_shows_the_categories_and_asks_before_it_moves() {
        let _language = setup();
        let (home, paths) = world("tafel");
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        let mut state = page(&paths);
        let (texts, _) = frames(&ctx, &mut state, 1000.0, None);
        for wanted in
            [t("s_mlu_heimatort"), format!("{} (1)", t("s_mlu_k_pens")), format!("{} (1)", t("s_mlu_k_magazine"))]
        {
            assert!(texts.contains(&wanted), "{wanted}: {texts:?}");
        }
        state.reloc.home = "Levski".into();
        let button = tf("s_mlu_umziehen", &[("ort", Arg::from("Levski"))]);
        let (texts, _) = frames(&ctx, &mut state, 1000.0, Some(&button));
        let question = tf("s_mlu_frage", &[("ort", Arg::from("Levski")), ("m", Arg::from(3))]);
        assert!(texts.contains(&question), "the question after the click: {texts:?}");
        assert_eq!(on_disk(&paths, stock::TRADE_FILE).len(), 6, "nothing moved before the answer");
        frames(&ctx, &mut state, 1000.0, Some(&t("s_mlg_bestaetigen")));
        assert_eq!(on_disk(&paths, stock::TRADE_FILE).len(), 2, "moved after the answer");
        let (texts, _) = frames(&ctx, &mut state, 1000.0, None);
        assert!(texts.contains(&tf("s_mlu_abgleich", &[("n", Arg::from(3))])), "the list to compare: {texts:?}");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn an_open_file_dialog_blocks_the_move() {
        let _language = setup();
        let (home, paths) = world("dialog");
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        let mut state = page(&paths);
        state.reloc.home = "Levski".into();
        state.dialog = Some((crate::file_dialog::Pending::default(), super::super::Job::Import));
        let button = tf("s_mlu_umziehen", &[("ort", Arg::from("Levski"))]);
        let (texts, _) = frames(&ctx, &mut state, 1000.0, Some(&button));
        let question = tf("s_mlu_frage", &[("ort", Arg::from("Levski")), ("m", Arg::from(3))]);
        assert!(!texts.contains(&question), "no question while the dialog is open");
        assert_eq!(state.reloc.ask, None);
        state.dialog = None;
        frames(&ctx, &mut state, 1000.0, Some(&button));
        assert_eq!(state.reloc.ask, Some(Ask::Move), "asks once the dialog is closed");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_panel_fits_every_size_and_both_languages() {
        let _language = setup();
        let (home, paths) = world("passt");
        let mut findings = Vec::new();
        for choice in [Choice::De, Choice::En] {
            language::set_language(choice);
            for points in versekit_core::font_size::POINTS_RANGE.0..=versekit_core::font_size::POINTS_RANGE.1 {
                for (width, asking, report) in
                    [380.0, 520.0, 900.0].into_iter().flat_map(|w| [(w, false, false), (w, true, true)])
                {
                    let ctx = egui::Context::default();
                    crate::install(&ctx);
                    crate::apply_size(&ctx, points);
                    let mut state = page(&paths);
                    state.reloc.home = "Seraphim Station".into();
                    state.reloc.preview_open = true;
                    if asking {
                        state.reloc.ask = Some(Ask::Move);
                    }
                    if report {
                        let moved =
                            core::relocate(&state.raw, &state.trade, "Seraphim Station", &state.data, |s, _| {
                                s == Store::Trade
                            });
                        state.reloc.report = Some(Some(Report::new(1_793_000_000, "Seraphim Station", &moved)));
                    }
                    let (_, over) = frames(&ctx, &mut state, width, None);
                    if over > 0.5 {
                        findings.push(format!(
                            "{choice:?} size {points} width {width} asking {asking}: {over:.0} too wide"
                        ));
                    }
                }
            }
        }
        language::set_language(Choice::De);
        assert!(findings.is_empty(), "{}", findings.join("\n"));
        let _ = std::fs::remove_dir_all(home);
    }

    /// The real data of a copy (`VERSEKIT_KOPIE`), read only: what a move to `VERSEKIT_HEIMATORT` would strike
    /// and keep, and that the copy stays as it was.
    #[test]
    #[ignore = "needs a copy of real data"]
    fn real_copy_shows_what_a_move_would_do() {
        let Ok(root) = std::env::var("VERSEKIT_KOPIE") else { return };
        let home = std::env::var("VERSEKIT_HEIMATORT").unwrap_or_else(|_| "Levski".into());
        versekit_core::writeguard::deny(true);
        versekit_core::writeguard::allow_under(None);
        let paths = Paths::for_copy(Host::current(), &root).expect("copy");
        let mut state = StockState::with_files(&paths);
        let rows = categorized(&mut state);
        let mut by_category: Vec<(Category, Vec<String>)> = Vec::new();
        for (row, category) in &rows {
            match by_category.iter_mut().find(|(c, _)| c == category) {
                Some((_, names)) => names.push(row.name.clone()),
                None => by_category.push((*category, vec![row.name.clone()])),
            }
        }
        by_category.sort_by_key(|(c, _)| c.order());
        println!("Namen des Spiels mit Verbrauchbarem: {}", state.reloc.consumables().len());
        for (category, names) in &by_category {
            let mark = if state.reloc.struck(*category) { "STREICHEN" } else { "behalten" };
            println!("{mark:9} {} ({}): {}", category_text(*category), names.len(), names.join(" | "));
        }
        let strike: HashSet<(Store, usize)> =
            rows.iter().filter(|(_, c)| state.reloc.struck(*c)).map(|(r, _)| (r.store, r.index)).collect();
        let moved = core::relocate(&state.raw, &state.trade, &home, &state.data, |s, i| strike.contains(&(s, i)));
        let check = core::check((&state.raw, &state.trade), &moved.struck, (&moved.raw, &moved.trade), &home);
        println!(
            "Vorher {} Posten an {} Orten · nach {home}: {} umgebucht, {} zusammengelegt, {} gestrichen, {} bleiben · Nachprüfung {}",
            rows.len(),
            places_of(rows.iter().map(|(r, _)| r)).len(),
            moved.moved,
            moved.merged,
            moved.struck.len(),
            moved.kept(),
            if check.fine() { "gut" } else { "ABWEICHUNG" }
        );
        let denied = versekit_core::writeguard::take_denied();
        versekit_core::writeguard::deny(false);
        assert!(check.fine(), "{check:?}");
        assert!(denied.is_empty(), "the page wanted to write: {denied:?}");
    }

    /// Every file of a folder with its size and time of change.
    fn snapshot(folder: &std::path::Path, into: &mut Vec<(String, u64, Option<SystemTime>)>) {
        let Ok(entries) = std::fs::read_dir(folder) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.metadata() {
                Ok(meta) if meta.is_dir() => snapshot(&path, into),
                Ok(meta) => into.push((path.to_string_lossy().into_owned(), meta.len(), meta.modified().ok())),
                Err(_) => {}
            }
        }
    }

    /// The real stock of a copy (`VERSEKIT_KOPIE`) moved to `VERSEKIT_HEIMATORT` and taken back, in a throwaway
    /// folder: the two stock files are copied there, writing anywhere else is locked, and the copy stays as it was.
    #[test]
    #[ignore = "needs a copy of real data"]
    fn real_copy_moves_and_comes_back_in_a_throwaway_folder() {
        let _language = setup();
        let Ok(root) = std::env::var("VERSEKIT_KOPIE") else { return };
        let home = std::env::var("VERSEKIT_HEIMATORT").unwrap_or_else(|_| "Levski".into());
        let mut before_files = Vec::new();
        snapshot(std::path::Path::new(&root), &mut before_files);
        let scratch = std::env::temp_dir()
            .join(format!("versekit-lagerumzug-echt-{}", versekit_core::testdir::run_tag()))
            .to_string_lossy()
            .replace('\\', "/");
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).expect("scratch");
        versekit_core::writeguard::deny(true);
        versekit_core::writeguard::allow_under(Some(&scratch));
        let paths = Paths::for_copy(Host::current(), &root).expect("copy");
        let mut state = StockState::with_files(&paths);
        let (raw_file, trade_file) =
            (format!("{scratch}/{}", stock::RAW_FILE), format!("{scratch}/{}", stock::TRADE_FILE));
        for (from, to) in
            [(paths.app_file(stock::RAW_FILE), &raw_file), (paths.app_file(stock::TRADE_FILE), &trade_file)]
        {
            std::fs::write(to, std::fs::read(&from).expect("stock of the copy")).expect("into scratch");
        }
        state.files = Some((raw_file.clone(), trade_file.clone()));
        state.reloc.files = Some((paths.app_file(core::NAMES_FILE), format!("{scratch}/{}", core::REPORT_FILE)));
        state.reload();
        let (raw, trade) = (state.raw.clone(), state.trade.clone());
        state.reloc.home = home.clone();
        move_home(&mut state);
        let text = state.message.as_ref().map(|m| m.text.clone()).unwrap_or_default();
        println!("{text}");
        let report = state.reloc.report().cloned().expect("report");
        println!("Gestrichen zum Abgleich ({}):", report.struck.len());
        for (_, entry) in &report.struck {
            println!("  {} · {} · {}", name(entry), stock::number_of(entry, "menge"), place(entry));
        }
        let (raw_now, trade_now) = (stock::load(&raw_file, "test"), stock::load(&trade_file, "test"));
        println!("Nachher: {} Rohstoff-Posten, {} Handel/Items, alle in {home}", raw_now.len(), trade_now.len());
        assert!(text.contains(&t("s_mlu_geprueft")), "{text}");
        take_back(&mut state);
        assert_eq!(stock::load(&raw_file, "test"), raw, "materials back");
        assert_eq!(stock::load(&trade_file, "test"), trade, "goods and items back");
        let denied = versekit_core::writeguard::take_denied();
        versekit_core::writeguard::deny(false);
        versekit_core::writeguard::allow_under(None);
        let mut after_files = Vec::new();
        snapshot(std::path::Path::new(&root), &mut after_files);
        assert!(denied.is_empty(), "writes outside the throwaway folder: {denied:?}");
        assert_eq!(before_files, after_files, "the copy stays as it was");
        let _ = std::fs::remove_dir_all(scratch);
    }
}
