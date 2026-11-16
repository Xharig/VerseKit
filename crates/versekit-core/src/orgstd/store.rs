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

//! The org connections with their tokens, in one file in the user's own
//! folder: `%LOCALAPPDATA%\VerseKit\orga` (Windows, sealed with DPAPI to the
//! Windows user) or `$XDG_CONFIG_HOME/versekit/orga` (`0600` in a `0700`
//! folder). `VERSEKIT_ORGA_ORDNER` names another folder.
//!
//! ⚠⚠ Never in the data folder: it often lies in a cloud or on a shared disk
//! and goes into the backup and the error report.

use std::io::Write;
use std::sync::Mutex;

use serde_json::{Value, json};

use crate::orgstd::discovery::Discovery;
use crate::orgstd::{E_STORE, OrgError};
use crate::paths::Host;
use crate::writeguard;

/// The variable that names another folder for the connections.
pub const ENV_FOLDER: &str = "VERSEKIT_ORGA_ORDNER";
/// The last part of the own folder of the connections.
pub const FOLDER_LEAF: &str = "orga";
/// The file outside Windows.
pub const PLAIN_FILE: &str = "verbindungen.json";
/// The file under Windows, sealed with DPAPI.
pub const SEALED_FILE: &str = "verbindungen.dpapi";

/// One connection to an org server.
#[derive(Clone, PartialEq, Eq)]
pub struct Connection {
    pub discovery: Discovery,
    /// The member's name as the org server gives it.
    pub member: String,
    /// The rights the member granted.
    pub scopes: Vec<String>,
    pub access_token: String,
    /// Seconds since 1970 at which the access token runs out.
    pub access_expires: u64,
    pub refresh_token: String,
    pub connected_at: u64,
    /// The last successful check, seconds since 1970.
    pub checked_at: Option<u64>,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection")
            .field("id", &self.id())
            .field("member", &self.member)
            .field("scopes", &self.scopes)
            .field("access_expires", &self.access_expires)
            .field("tokens", &"<hidden>")
            .finish()
    }
}

impl Connection {
    /// The address of the org, which names the connection.
    pub fn id(&self) -> String {
        self.discovery.origin.url()
    }

    pub fn to_json(&self) -> Value {
        json!({
            "entdeckung": self.discovery.to_json(),
            "mitglied": self.member,
            "rechte": self.scopes,
            "zugang": self.access_token,
            "zugang_bis": self.access_expires,
            "erneuerung": self.refresh_token,
            "verbunden_seit": self.connected_at,
            "geprueft": self.checked_at,
        })
    }

    pub fn from_json(value: &Value) -> Option<Connection> {
        let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
        Some(Connection {
            discovery: Discovery::from_json(value.get("entdeckung")?)?,
            member: text("mitglied").unwrap_or_default(),
            scopes: value
                .get("rechte")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
            access_token: text("zugang").unwrap_or_default(),
            access_expires: value.get("zugang_bis").and_then(Value::as_u64).unwrap_or(0),
            refresh_token: text("erneuerung")?,
            connected_at: value.get("verbunden_seit").and_then(Value::as_u64).unwrap_or(0),
            checked_at: value.get("geprueft").and_then(Value::as_u64),
        })
    }
}

/// The own folder of the connections ([module](self)). Neither moves with the data folder nor into the backup.
pub fn folder(host: &Host) -> String {
    let join = |base: &str, parts: &[&str]| {
        let mut path = std::path::PathBuf::from(base);
        for part in parts {
            path.push(part);
        }
        path.to_string_lossy().into_owned()
    };
    if let Some(custom) = host.var_set(ENV_FOLDER) {
        return custom.to_string();
    }
    if cfg!(windows) {
        let base = host.var_set("LOCALAPPDATA").map_or_else(|| host.home(), str::to_string);
        return join(&base, &["VerseKit", FOLDER_LEAF]);
    }
    let config = host.var_set("XDG_CONFIG_HOME").map_or_else(|| join(&host.home(), &[".config"]), str::to_string);
    join(&config, &["versekit", FOLDER_LEAF])
}

/// Where the connections are kept: a file, or memory only (the preview, tests).
pub struct Store {
    file: Option<String>,
    memory: Mutex<Vec<Connection>>,
}

fn failed(_: impl std::fmt::Display) -> OrgError {
    OrgError::new(E_STORE)
}

impl Store {
    /// In memory only, starting with `connections`.
    pub fn memory(connections: Vec<Connection>) -> Store {
        Store { file: None, memory: Mutex::new(connections) }
    }

    /// In the file of `folder`.
    pub fn in_folder(folder: &str) -> Store {
        let name = if cfg!(windows) { SEALED_FILE } else { PLAIN_FILE };
        let file = std::path::Path::new(folder).join(name).to_string_lossy().into_owned();
        Store { file: Some(file), memory: Mutex::new(Vec::new()) }
    }

    /// In the own folder of this machine.
    pub fn system(host: &Host) -> Store {
        Store::in_folder(&folder(host))
    }

    /// The file, `None` in memory.
    pub fn file(&self) -> Option<&str> {
        self.file.as_deref()
    }

    /// The stored connections; a missing file is none.
    pub fn load(&self) -> Result<Vec<Connection>, OrgError> {
        let Some(file) = &self.file else {
            return Ok(self.memory.lock().map_err(failed)?.clone());
        };
        let bytes = match std::fs::read(file) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(failed(e)),
        };
        #[cfg(windows)]
        let bytes = crate::basetool::secrets::dpapi::unprotect(&bytes).map_err(failed)?;
        let root: Value = serde_json::from_slice(&bytes).map_err(failed)?;
        if root.get("format").and_then(Value::as_u64) != Some(1) {
            return Err(OrgError::new(E_STORE));
        }
        Ok(root
            .get("verbindungen")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Connection::from_json)
            .collect())
    }

    /// Stores `connections` in place of the ones before; none left removes the file.
    pub fn save(&self, connections: &[Connection]) -> Result<(), OrgError> {
        let Some(file) = &self.file else {
            *self.memory.lock().map_err(failed)? = connections.to_vec();
            return Ok(());
        };
        if connections.is_empty() {
            if std::path::Path::new(file).exists() {
                writeguard::check("remove", file).map_err(failed)?;
                std::fs::remove_file(file).map_err(failed)?;
            }
            return Ok(());
        }
        let list: Vec<Value> = connections.iter().map(Connection::to_json).collect();
        let bytes = json!({ "format": 1, "verbindungen": list }).to_string().into_bytes();
        #[cfg(windows)]
        let bytes = crate::basetool::secrets::dpapi::protect(&bytes).map_err(failed)?;
        write_private(file, &bytes)
    }
}

/// A new file readable by the user only, written to a side file first, then renamed.
fn write_private(file: &str, data: &[u8]) -> Result<(), OrgError> {
    let path = std::path::Path::new(file);
    let folder = path.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    writeguard::check("mkdir", &folder).map_err(failed)?;
    std::fs::create_dir_all(&folder).map_err(failed)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        writeguard::check("chmod", &folder).map_err(failed)?;
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).map_err(failed)?;
    }
    let temp = format!("{file}.neu");
    for (op, target) in [("remove", temp.as_str()), ("write", temp.as_str())] {
        writeguard::check(op, target).map_err(failed)?;
    }
    let _ = std::fs::remove_file(&temp);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let result = (|| {
        let mut out = options.open(&temp)?;
        out.write_all(data)?;
        out.sync_all()?;
        drop(out);
        crate::jsonio::os_replace(&temp, file)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result.map_err(failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orgstd::discovery::Origin;
    use crate::pypath::Flavor;

    fn sample(origin: &str) -> Connection {
        let origin = Origin::parse(origin).unwrap();
        let base = origin.url();
        Connection {
            discovery: Discovery {
                origin,
                name: "Beispiel".into(),
                tag: "BSP".into(),
                homepage: None,
                version: "1".into(),
                api_base: format!("{base}/versekit/v1"),
                device_endpoint: format!("{base}/versekit/v1/device/code"),
                token_endpoint: format!("{base}/versekit/v1/token"),
                revocation_endpoint: Some(format!("{base}/versekit/v1/revoke")),
                scopes: vec!["stock:read".into()],
            },
            member: "Pilot".into(),
            scopes: vec!["stock:read".into()],
            access_token: "vka_geheim_zugang".into(),
            access_expires: 100,
            refresh_token: "vkr_geheim_erneuerung".into(),
            connected_at: 50,
            checked_at: None,
        }
    }

    #[test]
    fn the_folder_is_the_users_own_and_can_be_named() {
        let host = Host::new(Flavor::native(), &[("LOCALAPPDATA", "L"), ("XDG_CONFIG_HOME", "X"), ("HOME", "H")], "/");
        let expected = if cfg!(windows) { ["L", "VerseKit", "orga"] } else { ["X", "versekit", "orga"] };
        assert_eq!(folder(&host), expected.iter().collect::<std::path::PathBuf>().to_string_lossy());
        let named = Host::new(Flavor::native(), &[(ENV_FOLDER, "eigen")], "/");
        assert_eq!(folder(&named), "eigen");
    }

    #[test]
    fn connections_go_to_the_file_and_back_and_debug_hides_the_tokens() {
        let dir = crate::testdir::Dir::new("orga-ablage");
        let store = Store::in_folder(&dir.path().join("orga").to_string_lossy());
        assert_eq!(store.load(), Ok(Vec::new()));
        let one = sample("https://orga.example.org");
        store.save(std::slice::from_ref(&one)).unwrap();
        assert_eq!(store.load(), Ok(vec![one.clone()]));
        let raw = std::fs::read(store.file().unwrap()).unwrap();
        let readable = String::from_utf8_lossy(&raw).contains("vkr_geheim_erneuerung");
        assert_eq!(readable, !cfg!(windows), "sealed under Windows, plain elsewhere");
        let shown = format!("{one:?}");
        assert!(!shown.contains("geheim"), "{shown}");
        store.save(&[]).unwrap();
        assert!(!std::path::Path::new(store.file().unwrap()).exists());
    }
}
