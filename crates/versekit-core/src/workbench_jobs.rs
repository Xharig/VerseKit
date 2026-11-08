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

//! The jobs of the fabricator, entered by hand: VerseKit only keeps the book and starts nothing in the game.
//!
//! The game writes nothing about a job of the fabricator into the log (no line in any log of a long play, checked
//! 07.11.2026), so the player enters what is made and the time the game shows. The fabricator works on
//! [`RUNNING`] jobs at once, [`WAITING`] more wait and move up as soon as one is done; the order of entering is the
//! order of the queue. From that the start and the end of each job follow ([`plan`]), and a done job is told once
//! ([`announce_due`]), also across restarts. Kept in `werkbank-auftraege.json`:
//! `{"version": 1, "auftraege": [{"id": 1, "name": "..", "sekunden": 1800, "eingetragen": 1.5e9}]}`.

use crate::jsonio::{self, Map, Newline, Value, map_set};
use crate::language::{t, tf};
use crate::paths::Paths;

pub const FILE: &str = "werkbank-auftraege.json";
/// The jobs the fabricator works on at once, the ones that may wait, and the done ones it lists (the limits of the
/// game files, `crafters`).
pub const RUNNING: usize = 2;
pub const WAITING: usize = 4;
pub const HISTORY: usize = 10;
/// The longest time a job may have, in seconds: a week.
pub const LONGEST: i64 = 7 * 24 * 3600;

/// One job as kept: what is made, how long it takes, when it was entered and whether its end was told.
#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    pub id: i64,
    pub name: String,
    pub seconds: i64,
    pub entered: f64,
    pub notified: bool,
}

/// Where a job stands at a moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum State {
    /// On the fabricator, done at `end`.
    Running { end: f64 },
    /// In the queue; it starts at `start` and is done at `end` when everything before runs as planned.
    Waiting { start: f64, end: f64 },
    /// Done since `end`.
    Done { end: f64 },
}

/// A job with its place in the queue.
#[derive(Clone, Debug, PartialEq)]
pub struct Planned {
    pub job: Job,
    pub start: f64,
    pub end: f64,
}

impl Planned {
    pub fn state(&self, now: f64) -> State {
        if self.end <= now {
            State::Done { end: self.end }
        } else if self.start <= now {
            State::Running { end: self.end }
        } else {
            State::Waiting { start: self.start, end: self.end }
        }
    }
}

/// A number as the file may hold it.
fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Int(text) => text.parse().ok(),
        Value::Float(x) => Some(*x),
        _ => None,
    }
}

fn job_of(map: &Map) -> Option<Job> {
    let get = |key: &str| map.iter().find(|(k, _)| k == key).map(|(_, v)| v);
    let seconds = get("sekunden").and_then(number)?;
    let entered = get("eingetragen").and_then(number)?;
    if !(seconds.is_finite() && entered.is_finite()) || seconds < 0.0 {
        return None;
    }
    Some(Job {
        id: get("id").and_then(number).map_or(0, |n| n as i64),
        name: get("name").and_then(Value::as_str).unwrap_or("").to_string(),
        seconds: (seconds as i64).min(LONGEST),
        entered,
        notified: matches!(get("gemeldet"), Some(Value::Bool(true))),
    })
}

fn value_of(job: &Job) -> Value {
    let mut map: Map = vec![
        ("id".into(), Value::int(job.id)),
        ("name".into(), Value::str(job.name.clone())),
        ("sekunden".into(), Value::int(job.seconds)),
        ("eingetragen".into(), Value::Float(job.entered)),
    ];
    if job.notified {
        map_set(&mut map, "gemeldet", Value::Bool(true));
    }
    Value::Object(map)
}

/// All kept jobs in the order they were entered; an entry that is no job is left out. Where an id is missing or
/// stands twice (a file changed by hand), every job gets a new one, so that taking one out never takes another.
pub fn load(paths: &Paths) -> Vec<Job> {
    let Ok(data) = jsonio::read_json(&paths.app_file(FILE)) else { return Vec::new() };
    let Some(Value::Array(list)) = data.get("auftraege") else { return Vec::new() };
    let mut jobs: Vec<Job> = list.iter().filter_map(Value::as_object).filter_map(job_of).collect();
    let mut seen = std::collections::HashSet::new();
    if jobs.iter().any(|job| job.id <= 0 || !seen.insert(job.id)) {
        for (n, job) in jobs.iter_mut().enumerate() {
            job.id = n as i64 + 1;
        }
    }
    jobs
}

fn save(paths: &Paths, jobs: &[Job]) -> bool {
    let data = Value::Object(vec![
        ("version".into(), Value::int(1)),
        ("auftraege".into(), Value::Array(jobs.iter().map(value_of).collect())),
    ]);
    match jsonio::write_text_atomic(&paths.app_file(FILE), &jsonio::dumps(&data, Some(1), false), Newline::Native) {
        Ok(()) => true,
        Err(e) => {
            crate::errors::record("workbench_jobs.save", &e);
            false
        }
    }
}

/// The queue: each job in the order of entering takes the first place of the fabricator that is free, not before
/// it was entered.
pub fn plan(jobs: &[Job]) -> Vec<Planned> {
    let mut free = [f64::NEG_INFINITY; RUNNING];
    jobs.iter()
        .map(|job| {
            let (slot, at) = free.iter().copied().enumerate().min_by(|a, b| a.1.total_cmp(&b.1)).unwrap_or((0, 0.0));
            let start = at.max(job.entered);
            let end = start + job.seconds as f64;
            free[slot] = end;
            Planned { job: job.clone(), start, end }
        })
        .collect()
}

/// How many jobs are not done at `now`: on the fabricator or waiting.
pub fn open_count(jobs: &[Job], now: f64) -> usize {
    plan(jobs).iter().filter(|p| p.end > now).count()
}

/// Why a job could not be entered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// The fabricator and its queue are full ([`RUNNING`] + [`WAITING`]).
    Full,
    /// No time, or one longer than [`LONGEST`].
    Time,
    /// The file could not be written.
    NotSaved,
}

/// The jobs without the done ones beyond the [`HISTORY`] newest.
fn trimmed(jobs: Vec<Job>, now: f64) -> Vec<Job> {
    let planned = plan(&jobs);
    let mut done: Vec<(f64, i64)> = planned.iter().filter(|p| p.end <= now).map(|p| (p.end, p.job.id)).collect();
    done.sort_by(|a, b| b.0.total_cmp(&a.0));
    let dropped: Vec<i64> = done.iter().skip(HISTORY).map(|(_, id)| *id).collect();
    jobs.into_iter().filter(|job| !dropped.contains(&job.id)).collect()
}

/// Enters a job at `now`: it goes to the end of the queue. Done jobs beyond the [`HISTORY`] newest go; the jobs are
/// fixed to `now` first, so that this moves no one already on the fabricator or done.
pub fn add(paths: &Paths, name: &str, seconds: i64, now: f64) -> Result<Vec<Job>, Refused> {
    if seconds <= 0 || seconds > LONGEST {
        return Err(Refused::Time);
    }
    let _saves = jsonio::lock_saves();
    let mut jobs = fixed(load(paths), now);
    if open_count(&jobs, now) >= RUNNING + WAITING {
        return Err(Refused::Full);
    }
    let id = jobs.iter().map(|j| j.id).max().unwrap_or(0) + 1;
    let name = name.trim();
    jobs.push(Job { id, name: name.to_string(), seconds, entered: now, notified: false });
    let jobs = trimmed(jobs, now);
    if save(paths, &jobs) { Ok(jobs) } else { Err(Refused::NotSaved) }
}

/// Takes the job `id` out; the ones after it move up.
///
/// ⚠ A job taken out of the queue changes the times of the ones behind it: a job on the fabricator keeps its
/// start, so each job still running is fixed to the moment it started.
pub fn remove(paths: &Paths, id: i64, now: f64) -> Vec<Job> {
    let _saves = jsonio::lock_saves();
    let jobs = fixed(load(paths), now);
    let jobs: Vec<Job> = jobs.into_iter().filter(|job| job.id != id).collect();
    save(paths, &jobs);
    jobs
}

/// The jobs fixed to `now`: one on the fabricator or done is entered anew at its start, so taking a job out moves
/// no one that ran already; one waiting starts no sooner than `now`, so it moves up from now on and not back in time.
fn fixed(jobs: Vec<Job>, now: f64) -> Vec<Job> {
    plan(&jobs)
        .into_iter()
        .map(|planned| {
            let state = planned.state(now);
            let mut job = planned.job;
            match state {
                State::Running { .. } | State::Done { .. } => job.entered = planned.start,
                State::Waiting { .. } => job.entered = job.entered.max(now),
            }
            job
        })
        .collect()
}

/// Takes every done job out.
pub fn clear_done(paths: &Paths, now: f64) -> Vec<Job> {
    let _saves = jsonio::lock_saves();
    let planned = plan(&fixed(load(paths), now));
    let jobs: Vec<Job> = planned.into_iter().filter(|p| p.end > now).map(|p| p.job).collect();
    save(paths, &jobs);
    jobs
}

/// `(due, all)`: due are the jobs done at `now` and not told yet; in `all` they are marked told. A job done longer
/// ago than a day (the overlay did not run) is marked told without a word. No disk.
pub fn due(jobs: &[Job], now: f64) -> (Vec<Job>, Vec<Job>) {
    let planned = plan(jobs);
    let mut found = Vec::new();
    let all = planned
        .into_iter()
        .map(|p| {
            let mut job = p.job;
            if p.end <= now && !job.notified {
                job.notified = true;
                if now - p.end <= crate::refinery_jobs::KEEP_DONE_SEC {
                    found.push(job.clone());
                }
            }
            job
        })
        .collect();
    (found, all)
}

/// The jobs done since the last call, each exactly once, also across a restart.
pub fn announce_due(paths: &Paths, now: f64) -> Vec<Job> {
    let _saves = jsonio::lock_saves();
    let (found, jobs) = due(&load(paths), now);
    if !found.is_empty() {
        save(paths, &jobs);
    }
    found
}

/// The name of a job as it stands in a line; a job without a name is called the fabricator's job.
pub fn label(job: &Job) -> String {
    if job.name.trim().is_empty() { t("s_wb_ohne_name") } else { job.name.trim().to_string() }
}

/// The clock time of a moment, `14:30`.
fn clock(at: f64) -> String {
    crate::formats::local_time(at as i64).map_or_else(String::new, |time| crate::formats::strftime_shown("%H:%M", &time))
}

/// What a job does at `now`, in words: the time left and the end, waiting with its end, or done.
pub fn state_text(planned: &Planned, now: f64) -> String {
    match planned.state(now) {
        State::Running { end } => {
            let left = crate::overview::countdown_text(end - now);
            tf("s_uv_raff_noch", &[("0", left.into()), ("1", clock(end).into())])
        }
        State::Waiting { end, .. } => tf("s_wb_wartet", &[("0", clock(end).into())]),
        State::Done { .. } => t("s_uv_raff_fertig"),
    }
}

/// The jobs as lines of the overlay: on the fabricator and waiting in the order of the queue, then the done ones of
/// the last day; `true` marks a done one.
pub fn lines(jobs: &[Job], now: f64) -> Vec<(String, bool)> {
    let planned = plan(jobs);
    let open = planned.iter().filter(|p| p.end > now);
    let done = planned.iter().filter(|p| p.end <= now && now - p.end <= crate::refinery_jobs::KEEP_DONE_SEC);
    open.map(|p| (format!("{} · {}", label(&p.job), state_text(p, now)), false))
        .chain(done.map(|p| (format!("{} · {}", label(&p.job), state_text(p, now)), true)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Host;
    use crate::pypath::Flavor;

    fn paths(name: &str) -> Paths {
        let dir = crate::testdir::fresh(name);
        let home = dir.to_string_lossy().into_owned();
        Paths::new(Host::new(Flavor::Posix, &[("HOME", &home)], &home))
    }

    fn job(id: i64, seconds: i64, entered: f64) -> Job {
        Job { id, name: format!("Teil {id}"), seconds, entered, notified: false }
    }

    /// Two run at once; the third waits for the first place that is free and starts there; one entered later
    /// starts no sooner than it was entered.
    #[test]
    fn two_run_at_once_and_the_others_move_up_when_one_is_done() {
        let jobs = [job(1, 600, 0.0), job(2, 300, 0.0), job(3, 120, 0.0), job(4, 60, 1000.0)];
        let planned = plan(&jobs);
        let times: Vec<(f64, f64)> = planned.iter().map(|p| (p.start, p.end)).collect();
        assert_eq!(times, [(0.0, 600.0), (0.0, 300.0), (300.0, 420.0), (1000.0, 1060.0)]);
        assert_eq!(planned[0].state(100.0), State::Running { end: 600.0 });
        assert_eq!(planned[2].state(100.0), State::Waiting { start: 300.0, end: 420.0 });
        assert_eq!(planned[2].state(350.0), State::Running { end: 420.0 }, "moved up once the second was done");
        assert_eq!(planned[1].state(300.0), State::Done { end: 300.0 });
        assert_eq!(open_count(&jobs, 100.0), 4, "two running, one waiting, one entered for later");
        assert_eq!(open_count(&jobs, 500.0), 2, "the second and the third are done");
    }

    /// Entering: kept in its file and read back; at most two running and four waiting; no time and too long a time
    /// are refused.
    #[test]
    fn jobs_are_entered_kept_and_refused_beyond_the_places_of_the_fabricator() {
        let paths = paths("werkbank-eintragen");
        assert_eq!(add(&paths, "Teil", 0, 0.0), Err(Refused::Time));
        assert_eq!(add(&paths, "Teil", LONGEST + 1, 0.0), Err(Refused::Time));
        for n in 0..(RUNNING + WAITING) {
            assert!(add(&paths, &format!(" Helm {n} "), 3600, 10.0).is_ok(), "{n}");
        }
        assert_eq!(add(&paths, "Zu viel", 3600, 10.0), Err(Refused::Full));
        let kept = load(&paths);
        assert_eq!(kept.len(), RUNNING + WAITING);
        assert_eq!(kept[0].name, "Helm 0", "the name without spaces around it");
        assert_eq!(kept.iter().map(|j| j.id).collect::<Vec<_>>(), [1, 2, 3, 4, 5, 6]);
        // Once the first two are done, two more places are free.
        assert!(add(&paths, "Später", 600, 3610.0).is_ok());
        assert_eq!(open_count(&load(&paths), 3610.0), 5);
    }

    /// A done job is told once, also when the file is read anew; a job not done yet is not.
    #[test]
    fn a_done_job_is_told_exactly_once() {
        let paths = paths("werkbank-melden");
        add(&paths, "Gewehr", 60, 0.0).unwrap();
        add(&paths, "Helm", 600, 0.0).unwrap();
        assert!(announce_due(&paths, 30.0).is_empty(), "nothing done yet");
        let told = announce_due(&paths, 61.0);
        assert_eq!(told.iter().map(|j| j.name.as_str()).collect::<Vec<_>>(), ["Gewehr"]);
        assert!(announce_due(&paths, 62.0).is_empty(), "told once");
        assert!(load(&paths)[0].notified, "kept as told");
        assert_eq!(announce_due(&paths, 700.0).len(), 1, "the second when it is done");
    }

    /// Taking a waiting job out moves the ones behind it up; the one running keeps its end.
    #[test]
    fn taking_a_job_out_moves_the_queue_and_keeps_the_running_ones() {
        let paths = paths("werkbank-entfernen");
        for (name, seconds) in [("A", 600), ("B", 600), ("C", 300), ("D", 300)] {
            add(&paths, name, seconds, 0.0).unwrap();
        }
        let jobs = remove(&paths, 3, 100.0);
        let planned = plan(&jobs);
        let ends: Vec<(&str, f64)> = planned.iter().map(|p| (p.job.name.as_str(), p.end)).collect();
        assert_eq!(ends, [("A", 600.0), ("B", 600.0), ("D", 900.0)], "D took the place of C");
        // Taking out a running one: the other one keeps its end, the next waiting one moves on its place.
        let jobs = remove(&paths, 1, 100.0);
        let planned = plan(&jobs);
        let ends: Vec<(&str, f64)> = planned.iter().map(|p| (p.job.name.as_str(), p.end)).collect();
        assert_eq!(ends, [("B", 600.0), ("D", 400.0)]);
    }

    /// A full queue filled up again and again, far beyond the history: entering a job never moves the end of a job
    /// already on the fabricator or waiting, also when the oldest done ones fall out.
    #[test]
    fn entering_never_moves_the_jobs_before_it_also_when_the_history_is_cut() {
        let paths = paths("werkbank-kette");
        let mut entered = 0;
        for step in 0..80 {
            let now = f64::from(step) * 30.0;
            let ends = |jobs: &[Job]| -> Vec<(i64, f64)> {
                plan(jobs).iter().filter(|p| p.end > now).map(|p| (p.job.id, p.end)).collect()
            };
            let before = ends(&load(&paths));
            if let Ok(jobs) = add(&paths, "Teil", 100, now) {
                entered += 1;
                let after: Vec<(i64, f64)> =
                    ends(&jobs).into_iter().filter(|(id, _)| before.iter().any(|(b, _)| b == id)).collect();
                assert_eq!(after, before, "step {step}: the jobs before keep their ends");
            }
        }
        assert!(entered > HISTORY + RUNNING + WAITING, "the history was cut: {entered} entered");
        assert!(load(&paths).len() <= HISTORY + RUNNING + WAITING);
    }

    /// A job done long ago (the overlay did not run) is marked told without a word; one of the last day is told.
    #[test]
    fn a_job_done_long_ago_is_not_told_any_more() {
        let jobs = [job(1, 60, 0.0), job(2, 60, 200_000.0)];
        let (found, all) = due(&jobs, 200_100.0);
        assert_eq!(found.iter().map(|j| j.id).collect::<Vec<_>>(), [2], "only the one of the last day");
        assert!(all.iter().all(|j| j.notified), "both marked told");
    }

    /// A file changed by hand with an id twice or none: every job gets its own, and taking one out takes only it.
    #[test]
    fn ids_twice_or_missing_are_given_anew() {
        let paths = paths("werkbank-ids");
        let text = r#"{"version": 1, "auftraege": [
            {"id": 3, "name": "A", "sekunden": 600, "eingetragen": 0.0},
            {"id": 3, "name": "B", "sekunden": 600, "eingetragen": 0.0},
            {"name": "C", "sekunden": 600, "eingetragen": 0.0}]}"#;
        let file = paths.app_file(FILE);
        std::fs::create_dir_all(std::path::Path::new(&file).parent().expect("a folder")).unwrap();
        std::fs::write(&file, text).unwrap();
        let jobs = load(&paths);
        assert_eq!(jobs.iter().map(|j| j.id).collect::<Vec<_>>(), [1, 2, 3]);
        let left = remove(&paths, 1, 10.0);
        assert_eq!(left.iter().map(|j| j.name.as_str()).collect::<Vec<_>>(), ["B", "C"]);
    }

    /// The done ones go by the button, and beyond the ten newest when a job is entered.
    #[test]
    fn done_jobs_go_by_the_button_and_beyond_the_history() {
        let paths = paths("werkbank-verlauf");
        for n in 0..12 {
            // Each one entered after the one before is done.
            add(&paths, &format!("T{n}"), 10, f64::from(n) * 100.0).unwrap();
        }
        let kept = load(&paths);
        assert_eq!(kept.len(), HISTORY + 1, "ten done and the one running");
        assert_eq!(kept[0].name, "T1", "the oldest done one went");
        assert_eq!(clear_done(&paths, 1200.0).len(), 0);
        assert!(load(&paths).is_empty());
    }

    /// The lines of the overlay: the open ones in the order of the queue, then the done ones of the last day.
    #[test]
    fn the_lines_say_the_time_left_waiting_and_done() {
        crate::language::set_thread_language(crate::language::Choice::De);
        let jobs = [job(4, 60, -90_000.0), job(1, 600, 0.0), job(2, 300, 0.0), job(3, 120, 0.0)];
        let lines = lines(&jobs, 100.0);
        assert_eq!(lines.len(), 3, "the job done more than a day ago is left out: {lines:?}");
        assert!(lines[0].0.starts_with("Teil 1 · noch 8m 20s · fertig um "), "{lines:?}");
        assert!(lines[2].0.starts_with("Teil 3 · wartet"), "{lines:?}");
        assert!(lines.iter().all(|(_, done)| !done));
        let later = super::lines(&jobs, 350.0);
        assert_eq!(later.iter().filter(|(_, done)| *done).count(), 1);
        assert!(later.iter().any(|(line, done)| *done && line == "Teil 2 · fertig, abholen"), "{later:?}");
        let nameless = Job { name: "  ".into(), ..job(9, 60, 0.0) };
        assert_eq!(label(&nameless), t("s_wb_ohne_name"));
    }
}
