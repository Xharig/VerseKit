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

use std::sync::Mutex;

use versekit_core::language::{Choice, set_thread_language};
use versekit_core::net::Fake;
use versekit_core::paths::Host;
use versekit_core::pypath::Flavor;

use super::*;

static OPENED: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn record(url: &str) -> bool {
    OPENED.lock().map(|mut list| list.push(url.to_string())).is_ok()
}

const PAGE: &str = r#"{"summaryStatus": "major_outage", "systems": [
    {"name": "Platform", "status": "operational"},
    {"name": "Persistent Universe", "status": "major_outage", "unresolvedIssues": [{"title": "PU down"}]}]}"#;

fn index() -> String {
    format!("{}/index.json", serverstatus::BASE)
}

fn german() -> std::sync::MutexGuard<'static, ()> {
    let guard = crate::language_lock();
    set_thread_language(Choice::De);
    guard
}

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    crate::install(&ctx);
    ctx.enable_accesskit();
    ctx
}

/// One frame; the accesskit nodes with role, label and the middle of their place.
fn frame(
    ctx: &egui::Context,
    state: &mut ServerStatusState,
    events: Vec<egui::Event>,
) -> Vec<(Role, String, egui::Pos2)> {
    let scheme = versekit_core::theme::current();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 6000.0))),
        events,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| show_page(ui, scheme, state));
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

/// Frames until the threads of the page are done, at most five seconds.
fn settle(ctx: &egui::Context, state: &mut ServerStatusState) -> Vec<(Role, String, egui::Pos2)> {
    for _ in 0..500 {
        let nodes = frame(ctx, state, Vec::new());
        if state.job.is_none() && state.notices_job.is_none() {
            return frame(ctx, state, Vec::new());
        }
        drop(nodes);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("the fetch did not finish");
}

fn has(nodes: &[(Role, String, egui::Pos2)], text: &str) -> bool {
    nodes.iter().any(|(_, label, _)| label.contains(text))
}

/// A data folder of its own for one test.
fn folder(name: &str) -> (Paths, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("versekit-serverstatus-seite-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let home = dir.to_string_lossy().into_owned();
    let host = Host::new(Flavor::native(), &[(versekit_core::legacy::ENV_HOME, home.as_str())], &home);
    (Paths::new(host), dir)
}

#[test]
fn the_preview_shows_the_systems_and_the_incidents_in_the_words_of_cig() {
    let _german = german();
    let ctx = context();
    let mut state = ServerStatusState::sample(now());
    frame(&ctx, &mut state, Vec::new());
    let nodes = frame(&ctx, &mut state, Vec::new());
    for word in ["Platform", "Persistent Universe", "operational", "maintenance", "Live Deployment", "Login Issues"] {
        assert!(has(&nodes, word), "missing: {word}");
    }
    for key in ["s_st_stoerung", "s_st_offen", "s_st_erledigt_kurz", "s_stv_hinweis", "s_st_letzte", "s_st_alle_zeigen"]
    {
        assert!(has(&nodes, &t(key)), "missing text: {key}");
    }
    assert!(has(&nodes, "Please stow your vehicles."), "the highlighted line");
    assert!(has(&nodes, &tf("s_st_zuletzt", &[("0", Arg::from("vor 2 Min."))])), "the age of the state");
}

#[test]
fn the_page_fetches_at_the_first_visit_and_then_looks_every_minute() {
    let _german = german();
    let ctx = context();
    let (paths, dir) = folder("takt");
    let net = Arc::new(Fake::default().with(&index(), PAGE).with_etag(&index(), "W/\"1\""));
    let mut state = ServerStatusState::with_net(&paths, Some(net.clone() as Arc<dyn Net>));
    let first = frame(&ctx, &mut state, Vec::new());
    assert!(has(&first, &t("s_st_leer")) || has(&first, "Platform"), "the state kept shows at once");
    let nodes = settle(&ctx, &mut state);
    assert!(has(&nodes, "Persistent Universe") && has(&nodes, "major_outage") && has(&nodes, &t("s_st_stoerung")));
    let asked = net.asked().len();
    // A minute on: one look, with the tag, and nothing redrawn.
    state.next_look = 1.0;
    settle(&ctx, &mut state);
    let looked: Vec<String> = net.asked()[asked..].to_vec();
    assert_eq!(looked, [format!("{} (unchanged)", index())], "asked with the tag");
    assert!(versekit_core::serverstatus::stored_state(&paths).systems.len() == 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn refresh_now_asks_without_the_tag_and_the_source_opens_in_the_browser() {
    let _german = german();
    let ctx = context();
    let (paths, dir) = folder("knopf");
    let net = Arc::new(Fake::default().with(&index(), PAGE).with_etag(&index(), "W/\"1\""));
    let mut state = ServerStatusState::with_net(&paths, Some(net.clone() as Arc<dyn Net>)).with_opener(record);
    let nodes = settle(&ctx, &mut state);
    let asked = net.asked().len();
    let button =
        nodes.iter().find(|(role, text, _)| *role == Role::Button && *text == t("s_st_nachsehen")).expect("button");
    frame(&ctx, &mut state, click(button.2));
    let nodes = settle(&ctx, &mut state);
    assert!(net.asked()[asked..].contains(&index()), "forced: without the tag");
    let link = nodes
        .iter()
        .find(|(role, text, _)| *role == Role::Link && text.starts_with(serverstatus::BASE))
        .expect("the source is a link");
    let before = OPENED.lock().map(|list| list.len()).unwrap_or(0);
    frame(&ctx, &mut state, click(link.2));
    let opened = OPENED.lock().map(|list| list[before..].to_vec()).unwrap_or_default();
    assert_eq!(opened, [format!("{}/", serverstatus::BASE)]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn without_the_net_the_last_state_shows_with_a_note() {
    let _german = german();
    let ctx = context();
    let (paths, dir) = folder("ohne");
    let mut state = ServerStatusState::with_net(&paths, Some(Arc::new(Fake::default()) as Arc<dyn Net>));
    let nodes = settle(&ctx, &mut state);
    assert!(has(&nodes, &t("s_stv_kein_netz")), "never fetched: no net");
    // A state from before, then the net is gone.
    let up = Fake::default().with(&index(), PAGE);
    versekit_core::serverstatus::state(&paths, &up, now(), true, None);
    let mut state = ServerStatusState::with_net(&paths, Some(Arc::new(Fake::default()) as Arc<dyn Net>));
    state.fetch(&ctx, false, true);
    state.next_look = f64::INFINITY;
    let nodes = settle(&ctx, &mut state);
    assert!(has(&nodes, &t("s_stv_alt_ohne_netz")) && has(&nodes, "Persistent Universe"));
    let _ = std::fs::remove_dir_all(&dir);
}
