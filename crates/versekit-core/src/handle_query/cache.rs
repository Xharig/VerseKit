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

//! The handles found, kept in the data folder (`handle-cache.json` in `Intern`), and the rules that keep the
//! search sparing: a handle found is asked again only when it is older than the days set, or when the player
//! forces it; one handle is never asked twice at once, and between two pages of the site there is a pause.
//!
//! Without net, or when the site cannot be reached, the handle kept stands, however old.

use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use super::page::{self, Membership, Org, Orgs, Profile};
use super::{Card, Failure, fetch};
use crate::jsonio::{self, Map, Value};
use crate::net::{self, FetchError, Headers, Net};
use crate::paths::Paths;

/// The file of the handles kept.
pub const CACHE_FILE: &str = "handle-cache.json";
/// The form of the file.
const FORMAT: i64 = 1;
/// The most handles kept; the oldest go first.
pub const KEPT_MAX: usize = 500;
/// The shortest time between two pages asked of the site.
pub const PAUSE: Duration = Duration::from_millis(1500);

/// How old a handle kept may be, in days (`0` asks every time).
pub const KEY_DAYS: &str = "handle_cache_tage";
pub const DAYS_DEFAULT: i64 = 7;
pub const DAYS_MAX: i64 = 365;
/// How many affiliations the search shows besides the main organization; a value above the most shows the most.
pub const KEY_AFFILIATIONS: &str = "handle_affiliationen";
pub const AFFILIATIONS_DEFAULT: i64 = 2;
pub const AFFILIATIONS_MAX: i64 = 3;
/// Whether hidden and redacted affiliations are left out.
pub const KEY_HIDE_HIDDEN: &str = "handle_verborgene_ausblenden";
/// After how many seconds the card and the field empty themselves (`0` keeps them).
pub const KEY_CLEAR_AFTER: &str = "handle_leeren_nach_sekunden";
pub const CLEAR_AFTER_DEFAULT: i64 = 30;
pub const CLEAR_AFTER_MAX: i64 = 600;

/// The settings of the search.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub days: i64,
    pub affiliations: usize,
    pub hide_hidden: bool,
    pub clear_after: i64,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            days: DAYS_DEFAULT,
            affiliations: AFFILIATIONS_DEFAULT as usize,
            hide_hidden: false,
            clear_after: CLEAR_AFTER_DEFAULT,
        }
    }
}

impl Settings {
    pub fn read(paths: &Paths) -> Settings {
        Settings {
            days: paths.setting_int(KEY_DAYS, DAYS_DEFAULT, Some(0), Some(DAYS_MAX)),
            affiliations: paths.setting_int(KEY_AFFILIATIONS, AFFILIATIONS_DEFAULT, Some(0), Some(AFFILIATIONS_MAX))
                as usize,
            hide_hidden: paths.setting_bool(KEY_HIDE_HIDDEN, false),
            clear_after: paths.setting_int(KEY_CLEAR_AFTER, CLEAR_AFTER_DEFAULT, Some(0), Some(CLEAR_AFTER_MAX)),
        }
    }

    /// Whether a card shown since `since` is to be emptied at `now`.
    pub fn clear_due(&self, since: f64, now: f64) -> bool {
        self.clear_after > 0 && now - since >= self.clear_after as f64
    }

    /// The affiliations of `orgs` the search shows: hidden ones left out when that is set, at most the number set.
    pub fn affiliations<'a>(&self, orgs: &'a Orgs) -> Vec<&'a Membership> {
        orgs.affiliations.iter().filter(|m| !self.hide_hidden || m.org().is_some()).take(self.affiliations).collect()
    }
}

/// One handle as the search has it.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub card: Card,
    /// When it was fetched (seconds).
    pub fetched: f64,
    /// Fetched just now, not taken from the file.
    pub live: bool,
    /// Asked and not reached: the handle kept stands, this is why.
    pub failed: Option<Failure>,
}

/// Keeps the search sparing: which handles are being asked, and when the site was asked last.
pub struct Gate {
    busy: Mutex<Vec<String>>,
    last: Mutex<Option<Instant>>,
    pause: Duration,
}

/// The one gate of the program: the handles and the members of organizations share its pause.
pub(crate) static GATE: Gate = Gate::new(PAUSE);

impl Gate {
    pub const fn new(pause: Duration) -> Gate {
        Gate { busy: Mutex::new(Vec::new()), last: Mutex::new(None), pause }
    }

    /// Takes `key` for one search; `None` while another search of it runs.
    pub(crate) fn enter(&self, key: &str) -> Option<Ticket<'_>> {
        let mut busy = self.busy.lock().unwrap_or_else(PoisonError::into_inner);
        if busy.iter().any(|k| k == key) {
            return None;
        }
        busy.push(key.to_string());
        Some(Ticket { gate: self, key: key.to_string() })
    }

    /// Waits until the pause since the last page asked is over, and counts this one as the last.
    pub(crate) fn wait_turn(&self) {
        let mut last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(before) = *last {
            let since = before.elapsed();
            if since < self.pause {
                std::thread::sleep(self.pause - since);
            }
        }
        *last = Some(Instant::now());
    }

    /// Whether a search of `key` runs.
    pub fn is_busy(&self, key: &str) -> bool {
        self.busy.lock().unwrap_or_else(PoisonError::into_inner).iter().any(|k| k == key)
    }
}

/// A search of one handle running; gives the handle free when it ends.
pub(crate) struct Ticket<'a> {
    gate: &'a Gate,
    key: String,
}

impl Drop for Ticket<'_> {
    fn drop(&mut self) {
        self.gate.busy.lock().unwrap_or_else(PoisonError::into_inner).retain(|k| *k != self.key);
    }
}

/// The net with the pause of the gate before every page.
pub(crate) struct Paced<'a> {
    pub(crate) net: &'a dyn Net,
    pub(crate) gate: &'a Gate,
}

impl Net for Paced<'_> {
    fn get(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        self.gate.wait_turn();
        self.net.get(url)
    }

    fn post(&self, url: &str, content_type: &str, body: &[u8]) -> Result<Vec<u8>, FetchError> {
        self.gate.wait_turn();
        self.net.post(url, content_type, body)
    }

    fn head(&self, url: &str) -> Result<Headers, FetchError> {
        self.gate.wait_turn();
        self.net.head(url)
    }
}

/// The key of a handle in the file: the site does not tell capitals from small letters.
pub fn key_of(handle: &str) -> String {
    handle.to_lowercase()
}

fn text(map: &Map, key: &str) -> String {
    map.iter().find(|(k, _)| k == key).and_then(|(_, v)| v.as_str()).unwrap_or_default().to_string()
}

fn number(map: &Map, key: &str) -> Option<f64> {
    match map.iter().find(|(k, _)| k == key).map(|(_, v)| v) {
        Some(Value::Int(n)) => n.parse().ok(),
        Some(Value::Float(x)) => Some(*x),
        _ => None,
    }
}

fn org_value(org: &Org) -> Value {
    let mut map: Map = vec![
        ("sid".into(), Value::str(&org.sid)),
        ("name".into(), Value::str(&org.name)),
        ("rang".into(), Value::str(&org.rank)),
        ("sterne".into(), Value::int(i64::from(org.stars))),
        ("logo".into(), Value::str(&org.logo)),
    ];
    if let Some(members) = org.members {
        map.push(("mitglieder".into(), Value::int(members as i64)));
    }
    for (key, value) in
        [("taetigkeit", &org.activity), ("nebentaetigkeit", &org.second_activity), ("einsatz", &org.commitment)]
    {
        if !value.is_empty() {
            map.push((key.into(), Value::str(value)));
        }
    }
    Value::Object(map)
}

fn membership_value(membership: &Membership) -> Value {
    match membership {
        Membership::Shown(org) => org_value(org),
        Membership::Hidden => Value::Object(vec![("verborgen".into(), Value::Bool(true))]),
    }
}

fn membership_from(value: &Value) -> Option<Membership> {
    let map = value.as_object()?;
    if map.iter().any(|(k, v)| k == "verborgen" && *v == Value::Bool(true)) {
        return Some(Membership::Hidden);
    }
    let sid = text(map, "sid");
    page::valid_handle(&sid)?;
    Some(Membership::Shown(Org {
        name: text(map, "name"),
        rank: text(map, "rang"),
        stars: number(map, "sterne").map_or(0, |n| n.clamp(0.0, 5.0) as u8),
        members: number(map, "mitglieder").filter(|n| *n >= 0.0).map(|n| n as u64),
        logo: text(map, "logo"),
        activity: text(map, "taetigkeit"),
        second_activity: text(map, "nebentaetigkeit"),
        commitment: text(map, "einsatz"),
        sid,
    }))
}

fn entry_value(card: &Card, fetched: f64) -> Value {
    let profile = &card.profile;
    let mut map: Map = vec![
        ("geholt".into(), Value::Float(fetched)),
        ("handle".into(), Value::str(&profile.handle)),
        ("name".into(), Value::str(&profile.name)),
        ("seit".into(), Value::str(&profile.enlisted)),
        ("titel".into(), Value::str(&profile.title)),
        ("titel_bild".into(), Value::str(&profile.title_image)),
        ("bild".into(), Value::str(&profile.image)),
    ];
    if !profile.record.is_empty() {
        map.push(("nummer".into(), Value::str(&profile.record)));
    }
    if !profile.fluency.is_empty() {
        map.push(("sprachen".into(), Value::Array(profile.fluency.iter().map(Value::str).collect())));
    }
    if let Some(main) = &card.orgs.main {
        map.push(("haupt".into(), membership_value(main)));
    }
    map.push(("weitere".into(), Value::Array(card.orgs.affiliations.iter().map(membership_value).collect())));
    Value::Object(map)
}

fn entry_from(value: &Value) -> Option<(Card, f64)> {
    let map = value.as_object()?;
    let fetched = number(map, "geholt")?;
    let handle = page::valid_handle(&text(map, "handle"))?;
    let field = |key: &str| map.iter().find(|(k, _)| k == key).map(|(_, v)| v);
    let profile = Profile {
        handle,
        name: text(map, "name"),
        enlisted: text(map, "seit"),
        title: text(map, "titel"),
        title_image: text(map, "titel_bild"),
        image: text(map, "bild"),
        record: text(map, "nummer"),
        fluency: match field("sprachen") {
            Some(Value::Array(list)) => list.iter().filter_map(|l| l.as_str().map(str::to_string)).collect(),
            _ => Vec::new(),
        },
    };
    let orgs = Orgs {
        main: field("haupt").and_then(membership_from),
        affiliations: match field("weitere") {
            Some(Value::Array(list)) => list.iter().filter_map(membership_from).collect(),
            _ => Vec::new(),
        },
    };
    Some((Card { profile, orgs }, fetched))
}

/// The handles kept: key, card, time fetched; the newest last.
pub fn read(paths: &Paths) -> Vec<(String, Card, f64)> {
    let Some(data) = jsonio::load_json(&paths.app_file(CACHE_FILE), None, "handle-cache") else { return Vec::new() };
    let Some(Value::Object(handles)) = data.get("handles") else { return Vec::new() };
    let mut list: Vec<(String, Card, f64)> = handles
        .iter()
        .filter_map(|(key, value)| entry_from(value).map(|(card, at)| (key.clone(), card, at)))
        .filter(|(key, card, _)| *key == key_of(&card.profile.handle))
        .collect();
    list.sort_by(|a, b| a.2.total_cmp(&b.2));
    list
}

/// Writes the handles kept, the newest [`KEPT_MAX`].
fn write(paths: &Paths, mut list: Vec<(String, Card, f64)>) -> bool {
    list.sort_by(|a, b| a.2.total_cmp(&b.2));
    let skip = list.len().saturating_sub(KEPT_MAX);
    let handles: Map = list.iter().skip(skip).map(|(key, card, at)| (key.clone(), entry_value(card, *at))).collect();
    let data = Value::Object(vec![("format".into(), Value::int(FORMAT)), ("handles".into(), Value::Object(handles))]);
    jsonio::save_json(&paths.app_file(CACHE_FILE), &data, 1, false).is_ok()
}

/// Keeps `card` as fetched at `at`.
fn keep(paths: &Paths, card: &Card, at: f64) -> bool {
    let key = key_of(&card.profile.handle);
    let mut list = read(paths);
    list.retain(|(k, _, _)| *k != key);
    list.push((key, card.clone(), at));
    write(paths, list)
}

/// Forgets `key`: the site knows the handle no more.
fn forget(paths: &Paths, key: &str) -> bool {
    let mut list = read(paths);
    let before = list.len();
    list.retain(|(k, _, _)| k != key);
    list.len() == before || write(paths, list)
}

/// Forgets every handle kept. True when nothing is left.
pub fn clear(paths: &Paths) -> bool {
    read(paths).is_empty() || write(paths, Vec::new())
}

/// The handle kept for `handle`, with the time it was fetched.
pub fn kept(paths: &Paths, handle: &str) -> Option<(Card, f64)> {
    let key = key_of(handle);
    read(paths).into_iter().find(|(k, _, _)| *k == key).map(|(_, card, at)| (card, at))
}

/// Whether a handle fetched at `fetched` is still fresh at `now` with `days` set.
pub fn fresh(fetched: f64, now: f64, days: i64) -> bool {
    days > 0 && now - fetched < days as f64 * 86_400.0 && fetched <= now + 60.0
}

/// Searches `handle`: from the file while it is fresh, else from the site. `force` asks the site in any case
/// (Ctrl+Enter).
///
/// # Errors
///
/// [`Failure`] when the handle is not one, the site knows none, or it cannot be reached and nothing is kept;
/// [`Failure::Busy`] while the same handle is being asked.
pub fn search(paths: &Paths, net: &dyn Net, handle: &str, force: bool, now: f64) -> Result<Found, Failure> {
    search_with(&GATE, paths, net, handle, force, now)
}

/// [`search`] through `gate`.
///
/// # Errors
///
/// As [`search`].
pub fn search_with(
    gate: &Gate,
    paths: &Paths,
    net: &dyn Net,
    handle: &str,
    force: bool,
    now: f64,
) -> Result<Found, Failure> {
    let handle = page::valid_handle(handle).ok_or(Failure::Invalid)?;
    let key = key_of(&handle);
    let settings = Settings::read(paths);
    let kept = kept(paths, &handle);
    let from_file = |failed: Option<Failure>| {
        kept.clone()
            .map(|(card, fetched)| Found { card, fetched, live: false, failed: failed.clone() })
            .ok_or_else(|| failed.unwrap_or(Failure::Offline))
    };
    if let Some((_, fetched)) = &kept
        && !force
        && fresh(*fetched, now, settings.days)
    {
        return from_file(None);
    }
    if net::offline_in(&paths.host) {
        return from_file(Some(Failure::Offline));
    }
    let Some(_ticket) = gate.enter(&key) else { return Err(Failure::Busy) };
    match fetch(&Paced { net, gate }, &handle) {
        Ok(card) => {
            // Under the key of the handle as the site writes it.
            if key_of(&card.profile.handle) != key {
                forget(paths, &key);
            }
            keep(paths, &card, now);
            Ok(Found { card, fetched: now, live: true, failed: None })
        }
        Err(Failure::NotFound) => {
            forget(paths, &key);
            Err(Failure::NotFound)
        }
        Err(failure) => from_file(Some(failure)),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::net::Fake;
    use crate::paths::Host;
    use crate::pypath::Flavor;
    use crate::testdir::Dir as TestDir;

    const DAY: f64 = 86_400.0;

    pub(crate) fn paths(dir: &TestDir, extra: &[(&str, &str)]) -> Paths {
        let home = dir.path().to_string_lossy().into_owned();
        let mut vars = vec![(crate::legacy::ENV_HOME, home.as_str())];
        vars.extend_from_slice(extra);
        Paths::new(Host::new(Flavor::native(), &vars, &home))
    }

    /// A site that cannot be reached.
    struct Down;

    impl Net for Down {
        fn get(&self, _url: &str) -> Result<Vec<u8>, FetchError> {
            Err(FetchError::Failed("timeout".to_string()))
        }

        fn head(&self, _url: &str) -> Result<Headers, FetchError> {
            Err(FetchError::Failed("timeout".to_string()))
        }
    }

    fn site() -> Fake {
        Fake::default()
            .with(&page::profile_url("Musterpilot"), page::tests::PROFILE)
            .with(&page::orgs_url("Musterpilot"), page::tests::ORGS)
    }

    fn quick() -> Gate {
        Gate::new(Duration::ZERO)
    }

    #[test]
    fn a_fresh_handle_comes_from_the_file_and_an_old_one_from_the_site() {
        let dir = TestDir::new("handle-cache-frisch");
        let paths = paths(&dir, &[]);
        let gate = quick();
        let first = search_with(&gate, &paths, &site(), "Musterpilot", false, 1000.0).expect("found");
        assert!(first.live);
        assert!(std::path::Path::new(&paths.app_file(CACHE_FILE)).is_file());
        // A day later the file answers, the site is not asked.
        let net = site();
        let again = search_with(&gate, &paths, &net, "MUSTERPILOT", false, 1000.0 + DAY).expect("kept");
        assert!(!again.live);
        assert_eq!(again.card, first.card);
        assert!(net.asked().is_empty());
        // Counter-check: older than the seven days set, the site is asked again.
        let net = site();
        let late = search_with(&gate, &paths, &net, "Musterpilot", false, 1000.0 + 8.0 * DAY).expect("found");
        assert!(late.live);
        assert_eq!(net.asked().len(), 2);
    }

    #[test]
    fn forcing_asks_the_site_even_when_the_handle_is_fresh() {
        let dir = TestDir::new("handle-cache-zwang");
        let paths = paths(&dir, &[]);
        let gate = quick();
        search_with(&gate, &paths, &site(), "Musterpilot", false, 1000.0).expect("found");
        let net = site();
        let forced = search_with(&gate, &paths, &net, "Musterpilot", true, 1001.0).expect("found");
        assert!(forced.live);
        assert_eq!(net.asked().len(), 2);
    }

    #[test]
    fn the_card_empties_after_the_seconds_set_and_zero_keeps_it() {
        let dir = TestDir::new("handle-leeren");
        let paths = paths(&dir, &[]);
        let standard = Settings::read(&paths);
        assert_eq!(standard.clear_after, CLEAR_AFTER_DEFAULT);
        assert!(!standard.clear_due(100.0, 129.9) && standard.clear_due(100.0, 130.0));
        assert!(paths.set_setting(KEY_CLEAR_AFTER, Value::int(0)));
        assert!(!Settings::read(&paths).clear_due(0.0, 1e9));
        assert!(paths.set_setting(KEY_CLEAR_AFTER, Value::int(99_999)));
        assert_eq!(Settings::read(&paths).clear_after, CLEAR_AFTER_MAX);
    }

    #[test]
    fn the_days_set_count_and_zero_always_asks() {
        let dir = TestDir::new("handle-cache-tage");
        let paths = paths(&dir, &[]);
        assert!(paths.set_setting(KEY_DAYS, Value::int(0)));
        assert_eq!(Settings::read(&paths).days, 0);
        let gate = quick();
        search_with(&gate, &paths, &site(), "Musterpilot", false, 1000.0).expect("found");
        let net = site();
        assert!(search_with(&gate, &paths, &net, "Musterpilot", false, 1001.0).expect("found").live);
        assert!(paths.set_setting(KEY_DAYS, Value::int(9999)));
        assert_eq!(Settings::read(&paths).days, DAYS_MAX);
        assert!(fresh(0.0, 2.0 * DAY, 3) && !fresh(0.0, 3.0 * DAY, 3));
        // A time ahead of now (a clock put back) is not fresh.
        assert!(!fresh(10.0 * DAY, 0.0, 30));
    }

    #[test]
    fn without_net_the_kept_handle_stands_however_old() {
        let dir = TestDir::new("handle-cache-offline");
        let online = paths(&dir, &[]);
        let gate = quick();
        search_with(&gate, &online, &site(), "Musterpilot", false, 1000.0).expect("found");
        let offline = paths(&dir, &[(crate::legacy::ENV_NO_NET, "1")]);
        let net = site();
        let found = search_with(&gate, &offline, &net, "Musterpilot", true, 1000.0 + 90.0 * DAY).expect("kept");
        assert!(!found.live);
        assert_eq!(found.failed, Some(Failure::Offline));
        assert!(net.asked().is_empty());
        assert_eq!(search_with(&gate, &offline, &net, "Niemand", false, 0.0), Err(Failure::Offline));
    }

    #[test]
    fn an_unreachable_site_shows_the_kept_handle_with_the_reason() {
        let dir = TestDir::new("handle-cache-fehler");
        let paths = paths(&dir, &[]);
        let gate = quick();
        search_with(&gate, &paths, &site(), "Musterpilot", false, 1000.0).expect("found");
        let found = search_with(&gate, &paths, &Down, "Musterpilot", true, 2000.0).expect("kept");
        assert!(!found.live);
        assert_eq!(found.fetched, 1000.0);
        assert_eq!(found.failed, Some(Failure::Failed("timeout".to_string())));
        assert_eq!(search_with(&gate, &paths, &Down, "Niemand", true, 2000.0), Err(Failure::Failed("timeout".into())));
    }

    #[test]
    fn a_handle_the_site_knows_no_more_is_forgotten() {
        let dir = TestDir::new("handle-cache-weg");
        let paths = paths(&dir, &[]);
        let gate = quick();
        search_with(&gate, &paths, &site(), "Musterpilot", false, 1000.0).expect("found");
        assert_eq!(search_with(&gate, &paths, &Fake::default(), "Musterpilot", true, 2000.0), Err(Failure::NotFound));
        assert!(kept(&paths, "Musterpilot").is_none());
    }

    #[test]
    fn one_handle_is_never_asked_twice_at_once() {
        let dir = TestDir::new("handle-cache-sperre");
        let paths = paths(&dir, &[]);
        let gate = quick();
        let ticket = gate.enter("musterpilot").expect("free");
        assert!(gate.is_busy("musterpilot"));
        let net = site();
        assert_eq!(search_with(&gate, &paths, &net, "Musterpilot", true, 0.0), Err(Failure::Busy));
        assert!(net.asked().is_empty());
        // Another handle goes on.
        assert_eq!(search_with(&gate, &paths, &net, "Andere", true, 0.0), Err(Failure::NotFound));
        drop(ticket);
        assert!(!gate.is_busy("musterpilot"));
        assert!(search_with(&gate, &paths, &site(), "Musterpilot", true, 0.0).is_ok());
    }

    #[test]
    fn between_two_pages_the_gate_pauses() {
        let gate = Gate::new(Duration::from_millis(120));
        let started = Instant::now();
        gate.wait_turn();
        gate.wait_turn();
        gate.wait_turn();
        assert!(started.elapsed() >= Duration::from_millis(240), "{:?}", started.elapsed());
        // Counter-check: without a pause there is no wait.
        let quick = quick();
        let started = Instant::now();
        quick.wait_turn();
        quick.wait_turn();
        assert!(started.elapsed() < Duration::from_millis(100));
    }

    #[test]
    fn the_file_keeps_the_card_whole_and_the_newest_handles() {
        let dir = TestDir::new("handle-cache-datei");
        let paths = paths(&dir, &[]);
        let card = fetch(&site(), "Musterpilot").expect("found");
        assert!(keep(&paths, &card, 5.0));
        let (back, at) = kept(&paths, "musterPILOT").expect("kept");
        assert_eq!(back, card, "everything shown comes back from the file");
        assert_eq!(at, 5.0);
        let mut list: Vec<(String, Card, f64)> = Vec::new();
        for n in 0..KEPT_MAX + 3 {
            let mut other = card.clone();
            other.profile.handle = format!("Pilot{n}");
            list.push((key_of(&other.profile.handle), other, n as f64));
        }
        assert!(write(&paths, list));
        let kept_now = read(&paths);
        assert_eq!(kept_now.len(), KEPT_MAX);
        assert_eq!(kept_now[0].0, "pilot3", "the oldest went first");
        assert!(clear(&paths));
        assert!(read(&paths).is_empty());
    }

    #[test]
    fn an_entry_written_before_number_and_languages_reads_on() {
        let card = fetch(&site(), "Musterpilot").expect("found");
        assert_eq!((card.profile.record.as_str(), card.profile.fluency.len()), ("#100001", 2));
        let Value::Object(mut map) = entry_value(&card, 5.0) else { panic!("an object") };
        map.retain(|(k, _)| k != "nummer" && k != "sprachen");
        let (old, _) = entry_from(&Value::Object(map)).expect("still read");
        assert!(old.profile.record.is_empty() && old.profile.fluency.is_empty());
        assert_eq!(old.profile.handle, "Musterpilot");
        assert_eq!(old.orgs, card.orgs);
    }

    #[test]
    fn the_settings_choose_the_affiliations_shown() {
        let orgs = parse(page::tests::ORGS);
        // Three organizations by default: the main one and two affiliations; an old larger number shows four.
        let dir = TestDir::new("handle-orgs-zahl");
        let paths = paths(&dir, &[]);
        assert_eq!(Settings::read(&paths).affiliations(&orgs).len(), 2);
        assert!(paths.set_setting(KEY_AFFILIATIONS, Value::int(20)));
        assert_eq!(Settings::read(&paths).affiliations, 3);
        assert!(paths.set_setting(KEY_AFFILIATIONS, Value::int(0)));
        assert!(Settings::read(&paths).affiliations(&orgs).is_empty(), "the main one alone");
        let all = Settings { days: 7, affiliations: 20, hide_hidden: false, clear_after: 30 };
        assert_eq!(all.affiliations(&orgs).len(), 4);
        let shown = Settings { hide_hidden: true, ..all };
        assert_eq!(shown.affiliations(&orgs).iter().filter_map(|m| m.org()).count(), 2);
        assert_eq!(shown.affiliations(&orgs).len(), 2);
        let two = Settings { affiliations: 2, ..all };
        assert_eq!(two.affiliations(&orgs)[1], &Membership::Hidden);
        assert_eq!(two.affiliations(&orgs).len(), 2);
        assert!(Settings { affiliations: 0, ..all }.affiliations(&orgs).is_empty());
    }

    fn parse(html: &str) -> Orgs {
        page::parse_orgs(html)
    }
}
