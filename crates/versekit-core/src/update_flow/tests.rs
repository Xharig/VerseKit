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

use std::fs;

use super::*;
use crate::net::{Fake, FetchError};
use crate::paths::Host;
use crate::testdir::Dir;

const BASE: &str = "https://github.com/Xharig/VerseKit/releases/download/v5.0.1";
const IMAGE: &str = "VerseKit-x86_64.AppImage";

fn sha(bytes: &[u8]) -> String {
    let dir = Dir::new("flow-summe");
    let file = dir.path().join("x");
    fs::write(&file, bytes).unwrap();
    updater::checksum_of(&file).unwrap()
}

fn api(published: &str, assets: &str) -> String {
    format!(r#"[{{"tag_name":"v5.0.1","published_at":"{published}","prerelease":false,"assets":[{assets}]}}]"#)
}

fn image_asset() -> String {
    format!(
        r#"{{"name":"{IMAGE}","browser_download_url":"{BASE}/{IMAGE}"}},{{"name":"SHA256SUMS.txt","browser_download_url":"{BASE}/SHA256SUMS.txt"}}"#
    )
}

/// GitHub with the release 5.0.1: its AppImage and the checksums.
fn github(content: &[u8]) -> Fake {
    Fake::default()
        .with(updater::API, api("2026-11-20T09:00:00Z", &image_asset()).into_bytes())
        .with(&format!("{BASE}/SHA256SUMS.txt"), format!("{}  {IMAGE}\n", sha(content)).into_bytes())
        .with(&format!("{BASE}/{IMAGE}"), content.to_vec())
}

fn host_in(dir: &Path) -> Host {
    let text = dir.join("daten").to_string_lossy().into_owned();
    fs::create_dir_all(&text).unwrap();
    Host::new(crate::pypath::Flavor::native(), &[(crate::legacy::ENV_HOME, &text)], &text)
}

struct Ends(u32);

impl Started for Ends {
    fn ended(&mut self) -> Option<i32> {
        if self.0 == 0 {
            return Some(127);
        }
        self.0 -= 1;
        None
    }
}

/// A net that hands the file out in pieces, as a real download does.
struct Pieces(Fake);

impl Net for Pieces {
    fn get(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        self.0.get(url)
    }

    fn head(&self, url: &str) -> Result<crate::net::Headers, FetchError> {
        self.0.head(url)
    }

    fn download(
        &self,
        url: &str,
        out: &mut dyn io::Write,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<u64, FetchError> {
        let body = self.0.get(url)?;
        let total = body.len() as u64;
        let mut written = 0;
        for piece in body.chunks(body.len().div_ceil(8)) {
            out.write_all(piece).map_err(|e| FetchError::Failed(e.to_string()))?;
            written += piece.len() as u64;
            // Twice the same value and then a smaller one: the display must not see either.
            progress(written, Some(total));
            progress(written, Some(total));
        }
        Ok(total)
    }
}

struct Case {
    dir: Dir,
    paths: Paths,
    appimage: PathBuf,
    text: String,
}

fn case(name: &str) -> Case {
    let dir = Dir::new(name);
    let paths = Paths::new(host_in(dir.path()));
    let appimage = dir.path().join(IMAGE);
    fs::write(&appimage, b"alt").unwrap();
    let text = appimage.to_string_lossy().into_owned();
    Case { dir, paths, appimage, text }
}

fn release_of(net: &dyn Net, paths: &Paths) -> Release {
    updater::check(net, paths, "5.0.0", true, auto_update::parse_moment("2026-11-20T10:00:00Z").unwrap())
        .offer
        .expect("a release on offer")
}

fn job<'a>(
    c: &'a Case,
    net: &'a dyn Net,
    start: &'a dyn Fn(&Path) -> io::Result<Box<dyn Started>>,
    game: &'a dyn Fn() -> bool,
) -> Job<'a> {
    Job {
        paths: &c.paths,
        download_net: net,
        own_version: "5.0.0",
        appimage: Some(&c.text),
        program: &c.appimage,
        temp: c.dir.path(),
        now: auto_update::parse_moment("2026-11-20T10:00:00Z").unwrap(),
        automatic: false,
        show_window: false,
        game_running: game,
        start,
        alive_wait: Duration::from_millis(300),
    }
}

fn lives(_: &Path) -> io::Result<Box<dyn Started>> {
    Ok(Box::new(Ends(u32::MAX)))
}

fn never(_: &Path) -> io::Result<Box<dyn Started>> {
    panic!("nothing is started")
}

fn no_game() -> bool {
    false
}

fn run_quiet(job: &Job<'_>, release: &Release) -> (Done, Vec<Event>) {
    let mut events = Vec::new();
    let done = run(job, release, &mut |e| events.push(e));
    (done, events)
}

fn lock_free(c: &Case) -> bool {
    !update_run::lock_held(&c.paths, updater::now())
}

#[test]
fn the_percentage_rises_without_repeating_and_ends_at_a_hundred() {
    let c = case("flow-prozent");
    let net = Pieces(github(b"a file of some length that comes in pieces"));
    let release = release_of(&net, &c.paths);
    let (done, events) = run_quiet(&job(&c, &net, &lives, &no_game), &release);
    assert_eq!(done, Done::Quit);
    let percents: Vec<u8> =
        events.iter().filter_map(|e| if let Event::Progress(p) = e { Some(*p) } else { None }).collect();
    assert!(percents.len() > 3, "{percents:?}");
    assert!(percents.windows(2).all(|w| w[0] < w[1]), "rising, no value twice: {percents:?}");
    assert_eq!(percents.last(), Some(&100));
    assert_eq!(fs::read(&c.appimage).unwrap(), b"a file of some length that comes in pieces");
}

#[test]
fn a_display_that_fails_does_not_end_the_download() {
    let c = case("flow-anzeige");
    let net = Pieces(github(b"a file of some length that comes in pieces"));
    let release = release_of(&net, &c.paths);
    let mut asked = 0;
    let done = run(&job(&c, &net, &lives, &no_game), &release, &mut |event| {
        if matches!(event, Event::Progress(_)) {
            asked += 1;
            panic!("the display broke");
        }
    });
    assert_eq!(done, Done::Quit, "installed all the same");
    assert_eq!(asked, 1, "not asked a second time");
}

#[test]
fn the_lock_is_taken_before_the_download_and_a_second_run_waits() {
    let c = case("flow-sperre");
    let net = github(b"neu");
    let release = release_of(&net, &c.paths);
    let now = auto_update::parse_moment("2026-11-20T10:00:00Z").unwrap();
    assert!(update_run::take_lock(&c.paths, std::process::id(), now), "somebody else is updating");
    let asked = net.asked().len();
    let (done, _) = run_quiet(&job(&c, &net, &never, &no_game), &release);
    assert_eq!(done, Done::Stopped(Some(t("up_laeuft_schon"))));
    assert_eq!(net.asked().len(), asked, "nothing fetched");
    assert_eq!(fs::read(&c.appimage).unwrap(), b"alt");
}

#[test]
fn the_lock_is_free_again_after_every_way_out_but_the_hand_over() {
    // Installed.
    let c = case("flow-frei-ok");
    let net = github(b"neu");
    let release = release_of(&net, &c.paths);
    assert_eq!(run_quiet(&job(&c, &net, &lives, &no_game), &release).0, Done::Quit);
    assert!(lock_free(&c), "after the exchange");
    // A file that is not what the checksum says.
    let c = case("flow-frei-falsch");
    let mut bad = github(b"neu");
    for (url, body) in &mut bad.bodies {
        if url.ends_with(".AppImage") {
            *body = b"untergeschoben".to_vec();
        }
    }
    let release = release_of(&bad, &c.paths);
    let (done, _) = run_quiet(&job(&c, &bad, &never, &no_game), &release);
    assert!(matches!(done, Done::Stopped(Some(_))));
    assert!(lock_free(&c), "after a wrong checksum");
    // The game started while the file came in (automatic update only).
    let c = case("flow-frei-spiel");
    let net = github(b"neu");
    let release = release_of(&net, &c.paths);
    let mut automatic = job(&c, &net, &never, &|| true);
    automatic.automatic = true;
    assert_eq!(run_quiet(&automatic, &release).0, Done::GameStarted);
    assert!(lock_free(&c), "after the game started");
    assert_eq!(fs::read(&c.appimage).unwrap(), b"alt", "nothing exchanged");
    // A click installs even with the game running (the question said so).
    let c = case("flow-klick-im-spiel");
    let release = release_of(&net, &c.paths);
    assert_eq!(run_quiet(&job(&c, &net, &lives, &|| true), &release).0, Done::Quit);
}

#[test]
fn a_build_of_the_source_says_git_pull_and_a_release_without_files_is_still_being_built() {
    let c = case("flow-quellcode");
    let net = github(b"neu");
    let release = release_of(&net, &c.paths);
    let mut source = job(&c, &net, &never, &no_game);
    source.own_version = "5.0.0-dev";
    source.appimage = None;
    let asked = net.asked().len();
    assert_eq!(run_quiet(&source, &release).0, Done::Stopped(Some(t("update_quellcode"))));
    assert_eq!(net.asked().len(), asked, "nothing fetched");
    assert!(lock_free(&c));
    // No file at all: still being built. Files, but none that fits: by hand.
    let none = Release { assets: Vec::new(), ..release.clone() };
    assert_eq!(run_quiet(&job(&c, &net, &never, &no_game), &none).0, Done::Stopped(Some(t("s_ub_wird_gebaut"))));
    let other = Release {
        assets: vec![updater::Asset { name: "notes.txt".into(), url: format!("{BASE}/notes.txt"), size: None }],
        ..release
    };
    assert_eq!(run_quiet(&job(&c, &net, &never, &no_game), &other).0, Done::Stopped(Some(t("selbst_holen"))));
    assert_ne!(t("s_ub_wird_gebaut"), t("selbst_holen"));
}

#[test]
fn each_checksum_problem_has_its_own_sentence_and_no_fragment_stays() {
    let mut sentences = Vec::new();
    for (name, key) in [
        ("flow-summe-fehlt", "up_keine_summen"),
        ("flow-summe-netz", "up_summen_netz"),
        ("flow-summe-fremd", "up_summen_fremd"),
        ("flow-summe-falsch", "up_summe_falsch"),
    ] {
        let c = case(name);
        let good = github(b"neu");
        let release = release_of(&good, &c.paths);
        let mut net = github(b"neu");
        let mut changed = release.clone();
        match key {
            "up_keine_summen" => changed.assets.retain(|a| a.name != updater::CHECKSUM_FILE),
            "up_summen_netz" => net.bodies.retain(|(url, _)| !url.ends_with("SHA256SUMS.txt")),
            "up_summen_fremd" => {
                for asset in &mut changed.assets {
                    if asset.name == updater::CHECKSUM_FILE {
                        asset.url = "https://example.org/SHA256SUMS.txt".into();
                    }
                }
            }
            _ => {
                for (url, body) in &mut net.bodies {
                    if url.ends_with(".AppImage") {
                        *body = b"untergeschoben".to_vec();
                    }
                }
            }
        }
        let (done, _) = run_quiet(&job(&c, &net, &never, &no_game), &changed);
        let Done::Stopped(Some(text)) = done else { panic!("{key}: {done:?}") };
        assert!(text.contains(&t(key)), "{key}: {text}");
        assert!(text.starts_with(&tf("update_fehler", &[("0", Arg::from(String::new()))])), "wrapped: {text}");
        assert_eq!(fs::read(&c.appimage).unwrap(), b"alt", "{key}: the old file stays");
        assert!(!c.dir.path().join(format!(".{IMAGE}.neu")).exists(), "{key}: no fragment");
        assert!(lock_free(&c), "{key}");
        sentences.push(text);
    }
    sentences.sort();
    sentences.dedup();
    assert_eq!(sentences.len(), 4, "four different sentences");
}

#[test]
fn a_new_appimage_that_dies_goes_back_and_is_remembered_as_failed() {
    let c = case("flow-zurueck");
    let net = github(b"kaputt");
    let release = release_of(&net, &c.paths);
    let dies = |_: &Path| -> io::Result<Box<dyn Started>> { Ok(Box::new(Ends(1))) };
    let j = job(&c, &net, &dies, &no_game);
    let (done, _) = run_quiet(&j, &release);
    assert_eq!(done, Done::Stopped(Some(t("up_zurueckgerollt"))));
    assert_eq!(fs::read(&c.appimage).unwrap(), b"alt", "the old version is back");
    assert!(update_run::failed_recently(&c.paths, &release.version, j.now + 60.0));
    assert!(lock_free(&c));
}

fn env_for(c: &Case, net: Arc<dyn Net>, start: Arc<StartFn>) -> Env {
    Env {
        net: net.clone(),
        download_net: net,
        own_version: "5.0.0".into(),
        appimage: Some(c.text.clone()),
        program: c.appimage.clone(),
        temp: c.dir.path().to_path_buf(),
        start,
        game_running: Arc::new(|| false),
        alive_wait: Duration::from_millis(300),
    }
}

fn wait_until_idle(runner: &Runner) -> State {
    let end = std::time::Instant::now() + Duration::from_secs(20);
    while runner.is_busy() && std::time::Instant::now() < end {
        std::thread::sleep(Duration::from_millis(5));
    }
    runner.state()
}

#[test]
fn the_runner_fetches_in_its_own_thread_and_asks_the_program_to_end() {
    let c = case("flow-runner");
    let env = env_for(&c, Arc::new(github(b"neu")), Arc::new(lives));
    let runner = Runner::default();
    assert_eq!(runner.state(), State::Idle);
    assert!(runner.start(&env, c.paths.host.clone(), false));
    assert_eq!(wait_until_idle(&runner), State::Quit);
    assert_eq!(fs::read(&c.appimage).unwrap(), b"neu");
}

/// A net whose list of releases waits until the test lets it go.
struct Held {
    inner: Fake,
    gate: Mutex<std::sync::mpsc::Receiver<()>>,
}

impl Net for Held {
    fn get(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        if url == updater::API {
            let _ = self.gate.lock().unwrap().recv_timeout(Duration::from_secs(20));
        }
        self.inner.get(url)
    }

    fn head(&self, url: &str) -> Result<crate::net::Headers, FetchError> {
        self.inner.head(url)
    }
}

#[test]
fn a_second_click_while_one_fetch_is_under_way_starts_nothing() {
    let c = case("flow-runner-zwei");
    let (open, gate) = std::sync::mpsc::channel();
    let net: Arc<dyn Net> = Arc::new(Held { inner: github(b"neu"), gate: Mutex::new(gate) });
    let env = env_for(&c, net, Arc::new(lives));
    let runner = Runner::default();
    assert!(runner.start(&env, c.paths.host.clone(), false));
    assert!(runner.is_busy());
    assert!(!runner.start(&env, c.paths.host.clone(), false), "one fetch at a time");
    assert!(runner.is_busy(), "the first goes on");
    open.send(()).unwrap();
    assert_eq!(wait_until_idle(&runner), State::Quit);
}

#[test]
fn the_runner_says_when_no_version_is_known_and_a_test_version_needs_its_channel() {
    let c = case("flow-runner-keine");
    let empty: Arc<dyn Net> = Arc::new(Fake::default().with(updater::API, b"[]".to_vec()));
    let env = env_for(&c, empty, Arc::new(lives));
    let runner = Runner::default();
    runner.start(&env, c.paths.host.clone(), true);
    assert_eq!(wait_until_idle(&runner), State::Idle);
    assert_eq!(runner.take_said(), [t("s_ub_keine_bekannt")]);
    assert!(runner.take_said().is_empty(), "said once");
    // The only release is a test version: the stable channel finds none, the test channel takes it.
    let c = case("flow-runner-vorab");
    let body = api("2026-11-20T09:00:00Z", &image_asset()).replace("\"prerelease\":false", "\"prerelease\":true");
    let net = github(b"neu");
    let net: Arc<dyn Net> = Arc::new(Fake {
        bodies: net
            .bodies
            .into_iter()
            .map(|(u, b)| if u == updater::API { (u, body.clone().into_bytes()) } else { (u, b) })
            .collect(),
        ..Fake::default()
    });
    let env = env_for(&c, net, Arc::new(lives));
    let runner = Runner::default();
    runner.start(&env, c.paths.host.clone(), false);
    assert_eq!(wait_until_idle(&runner), State::Idle);
    assert_eq!(runner.take_said(), [t("s_ub_keine_bekannt")]);
    runner.start(&env, c.paths.host.clone(), true);
    assert_eq!(wait_until_idle(&runner), State::Quit);
}
