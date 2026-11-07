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

const BASE: &str = "https://github.com/Xharig/VerseKit/releases/download";

/// A throwaway data folder (flat, through the variable), with more variables if wanted.
fn paths_in(name: &str, extra: &[(&str, &str)]) -> (Dir, Paths) {
    let folder = Dir::new(name);
    let text = folder.path().to_string_lossy().into_owned();
    let mut vars = vec![(legacy::ENV_HOME, text.as_str())];
    vars.extend_from_slice(extra);
    (folder, Paths::new(Host::new(Flavor::native(), &vars, &text)))
}

fn release(version: &str, prerelease: bool) -> Release {
    Release { version: version.into(), prerelease, ..Release::default() }
}

fn asset(name: &str) -> Asset {
    Asset { name: name.into(), url: format!("{BASE}/v5.0.1/{name}"), size: None }
}

fn sha(bytes: &[u8]) -> String {
    digest::digest(&digest::SHA256, bytes).as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn only_release_files_of_this_project_on_github_count() {
    for good in [
        format!("{BASE}/v5.0.0/VerseKit-Setup.exe"),
        "https://github.com/Xharig/SC-BP-Watcher/releases/download/v4.12.2/VerseKit-Setup.exe".to_string(),
        "https://GITHUB.com/xharig/versekit/releases/download/v5.0.0-rc1/SHA256SUMS.txt".to_string(),
        "https://github.com/Xharig/VerseKit/releases/v5.0.0/VerseKit-Setup.exe".to_string(),
        "https://github.com:443/Xharig/VerseKit/releases/download/v5.0.0/x.exe".to_string(),
    ] {
        assert!(url_ok(&good), "{good}");
    }
    for bad in [
        "http://github.com/Xharig/VerseKit/releases/download/v5.0.0/VerseKit-Setup.exe",
        "https://github.com.example.org/Xharig/VerseKit/releases/download/v5.0.0/x.exe",
        "https://objects.githubusercontent.com/Xharig/VerseKit/releases/download/v5.0.0/x.exe",
        "https://github.com/Fremd/VerseKit/releases/download/v5.0.0/x.exe",
        "https://github.com/Xharig/VerseKit/releases/download/v5.0.0/../../Fremd/x.exe",
        "https://github.com/Xharig/VerseKit/releases/download/v5.0.0/%2e%2e/x.exe",
        "https://github.com/Xharig/VerseKit/releases/download/v5.0.0/a/x.exe",
        "https://github.com/Xharig/VerseKit/releases/download/v5.0.0/",
        "https://github.com/Xharig/VerseKit/archive/v5.0.0.zip",
        "",
        "github.com/Xharig/VerseKit/releases/download/v5.0.0/x.exe",
    ] {
        assert!(!url_ok(bad), "{bad}");
    }
}

#[test]
fn a_file_name_from_the_server_becomes_a_bare_harmless_one_or_none() {
    assert_eq!(safe_filename("VerseKit-Setup.exe").as_deref(), Some("VerseKit-Setup.exe"));
    assert_eq!(safe_filename("../../autostart/boese.exe").as_deref(), Some("boese.exe"));
    assert_eq!(safe_filename(r"a\b\VerseKit-x86_64.AppImage ").as_deref(), Some("VerseKit-x86_64.AppImage"));
    assert_eq!(safe_filename("x.exe/").as_deref(), Some("x.exe"), "a slash at the end goes first, as in 4.12");
    for bad in ["", ".", "..", "update.bin", "SHA256SUMS.txt", "../", "x.exe/.."] {
        assert_eq!(safe_filename(bad), None, "{bad}");
    }
}

#[test]
fn the_checksum_file_is_read_like_sha256sum_and_a_doubtful_one_is_empty() {
    let a = "a".repeat(64);
    let b = "B".repeat(64);
    let table = parse_checksums(&format!(
        "{a}  VerseKit-Setup.exe\r\n{b} *VerseKit-x86_64.AppImage\n\nkaputt\n{}  zu-kurz.exe\n",
        "c".repeat(63)
    ));
    assert_eq!(table.len(), 2);
    assert_eq!(table["VerseKit-Setup.exe"], a);
    assert_eq!(table["VerseKit-x86_64.AppImage"], "b".repeat(64), "lower case, the star gone");
    assert!(parse_checksums(&format!("{a}  dateien/windows/VerseKit-Setup.exe\n")).is_empty(), "a path");
    assert!(parse_checksums(&format!("{a}  x.exe\n{b}  x.exe\n")).is_empty(), "a name twice");
    assert!(parse_checksums("").is_empty());
}

#[test]
fn the_highest_newer_release_is_offered_and_test_versions_only_when_wanted() {
    let list =
        [release("v5.0.0-rc9", true), release("v5.0.0-rc10", true), release("v4.12.2", false), release("", false)];
    assert_eq!(best_newer(&list, "4.12.2", false), None, "no finished version newer");
    assert_eq!(best_newer(&list, "4.12.2", true).map(|r| r.version.as_str()), Some("v5.0.0-rc10"), "rc10 over rc9");
    let with_final = [release("v5.0.0-rc10", true), release("v5.0.0", false), release("v5.0.1-rc1", true)];
    assert_eq!(best_newer(&with_final, "5.0.0-rc10", false).map(|r| r.version.as_str()), Some("v5.0.0"));
    assert_eq!(best_newer(&with_final, "5.0.0", true).map(|r| r.version.as_str()), Some("v5.0.1-rc1"));
    assert_eq!(best_newer(&with_final, "5.0.1", true), None);
    // The newest of a channel, whatever runs: from a test version back to the finished one.
    assert_eq!(latest(&with_final, false).map(|r| r.version.as_str()), Some("v5.0.0"));
    assert_eq!(latest(&with_final, true).map(|r| r.version.as_str()), Some("v5.0.1-rc1"));
    // A test version without the pre-release mark counts as one by its tag.
    let unmarked = [release("v4.12.2", false), release("v5.0.0-rc3", false)];
    assert_eq!(latest(&unmarked, false).map(|r| r.version.as_str()), Some("v4.12.2"), "stable box without rc");
    assert_eq!(latest(&unmarked, true).map(|r| r.version.as_str()), Some("v5.0.0-rc3"));
    assert_eq!(best_newer(&unmarked, "4.12.2", false), None, "rc not offered to a finished version");
    assert!(
        version_key("v5.0.0-rc13") > version_key("v5.0.0-rc1") && version_key("v5.0.0") > version_key("v5.0.0-rc13")
    );
}

#[test]
fn under_windows_only_the_installer_is_fetched_and_only_from_the_project() {
    let mut r = release("v5.0.1", false);
    r.assets = vec![
        asset("SHA256SUMS.txt"),
        asset("VerseKit-x86_64.AppImage"),
        asset("VerseKit.exe"),
        asset("VerseKit-Setup.exe"),
    ];
    assert_eq!(matching_asset(&r, Packaging::Exe).map(|a| a.name.as_str()), Some("VerseKit-Setup.exe"));
    assert_eq!(matching_asset(&r, Packaging::AppImage).map(|a| a.name.as_str()), Some("VerseKit-x86_64.AppImage"));
    assert_eq!(matching_asset(&r, Packaging::Source), None, "a build of the source is never replaced");
    r.assets = vec![asset("VerseKit.exe")];
    assert_eq!(matching_asset(&r, Packaging::Exe), None, "no installer, no update");
    r.assets = vec![Asset {
        name: "VerseKit-Setup.exe".into(),
        url: "https://example.org/VerseKit-Setup.exe".into(),
        size: None,
    }];
    assert_eq!(matching_asset(&r, Packaging::Exe), None, "not from the project");
}

#[test]
fn a_file_is_ours_by_its_old_or_new_name() {
    assert!(is_ours(r"C:\Programme\Verse-Kit\SC-BP-Watcher.exe"));
    assert!(is_ours("/home/x/Apps/VerseKit-x86_64.AppImage"));
    assert!(is_ours("/home/x/Apps/sc-bp-watcher-x86_64.AppImage"));
    assert!(!is_ours("/home/x/Apps/Firefox.AppImage"));
    assert!(!is_ours(r"C:\versekit\fremd.exe"), "the folder does not count, the name does");
    let dir = Dir::new("updater-appimage");
    let own = dir.path().join("VerseKit-x86_64.AppImage");
    let other = dir.path().join("Krita.AppImage");
    fs::write(&own, b"x").unwrap();
    fs::write(&other, b"x").unwrap();
    assert_eq!(own_appimage(Some(&own.to_string_lossy())), Some(own.clone()));
    assert_eq!(own_appimage(Some(&other.to_string_lossy())), None, "APPIMAGE of another program");
    assert_eq!(own_appimage(Some(&dir.path().join("weg.AppImage").to_string_lossy())), None);
    assert_eq!(packaging(Some(&own.to_string_lossy()), "5.0.0"), Packaging::AppImage);
    assert_eq!(packaging(None, "5.0.0-dev"), Packaging::Source, "a build of the source");
    let release_build = if cfg!(windows) { Packaging::Exe } else { Packaging::Source };
    assert_eq!(packaging(None, "5.0.0"), release_build);
}

#[test]
fn a_test_build_rc_updates_itself_to_the_next_rc_and_on_to_the_finished_version() {
    let release_build = if cfg!(windows) { Packaging::Exe } else { Packaging::Source };
    assert_eq!(packaging(None, "5.0.0-rc1"), release_build, "a test build is a program file, not the source");
    let (_dir, paths) = paths_in("updater-rc", &[]);
    let body = format!(
        r#"[{{"tag_name":"v5.0.0-rc2","published_at":"2026-11-03T10:00:00Z","prerelease":true,
             "assets":[{{"name":"VerseKit-Setup.exe","browser_download_url":"{BASE}/v5.0.0-rc2/VerseKit-Setup.exe"}}]}},
            {{"tag_name":"v5.0.0-rc1","published_at":"2026-11-01T10:00:00Z","prerelease":true,"assets":[]}}]"#
    );
    let net = Fake::default().with(API, body.into_bytes());
    let offer = check(&net, &paths, "5.0.0-rc1", false, 1000.0).offer.expect("rc2 without the test channel setting");
    assert_eq!(offer.version, "v5.0.0-rc2");
    assert!(check(&net, &paths, "5.0.0-rc2", true, 2000.0).offer.is_none(), "rc2 is the newest");
    let finished = vec![
        Release { version: "v5.0.0".into(), ..Release::default() },
        Release { version: "v5.0.0-rc2".into(), prerelease: true, ..Release::default() },
    ];
    assert_eq!(best_newer(&finished, "5.0.0-rc2", true).map(|r| r.version.as_str()), Some("v5.0.0"));
}

#[test]
fn a_test_build_asks_the_test_versions_too_and_a_finished_one_does_not() {
    let test_base = "https://github.com/Xharig/VerseKit-Testfassungen/releases/download";
    let (_dir, paths) = paths_in("updater-test-repo", &[]);
    let body = format!(
        r#"[{{"tag_name":"v5.0.0-rc3","published_at":"2026-11-05T10:00:00Z","prerelease":false,
             "assets":[{{"name":"VerseKit-Setup.exe","browser_download_url":"{test_base}/v5.0.0-rc3/VerseKit-Setup.exe"}}]}}]"#
    );
    // Only the test versions answer: the main repository is not reachable, the offer comes anyway.
    let net = Fake::default().with(TEST_API, body.into_bytes());
    let checked = check(&net, &paths, "5.0.0-rc2", true, 1000.0);
    assert_eq!(checked.fetched, Some(true), "one answer is enough");
    let offer = checked.offer.expect("rc3 from the test versions");
    assert_eq!(offer.version, "v5.0.0-rc3");
    assert_eq!(net.asked(), [TEST_API.to_string(), API.to_string()]);
    assert!(matching_asset(&offer, Packaging::Exe).is_some(), "the file of the test versions is accepted");
    // A finished version asks the main repository only.
    let finished = Fake::default().with(API, b"[]".to_vec());
    let _ = check(&finished, &paths, "5.0.0", true, 2000.0);
    assert_eq!(finished.asked(), [API.to_string()]);
    // Another repository stays refused.
    assert!(!url_ok("https://github.com/Xharig/VerseKit-Fremd/releases/download/v1/VerseKit-Setup.exe"));
    assert!(url_ok(&format!("{test_base}/v5.0.0-rc3/SHA256SUMS.txt")));
}

#[test]
fn the_answer_of_github_is_remembered_and_asked_again_only_after_ten_minutes() {
    let (_dir, paths) = paths_in("updater-check", &[]);
    let body = format!(
        r#"[{{"tag_name":"v5.0.1","name":"5.0.1","published_at":"2026-11-20T10:00:00Z","body":"Neu","prerelease":false,"draft":false,
             "assets":[{{"name":"VerseKit-Setup.exe","browser_download_url":"{BASE}/v5.0.1/VerseKit-Setup.exe","size":12}}]}},
            {{"tag_name":"v5.1.0","draft":true,"assets":[]}},
            {{"tag_name":"v5.0.2-rc1","published_at":"2026-11-21T10:00:00Z","prerelease":true,"assets":[]}}]"#
    );
    let net = Fake::default().with(API, body.into_bytes());
    let first = check(&net, &paths, "5.0.0", false, 1000.0);
    assert_eq!(first.fetched, Some(true));
    let offer = first.offer.expect("5.0.1 is newer");
    assert_eq!((offer.version.as_str(), offer.date.as_str()), ("v5.0.1", "2026-11-20"));
    assert_eq!(offer.assets[0].size, Some(12));
    let cache = cache_read(&paths);
    assert_eq!(cache.releases.len(), 2, "the draft is left out");
    assert_eq!(cache.checked, 1000.0);
    // Within ten minutes the remembered answer counts, without asking.
    let again = check(&net, &paths, "5.0.0", false, 1000.0 + MIN_INTERVAL - 1.0);
    assert_eq!((again.fetched, again.offer.map(|r| r.version)), (None, Some("v5.0.1".into())));
    assert_eq!(net.asked().len(), 1);
    // Later, or by force, it asks again; test versions only with the setting.
    let _ = check(&net, &paths, "5.0.0", true, 1001.0);
    assert_eq!(net.asked().len(), 2);
    assert!(paths.set_setting(PRERELEASE_SETTING, jsonio::Value::Bool(true)));
    assert_eq!(known_newer(&paths, "5.0.0").map(|r| r.version), Some("v5.0.2-rc1".into()));
}

#[test]
fn without_the_net_nothing_is_asked_but_what_is_known_is_offered() {
    let (_dir, paths) = paths_in("updater-ohne-netz", &[(legacy::ENV_NO_NET, "1")]);
    cache_write(&paths, &Cache { checked: 0.0, releases: vec![release("v5.0.1", false)] }).unwrap();
    let net = Fake::default();
    let checked = check(&net, &paths, "5.0.0", true, 99_999.0);
    assert!(net.asked().is_empty(), "SC_BP_NO_NET forbids asking");
    assert_eq!((checked.fetched, checked.offer.map(|r| r.version)), (None, Some("v5.0.1".into())));
}

#[test]
fn a_failed_look_says_so_and_keeps_what_is_known() {
    let (_dir, paths) = paths_in("updater-fehlschlag", &[]);
    cache_write(&paths, &Cache { checked: 0.0, releases: vec![release("v5.0.1", false)] }).unwrap();
    let checked = check(&Fake::default(), &paths, "5.0.0", false, 99_999.0);
    assert_eq!(checked.fetched, Some(false), "asking failed: no all-clear");
    assert_eq!(checked.offer.map(|r| r.version), Some("v5.0.1".into()));
    assert_eq!(cache_read(&paths).checked, 0.0, "the cache stays as it was");
}

/// A release with an installer and its checksum file on the stand-in net.
fn published(content: &[u8], checksums: &str) -> (Release, Fake) {
    let mut r = release("v5.0.1", false);
    r.assets = vec![asset("VerseKit-Setup.exe"), asset(CHECKSUM_FILE)];
    let net = Fake::default()
        .with(&format!("{BASE}/v5.0.1/VerseKit-Setup.exe"), content.to_vec())
        .with(&format!("{BASE}/v5.0.1/{CHECKSUM_FILE}"), checksums.as_bytes().to_vec());
    (r, net)
}

#[test]
fn a_download_with_the_published_checksum_is_kept_with_it() {
    let content = b"ein Installer".to_vec();
    let (r, net) = published(&content, &format!("{}  VerseKit-Setup.exe\n", sha(&content)));
    let dir = Dir::new("updater-laden");
    let target = dir.path().join("VerseKit-Setup.exe");
    let mut seen = Vec::new();
    let got = download(&net, &r, &r.assets[0], &target, &mut |p| seen.push(p)).expect("fetched");
    assert_eq!(got, Downloaded { path: target.clone(), checksum: sha(&content) });
    assert_eq!(fs::read(&target).unwrap(), content);
    assert_eq!(seen.last(), Some(&100));
    assert!(still_matches(&target, &got.checksum));
    // The checksums come before the file.
    assert!(net.asked()[0].ends_with(CHECKSUM_FILE), "{:?}", net.asked());
}

#[test]
fn a_file_that_does_not_match_its_checksum_is_thrown_away() {
    let (r, net) = published(b"manipuliert", &format!("{}  VerseKit-Setup.exe\n", sha(b"echt")));
    let dir = Dir::new("updater-falsch");
    let target = dir.path().join("VerseKit-Setup.exe");
    assert_eq!(download(&net, &r, &r.assets[0], &target, &mut |_| {}), Err(UpdateError::ChecksumWrong));
    assert!(!target.exists(), "no fragment stays");
}

#[test]
fn without_a_valid_checksum_nothing_is_fetched_and_each_reason_has_its_sentence() {
    let dir = Dir::new("updater-ohne-summe");
    let target = dir.path().join("VerseKit-Setup.exe");
    // No checksum file in the release.
    let mut r = release("v5.0.1", false);
    r.assets = vec![asset("VerseKit-Setup.exe")];
    let net = Fake::default().with(&r.assets[0].url, b"x".to_vec());
    assert_eq!(
        download(&net, &r, &r.assets[0], &target, &mut |_| {}),
        Err(UpdateError::NoChecksum(ChecksumProblem::Missing))
    );
    assert_eq!(net.asked().len(), 0, "nothing fetched in vain");
    // The checksum file from somewhere else.
    r.assets.push(Asset { name: CHECKSUM_FILE.into(), url: "https://example.org/SHA256SUMS.txt".into(), size: None });
    assert_eq!(
        download(&net, &r, &r.assets[0], &target, &mut |_| {}),
        Err(UpdateError::NoChecksum(ChecksumProblem::Foreign))
    );
    // Not to be fetched.
    r.assets[1] = asset(CHECKSUM_FILE);
    assert_eq!(
        download(&net, &r, &r.assets[0], &target, &mut |_| {}),
        Err(UpdateError::NoChecksum(ChecksumProblem::Net))
    );
    // The file not in it, or the file twice.
    let (r2, net2) = published(b"x", &format!("{}  anderes.exe\n", sha(b"x")));
    assert_eq!(
        download(&net2, &r2, &r2.assets[0], &target, &mut |_| {}),
        Err(UpdateError::NoChecksum(ChecksumProblem::Missing))
    );
    let (r3, net3) = published(b"x", &format!("{0}  VerseKit-Setup.exe\n{0}  VerseKit-Setup.exe\n", sha(b"x")));
    assert_eq!(
        download(&net3, &r3, &r3.assets[0], &target, &mut |_| {}),
        Err(UpdateError::NoChecksum(ChecksumProblem::Missing))
    );
    // A file from elsewhere, a name the update does not fetch.
    let foreign =
        Asset { name: "VerseKit-Setup.exe".into(), url: "https://example.org/VerseKit-Setup.exe".into(), size: None };
    assert_eq!(download(&net2, &r2, &foreign, &target, &mut |_| {}), Err(UpdateError::ForeignSource));
    let odd = asset("update.bin");
    assert_eq!(download(&net2, &r2, &odd, &target, &mut |_| {}), Err(UpdateError::ForeignFile("update.bin".into())));
    assert!(!target.exists());
    // Four situations, four sentences.
    let keys: Vec<String> = [ChecksumProblem::Missing, ChecksumProblem::Foreign, ChecksumProblem::Net]
        .into_iter()
        .map(|p| UpdateError::NoChecksum(p).message())
        .chain([UpdateError::ChecksumWrong.message()])
        .collect();
    assert!(keys.iter().all(|k| !k.is_empty() && !k.starts_with("up_")), "{keys:?}");
    assert_eq!(keys.iter().collect::<std::collections::BTreeSet<_>>().len(), 4);
}

#[test]
fn the_file_goes_next_to_the_appimage_and_else_to_the_temporary_folder() {
    let dir = Dir::new("updater-ziel");
    let temp = Path::new("/tmp-ersatz");
    let appimage = dir.path().join("VerseKit-x86_64.AppImage");
    assert_eq!(
        download_target("VerseKit-x86_64.AppImage", Packaging::AppImage, Some(&appimage), temp),
        dir.path().join(".VerseKit-x86_64.AppImage.neu")
    );
    assert_eq!(download_target("VerseKit-Setup.exe", Packaging::Exe, None, temp), temp.join("VerseKit-Setup.exe"));
    let gone = dir.path().join("fehlt").join("VerseKit-x86_64.AppImage");
    assert_eq!(download_target("x.AppImage", Packaging::AppImage, Some(&gone), temp), temp.join("x.AppImage"));
}

#[test]
fn the_environment_for_the_new_version_loses_the_traces_of_the_old_one() {
    let vars = [
        ("PATH", "/usr/bin"),
        ("LD_LIBRARY_PATH", "/tmp/.mount_x/usr/lib"),
        ("XDG_DATA_DIRS_ORIG", "/usr/share"),
        ("XDG_DATA_DIRS", "/tmp/.mount_x/share"),
        ("APPIMAGE", "/a/VerseKit.AppImage"),
        ("APPDIR", "/tmp/.mount_x"),
        ("_PYI_APPLICATION_HOME_DIR", "x"),
        ("__COMPAT_LAYER", "x"),
        ("VERSEKIT_UPDATE_SETUP", "x"),
    ]
    .map(|(k, v)| (k.to_string(), v.to_string()));
    let mut clean = clean_environment(vars);
    clean.sort();
    assert_eq!(
        clean,
        [("PATH".to_string(), "/usr/bin".to_string()), ("XDG_DATA_DIRS".to_string(), "/usr/share".to_string())]
    );
}

/// The AppImage runtime sets no library paths of its own: what is there without an `*_ORIG` is the user's, and
/// only its folders inside the mount go (the icons and portals of the desktop live in `XDG_DATA_DIRS`).
#[test]
fn the_library_paths_of_the_user_stay_without_the_folders_of_the_mount() {
    let pairs = |list: &[(&str, &str)]| list.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<Vec<_>>();
    let mount = "/tmp/.mount_VerseKabc";
    let start = pairs(&[
        ("APPDIR", mount),
        ("LD_LIBRARY_PATH", "/opt/cuda/lib64"),
        ("XDG_DATA_DIRS", "/tmp/.mount_VerseKabc/usr/share:/home/a/.local/share/flatpak/exports/share:/usr/share"),
        ("GTK_PATH", "/tmp/.mount_VerseKabc/usr/lib/gtk-3.0"),
        ("PYTHONPATH", "/run/appimage_x/lib:/home/a/py"),
        ("VERSEKIT_ERSETZT_PID", "4711"),
    ]);
    // Windows as before: a library path without `*_ORIG` goes whole.
    assert!(clean_environment_with(start.clone(), false).is_empty());
    let clean_environment = |vars| clean_environment_with(vars, true);
    let mut clean = clean_environment(start);
    clean.sort();
    assert_eq!(
        clean,
        pairs(&[
            ("LD_LIBRARY_PATH", "/opt/cuda/lib64"),
            ("PYTHONPATH", "/run/appimage_x/lib:/home/a/py"),
            ("XDG_DATA_DIRS", "/home/a/.local/share/flatpak/exports/share:/usr/share"),
        ]),
        "the user's folders stay, the mount's go, a value only of the mount goes whole"
    );
    // A mount under another name than `.mount_*`: its `APPDIR` names it.
    let clean = clean_environment(pairs(&[
        ("APPDIR", "/run/user/1000/appimage_x/"),
        ("XDG_DATA_DIRS", "/run/user/1000/appimage_x/usr/share:/run/user/1000/appimage_xy/share:/usr/share"),
    ]));
    assert_eq!(clean, pairs(&[("XDG_DATA_DIRS", "/run/user/1000/appimage_xy/share:/usr/share")]));
    // An `*_ORIG` still wins, also an empty one (the value was not set before the AppImage).
    let clean = clean_environment(pairs(&[("LD_LIBRARY_PATH", "/opt/x"), ("LD_LIBRARY_PATH_ORIG", "")]));
    assert!(clean.is_empty(), "{clean:?}");
}
