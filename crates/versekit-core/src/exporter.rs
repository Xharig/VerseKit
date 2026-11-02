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

//! Getting the Hangar-Exporter into a browser: one way per browser, all three alike.
//!
//! The installer puts the Firefox file (signed by Mozilla) into the folder [`FOLDER`] next to the program file.
//! Firefox opens that file and asks once whether to add the extension. Chrome and Edge get the release page,
//! where their package lies. Where the browser is not installed, or the file is not there (a build of the
//! source, Linux), the release page opens in the default browser. Nothing is installed by this program.

use std::path::{Path, PathBuf};

use crate::credits::HANGAR_EXPORTER;

/// The folder next to the program file with the Hangar-Exporter of the installer.
pub const FOLDER: &str = "erweiterung";

/// The entry of the Hangar-Exporter in the Chrome Web Store, once it is there; Edge installs from it as well.
/// Until then the buttons of Chrome and Edge open the release page with the package and its instructions.
pub const CHROME_STORE: Option<&str> = None;

/// The page the buttons of Chrome and Edge open.
pub fn chrome_page() -> &'static str {
    CHROME_STORE.unwrap_or(HANGAR_EXPORTER)
}

/// A browser the Hangar-Exporter is offered for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Browser {
    Firefox,
    Chrome,
    Edge,
}

impl Browser {
    pub const ALL: [Browser; 3] = [Browser::Firefox, Browser::Chrome, Browser::Edge];

    /// The text key of its button.
    pub fn label(self) -> &'static str {
        match self {
            Browser::Firefox => "as_exporter_firefox",
            Browser::Chrome => "as_exporter_chrome",
            Browser::Edge => "as_exporter_edge",
        }
    }

    /// The text key of what its button does.
    pub fn hint(self) -> &'static str {
        match self {
            Browser::Firefox => "as_exporter_firefox_h",
            Browser::Chrome | Browser::Edge => "as_exporter_chrome_h",
        }
    }

    /// The program file of the browser under Windows (`App Paths`), and its command under Linux.
    fn program_name(self) -> &'static str {
        match (self, cfg!(windows)) {
            (Browser::Firefox, true) => "firefox.exe",
            (Browser::Chrome, true) => "chrome.exe",
            (Browser::Edge, true) => "msedge.exe",
            (Browser::Firefox, false) => "firefox",
            (Browser::Chrome, false) => "google-chrome",
            (Browser::Edge, false) => "microsoft-edge",
        }
    }

    /// The name of the browser as a Flatpak app.
    fn flatpak_id(self) -> &'static str {
        match self {
            Browser::Firefox => "org.mozilla.firefox",
            Browser::Chrome => "com.google.Chrome",
            Browser::Edge => "com.microsoft.Edge",
        }
    }
}

/// Where a browser that is not on the path lies on Linux: Snap puts its commands into `/snap/bin` (not on the
/// path of every desktop), Flatpak gives every app a command under `exports/bin`, of the user first, then of the
/// system. Both take the address or the file like the browser itself.
pub fn package_commands(browser: Browser, home: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if browser == Browser::Firefox {
        found.push(PathBuf::from("/snap/bin/firefox"));
    }
    let id = browser.flatpak_id();
    found.push(home.join(".local/share/flatpak/exports/bin").join(id));
    found.push(PathBuf::from("/var/lib/flatpak/exports/bin").join(id));
    found
}

/// The Firefox file of the Hangar-Exporter in `folder` (`hangar-exporter-<version>.xpi`; the highest version
/// when there are several), `None` without one.
pub fn bundled_in(folder: &Path) -> Option<PathBuf> {
    let mut found: Vec<(String, PathBuf)> = std::fs::read_dir(folder)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let version = name.strip_prefix("hangar-exporter-")?.strip_suffix(".xpi")?.to_string();
            entry.path().is_file().then_some((version, entry.path()))
        })
        .collect();
    found.sort_by(|a, b| {
        if crate::version::is_newer(&a.0, &b.0) {
            std::cmp::Ordering::Greater
        } else if crate::version::is_newer(&b.0, &a.0) {
            std::cmp::Ordering::Less
        } else {
            a.0.cmp(&b.0)
        }
    });
    found.pop().map(|(_, path)| path)
}

/// The Firefox file next to the running program, `None` without one.
pub fn bundled() -> Option<PathBuf> {
    let program = std::env::current_exe().ok()?;
    bundled_in(&program.parent()?.join(FOLDER))
}

/// What a button opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Opening {
    /// The browser program with the file of the extension.
    File { program: PathBuf, file: PathBuf },
    /// The browser program with the release page.
    Page { program: PathBuf, url: &'static str },
    /// The release page in the default browser.
    Default(&'static str),
}

/// What the button of `browser` opens, given the Firefox file and where the browser is installed.
pub fn plan(browser: Browser, xpi: Option<&Path>, installed: Option<PathBuf>) -> Opening {
    let page = if browser == Browser::Firefox { HANGAR_EXPORTER } else { chrome_page() };
    match (browser, installed, xpi) {
        (Browser::Firefox, Some(program), Some(file)) => Opening::File { program, file: file.to_path_buf() },
        (_, Some(program), _) => Opening::Page { program, url: page },
        (_, None, _) => Opening::Default(page),
    }
}

/// Where the browser is installed: Windows names it under `App Paths` (the user's first), Linux finds its
/// command in `PATH`, else as a Snap or a Flatpak ([`package_commands`]).
pub fn installed(browser: Browser) -> Option<PathBuf> {
    let name = browser.program_name();
    #[cfg(windows)]
    {
        use crate::paths::registry;
        let key = format!(r"Software\Microsoft\Windows\CurrentVersion\App Paths\{name}");
        [registry::HKEY_CURRENT_USER, registry::HKEY_LOCAL_MACHINE]
            .into_iter()
            .filter_map(|root| registry::open(root, &key)?.value(""))
            .map(|value| PathBuf::from(value.trim_matches('"')))
            .find(|path| path.is_file())
    }
    #[cfg(not(windows))]
    {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        std::env::var_os("PATH")
            .map(|paths| std::env::split_paths(&paths).map(|dir| dir.join(name)).collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .chain(package_commands(browser, &home))
            .find(|path| path.is_file())
    }
}

/// Opens what the button of `browser` stands for; false when nothing could be opened.
pub fn open(browser: Browser) -> bool {
    match plan(browser, bundled().as_deref(), installed(browser)) {
        Opening::File { program, file } => start(&program, file.as_os_str()),
        Opening::Page { program, url } => start(&program, std::ffi::OsStr::new(url)),
        Opening::Default(url) => crate::browser::open_in_browser(url),
    }
}

fn start(program: &Path, argument: &std::ffi::OsStr) -> bool {
    crate::child::spawn_quiet(std::process::Command::new(program).arg(argument))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdir::Dir;

    #[test]
    fn the_newest_bundled_file_is_found_and_nothing_else() {
        let dir = Dir::new("exporter-datei");
        assert_eq!(bundled_in(dir.path()), None);
        assert_eq!(bundled_in(&dir.path().join("fehlt")), None);
        for name in ["hangar-exporter-0.5.0.xpi", "hangar-exporter-0.10.0.xpi", "fremd.xpi", "LICENSE.txt"] {
            std::fs::write(dir.path().join(name), b"x").unwrap();
        }
        std::fs::create_dir_all(dir.path().join("hangar-exporter-9.9.9.xpi")).unwrap();
        assert_eq!(
            bundled_in(dir.path()),
            Some(dir.path().join("hangar-exporter-0.10.0.xpi")),
            "0.10 over 0.5, no folder"
        );
    }

    #[test]
    fn firefox_gets_the_file_and_the_others_the_release_page() {
        let xpi = PathBuf::from("/p/erweiterung/hangar-exporter-0.5.0.xpi");
        let program = PathBuf::from("/usr/bin/browser");
        assert_eq!(
            plan(Browser::Firefox, Some(&xpi), Some(program.clone())),
            Opening::File { program: program.clone(), file: xpi.clone() }
        );
        assert_eq!(
            plan(Browser::Firefox, None, Some(program.clone())),
            Opening::Page { program: program.clone(), url: HANGAR_EXPORTER }
        );
        for browser in [Browser::Chrome, Browser::Edge] {
            assert_eq!(
                plan(browser, Some(&xpi), Some(program.clone())),
                Opening::Page { program: program.clone(), url: HANGAR_EXPORTER }
            );
        }
        for browser in Browser::ALL {
            assert_eq!(plan(browser, Some(&xpi), None), Opening::Default(HANGAR_EXPORTER), "not installed");
        }
        assert!(crate::browser::web_address(HANGAR_EXPORTER));
    }

    #[test]
    fn a_browser_off_the_path_is_looked_for_as_snap_and_as_flatpak_of_the_user_then_of_the_system() {
        let home = Path::new("/home/someone");
        assert_eq!(
            package_commands(Browser::Firefox, home),
            [
                PathBuf::from("/snap/bin/firefox"),
                home.join(".local/share/flatpak/exports/bin").join("org.mozilla.firefox"),
                PathBuf::from("/var/lib/flatpak/exports/bin/org.mozilla.firefox"),
            ]
        );
        let chrome = package_commands(Browser::Chrome, home);
        assert_eq!(chrome.len(), 2, "Chrome is no Snap: {chrome:?}");
        assert!(chrome.iter().all(|c| c.ends_with("com.google.Chrome")));
        assert!(package_commands(Browser::Edge, home).iter().all(|c| c.ends_with("com.microsoft.Edge")));
    }

    #[test]
    fn every_button_has_its_words_in_both_languages() {
        for browser in Browser::ALL {
            for key in [browser.label(), browser.hint()] {
                for lang in [crate::language::Lang::De, crate::language::Lang::En] {
                    assert!(crate::language::text_in(lang, key).is_some_and(|t| !t.is_empty()), "{key}");
                }
            }
        }
    }
}
