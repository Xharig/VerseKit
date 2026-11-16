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

//! The look of the pages of the server: the dark scheme and the accent corners of VerseKit, its font Xharig Sans
//! (a Lato subset, SIL OFL 1.1) and Lucide icons (ISC) inline. Everything comes from the server itself, so a page
//! looks the same without internet: the CSS is in the page, the two font files and their licenses are served
//! under `/assets/`.

use crate::http::{Request, Response, escape_html};

const FONT_400: &[u8] = include_bytes!("../../../assets/schrift/xharig-sans-400.woff2");
const FONT_700: &[u8] = include_bytes!("../../../assets/schrift/xharig-sans-700.woff2");
const FONT_LICENSE: &str = include_str!("../../../assets/schrift/OFL.txt");
const ICON_LICENSE: &str = include_str!("../../../assets/symbole/LICENSE-lucide.txt");

/// The address of the project, named once and quietly at the foot of every page.
pub(crate) const PROJECT: &str = "https://versekit.xharig.com";

const CSS: &str = r#"
@font-face{font-family:"Xharig Sans";font-style:normal;font-weight:400;font-display:swap;src:url(/assets/xharig-sans-400.woff2) format("woff2")}
@font-face{font-family:"Xharig Sans";font-style:normal;font-weight:700;font-display:swap;src:url(/assets/xharig-sans-700.woff2) format("woff2")}
:root{--grund:#10141c;--flaeche:#161c28;--linie:#232c3d;--linie-hell:#34415a;--feld:#0c1017;--hover:#222b3b;
--text:#e6edf3;--leise:#8b98a5;--neon:#9ce430;--neon-hell:#b8f266;--neon-dunkel:#1d2a14;--warn:#f2b84b;--rund:8px;color-scheme:dark}
*,*::before,*::after{box-sizing:border-box}
html{background:var(--grund);-webkit-text-size-adjust:100%;text-size-adjust:100%}
body{margin:0;background:var(--grund);color:var(--text);font-family:"Xharig Sans","Segoe UI",system-ui,-apple-system,"Helvetica Neue",Arial,sans-serif;
font-size:17px;line-height:1.6;-webkit-font-smoothing:antialiased;min-height:100vh;display:flex;flex-direction:column}
a{color:var(--neon);text-decoration:none;text-underline-offset:3px}
a:hover{text-decoration:underline}
:focus-visible{outline:2px solid var(--neon);outline-offset:3px;border-radius:4px}
h1,h2{text-wrap:balance}
p,li{text-wrap:pretty}
.mitte{width:100%;max-width:760px;margin:0 auto;padding:0 24px}
.ic{width:1.2em;height:1.2em;flex:none}
.kopf{border-bottom:1px solid var(--linie)}
.kopf .mitte{display:flex;align-items:center;gap:12px;min-height:64px;flex-wrap:wrap;padding-top:8px;padding-bottom:8px}
.org{display:flex;align-items:center;gap:10px;min-width:0;font-weight:700;font-size:19px;overflow-wrap:anywhere}
.org .ic{color:var(--neon)}
.etikett{font-size:13px;font-weight:700;line-height:1.4;color:var(--neon);border:1px solid var(--neon);border-radius:6px;padding:1px 8px;white-space:nowrap}
main{flex:1;padding:40px 0 32px}
h1{font-size:clamp(28px,5vw,40px);line-height:1.15;letter-spacing:-.01em;margin:0 0 10px;overflow-wrap:anywhere}
.einleitung{color:var(--leise);margin:0 0 28px;max-width:40em}
.karte{position:relative;background:var(--feld);border:1px solid var(--linie);border-radius:var(--rund);padding:20px 22px;margin:0 0 20px}
.karte::before,.karte::after{content:"";position:absolute;width:22px;height:22px;pointer-events:none;border:0 solid var(--neon)}
.karte::before{top:-1px;left:-1px;border-top-width:2px;border-left-width:2px;border-top-left-radius:var(--rund)}
.karte::after{bottom:-1px;right:-1px;border-bottom-width:2px;border-right-width:2px;border-bottom-right-radius:var(--rund)}
.karte h2{display:flex;align-items:center;gap:10px;font-size:19px;line-height:1.3;margin:0 0 12px}
.karte h2 .ic{color:var(--neon)}
.karte > :last-child{margin-bottom:0}
.werte{display:grid;grid-template-columns:auto minmax(0,1fr);gap:0;margin:0}
.werte dt,.werte dd{padding:9px 0;border-top:1px solid var(--linie);margin:0}
.werte dt{padding-right:24px}
.ganz{white-space:nowrap}
.werte dt:first-of-type,.werte dd:first-of-type{border-top:0;padding-top:0}
.werte dt{color:var(--leise)}
.werte dd{font-weight:700;overflow-wrap:anywhere}
.liste{list-style:none;margin:0;padding:0}
.liste li{display:flex;align-items:flex-start;gap:12px;padding:9px 0;border-top:1px solid var(--linie)}
.liste li:first-child{border-top:0;padding-top:0}
.liste .ic{color:var(--neon);margin-top:.2em}
.knopfreihe{display:flex;gap:12px;flex-wrap:wrap;margin:20px 0 0}
.knopf{display:inline-flex;align-items:center;justify-content:center;gap:10px;min-height:48px;padding:10px 22px;border-radius:var(--rund);
border:2px solid var(--linie-hell);background:none;font:inherit;font-weight:700;font-size:17px;line-height:1.3;color:var(--text);cursor:pointer}
.knopf:hover{text-decoration:none;border-color:var(--neon);color:var(--neon);background:var(--neon-dunkel)}
.knopf.voll{background:var(--neon);border-color:var(--neon);color:var(--grund)}
.knopf.voll:hover{background:var(--neon-hell);border-color:var(--neon-hell);color:var(--grund)}
label.feld{display:block;margin:18px 0 6px;font-weight:700}
label.feld:first-of-type{margin-top:0}
input[type=text],input[type=password],input[type=search],input[type=date],select,textarea{width:100%;min-height:48px;padding:10px 14px;font:inherit;font-size:18px;
background:var(--grund);color:var(--text);border:1px solid var(--linie-hell);border-radius:var(--rund)}
textarea{min-height:110px;resize:vertical;line-height:1.5}
input,select,textarea{color-scheme:dark;caret-color:var(--neon);-webkit-text-fill-color:var(--text)}
input[type=password]{font-family:system-ui,"Segoe UI",Arial,sans-serif;letter-spacing:.06em;text-shadow:none;opacity:1;filter:none}
input::placeholder,textarea::placeholder{-webkit-text-fill-color:var(--leise)}
input:-webkit-autofill,input:-webkit-autofill:hover,input:-webkit-autofill:focus,textarea:-webkit-autofill,select:-webkit-autofill{
-webkit-text-fill-color:var(--text);caret-color:var(--neon);-webkit-box-shadow:0 0 0 1000px var(--grund) inset;box-shadow:0 0 0 1000px var(--grund) inset;
transition:background-color 86400s ease-out 0s}
input:autofill{-webkit-text-fill-color:var(--text);box-shadow:0 0 0 1000px var(--grund) inset}
input[type=text]:hover,input[type=password]:hover,input[type=search]:hover,input[type=date]:hover,select:hover,textarea:hover{border-color:var(--neon)}
input[type=text]:focus-visible,input[type=password]:focus-visible,input[type=search]:focus-visible,input[type=date]:focus-visible,select:focus-visible,
textarea:focus-visible{outline:2px solid var(--neon);outline-offset:1px}
code{font-family:ui-monospace,Consolas,monospace;font-size:.9em;background:var(--flaeche);border-radius:4px;padding:1px 6px;overflow-wrap:anywhere}
.unsichtbar{position:absolute;width:1px;height:1px;overflow:hidden;clip-path:inset(50%);white-space:nowrap}
.reiter{display:flex;flex-wrap:wrap;gap:8px;margin:0 0 28px}
.reiter a,.reiter button{display:inline-flex;align-items:center;gap:8px;min-height:44px;padding:6px 14px;border:1px solid var(--linie);border-radius:var(--rund);
background:none;color:var(--text);font:inherit;font-size:15px;font-weight:700;cursor:pointer}
.reiter a:hover,.reiter button:hover{text-decoration:none;border-color:var(--neon);color:var(--neon)}
.reiter a[aria-current=page]{border-color:var(--neon);color:var(--neon);background:var(--neon-dunkel)}
.reiter form{margin:0 0 0 auto}
.suche{display:flex;flex-wrap:wrap;gap:10px;margin:0 0 12px}
.suche input{flex:1 1 220px;width:auto}
.suche select{flex:0 1 180px;width:auto}
.zahl{color:var(--leise);font-size:14px;margin:0 0 12px}
.knopf.klein{min-height:44px;padding:6px 16px;font-size:15px}
.knopfreihe.oben{margin:0 0 20px}
.knopfreihe form{margin:0}
.zeilen{list-style:none;margin:0;padding:0}
.zeilen li{padding:12px 0;border-top:1px solid var(--linie);min-width:0}
.zeilen li:first-child{border-top:0;padding-top:0}
.zeilen li form{margin:10px 0 0}
.titel{display:block;font-weight:700;overflow-wrap:anywhere}
a.titel{color:var(--text)}
a.titel:hover{color:var(--neon)}
.meta{display:flex;flex-wrap:wrap;gap:4px 16px;color:var(--leise);font-size:15px;margin-top:4px}
.meta span{display:inline-flex;align-items:center;gap:6px;min-width:0;overflow-wrap:anywhere}
.zeile-kopf{display:flex;flex-wrap:wrap;align-items:baseline;justify-content:space-between;gap:4px 16px}
.marke{display:inline-block;font-size:13px;font-weight:700;line-height:1.4;border:1px solid var(--linie-hell);border-radius:6px;padding:1px 8px;white-space:nowrap}
.marke.an{color:var(--neon);border-color:var(--neon)}
.marke.fertig{color:var(--grund);background:var(--neon);border-color:var(--neon)}
.marke.aus{color:var(--leise)}
.marke.rot{color:var(--warn);border-color:var(--warn)}
.balken{height:8px;border-radius:4px;background:var(--linie);overflow:hidden;margin:8px 0 2px}
.balken span{display:block;height:100%;background:var(--neon)}
.paare{display:grid;grid-template-columns:auto minmax(0,1fr);gap:2px 16px;margin:6px 0 0;font-size:15px}
.paare dt{color:var(--leise)}
.paare dd{margin:0;overflow-wrap:anywhere}
.notiz{white-space:pre-wrap}
.leer{color:var(--leise);margin:0}
.klein{color:var(--leise);font-size:14px;margin:6px 0 0}
fieldset.waren{border:0;padding:0;margin:20px 0 0;min-width:0}
fieldset.waren legend{font-weight:700;padding:0;margin:0 0 6px}
.reihe{display:grid;grid-template-columns:minmax(0,3fr) minmax(0,1fr) minmax(0,1fr);gap:10px;margin:0 0 10px}
.kacheln{display:grid;grid-template-columns:repeat(auto-fill,minmax(210px,1fr));gap:16px;margin:0 0 4px}
.kachel{display:flex;flex-direction:column;gap:6px;margin:0;color:var(--text)}
.kachel:hover{text-decoration:none;border-color:var(--neon)}
.kachel-kopf{display:flex;align-items:center;gap:10px;color:var(--leise);font-size:15px}
.kachel-kopf .ic{color:var(--neon)}
.kachel-zahl{font-size:30px;font-weight:700;line-height:1.1}
.pruefung li .ic{margin-top:.2em}
.pruefung li.rot{color:var(--warn)}
.pruefung li.rot .ic{color:var(--warn)}
.abstand{margin-top:28px}
.wer{display:flex;align-items:center;gap:8px;color:var(--leise);font-size:15px;margin:-12px 0 8px;overflow-wrap:anywhere}
.wer .ic{color:var(--neon)}
.zeilen li form + form{margin-top:6px}
input::placeholder{color:var(--leise)}
input.code{letter-spacing:.18em;text-transform:uppercase;font-weight:700}
label.recht{display:flex;align-items:flex-start;gap:12px;padding:10px 0;border-top:1px solid var(--linie);cursor:pointer}
label.recht:first-of-type{border-top:0;padding-top:0}
label.recht:hover{color:var(--neon)}
label.recht input{width:20px;height:20px;margin:.2em 0 0;accent-color:var(--neon);flex:none;cursor:pointer}
.hinweis{color:var(--leise);font-size:15px;border-left:2px solid var(--linie-hell);padding-left:14px;margin:18px 0 0}
.meldung{display:flex;align-items:flex-start;gap:12px;margin:0 0 20px;padding:14px 16px;border:1px solid var(--linie-hell);border-radius:var(--rund);background:var(--flaeche)}
.meldung .ic{margin-top:.2em}
.meldung.gut .ic{color:var(--neon)}
.meldung.warn{border-color:var(--warn)}
.meldung.warn .ic{color:var(--warn)}
.fuss{border-top:1px solid var(--linie);padding:20px 0 28px;color:var(--leise);font-size:14px}
.fuss p{margin:0}
.fuss p + p{margin-top:6px}
.fuss a{color:var(--leise);text-decoration:underline;text-decoration-color:var(--linie-hell)}
.fuss a:hover{color:var(--neon)}
@media (max-width:640px){
body{font-size:16px}
.mitte{padding:0 16px}
main{padding-top:28px}
.karte{padding:18px 16px}
.werte{grid-template-columns:minmax(0,1fr)}
.werte dt{padding-bottom:0}
.werte dd{border-top:0;padding-top:2px}
.werte dd:first-of-type{padding-top:2px}
.knopfreihe .knopf{flex:1 1 auto}
.knopfreihe form{flex:1 1 auto;display:flex}
.reiter form{margin:0}
.reiter a,.reiter button{padding:6px 10px}
.reihe{grid-template-columns:minmax(0,1fr) minmax(0,1fr)}
.reihe input[name=item]{grid-column:1 / -1}
.paare{grid-template-columns:minmax(0,1fr)}
.paare dd{margin-bottom:4px}
.kacheln{grid-template-columns:minmax(0,1fr) minmax(0,1fr)}
.kachel-zahl{font-size:24px}
}
"#;

/// The stylesheet at `/assets/look.css`: the rules above and the widths of the progress bars in steps of five
/// (`w0` … `w100`), so no page needs a style attribute and the policy allows styles only from the server itself.
fn stylesheet() -> String {
    let widths: String = (0..=20).map(|step| format!(".balken .w{n}{{width:{n}%}}", n = step * 5)).collect();
    format!("{CSS}{widths}\n")
}

/// The class of a progress bar for a share between 0 and 1, rounded to five percent.
pub(crate) fn bar_class(share: f64) -> String {
    let step = (share.clamp(0.0, 1.0) * 20.0).round() as u32;
    format!("w{}", step * 5)
}

/// A Lucide icon inline, tinted by the text color around it and hidden from screen readers.
pub(crate) fn icon(name: &str) -> String {
    let svg = match name {
        "server" => include_str!("../../../tools/symbol-vorlagen/server.svg"),
        "users" => include_str!("../../../tools/symbol-vorlagen/users.svg"),
        "shield-check" => include_str!("../../../tools/symbol-vorlagen/shield-check.svg"),
        "clock" => include_str!("../../../tools/symbol-vorlagen/clock.svg"),
        "tag" => include_str!("../../../tools/symbol-vorlagen/tag.svg"),
        "external-link" => include_str!("../../../tools/symbol-vorlagen/external-link.svg"),
        "circle-check" => include_str!("../../../tools/symbol-vorlagen/circle-check.svg"),
        "circle-x" => include_str!("../../../tools/symbol-vorlagen/circle-x.svg"),
        "triangle-alert" => include_str!("../../../tools/symbol-vorlagen/triangle-alert.svg"),
        "info" => include_str!("../../../tools/symbol-vorlagen/info.svg"),
        "lock" => include_str!("../../../tools/symbol-vorlagen/lock.svg"),
        "monitor" => include_str!("../../../tools/symbol-vorlagen/monitor.svg"),
        "user" => include_str!("../../../tools/symbol-vorlagen/user.svg"),
        "check" => include_str!("../../../tools/symbol-vorlagen/check.svg"),
        "share-2" => include_str!("../../../tools/symbol-vorlagen/share-2.svg"),
        "activity" => include_str!("../../../tools/symbol-vorlagen/activity.svg"),
        "layout-dashboard" => include_str!("../../../tools/symbol-vorlagen/layout-dashboard.svg"),
        "warehouse" => include_str!("../../../tools/symbol-vorlagen/warehouse.svg"),
        "hammer" => include_str!("../../../tools/symbol-vorlagen/hammer.svg"),
        "clipboard-list" => include_str!("../../../tools/symbol-vorlagen/clipboard-list.svg"),
        "package" => include_str!("../../../tools/symbol-vorlagen/package.svg"),
        "package-plus" => include_str!("../../../tools/symbol-vorlagen/package-plus.svg"),
        "door-open" => include_str!("../../../tools/symbol-vorlagen/door-open.svg"),
        "arrow-left" => include_str!("../../../tools/symbol-vorlagen/arrow-left.svg"),
        "lock-open" => include_str!("../../../tools/symbol-vorlagen/lock-open.svg"),
        "funnel-x" => include_str!("../../../tools/symbol-vorlagen/funnel-x.svg"),
        "search" => include_str!("../../../tools/symbol-vorlagen/search.svg"),
        "plus" => include_str!("../../../tools/symbol-vorlagen/plus.svg"),
        "pencil" => include_str!("../../../tools/symbol-vorlagen/pencil.svg"),
        "ban" => include_str!("../../../tools/symbol-vorlagen/ban.svg"),
        "rotate-ccw" => include_str!("../../../tools/symbol-vorlagen/rotate-ccw.svg"),
        "save" => include_str!("../../../tools/symbol-vorlagen/save.svg"),
        "map-pin" => include_str!("../../../tools/symbol-vorlagen/map-pin.svg"),
        "timer" => include_str!("../../../tools/symbol-vorlagen/timer.svg"),
        "x" => include_str!("../../../tools/symbol-vorlagen/x.svg"),
        "log-in" => include_str!("../../../tools/symbol-vorlagen/log-in.svg"),
        "user-check" => include_str!("../../../tools/symbol-vorlagen/user-check.svg"),
        "settings" => include_str!("../../../tools/symbol-vorlagen/settings.svg"),
        _ => return String::new(),
    };
    let compact: String = svg.split_whitespace().collect::<Vec<_>>().join(" ").replace("> <", "><");
    compact.replacen("<svg ", "<svg class=\"ic\" aria-hidden=\"true\" focusable=\"false\" ", 1)
}

/// German when the first language the browser asks for is German, else English.
pub(crate) fn german(request: &Request) -> bool {
    request
        .header("Accept-Language")
        .and_then(|value| value.split(',').next())
        .is_some_and(|first| first.trim().to_ascii_lowercase().starts_with("de"))
}

/// One of the two texts for the language.
pub(crate) fn pick<'a>(german: bool, de: &'a str, en: &'a str) -> &'a str {
    if german { de } else { en }
}

/// A whole page: the head with the org's name and tag, `body` as the main part, and the quiet foot that names the
/// open standard and the licenses of font and icons.
pub(crate) fn page(german: bool, org: &str, tag: &str, title: &str, body: &str) -> String {
    page_with_head(german, org, tag, title, body, "")
}

/// A whole page as [`page`] with `head` (markup of this server only) at the end of the head.
pub(crate) fn page_with_head(german: bool, org: &str, tag: &str, title: &str, body: &str, head: &str) -> String {
    let lang = pick(german, "de", "en");
    let tag = if tag.is_empty() { String::new() } else { format!("<span class=\"etikett\">{}</span>", escape_html(tag)) };
    let runs = pick(
        german,
        "Läuft mit dem offenen <a href=\"{p}\">VerseKit</a>-Orga-Standard <span class=\"ganz\">(versekit-org v1)</span>.",
        "Runs on the open <a href=\"{p}\">VerseKit</a> org standard <span class=\"ganz\">(versekit-org v1)</span>.",
    )
    .replace("{p}", PROJECT);
    let licenses = pick(
        german,
        "Schrift Xharig Sans (Lato, <a href=\"/assets/OFL.txt\">SIL OFL 1.1</a>) · Symbole Lucide (<a href=\"/assets/LICENSE-lucide.txt\">ISC</a>)",
        "Font Xharig Sans (Lato, <a href=\"/assets/OFL.txt\">SIL OFL 1.1</a>) · Icons Lucide (<a href=\"/assets/LICENSE-lucide.txt\">ISC</a>)",
    );
    format!(
        "<!doctype html><html lang=\"{lang}\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <meta name=\"color-scheme\" content=\"dark\"><meta name=\"robots\" content=\"noindex\">\
         <title>{title} · {org_title}</title>\
         <link rel=\"stylesheet\" href=\"/assets/look.css?v={version}\">{head}</head><body>\
         <header class=\"kopf\"><div class=\"mitte\"><span class=\"org\">{server}<span>{org}</span></span>{tag}</div></header>\
         <main><div class=\"mitte\">{body}</div></main>\
         <footer class=\"fuss\"><div class=\"mitte\"><p>{runs}</p><p>{licenses}</p></div></footer>\
         </body></html>",
        title = escape_html(title),
        version = env!("CARGO_PKG_VERSION"),
        org_title = escape_html(org),
        org = escape_html(org),
        server = icon("server"),
    )
}

/// The files under `/assets/`: the stylesheet, the two fonts and the licenses of font and icons; `None` for anything
/// else.
pub(crate) fn asset(path: &str) -> Option<Response> {
    let sheet;
    let (kind, body): (&str, &[u8]) = match path {
        "/assets/look.css" => {
            sheet = stylesheet();
            ("text/css; charset=utf-8", sheet.as_bytes())
        }
        "/assets/xharig-sans-400.woff2" => ("font/woff2", FONT_400),
        "/assets/xharig-sans-700.woff2" => ("font/woff2", FONT_700),
        "/assets/OFL.txt" => ("text/plain; charset=utf-8", FONT_LICENSE.as_bytes()),
        "/assets/LICENSE-lucide.txt" => ("text/plain; charset=utf-8", ICON_LICENSE.as_bytes()),
        _ => return None,
    };
    Some(Response {
        status: 200,
        headers: vec![
            ("Content-Type".into(), kind.into()),
            ("Cache-Control".into(), "public, max-age=604800".into()),
            ("X-Content-Type-Options".into(), "nosniff".into()),
        ],
        body: body.to_vec(),
    })
}
