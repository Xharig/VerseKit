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

use std::cell::Cell;

use versekit_core::handle_query::Failure;
use versekit_core::handle_query::log_monitor;
use versekit_core::language::{Choice, set_thread_language};
use versekit_core::paths::Host;
use versekit_core::pypath::Flavor;
use versekit_core::testdir::Dir;

use super::*;

thread_local! {
    static TOLD: Cell<usize> = const { Cell::new(0) };
}

/// Counts what would tell the overlay, on the thread of the test.
fn count() {
    TOLD.with(|told| told.set(told.get() + 1));
}

fn told() -> usize {
    TOLD.with(Cell::get)
}

/// A data folder of its own, never reaching the net.
fn paths(dir: &Dir) -> Paths {
    let home = dir.path().to_string_lossy().into_owned();
    Paths::new(Host::new(
        Flavor::native(),
        &[(versekit_core::legacy::ENV_HOME, home.as_str()), (versekit_core::legacy::ENV_NO_NET, "1")],
        &home,
    ))
}

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    crate::install(&ctx);
    ctx.enable_accesskit();
    ctx
}

type Nodes = Vec<(Role, String, egui::Pos2)>;

/// The page a frame draws.
#[derive(Clone, Copy, PartialEq, Eq)]
enum On {
    Search,
    Relations,
}

/// One frame of the page `on` and, below it, the settings of the search (they stand with the info bar) with
/// `events`: the accesskit nodes with role, label and the middle of their place, and the tab shown after it.
/// The frame starts on the tab `on`.
fn frame_on(
    ctx: &egui::Context,
    state: &mut HandlePageState,
    events: Vec<egui::Event>,
    on: On,
) -> (Nodes, Tab) {
    let scheme = versekit_core::theme::current();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 3000.0))),
        events,
        ..Default::default()
    };
    state.tab = match on {
        On::Search => Tab::Search,
        On::Relations => Tab::Relations,
    };
    let mut output = ctx.run_ui(input, |ui| {
        show_page(ui, scheme, state);
        show_settings(ui, scheme, state);
    });
    let action = state.tab;
    output.textures_delta.clear();
    let Some(update) = output.platform_output.accesskit_update else { return (Vec::new(), action) };
    let nodes = update
        .nodes
        .iter()
        .filter_map(|(_, n)| {
            let text = n.label().or(n.value())?.to_owned();
            let b = n.bounds()?;
            Some((n.role(), text, egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32)))
        })
        .collect();
    (nodes, action)
}

/// One frame of the page "Beziehungen", see [`frame_on`].
fn frame(ctx: &egui::Context, state: &mut HandlePageState, events: Vec<egui::Event>) -> Nodes {
    frame_on(ctx, state, events, On::Relations).0
}

/// Two frames of the page "Handle-Suche".
fn settled_search(ctx: &egui::Context, state: &mut HandlePageState) -> Nodes {
    frame_on(ctx, state, Vec::new(), On::Search);
    frame_on(ctx, state, Vec::new(), On::Search).0
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

fn find(nodes: &[(Role, String, egui::Pos2)], role: Role, label: &str) -> Option<egui::Pos2> {
    nodes.iter().find(|(r, l, _)| *r == role && l == label).map(|(_, _, at)| *at)
}

/// Two frames: the second has the places of the first laid out.
fn settled(ctx: &egui::Context, state: &mut HandlePageState) -> Vec<(Role, String, egui::Pos2)> {
    frame(ctx, state, Vec::new());
    frame(ctx, state, Vec::new())
}

fn names(rows: &[Row]) -> Vec<&str> {
    rows.iter().map(|r| r.name.as_str()).collect()
}

#[test]
fn the_list_sorts_by_stance_name_kind_and_time_and_filters() {
    let mut relations = Relations::default();
    relations.set(Kind::Handle, "Zeta", Some(Stance::Hostile));
    relations.set(Kind::Org, "NOVA", Some(Stance::Friendly));
    relations.set(Kind::Handle, "beta", Some(Stance::Neutral));
    relations.set(Kind::Handle, "Alpha", Some(Stance::Friendly));
    relations.set(Kind::Org, "REDSUN", Some(Stance::Hostile));
    let recent = vec![("Gamma".to_string(), 300.0), ("zeta".to_string(), 200.0)];
    let all = rows(&relations, &recent);
    assert_eq!(all.len(), 6, "a handle searched and marked stands once: {all:?}");
    let zeta = all.iter().find(|r| r.name == "Zeta").expect("Zeta");
    assert_eq!((zeta.stance, zeta.searched), (Some(Stance::Hostile), Some(200.0)));
    let gamma = all.iter().find(|r| r.name == "Gamma").expect("Gamma");
    assert_eq!((gamma.kind, gamma.stance), (Kind::Handle, None), "searched without a mark: unknown");
    let view = |sort, ascending| View { sort, ascending, ..View::default() };
    let order = |view: View| names(&arrange(&all, &view)).iter().map(|n| n.to_string()).collect::<Vec<String>>();
    let (a, b, g, z, nova, red) = ("Alpha", "beta", "Gamma", "Zeta", "NOVA", "REDSUN");
    assert_eq!(order(View::default()), [a, nova, b, red, z], "marked only: friendly, neutral, hostile");
    assert_eq!(order(view(Column::Stance, false)), [red, z, b, a, nova]);
    assert_eq!(order(view(Column::Name, true)), [a, b, nova, red, z]);
    assert_eq!(order(view(Column::Name, false)), [z, red, nova, b, a]);
    assert_eq!(order(view(Column::Kind, true)), [a, b, z, nova, red]);
    assert_eq!(order(view(Column::Searched, true)), [z, a, b, nova, red], "the newest first");
    let filtered = |stance, kind, text: &str| order(View { stance, kind, text: text.into(), ..View::default() });
    assert_eq!(filtered(3, 0, ""), [g], "unknown: searched without a mark");
    assert_eq!(filtered(4, 0, ""), [red, z], "hostile");
    assert_eq!(filtered(1, 2, ""), [nova], "friendly organizations");
    assert_eq!(filtered(0, 1, " TA "), [b, z], "a part of the name, capitals alike");
    assert!(filtered(2, 2, "").is_empty(), "nothing matches");
    // The handles searched lately: every one searched, marked or not, the newest first.
    assert_eq!(order(View::recent()), [g, z]);
    assert_eq!(order(View { stance: 4, ..View::recent() }), [g, z], "the stance filter counts not there");
    // A click on the column sorted turns it, a click on another sorts that one up.
    let mut state = HandlePageState::with_search(HandleSearch::empty().with_relations(relations));
    state.sort_by(Column::Stance);
    assert_eq!((state.list.view.sort, state.list.view.ascending), (Column::Stance, false));
    state.sort_by(Column::Name);
    assert_eq!((state.list.view.sort, state.list.view.ascending), (Column::Name, true));
    assert_eq!(state.recent_list.view.sort, Column::Searched, "the other list keeps its order");
    assert_eq!(names(state.arranged()), ["Alpha", "beta", "NOVA", "REDSUN", "Zeta"]);
}

#[test]
fn a_stance_changed_on_a_row_is_written_and_the_overlay_is_told() {
    set_thread_language(Choice::De);
    let dir = Dir::new("handle-seite-haltung");
    let paths = paths(&dir);
    let mut relations = Relations::default();
    relations.set(Kind::Handle, "Vex", Some(Stance::Neutral));
    assert!(relations.save(&paths));
    let mut state = HandlePageState::with_files(&paths).with_notify(count);
    let ctx = context();
    let nodes = settled(&ctx, &mut state);
    assert_eq!(told(), 0, "drawing tells nothing");
    let hostile =
        tf("s_hq_handle_als", &[("haltung", Arg::Text(t("s_hq_feindlich"))), ("org", Arg::Text("Vex".into()))]);
    let mark = find(&nodes, Role::CheckBox, &hostile).unwrap_or_else(|| panic!("the hostile mark: {nodes:?}"));
    frame(&ctx, &mut state, click(mark));
    assert_eq!(Relations::load(&paths).of(Kind::Handle, "Vex"), Some(Stance::Hostile), "written at once");
    assert_eq!(told(), 1, "the overlay is told");
    assert_eq!(state.arranged().first().map(|r| r.stance), Some(Some(Stance::Hostile)));
    // The button of the row takes the mark away.
    let nodes = settled(&ctx, &mut state);
    let remove = tf("s_hqs_entfernen", &[("name", Arg::Text("Vex".into()))]);
    let button = find(&nodes, Role::Button, &remove).unwrap_or_else(|| panic!("the remove button: {nodes:?}"));
    frame(&ctx, &mut state, click(button));
    assert!(Relations::load(&paths).is_empty());
    assert_eq!(told(), 2);
    assert!(state.arranged().is_empty(), "never searched, the row goes with its mark");
}

#[test]
fn a_mark_deleted_in_the_list_comes_back_at_its_place_with_undo() {
    set_thread_language(Choice::De);
    let dir = Dir::new("handle-seite-loeschen");
    let paths = paths(&dir);
    let mut relations = Relations::default();
    relations.set(Kind::Handle, "Erster", Some(Stance::Friendly));
    relations.set(Kind::Org, "MITTE", Some(Stance::Hostile));
    relations.set(Kind::Handle, "Letzter", Some(Stance::Neutral));
    assert!(relations.save(&paths));
    let mut state = HandlePageState::with_files(&paths).with_notify(count);
    let ctx = context();
    let nodes = settled(&ctx, &mut state);
    let remove = tf("s_hqs_entfernen", &[("name", Arg::Text("MITTE".into()))]);
    let button = find(&nodes, Role::Button, &remove).unwrap_or_else(|| panic!("the bin: {nodes:?}"));
    frame(&ctx, &mut state, click(button));
    assert_eq!(Relations::load(&paths).of(Kind::Org, "MITTE"), None, "deleted at once");
    assert_eq!(told(), 1);
    assert_eq!(names(state.arranged()), ["Erster", "Letzter"]);
    // The notice says what went and offers to take it back.
    let nodes = settled(&ctx, &mut state);
    let gone = tf("s_hqs_geloescht", &[("name", Arg::Text("MITTE".into()))]);
    assert!(nodes.iter().any(|(_, l, _)| *l == gone), "the notice: {nodes:?}");
    let undo = find(&nodes, Role::Button, &t("s_mlg_rueckgaengig")).unwrap_or_else(|| panic!("undo: {nodes:?}"));
    frame(&ctx, &mut state, click(undo));
    assert_eq!(Relations::load(&paths), relations, "back with its stance at its place in the file");
    assert_eq!(told(), 2, "the overlay is told again");
    let nodes = settled(&ctx, &mut state);
    assert!(find(&nodes, Role::Button, &t("s_mlg_rueckgaengig")).is_none(), "the notice went");
    assert_eq!(names(state.arranged()), ["Erster", "Letzter", "MITTE"]);
}

#[test]
fn a_click_on_a_handle_of_the_list_searches_it_at_the_top() {
    set_thread_language(Choice::De);
    let dir = Dir::new("handle-seite-klick");
    let paths = paths(&dir);
    let mut relations = Relations::default();
    relations.set(Kind::Handle, "Vex", Some(Stance::Hostile));
    relations.set(Kind::Org, "NOVA", Some(Stance::Friendly));
    assert!(relations.save(&paths));
    let mut state = HandlePageState::with_files(&paths).with_notify(count);
    let ctx = context();
    let nodes = settled(&ctx, &mut state);
    let name = find(&nodes, Role::Button, "Vex").unwrap_or_else(|| panic!("the handle: {nodes:?}"));
    assert!(state.search.shown().is_none());
    let (_, tab) = frame_on(&ctx, &mut state, click(name), On::Relations);
    assert_eq!(tab, Tab::Search, "the card shows on the tab of the search");
    // Without the net the card says so, for the handle clicked.
    assert_eq!(state.search.query, "Vex");
    assert_eq!(state.search.shown(), Some(&Err(Failure::Offline)));
    assert_eq!(told(), 0, "a search changes nothing to tell");
}

#[test]
fn a_click_on_an_organization_of_the_list_opens_its_card_without_asking_for_a_handle() {
    use versekit_core::handle_query::page;
    use versekit_core::net::{Fake, Net};
    set_thread_language(Choice::De);
    let dir = Dir::new("handle-seite-org-klick");
    let home = dir.path().to_string_lossy().into_owned();
    let online = Paths::new(Host::new(Flavor::native(), &[(versekit_core::legacy::ENV_HOME, home.as_str())], &home));
    let mut relations = Relations::default();
    relations.set(Kind::Org, "SEITENORG", Some(Stance::Friendly));
    assert!(relations.save(&online));
    let org_page = r#"<div id="organization"><div class="heading"><h1>Seitenstaffel / <span class="symbol">SEITENORG</span></h1></div><hr /></div>"#;
    let site = std::sync::Arc::new(Fake::default().with(&page::org_url("SEITENORG"), org_page));
    let net: std::sync::Arc<dyn Net> = site.clone();
    let mut state = HandlePageState::with_search(HandleSearch::with_net(&online, Some(net))).with_notify(count);
    let ctx = context();
    let nodes = settled(&ctx, &mut state);
    let org = find(&nodes, Role::Button, "SEITENORG").unwrap_or_else(|| panic!("the organization: {nodes:?}"));
    frame(&ctx, &mut state, click(org));
    assert_eq!(state.search.query, "SEITENORG");
    // The pause between two pages of the site is shared by the tests running at the same time.
    for _ in 0..3000 {
        if !state.search.searching() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        frame(&ctx, &mut state, Vec::new());
    }
    let shown = state.search.org_shown().cloned();
    assert!(
        matches!(&shown, Some(crate::handle_search::OrgShown::Card(found)) if found.org.name == "Seitenstaffel"),
        "{shown:?}"
    );
    let asked: Vec<String> = site.asked().into_iter().filter(|url| !url.contains("getOrgMembers")).collect();
    assert_eq!(asked, [page::org_url("SEITENORG")], "only the page of the organization, no handle");
}

#[test]
fn a_handle_opened_from_the_list_shows_its_card_with_the_logos() {
    use crate::handle_search::tests::{boxes, rich_site};
    set_thread_language(Choice::De);
    let dir = Dir::new("handle-seite-logos");
    let home = dir.path().to_string_lossy().into_owned();
    let online = Paths::new(Host::new(Flavor::native(), &[(versekit_core::legacy::ENV_HOME, home.as_str())], &home));
    let mut relations = Relations::default();
    relations.set(Kind::Handle, "Seitenpilot", Some(Stance::Friendly));
    assert!(relations.save(&online));
    let search = HandleSearch::with_net(&online, Some(std::sync::Arc::new(rich_site("Seitenpilot"))));
    let mut state = HandlePageState::with_search(search).with_notify(count);
    let ctx = context();
    let nodes = settled(&ctx, &mut state);
    let name = find(&nodes, Role::Button, "Seitenpilot").unwrap_or_else(|| panic!("the handle: {nodes:?}"));
    frame(&ctx, &mut state, click(name));
    let scheme = versekit_core::theme::current();
    let shapes = |state: &mut HandlePageState| {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 3000.0))),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            show_page(ui, scheme, state);
        });
        output.textures_delta.clear();
        output.shapes
    };
    // The pause between two pages of the site is shared by the tests running at the same time: up to a minute.
    let mut seen = (0, 0);
    let mut shown = false;
    for _ in 0..6000 {
        seen = boxes(&shapes(&mut state), crate::handle_search::LOGO_SIZE);
        shown |= state.search.shown().is_some();
        if seen.1 == 2 {
            assert_eq!(seen.0, 1, "the missing logo stays a quiet box");
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!(
        "the card with its pictures did not come: {seen:?}, shown once {shown}, searching {}, waiting {}, {:?}",
        state.search.searching(),
        state.search.pictures_waiting(),
        state.search.shown()
    );
}

#[test]
fn a_change_the_overlay_wrote_shows_within_a_second() {
    let dir = Dir::new("handle-seite-abgleich");
    let paths = paths(&dir);
    let mut state = HandlePageState::with_files(&paths);
    state.refresh(100.0);
    assert!(state.arranged().is_empty());
    let mut theirs = Relations::default();
    theirs.set(Kind::Org, "REDSUN", Some(Stance::Hostile));
    assert!(theirs.save(&paths));
    state.refresh(100.5);
    assert!(state.arranged().is_empty(), "looked at most once a second");
    state.refresh(101.5);
    assert_eq!(names(state.arranged()), ["REDSUN"]);
    assert_eq!(state.search.relations(), &theirs);
}

#[test]
fn a_setting_changed_on_the_page_is_written_and_the_overlay_is_told() {
    set_thread_language(Choice::De);
    let dir = Dir::new("handle-seite-einstellung");
    let paths = paths(&dir);
    let mut state = HandlePageState::with_files(&paths).with_notify(count);
    let ctx = context();
    let nodes = settled(&ctx, &mut state);
    assert!(paths.setting_bool(log_monitor::KEY_ON, true));
    let switch = find(&nodes, Role::CheckBox, &t("s_hq_einst_log")).unwrap_or_else(|| panic!("the switch: {nodes:?}"));
    frame(&ctx, &mut state, click(switch));
    assert!(!paths.setting_bool(log_monitor::KEY_ON, true), "written at once");
    assert_eq!(told(), 1);
}

#[test]
fn the_preview_shows_marks_and_a_handle_searched_without_one() {
    let mut state = HandlePageState::sample(1_000_000.0);
    let rows = state.arranged().to_vec();
    assert_eq!(names(&rows), ["NightOwl_77", "NOVA", "QuietComet", "REDSUN", "Vex"], "the marked ones");
    assert!(rows[0].searched.is_some() && rows[1].searched.is_none());
    let recent = state.arranged_of(Which::Recent).to_vec();
    assert_eq!(names(&recent), ["Drifter_9", "NightOwl_77", "Vex"], "searched lately, the newest first");
    assert_eq!(recent[0].stance, None);
}

#[test]
fn the_page_shows_the_members_in_the_card_and_a_click_searches_one() {
    set_thread_language(Choice::De);
    let mut state = HandlePageState::sample(1_000_000.0);
    let ctx = context();
    let nodes = settled_search(&ctx, &mut state);
    let buttons = |name: &str| nodes.iter().filter(|(r, l, _)| *r == Role::Button && l == name).count();
    assert_eq!(buttons("Starlight_Nova"), 1, "once, in the card: {nodes:?}");
    assert!(nodes.iter().any(|(_, l, _)| *l == t("s_hqm_verborgen")), "the hidden member");
    let member = find(&nodes, Role::Button, "Starlight_Nova").unwrap_or_else(|| panic!("the member: {nodes:?}"));
    frame_on(&ctx, &mut state, click(member), On::Search);
    // The preview asks no net: the card says so, for the member clicked.
    assert_eq!(state.search.query, "Starlight_Nova");
    assert_eq!(state.search.shown(), Some(&Err(Failure::Offline)));
    assert_eq!(state.search.members_sid(), Some("NOVA"), "the members stay open");
}

#[test]
fn the_search_shows_the_handles_searched_lately_and_leads_to_the_relations() {
    set_thread_language(Choice::De);
    let mut state = HandlePageState::sample(1_000_000.0);
    let ctx = context();
    let nodes = settled_search(&ctx, &mut state);
    let has = |text: &str| nodes.iter().any(|(_, l, _)| l == text);
    let rows = |name: &str| nodes.iter().filter(|(r, l, _)| *r == Role::Button && l == name).count();
    assert!(has(&t("s_hq_verlauf")), "{nodes:?}");
    assert_eq!(rows("Drifter_9"), 2, "searched lately and a member in the card: {nodes:?}");
    // Marked, never searched: on the page of the relations only.
    assert_eq!(rows("REDSUN"), 0, "{nodes:?}");
    assert_eq!(rows("QuietComet"), 1, "only in the card, as a member");
    let tab = find(&nodes, Role::Tab, &t("s_hqt_reiter_beziehungen")).unwrap_or_else(|| panic!("the tab: {nodes:?}"));
    let (_, shown) = frame_on(&ctx, &mut state, click(tab), On::Search);
    assert_eq!(shown, Tab::Relations, "a click on the tab opens the relations");
    // The page of the relations shows the marked ones with the export and the import.
    frame(&ctx, &mut state, Vec::new());
    let nodes = frame(&ctx, &mut state, Vec::new());
    let has = |text: &str| nodes.iter().any(|(_, l, _)| l == text);
    assert!(has("REDSUN") && has("QuietComet") && !has("Drifter_9"), "marked only: {nodes:?}");
    assert!(has(&t("s_hq_export")) && has(&t("s_hq_import")), "{nodes:?}");
    // Counter-check: drawing alone changes no tab.
    assert_eq!(frame_on(&ctx, &mut state, Vec::new(), On::Relations).1, Tab::Relations);
    assert_eq!(frame_on(&ctx, &mut state, Vec::new(), On::Search).1, Tab::Search);
}

#[test]
fn every_text_key_of_the_page_exists_in_both_languages() {
    let keys = [
        "hf_handlesuche",
        "s_hqt_reiter_suche",
        "s_hqt_reiter_beziehungen",
        "s_hqt_karte_suche",
        "s_hqt_karte_austausch",
        "s_hqb_lead",
        "s_hqs_verlauf_h",
        "s_hqs_verlauf_leer",
        "s_hqs_lead",
        "s_hqs_liste",
        "s_hqs_liste_h",
        "s_hqs_filter",
        "s_hqs_alle_markierten",
        "s_hqs_unbekannt",
        "s_hqs_alle_arten",
        "s_hqs_handles",
        "s_hqs_orgs",
        "s_hqs_haltung",
        "s_hqs_art",
        "s_hqs_name",
        "s_hqs_gesucht",
        "s_hqs_handle",
        "s_hqs_org",
        "s_hqs_suchen",
        "s_hqs_org_oeffnen",
        "s_hqs_entfernen",
        "s_hqs_geloescht",
        "s_hqs_leer",
        "s_hqs_keine_treffer",
        "s_hqs_mehr",
        "s_hqs_einstellungen",
        "s_hqs_einstellungen_h",
        "s_hqs_einstellungen_wo",
    ];
    for key in keys {
        for lang in [language::Lang::De, language::Lang::En] {
            let text = language::text_in(lang, key).unwrap_or_else(|| panic!("{key} fehlt"));
            assert!(!text.contains('—') && !text.contains('–'), "{key}: Gedankenstrich");
            for sentence in text.split(['.', '!', '?']) {
                assert!(sentence.split_whitespace().count() <= 10, "{key}: Satz zu lang: {sentence}");
            }
        }
    }
}
