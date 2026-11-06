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

//! What the screens say before a window exists: the scaling of the system
//! and whether a remembered window place still lies on a screen.
//!
//! All places are in screen pixels. Windows asks the system for each answer.
//! Elsewhere the screens are read into a [`Setup`] (macOS: `NSScreen`, X11:
//! RandR and the work area of the window manager) and kept for a second.
//! There "pixels" are the points of the system times the scale of the main
//! screen, so that the point-to-pixel conversion of the windows stays one
//! number. Wayland tells no places and lets no window be placed: it gives
//! an empty answer, as before.

use std::cell::RefCell;

/// Screen pixels per point of the system (Windows scaling, 1.25 at 125 %);
/// 1.0 where it cannot be asked.
#[cfg(windows)]
pub fn system_scale() -> f32 {
    if let Some(setup) = fixed_setup() {
        return setup.scale;
    }
    // SAFETY: GetDpiForSystem takes nothing and only returns a number.
    let dpi = unsafe { windows_sys::Win32::UI::HiDpi::GetDpiForSystem() };
    if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 }
}

#[cfg(not(windows))]
pub fn system_scale() -> f32 {
    current().scale
}

/// A rectangle on the screens in screen pixels: left, top, width, height.
pub type Area = (i32, i32, i32, i32);

/// One screen: all of it, and its work area without task bar, menu bar and dock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Monitor {
    pub area: Area,
    pub work: Area,
}

/// The screens of the system in one look.
#[derive(Clone, Debug, PartialEq)]
pub struct Setup {
    /// Screen pixels per point of the system.
    pub scale: f32,
    /// Index of the main screen in `monitors`.
    pub primary: usize,
    pub monitors: Vec<Monitor>,
}

impl Default for Setup {
    /// Nothing known: no screens, scale 1.0.
    fn default() -> Setup {
        Setup { scale: 1.0, primary: 0, monitors: Vec::new() }
    }
}

impl Setup {
    /// All screens; empty where they cannot be asked.
    pub fn screens(&self) -> Vec<Area> {
        self.monitors.iter().map(|m| m.area).collect()
    }

    /// The screen (`work = false`) or its work area (`work = true`) of the
    /// monitor that holds `x`, `y`, or of the nearest one.
    pub fn monitor_area(&self, x: i32, y: i32, work: bool) -> Option<Area> {
        let nearest = self.monitors.iter().min_by_key(|m| distance(m.area, x, y))?;
        Some(if work { nearest.work } else { nearest.area })
    }

    /// The main screen or its work area.
    pub fn primary_area(&self, work: bool) -> Option<Area> {
        let main = self.monitors.get(self.primary).or_else(|| self.monitors.first())?;
        Some(if work { main.work } else { main.area })
    }
}

/// How far the point lies from the rectangle, squared (0 inside it, right
/// and bottom edge excluded): the nearest monitor is the one with the least.
fn distance((ax, ay, aw, ah): Area, x: i32, y: i32) -> i128 {
    let gap = |from: i32, size: i32, at: i32| -> i128 {
        let last = i128::from(from) + i128::from(size.max(1)) - 1;
        let at = i128::from(at);
        if at < i128::from(from) {
            i128::from(from) - at
        } else if at > last {
            at - last
        } else {
            0
        }
    };
    let (dx, dy) = (gap(ax, aw, x), gap(ay, ah, y));
    dx * dx + dy * dy
}

/// A screen in the Cocoa way, `x`, `y` of the lower left corner with the
/// y axis upwards, `w`, `h` in points, as a pixel [`Area`] with the y axis
/// downwards from the top of the main screen (`primary_height` points high).
pub fn from_cocoa((x, y, w, h): (f64, f64, f64, f64), primary_height: f64, scale: f64) -> Area {
    let px = |v: f64| (v * scale).round() as i32;
    (px(x), px(primary_height - (y + h)), px(w), px(h))
}

/// A point in the Cocoa way (y upwards from the lower left corner of the
/// main screen, `primary_height` points high) as a pixel place.
pub fn point_from_cocoa((x, y): (f64, f64), primary_height: f64, scale: f64) -> (i32, i32) {
    ((x * scale).round() as i32, ((primary_height - y) * scale).round() as i32)
}

/// The part of `monitor` that lies inside the work area of the window
/// manager, which is given for all screens together (`_NET_WORKAREA`). A
/// work area that leaves out less than half of the screen is not believed.
pub fn work_inside(monitor: Area, desktop_work: Area) -> Area {
    let left = monitor.0.max(desktop_work.0);
    let top = monitor.1.max(desktop_work.1);
    let right = monitor.0.saturating_add(monitor.2).min(desktop_work.0.saturating_add(desktop_work.2));
    let bottom = monitor.1.saturating_add(monitor.3).min(desktop_work.1.saturating_add(desktop_work.3));
    let (w, h) = (right - left, bottom - top);
    if w > 0 && h > 0 && i64::from(w) * i64::from(h) * 2 >= i64::from(monitor.2) * i64::from(monitor.3) {
        (left, top, w, h)
    } else {
        monitor
    }
}

/// The scale an X11 window gets, as winit decides it: `WINIT_X11_SCALE_FACTOR`
/// if it is a number, otherwise `Xft.dpi` / 96, otherwise 1.0.
pub fn x11_scale(env: Option<&str>, xft_dpi: Option<&str>) -> f32 {
    let number = |s: &str| s.trim().parse::<f64>().ok().filter(|v| v.is_finite() && *v > 0.0);
    let scale = env.and_then(number).or_else(|| xft_dpi.and_then(number).map(|dpi| dpi / 96.0)).unwrap_or(1.0);
    scale.clamp(0.5, 8.0) as f32
}

/// The window system the windows of the program open on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    X11,
    Wayland,
    /// Neither is there.
    None,
}

/// The window system for `DISPLAY`, `WAYLAND_DISPLAY` and `XDG_SESSION_TYPE`: X11 wherever an X server
/// is there, under a Wayland session too (XWayland), as only X11 lets a window be placed, kept on top and
/// grab a hotkey; Wayland without one. Empty values count as not set.
pub fn choose_backend(display: Option<&str>, wayland_display: Option<&str>, session_type: Option<&str>) -> Backend {
    let set = |v: Option<&str>| v.is_some_and(|v| !v.is_empty());
    if set(display) {
        Backend::X11
    } else if set(wayland_display) || session_type.is_some_and(|s| s.trim().eq_ignore_ascii_case("wayland")) {
        Backend::Wayland
    } else {
        Backend::None
    }
}

/// The window system of this process, as [`choose_backend`] decides it from the environment.
pub fn backend() -> Backend {
    let var = |name: &str| std::env::var(name).ok();
    choose_backend(var("DISPLAY").as_deref(), var("WAYLAND_DISPLAY").as_deref(), var("XDG_SESSION_TYPE").as_deref())
}

/// Is the window system X11? The windows open on X11 wherever [`choose_backend`] says so; Wayland
/// places no window by coordinates.
pub fn x11_in_use(display: Option<&str>, wayland: Option<&str>, session: Option<&str>) -> bool {
    choose_backend(display, wayland, session) == Backend::X11
}

/// Does the strip a window is grabbed at (`x`, `y`, `width` wide) lie on
/// one of `screens`? At least 40 pixels of it (or its whole width) and 10
/// pixels of its height must be on one screen (`screen.title_visible`).
pub fn title_visible(x: i32, y: i32, width: i32, screens: &[Area]) -> bool {
    screens.iter().any(|&(sx, sy, sw, sh)| {
        let wide = x.saturating_add(width).min(sx.saturating_add(sw)) - x.max(sx);
        let high = y.saturating_add(20).min(sy.saturating_add(sh)) - y.max(sy);
        wide >= width.min(40) && high >= 10
    })
}

/// Where a window of `size` (screen pixels) opens: the rectangle (x, y, width, height) in screen pixels, wholly in the
/// work area of one screen. With a remembered place `at` it is the screen that holds it, or the nearest one when it lies
/// on none (a screen went away, a place above a smaller screen), and the window is moved only as far as it needs to lie
/// in that work area, its top edge first; without one it stands in the middle of the main screen. A window larger than
/// the work area shrinks to it. Everything is in the pixels of the whole desktop, so every screen counts with its own
/// scaling and no single scale comes in. `nearest` gives the work area of the screen that holds or is nearest to a
/// point, `primary` the one of the main screen; `None` where the screens cannot be asked.
pub fn plan_window(
    at: Option<(i32, i32)>,
    size: (i32, i32),
    nearest: &dyn Fn(i32, i32) -> Option<Area>,
    primary: Option<Area>,
) -> Option<Area> {
    let work = match at {
        Some((x, y)) => nearest(x, y)?,
        None => primary?,
    };
    let (wx, wy, ww, wh) = work;
    if ww <= 0 || wh <= 0 {
        return None;
    }
    let (w, h) = (size.0.clamp(1, ww), size.1.clamp(1, wh));
    let (x, y) = match at {
        Some((x, y)) => (x.clamp(wx, wx + ww - w), y.clamp(wy, wy + wh - h)),
        None => (wx + (ww - w) / 2, wy + (wh - h) / 2),
    };
    Some((x, y, w, h))
}

/// Puts the window `hwnd` at `area` (screen pixels of the whole desktop); with `size` also its size, otherwise only its
/// place. Whether Windows did it. Not on other systems.
#[cfg(windows)]
pub fn place_window(hwnd: isize, area: Area, size: bool) -> bool {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
    };
    let (x, y, w, h) = area;
    let flags = SWP_NOZORDER | SWP_NOACTIVATE | if size { 0 } else { SWP_NOSIZE };
    // SAFETY: a window handle of this process and plain numbers; Windows checks the handle.
    unsafe { SetWindowPos(hwnd as HWND, std::ptr::null_mut(), x, y, w, h, flags) != 0 }
}

#[cfg(not(windows))]
pub fn place_window(_hwnd: isize, _area: Area, _size: bool) -> bool {
    false
}

// ------------------------------------------------------------ fixed screens

thread_local! {
    static FIXED: RefCell<Option<Setup>> = const { RefCell::new(None) };
}

/// Answers for the screens given instead of the system's, in this thread
/// only, until the guard is dropped. For tests with dummy screens (several
/// monitors, negative places, other scales) and for previews.
#[must_use = "the screens are the system's again once this is dropped"]
pub struct Fixed(Option<Setup>);

pub fn fixed(setup: Setup) -> Fixed {
    Fixed(FIXED.with(|f| f.replace(Some(setup))))
}

impl Drop for Fixed {
    fn drop(&mut self) {
        FIXED.with(|f| *f.borrow_mut() = self.0.take());
    }
}

fn fixed_setup() -> Option<Setup> {
    FIXED.with(|f| f.borrow().clone())
}

// ------------------------------------------------------------ the system

/// All screens there are now; empty where they cannot be asked.
#[cfg(windows)]
pub fn screens() -> Vec<Area> {
    use windows_sys::Win32::Foundation::{LPARAM, RECT};
    use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};
    if let Some(setup) = fixed_setup() {
        return setup.screens();
    }
    unsafe extern "system" fn collect(_m: HMONITOR, _dc: HDC, rect: *mut RECT, data: LPARAM) -> i32 {
        // SAFETY: Windows passes a valid rectangle; `data` is the vector below.
        let (r, list) = unsafe { (&*rect, &mut *(data as *mut Vec<Area>)) };
        if r.right > r.left && r.bottom > r.top {
            list.push((r.left, r.top, r.right - r.left, r.bottom - r.top));
        }
        1
    }
    let mut list: Vec<Area> = Vec::new();
    // SAFETY: the vector lives for the call and is only touched in `collect`.
    unsafe {
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            Some(collect),
            &mut list as *mut Vec<Area> as LPARAM,
        )
    };
    list
}

#[cfg(not(windows))]
pub fn screens() -> Vec<Area> {
    current().screens()
}

/// Does the strip a window is grabbed at, `x`, `y`, `width` wide (screen
/// pixels), lie on a screen that is there now? Where the screens cannot be
/// asked: yes.
pub fn on_a_screen(x: i32, y: i32, width: i32) -> bool {
    let all = screens();
    all.is_empty() || title_visible(x, y, width, &all)
}

/// The screen (`work = false`) or its work area without the task bar
/// (`work = true`) of the monitor that holds `x`, `y`, or of the nearest one.
#[cfg(windows)]
pub fn monitor_area(x: i32, y: i32, work: bool) -> Option<Area> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint};
    if let Some(setup) = fixed_setup() {
        return setup.monitor_area(x, y, work);
    }
    // SAFETY: the point is passed by value; the result is checked for null.
    let monitor = unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST) };
    if monitor.is_null() {
        return None;
    }
    let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    // SAFETY: `info` lives for the call and carries its size as Windows asks.
    if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        return None;
    }
    let r = if work { info.rcWork } else { info.rcMonitor };
    (r.right > r.left && r.bottom > r.top).then_some((r.left, r.top, r.right - r.left, r.bottom - r.top))
}

#[cfg(not(windows))]
pub fn monitor_area(x: i32, y: i32, work: bool) -> Option<Area> {
    current().monitor_area(x, y, work)
}

/// The main screen (`work = false`) or its work area. Windows: the screen
/// that holds the point 0, 0; dummy screens: the one marked, as elsewhere.
#[cfg(windows)]
pub fn primary_area(work: bool) -> Option<Area> {
    if let Some(setup) = fixed_setup() {
        return setup.primary_area(work);
    }
    monitor_area(0, 0, work)
}

#[cfg(not(windows))]
pub fn primary_area(work: bool) -> Option<Area> {
    current().primary_area(work)
}

/// Where the pointer is now (screen pixels), if the system tells; only
/// elsewhere than Windows, whose pointer comes with its visibility.
#[cfg(target_os = "macos")]
pub fn pointer_position() -> Option<(i32, i32)> {
    if fixed_setup().is_some() {
        return None;
    }
    mac::pointer(&current())
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn pointer_position() -> Option<(i32, i32)> {
    if fixed_setup().is_some() {
        return None;
    }
    x11::pointer()
}

#[cfg(not(any(windows, unix)))]
pub fn pointer_position() -> Option<(i32, i32)> {
    None
}

// ------------------------------------------------------------ not Windows

/// How long a look at the screens counts.
#[cfg(not(windows))]
const FRESH: std::time::Duration = std::time::Duration::from_secs(1);

#[cfg(not(windows))]
struct Looked {
    at: std::time::Instant,
    setup: Option<Setup>,
}

#[cfg(not(windows))]
static LOOKED: std::sync::Mutex<Option<Looked>> = std::sync::Mutex::new(None);

/// The screens now: the fixed ones, else what the system says, looked at
/// at most once a second. A look that fails (macOS asks its screens in the
/// main thread only) leaves the last answer in place.
#[cfg(not(windows))]
pub fn current() -> Setup {
    if let Some(setup) = fixed_setup() {
        return setup;
    }
    let mut looked = LOOKED.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(l) = looked.as_ref()
        && l.at.elapsed() < FRESH
    {
        return l.setup.clone().unwrap_or_default();
    }
    match ask() {
        Some(setup) => {
            *looked = Some(Looked { at: std::time::Instant::now(), setup: Some(setup.clone()) });
            setup
        }
        // Nothing to ask here (another thread, no X server): the last answer stays, and its age
        // too, so that a thread that can ask will. Without one, the failed look counts.
        None => match looked.as_ref().and_then(|l| l.setup.clone()) {
            Some(kept) => kept,
            None => {
                *looked = Some(Looked { at: std::time::Instant::now(), setup: None });
                Setup::default()
            }
        },
    }
}

#[cfg(target_os = "macos")]
fn ask() -> Option<Setup> {
    mac::query()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn ask() -> Option<Setup> {
    x11::query()
}

#[cfg(not(any(windows, unix)))]
fn ask() -> Option<Setup> {
    None
}

#[cfg(target_os = "macos")]
mod mac;
// Built on macOS too, so that the code is type checked there.
#[cfg(unix)]
#[cfg_attr(target_os = "macos", allow(dead_code))]
mod x11;

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(area: Area) -> Monitor {
        Monitor { area, work: area }
    }

    #[cfg(windows)]
    #[test]
    fn scale_and_screens() {
        let scale = system_scale();
        assert!((1.0..=4.0).contains(&scale), "{scale}");
        assert!(!on_a_screen(-1_000_000, -1_000_000, 800), "far away is on no screen");
    }

    #[test]
    fn the_grab_strip_must_lie_well_on_a_screen() {
        let one = [(0, 0, 1920, 1080)];
        assert!(title_visible(100, 50, 541, &one));
        // 6 pixels on the screen are not enough to grab it.
        assert!(!title_visible(-535, 15, 541, &one));
        assert!(title_visible(-500, 15, 541, &one), "41 pixels are");
        // A narrow window needs its whole width.
        assert!(title_visible(0, 0, 30, &one));
        // Height: 10 of the 20 pixels below the top edge.
        assert!(!title_visible(100, 1071, 300, &one));
        assert!(title_visible(100, 1070, 300, &one));
        // A second screen to the left takes the negative place.
        let two = [(0, 0, 1920, 1080), (-1920, 0, 1920, 1080)];
        assert!(title_visible(-571, 15, 541, &two));
        assert!(!title_visible(-30_000, 15, 541, &two));
    }

    #[test]
    fn fixed_screens_answer_in_place_of_the_system_and_only_in_their_thread() {
        let setup = Setup {
            scale: 2.0,
            primary: 1,
            monitors: vec![
                monitor((-2560, -200, 2560, 1440)),
                Monitor { area: (0, 0, 3024, 1964), work: (0, 76, 3024, 1888) },
            ],
        };
        {
            let _screens = fixed(setup.clone());
            assert_eq!(system_scale(), 2.0);
            assert_eq!(screens(), vec![(-2560, -200, 2560, 1440), (0, 0, 3024, 1964)]);
            assert_eq!(primary_area(true), Some((0, 76, 3024, 1888)));
            assert_eq!(primary_area(false), Some((0, 0, 3024, 1964)));
            assert_eq!(monitor_area(-100, 50, true), Some((-2560, -200, 2560, 1440)));
            assert!(on_a_screen(-2000, 0, 541));
            assert!(!on_a_screen(-9000, 0, 541));
            // Another thread asks the system.
            std::thread::spawn(|| assert_ne!(screens().first(), Some(&(-2560, -200, 2560, 1440))))
                .join()
                .expect("thread");
        }
        assert_ne!(screens().first(), Some(&(-2560, -200, 2560, 1440)), "the system again");
    }

    #[cfg(windows)]
    #[test]
    fn the_work_area_lies_inside_its_screen() {
        let screen = primary_area(false).expect("a main screen");
        let work = primary_area(true).expect("its work area");
        assert!(work.0 >= screen.0 && work.1 >= screen.1, "{work:?} in {screen:?}");
        assert!(work.0 + work.2 <= screen.0 + screen.2 && work.1 + work.3 <= screen.1 + screen.3);
        // Far away the nearest monitor answers, never nothing.
        assert!(monitor_area(-1_000_000, -1_000_000, true).is_some());
    }

    #[test]
    fn fixed_screens_nest_and_come_back() {
        let outer = fixed(Setup { monitors: vec![monitor((0, 0, 100, 100))], ..Setup::default() });
        {
            let _inner = fixed(Setup { monitors: vec![monitor((0, 0, 200, 200))], ..Setup::default() });
            assert_eq!(screens(), vec![(0, 0, 200, 200)]);
        }
        assert_eq!(screens(), vec![(0, 0, 100, 100)]);
        drop(outer);
    }

    #[test]
    fn no_screens_known_means_every_place_counts_as_on_a_screen() {
        let none = Setup::default();
        let _screens = fixed(none.clone());
        assert!(on_a_screen(-1_000_000, -1_000_000, 800));
        assert_eq!(none.monitor_area(0, 0, true), None);
        assert_eq!(none.primary_area(false), None);
    }

    #[test]
    fn the_nearest_monitor_answers_for_a_point_off_every_screen() {
        // A main screen and a tall one to its left, higher up (negative places).
        let setup = Setup {
            scale: 1.0,
            primary: 0,
            monitors: vec![
                Monitor { area: (0, 0, 1920, 1080), work: (0, 25, 1920, 1055) },
                Monitor { area: (-1440, -2018, 1440, 3440), work: (-1440, -2018, 1440, 3400) },
            ],
        };
        // Inside each; the edges: left and top inclusive, right and bottom not.
        assert_eq!(setup.monitor_area(10, 10, false), Some((0, 0, 1920, 1080)));
        assert_eq!(setup.monitor_area(-1, 10, false), Some((-1440, -2018, 1440, 3440)));
        assert_eq!(setup.monitor_area(1920, 10, false), Some((0, 0, 1920, 1080)), "off the right edge: nearest");
        // Far away: the nearest one, never nothing; below both it is the one that reaches lower.
        assert_eq!(setup.monitor_area(1500, 3000, true), Some((0, 25, 1920, 1055)));
        assert_eq!(setup.monitor_area(500, 9000, true), Some((-1440, -2018, 1440, 3400)));
        assert_eq!(setup.monitor_area(-5000, -9000, true), Some((-1440, -2018, 1440, 3400)));
        assert_eq!(setup.primary_area(true), Some((0, 25, 1920, 1055)));
    }

    #[test]
    fn the_main_screen_is_the_one_that_is_marked_not_the_first() {
        let setup = Setup {
            scale: 1.0,
            primary: 1,
            monitors: vec![monitor((0, 0, 1280, 1024)), monitor((1280, 0, 1920, 1080))],
        };
        assert_eq!(setup.primary_area(false), Some((1280, 0, 1920, 1080)));
        // A wrong index does not panic.
        let wrong = Setup { primary: 7, ..setup };
        assert_eq!(wrong.primary_area(false), Some((0, 0, 1280, 1024)));
    }

    #[test]
    fn cocoa_screens_turn_into_pixels_from_the_top_of_the_main_screen() {
        // A laptop screen of 1512 x 982 points at scale 2.0, menu bar 38 points,
        // dock hidden: the work area starts below the menu bar.
        let (frame, visible) = ((0.0, 0.0, 1512.0, 982.0), (0.0, 0.0, 1512.0, 944.0));
        assert_eq!(from_cocoa(frame, 982.0, 2.0), (0, 0, 3024, 1964));
        assert_eq!(from_cocoa(visible, 982.0, 2.0), (0, 76, 3024, 1888));
        // A dock of 80 points at the bottom: the visible frame lifts its lower edge.
        assert_eq!(from_cocoa((0.0, 80.0, 1512.0, 864.0), 982.0, 2.0), (0, 76, 3024, 1728));
        // An external screen left of it and 200 points lower (Cocoa: y = -200).
        assert_eq!(from_cocoa((-2560.0, -200.0, 2560.0, 1440.0), 982.0, 2.0), (-5120, -516, 5120, 2880));
        // The same at scale 1.0.
        assert_eq!(from_cocoa((-2560.0, -200.0, 2560.0, 1440.0), 982.0, 1.0), (-2560, -258, 2560, 1440));
    }

    #[test]
    fn a_cocoa_point_turns_into_a_pixel_place_like_the_screens_do() {
        // The pointer in the lower left corner of the laptop screen (1512 x 982 points, scale 2.0).
        assert_eq!(point_from_cocoa((0.0, 0.0), 982.0, 2.0), (0, 1964));
        // 100 points right of and 50 below the upper left corner.
        assert_eq!(point_from_cocoa((100.0, 932.0), 982.0, 2.0), (200, 100));
        // On a screen to the left and above the main one.
        assert_eq!(point_from_cocoa((-300.0, 1200.0), 982.0, 1.0), (-300, -218));
    }

    #[test]
    fn the_work_area_of_the_window_manager_is_cut_to_each_screen() {
        let left = (0, 0, 1920, 1080);
        let right = (1920, 0, 2560, 1440);
        // A panel of 30 pixels at the top of everything.
        let all = (0, 30, 4480, 1410);
        assert_eq!(work_inside(left, all), (0, 30, 1920, 1050));
        assert_eq!(work_inside(right, all), (1920, 30, 2560, 1410));
        // A work area that is empty on this screen or nearly all of it taken is not believed.
        assert_eq!(work_inside(left, (3000, 0, 100, 100)), left);
        assert_eq!(work_inside(left, (0, 0, 1920, 400)), left);
    }

    #[test]
    fn the_x11_scale_follows_winit() {
        assert_eq!(x11_scale(None, None), 1.0);
        assert_eq!(x11_scale(None, Some("144")), 1.5);
        assert_eq!(x11_scale(None, Some("96\n")), 1.0);
        assert_eq!(x11_scale(Some("2"), Some("144")), 2.0, "the variable wins");
        assert_eq!(x11_scale(Some("randr"), Some("120")), 1.25, "not a number: the rest decides");
        assert_eq!(x11_scale(Some("0"), None), 1.0);
        assert_eq!(x11_scale(None, Some("100000")), 8.0, "never absurd");
    }

    #[test]
    fn x11_is_asked_only_where_windows_are_placed_by_coordinates() {
        assert!(x11_in_use(Some(":0"), None, None));
        assert!(!x11_in_use(None, None, None), "no display");
        assert!(x11_in_use(Some(":0"), Some("wayland-0"), Some("wayland")), "XWayland beside a Wayland session");
        assert!(!x11_in_use(None, Some("wayland-0"), Some("wayland")), "Wayland alone");
        assert!(x11_in_use(Some(":0"), Some(""), None));
    }

    /// `DISPLAY`, `WAYLAND_DISPLAY`, `XDG_SESSION_TYPE` and the window system they lead to.
    type Case = (Option<&'static str>, Option<&'static str>, Option<&'static str>, Backend);

    /// X11 wherever an X server is there, Wayland only without one, nothing without either.
    #[test]
    fn the_window_system_is_x11_wherever_there_is_an_x_server() {
        let cases: [Case; 10] = [
            (Some(":0"), None, Some("x11"), Backend::X11),
            (Some(":0"), None, None, Backend::X11),
            // Plasma under Wayland with XWayland: X11 all the same.
            (Some(":0"), Some("wayland-0"), Some("wayland"), Backend::X11),
            (Some(":1"), Some("wayland-1"), None, Backend::X11),
            // A Wayland session without an X server.
            (None, Some("wayland-0"), Some("wayland"), Backend::Wayland),
            (None, Some("wayland-0"), None, Backend::Wayland),
            (Some(""), Some("wayland-0"), None, Backend::Wayland),
            (None, None, Some(" Wayland "), Backend::Wayland),
            // Nothing to open a window on.
            (None, None, Some("tty"), Backend::None),
            (Some(""), Some(""), None, Backend::None),
        ];
        for (display, wayland, session, want) in cases {
            assert_eq!(choose_backend(display, wayland, session), want, "{display:?} {wayland:?} {session:?}");
        }
    }

    /// Three screens side by side, as a player has them: on the left a small one upright at 100 %, reaching far
    /// above the middle one; in the middle and on the right, the main screen, a large one each at 150 %. All places in
    /// the pixels of the whole desktop, the task bar at the bottom of each.
    fn three_screens() -> Setup {
        Setup {
            scale: 1.5,
            primary: 2,
            monitors: vec![
                Monitor { area: (-3640, -520, 1080, 1920), work: (-3640, -520, 1080, 1880) },
                Monitor { area: (-2560, 0, 2560, 1440), work: (-2560, 0, 2560, 1400) },
                Monitor { area: (0, 0, 2560, 1440), work: (0, 0, 2560, 1400) },
            ],
        }
    }

    fn plan(setup: &Setup, at: Option<(i32, i32)>, size: (i32, i32)) -> Option<Area> {
        plan_window(at, size, &|x, y| setup.monitor_area(x, y, true), setup.primary_area(true))
    }

    #[test]
    fn a_window_opens_on_its_own_screen_with_the_top_edge_in_its_work_area_whatever_the_scalings() {
        let setup = three_screens();
        let size = (1920, 1230);
        // Where it was, on the middle screen: it stays.
        assert_eq!(plan(&setup, Some((-2000, 100)), size), Some((-2000, 100, 1920, 1230)));
        // Above the middle screen lie places that belong to no screen: it opens on the middle screen, at its top.
        assert_eq!(plan(&setup, Some((-2000, -300)), size), Some((-2000, 0, 1920, 1230)));
        assert_eq!(plan(&setup, Some((-1000, -480)), size), Some((-1920, 0, 1920, 1230)));
        // The left screen reaches higher: a place above the middle one that lies on it keeps its height.
        assert_eq!(plan(&setup, Some((-3500, -400)), (1000, 800)), Some((-3560, -400, 1000, 800)));
        // Larger than the work area of the left screen: it shrinks to it and stands at its edge.
        assert_eq!(plan(&setup, Some((-3600, -500)), size), Some((-3640, -500, 1080, 1230)));
        // The bottom edge may not leave the work area either (the task bar).
        assert_eq!(plan(&setup, Some((100, 900)), size), Some((100, 170, 1920, 1230)));
        // No place remembered: in the middle of the main screen.
        assert_eq!(plan(&setup, None, size), Some((320, 85, 1920, 1230)));
        // Whatever the place: the whole window lies in the work area of one screen, the top edge with it.
        for x in (-4000..2800).step_by(173) {
            for y in (-900..1700).step_by(131) {
                let (px, py, w, h) = plan(&setup, Some((x, y)), size).expect("a place");
                let work = setup.monitor_area(px, py, true).expect("a screen");
                let inside = px >= work.0 && py >= work.1 && px + w <= work.0 + work.2 && py + h <= work.1 + work.3;
                assert!(inside, "({x}, {y}) gives {:?} outside {work:?}", (px, py, w, h));
            }
        }
        // No screen to ask: nothing is planned.
        assert_eq!(plan_window(Some((0, 0)), size, &|_, _| None, None), None);
    }

    #[test]
    fn the_place_does_not_wander_when_it_is_stored_and_planned_again_and_again() {
        let setup = three_screens();
        for start in [(-2000, 100), (-2000, -300), (-3500, -400), (320, 85), (2000, 1700), (-4000, 2500)] {
            let first = plan(&setup, Some(start), (1920, 1230)).expect("a place");
            let mut now = first;
            for round in 0..6 {
                // What the window shows is stored as it is and planned from there on the next start.
                let again = plan(&setup, Some((now.0, now.1)), (now.2, now.3)).expect("a place");
                assert_eq!(again, first, "start {start:?}, round {round}");
                now = again;
            }
        }
    }
}
