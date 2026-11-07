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

//! Which stick is which, and how do I know (`device_hub.py` of 4.12, the block "Deine Geräte").
//!
//! ## Three sources that name the same device differently
//!
//! | Source | What it knows | What it does not know |
//! |---|---|---|
//! | the system ([`input_device::devices`]) | what is connected **now** | nothing about the game |
//! | the `Game.log` ([`joysticks::devices`]) | what the game saw last | whether it is still there |
//! | the `actionmaps.xml` ([`joysticks::assignment`]) | which `js` number the binding means | whether the device exists |
//!
//! ⚠⚠ **And the numbers do not agree.** Measured on a setup with three devices: the left stick was `js0` for the
//! system and `js2` for the game. Whoever says "js1" must say whose js1 he means.
//!
//! What joins them is the **id**, the same curly string in all three. ⚠ **Never over the name**: the same
//! device is called `VIRPIL Controls 20241226 L-VPC Stick WarBRD-D` by the system, `L-VPC Stick WarBRD-D` by
//! the log and `LEFT VPC Stick WarBRD-D` by the binding.
//!
//! This part only **reads**; swapping an id is in [`joysticks`] and goes over `game_files`.

use crate::input_device::SystemDevice;
use crate::joysticks::{self, Assigned, Device};
use crate::paths::Paths;

/// The state of a device in the hub.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Status {
    /// Connected, known to the game, has a number.
    Ready,
    /// Connected, but the binding does not know it: in the game it does nothing.
    NoNumber,
    /// The binding expects it, but it is not there.
    Unplugged,
    /// Connected, and the game has never seen it.
    Unknown,
}

impl Status {
    /// The word of 4.12 (`bereit`, `ohne_nummer`, `abgesteckt`, `unbekannt`); the texts are `s_gh_<word>`.
    pub fn word(self) -> &'static str {
        match self {
            Status::Ready => "bereit",
            Status::NoNumber => "ohne_nummer",
            Status::Unplugged => "abgesteckt",
            Status::Unknown => "unbekannt",
        }
    }
}

/// One device of the hub.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The common point of reference (capital letters).
    pub id: String,
    /// The first eight characters: they tell the devices apart already.
    pub short: String,
    /// The best name there is: binding before log before system.
    pub name: String,
    pub status: Status,
    /// The `js` number of the game, if the binding has one.
    pub number: Option<i64>,
    /// `/dev/input/js0` or the device path of Windows, or empty.
    pub system_path: String,
    pub system_name: String,
    /// Is it there **now**?
    pub connected: bool,
    /// Has the game ever seen it?
    pub in_game: bool,
}

/// The first eight characters of an id.
fn short(id: &str) -> String {
    id.chars().take(8).collect()
}

/// All devices from all three sources, joined over the id, sorted: what has a number first (by number), then
/// the rest by name. ⚠ Two with the same number or name stay in the order of their id, so the list does not
/// jump (4.12 left that to the order of a set).
pub fn overview_from(system: &[SystemDevice], seen: &[Device], stored: &[Assigned]) -> Vec<Entry> {
    // Later ones of the same id win in each source, like the dictionaries of 4.12.
    let mut live: Vec<(String, &SystemDevice)> = Vec::new();
    for device in system.iter().filter(|d| !d.id.is_empty()) {
        upsert(&mut live, device.id.to_uppercase(), device);
    }
    let mut in_log: Vec<(String, &Device)> = Vec::new();
    for device in seen.iter().filter(|d| !d.id.is_empty()) {
        upsert(&mut in_log, device.id.to_uppercase(), device);
    }
    let mut in_use: Vec<(String, &Assigned)> = Vec::new();
    for entry in stored.iter().filter(|e| !e.id.is_empty()) {
        upsert(&mut in_use, entry.id.to_uppercase(), entry);
    }

    let mut ids: Vec<&String> = live.iter().map(|(id, _)| id).chain(in_log.iter().map(|(id, _)| id)).collect();
    ids.extend(in_use.iter().map(|(id, _)| id));
    ids.sort();
    ids.dedup();

    let mut out: Vec<Entry> = ids
        .into_iter()
        .map(|id| {
            let on_system = pick(&live, id);
            let logged = pick(&in_log, id);
            let bound = pick(&in_use, id);
            // ⚠ The name of the **binding** wins: that is the one the player saw last, and often he gave it
            // himself. The name of the system is the longest, but also the most unwieldy.
            let name =
                [bound.map(|b| b.name.as_str()), logged.map(|l| l.name.as_str()), on_system.map(|s| s.name.as_str())]
                    .into_iter()
                    .flatten()
                    .find(|n| !n.is_empty())
                    .unwrap_or("")
                    .to_string();
            let status = match (bound.is_some(), on_system.is_some(), logged.is_some()) {
                (true, true, _) => Status::Ready,
                (true, false, _) => Status::Unplugged,
                (false, true, true) => Status::NoNumber,
                (false, true, false) => Status::Unknown,
                // Only in the log and nowhere else: was there once, is gone, has no binding. That is old stock,
                // no state of its own.
                (false, false, _) => Status::Unplugged,
            };
            Entry {
                id: id.clone(),
                short: short(id),
                name,
                status,
                number: bound.map(|b| b.number),
                system_path: on_system.map(|s| s.path.clone()).unwrap_or_default(),
                system_name: on_system.map(|s| s.name.clone()).unwrap_or_default(),
                connected: on_system.is_some(),
                in_game: logged.is_some(),
            }
        })
        .collect();
    out.sort_by(|a, b| {
        (a.number.is_none(), a.number.unwrap_or(0), a.name.to_lowercase(), &a.id).cmp(&(
            b.number.is_none(),
            b.number.unwrap_or(0),
            b.name.to_lowercase(),
            &b.id,
        ))
    });
    out
}

fn pick<'a, T>(list: &[(String, &'a T)], id: &str) -> Option<&'a T> {
    list.iter().find(|(known, _)| known == id).map(|(_, value)| *value)
}

fn upsert<'a, T>(list: &mut Vec<(String, &'a T)>, id: String, value: &'a T) {
    match list.iter_mut().find(|(known, _)| *known == id) {
        Some(slot) => slot.1 = value,
        None => list.push((id, value)),
    }
}

/// The hub read from the files: `system` is what [`input_device::devices`] gave.
pub fn overview(paths: &Paths, system: &[SystemDevice], folder: Option<&str>, file: Option<&str>) -> Vec<Entry> {
    overview_from(system, &joysticks::devices(paths, folder), &joysticks::assignment(paths, file, folder))
}

/// The hub in numbers, for a headline that names the state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    pub devices: Vec<Entry>,
    pub ready: usize,
    pub no_number: usize,
    pub unplugged: usize,
    pub unknown: usize,
    /// Nothing is missing, nothing hangs about without a number.
    pub all_well: bool,
}

/// [`Summary`] of a list.
pub fn summary_of(devices: Vec<Entry>) -> Summary {
    let count = |status: Status| devices.iter().filter(|d| d.status == status).count();
    let (ready, no_number, unplugged, unknown) =
        (count(Status::Ready), count(Status::NoNumber), count(Status::Unplugged), count(Status::Unknown));
    // ⚠ A device that stands only in the log (old stock: no binding, not plugged in) is listed as "not plugged
    // in" but is no problem: 4.12 counted it and so never saw "all well" with an old stick in the log.
    let missing = devices.iter().filter(|d| d.status == Status::Unplugged && d.number.is_some()).count();
    Summary { ready, no_number, unplugged, unknown, all_well: no_number + missing + unknown == 0, devices }
}

/// [`overview`] as a [`Summary`].
pub fn summary(paths: &Paths, system: &[SystemDevice], folder: Option<&str>, file: Option<&str>) -> Summary {
    summary_of(overview(paths, system, folder, file))
}

/// What the assistant offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// The same device under a new id: hang the binding on it.
    Swap,
    /// The game does not know it yet: start it once.
    Start,
    /// The binding expects it: plug it in or tidy up.
    PlugIn,
}

/// One step of the assistant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    pub step: Step,
    /// The device concerned.
    pub device: Entry,
    /// For the swap: the entry whose binding is taken over.
    pub old: Option<Entry>,
}

/// What is to be done: concrete steps instead of bare states.
///
/// ## The case it is built for
///
/// A stick gets a new id (another USB port, new firmware). Then the same device stands **twice**: once
/// unplugged (the old id, to which the whole binding is hung) and once without number (the new one). Who does
/// not know sees two problems where there is one, and the cure is one move.
///
/// ## ⚠⚠ Nothing is guessed
///
/// A step to swap comes up **only** where exactly **one** device is missing and exactly **one** new stands
/// there. With more it would be guessing, and a wrongly guessed replacement swaps two sticks, which one only
/// notices in battle. And ⚠ the **name** plays no part, however tempting.
pub fn suggestions_of(items: &[Entry]) -> Vec<Suggestion> {
    let of = |status: Status| -> Vec<&Entry> { items.iter().filter(|d| d.status == status).collect() };
    // ⚠ Only a device with a binding is missing: one that stands only in the log has nothing to take over, and
    // "there is a binding for it" would be untrue (4.12 counted it and so held back a swap that was clear).
    let missing: Vec<&Entry> = of(Status::Unplugged).into_iter().filter(|d| d.number.is_some()).collect();
    let without = of(Status::NoNumber);
    let fresh = of(Status::Unknown);

    // ⚠ A device the game does not know yet is a candidate as well: whether the game has seen the new id
    // depends only on whether a round was played since. For "is this the same stick" that says nothing.
    let candidates: Vec<&Entry> = without.iter().chain(fresh.iter()).copied().collect();
    if missing.len() == 1 && candidates.len() == 1 {
        return vec![Suggestion { step: Step::Swap, device: candidates[0].clone(), old: Some(missing[0].clone()) }];
    }
    let mut out: Vec<Suggestion> =
        fresh.iter().map(|d| Suggestion { step: Step::Start, device: (*d).clone(), old: None }).collect();
    out.extend(missing.iter().map(|d| Suggestion { step: Step::PlugIn, device: (*d).clone(), old: None }));
    out
}

/// A mark of the files the hub reads: path, time and size of every binding file; of the running `Game.log`
/// the file itself (identity) and its size, the size only up to [`joysticks::LOG_HEAD`] bytes (about as much as
/// [`joysticks::devices`] reads), so a log that grows all the time in the game triggers nothing after that. A
/// new `Game.log` at the next start has a new identity.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SourceStamp(Vec<(String, u128, u64)>);

/// The mark of the files; empty where there is no game folder.
pub fn source_stamp(paths: &Paths, folder: Option<&str>) -> SourceStamp {
    let Some(base) = folder.map(str::to_string).filter(|f| !f.is_empty()).or_else(|| paths.game_folder()) else {
        return SourceStamp::default();
    };
    let mut out = Vec::new();
    for path in joysticks::all_actionmaps(paths, Some(&base)) {
        let Ok(info) = std::fs::metadata(&path) else { continue };
        let at = info.modified().map_or(0, |t| u128::try_from(crate::paths::nanos(t)).unwrap_or(0));
        out.push((path, at, info.len()));
    }
    if let Some(log) = paths.game_log(Some(&base))
        && let Ok(info) = std::fs::metadata(&log)
    {
        let (device, file) = crate::paths::file_identity(&log);
        out.push((log, u128::from(device) << 64 | u128::from(file), info.len().min(joysticks::LOG_HEAD as u64)));
    }
    // The newest kept log: `devices` falls back to it when the running one holds no device.
    if let Some(backup) = paths.log_backups(Some(&base)).pop()
        && let Ok(info) = std::fs::metadata(&backup)
    {
        let at = info.modified().map_or(0, |t| u128::try_from(crate::paths::nanos(t)).unwrap_or(0));
        out.push((backup, at, info.len()));
    }
    SourceStamp(out)
}

/// Notes when a device comes or goes: only **asks**, does not listen. Every [`Watchdog::check`] gets the list
/// once and compares it with the one before. The first call only sets the basis: telling everything as new
/// would be a false alarm at every start of the program.
#[derive(Debug, Default)]
pub struct Watchdog {
    state: Option<Vec<SystemDevice>>,
    stamp: Option<SourceStamp>,
}

impl Watchdog {
    /// Has the game written binding or log anew since the last call? The first call only sets the basis and
    /// answers no. ⚠ Without this look the overview stays at "no number" after plugging in: the game enters the
    /// numbers at its start, and nobody plugs anything in then.
    pub fn sources_changed(&mut self, stamp: SourceStamp) -> bool {
        match self.stamp.replace(stamp.clone()) {
            None => false,
            Some(before) => before != stamp,
        }
    }

    /// `(came, went)` since the last call; at the first call always empty.
    pub fn check(&mut self, current: &[SystemDevice]) -> (Vec<SystemDevice>, Vec<SystemDevice>) {
        let current: Vec<SystemDevice> = {
            let mut list: Vec<SystemDevice> = Vec::new();
            for device in current.iter().filter(|d| !d.id.is_empty()) {
                let id = device.id.to_uppercase();
                match list.iter_mut().find(|known| known.id.to_uppercase() == id) {
                    Some(slot) => *slot = device.clone(),
                    None => list.push(device.clone()),
                }
            }
            list
        };
        let Some(before) = self.state.replace(current.clone()) else { return (Vec::new(), Vec::new()) };
        let known = |list: &[SystemDevice], id: &str| list.iter().any(|d| d.id.to_uppercase() == id.to_uppercase());
        let came = current.iter().filter(|d| !known(&before, &d.id)).cloned().collect();
        let went = before.iter().filter(|d| !known(&current, &d.id)).cloned().collect();
        (came, went)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sys(id: &str, name: &str) -> SystemDevice {
        SystemDevice { path: format!("/dev/input/{name}"), name: name.into(), id: id.into() }
    }

    fn logged(slot: u64, id: &str, name: &str) -> Device {
        Device { slot, name: name.into(), id: id.into() }
    }

    fn bound(number: i64, id: &str, name: &str) -> Assigned {
        Assigned { number, name: name.into(), id: id.into() }
    }

    #[test]
    fn the_three_sources_are_joined_over_the_id_and_the_name_of_the_binding_wins() {
        let list = overview_from(
            &[sys("aaaa", "System A"), sys("BBBB", "System B"), sys("CCCC", "System C"), sys("", "No id")],
            &[logged(0, "AAAA", "Log A"), logged(1, "BBBB", "Log B"), logged(2, "DDDD", "Log D")],
            &[bound(2, "AAAA", "Binding A"), bound(1, "EEEE", "Binding E")],
        );
        let line: Vec<(&str, &str, Option<i64>, &str)> =
            list.iter().map(|d| (d.id.as_str(), d.name.as_str(), d.number, d.status.word())).collect();
        assert_eq!(
            line,
            [
                ("EEEE", "Binding E", Some(1), "abgesteckt"),
                ("AAAA", "Binding A", Some(2), "bereit"),
                ("BBBB", "Log B", None, "ohne_nummer"),
                ("DDDD", "Log D", None, "abgesteckt"),
                ("CCCC", "System C", None, "unbekannt"),
            ],
            "numbers first, then the rest by name; only in the log: old stock, counted as unplugged"
        );
        assert_eq!(list[1].short, "AAAA");
        assert!(list[1].connected && list[1].in_game && !list[0].connected);
        assert_eq!(list[1].system_name, "System A");
    }

    #[test]
    fn a_number_of_zero_is_a_number() {
        let list = overview_from(&[sys("AAAA", "S")], &[], &[bound(0, "AAAA", "B")]);
        assert_eq!(list[0].number, Some(0));
        assert_eq!(list[0].status, Status::Ready);
    }

    #[test]
    fn the_summary_counts_the_states_and_says_when_all_is_well() {
        let fine = summary_of(overview_from(&[sys("A", "S")], &[logged(0, "A", "L")], &[bound(1, "A", "B")]));
        assert!(fine.all_well);
        assert_eq!((fine.ready, fine.no_number, fine.unplugged, fine.unknown), (1, 0, 0, 0));
        let not =
            summary_of(overview_from(&[sys("A", "S"), sys("B", "T")], &[logged(0, "A", "L")], &[bound(1, "C", "B")]));
        assert!(!not.all_well);
        assert_eq!((not.ready, not.no_number, not.unplugged, not.unknown), (0, 1, 1, 1));
        assert!(summary_of(Vec::new()).all_well, "nothing there, nothing missing");
    }

    #[test]
    fn the_assistant_offers_the_swap_only_for_one_missing_and_one_new_device() {
        // One missing, one without a number: the swap.
        let one = overview_from(&[sys("B", "S")], &[logged(0, "B", "L")], &[bound(1, "A", "Old")]);
        let steps = suggestions_of(&one);
        assert_eq!(steps.len(), 1);
        assert_eq!((steps[0].step, steps[0].device.id.as_str()), (Step::Swap, "B"));
        assert_eq!(steps[0].old.as_ref().map(|o| o.id.as_str()), Some("A"));

        // A device the game has never seen is a candidate as well.
        let fresh = overview_from(&[sys("B", "S")], &[], &[bound(1, "A", "Old")]);
        assert_eq!(suggestions_of(&fresh)[0].step, Step::Swap);

        // Two missing: nothing is guessed, each gets its own step.
        let two = overview_from(&[sys("C", "S")], &[], &[bound(1, "A", "Old"), bound(2, "B", "Old 2")]);
        let steps = suggestions_of(&two);
        assert_eq!(steps.iter().map(|s| s.step).collect::<Vec<_>>(), [Step::Start, Step::PlugIn, Step::PlugIn]);

        // Two new ones: no swap either.
        let many = overview_from(&[sys("C", "S"), sys("D", "T")], &[], &[bound(1, "A", "Old")]);
        assert!(suggestions_of(&many).iter().all(|s| s.step != Step::Swap));

        assert!(suggestions_of(&overview_from(&[], &[], &[])).is_empty());

        // The name plays no part: three devices with the same name, one missing, two new: no swap.
        let same = overview_from(&[sys("C", "Stick"), sys("D", "Stick")], &[], &[bound(1, "A", "Stick")]);
        assert!(suggestions_of(&same).iter().all(|s| s.step != Step::Swap));
    }

    #[test]
    fn a_device_that_stands_only_in_the_log_is_old_stock_and_neither_a_problem_nor_a_binding() {
        // Old stock: in the log, no binding, not plugged in.
        let old = overview_from(&[sys("A", "S")], &[logged(0, "A", "L"), logged(1, "Z", "Old")], &[bound(1, "A", "B")]);
        assert_eq!(old.iter().find(|d| d.id == "Z").map(|d| d.status), Some(Status::Unplugged), "listed");
        assert!(summary_of(old.clone()).all_well, "but no reason for worry");
        assert!(suggestions_of(&old).is_empty(), "and no advice to plug in a binding that does not exist");
        // It does not hold back a swap that is clear: one bound device missing, one new, one old stock.
        let swap = overview_from(&[sys("N", "New")], &[logged(1, "Z", "Old")], &[bound(1, "A", "Gone")]);
        let steps = suggestions_of(&swap);
        assert_eq!((steps.len(), steps[0].step), (1, Step::Swap), "{steps:?}");
    }

    #[test]
    fn the_watchdog_tells_who_came_and_who_went_and_never_at_the_first_call() {
        let mut watch = Watchdog::default();
        let (a, b) = (sys("AAAA", "A"), sys("BBBB", "B"));
        assert_eq!(
            watch.check(std::slice::from_ref(&a)),
            (Vec::new(), Vec::new()),
            "the first call only sets the basis"
        );
        assert_eq!(watch.check(std::slice::from_ref(&a)), (Vec::new(), Vec::new()));
        let (came, went) = watch.check(&[a.clone(), b.clone()]);
        assert_eq!((came, went), (vec![b.clone()], Vec::new()));
        let (came, went) = watch.check(&[sys("bbbb", "B")]);
        assert_eq!((came.len(), went), (0, vec![a]), "the case of the id does not matter");
        // The same id twice in another spelling is one device (the last counts).
        let mut twice = Watchdog::default();
        twice.check(&[]);
        let (came, _) = twice.check(&[sys("ab", "first"), sys("AB", "second")]);
        assert_eq!(came.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["second"]);
        let (came, went) = watch.check(&[sys("", "no id")]);
        assert_eq!((came.len(), went.len()), (0, 1), "a device without an id is not tracked: it does not come");
    }

    #[test]
    fn the_watchdog_notes_when_the_files_changed() {
        let mut watch = Watchdog::default();
        let one = SourceStamp(vec![("a".into(), 1, 2)]);
        let two = SourceStamp(vec![("a".into(), 2, 2)]);
        assert!(!watch.sources_changed(one.clone()), "the first call only sets the basis");
        assert!(!watch.sources_changed(one));
        assert!(watch.sources_changed(two.clone()));
        assert!(!watch.sources_changed(two));
    }
}
