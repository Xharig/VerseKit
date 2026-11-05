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

//! The window's side of the docking point (see [`crate::extension`]): the
//! fixed entry in the sidebar, and the carrying out of what the pages of
//! an extension ask for.

use versekit_core::paths::Paths;
use versekit_core::theme::Scheme;
use versekit_core::{language, theme};

use super::{SamplePage, search_form};
use crate::extension::{Command, Extension, Slot};
use crate::org_stock_page::{self, Link};
use crate::{contract_log, widgets};

/// The share of the sidebar's height the pages of an extension may take before they scroll.
const SUB_LIST_SHARE: f32 = 0.4;

impl SamplePage {
    /// Adds the pages of an extension: one fixed entry under the overview.
    pub fn set_extension(&mut self, extension: Box<dyn Extension>) {
        debug_assert!(
            extension
                .pages()
                .iter()
                .all(|(key, _)| !super::NAV.iter().any(|(_, pages)| pages.iter().any(|(own, _)| own == key))),
            "a page of the extension has the key of a page of the window"
        );
        self.extension = Some(Slot::new(extension));
    }

    /// Opens a page by its key.
    pub fn go_to(&mut self, page: &str) {
        self.page = page.to_string();
    }

    /// The key of the open page (empty on the overview).
    pub fn page_key(&self) -> &str {
        &self.page
    }

    /// The window reads the game logs of other folders (`paths`) and writes its settings under
    /// them from now on; the settings and lists it already shows stay as they are. A window
    /// without data of its own is left as it is.
    pub fn use_paths(&mut self, paths: Paths) {
        if self.store.is_none() {
            return;
        }
        self.contracts = contract_log::LogData::load(&paths);
        self.contracts.rescan(None);
        self.stats = crate::statistics_page::StatsState::of(&paths);
        // The sync reads and writes its files in the folder of the window: it follows. A connection
        // an embedding program gave the storage stays as it is.
        if !self.storage_given {
            match self.real_basetool.clone() {
                // A tool connected the real Basetool under its own name: it stays so, bookings lock included.
                Some((keyname, book_lock)) => {
                    self.connect_system_as(&paths, Some(&keyname));
                    self.org_stock.lock_bookings(book_lock);
                }
                None => self.connect_system(&paths),
            }
        }
        if let Some(store) = &mut self.store {
            store.paths = paths;
        }
    }

    /// The storage page works through this connection to the Basetool. A made-up Basetool
    /// takes the connection page along: next to it the page must not reach the real one.
    pub fn use_storage_link(&mut self, link: Link) {
        self.storage_given = true;
        // A lock of the bookings stays with a real link; the made-up Basetool has none.
        let lock = self.org_stock.bookings_locked().map(str::to_string).filter(|_| !link.demo);
        self.real_basetool = None;
        if link.demo && self.real() {
            self.connection =
                crate::basetool_page::ConnectionState::new(Some(crate::basetool_page::Wire::preview(true)));
        }
        let paths = self.store.as_ref().map(|store| &store.paths);
        self.org_stock = org_stock_page::OrgStockState::new(Some(link), paths);
        self.org_stock.lock_bookings(lock);
    }

    /// The connection page and the storage page go to the real Basetool, as in the window of a
    /// user. The credentials go to the system's keychain under `keyname`. With `book_lock` nothing
    /// is booked: the storage page is read only and says why. A window without data of its own
    /// is left as it is.
    pub fn use_real_basetool(&mut self, keyname: &str, book_lock: Option<String>) {
        let Some(store) = &self.store else { return };
        let paths = store.paths.working_copy();
        self.connect_system_as(&paths, Some(keyname));
        // The page is there when the sync says so, not because a tool gave it a link.
        self.storage_given = false;
        self.org_stock.lock_bookings(book_lock.clone());
        // Remembered: a later change of the folders connects again the same way.
        self.real_basetool = Some((keyname.to_string(), book_lock));
    }

    /// Start and end of a frame for the extension; at the end what its pages asked for is done.
    pub(super) fn extension_frame(&mut self, ui: &egui::Ui, begin: bool) {
        let page = self.page.clone();
        let Some(slot) = &mut self.extension else { return };
        slot.extension.frame(ui, &page, begin);
        if !begin {
            let commands = slot.extension.take_commands();
            self.apply_commands(ui.ctx(), commands);
        }
    }

    /// The fixed entry under the overview and, below it, the pages of the extension. The
    /// menu filter never hides the entry; it only narrows the pages shown under it.
    ///
    /// Returns whether the filter found a page of the extension.
    pub(super) fn extension_entry(&mut self, nav: &mut egui::Ui, scheme: &Scheme, filter: &str) -> bool {
        let Some(slot) = &mut self.extension else { return false };
        let filter = search_form(filter.trim());
        let filtering = !filter.is_empty();
        let name = slot.extension.name();
        // A page chosen from outside (a tile, a command, the history) opens the pages.
        if self.extension_page != self.page {
            if slot.has(&self.page) {
                slot.open = true;
            }
            self.extension_page.clone_from(&self.page);
        }
        let matches = |text: &str| search_form(text).contains(&filter);
        let by_name = matches(&name);
        let shown: Vec<(&'static str, &'static str)> = slot
            .extension
            .pages()
            .iter()
            .copied()
            .filter(|(key, _)| {
                !filtering || by_name || matches(key) || slot.extension.infos(key).iter().any(|info| matches(info))
            })
            .collect();
        let open = if filtering { !shown.is_empty() } else { slot.open };
        let found = filtering && !shown.is_empty();
        // Where the open page is, when its list is folded away.
        let holds_page = slot.has(&self.page) && !open;
        if widgets::nav_picture_entry(nav, scheme, slot.picture(), &name, holds_page, open).clicked() && !filtering {
            slot.open = !slot.open;
        }
        if open {
            // The list never grows past a share of the sidebar: more pages scroll inside it.
            let room = (nav.available_height() * SUB_LIST_SHARE).max(widgets::NAV_ROW_HEIGHT * 3.0);
            let area =
                egui::ScrollArea::vertical().id_salt("extension-pages").max_height(room).auto_shrink([false, true]);
            widgets::scroll_area(nav, scheme, area, |nav| {
                for (key, icon) in shown {
                    if widgets::nav_sub_entry(nav, scheme, icon, key, self.page == key).clicked() {
                        self.page = key.to_string();
                    }
                }
            });
        }
        nav.add_space(2.0);
        found
    }

    /// Does what the pages of the extension asked for.
    pub(super) fn apply_commands(&mut self, ctx: &egui::Context, commands: Vec<Command>) {
        // What a command changed shows in the next frame.
        if !commands.is_empty() {
            ctx.request_repaint();
        }
        for command in commands {
            match command {
                Command::Restart(args) => self.restart(ctx, &args),
                Command::UsePaths(paths) => self.use_paths(*paths),
                Command::UseStorageLink(link) => self.use_storage_link(*link),
                Command::UseRealBasetool { keyname, book_lock } => self.use_real_basetool(&keyname, book_lock),
                Command::SetScheme(id) => {
                    theme::set_current(id);
                    ctx.request_repaint();
                }
                Command::SetSize(points) => {
                    self.size = points;
                    ctx.request_repaint();
                }
                Command::SetEnglish(english) => {
                    self.general_english = english;
                    language::set_language(if english { language::Choice::En } else { language::Choice::De });
                    ctx.request_repaint();
                }
                Command::ShowUpdateBar(on) => {
                    self.update_loaded = !on;
                    self.loading = None;
                }
                Command::SetUpdateVersion(version) => self.update_version = version,
                Command::OpenWizard => self.open_setup(),
                Command::ResetNewMark => self.forget_seen(),
                Command::SetTestBuild(on) => self.test_build = on,
                Command::SetErrorCount(count) => self.error_count = count,
                Command::Rescan => self.start_contract_scan(),
                Command::OpenPage(page) => self.page = page,
                Command::SetOverlayPopup { on, seconds } => {
                    self.overlay_popup = on;
                    self.overlay_seconds = seconds;
                }
                Command::ShowTextKeys(on) => {
                    language::set_show_keys(on);
                    ctx.request_repaint();
                }
                Command::Notice(text) => self.notify(ctx, &text),
            }
        }
    }

    fn notify(&mut self, ctx: &egui::Context, text: &str) {
        self.pin_notice = Some((ctx.input(|i| i.time), text.to_string()));
    }

    /// Starts this program again with `args` and closes this window at once. The arguments
    /// are not checked: the extension is trusted code of the same program.
    fn restart(&mut self, ctx: &egui::Context, args: &[String]) {
        self.save_and_wait();
        let started = std::env::current_exe().and_then(|exe| std::process::Command::new(exe).args(args).spawn());
        match started {
            Ok(_) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Err(error) => {
                let text = versekit_core::language::tf(
                    "s_bs_neustart_nein",
                    &[("0", versekit_core::language::Arg::from(error.to_string()))],
                );
                self.notify(ctx, &text);
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use egui::{Pos2, Shape};
    use versekit_core::font_size;

    use super::*;
    use crate::extension::Ctx;

    /// What the mock extension was asked, and what it will ask the window for.
    #[derive(Default)]
    struct Log {
        shown: Vec<String>,
        frames: (u32, u32),
        commands: Vec<Command>,
    }

    struct Mock(Rc<RefCell<Log>>);

    /// A PNG of a few pixels, as the picture of the mock.
    fn picture_bytes() -> &'static [u8] {
        static BYTES: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
        BYTES.get_or_init(|| {
            let mut bytes = Vec::new();
            let image = image::RgbaImage::from_pixel(4, 4, image::Rgba([156, 228, 48, 255]));
            image
                .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
                .expect("a PNG of four pixels");
            bytes
        })
    }

    impl Extension for Mock {
        fn name(&self) -> String {
            "Beispiel-Erweiterung".to_string()
        }

        fn picture(&self) -> &'static [u8] {
            picture_bytes()
        }

        fn pages(&self) -> &'static [(&'static str, &'static str)] {
            &[("Erste Seite", "database"), ("Zweite Seite", "gauge")]
        }

        fn infos(&self, page: &str) -> &'static [&'static str] {
            if page == "Zweite Seite" { &["Spezialwort"] } else { &[] }
        }

        fn show(&mut self, ui: &mut egui::Ui, page: &str, _ctx: &Ctx<'_>) {
            self.0.borrow_mut().shown.push(page.to_string());
            ui.label(format!("Inhalt von {page}"));
        }

        fn frame(&mut self, _ui: &egui::Ui, _page: &str, begin: bool) {
            let mut log = self.0.borrow_mut();
            if begin {
                log.frames.0 += 1;
            } else {
                log.frames.1 += 1;
            }
        }

        fn take_commands(&mut self) -> Vec<Command> {
            std::mem::take(&mut self.0.borrow_mut().commands)
        }
    }

    /// An extension with far more pages than the sidebar has room for.
    struct Many;

    const MANY: [(&str, &str); 15] = [
        ("Seite 01", "database"),
        ("Seite 02", "database"),
        ("Seite 03", "database"),
        ("Seite 04", "database"),
        ("Seite 05", "database"),
        ("Seite 06", "database"),
        ("Seite 07", "database"),
        ("Seite 08", "database"),
        ("Seite 09", "database"),
        ("Seite 10", "database"),
        ("Seite 11", "database"),
        ("Seite 12", "database"),
        ("Seite 13", "database"),
        ("Seite 14", "database"),
        ("Seite 15", "database"),
    ];

    impl Extension for Many {
        fn name(&self) -> String {
            "Viele Seiten".to_string()
        }

        fn picture(&self) -> &'static [u8] {
            picture_bytes()
        }

        fn pages(&self) -> &'static [(&'static str, &'static str)] {
            &MANY
        }

        fn show(&mut self, ui: &mut egui::Ui, page: &str, _ctx: &Ctx<'_>) {
            ui.label(page.to_string());
        }

        fn take_commands(&mut self) -> Vec<Command> {
            Vec::new()
        }
    }

    fn window(extension: bool) -> (SamplePage, Rc<RefCell<Log>>) {
        let log = Rc::new(RefCell::new(Log::default()));
        let mut page = SamplePage::preview();
        if extension {
            page.set_extension(Box::new(Mock(log.clone())));
        }
        (page, log)
    }

    thread_local! {
        /// The one context of a test: what is open (a list) stays open from one draw to the next.
        static CTX: egui::Context = {
            let ctx = egui::Context::default();
            crate::install(&ctx);
            ctx
        };
    }

    /// One frame of `page` with `events`; returns the overflow and every text drawn with its place.
    fn frame(
        ctx: &egui::Context,
        page: &mut SamplePage,
        (width, height): (f32, f32),
        events: Vec<egui::Event>,
    ) -> (f32, Vec<(String, Pos2)>) {
        fn collect(shape: &Shape, into: &mut Vec<(String, Pos2)>) {
            match shape {
                Shape::Text(text) => into.push((text.galley.text().to_string(), text.pos)),
                Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, into)),
                _ => {}
            }
        }
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(width, height))),
            events,
            ..Default::default()
        };
        let (mut overflow, mut texts) = (0.0, Vec::new());
        let mut output = ctx.run_ui(input, |ui| overflow = page.show(ui));
        output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
        output.textures_delta.clear();
        (overflow, texts)
    }

    /// Draws `page` for a few frames; returns the overflow and every text drawn with its place.
    pub(crate) fn draw(page: &mut SamplePage, width: f32, height: f32) -> (f32, Vec<(String, Pos2)>) {
        CTX.with(|ctx| {
            let mut last = (0.0, Vec::new());
            for _ in 0..3 {
                last = frame(ctx, page, (width, height), Vec::new());
            }
            last
        })
    }

    /// Draws `page` until its layout has settled and gives the places of the cards of the last frame. The scroll
    /// area takes the room of its bar after the first frame in which the page turned out to be taller than the
    /// window, and whatever takes its width from the page below (the head of a page) follows a frame later; a page
    /// that reads in a thread of its own changes its layout when the answer comes, which can take more frames on a
    /// slow build. So at least six frames, then on until two frames in a row put every card in the same place
    /// (at most [`SETTLE_FRAMES`]).
    pub(crate) fn draw_cards(page: &mut SamplePage, width: f32, height: f32) -> Vec<egui::Rect> {
        CTX.with(|ctx| {
            for _ in 0..6 {
                frame(ctx, page, (width, height), Vec::new());
            }
            crate::widgets::drawn_cards::take();
            frame(ctx, page, (width, height), Vec::new());
            let mut cards = crate::widgets::drawn_cards::take();
            for _ in 0..SETTLE_FRAMES {
                frame(ctx, page, (width, height), Vec::new());
                let again = crate::widgets::drawn_cards::take();
                if again == cards {
                    break;
                }
                cards = again;
            }
            cards
        })
    }

    /// The most frames [`draw_cards`] waits for the cards to stand still.
    pub(crate) const SETTLE_FRAMES: usize = 40;

    /// The texts of a frame with their places.
    pub(crate) type Texts = Vec<(String, Pos2)>;

    /// Draws `page`, then turns the mouse wheel over it until it has been scrolled to the end; returns the texts
    /// drawn at the end and the texts drawn at the start.
    pub(crate) fn draw_scrolled_to_end(page: &mut SamplePage, width: f32, height: f32) -> (Texts, Texts) {
        let size = (width, height);
        CTX.with(|ctx| {
            let first = draw(page, width, height).1;
            let over = Pos2::new(width * 0.7, height * 0.6);
            frame(ctx, page, size, vec![egui::Event::PointerMoved(over)]);
            let mut texts = first.clone();
            for _ in 0..80 {
                let wheel = egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -400.0),
                    modifiers: egui::Modifiers::NONE,
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Wheel,
                };
                frame(ctx, page, size, vec![wheel]);
                texts = frame(ctx, page, size, Vec::new()).1;
            }
            (texts, first)
        })
    }

    /// Draws `page`, then clicks on the text `clicks[0]`, draws on and clicks `clicks[1]`, and so
    /// on; returns the texts drawn after each click. A text that is not drawn fails the test.
    pub(crate) fn draw_clicking(
        page: &mut SamplePage,
        width: f32,
        height: f32,
        clicks: &[&str],
    ) -> Vec<Vec<(String, Pos2)>> {
        let size = (width, height);
        CTX.with(|ctx| {
            let mut texts = draw(page, width, height).1;
            let mut after = Vec::new();
            for target in clicks {
                let at = place(&texts, target).unwrap_or_else(|| panic!("`{target}` is not drawn: {texts:?}"));
                let pos = at + egui::vec2(4.0, 4.0);
                let button = |pressed| egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                };
                frame(ctx, page, size, vec![egui::Event::PointerMoved(pos)]);
                frame(ctx, page, size, vec![button(true)]);
                frame(ctx, page, size, vec![button(false)]);
                for _ in 0..3 {
                    texts = frame(ctx, page, size, Vec::new()).1;
                }
                after.push(texts.clone());
            }
            after
        })
    }

    fn place(texts: &[(String, Pos2)], text: &str) -> Option<Pos2> {
        texts.iter().find(|(shown, _)| shown == text).map(|(_, at)| *at)
    }

    #[test]
    fn the_entry_sits_under_the_overview_and_above_the_menu_filter() {
        theme::set_current("original");
        let (mut page, _) = window(true);
        let (width, height) = font_size::BASE_MIN_SIZE;
        let (_, texts) = draw(&mut page, width.max(1280.0), height.max(820.0));
        let overview = place(&texts, &language::t("hf_uebersicht")).expect("the overview is drawn");
        let entry = place(&texts, "Beispiel-Erweiterung").expect("the entry is drawn");
        let filter = place(&texts, &language::t("hf_menue_filtern")).expect("the filter hint is drawn");
        assert!(overview.y < entry.y && entry.y < filter.y, "{overview:?} {entry:?} {filter:?}");
        // Same column as the overview, so it is a row of the same list.
        assert!((overview.x - entry.x).abs() < 40.0, "{overview:?} {entry:?}");
        // Counter-check: without an extension nothing of it is there.
        let (mut plain, _) = window(false);
        let (_, texts) = draw(&mut plain, width.max(1280.0), height.max(820.0));
        assert!(place(&texts, "Beispiel-Erweiterung").is_none());
        assert!(place(&texts, &language::t("hf_menue_filtern")).is_some());
    }

    #[test]
    fn the_filter_never_hides_the_entry_but_narrows_its_pages() {
        theme::set_current("original");
        let (mut page, _) = window(true);
        let (width, height) = font_size::BASE_MIN_SIZE;
        let (width, height) = (width.max(1280.0), height.max(820.0));
        // Closed and not filtering: the pages are not listed.
        let (_, texts) = draw(&mut page, width, height);
        assert!(place(&texts, "Erste Seite").is_none());
        // Filtering for a word of the other page lists that page only, the entry stays.
        page.nav_filter = "spezialwort".to_string();
        let (_, texts) = draw(&mut page, width, height);
        assert!(place(&texts, "Beispiel-Erweiterung").is_some(), "the entry stays");
        assert!(place(&texts, "Zweite Seite").is_some(), "the page with the word is listed");
        assert!(place(&texts, "Erste Seite").is_none(), "the other page is not listed");
        assert!(place(&texts, &language::t("hf_menue_leer")).is_none(), "a hit is no empty result");
        // A word nothing matches: the entry still stays, no page.
        page.nav_filter = "gibt-es-nirgends".to_string();
        let (_, texts) = draw(&mut page, width, height);
        assert!(place(&texts, "Beispiel-Erweiterung").is_some());
        assert!(place(&texts, "Zweite Seite").is_none());
        assert!(place(&texts, &language::t("hf_menue_leer")).is_some(), "nothing found says so");
    }

    #[test]
    fn a_page_of_the_extension_is_drawn_and_opens_its_list() {
        theme::set_current("original");
        let (mut page, log) = window(true);
        let (width, height) = font_size::BASE_MIN_SIZE;
        let (width, height) = (width.max(1280.0), height.max(820.0));
        page.go_to("Zweite Seite");
        let (overflow, texts) = draw(&mut page, width, height);
        assert!(overflow < 0.5, "{overflow}");
        assert!(place(&texts, "Inhalt von Zweite Seite").is_some(), "the page is drawn");
        assert!(log.borrow().shown.iter().all(|shown| shown == "Zweite Seite") && !log.borrow().shown.is_empty());
        assert!(place(&texts, "Erste Seite").is_some(), "choosing a page lists the pages");
        // Every frame is announced at its start and its end.
        let frames = log.borrow().frames;
        assert!(frames.0 > 0 && frames.0 == frames.1, "{frames:?}");
        // The window pins no page of the extension to the overview.
        assert_eq!(page.page_key(), "Zweite Seite");
    }

    #[test]
    fn the_commands_of_the_pages_change_the_window() {
        let ctx = egui::Context::default();
        let (mut page, _) = window(true);
        page.update_loaded = true;
        page.new_pages.clear();
        page.apply_commands(
            &ctx,
            vec![
                Command::ShowUpdateBar(true),
                Command::ResetNewMark,
                Command::SetSize(3),
                Command::SetErrorCount(4),
                Command::SetOverlayPopup { on: true, seconds: 9 },
                Command::SetTestBuild(true),
                Command::SetUpdateVersion("5.0.1".into()),
                Command::OpenPage("Erste Seite".into()),
            ],
        );
        assert!(!page.update_loaded && !page.new_pages.is_empty() && page.size == 3 && page.error_count == 4);
        assert_eq!(page.update_version, "5.0.1");
        assert!(page.overlay_popup && page.overlay_seconds == 9 && page.test_build);
        assert_eq!(page.page_key(), "Erste Seite");
        // Counter-check: no command, nothing moves.
        page.apply_commands(&ctx, Vec::new());
        assert_eq!(page.size, 3);
        // Text keys: shown while on, back to the texts when off.
        page.apply_commands(&ctx, vec![Command::ShowTextKeys(true)]);
        assert_eq!(language::t("hf_uebersicht"), "[hf_uebersicht]");
        page.apply_commands(&ctx, vec![Command::ShowTextKeys(false)]);
        assert_ne!(language::t("hf_uebersicht"), "[hf_uebersicht]");
    }

    /// The six statistics pages in the window: on an empty data folder they say that nothing
    /// is recorded, not that they come later, and none shows a made-up figure; on the sample
    /// page they show the sample figures.
    #[test]
    fn the_statistics_pages_in_the_window_show_real_or_sample_figures() {
        use versekit_core::legacy::ENV_HOME;
        use versekit_core::paths::{ENV_INSTALL_DIR, Host};
        use versekit_core::pypath::Flavor;

        theme::set_thread_current("original");
        let dir =
            std::env::temp_dir().join(format!("versekit-fenster-statistik-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        let (home, game) = (dir.join("home"), dir.join("LIVE"));
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("Game.log"), "<2026-10-01T10:00:00.000Z> Log started\n").unwrap();
        let (home_text, game_text) = (home.to_string_lossy().into_owned(), game.to_string_lossy().into_owned());
        let host = Host::new(
            Flavor::native(),
            &[(ENV_HOME, &home_text), (ENV_INSTALL_DIR, &game_text)],
            dir.to_str().unwrap(),
        );
        let mut page = SamplePage::with_settings(Paths::new(host));
        // The language of this thread only: the language of the program is one for all tests side by side.
        language::set_thread_language(language::Choice::De);
        let (width, height) = (1280.0, 900.0);
        let shows = |texts: &[(String, Pos2)], part: &str| texts.iter().any(|(text, _)| text.contains(part));
        for key in [
            "hf_st_auswertung",
            "hf_st_uebersicht",
            "hf_st_schiffe",
            "hf_st_auftraege",
            "hf_st_quantum",
            "hf_st_stabil",
        ] {
            page.go_to(key);
            draw(&mut page, width, height);
            // Entering the page starts a reading of the logs: waits for its end instead of a fixed time, so that
            // the checks below look at the page as read and not at its loading or running line.
            page.stats.wait();
            let texts = draw(&mut page, width, height).1;
            for busy in ["s_sa_laedt", "s_sa_laeuft"] {
                assert!(!shows(&texts, &language::t(busy)), "{key} has read its logs: {busy}");
            }
            assert!(!shows(&texts, super::super::glance::NOT_YET), "{key} is built, it does not come later");
            assert!(!shows(&texts, "Hier steht die Seite."), "{key}");
            for sample in ["AEGS Gladius", "Pyro6", "Retake the Outpost", "128", "41 h"] {
                assert!(!shows(&texts, sample), "{key}: made-up {sample}");
            }
        }
        page.go_to("hf_st_uebersicht");
        let texts = draw(&mut page, width, height).1;
        assert!(shows(&texts, "Noch keine Sitzung gefunden"), "{:?}", texts.iter().map(|t| &t.0).collect::<Vec<_>>());
        let _ = std::fs::remove_dir_all(&dir);

        // The sample page shows its sample figures.
        let (mut sample, _) = window(true);
        sample.go_to("hf_st_schiffe");
        assert!(shows(&draw(&mut sample, width, height).1, "AEGS Gladius"));
    }

    /// The foot bar of a window that stays open looks again while it runs: the game in the process list runs,
    /// also with a quiet log; once it is gone, the end counts as soon as the log is quiet for the short time of
    /// the overlay, not the five minutes of the contract log.
    #[test]
    fn the_foot_bar_says_the_game_ended_a_few_seconds_after_its_process_is_gone() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::time::{Duration, SystemTime};
        use versekit_core::paths::Host;
        use versekit_core::pypath::Flavor;
        static IN_LIST: AtomicBool = AtomicBool::new(true);

        let dir = std::env::temp_dir().join(format!("versekit-fuss-ende-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        let game = dir.join("LIVE");
        std::fs::create_dir_all(&game).unwrap();
        let log = game.join("Game.log");
        std::fs::write(&log, "<2026-10-23T20:30:00.000Z> Log started\n").unwrap();
        let (home, install) = (dir.to_string_lossy().into_owned(), game.to_string_lossy().into_owned());
        let host = Host::new(Flavor::native(), &[("SC_BP_HOME", &home), ("SC_INSTALL_DIR", &install)], &home);
        let paths = Paths::new(host.clone());
        let mut page = SamplePage::with_settings(Paths::new(host));
        page.game_process = |_| Some(IN_LIST.load(Ordering::SeqCst));
        let age = |seconds: u64| {
            let at = SystemTime::now() - Duration::from_secs(seconds);
            std::fs::File::options().write(true).open(&log).unwrap().set_modified(at).unwrap();
        };
        let (on, off) = (language::t("ov_spiel_an"), language::t("ov_spiel_aus"));

        // Playing: in the list, the log quiet for ten minutes (a loading screen) changes nothing.
        IN_LIST.store(true, Ordering::SeqCst);
        age(600);
        assert!(page.foot_idle(0.0).starts_with(&on), "{}", page.foot_idle(0.0));
        // The game ends; its log was written last as it closed, 35 seconds ago.
        IN_LIST.store(false, Ordering::SeqCst);
        age(35);
        assert!(page.foot_idle(1.0).starts_with(&on), "within the two seconds the last look stands");
        let after = page.foot_idle(2.5);
        assert!(after.starts_with(&off), "the next look in the same run: {after}");
        // Counter-check: the rule of the contract log alone still calls this a running game (five minutes).
        assert!(versekit_core::mission_log::game_running(&paths), "the log alone would say it runs");
        // The last moments of the end (log written a few seconds ago) still count as running.
        age(5);
        assert!(page.foot_idle(5.0).starts_with(&on), "a log written just now still counts");
        // Without a readable process list the log alone decides, with its long quiet time.
        page.game_process = |_| None;
        age(35);
        assert!(page.foot_idle(8.0).starts_with(&on), "no list: the long quiet time");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// On real data the foot bar tells the game state from the log and the update bar
    /// names only a version given from outside; the sample page keeps its made-up ones.
    #[test]
    fn real_data_show_no_made_up_game_or_version() {
        use std::time::{Duration, SystemTime};
        use versekit_core::paths::Host;
        use versekit_core::pypath::Flavor;

        theme::set_current("original");
        let dir = std::env::temp_dir().join(format!("versekit-fuss-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        let game = dir.join("LIVE");
        std::fs::create_dir_all(&game).unwrap();
        let log = game.join("Game.log");
        std::fs::write(&log, "<2026-10-21T19:00:00.000Z> Log started\n").unwrap();
        let (home, install) = (dir.to_string_lossy().into_owned(), game.to_string_lossy().into_owned());
        let host = Host::new(Flavor::native(), &[("SC_BP_HOME", &home), ("SC_INSTALL_DIR", &install)], &home);
        let mut page = SamplePage::with_settings(Paths::new(host));
        // A machine where the game is not in the process list: the log alone tells it then.
        page.game_process = |_| Some(false);
        let (width, height) = font_size::BASE_MIN_SIZE;
        let (width, height) = (width.max(1280.0), height.max(820.0));
        let shows = |texts: &[(String, Pos2)], part: &str| texts.iter().any(|(text, _)| text.contains(part));
        let first = draw(&mut page, width, height).1;
        assert!(!shows(&first, "Basetool-Daten werden geladen"), "no made-up load at the start");
        // The start demo of the foot bar would stand where the game state stands.
        let look = |page: &mut SamplePage| {
            page.game_seen = None;
            page.loading_shown = true;
            page.loading = None;
            draw(page, width, height).1
        };

        let texts = look(&mut page);
        let foot: Vec<&String> = texts.iter().map(|(text, _)| text).filter(|text| text.contains("·")).collect();
        assert!(shows(&texts, &language::t("ov_spiel_an")), "a fresh log: the game runs, foot {foot:?}");
        assert!(!shows(&texts, "Stanton · 4.3.1"), "no made-up game state");
        assert!(!shows(&texts, super::super::SAMPLE_UPDATE), "no made-up version");
        assert!(!shows(&texts, "ist da."), "no version named: no update bar");
        // The overview counts the data that are open, or says the page comes later.
        for sample in ["412 SCU", "412 h", "41 200 aUEC", "Alle Dienste laufen", "Fertig in 2 h 10 min"] {
            assert!(!shows(&texts, sample), "made-up {sample}");
        }
        assert!(shows(&texts, "Noch nichts eingelagert"), "an empty stock says so");
        // The refinery tile exists only where the terminal can be read.
        assert!(
            shows(&texts, &language::tf("s_uv_raff_leer", &[("0", language::Arg::from("Alt+Umschalt+R"))]))
                || !cfg!(windows),
            "a tile without data says why: {texts:?}"
        );
        assert!(!shows(&texts, super::super::glance::NOT_YET), "no tile of 4.12 comes later any more");
        // The backup page works on the real data: giving out and resetting, no sample content.
        let key = "hf_sichern";
        page.go_to(key);
        let texts = look(&mut page);
        assert!(!shows(&texts, super::super::glance::NOT_YET), "{key} is built");
        assert!(shows(&texts, &language::t("s_be_alle")), "{key} gives out the collection: {texts:?}");
        assert!(!shows(&texts, "Laranite") && !shows(&texts, "Hier steht die Seite."), "{key}");
        // What is new reads the change log and shows no made-up version: the sample gallery stays away.
        page.go_to("hf_wasistneu");
        let texts = look(&mut page);
        assert!(!shows(&texts, super::super::glance::NOT_YET), "what is new is built");
        assert!(shows(&texts, &language::t("s_wn_lead")), "it draws its own page");
        assert!(!shows(&texts, &language::t("s_mu_aeltere")), "no made-up history");
        // The refineries without mining data: no sample table, the way to fetch them instead.
        page.go_to("hf_raffinerien");
        let texts = look(&mut page);
        assert!(
            shows(&texts, &language::t("s_rf_keine")) && shows(&texts, &language::t("s_bg_jetzt_holen")),
            "{texts:?}"
        );
        assert!(!shows(&texts, "Checkmate") && !shows(&texts, "Hier steht die Seite."), "{texts:?}");
        page.go_to("");
        let group = super::super::group_of("hf_meinlager").expect("a group");
        assert!(super::super::nav_hits_in("Titanium", group, "hf_meinlager", false).is_empty(), "no sample stock");
        assert!(!super::super::nav_hits_in("Titanium", group, "hf_meinlager", true).is_empty(), "counter-check");

        let old = SystemTime::now() - Duration::from_secs(600);
        std::fs::File::options().write(true).open(&log).unwrap().set_modified(old).unwrap();
        let texts = look(&mut page);
        assert!(shows(&texts, &language::t("ov_spiel_aus")), "an old log: the game does not run");

        page.apply_commands(&egui::Context::default(), vec![Command::SetUpdateVersion("5.0.1".into())]);
        let texts = look(&mut page);
        assert!(shows(&texts, "VerseKit 5.0.1 ist da."), "the named version shows");
        assert!(shows(&texts, "Update 5.0.1"));
        page.apply_commands(&egui::Context::default(), vec![Command::ShowUpdateBar(false)]);
        assert!(!shows(&look(&mut page), "ist da."), "the switch still takes the bar away");

        // Counter-check: the sample page keeps its made-up state.
        let mut sample = SamplePage::preview();
        assert!(shows(&draw(&mut sample, width, height).1, "Basetool-Daten werden geladen"));
        let texts = look(&mut sample);
        let bar = format!("VerseKit {} ist da.", super::super::SAMPLE_UPDATE);
        assert!(shows(&texts, "Stanton · 4.3.1") && shows(&texts, &bar));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Clicks into the menu filter, types `word` and presses `key` once.
    fn filter_and_press(page: &mut SamplePage, word: &str, key: egui::Key) {
        let (width, height) = font_size::BASE_MIN_SIZE;
        let size = (width.max(1280.0), height.max(820.0));
        CTX.with(|ctx| {
            let texts = draw(page, size.0, size.1).1;
            let at = place(&texts, &language::t("hf_menue_filtern")).expect("the filter hint is drawn");
            let pos = at + egui::vec2(4.0, 4.0);
            let button = |pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            frame(ctx, page, size, vec![egui::Event::PointerMoved(pos)]);
            frame(ctx, page, size, vec![button(true)]);
            frame(ctx, page, size, vec![button(false)]);
            frame(ctx, page, size, vec![egui::Event::Text(word.to_string())]);
            assert_eq!(page.nav_filter, word, "the keys went into the filter");
            let press = |pressed| egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            };
            frame(ctx, page, size, vec![press(true)]);
            frame(ctx, page, size, vec![press(false)]);
            for _ in 0..3 {
                frame(ctx, page, size, Vec::new());
            }
        });
    }

    #[test]
    fn enter_in_the_menu_filter_opens_the_first_hit_and_empties_the_field() {
        theme::set_current("original");
        let mut page = SamplePage::preview();
        page.go_to("");
        // The name of the first page of the first group: it is the first hit.
        filter_and_press(&mut page, &language::t("hf_liste"), egui::Key::Enter);
        assert_eq!(page.page, "hf_liste", "the first hit opened");
        assert_eq!(page.nav_filter, "", "the field is empty again");
    }

    #[test]
    fn escape_in_the_menu_filter_empties_the_field_and_opens_nothing() {
        theme::set_current("original");
        let mut page = SamplePage::preview();
        page.go_to("");
        filter_and_press(&mut page, &language::t("hf_liste"), egui::Key::Escape);
        assert_eq!(page.nav_filter, "", "the field is empty");
        assert_eq!(page.page, "", "no page opened");
    }

    #[test]
    fn what_a_page_asks_for_during_the_frame_is_done_at_its_end() {
        theme::set_current("original");
        let (mut page, log) = window(true);
        log.borrow_mut().commands.push(Command::SetSize(5));
        let (width, height) = font_size::BASE_MIN_SIZE;
        draw(&mut page, width.max(1280.0), height.max(820.0));
        assert_eq!(page.size, 5);
        assert!(log.borrow().commands.is_empty(), "taken");
    }

    #[test]
    fn many_pages_scroll_in_their_list_and_push_nothing_out() {
        theme::set_current("original");
        let (width, height) = font_size::BASE_MIN_SIZE;
        for points in [font_size::POINTS_RANGE.0, font_size::POINTS_RANGE.1] {
            let mut page = SamplePage::preview();
            page.size = points;
            page.set_extension(Box::new(Many));
            page.go_to("Seite 01");
            let (overflow, texts) = draw(&mut page, width, height);
            // The button's name may be shortened in the big sizes: its first word finds it.
            let start = language::t("s_sp_start_knopf");
            let word = start.split_whitespace().next().unwrap_or_default().to_string();
            let foot = texts
                .iter()
                .find(|(text, _)| text.starts_with(&word))
                .map(|(_, at)| *at)
                .unwrap_or_else(|| panic!("the launch button is drawn at {points}: {texts:?}"));
            let filter = place(&texts, &language::t("hf_menue_filtern")).expect("the filter is drawn");
            assert!(foot.y < height && filter.y < foot.y, "{points}: filter {filter:?} launcher {foot:?}");
            assert!(overflow < 0.5 && page.frame_overflow_y < 0.5, "{points}: {overflow} {}", page.frame_overflow_y);
            assert!(place(&texts, "Seite 01").is_some(), "the first page is listed");
        }
    }

    #[test]
    fn the_entry_and_its_pages_fit_every_size_and_scheme() {
        let (width, height) = font_size::BASE_MIN_SIZE;
        let mut findings = Vec::new();
        for scheme in theme::schemes() {
            theme::set_thread_current(scheme.id);
            for points in font_size::POINTS_RANGE.0..=font_size::POINTS_RANGE.1 {
                let (mut page, _) = window(true);
                page.size = points;
                for key in ["", "Erste Seite", "Zweite Seite"] {
                    page.go_to(key);
                    let (overflow, texts) = draw(&mut page, width, height);
                    if overflow > 0.5 {
                        findings.push(format!("{} {points} {key}: {overflow:.0} zu breit", scheme.id));
                    }
                    if place(&texts, "Beispiel-Erweiterung").is_none() {
                        findings.push(format!("{} {points} {key}: Eintrag fehlt", scheme.id));
                    }
                }
            }
        }
        assert!(findings.is_empty(), "({}):\n{}", findings.len(), findings.join("\n"));
    }
}
