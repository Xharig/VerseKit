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

//! The handle search: who is behind a handle, from the public pages of the RSI website.
//!
//! Only the two public pages of a citizen and the public list of the members of an organization ([`members`])
//! are asked, without logging in. The search asks them through the door [`Net`], so that tests never touch the
//! net.

pub mod cache;
pub mod log_monitor;
pub mod members;
pub mod page;
pub mod pictures;
pub mod relations;

use crate::net::{FetchError, Net};
pub use cache::{Found, Settings, search};
pub use page::{Membership, Org, Orgs, Profile};

/// What the search shows of a handle.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Card {
    pub profile: Profile,
    pub orgs: Orgs,
}

impl Card {
    /// The profile page of the handle.
    pub fn url(&self) -> String {
        page::profile_url(&self.profile.handle)
    }
}

/// Why a search shows nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The text typed cannot be a handle.
    Invalid,
    /// The site knows no such handle.
    NotFound,
    /// Fetching is switched off.
    Offline,
    /// The site refused or could not be reached; what it was.
    Failed(String),
    /// The same handle is being asked already.
    Busy,
}

impl From<FetchError> for Failure {
    fn from(error: FetchError) -> Failure {
        match error {
            FetchError::Missing => Failure::NotFound,
            FetchError::Offline => Failure::Offline,
            FetchError::Refused => Failure::Failed("HTTP 403".to_string()),
            FetchError::Failed(what) => Failure::Failed(what),
        }
    }
}

/// The text of a page, as far as it is text.
fn text_of(body: &[u8]) -> String {
    String::from_utf8_lossy(body).into_owned()
}

/// Fetches the profile and the organizations of `handle` from the site: two pages, one after the other.
///
/// # Errors
///
/// [`Failure`] when the handle is not one, the site knows none or cannot be reached.
pub fn fetch(net: &dyn Net, handle: &str) -> Result<Card, Failure> {
    let handle = page::valid_handle(handle).ok_or(Failure::Invalid)?;
    let profile = page::parse_profile(&text_of(&net.get(&page::profile_url(&handle))?)).ok_or(Failure::NotFound)?;
    // The handle as the site writes it: the address of the second page is the same in every case.
    let orgs = page::parse_orgs(&text_of(&net.get(&page::orgs_url(&profile.handle))?));
    Ok(Card { profile, orgs })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::Fake;

    fn site() -> Fake {
        Fake::default()
            .with(&page::profile_url("Musterpilot"), page::tests::PROFILE)
            .with(&page::orgs_url("Musterpilot"), page::tests::ORGS)
    }

    #[test]
    fn a_handle_is_fetched_from_its_two_pages() {
        let net = site();
        let card = fetch(&net, " Musterpilot ").expect("found");
        assert_eq!(card.profile.name, "Muster & Co");
        assert_eq!(card.orgs.main.as_ref().and_then(Membership::org).map(|o| o.sid.as_str()), Some("TEST"));
        assert_eq!(net.asked(), [page::profile_url("Musterpilot"), page::orgs_url("Musterpilot")]);
        assert_eq!(card.url(), "https://robertsspaceindustries.com/citizens/Musterpilot");
    }

    #[test]
    fn an_unknown_handle_and_a_bad_text_ask_nothing_more() {
        let net = site();
        assert_eq!(fetch(&net, "Niemand"), Err(Failure::NotFound));
        assert_eq!(net.asked(), [page::profile_url("Niemand")], "no second page after a missing first one");
        let net = site();
        assert_eq!(fetch(&net, "a/../b"), Err(Failure::Invalid));
        assert!(net.asked().is_empty());
    }

    #[test]
    fn a_page_without_a_profile_counts_as_not_found() {
        let net = Fake::default().with(&page::profile_url("Leer"), "<html>Wartung</html>");
        assert_eq!(fetch(&net, "Leer"), Err(Failure::NotFound));
        assert_eq!(net.asked().len(), 1);
    }
}
