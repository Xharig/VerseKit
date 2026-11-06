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

//! The pictures of these checks are made here: lines of yellow and orange writing on a dark ground, as blocks of
//! letters or, under Windows, written with a font of the system into a picture in memory. No picture of a screen.

use std::sync::atomic::AtomicUsize;

use super::*;
use crate::refinery_scan::{Area, Reason};
use crate::screen_grab::GrabError;

/// Blue, green, red, alpha.
const GROUND: [u8; 4] = [28, 22, 20, 255];
const YELLOW: [u8; 4] = [60, 220, 255, 255];
const ORANGE: [u8; 4] = [30, 150, 255, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];

/// The size of the part grabbed from a game window of 2560 × 1440.
const PART: (i64, i64) = (1280, 576);

fn ground() -> Bitmap {
    Bitmap { width: PART.0, height: PART.1, raw: GROUND.repeat((PART.0 * PART.1) as usize) }
}

fn fill(picture: &mut Bitmap, (x, y, w, h): Area, color: [u8; 4]) {
    for row in y..y + h {
        for col in x..x + w {
            let at = ((row * picture.width + col) * 4) as usize;
            picture.raw[at..at + 4].copy_from_slice(&color);
        }
    }
}

/// A line of `letters` blocks six points wide and three apart, its right end at `right`.
fn line_of_blocks(picture: &mut Bitmap, right: i64, top: i64, height: i64, letters: i64, color: [u8; 4]) {
    for i in 0..letters {
        fill(picture, (right - (i + 1) * 9 + 3, top, 6, height), color);
    }
}

/// A block of three lines of writing at the right edge, as `r_DisplayInfo` draws it.
fn block() -> Bitmap {
    let mut picture = ground();
    line_of_blocks(&mut picture, PART.0 - 8, 10, 12, 28, YELLOW);
    line_of_blocks(&mut picture, PART.0 - 8, 30, 12, 30, ORANGE);
    line_of_blocks(&mut picture, PART.0 - 8, 50, 12, 32, YELLOW);
    picture
}

fn lines_of(texts: &[&str]) -> Vec<String> {
    texts.iter().map(|t| (*t).to_string()).collect()
}

#[test]
fn the_frame_rates_are_read_from_the_lines_of_the_block() {
    let block = lines_of(&[
        "CryEngine Info",
        "FPS 40.6 [26..47], - 25.7ms",
        "Server FPS 19.5 [18.4..20.5]",
        "ShardId: pub_euw1b 12881860 420",
    ]);
    assert_eq!(parse(&block), Some(Reading { fps: 40.6, server: Some(19.5), players: None }));
    // The line of the server first, letters read for digits and a comma for the point.
    let misread = lines_of(&["Server FPS 19,5 [18.4..20.5]", "FPS: 4O.6 [26..47]"]);
    assert_eq!(parse(&misread), Some(Reading { fps: 40.6, server: Some(19.5), players: None }));
    // The server alone is no reading; the line of the server never gives the own ones.
    assert_eq!(parse(&lines_of(&["Server FPS 19.5"])), None);
    // Values no game reaches do not count, a server of zero is no server.
    assert_eq!(parse(&lines_of(&["FPS 0"])), None);
    assert_eq!(parse(&lines_of(&["FPS 40", "Server FPS 0"])), Some(Reading { fps: 40.0, server: None, players: None }));
    // FPS inside another word is not the mark.
    assert_eq!(parse(&lines_of(&["SHOWFPS 40"])), None);
    assert_eq!(parse(&lines_of(&["nothing here"])), None);
}

#[test]
fn the_line_that_names_the_server_never_gives_its_frame_rate() {
    let rate = "Server FPS 16.4 [11.8..18.0] - 60.9ms [50.8..65.2], Hitches: 0";
    let name = "Server: ptu-use1b-sc-alpha-500-13119881-13121230-game-46, restarts 0, pop. [shard 432, srv 78]";
    let own = "FPS 17.2 [10..21] - 58.1ms";
    // Both lines: the rate is the one of its line, the players come from the other.
    let both = parse(&lines_of(&[own, rate, name])).expect("a reading");
    assert_eq!((both.fps, both.server), (17.2, Some(16.4)));
    assert_eq!(both.players, Some(Players { shard: 432, server: Some(78) }));
    // The line of the rate missed by the recognition, the name read as a rate (`Server 500`): no rate.
    for misread in ["Server 500 alpha-500 pop. [shard 432, srv 78]", "Server: alpha-500, restarts 0", "Server 500,0 pop."] {
        let reading = parse(&lines_of(&[own, misread])).expect("a reading");
        assert_eq!(reading.server, None, "{misread:?}");
    }
    // A rate no server reaches does not count; a line with `Server` and no `FPS` still does.
    assert_eq!(parse(&lines_of(&[own, "Server FPS 500,0"])).expect("a reading").server, None);
    assert_eq!(parse(&lines_of(&[own, "Server FPS 120"])).expect("a reading").server, Some(120.0));
    assert_eq!(parse(&lines_of(&[own, "Server 17.4"])).expect("a reading").server, Some(17.4));
    assert_eq!(parse(&lines_of(&[own, "Server: 17.4 [10..20]"])).expect("a reading").server, Some(17.4));
}

/// The players stand at the end of the line `Server: …` of level 3 (the lines of a picture of 1 November 2026);
/// the part read starts left of the yellow lines, so the line comes cut at its start.
#[test]
fn the_players_on_the_shard_and_the_server_are_read_from_the_server_line() {
    let block = lines_of(&[
        "FPS 60.0 [58..60], - 16.7ms [8.4ms]",
        "Session: 0e651342-902f-ce41-aa5b-8902be046577",
        "ShardId: pub_euw1b 12881860 120",
        "Server FPS 18.4 [15.3..19.7] - 54.2ms [50.8..65.2], Hitches: 0",
        "lpha-4102-12974691-game-77, restarts 0, pop. [shard 482, srv 37]",
        "Working Set (OS): 4.69 GiB Committed (OS): 29.55 GiB",
    ]);
    let players = Players { shard: 482, server: Some(37) };
    assert_eq!(parse(&block), Some(Reading { fps: 60.0, server: Some(18.4), players: Some(players) }));
    // Letters read for digits; the server cut off; the shard id is no count of players.
    let misread = lines_of(&["FPS 60", "restarts 0, pop. [shard 4O2, srv 3l]"]);
    assert_eq!(parse(&misread).and_then(|r| r.players), Some(Players { shard: 402, server: Some(31) }));
    let cut = lines_of(&["FPS 60", "pop. [shard 482"]);
    assert_eq!(parse(&cut).and_then(|r| r.players), Some(Players { shard: 482, server: None }));
    assert_eq!(parse(&lines_of(&["FPS 60", "ShardId: pub_euw1b 12881860 120"])).and_then(|r| r.players), None);
    // The recognition splits the line, reads the comma and `srv` otherwise: the next number still counts.
    let split = lines_of(&["FPS 60", "restarts 0, pop. [shard 482,", "sry 37]"]);
    assert_eq!(parse(&split).and_then(|r| r.players), Some(Players { shard: 482, server: Some(37) }));
    let odd_marks = lines_of(&["FPS 60", "pop. (shard 482. srv: 37)"]);
    assert_eq!(parse(&odd_marks).and_then(|r| r.players), Some(Players { shard: 482, server: Some(37) }));
    // What the recognition read in the game on 1 November 2026: `FPS` dropped from the line of the server, the
    // closing bracket read as a 1.
    let real = lines_of(&[
        "FPS 59.9 [ - 16.9ms [8.7n51",
        "Shardld: pub euwib 12881860 120",
        "Server 17.4 U5.8..L8.41 37.4rns Hitches: O",
        "restarts 0, pop. (shard 480, srv 461",
        "Net: ping 26.075, 65.048köps. 300vt L 30.867kb05",
        "Server-Time: Sun L 2026",
    ]);
    let read = parse(&real).expect("a reading");
    assert_eq!((read.fps, read.server), (59.9, Some(17.4)));
    assert_eq!(read.players, Some(Players { shard: 480, server: Some(46) }));
    // A look that misses a part keeps it from the last look that had it, while it is fresh; then it goes.
    let mut kept = Kept::default();
    let t0 = Instant::now();
    let full = kept.fill(read, t0);
    assert_eq!(full.players, Some(Players { shard: 480, server: Some(46) }));
    let missing = Reading { fps: 60.0, server: None, players: Some(Players { shard: 481, server: None }) };
    let filled = kept.fill(missing, t0 + Duration::from_secs(3));
    assert_eq!((filled.server, filled.players), (Some(17.4), Some(Players { shard: 481, server: Some(46) })));
    let late = kept.fill(Reading { fps: 60.0, server: None, players: None }, t0 + FRESH + Duration::from_secs(4));
    assert_eq!((late.server, late.players), (None, None), "too old: gone");
    // A bracket read as a bracket keeps the whole number.
    let kept = lines_of(&["FPS 60", "pop. [shard 480, srv 41]"]);
    assert_eq!(parse(&kept).and_then(|r| r.players), Some(Players { shard: 480, server: Some(41) }));
    // No more on a server than on its shard.
    let odd = lines_of(&["FPS 60", "pop. [shard 40, srv 370]"]);
    assert_eq!(parse(&odd).and_then(|r| r.players), Some(Players { shard: 40, server: None }));
}

#[test]
fn lines_count_only_where_yellow_or_orange_writing_ends_at_the_right_edge() {
    let found = lines(&block());
    let edges: Vec<(i64, i64)> = found.iter().map(|l| (l.top, l.bottom)).collect();
    assert_eq!(edges, [(10, 21), (30, 41), (50, 61)]);
    assert!(found.iter().all(|l| l.right == PART.0 - 9), "{found:?}");
    let job = job(&found, PART.0).expect("a block");
    assert_eq!(job.mode, Mode::Dark);
    assert_eq!(job.scale, 3.0, "letters 12 points high are enlarged to 36");
    let (x, y, w, h) = job.area;
    assert!(x <= found[2].left && y <= 10 && x + w == PART.0 && y + h > 61, "{:?}", job.area);

    // The same lines in white: no writing of the block.
    let mut white = ground();
    for top in [10, 30, 50] {
        line_of_blocks(&mut white, PART.0 - 8, top, 12, 28, WHITE);
    }
    assert!(lines(&white).is_empty());
    // Yellow lines on the left side: not the block.
    let mut left = ground();
    for top in [10, 30, 50] {
        line_of_blocks(&mut left, 400, top, 12, 28, YELLOW);
    }
    assert!(lines(&left).is_empty());
    // A yellow sky over the whole width is no writing.
    let mut sky = ground();
    fill(&mut sky, (0, 100, PART.0, 200), YELLOW);
    assert!(lines(&sky).is_empty());
    // One line alone is no block.
    let mut one = ground();
    line_of_blocks(&mut one, PART.0 - 8, 10, 12, 28, YELLOW);
    assert_eq!(lines(&one).len(), 1);
    assert_eq!(super::job(&lines(&one), PART.0), None);
}

/// A recognition that gives fixed words and counts how often it was asked.
struct Words {
    words: Result<Vec<Word>, Reason>,
    asked: Arc<AtomicUsize>,
    jobs: Arc<Mutex<Vec<Job>>>,
}

impl Words {
    fn new(words: Result<Vec<Word>, Reason>) -> (Words, Arc<AtomicUsize>, Arc<Mutex<Vec<Job>>>) {
        let (asked, jobs) = (Arc::new(AtomicUsize::new(0)), Arc::new(Mutex::new(Vec::new())));
        (Words { words, asked: asked.clone(), jobs: jobs.clone() }, asked, jobs)
    }
}

impl Reader for Words {
    fn read(&mut self, _: &Bitmap, jobs: &[Job]) -> Result<Vec<Vec<Word>>, ScanError> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        lock(&self.jobs).extend_from_slice(jobs);
        match &self.words {
            Ok(words) => Ok(vec![words.clone()]),
            Err(reason) => Err(ScanError::new(*reason, "")),
        }
    }
}

/// The words of the block as the recognition gives them, in points of the part.
fn block_words() -> Vec<Word> {
    vec![
        Word::new("FPS", 1000.0, 10.0, 30.0, 12.0),
        Word::new("40.6", 1035.0, 10.0, 40.0, 12.0),
        Word::new("[26..47],", 1080.0, 10.0, 80.0, 12.0),
        Word::new("Server", 980.0, 30.0, 50.0, 12.0),
        Word::new("FPS", 1035.0, 30.0, 30.0, 12.0),
        Word::new("19.5", 1070.0, 30.0, 40.0, 12.0),
    ]
}

#[test]
fn the_recognition_is_asked_only_where_a_block_stands() {
    let (mut reader, asked, jobs) = Words::new(Ok(block_words()));
    assert_eq!(look(&block(), &mut reader), Look::Read(Reading { fps: 40.6, server: Some(19.5), players: None }));
    assert_eq!(asked.load(Ordering::SeqCst), 1);
    assert_eq!(lock(&jobs).as_slice(), [super::job(&lines(&block()), PART.0).expect("a job")]);
    // Without the block nothing is asked and nothing is read.
    assert_eq!(look(&ground(), &mut reader), Look::Nothing);
    assert_eq!(asked.load(Ordering::SeqCst), 1);
    // A recognition that fails, or words without the frame rates: a block that could not be read.
    let (mut failing, _, _) = Words::new(Err(Reason::Ocr));
    assert_eq!(look(&block(), &mut failing), Look::Unread);
    let (mut other, _, _) = Words::new(Ok(vec![Word::new("Hallo", 1000.0, 10.0, 40.0, 12.0)]));
    assert_eq!(look(&block(), &mut other), Look::Unread);
}

#[test]
fn the_part_is_the_upper_right_corner_of_the_game_window() {
    assert_eq!(area((0, 0, 2560, 1440)), (1280, 0, 1280, 576));
    assert_eq!(area((100, 50, 1920, 1080)), (1060, 50, 960, 432));
}

/// A made-up screen: the game in front or not, the block there or not; counts the grabs.
#[derive(Clone)]
struct Screen {
    front: Arc<AtomicBool>,
    with_block: Arc<AtomicBool>,
    grabs: Arc<AtomicUsize>,
}

impl Source for Screen {
    fn game(&mut self) -> Option<crate::screen_grab::Area> {
        self.front.load(Ordering::SeqCst).then_some((0, 0, 2560, 1440))
    }

    fn grab(&mut self, part: crate::screen_grab::Area) -> Result<Vec<u8>, GrabError> {
        self.grabs.fetch_add(1, Ordering::SeqCst);
        assert_eq!(part, (1280, 0, 1280, 576), "only the corner is grabbed");
        Ok(if self.with_block.load(Ordering::SeqCst) { block() } else { ground() }.raw)
    }
}

fn wait_for(what: impl Fn() -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(5);
    while Instant::now() < end {
        if what() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

#[test]
fn the_watch_reads_only_while_the_game_is_in_front_and_forgets_a_block_that_went() {
    let screen = Screen {
        front: Arc::new(AtomicBool::new(false)),
        with_block: Arc::new(AtomicBool::new(true)),
        grabs: Arc::new(AtomicUsize::new(0)),
    };
    let (reader, _, _) = Words::new(Ok(block_words()));
    let woken = Arc::new(AtomicUsize::new(0));
    let wakes = woken.clone();
    let watch = Watch::start(Box::new(screen.clone()), Box::new(reader), Duration::from_millis(10), move || {
        wakes.fetch_add(1, Ordering::SeqCst);
    })
    .expect("the thread starts");
    // The game in the background: nothing is grabbed.
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(screen.grabs.load(Ordering::SeqCst), 0);
    assert_eq!(watch.reading(), None);
    screen.front.store(true, Ordering::SeqCst);
    assert!(wait_for(|| watch.reading().is_some()), "the block is read once the game is in front");
    assert_eq!(watch.reading(), Some(Reading { fps: 40.6, server: Some(19.5), players: None }));
    assert!(woken.load(Ordering::SeqCst) >= 1);
    // The player switched the block off in the game: the reading goes at once.
    screen.with_block.store(false, Ordering::SeqCst);
    assert!(wait_for(|| watch.reading().is_none()), "a block that went is not shown");
    assert!(watch.alive());
    watch.stop();
}

/// Lines of text written with a font of the system into a picture in memory of the size of the part, right-aligned
/// at the right edge, each in its colour (red, green, blue), `size` points high.
#[cfg(windows)]
fn written(texts: &[(&str, [u8; 3])], size: i32) -> Bitmap {
    use windows_sys::Win32::Foundation::SIZE;
    use windows_sys::Win32::Graphics::Gdi::{
        ANTIALIASED_QUALITY, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CLIP_DEFAULT_PRECIS, CreateCompatibleDC,
        CreateDIBSection, CreateFontW, DEFAULT_CHARSET, DIB_RGB_COLORS, DeleteDC, DeleteObject, FIXED_PITCH, GdiFlush,
        GetTextExtentPoint32W, OUT_DEFAULT_PRECIS, SelectObject, SetBkMode, SetTextColor, TRANSPARENT, TextOutW,
    };

    let (width, height) = (PART.0 as i32, PART.1 as i32);
    let face: Vec<u16> = "Consolas\0".encode_utf16().collect();
    // SAFETY: every handle made here is released before the end; the bits live as long as the bitmap.
    unsafe {
        let memory = CreateCompatibleDC(std::ptr::null_mut());
        assert!(!memory.is_null(), "a drawing context in memory");
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..std::mem::zeroed()
        };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let bitmap = CreateDIBSection(memory, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
        assert!(!bitmap.is_null() && !bits.is_null(), "a picture in memory");
        let length = (width * height * 4) as usize;
        std::slice::from_raw_parts_mut(bits.cast::<u8>(), length).copy_from_slice(&GROUND.repeat(length / 4));
        let old_bitmap = SelectObject(memory, bitmap);
        let font = CreateFontW(
            -size,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            u32::from(DEFAULT_CHARSET),
            u32::from(OUT_DEFAULT_PRECIS),
            u32::from(CLIP_DEFAULT_PRECIS),
            u32::from(ANTIALIASED_QUALITY),
            u32::from(FIXED_PITCH),
            face.as_ptr(),
        );
        assert!(!font.is_null(), "the font");
        let old_font = SelectObject(memory, font);
        SetBkMode(memory, TRANSPARENT as i32);
        for (i, (text, [r, g, b])) in texts.iter().enumerate() {
            let wide: Vec<u16> = text.encode_utf16().collect();
            let mut extent = SIZE { cx: 0, cy: 0 };
            GetTextExtentPoint32W(memory, wide.as_ptr(), wide.len() as i32, &mut extent);
            SetTextColor(memory, u32::from(*r) | (u32::from(*g) << 8) | (u32::from(*b) << 16));
            TextOutW(
                memory,
                width - 8 - extent.cx,
                10 + i as i32 * (size + size / 2),
                wide.as_ptr(),
                wide.len() as i32,
            );
        }
        GdiFlush();
        let raw = std::slice::from_raw_parts(bits.cast::<u8>(), length).to_vec();
        SelectObject(memory, old_font);
        SelectObject(memory, old_bitmap);
        DeleteObject(font);
        DeleteObject(bitmap);
        DeleteDC(memory);
        Bitmap { width: PART.0, height: PART.1, raw }
    }
}

#[cfg(windows)]
const DISPLAY_INFO: [(&str, [u8; 3]); 3] = [
    ("FPS 40.6 [26..47], - 25.7ms", [255, 220, 60]),
    ("Server FPS 19.5 [18.4..20.5]", [255, 150, 30]),
    ("ShardId: pub_euw1b 12881860 420", [255, 220, 60]),
];

#[cfg(windows)]
#[test]
fn a_block_written_in_a_font_of_the_system_is_found_line_by_line() {
    let picture = written(&DISPLAY_INFO, 16);
    let found = lines(&picture);
    assert_eq!(found.len(), 3, "{found:?}");
    assert!(found.windows(2).all(|pair| pair[0].bottom < pair[1].top), "{found:?}");
    assert!(found.iter().all(|l| l.right >= PART.0 - 16), "{found:?}");
    assert!(job(&found, PART.0).is_some());
    // The same text in white is not the block.
    let white: Vec<(&str, [u8; 3])> = DISPLAY_INFO.iter().map(|(text, _)| (*text, [255, 255, 255])).collect();
    assert!(lines(&written(&white, 16)).is_empty());
}

#[cfg(windows)]
#[test]
fn the_text_recognition_of_windows_reads_the_frame_rates_of_such_a_block() {
    let picture = written(&DISPLAY_INFO, 16);
    let job = job(&lines(&picture), PART.0).expect("a block");
    match System.read(&picture, std::slice::from_ref(&job)) {
        Err(e) if matches!(e.reason, Reason::NoLanguage | Reason::Unsupported) => {
            eprintln!("no text recognition on this system ({e}): the reading is not checked here");
        }
        Err(e) => panic!("the recognition failed: {e}"),
        Ok(words) => {
            let words: Vec<Word> = words.into_iter().flatten().collect();
            let text = text_lines(&words);
            assert_eq!(parse(&text), Some(Reading { fps: 40.6, server: Some(19.5), players: None }), "{text:?}");
            assert_eq!(look(&picture, &mut System), Look::Read(Reading { fps: 40.6, server: Some(19.5), players: None }));
            // Without the block the recognition is not asked and nothing is read.
            assert_eq!(look(&ground(), &mut System), Look::Nothing);
        }
    }
}
