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

//! Pass on unexpected errors without failing itself.
//!
//! Whoever logs the errors registers with `set_sink`. Without a sink the
//! messages are silently dropped and the program keeps running.

use std::fmt::Display;
use std::sync::RwLock;

type Sink = Box<dyn Fn(&str, &str) + Send + Sync>;

static SINK: RwLock<Option<Sink>> = RwLock::new(None);

/// Set the sink for reported errors (location, text).
pub fn set_sink(sink: impl Fn(&str, &str) + Send + Sync + 'static) {
    if let Ok(mut slot) = SINK.write() {
        *slot = Some(Box::new(sink));
    }
}

/// Report an error under its location (`paths.settings`).
///
/// ⚠ The sink must not call `record` again (infinite loop).
pub fn record(label: &str, error: &dyn Display) {
    if let Ok(slot) = SINK.read()
        && let Some(sink) = slot.as_ref()
    {
        sink(label, &error.to_string());
    }
}
