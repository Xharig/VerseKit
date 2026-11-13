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

//! Whom the program thanks and whose work it names: the sources it bundles,
//! the sources it fetches, and the people whose feedback became something
//! that would not exist otherwise. The page "Danke & Lizenzen" draws these
//! lists; `tools/parity_thanks.py` and `versekit-parity thanks` check them
//! against the lists of the Python version, so nothing named there goes
//! missing.
//!
//! A credit is never lowered: a source stays named with its licence and
//! address as long as the program uses it.

/// The licence of a source, or "no licence stated".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Licence {
    Named(&'static str),
    None,
}

/// One source: its name, licence, what it gives (a text key) and its address, and for a bundled source with
/// a licence that asks for its notice the full licence text shipped in the program file.
#[derive(Clone, Copy, Debug)]
pub struct Credit {
    pub name: &'static str,
    pub licence: Licence,
    pub text: &'static str,
    pub url: &'static str,
    pub licence_text: Option<&'static str>,
}

impl Credit {
    /// The same source with its full licence text.
    const fn with_licence_text(self, text: &'static str) -> Credit {
        Credit { licence_text: Some(text), ..self }
    }
}

/// The licence texts of the bundled sources, in the program file.
pub const LUCIDE_LICENCE: &str = include_str!("../licenses/lucide-LICENSE.txt");
pub const FLAG_ICONS_LICENCE: &str = include_str!("../licenses/flag-icons-LICENSE.txt");
pub const DPOP_LICENCE: &str = include_str!("../licenses/dpop-reference-LICENSE.txt");
pub const FLEETMANIFEST_LICENCE: &str = crate::fleet_analysis::LICENSE;
pub const HANDLE_QUERY_LICENCE: &str = include_str!("../licenses/star-citizen-handle-query-LICENSE.txt");

/// The tool the handle search and the log monitor are modelled on.
pub const HANDLE_QUERY: &str = "https://github.com/Kuehlwagen/Star-Citizen-Handle-Query";

/// The newest release of the Hangar-Exporter in its public repository: the thanks, the hangar page and the
/// setup assistant all point here (the repository of its source is private).
pub const HANGAR_EXPORTER: &str = "https://github.com/Drachennarr/Hangar-Exporter-Releases/releases/latest";

const fn credit(name: &'static str, licence: Licence, text: &'static str, url: &'static str) -> Credit {
    Credit { name, licence, text, url, licence_text: None }
}

const MIT: Licence = Licence::Named("MIT");

/// Part of the program file, works without a network.
pub const BUNDLED: [Credit; 7] = [
    credit("Lucide", Licence::Named("ISC"), "s_dkv_symbole", "https://lucide.dev").with_licence_text(LUCIDE_LICENCE),
    credit("flag-icons", MIT, "s_dk_flaggen", "https://flagicons.lipis.dev").with_licence_text(FLAG_ICONS_LICENCE),
    credit("Basetool DPoP reference", MIT, "s_dk_dpop", "https://krt-profit.github.io/basetool/")
        .with_licence_text(DPOP_LICENCE),
    credit("Fleetmanifest (Maluh)", MIT, "s_dkv_fleetmanifest", "https://github.com/MaluhTV/SC_Fleetmanifest")
        .with_licence_text(FLEETMANIFEST_LICENCE),
    credit("Contested Zones maps (Zane_DragonBorn)", Licence::None, "s_dkv_karten_zane", MAPS_STATIONS),
    credit("PYAM maps (Kerast)", Licence::None, "s_dkv_karten_kerast", MAPS_PYAM),
    credit("Star Citizen Handle Query (Kuehlwagen)", MIT, "s_dkv_schq", HANDLE_QUERY)
        .with_licence_text(HANDLE_QUERY_LICENCE),
];

/// The post of the maps of Checkmate, Orbituary and Ruin Station: their creator is named under each map.
pub const MAPS_STATIONS: &str =
    "https://robertsspaceindustries.com/community-hub/post/accurate-pyro-contested-zone-maps-c-zs-7l8Ih9mXyPDpu";
/// The post of the maps of PYAM-EXHANG-0-1 and PYAM-SUPVISR-3-4/5.
pub const MAPS_PYAM: &str =
    "https://www.reddit.com/r/starcitizen/comments/1i3iyq9/supervisor_exectutive_hangar_maps_pyamsupvisr345/";

/// The sources that are not part of the program: fetched when needed.
pub const FETCHED: [Credit; 13] = [
    credit("Star Citizen Mission DataBase", Licence::Named("CC BY-NC-ND 4.0"), "s_dkv_scmdb", "https://scmdb.net"),
    credit("UEX Corp", Licence::None, "s_dkv_uex", "https://uexcorp.space"),
    credit(
        "scunpacked-data (Star Citizen Wiki)",
        Licence::None,
        "s_dkv_scunpacked",
        "https://github.com/StarCitizenWiki/scunpacked-data",
    ),
    credit("erkul.games", Licence::None, "s_dk_erkul", "https://erkul.games"),
    credit(
        "Star Citizen: Hangar Extension (AlyxOne)",
        MIT,
        "s_dkv_hangarext",
        "https://robertsspaceindustries.com/community-hub/post/star-citizen-hangar-extension-browser-add-on-7xzYDnJDV6c2W",
    ),
    credit("Star Citizen Hangar XPLORer (dolkensp)", MIT, "s_dkv_xplorer", "https://github.com/dolkensp/HangarXPLOR"),
    credit("Hangar-Exporter (Drachennarr)", Licence::Named("GPL-3.0"), "s_dkv_exporter", HANGAR_EXPORTER),
    credit(
        "Profit Basetool (greluc)",
        Licence::Named("GPL-3.0"),
        "s_dk_basetool",
        "https://github.com/krt-profit/basetool",
    ),
    credit("SC Launch Configurator (Luftwerft)", Licence::None, "s_dkv_luftwerft", "https://www.luftwerft.com/"),
    credit("StarStrings (MrKraken)", Licence::None, "s_dkv_ss", "https://starstrings.app"),
    credit(
        "StarCitizen-Localization (Dymerz)",
        Licence::None,
        "s_dkv_dymerz",
        "https://github.com/Dymerz/StarCitizen-Localization",
    ),
    credit("Star_citizen_ES (Thord82)", Licence::None, "s_dkv_thord82", "https://github.com/Thord82/Star_citizen_ES"),
    credit("contestedzonetimers.com (Appieman)", Licence::None, "s_dkv_contested", "https://contestedzonetimers.com/"),
];

/// A source that version 5 names and version 4.12 does not.
pub const ADDED: [&str; 5] = [
    "Contested Zones maps (Zane_DragonBorn)",
    "PYAM maps (Kerast)",
    "Hangar-Exporter (Drachennarr)",
    "contestedzonetimers.com (Appieman)",
    "Star Citizen Handle Query (Kuehlwagen)",
];
/// People that version 5 names and version 4.12 does not: their ideas or the faults they found came with 5.0.
pub const ADDED_PEOPLE: [&str; 11] = [
    "Katschaeng",
    "pinhead92",
    "DarthUrmel",
    "smokyjohn",
    "Lenoro",
    "MindBurnt",
    "Shockwaffel",
    "Kuehlwagen",
    "LunaticSmile",
    "SirFrog",
    "Shockwave",
];
/// Ideas of people that version 4.12 names too, which version 5 adds: the person and the text key.
pub const ADDED_IDEAS: [(&str, &str); 2] = [("P6", "s_dkv_p6_idee2"), ("P6", "s_dkv_p6_idee3")];

/// One person: the name as it stands in the changelog, the group, and the
/// text keys of what became of their ideas and of the faults they found.
#[derive(Clone, Copy, Debug)]
pub struct Person {
    pub name: &'static str,
    pub group: &'static str,
    pub ideas: &'static [&'static str],
    pub found: &'static [&'static str],
}

const fn person(
    name: &'static str,
    group: &'static str,
    ideas: &'static [&'static str],
    found: &'static [&'static str],
) -> Person {
    Person { name, group, ideas, found }
}

/// Everyone named on the page, in the order of the page.
pub const PEOPLE: [Person; 41] = [
    person("Haldjas", "pr0", &["s_dkv_haldjas_idee"], &["s_dkv_haldjas_bugs"]),
    person("rurudotorg", "SC4M", &["s_dkv_rurudotorg_idee"], &["s_dkv_rurudotorg_bugs", "s_dkv_rurudotorg_bugs2"]),
    person("Morkhan", "KRT", &["s_dkv_morkhan_idee"], &["s_dk_morkhan_bugs", "s_dkv_morkhan_bugs2"]),
    person("Horthy", "KRT", &["s_dkv_horthy_idee"], &[]),
    person(
        "Bushwick4712",
        "KRT",
        &["s_dkv_bushwick_idee", "s_dkv_bushwick_idee2", "s_dkv_bushwick_idee3", "s_dkv_bushwick_idee4"],
        &["s_dkv_bushwick_bugs", "s_dkv_bushwick_bugs2", "s_dkv_bushwick_bugs3", "s_dkv_bushwick_bugs4"],
    ),
    person("Choopa", "", &["s_dkv_choopa_idee"], &[]),
    person("YoshimitsuDE", "KRT", &["s_dkv_yoshimitsu_idee"], &[]),
    person("Zwaersch", "KRT", &["s_dkv_zwaersch_idee"], &["s_dkv_zwaersch_bugs", "s_dkv_zwaersch_bugs2"]),
    person(
        "Blackd0g84",
        "KRT",
        &[
            "s_dkv_blackdog_idee",
            "s_dkv_blackdog_idee2",
            "s_dkv_blackdog_idee3",
            "s_dkv_blackdog_idee4",
            "s_dkv_blackdog_idee5",
            "s_dkv_blackdog_idee6",
        ],
        &["s_dkv_blackdog_bugs"],
    ),
    person(
        "Aeternitas26",
        "KRT",
        &[
            "s_dkv_aeternitas_idee",
            "s_dkv_aeternitas_idee2",
            "s_dkv_aeternitas_idee3",
            "s_dkv_aeternitas_idee4",
            "s_dkv_aeternitas_idee5",
        ],
        &["s_dk_aeternitas_bugs", "s_dkv_aeternitas_bugs2", "s_dkv_aeternitas_bugs3", "s_dkv_aeternitas_bugs4"],
    ),
    person("KynoTnis", "ADI", &["s_dkv_kynotnis_idee"], &["s_dkv_kynotnis_bugs"]),
    person("ryze", "KRT", &["s_dkv_ryze_idee"], &[]),
    person("Drachennarr", "KRT", &["s_dkv_drachennarr_idee"], &["s_dkv_drachennarr_bugs"]),
    person("Maluh", "KRT", &["s_dkv_maluh_idee"], &[]),
    person("P6", "KRT", &["s_dkv_p6_idee", "s_dkv_p6_idee2", "s_dkv_p6_idee3"], &["s_dkv_p6_bugs", "s_dk_p6_bugs2"]),
    person("sotth", "", &["s_dkv_sotth_idee"], &[]),
    person("Rhino", "", &["s_dkv_rhino_idee"], &["s_dkv_rhino_bugs"]),
    person("F_i_r_e", "KRT", &[], &["s_dkv_fire_bugs"]),
    person(
        "greluc",
        "KRT",
        &["s_dkv_greluc_idee", "s_dkv_greluc_idee2"],
        &["s_dkv_greluc_bugs", "s_dkv_greluc_bugs2", "s_dkv_greluc_bugs3", "s_dkv_greluc_bugs4"],
    ),
    person(
        "Parsul",
        "KRT",
        &["s_dkv_parsul_idee", "s_dkv_parsul_idee2"],
        &[
            "s_dkv_parsul_bugs",
            "s_dkv_parsul_bugs2",
            "s_dkv_parsul_bugs3",
            "s_dkv_parsul_bugs4",
            "s_dkv_parsul_bugs5",
            "s_dkv_parsul_bugs6",
            "s_dkv_parsul_bugs7",
            "s_dkv_parsul_bugs8",
        ],
    ),
    person("AlyxOne", "", &["s_dkv_alyxone_idee"], &["s_dkv_alyxone_bugs"]),
    person("Katschaeng", "KRT", &["s_dkv_katschaeng_idee"], &[]),
    person("pinhead92", "KRT", &["s_dkv_pinhead_idee"], &[]),
    person("DarthUrmel", "KRT", &["s_dkv_darthurmel_idee"], &[]),
    person("smokyjohn", "KRT", &["s_dkv_smokyjohn_idee"], &[]),
    person("Lenoro", "KRT", &[], &["s_dkv_lenoro_bugs"]),
    person("MindBurnt", "R/SC", &["s_dkv_mindburnt_idee", "s_dkv_mindburnt_idee2"], &[]),
    person(
        "Shockwaffel",
        "KRT",
        &["s_dkv_shockwaffel_idee", "s_dkv_shockwaffel_idee2"],
        &["s_dkv_shockwaffel_bugs", "s_dkv_shockwaffel_bugs2"],
    ),
    person(
        "Kuehlwagen",
        "KRT",
        &["s_dkv_kuehlwagen_idee", "s_dkv_kuehlwagen_idee2", "s_dkv_kuehlwagen_idee3"],
        &["s_dkv_kuehlwagen_bugs"],
    ),
    person("George_Rotfuchs", "", &[], &["s_dkv_george_bugs"]),
    person("TomLov", "", &["s_dkv_tomlov_idee"], &[]),
    person("LunaticSmile", "KRT", &["s_dkv_lunaticsmile_idee"], &[]),
    person("SirFrog", "KRT", &[], &["s_dkv_sirfrog_bugs"]),
    person("Shockwave", "TSA", &[], &["s_dkv_shockwave_bugs"]),
    person("fok", "", &[], &["s_dkv_fok_bugs", "s_dkv_fok_bugs2"]),
    person("KillingJoke271", "KRT", &[], &["s_dkv_killingjoke_bugs"]),
    person("Keendary", "KRT", &["s_dkv_keendary_idee"], &["s_dkv_keendary_bugs"]),
    person("LOGGAN89", "", &[], &["s_dkv_loggan_bugs"]),
    person("oGundalf", "", &[], &["s_dkv_ogundalf_bugs", "s_dkv_ogundalf_bugs2"]),
    person("Autobrake", "", &[], &["s_dkv_autobrake_bugs"]),
    person("Karasimsek", "", &[], &["s_dkv_karasimsek_bugs"]),
];

/// The notice for the trademark and fan content, as the fan kit asks: its
/// wording is not changed.
pub const FAN_CONTENT_HEAD: &str = "s_dk_fankit_kopf";
pub const FAN_CONTENT: &str = "s_dk_fankit";

/// The text without the marks of the language file (`**bold**`, backticks):
/// a label is bold or not as a whole.
pub fn plain(text: &str) -> String {
    text.replace("**", "").replace('`', "")
}

/// Every text key the page uses for sources, people and notices.
pub fn text_keys() -> Vec<&'static str> {
    let mut keys: Vec<&'static str> = BUNDLED.iter().chain(FETCHED.iter()).map(|c| c.text).collect();
    for person in &PEOPLE {
        keys.extend(person.ideas.iter().chain(person.found.iter()));
    }
    keys.extend([
        FAN_CONTENT_HEAD,
        FAN_CONTENT,
        "s_dkv_marken",
        "s_dkv_lead",
        "s_dkv_selbst_h",
        "s_dkv_extern_h",
        "s_dkv_leute_h",
        LICENCE_TEXT_HEAD,
        LICENCE_TEXT_NAME,
    ]);
    keys
}

/// The head of the folded licence text of a box, and its name for screen readers (`{0}` the source).
pub const LICENCE_TEXT_HEAD: &str = "s_dkv_lizenztext";
pub const LICENCE_TEXT_NAME: &str = "s_dkv_lizenztext_von";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::{Lang, text_in};

    #[test]
    fn every_text_exists_in_both_languages_and_has_no_long_dash() {
        for key in text_keys() {
            for lang in [Lang::De, Lang::En] {
                let text = text_in(lang, key).unwrap_or_else(|| panic!("{key} fehlt"));
                assert!(!text.is_empty(), "{key} leer");
                // The notice of the fan kit keeps its wording.
                if key != FAN_CONTENT {
                    assert!(!text.contains('—'), "{key} hat einen Gedankenstrich");
                }
            }
        }
    }

    /// The info bar and the tip about the version number stand as ideas of Shockwaffel, in both languages.
    #[test]
    fn shockwaffel_stands_with_the_info_bar_as_his_idea() {
        let person = PEOPLE.iter().find(|p| p.name == "Shockwaffel").expect("Shockwaffel on the page");
        assert_eq!(person.group, "KRT");
        assert!(person.ideas.contains(&"s_dkv_shockwaffel_idee"), "{:?}", person.ideas);
        for (lang, bar) in [(Lang::De, "Info-Leiste"), (Lang::En, "info bar")] {
            let text = text_in(lang, "s_dkv_shockwaffel_idee").unwrap_or_default();
            assert!(text.to_lowercase().contains(&bar.to_lowercase()), "{lang:?}: {text}");
        }
    }

    #[test]
    fn names_that_are_not_credited_stand_nowhere_on_the_page() {
        const NOT_CREDITED: [&str; 3] = ["Deutsch Launcher", "SCDL", "rjcncpt"];
        for key in text_keys() {
            for lang in [Lang::De, Lang::En] {
                let text = text_in(lang, key).unwrap_or_default();
                assert!(!NOT_CREDITED.iter().any(|name| text.contains(name)), "{key} ({lang:?})");
            }
        }
        for credit in BUNDLED.iter().chain(FETCHED.iter()) {
            let named = [credit.name, credit.url];
            assert!(!NOT_CREDITED.iter().any(|name| named.iter().any(|n| n.contains(name))), "{}", credit.name);
        }
    }

    #[test]
    fn the_makers_of_the_maps_and_the_source_of_the_start_time_are_named_with_their_links() {
        let named =
            |name: &str, url: &str| FETCHED.iter().chain(BUNDLED.iter()).any(|c| c.name.contains(name) && c.url == url);
        assert!(named("Zane_DragonBorn", MAPS_STATIONS));
        assert!(named("Kerast", MAPS_PYAM));
        assert!(named("Appieman", "https://contestedzonetimers.com/"));
        for name in ADDED {
            assert!(BUNDLED.iter().chain(FETCHED.iter()).any(|c| c.name == name), "{name}");
        }
    }

    #[test]
    fn the_exporter_and_alyxone_stay_named() {
        assert!(FETCHED.iter().any(|c| c.name == "Hangar-Exporter (Drachennarr)" && c.url == HANGAR_EXPORTER));
        // The public repository of the releases, not the private one of the source: that one is a 404 for players.
        assert!(HANGAR_EXPORTER.starts_with("https://github.com/Drachennarr/Hangar-Exporter-Releases/releases/"));
        assert!(HANGAR_EXPORTER.ends_with("/releases/latest"));
        assert!(FETCHED.iter().any(|c| c.name == "Star Citizen: Hangar Extension (AlyxOne)"));
        assert!(PEOPLE.iter().any(|p| p.name == "AlyxOne" && !p.ideas.is_empty() && !p.found.is_empty()));
        assert!(PEOPLE.iter().any(|p| p.name == "Drachennarr"));
    }

    #[test]
    fn the_people_new_in_5_are_named_with_their_ideas() {
        // The shopping basket of 5.0 was their idea: an empty list would prove nothing.
        for name in
            ["Katschaeng", "pinhead92", "DarthUrmel", "smokyjohn", "Lenoro", "MindBurnt", "Shockwaffel", "Kuehlwagen"]
        {
            assert!(ADDED_PEOPLE.contains(&name), "{name} {ADDED_PEOPLE:?}");
        }
        for name in ADDED_PEOPLE {
            let person = PEOPLE.iter().find(|p| p.name == name);
            // Everyone has a thank you: an idea or a fault found (Lenoro found one, no idea).
            assert!(person.is_some_and(|p| !p.ideas.is_empty() || !p.found.is_empty()), "{name}");
        }
        assert!(PEOPLE.iter().any(|p| p.name == "Katschaeng" && !p.ideas.is_empty()));
    }

    #[test]
    fn the_ideas_new_in_5_stand_with_their_person() {
        assert!(ADDED_IDEAS.contains(&("P6", "s_dkv_p6_idee2")), "{ADDED_IDEAS:?}");
        for (name, key) in ADDED_IDEAS {
            assert!(PEOPLE.iter().any(|p| p.name == name && p.ideas.contains(&key)), "{name} {key}");
            assert!(text_keys().contains(&key), "{key}");
        }
    }

    #[test]
    fn every_bundled_source_with_a_licence_ships_its_licence_text() {
        for credit in BUNDLED {
            let Licence::Named(name) = credit.licence else {
                assert!(credit.licence_text.is_none(), "{}: a text without a licence", credit.name);
                continue;
            };
            let text = credit.licence_text.unwrap_or_else(|| panic!("{}: no licence text", credit.name));
            let head = text.lines().next().unwrap_or_default();
            assert!(head.contains(name), "{}: the text is no {name} licence: {head}", credit.name);
            assert!(text.contains("Copyright (c)"), "{}: no copyright line", credit.name);
            assert!(text.contains("permission notice"), "{}: no permission notice", credit.name);
        }
        let holder = |name: &str| BUNDLED.iter().find(|c| c.name == name).and_then(|c| c.licence_text);
        assert!(holder("Lucide").is_some_and(|t| t.contains("Lucide Icons and Contributors")));
        assert!(holder("flag-icons").is_some_and(|t| t.contains("Panayiotis Lipiridis")));
        assert!(holder("Basetool DPoP reference").is_some_and(|t| t.contains("Lucas Greuloch")));
        assert!(holder("Fleetmanifest (Maluh)").is_some_and(|t| t.contains("MaluhTV")));
        assert!(holder("Star Citizen Handle Query (Kuehlwagen)").is_some_and(|t| t.contains("Kuehlwagen@GitHub")));
        // The page names him by his handle; the name of the licence stands only in the licence text itself.
        for lang in [Lang::De, Lang::En] {
            let shown = text_in(lang, "s_dkv_schq").unwrap_or_default();
            assert!(shown.contains("Kuehlwagen (KRT)"), "{lang:?}: {shown}");
            for key in text_keys() {
                assert!(!text_in(lang, key).unwrap_or_default().contains("Rädel"), "{lang:?} {key}: the full name");
            }
        }
        assert!(FETCHED.iter().all(|c| c.licence_text.is_none()), "a fetched source is not shipped");
    }

    #[test]
    fn nobody_is_named_twice() {
        let mut names: Vec<&str> = PEOPLE.iter().map(|p| p.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), PEOPLE.len());
    }
}
