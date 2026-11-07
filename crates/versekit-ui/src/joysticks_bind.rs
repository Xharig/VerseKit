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

//! The window "Neu belegen" (`binding_window.py` of 4.12): "press the key, the button or the mouse button you
//! want now".
//!
//! 1. The player clicks a line of the list.
//! 2. The window **waits for an input**: button of a stick, key, mouse button, all on an equal footing.
//! 3. What was detected stands there at once, with who already sits on that input.
//! 4. **Only a click on "Übernehmen" writes.** Nothing happens on the side.
//!
//! ⚠⚠ **Why not write as soon as something is detected?** Because the detection can be wrong: a shaky axis, a
//! button that bounces on letting go. Between "detected" and "written" belongs a person: the whole control of
//! the player hangs on that file.
//!
//! Two ways in, on purpose:
//!
//! | What | How it is detected |
//! |---|---|
//! | stick, pedals, game pad | the device (a thread of its own, `input_device::wait`) |
//! | keyboard, mouse | the events of **this window** while it is open |
//!
//! ⚠ The keyboard is expressly **not** read all over the system: that would be a key logger. Only this window
//! listens, while it is open. A mouse button counts only inside the box: a click on a button of the window is
//! no binding.
//!
//! **Modifiers and the number block** come from the raw key events, not from the sign:
//!
//! | What | How it is told |
//! |---|---|
//! | a modifier alone (`lshift`, `ralt`) | the place of the key (`physical_key`): the toolkit names the side there. It counts when let go with no other key in between |
//! | a key with modifiers (`lctrl+lshift+s`) | the modifiers held down in the box, by side, in the order Ctrl, Alt, Shift. A modifier the flags of the toolkit report without its key having gone down in the box counts as the left one |
//! | the number block (`np_5`, `np_enter`) | Windows: the scan code in the messages of the own window ([`NumberBlockTap`]). Elsewhere only where the toolkit gives it away: Num Lock off (`End` on the place of `1`), `np_add`, `np_period` |
//!
//! ⚠ **What the window cannot do:** on Linux and macOS, with Num Lock on, the figures, `np_subtract`,
//! `np_divide`, `np_enter` and `np_multiply` of the number block read as their twins of the main block. The
//! Windows and Command keys are no modifiers of the game: a key with them is refused with a line that says so.
//! Escape closes the window and cannot be bound; Tab moves the focus and cannot be bound either. Right Alt on a
//! German layout (Alt Gr) also reports Ctrl: without a Ctrl key down in the box that Ctrl is left out.
//!
//! ⌨ **The keyboard works the window, too:** the keys are taken only while the box has the focus (it gets it when
//! the window opens). Tab leaves the box for the buttons, where Enter and Space press them as everywhere.
//!
//! ⚠ Not known: whether the game takes a key by its place on the keyboard (`y`) or by the sign it makes. The
//! window takes the sign (`Key`), not the place; on a German keyboard `Z` and `Y` could be exchanged.
//! To be tried by the player against `pp_rebindkeys export`. With a modifier held, a letter is still taken by its
//! sign; any other key by its place, because Shift turns a figure into another sign (`Shift+7` is `/`).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};

use egui::{Context, Key, PointerButton, RichText, Ui};
use versekit_core::controls::{self, Devices};
use versekit_core::input_device::{self, NumberBlockTap, Pressed};
use versekit_core::joysticks_write;
use versekit_core::language::{Arg, t, tf};
use versekit_core::paths::Paths;
use versekit_core::theme::Scheme;

use crate::settings::Said;
use crate::widgets::{self, ButtonKind};
use crate::{color, control_radius};

/// How long a stick is waited for before the thread starts again, in seconds.
const PATIENCE: f64 = 20.0;

/// The name the game gives a key; `None` where the key can not be bound sensibly. Escape is not here: it
/// closes the window.
pub fn key_name(key: Key) -> Option<String> {
    let fixed = match key {
        Key::Backspace => "backspace",
        Key::Tab => "tab",
        Key::Enter => "enter",
        Key::Space => "space",
        Key::ArrowUp => "up",
        Key::ArrowDown => "down",
        Key::ArrowLeft => "left",
        Key::ArrowRight => "right",
        Key::Home => "home",
        Key::End => "end",
        Key::PageUp => "pgup",
        Key::PageDown => "pgdn",
        Key::Insert => "insert",
        Key::Delete => "delete",
        Key::Comma => "comma",
        Key::Period => "period",
        Key::Slash => "slash",
        Key::Minus => "minus",
        Key::Equals => "equals",
        Key::Semicolon => "semicolon",
        Key::Quote => "apostrophe",
        Key::Backtick => "grave",
        Key::Backslash => "backslash",
        Key::OpenBracket => "lbracket",
        Key::CloseBracket => "rbracket",
        Key::F1 => "f1",
        Key::F2 => "f2",
        Key::F3 => "f3",
        Key::F4 => "f4",
        Key::F5 => "f5",
        Key::F6 => "f6",
        Key::F7 => "f7",
        Key::F8 => "f8",
        Key::F9 => "f9",
        Key::F10 => "f10",
        Key::F11 => "f11",
        Key::F12 => "f12",
        Key::Num0 => "0",
        Key::Num1 => "1",
        Key::Num2 => "2",
        Key::Num3 => "3",
        Key::Num4 => "4",
        Key::Num5 => "5",
        Key::Num6 => "6",
        Key::Num7 => "7",
        Key::Num8 => "8",
        Key::Num9 => "9",
        Key::CapsLock => "capslock",
        Key::Pause => "pause",
        Key::ControlLeft => "lctrl",
        Key::ControlRight => "rctrl",
        Key::AltLeft => "lalt",
        Key::AltRight => "ralt",
        Key::ShiftLeft => "lshift",
        Key::ShiftRight => "rshift",
        other => {
            // The letters: `A` to `Z` stand in a row in the enum.
            let (first, last, at) = (Key::A as usize, Key::Z as usize, other as usize);
            return (first..=last)
                .contains(&at)
                .then(|| char::from(b'a' + u8::try_from(at - first).unwrap_or(0)).to_string());
        }
    };
    Some(fixed.to_string())
}

/// The modifier keys by the name the game gives them, in the order the game writes them in front of a key.
const MODIFIERS: [(Key, &str); 6] = [
    (Key::ControlLeft, "lctrl"),
    (Key::ControlRight, "rctrl"),
    (Key::AltLeft, "lalt"),
    (Key::AltRight, "ralt"),
    (Key::ShiftLeft, "lshift"),
    (Key::ShiftRight, "rshift"),
];

/// The figures of the main block and the names of their twins on the number block.
const FIGURES: [(Key, &str); 10] = [
    (Key::Num0, "np_0"),
    (Key::Num1, "np_1"),
    (Key::Num2, "np_2"),
    (Key::Num3, "np_3"),
    (Key::Num4, "np_4"),
    (Key::Num5, "np_5"),
    (Key::Num6, "np_6"),
    (Key::Num7, "np_7"),
    (Key::Num8, "np_8"),
    (Key::Num9, "np_9"),
];

/// What the keys of the number block read as with Num Lock off, in the order of [`FIGURES`].
const NUM_LOCK_OFF: [Key; 10] = [
    Key::Insert,
    Key::End,
    Key::ArrowDown,
    Key::PageDown,
    Key::ArrowLeft,
    Key::Clear,
    Key::ArrowRight,
    Key::Home,
    Key::ArrowUp,
    Key::PageUp,
];

/// The name of a modifier key (`lshift`), by its place first.
fn modifier_name(key: Key, physical: Option<Key>) -> Option<&'static str> {
    let place = physical.unwrap_or(key);
    MODIFIERS.iter().find(|(known, _)| *known == place).map(|(_, name)| *name)
}

/// The Windows and the Command key: no modifiers the game knows.
fn is_system_key(key: Key, physical: Option<Key>) -> bool {
    matches!(physical.unwrap_or(key), Key::SuperLeft | Key::SuperRight)
}

fn is_letter(key: Key) -> bool {
    (Key::A as usize..=Key::Z as usize).contains(&(key as usize))
}

/// Is the key event the toolkit made of the number block key `name`?
fn number_block_twin(name: &str, key: Key, physical: Option<Key>) -> bool {
    match name {
        "np_enter" => physical == Some(Key::Enter),
        "np_add" => physical == Some(Key::Plus),
        "np_subtract" => physical == Some(Key::Minus),
        "np_divide" => physical == Some(Key::Slash),
        "np_period" => physical.is_none() && matches!(key, Key::Period | Key::Comma | Key::Delete),
        _ => FIGURES.iter().any(|(figure, known)| *known == name && physical == Some(*figure)),
    }
}

/// The key of the number block where the toolkit gives it away by itself: a figure that reads as an arrow
/// (Num Lock off), the plus key (no other key has that place), the decimal key (the toolkit knows no place
/// for it).
fn number_block_guess(key: Key, physical: Option<Key>) -> Option<&'static str> {
    match physical {
        Some(Key::Plus) => Some("np_add"),
        Some(place) => FIGURES
            .iter()
            .zip(NUM_LOCK_OFF)
            .find(|((figure, _), off)| *figure == place && *off == key)
            .map(|((_, name), _)| *name),
        None => matches!(key, Key::Period | Key::Comma | Key::Delete).then_some("np_period"),
    }
}

/// The name of a key that is no modifier. With a modifier held a letter is taken by its sign, any other key by
/// its place: Shift turns a figure into another sign.
fn base_name(key: Key, physical: Option<Key>, modified: bool) -> Option<String> {
    if !modified || is_letter(key) {
        return key_name(key);
    }
    physical.and_then(key_name).or_else(|| key_name(key))
}

/// The modifiers in front of a key: those held down in the box, by side; a modifier the flags report without
/// its key held counts as the left one. ⚠ Right Alt on a German layout (Alt Gr) reports Ctrl as well: without
/// a Ctrl key held that Ctrl is left out.
fn modifier_prefix(modifiers: egui::Modifiers, held: &[&'static str]) -> Vec<&'static str> {
    let alt_gr = held.contains(&"ralt");
    let mut out = Vec::new();
    for (on, left, right) in
        [(modifiers.ctrl, "lctrl", "rctrl"), (modifiers.alt, "lalt", "ralt"), (modifiers.shift, "lshift", "rshift")]
    {
        if !on {
            continue;
        }
        let sides: Vec<&'static str> = [left, right].into_iter().filter(|side| held.contains(side)).collect();
        if !sides.is_empty() {
            out.extend(sides);
        } else if !(left == "lctrl" && alt_gr) {
            out.push(left);
        }
    }
    out
}

/// The flags that go with the modifiers held.
fn flags_of(held: &[&'static str]) -> egui::Modifiers {
    let has = |a: &str, b: &str| held.contains(&a) || held.contains(&b);
    egui::Modifiers {
        ctrl: has("lctrl", "rctrl"),
        alt: has("lalt", "ralt"),
        shift: has("lshift", "rshift"),
        ..egui::Modifiers::NONE
    }
}

/// `lctrl+lshift+s`.
fn joined(prefix: &[&str], key: &str) -> String {
    prefix.iter().copied().chain(std::iter::once(key)).collect::<Vec<_>>().join("+")
}

/// What the toolkit made of cut, copy and paste: it swallows the key and gives this instead.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Clip {
    Cut,
    Copy,
    Paste,
}

/// One thing the keyboard did in the box.
#[derive(Clone, Copy, Debug)]
enum Stroke {
    Down {
        key: Key,
        physical: Option<Key>,
        modifiers: egui::Modifiers,
    },
    Up {
        key: Key,
        physical: Option<Key>,
    },
    Clipboard(Clip),
    /// A key left to the toolkit (Tab).
    Passed,
}

/// What the strokes of a frame come to.
#[derive(Debug, Default, PartialEq, Eq)]
struct Read {
    /// The inputs detected, in the order they were pressed (the last counts).
    names: Vec<String>,
    /// A key with the Windows or the Command key was pressed: not taken.
    refused: bool,
}

/// The modifier keys down in the box, kept from frame to frame.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Held {
    /// By side (`lshift`); `super` for the Windows or the Command key.
    down: Vec<&'static str>,
    /// The modifier pressed last with no other key since: it counts on its own when it is let go.
    alone: Option<&'static str>,
}

/// The names of the game for the strokes of a frame. `tapped` are the keys of the number block the window saw in
/// this frame ([`NumberBlockTap`]): each one replaces the key event the toolkit made of it; one without such an
/// event (`np_multiply`) comes last.
///
/// A modifier alone counts when it is let go with no other key pressed in between: Shift held for Shift+Tab or
/// for `lshift+s` is no binding of its own.
fn read_strokes(strokes: &[Stroke], held: &mut Held, tapped: &[&'static str]) -> Read {
    let mut read = Read::default();
    let mut tapped: Vec<&'static str> = tapped.to_vec();
    for stroke in strokes {
        match *stroke {
            Stroke::Passed => held.alone = None,
            Stroke::Up { key, physical } => {
                let side = if is_system_key(key, physical) { Some("super") } else { modifier_name(key, physical) };
                let Some(side) = side else { continue };
                held.down.retain(|h| *h != side);
                if held.alone.take() == Some(side) {
                    read.names.push(side.to_string());
                }
            }
            Stroke::Down { key, physical, modifiers } => {
                if is_system_key(key, physical) {
                    if !held.down.contains(&"super") {
                        held.down.push("super");
                    }
                    held.alone = None;
                    read.refused = true;
                    continue;
                }
                if let Some(side) = modifier_name(key, physical) {
                    if !held.down.contains(&side) {
                        held.down.push(side);
                    }
                    held.alone = Some(side);
                    continue;
                }
                held.alone = None;
                if modifiers.mac_cmd || held.down.contains(&"super") {
                    read.refused = true;
                    continue;
                }
                let block = match tapped.iter().position(|name| number_block_twin(name, key, physical)) {
                    Some(at) => Some(tapped.remove(at)),
                    None => number_block_guess(key, physical),
                };
                let modified = modifiers.shift || modifiers.ctrl || modifiers.alt;
                let Some(base) = block.map(str::to_string).or_else(|| base_name(key, physical, modified)) else {
                    continue;
                };
                read.names.push(joined(&modifier_prefix(modifiers, &held.down), &base));
            }
            Stroke::Clipboard(clip) => {
                held.alone = None;
                let mut flags = flags_of(&held.down);
                let key = match (clip, flags.ctrl, flags.shift) {
                    (Clip::Cut, false, true) => "delete",
                    (Clip::Paste, false, true) => "insert",
                    (Clip::Cut, _, _) => "x",
                    (Clip::Copy, _, _) => "c",
                    (Clip::Paste, _, _) => "v",
                };
                if !flags.ctrl && !flags.shift {
                    flags.ctrl = true;
                }
                read.names.push(joined(&modifier_prefix(flags, &held.down), key));
            }
        }
    }
    if !tapped.is_empty() {
        held.alone = None;
    }
    let flags = flags_of(&held.down);
    for name in tapped {
        read.names.push(joined(&modifier_prefix(flags, &held.down), name));
    }
    read
}

/// The name the game gives a mouse button. ⚠ Many toolkits count the middle button as 2 and the right one as
/// 3, Star Citizen the other way round (`mouse2` is the right one): without the swap every binding sits on the
/// wrong button.
pub fn mouse_name(button: PointerButton) -> Option<&'static str> {
    match button {
        PointerButton::Primary => Some("mouse1"),
        PointerButton::Secondary => Some("mouse2"),
        PointerButton::Middle => Some("mouse3"),
        _ => None,
    }
}

/// What the prompt of the window says.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Prompt {
    /// "Press the key now."
    Press,
    /// Something was detected: "Mistyped? Press again."
    Again,
    /// No stick can be detected: keyboard and mouse still go.
    OnlyKeyboard,
    /// The stick stands in no binding: the game would have to see it first.
    Foreign,
    /// Two sticks of the same kind sit on the computer: which one was pressed is not told by the system.
    Same,
    /// A key with the Windows or the Command key was pressed: not taken.
    System,
}

/// A window that is open.
struct Open {
    action: String,
    /// The group (`spaceship_general`); empty where not known (the usual one then).
    section: String,
    /// The plain name of the action (or the action itself).
    name: String,
    /// The device the line belongs to (`js1`), empty for an action nothing points to.
    device_id: String,
    /// What lay on the input before, in plain words.
    previous: String,
    /// What was detected: (device, input without prefix).
    detected: Option<(String, String)>,
    prompt: Prompt,
    /// The names of those who already sit on the input.
    conflict: String,
    /// Why the last writing failed.
    failure: Option<String>,
}

/// The thread that waits for a stick.
struct Listener {
    answer: Receiver<Option<Pressed>>,
    stop: Arc<AtomicBool>,
    started: std::time::Instant,
}

/// A thread that ends sooner than this has found nothing to wait for (no readable device): it is not started
/// again and again, the window falls back to keyboard and mouse.
const QUICK: std::time::Duration = std::time::Duration::from_secs(2);

/// The id of the box that takes the keys.
fn box_id() -> egui::Id {
    egui::Id::unique("joysticks-bind-box")
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// What the window keeps.
#[derive(Default)]
pub struct BindState {
    open: Option<Open>,
    listener: Option<Listener>,
    /// Keys taken at the start of the frame, worked in when the window is drawn.
    keys: Vec<String>,
    /// A key with the Windows or the Command key was refused in this frame.
    refused: bool,
    /// The modifiers down in the box, kept from frame to frame.
    held: Held,
    /// The keys of the number block, from the messages of the window (Windows).
    tap: Option<NumberBlockTap>,
    /// The box gets the focus when it is drawn (set when the window opens).
    focus_box: bool,
    /// Sticks can be read at all (asked when the window opens: the question is not cheap on Windows).
    sticks: bool,
    /// The thread for sticks ended at once: not started again.
    gave_up: bool,
    /// The reading of sticks can be left out (the tests: no hardware).
    #[cfg(test)]
    no_sticks: bool,
    /// The wait for a press of a stick, replaced in the tests.
    waiter: Option<Waiter>,
}

/// Waits for a press: (seconds, whether to stop).
pub(crate) type Waiter = Arc<dyn Fn(f64, &dyn Fn() -> bool) -> Option<Pressed> + Send + Sync>;

/// What the window knows about the file and the sticks, from the page.
pub struct BindInfo<'a> {
    pub paths: &'a Paths,
    /// Why nothing may be written now.
    pub quiet: Option<&'a str>,
    /// The sticks in the binding: (number, id).
    pub sticks: Vec<(i64, String)>,
    /// The view "all" for the check of who sits on an input.
    pub all: &'a Devices,
    /// The plain name of an action, if the game has one.
    pub label: &'a dyn Fn(&str) -> Option<String>,
}

/// What came of the window in this frame.
pub enum Outcome {
    /// Nothing worth telling.
    Nothing,
    /// A change was made: the line for the foot; the files are to be read again.
    Written(Said),
    /// A change was made that has nothing to tell; the files are to be read again.
    Refresh,
}

/// The line "already on": the names of those who sit on the input (at most three), or empty.
fn conflict_text(info: &BindInfo<'_>, action: &str, device: &str, input: &str) -> String {
    let entries = controls::conflicts(info.all, action, device, input);
    if entries.is_empty() {
        return String::new();
    }
    let names: Vec<String> = entries
        .iter()
        .take(3)
        .map(|e| (info.label)(&e.action).filter(|n| !n.is_empty()).unwrap_or_else(|| e.action.clone()))
        .collect();
    tf("s_js_b_konflikt", &[("0", Arg::from(names.join(", ").as_str()))])
}

impl BindState {
    /// No stick is read from now on (the tests: no hardware).
    #[cfg(test)]
    pub(crate) fn without_sticks(&mut self) {
        self.no_sticks = true;
    }

    /// Sticks are there, and this is what waiting for them says (the tests).
    #[cfg(test)]
    pub(crate) fn with_waiter(&mut self, waiter: Waiter) {
        self.no_sticks = false;
        self.waiter = Some(waiter);
    }

    /// Is the window open?
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// Opens the window for a line.
    pub fn open(&mut self, action: &str, section: &str, name: &str, device_id: &str, previous: &str) {
        self.listener = None;
        self.keys.clear();
        self.refused = false;
        self.held = Held::default();
        self.tap = NumberBlockTap::start();
        self.gave_up = false;
        self.focus_box = true;
        self.sticks = self.sticks_available();
        let prompt = if self.sticks { Prompt::Press } else { Prompt::OnlyKeyboard };
        self.open = Some(Open {
            action: action.to_string(),
            section: section.to_string(),
            name: name.to_string(),
            device_id: device_id.to_string(),
            previous: previous.to_string(),
            detected: None,
            prompt,
            conflict: String::new(),
            failure: None,
        });
    }

    fn sticks_available(&self) -> bool {
        #[cfg(test)]
        if self.no_sticks {
            return false;
        }
        #[cfg(test)]
        if self.waiter.is_some() {
            return true;
        }
        input_device::available()
    }

    /// Closes the window and lets the thread go.
    pub fn close(&mut self) {
        self.open = None;
        self.listener = None;
        self.keys.clear();
        self.held = Held::default();
        self.tap = None;
    }

    /// Takes the keys of this frame **before** the page is drawn: a key that is bound must not at the same time
    /// press a button of the page (Space, Enter). Only while the box has the focus: with the focus on a button the
    /// keys do what they do everywhere (Tab moves on, Enter presses). Escape closes the window in any case.
    pub fn take_keys(&mut self, ctx: &Context) {
        if self.open.is_none() {
            return;
        }
        let in_box = ctx.memory(|memory| memory.has_focus(box_id())) || self.focus_box;
        // Taken in every frame: what was pressed while the focus was elsewhere does not count later.
        let tapped = self.tap.as_ref().map(NumberBlockTap::take).unwrap_or_default();
        let mut escape = false;
        let mut strokes: Vec<Stroke> = Vec::new();
        ctx.input_mut(|input| {
            input.events.retain(|event| match event {
                egui::Event::Key { key: Key::Escape, pressed: true, .. } => {
                    escape = true;
                    false
                }
                // Tab is left to the toolkit: it moves the focus out of the box.
                egui::Event::Key { key: Key::Tab, pressed, .. } => {
                    if *pressed && in_box {
                        strokes.push(Stroke::Passed);
                    }
                    true
                }
                _ if !in_box => true,
                egui::Event::Key { key, physical_key, pressed: true, repeat: false, modifiers } => {
                    strokes.push(Stroke::Down { key: *key, physical: *physical_key, modifiers: *modifiers });
                    false
                }
                egui::Event::Key { key, physical_key, pressed: false, .. } => {
                    strokes.push(Stroke::Up { key: *key, physical: *physical_key });
                    false
                }
                egui::Event::Cut => {
                    strokes.push(Stroke::Clipboard(Clip::Cut));
                    false
                }
                egui::Event::Copy => {
                    strokes.push(Stroke::Clipboard(Clip::Copy));
                    false
                }
                egui::Event::Paste(_) | egui::Event::PasteImage(_) => {
                    strokes.push(Stroke::Clipboard(Clip::Paste));
                    false
                }
                // The repeat and the text of a key that was taken go too.
                egui::Event::Key { .. } | egui::Event::Text(_) => false,
                _ => true,
            });
        });
        if escape {
            self.close();
            return;
        }
        if !in_box {
            // A modifier let go outside the box (Shift+Tab) is no binding.
            self.held.alone = None;
            return;
        }
        let read = read_strokes(&strokes, &mut self.held, &tapped);
        self.refused |= read.refused;
        self.keys.extend(read.names);
    }

    /// Something was detected: shown, not written. Whoever mistyped presses once more.
    fn detect(&mut self, device: &str, input: &str, info: &BindInfo<'_>) {
        let Some(open) = &mut self.open else { return };
        open.conflict = conflict_text(info, &open.action, device, input);
        open.detected = Some((device.to_string(), input.to_string()));
        open.prompt = Prompt::Again;
        open.failure = None;
    }

    /// Starts the thread for sticks if none runs, takes what it found.
    fn listen(&mut self, ctx: &Context, info: &BindInfo<'_>) {
        if let Some(listener) = &self.listener {
            let quick = listener.started.elapsed() < QUICK;
            match listener.answer.try_recv() {
                Ok(found) => {
                    self.listener = None;
                    let Some(pressed) = found else {
                        // Ended at once: nothing to wait for. Not started again in every frame (that would
                        // ask the system ten times a second); keyboard and mouse still go.
                        if quick {
                            self.gave_up = true;
                            if let Some(open) = &mut self.open
                                && open.prompt == Prompt::Press
                            {
                                open.prompt = Prompt::OnlyKeyboard;
                            }
                        }
                        return;
                    };
                    let numbers: Vec<i64> = info
                        .sticks
                        .iter()
                        .filter(|(_, id)| id.eq_ignore_ascii_case(&pressed.id))
                        .map(|(number, _)| *number)
                        .collect();
                    let prompt = match numbers.as_slice() {
                        [number] => {
                            self.detect(&format!("js{number}"), &pressed.input, info);
                            None
                        }
                        // Two sticks of one kind share the id: which was pressed is not told. Said instead
                        // of guessed (4.12 took the first, an earlier version here the last).
                        [_, _, ..] => Some(Prompt::Same),
                        // The device stands in no binding: which number the game gives it is not known.
                        [] => Some(Prompt::Foreign),
                    };
                    if let (Some(prompt), Some(open)) = (prompt, &mut self.open) {
                        open.prompt = prompt;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                    return;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.listener = None,
            }
        }
        if self.listener.is_none() && self.sticks && !self.gave_up {
            let stop = Arc::new(AtomicBool::new(false));
            let flag = Arc::clone(&stop);
            let (sender, answer) = mpsc::channel();
            let waiter = self.waiter.clone();
            let spawned = std::thread::Builder::new().name("joysticks-press".into()).spawn(move || {
                let stopped = || flag.load(Ordering::Relaxed);
                let found = match waiter {
                    Some(wait) => wait(PATIENCE, &stopped),
                    None => input_device::wait(PATIENCE, &stopped),
                };
                let _ = sender.send(found);
            });
            if spawned.is_ok() {
                self.listener = Some(Listener { answer, stop, started: std::time::Instant::now() });
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    /// Draws the window; gives what came of it.
    pub fn show(&mut self, ctx: &Context, scheme: &Scheme, info: &BindInfo<'_>) -> Outcome {
        if self.open.is_none() {
            return Outcome::Nothing;
        }
        self.listen(ctx, info);
        if std::mem::take(&mut self.refused)
            && let Some(open) = &mut self.open
        {
            open.prompt = Prompt::System;
        }
        for name in std::mem::take(&mut self.keys) {
            self.detect("kb1", &name, info);
        }
        let want_focus = std::mem::take(&mut self.focus_box);
        let first = t("s_js_b_erst");
        let mut from_box: Option<(String, String)> = None;
        let (mut take, mut clear, mut cancel) = (false, false, false);
        let Some(open) = &self.open else { return Outcome::Nothing };
        let modal = egui::Modal::new(egui::Id::unique("joysticks-bind")).frame(egui::Frame::NONE).show(ctx, |ui| {
            ui.set_max_width(widgets::DIALOG_WIDTH + 80.0);
            widgets::card(ui, scheme, |ui| {
                ui.set_min_width(ui.available_width());
                let title = widgets::bold(ui, t("s_js_b_titel"), egui::TextStyle::Body);
                ui.label(title);
                ui.add_space(4.0);
                ui.add(egui::Label::new(RichText::new(&open.name).strong()).wrap());
                let section = if open.section.is_empty() { String::new() } else { format!(" · {}", open.section) };
                let sub = format!("{}{section}", open.device_id);
                if !sub.trim_start_matches(" · ").is_empty() {
                    ui.add(
                        egui::Label::new(RichText::new(sub.trim_start_matches(" · ")).small().color(color(scheme.sub)))
                            .wrap(),
                    );
                }
                if !open.previous.is_empty() {
                    ui.add_space(4.0);
                    let line = tf("s_js_b_bisher", &[("0", Arg::from(open.previous.as_str()))]);
                    ui.add(egui::Label::new(RichText::new(line).small().color(color(scheme.sub))).wrap());
                }
                ui.add_space(8.0);
                let (prompt, hue) = match open.prompt {
                    Prompt::Press => (t("s_js_b_druecke"), scheme.fg),
                    Prompt::Again => (t("s_js_b_nochmal"), scheme.sub),
                    Prompt::OnlyKeyboard => (t("s_js_b_nur_tastatur"), scheme.fg),
                    Prompt::Foreign => (t("s_js_b_fremd"), scheme.gold),
                    Prompt::Same => (t("s_js_b_gleich"), scheme.gold),
                    Prompt::System => (t("s_js_b_systemtaste"), scheme.gold),
                };
                ui.add(egui::Label::new(RichText::new(prompt).color(color(hue))).wrap());
                ui.add_space(8.0);
                from_box = capture_box(ui, scheme, open, info, want_focus);
                if !open.conflict.is_empty() {
                    ui.add_space(4.0);
                    ui.add(egui::Label::new(RichText::new(&open.conflict).small().color(color(scheme.gold))).wrap());
                }
                ui.add_space(4.0);
                ui.add(egui::Label::new(RichText::new(t("s_js_spiel_zu")).small().color(color(scheme.sub))).wrap());
                if let Some(reason) = &open.failure {
                    ui.add(egui::Label::new(RichText::new(reason).small().color(color(scheme.red))).wrap());
                }
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    let label = t("s_js_b_uebernehmen");
                    let kind = if open.detected.is_some() { ButtonKind::Strong } else { ButtonKind::Normal };
                    // Nothing to take over before something is detected; the game must be quiet.
                    let reason = info.quiet.or_else(|| open.detected.is_none().then_some(first.as_str()));
                    take = ui.add(widgets::Button::new(&label, kind, scheme).blocked(reason)).clicked();
                    if !open.device_id.is_empty() {
                        let label = t("s_js_b_loeschen");
                        clear = ui
                            .add(widgets::Button::new(&label, ButtonKind::Danger, scheme).blocked(info.quiet))
                            .clicked();
                    }
                    cancel = ui.add(widgets::Button::new(&t("s_js_b_abbruch"), ButtonKind::Normal, scheme)).clicked();
                });
                if let Some(reason) = info.quiet {
                    ui.add(egui::Label::new(RichText::new(reason).small().color(color(scheme.sub))).wrap());
                }
            });
        });
        if let Some((device, input)) = from_box {
            self.detect(&device, &input, info);
        }
        // A click beside the window closes it, like every other dialog.
        if cancel || modal.should_close() {
            self.close();
            return Outcome::Nothing;
        }
        let Some(open) = &self.open else { return Outcome::Nothing };
        let write = if take {
            open.detected.clone()
        } else if clear {
            Some((open.device_id.clone(), String::new()))
        } else {
            None
        };
        let Some((device, input)) = write else { return Outcome::Nothing };
        let (action, section) = (open.action.clone(), open.section.clone());
        match joysticks_write::bind_action(info.paths, &action, &section, &device, &input, None, None) {
            Ok(done) => {
                let copy = done.backup.as_deref().map(crate::joysticks_page::base_name).unwrap_or_default();
                self.close();
                Outcome::Written(Said::note(tf("s_js_fertig", &[("0", Arg::from(copy.as_str()))])))
            }
            Err(failure) => {
                if let Some(open) = &mut self.open {
                    open.failure = Some(tf("s_js_schief", &[("0", Arg::from(failure.text().as_str()))]));
                }
                Outcome::Nothing
            }
        }
    }
}

/// The box in which a mouse button or the wheel counts, with what was detected in it. Gives a detection.
fn capture_box(
    ui: &mut Ui,
    scheme: &Scheme,
    open: &Open,
    info: &BindInfo<'_>,
    want_focus: bool,
) -> Option<(String, String)> {
    let radius = control_radius(scheme);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 64.0), egui::Sense::hover());
    let response = ui.interact(rect, box_id(), egui::Sense::click());
    if want_focus {
        response.request_focus();
    }
    if response.has_focus() {
        // The arrow keys are keys to bind here, not a way to move the focus (Tab still moves it).
        let filter = egui::EventFilter { tab: false, horizontal_arrows: true, vertical_arrows: true, escape: false };
        ui.memory_mut(|memory| memory.set_focus_lock_filter(response.id, filter));
    }
    ui.painter().rect_filled(rect, radius, color(scheme.field));
    let edge = if response.hovered() { color(scheme.accent) } else { color(scheme.line) };
    ui.painter().rect_stroke(rect, radius, egui::Stroke::new(1.0, edge), egui::StrokeKind::Inside);
    let (main, sub) = match &open.detected {
        Some((device, input)) => {
            let readable = controls::input_readable(input, controls::device_kind(device));
            let who = info
                .sticks
                .iter()
                .find(|(number, _)| *device == format!("js{number}"))
                .map_or_else(|| device.clone(), |(_, id)| id.split('-').next().unwrap_or(id).to_string());
            (readable, format!("{who} · {device}_{input}"))
        }
        None => (t("s_js_b_leer"), String::new()),
    };
    let inner = rect.shrink2(egui::vec2(8.0, 6.0));
    let hue = if open.detected.is_some() { color(scheme.accent) } else { color(scheme.sub) };
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Center)),
        |ui| {
            // Not selectable: a label that can be selected takes the click away from the box under it.
            ui.add(egui::Label::new(RichText::new(&main).heading().strong().color(hue)).wrap().selectable(false));
            if !sub.is_empty() {
                ui.add(egui::Label::new(RichText::new(&sub).small().color(color(scheme.sub))).wrap().selectable(false));
            }
        },
    );
    // The name says what the box does; what it holds follows as the value (a screen reader reads both).
    let label = tf("s_js_b_feld", &[("0", Arg::from(main.as_str()))]);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
    widgets::focus_ring(ui, &response, scheme, radius);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    // The mouse buttons and the wheel, only here.
    for button in [PointerButton::Primary, PointerButton::Secondary, PointerButton::Middle] {
        if response.clicked_by(button)
            && let Some(name) = mouse_name(button)
        {
            return Some(("mo1".to_string(), name.to_string()));
        }
    }
    if response.hovered() {
        let wheel = ui.input(|i| i.smooth_scroll_delta.y);
        if wheel != 0.0 {
            return Some(("mo1".to_string(), if wheel > 0.0 { "mwheel_up" } else { "mwheel_down" }.to_string()));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_that_is_taken_no_longer_reaches_the_page() {
        let ctx = Context::default();
        let mut state = BindState::default();
        state.without_sticks();
        state.open("a", "s", "n", "kb1", "");
        let key = Key::F5;
        let event = egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let raw = egui::RawInput { events: vec![event], ..Default::default() };
        let mut left = usize::MAX;
        let mut output = ctx.run_ui(raw, |ui| {
            state.take_keys(ui.ctx());
            left = ui.ctx().input(|i| i.events.len());
        });
        output.textures_delta.clear();
        assert_eq!(state.keys, ["f5"]);
        assert_eq!(left, 0, "the key is taken: the page must not see it");
    }

    #[test]
    fn the_keys_get_the_names_the_game_writes() {
        assert_eq!(key_name(Key::A).as_deref(), Some("a"));
        assert_eq!(key_name(Key::M).as_deref(), Some("m"));
        assert_eq!(key_name(Key::Z).as_deref(), Some("z"), "the letters stand in a row in the enum");
        assert_eq!(key_name(Key::Num0).as_deref(), Some("0"));
        assert_eq!(key_name(Key::Num9).as_deref(), Some("9"));
        assert_eq!(key_name(Key::F1).as_deref(), Some("f1"));
        assert_eq!(key_name(Key::F12).as_deref(), Some("f12"));
        assert_eq!(key_name(Key::F13), None, "the game has no such key");
        assert_eq!(key_name(Key::PageUp).as_deref(), Some("pgup"));
        assert_eq!(key_name(Key::PageDown).as_deref(), Some("pgdn"));
        assert_eq!(key_name(Key::Quote).as_deref(), Some("apostrophe"));
        assert_eq!(key_name(Key::Backtick).as_deref(), Some("grave"));
        assert_eq!(key_name(Key::OpenBracket).as_deref(), Some("lbracket"));
        assert_eq!(key_name(Key::Equals).as_deref(), Some("equals"));
        assert_eq!(key_name(Key::Escape), None, "Escape closes the window");
        assert_eq!(key_name(Key::Copy), None);
        assert_eq!(key_name(Key::CapsLock).as_deref(), Some("capslock"));
        assert_eq!(key_name(Key::Pause).as_deref(), Some("pause"));
        assert_eq!(key_name(Key::ShiftLeft).as_deref(), Some("lshift"));
        assert_eq!(key_name(Key::ShiftRight).as_deref(), Some("rshift"));
        assert_eq!(key_name(Key::ControlRight).as_deref(), Some("rctrl"));
        assert_eq!(key_name(Key::AltRight).as_deref(), Some("ralt"));
        assert_eq!(key_name(Key::SuperLeft), None, "the game knows no Windows key");
        // Every letter once and in order.
        let letters: Vec<String> = [
            Key::A,
            Key::B,
            Key::C,
            Key::D,
            Key::E,
            Key::F,
            Key::G,
            Key::H,
            Key::I,
            Key::J,
            Key::K,
            Key::L,
            Key::M,
            Key::N,
            Key::O,
            Key::P,
            Key::Q,
            Key::R,
            Key::S,
            Key::T,
            Key::U,
            Key::V,
            Key::W,
            Key::X,
            Key::Y,
            Key::Z,
        ]
        .into_iter()
        .filter_map(key_name)
        .collect();
        assert_eq!(letters.concat(), "abcdefghijklmnopqrstuvwxyz");
    }

    fn down(key: Key, physical: Key, modifiers: egui::Modifiers) -> Stroke {
        Stroke::Down { key, physical: Some(physical), modifiers }
    }

    fn up(key: Key) -> Stroke {
        Stroke::Up { key, physical: Some(key) }
    }

    /// The names the strokes come to, with a fresh state.
    fn names(strokes: &[Stroke], tapped: &[&'static str]) -> Vec<String> {
        read_strokes(strokes, &mut Held::default(), tapped).names
    }

    const NONE: egui::Modifiers = egui::Modifiers::NONE;
    const SHIFT: egui::Modifiers = egui::Modifiers::SHIFT;
    const CTRL: egui::Modifiers = egui::Modifiers::CTRL;

    #[test]
    fn a_modifier_alone_counts_on_letting_go_and_names_its_side() {
        let shift_right = [down(Key::ShiftRight, Key::ShiftRight, SHIFT), up(Key::ShiftRight)];
        assert_eq!(names(&shift_right, &[]), ["rshift"]);
        let ctrl_left = [down(Key::ControlLeft, Key::ControlLeft, CTRL), up(Key::ControlLeft)];
        assert_eq!(names(&ctrl_left, &[]), ["lctrl"]);
        let alt_gr = [down(Key::AltRight, Key::AltRight, egui::Modifiers::ALT), up(Key::AltRight)];
        assert_eq!(names(&alt_gr, &[]), ["ralt"]);
        let caps = [down(Key::CapsLock, Key::CapsLock, NONE)];
        assert_eq!(names(&caps, &[]), ["capslock"], "Caps Lock is a key of its own, taken at once");
        // Held down over two frames: nothing while held, the side when let go.
        let mut held = Held::default();
        assert!(read_strokes(&[down(Key::ShiftLeft, Key::ShiftLeft, SHIFT)], &mut held, &[]).names.is_empty());
        assert_eq!(read_strokes(&[up(Key::ShiftLeft)], &mut held, &[]).names, ["lshift"]);
        // Shift held for Shift+Tab is no binding.
        let tab = [down(Key::ShiftLeft, Key::ShiftLeft, SHIFT), Stroke::Passed, up(Key::ShiftLeft)];
        assert!(names(&tab, &[]).is_empty());
    }

    #[test]
    fn a_key_with_modifiers_is_joined_with_the_sides_held_in_the_order_of_the_game() {
        let shift_s = [down(Key::ShiftLeft, Key::ShiftLeft, SHIFT), down(Key::S, Key::S, SHIFT), up(Key::ShiftLeft)];
        assert_eq!(names(&shift_s, &[]), ["lshift+s"], "the Shift used for the key does not count on its own");
        let both = CTRL | SHIFT;
        let three = [
            down(Key::ShiftLeft, Key::ShiftLeft, SHIFT),
            down(Key::ControlRight, Key::ControlRight, both),
            down(Key::F5, Key::F5, both),
        ];
        assert_eq!(names(&three, &[]), ["rctrl+lshift+f5"], "Ctrl in front of Shift, whatever went down first");
        // The flags say Ctrl, no Ctrl key went down in the box: the left one.
        assert_eq!(names(&[down(Key::S, Key::S, CTRL)], &[]), ["lctrl+s"]);
        // Alt Gr reports Ctrl as well: left out without a Ctrl key down.
        let alt_gr = egui::Modifiers { ctrl: true, alt: true, ..NONE };
        let at = [down(Key::AltRight, Key::AltRight, alt_gr), down(Key::Q, Key::Q, alt_gr)];
        assert_eq!(names(&at, &[]), ["ralt+q"]);
        // Shift makes `/` of the 7 on a German layout: the place counts, not the sign.
        let seven = [down(Key::ShiftLeft, Key::ShiftLeft, SHIFT), down(Key::Slash, Key::Num7, SHIFT)];
        assert_eq!(names(&seven, &[]), ["lshift+7"]);
        // A letter keeps its sign.
        let z = [down(Key::ShiftLeft, Key::ShiftLeft, SHIFT), down(Key::Z, Key::Y, SHIFT)];
        assert_eq!(names(&z, &[]), ["lshift+z"]);
        // Cut, copy and paste come without their key.
        let mut held = Held::default();
        read_strokes(&[down(Key::ControlLeft, Key::ControlLeft, CTRL)], &mut held, &[]);
        assert_eq!(read_strokes(&[Stroke::Clipboard(Clip::Copy)], &mut held, &[]).names, ["lctrl+c"]);
        let mut held = Held::default();
        read_strokes(&[down(Key::ShiftRight, Key::ShiftRight, SHIFT)], &mut held, &[]);
        assert_eq!(read_strokes(&[Stroke::Clipboard(Clip::Cut)], &mut held, &[]).names, ["rshift+delete"]);
        assert_eq!(names(&[Stroke::Clipboard(Clip::Paste)], &[]), ["lctrl+v"]);
    }

    #[test]
    fn the_windows_key_is_refused_and_takes_nothing() {
        let read = read_strokes(
            &[down(Key::SuperLeft, Key::SuperLeft, NONE), down(Key::S, Key::S, NONE)],
            &mut Held::default(),
            &[],
        );
        assert!(read.refused && read.names.is_empty(), "{read:?}");
        let read = read_strokes(&[down(Key::S, Key::S, egui::Modifiers::MAC_CMD)], &mut Held::default(), &[]);
        assert!(read.refused && read.names.is_empty(), "{read:?}");
        let mut held = Held::default();
        read_strokes(&[down(Key::SuperLeft, Key::SuperLeft, NONE), up(Key::SuperLeft)], &mut held, &[]);
        assert_eq!(read_strokes(&[down(Key::S, Key::S, NONE)], &mut held, &[]).names, ["s"], "let go: keys go again");
    }

    #[test]
    fn the_number_block_replaces_the_figure_the_toolkit_made_of_it() {
        assert_eq!(names(&[down(Key::Num5, Key::Num5, NONE)], &["np_5"]), ["np_5"]);
        assert_eq!(names(&[down(Key::Num5, Key::Num5, NONE)], &[]), ["5"], "the row of figures");
        assert_eq!(names(&[down(Key::Enter, Key::Enter, NONE)], &["np_enter"]), ["np_enter"]);
        assert_eq!(names(&[down(Key::Minus, Key::Minus, NONE)], &["np_subtract"]), ["np_subtract"]);
        assert_eq!(names(&[down(Key::Slash, Key::Slash, NONE)], &["np_divide"]), ["np_divide"]);
        let decimal = [Stroke::Down { key: Key::Comma, physical: None, modifiers: NONE }];
        assert_eq!(names(&decimal, &["np_period"]), ["np_period"]);
        // The toolkit gives no key event for the multiply key: it comes last.
        assert_eq!(names(&[down(Key::F2, Key::F2, NONE)], &["np_multiply"]), ["f2", "np_multiply"]);
        // A tapped key matches its own twin, not the next key.
        let two = [down(Key::Num1, Key::Num1, NONE), down(Key::Num2, Key::Num2, NONE)];
        assert_eq!(names(&two, &["np_2"]), ["1", "np_2"]);
        // With a modifier.
        let ctrl_enter = [down(Key::ControlLeft, Key::ControlLeft, CTRL), down(Key::Enter, Key::Enter, CTRL)];
        assert_eq!(names(&ctrl_enter, &["np_enter"]), ["lctrl+np_enter"]);
        // Without the messages of the window: what the toolkit gives away by itself.
        assert_eq!(names(&[down(Key::End, Key::Num1, NONE)], &[]), ["np_1"], "Num Lock off");
        assert_eq!(names(&[down(Key::Clear, Key::Num5, NONE)], &[]), ["np_5"], "Num Lock off");
        assert_eq!(names(&[down(Key::Plus, Key::Plus, NONE)], &[]), ["np_add"]);
        assert_eq!(names(&decimal, &[]), ["np_period"]);
        assert_eq!(names(&[down(Key::End, Key::End, NONE)], &[]), ["end"], "the End key itself");
    }

    #[test]
    fn the_mouse_buttons_swap_middle_and_right_like_the_game_counts_them() {
        assert_eq!(mouse_name(PointerButton::Primary), Some("mouse1"));
        assert_eq!(mouse_name(PointerButton::Secondary), Some("mouse2"), "the right one is mouse2");
        assert_eq!(mouse_name(PointerButton::Middle), Some("mouse3"), "the middle one is mouse3");
        assert_eq!(mouse_name(PointerButton::Extra1), None);
    }
}
