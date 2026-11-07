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

//! The rows of the page General that differ by system: the icon next to the clock (Windows), the message of the
//! system (the balloon on Windows, `notify-send` on Linux), the entry in the application menu (Linux), the bridge to
//! scmdb.net (off by default, run by the overlay), and below the hotkey the command for a shortcut of the desktop
//! under Wayland.
//!
//! Where the system cannot, the row says why instead of showing a switch that does nothing.

use egui::{RichText, Ui};
use versekit_core::jsonio::Value;
use versekit_core::language::{Arg, t, tf};
use versekit_core::paths::{Host, Paths};
use versekit_core::theme::Scheme;
use versekit_core::{desktop_entry, notify, overlay, scmdb_bridge};

use crate::color;
use crate::widgets::{self, ButtonKind};

/// The rows of the group "Start" this module draws, after the start with the system and the graphics card.
pub(super) const FIRST_ROW: usize = 2;

/// What a click in one of the rows asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Wish {
    Tray(bool),
    Notice(bool),
    /// Write the entry in the menu (true) or take it away.
    Menu(bool),
    /// Connect to scmdb.net (true) or not.
    Scmdb(bool),
}

/// The row of the bridge to scmdb.net for [`DesktopRows::control`]; it stands in the group Datenschutz.
pub(super) const SCMDB_ROW: usize = 3;

/// Whether the message of the system can show: on Windows only as a balloon at the icon next to the clock.
fn notice_reachable_on(windows: bool, tray: bool) -> bool {
    !windows || tray
}

/// The state of the rows.
pub(super) struct DesktopRows {
    pub tray: bool,
    pub notice: bool,
    /// The bridge to scmdb.net (`scmdb_bruecke`), off by default.
    pub scmdb: bool,
    /// Can this system show a message of the system?
    notice_possible: bool,
    /// Is the entry in the menu there?
    menu_present: bool,
    /// The command for a shortcut of the desktop, where the hotkey cannot reach everywhere.
    pub shortcut: Option<String>,
    /// Is this Windows, where the message of the system hangs on the icon next to the clock?
    windows: bool,
}

impl Default for DesktopRows {
    fn default() -> Self {
        DesktopRows {
            tray: true,
            notice: true,
            scmdb: false,
            notice_possible: true,
            menu_present: false,
            shortcut: None,
            windows: cfg!(windows),
        }
    }
}

impl DesktopRows {
    /// Takes over the settings and asks the system what it can.
    pub fn load(&mut self, paths: &Paths) {
        self.tray = paths.setting_bool(overlay::keys::TRAY, true);
        self.notice = paths.setting_bool(overlay::keys::SYSTEM_NOTICE, true);
        self.scmdb = scmdb_bridge::enabled(paths);
        self.notice_possible = cfg!(windows) || notify::available();
        self.menu_present = desktop_entry::present(&paths.host);
        self.shortcut = desktop_entry::shortcut_command(&paths.host);
    }

    /// Can the message of the system reach the player with the icon as it is set? On Windows it needs the icon next
    /// to the clock, elsewhere nothing.
    pub fn notice_reachable(&self) -> bool {
        notice_reachable_on(self.windows, self.tray)
    }

    /// The settings as stored, under the keys the overlay reads.
    pub fn setting_values(&self) -> [(&'static str, Value); 3] {
        [
            (overlay::keys::TRAY, Value::Bool(self.tray)),
            (overlay::keys::SYSTEM_NOTICE, Value::Bool(self.notice)),
            (scmdb_bridge::SETTING, Value::Bool(self.scmdb)),
        ]
    }

    /// The control of row `r` (counted from [`FIRST_ROW`]) in the column of controls.
    pub fn control(&mut self, row: &mut Ui, scheme: &Scheme, r: usize) -> Option<Wish> {
        let reason = |row: &mut Ui, key: &str| {
            row.label(RichText::new(t(key)).color(color(scheme.sub)));
        };
        match r {
            0 if !cfg!(windows) => reason(row, "s_nur_win"),
            0 => {
                if row.add(widgets::Toggle::new(&mut self.tray, scheme)).changed() {
                    return Some(Wish::Tray(self.tray));
                }
            }
            1 if !self.notice_possible => reason(row, "s_systemmeldung_fehlt"),
            // On Windows the message is a balloon at the icon next to the clock: without the icon the switch is
            // grey and says what it needs.
            1 if !self.notice_reachable() => {
                let mut shown = self.notice;
                row.add_enabled(false, widgets::Toggle::new(&mut shown, scheme))
                    .on_disabled_hover_text(t("s_systemmeldung_braucht"));
            }
            1 => {
                if row.add(widgets::Toggle::new(&mut self.notice, scheme)).changed() {
                    return Some(Wish::Notice(self.notice));
                }
            }
            SCMDB_ROW => {
                if row.add(widgets::Toggle::new(&mut self.scmdb, scheme)).changed() {
                    return Some(Wish::Scmdb(self.scmdb));
                }
            }
            _ if !desktop_entry::available() => reason(row, "s_ei_nur_linux"),
            _ => {
                let mut wish = None;
                row.horizontal_wrapped(|row| {
                    if self.menu_present {
                        row.label(RichText::new(t("s_menue_steht")).color(color(scheme.sub)));
                    }
                    let (away, make) = (t("s_menue_weg"), t("s_menue_anlegen"));
                    let away = widgets::Button::new(&away, ButtonKind::Normal, scheme);
                    if self.menu_present && row.add(away).clicked() {
                        wish = Some(Wish::Menu(false));
                    }
                    let make = widgets::Button::new(&make, ButtonKind::Normal, scheme);
                    if row.add(make).clicked() {
                        wish = Some(Wish::Menu(true));
                    }
                });
                return wish;
            }
        }
        None
    }

    /// Does what a click asked for and gives what is said about it. The entry in the menu is written only for a
    /// machine (`host`, the program on real data); the switches are stored with the other settings.
    pub fn apply(&mut self, wish: Wish, host: Option<&Host>) -> Option<String> {
        let state = |on: bool| t(if on { "e_an" } else { "e_aus" });
        match wish {
            Wish::Tray(_) => None,
            Wish::Notice(on) => Some(format!("{}: {}", t("s_systemmeldung"), state(on))),
            // The overlay runs the bridge; here only who holds the port is asked. Another watcher that answers there
            // means the port is in use.
            Wish::Scmdb(on) => {
                let busy = on
                    && host
                        .and_then(|h| scmdb_bridge::who_answers(h, std::time::Duration::from_millis(300)))
                        .is_some_and(|client| client != scmdb_bridge::CLIENT);
                let said = if !on {
                    t("e_aus")
                } else if busy {
                    t("s_scmdb_belegt")
                } else {
                    t("s_scmdb_bereit")
                };
                Some(format!("{}: {said}", t("s_scmdb")))
            }
            Wish::Menu(make) => {
                let host = host?;
                let said = if make {
                    match desktop_entry::create(host) {
                        Ok(file) => tf("as_menue_da", &[("0", Arg::from(file.to_string_lossy().as_ref()))]),
                        Err(why) => tf("as_menue_nein", &[("0", Arg::from(why.as_str()))]),
                    }
                } else if desktop_entry::remove(host) {
                    t("s_menue_weg_ok")
                } else {
                    tf("as_menue_nein", &[("0", Arg::from(desktop_entry::FILE_NAME))])
                };
                self.menu_present = desktop_entry::present(host);
                Some(said)
            }
        }
    }

    /// Below the hotkey: why it does not reach everywhere under Wayland, the command for a shortcut of the
    /// desktop, and a button that copies it. True when it was copied.
    pub fn shortcut_hint(&self, ui: &mut Ui, scheme: &Scheme) -> bool {
        let Some(command) = &self.shortcut else { return false };
        ui.add_space(4.0);
        widgets::hint(ui, scheme, &t("lx_hk_wayland"));
        widgets::hint(ui, scheme, &t("lx_hk_kde"));
        let mut copied = false;
        ui.horizontal_wrapped(|row| {
            row.label(RichText::new(command).monospace().color(color(scheme.fg)));
            let label = t("lx_hk_kopieren");
            let copy = widgets::Button::new(&label, ButtonKind::Normal, scheme).icon("copy");
            if row.add(copy).clicked() {
                row.ctx().copy_text(command.clone());
                copied = true;
            }
        });
        copied
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use versekit_core::language;
    use versekit_core::pypath::Flavor;
    use versekit_core::testdir::Dir;

    #[test]
    fn the_switches_are_stored_under_the_keys_the_overlay_reads() {
        let rows = DesktopRows { tray: false, notice: true, scmdb: true, ..DesktopRows::default() };
        assert_eq!(
            rows.setting_values(),
            [
                (overlay::keys::TRAY, Value::Bool(false)),
                (overlay::keys::SYSTEM_NOTICE, Value::Bool(true)),
                (scmdb_bridge::SETTING, Value::Bool(true)),
            ]
        );
        let fresh = DesktopRows::default();
        assert!(fresh.tray && fresh.notice, "both on by default, as the overlay reads them");
        assert!(!fresh.scmdb, "the bridge to scmdb.net is off by default");
    }

    #[test]
    fn the_entry_in_the_menu_is_written_and_taken_away_only_for_a_machine() {
        let _language = crate::language_lock();
        language::set_language(language::Choice::De);
        let dir = Dir::new("allgemein-menue");
        let data = dir.path().to_string_lossy().into_owned();
        let host = Host::new(Flavor::Posix, &[("XDG_DATA_HOME", &data), ("HOME", &data)], &data);
        let mut rows = DesktopRows::default();
        assert_eq!(rows.apply(Wish::Menu(true), None), None, "the sample page writes nothing");
        let file = desktop_entry::target_file(dir.path());
        assert!(!file.exists());
        let said = rows.apply(Wish::Menu(true), Some(&host)).unwrap();
        if desktop_entry::available() {
            assert!(file.is_file(), "{said}");
            assert_eq!(said, tf("as_menue_da", &[("0", Arg::from(file.to_string_lossy().as_ref()))]));
            assert!(rows.menu_present);
            assert_eq!(rows.apply(Wish::Menu(false), Some(&host)).unwrap(), t("s_menue_weg_ok"));
            assert!(!file.exists() && !rows.menu_present);
        } else {
            assert!(!file.exists(), "nothing is written where there are no such menus");
            assert!(said.starts_with(&tf("as_menue_nein", &[("0", Arg::from(""))])), "{said}");
        }
    }

    #[test]
    fn the_switch_of_the_message_says_its_state_and_the_icon_says_nothing() {
        let _language = crate::language_lock();
        language::set_language(language::Choice::De);
        let mut rows = DesktopRows::default();
        assert_eq!(rows.apply(Wish::Notice(false), None).unwrap(), format!("{}: {}", t("s_systemmeldung"), t("e_aus")));
        assert_eq!(rows.apply(Wish::Tray(true), None), None);
    }

    /// The switch of the bridge says off, ready, or that another watcher holds the port.
    #[test]
    fn the_switch_of_the_bridge_says_whether_the_port_is_taken_by_another_watcher() {
        // The texts of this thread only: other tests switch the language of the whole program meanwhile.
        language::set_thread_language(language::Choice::De);
        let mut rows = DesktopRows::default();
        let said = |rows: &mut DesktopRows, on, host| rows.apply(Wish::Scmdb(on), host).unwrap();
        assert_eq!(said(&mut rows, false, None), format!("{}: {}", t("s_scmdb"), t("e_aus")));
        assert_eq!(said(&mut rows, true, None), format!("{}: {}", t("s_scmdb"), t("s_scmdb_bereit")));
        // Another watcher on the port: it answers every /ping without our name, until the test is done.
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port().to_string();
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let other = std::thread::spawn({
            let done = done.clone();
            move || {
                use std::io::{Read, Write};
                for stream in listener.incoming() {
                    if done.load(std::sync::atomic::Ordering::SeqCst) {
                        break;
                    }
                    let Ok(mut stream) = stream else { continue };
                    let mut buffer = [0u8; 1024];
                    let _ = stream.read(&mut buffer);
                    let body = "{\"status\":\"ok\",\"version\":\"0.1.9\",\"channel\":\"LIVE\"}";
                    let answer = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}", body.len());
                    let _ = stream.write_all(answer.as_bytes());
                }
            }
        });
        let host = Host::new(Flavor::native(), &[(scmdb_bridge::ENV_PORT, port.as_str())], ".");
        // The page waits a moment only for the answer; on a busy machine the watcher of the test may come too
        // late now and then: asked again, it is there.
        let taken = format!("{}: {}", t("s_scmdb"), t("s_scmdb_belegt"));
        let answers: Vec<String> = (0..10).map(|_| said(&mut rows, true, Some(&host))).take_while(|s| *s != taken).collect();
        assert!(answers.len() < 10, "another watcher on the port is never told: {answers:?}");
        done.store(true, std::sync::atomic::Ordering::SeqCst);
        // One more call ends the wait of the watcher for the next request.
        let _ = std::net::TcpStream::connect(("127.0.0.1", port.parse::<u16>().unwrap()));
        other.join().unwrap();
    }

    #[test]
    fn on_real_data_each_row_says_what_this_system_can_and_reads_its_setting() {
        let _language = crate::language_lock();
        language::set_language(language::Choice::De);
        let file = "{\n  \"tray\": false,\n  \"systemmeldung\": false\n}\n";
        let (mut page, dir) = super::super::real_data::real_page("allgemein-system", file, &[]);
        assert!(!page.desktop.tray && !page.desktop.notice, "read under the keys of the overlay");
        page.go_to("hf_allgemein");
        let texts = super::super::docking::tests::draw(&mut page, 1280.0, 2400.0).1;
        let row_of = |name: &str| texts.iter().find(|(text, _)| text == name).map(|(_, at)| at.y);
        // A text in the row of `name`: on the same height, give or take a line.
        let in_row = |name: &str, part: &str| {
            let y = row_of(name).unwrap_or_else(|| panic!("{name} not shown: {texts:?}"));
            texts.iter().any(|(text, at)| text.contains(part) && (at.y - y).abs() < 24.0)
        };
        assert_eq!(in_row(&t("s_tray"), &t("s_nur_win")), !cfg!(windows), "the icon: the reason off Windows");
        let message = cfg!(windows) || notify::available();
        assert_eq!(in_row(&t("s_systemmeldung"), &t("s_systemmeldung_fehlt")), !message, "notify-send missing");
        // The entry in the application menu: a row with its button where there are such menus, no row elsewhere.
        let menus = desktop_entry::available();
        assert_eq!(row_of(&t("s_menue")).is_some(), menus, "the row only where there are such menus: {texts:?}");
        if menus {
            assert!(in_row(&t("s_menue"), &t("s_menue_anlegen")), "the button where there are such menus");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    /// One click on the switch of the message of the system (row 1), as a person makes it: what the row asks for
    /// and the state of the switch afterwards.
    fn click_the_message(rows: &mut DesktopRows) -> (Option<Wish>, bool) {
        /// One frame of the row: where it stands and what it asked for.
        fn frame(ctx: &egui::Context, rows: &mut DesktopRows, events: Vec<egui::Event>) -> (egui::Rect, Option<Wish>) {
            let scheme = versekit_core::theme::by_id("original").expect("original");
            let input = egui::RawInput { events, ..Default::default() };
            let mut shown = (egui::Rect::NOTHING, None);
            let mut output = ctx.run_ui(input, |ui| {
                let row = ui.scope(|ui| rows.control(ui, scheme, 1));
                shown = (row.response.rect, row.inner);
            });
            output.textures_delta.clear();
            shown
        }
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let at = frame(&ctx, rows, Vec::new()).0.center();
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let mut wish = None;
        for events in [vec![egui::Event::PointerMoved(at)], vec![button(true)], vec![button(false)], Vec::new()] {
            wish = wish.or(frame(&ctx, rows, events).1);
        }
        (wish, rows.notice)
    }

    /// On Windows the message of the system is a balloon at the icon next to the clock: without the icon its
    /// switch is grey and a click does nothing; with the icon the click switches it. Elsewhere the icon does not
    /// matter.
    #[test]
    fn without_the_icon_on_windows_the_switch_of_the_message_is_grey_and_does_nothing() {
        let _language = crate::language_lock();
        language::set_language(language::Choice::De);
        let mut grey = DesktopRows { tray: false, notice: true, windows: true, ..DesktopRows::default() };
        assert!(!grey.notice_reachable());
        assert_eq!(click_the_message(&mut grey), (None, true), "a grey switch keeps its state");
        let mut with_icon = DesktopRows { tray: true, notice: true, windows: true, ..DesktopRows::default() };
        assert_eq!(click_the_message(&mut with_icon), (Some(Wish::Notice(false)), false), "the click lands");
        let mut linux = DesktopRows { tray: false, notice: true, windows: false, ..DesktopRows::default() };
        assert!(linux.notice_reachable(), "off Windows the icon does not matter");
        assert_eq!(click_the_message(&mut linux), (Some(Wish::Notice(false)), false));
    }

    #[test]
    fn every_text_of_the_rows_is_there_in_both_languages() {
        for key in [
            "s_tray",
            "s_tray_h",
            "s_tray_h_ohne",
            "s_nur_win",
            "s_systemmeldung",
            "s_systemmeldung_h",
            "s_systemmeldung_h_linux",
            "s_systemmeldung_braucht",
            "s_systemmeldung_fehlt",
            "s_menue",
            "s_menue_h",
            "s_menue_anlegen",
            "s_menue_weg",
            "s_menue_steht",
            "s_menue_weg_ok",
            "s_ei_nur_linux",
            "s_scmdb",
            "s_scmdb_h",
            "s_scmdb_bereit",
            "s_scmdb_belegt",
            "lx_hk_wayland",
            "lx_hk_kde",
            "lx_hk_kopieren",
        ] {
            for lang in [language::Lang::De, language::Lang::En] {
                assert!(language::text_in(lang, key).is_some_and(|text| !text.is_empty()), "{key} {lang:?}");
            }
        }
    }
}
