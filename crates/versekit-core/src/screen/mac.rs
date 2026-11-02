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

//! The screens of macOS: `NSScreen` gives every screen and, as the visible
//! frame, its work area without menu bar and dock. Cocoa counts from the
//! lower left corner of the main screen; the window functions (winit, egui)
//! count from its upper left corner, in points.

use objc2_app_kit::{NSEvent, NSScreen};
use objc2_foundation::MainThreadMarker;

use super::{Monitor, Setup, from_cocoa, point_from_cocoa};

/// Every screen. `None` outside the main thread, which is the only one that
/// may ask.
pub(super) fn query() -> Option<Setup> {
    let mtm = MainThreadMarker::new()?;
    let screens = NSScreen::screens(mtm);
    let count = screens.count();
    // The first one is the main screen: the one with the menu bar, at 0, 0.
    let main = screens.firstObject()?;
    let height = main.frame().size.height;
    let scale = main.backingScaleFactor();
    let rect = |r: objc2_foundation::NSRect| (r.origin.x, r.origin.y, r.size.width, r.size.height);
    let monitors = (0..count)
        .map(|i| {
            let screen = screens.objectAtIndex(i);
            Monitor {
                area: from_cocoa(rect(screen.frame()), height, scale),
                work: from_cocoa(rect(screen.visibleFrame()), height, scale),
            }
        })
        .collect();
    Some(Setup { scale: scale as f32, primary: 0, monitors })
}

/// Where the pointer is, in pixels of `setup`.
pub(super) fn pointer(setup: &Setup) -> Option<(i32, i32)> {
    let scale = f64::from(setup.scale);
    let primary = setup.primary_area(false)?;
    let at = NSEvent::mouseLocation();
    Some(point_from_cocoa((at.x, at.y), f64::from(primary.3) / scale, scale))
}
