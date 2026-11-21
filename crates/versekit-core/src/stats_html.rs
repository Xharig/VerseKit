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

//! The statistics as one HTML page to read, print or pass on: the same figures as the JSON export
//! ([`crate::stats_report::export_value`]), made into a page in the language of the program.
//!
//! The page is one file: its style is inside, it loads nothing and runs no script. It holds figures only, no
//! account name and no place (the export holds none). Every text that comes from the data is escaped.

use crate::formats;
use crate::jsonio::{self, Value};
use crate::language::{self, Arg, Lang, t, tf};

/// The mark of the program, the one green.
const GREEN: &str = "156, 228, 48";

/// `text` safe for the body and for an attribute of an HTML page.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

fn number(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Int(text) => text.parse().ok(),
        Value::Float(x) if x.is_finite() => Some(*x as i64),
        _ => None,
    }
}

/// A stamp of the export (`2026-11-19T10:00:00Z`) as `2026-11-19 10:00`.
fn stamp(text: &str) -> String {
    let text = text.trim_end_matches('Z').replace('T', " ");
    text.get(..16).unwrap_or(&text).to_string()
}

fn code(lang: Lang) -> &'static str {
    match lang {
        Lang::De => "de",
        Lang::En => "en",
    }
}

/// The page for the export `value`, in the language the program shows now.
pub fn page(value: &Value) -> String {
    let total = value.get("gesamt");
    let figure = |key: &str| number(total.and_then(|t| t.get(key))).unwrap_or(0);
    let mut cards: Vec<(String, String)> = vec![
        (t("s_sx_spielzeit"), formats::duration(figure("spielzeit") as f64)),
        (t("s_sx_sitzungen"), figure("sitzungen").to_string()),
        (t("s_sx_auftraege"), figure("auftraege").to_string()),
        (t("s_sx_spruenge"), figure("spruenge").to_string()),
        (t("s_sx_tode"), figure("tode").to_string()),
        (t("s_sx_verloren"), figure("verloren").to_string()),
    ];
    cards.push((
        t("s_sx_html_schnitt"),
        tf(
            "s_sx_schnitt",
            &[
                ("0", Arg::from(formats::duration(figure("schnitt") as f64))),
                ("1", Arg::from(formats::duration(figure("laengste") as f64))),
            ],
        ),
    ));
    let mut body = String::new();
    body.push_str("<div class=\"cards\">\n");
    for (name, shown) in &cards {
        body.push_str(&format!(
            "<div class=\"card\"><div class=\"n\">{}</div><div>{}</div></div>\n",
            escape(shown),
            escape(name)
        ));
    }
    body.push_str("</div>\n");
    body.push_str(&heat_table(value));
    body.push_str(&sessions_table(value));
    let created = value.get("erstellt").and_then(Value::as_str).map(stamp).unwrap_or_default();
    let version = value.get("version").and_then(Value::as_str).unwrap_or("");
    let lang = language::current();
    format!(
        "<!DOCTYPE html>\n<html lang=\"{}\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{}</title>\n\
         <style>{STYLE}</style>\n</head>\n<body>\n<h1>{}</h1>\n<p class=\"sub\">{}</p>\n{body}\
         <p class=\"sub\">{}</p>\n</body>\n</html>\n",
        code(lang),
        escape(&t("s_sx_html_titel")),
        escape(&t("s_sx_html_titel")),
        escape(&tf("s_sx_html_erstellt", &[("0", Arg::from(created)), ("1", Arg::from(version.to_string()))])),
        escape(&t("s_sx_html_fuss")),
    )
}

fn heat_table(value: &Value) -> String {
    let Some(Value::Array(rows)) = value.get("waermekarte") else { return String::new() };
    let grid: Vec<Vec<i64>> = rows
        .iter()
        .map(|row| match row {
            Value::Array(cells) => cells.iter().map(|c| number(Some(c)).unwrap_or(0)).collect(),
            _ => Vec::new(),
        })
        .collect();
    let most = grid.iter().flatten().copied().max().unwrap_or(0);
    let days = t("s_sx_tage");
    let days: Vec<&str> = days.split(',').collect();
    let mut out = format!("<h2>{}</h2>\n<table class=\"heat\">\n<tr><th></th>", escape(&t("s_sx_wann")));
    for hour in 0..24 {
        out.push_str(&format!("<th>{hour}</th>"));
    }
    out.push_str("</tr>\n");
    for (day, row) in grid.iter().enumerate() {
        out.push_str(&format!("<tr><th>{}</th>", escape(days.get(day).copied().unwrap_or(""))));
        for hour in 0..24 {
            let seconds = row.get(hour).copied().unwrap_or(0);
            let alpha = if most > 0 && seconds > 0 { 0.15 + 0.85 * seconds as f64 / most as f64 } else { 0.0 };
            out.push_str(&format!(
                "<td style=\"background: rgba({GREEN}, {alpha:.2})\" title=\"{}\"></td>",
                escape(&formats::duration(seconds as f64))
            ));
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</table>\n");
    out
}

fn sessions_table(value: &Value) -> String {
    let Some(Value::Array(rows)) = value.get("sitzungen") else { return String::new() };
    if rows.is_empty() {
        return String::new();
    }
    let mut out = format!(
        "<h2>{}</h2>\n<table>\n<tr><th>{}</th><th class=\"r\">{}</th><th class=\"r\">{}</th><th class=\"r\">{}</th>\
         <th class=\"r\">{}</th></tr>\n",
        escape(&t("s_sx_sitzungen")),
        escape(&t("s_sx_html_von")),
        escape(&t("s_sx_html_dauer")),
        escape(&t("s_sx_auftraege")),
        escape(&t("s_sx_spruenge")),
        escape(&t("s_sx_tode")),
    );
    for row in rows {
        let text = |key: &str| row.get(key).and_then(Value::as_str).unwrap_or("");
        let seconds = match (parse(text("von")), parse(text("bis"))) {
            (Some(from), Some(to)) if to >= from => to - from,
            _ => 0,
        };
        let cell = |key: &str| number(row.get(key)).map_or_else(|| "–".to_string(), |n| n.to_string());
        out.push_str(&format!(
            "<tr><td>{}</td><td class=\"r\">{}</td><td class=\"r\">{}</td><td class=\"r\">{}</td><td class=\"r\">{}</td></tr>\n",
            escape(&stamp(text("von"))),
            escape(&formats::duration(seconds as f64)),
            cell("auftraege"),
            cell("spruenge"),
            cell("tode"),
        ));
    }
    out.push_str("</table>\n");
    out
}

/// The seconds of a stamp of the export, `None` for one that is none.
fn parse(text: &str) -> Option<i64> {
    crate::stats_report::log_seconds(text.trim_end_matches('Z'))
}

const STYLE: &str = "body{font-family:system-ui,sans-serif;margin:2rem auto;max-width:60rem;padding:0 1rem;\
background:#14161a;color:#e8eaed}h1{color:#9ce430}h2{margin-top:2rem}.sub{color:#9aa0a6}\
.cards{display:flex;flex-wrap:wrap;gap:.75rem}.card{background:#1d2026;border-radius:.5rem;padding:.75rem 1rem;\
min-width:9rem}.n{font-size:1.5rem;font-weight:600}table{border-collapse:collapse;width:100%}\
th,td{padding:.25rem .5rem;border-bottom:1px solid #2c3036;text-align:left}.r{text-align:right}\
.heat{width:auto}.heat td{width:1.1rem;height:1.1rem;padding:0;border:1px solid #14161a}\
.heat th{font-weight:400;font-size:.7rem;padding:0 .2rem;border:0;color:#9aa0a6}\
@media print{body{background:#fff;color:#000}.card{background:#eee}}";

/// Writes the page for `value` to `target`; false when it does not work (the reason is recorded).
pub fn write(target: &str, value: &Value) -> bool {
    match jsonio::write_text_atomic(target, &page(value), jsonio::Newline::Native) {
        Ok(()) => true,
        Err(e) => {
            crate::errors::record("play_stats.export_html", &e);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsonio::Map;

    fn export() -> Value {
        let total: Map = vec![
            ("spielzeit".into(), Value::int(7200)),
            ("sitzungen".into(), Value::int(2)),
            ("schnitt".into(), Value::int(3600)),
            ("laengste".into(), Value::int(5400)),
            ("auftraege".into(), Value::int(12)),
            ("spruenge".into(), Value::int(34)),
            ("tode".into(), Value::int(5)),
            ("verloren".into(), Value::int(1)),
        ];
        let mut heat = vec![vec![0i64; 24]; 7];
        heat[2][20] = 3600;
        let session: Map = vec![
            ("von".into(), Value::str("2026-11-18T19:00:00Z")),
            ("bis".into(), Value::str("2026-11-18T20:30:00Z")),
            ("auftraege".into(), Value::int(7)),
            ("fehlgeschlagen".into(), Value::Null),
            ("spruenge".into(), Value::int(9)),
            ("tode".into(), Value::Null),
        ];
        Value::Object(vec![
            ("werkzeug".into(), Value::str("VerseKit <b>")),
            ("version".into(), Value::str("5.4.0")),
            ("erstellt".into(), Value::str("2026-11-19T10:00:00Z")),
            ("gesamt".into(), Value::Object(total)),
            (
                "waermekarte".into(),
                Value::Array(heat.into_iter().map(|r| Value::Array(r.into_iter().map(Value::int).collect())).collect()),
            ),
            ("sitzungen".into(), Value::Array(vec![Value::Object(session)])),
        ])
    }

    #[test]
    fn the_page_holds_the_figures_the_heat_map_and_the_sessions_and_loads_nothing() {
        language::set_thread_language(language::Choice::De);
        let html = page(&export());
        assert!(html.starts_with("<!DOCTYPE html>") && html.contains("<html lang=\"de\">"));
        assert!(html.contains("2 h"), "the playtime as the program writes it");
        assert!(html.contains(">12<") && html.contains(">34<"), "contracts and jumps");
        assert!(html.contains("2026-11-18 19:00"), "the session");
        assert!(html.contains("1 h 30 min"), "its length from the two stamps: {html}");
        assert!(html.contains("rgba(156, 228, 48, 1.00)"), "the busiest hour is the full green");
        assert!(html.contains("Mi"), "the weekdays of the language");
        assert!(!html.contains("<script") && !html.contains("http://") && !html.contains("https://"), "loads nothing");
        assert!(html.contains("–"), "a figure the log did not give is a dash, not 0");
    }

    #[test]
    fn what_comes_from_the_data_is_escaped_and_the_language_is_the_one_of_the_program() {
        let mut value = export();
        if let Value::Object(map) = &mut value {
            crate::jsonio::map_set(map, "version", Value::str("<script>alert(1)</script>"));
        }
        language::set_thread_language(language::Choice::En);
        let html = page(&value);
        assert!(!html.contains("<script>alert"), "escaped");
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(html.contains("<html lang=\"en\">") && html.contains("Sessions"));
        assert_eq!(escape("a&b<c>\"d'"), "a&amp;b&lt;c&gt;&quot;d&#39;");
    }

    #[test]
    fn writing_gives_a_file_and_says_no_when_it_cannot() {
        let dir = std::env::temp_dir().join(format!("versekit-stathtml-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        let target = dir.join("bericht.html").to_string_lossy().into_owned();
        assert!(write(&target, &export()));
        assert!(std::fs::read_to_string(&target).expect("file").contains("<table"));
        let blocked = dir.join("kein-ordner").join("x").join("b.html").to_string_lossy().into_owned();
        std::fs::write(dir.join("kein-ordner"), "x").expect("a file where a folder would be");
        assert!(!write(&blocked, &export()));
        let _ = std::fs::remove_dir_all(dir);
    }
}
