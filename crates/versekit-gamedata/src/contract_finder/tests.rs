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

use versekit_core::paths::Host;
use versekit_core::pypath::Flavor;
use versekit_core::testdir::Dir;

use super::*;

fn object(json: &str) -> Map {
    jsonio::loads(json).expect("json").as_object().cloned().expect("object")
}

/// The reputation table of the tests: one title with every term, one with reputation only, one with neither.
fn table() -> reputation::Table {
    reputation::Table {
        version: "4.10.3-live.1".into(),
        terms: object(
            r#"{"@Full_Title_001": {"rang": "Jr. Contractor", "rep": 800, "rang_max": "Elite Contractor", "rep_max": 95250,
                "warte": [30, 45], "warte_streuung": 10, "teilbar": false, "auftraggeber": "Bit Zeros"},
                "quiet_title": {"warte_streuung": 5, "teilbar": true}}"#,
        ),
        contracts: object(
            r#"{"full_title_001": [{"wer": "Bit Zeros", "was": "Standing", "wieviel": [50, 200]},
                                   {"wer": "Rivals", "was": "Standing", "wieviel": -25}],
                "rep_only_title": [{"wer": "Headhunters", "was": "", "wieviel": 75}, {"wer": "", "was": "", "wieviel": 5}]}"#,
        ),
        regions: object(r#"{"full_title_001": "Pyro (Bloom, Pyro IV), Stanton", "rep_only_title": "  "}"#),
        ..reputation::Table::default()
    }
}

fn missions() -> HashMap<String, Vec<String>> {
    [
        (
            "Full_Title_001".to_string(),
            vec!["Zeta Rifle".to_string(), "alpha helmet".to_string(), "Zeta Rifle".to_string()],
        ),
        ("Catalog_Only_Title".to_string(), vec!["Beta Legs".to_string()]),
    ]
    .into_iter()
    .collect()
}

fn by_key<'a>(contracts: &'a [Contract], key: &str) -> &'a Contract {
    contracts.iter().find(|c| c.key == key).unwrap_or_else(|| panic!("{key} missing: {contracts:?}"))
}

/// Every title of the table and the catalog is one contract with what the files state of it; what they do not state
/// stays unknown.
#[test]
fn the_contracts_join_the_terms_the_reputation_the_regions_and_the_blueprints() {
    let titles: HashMap<String, String> = [("full_title_001".to_string(), "Recover the [ITEM]".to_string())].into();
    let missions = missions();
    let contracts = contracts(&table(), Some(&missions), &titles);
    let keys: Vec<&str> = contracts.iter().map(|c| c.key.as_str()).collect();
    assert_eq!(
        keys,
        ["catalog_only_title", "full_title_001", "quiet_title", "rep_only_title"],
        "one per title, by key"
    );

    let full = by_key(&contracts, "full_title_001");
    assert_eq!(full.title.as_deref(), Some("Recover the [ITEM]"));
    assert_eq!(full.giver.as_deref(), Some("Bit Zeros"));
    assert_eq!(
        full.gains,
        [
            Gain { who: "Bit Zeros".into(), kind: "Standing".into(), low: 50, high: 200, unknown: false },
            Gain { who: "Rivals".into(), kind: "Standing".into(), low: -25, high: -25, unknown: false },
        ]
    );
    assert_eq!(full.best_gain(), Some(200));
    assert_eq!((full.cooldown, full.cooldown_spread), (Some((30, 45)), Some(10)));
    assert_eq!(full.shareable, Some(false));
    assert_eq!(full.region.as_deref(), Some("Pyro (Bloom, Pyro IV), Stanton"));
    assert_eq!(full.systems, ["Pyro", "Stanton"]);
    assert_eq!(full.rank, Some(Rank { name: "Jr. Contractor".into(), reputation: 800 }));
    assert_eq!(full.rank_max, Some(Rank { name: "Elite Contractor".into(), reputation: 95250 }));
    assert_eq!(full.blueprints.as_deref(), Some(&["alpha helmet".to_string(), "Zeta Rifle".to_string()][..]));

    // A spread without a cooldown is none; the rest is unknown, not made up.
    let quiet = by_key(&contracts, "quiet_title");
    assert_eq!((quiet.cooldown, quiet.cooldown_spread, quiet.shareable), (None, None, Some(true)));
    assert_eq!((quiet.title.as_deref(), quiet.giver.as_deref(), quiet.rank.as_ref()), (None, None, None));
    assert!(quiet.gains.is_empty() && quiet.region.is_none() && quiet.systems.is_empty());
    assert_eq!(quiet.blueprints.as_deref(), Some(&[][..]), "the catalog knows no blueprint of it");

    let rep_only = by_key(&contracts, "rep_only_title");
    assert_eq!(rep_only.gains, [Gain { who: "Headhunters".into(), kind: String::new(), low: 75, high: 75, unknown: false }]);
    assert_eq!(rep_only.region, None, "an empty region is none");
    assert_eq!(by_key(&contracts, "catalog_only_title").blueprints.as_deref(), Some(&["Beta Legs".to_string()][..]));

    // Without a catalog the blueprints are unknown.
    let without = contracts_without_catalog();
    assert!(without.iter().all(|c| c.blueprints.is_none()), "{without:?}");
    assert!(!without.iter().any(|c| c.key == "catalog_only_title"));
}

/// A gain whose amount the table does not know stays in the contract, marked unknown; it is no gain of zero and does
/// not count for the largest one.
#[test]
fn a_gain_of_an_unknown_amount_is_kept_and_marked() {
    let table = reputation::Table {
        contracts: object(
            r#"{"shubin_title": [{"wer": "Shubin", "was": "Standing", "wieviel": null}, {"wer": "Guild", "was": "", "wieviel": 40}],
                "only_unknown_title": [{"wer": "Covalex", "was": "Standing", "wieviel": null}]}"#,
        ),
        ..reputation::Table::default()
    };
    let found = contracts(&table, None, &HashMap::new());
    let shubin = by_key(&found, "shubin_title");
    assert_eq!(
        shubin.gains,
        [
            Gain { who: "Shubin".into(), kind: "Standing".into(), low: 0, high: 0, unknown: true },
            Gain { who: "Guild".into(), kind: String::new(), low: 40, high: 40, unknown: false },
        ]
    );
    assert_eq!(shubin.best_gain(), Some(40), "the unknown one is not a zero that could win or lose");
    let only = by_key(&found, "only_unknown_title");
    assert_eq!(only.gains.len(), 1);
    assert_eq!(only.best_gain(), None);
}

fn contracts_without_catalog() -> Vec<Contract> {
    contracts(&table(), None, &HashMap::new())
}

#[test]
fn the_titles_come_from_the_language_file_without_marks_and_with_their_placeholders_named() {
    let ini = "\u{feff}Full_Title_001,P=Recover ~mission(Item|Cargo) at ~mission(location)\r\n\
               full_title_001=A second line of the key\r\n\
               Marked_Title=<EM4>[BP]</EM4> Marked  Contract\r\n\
               Other_Title=Not wanted\r\n\
               Empty_Title=\r\n\
               no equals sign\r\n";
    let wanted: HashSet<String> =
        ["full_title_001", "marked_title", "empty_title", "missing_title"].into_iter().map(String::from).collect();
    let titles = titles_of(ini, &wanted);
    assert_eq!(
        titles.get("full_title_001").map(String::as_str),
        Some("Recover [ITEM] at [LOCATION]"),
        "first line counts"
    );
    assert_eq!(titles.get("marked_title").map(String::as_str), Some("Marked Contract"));
    assert_eq!(titles.len(), 2, "{titles:?}");
}

#[test]
fn the_systems_of_a_region_are_its_parts_outside_the_brackets() {
    assert_eq!(systems_of("Nyx, Pyro (Bloom, Pyro IV), Stanton"), ["Nyx", "Pyro", "Stanton"]);
    assert_eq!(systems_of("Stanton (Crusader, microTech)"), ["Stanton"]);
    assert_eq!(systems_of("Pyro, Pyro (Bloom)"), ["Pyro"]);
    assert!(systems_of("").is_empty());
}

fn sample() -> Vec<Contract> {
    let rank = |name: &str, reputation: i64| Some(Rank { name: name.into(), reputation });
    let contract = |key: &str, title: Option<&str>| Contract {
        key: key.into(),
        title: title.map(str::to_string),
        ..Contract::default()
    };
    vec![
        Contract {
            giver: Some("Headhunters".into()),
            gains: vec![Gain { who: "Headhunters".into(), kind: "Standing".into(), low: 50, high: 50, unknown: false }],
            cooldown: Some((45, 45)),
            shareable: Some(true),
            region: Some("Pyro (Bloom)".into()),
            systems: vec!["Pyro".into()],
            rank: rank("Jr. Contractor", 800),
            blueprints: Some(vec!["Arclight Pistol".into()]),
            ..contract("hh_title", Some("Hit the Target"))
        },
        Contract {
            giver: Some("Bit Zeros".into()),
            gains: vec![Gain { who: "Bit Zeros".into(), kind: "Standing".into(), low: 100, high: 300, unknown: false }],
            cooldown: Some((10, 30)),
            shareable: Some(false),
            region: Some("Stanton".into()),
            systems: vec!["Stanton".into()],
            rank: rank("Neutral", 0),
            blueprints: Some(Vec::new()),
            ..contract("bz_title", Some("Äther Recovery"))
        },
        Contract {
            giver: Some("Bit Zeros".into()),
            shareable: Some(true),
            rank: rank("Contractor", 2200),
            blueprints: Some(vec!["Beta Legs".into(), "Gamma Arms".into()]),
            ..contract("bz2_title", None)
        },
        contract("unknown_title", Some("Mystery")),
    ]
}

fn keys(contracts: &[Contract], view: &View) -> Vec<String> {
    arrange(contracts, view).into_iter().map(|at| contracts[at].key.clone()).collect()
}

#[test]
fn the_list_filters_by_text_giver_system_rank_blueprint_and_sharing() {
    let all = sample();
    let view = |change: &dyn Fn(&mut View)| {
        let mut view = View::new();
        change(&mut view);
        keys(&all, &view)
    };
    assert_eq!(view(&|_| {}), ["bz_title", "bz2_title", "hh_title", "unknown_title"], "by title, the key without one");
    assert_eq!(view(&|v| v.text = "aether".into()), ["bz_title"], "umlauts alike");
    assert_eq!(view(&|v| v.text = "ARCLIGHT".into()), ["hh_title"], "a blueprint");
    assert_eq!(view(&|v| v.text = "bloom".into()), ["hh_title"], "a region");
    assert_eq!(view(&|v| v.text = "bz2".into()), ["bz2_title"], "the key");
    assert_eq!(view(&|v| v.giver = Some("Bit Zeros".into())), ["bz_title", "bz2_title"]);
    assert_eq!(view(&|v| v.system = Some("Stanton".into())), ["bz_title"]);
    let held = |name: &str, reputation: i64| Some(Rank { name: name.into(), reputation });
    assert_eq!(
        view(&|v| v.rank = held("Neutral", 0)),
        ["bz_title"],
        "a holder of Neutral takes what starts at Neutral"
    );
    assert_eq!(
        view(&|v| v.rank = held("Jr. Contractor", 800)),
        ["bz_title", "hh_title"],
        "a holder takes what starts at or below its rank, not the contract without a known rank"
    );
    assert_eq!(view(&|v| v.rank = held("Contractor", 2200)), ["bz_title", "bz2_title", "hh_title"]);
    assert_eq!(without_rank(&all, &View::new()), 1, "the contract without a rank");
    let mut shareable = View::new();
    shareable.shareable = true;
    shareable.rank = held("Contractor", 2200);
    assert_eq!(without_rank(&all, &shareable), 0, "the one without a rank is not shareable either");
    assert_eq!(view(&|v| v.with_blueprint = true), ["bz2_title", "hh_title"], "an empty list is no blueprint");
    assert_eq!(view(&|v| v.shareable = true), ["bz2_title", "hh_title"], "unknown is not shareable");
    assert!(
        view(&|v| {
            v.giver = Some("Headhunters".into());
            v.system = Some("Stanton".into());
        })
        .is_empty()
    );
}

/// A highest rank keeps a holder of a higher rank from the contract; a contract without one takes every rank above its
/// lowest.
#[test]
fn a_highest_rank_keeps_higher_ranks_out() {
    let rank = |name: &str, reputation: i64| Rank { name: name.into(), reputation };
    let capped =
        Contract { rank: Some(rank("Neutral", 0)), rank_max: Some(rank("Contractor", 2200)), ..Contract::default() };
    let open = Contract { rank: Some(rank("Neutral", 0)), ..Contract::default() };
    assert!(takes(&capped, &rank("Contractor", 2200)), "the highest rank itself still gets it");
    assert!(!takes(&capped, &rank("Sr. Contractor", 5800)));
    assert!(takes(&open, &rank("Sr. Contractor", 5800)));
    assert!(!takes(&Contract::default(), &rank("Neutral", 0)), "an unknown lowest rank is not guessed");
}

#[test]
fn the_list_sorts_by_each_column_and_unknown_values_stand_last_either_way() {
    let all = sample();
    let sorted = |column: Column, ascending: bool| keys(&all, &View { sort: column, ascending, ..View::new() });
    assert_eq!(sorted(Column::Gain, true), ["hh_title", "bz_title", "bz2_title", "unknown_title"]);
    assert_eq!(sorted(Column::Gain, false), ["bz_title", "hh_title", "bz2_title", "unknown_title"]);
    assert_eq!(sorted(Column::Cooldown, true), ["bz_title", "hh_title", "bz2_title", "unknown_title"]);
    assert_eq!(sorted(Column::Rank, true), ["bz_title", "hh_title", "bz2_title", "unknown_title"]);
    assert_eq!(sorted(Column::Rank, false), ["bz2_title", "hh_title", "bz_title", "unknown_title"]);
    assert_eq!(sorted(Column::Giver, true), ["bz_title", "bz2_title", "hh_title", "unknown_title"]);
    assert_eq!(sorted(Column::Shareable, true), ["bz2_title", "hh_title", "bz_title", "unknown_title"], "yes first");
    assert_eq!(sorted(Column::Region, false), ["bz_title", "hh_title", "bz2_title", "unknown_title"]);
    assert_eq!(sorted(Column::Blueprints, false), ["bz2_title", "hh_title", "bz_title", "unknown_title"]);
    assert_eq!(sorted(Column::Title, false), ["unknown_title", "hh_title", "bz2_title", "bz_title"]);

    let mut view = View::new();
    view.sort_by(Column::Gain);
    assert_eq!((view.sort, view.ascending), (Column::Gain, true));
    view.sort_by(Column::Gain);
    assert_eq!((view.sort, view.ascending), (Column::Gain, false), "the same column turns");
    view.sort_by(Column::Rank);
    assert_eq!((view.sort, view.ascending), (Column::Rank, true));
}

#[test]
fn the_choices_are_every_giver_system_and_lowest_rank_once() {
    let mut all = sample();
    all.push(Contract {
        key: "again".into(),
        rank: Some(Rank { name: "Jr. Contractor".into(), reputation: 700 }),
        systems: vec!["Nyx".into(), "Pyro".into()],
        ..Contract::default()
    });
    let choices = Choices::of(&all);
    assert_eq!(choices.givers, ["Bit Zeros", "Headhunters"]);
    assert_eq!(choices.systems, ["Nyx", "Pyro", "Stanton"]);
    let ranks: Vec<(&str, i64)> = choices.ranks.iter().map(|r| (r.name.as_str(), r.reputation)).collect();
    assert_eq!(ranks, [("Neutral", 0), ("Jr. Contractor", 700), ("Contractor", 2200)]);
}

/// The files of a data folder and the language file of the game folder given from outside.
#[test]
fn the_finder_reads_the_files_of_the_data_folder_and_the_game() {
    let dir = Dir::new("auftragsfinder-laden");
    let home = dir.path().join("daten");
    let game = dir.path().join("LIVE");
    std::fs::create_dir_all(game.join("data").join("Localization").join("english")).unwrap();
    std::fs::write(game.join("Game.log"), "").unwrap();
    std::fs::write(
        game.join("data").join("Localization").join("english").join("global.ini"),
        "\u{feff}Full_Title_001=Recover the Data\r\nQuiet_Title=Quiet Work\r\n",
    )
    .unwrap();
    let (home_text, game_text) = (home.to_string_lossy().into_owned(), game.to_string_lossy().into_owned());
    let paths = Paths::new(Host::new(
        Flavor::native(),
        &[
            (versekit_core::envvars::ENV_HOME, home_text.as_str()),
            (versekit_core::envvars::ENV_NO_NET, "1"),
            (versekit_core::paths::ENV_INSTALL_DIR, game_text.as_str()),
        ],
        &home_text,
    ));
    assert!(load(&paths).contracts.is_empty(), "nothing stored, nothing listed");
    let before = Stamps::of(&paths);

    let table = r#"{"format": 3, "version": "4.10.3-live.1",
        "auftraege": {"full_title_001": [{"wer": "Bit Zeros", "was": "Standing", "wieviel": 100}]},
        "regionen": {"full_title_001": "Stanton"}, "datenfehler": "",
        "angaben": {"full_title_001": {"rang": "Neutral", "rep": 0, "warte": 15, "teilbar": true, "auftraggeber": "Bit Zeros"},
                    "quiet_title": {}},
        "angaben_quelle": "spieldaten"}"#;
    let catalog = r#"{"version": "4.10.3-live.1", "bauplaene": {"x": {"n": "X"}},
        "missionen": {"Full_Title_001": {"bp": ["X"]}}}"#;
    let table_file = paths.app_file(reputation::CACHE_FILE);
    std::fs::create_dir_all(std::path::Path::new(&table_file).parent().unwrap()).unwrap();
    std::fs::write(&table_file, table).unwrap();
    std::fs::write(paths.app_file(versekit_core::catalog_cache::LIST_CACHE_FILE), catalog).unwrap();
    assert_ne!(Stamps::of(&paths), before, "the stamps see the files");

    let finder = load(&paths);
    assert_eq!(finder.version, "4.10.3-live.1");
    let full = by_key(&finder.contracts, "full_title_001");
    assert_eq!(full.title.as_deref(), Some("Recover the Data"));
    assert_eq!(full.giver.as_deref(), Some("Bit Zeros"));
    assert_eq!((full.cooldown, full.shareable), (Some((15, 15)), Some(true)));
    assert_eq!(full.systems, ["Stanton"]);
    assert_eq!(full.blueprints.as_deref(), Some(&["X".to_string()][..]));
    let quiet = by_key(&finder.contracts, "quiet_title");
    assert_eq!(quiet.title.as_deref(), Some("Quiet Work"));
    assert_eq!(quiet.blueprints.as_deref(), Some(&[][..]));
}
