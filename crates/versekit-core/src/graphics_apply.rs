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

//! All the changes of the graphics page in one go, and the ones kept for the end of the game
//! (`apply_all`, `apply_queued`, `queued`, `set_queued` of `graphics.py` in 4.12).
//!
//! The changes of the page are three kinds of files: the commands of the `user.cfg`, the renderer in
//! `GraphicsSettings.json`, the lines of `attributes.xml`. They are written in this order. Every writing goes
//! through `game_files` (the game must be quiet; a copy first).
//!
//! **Kept for later:** while the game runs the page writes nothing but keeps the changes in the settings
//! (`grafik_vorgemerkt`, the same form as 4.12 kept them in: `{name: value}`, `GraphicsRenderer` for the
//! renderer, `cfg:<command>` for a command). The overlay writes them as soon as the game is quiet.
//!
//! ⚠ Different from 4.12: the three kinds are **checked before the first one is written** (a bad line of
//! the `user.cfg`, a name or level that is not known). 4.12 wrote the first file and only then met the error
//! of the next one.

use crate::game_files::WriteError;
use crate::graphics;
use crate::jsonio::{self, Value};
use crate::language::{Arg, t, tf};
use crate::paths::Paths;
use crate::renderer;
use crate::usercfg::{self, SetError};

/// The key in the settings under which the changes wait.
pub const QUEUE_SETTING: &str = "grafik_vorgemerkt";
const CFG_PREFIX: &str = "cfg:";

/// What the page wants written.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Changes {
    /// Lines of `attributes.xml`; `None` takes the line out.
    pub attributes: Vec<(String, Option<f64>)>,
    /// 0 or 1 for `GraphicsSettings.json`.
    pub renderer: Option<i64>,
    /// Commands of the `user.cfg`; `None` takes the line out.
    pub cfg: Vec<(String, Option<String>)>,
}

impl Changes {
    /// How many things are meant (what the note "n changes kept" counts).
    pub fn len(&self) -> usize {
        self.attributes.len() + usize::from(self.renderer.is_some()) + self.cfg.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// What is wanted for a line of `attributes.xml`: `Some(None)` is "take the line out".
    pub fn attribute(&self, name: &str) -> Option<Option<f64>> {
        self.attributes.iter().find(|(n, _)| n == name).map(|(_, v)| *v)
    }

    /// Wants a line of `attributes.xml` to be so (`None`: taken out); a second wish for the same line replaces
    /// the first.
    pub fn set_attribute(&mut self, name: &str, value: Option<f64>) {
        match self.attributes.iter_mut().find(|(n, _)| n == name) {
            Some(entry) => entry.1 = value,
            None => self.attributes.push((name.to_string(), value)),
        }
    }

    /// What is wanted for a command of the `user.cfg`: `Some(None)` is "take the line out".
    pub fn cfg_value(&self, command: &str) -> Option<Option<&str>> {
        self.cfg.iter().find(|(c, _)| c == command).map(|(_, v)| v.as_deref())
    }

    /// Wants a command of the `user.cfg` to be so (`None`: taken out).
    pub fn set_cfg(&mut self, command: &str, value: Option<String>) {
        match self.cfg.iter_mut().find(|(c, _)| c == command) {
            Some(entry) => entry.1 = value,
            None => self.cfg.push((command.to_string(), value)),
        }
    }

    /// In the form the settings keep them in.
    pub fn to_value(&self) -> Value {
        let number = |x: f64| if x.fract() == 0.0 && x.abs() < 1e15 { Value::int(x as i64) } else { Value::Float(x) };
        let mut map: jsonio::Map = Vec::new();
        for (name, value) in &self.attributes {
            map.push((name.clone(), value.map_or(Value::Null, number)));
        }
        if let Some(value) = self.renderer {
            map.push((renderer::KEY.to_string(), Value::int(value)));
        }
        for (command, value) in &self.cfg {
            map.push((format!("{CFG_PREFIX}{command}"), value.as_deref().map_or(Value::Null, Value::str)));
        }
        Value::Object(map)
    }

    /// From the settings' form. A value of a kind that does not belong (a text for a number, a number for a
    /// command) is left out, not read as "take the line out": that would put a setting back to its default.
    pub fn from_value(value: &Value) -> Changes {
        let mut changes = Changes::default();
        let Some(map) = value.as_object() else { return changes };
        for (name, value) in map {
            if name == renderer::KEY {
                changes.renderer = match value {
                    Value::Int(text) => text.parse().ok(),
                    Value::Float(x) if x.fract() == 0.0 => Some(*x as i64),
                    Value::Bool(b) => Some(i64::from(*b)),
                    _ => None,
                };
            } else if let Some(command) = name.strip_prefix(CFG_PREFIX) {
                match value {
                    Value::Str(text) => changes.cfg.push((command.to_string(), Some(text.clone()))),
                    Value::Null => changes.cfg.push((command.to_string(), None)),
                    _ => {}
                }
            } else {
                match value {
                    Value::Int(text) => {
                        if let Ok(number) = text.parse::<f64>() {
                            changes.attributes.push((name.clone(), Some(number)));
                        }
                    }
                    Value::Float(x) => changes.attributes.push((name.clone(), Some(*x))),
                    Value::Bool(b) => changes.attributes.push((name.clone(), Some(f64::from(u8::from(*b))))),
                    Value::Null => changes.attributes.push((name.clone(), None)),
                    _ => {}
                }
            }
        }
        changes
    }
}

/// Why the changes were not (all) written.
#[derive(Debug)]
pub enum ApplyError {
    /// A line of the `user.cfg` that is not valid: nothing was written.
    BadLine(String),
    /// A name or level of `attributes.xml` that is wrong: nothing was written.
    BadAttribute,
    /// The `user.cfg` could not be set, or the game's folder is not known.
    UserCfg(SetError),
    /// There is no folder of the game to write the `user.cfg` into.
    NoFolder,
    /// `GraphicsSettings.json`.
    Renderer(renderer::Failure),
    /// `attributes.xml`.
    Attributes(graphics::Failure),
}

impl ApplyError {
    /// Did it fail because the game runs (or nobody can tell)? Then nothing is lost by waiting.
    pub fn game_not_quiet(&self) -> bool {
        let quiet = |error: &WriteError| matches!(error, WriteError::GameRuns | WriteError::GameUnknown);
        match self {
            ApplyError::UserCfg(SetError::Write(error))
            | ApplyError::Renderer(renderer::Failure::Write(error))
            | ApplyError::Attributes(graphics::Failure::Write(error)) => quiet(error),
            _ => false,
        }
    }

    /// The reason in the language of the program.
    pub fn reason(&self) -> String {
        let write = |error: &WriteError| error.reason();
        match self {
            ApplyError::BadLine(line) => tf("s_gr_f_cfg", &[("0", Arg::from(line.as_str()))]),
            ApplyError::BadAttribute => t("s_gr_f_abgelehnt"),
            ApplyError::NoFolder => t("s_gr_f_keine_datei"),
            ApplyError::UserCfg(SetError::Bad(bad)) => {
                tf("s_gr_f_cfg", &[("0", Arg::from(bad.first().map_or("", |(_, line)| line.as_str())))])
            }
            ApplyError::UserCfg(SetError::Write(error)) => write(error),
            ApplyError::Renderer(renderer::Failure::NoFile) => t("s_gr_f_keine_datei"),
            ApplyError::Renderer(renderer::Failure::Read) => t("s_gr_f_lesen"),
            ApplyError::Renderer(renderer::Failure::Invalid) => t("s_gr_f_abgelehnt"),
            ApplyError::Renderer(renderer::Failure::Write(error)) => write(error),
            ApplyError::Attributes(graphics::Failure::NoFile) => t("s_gr_f_keine_datei"),
            ApplyError::Attributes(graphics::Failure::Read) => t("s_gr_f_lesen"),
            // A file without its closing tag is a file to read, not a value that is not allowed.
            ApplyError::Attributes(graphics::Failure::Refused(graphics::ChangeError::NoClose)) => t("s_gr_f_lesen"),
            ApplyError::Attributes(graphics::Failure::Refused(_)) => t("s_gr_f_abgelehnt"),
            ApplyError::Attributes(graphics::Failure::Write(error)) => write(error),
        }
    }
}

fn base_name(path: &str) -> String {
    std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Writes everything: `user.cfg`, then the renderer, then `attributes.xml`, and at the first error stops.
/// Gives the names of the copies made (`user.cfg` for the command file, whose copies are its own business).
/// The game must be quiet (every writing asks).
///
/// # Errors
///
/// [`ApplyError`]: bad lines or names (nothing written then), or a file that could not be written.
pub fn apply_all(paths: &Paths, game_folder: Option<&str>, changes: &Changes) -> Result<Vec<String>, ApplyError> {
    // Before anything is written: what can be known to be wrong.
    if !changes.cfg.is_empty() {
        let lines: Vec<String> = changes
            .cfg
            .iter()
            .filter_map(|(command, value)| {
                value.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(|v| format!("{command} = {v}"))
            })
            .collect();
        if let Some((_, line)) = usercfg::check(&lines).into_iter().next() {
            return Err(ApplyError::BadLine(line));
        }
    }
    graphics::validate(&changes.attributes).map_err(|_| ApplyError::BadAttribute)?;
    if changes.renderer.is_some_and(|value| !(0..=1).contains(&value)) {
        return Err(ApplyError::Renderer(renderer::Failure::Invalid));
    }
    // And what only the files can tell: each is found, read and the new text worked out, without writing. A
    // file that is missing, broken or not readable is known before the first of the three is touched.
    let cfg_folder = if changes.cfg.is_empty() {
        None
    } else {
        let folder = game_folder.filter(|f| !f.is_empty()).map(str::to_string).or_else(|| paths.game_folder());
        let folder = folder.ok_or(ApplyError::NoFolder)?;
        usercfg::check_set(&folder, &changes.cfg).map_err(ApplyError::UserCfg)?;
        Some(folder)
    };
    if let Some(value) = changes.renderer {
        renderer::check(paths, None, value).map_err(ApplyError::Renderer)?;
    }
    let attribute_path = game_folder.filter(|f| !f.is_empty()).and_then(|f| graphics::attribute_file(paths, Some(f)));
    if !changes.attributes.is_empty() {
        graphics::check(paths, attribute_path.as_deref(), &changes.attributes).map_err(ApplyError::Attributes)?;
    }

    let mut backups = Vec::new();
    if let Some(folder) = &cfg_folder
        && usercfg::set_values(paths, folder, &changes.cfg).map_err(ApplyError::UserCfg)?
    {
        backups.push("user.cfg".to_string());
    }
    if let Some(value) = changes.renderer
        && let Some(backup) = renderer::write(paths, None, value).map_err(ApplyError::Renderer)?
    {
        backups.push(base_name(&backup));
    }
    if !changes.attributes.is_empty()
        && let Some(backup) =
            graphics::write(paths, attribute_path.as_deref(), &changes.attributes).map_err(ApplyError::Attributes)?
    {
        backups.push(base_name(&backup));
    }
    Ok(backups)
}

/// The changes kept for the end of the game; empty where there are none (or the settings cannot be read now:
/// whoever must not take that for "none" asks [`queued_checked`]).
pub fn queued(paths: &Paths) -> Changes {
    queued_checked(paths).unwrap_or_default()
}

/// Like [`queued`], `None` where the settings could not be read (a file locked for a moment, written just now):
/// that is not "nothing is kept", and a page that took it so would drop what the player wished.
pub fn queued_checked(paths: &Paths) -> Option<Changes> {
    let (settings, readable) = paths.settings_read();
    if !readable {
        return None;
    }
    Some(
        settings
            .iter()
            .find(|(k, _)| k == QUEUE_SETTING)
            .map_or_else(Changes::default, |(_, v)| Changes::from_value(v)),
    )
}

/// Keeps the changes for later; empty ones take the note back. False where the settings could not be written.
pub fn set_queued(paths: &Paths, changes: &Changes) -> bool {
    paths.set_setting(QUEUE_SETTING, changes.to_value())
}

/// What [`apply_queued`] did.
#[derive(Debug)]
pub enum Queued {
    /// Nothing was kept.
    Nothing,
    /// Something is kept but the game is not quiet yet: it stays, try again later.
    NotYet,
    /// It was written, or it failed: the note is gone, so a failure shows once and not at every look.
    Done(Result<Vec<String>, ApplyError>),
}

/// Writes what was kept, once the game is quiet. Called now and then by the overlay and by the page, whichever
/// is there. The note goes when the writing was done or failed for a reason that does not pass; it **stays**
/// where the game turned out to run (or to be unknown) after all, and where it cannot be taken back (then the
/// next look would write it a second time).
pub fn apply_queued(paths: &Paths, game_folder: Option<&str>) -> Queued {
    let Some(changes) = queued_checked(paths) else { return Queued::NotYet };
    if changes.is_empty() {
        return Queued::Nothing;
    }
    if crate::game_files::check_quiet(paths).is_err() {
        return Queued::NotYet;
    }
    let result = apply_all(paths, game_folder, &changes);
    if result.as_ref().is_err_and(ApplyError::game_not_quiet) {
        return Queued::NotYet;
    }
    if !set_queued(paths, &Changes::default()) {
        // Written already (a second go finds the texts the same and writes nothing); the note is tried again.
        crate::errors::record("graphics_apply.take_back", &"the kept changes could not be taken back");
        return Queued::NotYet;
    }
    Queued::Done(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_start;
    use crate::paths::{ENV_INSTALL_DIR, Host};
    use crate::pypath::Flavor;

    fn scratch(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("versekit-gfxapply-{}-{name}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        dir.to_string_lossy().replace('\\', "/")
    }

    const ATTRIBUTES: &str = "<Attributes>\n <Attr name=\"VSync\" value=\"0\"/>\n</Attributes>\n";
    const RENDERER_JSON: &str = "{\"GraphicsSettings\": {\"GraphicsRenderer\": 1}}";

    /// A made-up installation under a throwaway home, the game quiet for `age` seconds.
    fn world(name: &str, age: u64) -> (String, Paths, String) {
        let home = scratch(name);
        let game = format!("{home}/StarCitizen/LIVE");
        let profile = format!("{game}/USER/Client/0/Profiles/default");
        std::fs::create_dir_all(&profile).expect("folder");
        std::fs::write(format!("{profile}/attributes.xml"), ATTRIBUTES).expect("attributes");
        std::fs::write(format!("{game}/user.cfg"), "g_language = german_(germany)\nsys_MaxFPS = 30\n").expect("cfg");
        std::fs::write(format!("{game}/Game.log"), "x").expect("log");
        let at = std::time::SystemTime::now() - std::time::Duration::from_secs(age);
        std::fs::File::options()
            .write(true)
            .open(format!("{game}/Game.log"))
            .and_then(|f| f.set_modified(at))
            .expect("age");
        let local = format!("{home}/AppData/Local");
        let version = format!("{local}/Star Citizen/starcitizen_(sc-alpha-4.2.0)_x/GraphicsSettings");
        std::fs::create_dir_all(&version).expect("folder");
        std::fs::write(format!("{version}/GraphicsSettings.json"), RENDERER_JSON).expect("renderer");
        let vars = [
            ("SC_BP_HOME", home.as_str()),
            ("HOME", home.as_str()),
            ("LOCALAPPDATA", local.as_str()),
            (ENV_INSTALL_DIR, game.as_str()),
            (game_start::ENV_NO_PROCESS, "1"),
        ];
        let paths = Paths::new(Host::new(Flavor::Windows, &vars, &home));
        (home, paths, game)
    }

    fn all_changes() -> Changes {
        Changes {
            attributes: vec![("VSync".into(), Some(1.0)), ("Sharpening".into(), Some(0.25))],
            renderer: Some(0),
            cfg: vec![("sys_MaxFPS".into(), Some("60".into())), ("r_DisplayInfo".into(), None)],
        }
    }

    #[test]
    fn the_changes_keep_in_the_settings_in_the_form_of_4_12_and_come_back_from_it() {
        let changes = all_changes();
        let value = changes.to_value();
        let text = jsonio::dumps(&value, None, false);
        assert_eq!(
            text,
            "{\"VSync\": 1, \"Sharpening\": 0.25, \"GraphicsRenderer\": 0, \"cfg:sys_MaxFPS\": \"60\", \"cfg:r_DisplayInfo\": null}"
        );
        assert_eq!(Changes::from_value(&jsonio::loads(&text).unwrap()), changes);
        assert_eq!(changes.len(), 5);
        assert!(Changes::from_value(&Value::Null).is_empty(), "a note that is no object is nothing");
    }

    #[test]
    fn everything_is_written_in_order_and_the_copies_are_named() {
        let (home, paths, game) = world("alles", 7200);
        let backups = apply_all(&paths, Some(&game), &all_changes()).expect("written");
        assert_eq!(backups.len(), 3, "{backups:?}");
        assert_eq!(backups[0], "user.cfg");
        assert!(backups[1].starts_with("GraphicsSettings.json.versekit-"), "{backups:?}");
        assert!(backups[2].starts_with("attributes.xml.versekit-"), "{backups:?}");
        let cfg = std::fs::read_to_string(format!("{game}/user.cfg")).unwrap().replace("\r\n", "\n");
        assert_eq!(cfg, "sys_MaxFPS = 60\ng_language = german_(germany)\n");
        assert_eq!(renderer::read(&paths, None), Some(0));
        let attributes = graphics::read(&paths, graphics::attribute_file(&paths, Some(&game)).as_deref()).unwrap();
        assert!(attributes.contains(&("VSync", 1.0)) && attributes.contains(&("Sharpening", 0.25)), "{attributes:?}");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_bad_line_or_name_stops_everything_before_the_first_file() {
        let (home, paths, game) = world("schlecht", 7200);
        let mut changes = all_changes();
        changes.cfg.push(("good".into(), Some("1".into())));
        changes.cfg.push(("bad key".into(), Some("2".into())));
        assert!(
            matches!(apply_all(&paths, Some(&game), &changes), Err(ApplyError::BadLine(line)) if line == "bad key = 2")
        );
        let mut changes = all_changes();
        changes.attributes.push(("NoSuchSetting".into(), Some(1.0)));
        assert!(matches!(apply_all(&paths, Some(&game), &changes), Err(ApplyError::BadAttribute)));
        let mut changes = all_changes();
        changes.renderer = Some(2);
        assert!(matches!(
            apply_all(&paths, Some(&game), &changes),
            Err(ApplyError::Renderer(renderer::Failure::Invalid))
        ));
        // Nothing was touched by any of the three.
        assert_eq!(
            std::fs::read_to_string(format!("{game}/user.cfg")).unwrap().replace("\r\n", "\n"),
            "g_language = german_(germany)\nsys_MaxFPS = 30\n"
        );
        assert_eq!(renderer::read(&paths, None), Some(1));
        assert_eq!(
            std::fs::read_to_string(format!("{game}/USER/Client/0/Profiles/default/attributes.xml")).unwrap(),
            ATTRIBUTES,
            "nor the attributes"
        );
        assert!(!std::path::Path::new(&format!("{game}/user.cfg.versekit-vorher")).exists());
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_file_that_is_missing_or_broken_is_known_before_the_first_of_the_three_is_written() {
        let (home, paths, game) = world("vorab", 7200);
        let json = format!(
            "{home}/AppData/Local/Star Citizen/starcitizen_(sc-alpha-4.2.0)_x/GraphicsSettings/GraphicsSettings.json"
        );
        let attributes = format!("{game}/USER/Client/0/Profiles/default/attributes.xml");
        let cfg = format!("{game}/user.cfg");
        let before = (std::fs::read(&cfg).unwrap(), std::fs::read(&attributes).unwrap());
        // No renderer file: the user.cfg, which comes first, is not touched.
        std::fs::rename(&json, format!("{json}.weg")).unwrap();
        let result = apply_all(&paths, Some(&game), &all_changes());
        assert!(matches!(result, Err(ApplyError::Renderer(renderer::Failure::NoFile))), "{result:?}");
        // The attributes without their closing tag: neither the user.cfg nor the renderer is touched.
        std::fs::rename(format!("{json}.weg"), &json).unwrap();
        std::fs::write(&attributes, "<Attributes>\n <Attr name=\"VSync\" value=\"0\"/>\n").unwrap();
        let result = apply_all(&paths, Some(&game), &all_changes());
        assert!(
            matches!(result, Err(ApplyError::Attributes(graphics::Failure::Refused(graphics::ChangeError::NoClose)))),
            "{result:?}"
        );
        assert_eq!(std::fs::read(&cfg).unwrap(), before.0, "the user.cfg is as it was");
        assert_eq!(renderer::read(&paths, None), Some(1), "and the renderer");
        assert!(!std::path::Path::new(&format!("{game}/user.cfg.versekit-vorher")).exists());
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_value_of_the_wrong_kind_in_the_kept_note_is_left_out_and_not_read_as_take_the_line_out() {
        let note = jsonio::loads(
            "{\"VSync\": \"viel\", \"MotionBlur\": null, \"Sharpening\": 0.5, \"GraphicsRenderer\": \"x\", \"cfg:a\": 5, \"cfg:b\": \"1\", \"cfg:c\": null}",
        )
        .unwrap();
        let changes = Changes::from_value(&note);
        assert_eq!(changes.attributes, [("MotionBlur".to_string(), None), ("Sharpening".to_string(), Some(0.5))]);
        assert_eq!(changes.renderer, None);
        assert_eq!(changes.cfg, [("b".to_string(), Some("1".to_string())), ("c".to_string(), None)]);
    }

    #[test]
    fn kept_changes_stay_where_the_game_turns_out_to_run_and_where_the_settings_cannot_be_read() {
        let (home, paths, game) = world("bleibt", 7200);
        assert!(set_queued(&paths, &all_changes()));
        // The log is written right now: the game started after the look: nothing is lost.
        let log = format!("{game}/Game.log");
        std::fs::File::options()
            .write(true)
            .open(&log)
            .and_then(|f| f.set_modified(std::time::SystemTime::now()))
            .unwrap();
        assert!(matches!(apply_queued(&paths, Some(&game)), Queued::NotYet));
        assert_eq!(queued(&paths), all_changes(), "the note stays");
        // Settings that cannot be read: not "nothing kept". A folder where the file should be cannot be read, and
        // it does not depend on the clock. (A half-written file counts as unreadable only for two seconds after
        // it was written and is healed from the last good copy after that: a test thread that stalls for longer
        // between the writing and the look, as under a full build, saw the healed settings and failed now and then.)
        let settings = paths.settings_file();
        std::fs::remove_file(&settings).unwrap();
        std::fs::create_dir(&settings).unwrap();
        assert!(queued_checked(&paths).is_none());
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn kept_changes_wait_while_the_game_runs_and_are_written_once_when_it_is_quiet() {
        let (home, paths, game) = world("vorgemerkt", 5);
        assert!(matches!(apply_queued(&paths, Some(&game)), Queued::Nothing));
        assert!(set_queued(&paths, &all_changes()));
        assert_eq!(queued(&paths), all_changes());
        // The log was written a moment ago: the game runs.
        assert!(matches!(apply_queued(&paths, Some(&game)), Queued::NotYet));
        assert_eq!(queued(&paths), all_changes(), "the note stays");
        assert_eq!(renderer::read(&paths, None), Some(1), "and nothing was written");
        // The game has been quiet for a long time.
        let log = format!("{game}/Game.log");
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(7200);
        std::fs::File::options().write(true).open(&log).and_then(|f| f.set_modified(old)).expect("age");
        let Queued::Done(Ok(backups)) = apply_queued(&paths, Some(&game)) else { panic!("written") };
        assert_eq!(backups.len(), 3);
        assert!(queued(&paths).is_empty(), "the note is gone");
        assert!(matches!(apply_queued(&paths, Some(&game)), Queued::Nothing), "and it is written once");
        assert_eq!(renderer::read(&paths, None), Some(0));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_failure_shows_once_the_note_goes_all_the_same() {
        let (home, paths, game) = world("fehler", 7200);
        let changes = Changes { attributes: vec![("VSync".into(), Some(1.0))], ..Changes::default() };
        assert!(set_queued(&paths, &changes));
        std::fs::remove_file(format!("{game}/USER/Client/0/Profiles/default/attributes.xml")).unwrap();
        let Queued::Done(Err(error)) = apply_queued(&paths, Some(&game)) else { panic!("an error") };
        assert!(matches!(error, ApplyError::Attributes(graphics::Failure::NoFile)), "{error:?}");
        assert!(queued(&paths).is_empty());
        let _ = std::fs::remove_dir_all(home);
    }
}
