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

//! The page `/device` where a member confirms a login: type the code shown in
//! VerseKit and the personal key, see the rights asked for, untick any, then
//! confirm or refuse. German or English after `Accept-Language`, no scripts.

use crate::http::{Request, Response, escape_html, value};
use crate::look::{german, icon, page as frame, pick};
use crate::{Decision, PANEL_PATH, PANEL_TRIES, PANEL_WRONG_PER_MINUTE, Server, State, hash, normal_code};

/// The label of a right on the pages.
pub(crate) fn scope_label(scope: &str, german: bool) -> &'static str {
    let (de, en) = match scope {
        "stock:read" => ("Orga-Bestand lesen", "Read the org's stock"),
        "stock:write" => ("Eigenen Bestand teilen", "Share your own stock"),
        "blueprints:read" => ("Baupläne der Orga lesen", "Read the org's blueprints"),
        "blueprints:write" => ("Eigene Baupläne teilen", "Share your own blueprints"),
        "orders:read" => ("Aufträge lesen", "Read the orders"),
        "orders:write" => ("Lieferungen zu Aufträgen buchen", "Book deliveries to orders"),
        _ => ("?", "?"),
    };
    pick(german, de, en)
}

/// The kind of a message box: what went well, what needs a look.
#[derive(Clone, Copy)]
pub(crate) enum Tone {
    Good,
    Warn,
    /// Neither good nor bad: something to know.
    Info,
}

pub(crate) fn note_box(tone: Tone, text: &str) -> String {
    let (class, symbol) = match tone {
        Tone::Good => ("gut", "circle-check"),
        Tone::Warn => ("warn", "triangle-alert"),
        Tone::Info => ("info", "info"),
    };
    format!("<div class=\"meldung {class}\" role=\"status\">{}<span>{}</span></div>", icon(symbol), escape_html(text))
}

fn answer(server: &Server, de: bool, body: &str) -> Response {
    let config = server.config();
    let title = pick(de, "Gerät verbinden", "Connect a device");
    Response::html(200, frame(de, &config.name, &config.tag, title, body))
        .with_header("Content-Language", pick(de, "de", "en"))
        .with_header("Vary", "Accept-Language")
}

/// The end of a login: a message, and the way back to the status page.
fn message(server: &Server, de: bool, tone: Tone, text: &str) -> Response {
    let body = format!(
        "<h1>{title}</h1>{note}<div class=\"knopfreihe\"><a class=\"knopf\" href=\"/\">{back}</a></div>",
        title = pick(de, "Gerät verbinden", "Connect a device"),
        note = note_box(tone, text),
        back = pick(de, "Zur Übersicht", "To the overview"),
    );
    answer(server, de, &body)
}

fn code_form(server: &Server, de: bool, code: &str, note: Option<&str>) -> Response {
    let body = format!(
        "<h1>{title}</h1><p class=\"einleitung\">{intro}</p>{note}\
         <section class=\"karte\"><h2>{lock}{head}</h2>\
         <form method=\"post\" action=\"{PANEL_PATH}\">\
         <label class=\"feld\" for=\"c\">{code_label}</label>\
         <input id=\"c\" class=\"code\" type=\"text\" name=\"user_code\" value=\"{code}\" placeholder=\"BCDF-GHJK\" \
         autocomplete=\"off\" autocapitalize=\"characters\" spellcheck=\"false\" required>\
         <label class=\"feld\" for=\"k\">{key_label}</label>\
         <input id=\"k\" type=\"password\" name=\"key\" autocomplete=\"current-password\" required>\
         <div class=\"knopfreihe\"><button class=\"knopf voll\" type=\"submit\" name=\"action\" value=\"show\">{go}</button></div>\
         </form><p class=\"hinweis\">{warning}</p></section>",
        title = pick(de, "Gerät verbinden", "Connect a device"),
        intro = pick(
            de,
            "Bestätige hier die Anmeldung, die du gerade in VerseKit gestartet hast.",
            "Confirm here the login you just started in VerseKit.",
        ),
        note = note.map(|text| note_box(Tone::Warn, text)).unwrap_or_default(),
        lock = icon("lock"),
        head = pick(de, "Code und Schlüssel", "Code and key"),
        code_label = pick(de, "Code aus VerseKit", "Code shown in VerseKit"),
        code = escape_html(code),
        key_label = pick(de, "Dein persönlicher Schlüssel", "Your personal key"),
        go = pick(de, "Weiter", "Continue"),
        warning = pick(
            de,
            "Gib den Code nur ein, wenn du ihn gerade selbst in VerseKit angefordert hast.",
            "Only type the code if you just asked for it in VerseKit yourself.",
        ),
    );
    answer(server, de, &body)
}

/// Answers `GET` and `POST` of `/device`. After the right key the page of the rights carries a short-lived ticket
/// instead of the key; a form may also send the key with the decision at once. Wrong keys count per code and, over
/// all codes, per minute.
pub(crate) fn page(server: &Server, state: &mut State, request: &Request, now: u64) -> Response {
    let de = german(request);
    if request.method == "GET" {
        let query = request.query_pairs();
        return code_form(server, de, &normal_code(value(&query, "user_code").unwrap_or_default()), None);
    }
    if request.method != "POST" {
        return crate::not_allowed();
    }
    let form = request.form();
    let code = normal_code(value(&form, "user_code").unwrap_or_default());
    let Some(index) = state.pending.iter().position(|p| p.user_code == code && p.decision == Decision::Open) else {
        let unknown = pick(de, "Dieser Code ist unbekannt oder abgelaufen.", "This code is unknown or has expired.");
        return code_form(server, de, "", Some(unknown));
    };
    let ticket = value(&form, "ticket").filter(|t| !t.is_empty()).map(hash);
    let by_ticket = ticket.and_then(|wanted| {
        let (stored, id) = state.pending[index].ticket.as_ref()?;
        (*stored == wanted).then(|| id.clone())
    });
    let member = match by_ticket.and_then(|id| server.config().members.iter().find(|m| m.id == id)) {
        Some(member) => member,
        None => {
            let minute = now / 60;
            if state.panel_fails.0 == minute && state.panel_fails.1 >= PANEL_WRONG_PER_MINUTE {
                let wait = pick(
                    de,
                    "Gerade kamen zu viele falsche Schlüssel. Versuch es in einer Minute wieder.",
                    "Too many wrong keys just now. Try again in a minute.",
                );
                return message(server, de, Tone::Warn, wait);
            }
            match server.member_by_key(value(&form, "key").unwrap_or_default()) {
                Some(member) => member,
                None => {
                    state.panel_fails = if state.panel_fails.0 == minute { (minute, state.panel_fails.1 + 1) } else { (minute, 1) };
                    state.pending[index].tries += 1;
                    if state.pending[index].tries >= PANEL_TRIES {
                        state.pending.remove(index);
                        let void =
                            pick(de, "Zu viele falsche Schlüssel, der Code gilt nicht mehr.", "Too many wrong keys, the code is void.");
                        return message(server, de, Tone::Warn, void);
                    }
                    return code_form(server, de, &code, Some(pick(de, "Der Schlüssel stimmt nicht.", "The key is wrong.")));
                }
            }
        }
    };
    let fresh_ticket = server.random_hex(24);
    let login = &mut state.pending[index];
    match value(&form, "action") {
        Some("approve") => {
            let ticked: Vec<&str> = form.iter().filter(|(n, _)| n == "scope").map(|(_, v)| v.as_str()).collect();
            let scopes: Vec<String> = login.scopes.iter().filter(|s| ticked.contains(&s.as_str())).cloned().collect();
            if scopes.is_empty() {
                login.decision = Decision::Denied;
                let none = pick(de, "Keine Rechte gewählt, die Anmeldung ist abgelehnt.", "No rights chosen, the login is refused.");
                return message(server, de, Tone::Warn, none);
            }
            login.decision = Decision::Approved { member: member.id.clone(), scopes };
            let done = pick(de, "Freigegeben. VerseKit verbindet sich gleich.", "Confirmed. VerseKit connects in a moment.");
            message(server, de, Tone::Good, done)
        }
        Some("deny") => {
            login.decision = Decision::Denied;
            message(server, de, Tone::Warn, pick(de, "Abgelehnt. VerseKit bekommt keinen Zugang.", "Refused. VerseKit gets no access."))
        }
        _ => {
            login.ticket = Some((hash(&fresh_ticket), member.id.clone()));
            let boxes: String = login
                .scopes
                .iter()
                .map(|s| {
                    format!(
                        "<label class=\"recht\"><input type=\"checkbox\" name=\"scope\" value=\"{v}\" checked><span>{l}</span></label>",
                        v = escape_html(s),
                        l = escape_html(scope_label(s, de)),
                    )
                })
                .collect();
            let body = format!(
                "<h1>{title}</h1><p class=\"einleitung\">{hello}</p>\
                 <section class=\"karte\"><h2>{shield}{head}</h2>\
                 <form method=\"post\" action=\"{PANEL_PATH}\">{boxes}\
                 <input type=\"hidden\" name=\"user_code\" value=\"{code}\">\
                 <input type=\"hidden\" name=\"ticket\" value=\"{fresh_ticket}\">\
                 <div class=\"knopfreihe\">\
                 <button class=\"knopf voll\" type=\"submit\" name=\"action\" value=\"approve\">{check}{approve}</button>\
                 <button class=\"knopf\" type=\"submit\" name=\"action\" value=\"deny\">{deny}</button></div>\
                 </form><p class=\"hinweis\">{untick}</p></section>",
                title = pick(de, "Gerät verbinden", "Connect a device"),
                hello = pick(de, "Hallo {name}. VerseKit möchte diese Rechte:", "Hello {name}. VerseKit asks for these rights:")
                    .replace("{name}", &escape_html(&member.name)),
                shield = icon("shield-check"),
                head = pick(de, "Angefragte Rechte", "Rights asked for"),
                code = escape_html(&code),
                check = icon("check"),
                approve = pick(de, "Freigeben", "Confirm"),
                deny = pick(de, "Ablehnen", "Refuse"),
                untick = pick(
                    de,
                    "Nimm den Haken bei jedem Recht weg, das VerseKit nicht bekommen soll.",
                    "Untick every right VerseKit should not get.",
                ),
            );
            answer(server, de, &body)
        }
    }
}
