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

//! The page "Startprogramme" (`start_programs_page` of 4.12): further programs that start together
//! with the game, an entry of its own in the settings group of the menu.
//!
//! Everything is saved at once, there is no save button (like every setting of the program): a switch
//! and a choice at the click, a text field as soon as it is left (or Enter is pressed) or the page
//! comes back without the click that would have left it: whatever stands in an unfocused field is
//! what is stored.
//!
//! ⚠⚠ Exactly what the player enters is run: no preset entries, nothing from the network. The page says
//! so at its foot ("auf eigenes Risiko"). "Jetzt starten" goes through the [`Spawner`] given to the
//! page; a check gives it a recorder, so no test ever starts a program.
//!
//! A backup that is restored never switches on a command it brings along: see
//! `start_programs::safe_import`.

use std::sync::Arc;

use egui::{RichText, Ui, WidgetInfo};
use versekit_core::game_start::Spawner;
use versekit_core::jsonio::Value;
use versekit_core::language::{Arg, t, tf};
use versekit_core::paths::Paths;
use versekit_core::start_programs::{self, Entry, When};
use versekit_core::theme::Scheme;

use crate::file_dialog::{self, Pending};
use crate::settings::Said;
use crate::widgets::{self, ButtonKind};
use crate::{color, layout, settings};

/// The system dialog to choose a file (a check puts an answer in its place).
pub type ChooseFile = fn(&egui::Context, &str, Option<&str>) -> Pending;

/// The moments and the words of their name; the hint of a moment is its key with `_h`.
const WHEN_KEYS: [(When, &str); 3] =
    [(When::Launcher, "s_pg_w_launcher"), (When::Game, "s_pg_w_spiel"), (When::Replaces, "s_pg_w_ersetzt")];

/// Longest time to wait the page offers (a day); a longer one in the file stays until it is changed.
const WAIT_MOST: i64 = 86_400;
/// The least width of a card of a program before the cards stand under each other.
const PROGRAM_CELL: f32 = 520.0;

/// One entry with the texts of its fields (kept while they are typed).
struct Item {
    /// Tells the card from the others through adding and removing.
    uid: u64,
    entry: Entry,
    name: String,
    file: String,
    args: String,
}

impl Item {
    fn new(uid: u64, entry: Entry) -> Item {
        Item { uid, name: entry.name.clone(), file: entry.file.clone(), args: entry.args.clone(), entry }
    }

    /// Takes over what stands in the text fields (without blanks around it); true if that changed the entry.
    fn apply_texts(&mut self) -> bool {
        let (name, file, args) = (self.name.trim(), self.file.trim(), self.args.trim());
        let changed = name != self.entry.name || file != self.entry.file || args != self.entry.args;
        self.entry.name = name.to_string();
        self.entry.file = file.to_string();
        self.entry.args = args.to_string();
        changed
    }
}

/// What the page keeps.
pub struct StartProgramsState {
    items: Vec<Item>,
    main_on: bool,
    /// Read from the settings (false: read at the next frame).
    loaded: bool,
    next_uid: u64,
    /// The file dialog that is open: the card it belongs to and the dialog.
    dialog: Option<(u64, Pending)>,
    choose: ChooseFile,
    /// The line for the foot of the window (a notice).
    notice: Option<Said>,
    /// A program that was removed a moment ago: since when, where it stood, the card (to put it back).
    undo: Option<(f64, usize, Item)>,
    /// When the list was written last (for the mark "Gespeichert").
    saved_at: Option<f64>,
    /// The time of the frame being drawn.
    now: f64,
}

impl Default for StartProgramsState {
    fn default() -> StartProgramsState {
        StartProgramsState::with_dialog(choose_any_file)
    }
}

/// The system dialog for a file of any kind.
fn choose_any_file(ctx: &egui::Context, title: &str, start: Option<&str>) -> Pending {
    file_dialog::open_file(ctx, title, &[], start)
}

impl StartProgramsState {
    pub fn with_dialog(choose: ChooseFile) -> StartProgramsState {
        StartProgramsState {
            items: Vec::new(),
            main_on: true,
            loaded: false,
            next_uid: 1,
            dialog: None,
            choose,
            notice: None,
            undo: None,
            saved_at: None,
            now: 0.0,
        }
    }

    /// The settings were read again (a backup, another window): the list is read again at the next frame.
    pub fn invalidate(&mut self) {
        self.loaded = false;
    }

    /// The line for the foot, once.
    pub fn take_notice(&mut self) -> Option<Said> {
        self.notice.take()
    }

    /// Takes over what stands in the text fields and writes the list if that changed anything: what was
    /// typed must not be lost when the page goes away or the list is read again.
    pub fn flush(&mut self, paths: &Paths) {
        let mut changed = false;
        for item in &mut self.items {
            changed |= item.apply_texts();
        }
        if changed {
            self.save(paths);
        }
    }

    fn load(&mut self, paths: &Paths) {
        self.flush(paths);
        self.main_on = start_programs::enabled(paths);
        self.items.clear();
        for entry in start_programs::entries(paths) {
            self.next_uid += 1;
            self.items.push(Item::new(self.next_uid, entry));
        }
        self.loaded = true;
    }

    /// Writes the list; says so when it could not be written.
    fn save(&mut self, paths: &Paths) {
        let entries: Vec<Entry> = self.items.iter().map(|item| item.entry.clone()).collect();
        if start_programs::save(paths, &entries) {
            self.saved_at = Some(self.now);
        } else {
            self.notice = Some(Said::fault(t("s_hl_fehler")));
        }
    }
}

/// What the player did at one card in a frame.
#[derive(Default)]
struct Did {
    changed: bool,
    browse: bool,
    run: bool,
    remove: bool,
}

/// A line of a field: its name above, the field below, named by it for screen readers. Returns whether the
/// field has the focus (a field without it is taken over by the caller) and whether the button beside it was
/// clicked.
fn field(
    ui: &mut Ui,
    scheme: &Scheme,
    salt: &str,
    label: &str,
    text: &mut String,
    hint: &str,
    button: Option<&str>,
) -> (bool, bool) {
    ui.add_space(4.0);
    let name = ui.label(RichText::new(label).small().color(color(scheme.sub)));
    let (mut focused, mut browse) = (false, false);
    ui.horizontal(|ui| {
        let reserved = button.map_or(0.0, |word| {
            widgets::button_size(ui, scheme, ButtonKind::Normal, word).x + ui.spacing().item_spacing.x
        });
        // Never wider than a field needs to be: in a wide window its button stays right beside it.
        let width = (ui.available_width() - reserved - layout::SAFETY).clamp(80.0, widgets::FIELD_MAX_WIDTH);
        let response =
            ui.add(widgets::text_field(text, hint, scheme).id_salt(salt).desired_width(width)).labelled_by(name.id);
        focused = response.has_focus();
        if let Some(word) = button {
            browse = ui.add(widgets::Button::new(word, ButtonKind::Normal, scheme)).clicked();
        }
    });
    (focused, browse)
}

/// A switch in the right of a setting row, named for screen readers.
fn switch(row: &mut Ui, scheme: &Scheme, on: &mut bool, name: &str) -> bool {
    let switch = row.add(widgets::Toggle::new(on, scheme));
    let (enabled, now) = (switch.enabled(), *on);
    switch.widget_info(|| WidgetInfo::selected(egui::Role::CheckBox, enabled, now, name));
    switch.changed()
}

/// One card: switch with name and file, the fields, the moment, end with the game, the two buttons.
fn card(ui: &mut Ui, scheme: &Scheme, item: &mut Item) -> Did {
    let mut did = Did::default();
    let title = if !item.entry.name.is_empty() {
        item.entry.name.clone()
    } else if let Some(base) = item.entry.file.rsplit(['/', '\\']).next().filter(|b| !b.is_empty()) {
        base.to_string()
    } else {
        t("s_pg_neuer")
    };
    let sub = if item.entry.file.is_empty() { t("s_pg_keine_datei") } else { item.entry.file.clone() };
    widgets::card(ui, scheme, |ui| {
        settings::setting_row(ui, scheme, &title, &sub, false, |row| {
            did.changed |= switch(row, scheme, &mut item.entry.on, &title);
        });
        // A field that has no focus shows what is stored: whatever was typed is taken over (the focus is
        // gone with a click elsewhere, with Enter, and with the page being away).
        let (focused, _) = field(ui, scheme, "name", &t("s_pg_name"), &mut item.name, &t("s_pl_programmname"), None);
        let (file_focused, browse) = field(
            ui,
            scheme,
            "datei",
            &t("s_pg_datei"),
            &mut item.file,
            &t("s_pl_programmdatei"),
            Some(&t("s_pg_suchen")),
        );
        did.browse = browse;
        let (args_focused, _) =
            field(ui, scheme, "argumente", &t("s_pg_argumente"), &mut item.args, &t("s_pg_keine"), None);
        if !(focused || file_focused || args_focused) {
            did.changed |= item.apply_texts();
            item.name.clone_from(&item.entry.name);
            item.file.clone_from(&item.entry.file);
            item.args.clone_from(&item.entry.args);
        }
        ui.add_space(4.0);
        let wait_label = ui.label(RichText::new(t("s_pg_warten")).small().color(color(scheme.sub)));
        did.changed |= widgets::number_field_labelled(
            ui,
            scheme,
            &format!("warten-{}", item.uid),
            &mut item.entry.wait,
            0..=WAIT_MOST,
            &t("s_pg_sekunden"),
            Some(wait_label.id),
        );
        ui.add_space(6.0);
        ui.label(RichText::new(t("s_pg_wann")).small().color(color(scheme.sub)));
        for (when, key) in WHEN_KEYS {
            if widgets::radio(ui, scheme, item.entry.when == when, &t(key)).clicked() && item.entry.when != when {
                item.entry.when = when;
                did.changed = true;
            }
        }
        if let Some((_, key)) = WHEN_KEYS.iter().find(|(when, _)| *when == item.entry.when) {
            ui.label(RichText::new(t(&format!("{key}_h"))).small().color(color(scheme.sub)));
        }
        ui.add_space(6.0);
        let end = t("s_pg_beenden");
        settings::setting_row(ui, scheme, &end, "", false, |row| {
            did.changed |= switch(row, scheme, &mut item.entry.end, &format!("{title}: {end}"));
        });
        // The buttons of the card together in one line; a wider gap before the red one keeps a mis-click away.
        ui.horizontal(|ui| {
            did.run = ui.add(widgets::Button::new(&t("s_pg_jetzt"), ButtonKind::Normal, scheme)).clicked();
            ui.add_space(3.0 * layout::GAP);
            did.remove = ui.add(widgets::Button::new(&t("s_pg_entfernen"), ButtonKind::Danger, scheme)).clicked();
        });
    });
    did
}

/// Draws the page; every change is saved at once.
pub fn show_page(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &mut StartProgramsState,
    paths: &Paths,
    spawner: &Arc<dyn Spawner>,
) {
    state.now = ui.input(|i| i.time);
    if !state.loaded {
        state.load(paths);
    }
    ui.label(RichText::new(t("s_pg_lead")).small().color(color(scheme.sub)));
    ui.add_space(8.0);

    // The answer of a file dialog goes into the card it was opened for.
    if let Some((uid, pending)) = &state.dialog
        && let Some(answer) = pending.take()
    {
        let uid = *uid;
        state.dialog = None;
        if let Some(path) = answer
            && let Some(item) = state.items.iter_mut().find(|item| item.uid == uid)
        {
            item.file.clone_from(&path);
            item.entry.file = path;
            state.save(paths);
        }
    }

    // The main card: the title with the saved mark, the main switch (off, nothing starts and the list stays)
    // and, with no program yet, what to do.
    let name = t("s_pg_aktiv");
    let mut flipped = false;
    widgets::card(ui, scheme, |ui| {
        // The title line is always as tall, so that nothing below moves when the mark shows.
        ui.horizontal(|ui| {
            ui.set_min_height(22.0);
            widgets::card_title(ui, scheme, &t("s_pg_kopf"));
            ui.add_space(8.0);
            widgets::saved_hint(ui, scheme, state.saved_at, &t("s_pg_gespeichert"));
        });
        ui.add_space(4.0);
        settings::setting_row(ui, scheme, &name, &t("s_pg_aktiv_h"), false, |row| {
            flipped = switch(row, scheme, &mut state.main_on, &name);
        });
        if state.items.is_empty() {
            ui.label(RichText::new(t("s_pg_leer")).small().color(color(scheme.sub)));
        }
    });
    if flipped {
        let word = if state.main_on { t("e_an") } else { t("e_aus") };
        state.notice = Some(if paths.set_setting(start_programs::SETTING_ON, Value::Bool(state.main_on)) {
            Said::note(format!("{name}: {word}"))
        } else {
            Said::fault(t("s_hl_fehler"))
        });
    }
    ui.add_space(8.0);

    let (mut save, mut remove, mut opened) = (false, None, None);
    // In a wide window the cards of the programs stand side by side, up to three in a row.
    let count = state.items.len();
    let columns = layout::columns_that_fit(ui.available_width(), PROGRAM_CELL, 3);
    let mut drawn: Vec<(usize, Did)> = Vec::with_capacity(count);
    for first in (0..count).step_by(columns) {
        let items = &mut state.items;
        layout::columns_or_stack(ui, PROGRAM_CELL, columns, |ui, column| {
            let index = first + column;
            if index < count {
                let uid = items[index].uid;
                drawn.push((index, ui.push_id(("startprogramm", uid), |ui| card(ui, scheme, &mut items[index])).inner));
            }
        });
        ui.add_space(8.0);
    }
    for (index, did) in drawn {
        save |= did.changed;
        if did.browse {
            opened = Some(state.items[index].uid);
        }
        if did.remove {
            remove = Some(index);
        }
        if did.run {
            let entry = state.items[index].entry.clone();
            state.notice = Some(match start_programs::launch(&entry, Arc::clone(spawner), paths.host.flavor, false) {
                Ok(()) => Said::note(t("s_pg_gestartet")),
                Err(error) => Said::fault(tf("s_pg_nicht", &[("0", Arg::from(error.reason().as_str()))])),
            });
        }
    }
    if let Some(index) = remove {
        let item = state.items.remove(index);
        state.undo = Some((state.now, index, item));
        save = true;
    }
    if let Some(uid) = opened {
        let start = state
            .items
            .iter()
            .find(|item| item.uid == uid)
            .map(|item| paths.host.flavor.dirname(&item.entry.file))
            .filter(|folder| !folder.is_empty());
        state.dialog = Some((uid, (state.choose)(ui.ctx(), &t("s_pg_programm_waehlen"), start.as_deref())));
    }
    // A removed program can be taken back for a few seconds, to the place where it stood.
    if let Some((since, index, item)) = state.undo.take() {
        let word = t("s_mlg_rueckgaengig");
        let shown = widgets::notice(ui.ctx(), scheme, since, "trash-2", &t("s_pg_entfernt"), Some(&word));
        if shown.acted {
            state.items.insert(index.min(state.items.len()), item);
            save = true;
        } else if shown.shown {
            state.undo = Some((since, index, item));
        }
    }
    if save {
        state.save(paths);
    }
    // The closing card: the button for one more program and the risk.
    let add = widgets::card(ui, scheme, |ui| {
        widgets::card_title(ui, scheme, &t("s_pg_neu_kopf"));
        ui.add_space(4.0);
        let add = ui.add(widgets::Button::new(&t("s_pg_neu"), ButtonKind::Strong, scheme)).clicked();
        ui.add_space(10.0);
        ui.label(RichText::new(t("s_pg_risiko")).small().color(color(scheme.sub)));
        add
    })
    .inner;
    if add {
        state.next_uid += 1;
        state.items.push(Item::new(state.next_uid, Entry::default()));
        state.save(paths);
    }
}

#[cfg(test)]
mod tests {
    use egui::{Event, Key, Pos2, Rect};
    use versekit_core::game_start::Recorder;
    use versekit_core::language::{self, Choice};
    use versekit_core::paths::Host;
    use versekit_core::pypath::Flavor;
    use versekit_core::theme;

    use super::*;

    fn scratch(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("versekit-startpage-{}-{name}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        dir.to_string_lossy().replace('\\', "/")
    }

    fn paths_in(home: &str) -> Paths {
        Paths::new(Host::new(Flavor::Posix, &[("SC_BP_HOME", home), ("HOME", home)], home))
    }

    /// The answer of the stand-in for the file dialog.
    const CHOSEN: &str = "/opt/tool/tool.exe";

    fn chooses(_: &egui::Context, _: &str, _: Option<&str>) -> Pending {
        Pending::answered(Some(CHOSEN.to_string()))
    }

    fn cancels(_: &egui::Context, _: &str, _: Option<&str>) -> Pending {
        Pending::answered(None)
    }

    type Texts = Vec<(String, Pos2)>;
    type Named = Vec<(egui::Role, String, Rect)>;

    struct Bench {
        ctx: egui::Context,
        state: StartProgramsState,
        paths: Paths,
        recorder: Arc<Recorder>,
        spawner: Arc<dyn Spawner>,
        home: String,
    }

    impl Bench {
        fn new(name: &str) -> Bench {
            let ctx = egui::Context::default();
            crate::install(&ctx);
            ctx.enable_accesskit();
            let home = scratch(name);
            let recorder = Arc::new(Recorder::default());
            let spawner: Arc<dyn Spawner> = recorder.clone();
            Bench {
                ctx,
                state: StartProgramsState::with_dialog(chooses),
                paths: paths_in(&home),
                recorder,
                spawner,
                home,
            }
        }

        /// One frame: the texts drawn and the named elements with their place (role, name, rect).
        fn frame(&mut self, events: Vec<Event>) -> (Texts, Named) {
            let scheme = theme::by_id("original").expect("original");
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 1600.0))),
                events,
                ..Default::default()
            };
            let (state, paths, spawner) = (&mut self.state, &self.paths, &self.spawner);
            let mut output = self.ctx.run_ui(input, |ui| show_page(ui, scheme, state, paths, spawner));
            output.textures_delta.clear();
            let mut texts = Vec::new();
            fn collect(shape: &egui::Shape, into: &mut Texts) {
                match shape {
                    egui::Shape::Text(text) => into.push((text.galley.text().to_string(), text.pos)),
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, into)),
                    _ => {}
                }
            }
            output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
            let nodes = output
                .platform_output
                .accesskit_update
                .map(|update| {
                    update
                        .nodes
                        .iter()
                        .filter_map(|(_, node)| {
                            let b = node.bounds()?;
                            let rect = Rect::from_min_max(
                                Pos2::new(b.x0 as f32, b.y0 as f32),
                                Pos2::new(b.x1 as f32, b.y1 as f32),
                            );
                            Some((node.role(), node.label().unwrap_or_default().to_string(), rect))
                        })
                        .collect()
                })
                .unwrap_or_default();
            (texts, nodes)
        }

        fn settle(&mut self) -> (Texts, Named) {
            let mut last = (Vec::new(), Vec::new());
            for _ in 0..4 {
                last = self.frame(Vec::new());
            }
            last
        }

        fn press_at(&mut self, at: Pos2) {
            let button = |pressed| Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            self.frame(vec![Event::PointerMoved(at)]);
            self.frame(vec![button(true)]);
            self.frame(vec![button(false)]);
            self.settle();
        }

        /// Clicks the text (or the first of several) drawn as `label`.
        fn click(&mut self, label: &str) {
            let texts = self.settle().0;
            let at = texts
                .iter()
                .find(|(text, _)| text == label)
                .map(|(_, at)| *at + egui::vec2(3.0, 3.0))
                .unwrap_or_else(|| panic!("{label} is not on the screen: {texts:?}"));
            self.press_at(at);
        }

        /// Clicks the element of that role with that name (a switch has no text of its own).
        fn click_named(&mut self, role: egui::Role, name: &str) {
            let nodes = self.settle().1;
            let rect = nodes
                .iter()
                .find(|(r, n, _)| *r == role && n == name)
                .map(|(_, _, rect)| *rect)
                .unwrap_or_else(|| panic!("no {role:?} `{name}`: {nodes:?}"));
            self.press_at(rect.center());
        }

        /// Types into the field that shows `hint`, without leaving it.
        fn type_only(&mut self, hint: &str, text: &str) {
            self.click(hint);
            self.frame(vec![Event::Text(text.to_string())]);
            self.frame(Vec::new());
        }

        /// Types into the field that shows `hint` and presses Enter.
        fn type_into(&mut self, hint: &str, text: &str) {
            self.type_only(hint, text);
            let key = |pressed| Event::Key {
                key: Key::Enter,
                physical_key: Some(Key::Enter),
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            };
            self.frame(vec![key(true)]);
            self.frame(vec![key(false)]);
            self.settle();
        }

        fn stored(&self) -> Vec<Entry> {
            start_programs::entries(&self.paths)
        }

        fn file_in_home(&self, name: &str) -> String {
            let path = format!("{}/{name}", self.home);
            std::fs::write(&path, "x").expect("file");
            path
        }
    }

    fn shows(texts: &[(String, Pos2)], text: &str) -> bool {
        texts.iter().any(|(t, _)| t.contains(text))
    }

    fn setup() -> std::sync::MutexGuard<'static, ()> {
        let guard = crate::language_lock();
        theme::set_current("original");
        language::set_thread_language(Choice::De);
        guard
    }

    #[test]
    fn an_entry_is_added_filled_chosen_and_removed_and_every_step_is_in_the_file_at_once() {
        let _language = setup();
        let mut bench = Bench::new("karte");
        let texts = bench.settle().0;
        assert!(shows(&texts, &t("s_pg_leer")), "an empty list says so and what to do: {texts:?}");
        assert!(shows(&texts, &t("s_pg_risiko")), "the risk stands on the page: {texts:?}");
        bench.click(&t("s_pg_neu"));
        assert_eq!(bench.stored(), vec![Entry::default()], "the new entry is in the file without a save button");
        let texts = bench.settle().0;
        // The card is named; that no file is chosen yet stands on hover over its head and in the empty field below.
        assert!(shows(&texts, &t("s_pg_neuer")) && !shows(&texts, &t("s_pg_keine_datei")), "{texts:?}");
        assert!(!shows(&texts, &t("s_pg_leer")));
        // A text field is taken when Enter is pressed.
        bench.type_into(&t("s_pl_programmname"), "TeamSpeak");
        bench.type_into(&t("s_pg_keine"), "  --minimized  ");
        let stored = bench.stored();
        assert_eq!((stored[0].name.as_str(), stored[0].args.as_str()), ("TeamSpeak", "--minimized"));
        let texts = bench.settle().0;
        assert!(shows(&texts, "TeamSpeak"), "the card carries the name now: {texts:?}");
        // The moment: a choice with the line that explains it.
        bench.click(&t("s_pg_w_spiel"));
        assert_eq!(bench.stored()[0].when, When::Game);
        assert!(shows(&bench.settle().0, &t("s_pg_w_spiel_h")), "the line of the chosen moment shows");
        // The switches (no text of their own: found by their names, each card has its own).
        bench.click_named(egui::Role::CheckBox, &format!("TeamSpeak: {}", t("s_pg_beenden")));
        assert!(bench.stored()[0].end);
        bench.click_named(egui::Role::CheckBox, "TeamSpeak");
        assert!(!bench.stored()[0].on, "the switch of the card");
        // Browse: the answer of the dialog becomes the file.
        bench.click(&t("s_pg_suchen"));
        bench.settle();
        assert_eq!(bench.stored()[0].file, CHOSEN);
        assert!(shows(&bench.settle().0, CHOSEN), "and the card shows where it lives");
        // Remove.
        bench.click(&t("s_pg_entfernen"));
        assert!(bench.stored().is_empty());
        assert!(shows(&bench.settle().0, &t("s_pg_leer")));
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_removed_program_comes_back_where_it_stood_when_undone() {
        let _language = setup();
        let mut bench = Bench::new("rueckgaengig");
        let items = vec![
            Entry { name: "Erstes".into(), ..Entry::default() },
            Entry { name: "Zweites".into(), file: "/opt/z".into(), wait: 7, ..Entry::default() },
            Entry { name: "Drittes".into(), ..Entry::default() },
        ];
        assert!(start_programs::save(&bench.paths, &items));
        // The red button of the second card (the second of its kind from the top).
        let texts = bench.settle().0;
        let mut places: Vec<Pos2> =
            texts.iter().filter(|(text, _)| *text == t("s_pg_entfernen")).map(|(_, at)| *at).collect();
        places.sort_by(|a, b| a.y.total_cmp(&b.y));
        bench.press_at(places[1] + egui::vec2(3.0, 3.0));
        assert_eq!(bench.stored().len(), 2, "gone from the file at once");
        let texts = bench.settle().0;
        assert!(shows(&texts, &t("s_pg_entfernt")) && shows(&texts, &t("s_mlg_rueckgaengig")), "{texts:?}");
        bench.click(&t("s_mlg_rueckgaengig"));
        assert_eq!(bench.stored(), items, "back in its place with everything it had");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn what_was_typed_is_taken_over_when_the_field_is_left_without_enter_and_when_the_page_goes_away() {
        let _language = setup();
        let mut bench = Bench::new("tippen");
        assert!(start_programs::save(&bench.paths, &[Entry::default()]));
        bench.state.invalidate();
        // A click elsewhere leaves the field.
        bench.type_only(&t("s_pl_programmname"), "Mumble");
        bench.click(&t("s_pg_aktiv"));
        assert_eq!(bench.stored()[0].name, "Mumble", "taken over with the click elsewhere");
        // The page goes away with the text still in the field: the window flushes it.
        bench.type_only(&t("s_pg_keine"), "--go");
        assert_eq!(bench.stored()[0].args, "", "still being typed");
        bench.state.flush(&bench.paths);
        assert_eq!(bench.stored()[0].args, "--go", "flushed");
        // And a list that is read again keeps what stood in a field.
        bench.type_only(&t("s_pl_programmdatei"), "/opt/x");
        bench.state.invalidate();
        bench.settle();
        assert_eq!(bench.stored()[0].file, "/opt/x");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn every_input_of_a_card_has_a_name_for_screen_readers() {
        let _language = setup();
        let mut bench = Bench::new("namen");
        assert!(start_programs::save(&bench.paths, &[Entry { name: "TS".into(), ..Entry::default() }]));
        // The first frame after the tree for screen readers was switched on carries all of it.
        let scheme = theme::by_id("original").expect("original");
        let (state, paths, spawner) = (&mut bench.state, &bench.paths, &bench.spawner);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 1600.0))),
            ..Default::default()
        };
        let mut output = bench.ctx.run_ui(input, |ui| show_page(ui, scheme, state, paths, spawner));
        output.textures_delta.clear();
        let update = output.platform_output.accesskit_update.expect("accesskit is on");
        // A field is named by the label it points to (`labelled_by`), not by a label of its own.
        let node_of = |id: egui::accesskit::NodeId| update.nodes.iter().find(|(own, _)| *own == id).map(|(_, n)| n);
        // A text label is a node whose words sit in the text runs below it.
        let label_of = |id: egui::accesskit::NodeId| {
            let node = node_of(id)?;
            let words: Vec<String> = node
                .children()
                .iter()
                .filter_map(|child| node_of(*child))
                .filter_map(|run| run.value().map(str::to_owned))
                .collect();
            Some(node.label().map(str::to_owned).unwrap_or_else(|| words.join("")))
        };
        let named: Vec<String> = update
            .nodes
            .iter()
            .filter(|(_, node)| node.role() == egui::Role::TextInput)
            .flat_map(|(_, node)| node.labelled_by().iter().filter_map(|id| label_of(*id)).collect::<Vec<_>>())
            .collect();
        for label in [t("s_pg_name"), t("s_pg_datei"), t("s_pg_argumente"), t("s_pg_warten")] {
            let seen: Vec<String> = update
                .nodes
                .iter()
                .map(|(_, n)| format!("{:?}/{:?}/{}", n.role(), n.label(), n.labelled_by().len()))
                .collect();
            assert!(named.contains(&label), "no text field is named `{label}`: {named:?} of {seen:?}");
        }
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_dialog_that_is_cancelled_changes_nothing() {
        let _language = setup();
        let mut bench = Bench::new("abbruch");
        bench.state.choose = cancels;
        let kept = vec![Entry { name: "A".into(), file: "/opt/old/old.exe".into(), ..Entry::default() }];
        assert!(start_programs::save(&bench.paths, &kept));
        bench.click(&t("s_pg_suchen"));
        bench.settle();
        assert_eq!(bench.stored(), kept, "the file stays");
        assert!(shows(&bench.settle().0, "/opt/old/old.exe"));
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_main_switch_is_kept_and_leaves_the_list_alone() {
        let _language = setup();
        let mut bench = Bench::new("hauptschalter");
        assert!(start_programs::save(&bench.paths, &[Entry { name: "A".into(), ..Entry::default() }]));
        assert!(start_programs::enabled(&bench.paths));
        bench.click_named(egui::Role::CheckBox, &t("s_pg_aktiv"));
        assert!(!start_programs::enabled(&bench.paths), "off");
        assert_eq!(bench.stored().len(), 1, "the list stays");
        let said = bench.state.take_notice().expect("the foot says it");
        assert_eq!(said, Said::note(format!("{}: {}", t("s_pg_aktiv"), t("e_aus"))));
        bench.click_named(egui::Role::CheckBox, &t("s_pg_aktiv"));
        assert!(start_programs::enabled(&bench.paths), "on again");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn start_now_starts_through_the_spawner_or_says_why_not_in_words() {
        let _language = setup();
        let mut bench = Bench::new("jetzt");
        let file = bench.file_in_home("tool.sh");
        assert!(start_programs::save(
            &bench.paths,
            &[
                Entry { name: "Da".into(), file: file.clone(), args: "-a 'b c'".into(), ..Entry::default() },
                Entry { name: "Weg".into(), file: format!("{}/nicht-da", bench.home), ..Entry::default() },
                Entry { name: "Leer".into(), ..Entry::default() },
            ],
        ));
        bench.state.invalidate();
        let buttons = bench.settle().0;
        let mut places: Vec<Pos2> =
            buttons.iter().filter(|(text, _)| *text == t("s_pg_jetzt")).map(|(_, at)| *at).collect();
        assert_eq!(places.len(), 3, "one button per card: {buttons:?}");
        places.sort_by(|a, b| a.y.total_cmp(&b.y));
        bench.press_at(places[0] + egui::vec2(3.0, 3.0));
        let calls = bench.recorder.program_calls();
        assert_eq!(calls.len(), 1, "started through the stand-in");
        assert_eq!(calls[0].command, vec![file.clone(), "-a".into(), "b c".into()]);
        assert_eq!(bench.state.take_notice(), Some(Said::note(t("s_pg_gestartet"))));
        bench.press_at(places[1] + egui::vec2(3.0, 3.0));
        let said = bench.state.take_notice().expect("a reason");
        assert!(said.fault && said.text.contains("nicht-da") && !said.text.contains('—'), "{said:?}");
        bench.press_at(places[2] + egui::vec2(3.0, 3.0));
        let said = bench.state.take_notice().expect("a reason");
        assert!(said.fault && said.text.contains(&t("s_pg_grund_leer")) && !said.text.contains('—'), "{said:?}");
        assert_eq!(bench.recorder.program_calls().len(), 1, "nothing more started");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_list_is_read_again_when_the_settings_were() {
        let _language = setup();
        let mut bench = Bench::new("neu-lesen");
        assert!(shows(&bench.settle().0, &t("s_pg_leer")));
        assert!(start_programs::save(&bench.paths, &[Entry { name: "Von woanders".into(), ..Entry::default() }]));
        assert!(shows(&bench.settle().0, &t("s_pg_leer")), "not before it is told");
        bench.state.invalidate();
        assert!(shows(&bench.settle().0, "Von woanders"));
        let _ = std::fs::remove_dir_all(&bench.home);
    }
}
