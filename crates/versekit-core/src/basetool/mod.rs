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

//! The connection to the KRT Profit Basetool (Exchange API v1): login,
//! requests, the sync in the background and its error paths
//! (`basetool.py`, `basetool_sync.py`, `secret_store.py`,
//! `dpop_reference/`).
//!
//! | Module | Does |
//! |---|---|
//! | `config` | issuer, API, client id; the test environment only through environment variables |
//! | `jose`, `proof` | DPoP proofs, server nonce, server clock |
//! | `key`, `cng`, `secrets` | the installation key and the refresh token in the system's store |
//! | `http` | one request over TLS |
//! | `connection` | device login, tokens, requests with nonce and refresh, disconnecting |
//! | `scopes` | which rights the switched-on areas need |
//! | `feed` | snapshot and feed to the end, resolving names, locations, sending change sets |
//! | `guard` | the write guard and the request journal for a test against the real Basetool |
//! | `hold` | change sets the Basetool holds back until the member confirms them |
//! | `pacing` | answered operations that are not sent again, the wait after `Retry-After` |
//! | `state` | the sync state of each installation |
//! | `schedule` | when the next sync is due |
//! | `blueprint_sync` | the blueprints against the Basetool: plan and change sets, without the net |
//! | `stock_sync` | the two stocks against the Basetool: lots, plan and change sets, without the net |
//! | `stock_split` | personal and shared parts of the own lots, sharing into the pool of a unit |
//! | `sync` | the worker in the background, the published state, jobs of the pages |
//! | `overview` | the lines and decision cards of the connection page |
//! | `decisions` | what the player decides when the two sides differ |
//! | `ship_sync` | the hangar against the ships over there: identifiers, plan and change sets, without the net |
//! | `orgstock` | the storage of the own units: reading, booking, looking up goods |
//! | `demand` | what the open orders of the own units still need, in memory only |
//! | `demo` | a made-up Basetool in memory for the preview and the tests |
//!
//! ⚠ No token, key or login code leaves these modules: not into the error
//! log, the report or the backup. Errors carry the server's code only.

pub mod area_support;
pub mod blueprint_area;
pub mod blueprint_sync;
#[cfg(windows)]
pub mod cng;
pub mod config;
pub mod connection;
pub mod decisions;
pub mod demand;
pub mod demo;
pub mod error;
pub mod exchange_double;
pub mod feed;
pub mod guard;
pub mod hold;
pub mod http;
pub mod jose;
pub mod key;
pub mod orgstock;
pub mod orgunits;
pub mod overview;
pub mod pacing;
pub mod proof;
pub mod schedule;
pub mod scopes;
pub mod secrets;
pub mod ship_area;
pub mod ship_sync;
pub mod state;
pub mod stock_area;
pub mod stock_split;
pub mod stock_sync;
pub mod sync;
