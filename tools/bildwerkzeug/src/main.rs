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
//! the README), echt (the pages that read the game folder and the data folder, on a made-up installation in a
//! throwaway folder, the hangar with the slots of the game data among them; `--markt=<folder>` gives the price
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
    use versekit_core::legacy::{ENV_HOME, ENV_NO_NET};
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
    let pages: [(&str, Option<&str>); 46] = [
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
        ("bedarf", Some("hf_bedarf")),
        ("einkaufsliste", Some("hf_einkaufsliste")),
        ("contested", Some("hf_contested")),
        ("statistik", Some("hf_st_uebersicht")),
        ("statistik-schiffe", Some("hf_st_schiffe")),
        ("statistik-auftraege", Some("hf_st_auftraege")),
        ("statistik-quantum", Some("hf_st_quantum")),
        ("statistik-stabil", Some("hf_st_stabil")),
        ("allgemein", Some("hf_allgemein")),
        ("anzeige", Some("hf_overlay")),
        ("darstellung", Some("hf_darstellung")),
        ("ordner", Some("hf_ordner")),
        ("module", Some("hf_module")),
        ("erkennung", Some("hf_erkennung")),
        ("startprogramme", Some("hf_startprogramme")),
        ("basetool", Some("k_basetool_verbindung")),
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
        page_picture(dir, english, name, key, SamplePage::preview_still());
    }
}

/// What a page gets done before its picture, so that it shows what it is for: a search typed into its field.
fn touch(name: &str, english: bool, s: &mut Shots) {
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
    if name == "hangar" {
        // The loadout of the first ship unfolded: its slots from the game data.
        let title = versekit_core::language::t("s_wk_titel");
        match s.texts(&title).into_iter().chain(s.named(&title)).next() {
            Some(fold) => {
                s.click(fold.center());
                s.steps(8);
                // The page scrolled so that the card of the ship stands near the top: the slots fill the picture.
                s.hover(pos2(fold.center().x + 400.0, fold.center().y));
                s.harness.event(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: vec2(0.0, -(fold.min.y - 235.0).max(0.0)),
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
        "laeden" => "CF-337 Panther",
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
        // A page that only lists shows more with the first entry open.
        if name == "herstellung" {
            let first = s
                .harness
                .query_all_by(|n| n.label().or_else(|| n.value()).is_some_and(|v| v.contains("Frostline")))
                .map(|n| n.rect())
                .next();
            match first {
                Some(first) => s.click(first.center()),
                None => println!("Herstellung: Frostline nicht gefunden, Seite ohne Auswahl"),
            }
            s.steps(8);
        }
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
        let small = image::imageops::resize(&image, 1400, 860, image::imageops::FilterType::Lanczos3);
        let file = format!("{}/screenshot-{name}{}.png", s.dir, if english { "-en" } else { "" });
        small.save(&file).expect("Bild speichern");
        println!("gespeichert: {file}");
    }
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
    let pages: [(&str, &str); 18] = [
        ("hangar", "hf_hangar"),
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
        ("erkennung", "hf_erkennung"),
        ("startprogramme", "hf_startprogramme"),
        ("laeden", "hf_laeden"),
        ("verkauf", "hf_verkauf"),
        ("bestand", "hf_sichern"),
        ("ueber", "hf_ueber"),
    ];
    let (tmp, host) = install::build(SETTINGS_OF_4_12, market);
    for (name, key) in pages {
        if only.is_some_and(|only| only != name) {
            continue;
        }
        let paths = versekit_core::paths::Paths::new(host.clone());
        page_picture(dir, english, name, Some(key), SamplePage::with_settings(paths));
    }
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
        text: Message::Plain(tf(
            "ov_a_zeile_mit",
            &[("0", Arg::from(tf("auftrag_zeile", &[("0", Arg::from(title))]))), ("1", Arg::from(tail))],
        )),
        goals: Vec::new(),
        grey: false,
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
                launcher: true,
                status: &status,
                location: None,
                shard: Some("euw1b"),
                shard_hint: None,
                signature: None,
                demand: None,
                update: None,
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

    if what == "pruefung" {
        review_pictures(&dir);
        return;
    }

    if what == "overlay" {
        overlay_picture(&dir, english);
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
