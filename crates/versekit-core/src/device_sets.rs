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

//! Device sets (`device_set.py` of 4.12): a whole set-up under one name, "with pedals" and "without pedals".
//!
//! | Inside | Not inside |
//! |---|---|
//! | which id has which number (`js1`, `js2`, …) | the binding itself |
//! | dead zone and saturation of every axis | which key triggers which action |
//! | exponent and invert of every game axis | |
//!
//! ⚠⚠ **The binding is not part of a set on purpose.** The game saves complete profiles in `controls/mappings`
//! (`pp_rebindkeys load <name>`) and Verse-Kit puts them there as well. To keep both would be to keep two truths
//! about one thing. A set answers the other question: **how** the axes answer, not **what** lies on them.
//!
//! ⚠ **When a set is applied only what is there is written.** The point is changing set-ups. A device that is not
//! there is **skipped** and reported, instead of putting its setting on another device: a value on the wrong stick
//! is worse than a missing one.
//!
//! The file `joystick-saetze.json` is an own file of the player, **not** a file of the game: no guard of the game
//! stands before it, and it belongs to the files a backup of the player's data has to take.
//!
//! ## What is different from 4.12
//!
//! * The number a game axis is written under is the one the device has **now** (4.12 took the stored one, which
//!   hits the wrong block after a change of the numbers) and only what differs is written.
//! * The values of a set are written in one text, with one copy (4.12: one copy for each value).

use std::collections::HashSet;

use crate::axes::{self, DeviceBlock, GameBlock, GameProp, Prop, Values};
use crate::axes_write::{Setting, apply_text};
use crate::formats;
use crate::joysticks_write::{Done, Failure, commit, open};
use crate::jsonio::{self, Value};
use crate::paths::Paths;
use crate::pyfloat;
use crate::text::py_strip;

/// The file of the sets, among the own files of the program.
pub const FILE: &str = "joystick-saetze.json";

/// The longest name of a set. It is only shown, it never becomes a file name.
pub const NAME_MAX: usize = 40;

/// What a set remembers of a device.
#[derive(Clone, Debug, PartialEq)]
pub struct SetDevice {
    pub name: String,
    /// The axes in the order of the file with both values; `None` is "not set", and it **counts**: applying the
    /// set takes the value out. A set of only the set values would be a list of additions: whoever sets a
    /// saturation for "with pedals" and then applies "without pedals" would keep it.
    pub axes: Vec<(String, Values)>,
}

/// Exponent and invert of a game axis; only what is set.
#[derive(Clone, Debug, PartialEq)]
pub struct GameValues {
    pub exponent: Option<f64>,
    pub invert: Option<f64>,
}

/// A set.
#[derive(Clone, Debug, PartialEq)]
pub struct Set {
    pub name: String,
    /// `JJJJ-MM-TT HH:MM`, the time it was saved.
    pub stand: String,
    pub devices: Vec<(String, SetDevice)>,
    /// The number of a device in the game when the set was saved.
    pub numbers: Vec<(String, i64)>,
    pub game: Vec<(String, Vec<(String, GameValues)>)>,
}

/// May this be the name of a set? Gives the key of the text on a refusal.
///
/// # Errors
///
/// The key of the text: `s_gs_f_name_leer`, `s_gs_f_name_lang`, `s_gs_f_name_zeichen`.
pub fn check_name(name: &str) -> Result<(), &'static str> {
    let name = py_strip(name);
    if name.is_empty() {
        return Err("s_gs_f_name_leer");
    }
    if name.chars().count() > NAME_MAX {
        return Err("s_gs_f_name_lang");
    }
    if name.chars().any(|c| u32::from(c) < 32) {
        return Err("s_gs_f_name_zeichen");
    }
    Ok(())
}

fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Int(text) => text.parse().ok(),
        Value::Float(x) => Some(*x),
        _ => None,
    }
}

fn optional(value: Option<&Value>) -> Option<f64> {
    value.and_then(number)
}

fn float(value: Option<f64>) -> Value {
    value.map_or(Value::Null, Value::Float)
}

impl Set {
    /// A set from its place in the file; what does not fit is left out (a file the player edited by hand must not
    /// bring the page down).
    fn from_value(name: &str, value: &Value) -> Set {
        let object = |key: &str| value.get(key).and_then(Value::as_object).cloned().unwrap_or_default();
        let devices = object("geraete")
            .into_iter()
            .map(|(ident, entry)| {
                let axes = entry
                    .get("achsen")
                    .and_then(Value::as_object)
                    .map(|axes| {
                        axes.iter()
                            .map(|(axis, values)| {
                                let values = Values {
                                    deadzone: optional(values.get(Prop::Deadzone.name())),
                                    saturation: optional(values.get(Prop::Saturation.name())),
                                };
                                (axis.clone(), values)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let device =
                    SetDevice { name: entry.get("name").and_then(Value::as_str).unwrap_or("").to_string(), axes };
                (ident, device)
            })
            .collect();
        let numbers =
            object("nummern").into_iter().filter_map(|(ident, n)| number(&n).map(|n| (ident, n as i64))).collect();
        let game = object("spielachsen")
            .into_iter()
            .map(|(ident, axes)| {
                let list = axes
                    .as_object()
                    .map(|axes| {
                        axes.iter()
                            .map(|(axis, props)| {
                                let values = GameValues {
                                    exponent: optional(props.get(GameProp::Exponent.name())),
                                    invert: optional(props.get(GameProp::Invert.name())),
                                };
                                (axis.clone(), values)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                (ident, list)
            })
            .collect();
        Set {
            name: name.to_string(),
            stand: value.get("stand").and_then(Value::as_str).unwrap_or("").to_string(),
            devices,
            numbers,
            game,
        }
    }

    /// The set as it stands in the file (the name is the key, not a field).
    pub fn to_value(&self) -> Value {
        let devices = self
            .devices
            .iter()
            .map(|(ident, device)| {
                let axes = device
                    .axes
                    .iter()
                    .map(|(axis, v)| {
                        let pair = vec![
                            (Prop::Deadzone.name().to_string(), float(v.deadzone)),
                            (Prop::Saturation.name().to_string(), float(v.saturation)),
                        ];
                        (axis.clone(), Value::Object(pair))
                    })
                    .collect();
                let entry = vec![
                    ("name".to_string(), Value::str(device.name.clone())),
                    ("achsen".to_string(), Value::Object(axes)),
                ];
                (ident.clone(), Value::Object(entry))
            })
            .collect();
        let numbers = self.numbers.iter().map(|(ident, n)| (ident.clone(), Value::int(*n))).collect();
        let game = self
            .game
            .iter()
            .map(|(ident, axes)| {
                let axes = axes
                    .iter()
                    .map(|(axis, v)| {
                        let mut props = Vec::new();
                        if let Some(x) = v.exponent {
                            props.push((GameProp::Exponent.name().to_string(), Value::Float(x)));
                        }
                        if let Some(x) = v.invert {
                            props.push((GameProp::Invert.name().to_string(), Value::Float(x)));
                        }
                        (axis.clone(), Value::Object(props))
                    })
                    .collect();
                (ident.clone(), Value::Object(axes))
            })
            .collect();
        Value::Object(vec![
            ("stand".to_string(), Value::str(self.stand.clone())),
            ("geraete".to_string(), Value::Object(devices)),
            ("nummern".to_string(), Value::Object(numbers)),
            ("spielachsen".to_string(), Value::Object(game)),
        ])
    }
}

/// Everything in the file: the whole object, so that what the program does not know stays.
fn load_data(paths: &Paths) -> Vec<(String, Value)> {
    let mut data = match jsonio::read_json(&paths.app_file(FILE)) {
        Ok(Value::Object(map)) => map,
        _ => Vec::new(),
    };
    if !matches!(data.iter().find(|(key, _)| key == "saetze"), Some((_, Value::Object(_)))) {
        jsonio::map_set(&mut data, "saetze", Value::Object(Vec::new()));
    }
    data
}

fn sets_of(data: &[(String, Value)]) -> Vec<(String, Value)> {
    data.iter().find(|(key, _)| key == "saetze").and_then(|(_, v)| v.as_object().cloned()).unwrap_or_default()
}

/// All saved sets, the newest first (equal times keep the order of the file).
pub fn sets(paths: &Paths) -> Vec<Set> {
    let mut out: Vec<Set> =
        sets_of(&load_data(paths)).iter().map(|(name, value)| Set::from_value(name, value)).collect();
    out.sort_by(|a, b| b.stand.cmp(&a.stand));
    out
}

/// One set by its name, or `None`.
pub fn get(paths: &Paths, name: &str) -> Option<Set> {
    let name = py_strip(name);
    sets_of(&load_data(paths))
        .iter()
        .find(|(known, _)| known == name)
        .map(|(known, value)| Set::from_value(known, value))
}

/// The state as it is, without saving it: separate from saving so that the page can show what goes into the set.
/// Only the devices that count are in it (a dead block would come to life again at the next applying).
pub fn capture(blocks: &[DeviceBlock], game: &[GameBlock], stand: &str) -> Set {
    let mut devices: Vec<(String, SetDevice)> = Vec::new();
    for block in blocks.iter().filter(|b| !b.ident.is_empty() && b.active) {
        if devices.iter().any(|(known, _)| *known == block.ident) {
            continue;
        }
        // The block the page shows and applying writes into, so that the set keeps what the player saw.
        let Some(shown) = axes::shown_block(blocks, &block.ident) else { continue };
        devices.push((block.ident.clone(), SetDevice { name: shown.name.clone(), axes: shown.axes.clone() }));
    }
    let mut numbers: Vec<(String, i64)> = Vec::new();
    let mut spiel: Vec<(String, Vec<(String, GameValues)>)> = Vec::new();
    for block in game.iter().filter(|b| b.kind == "joystick" && !b.ident.is_empty()) {
        match numbers.iter_mut().find(|(known, _)| *known == block.ident) {
            Some(slot) => slot.1 = block.number,
            None => numbers.push((block.ident.clone(), block.number)),
        }
        let values: Vec<(String, GameValues)> = block
            .axes
            .iter()
            .filter(|(_, v)| v.exponent.is_some() || v.invert.is_some())
            .map(|(axis, v)| (axis.clone(), GameValues { exponent: v.exponent, invert: v.invert }))
            .collect();
        if !values.is_empty() {
            match spiel.iter_mut().find(|(known, _)| *known == block.ident) {
                Some(slot) => slot.1 = values,
                None => spiel.push((block.ident.clone(), values)),
            }
        }
    }
    Set { name: String::new(), stand: stand.to_string(), devices, numbers, game: spiel }
}

/// `JJJJ-MM-TT HH:MM` of now.
fn now_stamp() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|since| i64::try_from(since.as_secs()).ok())
        .unwrap_or(0);
    formats::local_time(seconds).map(|time| formats::strftime("%Y-%m-%d %H:%M", &time)).unwrap_or_default()
}

fn write_data(paths: &Paths, data: Vec<(String, Value)>) -> Result<(), Failure> {
    jsonio::save_json(&paths.app_file(FILE), &Value::Object(data), 1, false).map_err(|_| Failure::SetWrite)
}

/// Saves the state of the file under a name. Gives how many devices went into the set.
///
/// # Errors
///
/// [`Failure::Name`] for a bad name, [`Failure::SetTaken`] where the name is taken and `overwrite` is not set,
/// [`Failure::SetNothing`] where no device is there, [`Failure::SetWrite`] where the file could not be written.
pub fn save(
    paths: &Paths,
    name: &str,
    overwrite: bool,
    file: Option<&str>,
    folder: Option<&str>,
) -> Result<usize, Failure> {
    check_name(name).map_err(Failure::Name)?;
    let name = py_strip(name);
    let mut data = load_data(paths);
    let mut all = sets_of(&data);
    if all.iter().any(|(known, _)| known == name) && !overwrite {
        return Err(Failure::SetTaken);
    }
    let text = axes::file_text(paths, file, folder).unwrap_or_default();
    let alive: HashSet<String> = axes::valid_idents(paths, folder, file);
    let blocks = axes::device_axes_from_text(&text, &alive);
    let game = axes::game_axes_from_text(&text, &alive);
    let set = capture(&blocks, &game, &now_stamp());
    if set.devices.is_empty() {
        return Err(Failure::SetNothing);
    }
    jsonio::map_set(&mut all, name, set.to_value());
    jsonio::map_set(&mut data, "saetze", Value::Object(all));
    write_data(paths, data)?;
    Ok(set.devices.len())
}

/// Takes a set away.
///
/// # Errors
///
/// [`Failure::SetUnknown`], [`Failure::SetWrite`].
pub fn delete(paths: &Paths, name: &str) -> Result<(), Failure> {
    let name = py_strip(name);
    let mut data = load_data(paths);
    let mut all = sets_of(&data);
    let Some(at) = all.iter().position(|(known, _)| known == name) else { return Err(Failure::SetUnknown) };
    all.remove(at);
    jsonio::map_set(&mut data, "saetze", Value::Object(all));
    write_data(paths, data)
}

/// What applying a set would do: the settings that differ, and the names of the devices of the set that are not
/// there now. A value that is `None` in the set takes the value out and is a change if something stands there.
pub fn plan(set: &Set, blocks: &[DeviceBlock], game: &[GameBlock]) -> (Vec<Setting>, Vec<String>) {
    let mut settings = Vec::new();
    let mut missing = Vec::new();
    for (ident, device) in &set.devices {
        let Some(target) = axes::shown_block(blocks, ident) else {
            missing.push(if device.name.is_empty() { ident.chars().take(8).collect() } else { device.name.clone() });
            continue;
        };
        for (axis, values) in &device.axes {
            for prop in Prop::ALL {
                let now = target.axis(axis).and_then(|v| v.get(prop));
                if now != values.get(prop) {
                    settings.push(Setting::Device {
                        ident: ident.clone(),
                        axis: axis.clone(),
                        prop,
                        value: values.get(prop),
                    });
                }
            }
        }
    }
    // The game axes last: they hang on the number, and they should not write when the axes failed. The number is the
    // one the device has now.
    for (ident, axes_of) in &set.game {
        if axes::shown_block(blocks, ident).is_none() {
            continue;
        }
        let Some(current) = game.iter().find(|b| b.kind == "joystick" && b.ident == *ident) else { continue };
        for (axis, values) in axes_of {
            let now = current.axes.iter().find(|(known, _)| known == axis).map(|(_, v)| v);
            for (prop, wanted, have) in [
                (GameProp::Exponent, values.exponent, now.and_then(|v| v.exponent)),
                (GameProp::Invert, values.invert, now.and_then(|v| v.invert)),
            ] {
                // The exponent is written with six digits: what looks alike in the file is no difference (else a
                // value like 1.10000002 would be a change for ever that writes nothing).
                let differs = match prop {
                    GameProp::Exponent => have.map(pyfloat::general) != wanted.map(pyfloat::general),
                    GameProp::Invert => wanted != have,
                };
                if wanted.is_some() && differs {
                    settings.push(Setting::Game { number: current.number, axis: axis.clone(), prop, value: wanted });
                }
            }
        }
    }
    (settings, missing)
}

/// [`plan`] worked into the text; gives the new text and how many values were written.
///
/// # Errors
///
/// [`Failure::NothingToDo`] where everything stands in the text already, and the refusals of the settings.
pub fn apply_to_text(set: &Set, text: &str, alive: &HashSet<String>) -> Result<(String, usize), Failure> {
    let blocks = axes::device_axes_from_text(text, alive);
    let game = axes::game_axes_from_text(text, alive);
    let (settings, _) = plan(set, &blocks, &game);
    if settings.is_empty() {
        return Err(Failure::NothingToDo);
    }
    apply_text(text, alive, &settings)
}

/// Applies a set to the file: one text, one copy, one writing.
///
/// # Errors
///
/// [`Failure::SetUnknown`], [`Failure::NoFile`], [`Failure::NothingToDo`] and the refusals of the writing.
pub fn apply(paths: &Paths, name: &str, file: Option<&str>, folder: Option<&str>) -> Result<Done, Failure> {
    let set = get(paths, name).ok_or(Failure::SetUnknown)?;
    let read = open(paths, file, folder)?;
    let alive = axes::valid_idents(paths, folder, Some(&read.path));
    let (new, count) = apply_to_text(&set, &read.text, &alive)?;
    commit(paths, &read, &new, count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::legacy::ENV_HOME;
    use crate::paths::Host;
    use crate::pypath::Flavor;

    const LEFT: &str = "03F33344-0000-0000-0000-504944564944";
    const RIGHT: &str = "43F43344-0000-0000-0000-504944564944";

    fn doc(left_dz: &str, left_exp: &str, with_right: bool) -> String {
        let right = if with_right {
            format!(
                "<deviceoptions name=\"RIGHT Stick  {{{RIGHT}}}\"><option input=\"x\" deadzone=\"0.3\"/></deviceoptions><options type=\"joystick\" instance=\"2\" Product=\"RIGHT Stick  {{{RIGHT}}}\"><flight_move_roll exponent=\"2\" invert=\"1\"/></options>"
            )
        } else {
            String::new()
        };
        format!(
            "<ActionMaps><ActionProfiles><deviceoptions name=\"LEFT Stick  {{{LEFT}}}\"><option input=\"x\" deadzone=\"{left_dz}\"/><option input=\"y\" saturation=\"0.8\"/></deviceoptions>{right}<options type=\"joystick\" instance=\"1\" Product=\"LEFT Stick  {{{LEFT}}}\"><flight_move_pitch exponent=\"{left_exp}\"/></options></ActionProfiles></ActionMaps>"
        )
    }

    fn alive(both: bool) -> HashSet<String> {
        let mut set: HashSet<String> = [LEFT.to_string()].into_iter().collect();
        if both {
            set.insert(RIGHT.to_string());
        }
        set
    }

    fn captured(text: &str, both: bool) -> Set {
        let blocks = axes::device_axes_from_text(text, &alive(both));
        let game = axes::game_axes_from_text(text, &alive(both));
        capture(&blocks, &game, "2026-10-23 03:00")
    }

    #[test]
    fn the_state_is_captured_with_what_is_not_set_and_the_numbers() {
        let set = captured(&doc("0.152", "1.5", true), true);
        assert_eq!(set.devices.iter().map(|(i, _)| i.as_str()).collect::<Vec<_>>(), [LEFT, RIGHT]);
        let left = &set.devices[0].1;
        assert_eq!(left.name, "LEFT Stick");
        // x: a dead zone, no saturation; y: a saturation, no dead zone: both fields stand, `None` counts.
        assert_eq!(left.axes[0], ("x".to_string(), Values { deadzone: Some(0.152), saturation: None }));
        assert_eq!(left.axes[1], ("y".to_string(), Values { deadzone: None, saturation: Some(0.8) }));
        // In the order of the blocks in the file (the right one stands first there).
        assert_eq!(set.numbers, [(RIGHT.to_string(), 2), (LEFT.to_string(), 1)]);
        assert_eq!(set.game[1].1[0].1, GameValues { exponent: Some(1.5), invert: None });
        assert_eq!(set.game[0].1[0].1, GameValues { exponent: Some(2.0), invert: Some(1.0) });
        // A dead device is not in it.
        let only_left = captured(&doc("0.152", "1.5", true), false);
        assert_eq!(only_left.devices.len(), 1);
    }

    #[test]
    fn the_file_form_is_the_one_of_4_12_and_reads_back() {
        let set = captured(&doc("0.152", "1.5", true), true);
        let text = jsonio::dumps(&set.to_value(), None, false);
        assert!(text.starts_with("{\"stand\": \"2026-10-23 03:00\", \"geraete\": {\"03F33344"), "{text}");
        assert!(text.contains("\"achsen\": {\"x\": {\"deadzone\": 0.152, \"saturation\": null}"), "{text}");
        assert!(text.contains("\"nummern\": {\"43F43344-0000-0000-0000-504944564944\": 2, \"03F33344-0000-0000-0000-504944564944\": 1}"), "{text}");
        assert!(text.contains("\"flight_move_pitch\": {\"exponent\": 1.5}}"), "{text}");
        assert!(text.contains("\"flight_move_roll\": {\"exponent\": 2.0, \"invert\": 1.0}"), "{text}");
        let back = Set::from_value("", &jsonio::loads(&text).expect("json"));
        assert_eq!(back, Set { name: String::new(), ..set });
    }

    #[test]
    fn applying_writes_only_what_differs_takes_out_what_the_set_does_not_have_and_skips_missing_devices() {
        let set = captured(&doc("0.152", "1.5", true), true);
        // Now: another dead zone, the exponent changed, no saturation on y, and the right stick is not there.
        let text = doc("0.2", "1", false).replace(
            "<option input=\"y\" saturation=\"0.8\"/>",
            "<option input=\"y\" saturation=\"0.5\"/><option input=\"rotz\" saturation=\"0.4\"/>",
        );
        let blocks = axes::device_axes_from_text(&text, &alive(false));
        let game = axes::game_axes_from_text(&text, &alive(false));
        let (settings, missing) = plan(&set, &blocks, &game);
        assert_eq!(missing, ["RIGHT Stick"]);
        let wanted: Vec<String> = settings
            .iter()
            .map(|s| match s {
                Setting::Device { axis, prop, value, .. } => format!("{axis} {} {value:?}", prop.name()),
                Setting::Game { number, axis, prop, value } => format!("{number} {axis} {} {value:?}", prop.name()),
            })
            .collect();
        assert_eq!(
            wanted,
            ["x deadzone Some(0.152)", "y saturation Some(0.8)", "1 flight_move_pitch exponent Some(1.5)"],
            "rotz is not in the set: the set does not touch it"
        );
        let (new, count) = apply_to_text(&set, &text, &alive(false)).expect("applied");
        assert_eq!(count, 3);
        assert!(
            new.contains("deadzone=\"0.152\"")
                && new.contains("exponent=\"1.5\"")
                && new.contains("saturation=\"0.8\""),
            "{new}"
        );
        // The same again: nothing to do.
        assert!(matches!(apply_to_text(&set, &new, &alive(false)), Err(Failure::NothingToDo)));
    }

    #[test]
    fn an_exponent_that_looks_alike_in_the_file_is_no_difference() {
        // The file is written with six digits: 1.10000002 and 1.1 are one and the same to it, and a set that
        // calls this a change would offer a writing that writes nothing, for ever.
        let set = captured(&doc("0.152", "1.10000002", false), false);
        let now = doc("0.152", "1.1", false);
        let blocks = axes::device_axes_from_text(&now, &alive(false));
        let game = axes::game_axes_from_text(&now, &alive(false));
        assert!(plan(&set, &blocks, &game).0.is_empty());
        assert!(matches!(apply_to_text(&set, &now, &alive(false)), Err(Failure::NothingToDo)));
        // A real difference still counts.
        let other = doc("0.152", "1.2", false);
        let blocks = axes::device_axes_from_text(&other, &alive(false));
        let game = axes::game_axes_from_text(&other, &alive(false));
        assert_eq!(plan(&set, &blocks, &game).0.len(), 1);
    }

    #[test]
    fn of_two_live_blocks_of_one_id_the_set_keeps_the_one_the_page_shows() {
        // Two names for one id, both live: the page shows (and applying writes) the first by name.
        let text = doc("0.152", "1.5", false).replace(
            "</ActionProfiles>",
            &format!("<deviceoptions name=\"AAA Stick  {{{LEFT}}}\"><option input=\"x\" deadzone=\"0.4\"/></deviceoptions></ActionProfiles>"),
        );
        let blocks = axes::device_axes_from_text(&text, &alive(false));
        let shown = axes::shown_block(&blocks, LEFT).expect("shown");
        let set = captured(&text, false);
        assert_eq!(set.devices[0].1.name, shown.name);
        assert_eq!(set.devices[0].1.axes, shown.axes);
    }

    #[test]
    fn a_value_the_set_does_not_have_is_taken_out_where_it_stands() {
        // "Without saturation" is a state too: a set saved with y holding no saturation takes it out.
        let set = captured(
            &doc("0.152", "1.5", false)
                .replace("<option input=\"y\" saturation=\"0.8\"/>", "<option input=\"y\" deadzone=\"0.1\"/>"),
            false,
        );
        let now = doc("0.152", "1.5", false)
            .replace("<option input=\"y\" saturation=\"0.8\"/>", "<option input=\"y\" saturation=\"0.9\"/>");
        let (new, count) = apply_to_text(&set, &now, &alive(false)).expect("applied");
        assert_eq!(count, 2, "the dead zone of y is added, the saturation goes: {new}");
        assert!(!new.contains("saturation") && new.contains("input=\"y\" deadzone=\"0.1\""), "{new}");
    }

    #[test]
    fn the_name_of_a_set_is_checked_and_a_game_axis_uses_the_number_the_device_has_now() {
        for (name, key) in [
            ("", "s_gs_f_name_leer"),
            ("   ", "s_gs_f_name_leer"),
            (&"x".repeat(41), "s_gs_f_name_lang"),
            ("a\tb", "s_gs_f_name_zeichen"),
        ] {
            assert_eq!(check_name(name), Err(key), "{name:?}");
        }
        assert!(check_name(&"ä".repeat(40)).is_ok() && check_name(" Mit Pedalen ").is_ok());
        // The set was saved when the left stick was number 1; now it is number 3: the exponent goes to 3.
        let set = captured(&doc("0.152", "1.5", false), false);
        let now = doc("0.152", "1", false).replace("instance=\"1\"", "instance=\"3\"");
        let (new, _) = apply_to_text(&set, &now, &alive(false)).expect("applied");
        assert!(new.contains("instance=\"3\" Product=\"LEFT Stick  {03F33344-0000-0000-0000-504944564944}\"><flight_move_pitch exponent=\"1.5\"/>"), "{new}");
    }

    #[test]
    fn the_sets_are_saved_listed_newest_first_replaced_and_deleted_in_the_file_of_the_program() {
        let home = std::env::temp_dir().join(format!("versekit-sets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("folder");
        let text = home.to_string_lossy().replace('\\', "/");
        let paths = Paths::new(Host::new(Flavor::native(), &[(ENV_HOME, text.as_str())], &text));
        let maps = format!("{text}/actionmaps.xml");
        std::fs::write(&maps, doc("0.152", "1.5", true)).expect("maps");
        // Ids are alive when the log names them or the binding gives them a number: here the Product entries do.
        assert!(sets(&paths).is_empty());
        assert_eq!(save(&paths, "  Mit Pedalen ", false, Some(&maps), None).expect("saved"), 2);
        assert!(matches!(save(&paths, "Mit Pedalen", false, Some(&maps), None), Err(Failure::SetTaken)));
        assert!(matches!(save(&paths, "", false, Some(&maps), None), Err(Failure::Name("s_gs_f_name_leer"))));
        assert_eq!(save(&paths, "Mit Pedalen", true, Some(&maps), None).expect("replaced"), 2);
        std::fs::write(&maps, doc("0.2", "1", false)).expect("maps");
        assert_eq!(save(&paths, "Ohne", false, Some(&maps), None).expect("saved"), 1);
        let names: Vec<String> = sets(&paths).into_iter().map(|s| s.name).collect();
        assert_eq!(names.len(), 2);
        assert_eq!(get(&paths, " Ohne ").map(|s| s.devices.len()), Some(1));
        delete(&paths, "Ohne").expect("deleted");
        assert!(matches!(delete(&paths, "Ohne"), Err(Failure::SetUnknown)));
        assert_eq!(sets(&paths).len(), 1);
        // A file the player broke does not bring anything down.
        std::fs::write(paths.app_file(FILE), "{kaputt").expect("broken");
        assert!(sets(&paths).is_empty());
        let _ = std::fs::remove_dir_all(home);
    }
}
