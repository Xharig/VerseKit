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

//! The error color of every scheme. `red` is the loud one (dots, borders, fills, the text on the overlay ground):
//! strong, readable as text on the ground and as a mark (3:1) on every other, and clearly apart from the colors it
//! is mistaken for (gold and yellow, which mean warnings, the orange of the brand buttons, the accent).
//! `red_text` is the red for text on boxes, fields, bars and the danger button: 4.5:1 on all of them, in the
//! schemes with light boxes (`krt`) a lighter shade of the same red. Every scheme but `krt` has `red_text == red`,
//! and `krt` has the same loud red as `original`, `eis` and `nebel`.
//! `-- --nocapture` prints the numbers of every scheme.

use versekit_core::theme::{self, Color, KRT_ORANGE, Scheme};

/// Text on its ground, after WCAG 2.x.
const TEXT_CONTRAST: f64 = 4.5;
/// Dots, borders and fills on their ground, after WCAG 2.x (non-text contrast).
const MARK_CONTRAST: f64 = 3.0;
/// How strong the loud red is at least: the chroma C* of CIE L*a*b* (a pale pink is about 30, the salmon `krt` had
/// 56, a loud red 70 and more).
const MIN_CHROMA: f64 = 70.0;
/// The red for text on boxes may give up some of it for the contrast, but stays a red and no pink.
const MIN_TEXT_CHROMA: f64 = 50.0;
/// How far the hue of the text red may turn from the loud red (degrees in CIE L*a*b*).
const MAX_HUE_TURN: f64 = 10.0;
/// How far the red is from gold and yellow at least (CIE76 Delta E; about 2.3 is the least a trained eye sees).
const MIN_FROM_WARNINGS: f64 = 45.0;
/// How far from the orange of the brand buttons and from the accent.
const MIN_FROM_ORANGE: f64 = 40.0;
/// In `glut` the accent is red as well; the error color is only kept apart as far as that scheme allows.
const MIN_FROM_RED_ACCENT: f64 = 25.0;

/// A color in CIE L*a*b* (D65).
fn lab(color: Color) -> (f64, f64, f64) {
    let linear = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let (r, g, b) = (linear(color.r), linear(color.g), linear(color.b));
    let x = (0.4124564 * r + 0.3575761 * g + 0.1804375 * b) / 0.95047;
    let y = 0.2126729 * r + 0.7151522 * g + 0.0721750 * b;
    let z = (0.0193339 * r + 0.119192 * g + 0.9503041 * b) / 1.08883;
    let f = |t: f64| if t > 216.0 / 24389.0 { t.cbrt() } else { (24389.0 / 27.0 * t + 16.0) / 116.0 };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    (116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz))
}

fn distance(a: Color, b: Color) -> f64 {
    let (a, b) = (lab(a), lab(b));
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2) + (a.2 - b.2).powi(2)).sqrt()
}

fn chroma(color: Color) -> f64 {
    let (_, a, b) = lab(color);
    a.hypot(b)
}

/// The angle of the hue in CIE L*a*b*, in degrees.
fn hue(color: Color) -> f64 {
    let (_, a, b) = lab(color);
    b.atan2(a).to_degrees().rem_euclid(360.0)
}

/// How far two hue angles are apart, 0 … 180.
fn hue_turn(a: Color, b: Color) -> f64 {
    let turn = (hue(a) - hue(b)).abs();
    turn.min(360.0 - turn)
}

/// What is wrong with the `red` and the `red_text` of a scheme; empty when nothing.
fn faults(s: &Scheme) -> Vec<String> {
    let mut found = Vec::new();
    // The loud red: text on the overlay ground, a mark on every other ground.
    let ratio = s.red.contrast(s.bg);
    if ratio < TEXT_CONTRAST {
        found.push(format!("{}: Rot auf Grund nur {ratio:.2}:1 als Schrift", s.id));
    }
    let marks = [("Kasten", s.surface), ("Feld", s.field), ("Leiste", s.bar), ("Gefahrknopf", s.danger_fill)];
    for (name, ground) in marks {
        let ratio = s.red.contrast(ground);
        if ratio < MARK_CONTRAST {
            found.push(format!("{}: Rot auf {name} nur {ratio:.2}:1 als Marke", s.id));
        }
    }
    if chroma(s.red) < MIN_CHROMA {
        found.push(format!("{}: Rot zu blass (Sättigung {:.0} statt {MIN_CHROMA})", s.id, chroma(s.red)));
    }
    // The red for text: readable everywhere, still a red.
    let text = s.red_text();
    let grounds = [("Grund", s.bg), ("Kasten", s.surface), ("Feld", s.field), ("Leiste", s.bar), ("Gefahrknopf", s.danger_fill)];
    for (name, ground) in grounds {
        let ratio = text.contrast(ground);
        if ratio < TEXT_CONTRAST {
            found.push(format!("{}: Schrift-Rot auf {name} nur {ratio:.2}:1", s.id));
        }
    }
    if chroma(text) < MIN_TEXT_CHROMA {
        found.push(format!("{}: Schrift-Rot zu blass (Sättigung {:.0} statt {MIN_TEXT_CHROMA})", s.id, chroma(text)));
    }
    if hue_turn(text, s.red) > MAX_HUE_TURN {
        found.push(format!("{}: Schrift-Rot dreht den Farbton um {:.0}° (höchstens {MAX_HUE_TURN})", s.id, hue_turn(text, s.red)));
    }
    let apart = [
        ("Gold", s.gold, MIN_FROM_WARNINGS),
        ("Gelb", s.yellow, MIN_FROM_WARNINGS),
        ("Orange der Markenknöpfe", KRT_ORANGE, MIN_FROM_ORANGE),
        ("Akzent", s.accent, if s.id == "glut" { MIN_FROM_RED_ACCENT } else { MIN_FROM_ORANGE }),
    ];
    for (shade, color) in [("Rot", s.red), ("Schrift-Rot", text)] {
        for (name, other, least) in apart {
            let far = distance(color, other);
            if far < least {
                found.push(format!("{}: {shade} nur {far:.0} von {name} entfernt (mindestens {least})", s.id));
            }
        }
    }
    found
}

#[test]
fn the_red_of_every_scheme_is_strong_readable_and_apart_from_gold_yellow_and_orange() {
    for s in theme::schemes() {
        println!(
            "{:9} {} Schrift {} ({:.2} auf Kasten, Sättigung {:.0}) Sättigung {:.0} | Kontrast Grund {:.2} Kasten {:.2} Feld {:.2} Leiste {:.2} | Abstand Gold {:.0} Gelb {:.0} Orange {:.0} Akzent {:.0}",
            s.id,
            s.red.to_hex(),
            s.red_text().to_hex(),
            s.red_text().contrast(s.surface),
            chroma(s.red_text()),
            chroma(s.red),
            s.red.contrast(s.bg),
            s.red.contrast(s.surface),
            s.red.contrast(s.field),
            s.red.contrast(s.bar),
            distance(s.red, s.gold),
            distance(s.red, s.yellow),
            distance(s.red, KRT_ORANGE),
            distance(s.red, s.accent),
        );
    }
    let all: Vec<String> = theme::schemes().iter().flat_map(faults).collect();
    assert!(all.is_empty(), "Rot nicht klar genug:\n{}", all.join("\n"));
}

/// The check sees the reds it is made for: the ones before the change, a washed-out pink and an orange.
#[test]
fn the_red_check_finds_the_old_reds_a_pale_pink_and_an_orange() {
    let with_red = |id: &str, hex: &str| {
        let mut s = theme::by_id(id).expect("scheme").clone();
        s.red = Color::hex(hex);
        s
    };
    // The error color of `glut` before: an orange that is hardly told from gold and yellow.
    let old_glut = faults(&with_red("glut", "#ffa53a"));
    assert!(old_glut.iter().any(|f| f.contains("Gelb")) && old_glut.iter().any(|f| f.contains("Gold")), "{old_glut:?}");
    // The red of `original` before: weak, and not even readable as text on the bars.
    let old_original = faults(&with_red("original", "#e05454"));
    assert!(old_original.iter().any(|f| f.contains("blass")), "{old_original:?}");
    // A pale pink is no red.
    assert!(faults(&with_red("original", "#ffb3b3")).iter().any(|f| f.contains("blass")));
    // The salmon of `krt` before: close to the orange of its accent.
    let old_krt = faults(&with_red("krt", "#ff7a70"));
    assert!(old_krt.iter().any(|f| f.contains("Orange") || f.contains("Akzent")), "{old_krt:?}");
    // The salmon of `krt` a day later: far enough from the orange, but pastel, not loud.
    let pastel_krt = faults(&with_red("krt", "#ff7782"));
    assert!(pastel_krt.iter().any(|f| f.contains("blass")), "{pastel_krt:?}");
    // A loud red the overlay ground cannot carry as text.
    assert!(faults(&with_red("krt", "#e53935")).iter().any(|f| f.contains("Grund")));
    // The reds now are fine.
    for s in theme::schemes() {
        assert!(faults(s).is_empty(), "{:?}", faults(s));
    }
}

/// `krt` has the loud red of the other schemes (the Executive Hangar line of the overlay was hardly told from the
/// orange accent); text on its light boxes has its own, lighter shade; no other scheme needs one.
#[test]
fn krt_has_the_loud_red_of_the_others_and_a_lighter_one_for_text_on_its_boxes() {
    let loud = theme::by_id("original").expect("original").red;
    assert_eq!(loud.to_hex(), "#fe3d51");
    for id in ["eis", "nebel"] {
        assert_eq!(theme::by_id(id).expect(id).red, loud, "{id}");
    }
    let krt = theme::by_id("krt").expect("krt");
    assert_eq!(krt.red, loud);
    // On the overlay ground the loud red itself is text.
    assert!(krt.red.contrast(krt.bg) >= TEXT_CONTRAST);
    // On the boxes it is not, the text red is: 4.5:1 and the lightest change that gets there.
    assert!(krt.red.contrast(krt.surface) < TEXT_CONTRAST);
    assert!(krt.red_text().contrast(krt.surface) >= TEXT_CONTRAST);
    assert_ne!(krt.red_text(), krt.red);
    let white = Color::hex("#ffffff");
    let share = (0..=100)
        .find(|n| krt.red.mix(white, f64::from(*n) / 100.0) == krt.red_text())
        .expect("red_text is the red mixed with white");
    let one_step_less = krt.red.mix(white, f64::from(share - 1) / 100.0);
    assert!(one_step_less.contrast(krt.surface) < TEXT_CONTRAST, "red_text is lighter than it must be");
    // Everywhere else the loud red is already readable and the text red is the same.
    for s in theme::schemes().iter().filter(|s| s.id != "krt") {
        assert_eq!(s.red_text(), s.red, "{}", s.id);
    }
}
