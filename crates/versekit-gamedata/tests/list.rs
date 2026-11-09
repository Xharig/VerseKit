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

//! The list catalog from the synthetic `Data.p4k`: a new user without any
//! cache of the 4.12 version gets the whole catalog, scmdb completes it where
//! it can be had and stands in where the archive cannot be read, the cache
//! file, and the quiet fallback without an installation.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use versekit_core::catalog_cache::{self, CatalogCache};
use versekit_core::jsonio::{self, Value};
use versekit_core::legacy;
use versekit_core::paths::{Host, Paths};
use versekit_core::pypath::Flavor;
use versekit_core::writeguard;
use versekit_gamedata::corrections::{self, Correction};
use versekit_gamedata::list::{self, CACHE_FILE, Offline, Supply};

/// What scmdb gives for the fixture build (`4.12.0-live.13000000`): the
/// ways of the rifle with rewards and places by system.
const THEIRS: &str = r#"{"version": "4.12.0-live.13000000", "bauplaene": {
    "p8-ar rifle": {"n": "P8-AR Rifle",
        "q": [{"auftrag": "Hunt [TARGET] in the ~mission(Missing)", "typ": "Special hunt",
               "fraktion": "Highpoint Wilderness Specialists", "uec": 15000, "ruf": 200, "rang": "Rank Two",
               "rep": 2500, "wo": {"system": "Stanton", "orte": ["Crusader", "Hurston"], "mehr": 0}},
              {"auftrag": "Hunt the Kopion at [LOCATION] for Highpoint Org", "typ": "Hunting",
               "fraktion": "Highpoint Wilderness Specialists", "uec": 12000, "ruf": 200, "rang": "Rank Two",
               "rep": 2500, "wo": {"system": "Stanton", "orte": ["Hurston"], "mehr": 0}}]}}}"#;

/// A 4.12 cache that disagrees with the game data in every place the old
/// completion read it: the starter blueprint, the patch, the reputation
/// limit and the place. None of it may show up.
const POISON: &str = r#"{"version": "4.12.0-live.1", "geholt": "2026-10-21 03:00", "bauplaene": {
    "p8-ar rifle": {"n": "P8-AR Rifle", "start": true, "seit": "4.12.0-live.1",
        "q": [{"auftrag": "Template title", "typ": "Poison", "fraktion": "Nobody", "uec": 1, "ruf": 1,
               "wo": {"system": "Poison", "orte": ["Nowhere"], "mehr": 9}}]},
    "only in 4.12": {"n": "Only In 4.12", "start": true}
}, "missionen": {"x": {"bp": ["P8-AR Rifle"], "rep_max": 1, "teilbar": false, "cooldown": 1}}}"#;

/// The supply of the tests: scmdb's catalog or an error, and a count of the
/// requests made.
struct Fake {
    theirs: Option<CatalogCache>,
    build: Option<String>,
    /// The list of known errors is missing though the catalog is there.
    no_list: bool,
    asked: AtomicUsize,
}

impl Fake {
    fn up() -> Arc<Self> {
        let theirs = catalog_cache::parse(&jsonio::loads(THEIRS).unwrap());
        Arc::new(Self {
            theirs: Some(theirs),
            build: Some("4.12.0-live.13000000".into()),
            no_list: false,
            asked: AtomicUsize::new(0),
        })
    }

    fn down() -> Arc<Self> {
        Arc::new(Self { theirs: None, build: None, no_list: false, asked: AtomicUsize::new(0) })
    }

    fn without_list() -> Arc<Self> {
        let up = Self::up();
        Arc::new(Self { theirs: up.theirs.clone(), build: up.build.clone(), no_list: true, asked: AtomicUsize::new(0) })
    }
}

impl Supply for Fake {
    fn current_build(&self) -> Result<String, String> {
        self.build.clone().ok_or_else(|| "scmdb is down".to_string())
    }

    fn corrections(&self, _build: Option<&str>) -> (Vec<Correction>, Option<String>) {
        (corrections::built_in(), (self.theirs.is_none() || self.no_list).then(|| "scmdb is down".to_string()))
    }

    fn catalog(&self, _build: &str, _corrections: &[Correction]) -> Result<CatalogCache, String> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        self.theirs.clone().ok_or_else(|| "scmdb is down".to_string())
    }
}

/// A fresh data folder next to a fixture channel folder. There is no cache
/// of the 4.12 version in it, as for a new user.
fn case(name: &str) -> (Paths, PathBuf, String) {
    let channel = common::channel(name);
    let data = channel.parent().unwrap().join("daten");
    if data.exists() {
        fs::remove_dir_all(&data).unwrap();
    }
    fs::create_dir_all(&data).unwrap();
    let text = data.to_string_lossy().into_owned();
    let paths = Paths::new(Host::new(Flavor::native(), &[("SC_BP_HOME", &text)], &text));
    let cache = data.join(CACHE_FILE).to_string_lossy().into_owned();
    (paths, channel, cache)
}

/// Puts a 4.12 cache into the data folder.
fn with_4_12_cache(paths: &Paths, text: &str) {
    fs::write(paths.app_file(catalog_cache::CACHE_FILE), text).unwrap();
}

fn blueprint<'a>(catalog: &'a CatalogCache, key: &str) -> &'a catalog_cache::Blueprint {
    catalog.blueprints.iter().find(|b| b.key == key).unwrap_or_else(|| panic!("no blueprint {key}"))
}

/// The wait for a build, up to ten seconds.
fn finished(pending: &list::Pending) -> Option<CatalogCache> {
    for _ in 0..1000 {
        if let list::Progress::Done(built) = pending.poll() {
            return built;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("the build did not finish");
}

/// What a patch does to the archive: another file of the same name.
fn patch(channel: &Path) {
    fs::write(channel.join("Data.p4k"), b"not an archive any more").unwrap();
}

/// The cache file as a JSON object, with `change` done to its stamp.
fn change_stamp(cache: &str, change: impl FnOnce(&mut Vec<(String, Value)>)) {
    let Value::Object(mut data) = jsonio::read_json(cache).unwrap() else { panic!("an object") };
    let Some((_, Value::Object(stamp))) = data.iter_mut().find(|(k, _)| k == "quelle") else { panic!("a stamp") };
    change(stamp);
    jsonio::write_text_atomic(cache, &jsonio::dumps(&Value::Object(data), None, false), jsonio::Newline::Native)
        .unwrap();
}

#[test]
fn a_new_user_without_the_4_12_cache_gets_the_full_catalog() {
    let (paths, channel, cache) = case("list-new-user");
    assert!(!Path::new(&paths.app_file(catalog_cache::CACHE_FILE)).exists(), "no cache of the 4.12 version");
    let catalog = list::list_catalog_with(&paths, Some(&channel), &cache, Arc::new(Offline)).expect("a catalog");
    assert!(Path::new(&cache).exists(), "built from the archive");

    // Every blueprint with a name: the rifle with the ways of its three contracts, the starter pistol.
    let keys: Vec<&str> = catalog.blueprints.iter().map(|b| b.key.as_str()).collect();
    assert_eq!(keys, ["p8-ar rifle", "s-38 pistol"]);
    let rifle = blueprint(&catalog, "p8-ar rifle");
    assert_eq!(rifle.ways.len(), 3);
    assert_eq!(
        (rifle.size, rifle.grade, rifle.class.as_deref(), rifle.maker.as_deref()),
        (Some(3), Some(2), Some("Military"), Some("Behring"))
    );

    // The starter blueprint comes from the game data.
    let pistol = blueprint(&catalog, "s-38 pistol");
    assert!(pistol.start && !pistol.no_way && !rifle.start);

    // The reputation limit, shareable and cooldown come from the game data.
    assert_eq!(catalog.missions.len(), 3);
    let shareable: Vec<Option<bool>> = catalog.missions.iter().map(|m| m.shareable).collect();
    assert_eq!(shareable.iter().filter(|s| **s == Some(true)).count(), 2);
    assert_eq!(shareable.iter().filter(|s| **s == Some(false)).count(), 1, "an override says it cannot be shared");
    for mission in &catalog.missions {
        assert_eq!(mission.blueprints, ["p8-ar rifle"]);
        assert_eq!(
            (mission.reputation_max, mission.rank_max.as_deref(), mission.cooldown),
            (Some(95250), Some("Elite"), Some(45))
        );
    }
    let caps = versekit_core::blueprints::reward_caps(&catalog);
    assert_eq!(caps.get("p8-ar rifle"), Some(&(95250, Some("Elite".to_string()))));
    assert_eq!(versekit_core::blueprints::contract_traits(&catalog, "p8-ar rifle"), (Some(false), Some(45)));

    // The version is the build; the patch of a blueprint is not known on the first build.
    assert_eq!(catalog.version, "4.12.0-live.13000000");
    assert!(catalog.blueprints.iter().all(|b| b.since.is_none()));
    // Without scmdb the places stand as the data list them.
    let place = rifle.ways[0].place.as_ref().expect("a place");
    assert_eq!(place.system, "");
}

/// The button "Jetzt neu holen": builds and asks scmdb although the cache is up to date.
#[test]
fn the_own_corrections_come_before_the_game_data_and_stay_out_of_the_cache() {
    let (paths, channel, cache) = case("list-overrides");
    fs::write(
        paths.app_file(versekit_core::overrides::FILE),
        r#"{"overrides": {"p8-ar rifle": {"class": "Ind", "size": 5, "grade": "D"}, "s-38 pistol": {"size": 2}}}"#,
    )
    .unwrap();
    let seen = |catalog: &CatalogCache| {
        let (rifle, pistol) = (blueprint(catalog, "p8-ar rifle"), blueprint(catalog, "s-38 pistol"));
        (rifle.class.clone(), rifle.size, rifle.grade, pistol.size)
    };
    let wanted = (Some("Industrial".to_string()), Some(5), Some(4), Some(2));
    let built = list::list_catalog_with(&paths, Some(&channel), &cache, Arc::new(Offline)).expect("a catalog");
    assert_eq!(seen(&built), wanted, "the build hands out the corrections");
    let start = list::start_catalog_with(&paths, Some(&channel), &cache, Arc::new(Offline));
    assert!(start.rebuild.is_none());
    assert_eq!(seen(&start.catalog.expect("from the cache")), wanted, "so does the cache");
    assert_eq!(seen(&catalog_cache::load_current(&paths).expect("current")), wanted, "and the other readers");
    let kept = catalog_cache::parse(&jsonio::read_json(&cache).unwrap());
    assert_eq!(blueprint(&kept, "p8-ar rifle").size, Some(3), "the cache keeps the game data");

    fs::remove_file(paths.app_file(versekit_core::overrides::FILE)).unwrap();
    let start = list::start_catalog_with(&paths, Some(&channel), &cache, Arc::new(Offline));
    let rifle = start.catalog.expect("from the cache");
    assert_eq!(
        (blueprint(&rifle, "p8-ar rifle").size, blueprint(&rifle, "p8-ar rifle").class.as_deref()),
        (Some(3), Some("Military")),
        "without the file the game data count again"
    );
}

#[test]
fn a_build_on_demand_asks_again_although_the_cache_is_current() {
    let (paths, channel, cache) = case("list-on-demand");
    let up = Fake::up();
    let first = list::list_catalog_with(&paths, Some(&channel), &cache, Arc::clone(&up) as Arc<dyn Supply>).unwrap();
    assert_eq!(up.asked.load(Ordering::SeqCst), 1);
    let start = list::start_catalog_with(&paths, Some(&channel), &cache, Arc::clone(&up) as Arc<dyn Supply>);
    assert!(start.rebuild.is_none(), "the cache is current: the usual way builds nothing");
    assert_eq!(up.asked.load(Ordering::SeqCst), 1);
    // On demand: built and asked once more, the cache written again.
    let archive = channel.join("Data.p4k");
    let rebuild = list::rebuild_now_archive(&paths, Some(&archive), &cache, Arc::clone(&up) as Arc<dyn Supply>);
    let again = rebuild.run().expect("a catalog");
    assert_eq!(up.asked.load(Ordering::SeqCst), 2, "scmdb is asked again");
    assert_eq!(again.blueprints.len(), first.blueprints.len());
    // And without anything to build from it says so (`None`) instead of pretending.
    let nothing = list::rebuild_now_archive(&paths, None, &cache, Fake::down() as Arc<dyn Supply>);
    assert!(nothing.run().is_none());
}

/// The red counter check of the test of the new user: a 4.12 cache next to
/// the data must change nothing.
#[test]
fn a_4_12_cache_is_no_source_of_the_catalog() {
    let (paths, channel, cache) = case("list-poison");
    let clean = list::list_catalog_with(&paths, Some(&channel), &cache, Arc::new(Offline)).expect("a catalog");
    fs::remove_file(&cache).unwrap();
    with_4_12_cache(&paths, POISON);
    let again = list::list_catalog_with(&paths, Some(&channel), &cache, Arc::new(Offline)).expect("a catalog");
    assert_eq!(again, clean, "the 4.12 cache changed the catalog");
    assert!(again.blueprints.iter().all(|b| b.key != "only in 4.12" && !b.start || b.key == "s-38 pistol"));

    // With the history fed (a build with the network) the 4.12 cache is the
    // basis of comparison for what is new, and only that: the names of the
    // history are the game's, none of the old cache's.
    let up = Fake::up();
    fs::remove_file(&cache).unwrap();
    let with_net = list::list_catalog_with(&paths, Some(&channel), &cache, Arc::clone(&up) as Arc<dyn Supply>).unwrap();
    assert!(with_net.blueprints.iter().all(|b| b.key != "only in 4.12"));
    let history = fs::read_to_string(paths.app_file("patch-historie.json")).unwrap();
    assert!(history.contains("S-38 Pistol") && !history.contains("Only In 4.12"), "{history}");
    assert!(!history.contains("Poison"), "{history}");
}

#[test]
fn scmdb_completes_rewards_and_places_when_it_can_be_had() {
    let (paths, channel, cache) = case("list-scmdb-up");
    let supply = Fake::up();
    let catalog =
        list::list_catalog_with(&paths, Some(&channel), &cache, Arc::clone(&supply) as Arc<dyn Supply>).unwrap();
    assert_eq!(supply.asked.load(Ordering::SeqCst), 1);
    let rifle = blueprint(&catalog, "p8-ar rifle");
    let way = rifle.ways.iter().find(|w| w.contract == "Hunt the Kopion at [LOCATION] for Highpoint Org").unwrap();
    assert_eq!(way.reward, Some(12000), "the data state a fixed reward");
    let sub = rifle.ways.iter().find(|w| w.contract.starts_with("Hunt [TARGET]")).unwrap();
    assert_eq!(sub.place.as_ref().map(|p| p.system.as_str()), Some("Stanton"), "scmdb knows the system");
    // Nothing is due while the stamp holds, and scmdb is not asked again.
    let start = list::start_catalog_with(&paths, Some(&channel), &cache, Arc::clone(&supply) as Arc<dyn Supply>);
    assert!(start.rebuild.is_none());
    assert_eq!(supply.asked.load(Ordering::SeqCst), 1);
}

#[test]
fn without_scmdb_the_catalog_is_whole_and_scmdb_is_asked_again_later() {
    let (paths, channel, cache) = case("list-scmdb-down");
    let supply = Fake::down();
    let dyn_supply = || Arc::clone(&supply) as Arc<dyn Supply>;
    let catalog = list::list_catalog_with(&paths, Some(&channel), &cache, dyn_supply()).expect("a catalog");
    assert_eq!(catalog.blueprints.len(), 2, "the game data stand alone");
    // Not due right away.
    assert!(list::start_catalog_with(&paths, Some(&channel), &cache, dyn_supply()).rebuild.is_none());
    // Due after the waiting time: the stamp says scmdb was tried long ago.
    change_stamp(&cache, |stamp| jsonio::map_set(stamp, "versuch", Value::str("1000")));
    let start = list::start_catalog_with(&paths, Some(&channel), &cache, dyn_supply());
    assert!(start.rebuild.is_some(), "scmdb is asked again");
    assert_eq!(start.catalog.as_ref(), Some(&catalog), "the last build stands in meanwhile");
}

/// scmdb's catalog without its list of known errors counts as not brought:
/// names, pools and factions stay uncorrected, so scmdb is asked again.
#[test]
fn a_catalog_without_the_list_of_known_errors_is_asked_for_again() {
    let (paths, channel, cache) = case("list-no-list");
    let supply = Fake::without_list();
    let dyn_supply = || Arc::clone(&supply) as Arc<dyn Supply>;
    list::list_catalog_with(&paths, Some(&channel), &cache, dyn_supply()).expect("a catalog");
    let Value::Object(data) = jsonio::read_json(&cache).unwrap() else { panic!("an object") };
    let stamp = data.iter().find(|(k, _)| k == "quelle").map(|(_, v)| v.clone()).unwrap();
    assert_eq!(stamp.get("scmdb").and_then(Value::as_str), Some(""), "not brought");
    assert!(list::start_catalog_with(&paths, Some(&channel), &cache, dyn_supply()).rebuild.is_none());
    change_stamp(&cache, |stamp| jsonio::map_set(stamp, "versuch", Value::str("1000")));
    assert!(list::start_catalog_with(&paths, Some(&channel), &cache, dyn_supply()).rebuild.is_some());
}

/// The pages that only read the catalog (the contract log, the backup, the
/// error report) take it from the file this version builds.
#[test]
fn the_other_readers_take_the_catalog_of_this_version() {
    let (paths, channel, cache) = case("list-readers");
    list::list_catalog_with(&paths, Some(&channel), &cache, Fake::up()).expect("a catalog");
    assert_eq!(catalog_cache::current_file(&paths), cache);
    let missions = versekit_core::mission_log::mission_blueprints(&paths);
    assert_eq!(
        missions.get("Hunt_Title").map(Vec::as_slice),
        Some(["P8-AR Rifle".to_string()].as_slice()),
        "{missions:?}"
    );
    assert!(versekit_core::export::catalog_view(&paths).blueprints.contains_key("s-38 pistol"));
    // An upgrader who has not started the list yet is read from the 4.12 file.
    fs::remove_file(&cache).unwrap();
    assert_eq!(catalog_cache::current_file(&paths), paths.app_file(catalog_cache::CACHE_FILE));
}

#[test]
fn without_an_installation_scmdb_gives_the_catalog() {
    let (paths, _, cache) = case("list-no-archive");
    let supply = Fake::up();
    let start = list::start_catalog_with(&paths, None, &cache, Arc::clone(&supply) as Arc<dyn Supply>);
    assert!(start.catalog.is_none(), "nothing of the 4.12 version to show");
    let rebuild = start.rebuild.expect("a build is due");
    let built = finished(&rebuild.spawn().expect("a thread")).expect("scmdb's catalog");
    assert_eq!(built.blueprints.len(), 1);
    assert_eq!(built.version, "4.12.0-live.13000000");
    assert!(Path::new(&cache).exists());
    // Current for a while: asked again only after the waiting time.
    let next = list::start_catalog_with(&paths, None, &cache, Arc::clone(&supply) as Arc<dyn Supply>);
    assert!(next.rebuild.is_none());
    assert_eq!(next.catalog.as_ref(), Some(&built));
    // Without an archive no patch can be seen, so after the waiting time scmdb is asked again.
    change_stamp(&cache, |stamp| jsonio::map_set(stamp, "versuch", Value::str("1000")));
    let later = list::start_catalog_with(&paths, None, &cache, Arc::clone(&supply) as Arc<dyn Supply>);
    assert!(later.rebuild.is_some(), "due again");
    assert_eq!(later.catalog.as_ref(), Some(&built), "the last catalog stands in meanwhile");
}

#[test]
fn an_archive_that_cannot_be_read_falls_back_to_scmdb() {
    let (paths, channel, cache) = case("list-archive-broken");
    patch(&channel);
    let built = list::list_catalog_with(&paths, Some(&channel), &cache, Fake::up()).expect("scmdb's catalog");
    assert_eq!(built.blueprints.len(), 1);
    // Neither the archive nor scmdb: nothing, and no crash.
    fs::remove_file(&cache).unwrap();
    assert_eq!(list::list_catalog_with(&paths, Some(&channel), &cache, Fake::down()), None);
}

#[test]
fn the_cache_file_is_read_and_built_again_when_needed() {
    let (paths, channel, cache) = case("list-cache");
    let first = list::list_catalog_with(&paths, Some(&channel), &cache, Arc::new(Offline)).expect("a catalog");
    let written = fs::read_to_string(&cache).expect("the cache file is written");

    // Read from the file, not from the archive: a changed name shows.
    fs::write(&cache, written.replace("\"P8-AR Rifle\"", "\"Aus dem Speicher\"")).unwrap();
    let from_file = list::list_catalog_with(&paths, Some(&channel), &cache, Arc::new(Offline)).unwrap();
    assert!(from_file.blueprints.iter().any(|b| b.name == "Aus dem Speicher"));

    // Broken: built again, no crash, and written anew.
    fs::write(&cache, "{kaputt").unwrap();
    let again = list::list_catalog_with(&paths, Some(&channel), &cache, Arc::new(Offline)).unwrap();
    assert_eq!(again.blueprints, first.blueprints);

    // Another layout of the cache is built again as well.
    let text = fs::read_to_string(&cache).unwrap();
    let format = format!("\"format\": {}", list::CACHE_FORMAT);
    assert!(text.contains(&format), "the layout is named in the stamp");
    assert!(
        text.contains("\"beschreibungen\": {") && text.contains("\"text\": \"item_Descbehr_rifle\""),
        "the description keys of the items are kept"
    );
    fs::write(&cache, text.replacen(&format, "\"format\": 3", 1).replace("\"P8-AR Rifle\"", "\"Alt\"")).unwrap();
    let rebuilt = list::list_catalog_with(&paths, Some(&channel), &cache, Arc::new(Offline)).unwrap();
    assert!(rebuilt.blueprints.iter().all(|b| b.name != "Alt"));
}

/// A data folder as 5.2.6 left it, on the same build of the game: the catalog cache of format 5 and the reputation
/// table of format 2, without the terms of the contracts. The old table counts until the catalog is built anew; the
/// first build after the update writes both in the new form.
#[test]
fn a_data_folder_of_5_2_6_is_built_anew_on_the_same_build_and_gets_the_terms() {
    use versekit_core::injection::reputation;
    let (paths, channel, cache) = case("list-526");
    list::list_catalog_with(&paths, Some(&channel), &cache, Arc::new(Offline)).expect("a catalog");
    let text = fs::read_to_string(&cache).unwrap();
    let format = format!("\"format\": {}", list::CACHE_FORMAT);
    fs::write(&cache, text.replacen(&format, "\"format\": 5", 1)).unwrap();
    let table_file = paths.app_file(reputation::CACHE_FILE);
    let old = r#"{"format": 2, "version": "4.12.0", "auftraege": {"old_title": [{"wer": "Alt", "was": "Standing", "wieviel": 1}]}, "regionen": {"old_title": "Stanton"}, "datenfehler": ""}"#;
    fs::write(&table_file, old).unwrap();
    assert!(!reputation::load(&paths).contracts.is_empty(), "the old table is not empty before the build");

    list::list_catalog_with(&paths, Some(&channel), &cache, Arc::new(Offline)).expect("a catalog");
    assert!(fs::read_to_string(&cache).unwrap().contains(&format), "the cache is built anew");
    let written = jsonio::read_json(&table_file).unwrap();
    assert_eq!(written.get("format"), Some(&Value::int(reputation::FORMAT)), "the table is written anew");
    let table = reputation::load(&paths);
    assert!(!table.terms.is_empty(), "the terms of the contracts are there");
    assert_eq!(table.terms_source, reputation::TERMS_FROM_GAME);
    assert!(table.contracts.iter().all(|(key, _)| key != "old_title"), "the old rows are gone");
}

/// A game folder without an archive and the archive in another folder: the list
/// reads the archive that is given from outside, and only that one.
#[test]
fn the_archive_given_from_outside_is_read_where_it_lies() {
    let (paths, channel, _) = case("list-given");
    let data = paths.app_folder();
    let game = channel.parent().unwrap().join("nachbau");
    fs::create_dir_all(&game).unwrap();
    fs::write(game.join("Game.log"), b"").unwrap();
    let game_text = game.to_string_lossy().into_owned();
    let host = Host::new(
        Flavor::native(),
        &[(legacy::ENV_HOME, &data), (versekit_core::paths::ENV_INSTALL_DIR, &game_text)],
        &data,
    );
    let paths = Paths::new(host);
    let cache = paths.app_file(CACHE_FILE);

    // Without the file given, the game folder holds no archive: nothing is built.
    assert_eq!(list::list_catalog_offline(&paths), None);
    assert!(!Path::new(&cache).exists());

    let archive = channel.join(versekit_gamedata::ARCHIVE_NAME).to_string_lossy().into_owned();
    paths.set_given_archive(&archive, versekit_core::given::GivenBy::Named("Werkzeug".into()));
    let new = list::list_catalog_offline(&paths).expect("a catalog");
    assert_eq!(new.blueprints.len(), 2, "built from the archive that was given");
    assert!(Path::new(&cache).exists());
    let start = list::start_catalog(&paths);
    // The network is not asked here, so a build may be due; the list starts with the last one of that archive.
    assert_eq!(
        start.catalog.map(|catalog| catalog.blueprints.len()),
        Some(2),
        "the cache of that archive is the start"
    );
}

#[test]
fn a_refused_write_still_shows_the_list() {
    let (paths, channel, cache) = case("list-locked");
    writeguard::deny(true);
    writeguard::allow_under(None);
    let new = list::list_catalog_with(&paths, Some(&channel), &cache, Arc::new(Offline));
    let denied = writeguard::take_denied();
    writeguard::deny(false);
    let new = new.expect("a catalog");
    assert_eq!(new.blueprints.len(), 2);
    assert!(!Path::new(&cache).exists());
    assert!(denied.iter().any(|d| d.contains(CACHE_FILE)), "{denied:?}");
}

/// The cache file after a build, with one name changed: a catalog that no
/// build of this archive would give, so that the one a test gets can be
/// told from the others. Returns it as read back.
fn marked(cache: &str) -> CatalogCache {
    let text = fs::read_to_string(cache).expect("the cache file is written");
    assert!(text.contains("\"P8-AR Rifle\""));
    fs::write(cache, text.replace("\"P8-AR Rifle\"", "\"Aus dem Speicher\"")).unwrap();
    let stored = catalog_cache::parse(&jsonio::read_json(cache).unwrap());
    assert!(stored.blueprints.iter().any(|b| b.name == "Aus dem Speicher"));
    stored
}

#[test]
fn the_start_shows_the_last_build_even_when_the_archive_cannot_be_read() {
    let (paths, channel, cache) = case("list-start-stale");
    list::list_catalog_with(&paths, Some(&channel), &cache, Arc::new(Offline)).expect("a catalog");
    let last = marked(&cache);

    let current = list::start_catalog_with(&paths, Some(&channel), &cache, Arc::new(Offline));
    assert!(current.rebuild.is_none(), "nothing is due while the stamp holds");
    assert_eq!(current.catalog.as_ref(), Some(&last));

    // After the patch nothing in the archive can be read. The start still
    // answers with the last build.
    patch(&channel);
    let stored = fs::read_to_string(&cache).unwrap();
    let start = list::start_catalog_with(&paths, Some(&channel), &cache, Arc::new(Offline));
    assert_eq!(start.catalog.as_ref(), Some(&last), "the last build stands in");
    let rebuild = start.rebuild.expect("a build is due");
    let pending = rebuild.spawn().expect("a thread");
    assert_eq!(finished(&pending), None, "only the build reads the archive, and it cannot here");
    assert_eq!(fs::read_to_string(&cache).unwrap(), stored, "the last build stays in its file");
}

/// After an update that changed the layout of the cache, the start shows the build of the older layout until the
/// new build is done (while the game runs, the archive waits): the list is never empty. A layout too old to read
/// does not stand in.
#[test]
fn the_start_after_an_update_of_the_layout_shows_the_last_build_meanwhile() {
    let (paths, channel, cache) = case("list-start-layout");
    list::list_catalog_with(&paths, Some(&channel), &cache, Arc::new(Offline)).expect("a catalog");
    let last = marked(&cache);
    let format = format!("\"format\": {}", list::CACHE_FORMAT);
    let text = fs::read_to_string(&cache).unwrap();
    fs::write(&cache, text.replacen(&format, "\"format\": 5", 1)).unwrap();
    let start = list::start_catalog_with(&paths, Some(&channel), &cache, Arc::new(Offline));
    assert_eq!(start.catalog.as_ref(), Some(&last), "the build of the older layout stands in");
    assert!(start.rebuild.is_some(), "and the new build is due");

    fs::write(&cache, text.replacen(&format, "\"format\": 2", 1)).unwrap();
    let start = list::start_catalog_with(&paths, Some(&channel), &cache, Arc::new(Offline));
    assert_eq!(start.catalog, None, "a layout too old stands in for nothing");
}

/// The blocking way, for callers that wait for the build anyway, answers
/// with the last build of the same archive when the build fails, and only
/// without one with the 4.12 cache.
#[test]
fn the_blocking_way_falls_back_to_the_last_build_then_to_the_4_12_cache() {
    let (paths, channel, cache) = case("list-blocking-fallback");
    list::list_catalog_with(&paths, Some(&channel), &cache, Arc::new(Offline)).expect("a catalog");
    let last = marked(&cache);
    with_4_12_cache(&paths, POISON);
    patch(&channel);
    let got = list::list_catalog_with(&paths, Some(&channel), &cache, Arc::new(Offline));
    assert_eq!(got.as_ref(), Some(&last), "the last build stands in before the 4.12 cache");

    fs::remove_file(&cache).unwrap();
    let got = list::list_catalog_with(&paths, Some(&channel), &cache, Arc::new(Offline));
    assert_eq!(got, catalog_cache::load(&paths), "the last resort is the 4.12 cache of an upgrader");
}

#[test]
fn the_4_12_cache_only_stands_in_until_the_first_build() {
    let (paths, channel, cache) = case("list-start-none");
    with_4_12_cache(&paths, POISON);
    let stand_in = catalog_cache::load(&paths);
    let start = list::start_catalog_with(&paths, Some(&channel), &cache, Arc::new(Offline));
    assert_eq!(start.catalog.as_ref(), stand_in.as_ref(), "shown while the first build runs");
    let rebuild = start.rebuild.expect("a build is due");
    let built = finished(&rebuild.spawn().expect("a thread")).expect("a catalog");
    assert!(built.blueprints.iter().all(|b| b.key != "only in 4.12"), "but never part of the result");
    assert!(Path::new(&cache).exists(), "the build keeps its result");

    // A build of another archive is no stand-in: its catalog may be of another game.
    marked(&cache);
    let other = common::channel("list-start-other");
    let start = list::start_catalog_with(&paths, Some(&other), &cache, Arc::new(Offline));
    assert_eq!(start.catalog.as_ref(), stand_in.as_ref());
    assert!(start.rebuild.is_some());
}

#[test]
fn the_start_without_an_archive_or_any_catalog_has_nothing_to_wait_for_but_scmdb() {
    let (paths, _, cache) = case("list-start-without");
    // Offline and nothing at hand: a build is due (it will find nothing), nothing is shown.
    let start = list::start_catalog_with(&paths, None, &cache, Arc::new(Offline));
    assert!(start.catalog.is_none());
    assert!(finished(&start.rebuild.expect("a build").spawn().expect("a thread")).is_none());
}

#[test]
fn a_build_in_the_background_wakes_the_window_when_it_is_done() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex, mpsc};

    let (go, wait) = mpsc::channel::<()>();
    let pending = Arc::new(Mutex::new(
        list::Pending::spawn(move || {
            wait.recv().ok()?;
            Some(CatalogCache::default())
        })
        .expect("a thread"),
    ));
    let woken = Arc::new(AtomicUsize::new(0));
    // What the window finds when the wake-up reaches it: the result must be there already.
    let found = Arc::new(Mutex::new(None));
    assert!(!pending.lock().unwrap().has_wake());
    let (counter, looking, seen) = (Arc::clone(&woken), Arc::clone(&pending), Arc::clone(&found));
    pending.lock().unwrap().set_wake(move || {
        counter.fetch_add(1, Ordering::SeqCst);
        let progress = looking.lock().unwrap().poll();
        *seen.lock().unwrap() = Some(progress);
    });
    assert!(pending.lock().unwrap().has_wake());
    assert_eq!(pending.lock().unwrap().poll(), list::Progress::Waiting, "the build holds the thread, not the caller");
    assert_eq!(woken.load(Ordering::SeqCst), 0);
    go.send(()).unwrap();
    for _ in 0..1000 {
        if woken.load(Ordering::SeqCst) > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert_eq!(woken.load(Ordering::SeqCst), 1, "woken once");
    assert_eq!(
        *found.lock().unwrap(),
        Some(list::Progress::Done(Some(CatalogCache::default()))),
        "the result is there when the wake-up comes"
    );

    // A build that dies ends the waiting as well, and wakes the window.
    let (go, wait) = mpsc::channel::<()>();
    let dying = list::Pending::spawn(move || {
        wait.recv().ok()?;
        panic!("a broken archive")
    })
    .expect("a thread");
    let woken = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&woken);
    dying.set_wake(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    go.send(()).unwrap();
    assert_eq!(finished(&dying), None);
    for _ in 0..1000 {
        if woken.load(Ordering::SeqCst) > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(woken.load(Ordering::SeqCst), 1, "a dead build wakes the window too");
}

#[test]
fn a_build_that_ended_before_the_wake_was_set_is_found_by_asking() {
    // The build says when it has returned; then only the hand-over in its thread is left. The
    // wait of 500 ms after it is still an assumption about the computer, though a far smaller one
    // than the fixed 200 ms before, which lost the race under a full test run.
    let (returned, has_returned) = std::sync::mpsc::channel();
    let pending = list::Pending::spawn(move || {
        let _ = returned.send(());
        Some(CatalogCache::default())
    })
    .expect("a thread");
    has_returned.recv_timeout(std::time::Duration::from_secs(30)).expect("the build ran");
    std::thread::sleep(std::time::Duration::from_millis(500));
    // A panic in the build thread would be swallowed there: the call is noted instead.
    let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let note = std::sync::Arc::clone(&called);
    pending.set_wake(move || note.store(true, std::sync::atomic::Ordering::SeqCst));
    assert_eq!(pending.poll(), list::Progress::Done(Some(CatalogCache::default())));
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(!called.load(std::sync::atomic::Ordering::SeqCst), "a build that is over calls nobody");
}

#[test]
fn a_build_in_the_background_keeps_the_write_lock_of_its_caller() {
    let (_, _, cache) = case("list-start-lock");
    writeguard::deny(true);
    writeguard::allow_under(None);
    let target = cache.clone();
    let pending = list::Pending::spawn(move || {
        // The build writes its cache like this; the lock refuses it.
        let refused = versekit_core::jsonio::write_text_atomic(&target, "x", versekit_core::jsonio::Newline::Keep)
            .is_err_and(|e| writeguard::refused(&e));
        refused.then(CatalogCache::default)
    })
    .expect("a thread");
    writeguard::deny(false);
    assert_eq!(finished(&pending), Some(CatalogCache::default()), "the thread was locked like its caller");
    assert!(!Path::new(&cache).exists());
    // What the lock refused in the thread is handed back to the caller.
    let denied = pending.take_denied();
    assert!(denied.iter().any(|d| d.contains(CACHE_FILE)), "{denied:?}");
    assert!(pending.take_denied().is_empty());
}
