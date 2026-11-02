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

//! Rust side of the check for the thanks page (`tools/parity_thanks.py`):
//! the sources, the people and the text keys of version 5.

use std::collections::BTreeSet;
use std::fs;

use serde_json::{Value, json};
use versekit_core::credits::{BUNDLED, FETCHED, Licence, PEOPLE, text_keys};
use versekit_core::language::{Lang, text_in};

pub fn run(_python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let mut boxes: Vec<Value> = BUNDLED
        .iter()
        .chain(FETCHED.iter())
        .map(|c| {
            let licence = match c.licence {
                Licence::Named(name) => name.to_string(),
                Licence::None => "NONE".to_string(),
            };
            json!({ "name": c.name, "licence": licence, "key": c.text, "url": c.url })
        })
        .collect();
    let mut people: Vec<Value> = PEOPLE
        .iter()
        .map(|p| json!({ "name": p.name, "group": p.group, "ideas": p.ideas, "found": p.found }))
        .collect();
    if gegenprobe {
        // A source and a person dropped, a licence and a group changed.
        boxes.remove(5);
        boxes[1]["licence"] = json!("GPL-3.0");
        people.pop();
        people[0]["group"] = json!("KRT");
    }
    let mut keys: BTreeSet<&str> = text_keys().into_iter().collect();
    keys.extend(["hf_wer", "hf_titel", "s_ub_wer_h", "s_dk_dabei", "s_dk_dabei_h", "s_dk_extern", "s_dk_leute"]);
    keys.extend(["s_dk_aufklappen", "s_dk_keine_lizenz", "s_ub_auf", "s_ub_auf_nein"]);
    let texts: serde_json::Map<String, Value> = keys
        .iter()
        .map(|k| {
            let pair = [Lang::De, Lang::En].map(|l| text_in(l, k).unwrap_or_default());
            ((*k).to_string(), json!(pair))
        })
        .collect();
    let result = json!({ "boxes": boxes, "people": people, "used": keys, "texts": texts });
    fs::write(out, serde_json::to_vec_pretty(&result).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
