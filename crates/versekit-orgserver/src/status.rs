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

//! The read-only status page at `/`: the org's name and tag, the version of the server, since when it runs, how
//! many members are connected, the rights the org offers, and links to the confirmation page and the discovery
//! document. It names no member and holds no token and no key. German or English after `Accept-Language`.

use crate::http::{Request, Response, escape_html};
use crate::look::{german, icon, page as frame, pick};
use crate::panel::{Tone, note_box, scope_label};
use crate::{PANEL_PATH, SCOPES, Server, WELL_KNOWN, setup};

/// Year, month and day of a count of days since 1970-01-01 (the civil calendar).
pub(crate) fn date_of(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// `2026-11-14 18:03 UTC`.
pub(crate) fn moment(seconds: u64) -> String {
    let (year, month, day) = date_of((seconds / 86_400) as i64);
    let minute = seconds % 86_400 / 60;
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02} UTC", minute / 60, minute % 60)
}

/// `3 d 4 h 12 min`, without the parts in front that are zero.
fn duration(seconds: u64, german: bool) -> String {
    let (days, hours, minutes) = (seconds / 86_400, seconds % 86_400 / 3600, seconds % 3600 / 60);
    let (d, h, m) = if german { ("Tg.", "Std.", "Min.") } else { ("d", "h", "min") };
    match (days, hours) {
        (0, 0) => format!("{minutes} {m}"),
        (0, _) => format!("{hours} {h} {minutes} {m}"),
        _ => format!("{days} {d} {hours} {h} {minutes} {m}"),
    }
}

/// Answers `GET /`.
pub(crate) fn page(server: &Server, request: &Request, now: u64) -> Response {
    let de = german(request);
    let config = server.config();
    let started = server.started();
    let since = format!("{} · {}", moment(started), duration(now.saturating_sub(started), de));
    let rights: String = SCOPES
        .iter()
        .map(|s| format!("<li>{}<span>{}</span></li>", icon("check"), escape_html(scope_label(s, de))))
        .collect();
    let mut notes = String::new();
    if config.test_mode {
        notes.push_str(&note_box(
            Tone::Warn,
            pick(
                de,
                "Testbetrieb ohne HTTPS: nur zum Ausprobieren, nicht für echte Mitglieder.",
                "Test mode without HTTPS: for trying out only, not for real members.",
            ),
        ));
    }
    if setup::incomplete(server, now) {
        notes.push_str(&note_box(
            Tone::Warn,
            pick(
                de,
                "Einrichtung unvollständig. Die Orga-Leitung sieht in der Verwaltung, was fehlt.",
                "Setup incomplete. The org's leadership sees in the administration what is missing.",
            ),
        ));
    }
    let body = format!(
        "<h1>{title}</h1><p class=\"einleitung\">{intro}</p>{notes}\
         <section class=\"karte\"><h2>{activity}{state}</h2><dl class=\"werte\">\
         <dt>{running}</dt><dd>{since}</dd>\
         <dt>{members}</dt><dd>{count}</dd>\
         <dt>{version}</dt><dd>{server_version}</dd>\
         <dt>{standard}</dt><dd>versekit-org v1</dd>\
         </dl></section>\
         <section class=\"karte\"><h2>{shield}{offered}</h2><ul class=\"liste\">{rights}</ul>\
         <p class=\"hinweis\">{choose}</p></section>\
         <div class=\"knopfreihe\">{portal}\
         <a class=\"knopf voll\" href=\"{PANEL_PATH}\">{monitor}{device}</a>\
         <a class=\"knopf\" href=\"{WELL_KNOWN}\">{share}{discovery}</a></div>\
         <p class=\"hinweis\">{note}</p>",
        title = pick(de, "Orga-Server", "Org server"),
        intro = pick(
            de,
            "Über diesen Server verbinden die Mitglieder VerseKit mit der Orga. In VerseKit unter Einstellungen › Orga verbinden die Adresse dieses Servers eintragen.",
            "Members connect VerseKit to the org through this server. In VerseKit, enter the address of this server under Settings › Connect org.",
        ),
        activity = icon("activity"),
        state = pick(de, "Zustand", "State"),
        running = pick(de, "Läuft seit", "Running since"),
        members = pick(de, "Verbundene Mitglieder", "Connected members"),
        count = server.connected_members(),
        version = pick(de, "Version des Servers", "Version of the server"),
        server_version = env!("CARGO_PKG_VERSION"),
        standard = pick(de, "Standard", "Standard"),
        shield = icon("shield-check"),
        offered = pick(de, "Angebotene Rechte", "Rights offered"),
        choose = pick(
            de,
            "Bei der Anmeldung wählt jedes Mitglied selbst, welche Rechte VerseKit bekommt.",
            "When signing in, every member chooses which rights VerseKit gets.",
        ),
        portal = if config.discord.is_some() {
            format!(
                "<a class=\"knopf voll\" href=\"{}\">{}{}</a>",
                crate::portal::ROOT,
                icon("log-in"),
                pick(de, "Mitglieder-Portal", "Members' portal")
            )
        } else {
            String::new()
        },
        monitor = icon("monitor"),
        device = pick(de, "Gerät verbinden", "Connect a device"),
        share = icon("share-2"),
        discovery = pick(de, "Discovery-Dokument", "Discovery document"),
        note = pick(
            de,
            "Diese Seite zeigt nur Zahlen: keine Namen von Mitgliedern, keine Schlüssel, keine Tokens.",
            "This page shows numbers only: no names of members, no keys, no tokens.",
        ),
    );
    let lang = pick(de, "de", "en");
    Response::html(200, frame(de, &config.name, &config.tag, pick(de, "Orga-Server", "Org server"), &body))
        .with_header("Content-Language", lang)
        .with_header("Vary", "Accept-Language")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_become_dates() {
        assert_eq!(date_of(0), (1970, 1, 1));
        assert_eq!(date_of(20_406), (2025, 11, 14));
        assert_eq!(date_of(11_016), (2000, 2, 29));
        assert_eq!(moment(1_763_143_380), "2025-11-14 18:03 UTC");
    }

    #[test]
    fn durations_leave_out_the_empty_front() {
        assert_eq!(duration(59, true), "0 Min.");
        assert_eq!(duration(3_720, false), "1 h 2 min");
        assert_eq!(duration(2 * 86_400 + 3_600, true), "2 Tg. 1 Std. 0 Min.");
    }
}
