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

use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Rect, pos2, vec2};
use versekit_core::jsonio;
use versekit_core::language::{Choice, set_language};
use versekit_core::paths::{COPY_DATA_FOLDER, Host};
use versekit_core::signature_scan::Gray;
use versekit_core::testdir;

use super::*;

const PICTURES: &str = include_str!("../../../versekit-core/tests/data/signatur-pruefbilder.json");

/// The first real crop of the test pictures with its number.
fn crop() -> (u64, Gray) {
    let file = jsonio::loads(PICTURES).unwrap();
    let Some(Value::Object(images)) = file.get("bilder") else { panic!("bilder") };
    let (key, raster) = &images[0];
    let number: String = key.split('#').next().unwrap().chars().filter(char::is_ascii_digit).collect();
    let Value::Array(rows) = raster else { panic!("rows") };
    let (mut data, mut w) = (Vec::new(), 0);
    for row in rows {
        let Value::Array(row) = row else { panic!("row") };
        w = row.len();
        data.extend(row.iter().map(|v| if let Value::Int(v) = v { v.parse::<u8>().unwrap() } else { 0 }));
    }
    (number.parse().unwrap(), Gray::new(w, rows.len(), data))
}

fn copy(name: &str) -> Paths {
    let root = testdir::fresh(name);
    std::fs::create_dir_all(root.join(COPY_DATA_FOLDER)).expect("data folder");
    let host = Host::new(versekit_core::pypath::Flavor::native(), &[], &root.to_string_lossy());
    Paths::for_copy(host, &root.to_string_lossy()).expect("a copy")
}

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    crate::install(&ctx);
    ctx
}

fn input(events: Vec<Event>, time: f64) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(700.0, 600.0))),
        events,
        time: Some(time),
        ..Default::default()
    }
}

fn texts(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
    fn collect(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
        match shape {
            egui::epaint::Shape::Text(text) => out.push(text.galley.text().to_string()),
            egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut out));
    out
}

/// Draws the model once and says where it stands (the corner it begins at) and whether it changed the place.
fn model_frame(ctx: &egui::Context, layout: &mut Layout, events: Vec<Event>, time: f64) -> (Pos2, f32, bool) {
    let scheme = versekit_core::theme::current();
    let (mut start, mut width, mut changed) = (Pos2::ZERO, 0.0, false);
    let mut out = ctx.run_ui(input(events, time), |ui| {
        start = ui.cursor().min;
        width = ui.available_width().min(MODEL_WIDTH);
        changed = place_model(ui, scheme, layout, &Ground::MODEL, None);
    });
    out.textures_delta.clear();
    (start, width, changed)
}

fn press(at: Pos2) -> Vec<Event> {
    vec![
        Event::PointerMoved(at),
        Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE },
    ]
}

#[test]
fn a_small_crop_grows_by_whole_steps_and_a_wide_one_shrinks_to_the_room() {
    assert_eq!(picture_zoom((60, 20), 500.0), 6.0);
    assert_eq!(picture_zoom((120, 30), 500.0), 3.0);
    assert_eq!(picture_zoom((120, 30), 240.0), 2.0);
    // Wider than the window: drawn into the width there is, not beyond it.
    let wide = picture_zoom((600, 40), 416.0);
    assert!((600.0 * wide - 416.0).abs() < 0.01, "{wide}");
}

#[test]
fn dragging_the_window_in_the_model_gives_the_offset_and_the_window_follows_the_hand() {
    let ctx = context();
    let mut layout = Layout::default();
    let (start, width, _) = model_frame(&ctx, &mut layout, vec![], 0.0);
    let scale = width / GAME.2 as f32;
    let size = signature_window::window_size(layout.size.font_px(GAME.3), 18, true);
    let (x, y) = signature_window::place(PILL, GAME, size, &layout);
    let centre = start + vec2(x as f32 + size.0 as f32 / 2.0, y as f32 + size.1 as f32 / 2.0) * scale;
    // Hover, press, move by 40 points to the right and 20 down, release.
    model_frame(&ctx, &mut layout, vec![Event::PointerMoved(centre)], 0.1);
    model_frame(&ctx, &mut layout, press(centre), 0.2);
    let moved = centre + vec2(40.0, 20.0);
    let (_, _, changed) = model_frame(&ctx, &mut layout, vec![Event::PointerMoved(moved)], 0.3);
    let (_, _, changed_again) = model_frame(&ctx, &mut layout, vec![Event::PointerMoved(moved)], 0.4);
    assert!(changed || changed_again, "the drag changed nothing");
    // 40 points on the model are 40 / scale points in the game: per mille of 1600 and 900.
    let expected_dx = (40.0 / scale / GAME.2 as f32 * 1000.0).round() as i32;
    let expected_dy = (20.0 / scale / GAME.3 as f32 * 1000.0).round() as i32;
    assert!((layout.dx - expected_dx).abs() <= 2, "{} against {expected_dx}", layout.dx);
    assert!((layout.dy - expected_dy).abs() <= 2, "{} against {expected_dy}", layout.dy);
    let (_, _, idle) = model_frame(
        &ctx,
        &mut layout,
        vec![Event::PointerButton {
            pos: moved,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
        0.5,
    );
    assert!(!idle, "letting go changes nothing more");
}

#[test]
fn the_arrow_keys_move_the_window_when_the_model_has_the_focus_and_stay_within_the_limit() {
    let ctx = context();
    let mut layout = Layout::default();
    // Focus the model by a click, then press the keys.
    let (start, width, _) = model_frame(&ctx, &mut layout, vec![], 0.0);
    let scale = width / GAME.2 as f32;
    let size = signature_window::window_size(layout.size.font_px(GAME.3), 18, true);
    let (x, y) = signature_window::place(PILL, GAME, size, &layout);
    let centre = start + vec2(x as f32 + size.0 as f32 / 2.0, y as f32 + size.1 as f32 / 2.0) * scale;
    model_frame(&ctx, &mut layout, press(centre), 0.1);
    model_frame(
        &ctx,
        &mut layout,
        vec![Event::PointerButton {
            pos: centre,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
        0.2,
    );
    let key = |key: Key| Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    let (_, _, changed) = model_frame(&ctx, &mut layout, vec![key(Key::ArrowRight)], 0.3);
    assert!(changed && layout.dx == KEY_STEP, "{layout:?}");
    model_frame(&ctx, &mut layout, vec![key(Key::ArrowUp)], 0.4);
    assert_eq!(layout.dy, -KEY_STEP);
    layout.dx = signature_window::OFFSET_LIMIT;
    model_frame(&ctx, &mut layout, vec![key(Key::ArrowRight)], 0.5);
    assert_eq!(layout.dx, signature_window::OFFSET_LIMIT);
}

#[test]
fn without_a_game_picture_the_window_model_shows_the_signature_as_large_as_the_frame_model() {
    // A place kept on a wide screen: a few per mille, far too small for the model.
    let kept = Some([493, 60, 23, 28]);
    assert_eq!(shown_pill(&Ground::MODEL, kept), None, "the place of the model");
    let picture = Ground { texture: Some(egui::TextureId::Managed(7)), size: (5120, 1440) };
    assert_eq!(shown_pill(&picture, kept), kept, "on the picture the real place");
    assert_eq!(shown_pill(&picture, None), None);
}

#[test]
fn the_model_says_what_it_shows_in_both_languages_and_stays_inside_a_narrow_page() {
    let _language = crate::language_lock();
    let ctx = context();
    let scheme = versekit_core::theme::current();
    let mut seen = Vec::new();
    for choice in [Choice::De, Choice::En] {
        set_language(choice);
        let (game, pill) = (t("s_sg_modell_spiel"), signature_window::with_thousands(EXAMPLE));
        seen.push(game.clone());
        for width in [260.0, 700.0] {
            let mut layout = Layout::default();
            let mut over = 0.0f32;
            let mut shown = Vec::new();
            for _ in 0..2 {
                let mut out = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 600.0))),
                        ..Default::default()
                    },
                    |ui| {
                        place_model(ui, scheme, &mut layout, &Ground::MODEL, None);
                        over = (ui.min_rect().width() - width).max(0.0);
                    },
                );
                out.textures_delta.clear();
                shown = texts(&out.shapes);
            }
            assert!(shown.contains(&game) && shown.contains(&pill), "{choice:?} {shown:?}");
            assert!(over <= 0.5, "sticks out by {over} at {width}");
        }
    }
    assert_ne!(seen[0], seen[1], "the model is named in both languages");
    set_language(Choice::De);
}

#[test]
fn teaching_stores_the_digits_says_what_came_of_it_and_the_check_reads_the_picture_again() {
    let _language = crate::language_lock();
    let paths = copy("scanner-ui-anlernen");
    let mut state = ScannerState::with_files(&paths);
    let (number, crop) = crop();
    let mining = MiningData::default();
    // No picture yet.
    state.learn(&mining);
    assert_eq!(state.said.as_ref().map(|s| s.0.clone()), Some(vec![t("scan_warte")]));
    state.frame = Some(Frame::new(crop, Some(number), 1));
    // Nothing typed: the reason, and nothing is stored.
    state.learn(&mining);
    assert_eq!(state.said, Some((vec![t("scan_grund_anlernen_leer")], false)));
    state.typed = number.to_string();
    state.learn(&mining);
    let (lines, _) = state.said.clone().expect("said");
    let formatted = signature_window::with_thousands(number);
    assert!(lines[0].contains(&formatted), "{lines:?}");
    assert!(lines.len() >= 2, "the check line follows: {lines:?}");
    assert!(state.typed.is_empty(), "the field is emptied after a teaching");
    let (own, samples, _) = state.files().expect("files");
    assert!(std::path::Path::new(own.of(signature_scan::Variant::Green)).is_file());
    assert!(!std::path::Path::new(own.of(signature_scan::Variant::Bright)).exists(), "this frame has no bright image");
    assert!(!std::path::Path::new(&own.shared).exists(), "the shared file is not written any more");
    assert_eq!(signature_scan::samples(&samples).len(), 1);
}

#[test]
fn teaching_a_picture_with_both_grey_images_stores_each_in_its_own_file_and_keeps_one_image() {
    let _language = crate::language_lock();
    let paths = copy("scanner-ui-anlernen-beide");
    let mut state = ScannerState::with_files(&paths);
    let (number, crop) = crop();
    let brighter = Gray::new(crop.w, crop.h, crop.data.iter().map(|v| v.saturating_add(25)).collect());
    state.frame = Some(Frame {
        image: brighter.clone(),
        variant: Some(signature_scan::Variant::Bright),
        images: signature_scan::Images { green: Some(crop), bright: Some(brighter) },
        value: Some(number),
        number: 1,
    });
    state.typed = number.to_string();
    state.learn(&MiningData::default());
    let (own, samples, _) = state.files().expect("files");
    for variant in signature_scan::VARIANTS {
        assert!(std::path::Path::new(own.of(variant)).is_file(), "{variant:?}");
    }
    assert!(!std::path::Path::new(&own.shared).exists());
    assert_eq!(signature_scan::samples(&samples).len(), 1, "one image for the number");
}

#[test]
fn the_picture_of_the_overlay_is_taken_while_none_stands_and_the_next_one_is_a_newer_one() {
    let paths = copy("scanner-ui-bild");
    let mut state = ScannerState::with_files(&paths);
    let (number, crop) = crop();
    let (_, _, file) = state.files().expect("files");
    let ctx = context();
    let follow = |state: &mut ScannerState, time: f64| {
        ctx.run_ui(input(vec![], time), |ui| state.follow(ui)).textures_delta.clear();
    };
    follow(&mut state, 0.0);
    assert!(state.frame.is_none(), "no file yet");
    assert!(signature_watch::write_last_frame(&file, &Frame::new(crop.clone(), Some(number), 4)));
    follow(&mut state, 1.0);
    assert_eq!(state.frame.as_ref().map(|f| f.number), Some(4));
    // A newer one does not replace the standing one.
    assert!(signature_watch::write_last_frame(&file, &Frame::new(crop.clone(), None, 5)));
    follow(&mut state, 2.0);
    assert_eq!(state.frame.as_ref().map(|f| f.number), Some(4));
    // "Next image": the one that stood is set aside, the file still holds 5.
    state.next();
    follow(&mut state, 3.0);
    assert_eq!(state.frame.as_ref().map(|f| f.number), Some(5));
    state.next();
    follow(&mut state, 4.0);
    assert!(state.frame.is_none(), "5 was set aside, nothing newer yet");
    assert!(signature_watch::write_last_frame(&file, &Frame::new(crop, None, 6)));
    follow(&mut state, 5.0);
    assert_eq!(state.frame.as_ref().map(|f| f.number), Some(6));
}

#[test]
fn the_settings_of_the_scanner_go_to_the_file_under_their_names_and_come_back() {
    let paths = copy("scanner-ui-einstellungen");
    let mut state = ScannerState::with_files(&paths);
    assert!(!state.on, "off by default");
    state.on = true;
    state.layout = Layout {
        dx: -120,
        dy: 45,
        size: signature_window::Size(14),
        overlay_line: true,
        place: signature_window::Place::AboveSignature,
        linger: 12,
    };
    state.search_frame = Some([300, 20, 400, 120]);
    for (key, value) in state.setting_values() {
        assert!(paths.set_setting(key, value), "{key}");
    }
    let again = ScannerState::with_files(&paths);
    assert!(again.on);
    assert_eq!(again.layout, state.layout);
    assert_eq!(again.search_frame, Some([300, 20, 400, 120]));
    let names: Vec<&str> = state.setting_values().iter().map(|(k, _)| *k).collect();
    assert_eq!(
        names,
        [
            "signatur_wache",
            "signatur_fenster_dx",
            "signatur_fenster_dy",
            "signatur_fenster_groesse",
            "signatur_overlay_zeile",
            "signatur_fenster_ort",
            "signatur_fenster_nachlauf",
            "signatur_rahmen"
        ]
    );
    // The old choice "Abbau mit" in the settings file is ignored without a word.
    assert!(paths.set_setting("signatur_geraet", Value::str("alles")));
    assert_eq!(ScannerState::with_files(&paths).layout, state.layout);
    // A removed frame is stored as nothing and read as none.
    state.search_frame = None;
    for (key, value) in state.setting_values() {
        assert!(paths.set_setting(key, value), "{key}");
    }
    assert_eq!(ScannerState::with_files(&paths).search_frame, None);
}

#[test]
fn one_drag_on_the_model_draws_the_search_frame_in_per_mille_of_the_window() {
    let ctx = context();
    let scheme = versekit_core::theme::current();
    let mut frame: Option<[i32; 4]> = None;
    let run = |events: Vec<Event>, time: f64, frame: &mut Option<[i32; 4]>| -> (Pos2, f32, bool) {
        let (mut start, mut width, mut changed) = (Pos2::ZERO, 0.0, false);
        let mut out = ctx.run_ui(input(events, time), |ui| {
            start = ui.cursor().min;
            width = ui.available_width().min(MODEL_WIDTH);
            changed = frame_model(ui, scheme, frame, None, &Ground::MODEL);
        });
        out.textures_delta.clear();
        (start, width, changed)
    };
    let (start, width, _) = run(vec![], 0.0, &mut frame);
    let height = width * GAME.3 as f32 / GAME.2 as f32;
    // From a fifth of the width and a tenth of the height to half the width and a third of the height.
    let from = start + vec2(width * 0.2, height * 0.1);
    let to = start + vec2(width * 0.5, height * 0.33);
    run(vec![Event::PointerMoved(from)], 0.1, &mut frame);
    run(press(from), 0.2, &mut frame);
    let (_, _, moving) = run(vec![Event::PointerMoved(to)], 0.3, &mut frame);
    run(vec![Event::PointerMoved(to)], 0.4, &mut frame);
    assert!(!moving && frame.is_none(), "nothing is stored while the drag runs: {frame:?}");
    let (_, _, changed) = run(vec![release(to)], 0.5, &mut frame);
    assert!(changed, "letting go stores the frame");
    let drawn = frame.expect("a frame was drawn");
    let near = |a: i32, b: i32| (a - b).abs() <= 25;
    assert!(near(drawn[0], 200) && near(drawn[1], 100) && near(drawn[2], 300) && near(drawn[3], 230), "{drawn:?}");
    // Nothing more after letting go.
    let (_, _, idle) = run(vec![Event::PointerMoved(to)], 0.6, &mut frame);
    assert!(!idle);
    assert_eq!(frame, Some(drawn));
}

fn release(at: Pos2) -> Event {
    Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE }
}

// ------------------------------------------------------------ moving and sizing the frame

#[test]
fn per_mille_and_the_model_turn_into_each_other_both_ways() {
    let rect = Rect::from_min_size(pos2(37.0, 112.0), vec2(460.0, 192.6));
    for frame in [[0, 0, 1000, 1000], [300, 20, 400, 120], [455, 61, 90, 38], [990, 990, 10, 10]] {
        let area = mille_to_model(rect, frame);
        let (x0, y0) = model_to_mille(rect, area.min);
        let (x1, y1) = model_to_mille(rect, area.max);
        let back = [x0.round() as i32, y0.round() as i32, (x1 - x0).round() as i32, (y1 - y0).round() as i32];
        assert_eq!(back, frame);
    }
    // The corners of the model are 0 and 1000.
    assert_eq!(model_to_mille(rect, rect.min), (0.0, 0.0));
    let (x, y) = model_to_mille(rect, rect.max);
    assert!((x - 1000.0).abs() < 0.01 && (y - 1000.0).abs() < 0.01, "{x} {y}");
}

#[test]
fn a_press_takes_hold_of_a_handle_before_the_frame_and_outside_of_nothing() {
    let frame = Some([300, 200, 400, 100]);
    let reach = (8.0, 15.0);
    assert_eq!(grip_at(frame, (300.0, 200.0), reach), Grip::Corner(0));
    assert_eq!(grip_at(frame, (706.0, 190.0), reach), Grip::Corner(1));
    assert_eq!(grip_at(frame, (295.0, 312.0), reach), Grip::Corner(2));
    assert_eq!(grip_at(frame, (700.0, 300.0), reach), Grip::Corner(3));
    assert_eq!(grip_at(frame, (500.0, 250.0), reach), Grip::Move);
    assert_eq!(grip_at(frame, (305.0, 250.0), reach), Grip::Move, "the edge between two handles moves");
    assert_eq!(grip_at(frame, (100.0, 100.0), reach), Grip::New);
    assert_eq!(grip_at(frame, (720.0, 250.0), reach), Grip::New);
    assert_eq!(grip_at(None, (500.0, 250.0), reach), Grip::New);
    // The pointer says what a press would do.
    assert_eq!(Grip::Move.cursor(false), CursorIcon::Grab);
    assert_eq!(Grip::Move.cursor(true), CursorIcon::Grabbing);
    assert_eq!(Grip::Corner(0).cursor(false), CursorIcon::ResizeNwSe);
    assert_eq!(Grip::Corner(3).cursor(false), CursorIcon::ResizeNwSe);
    assert_eq!(Grip::Corner(1).cursor(false), CursorIcon::ResizeNeSw);
    assert_eq!(Grip::Corner(2).cursor(false), CursorIcon::ResizeNeSw);
    assert_eq!(Grip::New.cursor(false), CursorIcon::Crosshair);
}

#[test]
fn moving_keeps_the_size_and_the_frame_stays_inside_the_picture() {
    let start = Some([300, 200, 400, 100]);
    assert_eq!(drag_frame(start, Grip::Move, (500, 250), (560, 230)), Some([360, 180, 400, 100]));
    // Far over the right and lower edge: it stops at the edge, the size stays.
    assert_eq!(drag_frame(start, Grip::Move, (500, 250), (1400, 1600)), Some([600, 900, 400, 100]));
    // Over the left and upper edge.
    assert_eq!(drag_frame(start, Grip::Move, (500, 250), (-300, -500)), Some([0, 0, 400, 100]));
    for to in [(-5000, -5000), (5000, 5000), (0, 1000), (1000, 0), (731, 12)] {
        let [x, y, w, h] = drag_frame(start, Grip::Move, (500, 250), to).expect("a frame");
        assert!(x >= 0 && y >= 0 && x + w <= 1000 && y + h <= 1000, "{to:?}: {x} {y} {w} {h}");
        assert_eq!((w, h), (400, 100));
    }
}

#[test]
fn a_handle_changes_the_size_with_the_other_corner_fixed_and_inside_the_picture() {
    let start = Some([300, 200, 400, 100]);
    // The right bottom corner out by 50 and 30.
    assert_eq!(drag_frame(start, Grip::Corner(3), (700, 300), (750, 330)), Some([300, 200, 450, 130]));
    // The left top corner in by 100 and 20: the right bottom one stays.
    assert_eq!(drag_frame(start, Grip::Corner(0), (300, 200), (400, 220)), Some([400, 220, 300, 80]));
    // The right top corner beyond the window: cut at the edge.
    assert_eq!(drag_frame(start, Grip::Corner(1), (700, 200), (1300, -400)), Some([300, 0, 700, 300]));
    // Pulled over the other corner: the frame turns over and stays a frame.
    assert_eq!(drag_frame(start, Grip::Corner(3), (700, 300), (100, 100)), Some([100, 100, 200, 100]));
    // Smaller than the least: no frame, the one before stays.
    assert_eq!(drag_frame(start, Grip::Corner(3), (700, 300), (305, 205)), None);
    // A new one from the press to the pointer, cut at the window.
    assert_eq!(drag_frame(start, Grip::New, (100, 100), (50, 1200)), Some([50, 100, 50, 900]));
    assert_eq!(drag_frame(None, Grip::Move, (100, 100), (300, 300)), Some([100, 100, 200, 200]), "no frame: drawn");
}

/// Runs the model of the frame on a ground and gives where it stood, what it said and the pointer it showed.
fn frame_run(
    ctx: &egui::Context,
    ground: &Ground,
    frame: &mut Option<[i32; 4]>,
    events: Vec<Event>,
    time: f64,
) -> (Rect, bool, CursorIcon) {
    let scheme = versekit_core::theme::current();
    let (mut rect, mut changed) = (Rect::NOTHING, false);
    let mut out = ctx.run_ui(input(events, time), |ui| {
        let start = ui.cursor().min;
        changed = frame_model(ui, scheme, frame, None, ground);
        rect = Rect::from_min_max(start, ui.min_rect().max);
    });
    out.textures_delta.clear();
    (rect, changed, out.platform_output.cursor_icon)
}

/// A drag on the model from `from` to `to`, in steps like a hand; gives whether letting go changed the frame.
fn drag(ctx: &egui::Context, ground: &Ground, frame: &mut Option<[i32; 4]>, from: Pos2, to: Pos2, time: f64) -> bool {
    frame_run(ctx, ground, frame, vec![Event::PointerMoved(from)], time);
    frame_run(ctx, ground, frame, press(from), time + 0.05);
    for step in 1..=4 {
        let at = from + (to - from) * (step as f32 / 4.0);
        frame_run(ctx, ground, frame, vec![Event::PointerMoved(at)], time + 0.05 + step as f32 as f64 * 0.05);
    }
    frame_run(ctx, ground, frame, vec![release(to)], time + 0.4).1
}

#[test]
fn on_the_model_the_frame_moves_and_changes_its_size_by_hand_and_is_stored_when_let_go() {
    let ctx = context();
    let ground = Ground::MODEL;
    let mut frame = Some([300, 100, 300, 200]);
    let (rect, _, _) = frame_run(&ctx, &ground, &mut frame, vec![], 0.0);
    let point = |x: i32, y: i32| rect.min + vec2(x as f32 / 1000.0 * rect.width(), y as f32 / 1000.0 * rect.height());

    // The pointer over the frame, over a handle, outside.
    let (_, _, inside) = frame_run(&ctx, &ground, &mut frame, vec![Event::PointerMoved(point(450, 200))], 0.1);
    assert_eq!(inside, CursorIcon::Grab);
    let (_, _, corner) = frame_run(&ctx, &ground, &mut frame, vec![Event::PointerMoved(point(600, 300))], 0.2);
    assert_eq!(corner, CursorIcon::ResizeNwSe);
    let (_, _, outside) = frame_run(&ctx, &ground, &mut frame, vec![Event::PointerMoved(point(800, 800))], 0.3);
    assert_eq!(outside, CursorIcon::Crosshair);

    // Inside: moved by 100 and 50 per mille, the size stays.
    assert!(drag(&ctx, &ground, &mut frame, point(450, 200), point(550, 250), 1.0));
    let [x, y, w, h] = frame.expect("a frame");
    assert!((x - 400).abs() <= 3 && (y - 150).abs() <= 3 && (w, h) == (300, 200), "{frame:?}");

    // The right bottom handle: larger, the left top corner stays.
    let before = frame.expect("a frame");
    assert!(drag(&ctx, &ground, &mut frame, point(before[0] + 300, before[1] + 200), point(900, 600), 2.0));
    let [x2, y2, w2, h2] = frame.expect("a frame");
    assert_eq!((x2, y2), (before[0], before[1]), "the other corner stays");
    assert!((x2 + w2 - 900).abs() <= 3 && (y2 + h2 - 600).abs() <= 3, "{frame:?}");

    // Moved far over the edge: it stops inside the picture.
    let held = frame.expect("a frame");
    let centre = point(held[0] + held[2] / 2, held[1] + held[3] / 2);
    assert!(drag(&ctx, &ground, &mut frame, centre, centre + vec2(2000.0, 2000.0), 3.0));
    let [x3, y3, w3, h3] = frame.expect("a frame");
    assert_eq!((x3 + w3, y3 + h3, w3, h3), (1000, 1000, w2, h2), "at the right bottom edge, same size");

    // Outside: a new frame.
    assert!(drag(&ctx, &ground, &mut frame, point(20, 20), point(220, 120), 4.0));
    let [x4, y4, w4, h4] = frame.expect("a frame");
    assert!((x4 - 20).abs() <= 3 && (y4 - 20).abs() <= 3 && (w4 - 200).abs() <= 3 && (h4 - 100).abs() <= 3);
}

#[test]
fn the_frame_moved_is_written_to_the_settings_only_when_let_go() {
    let paths = copy("scanner-ui-rahmen-loslassen");
    let ctx = context();
    let ground = Ground::MODEL;
    let mut state = ScannerState::with_files(&paths);
    state.search_frame = Some([300, 100, 300, 200]);
    let stored = |state: &ScannerState| {
        for (key, value) in state.setting_values() {
            assert!(paths.set_setting(key, value), "{key}");
        }
    };
    stored(&state);
    let (rect, _, _) = frame_run(&ctx, &ground, &mut state.search_frame, vec![], 0.0);
    let point = |x: i32, y: i32| rect.min + vec2(x as f32 / 1000.0 * rect.width(), y as f32 / 1000.0 * rect.height());
    frame_run(&ctx, &ground, &mut state.search_frame, press(point(450, 200)), 0.1);
    frame_run(&ctx, &ground, &mut state.search_frame, vec![Event::PointerMoved(point(500, 220))], 0.2);
    frame_run(&ctx, &ground, &mut state.search_frame, vec![Event::PointerMoved(point(550, 250))], 0.3);
    // The page writes what changed each frame: during the drag that is nothing.
    assert_eq!(state.search_frame, Some([300, 100, 300, 200]), "unchanged while held");
    stored(&state);
    assert_eq!(Hint::read(&paths).frame, Some([300, 100, 300, 200]));
    frame_run(&ctx, &ground, &mut state.search_frame, vec![release(point(550, 250))], 0.4);
    stored(&state);
    let kept = Hint::read(&paths).frame.expect("a frame in the settings");
    assert!((kept[0] - 400).abs() <= 3 && (kept[1] - 150).abs() <= 3 && kept[2..] == [300, 200], "{kept:?}");
    assert_eq!(ScannerState::with_files(&paths).search_frame, Some(kept), "the same key as before");
}

#[test]
fn the_arrow_keys_move_the_frame_finely_and_not_out_of_the_picture() {
    let ctx = context();
    let ground = Ground::MODEL;
    let mut frame = Some([0, 100, 300, 200]);
    let (rect, _, _) = frame_run(&ctx, &ground, &mut frame, vec![], 0.0);
    let inside = rect.min + vec2(rect.width() * 0.1, rect.height() * 0.2);
    frame_run(&ctx, &ground, &mut frame, press(inside), 0.1);
    frame_run(&ctx, &ground, &mut frame, vec![release(inside)], 0.2);
    let key = |key: Key| Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    let (_, changed, _) = frame_run(&ctx, &ground, &mut frame, vec![key(Key::ArrowDown)], 0.3);
    assert!(changed);
    assert_eq!(frame, Some([0, 100 + FRAME_KEY_STEP, 300, 200]));
    let (_, at_edge, _) = frame_run(&ctx, &ground, &mut frame, vec![key(Key::ArrowLeft)], 0.4);
    assert!(!at_edge, "at the left edge already");
    assert_eq!(frame, Some([0, 100 + FRAME_KEY_STEP, 300, 200]));
}

#[test]
fn the_arrow_keys_keep_the_focus_on_the_model_with_buttons_beside_it() {
    let ctx = context();
    let scheme = versekit_core::theme::current();
    let ground = Ground::MODEL;
    let mut frame = Some([300, 100, 300, 200]);
    // The model with two buttons below it, as on the page.
    let run = |events: Vec<Event>, time: f64, frame: &mut Option<[i32; 4]>| -> Rect {
        let mut rect = Rect::NOTHING;
        let mut out = ctx.run_ui(input(events, time), |ui| {
            let start = ui.cursor().min;
            frame_model(ui, scheme, frame, None, &ground);
            rect = Rect::from_min_max(start, ui.min_rect().max);
            let _ = ui.button("a");
            let _ = ui.button("b");
        });
        out.textures_delta.clear();
        rect
    };
    let rect = run(vec![], 0.0, &mut frame);
    let inside = rect.min + vec2(rect.width() * 0.45, rect.height() * 0.2);
    run(press(inside), 0.1, &mut frame);
    run(vec![release(inside)], 0.2, &mut frame);
    // The frame the model asks for when it gains the focus.
    assert!(ctx.has_requested_repaint(), "the focus asks for a frame");
    run(vec![], 0.25, &mut frame);
    let key = |key: Key| Event::Key { key, physical_key: Some(key), pressed: true, repeat: false, modifiers: Modifiers::NONE };
    // Four presses in a row: each one moves the frame, none moves the focus to a button.
    for (step, time) in [(1, 0.3), (2, 0.4), (3, 0.5), (4, 0.6)] {
        run(vec![key(Key::ArrowDown)], time, &mut frame);
        assert_eq!(frame, Some([300, 100 + step * FRAME_KEY_STEP, 300, 200]), "press {step}");
    }
}

#[test]
fn a_small_frame_can_still_be_taken_by_its_inside_and_a_large_one_by_its_corners() {
    let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(460.0, 258.75));
    // The smallest frame: 10 per mille, under 5 points on the model.
    let small = Some([500, 500, FRAME_MIN, FRAME_MIN + 4]);
    let reach = handle_reach(rect, small);
    assert_eq!(grip_at(small, (505.0, 507.0), reach), Grip::Move, "the middle moves it");
    assert_eq!(grip_at(small, (500.0, 500.0), reach), Grip::Corner(0));
    // A large frame: the full reach of a handle.
    let large = Some([200, 200, 400, 300]);
    let (across, down) = handle_reach(rect, large);
    assert!((across - HANDLE_REACH / 460.0 * 1000.0).abs() < 0.01 && (down - HANDLE_REACH / 258.75 * 1000.0).abs() < 0.01);
    assert_eq!(grip_at(large, (200.0 + across * 0.9, 200.0), (across, down)), Grip::Corner(0));
}

#[test]
fn the_handle_under_the_pointer_grows_and_the_focus_rings_the_model() {
    let ctx = context();
    let scheme = versekit_core::theme::current();
    let ground = Ground::MODEL;
    let mut frame = Some([300, 100, 300, 200]);
    let draw = |events: Vec<Event>, time: f64, frame: &mut Option<[i32; 4]>| {
        let mut shapes = Vec::new();
        let mut rect = Rect::NOTHING;
        let mut out = ctx.run_ui(input(events, time), |ui| {
            let start = ui.cursor().min;
            frame_model(ui, scheme, frame, None, &ground);
            rect = Rect::from_min_max(start, ui.min_rect().max);
        });
        out.textures_delta.clear();
        shapes.extend(out.shapes);
        (rect, shapes)
    };
    let big = |shapes: &[egui::epaint::ClippedShape]| {
        rects(shapes).into_iter().filter(|r| (r.rect.width() - HANDLE * 1.5).abs() < 0.5 && r.fill == HUD_TEXT).count()
    };
    let ring = |shapes: &[egui::epaint::ClippedShape], rect: Rect| {
        rects(shapes).into_iter().any(|r| r.stroke.color == color(scheme.accent) && r.rect.contains_rect(rect.expand(1.0)))
    };
    let (rect, plain) = draw(vec![], 0.0, &mut frame);
    assert_eq!(big(&plain), 0, "no handle grown without the pointer");
    assert!(!ring(&plain, rect), "no ring without the focus");
    let corner = rect.min + vec2(0.6 * rect.width(), 0.3 * rect.height());
    draw(vec![Event::PointerMoved(corner)], 0.1, &mut frame);
    let (_, over) = draw(vec![Event::PointerMoved(corner)], 0.2, &mut frame);
    assert_eq!(big(&over), 1, "the handle under the pointer grows");
    let inside = rect.min + vec2(0.45 * rect.width(), 0.2 * rect.height());
    draw(press(inside), 0.3, &mut frame);
    draw(vec![release(inside)], 0.4, &mut frame);
    let (_, focused) = draw(vec![Event::PointerMoved(rect.max + vec2(50.0, 50.0))], 0.5, &mut frame);
    assert!(ring(&focused, rect), "the model has the focus: a ring around it");
}

#[test]
fn the_buttons_and_the_line_of_the_game_picture_fit_into_the_cards_in_both_languages() {
    let _language = crate::language_lock();
    let paths = copy("scanner-ui-spielbild-raender");
    let ctx = context();
    let scheme = versekit_core::theme::current();
    for choice in [Choice::De, Choice::En] {
        set_language(choice);
        let mut state = ScannerState::with_files(&paths);
        state.search_frame = Some([300, 20, 400, 120]);
        // While it waits: the line with the countdown and the blocked button.
        state.start_fetch_from(&ctx, 0.0, Box::new(MadeScreen(None)), std::time::Duration::from_millis(1500));
        for width in [300.0, 420.0, 610.0, 900.0] {
            let (mut over, mut shown) = (0.0f32, Vec::new());
            for _ in 0..2 {
                let mut out = ctx.run_ui(
                    RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 1400.0))), ..Default::default() },
                    |ui| {
                        model_cards(ui, scheme, &mut state, ["Lage", "", &t("s_sg_lage_mitte")], ["Rahmen", "", &t("s_sg_rahmen_weg")]);
                        over = (ui.min_rect().width() - width).max(0.0);
                    },
                );
                out.textures_delta.clear();
                shown = texts(&out.shapes);
            }
            assert!(over <= 0.5, "{choice:?} sticks out by {over} at {width}");
            for key in ["s_sg_rahmen_weg", "s_sg_rahmen_zeigen", "s_sg_bild_holen"] {
                if key != "s_sg_bild_holen" || versekit_core::screen_grab::supported() {
                    assert!(shown.iter().any(|s| s.eq_ignore_ascii_case(&t(key))), "{choice:?} {width}: {key} missing in {shown:?}");
                }
            }
        }
        for _ in 0..400 {
            state.poll_fetch(&ctx);
            if state.fetch.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    set_language(Choice::De);
}

// ------------------------------------------------------------ the picture of the game behind the models

/// A made-up game window of `w` × `h` at `(x, y)` of the desktop: dark, with a white box where the signature is.
fn game_screen(w: i32, h: i32, signature: (i32, i32, i32, i32)) -> Vec<u8> {
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let lit =
                x >= signature.0 && x < signature.0 + signature.2 && y >= signature.1 && y < signature.1 + signature.3;
            out.extend(if lit { [255, 255, 255, 255] } else { [20, 30, 40, 255] });
        }
    }
    out
}

/// The bright box in a picture, in its pixels: left, top, right, bottom.
fn bright_box(picture: &GamePicture) -> (usize, usize, usize, usize) {
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for y in 0..picture.h {
        for x in 0..picture.w {
            if picture.rgb[(y * picture.w + x) * 3] > 128 {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
            }
        }
    }
    (x0, y0, x1, y1)
}

#[test]
fn a_frame_drawn_around_the_signature_in_the_picture_is_read_and_shown_in_the_game_at_that_very_place() {
    // Game windows: a wide one on a second screen right of the first, one on a screen left of it (negative
    // points), a window with its frame, and a plain 16 : 9 one.
    let windows = [(2560, 0, 3440, 1440), (-1920, -120, 1920, 1080), (113, 71, 1616, 939), (0, 0, 2560, 1440)];
    let ctx = context();
    for (round, game) in windows.into_iter().enumerate() {
        let time = round as f64 * 10.0;
        // The signature in the game window, in points from its corner, top in the middle.
        let signature = (game.2 * 47 / 100, game.3 * 9 / 100, game.2 / 14, game.3 / 26);
        let picture =
            GamePicture::from_bgra(&game_screen(game.2, game.3, signature), game.2, game.3).expect("a picture");
        let texture = ctx.load_texture(
            "probe",
            egui::ColorImage::from_rgb([picture.w, picture.h], &picture.rgb),
            TextureOptions::LINEAR,
        );
        let ground = Ground { texture: Some(texture.id()), size: picture.game };
        let mut frame = None;
        let (rect, _, _) = frame_run(&ctx, &ground, &mut frame, vec![], time);
        assert!((rect.height() / rect.width() - game.3 as f32 / game.2 as f32).abs() < 0.01, "the shape of {game:?}");
        // Where the player sees the signature on the model, the frame is drawn around it.
        let (x0, y0, x1, y1) = bright_box(&picture);
        let on_model = |x: usize, y: usize| {
            rect.min + vec2(x as f32 / picture.w as f32 * rect.width(), y as f32 / picture.h as f32 * rect.height())
        };
        assert!(drag(&ctx, &ground, &mut frame, on_model(x0, y0), on_model(x1, y1), time + 1.0), "{game:?}");
        // The area the scanner reads, and the one the overlay shows over the game: both are this one.
        let (area, drawn) = Hint { frame, places: Vec::new() }.narrow(game).expect("an area");
        assert!(drawn);
        let wanted = (game.0 + signature.0, game.1 + signature.1, signature.2, signature.3);
        // A point of the model and a per mille of the window are the coarsest steps on the way.
        let slack = (game.2 as f32 / rect.width()).max(game.2 as f32 / 1000.0) as i32 + 2;
        let (dx, dy) = ((area.0 - wanted.0).abs(), (area.1 - wanted.1).abs());
        let (dr, db) = ((area.0 + area.2 - wanted.0 - wanted.2).abs(), (area.1 + area.3 - wanted.1 - wanted.3).abs());        assert!(
            dx <= slack && dy <= slack && dr <= slack && db <= slack,
            "{game:?}: {area:?} against {wanted:?}, slack {slack}"
        );
    }
}

#[test]
fn both_models_stand_on_the_picture_of_the_game_and_without_one_on_the_grey_ground_with_a_line() {
    let _language = crate::language_lock();
    set_language(Choice::De);
    let paths = copy("scanner-ui-spielbild");
    let ctx = context();
    let scheme = versekit_core::theme::current();
    let mut state = ScannerState::with_files(&paths);
    let draw = |state: &mut ScannerState| {
        let mut shapes = Vec::new();
        for _ in 0..2 {
            // High enough for both cards one below the other.
            let tall = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(700.0, 1400.0))),
                ..input(vec![], 0.0)
            };
            let mut out = ctx.run_ui(tall, |ui| {
                model_cards(ui, scheme, state, ["Lage", "", "x"], ["Rahmen", "", "y"]);
            });
            out.textures_delta.clear();
            shapes = out.shapes;
        }
        shapes
    };
    fn textured(shape: &egui::epaint::Shape, count: &mut Vec<Rect>) {
        match shape {
            egui::epaint::Shape::Mesh(mesh) if mesh.texture_id != egui::TextureId::default() => {
                count.push(Rect::from_points(&mesh.vertices.iter().map(|v| v.pos).collect::<Vec<_>>()));
            }
            egui::epaint::Shape::Rect(image) if image.fill_texture_id() != egui::TextureId::default() => {
                count.push(image.rect);
            }
            egui::epaint::Shape::Vec(list) => list.iter().for_each(|shape| textured(shape, count)),
            _ => {}
        }
    }
    let large = |shapes: &[egui::epaint::ClippedShape]| {
        let mut found = Vec::new();
        shapes.iter().for_each(|clipped| textured(&clipped.shape, &mut found));
        found.into_iter().filter(|r| r.width() > 200.0).count()
    };
    let without = draw(&mut state);
    assert_eq!(large(&without), 0, "no picture yet");
    if versekit_core::screen_grab::supported() {
        assert!(texts(&without).contains(&t("s_sg_bild_keins")), "{:?}", texts(&without));
        assert!(texts(&without).contains(&t("s_sg_bild_holen")));
    }
    // A picture in the file: both models stand on it, the line goes, the button fetches anew.
    let picture = GamePicture::from_bgra(&game_screen(1720, 720, (800, 60, 120, 30)), 1720, 720).expect("a picture");
    assert!(picture.write(&paths.app_file(crate::game_picture::FILE)));
    let mut state = ScannerState::with_files(&paths);
    let with = draw(&mut state);
    assert_eq!(large(&with), 2, "both models on the picture");
    assert!(!texts(&with).contains(&t("s_sg_bild_keins")));
    if versekit_core::screen_grab::supported() {
        assert!(texts(&with).contains(&t("s_sg_bild_neu")));
    }
}

/// A made-up screen for the fetch: the game is in front at once (or never), its window a picture.
struct MadeScreen(Option<(i32, i32, i32, i32)>);

impl Source for MadeScreen {
    fn game(&mut self) -> Option<(i32, i32, i32, i32)> {
        self.0
    }
    fn grab(&mut self, area: (i32, i32, i32, i32)) -> Result<Vec<u8>, versekit_core::screen_grab::GrabError> {
        Ok(game_screen(area.2, area.3, (10, 10, 20, 20)))
    }
}

#[test]
fn the_fetch_keeps_the_picture_in_the_folder_intern_says_the_countdown_and_says_when_the_game_did_not_come() {
    let _language = crate::language_lock();
    set_language(Choice::De);
    let paths = copy("scanner-ui-holen");
    let ctx = context();
    let mut state = ScannerState::with_files(&paths);
    let wait = std::time::Duration::from_secs(5);
    let settle = |state: &mut ScannerState| {
        for _ in 0..400 {
            state.poll_fetch(&ctx);
            if state.fetch.is_none() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the fetch did not end");
    };
    state.start_fetch_from(&ctx, 10.0, Box::new(MadeScreen(Some((0, 0, 1600, 900)))), wait);
    assert_eq!(state.ground_line(13.2), Some(tf("s_sg_bild_warte", &[("0", Arg::from(57i64))])));
    settle(&mut state);
    assert_eq!(state.picture.as_ref().map(|p| p.game), Some((1600, 900)));
    let file = paths.app_file(crate::game_picture::FILE);
    assert!(file.replace('\\', "/").contains("/Intern/"), "{file}");
    assert_eq!(GamePicture::read(&file).map(|p| p.game), Some((1600, 900)));
    assert_eq!(state.ground_line(20.0), None, "a picture: nothing to say");
    // The game does not come: said, and the picture before stays.
    state.start_fetch_from(&ctx, 30.0, Box::new(MadeScreen(None)), std::time::Duration::from_millis(30));
    settle(&mut state);
    assert_eq!(state.ground_line(40.0), Some(t("s_sg_bild_kein_spiel")));
    assert_eq!(state.picture.as_ref().map(|p| p.game), Some((1600, 900)));
}

#[test]
fn the_box_shows_the_switch_and_only_while_it_is_on_the_teaching() {
    let _language = crate::language_lock();
    let paths = copy("scanner-ui-kasten");
    let mut state = ScannerState::with_files(&paths);
    let ctx = context();
    let scheme = versekit_core::theme::current();
    let mining = MiningData::default();
    let draw = |state: &mut ScannerState| {
        let mut shown = Vec::new();
        for _ in 0..2 {
            let mut out = ctx.run_ui(input(vec![], 0.0), |ui| show_box(ui, scheme, state, &mining));
            out.textures_delta.clear();
            shown = texts(&out.shapes);
        }
        shown
    };
    if versekit_core::screen_grab::supported() {
        let off = draw(&mut state);
        assert!(off.contains(&t("scan_wache_aus")), "{off:?}");
        assert!(!off.contains(&t("s_bg_scan_anlernen")));
        state.on = true;
        let on = draw(&mut state);
        assert!(on.contains(&t("s_bg_scan_anlernen")) && on.contains(&t("scan_warte")), "{on:?}");
        assert!(on.contains(&t("scan_naechstes")) && on.contains(&t("scan_ordner")));
    }
    let _ = pos2(0.0, 0.0);
}

#[test]
fn the_signal_to_show_the_frame_goes_a_moment_after_the_click_and_once() {
    let mut state = ScannerState::default();
    let ctx = context();
    let poll = |state: &mut ScannerState, time: f64| {
        ctx.run_ui(input(vec![], time), |ui| state.poll_show(ui.ctx())).textures_delta.clear();
    };
    poll(&mut state, 0.0);
    assert_eq!(state.show_due, None, "nothing asked");
    state.request_show(10.0);
    assert_eq!(state.show_due, Some(10.0 + SHOW_DELAY));
    poll(&mut state, 10.5);
    assert!(state.show_due.is_some(), "not yet: the frame just drawn is written first");
    poll(&mut state, 12.0);
    assert_eq!(state.show_due, None, "sent, and not again");
}

#[test]
fn a_picture_kept_for_an_unsure_number_comes_first_is_taught_away_and_can_be_discarded_or_set_aside() {
    let paths = copy("scanner-ui-gemerkt");
    let mut state = ScannerState::with_files(&paths);
    let (number, crop) = crop();
    let ctx = context();
    let follow = |state: &mut ScannerState, time: f64| {
        ctx.run_ui(input(vec![], time), |ui| state.follow(ui)).textures_delta.clear();
    };
    let folder = state.pending_folder().expect("folder");
    let (_, _, last) = state.files().expect("files");
    // The last picture of the overlay is there, and two kept ones: the newest kept one comes first.
    assert!(signature_watch::write_last_frame(&last, &Frame::new(crop.clone(), Some(number), 9)));
    let shade = |by: u8| Gray::new(crop.w, crop.h, crop.data.iter().map(|v| v.saturating_add(by)).collect());
    assert!(signature_watch::keep_pending(&folder, &Frame::new(shade(1), None, 1)));
    std::thread::sleep(std::time::Duration::from_millis(3));
    assert!(signature_watch::keep_pending(&folder, &Frame::new(shade(2), None, 2)));
    assert_eq!(state.pending_count(), 2);
    follow(&mut state, 0.0);
    assert_eq!(state.frame.as_ref().map(|f| f.number), Some(2), "the newest kept picture");
    assert!(state.pending.is_some());

    // Set aside: the other kept one follows, the file stays.
    state.next();
    follow(&mut state, 1.0);
    assert_eq!(state.frame.as_ref().map(|f| f.number), Some(1));
    assert_eq!(state.pending_count(), 2);
    // Discarded: the file is gone for good.
    state.discard();
    assert_eq!(state.pending_count(), 1);
    // Only the one set aside is left: then the last picture of the overlay comes.
    follow(&mut state, 2.0);
    assert_eq!(state.frame.as_ref().map(|f| f.number), Some(9));
    assert!(state.pending.is_none());

    // Taught: a kept picture is done with, and the window may close.
    state.next();
    state.skipped.clear();
    follow(&mut state, 3.0);
    assert_eq!(state.frame.as_ref().map(|f| f.number), Some(2));
    state.typed = number.to_string();
    assert!(!state.taught);
    state.learn(&MiningData::default());
    assert!(state.taught);
    assert_eq!(state.pending_count(), 0, "the kept picture is gone after it was taught");
}

// ------------------------------------------------------------ the two models

fn rects(shapes: &[egui::epaint::ClippedShape]) -> Vec<egui::epaint::RectShape> {
    fn collect(shape: &egui::epaint::Shape, out: &mut Vec<egui::epaint::RectShape>) {
        match shape {
            egui::epaint::Shape::Rect(rect) => out.push(rect.clone()),
            egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut out));
    out
}

fn dashes(shapes: &[egui::epaint::ClippedShape]) -> usize {
    fn count(shape: &egui::epaint::Shape) -> usize {
        match shape {
            egui::epaint::Shape::LineSegment { .. } => 1,
            egui::epaint::Shape::Vec(shapes) => shapes.iter().map(count).sum(),
            _ => 0,
        }
    }
    shapes.iter().map(|clipped| count(&clipped.shape)).sum()
}

/// Draws the model of the frame twice and gives its shapes.
fn frame_shapes(frame: Option<[i32; 4]>, remembered: Option<[i32; 4]>, width: f32) -> Vec<egui::epaint::ClippedShape> {
    let ctx = context();
    let scheme = versekit_core::theme::current();
    let mut frame = frame;
    let mut shapes = Vec::new();
    for _ in 0..2 {
        let mut out = ctx.run_ui(
            RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 600.0))), ..Default::default() },
            |ui| {
                frame_model(ui, scheme, &mut frame, remembered, &Ground::MODEL);
            },
        );
        out.textures_delta.clear();
        shapes = out.shapes;
    }
    shapes
}

#[test]
fn the_model_stands_on_a_ground_lighter_than_the_card_with_a_clear_edge() {
    let scheme = versekit_core::theme::current();
    let shapes = frame_shapes(None, None, 700.0);
    let ground = rects(&shapes).into_iter().max_by(|a, b| a.rect.area().total_cmp(&b.rect.area())).expect("ground");
    let card = color(scheme.surface);
    let light = |c: egui::Color32| u32::from(c.r()) + u32::from(c.g()) + u32::from(c.b());
    assert!(light(ground.fill) > light(card), "{:?} is not lighter than {card:?}", ground.fill);
    assert!(ground.stroke.width >= 1.5, "the edge is clear: {:?}", ground.stroke);
    assert!(ground.rect.width() > 500.0, "wider than before: {:?}", ground.rect);
    // On a wide page the picture of the search frame grows up to its own limit.
    let wide = frame_shapes(None, None, 1400.0);
    let ground = rects(&wide).into_iter().max_by(|a, b| a.rect.area().total_cmp(&b.rect.area())).expect("ground");
    assert!((ground.rect.width() - MODEL_WIDTH).abs() < 1.0, "as wide as allowed: {:?}", ground.rect);
}

#[test]
fn the_signature_box_in_the_model_is_a_dark_box_with_the_symbol_and_an_example_number() {
    let _language = crate::language_lock();
    set_language(Choice::De);
    let ctx = context();
    let scheme = versekit_core::theme::current();
    let mut frame = None;
    let mut draw = || {
        let mut out = ctx.run_ui(
            RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(700.0, 600.0))), ..Default::default() },
            |ui| {
                frame_model(ui, scheme, &mut frame, None, &Ground::MODEL);
            },
        );
        out.textures_delta.clear();
        out.shapes
    };
    let shapes = draw();
    let boxes: Vec<_> = rects(&shapes).into_iter().filter(|r| r.fill == HUD).collect();
    assert_eq!(boxes.len(), 1, "one box as in the game");
    assert!(texts(&shapes).contains(&signature_window::with_thousands(EXAMPLE)));
    // The symbol is an image of the icon set, drawn inside the box once the loader has made it.
    fn images(shape: &egui::epaint::Shape, inside: Rect, count: &mut usize) {
        match shape {
            egui::epaint::Shape::Rect(image)
                if image.fill_texture_id() != egui::TextureId::default() && inside.contains_rect(image.rect) =>
            {
                *count += 1;
            }
            egui::epaint::Shape::Vec(list) => list.iter().for_each(|shape| images(shape, inside, count)),
            _ => {}
        }
    }
    let mut symbols = 0;
    for _ in 0..80 {
        symbols = 0;
        draw().iter().for_each(|clipped| images(&clipped.shape, boxes[0].rect, &mut symbols));
        if symbols > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert!(symbols >= 1, "the location symbol stands in the box");
}

#[test]
fn a_drawn_frame_is_in_the_house_colour_with_a_handle_on_each_corner() {
    let accent = egui::Color32::from_rgb(0x9c, 0xe4, 0x30);
    let shapes = frame_shapes(Some([200, 100, 300, 230]), None, 700.0);
    let all = rects(&shapes);
    let handles: Vec<_> = all.iter().filter(|r| r.fill == accent && (r.rect.width() - HANDLE).abs() < 0.5).collect();
    assert_eq!(handles.len(), 4, "{all:?}");
    let area = all.iter().find(|r| r.stroke.color == accent && r.rect.width() > 40.0).expect("the frame");
    assert!(area.stroke.width >= 2.0);
    for corner in [area.rect.left_top(), area.rect.right_top(), area.rect.left_bottom(), area.rect.right_bottom()] {
        assert!(handles.iter().any(|h| h.rect.center().distance(corner) < 1.0), "no handle at {corner:?}");
    }
    // Without a frame there are no handles.
    let none = rects(&frame_shapes(None, None, 700.0));
    assert!(!none.iter().any(|r| (r.rect.width() - HANDLE).abs() < 0.5 && r.fill == accent));
}

#[test]
fn without_a_frame_the_place_kept_is_dashed_and_named_in_both_languages_and_a_frame_wins() {
    let _language = crate::language_lock();
    let place = Some([440, 470, 130, 60]);
    for choice in [Choice::De, Choice::En] {
        set_language(choice);
        let word = t("s_sg_modell_gemerkt");
        let kept = frame_shapes(None, place, 700.0);
        assert!(texts(&kept).contains(&word), "{choice:?}: {:?}", texts(&kept));
        assert!(dashes(&kept) >= 8, "dashed, not a closed line: {}", dashes(&kept));
        let nothing = frame_shapes(None, None, 700.0);
        assert!(!texts(&nothing).contains(&word));
        assert_eq!(dashes(&nothing), 0);
        let drawn = frame_shapes(Some([200, 100, 300, 230]), place, 700.0);
        assert!(!texts(&drawn).contains(&word), "a frame drawn has priority over the place kept");
        assert_eq!(dashes(&drawn), 0);
    }
    assert_ne!(
        {
            set_language(Choice::De);
            t("s_sg_modell_gemerkt")
        },
        {
            set_language(Choice::En);
            t("s_sg_modell_gemerkt")
        }
    );
    set_language(Choice::De);
}

/// Draws the two cards at `width` and gives the top left corner of each title.
fn card_titles(width: f32) -> (Pos2, Pos2, f32) {
    let ctx = context();
    let scheme = versekit_core::theme::current();
    let mut state = ScannerState::default();
    let (place, frame) = (t("s_sg_lage"), t("s_sg_rahmen"));
    let (mut found, mut over) = ((Pos2::ZERO, Pos2::ZERO), 0.0f32);
    for _ in 0..2 {
        let mut out = ctx.run_ui(
            RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 1400.0))), ..Default::default() },
            |ui| {
                model_cards(ui, scheme, &mut state, [&place, "", "x"], [&frame, "", "y"]);
                over = (ui.min_rect().width() - width).max(0.0);
            },
        );
        out.textures_delta.clear();
        let at = |word: &str| -> Pos2 {
            fn find(shape: &egui::epaint::Shape, word: &str, out: &mut Option<Pos2>) {
                match shape {
                    egui::epaint::Shape::Text(text) if text.galley.text() == word => *out = Some(text.pos),
                    egui::epaint::Shape::Vec(list) => list.iter().for_each(|shape| find(shape, word, out)),
                    _ => {}
                }
            }
            let mut out_pos = None;
            out.shapes.iter().for_each(|clipped| find(&clipped.shape, word, &mut out_pos));
            out_pos.unwrap_or_else(|| panic!("no title {word:?}"))
        };
        found = (at(&place), at(&frame));
    }
    (found.0, found.1, over)
}

#[test]
fn the_two_models_stand_one_below_the_other_on_every_page() {
    let _language = crate::language_lock();
    set_language(Choice::De);
    for width in [1400.0, 900.0, 400.0, 300.0] {
        let (upper, lower, over) = card_titles(width);
        assert!((upper.x - lower.x).abs() < 1.0, "one column at {width}: {upper:?} {lower:?}");
        assert!(lower.y > upper.y + 150.0, "the second below the first at {width}");
        assert!(over <= 0.5, "sticks out by {over} at {width}");
    }
}
