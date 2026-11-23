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

//! The short notice over the game after a scan by hotkey: what came of it, in a few words, at the top edge of the
//! picture for a moment ([`place`]). The overlay shows it (`versekit_overlay::notice`); any scan can make one.
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
    /// A stock scan met a tile it counted already in this round: in the colour of the text, with the sound
    /// [`Cue::Counted`]; nothing changed.
    Counted,
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
            NoticeKind::Counted => Some(Cue::Counted),
            NoticeKind::Working => None,
        }
    }
}

/// The stock a scan was meant for. A notice of a stock scan always names it, in a colour and with a sound of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    /// The own stock ("Mein Lager").
    Own,
    /// The stock of the unit ("Lager der Einheit").
    Unit,
}

/// A notice: its kind, the title (shown in capitals) and the line below it, empty where there is none. `away` for
/// the one that says Star Citizen was not in front; `target` for a stock scan: the stock it was meant for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub kind: NoticeKind,
    pub title: String,
    pub detail: String,
    pub away: bool,
    pub target: Option<Target>,
}

impl Notice {
    pub fn new(kind: NoticeKind, title: impl Into<String>, detail: impl Into<String>) -> Notice {
        Notice { kind, title: title.into(), detail: detail.into(), away: false, target: None }
    }

    /// The notice of a stock scan into `target`.
    pub fn for_target(
        target: Target,
        kind: NoticeKind,
        title: impl Into<String>,
        detail: impl Into<String>,
    ) -> Notice {
        Notice { target: Some(target), ..Notice::new(kind, title, detail) }
    }

    /// The sound after the reading: a taken stock scan has the sound of its stock, any other the one of its kind;
    /// `None` while it runs.
    pub fn cue(&self) -> Option<Cue> {
        match (self.kind, self.target) {
            (NoticeKind::Done, Some(Target::Own)) => Some(Cue::StockOwn),
            (NoticeKind::Done, Some(Target::Unit)) => Some(Cue::StockUnit),
            _ => self.kind.sound(),
        }
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

/// Where a notice of `size` stands over the game `area` (left, top, width, height): in the middle across, at the top
/// edge (a fiftieth of the height below it). Never over the middle of the picture: there stands what the player
/// scans (the refinery terminal with up to three job cards side by side, the card under the pointer), and a notice
/// there covers it for the player.
pub fn place(area: (i32, i32, i32, i32), size: (i32, i32)) -> (i32, i32, i32, i32) {
    let (x, y, w, h) = area;
    let left = x + (w - size.0) / 2;
    let top = y + h / 50;
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
    fn a_taken_stock_scan_sounds_by_its_stock_and_the_rest_by_its_kind() {
        let own = Notice::for_target(Target::Own, NoticeKind::Done, "Mein Lager", "3x Parallax");
        let unit = Notice::for_target(Target::Unit, NoticeKind::Done, "Lager der Einheit", "3x Parallax");
        assert_eq!((own.cue(), unit.cue()), (Some(Cue::StockOwn), Some(Cue::StockUnit)));
        assert_ne!(own.cue(), unit.cue());
        let failed = Notice::for_target(Target::Unit, NoticeKind::Nothing, "Lager der Einheit", "");
        assert_eq!(failed.cue(), Some(Cue::Failed), "a failure sounds the same for both");
        assert_eq!(Notice::new(NoticeKind::Done, "x", "").cue(), Some(Cue::Done));
        assert_eq!(Notice::new(NoticeKind::Working, "x", "").cue(), None);
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
    fn the_notice_stands_in_the_middle_across_at_the_top_edge() {
        assert_eq!(font_px(1440), 40);
        assert_eq!(font_px(300), 18);
        assert_eq!(place((0, 0, 5120, 1440), (600, 100)), (2260, 28, 600, 100));
        // A game window on a second screen left of the first.
        assert_eq!(place((-2560, 0, 2560, 1440), (600, 100)), (-1580, 28, 600, 100));
        assert_eq!(place((0, 0, 400, 100), (600, 200)).1, 2);
    }

    /// Levski, 22.11.2026: the notice stood over the terminal, in the picture the reading took and before the eyes
    /// of the player, where two or three job cards may stand side by side. The terminal fills the middle of the
    /// picture; the notice keeps to the band at the top edge, on every usual screen.
    #[test]
    fn the_notice_never_stands_over_the_middle_of_the_picture_where_the_terminal_is() {
        for (w, h) in [(1920, 1080), (2560, 1440), (3440, 1440), (3840, 2160), (5120, 1440)] {
            let size = (w / 4, font_px(h) * 5 / 2);
            let (_, top, _, high) = place((0, 0, w, h), size);
            assert!(top + high <= h / 8, "{w}x{h}: {top}+{high}");
        }
    }
}
