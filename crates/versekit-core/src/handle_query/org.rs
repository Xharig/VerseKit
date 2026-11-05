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

//! Organizations searched by their SID or their name, from the public pages of the RSI website: the page of an
//! organization (`/orgs/<SID>`) and the search of the list of organizations, which the site loads from
//! [`SEARCH_API`] (a JSON body sent, a JSON answer with the list as HTML in `data.html`, one
//! `<div class="org-cell">` per organization). The site finds a name only as a whole, capitals and small letters
//! alike; its `totalrows` counts every organization and says nothing.
//!
//! [`lookup`] takes what the player typed into the field of the search: a text a handle can be is asked as a
//! handle first, then as a SID, then as a name; any other text only as a name. Every page goes through the gate
//! of the handle search, with its pause.

use super::cache::{self, GATE, Gate, Paced};
use super::page::{self, Org, after, between, clean, element, image_in, valid_handle};
use super::{Failure, text_of};
use crate::jsonio::{self, Value};
use crate::net::{self, Net};
use crate::paths::Paths;
use crate::text::py_strip;

/// The address the list of organizations loads its search from.
pub const SEARCH_API: &str = "https://robertsspaceindustries.com/api/orgs/getOrgs";
/// How many organizations one search asks for, as the site asks it.
pub const SEARCH_PAGE_SIZE: usize = 12;
/// The longest name searched.
pub const NAME_MAX: usize = 100;

/// One organization the search of names found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hit {
    pub sid: String,
    pub name: String,
    /// The address of its logo.
    pub logo: String,
    pub members: Option<u64>,
}

/// An organization from its page, and when it was fetched (seconds).
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub org: Org,
    pub fetched: f64,
}

/// What a search of the field found.
#[derive(Clone, Debug, PartialEq)]
pub enum Answer {
    Handle(Box<cache::Found>),
    Org(Found),
    /// Several organizations of that name, in the order of the site.
    Choices(Vec<Hit>),
}

/// The text typed as a name to search, stripped; `None` when it is empty, too long or holds a control character.
pub fn valid_name(text: &str) -> Option<String> {
    let text = py_strip(text);
    let fine = !text.is_empty() && text.chars().count() <= NAME_MAX && !text.chars().any(char::is_control);
    fine.then(|| text.to_string())
}

/// The first number in `text` (`3723 members`, `5,010`).
fn number(text: &str) -> Option<u64> {
    clean(text).split_ascii_whitespace().next()?.replace([',', '.'], "").parse().ok()
}

/// The organization on `html`, its page; `None` when the page shows none (an unknown SID gets a page without it).
pub fn parse_org_page(html: &str) -> Option<Org> {
    let body = after(html, "id=\"organization\"")?;
    let heading = after(body, "class=\"heading").unwrap_or(body);
    let heading = heading.find("<hr").map_or(heading, |end| &heading[..end]);
    let title = element(heading, "h1")?;
    let sid = between(title, "class=\"symbol\">", "<").map(clean).and_then(|sid| valid_handle(&sid))?;
    // The name stands in front of the SID: `Name / <span class="symbol">SID</span>`.
    let name = clean(title.find("<span").map_or(title, |at| &title[..at]));
    let name = py_strip(name.trim_end_matches('/')).to_string();
    let activity = |class: &str| {
        after(heading, class).and_then(|part| between(part, "alt=\"", "\"")).map(clean).unwrap_or_default()
    };
    Some(Org {
        name,
        logo: after(heading, "class=\"logo").map(image_in).unwrap_or_default(),
        members: between(heading, "class=\"count\">", "</span>").and_then(number),
        commitment: between(heading, "class=\"commitment\">", "</li>").map(clean).unwrap_or_default(),
        activity: activity("class=\"primary"),
        second_activity: activity("class=\"secondary"),
        sid,
        ..Org::default()
    })
}

/// The body sent to search `name`; the lists of the filters stay empty.
pub fn search_body(name: &str) -> String {
    let name = jsonio::dumps(&Value::str(name), None, false);
    format!(
        r#"{{"sort":"","search":{name},"commitment":[],"roleplay":[],"size":[],"model":[],"activity":[],"language":[],"recruiting":[],"pagesize":{SEARCH_PAGE_SIZE},"page":1}}"#
    )
}

/// One organization in its cell of the list.
fn hit_in(part: &str) -> Option<Hit> {
    let sid = between(part, "href=\"/orgs/", "\"").map(clean).and_then(|sid| valid_handle(&sid))?;
    Some(Hit {
        name: element(part, "h3").map(clean).unwrap_or_default(),
        logo: image_in(part),
        members: after(part, "Members:").and_then(|rest| between(rest, "class=\"value\">", "<")).and_then(number),
        sid,
    })
}

/// The organizations of the answer to a search, each once, in its order; none when the site said no.
///
/// # Errors
///
/// [`Failure::Failed`] when the answer is no JSON of that form.
pub fn parse_search(text: &str) -> Result<Vec<Hit>, Failure> {
    let wrong = || Failure::Failed("JSON".to_string());
    let answer = jsonio::loads(text).map_err(|_| wrong())?;
    if !matches!(answer.get("success"), Some(Value::Int(n)) if n == "1") {
        return Ok(Vec::new());
    }
    let html = answer.get("data").and_then(|data| data.get("html")).and_then(Value::as_str).ok_or_else(wrong)?;
    let mut hits: Vec<Hit> = Vec::new();
    for part in html.split("class=\"org-cell").skip(1) {
        if let Some(hit) = hit_in(part)
            && !hits.iter().any(|h| h.sid.eq_ignore_ascii_case(&hit.sid))
        {
            hits.push(hit);
        }
    }
    hits.truncate(SEARCH_PAGE_SIZE);
    Ok(hits)
}

/// The organization `sid` from its page.
///
/// # Errors
///
/// [`Failure::NotFound`] when the site knows no such organization, else as [`Net::get`].
pub fn fetch_org(net: &dyn Net, sid: &str) -> Result<Org, Failure> {
    parse_org_page(&text_of(&net.get(&page::org_url(sid))?)).ok_or(Failure::NotFound)
}

/// The organizations the site finds for `name`.
///
/// # Errors
///
/// As [`Net::post`] and [`parse_search`].
pub fn fetch_names(net: &dyn Net, name: &str) -> Result<Vec<Hit>, Failure> {
    let body = net.post(SEARCH_API, "application/json", search_body(name).as_bytes())?;
    parse_search(&text_of(&body))
}

/// The organization `sid` from the site, at `now`.
///
/// # Errors
///
/// [`Failure`] when `sid` is no SID, the site knows none or cannot be reached; [`Failure::Busy`] while the same
/// organization is being asked.
pub fn org(paths: &Paths, net: &dyn Net, sid: &str, now: f64) -> Result<Found, Failure> {
    org_with(&GATE, paths, net, sid, now)
}

/// [`org`] through `gate`.
///
/// # Errors
///
/// As [`org`].
pub fn org_with(gate: &Gate, paths: &Paths, net: &dyn Net, sid: &str, now: f64) -> Result<Found, Failure> {
    let sid = valid_handle(sid).ok_or(Failure::Invalid)?;
    if net::offline_in(&paths.host) {
        return Err(Failure::Offline);
    }
    let Some(_ticket) = gate.enter(&format!("orgseite:{}", sid.to_lowercase())) else { return Err(Failure::Busy) };
    let org = fetch_org(&Paced { net, gate }, &sid)?;
    Ok(Found { org, fetched: now })
}

/// Searches what the player typed: a handle, the SID of an organization or its name, in that order. `force`
/// fetches a handle anew. One organization of the name searched opens at once, several are the choices.
///
/// # Errors
///
/// [`Failure::Invalid`] when the text cannot be searched, [`Failure::NotFound`] when nothing was found, else
/// the failure of the first page that failed.
pub fn lookup(paths: &Paths, net: &dyn Net, text: &str, force: bool, now: f64) -> Result<Answer, Failure> {
    lookup_with(&GATE, paths, net, text, force, now)
}

/// [`lookup`] through `gate`.
///
/// # Errors
///
/// As [`lookup`].
pub fn lookup_with(
    gate: &Gate,
    paths: &Paths,
    net: &dyn Net,
    text: &str,
    force: bool,
    now: f64,
) -> Result<Answer, Failure> {
    let name = valid_name(text).ok_or(Failure::Invalid)?;
    if let Some(handle) = valid_handle(&name) {
        match cache::search_with(gate, paths, net, &handle, force, now) {
            Err(Failure::NotFound) => {}
            other => return other.map(|found| Answer::Handle(Box::new(found))),
        }
        match org_with(gate, paths, net, &handle, now) {
            Err(Failure::NotFound) => {}
            other => return other.map(Answer::Org),
        }
    }
    if net::offline_in(&paths.host) {
        return Err(Failure::Offline);
    }
    let hits = {
        let Some(_ticket) = gate.enter(&format!("orgsuche:{}", name.to_lowercase())) else {
            return Err(Failure::Busy);
        };
        fetch_names(&Paced { net, gate }, &name)?
    };
    match hits.as_slice() {
        [] => Err(Failure::NotFound),
        [one] => Ok(org_with(gate, paths, net, &one.sid, now).map_or(Answer::Choices(hits.clone()), Answer::Org)),
        _ => Ok(Answer::Choices(hits)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::Fake;
    use crate::testdir::Dir as TestDir;
    use std::time::Duration;

    /// The page of an organization built by hand in the shape of the site: made-up names, the noise of the real
    /// page (banner, decoration, the description below) kept.
    pub(crate) const ORG_PAGE: &str = r#"<html><body>
<div id="post-background" style="background-image:url('/media/x/background_blur/TEST-Background.jpg');"></div>
<div id="contentbody" class="public profile" style="">
<div id="organization" class="public">
  <div class="wrapper"><div class="content-wrapper">
  <div class="heading ">
    <div class="banner"><img src="/media/test/banner/TEST-Banner.png" /></div>
    <div class="inner">
      <div class="logo noshadow">
        <img src="/media/test/logo/TEST-Logo.png" />
        <span class="count">1,234 members</span>
      </div>
      <h1>Test &amp; Geschwader / <span class="symbol">TEST</span></h1>
      <ul class="tags clearfix">
        <li class="model">Organization</li>
        <li class="commitment">Regular</li>
      </ul>
      <ul class="focus clearfix">
        <li class="primary tooltip-wrap">
          <img src="/media/i/icon/Security.png" alt="Security" />
          <div class="rsi-tooltip"><div class="content">Security</div><span class="bottom"></span></div>
        </li>
        <li class="secondary tooltip-wrap">
          <img src="/media/i/icon/Trade.png" alt="Trading" />
          <div class="rsi-tooltip"><div class="content">Trading</div><span class="bottom"></span></div>
        </li>
      </ul>
    </div>
    <hr />
  </div>
  <div class="content join-us clearfix">
    <ul class="subnav clearfix"><li><a href="/orgs/TEST" class="trans-02s active">Description</a></li>
    <li><a href="/orgs/TEST/members" class="trans-02s">Members</a></li></ul>
    <div class="body markitup-text"><h1>Willkommen</h1><p>Eine <span class="symbol">ANDERE</span> Org.</p></div>
  </div>
  </div></div>
</div></div></body></html>"#;

    /// The answer of the site to a search, built the same way: two organizations of one name and one twice.
    const SEARCH: &str = r#"{"success":1,"code":"OK","msg":"OK","data":{"totalrows":99493,"html":"\t<div class=\"org-cell \">\n  <a class=\"trans-03s clearfix\" href=\"\/orgs\/TEST\">\n  <span class=\"left\"><span class=\"thumb\"><img src=\"\/media\/t\/avatar\/TEST-Logo.png\" \/><\/span>\n  <span class=\"identity\"><h3 class=\"trans-03s name\">Test &amp; Geschwader<\/h3><span class=\"symbol\">TEST<\/span><\/span><\/span>\n  <span class=\"right\"><span class=\"infocontainer\"><span class=\"infoitem\"><span class=\"label\">Archetype: <\/span><span class=\"value\">Organization<\/span><\/span>\n  <span class=\"infoitem\"><span class=\"label\">Commitment: <\/span><span class=\"value regular\">Regular<\/span><\/span><\/span>\n  <span class=\"infocontainer\"><span class=\"infoitem\"><span class=\"label\">Members: <\/span><span class=\"value\">3724<\/span><\/span><\/span><\/span>\n  <\/a><div class=\"controls\"><\/div><\/div>\t<div class=\"org-cell \">\n  <a class=\"trans-03s clearfix\" href=\"\/orgs\/TESTZWEI\">\n  <span class=\"identity\"><h3 class=\"trans-03s name\">Test &amp; Geschwader<\/h3><span class=\"symbol\">TESTZWEI<\/span><\/span>\n  <span class=\"infoitem\"><span class=\"label\">Members: <\/span><span class=\"value\">12<\/span><\/span>\n  <\/a><\/div>\t<div class=\"org-cell \">\n  <a class=\"trans-03s clearfix\" href=\"\/orgs\/test\"><h3 class=\"name\">Doppelt<\/h3><\/a><\/div>"}}"#;

    /// An answer with one organization.
    const ONE: &str = r#"{"success":1,"data":{"html":"<div class=\"org-cell\"><a href=\"/orgs/EINZEL\"><h3 class=\"name\">Einzel Staffel</h3></a></div>"}}"#;

    fn quick() -> Gate {
        Gate::new(Duration::ZERO)
    }

    fn page_of(sid: &str, name: &str) -> String {
        ORG_PAGE.replace("Test &amp; Geschwader", name).replace(">TEST<", &format!(">{sid}<"))
    }

    #[test]
    fn the_page_of_an_organization_is_read_from_its_heading() {
        let org = parse_org_page(ORG_PAGE).expect("an organization");
        assert_eq!((org.sid.as_str(), org.name.as_str()), ("TEST", "Test & Geschwader"));
        assert_eq!(org.members, Some(1234));
        assert_eq!(
            org.logo, "https://robertsspaceindustries.com/media/test/logo/TEST-Logo.png",
            "the logo, not the banner"
        );
        assert_eq!(org.commitment, "Regular");
        assert_eq!((org.activity.as_str(), org.second_activity.as_str()), ("Security", "Trading"));
        assert!(org.rank.is_empty() && org.stars == 0, "a rank belongs to a member only");
        // Counter-checks: the page of an unknown SID and a heading without a SID show none.
        assert_eq!(parse_org_page("<html><title>404 - Roberts Space Industries</title></html>"), None);
        assert_eq!(parse_org_page(&ORG_PAGE.replacen("class=\"symbol\"", "class=\"sym\"", 1)), None);
    }

    #[test]
    fn the_answer_to_a_search_gives_each_organization_once() {
        let hits = parse_search(SEARCH).expect("JSON");
        let shown: Vec<(&str, &str, Option<u64>)> =
            hits.iter().map(|h| (h.sid.as_str(), h.name.as_str(), h.members)).collect();
        assert_eq!(shown, [("TEST", "Test & Geschwader", Some(3724)), ("TESTZWEI", "Test & Geschwader", Some(12))]);
        assert_eq!(hits[0].logo, "https://robertsspaceindustries.com/media/t/avatar/TEST-Logo.png");
        assert!(hits[1].logo.is_empty());
        assert_eq!(parse_search(r#"{"success":0,"data":null}"#), Ok(Vec::new()), "a refusal finds nothing");
        assert!(parse_search("<html>Wartung</html>").is_err());
        assert!(parse_search(r#"{"success":1,"data":{}}"#).is_err());
        assert!(search_body("Das \"Kartell\"").contains(r#""search":"Das \"Kartell\"","#), "the name is escaped");
    }

    #[test]
    fn names_that_cannot_be_searched_are_refused() {
        assert_eq!(valid_name("  Das Kartell ").as_deref(), Some("Das Kartell"));
        for bad in ["", "   ", "a\u{7}b", "Zeile\nzwei", &"x".repeat(NAME_MAX + 1)] {
            assert_eq!(valid_name(bad), None, "{bad:?}");
        }
    }

    /// A site with the handle `Musterpilot`, the organization `TEST` and two organizations named `Zwilling`.
    fn site() -> Fake {
        Fake::default()
            .with(&page::profile_url("Musterpilot"), page::tests::PROFILE)
            .with(&page::orgs_url("Musterpilot"), page::tests::ORGS)
            .with(&page::org_url("TEST"), ORG_PAGE)
            .with(&page::org_url("EINZEL"), page_of("EINZEL", "Einzel Staffel"))
            .with_post(SEARCH_API, search_body("Einzel Staffel"), ONE)
            .with_post(SEARCH_API, search_body("Zwilling"), SEARCH)
            .with_post(SEARCH_API, search_body("Niemand"), r#"{"success":1,"data":{"totalrows":5,"html":""}}"#)
    }

    #[test]
    fn a_handle_is_asked_first_then_the_sid_then_the_name() {
        let dir = TestDir::new("org-suche-reihenfolge");
        let paths = cache::tests::paths(&dir, &[]);
        let gate = quick();
        let net = site();
        let Ok(Answer::Handle(found)) = lookup_with(&gate, &paths, &net, " Musterpilot ", false, 1000.0) else {
            panic!("a handle")
        };
        assert_eq!(found.card.profile.handle, "Musterpilot");
        assert!(net.asked().iter().all(|url| url.contains("/citizens/")), "no organization asked: {:?}", net.asked());
        let net = site();
        let Ok(Answer::Org(found)) = lookup_with(&gate, &paths, &net, "TEST", false, 1000.0) else { panic!("the SID") };
        assert_eq!((found.org.sid.as_str(), found.fetched), ("TEST", 1000.0));
        assert_eq!(net.asked(), [page::profile_url("TEST"), page::org_url("TEST")], "no handle TEST, then the SID");
        let net = site();
        let Ok(Answer::Choices(hits)) = lookup_with(&gate, &paths, &net, "Zwilling", false, 0.0) else {
            panic!("the name")
        };
        assert_eq!(hits.len(), 2);
        assert_eq!(net.asked().len(), 3, "handle, SID, name: {:?}", net.asked());
    }

    #[test]
    fn a_name_with_spaces_is_only_searched_and_one_organization_opens() {
        let dir = TestDir::new("org-suche-name");
        let paths = cache::tests::paths(&dir, &[]);
        let gate = quick();
        let net = site();
        let Ok(Answer::Org(found)) = lookup_with(&gate, &paths, &net, "Einzel Staffel", false, 0.0) else {
            panic!("one organization")
        };
        assert_eq!((found.org.sid.as_str(), found.org.name.as_str()), ("EINZEL", "Einzel Staffel"));
        assert_eq!(
            net.asked(),
            [format!("POST {SEARCH_API} {}", search_body("Einzel Staffel")), page::org_url("EINZEL")],
            "no handle with a space is asked"
        );
        assert_eq!(lookup_with(&gate, &paths, &site(), "Niemand", false, 0.0), Err(Failure::NotFound));
        // Counter-check: a hit whose page does not come stands as the one choice.
        let net = Fake::default().with_post(SEARCH_API, search_body("Einzel Staffel"), ONE);
        let answer = lookup_with(&gate, &paths, &net, "Einzel Staffel", false, 0.0);
        assert!(
            matches!(answer, Ok(Answer::Choices(ref hits)) if hits.len() == 1 && hits[0].sid == "EINZEL"),
            "{answer:?}"
        );
    }

    #[test]
    fn without_net_or_with_a_bad_text_nothing_is_asked() {
        let dir = TestDir::new("org-suche-offline");
        let gate = quick();
        let offline = cache::tests::paths(&dir, &[(crate::legacy::ENV_NO_NET, "1")]);
        let net = site();
        assert_eq!(lookup_with(&gate, &offline, &net, "Das Kartell", false, 0.0), Err(Failure::Offline));
        assert_eq!(lookup_with(&gate, &offline, &net, "TEST", false, 0.0), Err(Failure::Offline));
        assert_eq!(org_with(&gate, &offline, &net, "TEST", 0.0), Err(Failure::Offline));
        let online = cache::tests::paths(&dir, &[]);
        assert_eq!(lookup_with(&gate, &online, &net, " \n ", false, 0.0), Err(Failure::Invalid));
        assert_eq!(org_with(&gate, &online, &net, "a/b", 0.0), Err(Failure::Invalid));
        assert!(net.asked().is_empty(), "{:?}", net.asked());
        let ticket = gate.enter("orgseite:test").expect("free");
        assert_eq!(org_with(&gate, &online, &net, "TEST", 0.0), Err(Failure::Busy));
        drop(ticket);
        assert!(org_with(&gate, &online, &net, "TEST", 0.0).is_ok());
    }
}
