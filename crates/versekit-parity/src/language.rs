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

//! Rust side of the parity check for the user interface texts
//! (`tools/parity_language.py`): renders every case of the Python side and
//! writes the results in the same order.

use std::fs;

use versekit_core::formats;
use versekit_core::jsonio::{self, Map, Value};
use versekit_core::language::{self, Arg, Lang};
use versekit_core::paths::Host;
use versekit_core::pypath::Flavor;

fn field<'a>(value: &'a Value, name: &str) -> Result<&'a Value, String> {
    value.get(name).ok_or_else(|| format!("{name} fehlt"))
}

fn cases<'a>(input: &'a Value, name: &str) -> Result<&'a [Value], String> {
    match field(input, name)? {
        Value::Array(items) => Ok(items),
        _ => Err(format!("{name} ist keine Liste")),
    }
}

fn text<'a>(case: &'a Value, name: &str) -> Result<&'a str, String> {
    field(case, name)?.as_str().ok_or_else(|| format!("{name} ist kein Text"))
}

fn lang(case: &Value) -> Result<Lang, String> {
    let code = text(case, "lang")?;
    Lang::from_code(code).ok_or_else(|| format!("unbekannte Sprache {code}"))
}

/// `[name, type, value]` → (name, Arg).
fn arg(item: &Value) -> Result<(String, Arg), String> {
    let Value::Array(parts) = item else { return Err("Wert ist keine Liste".into()) };
    let [name, kind, value] = parts.as_slice() else { return Err("Wert braucht drei Teile".into()) };
    let name = name.as_str().ok_or("Name ist kein Text")?.to_string();
    let arg = match (kind.as_str(), value) {
        (Some("text"), Value::Str(s)) => Arg::Text(s.clone()),
        (Some("int"), Value::Int(n)) => Arg::Int(n.parse().map_err(|_| format!("Zahl {n} zu groß"))?),
        (Some("float"), Value::Str(s)) => Arg::Float(s.parse().map_err(|_| format!("keine Kommazahl: {s}"))?),
        _ => return Err(format!("unbekannter Wert {item:?}")),
    };
    Ok((name, arg))
}

fn render_texts(input: &Value, gegenprobe: bool) -> Result<Vec<Value>, String> {
    let mut out = Vec::new();
    for case in cases(input, "texts")? {
        let mut args = match field(case, "args")? {
            Value::Array(items) => items.iter().map(arg).collect::<Result<Vec<_>, _>>()?,
            _ => return Err("args ist keine Liste".into()),
        };
        if gegenprobe && args.len() >= 2 {
            let first = args[0].1.clone();
            args[0].1 = args[1].1.clone();
            args[1].1 = first;
        }
        let borrowed: Vec<(&str, Arg)> = args.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
        out.push(Value::str(language::tf_in(lang(case)?, text(case, "key")?, &borrowed)));
    }
    Ok(out)
}

fn number(case: &Value) -> Result<Arg, String> {
    let item = field(case, "value")?;
    let Value::Array(parts) = item else { return Err("value ist keine Liste".into()) };
    let [kind, value] = parts.as_slice() else { return Err("value braucht zwei Teile".into()) };
    arg(&Value::Array(vec![Value::str("value"), kind.clone(), value.clone()])).map(|(_, a)| a)
}

fn timestamp(case: &Value) -> Result<f64, String> {
    let text = text(case, "ts")?;
    text.parse().map_err(|_| format!("keine Zeit: {text}"))
}

fn render_formats(input: &Value, gegenprobe: bool) -> Result<Vec<Value>, String> {
    let mut out = Vec::new();
    for case in cases(input, "formats")? {
        let lang = lang(case)?;
        let text = match text(case, "art")? {
            "fixed" => {
                let places = match field(case, "places")? {
                    Value::Int(n) => n.parse().map_err(|_| "places".to_string())?,
                    _ => return Err("places ist keine Zahl".into()),
                };
                let value = match number(case)? {
                    Arg::Int(n) => n as f64,
                    Arg::Float(x) => x,
                    _ => return Err("fixed braucht eine Zahl".into()),
                };
                formats::fixed(lang, value, places)
            }
            "count" => {
                let value = match number(case)? {
                    Arg::Int(n) => n,
                    Arg::Float(x) => x.trunc() as i64,
                    _ => return Err("count braucht eine Zahl".into()),
                };
                formats::count(if gegenprobe { Lang::En } else { lang }, value)
            }
            "decimal" => formats::decimal_mark(lang).to_string(),
            "moment" => formats::moment(lang, timestamp(case)?, "m_erster_datum"),
            "report_date" => {
                let ts = timestamp(case)?;
                formats::report_date(lang, if gegenprobe { ts.floor() } else { ts })
            }
            "date_time" => formats::date_time(lang, timestamp(case)?),
            "phrase_moment" => language::tf_in(lang, "m_erster_lauf", &[("0", Arg::moment(timestamp(case)?))]),
            other => return Err(format!("unbekannte Art {other}")),
        };
        out.push(Value::str(text));
    }
    Ok(out)
}

fn sorted_keys<'a>(keys: impl Iterator<Item = &'a String>) -> Value {
    let mut keys: Vec<&String> = keys.collect();
    keys.sort();
    Value::Array(keys.into_iter().map(|k| Value::str(k.as_str())).collect())
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let input = jsonio::read_json(python_json).map_err(|e| e.to_string())?;
    let [de, _] = language::tables();
    let mut result: Map = Vec::new();
    result.push(("keys".into(), sorted_keys(de.texts.keys())));
    result.push(("field_paths".into(), sorted_keys(de.fields.keys())));
    result.push(("texts".into(), Value::Array(render_texts(&input, gegenprobe)?)));

    let mut kinds = Vec::new();
    for case in cases(&input, "kinds")? {
        let mut label = language::kind_label_in(lang(case)?, text(case, "raw")?);
        if gegenprobe && label == "Sonstiges" {
            label = "Other".into();
        }
        kinds.push(Value::str(label));
    }
    result.push(("kinds".into(), Value::Array(kinds)));

    let mut fields = Vec::new();
    for case in cases(&input, "fields")? {
        fields.push(Value::str(language::pa_field_in(lang(case)?, text(case, "path")?)));
    }
    result.push(("fields".into(), Value::Array(fields)));

    let mut properties = Vec::new();
    for case in cases(&input, "properties")? {
        let key = field(case, "key")?.as_str();
        let laser = matches!(field(case, "laser")?, Value::Bool(true));
        properties.push(Value::str(language::property_name_in(lang(case)?, text(case, "name")?, key, laser)));
    }
    result.push(("properties".into(), Value::Array(properties)));

    let mut titles = Vec::new();
    for case in cases(&input, "titles")? {
        let mut host = Host::new(Flavor::native(), &[], "/");
        if let Some(value) = field(case, "env")?.as_str() {
            host.set_var("SC_BP_TESTFASSUNG", value);
        }
        titles.push(Value::str(language::window_title_in(lang(case)?, text(case, "text")?, &host)));
    }
    result.push(("titles".into(), Value::Array(titles)));

    let mut system = Vec::new();
    for case in cases(&input, "system")? {
        let mut host = Host::new(Flavor::native(), &[], "/");
        for (name, value) in field(case, "env")?.as_object().ok_or("env ist kein Objekt")? {
            host.set_var(name, value.as_str().unwrap_or_default());
        }
        system.push(Value::str(language::system_language(&host).code()));
    }
    result.push(("system".into(), Value::Array(system)));

    let mut choices = Vec::new();
    for case in cases(&input, "choices")? {
        let settings = field(case, "settings")?.as_object().ok_or("settings ist kein Objekt")?;
        choices.push(Value::str(language::choice_of(settings).code()));
    }
    result.push(("choices".into(), Value::Array(choices)));
    result.push(("formats".into(), Value::Array(render_formats(&input, gegenprobe)?)));
    let tz = std::env::var("TZ").ok().map_or(Value::Null, Value::str);
    result.push(("tz".into(), tz));

    fs::write(out, jsonio::dumps(&Value::Object(result), Some(1), false)).map_err(|e| e.to_string())
}
