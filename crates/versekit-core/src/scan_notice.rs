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

//! The short notice over the game after a scan by hotkey: what came of it, in a few words, in the middle of the
//! picture for a moment. The overlay shows it (`versekit_overlay::notice`); any scan can make one.
//!
//! A notice has a kind, a title in capitals and a line below it (the name read, the candidates, or nothing).

use std::time::Duration;

use crate::sound::Cue;

/// What came of a scan.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NoticeKind {
    /// Read and taken: in the accent, with the sound [`Cue::Done`].
    Done,
    /// Read, but already there: in the colour of the text, with the sound [`Cue::AlreadyThere`].
    AlreadyThere,
    /// Read, but not sure: the player is asked; in the colour of the text, with the sound [`Cue::Ask`].
    Unsure,
    /// Nothing read, the game not in front or a fault: in the colour of the text, with the sound [`Cue::Failed`].
    Nothing,
    /// The reading runs: in the colour of the text, no sound beyond the one of the key; it stays until the outcome
    /// takes its place.
    Working,
}

impl NoticeKind {
    /// Whether the notice stands in the accent.
    pub fn accent(self) -> bool {
        self == NoticeKind::Done
    }

    /// The sound after the reading; `None` while it runs.
    pub fn sound(self) -> Option<Cue> {
        match self {
            NoticeKind::Done => Some(Cue::Done),
            NoticeKind::AlreadyThere => Some(Cue::AlreadyThere),
            NoticeKind::Unsure => Some(Cue::Ask),
            NoticeKind::Nothing => Some(Cue::Failed),
            NoticeKind::Working => None,
        }
    }
}

/// A notice: its kind, the title (shown in capitals) and the line below it, empty where there is none. `away` for
/// the one that says Star Citizen was not in front.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub kind: NoticeKind,
    pub title: String,
    pub detail: String,
    pub away: bool,
}

impl Notice {
    pub fn new(kind: NoticeKind, title: impl Into<String>, detail: impl Into<String>) -> Notice {
        Notice { kind, title: title.into(), detail: detail.into(), away: false }
    }

    /// The notice that Star Citizen was not in front: nothing was read.
    pub fn game_away(title: impl Into<String>) -> Notice {
        Notice { away: true, ..Notice::new(NoticeKind::Nothing, title, "") }
    }

    /// The title as it is shown: in capitals.
    pub fn shown_title(&self) -> String {
        self.title.to_uppercase()
    }

    /// How long the notice stays: two seconds, a question a little longer to be read, a running reading until its
    /// outcome comes (at most half a minute).
    pub fn duration(&self) -> Duration {
        match self.kind {
            NoticeKind::Unsure => Duration::from_millis(3500),
            NoticeKind::Working => Duration::from_secs(30),
            _ => Duration::from_secs(2),
        }
    }
}

/// The height of the letters of the title for a picture `height` points high: a thirty-sixth of it, at least 18.
pub fn font_px(height: i32) -> i32 {
    (height / 36).max(18)
}

/// Where a notice of `size` stands over the game `area` (left, top, width, height): in the middle across, a little
/// above the middle downwards, so it does not cover the crosshair or the hint the player looks at.
pub fn place(area: (i32, i32, i32, i32), size: (i32, i32)) -> (i32, i32, i32, i32) {
    let (x, y, w, h) = area;
    let left = x + (w - size.0) / 2;
    let top = y + h * 3 / 10 - size.1 / 2;
    (left, top.max(y), size.0, size.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kinds_have_their_colour_and_sound() {
        assert!(NoticeKind::Done.accent());
        assert!(!NoticeKind::AlreadyThere.accent() && !NoticeKind::Unsure.accent() && !NoticeKind::Nothing.accent());
        assert_eq!(NoticeKind::Done.sound(), Some(Cue::Done));
        assert_eq!(NoticeKind::AlreadyThere.sound(), Some(Cue::AlreadyThere));
        assert_eq!(NoticeKind::Unsure.sound(), Some(Cue::Ask));
        assert_eq!(NoticeKind::Nothing.sound(), Some(Cue::Failed));
        assert_eq!(NoticeKind::Working.sound(), None, "the click of the key says that it runs");
    }

    #[test]
    fn only_the_notice_of_the_game_away_says_so() {
        let away = Notice::game_away("Star Citizen ist nicht im Vordergrund");
        assert!(away.away);
        assert_eq!((away.kind, away.detail.as_str()), (NoticeKind::Nothing, ""));
        assert!(!Notice::new(NoticeKind::Nothing, "Nichts erkannt", "").away);
    }

    #[test]
    fn the_title_stands_in_capitals_and_a_question_stays_longer() {
        let notice = Notice::new(NoticeKind::Done, "Bauplan erfasst", "Sedulity (Ind/2/B)");
        assert_eq!(notice.shown_title(), "BAUPLAN ERFASST");
        assert_eq!(notice.duration(), Duration::from_secs(2));
        assert!(Notice::new(NoticeKind::Unsure, "x", "").duration() > notice.duration());
    }

    #[test]
    fn the_notice_stands_in_the_middle_across_and_above_the_middle_downwards() {
        assert_eq!(font_px(1440), 40);
        assert_eq!(font_px(300), 18);
        assert_eq!(place((0, 0, 5120, 1440), (600, 100)), (2260, 382, 600, 100));
        // A game window on a second screen left of the first.
        assert_eq!(place((-2560, 0, 2560, 1440), (600, 100)), (-1580, 382, 600, 100));
        assert_eq!(place((0, 0, 400, 100), (600, 200)).1, 0);
    }
}
