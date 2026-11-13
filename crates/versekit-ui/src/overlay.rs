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

//! What the overlay draws: its bar with the Lucide buttons, the status,
//! location and update rows, the rows of finds and hints, the grip at the
//! free corner, and the narrow strip that stands for a waiting overlay.
//!
//! The whole window is the overlay. Only the background takes the opacity;
//! text, icons and the accent stay solid. Square schemes draw square with
//! corner brackets, the others rounded.
//!
//! [`show`] draws one frame and returns what was clicked, where the lock
//! sits (it stays clickable while clicks pass through) and the sizes the
//! window needs.

use egui::{
    Align, Color32, CursorIcon, Layout, Rect, Response, RichText, Role, ScrollArea, Sense, Stroke, StrokeKind,
    TextStyle, Ui, Vec2, WidgetInfo, pos2, vec2,
};
use versekit_core::armistice::Zone;
use versekit_core::contested::{LAMPS, Lamp, OverlayLine, Phase};
use versekit_core::contracts::OBJECTIVES_MAX;
use versekit_core::language::{Phrase, t, tf};
use versekit_core::live_contracts::Line as ContractLine;
use versekit_core::overlay::{BarSide, Message, Row, Rows, Shown};
use versekit_core::theme::{Color, IconTint, Scheme};

use crate::widgets::{corner_marks, rounded_corner_marks};
use crate::{box_radius, color};

/// Height of the bar in points.
pub const BAR_HEIGHT: f32 = 28.0;
/// Side of a button in the bar and of its icon.
pub const BAR_BUTTON: f32 = 24.0;
const BAR_ICON: f32 = 16.0;
/// Height of the waiting strip in points.
pub const STRIP_HEIGHT: f32 = 22.0;
const GRIP: f32 = 18.0;
const PAD: f32 = 8.0;

/// Everything one frame shows.
pub struct View<'a> {
    pub scheme: &'a Scheme,
    pub shown: Shown,
    pub rows: &'a Rows,
    /// The running contracts, as the log says: shown above the rows, nothing while there are none.
    pub contracts: &'a [ContractLine],
    /// The most contracts the game allows at a time, for the head of the bar (`3/10`).
    pub contracts_max: usize,
    /// The title of the contract the player just hid: a line with the way back, for a few seconds.
    pub hidden: Option<&'a str>,
    /// The refinery jobs, one line each, `true` for a done one: under the contracts, nothing while there are none.
    pub refinery: &'a [(String, bool)],
    /// The lines of the Kartellbank under the jobs, `true` for one that needs the member (the queue); a click on
    /// one opens the bank page of the main window.
    pub bank: &'a [(String, bool)],
    /// A way to start the game is known: the rocket stands in the bar. Without one there is no
    /// button; a button that does nothing is worse than none.
    pub launcher: bool,
    pub status: &'a Message,
    pub location: Option<&'a Phrase>,
    /// The shard of the session in short and the frame rates read from the picture of the game: at the right of the
    /// first line, while the game runs.
    pub shard: Option<&'a str>,
    /// The whole name of the shard, on hover over it.
    pub shard_hint: Option<&'a str>,
    /// The scanned signature with its material in one line, while the scanner reads one and the player asked
    /// for the line (the window above the signature in the game is the usual place).
    pub signature: Option<&'a str>,
    /// What the unit wants of the ores behind that signature ("Die Einheit sucht: …"), under its line.
    pub demand: Option<&'a str>,
    /// A newer version is ready (only shown while the game runs).
    pub update: Option<&'a str>,
    /// Whole days that version has waited, from one day on (`auto_update::days_waiting`): then the line shows with
    /// the game closed too, with the button that installs it now.
    pub update_days: Option<u64>,
    /// The player left or entered an armistice zone just now: a framed notice under the status, while the game runs.
    pub zone: Option<Zone>,
    pub game: bool,
    /// The lock is closed: clicks pass through except on it.
    pub locked: bool,
    pub bar: BarSide,
    /// Which window edges stay put, `(bottom, right)`; the grip sits opposite.
    pub anchored: (bool, bool),
    /// Background opacity, 0.3 … 1.0.
    pub opacity: f32,
    /// Buttons besides the fixed ones: in the bar while there is room,
    /// otherwise behind "⋯".
    pub extras: &'a [Extra],
    /// Extras that do not fit go behind "⋯"; off only for a counter check.
    pub menu: bool,
    /// The area that moves the window lies under the buttons too; on only
    /// for a counter check.
    pub drag_under_buttons: bool,
    /// The line of the Executive Hangar under the status; none while the player did not ask for it.
    pub hangar: Option<&'a OverlayLine>,
    /// The hints of the bar go to a window of their own (only into [`Output::hint`]) instead of a tooltip, which
    /// the overlay's window would cut off where it ends.
    pub hints_outside: bool,
}

/// The id under which a frame keeps [`View::hints_outside`] for the buttons of the bar.
fn hints_outside_id() -> egui::Id {
    egui::Id::unique("overlay-hinweise-aussen")
}

/// The id under which a frame keeps where the symbol of the hint shown stands ([`Output::hint_rect`]).
fn hint_rect_id() -> egui::Id {
    egui::Id::unique("overlay-hinweis-stelle")
}

/// Shows `meaning` on hover: as a tooltip, or, where the view gives its hints to a window of its own, only in `hint`
/// once the pointer rested on it as long as a tooltip waits.
fn hover_hint(ui: &Ui, response: Response, meaning: &str, hint: &mut Option<String>) -> Response {
    // Where the symbol stands: the window of the hint keeps clear of it.
    let remember = |ui: &Ui| {
        let seen = response.rect.intersect(ui.clip_rect());
        let seen = if seen.is_positive() { seen } else { response.rect };
        ui.ctx().data_mut(|d| d.insert_temp(hint_rect_id(), seen));
    };
    if !ui.ctx().data(|d| d.get_temp::<bool>(hints_outside_id())).unwrap_or(false) {
        if response.is_tooltip_open() {
            *hint = Some(meaning.to_string());
            remember(ui);
        }
        return response.on_hover_text(meaning);
    }
    if response.hovered() {
        let delay = ui.style().interaction.tooltip_delay;
        let still = ui.input(|i| i.pointer.time_since_last_movement());
        if still >= delay {
            *hint = Some(meaning.to_string());
            remember(ui);
        } else {
            ui.ctx().request_repaint_after_secs(delay - still);
        }
    }
    response
}

/// A button besides the fixed ones of the bar.
#[derive(Clone, Debug)]
pub struct Extra {
    pub action: Action,
    /// The Lucide icon.
    pub icon: &'static str,
    /// What it does: the hint on hover and the name for screen readers.
    pub meaning: String,
    pub id: &'static str,
    /// The colour its icon stands in.
    pub tone: Tone,
}

/// The colour of the icon of an extra button, from the colours of the scheme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tone {
    /// Like the other buttons of the bar.
    #[default]
    Normal,
    /// "On": the accent (a switch of the bar, like the lock).
    Lit,
    /// Something happened just now: yellow.
    Flash,
    /// Something is wrong: red.
    Fault,
}

impl Tone {
    /// The colour of anti-AFK in its state: off like the others, waiting in the accent, a press yellow for a
    /// moment, a stop or blind red.
    pub fn of_anti_afk(look: versekit_core::anti_afk::Look) -> Tone {
        use versekit_core::anti_afk::Look;
        match look {
            Look::Off => Tone::Normal,
            Look::Waiting => Tone::Lit,
            Look::Pressed => Tone::Flash,
            Look::Stopped | Look::Blind => Tone::Fault,
        }
    }

    /// The colour in scheme `s`; `normal` is the tint of the other buttons.
    pub fn tint(self, s: &Scheme, normal: Color) -> Color {
        match self {
            Tone::Normal => normal,
            Tone::Lit => s.accent,
            Tone::Flash => s.yellow,
            Tone::Fault => s.red,
        }
    }
}

/// What was clicked in the bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Quit,
    Clear,
    Fold,
    Lock,
    Rescan,
    /// Brings the main window to the front on the overview.
    Overview,
    /// Starts the game by the way the player has (the rocket); only there while a way is known.
    Launch,
    /// One of the extra buttons, by its id.
    Extra(&'static str),
}

/// A drag that began this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drag {
    /// On the bar: moves the window.
    Move,
    /// On the grip: resizes it.
    Resize,
}

/// What one frame learned.
#[derive(Clone, Debug, Default)]
pub struct Output {
    pub actions: Vec<Action>,
    pub drag: Option<Drag>,
    /// The lock button in window points; `None` while not drawn.
    pub lock: Option<Rect>,
    /// The grip for resizing in window points; `None` while the overlay is not open.
    pub grip: Option<Rect>,
    /// The buttons drawn in the bar and where, in window points.
    pub buttons: Vec<(Action, Rect)>,
    /// The extras behind the closed "⋯".
    pub in_menu: Vec<Action>,
    /// The "⋯" button; `None` while every extra fits.
    pub more: Option<Rect>,
    /// Where the title (or the opened menu) ends on the left.
    pub title_end: f32,
    /// The hint of the bar shown in the last frame: a button's own, or the
    /// one of the free bar.
    pub hint: Option<String>,
    /// Where the symbol of that hint stands, in window points (the visible part of it).
    pub hint_rect: Option<Rect>,
    /// The narrowest window that still shows every button of the bar.
    pub min_width: f32,
    /// The size of the waiting strip.
    pub strip: Vec2,
    /// The row texts drawn this frame (names, hints, the placeholder): what
    /// a check of the content looks for.
    pub drawn: Vec<String>,
    /// Rows of finds and hints wholly in view.
    pub visible_rows: usize,
    /// The contract the player took out of the bar by hand.
    pub hide: Option<String>,
    /// The hide buttons of the contract lines in window points, in the order of the lines.
    pub hide_buttons: Vec<Rect>,
    /// The player took the hiding back (the button of the line that says a contract was hidden).
    pub unhide: bool,
    /// That button in window points; `None` while the line is not drawn.
    pub unhide_button: Option<Rect>,
    /// The lines of the contract bar drawn this frame (header, lines, objectives).
    pub contract_texts: Vec<String>,
    /// A bank line was clicked: the bank page of the main window opens.
    pub open_bank: bool,
    /// The player asked to install the waiting update now (the button of its line, only while the game is closed).
    pub update_now: bool,
    /// That button in window points, and whether it can be used; `None` while the line is not drawn.
    pub update_button: Option<(Rect, bool)>,
}

fn with_alpha(c: Color, opacity: f32) -> Color32 {
    let base = color(c);
    Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), (opacity.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// The icon at the free corner points where the window can grow.
pub fn grip_icon(anchored: (bool, bool)) -> &'static str {
    match anchored {
        (false, false) => "arrow-down-right",
        (false, true) => "arrow-down-left",
        (true, false) => "arrow-up-right",
        (true, true) => "arrow-up-left",
    }
}

/// Where the grip for resizing sits in the open overlay `rect`: in the corner away from the anchored edges (the
/// window grows to it). Where that corner is on the side of the bar, the grip sits in the same corner right under
/// (or over) the bar, never in it: there it would lie over the buttons of the bar (over the cross).
fn grip_rect(rect: Rect, bar: BarSide, (bottom, right): (bool, bool)) -> Rect {
    let x = if right { rect.min.x + GRIP / 2.0 + 2.0 } else { rect.max.x - GRIP / 2.0 - 2.0 };
    let top_corner = bottom;
    let y = match (top_corner, bar) {
        (true, BarSide::Top) => rect.min.y + BAR_HEIGHT + GRIP / 2.0 + 2.0,
        (true, BarSide::Bottom) => rect.min.y + GRIP / 2.0 + 2.0,
        (false, BarSide::Bottom) => rect.max.y - BAR_HEIGHT - GRIP / 2.0 - 2.0,
        (false, BarSide::Top) => rect.max.y - GRIP / 2.0 - 2.0,
    };
    Rect::from_center_size(pos2(x, y), Vec2::splat(GRIP))
}

/// A button of the bar: only the icon, accent on hover, with its meaning
/// on hover and for screen readers. `hint` takes the meaning while its hint
/// is shown.
#[allow(clippy::too_many_arguments)]
fn bar_button(
    ui: &mut Ui,
    rect: Rect,
    icon: &str,
    tint: Color,
    hover: Color,
    meaning: &str,
    id: impl std::hash::Hash + std::fmt::Debug,
    hint: &mut Option<String>,
) -> Response {
    bar_button_named(ui, rect, icon, tint, hover, meaning, meaning, id, hint)
}

/// [`bar_button`] with a name for screen readers that differs from the hint
/// (the hide buttons of several contracts must not all be called the same).
#[allow(clippy::too_many_arguments)]
fn bar_button_named(
    ui: &mut Ui,
    rect: Rect,
    icon: &str,
    tint: Color,
    hover: Color,
    meaning: &str,
    name: &str,
    id: impl std::hash::Hash + std::fmt::Debug,
    hint: &mut Option<String>,
) -> Response {
    let response = ui.interact(rect, ui.make_persistent_id(("overlay-knopf", id)), Sense::click());
    let tone = if response.hovered() { hover } else { tint };
    // Under the pointer the button gets a field too: a changed color of the icon alone is hard to see.
    if response.hovered() {
        let fill = ui.visuals().widgets.hovered.bg_fill;
        ui.painter().rect_filled(rect.shrink(1.0), 4.0, fill);
    }
    if let Some(image) = crate::icons::image(icon, tone, BAR_ICON) {
        image.paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(BAR_ICON)));
    }
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, name));
    hover_hint(ui, response.on_hover_cursor(CursorIcon::PointingHand), meaning, hint)
}

/// The buttons of the bar from the right edge: (action, icon, tint, meaning, id).
fn bar_buttons(view: &View) -> Vec<(Action, &'static str, Color, String, &'static str)> {
    let s = view.scheme;
    let open = view.shown == Shown::Open;
    let normal = s.icon(IconTint::Normal);
    let mut buttons = vec![
        (Action::Quit, "x", normal, t("hinweis_schliessen"), "quit"),
        (Action::Clear, "eraser", normal, t("ov_h_leeren"), "clear"),
        (
            Action::Fold,
            if open { "chevron-up" } else { "chevron-down" },
            normal,
            t(if open { "ov_h_einklappen" } else { "hinweis_ausklappen" }),
            "fold",
        ),
        (
            Action::Lock,
            if view.locked { "lock" } else { "lock-open" },
            if view.locked { s.accent } else { normal },
            t(if view.locked { "hinweis_schloss" } else { "hinweis_schloss_zu" }),
            "lock",
        ),
        (Action::Rescan, "rotate-cw", normal, t("ov_h_neulesen"), "rescan"),
        (Action::Overview, "settings", normal, t("hinweis_einstellungen"), "overview"),
    ];
    // A rocket, no play arrow: a triangle stands for playing a video everywhere, a rocket says "start" and space.
    if view.launcher {
        buttons.push((Action::Launch, "rocket", s.accent, t("s_sp_start"), "launch"));
    }
    buttons
}

/// How many extra buttons stand directly in a bar `width` points wide, when
/// `title` points on the left are taken and `fixed` buttons always stand
/// there. The rest goes behind "⋯", which then takes a place of its own.
pub fn extras_direct(width: f32, title: f32, fixed: usize, extras: usize) -> usize {
    // The small addition keeps a width summed from fractions from losing a place.
    let places = ((width - title - PAD) / BAR_BUTTON + 0.001).floor().max(0.0) as usize;
    let free = places.saturating_sub(fixed);
    if extras <= free { extras } else { free.saturating_sub(1) }
}

/// The bar at `rect`: title on the left (a drag moves the window), buttons
/// on the right: the fixed ones, the extras that fit and "⋯" for the rest.
/// While "⋯" is open, the rest stands where the title was. Returns the
/// narrowest width that shows all of it.
fn bar(ui: &mut Ui, rect: Rect, view: &View, out: &mut Output) -> f32 {
    let s = view.scheme;
    let painter = ui.painter().clone();
    let title_font = crate::fonts::bold(TextStyle::Body.resolve(ui.style()).size);
    let title = painter.layout_no_wrap(t("hf_titel"), title_font, color(s.accent));
    let fixed = bar_buttons(view);
    let title_width = PAD + title.size().x + 12.0;
    let direct = match view.menu {
        true => extras_direct(rect.width(), title_width, fixed.len(), view.extras.len()),
        false => view.extras.len(),
    };
    let (shown, hidden) = view.extras.split_at(direct);
    let more_id = ui.make_persistent_id("overlay-mehr");
    if hidden.is_empty() {
        // Nothing behind "⋯": it opens closed when it comes back.
        ui.data_mut(|d| d.remove::<bool>(more_id));
    }
    let open = !hidden.is_empty() && ui.data(|d| d.get_temp::<bool>(more_id)).unwrap_or(false);
    let places = fixed.len() + shown.len() + usize::from(!hidden.is_empty());
    let with_more = fixed.len() + usize::from(!view.extras.is_empty());
    let mut min_width = title_width + with_more as f32 * BAR_BUTTON + PAD;
    // At the narrowest every extra is behind "⋯": the menu opened there takes the place of the title and must fit
    // in the same width, so opening it never widens the window.
    if view.menu && !view.extras.is_empty() {
        let all_hidden = view.extras.len() as f32 * BAR_BUTTON;
        min_width = min_width.max(PAD + all_hidden + 12.0 + with_more as f32 * BAR_BUTTON + PAD);
    }
    if open {
        // The hidden buttons take the place of the title.
        min_width = min_width.max(PAD + hidden.len() as f32 * BAR_BUTTON + 12.0 + places as f32 * BAR_BUTTON + PAD);
    }

    // The free bar between the buttons moves the window. It must not lie
    // under them: it would share their hover, and only the first hint of a
    // layer is shown.
    let buttons_left = rect.max.x - PAD / 2.0 - places as f32 * BAR_BUTTON;
    let free_left = if open { rect.min.x + PAD / 2.0 + hidden.len() as f32 * BAR_BUTTON } else { rect.min.x };
    let free = match view.drag_under_buttons {
        true => rect,
        false => Rect::from_min_max(pos2(free_left, rect.min.y), pos2(buttons_left.max(free_left), rect.max.y)),
    };
    let drag = ui.interact(free, ui.make_persistent_id("overlay-leiste"), Sense::click_and_drag());
    if drag.drag_started() {
        out.drag = Some(Drag::Move);
    }
    hover_hint(ui, drag.on_hover_cursor(CursorIcon::Grab), &t("hinweis_ziehen"), &mut out.hint);

    let y = rect.center().y;
    let hover = s.icon(IconTint::Hover);
    let normal = s.icon(IconTint::Normal);
    let mut close_menu = false;
    if open {
        let mut x = rect.min.x + PAD / 2.0;
        for extra in hidden {
            let r = Rect::from_min_size(pos2(x, y - BAR_BUTTON / 2.0), Vec2::splat(BAR_BUTTON));
            x += BAR_BUTTON;
            out.buttons.push((extra.action, r));
            // Extras have ids of their own: an extra called like a fixed button is another one.
            let tint = extra.tone.tint(s, normal);
            if bar_button(ui, r, extra.icon, tint, hover, &extra.meaning, ("extra", extra.id), &mut out.hint).clicked()
            {
                out.actions.push(extra.action);
                close_menu = true;
            }
        }
        out.title_end = x;
    } else {
        painter.galley(pos2(rect.min.x + PAD, y - title.size().y / 2.0), title.clone(), color(s.accent));
        out.title_end = rect.min.x + PAD + title.size().x;
        out.in_menu = hidden.iter().map(|e| e.action).collect();
    }

    let mut x = rect.max.x - PAD / 2.0;
    let place = |x: &mut f32| {
        let r = Rect::from_center_size(pos2(*x - BAR_BUTTON / 2.0, y), Vec2::splat(BAR_BUTTON));
        *x -= BAR_BUTTON;
        r
    };
    for (action, icon, tint, meaning, id) in fixed {
        let r = place(&mut x);
        if action == Action::Lock {
            out.lock = Some(r);
        }
        out.buttons.push((action, r));
        if bar_button(ui, r, icon, tint, hover, &meaning, id, &mut out.hint).clicked() {
            out.actions.push(action);
        }
    }
    for extra in shown {
        let r = place(&mut x);
        out.buttons.push((extra.action, r));
        let tint = extra.tone.tint(s, normal);
        if bar_button(ui, r, extra.icon, tint, hover, &extra.meaning, ("extra", extra.id), &mut out.hint).clicked() {
            out.actions.push(extra.action);
        }
    }
    if !hidden.is_empty() {
        let r = place(&mut x);
        out.more = Some(r);
        let (tint, meaning) = if open { (s.accent, t("ov_h_mehr_zu")) } else { (normal, t("ov_h_mehr")) };
        if bar_button(ui, r, "ellipsis", tint, hover, &meaning, "mehr", &mut out.hint).clicked() {
            close_menu = open;
            ui.data_mut(|d| d.insert_temp(more_id, !open));
        }
    }
    if close_menu {
        ui.data_mut(|d| d.insert_temp(more_id, false));
    }
    min_width
}

/// The waiting strip: an eye alone, what it does in its hint.
fn strip(ui: &mut Ui, view: &View, out: &mut Output) {
    let s = view.scheme;
    let rect = ui.max_rect();
    out.strip = vec2(PAD + BAR_ICON + PAD, STRIP_HEIGHT);
    let radius = if s.square { 0.0 } else { STRIP_HEIGHT / 2.0 };
    ui.painter().rect(
        rect,
        radius,
        with_alpha(s.bar, view.opacity.max(0.85)),
        Stroke::new(1.0, color(s.accent)),
        StrokeKind::Inside,
    );
    let y = rect.center().y;
    if let Some(eye) = crate::icons::image("eye", s.accent, BAR_ICON) {
        eye.paint_at(ui, Rect::from_center_size(pos2(rect.min.x + PAD + BAR_ICON / 2.0, y), Vec2::splat(BAR_ICON)));
    }
    let meaning = t("ov_streifen_h");
    let response = ui.interact(rect, ui.make_persistent_id("overlay-streifen"), Sense::hover());
    response.widget_info(|| WidgetInfo::labeled(Role::Image, true, &meaning));
    let _ = hover_hint(ui, response, &meaning, &mut out.hint);
}

/// Diameter of a dot of the hangar line and the gap between two.
const DOT: f32 = 9.0;
const DOT_GAP: f32 = 3.0;

/// The line of the Executive Hangar: its short name, five dots and what it does next in the colour of the
/// phase. The text is cut off with "…" where the line ends: the line never widens the window.
fn hangar_line(ui: &mut Ui, s: &Scheme, line: &OverlayLine, out: &mut Output) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(RichText::new(&line.label).small().strong().color(color(s.sub)));
        let width = LAMPS as f32 * DOT + (LAMPS - 1) as f32 * DOT_GAP;
        let (rect, response) = ui.allocate_exact_size(vec2(width, DOT), Sense::hover());
        let green = line.lamps.iter().filter(|lamp| **lamp == Lamp::Green).count();
        let said = tf("s_cz_lampen", &[("0", green.into()), ("1", LAMPS.into())]);
        response.widget_info(|| WidgetInfo::labeled(Role::Image, true, &said));
        let meaning = tf("ov_h_hangar", &[("0", said.as_str().into())]);
        hover_hint(ui, response, &meaning, &mut out.hint);
        for (i, lamp) in line.lamps.iter().enumerate() {
            let center = pos2(rect.min.x + DOT / 2.0 + i as f32 * (DOT + DOT_GAP), rect.center().y);
            let radius = DOT / 2.0 - 0.5;
            match lamp {
                Lamp::Red => ui.painter().circle_filled(center, radius, color(s.red)),
                Lamp::Green => ui.painter().circle_filled(center, radius, color(s.accent)),
                Lamp::Dark => ui.painter().circle_stroke(center, radius, Stroke::new(1.0, color(s.sub))),
            };
        }
        let tint = match line.phase {
            Some(Phase::Closed) => s.red,
            Some(Phase::Open) => s.accent,
            Some(Phase::Resetting) | None => s.sub,
        };
        ui.add(egui::Label::new(RichText::new(&line.text).small().color(color(tint))).truncate());
    });
    out.drawn.push(format!("{} {}", line.label, line.text));
}

/// The line of an update that has waited `days` whole days, in yellow, with the button that installs it now at its
/// right end. While the game runs the button is grey and its hint says why: the update never comes in the game.
fn update_waiting_line(ui: &mut Ui, s: &Scheme, version: &str, days: u64, game: bool, out: &mut Output) {
    let days = versekit_core::auto_update::days_text(days);
    let text = tf("ov_update_wartet", &[("0", version.into()), ("1", days.into())]);
    let rect = line_with_button(ui, |ui| {
        ui.add(egui::Label::new(RichText::new(&text).small().color(color(s.yellow))).wrap());
    });
    let usable = !game;
    let name = t("s_ul_jetzt");
    let meaning = if usable { name.clone() } else { t("ov_update_nach_spiel") };
    let response = ui.interact(rect, ui.make_persistent_id("overlay-update-jetzt"), Sense::click());
    let tone = if !usable {
        s.sub
    } else if response.hovered() {
        s.fg
    } else {
        s.accent
    };
    if usable && response.hovered() {
        let fill = ui.visuals().widgets.hovered.bg_fill;
        ui.painter().rect_filled(rect.shrink(1.0), 4.0, fill);
    }
    if let Some(image) = crate::icons::image("download", tone, BAR_ICON) {
        image.paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(BAR_ICON)));
    }
    response.widget_info(|| WidgetInfo::labeled(Role::Button, usable, &name));
    let response = if usable { response.on_hover_cursor(CursorIcon::PointingHand) } else { response };
    if hover_hint(ui, response, &meaning, &mut out.hint).clicked() && usable {
        out.update_now = true;
    }
    out.update_button = Some((rect, usable));
    out.drawn.push(text);
}

/// The notice of an armistice zone in a frame over the full width: leaving it (weapons are free again) in the color of
/// warnings and in bold, entering it quietly in the house color.
fn zone_notice(ui: &mut Ui, s: &Scheme, zone: Zone, out: &mut Output) {
    let (key, tint) = match zone {
        Zone::Left => ("ov_zone_verlassen", color(s.yellow)),
        Zone::Entered => ("ov_zone_betreten", color(s.accent)),
    };
    let text = t(key);
    egui::Frame::new()
        .fill(tint.gamma_multiply(0.18))
        .stroke(Stroke::new(1.0, tint))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let label = RichText::new(&text).color(tint);
            ui.add(egui::Label::new(if zone == Zone::Left { label.strong() } else { label }).wrap());
        });
    ui.add_space(2.0);
    out.drawn.push(text);
}

/// The key of the hint of the icon in front of a row: what the green tick, the blue circle, the gold star and the
/// grey "i" mean.
fn row_hint_key(entry: &Row) -> &'static str {
    match entry {
        Row::Found { .. } => "ov_h_zeile_gefunden",
        Row::Hint { .. } => "ov_h_zeile_hinweis",
        Row::Catalog { watch_title: Some(_), .. } => "ov_h_zeile_merkliste",
        Row::Catalog { watch_title: None, .. } => "ov_h_zeile_katalog",
    }
}

/// One row: its icon and its texts, wrapping instead of cutting off.
fn row(ui: &mut Ui, scheme: &Scheme, entry: &Row, out: &mut Output) {
    let meaning = t(row_hint_key(entry));
    let (icon, tint, name, line, line_color) = match entry {
        Row::Found { name, kind, code, time, caught_up } => {
            ("check", scheme.accent, Some(name.clone()), Row::found_line(kind, code, time, *caught_up), scheme.sub)
        }
        Row::Hint { text, .. } => ("info", scheme.sub, None, text.render(), scheme.sub),
        Row::Catalog { name, kind, time, watch_title } => {
            let (icon, tint, first) = match watch_title {
                Some(title) => ("star", scheme.yellow, tf("ov_jetzt_craftbar", &[("0", title.as_str().into())])),
                None => ("circle", scheme.blue, t("neu_craftbar")),
            };
            let parts: Vec<&str> =
                [first.as_str(), kind.as_str(), time.as_str()].into_iter().filter(|p| !p.is_empty()).collect();
            (icon, tint, Some(name.clone()), parts.join(" · "), tint)
        }
    };
    let shown = ui.horizontal_top(|ui| {
        let (icon_rect, icon_response) = ui.allocate_exact_size(vec2(BAR_ICON, BAR_ICON + 2.0), Sense::hover());
        if let Some(image) = crate::icons::image(icon, tint, BAR_ICON - 2.0) {
            image.paint_at(ui, Rect::from_center_size(icon_rect.center(), Vec2::splat(BAR_ICON - 2.0)));
        }
        hover_hint(ui, icon_response, &meaning, &mut out.hint);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            if let Some(name) = &name {
                let name_color = match entry {
                    Row::Catalog { watch_title: Some(_), .. } => scheme.yellow,
                    _ => scheme.fg,
                };
                ui.add(egui::Label::new(RichText::new(name).color(color(name_color))).wrap());
                out.drawn.push(name.clone());
            } else {
                out.drawn.push(line.clone());
            }
            ui.add(egui::Label::new(RichText::new(&line).small().color(color(line_color))).wrap());
        });
    });
    if ui.clip_rect().contains_rect(shown.response.rect) {
        out.visible_rows += 1;
    }
}

/// A line of text with one button at the right edge: the text column has a fixed width, so the
/// buttons of several lines stand in one column. Gives the rect of the button; `text` draws the text.
fn line_with_button(ui: &mut Ui, text: impl FnOnce(&mut Ui)) -> Rect {
    let width = ui.available_width();
    let column = (width - BAR_BUTTON - 4.0).max(40.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.allocate_ui_with_layout(vec2(column, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(column);
            text(ui);
        });
        ui.allocate_exact_size(vec2(BAR_BUTTON, BAR_BUTTON), Sense::hover()).0
    })
    .inner
}

/// The bar of the running contracts: its header, a line for each with the
/// button that takes it out by hand, under it the open objectives indented
/// (at most [`OBJECTIVES_MAX`], the rest counted) or, for a contract that
/// fell silent after a change of server, the grey line that says why.
/// Above them, for a few seconds after a contract was hidden, the line with the way back.
///
/// ⚠ A state, not a history: it shows what runs now and takes no room when
/// nothing does.
fn contracts_bar(ui: &mut Ui, view: &View, out: &mut Output) {
    if view.contracts.is_empty() && view.hidden.is_none() {
        return;
    }
    let s = view.scheme;
    let quiet = |ui: &mut Ui, text: &str| {
        ui.add(egui::Label::new(RichText::new(text).small().color(color(s.sub))).wrap());
    };
    if let Some(title) = view.hidden {
        let text = tf("ov_a_ausgeblendet", &[("0", title.into())]);
        let rect = line_with_button(ui, |ui| quiet(ui, &text));
        out.contract_texts.push(text);
        out.unhide_button = Some(rect);
        let meaning = t("s_p9_rueckgaengig");
        let name = tf("ov_a_zurueck_name", &[("0", title.into())]);
        if bar_button_named(ui, rect, "undo-2", s.accent, s.accent, &meaning, &name, "auftrag-zurueck", &mut out.hint)
            .clicked()
        {
            out.unhide = true;
        }
    }
    if view.contracts.is_empty() {
        ui.add_space(2.0);
        return;
    }
    let head = contracts_head(view.contracts.len(), view.contracts_max);
    quiet(ui, &head);
    out.contract_texts.push(head);
    for line in view.contracts {
        let text = line.text.render();
        let tone = if line.grey { s.sub } else { s.fg };
        // The text column has a fixed width, the button stands at the right edge in every line.
        let rect = line_with_button(ui, |ui| {
            ui.add(egui::Label::new(RichText::new(&text).small().color(color(tone))).wrap());
        });
        out.hide_buttons.push(rect);
        let meaning = t("ov_auftrag_weg");
        let name = tf("ov_a_weg_name", &[("0", line.title.as_str().into())]);
        let id = ("auftrag-weg", line.title.as_str());
        // The cross is what hides a tile on the pages of the main window too.
        if bar_button_named(ui, rect, "x", s.sub, s.red, &meaning, &name, id, &mut out.hint).clicked() {
            out.hide = Some(line.title.clone());
        }
        out.contract_texts.push(text);
        if line.grey {
            let why = t("ov_a_unklar");
            ui.horizontal_top(|ui| {
                ui.add_space(14.0);
                quiet(ui, &why);
            });
            out.contract_texts.push(why);
            continue;
        }
        for goal in line.goals.iter().take(OBJECTIVES_MAX) {
            ui.horizontal_top(|ui| {
                ui.add_space(14.0);
                // An empty circle: still open, as on the blueprint list.
                if let Some(image) = crate::icons::image("circle", s.sub, 10.0) {
                    let circle = ui.add(image.sense(Sense::hover()));
                    hover_hint(ui, circle, &t("ov_h_ziel_offen"), &mut out.hint);
                    ui.add_space(2.0);
                }
                quiet(ui, goal);
            });
            out.contract_texts.push(goal.clone());
        }
        if let Some(rest) = line.goals.len().checked_sub(OBJECTIVES_MAX).filter(|r| *r > 0) {
            let more = tf("ov_ziele_mehr", &[("0", rest.into())]);
            ui.horizontal_top(|ui| {
                ui.add_space(14.0);
                quiet(ui, &more);
            });
            out.contract_texts.push(more);
        }
    }
    ui.add_space(2.0);
}

/// The head of the bar of contracts with how many run of the most the game allows (`3/10`).
pub fn contracts_head(running: usize, most: usize) -> String {
    tf("ov_auftraege_kopf_zahl", &[("0", running.into()), ("1", most.into())])
}

/// How many rows the overlay shows on its first start.
pub const DEFAULT_ROWS: usize = 6;

/// The size of the overlay on its first start, in points: the bar, the
/// status and a location line and [`DEFAULT_ROWS`] rows of finds in the
/// text size in use; the width of 4.12.
pub fn default_size(ui: &Ui) -> Vec2 {
    let body = ui.text_style_height(&TextStyle::Body);
    let small = ui.text_style_height(&TextStyle::Small);
    let spacing = ui.spacing().item_spacing.y;
    // A row: name and quiet line, 4 points apart from the next.
    let row = body + small + 4.0;
    // Inner margins of the body, status and location lines, the gap above the rows.
    let lines = 8.0 + 2.0 * (small + spacing) + 4.0;
    vec2(versekit_core::overlay::DEFAULT_WIDTH as f32, BAR_HEIGHT + lines + DEFAULT_ROWS as f32 * row + 4.0)
}

/// The first line with the shard at its right: the shard stays whole, the status is cut with "…" where the room ends.
/// Where the grip sits at the top, its side is kept clear.
fn status_with_shard(
    ui: &mut Ui,
    s: &Scheme,
    status: RichText,
    shard: &str,
    hint: Option<&str>,
    (bottom, right): (bool, bool),
    out: &mut Output,
) {
    let height = ui.text_style_height(&TextStyle::Small);
    ui.allocate_ui_with_layout(vec2(ui.available_width(), height), Layout::right_to_left(Align::Center), |ui| {
        if bottom && !right {
            ui.add_space(GRIP);
        }
        let shown = ui.add(egui::Label::new(RichText::new(shard).small().color(color(s.sub))).extend());
        if let Some(hint) = hint {
            hover_hint(ui, shown, hint, &mut out.hint);
        }
        let rest = (ui.available_width() - if bottom && right { GRIP } else { 0.0 }).max(0.0);
        ui.allocate_ui_with_layout(vec2(rest, height), Layout::left_to_right(Align::Center), |ui| {
            ui.add(egui::Label::new(status).truncate());
        });
    });
}

/// The corners of the bar: rounded like the box where they lie on its edge, square towards the body.
fn bar_corners(radius: egui::CornerRadius, shown: Shown, bar: BarSide) -> egui::CornerRadius {
    let (r, zero) = (radius.nw, 0);
    match (shown, bar) {
        (Shown::Bar | Shown::Strip, _) => radius,
        (Shown::Open, BarSide::Top) => egui::CornerRadius { nw: r, ne: r, sw: zero, se: zero },
        (Shown::Open, BarSide::Bottom) => egui::CornerRadius { nw: zero, ne: zero, sw: r, se: r },
    }
}

/// Draws the overlay over the whole of `ui` and says what happened.
pub fn show(ui: &mut Ui, view: &View) -> Output {
    ui.ctx().data_mut(|d| d.remove::<Rect>(hint_rect_id()));
    let mut out = draw(ui, view);
    if out.hint.is_some() {
        out.hint_rect = ui.ctx().data(|d| d.get_temp::<Rect>(hint_rect_id()));
    }
    out
}

/// [`show`] without the place of the hint.
fn draw(ui: &mut Ui, view: &View) -> Output {
    let mut out = Output::default();
    ui.ctx().data_mut(|d| d.insert_temp(hints_outside_id(), view.hints_outside));
    let s = view.scheme;
    let rect = ui.max_rect();
    ui.style_mut().interaction.selectable_labels = false;
    if view.shown == Shown::Strip {
        strip(ui, view, &mut out);
        return out;
    }

    let radius = box_radius(s);
    ui.painter().rect(
        rect,
        radius,
        with_alpha(s.bg, view.opacity),
        Stroke::new(1.0, color(s.line)),
        StrokeKind::Inside,
    );
    let bar_rect = match (view.shown, view.bar) {
        (Shown::Bar, _) | (_, BarSide::Top) => Rect::from_min_size(rect.min, vec2(rect.width(), BAR_HEIGHT)),
        (_, BarSide::Bottom) => Rect::from_min_max(pos2(rect.min.x, rect.max.y - BAR_HEIGHT), rect.max),
    };
    ui.painter().rect_filled(
        bar_rect.shrink(1.0),
        bar_corners(radius, view.shown, view.bar),
        with_alpha(s.bar, view.opacity),
    );
    out.min_width = bar(ui, bar_rect, view, &mut out);

    if view.shown == Shown::Open {
        let body = if view.bar == BarSide::Top {
            Rect::from_min_max(pos2(rect.min.x, bar_rect.max.y), rect.max)
        } else {
            Rect::from_min_max(rect.min, pos2(rect.max.x, bar_rect.min.y))
        }
        .shrink2(vec2(PAD, 4.0));
        let (bottom, right) = view.anchored;
        // The grip sits at the free corner; text on its row keeps clear of it.
        let grip_rect = grip_rect(rect, view.bar, view.anchored);
        out.grip = Some(grip_rect);
        let mut content = ui.new_child(egui::UiBuilder::new().max_rect(body).layout(Layout::top_down(Align::Min)));
        let keep_clear = |ui: &mut Ui, text: RichText| {
            ui.horizontal(|ui| {
                if bottom && right {
                    ui.add_space(GRIP);
                } else if bottom {
                    // The grip stands at the right end of this line: the text wraps before it.
                    ui.set_max_width((ui.available_width() - GRIP).max(1.0));
                }
                ui.add(egui::Label::new(text).wrap());
            });
        };
        let status = RichText::new(view.status.render()).small().color(color(s.sub));
        match view.shard.filter(|_| view.game) {
            Some(shard) => status_with_shard(&mut content, s, status, shard, view.shard_hint, view.anchored, &mut out),
            None => keep_clear(&mut content, status),
        }
        if let Some(zone) = view.zone.filter(|_| view.game) {
            zone_notice(&mut content, s, zone, &mut out);
        }
        if let Some(place) = view.location {
            content.add(egui::Label::new(RichText::new(place.to_string()).small().color(color(s.sub))).wrap());
        }
        if let Some(line) = view.signature {
            content.add(egui::Label::new(RichText::new(line).small().color(color(s.accent))).wrap());
            if let Some(wanted) = view.demand {
                content.add(egui::Label::new(RichText::new(wanted).small().color(color(s.fg))).wrap());
            }
        }
        match (view.update, view.update_days) {
            (Some(version), Some(days)) => update_waiting_line(&mut content, s, version, days, view.game, &mut out),
            (Some(version), None) if view.game => {
                let text = tf("ov_update", &[("0", version.into())]);
                content.add(egui::Label::new(RichText::new(text).small().color(color(s.accent))).wrap());
            }
            _ => {}
        }
        if let Some(line) = view.hangar {
            hangar_line(&mut content, s, line, &mut out);
        }
        content.add_space(4.0);
        if !view.contracts.is_empty() || view.hidden.is_some() {
            // The bar takes at most about half of what is left; more rolls in its own area,
            // so the rows below always keep their place.
            let cap = (content.available_height() * 0.55).max(48.0);
            let area = ScrollArea::vertical()
                .id_salt("overlay-auftraege")
                .max_height(cap)
                .min_scrolled_height(cap)
                .auto_shrink([false, true]);
            crate::widgets::scroll_area(&mut content, s, area, |ui| contracts_bar(ui, view, &mut out));
        }
        for (line, done) in view.refinery {
            let tint = if *done { s.accent } else { s.fg };
            content.add(egui::Label::new(RichText::new(line).small().color(color(tint))).truncate());
        }
        if !view.refinery.is_empty() {
            content.add_space(4.0);
        }
        let open = t("ov_bank_oeffnen");
        for (line, needs) in view.bank {
            let tint = if *needs { s.accent } else { s.fg };
            let label = egui::Label::new(RichText::new(line).small().color(color(tint))).truncate().sense(Sense::click());
            let response = content.add(label).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(&open);
            if response.clicked() {
                out.open_bank = true;
            }
            out.drawn.push(line.clone());
        }
        if !view.bank.is_empty() {
            content.add_space(4.0);
        }
        crate::widgets::scroll_area(
            &mut content,
            s,
            ScrollArea::vertical().id_salt("overlay-zeilen").auto_shrink([false, false]),
            |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                for entry in view.rows.iter() {
                    row(ui, s, entry, &mut out);
                }
            },
        );

        let grip = ui.interact(grip_rect, ui.make_persistent_id("overlay-griff"), Sense::drag());
        if let Some(image) = crate::icons::image(grip_icon(view.anchored), s.accent, GRIP - 2.0) {
            image.paint_at(ui, grip_rect.shrink(1.0));
        }
        if grip.drag_started() {
            out.drag = Some(Drag::Resize);
        }
        let cursor = if bottom == right { CursorIcon::ResizeNwSe } else { CursorIcon::ResizeNeSw };
        grip.widget_info(|| WidgetInfo::labeled(Role::Button, true, t("hinweis_groesse")));
        hover_hint(ui, grip.on_hover_cursor(cursor), &t("hinweis_groesse"), &mut out.hint);
    }

    let stroke = Stroke::new(2.0, color(s.accent));
    if s.square {
        for mark in corner_marks(rect) {
            ui.painter().line(mark.to_vec(), stroke);
        }
    } else {
        for mark in rounded_corner_marks(rect, f32::from(radius.nw)) {
            ui.painter().line(mark, stroke);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Pos2;
    use versekit_core::{language, streamer, theme};

    fn rows() -> Rows {
        let mut rows = Rows::default();
        rows.push(
            Row::Found {
                name: "Ultra-Fine Kühler".into(),
                kind: "Kühler".into(),
                code: "Militär · S2 · A".into(),
                time: "12:00:00".into(),
                caught_up: false,
            },
            20,
        );
        rows.push(Row::Hint { text: Message::Plain("Ein Hinweis".into()), contract: None }, 20);
        rows
    }

    /// Draws one frame of `size` points and returns its output and the texts painted.
    fn frame(view: &View, size: Vec2, pointer: Option<Pos2>) -> (Output, Vec<String>) {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        crate::apply(&ctx, view.scheme);
        let mut out = Output::default();
        let mut painted = Vec::new();
        for _ in 0..2 {
            let mut input =
                egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)), ..Default::default() };
            if let Some(p) = pointer {
                input.events.push(egui::Event::PointerMoved(p));
            }
            let mut output = ctx.run_ui(input, |ui| out = show(ui, view));
            output.textures_delta.clear();
            painted = output
                .shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_string()),
                    _ => None,
                })
                .collect();
        }
        (out, painted)
    }

    fn view<'a>(scheme: &'a Scheme, rows: &'a Rows, status: &'a Message, shown: Shown) -> View<'a> {
        View {
            scheme,
            shown,
            rows,
            contracts: &[],
            contracts_max: 10,
            hidden: None,
            refinery: &[],
            bank: &[],
            launcher: false,
            status,
            location: None,
            shard: None,
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
            opacity: 0.93,
            extras: &[],
            menu: true,
            drag_under_buttons: false,
            hangar: None,
            hints_outside: false,
        }
    }

    /// Leaving an armistice zone stands in the open overlay in plain words, in both languages; entering one says the
    /// opposite; nothing stands there without a notice, in the folded bar, or while the game does not run.
    #[test]
    fn the_notice_of_an_armistice_zone_stands_in_the_open_overlay_in_both_languages() {
        let _language = crate::language_lock();
        let scheme = theme::by_id("original").expect("scheme");
        let (rows, status) = (rows(), Message::Plain("Status".into()));
        for choice in [language::Choice::De, language::Choice::En] {
            language::set_thread_language(choice);
            let mut shown = view(scheme, &rows, &status, Shown::Open);
            let (calm, painted) = frame(&shown, vec2(440.0, 300.0), None);
            assert!(!painted.contains(&t("ov_zone_verlassen")), "{choice:?}: no notice, no text");
            assert!(calm.drawn.iter().all(|text| text != &t("ov_zone_verlassen")));
            for (zone, key) in [(Zone::Left, "ov_zone_verlassen"), (Zone::Entered, "ov_zone_betreten")] {
                shown.zone = Some(zone);
                let (out, painted) = frame(&shown, vec2(440.0, 300.0), None);
                assert!(painted.contains(&t(key)), "{choice:?} {zone:?}: the notice is painted: {painted:?}");
                assert!(out.drawn.contains(&t(key)), "{choice:?} {zone:?}: and told as drawn");
                // Counter check: the other notice is not painted with it.
                let other = if zone == Zone::Left { "ov_zone_betreten" } else { "ov_zone_verlassen" };
                assert!(!painted.contains(&t(other)), "{choice:?} {zone:?}: only its own words");
            }
            shown.zone = Some(Zone::Left);
            let mut bar = view(scheme, &rows, &status, Shown::Bar);
            bar.zone = Some(Zone::Left);
            let (_, folded) = frame(&bar, vec2(440.0, 300.0), None);
            assert!(!folded.contains(&t("ov_zone_verlassen")), "{choice:?}: the folded bar shows no notice");
            shown.game = false;
            let (_, idle) = frame(&shown, vec2(440.0, 300.0), None);
            assert!(!idle.contains(&t("ov_zone_verlassen")), "{choice:?}: no notice without the game");
        }
        language::set_thread_language(language::Choice::De);
    }

    /// The bar carries the name of the program and no version number, open and folded: the version stands in the
    /// main window only.
    #[test]
    fn the_bar_shows_the_name_without_a_version_number() {
        let scheme = theme::by_id("original").expect("scheme");
        let (rows, status) = (rows(), Message::Plain("Status".into()));
        // Three numbers joined by dots, as `5.0.0` or `5.0.0-dev`.
        let has_version = |text: &str| {
            text.split(|c: char| !(c.is_ascii_digit() || c == '.'))
                .any(|word| word.split('.').filter(|part| !part.is_empty()).count() >= 3 && !word.contains(".."))
        };
        assert!(has_version("VerseKit 5.0.0-dev") && !has_version("Status 21:14"), "the check sees a version");
        for shown in [Shown::Open, Shown::Bar] {
            let (out, painted) = frame(&view(scheme, &rows, &status, shown), vec2(440.0, 300.0), None);
            assert!(painted.contains(&t("hf_titel")), "{shown:?}: the name stands in the bar: {painted:?}");
            let numbered: Vec<_> = painted.iter().filter(|text| has_version(text)).collect();
            assert!(numbered.is_empty(), "{shown:?}: a version number in the overlay: {numbered:?}");
            assert!(out.title_end > 0.0 && out.title_end < 200.0, "{shown:?}: the title ends after the name");
        }
    }

    #[test]
    fn the_hangar_line_shows_only_when_given_and_is_cut_off_instead_of_widening_the_window() {
        let scheme = theme::by_id("original").expect("scheme");
        let (rows, status) = (rows(), Message::Plain("Status".into()));
        let line = OverlayLine {
            label: "EXHANG".into(),
            phase: Some(Phase::Closed),
            lamps: [Lamp::Green, Lamp::Green, Lamp::Green, Lamp::Green, Lamp::Red],
            text: "öffnet in 14:39".into(),
        };
        let mut shown = view(scheme, &rows, &status, Shown::Open);
        let (without, painted) = frame(&shown, vec2(440.0, 300.0), None);
        assert!(!painted.iter().any(|t| t.contains("EXHANG") || t.contains("14:39")), "{painted:?}");
        assert!(!without.drawn.iter().any(|t| t.contains("EXHANG")));

        shown.hangar = Some(&line);
        let (with, painted) = frame(&shown, vec2(440.0, 300.0), None);
        assert!(
            painted.contains(&"EXHANG".to_string()) && painted.contains(&"öffnet in 14:39".to_string()),
            "{painted:?}"
        );
        assert!(with.drawn.contains(&"EXHANG öffnet in 14:39".to_string()));
        assert_eq!(with.min_width, without.min_width, "the line asks for no more room");

        // Where the line has no room the text is cut with "…" (the galley keeps the whole text and says it
        // was elided), and it stays inside the window.
        let elided = |width: f32| {
            let ctx = egui::Context::default();
            crate::install(&ctx);
            crate::apply(&ctx, scheme);
            let mut found = None;
            for _ in 0..2 {
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 300.0))),
                    ..Default::default()
                };
                let mut output = ctx.run_ui(input, |ui| {
                    show(ui, &shown);
                });
                output.textures_delta.clear();
                found = output.shapes.iter().find_map(|c| match &c.shape {
                    egui::Shape::Text(text) if text.galley.text() == "öffnet in 14:39" => {
                        Some((text.galley.elided, text.pos.x + text.galley.size().x))
                    }
                    _ => None,
                });
            }
            found.expect("the text of the line is drawn")
        };
        let (cut, right_edge) = elided(150.0);
        assert!(cut && right_edge <= 150.0, "cut off at the edge of the window: {cut} {right_edge}");
        assert!(!elided(440.0).0, "whole where there is room");
    }

    /// A drawn text: what it says, its left and right end, its top, and whether it was cut with "…".
    struct Drawn {
        text: String,
        left: f32,
        right: f32,
        top: f32,
        cut: bool,
    }

    /// Draws `view` `width` points wide in the text size `points`, the pointer at `pointer`, tooltips at once.
    fn drawn(view: &View, width: f32, points: i64, pointer: Option<Pos2>) -> Vec<Drawn> {
        drawn_with_width(view, width, points, pointer).1
    }

    /// [`drawn`] with the narrowest width the overlay asks for in that frame.
    fn drawn_with_width(view: &View, width: f32, points: i64, pointer: Option<Pos2>) -> (f32, Vec<Drawn>) {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        crate::apply(&ctx, view.scheme);
        crate::apply_size(&ctx, points);
        ctx.all_styles_mut(|style| {
            style.interaction.tooltip_delay = 0.0;
            style.interaction.show_tooltips_only_when_still = false;
        });
        let (mut found, mut min_width) = (Vec::new(), 0.0);
        for _ in 0..4 {
            let mut input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 400.0))),
                ..Default::default()
            };
            input.events.extend(pointer.map(egui::Event::PointerMoved));
            let mut output = ctx.run_ui(input, |ui| {
                min_width = show(ui, view).min_width;
            });
            output.textures_delta.clear();
            found = output
                .shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::Shape::Text(text) => Some(Drawn {
                        text: text.galley.text().to_string(),
                        // A right-aligned text is laid out from its right end: its rect says where it stands.
                        left: text.pos.x + text.galley.rect.min.x,
                        right: text.pos.x + text.galley.rect.max.x,
                        top: text.pos.y,
                        cut: text.galley.elided,
                    }),
                    _ => None,
                })
                .collect();
        }
        (min_width, found)
    }

    /// The shard stands at the right of the first line in every text size and the narrowest window: whole and inside
    /// it. The status before it is cut where the room ends; its whole name shows on hover; without the game it goes.
    #[test]
    fn the_shard_stands_whole_at_the_right_of_the_first_line_in_every_size() {
        let scheme = theme::by_id("original").expect("scheme");
        let rows = rows();
        let status =
            Message::Plain("444 Baupläne · Log gelesen · geprüft 17:26:00 · nichts Neues seit dem Start".into());
        let (short, whole) = ("euw1b · 420 · FPS 41 · Server 19,5", "Shard: EU-West 12881860-420 · seit 17:26");
        let mut shown = view(scheme, &rows, &status, Shown::Open);
        shown.shard = Some(short);
        shown.shard_hint = Some(whole);
        let (low, high) = versekit_core::font_size::POINTS_RANGE;
        for points in low..=high {
            // The narrowest the overlay lets itself be made in this size: what its bar needs.
            let (bar, _) = drawn_with_width(&shown, 440.0, points, None);
            let narrowest = bar.max(versekit_core::overlay::MIN_BAR_WIDTH as f32).ceil();
            for width in [narrowest, 440.0] {
                let what = format!("Stufe {points}, {width} breit");
                let texts = drawn(&shown, width, points, None);
                let shard = texts.iter().find(|t| t.text == short).unwrap_or_else(|| panic!("{what}: no shard"));
                let (left, right, cut) = (shard.left, shard.right, shard.cut);
                assert!(
                    !cut && right <= width - PAD + 0.5,
                    "{what}: the shard is cut or sticks out: {left}..{right} {cut}"
                );
                assert!(right >= width - PAD - 2.0, "{what}: the shard stands at the right edge: {left}..{right}");
                let first = texts.iter().find(|t| t.text == status.render()).unwrap_or_else(|| panic!("{what}"));
                assert!((first.top - shard.top).abs() < 2.0, "{what}: status and shard share the first line");
                assert!(first.right <= shard.left + 0.5, "{what}: the status runs into the shard");
                if width == narrowest {
                    assert!(first.cut, "{what}: the status gives way, not the shard");
                }
            }
        }
        // On hover the whole name shows.
        let texts = drawn(&shown, 440.0, 1, None);
        assert!(!texts.iter().any(|t| t.text == whole));
        let shard = texts.iter().find(|t| t.text == short).expect("shard");
        let over = Pos2::new((shard.left + shard.right) / 2.0, shard.top + 4.0);
        assert!(drawn(&shown, 440.0, 1, Some(over)).iter().any(|t| t.text == whole), "the hint shows on hover");
        // Without the game the shard goes and the status has the line to itself.
        shown.game = false;
        let texts = drawn(&shown, 440.0, 1, None);
        assert!(!texts.iter().any(|t| t.text == short));
        assert!(texts.iter().any(|t| t.text == status.render() && !t.cut));
    }

    /// What the unit wants of the scanned ores stands whole under the signature line, and only with it.
    #[test]
    fn the_demand_of_the_unit_stands_under_the_signature_line_and_only_with_it() {
        let scheme = theme::by_id("original").expect("scheme");
        let rows = rows();
        let status = Message::Plain("12 Baupläne".into());
        let mut shown = view(scheme, &rows, &status, Shown::Open);
        let (signature, wanted) = ("Signatur: 8,420 → 2× Agricium", "Die Einheit sucht: 40 SCU Agricium (ab Q 650)");
        shown.demand = Some(wanted);
        assert!(!drawn(&shown, 440.0, 1, None).iter().any(|t| t.text == wanted), "no signature line: no demand");
        shown.signature = Some(signature);
        let texts = drawn(&shown, 440.0, 1, None);
        let read = texts.iter().find(|t| t.text == signature).expect("the signature line");
        let line = texts.iter().find(|t| t.text == wanted).expect("the demand is drawn");
        assert!(line.top > read.top, "under the signature line: {} over {}", line.top, read.top);
        assert!(!line.cut, "whole");
    }

    /// Streamer mode: the lines of the overlay (status, hints, the signature line) hide a path, a
    /// learned handle and a mail address; with the mode off they stand as they are, in both languages.
    #[test]
    fn the_overlay_hides_private_values_in_streamer_mode() {
        let scheme = theme::by_id("original").expect("scheme");
        let handle = "Overlaypilot4711";
        streamer::learn(handle);
        let path = r"D:\Spiele\Privat\Starter.exe";
        let mail = "pilot@example.org";
        let status = Message::Plain(format!("Start nicht möglich: {path}, gemeldet von {mail}"));
        let mut rows = rows();
        rows.push(Row::Hint { text: Message::Plain(format!("Willkommen {handle}")), contract: None }, 20);
        let signature = format!("Signal von {handle} in {path}");
        let mut shown = view(scheme, &rows, &status, Shown::Open);
        shown.signature = Some(signature.as_str());
        let secrets = [path, mail, handle];
        for choice in [language::Choice::De, language::Choice::En] {
            language::set_thread_language(choice);
            streamer::set_thread_active(Some(false));
            let (_, open) = frame(&shown, vec2(520.0, 320.0), None);
            let open = open.join("\n");
            streamer::set_thread_active(Some(true));
            let (_, covered) = frame(&shown, vec2(520.0, 320.0), None);
            let covered = covered.join("\n");
            streamer::set_thread_active(None);
            language::set_thread_language(language::Choice::Auto);
            for secret in secrets {
                assert!(open.contains(secret), "{choice:?}: without the mode the overlay shows {secret}\n{open}");
                assert!(!covered.contains(secret), "{choice:?}: the overlay shows {secret} in streamer mode\n{covered}");
            }
            assert!(covered.contains(if choice == language::Choice::De { "[Geschwärzt]" } else { "[Redacted]" }));
        }
    }

    fn extras() -> Vec<Extra> {
        ["star", "circle", "info"]
            .into_iter()
            .map(|icon| Extra {
                action: Action::Extra(icon),
                icon,
                meaning: format!("Probe {icon}"),
                id: icon,
                tone: Tone::Normal,
            })
            .collect()
    }

    const FIXED: [Action; 6] =
        [Action::Quit, Action::Clear, Action::Fold, Action::Lock, Action::Rescan, Action::Overview];

    /// Nothing cut off: every button inside the bar, right of the title,
    /// none on another.
    fn nothing_cut_off(out: &Output, width: f32) -> Result<(), String> {
        let mut rects: Vec<Rect> = out.buttons.iter().map(|(_, r)| *r).chain(out.more).collect();
        rects.sort_by(|a, b| a.min.x.total_cmp(&b.min.x));
        for r in &rects {
            if r.min.x < 0.0 || r.max.x > width + 0.5 {
                return Err(format!("{r:?} outside 0..{width}"));
            }
        }
        let right: Vec<&Rect> = rects.iter().filter(|r| r.min.x >= out.title_end - 0.5).collect();
        if right.len() + out.buttons.iter().filter(|(_, r)| r.max.x <= out.title_end + 0.5).count() != rects.len() {
            return Err(format!("a button on the title, which ends at {}", out.title_end));
        }
        if rects.windows(2).any(|w| w[0].max.x > w[1].min.x + 0.5) {
            return Err("two buttons on each other".into());
        }
        Ok(())
    }

    /// The colours of the icons painted inside `rect` of a frame of `view`: an icon is a textured rectangle of
    /// the size of a bar icon, filled with its tint (or a mesh with the tint at its corners).
    fn colours_in(view: &View, rect: Rect) -> Vec<Color32> {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        crate::apply(&ctx, view.scheme);
        let mut colours = Vec::new();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, BAR_HEIGHT))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                show(ui, view);
            });
            output.textures_delta.clear();
            colours = output
                .shapes
                .iter()
                .flat_map(|c| match &c.shape {
                    egui::Shape::Rect(icon)
                        if icon.rect.size() == Vec2::splat(BAR_ICON) && rect.contains_rect(icon.rect) =>
                    {
                        vec![icon.fill]
                    }
                    egui::Shape::Mesh(mesh) => {
                        mesh.vertices.iter().filter(|v| rect.contains(v.pos)).map(|v| v.color).collect()
                    }
                    _ => Vec::new(),
                })
                .collect();
        }
        colours
    }

    /// An extra stands in the colour of its tone: anti-AFK grey when off, in the accent while it waits, yellow
    /// for a press, red when it cannot press.
    #[test]
    fn an_extra_is_drawn_in_the_colour_of_its_tone() {
        use versekit_core::anti_afk::Look;
        let rows = rows();
        let status = Message::Plain(String::new());
        for scheme in theme::schemes() {
            let normal = scheme.icon(IconTint::Normal);
            let looks = [
                (Look::Off, normal),
                (Look::Waiting, scheme.accent),
                (Look::Pressed, scheme.yellow),
                (Look::Stopped, scheme.red),
                (Look::Blind, scheme.red),
            ];
            for (look, expected) in looks {
                let tone = Tone::of_anti_afk(look);
                assert_eq!(tone.tint(scheme, normal), expected, "{}: {look:?}", scheme.id);
                let extras =
                    [Extra { action: Action::Extra("afk"), icon: "timer", meaning: "afk".into(), id: "afk", tone }];
                let mut v = view(scheme, &rows, &status, Shown::Bar);
                v.extras = &extras;
                let (out, _) = frame(&v, vec2(900.0, BAR_HEIGHT), None);
                let (_, rect) = *out.buttons.iter().find(|(a, _)| *a == Action::Extra("afk")).expect("the button");
                let colours = colours_in(&v, rect);
                assert!(!colours.is_empty(), "{}: the icon is painted", scheme.id);
                assert!(
                    colours.iter().all(|c| *c == color(expected)),
                    "{}: {look:?} wants {:?}, painted {:?}",
                    scheme.id,
                    color(expected),
                    colours.iter().map(|c| format!("{c:?}")).collect::<std::collections::BTreeSet<_>>()
                );
            }
        }
    }

    /// Narrow: the fixed buttons all there, the extras behind "⋯".
    /// Wide: every extra in the bar, no "⋯". Nothing cut off either way.
    #[test]
    fn extras_stand_in_the_bar_while_there_is_room() {
        let rows = rows();
        let status = Message::Plain("2 Baupläne".into());
        let extras = extras();
        for scheme in theme::schemes() {
            let mut v = view(scheme, &rows, &status, Shown::Bar);
            v.extras = &extras;
            let (first, _) = frame(&v, vec2(900.0, BAR_HEIGHT), None);
            let narrow = first.min_width;
            let (out, _) = frame(&v, vec2(narrow, BAR_HEIGHT), None);
            for action in FIXED {
                assert!(out.buttons.iter().any(|(a, _)| *a == action), "{}: {action:?} fehlt", scheme.id);
            }
            assert!(out.more.is_some(), "{}: no ⋯ at {narrow}", scheme.id);
            assert_eq!(out.in_menu.len(), 3, "{}: at the narrowest no extra fits", scheme.id);
            nothing_cut_off(&out, narrow).unwrap_or_else(|e| panic!("{} schmal: {e}", scheme.id));

            let (wide, _) = frame(&v, vec2(900.0, BAR_HEIGHT), None);
            assert!(wide.more.is_none() && wide.in_menu.is_empty(), "{}", scheme.id);
            for extra in &extras {
                assert!(wide.buttons.iter().any(|(a, _)| *a == extra.action), "{}", scheme.id);
            }
            nothing_cut_off(&wide, 900.0).unwrap_or_else(|e| panic!("{} breit: {e}", scheme.id));

            // One place more than the narrowest: one extra stands, two wait.
            let (between, _) = frame(&v, vec2(narrow + BAR_BUTTON, BAR_HEIGHT), None);
            assert_eq!(between.in_menu.len(), 2, "{}", scheme.id);
            nothing_cut_off(&between, narrow + BAR_BUTTON).unwrap_or_else(|e| panic!("{} dazwischen: {e}", scheme.id));
        }
        assert_eq!(extras_direct(300.0, 100.0, 6, 0), 0);
        assert_eq!(extras_direct(1000.0, 100.0, 6, 3), 3);
    }

    /// "⋯" opens the hidden extras where the title was; a click on one
    /// acts and closes it again.
    /// The hint shown after the pointer rests at `at` for a while.
    fn hint_at(v: &View, at: Pos2) -> Option<String> {
        hinted(v, at).hint
    }

    /// What the frame says after the pointer rests at `at` for a while.
    fn hinted(v: &View, at: Pos2) -> Output {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        crate::apply(&ctx, v.scheme);
        let mut out = Output::default();
        for n in 0..8 {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(600.0, 300.0))),
                time: Some(f64::from(n) * 0.3),
                events: if n == 0 { vec![egui::Event::PointerMoved(at)] } else { vec![] },
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| out = show(ui, v));
            output.textures_delta.clear();
        }
        out
    }

    /// The hint says where its symbol stands, for the window of the hint to keep clear of it, in both ways of
    /// showing hints, and says nothing while there is no hint.
    #[test]
    fn a_hint_tells_where_its_symbol_stands() {
        let rows = rows();
        let status = Message::Plain("x".into());
        let scheme = theme::by_id("original").expect("original");
        let mut v = view(scheme, &rows, &status, Shown::Open);
        let (out, _) = frame(&v, vec2(600.0, 300.0), None);
        let button = out.buttons.iter().find(|(a, _)| *a == Action::Rescan).map(|(_, r)| *r).expect("rescan");
        for outside in [false, true] {
            v.hints_outside = outside;
            let shown = hinted(&v, button.center());
            assert_eq!(shown.hint, Some(t("ov_h_neulesen")), "outside {outside}");
            let seen = shown.hint_rect.unwrap_or_else(|| panic!("outside {outside}: no place for the hint"));
            assert!(seen.contains(button.center()) && seen.width() <= button.width() + 1.0, "{seen:?} vs {button:?}");
            let idle = hinted(&v, pos2(300.0, 150.0));
            assert!(idle.hint.is_none() && idle.hint_rect.is_none(), "no hint and no place away from every symbol");
        }
    }

    /// Every button of the bar shows its own hint, as 4.12 did; the free bar
    /// the one for moving the window.
    #[test]
    fn every_button_shows_its_own_hint() {
        let rows = rows();
        let status = Message::Plain("2 Baupläne".into());
        let scheme = theme::by_id("original").expect("original");
        let v = view(scheme, &rows, &status, Shown::Open);
        let (out, _) = frame(&v, vec2(600.0, 300.0), None);
        let own = [
            (Action::Quit, "hinweis_schliessen"),
            (Action::Clear, "ov_h_leeren"),
            (Action::Fold, "ov_h_einklappen"),
            (Action::Lock, "hinweis_schloss_zu"),
            (Action::Rescan, "ov_h_neulesen"),
            (Action::Overview, "hinweis_einstellungen"),
        ];
        for (action, key) in own {
            let at = out.buttons.iter().find(|(a, _)| *a == action).map(|(_, r)| r.center());
            let at = at.unwrap_or_else(|| panic!("{action:?} is drawn"));
            assert_eq!(hint_at(&v, at), Some(t(key)), "{action:?}");
        }
        let title = pos2(PAD + 4.0, BAR_HEIGHT / 2.0);
        assert_eq!(hint_at(&v, title), Some(t("hinweis_ziehen")), "the free bar");
    }

    /// The keys of the hints of every symbol the overlay shows: its bar, the buttons that other parts put into it
    /// (the eye, the graduation cap, the timer, the bell), its strip, its rows and its lamps.
    const SYMBOL_HINTS: &[&str] = &[
        "hinweis_ziehen",
        "hinweis_groesse",
        "hinweis_schliessen",
        "ov_h_leeren",
        "ov_h_einklappen",
        "hinweis_ausklappen",
        "hinweis_schloss",
        "hinweis_schloss_zu",
        "ov_h_neulesen",
        "hinweis_einstellungen",
        "s_sp_start",
        "ov_h_mehr",
        "ov_h_mehr_zu",
        "ov_streifen_h",
        "hinweis_scanner_an",
        "hinweis_scanner_tot",
        "hinweis_scanner_aus",
        "hinweis_anlernen",
        "hinweis_aafk_an",
        "hinweis_aafk_aus",
        "hinweis_aafk_gedrueckt",
        "hinweis_aafk_gestoppt",
        "hinweis_aafk_blind",
        "hinweis_versionen",
        "hf_hinweis_update_da",
        "ov_auftrag_weg",
        "s_p9_rueckgaengig",
        "s_cz_lampen",
        "ov_h_hangar",
        "ov_h_ziel_offen",
        "ov_h_zeile_gefunden",
        "ov_h_zeile_hinweis",
        "ov_h_zeile_katalog",
        "ov_h_zeile_merkliste",
    ];

    /// No symbol is left without words: every hint exists in both language files itself (an empty English text
    /// would silently fall back to German) and is no bare key.
    #[test]
    fn every_symbol_of_the_overlay_has_a_hint_in_both_languages() {
        for (index, name) in [(0, "de"), (1, "en")] {
            let table = &language::tables()[index];
            for key in SYMBOL_HINTS {
                let text = table.texts.get(*key).map_or("", String::as_str).trim();
                assert!(!text.is_empty(), "{name}: the hint {key} is missing or empty");
                assert_ne!(text, *key, "{name}: the hint {key} is its own key");
            }
        }
        // The buttons themselves: whatever state the bar is in, each has a hint that is not empty.
        let scheme = theme::by_id("original").expect("original");
        let (rows, status) = (rows(), Message::Plain("x".into()));
        for lang in [language::Lang::De, language::Lang::En] {
            language::with_language(lang, || {
                for (open, locked, launcher) in [(true, false, false), (false, true, true)] {
                    let mut v = view(scheme, &rows, &status, if open { Shown::Open } else { Shown::Bar });
                    (v.locked, v.launcher) = (locked, launcher);
                    let buttons = bar_buttons(&v);
                    assert_eq!(buttons.len(), if launcher { 7 } else { 6 }, "{lang:?}: the buttons of the bar");
                    for (action, _, _, meaning, _) in buttons {
                        assert!(!meaning.trim().is_empty(), "{lang:?}: {action:?} has no hint");
                        assert!(
                            !meaning.starts_with("ov_") && !meaning.starts_with("hinweis_"),
                            "{lang:?}: {action:?} shows a key: {meaning}"
                        );
                    }
                }
            });
        }
    }

    /// The icons in front of the rows say what they mean on hover: the green tick, the grey "i", the blue circle
    /// and the gold star, each its own words.
    #[test]
    fn the_icons_of_the_rows_say_what_they_mean_on_hover() {
        let scheme = theme::by_id("original").expect("original");
        let mut rows = Rows::default();
        let catalog = |watch_title: Option<&str>| Row::Catalog {
            name: "Probe".into(),
            kind: "Kühler".into(),
            time: "12:00:00".into(),
            watch_title: watch_title.map(str::to_string),
        };
        rows.push(catalog(Some("Meine Liste")), 20);
        rows.push(catalog(None), 20);
        rows.push(Row::Hint { text: Message::Plain("Ein Hinweis".into()), contract: None }, 20);
        rows.push(
            Row::Found {
                name: "Ultra-Fine Kühler".into(),
                kind: "Kühler".into(),
                code: "S2".into(),
                time: "12:00:00".into(),
                caught_up: false,
            },
            20,
        );
        let status = Message::Plain("x".into());
        let mut v = view(scheme, &rows, &status, Shown::Open);
        // As a tooltip, and as the program shows its hints: in the window of their own (`hints_outside`).
        for outside in [false, true] {
            v.hints_outside = outside;
            let mut seen = std::collections::BTreeSet::new();
            for y in (30..300).step_by(4) {
                for x in [10.0, 16.0, 22.0] {
                    seen.extend(hint_at(&v, pos2(x, y as f32)));
                }
            }
            for key in ["ov_h_zeile_gefunden", "ov_h_zeile_hinweis", "ov_h_zeile_katalog", "ov_h_zeile_merkliste"] {
                assert!(seen.contains(&t(key)), "outside {outside}: the icon with {key} shows no hint: {seen:?}");
            }
        }
    }

    /// The circle before an open objective and the name of the server hint too, the way the program shows its
    /// hints (in the window of their own).
    #[test]
    fn the_open_objective_and_the_server_name_hint_in_the_window_of_their_own() {
        let scheme = theme::by_id("original").expect("original");
        let (rows, status) = (rows(), Message::Plain("x".into()));
        let lines = contract_lines();
        let mut v = view(scheme, &rows, &status, Shown::Open);
        v.hints_outside = true;
        v.contracts = &lines;
        let mut seen = std::collections::BTreeSet::new();
        for y in (60..260).step_by(4) {
            for x in (12..44).step_by(4) {
                seen.extend(hint_at(&v, pos2(x as f32, y as f32)));
            }
        }
        assert!(seen.contains(&t("ov_h_ziel_offen")), "the circle of an open objective: {seen:?}");
        let (short, whole) = ("euw1b · 420", "Shard: EU-West 12881860-420 · seit 17:26");
        v.contracts = &[];
        (v.shard, v.shard_hint) = (Some(short), Some(whole));
        let mut seen = std::collections::BTreeSet::new();
        for y in (30..52).step_by(4) {
            for x in (400..596).step_by(6) {
                seen.extend(hint_at(&v, pos2(x as f32, y as f32)));
            }
        }
        assert!(seen.contains(whole), "the name of the server: {seen:?}");
    }

    /// The grip in the corner and the lamps of the hangar hand their hint to the window of its own too: a
    /// tooltip there would be cut off at the edge of the overlay.
    #[test]
    fn the_grip_and_the_lamps_hint_like_the_buttons() {
        let scheme = theme::by_id("original").expect("original");
        let (rows, status) = (rows(), Message::Plain("x".into()));
        let mut v = view(scheme, &rows, &status, Shown::Open);
        v.hints_outside = true;
        let size = vec2(600.0, 300.0);
        let grip = hint_at(&v, pos2(size.x - 6.0, size.y - 6.0));
        assert_eq!(grip, Some(t("hinweis_groesse")), "the grip");
        let line = OverlayLine {
            label: "EH".into(),
            phase: Some(Phase::Open),
            lamps: [Lamp::Green, Lamp::Green, Lamp::Dark, Lamp::Dark, Lamp::Dark],
            text: "offen".into(),
        };
        v.hangar = Some(&line);
        let mut seen = std::collections::BTreeSet::new();
        for y in (30..120).step_by(3) {
            for x in (8..90).step_by(4) {
                seen.extend(hint_at(&v, pos2(x as f32, y as f32)));
            }
        }
        let lamps = tf("s_cz_lampen", &[("0", 2.into()), ("1", LAMPS.into())]);
        let meaning = tf("ov_h_hangar", &[("0", lamps.as_str().into())]);
        assert!(seen.contains(&meaning), "the lamps: {seen:?}");
    }

    /// The rocket and the locked lock, the two states the other test of the bar leaves out, say what they do.
    #[test]
    fn the_rocket_and_the_locked_lock_show_their_hints() {
        let scheme = theme::by_id("original").expect("original");
        let (rows, status) = (rows(), Message::Plain("x".into()));
        let mut v = view(scheme, &rows, &status, Shown::Open);
        (v.launcher, v.locked) = (true, true);
        let (out, _) = frame(&v, vec2(600.0, 300.0), None);
        for (action, key) in [(Action::Launch, "s_sp_start"), (Action::Lock, "hinweis_schloss")] {
            let at = out.buttons.iter().find(|(a, _)| *a == action).map(|(_, r)| r.center());
            let at = at.unwrap_or_else(|| panic!("{action:?} is drawn"));
            assert_eq!(hint_at(&v, at), Some(t(key)), "{action:?}");
        }
        // The fold button in the folded bar says the other thing.
        let folded = view(scheme, &rows, &status, Shown::Bar);
        let (out, _) = frame(&folded, vec2(600.0, BAR_HEIGHT), None);
        let at = out.buttons.iter().find(|(a, _)| *a == Action::Fold).map(|(_, r)| r.center()).expect("fold");
        assert_eq!(hint_at(&folded, at), Some(t("hinweis_ausklappen")), "unfold");
    }

    /// With the hints in a window of their own the folded bar names a button's hint once the pointer rests on it,
    /// and draws no tooltip of its own, which the window of the bar would cut off.
    #[test]
    fn the_hint_goes_out_of_the_window_when_asked() {
        let rows = rows();
        let status = Message::Plain("2 Baupläne".into());
        let scheme = theme::by_id("original").expect("original");
        let mut v = view(scheme, &rows, &status, Shown::Bar);
        v.hints_outside = true;
        let (out, _) = frame(&v, vec2(600.0, BAR_HEIGHT), None);
        let at = out.buttons.iter().find(|(a, _)| *a == Action::Rescan).map(|(_, r)| r.center()).expect("rescan");
        assert_eq!(hint_at(&v, at), Some(t("ov_h_neulesen")), "the hint for the window of its own");
        let ctx = egui::Context::default();
        crate::install(&ctx);
        crate::apply(&ctx, scheme);
        let mut tooltip = false;
        for n in 0..8 {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(600.0, BAR_HEIGHT))),
                time: Some(f64::from(n) * 0.3),
                events: if n == 0 { vec![egui::Event::PointerMoved(at)] } else { vec![] },
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                show(ui, &v);
                tooltip |= ui.ctx().memory(|m| m.layer_ids().any(|l| l.order == egui::Order::Tooltip));
            });
            output.textures_delta.clear();
        }
        assert!(!tooltip, "no tooltip in the window");
        // Counter-check: without the window of its own the tooltip is there.
        v.hints_outside = false;
        let mut tooltip = false;
        for n in 0..8 {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(600.0, BAR_HEIGHT))),
                time: Some(10.0 + f64::from(n) * 0.3),
                events: if n == 0 { vec![egui::Event::PointerMoved(at)] } else { vec![] },
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                show(ui, &v);
                tooltip |= ui.ctx().memory(|m| m.layer_ids().any(|l| l.order == egui::Order::Tooltip));
            });
            output.textures_delta.clear();
        }
        assert!(tooltip, "the tooltip of egui");
    }

    #[test]
    fn the_menu_opens_in_the_bar_and_closes_after_a_click() {
        let rows = rows();
        let status = Message::Plain("2 Baupläne".into());
        let extras = extras();
        let scheme = theme::by_id("original").expect("original");
        let mut v = view(scheme, &rows, &status, Shown::Bar);
        v.extras = &extras;
        let ctx = egui::Context::default();
        crate::install(&ctx);
        crate::apply(&ctx, scheme);
        let mut size = vec2(900.0, BAR_HEIGHT);
        let run = |events: Vec<egui::Event>, size: Vec2| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                events,
                ..Default::default()
            };
            let mut out = Output::default();
            let mut output = ctx.run_ui(input, |ui| out = show(ui, &v));
            output.textures_delta.clear();
            out
        };
        let click = |p: Pos2| {
            vec![
                egui::Event::PointerMoved(p),
                egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
                egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
            ]
        };
        size.x = run(vec![], size).min_width;
        let closed = run(vec![], size);
        let more = closed.more.expect("⋯ at the narrowest");
        run(click(more.center()), size);
        let open = run(vec![], size);
        assert!(open.in_menu.is_empty(), "the menu is open");
        let first = open.buttons.iter().find(|(a, _)| *a == Action::Extra("star")).map(|(_, r)| *r);
        let first = first.expect("the hidden extra stands where the title was");
        assert!(first.max.x <= open.title_end + 0.5);
        assert!(open.min_width <= size.x + 0.5, "the open menu fits where the title was");
        nothing_cut_off(&open, size.x).unwrap_or_else(|e| panic!("offen: {e}"));
        let acted = run(click(first.center()), size);
        assert_eq!(acted.actions, vec![Action::Extra("star")]);
        let again = run(vec![], size);
        assert_eq!(again.in_menu.len(), 3, "closed after the click");
    }

    #[test]
    fn an_open_overlay_shows_its_rows_and_buttons() {
        let rows = rows();
        let status = Message::Plain("2 Baupläne".into());
        for scheme in theme::schemes() {
            let (out, painted) = frame(&view(scheme, &rows, &status, Shown::Open), vec2(420.0, 500.0), None);
            for text in ["Ultra-Fine Kühler", "Ein Hinweis", "2 Baupläne"] {
                assert!(painted.iter().any(|p| p == text), "{}: {text} fehlt in {painted:?}", scheme.id);
            }
            assert!(out.lock.is_some(), "the lock is drawn");
            assert!(out.min_width > 150.0 && out.min_width < 420.0, "{}", out.min_width);
        }
    }

    #[test]
    fn a_folded_overlay_shows_only_its_bar() {
        let rows = rows();
        let status = Message::Plain("2 Baupläne".into());
        let theme = theme::by_id("original").expect("original");
        let (out, painted) = frame(&view(theme, &rows, &status, Shown::Bar), vec2(320.0, BAR_HEIGHT), None);
        assert!(!painted.iter().any(|p| p == "Ultra-Fine Kühler"), "rows hidden while folded");
        assert!(out.drawn.is_empty());
        assert!(out.lock.is_some(), "the bar keeps its buttons");
    }

    /// The pop-up cycle drawn: waiting strip without rows, then open again with them.
    #[test]
    fn the_strip_is_an_eye_alone_and_the_rows_come_back() {
        let rows = rows();
        let status = Message::Plain("2 Baupläne".into());
        let scheme = theme::by_id("krt").expect("krt");
        let mut v = view(scheme, &rows, &status, Shown::Open);
        let (open, _) = frame(&v, vec2(420.0, 500.0), None);
        assert!(open.drawn.iter().any(|d| d == "Ultra-Fine Kühler"));
        v.shown = Shown::Strip;
        let (strip, painted) = frame(&v, vec2(200.0, STRIP_HEIGHT), Some(pos2(10.0, 10.0)));
        assert!(painted.is_empty(), "the strip writes nothing beside the eye: {painted:?}");
        assert!(strip.drawn.is_empty(), "{:?}", strip.drawn);
        assert_eq!(strip.strip.x, PAD + BAR_ICON + PAD, "as wide as the eye");
        v.shown = Shown::Open;
        let (back, _) = frame(&v, vec2(420.0, 500.0), None);
        assert_eq!(back.drawn, open.drawn, "the same content after coming back");
    }

    /// The strip says what it does on hover, in a hint and as the name for screen readers.
    #[test]
    fn the_eye_of_the_strip_says_what_it_does_on_hover() {
        // The hint and the text it is compared with in one language, whatever other tests switch meanwhile.
        language::set_thread_language(language::Choice::De);
        let rows = rows();
        let status = Message::Plain("2 Baupläne".into());
        let scheme = theme::by_id("original").expect("original");
        let mut v = view(scheme, &rows, &status, Shown::Strip);
        v.hints_outside = true;
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let mut last = Output::default();
        for second in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(40.0, STRIP_HEIGHT))),
                events: if second == 0 { vec![egui::Event::PointerMoved(pos2(12.0, 10.0))] } else { Vec::new() },
                time: Some(f64::from(second)),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| last = show(ui, &v));
            // Headless: no renderer takes the textures.
            output.textures_delta.clear();
        }
        assert_eq!(last.hint, Some(t("ov_streifen_h")), "the hint of the eye");
        assert!(last.drawn.is_empty());
    }

    /// The lines that only told the state are gone: no "game not running", no countdown, no waiting placeholder.
    /// The update line stays while the game runs.
    #[test]
    fn the_overlay_draws_no_state_lines() {
        let rows = Rows::default();
        let status = Message::Plain("0".into());
        let scheme = theme::by_id("original").expect("original");
        for choice in [language::Choice::De, language::Choice::En] {
            language::set_thread_language(choice);
            let mut v = view(scheme, &rows, &status, Shown::Open);
            v.update = Some("5.0.1");
            let state_line = |painted: &[String]| -> Vec<String> {
                painted.iter().filter(|p| GONE_LINES.iter().any(|gone| p.contains(gone))).cloned().collect()
            };
            let (out, painted) = frame(&v, vec2(420.0, 400.0), None);
            assert!(painted.contains(&tf("ov_update", &[("0", "5.0.1".into())])), "{choice:?}: {painted:?}");
            assert!(out.drawn.is_empty(), "{choice:?}: no placeholder while empty: {:?}", out.drawn);
            assert_eq!(state_line(&painted), Vec::<String>::new(), "{choice:?}");
            v.game = false;
            let (_, painted) = frame(&v, vec2(420.0, 400.0), None);
            assert!(!painted.iter().any(|p| p.contains("5.0.1")), "no update hint without the game");
            assert_eq!(state_line(&painted), Vec::<String>::new(), "{choice:?}: no line about the game");
            assert!(painted.contains(&"0".to_string()), "{choice:?}: the status stays: {painted:?}");
            v.shown = Shown::Strip;
            let (_, painted) = frame(&v, vec2(200.0, STRIP_HEIGHT), Some(pos2(10.0, 10.0)));
            assert_eq!(state_line(&painted), Vec::<String>::new(), "{choice:?}: the strip");
        }
        language::set_thread_language(language::Choice::De);
    }

    /// The wordings of the state lines the overlay no longer draws, in both languages.
    const GONE_LINES: [&str; 12] = [
        "Overlay wartet",
        "Overlay waiting",
        "Star Citizen läuft nicht",
        "Star Citizen is not running",
        "Bleibt, solange",
        "Stays while the pointer",
        "Blendet in",
        "Hides in",
        "Klappt in",
        "Folds in",
        "Warte auf neue Baupläne",
        "Waiting for new blueprints",
    ];

    /// The default size shows 5 to 7 rows in every scheme and text size.
    #[test]
    fn the_default_size_shows_about_six_rows() {
        let mut many = Rows::default();
        for n in 0..12 {
            let name = format!("Bauplan {n}");
            many.push(
                Row::Found {
                    name,
                    kind: "Kühler".into(),
                    code: "S1 · A".into(),
                    time: "12:00".into(),
                    caught_up: false,
                },
                20,
            );
        }
        let status = Message::Plain("12 Baupläne".into());
        for scheme in theme::schemes() {
            for points in [0, 1, 4, 7] {
                let ctx = egui::Context::default();
                crate::install(&ctx);
                crate::apply(&ctx, scheme);
                ctx.set_zoom_factor(versekit_core::font_size::zoom_factor(points));
                let mut size = Vec2::ZERO;
                let mut seen = 0;
                for round in 0..3 {
                    let screen = if round == 0 { vec2(800.0, 800.0) } else { size };
                    let input = egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, screen)),
                        ..Default::default()
                    };
                    let mut output = ctx.run_ui(input, |ui| {
                        size = default_size(ui);
                        seen = show(ui, &view(scheme, &many, &status, Shown::Open)).visible_rows;
                    });
                    output.textures_delta.clear();
                }
                assert!((5..=7).contains(&seen), "{} Stufe {points}: {seen} Zeilen bei {size:?}", scheme.id);
            }
        }
    }

    #[test]
    fn the_grip_never_lies_in_the_bar_and_the_cross_says_quit_in_every_corner() {
        let scheme = theme::by_id("original").expect("original");
        let (rows, status) = (rows(), Message::Plain("x".into()));
        // The size `hint_at` draws in.
        let size = vec2(600.0, 300.0);
        for bar in [BarSide::Top, BarSide::Bottom] {
            for anchored in [(false, false), (false, true), (true, false), (true, true)] {
                let mut v = view(scheme, &rows, &status, Shown::Open);
                (v.bar, v.anchored) = (bar, anchored);
                let (out, _) = frame(&v, size, None);
                let what = format!("bar {bar:?}, anchored {anchored:?}");
                let grip = out.grip.unwrap_or_else(|| panic!("{what}: no grip"));
                assert!(
                    Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(grip),
                    "{what}: grip inside the window"
                );
                let bar_rect = match bar {
                    BarSide::Top => Rect::from_min_size(egui::Pos2::ZERO, vec2(size.x, BAR_HEIGHT)),
                    BarSide::Bottom => Rect::from_min_max(pos2(0.0, size.y - BAR_HEIGHT), pos2(size.x, size.y)),
                };
                assert!(!grip.intersects(bar_rect), "{what}: the grip {grip:?} lies in the bar {bar_rect:?}");
                for (action, button) in &out.buttons {
                    assert!(!grip.intersects(*button), "{what}: the grip covers {action:?} {button:?}");
                }
                // The pointer on the cross: its own hint, as a tooltip and in the window of its own.
                let quit = out.buttons.iter().find(|(a, _)| *a == Action::Quit).map(|(_, r)| r.center()).expect("quit");
                for outside in [false, true] {
                    v.hints_outside = outside;
                    assert_eq!(hint_at(&v, quit), Some(t("hinweis_schliessen")), "{what}, outside {outside}");
                }
            }
        }
    }

    /// With the bar on top the grip stands in the first line of the body (right under the bar) in the two corners at
    /// the top: the status and the shard of that line keep clear of it, in every text size and at the narrowest
    /// window, with and without the shard.
    #[test]
    fn the_grip_under_the_bar_does_not_cut_the_first_line() {
        let scheme = theme::by_id("original").expect("original");
        let rows = rows();
        let status = Message::Plain("444 Baupläne · Log gelesen · geprüft 17:26:00".into());
        let (short, whole) = ("euw1b · 420 · FPS 41", "Shard: EU-West 12881860-420");
        let (low, high) = versekit_core::font_size::POINTS_RANGE;
        for anchored in [(true, false), (true, true)] {
            for with_shard in [false, true] {
                for points in [low, high] {
                    let mut v = view(scheme, &rows, &status, Shown::Open);
                    (v.bar, v.anchored) = (BarSide::Top, anchored);
                    if with_shard {
                        (v.shard, v.shard_hint) = (Some(short), Some(whole));
                    }
                    let (narrowest, _) = drawn_with_width(&v, 440.0, points, None);
                    let width = narrowest.max(versekit_core::overlay::MIN_BAR_WIDTH as f32).ceil();
                    for width in [width, 440.0] {
                        let what = format!("anchored {anchored:?}, shard {with_shard}, size {points}, width {width}");
                        let ctx = egui::Context::default();
                        crate::install(&ctx);
                        crate::apply(&ctx, scheme);
                        crate::apply_size(&ctx, points);
                        let mut grip = None;
                        let mut texts = Vec::new();
                        for _ in 0..3 {
                            let input = egui::RawInput {
                                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 400.0))),
                                ..Default::default()
                            };
                            let mut output = ctx.run_ui(input, |ui| grip = show(ui, &v).grip);
                            output.textures_delta.clear();
                            texts = output
                                .shapes
                                .iter()
                                .filter_map(|c| match &c.shape {
                                    egui::Shape::Text(text) => Some((
                                        text.galley.text().to_string(),
                                        Rect::from_min_size(
                                            text.pos + text.galley.rect.min.to_vec2(),
                                            text.galley.rect.size(),
                                        ),
                                    )),
                                    _ => None,
                                })
                                .collect();
                        }
                        let grip = grip.unwrap_or_else(|| panic!("{what}: no grip"));
                        let first_line: Vec<_> =
                            texts.iter().filter(|(text, _)| text == &status.render() || text == short).collect();
                        assert!(!first_line.is_empty(), "{what}: the first line is drawn");
                        for (text, rect) in first_line {
                            assert!(!rect.intersects(grip), "{what}: the grip {grip:?} cuts {text:?} at {rect:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_grip_points_where_the_window_grows() {
        assert_eq!(grip_icon((false, false)), "arrow-down-right");
        assert_eq!(grip_icon((true, true)), "arrow-up-left");
        for anchored in [(false, false), (false, true), (true, false), (true, true)] {
            assert!(crate::icons::source(grip_icon(anchored)).is_some());
        }
    }

    #[test]
    fn a_click_on_a_bar_button_is_reported() {
        let rows = rows();
        let status = Message::Plain("x".into());
        let scheme = theme::by_id("original").expect("original");
        let v = view(scheme, &rows, &status, Shown::Open);
        let size = vec2(420.0, 300.0);
        let (first, _) = frame(&v, size, None);
        let lock = first.lock.expect("lock drawn").center();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let mut seen = Vec::new();
        let events = [
            vec![egui::Event::PointerMoved(lock)],
            vec![egui::Event::PointerButton {
                pos: lock,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }],
            vec![egui::Event::PointerButton {
                pos: lock,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
            vec![],
        ];
        for events in events {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| seen.extend(show(ui, &v).actions));
            output.textures_delta.clear();
        }
        assert_eq!(seen, vec![Action::Lock]);
    }

    fn line(title: &str, text: &str, goals: &[&str], grey: bool) -> ContractLine {
        ContractLine {
            title: title.into(),
            text: Message::Plain(text.into()),
            goals: goals.iter().map(|g| (*g).to_string()).collect(),
            grey,
        }
    }

    /// Three lines: one with objectives (more than fit), a grey one, a bare one.
    fn contract_lines() -> Vec<ContractLine> {
        let many: Vec<String> = (1..=8).map(|n| format!("Ziel {n}")).collect();
        let many: Vec<&str> = many.iter().map(String::as_str).collect();
        vec![
            line(
                "Eins",
                "Auftrag angenommen: Eins  →  Baupläne 1/3 · dir fehlt: Ein sehr langer Name eines Bauplans",
                &many,
                false,
            ),
            line("Zwei", "Auftrag angenommen: Zwei  →  Baupläne 0/2", &["Nicht gezeigt"], true),
            line("Drei", "Auftrag angenommen: Drei", &[], false),
        ]
    }

    #[test]
    fn the_refinery_jobs_stand_under_the_contracts_and_take_no_room_without_any() {
        let rows = rows();
        let status = Message::Plain("x".into());
        let jobs = vec![("Gold · noch 1m 0s".to_string(), false), ("Iron · fertig, abholen".to_string(), true)];
        for scheme in theme::schemes() {
            let mut v = view(scheme, &rows, &status, Shown::Open);
            let (_, without) = frame(&v, vec2(420.0, 400.0), None);
            assert!(!without.iter().any(|p| p.starts_with("Gold") || p.starts_with("Iron")), "{}", scheme.id);
            v.refinery = &jobs;
            let (_, painted) = frame(&v, vec2(420.0, 400.0), None);
            for (line, _) in &jobs {
                assert!(painted.contains(line), "{}: {line} in {painted:?}", scheme.id);
            }
        }
    }

    /// The bar says what runs: header, a line each, the objectives indented
    /// and counted past the limit, the reason for a grey one, a button to hide
    /// each; and it takes no room when nothing runs.
    #[test]
    fn the_bar_of_running_contracts_shows_what_runs_and_nothing_else() {
        // The lines and the texts they are compared with in one language, whatever other tests switch meanwhile.
        language::set_thread_language(language::Choice::De);
        let rows = rows();
        let status = Message::Plain("x".into());
        let lines = contract_lines();
        for scheme in theme::schemes() {
            let mut v = view(scheme, &rows, &status, Shown::Open);
            let (empty, _) = frame(&v, vec2(420.0, 400.0), None);
            assert!(empty.contract_texts.is_empty() && empty.hide_buttons.is_empty(), "{}", scheme.id);
            v.contracts = &lines;
            let (out, _) = frame(&v, vec2(420.0, 400.0), None);
            let texts = &out.contract_texts;
            assert_eq!(texts[0], contracts_head(lines.len(), 10), "{}", scheme.id);
            assert!(texts[0].ends_with(&format!("{}/10", lines.len())), "{}: {}", scheme.id, texts[0]);
            // The most the player set stands behind the count.
            v.contracts_max = 12;
            let (more, _) = frame(&v, vec2(420.0, 400.0), None);
            assert!(more.contract_texts[0].ends_with(&format!("{}/12", lines.len())), "{}", scheme.id);
            v.contracts_max = 10;
            assert!(texts.contains(&lines[0].text.render()) && texts.contains(&lines[2].text.render()));
            assert!(texts.contains(&t("ov_a_unklar")), "{}: the reason for the grey one", scheme.id);
            assert!(!texts.contains(&"Nicht gezeigt".to_string()), "{}: a grey one shows no objectives", scheme.id);
            let shown_goals = texts.iter().filter(|x| x.starts_with("Ziel ")).count();
            assert_eq!(shown_goals, OBJECTIVES_MAX, "{}", scheme.id);
            assert!(texts.contains(&tf("ov_ziele_mehr", &[("0", 2.into())])), "{}: the rest is counted", scheme.id);
            assert_eq!(out.hide_buttons.len(), 3, "{}", scheme.id);
        }
    }

    /// Nothing sticks out at the narrowest bar: the hide buttons lie inside the
    /// window and the long line wraps instead of pushing them away.
    #[test]
    fn the_hide_buttons_stay_inside_a_narrow_window() {
        let rows = rows();
        let status = Message::Plain("x".into());
        let lines = contract_lines();
        for scheme in theme::schemes() {
            let mut v = view(scheme, &rows, &status, Shown::Open);
            v.contracts = &lines;
            let (first, _) = frame(&v, vec2(900.0, 500.0), None);
            for width in [first.min_width.max(260.0), 330.0, 420.0] {
                let (out, _) = frame(&v, vec2(width, 500.0), None);
                for r in &out.hide_buttons {
                    assert!(r.min.x >= 0.0 && r.max.x <= width + 0.5, "{} {width}: {r:?}", scheme.id);
                }
                // Each button stands right of the text of its line: no two on top of each other,
                // and all at the same place from the right edge, whatever the text is.
                assert!(out.hide_buttons.windows(2).all(|w| w[0].max.y <= w[1].min.y + 0.5), "{} {width}", scheme.id);
                assert!(
                    out.hide_buttons.windows(2).all(|w| (w[0].max.x - w[1].max.x).abs() < 0.5),
                    "{} {width}: the buttons do not stand in a column",
                    scheme.id
                );
            }
        }
    }

    /// Every size of the text, both languages, every scheme, the narrowest
    /// window that size allows: nothing of the bar sticks out, and the rows
    /// below keep their place even with four contracts of six objectives each.
    #[test]
    fn the_bar_of_contracts_holds_at_every_size_and_language() {
        let _language = crate::language_lock();
        let rows = {
            let mut rows = rows();
            for n in 0..6 {
                rows.push(
                    Row::Found {
                        name: format!("Bauplan {n}"),
                        kind: "Kühler".into(),
                        code: "S1 · A".into(),
                        time: "12:00:00".into(),
                        caught_up: false,
                    },
                    20,
                );
            }
            rows
        };
        let status = Message::Plain("x".into());
        let goals: Vec<String> = (1..=6).map(|n| format!("Ziel {n} mit einem etwas längeren Wortlaut")).collect();
        let goals: Vec<&str> = goals.iter().map(String::as_str).collect();
        let lines: Vec<ContractLine> = (1..=4)
            .map(|n| {
                line(
                    &format!("Auftrag {n}"),
                    &format!("Auftrag angenommen: Auftrag {n}  →  Baupläne 1/3 · dir fehlt: Name"),
                    &goals,
                    false,
                )
            })
            .collect();
        for choice in [versekit_core::language::Choice::De, versekit_core::language::Choice::En] {
            versekit_core::language::set_thread_language(choice);
            for scheme in theme::schemes() {
                for points in versekit_core::font_size::POINTS_RANGE.0..=versekit_core::font_size::POINTS_RANGE.1 {
                    let (width, height) = versekit_core::font_size::min_window(points);
                    let mut v = view(scheme, &rows, &status, Shown::Open);
                    v.contracts = &lines;
                    v.hidden = Some("Ein Auftrag mit einem ziemlich langen Titel, der umbrechen muss");
                    let ctx = egui::Context::default();
                    crate::install(&ctx);
                    crate::apply(&ctx, scheme);
                    crate::apply_size(&ctx, points);
                    let mut out = Output::default();
                    for _ in 0..3 {
                        let input = egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, height.max(254.0)))),
                            ..Default::default()
                        };
                        let mut output = ctx.run_ui(input, |ui| out = show(ui, &v));
                        output.textures_delta.clear();
                    }
                    let what = format!("{choice:?} {} Stufe {points} {width}x{height}", scheme.id);
                    for r in &out.hide_buttons {
                        assert!(r.min.x >= 0.0 && r.max.x <= width + 0.5, "{what}: {r:?}");
                    }
                    assert!(out.hide_buttons.windows(2).all(|w| (w[0].max.x - w[1].max.x).abs() < 0.5), "{what}");
                    // The way back stands in the same column as the hide buttons and inside the window.
                    let back = out.unhide_button.expect("the line with the way back is drawn");
                    assert!(back.min.x >= 0.0 && back.max.x <= width + 0.5, "{what}: {back:?}");
                    assert!(out.hide_buttons.iter().all(|r| (r.max.x - back.max.x).abs() < 0.5), "{what}");
                    assert!(out.visible_rows >= 1, "{what}: the rows are pushed out of the window");
                }
            }
        }
        versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
    }

    /// The line with the way back stands there while a contract was just hidden, also when nothing
    /// else runs (then it is the only line), and says its title; a click on its button reports it.
    #[test]
    fn the_line_with_the_way_back_shows_the_hidden_contract_and_reports_the_click() {
        let rows = rows();
        let status = Message::Plain("x".into());
        let scheme = theme::by_id("original").expect("original");
        let mut v = view(scheme, &rows, &status, Shown::Open);
        let size = vec2(420.0, 500.0);
        let (none, _) = frame(&v, size, None);
        assert!(none.unhide_button.is_none() && none.contract_texts.is_empty(), "nothing hidden: no line");
        v.hidden = Some("Zwei");
        let (first, _) = frame(&v, size, None);
        let said = tf("ov_a_ausgeblendet", &[("0", "Zwei".into())]);
        assert_eq!(first.contract_texts, [said], "only the line with the way back, no header of an empty bar");
        let target = first.unhide_button.expect("the button of the way back").center();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let mut taken_back = false;
        let press = |pressed| egui::Event::PointerButton {
            pos: target,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let steps = [vec![], vec![egui::Event::PointerMoved(target)], vec![press(true)], vec![press(false)], vec![]];
        for events in steps {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| taken_back |= show(ui, &v).unhide);
            output.textures_delta.clear();
        }
        assert!(taken_back, "the click on the button reports the way back");
        // Beside running contracts the line stands above their header.
        let lines = contract_lines();
        v.contracts = &lines;
        let (both, _) = frame(&v, size, None);
        assert_eq!(both.contract_texts[1], contracts_head(lines.len(), 10));
        assert_eq!(both.hide_buttons.len(), 3);
    }

    /// An update that has waited a day or more shows in yellow with its days, with the game closed too, and its button
    /// installs it now; while the game runs the button stays grey and a click on it reports nothing. Below a day the
    /// line stays as it was: only in the game, without a button.
    #[test]
    fn an_update_waiting_for_days_shows_with_its_button_which_is_grey_in_the_game() {
        let rows = rows();
        let status = Message::Plain("x".into());
        let scheme = theme::by_id("original").expect("original");
        let size = vec2(420.0, 500.0);
        let click = |v: &View, target: Pos2| -> bool {
            let ctx = egui::Context::default();
            crate::install(&ctx);
            let press = |pressed| egui::Event::PointerButton {
                pos: target,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            let mut asked = false;
            let steps =
                [vec![], vec![egui::Event::PointerMoved(target)], vec![press(true)], vec![press(false)], vec![]];
            for events in steps {
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                    events,
                    ..Default::default()
                };
                let mut output = ctx.run_ui(input, |ui| asked |= show(ui, v).update_now);
                output.textures_delta.clear();
            }
            asked
        };
        for choice in [language::Choice::De, language::Choice::En] {
            language::set_thread_language(choice);
            let mut v = view(scheme, &rows, &status, Shown::Open);
            v.update = Some("5.3.0");
            let waiting =
                tf("ov_update_wartet", &[("0", "5.3.0".into()), ("1", tf("zeit_tage", &[("0", 3.into())]).into())]);
            // Below a day: the old line in the game, nothing without it, no button.
            let (out, painted) = frame(&v, size, None);
            assert!(out.update_button.is_none() && !painted.contains(&waiting), "{choice:?}");
            assert!(painted.contains(&tf("ov_update", &[("0", "5.3.0".into())])), "{choice:?}");
            // Three days, game closed: the yellow line with a usable button, and a click asks for the update.
            v.update_days = Some(3);
            v.game = false;
            let (out, painted) = frame(&v, size, None);
            assert!(painted.contains(&waiting), "{choice:?}: {painted:?}");
            assert!(!painted.contains(&tf("ov_update", &[("0", "5.3.0".into())])), "{choice:?}: one line, not two");
            let (button, usable) = out.update_button.expect("the button of the line");
            assert!(usable && button.min.x >= 0.0 && button.max.x <= size.x, "{choice:?}: {button:?}");
            assert!(click(&v, button.center()), "{choice:?}: the click asks for the update");
            // In the game: the line stays, the button is grey and a click asks for nothing.
            v.game = true;
            let (out, painted) = frame(&v, size, None);
            assert!(painted.contains(&waiting), "{choice:?}");
            let (button, usable) = out.update_button.expect("the button of the line");
            assert!(!usable, "{choice:?}: grey in the game");
            assert!(!click(&v, button.center()), "{choice:?}: no update in the game");
        }
        language::set_thread_language(language::Choice::De);
    }

    /// The rocket stands in the bar only while a way to start the game is known, inside the window at
    /// the narrowest width the bar reports, and a click on it reports the start.
    #[test]
    fn the_rocket_stands_in_the_bar_only_while_a_way_to_start_is_known() {
        let rows = rows();
        let status = Message::Plain("x".into());
        for scheme in theme::schemes() {
            let mut v = view(scheme, &rows, &status, Shown::Open);
            let (without, _) = frame(&v, vec2(420.0, 300.0), None);
            assert!(!without.buttons.iter().any(|(a, _)| *a == Action::Launch), "{}: no way, no rocket", scheme.id);
            v.launcher = true;
            let (with, _) = frame(&v, vec2(420.0, 300.0), None);
            let rocket: Vec<Rect> =
                with.buttons.iter().filter(|(a, _)| *a == Action::Launch).map(|(_, r)| *r).collect();
            assert_eq!(rocket.len(), 1, "{}", scheme.id);
            assert!(rocket[0].min.x >= 0.0 && rocket[0].max.x <= 420.0, "{}: {:?}", scheme.id, rocket[0]);
            assert!(with.min_width > without.min_width, "{}: the narrowest bar takes the rocket in", scheme.id);
            // At the narrowest width every button still stands inside the window.
            let (narrow, _) = frame(&v, vec2(with.min_width, 300.0), None);
            assert!(
                narrow.buttons.iter().all(|(_, r)| r.min.x >= -0.5 && r.max.x <= with.min_width + 0.5),
                "{}",
                scheme.id
            );
        }
        // The click.
        let scheme = theme::by_id("original").expect("original");
        let mut v = view(scheme, &rows, &status, Shown::Open);
        v.launcher = true;
        let size = vec2(420.0, 300.0);
        let (first, _) = frame(&v, size, None);
        let target = first.buttons.iter().find(|(a, _)| *a == Action::Launch).expect("rocket").1.center();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let mut actions = Vec::new();
        let press = |pressed| egui::Event::PointerButton {
            pos: target,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let steps = [vec![], vec![egui::Event::PointerMoved(target)], vec![press(true)], vec![press(false)], vec![]];
        for events in steps {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| actions.extend(show(ui, &v).actions));
            output.textures_delta.clear();
        }
        assert_eq!(actions, vec![Action::Launch]);
    }

    /// A click on the button of a line reports the contract of that line.
    #[test]
    fn a_click_on_a_hide_button_hides_that_contract() {
        let rows = rows();
        let status = Message::Plain("x".into());
        let lines = contract_lines();
        let scheme = theme::by_id("original").expect("original");
        let mut v = view(scheme, &rows, &status, Shown::Open);
        v.contracts = &lines;
        // Tall enough that the third line stands inside the window at the text sizes of the interface.
        let size = vec2(420.0, 800.0);
        let (first, _) = frame(&v, size, None);
        let target = first.hide_buttons[2].center();
        assert!(target.y < size.y, "the button of the third line lies outside the window: {target:?}");
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let mut hidden = Vec::new();
        let press = |pressed| egui::Event::PointerButton {
            pos: target,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let steps = [vec![], vec![egui::Event::PointerMoved(target)], vec![press(true)], vec![press(false)], vec![]];
        for events in steps {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| hidden.extend(show(ui, &v).hide));
            output.textures_delta.clear();
        }
        assert_eq!(hidden, vec!["Drei".to_string()]);
    }

    /// The highest opacity the painted meshes give the point `p` (vertex colors only).
    fn cover_at(primitives: &[egui::ClippedPrimitive], p: Pos2) -> u8 {
        let mut most = 0u8;
        for clipped in primitives {
            let egui::epaint::Primitive::Mesh(mesh) = &clipped.primitive else { continue };
            if !clipped.clip_rect.contains(p) {
                continue;
            }
            for triangle in mesh.indices.as_chunks::<3>().0 {
                let [a, b, c] = triangle.map(|i| mesh.vertices[i as usize]);
                let d = (b.pos.y - c.pos.y) * (a.pos.x - c.pos.x) + (c.pos.x - b.pos.x) * (a.pos.y - c.pos.y);
                if d.abs() < 1e-6 {
                    continue;
                }
                let wa = ((b.pos.y - c.pos.y) * (p.x - c.pos.x) + (c.pos.x - b.pos.x) * (p.y - c.pos.y)) / d;
                let wb = ((c.pos.y - a.pos.y) * (p.x - c.pos.x) + (a.pos.x - c.pos.x) * (p.y - c.pos.y)) / d;
                let wc = 1.0 - wa - wb;
                if wa < 0.0 || wb < 0.0 || wc < 0.0 {
                    continue;
                }
                let alpha = wa * f32::from(a.color.a()) + wb * f32::from(b.color.a()) + wc * f32::from(c.color.a());
                most = most.max(alpha.round() as u8);
            }
        }
        most
    }

    /// Beside the rounded box nothing is painted: open with the bar on top or below and folded, the pixel
    /// next to each window corner stays transparent, in every rounded scheme. Square schemes cover their corners.
    #[test]
    fn the_corners_beside_the_rounded_box_stay_transparent() {
        let rows = rows();
        let status = Message::Plain("2 Baupläne".into());
        let mut covered = Vec::new();
        for scheme in theme::schemes() {
            for (shown, bar, size) in [
                (Shown::Open, BarSide::Top, vec2(420.0, 300.0)),
                (Shown::Open, BarSide::Bottom, vec2(420.0, 300.0)),
                (Shown::Bar, BarSide::Top, vec2(420.0, BAR_HEIGHT)),
                (Shown::Bar, BarSide::Bottom, vec2(420.0, BAR_HEIGHT)),
            ] {
                let mut v = view(scheme, &rows, &status, shown);
                v.bar = bar;
                let ctx = egui::Context::default();
                crate::install(&ctx);
                crate::apply(&ctx, scheme);
                let mut last = Vec::new();
                for _ in 0..2 {
                    let input = egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                        ..Default::default()
                    };
                    let mut output = ctx.run_ui(input, |ui| {
                        show(ui, &v);
                    });
                    output.textures_delta.clear();
                    last = ctx.tessellate(output.shapes, output.pixels_per_point);
                }
                let corners = [
                    pos2(1.5, 1.5),
                    pos2(size.x - 1.5, 1.5),
                    pos2(1.5, size.y - 1.5),
                    pos2(size.x - 1.5, size.y - 1.5),
                ];
                let cover: Vec<u8> = corners.iter().map(|&p| cover_at(&last, p)).collect();
                let all_covered = cover.iter().all(|&a| a > 0);
                if scheme.square {
                    assert!(all_covered, "{} {shown:?} {bar:?}: square box, corners {cover:?}", scheme.id);
                } else if cover.iter().any(|&a| a > 0) {
                    covered.push(format!("{} {shown:?} {bar:?}: corners nw ne sw se {cover:?}", scheme.id));
                }
            }
        }
        assert!(covered.is_empty(), "painted beside the rounded box:\n{}", covered.join("\n"));
    }
}
