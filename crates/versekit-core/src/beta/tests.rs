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
use crate::paths::Host;
use crate::pypath::Flavor;
use crate::testdir::Dir;

const TAG: &str = "beta-v5.7.0-beta.1";
const BASE: &str = "https://github.com/Xharig/VerseKit/releases/download";
const SETUP: &str = "VerseKit-Setup.exe";
const PLAIN: &[u8] = b"MZ the installer of a beta, and some more bytes for the content";

fn code(text: &str) -> Code {
    Code::parse(text).expect("a code")
}

/// Three testers; the second and the third share no first four characters with the first.
fn testers() -> Vec<Code> {
    vec![code("QX4M-K7QM-2XRA-9FHD-W3TB"), code("A1B2-C3D4-E5F6-G7H8-J9K0"), code("ZZZZ-0000-1111-2222-3333")]
}

fn paths_in(name: &str) -> (Dir, Paths) {
    let folder = Dir::new(name);
    let text = folder.path().to_string_lossy().into_owned();
    let vars = vec![(crate::envvars::ENV_HOME, text.as_str())];
    (folder, Paths::new(Host::new(Flavor::native(), &vars, &text)))
}

fn url(name: &str) -> String {
    format!("{BASE}/{TAG}/{name}")
}

fn asset(name: &str) -> Asset {
    Asset { name: name.into(), url: url(name), size: None }
}

/// A beta release as GitHub holds it: the key ring for `codes`, the encrypted installer, the checksums. `bend`
/// changes the encrypted file before it is published (the checksums then name the changed file).
struct Built {
    release: Release,
    net: Fake,
    encrypted: Vec<u8>,
}

fn built(codes: &[Code], bend: impl FnOnce(&mut Vec<u8>)) -> Built {
    let key = RoundKey::generate().unwrap();
    let ring = Keyring::seal(TAG, Cost::LOWEST, codes, &key).unwrap().to_json();
    let mut encrypted = seal_file(&key, TAG, SETUP, PLAIN).unwrap();
    bend(&mut encrypted);
    let sums = format!(
        "{}  {KEYRING_FILE}\n{}  {SETUP}{SUFFIX}\n",
        sha256(ring.as_bytes()),
        sha256(&encrypted)
    );
    let release = Release {
        version: TAG.into(),
        prerelease: true,
        assets: vec![asset(&format!("{SETUP}{SUFFIX}")), asset(KEYRING_FILE), asset(updater::CHECKSUM_FILE)],
        ..Release::default()
    };
    let net = Fake::default()
        .with(&url(KEYRING_FILE), ring.into_bytes())
        .with(&url(&format!("{SETUP}{SUFFIX}")), encrypted.clone())
        .with(&url(updater::CHECKSUM_FILE), sums.into_bytes());
    Built { release, net, encrypted }
}

/// What lies in the folder beside the target, by name.
fn left_in(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder)
        .map(|list| list.filter_map(Result::ok).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    names.sort();
    names
}

fn fetch(built: &Built, code: &Code, name: &str) -> (Dir, PathBuf, Result<Downloaded, UpdateError>) {
    let folder = Dir::new(name);
    let target = folder.path().join(SETUP);
    let asset = matching_asset(&built.release, Packaging::Exe).expect("the encrypted installer").clone();
    let result = download(&built.net, &built.release, &asset, code, &target, &mut |_| {});
    (folder, target, result)
}

// ------------------------------------------------------------ the code

#[test]
fn a_code_is_read_however_it_is_typed_and_never_printed() {
    let plain = code("QX4M-K7QM-2XRA-9FHD-W3TB");
    for typed in ["qx4m k7qm 2xra 9fhd w3tb", "QX4MK7QM2XRA9FHDW3TB", " qx4m-k7qm-2xra-9fhd-w3tb "] {
        assert_eq!(Code::parse(typed), Some(plain.clone()), "{typed}");
    }
    // O is 0, I and L are 1.
    assert_eq!(Code::parse("OOOO-IIII-LLLL-0000-1111"), Some(code("0000-1111-1111-0000-1111")));
    for bad in ["", "QX4M-K7QM-2XRA-9FHD", "QX4M-K7QM-2XRA-9FHD-W3TB-A", "QX4M-K7QM-2XRA-9FHD-W3TU", "QX4M-K7QM-2XRA-9FHD-W3Tä"] {
        assert_eq!(Code::parse(bad), None, "{bad}");
    }
    assert_eq!(plain.id(), "QX4M");
    assert_eq!(plain.shown(), "QX4M-K7QM-2XRA-9FHD-W3TB");
    assert_eq!(plain.masked(), "QX4M-••••-••••-••••-••••");
    let printed = format!("{plain:?} {:?}", RoundKey::generate().unwrap());
    assert!(!printed.contains("K7QM") && printed.contains("Code(…)"), "{printed}");
    let made = Code::generate().unwrap();
    assert_eq!(Code::parse(&made.shown()), Some(made));
}

// ------------------------------------------------------------ the key ring

#[test]
fn every_tester_opens_the_key_ring_with_his_own_code_and_nobody_else_does() {
    let key = RoundKey::generate().unwrap();
    let codes = testers();
    let ring = Keyring::seal(TAG, Cost::LOWEST, &codes, &key).unwrap();
    let ring = Keyring::parse(ring.to_json().as_bytes()).unwrap();
    assert_eq!(ring.entries.len(), 3);
    for code in &codes {
        assert_eq!(ring.unlock(TAG, code), Ok(key.clone()));
    }
    // The first four characters of a tester, the rest wrong: the entry is found, the code does not open it.
    assert_eq!(ring.unlock(TAG, &code("QX4M-K7QM-2XRA-9FHD-W3TC")), Err(Problem::WrongCode));
    // A code without an entry: not in this round.
    assert_eq!(ring.unlock(TAG, &code("NEW1-K7QM-2XRA-9FHD-W3TB")), Err(Problem::NotInRound));
    // The key ring of another release.
    assert_eq!(ring.unlock("beta-v5.7.0-beta.2", &codes[0]), Err(Problem::Foreign));
    // ⚠ No name, no code in what is published.
    let json = Keyring::seal(TAG, Cost::LOWEST, &codes, &key).unwrap().to_json();
    assert!(!json.contains("K7QM") && !json.contains("C3D4"), "{json}");
}

#[test]
fn a_changed_or_odd_key_ring_opens_nothing() {
    let key = RoundKey::generate().unwrap();
    let codes = testers();
    let json = Keyring::seal(TAG, Cost::LOWEST, &codes, &key).unwrap().to_json();
    let mut value: Json = serde_json::from_str(&json).unwrap();
    // One wrapped byte changed: that tester's code no longer opens it.
    let wrapped = value["eintraege"][0]["schluessel"].as_str().unwrap().to_string();
    let flipped = format!("{}{}", if wrapped.starts_with('0') { "1" } else { "0" }, &wrapped[1..]);
    value["eintraege"][0]["schluessel"] = Json::from(flipped);
    let ring = Keyring::parse(value.to_string().as_bytes()).unwrap();
    assert_eq!(ring.unlock(TAG, &codes[0]), Err(Problem::WrongCode));
    assert_eq!(ring.unlock(TAG, &codes[1]), Ok(key.clone()));
    // An entry moved to another tag: its binding to the release breaks.
    let mut moved: Json = serde_json::from_str(&json).unwrap();
    moved["tag"] = Json::from("beta-v5.7.0-beta.9");
    assert_eq!(Keyring::parse(moved.to_string().as_bytes()).unwrap().unlock("beta-v5.7.0-beta.9", &codes[0]), Err(Problem::WrongCode));
    // A cost out of bounds, a cut-off file, something else: broken, never a hang.
    for (field, number) in [("speicher_kib", 4 * 1024 * 1024), ("durchlaeufe", 1000), ("spuren", 0)] {
        let mut costly: Json = serde_json::from_str(&json).unwrap();
        costly["argon2id"][field] = Json::from(number);
        assert_eq!(Keyring::parse(costly.to_string().as_bytes()), Err(Problem::Damaged), "{field}");
    }
    assert_eq!(Keyring::parse(&json.as_bytes()[..json.len() / 2]), Err(Problem::Damaged));
    assert_eq!(Keyring::parse(b"[]"), Err(Problem::Damaged));
    // Two testers whose codes start alike cannot share a key ring: one id per entry.
    let twins = [code("QX4M-0000-0000-0000-0000"), code("QX4M-1111-1111-1111-1111")];
    assert!(Keyring::seal(TAG, Cost::LOWEST, &twins, &key).is_err());
}

// ------------------------------------------------------------ the files

#[test]
fn a_file_opens_only_whole_with_its_key_its_release_and_its_name() {
    let key = RoundKey::generate().unwrap();
    let sealed = seal_file(&key, TAG, SETUP, PLAIN).unwrap();
    let (plain, checksum) = open_file(&key, TAG, SETUP, sealed.clone()).unwrap();
    assert_eq!((plain.as_slice(), checksum), (PLAIN, sha256(PLAIN)));
    assert!(!sealed.windows(PLAIN.len().min(16)).any(|w| w == &PLAIN[..16]), "the content is not readable");
    assert_eq!(open_file(&RoundKey::generate().unwrap(), TAG, SETUP, sealed.clone()), Err(Problem::Damaged));
    assert_eq!(open_file(&key, "beta-v5.7.0-beta.2", SETUP, sealed.clone()), Err(Problem::Foreign));
    assert_eq!(open_file(&key, TAG, "VerseKit-x86_64.AppImage", sealed.clone()), Err(Problem::Foreign));
    // Every single byte of the file matters, the head included.
    for at in [0, 9, 20, sealed.len() / 2, sealed.len() - 1] {
        let mut bent = sealed.clone();
        bent[at] ^= 0x01;
        assert!(open_file(&key, TAG, SETUP, bent).is_err(), "byte {at}");
    }
    // Cut off anywhere: broken.
    for keep in [0, 5, 12, sealed.len() - 17, sealed.len() - 1] {
        assert_eq!(open_file(&key, TAG, SETUP, sealed[..keep].to_vec()), Err(Problem::Damaged), "cut at {keep}");
    }
    // A head that names another tag and is bound anew would need the key: with a changed head the content fails.
    let text = String::from_utf8_lossy(&sealed).replace(TAG, "beta-v5.7.0-beta.0").into_bytes();
    assert!(open_file(&key, "beta-v5.7.0-beta.0", SETUP, text).is_err());
}

// ------------------------------------------------------------ fetching

#[test]
fn the_right_code_brings_the_plain_installer_and_nothing_else_stays() {
    let codes = testers();
    let built = built(&codes, |_| {});
    let (folder, target, result) = fetch(&built, &codes[1], "beta-holen");
    let downloaded = result.expect("the beta is fetched");
    assert_eq!(downloaded.path, target);
    assert_eq!(fs::read(&target).unwrap(), PLAIN);
    assert_eq!(downloaded.checksum, sha256(PLAIN));
    assert!(updater::still_matches(&target, &downloaded.checksum));
    assert_eq!(left_in(folder.path()), [SETUP], "no encrypted or half file stays");
}

#[test]
fn a_wrong_code_is_told_before_the_file_comes_and_leaves_nothing() {
    let codes = testers();
    let built = built(&codes, |_| {});
    for (wrong, problem) in
        [(code("QX4M-K7QM-2XRA-9FHD-W3TC"), Problem::WrongCode), (code("NEW1-K7QM-2XRA-9FHD-W3TB"), Problem::NotInRound)]
    {
        let (folder, _, result) = fetch(&built, &wrong, "beta-falsch");
        assert_eq!(result, Err(UpdateError::Beta(problem)));
        assert!(left_in(folder.path()).is_empty(), "nothing stays: {:?}", left_in(folder.path()));
        assert!(
            !built.net.asked().iter().any(|u| u.ends_with(SUFFIX)),
            "the installer is not fetched for a code that does not fit: {:?}",
            built.net.asked()
        );
    }
}

#[test]
fn a_changed_or_cut_file_is_never_installed_even_with_a_fitting_checksum_list() {
    let codes = testers();
    #[allow(clippy::ptr_arg)] // one kind of function with `cut`, which needs the Vec
    fn flip(bytes: &mut Vec<u8>) {
        let at = bytes.len() / 2;
        bytes[at] ^= 0x40;
    }
    fn cut(bytes: &mut Vec<u8>) {
        bytes.truncate(bytes.len() - 20);
    }
    for (i, bend) in [flip as fn(&mut Vec<u8>), cut].into_iter().enumerate() {
        let built = built(&codes, bend);
        let (folder, target, result) = fetch(&built, &codes[0], "beta-kaputt");
        assert_eq!(result, Err(UpdateError::Beta(Problem::Damaged)), "case {i}");
        assert!(!target.exists());
        assert!(left_in(folder.path()).is_empty(), "case {i}: {:?}", left_in(folder.path()));
    }
}

#[test]
fn a_file_that_does_not_match_the_published_checksum_is_thrown_away() {
    let codes = testers();
    let mut built = built(&codes, |_| {});
    let mut other = built.encrypted.clone();
    other.truncate(other.len() - 1);
    let name = url(&format!("{SETUP}{SUFFIX}"));
    built.net.bodies.retain(|(u, _)| *u != name);
    built.net.bodies.push((name, other));
    let (folder, _, result) = fetch(&built, &codes[0], "beta-summe");
    assert_eq!(result, Err(UpdateError::ChecksumWrong));
    assert!(left_in(folder.path()).is_empty());
}

#[test]
fn only_encrypted_files_of_this_project_are_taken() {
    assert_eq!(plain_name("VerseKit-Setup.exe.vkbeta").as_deref(), Some("VerseKit-Setup.exe"));
    assert_eq!(plain_name("VerseKit-x86_64.AppImage.VKBETA").as_deref(), Some("VerseKit-x86_64.AppImage"));
    for bad in ["VerseKit-Setup.exe", "../VerseKit-Setup.exe.vkbeta", "x.txt.vkbeta", ".vkbeta", "a/b.exe.vkbeta"] {
        assert_eq!(plain_name(bad), None, "{bad}");
    }
    let mut release = built(&testers(), |_| {}).release;
    release.assets.push(asset("VerseKit-x86_64.AppImage.vkbeta"));
    assert_eq!(matching_asset(&release, Packaging::Exe).map(|a| a.name.as_str()), Some("VerseKit-Setup.exe.vkbeta"));
    assert_eq!(matching_asset(&release, Packaging::AppImage).map(|a| a.name.as_str()), Some("VerseKit-x86_64.AppImage.vkbeta"));
    assert_eq!(matching_asset(&release, Packaging::Source), None);
    // ⚠ The old way never takes an encrypted file for an installer.
    assert_eq!(updater::matching_asset(&release, Packaging::Exe), None);
    assert_eq!(updater::matching_asset(&release, Packaging::AppImage), None);
}

// ------------------------------------------------------------ who is offered a beta

/// The answer of the GitHub API: a finished version, a test version and a beta.
fn api_answer(beta: &Release) -> Vec<u8> {
    let assets: Vec<Json> =
        beta.assets.iter().map(|a| json!({"name": a.name, "browser_download_url": a.url, "size": 1})).collect();
    json!([
        {"tag_name": TAG, "name": "Beta", "prerelease": true, "draft": false, "published_at": "2026-11-24T10:00:00Z", "body": "", "assets": assets},
        {"tag_name": "v5.6.1", "name": "5.6.1", "prerelease": false, "draft": false, "published_at": "2026-11-23T10:00:00Z", "body": "", "assets": []},
        {"tag_name": "v5.6.2-rc1", "name": "rc", "prerelease": true, "draft": false, "published_at": "2026-11-23T12:00:00Z", "body": "", "assets": []},
    ])
    .to_string()
    .into_bytes()
}

#[test]
fn the_old_ways_never_see_a_beta() {
    let built = built(&testers(), |_| {});
    let answer = api_answer(&built.release);
    let releases = updater::releases_from_api(&answer).unwrap();
    assert!(releases.iter().all(|r| !is_beta_tag(&r.version)), "{releases:?}");
    assert!(!updater::is_app_tag(TAG));
    let mut all = releases.clone();
    all.push(built.release.clone());
    for with_prerelease in [false, true] {
        assert!(updater::best_newer(&all, "5.6.1", with_prerelease).is_none_or(|r| !is_beta_tag(&r.version)));
        assert!(updater::latest(&all, with_prerelease).is_none_or(|r| !is_beta_tag(&r.version)));
    }
    assert_eq!(releases_from_api(&answer).unwrap().iter().map(|r| r.version.as_str()).collect::<Vec<_>>(), [TAG]);
}

/// Runs a look at GitHub for a program at 5.6.1 on `stage` with `stored`, and says what is offered and what was
/// asked.
fn look(name: &str, stage: Stage, stored: Option<&Code>) -> (Dir, Paths, Option<String>, Vec<String>) {
    let codes = testers();
    let built = built(&codes, |_| {});
    let net = built.net.with(updater::API, api_answer(&built.release));
    let (dir, paths) = paths_in(name);
    assert!(choose(&paths, stage));
    if let Some(code) = stored {
        assert!(save_code(&paths, code));
    }
    let checked = updater::check(&net, &paths, "5.6.1", true, 1000.0);
    let offered = checked.offer.map(|r| r.version);
    assert_eq!(updater::known_newer(&paths, "5.6.1").map(|r| r.version), offered, "the same choice without the net");
    (dir, paths, offered, net.asked())
}

#[test]
fn stable_and_test_are_never_offered_a_beta_and_never_touch_it() {
    let codes = testers();
    for (stage, expected) in [(Stage::Stable, None), (Stage::Test, Some("v5.6.2-rc1"))] {
        // A code in the settings (left from an earlier time) changes nothing on these stages.
        let (_dir, paths, offered, asked) = look("beta-stufe-aus", stage, Some(&codes[0]));
        assert_eq!(offered.as_deref(), expected, "{stage:?}");
        assert_eq!(asked, [updater::API], "{stage:?}: only the list is asked");
        assert!(!Path::new(&paths.app_file(CACHE)).exists(), "{stage:?}: no beta is remembered");
        assert!(!Path::new(&paths.app_file(STATE_FILE)).exists());
        assert_eq!(status(&paths), Status::Off);
    }
}

#[test]
fn a_beta_tester_is_offered_the_beta_only_with_a_code_that_opens_it() {
    let codes = testers();
    let (_dir, paths, offered, asked) = look("beta-passt", Stage::Beta, Some(&codes[2]));
    assert_eq!(offered.as_deref(), Some(TAG));
    assert_eq!(status(&paths), Status::Fits(TAG.into()));
    assert!(asked.contains(&url(KEYRING_FILE)) && !asked.iter().any(|u| u.ends_with(SUFFIX)), "{asked:?}");
    // The code is nowhere but in the settings: not in the remembered lists, not in the state.
    for file in [CACHE, STATE_FILE, updater::CACHE] {
        let text = fs::read_to_string(paths.app_file(file)).unwrap_or_default();
        assert!(!text.contains("0000-1111") && !text.contains("ZZZZ00001111"), "{file}: {text}");
    }
    // Without a code, and with codes that do not open it: the test version, never the beta.
    for (stored, problem) in [
        (None, None),
        (Some(code("ZZZZ-0000-1111-2222-3334")), Some(Problem::WrongCode)),
        (Some(code("OLD1-0000-1111-2222-3333")), Some(Problem::NotInRound)),
    ] {
        let (_dir, paths, offered, _) = look("beta-passt-nicht", Stage::Beta, stored.as_ref());
        assert_eq!(offered.as_deref(), Some("v5.6.2-rc1"), "{problem:?}");
        assert_eq!(status(&paths), problem.map_or(Status::NoCode, Status::Problem));
    }
}

#[test]
fn a_beta_whose_files_are_not_all_there_yet_is_asked_again_and_not_remembered_as_damaged() {
    let codes = testers();
    let built = built(&codes, |_| {});
    let mut early = built.release.clone();
    early.assets.retain(|a| a.name != KEYRING_FILE);
    let (_dir, paths) = paths_in("beta-unfertig");
    assert!(choose(&paths, Stage::Beta));
    assert!(save_code(&paths, &codes[1]));
    let net = built.net.with(updater::API, api_answer(&early));
    updater::check(&net, &paths, "5.6.1", true, 1000.0);
    assert_eq!(status(&paths), Status::Unchecked, "the release is still being filled: nothing is remembered");
    // The files are all there now: the next look opens it.
    let full = self::built(&codes, |_| {});
    let net = full.net.with(updater::API, api_answer(&full.release));
    let checked = updater::check(&net, &paths, "5.6.1", true, 2000.0);
    assert_eq!(status(&paths), Status::Fits(TAG.into()));
    assert_eq!(checked.offer.map(|r| r.version).as_deref(), Some(TAG));
}

/// A click on the update while the remembered list still holds a beta whose installer was not uploaded yet (the
/// files of a release come one after the other): the click asks GitHub again and finds the installer, instead of
/// sending the player to fetch it by hand.
#[test]
#[cfg(windows)]
fn a_click_on_a_beta_remembered_before_its_installer_was_up_asks_again_and_finds_it() {
    use crate::update_flow::{self, Done, Env, Fetch};
    let codes = testers();
    let full = built(&codes, |_| {});
    let (dir, paths) = paths_in("beta-halb-hochgeladen");
    assert!(choose(&paths, Stage::Beta));
    assert!(save_code(&paths, &codes[0]));
    // A look in the seconds when GitHub had the key ring but not yet the installer of Windows.
    let mut early = full.release.clone();
    early.assets.retain(|a| !a.name.ends_with(SUFFIX));
    let now = updater::now();
    let github = |release: &Release| Fake { bodies: full.net.bodies.clone(), ..Fake::default() }.with(updater::API, api_answer(release));
    updater::check(&github(&early), &paths, "5.6.1", true, now);
    assert_eq!(status(&paths), Status::Fits(TAG.into()), "the key ring opened: the bar offers the beta");
    // The click soon after: the list is fresh, GitHub has every file now. The download is refused here (nothing
    // is installed or started); what counts is that the installer was found.
    let env = Env {
        net: std::sync::Arc::new(github(&full.release)),
        download_net: std::sync::Arc::new(Fake::default()),
        own_version: "5.6.1".into(),
        appimage: None,
        program: dir.path().join("VerseKit.exe"),
        temp: dir.path().to_path_buf(),
        start: std::sync::Arc::new(|_: &std::path::Path| Err(std::io::Error::other("nothing starts"))),
        game_running: std::sync::Arc::new(|| false),
        alive_wait: std::time::Duration::from_millis(10),
    };
    let fetch = Fetch { with_prerelease: true, beta: true, without_backup: false };
    let done = update_flow::fetch_newest(&env, &paths, fetch, &mut |_| {}, &|v: &str| assert_eq!(v, TAG));
    assert_ne!(done, Done::Stopped(Some(crate::language::t("selbst_holen"))), "never „fetch it yourself“ for a beta");
    assert!(matches!(done, Done::Stopped(Some(_))), "the download was tried and refused here: {done:?}");
    assert!(cache_read(&paths)[0].assets.iter().any(|a| a.name.ends_with(SUFFIX)), "the remembered beta has its installer now");
}

#[test]
fn leaving_the_stage_deletes_the_code_and_what_it_opened() {
    let codes = testers();
    let (_dir, paths, offered, _) = look("beta-verlassen", Stage::Beta, Some(&codes[0]));
    assert_eq!(offered.as_deref(), Some(TAG));
    assert!(choose(&paths, Stage::Stable));
    assert_eq!(stored_code(&paths), None);
    assert!(!Path::new(&paths.app_file(STATE_FILE)).exists());
    assert_eq!(Stage::of(&paths), Stage::Stable);
    assert_eq!(updater::known_newer(&paths, "5.6.1"), None);
    // Back on the stage, the old code is gone: nothing opens before a code is given again.
    assert!(choose(&paths, Stage::Beta));
    assert_eq!(status(&paths), Status::NoCode);
    assert_eq!(updater::known_newer(&paths, "5.6.1").map(|r| r.version).as_deref(), Some("v5.6.2-rc1"));
}

#[test]
fn the_key_of_a_round_comes_from_the_secret_and_the_tag_alone() {
    let secret = [7u8; 32];
    let a = RoundKey::derive(&secret, TAG).unwrap();
    assert_eq!(a, RoundKey::derive(&secret, TAG).unwrap(), "the same secret and tag: the same key");
    assert_ne!(a, RoundKey::derive(&secret, "beta-v5.7.0-beta.2").unwrap(), "another tag: another key");
    assert_ne!(a, RoundKey::derive(&[8u8; 32], TAG).unwrap(), "another secret: another key");
    assert!(RoundKey::derive(&[7u8; 31], TAG).is_err(), "a short secret is refused");
    assert!(RoundKey::derive(&secret, "v5.7.0").is_err(), "no beta tag");
    // A file sealed in one job opens with the key a second job draws, and with no other.
    let sealed = seal_file(&a, TAG, SETUP, PLAIN).unwrap();
    let again = RoundKey::derive(&secret, TAG).unwrap();
    assert_eq!(open_file(&again, TAG, SETUP, sealed.clone()).unwrap().0, PLAIN);
    let other = RoundKey::derive(&secret, "beta-v5.7.0-beta.2").unwrap();
    assert_eq!(open_file(&other, TAG, SETUP, sealed), Err(Problem::Damaged));
}

#[test]
fn a_key_ring_of_a_derived_key_is_opened_by_every_code_of_the_list_reserve_codes_included() {
    let key = RoundKey::derive(&[9u8; 32], TAG).unwrap();
    let mut codes = testers();
    codes.extend((0..20).map(|n| code(&format!("R{n:03}-K7QM-2XRA-9FHD-W3TB"))));
    let ring = Keyring::parse(Keyring::seal(TAG, Cost::LOWEST, &codes, &key).unwrap().to_json().as_bytes()).unwrap();
    assert_eq!(ring.entries.len(), 23);
    for code in &codes {
        assert_eq!(ring.unlock(TAG, code), Ok(key.clone()));
    }
}

#[test]
fn the_error_report_never_shows_the_code() {
    let (_dir, paths) = paths_in("beta-bericht");
    assert!(choose(&paths, Stage::Beta));
    assert!(save_code(&paths, &code("QX4M-K7QM-2XRA-9FHD-W3TB")));
    for lang in [crate::language::Lang::De, crate::language::Lang::En] {
        let line = crate::report_facts::settings_line(&paths, lang).unwrap();
        assert!(line.contains("betatester=True") || line.contains("betatester=true"), "{line}");
        assert!(!line.contains("K7QM") && !line.contains(SETTING_CODE), "{line}");
    }
}
