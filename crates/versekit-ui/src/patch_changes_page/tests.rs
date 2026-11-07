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

use egui::Role;
use versekit_core::erkul_fetch::pack;
use versekit_core::language::{Choice, set_language};
use versekit_core::paths::Host;
use versekit_core::pypath::Flavor;

use super::*;

fn german() -> std::sync::MutexGuard<'static, ()> {
    let guard = crate::language_lock();
    set_language(Choice::De);
    guard
}

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    crate::install(&ctx);
    ctx.enable_accesskit();
    ctx
}

/// One frame of the page; the accesskit nodes with role, label and the middle of their place.
fn frame(
    ctx: &egui::Context,
    state: &mut PatchChangesState,
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

/// A frame of another page: the page is left.
fn elsewhere(ctx: &egui::Context) {
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.label("woanders");
    });
    output.textures_delta.clear();
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

fn has(nodes: &[(Role, String, egui::Pos2)], text: &str) -> bool {
    nodes.iter().any(|(_, label, _)| label.contains(text))
}

fn folder(name: &str) -> (Paths, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("versekit-patches-seite-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let home = dir.to_string_lossy().into_owned();
    let host = Host::new(Flavor::native(), &[(versekit_core::legacy::ENV_HOME, home.as_str())], &home);
    (Paths::new(host), dir)
}

/// Keeps a patch in the data folder as the fetch does.
fn keep(paths: &Paths, version: &str, kind: &str) {
    let catalog = format!(
        r#"{{"patches": [{{"dataVersion": "{version}", "generatedAt": "2026-10-02T10:00:00Z",
            "summary": {{"modified": 1}}, "path": "c.bin"}}]}}"#
    );
    let changelog = format!(
        r#"{{"categories": [{{"kind": "{kind}", "modified": [{{"name": "Thing", "changes": [
            {{"path": "weapon.ammo.speed", "oldValue": 100, "newValue": 120}}]}}]}}]}}"#
    );
    let feed = FnFeed(move |path: &str| match path {
        "LIVE/catalog.bin" => Some(pack(&catalog)),
        "LIVE/c.bin" => Some(pack(&changelog)),
        _ => None,
    });
    assert_eq!(patch_changes::sync(paths, &feed).map(|v| v.len()), Ok(1));
}

#[test]
fn the_preview_shows_the_list_the_areas_and_the_fields() {
    let _german = german();
    let ctx = context();
    let mut state = PatchChangesState::sample();
    frame(&ctx, &mut state, Vec::new());
    let nodes = frame(&ctx, &mut state, Vec::new());
    for text in ["4.10.0-LIVE.12519617", "4.9.0-LIVE.12232306", "Drake Cutter", "MISC Starlancer TAC"] {
        assert!(has(&nodes, text), "missing: {text}");
    }
    assert!(!has(&nodes, "4.10.1-LIVE"), "a patch without changes is not listed");
    assert!(has(&nodes, &tf("s_pav_leere_weg", &[("n", Arg::from(2))])), "but counted");
    assert!(has(&nodes, &t("s_pa_nur_hier")), "the patch the source dropped");
    assert!(has(&nodes, &format!("{} 3", t("s_pa_alle"))) && has(&nodes, "ships 2") && has(&nodes, "weapons 1"));
    assert!(!has(&nodes, "Behring M5A Cannon"), "the largest area stands chosen");
    assert!(has(&nodes, "30000 → 36000  (+20 %)"), "a field with its change");
    let weapons = nodes.iter().find(|(role, text, _)| *role == Role::Button && text == "weapons 1").expect("area");
    frame(&ctx, &mut state, click(weapons.2));
    let nodes = frame(&ctx, &mut state, Vec::new());
    assert!(has(&nodes, "Behring M5A Cannon") && has(&nodes, &tf("s_pa_weggefallen", &[("alt", Arg::from("2.5"))])));
    assert!(has(&nodes, &tf("s_pa_groesse", &[("n", Arg::from("5"))])));
}

#[test]
fn an_empty_collection_says_how_to_fill_it_and_fetching_without_the_net_says_so() {
    let _german = german();
    let ctx = context();
    let (paths, dir) = folder("leer");
    let mut state = PatchChangesState::with_files(&paths);
    let nodes = frame(&ctx, &mut state, Vec::new());
    assert!(has(&nodes, &tf("s_pav_erst_holen", &[("knopf", Arg::from(t("s_pa_suchen")))])));
    let button =
        nodes.iter().find(|(role, text, _)| *role == Role::Button && *text == t("s_pa_suchen")).expect("button");
    frame(&ctx, &mut state, click(button.2));
    let mut nodes = Vec::new();
    for _ in 0..500 {
        nodes = frame(&ctx, &mut state, Vec::new());
        if state.sync_job.is_none() {
            nodes = frame(&ctx, &mut state, Vec::new());
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(has(&nodes, &t("s_pa_kein_netz")), "the tests never ask erkul: no net");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_new_visit_builds_again_only_when_the_files_changed() {
    let _german = german();
    let ctx = context();
    let (paths, dir) = folder("wache");
    keep(&paths, "4.10.0-LIVE.1", "ships");
    let mut state = PatchChangesState::with_files(&paths);
    frame(&ctx, &mut state, Vec::new());
    let built = state.builds();
    elsewhere(&ctx);
    elsewhere(&ctx);
    frame(&ctx, &mut state, Vec::new());
    assert_eq!(state.builds(), built, "the same files: nothing built again");
    keep(&paths, "4.11.0-LIVE.2", "weapons");
    frame(&ctx, &mut state, Vec::new());
    assert_eq!(state.builds(), built, "while the page stays open nothing is looked at");
    elsewhere(&ctx);
    elsewhere(&ctx);
    let nodes = frame(&ctx, &mut state, Vec::new());
    assert_eq!(state.builds(), built + 1, "another file: built again");
    assert!(has(&nodes, "4.11.0-LIVE.2") && has(&nodes, &t("s_pa_nur_hier")));
    let _ = std::fs::remove_dir_all(&dir);
}
