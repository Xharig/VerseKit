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

//! Renders the window model of the sample page without a window
//! (egui_kittest on wgpu), clicks and drags in it and saves PNGs, for README
//! and forum images.
//!
//! Usage, from this folder:
//!     cargo run --release -- <target folder> [scheme] [what] [search word] [--en]
//! `scheme`: original, eis, nebel, glut, kontrast, krt (default krt).
//! `what`: alle (default), uebersicht, kachel, gruppen, filter, assistent,
//! seite (opens the first menu filter hit for the search word),
//! einrichtung (every step of the real setup assistant for a new program),
//! willkommen (every step of the short setup "Willkommen in 5.0" for a program
//! set up with 4.12; both on a throwaway data folder, nothing of the computer
//! is touched), galerie (the pictures of the gallery of 5.0, 700 × 430),
//! webseite (one picture of the whole window per page of the website, 1400 × 860; a fourth word names a
//! single picture, e.g. `beziehungen`), overlay (the overlay window on made-up finds, for the website and
//! the README, and the same window twice with the notice of an armistice zone), kartellbank (the page of the Cartel
//! Bank on the made-up bank of the sample page), updatewartet (a check picture of an update that has waited three days, in the overlay and the
//! bar of the main window, with the game closed and running), echt (the pages that read the game folder and the
//! data folder, on a made-up installation in a throwaway folder, the hangar with the slots of the game data among
//! them and `hangarwerte` with the hover of the base values of a ship; `--markt=<folder>` gives the price
//! lists and the slot files to copy; a fourth word names a single picture), verbunden (the pages that need a connection to the
//! Basetool, on the demonstration Basetool of the program), farbschemata (the page "Darstellung" in each colour
//! scheme).
//! `--en` draws in English (default German).
//! Targets are found by their names and roles, not by fixed points, so the
//! images keep working when the sample page changes.
use egui::accesskit::Role;
use egui::{Pos2, Rect, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use versekit_ui::sample::SamplePage;

mod install;

/// Hover text of a group head in the sidebar.
const GROUP_HEAD: &str = "Klicken: auf- und zuklappen · Ziehen: verschieben";
/// Heading of the wizard card on the full sample page.
const WIZARD: &str = "Einrichtungsassistent (Schritte links anklickbar)";
/// Size of the window model in points.
const WINDOW: egui::Vec2 = vec2(1226.0, 548.0);
/// Rendered pixels per point.
const SCALE: f32 = 2.0;

struct Shots {
    dir: String,
    harness: Harness<'static>,
}

impl Shots {
    fn steps(&mut self, n: usize) {
        self.harness.run_steps(n);
    }
    fn hover(&mut self, pos: Pos2) {
        self.harness.hover_at(pos);
        self.steps(2);
    }
    fn click(&mut self, pos: Pos2) {
        self.hover(pos);
        self.harness.drag_at(pos);
        self.steps(1);
        self.harness.drop_at(pos);
        self.steps(3);
    }
    /// Presses at `from` and moves to `to` in small steps, keeps the button down.
    fn hold_and_move(&mut self, from: Pos2, to: Pos2) {
        self.hover(from);
        self.harness.drag_at(from);
        self.steps(1);
        for i in 1..=12 {
            let p = from + (to - from) * (i as f32 / 12.0);
            self.harness.hover_at(p);
            self.steps(1);
        }
        self.steps(2);
    }
    fn release(&mut self, at: Pos2) {
        self.harness.drop_at(at);
        self.steps(4);
    }
    fn type_text(&mut self, text: &str) {
        self.harness.event(egui::Event::Text(text.to_string()));
        self.steps(3);
    }
    /// Places of the elements named `label` (hover texts and buttons), top to bottom.
    fn named(&self, label: &str) -> Vec<Rect> {
        sorted(self.harness.query_all_by_label(label).map(|n| n.rect()).collect())
    }
    /// Places of the texts that read `text`, top to bottom.
    fn texts(&self, text: &str) -> Vec<Rect> {
        sorted(self.harness.query_all_by_value(text).map(|n| n.rect()).collect())
    }
    /// The first element named `label`, or else the first text that reads it;
    /// ends the run when there is neither.
    fn first_named(&self, label: &str) -> Rect {
        let found = self.named(label).into_iter().chain(self.texts(label)).next();
        found.unwrap_or_else(|| panic!("„{label}“ nicht gefunden"))
    }
    /// The first text that reads `text`; ends the run when there is none.
    fn first_text(&self, text: &str) -> Rect {
        *self.texts(text).first().unwrap_or_else(|| panic!("Text „{text}“ nicht gefunden"))
    }
    /// The menu filter: the first text field of the window model.
    fn filter_field(&self) -> Rect {
        let fields = sorted(self.harness.query_all_by_role(Role::TextInput).map(|n| n.rect()).collect());
        *fields.first().expect("Menüfilter nicht gefunden")
    }
    /// Types `word` into the menu filter.
    fn filter(&mut self, word: &str) {
        let field = self.filter_field();
        self.click(field.center());
        self.type_text(word);
    }
    fn snap(&mut self, name: &str, area: Option<Rect>) {
        self.harness.remove_cursor();
        self.steps(3);
        // The graphics card may be slow to answer on a computer under load: a few tries before giving up.
        let image = (0..5)
            .find_map(|_| {
                self.harness.render().ok().or_else(|| {
                    std::thread::sleep(std::time::Duration::from_millis(800));
                    None
                })
            })
            .expect("render");
        let image = match area {
            Some(area) => image::imageops::crop_imm(
                &image,
                (area.min.x * SCALE) as u32,
                (area.min.y * SCALE) as u32,
                (area.width() * SCALE) as u32,
                (area.height() * SCALE) as u32,
            )
            .to_image(),
            None => image,
        };
        image.save(format!("{}/{name}.png", self.dir)).expect("Bild speichern");
        println!("gespeichert: {name}");
    }
}

fn sorted(mut rects: Vec<Rect>) -> Vec<Rect> {
    rects.sort_by(|a, b| a.min.y.total_cmp(&b.min.y).then(a.min.x.total_cmp(&b.min.x)));
    rects
}

/// Runs the page in a harness of `size`; fonts arrive one frame after
/// install, like in eframe's setup.
fn harness(mut page: SamplePage, size: egui::Vec2) -> Harness<'static> {
    let mut installed = false;
    Harness::builder()
        .with_size(size)
        .with_pixels_per_point(SCALE)
        .with_max_steps(50)
        .with_accessibility_check(false)
        .wgpu()
        .build_ui(move |ui| {
            if !installed {
                versekit_ui::install(ui.ctx());
                installed = true;
                ui.ctx().request_repaint();
                return;
            }
            page.show(ui);
        })
}

/// The settings file of a program set up with 4.12 (made anonymous): the short setup needs a program that was set
/// up before 5.0.
const SETTINGS_OF_4_12: &str = include_str!("../../pruefdaten/einstellungen-4-12-beispiel.json");

/// A throwaway data folder with a game folder (an old `Game.log` in `LIVE`: the game does not run, so the short
/// setup may open), the settings of a program set up with 4.12 where `set_up` says so. Nothing of the computer
/// is touched, and the Basetool cannot go out.
fn throwaway(set_up: bool) -> (std::path::PathBuf, versekit_core::paths::Paths) {
    use versekit_core::envvars::{ENV_HOME, ENV_NO_NET};
    use versekit_core::paths::{ENV_INSTALL_DIR, Host, Paths};
    // The picture shows the game folder in the field: a path in the home folder would show the name of the
    // user. The folder lies in the root of the system drive instead (a plain `StarCitizen\LIVE` under it), and
    // falls back to the temporary folder where that is not allowed.
    let name = format!("vk-bilder-{}", std::process::id());
    let root = std::env::var("SystemDrive").map(|d| std::path::PathBuf::from(format!("{d}\\"))).ok();
    let mut dir = root.unwrap_or_else(std::env::temp_dir).join(&name);
    let mut live = dir.join("StarCitizen").join("LIVE");
    if std::fs::create_dir_all(&live).is_err() {
        dir = std::env::temp_dir().join(&name);
        live = dir.join("StarCitizen").join("LIVE");
        std::fs::create_dir_all(&live).expect("Ordner");
    }
    let data = dir.join("Daten");
    std::fs::create_dir_all(&data).expect("Datenordner");
    let log = live.join("Game.log");
    std::fs::write(&log, "").expect("Log");
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(7200);
    std::fs::File::options().write(true).open(&log).expect("Log").set_modified(old).expect("Zeit");
    if set_up {
        std::fs::write(data.join("einstellungen.json"), SETTINGS_OF_4_12).expect("Einstellungen");
    }
    let (home, data, game) =
        (dir.to_string_lossy().into_owned(), data.to_string_lossy().into_owned(), live.to_string_lossy().into_owned());
    let host = Host::new(
        versekit_core::pypath::Flavor::native(),
        &[(ENV_HOME, &data), ("HOME", &home), ("USERPROFILE", &home), (ENV_NO_NET, "1"), (ENV_INSTALL_DIR, &game)],
        &home,
    );
    (dir, Paths::new(host))
}

/// Saves a picture of every step of the setup assistant: the full one (`short` false, a new program) or the
/// short one `Willkommen in 5.0` (a program set up with 4.12). The assistant is the real one on a throwaway
/// folder; each step is confirmed as a player does, and the game step waits for the reading of the logs.
/// `tag` names the pictures: `<tag>_<language>_<scheme>_<step>.png`.
fn assistant_steps(dir: String, scheme: &str, short: bool, english: bool) {
    let (tmp, paths) = throwaway(short);
    let mut page = SamplePage::with_settings(paths);
    let opened = if short { page.open_short_if_needed() } else { page.open_setup_if_needed() };
    assert!(opened, "der Assistent hat sich nicht geöffnet");
    let mut s = Shots { dir, harness: harness(page, vec2(1100.0, 780.0)) };
    s.steps(30);
    let tag = if short { "willkommen" } else { "einrichtung" };
    let lang = if english { "en" } else { "de" };
    // How many steps there are: the counter of the first one says it.
    let counter = |k: usize, n: usize| {
        versekit_core::language::tf(
            "schritt_von",
            &[("0", versekit_core::language::Arg::from(k)), ("1", versekit_core::language::Arg::from(n))],
        )
    };
    let total = (1..=14).find(|n| !s.texts(&counter(1, *n)).is_empty()).expect("Schrittzähler nicht gefunden");
    let confirm = versekit_core::language::t("s_mlg_bestaetigen");
    for k in 1..=total {
        s.snap(&format!("{tag}_{lang}_{scheme}_{k:02}"), None);
        if k == total {
            break;
        }
        // Confirm; the game step keeps the player until the logs are read, so it is asked again.
        let mut shown_wait = false;
        for _ in 0..200 {
            if let Some(button) = s.named(&confirm).first().copied() {
                s.click(button.center());
            }
            s.steps(3);
            if !s.texts(&counter(k + 1, total)).is_empty() {
                break;
            }
            if !shown_wait {
                // The step stayed: the reading. Wait a moment and show what it says.
                shown_wait = true;
                std::thread::sleep(std::time::Duration::from_millis(1500));
                s.steps(6);
                s.snap(&format!("{tag}_{lang}_{scheme}_{k:02}b"), None);
            } else {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        s.steps(30);
    }
    let _ = std::fs::remove_dir_all(tmp);
}

/// The pictures of the gallery "Was ist neu in 5.0": the page of each highlight on the sample data in a tall window
/// drawn at twice the size; a part of the page in the proportions of the gallery, from the text of its place where
/// it has one, is cut out and shrunk to 700 × 430 pixels. Named as the gallery takes them: `<picture>.png`, in
/// English `<picture>-en.png`.
fn gallery_pictures(dir: &str, english: bool) {
    use versekit_core::gallery::{Target, highlights};
    use versekit_core::language::t;
    // Tall enough that the place of each highlight stands in the window without scrolling: the picture is cut
    // out around it.
    const SIZE: egui::Vec2 = vec2(1400.0, 2400.0);
    // Where the page begins below the head bar and the update bar of the sample page, in points.
    const PAGE_TOP: f32 = 92.0;
    // The group of each page in the menu, and the text the picture scrolls to.
    let places: [(&str, &str, Option<&str>); 5] = [
        ("hf_einkaufsliste", "hf_gruppe_beschaffung", None),
        ("hf_meinlager", "hf_gruppe_logistik", None),
        ("hf_liste", "hf_gruppe_bp", None),
        ("hf_allgemein", "hf_gruppe_einst", Some("s_ei_datenschutz")),
        ("hf_overlay", "hf_gruppe_einst", Some("s_ov_ereignisse")),
    ];
    for point in highlights("5.0.0") {
        let Target::Page(key) = point.target else { continue };
        let (_, group, anchor) = places.iter().find(|(k, _, _)| *k == key).copied().expect("Ort im Menü");
        let mut page = SamplePage::preview();
        let (mut installed, mut opened) = (false, false);
        let harness = Harness::builder()
            .with_size(SIZE)
            .with_pixels_per_point(SCALE)
            .with_max_steps(50)
            .with_accessibility_check(false)
            .wgpu()
            .build_ui(move |ui| {
                if !installed {
                    versekit_ui::install(ui.ctx());
                    installed = true;
                    ui.ctx().request_repaint();
                    return;
                }
                if !opened {
                    page.open_page(ui.ctx(), key, group);
                    opened = true;
                }
                page.show(ui);
            });
        let mut s = Shots { dir: dir.to_string(), harness };
        s.steps(12);
        // The place of the text the picture shows first; none: the head of the page.
        let shown_from = anchor.map(|anchor| {
            // A group title is drawn in capitals: the text is compared without case.
            let text = t(anchor).to_lowercase();
            s.harness
                .query_all_by(move |node| {
                    node.label().or_else(|| node.value()).is_some_and(|shown| shown.to_lowercase().starts_with(&text))
                })
                .map(|node| node.rect().min.y)
                .next()
                .unwrap_or_else(|| panic!("„{}“ nicht gefunden", t(anchor)))
        });
        s.harness.remove_cursor();
        s.steps(3);
        let image = (0..5)
            .find_map(|_| {
                s.harness.render().ok().or_else(|| {
                    std::thread::sleep(std::time::Duration::from_millis(800));
                    None
                })
            })
            .expect("render");
        // The page alone: right of the menu (its filter field and the button beside it mark the edge), below the
        // head bar and the bar of the made-up update of the sample page, in the proportions of the gallery.
        let left = s.filter_field().max.x + 30.0;
        let width = SIZE.x - left;
        let height = width * 430.0 / 700.0;
        let top = shown_from.map_or(PAGE_TOP, |y| y - 40.0).clamp(PAGE_TOP, SIZE.y - height);
        let page = image::imageops::crop_imm(
            &image,
            (left * SCALE) as u32,
            (top * SCALE) as u32,
            (width * SCALE) as u32,
            (height * SCALE) as u32,
        )
        .to_image();
        let small = image::imageops::resize(&page, 700, 430, image::imageops::FilterType::Lanczos3);
        let name = format!("{}/{}{}.png", s.dir, point.picture, if english { "-en" } else { "" });
        small.save(&name).expect("Bild speichern");
        println!("gespeichert: {name}");
    }
}

/// The pictures of the website: the whole window model with the menu, one page each on the sample data, drawn at
/// twice the size and shrunk to 1400 × 860 pixels. Named as the website takes them: `screenshot-<name>.png`, in
/// English `screenshot-<name>-en.png`. A page key of `None` is the start page the window opens on.
fn website_pictures(dir: &str, english: bool, only: Option<&str>) {
    // The pictures of the start page and of every stop of the tour (`assets/seite.js`). The hangar is drawn by
    // `echt`: its slots come from the game data of the made-up installation.
    let pages: [(&str, Option<&str>); 48] = [
        ("uebersicht", None),
        ("notizen", Some("hf_notizen")),
        ("liste", Some("hf_liste")),
        ("fortschritt", Some("hf_fortschritt")),
        ("auftragslog", Some("hf_auftragslog")),
        ("meinlager", Some("hf_meinlager")),
        ("staffellager", Some("hf_staffellager")),
        ("herstellung", Some("hf_herstellung")),
        ("farmliste", Some("hf_farmliste")),
        ("bergbau", Some("hf_bergbau")),
        ("raffinerien", Some("hf_raffinerien")),
        ("bergung", Some("hf_bergung")),
        ("laeden", Some("hf_laeden")),
        ("verkauf", Some("hf_verkauf")),
        ("routen", Some("hf_routen")),
        ("kartellbank", Some("hf_bank")),
        ("bedarf", Some("hf_bedarf")),
        ("einkaufsliste", Some("hf_einkaufsliste")),
        ("contested", Some("hf_contested")),
        ("statistik", Some("hf_st_uebersicht")),
        ("statistik-schiffe", Some("hf_st_schiffe")),
        ("statistik-auftraege", Some("hf_st_auftraege")),
        ("statistik-quantum", Some("hf_st_quantum")),
        ("statistik-stabil", Some("hf_st_stabil")),
        ("statistik-flugbuch", Some("hf_st_flugbuch")),
        ("allgemein", Some("hf_allgemein")),
        ("anzeige", Some("hf_overlay")),
        ("darstellung", Some("hf_darstellung")),
        ("ordner", Some("hf_ordner")),
        ("module", Some("hf_module")),
        ("startprogramme", Some("hf_startprogramme")),
        ("basetool", Some("k_basetool_verbindung")),
        ("orga-verbinden", Some("s_os_seite")),
        ("bestand", Some("hf_sichern")),
        ("auftragstexte", Some("hf_spiel")),
        ("uebersetzung", Some("hf_uebersetzung")),
        ("grafik", Some("hf_grafik")),
        ("blickwinkel", Some("hf_blickwinkel")),
        ("joysticks", Some("hf_joysticks")),
        ("achsen", Some("hf_achsen")),
        // Pages that left the menu: the window opens the page that took over, at the place of the old one (the
        // card of the shader cache on the page Grafik, the tab "Beziehungen" of the handle search).
        ("shader", Some("hf_shader")),
        ("ueber", Some("hf_ueber")),
        ("handlesuche", Some("hf_handlesuche")),
        ("beziehungen", Some("hf_beziehungen")),
        // The picture of the README: the stock filtered to what lies there to sell.
        ("meinlager-verkauf", Some("hf_meinlager")),
        ("patchaenderungen", Some("hf_patchaenderungen")),
        ("serverstatus", Some("hf_serverstatus")),
        ("danke", Some("hf_danke")),
    ];
    for (name, key) in pages {
        if only.is_some_and(|only| only != name) {
            continue;
        }
        // The Cartel Bank page is only in the menu of a window with its switch on and a connection standing: the
        // picture takes the page on its made-up bank.
        let page = if name == "kartellbank" { SamplePage::preview_still_bank() } else { SamplePage::preview_still() };
        page_picture(dir, english, name, key, page);
    }
}

/// What a page gets done before its picture, so that it shows what it is for: a search typed into its field.
fn touch(name: &str, english: bool, s: &mut Shots) {
    if name == "signatur" {
        // The digit with the suspect pattern opened, the page scrolled to the cards of the pictures; the pointer
        // rests on one card, whose bin shows.
        let word = if english { "suspect" } else { "verdächtig" };
        let head = s.harness.query_all_by_label_contains(word).next().map(|n| n.rect());
        match head {
            Some(head) => {
                s.click(head.center());
                s.steps(12);
            }
            None => println!("{name}: Kopf mit „{word}“ nicht gefunden"),
        }
        s.hover(pos2(800.0, 400.0));
        s.harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, -330.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: egui::Modifiers::NONE,
        });
        s.steps(12);
        s.hover(pos2(500.0, 640.0));
        s.steps(8);
        return;
    }
    if name == "hangarwerte" {
        // The page scrolled so that the card of the Cutlass Black stands in view (its name lies below the edge of
        // the window at first), then the pointer rests on its name: its base values in the hover.
        let Some(ship) = s.texts("Cutlass Black").first().copied() else {
            println!("{name}: Schiff „Cutlass Black“ nicht gefunden");
            return;
        };
        s.hover(pos2(800.0, 400.0));
        s.harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, -(ship.min.y - 300.0).max(0.0)),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: egui::Modifiers::NONE,
        });
        s.steps(12);
        match s.texts("Cutlass Black").first().copied() {
            Some(ship) => {
                s.hover(ship.left_center() + vec2(6.0, 0.0));
                s.steps(12);
            }
            None => println!("{name}: Schiff „Cutlass Black“ nach dem Scrollen nicht gefunden"),
        }
        return;
    }
    if name == "meinlager-verkauf" {
        // The purpose filter of the stock: only what lies there to sell.
        let all = versekit_core::language::t("s_ml_alle_zwecke");
        let sell = versekit_core::language::t("s_ml_verkaufen");
        match s.texts(&all).first().copied() {
            Some(combo) => {
                s.click(combo.center());
                match s.named(&sell).into_iter().chain(s.texts(&sell)).next() {
                    Some(option) => s.click(option.center()),
                    None => println!("{name}: Auswahl „{sell}“ nicht gefunden"),
                }
            }
            None => println!("{name}: Filter „{all}“ nicht gefunden"),
        }
        s.steps(8);
        return;
    }
    if name == "grafik" || name == "shader" {
        // The card of the shader cache below the graphics settings on the page Grafik, scrolled into view.
        let title = versekit_core::language::t("s_sc_titel");
        match s.texts(&title).into_iter().chain(s.named(&title)).next() {
            Some(card) => {
                s.hover(pos2(800.0, 500.0));
                s.harness.event(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: vec2(0.0, -(card.min.y - 470.0)),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            None => println!("{name}: Karte „{title}“ nicht gefunden"),
        }
        s.steps(12);
        return;
    }
    if name == "notizen" {
        // Two made-up notes, the second one open: the page of the sample keeps none.
        let notes = if english {
            [
                "Hornet loadout\nTwo size 3 guns for the nose turret\nCheck the shield generator price in Area18",
                "Mining evening on Saturday\nMeet at Port Tressler, 8 pm\nFill up the Prospector\nCompare the refineries in Pyro afterwards",
            ]
        } else {
            [
                "Ausstattung Hornet\nZwei Waffen Größe 3 für den Nasenturm\nPreis des Schildgenerators in Area18 prüfen",
                "Mining-Abend am Samstag\nTreffpunkt Port Tressler, 20 Uhr\nProspector volltanken\nDanach die Raffinerien in Pyro vergleichen",
            ]
        };
        let new = versekit_core::language::t("s_nz_neu");
        for text in notes {
            match s.named(&new).into_iter().chain(s.texts(&new)).next() {
                Some(button) => s.click(button.center()),
                None => {
                    println!("{name}: Knopf „{new}“ nicht gefunden");
                    return;
                }
            }
            // A new note takes the cursor: the text goes straight in.
            s.steps(4);
            s.type_text(text);
        }
        // A click on the lead line of the page takes the cursor out of the text.
        let lead = versekit_core::language::t("s_nz_lead");
        if let Some(line) = s.texts(&lead).first().copied() {
            s.click(line.center());
        }
        s.steps(8);
        return;
    }
    if name == "meinlager" || name == "bergbau" || name == "herstellung" {
        // What is new on the page, unfolded: the yield of the refinery with its storage place, the nine methods of
        // the refinery side by side, the jobs at the fabricator. A folded head is found by the start of its text.
        let head = match name {
            "meinlager" => versekit_core::language::t("s_rf_titel"),
            "bergbau" => versekit_core::language::t("s_rm_alle"),
            _ => versekit_core::language::t("s_wb_titel"),
        };
        let found = s
            .harness
            .query_all_by(|n| n.label().or_else(|| n.value()).is_some_and(|v| v.starts_with(&head)))
            .map(|n| n.rect())
            .next();
        match found {
            Some(fold) => s.click(fold.center()),
            None => println!("{name}: Klappe „{head}“ nicht gefunden"),
        }
        s.steps(12);
        if name != "herstellung" {
            return;
        }
        // The recipe of one blueprint open beside the list: typed into the search of the blueprints (the wide field;
        // the narrow one above it is the name of a new job), then the hit below the field.
        let fields = sorted(s.harness.query_all_by_role(Role::TextInput).map(|n| n.rect()).collect());
        let Some(field) = fields.iter().find(|f| f.min.x > 240.0 && f.width() > 300.0).copied() else {
            println!("{name}: Suchfeld nicht gefunden, Seite ohne Auswahl");
            return;
        };
        s.click(field.center());
        // A cooler of the game data on the made-up installation, the cooler of the sample data on the sample page.
        for word in ["Glacier", "Frostline"] {
            s.type_text(word);
            s.steps(8);
            let hit = s
                .harness
                .query_all_by(|n| n.label().or_else(|| n.value()).is_some_and(|v| v.contains(word)))
                .map(|n| n.rect())
                .find(|r| r.min.y > field.max.y + 10.0);
            if let Some(hit) = hit {
                s.click(hit.center());
                s.steps(8);
                return;
            }
            for _ in 0..word.len() {
                s.harness.key_press(egui::Key::Backspace);
            }
            s.steps(4);
        }
        println!("{name}: kein Kühler gefunden, Seite ohne Auswahl");
        return;
    }
    if name == "beziehungen" {
        // One of the made-up handles marked as watched, so that all four stances stand in the list.
        use versekit_core::language::{Arg, t, tf};
        let label =
            tf("s_hq_handle_als", &[("haltung", Arg::Text(t("s_hq_beobachten"))), ("org", Arg::Text("Vex".into()))]);
        // The marks of the rows top to bottom; the handle Vex stands last.
        match s.named(&label).last().copied() {
            Some(mark) => s.click(mark.center()),
            None => println!("{name}: Markierung „{label}“ nicht gefunden"),
        }
        s.steps(8);
        return;
    }
    if name == "laeden" {
        // The shops of one place: the field "Ort oder Shop" below the search, then the cursor out of it.
        let fields = sorted(s.harness.query_all_by_role(Role::TextInput).map(|n| n.rect()).collect());
        let Some(field) = fields.iter().filter(|f| f.min.x > 240.0).nth(1).copied() else {
            println!("{name}: Feld „Ort oder Shop“ nicht gefunden");
            return;
        };
        s.click(field.center());
        s.type_text("Area 18");
        s.steps(8);
        s.harness.key_press(egui::Key::Escape);
        s.steps(4);
        let lead = versekit_core::language::t("s_ld_ort");
        if let Some(line) = s.texts(&lead).first().copied() {
            s.click(line.center());
        }
        s.steps(8);
        return;
    }
    if name == "hangar" {
        // The loadout of the first ship unfolded: its slots from the game data.
        let title = versekit_core::language::t("s_wk_titel");
        match s.texts(&title).into_iter().chain(s.named(&title)).next() {
            Some(fold) => {
                s.click(fold.center());
                s.steps(8);
                // The page scrolled so that the card of the ship stands near the top: the slots fill the picture.
                // The head of the fold stops low enough for the heading of the list and the line above the card
                // ("5 von 5 Schiffen …") to stand whole under the search field.
                s.hover(pos2(fold.center().x + 400.0, fold.center().y));
                s.harness.event(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    // `VERSEKIT_HANGAR_SCROLL`: points further down, to show the part of the list further below.
                    delta: vec2(
                        0.0,
                        -((fold.min.y - 346.0).max(0.0)
                            + std::env::var("VERSEKIT_HANGAR_SCROLL").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0)),
                    ),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            None => println!("{name}: Ausstattung „{title}“ nicht gefunden"),
        }
        s.steps(12);
        return;
    }
    let search = match name {
        "bergung" => "Cutlass Black",
        "zerlegen" => "Aves Helmet",
        _ => return,
    };
    // The search of the page is the first text field right of the menu.
    let fields = sorted(s.harness.query_all_by_role(Role::TextInput).map(|n| n.rect()).collect());
    let Some(field) = fields.iter().find(|f| f.min.x > 240.0).copied() else {
        println!("{name}: Suchfeld nicht gefunden, Seite ohne Suche");
        return;
    };
    s.click(field.center());
    s.type_text(search);
    s.steps(8);
    if name == "bergung" {
        // The salvage page looks up the first suggestion with Enter: what the ship carries and what it is worth.
        s.harness.key_press(egui::Key::Enter);
        // The look-up runs in the background: wait until the line "Schlage nach …" is gone.
        let busy = versekit_core::language::t("s_wr_hole");
        for _ in 0..100 {
            s.steps(2);
            let still = s.harness.query_all_by(|n| n.label().or_else(|| n.value()).is_some_and(|v| v.starts_with(&busy)));
            if still.count() == 0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        s.steps(12);
        return;
    }
    // The first hit opens: where the part stands and what it costs.
    let hit = s
        .harness
        .query_all_by(|n| n.label().or_else(|| n.value()).is_some_and(|v| v.contains(search)))
        .map(|n| n.rect())
        .find(|r| r.min.y > field.max.y + 40.0);
    match hit {
        Some(hit) => s.click(hit.center()),
        None => println!("{name}: Treffer für „{search}“ nicht gefunden, Seite ohne geöffneten Eintrag"),
    }
    s.steps(8);
}

/// One picture of the whole window model with `page` open on the page `key` (none: the start page), 1400 × 860
/// pixels, saved as `screenshot-<name>.png` (`-en` in English).
fn page_picture(dir: &str, english: bool, name: &str, key: Option<&'static str>, mut page: SamplePage) {
    const SIZE: egui::Vec2 = vec2(1400.0, 860.0);
    // The window as the program draws it: with its own title bar.
    page.use_own_frame(true);
    {
        let (mut installed, mut opened) = (false, false);
        let harness = Harness::builder()
            .with_size(SIZE)
            .with_pixels_per_point(SCALE)
            .with_max_steps(50)
            .with_accessibility_check(false)
            .wgpu()
            .build_ui(move |ui| {
                if !installed {
                    versekit_ui::install(ui.ctx());
                    installed = true;
                    ui.ctx().request_repaint();
                    return;
                }
                if !opened {
                    if let Some(key) = key {
                        // A page without an entry in the menu ("Was ist neu") has no group.
                        let group = versekit_ui::sample::group_of_page(key).unwrap_or("");
                        page.open_page(ui.ctx(), key, group);
                    }
                    opened = true;
                }
                page.show(ui);
            });
        let mut s = Shots { dir: dir.to_string(), harness };
        s.steps(12);
        touch(name, english, &mut s);
        // A picture of a hover keeps the pointer where it rests.
        if name != "hangarwerte" && name != "signatur" {
            s.harness.remove_cursor();
            s.steps(3);
        }
        let image = (0..5)
            .find_map(|_| {
                s.harness.render().ok().or_else(|| {
                    std::thread::sleep(std::time::Duration::from_millis(800));
                    None
                })
            })
            .expect("render");
        let small = image::imageops::resize(&image, 1400, 860, image::imageops::FilterType::Lanczos3);
        let file = format!("{}/screenshot-{name}{}.png", s.dir, if english { "-en" } else { "" });
        small.save(&file).expect("Bild speichern");
        println!("gespeichert: {file}");
    }
}

/// The notice of an armistice zone in the overlay: two windows side by side on a plain dark ground, the left one
/// after leaving a zone, the right one after entering one, each with a find above the notice. Named
/// `screenshot-overlay-schutzzone.png`, in English `screenshot-overlay-schutzzone-en.png`.
fn zone_picture(dir: &str, english: bool) {
    use versekit_core::armistice::Zone;
    use versekit_core::blueprints::kind_readable;
    use versekit_core::language::Phrase;
    use versekit_core::overlay::{BarSide, DEFAULT_WIDTH, Message, Row, Rows, Shown};
    use versekit_ui::overlay::{self, View};
    const MARGIN: f32 = 12.0;
    const HEIGHT: f32 = 128.0;
    let scheme = versekit_core::theme::current();
    let mut rows = Rows::default();
    rows.push(
        Row::Found {
            name: "Pembroke Helmet".into(),
            kind: kind_readable("Char_Armor_Helmet"),
            code: "S1".into(),
            time: "20:41:07".into(),
            caught_up: false,
        },
        overlay::DEFAULT_ROWS,
    );
    let status = Message::Phrase(Phrase::new("ov_status").with("0", 442).with("1", "20:44:52"));
    let width = DEFAULT_WIDTH as f32;
    let size = vec2(width * 2.0 + MARGIN * 3.0, HEIGHT + 2.0 * MARGIN);
    let mut installed = false;
    let mut harness = Harness::builder()
        .with_size(size)
        .with_pixels_per_point(SCALE)
        .with_max_steps(50)
        .with_accessibility_check(false)
        .wgpu()
        .build_ui(move |ui| {
            if !installed {
                versekit_ui::install(ui.ctx());
                installed = true;
                ui.ctx().request_repaint();
                return;
            }
            ui.painter().rect_filled(ui.max_rect(), 0.0, egui::Color32::from_rgb(0x0a, 0x10, 0x1c));
            for (column, zone) in [(0.0, Zone::Left), (1.0, Zone::Entered)] {
                let view = View {
                    scheme,
                    shown: Shown::Open,
                    rows: &rows,
                    contracts: &[],
                    contracts_max: 10,
                    hidden: None,
                    refinery: &[],
                    bank: &[],
                    launcher: true,
                    status: &status,
                    location: None,
                    shard: Some("euw1b"),
                    shard_hint: None,
                    signature: None,
                    demand: None,
                    update: None,
                    update_days: None,
                    zone: Some(zone),
                    game: true,
                    locked: false,
                    bar: BarSide::Top,
                    anchored: (false, false),
                    opacity: 1.0,
                    extras: &[],
                    menu: true,
                    drag_under_buttons: false,
                    hangar: None,
                    hints_outside: false,
                };
                let at = Rect::from_min_size(pos2(MARGIN + column * (width + MARGIN), MARGIN), vec2(width, HEIGHT));
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(at));
                child.set_clip_rect(at);
                overlay::show(&mut child, &view);
            }
        });
    harness.run_steps(12);
    harness.remove_cursor();
    harness.run_steps(3);
    let image = (0..5)
        .find_map(|_| {
            harness.render().ok().or_else(|| {
                std::thread::sleep(std::time::Duration::from_millis(800));
                None
            })
        })
        .expect("render");
    let file = format!("{dir}/screenshot-overlay-schutzzone{}.png", if english { "-en" } else { "" });
    image.save(&file).expect("Bild speichern");
    println!("gespeichert: {file}");
}

/// The pictures for checking the work of 5.1.4 by eye, without a window on any screen and at the full pixel size
/// (`pruefbild-<name>.png`): the own title bar at rest and with the pointer on the maximize button, the page General
/// with the group Privacy, the notes page and the stock page.
fn review_pictures(dir: &str) {
    let window = vec2(1226.0, 548.0);
    // The title bar of its own.
    let mut page = SamplePage::preview_still();
    page.use_own_frame(true);
    let mut s = Shots { dir: dir.to_string(), harness: harness(page, window) };
    s.steps(10);
    let bar = Rect::from_min_size(pos2(0.0, 0.0), vec2(window.x, 64.0));
    s.snap("pruefbild-titelleiste", Some(bar));
    let big = s.first_named(&versekit_core::language::t("s_fk_gross"));
    s.hover(big.center());
    let image = s.harness.render().expect("render");
    image::imageops::crop_imm(&image, 0, 0, (window.x * SCALE) as u32, (64.0 * SCALE) as u32)
        .to_image()
        .save(format!("{dir}/pruefbild-titelleiste-maximieren.png"))
        .expect("Bild speichern");
    println!("gespeichert: pruefbild-titelleiste-maximieren");
    // Pages that are shown tall enough to see all of them.
    for (name, key) in
        [("allgemein", "hf_allgemein"), ("notizen", "hf_notizen"), ("meinlager", "hf_meinlager")]
    {
        let mut page = SamplePage::preview_still();
        let mut opened = false;
        let mut installed = false;
        let harness = Harness::builder()
            .with_size(vec2(1300.0, 1500.0))
            .with_pixels_per_point(SCALE)
            .with_max_steps(50)
            .with_accessibility_check(false)
            .wgpu()
            .build_ui(move |ui| {
                if !installed {
                    versekit_ui::install(ui.ctx());
                    installed = true;
                    ui.ctx().request_repaint();
                    return;
                }
                if !opened {
                    let group = versekit_ui::sample::group_of_page(key).unwrap_or("");
                    page.open_page(ui.ctx(), key, group);
                    opened = true;
                }
                page.show(ui);
            });
        let mut s = Shots { dir: dir.to_string(), harness };
        s.steps(14);
        s.snap(&format!("pruefbild-{name}"), None);
    }
}

/// The pictures of the pages that read the game folder and the data folder, on a made-up installation
/// (`install.rs`): the window model on real data, with nothing of the computer touched. Named as the website takes
/// them, like [`website_pictures`].
fn real_pictures(dir: &str, english: bool, only: Option<&str>, market: Option<&std::path::Path>) {
    let pages: [(&str, &str); 19] = [
        ("hangar", "hf_hangar"),
        ("hangarwerte", "hf_hangar"),
        ("bergung", "hf_bergung"),
        ("zerlegen", "hf_bergung"),
        ("wasistneu", "hf_wasistneu"),
        ("auftragstexte", "hf_spiel"),
        ("uebersetzung", "hf_uebersetzung"),
        ("grafik", "hf_grafik"),
        ("blickwinkel", "hf_blickwinkel"),
        ("joysticks", "hf_joysticks"),
        ("achsen", "hf_achsen"),
        ("shader", "hf_shader"),
        ("module", "hf_module"),
        ("startprogramme", "hf_startprogramme"),
        ("laeden", "hf_laeden"),
        ("verkauf", "hf_verkauf"),
        ("bestand", "hf_sichern"),
        ("ueber", "hf_ueber"),
        // The jobs at the fabricator are kept in the data folder: the page needs the made-up installation. Last,
        // because opening it folds groups of the menu, and the folding is kept for the pages after it.
        ("herstellung", "hf_herstellung"),
    ];
    let (tmp, host) = install::build(SETTINGS_OF_4_12, market);
    // A hangar file of one's own in place of the made-up fleet (the loadout pictures of one ship).
    if let Ok(file) = std::env::var("VERSEKIT_HANGAR_DATEI") {
        let target = versekit_core::paths::Paths::new(host.clone()).app_file(versekit_core::hangar::FILE);
        if let Err(e) = std::fs::copy(&file, &target) {
            println!("Hangar-Datei {file} nicht übernommen: {e}");
        }
    }
    {
        // Three made-up jobs at the fabricator, entered a while ago: two run, one waits.
        let paths = versekit_core::paths::Paths::new(host.clone());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0.0, |d| d.as_secs_f64());
        for (job, minutes, ago) in
            [("Arclight Pistol", 45, 30.0), ("Pembroke Helmet", 90, 25.0), ("Morozov-SH Helmet", 30, 20.0)]
        {
            if let Err(e) = versekit_core::workbench_jobs::add(&paths, job, minutes * 60, now - ago * 60.0) {
                println!("Fabrikator-Auftrag {job} nicht eingetragen: {e:?}");
            }
        }
    }
    for (name, key) in pages {
        if only.is_some_and(|only| only != name) {
            continue;
        }
        let paths = versekit_core::paths::Paths::new(host.clone());
        page_picture(dir, english, name, Some(key), SamplePage::with_settings(paths));
    }
    let _ = std::fs::remove_dir_all(tmp);
}

/// The tab Signatur of the mining page on a made-up installation: the scanner on, a few pictures taught (several of
/// one number among them) and one suspect pattern. Named `screenshot-signatur.png`, in English
/// `screenshot-signatur-en.png`.
fn signature_picture(dir: &str, english: bool) {
    use versekit_core::jsonio::{self, Value};
    use versekit_core::signature_scan::{self as scan, ByVariant, Gray, Variant};
    const CROPS: &str = include_str!("../../../crates/versekit-core/tests/data/signatur-pruefbilder.json");
    let (tmp, host) = install::build(SETTINGS_OF_4_12, None);
    let paths = versekit_core::paths::Paths::new(host.clone());
    paths.set_setting(versekit_core::signature_watch::SETTING, Value::Bool(true));
    let files = scan::OwnFiles::in_folder(|name| paths.app_file(name));
    let folder = std::path::Path::new(&files.shared).parent().map(|p| p.to_string_lossy().into_owned());
    let samples = format!("{}/{}", folder.unwrap_or_default().trim_end_matches(['/', '\\']), scan::SAMPLE_FOLDER);
    let file = jsonio::loads(CROPS).expect("Prüfbilder");
    let Some(Value::Object(images)) = file.get("bilder") else { panic!("bilder") };
    let mut taught = 0;
    for (key, raster) in images {
        let Value::Array(rows) = raster else { continue };
        let (mut data, mut w) = (Vec::new(), 0);
        for row in rows {
            let Value::Array(row) = row else { continue };
            w = row.len();
            data.extend(row.iter().map(|v| if let Value::Int(v) = v { v.parse::<u8>().unwrap_or(0) } else { 0 }));
        }
        let number: String = key.split('#').next().unwrap_or("").chars().filter(char::is_ascii_digit).collect();
        let gray = Gray::new(w, rows.len(), data);
        let known = scan::templates_by_variant(scan::SHIPPED_TEMPLATES, &files);
        let images = ByVariant::both(Some(gray));
        if scan::learn_variants(&images, &number, Some(Variant::Green), &known, &files, &samples).is_ok() {
            taught += 1;
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        if taught == 9 {
            break;
        }
    }
    // One pattern too wide for a digit, as a name or a speed leaves it.
    let own_file = files.of(Variant::Green).to_string();
    let mut file = scan::own::OwnFile::open(&own_file).expect("Muster");
    let mut block = [0u8; scan::NORM_W * scan::NORM_H];
    for y in 4..16 {
        for x in 0..scan::NORM_W {
            block[y * scan::NORM_W + x] = 1;
        }
    }
    file.add(scan::own::Kind::Taught, '4', &block, None, 24);
    file.save(&own_file).expect("speichern");
    let mut page = SamplePage::with_settings(paths);
    page.open_mining_tab(2);
    page_picture(dir, english, "signatur", Some("hf_bergbau"), page);
    let _ = std::fs::remove_dir_all(tmp);
}

/// The pictures of the pages that need a connection to the Basetool (the unit demand, the unit storage, the
/// connection itself): the window model with the demonstration Basetool of the program, logged in and with the
/// switches of those pages on. Nothing goes out; the folder of the connection is a throwaway one.
fn connected_pictures(dir: &str, english: bool, only: Option<&str>) {
    use versekit_core::basetool::demo::DemoLogin;
    use versekit_core::basetool::scopes::{SETTING_DEMAND, SETTING_ORGSTOCK};
    use versekit_ui::basetool_page::Wire;
    let pages: [(&str, &str); 3] =
        [("bedarf", "hf_bedarf"), ("staffellager", "hf_staffellager"), ("basetool", "k_basetool_verbindung")];
    let folder = std::env::temp_dir().join(format!("vk-bilder-basetool-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    for (name, key) in pages {
        if only.is_some_and(|only| only != name) {
            continue;
        }
        let wire = Wire::in_folder(&folder.to_string_lossy(), false);
        wire.demo.as_ref().expect("Demo-Basetool").script_login(DemoLogin::Approve, 1);
        wire.sync.set_setting(SETTING_DEMAND, versekit_core::jsonio::Value::Bool(true));
        wire.sync.set_setting(SETTING_ORGSTOCK, versekit_core::jsonio::Value::Bool(true));
        wire.sync.connect();
        let end = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !wire.sync.connection().connected() && std::time::Instant::now() < end {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(wire.sync.connection().connected(), "keine Verbindung zum Demo-Basetool");
        wire.sync.pass_now().expect("Abgleich");
        let mut page = SamplePage::preview_still();
        page.connect_wire(wire);
        page_picture(dir, english, name, Some(key), page);
    }
    let _ = std::fs::remove_dir_all(folder);
}

/// The pictures of the page "Darstellung" in each colour scheme, named `screenshot-farbschema-<scheme>.png` as the
/// tour of the website takes them.
fn scheme_pictures(dir: &str, english: bool) {
    for scheme in ["original", "krt", "eis", "nebel", "glut", "kontrast"] {
        assert!(versekit_core::theme::set_current(scheme), "Farbschema {scheme} unbekannt");
        let name = format!("farbschema-{scheme}");
        page_picture(dir, english, &name, Some("hf_darstellung"), SamplePage::preview_still());
    }
}

/// The picture of the overlay: the real window (`overlay::show`) open on made-up finds, running contracts and the
/// status line, drawn at twice the size on a plain dark ground. Named as the website takes it:
/// `screenshot-overlay.png`, in English `screenshot-overlay-en.png`.
fn overlay_picture(dir: &str, english: bool) {
    use versekit_core::blueprints::kind_readable;
    use versekit_core::language::{Arg, Phrase, t, tf};
    use versekit_core::live_contracts::Line as ContractLine;
    use versekit_core::overlay::{BarSide, DEFAULT_WIDTH, Message, Row, Rows, Shown};
    use versekit_ui::overlay::{self, Action, Extra, Tone, View};
    // Ground around the window, in points: the corner marks of the window stand on it.
    const MARGIN: f32 = 12.0;
    // The window as tall as its content needs.
    const HEIGHT: f32 = 272.0;
    let scheme = versekit_core::theme::current();
    let mut rows = Rows::default();
    for row in [
        Row::Found {
            name: "Arclight Pistol".into(),
            kind: kind_readable("WeaponPersonal"),
            code: "S1".into(),
            time: "20:12:33".into(),
            caught_up: false,
        },
        Row::Found {
            name: "Pembroke Helmet".into(),
            kind: kind_readable("Char_Armor_Helmet"),
            code: "S2".into(),
            time: "20:41:07".into(),
            caught_up: false,
        },
        Row::Catalog {
            name: "Zephyr".into(),
            kind: kind_readable("QuantumDrive"),
            time: "20:44:52".into(),
            watch_title: None,
        },
    ] {
        rows.push(row, overlay::DEFAULT_ROWS);
    }
    // The lines of the running contracts as the log reader makes them: the title, then what the catalog says.
    let contract = |title: &str, tail: String| ContractLine {
        title: title.into(),
        text: Message::Plain(tf("ov_a_zeile_mit", &[("0", Arg::from(title)), ("1", Arg::from(tail))])),
        goals: Vec::new(),
        grey: false,
        count: 1,
        complete: false,
    };
    let contracts = vec![
        contract(
            "Retake the Outpost",
            tf("auftrag_fehlt", &[("0", Arg::from(2)), ("1", Arg::from(3)), ("2", Arg::from("Morozov-SH Helmet"))]),
        ),
        contract("Protect Fuel Tanks", tf("auftrag_komplett", &[("0", Arg::from(4)), ("1", Arg::from(4))])),
    ];
    let status = Message::Phrase(Phrase::new("ov_status").with("0", 442).with("1", "20:44:52"));
    let extras: Vec<Extra> = [
        ("bell", "hinweis_versionen", Tone::Normal),
        ("timer-off", "hinweis_aafk_aus", Tone::Normal),
        ("eye", "hinweis_scanner_an", Tone::Lit),
        ("graduation-cap", "hinweis_anlernen", Tone::Normal),
    ]
    .into_iter()
    .map(|(icon, hint, tone)| Extra { action: Action::Extra(icon), icon, meaning: t(hint), id: icon, tone })
    .collect();
    let width = DEFAULT_WIDTH as f32;
    let size = vec2(width + 2.0 * MARGIN, HEIGHT + 2.0 * MARGIN);
    let mut installed = false;
    let mut harness = Harness::builder()
        .with_size(size)
        .with_pixels_per_point(SCALE)
        .with_max_steps(50)
        .with_accessibility_check(false)
        .wgpu()
        .build_ui(move |ui| {
            if !installed {
                versekit_ui::install(ui.ctx());
                installed = true;
                ui.ctx().request_repaint();
                return;
            }
            ui.painter().rect_filled(ui.max_rect(), 0.0, egui::Color32::from_rgb(0x0a, 0x10, 0x1c));
            let view = View {
                scheme,
                shown: Shown::Open,
                rows: &rows,
                contracts: &contracts,
                contracts_max: 10,
                hidden: None,
                refinery: &[],
                bank: &[],
                launcher: true,
                status: &status,
                location: None,
                shard: Some("euw1b"),
                shard_hint: None,
                signature: None,
                demand: None,
                update: None,
                update_days: None,
                zone: None,
                game: true,
                locked: false,
                bar: BarSide::Top,
                anchored: (false, false),
                opacity: 1.0,
                extras: &extras,
                menu: true,
                drag_under_buttons: false,
                hangar: None,
                hints_outside: false,
            };
            let at = Rect::from_min_size(pos2(MARGIN, MARGIN), vec2(width, HEIGHT));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(at));
            child.set_clip_rect(at);
            overlay::show(&mut child, &view);
        });
    harness.run_steps(12);
    harness.remove_cursor();
    harness.run_steps(3);
    let image = (0..5)
        .find_map(|_| {
            harness.render().ok().or_else(|| {
                std::thread::sleep(std::time::Duration::from_millis(800));
                None
            })
        })
        .expect("render");
    let file = format!("{dir}/screenshot-overlay{}.png", if english { "-en" } else { "" });
    image.save(&file).expect("Bild speichern");
    println!("gespeichert: {file}");
}

/// A check picture of an update that has waited three days: the overlay with the game closed (the button can be
/// used) and with the game running (grey), and the bar of the main window above every page (usable, and grey in the
/// game). Marked across the top as a check picture with made-up versions.
fn update_waiting_picture(dir: &str, english: bool) {
    use versekit_core::language::{Arg, t, tf};
    use versekit_core::overlay::{BarSide, DEFAULT_WIDTH, Message, Rows, Shown};
    use versekit_ui::overlay::{self, View};
    use versekit_ui::settings::{UpdateTexts, update_bar};
    const MARGIN: f32 = 12.0;
    const OVERLAY_HEIGHT: f32 = 150.0;
    const BAR_WIDTH: f32 = 760.0;
    let scheme = versekit_core::theme::current();
    let rows = Rows::default();
    let status = Message::Plain(t("ov_spiel_aus"));
    let playing = Message::Plain(t("ov_spiel_an"));
    let days = versekit_core::auto_update::days_text(3);
    let message = tf("s_ul_wartet", &[("version", Arg::from("5.3.0")), ("tage", Arg::from(days))]);
    let (update_word, news_word, blocked) = (t("s_ul_jetzt"), t("was_ist_neu"), t("ov_update_nach_spiel"));
    let width = DEFAULT_WIDTH as f32;
    let size = vec2(MARGIN * 3.0 + width * 2.0, 28.0 + OVERLAY_HEIGHT + 2.0 * 70.0 + 4.0 * MARGIN);
    let mut installed = false;
    let mut harness = Harness::builder()
        .with_size(size)
        .with_pixels_per_point(SCALE)
        .with_max_steps(50)
        .with_accessibility_check(false)
        .wgpu()
        .build_ui(move |ui| {
            if !installed {
                versekit_ui::install(ui.ctx());
                installed = true;
                ui.ctx().request_repaint();
                return;
            }
            ui.painter().rect_filled(ui.max_rect(), 0.0, egui::Color32::from_rgb(0x0a, 0x10, 0x1c));
            // The mark of a check picture across the top.
            let stripe = Rect::from_min_size(pos2(0.0, 0.0), vec2(size.x, 22.0));
            ui.painter().rect_filled(stripe, 0.0, egui::Color32::from_rgb(0xb0, 0x10, 0x10));
            ui.painter().text(
                stripe.center(),
                egui::Align2::CENTER_CENTER,
                "PRÜFBILD · erfundene Versionen (5.3.0 wartet seit 3 Tagen), nicht live",
                egui::FontId::proportional(13.0),
                egui::Color32::WHITE,
            );
            for (column, game) in [(0.0, false), (1.0, true)] {
                let view = View {
                    scheme,
                    shown: Shown::Open,
                    rows: &rows,
                    contracts: &[],
                    contracts_max: 10,
                    hidden: None,
                    refinery: &[],
                    bank: &[],
                    launcher: true,
                    status: if game { &playing } else { &status },
                    location: None,
                    shard: None,
                    shard_hint: None,
                    signature: None,
                    demand: None,
                    update: Some("5.3.0"),
                    update_days: Some(3),
                    zone: None,
                    game,
                    locked: false,
                    bar: BarSide::Top,
                    anchored: (false, false),
                    opacity: 1.0,
                    extras: &[],
                    menu: true,
                    drag_under_buttons: false,
                    hangar: None,
                    hints_outside: false,
                };
                let at = Rect::from_min_size(
                    pos2(MARGIN + column * (width + MARGIN), 28.0 + MARGIN),
                    vec2(width, OVERLAY_HEIGHT),
                );
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(at));
                child.set_clip_rect(at);
                overlay::show(&mut child, &view);
            }
            for (row, grey) in [(0.0, false), (1.0, true)] {
                let texts = UpdateTexts {
                    message: &message,
                    update: &update_word,
                    news: &news_word,
                    update_blocked: grey.then_some(blocked.as_str()),
                    urgent: true,
                };
                let at = Rect::from_min_size(
                    pos2(MARGIN, 28.0 + OVERLAY_HEIGHT + 2.0 * MARGIN + row * 70.0),
                    vec2(BAR_WIDTH, 60.0),
                );
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(at));
                child.painter().rect_filled(at.expand(4.0), 0.0, egui::Color32::from_rgb(0x12, 0x18, 0x24));
                update_bar(&mut child, scheme, &texts);
            }
        });
    harness.run_steps(12);
    // The pointer over the grey button of the bar in the game: its hint says why.
    let grey = harness
        .query_all_by_role_and_label(Role::Button, &t("s_ul_jetzt"))
        .map(|n| n.rect())
        .max_by(|a, b| a.min.y.total_cmp(&b.min.y));
    if let Some(grey) = grey {
        harness.hover_at(grey.center());
        harness.run_steps(40);
    }
    let image = (0..5)
        .find_map(|_| {
            harness.render().ok().or_else(|| {
                std::thread::sleep(std::time::Duration::from_millis(800));
                None
            })
        })
        .expect("render");
    let file = format!("{dir}/PRUEFBILD-update-wartet{}.png", if english { "-en" } else { "-de" });
    image.save(&file).expect("Bild speichern");
    println!("gespeichert: {file}");
}

/// A check picture of the running contracts on a small screen: 1366 × 768 pixels with the task bar below, the
/// largest text that screen allows, two contracts (one with every blueprint had, one with one missing) and three
/// finds. The overlay starts in its default size in the upper left corner and grows as the program lets it
/// ([`versekit_core::overlay::grown`]), as far as the work area goes. Marked across the top as a check picture.
fn contracts_picture(dir: &str, english: bool, all_open: bool) {
    use versekit_core::blueprints::kind_readable;
    use versekit_core::language::{Arg, Phrase, tf};
    use versekit_core::live_contracts::Line as ContractLine;
    use versekit_core::overlay::{BarSide, Corner, Layout, Message, Row, Rows, Shown, grown};
    use versekit_ui::overlay::{self, View};
    // The screen in pixels, the largest text it allows (the zoom of the overlay) and the task bar.
    const SCREEN: (f32, f32) = (1366.0, 768.0);
    const TASK_BAR: f32 = 40.0;
    const STRIPE: f32 = 22.0;
    let points = versekit_core::font_size::max_points_for_screen(SCREEN.0, SCREEN.1);
    let zoom = versekit_core::font_size::zoom_factor(points);
    let scheme = versekit_core::theme::current();
    let mut rows = Rows::default();
    for (name, kind, code, time) in [
        ("Arclight Pistol", "WeaponPersonal", "S1", "20:12:33"),
        ("Pembroke Helmet", "Char_Armor_Helmet", "S2", "20:41:07"),
        ("Morozov-SH Core", "Char_Armor_Torso", "S2", "20:44:52"),
    ] {
        let row = Row::Found {
            name: name.into(),
            kind: kind_readable(kind),
            code: code.into(),
            time: time.into(),
            caught_up: false,
        };
        rows.push(row, overlay::DEFAULT_ROWS);
    }
    let goals = |names: &[&str]| names.iter().map(|g| (*g).to_string()).collect::<Vec<_>>();
    let contracts = vec![
        ContractLine {
            title: "Protect Fuel Tanks".into(),
            text: Message::Plain(tf(
                "ov_a_zeile_mit",
                &[
                    ("0", Arg::from("Protect Fuel Tanks")),
                    ("1", Arg::from(tf("auftrag_komplett", &[("0", Arg::from(3)), ("1", Arg::from(3))]))),
                ],
            )),
            goals: goals(&["Barge-Inverter abholen", "Barge-Inverter zu Port Tressler bringen", "Wachen ausschalten"]),
            grey: false,
            count: 1,
            // With `all_open` both lines keep their objectives: the window has to grow.
            complete: !all_open,
        },
        ContractLine {
            title: "Retake the Outpost".into(),
            text: Message::Plain(tf(
                "ov_a_zeile_mit",
                &[
                    ("0", Arg::from("Retake the Outpost")),
                    (
                        "1",
                        Arg::from(tf(
                            "auftrag_fehlt",
                            &[("0", Arg::from(2)), ("1", Arg::from(3)), ("2", Arg::from("Morozov-SH Helmet"))],
                        )),
                    ),
                ],
            )),
            goals: goals(&["Zum Außenposten fliegen", "Angreifer ausschalten", "Funkturm reparieren"]),
            grey: false,
            count: 1,
            complete: false,
        },
    ];
    let status = Message::Phrase(Phrase::new("ov_status").with("0", 442).with("1", "20:44:52"));
    // In points of the zoomed overlay: the screen, the work area, the place the player chose.
    let size = vec2(SCREEN.0 / zoom, SCREEN.1 / zoom);
    let to_px = |v: f32| (v * zoom).round() as i32;
    let work = versekit_core::overlay::Rect::new(0, 0, SCREEN.0 as i32, (SCREEN.1 - TASK_BAR) as i32);
    let layout = Layout { corner: Corner::TopLeft, bar: BarSide::Top };
    let mut grow = 0i32;
    let mut base = None;
    let mut installed = false;
    let mut harness = Harness::builder()
        .with_size(size)
        .with_pixels_per_point(zoom)
        .with_max_steps(80)
        .with_accessibility_check(false)
        .wgpu()
        .build_ui(move |ui| {
            if !installed {
                versekit_ui::install(ui.ctx());
                installed = true;
                ui.ctx().request_repaint();
                return;
            }
            // The whole picture is the screen: the ground goes under the margins of the frame too.
            let screen = ui.ctx().viewport_rect();
            let painter = ui.painter().with_clip_rect(screen);
            painter.rect_filled(screen, 0.0, egui::Color32::from_rgb(0x0a, 0x10, 0x1c));
            let bar = Rect::from_min_max(pos2(0.0, size.y - TASK_BAR / zoom), size.to_pos2());
            painter.rect_filled(bar, 0.0, egui::Color32::from_rgb(0x20, 0x24, 0x2c));
            // The place the player chose: the default size of this text, under the mark of the check picture.
            let chosen = *base.get_or_insert_with(|| {
                let default = overlay::default_size(ui);
                let (x, y) = (versekit_core::overlay::EDGE, to_px(STRIPE / zoom) + versekit_core::overlay::EDGE);
                versekit_core::overlay::Rect::new(x, y, to_px(default.x), to_px(default.y))
            });
            let window = grown(layout, chosen, grow, work);
            let view = View {
                scheme,
                shown: Shown::Open,
                rows: &rows,
                contracts: &contracts,
                contracts_max: 10,
                hidden: None,
                refinery: &[],
                bank: &[],
                launcher: true,
                status: &status,
                location: None,
                shard: Some("euw1b"),
                shard_hint: None,
                signature: None,
                demand: None,
                update: None,
                update_days: None,
                zone: None,
                game: true,
                locked: false,
                bar: BarSide::Top,
                anchored: layout.anchored(),
                opacity: 1.0,
                extras: &[],
                menu: true,
                drag_under_buttons: false,
                hangar: None,
                hints_outside: false,
            };
            let at = Rect::from_min_size(
                pos2(window.x as f32 / zoom, window.y as f32 / zoom),
                vec2(window.w as f32 / zoom, window.h as f32 / zoom),
            );
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(at));
            child.set_clip_rect(at);
            let out = overlay::show(&mut child, &view);
            // As the program does it: the window takes what the contracts lack, up to the edge of the work area.
            if let Some(need) = out.contracts_need {
                let want = (grow + (need * zoom).round() as i32)
                    .clamp(0, versekit_core::overlay::growth_room(chosen, work));
                if (want - grow).abs() > 1 {
                    grow = want;
                    ui.ctx().request_repaint();
                }
            }
            let stripe = Rect::from_min_size(pos2(0.0, 0.0), vec2(size.x, STRIPE / zoom));
            ui.painter().rect_filled(stripe, 0.0, egui::Color32::from_rgb(0xb0, 0x10, 0x10));
            ui.painter().text(
                stripe.center(),
                egui::Align2::CENTER_CENTER,
                format!("PRÜFBILD NACHHER · 1366×768, Schrift {points} (größte für diesen Bildschirm), erfundene Aufträge"),
                egui::FontId::proportional(12.0 / zoom),
                egui::Color32::WHITE,
            );
        });
    harness.run_steps(40);
    harness.remove_cursor();
    harness.run_steps(3);
    let image = (0..5)
        .find_map(|_| {
            harness.render().ok().or_else(|| {
                std::thread::sleep(std::time::Duration::from_millis(800));
                None
            })
        })
        .expect("render");
    let kind = if all_open { "-aufgeklappt" } else { "" };
    let file = format!("{dir}/PRUEFBILD-auftraege-1366{kind}{}.png", if english { "-en" } else { "-de" });
    image.save(&file).expect("Bild speichern");
    println!("gespeichert: {file}");
}

fn main() {
    // `--en` draws in English; the other words are the folder, the scheme, the kind of pictures, a search word.
    let mut words: Vec<String> = std::env::args().skip(1).collect();
    let english = words.iter().any(|w| w == "--en");
    // `--markt=<folder>`: where `echt` copies the price lists of the market from (public data, read only).
    let market = words.iter().find_map(|w| w.strip_prefix("--markt=")).map(std::path::PathBuf::from);
    words.retain(|w| w != "--en" && w != "--de" && !w.starts_with("--markt="));
    let arg = |n: usize| words.get(n - 1).cloned();
    let dir = arg(1).expect("Zielordner");
    let scheme = arg(2).unwrap_or_else(|| "krt".into());
    let what = arg(3).unwrap_or_else(|| "alle".into());
    let all = what == "alle";
    versekit_core::language::set_language(if english {
        versekit_core::language::Choice::En
    } else {
        versekit_core::language::Choice::De
    });
    versekit_core::theme::set_current(&scheme);

    // The real setup assistant, step by step: the full one for a new program, the short one for a program set
    // up with 4.12.
    if what == "einrichtung" || what == "willkommen" {
        assistant_steps(dir, &scheme, what == "willkommen", english);
        return;
    }

    if what == "galerie" {
        gallery_pictures(&dir, english);
        return;
    }

    if what == "webseite" {
        website_pictures(&dir, english, arg(4).as_deref());
        return;
    }

    if what == "verbunden" {
        connected_pictures(&dir, english, arg(4).as_deref());
        return;
    }

    if what == "farbschemata" {
        scheme_pictures(&dir, english);
        return;
    }

    if what == "echt" {
        real_pictures(&dir, english, arg(4).as_deref(), market.as_deref());
        return;
    }

    if what == "signatur" {
        signature_picture(&dir, english);
        return;
    }

    if what == "pruefung" {
        review_pictures(&dir);
        return;
    }

    if what == "auftraege" {
        contracts_picture(&dir, english, false);
        contracts_picture(&dir, english, true);
        return;
    }

    if what == "overlay" {
        overlay_picture(&dir, english);
        zone_picture(&dir, english);
        return;
    }

    // The page "Kartellbank" in the window model, with the made-up bank of the sample page.
    if what == "kartellbank" {
        website_pictures(&dir, english, Some("kartellbank"));
        return;
    }

    if what == "updatewartet" {
        update_waiting_picture(&dir, english);
        return;
    }

    if what == "assistent" {
        // The wizard lives in the catalog of the full sample page.
        let mut s = Shots { dir, harness: harness(SamplePage::new(), vec2(1300.0, 2400.0)) };
        s.steps(6);
        let head = s.first_text(WIZARD);
        let card = Rect::from_min_size(pos2(0.0, head.min.y - 8.0), vec2(1300.0, 200.0));
        s.snap("x_assistent_1_start", Some(card));
        let confirm = s.first_named("Bestätigen");
        s.click(confirm.center());
        s.snap("x_assistent_2_bestaetigt", Some(card));
        // Jump to "Fertig" with steps still open.
        let done = s.first_named("Fertig");
        s.click(done.center());
        s.snap("x_assistent_3_fertig_gesperrt", Some(card));
        return;
    }

    let mut s = Shots { dir, harness: harness(SamplePage::preview(), WINDOW) };
    s.steps(6);
    if what == "seite" {
        let word = arg(4).expect("Suchwort");
        s.filter(&word);
        // The first hit below the field.
        let field = s.filter_field();
        let hit = s
            .harness
            .query_all_by_role(Role::Label)
            .filter(|n| n.rect().min.y > field.max.y && n.rect().max.x < field.max.x + 40.0)
            .filter(|n| n.value().is_some_and(|v| v.to_lowercase().contains(&word.to_lowercase())))
            .map(|n| n.rect())
            .min_by(|a, b| a.min.y.total_cmp(&b.min.y))
            .expect("kein Treffer");
        s.click(hit.center());
        s.steps(8);
        s.snap(&format!("z_seite_{word}"), None);
    }
    if all || what == "uebersicht" {
        s.snap(&format!("1_uebersicht_{scheme}"), None);
    }
    if all || what == "kachel" {
        // Hold the "Mein Lager" tile over the "Bauplan-Liste" tile.
        let from = s.first_text("Mein Lager").center();
        let to = s.first_text("Bauplan-Liste").center();
        s.hold_and_move(from, to);
        s.snap(&format!("2_kachel_ziehen_{scheme}"), None);
        s.release(to);
    }
    if all || what == "gruppen" {
        // Fold the first two groups so more show, then drag the fourth one up.
        let heads = s.named(GROUP_HEAD);
        s.click(heads[0].center());
        let heads = s.named(GROUP_HEAD);
        s.click(heads[1].center());
        let heads = s.named(GROUP_HEAD);
        let from = heads[3].center();
        let to = pos2(from.x, heads[0].min.y + 2.0);
        s.hold_and_move(from, to);
        s.snap(&format!("3_gruppe_ziehen_{scheme}"), None);
        s.release(to);
        s.snap(&format!("3b_gruppe_oben_{scheme}"), None);
    }
    if all || what == "filter" {
        s.filter("overlay");
        s.snap(&format!("4_menue_filter_{scheme}"), None);
        // Open the hit "Overlay › Wo das Overlay sitzt", scroll the page so
        // the whole screen model shows.
        let hit = s.first_text("Overlay › Wo das Overlay sitzt");
        s.click(hit.center());
        s.steps(4);
        s.hover(pos2(WINDOW.x * 0.65, WINDOW.y * 0.6));
        s.harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, -110.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: egui::Modifiers::NONE,
        });
        s.steps(8);
        s.snap(&format!("5_overlay_platz_{scheme}"), None);
    }
}
