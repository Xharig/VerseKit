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

//! The environment variables the program reads, by their names. A variable under the name of the previous
//! version still counts where the new one is not set: [`crate::paths::Host::current`] and [`process_var`] take it
//! over ([`crate::legacy::OLD_ENV_NAMES`]).

/// A data folder of its own (test runs, special cases); everything in it lies flat.
pub const ENV_HOME: &str = "VERSEKIT_HOME";

/// The Basetool connection. The test environment (sandbox) is chosen only through these, never in the user
/// interface: another issuer, and only together with it another API address, client id and certificate file.
pub const ENV_BASETOOL_ISSUER: &str = "VERSEKIT_BASETOOL_ISSUER";
pub const ENV_BASETOOL_API: &str = "VERSEKIT_BASETOOL_API";
pub const ENV_BASETOOL_CLIENT: &str = "VERSEKIT_BASETOOL_CLIENT";
pub const ENV_BASETOOL_CA: &str = "VERSEKIT_BASETOOL_CA";
/// The name of the Windows key, so test runs never touch the player's key.
pub const ENV_BASETOOL_KEYNAME: &str = "VERSEKIT_BASETOOL_KEYNAME";

/// No network: only the own computer (127.0.0.1) stays reachable.
pub const ENV_NO_NET: &str = "VERSEKIT_NO_NET";

/// Another address for error reports (test runs); anything that is not an `https://` address, or `aus`, turns
/// sending off.
pub const ENV_REPORT_TARGET: &str = "VERSEKIT_BERICHT_ZIEL";

/// The folder of the stored secrets, for test runs.
pub const ENV_SECRETS: &str = "VERSEKIT_SECRETS";
/// Secrets in a file even where the Secret Service is there, for test runs.
pub const ENV_SECRETS_FILE: &str = "VERSEKIT_SECRETS_FILE";

/// The language of this run (`de` or `en`), before the one of the system.
pub const ENV_LANGUAGE: &str = "VERSEKIT_SPRACHE";

/// A name only: the build decides whether it is a test build (`edition::IS_TEST_BUILD`), no variable makes one. The
/// tests use the name to show that neither it nor its old name has an effect.
pub const ENV_TEST_BUILD: &str = "VERSEKIT_TESTFASSUNG";

/// The variable `name` of this process; where it is not set, the one under the name of the previous version.
pub fn process_var(name: &str) -> Option<String> {
    std::env::var(name).ok().or_else(|| std::env::var(crate::legacy::old_env_name(name)?).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_starts_with_the_name_of_the_program() {
        for name in [
            ENV_HOME,
            ENV_BASETOOL_ISSUER,
            ENV_BASETOOL_API,
            ENV_BASETOOL_CLIENT,
            ENV_BASETOOL_CA,
            ENV_BASETOOL_KEYNAME,
            ENV_NO_NET,
            ENV_REPORT_TARGET,
            ENV_SECRETS,
            ENV_SECRETS_FILE,
            ENV_LANGUAGE,
            ENV_TEST_BUILD,
        ] {
            assert!(name.starts_with("VERSEKIT_"), "{name}");
            assert!(crate::legacy::old_env_name(name).is_some(), "{name} has no old name to fall back to");
        }
    }
}
