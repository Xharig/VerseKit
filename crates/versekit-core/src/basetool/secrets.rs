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

//! Storing secrets where the system keeps them, never with the data
//! (`secret_store.py`). The refresh token, and outside Windows the DPoP key.
//!
//! | System | Store |
//! |---|---|
//! | Windows | DPAPI (bound to the Windows user), the encrypted file under `%LOCALAPPDATA%\VerseKit\basetool` |
//! | Linux | Secret Service through `secret-tool` (KWallet under KDE) |
//! | Linux without Secret Service, macOS | a `0600` file in a `0700` folder under `~/.config/versekit/basetool`; the page shows it (`uses_file`) |
//!
//! Same places, names and attributes as the previous version, so an
//! existing connection keeps working.
//!
//! ⚠⚠ Never in the data folder: it often lies in a cloud or on a shared
//! disk, goes into the backup archive and into the error report.

use std::collections::HashMap;
use std::fmt;
use std::io::Write;
use std::sync::{Mutex, OnceLock};

use crate::legacy;
use crate::paths::Host;
use crate::writeguard;

/// Name of the refresh token in the store.
pub const REFRESH_SECRET: &str = "refresh-token";
/// Attribute value under which `secret-tool` finds the entries.
pub const APPLICATION: &str = "versekit";
const LABEL: &str = "VerseKit, KRT Profit Basetool";

/// A secret could not be stored or read. `code` tells the page why:
/// `SECRET_REFUSED` (the Secret Service said no), `SECRET_UNSAFE` (file or
/// folder readable by others and not fixable) or `SECRET_FAILED`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecretError {
    pub code: &'static str,
    pub message: String,
}

impl SecretError {
    fn new(code: &'static str, message: impl Into<String>) -> SecretError {
        SecretError { code, message: message.into() }
    }
}

impl fmt::Display for SecretError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for SecretError {}

/// Where secrets are kept.
pub trait SecretStore: Send + Sync {
    /// Stores a secret, replacing one of the same name.
    fn save(&self, name: &str, text: &str) -> Result<(), SecretError>;
    /// The secret, or `None` if there is none.
    fn load(&self, name: &str) -> Result<Option<String>, SecretError>;
    /// Deletes it wherever it may lie; a missing one is no error.
    fn delete(&self, name: &str) -> Result<(), SecretError>;
    /// Whether a secret lies in a plain file; the page says so.
    fn uses_file(&self) -> bool;
}

/// Secrets in memory, for tests.
#[derive(Default)]
pub struct MemorySecrets {
    entries: Mutex<HashMap<String, String>>,
}

impl SecretStore for MemorySecrets {
    fn save(&self, name: &str, text: &str) -> Result<(), SecretError> {
        self.entries.lock().map_err(|_| SecretError::new("SECRET_FAILED", "lock"))?.insert(name.into(), text.into());
        Ok(())
    }

    fn load(&self, name: &str) -> Result<Option<String>, SecretError> {
        Ok(self.entries.lock().map_err(|_| SecretError::new("SECRET_FAILED", "lock"))?.get(name).cloned())
    }

    fn delete(&self, name: &str) -> Result<(), SecretError> {
        self.entries.lock().map_err(|_| SecretError::new("SECRET_FAILED", "lock"))?.remove(name);
        Ok(())
    }

    fn uses_file(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Dpapi,
    SecretService,
    File,
}

/// The store of this system.
pub struct SystemSecrets {
    folder: String,
    secret_tool: Option<String>,
    backend: OnceLock<Backend>,
    /// Where each secret last really came from or went to.
    origin: Mutex<HashMap<String, Backend>>,
}

/// `secret-tool` on the search path, unless test runs ask for the file.
fn find_secret_tool(host: &Host) -> Option<String> {
    if cfg!(windows) || host.var(legacy::ENV_SECRETS_FILE).is_some_and(|v| !v.is_empty() && v != "0") {
        return None;
    }
    let path = host.var("PATH")?;
    std::env::split_paths(path)
        .map(|dir| dir.join("secret-tool"))
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
}

impl SystemSecrets {
    pub fn from_host(host: &Host) -> SystemSecrets {
        SystemSecrets {
            folder: folder(host),
            secret_tool: find_secret_tool(host),
            backend: OnceLock::new(),
            origin: Mutex::new(HashMap::new()),
        }
    }

    pub fn folder(&self) -> &str {
        &self.folder
    }

    /// DPAPI, Secret Service or file; decided once.
    pub fn backend(&self) -> Backend {
        *self.backend.get_or_init(|| {
            if cfg!(windows) {
                Backend::Dpapi
            } else if self.secret_service_ok() {
                Backend::SecretService
            } else {
                Backend::File
            }
        })
    }

    /// Whether the Secret Service answers: a lookup without a hit ends with
    /// 1 and without a message; a missing service writes one.
    fn secret_service_ok(&self) -> bool {
        let Some(tool) = &self.secret_tool else { return false };
        match run(tool, &["lookup", "application", APPLICATION, "name", "__probe__"], None) {
            Ok(out) => matches!(out.status.code(), Some(0 | 1)) && out.stderr.iter().all(u8::is_ascii_whitespace),
            Err(_) => false,
        }
    }

    fn file(&self, name: &str) -> String {
        let safe: String = name.chars().filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.')).collect();
        let ending = if cfg!(windows) { ".dpapi" } else { ".secret" };
        std::path::Path::new(&self.folder).join(format!("{safe}{ending}")).to_string_lossy().into_owned()
    }

    fn remember(&self, name: &str, backend: Backend) {
        if let Ok(mut origin) = self.origin.lock() {
            origin.insert(name.to_string(), backend);
        }
    }

    fn ensure_folder(&self) -> Result<(), SecretError> {
        writeguard::check("mkdir", &self.folder).map_err(|e| SecretError::new("SECRET_FAILED", e.to_string()))?;
        std::fs::create_dir_all(&self.folder).map_err(|e| SecretError::new("SECRET_FAILED", e.to_string()))?;
        tighten(&self.folder, 0o700)
    }

    /// New file with `0600`, written to a side file first, then renamed.
    fn write_private(&self, path: &str, data: &[u8]) -> Result<(), SecretError> {
        self.ensure_folder()?;
        let temp = format!("{path}.neu");
        let failed = |e: std::io::Error| SecretError::new("SECRET_FAILED", e.to_string());
        for (op, target) in [("remove", temp.as_str()), ("write", temp.as_str()), ("replace", path)] {
            writeguard::check(op, target).map_err(failed)?;
        }
        let _ = std::fs::remove_file(&temp);
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let result = (|| {
            let mut file = options.open(&temp)?;
            file.write_all(data)?;
            file.sync_all()?;
            drop(file);
            crate::jsonio::os_replace(&temp, path)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        result.map_err(failed)
    }
}

/// The own folder of the secrets: `SC_BP_SECRETS`, otherwise under
/// `%LOCALAPPDATA%` (Windows) or `$XDG_CONFIG_HOME`/`~/.config`. Neither moves
/// with the data folder or into the backup.
///
/// ⚠ `%LOCALAPPDATA%`, not `%APPDATA%`: the latter roams with the profile in
/// company networks, and the key must never leave the computer.
pub fn folder(host: &Host) -> String {
    let join = |base: &str, parts: &[&str]| {
        let mut path = std::path::PathBuf::from(base);
        for part in parts {
            path.push(part);
        }
        path.to_string_lossy().into_owned()
    };
    if let Some(custom) = host.var_set(legacy::ENV_SECRETS) {
        return custom.to_string();
    }
    if cfg!(windows) {
        let base = host.var_set("LOCALAPPDATA").map_or_else(|| host.home(), str::to_string);
        return join(&base, &["VerseKit", "basetool"]);
    }
    let config = host.var_set("XDG_CONFIG_HOME").map_or_else(|| join(&host.home(), &[".config"]), str::to_string);
    join(&config, &["versekit", "basetool"])
}

/// How long `secret-tool` may take; a locked wallet asks for its password
/// in a window that may lie behind the game.
pub const TOOL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

fn run(tool: &str, args: &[&str], input: Option<&[u8]>) -> std::io::Result<std::process::Output> {
    use std::process::{Command, Stdio};
    let mut child = Command::new(tool)
        .args(args)
        .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let (Some(data), Some(mut stdin)) = (input, child.stdin.take()) {
        stdin.write_all(data)?;
    }
    let pid = child.id();
    let (send, answer) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = send.send(child.wait_with_output());
    });
    match answer.recv_timeout(TOOL_TIMEOUT) {
        Ok(output) => output,
        Err(_) => {
            #[cfg(unix)]
            // SAFETY: `kill` only sends a signal; the process is our child.
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGKILL);
            }
            #[cfg(not(unix))]
            let _ = pid;
            Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "secret-tool did not answer"))
        }
    }
}

/// Folder or file on `mode`, or refused: a symbolic link, another owner or
/// rights for others that cannot be removed make it unusable.
#[cfg(unix)]
fn tighten(path: &str, mode: u32) -> Result<(), SecretError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let unsafe_ = |what: String| SecretError::new("SECRET_UNSAFE", format!("{name}: {what}"));
    let info = match std::fs::symlink_metadata(path) {
        Ok(info) => info,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(unsafe_(e.to_string())),
    };
    if info.file_type().is_symlink() {
        return Err(unsafe_("symbolic link".into()));
    }
    // SAFETY: `getuid` has no preconditions and cannot fail.
    if info.uid() != unsafe { libc::getuid() } {
        return Err(unsafe_("owned by another user".into()));
    }
    if info.mode() & 0o077 != 0 {
        writeguard::check("chmod", path).map_err(|e| unsafe_(e.to_string()))?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).map_err(|e| unsafe_(e.to_string()))?;
        let again = std::fs::symlink_metadata(path).map_err(|e| unsafe_(e.to_string()))?;
        if again.mode() & 0o077 != 0 {
            return Err(unsafe_("readable by others".into()));
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn tighten(_path: &str, _mode: u32) -> Result<(), SecretError> {
    Ok(())
}

impl SecretStore for SystemSecrets {
    fn save(&self, name: &str, text: &str) -> Result<(), SecretError> {
        match self.backend() {
            Backend::Dpapi => {
                let sealed = dpapi::protect(text.as_bytes())?;
                self.write_private(&self.file(name), &sealed)?;
            }
            Backend::SecretService => {
                writeguard::check("secret-store", name)
                    .map_err(|e| SecretError::new("SECRET_FAILED", e.to_string()))?;
                let tool = self.secret_tool.as_deref().unwrap_or("secret-tool");
                let label = format!("--label={LABEL} ({name})");
                let out =
                    run(tool, &["store", &label, "application", APPLICATION, "name", name], Some(text.as_bytes()))
                        .map_err(|e| SecretError::new("SECRET_REFUSED", format!("secret-tool: {e}")))?;
                if !out.status.success() {
                    // Refused (locked wallet, cancelled prompt): no silent
                    // way into the file; the player learns it.
                    return Err(SecretError::new(
                        "SECRET_REFUSED",
                        format!("secret-tool store: {}", out.status.code().unwrap_or(-1)),
                    ));
                }
            }
            Backend::File => self.write_private(&self.file(name), text.as_bytes())?,
        }
        self.remember(name, self.backend());
        Ok(())
    }

    fn load(&self, name: &str) -> Result<Option<String>, SecretError> {
        let backend = self.backend();
        if backend == Backend::SecretService {
            let tool = self.secret_tool.as_deref().unwrap_or("secret-tool");
            let out = run(tool, &["lookup", "application", APPLICATION, "name", name], None)
                .map_err(|e| SecretError::new("SECRET_FAILED", format!("secret-tool: {e}")))?;
            if out.status.success() && !out.stdout.is_empty() {
                self.remember(name, Backend::SecretService);
                return Ok(Some(String::from_utf8_lossy(&out.stdout).into_owned()));
            }
        }
        let path = self.file(name);
        if !std::path::Path::new(&path).exists() {
            return Ok(None);
        }
        tighten(&self.folder, 0o700)?;
        tighten(&path, 0o600)?;
        let data = match std::fs::read(&path) {
            Ok(data) => data,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(SecretError::new("SECRET_FAILED", e.to_string())),
        };
        let (data, origin) =
            if backend == Backend::Dpapi { (dpapi::unprotect(&data)?, Backend::Dpapi) } else { (data, Backend::File) };
        self.remember(name, origin);
        String::from_utf8(data).map(Some).map_err(|_| SecretError::new("SECRET_FAILED", "not UTF-8"))
    }

    fn delete(&self, name: &str) -> Result<(), SecretError> {
        if let Some(tool) = &self.secret_tool {
            writeguard::check("secret-store", name).map_err(|e| SecretError::new("SECRET_FAILED", e.to_string()))?;
            let _ = run(tool, &["clear", "application", APPLICATION, "name", name], None);
        }
        if let Ok(mut origin) = self.origin.lock() {
            origin.remove(name);
        }
        let path = self.file(name);
        if !std::path::Path::new(&path).exists() {
            return Ok(());
        }
        writeguard::check("remove", &path).map_err(|e| SecretError::new("SECRET_FAILED", e.to_string()))?;
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(SecretError::new("SECRET_FAILED", format!("{name}: {e}"))),
        }
    }

    fn uses_file(&self) -> bool {
        if cfg!(windows) {
            return false;
        }
        let in_file = self.origin.lock().map(|o| o.values().any(|b| *b == Backend::File)).unwrap_or(false);
        in_file || self.backend() == Backend::File
    }
}

#[cfg(windows)]
mod dpapi {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
    };

    use super::SecretError;

    fn call(data: &[u8], protect: bool) -> Result<Vec<u8>, SecretError> {
        let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr().cast_mut() };
        let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        // SAFETY: the input lives through the call; DPAPI allocates the
        // output, which is freed with `LocalFree` below.
        let ok = unsafe {
            if protect {
                CryptProtectData(
                    &input,
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
            } else {
                CryptUnprotectData(
                    &input,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
            }
        };
        if ok == 0 || output.pbData.is_null() {
            let code = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
            return Err(SecretError::new(
                "SECRET_FAILED",
                format!("DPAPI {} fehlgeschlagen ({code})", if protect { "protect" } else { "unprotect" }),
            ));
        }
        // SAFETY: DPAPI wrote `cbData` bytes at `pbData`.
        let bytes = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
        // SAFETY: the buffer came from DPAPI and is freed once.
        unsafe { LocalFree(output.pbData.cast()) };
        Ok(bytes)
    }

    pub fn protect(data: &[u8]) -> Result<Vec<u8>, SecretError> {
        call(data, true)
    }

    pub fn unprotect(data: &[u8]) -> Result<Vec<u8>, SecretError> {
        call(data, false)
    }
}

#[cfg(not(windows))]
mod dpapi {
    use super::SecretError;

    pub fn protect(_data: &[u8]) -> Result<Vec<u8>, SecretError> {
        Err(SecretError::new("SECRET_FAILED", "DPAPI is Windows only"))
    }

    pub fn unprotect(_data: &[u8]) -> Result<Vec<u8>, SecretError> {
        Err(SecretError::new("SECRET_FAILED", "DPAPI is Windows only"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pypath::Flavor;

    fn scratch(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("versekit-secrets-{name}-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.to_string_lossy().into_owned()
    }

    #[test]
    fn folder_never_in_the_data_folder() {
        let host = Host::new(Flavor::Posix, &[("HOME", "/home/spieler"), ("SC_BP_HOME", "/daten")], "/");
        if !cfg!(windows) {
            assert_eq!(folder(&host), "/home/spieler/.config/versekit/basetool");
            let xdg = Host::new(Flavor::Posix, &[("HOME", "/home/spieler"), ("XDG_CONFIG_HOME", "/cfg")], "/");
            assert_eq!(folder(&xdg), "/cfg/versekit/basetool");
        }
        let custom = Host::new(Flavor::Posix, &[(legacy::ENV_SECRETS, "/tmp/s")], "/");
        assert_eq!(folder(&custom), "/tmp/s");
    }

    #[cfg(unix)]
    #[test]
    fn file_store_is_private_and_round_trips() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("file");
        let host = Host::new(Flavor::Posix, &[(legacy::ENV_SECRETS, &dir), (legacy::ENV_SECRETS_FILE, "1")], "/");
        let store = SystemSecrets::from_host(&host);
        assert_eq!(store.backend(), Backend::File);
        assert_eq!(store.load(REFRESH_SECRET).unwrap(), None);
        store.save(REFRESH_SECRET, "geheim-ü").unwrap();
        assert_eq!(store.load(REFRESH_SECRET).unwrap().as_deref(), Some("geheim-ü"));
        let file = format!("{dir}/refresh-token.secret");
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        assert!(store.uses_file());
        // Readable by others: tightened again before reading.
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(store.load(REFRESH_SECRET).unwrap().is_some());
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
        // A symbolic link is never used.
        std::fs::remove_file(&file).unwrap();
        std::os::unix::fs::symlink("/etc/hosts", &file).unwrap();
        assert_eq!(store.load(REFRESH_SECRET).unwrap_err().code, "SECRET_UNSAFE");
        std::fs::remove_file(&file).unwrap();
        store.save(REFRESH_SECRET, "x").unwrap();
        store.delete(REFRESH_SECRET).unwrap();
        store.delete(REFRESH_SECRET).unwrap();
        assert_eq!(store.load(REFRESH_SECRET).unwrap(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn dpapi_seals_and_opens_only_its_own_data() {
        let text = "geheim-ü".as_bytes();
        let sealed = dpapi::protect(text).unwrap();
        assert_ne!(sealed.as_slice(), text, "the sealed bytes are not the text");
        assert!(!sealed.windows(text.len()).any(|w| w == text), "the text does not stand in the sealed bytes");
        assert_eq!(dpapi::unprotect(&sealed).unwrap(), text);
        // A changed last byte, plain text and nothing at all are refused,
        // never opened to something else.
        let mut changed = sealed.clone();
        let last = changed.len() - 1;
        changed[last] ^= 0x5a;
        assert_eq!(dpapi::unprotect(&changed).unwrap_err().code, "SECRET_FAILED");
        assert_eq!(dpapi::unprotect(text).unwrap_err().code, "SECRET_FAILED");
        assert_eq!(dpapi::unprotect(&[]).unwrap_err().code, "SECRET_FAILED");
    }

    #[cfg(windows)]
    #[test]
    fn dpapi_store_round_trips_in_its_own_folder() {
        let dir = scratch("dpapi");
        let host = Host::new(Flavor::native(), &[(legacy::ENV_SECRETS, &dir)], &dir);
        let store = SystemSecrets::from_host(&host);
        assert_eq!(store.backend(), Backend::Dpapi);
        assert_eq!(store.load(REFRESH_SECRET).unwrap(), None);
        store.save(REFRESH_SECRET, "geheim-ü").unwrap();
        let file = std::path::Path::new(&dir).join("refresh-token.dpapi");
        let sealed = std::fs::read(&file).unwrap();
        assert!(!sealed.windows("geheim".len()).any(|w| w == b"geheim"), "no plain text on disk");
        assert_eq!(store.load(REFRESH_SECRET).unwrap().as_deref(), Some("geheim-ü"));
        // A file that was not sealed by DPAPI is refused.
        std::fs::write(&file, b"geheim-ue").unwrap();
        assert_eq!(store.load(REFRESH_SECRET).unwrap_err().code, "SECRET_FAILED");
        store.delete(REFRESH_SECRET).unwrap();
        assert!(!file.exists());
        assert_eq!(store.load(REFRESH_SECRET).unwrap(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_write_lock_holds() {
        let dir = scratch("lock");
        let host = Host::new(Flavor::Posix, &[(legacy::ENV_SECRETS, &dir), (legacy::ENV_SECRETS_FILE, "1")], "/");
        let store = SystemSecrets::from_host(&host);
        writeguard::deny(true);
        let refused = store.save(REFRESH_SECRET, "x");
        writeguard::deny(false);
        assert!(refused.is_err());
        assert!(!std::path::Path::new(&dir).exists());
    }
}
