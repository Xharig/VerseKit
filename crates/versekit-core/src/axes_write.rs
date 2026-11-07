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

//! Writing dead zone, saturation, sensitivity and invert into the `actionmaps.xml` (`curves.apply`,
//! `curves.apply_to_game` of 4.12). Like every change of that file: **as text, at the places it concerns**, over
//! `game_files::write` (the game must be quiet, a copy first), and the result must parse before it is written.
//!
//! ⚠⚠ **One click is one writing and one copy.** 4.12 wrote each value alone and made a copy for each; a gear set
//! with ten values left ten copies in the same second, and the oldest of ten kept ones, the state before the first
//! change, was gone. Here a list of settings is worked into one text and written once.
//!
//! ⚠ **Through the id, never the name**, and into the block the page **shows** (see [`axes::shown_block`]).
//!
//! ⚠ **All doubles are collected.** The game writes every saturation twice and adds entries; changing only the first
//! would leave the second with the old value, and the last wins. Exactly **one** entry is left for an axis and a
//! property.
//!
//! ⚠ **`repr`, not `%g`.** The game writes the full width (`0.098999992`); `%g` would shorten it and change a value
//! the player did not touch (a gear set announced one change and made ten).

use std::collections::HashSet;

use roxmltree::Node;

use crate::axes::{self, GameProp, Prop};
use crate::joysticks;
use crate::joysticks_write::{
    Done, Edit, Failure, append_child, apply, commit, escape, is_element, open, parse, whole_line,
};
use crate::paths::Paths;
use crate::pyfloat;

/// One setting to write.
#[derive(Clone, Debug, PartialEq)]
pub enum Setting {
    /// Dead zone or saturation of an axis of a device; `None` takes the entry out.
    Device { ident: String, axis: String, prop: Prop, value: Option<f64> },
    /// Sensitivity or invert of a game axis of the device with this number; `None` takes the attribute out.
    Game { number: i64, axis: String, prop: GameProp, value: Option<f64> },
}

fn check_range(value: Option<f64>, range: (f64, f64)) -> Result<(), Failure> {
    match value {
        Some(v) if !(range.0 <= v && v <= range.1) => Err(Failure::Range),
        _ => Ok(()),
    }
}

/// Whether `name` is fit to be an element name in the text: letters, digits, `_`, `.` and `-`, not starting with a
/// digit.
fn is_xml_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// The range of an attribute with the blanks that stand before it.
fn attribute_with_blanks(text: &str, node: Node, name: &str) -> Option<std::ops::Range<usize>> {
    let range = node.attribute_node(name)?.range();
    let mut start = range.start;
    while start > 0 && matches!(text.as_bytes()[start - 1], b' ' | b'\t') {
        start -= 1;
    }
    Some(start..range.end)
}

/// Dead zone or saturation of an axis of a device put in `text`.
///
/// # Errors
///
/// [`Failure::Range`] outside 0 to 1, [`Failure::Ident`] without an id, [`Failure::Unreadable`] where the text does
/// not parse, [`Failure::Device`] where no live block has the id.
pub fn set_device_text(
    text: &str,
    alive: &HashSet<String>,
    ident: &str,
    axis: &str,
    prop: Prop,
    value: Option<f64>,
) -> Result<String, Failure> {
    check_range(value, (0.0, 1.0))?;
    let ident = ident.to_uppercase();
    if ident.is_empty() {
        return Err(Failure::Ident);
    }
    let document = parse(text).ok_or(Failure::Unreadable)?;
    let blocks = axes::device_axes_from_text(text, alive);
    let block = axes::shown_block(&blocks, &ident).ok_or(Failure::Device)?;
    let node = document
        .descendants()
        .find(|n| is_element(*n, "deviceoptions") && n.range() == block.range)
        .ok_or(Failure::Device)?;
    let mut entries = node.children().filter(|n| {
        is_element(*n, "option") && n.attribute("input").unwrap_or("") == axis && n.attribute(prop.name()).is_some()
    });
    let keep = entries.next();
    let others: Vec<Node> = entries.collect();
    // ⚠ 4.12 cut every further entry out whole, also one that carries the other property too (the dead zone
    // next to a saturation was lost with it). Here only the attribute of this property goes from such an entry.
    let mut edits: Vec<Edit> = Vec::new();
    for other in &others {
        let carries_more = other.attributes().any(|a| a.name() != "input" && a.name() != prop.name());
        if !carries_more {
            edits.push(Edit { range: whole_line(text, other.range()), with: String::new() });
        } else if let Some(range) = attribute_with_blanks(text, *other, prop.name()) {
            edits.push(Edit { range, with: String::new() });
        }
    }
    match (value, keep) {
        (Some(v), Some(keep)) => {
            if let Some(attr) = keep.attribute_node(prop.name()) {
                edits.push(Edit { range: attr.range_value(), with: pyfloat::repr(v) });
            }
        }
        (Some(v), None) => {
            let child = format!("<option input=\"{}\" {}=\"{}\"/>", escape(axis), prop.name(), pyfloat::repr(v));
            edits.push(append_child(text, node, &child));
        }
        (None, Some(keep)) => {
            // Only the one attribute goes; an entry that carries the other property too stays.
            let others_left = keep.attributes().filter(|a| a.name() != "input" && a.name() != prop.name()).count();
            if others_left == 0 {
                edits.push(Edit { range: whole_line(text, keep.range()), with: String::new() });
            } else if let Some(range) = attribute_with_blanks(text, keep, prop.name()) {
                edits.push(Edit { range, with: String::new() });
            }
        }
        (None, None) => {}
    }
    Ok(apply(text, edits))
}

/// Sensitivity or invert of a game axis of the device with this number put in `text`.
///
/// # Errors
///
/// [`Failure::Range`] outside the range of the property, [`Failure::Unreadable`] where the text does not parse,
/// [`Failure::Device`] where no `<options type="joystick">` has the number.
pub fn set_game_text(
    text: &str,
    number: i64,
    axis: &str,
    prop: GameProp,
    value: Option<f64>,
) -> Result<String, Failure> {
    check_range(value, prop.range())?;
    // The name goes into the text as an element name: whatever is no name is refused (a set file by hand could
    // bring a name that opens a foreign element).
    if !is_xml_name(axis) {
        return Err(Failure::Property);
    }
    let document = parse(text).ok_or(Failure::Unreadable)?;
    let target = document
        .descendants()
        .filter(|n| is_element(*n, "options") && n.attribute("type").unwrap_or("").to_lowercase() == "joystick")
        .find(|n| match n.attribute("instance").filter(|v| !v.is_empty()) {
            None => number == 0,
            Some(v) => joysticks::py_int_text(v) == Some(number),
        })
        .ok_or(Failure::Device)?;
    // The last one counts when the page reads (the last wins), so that is the one that is written.
    let child = target.children().rfind(|n| is_element(*n, axis));
    let shown = |v: f64| match prop {
        GameProp::Invert => format!("{}", v.trunc() as i64),
        GameProp::Exponent => pyfloat::general(v),
    };
    let mut edits = Vec::new();
    match (value, child) {
        (None, Some(child)) => {
            let attributes_left = child.attributes().filter(|a| a.name() != prop.name()).count();
            let has_children = child.children().any(|n| n.is_element());
            if attributes_left == 0 && !has_children {
                edits.push(Edit { range: whole_line(text, child.range()), with: String::new() });
            } else if let Some(range) = attribute_with_blanks(text, child, prop.name()) {
                edits.push(Edit { range, with: String::new() });
            }
        }
        (None, None) => {}
        (Some(v), None) => {
            let element = format!("<{axis} {}=\"{}\"/>", prop.name(), shown(v));
            edits.push(append_child(text, target, &element));
        }
        (Some(v), Some(child)) => {
            if let Some(attr) = child.attribute_node(prop.name()) {
                edits.push(Edit { range: attr.range_value(), with: shown(v) });
            } else {
                // After the last attribute, or after the name where it has none.
                let at = child
                    .attributes()
                    .map(|a| a.range().end)
                    .max()
                    .unwrap_or(child.range().start + 1 + child.tag_name().name().len());
                edits.push(Edit { range: at..at, with: format!(" {}=\"{}\"", prop.name(), shown(v)) });
            }
        }
    }
    Ok(apply(text, edits))
}

/// A list of settings worked into `text`, one after the other (every one is looked for in the text the one before
/// has left). Gives the new text and how many settings were made.
///
/// # Errors
///
/// The first refusal of a setting; nothing is made then.
pub fn apply_text(text: &str, alive: &HashSet<String>, settings: &[Setting]) -> Result<(String, usize), Failure> {
    let mut current = text.to_string();
    for setting in settings {
        current = match setting {
            Setting::Device { ident, axis, prop, value } => {
                set_device_text(&current, alive, ident, axis, *prop, *value)?
            }
            Setting::Game { number, axis, prop, value } => set_game_text(&current, *number, axis, *prop, *value)?,
        };
    }
    Ok((current, settings.len()))
}

/// What it takes to make the axes of one device like those of another (`align` of 4.12): dead zone and saturation
/// of every axis both have, **only where they differ** (a blind writing of all of them would leave a copy for each),
/// a missing value of the source taking the value of the target out. Nothing is made up for an axis only one
/// device has: a pair of pedals has no `rotx`, and an invented value is worse than none.
///
/// # Errors
///
/// [`Failure::Ident`] without an id, [`Failure::Device`] for the same device twice or one that has no block,
/// [`Failure::NothingShared`] where the devices have no axis in common.
pub fn align_settings(blocks: &[axes::DeviceBlock], from_ident: &str, to_ident: &str) -> Result<Vec<Setting>, Failure> {
    let (from, to) = (from_ident.to_uppercase(), to_ident.to_uppercase());
    if from.is_empty() || to.is_empty() {
        return Err(Failure::Ident);
    }
    if from == to {
        return Err(Failure::Device);
    }
    let (Some(source), Some(target)) = (axes::shown_block(blocks, &from), axes::shown_block(blocks, &to)) else {
        return Err(Failure::Device);
    };
    let shared: Vec<&str> =
        axes::AXES.iter().copied().filter(|axis| source.axis(axis).is_some() && target.axis(axis).is_some()).collect();
    if shared.is_empty() {
        return Err(Failure::NothingShared);
    }
    let mut out = Vec::new();
    for axis in shared {
        let (wanted, now) =
            (source.axis(axis).copied().unwrap_or_default(), target.axis(axis).copied().unwrap_or_default());
        for prop in Prop::ALL {
            if wanted.get(prop) != now.get(prop) {
                out.push(Setting::Device { ident: to.clone(), axis: axis.to_string(), prop, value: wanted.get(prop) });
            }
        }
    }
    Ok(out)
}

/// [`align_settings`] worked into the text; gives the new text and how many values were written (0 where the two
/// are alike already: the text is the same then).
///
/// # Errors
///
/// See [`align_settings`] and [`apply_text`].
pub fn align_text(
    text: &str,
    alive: &HashSet<String>,
    from_ident: &str,
    to_ident: &str,
) -> Result<(String, usize), Failure> {
    let blocks = axes::device_axes_from_text(text, alive);
    let settings = align_settings(&blocks, from_ident, to_ident)?;
    apply_text(text, alive, &settings)
}

/// Makes the axes of one device like those of another: one text, one copy, one writing.
///
/// # Errors
///
/// See [`align_text`]; [`Failure::Same`] where the devices are alike already (nothing is written, no copy is made;
/// 4.12 said "0 values copied, a copy lies beside it" there, and there was none).
pub fn align(
    paths: &Paths,
    from_ident: &str,
    to_ident: &str,
    file: Option<&str>,
    folder: Option<&str>,
) -> Result<Done, Failure> {
    let read = open(paths, file, folder)?;
    let alive = axes::valid_idents(paths, folder, Some(&read.path));
    let (new, count) = align_text(&read.text, &alive, from_ident, to_ident)?;
    commit(paths, &read, &new, count)
}

/// The blocks nobody asks for any more: their id belongs to no connected and no bound device. A block without an id
/// (mouse, keyboard) is not dead, only not to be matched.
///
/// ⚠ It goes by the **id**, as 4.12 does, and not by the field `active` of the block: the older name of a device
/// that is alive stays out of this (it is not "old", only under an old name).
pub fn dead_blocks(blocks: &[axes::DeviceBlock]) -> Vec<&axes::DeviceBlock> {
    blocks.iter().filter(|block| !block.ident.is_empty() && !block.ident_alive).collect()
}

/// The text with the dead blocks cut out, each with its whole line (so no empty line stays), and how many went.
/// Nothing else is touched: the indent and the comments of the game stay as they are.
///
/// # Errors
///
/// [`Failure::Unreadable`] where the text does not parse, [`Failure::NothingToClean`] where no block is dead.
pub fn clean_up_text(text: &str, alive: &HashSet<String>) -> Result<(String, usize), Failure> {
    parse(text).ok_or(Failure::Unreadable)?;
    let blocks = axes::device_axes_from_text(text, alive);
    let dead = dead_blocks(&blocks);
    if dead.is_empty() {
        return Err(Failure::NothingToClean);
    }
    let edits: Vec<Edit> =
        dead.iter().map(|block| Edit { range: whole_line(text, block.range.clone()), with: String::new() }).collect();
    Ok((apply(text, edits), dead.len()))
}

/// How many blocks [`clean_up`] would take out (for the question); 0 where there is no file.
pub fn count_dead(paths: &Paths, file: Option<&str>, folder: Option<&str>) -> usize {
    let Some(text) = axes::file_text(paths, file, folder) else { return 0 };
    let alive = axes::valid_idents(paths, folder, file);
    // Without any live id every block would count as dead: the state of the devices is not known then.
    if alive.is_empty() {
        return 0;
    }
    dead_blocks(&axes::device_axes_from_text(&text, &alive)).len()
}

/// Takes the dead blocks out of the file: one copy, only while the game is quiet.
///
/// # Errors
///
/// [`Failure::NoFile`], [`Failure::NothingToClean`] and the refusals of the writing.
pub fn clean_up(paths: &Paths, file: Option<&str>, folder: Option<&str>) -> Result<Done, Failure> {
    let read = open(paths, file, folder)?;
    let alive = axes::valid_idents(paths, folder, Some(&read.path));
    // ⚠ Without any live id (no log, nothing bound) every block would count as dead and the whole set-up of the
    // dead zones would go: that is refused, as if there were nothing to clear.
    if alive.is_empty() {
        return Err(Failure::NothingToClean);
    }
    let (new, count) = clean_up_text(&read.text, &alive)?;
    commit(paths, &read, &new, count)
}

/// The settings that carry the values of an overtaken block over to the live one: every value that is set in the
/// old block, written with the width the game gave it.
pub fn adopt_settings(case: &axes::Adoptable) -> Vec<Setting> {
    case.values
        .iter()
        .map(|(axis, prop, old, _now)| Setting::Device {
            ident: case.new.ident.clone(),
            axis: axis.clone(),
            prop: *prop,
            value: Some(*old),
        })
        .collect()
}

/// Writes a list of settings: one text, one copy, one writing.
///
/// # Errors
///
/// [`Failure::NoFile`], the refusals of the settings, [`Failure::Same`] where nothing changes, and the refusals of
/// the writing (the game runs, the file changed, the text would not parse).
pub fn apply_settings(
    paths: &Paths,
    settings: &[Setting],
    file: Option<&str>,
    folder: Option<&str>,
) -> Result<Done, Failure> {
    let read = open(paths, file, folder)?;
    let alive = axes::valid_idents(paths, folder, Some(&read.path));
    let (new, count) = apply_text(&read.text, &alive, settings)?;
    commit(paths, &read, &new, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEFT: &str = "03F33344-0000-0000-0000-504944564944";
    const OLD: &str = "83F33344-0000-0000-0000-504944564944";

    fn alive() -> HashSet<String> {
        [LEFT.to_string()].into_iter().collect()
    }

    fn file() -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- kept -->\n<ActionMaps>\n <ActionProfiles version=\"1\">\n  <deviceoptions name=\"LEFT VPC Stick  {{{LEFT}}}\">\n   <option input=\"x\" deadzone=\"0.152\"/>\n   <option input=\"y\" deadzone=\"0.15000001\" saturation=\"0.8\"/>\n   <option input=\"x\" saturation=\"0.84799999\"/>\n   <option input=\"x\" saturation=\"0.84799999\"/>\n  </deviceoptions>\n  <deviceoptions name=\"LEFT VPC Stick  {{{OLD}}}\">\n   <option input=\"x\" deadzone=\"0.2\"/>\n  </deviceoptions>\n  <options type=\"joystick\" instance=\"1\" Product=\"LEFT VPC Stick  {{{LEFT}}}\">\n   <flight_move_pitch exponent=\"1.5\" invert=\"1\"/>\n   <flight_move_yaw invert=\"0\"/>\n  </options>\n  <options type=\"joystick\" instance=\"2\"/>\n </ActionProfiles>\n</ActionMaps>\n"
        )
    }

    fn device(text: &str, axis: &str, prop: Prop, value: Option<f64>) -> Result<String, Failure> {
        set_device_text(text, &alive(), LEFT, axis, prop, value)
    }

    fn values(text: &str, axis: &str) -> axes::Values {
        let blocks = axes::device_axes_from_text(text, &alive());
        axes::shown_block(&blocks, LEFT).and_then(|b| b.axis(axis).copied()).unwrap_or_default()
    }

    #[test]
    fn a_value_replaces_the_first_entry_and_takes_the_doubles_out_nothing_else_moves() {
        let text = file();
        let new = device(&text, "x", Prop::Saturation, Some(0.9)).expect("written");
        assert_eq!(new, text.replace("   <option input=\"x\" saturation=\"0.84799999\"/>\n   <option input=\"x\" saturation=\"0.84799999\"/>\n", "   <option input=\"x\" saturation=\"0.9\"/>\n"));
        assert!(new.contains("<!-- kept -->") && new.starts_with("<?xml"), "the comment and the declaration stay");
        assert_eq!(values(&new, "x").saturation, Some(0.9));
        assert_eq!(values(&new, "x").deadzone, Some(0.152), "the other property is untouched");
    }

    #[test]
    fn the_full_width_the_game_wrote_is_not_shortened_where_the_player_did_not_touch_it() {
        let text = file();
        // The game's value stays as it stands in the text; only the touched one is written, with `repr`.
        let new = device(&text, "y", Prop::Deadzone, Some(0.098999992)).expect("written");
        assert!(new.contains("deadzone=\"0.098999992\""), "{new}");
        assert!(new.contains("saturation=\"0.84799999\""), "an untouched value keeps its width");
        let new = device(&text, "x", Prop::Deadzone, Some(1e-5)).expect("written");
        assert!(new.contains("deadzone=\"1e-05\""), "the way Python writes it: {new}");
        let new = device(&text, "x", Prop::Deadzone, Some(1.0)).expect("written");
        assert!(new.contains("deadzone=\"1.0\""), "{new}");
    }

    #[test]
    fn a_missing_entry_is_added_at_the_end_of_the_block_with_the_indent_of_its_neighbours() {
        let text = file();
        let new = device(&text, "rotz", Prop::Saturation, Some(0.5)).expect("written");
        let at = new.find("<option input=\"rotz\"").expect("added");
        assert!(new[..at].ends_with("\n   "), "indented like the others: {new}");
        assert!(new.contains("saturation=\"0.5\"/>\n  </deviceoptions>"), "{new}");
        assert_eq!(values(&new, "rotz").saturation, Some(0.5));
        // A new entry is written the way Python writes the number, as an entry that is there is.
        let one = device(&text, "rotz", Prop::Saturation, Some(1.0)).expect("written");
        assert!(one.contains("<option input=\"rotz\" saturation=\"1.0\"/>"), "{one}");
        let small = device(&text, "rotz", Prop::Deadzone, Some(1e-5)).expect("written");
        assert!(small.contains("<option input=\"rotz\" deadzone=\"1e-05\"/>"), "{small}");
        // The same again changes nothing.
        assert!(
            matches!(set_device_text(&new, &alive(), LEFT, "rotz", Prop::Saturation, Some(0.5)), Ok(again) if again == new)
        );
    }

    #[test]
    fn a_value_taken_out_removes_the_attribute_and_an_entry_with_nothing_left() {
        let text = file();
        // x saturation stands twice alone: both entries go.
        let new = device(&text, "x", Prop::Saturation, None).expect("taken out");
        assert!(!new.contains("0.84799999"), "{new}");
        assert_eq!(values(&new, "x").saturation, None);
        assert_eq!(values(&new, "x").deadzone, Some(0.152));
        // y has both properties in one entry: only the one attribute goes, the entry stays.
        let new = device(&text, "y", Prop::Saturation, None).expect("taken out");
        assert!(new.contains("<option input=\"y\" deadzone=\"0.15000001\"/>"), "{new}");
        // Nothing to take out: the text is the same.
        assert_eq!(device(&text, "rotz", Prop::Deadzone, None).expect("same"), text);
    }

    #[test]
    fn a_further_entry_that_carries_the_other_property_too_loses_only_the_attribute() {
        // 4.12 cut the second entry out whole and with it the dead zone 0.2.
        let text = file().replace(
            "   <option input=\"x\" saturation=\"0.84799999\"/>\n   <option input=\"x\" saturation=\"0.84799999\"/>\n",
            "   <option input=\"x\" saturation=\"1\"/>\n   <option input=\"x\" deadzone=\"0.2\" saturation=\"0.9\"/>\n",
        );
        let new = device(&text, "x", Prop::Saturation, Some(0.5)).expect("written");
        assert!(
            new.contains("<option input=\"x\" deadzone=\"0.2\"/>"),
            "the dead zone of the second entry stays: {new}"
        );
        assert!(new.contains("saturation=\"0.5\"") && !new.contains("saturation=\"0.9\""), "{new}");
        assert_eq!(values(&new, "x").saturation, Some(0.5));
        assert_eq!(values(&new, "x").deadzone, Some(0.2), "the last entry that has one wins, as before");
    }

    #[test]
    fn a_game_axis_that_stands_twice_is_written_where_it_is_read_the_last() {
        let text = file().replace(
            "<flight_move_yaw invert=\"0\"/>",
            "<flight_move_yaw invert=\"0\"/>\n   <flight_move_pitch exponent=\"1\"/>",
        );
        let new = set_game_text(&text, 1, "flight_move_pitch", GameProp::Exponent, Some(2.5)).expect("written");
        let read = axes::game_axes_from_text(&new, &alive());
        let pitch = read[0].axes.iter().find(|(axis, _)| axis == "flight_move_pitch").map(|(_, v)| v.exponent);
        assert_eq!(pitch, Some(Some(2.5)), "{new}");
        assert!(new.contains("exponent=\"1.5\""), "the first stays as it was: {new}");
    }

    #[test]
    fn a_name_that_is_no_element_name_is_refused_for_a_game_axis() {
        let text = file();
        for bad in ["x a=\"1\"/><evil b=\"2\"", "two words", "", "1abc", "a<b"] {
            assert!(
                matches!(set_game_text(&text, 1, bad, GameProp::Exponent, Some(1.0)), Err(Failure::Property)),
                "{bad}"
            );
        }
        assert!(set_game_text(&text, 1, "flight_move_roll", GameProp::Exponent, Some(1.0)).is_ok());
    }

    #[test]
    fn the_block_that_is_written_is_the_one_the_page_shows_not_the_last_of_the_id() {
        // Two blocks of one id: the older name is last in the file. The page shows the block under the name the
        // game uses now; the writing goes there.
        let text = file().replace(OLD, LEFT).replace(
            "LEFT VPC Stick  {03F33344-0000-0000-0000-504944564944}\">\n   <option input=\"x\" deadzone=\"0.2\"",
            "L-VPC Stick  {03F33344-0000-0000-0000-504944564944}\">\n   <option input=\"x\" deadzone=\"0.2\"",
        );
        let new = device(&text, "x", Prop::Deadzone, Some(0.5)).expect("written");
        let first = &new[..new.find("L-VPC").expect("the second block")];
        assert!(first.contains("deadzone=\"0.5\""), "{new}");
        assert!(new.contains("deadzone=\"0.2\""), "the block with the old name is untouched: {new}");
    }

    #[test]
    fn the_refusals_come_before_anything_is_written() {
        let text = file();
        assert!(matches!(device(&text, "x", Prop::Deadzone, Some(1.5)), Err(Failure::Range)));
        assert!(matches!(device(&text, "x", Prop::Deadzone, Some(-0.1)), Err(Failure::Range)));
        assert!(matches!(device(&text, "x", Prop::Deadzone, Some(f64::NAN)), Err(Failure::Range)));
        assert!(matches!(set_device_text(&text, &alive(), "", "x", Prop::Deadzone, Some(0.1)), Err(Failure::Ident)));
        assert!(matches!(
            set_device_text(&text, &alive(), "FFFF", "x", Prop::Deadzone, Some(0.1)),
            Err(Failure::Device)
        ));
        // A dead id is no device either: nothing is written for a block the game ignores.
        assert!(matches!(set_device_text(&text, &alive(), OLD, "x", Prop::Deadzone, Some(0.1)), Err(Failure::Device)));
        assert!(matches!(device("<a><deviceoptions", "x", Prop::Deadzone, Some(0.1)), Err(Failure::Unreadable)));
        // 0 and 1 are in.
        assert!(
            device(&text, "x", Prop::Deadzone, Some(0.0)).is_ok()
                && device(&text, "x", Prop::Deadzone, Some(1.0)).is_ok()
        );
    }

    #[test]
    fn the_sensitivity_is_set_changed_and_taken_out_on_the_game_axis_of_the_device_with_the_number() {
        let text = file();
        let new = set_game_text(&text, 1, "flight_move_pitch", GameProp::Exponent, Some(2.5)).expect("set");
        assert!(new.contains("<flight_move_pitch exponent=\"2.5\" invert=\"1\"/>"), "{new}");
        // `%g`: 1.5 is "1.5", 2.0 is "2".
        let new = set_game_text(&text, 1, "flight_move_pitch", GameProp::Exponent, Some(2.0)).expect("set");
        assert!(new.contains("exponent=\"2\""), "{new}");
        // A game axis that is not there is made, in the block with the number.
        let new = set_game_text(&text, 1, "flight_move_roll", GameProp::Exponent, Some(0.5)).expect("made");
        assert!(new.contains("   <flight_move_roll exponent=\"0.5\"/>\n  </options>"), "{new}");
        // An attribute added to an element that has others, and to one that has none.
        let new = set_game_text(&text, 1, "flight_move_yaw", GameProp::Exponent, Some(3.0)).expect("added");
        assert!(new.contains("<flight_move_yaw invert=\"0\" exponent=\"3\"/>"), "{new}");
        // Invert is a whole number.
        let new = set_game_text(&text, 1, "flight_move_yaw", GameProp::Invert, Some(1.0)).expect("set");
        assert!(new.contains("<flight_move_yaw invert=\"1\"/>"), "{new}");
        // Taken out: the attribute goes; an element with nothing left goes too.
        let new = set_game_text(&text, 1, "flight_move_pitch", GameProp::Exponent, None).expect("out");
        assert!(new.contains("<flight_move_pitch invert=\"1\"/>"), "{new}");
        let new = set_game_text(&text, 1, "flight_move_yaw", GameProp::Invert, None).expect("out");
        assert!(!new.contains("flight_move_yaw"), "{new}");
        // The empty place with the number 2 is a block too (a self-closing one gets its first child).
        let new = set_game_text(&text, 2, "flight_move_pitch", GameProp::Exponent, Some(1.5)).expect("made");
        assert!(
            new.contains(
                "<options type=\"joystick\" instance=\"2\">\n    <flight_move_pitch exponent=\"1.5\"/>\n  </options>"
            ),
            "{new}"
        );
        // No such number, a value out of range.
        assert!(matches!(
            set_game_text(&text, 7, "flight_move_pitch", GameProp::Exponent, Some(1.0)),
            Err(Failure::Device)
        ));
        assert!(matches!(
            set_game_text(&text, 1, "flight_move_pitch", GameProp::Exponent, Some(11.0)),
            Err(Failure::Range)
        ));
        assert!(matches!(
            set_game_text(&text, 1, "flight_move_pitch", GameProp::Exponent, Some(0.05)),
            Err(Failure::Range)
        ));
    }

    #[test]
    fn two_devices_are_made_alike_only_where_they_differ_and_only_on_axes_both_have() {
        const RIGHT: &str = "43F43344-0000-0000-0000-504944564944";
        let text = file().replace(
            "  <deviceoptions name=\"LEFT VPC Stick  {83F33344-0000-0000-0000-504944564944}\">\n   <option input=\"x\" deadzone=\"0.2\"/>\n  </deviceoptions>\n",
            &format!("  <deviceoptions name=\"RIGHT VPC Stick  {{{RIGHT}}}\">\n   <option input=\"x\" deadzone=\"0.152\"/>\n   <option input=\"y\" saturation=\"0.5\"/>\n   <option input=\"slider1\" deadzone=\"0.3\"/>\n   <option input=\"x\" saturation=\"0.7\"/>\n  </deviceoptions>\n"),
        );
        let alive: HashSet<String> = [LEFT.to_string(), RIGHT.to_string()].into_iter().collect();
        let blocks = axes::device_axes_from_text(&text, &alive);
        let settings = align_settings(&blocks, LEFT, RIGHT).expect("settings");
        // x: the dead zone is the same already; the saturation 0.848 is to be written. y: the dead zone 0.15000001 is
        // to be written, and the saturation of the target (0.5) is to be taken out, because the source has none.
        let wanted: Vec<(String, Prop, Option<f64>)> = settings
            .iter()
            .map(|s| match s {
                Setting::Device { axis, prop, value, .. } => (axis.clone(), *prop, *value),
                Setting::Game { .. } => panic!("a device"),
            })
            .collect();
        assert_eq!(
            wanted,
            [
                ("x".to_string(), Prop::Saturation, Some(0.84799999)),
                ("y".to_string(), Prop::Deadzone, Some(0.15000001)),
                ("y".to_string(), Prop::Saturation, Some(0.8)),
            ]
        );
        let (new, count) = align_text(&text, &alive, LEFT, RIGHT).expect("made");
        assert_eq!(count, 3);
        let after = axes::device_axes_from_text(&new, &alive);
        let target = axes::shown_block(&after, RIGHT).expect("target");
        assert_eq!(
            target.axis("x").copied().map(|v| (v.deadzone, v.saturation)),
            Some((Some(0.152), Some(0.84799999)))
        );
        assert_eq!(target.axis("slider1").and_then(|v| v.deadzone), Some(0.3), "an axis the source has not is left");
        // Alike already: nothing to write (the text stays the same).
        let (again, none) = align_text(&new, &alive, LEFT, RIGHT).expect("alike");
        assert_eq!((again, none), (new, 0));
        // The refusals.
        assert!(matches!(align_settings(&blocks, LEFT, LEFT), Err(Failure::Device)));
        assert!(matches!(align_settings(&blocks, "", RIGHT), Err(Failure::Ident)));
        assert!(matches!(align_settings(&blocks, LEFT, "FFFF"), Err(Failure::Device)));
        let no_shared = text.replace("<option input=\"x\" deadzone=\"0.152\"/>\n   <option input=\"y\" saturation=\"0.5\"/>\n   <option input=\"slider1\" deadzone=\"0.3\"/>\n   <option input=\"x\" saturation=\"0.7\"/>", "<option input=\"rotz\" deadzone=\"0.3\"/>");
        let blocks = axes::device_axes_from_text(&no_shared, &alive);
        assert!(matches!(align_settings(&blocks, LEFT, RIGHT), Err(Failure::NothingShared)));
    }

    #[test]
    fn the_dead_blocks_are_cut_out_with_their_lines_and_nothing_else_moves() {
        let text = file();
        let (new, count) = clean_up_text(&text, &alive()).expect("cleaned");
        assert_eq!(count, 1);
        assert_eq!(
            new,
            text.replace(
                "  <deviceoptions name=\"LEFT VPC Stick  {83F33344-0000-0000-0000-504944564944}\">\n   <option input=\"x\" deadzone=\"0.2\"/>\n  </deviceoptions>\n",
                ""
            )
        );
        assert!(new.contains("<!-- kept -->") && new.contains("<flight_move_pitch"));
        // Nothing dead (or a block without an id, like the mouse): nothing to clean, said as it is.
        assert!(matches!(clean_up_text(&new, &alive()), Err(Failure::NothingToClean)));
        let mouse = "<A><deviceoptions name=\"Mouse\"><option input=\"x\" deadzone=\"0.1\"/></deviceoptions></A>";
        assert!(matches!(clean_up_text(mouse, &alive()), Err(Failure::NothingToClean)));
        assert!(matches!(clean_up_text("<A><deviceoptions", &alive()), Err(Failure::Unreadable)));
        // A block under an older name of a device that is alive is no old entry: its id is alive.
        let two = file().replace(OLD, LEFT);
        assert!(matches!(clean_up_text(&two, &alive()), Err(Failure::NothingToClean)));
    }

    #[test]
    fn what_hangs_on_an_old_id_is_carried_to_the_live_block_with_the_width_the_game_wrote() {
        let text = file().replace(
            "<option input=\"x\" deadzone=\"0.2\"/>\n  </deviceoptions>\n  <options",
            "<option input=\"x\" deadzone=\"0.098999992\"/>\n   <option input=\"rotz\" saturation=\"0.7\"/>\n  </deviceoptions>\n  <options",
        );
        let summary = axes::summary_from_text(&text, &alive());
        assert_eq!(summary.adoptable.len(), 1);
        let settings = adopt_settings(&summary.adoptable[0]);
        let (new, count) = apply_text(&text, &alive(), &settings).expect("carried");
        assert_eq!(count, 2);
        assert!(new.contains("<option input=\"x\" deadzone=\"0.098999992\"/>"), "{new}");
        assert_eq!(values(&new, "rotz").saturation, Some(0.7));
        // The old block is still there: cleaning is a step of its own.
        assert!(new.contains("83F33344"));
    }

    #[test]
    fn several_settings_are_one_text_and_the_line_ends_and_the_mark_stay() {
        let text = file().replace('\n', "\r\n");
        let settings = [
            Setting::Device { ident: LEFT.into(), axis: "x".into(), prop: Prop::Deadzone, value: Some(0.2) },
            Setting::Device { ident: LEFT.into(), axis: "rotz".into(), prop: Prop::Saturation, value: Some(0.7) },
            Setting::Game { number: 1, axis: "flight_move_pitch".into(), prop: GameProp::Exponent, value: Some(1.25) },
        ];
        let (new, count) = apply_text(&text, &alive(), &settings).expect("made");
        assert_eq!(count, 3);
        assert!(new.contains("deadzone=\"0.2\"") && new.contains("rotz") && new.contains("exponent=\"1.25\""), "{new}");
        assert!(!new.replace("\r\n", "").contains('\n'), "every line end is still a \\r\\n");
        // One refusal stops the whole list.
        let bad = [
            settings[0].clone(),
            Setting::Device { ident: LEFT.into(), axis: "x".into(), prop: Prop::Deadzone, value: Some(7.0) },
        ];
        assert!(matches!(apply_text(&text, &alive(), &bad), Err(Failure::Range)));
    }
}
