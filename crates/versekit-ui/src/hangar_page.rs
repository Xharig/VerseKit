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

//! The hangar page shows the own ships from `hangar.json`: import an export
//! file, enter a ship by hand from the UEX list, and per ship remove it, mark
//! it ready and choose whether it can be melted. The hangar delivery
//! settings hold the receiver for the browser extension, with its
//! pairing code. The receiver runs on its own thread, only with own data.
//!
//! Every change is saved at once, merged with what another place wrote
//! meanwhile (`hangar::save_held`). Slots come from the stored erkul file;
//! an import, a ship by hand, a wish and entering the page ask
//! [`crate::slot_sync`] for the missing ones.

use versekit_core::language::SpawnWithLanguage as _;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::SystemTime;

use egui::{RichText, Role, Ui, WidgetInfo};
use versekit_core::basetool::orgunits::label as unit_label;
use versekit_core::basetool::scopes::SETTING_SHIPS;
use versekit_core::cart::Sources as CartSources;
use versekit_core::equipment;
use versekit_core::fleet_analysis;
use versekit_core::hangar::{self, CATALOG, Held, Imported, Rejected, Saved};
use versekit_core::hangar_formats::{self, ReadFail};
use versekit_core::hangar_inbox::{self as inbox, Event, Files, Gate, Inbox};
use versekit_core::jsonio::{self, Map, Value, map_set};
use versekit_core::language::{Arg, t, tf};
use versekit_core::paths::Paths;
use versekit_core::ships::{self, ErkulShips, ShipId, UexShips};
use versekit_core::theme::{Color, Scheme};
use versekit_core::writeguard;

use crate::equipment_box::{self, EquipmentState, Extra};
use crate::fleet_analysis_view::FleetView;
use crate::layout::GAP;
use crate::ship_names::ShipNames;
use crate::shopping_page::{PriceLookup, Stores};
use crate::slot_sync::{SlotSync, Trigger};
use crate::widgets::{self, ButtonKind};
use crate::{color, file_dialog, settings};

/// How long the copy button shows its confirmation.
const COPIED_SECONDS: f64 = 2.0;
/// How often the page looks whether `hangar.json` changed.
const LOOK_SECONDS: f64 = 1.0;
/// From this width the direct steps and the pairing stand side by side.
const PAIRING_SIDE_BY_SIDE: f32 = 760.0;

/// The room between two ship cards.
const CARD_GAP: f32 = GAP;

/// The receiver as the window sees it.
struct Receiver {
    inbox: Arc<Inbox>,
    port: u16,
    token: String,
    enabled: bool,
    /// Bound and answering; false when the port was taken.
    listening: bool,
    repaint: Arc<OnceLock<egui::Context>>,
}

/// How the message line under the enter button is colored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tone {
    Good,
    Bad,
    /// A hint, nothing went wrong.
    Quiet,
}

/// What the hangar page and its settings hold.
pub struct HangarState {
    receiver: Option<Receiver>,
    file: Option<String>,
    held: Held,
    uex: UexShips,
    erkul: ErkulShips,
    /// `UexShips::choice_names`, for the field to enter a ship: each name once.
    all_names: Vec<String>,
    pub search: String,
    seen: (u64, Option<SystemTime>),
    looked_at: f64,
    copied_at: Option<f64>,
    /// Whether the pairing settings on the hangar page are unfolded.
    pairing_open: bool,
    /// The part with import and entering by hand (the button "Schiffe holen oder eintragen" in the head card), once
    /// clicked; until then it is open only while the hangar is empty.
    adding_open: Option<bool>,
    import_path: String,
    hand_name: String,
    real_money: bool,
    lti: bool,
    message: Option<(String, Tone)>,
    /// Slots per ship, looked up once (`slot_key` → what the files say).
    slots: HashMap<[String; 4], SlotInfo>,
    /// The height each row had when it was last drawn, by ship.
    heights: HashMap<usize, f32>,
    /// The file dialog while it is open.
    dialog: Option<file_dialog::Pending>,
    /// The in-game names of the ships.
    names: ShipNames,
    /// The search the list was last sorted for.
    ordered_for: String,
    /// The fleet analysis over the list.
    fleet: FleetView,
    /// The data folder, for the order of the stops and the stores.
    paths: Option<Paths>,
    /// What the boxes "Loadout & cart" read: slots, parts, shops, prices.
    stores: Arc<Stores>,
    /// What the boxes remember while the program runs.
    equipment: EquipmentState,
    /// The lookup of the shop prices of the parts in the open boxes.
    lookup: PriceLookup,
    /// The items that can be built and the traits of the blueprint catalog, for the choice of parts.
    extra: Arc<Extra>,
    extra_set: bool,
    /// The wish list: the name being entered and the message under it.
    wish_name: String,
    wish_message: Option<(String, Tone)>,
    /// Set when the page is to scroll to the section "Planned" the next time it is drawn.
    scroll_to_planned: bool,
    /// The sync, for the choice of the unit of a ship.
    link: Option<crate::stock_share::Link>,
    /// The units a ship may be given to (id and name), while the choice is on offer.
    unit_menu: Option<Vec<(String, String)>>,
    /// The fetches of the slots from erkul.
    sync: SlotSync,
    /// The frame the page was last drawn in: a gap means the page was entered again.
    last_frame: Option<u64>,
    /// The ship whose parts the crafting page is to show, until the window takes it.
    craft_jump: Option<String>,
    /// The dialog to save or open a file of share codes, while it is open.
    share_dialog: Option<ShareDialog>,
    /// The notice after a ship changed between the own ships and the wish list, or went on ready by itself because
    /// its list was ticked off: text, icon, since when.
    moved: Option<(String, &'static str, Option<f64>)>,
    /// The question after a ship was put on ready with parts of its list still open.
    ready_question: Option<ReadyQuestion>,
}

/// The ship a question about its list is about: the place in the hangar, checked against name and maker before
/// anything is ticked, because the list can change under an open question.
struct ReadyQuestion {
    index: usize,
    name: String,
    maker: String,
}

/// A dialog of the share codes and the box it belongs to.
enum ShareDialog {
    /// Where to save these codes.
    Save { key: String, text: String, pending: file_dialog::Pending },
    /// Which file to read.
    Open { key: String, pending: file_dialog::Pending },
}

/// The most a file of share codes is read of.
const SHARE_FILE_LIMIT: u64 = 1024 * 1024;

const SAMPLE: &str = r#"{"format": 1, "schiffe": [
 {"name": "Carrack", "hersteller": "Anvil Aerospace", "herkunft": "pledge", "hkurz": "ANVL", "kurz": "ANVL_Carrack", "lti": true, "belegung": {}, "schmelzbar": false},
 {"name": "C8 Pisces", "hersteller": "Anvil Aerospace", "hkurz": "ANVL", "herkunft": "katalog", "paket": "Carrack", "belegung": {}},
 {"name": "Cutlass Black", "hersteller": "Drake Interplanetary", "herkunft": "pledge", "versicherung": 120, "belegung": {"a": {"ref": "x", "name": "X", "weg": "kaufen"}}, "einsatzbereit": true},
 {"name": "Gladius", "hersteller": "Aegis Dynamics", "herkunft": "pledge", "lti": true, "paket": "Carrack", "belegung": {"a": {"ref": "x", "erledigt": true}}, "schmelz_wahl": "ja"},
 {"name": "Avenger Titan", "hersteller": "", "herkunft": "ingame", "belegung": {}},
 {"name": "Freelancer MAX", "hersteller": "MISC", "herkunft": "pledge", "versicherung": 6, "belegung": {}},
 {"name": "Polaris", "hersteller": "RSI", "herkunft": "liste", "quelle": "Fleetyards", "belegung": {}}]}"#;

/// Lines of a language file for the sample.
const SAMPLE_NAMES: [&str; 4] = [
    "vehicle_NameANVL_Carrack=Carrack",
    "vehicle_NameAEGS_Gladius=Gladius",
    "vehicle_NameDRAK_Cutlass_Black=Cutlass Black",
    "vehicle_NameAEGS_Avenger_Titan=Avenger Titan",
];

impl Default for HangarState {
    fn default() -> Self {
        let data = match jsonio::loads(SAMPLE) {
            Ok(Value::Object(map)) => map,
            _ => hangar::empty(),
        };
        HangarState {
            receiver: None,
            file: None,
            held: Held::detached(data),
            uex: UexShips::default(),
            erkul: ErkulShips::default(),
            all_names: Vec::new(),
            search: String::new(),
            seen: (0, None),
            looked_at: f64::NEG_INFINITY,
            copied_at: None,
            pairing_open: false,
            adding_open: None,
            import_path: String::new(),
            hand_name: String::new(),
            real_money: false,
            lti: false,
            message: None,
            slots: HashMap::new(),
            heights: HashMap::new(),
            dialog: None,
            names: ShipNames::detached(versekit_core::asop::read_keys(&SAMPLE_NAMES)),
            ordered_for: String::new(),
            fleet: FleetView::default(),
            paths: None,
            stores: Arc::new(Stores::empty()),
            equipment: EquipmentState::default(),
            lookup: PriceLookup::default(),
            extra: Arc::new(Extra::default()),
            extra_set: false,
            wish_name: String::new(),
            wish_message: None,
            scroll_to_planned: false,
            link: None,
            unit_menu: None,
            sync: crate::slot_sync::global().clone(),
            last_frame: None,
            craft_jump: None,
            share_dialog: None,
            moved: None,
            ready_question: None,
        }
    }
}

fn modified(file: &str) -> Option<SystemTime> {
    std::fs::metadata(file).and_then(|m| m.modified()).ok()
}

/// The texts of the settings of the pairing, as the menu filter finds them on the hangar page.
pub const PAIRING_SETTINGS: [&str; 3] = ["s_hgl_an", "s_hgl_code", "s_hgl_erneuern"];

impl HangarState {
    /// Unfolds the pairing settings: the way from the menu filter to a setting that lies in
    /// the fold "Kopplung verwalten".
    pub fn point_to_pairing(&mut self) {
        // The pairing sits in the part "Schiffe holen oder eintragen", which is closed once the
        // hangar has ships.
        self.adding_open = Some(true);
        self.pairing_open = true;
    }

    /// Whether `setting` (a shown name) is one of the pairing settings.
    pub fn is_pairing_setting(setting: &str) -> bool {
        PAIRING_SETTINGS.iter().any(|key| t(key) == setting)
    }

    /// The ship whose parts the crafting page is to show, once: the button of a ship card asked for it.
    pub fn take_craft_jump(&mut self) -> Option<String> {
        self.craft_jump.take()
    }

    /// Whether the pairing settings are unfolded.
    pub fn pairing_unfolded(&self) -> bool {
        self.pairing_open
    }

    /// The in-game names of the ships, which write after a pause.
    pub(crate) fn ship_names(&mut self) -> &mut ShipNames {
        &mut self.names
    }

    /// With own data: the ships of `hangar.json`, the UEX list and the
    /// stored slots of the same folder.
    pub fn with_file(paths: &Paths) -> HangarState {
        let file = paths.app_file(hangar::FILE);
        let uex = UexShips::load(&paths.app_file(ships::UEX_FILE));
        let erkul = ErkulShips::load(&paths.app_file(ships::ERKUL_FILE));
        let all_names = uex.choice_names();
        let names = ShipNames::with_paths(paths);
        let mut state = HangarState {
            file: Some(file),
            uex,
            erkul,
            all_names,
            names,
            paths: Some(Paths::new(paths.host.clone())),
            stores: Arc::new(Stores::load(paths)),
            ..HangarState::default()
        };
        state.reload();
        state
    }

    /// Connects the page to the sync: the unit of a ship can be chosen.
    pub fn connect(&mut self, link: Option<crate::stock_share::Link>) {
        self.link = link;
    }

    /// The choice of the unit of ship `i`; the next sync sends it (the Basetool's own wins where both changed).
    fn set_unit(&mut self, i: usize, pick: UnitPick) {
        if let Some(entry) = hangar::ship_mut(&mut self.held.data, i) {
            match pick {
                UnitPick::Back => entry.retain(|(k, _)| k != "einheit"),
                UnitPick::Set(id) => map_set(entry, "einheit", Value::str(id)),
            }
            self.save();
        }
    }

    /// Whether the hangar is shared over the Basetool (connection there, area switched on).
    fn basetool_ships_on(&self) -> bool {
        self.link.as_ref().is_some_and(|link| link.sync.area_visible(SETTING_SHIPS))
    }

    /// The units to choose from: only with the hangar area on and more than one unit (whoever is in
    /// one gets it from the Basetool by itself).
    fn unit_choices(&self) -> Option<Vec<(String, String)>> {
        let link = self.link.as_ref()?;
        if !link.sync.area_visible(SETTING_SHIPS) {
            return None;
        }
        let units = link.units.get().1?;
        (units.len() > 1).then(|| units.iter().map(|u| (u.id.clone(), unit_label(u))).collect())
    }

    /// The stores of the boxes "Loadout & cart" (slots, parts, shops, prices).
    pub(crate) fn stores(&self) -> Arc<Stores> {
        self.stores.clone()
    }

    /// Looks up the shop prices the open boxes still lack, in the background. When the lookup stored something
    /// the stores are read again at the next look at the files.
    fn look_up_prices(&mut self, ui: &Ui) {
        let mut missing = std::mem::take(&mut self.equipment.missing);
        missing.sort();
        missing.dedup();
        self.lookup.ask(ui, self.paths.as_ref(), missing);
    }

    /// Whether the data for the choice of parts is still to be put in (`set_extra`).
    pub fn needs_extra(&self) -> bool {
        !self.extra_set
    }

    /// The items that can be built and the traits of the blueprint catalog, once they are there.
    pub fn set_extra(&mut self, extra: Extra) {
        self.extra = Arc::new(extra);
        self.extra_set = true;
    }

    /// Start the receiver on its fixed port (chosen and stored the first
    /// time, like the token).
    pub fn start_receiver(&mut self, paths: &Paths) {
        if self.receiver.is_some() {
            return;
        }
        let Some(file) = self.file.clone() else { return };
        let enabled = paths.setting_bool(inbox::SETTING_ON, true);
        let token = match paths.setting(inbox::SETTING_TOKEN).filter(|t| inbox::valid_token(t)) {
            Some(token) => token,
            None => {
                let token = inbox::new_token().unwrap_or_default();
                paths.set_setting(inbox::SETTING_TOKEN, Value::str(token.clone()));
                token
            }
        };
        let stored = paths.setting_int(inbox::SETTING_PORT, 0, None, None);
        let port = match u16::try_from(stored).ok().filter(|p| *p > 0) {
            Some(port) => port,
            None => {
                let port = inbox::pick_port().unwrap_or(inbox::DEFAULT_PORT);
                paths.set_setting(inbox::SETTING_PORT, Value::int(i64::from(port)));
                port
            }
        };
        let shared = Inbox::new(Gate { enabled, token: token.clone(), ready: true });
        let repaint: Arc<OnceLock<egui::Context>> = Arc::new(OnceLock::new());
        let listening = match inbox::bind(port) {
            Ok(listener) => {
                let files = Files { hangar: file, ship_catalog: paths.app_file(hangar::SHIP_CATALOG_FILE) };
                let (thread_inbox, thread_repaint) = (Arc::clone(&shared), Arc::clone(&repaint));
                // The write lock of this thread (the preview's) holds in the receiver too.
                let guard = writeguard::state();
                std::thread::Builder::new()
                    .name("hangar-einlieferung".into())
                    .spawn_with_language(move || {
                        inbox::serve(listener, thread_inbox, files, guard, || {
                            if let Some(ctx) = thread_repaint.get() {
                                ctx.request_repaint();
                            }
                        })
                    })
                    .is_ok()
            }
            Err(_) => false,
        };
        self.receiver = Some(Receiver { inbox: shared, port, token, enabled, listening, repaint });
    }

    /// The settings this part keeps in the settings file.
    pub fn setting_values(&self) -> Vec<(&'static str, Value)> {
        match &self.receiver {
            Some(r) => vec![
                (inbox::SETTING_ON, Value::Bool(r.enabled)),
                (inbox::SETTING_TOKEN, Value::str(r.token.clone())),
                (inbox::SETTING_PORT, Value::int(i64::from(r.port))),
            ],
            None => Vec::new(),
        }
    }

    /// The pairing code, when the receiver runs.
    pub fn pairing_code(&self) -> Option<String> {
        self.receiver.as_ref().map(|r| inbox::pairing_code(r.port, &r.token))
    }

    /// `fleet.load`: duplicates are merged and saved.
    fn reload(&mut self) {
        let Some(file) = &self.file else { return };
        self.held = {
            let _lock = hangar::lock();
            hangar::load_held(file)
        };
        // Entries of a plan without their slot are marked, not counted; the mark is saved with the next change.
        self.stores.erkul.tidy_plans(&mut self.held.data);
        self.seen = (self.receiver.as_ref().map_or(0, |r| r.inbox.deliveries()), modified(file));
        self.heights.clear();
        self.names.reorder();
    }

    /// Looks up the slots of every ship not looked up yet.
    fn fill_slots(&mut self) {
        let missing: Vec<([String; 4], SlotInfo)> = hangar::ships(&self.held.data)
            .into_iter()
            .filter(|entry| !self.slots.contains_key(&slot_key(entry)))
            .map(|entry| (slot_key(entry), slot_info(self, entry)))
            .collect();
        self.slots.extend(missing);
    }

    /// Reload when a delivery came in or another place changed the file.
    fn follow(&mut self, ui: &Ui) {
        if let Some(r) = &self.receiver {
            let _ = r.repaint.get_or_init(|| ui.ctx().clone());
        }
        let Some(file) = &self.file else { return };
        let now = ui.input(|i| i.time);
        let deliveries = self.receiver.as_ref().map_or(0, |r| r.inbox.deliveries());
        if deliveries != self.seen.0 || now - self.looked_at >= LOOK_SECONDS {
            self.looked_at = now;
            // The stored slots, parts and prices: read again where a file changed.
            if let Some(paths) = &self.paths
                && self.stores.changed(paths)
            {
                self.stores = Arc::new(Stores::load(paths));
                // Slots fetched in the background show on the cards too.
                self.erkul = ErkulShips::load(&paths.app_file(ships::ERKUL_FILE));
                self.slots.clear();
                self.stores.erkul.tidy_plans(&mut self.held.data);
            }
            // The order of the stops is kept by the shopping list too: read again, so a move there shows here.
            self.equipment.order_loaded = false;
            let touched = modified(file) != self.seen.1;
            if deliveries != self.seen.0 || (touched && hangar::changed_elsewhere(file, &self.held)) {
                self.reload();
            } else if touched {
                self.seen.1 = modified(file);
            }
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(LOOK_SECONDS));
    }

    /// `fleet.save`; without own data the change stays in the window. A
    /// failure or a file set aside replaces the message line; after a merge
    /// the page reloads, as it holds less than the file then.
    fn save(&mut self) -> Saved {
        let Some(file) = self.file.clone() else { return Saved::Written };
        let saved = {
            let _lock = hangar::lock();
            hangar::save_held(&file, &mut self.held)
        };
        self.seen.1 = modified(&file);
        match saved {
            Saved::Failed => self.message = Some((t("s_hgl_nicht_gespeichert"), Tone::Bad)),
            Saved::SetAside => self.message = Some((t("s_hgl_beiseite"), Tone::Bad)),
            Saved::Merged => self.reload(),
            Saved::Written => {}
        }
        saved
    }

    /// `hangar.bak.json` before something is removed; false (with the
    /// message) when the copy failed.
    fn backup(&mut self) -> bool {
        let Some(file) = &self.file else { return true };
        let made = hangar::backup(file);
        if !made {
            self.message = Some((t("s_hgl_keine_sicherung"), Tone::Bad));
        }
        made
    }

    /// The import of `pages._hangar.importieren`, for a chosen or dropped file.
    fn import(&mut self, path: &str) {
        let entries = match hangar_formats::read_file(path.trim(), self.uex.names()) {
            Err(ReadFail::Rejected(Rejected::Version)) => Err(t("s_hgl_import_version")),
            Err(ReadFail::Rejected(Rejected::Transfer)) => Err(t("s_hg_import_uebergabe")),
            Err(ReadFail::Error(_)) => Err(t("s_hg_import_fehler")),
            Ok(entries) if entries.is_empty() => Err(t("s_hg_import_leer")),
            Ok(entries) => Ok(entries),
        };
        let entries = match entries {
            Ok(entries) => entries,
            Err(text) => {
                self.message = Some((text, Tone::Bad));
                return;
            }
        };
        let before = self.held.data.clone();
        let done = hangar::import_entries(&mut self.held.data, &entries);
        if !done.removed.is_empty() && !self.backup() {
            self.held.data = before;
            return;
        }
        let text = import_message(&done, &hangar::approximations(&entries));
        self.message = Some((text.clone(), Tone::Good));
        // A failed save keeps its own red line; a file set aside adds to the import line.
        if self.save() == Saved::SetAside {
            self.message = Some((format!("{text} {}", t("s_hgl_beiseite")), Tone::Bad));
        }
        self.ask_slots(Trigger::Import);
    }

    /// Asks for the slots of the ships, quietly: the cards show them once the store has them.
    fn ask_slots(&self, why: Trigger) {
        if let Some(paths) = &self.paths {
            self.sync.request(paths, why);
        }
    }

    /// `pages._hangar.von_hand`: only a name the UEX list knows, stored as
    /// UEX writes it.
    fn enter_by_hand(&mut self) {
        let Some(name) = self.uex.name_of(self.hand_name.trim()).map(str::to_string) else {
            self.message = Some((t("s_hg_kein_name"), Tone::Bad));
            return;
        };
        if hangar::add_by_hand(&mut self.held.data, &name, self.real_money, self.lti) {
            self.message = Some((tf("s_hg_getragen", &[("name", Arg::from(name.as_str()))]), Tone::Good));
            if self.save().ok() {
                self.hand_name.clear();
            }
            self.ask_slots(Trigger::ByHand);
        } else {
            self.message = Some((t("s_hg_schon_da"), Tone::Bad));
        }
    }

    /// `fleet.remove` from a row, after a copy of the file.
    fn remove(&mut self, name: &str, maker: &str) {
        if !self.backup() {
            return;
        }
        if hangar::remove(&mut self.held.data, name, maker) {
            self.message = None;
            self.save();
        }
    }

    /// The answer of the file dialog, once it was closed: the file is
    /// imported; without a file a quiet hint names the other two ways.
    fn take_dialog(&mut self) {
        let Some(answer) = self.dialog.as_ref().and_then(file_dialog::Pending::take) else { return };
        self.dialog = None;
        match answer {
            Some(path) => {
                self.import_path = path.clone();
                self.import(&path);
            }
            None => self.message = Some((t("s_hgl_keine_datei"), Tone::Quiet)),
        }
    }

    /// A file dropped onto the window is imported.
    fn take_dropped(&mut self, ui: &Ui) {
        let dropped: Vec<String> =
            ui.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_string_lossy().into_owned()).collect());
        if let Some(path) = dropped.first() {
            self.import_path = path.clone();
            self.import(path);
        }
    }
}

// ------------------------------------------------------------ messages

/// The line after an import: taken, removed, only roughly matched.
fn import_message(imported: &Imported, approximations: &[String]) -> String {
    let mut text = tf("s_hg_import_ok", &[("neu", Arg::from(imported.new)), ("alt", Arg::from(imported.existing))]);
    if !imported.removed.is_empty() {
        text.push(' ');
        text.push_str(&tf(
            "s_hg_import_weg",
            &[("anzahl", Arg::from(imported.removed.len())), ("namen", Arg::from(imported.removed.join(", ")))],
        ));
    }
    if !approximations.is_empty() {
        text.push(' ');
        text.push_str(&tf("s_hg_import_naeherung", &[("namen", Arg::from(approximations.join(", ")))]));
    }
    text
}

/// The line about the last request, if there was one.
fn last_event_line(receiver: &Receiver) -> Option<(String, bool)> {
    let line = match receiver.inbox.last()? {
        Event::Delivered { at, delivered } => {
            let n = Arg::from(delivered.ships);
            let mut text = if delivered.unchanged {
                tf("s_hgl_unveraendert", &[("zeit", Arg::from(at.as_str())), ("n", n)])
            } else {
                tf("s_hgl_empfangen", &[("zeit", Arg::from(at.as_str())), ("n", n)])
            };
            text.push(' ');
            text.push_str(&import_message(&delivered.imported, &delivered.approximations));
            (text, true)
        }
        // The extension checks the connection with an empty object: refused, but fine.
        Event::Refused { at, status: 400, code } if code == "schema" => {
            (tf("s_hgl_probe", &[("zeit", Arg::from(at.as_str()))]), true)
        }
        Event::Refused { at, status, code } => {
            let reason = match code.as_str() {
                "token" => t("s_hgl_grund_token"),
                "hangarzugang_aus" => t("s_hgl_grund_aus"),
                "uebergabe" | "version" | "schiffe" | "leer" | "json" => t("s_hgl_grund_datei"),
                _ => tf("s_hgl_grund_sonst", &[("status", Arg::from(status as usize))]),
            };
            (tf("s_hgl_abgelehnt", &[("zeit", Arg::from(at.as_str())), ("grund", Arg::from(reason))]), false)
        }
    };
    Some(line)
}

/// A long text without spaces that still wraps (the pairing code).
fn wrapping_code(ui: &mut Ui, scheme: &Scheme, code: &str) {
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let mut job = egui::text::LayoutJob::simple(code.to_string(), font, color(scheme.fg), ui.available_width());
    job.wrap.break_anywhere = true;
    ui.label(job);
}

// ------------------------------------------------------------ settings

/// Whether the receiver listens, is switched off or found its port taken.
fn status_line(receiver: &Receiver) -> String {
    let port = Arg::from(receiver.port as usize);
    if !receiver.listening {
        tf("s_hgl_belegt", &[("port", port)])
    } else if receiver.enabled {
        tf("s_hgl_bereit", &[("port", port)])
    } else {
        t("s_hgl_aus")
    }
}

/// The pairing at a glance on the hangar page: its state and a copy button,
/// with the full settings folded away so the code is not always in sight.
fn pairing_box(ui: &mut Ui, scheme: &Scheme, state: &mut HangarState) {
    let Some((status, code)) = state.receiver.as_ref().map(|r| (status_line(r), inbox::pairing_code(r.port, &r.token)))
    else {
        small(ui, &t("s_hgl_ohne_daten"), scheme.sub);
        // A jump from the menu filter to a pairing setting still arrives somewhere visible.
        if state.pairing_open {
            show_settings(ui, scheme, state);
        }
        return;
    };
    ui.label(RichText::new(t("s_hgl_code")).strong());
    small(ui, &status, scheme.sub);
    let now = ui.input(|i| i.time);
    let copied = state.copied_at.is_some_and(|at| now - at < COPIED_SECONDS);
    if copied {
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(COPIED_SECONDS));
    }
    let word = if copied { t("s_hgl_kopiert") } else { t("s_hgl_kopieren") };
    if ui.add(widgets::Button::new(&word, ButtonKind::Strong, scheme).icon("copy")).clicked() {
        ui.ctx().copy_text(code);
        state.copied_at = Some(now);
    }
    ui.add_space(4.0);
    let open = state.pairing_open;
    let shown = widgets::collapsing(&t("s_hgl_verwalten"))
        .id_salt("hangar-kopplung")
        .open(Some(open))
        .show(ui, |ui| show_settings(ui, scheme, state));
    if shown.header_response.clicked() {
        state.pairing_open = !open;
    }
}

/// The hangar delivery settings.
pub fn show_settings(ui: &mut Ui, scheme: &Scheme, state: &mut HangarState) {
    let Some(receiver) = &mut state.receiver else {
        widgets::status_box(ui, scheme, scheme.sub, "info", &t("hf_hangar_einlieferung"), &t("s_hgl_ohne_daten"));
        return;
    };
    let _ = receiver.repaint.get_or_init(|| ui.ctx().clone());
    let now = ui.input(|i| i.time);
    let copied = state.copied_at.is_some_and(|at| now - at < COPIED_SECONDS);
    if state.copied_at.is_some() && copied {
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(COPIED_SECONDS));
    }
    let group = t("s_hgl_gruppe");
    let mut copy_clicked = false;
    let mut renew_clicked = false;
    let mut switched = false;
    settings::settings_page(ui, scheme, "hangar-einlieferung", &[(group.as_str(), 3)], &mut |ui, _, r| match r {
        0 => {
            let name = t("s_hgl_an");
            settings::setting_row(ui, scheme, &name, &t("s_hgl_an_hilfe"), false, |row| {
                let switch = row.add(widgets::Toggle::new(&mut receiver.enabled, scheme));
                let (enabled, on) = (switch.enabled(), receiver.enabled);
                switch.widget_info(|| WidgetInfo::selected(Role::CheckBox, enabled, on, &name));
                switched = switch.changed();
            });
        }
        1 => {
            settings::setting_row(ui, scheme, &t("s_hgl_code"), &t("s_hgl_code_hilfe"), false, |row| {
                let word = if copied { t("s_hgl_kopiert") } else { t("s_hgl_kopieren") };
                copy_clicked = row.add(widgets::Button::new(&word, ButtonKind::Strong, scheme).icon("copy")).clicked();
            });
            wrapping_code(ui, scheme, &inbox::pairing_code(receiver.port, &receiver.token));
            ui.add_space(6.0);
        }
        _ => {
            let status = status_line(receiver);
            let last = last_event_line(receiver).map(|(text, _)| text).unwrap_or_default();
            settings::setting_row(ui, scheme, &status, &last, false, |row| {
                let renew = row.add(widgets::Button::new(&t("s_hgl_erneuern"), ButtonKind::Normal, scheme));
                renew_clicked = renew.on_hover_text(t("s_hgl_erneuern_hilfe")).clicked();
            });
        }
    });
    if copy_clicked {
        ui.ctx().copy_text(inbox::pairing_code(receiver.port, &receiver.token));
        state.copied_at = Some(now);
    }
    if renew_clicked && let Some(token) = inbox::new_token() {
        receiver.token = token;
        state.copied_at = None;
    }
    if switched || renew_clicked {
        receiver.inbox.set_gate(Gate { enabled: receiver.enabled, token: receiver.token.clone(), ready: true });
    }
}

// ------------------------------------------------------------ the page

/// `_hangar_matches`: every word on its own in name, maker and both codes.
fn matches(entry: &Map, search: &str) -> bool {
    let words: Vec<String> = search.split_whitespace().map(hangar::slim).filter(|w| !w.is_empty()).collect();
    if words.is_empty() {
        return true;
    }
    let haystack =
        hangar::slim(&["name", "hersteller", "hkurz", "kurz"].map(|k| hangar::field_text(entry, k)).join(" "));
    words.iter().all(|w| haystack.contains(w.as_str()))
}

/// The details line of a ship: origin, insurance, package.
fn details(ships: &[&Map], index: usize) -> String {
    let entry = ships[index];
    let origin = match hangar::field_text(entry, "herkunft") {
        hangar::PLEDGE => t("s_hg_pledge"),
        hangar::BASETOOL => t("s_hg_basetool"),
        hangar::LIST => {
            let source = hangar::field_text(entry, "quelle");
            tf("s_hg_liste", &[("0", Arg::from(if source.is_empty() { "?" } else { source }))])
        }
        CATALOG => {
            let carrier = hangar::field_text(entry, "paket");
            tf("s_hg_katalog", &[("schiff", Arg::from(if carrier.is_empty() { "?" } else { carrier }))])
        }
        _ => t("s_hg_ingame"),
    };
    let mut parts = vec![origin];
    if hangar::field_true(entry, "lti") {
        parts.push(t("s_hg_lti"));
    } else {
        let months = hangar::field_int(entry, "versicherung");
        if months >= 12 && months % 12 == 0 {
            parts.push(tf("s_hg_vers_jahre", &[("n", Arg::from((months / 12) as usize))]));
        } else if months > 0 {
            parts.push(tf("s_hg_vers_monate", &[("n", Arg::from(months as usize))]));
        }
    }
    let carrier = hangar::bundled_with(ships, index);
    if !carrier.is_empty() && hangar::field_text(entry, "herkunft") != CATALOG {
        parts.push(tf("s_hg_beilage", &[("schiff", Arg::from(carrier))]));
    }
    parts.join("  ·  ")
}

/// The slots part of the details line (three states, `_hangar_row`):
/// erkul knows the ship, UEX calls it a concept, or nothing is known.
fn slots_part(state: &HangarState, entry: &Map) -> (String, bool, bool) {
    let found = state.slots.get(&slot_key(entry)).copied().unwrap_or_else(|| slot_info(state, entry));
    match found {
        SlotInfo { count: Some(n), .. } => (tf("s_hg_plaetze", &[("n", Arg::from(n.max(0) as usize))]), false, true),
        SlotInfo { concept: true, .. } => (t("s_hg_konzept"), true, false),
        _ => (t("s_hg_ohne_daten"), false, false),
    }
}

/// What the stored files say about a ship's slots.
#[derive(Clone, Copy, Debug)]
struct SlotInfo {
    count: Option<i64>,
    concept: bool,
}

/// Name, maker and both codes: what the slot lookup goes by.
fn slot_key(entry: &Map) -> [String; 4] {
    ["name", "hersteller", "kurz", "hkurz"].map(|k| hangar::field_text(entry, k).to_string())
}

fn slot_info(state: &HangarState, entry: &Map) -> SlotInfo {
    let id = ShipId {
        name: hangar::field_text(entry, "name"),
        maker: hangar::field_text(entry, "hersteller"),
        short: hangar::field_text(entry, "kurz"),
        maker_short: hangar::field_text(entry, "hkurz"),
    };
    SlotInfo { count: state.erkul.slot_count(id), concept: state.uex.is_concept(id.name) }
}

/// `_melt_caption`: the caption of a melt state, with the CCU data.
fn melt_caption(meltable: Option<bool>, upgradeable: bool) -> String {
    match fleet_analysis::state_for(meltable, upgradeable) {
        Some(state) => t(state.key()),
        None => t("s_fa_m_offen"),
    }
}

/// `_melt_choice`: automatic (from the import), yes or no, each with what
/// the CCU data says. Returns the new choice when it changed.
fn melt_choice(ui: &mut Ui, entry: &Map, index: usize) -> Option<String> {
    let up = fleet_analysis::upgradeable(entry);
    let auto = match hangar::melt_imported(entry) {
        Some(b) => tf("s_fa_m_import", &[("text", Arg::from(melt_caption(Some(b), up)))]),
        None => t("s_fa_m_offen"),
    };
    let choices = [
        ("", auto),
        (hangar::CHOICE_YES, melt_caption(Some(true), up)),
        (hangar::CHOICE_NO, melt_caption(Some(false), up)),
    ];
    let current = hangar::melt_choice(entry);
    let mut picked = current.to_string();
    let shown = choices.iter().find(|(v, _)| *v == current).map(|(_, l)| l.clone()).unwrap_or_default();
    // As wide as the longest of its own choices, so picking another one does
    // not move the right edge.
    let font = egui::TextStyle::Button.resolve(ui.style());
    let widest = choices
        .iter()
        .map(|(_, label)| {
            ui.fonts_mut(|f| f.layout_no_wrap(label.clone(), font.clone(), egui::Color32::WHITE).size().x)
        })
        .fold(0.0_f32, f32::max);
    let width = widest + 2.0 * ui.spacing().button_padding.x + ui.spacing().icon_width + ui.spacing().item_spacing.x;
    let combo = egui::ComboBox::from_id_salt(("hangar-schmelzen", index)).width(width).selected_text(&shown).show_ui(
        ui,
        |list| {
            for (value, label) in &choices {
                list.selectable_value(&mut picked, value.to_string(), label);
            }
        },
    );
    widgets::name_combo(&combo.response, &t("s_hgl_schmelzen"), &shown);
    (picked != current).then_some(picked)
}

/// What the choice of the unit asks for.
enum UnitPick {
    /// Back to "as at the Basetool": the next sync takes the unit from there.
    Back,
    /// A unit, or none (an empty id).
    Set(String),
}

/// The marker of "never synced" among the choices; never stored.
const UNSYNCED: &str = "\u{0}offen";

/// `_unit_choice`: the unit of a ship at the Basetool. ⚠ "Never synced" (no `einheit`) is not the same as "no
/// unit" (empty): whoever clicks the shown "No unit" would take the ship out of its unit explicitly.
fn unit_choice(ui: &mut Ui, entry: &Map, index: usize, menu: &[(String, String)]) -> Option<UnitPick> {
    let stored: Option<String> =
        entry.iter().find(|(k, _)| k == "einheit").and_then(|(_, v)| v.as_str().map(str::to_string));
    let mut choices: Vec<(String, String)> = vec![(String::new(), t("s_hg_keine_einheit"))];
    choices.extend(menu.iter().cloned());
    let current = stored.clone().unwrap_or_else(|| UNSYNCED.to_string());
    match &stored {
        None => choices.push((UNSYNCED.to_string(), t("s_hg_einheit_offen"))),
        // A unit the member is not in: shown, not to be chosen again.
        Some(id) if !choices.iter().any(|(v, _)| v == id) => choices.push((id.clone(), t("s_hg_einheit_fremd"))),
        Some(_) => {}
    }
    let shown = choices.iter().find(|(v, _)| *v == current).map(|(_, l)| l.clone()).unwrap_or_default();
    let font = egui::TextStyle::Button.resolve(ui.style());
    let widest = choices
        .iter()
        .map(|(_, label)| {
            ui.fonts_mut(|f| f.layout_no_wrap(label.clone(), font.clone(), egui::Color32::WHITE).size().x)
        })
        .fold(0.0_f32, f32::max);
    let width = widest + 2.0 * ui.spacing().button_padding.x + ui.spacing().icon_width + ui.spacing().item_spacing.x;
    let mut picked = current.clone();
    let combo = egui::ComboBox::from_id_salt(("hangar-einheit", index)).width(width).selected_text(&shown).show_ui(
        ui,
        |list| {
            for (value, label) in &choices {
                list.selectable_value(&mut picked, value.clone(), label);
            }
        },
    );
    widgets::name_combo(&combo.response, &t("s_ml_einheit"), &shown);
    if picked == current {
        return None;
    }
    Some(if picked == UNSYNCED { UnitPick::Back } else { UnitPick::Set(picked) })
}

/// The ready switch in a right-to-left line: the switch at the
/// right edge, its label before it; named for screen readers.
fn ready_switch(ui: &mut Ui, scheme: &Scheme, label: &str, on: &mut bool) -> bool {
    let response = ui.add(widgets::Toggle::new(on, scheme));
    let (enabled, now) = (response.enabled(), *on);
    response.widget_info(|| WidgetInfo::selected(Role::CheckBox, enabled, now, label));
    ui.label(RichText::new(label).small());
    response.changed()
}

/// How wide a small text is drawn.
fn small_width(ui: &Ui, text: &str) -> f32 {
    let font = egui::TextStyle::Small.resolve(ui.style());
    ui.fonts_mut(|f| f.layout_no_wrap(text.to_string(), font, egui::Color32::WHITE).size().x)
}

/// The mark of a ship: parts still to get, or fully fitted.
enum Mark {
    Open(usize),
    Fitted,
}

/// Size of the tick before the fully-fitted text.
const TICK: f32 = 14.0;

impl Mark {
    fn of(entry: &Map) -> Option<Mark> {
        match hangar::open_count(entry) {
            0 if hangar::fully_fitted(entry) => Some(Mark::Fitted),
            0 => None,
            n => Some(Mark::Open(n)),
        }
    }

    fn text(&self) -> String {
        match self {
            Mark::Open(n) => tf("s_hg_offen", &[("n", Arg::from(*n))]),
            Mark::Fitted => t("s_hgl_fertig"),
        }
    }

    fn width(&self, ui: &Ui) -> f32 {
        let tick = match self {
            Mark::Open(_) => 0.0,
            Mark::Fitted => TICK + ui.spacing().item_spacing.x,
        };
        tick + small_width(ui, &self.text())
    }

    /// In a right-to-left line: the text, then the tick before it.
    fn show(&self, ui: &mut Ui, scheme: &Scheme) {
        match self {
            Mark::Open(_) => {
                ui.label(RichText::new(self.text()).small().color(color(scheme.accent)));
            }
            Mark::Fitted => {
                ui.label(RichText::new(self.text()).small().color(color(scheme.gold)))
                    .on_hover_text(t("s_hg_fertig_hilfe"));
                if let Some(image) = crate::icons::image("check", scheme.yellow, TICK) {
                    ui.add(image);
                }
            }
        }
    }
}

/// What a row asks the page to do.
enum RowAction {
    /// Name and maker of the ship.
    Remove(String, String),
    Ready(bool),
    Melt(String),
    Unit(UnitPick),
    /// The box "Loadout & cart" asks for a change of the equipment.
    Equip(equipment_box::Action),
    /// The parts that can be crafted for the ship of this name.
    Craft(String),
    /// The ship is gone (a wipe): back onto the wish list.
    ToPlanned,
}

/// One ship: name and remove button; origin, insurance, package, slots and
/// the mark of planned parts; melt choice and ready switch; in-game name.
fn ship_row(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &HangarState,
    names: &mut ShipNames,
    ships: &[&Map],
    i: usize,
    equip: Option<(&equipment_box::Ctx<'_>, &mut EquipmentState)>,
) -> Option<RowAction> {
    let entry = ships[i];
    let mut action = None;
    ui.horizontal(|line| {
        line.with_layout(egui::Layout::right_to_left(egui::Align::Center), |line| {
            let word = t("s_hg_entfernen");
            if line.add(widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("trash-2")).clicked() {
                let maker = hangar::field_text(entry, "hersteller").to_string();
                action = Some(RowAction::Remove(hangar::field_text(entry, "name").to_string(), maker));
            }
            // The name takes the room left of the button, from the left edge.
            line.with_layout(egui::Layout::left_to_right(egui::Align::Center), |line| {
                line.add(egui::Label::new(RichText::new(hangar::field_text(entry, "name")).strong()).truncate());
            });
        });
    });
    let (slots, concept, _) = slots_part(state, entry);
    let line = format!("{}  ·  {slots}", details(ships, i));
    let tone = if concept { scheme.gold } else { scheme.sub };
    ui.add(egui::Label::new(RichText::new(line).small().color(color(tone))).wrap());
    // Fixed places: the melt choice at the left, the ready switch at the
    // right under the remove button, the mark right-aligned before it. What does
    // not fit goes to its own line below, right-aligned; nothing moves the
    // controls.
    let mark = Mark::of(entry);
    let ready_label = t("s_hg_einsatzbereit");
    let gap = ui.spacing().item_spacing.x;
    let ready_width = small_width(ui, &ready_label) + gap + widgets::TOGGLE_SIZE.x;
    let mut ready = hangar::field_true(entry, "einsatzbereit");
    let (mut ready_below, mut mark_below) = (false, false);
    ui.horizontal(|line| {
        if hangar::melt_applies(entry)
            && let Some(choice) = melt_choice(line, entry, i)
        {
            action = Some(RowAction::Melt(choice));
        }
        if let Some(menu) = &state.unit_menu
            && let Some(pick) = unit_choice(line, entry, i, menu)
        {
            action = Some(RowAction::Unit(pick));
        }
        if line.available_width() < ready_width {
            ready_below = true;
            mark_below = mark.is_some();
            return;
        }
        line.with_layout(egui::Layout::right_to_left(egui::Align::Center), |line| {
            if ready_switch(line, scheme, &ready_label, &mut ready) {
                action = Some(RowAction::Ready(ready));
            }
            if let Some(mark) = &mark {
                if line.available_width() >= mark.width(line) {
                    mark.show(line, scheme);
                } else {
                    mark_below = true;
                }
            }
        });
    });
    // A line of its own, right-aligned like the remove button in the head line.
    let right_line = |ui: &mut Ui, add: &mut dyn FnMut(&mut Ui)| {
        ui.horizontal(|line| {
            line.with_layout(egui::Layout::right_to_left(egui::Align::Center), |line| add(line));
        });
    };
    if ready_below {
        right_line(ui, &mut |line| {
            if ready_switch(line, scheme, &ready_label, &mut ready) {
                action = Some(RowAction::Ready(ready));
            }
        });
    }
    if mark_below && let Some(mark) = &mark {
        right_line(ui, &mut |line| mark.show(line, scheme));
    }
    // Name, maker and how many alike came before: stays with the ship when
    // the list or the file order changes.
    let (name, maker) = (hangar::field_text(entry, "name"), hangar::field_text(entry, "hersteller"));
    let before = ships[..i]
        .iter()
        .filter(|s| hangar::field_text(s, "name") == name && hangar::field_text(s, "hersteller") == maker)
        .count();
    let key = format!("{name}\u{1f}{maker}\u{1f}{before}");
    // The slots and the cart of the ship.
    if let Some((ctx, eq_state)) = equip
        && let Some(asked) = equipment_box::show(ui, scheme, ctx, eq_state, entry, &key, false)
    {
        action = Some(RowAction::Equip(asked));
    }
    // To the crafting page, with this ship in its filter: what can be built for it, with or without the blueprint.
    let word = t("s_hg_herstellbar");
    let spoken = tf("s_hg_herstellbar_fuer", &[("schiff", Arg::from(name))]);
    let craft = widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("zap").spoken(&spoken);
    ui.horizontal_wrapped(|row| {
        if row.add(craft).on_hover_text(t("s_hg_herstellbar_tip")).clicked() {
            action = Some(RowAction::Craft(name.trim().to_string()));
        }
        // A vehicle of a package follows its carrier and does not move on its own.
        if hangar::field_text(entry, "herkunft") != CATALOG {
            let word = t("s_hg_nach_geplant");
            let spoken = tf("s_hg_nach_geplant_fuer", &[("schiff", Arg::from(name))]);
            let back = widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("star").spoken(&spoken);
            if row.add(back).on_hover_text(t("s_hg_nach_geplant_tip")).clicked() {
                action = Some(RowAction::ToPlanned);
            }
        }
    });
    names.card(ui, scheme, name, &key);
    action
}

fn small(ui: &mut Ui, text: &str, tone: Color) {
    ui.add(egui::Label::new(RichText::new(text).small().color(color(tone))).wrap());
}

/// The three steps of the direct way and where to get the Hangar-Exporter.
fn direct_steps(ui: &mut Ui, scheme: &Scheme) {
    ui.label(RichText::new(t("s_hgl_direkt_titel")).strong());
    small(ui, &t("s_hgl_direkt_text"), scheme.sub);
    for step in ["s_hgl_direkt_1", "s_hgl_direkt_2", "s_hgl_direkt_3"] {
        small(ui, &t(step), scheme.sub);
    }
    small(ui, &t("s_hgl_exporter_holen"), scheme.sub);
    exporter_buttons(ui, scheme);
}

/// One button per browser for getting the Hangar-Exporter, all three alike: Firefox opens the file the installer
/// brought, Chrome and Edge the page with their package (`exporter::plan`).
pub(crate) fn exporter_buttons(ui: &mut Ui, scheme: &Scheme) {
    use versekit_core::exporter::{self, Browser};
    ui.horizontal_wrapped(|row| {
        for browser in Browser::ALL {
            let word = t(browser.label());
            let icon = if browser == Browser::Firefox { "puzzle" } else { "external-link" };
            let button = row.add(widgets::Button::new(&word, ButtonKind::Normal, scheme).icon(icon));
            if button.on_hover_text(t(browser.hint())).clicked() {
                exporter::open(browser);
            }
        }
    });
}

/// Fetching from the pledge store: first straight from the Hangar-Exporter,
/// then from an export file or a file dropped onto the window.
fn import_part(ui: &mut Ui, scheme: &Scheme, state: &mut HangarState) {
    ui.label(RichText::new(t("s_hg_import_titel")).strong());
    ui.add_space(2.0);
    // Steps and pairing side by side; too narrow, the pairing goes below.
    if ui.available_width() >= PAIRING_SIDE_BY_SIDE {
        ui.columns(2, |cols| {
            direct_steps(&mut cols[0], scheme);
            pairing_box(&mut cols[1], scheme, state);
        });
    } else {
        direct_steps(ui, scheme);
        ui.add_space(6.0);
        pairing_box(ui, scheme, state);
    }
    ui.add_space(8.0);
    ui.label(RichText::new(t("s_hgl_datei_titel")).strong());
    small(ui, &t("s_hgl_datei_text"), scheme.sub);
    small(ui, &t("s_hgl_import_json"), scheme.gold);
    // With the hangar shared over the Basetool, its ships win over a file.
    if state.basetool_ships_on() {
        small(ui, &t("s_hg_import_basetool_2"), scheme.gold);
    }
    ui.add_space(4.0);
    let hovering = ui.input(|i| !i.raw.hovered_files.is_empty());
    let width = ui.available_width();
    ui.add(widgets::text_field(&mut state.import_path, &t("s_hgl_pfad"), scheme).desired_width(width));
    if hovering {
        small(ui, &t("s_hgl_ablegen"), scheme.accent);
    }
    ui.horizontal_wrapped(|row| {
        // JSON first: the way the page recommends; CSV stays choosable.
        let (word, open) = (t("s_hg_import_knopf"), t("s_hgl_dialog_offen"));
        let choose = widgets::Button::new(&word, ButtonKind::Strong, scheme).icon("folder");
        let busy = state.dialog.is_some();
        if row.add(choose.blocked(busy.then_some(open.as_str()))).clicked() && !busy {
            let start =
                std::path::Path::new(state.import_path.trim()).parent().map(|p| p.to_string_lossy().into_owned());
            // JSON first: the way the page recommends; CSV stays choosable.
            let filters: file_dialog::Filters = &[("JSON", &["json"]), ("CSV", &["csv"])];
            state.dialog = Some(file_dialog::open_file(row.ctx(), &word, filters, start.as_deref()));
        }
        let can = !state.import_path.trim().is_empty();
        let (word, reason) = (t("s_hgl_einlesen"), t("s_hgl_erst_pfad"));
        let read = widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("download");
        if row.add(read.blocked((!can).then_some(reason.as_str()))).clicked() && can {
            let path = state.import_path.clone();
            state.import(&path);
        }
    });
}

/// Entering by hand: a ship of the UEX list, bought with real money
/// (with or without LTI) or in the game.
fn hand_part(ui: &mut Ui, scheme: &Scheme, state: &mut HangarState) {
    ui.label(RichText::new(t("s_hg_hand_titel")).strong());
    small(ui, &t("s_hgl_hand_text"), scheme.sub);
    if state.all_names.is_empty() {
        small(ui, &t("s_hgl_ohne_uex"), scheme.gold);
    }
    ui.add_space(4.0);
    let more = |n: usize| tf("s_hgl_mehr", &[("n", Arg::from(n))]);
    let hint = t("s_hgl_pl_schiff");
    let width = ui.available_width();
    let field = widgets::SuggestField::new("hangar-eintragen", &hint, scheme, &more).width(width);
    let shown = field.show(ui, &mut state.hand_name, &state.all_names);
    let typed_enter = shown.response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    if widgets::switch_row(ui, scheme, &t("s_hg_mit_echtgeld"), &mut state.real_money).changed() && !state.real_money {
        state.lti = false;
    }
    if state.real_money {
        widgets::switch_row(ui, scheme, &t("s_hg_hat_lti"), &mut state.lti);
    }
    let word = t("s_hg_eintragen");
    if ui.add(widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("plus")).clicked()
        || (typed_enter && shown.picked.is_none())
    {
        state.enter_by_hand();
    }
    if let Some((text, tone)) = &state.message {
        let tone = match tone {
            Tone::Good => scheme.accent,
            Tone::Bad => scheme.red_text(),
            Tone::Quiet => scheme.sub,
        };
        small(ui, text, tone);
    }
}

impl HangarState {
    /// Saves after a change on the wish list; a failure (or a file set aside) is said on this page too, not
    /// only on the page of the hangar.
    fn save_wish(&mut self) -> bool {
        let ok = self.save().ok();
        if !ok {
            self.wish_message = self.message.clone();
        }
        ok
    }

    /// `pages._wishlist.eintragen`: puts the ship being entered on the list of planned ships. Three refusals,
    /// each said: not a ship of the list, already in the hangar (one does not wish for what one has), already
    /// on the list.
    fn enter_wish(&mut self) {
        let name = self.wish_name.trim().to_string();
        match self.plan_ship(&name) {
            Planned::Noted => {
                self.wish_message =
                    Some((tf("s_hg_wunsch_notiert", &[("name", Arg::from(name.as_str()))]), Tone::Good));
                self.wish_name.clear();
            }
            // `save_wish` has said why it failed.
            Planned::NotSaved => {}
            refused => self.wish_message = Some((t(refused.refusal_key()), Tone::Bad)),
        }
    }

    /// Puts the ship on the list of planned ships, stored with the maker UEX has in the name; the one
    /// way in for the section "Planned" and for the page "Shops".
    fn plan_ship(&mut self, name: &str) -> Planned {
        let name = name.trim();
        if !self.uex.knows(name) {
            return Planned::NotAShip;
        }
        let maker = self.uex.manufacturer(name);
        if hangar::owns_wished(&self.held.data, name, &maker) {
            Planned::Owned
        } else if hangar::wishlist_add(&mut self.held.data, name, &maker) {
            // The maker goes with it: without it erkul does not find some ships (the Prospector).
            if self.save_wish() {
                // A planned ship gets its slots at once, not only at the next start.
                self.ask_slots(Trigger::Wish);
                Planned::Noted
            } else {
                Planned::NotSaved
            }
        } else {
            Planned::Twice
        }
    }

    /// Whether the ship stands on the list of planned ships.
    pub(crate) fn is_planned(&self, name: &str) -> bool {
        hangar::wishlist_contains(&self.held.data, name.trim())
    }

    /// For the page "Shops": reads the file again where it changed elsewhere, so that "already planned"
    /// is not decided on an old state.
    pub(crate) fn follow_planned(&mut self, ui: &Ui) {
        self.follow(ui);
    }

    /// For the page "Shops": plans the ship and says what came of it, as (line, failed).
    pub(crate) fn plan_ship_said(&mut self, name: &str) -> (String, bool) {
        match self.plan_ship(name) {
            Planned::Noted => (tf("s_hg_im_hangar_vorgemerkt", &[("0", Arg::from(name.trim()))]), false),
            Planned::NotSaved => {
                (self.wish_message.clone().map_or_else(|| t("s_hgl_nicht_gespeichert"), |(text, _)| text), true)
            }
            Planned::NotAShip if self.all_names.is_empty() => (t("s_hgl_ohne_uex"), true),
            refused => (t(refused.refusal_key()), true),
        }
    }

    /// Makes the page scroll to the section "Planned" the next time it is drawn.
    pub fn show_planned(&mut self) {
        self.scroll_to_planned = true;
    }

    /// Whether the page is waiting to scroll to the section "Planned".
    pub fn planned_pending(&self) -> bool {
        self.scroll_to_planned
    }

    /// The sentence after a move, with the note when the ship joined an entry of the same ship.
    fn moved_text(key: &str, name: &str, moved: hangar::Moved) -> String {
        let mut text = tf(key, &[("name", Arg::from(name))]);
        if moved == hangar::Moved::Joined {
            text.push(' ');
            text.push_str(&t("s_hg_verschoben_dazu"));
        }
        text
    }

    /// The sentence for a move that was refused.
    fn move_refusal(refused: hangar::MoveRefused) -> String {
        t(match refused {
            hangar::MoveRefused::Gone => "s_hg_verschieben_weg",
            hangar::MoveRefused::Included => "s_hg_verschieben_paket",
        })
    }

    /// A planned ship was bought: it goes into the own ships with all it holds, after a copy of the file.
    fn move_to_hangar(&mut self, name: &str) {
        if !self.backup() {
            self.wish_message = self.message.clone();
            return;
        }
        let stock = hangar::wishlist(&self.held.data)
            .into_iter()
            .find(|w| hangar::field_text(w, "name") == name)
            .map(|wish| self.stores.erkul.stock_ports(wish))
            .unwrap_or_default();
        match hangar::wish_to_hangar(&mut self.held.data, name, &stock) {
            Ok(moved) => {
                self.wish_message = None;
                self.heights.clear();
                if self.save_wish() {
                    self.moved = Some((Self::moved_text("s_hg_verschoben_meine", name, moved), "plane-landing", None));
                }
                self.ask_slots(Trigger::ByHand);
            }
            Err(refused) => self.wish_message = Some((Self::move_refusal(refused), Tone::Bad)),
        }
    }

    /// An own ship is gone (a wipe): the ship at `index` goes back onto the wish list with all it holds, after a
    /// copy of the file.
    fn move_to_planned(&mut self, index: usize) {
        if !self.backup() {
            return;
        }
        let Some(entry) = hangar::ships(&self.held.data).get(index).map(|e| (*e).clone()) else { return };
        let name = hangar::field_text(&entry, "name").to_string();
        let stock = self.stores.erkul.stock_ports(&entry);
        match hangar::hangar_to_wish(&mut self.held.data, index, &stock) {
            Ok(moved) => {
                self.message = None;
                self.heights.clear();
                if self.save().ok() {
                    self.moved = Some((Self::moved_text("s_hg_verschoben_geplant", &name, moved), "star", None));
                }
            }
            Err(refused) => self.message = Some((Self::move_refusal(refused), Tone::Bad)),
        }
    }

    /// Share, save and open of a box: they change no entry. True when the action was one of them.
    fn share_action(&mut self, ui: &Ui, entry: &Map, action: &equipment_box::Action) -> bool {
        use equipment_box::Action;
        use versekit_core::equipment_share as share;
        let (ship, maker) = (hangar::field_text(entry, "name"), hangar::field_text(entry, "hersteller"));
        let stock = self.stores.erkul.stock_ports(entry);
        let all = equipment::read_all(entry, &stock);
        let code_of =
            |e: &equipment::Equipment| share::encode(&share::share(ship, maker, e, &self.extra.ids));
        match action {
            Action::Share { key } => {
                let fallback = equipment::Equipment { name: equipment::DEFAULT_NAME.into(), ..Default::default() };
                let code = code_of(all.active().unwrap_or(&fallback));
                ui.ctx().copy_text(code.clone());
                let n = Arg::from(code.chars().count());
                let said = if code.len() > share::CHAT_LIMIT {
                    (tf("s_wk_teilen_zu_lang", &[("n", n)]), true)
                } else {
                    (tf("s_wk_teilen_kopiert", &[("n", n)]), false)
                };
                self.equipment.said = Some((key.clone(), said.0, said.1));
            }
            Action::SaveFile { key } => {
                if self.share_dialog.is_none() {
                    let codes: Vec<String> = if all.0.is_empty() {
                        vec![code_of(&equipment::Equipment { name: equipment::DEFAULT_NAME.into(), ..Default::default() })]
                    } else {
                        all.0.iter().map(code_of).collect()
                    };
                    let title = t("s_wk_datei_speichern");
                    let name = file_name(&tf("s_wk_datei_name", &[("schiff", Arg::from(ship))]));
                    let filters: file_dialog::Filters = &[("Text", &[share::FILE_ENDING])];
                    let pending = file_dialog::save_file(ui.ctx(), &title, filters, None, &name);
                    self.share_dialog = Some(ShareDialog::Save { key: key.clone(), text: share::file_text(&codes), pending });
                }
            }
            Action::OpenFile { key } => {
                if self.share_dialog.is_none() {
                    let title = t("s_wk_datei_oeffnen");
                    let filters: file_dialog::Filters = &[("Text", &[share::FILE_ENDING])];
                    let pending = file_dialog::open_file(ui.ctx(), &title, filters, None);
                    self.share_dialog = Some(ShareDialog::Open { key: key.clone(), pending });
                }
            }
            _ => return false,
        }
        true
    }

    /// The answer of the dialog of the share codes, once it was closed: the file is written, or read into the field
    /// of its box. A cancel says nothing.
    fn take_share_dialog(&mut self) {
        let answer = match &self.share_dialog {
            Some(ShareDialog::Save { pending, .. } | ShareDialog::Open { pending, .. }) => pending.take(),
            None => return,
        };
        let Some(answer) = answer else { return };
        let Some(dialog) = self.share_dialog.take() else { return };
        let Some(path) = answer else { return };
        match dialog {
            ShareDialog::Save { key, text, .. } => {
                let said = match jsonio::write_text_atomic(&path, &text, jsonio::Newline::Native) {
                    Ok(()) => (tf("s_wk_datei_gespeichert", &[("datei", Arg::from(path.as_str()))]), false),
                    Err(e) => (tf("s_wk_datei_fehler", &[("grund", Arg::from(e.to_string()))]), true),
                };
                self.equipment.said = Some((key, said.0, said.1));
            }
            ShareDialog::Open { key, .. } => match read_share_file(&path) {
                Ok(text) => {
                    self.equipment.said = None;
                    self.equipment.pasting = Some(equipment_box::Pasting { key, text });
                }
                Err(reason) => {
                    let said = tf("s_wk_datei_lesefehler", &[("grund", Arg::from(reason))]);
                    self.equipment.said = Some((key, said, true));
                }
            },
        }
    }
}

/// The equipments of the other entries of the same ship as `entry`, own ships first, then planned ones, each with the
/// line that names where it stands. The entry itself is left out.
fn same_type_loadouts(data: &Map, stores: &Stores, entry: &Map) -> Vec<(String, equipment::Equipment)> {
    let (ship, maker) = (hangar::field_text(entry, "name"), hangar::field_text(entry, "hersteller"));
    let mut found = Vec::new();
    for (list, entries) in [(t("s_wk_aus_meine"), hangar::ships(data)), (t("s_hg_geplant_titel"), hangar::wishlist(data))] {
        for other in entries {
            if other == entry || !equipment_box::same_type(other, ship, maker, stores) {
                continue;
            }
            let stock = stores.erkul.stock_ports(other);
            for loadout in equipment::read_all(other, &stock).0 {
                let line = tf(
                    "s_wk_uebernehmen_eintrag",
                    &[
                        ("liste", Arg::from(list.as_str())),
                        ("schiff", Arg::from(hangar::field_text(other, "name"))),
                        ("name", Arg::from(loadout.name.as_str())),
                    ],
                );
                found.push((line, loadout));
            }
        }
    }
    found
}

/// A file name from a ship name: the characters a file system does not take become spaces.
fn file_name(stem: &str) -> String {
    let clean: String = stem.chars().map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { ' ' } else { c }).collect();
    format!("{}.{}", clean.trim(), versekit_core::equipment_share::FILE_ENDING)
}

/// The text of a file of share codes; a file too big for codes is not read.
fn read_share_file(path: &str) -> Result<String, String> {
    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > SHARE_FILE_LIMIT {
        return Err(t("s_wk_datei_zu_gross"));
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// What putting a ship on the list of planned ships came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Planned {
    Noted,
    /// A ship UEX does not list.
    NotAShip,
    /// The hangar holds it already.
    Owned,
    /// It is on the list already.
    Twice,
    /// The file could not be written.
    NotSaved,
}

impl Planned {
    /// The text that says why a ship was refused.
    fn refusal_key(self) -> &'static str {
        match self {
            Planned::NotAShip => "s_hg_kein_name",
            Planned::Owned => "s_hg_wunsch_schon_da",
            _ => "s_hg_wunsch_doppelt",
        }
    }
}

/// The entry of a wished ship by its name, to change it.
fn wish_mut<'a>(data: &'a mut Map, name: &str) -> Option<&'a mut Map> {
    let wanted = hangar::slim(name);
    match data.iter_mut().find(|(k, _)| k == "wunsch") {
        Some((_, Value::Array(items))) => items.iter_mut().find_map(|w| match w {
            Value::Object(w) if hangar::slim(hangar::field_text(w, "name")) == wanted => Some(w),
            _ => None,
        }),
        _ => None,
    }
}

/// What a wished ship costs: buying at the cheapest place, renting per day; else the sentence that says why
/// there is no price (many ships can only be had for real money). Nothing is fetched, the prices are in the
/// UEX files already.
fn wish_price(state: &HangarState, name: &str, scheme: &Scheme) -> (String, Color) {
    let market = &state.stores.market;
    let mut parts: Vec<String> = Vec::new();
    if let Some(best) = market.buy_at(name).first() {
        let place = if best.place.is_empty() { "?".to_string() } else { best.place.clone() };
        parts.push(tf(
            "s_hg_wunsch_kauf",
            &[("0", Arg::from(crate::shopping_page::plain(best.price))), ("1", Arg::from(place))],
        ));
    }
    if let Some(rent) = market.rent_at(name).first() {
        parts.push(tf("s_hg_wunsch_miete", &[("0", Arg::from(crate::shopping_page::plain(rent.price)))]));
    }
    if parts.is_empty() { (t("s_hg_wunsch_kein_preis"), scheme.gold) } else { (parts.join("  ·  "), scheme.sub) }
}

/// The section "Planned" below the own ships: ships one has in mind, with the price and the loadout one
/// can plan before owning them.
fn show_planned(ui: &mut Ui, scheme: &Scheme, state: &mut HangarState, sources: Option<&dyn CartSources>) {
    let head = widgets::heading(ui, scheme, &t("s_hg_geplant_titel"));
    if std::mem::take(&mut state.scroll_to_planned) {
        head.scroll_to_me_animation(Some(egui::Align::TOP), egui::style::ScrollAnimation::none());
    }
    ui.label(RichText::new(t("s_wl_lead")).small().color(color(scheme.sub)));
    ui.add_space(GAP);
    if state.all_names.is_empty() {
        small(ui, &t("s_hgl_ohne_uex"), scheme.gold);
    }
    let more = |n: usize| tf("s_hgl_mehr", &[("n", Arg::from(n))]);
    let hint = t("s_hgl_pl_schiff");
    let width = ui.available_width();
    let all_names = std::mem::take(&mut state.all_names);
    let shown = widgets::SuggestField::new("wunsch-eintragen", &hint, scheme, &more).width(width).show(
        ui,
        &mut state.wish_name,
        &all_names,
    );
    state.all_names = all_names;
    let typed_enter = shown.response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    let word = t("s_hg_wunsch_eintragen");
    if ui.add(widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("plus")).clicked()
        || (typed_enter && shown.picked.is_none())
    {
        state.enter_wish();
    }
    if let Some((text, tone)) = &state.wish_message {
        let tone = match tone {
            Tone::Good => scheme.accent,
            Tone::Bad => scheme.red_text(),
            Tone::Quiet => scheme.sub,
        };
        small(ui, text, tone);
    }
    ui.add_space(GAP);
    let names: Vec<String> =
        hangar::wishlist(&state.held.data).iter().map(|w| hangar::field_text(w, "name").to_string()).collect();
    ui.label(widgets::bold(ui, tf("s_hg_wunsch_meine", &[("n", Arg::from(names.len()))]), egui::TextStyle::Body));
    ui.add_space(4.0);
    if names.is_empty() {
        widgets::empty_state(ui, scheme, "star", &t("s_wl_leer"));
        return;
    }
    let mut equipment = std::mem::take(&mut state.equipment);
    let (stores, extra) = (state.stores.clone(), state.extra.clone());
    let data = &state.held.data;
    let others = |entry: &Map| same_type_loadouts(data, &stores, entry);
    let mut asked: Option<(String, WishAction)> = None;
    for name in &names {
        let entry: Map = hangar::wishlist(&state.held.data)
            .into_iter()
            .find(|w| hangar::field_text(w, "name") == name)
            .cloned()
            .unwrap_or_default();
        let (price, tone) = wish_price(state, name, scheme);
        let card = widgets::card(ui, scheme, |cell| {
            cell.set_width(cell.available_width());
            let mut action = None;
            cell.horizontal(|line| {
                line.with_layout(egui::Layout::right_to_left(egui::Align::Center), |line| {
                    if crate::farm_page::row_button(line, scheme, "trash-2", &t("s_hg_wunsch_streichen"), name, true) {
                        action = Some(WishAction::Remove);
                    }
                    line.with_layout(egui::Layout::left_to_right(egui::Align::Center), |line| {
                        line.add(egui::Label::new(widgets::bold(line, name.clone(), egui::TextStyle::Body)).truncate());
                    });
                });
            });
            cell.add(egui::Label::new(RichText::new(price).small().color(color(tone))).wrap());
            let word = t("s_hg_nach_meine");
            let spoken = tf("s_hg_nach_meine_fuer", &[("schiff", Arg::from(name.as_str()))]);
            let bought = widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("plane-landing").spoken(&spoken);
            if cell.add(bought).on_hover_text(t("s_hg_nach_meine_tip")).clicked() {
                action = Some(WishAction::ToHangar);
            }
            let open = hangar::open_count(&entry);
            if open > 0 {
                small(cell, &tf("s_hg_offen", &[("n", Arg::from(open))]), scheme.accent);
            }
            let ctx = sources.map(|sources| equipment_box::Ctx {
                stores: &stores,
                extra: &extra,
                sources,
                paths: state.paths.as_ref(),
                others: Some(&others),
            });
            if let Some(ctx) = &ctx
                && let Some(done) =
                    equipment_box::show(cell, scheme, ctx, &mut equipment, &entry, &format!("wish\u{1f}{name}"), true)
            {
                action = Some(WishAction::Equip(done));
            }
            action
        });
        if let Some(action) = card.inner {
            asked = Some((name.clone(), action));
        }
        ui.add_space(CARD_GAP);
    }
    state.equipment = equipment;
    state.look_up_prices(ui);
    let Some((name, action)) = asked else { return };
    match action {
        WishAction::Remove => {
            // A copy of the file first, as when a ship is removed from the hangar: the plan hangs on it.
            if state.backup() && hangar::wishlist_remove(&mut state.held.data, &name) {
                state.wish_message = None;
                state.save_wish();
            }
        }
        WishAction::ToHangar => state.move_to_hangar(&name),
        WishAction::Equip(done) => {
            let entry = hangar::wishlist(&state.held.data)
                .into_iter()
                .find(|w| hangar::field_text(w, "name") == name)
                .cloned()
                .unwrap_or_default();
            if state.share_action(ui, &entry, &done) {
                return;
            }
            let (stores, extra) = (state.stores.clone(), state.extra.clone());
            if let Some(entry) = wish_mut(&mut state.held.data, &name)
                && change_plan(entry, &done, &stores, &extra, &mut state.equipment)
            {
                state.save_wish();
            }
            ui.ctx().request_repaint();
        }
    }
}

/// A file written elsewhere may hold the equipments and no plan: before the player changes a slot the plan is
/// made from the active equipment, so that the change is made on what is stored and does not replace it.
fn restore_plan(entry: &mut Map, stores: &Stores, extra: &Extra) {
    let exact = stores.erkul.slot_paths(entry);
    if equipment::restore_loadout(entry, &|reference| equipment_box::part_name(stores, &extra.craft, reference), &exact) {
        respell_plan(entry, stores);
    }
}

/// What the box asked of a ship or a wish, done on its entry: the plan (`belegung`, which the pages work on)
/// is changed, and the active equipment is stored in the form of the exchange (#2821) with it. True when
/// something changed (then the file is to be saved).
///
/// The named equipments (switch, new, rename, delete and its undo) and what the plan cannot say (how a part was
/// got, its quality) are changed on the equipments; a switch makes the plan the one of the new active equipment.
/// What was deleted waits in `box_state` to be put back.
fn change_plan(
    entry: &mut Map,
    action: &equipment_box::Action,
    stores: &Stores,
    extra: &Extra,
    box_state: &mut EquipmentState,
) -> bool {
    use equipment_box::Action;
    restore_plan(entry, stores, extra);
    let stock = stores.erkul.stock_ports(entry);
    let exact = stores.erkul.slot_paths(entry);
    let name_of = |reference: &str| equipment_box::part_name(stores, &extra.craft, reference);
    let changed = match action {
        Action::Switch { name } => equipment::switch(entry, name, &stock, &name_of, &exact),
        Action::Create { name } => equipment::create(entry, name, &stock),
        Action::Rename { old, new } => {
            equipment::ensure_stored(entry, &stock);
            equipment::rename(entry, old, new)
        }
        Action::Delete { name, key } => match equipment::delete(entry, name, &stock, &name_of, &exact) {
            Some(removed) => {
                box_state.removed = Some((key.clone(), removed));
                box_state.removed_at = None;
                true
            }
            None => false,
        },
        Action::Undo => match box_state.removed.take() {
            Some((_, removed)) => equipment::undo_delete(entry, &removed, &stock, &name_of, &exact),
            None => false,
        },
        Action::Acquire { paths, acquisition, blueprint } => {
            equipment::set_acquisition(entry, paths, *acquisition, blueprint.as_deref(), &stock)
        }
        Action::Quality { paths, slot, value } => equipment::set_quality(entry, paths, slot, *value, &stock),
        Action::AddShared { equipments, key } => {
            let names: Vec<String> =
                equipments.iter().map(|e| equipment::add_as_active(entry, e, &stock, &name_of, &exact)).collect();
            let said = tf("s_wk_code_angelegt", &[("namen", Arg::from(names.join(", ")))]);
            box_state.said = Some((key.clone(), said, false));
            !names.is_empty()
        }
        // "Buy" or "build" in the cart is how the part is got: a pledge before does not stick.
        Action::Method { paths, way } => {
            let acquisition = equipment::Acquisition::from_way(way);
            let part = paths.first().and_then(|path| plan_part(entry, path));
            let blueprint = part
                .and_then(|(reference, name)| equipment_box::recipe_for(extra, &reference, &name))
                .map(|recipe| recipe.blueprint.clone());
            equipment::set_acquisition(entry, paths, acquisition, blueprint.as_deref(), &stock)
        }
        _ => {
            let applied = equipment_box::apply(entry, action);
            if applied {
                equipment::sync_entry(entry, &stock);
            }
            applied
        }
    };
    if changed && matches!(action, Action::Switch { .. } | Action::Delete { .. } | Action::Undo | Action::AddShared { .. }) {
        respell_plan(entry, stores);
    }
    // A change closes the way back to what was deleted before.
    if changed && !matches!(action, Action::Delete { .. } | Action::Undo) {
        box_state.removed = None;
    }
    // The changed plan may leave an entry without its slot, or give one its slot back.
    stores.erkul.tidy_plan(entry) || changed
}

/// A plan made from an equipment writes a path in lower case where the erkul data has no spelling for it
/// without the plan: the slots below a rack the plan puts in. With the plan those slots are known, and the plan
/// takes their spelling, rack below rack.
fn respell_plan(entry: &mut Map, stores: &Stores) {
    use versekit_core::erkul_slots::{hardpoints, ship_id};
    for _ in 0..4 {
        let Some((_, Value::Object(plan))) = entry.iter().find(|(k, _)| k == "belegung") else { return };
        let plan = plan.clone();
        let exact: Vec<String> = hardpoints(&stores.erkul.ships, &stores.erkul.parts, ship_id(entry), &plan)
            .into_iter()
            .map(|s| s.path)
            .collect();
        let mut renamed = false;
        let spelled: Map = plan
            .into_iter()
            .map(|(path, part)| match exact.iter().find(|p| **p != path && p.to_lowercase() == path) {
                Some(found) => {
                    renamed = true;
                    (found.clone(), part)
                }
                None => (path, part),
            })
            .collect();
        if !renamed {
            return;
        }
        map_set(entry, "belegung", Value::Object(spelled));
    }
}

/// The part the plan puts into a slot: its identifier and its name.
fn plan_part(entry: &Map, path: &str) -> Option<(String, String)> {
    let Some((_, Value::Object(plan))) = entry.iter().find(|(k, _)| k == "belegung") else { return None };
    let part = plan.iter().find(|(k, _)| k == path).and_then(|(_, v)| v.as_object())?;
    Some((hangar::field_text(part, "ref").to_string(), hangar::field_text(part, "name").to_string()))
}

/// What a row of the wish list asks.
enum WishAction {
    Remove,
    /// The ship was bought: into the own ships.
    ToHangar,
    Equip(equipment_box::Action),
}

/// The hangar page without the boxes "Loadout & cart": they need the recipes.
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut HangarState) {
    show_page_with(ui, scheme, state, None);
}

/// The hangar page; with `sources` every ship has its box "Loadout & cart".
pub fn show_page_with(ui: &mut Ui, scheme: &Scheme, state: &mut HangarState, sources: Option<&dyn CartSources>) {
    // Entering the page fetches the slots a ship still lacks, once per entering.
    let frame = ui.ctx().cumulative_frame_nr();
    let entered = state.last_frame.is_none_or(|last| frame > last + 1);
    state.last_frame = Some(frame);
    if entered {
        state.sync.wake(ui.ctx());
        state.ask_slots(Trigger::PageOpen);
    }
    state.follow(ui);
    state.names.follow();
    state.take_dropped(ui);
    state.take_dialog();
    state.take_share_dialog();
    // A ship that changed lists is said at the top right for a few seconds: its card is gone from where it was.
    if let Some((text, icon, since)) = state.moved.as_mut() {
        let now = ui.input(|i| i.time);
        let since = *since.get_or_insert(now);
        if !widgets::notice(ui.ctx(), scheme, since, icon, text, None).shown {
            state.moved = None;
        }
    }
    ready_question(ui, scheme, state);
    state.unit_menu = state.unit_choices();
    ui.label(RichText::new(t("s_hgl_lead")).small().color(color(scheme.sub)));
    if let Some((line, fine)) = state.receiver.as_ref().and_then(last_event_line) {
        let tone = if fine { scheme.accent } else { scheme.red_text() };
        ui.add(egui::Label::new(RichText::new(line).small().color(color(tone))).wrap());
    }
    ui.add_space(4.0);
    let width = ui.available_width();
    widgets::search_field(ui, &mut state.search, &t("s_hgl_suche"), scheme, width);
    ui.add_space(GAP);
    // Lead and search stay put; the rest scrolls, in the room left.
    let room = crate::layout::list_room(ui, ui.text_style_height(&egui::TextStyle::Body) + 10.0);
    let area = egui::ScrollArea::vertical()
        .id_salt("hangar-seite")
        .max_height(room)
        .min_scrolled_height(room)
        .auto_shrink([false, false]);
    widgets::scroll_area(ui, scheme, area, |ui| {
        ui.set_max_width(ui.available_width() - crate::layout::SAFETY);
        let open = state.adding_open.unwrap_or_else(|| hangar::ships(&state.held.data).is_empty());
        head_card(ui, scheme, state, open);
        ui.add_space(GAP);
        if open {
            import_part(ui, scheme, state);
            ui.add_space(GAP);
            hand_part(ui, scheme, state);
            ui.add_space(GAP);
        }
        if state.fleet.open {
            crate::fleet_analysis_view::show(ui, scheme, &mut state.fleet, &hangar::ships(&state.held.data));
            ui.add_space(GAP);
        }
        ship_list(ui, scheme, state, sources);
        ui.add_space(GAP);
        show_planned(ui, scheme, state, sources);
    });
    state.names.finish_frame();
}

/// The slim card at the head of the page: the note that the game does not tell the hangar on the left, two buttons
/// on the right that open the parts below it (getting or entering ships, the fleet analysis). `adding_open` is
/// whether the first part is open now.
fn head_card(ui: &mut Ui, scheme: &Scheme, state: &mut HangarState, adding_open: bool) {
    /// The least width the note keeps beside the buttons; below that the buttons go under it.
    const LEAST_NOTE: f32 = 240.0;
    /// What a button adds to its label: the icon with its gap and the frame (as in `widgets::Button`).
    const BUTTON_EXTRA: f32 = widgets::BUTTON_ICON_SIZE + 6.0 + 30.0;
    let arrow = |open: bool| if open { "chevron-down" } else { "chevron-right" };
    let (fleet_word, adding_word) = (t("s_fa_titel"), t("s_hgl_hinzufuegen"));
    let (fleet_open, clicks) = (state.fleet.open, std::cell::Cell::new((false, false)));
    // Made in the order they stand (note, "getting ships", "fleet analysis"), so that Tab goes the way the eye does.
    let buttons = |ui: &mut Ui| {
        let adding = |ui: &mut Ui| {
            let button = widgets::Button::new(&adding_word, ButtonKind::Normal, scheme).icon(arrow(adding_open));
            if ui.add(button).clicked() {
                clicks.set((true, clicks.get().1));
            }
        };
        let fleet = |ui: &mut Ui| {
            let button = widgets::Button::new(&fleet_word, ButtonKind::Normal, scheme).icon(arrow(fleet_open));
            if ui.add(button).clicked() {
                clicks.set((clicks.get().0, true));
            }
        };
        adding(ui);
        fleet(ui);
    };
    widgets::card(ui, scheme, |card| {
        let font = egui::TextStyle::Button.resolve(card.style());
        let wide = |text: &str| {
            let shown = widgets::button_label(scheme, ButtonKind::Normal, text);
            card.painter().layout_no_wrap(shown, font.clone(), egui::Color32::WHITE).size().x + BUTTON_EXTRA
        };
        let spacing = card.spacing().item_spacing.x;
        let buttons_width = spacing * 2.0 + wide(&fleet_word) + wide(&adding_word);
        let note_width = card.available_width() - buttons_width;
        if note_width >= LEAST_NOTE {
            // The note takes what the buttons leave, so that they stand at the right edge. (A layout of its own
            // straight in the card would take the whole height left and centre the row in it.)
            card.horizontal(|line| {
                let height = line.spacing().interact_size.y;
                line.allocate_ui_with_layout(
                    egui::vec2(note_width, height),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |hint| small(hint, &t("s_hg_hinweis"), scheme.sub),
                );
                buttons(line);
            });
        } else {
            small(card, &t("s_hg_hinweis"), scheme.sub);
            card.add_space(4.0);
            card.horizontal_wrapped(buttons);
        }
    });
    let (adding_clicked, fleet_clicked) = clicks.get();
    if adding_clicked {
        state.adding_open = Some(!adding_open);
    }
    if fleet_clicked {
        state.fleet.open = !fleet_open;
    }
}

/// The heading with the ship count and one striped row per ship.
fn ship_list(ui: &mut Ui, scheme: &Scheme, state: &mut HangarState, sources: Option<&dyn CartSources>) {
    state.fill_slots();
    let ships = hangar::ships(&state.held.data);
    // The fixed search, or else the ship being entered.
    let search = match state.search.trim() {
        "" => state.hand_name.trim().to_string(),
        s => s.to_string(),
    };
    let names = &mut state.names;
    names.prepare(&ships);
    if search != state.ordered_for {
        names.reorder();
        state.ordered_for = search.clone();
    }
    // As at the terminal: the star first, then own names, then the rest.
    let mut order: Vec<(usize, (u8, String, String))> = (0..ships.len())
        .map(|i| {
            let name = hangar::field_text(ships[i], "name");
            let (level, own) = names.rank(name);
            let lower = name.to_lowercase();
            (i, (level, if own.is_empty() { lower.clone() } else { own.to_lowercase() }, lower))
        })
        .collect();
    order.sort_by(|(a, x), (b, y)| {
        let (p, q) = (hangar::field_text(ships[*a], "name"), hangar::field_text(ships[*b], "name"));
        x.cmp(y).then_with(|| p.cmp(q))
    });
    // The search also finds the own name in the game.
    let wanted = search.to_lowercase();
    let shown: Vec<usize> = order
        .into_iter()
        .map(|(i, _)| i)
        .filter(|&i| {
            matches(ships[i], &search)
                || names.own_name(hangar::field_text(ships[i], "name")).to_lowercase().contains(&wanted)
        })
        .collect();
    let head = if search.is_empty() || ships.is_empty() {
        tf("s_hg_meine", &[("n", Arg::from(ships.len()))])
    } else {
        tf("s_hg_meine_gefiltert", &[("n", Arg::from(shown.len())), ("alle", Arg::from(ships.len()))])
    };
    // The head of the own ships like the one of the planned ones below them.
    widgets::heading(ui, scheme, &head);
    ui.add_space(4.0);
    if ships.is_empty() {
        widgets::empty_state(ui, scheme, "plane", &t("s_hg_leer"));
        return;
    }
    if shown.is_empty() {
        let text = tf("s_hg_filter_leer", &[("text", Arg::from(search.as_str()))]);
        widgets::empty_state(ui, scheme, "plane", &text);
        return;
    }
    let source = match state.erkul.game_version() {
        "" => t("s_hg_keine_daten"),
        version => tf("s_hg_quelle", &[("version", Arg::from(version))]),
    };
    small(ui, &source, scheme.sub);
    if state.names.without_language_file() {
        small(ui, &t("s_hgn_keine_ini"), scheme.gold);
    } else if state.names.can_write() {
        // `pages._ship_names` foot: the names into the game by hand.
        small(ui, &t("s_as_patch_hinweis"), scheme.sub);
        let busy = state.names.writing().then(|| t("s_as_laeuft"));
        let label = t("s_as_einspielen");
        let write = widgets::Button::new(&label, widgets::ButtonKind::Normal, scheme).blocked(busy.as_deref());
        if ui.add(write).clicked() {
            state.names.write_now();
        }
        if let Some((text, bad)) = state.names.written() {
            small(ui, text, if *bad { scheme.red_text() } else { scheme.sub });
        }
        if state.names.writing() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
        }
    }
    state.names.footer(ui, scheme);
    ui.add_space(4.0);
    let mut chosen: Option<(usize, RowAction)> = None;
    let mut names = std::mem::take(&mut state.names);
    let mut equipment = std::mem::take(&mut state.equipment);
    let (stores, extra) = (state.stores.clone(), state.extra.clone());
    let without = shown.iter().filter(|&&i| !slots_part(state, ships[i]).2).count();
    let mut measured: Vec<(usize, f32)> = Vec::new();
    // One card per ship, as on the sample page, with room between the cards.
    ui.scope(|list| {
        // The list above already keeps the margin of the page (`SAFETY`); taking it off twice made the cards
        // two points narrower than the other boxes of the page.
        list.set_width(list.available_width());
        let width = list.available_width();
        for &i in &shown {
            // A card out of sight only takes its room, at the height it last had.
            if let Some(&height) = state.heights.get(&i) {
                let rect = egui::Rect::from_min_size(list.cursor().min, egui::vec2(width, height));
                if !list.is_rect_visible(rect) {
                    list.allocate_space(egui::vec2(width, height));
                    list.add_space(CARD_GAP);
                    continue;
                }
            }
            let card = widgets::card(list, scheme, |cell| {
                cell.set_width(cell.available_width());
                let others = |entry: &Map| same_type_loadouts(&state.held.data, &stores, entry);
                let ctx = sources.map(|sources| equipment_box::Ctx {
                    stores: &stores,
                    extra: &extra,
                    sources,
                    paths: state.paths.as_ref(),
                    others: Some(&others),
                });
                let equip = ctx.as_ref().map(|ctx| (ctx, &mut equipment));
                ship_row(cell, scheme, state, &mut names, &ships, i, equip)
            });
            if let Some(action) = card.inner {
                chosen = Some((i, action));
            }
            measured.push((i, card.response.rect.height()));
            list.add_space(CARD_GAP);
        }
    });
    state.names = names;
    state.equipment = equipment;
    state.look_up_prices(ui);
    state.heights.extend(measured);
    if without > 0 {
        ui.add_space(6.0);
        small(ui, &tf("s_hgl_ohne_erklaert", &[("n", Arg::from(without))]), scheme.sub);
    }
    let Some((i, action)) = chosen else { return };
    match action {
        RowAction::Remove(name, maker) => state.remove(&name, &maker),
        RowAction::Ready(on) => {
            let mut question = None;
            if let Some(entry) = hangar::ship_mut(&mut state.held.data, i) {
                map_set(entry, "einsatzbereit", Value::Bool(on));
                if !on {
                    // Switched off with the plan complete: it does not go on again by itself for this plan.
                    hangar::note_switched_off(entry);
                }
                // The switch is set at once; with parts open the player is asked whether they count as fitted.
                if on && hangar::open_count(entry) > 0 {
                    let (name, maker) = (hangar::field_text(entry, "name"), hangar::field_text(entry, "hersteller"));
                    question = Some(ReadyQuestion { index: i, name: name.to_string(), maker: maker.to_string() });
                }
                state.save();
            }
            state.ready_question = question;
            ui.ctx().request_repaint();
        }
        RowAction::Melt(choice) => {
            if let Some(entry) = hangar::ship_mut(&mut state.held.data, i)
                && hangar::set_melt_choice(entry, &choice)
            {
                state.save();
            }
        }
        RowAction::Unit(pick) => state.set_unit(i, pick),
        RowAction::Craft(name) => state.craft_jump = Some(name),
        RowAction::ToPlanned => state.move_to_planned(i),
        RowAction::Equip(asked) => {
            let entry = hangar::ships(&state.held.data).get(i).map(|e| (*e).clone()).unwrap_or_default();
            if state.share_action(ui, &entry, &asked) {
                return;
            }
            let (stores, extra) = (state.stores.clone(), state.extra.clone());
            let mut went_ready = None;
            if let Some(entry) = hangar::ship_mut(&mut state.held.data, i) {
                let was_fitted = hangar::fully_fitted(entry);
                if change_plan(entry, &asked, &stores, &extra, &mut state.equipment) {
                    if ready_after_tick(entry, was_fitted, &asked) {
                        went_ready = Some(hangar::field_text(entry, "name").to_string());
                    }
                    // The card was drawn with the old height.
                    state.heights.clear();
                    state.save();
                }
            }
            if let Some(name) = went_ready {
                let said = tf("s_hg_einsatz_auto", &[("schiff", Arg::from(name))]);
                state.moved = Some((said, "circle-check", None));
            }
            ui.ctx().request_repaint();
        }
    }
}

/// The last part ticked off puts the ship on ready (`was_fitted`: the plan was complete before the change); a tick
/// taken back and any other change of the plan do not. True when the ship went on ready.
fn ready_after_tick(entry: &mut Map, was_fitted: bool, asked: &equipment_box::Action) -> bool {
    matches!(asked, equipment_box::Action::Tick { on: true, .. }) && hangar::ready_when_fitted(entry, was_fitted)
}

/// Whether the ship at `index` of the hangar is the one the question was asked about.
fn is_the_asked_ship(data: &Map, asked: &ReadyQuestion) -> bool {
    hangar::ships(data).get(asked.index).is_some_and(|entry| {
        hangar::field_text(entry, "name") == asked.name && hangar::field_text(entry, "hersteller") == asked.maker
    })
}

/// The question after a ship was put on ready with parts of its list open: tick them all off as fitted, or leave
/// the list as it is ("No", Esc or a click beside the dialog; the switch stays on either way). Only while the
/// ship is the one asked about and has parts open.
fn ready_question(ui: &mut Ui, scheme: &Scheme, state: &mut HangarState) {
    let Some(asked) = &state.ready_question else { return };
    let open = if is_the_asked_ship(&state.held.data, asked) {
        hangar::ships(&state.held.data).get(asked.index).map_or(0, |entry| hangar::open_count(entry))
    } else {
        0
    };
    if open == 0 {
        state.ready_question = None;
        return;
    }
    let title = t("s_hg_einsatz_frage_titel");
    let text = tf("s_hg_einsatz_frage", &[("schiff", Arg::from(asked.name.as_str())), ("n", Arg::from(open))]);
    let ((yes, no), close) = widgets::dialog(ui.ctx(), scheme, "einsatzbereit-frage", |d| {
        widgets::heading(d, scheme, &title);
        d.add(egui::Label::new(text.as_str()).wrap());
        d.add_space(8.0);
        d.horizontal(|row| {
            let no = row.add(widgets::Button::new(&t("e_nein"), ButtonKind::Normal, scheme)).clicked();
            let yes = row.add(widgets::Button::new(&t("e_ja"), ButtonKind::Strong, scheme)).clicked();
            (yes, no)
        })
        .inner
    });
    if yes {
        if let Some(asked) = state.ready_question.take()
            && is_the_asked_ship(&state.held.data, &asked)
            && let Some(entry) = hangar::ship_mut(&mut state.held.data, asked.index)
            && hangar::tick_all(entry) > 0
        {
            // The card was drawn with the old height.
            state.heights.clear();
            state.save();
        }
        ui.ctx().request_repaint();
    } else if no || close {
        state.ready_question = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_takes_every_word_on_its_own_and_not_the_package() {
        let entry = match jsonio::loads(r#"{"name": "ATLS IKTI Akuma", "hkurz": "ARGO", "paket": "Carrack"}"#) {
            Ok(Value::Object(m)) => m,
            _ => panic!(),
        };
        assert!(matches(&entry, "argo a.t.l.s"));
        assert!(matches(&entry, "  "));
        assert!(!matches(&entry, "carrack"));
        assert!(!matches(&entry, "argo titan"));
    }

    #[test]
    fn details_name_origin_insurance_and_package() {
        steady();
        let state = HangarState::default();
        let ships = hangar::ships(&state.held.data);
        let line = |name: &str| {
            let i = ships.iter().position(|s| hangar::field_text(s, "name") == name).unwrap();
            details(&ships, i)
        };
        assert_eq!(
            line("Cutlass Black"),
            format!("{}  ·  {}", t("s_hg_pledge"), tf("s_hg_vers_jahre", &[("n", Arg::from(10usize))]))
        );
        assert!(line("Gladius").contains(&tf("s_hg_beilage", &[("schiff", Arg::from("Carrack"))])));
        assert!(!line("C8 Pisces").contains(&tf("s_hg_beilage", &[("schiff", Arg::from("Carrack"))])));
        assert!(line("Polaris").contains("Fleetyards"));
    }

    const TWO: &str = r#"{"format": 1, "schiffe": [
        {"name": "Carrack", "hersteller": "Anvil", "herkunft": "pledge", "belegung": {}},
        {"name": "Gladius", "hersteller": "Aegis", "herkunft": "pledge", "belegung": {}}]}"#;

    /// Pins this test thread to the plain scheme: other tests switch the scheme of the whole program meanwhile,
    /// and a page drawn in another scheme between two frames has its rows elsewhere.
    fn steady_scheme() {
        versekit_core::theme::set_thread_current("original");
    }

    /// Pins this test thread to German and the plain scheme, for the tests that compare a shown text with the
    /// language file without holding the language lock.
    fn steady() {
        steady_scheme();
        versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
    }

    /// The page drawn in a window of `width`; the texts with their places.
    fn head_drawn(
        ctx: &egui::Context,
        state: &mut HangarState,
        width: f32,
        events: Vec<egui::Event>,
    ) -> Vec<(String, egui::Pos2)> {
        fn collect(shape: &egui::Shape, into: &mut Vec<(String, egui::Pos2)>) {
            match shape {
                egui::Shape::Text(text) => into.push((text.galley.text().to_string(), text.pos)),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, into)),
                _ => {}
            }
        }
        let scheme = versekit_core::theme::current();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 2000.0))),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| show_page(ui, scheme, state));
        output.textures_delta.clear();
        let mut texts = Vec::new();
        output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
        texts
    }

    /// Clicks the text `label` of the page and lets the page settle.
    fn click_on(ctx: &egui::Context, state: &mut HangarState, label: &str) -> Vec<(String, egui::Pos2)> {
        let mut texts = Vec::new();
        for _ in 0..3 {
            texts = head_drawn(ctx, state, 1100.0, Vec::new());
        }
        let at = texts
            .iter()
            .find(|(text, _)| text == label)
            .map(|(_, at)| *at + egui::vec2(3.0, 3.0))
            .unwrap_or_else(|| panic!("{label} is not on the page: {texts:?}"));
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        head_drawn(ctx, state, 1100.0, vec![egui::Event::PointerMoved(at)]);
        head_drawn(ctx, state, 1100.0, vec![button(true)]);
        head_drawn(ctx, state, 1100.0, vec![button(false)]);
        for _ in 0..3 {
            texts = head_drawn(ctx, state, 1100.0, Vec::new());
        }
        texts
    }

    /// The head of the page is one slim card: the note on the left, the two buttons on the right; each button
    /// opens its part below and closes it again, and neither part is built before.
    #[test]
    fn the_head_is_one_card_with_the_note_and_two_buttons_that_open_the_parts() {
        let _language = crate::language_lock();
        steady();
        let (mut state, dir) = page_on("kopf", TWO);
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let has = |texts: &[(String, egui::Pos2)], key: &str| texts.iter().any(|(text, _)| *text == t(key));
        let at = |texts: &[(String, egui::Pos2)], key: &str| {
            texts.iter().find(|(text, _)| *text == t(key)).map(|(_, at)| *at).unwrap_or_else(|| panic!("{key}"))
        };
        let mut texts = Vec::new();
        for _ in 0..3 {
            texts = head_drawn(&ctx, &mut state, 1100.0, Vec::new());
        }
        // Two ships: both parts are closed. The note left of the buttons, the buttons side by side.
        let head = ["s_hg_hinweis", "s_hgl_hinzufuegen", "s_fa_titel"];
        assert!(head.iter().all(|key| has(&texts, key)), "{texts:?}");
        assert!(!has(&texts, "s_hg_import_titel") && !has(&texts, "s_fav_text"), "closed: nothing built");
        let (note, adding, fleet) = (at(&texts, head[0]), at(&texts, head[1]), at(&texts, head[2]));
        assert!(note.x < adding.x && adding.x < fleet.x, "{note:?} {adding:?} {fleet:?}");
        assert!((adding.y - fleet.y).abs() < 2.0, "the buttons stand in one line: {adding:?} {fleet:?}");
        // The fleet analysis opens by its button and closes by it, the other part stays shut.
        let texts = click_on(&ctx, &mut state, &t("s_fa_titel"));
        assert!(has(&texts, "s_fav_text") && !has(&texts, "s_hg_import_titel"), "{texts:?}");
        let texts = click_on(&ctx, &mut state, &t("s_fa_titel"));
        assert!(!has(&texts, "s_fav_text"), "closed again");
        // Getting or entering ships opens by its button the same way.
        let texts = click_on(&ctx, &mut state, &t("s_hgl_hinzufuegen"));
        assert!(has(&texts, "s_hg_import_titel") && !has(&texts, "s_fav_text"), "{texts:?}");
        let texts = click_on(&ctx, &mut state, &t("s_hgl_hinzufuegen"));
        assert!(!has(&texts, "s_hg_import_titel"), "closed again");
        // Too narrow for the note beside the buttons: the buttons stand under the note (the second one beside the
        // first or, where the window has no room for both, under it).
        let mut narrow = Vec::new();
        for _ in 0..3 {
            narrow = head_drawn(&ctx, &mut state, 380.0, Vec::new());
        }
        let (note, adding, fleet) = (at(&narrow, head[0]), at(&narrow, head[1]), at(&narrow, head[2]));
        assert!(adding.y > note.y + 8.0, "the buttons stand under the note: {note:?} {adding:?}");
        assert!(fleet.y > adding.y - 2.0, "the fleet button is not above the first: {adding:?} {fleet:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A page on `hangar.json` in a new folder, with a UEX list of two ships.
    fn page_on(name: &str, start: &str) -> (HangarState, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("versekit-hangarseite-{name}-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        let file = dir.join(hangar::FILE).to_string_lossy().into_owned();
        std::fs::write(&file, start).expect("start");
        let uex_file = dir.join(ships::UEX_FILE);
        let list = r#"{"format": 5, "schiffe": {"1": {"name": "Aegis Avenger Titan"}, "2": {"name": "Anvil Arrow"}}}"#;
        std::fs::write(&uex_file, list).expect("UEX");
        let uex = UexShips::load(&uex_file.to_string_lossy());
        let mut state = HangarState { file: Some(file), all_names: uex.choice_names(), uex, ..HangarState::default() };
        state.reload();
        (state, dir)
    }

    #[test]
    fn a_ship_that_uex_lists_twice_is_offered_once_in_the_field_to_enter_a_ship() {
        use versekit_core::paths::Host;
        use versekit_core::pypath::Flavor;
        let dir = std::env::temp_dir().join(format!("versekit-hangar-doppelt-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        let home = dir.to_string_lossy().into_owned();
        let paths = Paths::new(Host::new(Flavor::native(), &[(versekit_core::legacy::ENV_HOME, &home)], &home));
        // UEX gives the Sabre Raven EX under two ids with one name.
        let list = r#"{"format": 5, "schiffe": {"290": {"name": "Aegis Sabre Raven EX"},
                       "291": {"name": "Aegis Sabre Raven EX"}, "3": {"name": "Aegis Sabre"}}}"#;
        std::fs::write(paths.app_file(ships::UEX_FILE), list).expect("UEX");
        let state = HangarState::with_file(&paths);
        assert_eq!(state.all_names, ["Aegis Sabre", "Aegis Sabre Raven EX"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn ship_names(state: &HangarState) -> Vec<String> {
        hangar::ships(&state.held.data).iter().map(|s| hangar::field_text(s, "name").to_string()).collect()
    }

    #[test]
    fn a_failed_save_after_an_import_stays_red() {
        steady();
        // Ships bought in the game: the import removes none, no backup is due.
        let (mut state, dir) = page_on("importfehler", &TWO.replace("pledge", "ingame"));
        let list = dir.join("liste.json");
        let ship = r#"{"matrix": "Arrow", "matrixZuordnung": "sicher", "name": "Arrow", "code": "ANVL_Arrow",
            "manufacturer": {"code": "ANVL", "name": "Anvil"}}"#;
        std::fs::write(&list, format!(r#"{{"schema": "hangar-schiffe", "version": 1, "schiffe": [{ship}]}}"#))
            .expect("list");
        // Writing only elsewhere: the save fails.
        versekit_core::writeguard::deny(true);
        versekit_core::writeguard::allow_under(Some(&std::env::temp_dir().join("anderswo").to_string_lossy()));
        state.import(&list.to_string_lossy());
        versekit_core::writeguard::deny(false);
        versekit_core::writeguard::allow_under(None);
        let _ = versekit_core::writeguard::take_denied();
        assert_eq!(state.message, Some((t("s_hgl_nicht_gespeichert"), Tone::Bad)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn removing_keeps_a_backup_first() {
        steady();
        let (mut state, dir) = page_on("sicherung", TWO);
        state.remove("Gladius", "Aegis");
        assert_eq!(std::fs::read_to_string(dir.join("hangar.bak.json")).expect("backup"), TWO);
        assert_eq!(ship_names(&state), vec!["Carrack"]);

        // No backup possible: nothing is removed.
        std::fs::remove_file(dir.join("hangar.bak.json")).expect("away");
        std::fs::create_dir(dir.join("hangar.bak.json")).expect("in the way");
        state.remove("Carrack", "Anvil");
        assert_eq!(state.message, Some((t("s_hgl_keine_sicherung"), Tone::Bad)));
        assert_eq!(ship_names(&state), vec!["Carrack"]);
        let on_disk = hangar::read(&dir.join(hangar::FILE).to_string_lossy()).expect("file");
        assert_eq!(hangar::ships(&on_disk).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_ship_by_hand_is_stored_as_uex_writes_it() {
        let (mut state, dir) = page_on("handname", TWO);
        state.hand_name = "  avenger titan ".into();
        state.enter_by_hand();
        assert!(ship_names(&state).contains(&"Aegis Avenger Titan".to_string()), "{:?}", ship_names(&state));
        assert!(state.hand_name.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn under_the_names_stands_how_many_ships_can_be_renamed_and_which_not() {
        steady_scheme();
        use versekit_core::language::{Choice, set_thread_language};
        let _language = crate::language_lock();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        // The sample: seven ships, four of them in its language file.
        let mut state = HangarState::default();
        let without = "C8 Pisces, Freelancer MAX, Polaris";
        for choice in [Choice::De, Choice::En] {
            set_thread_language(choice);
            let (labels, _, _) = frame(&ctx, &mut state, Vec::new());
            let shown = |text: &str| labels.iter().any(|(_, label)| label == text);
            let stand = tf("s_as_stand", &[("0", Arg::from(4)), ("1", Arg::from(7))]);
            let ohne = tf("s_as_ohne", &[("0", Arg::from(without))]);
            assert!(shown(&stand), "{choice:?} {stand}: {labels:?}");
            assert!(shown(&ohne), "{choice:?} {ohne}: {labels:?}");
            for sentence in [&stand, &ohne] {
                assert!(!sentence.contains('—'), "{sentence}");
                for part in sentence.split(". ") {
                    assert!(part.split_whitespace().count() <= 10, "{choice:?}: {part}");
                }
            }
        }
        set_thread_language(Choice::De);
        assert!(tf("s_as_stand", &[("0", Arg::from(4)), ("1", Arg::from(7))]).starts_with("4 von 7"));
        // Without the table of the language file neither line.
        state.names = ShipNames::default();
        let (labels, _, _) = frame(&ctx, &mut state, Vec::new());
        assert!(!labels.is_empty());
        assert!(!labels.iter().any(|(_, label)| label.contains("lassen sich benennen")), "{labels:?}");
        assert!(!labels.iter().any(|(_, label)| label.starts_with("Ohne Zuordnung")), "{labels:?}");
    }

    #[test]
    fn the_page_asks_for_slots_after_an_import_a_ship_by_hand_and_on_entering() {
        steady_scheme();
        use crate::slot_sync::tests::{Counter, WAIT};
        let _language = crate::language_lock();
        let (mut state, dir) = page_on("steckplaetze", &TWO.replace("pledge", "ingame"));
        let home = dir.to_string_lossy().into_owned();
        let host = versekit_core::paths::Host::new(
            versekit_core::pypath::Flavor::native(),
            &[(versekit_core::legacy::ENV_HOME, home.as_str())],
            &home,
        );
        state.paths = Some(Paths::new(host));
        let counter = Counter::new("4.3");
        state.sync = counter.sync();
        // An import asks; one that failed does not.
        let list = dir.join("liste.json");
        let ship = r#"{"matrix": "Arrow", "matrixZuordnung": "sicher", "name": "Arrow", "code": "ANVL_Arrow",
            "manufacturer": {"code": "ANVL", "name": "Anvil"}}"#;
        std::fs::write(&list, format!(r#"{{"schema": "hangar-schiffe", "version": 1, "schiffe": [{ship}]}}"#))
            .expect("list");
        state.import(&list.to_string_lossy());
        assert_eq!(state.sync.asked(Trigger::Import), 1);
        state.import(&dir.join("fehlt.json").to_string_lossy());
        assert_eq!(state.sync.asked(Trigger::Import), 1, "a failed import asks nothing");
        // A ship by hand asks; a name the list does not know does not.
        state.hand_name = "avenger titan".into();
        state.enter_by_hand();
        assert_eq!(state.sync.asked(Trigger::ByHand), 1);
        state.hand_name = "Nothing at all".into();
        state.enter_by_hand();
        assert_eq!(state.sync.asked(Trigger::ByHand), 1, "an unknown name asks nothing");
        assert!(state.sync.wait_idle(WAIT));
        assert!(counter.catalogs() >= 1, "the requests reached the feed");
        // Entering the page asks once; staying on it does not; entering again asks again.
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let scheme = versekit_core::theme::current();
        let draw = |state: &mut HangarState| {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| show_page(ui, scheme, state));
            output.textures_delta.clear();
        };
        draw(&mut state);
        draw(&mut state);
        assert_eq!(state.sync.asked(Trigger::PageOpen), 1);
        // A frame on another page.
        let _ = ctx.run_ui(egui::RawInput::default(), |_| {});
        draw(&mut state);
        assert_eq!(state.sync.asked(Trigger::PageOpen), 2);
        assert!(state.sync.wait_idle(WAIT));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_wish_list_takes_a_ship_of_the_list_with_its_maker_and_says_why_it_refuses_another() {
        steady_scheme();
        let _language = crate::language_lock();
        versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
        let (mut state, dir) = page_on(
            "wunsch",
            r#"{"format": 1, "schiffe": [{"name": "Avenger Titan", "herkunft": "pledge", "belegung": {}}]}"#,
        );
        let said = |state: &HangarState| state.wish_message.clone().map(|m| m.0);
        let home = dir.to_string_lossy().into_owned();
        let vars = [(versekit_core::legacy::ENV_HOME, home.as_str()), (versekit_core::legacy::ENV_NO_NET, "1")];
        state.paths =
            Some(Paths::new(versekit_core::paths::Host::new(versekit_core::pypath::Flavor::native(), &vars, &home)));
        let counter = crate::slot_sync::tests::Counter::new("4.3");
        state.sync = counter.sync();
        // Not a ship of the list.
        state.wish_name = "Nothing at all".into();
        state.enter_wish();
        assert_eq!(said(&state), Some(t("s_hg_kein_name")));
        // One that stands in the hangar: one does not wish for what one has.
        state.wish_name = "Avenger Titan".into();
        state.enter_wish();
        assert_eq!(said(&state), Some(t("s_hg_wunsch_schon_da")));
        assert!(hangar::wishlist(&state.held.data).is_empty());
        // A ship of the list: stored as typed, with the maker UEX has in the name; the file has it.
        state.wish_name = "Arrow".into();
        state.enter_wish();
        assert_eq!(said(&state), Some(tf("s_hg_wunsch_notiert", &[("name", Arg::from("Arrow"))])));
        assert!(state.wish_name.is_empty());
        let wished = hangar::wishlist(&state.held.data);
        assert_eq!(
            (hangar::field_text(wished[0], "name"), hangar::field_text(wished[0], "hersteller")),
            ("Arrow", "Anvil")
        );
        let file = dir.join(hangar::FILE);
        assert!(std::fs::read_to_string(&file).expect("file").contains("\"wunsch\""));
        // The same again.
        state.wish_name = "arrow".into();
        state.enter_wish();
        assert_eq!(said(&state), Some(t("s_hg_wunsch_doppelt")));
        // Only the ship that was noted asked for its slots.
        assert_eq!(state.sync.asked(Trigger::Wish), 1, "the planned ship asks for its slots, the refused ones not");
        // What that fetch does, on a made-up erkul: the wish is found with its maker and stored.
        assert!(state.sync.wait_idle(crate::slot_sync::tests::WAIT));
        assert_eq!(counter.catalogs(), 1);
        let paths = Paths::new(state.paths.as_ref().expect("paths").host.clone());
        let erkul = ErkulShips::load(&paths.app_file(ships::ERKUL_FILE));
        let arrow = versekit_core::ships::ShipId { name: "Arrow", maker: "Anvil", short: "", maker_short: "" };
        assert_eq!(erkul.slot_count(arrow), Some(1));
        // The list shows the ship with the sentence for "no price", and striking takes it off the file too.
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let scheme = versekit_core::theme::current();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| show_planned(ui, scheme, &mut state, None));
        output.textures_delta.clear();
        let mut shown = Vec::new();
        fn collect(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
            match shape {
                egui::epaint::Shape::Text(text) => out.push(text.galley.text().to_string()),
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, out)),
                _ => {}
            }
        }
        output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut shown));
        assert!(shown.contains(&tf("s_hg_wunsch_meine", &[("n", Arg::Int(1))])), "{shown:?}");
        assert!(shown.contains(&"Arrow".to_string()) && shown.contains(&t("s_hg_wunsch_kein_preis")), "{shown:?}");
        assert!(hangar::wishlist_remove(&mut state.held.data, "ARROW"));
        state.save();
        assert!(!std::fs::read_to_string(&file).expect("file").contains("Arrow"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_ship_planned_from_the_shops_is_stored_like_the_one_entered_in_the_section() {
        let _language = crate::language_lock();
        versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
        let start = r#"{"format": 1, "schiffe": [{"name": "Avenger Titan", "herkunft": "pledge", "belegung": {}}]}"#;
        let (mut entered, dir_a) = page_on("plan-eingetragen", start);
        let (mut planned, dir_b) = page_on("plan-aus-laeden", start);
        entered.wish_name = "Arrow".into();
        entered.enter_wish();
        let said = planned.plan_ship_said("Arrow");
        assert_eq!(said, (tf("s_hg_im_hangar_vorgemerkt", &[("0", Arg::from("Arrow"))]), false));
        assert_eq!(hangar::wishlist(&entered.held.data), hangar::wishlist(&planned.held.data), "the same entry");
        assert!(std::fs::read_to_string(dir_b.join(hangar::FILE)).expect("file").contains("\"Anvil\""), "with maker");
        assert!(planned.is_planned("arrow"), "found as the list finds a name");
        // Counter-check: a ship not planned is not found as planned.
        assert!(!planned.is_planned("Avenger Titan"));
        // The same again, a ship of the hangar, and a name UEX does not list: each refused and said.
        assert_eq!(planned.plan_ship_said("Arrow"), (t("s_hg_wunsch_doppelt"), true));
        assert_eq!(planned.plan_ship_said("Avenger Titan"), (t("s_hg_wunsch_schon_da"), true));
        assert_eq!(planned.plan_ship_said("Nothing at all"), (t("s_hg_kein_name"), true));
        assert_eq!(hangar::wishlist(&planned.held.data).len(), 1, "nothing was added by the refusals");
        let _ = std::fs::remove_dir_all(&dir_a);
        let _ = std::fs::remove_dir_all(&dir_b);
    }

    #[test]
    fn the_hangar_page_ends_with_the_section_planned_and_scrolls_to_it_once_asked() {
        steady_scheme();
        let _language = crate::language_lock();
        versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let scheme = versekit_core::theme::current();
        let mut state = HangarState::default();
        hangar::wishlist_add(&mut state.held.data, "Arrow", "Anvil");
        let texts = |state: &mut HangarState| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 6000.0))),
                    ..Default::default()
                },
                |ui| show_page(ui, scheme, state),
            );
            output.textures_delta.clear();
            let mut shown = Vec::new();
            fn collect(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
                match shape {
                    egui::epaint::Shape::Text(text) => out.push(text.galley.text().to_string()),
                    egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, out)),
                    _ => {}
                }
            }
            output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut shown));
            shown
        };
        texts(&mut state);
        let shown = texts(&mut state);
        let position = |shown: &[String], text: &str| shown.iter().position(|s| s == text);
        let ships = tf("s_hg_meine", &[("n", Arg::from(hangar::ships(&state.held.data).len()))]);
        let (own, planned) = (position(&shown, &ships), position(&shown, &t("s_hg_geplant_titel")));
        assert!(own.is_some() && planned.is_some() && own < planned, "below the own ships: {shown:?}");
        assert!(position(&shown, &tf("s_hg_wunsch_meine", &[("n", Arg::from(1))])).is_some(), "{shown:?}");
        assert!(shown.contains(&"Arrow".to_string()), "{shown:?}");
        // Asked for: the page scrolls once and is done with it.
        assert!(!state.planned_pending());
        state.show_planned();
        assert!(state.planned_pending());
        texts(&mut state);
        assert!(!state.planned_pending());
    }

    #[test]
    fn a_ship_with_a_maker_in_the_hangar_is_not_wished_for_and_a_failed_save_is_said_on_the_list() {
        let _language = crate::language_lock();
        versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
        let (mut state, dir) = page_on(
            "wunsch-hersteller",
            r#"{"format": 1, "schiffe": [{"name": "Arrow", "hersteller": "Anvil Aerospace", "herkunft": "pledge", "belegung": {}}]}"#,
        );
        // UEX writes "Anvil Arrow"; the hangar has "Arrow" of "Anvil Aerospace".
        state.wish_name = "Arrow".into();
        state.enter_wish();
        assert_eq!(state.wish_message.clone().map(|m| m.0), Some(t("s_hg_wunsch_schon_da")));
        assert!(hangar::wishlist(&state.held.data).is_empty());
        // A save that fails: said on this page, the name stays in the field, nothing is reported as noted.
        let (mut state, dir2) = page_on("wunsch-fehler", r#"{"format": 1, "schiffe": []}"#);
        versekit_core::writeguard::deny(true);
        versekit_core::writeguard::allow_under(Some(&std::env::temp_dir().join("anderswo").to_string_lossy()));
        state.wish_name = "Arrow".into();
        state.enter_wish();
        versekit_core::writeguard::deny(false);
        versekit_core::writeguard::allow_under(None);
        let _ = versekit_core::writeguard::take_denied();
        assert_eq!(state.wish_message, Some((t("s_hgl_nicht_gespeichert"), Tone::Bad)));
        assert_eq!(state.wish_name, "Arrow");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&dir2);
    }

    #[test]
    fn a_change_of_the_plan_is_stored_in_the_form_of_the_exchange_and_builds_on_what_is_stored() {
        let entry_of = |text: &str| match jsonio::loads(text) {
            Ok(Value::Object(map)) => map,
            other => panic!("{other:?}"),
        };
        let set = |path: &str, reference: &str| equipment_box::Action::SetPart {
            paths: vec![path.to_string()],
            reference: reference.into(),
            name: reference.to_uppercase(),
        };
        let stores = Stores::empty();
        let (extra, mut box_state) = (Extra::default(), EquipmentState::default());
        let mut change = |entry: &mut Map, action: &equipment_box::Action, stores: &Stores| {
            change_plan(entry, action, stores, &extra, &mut box_state)
        };
        // A ship with a plan and no equipments yet: the change writes them.
        let mut ship = entry_of(r#"{"name": "Cutlass", "belegung": {}}"#);
        assert!(change(&mut ship, &set("c1", "cooler-9"), &stores));
        let stored = equipment::read_all(&ship, &[]);
        assert_eq!(stored.active().map(|e| e.slots.len()), Some(1), "{ship:?}");
        assert_eq!(stored.active().unwrap().slot("c1").unwrap().item_guid, "cooler-9");
        // The same change again changes nothing and says so (no write).
        assert!(!change(&mut ship, &set("c1", "cooler-9"), &stores));
        // A file written elsewhere: equipments, a quality, and no plan. The change is made on what is stored.
        let mut elsewhere = entry_of(
            r#"{"name": "Cutlass", "ausstattungen": [{"name": "Main", "active": true, "slots": [
                {"portPath": "s1", "itemGuid": "shield-1", "acquisition": "crafted", "quality": {"core": 800}}]}]}"#,
        );
        assert!(change(&mut elsewhere, &set("c1", "cooler-9"), &stores));
        let kept = equipment::read_all(&elsewhere, &[]);
        let active = kept.active().unwrap();
        assert_eq!(active.slots.len(), 2, "the stored slot stays, the new one comes");
        assert_eq!(active.slot("s1").unwrap().quality_at("core"), 800);
        // A wish is the same: its entry is changed the same way.
        let mut wish = entry_of(r#"{"name": "Carrack", "belegung": {}}"#);
        assert!(change(&mut wish, &set("c1", "cooler-9"), &stores));
        assert!(equipment::read_all(&wish, &[]).active().is_some());
    }

    #[test]
    fn the_named_equipments_and_the_way_a_part_was_got_are_changed_through_the_box() {
        use equipment::Acquisition;
        use equipment_box::Action;
        let mut ship = match jsonio::loads(
            r#"{"name": "Cutlass", "belegung": {"c1": {"ref": "cooler-9", "name": "Cooler 9", "weg": "kaufen"}}}"#,
        ) {
            Ok(Value::Object(map)) => map,
            other => panic!("{other:?}"),
        };
        let stores = Stores::empty();
        let extra = Extra {
            recipes: vec![equipment_box::RecipeInfo {
                entity: "cooler-9".into(),
                basis: "Cooler 9".into(),
                blueprint: "bp-cooler-9".into(),
                slots: vec!["Frame".into()],
            }],
            ..Extra::default()
        };
        let mut box_state = EquipmentState::default();
        let mut change = |ship: &mut Map, action: Action| change_plan(ship, &action, &stores, &extra, &mut box_state);
        let active = |ship: &Map| equipment::read_all(ship, &[]).active().cloned().expect("an active one");
        let paths = vec!["c1".to_string()];
        // A new one, renamed; back to the first; the new one deleted and put back.
        assert!(change(&mut ship, Action::Create { name: "Racing".into() }));
        assert!(change(&mut ship, Action::Rename { old: "Racing".into(), new: "Race".into() }));
        assert_eq!(active(&ship).name, "Race");
        assert!(change(&mut ship, Action::Switch { name: equipment::DEFAULT_NAME.into() }));
        assert_eq!(active(&ship).name, equipment::DEFAULT_NAME);
        assert!(change(&mut ship, Action::Delete { name: "Race".into(), key: "k".into() }));
        assert_eq!(equipment::names(&ship, &[]).0, [equipment::DEFAULT_NAME]);
        assert!(change(&mut ship, Action::Undo));
        assert_eq!(equipment::names(&ship, &[]).0, [equipment::DEFAULT_NAME, "Race"]);
        assert!(!change(&mut ship, Action::Undo), "put back once");
        // Built in the cart: the blueprint comes from the recipe, the quality is set per recipe slot.
        assert!(change(&mut ship, Action::Method { paths: paths.clone(), way: versekit_core::cart::CRAFT.into() }));
        let built = active(&ship).slot("c1").cloned().expect("the slot");
        assert_eq!((built.acquisition, built.blueprint_guid.as_deref()), (Acquisition::Crafted, Some("bp-cooler-9")));
        assert!(change(&mut ship, Action::Quality { paths: paths.clone(), slot: "Frame".into(), value: 640 }));
        assert_eq!(active(&ship).slot("c1").unwrap().quality_at("Frame"), 640);
        // A pledge is kept in the equipment; "buy" in the cart makes it bought again.
        assert!(change(
            &mut ship,
            Action::Acquire { paths: paths.clone(), acquisition: Acquisition::Pledge, blueprint: None }
        ));
        assert_eq!(active(&ship).slot("c1").unwrap().acquisition, Acquisition::Pledge);
        assert!(change(&mut ship, Action::Method { paths: paths.clone(), way: versekit_core::cart::BUY.into() }));
        assert_eq!(active(&ship).slot("c1").unwrap().acquisition, Acquisition::Purchased);
        // A delete is undone only until the next change.
        assert!(change(&mut ship, Action::Delete { name: "Race".into(), key: "k".into() }));
        assert!(change(&mut ship, Action::Create { name: "Third".into() }));
        assert!(!change(&mut ship, Action::Undo));
    }

    #[test]
    fn the_wish_list_with_cards_sticks_out_nowhere_in_both_languages() {
        steady_scheme();
        use versekit_core::language::{Choice as Lang, set_thread_language};
        let _language = crate::language_lock();
        let scheme = versekit_core::theme::current();
        let mut findings = Vec::new();
        for lang in [Lang::De, Lang::En] {
            set_thread_language(lang);
            for points in versekit_core::font_size::POINTS_RANGE.0..=versekit_core::font_size::POINTS_RANGE.1 {
                for width in [380.0_f32, 520.0, 900.0] {
                    let ctx = egui::Context::default();
                    crate::install(&ctx);
                    crate::apply_size(&ctx, points);
                    let mut state = HangarState::default();
                    for name in ["Arrow", "Constellation Andromeda Extra Long Edition With Added Words"] {
                        hangar::wishlist_add(&mut state.held.data, name, "Roberts Space Industries");
                    }
                    state.wish_message = Some((t("s_hg_wunsch_schon_da"), Tone::Bad));
                    let mut over = 0.0_f32;
                    for _ in 0..3 {
                        let mut output = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    egui::vec2(width, 3000.0),
                                )),
                                ..Default::default()
                            },
                            |ui| {
                                show_planned(ui, scheme, &mut state, None);
                                over = (ui.min_rect().right() - ui.ctx().content_rect().right()).max(0.0);
                            },
                        );
                        output.textures_delta.clear();
                    }
                    if over > 0.5 {
                        findings.push(format!("{lang:?} Größe {points} Breite {width}: {over:.0} zu breit"));
                    }
                }
            }
        }
        set_thread_language(Lang::De);
        assert!(findings.is_empty(), "{}", findings.join("\n"));
    }

    #[test]
    fn the_parts_of_the_open_boxes_without_a_shop_price_are_looked_up_once_and_in_the_background() {
        steady();
        let (mut state, dir) = page_on("preise", TWO);
        let data_dir = dir.to_string_lossy().into_owned();
        state.paths = Some(Paths::new(versekit_core::paths::Host::new(
            versekit_core::pypath::Flavor::native(),
            &[(versekit_core::legacy::ENV_HOME, &data_dir)],
            &data_dir,
        )));
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let ask = |state: &mut HangarState, wanted: &[(&str, &str)]| {
            state.equipment.missing = wanted.iter().map(|(c, n)| (c.to_string(), n.to_string())).collect();
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| state.look_up_prices(ui));
            output.textures_delta.clear();
        };
        // Nothing missing: no lookup. The list of the boxes is emptied each frame.
        ask(&mut state, &[]);
        assert!(state.lookup.job().is_none());
        // Two boxes want the same part: one lookup, the part asked once.
        ask(&mut state, &[("part-a", "Part A"), ("part-a", "Part A")]);
        assert!(state.lookup.job().is_some(), "the first frame starts the lookup");
        assert!(state.equipment.missing.is_empty());
        let end = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while state.lookup.job().is_some() && std::time::Instant::now() < end {
            ask(&mut state, &[]);
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(state.lookup.job().is_none(), "the lookup ended (the tests ask no net)");
        // Straight after: not asked again.
        ask(&mut state, &[("part-a", "Part A")]);
        assert!(state.lookup.job().is_none(), "no new lookup at the next frame");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn after_a_merge_the_page_reloads() {
        let (mut state, dir) = page_on("nachladen", TWO);
        let file = dir.join(hangar::FILE).to_string_lossy().into_owned();
        let elsewhere = TWO.replace(r#""name": "Gladius""#, r#""name": "Gladius", "einheit": "u1""#);
        std::fs::write(&file, &elsewhere).expect("other place");
        if let Some(entry) = hangar::ship_mut(&mut state.held.data, 0) {
            map_set(entry, "einsatzbereit", Value::Bool(true));
        }
        assert_eq!(state.save(), Saved::Merged);
        assert!(!hangar::changed_elsewhere(&file, &state.held), "the page holds what the file holds");
        assert_eq!(hangar::field_text(hangar::ships(&state.held.data)[1], "einheit"), "u1");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn changes_stay_in_the_window_without_own_data() {
        let mut state = HangarState::default();
        let before = hangar::ships(&state.held.data).len();
        assert!(hangar::remove(&mut state.held.data, "Polaris", "RSI"));
        assert!(state.save().ok());
        assert_eq!(hangar::ships(&state.held.data).len(), before - 1);
        state.hand_name = "Gibt es nicht".into();
        state.enter_by_hand();
        assert_eq!(state.message.as_ref().map(|m| m.1), Some(Tone::Bad));
    }

    /// Time per frame of the page with a real data folder (only read):
    /// `VERSEKIT_MESSORDNER=<folder with hangar.json, schiffe.json,
    /// erkul-schiffe.json, namen-original.json> cargo test --release -p versekit-ui bildzeit -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn bildzeit() {
        let Ok(folder) = std::env::var("VERSEKIT_MESSORDNER") else { return };
        let file = |name: &str| format!("{folder}/{name}");
        let uex = UexShips::load(&file(ships::UEX_FILE));
        // The names of the stored original texts stand in for the language file.
        let saved = match jsonio::read_json(&file(versekit_core::asop::NAMES_FILE)) {
            Ok(Value::Object(map)) => versekit_core::asop::saved_name_lines(&map),
            _ => Vec::new(),
        };
        let table = versekit_core::asop::factory_names(&versekit_core::asop::read_keys(&saved), &Map::new());
        println!("{} Namen in der Tabelle", table.len());
        let mut state = HangarState {
            held: Held::detached(hangar::read(&file(hangar::FILE)).unwrap_or_else(hangar::empty)),
            erkul: ErkulShips::load(&file(ships::ERKUL_FILE)),
            all_names: uex.choice_names(),
            uex,
            names: ShipNames::detached(table),
            ..HangarState::default()
        };
        let scheme = versekit_core::theme::current();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let frame = |state: &mut HangarState| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1226.0, 900.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| show_page(ui, scheme, state));
            output.textures_delta.clear();
        };
        for _ in 0..5 {
            frame(&mut state);
        }
        let rounds = 60;
        let start = std::time::Instant::now();
        for _ in 0..rounds {
            frame(&mut state);
        }
        let each = start.elapsed().as_secs_f64() * 1000.0 / f64::from(rounds);
        println!("{} Schiffe: {each:.2} ms je Bild", hangar::ships(&state.held.data).len());
    }

    /// Every own control of a ship row has a name for screen readers.
    #[test]
    fn row_controls_have_names() {
        steady();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        let scheme = versekit_core::theme::current();
        let mut state = HangarState::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| show_page(ui, scheme, &mut state));
        output.textures_delta.clear();
        let update = output.platform_output.accesskit_update.expect("accesskit is on");
        let labels: Vec<(Role, String)> =
            update.nodes.iter().filter_map(|(_, n)| n.label().map(|l| (n.role(), l.to_owned()))).collect();
        let has = |role: Role, name: &str| labels.iter().any(|(r, l)| *r == role && l == name);
        assert!(has(Role::CheckBox, &t("s_hg_einsatzbereit")), "{labels:?}");
        assert!(has(Role::ComboBox, &t("s_hgl_schmelzen")), "{labels:?}");
        assert!(has(Role::Button, &t("s_hg_entfernen")), "{labels:?}");
        assert!(has(Role::CheckBox, &t("s_as_stern")), "{labels:?}");
    }

    #[test]
    fn the_button_of_a_ship_asks_for_its_craftable_parts_once() {
        steady_scheme();
        let _language = crate::language_lock();
        versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
        let start = r#"{"format": 1, "schiffe": [{"name": "Arrow", "hersteller": "Anvil Aerospace"},
            {"name": "Avenger Titan", "hersteller": "Aegis Dynamics"}]}"#;
        let (mut state, dir) = page_on("craft-jump", start);
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        let scheme = versekit_core::theme::current();
        let run = |state: &mut HangarState, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 4000.0))),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| show_page(ui, scheme, state));
            output.textures_delta.clear();
            output.platform_output.accesskit_update.expect("accesskit is on").nodes
        };
        let nodes = run(&mut state, Vec::new());
        let spoken = tf("s_hg_herstellbar_fuer", &[("schiff", Arg::from("Avenger Titan"))]);
        assert_eq!(spoken, "Herstellbare Teile für Avenger Titan");
        let at = nodes
            .iter()
            .find(|(_, n)| n.role() == Role::Button && n.label() == Some(spoken.as_str()))
            .and_then(|(_, n)| n.bounds())
            .map(|b| egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32))
            .expect("every ship card has the button, named with its ship");
        assert_eq!(state.take_craft_jump(), None, "nothing is asked before the click");
        run(&mut state, click(at));
        assert_eq!(state.take_craft_jump().as_deref(), Some("Avenger Titan"));
        assert_eq!(state.take_craft_jump(), None, "taken once");
        let _ = std::fs::remove_dir_all(dir);
    }

    fn labels_of(ctx: &egui::Context, state: &mut HangarState) -> Vec<(Role, String)> {
        let scheme = versekit_core::theme::current();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1100.0, 900.0))),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| show_page(ui, scheme, state));
        output.textures_delta.clear();
        let update = output.platform_output.accesskit_update.expect("accesskit is on");
        update.nodes.iter().filter_map(|(_, n)| n.label().map(|l| (n.role(), l.to_owned()))).collect()
    }

    /// The unit of a ship: a choice only with the hangar area on and more than one unit; "never synced" is
    /// no unit; the choice goes into the hangar file and out with the next sync.
    #[test]
    fn the_unit_of_a_ship_is_offered_only_with_the_area_on_and_two_units_and_goes_into_the_file() {
        use versekit_core::basetool::exchange_double::ExchangeDouble;
        let _language = crate::language_lock();
        versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
        let dir = std::env::temp_dir()
            .join(format!("versekit-hangar-einheit-{}", versekit_core::testdir::run_tag()))
            .to_string_lossy()
            .replace('\\', "/");
        let _ = std::fs::remove_dir_all(&dir);
        let double = std::sync::Arc::new(ExchangeDouble::new());
        let wire = crate::basetool_page::Wire::over_double(&double, &dir);
        let paths = wire.sync.paths().clone();
        let file = paths.app_file(hangar::FILE);
        let text = r#"{"format": 1, "schiffe": [{"name": "Avenger Titan", "hersteller": "Aegis Dynamics",
            "herkunft": "pledge", "belegung": {}}]}"#;
        let Ok(Value::Object(data)) = jsonio::loads(text) else { panic!("hangar") };
        assert!(hangar::write(&file, &data));
        double.set_units(&[("u1", "Alpha")]);
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        let mut state = HangarState::with_file(&paths);
        state.connect(Some(crate::stock_share::Link::from_wire(&wire)));
        let named = |labels: &[(Role, String)]| labels.iter().any(|(r, l)| *r == Role::ComboBox && l == &t("s_ml_einheit"));
        // The area is off.
        assert!(!named(&labels_of(&ctx, &mut state)));
        assert!(wire.sync.set_setting(SETTING_SHIPS, Value::Bool(true)));
        wire.sync.pass_now().expect("pass");
        // One unit: whoever is in one gets it from the Basetool by itself.
        assert!(!named(&labels_of(&ctx, &mut state)), "one unit, nothing to choose");
        double.set_units(&[("u1", "Alpha"), ("u2", "Beta")]);
        wire.sync.pass_now().expect("pass");
        assert!(named(&labels_of(&ctx, &mut state)), "two units, a choice");

        let unit_of = |file: &str| {
            let data = hangar::load(file);
            hangar::ships(&data).first().and_then(|s| s.iter().find(|(k, _)| k == "einheit").map(|(_, v)| v.clone()))
        };
        // The unit goes into the file; "no unit" (empty) is not the same as "never synced" (no key).
        state.set_unit(0, UnitPick::Set("u2".into()));
        assert_eq!(unit_of(&file), Some(Value::str("u2")));
        state.set_unit(0, UnitPick::Set(String::new()));
        assert_eq!(unit_of(&file), Some(Value::str("")));
        state.set_unit(0, UnitPick::Back);
        assert_eq!(unit_of(&file), None);
        // A unit chosen here goes to the Basetool with the next sync.
        state.set_unit(0, UnitPick::Set("u2".into()));
        wire.sync.pass_now().expect("pass");
        wire.sync.pass_now().expect("pass");
        assert_eq!(double.ships().first().and_then(|s| s.unit.clone()), Some("u2".to_string()));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The import part says that the Basetool comes first only while the hangar is shared over it.
    #[test]
    fn the_import_says_the_basetool_comes_first_only_with_the_hangar_area_on() {
        steady_scheme();
        use versekit_core::basetool::exchange_double::ExchangeDouble;
        let _language = crate::language_lock();
        let dir = std::env::temp_dir()
            .join(format!("versekit-hangar-vorrang-{}", versekit_core::testdir::run_tag()))
            .to_string_lossy()
            .replace('\\', "/");
        let _ = std::fs::remove_dir_all(&dir);
        let double = std::sync::Arc::new(ExchangeDouble::new());
        let wire = crate::basetool_page::Wire::over_double(&double, &dir);
        let paths = wire.sync.paths().clone();
        let Ok(Value::Object(data)) = jsonio::loads(r#"{"format": 1, "schiffe": []}"#) else { panic!("hangar") };
        assert!(hangar::write(&paths.app_file(hangar::FILE), &data));
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        // Whether the hint is painted in a frame of the page.
        let shows = |state: &mut HangarState| {
            fn collect(shape: &egui::Shape, into: &mut Vec<String>) {
                match shape {
                    egui::Shape::Text(text) => into.push(text.galley.text().to_string()),
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, into)),
                    _ => {}
                }
            }
            let scheme = versekit_core::theme::current();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1100.0, 2000.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| show_page(ui, scheme, state));
            output.textures_delta.clear();
            let mut texts = Vec::new();
            output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
            texts.contains(&t("s_hg_import_basetool_2"))
        };
        versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
        let mut state = HangarState::with_file(&paths);
        assert!(state.adding_open.is_none_or(|open| open), "the import part is open on an empty hangar");
        assert!(!shows(&mut state), "no connection");
        state.connect(Some(crate::stock_share::Link::from_wire(&wire)));
        assert!(!shows(&mut state), "area off");
        assert!(wire.sync.set_setting(SETTING_SHIPS, Value::Bool(true)));
        wire.sync.pass_now().expect("pass");
        for choice in [versekit_core::language::Choice::De, versekit_core::language::Choice::En] {
            versekit_core::language::set_thread_language(choice);
            assert!(shows(&mut state), "area on");
        }
        versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A page on a hangar and an `asop.json` in a new folder.
    fn names_page(name: &str, hangar_text: &str, table: &[&str]) -> (HangarState, std::path::PathBuf, String) {
        let (mut state, dir) = page_on(name, hangar_text);
        let file = dir.join(versekit_core::asop::FILE).to_string_lossy().into_owned();
        state.names = ShipNames::on_file(&file, versekit_core::asop::read_keys(table));
        (state, dir, file)
    }

    /// One frame of the page; returns the accesskit nodes with a label and
    /// the places of the text fields and of the star switches.
    fn frame(
        ctx: &egui::Context,
        state: &mut HangarState,
        events: Vec<egui::Event>,
    ) -> (Vec<(Role, String)>, Vec<egui::Pos2>, Vec<egui::Pos2>) {
        let scheme = versekit_core::theme::current();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 4000.0))),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| show_page(ui, scheme, state));
        output.textures_delta.clear();
        let Some(update) = output.platform_output.accesskit_update else {
            return (Vec::new(), Vec::new(), Vec::new());
        };
        let labels = update
            .nodes
            .iter()
            .filter_map(|(_, n)| n.label().or(n.value()).map(|l| (n.role(), l.to_owned())))
            .collect();
        let places = |keep: &dyn Fn(&egui::accesskit::Node) -> bool| -> Vec<egui::Pos2> {
            let mut found: Vec<egui::Pos2> = update
                .nodes
                .iter()
                .filter(|(_, n)| keep(n))
                .filter_map(|(_, n)| n.bounds())
                .map(|b| egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32))
                .collect();
            found.sort_by(|a, b| a.y.total_cmp(&b.y));
            found
        };
        let star = t("s_as_stern");
        let mut fields = places(&|n| n.role() == Role::TextInput);
        // The field to enter a planned ship stands below the ships and is no name field.
        fields.pop();
        let stars = places(&|n| n.role() == Role::CheckBox && n.label() == Some(star.as_str()));
        (labels, fields, stars)
    }

    /// The ships in the order the page draws them: the bold names, top down.
    fn drawn(ctx: &egui::Context, state: &mut HangarState, names: &[&str]) -> Vec<String> {
        let scheme = versekit_core::theme::current();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 4000.0))),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| show_page(ui, scheme, state));
        output.textures_delta.clear();
        let Some(update) = output.platform_output.accesskit_update else { return Vec::new() };
        let mut found: Vec<(f64, String)> = update
            .nodes
            .iter()
            .filter(|(_, n)| n.role() == Role::Label)
            .filter_map(|(_, n)| Some((n.bounds()?.y0, n.value()?.to_string())))
            .filter(|(_, text)| names.contains(&text.as_str()))
            .collect();
        found.sort_by(|a, b| a.0.total_cmp(&b.0));
        found.into_iter().map(|(_, text)| text).collect()
    }

    fn click(at: egui::Pos2) -> Vec<egui::Event> {
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        };
        vec![egui::Event::PointerMoved(at), press(true), press(false)]
    }

    /// The pairing settings as drawn, after a jump to `pointed` (or none), as the shapes of one frame.
    fn pairing_drawn(pointed: Option<&str>) -> String {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let scheme = versekit_core::theme::current();
        let receiver = Receiver {
            inbox: Inbox::new(Gate { enabled: true, token: "a".repeat(64), ready: true }),
            port: 65535,
            token: "a".repeat(64),
            enabled: true,
            listening: true,
            repaint: Arc::new(OnceLock::new()),
        };
        let mut state = HangarState { receiver: Some(receiver), ..HangarState::default() };
        state.point_to_pairing();
        let found = pointed.map(|text| (text.to_string(), 0.0));
        crate::highlight::publish(&ctx, found.as_ref());
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 900.0))),
            time: Some(0.5),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| show_settings(ui, scheme, &mut state));
        output.textures_delta.clear();
        format!("{:?}", output.shapes)
    }

    #[test]
    fn a_jump_from_the_menu_filter_lights_the_row_of_its_setting_only() {
        steady_scheme();
        versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
        let plain = pairing_drawn(None);
        for key in PAIRING_SETTINGS {
            let lit = pairing_drawn(Some(&t(key)));
            assert_ne!(lit, plain, "{key}: the row lights up after the jump");
        }
        // A name that is no setting of the pairing lights nothing.
        assert_eq!(pairing_drawn(Some("Etwas ganz anderes")), plain);
        // Two settings light different rows.
        assert_ne!(pairing_drawn(Some(&t("s_hgl_an"))), pairing_drawn(Some(&t("s_hgl_code"))));
    }

    #[test]
    fn identical_ships_share_their_name_and_enter_stores_it() {
        steady();
        let hangar_text = r#"{"format": 1, "schiffe": [
            {"name": "Gladius", "hersteller": "Aegis", "herkunft": "pledge", "belegung": {}},
            {"name": "Gladius", "hersteller": "", "herkunft": "ingame", "belegung": {}},
            {"name": "Idris", "hersteller": "Aegis", "herkunft": "pledge", "belegung": {}}]}"#;
        let (mut state, dir, file) = names_page("namen", hangar_text, &["vehicle_NameAEGS_Gladius=Gladius"]);
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        frame(&ctx, &mut state, Vec::new());
        let (labels, fields, _) = frame(&ctx, &mut state, Vec::new());
        let has = |role: Role, name: &str| labels.iter().any(|(r, l)| *r == role && l == name);
        let all = tf("s_hgn_alle", &[("n", Arg::from(2usize)), ("schiff", Arg::from("Gladius"))]);
        assert!(has(Role::Label, &all), "{labels:?}");
        assert!(has(Role::Label, &t("s_as_zeile_ohne")), "{labels:?}");
        // The search field, the path field, the hand field, then the two name fields.
        let names: Vec<egui::Pos2> = fields.iter().rev().take(2).copied().collect();
        assert_eq!(names.len(), 2, "{fields:?}");

        frame(&ctx, &mut state, click(names[1]));
        frame(&ctx, &mut state, vec![egui::Event::Text("Nachtfalke".into())]);
        assert!(!std::path::Path::new(&file).exists(), "nothing is stored while typing");
        // The other Gladius shows the text as it is typed.
        let (labels, _, _) = frame(&ctx, &mut state, Vec::new());
        let typed = labels.iter().filter(|(r, l)| *r == Role::TextInput && l == "Nachtfalke").count();
        assert_eq!(typed, 2, "{labels:?}");
        let enter = egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        };
        frame(&ctx, &mut state, vec![enter]);
        frame(&ctx, &mut state, Vec::new());
        let stored = versekit_core::asop::load(&file);
        assert_eq!(versekit_core::asop::entry(&stored, "vehicle_NameAEGS_Gladius"), ("Nachtfalke".to_string(), false));
        let (labels, _, _) = frame(&ctx, &mut state, Vec::new());
        assert!(labels.iter().any(|(_, l)| l == &t("s_hgn_gemerkt")), "{labels:?}");
        assert_eq!(state.names.own_name("Gladius"), "Nachtfalke");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A stored name goes into the game a moment later, once for names typed
    /// in a row; the button writes at once.
    #[test]
    fn a_stored_name_is_written_into_the_game_and_the_button_writes_at_once() {
        steady();
        use std::sync::atomic::{AtomicUsize, Ordering};
        let hangar_text = r#"{"format": 1, "schiffe": [{"name": "Gladius", "hersteller": "Aegis", "herkunft": "pledge", "belegung": {}}]}"#;
        let (mut state, dir, file) = names_page("schreiben", hangar_text, &["vehicle_NameAEGS_Gladius=Gladius"]);
        let writes = std::sync::Arc::new(AtomicUsize::new(0));
        let counted = writes.clone();
        // The text is taken here: the writer runs in a thread of its own, which has the language of the program.
        let stands = t("s_as_steht");
        state.names.set_writer(std::sync::Arc::new(move || {
            counted.fetch_add(1, Ordering::SeqCst);
            (stands.clone(), false)
        }));
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        frame(&ctx, &mut state, Vec::new());
        let (_, _, stars) = frame(&ctx, &mut state, Vec::new());
        frame(&ctx, &mut state, click(stars[0]));
        assert!(versekit_core::asop::entry(&versekit_core::asop::load(&file), "vehicle_NameAEGS_Gladius").1);
        assert_eq!(writes.load(Ordering::SeqCst), 0, "not at once");
        let (labels, _, _) = frame(&ctx, &mut state, Vec::new());
        assert!(labels.iter().any(|(_, l)| l == &t("s_as_gemerkt")), "{labels:?}");
        std::thread::sleep(WRITE_WAIT);
        for _ in 0..200 {
            frame(&ctx, &mut state, Vec::new());
            if writes.load(Ordering::SeqCst) == 1 && !state.names.writing() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(writes.load(Ordering::SeqCst), 1);
        assert_eq!(state.names.written().cloned(), Some((t("s_as_steht"), false)));
        let scheme = versekit_core::theme::current();
        let button = {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 4000.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| show_page(ui, scheme, &mut state));
            output.textures_delta.clear();
            let update = output.platform_output.accesskit_update.expect("accesskit is on");
            let name = t("s_as_einspielen");
            update
                .nodes
                .iter()
                .find(|(_, n)| n.role() == Role::Button && n.label() == Some(name.as_str()))
                .and_then(|(_, n)| n.bounds())
                .map(|b| egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32))
                .expect("the button is there")
        };
        frame(&ctx, &mut state, click(button));
        for _ in 0..200 {
            frame(&ctx, &mut state, Vec::new());
            if writes.load(Ordering::SeqCst) == 2 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(writes.load(Ordering::SeqCst), 2, "the button writes at once");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Longer than the delay of the writing.
    const WRITE_WAIT: std::time::Duration = std::time::Duration::from_millis(950);

    #[test]
    fn the_star_is_stored_at_once_and_sorts_first_after_a_new_search() {
        steady();
        let hangar_text = r#"{"format": 1, "schiffe": [
            {"name": "Arrow", "hersteller": "Anvil", "herkunft": "pledge", "belegung": {}},
            {"name": "Zeus", "hersteller": "RSI", "herkunft": "pledge", "belegung": {}}]}"#;
        let table = ["vehicle_NameANVL_Arrow=Arrow", "vehicle_NameRSI_Zeus=Zeus"];
        let (mut state, dir, file) = names_page("stern", hangar_text, &table);
        let ships = ["Arrow", "Zeus"];
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        frame(&ctx, &mut state, Vec::new());
        let (_, _, stars) = frame(&ctx, &mut state, Vec::new());
        assert_eq!(drawn(&ctx, &mut state, &ships), vec!["Arrow", "Zeus"]);
        assert_eq!(stars.len(), 2, "{stars:?}");
        frame(&ctx, &mut state, click(stars[1]));
        let stored = versekit_core::asop::load(&file);
        assert_eq!(versekit_core::asop::entry(&stored, "vehicle_NameRSI_Zeus"), (String::new(), true));
        // The ship stays where it is until the search changes.
        frame(&ctx, &mut state, Vec::new());
        assert_eq!(drawn(&ctx, &mut state, &ships), vec!["Arrow", "Zeus"]);
        state.search = "a".into();
        frame(&ctx, &mut state, Vec::new());
        state.search.clear();
        frame(&ctx, &mut state, Vec::new());
        assert_eq!(drawn(&ctx, &mut state, &ships), vec!["Zeus", "Arrow"]);
        // An own name without the star comes after the star, before the rest.
        let (done, _) = versekit_core::asop::store(&file, "vehicle_NameANVL_Arrow", "Pfeil", false);
        assert_eq!(done, versekit_core::asop::Stored::Written);
        state.names.reorder();
        assert_eq!(drawn(&ctx, &mut state, &ships), vec!["Zeus", "Arrow"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Melt choice at the left, the ready switch at the right under
    /// the remove button, on every row, with and without the mark of open or
    /// fitted parts, at every width.
    #[test]
    fn row_controls_keep_their_places() {
        steady();
        use versekit_core::language::{Choice, Lang, set_thread_language};
        let scheme = versekit_core::theme::current();
        let mut findings = Vec::new();
        for (language, width) in
            [Choice::En, Choice::De].into_iter().flat_map(|l| [340.0_f32, 380.0, 520.0, 900.0].map(|w| (l, w)))
        {
            set_thread_language(language);
            let ctx = egui::Context::default();
            crate::install(&ctx);
            ctx.enable_accesskit();
            // Open mark (Cutlass Black), fitted mark (Gladius), none (Carrack, Freelancer MAX).
            let mut state = HangarState::default();
            for (name, maker) in [("Avenger Titan", ""), ("C8 Pisces", "Anvil Aerospace"), ("Polaris", "RSI")] {
                assert!(hangar::remove(&mut state.held.data, name, maker), "{name}");
            }
            let mut nodes: Vec<(egui::accesskit::NodeId, Role, String, egui::accesskit::Rect)> = Vec::new();
            for _ in 0..3 {
                // Tall enough that every row stays in sight in every frame.
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 12000.0))),
                    ..Default::default()
                };
                let mut output = ctx.run_ui(input, |ui| show_page(ui, scheme, &mut state));
                output.textures_delta.clear();
                // An update may carry only the nodes that changed: keep the latest of each.
                if let Some(update) = output.platform_output.accesskit_update {
                    for (id, n) in update.nodes {
                        nodes.retain(|(other, ..)| *other != id);
                        if let (Some(label), Some(bounds)) = (n.label().or(n.value()), n.bounds()) {
                            nodes.push((id, n.role(), label.to_string(), bounds));
                        }
                    }
                }
            }
            let nodes: Vec<(Role, String, egui::accesskit::Rect)> =
                nodes.into_iter().map(|(_, role, label, bounds)| (role, label, bounds)).collect();
            // Other tests switch the language meanwhile: either one counts.
            let both = |key: &str, args: &[(&str, Arg)]| {
                [Lang::De, Lang::En].map(|lang| versekit_core::language::tf_in(lang, key, args))
            };
            let edges = |role: Role, names: &[String; 2], right: bool| -> Vec<i64> {
                let mut found: Vec<(f64, i64)> = nodes
                    .iter()
                    .filter(|(r, l, _)| *r == role && names.contains(l))
                    .map(|(_, _, b)| (b.y0, (if right { b.x1 } else { b.x0 }).round() as i64))
                    .collect();
                found.sort_by(|a, b| a.0.total_cmp(&b.0));
                found.into_iter().map(|(_, x)| x).collect()
            };
            let at = format!("{} {width}", if matches!(language, Choice::De) { "de" } else { "en" });
            let melt = edges(Role::ComboBox, &both("s_hgl_schmelzen", &[]), false);
            let ready = edges(Role::CheckBox, &both("s_hg_einsatzbereit", &[]), true);
            let remove = edges(Role::Button, &both("s_hg_entfernen", &[]), true);
            if melt.len() != 4 || ready.len() != 4 || remove.len() != 4 {
                findings.push(format!("{at}:zu wenige Zeilen {melt:?} {ready:?} {remove:?}"));
                continue;
            }
            if melt.iter().any(|x| *x != melt[0]) {
                findings.push(format!("{at}:Schmelzen links {melt:?}"));
            }
            if ready.iter().any(|x| *x != ready[0]) {
                findings.push(format!("{at}:Einsatzbereit rechts {ready:?}"));
            }
            if ready[0] != remove[0] {
                findings.push(format!("{at}:Einsatzbereit {} gegen Austragen {}", ready[0], remove[0]));
            }
            // The marks: right-aligned before the ready label, or on
            // their own line below, right-aligned with the switch.
            let ready_left = edges(Role::Label, &both("s_hg_einsatzbereit", &[]), false);
            for (key, fitted) in [("s_hg_offen", false), ("s_hgl_fertig", true)] {
                let text = both(key, &[("n", Arg::from(1usize))]);
                let (left, right) = (edges(Role::Label, &text, false), edges(Role::Label, &text, true));
                let beside = right.first().is_some_and(|x| ready_left.iter().any(|r| r - x > 0 && r - x < 20));
                let under = right.first() == ready.first() && !left.is_empty();
                if !(beside || under) {
                    findings.push(format!("{at}:Marke {key} bei {left:?}..{right:?}, Einsatzbereit {ready_left:?}"));
                }
                // Narrow, the long mark has to move to its own line; wide, it stays beside.
                if fitted && ((width <= 340.0 && !under) || (width >= 900.0 && under)) {
                    findings.push(format!("{at}:Marke {key} darunter: {under}"));
                }
            }
            // The name box: the star switch at the same place on every row.
            let star = edges(Role::CheckBox, &both("s_as_stern", &[]), true);
            if star.is_empty() || star.iter().any(|x| *x != star[0]) {
                findings.push(format!("{at}:Stern rechts {star:?}"));
            }
        }
        set_thread_language(Choice::Auto);
        assert!(findings.is_empty(), "{}", findings.join("\n"));
    }

    #[test]
    fn a_name_still_being_typed_is_stored_when_the_window_closes() {
        steady();
        let (mut state, dir, file) = names_page("schliessen", TWO, &["vehicle_NameAEGS_Gladius=Gladius"]);
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        frame(&ctx, &mut state, Vec::new());
        let (_, fields, _) = frame(&ctx, &mut state, Vec::new());
        frame(&ctx, &mut state, click(*fields.last().expect("name field")));
        frame(&ctx, &mut state, vec![egui::Event::Text("Falke".into())]);
        assert!(!std::path::Path::new(&file).exists());
        drop(state);
        let stored = versekit_core::asop::load(&file);
        assert_eq!(versekit_core::asop::entry(&stored, "vehicle_NameAEGS_Gladius").0, "Falke");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_name_that_cannot_be_stored_stays_on_the_page() {
        steady();
        let (mut state, dir, file) = names_page("fehler", TWO, &["vehicle_NameAEGS_Gladius=Gladius"]);
        // A folder in place of the file: writing fails.
        std::fs::create_dir_all(&file).expect("in the way");
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        frame(&ctx, &mut state, Vec::new());
        let (_, fields, _) = frame(&ctx, &mut state, Vec::new());
        let field = *fields.last().expect("name field");
        frame(&ctx, &mut state, click(field));
        frame(&ctx, &mut state, vec![egui::Event::Text("Falke".into())]);
        frame(&ctx, &mut state, click(egui::pos2(5.0, 3990.0)));
        let (labels, _, _) = frame(&ctx, &mut state, Vec::new());
        assert!(labels.iter().any(|(r, l)| *r == Role::TextInput && l == "Falke"), "{labels:?}");
        assert!(labels.iter().any(|(_, l)| l == &t("s_as_nicht_gespeichert")), "{labels:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_search_finds_the_own_name() {
        steady();
        let mut state = HangarState::default();
        let mut data = versekit_core::asop::empty();
        versekit_core::asop::set_name(&mut data, "vehicle_NameANVL_Carrack", "Mutterschiff", false);
        state.names.set_data_for_test(data);
        state.search = "mutter".into();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        frame(&ctx, &mut state, Vec::new());
        let (labels, _, _) = frame(&ctx, &mut state, Vec::new());
        let head = tf("s_hg_meine_gefiltert", &[("n", Arg::from(1usize)), ("alle", Arg::from(7usize))]);
        assert!(labels.iter().any(|(_, l)| l == &head), "{labels:?}");
    }

    /// The page and its settings fit a narrow window at every size, with a
    /// long search and a long last message.
    #[test]
    fn nothing_sticks_out() {
        steady();
        let scheme = versekit_core::theme::current();
        let mut findings = Vec::new();
        for points in versekit_core::font_size::POINTS_RANGE.0..=versekit_core::font_size::POINTS_RANGE.1 {
            // With the fleet analysis closed and open.
            for (width, open) in [380.0, 520.0, 900.0].into_iter().flat_map(|w| [(w, false), (w, true)]) {
                let ctx = egui::Context::default();
                crate::install(&ctx);
                crate::apply_size(&ctx, points);
                let mut state = HangarState {
                    message: Some((t("s_hg_import_leer"), Tone::Bad)),
                    real_money: true,
                    fleet: FleetView::with_open(open),
                    ..HangarState::default()
                };
                let shared = Inbox::new(Gate { enabled: true, token: "a".repeat(64), ready: true });
                state.receiver = Some(Receiver {
                    inbox: shared,
                    port: 65535,
                    token: "a".repeat(64),
                    enabled: true,
                    listening: false,
                    repaint: Arc::new(OnceLock::new()),
                });
                let mut over = 0.0;
                for page in 0..2 {
                    for _ in 0..3 {
                        let input = egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 4000.0))),
                            ..Default::default()
                        };
                        let mut output = ctx.run_ui(input, |ui| {
                            if page == 0 {
                                show_page(ui, scheme, &mut state);
                            } else {
                                show_settings(ui, scheme, &mut state);
                            }
                            over = (ui.min_rect().right() - ui.ctx().content_rect().right()).max(0.0);
                        });
                        output.textures_delta.clear();
                    }
                    if over > 0.5 {
                        findings.push(format!("Seite {page} Größe {points} Breite {width}: {over:.0} zu breit"));
                    }
                }
            }
        }
        assert!(findings.is_empty(), "{}", findings.join("\n"));
    }

    /// The rectangles of a frame's shapes with their fill and border color.
    fn painted_rects(shapes: &[egui::epaint::ClippedShape]) -> Vec<(egui::Rect, egui::Color32, egui::Color32)> {
        fn walk(shape: &egui::epaint::Shape, out: &mut Vec<(egui::Rect, egui::Color32, egui::Color32)>) {
            match shape {
                egui::epaint::Shape::Vec(inner) => inner.iter().for_each(|s| walk(s, out)),
                egui::epaint::Shape::Rect(r) => out.push((r.rect, r.fill, r.stroke.color)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        shapes.iter().for_each(|s| walk(&s.shape, &mut out));
        out
    }

    /// Every ship is one card of its own: room between two cards, and the
    /// in-game name block lies inside the frame of its card, once per card.
    #[test]
    fn ship_cards_stand_apart_and_hold_their_name_block() {
        steady();
        let hangar_text = r#"{"format": 1, "schiffe": [
            {"name": "Gladius", "hersteller": "Aegis", "herkunft": "pledge", "belegung": {}},
            {"name": "Idris", "hersteller": "Aegis", "herkunft": "pledge", "belegung": {}},
            {"name": "Carrack", "hersteller": "Anvil", "herkunft": "pledge", "belegung": {}}]}"#;
        let table =
            ["vehicle_NameAEGS_Gladius=Gladius", "vehicle_NameAEGS_Idris=Idris", "vehicle_NameANVL_Carrack=Carrack"];
        let mut findings = Vec::new();
        for scheme in versekit_core::theme::schemes() {
            for width in [380.0_f32, 900.0] {
                let (mut state, _dir, _file) = names_page("karten", hangar_text, &table);
                let ctx = egui::Context::default();
                crate::install(&ctx);
                let mut shapes = Vec::new();
                for _ in 0..4 {
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 4000.0))),
                        ..Default::default()
                    };
                    let mut output = ctx.run_ui(input, |ui| show_page(ui, scheme, &mut state));
                    output.textures_delta.clear();
                    shapes = output.shapes;
                }
                let rects = painted_rects(&shapes);
                let (surface, line, bar) = (color(scheme.surface), color(scheme.line), color(scheme.bar));
                let big = |r: &egui::Rect| r.width() > width * 0.5;
                let mut cards: Vec<egui::Rect> = rects
                    .iter()
                    .filter(|(r, f, s)| *f == surface && *s == line && big(r) && r.height() > 60.0)
                    .map(|x| x.0)
                    .collect();
                let blocks: Vec<egui::Rect> =
                    rects.iter().filter(|(r, f, s)| *f == bar && *s == line && big(r)).map(|x| x.0).collect();
                // The card of the head (note and buttons, as tall as two lines in a narrow window) is no ship card.
                cards.retain(|card| blocks.iter().any(|b| card.contains_rect(*b)));
                cards.sort_by(|a, b| a.min.y.total_cmp(&b.min.y));
                let at = format!("{} width {width}", scheme.id);
                if scheme.bar == scheme.surface {
                    findings.push(format!("{at}: the name block has the color of the card"));
                }
                if cards.len() != 3 || blocks.len() != 3 {
                    findings.push(format!("{at}: {} cards, {} name blocks, wanted 3 and 3", cards.len(), blocks.len()));
                    continue;
                }
                for pair in cards.windows(2) {
                    let gap = pair[1].min.y - pair[0].max.y;
                    if gap < 4.0 {
                        findings.push(format!("{at}: only {gap:.1} points between two cards"));
                    }
                }
                for card in &cards {
                    let inside = blocks.iter().filter(|b| card.contains_rect(**b)).count();
                    if inside != 1 {
                        findings.push(format!("{at}: {inside} name blocks inside the card at {:.0}", card.min.y));
                    }
                }
            }
        }
        assert!(findings.is_empty(), "{}", findings.join("\n"));
    }

    /// The centre of the button with this name in a frame, from the screen reader's view.
    fn button_at(nodes: &[(egui::accesskit::NodeId, egui::accesskit::Node)], name: &str) -> Option<egui::Pos2> {
        nodes
            .iter()
            .find(|(_, n)| n.role() == Role::Button && n.label() == Some(name))
            .and_then(|(_, n)| n.bounds())
            .map(|b| egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32))
    }

    fn loadout_names(entry: &Map) -> Vec<String> {
        equipment::read_all(entry, &[]).0.into_iter().map(|e| e.name).collect()
    }

    #[test]
    fn a_ship_moves_to_planned_and_back_through_its_buttons_with_its_loadouts() {
        steady();
        let start = r#"{"format": 1, "schiffe": [{"name": "Arrow", "hersteller": "Anvil Aerospace", "herkunft": "ingame",
            "notiz": "Mein Erster", "belegung": {"hp_a": {"ref": "laser-3", "name": "Laser 3", "weg": "kaufen"}},
            "ausstattungen": [
                {"name": "PvP", "active": true, "slots": [{"portPath": "hp_a", "itemGuid": "laser-3", "acquisition": "purchased"}]},
                {"name": "Rennen", "active": false, "slots": [{"portPath": "hp_b", "itemGuid": "cooler-1", "acquisition": "looted"}]}]}]}"#;
        let (mut state, dir) = page_on("verschieben", start);
        let file = dir.join(hangar::FILE).to_string_lossy().into_owned();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        let scheme = versekit_core::theme::current();
        let run = |state: &mut HangarState, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 4000.0))),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| show_page(ui, scheme, state));
            output.textures_delta.clear();
            output.platform_output.accesskit_update.expect("accesskit is on").nodes
        };
        let before = loadout_names(hangar::ships(&state.held.data)[0]);
        assert_eq!(before, ["PvP", "Rennen"]);
        // Gone after a wipe: onto the wish list.
        let to_planned = tf("s_hg_nach_geplant_fuer", &[("schiff", Arg::from("Arrow"))]);
        let at = button_at(&run(&mut state, Vec::new()), &to_planned).expect("every own ship has the button");
        run(&mut state, click(at));
        assert!(hangar::ships(&state.held.data).is_empty());
        let on_disk = hangar::read(&file).expect("file");
        assert!(hangar::ships(&on_disk).is_empty() && hangar::wishlist_contains(&on_disk, "Arrow"), "stored at once");
        let wish = hangar::wishlist(&on_disk)[0];
        assert_eq!(loadout_names(wish), before, "the loadouts come along");
        assert_eq!(hangar::field_text(wish, "notiz"), "Mein Erster", "and what else was entered");
        assert!(state.moved.as_ref().is_some_and(|(text, _, _)| text.contains("Arrow")), "the move is said");
        // Bought again: back into the own ships.
        let to_hangar = tf("s_hg_nach_meine_fuer", &[("schiff", Arg::from("Arrow"))]);
        let at = button_at(&run(&mut state, Vec::new()), &to_hangar).expect("every planned ship has the button");
        run(&mut state, click(at));
        let on_disk = hangar::read(&file).expect("file");
        assert!(hangar::wishlist(&on_disk).is_empty());
        let ship = hangar::ships(&on_disk)[0];
        assert_eq!(loadout_names(ship), before);
        assert_eq!(hangar::field_text(ship, "herkunft"), hangar::INGAME);
        assert_eq!(equipment::read_all(ship, &[]).active().map(|e| e.name.clone()).as_deref(), Some("PvP"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_loadouts_of_the_other_ships_of_the_same_type_are_offered_in_both_lists() {
        steady();
        let data = match jsonio::loads(
            r#"{"format": 1, "schiffe": [
                {"name": "Gladius", "hersteller": "Aegis Dynamics", "belegung": {},
                 "ausstattungen": [{"name": "PvP", "active": true, "slots": [{"portPath": "a", "itemGuid": "g1", "acquisition": "crafted"}]}]},
                {"name": "Arrow", "hersteller": "Anvil Aerospace", "belegung": {},
                 "ausstattungen": [{"name": "Schnell", "active": true, "slots": [{"portPath": "a", "itemGuid": "g2", "acquisition": "purchased"}]}]}],
             "wunsch": [{"name": "Gladius", "hersteller": "Aegis Dynamics", "belegung": {}}]}"#,
        ) {
            Ok(Value::Object(map)) => map,
            other => panic!("{other:?}"),
        };
        let stores = Stores::empty();
        let wish = hangar::wishlist(&data)[0].clone();
        let offered = same_type_loadouts(&data, &stores, &wish);
        let lines: Vec<&str> = offered.iter().map(|(line, _)| line.as_str()).collect();
        let want = tf(
            "s_wk_uebernehmen_eintrag",
            &[("liste", Arg::from(t("s_wk_aus_meine"))), ("schiff", Arg::from("Gladius")), ("name", Arg::from("PvP"))],
        );
        assert_eq!(lines, [want.as_str()], "the own Gladius, not the Arrow");
        assert_eq!(offered[0].1.slot("a").map(|s| s.acquisition), Some(equipment::Acquisition::Crafted), "as it is");
        // The other way round, the planned Gladius has nothing yet; the own one is never offered to itself.
        let own = hangar::ships(&data)[0].clone();
        assert!(same_type_loadouts(&data, &stores, &own).is_empty());
        // Taken over, it is a loadout of the wish.
        let mut wish = wish;
        let mut box_state = EquipmentState::default();
        let take = equipment_box::Action::AddShared { equipments: vec![offered[0].1.clone()], key: "k".into() };
        assert!(change_plan(&mut wish, &take, &stores, &Extra::default(), &mut box_state));
        assert_eq!(loadout_names(&wish), ["Standard", "PvP"]);
        assert_eq!(equipment::read_all(&wish, &[]).active().map(|e| e.name.clone()).as_deref(), Some("PvP"));
        // Now the planned Gladius has one of its own, and that is offered to the own Gladius from Planned.
        let mut data = data;
        if let Some((_, Value::Array(wishes))) = data.iter_mut().find(|(k, _)| k == "wunsch") {
            wishes[0] = Value::Object(wish);
        }
        let from_planned = tf(
            "s_wk_uebernehmen_eintrag",
            &[("liste", Arg::from(t("s_hg_geplant_titel"))), ("schiff", Arg::from("Gladius")), ("name", Arg::from("PvP"))],
        );
        let offered = same_type_loadouts(&data, &stores, &own);
        assert!(offered.iter().any(|(line, _)| *line == from_planned), "{:?}", offered.iter().map(|(l, _)| l).collect::<Vec<_>>());
    }

    #[test]
    fn a_vehicle_of_a_package_has_no_button_to_move_it() {
        steady();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        let mut state = HangarState::default();
        let labels = labels_of(&ctx, &mut state);
        let has = |ship: &str| {
            let name = tf("s_hg_nach_geplant_fuer", &[("schiff", Arg::from(ship))]);
            labels.iter().any(|(role, label)| *role == Role::Button && *label == name)
        };
        assert!(!has("C8 Pisces"), "it follows the Carrack: {labels:?}");
        assert!(has("Carrack") && has("Gladius"), "{labels:?}");
    }

    #[test]
    fn a_loadout_goes_out_as_a_code_and_comes_back_as_the_same_loadout_on_another_card() {
        use equipment_box::Action;
        use versekit_core::equipment_share::{self as share, PartIds};
        steady();
        const LASER: &str = "bbbbbbbb-0000-4000-8000-000000000001";
        const COOLER: &str = "bbbbbbbb-0000-4000-8000-000000000002";
        let mut state = HangarState {
            extra: Arc::new(Extra { ids: PartIds::new([(LASER.into(), "KLWE_Laser_S3".into())]), ..Extra::default() }),
            ..HangarState::default()
        };
        let sender = match jsonio::loads(&format!(
            r#"{{"name": "Gladius", "hersteller": "Aegis Dynamics",
                "belegung": {{"hp_a": {{"ref": "{LASER}", "name": "Laser", "weg": "bauen"}}, "hp_b": {{"ref": "{COOLER}", "name": "Cooler", "weg": "kaufen"}}}},
                "ausstattungen": [{{"name": "PvP", "active": true, "slots": [
                    {{"portPath": "hp_a", "itemGuid": "{LASER}", "acquisition": "crafted", "quality": {{"Frame": 900}}}},
                    {{"portPath": "hp_b", "itemGuid": "{COOLER}", "acquisition": "purchased"}}]}}]}}"#
        )) {
            Ok(Value::Object(map)) => map,
            other => panic!("{other:?}"),
        };
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            assert!(state.share_action(ui, &sender, &Action::Share { key: "k".into() }));
            assert!(!state.share_action(ui, &sender, &Action::Undo), "the other actions are the plan's");
        });
        output.textures_delta.clear();
        let code = output
            .platform_output
            .commands
            .iter()
            .find_map(|c| match c {
                egui::OutputCommand::CopyText(text) => Some(text.clone()),
                _ => None,
            })
            .expect("the code is copied");
        assert!(code.starts_with(share::PREFIX) && code.len() < share::CHAT_LIMIT, "{code}");
        let said = state.equipment.said.clone().expect("said");
        assert_eq!((said.0.as_str(), said.2), ("k", false));
        assert!(said.1.contains(&code.chars().count().to_string()), "{said:?}");
        // The receiver's planned Gladius, without a loadout yet.
        let shared = share::decode(&code).expect("own code");
        let came = share::equipment_of(&shared, &state.extra.ids, &|guid: &str| guid == COOLER);
        let mut receiver = match jsonio::loads(r#"{"name": "Gladius", "hersteller": "Aegis Dynamics", "belegung": {}}"#) {
            Ok(Value::Object(map)) => map,
            other => panic!("{other:?}"),
        };
        let stores = Stores::empty();
        let mut box_state = EquipmentState::default();
        let added = Action::AddShared { equipments: vec![came], key: "k".into() };
        assert!(change_plan(&mut receiver, &added, &stores, &state.extra, &mut box_state));
        let got = equipment::read_all(&receiver, &[]).active().cloned().expect("an active one");
        let want = equipment::read_all(&sender, &[]).active().cloned().expect("the sent one");
        let parts = |e: &equipment::Equipment| {
            let mut all: Vec<(String, String)> = e.slots.iter().map(|s| (s.port_path.clone(), s.item_guid.clone())).collect();
            all.sort();
            all
        };
        assert_eq!(got.name, "PvP");
        assert_eq!(parts(&got), parts(&want), "the same parts in the same slots");
        assert_eq!(plan_part(&receiver, "hp_a").map(|(r, _)| r).as_deref(), Some(LASER), "the plan of the pages follows");
        assert!(box_state.said.as_ref().is_some_and(|(_, text, bad)| text.contains("PvP") && !bad), "{:?}", box_state.said);
    }

    // ------------------------------------------------------------ the switch "ready" and the list of the ship

    /// Two Gladius of one name with parts open: two on the first, one on the second.
    const READY_FLEET: &str = r#"{"format": 1, "schiffe": [
        {"name": "Gladius", "hersteller": "Aegis", "herkunft": "pledge", "belegung": {
            "a": {"ref": "x", "name": "X", "weg": "kaufen"}, "b": {"ref": "y", "name": "Y", "weg": "kaufen"}}},
        {"name": "Gladius", "hersteller": "", "herkunft": "ingame", "belegung": {
            "a": {"ref": "x", "name": "X", "weg": "kaufen"}}}]}"#;

    /// One frame of the page: what it shows as (role, text) and the places of the controls named `named`.
    fn ready_frame(
        ctx: &egui::Context,
        state: &mut HangarState,
        events: Vec<egui::Event>,
        named: &[(Role, String)],
    ) -> (Vec<(Role, String)>, Vec<Vec<egui::Pos2>>) {
        let scheme = versekit_core::theme::current();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 4000.0))),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| show_page(ui, scheme, state));
        output.textures_delta.clear();
        let Some(update) = output.platform_output.accesskit_update else { return (Vec::new(), Vec::new()) };
        let shown = update
            .nodes
            .iter()
            .filter_map(|(_, n)| n.label().or(n.value()).map(|l| (n.role(), l.to_owned())))
            .collect();
        let places = named
            .iter()
            .map(|(role, name)| {
                let mut found: Vec<egui::Pos2> = update
                    .nodes
                    .iter()
                    .filter(|(_, n)| n.role() == *role && n.label() == Some(name.as_str()))
                    .filter_map(|(_, n)| n.bounds())
                    .map(|b| egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32))
                    .collect();
                found.sort_by(|a, b| a.y.total_cmp(&b.y));
                found
            })
            .collect();
        (shown, places)
    }

    /// Clicks the switch "Ready" of the ship that stands `which`th from the top. The page after the click; the places
    /// of "Yes" and "No".
    fn switch_on(
        state: &mut HangarState,
        ctx: &egui::Context,
        which: usize,
    ) -> (Vec<(Role, String)>, Vec<egui::Pos2>, Vec<egui::Pos2>) {
        let switch = (Role::CheckBox, t("s_hg_einsatzbereit"));
        let buttons = [(Role::Button, t("e_ja")), (Role::Button, t("e_nein"))];
        let named = [switch, buttons[0].clone(), buttons[1].clone()];
        ready_frame(ctx, state, Vec::new(), &named);
        let (_, places) = ready_frame(ctx, state, Vec::new(), &named);
        let at = places[0].get(which).copied().expect("the switch of that ship");
        ready_frame(ctx, state, click(at), &named);
        // A dialog is laid out in its first frame and seen from its second.
        ready_frame(ctx, state, Vec::new(), &named);
        let (shown, places) = ready_frame(ctx, state, Vec::new(), &named);
        (shown, places[1].clone(), places[2].clone())
    }

    fn ready_flags(state: &HangarState) -> Vec<bool> {
        hangar::ships(&state.held.data).iter().map(|s| hangar::field_true(s, "einsatzbereit")).collect()
    }

    fn open_counts(state: &HangarState) -> Vec<usize> {
        hangar::ships(&state.held.data).iter().map(|s| hangar::open_count(s)).collect()
    }

    /// The question is there, whatever the ship is called in it.
    fn asks(shown: &[(Role, String)]) -> bool {
        shown.iter().any(|(_, text)| *text == t("s_hg_einsatz_frage_titel"))
    }

    #[test]
    fn a_ship_put_on_ready_with_parts_open_asks_and_yes_ticks_its_list_and_no_other() {
        yes_ticks_that_ship_only(0);
    }

    #[test]
    fn the_second_of_two_ships_of_one_kind_is_the_one_that_is_asked_about_and_ticked() {
        yes_ticks_that_ship_only(1);
    }

    /// Clicks the switch of the ship `which`th from the top, answers Yes and sees that this ship, and no other, is
    /// ticked off.
    fn yes_ticks_that_ship_only(which: usize) {
        steady();
        let _language = crate::language_lock();
        let (mut state, dir) = page_on(&format!("einsatz-ja-{which}"), READY_FLEET);
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        let (shown, yes, _) = switch_on(&mut state, &ctx, which);
        assert!(ready_flags(&state).iter().any(|on| *on), "the click switched nothing");
        assert!(state.ready_question.is_some(), "no question was set");
        let near: Vec<_> = shown.iter().filter(|(_, text)| text.to_lowercase().contains("abhaken")).collect();
        assert!(asks(&shown), "{} nodes, none is the question: {near:?}", shown.len());
        let flags = ready_flags(&state);
        assert_eq!(flags.iter().filter(|on| **on).count(), 1, "the switch of one ship is on: {flags:?}");
        let before = open_counts(&state);
        let ready_at = flags.iter().position(|on| *on).expect("the ship on ready");
        assert!(before[ready_at] > 0, "it has parts open");
        let click_yes = click(yes.first().copied().expect("the button Yes"));
        ready_frame(&ctx, &mut state, click_yes, &[]);
        let after = open_counts(&state);
        assert_eq!(after[ready_at], 0, "all of its list is ticked off");
        for (i, count) in after.iter().enumerate() {
            if i != ready_at {
                assert_eq!(*count, before[i], "the other ships keep their list: {before:?} {after:?}");
            }
        }
        assert_eq!(ready_flags(&state), flags, "no other switch moved");
        let on_disk = hangar::read(&dir.join(hangar::FILE).to_string_lossy()).expect("file");
        assert_eq!(hangar::open_count(hangar::ships(&on_disk)[ready_at]), 0, "written to the file");
        let (shown, _) = ready_frame(&ctx, &mut state, Vec::new(), &[]);
        assert!(!asks(&shown), "the question is gone");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_leaves_the_list_and_only_the_switch_changes() {
        steady();
        let _language = crate::language_lock();
        let (mut state, dir) = page_on("einsatz-nein", READY_FLEET);
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        let before = open_counts(&state);
        let (shown, _, no) = switch_on(&mut state, &ctx, 0);
        assert!(asks(&shown), "{shown:?}");
        let click_no = click(no.first().copied().expect("the button No"));
        ready_frame(&ctx, &mut state, click_no, &[]);
        assert_eq!(open_counts(&state), before, "nothing was ticked");
        assert_eq!(ready_flags(&state).iter().filter(|on| **on).count(), 1, "the switch stays on");
        let (shown, _) = ready_frame(&ctx, &mut state, Vec::new(), &[]);
        assert!(!asks(&shown), "the question is gone");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn escape_answers_no_and_switching_off_takes_the_question_back() {
        steady();
        let _language = crate::language_lock();
        let (mut state, dir) = page_on("einsatz-esc", READY_FLEET);
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        let before = open_counts(&state);
        let (shown, _, _) = switch_on(&mut state, &ctx, 0);
        assert!(asks(&shown), "{shown:?}");
        let escape = egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: Some(egui::Key::Escape),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        ready_frame(&ctx, &mut state, vec![escape], &[]);
        let (shown, _) = ready_frame(&ctx, &mut state, Vec::new(), &[]);
        assert!(!asks(&shown) && state.ready_question.is_none(), "Esc closed the question");
        assert_eq!(open_counts(&state), before, "and ticked nothing");
        assert_eq!(ready_flags(&state).iter().filter(|on| **on).count(), 1, "the switch stays on");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_ship_switched_off_with_a_complete_list_stays_off_when_a_tick_is_set_again() {
        steady();
        let _language = crate::language_lock();
        let fleet = r#"{"format": 1, "schiffe": [
            {"name": "Gladius", "hersteller": "Aegis", "herkunft": "pledge", "belegung": {
                "a": {"ref": "x", "name": "X", "weg": "kaufen", "erledigt": true}}}]}"#;
        let (mut state, dir) = page_on("einsatz-aus", fleet);
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        // On (nothing to ask), then off with the list complete.
        switch_on(&mut state, &ctx, 0);
        assert_eq!(ready_flags(&state), [true]);
        switch_on(&mut state, &ctx, 0);
        assert_eq!(ready_flags(&state), [false], "the second click switches it off");
        // The tick is taken back and set again in the box: the ship stays off.
        let ship = hangar::ship_mut(&mut state.held.data, 0).expect("ship");
        let tick = |on: bool| equipment_box::Action::Tick { paths: vec!["a".to_string()], on };
        assert!(equipment_box::apply(ship, &tick(false)));
        let was = hangar::fully_fitted(ship);
        assert!(equipment_box::apply(ship, &tick(true)));
        assert!(!ready_after_tick(ship, was, &tick(true)), "the same list, switched off: it stays off");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_ship_with_nothing_open_is_not_asked() {
        steady();
        let _language = crate::language_lock();
        let fleet = r#"{"format": 1, "schiffe": [
            {"name": "Gladius", "hersteller": "Aegis", "herkunft": "pledge", "belegung": {
                "a": {"ref": "x", "name": "X", "weg": "kaufen", "erledigt": true}}}]}"#;
        let (mut state, dir) = page_on("einsatz-nichts", fleet);
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        let (shown, _, _) = switch_on(&mut state, &ctx, 0);
        assert!(!asks(&shown), "{shown:?}");
        assert_eq!(ready_flags(&state), [true], "the switch is on");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_last_tick_in_the_box_puts_the_ship_on_ready_and_a_tick_taken_back_does_not() {
        use equipment_box::Action;
        let mut ship = match jsonio::loads(
            r#"{"name": "Gladius", "belegung": {"a": {"ref": "x", "erledigt": true}, "b": {"ref": "y"}}}"#,
        ) {
            Ok(Value::Object(m)) => m,
            other => panic!("{other:?}"),
        };
        let tick = |on: bool| Action::Tick { paths: vec!["b".to_string()], on };
        let was = hangar::fully_fitted(&ship);
        // Taking the open part away completes the plan too, but that is no tick: the ship does not go on ready.
        let clear = Action::Clear { paths: vec!["b".to_string()] };
        let mut cleared = ship.clone();
        assert!(equipment_box::apply(&mut cleared, &clear));
        assert!(hangar::fully_fitted(&cleared), "nothing open any more");
        assert!(!ready_after_tick(&mut cleared, was, &clear), "another change of the plan");
        assert!(!hangar::field_true(&cleared, "einsatzbereit"));
        assert!(equipment_box::apply(&mut ship, &tick(true)));
        assert!(ready_after_tick(&mut ship, was, &tick(true)), "the last part");
        assert!(hangar::field_true(&ship, "einsatzbereit"));
        // Switched off by hand, then a tick taken back and the plan still complete next time: nothing comes back.
        map_set(&mut ship, "einsatzbereit", Value::Bool(false));
        let was = hangar::fully_fitted(&ship);
        assert!(!ready_after_tick(&mut ship, was, &tick(true)), "it was complete already");
        assert!(equipment_box::apply(&mut ship, &tick(false)));
        let was = hangar::fully_fitted(&ship);
        assert!(!ready_after_tick(&mut ship, was, &tick(false)), "a tick taken back");
        assert!(!hangar::field_true(&ship, "einsatzbereit"));
    }
}
