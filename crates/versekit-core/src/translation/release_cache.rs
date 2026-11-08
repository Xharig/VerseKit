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

//! What the GitHub API said about the newest release of a source, kept with its `ETag`.
//!
//! GitHub lets 60 requests an hour through without signing in, and the translation looks every five minutes
//! for every source and channel. A request that carries the tag of the last answer and gets
//! `304 Not Modified` does not count against that limit, so each answer is kept here and asked with its tag
//! the next time. When GitHub says the limit is used up, the time to wait is kept too, and nothing is asked
//! until it is over.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::Latest;
use crate::errors;
use crate::jsonio;
use crate::paths::Paths;

/// The file in the data folder.
pub(super) const FILE: &str = "uebersetzung_abfragen.json";

/// One kept answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Kept {
    /// The `ETag` GitHub gave the answer.
    pub etag: String,
    pub latest: Latest,
}

/// The kept answers by key, and until when GitHub is left alone.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Book {
    answers: BTreeMap<String, Kept>,
    /// Seconds since 1970; no request before.
    pause_until: f64,
}

/// The key of an answer: the address and the file of the release it was read for, since one release serves
/// several sources.
pub(super) fn key(url: &str, asset: Option<&str>) -> String {
    format!("{url}#{}", asset.unwrap_or_default())
}

impl Book {
    pub(super) fn get(&self, key: &str) -> Option<&Kept> {
        self.answers.get(key)
    }

    /// Keep `latest` under `key` with the tag it came with; an answer without a tag cannot be asked again
    /// conditionally and is not kept.
    pub(super) fn keep(&mut self, key: &str, etag: Option<&str>, latest: &Latest) {
        match etag.filter(|tag| !tag.is_empty()) {
            Some(etag) => {
                self.answers.insert(key.to_string(), Kept { etag: etag.to_string(), latest: latest.clone() });
            }
            None => {
                self.answers.remove(key);
            }
        }
    }

    /// Whether GitHub is to be left alone at `at`.
    pub(super) fn paused(&self, at: f64) -> bool {
        at < self.pause_until
    }

    /// Leave GitHub alone for `seconds` from `at`.
    pub(super) fn pause(&mut self, at: f64, seconds: u64) {
        self.pause_until = self.pause_until.max(at + seconds as f64);
    }
}

fn from_value(value: &Value) -> Book {
    let text = |entry: &Value, name: &str| entry.get(name).and_then(Value::as_str).map(str::to_string);
    let answers = value
        .get("antworten")
        .and_then(Value::as_object)
        .map(|all| {
            all.iter()
                .filter_map(|(key, entry)| {
                    let latest = Latest {
                        ident: Some(text(entry, "kennung").filter(|ident| !ident.is_empty())?),
                        url: text(entry, "url")?,
                        size: entry.get("groesse").and_then(Value::as_u64).unwrap_or(0),
                        date: text(entry, "datum").filter(|date| !date.is_empty()),
                    };
                    let etag = text(entry, "etag").filter(|tag| !tag.is_empty())?;
                    Some((key.clone(), Kept { etag, latest }))
                })
                .collect()
        })
        .unwrap_or_default();
    Book { answers, pause_until: value.get("pause_bis").and_then(Value::as_f64).unwrap_or(0.0) }
}

fn to_value(book: &Book) -> Value {
    json!({
        "pause_bis": book.pause_until,
        "antworten": book.answers.iter().map(|(key, kept)| (key.clone(), json!({
            "etag": kept.etag,
            "kennung": kept.latest.ident,
            "url": kept.latest.url,
            "groesse": kept.latest.size,
            "datum": kept.latest.date,
        }))).collect::<serde_json::Map<_, _>>(),
    })
}

/// The kept answers; empty if there are none or the file is broken.
pub(super) fn read(paths: &Paths) -> Book {
    std::fs::read(paths.app_file(FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .map(|value| from_value(&value))
        .unwrap_or_default()
}

/// Keep the answers. A file that cannot be written costs only the saving of requests; it goes to the error log.
pub(super) fn write(paths: &Paths, book: &Book) {
    let text = to_value(book).to_string();
    if let Err(error) = jsonio::write_text_atomic(&paths.app_file(FILE), &text, jsonio::Newline::Keep) {
        errors::record("translation.release_cache", &error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::Fake;
    use crate::paths::Host;
    use crate::pypath::Flavor;
    use crate::translation::{Problem, Spec, latest, spec};

    const RELEASE: &str = "https://api.github.com/repos/MrKraken/StarStrings/releases/latest";

    fn body(tag: &str, file: &str) -> String {
        format!(
            r#"{{"tag_name": "{tag}", "published_at": "2026-10-28T09:00:00Z", "assets": [{{"name": "{file}", "browser_download_url": "https://dl/{tag}/{file}", "size": 10}}]}}"#
        )
    }

    fn paths(name: &str) -> (crate::testdir::Dir, Paths) {
        let dir = crate::testdir::Dir::new(&format!("release-cache-{name}"));
        let root = dir.path().to_string_lossy().into_owned();
        let data = format!("{root}/data");
        std::fs::create_dir_all(&data).unwrap();
        let host = Host::new(Flavor::Posix, &[("HOME", root.as_str()), ("SC_BP_HOME", &data)], &root);
        (dir, Paths::new(host))
    }

    fn starstrings(paths: &Paths) -> Spec {
        spec(paths, "starstrings", None).unwrap()
    }

    #[test]
    fn the_second_look_asks_with_the_tag_and_costs_nothing() {
        let (_dir, paths) = paths("tag");
        let net = Fake::default().with(RELEASE, body("v2", "StarStrings-LIVE.zip")).with_etag(RELEASE, "\"a\"");
        let first = latest(&net, &paths, &starstrings(&paths)).unwrap();
        assert_eq!(first.ident.as_deref(), Some("v2"));
        let second = latest(&net, &paths, &starstrings(&paths)).unwrap();
        assert_eq!(second, first, "the kept answer stands in for the one GitHub did not send");
        assert_eq!(net.asked(), [RELEASE.to_string(), format!("{RELEASE} (unchanged)")]);
    }

    #[test]
    fn the_tag_outlives_the_program() {
        let (_dir, paths) = paths("restart");
        let net = Fake::default().with(RELEASE, body("v2", "StarStrings-LIVE.zip")).with_etag(RELEASE, "\"a\"");
        latest(&net, &paths, &starstrings(&paths)).unwrap();
        assert!(std::path::Path::new(&paths.app_file(FILE)).is_file());
        let again = Fake::default().with(RELEASE, body("v2", "StarStrings-LIVE.zip")).with_etag(RELEASE, "\"a\"");
        latest(&again, &paths, &starstrings(&paths)).unwrap();
        assert_eq!(again.asked(), [format!("{RELEASE} (unchanged)")], "after a start: one request, a free one");
    }

    #[test]
    fn a_new_release_replaces_the_kept_answer() {
        let (_dir, paths) = paths("newer");
        let old = Fake::default().with(RELEASE, body("v2", "StarStrings-LIVE.zip")).with_etag(RELEASE, "\"a\"");
        latest(&old, &paths, &starstrings(&paths)).unwrap();
        let new = Fake::default().with(RELEASE, body("v3", "StarStrings-LIVE.zip")).with_etag(RELEASE, "\"b\"");
        let found = latest(&new, &paths, &starstrings(&paths)).unwrap();
        assert_eq!(found.ident.as_deref(), Some("v3"));
        assert_eq!(found.url, "https://dl/v3/StarStrings-LIVE.zip");
        let key = key(RELEASE, Some("StarStrings-LIVE.zip"));
        assert_eq!(read(&paths).get(&key).map(|kept| kept.etag.as_str()), Some("\"b\""));
    }

    #[test]
    fn an_answer_without_a_tag_is_asked_in_full_every_time() {
        let (_dir, paths) = paths("untagged");
        let net = Fake::default().with(RELEASE, body("v2", "StarStrings-LIVE.zip"));
        latest(&net, &paths, &starstrings(&paths)).unwrap();
        latest(&net, &paths, &starstrings(&paths)).unwrap();
        assert_eq!(net.asked(), [RELEASE, RELEASE]);
        assert!(read(&paths).get(&key(RELEASE, Some("StarStrings-LIVE.zip"))).is_none());
    }

    #[test]
    fn sources_of_one_release_keep_their_own_answers() {
        let (_dir, paths) = paths("shared");
        let url = "https://api.github.com/repos/Dymerz/StarCitizen-Localization/releases/latest";
        let release = r#"{"tag_name": "v4", "published_at": "2026-09-16T04:52:00Z", "assets": [
            {"name": "german_.germany.zip", "browser_download_url": "https://dl/de.zip", "size": 1},
            {"name": "french_.france.zip", "browser_download_url": "https://dl/fr.zip", "size": 2}]}"#;
        let net = Fake::default().with(url, release).with_etag(url, "\"d\"");
        let german = latest(&net, &paths, &spec(&paths, "dymerz_de", None).unwrap()).unwrap();
        let french = latest(&net, &paths, &spec(&paths, "dymerz_fr", None).unwrap()).unwrap();
        assert_eq!((german.url.as_str(), french.url.as_str()), ("https://dl/de.zip", "https://dl/fr.zip"));
        let again = latest(&net, &paths, &spec(&paths, "dymerz_fr", None).unwrap()).unwrap();
        assert_eq!(again, french, "not the answer of the German file");
    }

    #[test]
    fn a_used_up_limit_pauses_the_asking_and_the_kept_answer_stands_in() {
        let (_dir, paths) = paths("limit");
        let net = Fake::default().with(RELEASE, body("v2", "StarStrings-LIVE.zip")).with_etag(RELEASE, "\"a\"");
        let first = latest(&net, &paths, &starstrings(&paths)).unwrap();
        let limited = Fake::default().with_rate_limit(RELEASE, 1800);
        assert_eq!(latest(&limited, &paths, &starstrings(&paths)), Ok(first.clone()));
        assert_eq!(limited.asked(), [RELEASE], "asked once and told to wait");
        for _ in 0..3 {
            assert_eq!(latest(&limited, &paths, &starstrings(&paths)), Ok(first.clone()));
        }
        assert_eq!(limited.asked(), [RELEASE], "nothing more during the pause");
    }

    #[test]
    fn a_used_up_limit_without_a_kept_answer_is_a_refusal_and_pauses_too() {
        let (_dir, paths) = paths("limit-bare");
        let limited = Fake::default().with_rate_limit(RELEASE, 1800);
        assert_eq!(latest(&limited, &paths, &starstrings(&paths)), Err(Problem::Refused));
        assert_eq!(latest(&limited, &paths, &starstrings(&paths)), Err(Problem::Refused));
        assert_eq!(limited.asked(), [RELEASE]);
    }

    #[test]
    fn the_pause_ends() {
        let (_dir, paths) = paths("pause-over");
        let mut book = Book::default();
        book.pause(0.0, 60);
        write(&paths, &book);
        let net = Fake::default().with(RELEASE, body("v2", "StarStrings-LIVE.zip")).with_etag(RELEASE, "\"a\"");
        assert!(latest(&net, &paths, &starstrings(&paths)).is_ok(), "a pause of 1970 is over");
        assert_eq!(net.asked(), [RELEASE]);
        let at = crate::updater::now();
        assert!(!Book::default().paused(at));
        let mut waiting = Book::default();
        waiting.pause(at, 60);
        assert!(waiting.paused(at + 59.0) && !waiting.paused(at + 61.0));
        waiting.pause(at, 10);
        assert!(waiting.paused(at + 59.0), "a shorter wish does not cut a pause short");
    }

    #[test]
    fn other_errors_are_not_hidden_by_the_kept_answer() {
        let (_dir, paths) = paths("down");
        let net = Fake::default().with(RELEASE, body("v2", "StarStrings-LIVE.zip")).with_etag(RELEASE, "\"a\"");
        latest(&net, &paths, &starstrings(&paths)).unwrap();
        let gone = Fake::default();
        assert!(matches!(latest(&gone, &paths, &starstrings(&paths)), Err(Problem::Net(_))));
        assert_eq!(gone.asked().len(), 1, "a failure is asked for again at the next look");
    }

    #[test]
    fn a_broken_file_is_an_empty_book() {
        let (_dir, paths) = paths("broken");
        std::fs::write(paths.app_file(FILE), "{ not json").unwrap();
        assert_eq!(read(&paths), Book::default());
        std::fs::write(paths.app_file(FILE), r#"{"antworten": {"k": {"etag": "x"}}, "pause_bis": "soon"}"#).unwrap();
        assert_eq!(read(&paths), Book::default(), "an entry without its fields and a pause that is no number");
    }
}
