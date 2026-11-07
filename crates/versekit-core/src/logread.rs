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

//! Reading a game log a piece at a time, so that a log of gigabytes never stands in memory whole.
//!
//! ⚠ The game can fill its log with millions of lines in one session. A reader that comes back to the same
//! log again and again keeps where it stopped ([`Kept`]) and reads only what was added since; [`Session`] tells
//! whether the file under the path is still the log of the same game session (the game starts a new log under
//! the same name, and Windows may even hand the new file the time of creation of the old one).

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::Mutex;
use std::time::SystemTime;

/// The most bytes read at once.
pub const BLOCK: usize = 4 << 20;
/// How far back from the end a reader that only needs the newest lines starts at the most.
pub const BACK_LIMIT: u64 = 32 << 20;
/// The bytes at the start of a log that tell the log of one session from the next one (its first line carries
/// the time the game started).
const HEAD_ID: usize = 1024;
/// How many logs a [`Kept`] keeps in mind.
const KEPT: usize = 4;

/// What tells the log of one game session from the next one under the same path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    path: String,
    created: Option<SystemTime>,
    head: Vec<u8>,
}

impl Session {
    /// The file just opened is the log `earlier` was taken from, grown or not: the same path, the same time of
    /// creation where the system keeps one, and the start read then still stands at its start.
    pub fn continues(&self, earlier: &Session) -> bool {
        self.path == earlier.path && self.created == earlier.created && self.head.starts_with(&earlier.head)
    }
}

/// A log opened for reading, with its size at the opening.
pub struct Opened {
    pub file: File,
    pub size: u64,
    pub session: Session,
}

/// Opens a log and reads what tells its session.
pub fn open(path: &str) -> io::Result<Opened> {
    let mut file = File::open(path)?;
    let meta = file.metadata()?;
    let head = read_start(&mut file, HEAD_ID)?;
    Ok(Opened {
        file,
        size: meta.len(),
        session: Session { path: path.to_string(), created: meta.created().ok(), head },
    })
}

/// The first `limit` bytes of the file (fewer when it is shorter).
pub fn read_start(file: &mut File, limit: usize) -> io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = vec![0; limit];
    let got = fill(file, &mut bytes)?;
    count(got, limit);
    bytes.truncate(got);
    Ok(bytes)
}

/// Reads until `buf` is full or the file ends; the number of bytes read.
fn fill(file: &mut File, buf: &mut [u8]) -> io::Result<usize> {
    let mut got = 0;
    while got < buf.len() {
        match file.read(&mut buf[got..]) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(got)
}

/// Hands the lines between `start` and `end` to `each`, in pieces of at most [`BLOCK`] bytes, each ending after
/// a `\n` (a line longer than a block is handed in parts). With `cut`, `start` may lie inside a line: the bytes
/// up to the first line end are passed over. A last line without its end is not handed; it is read the next
/// time. Returns where the next reading starts: after the last line end handed.
pub fn read_lines(file: &mut File, start: u64, end: u64, cut: bool, mut each: impl FnMut(&[u8])) -> io::Result<u64> {
    file.seek(SeekFrom::Start(start))?;
    let mut buf: Vec<u8> = Vec::new();
    let mut left = end.saturating_sub(start);
    // The place in the file of the first byte in `buf`.
    let mut at = start;
    let mut skipping = cut && start > 0;
    while left > 0 {
        let old = buf.len();
        let want = usize::try_from(left).unwrap_or(usize::MAX).min(BLOCK - old);
        buf.resize(old + want, 0);
        let got = fill(file, &mut buf[old..])?;
        count(got, buf.len());
        buf.truncate(old + got);
        if got == 0 {
            break;
        }
        left -= got as u64;
        let mut from = 0;
        if skipping {
            match buf.iter().position(|&b| b == b'\n') {
                Some(i) => {
                    from = i + 1;
                    skipping = false;
                }
                None => {
                    at += buf.len() as u64;
                    buf.clear();
                    continue;
                }
            }
        }
        match buf[from..].iter().rposition(|&b| b == b'\n') {
            Some(i) => {
                let done = from + i + 1;
                each(&buf[from..done]);
                buf.drain(..done);
                at += done as u64;
            }
            None if buf.len() == BLOCK => {
                each(&buf[from..]);
                at += buf.len() as u64;
                buf.clear();
            }
            None => {
                buf.drain(..from);
                at += from as u64;
            }
        }
    }
    Ok(at)
}

/// The bytes from `start` to `end`, at most one [`BLOCK`]: the last line, not yet ended, after [`read_lines`].
pub fn read_rest(file: &mut File, start: u64, end: u64) -> io::Result<Vec<u8>> {
    let length = usize::try_from(end.saturating_sub(start)).unwrap_or(usize::MAX).min(BLOCK);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = vec![0; length];
    let got = fill(file, &mut bytes)?;
    count(got, length);
    bytes.truncate(got);
    Ok(bytes)
}

/// What a reader keeps of a log it follows: the session, where the next reading starts and its own state.
pub struct Followed<T> {
    pub session: Session,
    pub next: u64,
    pub state: T,
}

/// What a reader keeps of the last few logs it read.
pub struct Kept<T> {
    slots: Mutex<Vec<Followed<T>>>,
}

impl<T> Kept<T> {
    pub const fn new() -> Self {
        Kept { slots: Mutex::new(Vec::new()) }
    }

    /// Takes out what was kept of the log just opened: `None` when nothing was, or when the file is another
    /// session's log now or shorter than where the reading stopped.
    pub fn take(&self, opened: &Opened) -> Option<Followed<T>> {
        let mut slots = self.slots.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let index = slots.iter().position(|f| f.session.path == opened.session.path)?;
        let followed = slots.remove(index);
        (opened.session.continues(&followed.session) && opened.size >= followed.next).then_some(followed)
    }

    /// Keeps what was read of a log, the newest first; the oldest one goes beyond [`KEPT`].
    pub fn put(&self, followed: Followed<T>) {
        let mut slots = self.slots.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        slots.retain(|f| f.session.path != followed.session.path);
        slots.insert(0, followed);
        slots.truncate(KEPT);
    }
}

impl<T> Default for Kept<T> {
    fn default() -> Self {
        Kept::new()
    }
}

#[cfg(test)]
thread_local! {
    /// The bytes this thread read from logs, and the largest buffer it read them into.
    static COUNTED: std::cell::Cell<(u64, usize)> = const { std::cell::Cell::new((0, 0)) };
}

fn count(_read: usize, _buffer: usize) {
    #[cfg(test)]
    COUNTED.with(|c| {
        let (read, largest) = c.get();
        c.set((read + _read as u64, largest.max(_buffer)));
    });
}

/// The bytes this thread read from logs since the last call, and the largest buffer it used.
#[cfg(test)]
pub(crate) fn counted() -> (u64, usize) {
    COUNTED.with(|c| c.replace((0, 0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("versekit-logread-{}-{name}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn lines_of(path: &std::path::Path, start: u64, cut: bool) -> (Vec<String>, u64) {
        let mut file = File::open(path).unwrap();
        let size = file.metadata().unwrap().len();
        let mut pieces = Vec::new();
        let next =
            read_lines(&mut file, start, size, cut, |b| pieces.push(String::from_utf8_lossy(b).into_owned())).unwrap();
        (pieces, next)
    }

    #[test]
    fn whole_lines_are_handed_and_the_unended_one_waits() {
        let dir = scratch("zeilen");
        let log = dir.join("Game.log");
        std::fs::write(&log, "one\ntwo\nthr").unwrap();
        let (pieces, next) = lines_of(&log, 0, false);
        assert_eq!((pieces.concat(), next), ("one\ntwo\n".to_string(), 8));
        // Started inside a line, with `cut` the rest of that line is passed over.
        let (pieces, next) = lines_of(&log, 1, true);
        assert_eq!((pieces.concat(), next), ("two\n".to_string(), 8));
        let mut file = File::open(&log).unwrap();
        assert_eq!(read_rest(&mut file, 8, 11).unwrap(), b"thr");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pieces_stay_within_a_block_and_lose_no_line() {
        let dir = scratch("bloecke");
        let log = dir.join("Game.log");
        let line = "<2026-11-05T22:30:00.000Z> a line of the log that fills it\n";
        let text = line.repeat(BLOCK * 3 / line.len());
        std::fs::write(&log, &text).unwrap();
        counted();
        let (pieces, next) = lines_of(&log, 0, false);
        let (read, largest) = counted();
        assert!(pieces.len() >= 3 && pieces.iter().all(|p| p.len() <= BLOCK && p.ends_with('\n')));
        assert_eq!(pieces.concat(), text);
        assert_eq!((next, read), (text.len() as u64, text.len() as u64));
        assert!(largest <= BLOCK, "{largest}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_same_session_goes_on_and_a_new_log_under_the_name_does_not() {
        let dir = scratch("sitzung");
        let log = dir.join("Game.log");
        let path = log.to_string_lossy().into_owned();
        std::fs::write(&log, "<2026-11-05T22:27:20.000Z> Log started\n").unwrap();
        let kept: Kept<u32> = Kept::new();
        let opened = open(&path).unwrap();
        kept.put(Followed { session: opened.session, next: opened.size, state: 7 });
        std::fs::write(&log, "<2026-11-05T22:27:20.000Z> Log started\nmore\n").unwrap();
        let opened = open(&path).unwrap();
        let again = kept.take(&opened).expect("the same log, grown");
        assert_eq!(again.state, 7);
        kept.put(Followed { session: opened.session, next: opened.size, state: 8 });
        // The game started anew: another first line under the same name.
        std::fs::write(&log, "<2026-11-05T23:18:00.000Z> Log started\nmore and more lines\n").unwrap();
        assert!(kept.take(&open(&path).unwrap()).is_none());
        // A log shorter than where the reading stopped is read anew as well.
        std::fs::write(&log, "<2026-11-05T23:18:00.000Z> Log started\n").unwrap();
        let opened = open(&path).unwrap();
        kept.put(Followed { session: opened.session, next: opened.size + 5, state: 9 });
        assert!(kept.take(&open(&path).unwrap()).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The two lines the game writes millions of times when it floods its log.
    const FLOOD: &str = "<2026-11-05T22:30:00.123Z> [Notice] <Monitored Space> \"Überwachter Raum ausgefallen: \" [4711]\n\
<2026-11-05T22:30:00.124Z> [Notice] <Monitored Space> \"Überwachter Raum wiederhergestellt: \" [4711]\n";

    /// About `megabytes` of flood lines.
    fn flood(megabytes: usize) -> String {
        FLOOD.repeat((megabytes << 20) / FLOOD.len())
    }

    fn append(path: &str, text: &str) {
        use std::io::Write;
        std::fs::File::options().append(true).open(path).and_then(|mut f| f.write_all(text.as_bytes())).unwrap();
    }

    fn paths_in(home: &str, game: &str) -> crate::paths::Paths {
        use crate::paths::{ENV_INSTALL_DIR, Host, Paths};
        Paths::new(Host::new(
            crate::pypath::Flavor::Posix,
            &[("SC_BP_HOME", home), ("HOME", home), (ENV_INSTALL_DIR, game)],
            home,
        ))
    }

    const START: &str = "<2026-11-05T22:27:20.000Z> Log started on Thu Nov  5 22:27:20 2026\n\
<2026-11-05T22:27:20.001Z> Host CPU: AMD Ryzen 7 5800X3D 8-Core Processor\n\
<2026-11-05T22:28:00.000Z> <Update Inventory Location> Player [Xy] is changing location. Landing [0] -> [4711]. Location [a] -> [b]\n\
<2026-11-05T22:29:00.000Z> <CEntityComponentShopUIProvider::SendShopBuyRequest> Sending SShopBuyRequest - playerId[1] shopId[2] shopName[SCShop_Test] kioskId[3] client_price[150.000000] itemClassGUID[abc] itemName[behr_lmg_ballistic_01_mag] quantity[3]\n\
<2026-11-05T22:29:00.500Z> <CEntityComponentShopUIProvider::RmShopFlowResponse> Received response type[Buying] result[Success]\n";

    #[test]
    fn a_flooded_log_is_read_within_the_limit_and_then_only_what_was_added() {
        let dir = scratch("flut");
        let home = dir.to_string_lossy().replace('\\', "/");
        let game = format!("{home}/StarCitizen/LIVE");
        std::fs::create_dir_all(&game).unwrap();
        let log = format!("{game}/Game.log");
        {
            use std::io::Write;
            let mut out = std::io::BufWriter::new(std::fs::File::create(&log).unwrap());
            out.write_all(START.as_bytes()).unwrap();
            let chunk = flood(1);
            for _ in 0..300 {
                out.write_all(chunk.as_bytes()).unwrap();
            }
        }
        let size = std::fs::metadata(&log).unwrap().len();
        assert!(size > 300_000_000, "{size}");
        let paths = paths_in(&home, &game);
        let one_mb = flood(1);
        let added_most = one_mb.len() as u64 + 64 * 1024;
        counted();

        // The place: the change stands at the very start, further back than the limit.
        let first = crate::location::place(&paths, None).expect("a log");
        let (read, largest) = counted();
        assert_eq!(first.number, None, "the change lies beyond the limit");
        assert!(read <= BACK_LIMIT + HEAD_ID as u64, "{read}");
        assert!(largest <= BLOCK, "{largest}");
        append(&log, &one_mb);
        append(
            &log,
            "<2026-11-05T23:00:00.000Z> <Update Inventory Location> Player [Xy] is changing location. Landing [4711] -> [0]\n",
        );
        let second = crate::location::place(&paths, None).expect("a log");
        let (read, _) = counted();
        assert_eq!((second.number.as_deref(), second.travelling), (Some("0"), true));
        assert!(read <= added_most, "only what was added: {read}");

        // The class: the whole log is searched a piece at a time, once.
        let info = crate::machine_info::read(&paths, None).expect("a log");
        let (read, largest) = counted();
        assert_eq!(info.cpu.as_deref(), Some("AMD Ryzen 7 5800X3D 8-Core Processor"));
        assert_eq!(info.machine_class, None);
        assert!(largest <= BLOCK, "never more than a block at once: {largest}");
        assert!(read <= std::fs::metadata(&log).unwrap().len() + (1 << 20), "{read}");
        append(&log, &one_mb);
        append(&log, "<2026-11-05T23:01:00.000Z> Final rating: Machine class 3\n");
        let info = crate::machine_info::read(&paths, None).expect("a log");
        let (read, _) = counted();
        assert_eq!(info.machine_class, Some(3));
        assert!(read <= added_most, "only what was added: {read}");

        // The stock journal: every line worth a look of the whole log, read a piece at a time, then on from there.
        let path = log.as_str();
        let entry = crate::stock_journal::read_log(path, None).expect("a session");
        let (read, largest) = counted();
        assert!(largest <= BLOCK, "{largest}");
        assert!(read <= std::fs::metadata(&log).unwrap().len() + HEAD_ID as u64, "{read}");
        let bought = |entry: &crate::jsonio::Map| match crate::mission_log::get(entry, "kaeufe") {
            Some(crate::jsonio::Value::Array(items)) => items.len(),
            _ => 0,
        };
        assert_eq!(bought(&entry), 1);
        append(&log, &one_mb);
        let entry = crate::stock_journal::read_log(path, None).expect("a session");
        let (read, _) = counted();
        assert_eq!(bought(&entry), 1);
        assert!(read <= added_most, "only what was added: {read}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reading_on_finds_the_same_as_reading_the_whole_log_anew() {
        let dir = scratch("gleich");
        let log = dir.join("Game.log");
        let path = log.to_string_lossy().into_owned();
        let lines: Vec<&str> = START.lines().collect();
        let names = "<2026-11-05T22:40:00.000Z> <RequestLocationInventory> Player[Xy] requested inventory for Location[Stanton2_Orison]\n";
        let query = "<2026-11-05T22:40:01.000Z> <Query Inventory> Request[5] Inventory[1:Location:4711]\n";
        // Cut between every line and inside one: the name window and the unended line go on across the cuts.
        let parts = [
            format!("{}\n{}\n", lines[0], lines[1]),
            format!("{}\n{names}", lines[2]),
            query.to_string(),
            format!("{}\n{}", lines[3], &lines[4][..20]),
            format!("{}\n<2026-11-05T22:50:00.000Z> the end", &lines[4][20..]),
        ];
        std::fs::write(&log, "").unwrap();
        let mut whole = String::new();
        for part in &parts {
            append(&path, part);
            whole.push_str(part);
            let on = (crate::location::last_change(&path), crate::stock_journal::read_log(&path, None));
            // A copy under another name is read from the start.
            let fresh_path = dir.join(format!("Kopie-{}.log", whole.len()));
            std::fs::write(&fresh_path, &whole).unwrap();
            let fresh_path = fresh_path.to_string_lossy().into_owned();
            let fresh = (crate::location::last_change(&fresh_path), crate::stock_journal::read_log(&fresh_path, None));
            assert_eq!(on, fresh, "after {:?}", part);
            assert_eq!(on.1, crate::stock_journal::read_log_text(&whole, None));
        }
        let (change, pairs, since) = crate::location::last_change(&path).expect("a change");
        assert_eq!(change, ("0".to_string(), "4711".to_string()));
        assert_eq!(pairs, vec![("4711".to_string(), "Stanton2_Orison".to_string())]);
        assert_eq!(since, Some(1_793_917_680));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
