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

//! The picture of the game behind the two models of the signature window: one grab of the whole game window,
//! the same way the scanner grabs (the window of the game while it is in front, in physical points), made
//! smaller and kept in one file of the folder `Intern` until the next one replaces it.
//!
//! The scanner itself never grabs the whole window (only the area it searches), so there is no picture to take
//! over from it: the player asks for one in the main window, switches to the game and has a chosen number of
//! seconds to switch the scanner on and scan; then the picture is taken as soon as the game is in front. Before
//! that nothing is grabbed, so no other window ends up in the picture. A hotkey of the overlay takes it at once
//! ([`take_now`]).

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::paths::Paths;
use crate::screen_grab::{Area, GrabError};
use crate::signature_watch::Source;

/// The file in the folder `Intern`.
pub const FILE: &str = "signatur-spielbild.bin";
const MAGIC: &[u8; 4] = b"VKG1";
/// The longest side of the picture kept, in pixels: the models are at most a few hundred points wide.
pub const LONGEST: usize = 1280;
/// So long the game has to come to the front once the chosen seconds are over.
pub const WAIT: Duration = Duration::from_secs(60);
/// So often it is asked whether the game is in front, and whether the fetch was stopped.
pub const LOOK_EVERY: Duration = Duration::from_millis(250);
/// The seconds the player can choose between the click and the picture.
pub const DELAYS: [i64; 4] = [5, 10, 20, 30];
/// The seconds while the setting is missing.
pub const DEFAULT_DELAY: i64 = 10;
/// The setting of the seconds.
pub const DELAY_SETTING: &str = "spielbild_wartezeit";

/// The seconds of the settings: missing or unreadable is [`DEFAULT_DELAY`], anything else the nearest of [`DELAYS`].
pub fn delay(paths: &Paths) -> i64 {
    nearest_delay(paths.setting_int(DELAY_SETTING, DEFAULT_DELAY, None, None))
}

/// The one of [`DELAYS`] nearest to `seconds`, the smaller one of two as near.
pub fn nearest_delay(seconds: i64) -> i64 {
    DELAYS.into_iter().min_by_key(|d| d.abs_diff(seconds)).unwrap_or(DEFAULT_DELAY)
}

/// The picture: RGB, top row first, and the size of the game window it was taken of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GamePicture {
    pub w: usize,
    pub h: usize,
    /// Width and height of the game window in physical points.
    pub game: (i32, i32),
    pub rgb: Vec<u8>,
}

impl GamePicture {
    /// The picture of a grab (BGRA, top row first) of a game window `game_w` × `game_h`, made smaller so that its
    /// longest side is at most [`LONGEST`]; each pixel is the mean of the points it stands for.
    pub fn from_bgra(raw: &[u8], game_w: i32, game_h: i32) -> Option<GamePicture> {
        let (gw, gh) = (usize::try_from(game_w).ok()?, usize::try_from(game_h).ok()?);
        if gw == 0 || gh == 0 || raw.len() != gw.checked_mul(gh)?.checked_mul(4)? {
            return None;
        }
        let longest = gw.max(gh);
        let (w, h) = if longest <= LONGEST {
            (gw, gh)
        } else {
            ((gw * LONGEST / longest).max(1), (gh * LONGEST / longest).max(1))
        };
        let mut rgb = Vec::with_capacity(w * h * 3);
        for oy in 0..h {
            let (y0, y1) = (oy * gh / h, ((oy + 1) * gh / h).max(oy * gh / h + 1));
            for ox in 0..w {
                let (x0, x1) = (ox * gw / w, ((ox + 1) * gw / w).max(ox * gw / w + 1));
                let mut sum = [0u64; 3];
                for y in y0..y1 {
                    for x in x0..x1 {
                        let at = (y * gw + x) * 4;
                        // BGRA: blue first.
                        sum[0] += u64::from(raw[at + 2]);
                        sum[1] += u64::from(raw[at + 1]);
                        sum[2] += u64::from(raw[at]);
                    }
                }
                let count = ((y1 - y0) * (x1 - x0)) as u64;
                rgb.extend(sum.map(|s| (s / count) as u8));
            }
        }
        Some(GamePicture { w, h, game: (game_w, game_h), rgb })
    }

    /// The bytes of the file: magic, width, height, width and height of the game window, then the RGB points.
    pub fn bytes(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        for number in [self.w as u32, self.h as u32, self.game.0 as u32, self.game.1 as u32] {
            out.extend_from_slice(&number.to_le_bytes());
        }
        out.extend_from_slice(&self.rgb);
        out
    }

    /// The picture from its bytes; `None` for anything that is not one.
    pub fn of(bytes: &[u8]) -> Option<GamePicture> {
        if bytes.get(..4)? != MAGIC {
            return None;
        }
        let number = |at: usize| -> Option<u32> { Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?)) };
        let (w, h) = (number(4)? as usize, number(8)? as usize);
        let game = (i32::try_from(number(12)?).ok()?, i32::try_from(number(16)?).ok()?);
        let rgb = bytes.get(20..)?;
        if w == 0 || h == 0 || game.0 <= 0 || game.1 <= 0 || rgb.len() != w.checked_mul(h)?.checked_mul(3)? {
            return None;
        }
        Some(GamePicture { w, h, game, rgb: rgb.to_vec() })
    }

    /// Keeps the picture in `path`, replacing the one before without ever leaving a half-written file.
    pub fn write(&self, path: &str) -> bool {
        let temp = crate::jsonio::temp_name(path);
        crate::jsonio::write_new(&temp, &self.bytes()).is_ok() && crate::jsonio::replace_file(&temp, path).is_ok()
    }

    /// The picture kept in `path`, if there is one.
    pub fn read(path: &str) -> Option<GamePicture> {
        GamePicture::of(&std::fs::read(path).ok()?)
    }
}

/// What a fetch came to.
#[derive(Debug, PartialEq, Eq)]
pub enum Fetched {
    Got(GamePicture),
    /// The game did not come to the front in time.
    NoGame,
    /// The game was in front, but the screen could not be read.
    Failed(GrabError),
    /// The player stopped the fetch before the picture was taken.
    Stopped,
}

/// Lets `delay` pass, then waits up to `wait` more for the game to be in front, asking every `every`, and grabs
/// its window once. Nothing is grabbed before `delay` is over. `stop` ends it at the next look.
pub fn fetch(source: &mut dyn Source, delay: Duration, wait: Duration, every: Duration, stop: &AtomicBool) -> Fetched {
    let start = Instant::now();
    loop {
        if stop.load(Ordering::Relaxed) {
            return Fetched::Stopped;
        }
        let passed = start.elapsed();
        if passed < delay {
            std::thread::sleep(every.min(delay - passed));
            continue;
        }
        if let Some(game) = source.game() {
            return grab_once(source, game);
        }
        if passed >= delay + wait {
            return Fetched::NoGame;
        }
        std::thread::sleep(every);
    }
}

/// Grabs the game window at once, where the game is in front (the hotkey of the overlay), and keeps the picture in
/// `file`; a picture that cannot be written counts as failed.
pub fn take_now(source: &mut dyn Source, file: &str) -> Fetched {
    let Some(game) = source.game() else { return Fetched::NoGame };
    keep(grab_once(source, game), file)
}

/// A picture fetched goes into `file`; one that cannot be written counts as failed.
pub fn keep(fetched: Fetched, file: &str) -> Fetched {
    match fetched {
        Fetched::Got(picture) if !picture.write(file) => Fetched::Failed(GrabError::Failed),
        other => other,
    }
}

fn grab_once(source: &mut dyn Source, game: Area) -> Fetched {
    match source.grab(game) {
        Ok(raw) => {
            GamePicture::from_bgra(&raw, game.2, game.3).map_or(Fetched::Failed(GrabError::Failed), Fetched::Got)
        }
        Err(error) => Fetched::Failed(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A made-up screen: the game comes to the front after `after` questions, its picture a colour per quarter. It
    /// writes down when it was asked for the game.
    struct Made {
        game: Area,
        after: usize,
        asked: usize,
        grabbed: Vec<Area>,
        looked: Vec<Instant>,
    }

    impl Source for Made {
        fn game(&mut self) -> Option<Area> {
            self.asked += 1;
            self.looked.push(Instant::now());
            (self.asked > self.after).then_some(self.game)
        }
        fn grab(&mut self, area: Area) -> Result<Vec<u8>, GrabError> {
            self.grabbed.push(area);
            Ok(quarters(area.2, area.3))
        }
    }

    /// BGRA with red left top, green right top, blue left bottom, white right bottom.
    fn quarters(w: i32, h: i32) -> Vec<u8> {
        let mut out = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let bgra = match (x < w / 2, y < h / 2) {
                    (true, true) => [0, 0, 255, 255],
                    (false, true) => [0, 255, 0, 255],
                    (true, false) => [255, 0, 0, 255],
                    (false, false) => [255, 255, 255, 255],
                };
                out.extend(bgra);
            }
        }
        out
    }

    fn pixel(picture: &GamePicture, x: usize, y: usize) -> [u8; 3] {
        let at = (y * picture.w + x) * 3;
        [picture.rgb[at], picture.rgb[at + 1], picture.rgb[at + 2]]
    }

    #[test]
    fn a_wide_game_window_is_made_smaller_in_its_own_shape_and_the_colours_stay_where_they_were() {
        let picture = GamePicture::from_bgra(&quarters(3440, 1440), 3440, 1440).expect("a picture");
        assert_eq!((picture.w, picture.h), (LONGEST, 1440 * LONGEST / 3440));
        assert_eq!(picture.game, (3440, 1440));
        assert_eq!(pixel(&picture, 10, 10), [255, 0, 0], "red stays left top");
        assert_eq!(pixel(&picture, picture.w - 10, 10), [0, 255, 0]);
        assert_eq!(pixel(&picture, 10, picture.h - 10), [0, 0, 255]);
        assert_eq!(pixel(&picture, picture.w - 10, picture.h - 10), [255, 255, 255]);
        // A small window keeps its size; a grab of the wrong length is no picture.
        let small = GamePicture::from_bgra(&quarters(800, 600), 800, 600).expect("small");
        assert_eq!((small.w, small.h), (800, 600));
        assert_eq!(GamePicture::from_bgra(&quarters(800, 600), 800, 601), None);
    }

    #[test]
    fn the_picture_goes_to_its_file_and_comes_back_and_a_strange_file_is_no_picture() {
        let folder = crate::testdir::fresh("spielbild-datei");
        let path = folder.join(FILE).to_string_lossy().into_owned();
        assert_eq!(GamePicture::read(&path), None, "no file yet");
        let picture = GamePicture::from_bgra(&quarters(1920, 1080), 1920, 1080).expect("a picture");
        assert!(picture.write(&path));
        assert_eq!(GamePicture::read(&path), Some(picture.clone()));
        // The next one replaces it.
        let next = GamePicture::from_bgra(&quarters(2560, 1080), 2560, 1080).expect("the next");
        assert!(next.write(&path));
        assert_eq!(GamePicture::read(&path).map(|p| p.game), Some((2560, 1080)));
        let mut cut = picture.bytes();
        cut.pop();
        assert_eq!(GamePicture::of(&cut), None, "one byte short");
        assert_eq!(GamePicture::of(b"VKS2 something else"), None);
    }

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn the_fetch_waits_for_the_game_to_come_to_the_front_and_grabs_its_window_exactly_once() {
        let game = (2560, 0, 1600, 900);
        let go = AtomicBool::new(false);
        let mut screen = Made { game, after: 3, asked: 0, grabbed: Vec::new(), looked: Vec::new() };
        let fetched = fetch(&mut screen, Duration::ZERO, Duration::from_secs(5), MS(1), &go);
        let Fetched::Got(picture) = fetched else { panic!("{fetched:?}") };
        assert_eq!(picture.game, (1600, 900));
        assert_eq!(screen.grabbed, vec![game], "the whole game window, once, on the second screen");
        // The game never comes: nothing is grabbed.
        let mut away = Made { game, after: usize::MAX, asked: 0, grabbed: Vec::new(), looked: Vec::new() };
        assert_eq!(fetch(&mut away, Duration::ZERO, MS(20), MS(1), &go), Fetched::NoGame);
        assert!(away.grabbed.is_empty());
    }

    /// The game in front at once is not grabbed before the chosen seconds are over: the player still switches the
    /// scanner on and scans.
    #[test]
    fn the_fetch_lets_the_chosen_time_pass_before_it_looks_at_the_game() {
        let game = (0, 0, 1600, 900);
        let go = AtomicBool::new(false);
        let mut screen = Made { game, after: 0, asked: 0, grabbed: Vec::new(), looked: Vec::new() };
        let start = Instant::now();
        let fetched = fetch(&mut screen, MS(120), Duration::from_secs(5), MS(10), &go);
        assert!(matches!(fetched, Fetched::Got(_)), "{fetched:?}");
        // When the game was asked for, not how long the whole fetch took: making the picture smaller takes long
        // in a debug build by itself.
        let first = screen.looked.first().copied().expect("asked for the game");
        assert!(first.duration_since(start) >= MS(120), "asked before the time: {:?}", first.duration_since(start));
        assert_eq!(screen.asked, 1, "the game is asked for only once the time is over");
        assert_eq!(screen.grabbed, vec![game]);
        // The time over, the game not in front: it is waited for, at most `wait`.
        let mut away = Made { game, after: usize::MAX, asked: 0, grabbed: Vec::new(), looked: Vec::new() };
        let start = Instant::now();
        assert_eq!(fetch(&mut away, MS(40), MS(40), MS(5), &go), Fetched::NoGame);
        assert!(start.elapsed() >= MS(80), "{:?}", start.elapsed());
        let first = away.looked.first().copied().expect("asked for the game");
        assert!(first.duration_since(start) >= MS(40), "asked before the time: {:?}", first.duration_since(start));
        assert!(away.grabbed.is_empty());
    }

    /// Stopped while the time runs, or while it waits for the game: it ends at the next look and grabs nothing.
    #[test]
    fn a_stopped_fetch_ends_at_once_and_grabs_nothing() {
        let game = (0, 0, 1600, 900);
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let stopper = stop.clone();
        let late = std::thread::spawn(move || {
            std::thread::sleep(MS(30));
            stopper.store(true, Ordering::Relaxed);
        });
        let mut screen = Made { game, after: 0, asked: 0, grabbed: Vec::new(), looked: Vec::new() };
        let start = Instant::now();
        assert_eq!(fetch(&mut screen, Duration::from_secs(30), WAIT, MS(5), &stop), Fetched::Stopped);
        assert!(start.elapsed() < Duration::from_secs(10), "{:?}", start.elapsed());
        assert!(screen.grabbed.is_empty());
        late.join().unwrap();
        let mut away = Made { game, after: usize::MAX, asked: 0, grabbed: Vec::new(), looked: Vec::new() };
        assert_eq!(fetch(&mut away, Duration::ZERO, WAIT, MS(5), &stop), Fetched::Stopped, "stopped before");
    }

    /// The hotkey: the game in front is grabbed at once and kept in the file; without the game nothing is grabbed and
    /// the picture before stays.
    #[test]
    fn take_now_grabs_the_game_in_front_at_once_and_keeps_it() {
        let folder = crate::testdir::fresh("spielbild-sofort");
        let file = folder.join(FILE).to_string_lossy().into_owned();
        let game = (0, 0, 1920, 1080);
        let mut screen = Made { game, after: 0, asked: 0, grabbed: Vec::new(), looked: Vec::new() };
        let Fetched::Got(picture) = take_now(&mut screen, &file) else { panic!("a picture") };
        assert_eq!(screen.grabbed, vec![game]);
        assert_eq!(GamePicture::read(&file), Some(picture));
        let mut away = Made { game: (0, 0, 800, 600), after: usize::MAX, asked: 0, grabbed: Vec::new(), looked: Vec::new() };
        assert_eq!(take_now(&mut away, &file), Fetched::NoGame);
        assert_eq!(away.asked, 1, "asked once, not waited for");
        assert_eq!(GamePicture::read(&file).map(|p| p.game), Some((1920, 1080)), "the picture before stays");
        // A file that cannot be written: failed.
        let blocked = folder.join("ordner").join("gibt-es-nicht").join(FILE).to_string_lossy().into_owned();
        let mut screen = Made { game, after: 0, asked: 0, grabbed: Vec::new(), looked: Vec::new() };
        assert_eq!(take_now(&mut screen, &blocked), Fetched::Failed(GrabError::Failed));
    }

    #[test]
    fn the_seconds_of_the_settings_are_one_of_the_choices_and_ten_without_a_setting() {
        assert_eq!(nearest_delay(10), 10);
        assert_eq!(nearest_delay(0), 5);
        assert_eq!(nearest_delay(14), 10);
        assert_eq!(nearest_delay(15), 10, "the smaller of two as near");
        assert_eq!(nearest_delay(16), 20);
        assert_eq!(nearest_delay(999), 30);
        use crate::paths::Host;
        use crate::pypath::Flavor;
        let dir = crate::testdir::fresh("spielbild-wartezeit");
        let home = dir.to_string_lossy().into_owned();
        let paths = Paths::new(Host::new(Flavor::Posix, &[("HOME", &home)], &home));
        assert_eq!(delay(&paths), DEFAULT_DELAY);
        paths.set_setting(DELAY_SETTING, crate::jsonio::Value::int(30));
        assert_eq!(delay(&paths), 30);
        paths.set_setting(DELAY_SETTING, crate::jsonio::Value::str("lange"));
        assert_eq!(delay(&paths), DEFAULT_DELAY);
    }
}
