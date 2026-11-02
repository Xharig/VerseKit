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

//! Rust side of the check for the goods with a quality: the list read from a DataForge and the English
//! `global.ini`, and the file `waren-guete.json` with the stamp `1:2` as `rust-datei.json` beside the output. With
//! `--gegenprobe` the first good of the list changes its unit and the file gets one byte more, each on its own, and
//! the comparison must report both.

use std::path::Path;

use versekit_core::jsonio::{self, Value};
use versekit_gamedata::quality_goods::{self, Unit};

/// The stamp the Python side writes into its file as well.
const FILE_STAMP: &str = "1:2";

pub fn run(dcb: &str, ini: &str, out: &str, counter: bool) -> Result<(), String> {
    let bytes = std::fs::read(dcb).map_err(|e| format!("{dcb}: {e}"))?;
    let raw_ini = std::fs::read(ini).map_err(|e| format!("{ini}: {e}"))?;
    let mut goods = quality_goods::goods_from_dataforge(&bytes, &raw_ini).map_err(|e| e.to_string())?;
    let mut file = quality_goods::file_text(FILE_STAMP, &goods);
    if counter {
        file.push(' ');
    }
    if counter && let Some(good) = goods.first_mut() {
        good.unit = if good.unit == Unit::Scu { Unit::Piece } else { Unit::Scu };
    }
    let list = Value::Object(vec![(
        "waren".into(),
        Value::Array(
            goods
                .iter()
                .map(|good| {
                    Value::Array(vec![
                        Value::str(&good.name),
                        Value::str(good.unit.key()),
                        good.piece_scu.map_or(Value::Null, Value::Float),
                    ])
                })
                .collect(),
        ),
    )]);
    std::fs::write(out, jsonio::dumps(&list, Some(1), false)).map_err(|e| format!("{out}: {e}"))?;
    let beside = Path::new(out).with_file_name("rust-datei.json");
    std::fs::write(&beside, file).map_err(|e| format!("{}: {e}", beside.display()))?;
    println!("{} Waren", goods.len());
    Ok(())
}
