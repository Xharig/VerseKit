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

//! The credentials of a copy of the data (`--daten`) stay in the copy: the connection the window makes on it
//! keeps the token in a folder inside the copy, never in the one of the player's own installation.

use versekit_core::basetool::secrets::{self, REFRESH_SECRET};
use versekit_core::legacy::{ENV_NO_NET, ENV_SECRETS, ENV_SECRETS_FILE};
use versekit_core::paths::{COPY_DATA_FOLDER, Host, Paths};

use super::SamplePage;

#[test]
fn on_a_copy_the_credentials_of_the_basetool_stay_in_the_copy() {
    // The folder of the player's own installation is only compared by name: the real program may write there while
    // the test runs.
    let real = secrets::folder(&Host::current());
    let root = versekit_core::testdir::fresh("basetool-geheim-kopie");
    std::fs::create_dir_all(root.join(COPY_DATA_FOLDER)).expect("data folder");
    let text = root.to_string_lossy().into_owned();
    // The local folders of the computer stand apart from the copy, as on a real one; the copy does not move them.
    let local = versekit_core::testdir::fresh("basetool-geheim-lokal");
    let local_text = local.to_string_lossy().into_owned();
    let machine = [("LOCALAPPDATA", local_text.as_str()), ("XDG_CONFIG_HOME", local_text.as_str())];
    let installation = secrets::folder(&Host::new(versekit_core::pypath::Flavor::native(), &machine, &text));
    // No folder for the credentials given: the copy has to find its own. Nothing can go out.
    let mut vars = vec![(ENV_NO_NET, "1")];
    vars.extend_from_slice(&machine);
    let host = Host::new(versekit_core::pypath::Flavor::native(), &vars, &text);
    let paths = Paths::for_copy(host, &text).expect("a copy");
    assert!(paths.host.var_set(ENV_SECRETS).is_none());
    assert!(std::path::Path::new(&installation).starts_with(&local), "{installation}");

    let mut page = SamplePage::new();
    page.connect_system(&paths);
    let wire = page.connection.wire().expect("the connection is made");
    let folder = secrets::folder(&wire.sync.paths().host);
    assert!(std::path::Path::new(&folder).starts_with(&root), "the credentials lie in the copy: {folder}");
    assert_ne!(folder, real);
    assert!(!std::path::Path::new(&folder).starts_with(&real), "not inside the player's own folder: {folder}");
    assert_ne!(folder, installation, "never the folder of the installation on this computer");
    assert_eq!(wire.sync.paths().host.var_set(ENV_SECRETS_FILE), Some("1"));
    // A token the connection keeps lands in that folder.
    wire.sync.connection().secrets().save(REFRESH_SECRET, "token-der-kopie").expect("saved");
    assert!(std::fs::read_dir(&folder).is_ok_and(|mut entries| entries.next().is_some()), "{folder}");
    wire.sync.stop();
    assert!(!std::path::Path::new(&installation).exists(), "nothing was written for the installation");
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(local);
}
