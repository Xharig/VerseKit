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
//! over from it: the player asks for one, switches to the game, and the picture is taken the moment the game is
//! in front. Before that nothing is grabbed, so no other window ends up in the picture.

use std::time::{Duration, Instant};

use versekit_core::screen_grab::{Area, GrabError};
use versekit_core::signature_watch::Source;

/// The file in the folder `Intern`.
pub const FILE: &str = "signatur-spielbild.bin";
const MAGIC: &[u8; 4] = b"VKG1";
/// The longest side of the picture kept, in pixels: the models are at most a few hundred points wide.
pub const LONGEST: usize = 1280;
/// So long the game has to come to the front after the click.
pub const WAIT: Duration = Duration::from_secs(60);
/// So often it is asked whether the game is in front.
pub const LOOK_EVERY: Duration = Duration::from_millis(250);

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
        let temp = versekit_core::jsonio::temp_name(path);
        versekit_core::jsonio::write_new(&temp, &self.bytes()).is_ok()
            && versekit_core::jsonio::replace_file(&temp, path).is_ok()
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
}

/// Waits up to `wait` for the game to be in front, asking every `every`, and then grabs its window once.
pub fn fetch(source: &mut dyn Source, wait: Duration, every: Duration) -> Fetched {
    let start = Instant::now();
    loop {
        if let Some(game) = source.game() {
            return grab_once(source, game);
        }
        if start.elapsed() >= wait {
            return Fetched::NoGame;
        }
        std::thread::sleep(every);
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

    /// A made-up screen: the game comes to the front after `after` questions, its picture a colour per quarter.
    struct Made {
        game: Area,
        after: usize,
        asked: usize,
        grabbed: Vec<Area>,
    }

    impl Source for Made {
        fn game(&mut self) -> Option<Area> {
            self.asked += 1;
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
        let folder = versekit_core::testdir::fresh("spielbild-datei");
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

    #[test]
    fn the_fetch_waits_for_the_game_to_come_to_the_front_and_grabs_its_window_exactly_once() {
        let game = (2560, 0, 1600, 900);
        let mut screen = Made { game, after: 3, asked: 0, grabbed: Vec::new() };
        let fetched = fetch(&mut screen, Duration::from_secs(5), Duration::from_millis(1));
        let Fetched::Got(picture) = fetched else { panic!("{fetched:?}") };
        assert_eq!(picture.game, (1600, 900));
        assert_eq!(screen.grabbed, vec![game], "the whole game window, once, on the second screen");
        // The game never comes: nothing is grabbed.
        let mut away = Made { game, after: usize::MAX, asked: 0, grabbed: Vec::new() };
        assert_eq!(fetch(&mut away, Duration::from_millis(20), Duration::from_millis(1)), Fetched::NoGame);
        assert!(away.grabbed.is_empty());
    }
}
