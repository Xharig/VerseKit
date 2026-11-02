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

//! The frame rates beside the shard: the watch that reads them from the picture of the game runs while the player
//! switched it on in the settings, and only in a run that may grab the screen.

use versekit_core::fps_scan::{self, Reader, Reading, System, Watch};
use versekit_core::signature_watch::{Screen, Source};

pub struct FpsLine {
    /// A run that may grab the screen: real data, no self-check, a system that can.
    active: bool,
    watch: Option<Watch>,
}

impl FpsLine {
    pub fn new(real: bool) -> FpsLine {
        FpsLine { active: real && fps_scan::supported(), watch: None }
    }

    /// Starts or ends the watch as the switch says.
    pub fn apply(&mut self, on: bool, ctx: &egui::Context) {
        self.apply_from(on, ctx, || (Box::new(Screen), Box::new(System)));
    }

    /// As [`FpsLine::apply`], with the pictures and the recognition from `parts` (the checks make them up).
    pub fn apply_from(
        &mut self,
        on: bool,
        ctx: &egui::Context,
        parts: impl FnOnce() -> (Box<dyn Source>, Box<dyn Reader>),
    ) {
        if !self.active {
            return;
        }
        if on && self.watch.is_none() {
            let (source, reader) = parts();
            let waking = ctx.clone();
            self.watch = Watch::start(source, reader, fps_scan::EVERY, move || waking.request_repaint());
        } else if !on && let Some(watch) = self.watch.take() {
            watch.stop();
        }
    }

    /// The frame rates while a fresh reading stands.
    pub fn reading(&self) -> Option<Reading> {
        self.watch.as_ref().and_then(Watch::reading)
    }

    /// Ends the watch when the overlay closes.
    pub fn close(&mut self) {
        if let Some(watch) = self.watch.take() {
            watch.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A screen that must never be asked: the switch is off or the run may not grab.
    struct Never;

    impl Source for Never {
        fn game(&mut self) -> Option<versekit_core::screen_grab::Area> {
            panic!("the screen is asked")
        }
        fn grab(
            &mut self,
            _: versekit_core::screen_grab::Area,
        ) -> Result<Vec<u8>, versekit_core::screen_grab::GrabError> {
            panic!("the screen is grabbed")
        }
    }

    #[test]
    fn the_watch_runs_only_while_switched_on_and_never_in_a_run_that_may_not_grab() {
        let ctx = egui::Context::default();
        let started = std::cell::Cell::new(0);
        let parts = || {
            started.set(started.get() + 1);
            (Box::new(Never) as Box<dyn Source>, Box::new(System) as Box<dyn Reader>)
        };
        // The self-check and the samples never grab, whatever the switch says.
        let mut quiet = FpsLine::new(false);
        quiet.apply_from(true, &ctx, parts);
        assert!(quiet.watch.is_none() && started.get() == 0);
        // Off by default: nothing starts.
        let mut line = FpsLine { active: true, watch: None };
        line.apply_from(false, &ctx, parts);
        assert!(line.watch.is_none() && started.get() == 0);
        assert_eq!(line.reading(), None);
        line.close();
    }

    /// The game in the background: the screen is asked, never grabbed.
    struct Background;

    impl Source for Background {
        fn game(&mut self) -> Option<versekit_core::screen_grab::Area> {
            None
        }
        fn grab(
            &mut self,
            _: versekit_core::screen_grab::Area,
        ) -> Result<Vec<u8>, versekit_core::screen_grab::GrabError> {
            panic!("the screen is grabbed while the game is not in front")
        }
    }

    #[test]
    fn switched_on_the_watch_starts_once_and_goes_with_the_switch() {
        let ctx = egui::Context::default();
        let started = std::cell::Cell::new(0);
        let parts = || {
            started.set(started.get() + 1);
            (Box::new(Background) as Box<dyn Source>, Box::new(System) as Box<dyn Reader>)
        };
        let mut line = FpsLine { active: true, watch: None };
        line.apply_from(true, &ctx, parts);
        line.apply_from(true, &ctx, parts);
        assert_eq!(started.get(), 1, "one watch, not one per frame");
        assert!(line.watch.as_ref().is_some_and(Watch::alive));
        assert_eq!(line.reading(), None, "nothing read while the game is not in front");
        line.apply_from(false, &ctx, parts);
        assert!(line.watch.is_none());
        line.apply_from(true, &ctx, parts);
        assert_eq!(started.get(), 2);
        line.close();
        assert!(line.watch.is_none());
    }
}
