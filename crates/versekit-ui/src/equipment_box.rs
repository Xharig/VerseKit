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
use versekit_core::erkul_slots::{SlotInfo, hardpoints, ship_id};
use versekit_core::hangar::{field_text, field_true};
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
        | Action::Quality { .. } => {}
    }
    changed
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
}

/// What the box draws from.
pub(crate) struct Ctx<'a> {
    pub stores: &'a Stores,
    pub extra: &'a Extra,
    pub sources: &'a dyn Sources,
    pub paths: Option<&'a Paths>,
}

/// A fold of the box. Its identifier is the ship, not the title: the count in the title changes with every tick.
fn fold(title: &str, salt: &str, key: &str, open_all: bool) -> egui::CollapsingHeader {
    let header = widgets::collapsing(title).id_salt((salt.to_string(), key.to_string()));
    if open_all { header.open(Some(true)) } else { header }
}

/// The slots that stand alike, put into one line.
struct Group {
    kind: String,
    size: Option<i64>,
    factory: Option<(String, String)>,
    own: Option<(String, String)>,
    paths: Vec<String>,
    slot: SlotInfo,
}

/// `pages._slot_list`: slots with the same kind, size, factory part and own choice are one line, in the
/// order of their first appearance. A different choice takes a slot out of its group.
fn groups(slots: Vec<SlotInfo>, chosen: &Map) -> Vec<Group> {
    let mut found: Vec<(String, Group)> = Vec::new();
    for slot in slots {
        let own = chosen
            .iter()
            .find(|(path, _)| *path == slot.path)
            .and_then(|(_, part)| part.as_object())
            .map(|part| (field_text(part, "ref").to_string(), field_text(part, "name").to_string()))
            .filter(|(reference, _)| !reference.is_empty());
        let key = format!(
            "{}|{:?}|{}|{}",
            slot.kind,
            slot.size,
            slot.factory.as_ref().map_or("", |(r, _)| r.as_str()),
            own.as_ref().map_or("", |(r, _)| r.as_str())
        );
        match found.iter_mut().find(|(k, _)| *k == key) {
            Some((_, group)) => group.paths.push(slot.path.clone()),
            None => {
                let group = Group {
                    kind: slot.kind.clone(),
                    size: slot.size,
                    factory: slot.factory.clone(),
                    own,
                    paths: vec![slot.path.clone()],
                    slot,
                };
                found.push((key, group));
            }
        }
    }
    found.into_iter().map(|(_, group)| group).collect()
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

fn group_title(group: &Group) -> String {
    let size = group.size.map_or(String::new(), |s| format!(" S{s}"));
    if group.paths.len() > 1 {
        format!("{} × {}{size}", group.paths.len(), group.kind)
    } else {
        format!("{}{size}", group.kind)
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
    for group in groups {
        let id = format!("equipment-pick-{key}-{}", group.paths[0]);
        let title = group_title(group);
        let own = group
            .own
            .as_ref()
            .filter(|(reference, _)| Some(reference.as_str()) != group.factory.as_ref().map(|(r, _)| r.as_str()));
        let listed = group_choices(ctx, state, &id, group, &catalog);
        let (choices, entries) = (&listed.0, &listed.1);
        let stock = usize::from(group.factory.is_some());
        // The part that sits in the slot, as its line in the list reads.
        let shown = match own {
            Some((reference, name)) => match choices.iter().position(|c| same_part(c, reference, name)) {
                Some(at) => entries[at].clone(),
                None => {
                    let label = catalog.get(reference.as_str()).map_or(String::new(), |(g, c)| part_label(g, c, false));
                    if label.is_empty() { name.clone() } else { format!("{name}  ·  {label}") }
                }
            },
            None if stock > 0 => entries[0].clone(),
            None => String::new(),
        };
        let reset = own.is_some();
        let field = Field {
            id: &id,
            name: &format!("{title}: {}", t("s_p9_hg_auswaehlen")),
            hint: &t(if stock > 0 { "s_pl_teil" } else { "s_wk_ab_werk_leer" }),
            // No part at all, or none that holds what was typed.
            empty: &t(if entries.is_empty() { "s_p9_hg_keine_auswahl" } else { "s_mu_auswahl_leer" }),
        };
        line(ui, scheme, stacked, &title, |row| {
            let width = (row.available_width() - button_room(row, 1)).max(1.0);
            let (picked, _) = select(row, scheme, state, &field, &shown, entries, width);
            match picked {
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
            if row_button(row, scheme, "undo-2", &meaning, &title, reset) {
                actions.push(Action::Clear { paths: group.paths.clone() });
            }
        });
        if let Some((reference, name)) = own {
            let part = Part { reference, name, stacked };
            slot_details(ui, scheme, ctx, state, &id, group, &part, active, actions);
        }
    }
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
            row.add(egui::Label::new(RichText::new(t(problem)).small().color(color(scheme.red))).wrap());
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
        }
    }

    #[test]
    fn slots_that_stand_alike_are_one_line_and_a_different_choice_leaves_the_group() {
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
        let chosen = entry(r#"{"b": {"ref": "other", "name": "Other"}}"#);
        let split = groups(slots, &chosen);
        assert_eq!(split.iter().map(|g| g.paths.clone()).collect::<Vec<_>>(), [vec!["a", "c"], vec!["b"], vec!["d"]]);
        assert_eq!(split[1].own, Some(("other".to_string(), "Other".to_string())));
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
        std::fs::write(
            paths.app_file(versekit_core::ships::ERKUL_FILE),
            r#"{"format": 4, "spielversion": "4.0", "hersteller": {}, "schiffe": {"testship": {"id": "test_ship", "name": "Test Ship",
                "plaetze": [], "slots": [
                {"pfad": "c1", "art": "Cooler", "groesse": 1, "werk": {"ref": "f-cooler", "name": "Factory Cooler"}},
                {"pfad": "c2", "art": "Cooler", "groesse": 1, "werk": {"ref": "f-cooler", "name": "Factory Cooler"}},
                {"pfad": "c3", "art": "Cooler", "groesse": 1, "werk": {"ref": "f-cooler", "name": "Factory Cooler"}},
                {"pfad": "s1", "art": "Shield", "groesse": 2}]}}}"#,
        )
        .expect("erkul file");
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
                    let box_ctx = Ctx { stores, extra: &extra, sources: &Fake, paths: None };
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
        versekit_core::language::set_language(versekit_core::language::Choice::De);
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
        // With the stored slots: the group of the two alike, the own choice, the factory part, the cart.
        let paths = folder("box");
        let stores = Stores::load(&paths);
        let mut state = EquipmentState::default();
        let shown = drawn(&stores, &ship, &mut state);
        assert!(shown.contains(&t("s_wk_auslegung")), "{shown:?}");
        assert!(shown.contains(&"2 × Cooler S1".to_string()), "c2 and c3 stand alike: {shown:?}");
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
        use versekit_core::language::{Choice as Lang, set_language};
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
        let paths = folder("rand");
        let stores = Stores::load(&paths);
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
            set_language(lang);
            for points in versekit_core::font_size::POINTS_RANGE.0..=versekit_core::font_size::POINTS_RANGE.1 {
                for width in [380.0_f32, 520.0, 900.0] {
                    // 0: as it is, 1: the list of the coolers open, 2: renaming to a name another one has.
                    for mode in 0..3 {
                        let picking = mode == 1;
                        let ctx = egui::Context::default();
                        crate::install(&ctx);
                        crate::apply_size(&ctx, points);
                        ctx.all_styles_mut(|style| style.animation_time = 0.0);
                        let extra = Extra { recipes: recipes.clone(), ..Extra::default() };
                        let mut state = EquipmentState { open_all: true, ..EquipmentState::default() };
                        if mode == 2 {
                            state.renaming = Some(Renaming {
                                key: "key".into(),
                                old: "Standard".into(),
                                text: "A Second Loadout With A Long Name".into(),
                                fresh: false,
                                problem: Some("s_wk_ausst_doppelt"),
                            });
                        }
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
                                    let box_ctx = Ctx { stores: &stores, extra: &extra, sources: &Fake, paths: None };
                                    show(ui, scheme, &box_ctx, &mut state, &ship, "key", false);
                                    over = (ui.min_rect().right() - ui.ctx().content_rect().right()).max(0.0);
                                },
                            );
                            output.textures_delta.clear();
                            if frame == 0 && picking {
                                field = output.platform_output.accesskit_update.as_ref().and_then(|update| {
                                    update
                                        .nodes
                                        .iter()
                                        .find(|(_, n)| {
                                            n.role() == egui::accesskit::Role::ComboBox
                                                && n.label().is_some_and(|l| l.starts_with("2 × Cooler"))
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
        set_language(Lang::De);
        let _ = std::fs::remove_dir_all(
            std::env::temp_dir().join(format!("versekit-ausstattung-{}-rand", versekit_core::testdir::run_tag())),
        );
        assert!(findings.is_empty(), "{}", findings.join("\n"));
    }

    /// Puts `parts` into the list of the two coolers alike (`c2`, `c3` of the test ship), the stock cooler at the
    /// top, as the box keeps a list it worked out.
    fn preset_coolers(state: &mut EquipmentState, stores: &Stores, extra: &Extra, parts: Vec<Choice>) {
        let mark = sources_mark(&Ctx { stores, extra, sources: &Fake, paths: None });
        let list = stock_first(parts, Some(&("f-cooler".into(), "Factory Cooler".into())));
        let entries = list
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let text = entry_text(c, &HashMap::new());
                if i == 0 { tf("s_wk_ab_werk", &[("name", Arg::from(text))]) } else { text }
            })
            .collect();
        let key = format!("equipment-pick-key-c2|Cooler|Some(1)|f-cooler|{:?}", versekit_core::language::current());
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
                let box_ctx = Ctx { stores, extra: &extra, sources: &Fake, paths: None };
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
                    .unwrap_or_else(|| panic!("{name} is not there"));
                at = Some(egui::pos2((bounds.x0 + 30.0) as f32, ((bounds.y0 + bounds.y1) / 2.0) as f32));
            }
        }
        asked
    }

    #[test]
    fn a_pick_in_the_field_of_a_slot_sets_the_part_and_the_stock_part_goes_back_to_it() {
        let _language = crate::language_lock();
        versekit_core::language::set_language(versekit_core::language::Choice::De);
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
        // The two coolers alike: their list holds a part besides the stock one.
        preset_coolers(&mut state, &stores, &Extra::default(), vec![choice("Arctic", "arctic")]);
        assert_eq!(click_node(&ctx, &stores, &ship, &mut state, Role::ComboBox, "2 × Cooler S1"), None);
        let set = click_node(&ctx, &stores, &ship, &mut state, Role::ListBoxOption, "Arctic");
        assert_eq!(
            set,
            Some(Action::SetPart {
                paths: vec!["c2".into(), "c3".into()],
                reference: "arctic".into(),
                name: "Arctic".into()
            })
        );
        // The slot with an own part: the stock part at the top of its list takes it back.
        assert_eq!(click_node(&ctx, &stores, &ship, &mut state, Role::ComboBox, "Cooler S1"), None);
        let stock = tf("s_wk_ab_werk", &[("name", Arg::from("Factory Cooler"))]);
        let back = click_node(&ctx, &stores, &ship, &mut state, Role::ListBoxOption, &stock);
        assert_eq!(back, Some(Action::Clear { paths: vec!["c1".into()] }));
        let _ = std::fs::remove_dir_all(
            std::env::temp_dir().join(format!("versekit-ausstattung-{}-wahl", versekit_core::testdir::run_tag())),
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
        versekit_core::language::set_language(versekit_core::language::Choice::De);
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
}
