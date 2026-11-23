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

//! The slots of a ship as blocks: the areas of the game (systems, drive, avionics, weapons, mining, cargo, other),
//! in each area one block per part with a coloured edge at the left, the slots below a part indented in the same
//! block and always visible, many alike slots as a small grid, and the way a part was got as a small tag at the
//! right of its line.

use std::collections::HashSet;

use super::*;

/// The width one level of indentation takes.
const INDENT: f32 = 18.0;
/// The width of the coloured edge of a block.
const EDGE: f32 = 4.0;
/// From this many alike slots on they stand in a grid.
const GRID_FROM: usize = 4;
/// The slots in one row of the grid, at most; fewer where the room is small.
const GRID_COLUMNS: usize = 4;
/// The frame of a cell of the grid: its line and the margin inside it take this room on each side. The line is
/// there in every cell (clear where the cell is like most), so that all cells are as wide.
const CELL_STROKE: f32 = 1.5;
const CELL_MARGIN: i8 = 2;
const CELL_FRAME: f32 = CELL_STROKE + CELL_MARGIN as f32;

/// A part of the game's own order of the ship systems.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Area {
    Systems,
    Drive,
    Avionics,
    Weapons,
    Mining,
    Cargo,
    Other,
}

/// The areas in the order they are shown, with the key of their heading.
const AREAS: [(Area, &str); 7] = [
    (Area::Systems, "s_wk_bereich_systeme"),
    (Area::Drive, "s_wk_bereich_antrieb"),
    (Area::Avionics, "s_wk_bereich_avionik"),
    (Area::Weapons, "s_wk_bereich_waffen"),
    (Area::Mining, "s_wk_bereich_bergbau"),
    (Area::Cargo, "s_wk_bereich_fracht"),
    (Area::Other, "s_wk_bereich_sonstiges"),
];

/// The area a kind of slot belongs to; a kind no area knows is "other".
pub(super) fn area_of(kind: &str) -> Area {
    match kind.to_lowercase().as_str() {
        "powerplant" | "cooler" | "shield" => Area::Systems,
        "quantumdrive" | "jumpdrive" => Area::Drive,
        "radar" | "battery" | "flightcontroller" | "avionics" => Area::Avionics,
        "weapongun" | "turret" | "turretbase" | "missilelauncher" | "missile" | "bomblauncher" | "bomb" | "emp" => {
            Area::Weapons
        }
        "weaponmining" | "mininglaser" | "miningmodifier" => Area::Mining,
        "container" | "cargo" => Area::Cargo,
        _ => Area::Other,
    }
}

/// The edge of the blocks of mining: a green of its own that no scheme changes, so that mining and weapons stay
/// apart where a scheme (the Cartel one) makes the house colour orange as well.
const MINING_GREEN: egui::Color32 = egui::Color32::from_rgb(0x3F, 0xB9, 0x6B);

/// The colour at the left edge of the blocks of an area, out of the colours of the scheme: mining in a green of its
/// own, weapons between the gold and the red, cargo between the blue and the red, the rest in the line colour.
pub(super) fn edge_color(scheme: &Scheme, area: Area) -> egui::Color32 {
    let mix = |a: versekit_core::theme::Color, b: versekit_core::theme::Color| {
        let half = |x: u8, y: u8| u8::try_from((u16::from(x) + u16::from(y)) / 2).unwrap_or(x);
        egui::Color32::from_rgb(half(a.r, b.r), half(a.g, b.g), half(a.b, b.b))
    };
    match area {
        Area::Mining => MINING_GREEN,
        Area::Weapons => mix(scheme.gold, scheme.red),
        Area::Cargo => mix(scheme.blue, scheme.red),
        _ => color(scheme.line),
    }
}

/// A part that holds slots, or the heading of slots that hang in no part of the list.
pub(super) struct Node {
    /// The slot of the part; `None` for a heading only (a remote turret that is no slot, slots on the ship itself).
    pub(super) head: Option<Group>,
    /// The name above the slots where there is no part.
    label: String,
    kids: Vec<Kid>,
    /// A small line above blocks of slots that stand alike (their count, "mixed" or "all alike", the button).
    pub(super) header: Option<Group>,
}

#[allow(clippy::large_enum_variant)]
enum Kid {
    /// Slots that stand alike: one line each, or a grid.
    Slots(Group),
    /// A part that holds slots of its own.
    Node(usize),
}

/// The slot kind and size in words, without a count and without a mount: "Mining head S2".
fn kind_title(group: &Group) -> String {
    numbered_title(group, None)
}

/// The kind with a number before the size: "Shield generator 2 S2".
fn numbered_title(group: &Group, number: Option<usize>) -> String {
    let size = group.size.map_or(String::new(), |s| format!(" S{s}"));
    let kind = if group.kind.is_empty() { t("s_wk_platz") } else { kind_word(&group.kind) };
    match number {
        Some(n) => format!("{kind} {n}{size}"),
        None => format!("{kind}{size}"),
    }
}

/// Whether many alike slots stand in a grid: enough of them, none fixed, none of a built part (which needs its
/// own lines for the blueprint and the quality).
fn in_grid(group: &Group, crafted: &dyn Fn(&str) -> bool) -> bool {
    group.paths.len() >= GRID_FROM && !group.slot.fixed && !group.paths.iter().any(|path| crafted(path))
}

/// The lines of the list as a forest of parts: each part that holds slots is a node of its own (four gimbal mounts
/// are four nodes), the lines below a part are its kids. Returns the nodes and the places of the roots.
pub(super) fn forest(
    grouped: Vec<Group>,
    holders: &HashSet<String>,
    crafted: &dyn Fn(&str) -> bool,
) -> (Vec<Node>, Vec<usize>) {
    let mut nodes: Vec<Node> = Vec::new();
    let mut roots: Vec<usize> = Vec::new();
    let mut by_path: HashMap<String, usize> = HashMap::new();
    // Where a line hangs: the node of its mount, made up where the mount is no slot.
    fn parent(
        mount: &Option<(String, String)>,
        nodes: &mut Vec<Node>,
        roots: &mut Vec<usize>,
        by_path: &mut HashMap<String, usize>,
    ) -> Option<usize> {
        let (path, name) = mount.as_ref()?;
        if let Some(at) = by_path.get(path) {
            return Some(*at);
        }
        nodes.push(Node { head: None, label: name.clone(), kids: Vec::new(), header: None });
        roots.push(nodes.len() - 1);
        by_path.insert(path.clone(), nodes.len() - 1);
        Some(nodes.len() - 1)
    }
    for group in grouped {
        let holder = group.paths.first().is_some_and(|path| holders.contains(path));
        let above = parent(&group.mount, &mut nodes, &mut roots, &mut by_path);
        if holder {
            for at in 0..group.paths.len() {
                let one = group.single(at);
                nodes.push(Node { head: Some(one), label: String::new(), kids: Vec::new(), header: None });
                let id = nodes.len() - 1;
                by_path.insert(group.paths[at].clone(), id);
                match above {
                    Some(parent) => nodes[parent].kids.push(Kid::Node(id)),
                    None => roots.push(id),
                }
            }
        } else {
            match above {
                Some(parent) => nodes[parent].kids.push(Kid::Slots(group)),
                None => {
                    let count = group.paths.len();
                    if count > 1 && !group.slot.fixed && !in_grid(&group, crafted) {
                        // Slots that stand alike, few: a block each, under a line that says what they hold.
                        nodes.push(Node { head: None, label: String::new(), kids: Vec::new(), header: Some(group.clone()) });
                        roots.push(nodes.len() - 1);
                        for at in 0..count {
                            nodes.push(Node { head: Some(group.single(at)), label: String::new(), kids: Vec::new(), header: None });
                            roots.push(nodes.len() - 1);
                        }
                    } else {
                        nodes.push(Node { head: None, label: String::new(), kids: vec![Kid::Slots(group)], header: None });
                        roots.push(nodes.len() - 1);
                    }
                }
            }
        }
    }
    (nodes, roots)
}

/// The area of a node: the kind of its part, else of its first line below.
fn node_area(nodes: &[Node], id: usize) -> Area {
    let node = &nodes[id];
    if let Some(header) = &node.header {
        return area_of(&header.kind);
    }
    if let Some(head) = &node.head {
        return area_of(&head.kind);
    }
    node.kids
        .iter()
        .map(|kid| match kid {
            Kid::Slots(group) => area_of(&group.kind),
            Kid::Node(child) => node_area(nodes, *child),
        })
        .next()
        .unwrap_or(Area::Other)
}

/// The names of the parts that stand as a block, a number after those that read alike: "Mining head S2 1", "… 2".
fn head_titles(nodes: &[Node]) -> Vec<String> {
    let plain: Vec<String> = nodes.iter().map(|n| n.head.as_ref().map_or_else(|| n.label.clone(), kind_title)).collect();
    plain
        .iter()
        .enumerate()
        .map(|(at, title)| {
            let Some(head) = &nodes[at].head else { return title.clone() };
            let alike: Vec<usize> =
                (0..plain.len()).filter(|other| nodes[*other].head.is_some() && plain[*other] == *title).collect();
            match alike.iter().position(|other| *other == at) {
                Some(n) if alike.len() > 1 => numbered_title(head, Some(n + 1)),
                _ => title.clone(),
            }
        })
        .collect()
}

/// What every block draws from, besides the lines.
struct Blocks<'a> {
    of: LineOf<'a>,
    key: &'a str,
    nodes: &'a [Node],
    titles: &'a [String],
}

/// The slots of the ship in the order of the game's areas, each part a block. `holders` are the paths of the slots
/// that hold slots.
#[allow(clippy::too_many_arguments)]
pub(super) fn slot_list(
    ui: &mut Ui,
    scheme: &Scheme,
    ctx: &Ctx,
    state: &mut EquipmentState,
    key: &str,
    grouped: Vec<Group>,
    holders: &HashSet<String>,
    active: Option<&Equipment>,
    actions: &mut Vec<Action>,
) {
    let crafted = |path: &str| {
        active.and_then(|e| e.slot(path)).is_some_and(|s| s.acquisition == Acquisition::Crafted)
    };
    let (nodes, roots) = forest(grouped, holders, &crafted);
    let titles = head_titles(&nodes);
    // Room for the title, the field, the way back and the tag, and the indent and edge of a block.
    let stacked = ui.available_width()
        < TITLE_WIDTH
            + ui.spacing().item_spacing.x * 2.0
            + FIELD_MIN_WIDTH
            + button_room(ui, 1)
            + origin_width(ui)
            + 3.0 * INDENT
            + EDGE;
    let catalog: HashMap<&str, (&str, &str)> =
        ctx.stores.catalog.iter().map(|p| (p.code.as_str(), (p.grade.as_str(), p.class.as_str()))).collect();
    let blocks = Blocks { of: LineOf { scheme, ctx, stacked, catalog: &catalog, active }, key, nodes: &nodes, titles: &titles };
    for (area, heading) in AREAS {
        let ours: Vec<usize> = roots.iter().copied().filter(|id| node_area(&nodes, *id) == area).collect();
        if ours.is_empty() {
            continue;
        }
        ui.add_space(4.0);
        ui.label(widgets::bold(ui, t(heading), egui::TextStyle::Body));
        for id in ours {
            if let Some(header) = &nodes[id].header {
                group_head(ui, &blocks.of, header, actions);
                continue;
            }
            let frame = egui::Frame::new()
                .fill(color(scheme.field))
                .corner_radius(4)
                .inner_margin(egui::Margin { left: 12, right: 8, top: 6, bottom: 6 });
            let shown = frame.show(ui, |inner| {
                inner.set_width(inner.available_width());
                show_node(inner, &blocks, state, id, actions);
            });
            let rect = shown.response.rect;
            let bar = egui::Rect::from_min_max(rect.min, egui::pos2(rect.min.x + EDGE, rect.max.y));
            let radius = egui::CornerRadius { nw: 4, sw: 4, ne: 0, se: 0 };
            ui.painter().rect_filled(bar, radius, edge_color(scheme, area));
            ui.add_space(4.0);
        }
    }
}

/// A part with the slots below it, indented in the same block.
fn show_node(ui: &mut Ui, blocks: &Blocks, state: &mut EquipmentState, id: usize, actions: &mut Vec<Action>) {
    let node = &blocks.nodes[id];
    let title = &blocks.titles[id];
    match &node.head {
        Some(head) => {
            let slot_id = format!("equipment-slot-{}-{}", blocks.key, head.paths[0]);
            slot_line(ui, &blocks.of, state, &slot_id, head, (title, title), actions);
        }
        None if !node.label.is_empty() => {
            ui.label(widgets::bold(ui, node.label.clone(), egui::TextStyle::Body));
        }
        None => {}
    }
    if node.kids.is_empty() {
        return;
    }
    let nested = node.head.is_some() || !node.label.is_empty();
    let margin = egui::Margin { left: if nested { INDENT as i8 } else { 0 }, ..egui::Margin::ZERO };
    egui::Frame::new().inner_margin(margin).show(ui, |inner| {
        inner.set_width(inner.available_width());
        for kid in &node.kids {
            match kid {
                Kid::Slots(group) => show_slots(inner, blocks, state, group, title, actions),
                Kid::Node(child) => show_node(inner, blocks, state, *child, actions),
            }
        }
    });
}

/// The action that puts the part of the first slot of a line into all the others, `None` where they hold it already.
pub(super) fn same_for_all(group: &Group) -> Option<Action> {
    if group.common().is_some() || group.paths.len() < 2 {
        return None;
    }
    let paths: Vec<String> = group.paths[1..].to_vec();
    Some(match &group.fills[0] {
        Fill::Own(reference, name) => Action::SetPart { paths, reference: reference.clone(), name: name.clone() },
        Fill::Empty => Action::Empty { paths },
        Fill::Stock => match &group.members[0].factory {
            Some((reference, name)) if group.members[1..].iter().any(|m| m.factory.as_ref().map(|(r, _)| r) != Some(reference)) => {
                Action::SetPart { paths, reference: reference.clone(), name: name.clone() }
            }
            Some(_) => Action::Clear { paths },
            None => Action::Empty { paths },
        },
    })
}

/// Slots that stand alike: a line where there is one, else a small heading with the button "all alike" and then
/// a line each, or a grid where there are many.
fn show_slots(ui: &mut Ui, blocks: &Blocks, state: &mut EquipmentState, group: &Group, mount: &str, actions: &mut Vec<Action>) {
    let of = &blocks.of;
    let title = kind_title(group);
    // What a screen reader says for the slots: the part they sit in, then what they are.
    let whole = if mount.is_empty() { title.clone() } else { format!("{mount}: {title}") };
    if group.paths.len() == 1 {
        let id = format!("equipment-slot-{}-{}", blocks.key, group.paths[0]);
        slot_line(ui, of, state, &id, group, (&title, &whole), actions);
        return;
    }
    if group.slot.fixed {
        // Slots the game fixes are no choice: one line for all of them, "16 × container S1 · part · built in".
        let id = format!("equipment-slot-{}-{}", blocks.key, group.paths[0]);
        let counted = format!("{} × {title}", group.paths.len());
        let spoken = format!("{whole}, {counted}");
        slot_line(ui, of, state, &id, group, (&counted, &spoken), actions);
        return;
    }
    group_head(ui, of, group, actions);
    let crafted = |path: &str| {
        of.active.and_then(|e| e.slot(path)).is_some_and(|s| s.acquisition == Acquisition::Crafted)
    };
    if in_grid(group, &crafted) {
        show_grid(ui, blocks, state, group, &whole, actions);
        return;
    }
    for at in 0..group.paths.len() {
        let one = group.single(at);
        let id = format!("equipment-slot-{}-{}", blocks.key, one.paths[0]);
        let slot_title = tf("s_wk_platz_n", &[("n", Arg::from((at + 1).to_string()))]);
        let spoken = format!("{whole}, {slot_title}");
        slot_line(ui, of, state, &id, &one, (&slot_title, &spoken), actions);
    }
}

/// The small line above slots that stand alike: how many, whether they hold the same part ("all alike") or not
/// ("mixed", tinted), and the button that puts the part of the first into all the others.
fn group_head(ui: &mut Ui, of: &LineOf, group: &Group, actions: &mut Vec<Action>) {
    let heading = format!("{} × {}", group.paths.len(), kind_title(group));
    let mixed = group.common().is_none();
    let mark = t(if mixed { "s_wk_marke_gemischt" } else { "s_wk_marke_gleich" });
    let tone = if mixed { mixed_tone(of.scheme) } else { color(of.scheme.sub) };
    let word = t("s_wk_alle_gleich");
    let asked = same_for_all(group);
    let button_width = text_width(ui, &word, egui::TextStyle::Button, false) + 24.0;
    let left_width = text_width(ui, &heading, egui::TextStyle::Small, false) + origin_width(ui) + ui.spacing().item_spacing.x;
    spread(
        ui,
        left_width,
        button_width,
        0.0,
        |row| {
            row.label(sub(of.scheme, heading.clone()));
            tag(row, of.scheme, &mark, tone, &heading, false);
        },
        |right| {
            let spoken = tf("s_wk_alle_gleich_fuer", &[("platz", Arg::from(heading.as_str()))]);
            let reason = t("s_wk_alle_gleich_schon");
            let button = widgets::Button::new(&word, ButtonKind::Normal, of.scheme).spoken(&spoken);
            let button = button.blocked(asked.is_none().then_some(reason.as_str()));
            if right.add(button).on_hover_text(t("s_wk_alle_gleich_tip")).clicked()
                && let Some(action) = asked.clone()
            {
                actions.push(action);
            }
        },
    );
}

/// The tint of what differs: the cargo colour of the scheme (blue and red mixed).
pub(super) fn mixed_tone(scheme: &Scheme) -> egui::Color32 {
    edge_color(scheme, Area::Cargo)
}

/// The least width of one cell of the grid, from the size of the text: the number, a field that shows a few words of
/// a part, the button back to the stock part; and the tag of the origin below them.
fn cell_min(ui: &Ui) -> f32 {
    let gap = ui.spacing().item_spacing.x;
    let number = text_width(ui, "88", egui::TextStyle::Small, false);
    let field = text_width(ui, "MMMMMMMM", egui::TextStyle::Body, false) + 2.0 * ui.spacing().interact_size.y;
    let top = number + gap + field + button_room(ui, 1);
    top.max(origin_width(ui)) + 2.0 * CELL_FRAME
}

/// Many alike slots as small cells, a few in a row, each with its number: the field with the way back, below it the
/// tag. A cell that holds another part than most of them has a coloured frame.
fn show_grid(ui: &mut Ui, blocks: &Blocks, state: &mut EquipmentState, group: &Group, title: &str, actions: &mut Vec<Action>) {
    let of = &blocks.of;
    let gap = ui.spacing().item_spacing.x;
    let room = ui.available_width();
    let columns = (((room + gap) / (cell_min(ui) + gap)).floor() as usize).clamp(1, GRID_COLUMNS);
    let width = ((room - gap * (columns as f32 - 1.0)) / columns as f32).floor();
    const FRAME: f32 = CELL_FRAME;
    let inner = width - 2.0 * FRAME;
    let height = ui.spacing().interact_size.y * 2.0 + ui.spacing().item_spacing.y + 2.0 * FRAME;
    let parts: Vec<&str> = (0..group.paths.len()).map(|at| group.part_at(at)).collect();
    let usual = parts
        .iter()
        .max_by_key(|part| parts.iter().filter(|other| other == part).count())
        .copied()
        .unwrap_or("");
    for first in (0..group.paths.len()).step_by(columns) {
        ui.horizontal_top(|row| {
            let last = (first + columns).min(group.paths.len());
            for (at, part) in parts.iter().enumerate().take(last).skip(first) {
                let one = group.single(at);
                let id = format!("equipment-slot-{}-{}", blocks.key, one.paths[0]);
                let number = (at + 1).to_string();
                let spoken = format!("{title}, {}", tf("s_wk_platz_n", &[("n", Arg::from(number.clone()))]));
                let edge = if *part == usual { egui::Stroke::new(CELL_STROKE, egui::Color32::TRANSPARENT) } else { egui::Stroke::new(CELL_STROKE, mixed_tone(of.scheme)) };
                row.allocate_ui(egui::vec2(width, height), |cell| {
                    cell.set_width(width);
                    egui::Frame::new().stroke(edge).corner_radius(4).inner_margin(egui::Margin::same(CELL_MARGIN)).show(cell, |cell| {
                        cell.set_width(inner);
                        // The two lines of a cell stand one below the other, whatever the layout of the row is.
                        cell.vertical(|stack| {
                            stack.horizontal(|top| {
                                top.label(sub(of.scheme, number.clone()));
                                let room = inner - button_room(top, 1) - text_width(top, &number, egui::TextStyle::Small, false) - gap;
                                slot_controls(top, of, state, &id, &one, &spoken, room.max(1.0), actions);
                            });
                            stack.horizontal(|below| origin(below, of, state, &id, &one, &spoken, actions));
                        });
                    });
                });
            }
        });
    }
}

/// One slot: its name, the field with the way back, the tag of its origin; below it what a built part adds.
fn slot_line(
    ui: &mut Ui,
    of: &LineOf,
    state: &mut EquipmentState,
    id: &str,
    group: &Group,
    names: (&str, &str),
    actions: &mut Vec<Action>,
) {
    let (title, spoken) = names;
    line(ui, of.scheme, of.stacked, title, |row| {
        let gap = row.spacing().item_spacing.x;
        let width = (row.available_width() - button_room(row, 1) - origin_width(row) - gap).max(1.0);
        slot_controls(row, of, state, id, group, spoken, width, actions);
        origin(row, of, state, id, group, spoken, actions);
    });
    // A port the game data give no size (one right on the ship, empty at the factory): every part of its kind.
    if group.size.is_none() && group.kind != "Paints" {
        ui.add(egui::Label::new(sub(of.scheme, t("s_wk_groesse_offen"))).wrap());
    }
    let own = group
        .own
        .as_ref()
        .filter(|(reference, _)| Some(reference.as_str()) != group.factory.as_ref().map(|(r, _)| r.as_str()));
    if let Some((reference, name)) = own
        && !group.slot.fixed
    {
        let part = Part { reference, name, stacked: of.stacked };
        built_details(ui, of.scheme, of.ctx, state, id, group, &part, of.active, actions);
    }
}

/// The width of the tag of the origin: the widest word it can say.
fn origin_width(ui: &Ui) -> f32 {
    let mut words: Vec<String> = WAYS.iter().map(|a| acquisition_word(*a)).collect();
    words.push(t("s_wk_ab_werk_schild"));
    words.push(t("s_wk_leer"));
    words.push(t("s_wk_marke_gemischt"));
    words.push(t("s_wk_marke_gleich"));
    words.iter().map(|w| text_width(ui, w, egui::TextStyle::Small, false)).fold(0.0, f32::max) + 20.0
}

/// A small tag: its word in `tone` in a thin frame; a click where it can be one (`click`).
fn tag(ui: &mut Ui, scheme: &Scheme, word: &str, tone: egui::Color32, name: &str, click: bool) -> egui::Response {
    let width = origin_width(ui);
    let sense = if click { egui::Sense::click() } else { egui::Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, ui.spacing().interact_size.y), sense);
    if ui.is_rect_visible(rect) {
        let inner = rect.shrink2(egui::vec2(0.0, 2.0));
        let hot = click && response.hovered();
        let stroke = egui::Stroke::new(1.0, if hot { color(scheme.accent) } else { tone });
        ui.painter().rect(inner, 8.0, color(scheme.surface), stroke, egui::StrokeKind::Inside);
        let font = egui::TextStyle::Small.resolve(ui.style());
        ui.painter().text(inner.center(), egui::Align2::CENTER_CENTER, word, font, tone);
    }
    let spoken = format!("{name}: {word}");
    let enabled = response.enabled();
    let role = if click { egui::accesskit::Role::Button } else { egui::accesskit::Role::Label };
    response.widget_info(|| egui::WidgetInfo::labeled(role, enabled, spoken.clone()));
    if click {
        widgets::focus_ring(ui, &response, scheme, egui::CornerRadius::same(8)); response.on_hover_cursor(egui::CursorIcon::PointingHand) } else { response }
}

/// The tag at the right of the line of a slot: "from the factory" for the stock part; for a part of the player's
/// the way it was got, with a list to change it; "to get" (in yellow) as long as that is not known.
fn origin(row: &mut Ui, of: &LineOf, state: &mut EquipmentState, id: &str, group: &Group, spoken: &str, actions: &mut Vec<Action>) {
    let scheme = of.scheme;
    let own = group
        .own
        .as_ref()
        .filter(|(reference, _)| Some(reference.as_str()) != group.factory.as_ref().map(|(r, _)| r.as_str()));
    let Some((reference, name)) = own.filter(|_| !group.slot.fixed) else {
        let (word, tone) = match group.common() {
            Some(Fill::Empty) => (t("s_wk_leer"), color(scheme.sub)),
            _ => (t("s_wk_ab_werk_schild"), color(scheme.sub)),
        };
        tag(row, scheme, &word, tone, spoken, false);
        return;
    };
    let acquisition = of.active.and_then(|e| e.slot(&group.paths[0])).map_or(Acquisition::Unknown, |s| s.acquisition);
    let tone = if acquisition == Acquisition::Unknown { color(scheme.yellow) } else { color(scheme.fg) };
    let name_for_reader = format!("{spoken}: {}", t("s_wk_herkunft"));
    let response = tag(row, scheme, &acquisition_word(acquisition), tone, &name_for_reader, true);
    egui::Popup::menu(&response)
        .id(row.make_persistent_id((id, "herkunft")))
        .frame(
            egui::Frame::new()
                .fill(color(scheme.field))
                .stroke(egui::Stroke::new(1.0, color(scheme.accent)))
                .corner_radius(crate::control_radius(scheme))
                .inner_margin(egui::Margin::same(4)),
        )
        .show(|menu| {
            for way in WAYS {
                let word = acquisition_word(way);
                let spoken = format!("{name}: {word}");
                if menu.add(widgets::Button::new(&word, ButtonKind::Normal, scheme).spoken(&spoken).wide()).clicked() && way != acquisition {
                    let blueprint = (way == Acquisition::Crafted)
                        .then(|| recipe_for(of.ctx.extra, reference, name).map(|r| r.blueprint.clone()))
                        .flatten();
                    actions.push(Action::Acquire { paths: group.paths.clone(), acquisition: way, blueprint });
                    menu.close();
                }
            }
        });
    let _ = state;
}

/// The field of a slot that is list and search in one, with the stock part at the top and "leave empty" at the end,
/// and the way back to the stock part. A fixed slot shows its factory part in a field that is off.
#[allow(clippy::too_many_arguments)]
fn slot_controls(
    row: &mut Ui,
    of: &LineOf,
    state: &mut EquipmentState,
    id: &str,
    group: &Group,
    spoken: &str,
    width: f32,
    actions: &mut Vec<Action>,
) {
    let (scheme, ctx, catalog) = (of.scheme, of.ctx, of.catalog);
    if group.slot.fixed {
        let part = match &group.factory {
            Some((reference, name)) => {
                let label = catalog.get(reference.as_str()).map_or(String::new(), |(g, c)| part_label(g, c, false));
                if label.is_empty() { name.clone() } else { format!("{name}  ·  {label}") }
            }
            None => mixed_text(group),
        };
        let shown = tf("s_wk_fest_teil", &[("name", Arg::from(part))]);
        let reason = t("s_wk_fest_tip");
        let name = format!("{spoken}: {shown}");
        let height = row.spacing().interact_size.y;
        let mut text = shown.clone();
        let field = row.add_enabled_ui(false, |inner| {
            inner.allocate_ui(egui::vec2(width, height), |inner| {
                widgets::SelectField::new(id, &name, "", "", scheme).show::<String>(inner, &mut text, &[]);
            });
        });
        // egui shows no tooltip on a disabled widget by itself: the reason has to be asked for.
        field.response.on_disabled_hover_text(&reason);
        row_button(row, scheme, "undo-2", &reason, spoken, false);
        return;
    }
    let own = group
        .own
        .as_ref()
        .filter(|(reference, _)| Some(reference.as_str()) != group.factory.as_ref().map(|(r, _)| r.as_str()));
    let listed = group_choices(ctx, state, id, group, catalog);
    let (choices, entries) = (&listed.0, &listed.1);
    let stock = usize::from(group.factory.is_some());
    // The end of the list: leave the slot empty. Where the factory fits nothing it is empty already.
    let leave_empty = (stock > 0 || own.is_some()).then(|| t("s_wk_leer_lassen"));
    let mut all: Vec<String> = entries.to_vec();
    all.extend(leave_empty.iter().cloned());
    // The part that sits in the slot, as its line in the list reads.
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
    // Back to the stock part goes where the slot holds something else.
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
}

/// Below a slot with a part that was built: its blueprint and the quality per recipe slot (0 to 1000, 500 when none
/// is set, as on the crafting page).
#[allow(clippy::too_many_arguments)]
fn built_details(
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
    if slot.map_or(Acquisition::Unknown, |s| s.acquisition) != Acquisition::Crafted {
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
        let number_id = format!("{id}-qualitaet-{recipe_slot}");
        line(ui, scheme, part.stacked, &name, |row| {
            let gap = row.spacing().item_spacing.x;
            let margin = egui::Margin::symmetric(4, 2);
            let box_width = text_width(row, "1000", egui::TextStyle::Body, false) + margin.sum().x + 2.0;
            let width = (row.available_width() - box_width - gap - button_room(row, 1)).max(40.0);
            let mut value = i64::from(current);
            let response = row.add(widgets::Slider::new(&mut value, 0, 1000, scheme).width(width).on_release());
            let shown = widgets::Slider::shown(row, &response).unwrap_or(value);
            let enabled = response.enabled();
            response.widget_info(|| egui::WidgetInfo::slider(enabled, shown as f64, &name));
            // The number beside the slider: what is typed stays in a buffer until the field is left.
            let mut text = state.numbers.get(&number_id).cloned().unwrap_or_else(|| shown.to_string());
            let field = widgets::text_field(&mut text, "", scheme)
                .id_salt(&number_id)
                .margin(margin)
                .align(egui::Align2::RIGHT_CENTER)
                .desired_width(box_width);
            let typed = row.add(field);
            let (typed_enabled, typed_text) = (typed.enabled(), text.clone());
            typed.widget_info(|| {
                let mut info = egui::WidgetInfo::labeled(egui::Role::TextInput, typed_enabled, &name);
                info.current_text_value = Some(typed_text.clone());
                info
            });
            if typed.has_focus() || typed.changed() {
                state.numbers.insert(number_id.clone(), text.clone());
            }
            if typed.lost_focus() {
                state.numbers.remove(&number_id);
                let escape = row.input(|i| i.key_pressed(egui::Key::Escape));
                if !escape
                    && let Some(number) = quality_number(&text)
                    && number != current
                {
                    actions.push(Action::Quality { paths: group.paths.clone(), slot: recipe_slot.clone(), value: number });
                }
            }
            if response.changed() {
                let value = u16::try_from(value.clamp(0, 1000)).unwrap_or(equipment::BASE_QUALITY);
                actions.push(Action::Quality { paths: group.paths.clone(), slot: recipe_slot.clone(), value });
            }
        });
    }
}
