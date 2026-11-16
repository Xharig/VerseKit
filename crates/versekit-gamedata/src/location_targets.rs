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

//! The places a location query of a contract leads to.
//!
//! A query is a search over the tags of the location templates (`missionLocationTemplates` of the Basetool catalog).
//! The templates it finds name their place by a body code in their class name (`Stanton2c`, `Pyro5a`, a system
//! alone as in `RestStop_Pyro`); the star map gives the planet of each code.

use std::collections::{BTreeSet, HashMap};

use serde::Deserialize;

use crate::contract_facts::{Query, TagType};

/// A query that finds more templates than this stands for any place of the game and names none.
pub(crate) const BROAD_QUERY: usize = 300;

/// The system and, where the template lies at a planet or one of its moons, the planet, by English name.
pub(crate) type Place = (String, Option<String>);

/// A location template with the tags its searches look at.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Template {
    class_name: String,
    general: Vec<String>,
    produces: Vec<String>,
    consumes: Vec<String>,
}

/// The location templates of a game channel that are not disabled.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocationTargets {
    templates: Vec<Template>,
}

impl LocationTargets {
    /// Reads the templates of a Basetool catalog in JSON.
    ///
    /// # Errors
    ///
    /// When the bytes are no catalog JSON.
    pub fn from_catalog(json: &[u8]) -> Result<Self, serde_json::Error> {
        let catalog: RawCatalog = serde_json::from_slice(json)?;
        let templates = catalog
            .mission_location_templates
            .into_iter()
            .filter(|template| !template.disabled)
            .map(|template| Template {
                class_name: template.class_name,
                general: template.tags,
                produces: template.produces_tags.positive_tags,
                consumes: template.consumes_tags.positive_tags,
            })
            .collect();
        Ok(Self { templates })
    }

    /// The class names of the templates that satisfy every tag search of `query`: for each search one of its terms
    /// accepts the template. A query without a tag search finds none.
    pub(crate) fn matching(&self, query: &Query) -> Vec<&str> {
        if query.searches.is_empty() {
            return Vec::new();
        }
        self.templates
            .iter()
            .filter(|template| {
                query.searches.iter().all(|search| {
                    let tags = template.tags(search.kind);
                    search.terms.iter().any(|term| {
                        term.positive.iter().all(|wanted| tags.iter().any(|tag| is_or_below(tag, wanted)))
                            && !term.negative.iter().any(|unwanted| tags.iter().any(|tag| is_or_below(tag, unwanted)))
                    })
                })
            })
            .map(|template| template.class_name.as_str())
            .collect()
    }

    /// The places `query` leads to; none for a query broader than [`BROAD_QUERY`].
    pub(crate) fn places(&self, query: &Query, bodies: &Bodies) -> BTreeSet<Place> {
        let found = self.matching(query);
        if found.len() > BROAD_QUERY {
            return BTreeSet::new();
        }
        found.into_iter().filter_map(|class_name| bodies.place(class_name)).collect()
    }

    /// The number of templates.
    pub fn len(&self) -> usize {
        self.templates.len()
    }

    /// Whether there is no template.
    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }
}

/// The systems of the star map and the planet each body code lies at.
#[derive(Debug, Clone, Default)]
pub(crate) struct Bodies {
    systems: Vec<(String, String)>,
    planets: HashMap<String, Option<String>>,
}

impl Bodies {
    /// The bodies of `systems`, each a code in lower case (`stanton`) with its English name.
    pub(crate) fn new(systems: impl IntoIterator<Item = (String, String)>) -> Self {
        let mut systems: Vec<(String, String)> = systems.into_iter().collect();
        systems.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
        Self { systems, planets: HashMap::new() }
    }

    /// The body code a star map record name is, in lower case: the whole name is a system code with a number and an
    /// optional letter (`Stanton2c`); `None` for any other name.
    pub(crate) fn body_code(&self, record_name: &str) -> Option<String> {
        let code = self.find_code(record_name)?;
        (code.start == 0 && code.end == record_name.len() && code.number.is_some())
            .then(|| record_name.to_ascii_lowercase())
    }

    /// Notes that the body `code` (from [`Bodies::body_code`]) lies at `planet`.
    pub(crate) fn add_body(&mut self, code: String, planet: Option<String>) {
        self.planets.insert(code, planet);
    }

    /// The place a template with the class name `class_name` lies at: the system its first body code names and,
    /// with a number, the planet of that body, else of the body without its letter; none for a name without a code.
    pub(crate) fn place(&self, class_name: &str) -> Option<Place> {
        let code = self.find_code(class_name)?;
        let system = self.systems.iter().find(|(key, _)| *key == code.system)?.1.clone();
        let planet = code.number.as_ref().and_then(|number| {
            let bare = format!("{}{number}", code.system);
            let whole = code.letter.map_or_else(|| bare.clone(), |letter| format!("{bare}{letter}"));
            let planet = |key: &str| self.planets.get(key).cloned().flatten();
            planet(&whole).or_else(|| planet(&bare))
        });
        Some((system, planet))
    }

    /// The first system code in `name` that starts a word (no letter before it) and ends it: digits and one lower
    /// case letter after the digits are part of it, a lower case letter after that makes it part of a longer word.
    fn find_code(&self, name: &str) -> Option<Code> {
        let bytes = name.as_bytes();
        (0..bytes.len()).filter(|&start| start == 0 || !bytes[start - 1].is_ascii_alphabetic()).find_map(|start| {
            self.systems.iter().find_map(|(system, _)| {
                let end = start + system.len();
                if !name.get(start..end)?.eq_ignore_ascii_case(system) {
                    return None;
                }
                let digits = bytes[end..].iter().take_while(|byte| byte.is_ascii_digit()).count();
                let mut next = end + digits;
                let letter = (digits > 0).then(|| bytes.get(next).copied().filter(u8::is_ascii_lowercase)).flatten();
                if letter.is_some() {
                    next += 1;
                }
                if bytes.get(next).is_some_and(u8::is_ascii_lowercase) {
                    return None;
                }
                Some(Code {
                    system: system.clone(),
                    number: (digits > 0).then(|| name[end..end + digits].to_string()),
                    letter: letter.map(char::from),
                    start,
                    end: next,
                })
            })
        })
    }
}

/// A system code with the number and the letter of a body, where `name[start..end]` holds it.
struct Code {
    system: String,
    number: Option<String>,
    letter: Option<char>,
    start: usize,
    end: usize,
}

impl Template {
    fn tags(&self, kind: TagType) -> &[String] {
        match kind {
            TagType::General => &self.general,
            TagType::Produces => &self.produces,
            TagType::Consumes => &self.consumes,
        }
    }
}

/// Whether `tag` is `wanted` or lies below it (`Outpost.Colonial` below `Outpost`), ignoring ASCII case.
fn is_or_below(tag: &str, wanted: &str) -> bool {
    let (tag, wanted) = (tag.as_bytes(), wanted.as_bytes());
    tag.len() >= wanted.len()
        && tag[..wanted.len()].eq_ignore_ascii_case(wanted)
        && (tag.len() == wanted.len() || tag[wanted.len()] == b'.')
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCatalog {
    #[serde(default)]
    mission_location_templates: Vec<RawTemplate>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTemplate {
    class_name: String,
    #[serde(default)]
    disabled: bool,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    produces_tags: RawTags,
    #[serde(default)]
    consumes_tags: RawTags,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTags {
    #[serde(default)]
    positive_tags: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::{BROAD_QUERY, Bodies, LocationTargets};
    use crate::contract_facts::{Query, TagSearch, TagType, Term};

    /// A catalog of templates: class name, tags, and whether it is disabled.
    fn templates(list: &[(&str, &[&str], bool)]) -> LocationTargets {
        let templates: Vec<String> = list
            .iter()
            .map(|(name, tags, disabled)| {
                let tags: Vec<String> = tags.iter().map(|tag| format!("\"{tag}\"")).collect();
                format!(
                    "{{\"className\": \"{name}\", \"disabled\": {disabled}, \"tags\": [{}], \
                      \"producesTags\": {{\"positiveTags\": [\"Missions.ItemType.Waste\"], \"negativeTags\": []}}, \
                      \"consumesTags\": {{\"positiveTags\": [\"Missions.ItemType.Food\"], \"negativeTags\": []}}}}",
                    tags.join(", ")
                )
            })
            .collect();
        let json = format!("{{\"missionLocationTemplates\": [{}]}}", templates.join(", "));
        LocationTargets::from_catalog(json.as_bytes()).unwrap()
    }

    fn term(positive: &[&str], negative: &[&str]) -> Term {
        Term {
            positive: positive.iter().map(|tag| (*tag).to_string()).collect(),
            negative: negative.iter().map(|tag| (*tag).to_string()).collect(),
        }
    }

    fn search(kind: TagType, terms: Vec<Term>) -> TagSearch {
        TagSearch { kind, terms }
    }

    fn query(searches: Vec<TagSearch>) -> Query {
        Query { searches }
    }

    fn all(targets: &LocationTargets, query: &Query) -> Vec<String> {
        targets.matching(query).into_iter().map(str::to_string).collect()
    }

    fn bodies() -> Bodies {
        let mut bodies = Bodies::new([
            ("stanton".to_string(), "Stanton".to_string()),
            ("pyro".to_string(), "Pyro".to_string()),
            ("nyx".to_string(), "Nyx".to_string()),
        ]);
        for (record, planet) in [
            ("Stanton2", "Crusader"),
            ("Stanton2c", "Crusader"),
            ("Stanton4", "microTech"),
            ("Pyro5", "Pyro V"),
            ("Pyro5a", "Pyro V"),
            ("Nyx2", "Nyx II"),
        ] {
            let code = bodies.body_code(record).unwrap_or_else(|| panic!("{record} is a body code"));
            bodies.add_body(code, Some(planet.to_string()));
        }
        bodies
    }

    #[test]
    fn a_tag_below_the_wanted_tag_satisfies_it_but_not_the_other_way_round() {
        let targets = templates(&[
            ("Colonial", &["Missions.LocationType.Outpost.Colonial"], false),
            ("Plain", &["Missions.LocationType.Outpost"], false),
            ("Longer", &["Missions.LocationType.OutpostRayari"], false),
        ]);
        let outpost = query(vec![search(TagType::General, vec![term(&["Missions.LocationType.Outpost"], &[])])]);
        assert_eq!(all(&targets, &outpost), ["Colonial", "Plain"], "a longer name is no child of the tag");
        let colonial =
            query(vec![search(TagType::General, vec![term(&["Missions.LocationType.Outpost.Colonial"], &[])])]);
        assert_eq!(all(&targets, &colonial), ["Colonial"], "the parent does not satisfy the child");
        let shouted = query(vec![search(TagType::General, vec![term(&["MISSIONS.locationtype.OUTPOST"], &[])])]);
        assert_eq!(all(&targets, &shouted), ["Colonial", "Plain"], "tags compare without regard to case");
    }

    #[test]
    fn a_negative_tag_rules_a_template_out_and_every_positive_tag_is_needed() {
        let targets = templates(&[
            ("Both", &["Missions.A", "Missions.B"], false),
            ("OnlyA", &["Missions.A"], false),
            ("BothAndBad", &["Missions.A", "Missions.B", "Missions.Bad.Child"], false),
        ]);
        let wanted =
            query(vec![search(TagType::General, vec![term(&["Missions.A", "Missions.B"], &["Missions.Bad"])])]);
        assert_eq!(all(&targets, &wanted), ["Both"]);
        let no_positive = query(vec![search(TagType::General, vec![term(&[], &["Missions.Bad"])])]);
        assert_eq!(all(&targets, &no_positive), ["Both", "OnlyA"], "a term with negative tags alone is allowed");
    }

    #[test]
    fn the_terms_of_a_search_are_alternatives_and_the_searches_of_a_query_all_hold() {
        let targets = templates(&[
            ("AB", &["Missions.A", "Missions.B"], false),
            ("AC", &["Missions.A", "Missions.C"], false),
            ("OnlyC", &["Missions.C"], false),
        ]);
        let either =
            query(vec![search(TagType::General, vec![term(&["Missions.B"], &[]), term(&["Missions.C"], &[])])]);
        assert_eq!(all(&targets, &either), ["AB", "AC", "OnlyC"], "terms: or");
        let both = query(vec![
            search(TagType::General, vec![term(&["Missions.A"], &[])]),
            search(TagType::General, vec![term(&["Missions.B"], &[]), term(&["Missions.C"], &[])]),
        ]);
        assert_eq!(all(&targets, &both), ["AB", "AC"], "searches: and");
        assert!(all(&targets, &query(Vec::new())).is_empty(), "a query without a tag search names no place");
        assert!(all(&targets, &query(vec![search(TagType::General, Vec::new())])).is_empty(), "no term accepts none");
    }

    #[test]
    fn the_tag_type_picks_which_tags_of_the_template_are_searched() {
        let targets = templates(&[("Place", &["Missions.Own"], false)]);
        let of = |kind, tag| query(vec![search(kind, vec![term(&[tag], &[])])]);
        assert_eq!(all(&targets, &of(TagType::General, "Missions.Own")), ["Place"]);
        assert!(all(&targets, &of(TagType::General, "Missions.ItemType.Waste")).is_empty());
        assert_eq!(all(&targets, &of(TagType::Produces, "Missions.ItemType.Waste")), ["Place"]);
        assert!(all(&targets, &of(TagType::Produces, "Missions.ItemType.Food")).is_empty());
        assert_eq!(all(&targets, &of(TagType::Consumes, "Missions.ItemType.Food")), ["Place"]);
        assert!(all(&targets, &of(TagType::Consumes, "Missions.Own")).is_empty());
    }

    #[test]
    fn a_disabled_template_is_never_found() {
        let targets = templates(&[("On", &["Missions.A"], false), ("Off", &["Missions.A"], true)]);
        assert_eq!(targets.len(), 1);
        let any = query(vec![search(TagType::General, vec![term(&["Missions.A"], &[])])]);
        assert_eq!(all(&targets, &any), ["On"]);
    }

    #[test]
    fn a_body_code_names_the_system_and_the_planet() {
        let bodies = bodies();
        let at = |name: &str| bodies.place(name);
        let some = |system: &str, planet: &str| Some((system.to_string(), Some(planet.to_string())));
        assert_eq!(at("Outpost_Rayari_Stanton2c_Layout"), some("Stanton", "Crusader"), "Yela lies at Crusader");
        assert_eq!(at("Stanton2c"), some("Stanton", "Crusader"));
        assert_eq!(at("Stanton4_Station"), some("Stanton", "microTech"));
        assert_eq!(at("Pyro5a"), some("Pyro", "Pyro V"));
        assert_eq!(at("Nyx2"), some("Nyx", "Nyx II"));
        assert_eq!(at("Outpost_Nyx2_X"), some("Nyx", "Nyx II"), "a code inside a longer name");
        assert_eq!(
            at("Stanton2d"),
            some("Stanton", "Crusader"),
            "a moon the star map does not know lies at its planet"
        );
    }

    #[test]
    fn a_system_alone_names_the_system_only() {
        let bodies = bodies();
        let only = |system: &str| Some((system.to_string(), None));
        assert_eq!(bodies.place("RestStop_Pyro"), only("Pyro"));
        assert_eq!(bodies.place("Pyro"), only("Pyro"));
        assert_eq!(bodies.place("Stanton_Cave_01"), only("Stanton"));
        assert_eq!(bodies.place("Stanton9"), only("Stanton"), "a number no star map object has: the system");
    }

    #[test]
    fn a_word_that_only_starts_like_a_system_names_no_place() {
        let bodies = bodies();
        assert_eq!(bodies.place("Stantonia"), None);
        assert_eq!(bodies.place("Pyroclast_Cave"), None);
        assert_eq!(bodies.place("Stanton2cd"), None, "two letters after the number");
        assert_eq!(bodies.place("SubStanton2"), None, "not at the start of a word");
        assert_eq!(bodies.place("Outpost_Generic"), None);
    }

    #[test]
    fn only_a_whole_record_name_is_a_body_code() {
        let bodies = bodies();
        assert_eq!(bodies.body_code("Stanton2c").as_deref(), Some("stanton2c"));
        assert_eq!(bodies.body_code("PYRO5").as_deref(), Some("pyro5"));
        assert_eq!(bodies.body_code("StantonStar"), None, "a system without a number");
        assert_eq!(bodies.body_code("Stanton2_Station"), None);
        assert_eq!(bodies.body_code("Stantonia"), None);
    }

    #[test]
    fn a_query_that_finds_too_many_templates_names_no_place() {
        let any = query(vec![search(TagType::General, vec![term(&["Missions.Wide"], &[])])]);
        let names: Vec<String> = (0..=BROAD_QUERY).map(|n| format!("Stanton1_Place{n}")).collect();
        let list: Vec<(&str, &[&str], bool)> =
            names.iter().map(|name| (name.as_str(), &["Missions.Wide"][..], false)).collect();
        let wide = templates(&list);
        assert_eq!(wide.matching(&any).len(), BROAD_QUERY + 1);
        assert!(wide.places(&any, &bodies()).is_empty());
        let narrow = templates(&list[..BROAD_QUERY]);
        let at: Vec<_> = narrow.places(&any, &bodies()).into_iter().collect();
        assert_eq!(at, [("Stanton".to_string(), None)], "a query at the limit still counts");
    }
}
