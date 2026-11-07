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

//! The layout check of all pages of the main window, without a window (egui_kittest without a renderer).
//!
//! Every page is opened with the sample data in the smallest window the program allows and in a usual one,
//! in every size of the text, in German and English, and, where a page has a list or a detail, with an entry
//! chosen and the detail open. Three things are measured in every frame:
//!
//! - nothing is cut off: no line of text and no control reaches over the edge of its card or of the window, and
//!   no two lines of text lie over each other;
//! - the cards of a row begin on one line (each is as tall as its content);
//! - the height of a card stays the same over thirty frames (nothing grows by itself).
//!
//! Beyond the sample data come three states: the first entry of every list that opens a detail chosen, the
//! pages of the updates with releases remembered (in a throwaway data folder), and the maps of the Contested
//! Zones dragged far in every direction, after which the map still fills its frame as before.
//!
//! The finds of each part of the run are written to the temporary folder (`versekit-layout-guard`) in full.
//! Every rule has a counter-check below that builds the fault and has to see it.

use std::path::Path;
use std::sync::Arc;

use egui::accesskit::Role;
use egui::epaint::Shape;
use egui::{Pos2, Rect, Vec2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::NodeT;
use versekit_core::font_size;
use versekit_core::language::{Choice, set_thread_language};
use versekit_core::paths::{Host, Paths};
use versekit_core::pypath::Flavor;
use versekit_core::theme;
use versekit_core::updater::{self, Asset, Cache, Release};
use versekit_ui::sample::SamplePage;
use versekit_ui::widgets::drawn_cards;

/// How far an edge may differ before it counts (rounding of the pixels).
const TOLERANCE: f32 = 0.75;
/// Frames a page gets to settle before it is measured.
const SETTLE_FRAMES: usize = 6;
/// Frames over which the height of a card has to stay the same.
const WATCHED_FRAMES: usize = 30;
/// How many parts the pages are split into: each part is a test of its own and runs in a thread of its own.
const PARTS: usize = 8;

/// The usual size of a window, in interface points.
const USUAL_WINDOW: Vec2 = vec2(1280.0, 820.0);

fn smallest_window() -> Vec2 {
    vec2(font_size::BASE_MIN_SIZE.0, font_size::BASE_MIN_SIZE.1)
}

/// A line of text as it stands in a frame.
struct Line {
    text: String,
    rect: Rect,
    /// The area it is cut to when drawn.
    clip: Rect,
    /// Whether the galley had to shorten it ("…").
    shortened: bool,
    /// Its place in the order of drawing: what is drawn later lies on top.
    order: usize,
}

/// A control or label as the accessibility tree knows it.
struct Node {
    /// The same from frame to frame (made from the id of the widget).
    id: u128,
    role: Role,
    name: String,
    rect: Rect,
}

/// A picture as it stands in a frame: its whole place and the area it is cut to.
struct Picture {
    rect: Rect,
    clip: Rect,
}

impl Picture {
    /// The part of it that can be seen.
    fn seen(&self) -> f32 {
        let seen = self.rect.intersect(self.clip);
        if seen.is_positive() { seen.area() } else { 0.0 }
    }
}

/// What one frame of a page put on the screen.
struct Frame {
    /// Every card of the page with the part of it that can be seen.
    cards: Vec<(Rect, Rect)>,
    /// The cards that float above the page (a notice, a dialog), each with the place of its background in the
    /// order of drawing: the lines drawn after it in its place are its own.
    floating: Vec<(Rect, usize)>,
    lines: Vec<Line>,
    nodes: Vec<Node>,
    pictures: Vec<Picture>,
    window: Rect,
    /// How far the page itself says it reaches over its right edge.
    overflow: f32,
}

struct Probe {
    page: SamplePage,
    overflow: f32,
}

fn harness(key: &str) -> Harness<'static, Probe> {
    let mut page = SamplePage::preview();
    page.go_to(key);
    harness_of(page)
}

fn harness_of(page: SamplePage) -> Harness<'static, Probe> {
    drawn_cards::record(true);
    Harness::builder()
        .with_size(smallest_window())
        .with_context_setup(versekit_ui::install)
        .with_accessibility_check(false)
        .allow_missing_glyphs()
        .build_ui_state(|ui, probe: &mut Probe| probe.overflow = probe.page.show(ui), Probe { page, overflow: 0.0 })
}

/// Sets the size of the text and the window and lets the page settle.
fn settle(harness: &mut Harness<'_, Probe>, points: i64, window: Vec2) {
    harness.state_mut().page.size = points;
    harness.ctx.options_mut(|options| options.zoom_factor = font_size::zoom_factor(points));
    harness.set_size(window);
    for _ in 0..SETTLE_FRAMES {
        harness.set_size(window);
        harness.step();
    }
    drawn_cards::take();
}

fn collect_lines(shape: &Shape, (clip, order): (Rect, usize), out: &mut Vec<Line>, pictures: &mut Vec<Picture>) {
    match shape {
        Shape::Text(text) => {
            for row in &text.galley.rows {
                let rect = row.rect_without_leading_space().translate(text.pos.to_vec2());
                let line = row.row.text();
                if !line.trim().is_empty() {
                    out.push(Line { text: line.to_string(), rect, clip, shortened: text.galley.elided, order });
                }
            }
        }
        // A picture is a rectangle filled with a texture, or a mesh with one (the font has the default texture).
        Shape::Rect(rect) if rect.brush.is_some() => pictures.push(Picture { rect: rect.rect, clip }),
        Shape::Mesh(mesh) if mesh.texture_id != egui::TextureId::default() => {
            pictures.push(Picture { rect: mesh.calc_bounds(), clip });
        }
        Shape::Vec(shapes) => shapes.iter().for_each(|inner| collect_lines(inner, (clip, order), out, pictures)),
        _ => {}
    }
}

/// Whether `shape` paints a rectangle in the place of `card` (a frame paints its background a stroke inside).
fn paints_card(shape: &Shape, card: Rect) -> bool {
    match shape {
        Shape::Rect(rect) => (rect.rect.min - card.min).length() <= 3.0 && (rect.rect.max - card.max).length() <= 3.0,
        Shape::Vec(shapes) => shapes.iter().any(|inner| paints_card(inner, card)),
        _ => false,
    }
}

/// Roles whose bounds are the visible part of a control or a label (containers, scroll areas and the
/// window have bounds of their own that mean nothing for the check).
fn is_control(role: Role) -> bool {
    matches!(
        role,
        Role::Button
            | Role::CheckBox
            | Role::RadioButton
            | Role::Switch
            | Role::TextInput
            | Role::MultilineTextInput
            | Role::ComboBox
            | Role::Slider
            | Role::SpinButton
            | Role::Link
            | Role::Tab
            | Role::Label
            | Role::ListBoxOption
            | Role::MenuItem
    )
}

fn frame_of(harness: &mut Harness<'_, Probe>, window: Vec2) -> Frame {
    harness.set_size(window);
    harness.step();
    let (cards, floating) = drawn_cards::take_split();
    let cards: Vec<(Rect, Rect)> = cards.into_iter().map(|(card, clip)| (card, card.intersect(clip))).collect();
    let (mut lines, mut pictures) = (Vec::new(), Vec::new());
    let shapes = &harness.output().shapes;
    for (order, clipped) in shapes.iter().enumerate() {
        collect_lines(&clipped.shape, (clipped.clip_rect, order), &mut lines, &mut pictures);
    }
    // A card that floats is drawn after the page: its background is the last rectangle in its place.
    let floating: Vec<(Rect, usize)> = floating
        .into_iter()
        .map(|card| (card, shapes.iter().rposition(|clipped| paints_card(&clipped.shape, card)).unwrap_or(0)))
        .collect();
    let mut nodes = Vec::new();
    let mut stack = vec![harness.kittest_state().root()];
    while let Some(node) = stack.pop() {
        stack.extend(node.children());
        let role = node.role();
        if let (true, Some(bounds)) = (is_control(role), node.raw_bounds()) {
            let rect = Rect::from_min_max(
                egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
            );
            let name = node.label().or_else(|| node.value()).unwrap_or_default();
            nodes.push(Node { id: u128::from(node.id()), role, name, rect });
        }
    }
    Frame {
        cards,
        floating,
        lines,
        nodes,
        pictures,
        window: Rect::from_min_size(egui::Pos2::ZERO, window),
        overflow: harness.state().overflow,
    }
}

/// The places of the cards, whole.
fn places(cards: &[(Rect, Rect)]) -> Vec<Rect> {
    cards.iter().map(|(card, _)| *card).collect()
}

/// The innermost card (smallest) whose visible part holds the middle of `rect`.
fn owner(cards: &[(Rect, Rect)], rect: Rect) -> Option<Rect> {
    cards
        .iter()
        .filter(|(_, seen)| seen.contains(rect.center()))
        .map(|(card, _)| *card)
        .min_by(|a, b| a.area().total_cmp(&b.area()))
}

/// Anything of the frame that reaches over the edge of its card or of the window.
fn cut_off(frame: &Frame) -> Vec<String> {
    let mut found = Vec::new();
    if frame.overflow > TOLERANCE {
        found.push(format!("die Seite ragt {:.0} Punkte über den rechten Rand", frame.overflow));
    }
    let wide = |rect: Rect, bound: Rect| rect.min.x < bound.min.x - TOLERANCE || rect.max.x > bound.max.x + TOLERANCE;
    for line in &frame.lines {
        let seen_high = line.rect.max.y > line.clip.min.y && line.rect.min.y < line.clip.max.y;
        if !seen_high {
            continue;
        }
        if wide(line.rect, line.clip) {
            found.push(format!("Text über dem Rand des Bereichs: {:?}", line.text));
        } else if let Some(card) = owner(&frame.cards, line.rect)
            && wide(line.rect, card)
        {
            found.push(format!("Text über dem Rand der Karte: {:?}", line.text));
        }
        if line.shortened {
            found.push(format!("Text gekürzt: {:?}", line.text));
        }
    }
    found.extend(overlapping(frame));
    for node in &frame.nodes {
        if node.rect.width() < 1.0 || node.rect.height() < 1.0 {
            continue;
        }
        let describe = || format!("{:?} {:?}", node.role, node.name);
        if wide(node.rect, frame.window) {
            found.push(format!("Bedienelement über dem Rand des Fensters: {}", describe()));
        } else if let Some(card) = owner(&frame.cards, node.rect)
            && wide(node.rect, card)
        {
            found.push(format!("Bedienelement über dem Rand der Karte: {} {:?} in {card:?}", describe(), node.rect));
        }
    }
    found
}

/// Lines of text that lie over each other in the same card, or both outside every card. The lines of a card that
/// floats over the page belong to that card: it covers what lies under it. Only the parts that can be seen count.
fn overlapping(frame: &Frame) -> Vec<String> {
    let floating = |line: &Line| {
        frame
            .floating
            .iter()
            .filter(|(card, from)| line.order >= *from && card.contains(line.rect.center()))
            .map(|(card, _)| *card)
            .min_by(|a, b| a.area().total_cmp(&b.area()))
    };
    let seen: Vec<(&Line, Rect, Option<Rect>)> = frame
        .lines
        .iter()
        .filter_map(|line| {
            let seen = line.rect.intersect(line.clip);
            let card = floating(line).or_else(|| owner(&frame.cards, line.rect));
            seen.is_positive().then_some((line, seen, card))
        })
        .collect();
    let mut found = Vec::new();
    for (i, (a, a_seen, a_card)) in seen.iter().enumerate() {
        for (b, b_seen, b_card) in &seen[i + 1..] {
            if a_card != b_card {
                continue;
            }
            let both = a_seen.intersect(*b_seen);
            if both.is_positive() && both.area() > 0.3 * a_seen.area().min(b_seen.area()) {
                found.push(format!("Texte überdecken sich: {:?} und {:?}", a.text, b.text));
            }
        }
    }
    found
}

/// Cards that stand side by side in a row differ in their height or sit crooked.
fn crooked(cards: &[Rect]) -> Vec<String> {
    let mut found = Vec::new();
    let inside = |inner: Rect, outer: Rect| outer.expand(TOLERANCE).contains_rect(inner);
    for (i, a) in cards.iter().enumerate() {
        for b in &cards[i + 1..] {
            // A card inside another one is no neighbour of it.
            if inside(*a, *b) || inside(*b, *a) {
                continue;
            }
            let (left, right) = if a.min.x <= b.min.x { (*a, *b) } else { (*b, *a) };
            // Side by side: the left one ends before the right one starts, close to it.
            let gap = right.min.x - left.max.x;
            if !(-TOLERANCE..=24.0).contains(&gap) {
                continue;
            }
            let overlap = left.max.y.min(right.max.y) - left.min.y.max(right.min.y);
            if overlap < 0.4 * left.height().min(right.height()) {
                continue;
            }
            // A card that has another one right above or under it is in a column, not in a row.
            let in_a_column = |card: Rect| {
                cards.iter().any(|other| {
                    (other.min.x - card.min.x).abs() <= TOLERANCE
                        && ((other.min.y >= card.max.y - TOLERANCE && other.min.y - card.max.y <= 24.0)
                            || (card.min.y >= other.max.y - TOLERANCE && card.min.y - other.max.y <= 24.0))
                })
            };
            if in_a_column(left) || in_a_column(right) {
                continue;
            }
            // Each card is as tall as its content (no card is stretched to the height of its row), but cards side
            // by side begin on one line.
            if (left.min.y - right.min.y).abs() > TOLERANCE {
                found.push(format!("Karten nebeneinander beginnen verschieden hoch: {left:?} und {right:?}"));
            }
        }
    }
    found
}

/// A card whose height differs from what it had in the first frame of the watch.
fn grown(first: &[Rect], later: &[Rect], at: usize) -> Vec<String> {
    if first.len() != later.len() {
        return vec![format!("Bild {at}: {} Karten statt {}", later.len(), first.len())];
    }
    first
        .iter()
        .zip(later)
        .filter(|(was, now)| (was.height() - now.height()).abs() > TOLERANCE)
        .map(|(was, now)| {
            format!("Bild {at}: Karte bei {was:?} wächst auf {:.1} statt {:.1}", now.height(), was.height())
        })
        .collect()
}

/// A control or label whose height differs from what it had in the first frame of the watch (a box that is no
/// card, a button). Only those that stand in both frames are compared, by the id egui gives them.
fn grown_nodes(first: &[Node], later: &[Node], at: usize) -> Vec<String> {
    let mut found = Vec::new();
    for now in later {
        let was = first.iter().find(|was| was.id == now.id && was.role == now.role);
        if let Some(was) = was
            && (was.rect.height() - now.rect.height()).abs() > TOLERANCE
        {
            found.push(format!(
                "Bild {at}: {:?} {:?} wächst auf {:.1} statt {:.1}",
                now.role,
                now.name,
                now.rect.height(),
                was.rect.height()
            ));
        }
    }
    found
}

/// Where one measurement was made.
struct Place {
    key: &'static str,
    points: i64,
    window: Vec2,
    english: bool,
    scheme: &'static str,
    what: String,
}

impl Place {
    fn text(&self) -> String {
        format!(
            "{} | {} | Stufe {} | {:.0}x{:.0} | {} | {}",
            if self.key.is_empty() { "Übersicht" } else { self.key },
            self.what,
            self.points,
            self.window.x,
            self.window.y,
            if self.english { "EN" } else { "DE" },
            self.scheme
        )
    }
}

/// Measures the open page in the state it is in: cut off and crooked in the last frame, and the height of
/// every card over [`WATCHED_FRAMES`] frames.
fn measure(harness: &mut Harness<'_, Probe>, window: Vec2, watch: bool, place: &Place, findings: &mut Vec<String>) {
    let first = frame_of(harness, window);
    let mut here: Vec<String> = cut_off(&first);
    // The tiles of the overview are of the sizes the user chose: they stand next to each other unevenly.
    if !place.key.is_empty() {
        here.extend(crooked(&places(&first.cards)));
    }
    if watch {
        for at in 1..=WATCHED_FRAMES {
            let later = frame_of(harness, window);
            here.extend(grown(&places(&first.cards), &places(&later.cards), at));
            here.extend(grown_nodes(&first.nodes, &later.nodes, at));
            if at == WATCHED_FRAMES {
                here.extend(cut_off(&later));
            }
        }
    }
    here.sort();
    here.dedup();
    findings.extend(here.into_iter().map(|find| format!("{} | {find}", place.text())));
}

/// Every key of the pages of the window, the overview first.
fn page_keys() -> Vec<&'static str> {
    SamplePage::page_keys()
}

/// Pages that no click is made on, whatever the click would hit: they start programs, open folders and
/// ask for files, and the check must never reach out of its own process.
const NO_CLICKS: [&str; 4] = ["hf_ordner", "hf_startprogramme", "hf_sichern", "hf_diagnose"];
/// Sizes of the text of the states with a click (the usual one and the biggest).
const CLICK_POINTS: [i64; 2] = [1, 7];
/// How many labels of a page are clicked, one after the other, each in a page of its own.
const LABEL_CLICKS: usize = 3;

/// The places of the labels in the page area that no control covers: a click there lands on a row or a card
/// that has no control of its own (a list row, a head that unfolds), spread over the page from top to bottom.
fn click_places(frame: &Frame) -> Vec<(egui::Pos2, String)> {
    // The side bar is left of this, the head above it and the status bar below it.
    let area = Rect::from_min_max(egui::pos2(240.0, 150.0), egui::pos2(frame.window.max.x, frame.window.max.y - 40.0));
    let covered = |rect: Rect| {
        frame.nodes.iter().any(|node| !matches!(node.role, Role::Label) && node.rect.expand(2.0).intersects(rect))
    };
    let mut labels: Vec<&Node> = frame
        .nodes
        .iter()
        .filter(|node| matches!(node.role, Role::Label) && area.contains_rect(node.rect) && !covered(node.rect))
        .collect();
    labels.sort_by(|a, b| a.rect.min.y.total_cmp(&b.rect.min.y).then(a.rect.min.x.total_cmp(&b.rect.min.x)));
    labels.dedup_by(|a, b| (a.rect.min.y - b.rect.min.y).abs() < 6.0);
    let step = (labels.len() as f32 / LABEL_CLICKS as f32).max(1.0);
    let mut places: Vec<(egui::Pos2, String)> = (0..LABEL_CLICKS)
        .filter_map(|n| labels.get((n as f32 * step) as usize))
        .map(|node| (node.rect.center(), node.name.clone()))
        .collect();
    // Rows of a list are drawn without a label of their own: a click a fifth in from the left, a quarter,
    // half and three quarters of the way down the page, where no control stands.
    for share in [0.25, 0.5, 0.75] {
        let at = egui::pos2(area.min.x + 0.2 * area.width(), area.min.y + share * area.height());
        if !covered(Rect::from_center_size(at, vec2(4.0, 4.0))) {
            places.push((at, format!("Stelle {:.0} %", share * 100.0)));
        }
    }
    places
}

fn click(harness: &mut Harness<'_, Probe>, at: egui::Pos2) {
    let button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    harness.event(egui::Event::PointerMoved(at));
    harness.step();
    harness.event(button(true));
    harness.step();
    harness.event(button(false));
    harness.step();
    // The pointer leaves, so no hint stands over the page when it is measured.
    harness.event(egui::Event::PointerGone);
    harness.step();
}

/// The page with a click made on it: an entry chosen or a detail opened, wherever the page has one.
fn click_states(key: &'static str, findings: &mut Vec<String>) {
    theme::set_thread_current("krt");
    for english in [false, true] {
        set_thread_language(if english { Choice::En } else { Choice::De });
        for points in CLICK_POINTS {
            for (window, watch) in [(smallest_window(), true), (USUAL_WINDOW, false)] {
                let mut base = harness(key);
                settle(&mut base, points, window);
                let places = click_places(&frame_of(&mut base, window));
                drop(base);
                for (n, (at, name)) in places.into_iter().enumerate() {
                    let mut harness = harness(key);
                    settle(&mut harness, points, window);
                    click(&mut harness, at);
                    settle(&mut harness, points, window);
                    let what = format!("Klick {} auf {name:?}", n + 1);
                    let place = Place { key, points, window, english, scheme: "krt", what };
                    measure(&mut harness, window, watch, &place, findings);
                }
            }
        }
    }
}

/// The pages whose list opens the detail of an entry beside the list or under its row, with entries in the sample.
const LISTS_WITH_DETAIL: [&str; 4] = ["hf_liste", "hf_auftragslog", "hf_herstellung", "hf_bergbau"];

/// The texts of a frame, sorted: what a page shows, whatever stands where.
fn texts(frame: &Frame) -> Vec<String> {
    let mut texts: Vec<String> = frame.lines.iter().map(|line| line.text.clone()).collect();
    texts.sort();
    texts
}

/// The page with the first entry of its list chosen, through every size, window and language.
fn chosen_states(key: &'static str, findings: &mut Vec<String>) {
    theme::set_thread_current("krt");
    for english in [false, true] {
        set_thread_language(if english { Choice::En } else { Choice::De });
        let mut harness = harness(key);
        settle(&mut harness, 1, smallest_window());
        let before = texts(&frame_of(&mut harness, smallest_window()));
        let head = Place {
            key,
            points: 1,
            window: smallest_window(),
            english,
            scheme: "krt",
            what: "erster Eintrag gewählt".to_string(),
        };
        if !harness.state_mut().page.choose_first_entry() {
            findings.push(format!("{} | die Liste hat keinen Eintrag zum Wählen", head.text()));
            continue;
        }
        settle(&mut harness, 1, smallest_window());
        if texts(&frame_of(&mut harness, smallest_window())) == before {
            findings.push(format!("{} | der gewählte Eintrag öffnet nichts", head.text()));
        }
        for points in font_size::POINTS_RANGE.0..=font_size::POINTS_RANGE.1 {
            for (window, watch) in [(smallest_window(), true), (USUAL_WINDOW, false)] {
                settle(&mut harness, points, window);
                let place = Place { points, window, what: head.what.clone(), ..head };
                measure(&mut harness, window, watch, &place, findings);
            }
        }
    }
}

/// The pages that show the remembered releases, on the user's own data.
const RELEASE_PAGES: [&str; 2] = ["hf_ueber", "hf_wasistneu"];

/// Remembered releases: a test version, the newest finished one with a long text and the one before it.
fn releases() -> Vec<Release> {
    const BASE: &str = "https://github.com/Xharig/VerseKit/releases/download";
    let text = |lead: &str| {
        format!(
            "> {lead}\n\n### Neu\n- Eine neue Seite, deren Beschreibung lang genug ist, um über mehr als eine Zeile zu \
             laufen, auch im breiten Fenster\n- Ein zweiter Punkt\n\n### Behoben\n- Ein Fehler, der nur beim Ziehen auftrat\n"
        )
    };
    let release = |version: &str, prerelease: bool, lead: &str| {
        let setup = format!("VerseKit-{version}-setup.exe");
        Release {
            version: version.into(),
            name: version.into(),
            date: "2026-11-15".into(),
            time: "2026-11-15T18:00:00Z".into(),
            text: text(lead),
            prerelease,
            assets: [setup.as_str(), updater::CHECKSUM_FILE]
                .iter()
                .map(|name| Asset { name: name.to_string(), url: format!("{BASE}/{version}/{name}"), size: None })
                .collect(),
        }
    };
    vec![
        release("v5.1.0-rc1", true, "Eine Testversion mit allem, was für die nächste Version geplant ist."),
        release("v5.0.1", false, "Die neueste fertige Version."),
        release("v5.0.0", false, "Die erste Version in Rust."),
    ]
}

/// What updating needs, for a program of the version `own` in `dir`: a net that answers nothing and a start that
/// starts nothing.
fn offline_updates(dir: &Path, own: &str) -> versekit_core::update_flow::Env {
    use versekit_core::update_swap::Started;
    let net: Arc<dyn versekit_core::net::Net> = Arc::new(versekit_core::net::Fake::default());
    versekit_core::update_flow::Env {
        net: net.clone(),
        download_net: net,
        own_version: own.to_string(),
        appimage: None,
        program: dir.join("VerseKit.exe"),
        temp: dir.to_path_buf(),
        start: Arc::new(|_: &Path| -> std::io::Result<Box<dyn Started>> { Err(std::io::Error::other("probe")) }),
        game_running: Arc::new(|| false),
        alive_wait: std::time::Duration::ZERO,
    }
}

/// Two sticks in the game folder `game`, with values on their axes, flight functions on them and an old entry of an
/// earlier id: the page Achsen & Kurven shows all its parts, not only its sentence for a missing file.
fn sticks(game: &Path) {
    let (left, right) = ("03F33344-0000-0000-0000-504944564944", "43F43344-0000-0000-0000-504944564944");
    std::fs::write(
        game.join("Game.log"),
        format!("Connected joystick0:  VPC Stick L  {{{left}}}\nConnected joystick1:  VPC Stick R  {{{right}}}\n"),
    )
    .expect("game log");
    let profile = game.join("USER").join("Client").join("0").join("Profiles").join("default");
    std::fs::create_dir_all(&profile).expect("profile");
    let maps = format!(
        r#"<ActionMaps><ActionProfiles version="1" profileName="default">
<deviceoptions name="VPC Stick L  {{{left}}}"><option input="x" deadzone="0.05"/><option input="x" saturation="0.95"/><option input="y" deadzone="0.05"/><option input="rotz" deadzone="0.1"/><option input="slider1" saturation="0.9"/></deviceoptions>
<deviceoptions name="VPC Stick R  {{{right}}}"><option input="x" deadzone="0.04"/><option input="y" deadzone="0.04"/></deviceoptions>
<deviceoptions name="VPC Stick L  {{83F33344-0000-0000-0000-504944564944}}"><option input="x" deadzone="0.2"/></deviceoptions>
<options type="joystick" instance="1" Product="VPC Stick L  {{{left}}}"><flight_move_pitch exponent="1.5"/><flight_move_strafe_vertical exponent="1.2"/><flight_move_roll exponent="1.3"/></options>
<options type="joystick" instance="2" Product="VPC Stick R  {{{right}}}"/>
<actionmap name="spaceship_movement"><action name="v_pitch"><rebind input="js1_y"/></action><action name="v_strafe_vertical"><rebind input="js1_y"/></action><action name="v_roll"><rebind input="js1_x"/></action><action name="v_yaw"><rebind input="js2_x"/></action></actionmap>
</ActionProfiles></ActionMaps>"#
    );
    std::fs::write(profile.join("actionmaps.xml"), maps).expect("bindings");
}

/// The window on the data folder `dir`, a throwaway one, with [`releases`] remembered and the page `key` open.
/// Nothing reaches the net, the home folder of this computer or its game installation.
fn release_page(dir: &Path, key: &str, own: &str) -> SamplePage {
    use versekit_core::legacy::{ENV_BASETOOL_KEYNAME, ENV_HOME, ENV_NO_NET};
    let game = dir.join("LIVE");
    std::fs::create_dir_all(&game).expect("game folder");
    std::fs::write(game.join("Game.log"), "").expect("game log");
    if key == "hf_achsen" {
        sticks(&game);
    }
    let (home, game) = (dir.to_string_lossy().into_owned(), game.to_string_lossy().into_owned());
    let vars = [
        (ENV_HOME, home.as_str()),
        ("HOME", home.as_str()),
        ("USERPROFILE", home.as_str()),
        (ENV_NO_NET, "1"),
        (ENV_BASETOOL_KEYNAME, "VerseKit Pruefung"),
        (versekit_core::paths::ENV_INSTALL_DIR, game.as_str()),
    ];
    let host = Host::new(Flavor::native(), &vars, &home);
    let cache = Cache { checked: updater::now(), releases: releases() };
    updater::cache_write(&Paths::new(host.clone()), &cache).expect("the releases are remembered");
    let mut page = SamplePage::with_settings_using(Paths::new(host), Arc::new(versekit_gamedata::list::Offline));
    page.use_updates(offline_updates(dir, own));
    page.go_to(key);
    page
}

/// The page `key` with releases remembered, for a build of the source and for a program file, through every
/// size, window and language.
fn release_states(key: &'static str, findings: &mut Vec<String>) {
    theme::set_thread_current("krt");
    for (build, own) in [("Quellcode", "5.0.0-dev"), ("Programmdatei", "5.0.0")] {
        let dir = versekit_core::testdir::Dir::new("layout-guard-versionen");
        for english in [false, true] {
            set_thread_language(if english { Choice::En } else { Choice::De });
            let mut harness = harness_of(release_page(dir.path(), key, own));
            // The look for new versions runs in a thread of its own: its answer comes before the measuring.
            for _ in 0..10 {
                settle(&mut harness, 1, smallest_window());
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            for points in font_size::POINTS_RANGE.0..=font_size::POINTS_RANGE.1 {
                for (window, watch) in [(smallest_window(), true), (USUAL_WINDOW, false)] {
                    settle(&mut harness, points, window);
                    let what = format!("Versionen geladen, {build}");
                    let place = Place { key, points, window, english, scheme: "krt", what };
                    measure(&mut harness, window, watch, &place, findings);
                }
            }
        }
    }
}

/// The page with the maps of the Contested Zones.
const MAP_PAGE: &str = "hf_contested";
/// How far a map is dragged, in shares of the part of it in sight, and how often in each direction.
const DRAG_SHARE: f32 = 0.4;
const DRAGS: usize = 3;

/// The biggest picture of the frame, if it is big enough to be a map and not a symbol.
fn map_of(frame: &Frame) -> Option<&Picture> {
    frame
        .pictures
        .iter()
        .filter(|picture| picture.rect.width().max(picture.rect.height()) >= 200.0)
        .max_by(|a, b| a.rect.area().total_cmp(&b.rect.area()))
}

/// How long a map may be decoding at most (a busy machine takes minutes for the big ones), and how long it may take
/// to come into sight once the page no longer says it is loading.
const MAP_LOADING: std::time::Duration = std::time::Duration::from_secs(600);
const MAP_SIGHT: std::time::Duration = std::time::Duration::from_secs(60);

/// Waits until the map is in sight: as long as the page says it is loading (`s_cz_karte_laedt`, in the tree of
/// the controls), up to
/// [`MAP_LOADING`], else up to [`MAP_SIGHT`]. Whether the page still said it was loading at the end.
fn wait_for_map(harness: &mut Harness<'_, Probe>, window: Vec2) -> bool {
    let loading = versekit_core::language::t("s_cz_karte_laedt");
    let start = std::time::Instant::now();
    loop {
        let frame = frame_of(harness, window);
        // The tree of the controls, not the lines: the words stand at the top of the frame, which may lie above
        // the window while the link under it is in sight.
        let still = harness.root().children_recursive().any(|node| {
            let node = node.accesskit_node();
            node.label().or_else(|| node.value()).is_some_and(|text| text.contains(loading.as_str()))
        });
        if map_of(&frame).is_some_and(|map| map.seen() > 0.0) {
            return false;
        }
        let waited = start.elapsed();
        if waited >= MAP_LOADING || (!still && waited >= MAP_SIGHT) {
            return still;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// Scrolls the node of `role` (named `name`, or else the lowest one) into sight.
fn scroll_to(harness: &mut Harness<'_, Probe>, role: Role, name: Option<&str>) -> bool {
    let found = {
        let root = harness.root();
        let top = |node: &egui_kittest::Node<'_>| node.accesskit_node().raw_bounds().map_or(f64::MIN, |b| b.y0);
        let chosen = root
            .children_recursive()
            .filter(|node| node.accesskit_node().role() == role)
            .filter(|node| name.is_none_or(|name| node.accesskit_node().label().as_deref() == Some(name)))
            .max_by(|a, b| top(a).total_cmp(&top(b)));
        if let Some(node) = &chosen {
            node.scroll_to_me();
        }
        chosen.is_some()
    };
    harness.step();
    harness.step();
    found
}

/// A drag with the left button from `from` by `by`, in small steps.
fn drag(harness: &mut Harness<'_, Probe>, from: Pos2, by: Vec2) {
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    harness.event(egui::Event::PointerMoved(from));
    harness.step();
    harness.event(button(from, true));
    harness.step();
    for n in 1..=8 {
        harness.event(egui::Event::PointerMoved(from + by * (n as f32 / 8.0)));
        harness.step();
    }
    harness.event(button(from + by, false));
    harness.step();
    harness.event(egui::Event::PointerGone);
    harness.step();
}

/// Every map of the Contested Zones, dragged far to the left, the right, up and down: after each direction it
/// fills as much of its frame as before (a map dragged out of its frame cannot be reached again), and in the end
/// nothing of the page is cut off or crooked.
fn map_drags(findings: &mut Vec<String>) {
    theme::set_thread_current("krt");
    set_thread_language(Choice::De);
    let window = USUAL_WINDOW;
    for points in CLICK_POINTS {
        let place = |what: String| Place { key: MAP_PAGE, points, window, english: false, scheme: "krt", what };
        let mut harness = harness(MAP_PAGE);
        settle(&mut harness, points, window);
        let tabs: Vec<String> = frame_of(&mut harness, window)
            .nodes
            .into_iter()
            .filter(|node| node.role == Role::Tab)
            .map(|node| node.name)
            .collect();
        if tabs.is_empty() {
            findings.push(format!("{} | keine Reiter der Karten", place("Karten".into()).text()));
        }
        for name in tabs {
            scroll_to(&mut harness, Role::Tab, Some(&name));
            let tab = frame_of(&mut harness, window)
                .nodes
                .into_iter()
                .find(|node| node.role == Role::Tab && node.name == name);
            if let Some(tab) = tab {
                click(&mut harness, tab.rect.center());
            }
            // The link to the maker stands under the map: with it in sight the map is too.
            scroll_to(&mut harness, Role::Link, None);
            let still_loading = wait_for_map(&mut harness, window);
            settle(&mut harness, points, window);
            let frame = frame_of(&mut harness, window);
            let Some(map) = map_of(&frame) else {
                let why = if still_loading { " (lädt noch)" } else { "" };
                findings.push(format!("{} | die Karte kam nicht{why}", place(format!("Karte {name:?}")).text()));
                continue;
            };
            let (seen, part) = (map.seen(), map.rect.intersect(map.clip));
            if seen < 100.0 * 100.0 {
                findings.push(format!("{} | die Karte ist nicht im Blick", place(format!("Karte {name:?}")).text()));
                continue;
            }
            let reach = part.size() * DRAG_SHARE;
            for (word, way) in [
                ("links", vec2(-reach.x, 0.0)),
                ("rechts", vec2(reach.x, 0.0)),
                ("oben", vec2(0.0, -reach.y)),
                ("unten", vec2(0.0, reach.y)),
            ] {
                for _ in 0..DRAGS {
                    drag(&mut harness, part.center(), way);
                }
                let now = map_of(&frame_of(&mut harness, window)).map_or(0.0, Picture::seen);
                if now < seen - 1.0 {
                    findings.push(format!(
                        "{} | nach dem Ziehen nach {word} füllt die Karte {:.0} % dessen, was sie vorher füllte",
                        place(format!("Karte {name:?}")).text(),
                        100.0 * now / seen
                    ));
                }
            }
            measure(&mut harness, window, false, &place(format!("Karte {name:?} gezogen")), findings);
        }
    }
}

/// (page, part of the find): faults the check found that are not repaired here.
const KNOWN_OPEN: [(&str, &str); 0] = [];

/// The pages of the part `part` (every `PARTS`-th one), through every size, window and language.
fn run_part(part: usize) {
    let mut findings = Vec::new();
    let keys: Vec<&'static str> = page_keys().into_iter().skip(part).step_by(PARTS).collect();
    assert!(!keys.is_empty(), "part {part} has no page");
    // One page alone for a look at it (`LAYOUT_GUARD_PAGE=hf_overlay cargo test ...`).
    let only = std::env::var("LAYOUT_GUARD_PAGE").ok();
    let keys: Vec<&'static str> =
        keys.into_iter().filter(|key| only.as_deref().is_none_or(|wanted| wanted == *key)).collect();
    for key in keys {
        let mut harness = harness(key);
        for english in [false, true] {
            set_thread_language(if english { Choice::En } else { Choice::De });
            theme::set_thread_current("krt");
            for points in font_size::POINTS_RANGE.0..=font_size::POINTS_RANGE.1 {
                for (window, watch) in [(smallest_window(), true), (USUAL_WINDOW, false)] {
                    settle(&mut harness, points, window);
                    let place = Place { key, points, window, english, scheme: "krt", what: "Grundzustand".to_string() };
                    measure(&mut harness, window, watch, &place, &mut findings);
                }
            }
            // A round scheme at both ends of the sizes: the corners follow the rounding.
            theme::set_thread_current("eis");
            for points in [font_size::POINTS_RANGE.0, font_size::POINTS_RANGE.1] {
                settle(&mut harness, points, smallest_window());
                let place = Place {
                    key,
                    points,
                    window: smallest_window(),
                    english,
                    scheme: "eis",
                    what: "Grundzustand".to_string(),
                };
                measure(&mut harness, smallest_window(), false, &place, &mut findings);
            }
        }
        if !NO_CLICKS.contains(&key) {
            click_states(key, &mut findings);
        }
        if LISTS_WITH_DETAIL.contains(&key) {
            chosen_states(key, &mut findings);
        }
        if RELEASE_PAGES.contains(&key) {
            release_states(key, &mut findings);
        }
        if key == MAP_PAGE {
            map_drags(&mut findings);
        }
    }
    set_thread_language(Choice::Auto);
    drawn_cards::record(false);
    // Known faults of pages that another change is on: they stay out of the finds, and the entry has to go as
    // soon as the fault is gone.
    let mut gone = Vec::new();
    for (page, fragment) in KNOWN_OPEN {
        let before = findings.len();
        findings.retain(|find| !(find.starts_with(page) && find.contains(fragment)));
        let owned = page_keys().into_iter().skip(part).step_by(PARTS).any(|key| key == page);
        if owned && only.is_none() && findings.len() == before {
            gone.push(format!("{page}: {fragment}"));
        }
    }
    assert!(gone.is_empty(), "known faults that no longer occur, remove them from KNOWN_OPEN: {gone:?}");
    report(&format!("part-{part}"), &findings);
}

/// Writes every find to the temporary folder and fails with the first ones.
fn report(name: &str, findings: &[String]) {
    let dir = std::env::temp_dir().join("versekit-layout-guard");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join(format!("{name}.txt")), findings.join("\n"));
    let shown: Vec<&String> = findings.iter().take(40).collect();
    assert!(
        findings.is_empty(),
        "{} Funde (alle in {}):\n{}",
        findings.len(),
        dir.join(format!("{name}.txt")).display(),
        shown.iter().map(|find| find.as_str()).collect::<Vec<_>>().join("\n")
    );
}

macro_rules! parts {
    ($($name:ident: $number:expr),* $(,)?) => {
        $(
            #[test]
            fn $name() {
                run_part($number);
            }
        )*
    };
}

parts!(
    pages_part_0: 0,
    pages_part_1: 1,
    pages_part_2: 2,
    pages_part_3: 3,
    pages_part_4: 4,
    pages_part_5: 5,
    pages_part_6: 6,
    pages_part_7: 7,
);

fn card(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
    Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y1))
}

fn line(text: &str, rect: Rect, clip: Rect) -> Line {
    Line { text: text.to_string(), rect, clip, shortened: false, order: 0 }
}

#[test]
fn the_check_sees_a_line_that_reaches_over_its_card() {
    let page = card(0.0, 0.0, 400.0, 300.0);
    let one = card(10.0, 10.0, 200.0, 100.0);
    let frame = |text_right: f32| Frame {
        cards: vec![(one, one)],
        floating: Vec::new(),
        lines: vec![line("Text", card(20.0, 20.0, text_right, 40.0), page)],
        nodes: Vec::new(),
        pictures: Vec::new(),
        window: page,
        overflow: 0.0,
    };
    assert!(cut_off(&frame(150.0)).is_empty(), "a line inside its card is no find");
    let found = cut_off(&frame(260.0));
    assert!(found.iter().any(|find| find.contains("Karte")), "{found:?}");
}

#[test]
fn the_check_sees_lines_over_each_other_but_not_over_a_floating_card() {
    let page = card(0.0, 0.0, 400.0, 300.0);
    let one = card(10.0, 10.0, 300.0, 200.0);
    let frame = |second_top: f32, floating: Vec<(Rect, usize)>| Frame {
        cards: vec![(one, one)],
        floating,
        lines: vec![
            line("Erste Zeile", card(20.0, 20.0, 200.0, 40.0), page),
            line("Zweite Zeile", card(20.0, second_top, 200.0, second_top + 20.0), page),
        ],
        nodes: Vec::new(),
        pictures: Vec::new(),
        window: page,
        overflow: 0.0,
    };
    assert!(cut_off(&frame(42.0, Vec::new())).is_empty(), "lines one under the other");
    let found = cut_off(&frame(30.0, Vec::new()));
    assert!(found.iter().any(|find| find.contains("überdecken")), "{found:?}");
    // The second line belongs to a notice that floats over the card and covers what lies under it: no find.
    let notice = card(15.0, 32.0, 250.0, 70.0);
    assert!(cut_off(&frame(30.0, vec![(notice, 0)])).iter().all(|find| !find.contains("überdecken")));
    // Two lines of the floating notice over each other are a find.
    let both = card(15.0, 15.0, 250.0, 70.0);
    assert!(cut_off(&frame(30.0, vec![(both, 0)])).iter().any(|find| find.contains("überdecken")));
    // A line of the page drawn before the notice lies under it and is none of its lines.
    let mut under = frame(30.0, vec![(both, 3)]);
    under.lines[1].order = 5;
    assert!(cut_off(&under).iter().all(|find| !find.contains("überdecken")), "{:?}", cut_off(&under));
    // A line cut away by its area does not lie over anything.
    let mut hidden = frame(30.0, Vec::new());
    hidden.lines[1].clip = card(0.0, 100.0, 400.0, 300.0);
    assert!(cut_off(&hidden).is_empty(), "{:?}", cut_off(&hidden));
}

#[test]
fn the_check_measures_the_part_of_a_picture_in_its_frame() {
    let frame = card(100.0, 100.0, 500.0, 400.0);
    let whole = Picture { rect: card(100.0, 150.0, 500.0, 350.0), clip: frame };
    assert_eq!(whole.seen(), 400.0 * 200.0);
    let half_out = Picture { rect: card(300.0, 150.0, 600.0, 350.0), clip: frame };
    assert_eq!(half_out.seen(), 200.0 * 200.0);
    let gone = Picture { rect: card(900.0, 150.0, 1300.0, 350.0), clip: frame };
    assert_eq!(gone.seen(), 0.0);
    let pictures = vec![Picture { rect: card(0.0, 0.0, 24.0, 24.0), clip: frame }, whole, half_out];
    let found = Frame {
        cards: Vec::new(),
        floating: Vec::new(),
        lines: Vec::new(),
        nodes: Vec::new(),
        pictures,
        window: frame,
        overflow: 0.0,
    };
    assert_eq!(map_of(&found).map(|map| map.rect), Some(card(100.0, 150.0, 500.0, 350.0)), "the biggest, no symbol");
}

#[test]
fn the_check_sees_a_control_over_the_window_and_a_shortened_line() {
    let page = card(0.0, 0.0, 400.0, 300.0);
    let mut shortened = line("lang…", card(20.0, 20.0, 80.0, 40.0), page);
    shortened.shortened = true;
    let frame = Frame {
        cards: Vec::new(),
        floating: Vec::new(),
        lines: vec![shortened],
        nodes: vec![Node {
            id: 1,
            role: Role::Button,
            name: "Knopf".to_string(),
            rect: card(350.0, 20.0, 430.0, 50.0),
        }],
        pictures: Vec::new(),
        window: page,
        overflow: 3.0,
    };
    let found = cut_off(&frame);
    for part in ["Bedienelement über dem Rand des Fensters", "gekürzt", "ragt 3 Punkte"] {
        assert!(found.iter().any(|find| find.contains(part)), "{part}: {found:?}");
    }
}

#[test]
fn the_check_sees_cards_of_a_row_that_differ_and_a_card_that_grows() {
    let level = [card(10.0, 10.0, 200.0, 110.0), card(208.0, 10.0, 398.0, 110.0)];
    assert!(crooked(&level).is_empty());
    // A card taller than its neighbour is as tall as its content: no finding.
    let tall = [card(10.0, 10.0, 200.0, 110.0), card(208.0, 10.0, 398.0, 135.0)];
    assert!(crooked(&tall).is_empty(), "{:?}", crooked(&tall));
    let low = [card(10.0, 10.0, 200.0, 110.0), card(208.0, 30.0, 398.0, 110.0)];
    assert!(crooked(&low).iter().any(|find| find.contains("beginnen verschieden")));
    // Two columns, the left one a card taller than the right one has under another card: no row.
    let column = [card(10.0, 10.0, 200.0, 200.0), card(208.0, 10.0, 398.0, 100.0), card(208.0, 106.0, 398.0, 200.0)];
    assert!(crooked(&column).is_empty(), "{:?}", crooked(&column));
    assert!(grown(&level, &level, 1).is_empty());
    let more = [level[0], card(208.0, 10.0, 398.0, 112.0)];
    assert!(!grown(&level, &more, 1).is_empty());
    assert!(!grown(&level, &level[..1], 1).is_empty());
    let node = |id: u128, name: &str, height: f32| Node {
        id,
        role: Role::RadioButton,
        name: name.to_string(),
        rect: card(0.0, 0.0, 100.0, height),
    };
    let boxes = [node(1, "Fertige Version", 80.0), node(2, "Testversion", 80.0)];
    assert!(grown_nodes(&boxes, &boxes, 1).is_empty());
    let taller = [node(1, "Fertige Version", 82.0), node(2, "Testversion", 80.0)];
    assert!(grown_nodes(&boxes, &taller, 1).iter().any(|find| find.contains("Fertige Version")));
    // A node that came or went has nothing to be compared with, whatever its name.
    assert!(grown_nodes(&boxes, &[node(3, "Testversion", 30.0)], 1).is_empty());
}

#[test]
fn the_parts_cover_every_page_once() {
    let keys = page_keys();
    let mut seen = std::collections::HashSet::new();
    assert!(keys.iter().all(|key| seen.insert(*key)), "a page key stands twice");
    for part in 0..PARTS {
        assert!(keys.iter().skip(part).step_by(PARTS).count() > 0, "part {part} is empty");
    }
    // Every page the overview can pin is among them, and every page of the states beyond the sample data.
    for (_, key) in versekit_core::overview::PAGES {
        assert!(keys.contains(&key), "{key} is checked in no part");
    }
    for key in LISTS_WITH_DETAIL.iter().chain(&RELEASE_PAGES).chain([&MAP_PAGE]) {
        assert!(keys.contains(key), "{key} is no page of the window");
    }
}

/// Wide and flat windows (interface points at the size `normal`): Full HD, WQHD, ultrawide, and two flat ones.
const WIDE_WINDOWS: [Vec2; 5] =
    [vec2(1920.0, 1080.0), vec2(2560.0, 1440.0), vec2(3440.0, 1440.0), vec2(3440.0, 900.0), vec2(2560.0, 600.0)];
/// The farthest a switch, a choice, a slider or a number field stands from the label to its left on its line.
const MOST_FROM_NAME: f32 = 700.0;
/// The farthest a button stands from the field to its left on its line.
const NEAR_FIELD: f32 = 48.0;

/// The smallest of `cards` that holds the middle of `rect` (a card in a card is the nearer one).
fn innermost(cards: &[Rect], rect: Rect) -> Option<Rect> {
    cards.iter().copied().filter(|card| card.contains(rect.center())).min_by(|a, b| a.area().total_cmp(&b.area()))
}

/// What stands too far apart in a wide window: a control far from its name, a field wider than a field gets
/// (search lines are as wide as their place by the house rules and are left out), a button far from its field.
fn spread(nodes: &[Node], searches: &[Rect], lists: &[Rect], cards: &[Rect]) -> Vec<String> {
    let mut found = Vec::new();
    let is_search =
        |rect: Rect| searches.iter().any(|s| (s.min - rect.min).length() < 3.0 && (s.max - rect.max).length() < 3.0);
    // A row of a list has its actions at its right end by the house rules.
    let in_a_list = |rect: Rect| lists.iter().any(|list| list.contains(rect.center()));
    for node in nodes {
        if in_a_list(node.rect) {
            continue;
        }
        // What stands in the card beside it on the same height is not on its line.
        let card = innermost(cards, node.rect);
        let on_line = |other: Rect| {
            other.max.y > node.rect.min.y + 2.0
                && other.min.y < node.rect.max.y - 2.0
                && card.is_none_or(|card| card.contains(other.center()))
        };
        match node.role {
            Role::TextInput if !is_search(node.rect) => {
                if node.rect.width() > versekit_ui::widgets::FIELD_MAX_WIDTH + TOLERANCE {
                    found.push(format!("Feld {:.0} breit: {:?}", node.rect.width(), node.name));
                }
                let next = nodes
                    .iter()
                    .filter(|b| {
                        b.role == Role::Button && on_line(b.rect) && b.rect.min.x >= node.rect.max.x - TOLERANCE
                    })
                    .map(|b| b.rect.min.x - node.rect.max.x)
                    .fold(f32::INFINITY, f32::min);
                if next.is_finite() && next > NEAR_FIELD {
                    found.push(format!("Knopf {next:.0} Punkte neben dem Feld {:?}", node.name));
                }
            }
            Role::CheckBox | Role::Switch | Role::ComboBox | Role::Slider | Role::SpinButton => {
                // The nearest thing to its left on its line: when that is a label, it is the name of the control.
                let left = nodes
                    .iter()
                    .filter(|o| o.id != node.id && on_line(o.rect) && o.rect.max.x <= node.rect.min.x + TOLERANCE)
                    .filter(|o| is_control(o.role))
                    .min_by(|a, b| (node.rect.min.x - a.rect.max.x).total_cmp(&(node.rect.min.x - b.rect.max.x)));
                if let Some(name) = left.filter(|o| o.role == Role::Label) {
                    let gap = node.rect.min.x - name.rect.max.x;
                    if gap > MOST_FROM_NAME {
                        found.push(format!("{:?} {:?} steht {gap:.0} Punkte von seinem Namen", node.role, node.name));
                    }
                }
            }
            _ => {}
        }
    }
    found.sort();
    found.dedup();
    found
}

/// In a wide or flat window the controls stay with their names, fields keep a width one can use, and a button
/// stands beside its field: on every page.
#[test]
fn in_a_wide_window_nothing_stands_far_from_what_it_belongs_to() {
    set_thread_language(Choice::De);
    theme::set_thread_current("krt");
    let mut findings = Vec::new();
    let mut controls = 0;
    // The hangar is rebuilt in a session of its own.
    for key in page_keys().into_iter().filter(|key| *key != "hf_hangar") {
        for window in WIDE_WINDOWS {
            let mut harness = harness(key);
            settle(&mut harness, 1, window);
            drawn_cards::take_searches();
            drawn_cards::take_lists();
            let frame = frame_of(&mut harness, window);
            let (searches, lists) = (drawn_cards::take_searches(), drawn_cards::take_lists());
            // The window itself stands in a list frame: only the lists on the page count.
            let lists: Vec<Rect> = lists.into_iter().filter(|list| list.width() < window.x - 40.0).collect();
            controls += frame.nodes.iter().filter(|n| matches!(n.role, Role::CheckBox | Role::TextInput)).count();
            let cards: Vec<Rect> = frame.cards.iter().map(|(card, _)| *card).collect();
            for find in spread(&frame.nodes, &searches, &lists, &cards) {
                findings.push(format!("{key} {:.0}×{:.0}: {find}", window.x, window.y));
            }
        }
    }
    drawn_cards::record(false);
    assert!(controls > 100, "only {controls} switches and fields looked at");
    assert!(findings.is_empty(), "{} Befunde:\n{}", findings.len(), findings.join("\n"));
}

#[test]
fn the_check_of_wide_windows_sees_what_stands_apart() {
    let node =
        |id: u128, role: Role, x0: f32, x1: f32| Node { id, role, name: String::new(), rect: card(x0, 10.0, x1, 30.0) };
    // A name, its switch right after it, a field of a usual width with its button beside it: fine.
    let fine = [node(1, Role::Label, 10.0, 300.0), node(2, Role::Switch, 380.0, 424.0)];
    assert!(spread(&fine, &[], &[], &[]).is_empty());
    let field = [node(1, Role::TextInput, 10.0, 500.0), node(2, Role::Button, 508.0, 600.0)];
    assert!(spread(&field, &[], &[], &[]).is_empty());
    // The switch at the far edge, a field over the whole width, a button far from its field.
    let far = [node(1, Role::Label, 10.0, 300.0), node(2, Role::Switch, 1500.0, 1544.0)];
    assert_eq!(spread(&far, &[], &[], &[]).len(), 1);
    assert_eq!(spread(&[node(1, Role::TextInput, 10.0, 1500.0)], &[], &[], &[]).len(), 1);
    let apart = [node(1, Role::TextInput, 10.0, 500.0), node(2, Role::Button, 900.0, 1000.0)];
    assert_eq!(spread(&apart, &[], &[], &[]).len(), 1);
    // The same button in the card beside the one of the field is not its button.
    let two_cards = [card(0.0, 0.0, 700.0, 100.0), card(710.0, 0.0, 1400.0, 100.0)];
    assert!(spread(&apart, &[], &[], &two_cards).is_empty());
    assert_eq!(spread(&apart, &[], &[], &[card(0.0, 0.0, 1400.0, 100.0)]).len(), 1, "one card: still far");
    // A search line may be as wide as its place; a row of a list keeps its actions at its right end; a choice after
    // another choice is no pair of a name and its control.
    let search = node(1, Role::TextInput, 10.0, 1500.0);
    let place = search.rect;
    assert!(spread(&[search], &[place], &[], &[]).is_empty());
    assert!(spread(&far, &[], &[card(0.0, 0.0, 2000.0, 100.0)], &[]).is_empty());
    let choices = [
        node(1, Role::Label, 10.0, 100.0),
        node(2, Role::ComboBox, 120.0, 300.0),
        node(3, Role::ComboBox, 1000.0, 1200.0),
    ];
    assert!(spread(&choices, &[], &[], &[]).is_empty());
}

// ------------------------------------------------------------ the rules of every page

/// The windows every page is drawn in for the rules: the usual widths and flat ones.
const RULE_WINDOWS: [Vec2; 6] = [
    vec2(1280.0, 800.0),
    vec2(1920.0, 1080.0),
    vec2(2560.0, 1440.0),
    vec2(3440.0, 1440.0),
    vec2(3440.0, 900.0),
    vec2(2560.0, 600.0),
];
/// The least share of its width a card fills with its content (the rest is empty to the right).
const LEAST_FILL: f32 = 0.65;
/// Cards narrower than this are left to the other checks: a small card is full by its nature.
const FILL_FROM_WIDTH: f32 = 320.0;

/// Pages that keep a rule apart, each with its reason. A page here whose findings are gone has to leave the list.
const RULE_EXCEPTIONS: [(&str, &str); 23] = [
    // Pages whose rebuild on the building blocks comes in 5.1.4 after this change (a session of its own): each leaves
    // the list when it is rebuilt; the check then holds for it like for every other page.
    ("hf_farmliste", "Umbau ausstehend: Karte über volle Breite"),
    ("hf_bergbau", "Umbau ausstehend: leere Karten, Hinweiszeilen"),
    ("hf_bergung", "Umbau ausstehend: Karte über volle Breite"),
    ("hf_einkaufsliste", "Umbau ausstehend: Karten über volle Breite"),
    ("hf_contested", "Umbau ausstehend: Karten über volle Breite"),
    ("hf_allgemein", "Umbau ausstehend: Accounts und Bestand über volle Breite"),
    ("hf_ordner", "Umbau ausstehend: Spielkanäle schmal"),
    ("hf_module", "Umbau ausstehend: eine Spalte bei 1280"),
    ("hf_erkennung", "Umbau ausstehend: Karten über volle Breite"),
    ("hf_startprogramme", "Umbau ausstehend: Karten über volle Breite"),
    ("k_basetool_verbindung", "Umbau ausstehend: Verbindung und Datenschutz über volle Breite"),
    ("hf_sichern", "Umbau ausstehend: Karten über volle Breite"),
    ("hf_spiel", "Umbau ausstehend: Karten über volle Breite"),
    ("hf_grafik", "Umbau ausstehend: Schreib-Leiste und Rechner-Karte über volle Breite (Shader-Karte erfüllt die Regeln)"),
    ("hf_blickwinkel", "Umbau ausstehend: Karte über volle Breite"),
    ("hf_joysticks", "Umbau ausstehend: Karten über volle Breite"),
    ("hf_ueber", "Umbau ausstehend (Auftrag Update & Über)"),
    ("hf_patchaenderungen", "Umbau ausstehend (Auftrag Geänderte Spielwerte)"),
    ("hf_serverstatus", "Umbau ausstehend (Auftrag Serverstatus)"),
    ("hf_handlesuche", "Umbau ausstehend (Auftrag Handle-Suche)"),
    ("hf_danke", "Umbau ausstehend (Auftrag Danke & Lizenzen)"),
    ("hf_wasistneu", "Umbau ausstehend (Auftrag Was ist neu)"),
    ("hf_diagnose", "Umbau ausstehend: Karten über volle Breite"),
];

/// Cards that run empty to the right: the content (lines and controls seen in the card) ends before
/// [`LEAST_FILL`] of its width.
fn empty_cards(cards: &[(Rect, Rect)], lines: &[Rect], nodes: &[Rect], pictures: &[Rect]) -> Vec<String> {
    empty_cards_named(cards, lines, nodes, pictures, &[])
}

/// [`empty_cards`] that names each card found by the topmost of `texts` in it (its title).
fn empty_cards_named(
    cards: &[(Rect, Rect)],
    lines: &[Rect],
    nodes: &[Rect],
    pictures: &[Rect],
    texts: &[(Rect, String)],
) -> Vec<String> {
    let mut found = Vec::new();
    for (card, seen) in cards {
        if card.width() < FILL_FROM_WIDTH || seen.height() < 60.0 {
            continue;
        }
        // A card with a picture area is an editor: it may take the whole width (its pictures are checked apart).
        if pictures.iter().any(|picture| card.contains(picture.center())) {
            continue;
        }
        // The content of the whole card, also of the part below the edge of the window: a card cut by it is not empty.
        let inside = |rect: &&Rect| card.contains(rect.center()) && rect.width() < card.width();
        let right = lines.iter().chain(nodes).filter(inside).map(|rect| rect.max.x).fold(card.min.x, f32::max);
        let fill = (right - card.min.x) / card.width();
        if fill < LEAST_FILL {
            let title = texts
                .iter()
                .filter(|(rect, _)| card.contains(rect.center()))
                .min_by(|a, b| a.0.top().total_cmp(&b.0.top()))
                .map_or("", |(_, text)| text.as_str());
            found.push(format!(
                "Karte „{title}“ zu {:.0} % leer ({:.0} breit, Inhalt bis {:.0})",
                (1.0 - fill) * 100.0,
                card.width(),
                right - card.min.x
            ));
        }
    }
    found
}

/// Columns of cards side by side whose ends lie far apart while a card of the longer one would fit into the shorter
/// one: moving it would end both higher. The cards of a page only (a card in a card is left out); a wide card begins a
/// new run of columns.
fn unbalanced_columns(cards: &[Rect]) -> Vec<String> {
    const SPACE: f32 = 12.0;
    let top: Vec<Rect> = cards
        .iter()
        .copied()
        .filter(|c| !cards.iter().any(|o| o != c && o.contains_rect(*c) && o.area() > c.area()))
        .collect();
    let (Some(left), Some(right)) =
        (top.iter().map(|c| c.left()).reduce(f32::min), top.iter().map(|c| c.right()).reduce(f32::max))
    else {
        return Vec::new();
    };
    let wide: Vec<Rect> = top.iter().copied().filter(|c| c.width() > 0.6 * (right - left)).collect();
    let narrow: Vec<Rect> = top.iter().copied().filter(|c| c.width() <= 0.6 * (right - left)).collect();
    // The run of a card: how many wide cards stand above it.
    let run = |c: &Rect| wide.iter().filter(|w| w.top() < c.top()).count();
    let mut found = Vec::new();
    for a in &narrow {
        for b in &narrow {
            // `a` is the last card of its column in its run, `b` the last of another column of the same run.
            let last = |c: &Rect| {
                !narrow
                    .iter()
                    .any(|o| o != c && (o.left() - c.left()).abs() < 1.0 && run(o) == run(c) && o.top() > c.top())
            };
            if (a.left() - b.left()).abs() < 1.0 || run(a) != run(b) || !last(a) || !last(b) {
                continue;
            }
            let gap = a.bottom() - b.bottom();
            // Some card of the column of `a` that would end higher under `b`.
            let movable = narrow
                .iter()
                .filter(|c| (c.left() - a.left()).abs() < 1.0 && run(c) == run(a) && c.top() > b.top())
                .any(|c| c.height() + SPACE + 0.5 < gap);
            if movable {
                found.push(format!("Spalten laufen {gap:.0} auseinander, eine Karte passt in die kürzere"));
            }
        }
    }
    found
}

/// Settings rows without a control: by the house rules a small heading instead.
fn bare_rows(rows: &[Rect]) -> Vec<String> {
    rows.iter()
        .map(|row| format!("Zeile ohne Steuerelement bei {:.0},{:.0} (Untertitel nehmen)", row.min.x, row.min.y))
        .collect()
}

/// Picture areas narrower than [`versekit_ui::widgets::PICTURE_MIN_WIDTH`]: nobody makes out what they show.
fn small_pictures(pictures: &[Rect]) -> Vec<String> {
    let least = versekit_ui::widgets::PICTURE_MIN_WIDTH;
    pictures
        .iter()
        .filter(|picture| picture.width() < least - TOLERANCE)
        .map(|picture| format!("Bildfläche nur {:.0} breit (mindestens {least:.0})", picture.width()))
        .collect()
}

/// Hints in small grey text that stand as a line under a row: right below a line with a control, beginning where the
/// name of that line begins. The house rules want them on hover behind the symbol `info`.
fn hints_under_rows(hints: &[Rect], nodes: &[Node], cards: &[Rect]) -> Vec<String> {
    let control = |role: Role| {
        matches!(
            role,
            Role::CheckBox
                | Role::Switch
                | Role::ComboBox
                | Role::Slider
                | Role::SpinButton
                | Role::TextInput
                | Role::RadioButton
        )
    };
    let mut found = Vec::new();
    for hint in hints {
        // Only a row of the card the hint stands in.
        let card = innermost(cards, *hint);
        let in_card = |rect: Rect| card.is_none_or(|card| innermost(cards, rect) == Some(card));
        let row = nodes.iter().filter(|n| control(n.role) && in_card(n.rect)).find(|n| {
            let above = n.rect.max.y <= hint.min.y + 2.0 && n.rect.max.y >= hint.min.y - 30.0;
            let line =
                |o: &&Node| o.rect.max.y > n.rect.min.y + 2.0 && o.rect.min.y < n.rect.max.y - 2.0 && in_card(o.rect);
            let named_at_the_hint =
                nodes.iter().filter(line).any(|o| o.role == Role::Label && (o.rect.min.x - hint.min.x).abs() < 6.0);
            above && n.rect.min.x > hint.min.x && named_at_the_hint
        });
        if row.is_some() {
            let name = row.map(|n| n.name.as_str()).unwrap_or_default();
            found.push(format!(
                "Hinweis als Zeile unter einer Einstellung bei {:.0},{:.0} ({name})",
                hint.min.x, hint.min.y
            ));
        }
    }
    found
}

/// Choices with more than one piece standing out as chosen.
fn doubled_choices(choices: &[(String, bool)]) -> Vec<String> {
    let mut names: Vec<&str> = choices.iter().filter(|(_, on)| *on).map(|(name, _)| name.as_str()).collect();
    names.sort_unstable();
    let mut found = Vec::new();
    for name in names.iter().copied() {
        let count = names.iter().filter(|n| **n == name).count();
        let said = format!("Auswahl „{name}“ {count}-mal hervorgehoben");
        if count > 1 && !found.contains(&said) {
            found.push(said);
        }
    }
    found
}

/// Every page of the side bar, on real data where it has them in the sample, in German and English, in every window
/// of [`RULE_WINDOWS`]: no card runs empty to the right, no hint stands as a line under a row, no choice shows two
/// pieces as chosen. A page that keeps a rule apart stands in [`RULE_EXCEPTIONS`] with its reason.
#[test]
fn every_page_keeps_the_rules_of_cards_hints_and_choices() {
    let mut by_page: Vec<(String, Vec<String>)> = Vec::new();
    for language in [Choice::De, Choice::En] {
        set_thread_language(language);
        theme::set_thread_current("krt");
        // One page alone for a look at it (`LAYOUT_GUARD_PAGE=hf_overlay cargo test ...`).
        let only = std::env::var("LAYOUT_GUARD_PAGE").ok();
        for key in page_keys().into_iter().filter(|key| only.as_deref().is_none_or(|one| one == *key)) {
            // The sample of the page and the page on data of a throwaway folder, offline.
            let dir = versekit_core::testdir::Dir::new("layout-guard-regeln");
            let placeholder = versekit_core::language::t("s_mu_seite");
            for (window, real) in RULE_WINDOWS.iter().flat_map(|w| [(*w, false), (*w, true)]) {
                let mut harness = if real { harness_of(release_page(dir.path(), key, "5.0.0")) } else { harness(key) };
                settle(&mut harness, 1, window);
                // The sticks are read in a thread of their own: their page is measured once they stand on it.
                if real && key == "hf_achsen" {
                    for _ in 0..20 {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                        settle(&mut harness, 1, window);
                    }
                }
                drawn_cards::take_hints();
                drawn_cards::take_choices();
                drawn_cards::take_rows();
                drawn_cards::take_pictures();
                drawn_cards::take_bare_rows();
                let frame = frame_of(&mut harness, window);
                let (hints, choices, rows) =
                    (drawn_cards::take_hints(), drawn_cards::take_choices(), drawn_cards::take_rows());
                let (pictures, bare) = (drawn_cards::take_pictures(), drawn_cards::take_bare_rows());
                // The real page that only shows the placeholder of the sample says nothing of the page.
                if real && frame.lines.iter().any(|l| l.text == placeholder) {
                    continue;
                }
                // The rows count with their column of controls: it is where every control of the card stands.
                let lines: Vec<Rect> = frame.lines.iter().map(|l| l.rect.intersect(l.clip)).chain(rows).collect();
                // A card in a card is content of the outer one.
                let nodes: Vec<Rect> = frame
                    .nodes
                    .iter()
                    .map(|n| n.rect)
                    .chain(frame.cards.iter().map(|(card, seen)| card.intersect(*seen)))
                    .collect();
                let texts: Vec<(Rect, String)> = frame.lines.iter().map(|l| (l.rect, l.text.clone())).collect();
                let mut found = empty_cards_named(&frame.cards, &lines, &nodes, &pictures, &texts);
                found.extend(small_pictures(&pictures));
                found.extend(bare_rows(&bare));
                let page_cards: Vec<Rect> = frame.cards.iter().map(|(card, _)| *card).collect();
                found.extend(unbalanced_columns(&page_cards));
                let cards: Vec<Rect> = frame.cards.iter().map(|(card, _)| *card).collect();
                found.extend(hints_under_rows(&hints, &frame.nodes, &cards));
                found.extend(doubled_choices(&choices));
                let tag = format!(
                    "{}{} {:.0}×{:.0}",
                    if language == Choice::De { "DE" } else { "EN" },
                    if real { " echt" } else { "" },
                    window.x,
                    window.y
                );
                if found.is_empty() {
                    continue;
                }
                match by_page.iter_mut().find(|(page, _)| page == key) {
                    Some((_, list)) => list.extend(found.into_iter().map(|f| format!("{tag}: {f}"))),
                    None => by_page.push((key.to_string(), found.into_iter().map(|f| format!("{tag}: {f}")).collect())),
                }
            }
        }
    }
    drawn_cards::record(false);
    set_thread_language(Choice::De);
    // Every page with findings, for the list of what is left to do.
    for (page, list) in &by_page {
        let count = |word: &str| list.iter().filter(|f| f.contains(word)).count();
        let (empty, hints, choices) = (count("leer ("), count("Hinweis als Zeile"), count("hervorgehoben"));
        let (pictures, bare, columns) = (count("Bildfläche"), count("ohne Steuerelement"), count("auseinander"));
        let real = if list.iter().any(|f| f.contains(" echt ")) { ", auch echt" } else { "" };
        eprintln!(
            "SEITE {page}: leere Karten {empty}, Hinweiszeilen {hints}, doppelt hervorgehoben {choices}, kleine Bildflächen {pictures}, Zeilen ohne Steuerelement {bare}, ungleiche Spalten {columns}{real}"
        );
        let shown = if std::env::var("LAYOUT_GUARD_PAGE").is_ok() { list.len() } else { 4 };
        for finding in list.iter().take(shown) {
            eprintln!("  BEFUND {finding}");
        }
    }
    let open: Vec<String> = by_page
        .iter()
        .filter(|(page, _)| !RULE_EXCEPTIONS.iter().any(|(own, _)| own == page))
        .map(|(page, list)| format!("{page}:\n  {}", list.iter().take(6).cloned().collect::<Vec<_>>().join("\n  ")))
        .collect();
    let stale: Vec<&str> =
        RULE_EXCEPTIONS.iter().map(|(page, _)| *page).filter(|page| !by_page.iter().any(|(p, _)| p == page)).collect();
    assert!(open.is_empty(), "{} Seiten verletzen die Regeln:\n{}", open.len(), open.join("\n"));
    // Only a run over every page knows which exceptions are left without a finding.
    if std::env::var("LAYOUT_GUARD_PAGE").is_err() {
        assert!(stale.is_empty(), "Ausnahmen ohne Befund, aus der Liste nehmen: {stale:?}");
    }
}

#[test]
fn the_rules_of_every_page_see_an_empty_card_a_hint_under_a_row_and_a_doubled_choice() {
    // A card of 1000 whose content ends at 400: empty; one whose content ends at 800: full.
    let lines = [card(10.0, 10.0, 400.0, 30.0)];
    let full = [card(10.0, 40.0, 800.0, 60.0)];
    let seen = card(0.0, 0.0, 1000.0, 100.0);
    assert_eq!(empty_cards(&[(seen, seen)], &lines, &[], &[]).len(), 1);
    assert!(empty_cards(&[(seen, seen)], &lines, &full, &[]).is_empty());
    // A card with a picture area is an editor and may be wide; the picture keeps its least width.
    assert!(empty_cards(&[(seen, seen)], &lines, &[], &[card(10.0, 40.0, 600.0, 90.0)]).is_empty());
    assert!(small_pictures(&[card(10.0, 40.0, 600.0, 90.0)]).is_empty());
    assert_eq!(small_pictures(&[card(10.0, 40.0, 400.0, 90.0)]).len(), 1, "a picture too small to see");
    // Two columns: the left ends 600 lower, and its second card (100 tall) would fit under the right one.
    let uneven = [
        card(0.0, 0.0, 400.0, 300.0),
        card(0.0, 312.0, 400.0, 412.0),
        card(0.0, 424.0, 400.0, 1000.0),
        card(412.0, 0.0, 812.0, 400.0),
    ];
    assert!(!unbalanced_columns(&uneven).is_empty());
    // Level columns, or one long card that no move makes shorter: fine.
    let level = [card(0.0, 0.0, 400.0, 300.0), card(0.0, 312.0, 400.0, 412.0), card(412.0, 0.0, 812.0, 400.0)];
    assert!(unbalanced_columns(&level).is_empty());
    let long = [card(0.0, 0.0, 400.0, 1000.0), card(412.0, 0.0, 812.0, 200.0)];
    assert!(unbalanced_columns(&long).is_empty());
    assert_eq!(bare_rows(&[card(10.0, 10.0, 400.0, 30.0)]).len(), 1);
    // A row of a name and its switch, a hint right under the name: found; the same hint away from any row: not.
    let node = |role: Role, x0: f32, x1: f32, y0: f32| Node {
        id: 0,
        role,
        name: String::new(),
        rect: card(x0, y0, x1, y0 + 20.0),
    };
    let row = [node(Role::Label, 20.0, 200.0, 10.0), node(Role::Switch, 380.0, 424.0, 10.0)];
    assert_eq!(hints_under_rows(&[card(20.0, 32.0, 300.0, 44.0)], &row, &[]).len(), 1);
    assert!(hints_under_rows(&[card(20.0, 200.0, 300.0, 212.0)], &row, &[]).is_empty());
    // The same hint in a card of its own beside the row: not under the row.
    let cards = [card(0.0, 0.0, 450.0, 31.0), card(0.0, 31.0, 450.0, 100.0)];
    assert!(hints_under_rows(&[card(20.0, 32.0, 300.0, 44.0)], &row, &cards).is_empty());
    // Two pieces of one choice chosen: found; one chosen of each: not.
    let doubled = [("platz".to_string(), true), ("platz".to_string(), true), ("schema".to_string(), true)];
    assert_eq!(doubled_choices(&doubled).len(), 1);
    assert!(doubled_choices(&[("platz".to_string(), true), ("platz".to_string(), false)]).is_empty());
}
