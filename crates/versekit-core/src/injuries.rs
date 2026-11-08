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

//! Injuries in the Game.log, in every game language: the warning of the HUD ("Minor Injury Detected") as a
//! notification, `Added notification "<text>…"`, the way the blueprints and the contracts come.
//!
//! The text is translated; its language-neutral keys stand in the `global.ini` of the game
//! (`hud_actorStatus_injuryWarning_severity_*`). The phrases are taken from the unpacked `global.ini` files of the
//! installation (a translation lies there), then from the bundled table of the wordings seen.
//!
//! The line, from a log of 04.11.2026 (German translation):
//! `[Notice] <SHUDEvent_OnNotification> Added notification "Leichte Verletzung erkannt - Linker Arm - Behandlung
//! der Stufe 3 erforderlich : " [91] to queue. …`. One such line per injury; a line with only the text and
//! `<UpdateNotificationItem> Notification "…"` follow and do not count. Body part and tier after the phrase are
//! not read. The same text again within [`REPEAT_GAP`] seconds counts once.

use std::fs;
use std::path::Path;

use crate::phrases::Sources;
use crate::text::{decode_ignore, lines_universal, py_strip};

/// The notification of the HUD, the same in every language.
pub const NOTIFICATION: &str = "Added notification \"";
/// The same text this soon again is the same injury.
pub const REPEAT_GAP: i64 = 10;

/// How bad an injury is, as the HUD tells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Severity {
    Minor,
    Moderate,
    Severe,
}

impl Severity {
    pub const ALL: [Severity; 3] = [Severity::Minor, Severity::Moderate, Severity::Severe];

    /// The key of the text in the `global.ini`.
    pub fn ini_key(self) -> &'static str {
        match self {
            Severity::Minor => "hud_actorStatus_injuryWarning_severity_injury",
            Severity::Moderate => "hud_actorStatus_injuryWarning_severity_majorInjury",
            Severity::Severe => "hud_actorStatus_injuryWarning_severity_deadlyInjury",
        }
    }

    /// The key in `statistik.json`.
    pub fn stored(self) -> &'static str {
        match self {
            Severity::Minor => "leicht",
            Severity::Moderate => "mittel",
            Severity::Severe => "schwer",
        }
    }

    pub fn index(self) -> usize {
        match self {
            Severity::Minor => 0,
            Severity::Moderate => 1,
            Severity::Severe => 2,
        }
    }
}

/// The wordings seen, for when no `global.ini` of the installation holds them: the English ones of the game files,
/// and the German ones of two versions of the translation found in game logs ("erkannt" the newer one).
pub const TABLE: [(Severity, &str); 9] = [
    (Severity::Minor, "Minor Injury Detected"),
    (Severity::Moderate, "Moderate Injury Detected"),
    (Severity::Severe, "Severe Injury Detected"),
    (Severity::Minor, "Leichte Verletzung erkannt"),
    (Severity::Moderate, "Mittelschwere Verletzung erkannt"),
    (Severity::Severe, "Schwere Verletzung erkannt"),
    (Severity::Minor, "Leichte Verletzung festgestellt"),
    (Severity::Moderate, "Mäßige Verletzung festgestellt"),
    (Severity::Severe, "Schwere Verletzung festgestellt"),
];

/// The phrases of the three severities, longest first, so that no phrase takes the line of a longer one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Phrases(Vec<(Severity, String)>);

impl Phrases {
    /// The bundled table only.
    pub fn table() -> Phrases {
        Phrases::from_list(TABLE.iter().map(|(severity, text)| (*severity, (*text).to_string())).collect())
    }

    /// The phrases of the `global.ini` files of `sources`, then the table.
    pub fn of(sources: &Sources) -> Phrases {
        let mut list: Vec<(Severity, String)> = Vec::new();
        for file in sources.ini_files() {
            list.extend(from_ini(&file));
        }
        list.extend(TABLE.iter().map(|(severity, text)| (*severity, (*text).to_string())));
        Phrases::from_list(list)
    }

    fn from_list(list: Vec<(Severity, String)>) -> Phrases {
        let mut kept: Vec<(Severity, String)> = Vec::new();
        for (severity, text) in list {
            let text = py_strip(&text).trim_end_matches(':').trim_end().to_string();
            if !text.is_empty() && !kept.iter().any(|(_, known)| *known == text) {
                kept.push((severity, text));
            }
        }
        kept.sort_by_key(|(_, text)| std::cmp::Reverse(text.chars().count()));
        Phrases(kept)
    }

    /// The severity of an injury the line tells of: a notification whose text begins with a phrase.
    pub fn severity(&self, line: &str) -> Option<Severity> {
        self.notice(line).map(|(severity, _)| severity)
    }

    /// The severity of an injury the line tells of and the whole text of its notification (with body part and
    /// tier, which tell two injuries of one moment apart).
    pub fn notice<'a>(&self, line: &'a str) -> Option<(Severity, &'a str)> {
        let (_, after) = line.split_once(NOTIFICATION)?;
        let text = after.split('"').next().unwrap_or("").trim();
        let severity =
            self.0.iter().find(|(_, phrase)| text.starts_with(phrase.as_str())).map(|(severity, _)| *severity)?;
        Some((severity, text))
    }
}

/// The phrases of one `global.ini`: the value of each key of an injury, a key with `,P` behind its name too.
pub fn from_ini(path: &Path) -> Vec<(Severity, String)> {
    let Ok(bytes) = fs::read(path) else { return Vec::new() };
    let text = decode_ignore(&bytes);
    let mut found = Vec::new();
    for line in lines_universal(text.trim_start_matches('\u{feff}')) {
        let Some((key, value)) = line.split_once('=') else { continue };
        let key = key.split(',').next().unwrap_or("").trim();
        if let Some(severity) = Severity::ALL.into_iter().find(|s| s.ini_key().eq_ignore_ascii_case(key)) {
            found.push((severity, value.to_string()));
        }
    }
    found
}

/// The injuries of a session, one count per severity; the same text again within [`REPEAT_GAP`] seconds counts
/// once.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Count {
    counts: [i64; 3],
    last: std::collections::HashMap<String, i64>,
}

impl Count {
    /// A warning of `severity` with the text `text` at `moment` (seconds; `None` without a stamp, which always
    /// counts).
    pub fn add(&mut self, severity: Severity, text: &str, moment: Option<i64>) {
        if let Some(now) = moment {
            let again = self.last.get(text).is_some_and(|before| now - before <= REPEAT_GAP);
            self.last.insert(text.to_string(), now);
            if again {
                return;
            }
        }
        self.counts[severity.index()] += 1;
    }

    /// Light, moderate, severe.
    pub fn counts(&self) -> [i64; 3] {
        self.counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str) -> String {
        format!("<2026-11-07T10:00:00.000Z> [Notice] <SHUDEvent_OnNotification> Added notification \"{text}: \" [7]")
    }

    #[test]
    fn the_table_tells_the_three_severities_in_english_and_the_light_one_in_german() {
        let phrases = Phrases::table();
        assert_eq!(phrases.severity(&line("Minor Injury Detected")), Some(Severity::Minor));
        assert_eq!(phrases.severity(&line("Moderate Injury Detected")), Some(Severity::Moderate));
        assert_eq!(phrases.severity(&line("Severe Injury Detected")), Some(Severity::Severe));
        assert_eq!(phrases.severity(&line("Leichte Verletzung erkannt")), Some(Severity::Minor));
        // No injury, another notification, a removed one, a mention outside a notification: nothing.
        assert_eq!(phrases.severity(&line("No Injury Detected")), None);
        assert_eq!(phrases.severity(&line("Bauplan erhalten: Helm")), None);
        assert_eq!(phrases.severity("Removed notification \"Minor Injury Detected: \""), None);
        assert_eq!(phrases.severity("<Debug> Minor Injury Detected"), None);
    }

    #[test]
    fn the_phrases_of_a_global_ini_come_first_by_their_keys_in_any_language() {
        let dir = crate::testdir::fresh("verletzungen-ini");
        let folder = dir.join("Localization").join("french_(france)");
        fs::create_dir_all(&folder).unwrap();
        let ini = "\u{feff}hud_actorStatus_injuryWarning_severity_injury=Blessure légère détectée\n\
                   hud_actorStatus_injuryWarning_severity_majorInjury,P=Blessure modérée détectée\n\
                   hud_actorStatus_injuryWarning_severity_deadlyInjury=Blessure grave détectée\n\
                   hud_actorStatus_injuryWarning_severity_no_injury=Aucune blessure détectée\n\
                   autre_cle=Blessure\n";
        fs::write(folder.join("global.ini"), ini).unwrap();
        let sources = Sources { phrases_file: None, localization_folder: Some(dir.join("Localization")) };
        let phrases = Phrases::of(&sources);
        assert_eq!(phrases.severity(&line("Blessure légère détectée")), Some(Severity::Minor));
        assert_eq!(phrases.severity(&line("Blessure modérée détectée")), Some(Severity::Moderate), "a key with ,P");
        assert_eq!(phrases.severity(&line("Blessure grave détectée")), Some(Severity::Severe));
        assert_eq!(phrases.severity(&line("Aucune blessure détectée")), None, "no injury is none");
        assert_eq!(phrases.severity(&line("Minor Injury Detected")), Some(Severity::Minor), "the table stays");
    }

    /// The six German wordings of the two versions of the translation, as the game logs hold them, with body part and
    /// tier behind the phrase.
    #[test]
    fn both_german_versions_of_the_translation_tell_their_severity() {
        let phrases = Phrases::table();
        for (text, severity) in [
            ("Leichte Verletzung erkannt", Severity::Minor),
            ("Mittelschwere Verletzung erkannt", Severity::Moderate),
            ("Schwere Verletzung erkannt", Severity::Severe),
            ("Leichte Verletzung festgestellt", Severity::Minor),
            ("Mäßige Verletzung festgestellt", Severity::Moderate),
            ("Schwere Verletzung festgestellt", Severity::Severe),
        ] {
            let full = format!("{text} - Linker Arm - Behandlung der Stufe 3 erforderlich ");
            assert_eq!(phrases.severity(&line(&full)), Some(severity), "{text}");
        }
    }

    #[test]
    fn the_same_text_within_ten_seconds_counts_once_and_two_body_parts_twice() {
        let mut count = Count::default();
        let arm = "Leichte Verletzung erkannt - Linker Arm";
        count.add(Severity::Minor, arm, Some(100));
        count.add(Severity::Minor, arm, Some(105));
        count.add(Severity::Minor, arm, Some(114));
        count.add(Severity::Minor, "Leichte Verletzung erkannt - Rechtes Bein", Some(105));
        count.add(Severity::Moderate, "Mittelschwere Verletzung erkannt - Kopf", Some(106));
        count.add(Severity::Minor, arm, Some(130));
        count.add(Severity::Severe, "Schwere Verletzung erkannt - Torso", None);
        count.add(Severity::Severe, "Schwere Verletzung erkannt - Torso", None);
        assert_eq!(count.counts(), [3, 1, 2]);
    }
}
