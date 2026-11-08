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

//! From which reputation a rank begins, for the rank names in the
//! reputation menu (`Gildenmitglied [ab 10.000]`): port of
//! `rank_thresholds.py`.
//!
//! The numbers come from `StarCitizenWiki/scunpacked-data`, fetched at run
//! time and never shipped; only the result is stored (`rufstufen.json`),
//! bound to the game version. The mapping of language keys to standings is
//! hand work; a group whose standings are missing or do not rise gets no
//! number at all.

use std::sync::LazyLock;

use regex::Regex;

use crate::errors;
use crate::jsonio::{self, Map, Value};
use crate::net::Net;
use crate::paths::Paths;

pub const CACHE_FILE: &str = "rufstufen.json";
pub const FORMAT: i64 = 1;
/// Standings from this threshold get a number.
pub const MIN_SHOWN: i64 = 1;
/// The setting „Ruf-Stufen an den Rängen“.
pub const SETTING: &str = "ruf_stufen";
pub const BASE: &str = "https://raw.githubusercontent.com/StarCitizenWiki/scunpacked-data/master/factions";
pub const USER_AGENT: &str = crate::net::USER_AGENT;

/// One group: file, scope, and (language key, standing name) rising.
struct Group {
    file: &'static str,
    scope: &'static str,
    ranks: Vec<(String, String)>,
}

fn numbered(key: &str, name: &str, range: std::ops::RangeInclusive<u32>) -> Vec<(String, String)> {
    range.map(|n| (key.replace("{}", &n.to_string()), name.replace("{}", &n.to_string()))).collect()
}

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter().map(|(k, n)| (k.to_string(), n.to_string())).collect()
}

/// `rank_thresholds.RANKS`, in its order.
static RANKS: LazyLock<Vec<Group>> = LazyLock::new(|| {
    let mut security = numbered("RepStanding_Security_Rank{}", "Security_Rank{}_MercenaryGuild", 0..=4);
    security.extend(pairs(&[
        ("RepStanding_Security_Rank5", "Security_Rank5"),
        ("RepStanding_Security_Rank6", "Security_Rank6"),
    ]));
    vec![
        Group {
            file: "faction_reputation_lawful_foxwellenforcement.json",
            scope: "FactionReputation",
            ranks: numbered("RepScope_Contractor_Rank{}", "FactionRep_Allied_Rank{}", 1..=6),
        },
        Group {
            file: "faction_lawful_aciedo.json",
            scope: "Technician",
            ranks: numbered("RepScope_Technician_Rank{}", "Technician_Rank{}", 0..=5),
        },
        Group {
            file: "faction_reputation_lawful_bountyhuntersguild.json",
            scope: "BountyHunter_BountyHuntersGuild",
            ranks: pairs(&[
                ("RepStanding_Bounty_Applicant_Name", "BountyHunter_Applicant_BountyHuntersGuild"),
                ("RepStanding_Bounty_Probation_Name", "BountyHunter_Probation_BountyHuntersGuild"),
                ("RepStanding_Bounty_Junior_Name", "BountyHunter_Junior_BountyHuntersGuild"),
                ("RepStanding_Bounty_MidLevel_Name", "BountyHunter_MidLevel_BountyHuntersGuild"),
                ("RepStanding_Bounty_Senior_Name", "BountyHunter_Senior_BountyHuntersGuild"),
                // The names swap here: `Master` in the key is `VeteranAgent` in the data.
                ("RepStanding_Bounty_MasterBountyHunter_Name", "BountyHunter_VeteranAgent_BountyHuntersGuild"),
                ("RepStanding_Bounty_LegendaryBountyHunter_Name", "BountyHunter_MasterAgent_BountyHuntersGuild"),
            ]),
        },
        Group {
            file: "faction_reputation_lawful_covalexshipping.json",
            scope: "Hauling",
            ranks: numbered("RepStanding_TransportGuild_Rank{}", "Delivery_Rank{}", 0..=6),
        },
        Group {
            file: "factionreputation_lawful_northrockservicegroup.json",
            scope: "Security_MercenaryGuild",
            ranks: security,
        },
        Group {
            file: "factionreputation_battaglia.json",
            scope: "MissionProviderReputation_Battaglia",
            ranks: numbered("mg_battaglia_RepScope_Rank{}", "MissionProviderRep_Battaglia_Rank{}", 0..=5),
        },
        Group {
            file: "factionreputation_wikelo.json",
            scope: "Wikelo",
            ranks: pairs(&[
                ("RepStanding_Barter_Rank0_Name", "Nothing"),
                ("RepStanding_Barter_Rank1_Name", "Armour"),
                ("RepStanding_Barter_Rank2_Name", "Wolf"),
            ]),
        },
    ]
});

/// Our addition at the end of a rank name (`rank_thresholds._SUFFIX`).
static SUFFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"{}\[(?:ab [\d.]+|[\d,]+\+)\]$", crate::text::PY_SPACE)).expect("pattern"));

/// The stored thresholds: game version and key → threshold, in file order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stored {
    pub version: String,
    pub thresholds: Vec<(String, Value)>,
}

/// `rank_thresholds.load`.
pub fn load(paths: &Paths) -> Stored {
    match jsonio::read_json(&paths.app_file(CACHE_FILE)) {
        Ok(data) if data.get("format") == Some(&Value::int(FORMAT)) => match data.get("stufen") {
            Some(Value::Object(map)) => Stored {
                version: data.get("version").and_then(Value::as_str).unwrap_or("").to_string(),
                thresholds: map.clone(),
            },
            _ => Stored::default(),
        },
        _ => Stored::default(),
    }
}

fn save(paths: &Paths, version: &str, table: &[(String, i64)]) -> bool {
    let data: Map = vec![
        ("format".into(), Value::int(FORMAT)),
        ("version".into(), Value::str(version)),
        ("stufen".into(), Value::Object(table.iter().map(|(k, v)| (k.clone(), Value::int(*v))).collect())),
    ];
    let target = paths.app_file(CACHE_FILE);
    let text = jsonio::dumps(&Value::Object(data), None, false);
    match jsonio::write_text_atomic(&target, &text, jsonio::Newline::Keep) {
        Ok(()) => true,
        Err(error) => {
            errors::record("rank_thresholds.save", &error);
            false
        }
    }
}

/// `rank_thresholds._standings`: standing name → threshold of one scope.
fn standings<'a>(raw: Option<&'a Value>, scope_name: &str) -> Vec<(&'a str, &'a Value)> {
    let Some(context) = raw.and_then(|r| r.get("Reputation")).and_then(|r| r.get("Context")) else {
        return Vec::new();
    };
    let mut scopes: Vec<&Value> = context.get("PrimaryScope").into_iter().collect();
    if let Some(Value::Array(more)) = context.get("AdditionalScopes") {
        scopes.extend(more);
    }
    for scope in scopes {
        if scope.as_object().is_some() && scope.get("ScopeName").and_then(Value::as_str) == Some(scope_name) {
            let Some(Value::Array(list)) = scope.get("Standings") else { return Vec::new() };
            return list
                .iter()
                .filter(|s| s.as_object().is_some())
                .filter_map(|s| Some((s.get("Name").and_then(Value::as_str)?, s.get("MinReputation")?)))
                .collect();
        }
    }
    Vec::new()
}

/// `rank_thresholds.prepare`: language key → threshold; a group only
/// whole or not at all.
pub fn prepare(raw_by_file: &[(String, Value)]) -> Vec<(String, i64)> {
    let mut out = Vec::new();
    for group in RANKS.iter() {
        let raw = raw_by_file.iter().find(|(f, _)| f == group.file).map(|(_, v)| v);
        let found = standings(raw, group.scope);
        // A later standing of the same name wins, like a Python dict.
        let value_of = |name: &str| found.iter().rev().find(|(n, _)| *n == name).map(|(_, v)| *v);
        let values: Option<Vec<i64>> = group
            .ranks
            .iter()
            .map(|(_, name)| match value_of(name) {
                Some(Value::Int(text)) => text.parse().ok(),
                _ => None,
            })
            .collect();
        let Some(values) = values else { continue };
        if values.windows(2).any(|w| w[1] <= w[0]) {
            continue;
        }
        out.extend(group.ranks.iter().zip(values).map(|((key, _), value)| (key.clone(), value)));
    }
    out
}

/// `rank_thresholds.refresh`: fetch when missing or of another version.
/// Returns the number of keys; on errors the old state stays.
pub fn refresh(paths: &Paths, net: &dyn Net, game_version: &str) -> usize {
    let old = load(paths);
    if !old.thresholds.is_empty() && (game_version.is_empty() || old.version == game_version) {
        return old.thresholds.len();
    }
    if crate::net::offline_in(&paths.host) {
        return old.thresholds.len();
    }
    let mut raw_by_file = Vec::new();
    for group in RANKS.iter() {
        match net.get(&format!("{BASE}/{}", group.file)) {
            Ok(bytes) => match jsonio::loads(&String::from_utf8_lossy(&bytes)) {
                Ok(value) => raw_by_file.push((group.file.to_string(), value)),
                Err(error) => errors::record("rank_thresholds.refresh", &error),
            },
            Err(error) => errors::record("rank_thresholds.refresh", &error),
        }
    }
    let table = prepare(&raw_by_file);
    if table.is_empty() {
        return old.thresholds.len();
    }
    save(paths, game_version, &table);
    table.len()
}

fn grouped(value: i64, sep: &str) -> String {
    super::texts::group_digits(i128::from(value), sep)
}

/// `rank_thresholds.suffix`: ` [ab 10.000]` in German, ` [10,000+]` else.
pub fn suffix(value: i64, lang_code: &str) -> String {
    if lang_code == "de" { format!(" [ab {}]", grouped(value, ".")) } else { format!(" [{}+]", grouped(value, ",")) }
}

/// `rank_thresholds.strip_suffix`.
pub fn strip_suffix(text: &str) -> String {
    SUFFIX.replace(text, "").into_owned()
}

/// `rank_thresholds.with_suffix`.
pub fn with_suffix(text: &str, tail: &str) -> String {
    format!("{}{tail}", crate::text::py_rstrip(&strip_suffix(text)))
}

/// `rank_thresholds.build_table`: language key → addition; empty when off.
pub fn build_table(paths: &Paths, lang_code: &str, stored: &Stored) -> Vec<(String, String)> {
    if !paths.setting_bool(SETTING, true) {
        return Vec::new();
    }
    stored
        .thresholds
        .iter()
        .filter_map(|(key, value)| match value {
            Value::Int(text) => {
                text.parse::<i64>().ok().filter(|v| *v >= MIN_SHOWN).map(|v| (key.clone(), suffix(v, lang_code)))
            }
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Without a stored table the ranks are asked for, unless the given environment has
    /// `SC_BP_NO_NET`; the shell of the check does not count.
    #[test]
    fn no_net_of_the_given_environment_asks_nothing() {
        use crate::paths::Host;
        use crate::pypath::Flavor;
        let dir = std::env::temp_dir().join(format!("versekit-raenge-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let data = dir.to_string_lossy().into_owned();
        let paths = |no_net: &str| {
            Paths::new(Host::new(Flavor::native(), &[("SC_BP_HOME", data.as_str()), ("SC_BP_NO_NET", no_net)], "/"))
        };
        let net = crate::net::Fake::default();
        assert_eq!(refresh(&paths("1"), &net, "4.3"), 0);
        assert!(net.asked().is_empty(), "{:?}", net.asked());
        refresh(&paths("0"), &net, "4.3");
        assert_eq!(net.asked().len(), RANKS.len(), "{:?}", net.asked());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn suffix_per_language() {
        assert_eq!(suffix(10000, "de"), " [ab 10.000]");
        assert_eq!(suffix(10000, "en"), " [10,000+]");
        assert_eq!(with_suffix("Gildenmitglied [ab 800]", " [ab 800]"), "Gildenmitglied [ab 800]");
        assert_eq!(with_suffix("Member [800+]", " [800+]"), "Member [800+]");
        // Like 4.12: a space behind the addition hides it from the pattern.
        assert_eq!(with_suffix("Member [800+] ", " [800+]"), "Member [800+] [800+]");
    }

    fn faction(scope: &str, standings: &[(&str, i64)]) -> Value {
        let list: Vec<String> =
            standings.iter().map(|(n, v)| format!(r#"{{"Name": "{n}", "MinReputation": {v}}}"#)).collect();
        jsonio::loads(&format!(
            r#"{{"Reputation": {{"Context": {{"PrimaryScope": {{"ScopeName": "{scope}", "Standings": [{}]}}}}}}}}"#,
            list.join(",")
        ))
        .unwrap()
    }

    #[test]
    fn a_group_counts_only_whole_and_rising() {
        let wikelo = faction("Wikelo", &[("Nothing", 0), ("Armour", 100), ("Wolf", 500)]);
        let falling = faction("Technician", &[("Technician_Rank0", 0), ("Technician_Rank1", 5)]);
        let table = prepare(&[
            ("factionreputation_wikelo.json".into(), wikelo),
            ("faction_lawful_aciedo.json".into(), falling),
        ]);
        assert_eq!(
            table,
            vec![
                ("RepStanding_Barter_Rank0_Name".to_string(), 0),
                ("RepStanding_Barter_Rank1_Name".to_string(), 100),
                ("RepStanding_Barter_Rank2_Name".to_string(), 500)
            ]
        );
    }
}
