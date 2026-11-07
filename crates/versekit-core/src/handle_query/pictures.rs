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

//! The pictures of a card (profile picture, title picture, logos of the organizations), kept in the folder
//! [`FOLDER`] of `Intern`. A picture younger than [`FRESH_DAYS`] comes from there; an older one is fetched anew and
//! stands while the site cannot be reached. The folder keeps at most [`FILES_MAX`] pictures and [`BYTES_MAX`]
//! bytes, the oldest go first, and none older than [`DROP_DAYS`].

use std::path::Path;
use std::time::{Duration, SystemTime};

use crate::jsonio;
use crate::net::{self, Net};
use crate::paths::Paths;

/// The folder of the pictures, in `Intern`.
pub const FOLDER: &str = "handle-bilder";
/// How old a picture kept may be before it is fetched anew.
pub const FRESH_DAYS: u64 = 14;
/// How old a picture kept may get at all.
pub const DROP_DAYS: u64 = 60;
/// The largest picture taken, in bytes.
pub const PICTURE_MAX: usize = 1024 * 1024;
/// The most pictures and bytes the folder keeps.
pub const FILES_MAX: usize = 300;
pub const BYTES_MAX: u64 = 30 * 1024 * 1024;
/// The ending of a picture kept.
const ENDING: &str = "bild";

const DAY: Duration = Duration::from_secs(86_400);

/// The name of the file of `url`: a hash of the address.
pub fn file_name(url: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in url.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}.{ENDING}")
}

/// The folder of the pictures in the data folder.
pub fn folder(paths: &Paths) -> String {
    paths.app_file(FOLDER)
}

/// How old the file `path` is, from the time it was written.
fn age(path: &str) -> Option<Duration> {
    let written = std::fs::metadata(path).and_then(|m| m.modified()).ok()?;
    Some(SystemTime::now().duration_since(written).unwrap_or_default())
}

/// The bytes of the picture at `url`: kept while fresh, else fetched through `net` and kept. Without net, or when
/// the site does not answer, the picture kept stands however old. `None` when there is none.
pub fn get(paths: &Paths, net: &dyn Net, url: &str) -> Option<Vec<u8>> {
    if !url.starts_with("https://") {
        return None;
    }
    let folder = folder(paths);
    let path = Path::new(&folder).join(file_name(url)).to_string_lossy().into_owned();
    let kept = std::fs::read(&path).ok().filter(|b| !b.is_empty());
    let fresh = age(&path).is_some_and(|a| a < DAY * FRESH_DAYS as u32);
    if (kept.is_some() && fresh) || net::offline_in(&paths.host) {
        return kept;
    }
    match net.get(url) {
        Ok(bytes) if !bytes.is_empty() && bytes.len() <= PICTURE_MAX => {
            if store(&folder, &path, &bytes) {
                prune(&folder, FILES_MAX, BYTES_MAX, DAY * DROP_DAYS as u32);
            }
            Some(bytes)
        }
        _ => kept,
    }
}

/// Writes `bytes` to `path` in `folder` without leaving a half-written file.
fn store(folder: &str, path: &str, bytes: &[u8]) -> bool {
    let temp = jsonio::temp_name(path);
    jsonio::make_dirs(folder).is_ok()
        && jsonio::write_new(&temp, bytes).is_ok()
        && jsonio::replace_file(&temp, path).is_ok()
}

/// Leaves at most `files` pictures and `bytes` bytes in `folder`, none older than `oldest`: the newest stay.
/// The number of pictures taken away.
pub fn prune(folder: &str, files: usize, bytes: u64, oldest: Duration) -> usize {
    let Ok(entries) = std::fs::read_dir(folder) else { return 0 };
    let mut kept: Vec<(Duration, u64, String)> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == ENDING))
        .filter_map(|e| {
            let path = e.path().to_string_lossy().into_owned();
            let size = e.metadata().ok()?.len();
            Some((age(&path)?, size, path))
        })
        .collect();
    kept.sort_by_key(|(age, _, _)| *age);
    let (mut count, mut total, mut gone) = (0usize, 0u64, 0usize);
    for (age, size, path) in kept {
        if count < files && total + size <= bytes && age <= oldest {
            count += 1;
            total += size;
        } else if jsonio::remove_file(&path).is_ok() {
            gone += 1;
        }
    }
    gone
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::Fake;
    use crate::paths::Host;
    use crate::pypath::Flavor;
    use crate::testdir::Dir as TestDir;

    fn paths(dir: &TestDir, extra: &[(&str, &str)]) -> Paths {
        let home = dir.path().to_string_lossy().into_owned();
        let mut vars = vec![(crate::legacy::ENV_HOME, home.as_str())];
        vars.extend_from_slice(extra);
        Paths::new(Host::new(Flavor::native(), &vars, &home))
    }

    const URL: &str = "https://robertsspaceindustries.com/media/test/logo.png";

    #[test]
    fn a_picture_is_fetched_once_and_then_comes_from_the_folder() {
        let dir = TestDir::new("handle-bilder-holen");
        let paths = paths(&dir, &[]);
        let net = Fake::default().with(URL, b"bild".to_vec());
        assert_eq!(get(&paths, &net, URL).as_deref(), Some(&b"bild"[..]));
        assert_eq!(get(&paths, &net, URL).as_deref(), Some(&b"bild"[..]));
        assert_eq!(net.asked(), [URL], "the second time from the folder");
        assert!(Path::new(&folder(&paths)).join(file_name(URL)).is_file());
        // Without net only the folder answers; a picture not kept is none.
        let offline = super::tests::paths(&dir, &[(crate::legacy::ENV_NO_NET, "1")]);
        let other = "https://robertsspaceindustries.com/media/other.png";
        let quiet = Fake::default().with(other, b"x".to_vec());
        assert_eq!(get(&offline, &quiet, other), None);
        assert!(quiet.asked().is_empty());
        // Only https, and nothing too large.
        assert_eq!(get(&paths, &net, "http://robertsspaceindustries.com/x.png"), None);
        let big = "https://robertsspaceindustries.com/big.png";
        let large = Fake::default().with(big, vec![1u8; PICTURE_MAX + 1]);
        assert_eq!(get(&paths, &large, big), None);
    }

    #[test]
    fn the_folder_keeps_the_newest_within_its_limits() {
        let dir = TestDir::new("handle-bilder-grenzen");
        let folder = dir.path().join(FOLDER);
        std::fs::create_dir_all(&folder).expect("folder");
        let now = SystemTime::now();
        for n in 0..6u64 {
            let path = folder.join(format!("{n:016x}.{ENDING}"));
            std::fs::write(&path, vec![0u8; 100]).expect("written");
            let file = std::fs::File::options().write(true).open(&path).expect("open");
            file.set_modified(now - DAY * (n as u32 * 10)).expect("time");
        }
        std::fs::write(folder.join("fremd.txt"), b"bleibt").expect("other file");
        let folder = folder.to_string_lossy().into_owned();
        // Older than 45 days: the pictures of 50 days go.
        assert_eq!(prune(&folder, 100, 1_000_000, DAY * 45), 1);
        // Four at most, then 250 bytes at most.
        assert_eq!(prune(&folder, 4, 1_000_000, DAY * 45), 1);
        assert_eq!(prune(&folder, 4, 250, DAY * 45), 2);
        let left: Vec<String> = std::fs::read_dir(&folder)
            .expect("folder")
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left.len(), 3, "{left:?}");
        assert!(left.contains(&format!("{:016x}.{ENDING}", 0)) && left.contains(&format!("{:016x}.{ENDING}", 1)));
        assert!(left.contains(&"fremd.txt".to_string()), "only pictures are taken away");
    }
}
