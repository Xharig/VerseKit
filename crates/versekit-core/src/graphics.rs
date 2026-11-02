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

//! The graphics settings of Star Citizen in `attributes.xml` (`graphics.py` of 4.12): reading and writing.
//!
//! The file lies beside `actionmaps.xml` (`USER/Client/0/Profiles/default/attributes.xml`), one
//! `<Attr name="…" value="…"/>` to a line. It is **text** (no binary XML) and is read and written line by
//! line, not with an XML parser: whatever is not in that form is neither read nor touched.
//!
//! ⚠⚠ **The game saves only what differs from the default.** A setting on its default has no line. A missing
//! line reads as the default; writing the default takes the line out, any other value sets it or puts it in.
//! The defaults are those of the template in the archive (`Data/Libs/Config/Profiles/default/attributes.xml`).
//!
//! ⚠ Which number is which menu entry stands in no file of the game: the tables below were measured in the
//! menu of the game (change the menu, read the file).
//!
//! ⚠ The game writes the file when its settings are left, so it is written only while the game is quiet
//! (`game_files`, which refuses while it runs or when nobody can tell).
//!
//! A value is an `Option<f64>`: `None` is "no line" (a quality row that follows the overall level of the game).

use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;

use crate::errors;
use crate::game_files::{self, WriteError};
use crate::paths::Paths;

/// How a setting is chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// One of fixed values with a text.
    Choice,
    /// 0 or 1.
    Switch,
    /// 0 to 1 (0 to 100 % on the page).
    Slider,
}

/// One setting of the page.
#[derive(Clone, Copy, Debug)]
pub struct Setting {
    /// The name in the file.
    pub name: &'static str,
    pub kind: Kind,
    pub default: f64,
    /// The values with the key of their text (a choice only).
    pub choices: &'static [(i64, &'static str)],
}

/// The nine settings, in the order of the page.
pub const SETTINGS: [Setting; 9] = [
    Setting {
        name: "WindowMode",
        kind: Kind::Choice,
        default: 0.0,
        choices: &[(0, "s_gr_fenster_0"), (1, "s_gr_fenster_1"), (2, "s_gr_fenster_2")],
    },
    Setting { name: "VSync", kind: Kind::Switch, default: 1.0, choices: &[] },
    Setting {
        name: "Upscaling",
        kind: Kind::Choice,
        default: 0.0,
        choices: &[(0, "s_gr_up_0"), (1, "s_gr_up_1"), (2, "s_gr_up_2"), (3, "s_gr_up_3"), (4, "s_gr_up_4")],
    },
    Setting {
        name: "UpscalingTechnique",
        kind: Kind::Choice,
        default: 0.0,
        choices: &[(0, "s_gr_tech_0"), (1, "s_gr_tech_1"), (2, "s_gr_tech_2")],
    },
    Setting {
        name: "UpscalingModel",
        kind: Kind::Choice,
        default: 0.0,
        choices: &[(0, "s_gr_modell_0"), (1, "s_gr_modell_1")],
    },
    Setting { name: "MotionBlur", kind: Kind::Switch, default: 1.0, choices: &[] },
    Setting { name: "FilmGrain", kind: Kind::Switch, default: 1.0, choices: &[] },
    Setting { name: "Sharpening", kind: Kind::Slider, default: 0.0, choices: &[] },
    Setting { name: "ChromaticAberration", kind: Kind::Slider, default: 1.0, choices: &[] },
];

/// The quality rows of the menu of the game, in its order, with the key of their text. `SysSpec_Particles` and
/// `SysSpec_PlanetTerrainVirtualTextures` are in the file but not in the menu: they are in [`EDIT_ROWS`].
pub const QUALITY_ROWS: [(&str, &str); 16] = [
    ("SysSpec_ObjectDetail", "s_gr_q_objektdetails"),
    ("SysSpec_ObjectViewDistance", "s_gr_q_sichtweite"),
    ("SysSpec_TextureQuality", "s_gr_q_texturqualitaet"),
    ("SysSpec_TextureDetail", "s_gr_q_texturdetails"),
    ("SysSpec_TextureGround", "s_gr_q_bodentexturen"),
    ("SysSpec_TextureFiltering", "s_gr_q_texturfilter"),
    ("SysSpec_ShadowMaps", "s_gr_q_schatten"),
    ("SysSpec_ShadowScreenSpace", "s_gr_q_schatten_bild"),
    ("SysSpec_PlanetVolumetricClouds", "s_gr_q_wolken"),
    ("SysSpec_GasCloud", "s_gr_q_gaswolken"),
    ("SysSpec_Fog", "s_gr_q_nebel"),
    ("SysSpec_WaterCaustics", "s_gr_q_wasserkaustik"),
    ("SysSpec_WaterSim", "s_gr_q_wassersim"),
    ("SysSpec_Shading", "s_gr_q_shader"),
    ("SysSpec_PostProcessing", "s_gr_q_nachbearbeitung"),
    ("SysSpec_VideoComms", "s_gr_q_video"),
];

/// Every quality row of the file: the 16 of the menu and two more.
pub const EDIT_ROWS: [(&str, &str); 18] = [
    QUALITY_ROWS[0],
    QUALITY_ROWS[1],
    QUALITY_ROWS[2],
    QUALITY_ROWS[3],
    QUALITY_ROWS[4],
    QUALITY_ROWS[5],
    QUALITY_ROWS[6],
    QUALITY_ROWS[7],
    QUALITY_ROWS[8],
    QUALITY_ROWS[9],
    QUALITY_ROWS[10],
    QUALITY_ROWS[11],
    QUALITY_ROWS[12],
    QUALITY_ROWS[13],
    QUALITY_ROWS[14],
    QUALITY_ROWS[15],
    ("SysSpec_Particles", "s_gr_q_partikel"),
    ("SysSpec_PlanetTerrainVirtualTextures", "s_gr_q_planetentexturen"),
];

/// The overall quality: in the file only a number to remember by; what counts stands in the rows.
pub const OVERALL: &str = "SysSpec";

/// The highest level of a row (5 for some, else 4), `None` for a name that is no quality row.
pub fn top_level(name: &str) -> Option<i64> {
    const FIVE: [&str; 8] = [
        "SysSpec_ObjectDetail",
        "SysSpec_ObjectViewDistance",
        "SysSpec_TextureQuality",
        "SysSpec_ShadowMaps",
        "SysSpec_PlanetVolumetricClouds",
        "SysSpec_Fog",
        "SysSpec_PlanetTerrainVirtualTextures",
        OVERALL,
    ];
    if FIVE.contains(&name) {
        Some(5)
    } else if EDIT_ROWS.iter().any(|(row, _)| *row == name) {
        Some(4)
    } else {
        None
    }
}

/// What each preset sets in a row, from the template of the game (`sys_spec_Full.cfg`, sections 1 to 5). A row
/// not named stands on the level itself.
fn preset_exception(level: i64, row: &str) -> Option<i64> {
    let table: &[(&str, i64)] = match level {
        2 => &[("SysSpec_TextureFiltering", 3), ("SysSpec_TextureGround", 3)],
        4 => &[("SysSpec_TextureDetail", 3), ("SysSpec_TextureFiltering", 3), ("SysSpec_TextureGround", 3)],
        5 => &[
            ("SysSpec_GasCloud", 4),
            ("SysSpec_PlanetVolumetricClouds", 4),
            ("SysSpec_PostProcessing", 4),
            ("SysSpec_Shading", 4),
            ("SysSpec_ShadowScreenSpace", 4),
            ("SysSpec_TextureDetail", 3),
            ("SysSpec_TextureFiltering", 3),
            ("SysSpec_TextureGround", 3),
            ("SysSpec_VideoComms", 4),
            ("SysSpec_WaterCaustics", 4),
            ("SysSpec_WaterSim", 4),
            ("SysSpec_Particles", 4),
        ],
        _ => &[],
    };
    table.iter().find(|(name, _)| *name == row).map(|(_, to)| *to)
}

/// The level the preset `machine_class` (1 to 5) sets for `row`.
pub fn preset_level(machine_class: i64, row: &str) -> i64 {
    preset_exception(machine_class, row).unwrap_or(machine_class)
}

/// All rows of [`EDIT_ROWS`] with the level the preset sets.
pub fn preset_rows(level: i64) -> Vec<(&'static str, i64)> {
    EDIT_ROWS.iter().map(|(row, _)| (*row, preset_level(level, row))).collect()
}

/// A line `<Attr name="…" value="…"/>` as the game writes it, with the line end that goes with it.
fn line_pattern() -> &'static Regex {
    static LINE: OnceLock<Regex> = OnceLock::new();
    LINE.get_or_init(|| {
        Regex::new(r#"(?m)^[ \t]*<Attr\s+name="([^"]*)"\s+value="([^"]*)"\s*/>[ \t]*\r?\n?"#).expect("pattern")
    })
}

/// A number the way the file says it: whole numbers stay whole. `None` for what is no number.
pub fn number(text: &str) -> Option<f64> {
    text.trim().parse::<f64>().ok().filter(|value| value.is_finite())
}

/// The text of a value: a whole number without a point, else four places without trailing zeros.
pub fn format_value(value: f64) -> String {
    if value.fract() != 0.0 {
        let text = format!("{value:.4}");
        let text = text.trim_end_matches('0');
        return text.trim_end_matches('.').to_string();
    }
    format!("{}", value as i64)
}

/// Every setting line with name and text of value, the last of a name counting.
fn found(text: &str) -> HashMap<String, String> {
    line_pattern().captures_iter(text).map(|c| (c[1].to_string(), c[2].to_string())).collect()
}

/// A whole number from 1 to 5, as a level.
fn as_level(value: Option<f64>) -> Option<i64> {
    value.filter(|v| v.fract() == 0.0 && (1.0..=5.0).contains(v)).map(|v| v as i64)
}

/// The nine settings read from the text: a missing line or a value that is no number stands as the default.
pub fn read_text(text: &str) -> Vec<(&'static str, f64)> {
    let found = found(text);
    SETTINGS
        .iter()
        .map(|setting| {
            let value = found.get(setting.name).and_then(|v| number(v)).unwrap_or(setting.default);
            (setting.name, value)
        })
        .collect()
}

/// The levels (1 to 5) of the rows of [`EDIT_ROWS`] and of [`OVERALL`] that stand in the text.
pub fn levels_text(text: &str) -> Vec<(&'static str, i64)> {
    let found = found(text);
    EDIT_ROWS
        .iter()
        .map(|(row, _)| *row)
        .chain(std::iter::once(OVERALL))
        .filter_map(|row| as_level(found.get(row).and_then(|v| number(v))).map(|level| (row, level)))
        .collect()
}

/// The levels of the rows of the menu that stand in the text. A row that is missing follows the preset.
pub fn quality_text(text: &str) -> Vec<(&'static str, i64)> {
    let found = found(text);
    QUALITY_ROWS
        .iter()
        .filter_map(|(row, _)| as_level(found.get(*row).and_then(|v| number(v))).map(|level| (*row, level)))
        .collect()
}

/// The rows against the preset of the machine: `(above, below)`, each `(row, is, should)` in the order of the
/// menu.
#[allow(clippy::type_complexity)]
pub fn compare(
    levels: &[(&str, i64)],
    machine_class: i64,
) -> (Vec<(&'static str, i64, i64)>, Vec<(&'static str, i64, i64)>) {
    let (mut above, mut below) = (Vec::new(), Vec::new());
    for (row, _) in QUALITY_ROWS {
        let Some((_, level)) = levels.iter().find(|(name, _)| *name == row) else { continue };
        let wanted = preset_level(machine_class, row);
        match level.cmp(&wanted) {
            std::cmp::Ordering::Greater => above.push((row, *level, wanted)),
            std::cmp::Ordering::Less => below.push((row, *level, wanted)),
            std::cmp::Ordering::Equal => {}
        }
    }
    (above, below)
}

/// Why a change was refused before anything was touched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeError {
    /// A level that is no whole number from 1 to the top of its row (the names).
    Level(Vec<String>),
    /// A name that is no setting of the page (the names).
    Unknown(Vec<String>),
    /// No setting line and no `</Attributes>` to put one before.
    NoClose,
}

/// `text` with the setting `name` on `value`: the default takes the line out, else it is replaced or put in at
/// its place in the alphabet.
///
/// # Errors
///
/// [`ChangeError::NoClose`] where there is no place to put a line.
pub fn apply(text: &str, name: &str, value: Option<f64>, default: Option<f64>) -> Result<String, ChangeError> {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let spans: Vec<(usize, usize, &str)> = line_pattern()
        .captures_iter(text)
        .map(|c| {
            let whole = c.get(0).expect("whole match");
            (whole.start(), whole.end(), c.get(1).expect("name").as_str())
        })
        .collect();
    let own: Vec<(usize, usize)> = spans.iter().filter(|(_, _, n)| *n == name).map(|(s, e, _)| (*s, *e)).collect();
    // The default takes every line of the name out (a `None` with a number as default has nothing to write
    // either; 4.12 stopped with an error there, the page never asks that).
    let Some(value) = value.filter(|_| value != default) else {
        let mut out = text.to_string();
        for (start, end) in own.iter().rev() {
            out.replace_range(start..end, "");
        }
        return Ok(out);
    };
    let new_line = format!(" <Attr name=\"{name}\" value=\"{}\"/>{newline}", format_value(value));
    if let Some((start, end)) = own.first() {
        return Ok(format!("{}{new_line}{}", &text[..*start], &text[*end..]));
    }
    let at = if let Some((start, _, _)) = spans.iter().find(|(_, _, n)| *n > name) {
        *start
    } else if let Some((_, end, _)) = spans.last() {
        *end
    } else {
        text.rfind("</Attributes>").ok_or(ChangeError::NoClose)?
    };
    Ok(format!("{}{new_line}{}", &text[..at], &text[at..]))
}

/// The defaults of every name the page may change: a number for the nine, `None` for the quality rows.
fn default_of(name: &str) -> Option<Option<f64>> {
    if let Some(setting) = SETTINGS.iter().find(|s| s.name == name) {
        return Some(Some(setting.default));
    }
    (top_level(name).is_some()).then_some(None)
}

/// The changes as a dictionary has them: a name once, at the place of its first mention, with its last value
/// (4.12 hands the changes over as a dictionary).
fn as_dict(changes: &[(String, Option<f64>)]) -> Vec<(String, Option<f64>)> {
    let mut out: Vec<(String, Option<f64>)> = Vec::new();
    for (name, value) in changes {
        match out.iter_mut().find(|(known, _)| known == name) {
            Some(slot) => slot.1 = *value,
            None => out.push((name.clone(), *value)),
        }
    }
    out
}

/// A level out of range or a name that is no setting: refused before anything is read or touched (a level first).
///
/// # Errors
///
/// [`ChangeError::Level`] or [`ChangeError::Unknown`].
pub fn validate(changes: &[(String, Option<f64>)]) -> Result<(), ChangeError> {
    let changes = as_dict(changes);
    let bad: Vec<String> = changes
        .iter()
        .filter(|(name, value)| {
            top_level(name).is_some_and(|top| value.is_some_and(|v| !(v.fract() == 0.0 && v >= 1.0 && v <= top as f64)))
        })
        .map(|(name, _)| name.clone())
        .collect();
    if !bad.is_empty() {
        return Err(ChangeError::Level(bad));
    }
    let unknown: Vec<String> =
        changes.iter().filter(|(name, _)| default_of(name).is_none()).map(|(name, _)| name.clone()).collect();
    if !unknown.is_empty() {
        return Err(ChangeError::Unknown(unknown));
    }
    Ok(())
}

/// The text with every change made in turn, or why it is refused. `Ok(None)` when the text stays as it is.
///
/// # Errors
///
/// [`ChangeError`]: a level out of range or an unknown name (nothing is changed), or no place for a line.
pub fn change_text(text: &str, changes: &[(String, Option<f64>)]) -> Result<Option<String>, ChangeError> {
    let changes = as_dict(changes);
    validate(&changes)?;
    let mut fresh = text.to_string();
    for (name, value) in &changes {
        let default = default_of(name).expect("known name");
        fresh = apply(&fresh, name, *value, default)?;
    }
    Ok((fresh != text).then_some(fresh))
}

/// The `attributes.xml` of the player under the game folder. The case changes (`USER`/`user`,
/// `Client`/`client`), so the four spellings are tried, not guessed.
pub fn attribute_file(paths: &Paths, game_folder: Option<&str>) -> Option<String> {
    let root = match game_folder.filter(|f| !f.is_empty()) {
        Some(folder) => folder.to_string(),
        None => paths.game_folder()?,
    };
    let f = paths.host.flavor;
    for top in ["USER", "user"] {
        for middle in ["Client", "client"] {
            let path = f.join_all(&root, &[top, middle, "0", "Profiles", "default", "attributes.xml"]);
            if std::path::Path::new(&path).is_file() {
                return Some(path);
            }
        }
    }
    None
}

/// Why a file could not be changed.
#[derive(Debug)]
pub enum Failure {
    /// No `attributes.xml` found.
    NoFile,
    /// It could not be read.
    Read,
    /// Refused before touching it.
    Refused(ChangeError),
    /// The game runs (or nobody can tell) or the copy or the writing failed.
    Write(WriteError),
}

/// The settings of the file: the nine, `None` when the file is not there or cannot be read.
pub fn read(paths: &Paths, path: Option<&str>) -> Option<Vec<(&'static str, f64)>> {
    let path = path.map(str::to_string).or_else(|| attribute_file(paths, None))?;
    match std::fs::read(&path) {
        Ok(bytes) => Some(read_text(&String::from_utf8_lossy(&bytes))),
        Err(error) => {
            errors::record("graphics.read", &error);
            None
        }
    }
}

/// The levels of the rows of the file (all of [`EDIT_ROWS`] and [`OVERALL`] that stand in it).
pub fn read_levels(paths: &Paths, path: Option<&str>) -> Vec<(&'static str, i64)> {
    let Some(path) = path.map(str::to_string).or_else(|| attribute_file(paths, None)) else { return Vec::new() };
    match std::fs::read(&path) {
        Ok(bytes) => levels_text(&String::from_utf8_lossy(&bytes)),
        Err(error) => {
            errors::record("graphics.read_levels", &error);
            Vec::new()
        }
    }
}

/// Writes the changes into `attributes.xml`: first refused if a name or a level is wrong, then the file is read,
/// every change made in turn, and only if the text is another one a copy is made (`game_files`) and the file is
/// written. Gives the copy, `None` where nothing had to be written. The game must be quiet.
///
/// # Errors
///
/// [`Failure`]: no file, not readable, refused, or the game runs / the writing failed.
pub fn write(paths: &Paths, path: Option<&str>, changes: &[(String, Option<f64>)]) -> Result<Option<String>, Failure> {
    let (path, fresh) = prepare(paths, path, changes)?;
    let Some(fresh) = fresh else { return Ok(None) };
    game_files::write(paths, &path, fresh.as_bytes()).map_err(Failure::Write)
}

/// Everything [`write`] does except the writing, so that a wrong file or a wrong change is known before any
/// other file is touched.
///
/// # Errors
///
/// [`Failure`] as for [`write`], without the writing.
pub fn check(paths: &Paths, path: Option<&str>, changes: &[(String, Option<f64>)]) -> Result<(), Failure> {
    prepare(paths, path, changes).map(|_| ())
}

/// The path and the new text (`None`: nothing to change).
fn prepare(
    paths: &Paths,
    path: Option<&str>,
    changes: &[(String, Option<f64>)],
) -> Result<(String, Option<String>), Failure> {
    let path = path.map(str::to_string).or_else(|| attribute_file(paths, None)).ok_or(Failure::NoFile)?;
    // The names and levels are checked first, before the file is read.
    validate(changes).map_err(Failure::Refused)?;
    // Bytes that are no UTF-8 are not read "as far as they go": writing them back would put replacement signs in.
    let text = match std::fs::read(&path)
        .map_err(|e| e.to_string())
        .and_then(|b| String::from_utf8(b).map_err(|e| e.to_string()))
    {
        Ok(text) => text,
        Err(error) => {
            errors::record("graphics.write_read", &error);
            return Err(Failure::Read);
        }
    };
    let fresh = change_text(&text, changes).map_err(Failure::Refused)?;
    Ok((path, fresh))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_start;
    use crate::paths::{ENV_INSTALL_DIR, Host};
    use crate::pypath::Flavor;

    const FILE: &str = "<Attributes Version=\"35\">\r\n <Attr name=\"FOV\" value=\"54.7681\"/>\r\n <Attr name=\"SysSpec\" value=\"3\"/>\r\n <Attr name=\"VSync\" value=\"0\"/>\r\n</Attributes>\r\n";

    fn change(name: &str, value: Option<f64>) -> (String, Option<f64>) {
        (name.to_string(), value)
    }

    #[test]
    fn a_setting_is_read_with_its_default_where_the_line_is_missing_or_no_number() {
        let read = read_text(
            "<Attributes>\n <Attr name=\"VSync\" value=\"0\"/>\n <Attr name=\"Sharpening\" value=\"0.35\"/>\n <Attr name=\"Upscaling\" value=\"viel\"/>\n</Attributes>\n",
        );
        let value = |name: &str| read.iter().find(|(n, _)| *n == name).map(|(_, v)| *v);
        assert_eq!(value("VSync"), Some(0.0));
        assert_eq!(value("Sharpening"), Some(0.35));
        assert_eq!(value("Upscaling"), Some(0.0), "no number: the default");
        assert_eq!(value("MotionBlur"), Some(1.0), "no line: the default");
        assert_eq!(read.len(), 9);
        // The last line of a name counts.
        let twice = read_text("<Attr name=\"VSync\" value=\"0\"/>\n<Attr name=\"VSync\" value=\"1\"/>\n");
        assert_eq!(twice[1], ("VSync", 1.0));
    }

    #[test]
    fn a_value_is_written_whole_or_with_four_places_without_trailing_zeros() {
        for (value, text) in [
            (0.0, "0"),
            (1.0, "1"),
            (0.35, "0.35"),
            (0.123456, "0.1235"),
            (0.99999, "1"),
            (0.00001, "0"),
            (54.7681, "54.7681"),
        ] {
            assert_eq!(format_value(value), text, "{value}");
        }
    }

    #[test]
    fn a_line_is_replaced_put_in_its_place_in_the_alphabet_or_taken_out_with_the_default() {
        let set = |text: &str, name: &str, value: f64, default: f64| {
            apply(text, name, Some(value), Some(default)).expect("applied")
        };
        // New line in the alphabet: behind "FOV", before "SysSpec"; the line end of the file (CRLF) is kept.
        let put = set(FILE, "MotionBlur", 0.0, 1.0);
        assert_eq!(
            put,
            "<Attributes Version=\"35\">\r\n <Attr name=\"FOV\" value=\"54.7681\"/>\r\n <Attr name=\"MotionBlur\" value=\"0\"/>\r\n <Attr name=\"SysSpec\" value=\"3\"/>\r\n <Attr name=\"VSync\" value=\"0\"/>\r\n</Attributes>\r\n"
        );
        // Behind the last line when it is the largest.
        let last = set(FILE, "WindowMode", 2.0, 0.0);
        assert!(
            last.contains(
                "<Attr name=\"VSync\" value=\"0\"/>\r\n <Attr name=\"WindowMode\" value=\"2\"/>\r\n</Attributes>"
            ),
            "{last}"
        );
        // Replaced where it stands.
        let replaced = set(FILE, "SysSpec", 4.0, f64::NAN);
        assert!(replaced.contains("name=\"SysSpec\" value=\"4\"") && !replaced.contains("value=\"3\""), "{replaced}");
        // The default takes the line out.
        let removed = set(FILE, "VSync", 1.0, 1.0);
        assert!(!removed.contains("VSync"), "{removed}");
        // A level `None` takes the line of a quality row out.
        let none = apply(FILE, "SysSpec", None, None).expect("applied");
        assert!(!none.contains("SysSpec") && none.contains("FOV"), "{none}");
        // LF stays LF; into an empty file before the closing tag.
        let lf = set("<Attributes>\n</Attributes>\n", "VSync", 0.0, 1.0);
        assert_eq!(lf, "<Attributes>\n <Attr name=\"VSync\" value=\"0\"/>\n</Attributes>\n");
        // Without a closing tag nothing can be put in.
        assert_eq!(apply("nothing", "VSync", Some(0.0), Some(1.0)), Err(ChangeError::NoClose));
        // Several lines of a name: the first is replaced, the default takes all.
        let many = "<Attr name=\"VSync\" value=\"0\"/>\n<Attr name=\"VSync\" value=\"1\"/>\n";
        assert_eq!(
            set(many, "VSync", 0.5, 1.0),
            " <Attr name=\"VSync\" value=\"0.5\"/>\n<Attr name=\"VSync\" value=\"1\"/>\n"
        );
        assert_eq!(set(many, "VSync", 1.0, 1.0), "");
    }

    #[test]
    fn levels_and_presets_follow_the_tables_of_the_game() {
        let text = "<Attr name=\"SysSpec_ObjectViewDistance\" value=\"5\"/>\n<Attr name=\"SysSpec_Fog\" value=\"3.5\"/>\n<Attr name=\"SysSpec_WaterSim\" value=\"2\"/>\n<Attr name=\"SysSpec_Particles\" value=\"4\"/>\n<Attr name=\"SysSpec\" value=\"3\"/>\n";
        assert_eq!(
            levels_text(text),
            [("SysSpec_ObjectViewDistance", 5), ("SysSpec_WaterSim", 2), ("SysSpec_Particles", 4), ("SysSpec", 3)],
            "3.5 is no level; the two rows outside the menu and the overall level are read too"
        );
        assert_eq!(quality_text(text), [("SysSpec_ObjectViewDistance", 5), ("SysSpec_WaterSim", 2)], "only the menu");
        assert_eq!(
            (
                preset_level(1, "SysSpec_Fog"),
                preset_level(2, "SysSpec_TextureGround"),
                preset_level(5, "SysSpec_Particles")
            ),
            (1, 3, 4)
        );
        assert_eq!(preset_level(5, "SysSpec_ObjectDetail"), 5);
        assert_eq!(preset_rows(4).len(), 18);
        assert_eq!(top_level("SysSpec_Fog"), Some(5));
        assert_eq!(top_level("SysSpec_Shading"), Some(4));
        assert_eq!(top_level("VSync"), None);
        // Against the preset of the machine (class 3): above and below, in the order of the menu.
        let levels = quality_text(text);
        let (above, below) = compare(&levels, 3);
        assert_eq!(above, [("SysSpec_ObjectViewDistance", 5, 3)]);
        assert_eq!(below, [("SysSpec_WaterSim", 2, 3)]);
    }

    #[test]
    fn a_wrong_request_is_refused_before_anything_is_read_a_level_first() {
        assert_eq!(validate(&[change("SysSpec_Fog", Some(6.0))]), Err(ChangeError::Level(vec!["SysSpec_Fog".into()])));
        assert_eq!(
            validate(&[change("SysSpec_Shading", Some(5.0))]),
            Err(ChangeError::Level(vec!["SysSpec_Shading".into()])),
            "4 is the top there"
        );
        assert_eq!(validate(&[change("SysSpec_Fog", Some(2.5))]), Err(ChangeError::Level(vec!["SysSpec_Fog".into()])));
        assert_eq!(validate(&[change("Unsinn", Some(1.0))]), Err(ChangeError::Unknown(vec!["Unsinn".into()])));
        assert_eq!(
            validate(&[change("Unsinn", Some(1.0)), change("SysSpec_Fog", Some(0.0))]),
            Err(ChangeError::Level(vec!["SysSpec_Fog".into()])),
            "a level before a name"
        );
        assert_eq!(
            validate(&[change("SysSpec_Fog", None), change("VSync", Some(0.0)), change("Sharpening", Some(0.5))]),
            Ok(())
        );
    }

    fn scratch(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("versekit-graphics-{}-{name}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        dir.to_string_lossy().replace('\\', "/")
    }

    /// A user whose game is quiet, with an `attributes.xml` under `user/client/…`.
    fn world(name: &str, age: Option<u64>) -> (String, Paths, String) {
        let home = scratch(name);
        let game = format!("{home}/StarCitizen/LIVE");
        let folder = format!("{game}/user/client/0/Profiles/default");
        std::fs::create_dir_all(&folder).expect("folder");
        let file = format!("{folder}/attributes.xml");
        std::fs::write(&file, FILE).expect("file");
        let mut vars = vec![("SC_BP_HOME", home.as_str()), ("HOME", home.as_str()), (game_start::ENV_NO_PROCESS, "1")];
        if let Some(age) = age {
            std::fs::write(format!("{game}/Game.log"), "x").expect("log");
            let at = std::time::SystemTime::now() - std::time::Duration::from_secs(age);
            std::fs::File::options()
                .write(true)
                .open(format!("{game}/Game.log"))
                .and_then(|f| f.set_modified(at))
                .expect("age");
            vars.push((ENV_INSTALL_DIR, game.as_str()));
        }
        (home.clone(), Paths::new(Host::new(Flavor::Posix, &vars, &home)), file)
    }

    #[test]
    fn the_file_is_found_under_the_game_folder_in_any_spelling_of_the_case() {
        let (home, paths, file) = world("pfad", Some(7200));
        // (On a system that does not tell the case apart the first spelling that exists answers in its own
        // case, so the paths are compared without it.)
        let same = |found: Option<String>, wanted: &str| found.map(|f| f.to_lowercase()) == Some(wanted.to_lowercase());
        assert!(same(attribute_file(&paths, None), &file));
        // `USER/Client` instead of `user/client`: tried in turn.
        let other = format!("{home}/StarCitizen/PTU/USER/Client/0/Profiles/default");
        std::fs::create_dir_all(&other).expect("folder");
        std::fs::write(format!("{other}/attributes.xml"), FILE).expect("file");
        assert!(same(
            attribute_file(&paths, Some(&format!("{home}/StarCitizen/PTU"))),
            &format!("{other}/attributes.xml")
        ));
        assert_eq!(attribute_file(&paths, Some(&format!("{home}/nowhere"))), None);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_write_changes_the_file_with_a_copy_and_a_change_that_changes_nothing_writes_nothing() {
        let (home, paths, file) = world("schreiben", Some(7200));
        let backup = write(
            &paths,
            None,
            &[change("MotionBlur", Some(0.0)), change("SysSpec", Some(4.0)), change("VSync", Some(1.0))],
        )
        .expect("written")
        .expect("a copy");
        let now = std::fs::read_to_string(&file).unwrap();
        assert!(
            now.contains("name=\"MotionBlur\" value=\"0\"")
                && now.contains("name=\"SysSpec\" value=\"4\"")
                && !now.contains("VSync"),
            "{now}"
        );
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), FILE, "the copy holds what was there");
        assert!(backup.contains(".versekit-"), "{backup}");
        // The same again: the text stays, nothing is written, no new copy.
        let count = game_files::backups(&file).len();
        assert!(matches!(write(&paths, None, &[change("MotionBlur", Some(0.0))]), Ok(None)));
        assert_eq!(game_files::backups(&file).len(), count);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn nothing_is_written_when_the_request_is_wrong_the_file_is_missing_or_the_game_runs() {
        let (home, paths, file) = world("verweigert", Some(7200));
        assert!(matches!(
            write(&paths, None, &[change("SysSpec_Fog", Some(9.0))]),
            Err(Failure::Refused(ChangeError::Level(_)))
        ));
        assert!(matches!(
            write(&paths, None, &[change("Unsinn", Some(1.0))]),
            Err(Failure::Refused(ChangeError::Unknown(_)))
        ));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), FILE);
        assert!(matches!(
            write(&paths, Some(&format!("{home}/gibt-es-nicht.xml")), &[change("VSync", Some(0.0))]),
            Err(Failure::Read)
        ));
        let _ = std::fs::remove_dir_all(&home);
        // The game runs: refused, not even a copy.
        let (home, paths, file) = world("laeuft", Some(5));
        assert!(matches!(
            write(&paths, None, &[change("MotionBlur", Some(0.0))]),
            Err(Failure::Write(WriteError::GameRuns))
        ));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), FILE);
        assert!(game_files::backups(&file).is_empty());
        // Nobody can tell (no Game.log known): refused too.
        let (home2, paths2, file2) = world("unklar", None);
        assert!(matches!(
            write(&paths2, Some(&file2), &[change("MotionBlur", Some(0.0))]),
            Err(Failure::Write(WriteError::GameUnknown))
        ));
        // No file at all.
        let empty = scratch("leer");
        let paths3 =
            Paths::new(Host::new(Flavor::Posix, &[("SC_BP_HOME", empty.as_str()), ("HOME", empty.as_str())], &empty));
        assert!(matches!(write(&paths3, None, &[]), Err(Failure::NoFile)));
        for dir in [home, home2, empty] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}
