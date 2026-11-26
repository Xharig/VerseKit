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

//! Keys for the game actions of the Stream Deck that have none on the keyboard, put into the `actionmaps.xml` on
//! the player's click: first a plan ([`plan`]) the page shows, then the writing ([`apply`]), afterwards a way back
//! ([`undo`], or the whole file of before with [`restore_copy`]).
//!
//! ⚠⚠ **A binding of the player is never overwritten.** A key is only added where the action has no keyboard line of
//! the player and no factory key on the keyboard; a keyboard line the player emptied (deleted in the game) stays
//! empty. Mouse, stick and pad lines stay as they are: the keyboard key comes beside them.
//!
//! ⚠ Only free keys of a fixed set, in a fixed order ([`key_set`], in stages): free of every action of
//! the game (factory and own, [`crate::controls::conflicts`]), of the hotkeys of VerseKit and of the keys given out
//! in the same go. The writing goes the one way every change of the file goes ([`crate::joysticks_write`]): the game
//! must be quiet, a copy is made first, nothing broken is written.
//!
//! ⚠⚠ **No right Alt.** On a German keyboard (and every layout with Alt Gr) Windows sends the left Ctrl with the right
//! Alt, from the deck too; the game then finds no binding of `lctrl+ralt+i` and presses the key alone, `i`: in the
//! factory setting that switched the power of the thrusters off. A key whose key alone (or with Ctrl) is a dangerous
//! action of the game ([`DANGEROUS_ALONE`]) is not given out either.

use serde_json::{Value, json};

use crate::controls::{self, Profile, View};
use crate::game_binding::{self, How};
use crate::joysticks::Binding;
use crate::joysticks_write::{self, Failure};
use crate::overlay::Hotkey;
use crate::paths::Paths;

/// What was put in, in the data folder: for the list and the way back.
pub const RECORD_FILE: &str = "streamdeck_eintraege.json";

/// Left Alt and left Shift with a letter, then with a digit: the first stage of [`key_set`].
pub fn main_set() -> Vec<String> {
    with_letters_and_digits("lalt+lshift")
}

fn with_letters_and_digits(modifiers: &str) -> Vec<String> {
    let letters = (b'a'..=b'z').map(|c| format!("{modifiers}+{}", c as char));
    let digits = (b'0'..=b'9').map(|c| format!("{modifiers}+{}", c as char));
    letters.chain(digits).collect()
}

/// Every key VerseKit may give out, in the order it gives them: left Alt + left Shift with a letter or digit, then
/// left Ctrl + left Shift with them, then F1 to F12 with left Shift, left Ctrl, left Alt + left Shift and left Ctrl +
/// left Shift, then the number block 0 to 9 with left Ctrl, left Alt + left Shift and left Shift. Never the right Alt,
/// never left Ctrl with left Alt (Alt Gr on a German keyboard), never the Windows key, never Alt with F4.
pub fn key_set() -> Vec<String> {
    let mut keys = main_set();
    keys.extend(with_letters_and_digits("lctrl+lshift"));
    for modifiers in ["lshift", "lctrl", "lalt+lshift", "lctrl+lshift"] {
        keys.extend((1..=12).map(|n| format!("{modifiers}+f{n}")).filter(|key| !closes_the_window(key)));
    }
    for modifiers in ["lctrl", "lalt+lshift", "lshift"] {
        keys.extend((0..=9).map(|n| format!("{modifiers}+np_{n}")));
    }
    keys
}

/// Alt with F4 closes the window in front.
fn closes_the_window(key: &str) -> bool {
    key.ends_with("+f4") && key.split('+').any(|m| m.ends_with("alt"))
}

/// The actions of the game that must never come with a key VerseKit gives out, also not by the key alone: power, the
/// thrusters, eject, emergency exit, self destruct.
pub const DANGEROUS_ALONE: [&str; 8] = [
    "v_power_toggle",
    "v_power_toggle_thrusters",
    "v_power_toggle_shields",
    "v_power_toggle_weapons",
    "v_power_set_off",
    "v_eject",
    "v_emergency_exit",
    "v_self_destruct",
];

/// Is the key alone of `input` (`i` of `lalt+lshift+i`), or with a Ctrl, a dangerous action of the game
/// ([`DANGEROUS_ALONE`]), in the factory setting or the player's? The game may press it when a combination does not
/// come as it was bound.
fn dangerous_alone(all: &controls::Devices, action: &str, input: &str) -> bool {
    let base = input.rsplit('+').next().unwrap_or(input);
    [base.to_string(), format!("lctrl+{base}"), format!("rctrl+{base}"), format!("lctrl+ralt+{base}")]
        .iter()
        .any(|key| controls::conflicts(all, action, "kb1", key).iter().any(|e| DANGEROUS_ALONE.contains(&e.action.as_str())))
}

/// What happens with one action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// This key is put in.
    Add(String),
    /// The action has a key on the keyboard already: nothing to do.
    HasKey,
    /// The player emptied its keyboard line in the game: it stays so.
    Deleted,
    /// The game does not know the action.
    Unknown,
    /// The group of the action is not known: no place in the file.
    NoGroup,
    /// Every key of the set is taken.
    NoFreeKey,
    /// The keyboard line of the action holds a mouse button (`mouse`) and the mouse line of the action is free: the
    /// button moves to the mouse line, where it keeps working, and the keyboard line takes `key`.
    Move { mouse: String, key: String },
    /// The keyboard line of the action holds what the Stream Deck cannot press (a mouse button): this input. Its mouse
    /// line is taken (or the input is no plain mouse button), so nothing can move without a loss: it stays.
    NotSendable(String),
    /// The keyboard key of the action holds the right Alt (`from`, Alt Gr on a German keyboard): the game then
    /// presses the key alone too (`alone`: the action on it, where there is one). An own keyboard line with the safe
    /// `key` takes its place; `factory` where `from` was the factory setting (the way back then takes the line out).
    Relocate { from: String, key: String, alone: Option<String>, factory: bool },
    /// The player emptied the keyboard line in the game, and the plan puts this key in all the same; the way back
    /// empties the line again.
    Refill(String),
}

/// Does `input` hold the right Alt as a modifier (`ralt+p`)? On a German keyboard that is Alt Gr: Windows adds the
/// left Ctrl, and the game presses the key alone.
pub fn alt_gr(input: &str) -> bool {
    let parts: Vec<&str> = input.split('+').collect();
    parts.len() > 1 && parts[..parts.len() - 1].iter().any(|m| m.trim().eq_ignore_ascii_case("ralt"))
}

/// The action the key alone of `input` presses (a dangerous one first), besides `action` itself.
fn action_alone(all: &controls::Devices, action: &str, input: &str) -> Option<String> {
    let base = input.rsplit('+').next().unwrap_or(input);
    let on_it = controls::conflicts(all, action, "kb1", base);
    on_it.iter().find(|e| DANGEROUS_ALONE.contains(&e.action.as_str())).or_else(|| on_it.first()).map(|e| e.action.clone())
}

/// Does the game action `name` lie on a keyboard key with the right Alt ([`alt_gr`]), the player's or the factory's?
/// It is not safe: it counts as missing until it has a safe key of its own.
fn unsafe_key(name: &str, own: &[(String, Vec<Binding>)], profile: &Profile) -> bool {
    let resolved = game_binding::resolve(name, own, profile);
    matches!(&resolved.how, How::Keyboard { input, .. } if alt_gr(input))
}

/// Is `input` a plain button or wheel of the mouse, as the game writes it on a keyboard line (`mouse4`, `mwheel_up`)?
fn plain_mouse(input: &str) -> bool {
    let input = input.trim().to_ascii_lowercase();
    input.strip_prefix("mouse").is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        || matches!(input.as_str(), "mwheel_up" | "mwheel_down")
}

/// Is the mouse line of `action` in `section` free: no line of the player on a mouse (not even an emptied one) and
/// no mouse button of the factory setting?
fn mouse_line_free(action: &str, section: &str, own: &[(String, Vec<Binding>)], profile: &Profile) -> bool {
    let own_mouse = own.iter().filter(|(id, _)| id.starts_with("mo")).flat_map(|(_, list)| list.iter()).any(|b| {
        b.action == action && (section.is_empty() || b.section.is_empty() || b.section == section)
    });
    let factory_mouse = profile.defaults.get(action).is_some_and(|fields| fields.iter().any(|(f, i)| f == "mouse" && !i.is_empty()));
    !own_mouse && !factory_mouse
}

/// Is the game action `name` a missing key: known to the game, without a key on the keyboard the deck can press, and
/// not a mouse button that stays ([`stays_on_mouse`])? The one rule for the bar of the main window ([`count`]) and the
/// readiness on the deck: both say the same number.
pub fn missing(name: &str, own: &[(String, Vec<Binding>)], profile: &Profile) -> bool {
    game_binding::known(name, own, profile)
        && (!matches!(game_binding::resolve(name, own, profile).how, How::Keyboard { .. }) || unsafe_key(name, own, profile))
        && !stays_on_mouse(name, own, profile)
}

/// Does the game action `name` keep a mouse button the Stream Deck cannot press, because nothing can move without a
/// loss ([`Outcome::NotSendable`])? Such an action is no missing key: it works with the mouse.
pub fn stays_on_mouse(name: &str, own: &[(String, Vec<Binding>)], profile: &Profile) -> bool {
    let resolved = game_binding::resolve(name, own, profile);
    let (_, action) = game_binding::qualified(name, profile);
    match &resolved.how {
        How::NotSendable { input } => !(plain_mouse(input) && mouse_line_free(action, &resolved.group, own, profile)),
        _ => false,
    }
}

impl Outcome {
    /// The text key of the line on the page.
    pub fn text_key(&self) -> &'static str {
        match self {
            Outcome::Add(_) => "sd_f_neu",
            Outcome::HasKey => "sd_f_hat",
            Outcome::Deleted => "sd_f_geloescht",
            Outcome::Unknown => "sd_f_unbekannt",
            Outcome::NoGroup => "sd_f_ohne_gruppe",
            Outcome::NoFreeKey => "sd_f_voll",
            Outcome::Move { .. } => "sd_f_verschieben",
            Outcome::NotSendable(_) => "sd_f_maus",
            Outcome::Relocate { .. } => "sd_f_umziehen",
            Outcome::Refill(_) => "sd_f_wieder",
        }
    }
}

/// One action of the plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// The plain name, as the file writes it; its `actionmap` is [`Item::group`].
    pub action: String,
    pub group: String,
    pub outcome: Outcome,
}

/// Does a hotkey of VerseKit lie on the key `input` of the sets? Windows hears a hotkey on both Alt, Ctrl and Shift
/// keys.
fn hotkey_on(input: &str, hotkeys: &[Hotkey]) -> bool {
    let parts: Vec<&str> = input.split('+').collect();
    let Some((rest, mods)) = parts.split_last() else { return false };
    let has = |name: &str| mods.iter().any(|m| m.ends_with(name));
    let function = rest.strip_prefix('f').and_then(|n| n.parse::<u16>().ok()).filter(|n| (1..=24).contains(n));
    let code = match (rest.strip_prefix("np_"), function) {
        (Some(digit), _) => digit.parse::<u16>().ok().map(|n| 0x60 + n),
        (None, Some(n)) => Some(0x6F + n),
        (None, None) => rest.chars().next().filter(|_| rest.len() == 1).map(|c| c.to_ascii_uppercase() as u16),
    };
    hotkeys.iter().any(|h| h.alt == has("alt") && h.ctrl == has("ctrl") && h.shift == has("shift") && Some(h.key) == code)
}

/// The plan for the game actions `in_use`, with the bindings `own` and the factory setting `profile`; `hotkeys`
/// are the hotkeys of VerseKit. The keys come from [`key_set`], stage after stage as the earlier ones are taken. A
/// keyboard line the player emptied in the game gets a key too, unless `keep_deleted`.
pub fn plan(
    in_use: &[String],
    own: &[(String, Vec<Binding>)],
    profile: &Profile,
    hotkeys: &[Hotkey],
    keep_deleted: bool,
) -> Vec<Item> {
    let all = controls::view(View::All, own, profile);
    let set = key_set();
    let mut given: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for name in in_use {
        let known = game_binding::known(name, own, profile);
        // A qualified name (`vehicle_general.v_view_zoom_in`) is written as the plain name in its `actionmap`.
        let (_, action) = game_binding::qualified(name, profile);
        // An own keyboard line with an input always stands in the joined view, so it makes `how` a keyboard key
        // (sendable or not): the check below covers own and factory keys alike.
        let resolved = game_binding::resolve(name, own, profile);
        let mut free_key = || {
            let free = set.iter().find(|key| {
                !given.contains(key)
                    && !hotkey_on(key, hotkeys)
                    && controls::conflicts(&all, action, "kb1", key).is_empty()
                    // A combination of keys on the mouse device (`mo1_lalt+lshift+j`) holds the same keys.
                    && controls::conflicts(&all, action, "mo1", key).is_empty()
                    && !dangerous_alone(&all, action, key)
            });
            free.inspect(|key| given.push((*key).clone())).cloned()
        };
        let outcome = if !known {
            Outcome::Unknown
        } else if resolved.deleted {
            if keep_deleted || resolved.group.is_empty() {
                Outcome::Deleted
            } else {
                match free_key() {
                    Some(key) => Outcome::Refill(key),
                    None => Outcome::NoFreeKey,
                }
            }
        } else if let How::NotSendable { input } = &resolved.how {
            // A mouse button on the keyboard line moves to the free mouse line; the keyboard line takes a key.
            if plain_mouse(input) && !resolved.group.is_empty() && mouse_line_free(action, &resolved.group, own, profile) {
                match free_key() {
                    Some(key) => Outcome::Move { mouse: input.trim().to_ascii_lowercase(), key },
                    None => Outcome::NoFreeKey,
                }
            } else {
                Outcome::NotSendable(input.clone())
            }
        } else if let How::Keyboard { input, .. } = &resolved.how
            && alt_gr(input)
            && !resolved.group.is_empty()
        {
            // A key with Alt Gr (the player's or the factory's) gets a safe key of its own on the keyboard line; the
            // stick, mouse and pad lines stay.
            let (from, alone, factory) = (input.clone(), action_alone(&all, action, input), !resolved.own);
            match free_key() {
                Some(key) => Outcome::Relocate { from, key, alone, factory },
                None => Outcome::NoFreeKey,
            }
        } else if matches!(resolved.how, How::Keyboard { .. }) {
            Outcome::HasKey
        } else if resolved.group.is_empty() {
            Outcome::NoGroup
        } else {
            match free_key() {
                Some(key) => Outcome::Add(key),
                None => Outcome::NoFreeKey,
            }
        };
        out.push(Item { action: action.to_string(), group: resolved.group, outcome });
    }
    out
}

/// The numbers of a plan the page shows beside the button.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Count {
    /// Game actions of the deck without a key on the keyboard (also the deleted ones and those that get none).
    pub without_key: usize,
    /// Of them, the ones the player emptied in the game: they stay so.
    pub deleted: usize,
    /// Of them, the ones that get a key.
    pub addable: usize,
}

/// The numbers of a plan; an action the game does not know counts nowhere.
pub fn count(items: &[Item]) -> Count {
    let mut count = Count::default();
    for item in items {
        match item.outcome {
            // A mouse button that cannot move without a loss works with the mouse: no missing key.
            Outcome::HasKey | Outcome::Unknown | Outcome::NotSendable(_) => continue,
            Outcome::Add(_) | Outcome::Move { .. } | Outcome::Relocate { .. } | Outcome::Refill(_) => count.addable += 1,
            Outcome::Deleted => count.deleted += 1,
            Outcome::NoGroup | Outcome::NoFreeKey => {}
        }
        count.without_key += 1;
    }
    count
}

/// What was put in last.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// The binding file written.
    pub file: String,
    /// The copy of it made before.
    pub copy: Option<String>,
    /// When (Unix seconds).
    pub at: i64,
    /// `(action, group, input)`.
    pub entries: Vec<(String, String, String)>,
    /// The mouse buttons moved from the keyboard line to the mouse line: `(action, group, mouse button, key)`; the
    /// key is among [`Record::entries`] too.
    pub moved: Vec<(String, String, String, String)>,
    /// The player's keys with Alt Gr moved to a safe key: `(action, group, old input, new key)`; the new key is among
    /// [`Record::entries`] too.
    pub relocated: Vec<(String, String, String, String)>,
    /// The keyboard lines the player had emptied and that took a key: `(action, group, key)`; the key is among
    /// [`Record::entries`] too.
    pub refilled: Vec<(String, String, String)>,
}

impl Record {
    pub fn to_text(&self) -> String {
        let entries: Vec<Value> =
            self.entries.iter().map(|(a, g, i)| json!({"aktion": a, "gruppe": g, "taste": i})).collect();
        let moved: Vec<Value> =
            self.moved.iter().map(|(a, g, m, k)| json!({"aktion": a, "gruppe": g, "maus": m, "taste": k})).collect();
        let relocated: Vec<Value> =
            self.relocated.iter().map(|(a, g, o, k)| json!({"aktion": a, "gruppe": g, "vorher": o, "taste": k})).collect();
        let refilled: Vec<Value> =
            self.refilled.iter().map(|(a, g, k)| json!({"aktion": a, "gruppe": g, "taste": k})).collect();
        let value = json!({"version": 1, "datei": self.file, "kopie": self.copy, "zeit": self.at, "eintraege": entries,
                           "verschoben": moved, "umgezogen": relocated, "geloescht": refilled});
        serde_json::to_string_pretty(&value).unwrap_or_default()
    }

    pub fn parse(text: &str) -> Option<Record> {
        let value: Value = serde_json::from_str(text).ok()?;
        if value["version"].as_u64() != Some(1) {
            return None;
        }
        let three = |e: &Value| -> Option<(String, String, String)> {
            Some((e["aktion"].as_str()?.to_string(), e["gruppe"].as_str()?.to_string(), e["taste"].as_str()?.to_string()))
        };
        let entries = value["eintraege"].as_array()?.iter().filter_map(three).collect();
        let refilled = value["geloescht"].as_array().map(|l| l.iter().filter_map(three).collect()).unwrap_or_default();
        let four = |list: &str, third: &str| -> Vec<(String, String, String, String)> {
            value[list]
                .as_array()
                .map(|list| {
                    list.iter()
                        .filter_map(|e| {
                            let text = |k: &str| e[k].as_str().map(String::from);
                            Some((text("aktion")?, text("gruppe")?, text(third)?, text("taste")?))
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        Some(Record {
            file: value["datei"].as_str()?.to_string(),
            copy: value["kopie"].as_str().map(String::from),
            at: value["zeit"].as_i64().unwrap_or(0),
            entries,
            moved: four("verschoben", "maus"),
            relocated: four("umgezogen", "vorher"),
            refilled,
        })
    }
}

/// The record in the data folder; `None` where nothing was put in (or it was taken back).
pub fn load_record(paths: &Paths) -> Option<Record> {
    std::fs::read_to_string(paths.app_file(RECORD_FILE)).ok().and_then(|text| Record::parse(&text))
}

fn forget_record(paths: &Paths) {
    let _ = crate::jsonio::remove_file(&paths.app_file(RECORD_FILE));
}

/// Puts the keys of the plan in, in one writing with a copy first, and keeps the record.
///
/// # Errors
///
/// [`Failure::Nothing`] where the plan adds no key; the refusals of the writing (the game runs, the file changed …).
pub fn apply(paths: &Paths, items: &[Item], now: i64) -> Result<Record, Failure> {
    let adds: Vec<(String, String, String)> = items
        .iter()
        .filter_map(|item| match &item.outcome {
            Outcome::Add(key) | Outcome::Move { key, .. } | Outcome::Relocate { key, .. } | Outcome::Refill(key) => {
                Some((item.action.clone(), item.group.clone(), key.clone()))
            }
            _ => None,
        })
        .collect();
    let refilled: Vec<(String, String, String)> = items
        .iter()
        .filter_map(|item| match &item.outcome {
            Outcome::Refill(key) => Some((item.action.clone(), item.group.clone(), key.clone())),
            _ => None,
        })
        .collect();
    let relocated: Vec<(String, String, String, String)> = items
        .iter()
        .filter_map(|item| match &item.outcome {
            // The way back puts the player's key back; for the factory's it takes the own line out (empty here).
            Outcome::Relocate { from, key, factory, .. } => {
                let before = if *factory { String::new() } else { from.clone() };
                Some((item.action.clone(), item.group.clone(), before, key.clone()))
            }
            _ => None,
        })
        .collect();
    let moved: Vec<(String, String, String, String)> = items
        .iter()
        .filter_map(|item| match &item.outcome {
            Outcome::Move { mouse, key } => Some((item.action.clone(), item.group.clone(), mouse.clone(), key.clone())),
            _ => None,
        })
        .collect();
    let mice: Vec<(String, String, String)> = moved.iter().map(|(a, g, m, _)| (a.clone(), g.clone(), m.clone())).collect();
    let file = crate::joysticks::actionmaps_path(paths, None).ok_or(Failure::NoFile)?;
    let done = joysticks_write::fill(paths, &adds, &mice, Some(&file))?;
    let record = Record { file, copy: done.backup, at: now, entries: adds, moved, relocated, refilled };
    let _ = crate::jsonio::write_text_atomic(&paths.app_file(RECORD_FILE), &record.to_text(), crate::jsonio::Newline::Keep);
    Ok(record)
}

/// Takes out exactly the lines that were put in, where they still stand so; gives the actions whose line was taken.
/// The record goes when nothing of it is left to take back.
///
/// # Errors
///
/// [`Failure::Nothing`] without a record; the refusals of the reading and the writing.
pub fn undo(paths: &Paths) -> Result<Vec<String>, Failure> {
    let record = load_record(paths).ok_or(Failure::Nothing)?;
    let items: Vec<(String, String)> = record.entries.iter().map(|(a, _, i)| (a.clone(), i.clone())).collect();
    match joysticks_write::unfill(paths, &items, &record.moved, &record.relocated, &record.refilled, Some(&record.file)) {
        Ok((_, found)) => {
            forget_record(paths);
            Ok(found)
        }
        Err(Failure::Same) => {
            forget_record(paths);
            Ok(Vec::new())
        }
        Err(other) => Err(other),
    }
}

/// Puts the whole file of before back (the copy made before the keys went in); later changes of the player go with
/// it. A copy of the file as it is now is made first.
///
/// # Errors
///
/// [`Failure::Nothing`] without a record or a copy; the refusals of the writing.
pub fn restore_copy(paths: &Paths) -> Result<(), Failure> {
    let record = load_record(paths).ok_or(Failure::Nothing)?;
    let copy = record.copy.as_deref().ok_or(Failure::Nothing)?;
    let bytes = std::fs::read(copy).map_err(|_| Failure::Unreadable)?;
    crate::game_files::write(paths, &record.file, &bytes).map_err(Failure::Write)?;
    forget_record(paths);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::joysticks::bindings_from_text;

    fn profile() -> Profile {
        Profile::from_xml(
            r#"<ActionMaps>
              <actionmap name="spaceship_general" UILabel="@g">
                <action name="v_eject" UILabel="@e" keyboard="lalt+lshift+y"/>
                <action name="v_taken_a" UILabel="@a" keyboard="lalt+lshift+a"/>
                <action name="v_free" UILabel="@f"/>
                <action name="v_free2" UILabel="@f2"/>
                <action name="v_mouse" UILabel="@m" mouse="mouse3"/>
                <action name="v_deleted" UILabel="@d" keyboard="lalt+d"/>
              </actionmap>
            </ActionMaps>"#,
        )
        .expect("profile")
    }

    fn own(body: &str) -> Vec<(String, Vec<Binding>)> {
        bindings_from_text(&format!(
            r#"<ActionMaps version="1"><ActionProfiles version="1" profileName="default">{body}</ActionProfiles></ActionMaps>"#
        ))
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    /// The plan with the lines the player emptied kept empty.
    fn plan(in_use: &[String], own: &[(String, Vec<Binding>)], profile: &Profile, hotkeys: &[Hotkey]) -> Vec<Item> {
        super::plan(in_use, own, profile, hotkeys, true)
    }

    #[test]
    fn the_sets_have_their_keys_and_every_key_can_be_sent() {
        assert_eq!(main_set().len(), 36);
        assert_eq!(main_set()[0], "lalt+lshift+a");
        let set = key_set();
        // 36 + 36 letters and digits, 4 × 12 F keys without Alt + F4, 3 × 10 keys of the number block.
        assert_eq!(set.len(), 36 + 36 + 47 + 30);
        assert_eq!((set[36].as_str(), set[72].as_str(), set[119].as_str()), ("lctrl+lshift+a", "lshift+f1", "lctrl+np_0"));
        assert_eq!(set[..36], main_set()[..], "the first stage comes first");
        for (n, key) in set.iter().enumerate() {
            assert!(crate::anti_afk_key::parse(key).is_some(), "{key}");
            assert!(!set[..n].contains(key), "given once: {key}");
            let mods: Vec<&str> = key.split('+').rev().skip(1).collect();
            // The right Alt is Alt Gr on a German keyboard: Windows adds the left Ctrl, and the game presses the key
            // alone. Left Ctrl with left Alt is Alt Gr too.
            assert!(!key.contains("ralt"), "{key}");
            assert!(!(mods.contains(&"lctrl") && mods.contains(&"lalt")), "{key}");
            assert!(!key.contains("win"), "{key}");
            assert!(!(key.ends_with("+f4") && mods.contains(&"lalt")), "Alt + F4 closes the game: {key}");
        }
    }

    /// Hotkeys of VerseKit on every key of every set: nothing is free.
    fn every_key_held() -> Vec<Hotkey> {
        let codes = (b'A'..=b'Z').chain(b'0'..=b'9').map(u16::from).chain(0x70..=0x7B).chain(0x60..=0x69);
        let codes: Vec<u16> = codes.collect();
        let mut out = Vec::new();
        for mods in 0..8u8 {
            for key in &codes {
                out.push(Hotkey { ctrl: mods & 1 != 0, alt: mods & 2 != 0, shift: mods & 4 != 0, key: *key });
            }
        }
        out
    }

    #[test]
    fn a_hotkey_of_verse_kit_on_an_f_key_or_the_number_block_is_seen() {
        let ctrl_f5 = [Hotkey::parse("Strg+F5").expect("hotkey")];
        assert!(hotkey_on("lctrl+f5", &ctrl_f5) && !hotkey_on("lctrl+f6", &ctrl_f5) && !hotkey_on("lshift+f5", &ctrl_f5));
        assert!(key_set().iter().all(|key| hotkey_on(key, &every_key_held())), "every key of the sets is a hotkey key");
    }

    #[test]
    fn a_deck_of_170_actions_beside_a_full_factory_setting_gets_a_key_for_every_one() {
        // The factory setting holds every plain key, Alt, Ctrl and Shift with a letter, the F keys and the number
        // block alone; dangerous actions lie on `i`, `b` and Ctrl + `c`.
        let mut factory = Vec::new();
        let plain = (b'a'..=b'z').chain(b'0'..=b'9').map(|c| (c as char).to_string());
        let alone = plain.chain((1..=12).map(|n| format!("f{n}"))).chain((0..=9).map(|n| format!("np_{n}")));
        let letters = || (b'a'..=b'z').map(|c| c as char);
        let held: Vec<String> = alone
            .chain(letters().map(|c| format!("lalt+{c}")))
            .chain(letters().map(|c| format!("lctrl+{c}")))
            .chain(letters().map(|c| format!("lshift+{c}")))
            .filter(|k| !matches!(k.as_str(), "i" | "b" | "lctrl+c"))
            .collect();
        for (n, key) in held.iter().enumerate() {
            factory.push(format!(r#"<action name="v_held_{n}" UILabel="@h" keyboard="{key}"/>"#));
        }
        factory.push(r#"<action name="v_power_toggle_thrusters" UILabel="@t" keyboard="i"/>"#.into());
        factory.push(r#"<action name="v_eject" UILabel="@e" keyboard="b"/>"#.into());
        factory.push(r#"<action name="v_self_destruct" UILabel="@s" keyboard="lctrl+c"/>"#.into());
        for n in 0..170 {
            factory.push(format!(r#"<action name="v_deck_{n}" UILabel="@d"/>"#));
        }
        let xml = format!(r#"<ActionMaps><actionmap name="spaceship_general" UILabel="@g">{}</actionmap></ActionMaps>"#, factory.concat());
        let profile = Profile::from_xml(&xml).expect("profile");
        // 50 actions of the deck have a key (factory), 120 have none.
        let deck: Vec<String> = held.iter().take(50).enumerate().map(|(n, _)| format!("v_held_{n}")).chain((0..120).map(|n| format!("v_deck_{n}"))).collect();
        assert_eq!(deck.len(), 170);
        let items = plan(&deck, &[], &profile, &[]);
        assert!(!items.iter().any(|i| i.outcome == Outcome::NoFreeKey), "{:?}", items.iter().filter(|i| i.outcome == Outcome::NoFreeKey).count());
        assert_eq!(count(&items), Count { without_key: 120, deleted: 0, addable: 120 });
        let given: Vec<&String> = items.iter().filter_map(|i| if let Outcome::Add(k) = &i.outcome { Some(k) } else { None }).collect();
        for base in ["i", "b", "c"] {
            assert!(!given.iter().any(|k| k.ends_with(&format!("+{base}"))), "a dangerous key alone: {base}");
        }
        assert!(given.iter().any(|k| k.contains("+f")) && given.iter().any(|k| k.contains("np_")), "the later stages are used");
    }

    #[test]
    fn the_keys_go_out_stage_after_stage_when_the_earlier_ones_are_taken() {
        // VerseKit holds every left Alt + left Shift key and every left Ctrl + left Shift key: the F keys come next.
        let held: Vec<Hotkey> = every_key_held().into_iter().filter(|h| h.shift && (h.alt != h.ctrl) && h.key < 0x60).collect();
        let items = plan(&names(&["v_free", "v_free2"]), &own(""), &profile(), &held);
        assert_eq!(items[0].outcome, Outcome::Add("lshift+f1".into()));
        assert_eq!(items[1].outcome, Outcome::Add("lshift+f2".into()));
        // Every F key held too: the number block.
        let held: Vec<Hotkey> = every_key_held().into_iter().filter(|h| !(0x60..=0x69).contains(&h.key)).collect();
        let items = plan(&names(&["v_free"]), &own(""), &profile(), &held);
        assert_eq!(items[0].outcome, Outcome::Add("lctrl+np_0".into()));
    }

    #[test]
    fn a_key_whose_key_alone_switches_power_off_is_never_given_out() {
        // As on the device: `i` alone is the power of the thrusters, `b` alone eject, `c` with Ctrl self destruct.
        let profile = Profile::from_xml(
            r#"<ActionMaps>
              <actionmap name="spaceship_power" UILabel="@p"><action name="v_power_toggle_thrusters" UILabel="@t" keyboard="a"/></actionmap>
              <actionmap name="spaceship_general" UILabel="@g">
                <action name="v_eject" UILabel="@e" keyboard="b"/>
                <action name="v_self_destruct" UILabel="@s" keyboard="lctrl+c"/>
                <action name="v_lights" UILabel="@l" keyboard="d"/>
              </actionmap>
              <actionmap name="spaceship_movement" UILabel="@m"><action name="v_master_mode_cycle_long" UILabel="@mm"/></actionmap>
            </ActionMaps>"#,
        )
        .expect("profile");
        let body = r#"<actionmap name="spaceship_movement"><action name="v_master_mode_cycle_long"><rebind input="kb1_mouse4"/></action></actionmap>"#;
        let items = plan(&names(&["v_master_mode_cycle_long"]), &own(body), &profile, &[]);
        assert_eq!(
            items[0].outcome,
            Outcome::Move { mouse: "mouse4".into(), key: "lalt+lshift+d".into() },
            "not a (thrusters), not b (eject), not c (self destruct with Ctrl); d alone is harmless"
        );
        // The player's own binding counts too: eject moved from `b` to `d` alone, so `b` is free and `d` is not.
        let own_eject = r#"<actionmap name="spaceship_general"><action name="v_eject"><rebind input="kb1_d"/></action></actionmap>
            <actionmap name="spaceship_movement"><action name="v_master_mode_cycle_long"><rebind input="kb1_mouse4"/></action></actionmap>"#;
        let items = plan(&names(&["v_master_mode_cycle_long", "v_lights"]), &own(own_eject), &profile, &[]);
        assert_eq!(items[0].outcome, Outcome::Move { mouse: "mouse4".into(), key: "lalt+lshift+b".into() });
    }

    #[test]
    fn a_mouse_button_on_the_keyboard_line_moves_to_the_mouse_line_and_a_key_combination_of_the_mouse_is_taken() {
        // As in a real binding file: a mouse button on the keyboard device, a combination of keys on the mouse device.
        let body = r#"<actionmap name="spaceship_general">
            <action name="v_free"><rebind input="kb1_mouse4"/></action>
            <action name="v_eject"><rebind input="mo1_lalt+lshift+b"/></action>
        </actionmap>"#;
        let items = plan(&names(&["v_free", "v_free2"]), &own(body), &profile(), &[]);
        assert_eq!(
            items[0].outcome,
            Outcome::Move { mouse: "mouse4".into(), key: "lalt+lshift+c".into() },
            "the mouse line is free: the button moves there, the keyboard line takes a key (lalt+lshift+b is held on the mouse)"
        );
        assert_eq!(items[1].outcome, Outcome::Add("lalt+lshift+d".into()));
        assert_eq!(count(&items), Count { without_key: 2, deleted: 0, addable: 2 }, "both get a key");
    }

    #[test]
    fn a_mouse_button_stays_where_the_mouse_line_is_taken_and_counts_as_no_missing_key() {
        // v_free: its own mouse line holds another button; v_mouse: the factory gives it mouse3; v_free2: a combination.
        let body = r#"<actionmap name="spaceship_general">
            <action name="v_free"><rebind input="kb1_mouse4"/><rebind input="mo1_mouse5"/></action>
            <action name="v_mouse"><rebind input="kb1_mouse4"/></action>
            <action name="v_free2"><rebind input="kb1_lalt+mouse1"/></action>
        </actionmap>"#;
        let items = plan(&names(&["v_free", "v_mouse", "v_free2"]), &own(body), &profile(), &[]);
        assert_eq!(items[0].outcome, Outcome::NotSendable("mouse4".into()), "own mouse line taken: nothing is lost");
        assert_eq!(items[1].outcome, Outcome::NotSendable("mouse4".into()), "factory mouse button: nothing is lost");
        assert_eq!(items[2].outcome, Outcome::NotSendable("lalt+mouse1".into()), "no plain mouse button");
        assert_eq!(count(&items), Count::default(), "they work with the mouse: no missing key");
        for name in ["v_free", "v_mouse", "v_free2"] {
            assert!(stays_on_mouse(name, &own(body), &profile()), "{name}");
        }
        let movable = r#"<actionmap name="spaceship_general"><action name="v_free"><rebind input="kb1_mouse4"/></action></actionmap>"#;
        assert!(!stays_on_mouse("v_free", &own(movable), &profile()), "it can move: it is missing until it does");
        // An emptied mouse line is the player's too.
        let emptied = r#"<actionmap name="spaceship_general"><action name="v_free"><rebind input="kb1_mouse4"/><rebind input="mo1_"/></action></actionmap>"#;
        assert_eq!(plan(&names(&["v_free"]), &own(emptied), &profile(), &[])[0].outcome, Outcome::NotSendable("mouse4".into()));
    }

    #[test]
    fn the_bar_and_the_deck_count_the_same_missing_keys() {
        // Every kind of action: a key, none, the mouse only, deleted, a mouse button that moves, one that stays, an
        // unknown name, a stick only.
        let body = r#"<actionmap name="spaceship_general">
            <action name="v_deleted"><rebind input="kb1_"/></action>
            <action name="v_free"><rebind input="kb1_mouse4"/></action>
            <action name="v_mouse"><rebind input="kb1_mouse4"/></action>
            <action name="v_free2"><rebind input="js1_button3"/></action>
        </actionmap>"#;
        let names = names(&["v_eject", "v_taken_a", "v_free", "v_free2", "v_mouse", "v_deleted", "gibt_es_nicht"]);
        let own = own(body);
        let items = plan(&names, &own, &profile(), &[]);
        let deck = names.iter().filter(|n| missing(n, &own, &profile())).count();
        assert_eq!(count(&items).without_key, deck, "{items:?}");
        assert_eq!(deck, 3, "v_free (moves), v_free2 (stick only), v_deleted; not the unknown, not the one that stays");
        // Every key full: the same number still.
        assert_eq!(count(&plan(&names, &own, &profile(), &every_key_held())).without_key, deck);
        // After putting in: nothing on either side.
        let after = r#"<actionmap name="spaceship_general">
            <action name="v_deleted"><rebind input="kb1_"/></action>
            <action name="v_free"><rebind input="kb1_lalt+lshift+b"/><rebind input="mo1_mouse4"/></action>
            <action name="v_mouse"><rebind input="kb1_mouse4"/></action>
            <action name="v_free2"><rebind input="js1_button3"/><rebind input="kb1_lalt+lshift+c"/></action>
        </actionmap>"#;
        let own = own_of_body(after);
        let deck = names.iter().filter(|n| missing(n, &own, &profile())).count();
        assert_eq!(count(&plan(&names, &own, &profile(), &[])).without_key, deck);
        assert_eq!(deck, 1, "only the one the player deleted");
    }

    fn own_of_body(body: &str) -> Vec<(String, Vec<Binding>)> {
        own(body)
    }

    #[test]
    fn a_free_key_goes_where_none_is_and_skips_what_the_game_holds() {
        let items = plan(&names(&["v_free", "v_free2"]), &own(""), &profile(), &[]);
        // lalt+lshift+a is the factory key of another action; lalt+lshift+y too, further down.
        assert_eq!(items[0].outcome, Outcome::Add("lalt+lshift+b".into()));
        assert_eq!(items[1].outcome, Outcome::Add("lalt+lshift+c".into()), "a key is given once");
        assert_eq!(items[0].group, "spaceship_general");
    }

    #[test]
    fn a_key_of_the_player_or_of_the_factory_is_never_touched() {
        let body = r#"<actionmap name="spaceship_general"><action name="v_free"><rebind input="kb1_k"/></action></actionmap>"#;
        let items = plan(&names(&["v_free", "v_eject"]), &own(body), &profile(), &[]);
        assert_eq!(items[0].outcome, Outcome::HasKey, "own keyboard line");
        assert_eq!(items[1].outcome, Outcome::HasKey, "factory key");
    }

    #[test]
    fn a_deleted_keyboard_binding_stays_deleted() {
        let body = r#"<actionmap name="spaceship_general"><action name="v_deleted"><rebind input="kb1_"/></action></actionmap>"#;
        let items = plan(&names(&["v_deleted"]), &own(body), &profile(), &[]);
        assert_eq!(items[0].outcome, Outcome::Deleted);
    }

    #[test]
    fn a_mouse_only_action_gets_a_key_beside_its_mouse_button() {
        let items = plan(&names(&["v_mouse"]), &own(""), &profile(), &[]);
        assert_eq!(items[0].outcome, Outcome::Add("lalt+lshift+b".into()));
    }

    #[test]
    fn a_hotkey_of_verse_kit_on_alt_and_a_letter_is_skipped() {
        let hotkeys = [Hotkey::parse("Alt+Umschalt+B").expect("hotkey")];
        let items = plan(&names(&["v_free"]), &own(""), &profile(), &hotkeys);
        assert_eq!(items[0].outcome, Outcome::Add("lalt+lshift+c".into()));
        let only_alt = [Hotkey::parse("Alt+B").expect("hotkey")];
        let items = plan(&names(&["v_free"]), &own(""), &profile(), &only_alt);
        assert_eq!(items[0].outcome, Outcome::Add("lalt+lshift+b".into()), "Alt+B is another key than Alt+Shift+B");
    }

    #[test]
    fn an_unknown_action_and_a_full_set_say_so() {
        let items = plan(&names(&["gibt_es_nicht"]), &own(""), &profile(), &[]);
        assert_eq!(items[0].outcome, Outcome::Unknown);
        let items = plan(&names(&["v_free"]), &own(""), &profile(), &every_key_held());
        assert_eq!(items[0].outcome, Outcome::NoFreeKey);
    }

    #[test]
    fn a_qualified_name_gets_its_key_in_its_own_actionmap_and_is_written_plain() {
        let profile = Profile::from_xml(
            r#"<ActionMaps>
              <actionmap name="spaceship_view" UILabel="@v"><action name="v_view_zoom_in" UILabel="@z"/></actionmap>
              <actionmap name="vehicle_general" UILabel="@g"><action name="v_view_zoom_in" UILabel="@z"/></actionmap>
            </ActionMaps>"#,
        )
        .expect("profile");
        let body = r#"<actionmap name="spaceship_view"><action name="v_view_zoom_in"><rebind input="kb1_lalt+lshift+6"/></action></actionmap>"#;
        let items = plan(
            &names(&["spaceship_view.v_view_zoom_in", "vehicle_general.v_view_zoom_in"]),
            &own(body),
            &profile,
            &[],
        );
        assert_eq!(items[0].outcome, Outcome::HasKey, "the ship has its key");
        assert_eq!(items[1].outcome, Outcome::Add("lalt+lshift+a".into()), "the vehicle gets one");
        assert_eq!((items[1].action.as_str(), items[1].group.as_str()), ("v_view_zoom_in", "vehicle_general"));
        let text = joysticks_write::bind_many_text(
            &format!(r#"<ActionMaps version="1"><ActionProfiles version="1" profileName="default">{body}</ActionProfiles></ActionMaps>"#),
            &[(items[1].action.clone(), items[1].group.clone(), "lalt+lshift+a".into())],
        )
        .expect("added");
        let back = own(text.split("profileName=\"default\">").nth(1).and_then(|t| t.split("</ActionProfiles>").next()).expect("body"));
        assert!(back.iter().flat_map(|(_, l)| l.iter()).any(|b| b.section == "vehicle_general" && b.input == "lalt+lshift+a"), "{text}");
        assert!(back.iter().flat_map(|(_, l)| l.iter()).any(|b| b.section == "spaceship_view" && b.input == "lalt+lshift+6"), "{text}");
    }

    #[test]
    fn a_key_the_player_gave_another_action_is_skipped_and_a_stick_only_action_gets_one_beside() {
        // lalt+lshift+b is the player's own key of another action; v_free2 lies only on the stick.
        let body = r#"<actionmap name="spaceship_general">
            <action name="v_eject"><rebind input="kb1_lalt+lshift+b"/></action>
            <action name="v_free2"><rebind input="js1_button3"/></action>
        </actionmap>"#;
        let items = plan(&names(&["v_free", "v_free2"]), &own(body), &profile(), &[]);
        assert_eq!(items[0].outcome, Outcome::Add("lalt+lshift+c".into()), "the own key of another action is taken");
        assert_eq!(items[1].outcome, Outcome::Add("lalt+lshift+d".into()), "a stick line is no keyboard key");
    }

    // ------------------------------------------------------------ over the files

    /// A made-up installation under a throwaway home with `body` as its `actionmaps.xml`; the game is quiet for `age`
    /// seconds (the age of its log).
    fn world(name: &str, age: u64, body: &[u8]) -> (Paths, String, String) {
        let home = std::env::temp_dir()
            .join(format!("versekit-sdfill-{}-{name}", std::process::id()))
            .to_string_lossy()
            .replace('\\', "/");
        let _ = std::fs::remove_dir_all(&home);
        let game = format!("{home}/StarCitizen/LIVE");
        let folder = format!("{game}/USER/Client/0/Profiles/default");
        std::fs::create_dir_all(&folder).expect("folder");
        std::fs::write(format!("{folder}/actionmaps.xml"), body).expect("maps");
        std::fs::write(format!("{game}/Game.log"), "x").expect("log");
        let at = std::time::SystemTime::now() - std::time::Duration::from_secs(age);
        std::fs::File::options().write(true).open(format!("{game}/Game.log")).and_then(|f| f.set_modified(at)).expect("age");
        let vars = [
            ("VERSEKIT_HOME", home.as_str()),
            ("HOME", home.as_str()),
            (crate::paths::ENV_INSTALL_DIR, game.as_str()),
            (crate::game_start::ENV_NO_PROCESS, "1"),
        ];
        let paths = Paths::new(crate::paths::Host::new(crate::pypath::Flavor::native(), &vars, &home));
        (paths, home.clone(), format!("{folder}/actionmaps.xml"))
    }

    const MAPS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<ActionMaps version=\"1\">\r\n <ActionProfiles version=\"1\" profileName=\"default\">\r\n  <actionmap name=\"spaceship_general\">\r\n   <action name=\"v_free2\">\r\n    <rebind input=\"js1_button3\"/>\r\n   </action>\r\n  </actionmap>\r\n </ActionProfiles>\r\n</ActionMaps>\r\n";

    fn with_bom() -> Vec<u8> {
        let mut body = vec![0xef, 0xbb, 0xbf];
        body.extend_from_slice(MAPS.as_bytes());
        body
    }

    #[test]
    fn nothing_is_written_and_no_copy_made_while_the_game_runs() {
        let (paths, home, file) = world("laeuft", 5, &with_bom());
        let items = plan(&names(&["v_free"]), &own(""), &profile(), &[]);
        let refused = apply(&paths, &items, 1).expect_err("the game runs");
        assert!(refused.game_not_quiet(), "{refused:?}");
        assert_eq!(std::fs::read(&file).expect("file"), with_bom(), "the file stays as it is");
        assert!(crate::game_files::backups(&file).is_empty(), "not even a copy");
        assert!(load_record(&paths).is_none(), "no record");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_keys_go_in_with_one_copy_keep_mark_and_line_ends_and_come_out_where_unchanged() {
        let (paths, home, file) = world("eintragen", 7200, &with_bom());
        let items = plan(&names(&["v_free", "v_free2"]), &own(""), &profile(), &[]);
        let record = apply(&paths, &items, 42).expect("written");
        assert_eq!(record.entries.len(), 2);
        assert_eq!(crate::game_files::backups(&file).len(), 1, "one copy for the whole go");
        assert!(paths.app_file(RECORD_FILE).starts_with(&home), "the record lies in the throwaway home");
        let now = std::fs::read(&file).expect("file");
        assert!(now.starts_with(&[0xef, 0xbb, 0xbf]), "the mark of the byte order stays");
        let text = String::from_utf8(now[3..].to_vec()).expect("utf8");
        assert!(!text.replace("\r\n", "").contains('\n'), "the line ends stay");
        assert!(text.contains("kb1_lalt+lshift+b") && text.contains("kb1_lalt+lshift+c") && text.contains("js1_button3"), "{text}");
        // The player gives v_free another key in the game afterwards: that line stays, only v_free2's goes.
        std::fs::write(&file, String::from_utf8(now.clone()).expect("utf8").replace("kb1_lalt+lshift+b", "kb1_k")).expect("changed");
        let found = undo(&paths).expect("undone");
        assert_eq!(found, vec!["v_free2".to_string()]);
        let after = std::fs::read_to_string(&file).expect("file");
        assert!(after.contains("kb1_k"), "the player's change stays");
        assert!(!after.contains("kb1_lalt+lshift+c") && after.contains("js1_button3"), "{after}");
        assert!(load_record(&paths).is_none(), "the record goes when it is taken back");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_count_names_the_actions_without_a_key_and_the_deleted_ones_among_them() {
        let body = r#"<actionmap name="spaceship_general"><action name="v_deleted"><rebind input="kb1_"/></action></actionmap>"#;
        let items = plan(&names(&["v_free", "v_mouse", "v_deleted", "v_eject", "gibt_es_nicht"]), &own(body), &profile(), &[]);
        assert_eq!(count(&items), Count { without_key: 3, deleted: 1, addable: 2 });
        assert_eq!(count(&[]), Count::default());
        let full = plan(&names(&["v_free"]), &own(""), &profile(), &every_key_held());
        assert_eq!(count(&full), Count { without_key: 1, deleted: 0, addable: 0 }, "no free key: still without one");
    }

    #[test]
    fn the_record_goes_out_and_comes_back() {
        let record = Record {
            file: "C:/x/actionmaps.xml".into(),
            copy: Some("C:/x/actionmaps.xml.versekit-1".into()),
            at: 5,
            entries: vec![("v_free".into(), "spaceship_general".into(), "lalt+lshift+b".into())],
            moved: vec![("v_free".into(), "spaceship_general".into(), "mouse4".into(), "lalt+lshift+b".into())],
            relocated: vec![("v_eject".into(), "spaceship_general".into(), "ralt+p".into(), "lalt+lshift+c".into())],
            refilled: vec![("v_deleted".into(), "spaceship_general".into(), "lalt+lshift+d".into())],
        };
        assert_eq!(Record::parse(&record.to_text()), Some(record));
        assert_eq!(Record::parse("{}"), None);
        let before = r#"{"version": 1, "datei": "C:/x/actionmaps.xml", "zeit": 5, "eintraege": []}"#;
        let old = Record::parse(before).expect("a record of before");
        assert!(old.moved.is_empty() && old.relocated.is_empty(), "a record of before has nothing moved");
    }

    #[test]
    fn a_key_of_the_player_with_alt_gr_moves_to_a_safe_key_and_says_why() {
        // As on the device: zoom in on Alt Gr + P, and `p` alone is the power of the weapons.
        let profile = Profile::from_xml(
            r#"<ActionMaps>
              <actionmap name="spaceship_power" UILabel="@p"><action name="v_power_toggle_weapons" UILabel="@w" keyboard="p"/></actionmap>
              <actionmap name="player" UILabel="@f">
                <action name="zoom_in" UILabel="@z"/>
                <action name="zoom_out" UILabel="@o" keyboard="ralt+o"/>
              </actionmap>
            </ActionMaps>"#,
        )
        .expect("profile");
        let body = r#"<actionmap name="player"><action name="zoom_in"><rebind input="kb1_ralt+p"/></action></actionmap>"#;
        let items = plan(&names(&["zoom_in", "zoom_out"]), &own(body), &profile, &[]);
        assert_eq!(
            items[0].outcome,
            Outcome::Relocate {
                from: "ralt+p".into(),
                key: "lalt+lshift+a".into(),
                alone: Some("v_power_toggle_weapons".into()),
                factory: false,
            },
            "the player's key with Alt Gr moves, and the line says what P alone does"
        );
        assert_eq!(
            items[1].outcome,
            Outcome::Relocate { from: "ralt+o".into(), key: "lalt+lshift+b".into(), alone: None, factory: true },
            "a factory key with Alt Gr gets a safe key of its own too"
        );
        assert!(missing("zoom_in", &own(body), &profile), "not safe: missing until it moved");
        assert!(missing("zoom_out", &own(body), &profile), "the factory key with Alt Gr is not safe either");
        assert_eq!(count(&items), Count { without_key: 2, deleted: 0, addable: 2 });
        assert!(alt_gr("ralt+p") && alt_gr("lshift+ralt+1") && !alt_gr("ralt") && !alt_gr("lalt+lshift+p"));
        // Moved: safe, nothing missing.
        let after = r#"<actionmap name="player"><action name="zoom_in"><rebind input="kb1_lalt+lshift+a"/></action></actionmap>"#;
        assert!(!missing("zoom_in", &own(after), &profile));
    }

    const ALT_GR_MAPS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<ActionMaps version=\"1\">\r\n <ActionProfiles version=\"1\" profileName=\"default\">\r\n  <actionmap name=\"spaceship_general\">\r\n   <action name=\"v_free\">\r\n    <rebind input=\"kb1_ralt+p\"/>\r\n    <rebind input=\"js1_button7\"/>\r\n   </action>\r\n  </actionmap>\r\n </ActionProfiles>\r\n</ActionMaps>\r\n";

    #[test]
    fn the_key_with_alt_gr_moves_in_the_file_and_comes_back_and_a_record_grown_old_is_dropped() {
        let (paths, home, file) = world("altgr", 7200, ALT_GR_MAPS.as_bytes());
        let read = || own_of(&std::fs::read_to_string(&file).expect("file"));
        let items = plan(&names(&["v_free"]), &read(), &profile(), &[]);
        assert!(matches!(&items[0].outcome, Outcome::Relocate { from, .. } if from == "ralt+p"), "{items:?}");
        let record = apply(&paths, &items, 42).expect("written");
        assert_eq!(record.relocated.len(), 1);
        let text = std::fs::read_to_string(&file).expect("file");
        assert!(!text.contains("kb1_ralt+p") && text.contains("kb1_lalt+lshift+b") && text.contains("js1_button7"), "{text}");
        assert_eq!(count(&plan(&names(&["v_free"]), &read(), &profile(), &[])), Count::default(), "nothing missing after");
        undo(&paths).expect("undone");
        let back = std::fs::read_to_string(&file).expect("file");
        assert!(back.contains("kb1_ralt+p") && !back.contains("lalt+lshift") && back.contains("js1_button7"), "{back}");
        // Again, and the player changes the key in the game afterwards: the way back finds nothing, drops the record,
        // changes nothing and is no failure.
        let items = plan(&names(&["v_free"]), &read(), &profile(), &[]);
        apply(&paths, &items, 43).expect("written");
        let changed = std::fs::read_to_string(&file).expect("file").replace("kb1_lalt+lshift+b", "kb1_mouse4");
        std::fs::write(&file, &changed).expect("changed in the game");
        assert_eq!(undo(&paths).expect("no failure"), Vec::<String>::new());
        assert_eq!(std::fs::read_to_string(&file).expect("file"), changed, "the player's change stays");
        assert!(load_record(&paths).is_none(), "the old way back is dropped");
        let _ = std::fs::remove_dir_all(home);
    }

    const MOUSE_MAPS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<ActionMaps version=\"1\">\r\n <ActionProfiles version=\"1\" profileName=\"default\">\r\n  <actionmap name=\"spaceship_general\">\r\n   <action name=\"v_free\">\r\n    <rebind input=\"kb1_mouse4\"/>\r\n   </action>\r\n   <action name=\"v_free2\">\r\n    <rebind input=\"js1_button3\"/>\r\n   </action>\r\n  </actionmap>\r\n </ActionProfiles>\r\n</ActionMaps>\r\n";

    #[test]
    fn after_putting_in_nothing_is_missing_the_mouse_button_still_works_and_the_way_back_restores_it() {
        let (paths, home, file) = world("maus", 7200, MOUSE_MAPS.as_bytes());
        let read = || own_of(&std::fs::read_to_string(&file).expect("file"));
        let items = plan(&names(&["v_free", "v_free2"]), &read(), &profile(), &[]);
        assert!(matches!(items[0].outcome, Outcome::Move { .. }), "{items:?}");
        let record = apply(&paths, &items, 42).expect("written");
        assert_eq!(record.moved.len(), 1);
        let text = std::fs::read_to_string(&file).expect("file");
        assert!(text.contains("mo1_mouse4") && text.contains("js1_button3"), "the mouse button moved, the stick stays: {text}");
        assert!(!text.contains("kb1_mouse4"), "{text}");
        // Afterwards nothing is missing: every action of the deck has a key it can press.
        let again = plan(&names(&["v_free", "v_free2"]), &read(), &profile(), &[]);
        assert_eq!(count(&again), Count::default(), "{again:?}");
        assert!(again.iter().all(|i| i.outcome == Outcome::HasKey));
        // The way back: the mouse button on the keyboard line again, the mouse line and the keys gone.
        undo(&paths).expect("undone");
        let back = std::fs::read_to_string(&file).expect("file");
        assert!(back.contains("kb1_mouse4") && !back.contains("mo1_mouse4") && back.contains("js1_button3"), "{back}");
        assert!(!back.contains("kb1_lalt+lshift+"), "{back}");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_writing_refuses_to_touch_any_other_mouse_line_than_the_moved_one() {
        let old = MOUSE_MAPS.to_string();
        let mice = vec!["mouse4".to_string()];
        let moved = old.replace("kb1_mouse4\"/>", "kb1_lalt+lshift+b\"/>\r\n    <rebind input=\"mo1_mouse4\"/>");
        assert!(joysticks_write::keyboard_and_these_mice(&old, &moved, &mice, true).is_ok());
        let other = old.replace("kb1_mouse4\"/>", "kb1_lalt+lshift+b\"/>\r\n    <rebind input=\"mo1_mouse5\"/>");
        assert!(joysticks_write::keyboard_and_these_mice(&old, &other, &mice, true).is_err(), "another mouse line");
        let stick = moved.replace("js1_button3", "js1_button4");
        assert!(joysticks_write::keyboard_and_these_mice(&old, &stick, &mice, true).is_err(), "a stick line changed");
    }

    #[test]
    fn a_factory_key_with_alt_gr_gets_an_own_safe_key_beside_the_stick_and_the_way_back_takes_it_out() {
        // As in the factory setting: eject on Alt Gr + Y. The player has it on a stick too.
        let profile = Profile::from_xml(
            r#"<ActionMaps>
              <actionmap name="spaceship_general" UILabel="@g"><action name="v_eject" UILabel="@e" keyboard="ralt+y"/></actionmap>
            </ActionMaps>"#,
        )
        .expect("profile");
        let maps = MAPS.replace("<action name=\"v_free2\">", "<action name=\"v_eject\">");
        let (paths, home, file) = world("altgr-werk", 7200, maps.as_bytes());
        let read = || own_of(&std::fs::read_to_string(&file).expect("file"));
        let items = plan(&names(&["v_eject"]), &read(), &profile, &[]);
        assert_eq!(
            items[0].outcome,
            Outcome::Relocate { from: "ralt+y".into(), key: "lalt+lshift+a".into(), alone: None, factory: true }
        );
        apply(&paths, &items, 42).expect("written");
        let text = std::fs::read_to_string(&file).expect("file");
        assert!(text.contains("kb1_lalt+lshift+a") && text.contains("js1_button3"), "an own keyboard line beside the stick: {text}");
        assert!(!text.contains("kb1_ralt"), "no Alt Gr written: {text}");
        assert!(!missing("v_eject", &read(), &profile), "safe now");
        undo(&paths).expect("undone");
        let back = std::fs::read_to_string(&file).expect("file");
        assert!(!back.contains("kb1_") && back.contains("js1_button3"), "the own line goes, the factory key counts again: {back}");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn one_putting_in_leaves_no_key_missing_also_for_alt_gr_and_deleted_lines_and_the_way_back_empties_them_again() {
        // As the player's file: 162 actions on the deck beside a full factory setting; 40 of them on own keys with
        // Alt Gr, 21 emptied in the game, 50 on factory keys, 51 without any key. One also lies on a stick.
        let letters = || (b'a'..=b'z').map(|c| c as char);
        let held: Vec<String> = (b'a'..=b'z')
            .chain(b'0'..=b'9')
            .map(|c| (c as char).to_string())
            .chain((1..=12).map(|n| format!("f{n}")))
            .chain((0..=9).map(|n| format!("np_{n}")))
            .chain(letters().map(|c| format!("lalt+{c}")))
            .chain(letters().map(|c| format!("lctrl+{c}")))
            .chain(letters().map(|c| format!("lshift+{c}")))
            .filter(|k| !matches!(k.as_str(), "i" | "b" | "lctrl+c"))
            .collect();
        let mut factory: Vec<String> =
            held.iter().enumerate().map(|(n, k)| format!(r#"<action name="v_held_{n}" UILabel="@h" keyboard="{k}"/>"#)).collect();
        factory.push(r#"<action name="v_power_toggle_thrusters" UILabel="@t" keyboard="i"/>"#.into());
        factory.push(r#"<action name="v_eject" UILabel="@e" keyboard="b"/>"#.into());
        factory.push(r#"<action name="v_self_destruct" UILabel="@s" keyboard="lctrl+c"/>"#.into());
        factory.extend((0..112).map(|n| format!(r#"<action name="v_deck_{n}" UILabel="@d"/>"#)));
        let xml = format!(r#"<ActionMaps><actionmap name="spaceship_general" UILabel="@g">{}</actionmap></ActionMaps>"#, factory.concat());
        let profile = Profile::from_xml(&xml).expect("profile");
        let alt_gr_keys: Vec<String> =
            (b'a'..=b'z').chain(b'0'..=b'9').map(|c| format!("ralt+{}", c as char)).chain((1..=4).map(|n| format!("ralt+f{n}"))).collect();
        let mut lines = String::new();
        for (n, key) in alt_gr_keys.iter().enumerate() {
            lines.push_str(&format!("   <action name=\"v_deck_{n}\">\r\n    <rebind input=\"kb1_{key}\"/>\r\n   </action>\r\n"));
        }
        for n in 40..61 {
            lines.push_str(&format!("   <action name=\"v_deck_{n}\">\r\n    <rebind input=\"kb1_\"/>\r\n   </action>\r\n"));
        }
        lines.push_str("   <action name=\"v_deck_61\">\r\n    <rebind input=\"js1_button3\"/>\r\n   </action>\r\n");
        let maps = MAPS.replace(
            "   <action name=\"v_free2\">\r\n    <rebind input=\"js1_button3\"/>\r\n   </action>\r\n",
            &lines,
        );
        let deck: Vec<String> =
            (0..112).map(|n| format!("v_deck_{n}")).chain((0..50).map(|n| format!("v_held_{n}"))).collect();
        assert_eq!(deck.len(), 162);
        let (paths, home, file) = world("robert", 7200, maps.as_bytes());
        let read = || own_of(&std::fs::read_to_string(&file).expect("file"));
        let missing_now = || deck.iter().filter(|n| missing(n, &read(), &profile)).count();
        assert_eq!(missing_now(), 40 + 21 + 51, "Alt Gr, deleted and without a key count as missing");
        let items = super::plan(&deck, &read(), &profile, &[], false);
        assert_eq!(count(&items), Count { without_key: 112, deleted: 0, addable: 112 }, "{items:?}");
        apply(&paths, &items, 7).expect("written");
        assert_eq!(missing_now(), 0, "one putting in, nothing missing");
        assert_eq!(count(&super::plan(&deck, &read(), &profile, &[], false)).without_key, 0, "the bar is gone");
        let text = std::fs::read_to_string(&file).expect("file");
        assert!(!text.contains("\"kb1_\"") && !text.contains("kb1_ralt") && text.contains("js1_button3"), "{text}");
        undo(&paths).expect("undone");
        let back = std::fs::read_to_string(&file).expect("file");
        assert_eq!(back.matches("\"kb1_\"").count(), 21, "the deleted lines are deleted again: {back}");
        assert_eq!(back.matches("kb1_ralt+").count(), 40, "the Alt Gr keys are back");
        assert!(back.contains("js1_button3"));
        assert_eq!(missing_now(), 40 + 21 + 51);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_deleted_line_stays_deleted_where_the_player_keeps_it() {
        let body = r#"<actionmap name="spaceship_general"><action name="v_deleted"><rebind input="kb1_"/></action></actionmap>"#;
        let kept = super::plan(&names(&["v_deleted"]), &own(body), &profile(), &[], true);
        assert_eq!(kept[0].outcome, Outcome::Deleted);
        let filled = super::plan(&names(&["v_deleted"]), &own(body), &profile(), &[], false);
        assert_eq!(filled[0].outcome, Outcome::Refill("lalt+lshift+b".into()));
        assert_eq!(count(&filled), Count { without_key: 1, deleted: 0, addable: 1 });
    }

    fn own_of(text: &str) -> Vec<(String, Vec<Binding>)> {
        bindings_from_text(text)
    }
}
