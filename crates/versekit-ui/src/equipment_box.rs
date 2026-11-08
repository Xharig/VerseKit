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

//! The box "Loadout & cart" on the card of a ship (`pages._cart_box` of
//! 4.12): the slots of the ship with the part fitted at the factory, the
//! choice of another part for a slot, and the cart of what is not fitted at
//! the factory, with the two ways (buy or build) for every part, the tick for
//! "fitted", the sum, the crafting time and the shopping route.
//!
//! ⚠ The box changes nothing itself: it returns what was asked
//! ([`Action`]) and the page applies it to the entry of the ship
//! ([`apply`]) and saves.
//!
//! ⚠ Three situations, three sentences: no slot data (nothing can be said),
//! nothing to get (the loadout is the factory one), and parts to get.

use std::collections::HashMap;

use egui::{RichText, Ui};
use versekit_core::cart::{self, Condition, Kind, Line, Sources};
use versekit_core::equipment::{self, Acquisition, Equipment, Equipments, Removed};
use versekit_core::equipment_share::{self, Landing, PartIds, Refused, Shared};
use versekit_core::erkul_slots::{SlotInfo, hardpoints, ship_id};
use versekit_core::hangar::{self, field_text, field_true};
use versekit_core::ships::ShipId;
use versekit_core::jsonio::{Map, Value};
use versekit_core::language::{Arg, t, tf};
use versekit_core::part_choices::{self, Choice, CraftEntry, Origin as PartOrigin, Traits};
use versekit_core::paths::Paths;
use versekit_core::theme::Scheme;

use crate::color;
use crate::crafting_page::duration;
use crate::farm_page::row_button;
use crate::shopping_page::{Stores, cell, plain, route_block, saved_order, spread, sub, text_width, total_block};
use crate::widgets::{self, ButtonKind};

/// The width of the column that names the way in a cart item.
const WAY_WIDTH: f32 = 130.0;

/// What the choice of a part reads besides the stores: the items that can be
/// built, what the blueprint catalog knows about them, and their recipes.
#[derive(Default)]
pub struct Extra {
    pub craft: Vec<CraftEntry>,
    pub traits: HashMap<String, Traits>,
    pub recipes: Vec<RecipeInfo>,
    /// The class names of the parts in the game data, for the share codes.
    pub ids: PartIds,
}

/// The recipe of a part that can be built: its blueprint and the slots its quality is set at.
#[derive(Clone, Debug, Default)]
pub struct RecipeInfo {
    /// The entity the recipe makes, lower case.
    pub entity: String,
    /// The name of the product.
    pub basis: String,
    /// The identifier of the blueprint.
    pub blueprint: String,
    /// The slots of the first tier of the recipe, by their English name.
    pub slots: Vec<String>,
}

/// The recipe of a part, by its identifier or else by its name.
pub fn recipe_for<'e>(extra: &'e Extra, reference: &str, name: &str) -> Option<&'e RecipeInfo> {
    let reference = reference.to_lowercase();
    extra
        .recipes
        .iter()
        .find(|r| !r.entity.is_empty() && r.entity == reference)
        .or_else(|| extra.recipes.iter().find(|r| !name.is_empty() && r.basis.eq_ignore_ascii_case(name)))
}

/// What the box asks of the page.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Put this part into these slots.
    SetPart { paths: Vec<String>, reference: String, name: String },
    /// Back to the factory part.
    Clear { paths: Vec<String> },
    /// Leave these slots empty, without even the factory part.
    Empty { paths: Vec<String> },
    /// Buy or build these slots.
    Method { paths: Vec<String>, way: String },
    /// Tick these slots off, or take the tick back.
    Tick { paths: Vec<String>, on: bool },
    /// Make the named equipment the active one.
    Switch { name: String },
    /// A new equipment, a copy of the active one.
    Create { name: String },
    /// Give an equipment another name.
    Rename { old: String, new: String },
    /// Take an equipment away; `key` is the box it was taken from, which offers to put it back.
    Delete { name: String, key: String },
    /// Put back the equipment taken away last.
    Undo,
    /// How the parts in these slots were got; a built part gets `blueprint`.
    Acquire { paths: Vec<String>, acquisition: Acquisition, blueprint: Option<String> },
    /// The quality of the built parts in these slots at a recipe slot.
    Quality { paths: Vec<String>, slot: String, value: u16 },
    /// Copy the active equipment as a share code; `key` is the box that says what came of it.
    Share { key: String },
    /// Save every equipment of the ship as codes in a file.
    SaveFile { key: String },
    /// Read the codes of a file into the field of the box.
    OpenFile { key: String },
    /// Put these equipments in, read from codes; the last one becomes the active one.
    AddShared { equipments: Vec<Equipment>, key: String },
}

/// Applies a change of the plan (`belegung`) to the entry of a ship; true when something changed. The changes
/// of the named equipments are the page's ([`Action::Switch`] and the ones after it): here they change nothing.
pub fn apply(entry: &mut Map, action: &Action) -> bool {
    let mut changed = false;
    match action {
        Action::SetPart { paths, reference, name } => {
            for path in paths {
                changed |= cart::set_part(entry, path, reference, name, cart::BUY);
            }
        }
        Action::Clear { paths } => {
            for path in paths {
                changed |= cart::clear_part(entry, path);
            }
        }
        Action::Empty { paths } => {
            for path in paths {
                changed |= cart::set_empty(entry, path);
            }
        }
        Action::Method { paths, way } => {
            for path in paths {
                changed |= cart::set_method(entry, path, way);
            }
        }
        Action::Tick { paths, on } => {
            for path in paths {
                changed |= cart::set_done(entry, path, *on);
            }
        }
        Action::Switch { .. }
        | Action::Create { .. }
        | Action::Rename { .. }
        | Action::Delete { .. }
        | Action::Undo
        | Action::Acquire { .. }
        | Action::Quality { .. }
        | Action::Share { .. }
        | Action::SaveFile { .. }
        | Action::OpenFile { .. }
        | Action::AddShared { .. } => {}
    }
    changed
}

/// The field for share codes of one box while it is open, and the text pasted into it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Pasting {
    pub key: String,
    pub text: String,
}

/// The name of the equipment the box is renaming, and the name typed so far.
#[derive(Clone)]
struct Renaming {
    key: String,
    old: String,
    text: String,
    /// The field still has to get the cursor.
    fresh: bool,
    /// What is wrong with the typed name (the key of the sentence), shown below the field.
    problem: Option<&'static str>,
}

/// The parts that fit a group and their lines in the list.
type Listed = std::rc::Rc<(Vec<Choice>, Vec<String>)>;

/// What the boxes remember while the program runs: the lists of parts, the text of the fields and the order
/// of the stops.
#[derive(Default)]
pub struct EquipmentState {
    /// The parts that fit a group, the stock part first, and their lines in the list, by field and group; with
    /// the mark of the sources they came from.
    lists: HashMap<String, (usize, Listed)>,
    /// The text of each field.
    texts: HashMap<String, String>,
    /// The field whose list is open: its text is the player's, not the part's.
    editing: Option<String>,
    renaming: Option<Renaming>,
    /// The equipment taken away last and the box it was taken from: it can be put back.
    pub(crate) removed: Option<(String, Removed)>,
    /// Since when the notice of the delete stands.
    pub(crate) removed_at: Option<f64>,
    order: Vec<String>,
    pub(crate) order_loaded: bool,
    /// Every fold open (for the tests).
    pub(crate) open_all: bool,
    /// The parts of the open boxes that no shop price is known for, `(code, name)`: the page looks them up.
    pub(crate) missing: Vec<(String, String)>,
    /// The field for share codes, while one box has it open.
    pub(crate) pasting: Option<Pasting>,
    /// What sharing, saving or reading codes came to last: the box, the sentence, and whether it went wrong.
    pub(crate) said: Option<(String, String, bool)>,
    /// The box whose list of the equipments of other ships of the same type is open.
    pub(crate) borrowing: Option<String>,
    /// The groups of slots opened into a line for each slot, by the field of the group.
    pub(crate) open_groups: std::collections::HashSet<String>,
}

/// What the box draws from.
pub(crate) struct Ctx<'a> {
    pub stores: &'a Stores,
    pub extra: &'a Extra,
    pub sources: &'a dyn Sources,
    pub paths: Option<&'a Paths>,
    /// The equipments of the other entries of the same ship (own ships and planned ones), with the line that
    /// names each; asked only while the list to take one over is open.
    pub others: Option<&'a OthersOf<'a>>,
}

/// Asks for the equipments of the other entries of the same ship, each with the line that names it.
pub(crate) type OthersOf<'a> = dyn Fn(&Map) -> Vec<(String, Equipment)> + 'a;

/// A fold of the box. Its identifier is the ship, not the title: the count in the title changes with every tick.
fn fold(title: &str, salt: &str, key: &str, open_all: bool) -> egui::CollapsingHeader {
    let header = widgets::collapsing(title).id_salt((salt.to_string(), key.to_string()));
    if open_all { header.open(Some(true)) } else { header }
}

/// What a slot holds by the plan.
#[derive(Clone, Debug, PartialEq)]
enum Fill {
    /// The factory part (or nothing, where the factory fits none).
    Stock,
    /// A part of the player's choosing, `(identifier, name)`.
    Own(String, String),
    /// Left empty on purpose.
    Empty,
}

/// The slots that stand alike in one mount, put into one line. The line can be opened: then each slot of the
/// group has a line of its own below it.
#[derive(Clone)]
struct Group {
    kind: String,
    size: Option<i64>,
    factory: Option<(String, String)>,
    /// The own part all slots of the group hold; `None` where they hold the stock part, are empty or differ.
    own: Option<(String, String)>,
    /// The outermost slot of the list the slots sit in (a turret, a rack, a head), `(path, name)`: the part
    /// chosen there, else the stock one. Slots in different mounts are never one line.
    mount: Option<(String, String)>,
    paths: Vec<String>,
    /// What each slot holds, in the order of `paths`.
    fills: Vec<Fill>,
    /// Each slot of the group, in the order of `paths` (its factory part may differ from the others').
    members: Vec<SlotInfo>,
    slot: SlotInfo,
}

impl Group {
    /// What the slot at `at` really holds: the identifier of its part, `""` for none.
    fn part_at(&self, at: usize) -> &str {
        match &self.fills[at] {
            Fill::Own(reference, _) => reference,
            Fill::Stock => self.members[at].factory.as_ref().map_or("", |(r, _)| r.as_str()),
            Fill::Empty => "",
        }
    }

    /// What all slots hold, or `None` where they differ (also where each holds the factory part of its own
    /// and those differ).
    fn common(&self) -> Option<&Fill> {
        let first = self.fills.first()?;
        let same_part = (0..self.fills.len()).all(|at| self.part_at(at) == self.part_at(0));
        let same_kind = self.fills.iter().all(|f| std::mem::discriminant(f) == std::mem::discriminant(first));
        (same_part && (same_kind || !matches!(first, Fill::Empty))).then_some(first)
    }

    /// The group of one of its slots, for its own line when the group is open.
    fn single(&self, at: usize) -> Group {
        let fill = self.fills[at].clone();
        let slot = self.members[at].clone();
        Group {
            own: match &fill {
                Fill::Own(reference, name) => Some((reference.clone(), name.clone())),
                _ => None,
            },
            factory: slot.factory.clone(),
            paths: vec![self.paths[at].clone()],
            fills: vec![fill],
            members: vec![slot.clone()],
            slot,
            ..self.clone()
        }
    }
}

/// The own choice at a path of the plan, `(identifier, name)`.
fn own_at(chosen: &Map, path: &str) -> Option<(String, String)> {
    chosen
        .iter()
        .find(|(at, _)| at == path)
        .and_then(|(_, part)| part.as_object())
        .map(|part| (field_text(part, "ref").to_string(), field_text(part, "name").to_string()))
        .filter(|(reference, _)| !reference.is_empty())
}

/// What the plan puts at a path.
fn fill_at(chosen: &Map, path: &str) -> Fill {
    if let Some((reference, name)) = own_at(chosen, path) {
        return Fill::Own(reference, name);
    }
    let emptied = chosen.iter().find(|(at, _)| at == path).and_then(|(_, part)| part.as_object()).is_some_and(cart::is_emptied);
    if emptied { Fill::Empty } else { Fill::Stock }
}

/// The mount that holds each slot, `(path, name)`: the outermost slot of the list above it, named by
/// the part in it; else, where erkul lists no slot above it, the parent of the path when that holds several
/// slots (a remote turret with its two guns), named by the parent's own name. A parent with one slot (the
/// hardpoint of a wing gun, the cab of a mining head) is no mount of its own: such slots stand together.
fn mounts_of(slots: &[SlotInfo], chosen: &Map) -> Vec<Option<(String, String)>> {
    // The outermost slot of the list above each slot.
    let above: Vec<Option<&SlotInfo>> = slots
        .iter()
        .map(|slot| {
            slots
                .iter()
                .filter(|s| slot.path.strip_prefix(s.path.as_str()).is_some_and(|rest| rest.starts_with('/')))
                .min_by_key(|s| s.path.len())
        })
        .collect();
    // Counted per parent are the slots right in it, not those that hang in a slot of it (the modules of a head).
    let mut held: HashMap<&str, usize> = HashMap::new();
    for (slot, top) in slots.iter().zip(&above) {
        if top.is_none()
            && let Some((parent, _)) = slot.path.split_once('/')
        {
            *held.entry(parent).or_default() += 1;
        }
    }
    slots
        .iter()
        .zip(&above)
        .map(|(slot, top)| match top {
            Some(s) => {
                let name = own_at(chosen, &s.path)
                    .map(|(_, name)| name)
                    .or_else(|| s.factory.as_ref().map(|(_, name)| name.clone()))
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| plain_port(&s.path));
                Some((s.path.clone(), name))
            }
            None => {
                let (parent, _) = slot.path.split_once('/')?;
                (held.get(parent).copied().unwrap_or(0) > 1).then(|| (parent.to_string(), plain_port(parent)))
            }
        })
        .collect()
}

/// The name of a port as a player reads it: `hardpoint_remote_turret_top` → `remote turret top`.
fn plain_port(path: &str) -> String {
    let last = path.rsplit('/').next().unwrap_or(path);
    let bare = last.strip_prefix("hardpoint_").unwrap_or(last);
    bare.replace('_', " ")
}

/// `pages._slot_list`: slots of the same kind and size in the same mount are one line, in the order of their
/// first appearance, whatever is in them: the line says when they hold different parts (another choice, or
/// another part from the factory), and opened it has a line for each slot. Slots in another mount are another
/// line: the guns of the nose turret and those of the ball turret are fitted apart. Slots whose parent is no
/// mount (the guns on the two wings, the three mining heads of the Mole) stand together. Mounts of the same name
/// are told apart by a number: "Baler Salvage Head 1", "… 2".
fn groups(slots: Vec<SlotInfo>, chosen: &Map) -> Vec<Group> {
    let mut mounts = mounts_of(&slots, chosen);
    // Number the mounts that share a name, in the order they first appear.
    let mut seen: Vec<(String, String)> = Vec::new();
    for (path, name) in mounts.iter().flatten() {
        if !seen.iter().any(|(p, _)| p == path) {
            seen.push((path.clone(), name.clone()));
        }
    }
    for mount in mounts.iter_mut().flatten() {
        let alike: Vec<&String> = seen.iter().filter(|(_, n)| *n == mount.1).map(|(p, _)| p).collect();
        if alike.len() > 1
            && let Some(at) = alike.iter().position(|p| **p == mount.0)
        {
            mount.1 = format!("{} {}", mount.1, at + 1);
        }
    }
    let mut found: Vec<(String, Group)> = Vec::new();
    for (slot, mount) in slots.into_iter().zip(mounts) {
        let fill = fill_at(chosen, &slot.path);
        // A fixed slot never stands in one line with one the player fits.
        let key = format!("{}|{:?}|{}|{}", slot.kind, slot.size, mount.as_ref().map_or("", |(path, _)| path.as_str()), slot.fixed);
        match found.iter_mut().find(|(k, _)| *k == key) {
            Some((_, group)) => {
                group.paths.push(slot.path.clone());
                group.fills.push(fill);
                if group.factory != slot.factory {
                    group.factory = None;
                }
                group.members.push(slot);
            }
            None => {
                let group = Group {
                    kind: slot.kind.clone(),
                    size: slot.size,
                    factory: slot.factory.clone(),
                    own: None,
                    mount,
                    paths: vec![slot.path.clone()],
                    fills: vec![fill],
                    members: vec![slot.clone()],
                    slot,
                };
                found.push((key, group));
            }
        }
    }
    found
        .into_iter()
        .map(|(_, mut group)| {
            group.own = match group.common() {
                Some(Fill::Own(reference, name)) => Some((reference.clone(), name.clone())),
                _ => None,
            };
            group
        })
        .collect()
}

/// What the slots of a group that hold different parts hold: "mixed: 1 × CF-337, 1 × Revenant", in the order
/// the parts first appear.
fn mixed_text(group: &Group) -> String {
    let mut counted: Vec<(String, usize)> = Vec::new();
    for (fill, slot) in group.fills.iter().zip(&group.members) {
        let name = match fill {
            Fill::Own(_, name) => name.clone(),
            Fill::Stock => slot.factory.as_ref().map_or_else(|| t("s_wk_leer"), |(_, name)| name.clone()),
            Fill::Empty => t("s_wk_leer"),
        };
        match counted.iter_mut().find(|(n, _)| *n == name) {
            Some((_, count)) => *count += 1,
            None => counted.push((name, 1)),
        }
    }
    let list: Vec<String> = counted.iter().map(|(name, count)| format!("{count} × {name}")).collect();
    tf("s_wk_gemischt", &[("liste", Arg::from(list.join(", ")))])
}

/// The class of a part as the game writes it, in the language of the window.
fn class_label(class: &str) -> String {
    let key = match class.to_lowercase().as_str() {
        "civilian" => "s_ld_kl_civilian",
        "military" => "s_ld_kl_military",
        "industrial" => "s_ld_kl_industrial",
        "stealth" => "s_ld_kl_stealth",
        "competition" => "s_ld_kl_competition",
        "medical" => "s_ld_kl_medical",
        "mining" => "s_ld_kl_mining",
        "salvage and repair" => "s_ld_kl_salvage",
        _ => return class.to_string(),
    };
    t(key)
}

/// `pages._part_label`: grade and class of a part, "A · Civilian". Not guessed: what neither the choice nor
/// the shop catalog says stays out.
fn part_label(grade: &str, class: &str, craft_only: bool) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !grade.is_empty() {
        parts.push(grade.to_string());
    }
    if !class.is_empty() {
        parts.push(class_label(class));
    }
    if craft_only {
        parts.push(t("s_p9_hg_wegen_bauplan"));
    }
    parts.join(" · ")
}

/// The entry of the choice list: name and what goes after it, so that the search finds the grade and class too.
fn entry_text(choice: &Choice, catalog: &HashMap<&str, (&str, &str)>) -> String {
    let (mut grade, mut class) = (choice.grade.clone(), choice.class.clone());
    if let Some((g, c)) = catalog.get(choice.reference.as_str()) {
        if grade.is_empty() {
            grade = (*g).to_string();
        }
        if class.is_empty() {
            class = (*c).to_string();
        }
    }
    let label = part_label(&grade, &class, choice.origin == PartOrigin::Craftable);
    if label.is_empty() { choice.name.clone() } else { format!("{}  ·  {label}", choice.name) }
}

/// "2 × WeaponGun S3", with the mount the slots sit in before it: "Nose Turret: 2 × WeaponGun S3".
fn group_title(group: &Group) -> String {
    let size = group.size.map_or(String::new(), |s| format!(" S{s}"));
    let what = if group.paths.len() > 1 {
        format!("{} × {}{size}", group.paths.len(), group.kind)
    } else {
        format!("{}{size}", group.kind)
    };
    match &group.mount {
        Some((_, name)) => format!("{name}: {what}"),
        None => what,
    }
}

/// The cart of one ship, built the way the bill is built: the entry stands alone in a made-up fleet.
fn lines_of(entry: &Map, wishlist: bool, sources: &dyn Sources) -> (cart::State, Vec<Line>) {
    let key = if wishlist { "wunsch" } else { "schiffe" };
    let fleet: Map = vec![(key.to_string(), Value::Array(vec![Value::Object(entry.clone())]))];
    let bill = cart::invoice(&fleet, sources);
    let state = cart::line_items(entry, sources).0;
    let mut lines: Vec<Line> = bill.lines.into_iter().filter(|l| l.kind == Kind::Part).collect();
    cart::bill_order(&mut lines);
    (state, lines)
}

/// One way of a part: its word, what it costs or why it cannot be told, and a button to take it.
fn way_row(ui: &mut Ui, scheme: &Scheme, line: &Line, way: &str, actions: &mut Vec<Action>) {
    let buying = way == cart::BUY;
    let active = line.way == way;
    let count = line.count.max(1);
    let money = |single: f64| -> String {
        if count <= 1 {
            plain(single)
        } else {
            tf(
                "s_wk_route_je",
                &[("summe", Arg::from(plain(single * count as f64))), ("preis", Arg::from(plain(single)))],
            )
        }
    };
    let (condition, text) = if buying {
        let buy = &line.buy;
        match (buy.condition, buy.price) {
            (Condition::Known, Some(price)) => {
                // The place often stands in the name of the shop already: not twice.
                let shop = buy.shop.trim();
                let place = buy.place.trim();
                let mut places: Vec<&str> = if shop.is_empty() { Vec::new() } else { vec![shop] };
                if !place.is_empty() && !shop.to_lowercase().contains(&place.to_lowercase()) {
                    places.push(place);
                }
                let shown = if places.is_empty() { "?".to_string() } else { places.join(" · ") };
                let text = tf(
                    "s_wk_kauf_preis",
                    &[("preis", Arg::from(money(price))), ("laden", Arg::from(shown)), ("ort", Arg::from(""))],
                );
                (Condition::Known, text.trim_end_matches([' ', '·']).to_string())
            }
            (Condition::NoPrice, _) => (Condition::NoPrice, t("s_wk_kein_preis")),
            _ => (Condition::NotChecked, t("s_wk_nicht_geprueft")),
        }
    } else {
        let make = &line.craft;
        match (make.condition, make.material) {
            (Condition::Known, Some(material)) => (
                Condition::Known,
                tf(
                    "s_wk_bau_kosten",
                    &[
                        ("preis", Arg::from(money(material))),
                        ("dauer", Arg::from(duration(make.seconds.unwrap_or(0) * count))),
                    ],
                ),
            ),
            (Condition::NoRecipe, _) => (Condition::NoRecipe, t("s_wk_kein_rezept")),
            _ => (Condition::NotChecked, t("s_wk_nicht_geprueft")),
        }
    };
    let word = t(if buying { "s_wk_kaufen" } else { "s_wk_bauen" });
    // A missing price is no missing way; only "no blueprint" takes the button away.
    let button = !active && condition != Condition::NoRecipe;
    let gap = ui.spacing().item_spacing.x;
    let button_width = if button { text_width(ui, &word, egui::TextStyle::Button, false) + 24.0 } else { 0.0 };
    let left_width = WAY_WIDTH + text_width(ui, &text, egui::TextStyle::Small, false) + 2.0 * gap;
    spread(
        ui,
        left_width,
        button_width,
        0.0,
        |row| {
            let label = if active {
                widgets::bold(row, word.clone(), egui::TextStyle::Small).color(color(scheme.accent))
            } else {
                sub(scheme, word.clone())
            };
            cell(row, WAY_WIDTH, label);
            let tone = if condition == Condition::Known { scheme.fg } else { scheme.sub };
            row.label(RichText::new(text.clone()).small().color(color(tone)));
        },
        |right| {
            if button {
                let meaning = t(if buying { "s_p9_hg_weg_kaufen" } else { "s_p9_hg_weg_bauen" });
                let response = right.add(widgets::Button::new(&word, ButtonKind::Normal, scheme));
                let who = format!("{}: {meaning}", line.name);
                response.widget_info(|| egui::WidgetInfo::labeled(egui::accesskit::Role::Button, true, who.clone()));
                if response.clicked() {
                    actions.push(Action::Method { paths: line.paths.clone(), way: way.to_string() });
                }
            }
        },
    );
}

/// One part of the cart with both ways side by side.
fn cart_item(ui: &mut Ui, scheme: &Scheme, line: &Line, actions: &mut Vec<Action>) {
    let frame =
        egui::Frame::new().fill(color(scheme.field)).corner_radius(4).inner_margin(egui::Margin::symmetric(8, 6));
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|row| {
            let (icon, meaning) =
                if line.done { ("circle-check", t("s_p9_hg_haken_weg")) } else { ("circle", t("s_p9_hg_haken")) };
            if row_button(row, scheme, icon, &meaning, &line.name, true) {
                actions.push(Action::Tick { paths: line.paths.clone(), on: !line.done });
            }
            let name = if line.count > 1 { format!("{} × {}", line.count, line.name) } else { line.name.clone() };
            let tone = if line.done { scheme.sub } else { scheme.fg };
            row.label(widgets::bold(row, name, egui::TextStyle::Body).color(color(tone)));
            let hint = if line.done {
                t("s_wk_eingebaut")
            } else if line.factory_name.is_empty() {
                t("s_wk_zusaetzlich")
            } else {
                tf("s_wk_statt", &[("name", Arg::from(line.factory_name.clone()))])
            };
            row.label(sub(scheme, hint));
        });
        // Once it is fitted, the question "buy or build" is answered.
        if line.done {
            return;
        }
        for way in [cart::BUY, cart::CRAFT] {
            way_row(ui, scheme, line, way, actions);
        }
        // A raw material no shop sells is not free but has to be mined: the sum is a lower bound.
        if !line.craft.unpriced.is_empty() {
            let note = tf("s_wk_ohne_preis", &[("rohstoffe", Arg::from(line.craft.unpriced.join(", ")))]);
            ui.add(egui::Label::new(RichText::new(note).small().color(color(scheme.gold))).wrap());
        }
    });
    ui.add_space(4.0);
}

/// The width of the column that names the slot.
const TITLE_WIDTH: f32 = 130.0;
/// Below this width of a field the name of the slot stands above it.
const FIELD_MIN_WIDTH: f32 = 170.0;

/// The same part, by its reference or by its name.
fn same_part(choice: &Choice, reference: &str, name: &str) -> bool {
    (!reference.is_empty() && choice.reference == reference)
        || (!name.is_empty() && choice.name.eq_ignore_ascii_case(name))
}

/// The part that sits in a slot: by its reference where both have one, else by its name.
fn sits(choice: &Choice, reference: &str, name: &str) -> bool {
    if reference.is_empty() || choice.reference.is_empty() {
        !name.is_empty() && choice.name.eq_ignore_ascii_case(name)
    } else {
        choice.reference == reference
    }
}

/// The parts of a slot by size, then by name (`size_order`): a slot that takes several sizes shows them in
/// order, a part without a size last.
fn in_size_order(mut found: Vec<Choice>) -> Vec<Choice> {
    found.sort_by_cached_key(|c| versekit_core::size_order::key(c.size, &c.name));
    found
}

/// The parts of a slot with the stock part at the top and not a second time further down. A stock part the
/// sources do not know stands at the top all the same.
fn stock_first(mut found: Vec<Choice>, factory: Option<&(String, String)>) -> Vec<Choice> {
    let Some((reference, name)) = factory else { return found };
    let stock = match found.iter().position(|c| same_part(c, reference, name)) {
        Some(at) => found.remove(at),
        None => Choice {
            name: name.clone(),
            reference: reference.clone(),
            maker: String::new(),
            grade: String::new(),
            class: String::new(),
            origin: PartOrigin::ShipOnly,
            size: None,
        },
    };
    found.retain(|c| !same_part(c, reference, name));
    found.insert(0, stock);
    found
}

/// What the sources are: a list worked out from other ones is worked out again.
fn sources_mark(ctx: &Ctx) -> usize {
    std::ptr::from_ref(ctx.stores) as usize
        ^ ctx.extra.craft.len().rotate_left(16)
        ^ ctx.stores.catalog.len().rotate_left(32)
}

/// The key of the list of a field: the field, what the group takes and the language of its lines. A rack
/// swapped puts other slots under the same path.
fn list_key(id: &str, group: &Group) -> String {
    let factory = group.factory.as_ref().map_or("", |(r, _)| r.as_str());
    format!("{id}|{}|{:?}|{factory}|{:?}", group.kind, group.size, versekit_core::language::current())
}

/// The parts that fit a group, the stock part first, and their lines in the list: worked out once per field
/// and kept while the sources stay.
fn group_choices(
    ctx: &Ctx,
    state: &mut EquipmentState,
    id: &str,
    group: &Group,
    catalog: &HashMap<&str, (&str, &str)>,
) -> Listed {
    let mark = sources_mark(ctx);
    let key = list_key(id, group);
    if let Some((seen, listed)) = state.lists.get(&key)
        && *seen == mark
    {
        return listed.clone();
    }
    let sources = part_choices::Sources {
        craft: &ctx.extra.craft,
        traits: &ctx.extra.traits,
        shop: &ctx.stores.catalog,
        parts: &ctx.stores.erkul.parts,
    };
    let found = part_choices::choices(&sources, &group.kind, group.size, Some(&group.slot));
    let choices = stock_first(in_size_order(found), group.factory.as_ref());
    let stock = usize::from(group.factory.is_some());
    let entries = choices
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let text = entry_text(c, catalog);
            if i < stock { tf("s_wk_ab_werk", &[("name", Arg::from(text))]) } else { text }
        })
        .collect();
    let listed = std::rc::Rc::new((choices, entries));
    state.lists.insert(key, (mark, listed.clone()));
    listed
}

/// The slots of the ship, one line per group: the name of the slot, a field that is list and search in one
/// with the stock part at the top, and the way back to the stock part where an own part sits; below an own
/// part how it was got (from the `active` equipment).
#[allow(clippy::too_many_arguments)]
fn slot_list(
    ui: &mut Ui,
    scheme: &Scheme,
    ctx: &Ctx,
    state: &mut EquipmentState,
    key: &str,
    groups: &[Group],
    active: Option<&Equipment>,
    actions: &mut Vec<Action>,
) {
    ui.label(widgets::bold(ui, t("s_wk_auslegung"), egui::TextStyle::Body));
    let stacked = stacked_lines(ui);
    let catalog: HashMap<&str, (&str, &str)> =
        ctx.stores.catalog.iter().map(|p| (p.code.as_str(), (p.grade.as_str(), p.class.as_str()))).collect();
    let line_of = LineOf { scheme, ctx, stacked, catalog: &catalog, active };
    for group in groups {
        let id = format!("equipment-pick-{key}-{}", group.paths[0]);
        let title = group_title(group);
        // A group of several slots opens below its line into a line for each slot; at the start it is shut.
        let open = (group.paths.len() > 1).then(|| state.open_groups.contains(&id));
        if group_line(ui, &line_of, state, &id, group, (&title, &title), open, actions) && !state.open_groups.remove(&id) {
            state.open_groups.insert(id.clone());
        }
        // Opened: a line for each slot right below, in the same columns as every other line; its name says the
        // group too, for the screen reader.
        if open == Some(true) {
            for at in 0..group.paths.len() {
                let one = group.single(at);
                let slot_id = format!("equipment-slot-{key}-{}", one.paths[0]);
                let slot_title = tf("s_wk_platz_n", &[("n", Arg::from((at + 1).to_string()))]);
                let spoken = format!("{title}, {slot_title}");
                group_line(ui, &line_of, state, &slot_id, &one, (&slot_title, &spoken), None, actions);
            }
        }
    }
}

/// What every line of the slot list draws from.
struct LineOf<'a> {
    scheme: &'a Scheme,
    ctx: &'a Ctx<'a>,
    stacked: bool,
    catalog: &'a HashMap<&'a str, (&'a str, &'a str)>,
    active: Option<&'a Equipment>,
}

/// One line of the slot list: the field that is list and search in one with the stock part at the top and
/// "leave empty" at the end, the way back to the stock part, and for a group of several slots the arrow that
/// opens it (`open` says whether it is open); a line without the arrow keeps its place free, so that field and
/// buttons stand at the same edges in every line. What the slots hold differs: the field says "mixed: …".
/// `names` is the name shown and the one for the screen reader. True when the arrow was clicked.
#[allow(clippy::too_many_arguments)]
fn group_line(
    ui: &mut Ui,
    of: &LineOf,
    state: &mut EquipmentState,
    id: &str,
    group: &Group,
    names: (&str, &str),
    open: Option<bool>,
    actions: &mut Vec<Action>,
) -> bool {
    if group.slot.fixed {
        fixed_line(ui, of, id, group, names);
        return false;
    }
    let (title, spoken) = names;
    let (scheme, ctx, catalog) = (of.scheme, of.ctx, of.catalog);
    let own = group
        .own
        .as_ref()
        .filter(|(reference, _)| Some(reference.as_str()) != group.factory.as_ref().map(|(r, _)| r.as_str()));
    let listed = group_choices(ctx, state, id, group, catalog);
    let (choices, entries) = (&listed.0, &listed.1);
    let stock = usize::from(group.factory.is_some());
    // The end of the list: leave the slots empty. Where the factory fits nothing they are empty already.
    let leave_empty = (stock > 0 || own.is_some()).then(|| t("s_wk_leer_lassen"));
    let mut all: Vec<String> = entries.to_vec();
    all.extend(leave_empty.iter().cloned());
    // The part that sits in the slots, as its line in the list reads.
    let shown = match (group.common(), own) {
        (None, _) => mixed_text(group),
        (Some(Fill::Empty), _) => t("s_wk_leer"),
        (_, Some((reference, name))) => match choices.iter().position(|c| same_part(c, reference, name)) {
            Some(at) => entries[at].clone(),
            None => {
                let label = catalog.get(reference.as_str()).map_or(String::new(), |(g, c)| part_label(g, c, false));
                if label.is_empty() { name.clone() } else { format!("{name}  ·  {label}") }
            }
        },
        _ if stock > 0 => entries[0].clone(),
        _ => String::new(),
    };
    // Back to the stock part goes where any slot of the line holds something else.
    let factory_ref = group.factory.as_ref().map(|(r, _)| r.as_str());
    let reset = group.fills.iter().any(|f| match f {
        Fill::Stock => false,
        Fill::Own(reference, _) => Some(reference.as_str()) != factory_ref,
        Fill::Empty => true,
    });
    let field = Field {
        id,
        name: &format!("{spoken}: {}", t("s_p9_hg_auswaehlen")),
        hint: &t(if stock > 0 { "s_pl_teil" } else { "s_wk_ab_werk_leer" }),
        // No part at all, or none that holds what was typed.
        empty: &t(if entries.is_empty() { "s_p9_hg_keine_auswahl" } else { "s_mu_auswahl_leer" }),
    };
    let mut toggled = false;
    line(ui, scheme, of.stacked, title, |row| {
        let width = (row.available_width() - button_room(row, 2)).max(1.0);
        let (picked, _) = select(row, scheme, state, &field, &shown, &all, width);
        match picked {
            Some(at) if at == entries.len() => {
                if group.common() != Some(&Fill::Empty) {
                    actions.push(Action::Empty { paths: group.paths.clone() });
                }
            }
            // The stock part is "back to the stock part".
            Some(0) if stock > 0 => {
                if reset {
                    actions.push(Action::Clear { paths: group.paths.clone() });
                }
            }
            Some(at) => {
                // The part that sits there already stays as it is, with its way, tick and quality.
                if let Some(choice) = choices.get(at)
                    && !own.is_some_and(|(reference, name)| sits(choice, reference, name))
                {
                    actions.push(Action::SetPart {
                        paths: group.paths.clone(),
                        reference: choice.reference.clone(),
                        name: choice.name.clone(),
                    });
                }
            }
            None => {}
        }
        // The button keeps its place; at the stock part it is off, with the reason on hover.
        let meaning = t(match (reset, stock > 0) {
            (true, _) => "s_wk_auf_werk",
            (false, true) => "s_wk_nichts_zurueck",
            (false, false) => "s_wk_nichts_zurueck_leer",
        });
        if row_button(row, scheme, "undo-2", &meaning, spoken, reset) {
            actions.push(Action::Clear { paths: group.paths.clone() });
        }
        match open {
            Some(open) => {
                let (icon, meaning) = if open {
                    ("chevron-down", t("s_wk_zuklappen"))
                } else {
                    ("chevron-right", t("s_wk_aufklappen"))
                };
                toggled = row_button(row, scheme, icon, &meaning, spoken, true);
            }
            // The place of the arrow stays free.
            None => {
                row.allocate_exact_size(egui::vec2(widgets::ICON_BUTTON_SIZE, widgets::ICON_BUTTON_SIZE), egui::Sense::hover());
            }
        }
    });
    // A port the game data give no size (one right on the ship, empty at the factory): every part of its kind.
    if group.size.is_none() && group.kind != "Paints" {
        ui.add(egui::Label::new(sub(scheme, t("s_wk_groesse_offen"))).wrap());
    }
    if let Some((reference, name)) = own {
        let part = Part { reference, name, stacked: of.stacked };
        slot_details(ui, scheme, ctx, state, id, group, &part, of.active, actions);
    }
    toggled
}

/// The line of a fixed slot: its factory part in a field that is off, with the reason on hover, and the button
/// off in its place, so that the line stands at the same edges as every other one. It has nothing to open.
fn fixed_line(ui: &mut Ui, of: &LineOf, id: &str, group: &Group, names: (&str, &str)) {
    let (title, spoken) = names;
    let part = match &group.factory {
        Some((reference, name)) => {
            let label = of.catalog.get(reference.as_str()).map_or(String::new(), |(g, c)| part_label(g, c, false));
            if label.is_empty() { name.clone() } else { format!("{name}  ·  {label}") }
        }
        None => mixed_text(group),
    };
    let shown = tf("s_wk_fest_teil", &[("name", Arg::from(part))]);
    let reason = t("s_wk_fest_tip");
    let name = format!("{spoken}: {shown}");
    line(ui, of.scheme, of.stacked, title, |row| {
        let width = (row.available_width() - button_room(row, 2)).max(1.0);
        let height = row.spacing().interact_size.y;
        let mut text = shown.clone();
        let field = row.add_enabled_ui(false, |inner| {
            inner.allocate_ui(egui::vec2(width, height), |inner| {
                widgets::SelectField::new(id, &name, "", "", of.scheme).show::<String>(inner, &mut text, &[]);
            });
        });
        // egui shows no tooltip on a disabled widget by itself: the reason has to be asked for.
        field.response.on_disabled_hover_text(&reason);
        row_button(row, of.scheme, "undo-2", &reason, spoken, false);
        row.allocate_exact_size(egui::vec2(widgets::ICON_BUTTON_SIZE, widgets::ICON_BUTTON_SIZE), egui::Sense::hover());
    });
}

/// Whether the lines of the box put their name above instead of beside them: one choice for the whole box,
/// so that every field starts at the same edge.
fn stacked_lines(ui: &Ui) -> bool {
    ui.available_width() < TITLE_WIDTH + ui.spacing().item_spacing.x + FIELD_MIN_WIDTH + button_room(ui, 3)
}

/// The room `count` icon buttons take at the end of a line.
fn button_room(ui: &Ui, count: usize) -> f32 {
    count as f32 * (widgets::ICON_BUTTON_SIZE + ui.spacing().item_spacing.x)
}

/// A line of the box: its name in the column of the names, or above the line when the box is narrow, and
/// what `add` puts after it.
fn line<R>(ui: &mut Ui, scheme: &Scheme, stacked: bool, title: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    if stacked && !title.is_empty() {
        ui.label(sub(scheme, title.to_string()));
    }
    ui.horizontal(|row| {
        if !stacked {
            cell(row, TITLE_WIDTH, sub(scheme, title.to_string()));
        }
        add(row)
    })
    .inner
}

/// What a select field is called and says.
struct Field<'a> {
    id: &'a str,
    /// For screen readers.
    name: &'a str,
    /// In the empty field.
    hint: &'a str,
    /// In the list when nothing matches.
    empty: &'a str,
}

/// A select field `width` wide whose text is `shown` while its list is closed; what was picked in it and the
/// text it holds.
fn select(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &mut EquipmentState,
    field: &Field,
    shown: &str,
    entries: &[String],
    width: f32,
) -> (Option<usize>, String) {
    let mut text = state.texts.remove(field.id).unwrap_or_default();
    if state.editing.as_deref() != Some(field.id) {
        text = shown.to_string();
    }
    let height = ui.spacing().interact_size.y;
    let out = ui
        .allocate_ui(egui::vec2(width, height), |inner| {
            widgets::SelectField::new(field.id, field.name, field.hint, field.empty, scheme)
                .show(inner, &mut text, entries)
        })
        .inner;
    if out.is_open {
        state.editing = Some(field.id.to_string());
    } else if state.editing.as_deref() == Some(field.id) {
        state.editing = None;
    }
    state.texts.insert(field.id.to_string(), text.clone());
    (out.picked, text)
}

/// The ways a part can be got, in the order of the list.
const WAYS: [Acquisition; 5] =
    [Acquisition::Purchased, Acquisition::Crafted, Acquisition::Pledge, Acquisition::Looted, Acquisition::Unknown];

/// The way a part was got, in the language of the window.
fn acquisition_word(acquisition: Acquisition) -> String {
    t(match acquisition {
        Acquisition::Purchased => "s_wk_hk_purchased",
        Acquisition::Crafted => "s_wk_hk_crafted",
        Acquisition::Pledge => "s_wk_hk_pledge",
        Acquisition::Looted => "s_wk_hk_looted",
        Acquisition::Unknown => "s_wk_hk_unknown",
    })
}

/// The own part of a slot and whether the lines of the box put their name above them.
struct Part<'a> {
    reference: &'a str,
    name: &'a str,
    stacked: bool,
}

/// Below a slot with an own part: how it was got, and for a built one its blueprint and the quality per
/// recipe slot (0 to 1000, 500 when none is set, as on the crafting page).
#[allow(clippy::too_many_arguments)]
fn slot_details(
    ui: &mut Ui,
    scheme: &Scheme,
    ctx: &Ctx,
    state: &mut EquipmentState,
    id: &str,
    group: &Group,
    part: &Part,
    active: Option<&Equipment>,
    actions: &mut Vec<Action>,
) {
    let slot = active.and_then(|e| e.slot(&group.paths[0]));
    let acquisition = slot.map_or(Acquisition::Unknown, |s| s.acquisition);
    let label = t("s_wk_herkunft");
    let words: Vec<String> = WAYS.iter().map(|a| acquisition_word(*a)).collect();
    let field_id = format!("{id}-herkunft");
    let field = Field {
        id: &field_id,
        name: &format!("{}: {label}", part.name),
        hint: &t("s_wk_herkunft_ph"),
        empty: &t("s_mu_auswahl_leer"),
    };
    line(ui, scheme, part.stacked, &label, |row| {
        let width = (row.available_width() - button_room(row, 1)).max(1.0);
        let (picked, _) = select(row, scheme, state, &field, &acquisition_word(acquisition), &words, width);
        if let Some(way) = picked.map(|at| WAYS[at])
            && way != acquisition
        {
            let blueprint = (way == Acquisition::Crafted)
                .then(|| recipe_for(ctx.extra, part.reference, part.name).map(|r| r.blueprint.clone()))
                .flatten();
            actions.push(Action::Acquire { paths: group.paths.clone(), acquisition: way, blueprint });
        }
    });
    if acquisition != Acquisition::Crafted {
        return;
    }
    let recipe = recipe_for(ctx.extra, part.reference, part.name);
    line(ui, scheme, part.stacked, &t("s_wk_bauplan"), |row| {
        let text = recipe.map_or_else(|| t("s_wk_bauplan_fehlt"), |r| r.basis.clone());
        row.add(egui::Label::new(text).wrap());
    });
    let Some(recipe) = recipe else { return };
    for recipe_slot in &recipe.slots {
        let current = slot.map_or(equipment::BASE_QUALITY, |s| s.quality_at(recipe_slot));
        let name = tf("s_wk_qualitaet", &[("slot", Arg::from(recipe_slot.clone()))]);
        line(ui, scheme, part.stacked, &name, |row| {
            let gap = row.spacing().item_spacing.x;
            let number = text_width(row, "1000", egui::TextStyle::Body, false) + gap;
            let width = (row.available_width() - number - button_room(row, 1)).max(40.0);
            let mut value = i64::from(current);
            let response = row.add(widgets::Slider::new(&mut value, 0, 1000, scheme).width(width).on_release());
            let shown = widgets::Slider::shown(row, &response).unwrap_or(value);
            let enabled = response.enabled();
            response.widget_info(|| egui::WidgetInfo::slider(enabled, shown as f64, &name));
            row.label(shown.to_string());
            if response.changed() {
                let value = u16::try_from(value.clamp(0, 1000)).unwrap_or(equipment::BASE_QUALITY);
                actions.push(Action::Quality { paths: group.paths.clone(), slot: recipe_slot.clone(), value });
            }
        });
    }
}

/// The first name of the form "Loadout 2" no equipment has.
fn free_name(names: &[String]) -> String {
    (names.len() + 1..)
        .map(|n| tf("s_wk_ausst_neu_name", &[("n", Arg::from(n))]))
        .find(|name| !names.contains(name))
        .unwrap_or_default()
}

/// The named equipments of the ship at the top of the box: the active one in a select field (a pick makes
/// another one active), a new one, rename, delete, and the way back after a delete.
fn loadout_bar(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &mut EquipmentState,
    all: &Equipments,
    key: &str,
    actions: &mut Vec<Action>,
) {
    let stacked = stacked_lines(ui);
    let mut names: Vec<String> = all.0.iter().map(|e| e.name.clone()).collect();
    let active = all.0.iter().position(|e| e.active);
    if names.is_empty() {
        names.push(equipment::DEFAULT_NAME.to_string());
    }
    let active = active.unwrap_or(0).min(names.len() - 1);
    let current = names[active].clone();
    let renaming = if state.renaming.as_ref().is_some_and(|r| r.key == key) { state.renaming.take() } else { None };
    let label = t("s_wk_ausst_name");
    line(ui, scheme, stacked, &label, |row| {
        if let Some(mut renaming) = renaming {
            let width = (row.available_width() - button_room(row, 3)).max(1.0);
            let wrong = renaming.problem.is_some();
            let response = row
                .scope(|field| {
                    if wrong {
                        let red = egui::Stroke::new(1.0, color(scheme.red));
                        let look = &mut field.visuals_mut().widgets;
                        (look.inactive.bg_stroke, look.hovered.bg_stroke) = (red, red);
                        field.visuals_mut().selection.stroke.color = color(scheme.red);
                    }
                    widgets::clearable_field(field, &mut renaming.text, &t("s_wk_ausst_name_ph"), scheme, width)
                })
                .inner;
            if renaming.fresh {
                response.request_focus();
                renaming.fresh = false;
            }
            if response.changed() {
                renaming.problem = None;
            }
            let enter = response.lost_focus() && row.input(|i| i.key_pressed(egui::Key::Enter));
            let escape = response.lost_focus() && row.input(|i| i.key_pressed(egui::Key::Escape));
            let apply = row_button(row, scheme, "check", &t("s_wk_ausst_uebernehmen"), &renaming.text, true) || enter;
            let cancel = row_button(row, scheme, "x", &t("s_wk_ausst_abbrechen"), &renaming.old, true) || escape;
            let new = renaming.text.trim().to_string();
            if cancel || (apply && new == renaming.old) {
                return;
            }
            if apply {
                if new.is_empty() {
                    renaming.problem = Some("s_wk_ausst_leer");
                } else if names.contains(&new) {
                    renaming.problem = Some("s_wk_ausst_doppelt");
                } else {
                    actions.push(Action::Rename { old: renaming.old.clone(), new });
                    return;
                }
                // Enter took the cursor out of the field: it goes back for the correction.
                renaming.fresh = true;
            }
            state.renaming = Some(renaming);
            return;
        }
        let width = (row.available_width() - button_room(row, 3)).max(1.0);
        let field_id = format!("equipment-set-{key}");
        let field = Field { id: &field_id, name: &label, hint: &t("s_wk_ausst_ph"), empty: &t("s_mu_auswahl_leer") };
        let (picked, _) = select(row, scheme, state, &field, &current, &names, width);
        if let Some(at) = picked
            && at != active
        {
            actions.push(Action::Switch { name: names[at].clone() });
        }
        if row_button(row, scheme, "plus", &t("s_wk_ausst_neu"), &current, true) {
            let name = free_name(&names);
            actions.push(Action::Create { name: name.clone() });
            state.renaming =
                Some(Renaming { key: key.to_string(), old: name.clone(), text: name, fresh: true, problem: None });
        }
        if row_button(row, scheme, "pencil", &t("s_wk_ausst_umbenennen"), &current, true) {
            state.renaming = Some(Renaming {
                key: key.to_string(),
                old: current.clone(),
                text: current.clone(),
                fresh: true,
                problem: None,
            });
        }
        // The button keeps its place; with one equipment only it is off, with the reason on hover.
        let more = names.len() > 1;
        let meaning = t(if more { "s_wk_ausst_loeschen" } else { "s_wk_ausst_letzte" });
        if row_button(row, scheme, "trash-2", &meaning, &current, more) {
            actions.push(Action::Delete { name: current.clone(), key: key.to_string() });
        }
    });
    // The sentence stands below the field, in its column.
    if let Some(problem) = state.renaming.as_ref().filter(|r| r.key == key).and_then(|r| r.problem) {
        line(ui, scheme, stacked, "", |row| {
            row.add(egui::Label::new(RichText::new(t(problem)).small().color(color(scheme.red_text()))).wrap());
        });
    }
    // The way back stands a few seconds at the top right, as everywhere.
    if let Some((_, removed)) = state.removed.as_ref().filter(|(k, _)| k == key) {
        let now = ui.input(|i| i.time);
        let since = *state.removed_at.get_or_insert(now);
        let said = tf("s_mu_geloescht", &[("name", Arg::from(removed.name()))]);
        let word = t("s_p9_rueckgaengig");
        let notice = widgets::notice(ui.ctx(), scheme, since, "trash-2", &said, Some(&word));
        if notice.acted {
            actions.push(Action::Undo);
        } else if !notice.shown {
            (state.removed, state.removed_at) = (None, None);
        }
    }
}

/// Whether a share code is for the ship of this entry: the same ship by the rules of the hangar, the same name,
/// or the same ship in the slot data.
pub(crate) fn fits_ship(entry: &Map, shared: &Shared, ctx: &Ctx) -> bool {
    same_type(entry, &shared.ship, &shared.maker, ctx.stores)
}

/// Whether the entry is a ship of this name and maker: the same ship by the rules of the hangar, the same name, or
/// the same ship in the slot data.
pub(crate) fn same_type(entry: &Map, ship: &str, maker: &str, stores: &Stores) -> bool {
    if hangar::same_ship(entry, ship, maker, "", "") || hangar::slim(field_text(entry, "name")) == hangar::slim(ship) {
        return true;
    }
    let ships = &stores.erkul.ships;
    let theirs = ShipId { name: ship, maker, short: "", maker_short: "" };
    match (ships.entry(ship_id(entry)), ships.entry(theirs)) {
        (Some(own), Some(other)) => std::ptr::eq(own, other),
        _ => false,
    }
}

/// The sentence for a code that cannot be read.
fn refusal(refused: Refused) -> String {
    t(match refused {
        Refused::NoCode => "s_wk_code_keiner",
        Refused::Newer => "s_wk_code_neuer",
        Refused::Damaged => "s_wk_code_kaputt",
    })
}

/// What a code brings, part by part with how many: `(line, unknown)`.
fn arrival_lines(shared: &Shared, ctx: &Ctx) -> Vec<(String, bool)> {
    let known = |guid: &str| ctx.stores.erkul.parts.get(guid).is_some();
    let mut counted: Vec<(String, usize, bool)> = Vec::new();
    for (_, token, landing) in equipment_share::arrivals(shared, &ctx.extra.ids, &known) {
        let (what, unknown) = match landing {
            Landing::Part(guid) => {
                let name = ctx.stores.erkul.part_name(&guid);
                (if name == guid { token } else { name }, false)
            }
            Landing::Empty => (String::new(), false),
            Landing::Unknown => (token, true),
        };
        match counted.iter_mut().find(|(w, _, u)| *w == what && *u == unknown) {
            Some(found) => found.1 += 1,
            None => counted.push((what, 1, unknown)),
        }
    }
    let mut lines: Vec<(String, bool)> = counted
        .into_iter()
        .map(|(what, n, unknown)| {
            let n = Arg::from(n);
            if unknown {
                (tf("s_wk_code_unbekannt", &[("n", n), ("kennung", Arg::from(what))]), true)
            } else if what.is_empty() {
                (tf("s_wk_code_leer", &[("n", n)]), false)
            } else {
                (tf("s_wk_code_teil", &[("n", n), ("teil", Arg::from(what))]), false)
            }
        })
        .collect();
    if let Some(paint) = &shared.paint {
        lines.push(match equipment_share::land(paint, &ctx.extra.ids, &known) {
            Landing::Part(guid) => (tf("s_wk_code_lack", &[("teil", Arg::from(ctx.stores.erkul.part_name(&guid)))]), false),
            _ => (tf("s_wk_code_lack_unbekannt", &[("kennung", Arg::from(paint.as_str()))]), true),
        });
    }
    if lines.is_empty() {
        lines.push((t("s_wk_code_nichts"), false));
    }
    lines
}

/// The field for codes and what they bring, before anything is put in: per code the ship and the name, the parts,
/// an unknown part said as such; a code of another ship or one that cannot be read is said and left out.
fn paste_panel(ui: &mut Ui, scheme: &Scheme, ctx: &Ctx, state: &mut EquipmentState, entry: &Map, key: &str, actions: &mut Vec<Action>) {
    let Some(pasting) = state.pasting.as_mut().filter(|p| p.key == key) else { return };
    let width = ui.available_width();
    let label = ui.label(RichText::new(t("s_wk_einfuegen_titel")).small().color(color(scheme.sub)));
    let field = ui.add(widgets::text_area(&mut pasting.text, &t("s_wk_einfuegen_ph"), scheme).desired_width(width).desired_rows(2));
    field.labelled_by(label.id);
    let text = pasting.text.clone();
    let mut ready: Vec<Equipment> = Vec::new();
    if !text.trim().is_empty() {
        let found = equipment_share::read_all(&text);
        if found.is_empty() {
            ui.add(egui::Label::new(RichText::new(refusal(Refused::NoCode)).small().color(color(scheme.red_text()))).wrap());
        }
        let known = |guid: &str| ctx.stores.erkul.parts.get(guid).is_some();
        for result in found {
            ui.add_space(4.0);
            let shared = match result {
                Ok(shared) => shared,
                Err(refused) => {
                    ui.add(egui::Label::new(RichText::new(refusal(refused)).small().color(color(scheme.red_text()))).wrap());
                    continue;
                }
            };
            let name = if shared.name.is_empty() { equipment::DEFAULT_NAME.to_string() } else { shared.name.clone() };
            let head = tf("s_wk_code_schiff", &[("name", Arg::from(name)), ("schiff", Arg::from(shared.ship.as_str()))]);
            ui.add(egui::Label::new(widgets::bold(ui, head, egui::TextStyle::Body)).wrap());
            if !fits_ship(entry, &shared, ctx) {
                let other = tf("s_wk_code_anderes", &[("schiff", Arg::from(shared.ship.as_str()))]);
                ui.add(egui::Label::new(RichText::new(other).small().color(color(scheme.red_text()))).wrap());
                continue;
            }
            for (line, unknown) in arrival_lines(&shared, ctx) {
                let tone = if unknown { scheme.gold } else { scheme.sub };
                ui.add(egui::Label::new(RichText::new(line).small().color(color(tone))).wrap());
            }
            ready.push(equipment_share::equipment_of(&shared, &ctx.extra.ids, &known));
        }
        if !ready.is_empty() {
            ui.add(egui::Label::new(sub(scheme, t("s_wk_code_hinweis"))).wrap());
        }
    }
    ui.horizontal_wrapped(|row| {
        let word = if ready.len() > 1 { tf("s_wk_code_anlegen_n", &[("n", Arg::from(ready.len()))]) } else { t("s_wk_code_anlegen") };
        let reason = t("s_wk_code_erst");
        let add = widgets::Button::new(&word, ButtonKind::Strong, scheme).icon("plus");
        if row.add(add.blocked(ready.is_empty().then_some(reason.as_str()))).clicked() && !ready.is_empty() {
            actions.push(Action::AddShared { equipments: std::mem::take(&mut ready), key: key.to_string() });
            state.pasting = None;
        }
        let cancel = t("s_wk_code_schliessen");
        if row.add(widgets::Button::new(&cancel, ButtonKind::Normal, scheme).icon("x")).clicked() {
            state.pasting = None;
        }
    });
}

/// The equipments of the other entries of the same ship, each with a button that takes it over as a new equipment
/// of this one (and makes it the active one); a sentence where there is none.
fn borrow_panel(ui: &mut Ui, scheme: &Scheme, ctx: &Ctx, state: &mut EquipmentState, entry: &Map, key: &str, actions: &mut Vec<Action>) {
    let (Some(others), true) = (ctx.others, state.borrowing.as_deref() == Some(key)) else { return };
    let found = others(entry);
    if found.is_empty() {
        ui.add(egui::Label::new(sub(scheme, t("s_wk_uebernehmen_keine"))).wrap());
    }
    for (line, equipment) in found {
        ui.horizontal_wrapped(|row| {
            let word = t("s_wk_uebernehmen_knopf");
            let spoken = tf("s_wk_uebernehmen_knopf_fuer", &[("was", Arg::from(line.as_str()))]);
            if row.add(widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("plus").spoken(&spoken)).clicked() {
                actions.push(Action::AddShared { equipments: vec![equipment.clone()], key: key.to_string() });
                state.borrowing = None;
            }
            row.add(egui::Label::new(RichText::new(line.as_str()).small().color(color(scheme.sub))).wrap());
        });
    }
}

/// Share the active equipment as a code, put a code in, and the same for every equipment of the ship as a file.
fn share_bar(ui: &mut Ui, scheme: &Scheme, ctx: &Ctx, state: &mut EquipmentState, entry: &Map, key: &str, actions: &mut Vec<Action>) {
    let ship = field_text(entry, "name");
    let button = |row: &mut Ui, word: &str, icon: &str, spoken: &str, tip: &str| {
        let spoken = tf(spoken, &[("schiff", Arg::from(ship))]);
        row.add(widgets::Button::new(word, ButtonKind::Normal, scheme).icon(icon).spoken(&spoken)).on_hover_text(t(tip)).clicked()
    };
    ui.horizontal_wrapped(|row| {
        if button(row, &t("s_wk_teilen"), "share-2", "s_wk_teilen_fuer", "s_wk_teilen_tip") {
            actions.push(Action::Share { key: key.to_string() });
        }
        if button(row, &t("s_wk_einfuegen"), "clipboard-paste", "s_wk_einfuegen_fuer", "s_wk_einfuegen_tip") {
            let open = state.pasting.as_ref().is_some_and(|p| p.key == key);
            state.pasting = (!open).then(|| Pasting { key: key.to_string(), text: String::new() });
        }
        if button(row, &t("s_wk_datei_speichern"), "save", "s_wk_datei_speichern_fuer", "s_wk_datei_speichern_tip") {
            actions.push(Action::SaveFile { key: key.to_string() });
        }
        if button(row, &t("s_wk_datei_oeffnen"), "folder-open", "s_wk_datei_oeffnen_fuer", "s_wk_datei_oeffnen_tip") {
            actions.push(Action::OpenFile { key: key.to_string() });
        }
        if ctx.others.is_some()
            && button(row, &t("s_wk_uebernehmen"), "copy", "s_wk_uebernehmen_fuer", "s_wk_uebernehmen_tip")
        {
            let open = state.borrowing.as_deref() == Some(key);
            state.borrowing = (!open).then(|| key.to_string());
        }
    });
    borrow_panel(ui, scheme, ctx, state, entry, key, actions);
    if let Some((_, said, bad)) = state.said.as_ref().filter(|(k, _, _)| k == key) {
        let tone = if *bad { scheme.red_text() } else { scheme.accent };
        ui.add(egui::Label::new(RichText::new(said).small().color(color(tone))).wrap());
    }
    paste_panel(ui, scheme, ctx, state, entry, key, actions);
}

/// The cart: the parts, those fitted, the sum and the route.
fn cart_part(
    ui: &mut Ui,
    scheme: &Scheme,
    ctx: &Ctx,
    state: &mut EquipmentState,
    key: &str,
    lines: &[Line],
    actions: &mut Vec<Action>,
) {
    let (done, open): (Vec<Line>, Vec<Line>) = lines.iter().cloned().partition(|l| l.done);
    let title = tf("s_wk_posten", &[("n", Arg::from(open.len()))]);
    let open_all = state.open_all;
    fold(&title, "equipment-cart", key, open_all).show(ui, |inner| {
        for item in cart::group_items(&open) {
            cart_item(inner, scheme, &item, actions);
        }
        if !done.is_empty() {
            let installed = tf("s_wk_eingebaut_n", &[("n", Arg::from(done.len()))]);
            fold(&installed, "equipment-done", key, open_all).show(inner, |rows| {
                for item in cart::group_items(&done) {
                    cart_item(rows, scheme, &item, actions);
                }
            });
        }
        if !open.is_empty() {
            total_block(inner, scheme, &cart::total(&open));
            if !state.order_loaded {
                if let Some(paths) = ctx.paths {
                    state.order = saved_order(paths);
                }
                state.order_loaded = true;
            }
            route_block(inner, scheme, &mut state.order, ctx.paths, &open, ctx.sources, None);
        }
    });
}

/// Draws the box of one ship and returns what was asked of it.
pub(crate) fn show(
    ui: &mut Ui,
    scheme: &Scheme,
    ctx: &Ctx,
    state: &mut EquipmentState,
    entry: &Map,
    key: &str,
    wishlist: bool,
) -> Option<Action> {
    let mut actions: Vec<Action> = Vec::new();
    let chosen: Map = match entry.iter().find(|(k, _)| k == "belegung") {
        Some((_, Value::Object(plan))) => plan.clone(),
        _ => Map::new(),
    };
    fold(&t("s_wk_titel"), "equipment", key, state.open_all).show(ui, |inner| {
        let slots = hardpoints(&ctx.stores.erkul.ships, &ctx.stores.erkul.parts, ship_id(entry), &chosen);
        if slots.is_empty() {
            // No data is not "nothing to get".
            inner.add(egui::Label::new(sub(scheme, t("s_wk_keine_daten"))).wrap());
            return;
        }
        let all = equipment::read_all(entry, &[]);
        loadout_bar(inner, scheme, state, &all, key, &mut actions);
        share_bar(inner, scheme, ctx, state, entry, key, &mut actions);
        inner.add_space(4.0);
        let grouped = groups(slots, &chosen);
        slot_list(inner, scheme, ctx, state, key, &grouped, all.active(), &mut actions);
        let (situation, lines) = lines_of(entry, wishlist, ctx.sources);
        state.missing.extend(cart::missing_prices(&lines));
        inner.add_space(6.0);
        match situation {
            cart::State::NothingOpen => {
                // Everything planned is ticked off: not "unchanged" but a warning about the next claim.
                let fitted = !chosen.is_empty() && lines.is_empty() && chosen_all_done(&chosen);
                if fitted {
                    inner.add(
                        egui::Label::new(RichText::new(t("s_hg_fertig_hilfe")).small().color(color(scheme.gold)))
                            .wrap(),
                    );
                } else {
                    inner.add(egui::Label::new(sub(scheme, t("s_wk_nichts_offen"))).wrap());
                }
            }
            _ => cart_part(inner, scheme, ctx, state, key, &lines, &mut actions),
        }
    });
    if actions.len() > 1 {
        // One click, one action; the rest would only repeat it.
        actions.truncate(1);
    }
    actions.pop()
}

/// `cart.fully_fitted` on the plan: something planned and all of it ticked off.
fn chosen_all_done(plan: &Map) -> bool {
    !plan.is_empty()
        && plan.iter().all(|(_, part)| match part {
            Value::Object(p) => field_true(p, "erledigt"),
            _ => true,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(json: &str) -> Map {
        match versekit_core::jsonio::loads(json) {
            Ok(Value::Object(map)) => map,
            other => panic!("{other:?}"),
        }
    }

    fn slot(path: &str, kind: &str, size: i64, factory: &str) -> SlotInfo {
        SlotInfo {
            path: path.into(),
            kind: kind.into(),
            size: Some(size),
            fit: None,
            factory: (!factory.is_empty()).then(|| (factory.to_string(), factory.to_string())),
            fixed: false,
        }
    }

    #[test]
    fn a_fixed_slot_is_a_line_of_its_own_beside_alike_slots_the_player_fits() {
        let mut fixed = slot("c", "MissileLauncher", 3, "wing");
        fixed.fixed = true;
        let slots = vec![slot("a", "MissileLauncher", 3, "rack"), slot("b", "MissileLauncher", 3, "rack"), fixed];
        let lines = groups(slots, &Map::new());
        assert_eq!(lines.iter().map(|g| g.paths.clone()).collect::<Vec<_>>(), [vec!["a", "b"], vec!["c"]]);
        assert!(!lines[0].slot.fixed && lines[1].slot.fixed);
    }

    #[test]
    fn slots_that_stand_alike_are_one_line_that_says_when_they_hold_different_parts() {
        let _language = crate::language_lock();
        versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
        let slots = vec![
            slot("a", "Cooler", 1, "f"),
            slot("b", "Cooler", 1, "f"),
            slot("c", "Cooler", 1, "f"),
            slot("d", "Shield", 2, ""),
        ];
        let plain = groups(slots.clone(), &Map::new());
        assert_eq!(plain.iter().map(|g| g.paths.len()).collect::<Vec<_>>(), [3, 1]);
        assert_eq!(group_title(&plain[0]), "3 × Cooler S1");
        assert_eq!(group_title(&plain[1]), "Shield S2");
        assert_eq!(plain[0].common(), Some(&Fill::Stock));
        // The same part in all three: still one line, with that part.
        let same = entry(r#"{"a": {"ref": "o", "name": "Other"}, "b": {"ref": "o", "name": "Other"}, "c": {"ref": "o", "name": "Other"}}"#);
        assert_eq!(groups(slots.clone(), &same)[0].own, Some(("o".to_string(), "Other".to_string())));
        // Another part in one and one left empty: the line stays whole and says what is in it.
        let chosen = entry(r#"{"b": {"ref": "other", "name": "Other"}, "c": {"ref": "", "name": "", "leer": true}}"#);
        let mixed = groups(slots, &chosen);
        assert_eq!(mixed.iter().map(|g| g.paths.clone()).collect::<Vec<_>>(), [vec!["a", "b", "c"], vec!["d"]]);
        assert_eq!(mixed[0].common(), None);
        assert_eq!(mixed[0].own, None);
        assert_eq!(mixed[0].fills, [Fill::Stock, Fill::Own("other".into(), "Other".into()), Fill::Empty]);
        let liste = format!("1 × f, 1 × Other, 1 × {}", t("s_wk_leer"));
        assert_eq!(mixed_text(&mixed[0]), tf("s_wk_gemischt", &[("liste", Arg::from(liste))]));
        // Opened, each slot is a line of its own with what it holds.
        let second = mixed[0].single(1);
        assert_eq!((second.paths.clone(), second.own.clone()), (vec!["b".to_string()], Some(("other".into(), "Other".into()))));
        assert_eq!(mixed[0].single(2).common(), Some(&Fill::Empty));
    }

    #[test]
    fn the_guns_of_each_turret_are_a_line_of_their_own_named_after_the_turret() {
        // The weapons of the F7C-M Super Hornet Mk II as erkul gives them: two S3 guns in the nose turret, two
        // S4 guns on the wings without a turret, and two S3 guns in the gimbal mounts of the ball turret.
        let mut nose = slot("hardpoint_weapon_nose", "Turret", 3, "nose");
        nose.factory = Some(("nose".into(), "Anvil Hornet Mk II S3 Nose Turret".into()));
        let mut ball = slot("hardpoint_weapon_center", "Turret", 5, "ball");
        ball.factory = Some(("ball".into(), "Anvil F7C-M Mk II Ball Turret".into()));
        let slots = vec![
            nose,
            slot("hardpoint_weapon_nose/hardpoint_weapon_S1_left", "WeaponGun", 3, "cf337"),
            slot("hardpoint_weapon_nose/hardpoint_weapon_S1_right", "WeaponGun", 3, "cf337"),
            slot("hardpoint_weapon_left_wing/hardpoint_class_2", "WeaponGun", 4, "revenant"),
            slot("hardpoint_weapon_right_wing/hardpoint_class_2", "WeaponGun", 4, "revenant"),
            ball,
            slot("hardpoint_weapon_center/hardpoint_weapon_left", "Turret", 3, "varipuck"),
            slot("hardpoint_weapon_center/hardpoint_weapon_left/hardpoint_class_2", "WeaponGun", 3, "cf337"),
            slot("hardpoint_weapon_center/hardpoint_weapon_right", "Turret", 3, "varipuck"),
            slot("hardpoint_weapon_center/hardpoint_weapon_right/hardpoint_class_2", "WeaponGun", 3, "cf337"),
        ];
        let titles = |chosen: &Map| groups(slots.clone(), chosen).iter().map(group_title).collect::<Vec<_>>();
        assert_eq!(
            titles(&Map::new()),
            [
                "Turret S3",
                "Anvil Hornet Mk II S3 Nose Turret: 2 × WeaponGun S3",
                "2 × WeaponGun S4",
                "Turret S5",
                "Anvil F7C-M Mk II Ball Turret: 2 × Turret S3",
                "Anvil F7C-M Mk II Ball Turret: 2 × WeaponGun S3",
            ]
        );
        // A turret of the player's choosing names its guns.
        let chosen = entry(r#"{"hardpoint_weapon_center": {"ref": "tmsb", "name": "TMSB-5 Ball Turret"}}"#);
        assert!(titles(&chosen).contains(&"TMSB-5 Ball Turret: 2 × WeaponGun S3".to_string()), "{:?}", titles(&chosen));
        // Each gun of a turret can be fitted on its own: another part in one leaves the line whole.
        let one = entry(r#"{"hardpoint_weapon_nose/hardpoint_weapon_S1_left": {"ref": "rev", "name": "Revenant"}}"#);
        let nose_guns = &groups(slots.clone(), &one)[1];
        assert_eq!(nose_guns.paths.len(), 2);
        assert_eq!(nose_guns.common(), None, "the two nose guns hold different parts");
    }

    #[test]
    fn heads_modules_ore_pods_and_remote_turrets_stand_by_their_mount() {
        let _language = crate::language_lock();
        versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
        let named = |path: &str, kind: &str, size: i64, part: &str| {
            let mut s = slot(path, kind, size, part);
            s.factory = Some((part.to_lowercase().replace(' ', "-"), part.to_string()));
            s
        };
        // As erkul gives them: the Mole with three heads in their cabs and eight ore pods, the Vulture with two salvage
        // heads of two modules each, the Reclaimer with remote turrets that are no slots themselves.
        let mut slots = Vec::new();
        for cab in ["front", "left", "right"] {
            slots.push(named(&format!("hardpoint_mining_cab_{cab}/hardpoint_weapon_mining"), "WeaponMining", 2, "Arbor MH2"));
        }
        for pod in 1..=8 {
            slots.push(named(&format!("hardpoint_mining_pod_{pod}"), "Container", 1, "Argo Ore Pod"));
        }
        for (arm, second) in [("left", "ReadyGrip Tractor Module"), ("right", "Abrade Scraper Module")] {
            let head = format!("hardpoint_salvage_arm_{arm}/hardpoint_salvage_laser");
            slots.push(named(&head, "SalvageHead", 2, "Baler Salvage Head"));
            slots.push(named(&format!("{head}/hardpoint_salvage_subItem01"), "SalvageModifier", 1, "Cinch Scraper Module"));
            slots.push(named(&format!("{head}/hardpoint_salvage_subItem02"), "SalvageModifier", 1, second));
        }
        for turret in ["top", "bottom"] {
            for side in ["left", "right"] {
                slots.push(named(&format!("hardpoint_remote_turret_{turret}/turret_{side}/hardpoint_class_2"), "WeaponGun", 3, "CF-337"));
            }
        }
        let grouped = groups(slots, &Map::new());
        let titles: Vec<String> = grouped.iter().map(group_title).collect();
        assert_eq!(
            titles,
            [
                "3 × WeaponMining S2",
                "8 × Container S1",
                "2 × SalvageHead S2",
                "Baler Salvage Head 1: 2 × SalvageModifier S1",
                "Baler Salvage Head 2: 2 × SalvageModifier S1",
                "remote turret top: 2 × WeaponGun S3",
                "remote turret bottom: 2 × WeaponGun S3",
            ]
        );
        // The three heads of the Mole: one line, each head on its own line when opened.
        assert_eq!(grouped[0].paths.len(), 3);
        assert_eq!(grouped[0].single(1).paths, ["hardpoint_mining_cab_left/hardpoint_weapon_mining"]);
        // The two modules of a head come from the factory with different parts: one line that says so.
        assert_eq!(grouped[3].common(), None);
        let liste = "1 × Cinch Scraper Module, 1 × ReadyGrip Tractor Module";
        assert_eq!(mixed_text(&grouped[3]), tf("s_wk_gemischt", &[("liste", Arg::from(liste))]));
        // Opened, each module has its own stock part.
        assert_eq!(grouped[3].single(1).factory.map(|(_, n)| n).as_deref(), Some("ReadyGrip Tractor Module"));
    }

    #[test]
    fn the_actions_change_the_entry_for_every_slot_of_the_group() {
        let mut ship = entry(r#"{"name": "Cutlass", "belegung": {}}"#);
        let paths = vec!["a".to_string(), "b".to_string()];
        let set = Action::SetPart { paths: paths.clone(), reference: "r".into(), name: "R".into() };
        assert!(apply(&mut ship, &set));
        assert!(!apply(&mut ship, &set), "the same again changes nothing");
        assert!(apply(&mut ship, &Action::Method { paths: paths.clone(), way: cart::CRAFT.into() }));
        assert!(apply(&mut ship, &Action::Tick { paths: paths.clone(), on: true }));
        assert!(cart::is_done(&ship, "a") && cart::is_done(&ship, "b"));
        assert!(apply(&mut ship, &Action::Clear { paths: vec!["a".to_string()] }));
        assert!(!cart::is_done(&ship, "a") && cart::is_done(&ship, "b"));
    }

    /// A data folder of its own with the two stored erkul files for one ship.
    fn folder(name: &str) -> Paths {
        folder_with(name, "")
    }

    /// The slots of a nose turret with a long name and two guns that hold different parts from the factory,
    /// and a port without a size: for the check that nothing sticks out.
    const TURRET_SLOTS: &str = r#",
        {"pfad": "hardpoint_weapon_nose", "art": "Turret", "groesse": 3, "werk": {"ref": "nose", "name": "Anvil Hornet Mk II S3 Nose Turret"}},
        {"pfad": "hardpoint_weapon_nose/hardpoint_weapon_S1_left", "art": "WeaponGun", "groesse": 3, "werk": {"ref": "cf337", "name": "CF-337 Panther Repeater"}},
        {"pfad": "hardpoint_weapon_nose/hardpoint_weapon_S1_right", "art": "WeaponGun", "groesse": 3, "werk": {"ref": "rev", "name": "Revenant Gatling With A Long Name"}},
        {"pfad": "hardpoint_battery", "art": "Battery", "groesse": null}"#;

    /// [`folder`] with more slots on the test ship, written as the stored slots spell them after a comma.
    fn folder_with(name: &str, more: &str) -> Paths {
        use versekit_core::paths::Host;
        use versekit_core::pypath::Flavor;
        let dir =
            std::env::temp_dir().join(format!("versekit-ausstattung-{}-{name}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        let text = dir.to_string_lossy().into_owned();
        let paths = Paths::new(Host::new(
            versekit_core::pypath::Flavor::native(),
            &[(versekit_core::legacy::ENV_HOME, &text)],
            &text,
        ));
        let _ = Flavor::native();
        let text = r#"{"format": 4, "spielversion": "4.0", "hersteller": {}, "schiffe": {"testship": {"id": "test_ship", "name": "Test Ship",
                "plaetze": [], "slots": [
                {"pfad": "c1", "art": "Cooler", "groesse": 1, "werk": {"ref": "f-cooler", "name": "Factory Cooler"}},
                {"pfad": "c2", "art": "Cooler", "groesse": 1, "werk": {"ref": "f-cooler", "name": "Factory Cooler"}},
                {"pfad": "c3", "art": "Cooler", "groesse": 1, "werk": {"ref": "f-cooler", "name": "Factory Cooler"}},
                {"pfad": "s1", "art": "Shield", "groesse": 2}MORE]}}}"#
            .replace("MORE", more);
        std::fs::write(paths.app_file(versekit_core::ships::ERKUL_FILE), text).expect("erkul file");
        paths
    }

    /// Shops, one recipe and raw prices made up.
    struct Fake;

    impl Sources for Fake {
        fn shop_known(&self, reference: &str) -> bool {
            matches!(reference, "new-cooler" | "new-shield")
        }
        fn cheapest(&self, reference: &str) -> Option<(f64, String, String)> {
            (reference == "new-cooler").then(|| (22_700.0, "Dumper's Depot".to_string(), "Area18".to_string()))
        }
        fn shops_for(&self, reference: &str) -> Vec<cart::Offer> {
            (reference == "new-cooler")
                .then(|| cart::Offer {
                    shop: "Dumper's Depot".into(),
                    place: "Area18".into(),
                    system: "Stanton".into(),
                    price: 22_700.0,
                })
                .into_iter()
                .collect()
        }
        fn raw_buy_price(&self, material: &str) -> f64 {
            if material == "Iron" { 10.0 } else { 0.0 }
        }
        fn recipe_for(&self, reference: &str, _name: &str) -> Option<cart::Recipe> {
            (reference == "new-shield").then(|| cart::Recipe {
                blueprint: "New Shield".into(),
                steps: vec![cart::Step {
                    seconds: 300,
                    ingredients: vec![cart::Ingredient { material: "Iron".into(), amount: 2.0 }],
                }],
            })
        }
        fn ship_offers(&self, _name: &str) -> Vec<cart::Offer> {
            Vec::new()
        }
        fn slots(&self, ship: &Map, _chosen: &Map) -> Vec<cart::Slot> {
            if field_text(ship, "name") != "Test Ship" {
                return Vec::new();
            }
            let slot = |path: &str, kind: &str, size: i64, factory: &str| cart::Slot {
                path: path.into(),
                kind: kind.into(),
                size: Some(size),
                factory_ref: factory.into(),
                factory_name: String::new(),
            };
            vec![
                slot("c1", "Cooler", 1, "f-cooler"),
                slot("c2", "Cooler", 1, "f-cooler"),
                slot("c3", "Cooler", 1, "f-cooler"),
                slot("s1", "Shield", 2, ""),
            ]
        }
    }

    /// One frame of the box, with its folds open.
    fn drawn(stores: &Stores, ship: &Map, state: &mut EquipmentState) -> Vec<String> {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        // The folds open at once, not over a time the test does not give them.
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let scheme = versekit_core::theme::current();
        let extra = Extra::default();
        let mut shown = Vec::new();
        for _ in 0..3 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 2000.0))),
                    ..Default::default()
                },
                |ui| {
                    state.open_all = true;
                    let box_ctx = Ctx { stores, extra: &extra, sources: &Fake, paths: None, others: None };
                    show(ui, scheme, &box_ctx, state, ship, "key", false);
                },
            );
            output.textures_delta.clear();
            fn collect(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
                match shape {
                    egui::epaint::Shape::Text(text) => out.push(text.galley.text().to_string()),
                    egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, out)),
                    _ => {}
                }
            }
            shown.clear();
            output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut shown));
        }
        shown
    }

    #[test]
    fn the_box_says_no_data_where_there_are_no_slots_and_shows_slots_and_cart_where_there_are() {
        let _language = crate::language_lock();
        versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
        let ship = entry(
            r#"{"name": "Test Ship", "hersteller": "", "kurz": "test_ship", "belegung": {
                "c1": {"ref": "new-cooler", "name": "New Cooler", "weg": "kaufen"},
                "s1": {"ref": "new-shield", "name": "New Shield", "weg": "bauen"}}}"#,
        );
        // No stored slots: one sentence, no list.
        let mut state = EquipmentState::default();
        let shown = drawn(&Stores::empty(), &ship, &mut state);
        assert!(shown.contains(&t("s_wk_keine_daten")), "{shown:?}");
        assert!(!shown.contains(&t("s_wk_auslegung")), "{shown:?}");
        // With the stored slots: the group of the three coolers with what is in them, opened the own choice and the
        // factory part, the cart.
        let paths = folder("box");
        let stores = Stores::load(&paths);
        let mut state = EquipmentState::default();
        let closed = drawn(&stores, &ship, &mut state);
        let mixed = tf("s_wk_gemischt", &[("liste", Arg::from("1 × New Cooler, 2 × Factory Cooler"))]);
        assert!(closed.contains(&"3 × Cooler S1".to_string()) && closed.contains(&mixed), "{closed:?}");
        assert!(!closed.contains(&tf("s_wk_platz_n", &[("n", Arg::from("1"))])), "shut at the start: {closed:?}");
        state.open_groups.insert("equipment-pick-key-c1".into());
        let shown = drawn(&stores, &ship, &mut state);
        assert!(shown.contains(&t("s_wk_auslegung")), "{shown:?}");
        assert!(shown.contains(&"3 × Cooler S1".to_string()), "c1 to c3 stand in one mount: {shown:?}");
        for n in ["1", "2", "3"] {
            assert!(shown.contains(&tf("s_wk_platz_n", &[("n", Arg::from(n))])), "slot {n} has a line: {shown:?}");
        }
        let stock = tf("s_wk_ab_werk", &[("name", Arg::from("Factory Cooler"))]);
        assert!(shown.contains(&stock) && shown.contains(&"New Cooler".to_string()), "{shown:?}");
        assert!(shown.contains(&"Shield S2".to_string()) && shown.contains(&"New Shield".to_string()), "{shown:?}");
        assert!(shown.contains(&tf("s_wk_posten", &[("n", Arg::Int(2))])), "{shown:?}");
        // The cooler is bought, the shield built: both ways stand with their numbers.
        let bought = tf(
            "s_wk_kauf_preis",
            &[
                ("preis", Arg::from(plain(22_700.0))),
                ("laden", Arg::from("Dumper's Depot · Area18")),
                ("ort", Arg::from("")),
            ],
        );
        assert!(shown.contains(&bought.trim_end_matches([' ', '·']).to_string()), "{shown:?}");
        let built = tf("s_wk_bau_kosten", &[("preis", Arg::from(plain(20.0))), ("dauer", Arg::from(duration(300)))]);
        assert!(shown.contains(&built), "{shown:?}");
        assert!(shown.contains(&t("s_wk_kein_rezept")), "the cooler has no blueprint: {shown:?}");
        let _ = std::fs::remove_dir_all(
            std::env::temp_dir().join(format!("versekit-ausstattung-{}-box", versekit_core::testdir::run_tag())),
        );
    }

    #[test]
    fn the_box_sticks_out_nowhere_and_no_text_lies_on_another_in_both_languages() {
        use versekit_core::language::{Choice as Lang, set_thread_language};
        fn texts(shape: &egui::epaint::Shape, out: &mut Vec<(String, egui::Rect)>) {
            match shape {
                // Row by row: the box of a text of several lines also holds the room beside its last line.
                egui::epaint::Shape::Text(text) => {
                    for placed in &text.galley.rows {
                        let line = placed.row.text().to_string();
                        if !line.trim().is_empty() {
                            out.push((line, placed.rect_without_leading_space().translate(text.pos.to_vec2())));
                        }
                    }
                }
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| texts(shape, out)),
                _ => {}
            }
        }
        let _language = crate::language_lock();
        let paths = folder_with("rand", TURRET_SLOTS);
        let stores = Stores::load(&paths);
        // Case 3 has the list of the loadouts of another ship open, with a long line.
        let offered = vec![("Geplant: Test Ship, Ausstattung „A Very Long Loadout Name For The Check“".to_string(), Equipment::default())];
        let others: &OthersOf<'_> = &|_: &Map| offered.clone();
        let ship = entry(
            r#"{"name": "Test Ship", "hersteller": "", "kurz": "test_ship", "belegung": {
                "c1": {"ref": "new-cooler", "name": "Hurston Dynamics Predator Cooler With A Very Long Name", "weg": "kaufen"},
                "s1": {"ref": "new-shield", "name": "New Shield", "weg": "bauen"}},
                "ausstattungen": [{"name": "Standard", "active": true, "slots": [
                    {"portPath": "c1", "itemGuid": "new-cooler", "acquisition": "pledge"},
                    {"portPath": "s1", "itemGuid": "new-shield", "acquisition": "crafted", "quality": {"Frame": 820}}]},
                    {"name": "A Second Loadout With A Long Name", "active": false, "slots": []}]}"#,
        );
        // The built shield has a recipe: its blueprint line and a slider per recipe slot are drawn.
        let recipes = vec![RecipeInfo {
            entity: "new-shield".into(),
            basis: "New Shield Of A Very Long Blueprint Name".into(),
            blueprint: "bp-shield".into(),
            slots: vec!["Frame".into(), "Emitter Housing".into()],
        }];
        let scheme = versekit_core::theme::current();
        let mut findings = Vec::new();
        for lang in [Lang::De, Lang::En] {
            set_thread_language(lang);
            for points in versekit_core::font_size::POINTS_RANGE.0..=versekit_core::font_size::POINTS_RANGE.1 {
                for width in [380.0_f32, 520.0, 900.0] {
                    // 0: as it is, 1: the list of the coolers open, 2: renaming to a name another one has, 3: a code
                    // pasted with its preview, the list of another ship's loadouts open, a sentence said and every
                    // group opened.
                    for mode in 0..4 {
                        let picking = mode == 1;
                        let ctx = egui::Context::default();
                        crate::install(&ctx);
                        crate::apply_size(&ctx, points);
                        ctx.all_styles_mut(|style| style.animation_time = 0.0);
                        let extra = Extra { recipes: recipes.clone(), ..Extra::default() };
                        let mut state = EquipmentState { open_all: true, ..EquipmentState::default() };
                        // The coolers opened into a line each: the own part with its details stands there.
                        state.open_groups.insert("equipment-pick-key-c1".into());
                        if mode == 2 {
                            state.renaming = Some(Renaming {
                                key: "key".into(),
                                old: "Standard".into(),
                                text: "A Second Loadout With A Long Name".into(),
                                fresh: false,
                                problem: Some("s_wk_ausst_doppelt"),
                            });
                        }
                        if mode == 3 {
                            state.pasting = Some(Pasting {
                                key: "key".into(),
                                text: format!("{}\nVK1-kaputt\nnur ein Gruß", sent_code("Test Ship")),
                            });
                            state.borrowing = Some("key".into());
                            state.said = Some(("key".into(), "Gespeichert: Eine-sehr-lange-Datei-mit-Namen.txt".into(), false));
                            state.open_groups.insert("equipment-pick-key-hardpoint_weapon_nose/hardpoint_weapon_S1_left".into());
                            ctx.enable_accesskit();
                        }
                        // The right edges of the buttons back to the stock part, line by line.
                        let mut edges: Vec<f64> = Vec::new();
                        if picking {
                            // The list of the two coolers alike holds a long part; a click opens it.
                            let long = Choice {
                                name: "Very Long Cooler Name Mk II".into(),
                                reference: "r".into(),
                                maker: String::new(),
                                grade: "B".into(),
                                class: "Stealth".into(),
                                origin: PartOrigin::Craftable,
                                size: Some(1),
                            };
                            preset_coolers(&mut state, &stores, &extra, vec![long]);
                            ctx.enable_accesskit();
                        }
                        let (mut over, mut lines, mut field) = (0.0_f32, Vec::new(), None);
                        // Where the rows of the open list stand.
                        let mut list: Vec<(egui::Rect, String)> = Vec::new();
                        for frame in 0..5 {
                            let events = match (frame, field) {
                                (1 | 2, Some(pos)) => vec![egui::Event::PointerButton {
                                    pos,
                                    button: egui::PointerButton::Primary,
                                    pressed: frame == 1,
                                    modifiers: egui::Modifiers::NONE,
                                }],
                                _ => Vec::new(),
                            };
                            let mut output = ctx.run_ui(
                                egui::RawInput {
                                    screen_rect: Some(egui::Rect::from_min_size(
                                        egui::Pos2::ZERO,
                                        egui::vec2(width, 3000.0),
                                    )),
                                    events,
                                    ..Default::default()
                                },
                                |ui| {
                                    let others = (mode == 3).then_some(others);
                                    let box_ctx = Ctx { stores: &stores, extra: &extra, sources: &Fake, paths: None, others };
                                    show(ui, scheme, &box_ctx, &mut state, &ship, "key", false);
                                    over = (ui.min_rect().right() - ui.ctx().content_rect().right()).max(0.0);
                                },
                            );
                            output.textures_delta.clear();
                            if mode == 3
                                && let Some(update) = &output.platform_output.accesskit_update
                            {
                                let backs = [t("s_wk_auf_werk"), t("s_wk_nichts_zurueck"), t("s_wk_nichts_zurueck_leer")];
                                edges = update
                                    .nodes
                                    .iter()
                                    .filter(|(_, n)| n.role() == egui::accesskit::Role::Button)
                                    .filter(|(_, n)| n.label().is_some_and(|l| backs.iter().any(|b| l.ends_with(b.as_str()))))
                                    .filter_map(|(_, n)| n.bounds().map(|b| b.x1))
                                    .collect();
                            }
                            if frame == 0 && picking {
                                field = output.platform_output.accesskit_update.as_ref().and_then(|update| {
                                    update
                                        .nodes
                                        .iter()
                                        .find(|(_, n)| {
                                            n.role() == egui::accesskit::Role::ComboBox
                                                && n.label().is_some_and(|l| l.starts_with("3 × Cooler S1: "))
                                        })
                                        .and_then(|(_, n)| n.bounds())
                                        .map(|b| egui::pos2((b.x0 + 30.0) as f32, ((b.y0 + b.y1) / 2.0) as f32))
                                });
                                assert!(field.is_some(), "the field of the coolers is named");
                            }
                            lines.clear();
                            output.shapes.iter().for_each(|clipped| texts(&clipped.shape, &mut lines));
                            if let Some(update) = &output.platform_output.accesskit_update {
                                list = update
                                    .nodes
                                    .iter()
                                    .filter(|(_, n)| n.role() == egui::accesskit::Role::ListBoxOption)
                                    .filter_map(|(_, n)| {
                                        let b = n.bounds()?;
                                        let rect = egui::Rect::from_min_max(
                                            egui::pos2(b.x0 as f32, b.y0 as f32),
                                            egui::pos2(b.x1 as f32, b.y1 as f32),
                                        );
                                        Some((rect, n.label().unwrap_or_default().to_string()))
                                    })
                                    .collect();
                            }
                        }
                        let at = format!("{lang:?} Größe {points} Breite {width} Fall {mode}");
                        let drawn = |want: &str| lines.iter().any(|(line, _)| line.contains(want));
                        if !drawn("Emitter Housing") || !drawn(&t("s_wk_herkunft")) {
                            findings.push(format!("{at}: Herkunft oder Regler fehlen"));
                        }
                        if mode == 2 && !drawn(&t("s_wk_ausst_doppelt")) {
                            findings.push(format!("{at}: der Satz zum doppelten Namen fehlt"));
                        }
                        if mode == 3 {
                            for (key, what) in [
                                ("s_wk_code_anlegen", "die Vorschau des Codes"),
                                ("s_wk_uebernehmen_knopf", "die Liste zum Übernehmen"),
                                ("s_wk_groesse_offen", "der Hinweis ohne Größe"),
                            ] {
                                if !drawn(&t(key)) {
                                    findings.push(format!("{at}: {what} fehlt"));
                                }
                            }
                            // Every button back to the stock part stands at the same place, with or without an arrow.
                            if edges.len() < 4 || edges.iter().any(|x| (x - edges[0]).abs() > 1.0) {
                                findings.push(format!("{at}: die Zurück-Knöpfe stehen nicht an einer Kante: {edges:?}"));
                            }
                        }
                        if picking && !lines.iter().any(|(line, _)| line.contains("Very Long Cooler")) {
                            findings.push(format!("{at}: die Liste ging nicht auf"));
                        }
                        if over > 0.5 {
                            findings.push(format!("{at}: {over:.0} zu breit"));
                        }
                        // The open list lies over the lines below it on a layer of its own: the lines of the page
                        // and the lines of the list are checked each among themselves.
                        // A line of the list is a line of a row; a line of the page under the list is hidden by it.
                        let under = |rect: &egui::Rect| list.iter().any(|(row, _)| row.contains(rect.center()));
                        let of_row = |(text, rect): &(String, egui::Rect)| {
                            list.iter().any(|(row, label)| row.contains(rect.center()) && label.contains(text.trim()))
                        };
                        let rows: Vec<_> = lines.iter().filter(|line| of_row(line)).cloned().collect();
                        let page: Vec<_> = lines.iter().filter(|(_, rect)| !under(rect)).cloned().collect();
                        for group in [page, rows] {
                            'pairs: for (i, (a, ra)) in group.iter().enumerate() {
                                for (b, rb) in &group[i + 1..] {
                                    let both = ra.intersect(*rb);
                                    if both.width() > 1.0 && both.height() > 1.0 {
                                        findings.push(format!("{at}: {a:?} liegt auf {b:?}"));
                                        break 'pairs;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        set_thread_language(Lang::De);
        let _ = std::fs::remove_dir_all(
            std::env::temp_dir().join(format!("versekit-ausstattung-{}-rand", versekit_core::testdir::run_tag())),
        );
        assert!(findings.is_empty(), "{}", findings.join("\n"));
    }

    /// Puts `parts` into the list of the two coolers alike (`c2`, `c3` of the test ship), the stock cooler at the
    /// top, as the box keeps a list it worked out.
    fn preset_coolers(state: &mut EquipmentState, stores: &Stores, extra: &Extra, parts: Vec<Choice>) {
        let mark = sources_mark(&Ctx { stores, extra, sources: &Fake, paths: None, others: None });
        let list = stock_first(parts, Some(&("f-cooler".into(), "Factory Cooler".into())));
        let entries = list
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let text = entry_text(c, &HashMap::new());
                if i == 0 { tf("s_wk_ab_werk", &[("name", Arg::from(text))]) } else { text }
            })
            .collect();
        let key = format!("equipment-pick-key-c1|Cooler|Some(1)|f-cooler|{:?}", versekit_core::language::current());
        state.lists.insert(key, (mark, std::rc::Rc::new((list, entries))));
    }

    fn choice(name: &str, reference: &str) -> Choice {
        Choice {
            name: name.into(),
            reference: reference.into(),
            maker: String::new(),
            grade: String::new(),
            class: String::new(),
            origin: PartOrigin::Buyable,
            size: None,
        }
    }

    #[test]
    fn the_parts_of_a_slot_go_by_size_and_the_stock_part_still_stands_first() {
        let names = |list: Vec<Choice>| list.into_iter().map(|c| c.name).collect::<Vec<_>>();
        let sized = |name: &str, size: Option<i64>| Choice { size, ..choice(name, &name.to_lowercase()) };
        // As the sources hand them over: by name.
        let found = vec![
            sized("Aaron Rack", None),
            sized("Omnisky III Cannon", Some(1)),
            sized("Omnisky IX Cannon", Some(3)),
            sized("Omnisky VI Cannon", Some(2)),
            sized("Omnisky XII Cannon", Some(4)),
        ];
        assert_eq!(
            names(in_size_order(found.clone())),
            ["Omnisky III Cannon", "Omnisky VI Cannon", "Omnisky IX Cannon", "Omnisky XII Cannon", "Aaron Rack"]
        );
        let factory = ("omnisky ix cannon".to_string(), "Omnisky IX Cannon".to_string());
        assert_eq!(
            names(stock_first(in_size_order(found), Some(&factory))),
            ["Omnisky IX Cannon", "Omnisky III Cannon", "Omnisky VI Cannon", "Omnisky XII Cannon", "Aaron Rack"]
        );
    }

    #[test]
    fn the_stock_part_stands_first_and_only_once() {
        let names = |list: Vec<Choice>| list.into_iter().map(|c| c.name).collect::<Vec<_>>();
        let factory = ("f-cooler".to_string(), "Factory Cooler".to_string());
        let found = vec![choice("Alpha", "a"), choice("Factory Cooler", "f-cooler"), choice("Zeta", "z")];
        assert_eq!(names(stock_first(found.clone(), Some(&factory))), ["Factory Cooler", "Alpha", "Zeta"]);
        // Found by its name too, in any case.
        let by_name = vec![choice("Alpha", "a"), choice("FACTORY COOLER", "other-ref")];
        assert_eq!(names(stock_first(by_name, Some(&factory))), ["FACTORY COOLER", "Alpha"]);
        // A stock part the sources do not know stands at the top all the same.
        let unknown = stock_first(vec![choice("Alpha", "a")], Some(&factory));
        assert_eq!((unknown[0].reference.as_str(), unknown[0].origin), ("f-cooler", PartOrigin::ShipOnly));
        assert_eq!(names(stock_first(found, None)), ["Alpha", "Factory Cooler", "Zeta"]);
    }

    /// Clicks the node of a screen reader whose role is `role` and whose name starts with `name`; what the box
    /// asked in those frames.
    fn click_node(
        ctx: &egui::Context,
        stores: &Stores,
        ship: &Map,
        state: &mut EquipmentState,
        role: egui::accesskit::Role,
        name: &str,
    ) -> Option<Action> {
        let extra = Extra::default();
        let scheme = versekit_core::theme::current();
        let mut asked = None;
        let mut at = None;
        for frame in 0..4 {
            let events = match (frame, at) {
                (1 | 2, Some(pos)) => vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: frame == 1,
                    modifiers: egui::Modifiers::NONE,
                }],
                _ => Vec::new(),
            };
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 2000.0))),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                state.open_all = true;
                let box_ctx = Ctx { stores, extra: &extra, sources: &Fake, paths: None, others: None };
                if let Some(action) = show(ui, scheme, &box_ctx, state, ship, "key", false) {
                    asked.get_or_insert(action);
                }
            });
            output.textures_delta.clear();
            if frame == 0 {
                let update = output.platform_output.accesskit_update.expect("accesskit is on");
                let bounds = update
                    .nodes
                    .iter()
                    .find(|(_, n)| n.role() == role && n.label().is_some_and(|l| l.starts_with(name)))
                    .and_then(|(_, n)| n.bounds())
                    .unwrap_or_else(|| {
                        let there: Vec<&str> =
                            update.nodes.iter().filter(|(_, n)| n.role() == role).filter_map(|(_, n)| n.label()).collect();
                        panic!("{name} is not there, only {there:?}")
                    });
                at = Some(egui::pos2((bounds.x0 + 30.0) as f32, ((bounds.y0 + bounds.y1) / 2.0) as f32));
            }
        }
        asked
    }

    /// Every ship of the stored erkul slots: each slot stands in exactly one line of the box and can be reached
    /// on its own (a line of one slot, or its own line in the opened group), no slot is swallowed and none comes
    /// twice; the lines never mix two mounts. Where erkul's parts know the slots of a part fitted at the factory
    /// that the slots of the ship lack, that is a gap of the data, and it is named. On real data:
    /// `VERSEKIT_MESSORDNER=<data folder> cargo test --release -p versekit-ui jeder_platz_aller_schiffe --
    /// --ignored --nocapture`.
    #[test]
    #[ignore]
    fn jeder_platz_aller_schiffe_einzeln_erreichbar() {
        use versekit_core::erkul_slots::{ErkulParts, GAME_PARTS_FILE, GAME_SHIPS_FILE, hardpoints};
        use versekit_core::ships::{ErkulShips, ShipId};
        let Ok(folder) = std::env::var("VERSEKIT_MESSORDNER") else { return };
        let file = |name: &str| format!("{folder}/{name}");
        // As the program reads them: the slots of the game data, erkul's where those have nothing.
        let ships = ErkulShips::merged(ErkulShips::load(&file(GAME_SHIPS_FILE)), ErkulShips::load(&file("erkul-schiffe.json")));
        let parts = ErkulParts::merged(ErkulParts::load(&file(GAME_PARTS_FILE)), ErkulParts::load(&file("erkul-teile.json")));
        let listed = if std::path::Path::new(&file(GAME_SHIPS_FILE)).exists() { GAME_SHIPS_FILE } else { "erkul-schiffe.json" };
        println!("Schiffe aus {listed}");
        let text = std::fs::read_to_string(file(listed)).unwrap_or_default();
        let Ok(Value::Object(raw)) = versekit_core::jsonio::loads(&text)
        else {
            panic!("no erkul-schiffe.json in {folder}")
        };
        let Some(Value::Object(all)) = raw.iter().find(|(k, _)| k == "schiffe").map(|(_, v)| v.clone()) else { panic!("no ships") };
        let (mut gaps, mut count, mut slots_seen) = (Vec::new(), 0, 0);
        for (key, ship) in &all {
            let Some(ship) = ship.as_object() else { continue };
            let name = field_text(ship, "name").to_string();
            let slots = hardpoints(&ships, &parts, ShipId { name: &name, maker: "", short: key, maker_short: "" }, &Map::new());
            let grouped = groups(slots.clone(), &Map::new());
            let mut paths: Vec<String> = grouped.iter().flat_map(|g| g.paths.clone()).collect();
            let mut want: Vec<String> = slots.iter().map(|s| s.path.clone()).collect();
            paths.sort();
            want.sort();
            assert_eq!(paths, want, "{name}: every slot in exactly one line");
            for group in &grouped {
                for at in 0..group.paths.len() {
                    let one = group.single(at);
                    assert_eq!(one.paths, [group.paths[at].clone()], "{name}: each slot has a line of its own");
                }
                let mounts: Vec<Option<String>> = group
                    .paths
                    .iter()
                    .map(|p| slots.iter().filter(|s| p.strip_prefix(s.path.as_str()).is_some_and(|r| r.starts_with('/'))).min_by_key(|s| s.path.len()).map(|s| s.path.clone()))
                    .collect();
                assert!(mounts.iter().all(|m| *m == mounts[0]), "{name}: {} mixes mounts", group_title(group));
            }
            // A part fitted at the factory whose own slots erkul's parts know, and the ship's slots lack.
            for slot in &slots {
                let Some(part) = slot.factory.as_ref().and_then(|(r, _)| parts.get(r)) else { continue };
                for port in &part.ports {
                    let child = format!("{}/{}", slot.path, port.name);
                    if !slots.iter().any(|s| s.path.eq_ignore_ascii_case(&child)) {
                        gaps.push(format!("{name}: {child}"));
                    }
                }
            }
            count += 1;
            slots_seen += slots.len();
            if ["anvlhornetf7cmmk2", "argomole", "miscprospector", "drakgolem", "drakvulture", "aegsreclaimer", "drakcutlassblack"].contains(&key.as_str()) {
                println!("== {name}");
                for group in &grouped {
                    println!("   {}", group_title(group));
                }
            }
        }
        // Where erkul knows the ship too: which slots one has and the other lacks (no error: the game data hold).
        if listed == GAME_SHIPS_FILE {
            let only_erkul = ErkulShips::load(&file("erkul-schiffe.json"));
            let erkul_parts = ErkulParts::load(&file("erkul-teile.json"));
            // Only ships erkul keeps under the same key: its word-by-word search would name another ship.
            let erkul_text = std::fs::read_to_string(file("erkul-schiffe.json")).unwrap_or_default();
            let erkul_keys: Vec<String> = match versekit_core::jsonio::loads(&erkul_text) {
                Ok(Value::Object(root)) => match root.iter().find(|(k, _)| k == "schiffe").map(|(_, v)| v) {
                    Some(Value::Object(ships)) => ships.iter().map(|(k, _)| k.clone()).collect(),
                    _ => Vec::new(),
                },
                _ => Vec::new(),
            };
            let mut differ = Vec::new();
            for (key, ship) in &all {
                let Some(ship) = ship.as_object() else { continue };
                let name = field_text(ship, "name").to_string();
                let id = ShipId { name: &name, maker: "", short: key, maker_short: "" };
                if !erkul_keys.contains(key) {
                    continue;
                }
                let lower = |slots: Vec<versekit_core::erkul_slots::SlotInfo>| -> std::collections::BTreeSet<String> {
                    slots.into_iter().map(|s| s.path.to_lowercase()).collect()
                };
                let game = lower(hardpoints(&ships, &parts, id, &Map::new()));
                let theirs = lower(hardpoints(&only_erkul, &erkul_parts, id, &Map::new()));
                let (more, less) = (game.difference(&theirs).count(), theirs.difference(&game).count());
                if more + less > 0 {
                    differ.push(format!("{name} (+{more}/-{less})"));
                }
            }
            println!("Gegen erkul ({} Schiffe mit Unterschied): {}", differ.len(), differ.join(", "));
        }
        println!("{count} Schiffe, {slots_seen} Plätze, jeder einzeln erreichbar");
        println!("Lücken der Daten ({}): {}", gaps.len(), if gaps.is_empty() { "keine bekannt".to_string() } else { gaps.join("; ") });
        assert!(count > 0, "no ship read");
    }

    #[test]
    fn the_arrow_of_a_group_opens_a_line_for_each_slot_and_shuts_it_again() {
        let _language = crate::language_lock();
        versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
        use egui::accesskit::Role;
        let paths = folder("pfeil");
        let stores = Stores::load(&paths);
        let ship = entry(r#"{"name": "Test Ship", "hersteller": "", "kurz": "test_ship", "belegung": {}}"#);
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        ctx.enable_accesskit();
        let mut state = EquipmentState::default();
        let id = "equipment-pick-key-c1";
        // The arrow is named for the screen reader by its group and what it does.
        let open = format!("3 × Cooler S1: {}", t("s_wk_aufklappen"));
        assert_eq!(click_node(&ctx, &stores, &ship, &mut state, Role::Button, &open), None);
        assert!(state.open_groups.contains(id), "opened");
        let shown = drawn(&stores, &ship, &mut state);
        assert!(shown.contains(&tf("s_wk_platz_n", &[("n", Arg::from("3"))])), "{shown:?}");
        let shut = format!("3 × Cooler S1: {}", t("s_wk_zuklappen"));
        assert_eq!(click_node(&ctx, &stores, &ship, &mut state, Role::Button, &shut), None);
        assert!(!state.open_groups.contains(id), "shut again");
        // A slot alone has no arrow: there is nothing to open.
        let shield = format!("Shield S2: {}", t("s_wk_aufklappen"));
        assert!(!drawn_nodes(&ctx, &stores, &ship, &mut state).iter().any(|l| l.starts_with(&shield)));
        let _ = std::fs::remove_dir_all(
            std::env::temp_dir().join(format!("versekit-ausstattung-{}-pfeil", versekit_core::testdir::run_tag())),
        );
    }

    /// The names of all nodes of one frame of the box, for the screen reader.
    fn drawn_nodes(ctx: &egui::Context, stores: &Stores, ship: &Map, state: &mut EquipmentState) -> Vec<String> {
        let extra = Extra::default();
        let scheme = versekit_core::theme::current();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 2000.0))),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            state.open_all = true;
            let box_ctx = Ctx { stores, extra: &extra, sources: &Fake, paths: None, others: None };
            show(ui, scheme, &box_ctx, state, ship, "key", false);
        });
        output.textures_delta.clear();
        let update = output.platform_output.accesskit_update.expect("accesskit is on");
        update.nodes.iter().filter_map(|(_, n)| n.label().map(|l| l.to_string())).collect()
    }

    #[test]
    fn a_pick_in_the_field_of_a_slot_sets_the_part_and_the_stock_part_goes_back_to_it() {
        let _language = crate::language_lock();
        versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
        use egui::accesskit::Role;
        let paths = folder("wahl");
        let stores = Stores::load(&paths);
        let ship = entry(
            r#"{"name": "Test Ship", "hersteller": "", "kurz": "test_ship", "belegung": {
                "c1": {"ref": "new-cooler", "name": "New Cooler", "weg": "kaufen"}}}"#,
        );
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        ctx.enable_accesskit();
        let mut state = EquipmentState::default();
        // The three coolers of one mount: their list holds a part besides the stock one, and it goes into all three.
        preset_coolers(&mut state, &stores, &Extra::default(), vec![choice("Arctic", "arctic")]);
        assert_eq!(click_node(&ctx, &stores, &ship, &mut state, Role::ComboBox, "3 × Cooler S1"), None);
        let set = click_node(&ctx, &stores, &ship, &mut state, Role::ListBoxOption, "Arctic");
        assert_eq!(
            set,
            Some(Action::SetPart {
                paths: vec!["c1".into(), "c2".into(), "c3".into()],
                reference: "arctic".into(),
                name: "Arctic".into()
            })
        );
        // Opened, the slot with an own part has its own line: the stock part at the top of its list takes it back.
        // (A new window: the list of the line above is still open and would lie over the lines below.)
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        ctx.enable_accesskit();
        state.editing = None;
        state.open_groups.insert("equipment-pick-key-c1".into());
        // Its name for the screen reader says the group too: two open groups have no field of the same name.
        let first = format!("3 × Cooler S1, {}", tf("s_wk_platz_n", &[("n", Arg::from("1"))]));
        assert_eq!(click_node(&ctx, &stores, &ship, &mut state, Role::ComboBox, &format!("{first}:")), None);
        let stock = tf("s_wk_ab_werk", &[("name", Arg::from("Factory Cooler"))]);
        let back = click_node(&ctx, &stores, &ship, &mut state, Role::ListBoxOption, &stock);
        assert_eq!(back, Some(Action::Clear { paths: vec!["c1".into()] }));
        // A slot of the stock can be left empty, on its own.
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        ctx.enable_accesskit();
        state.editing = None;
        let second = format!("3 × Cooler S1, {}", tf("s_wk_platz_n", &[("n", Arg::from("2"))]));
        assert_eq!(click_node(&ctx, &stores, &ship, &mut state, Role::ComboBox, &format!("{second}:")), None);
        let empty = click_node(&ctx, &stores, &ship, &mut state, Role::ListBoxOption, &t("s_wk_leer_lassen"));
        assert_eq!(empty, Some(Action::Empty { paths: vec!["c2".into()] }));
        let _ = std::fs::remove_dir_all(
            std::env::temp_dir().join(format!("versekit-ausstattung-{}-wahl", versekit_core::testdir::run_tag())),
        );
    }

    #[test]
    fn a_fixed_slot_shows_its_factory_part_in_a_field_that_is_off_and_a_click_does_nothing() {
        let _language = crate::language_lock();
        versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
        use egui::accesskit::Role;
        let paths = folder_with(
            "fest",
            r#",
            {"pfad": "wing", "art": "MissileLauncher", "groesse": 3, "werk": {"ref": "wing-rack", "name": "MSD-322 Missile Rack"}, "fest": true}"#,
        );
        let stores = Stores::load(&paths);
        let ship = entry(r#"{"name": "Test Ship", "hersteller": "", "kurz": "test_ship"}"#);
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        ctx.enable_accesskit();
        let mut state = EquipmentState::default();
        let extra = Extra::default();
        let scheme = versekit_core::theme::current();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 2000.0))),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            state.open_all = true;
            let box_ctx = Ctx { stores: &stores, extra: &extra, sources: &Fake, paths: None, others: None };
            show(ui, scheme, &box_ctx, &mut state, &ship, "key", false);
        });
        output.textures_delta.clear();
        let update = output.platform_output.accesskit_update.expect("accesskit is on");
        let shown = tf("s_wk_fest_teil", &[("name", Arg::from("MSD-322 Missile Rack"))]);
        let field = update
            .nodes
            .iter()
            .find(|(_, n)| n.role() == Role::ComboBox && n.label().is_some_and(|l| l.starts_with("MissileLauncher S3:")))
            .map(|(_, n)| n.clone())
            .expect("the field of the fixed slot");
        assert!(field.label().is_some_and(|l| l.contains(&shown)), "{:?}", field.label());
        assert!(field.is_disabled(), "the field of a fixed slot is off");
        // A click on it chooses nothing.
        assert_eq!(click_node(&ctx, &stores, &ship, &mut state, Role::ComboBox, "MissileLauncher S3:"), None);
        assert!(state.editing.is_none(), "no list opened");
        let _ = std::fs::remove_dir_all(
            std::env::temp_dir().join(format!("versekit-ausstattung-{}-fest", versekit_core::testdir::run_tag())),
        );
    }

    #[test]
    fn grade_and_class_stand_after_the_name_so_that_the_search_finds_them() {
        let choice = Choice {
            name: "Bolt".into(),
            reference: "r".into(),
            maker: String::new(),
            grade: "B".into(),
            class: "Stealth".into(),
            origin: PartOrigin::Craftable,
            size: None,
        };
        let _language = crate::language_lock();
        versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
        let none = HashMap::new();
        assert_eq!(
            entry_text(&choice, &none),
            format!("Bolt  ·  B · {} · {}", t("s_ld_kl_stealth"), t("s_p9_hg_wegen_bauplan"))
        );
        // What the choice leaves out, the shop catalog adds.
        let bare = Choice { grade: String::new(), class: String::new(), origin: PartOrigin::Buyable, ..choice };
        let catalog: HashMap<&str, (&str, &str)> = [("r", ("C", "Military"))].into_iter().collect();
        assert_eq!(entry_text(&bare, &catalog), format!("Bolt  ·  C · {}", t("s_ld_kl_military")));
        assert_eq!(entry_text(&bare, &none), "Bolt");
    }

    const KNOWN: &str = "aaaaaaaa-0000-4000-8000-000000000001";
    const STRANGE: &str = "aaaaaaaa-0000-4000-8000-000000000002";

    /// Frames of the box with the folds open; the texts and the buttons (name, centre) of the last one, and the
    /// action of any frame.
    fn box_frames(
        stores: &Stores,
        extra: &Extra,
        ship: &Map,
        state: &mut EquipmentState,
        events: Vec<egui::Event>,
    ) -> (Vec<String>, Vec<(String, egui::Pos2)>, Option<Action>) {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let scheme = versekit_core::theme::current();
        let (mut texts, mut buttons, mut action) = (Vec::new(), Vec::new(), None);
        let mut events = Some(events);
        for round in 0..4 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 2400.0))),
                // The click comes once the box stands where it is drawn.
                events: if round == 2 { events.take().unwrap_or_default() } else { Vec::new() },
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                state.open_all = true;
                let box_ctx = Ctx { stores, extra, sources: &Fake, paths: None, others: None };
                if let Some(asked) = show(ui, scheme, &box_ctx, state, ship, "key", false) {
                    action = Some(asked);
                }
            });
            output.textures_delta.clear();
            fn collect(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
                match shape {
                    egui::epaint::Shape::Text(text) => out.push(text.galley.text().to_string()),
                    egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, out)),
                    _ => {}
                }
            }
            texts.clear();
            output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
            buttons = output
                .platform_output
                .accesskit_update
                .map(|update| {
                    update
                        .nodes
                        .iter()
                        .filter(|(_, n)| n.role() == egui::accesskit::Role::Button)
                        .filter_map(|(_, n)| {
                            let b = n.bounds()?;
                            Some((n.label()?.to_string(), egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32)))
                        })
                        .collect()
                })
                .unwrap_or_default();
        }
        (texts, buttons, action)
    }

    fn click_on(buttons: &[(String, egui::Pos2)], name: &str) -> Vec<egui::Event> {
        let at = buttons.iter().find(|(n, _)| n == name).map(|(_, p)| *p).unwrap_or_else(|| panic!("{name}: {buttons:?}"));
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        };
        vec![egui::Event::PointerMoved(at), press(true), press(false)]
    }

    /// The code of a loadout of the test ship, by a sender who knows both parts by their class names.
    fn sent_code(ship: &str) -> String {
        let ids = PartIds::new([(KNOWN.into(), "COOL_Test_S1".into()), (STRANGE.into(), "COOL_Strange_S1".into())]);
        let mut sent = Equipment { name: "Rennen".into(), ..Equipment::default() };
        sent.put(versekit_core::equipment::Slot::new("c1", KNOWN, Acquisition::Crafted));
        sent.put(versekit_core::equipment::Slot::new("c2", STRANGE, Acquisition::Purchased));
        sent.put(versekit_core::equipment::Slot::new("s1", "", Acquisition::Unknown));
        equipment_share::encode(&equipment_share::share(ship, "", &sent, &ids))
    }

    #[test]
    fn a_pasted_code_shows_what_arrives_first_and_is_put_in_on_the_click() {
        let _language = crate::language_lock();
        versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
        let ship = entry(r#"{"name": "Test Ship", "hersteller": "", "kurz": "test_ship", "belegung": {}}"#);
        let stores = Stores::load(&folder("teilen"));
        // The receiver's game data know only one of the two parts.
        let extra = Extra { ids: PartIds::new([(KNOWN.into(), "COOL_Test_S1".into())]), ..Extra::default() };
        let mut state = EquipmentState::default();
        let (texts, buttons, action) = box_frames(&stores, &extra, &ship, &mut state, Vec::new());
        assert_eq!(action, None);
        for word in ["s_wk_teilen_fuer", "s_wk_einfuegen_fuer", "s_wk_datei_speichern_fuer", "s_wk_datei_oeffnen_fuer"] {
            let name = tf(word, &[("schiff", Arg::from("Test Ship"))]);
            assert!(buttons.iter().any(|(n, _)| *n == name), "{name}: {buttons:?}");
        }
        assert!(!texts.is_empty());
        // Share is asked of the page.
        let share = tf("s_wk_teilen_fuer", &[("schiff", Arg::from("Test Ship"))]);
        let (_, _, action) = box_frames(&stores, &extra, &ship, &mut state, click_on(&buttons, &share));
        assert_eq!(action, Some(Action::Share { key: "key".into() }));
        // A code in a message: the ship, the name, the known part, the unknown one said as such, the emptied port.
        state.pasting = Some(Pasting { key: "key".into(), text: format!("Schau mal: {}  :)", sent_code("Test Ship")) });
        let (texts, buttons, _) = box_frames(&stores, &extra, &ship, &mut state, Vec::new());
        let lines = [
            tf("s_wk_code_schiff", &[("name", Arg::from("Rennen")), ("schiff", Arg::from("Test Ship"))]),
            tf("s_wk_code_teil", &[("n", Arg::from(1_usize)), ("teil", Arg::from("COOL_Test_S1"))]),
            tf("s_wk_code_unbekannt", &[("n", Arg::from(1_usize)), ("kennung", Arg::from("COOL_Strange_S1"))]),
            tf("s_wk_code_leer", &[("n", Arg::from(1_usize))]),
        ];
        for line in &lines {
            assert!(texts.contains(line), "{line}: {texts:?}");
        }
        assert!(texts.iter().any(|text| text.contains("unbekanntes Teil")), "said in words: {texts:?}");
        let (_, _, action) = box_frames(&stores, &extra, &ship, &mut state, click_on(&buttons, &t("s_wk_code_anlegen")));
        let Some(Action::AddShared { equipments, key }) = action else { panic!("{action:?}") };
        assert_eq!(key, "key");
        let came = &equipments[0];
        assert_eq!(came.name, "Rennen");
        assert_eq!(came.slot("c1").map(|s| s.item_guid.as_str()), Some(KNOWN));
        assert!(came.slot("c2").is_none(), "the unknown part leaves the factory part");
        assert_eq!(came.slot("s1").map(|s| s.item_guid.as_str()), Some(""));
        assert!(state.pasting.is_none(), "the field closes");
    }

    #[test]
    fn the_loadout_of_another_ship_of_the_type_is_taken_over_from_the_list() {
        let _language = crate::language_lock();
        versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
        let ship = entry(r#"{"name": "Test Ship", "hersteller": "", "kurz": "test_ship", "belegung": {}}"#);
        let stores = Stores::load(&folder("uebernehmen"));
        let extra = Extra::default();
        let mut theirs = Equipment { name: "Mining".into(), ..Equipment::default() };
        theirs.put(versekit_core::equipment::Slot::new("c1", KNOWN, Acquisition::Pledge));
        let offered = vec![("Geplant: Test Ship, Variante „Mining“".to_string(), theirs.clone())];
        let others: &OthersOf<'_> = &|_: &Map| offered.clone();
        let nothing: &OthersOf<'_> = &|_: &Map| Vec::new();
        let frames = |others: &OthersOf<'_>,
                      state: &mut EquipmentState,
                      events: Vec<egui::Event>| {
            let ctx = egui::Context::default();
            crate::install(&ctx);
            ctx.enable_accesskit();
            ctx.all_styles_mut(|style| style.animation_time = 0.0);
            let scheme = versekit_core::theme::current();
            let (mut action, mut texts, mut names) = (None, Vec::new(), Vec::new());
            let mut events = Some(events);
            for round in 0..4 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 2400.0))),
                    events: if round == 2 { events.take().unwrap_or_default() } else { Vec::new() },
                    ..Default::default()
                };
                let mut output = ctx.run_ui(input, |ui| {
                    state.open_all = true;
                    let box_ctx = Ctx { stores: &stores, extra: &extra, sources: &Fake, paths: None, others: Some(others) };
                    if let Some(asked) = show(ui, scheme, &box_ctx, state, &ship, "key", false) {
                        action = Some(asked);
                    }
                });
                // No renderer takes the new textures; egui complains about a pass dropped with them.
                output.textures_delta.clear();
                fn collect(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
                    match shape {
                        egui::epaint::Shape::Text(text) => out.push(text.galley.text().to_string()),
                        egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, out)),
                        _ => {}
                    }
                }
                texts.clear();
                output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
                names = output
                    .platform_output
                    .accesskit_update
                    .map(|u| {
                        u.nodes
                            .iter()
                            .filter(|(_, n)| n.role() == egui::accesskit::Role::Button)
                            .filter_map(|(_, n)| {
                                let b = n.bounds()?;
                                Some((n.label()?.to_string(), egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32)))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
            }
            (texts, names, action)
        };
        let mut state = EquipmentState::default();
        let (_, buttons, _) = frames(others, &mut state, Vec::new());
        let open = tf("s_wk_uebernehmen_fuer", &[("schiff", Arg::from("Test Ship"))]);
        frames(others, &mut state, click_on(&buttons, &open));
        let (texts, buttons, _) = frames(others, &mut state, Vec::new());
        assert!(texts.iter().any(|t| t.contains("Mining")), "the list is open: {texts:?}");
        let take = tf("s_wk_uebernehmen_knopf_fuer", &[("was", Arg::from(offered[0].0.as_str()))]);
        let (_, _, action) = frames(others, &mut state, click_on(&buttons, &take));
        assert_eq!(action, Some(Action::AddShared { equipments: vec![theirs], key: "key".into() }));
        assert_eq!(state.borrowing, None, "the list closes");
        // Without any other ship of the type: said in a sentence.
        let mut state = EquipmentState { borrowing: Some("key".into()), ..EquipmentState::default() };
        let (texts, _, _) = frames(nothing, &mut state, Vec::new());
        assert!(texts.contains(&t("s_wk_uebernehmen_keine")), "{texts:?}");
    }

    #[test]
    fn a_code_of_another_ship_or_a_broken_one_is_said_and_puts_nothing_in() {
        let _language = crate::language_lock();
        versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
        let ship = entry(r#"{"name": "Test Ship", "hersteller": "", "kurz": "test_ship", "belegung": {}}"#);
        let stores = Stores::load(&folder("teilen-fremd"));
        let extra = Extra { ids: PartIds::new([(KNOWN.into(), "COOL_Test_S1".into())]), ..Extra::default() };
        let cases = [
            (sent_code("Polaris"), tf("s_wk_code_anderes", &[("schiff", Arg::from("Polaris"))])),
            ("VK1-kaputt".to_string(), t("s_wk_code_kaputt")),
            (format!("VK9-{}", &sent_code("Test Ship")[4..]), t("s_wk_code_neuer")),
            ("nur ein Gruß".to_string(), t("s_wk_code_keiner")),
        ];
        for (text, said) in cases {
            let mut state = EquipmentState { pasting: Some(Pasting { key: "key".into(), text }), ..EquipmentState::default() };
            let (texts, buttons, _) = box_frames(&stores, &extra, &ship, &mut state, Vec::new());
            assert!(texts.contains(&said), "{said}: {texts:?}");
            let (_, _, action) = box_frames(&stores, &extra, &ship, &mut state, click_on(&buttons, &t("s_wk_code_anlegen")));
            assert_eq!(action, None, "{said}: nothing is put in");
        }
        // Counter-check: the same ship's code under the same click is put in.
        let mut state = EquipmentState {
            pasting: Some(Pasting { key: "key".into(), text: sent_code("Test Ship") }),
            ..EquipmentState::default()
        };
        let (_, buttons, _) = box_frames(&stores, &extra, &ship, &mut state, Vec::new());
        let (_, _, action) = box_frames(&stores, &extra, &ship, &mut state, click_on(&buttons, &t("s_wk_code_anlegen")));
        assert!(matches!(action, Some(Action::AddShared { .. })), "{action:?}");
    }
}
