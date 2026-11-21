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

use super::*;

/// The run of the process is one thing for all threads: the checks that begin a run take turns.
fn serial() -> std::sync::MutexGuard<'static, ()> {
    static TURN: Mutex<()> = Mutex::new(());
    TURN.lock().unwrap_or_else(|e| e.into_inner())
}

fn temp_file(name: &str) -> String {
    let dir = crate::testdir::fresh(name);
    dir.join(FILE).to_string_lossy().into_owned()
}

#[test]
fn the_weights_of_the_steps_add_up_to_the_whole_build() {
    let sum: f64 = Step::ALL.iter().map(|step| step.weight()).sum();
    assert!((sum - 1.0).abs() < 1e-9, "the shares are {sum}");
    for (at, step) in Step::ALL.iter().enumerate() {
        assert_eq!(step.number(), at + 1);
        assert_eq!(Step::from_number(at + 1), Some(*step));
    }
    assert_eq!(Step::from_number(0), None);
    assert_eq!(Step::from_number(Step::count() + 1), None);
}

/// A whole build as the archive library reports it, records overtaking each other: the fraction never falls, never
/// reaches one while the run lasts, and the steps that count know their share.
#[test]
fn the_fraction_rises_through_all_steps_and_never_falls() {
    let _turn = serial();
    let run = begin(None);
    let report = reporter();
    let mut last = run.snapshot().fraction;
    let mut check = |run: &Run| {
        let now = run.snapshot().fraction;
        assert!(now >= last, "the fraction fell from {last} to {now}");
        assert!(now < 1.0, "a running build is never whole");
        last = now;
    };
    for step_now in Step::ALL {
        step(step_now);
        check(&run);
        for _ in 0..step_now.readings() {
            report.reading_begins();
            for phase in 0..PHASES {
                // The calls arrive in a mixed order, as they do from the worker threads.
                for done in [0, 2048, 1024, 4096, 3072, 5000] {
                    report.records(phase, done, 5000);
                    check(&run);
                }
            }
            report.reading_ends();
            check(&run);
        }
    }
    let end = run.snapshot();
    assert_eq!(end.step, Step::Save);
    assert!(end.fraction > 0.9, "the last step stands at {}", end.fraction);
    assert_eq!(end.percent(), 99.min((end.fraction * 100.0) as u32));
}

#[test]
fn only_the_steps_that_count_records_know_their_part() {
    let _turn = serial();
    let run = begin(None);
    let report = reporter();
    assert_eq!(run.snapshot().step_fraction, None, "opening the archive does not count");
    step(Step::Reputation);
    assert_eq!(run.snapshot().step_fraction, Some(0.0));
    report.reading_begins();
    report.records(1, 5000, 5000);
    let first = run.snapshot().step_fraction.unwrap();
    assert!((first - 0.5).abs() < 1e-9, "one reading of two is half the step: {first}");
    report.reading_ends();
    assert!((run.snapshot().step_fraction.unwrap() - 0.5).abs() < 1e-9, "the reading ended, the half stays");
    report.reading_begins();
    report.records(1, 5000, 5000);
    report.reading_ends();
    assert!((run.snapshot().step_fraction.unwrap() - 1.0).abs() < 1e-9);
    step(Step::Scmdb);
    assert_eq!(run.snapshot().step_fraction, None, "scmdb's fetch has no total");
}

/// A loop over a known number tells its part, and the three loops of the blueprints fill their step in turn.
#[test]
fn counted_loops_fill_their_step_one_after_the_other() {
    let _turn = serial();
    let run = begin(None);
    step(Step::Blueprints);
    let mut parts = Vec::new();
    for entries in [400_usize, 1000, 50] {
        let count = counter(entries);
        for at in 0..entries {
            count.at(at);
            if at % (entries / 4) == 0 {
                parts.push(run.snapshot().step_fraction.unwrap());
            }
        }
        drop(count);
        parts.push(run.snapshot().step_fraction.unwrap());
    }
    assert!(parts.windows(2).all(|pair| pair[0] <= pair[1]), "the part of the step never falls: {parts:?}");
    let end = run.snapshot().step_fraction.unwrap();
    assert!((end - 1.0).abs() < 1e-9, "three loops of three fill the step: {end}");
    // About a loop of nothing: nothing to count, nothing reported.
    drop(counter(0));
}

#[test]
fn a_step_is_not_gone_back_to_and_a_reading_without_a_total_counts_nothing() {
    let _turn = serial();
    let run = begin(None);
    step(Step::Places);
    step(Step::Texts);
    assert_eq!(run.snapshot().step, Step::Places);
    let before = run.snapshot().fraction;
    reporter().records(1, 10, 0);
    assert_eq!(run.snapshot().fraction, before);
}

/// Whatever the counts do, what the window shows does not go back.
#[test]
fn the_shown_fraction_holds_even_when_the_counts_fall_back() {
    let _turn = serial();
    let run = begin(None);
    step(Step::Contracts);
    reporter().reading_begins();
    reporter().records(1, 5000, 5000);
    let high = run.snapshot().fraction;
    lock(&run.shared).reading = Some(0.1);
    let after = run.snapshot().fraction;
    assert!(after >= high, "fell from {high} to {after}");
}

#[test]
fn a_reporter_without_a_run_reports_nowhere() {
    let report = Reporter::default();
    report.reading_begins();
    report.records(0, 1, 2);
    report.reading_ends();
    step(Step::Save);
}

/// The whole path of the window: the run of this process is seen, ends with its thread's run, the file of it comes
/// and goes, and a failed run is told as one that did not work.
#[test]
fn the_window_sees_the_run_until_it_ends_and_the_file_goes_with_it() {
    let _turn = serial();
    let file = temp_file("katalogbau-lauf");
    let mut run = begin(Some(file.clone()));
    assert!(std::path::Path::new(&file).is_file(), "the file stands from the first moment");
    step(Step::Contracts);
    assert_eq!(snapshot().map(|seen| seen.step), Some(Step::Contracts));
    run.succeeded();
    drop(run);
    assert_eq!(snapshot(), None, "nothing runs any more");
    assert!(!std::path::Path::new(&file).exists(), "the file went with the run");
    let ended = take_ended().expect("the end is told");
    assert!(ended.worked && ended.steps == [Step::Archive, Step::Contracts]);

    // A run that is only dropped (the build gave nothing, or died) did not work.
    let run = begin(Some(file.clone()));
    step(Step::Texts);
    drop(run);
    let failed = take_ended().expect("the end is told");
    assert!(!failed.worked, "an unfinished run did not work");
    assert!(!std::path::Path::new(&file).exists());
}

/// The file another process reads: a run of another process is seen, one that is not kept up any more, one of this
/// process and a broken file are not.
#[test]
fn the_file_of_another_process_is_seen_while_it_is_kept_up() {
    let file = temp_file("katalogbau-datei");
    let other = i64::from(std::process::id()) + 1;
    let text = |pid: i64, stand: u64| {
        format!(
            "{{\"pid\": {pid}, \"begin\": 1000, \"stand\": {stand}, \"schritt\": 4, \"lesungen\": 0, \
             \"anteil\": 0.5, \"gesamt\": 0.28}}"
        )
    };
    jsonio::write_text_atomic(&file, &text(other, 2000), jsonio::Newline::Keep).unwrap();
    let seen = snapshot_of_file(&file, 2005).expect("a run of another process is seen");
    assert_eq!(seen.step, Step::Contracts);
    assert_eq!(seen.elapsed, 1005);
    assert!(seen.fraction >= 0.28, "not below what the other process said: {}", seen.fraction);
    assert!(seen.step_fraction.is_some_and(|part| (part - 0.5).abs() < 1e-9));
    assert_eq!(snapshot_of_file(&file, 2000 + STALE_SECONDS + 1), None, "a file nobody keeps up is none");
    assert!(snapshot_of_file(&file, 2000 + STALE_SECONDS).is_some());

    jsonio::write_text_atomic(&file, &text(i64::from(std::process::id()), 2000), jsonio::Newline::Keep).unwrap();
    assert_eq!(snapshot_of_file(&file, 2005), None, "the own run is asked in the process, not through the file");

    jsonio::write_text_atomic(&file, "{\"pid\": ", jsonio::Newline::Keep).unwrap();
    assert_eq!(snapshot_of_file(&file, 2005), None, "a broken file is none");
    jsonio::write_text_atomic(&file, &text(other, 2000).replace("\"schritt\": 4", "\"schritt\": 40"), jsonio::Newline::Keep)
        .unwrap();
    assert_eq!(snapshot_of_file(&file, 2005), None, "a step that does not exist is none");
    assert_eq!(snapshot_of_file("/no/such/dir/katalogbau.json", 2005), None);
}

/// A build that waits for the game to close is no build at work: the window is told, also through the file, and the
/// wait does not count as time spent.
#[test]
fn a_wait_for_the_archive_is_told_and_does_not_count() {
    let _turn = serial();
    let file = temp_file("katalogbau-warten");
    let run = begin(Some(file.clone()));
    assert!(!run.snapshot().waiting);
    waiting(true);
    assert!(run.snapshot().waiting);
    let text = jsonio::read_json(&file).unwrap();
    assert_eq!(text.get("wartet"), Some(&Value::Bool(true)), "the other process is told at once");
    waiting(false);
    assert!(!run.snapshot().waiting);
    assert_eq!(jsonio::read_json(&file).unwrap().get("wartet"), Some(&Value::Bool(false)));
    // Without a build on the thread the wait is nobody's business.
    drop(run);
    waiting(true);
    waiting(false);

    let other = i64::from(std::process::id()) + 1;
    let mine = format!(
        "{{\"pid\": {other}, \"begin\": 1000, \"stand\": 2000, \"schritt\": 1, \"lesungen\": 0, \"anteil\": 0.0, \
         \"gesamt\": 0.0, \"wartet\": true}}"
    );
    jsonio::write_text_atomic(&file, &mine, jsonio::Newline::Keep).unwrap();
    assert!(snapshot_of_file(&file, 2001).is_some_and(|seen| seen.waiting));
}

/// What a run writes is what another process reads back.
#[test]
fn what_a_run_writes_another_process_reads_back_as_the_same_place() {
    let _turn = serial();
    let file = temp_file("katalogbau-rundreise");
    let run = begin(Some(file.clone()));
    step(Step::Places);
    let report = reporter();
    report.reading_begins();
    report.records(1, 2500, 5000);
    // The file is rewritten with a move only once a second, so the step is written by the change of the step.
    step(Step::Blueprints);
    let own = run.snapshot();
    let text = jsonio::read_json(&file).unwrap();
    let pid = text.get("pid").cloned().unwrap();
    assert_eq!(pid, Value::int(i64::from(std::process::id())));
    // Read as the other process would: only the pid differs.
    let relabeled = jsonio::dumps(
        &Value::Object(
            text.as_object()
                .unwrap()
                .iter()
                .map(|(key, value)| {
                    if key == "pid" {
                        (key.clone(), Value::int(i64::from(std::process::id()) + 1))
                    } else {
                        (key.clone(), value.clone())
                    }
                })
                .collect(),
        ),
        None,
        false,
    );
    jsonio::write_text_atomic(&file, &relabeled, jsonio::Newline::Keep).unwrap();
    let read = snapshot_of_file(&file, unix_now()).expect("read back");
    assert_eq!(read.step, own.step);
    assert!(read.fraction >= own.fraction - 1e-9, "{} < {}", read.fraction, own.fraction);
    drop(run);
}
