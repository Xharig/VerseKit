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
use crate::net::Fake;

const PAGE: &str = r##"{"summaryStatus": "maintenance", "buildDate": "2026-08-26", "buildTime": "18:30",
    "colorOk": "#00ff00", "systems": [
        {"name": "Platform", "status": "operational"},
        {"name": "Persistent Universe", "status": "major_outage", "unresolvedIssues": [
            {"title": "Live Deployment", "severity": "maintenance ", "affected": ["Persistent Universe"],
             "createdAt": "2026-08-26 14:15:00 +0000 UTC", "filename": "2026-08-26_live-deployment.md",
             "permalink": "https://status.robertsspaceindustries.com/issues/2026-08-26-live-deployment/"}]},
        {"name": "Arena Commander", "status": "something new"}]}"##;

fn index() -> String {
    format!("{BASE}/index.json")
}

fn paths_in(name: &str, off: bool) -> (Paths, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("versekit-serverstatus-{}-{name}", crate::testdir::run_tag()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let home = dir.to_string_lossy().into_owned();
    let mut vars = vec![(crate::legacy::ENV_HOME, home.as_str())];
    if off {
        vars.push((crate::legacy::ENV_NO_NET, "1"));
    }
    let host = crate::paths::Host::new(crate::pypath::Flavor::native(), &vars, &home);
    (Paths::new(host), dir)
}

#[test]
fn the_answer_of_the_page_is_kept_in_the_form_of_4_12() {
    let state = state_of(PAGE.as_bytes(), 1000.0).expect("read");
    let Value::Object(state) = state else { panic!() };
    assert_eq!(field(&state, "gesamt").and_then(Value::as_str), Some("maintenance"), "the word of CIG");
    // 2026-08-26 18:30 UTC, the build time without seconds.
    assert_eq!(field(&state, "stand").and_then(number), Some(1_787_769_000.0));
    let Some(Value::Array(systems)) = field(&state, "systeme") else { panic!() };
    let lights: Vec<(&str, &str)> = systems
        .iter()
        .filter_map(Value::as_object)
        .map(|s| {
            let get = |k: &str| field(s, k).and_then(Value::as_str).unwrap_or("");
            (get("ampel"), get("farbe"))
        })
        .collect();
    assert_eq!(lights, [("ok", "#00ff00"), ("aus", "#e60000"), ("gestoert", "#cc4400")], "unknown is a disruption");
    assert!(state_of(b"not json", 1.0).is_none());
}

#[test]
fn a_system_brings_its_open_incidents_and_the_whole_takes_the_worst_colour() {
    let (paths, dir) = paths_in("meldung", false);
    let net = Fake::default().with(&index(), PAGE);
    let state = state(&paths, &net, 1000.0, false, None);
    let pu = &state.systems[1];
    assert_eq!(pu.incidents.len(), 1);
    let incident = &pu.incidents[0];
    assert_eq!(
        (incident.title.as_str(), incident.severity.as_str(), incident.affected.as_slice()),
        ("Live Deployment", "maintenance", ["Persistent Universe".to_string()].as_slice())
    );
    assert_eq!(incident.begun, Some(1_787_753_700), "2026-08-26 14:15 read as UTC");
    assert_eq!(state.colour(), Some("#e60000"), "the outage is the worst");
    assert!(!state.all_well());
    assert_eq!(stored_state(&paths), state, "kept as it was read, incidents included");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_page_is_asked_at_most_every_five_minutes_and_never_without_the_net() {
    let (paths, dir) = paths_in("takt", false);
    let net = Fake::default().with(&index(), PAGE);
    assert_eq!(stored(&paths), None, "nothing fetched yet");
    let first = refresh(&paths, &net, 1000.0).expect("fetched");
    assert_eq!((first.word.as_str(), first.fetched), ("maintenance", 1000.0));
    assert_eq!(refresh(&paths, &net, 1200.0), Some(first.clone()), "fresh: not asked again");
    assert_eq!(net.asked().len(), 1);
    assert_eq!(refresh(&paths, &net, 1400.0).map(|o| o.fetched), Some(1400.0), "five minutes on: asked again");
    assert_eq!(net.asked().len(), 2);
    // The page cannot be reached: the state kept stands.
    let down = Fake::default();
    assert_eq!(refresh(&paths, &down, 2000.0).map(|o| o.fetched), Some(1400.0));
    let _ = std::fs::remove_dir_all(&dir);

    let (quiet, dir) = paths_in("aus", true);
    assert_eq!(refresh(&quiet, &net, 1000.0), None, "SC_BP_NO_NET: nothing asked");
    assert_eq!(net.asked().len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_minute_asks_with_the_tag_and_a_forced_look_without() {
    let (paths, dir) = paths_in("etag", false);
    let net = Fake::default().with(&index(), PAGE).with_etag(&index(), "W/\"7\"");
    let first = state(&paths, &net, 1000.0, false, None);
    let (again, changed) = ask(&paths, &net, 1060.0);
    assert!(!changed, "the server says unchanged");
    assert_eq!(again.fetched, 1060.0, "the clock goes on");
    assert_eq!(again.systems, first.systems);
    assert_eq!(net.asked().last().map(String::as_str), Some(format!("{} (unchanged)", index()).as_str()));
    let forced = state(&paths, &net, 1100.0, true, None);
    assert_eq!(net.asked().last(), Some(&index()), "forced: asked without the tag, the body comes");
    assert_eq!(forced.fetched, 1100.0);

    // Something changed on the page: the minute sees it.
    let changed_page = PAGE.replace("\"major_outage\"", "\"operational\"");
    let net = Fake::default().with(&index(), changed_page);
    let (state, changed) = ask(&paths, &net, 1200.0);
    assert!(changed && state.systems[1].status == "operational");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn without_the_net_the_state_kept_stands_and_says_so() {
    let (paths, dir) = paths_in("ohne", false);
    let empty = state(&paths, &Fake::default(), 1000.0, true, None);
    assert!(empty.is_empty() && empty.no_net, "never fetched and no net: nothing, with the note");
    state(&paths, &Fake::default().with(&index(), PAGE), 1000.0, false, None);
    let kept = state(&paths, &Fake::default(), 2000.0, true, None);
    assert!(kept.no_net && kept.systems.len() == 3 && kept.fetched == 1000.0, "the last state, marked");
    let broken = state(&paths, &Fake::default().with(&index(), "<html>"), 3000.0, true, None);
    assert!(broken.no_net && broken.fetched == 1000.0, "an answer that cannot be read counts the same");
    let _ = std::fs::remove_dir_all(&dir);

    let (quiet, dir) = paths_in("ohne-aus", true);
    let net = Fake::default().with(&index(), PAGE);
    assert!(state(&quiet, &net, 1000.0, true, None).is_empty());
    assert!(net.asked().is_empty(), "SC_BP_NO_NET: nothing asked");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_state_kept_by_4_12_is_read() {
    let (paths, dir) = paths_in("alt", true);
    let target = paths.app_file(CACHE);
    std::fs::create_dir_all(std::path::Path::new(&target).parent().unwrap()).unwrap();
    std::fs::write(
        &target,
        r##"{"etag": "W/\"1\"", "lage": {"gesamt": "operational", "geholt": 1756240000.0, "systeme": [
            {"name": "Platform", "status": "operational", "ampel": "ok", "farbe": "#008000", "meldungen": [
                {"titel": "T", "schwere": "maintenance", "betroffen": ["Platform"], "begonnen": 1756230000,
                 "erledigt": null, "datei": "x.md", "adresse": ""}]}]}}"##,
    )
    .unwrap();
    assert_eq!(stored(&paths), Some(Overall { word: "operational".into(), fetched: 1_756_240_000.0 }));
    let state = stored_state(&paths);
    assert_eq!(state.systems[0].incidents[0].begun, Some(1_756_230_000));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_text_keeps_lines_highlighted_throughout_and_nothing_else() {
    let lines = text_from_html(
        "<p>1415 UTC - Initial Notice.</p><p><strong>Players are advised to stow their vehicles</strong></p>\
         <p>One <b>bold</b> word &amp; more<br/>second line</p><!-- raw HTML omitted --><p> </p>",
    );
    assert_eq!(
        lines,
        [
            ("1415 UTC - Initial Notice.".to_string(), false),
            ("Players are advised to stow their vehicles".to_string(), true),
            ("One bold word & more".to_string(), false),
            ("second line".to_string(), false),
        ]
    );
}

#[test]
fn the_notices_of_two_months_resolved_ones_fetched_once() {
    let (paths, dir) = paths_in("meldungen", false);
    let day = 86_400.0;
    let now = 1_790_000_000.0;
    let at = |seconds: f64| {
        let days = (seconds / day) as i64;
        // 1970-01-01 plus days, as the page writes it.
        let date = crate::export::utc_text(days * 86_400, false);
        format!("{} 12:00:00 +0000 UTC", &date[..10])
    };
    let history = format!(
        r#"{{"pages": [
            {{"title": "Open", "createdAt": "{}", "filename": "open.md"}},
            {{"title": "Done", "createdAt": "{}", "resolvedAt": "{}", "filename": "done.md"}},
            {{"title": "Old", "createdAt": "{}", "resolvedAt": "{}", "filename": "old.md"}}]}}"#,
        at(now - day),
        at(now - 10.0 * day),
        at(now - 9.0 * day),
        at(now - 90.0 * day),
        at(now - 89.0 * day)
    );
    let full = |title: &str, resolved: Option<String>| {
        let resolved = resolved.map_or(String::new(), |r| format!(r#", "resolvedAt": "{r}""#));
        format!(r#"{{"title": "{title}", "body": "<p>Text of {title}</p>"{resolved}}}"#)
    };
    let net = Fake::default()
        .with(&format!("{BASE}/issues/index.json"), history)
        .with(&format!("{BASE}/issues/open/index.json"), full("Open", None))
        .with(&format!("{BASE}/issues/done/index.json"), full("Done", Some(at(now - 9.0 * day))));
    let first = messages(&paths, &net, now, 2, 12);
    let titles: Vec<&str> = first.iter().map(|i| i.title.as_str()).collect();
    assert_eq!(titles, ["Open", "Done"], "older than two months: not shown");
    assert_eq!(first[1].lines, [("Text of Done".to_string(), false)]);
    let asked_first = net.asked().len();
    let again = messages(&paths, &net, now, 2, 12);
    assert_eq!(again, first);
    let fetched_again: Vec<String> = net.asked()[asked_first..].to_vec();
    assert!(fetched_again.iter().any(|u| u.contains("/open/")), "an open one is fetched again");
    assert!(!fetched_again.iter().any(|u| u.contains("/done/")), "a resolved one is kept");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_age_is_written_as_on_the_status_page() {
    crate::language::set_language(crate::language::Choice::De);
    let now = 10_000_000.0;
    assert_eq!(relative_time(None, now), "—");
    assert_eq!(relative_time(Some(now - 30.0), now), crate::language::t("s_st_gerade"));
    assert_eq!(relative_time(Some(now - 60.0 * 7.0), now), "vor 7 Min.");
    assert_eq!(relative_time(Some(now - 60.0), now), crate::language::t("s_st_gerade"), "under 90 seconds");
    assert_eq!(relative_time(Some(now - 95.0), now), crate::language::t("s_st_vor_min_1"));
    assert_eq!(relative_time(Some(now - 3.0 * 86_400.0), now), crate::language::t("s_st_vor_tag").replace("{0}", "3"));
    assert_eq!(
        relative_time(Some(now - 100.0 * 86_400.0), now),
        crate::language::t("s_st_vor_monat").replace("{0}", "3")
    );
}
