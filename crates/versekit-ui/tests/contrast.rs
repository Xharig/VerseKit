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

//! Measures the contrast of every text and every visible mark the widgets
//! draw, in every scheme (WCAG 2.x: text 4.5, borders and marks 3).

use versekit_core::theme::{self, Color, KRT_ORANGE, Scheme};
use versekit_ui::widgets::{ButtonKind, RankTone, button_colors, rank_bar, rank_edge, section_title_color};

/// (what, foreground, background, needed ratio)
fn pairs(s: &Scheme) -> Vec<(String, Color, Color, f64)> {
    let text = 4.5;
    let mark = 3.0;
    let mut all = vec![
        ("Text auf Grund".to_string(), s.fg, s.bg, text),
        ("Text auf Kasten".into(), s.fg, s.surface, text),
        ("Text im Feld".into(), s.fg, s.field, text),
        // The assistant draws on the field colour (its list frame).
        ("Gedämpft im Feld".into(), s.sub, s.field, text),
        ("Fehler im Feld".into(), s.red_text(), s.field, text),
        ("Gelb im Feld".into(), s.yellow, s.field, text),
        ("Akzentschrift im Feld".into(), s.accent, s.field, text),
        ("Text in Leiste/Tabellenstreifen".into(), s.fg, s.bar, text),
        ("Text in gewählter Zeile".into(), s.fg, s.accent_dark, text),
        ("Text in überfahrener Zeile".into(), s.fg, s.hover, text),
        ("Gedämpft auf Grund".into(), s.sub, s.bg, text),
        ("Gedämpft auf Kasten".into(), s.sub, s.surface, text),
        ("Gedämpft in Leiste".into(), s.sub, s.bar, text),
        ("Gold auf Grund".into(), s.gold, s.bg, text),
        ("Gold auf Kasten".into(), s.gold, s.surface, text),
        ("Gold in Leiste".into(), s.gold, s.bar, text),
        ("Hinweis im leeren Feld".into(), s.sub, s.field, text),
        ("Akzentschrift auf Grund (Reiter)".into(), s.accent, s.bg, text),
        ("Akzentschrift auf Kasten".into(), s.accent, s.surface, text),
        ("Hervorhebung in Vorschlagsliste".into(), s.accent, s.field, text),
        ("Fehler auf Kasten".into(), s.red_text(), s.surface, text),
        ("Fehler auf Leiste".into(), s.red_text(), s.bar, text),
        ("Fehler unter Feld".into(), s.red_text(), s.bg, text),
        // The loud red: text of the overlay (the Executive Hangar line) on its ground, a mark on the boxes.
        ("Rot als Schrift im Overlay (Grund)".into(), s.red, s.bg, text),
        ("Rote Marke auf Kasten".into(), s.red, s.surface, mark),
        ("Rote Marke im Feld".into(), s.red, s.field, mark),
        ("Rote Marke in Leiste".into(), s.red, s.bar, mark),
        ("Gelb auf Kasten".into(), s.yellow, s.surface, text),
        ("Blau auf Kasten".into(), s.blue, s.surface, text),
        ("Blau auf Grund".into(), s.blue, s.bg, text),
        ("Akzent-Markierung auf Grund".into(), s.accent, s.bg, mark),
        ("Akzent-Markierung auf Kasten".into(), s.accent, s.surface, mark),
        ("Rand überfahrener Zeile (Akzent auf Feld)".into(), s.accent, s.field, mark),
        ("Akzent-Balken in gewählter Zeile".into(), s.accent, s.accent_dark, mark),
        ("Rangbalken Rand Akzent auf Grund".into(), rank_edge(s, RankTone::Accent), s.bg, mark),
        ("Rangbalken Rand Rot auf Grund".into(), rank_edge(s, RankTone::Red), s.bg, mark),
        ("Rangbalken Rand Gold auf Grund".into(), rank_edge(s, RankTone::Gold), s.bg, mark),
        ("Schrift auf Rangbalken Akzent".into(), s.fg, rank_bar(s, RankTone::Accent), text),
        ("Schrift auf Rangbalken Rot".into(), s.fg, rank_bar(s, RankTone::Red), text),
        ("Schrift auf Rangbalken Gold".into(), s.fg, rank_bar(s, RankTone::Gold), text),
        ("Fehlerfarbe in Tabellenstreifen".into(), s.red_pale, s.bar, text),
        ("Text in Suchzeile".into(), s.fg, s.search_ground(), text),
        ("Hinweis in leerer Suchzeile".into(), s.sub, s.search_ground(), text),
        ("Akzent-Lupe in Suchzeile".into(), s.accent, s.search_ground(), mark),
        ("Rand der Suchzeile auf Grund".into(), s.search_border(), s.bg, mark),
        ("Rand der Suchzeile auf Kasten".into(), s.search_border(), s.surface, mark),
        // The small capital titles of the groups: in a card and, on some pages, on the ground.
        ("Abschnittsüberschrift auf Kasten".into(), section_title_color(s), s.surface, text),
        ("Abschnittsüberschrift auf Grund".into(), section_title_color(s), s.bg, text),
    ];
    let kinds = [
        ("normal", ButtonKind::Normal),
        ("stark", ButtonKind::Strong),
        ("Gefahr", ButtonKind::Danger),
        ("Marke", ButtonKind::Brand(KRT_ORANGE)),
    ];
    for (name, kind) in kinds {
        for hovered in [false, true] {
            let c = button_colors(s, kind, hovered);
            let state = if hovered { " überfahren" } else { "" };
            all.push((format!("Knopf {name}{state}"), c.text, c.fill, text));
        }
    }
    all
}

#[test]
fn every_text_and_mark_is_readable_in_every_scheme() {
    let mut low = Vec::new();
    for s in theme::schemes() {
        for (what, fg, bg, needed) in pairs(s) {
            let ratio = fg.contrast(bg);
            if ratio < needed {
                low.push(format!(
                    "{:<9} {what}: {ratio:.2} (braucht {needed}) {} auf {}",
                    s.id,
                    fg.to_hex(),
                    bg.to_hex()
                ));
            }
        }
    }
    assert!(low.is_empty(), "zu wenig Kontrast:\n{}", low.join("\n"));
}

#[test]
fn contrast_matches_known_values() {
    let black = Color::hex("#000000");
    let white = Color::hex("#ffffff");
    assert!((black.contrast(white) - 21.0).abs() < 1e-9);
    assert!((white.contrast(white) - 1.0).abs() < 1e-9);
    // WCAG example: #777777 on white is about 4.48.
    assert!((Color::hex("#777777").contrast(white) - 4.48).abs() < 0.01);
}
