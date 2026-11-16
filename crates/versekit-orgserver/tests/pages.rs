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

//! The pages of the template server: the status page answers in German and English, counts a connected member
//! and names none, holds no key and no token; no page loads anything from elsewhere; the font and the licenses are
//! served by the server itself; the environment sets name, address and the limit.

use versekit_orgserver::http::{Request, Response};
use versekit_orgserver::{Config, RATE_LIMIT, Server};

const KEY: &str = "geheimer-schluessel-der-pilotin-7";
const MEMBER: &str = "Pilotin Vega Geheimname";

fn server() -> Server {
    Server::new(Config::new("https://orga.example.org", "Nebelorga", "NBL").with_member("mitglied-id-geheim", MEMBER, KEY))
}

fn get(server: &Server, path: &str, language: Option<&str>) -> Response {
    let headers: Vec<(&str, &str)> = language.map(|l| ("Accept-Language", l)).into_iter().collect();
    server.handle(&Request::new("GET", path, &headers, b""))
}

fn text(answer: &Response) -> String {
    String::from_utf8(answer.body.clone()).unwrap()
}

/// Logs the member in and gives back everything secret the login made.
fn log_in(server: &Server) -> Vec<String> {
    let code = server.handle(&Request::new("POST", "/versekit/v1/device/code", &[], b"scope=stock%3Aread")).json_body();
    let device = code["device_code"].as_str().unwrap().to_string();
    let user = code["user_code"].as_str().unwrap().to_string();
    assert!(server.approve(&user, "mitglied-id-geheim", None));
    let form = format!("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code&device_code={device}");
    let tokens = server.handle(&Request::new("POST", "/versekit/v1/token", &[], form.as_bytes())).json_body();
    let access = tokens["access_token"].as_str().expect("a token").to_string();
    let refresh = tokens["refresh_token"].as_str().expect("a token").to_string();
    vec![device, user, access, refresh]
}

/// Nothing from another server: no script, no stylesheet, font or picture from elsewhere.
fn loads_nothing_from_elsewhere(page: &str) -> bool {
    let own = page.replacen("<link rel=\"stylesheet\" href=\"/assets/look.css?v=", "", 1);
    !["<script", "src=\"http", "src='http", "url(http", "url(//", "@import", "<link ", "<style", " style="].iter().any(|p| own.contains(p))
}

#[test]
fn the_status_page_counts_members_and_names_none_and_holds_no_secret() {
    let server = server();
    let before = get(&server, "/", Some("de-DE,de;q=0.9,en;q=0.8"));
    assert_eq!(before.status, 200);
    let page = text(&before);
    assert!(page.contains("Nebelorga") && page.contains("NBL"), "the org's name and tag");
    assert!(page.contains("Verbundene Mitglieder</dt><dd>0</dd>"), "{page}");
    assert!(page.contains(env!("CARGO_PKG_VERSION")));
    assert!(page.contains("href=\"/device\"") && page.contains("href=\"/.well-known/versekit-org.json\""));

    let secrets = log_in(&server);
    let page = text(&get(&server, "/", Some("de")));
    assert!(page.contains("Verbundene Mitglieder</dt><dd>1</dd>"), "{page}");
    for secret in secrets.iter().map(String::as_str).chain([KEY, MEMBER, "Pilotin", "mitglied-id-geheim"]) {
        assert!(!page.contains(secret), "the status page shows {secret:?}");
    }
}

#[test]
fn the_pages_speak_the_language_of_the_browser() {
    let server = server();
    let german = text(&get(&server, "/", Some("de-AT,en;q=0.5")));
    let english = text(&get(&server, "/", Some("en-GB,de;q=0.5")));
    let none = text(&get(&server, "/", None));
    assert!(german.contains("lang=\"de\"") && german.contains("Läuft seit"));
    assert!(english.contains("lang=\"en\"") && english.contains("Running since"));
    assert!(none.contains("lang=\"en\""), "without a wish the page is English");
    assert!(text(&get(&server, "/device", Some("de"))).contains("Code aus VerseKit"));
    assert!(text(&get(&server, "/device", Some("en"))).contains("Code shown in VerseKit"));
}

#[test]
fn no_page_loads_anything_from_elsewhere_and_the_font_comes_from_the_server() {
    let server = server();
    for (path, language) in [("/", "de"), ("/", "en"), ("/device", "de"), ("/device", "en")] {
        let answer = get(&server, path, Some(language));
        let page = text(&answer);
        assert!(loads_nothing_from_elsewhere(&page), "{path} {language}");
        assert!(page.contains("<link rel=\"stylesheet\" href=\"/assets/look.css?v="), "{path}: the stylesheet of the server");
        assert!(page.contains("<svg class=\"ic\""), "{path}: the icons are in the page");
        assert!(page.contains("versekit-org v1"), "{path}: the quiet line about the standard");
        let policy = answer.headers.iter().find(|(n, _)| n == "Content-Security-Policy").map(|(_, v)| v.as_str());
        assert!(
            policy.is_some_and(|p| p.contains("default-src 'none'")
                && p.contains("style-src 'self'")
                && p.contains("font-src 'self'")
                && !p.contains("unsafe")),
            "{policy:?}"
        );
    }
    let sheet = get(&server, "/assets/look.css", None);
    assert_eq!(sheet.status, 200);
    let css = text(&sheet);
    assert!(css.contains("url(/assets/xharig-sans-400.woff2)"), "the font of the server");
    assert!(!css.contains("url(http") && !css.contains("@import"), "nothing from elsewhere in the stylesheet");
    let font = get(&server, "/assets/xharig-sans-700.woff2", None);
    assert_eq!((font.status, &font.body[..4]), (200, &b"wOF2"[..]));
    assert!(text(&get(&server, "/assets/OFL.txt", None)).contains("SIL OPEN FONT LICENSE"));
    assert!(text(&get(&server, "/assets/LICENSE-lucide.txt", None)).contains("ISC"));
    assert_eq!(get(&server, "/assets/anything.css", None).status, 404);
}

#[test]
fn the_environment_sets_address_name_tag_and_limit_and_a_wrong_value_is_refused() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |name: &str| pairs.iter().find(|(n, _)| *n == name).map(|(_, v)| v.to_string())
    };
    let base = || Config::new("https://orga.example.org", "Aus der Datei", "DAT");
    let config = base()
        .with_env(env(&[
            ("VERSEKIT_ORG_PUBLIC_URL", "https://panel.example.net/"),
            ("VERSEKIT_ORG_NAME", "Aus der Umgebung"),
            ("VERSEKIT_ORG_TAG", "UMG"),
            ("VERSEKIT_ORG_RATE_LIMIT", "30"),
            ("VERSEKIT_ORG_POLL_INTERVAL", "2"),
        ]))
        .unwrap();
    assert_eq!(
        (config.public_url.as_str(), config.name.as_str(), config.tag.as_str(), config.rate_limit, config.poll_interval),
        ("https://panel.example.net", "Aus der Umgebung", "UMG", 30, 2)
    );
    let untouched = base().with_env(env(&[("VERSEKIT_ORG_NAME", "  ")])).unwrap();
    assert_eq!((untouched.name.as_str(), untouched.rate_limit), ("Aus der Datei", RATE_LIMIT));
    assert!(base().with_env(env(&[("VERSEKIT_ORG_RATE_LIMIT", "viele")])).is_err());
    assert!(base().with_env(env(&[("VERSEKIT_ORG_RATE_LIMIT", "0")])).is_err());
    assert!(base().with_env(env(&[("VERSEKIT_ORG_PUBLIC_URL", "http://orga.example.org")])).is_err());

    let file = r#"{"public_url": "https://orga.example.org", "name": "Datei", "rate_limit": 7}"#;
    assert_eq!(Config::from_json(file).unwrap().rate_limit, 7);
}
