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

//! The pages of an org's data are in the menu only with the connection, the right and the switch of their area:
//! not connected, connected without the right, the right without the switch, everything on; per org.

use std::sync::Arc;

use versekit_core::basetool::error::ApiError;
use versekit_core::basetool::http::{Request, Response, Transport};
use versekit_core::orgstd::areas::Areas;
use versekit_core::orgstd::client::OrgClient;
use versekit_core::orgstd::discovery::{Discovery, Origin};
use versekit_core::orgstd::hub::{Hub, Kind, system_clock};
use versekit_core::orgstd::store::{Connection, Store};

use super::{ORG_ORDERS_PAGE, ORG_STOCK_PAGE, SamplePage};
use crate::org_connect_page::OrgConnectState;

const ADDRESS: &str = "https://orga.example.org";
const OTHER: &str = "https://zweite.example.org";

struct Dead;

impl Transport for Dead {
    fn send(&self, _: &Request) -> Result<Response, ApiError> {
        Err(ApiError::network("test"))
    }
}

fn connection(address: &str, scopes: &[&str]) -> Connection {
    let api = format!("{address}/versekit/v1");
    Connection {
        discovery: Discovery {
            origin: Origin::parse(address).unwrap(),
            name: "Orga".into(),
            tag: "ORG".into(),
            homepage: None,
            version: "1".into(),
            api_base: api.clone(),
            device_endpoint: format!("{api}/device/code"),
            token_endpoint: format!("{api}/token"),
            revocation_endpoint: None,
            scopes: versekit_core::orgstd::SCOPES.iter().map(|s| s.to_string()).collect(),
        },
        member: "Pilot".into(),
        scopes: scopes.iter().map(|s| s.to_string()).collect(),
        access_token: String::new(),
        access_expires: 0,
        refresh_token: "x".into(),
        connected_at: 1,
        checked_at: None,
    }
}

/// A window whose orgs are `connections`, with the switches `areas`.
fn page_with(connections: Vec<Connection>, areas: &Areas) -> SamplePage {
    let client = OrgClient::new(Arc::new(Dead), "5.4.0");
    let hub = Hub::new(client, Store::memory(connections), system_clock());
    let mut page = SamplePage::new();
    page.orgs = OrgConnectState::with_hub(hub, false).with_areas(areas.clone());
    page
}

#[test]
fn not_connected_nothing_of_an_org_is_in_the_menu_whatever_is_switched_on() {
    let areas = Areas::in_memory();
    for kind in Kind::ALL {
        areas.set(ADDRESS, kind, true);
    }
    let page = page_with(Vec::new(), &areas);
    assert!(page.page_hidden(ORG_STOCK_PAGE) && page.page_hidden(ORG_ORDERS_PAGE));
}

#[test]
fn connected_without_the_right_the_page_is_not_there_even_with_the_switch() {
    let areas = Areas::in_memory();
    areas.set(ADDRESS, Kind::Stock, true);
    areas.set(ADDRESS, Kind::Orders, true);
    let page = page_with(vec![connection(ADDRESS, &["blueprints:read", "stock:write"])], &areas);
    assert!(page.page_hidden(ORG_STOCK_PAGE), "only the right to share is no right to read");
    assert!(page.page_hidden(ORG_ORDERS_PAGE));
}

#[test]
fn with_the_right_but_without_the_switch_the_page_is_not_there_and_freshly_connected_everything_is_off() {
    let areas = Areas::in_memory();
    let page = page_with(vec![connection(ADDRESS, &versekit_core::orgstd::SCOPES)], &areas);
    assert!(page.page_hidden(ORG_STOCK_PAGE) && page.page_hidden(ORG_ORDERS_PAGE), "off from the start");
    areas.set(ADDRESS, Kind::Stock, true);
    assert!(!page.page_hidden(ORG_STOCK_PAGE), "the switch of the stock opens the page of the stock");
    assert!(page.page_hidden(ORG_ORDERS_PAGE), "and no other");
    areas.set(ADDRESS, Kind::Orders, true);
    assert!(!page.page_hidden(ORG_ORDERS_PAGE));
    areas.set(ADDRESS, Kind::Stock, false);
    assert!(page.page_hidden(ORG_STOCK_PAGE), "switched off again");
}

#[test]
fn the_switches_count_per_org() {
    let areas = Areas::in_memory();
    areas.set(OTHER, Kind::Stock, true);
    let page = page_with(vec![connection(ADDRESS, &["stock:read"]), connection(OTHER, &["orders:read"])], &areas);
    assert!(page.page_hidden(ORG_STOCK_PAGE), "the org with the switch has no right to read the stock");
    areas.set(ADDRESS, Kind::Stock, true);
    assert!(!page.page_hidden(ORG_STOCK_PAGE), "now the right and the switch meet in one org");
    assert!(page.page_hidden(ORG_ORDERS_PAGE), "its switch for orders is off");
}
