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

//! Table test: 10,000 rows with filter and sorting, measured per frame.

use std::path::PathBuf;
use std::time::Instant;

use eframe::egui;
use egui_extras::{Column, TableBuilder};

use crate::{GREEN, Probe};

const ROWS: usize = 10_000;
const KINDS: [&str; 8] =
    ["Waffe", "Rüstung", "Schild", "Kraftwerk", "Kühler", "Quantenantrieb", "Munition", "Werkzeug"];
const MAKERS: [&str; 10] = [
    "Behring",
    "Klaus & Werner",
    "Gallenson",
    "Hurston",
    "Kastak Arms",
    "Lightning Power",
    "Aegis",
    "Wen/Cassel",
    "Juno Starwerk",
    "RSI",
];
const PLACES: [&str; 6] = ["Area18", "Lorville", "New Babbage", "Orison", "Pyro Gateway", "Nyx Levski"];

struct Row {
    name: String,
    kind: &'static str,
    size: u8,
    grade: char,
    place: &'static str,
    amount: u32,
}

/// Fixed pseudo-random sequence so every run shows the same rows.
fn rows() -> Vec<Row> {
    let mut seed: u64 = 0x5645_524B_4954;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    (0..ROWS)
        .map(|i| {
            let r = next();
            Row {
                name: format!("{} {}-{:04}", MAKERS[(r % 10) as usize], KINDS[((r >> 8) % 8) as usize], i),
                kind: KINDS[((r >> 8) % 8) as usize],
                size: ((r >> 16) % 6) as u8 + 1,
                grade: (b'A' + ((r >> 20) % 4) as u8) as char,
                place: PLACES[((r >> 24) % 6) as usize],
                amount: ((r >> 32) % 500) as u32,
            }
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq)]
enum SortBy {
    Name,
    Kind,
    Size,
    Amount,
}

pub struct TableApp {
    rows: Vec<Row>,
    view: Vec<usize>,
    filter: String,
    filter_seen: String,
    sort: SortBy,
    frame_ms: Vec<f32>,
    probe: Option<Probe>,
    frame: u64,
    scroll_to: Option<usize>,
}

impl TableApp {
    pub fn new(probe: Option<PathBuf>) -> Self {
        let rows = rows();
        let view = (0..rows.len()).collect();
        Self {
            rows,
            view,
            filter: String::new(),
            filter_seen: String::new(),
            sort: SortBy::Name,
            frame_ms: Vec::new(),
            probe: probe.map(Probe::new),
            frame: 0,
            scroll_to: None,
        }
    }

    fn rebuild_view(&mut self) {
        let needle = self.filter.to_lowercase();
        self.view = (0..self.rows.len())
            .filter(|&i| needle.is_empty() || self.rows[i].name.to_lowercase().contains(&needle))
            .collect();
        let rows = &self.rows;
        match self.sort {
            SortBy::Name => self.view.sort_by(|&a, &b| rows[a].name.cmp(&rows[b].name)),
            SortBy::Kind => self.view.sort_by(|&a, &b| rows[a].kind.cmp(rows[b].kind)),
            SortBy::Size => self.view.sort_by_key(|&i| std::cmp::Reverse(rows[i].size)),
            SortBy::Amount => self.view.sort_by_key(|&i| std::cmp::Reverse(rows[i].amount)),
        }
        self.filter_seen = self.filter.clone();
    }

    /// In the probe run: set the filter, sort and scroll through the whole list.
    fn probe_step(&mut self, ctx: &egui::Context) {
        let Some(probe) = self.probe.as_mut() else { return };
        match self.frame {
            20 => {
                self.filter = "kühler".into();
            }
            40 => {
                self.filter.clear();
                self.sort = SortBy::Amount;
                self.rebuild_view();
            }
            60..=260 => {
                self.scroll_to = Some(((self.frame - 60) as usize * 50).min(self.view.len() - 1));
            }
            261 => {
                let measured: Vec<f32> = self.frame_ms.iter().skip(5).copied().collect();
                let avg = measured.iter().sum::<f32>() / measured.len().max(1) as f32;
                let max = measured.iter().copied().fold(0.0, f32::max);
                probe.value("bilder", measured.len().to_string());
                probe.value("bildzeit_mittel_ms", format!("{avg:.2}"));
                probe.value("bildzeit_max_ms", format!("{max:.2}"));
                probe.check("zeilen_gesamt", self.rows.len() == ROWS);
                probe.check("ansicht_nach_filter_leer_wieder_voll", self.view.len() == ROWS);
                probe.finish();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            _ => {}
        }
        ctx.request_repaint();
    }
}

impl eframe::App for TableApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let started = Instant::now();
        self.frame += 1;
        let ctx = ui.ctx().clone();
        self.probe_step(&ctx);
        if self.filter != self.filter_seen {
            self.rebuild_view();
        }

        egui::Panel::top("kopf").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.heading("Verse-Kit Tabellen-Test");
                ui.label(format!("{} von {} Zeilen", self.view.len(), self.rows.len()));
            });
            ui.horizontal(|ui| {
                ui.label("Filter:");
                ui.text_edit_singleline(&mut self.filter);
                let avg = if self.frame_ms.is_empty() {
                    0.0
                } else {
                    let tail = &self.frame_ms[self.frame_ms.len().saturating_sub(60)..];
                    tail.iter().sum::<f32>() / tail.len() as f32
                };
                ui.colored_label(GREEN, format!("Aufbau je Bild: {avg:.2} ms"));
            });
            ui.add_space(6.0);
        });

        egui::CentralPanel::default_margins().show(ui, |ui| {
            let mut new_sort = None;
            let mut builder = TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .column(Column::remainder().at_least(220.0))
                .column(Column::initial(120.0))
                .column(Column::initial(60.0))
                .column(Column::initial(60.0))
                .column(Column::initial(120.0))
                .column(Column::initial(80.0));
            if let Some(row) = self.scroll_to.take() {
                builder = builder.scroll_to_row(row, Some(egui::Align::TOP));
            }
            builder
                .header(24.0, |mut header| {
                    for (title, sort) in [
                        ("Name", Some(SortBy::Name)),
                        ("Art", Some(SortBy::Kind)),
                        ("Größe", Some(SortBy::Size)),
                        ("Grad", None),
                        ("Ort", None),
                        ("Menge", Some(SortBy::Amount)),
                    ] {
                        header.col(|ui| {
                            let marked = sort == Some(self.sort);
                            let mut text = egui::RichText::new(title).strong();
                            if marked {
                                text = text.color(GREEN);
                            }
                            if ui.button(text).clicked() {
                                new_sort = sort;
                            }
                        });
                    }
                })
                .body(|body| {
                    body.rows(22.0, self.view.len(), |mut row| {
                        let item = &self.rows[self.view[row.index()]];
                        row.col(|ui| {
                            ui.label(&item.name);
                        });
                        row.col(|ui| {
                            ui.label(item.kind);
                        });
                        row.col(|ui| {
                            ui.label(item.size.to_string());
                        });
                        row.col(|ui| {
                            ui.label(item.grade.to_string());
                        });
                        row.col(|ui| {
                            ui.label(item.place);
                        });
                        row.col(|ui| {
                            ui.label(item.amount.to_string());
                        });
                    });
                });
            if let Some(sort) = new_sort {
                self.sort = sort;
                self.rebuild_view();
            }
        });

        self.frame_ms.push(started.elapsed().as_secs_f32() * 1000.0);
    }
}
