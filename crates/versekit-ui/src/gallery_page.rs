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

//! The dialog of the gallery „Was ist neu in X“ (data: [`versekit_core::gallery`]).
//!
//! One page for each highlight (picture, kind, title, text, way, a button to the place) and at the end the other
//! points of the series in two columns. Paged with the buttons below, the bars at the left of them or the arrow
//! keys. The caller asks [`gallery::due`] once at the start, keeps a [`GalleryState`] while it is open, draws it
//! every frame and acts on the [`GalleryOutcome`].

use egui::{Align, Layout, RichText, ScrollArea, Sense, TextStyle, TextureHandle, Ui, vec2};
use versekit_core::changelog::{self, Kind};
use versekit_core::gallery::{self, Highlight, Target};
use versekit_core::language::{Arg, Lang, current, t, tf};
use versekit_core::theme::{Color, Scheme};
use versekit_core::updater::Release;
use versekit_core::version::parts;

use crate::color;
use crate::widgets::{self, ButtonKind};

/// Widest the dialog gets.
pub const WIDTH: f32 = 760.0;
/// Room kept free around the dialog on each side.
const MARGIN: f32 = 16.0;
/// Height of the dialog outside its body: head, foot, the margins of the box and the gaps between them.
const CHROME: f32 = 130.0;
/// Room under the picture first assumed for the texts of a page, before one was measured.
const TEXT_ROOM: f32 = 190.0;
/// Room left free under the texts of a page, for the gap below the picture.
const SLACK: f32 = 8.0;
/// The smallest height the picture is shrunk to.
const MIN_PICTURE: f32 = 60.0;
/// Size of a bar of the page marks, and the height a click on it is taken in.
const MARK: egui::Vec2 = vec2(28.0, 4.0);
const MARK_HIT: f32 = 16.0;

/// The pictures of the gallery: name, German, English.
const PICTURES: [(&str, &[u8], &[u8]); 12] = [
    (
        "basetool",
        include_bytes!("../../../assets/galerie/basetool.png"),
        include_bytes!("../../../assets/galerie/basetool-en.png"),
    ),
    (
        "bedarf",
        include_bytes!("../../../assets/galerie/bedarf.png"),
        include_bytes!("../../../assets/galerie/bedarf-en.png"),
    ),
    (
        "deine_karten",
        include_bytes!("../../../assets/galerie/deine_karten.png"),
        include_bytes!("../../../assets/galerie/deine_karten-en.png"),
    ),
    (
        "grafik",
        include_bytes!("../../../assets/galerie/grafik.png"),
        include_bytes!("../../../assets/galerie/grafik-en.png"),
    ),
    (
        "karten",
        include_bytes!("../../../assets/galerie/karten.png"),
        include_bytes!("../../../assets/galerie/karten-en.png"),
    ),
    (
        "lager",
        include_bytes!("../../../assets/galerie/lager.png"),
        include_bytes!("../../../assets/galerie/lager-en.png"),
    ),
    (
        "shader",
        include_bytes!("../../../assets/galerie/shader.png"),
        include_bytes!("../../../assets/galerie/shader-en.png"),
    ),
    ("korb", include_bytes!("../../../assets/galerie/korb.png"), include_bytes!("../../../assets/galerie/korb-en.png")),
    (
        "laut_log",
        include_bytes!("../../../assets/galerie/laut_log.png"),
        include_bytes!("../../../assets/galerie/laut_log-en.png"),
    ),
    (
        "zeitraum",
        include_bytes!("../../../assets/galerie/zeitraum.png"),
        include_bytes!("../../../assets/galerie/zeitraum-en.png"),
    ),
    (
        "streamer",
        include_bytes!("../../../assets/galerie/streamer.png"),
        include_bytes!("../../../assets/galerie/streamer-en.png"),
    ),
    (
        "overlay_ereignisse",
        include_bytes!("../../../assets/galerie/overlay_ereignisse.png"),
        include_bytes!("../../../assets/galerie/overlay_ereignisse-en.png"),
    ),
];

/// The bytes of the picture `name` in `lang`: the English one in English, the German one otherwise.
pub fn picture_bytes(name: &str, lang: Lang) -> Option<&'static [u8]> {
    PICTURES.iter().find(|(n, _, _)| *n == name).map(|(_, de, en)| if lang == Lang::En { *en } else { *de })
}

/// What the user did in this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GalleryOutcome {
    /// The gallery was closed (button, Esc, a click beside it, or „Ansehen“): the caller calls
    /// [`gallery::mark_shown`] and drops the state.
    pub closed: bool,
    /// The place to open now.
    pub open: Option<Target>,
}

/// A picture made into a texture, with the language it was made for.
struct Picture {
    name: &'static str,
    lang: Lang,
    texture: Option<TextureHandle>,
}

/// The gallery while it is open: the version, the page shown, the pictures loaded and the list of the other points.
pub struct GalleryState {
    /// `4.0.1`.
    version: String,
    highlights: &'static [Highlight],
    releases: Vec<Release>,
    /// The page shown: a highlight, or the list after the last one.
    page: usize,
    pictures: Vec<Picture>,
    /// The other points of the series in the language they were read in.
    rest: Option<(Lang, Vec<(Kind, String)>)>,
    /// The height the texts under the picture took in the last frame.
    text_height: f32,
}

impl GalleryState {
    /// The gallery of the series of `version`, with the other points read from the change log built in and the
    /// release texts of `releases` (empty: the built-in log alone). `None` when the series has no highlights.
    pub fn new(version: &str, releases: Vec<Release>) -> Option<GalleryState> {
        let highlights = gallery::highlights(version);
        if highlights.is_empty() {
            return None;
        }
        let (major, minor, patch) = parts(version);
        Some(GalleryState {
            version: format!("{major}.{minor}.{patch}"),
            highlights,
            releases,
            page: 0,
            pictures: Vec::new(),
            rest: None,
            text_height: TEXT_ROOM,
        })
    }

    /// The version in the title.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// The page shown (0 is the first highlight).
    pub fn page(&self) -> usize {
        self.page
    }

    /// The other points of the series in the language in force, read once for each language.
    fn rest(&mut self) -> &[(Kind, String)] {
        let lang = current();
        if self.rest.as_ref().is_none_or(|(read, _)| *read != lang) {
            let entries = changelog::history(lang, &self.releases);
            self.rest = Some((lang, gallery::rest(&self.version, lang, &entries)));
        }
        self.rest.as_ref().map_or(&[], |(_, rest)| rest.as_slice())
    }

    /// How many pages there are: one for each highlight, and the list when it has points.
    pub fn pages(&mut self) -> usize {
        let list = usize::from(!self.rest().is_empty());
        self.highlights.len() + list
    }

    /// The texture of a picture in the language in force, made once.
    fn texture(&mut self, ctx: &egui::Context, name: &'static str) -> Option<TextureHandle> {
        let lang = current();
        if let Some(picture) = self.pictures.iter().find(|p| p.name == name && p.lang == lang) {
            return picture.texture.clone();
        }
        let texture = picture_bytes(name, lang).and_then(|bytes| {
            let decoded = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).ok()?.to_rgba8();
            let (width, height) = decoded.dimensions();
            let image = egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], decoded.as_raw());
            let suffix = if lang == Lang::En { "-en" } else { "" };
            Some(ctx.load_texture(format!("galerie-{name}{suffix}"), image, egui::TextureOptions::LINEAR))
        });
        self.pictures.push(Picture { name, lang, texture: texture.clone() });
        texture
    }

    /// Goes to page `page`, kept within the pages there are.
    fn go(&mut self, page: usize) {
        let last = self.pages().saturating_sub(1);
        self.page = page.min(last);
    }

    /// Draws the dialog over the window; gives what the user did.
    pub fn show(&mut self, ctx: &egui::Context, scheme: &Scheme) -> GalleryOutcome {
        let pages = self.pages();
        self.page = self.page.min(pages.saturating_sub(1));
        if ctx.memory(|m| m.focused().is_none()) {
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
                self.go(self.page + 1);
            }
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
                self.go(self.page.saturating_sub(1));
            }
        }
        let screen = ctx.content_rect();
        let width = WIDTH.min(screen.width() - 2.0 * MARGIN).max(200.0);
        let body = (screen.height() - 2.0 * MARGIN - CHROME).max(MIN_PICTURE);
        let title = tf("gal_titel", &[("0", Arg::from(self.version.as_str()))]);
        let highlight = self.highlights.get(self.page).copied();
        let texture = highlight.and_then(|h| self.texture(ctx, h.picture));
        let mut action = Action::None;
        let (_, close) = widgets::dialog_wide(ctx, scheme, "galerie", width, |ui| {
            let inner = ui.available_width();
            ui.set_width(inner);
            widgets::heading(ui, scheme, &title);
            ui.add_space(8.0);
            ui.allocate_ui_with_layout(vec2(inner, body), Layout::top_down(Align::Min), |ui| {
                ui.set_min_size(vec2(inner, body));
                let area = ScrollArea::vertical()
                    .id_salt("galerie-inhalt")
                    .max_height(body)
                    .min_scrolled_height(body)
                    .auto_shrink([false, true]);
                widgets::scroll_area(ui, scheme, area, |ui| match highlight {
                    Some(point) => {
                        let room = (body - self.text_height - SLACK).max(MIN_PICTURE);
                        self.text_height = highlight_page(ui, scheme, &point, texture.as_ref(), room);
                    }
                    None => {
                        let rest = self.rest().to_vec();
                        rest_page(ui, scheme, &rest);
                    }
                });
            });
            ui.add_space(8.0);
            action = foot(ui, scheme, self.page, pages, highlight.map(|h| h.target));
        });
        match action {
            Action::Go(page) => self.go(page),
            Action::Look(target) => return GalleryOutcome { closed: true, open: Some(target) },
            Action::Close => return GalleryOutcome { closed: true, open: None },
            Action::None => {}
        }
        GalleryOutcome { closed: close, open: None }
    }
}

/// What a click in the foot asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    None,
    Go(usize),
    Look(Target),
    Close,
}

/// The colour of a kind.
fn kind_color(scheme: &Scheme, kind: Kind) -> Color {
    match kind {
        Kind::New => scheme.accent,
        Kind::Better => scheme.blue,
        Kind::Fix => scheme.gold,
    }
}

/// The key of the word of a kind.
fn kind_key(kind: Kind) -> &'static str {
    match kind {
        Kind::New => "gal_art_neu",
        Kind::Better => "gal_art_bess",
        Kind::Fix => "gal_art_fix",
    }
}

/// One highlight: the picture at most `room` high (and as wide as the dialog allows, in its proportions), the
/// kind, the title, the text and the way. Returns the height the texts under the picture took.
fn highlight_page(ui: &mut Ui, scheme: &Scheme, point: &Highlight, texture: Option<&TextureHandle>, room: f32) -> f32 {
    let width = ui.available_width();
    if let Some(texture) = texture {
        let size = texture.size_vec2();
        let scale = (width / size.x).min(room / size.y).min(1.0);
        let shown = size * scale;
        ui.vertical_centered(|ui| {
            let image = egui::Image::from_texture(egui::load::SizedTexture::new(texture.id(), shown))
                .fit_to_exact_size(shown)
                .alt_text(t(point.title));
            ui.add(image);
        });
    }
    let top = ui.cursor().top();
    ui.add_space(14.0);
    let word = t(kind_key(point.kind)).to_uppercase();
    ui.horizontal(|ui| widgets::badge(ui, scheme, kind_color(scheme, point.kind), &word));
    ui.add_space(4.0);
    let heading = widgets::bold(ui, t(point.title), TextStyle::Heading).color(color(scheme.fg));
    ui.add(egui::Label::new(heading).wrap());
    ui.add_space(6.0);
    ui.add(egui::Label::new(RichText::new(t(point.text)).color(color(scheme.fg))).wrap());
    ui.add_space(8.0);
    let way = widgets::bold(ui, t(point.way), TextStyle::Body).color(color(scheme.gold));
    ui.add(egui::Label::new(way).wrap());
    ui.cursor().top() - top
}

/// The other points of the series, in two columns, each with the mark of its kind.
fn rest_page(ui: &mut Ui, scheme: &Scheme, rest: &[(Kind, String)]) {
    ui.add(egui::Label::new(widgets::bold(ui, t("gal_weitere"), TextStyle::Heading).color(color(scheme.fg))).wrap());
    ui.add_space(10.0);
    let half = rest.len().div_ceil(2);
    ui.columns(2, |columns| {
        for (column, part) in columns.iter_mut().zip([&rest[..half], &rest[half..]]) {
            for (kind, title) in part {
                column.horizontal_top(|row| {
                    let word = t(kind_key(*kind));
                    widgets::badge(row, scheme, kind_color(scheme, *kind), &word);
                    row.add(egui::Label::new(changelog::clean_row(title)).wrap());
                });
                column.add_space(6.0);
            }
        }
    });
}

/// The foot: the page marks, back and next, and at the right the button to the place and the one that closes.
fn foot(ui: &mut Ui, scheme: &Scheme, page: usize, pages: usize, target: Option<Target>) -> Action {
    let mut action = Action::None;
    ui.horizontal(|ui| {
        for n in 0..pages {
            if page_mark(ui, scheme, n, n == page).clicked() {
                action = Action::Go(n);
            }
        }
        ui.add_space(6.0);
        if page > 0
            && ui.add(widgets::Button::new(&t("gal_zurueck"), ButtonKind::Normal, scheme).icon("arrow-left")).clicked()
        {
            action = Action::Go(page - 1);
        }
        if page + 1 < pages
            && ui.add(widgets::Button::new(&t("gal_weiter"), ButtonKind::Normal, scheme).icon("arrow-right")).clicked()
        {
            action = Action::Go(page + 1);
        }
        let (try_word, look_word) = (t("gal_ausprobieren"), t("gal_ansehen"));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.add(widgets::Button::new(&t("gal_schliessen"), ButtonKind::Strong, scheme)).clicked() {
                action = Action::Close;
            }
            if let Some(target) = target {
                let button = if target == Target::CardView {
                    widgets::Button::new(&try_word, ButtonKind::Strong, scheme)
                } else {
                    widgets::Button::new(&look_word, ButtonKind::Normal, scheme).icon("eye")
                };
                if ui.add(button).clicked() {
                    action = Action::Look(target);
                }
            }
        });
    });
    action
}

/// The bar of page `n`: in the accent while it is shown, a click goes there.
fn page_mark(ui: &mut Ui, scheme: &Scheme, n: usize, on: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(MARK.x, MARK_HIT), Sense::click());
    let bar = egui::Rect::from_center_size(rect.center(), MARK);
    let tone = if on {
        scheme.accent
    } else if response.hovered() {
        scheme.hover_fg()
    } else {
        scheme.line
    };
    ui.painter().rect_filled(bar, 0.0, color(tone));
    let name = (n + 1).to_string();
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, &name));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Pos2, Rect, Shape};
    use versekit_core::language::{Choice, set_thread_language};
    use versekit_core::theme;

    /// One text of a frame: what it says, where it stands, the rectangle it is clipped to and whether it was cut
    /// short with dots.
    #[derive(Clone, Debug)]
    struct Shown {
        text: String,
        rect: Rect,
        clip: Rect,
        elided: bool,
    }

    /// What a frame drew and gave.
    struct Frame {
        texts: Vec<Shown>,
        /// The textures meshes were painted with.
        textures: Vec<egui::TextureId>,
        /// The bars of the page marks.
        marks: Vec<Rect>,
        /// The rectangles painted with a texture, with their clip.
        pictures: Vec<(Rect, Rect)>,
        screen: Rect,
    }

    impl Frame {
        fn shows(&self, text: &str) -> bool {
            self.texts.iter().any(|s| s.text == text)
        }

        fn at(&self, text: &str) -> Option<Pos2> {
            self.texts.iter().find(|s| s.text == text).map(|s| s.rect.center())
        }
    }

    /// The gallery in a window of its own without a screen, drawn frame by frame.
    struct Bench {
        ctx: egui::Context,
        state: GalleryState,
        screen: egui::Vec2,
        outcomes: Vec<GalleryOutcome>,
    }

    fn scheme() -> &'static Scheme {
        theme::by_id("original").expect("original")
    }

    impl Bench {
        fn new(version: &str) -> Bench {
            let ctx = egui::Context::default();
            crate::install(&ctx);
            let state = GalleryState::new(version, Vec::new()).expect("highlights");
            Bench { ctx, state, screen: vec2(1280.0, 820.0), outcomes: Vec::new() }
        }

        /// The window at the size step `points`: the same window holds fewer points.
        fn sized(mut self, points: i64) -> Bench {
            crate::apply_size(&self.ctx, points);
            self.screen = vec2(1280.0, 820.0) / versekit_core::font_size::zoom_factor(points);
            self
        }

        fn frame(&mut self, events: Vec<Event>) -> Frame {
            let screen = Rect::from_min_size(Pos2::ZERO, self.screen);
            let input = egui::RawInput { screen_rect: Some(screen), events, ..Default::default() };
            let state = &mut self.state;
            let mut outcome = GalleryOutcome::default();
            let mut output = self.ctx.run_ui(input, |ui| outcome = state.show(ui.ctx(), scheme()));
            output.textures_delta.clear();
            let mut frame =
                Frame { texts: Vec::new(), textures: Vec::new(), marks: Vec::new(), pictures: Vec::new(), screen };
            fn collect(shape: &Shape, clip: Rect, frame: &mut Frame) {
                match shape {
                    Shape::Rect(rect) if (rect.rect.size() - MARK).length() < 0.01 => frame.marks.push(rect.rect),
                    Shape::Rect(rect) if rect.fill_texture_id() != egui::TextureId::default() => {
                        frame.textures.push(rect.fill_texture_id());
                        frame.pictures.push((rect.rect, clip));
                    }
                    Shape::Text(text) => frame.texts.push(Shown {
                        text: text.galley.text().to_string(),
                        rect: text.galley.rect.translate(text.pos.to_vec2()),
                        clip,
                        elided: text.galley.elided,
                    }),
                    Shape::Mesh(mesh) => frame.textures.push(mesh.texture_id),
                    Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, clip, frame)),
                    _ => {}
                }
            }
            output.shapes.iter().for_each(|clipped| collect(&clipped.shape, clipped.clip_rect, &mut frame));
            if outcome != GalleryOutcome::default() {
                self.outcomes.push(outcome);
            }
            frame
        }

        fn settle(&mut self) -> Frame {
            let mut last = self.frame(Vec::new());
            for _ in 0..4 {
                last = self.frame(Vec::new());
            }
            last
        }

        fn click(&mut self, label: &str) {
            let shown = self.settle();
            let at = shown.at(label).unwrap_or_else(|| {
                panic!("{label} is not on the screen: {:?}", shown.texts.iter().map(|s| &s.text).collect::<Vec<_>>())
            });
            let button = |pressed| Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            self.frame(vec![Event::PointerMoved(at)]);
            self.frame(vec![button(true)]);
            self.frame(vec![button(false)]);
            self.frame(Vec::new());
        }

        fn key(&mut self, key: egui::Key) {
            self.settle();
            let event = |pressed| Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            };
            self.frame(vec![event(true)]);
            self.frame(vec![event(false)]);
        }
    }

    fn label(text: &str, kind: ButtonKind) -> String {
        widgets::button_label(scheme(), kind, text)
    }

    #[test]
    fn a_series_without_highlights_has_no_gallery() {
        assert!(GalleryState::new("4.1.0", Vec::new()).is_none());
        assert!(GalleryState::new("5.1.0", Vec::new()).is_none());
        let state = GalleryState::new("v4.0.2", Vec::new()).expect("4.0 has highlights");
        assert_eq!(state.version(), "4.0.2");
    }

    /// The gallery of 5.0 comes for a test build too, titled with the version without its mark, and each page shows
    /// its own picture of 700 × 430 in the language of the page.
    #[test]
    fn the_gallery_of_5_0_shows_its_five_highlights_with_their_pictures() {
        let _language = crate::language_lock();
        for (lang, choice) in [(Lang::De, Choice::De), (Lang::En, Choice::En)] {
            set_thread_language(choice);
            let mut bench = Bench::new("5.0.0-dev");
            assert_eq!(bench.state.version(), "5.0.0");
            assert_eq!(bench.state.pages(), 6, "five highlights and the list of the rest");
            for (page, point) in gallery::highlights("5.0.0").iter().enumerate() {
                bench.state.page = page;
                let shown = bench.settle();
                assert!(shown.shows(&t(point.title)), "{lang:?} {}", point.title);
                let picture = bench.state.pictures.last().expect("a picture");
                assert_eq!((picture.name, picture.lang), (point.picture, lang));
                let texture = picture.texture.clone().expect("the picture is loaded");
                assert_eq!(texture.size(), [700, 430], "{}", point.picture);
                assert!(shown.textures.contains(&texture.id()), "{lang:?} {}", point.picture);
            }
        }
        set_thread_language(Choice::De);
    }

    #[test]
    fn a_page_shows_picture_kind_title_text_and_way() {
        let _language = crate::language_lock();
        set_thread_language(Choice::De);
        let mut bench = Bench::new("4.0.1");
        let shown = bench.settle();
        let point = gallery::highlights("4.0.1")[0];
        for text in [
            tf("gal_titel", &[("0", Arg::from("4.0.1"))]),
            t("gal_art_neu").to_uppercase(),
            t(point.title),
            t(point.text),
            t(point.way),
        ] {
            assert!(
                shown.shows(&text),
                "{text} missing: {:?}",
                shown.texts.iter().map(|s| &s.text).collect::<Vec<_>>()
            );
        }
        let texture = bench.state.pictures.iter().find_map(|p| p.texture.clone()).expect("the picture is loaded");
        assert_eq!(texture.size(), [700, 430]);
        assert!(shown.textures.contains(&texture.id()), "the picture is painted");
        // The card view is tried, not looked at; the first page has no way back.
        assert!(shown.shows(&label(&t("gal_ausprobieren"), ButtonKind::Strong)));
        assert!(!shown.shows(&label(&t("gal_ansehen"), ButtonKind::Normal)));
        assert!(!shown.shows(&t("gal_zurueck")));
        assert!(shown.shows(&t("gal_weiter")));
    }

    #[test]
    fn next_and_back_page_through_and_the_last_page_lists_the_rest() {
        let _language = crate::language_lock();
        set_thread_language(Choice::De);
        let mut bench = Bench::new("3.97.0");
        let points = gallery::highlights("3.97.0");
        let pages = bench.state.pages();
        assert_eq!(pages, points.len() + 1, "the list of the other points is the last page");
        for (n, point) in points.iter().enumerate() {
            let shown = bench.settle();
            assert_eq!(bench.state.page(), n);
            assert!(shown.shows(&t(point.title)), "page {n}");
            bench.click(&t("gal_weiter"));
        }
        let last = bench.settle();
        assert_eq!(bench.state.page(), pages - 1);
        assert!(last.shows(&t("gal_weitere")));
        assert!(!last.shows(&t("gal_weiter")), "no page after the last");
        let rest = gallery::rest_built_in("3.97.0", Lang::De);
        assert!(last.shows(&changelog::clean_row(&rest[0].1)), "{:?}", rest[0]);
        assert!(
            !last.texts.iter().any(|s| s.text.starts_with("Bedarf der Einheit")),
            "a highlight is not listed again"
        );
        bench.click(&t("gal_zurueck"));
        assert_eq!(bench.state.page(), pages - 2);
        assert!(bench.settle().shows(&t(points[pages - 2].title)));
        assert!(bench.outcomes.is_empty(), "paging closes nothing: {:?}", bench.outcomes);
    }

    #[test]
    fn the_arrow_keys_and_the_page_marks_page_through() {
        let _language = crate::language_lock();
        set_thread_language(Choice::De);
        let mut bench = Bench::new("4.0.0");
        bench.key(egui::Key::ArrowRight);
        assert_eq!(bench.state.page(), 1);
        bench.key(egui::Key::ArrowLeft);
        bench.key(egui::Key::ArrowLeft);
        assert_eq!(bench.state.page(), 0, "not before the first");
        let shown = bench.settle();
        assert_eq!(shown.marks.len(), bench.state.pages(), "one mark for each page");
        let mut marks = shown.marks.clone();
        marks.sort_by(|a, b| a.left().total_cmp(&b.left()));
        let at = marks[2].center();
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        bench.frame(vec![Event::PointerMoved(at)]);
        bench.frame(vec![button(true)]);
        bench.frame(vec![button(false)]);
        bench.settle();
        assert_eq!(bench.state.page(), 2);
        assert!(bench.outcomes.is_empty(), "{:?}", bench.outcomes);
    }

    #[test]
    fn look_gives_the_place_and_closes() {
        let _language = crate::language_lock();
        set_thread_language(Choice::De);
        let mut bench = Bench::new("3.97.1");
        bench.click(&t("gal_weiter"));
        bench.click(&label(&t("gal_ansehen"), ButtonKind::Normal));
        assert_eq!(bench.outcomes, vec![GalleryOutcome { closed: true, open: Some(Target::Page("hf_bedarf")) }]);
        let mut cards = Bench::new("4.0.0");
        cards.click(&label(&t("gal_ausprobieren"), ButtonKind::Strong));
        assert_eq!(cards.outcomes, vec![GalleryOutcome { closed: true, open: Some(Target::CardView) }]);
    }

    #[test]
    fn close_and_escape_close_without_a_place() {
        let _language = crate::language_lock();
        set_thread_language(Choice::De);
        let mut bench = Bench::new("4.0.0");
        bench.click(&label(&t("gal_schliessen"), ButtonKind::Strong));
        assert_eq!(bench.outcomes, vec![GalleryOutcome { closed: true, open: None }]);
        let mut escaped = Bench::new("4.0.0");
        escaped.key(egui::Key::Escape);
        assert!(!escaped.outcomes.is_empty(), "Esc closes");
        assert!(escaped.outcomes.iter().all(|o| o.closed && o.open.is_none()), "{:?}", escaped.outcomes);
    }

    #[test]
    fn english_takes_the_english_picture_and_texts() {
        let _language = crate::language_lock();
        for (name, _, _) in PICTURES {
            assert_ne!(picture_bytes(name, Lang::En), picture_bytes(name, Lang::De), "{name}");
        }
        assert!(picture_bytes("nichts", Lang::De).is_none());
        set_thread_language(Choice::En);
        let mut bench = Bench::new("4.0.0");
        let shown = bench.settle();
        let point = gallery::highlights("4.0.0")[0];
        assert!(shown.shows(&t(point.title)) && t(point.title).starts_with("New:"), "{}", t(point.title));
        let picture = bench.state.pictures.first().expect("loaded");
        assert_eq!((picture.name, picture.lang), ("karten", Lang::En));
        assert_eq!(picture.texture.as_ref().map(|t| t.name()), Some("galerie-karten-en".to_string()));
        // Switching the language takes the German picture and the German list.
        set_thread_language(Choice::De);
        let again = bench.settle();
        assert!(again.shows(&t(point.title)));
        assert!(bench.state.pictures.iter().any(|p| p.lang == Lang::De && p.name == "karten"));
        set_thread_language(Choice::De);
    }

    /// What is cut on a page: a text cut short with dots, a text that does not stand wholly within its clip and
    /// the window (on the list only the width counts there: it scrolls), and the buttons of the foot missing.
    fn cut(frame: &Frame, list: bool, buttons: &[String]) -> Vec<String> {
        let mut found = Vec::new();
        for shown in &frame.texts {
            let inside = shown.clip.intersect(frame.screen).expand(0.5);
            let whole = if list {
                shown.rect.left() >= inside.left() && shown.rect.right() <= inside.right()
            } else {
                inside.contains_rect(shown.rect)
            };
            if shown.elided || !whole {
                found.push(format!(
                    "{:?} at {:?} clip {:?} elided {}",
                    shown.text, shown.rect, shown.clip, shown.elided
                ));
            }
        }
        for (rect, clip) in &frame.pictures {
            if !list && !clip.intersect(frame.screen).expand(0.5).contains_rect(*rect) {
                found.push(format!("picture at {rect:?} clip {clip:?}"));
            }
        }
        for button in buttons {
            let Some(shown) = frame.texts.iter().find(|s| &s.text == button) else {
                found.push(format!("button {button:?} missing"));
                continue;
            };
            if !shown.clip.intersect(frame.screen).expand(0.5).contains_rect(shown.rect) {
                found.push(format!("button {button:?} cut"));
            }
        }
        found
    }

    #[test]
    fn nothing_is_cut_at_1280_by_820_in_both_languages_and_the_size_steps() {
        let _language = crate::language_lock();
        let mut findings = Vec::new();
        for lang in [Choice::De, Choice::En] {
            set_thread_language(lang);
            for (points, _) in versekit_core::font_size::LEVELS.iter().map(|(name, p)| (*p, *name)) {
                for version in ["3.97.0", "4.0.0", "5.0.0"] {
                    let mut bench = Bench::new(version).sized(points);
                    let pages = bench.state.pages();
                    for page in 0..pages {
                        bench.state.page = page;
                        let frame = bench.settle();
                        let list = page >= gallery::highlights(version).len();
                        let mut buttons = vec![label(&t("gal_schliessen"), ButtonKind::Strong)];
                        if page > 0 {
                            buttons.push(t("gal_zurueck"));
                        }
                        if page + 1 < pages {
                            buttons.push(t("gal_weiter"));
                        }
                        for problem in cut(&frame, list, &buttons) {
                            findings.push(format!("{lang:?} size {points} {version} page {page}: {problem}"));
                        }
                        if !list {
                            let picture = bench.state.pictures.last().and_then(|p| p.texture.clone()).expect("picture");
                            assert!(frame.textures.contains(&picture.id()), "{lang:?} {points} {version} {page}");
                        }
                    }
                }
            }
        }
        set_thread_language(Choice::De);
        assert!(findings.is_empty(), "{}", findings.join("\n"));
    }

    #[test]
    fn the_cut_check_finds_a_text_outside_and_one_cut_short() {
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0));
        let text = |rect: Rect, elided| Shown { text: "a".into(), rect, clip: screen, elided };
        let fine = Rect::from_min_size(Pos2::new(10.0, 10.0), vec2(20.0, 10.0));
        let below = Rect::from_min_size(Pos2::new(10.0, 95.0), vec2(20.0, 10.0));
        let right = Rect::from_min_size(Pos2::new(90.0, 10.0), vec2(20.0, 10.0));
        let frame = |texts| Frame { texts, textures: Vec::new(), marks: Vec::new(), pictures: Vec::new(), screen };
        assert!(cut(&frame(vec![text(fine, false)]), false, &["a".into()]).is_empty());
        assert_eq!(cut(&frame(vec![text(fine, true)]), false, &[]).len(), 1, "cut short");
        assert_eq!(cut(&frame(vec![text(below, false)]), false, &[]).len(), 1, "below the window");
        assert!(cut(&frame(vec![text(below, false)]), true, &[]).is_empty(), "the list scrolls");
        assert_eq!(cut(&frame(vec![text(right, false)]), true, &[]).len(), 1, "beside the window");
        assert_eq!(cut(&frame(vec![]), false, &["b".into()]).len(), 1, "button missing");
        let mut picture = frame(vec![]);
        picture.pictures.push((below, screen));
        assert_eq!(cut(&picture, false, &[]).len(), 1, "picture below the window");
    }
}
