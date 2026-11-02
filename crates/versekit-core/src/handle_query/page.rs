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

//! Reading the two public pages of a citizen: the profile (`/citizens/<handle>`) and the organizations
//! (`/citizens/<handle>/organizations`).
//!
//! ⚠ The classes beside `label` and `value` change from page to page (`value data12`): only the words in front
//! of a class count, and a value is found by the label text in front of it, never by its position.
//!
//! Only what the pages show to everyone is taken, and of that only what the search shows: no location, no bio.

use crate::stock_fetch::html_unescape;
use crate::text::py_strip;

/// The site the pages are on.
pub const BASE: &str = "https://robertsspaceindustries.com";
/// The longest handle the site gives out.
pub const HANDLE_MAX: usize = 60;

/// The handle typed, stripped; `None` when it has a character no handle has (letters, digits, `_`, `-` only).
pub fn valid_handle(text: &str) -> Option<String> {
    let text = py_strip(text);
    let fine = !text.is_empty()
        && text.chars().count() <= HANDLE_MAX
        && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    fine.then(|| text.to_string())
}

/// The profile page of a handle.
pub fn profile_url(handle: &str) -> String {
    format!("{BASE}/citizens/{handle}")
}

/// The page of the organizations of a handle.
pub fn orgs_url(handle: &str) -> String {
    format!("{BASE}/citizens/{handle}/organizations")
}

/// The page of an organization.
pub fn org_url(sid: &str) -> String {
    format!("{BASE}/orgs/{sid}")
}

/// The page of the members of an organization.
pub fn members_url(sid: &str) -> String {
    format!("{BASE}/orgs/{sid}/members")
}

/// What the profile page shows of a citizen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Profile {
    /// The handle as the page writes it.
    pub handle: String,
    /// The name shown above it (community moniker).
    pub name: String,
    /// The date of enlisting as the page writes it (`Apr 8, 2015`).
    pub enlisted: String,
    /// The title under the name (`Wing Commander`) and the address of its picture.
    pub title: String,
    pub title_image: String,
    /// The address of the profile picture.
    pub image: String,
    /// The number of the citizen as the page writes it (`#100001`).
    pub record: String,
    /// The languages the citizen speaks, in the order of the page.
    pub fluency: Vec<String>,
}

/// One organization a citizen shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Org {
    pub sid: String,
    pub name: String,
    pub rank: String,
    /// How many of the five rank stars are lit.
    pub stars: u8,
    pub members: Option<u64>,
    /// The address of the logo; only shown as a link.
    pub logo: String,
    /// Only for the main organization: what it does and how much it asks of its members.
    pub activity: String,
    pub second_activity: String,
    pub commitment: String,
}

/// A membership: shown, or hidden or redacted by the citizen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Membership {
    Shown(Org),
    Hidden,
}

impl Membership {
    pub fn org(&self) -> Option<&Org> {
        match self {
            Membership::Shown(org) => Some(org),
            Membership::Hidden => None,
        }
    }
}

/// The organizations of a citizen: the main one and the affiliations, in the order of the page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Orgs {
    pub main: Option<Membership>,
    pub affiliations: Vec<Membership>,
}

impl Orgs {
    /// The organizations shown, the main one first.
    pub fn shown(&self) -> impl Iterator<Item = &Org> {
        self.main.iter().chain(self.affiliations.iter()).filter_map(Membership::org)
    }
}

/// The part of `html` from `from` on, or nothing.
fn after<'a>(html: &'a str, from: &str) -> Option<&'a str> {
    html.find(from).map(|at| &html[at + from.len()..])
}

/// The text between `start` and the next `end` after it.
pub(super) fn between<'a>(html: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let rest = after(html, start)?;
    rest.find(end).map(|at| &rest[..at])
}

/// The text of an element without its tags, decoded and stripped.
pub(super) fn clean(raw: &str) -> String {
    let mut text = String::with_capacity(raw.len());
    let mut in_tag = false;
    for c in raw.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(c),
            _ => {}
        }
    }
    py_strip(&html_unescape(&text)).to_string()
}

/// The content of the next element `tag` in `html`: everything between the end of its opening tag and its
/// closing tag.
fn element<'a>(html: &'a str, tag: &str) -> Option<&'a str> {
    let open = html.find(&format!("<{tag}"))?;
    let rest = &html[open..];
    let start = rest.find('>')? + 1;
    let end = rest[start..].find(&format!("</{tag}>"))?;
    Some(&rest[start..start + end])
}

/// The value written after the label `label`: the next `<strong>` behind `label</span>` in the same entry.
fn value_after(html: &str, label: &str) -> Option<String> {
    let mut rest = html;
    while let Some(at) = rest.find(label) {
        let behind = &rest[at + label.len()..];
        let tail = behind.trim_start();
        if tail.starts_with("</span>") {
            let entry = tail.find("</p>").map_or(tail, |end| &tail[..end]);
            return element(entry, "strong").map(clean).filter(|v| !v.is_empty());
        }
        rest = behind;
    }
    None
}

/// The address in the first `src="…"` of `html`, made whole (`/media/…` → `https://robertsspaceindustries.com/media/…`).
/// Only an address on https is kept.
fn image_in(html: &str) -> String {
    between(html, "src=\"", "\"").map(whole_url).unwrap_or_default()
}

fn whole_url(raw: &str) -> String {
    let raw = html_unescape(py_strip(raw));
    let url = if raw.starts_with('/') && !raw.starts_with("//") { format!("{BASE}{raw}") } else { raw };
    if url.starts_with("https://") && !url.contains(['"', '<', '>', ' ']) { url } else { String::new() }
}

/// Whether the class attribute starting `html` names `word` (`class="value data12"` names `value`).
pub(super) fn class_names(class: &str, word: &str) -> bool {
    class.split_ascii_whitespace().any(|w| w == word)
}

/// The profile shown on `html`; `None` when the page names no handle (no profile page).
pub fn parse_profile(html: &str) -> Option<Profile> {
    let handle = value_after(html, "Handle name")?;
    let block = after(html, "class=\"profile ").unwrap_or(html);
    let block = block.find("class=\"main-org").map_or(block, |end| &block[..end]);
    let image = after(block, "class=\"thumb\"").map(image_in).unwrap_or_default();
    let name = after(block, "class=\"info\"").and_then(|info| element(info, "strong")).map(clean).unwrap_or_default();
    let (title, title_image) = match after(block, "class=\"icon\"") {
        Some(icon) => {
            let picture = image_in(icon);
            let text = after(icon, "class=\"value\"").and_then(|v| v.find('<').map(|end| &v[1..end]));
            (text.map(clean).unwrap_or_default(), picture)
        }
        None => (String::new(), String::new()),
    };
    Some(Profile {
        handle,
        name,
        enlisted: value_after(html, "Enlisted").unwrap_or_default(),
        title,
        title_image,
        image,
        record: value_after(html, "UEE Citizen Record").unwrap_or_default(),
        fluency: value_after(html, "Fluency")
            .map(|list| list.split(',').map(|l| py_strip(l).to_string()).filter(|l| !l.is_empty()).collect())
            .unwrap_or_default(),
    })
}

/// The kind of an organization box.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Main,
    Affiliation,
}

/// One box of the page of organizations, by its class: its kind and whether it is shown (`visibility-V`).
fn box_kind(class: &str) -> Option<(Kind, bool)> {
    let kind = if class_names(class, "main") {
        Kind::Main
    } else if class_names(class, "affiliation") {
        Kind::Affiliation
    } else {
        return None;
    };
    Some((kind, class.split_ascii_whitespace().any(|w| w == "visibility-V")))
}

/// The organization in one box; `None` when the box shows none (hidden or redacted).
fn org_in(part: &str, main: bool) -> Option<Org> {
    let sid = between(part, "href=\"/orgs/", "\"").map(clean)?;
    if sid.is_empty() || !sid.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return None;
    }
    let link = format!("href=\"/orgs/{sid}\"");
    let logo = after(part, &link).map(image_in).unwrap_or_default();
    let members = between(part, "class=\"members\">", "</span>")
        .and_then(|m| clean(m).split_ascii_whitespace().next().map(str::to_string))
        .and_then(|m| m.replace([',', '.'], "").parse().ok());
    // The name is the text of the link whose class names `value`.
    let mut name = String::new();
    let mut rest = part;
    while let Some(at) = rest.find(&link) {
        let tail = &rest[at + link.len()..];
        let Some(close) = tail.find('>') else { break };
        let class = between(&tail[..close], "class=\"", "\"").unwrap_or_default();
        if class_names(class, "value") {
            name =
                tail[close + 1..].find("</a>").map(|end| clean(&tail[close + 1..close + 1 + end])).unwrap_or_default();
            break;
        }
        rest = tail;
    }
    let stars = between(part, "class=\"ranking", "</div>")
        .map_or(0, |ranking| ranking.matches("class=\"active\"").count().min(5) as u8);
    let value = |label: &str| if main { value_after(part, label).unwrap_or_default() } else { String::new() };
    Some(Org {
        rank: value_after(part, "Organization rank").unwrap_or_default(),
        activity: value("Prim. Activity"),
        second_activity: value("Sec. Activity"),
        commitment: value("Commitment"),
        sid,
        name,
        stars,
        members,
        logo,
    })
}

/// The organizations shown on `html`, the page of organizations of a citizen.
pub fn parse_orgs(html: &str) -> Orgs {
    const BOX: &str = "class=\"box-content org";
    let mut orgs = Orgs::default();
    let starts: Vec<usize> = html.match_indices(BOX).map(|(at, _)| at).collect();
    for (index, &start) in starts.iter().enumerate() {
        let end = starts.get(index + 1).copied().unwrap_or(html.len());
        let part = &html[start..end];
        let class = between(part, "class=\"", "\"").unwrap_or_default();
        let Some((kind, visible)) = box_kind(class) else { continue };
        let membership = match visible.then(|| org_in(part, kind == Kind::Main)).flatten() {
            Some(org) => Membership::Shown(org),
            None => Membership::Hidden,
        };
        match kind {
            Kind::Main => orgs.main = Some(membership),
            Kind::Affiliation => orgs.affiliations.push(membership),
        }
    }
    orgs
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A profile page built by hand in the shape of the site: made-up names, the noise of the real page
    /// (changing classes, decoration, the parts around) kept.
    pub(crate) const PROFILE: &str = r#"<html><head><title>Musterpilot | Testflug</title></head><body>
<div class="page-wrapper">
  <p class="entry citizen-record"><span class="label">UEE Citizen Record</span>
    <strong class="value">#100001</strong></p>
  <div class="box-content profile-wrapper clearfix"><div class="inner-bg clearfix">
    <div class="profile left-col">
      <span class="title">Profile </span>
      <div class="inner clearfix">
        <div class="thumb">
          <img src="/media/beispiel/heap_infobox/bild.png" />
          <span class="deco-corner top left"></span>
        </div>
        <div class="info">
          <p class="entry">
            <strong class="value">Muster &amp; Co</strong>
          </p>
          <p class="entry">
            <span class="label">Handle name</span>
            <strong class="value">Musterpilot</strong>
          </p>
          <p class="entry">
            <span class="icon">
              <img src="https://media.example.invalid/titel.png"/>
            </span>
            <span class="value">Wing Commander</span>
          </p>
        </div>
      </div>
    </div>
    <div class="main-org right-col visibility-V">
      <span class="title">Main organization </span>
      <a href="/orgs/TEST"><img src="/media/test/logo.png" /></a>
      <a href="/orgs/TEST" class="value data15">Testgeschwader</a>
      <span class="label data10">Organization rank</span>
      <strong class="value data12">Pilot</strong>
    </div>
  </div></div>
  <div class="left-col"><div class="inner">
    <p class="entry"><span class="label">Enlisted</span>
      <strong class="value">Jan 2, 2020</strong></p>
    <p class="entry"><span class="label">Location</span>
      <strong class="value">Irgendwo, Nirgends</strong></p>
    <p class="entry"><span class="label">Fluency</span>
      <strong class="value">          German,                    English          </strong></p>
  </div></div>
</div>
<script type="text/plain" data-cookieconsent="statistics"></script>
</body></html>"#;

    /// The page of organizations, built the same way: a main one, two shown affiliations (one with lit stars),
    /// one redacted and one hidden.
    pub(crate) const ORGS: &str = r#"<div class="page-wrapper"><div class="profile-content orgs-content clearfix">
<div class="box-content org main visibility-V"><div class="inner-bg clearfix">
  <div class="title">Main organization </div>
  <div class="left-col"><div class="inner clearfix">
    <div class="thumb">
      <a href="/orgs/TEST"><img src="/media/test/heap_infobox/TEST-Logo.png" /></a>
      <span class="members">1234 members</span>
    </div>
    <div class="info">
      <p class="entry"><a href="/orgs/TEST" class="value">Test &amp; Geschwader</a></p>
      <p class="entry"><span class="label">Spectrum Identification (SID)</span>
        <strong class="value">TEST</strong></p>
      <p class="entry"><span class="label">Organization rank</span>
        <strong class="value">Pilot</strong></p>
      <div class="ranking">
        <span class="active"><span></span></span>
        <span class="active"><span></span></span>
        <span ><span></span></span>
        <span ><span></span></span>
        <span ><span></span></span>
      </div>
    </div>
  </div></div>
  <div class="right-col"><div class="inner clearfix">
    <p class="entry"><span class="label">Prim. Activity</span> <strong class="value">Security</strong></p>
    <p class="entry"><span class="label">Sec. Activity</span> <strong class="value">Trading</strong></p>
    <p class="entry"><span class="label">Commitment</span> <strong class="value">Regular</strong></p>
  </div></div>
</div></div>
<div class="box-content org affiliation  visibility-V "><div class="inner-bg clearfix">
  <div class="title">Affiliation </div>
  <div class="thumb"><a href="/orgs/ZWEI"><img src="/media/zwei/Logo.jpg" /></a>
    <span class="members">14 members</span></div>
  <div class="info">
    <p class="entry orgtitle"><a href="/orgs/ZWEI" class="value data5">Zweite Staffel</a></p>
    <p class="entry"><span class="label data9">Organization rank</span>
      <strong class="value data2">Recruit</strong></p>
    <div class="ranking data1"><span ><span></span></span><span ><span></span></span></div>
  </div>
</div></div>
<div class="box-content org affiliation  visibility-R "><div class="inner-bg clearfix">
  <div class="title">Affiliation </div>
  <div class="member-visibility-restriction">REDACTED</div>
</div></div>
<div class="box-content org affiliation  visibility-V third"><div class="inner-bg clearfix">
  <div class="title">Affiliation </div>
  <div class="thumb"><a href="/orgs/DREI"><img src="/media/drei/Logo.png" /></a>
    <span class="members">5,010 members</span></div>
  <div class="info">
    <p class="entry orgtitle"><a href="/orgs/DREI" class="value data14">Dritte</a></p>
    <p class="entry"><span class="label data6">Organization rank</span>
      <strong class="value data11">Member</strong></p>
  </div>
</div></div>
<div class="box-content org affiliation  visibility-H "><div class="inner-bg clearfix">
  <div class="title">Affiliation </div>
</div></div>
</div></div>"#;

    #[test]
    fn the_profile_is_read_by_its_labels() {
        let profile = parse_profile(PROFILE).expect("a profile");
        assert_eq!(profile.handle, "Musterpilot");
        assert_eq!(profile.name, "Muster & Co");
        assert_eq!(profile.enlisted, "Jan 2, 2020");
        assert_eq!(profile.title, "Wing Commander");
        assert_eq!(profile.title_image, "https://media.example.invalid/titel.png");
        assert_eq!(profile.image, "https://robertsspaceindustries.com/media/beispiel/heap_infobox/bild.png");
        assert_eq!(profile.record, "#100001");
        assert_eq!(profile.fluency, ["German", "English"]);
        // Counter-check: a page without the two entries has neither.
        let bare = parse_profile(&PROFILE.replace("Fluency", "Sprachen").replace("UEE Citizen Record", "X"))
            .expect("a profile");
        assert!(bare.record.is_empty() && bare.fluency.is_empty());
    }

    #[test]
    fn a_page_without_a_handle_is_no_profile() {
        assert_eq!(parse_profile("<html><body><h1>404</h1></body></html>"), None);
        // Counter-check: the same page with the label is one.
        assert!(parse_profile(&PROFILE.replace("Handle name", "Handle")).is_none());
    }

    #[test]
    fn the_organizations_are_read_in_order_with_hidden_ones() {
        let orgs = parse_orgs(ORGS);
        let main = orgs.main.as_ref().and_then(Membership::org).expect("a main organization");
        assert_eq!(main.sid, "TEST");
        assert_eq!(main.name, "Test & Geschwader");
        assert_eq!(main.rank, "Pilot");
        assert_eq!(main.stars, 2);
        assert_eq!(main.members, Some(1234));
        assert_eq!(main.logo, "https://robertsspaceindustries.com/media/test/heap_infobox/TEST-Logo.png");
        assert_eq!((main.activity.as_str(), main.second_activity.as_str()), ("Security", "Trading"));
        assert_eq!(main.commitment, "Regular");
        assert_eq!(orgs.affiliations.len(), 4);
        let second = orgs.affiliations[0].org().expect("shown");
        assert_eq!(
            (second.sid.as_str(), second.name.as_str(), second.rank.as_str()),
            ("ZWEI", "Zweite Staffel", "Recruit")
        );
        assert_eq!((second.stars, second.members), (0, Some(14)));
        assert!(second.activity.is_empty(), "only the main organization names its activity");
        assert_eq!(orgs.affiliations[1], Membership::Hidden);
        assert_eq!(orgs.affiliations[2].org().map(|o| o.members), Some(Some(5010)));
        assert_eq!(orgs.affiliations[3], Membership::Hidden);
        assert_eq!(orgs.shown().map(|o| o.sid.as_str()).collect::<Vec<_>>(), ["TEST", "ZWEI", "DREI"]);
    }

    #[test]
    fn a_box_that_is_not_shown_hides_its_organization_even_with_a_link() {
        // Counter-check of the visibility: the shown box turned redacted is hidden.
        let orgs = parse_orgs(&ORGS.replacen("visibility-V ", "visibility-R ", 1));
        assert_eq!(orgs.affiliations[0], Membership::Hidden);
        assert!(parse_orgs("<html></html>").main.is_none());
    }

    #[test]
    fn handles_only_take_the_characters_the_site_allows() {
        assert_eq!(valid_handle("  Muster_pilot-1 ").as_deref(), Some("Muster_pilot-1"));
        for bad in ["", "   ", "a/b", "a b", "../x", "x?y=1", "ä", &"x".repeat(HANDLE_MAX + 1)] {
            assert_eq!(valid_handle(bad), None, "{bad}");
        }
        assert_eq!(profile_url("Ab"), "https://robertsspaceindustries.com/citizens/Ab");
        assert_eq!(orgs_url("Ab"), "https://robertsspaceindustries.com/citizens/Ab/organizations");
    }

    #[test]
    fn only_whole_https_addresses_are_kept() {
        assert_eq!(whole_url("/media/x.png"), "https://robertsspaceindustries.com/media/x.png");
        assert_eq!(whole_url("https://a.invalid/x.png"), "https://a.invalid/x.png");
        assert_eq!(whole_url("http://a.invalid/x.png"), "");
        assert_eq!(whole_url("javascript:alert(1)"), "");
        assert_eq!(whole_url("//a.invalid/x.png"), "");
    }
}
