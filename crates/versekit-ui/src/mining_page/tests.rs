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

// The tests stand in a block of their own: the check of the fixed texts (tests/standards.rs) reads a file
// up to `mod tests {` and leaves the names and messages of the tests out.
mod tests {
    use super::super::*;
    use egui::{Rect, vec2};
    use versekit_core::language::{Choice, set_language};

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

    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx
    }

    /// One page drawn a few times inside the page frame of the window: the texts and how far it sticks out.
    fn draw(ctx: &egui::Context, width: f32, mut add: impl FnMut(&mut Ui)) -> (Vec<String>, f32) {
        let scheme = versekit_core::theme::current();
        let (mut shown, mut over) = (Vec::new(), 0.0);
        for _ in 0..3 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 3000.0))),
                    ..Default::default()
                },
                |ui| {
                    let page = crate::layout::page(ui, scheme, "bergbau-test", |ui| add(ui));
                    over = page.overflow;
                },
            );
            output.textures_delta.clear();
            shown = texts(&output.shapes);
        }
        (shown, over)
    }

    fn mining(ctx: &egui::Context, state: &mut MiningState, width: f32) -> (Vec<String>, f32) {
        let scheme = versekit_core::theme::current();
        draw(ctx, width, |ui| {
            show_page(ui, scheme, state);
        })
    }

    #[test]
    fn the_sample_shows_the_method_the_ores_and_their_places() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut state = MiningState::default();
        let (shown, over) = mining(&context(), &mut state, 900.0);
        assert_eq!(over, 0.0);
        assert!(shown.contains(&t("s_bg_lead")), "{shown:?}");
        assert!(shown.contains(&tf("s_rm_nimm", &[("0", Arg::from("Dinyx Solventation"))])), "{shown:?}");
        assert!(shown.contains(&t("s_rm_zeit_laeuft")), "Dinyx is very slow: {shown:?}");
        // Without a search the raw materials stand there, the places do not.
        for ore in ["Gold (Ore)", "Hadanite", "Quantanium (Raw)", "Titanium (Ore)"] {
            assert!(shown.contains(&ore.to_string()), "{ore}: {shown:?}");
        }
        assert!(!shown.contains(&"Aaron Halo".to_string()), "{shown:?}");
        assert!(shown.contains(&tf("s_bg_orte", &[("0", Arg::from(3)), ("1", Arg::from(4))])), "{shown:?}");
        assert!(shown.contains(&t("s_bg_mehr_info")), "{shown:?}");
    }

    #[test]
    fn a_raw_material_from_another_page_opens_with_its_places_and_refineries() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut state = MiningState::default();
        // The recipe says "Quantanium", the mining data "Quantanium (Raw)": the key finds it.
        state.jump_to("Quantanium");
        assert_eq!(state.search, "Quantanium (Raw)");
        let (shown, _) = mining(&context(), &mut state, 900.0);
        assert!(shown.contains(&"Daymar".to_string()) && shown.contains(&"Aaron Halo".to_string()), "{shown:?}");
        // The refinery list is shut at first; its header names the best station and its bonus.
        assert!(shown.contains(&refinery_title()), "{shown:?}");
        assert!(!shown.contains(&t("s_bg_raff_alle")), "{shown:?}");
        let (shown, _) = mining(&context(), &mut state, 900.0);
        assert!(!shown.contains(&tf("s_bg_raff_spanne", &[("0", Arg::from(12))])), "{shown:?}");
        // A name without a match stands in the search and opens nothing.
        state.jump_to("Daymar");
        assert_eq!((state.search.as_str(), state.open.as_deref()), ("Daymar", None));
    }

    /// The header of the shut refinery list under Quantanium: the best station of the sample data.
    fn refinery_title() -> String {
        tf(
            "s_bg_raff_klappe_beste",
            &[("0", Arg::from(t("s_bg_raff_kopf"))), ("1", Arg::from("ARC-L1")), ("2", Arg::from(9))],
        )
    }

    /// One frame of the page with these events: the texts with their place on the screen.
    fn frame_with(ctx: &egui::Context, state: &mut MiningState, events: Vec<egui::Event>) -> Vec<(String, Rect)> {
        let scheme = versekit_core::theme::current();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(900.0, 3000.0))),
                events,
                ..Default::default()
            },
            |ui| {
                crate::layout::page(ui, scheme, "bergbau-test", |ui| {
                    show_page(ui, scheme, state);
                });
            },
        );
        output.textures_delta.clear();
        fn collect(shape: &egui::epaint::Shape, out: &mut Vec<(String, Rect)>) {
            match shape {
                egui::epaint::Shape::Text(text) => {
                    out.push((text.galley.text().to_string(), text.galley.rect.translate(text.pos.to_vec2())))
                }
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut out));
        out
    }

    /// Frames without input until the opening or closing animation of the list is over.
    fn settle(ctx: &egui::Context, state: &mut MiningState) -> Vec<(String, Rect)> {
        let mut last = Vec::new();
        for _ in 0..12 {
            last = frame_with(ctx, state, Vec::new());
        }
        last
    }

    fn click_on(ctx: &egui::Context, state: &mut MiningState, at: egui::Pos2) -> Vec<(String, Rect)> {
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame_with(ctx, state, vec![egui::Event::PointerMoved(at)]);
        frame_with(ctx, state, vec![button(true)]);
        frame_with(ctx, state, vec![button(false)]);
        settle(ctx, state)
    }

    #[test]
    fn the_refinery_list_is_shut_at_first_and_opens_and_shuts_with_a_click_on_its_header() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let ctx = context();
        let mut state = MiningState::default();
        state.jump_to("Quantanium");
        let has = |shown: &[(String, Rect)], text: &str| shown.iter().any(|(t, _)| t == text);
        let list = tf("s_bg_raff_spanne", &[("0", Arg::from(12))]);
        let shown = settle(&ctx, &mut state);
        // Shut: the header names the best station, the list and the button for all refineries stay away.
        let header = refinery_title();
        assert!(has(&shown, &header), "{shown:?}");
        assert!(!has(&shown, &list) && !has(&shown, &t("s_bg_raff_alle")), "{shown:?}");
        let at = shown.iter().find(|(t, _)| *t == header).expect("header").1.center();
        // A click opens it.
        let shown = click_on(&ctx, &mut state, at);
        assert!(has(&shown, &list) && has(&shown, &t("s_bg_raff_alle")), "{shown:?}");
        // A second click shuts it again.
        let shown = click_on(&ctx, &mut state, at);
        assert!(!has(&shown, &list) && !has(&shown, &t("s_bg_raff_alle")), "{shown:?}");
        assert!(has(&shown, &header), "{shown:?}");
    }

    #[test]
    fn a_place_lists_its_ores_per_tool_and_the_only_one_says_so() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut state = MiningState {
            search: "daymar".to_string(),
            open: Some("ort:Daymar".to_string()),
            ..MiningState::default()
        };
        let (shown, _) = mining(&context(), &mut state, 900.0);
        assert!(shown.contains(&"Stanton · Moon".to_string()), "{shown:?}");
        // Two tools: each with its heading; by hand Hadanite is the only ore there.
        assert!(shown.contains(&t("s_bg_art_schiff")) && shown.contains(&t("s_bg_art_fps")), "{shown:?}");
        assert!(shown.contains(&t("s_bg_einziges")), "{shown:?}");
        // Only the vehicle: the ores of the ship stay away and so does Daymar.
        state.tool = 2;
        state.search.clear();
        let (shown, _) = mining(&context(), &mut state, 900.0);
        assert!(shown.contains(&"Gold (Ore)".to_string()), "{shown:?}");
        assert!(!shown.contains(&"Titanium (Ore)".to_string()), "{shown:?}");
        assert!(state.is_narrowed());
        state.reset();
        assert!(!state.is_narrowed() && state.open.is_none());
    }

    #[test]
    fn the_signature_names_the_ore_both_ways() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut state = MiningState { signature: "6.340".to_string(), ..MiningState::default() };
        let (shown, _) = mining(&context(), &mut state, 900.0);
        assert!(
            shown.contains(&tf("s_bg_sig_treffer", &[("0", Arg::from(2)), ("1", Arg::from("Quantanium (Raw)"))])),
            "{shown:?}"
        );
        assert!(shown.contains(&t("s_bg_sig_genau")), "{shown:?}");
        assert!(shown.contains(&t("s_bg_sig_anzahl_1")), "one match, in the singular: {shown:?}");
        state.signature = "8600".to_string();
        let (shown, _) = mining(&context(), &mut state, 900.0);
        assert!(shown.contains(&t("s_bg_sig_nichts")), "{shown:?}");
        // With a letter it is a name: its signatures per count of chunks.
        state.signature = "quant".to_string();
        let (shown, _) = mining(&context(), &mut state, 900.0);
        let series = format!(
            "{}   {}",
            tf("s_bg_sig_vielfach", &[("0", Arg::from(1)), ("1", Arg::from("3170"))]),
            tf("s_bg_sig_vielfach", &[("0", Arg::from(2)), ("1", Arg::from("6340"))])
        );
        assert!(shown.contains(&series), "{shown:?}");
    }

    #[test]
    fn the_axes_chosen_lead_to_their_method() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        // Speed first, then costs.
        let mut state = MiningState { first: 3, second: 2, ..MiningState::default() };
        let (shown, _) = mining(&context(), &mut state, 900.0);
        assert!(shown.contains(&tf("s_rm_nimm", &[("0", Arg::from("Cormack Method"))])), "{shown:?}");
        assert!(!shown.contains(&t("s_rm_zeit_laeuft")), "a fast method: {shown:?}");
    }

    #[test]
    fn without_mining_data_the_page_offers_to_fetch_them() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        // Other tests switch the scheme of the whole run; a scheme with filled buttons writes them in capitals.
        versekit_core::theme::set_thread_current("original");
        use versekit_core::paths::Host;
        use versekit_core::pypath::Flavor;
        let dir = std::env::temp_dir().join(format!("versekit-bergbau-{}-leer", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        let text = dir.to_string_lossy().into_owned();
        let paths = Paths::new(Host::new(Flavor::native(), &[(versekit_core::legacy::ENV_HOME, &text)], &text));
        let mut state = MiningState::with_files(&paths);
        let ctx = context();
        let (shown, _) = mining(&ctx, &mut state, 900.0);
        assert!(shown.contains(&t("s_bg_keine_daten")) && shown.contains(&t("s_bg_jetzt_holen")), "{shown:?}");
        // The methods need no data and stand there anyway.
        assert!(shown.contains(&t("s_rm_kopf")), "{shown:?}");
        // The tests never ask the net: the fetch ends at once and says so.
        state.start_fetch(&ctx);
        let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while state.fetching() && std::time::Instant::now() < end {
            state.poll_fetch();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(state.said(), Some((t("m_h_kein_netz").as_str(), true)));
        // A failure says how to try again.
        let (shown, _) = mining(&ctx, &mut state, 900.0);
        assert!(shown.contains(&t("s_bg_nochmal")), "{shown:?}");
        let (shown, _) = mining(&ctx, &mut state, 900.0);
        assert!(shown.contains(&t("m_h_kein_netz")), "{shown:?}");
        // The page of the refineries says the same without data.
        let scheme = versekit_core::theme::current();
        let (shown, _) = draw(&ctx, 900.0, |ui| crate::refineries_page::show_page(ui, scheme, &mut state));
        assert!(shown.contains(&t("s_rf_keine")), "{shown:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_refinery_table_bundles_per_system_and_names_every_station() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut state = MiningState::default();
        let scheme = versekit_core::theme::current();
        let (shown, over) = draw(&context(), 380.0, |ui| crate::refineries_page::show_page(ui, scheme, &mut state));
        assert_eq!(over, 0.0, "the table scrolls in its own area: {shown:?}");
        for word in ["Pyro", "Stanton", "Check\nmate", "ARC-L1", "CRU-L1", "+9", "·", "-4"] {
            assert!(shown.iter().any(|s| s == word), "{word:?}: {shown:?}");
        }
        assert!(shown.contains(&t("s_rf_quelle")), "{shown:?}");
        // No legend below the table: the stations of a column stand in nothing until its head is hovered.
        let station = "ARC-L1 Wide Forest Station\nPyro Gateway (Stanton)".to_string();
        assert!(!shown.iter().any(|s| s.starts_with("Welche Station")), "{shown:?}");
        assert!(!shown.contains(&station), "{shown:?}");
    }

    /// The page drawn on a wide screen for a few seconds with the pointer resting at `at` (or nowhere): the
    /// shapes of the last frame, tooltips included.
    fn rest_pointer(state: &mut MiningState, at: Option<egui::Pos2>) -> Vec<egui::epaint::ClippedShape> {
        let ctx = context();
        let scheme = versekit_core::theme::current();
        let mut shapes = Vec::new();
        for frame in 0..5 {
            let events = match at {
                Some(at) if frame == 0 => vec![egui::Event::PointerMoved(at)],
                _ => Vec::new(),
            };
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1400.0, 3000.0))),
                    time: Some(f64::from(frame)),
                    events,
                    ..Default::default()
                },
                |ui| {
                    crate::layout::page(ui, scheme, "raffinerie-test", |ui| {
                        crate::refineries_page::show_page(ui, scheme, state)
                    });
                },
            );
            output.textures_delta.clear();
            shapes = output.shapes;
        }
        shapes
    }

    /// Resting the pointer on the head of a column lists every station of that column; the head of a single
    /// station names that one, and the pointer elsewhere shows nothing.
    #[test]
    fn the_head_of_a_refinery_column_lists_all_its_stations_on_hover() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut state = MiningState::default();
        let scheme = versekit_core::theme::current();
        let plain = rest_pointer(&mut state, None);
        let (_, placed) = stripes_and_texts(&plain, color(scheme.bar));
        let head_at = |name: &str| placed.iter().find(|(text, _)| text == name).expect("head").1.center();
        let bundle = "ARC-L1 Wide Forest Station\nPyro Gateway (Stanton)".to_string();
        assert!(!texts(&plain).contains(&bundle));
        let hovered = texts(&rest_pointer(&mut state, Some(head_at("ARC-L1"))));
        assert!(hovered.contains(&bundle), "{hovered:?}");
        let hovered = texts(&rest_pointer(&mut state, Some(head_at("Check\nmate"))));
        let alone = hovered.iter().filter(|s| *s == "Checkmate").count();
        assert!(alone == 1 && !hovered.contains(&bundle), "{hovered:?}");
    }

    /// The stripes of a frame filled with `fill`, and every text with where it stands.
    fn stripes_and_texts(
        shapes: &[egui::epaint::ClippedShape],
        fill: egui::Color32,
    ) -> (Vec<Rect>, Vec<(String, Rect)>) {
        fn collect(
            shape: &egui::epaint::Shape,
            fill: egui::Color32,
            stripes: &mut Vec<Rect>,
            texts: &mut Vec<(String, Rect)>,
        ) {
            match shape {
                egui::epaint::Shape::Rect(rect) if rect.fill == fill => stripes.push(rect.rect),
                egui::epaint::Shape::Text(text) => {
                    texts.push((text.galley.text().to_string(), text.galley.rect.translate(text.pos.to_vec2())))
                }
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, fill, stripes, texts)),
                _ => {}
            }
        }
        let (mut stripes, mut texts) = (Vec::new(), Vec::new());
        shapes.iter().for_each(|clipped| collect(&clipped.shape, fill, &mut stripes, &mut texts));
        (stripes, texts)
    }

    /// The materials stand still beside the values that scroll: each stripe on the left lies on the same height
    /// as its stripe on the right, down to the last row, and each system stands centred over its columns.
    #[test]
    fn the_refinery_rows_stay_level_and_the_systems_stand_over_their_columns() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let ctx = context();
        let scheme = versekit_core::theme::current();
        let mut state = MiningState::default();
        let fill = color(scheme.bar);
        let mut shapes = Vec::new();
        for _ in 0..3 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1400.0, 3000.0))),
                    ..Default::default()
                },
                |ui| {
                    crate::layout::page(ui, scheme, "raffinerie-test", |ui| {
                        crate::refineries_page::show_page(ui, scheme, &mut state)
                    });
                },
            );
            output.textures_delta.clear();
            shapes = output.shapes;
        }
        let (stripes, texts) = stripes_and_texts(&shapes, fill);
        let (columns, rows) = refining::matrix(state.data());
        let table = rows.len() / 2;
        let material = |name: &str| texts.iter().find(|(text, _)| text == name).expect("material").1;
        let (first, last) = (material(&rows[0].material), material(&rows[rows.len() - 1].material));
        // The stripes of the table only; the left ones begin where the materials begin.
        let (mut left, mut right): (Vec<Rect>, Vec<Rect>) = stripes
            .into_iter()
            .filter(|r| r.min.y > first.min.y - 30.0 && r.max.y < last.max.y + 30.0)
            .partition(|r| r.min.x <= first.min.x + 1.0);
        left.sort_by(|a, b| a.min.y.total_cmp(&b.min.y));
        right.sort_by(|a, b| a.min.y.total_cmp(&b.min.y));
        left.truncate(table);
        right.truncate(table);
        assert_eq!((left.len(), right.len()), (table, table), "a stripe on each side: {left:?} {right:?}");
        for (k, (l, r)) in left.iter().zip(&right).enumerate() {
            assert!(
                (l.min.y - r.min.y).abs() < 0.5 && (l.max.y - r.max.y).abs() < 0.5,
                "stripe {k}: materials {:.1}..{:.1}, values {:.1}..{:.1}",
                l.min.y,
                l.max.y,
                r.min.y,
                r.max.y
            );
        }
        // The heads stand at the right of their columns: the right edges give the columns.
        let heads: Vec<Rect> = columns
            .iter()
            .map(|c| {
                let head = refining::head(&crate::refineries_page::short_name(c));
                texts.iter().find(|(text, _)| *text == head).map(|(_, rect)| *rect).expect("head")
            })
            .collect();
        let width = heads[1].max.x - heads[0].max.x;
        let mut start = 0;
        for (system, count) in refining::groups(&columns) {
            let name = texts.iter().find(|(text, _)| *text == system).map(|(_, rect)| *rect).expect("system");
            let span = (heads[start].max.x - width, heads[start + count - 1].max.x);
            let middle = (span.0 + span.1) / 2.0;
            assert!((name.center().x - middle).abs() < 1.0, "{system}: at {:.1}, columns {span:?}", name.center().x);
            start += count;
        }
    }

    /// The table of the refineries in a window of this size: the stripes of its rows and every text.
    fn refinery_table_in(width: f32, height: f32) -> (Vec<Rect>, Vec<(String, Rect)>) {
        let ctx = context();
        let scheme = versekit_core::theme::current();
        let mut state = MiningState::default();
        let mut shapes = Vec::new();
        for _ in 0..3 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(width, height))),
                    ..Default::default()
                },
                |ui| {
                    crate::layout::page(ui, scheme, "raffinerie-test", |ui| {
                        crate::refineries_page::show_page(ui, scheme, &mut state)
                    });
                },
            );
            output.textures_delta.clear();
            shapes = output.shapes;
        }
        stripes_and_texts(&shapes, color(scheme.bar))
    }

    /// The table grows with the window: its columns share the width, its rows the height down to the bottom.
    #[test]
    fn the_refinery_table_grows_with_the_window() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let (columns, _) = refining::matrix(MiningState::default().data());
        let last = refining::head(&crate::refineries_page::short_name(&columns[columns.len() - 1]));
        let right_edge = |texts: &[(String, Rect)]| texts.iter().find(|(t, _)| *t == last).expect("head").1.max.x;
        let stripe = |stripes: &[Rect]| stripes.iter().map(Rect::height).fold(0.0, f32::max);
        // The sample has three materials: a window this low leaves them one line each, a tall one the most.
        let (small_stripes, small) = refinery_table_in(1000.0, 160.0);
        let (big_stripes, big) = refinery_table_in(1800.0, 1400.0);
        assert!(right_edge(&big) > 1800.0 - 120.0, "the columns reach the right side: {}", right_edge(&big));
        assert!(right_edge(&big) > right_edge(&small) + 400.0, "{} {}", right_edge(&big), right_edge(&small));
        assert!(
            stripe(&big_stripes) > stripe(&small_stripes) + 4.0,
            "{} {}",
            stripe(&big_stripes),
            stripe(&small_stripes)
        );
    }

    #[test]
    fn an_ore_that_is_the_only_one_of_its_tool_at_a_place_says_so() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        // Hadanite is the only ore for the hand at Daymar: no "100 %", "the only one here".
        let mut state = MiningState::default();
        state.jump_to("Hadanite");
        let (shown, _) = mining(&context(), &mut state, 900.0);
        assert!(shown.contains(&t("s_bg_einziges")), "{shown:?}");
        assert!(!shown.iter().any(|s| s.contains("100 %")), "{shown:?}");
        assert!(shown.contains(&t("s_bg_nur_orte_1")), "one place, in the singular: {shown:?}");
    }

    /// The texts of a frame that were cut with "…" because they did not fit.
    fn cut_texts(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
        fn collect(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
            match shape {
                egui::epaint::Shape::Text(text) if text.galley.elided => out.push(text.galley.text().to_string()),
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut out));
        out
    }

    /// One of the four pages, filled as far as it goes: an ore open with its refineries and a signature; the
    /// table; a wreck with its parts; a part taken apart.
    fn filled_page(which: usize) -> Box<dyn FnMut(&mut Ui)> {
        let scheme = versekit_core::theme::current();
        match which {
            0 => {
                let mut state = MiningState { signature: "~6400".to_string(), ..MiningState::default() };
                state.jump_to("Quantanium");
                Box::new(move |ui| {
                    show_page(ui, scheme, &mut state);
                })
            }
            1 => {
                let mut state = MiningState::default();
                Box::new(move |ui| crate::refineries_page::show_page(ui, scheme, &mut state))
            }
            2 => {
                let mut state = crate::salvage_page::SalvageState::default();
                let mut crafting = crate::crafting_page::CraftingState::default();
                state.typed = "Cutlass Black".into();
                state.look_up(&egui::Context::default());
                Box::new(move |ui| crate::salvage_page::show_page(ui, scheme, &mut state, &mut crafting))
            }
            _ => {
                let mut crafting = crate::crafting_page::CraftingState::default();
                let name = crafting.parts().map(|(_, book, _)| book.items[0].basis.clone()).unwrap_or_default();
                let mut state = crate::salvage_page::SalvageState::default();
                state.open_part(&name);
                Box::new(move |ui| crate::salvage_page::show_page(ui, scheme, &mut state, &mut crafting))
            }
        }
    }

    #[test]
    fn the_four_pages_fit_and_cut_nothing_at_every_size_and_width_in_both_languages() {
        let _language = crate::language_lock();
        let mut findings = Vec::new();
        for lang in [Choice::De, Choice::En] {
            set_language(lang);
            for points in versekit_core::font_size::POINTS_RANGE.0..=versekit_core::font_size::POINTS_RANGE.1 {
                for width in [380.0, 520.0, 900.0] {
                    for which in 0..4 {
                        let ctx = context();
                        crate::apply_size(&ctx, points);
                        let mut page = filled_page(which);
                        let scheme = versekit_core::theme::current();
                        let (mut over, mut cut) = (0.0, Vec::new());
                        for _ in 0..3 {
                            let mut output = ctx.run_ui(
                                egui::RawInput {
                                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 3000.0))),
                                    ..Default::default()
                                },
                                |ui| over = crate::layout::page(ui, scheme, "abbau-groesse", |ui| page(ui)).overflow,
                            );
                            output.textures_delta.clear();
                            cut = cut_texts(&output.shapes);
                        }
                        let at = format!("Seite {which} {lang:?} Größe {points} Breite {width}");
                        if over > 0.5 {
                            findings.push(format!("{at}: {over:.0} zu breit"));
                        }
                        if !cut.is_empty() {
                            findings.push(format!("{at}: abgeschnitten {cut:?}"));
                        }
                    }
                }
            }
        }
        set_language(Choice::De);
        assert!(findings.is_empty(), "{}", findings.join("\n"));
    }
}
