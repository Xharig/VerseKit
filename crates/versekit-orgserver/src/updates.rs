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

//! Whether a newer server is out: at most once a day the git tags of the server in the VerseKit repository on
//! GitHub (`orgserver-v<version>`, never the app's `v<version>`); only finished versions count, a tag with a
//! pre-release mark does not. The request carries nothing about the org: a plain GET with the name and version of
//! the server, which GitHub sees together with the address of the host. `VERSEKIT_ORG_UPDATE_CHECK=0` turns it off.

use serde_json::Value;

use crate::conformance::Exchange;

/// The tags of the server, and only those.
pub const TAGS: &str = "https://api.github.com/repos/Xharig/VerseKit/git/matching-refs/tags/orgserver-v";
/// The prefix of the server's git tags.
pub const TAG_PREFIX: &str = "orgserver-v";
/// Where the changes of a version are listed.
pub const RELEASE_PAGE: &str = "https://github.com/Xharig/VerseKit/releases/tag/";
/// Seconds between two checks.
pub const INTERVAL: u64 = 86_400;

/// A newer server version and where its changes are listed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    pub version: String,
    pub url: String,
}

/// What the last check found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpdateState {
    /// When it last asked, in seconds since 1970; 0 before the first time.
    pub checked: u64,
    pub offer: Option<Offer>,
    /// Why the last check failed, without anything of the answer.
    pub failed: Option<String>,
}

/// `1.2.3` as numbers; a version with a pre-release mark or another shape is none.
pub fn version_of(text: &str) -> Option<(u64, u64, u64)> {
    let mut parts = text.split('.');
    let mut next = || parts.next().filter(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))?.parse::<u64>().ok();
    let version = (next()?, next()?, next()?);
    parts.next().is_none().then_some(version)
}

/// The newest finished server version newer than `own` in an answer of the GitHub API for matching refs.
pub fn newest(body: &[u8], own: &str) -> Option<Offer> {
    let own = version_of(own)?;
    let list: Vec<Value> = serde_json::from_slice(body).ok()?;
    list.iter()
        .filter_map(|r| {
            let tag = r.get("ref").and_then(Value::as_str)?.strip_prefix("refs/tags/")?;
            let text = tag.strip_prefix(TAG_PREFIX)?;
            let version = version_of(text)?;
            (version > own).then(|| (version, Offer { version: text.to_string(), url: format!("{RELEASE_PAGE}{tag}") }))
        })
        .max_by_key(|(version, _)| *version)
        .map(|(_, offer)| offer)
}

/// Asks once: the offer, or why it failed.
pub fn check(exchange: &dyn Exchange, own: &str) -> Result<Option<Offer>, String> {
    let answer = exchange.exchange("GET", TAGS, &[("Accept", "application/vnd.github+json")], b"")?;
    if answer.status != 200 {
        return Err(format!("GitHub answered {}", answer.status));
    }
    Ok(newest(&answer.body, own))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = r#"[
        {"ref": "refs/tags/orgserver-v1.3.0-rc1"},
        {"ref": "refs/tags/orgserver-v1.2.0"},
        {"ref": "refs/tags/orgserver-v1.10.0"},
        {"ref": "refs/tags/orgserver-v0.9.0"},
        {"ref": "refs/tags/v5.9.0"},
        {"ref": "refs/heads/orgserver-v9.0.0"}
    ]"#;

    #[test]
    fn only_the_servers_own_finished_releases_count() {
        let offer = newest(LIST.as_bytes(), "1.0.0").unwrap();
        assert_eq!(offer.version, "1.10.0", "the highest by number, not by text");
        assert_eq!(offer.url, "https://github.com/Xharig/VerseKit/releases/tag/orgserver-v1.10.0");
        assert_eq!(newest(LIST.as_bytes(), "1.10.0"), None, "nothing newer");
        let app_only = r#"[{"ref": "refs/tags/v9.0.0"}]"#;
        assert_eq!(newest(app_only.as_bytes(), "1.0.0"), None, "an app release is never a server update");
        assert_eq!(newest(b"not json", "1.0.0"), None);
    }

    #[test]
    fn versions_are_three_numbers() {
        assert_eq!(version_of("1.2.3"), Some((1, 2, 3)));
        assert_eq!(version_of("1.2.3-rc1"), None);
        assert_eq!(version_of("1.2"), None);
        assert_eq!(version_of("1.2.3.4"), None);
    }
}
