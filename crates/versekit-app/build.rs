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

//! The Windows program file gets the program icon and its version details (Explorer, task manager, "Apps &
//! Features"); the numbers come from the version of the workspace.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../icon.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        windows_resources();
    }
}

#[cfg(windows)]
fn windows_resources() {
    let version = env!("CARGO_PKG_VERSION");
    let number = |name: &str| std::env::var(name).ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
    let packed = number("CARGO_PKG_VERSION_MAJOR") << 48
        | number("CARGO_PKG_VERSION_MINOR") << 32
        | number("CARGO_PKG_VERSION_PATCH") << 16;
    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon("../../icon.ico")
        .set("FileDescription", "VerseKit")
        .set("ProductName", "VerseKit")
        .set("CompanyName", "Xharig")
        .set("LegalCopyright", "Copyright (C) 2026 Xharig. GPL-3.0-only")
        .set("OriginalFilename", "versekit.exe")
        .set("InternalName", "versekit")
        .set("ProductVersion", version)
        .set("FileVersion", version)
        .set_version_info(winresource::VersionInfo::FILEVERSION, packed)
        .set_version_info(winresource::VersionInfo::PRODUCTVERSION, packed);
    if let Err(error) = resource.compile() {
        panic!("the resources of the program file could not be built: {error}");
    }
}
