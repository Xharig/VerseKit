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
//! webseite (one picture of the whole window per page of the website, 1400 × 860).
//! `--en` draws in English (default German).
//! Targets are found by their names and roles, not by fixed points, so the
//! images keep working when the sample page changes.
use egui::accesskit::Role;
use egui::{Pos2, Rect, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use versekit_ui::sample::SamplePage;

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
    let mut dir = root.map_or_else(std::env::temp_dir, |r| r).join(&name);
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
fn website_pictures(dir: &str, english: bool) {
    const SIZE: egui::Vec2 = vec2(1400.0, 860.0);
    let pages: [(&str, Option<&str>); 15] = [
        ("uebersicht", None),
        ("liste", Some("hf_liste")),
        ("auftragstexte", Some("hf_spiel")),
        ("herstellung", Some("hf_herstellung")),
        ("raffinerien", Some("hf_raffinerien")),
        ("bergbau", Some("hf_bergbau")),
        ("hangar", Some("hf_hangar")),
        ("meinlager", Some("hf_meinlager")),
        ("einkaufsliste", Some("hf_einkaufsliste")),
        ("routen", Some("hf_routen")),
        ("verkauf", Some("hf_verkauf")),
        ("statistik", Some("hf_st_uebersicht")),
        ("joysticks", Some("hf_joysticks")),
        ("karten", Some("hf_contested")),
        ("handlesuche", Some("hf_handlesuche")),
    ];
    for (name, key) in pages {
        let mut page = SamplePage::preview_still();
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
                        let group = versekit_ui::sample::group_of_page(key).expect("Seite im Menü");
                        page.open_page(ui.ctx(), key, group);
                    }
                    opened = true;
                }
                page.show(ui);
            });
        let mut s = Shots { dir: dir.to_string(), harness };
        s.steps(12);
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

fn main() {
    // `--en` draws in English; the other words are the folder, the scheme, the kind of pictures, a search word.
    let mut words: Vec<String> = std::env::args().skip(1).collect();
    let english = words.iter().any(|w| w == "--en");
    words.retain(|w| w != "--en" && w != "--de");
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
        website_pictures(&dir, english);
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
