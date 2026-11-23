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

//! The digits the player taught, on the tab Signatur: the digit patterns from 0 to 9, each digit folded on its own
//! with the suspect patterns first and edged in yellow, the pictures taught as a grid of cards, newest first, with a
//! search by number, removing the duplicates and forgetting all of them. Each card is deleted by the button that
//! shows when the pointer rests on it.
//!
//! The overlay sees a change by the time of the files and reads the digits again at its next beat.

use egui::{Align2, Color32, Rect, RichText, Role, Sense, Stroke, StrokeKind, TextureOptions, Ui, Vec2, WidgetInfo, pos2, vec2};
use versekit_core::language::{Arg, t, tf};
use versekit_core::signature_scan::own::{self, FileError, Problem, TaughtDigit};
use versekit_core::signature_scan::{self, NORM_H, NORM_W, OwnFiles, Pattern, Templates};
use versekit_core::signature_window;
use versekit_core::theme::{IconTint, Scheme};

use super::ScannerState;
use crate::color;
use crate::widgets::{self, ButtonKind};

/// How large a digit pattern is drawn: its grid points times this.
const DIGIT_ZOOM: f32 = 1.5;
/// The largest a picture taught is drawn in its card, in points.
const PICTURE_MOST: (f32, f32) = (150.0, 44.0);
/// The room round the content of a card.
const TILE_PAD: f32 = 4.0;
/// The height of the line of a card that names its number.
const CAPTION_HEIGHT: f32 = 18.0;
/// The side of the button that deletes a card.
const DELETE_SIDE: f32 = 22.0;
/// How high the grid of pictures grows before it scrolls.
const PICTURES_HEIGHT: f32 = 300.0;

/// A picture taught: its file, the number it was taught as (digits only and as shown) and its texture (`None`: not
/// readable).
struct Picture {
    name: String,
    number: String,
    shown: String,
    texture: Option<egui::TextureHandle>,
}

/// A digit pattern taught with its texture and what is suspect about it.
struct Digit {
    taught: TaughtDigit,
    texture: egui::TextureHandle,
    problem: Option<Problem>,
}

/// What the section shows, read from the files when it opens and after every change.
#[derive(Default)]
pub struct OwnView {
    /// Read since the last change.
    fresh: bool,
    /// The pictures taught, the newest first.
    pictures: Vec<Picture>,
    /// The patterns by digit, the suspect ones first within a digit.
    digits: Vec<Digit>,
    /// How many pictures have a newer one of the same number.
    duplicates: usize,
    /// The number the grid is narrowed to.
    search: String,
    /// The question before forgetting everything is open.
    asking: bool,
    /// What the last change came to, and whether it worked.
    said: Option<(String, bool)>,
}

impl OwnView {
    /// The files changed elsewhere (a number was taught): read them again when shown.
    pub fn stale(&mut self) {
        self.fresh = false;
    }

    /// Deletes the picture `name` with the digits only it gave.
    pub(super) fn forget_picture(&mut self, files: &OwnFiles, folder: &str, name: &str) {
        self.said = Some(outcome(own::forget_sample(files, folder, name)));
        self.fresh = false;
    }

    /// Deletes one digit pattern in every file.
    pub(super) fn forget_pattern(&mut self, files: &OwnFiles, digit: char, pattern: &Pattern) {
        self.said = Some(outcome(own::forget_digit(files, digit, pattern)));
        self.fresh = false;
    }

    /// Deletes every picture but the newest of each number, with the digits only they gave.
    pub(super) fn forget_duplicates(&mut self, files: &OwnFiles, folder: &str) {
        self.said = Some(match own::forget_duplicates(files, folder) {
            Ok((_, images)) => (tf("scan_doppelte_weg", &[("0", Arg::from(images))]), true),
            Err(FileError::Read) => (t("scan_eigene_fehler_lesen"), false),
            Err(FileError::Write) => (t("scan_eigene_fehler_schreiben"), false),
        });
        self.fresh = false;
    }

    /// Deletes every digit taught; the pictures stay.
    pub(super) fn forget_everything(&mut self, files: &OwnFiles) {
        self.said = Some(match own::forget_all(files) {
            Ok(()) => (t("scan_eigene_vergessen_ok"), true),
            Err(_) => (t("scan_eigene_fehler_schreiben"), false),
        });
        self.fresh = false;
    }

}

#[cfg(test)]
impl OwnView {
    /// The patterns in the order shown: (digit, suspect).
    pub(super) fn digit_order(&self) -> Vec<(char, bool)> {
        self.digits.iter().map(|d| (d.taught.digit, d.problem.is_some())).collect()
    }

    /// Types `text` into the search of the pictures.
    pub(super) fn search_for(&mut self, text: &str) {
        self.search = text.to_string();
    }

    /// The numbers of the pictures in the order shown.
    pub(super) fn picture_numbers(&self) -> Vec<String> {
        self.pictures.iter().map(|p| p.number.clone()).collect()
    }
}

/// How many pictures and how many patterns the player has taught; the files are read when they changed.
pub fn counts(ui: &Ui, state: &mut ScannerState) -> (usize, usize) {
    let Some((files, folder, _)) = state.files() else { return (0, 0) };
    if !state.own.fresh {
        refresh(ui, &mut state.own, &files, &folder);
    }
    (state.own.pictures.len(), state.own.digits.len())
}

/// A digit pattern as a texture: set points white on black.
fn pattern_texture(ui: &Ui, name: &str, pattern: &Pattern) -> egui::TextureHandle {
    let data: Vec<u8> = pattern.iter().map(|&v| if v != 0 { 235 } else { 18 }).collect();
    let image = egui::ColorImage::from_gray([NORM_W, NORM_H], &data);
    ui.ctx().load_texture(name, image, TextureOptions::NEAREST)
}

/// Reads the pictures and digit patterns again.
pub(super) fn refresh(ui: &Ui, view: &mut OwnView, files: &OwnFiles, folder: &str) {
    let mut names = signature_scan::samples(folder);
    names.reverse();
    view.pictures = names
        .into_iter()
        .map(|name| {
            let path = format!("{}/{name}", folder.trim_end_matches(['/', '\\']));
            let texture = std::fs::read(&path).ok().and_then(|bytes| own::gray_of_png(&bytes)).map(|gray| {
                let image = egui::ColorImage::from_gray([gray.w, gray.h], &gray.data);
                ui.ctx().load_texture(format!("signatur-eigen-{name}"), image, TextureOptions::NEAREST)
            });
            let number = own::sample_number(&name);
            let shown = number.parse::<u64>().map(signature_window::with_thousands).unwrap_or_else(|_| number.clone());
            Picture { name, number, shown, texture }
        })
        .collect();
    let mut seen: Vec<&str> = Vec::new();
    view.duplicates = 0;
    for picture in &view.pictures {
        if seen.contains(&picture.number.as_str()) {
            view.duplicates += 1;
        } else {
            seen.push(&picture.number);
        }
    }
    let shipped: Templates = signature_scan::templates(signature_scan::SHIPPED_TEMPLATES, "");
    let mut digits: Vec<Digit> = own::taught_digits(files)
        .into_iter()
        .enumerate()
        .map(|(at, taught)| {
            let texture = pattern_texture(ui, &format!("signatur-ziffer-{at}"), &taught.pattern);
            let problem = own::problem(taught.digit, &taught.pattern, &shipped, false);
            Digit { taught, texture, problem }
        })
        .collect();
    // Within a digit the suspect ones come first; the order of teaching stays otherwise.
    digits.sort_by_key(|d| (d.taught.digit, d.problem.is_none()));
    view.digits = digits;
    view.fresh = true;
}

/// What a change came to, for the line below.
fn outcome(result: Result<usize, FileError>) -> (String, bool) {
    match result {
        Ok(gone) => (tf("scan_eigene_geloescht", &[("0", Arg::from(gone))]), true),
        Err(FileError::Read) => (t("scan_eigene_fehler_lesen"), false),
        Err(FileError::Write) => (t("scan_eigene_fehler_schreiben"), false),
    }
}

/// The text that says why a pattern is suspect.
fn problem_text(problem: Problem) -> String {
    t(match problem {
        Problem::TooWide => "scan_verdacht_breit",
        Problem::TwoParts => "scan_verdacht_teile",
        Problem::OtherDigit => "scan_verdacht_andere",
    })
}

/// The section "Own digits": the digits by digit, the pictures as cards, and forgetting. Read when shown.
pub fn show(ui: &mut Ui, scheme: &Scheme, state: &mut ScannerState) {
    let Some((files, folder, _)) = state.files() else { return };
    let view = &mut state.own;
    if !view.fresh {
        refresh(ui, view, &files, &folder);
    }
    widgets::hint(ui, scheme, &t("scan_eigene_h"));
    if let Some((line, worked)) = &view.said {
        let tone = if *worked { scheme.accent } else { scheme.gold };
        ui.label(RichText::new(line).color(color(tone)));
    }
    if view.pictures.is_empty() && view.digits.is_empty() {
        widgets::hint(ui, scheme, &t("scan_eigene_leer"));
        return;
    }
    if let Some((digit, pattern)) = digits(ui, scheme, view) {
        view.forget_pattern(&files, digit, &pattern);
    }
    if let Some(name) = pictures(ui, scheme, view, &files, &folder) {
        view.forget_picture(&files, &folder, &name);
    }
    ui.add_space(6.0);
    if ui.add(widgets::Button::new(&t("scan_eigene_vergessen"), ButtonKind::Danger, scheme).icon("trash-2")).clicked() {
        view.asking = true;
    }
    if view.asking
        && let Some(yes) = ask_forget(ui, scheme)
    {
        view.asking = false;
        if yes {
            view.forget_everything(&files);
        }
    }
    // A change is shown at once: the lists are read again in the next frame.
    if !view.fresh {
        ui.ctx().request_repaint();
    }
}

/// What a card shows: the picture (`content` its size), a caption, a coloured edge, the name of its delete button
/// and a tip.
#[derive(Clone, Copy)]
struct Card<'a> {
    texture: Option<&'a egui::TextureHandle>,
    content: Vec2,
    caption: Option<&'a str>,
    edge: Option<Color32>,
    meaning: &'a str,
    tip: Option<&'a str>,
}

/// A card: the picture, below it a caption, and the button that deletes it, which shows while the pointer rests on
/// the card or the button has the focus. `edge` colours the frame (suspect). Gives whether the button was clicked.
fn tile(ui: &mut Ui, scheme: &Scheme, card: &Card) -> bool {
    let Card { texture, content, caption, edge, meaning, tip } = *card;
    let caption_height = if caption.is_some() { CAPTION_HEIGHT } else { 0.0 };
    let size = vec2(content.x + 2.0 * TILE_PAD, content.y + caption_height + 2.0 * TILE_PAD);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let delete_rect = Rect::from_min_size(pos2(rect.right() - DELETE_SIDE - 2.0, rect.top() + 2.0), Vec2::splat(DELETE_SIDE));
    let delete = ui.interact(delete_rect, response.id.with("loeschen"), Sense::click());
    delete.widget_info(|| WidgetInfo::labeled(Role::Button, true, meaning));
    let hot = ui.rect_contains_pointer(rect) || delete.has_focus();
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let stroke = match edge {
            Some(edge) => Stroke::new(2.0, edge),
            None => Stroke::new(1.0, color(scheme.line)),
        };
        painter.rect(rect, 4.0, color(scheme.field), stroke, StrokeKind::Inside);
        if let Some(texture) = texture {
            let at = Rect::from_min_size(pos2(rect.center().x - content.x / 2.0, rect.top() + TILE_PAD), content);
            egui::Image::new((texture.id(), content)).paint_at(ui, at);
        }
        if let Some(caption) = caption {
            painter.text(
                pos2(rect.center().x, rect.bottom() - TILE_PAD - CAPTION_HEIGHT / 2.0),
                Align2::CENTER_CENTER,
                caption,
                egui::TextStyle::Small.resolve(ui.style()),
                color(scheme.fg),
            );
        }
        if hot {
            let hovered = delete.hovered();
            let fill = if hovered { scheme.danger_fill } else { scheme.surface };
            painter.rect(delete_rect, 4.0, color(fill), Stroke::new(1.0, color(scheme.red)), StrokeKind::Inside);
            let tint = if hovered { IconTint::Hover } else { IconTint::Normal };
            if let Some(image) = crate::icons::themed("trash-2", scheme, tint, 14.0) {
                image.paint_at(ui, Rect::from_center_size(delete_rect.center(), Vec2::splat(14.0)));
            }
        }
        if delete.has_focus() {
            widgets::focus_ring(ui, &delete, scheme, 4.0.into());
        }
    }
    if let Some(tip) = tip {
        response.on_hover_text(tip);
    }
    // The button covers most of a small card, so the reason a pattern is suspect goes into its tip as well.
    let tip_of_button = tip.map_or_else(|| meaning.to_string(), |tip| format!("{meaning}\n{tip}"));
    delete.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip_of_button).clicked()
}

/// The digit patterns, each digit folded on its own with the number of its patterns (and of the suspect ones) in
/// its head. Gives the pattern to delete.
fn digits(ui: &mut Ui, scheme: &Scheme, view: &OwnView) -> Option<(char, Pattern)> {
    if view.digits.is_empty() {
        return None;
    }
    ui.add_space(6.0);
    ui.label(RichText::new(t("scan_eigene_ziffern")).strong());
    let mut chosen = None;
    for digit in '0'..='9' {
        let of_digit: Vec<&Digit> = view.digits.iter().filter(|d| d.taught.digit == digit).collect();
        if of_digit.is_empty() {
            continue;
        }
        let suspect = of_digit.iter().filter(|d| d.problem.is_some()).count();
        let (count, mark) = (Arg::from(of_digit.len()), digit.to_string());
        let head = if suspect == 0 {
            tf("scan_ziffer_kopf", &[("0", Arg::from(mark.as_str())), ("1", count)])
        } else {
            tf("scan_ziffer_kopf_verdacht", &[("0", Arg::from(mark.as_str())), ("1", count), ("2", Arg::from(suspect))])
        };
        widgets::collapsing(&head).id_salt(("signatur-ziffer", digit)).show(ui, |ui| {
            ui.horizontal_wrapped(|row| {
                for entry in &of_digit {
                    let meaning = tf("scan_eigene_ziffer_loeschen", &[("0", Arg::from(mark.as_str()))]);
                    let edge = entry.problem.map(|_| color(scheme.yellow));
                    let tip = entry.problem.map(problem_text);
                    let size = vec2(NORM_W as f32, NORM_H as f32) * DIGIT_ZOOM;
                    let card = Card {
                        texture: Some(&entry.texture),
                        content: size,
                        caption: None,
                        edge,
                        meaning: &meaning,
                        tip: tip.as_deref(),
                    };
                    if tile(row, scheme, &card) {
                        chosen = Some((digit, entry.taught.pattern));
                    }
                }
            });
        });
    }
    chosen
}

/// The pictures taught as a grid of cards, newest first, narrowed by the number typed in the search; the button that
/// removes the duplicates. Gives the picture to delete.
fn pictures(ui: &mut Ui, scheme: &Scheme, view: &mut OwnView, files: &OwnFiles, folder: &str) -> Option<String> {
    if view.pictures.is_empty() {
        return None;
    }
    ui.add_space(8.0);
    ui.label(RichText::new(tf("scan_bilder_kopf", &[("0", Arg::from(view.pictures.len()))])).strong());
    ui.horizontal_wrapped(|row| {
        row.add(widgets::text_field(&mut view.search, &t("scan_bilder_suche"), scheme).desired_width(150.0));
        if view.duplicates > 0
            && row.add(widgets::Button::new(&t("scan_doppelte_entfernen"), ButtonKind::Normal, scheme)).clicked()
        {
            view.forget_duplicates(files, folder);
        }
    });
    if view.duplicates > 0 {
        widgets::hint(ui, scheme, &tf("scan_doppelte_h", &[("0", Arg::from(view.duplicates))]));
    }
    let wanted: String = view.search.chars().filter(char::is_ascii_digit).collect();
    let shown: Vec<&Picture> = view.pictures.iter().filter(|p| p.number.contains(&wanted)).collect();
    if shown.is_empty() {
        widgets::hint(ui, scheme, &t("scan_bilder_keine"));
        return None;
    }
    let mut chosen = None;
    let area = egui::ScrollArea::vertical().id_salt("signatur-eigene-bilder").max_height(PICTURES_HEIGHT);
    widgets::scroll_area(ui, scheme, area, |ui| {
        ui.horizontal_wrapped(|row| {
            for picture in shown {
                let size = picture.texture.as_ref().map_or(vec2(PICTURE_MOST.0, 20.0), |texture| {
                    let size = texture.size_vec2();
                    size * (PICTURE_MOST.0 / size.x).min(PICTURE_MOST.1 / size.y).clamp(0.5, 2.0)
                });
                let meaning = tf("scan_eigene_bild_loeschen", &[("0", Arg::from(picture.shown.as_str()))]);
                let card = Card {
                    texture: picture.texture.as_ref(),
                    content: size,
                    caption: Some(&picture.shown),
                    edge: None,
                    meaning: &meaning,
                    tip: None,
                };
                if tile(row, scheme, &card) {
                    chosen = Some(picture.name.clone());
                }
            }
        });
    });
    chosen
}

/// The question before forgetting everything: `Some(yes)` once it is answered or closed, `None` while it stands.
fn ask_forget(ui: &Ui, scheme: &Scheme) -> Option<bool> {
    let (answer, close) = widgets::dialog(ui.ctx(), scheme, "signatur-eigene-vergessen", |ui| {
        widgets::heading(ui, scheme, &t("scan_eigene_vergessen"));
        ui.label(t("scan_eigene_vergessen_frage"));
        ui.add_space(8.0);
        ui.horizontal(|row| {
            if row.add(widgets::Button::new(&t("scan_eigene_vergessen_ja"), ButtonKind::Danger, scheme)).clicked() {
                return Some(true);
            }
            if row.add(widgets::Button::new(&t("e_abbrechen"), ButtonKind::Normal, scheme)).clicked() {
                return Some(false);
            }
            None
        })
        .inner
    });
    match (answer, close) {
        (Some(yes), _) => Some(yes),
        (None, true) => Some(false),
        (None, false) => None,
    }
}
