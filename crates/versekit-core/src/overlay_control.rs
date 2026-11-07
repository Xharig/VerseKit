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

//! Switches for a running overlay, in a small text file the overlay watches.
//!
//! The overlay reads them only when it is started with `--steuerung <file>`;
//! without that switch nothing is watched. Every line is `key=value`; the
//! file is the whole state, not a list of changes: a key that is missing means
//! "as the data say".
//!
//! | key | value |
//! |---|---|
//! | `spiel` | `ja` or `nein`: the game counts as running or not |
//! | `update` | the version the update hint names |
//! | `ecke` | the corner (`overlay_ecke`) |
//! | `leiste` | the bar side (`overlay_leiste`) |
//! | `modus` | `immer` or `popup` (`overlay_modus`) |
//! | `sekunden` | how long a find stays open (`popup_sekunden`) |
//! | `lage` | `BxH+X+Y` in points, or `daten`: where the overlay goes |
//! | `lage-nr` | a number; the place is taken again whenever it changes |

use std::io;

use crate::jsonio::{self, Newline};
use crate::overlay::{self, Geometry};

/// The name of the file inside the data folder of a copy.
pub const FILE: &str = "overlay-steuerung.txt";

/// Where the overlay is asked to go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// Back to the place the data remember.
    Remembered,
    /// This size and, if given, this place, in points of the system scaling.
    Given(Geometry),
}

/// The state the overlay is asked to take. `None` means "as the data say".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Control {
    pub game: Option<bool>,
    pub update: Option<String>,
    pub corner: Option<String>,
    pub bar: Option<String>,
    pub mode: Option<String>,
    pub seconds: Option<i64>,
    pub place: Option<Place>,
    /// Counts the requests for a place: the same place asked for again moves the overlay again.
    pub place_nr: u64,
}

impl Control {
    /// The file text.
    pub fn render(&self) -> String {
        let mut out = String::new();
        let mut line = |key: &str, value: String| {
            out.push_str(key);
            out.push('=');
            out.push_str(&value);
            out.push('\n');
        };
        if let Some(game) = self.game {
            line("spiel", if game { "ja" } else { "nein" }.into());
        }
        if let Some(update) = &self.update {
            line("update", update.clone());
        }
        if let Some(corner) = &self.corner {
            line("ecke", corner.clone());
        }
        if let Some(bar) = &self.bar {
            line("leiste", bar.clone());
        }
        if let Some(mode) = &self.mode {
            line("modus", mode.clone());
        }
        if let Some(seconds) = self.seconds {
            line("sekunden", seconds.to_string());
        }
        match self.place {
            Some(Place::Remembered) => line("lage", "daten".into()),
            Some(Place::Given((w, h, place))) => {
                let text = match place {
                    Some((x, y)) => overlay::geometry_text(overlay::Rect::new(x, y, w, h)),
                    None => format!("{w}x{h}"),
                };
                line("lage", text);
            }
            None => {}
        }
        line("lage-nr", self.place_nr.to_string());
        out
    }

    /// The state a file text asks for; lines that mean nothing are skipped.
    pub fn parse(text: &str) -> Control {
        let mut control = Control::default();
        for raw in text.lines() {
            let Some((key, value)) = raw.split_once('=') else { continue };
            let (key, value) = (key.trim(), value.trim());
            if value.is_empty() {
                continue;
            }
            match key {
                "spiel" => {
                    control.game = match value {
                        "ja" => Some(true),
                        "nein" => Some(false),
                        _ => None,
                    }
                }
                "update" => control.update = Some(value.to_string()),
                "ecke" => control.corner = Some(value.to_string()),
                "leiste" => control.bar = Some(value.to_string()),
                "modus" => control.mode = Some(value.to_string()),
                "sekunden" => control.seconds = value.parse().ok(),
                "lage" => {
                    control.place = if value == "daten" {
                        Some(Place::Remembered)
                    } else {
                        overlay::parse_geometry(value).map(Place::Given)
                    };
                }
                "lage-nr" => control.place_nr = value.parse().unwrap_or(0),
                _ => {}
            }
        }
        control
    }
}

/// The file at `path`, read: `None` if there is none or it cannot be read.
pub fn read(path: &str) -> Option<Control> {
    std::fs::read(path).ok().map(|bytes| Control::parse(&String::from_utf8_lossy(&bytes)))
}

/// Writes the file at `path`; it is replaced as a whole, so a reader never sees half of it.
pub fn write(path: &str, control: &Control) -> io::Result<()> {
    jsonio::write_text_atomic(path, &control.render(), Newline::Keep)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full() -> Control {
        Control {
            game: Some(false),
            update: Some("5.0.1".into()),
            corner: Some("unten-rechts".into()),
            bar: Some("unten".into()),
            mode: Some("popup".into()),
            seconds: Some(9),
            place: Some(Place::Given((540, 390, Some((-20, 40))))),
            place_nr: 3,
        }
    }

    #[test]
    fn what_is_rendered_reads_back_as_the_same_state() {
        let control = full();
        assert_eq!(Control::parse(&control.render()), control);
        let remembered = Control { place: Some(Place::Remembered), ..Control::default() };
        assert_eq!(Control::parse(&remembered.render()), remembered);
        let size_only = Control { place: Some(Place::Given((300, 200, None))), ..Control::default() };
        assert_eq!(Control::parse(&size_only.render()), size_only);
        assert_eq!(Control::parse(&Control::default().render()), Control::default());
    }

    #[test]
    fn a_missing_key_means_as_the_data_say_and_nonsense_is_skipped() {
        let control =
            Control::parse("spiel=ja\n\nkaputt\nsekunden=viel\nlage=unsinn\nunbekannt=1\n  ecke = oben-links \n");
        assert_eq!(control.game, Some(true));
        // Only "ja" and "nein" mean something; another word asks for nothing.
        assert_eq!(Control::parse("spiel=true\n").game, None);
        assert_eq!(Control::parse("spiel=nein\n").game, Some(false));
        assert_eq!(control.corner.as_deref(), Some("oben-links"));
        assert_eq!(control.seconds, None);
        assert_eq!(control.place, None);
        assert_eq!(control.update, None);
        assert_eq!(control.bar, None);
    }

    #[test]
    fn the_file_is_written_whole_and_read_from_disk() {
        let dir = std::env::temp_dir().join(format!("vk-control-{}", crate::testdir::run_tag()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join(FILE).to_string_lossy().into_owned();
        assert_eq!(read(&path), None, "no file, no state");
        write(&path, &full()).expect("written");
        assert_eq!(read(&path), Some(full()));
        let second = Control { game: Some(true), ..Control::default() };
        write(&path, &second).expect("rewritten");
        assert_eq!(read(&path), Some(second), "the old keys are gone");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
