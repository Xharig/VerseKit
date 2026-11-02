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

//! The members of an organization, as the public page `/orgs/<SID>/members` of the RSI website loads them: page by
//! page from [`API`], a JSON body sent (`symbol`, `search`, `pagesize`, `page`), a JSON answer with `success`,
//! `data.totalrows` and the list as HTML in `data.html` (one `<li class="member-item …">` per member).
//!
//! The first page comes at once, every further one only when asked, at most [`PAGES_MAX`]. What was fetched is
//! kept in `org-mitglieder-cache.json` in `Intern`, as old as the handles may be, and the site is asked through
//! the same gate as the handle search, with the same pause.

use super::cache::{self, GATE, Gate, Paced};
use super::page::{between, class_names, clean, valid_handle};
use super::{Failure, text_of};
use crate::jsonio::{self, Map, Value};
use crate::net::{self, Net};
use crate::paths::Paths;

/// The address the page of the members loads its pages from.
pub const API: &str = "https://robertsspaceindustries.com/api/orgs/getOrgMembers";
/// How many members one page holds, as the site asks it.
pub const PAGE_SIZE: usize = 32;
/// The most pages loaded of one organization.
pub const PAGES_MAX: u32 = 20;
/// The file of the members kept.
pub const CACHE_FILE: &str = "org-mitglieder-cache.json";
/// The most organizations kept; the oldest go first.
pub const ORGS_KEPT_MAX: usize = 30;
/// The form of the file.
const FORMAT: i64 = 1;

/// One page of members and the number of members the site counts.
pub type Page = (Vec<Member>, Option<u64>);

/// One member as the list shows it; a hidden one has no handle.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Member {
    pub handle: String,
    pub rank: String,
    /// How many of the five rank stars are lit.
    pub stars: u8,
}

impl Member {
    /// Whether the member hides who it is.
    pub fn hidden(&self) -> bool {
        self.handle.is_empty()
    }
}

/// The members of an organization loaded so far.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Roster {
    pub sid: String,
    pub members: Vec<Member>,
    /// How many members the site counts.
    pub total: Option<u64>,
    /// How many pages are loaded.
    pub pages: u32,
    /// The site has no more.
    pub end: bool,
    /// When the first page was fetched (seconds).
    pub fetched: f64,
}

impl Roster {
    /// Whether a further page can be asked.
    pub fn more(&self) -> bool {
        !self.end && self.pages < PAGES_MAX
    }
}

/// The members as the search has them.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub roster: Roster,
    /// Fetched just now, not taken from the file.
    pub live: bool,
    /// Asked and not reached: the members kept stand, this is why.
    pub failed: Option<Failure>,
}

/// The body sent for page `page` (from 1) of `sid`; `sid` is a valid SID.
pub fn request_body(sid: &str, page: u32) -> String {
    format!(r#"{{"symbol":"{sid}","search":"","pagesize":{PAGE_SIZE},"page":{page}}}"#)
}

/// The lit stars of a member: the width of `<span class="stars" style="width: 40%;">` in fifths.
fn stars_in(part: &str) -> u8 {
    between(part, "class=\"stars\"", ">")
        .and_then(|tag| between(tag, "width:", "%"))
        .and_then(|width| width.trim().parse::<f64>().ok())
        .map_or(0, |width| (width / 20.0).round().clamp(0.0, 5.0) as u8)
}

/// One member in its `<li>`: hidden when it is not shown (`org-visibility-V`) or names no citizen.
fn member_in(part: &str) -> Member {
    let class = between(part, "class=\"", "\"").unwrap_or_default();
    let shown = class_names(class, "org-visibility-V");
    let handle = between(part, "href=\"/citizens/", "\"").map(clean).and_then(|h| valid_handle(&h));
    Member {
        handle: if shown { handle.unwrap_or_default() } else { String::new() },
        rank: between(part, "class=\"rank\">", "</span>").map(clean).unwrap_or_default(),
        stars: stars_in(part),
    }
}

/// The members in `html`, the list of one page, in its order.
pub fn parse_members(html: &str) -> Vec<Member> {
    let starts: Vec<usize> = html
        .match_indices("<li ")
        .map(|(at, _)| at)
        .filter(|&at| between(&html[at..], "class=\"", "\"").is_some_and(|c| class_names(c, "member-item")))
        .collect();
    starts
        .iter()
        .enumerate()
        .map(|(index, &start)| {
            let end = starts.get(index + 1).copied().unwrap_or(html.len());
            let part = &html[start..end];
            member_in(part.find("</li>").map_or(part, |close| &part[..close]))
        })
        .collect()
}

/// The members of one page and the number the site counts, from its answer; `None` when the site said no.
///
/// # Errors
///
/// [`Failure::Failed`] when the answer is no JSON of that form.
pub fn parse_answer(text: &str) -> Result<Option<Page>, Failure> {
    let wrong = || Failure::Failed("JSON".to_string());
    let answer = jsonio::loads(text).map_err(|_| wrong())?;
    if !matches!(answer.get("success"), Some(Value::Int(n)) if n == "1") {
        return Ok(None);
    }
    let data = answer.get("data").ok_or_else(wrong)?;
    let html = data.get("html").and_then(Value::as_str).ok_or_else(wrong)?;
    let total = match data.get("totalrows") {
        Some(Value::Int(n)) => n.parse().ok(),
        Some(Value::Str(n)) => n.trim().parse().ok(),
        _ => None,
    };
    Ok(Some((parse_members(html), total)))
}

/// Page `page` of `sid` from the site.
///
/// # Errors
///
/// [`Failure::NotFound`] when the site knows no such organization, else as [`Net::post`].
pub fn fetch_page(net: &dyn Net, sid: &str, page: u32) -> Result<Page, Failure> {
    let body = net.post(API, "application/json", request_body(sid, page).as_bytes())?;
    parse_answer(&text_of(&body))?.ok_or(Failure::NotFound)
}

/// The key of an organization in the file.
pub fn key_of(sid: &str) -> String {
    sid.to_lowercase()
}

/// The key of an organization at the gate: no handle has a colon.
fn gate_key(sid: &str) -> String {
    format!("org:{}", key_of(sid))
}

fn member_value(member: &Member) -> Value {
    let mut map: Map = Vec::new();
    if member.hidden() {
        map.push(("verborgen".into(), Value::Bool(true)));
    } else {
        map.push(("handle".into(), Value::str(&member.handle)));
    }
    map.push(("rang".into(), Value::str(&member.rank)));
    map.push(("sterne".into(), Value::int(i64::from(member.stars))));
    Value::Object(map)
}

fn member_from(value: &Value) -> Option<Member> {
    let rank = value.get("rang").and_then(Value::as_str).unwrap_or_default().to_string();
    let stars = match value.get("sterne") {
        Some(Value::Int(n)) => n.parse::<u8>().unwrap_or(0).min(5),
        _ => 0,
    };
    if value.get("verborgen") == Some(&Value::Bool(true)) {
        return Some(Member { handle: String::new(), rank, stars });
    }
    let handle = valid_handle(value.get("handle").and_then(Value::as_str)?)?;
    Some(Member { handle, rank, stars })
}

fn roster_value(roster: &Roster) -> Value {
    let mut map: Map = vec![
        ("geholt".into(), Value::Float(roster.fetched)),
        ("sid".into(), Value::str(&roster.sid)),
        ("seiten".into(), Value::int(i64::from(roster.pages))),
        ("ende".into(), Value::Bool(roster.end)),
    ];
    if let Some(total) = roster.total {
        map.push(("gesamt".into(), Value::int(total as i64)));
    }
    map.push(("mitglieder".into(), Value::Array(roster.members.iter().map(member_value).collect())));
    Value::Object(map)
}

fn roster_from(value: &Value) -> Option<Roster> {
    let number = |key: &str| match value.get(key) {
        Some(Value::Int(n)) => n.parse::<f64>().ok(),
        Some(Value::Float(x)) => Some(*x),
        _ => None,
    };
    let sid = valid_handle(value.get("sid").and_then(Value::as_str)?)?;
    let members: Vec<Member> = match value.get("mitglieder") {
        Some(Value::Array(list)) => list.iter().filter_map(member_from).take(PAGES_MAX as usize * PAGE_SIZE).collect(),
        _ => Vec::new(),
    };
    Some(Roster {
        sid,
        members,
        total: number("gesamt").filter(|n| *n >= 0.0).map(|n| n as u64),
        pages: number("seiten").map_or(1, |n| n.clamp(1.0, f64::from(PAGES_MAX)) as u32),
        end: value.get("ende") == Some(&Value::Bool(true)),
        fetched: number("geholt")?,
    })
}

/// The organizations kept, the newest last.
pub fn read(paths: &Paths) -> Vec<Roster> {
    let Some(data) = jsonio::load_json(&paths.app_file(CACHE_FILE), None, "org-mitglieder-cache") else {
        return Vec::new();
    };
    let Some(Value::Object(orgs)) = data.get("orgs") else { return Vec::new() };
    let mut list: Vec<Roster> =
        orgs.iter().filter_map(|(key, value)| roster_from(value).filter(|r| *key == key_of(&r.sid))).collect();
    list.sort_by(|a, b| a.fetched.total_cmp(&b.fetched));
    list
}

/// Writes the organizations kept, the newest [`ORGS_KEPT_MAX`].
fn write(paths: &Paths, mut list: Vec<Roster>) -> bool {
    list.sort_by(|a, b| a.fetched.total_cmp(&b.fetched));
    let skip = list.len().saturating_sub(ORGS_KEPT_MAX);
    let orgs: Map = list.iter().skip(skip).map(|r| (key_of(&r.sid), roster_value(r))).collect();
    let data = Value::Object(vec![("format".into(), Value::int(FORMAT)), ("orgs".into(), Value::Object(orgs))]);
    jsonio::save_json(&paths.app_file(CACHE_FILE), &data, 1, false).is_ok()
}

fn keep(paths: &Paths, roster: &Roster) -> bool {
    let key = key_of(&roster.sid);
    let mut list = read(paths);
    list.retain(|r| key_of(&r.sid) != key);
    list.push(roster.clone());
    write(paths, list)
}

/// The members kept of `sid`.
pub fn kept(paths: &Paths, sid: &str) -> Option<Roster> {
    let key = key_of(sid);
    read(paths).into_iter().find(|r| key_of(&r.sid) == key)
}

/// Forgets every organization kept. True when nothing is left.
pub fn clear(paths: &Paths) -> bool {
    read(paths).is_empty() || write(paths, Vec::new())
}

/// Joins `members` of the next page to `roster`: a handle already listed (the list moved between two pages) only
/// once.
fn join(roster: &mut Roster, members: Vec<Member>, total: Option<u64>) {
    let got = members.len();
    for member in members {
        if member.hidden() || !roster.members.iter().any(|m| m.handle.eq_ignore_ascii_case(&member.handle)) {
            roster.members.push(member);
        }
    }
    roster.pages += 1;
    roster.total = total.or(roster.total);
    roster.end = got < PAGE_SIZE || roster.total.is_some_and(|t| roster.members.len() as u64 >= t);
}

/// The first page of the members of `sid`: from the file while it is fresh (with every page kept), else from the
/// site. `force` asks the site in any case.
///
/// # Errors
///
/// [`Failure`] when `sid` is no SID, the site knows none, or it cannot be reached and nothing is kept;
/// [`Failure::Busy`] while the same organization is being asked.
pub fn load(paths: &Paths, net: &dyn Net, sid: &str, force: bool, now: f64) -> Result<Found, Failure> {
    load_with(&GATE, paths, net, sid, force, now)
}

/// [`load`] through `gate`.
///
/// # Errors
///
/// As [`load`].
pub fn load_with(
    gate: &Gate,
    paths: &Paths,
    net: &dyn Net,
    sid: &str,
    force: bool,
    now: f64,
) -> Result<Found, Failure> {
    let sid = valid_handle(sid).ok_or(Failure::Invalid)?;
    let kept = kept(paths, &sid);
    let from_file = |failed: Option<Failure>| {
        kept.clone()
            .map(|roster| Found { roster, live: false, failed: failed.clone() })
            .ok_or_else(|| failed.unwrap_or(Failure::Offline))
    };
    let days = cache::Settings::read(paths).days;
    if let Some(roster) = &kept
        && !force
        && cache::fresh(roster.fetched, now, days)
    {
        return from_file(None);
    }
    if net::offline_in(&paths.host) {
        return from_file(Some(Failure::Offline));
    }
    let Some(_ticket) = gate.enter(&gate_key(&sid)) else { return Err(Failure::Busy) };
    match fetch_page(&Paced { net, gate }, &sid, 1) {
        Ok((members, total)) => {
            let mut roster = Roster { sid, fetched: now, ..Roster::default() };
            join(&mut roster, members, total);
            keep(paths, &roster);
            Ok(Found { roster, live: true, failed: None })
        }
        Err(Failure::NotFound) => Err(Failure::NotFound),
        Err(failure) => from_file(Some(failure)),
    }
}

/// The next page of `roster` joined to it, and kept.
///
/// # Errors
///
/// [`Failure`] when no further page is to be had or the site cannot be reached; the roster shown stands.
pub fn more(paths: &Paths, net: &dyn Net, roster: &Roster) -> Result<Found, Failure> {
    more_with(&GATE, paths, net, roster)
}

/// [`more`] through `gate`.
///
/// # Errors
///
/// As [`more`].
pub fn more_with(gate: &Gate, paths: &Paths, net: &dyn Net, roster: &Roster) -> Result<Found, Failure> {
    if !roster.more() {
        return Err(Failure::NotFound);
    }
    if net::offline_in(&paths.host) {
        return Err(Failure::Offline);
    }
    let Some(_ticket) = gate.enter(&gate_key(&roster.sid)) else { return Err(Failure::Busy) };
    let (members, total) = fetch_page(&Paced { net, gate }, &roster.sid, roster.pages + 1)?;
    let mut next = roster.clone();
    join(&mut next, members, total);
    keep(paths, &next);
    Ok(Found { roster: next, live: true, failed: None })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::net::Fake;
    use crate::testdir::Dir as TestDir;
    use std::time::Duration;

    /// The answer of the site for page 1 of KRT, its shape kept and every player made up: three shown members
    /// (one an affiliate) and one hidden one.
    pub(crate) const PAGE: &str = include_str!("../../tests/daten/org-mitglieder-seite.json");

    /// An answer of `count` made-up members `Pilot<from>` on, of `total`.
    pub(crate) fn answer(from: usize, count: usize, total: u64) -> String {
        let items: String = (from..from + count)
            .map(|n| {
                format!(
                    "<li class=\"member-item js-member-item org-main org-visibility-V\"><a class=\"membercard\" \
                     href=\"/citizens/Pilot{n}\"><span class=\"stars\" style=\"width: 20%;\"></span>\
                     <span class=\"rank\">Rank{n}</span></a></li>"
                )
            })
            .collect();
        let html = jsonio::dumps(&Value::str(items), None, false);
        format!(r#"{{"success":1,"data":{{"totalrows":{total},"html":{html}}},"code":"OK","msg":"OK"}}"#)
    }

    /// A site with an organization of 40 members on two pages.
    pub(crate) fn site(sid: &str) -> Fake {
        Fake::default().with_post(API, request_body(sid, 1), answer(0, PAGE_SIZE, 40)).with_post(
            API,
            request_body(sid, 2),
            answer(PAGE_SIZE, 8, 40),
        )
    }

    fn quick() -> Gate {
        Gate::new(Duration::ZERO)
    }

    #[test]
    fn the_answer_of_the_site_gives_handles_ranks_stars_and_hidden_ones() {
        let (members, total) = parse_answer(PAGE).expect("JSON").expect("success");
        assert_eq!(total, Some(3724));
        let shown: Vec<(&str, &str, u8)> =
            members.iter().map(|m| (m.handle.as_str(), m.rank.as_str(), m.stars)).collect();
        assert_eq!(
            shown,
            [
                ("Testpilot_Eins", "Pilot", 0),
                ("Testpilot-Zwei", "Officer", 3),
                ("TestpilotDrei", "Founder", 5),
                ("", "Pilot", 0),
            ]
        );
        assert!(members[3].hidden() && !members[0].hidden());
        // Counter-check: the same member made invisible is hidden although it still links a citizen.
        let redacted = PAGE.replacen("org-visibility-V", "org-visibility-R", 1);
        let (members, _) = parse_answer(&redacted).expect("JSON").expect("success");
        assert!(members[0].hidden() && !members[1].hidden());
    }

    #[test]
    fn a_refusal_of_the_site_and_a_broken_answer_are_told_apart() {
        assert_eq!(parse_answer(r#"{"success":0,"code":"ErrInvalidOrg","msg":"x","data":null}"#), Ok(None));
        assert!(parse_answer("<html>Wartung</html>").is_err());
        assert!(parse_answer(r#"{"success":1,"data":{}}"#).is_err());
        assert_eq!(request_body("KRT", 2), r#"{"symbol":"KRT","search":"","pagesize":32,"page":2}"#);
    }

    #[test]
    fn the_first_page_comes_at_once_and_more_only_when_asked() {
        let dir = TestDir::new("org-mitglieder-seiten");
        let paths = cache::tests::paths(&dir, &[]);
        let gate = quick();
        let net = site("TEST");
        let first = load_with(&gate, &paths, &net, "TEST", false, 1000.0).expect("found");
        assert!(first.live);
        assert_eq!((first.roster.members.len(), first.roster.pages, first.roster.total), (PAGE_SIZE, 1, Some(40)));
        assert!(first.roster.more());
        assert_eq!(net.asked(), [format!("POST {API} {}", request_body("TEST", 1))], "only the first page");
        let second = more_with(&gate, &paths, &net, &first.roster).expect("more");
        assert_eq!((second.roster.members.len(), second.roster.pages), (40, 2));
        assert!(second.roster.end && !second.roster.more(), "the site has no more");
        assert_eq!(second.roster.members[39].handle, "Pilot39");
        assert_eq!(net.asked().len(), 2);
        // Kept with both pages: the file answers, the site is not asked.
        let net = site("TEST");
        let again = load_with(&gate, &paths, &net, "test", false, 1000.0 + 3600.0).expect("kept");
        assert!(!again.live);
        assert_eq!(again.roster, second.roster);
        assert!(net.asked().is_empty());
        // Counter-check: older than the days set, the first page is asked anew.
        let late = load_with(&gate, &paths, &net, "TEST", false, 1000.0 + 8.0 * 86_400.0).expect("found");
        assert!(late.live && late.roster.pages == 1);
        assert_eq!(more_with(&gate, &paths, &net, &second.roster), Err(Failure::NotFound), "nothing past the end");
    }

    #[test]
    fn without_net_nothing_is_asked_and_the_members_kept_stand() {
        let dir = TestDir::new("org-mitglieder-offline");
        let online = cache::tests::paths(&dir, &[]);
        let gate = quick();
        let first = load_with(&gate, &online, &site("TEST"), "TEST", false, 1000.0).expect("found");
        let offline = cache::tests::paths(&dir, &[(crate::legacy::ENV_NO_NET, "1")]);
        let net = site("TEST");
        let found = load_with(&gate, &offline, &net, "TEST", true, 1000.0 + 90.0 * 86_400.0).expect("kept");
        assert_eq!(found.failed, Some(Failure::Offline));
        assert_eq!(more_with(&gate, &offline, &net, &first.roster), Err(Failure::Offline));
        assert_eq!(load_with(&gate, &offline, &net, "NONE", false, 0.0), Err(Failure::Offline));
        assert!(net.asked().is_empty());
        assert_eq!(load_with(&gate, &online, &net, "a/b", false, 0.0), Err(Failure::Invalid));
    }

    #[test]
    fn an_unknown_organization_is_not_found_and_one_is_never_asked_twice_at_once() {
        let dir = TestDir::new("org-mitglieder-sperre");
        let paths = cache::tests::paths(&dir, &[]);
        let gate = quick();
        let refusal = Fake::default().with_post(API, request_body("NONE", 1), r#"{"success":0,"data":null}"#);
        assert_eq!(load_with(&gate, &paths, &refusal, "NONE", false, 0.0), Err(Failure::NotFound));
        let ticket = gate.enter(&gate_key("TEST")).expect("free");
        assert_eq!(load_with(&gate, &paths, &site("TEST"), "TEST", true, 0.0), Err(Failure::Busy));
        assert!(load_with(&gate, &paths, &site("ZWEI"), "ZWEI", true, 0.0).is_ok(), "another goes on");
        assert!(gate.enter("test").is_some(), "a handle of the same name is another key");
        drop(ticket);
        assert!(load_with(&gate, &paths, &site("TEST"), "TEST", true, 0.0).is_ok());
    }

    #[test]
    fn the_file_keeps_the_newest_organizations_and_bounded_lists() {
        let dir = TestDir::new("org-mitglieder-datei");
        let paths = cache::tests::paths(&dir, &[]);
        let members = vec![
            Member { handle: "Eins".into(), rank: "Pilot".into(), stars: 2 },
            Member { handle: String::new(), rank: "Recruit".into(), stars: 0 },
        ];
        let roster = Roster { sid: "TEST".into(), members, total: Some(2), pages: 1, end: true, fetched: 5.0 };
        assert!(keep(&paths, &roster));
        assert_eq!(kept(&paths, "test"), Some(roster.clone()), "everything shown comes back from the file");
        let list: Vec<Roster> = (0..ORGS_KEPT_MAX + 2)
            .map(|n| Roster { sid: format!("ORG{n}"), fetched: n as f64, ..roster.clone() })
            .collect();
        assert!(write(&paths, list));
        let back = read(&paths);
        assert_eq!(back.len(), ORGS_KEPT_MAX);
        assert_eq!(back[0].sid, "ORG2", "the oldest went first");
        assert!(clear(&paths) && read(&paths).is_empty());
    }

    #[test]
    fn a_handle_the_next_page_repeats_is_listed_once() {
        let mut roster = Roster { sid: "TEST".into(), ..Roster::default() };
        join(&mut roster, parse_members(&answer_html(0, PAGE_SIZE)), Some(100));
        join(&mut roster, parse_members(&answer_html(PAGE_SIZE - 2, PAGE_SIZE)), Some(100));
        assert_eq!(roster.members.len(), 2 * PAGE_SIZE - 2);
        assert!(!roster.end, "a full page: the site may have more");
        let mut short = roster.clone();
        join(&mut short, Vec::new(), None);
        assert!(short.end, "an empty page ends it");
    }

    fn answer_html(from: usize, count: usize) -> String {
        let text = answer(from, count, 0);
        jsonio::loads(&text).ok().and_then(|v| v.get("data")?.get("html")?.as_str().map(str::to_string)).unwrap()
    }
}
