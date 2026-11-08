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

//! Verse-Kit without a user interface: everything that can be checked without
//! a window.
//!
//! ⚠ Every function here has a counterpart in the Python version and must
//! return the same result for the same inputs (parity check, `versekit-parity`).
//! Where Python has a quirk (decoding with `errors='ignore'`, `str.strip`), it
//! is reproduced, not improved.

pub mod anti_afk;
pub mod anti_afk_key;
pub mod archive_lock;
pub mod asop;
pub mod auto_update;
pub mod autostart;
pub mod axes;
pub mod axes_write;
pub mod backup;
pub mod basetool;
pub mod basket;
pub mod blueprints;
pub mod browser;
pub mod cart;
pub mod catalog_cache;
pub mod catalog_watch;
pub mod changelog;
pub mod child;
pub mod collection;
pub mod commodity_prices;
pub mod contested;
pub mod contracts;
pub mod controls;
pub mod crash;
pub mod credits;
pub mod desktop_entry;
pub mod detection;
pub mod device_hub;
pub mod device_sets;
pub mod diagnostics;
pub mod equipment;
pub mod equipment_share;
pub mod erkul_fetch;
pub mod erkul_slots;
pub mod error_log;
pub mod errors;
pub mod export;
pub mod exporter;
pub mod fabricator;
pub mod first_backup;
pub mod fleet_analysis;
pub mod font_size;
pub mod formats;
pub mod fov;
pub mod fps_scan;
pub mod gallery;
pub mod game_files;
pub mod game_ini;
pub mod game_picture;
pub mod game_start;
pub mod game_texts;
pub mod given;
pub mod graphics;
pub mod graphics_apply;
pub mod handle_query;
pub mod hangar;
pub mod hangar_formats;
pub mod hangar_inbox;
pub mod importer;
pub mod info_bar;
pub mod injection;
pub mod injuries;
pub mod input_device;
pub mod issue_council;
pub mod joysticks;
pub mod joysticks_write;
pub mod jsonio;
pub mod language;
pub mod legacy;
pub mod live_contracts;
pub mod location;
pub mod logread;
pub mod logscan;
pub mod logsource;
pub mod logtime;
pub mod machine_info;
pub mod memo;
pub mod mining_data;
pub mod mission_log;
pub mod mission_scan;
pub mod modules;
pub mod net;
pub mod notepad;
pub mod notify;
pub mod ordered;
pub mod overlay;
pub mod overlay_control;
pub mod overrides;
pub mod overview;
pub mod part_choices;
pub mod patch_changes;
pub mod patch_history;
pub mod paths;
pub mod phrases;
pub mod places;
pub mod play_stats;
pub mod playtime;
pub mod pyfloat;
pub mod pypath;
pub mod quality_mix;
pub mod raw_prices;
pub mod redact;
pub mod streamer;
pub mod refinery_jobs;
pub mod refinery_scan;
pub mod recipe_file;
pub mod refining;
pub mod renderer;
pub mod report;
pub mod report_facts;
pub mod report_send;
pub mod routes;
pub mod salvage;
pub mod scmdb;
pub mod scmdb_bridge;
pub mod screen;
pub mod screen_grab;
pub mod selling;
pub mod serverstatus;
pub mod setup;
pub mod shader_cache;
pub mod shard;
pub mod ship_market;
pub mod ships;
pub mod shops;
pub mod signature_scan;
pub mod signature_watch;
pub mod signature_window;
pub mod size_order;
pub mod sound;
pub mod start_programs;
pub mod stats_report;
pub mod stick_input;
pub mod stock;
pub mod stock_data;
pub mod stock_fetch;
pub mod stock_journal;
pub mod stock_relocation;
pub mod stock_use;
pub mod storage_move;
#[doc(hidden)]
pub mod testdir;
pub mod testmode;
pub mod text;
pub mod theme;
pub mod trade_sync;
pub mod trail;
pub mod translation;
pub mod uex;
pub mod update_flow;
pub mod update_run;
pub mod update_swap;
pub mod updater;
pub mod usage;
pub mod usercfg;
pub mod version;
pub mod watchlist;
pub mod win_ocr;
pub mod snap_layouts;
pub mod window_signal;
pub mod workbench_jobs;
pub mod writeguard;
pub mod zip;
