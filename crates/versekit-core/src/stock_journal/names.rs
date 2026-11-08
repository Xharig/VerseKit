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

//! Names for what the stock journal holds: the item of a purchase or a move
//! (it has a class and a GUID) and the place of an event (it has a number).
//!
//! ⚠ An item is named by the first source that knows it: `item_Name<class>`
//! of the original `global.ini`, the GUID in the catalog of the shops, the
//! GUID or the class in the game values. Without a name the class stays as it
//! is and the item cannot be taken over into the stock.
//!
//! ⚠ A place is named by `orte-gelernt.json` (number → key) and
//! `orte-namen.json` (key → name). The list of the stock (`orte.json` with the
//! places the player added, `orte-eigene.json`) often writes a station longer
//! than the game: `Checkmate` is `Checkmate Station`, `Pyro Gateway` is
//! `Pyro Gateway (Stanton)` or `Pyro Gateway (Nyx)`. A name is first looked up
//! as it is, then by its letters and digits alone (`Area18` is `Area 18`),
//! then as the start of exactly one entry of the list; of several
//! entries the one whose system in brackets comes first in the key of the place
//! wins. A name the list still does not know is `Shown`: taking it over adds it
//! to the list. Without any list every name counts as listed, as in the forms.

use std::collections::HashMap;

use crate::asop;
use crate::jsonio::{self, Value};
use crate::location;
use crate::paths::Paths;
use crate::shops::Catalog;
use crate::stock_data;

/// The game values read from the `Data.p4k` (GUID → class and name).
pub const VALUES_FILE: &str = "werte-spieldaten.json";
/// The key of the original `global.ini` in front of the class of an item.
const NAME_KEY: &str = "item_Name";

/// What the journal can tell about an item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemName {
    Named(String),
    /// No source knows it: the class of the log.
    Unnamed(String),
}

impl ItemName {
    /// The name, when there is one.
    pub fn name(&self) -> Option<&str> {
        match self {
            ItemName::Named(name) => Some(name),
            ItemName::Unnamed(_) => None,
        }
    }

    /// Only a named item is offered for the stock.
    pub fn adoptable(&self) -> bool {
        matches!(self, ItemName::Named(_))
    }
}

/// What the journal can tell about a place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaceName {
    /// The name as the list of the stock writes it.
    Listed(String),
    /// A name the list of the stock does not know: taking it over adds it to the list.
    Shown(String),
    /// Nobody has named the number (or the purchase has none).
    Unknown(String),
}

impl PlaceName {
    /// The name, when there is one.
    pub fn name(&self) -> Option<&str> {
        match self {
            PlaceName::Listed(name) | PlaceName::Shown(name) => Some(name),
            PlaceName::Unknown(_) => None,
        }
    }

    /// Does the list of the stock have this place?
    pub fn is_listed(&self) -> bool {
        matches!(self, PlaceName::Listed(_))
    }
}

/// A name from a game file is no name when it is empty or a placeholder.
fn usable(name: &str) -> bool {
    let name = name.trim();
    !(name.is_empty()
        || name.starts_with('@')
        || name.to_uppercase().contains("PLACEHOLDER")
        || name.contains("UNINITIALIZED"))
}

/// The sources of the names of items.
#[derive(Clone, Debug, Default)]
pub struct Items {
    /// Class in lower case → name, from `item_Name<class>`.
    by_class: HashMap<String, String>,
    /// GUID in lower case → name, from the catalog of the shops.
    by_shop_guid: HashMap<String, String>,
    /// GUID in lower case → name, from the game values.
    by_value_guid: HashMap<String, String>,
    /// Class in lower case → name, from the game values.
    by_value_class: HashMap<String, String>,
}

impl Items {
    /// From the parts: the original names (`key`, `text`), the shop parts (`GUID`, `name`) and the game values
    /// (`GUID`, `class`, `name`).
    pub fn from_parts(
        original: impl IntoIterator<Item = (String, String)>,
        shop_parts: impl IntoIterator<Item = (String, String)>,
        values: impl IntoIterator<Item = (String, String, String)>,
    ) -> Items {
        let mut items = Items::default();
        for (key, text) in original {
            if key.get(..NAME_KEY.len()).is_some_and(|front| front.eq_ignore_ascii_case(NAME_KEY)) && usable(&text) {
                items.by_class.entry(key[NAME_KEY.len()..].to_lowercase()).or_insert_with(|| text.trim().to_string());
            }
        }
        for (guid, name) in shop_parts {
            if usable(&name) {
                items.by_shop_guid.entry(guid.to_lowercase()).or_insert(name);
            }
        }
        for (guid, class, name) in values {
            if usable(&name) {
                items.by_value_guid.entry(guid.to_lowercase()).or_insert_with(|| name.clone());
                if !class.is_empty() {
                    items.by_value_class.entry(class.to_lowercase()).or_insert(name);
                }
            }
        }
        items
    }

    /// From the stored files; a missing or unreadable one gives no names.
    pub fn load(paths: &Paths) -> Items {
        let original = match jsonio::read_json(&paths.app_file(asop::NAMES_FILE)) {
            Ok(Value::Object(map)) => {
                map.into_iter().filter_map(|(key, value)| value.as_str().map(|text| (key, text.to_string()))).collect()
            }
            _ => Vec::new(),
        };
        let shop_parts = Catalog::load(paths).parts.into_iter().map(|part| (part.code, part.name)).collect::<Vec<_>>();
        let values = match jsonio::read_json(&paths.app_file(VALUES_FILE)).ok().as_ref().and_then(|d| d.get("values")) {
            Some(values) => match values.get("items") {
                Some(Value::Object(items)) => items
                    .iter()
                    .map(|(guid, item)| {
                        let text = |key: &str| item.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
                        (guid.clone(), text("class_name"), text("name"))
                    })
                    .collect(),
                _ => Vec::new(),
            },
            None => Vec::new(),
        };
        Items::from_parts(original, shop_parts, values)
    }

    /// Every name the sources give an item, the first source first; a name that two sources share stands once.
    pub fn candidates(&self, class: &str, guid: &str) -> Vec<&str> {
        let (class_key, guid_key) = (class.to_lowercase(), guid.to_lowercase());
        let mut found: Vec<&str> = Vec::new();
        for name in [
            self.by_class.get(&class_key),
            self.by_shop_guid.get(&guid_key),
            self.by_value_guid.get(&guid_key),
            self.by_value_class.get(&class_key),
        ]
        .into_iter()
        .flatten()
        {
            if !found.contains(&name.as_str()) {
                found.push(name);
            }
        }
        found
    }

    /// The name of an item by its class and its GUID.
    pub fn item(&self, class: &str, guid: &str) -> ItemName {
        match self.candidates(class, guid).first() {
            Some(name) => ItemName::Named((*name).to_string()),
            None => ItemName::Unnamed(class.to_string()),
        }
    }
}

/// The class of an item by its name: name in lower case → class, only where all sources together give that name
/// exactly one class (classes are compared without case). `values` are class and name of the game values, `seen`
/// class and GUID of what the journal met, named by `items`; the first spelling of a class counts.
pub fn classes_from(
    values: impl IntoIterator<Item = (String, String)>,
    seen: impl IntoIterator<Item = (String, String)>,
    items: &Items,
) -> HashMap<String, String> {
    let mut found: HashMap<String, Vec<String>> = HashMap::new();
    let mut add = |name: &str, class: &str| {
        let (name, class) = (name.trim().to_lowercase(), class.trim());
        if name.is_empty() || class.is_empty() {
            return;
        }
        let classes = found.entry(name).or_default();
        if !classes.iter().any(|known| known.eq_ignore_ascii_case(class)) {
            classes.push(class.to_string());
        }
    };
    for (class, name) in values {
        if usable(&name) {
            add(&name, &class);
        }
    }
    for (class, guid) in seen {
        for name in items.candidates(&class, &guid) {
            add(name, &class);
        }
    }
    found.into_iter().filter_map(|(name, mut classes)| (classes.len() == 1).then(|| (name, classes.remove(0)))).collect()
}

type ClassStamp = Vec<(String, Option<(Option<std::time::SystemTime>, u64)>)>;

/// The last table of [`classes_by_name`] with the state of the files it came from.
static CLASSES: std::sync::Mutex<Option<(ClassStamp, HashMap<String, String>)>> = std::sync::Mutex::new(None);

/// [`classes_from`] out of the stored files: the game values, the journal and the names of the journal. Read again
/// only when one of the files changed.
pub fn classes_by_name(paths: &Paths) -> HashMap<String, String> {
    let files = [VALUES_FILE, crate::stock_journal::FILE, asop::NAMES_FILE, crate::shops::CATALOG_CACHE];
    let stamp: ClassStamp = files
        .iter()
        .map(|name| {
            let path = paths.app_file(name);
            let meta = std::fs::metadata(&path).ok().map(|m| (m.modified().ok(), m.len()));
            (path, meta)
        })
        .collect();
    let mut memo = CLASSES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((seen, table)) = memo.as_ref()
        && *seen == stamp
    {
        return table.clone();
    }
    let values: Vec<(String, String)> =
        match jsonio::read_json(&paths.app_file(VALUES_FILE)).ok().as_ref().and_then(|d| d.get("values")) {
            Some(values) => match values.get("items") {
                Some(Value::Object(items)) => items
                    .iter()
                    .map(|(_, item)| {
                        let text = |key: &str| item.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
                        (text("class_name"), text("name"))
                    })
                    .collect(),
                _ => Vec::new(),
            },
            None => Vec::new(),
        };
    let journal = crate::stock_journal::load(paths);
    let mut seen: Vec<(String, String)> = Vec::new();
    for session in crate::stock_journal::sessions(&journal) {
        seen.extend(session.purchases.iter().chain(&session.sales).map(|deal| (deal.class.clone(), deal.guid.clone())));
    }
    let table = classes_from(values, seen, &Items::load(paths));
    *memo = Some((stamp, table.clone()));
    table
}

/// The sources of the names of places.
#[derive(Clone, Debug, Default)]
pub struct Places {
    /// Number → key, learned from the logs.
    learned: HashMap<String, String>,
    /// Key in lower case → readable name, from the game data.
    readable: HashMap<String, String>,
    /// The list of the stock, by lower case name.
    listed: HashMap<String, String>,
    /// The list of the stock by its letters and digits alone, in lower case (`Area 18` for `Area18`).
    squeezed: HashMap<String, String>,
    /// The list of the stock in its order (for a name that is the start of an entry).
    entries: Vec<String>,
    /// The systems the list names in brackets (`Pyro Gateway (Stanton)`), each once, as written.
    systems: Vec<String>,
}

/// A name by its letters and digits alone, in lower case.
fn squeeze(name: &str) -> String {
    name.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// The system in the brackets at the end of a name of the list, or `None`.
fn system_in(entry: &str) -> Option<&str> {
    let entry = entry.trim();
    let inner = entry.strip_suffix(')')?;
    let open = inner.rfind('(')?;
    let system = inner[open + 1..].trim();
    (!system.is_empty()).then_some(system)
}

impl Places {
    pub fn from_parts(
        learned: HashMap<String, String>,
        readable: HashMap<String, String>,
        list: impl IntoIterator<Item = String>,
    ) -> Places {
        let entries: Vec<String> = list.into_iter().collect();
        let listed = entries.iter().map(|name| (name.trim().to_lowercase(), name.clone())).collect();
        let mut squeezed = HashMap::new();
        for name in &entries {
            squeezed.entry(squeeze(name)).or_insert_with(|| name.clone());
        }
        let mut systems: Vec<String> = Vec::new();
        for system in entries.iter().filter_map(|e| system_in(e)) {
            if !systems.iter().any(|s| s.eq_ignore_ascii_case(system)) {
                systems.push(system.to_string());
            }
        }
        Places { learned, readable, listed, squeezed, entries, systems }
    }

    /// From the stored files; a missing or unreadable one gives no names. The places the player added belong to
    /// the list when there is one.
    pub fn load(paths: &Paths) -> Places {
        let mut list: Vec<String> = match jsonio::read_json(&paths.app_file(stock_data::PLACES_FILE)) {
            Ok(data) if data.get("format") == Some(&Value::int(1)) => match data.get("orte") {
                Some(Value::Array(names)) => names.iter().filter_map(|n| n.as_str().map(str::to_string)).collect(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        };
        if !list.is_empty() {
            for own in crate::places::own(paths) {
                if !list.iter().any(|n| n.trim().eq_ignore_ascii_case(&own.name)) {
                    list.push(own.name);
                }
            }
        }
        Places::from_parts(location::learned(paths), location::names(paths), list)
    }

    /// The entry of the list a name of the game stands for: the one it starts (followed by a space), and of
    /// several the one whose system in brackets comes first in the key of the place.
    fn entry_for(&self, name: &str, key: &str) -> Option<String> {
        let wanted = name.trim().to_lowercase();
        if wanted.is_empty() {
            return None;
        }
        let starting: Vec<&String> = self
            .entries
            .iter()
            .filter(|e| e.trim().to_lowercase().strip_prefix(&wanted).is_some_and(|rest| rest.starts_with(' ')))
            .collect();
        match starting.as_slice() {
            [] => None,
            [one] => Some((*one).clone()),
            many => {
                let key = key.to_lowercase();
                let at = |e: &String| system_in(e).and_then(|s| key.find(&s.to_lowercase()));
                let first = many.iter().filter_map(|e| at(e)).min()?;
                let winners: Vec<&&String> = many.iter().filter(|e| at(e) == Some(first)).collect();
                match winners.as_slice() {
                    [one] => Some((**one).clone()),
                    _ => None,
                }
            }
        }
    }

    /// The system of a place by its number: the system of the list whose name comes first in its key.
    pub fn system(&self, number: &str) -> Option<String> {
        let key = self.learned.get(number)?.to_lowercase();
        self.systems
            .iter()
            .filter_map(|s| key.find(&s.to_lowercase()).map(|at| (at, s)))
            .min_by_key(|(at, _)| *at)
            .map(|(_, s)| s.clone())
    }

    /// The same places with other learned numbers (for a check against logs not yet learned from).
    pub fn with_learned(mut self, learned: HashMap<String, String>) -> Places {
        self.learned = learned;
        self
    }

    /// The name of a place by its number.
    pub fn place(&self, number: &str) -> PlaceName {
        let Some(key) = self.learned.get(number) else { return PlaceName::Unknown(number.to_string()) };
        let name = location::display_name(key, &self.readable);
        if self.listed.is_empty() {
            return PlaceName::Listed(name);
        }
        let found = self
            .listed
            .get(&name.trim().to_lowercase())
            .or_else(|| self.squeezed.get(&squeeze(&name)).filter(|_| !squeeze(&name).is_empty()))
            .cloned()
            .or_else(|| self.entry_for(&name, key));
        match found {
            Some(listed) => PlaceName::Listed(listed),
            None => PlaceName::Shown(name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter().map(|(a, b)| ((*a).to_string(), (*b).to_string())).collect()
    }

    fn items() -> Items {
        Items::from_parts(
            pairs(&[
                ("item_Namebehr_lmg_ballistic_01_mag", "FS-9 Magazine (75 cap)"),
                ("item_NameDrink_bottle_cruz_01_lux_a", "Cruz Lux Water"),
                ("item_Namearma_barrel_flhd_s2", "Flash Hider S2"),
                ("item_Namecrlf_medgun_01", "ParaMed Medical Device"),
                ("item_Namenvtc_optics_rdot_x1_s1", "Reflex Sight"),
                ("item_Nameempty_one", ""),
                ("item_Nameplaceholder_one", "@item_Nameplaceholder_one"),
                ("ship_NameAEGS_Gladius", "Gladius"),
            ]),
            pairs(&[("72B91153-5A3E-4D71-AF5C-F6C57EA2891A", "Shop Name Of The Part")]),
            vec![
                (
                    "0e259726-3ea6-4b0d-98da-afad3eeccff6".to_string(),
                    "kap_armor_arms".to_string(),
                    "Armor Arms".to_string(),
                ),
                ("3efb1be1-8935-4124-8bf0-aec410297785".to_string(), String::new(), "Only By Guid".to_string()),
            ],
        )
    }

    #[test]
    fn five_classes_get_their_name_from_the_original_texts() {
        let items = items();
        for (class, name) in [
            ("behr_lmg_ballistic_01_mag", "FS-9 Magazine (75 cap)"),
            ("Drink_bottle_cruz_01_lux_a", "Cruz Lux Water"),
            ("arma_barrel_flhd_s2", "Flash Hider S2"),
            ("crlf_medgun_01", "ParaMed Medical Device"),
            ("NVTC_OPTICS_RDOT_X1_S1", "Reflex Sight"),
        ] {
            let found = items.item(class, "");
            assert_eq!(found, ItemName::Named(name.into()), "{class}");
            assert!(found.adoptable());
            assert_eq!(found.name(), Some(name));
        }
    }

    #[test]
    fn the_sources_are_asked_in_order() {
        let items = items();
        // The class beats the GUID.
        assert_eq!(
            items.item("behr_lmg_ballistic_01_mag", "72b91153-5a3e-4d71-af5c-f6c57ea2891a"),
            ItemName::Named("FS-9 Magazine (75 cap)".into())
        );
        // The GUID in the shop catalog (in any case) comes before the values.
        assert_eq!(
            items.item("unknown_class", "72b91153-5a3e-4d71-af5c-f6c57ea2891a"),
            ItemName::Named("Shop Name Of The Part".into())
        );
        // Then the GUID in the game values, and the class there.
        assert_eq!(items.item("x", "3EFB1BE1-8935-4124-8BF0-AEC410297785"), ItemName::Named("Only By Guid".into()));
        assert_eq!(items.item("KAP_ARMOR_ARMS", "no-guid"), ItemName::Named("Armor Arms".into()));
    }

    #[test]
    fn every_source_gives_its_name_once_in_the_order_of_the_sources() {
        let items = items();
        assert_eq!(
            items.candidates("behr_lmg_ballistic_01_mag", "72B91153-5A3E-4D71-AF5C-F6C57EA2891A"),
            ["FS-9 Magazine (75 cap)", "Shop Name Of The Part"]
        );
        // The same name from two sources stands once.
        assert_eq!(items.candidates("kap_armor_arms", "0e259726-3ea6-4b0d-98da-afad3eeccff6"), ["Armor Arms"]);
        assert!(items.candidates("AEGS_Gladius", "00000000-0000-0000-0000-000000000000").is_empty());
    }

    #[test]
    fn a_class_nobody_names_stays_unnamed_and_cannot_be_taken_over() {
        let items = items();
        for class in [
            "Harvestable_Trophy_1H_KopionHorn",
            "Fuse_subItem_standard",
            "empty_one",
            "placeholder_one",
            "AEGS_Gladius",
        ] {
            let found = items.item(class, "00000000-0000-0000-0000-000000000000");
            assert_eq!(found, ItemName::Unnamed(class.into()), "{class}");
            assert!(!found.adoptable());
            assert_eq!(found.name(), None);
        }
    }

    fn places() -> Places {
        Places::from_parts(
            [
                ("3170699229", "Stanton4_NewBabbage"),
                ("815", "Stanton1_Lorville"),
                ("900", "Pyro_Ruptura_OLP"),
                ("901", "Unreadable_Key"),
            ]
            .iter()
            .map(|(n, k)| ((*n).to_string(), (*k).to_string()))
            .collect(),
            [
                ("stanton4_newbabbage", "New Babbage"),
                ("stanton1_lorville", "Lorville"),
                ("pyro_ruptura_olp", "Ruptura OLP"),
            ]
            .iter()
            .map(|(k, n)| ((*k).to_string(), (*n).to_string()))
            .collect(),
            ["new babbage".to_string(), "Lorville".to_string(), "Port Olisar".to_string()],
        )
    }

    #[test]
    fn a_place_of_the_list_is_named_as_the_list_writes_it() {
        let found = places().place("3170699229");
        assert_eq!(found, PlaceName::Listed("new babbage".into()));
        assert!(found.is_listed());
        assert_eq!(places().place("815"), PlaceName::Listed("Lorville".into()));
    }

    #[test]
    fn a_place_outside_the_list_is_shown_with_its_name() {
        let found = places().place("900");
        assert_eq!(found, PlaceName::Shown("Ruptura OLP".into()));
        assert!(!found.is_listed());
        assert_eq!(found.name(), Some("Ruptura OLP"));
        // Learned, but no readable name for the key: the key with spaces.
        assert_eq!(places().place("901"), PlaceName::Shown("Unreadable Key".into()));
    }

    #[test]
    fn a_number_nobody_named_stays_a_number() {
        let found = places().place("424242");
        assert_eq!(found, PlaceName::Unknown("424242".into()));
        assert!(!found.is_listed());
        assert_eq!(found.name(), None);
    }

    /// The names of the game and the terminal list as they stand in a real copy.
    fn stations() -> Places {
        let map = |pairs: &[(&str, &str)]| pairs.iter().map(|(a, b)| ((*a).to_string(), (*b).to_string())).collect();
        Places::from_parts(
            map(&[
                ("1", "RR_P2_L4"),
                ("2", "RR_JP_StantonPyro"),
                ("3", "RR_JP_NyxPyro"),
                ("4", "RR_P6_LEO"),
                ("5", "Pyro5a_Outpost_col_m_trdpst_otlw_001"),
                ("6", "RR_P3_L3"),
                ("7", "Nyx_Levski"),
                ("8", "Stanton3_Area18"),
                ("9", "RR_JP_StantonPyro_Hub"),
            ]),
            map(&[
                ("rr_p2_l4", "Checkmate"),
                ("rr_jp_stantonpyro", "Pyro Gateway"),
                ("rr_jp_nyxpyro", "Pyro Gateway"),
                ("rr_p6_leo", "Ruin Station"),
                ("pyro5a_outpost_col_m_trdpst_otlw_001", "Rat's Nest Outpost"),
                ("rr_p3_l3", "Patch"),
                ("nyx_levski", "Levski"),
                ("stanton3_area18", "Area18"),
                ("rr_jp_stantonpyro_hub", "Jump Hub"),
            ]),
            [
                "Area 18",
                "Checkmate Station",
                "Jump Hub (Pyro)",
                "Jump Hub (Stanton)",
                "Levski",
                "Patch City",
                "Patch City Annex",
                "Pyro Gateway (Nyx)",
                "Pyro Gateway (Stanton)",
                "Ruin Station",
                "Stanton Gateway (Pyro)",
            ]
            .map(String::from),
        )
    }

    #[test]
    fn a_station_the_list_writes_longer_is_found_in_the_list() {
        let places = stations();
        assert_eq!(places.place("1"), PlaceName::Listed("Checkmate Station".into()));
        assert_eq!(places.place("4"), PlaceName::Listed("Ruin Station".into()));
        assert_eq!(places.place("7"), PlaceName::Listed("Levski".into()));
        assert_eq!(places.place("8"), PlaceName::Listed("Area 18".into()), "a space more or less is the same name");
    }

    #[test]
    fn of_two_gateways_the_system_in_the_key_decides() {
        let places = stations();
        assert_eq!(places.place("2"), PlaceName::Listed("Pyro Gateway (Stanton)".into()));
        assert_eq!(places.place("3"), PlaceName::Listed("Pyro Gateway (Nyx)".into()));
        // Both systems stand in the key: the first one is the system of the place.
        assert_eq!(places.place("9"), PlaceName::Listed("Jump Hub (Stanton)".into()));
    }

    #[test]
    fn a_name_that_starts_two_entries_without_a_system_stays_shown() {
        // "Patch" starts "Patch City" and "Patch City Annex" and no system tells them apart.
        assert_eq!(stations().place("6"), PlaceName::Shown("Patch".into()));
        // A part of a word is no start: "Rat's Nest Outpost" is not in the list at all.
        assert_eq!(stations().place("5"), PlaceName::Shown("Rat's Nest Outpost".into()));
    }

    #[test]
    fn the_system_of_a_place_comes_from_its_key() {
        let places = stations();
        assert_eq!(places.system("2").as_deref(), Some("Stanton"), "Stanton comes first in RR_JP_StantonPyro");
        assert_eq!(places.system("3").as_deref(), Some("Nyx"));
        assert_eq!(places.system("5").as_deref(), Some("Pyro"));
        assert_eq!(places.system("7").as_deref(), Some("Nyx"));
        assert_eq!(places.system("1"), None, "RR_P2_L4 names no system");
        assert_eq!(places.system("999"), None);
    }

    #[test]
    fn without_a_list_every_named_place_counts_as_listed() {
        let places = places()
            .with_learned([("900".to_string(), "Pyro_Ruptura_OLP".to_string())].into_iter().collect::<HashMap<_, _>>());
        let empty = Places::from_parts(places.learned.clone(), places.readable.clone(), Vec::new());
        assert_eq!(empty.place("900"), PlaceName::Listed("Ruptura OLP".into()));
        assert_eq!(empty.place("1"), PlaceName::Unknown("1".into()));
    }

    #[test]
    fn an_item_name_gives_its_class_only_where_the_sources_name_exactly_one() {
        let items = Items::from_parts(
            [("item_NameMining_Gadget_SHIN_Okunis".to_string(), "Okunis".to_string())],
            [("g-two".to_string(), "Twin".to_string())],
            Vec::new(),
        );
        let values = [
            ("SHLD_GODI_S02_CoverAll_SCItem", "CoverAll"),
            ("BEHR_BallisticGatling_S6", "AD6B Ballistic Gatling"),
            ("TWIN_A", "Twin"),
            ("PLACEHOLDER_X", "@item_placeholder"),
        ]
        .map(|(c, n)| (c.to_string(), n.to_string()));
        let seen = [("Mining_Gadget_SHIN_Okunis", ""), ("TWIN_B", "g-two"), ("shld_godi_s02_coverall_scitem", "")]
            .map(|(c, g)| (c.to_string(), g.to_string()));
        let classes = classes_from(values, seen, &items);
        assert_eq!(classes.get("coverall").map(String::as_str), Some("SHLD_GODI_S02_CoverAll_SCItem"), "case alone is no second class");
        assert_eq!(classes.get("ad6b ballistic gatling").map(String::as_str), Some("BEHR_BallisticGatling_S6"));
        assert_eq!(classes.get("okunis").map(String::as_str), Some("Mining_Gadget_SHIN_Okunis"), "named through the journal");
        assert_eq!(classes.get("twin"), None, "two classes carry the name: none is guessed");
        assert!(!classes.values().any(|c| c == "PLACEHOLDER_X"));
    }

    #[test]
    fn the_stored_files_are_read_in_their_formats() {
        use crate::legacy;
        use crate::paths::Host;
        use crate::pypath::Flavor;

        let root = std::env::temp_dir().join(format!("versekit-lagernamen-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("home")).unwrap();
        let host = Host::new(
            Flavor::native(),
            &[(legacy::ENV_HOME, root.join("home").to_str().unwrap())],
            root.to_str().unwrap(),
        );
        let paths = Paths::new(host);
        let write = |name: &str, text: &str| std::fs::write(paths.app_file(name), text).unwrap();
        // Nothing stored: nothing is named, nothing breaks.
        assert_eq!(Items::load(&paths).item("a_class", "g"), ItemName::Unnamed("a_class".into()));
        assert_eq!(Places::load(&paths).place("1"), PlaceName::Unknown("1".into()));
        write(asop::NAMES_FILE, r#"{"item_Namea_class": "A Class", "other": "x"}"#);
        write(crate::shops::CATALOG_CACHE, r#"{"format": 7, "teile": [{"n": "Shop Part", "s": "AA-BB"}]}"#);
        write(VALUES_FILE, r#"{"values": {"items": {"CC-DD": {"class_name": "valued_class", "name": "Valued"}}}}"#);
        write(stock_data::PLACES_FILE, r#"{"format": 1, "orte": ["Orison"]}"#);
        write(location::LEARNED_FILE, r#"{"orte": {"7": "Stanton2_Orison"}, "gelesen": {}}"#);
        write(location::NAMES_FILE, r#"{"format": 1, "orte": {"stanton2_orison": "Orison"}}"#);
        let items = Items::load(&paths);
        assert_eq!(items.item("a_class", ""), ItemName::Named("A Class".into()));
        assert_eq!(items.item("zzz", "aa-bb"), ItemName::Named("Shop Part".into()));
        assert_eq!(items.item("valued_class", ""), ItemName::Named("Valued".into()));
        assert_eq!(Places::load(&paths).place("7"), PlaceName::Listed("Orison".into()));
        // A place the player added belongs to the list.
        write(location::LEARNED_FILE, r#"{"orte": {"7": "Stanton2_Orison", "8": "Pyro_Odd"}, "gelesen": {}}"#);
        assert_eq!(Places::load(&paths).place("8"), PlaceName::Shown("Pyro Odd".into()));
        let added = crate::places::OwnPlace { name: "Pyro Odd".into(), system: None };
        assert!(crate::places::add_own(&paths, &[added]).is_some());
        assert_eq!(Places::load(&paths).place("8"), PlaceName::Listed("Pyro Odd".into()));
        let data = stock_data::StockData::load(|name| paths.app_file(name));
        assert_eq!(data.places(), ["Orison", "Pyro Odd"]);
        // A list of another format is not trusted: without a list every name counts, the added ones are no list.
        write(stock_data::PLACES_FILE, r#"{"format": 2, "orte": ["Orison"]}"#);
        assert_eq!(Places::load(&paths).place("7"), PlaceName::Listed("Orison".into()));
        assert!(stock_data::StockData::load(|name| paths.app_file(name)).places().is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
