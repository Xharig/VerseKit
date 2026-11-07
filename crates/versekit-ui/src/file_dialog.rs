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

//! The system dialogs to choose a file or a folder (`file_picker` of the
//! Python version), for every page: Windows and macOS show their own
//! dialog, Linux the one of the desktop through its helper programs
//! (`kdialog`, `zenity`, see [`helpers`]) and, when none gets through, through the XDG portal.
//!
//! The dialog runs on its own thread, the window keeps drawing; the page
//! asks `Pending::take` in later frames. It belongs to the window given to
//! `remember_parent`, when there is one.

mod helpers;

use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

use helpers::Ask;

/// File types of a dialog: shown name and endings without the dot, the
/// first one chosen at the start (`(("JSON", &["json"]), ("CSV", &["csv"]))`).
pub type Filters<'a> = &'a [(&'a str, &'a [&'a str])];

/// A dialog with the window it belongs to, the start of every dialog.
static PARENT: Mutex<Option<rfd::FileDialog>> = Mutex::new(None);

/// The window the dialogs belong to (eframe's `Frame`): they open above it
/// and keep it from being used meanwhile.
pub fn remember_parent<W: HasWindowHandle + HasDisplayHandle + ?Sized>(window: &W) {
    let template = rfd::FileDialog::new().set_parent(window);
    *PARENT.lock().unwrap_or_else(PoisonError::into_inner) = Some(template);
}

/// How a dialog ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    /// A file or a folder was chosen.
    Chosen(String),
    /// The dialog was closed without a choice.
    Cancelled,
    /// No dialog could be shown.
    Failed,
}

impl Answer {
    /// The path chosen, `None` for a cancel and a failure alike.
    pub fn into_path(self) -> Option<String> {
        match self {
            Answer::Chosen(path) => Some(path),
            Answer::Cancelled | Answer::Failed => None,
        }
    }
}

/// A dialog that is open or was closed: `take` gives `Some` once, when it
/// was closed (`Some(None)`: cancelled or no dialog could be shown; `take_answer` tells the two apart).
#[derive(Clone, Debug, Default)]
pub struct Pending(Arc<Mutex<Option<Answer>>>);

impl Pending {
    pub fn take(&self) -> Option<Option<String>> {
        self.take_answer().map(Answer::into_path)
    }

    /// How the dialog ended, once, when it was closed.
    pub fn take_answer(&self) -> Option<Answer> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).take()
    }

    /// Already answered, for a test that stands in for a dialog: `None` is a cancel.
    #[cfg(test)]
    pub(crate) fn answered(answer: Option<String>) -> Pending {
        Pending::ended(answer.map_or(Answer::Cancelled, Answer::Chosen))
    }

    fn ended(answer: Answer) -> Pending {
        Pending(Arc::new(Mutex::new(Some(answer))))
    }
}

/// The answer of a dialog of the system: a path or a cancel.
fn picked(path: Option<std::path::PathBuf>) -> Answer {
    path.map_or(Answer::Cancelled, |p| Answer::Chosen(p.to_string_lossy().into_owned()))
}

fn dialog(title: &str, filters: Filters, start: Option<&str>) -> rfd::FileDialog {
    let template = PARENT.lock().unwrap_or_else(PoisonError::into_inner).clone();
    let mut dialog = template.unwrap_or_default().set_title(title);
    for (name, endings) in filters {
        dialog = dialog.add_filter(*name, endings);
    }
    match start.filter(|s| Path::new(s).is_dir()) {
        Some(folder) => dialog.set_directory(folder),
        None => dialog,
    }
}

/// Runs the dialog on its own thread: first the helpers of the desktop (`helpers`, none off Linux), then, when
/// none gets through, the dialog of the system (`pick`). A new frame is asked for when it closes.
fn run(
    ctx: &egui::Context,
    helpers: Vec<Vec<String>>,
    origin: &'static str,
    pick: impl FnOnce() -> Answer + Send + 'static,
) -> Pending {
    let pending = Pending::default();
    let (slot, ctx) = (Arc::clone(&pending.0), ctx.clone());
    let started = std::thread::Builder::new().name("dateidialog".into()).spawn(move || {
        let answer = helpers::ask_desktop(&helpers, origin).unwrap_or_else(pick);
        *slot.lock().unwrap_or_else(PoisonError::into_inner) = Some(answer);
        ctx.request_repaint();
    });
    if started.is_ok() { pending } else { Pending::ended(Answer::Failed) }
}

/// A test must never open a system dialog (it would land on the screen of whoever runs it): while a test has
/// started the stand-in, a call is only noted and the dialog stays open without an answer.
#[cfg(test)]
pub(crate) mod stand_in {
    use std::cell::RefCell;

    use super::Pending;

    thread_local! {
        static CALLS: RefCell<Option<Vec<&'static str>>> = const { RefCell::new(None) };
    }

    /// From now on, on this thread, no system dialog opens.
    pub(crate) fn start() {
        CALLS.with(|calls| *calls.borrow_mut() = Some(Vec::new()));
    }

    /// Ends the stand-in; the kinds of the dialogs that were asked for (`open`, `save`, `folder`), in order.
    pub(crate) fn stop() -> Vec<&'static str> {
        CALLS.with(|calls| calls.borrow_mut().take().unwrap_or_default())
    }

    pub(super) fn call(kind: &'static str) -> Option<Pending> {
        CALLS.with(|calls| {
            calls.borrow_mut().as_mut().map(|list| {
                list.push(kind);
                Pending::default()
            })
        })
    }
}

/// An existing file, as text.
pub fn open_file(ctx: &egui::Context, title: &str, filters: Filters, start: Option<&str>) -> Pending {
    #[cfg(test)]
    if let Some(pending) = stand_in::call("open") {
        return pending;
    }
    let ask = Ask::Open(filters);
    let dialog = dialog(title, filters, start);
    run(ctx, helpers::for_here(ask, title, start), ask.origin(), move || picked(dialog.pick_file()))
}

/// Where to save a file, as text; `name` is the suggested file name.
pub fn save_file(ctx: &egui::Context, title: &str, filters: Filters, start: Option<&str>, name: &str) -> Pending {
    #[cfg(test)]
    if let Some(pending) = stand_in::call("save") {
        return pending;
    }
    let ask = Ask::Save(filters, name);
    let dialog = dialog(title, filters, start).set_file_name(name);
    run(ctx, helpers::for_here(ask, title, start), ask.origin(), move || picked(dialog.save_file()))
}

/// A folder, as text.
pub fn choose_folder(ctx: &egui::Context, title: &str, start: Option<&str>) -> Pending {
    #[cfg(test)]
    if let Some(pending) = stand_in::call("folder") {
        return pending;
    }
    let ask = Ask::Folder;
    let dialog = dialog(title, &[], start);
    run(ctx, helpers::for_here(ask, title, start), ask.origin(), move || picked(dialog.pick_folder()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_is_given_once() {
        let pending = Pending::answered(Some("a.json".into()));
        assert_eq!(pending.take(), Some(Some("a.json".to_string())));
        assert_eq!(pending.take(), None);
        let open = Pending::default();
        assert_eq!(open.take(), None, "still open");
    }

    /// A cancel and a failure are two answers; a page that only asks for the path sees neither as a choice.
    #[test]
    fn a_cancel_and_a_failure_are_told_apart_and_neither_is_a_path() {
        assert_eq!(Pending::answered(None).take_answer(), Some(Answer::Cancelled));
        assert_eq!(Pending::ended(Answer::Failed).take_answer(), Some(Answer::Failed));
        assert_eq!(Pending::ended(Answer::Failed).take(), Some(None));
        assert_eq!(Pending::answered(None).take(), Some(None));
        assert_eq!(picked(None), Answer::Cancelled, "the dialog of the system closed without a choice");
        assert_eq!(picked(Some("/a/b.zip".into())), Answer::Chosen("/a/b.zip".into()));
    }
}
