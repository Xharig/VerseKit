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

//! The share code of an equipment: one equipment of a ship as a short text to paste into a chat (`VK1-…`), and a
//! small text file with several of them. The code is made and read on the own computer; nothing is sent anywhere.
//!
//! The code holds the ship (name and maker as the hangar writes them), the name of the equipment, the paint and per
//! slot the port path and the part. A part goes by its class name in the game data (`KLWE_LaserRepeater_S3`) where
//! the game data at hand name it, else by its identifier in the game data (the GUID); never by a translated name.
//! How a part was got and its quality are the sender's own and stay out.
//!
//! Form: `VK1-` and then base64url without padding of the raw deflate of compact JSON
//! `{"s": ship, "m": maker, "n": name, "p": paint, "t": [[port path, part], …]}`; a port emptied on purpose has an
//! empty part. The digit after `VK` is the version of the form: a later one is refused with its own reason.
//!
//! ⚠ Reading a code never trusts it: the text is limited before and after unpacking, every field is checked, and
//! anything else is refused with a reason ([`Refused`]), never a panic.

use std::collections::HashMap;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::{Value as Json, json};

use crate::equipment::{Equipment, Slot, Acquisition, port_path};

/// The start of a code of the current form.
pub const PREFIX: &str = "VK1-";
/// The version of the form the program writes and reads.
pub const VERSION: u32 = 1;
/// The longest message a chat like Discord takes; the page warns above it.
pub const CHAT_LIMIT: usize = 2000;
/// The ending of the file with several codes: plain text, so that any editor and any chat shows it.
pub const FILE_ENDING: &str = "txt";

/// A code longer than this is not read.
const MAX_CODE: usize = 20_000;
/// What a code may unpack to at most.
const MAX_UNPACKED: usize = 128 * 1024;
/// The most slots a code may hold.
const MAX_SLOTS: usize = 1000;
/// The longest text in a field of a code.
const MAX_TEXT: usize = 300;

/// Why a text gave no equipment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// Nothing in the text looks like a code.
    NoCode,
    /// A code of a later form: a newer version of the program wrote it.
    Newer,
    /// It starts like a code but cannot be read: cut off, changed or not from the program.
    Damaged,
}

/// One equipment as the code carries it: the parts as class names or identifiers of the game data.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shared {
    /// The name of the ship as the hangar writes it.
    pub ship: String,
    /// Its maker as the hangar writes it; may be empty.
    pub maker: String,
    /// The name of the equipment.
    pub name: String,
    /// Port path and part: a class name, an identifier, or empty for a port emptied on purpose.
    pub slots: Vec<(String, String)>,
    /// The paint, in the same way as a part.
    pub paint: Option<String>,
}

/// The class names of the game data by the identifier of a part, and the way back.
#[derive(Clone, Debug, Default)]
pub struct PartIds {
    class_by_guid: HashMap<String, String>,
    guid_by_class: HashMap<String, String>,
}

impl PartIds {
    /// From pairs of identifier and class name; a pair with an empty side is left out.
    pub fn new(pairs: impl IntoIterator<Item = (String, String)>) -> PartIds {
        let mut ids = PartIds::default();
        for (guid, class) in pairs {
            let (guid, class) = (guid.trim().to_lowercase(), class.trim().to_string());
            if guid.is_empty() || class.is_empty() || !looks_like_guid(&guid) {
                continue;
            }
            ids.guid_by_class.insert(class.to_lowercase(), guid.clone());
            ids.class_by_guid.insert(guid, class);
        }
        ids
    }

    /// How a part goes into a code: its class name where it is known, else its identifier; empty stays empty.
    pub fn token(&self, guid: &str) -> String {
        let guid = guid.trim().to_lowercase();
        self.class_by_guid.get(&guid).cloned().unwrap_or(guid)
    }

    /// The identifier of a class name (case not significant).
    pub fn guid_of(&self, class: &str) -> Option<&str> {
        self.guid_by_class.get(&class.trim().to_lowercase()).map(String::as_str)
    }

    /// Whether the identifier is known to the game data.
    pub fn knows(&self, guid: &str) -> bool {
        self.class_by_guid.contains_key(&guid.trim().to_lowercase())
    }
}

/// Whether a text has the form of an identifier of the game data: `8-4-4-4-12` hexadecimal digits.
pub fn looks_like_guid(text: &str) -> bool {
    let groups: Vec<&str> = text.split('-').collect();
    groups.len() == 5
        && groups.iter().zip([8, 4, 4, 4, 12]).all(|(g, n)| g.len() == n && g.chars().all(|c| c.is_ascii_hexdigit()))
}

/// The equipment of a ship as it is shared: only the parts and the paint.
pub fn share(ship: &str, maker: &str, equipment: &Equipment, ids: &PartIds) -> Shared {
    Shared {
        ship: ship.trim().to_string(),
        maker: maker.trim().to_string(),
        name: equipment.name.trim().to_string(),
        slots: equipment
            .slots
            .iter()
            .map(|slot| {
                let part = if slot.item_guid.trim().is_empty() { String::new() } else { ids.token(&slot.item_guid) };
                (slot.port_path.clone(), part)
            })
            .collect(),
        paint: equipment.paint_item_guid.as_deref().filter(|p| !p.trim().is_empty()).map(|p| ids.token(p)),
    }
}

/// The code of a shared equipment.
pub fn encode(shared: &Shared) -> String {
    let slots: Vec<Json> = shared.slots.iter().map(|(path, part)| json!([path, part])).collect();
    let mut body = json!({"s": shared.ship, "n": shared.name, "t": slots});
    if !shared.maker.is_empty() {
        body["m"] = json!(shared.maker);
    }
    if let Some(paint) = &shared.paint {
        body["p"] = json!(paint);
    }
    let packed = miniz_oxide::deflate::compress_to_vec(body.to_string().as_bytes(), 10);
    format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(packed))
}

/// The file with several codes: two lines that say what it is, then one code per line.
pub fn file_text(codes: &[String]) -> String {
    let mut text = String::from("# Verse-Kit: Ausstattungen / loadouts\n# https://versekit.xharig.com\n");
    for code in codes {
        text.push_str(code);
        text.push('\n');
    }
    text
}

/// Whether a character can stand in the body of a code.
fn body_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

/// The codes in a text, in their order: the version of the form and the body. A code starts with `VK`, digits and
/// `-` at the start of the text or after a character that cannot be part of a code, and runs to the first
/// character that cannot.
fn codes_in(text: &str) -> Vec<(Option<u32>, &str)> {
    let mut found = Vec::new();
    let bytes = text.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        let start_ok = at == 0 || !text[..at].chars().next_back().is_some_and(body_char);
        if !(start_ok && text[at..].starts_with("VK")) {
            at += text[at..].chars().next().map_or(1, char::len_utf8);
            continue;
        }
        let digits_end = at + 2 + text[at + 2..].bytes().take_while(u8::is_ascii_digit).count();
        if digits_end == at + 2 || bytes.get(digits_end) != Some(&b'-') {
            at += 2;
            continue;
        }
        let version = text[at + 2..digits_end].parse::<u32>().ok();
        let body_start = digits_end + 1;
        let body_end = body_start + text[body_start..].bytes().take_while(|b| body_char(char::from(*b))).count();
        found.push((version, &text[body_start..body_end]));
        at = body_end.max(at + 1);
    }
    found
}

/// A text field of a code: trimmed, control characters as spaces, not too long.
fn field(value: &Json) -> Option<String> {
    let text = value.as_str()?;
    if text.chars().count() > MAX_TEXT {
        return None;
    }
    Some(text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect::<String>().trim().to_string())
}

/// A part or a path of a code: visible ASCII only.
fn plain(value: &Json) -> Option<String> {
    let text = field(value)?;
    text.chars().all(|c| c.is_ascii_graphic()).then_some(text)
}

/// The equipment of one body of the current form.
fn read_body(body: &str) -> Result<Shared, Refused> {
    if body.is_empty() || body.len() > MAX_CODE {
        return Err(Refused::Damaged);
    }
    let packed = URL_SAFE_NO_PAD.decode(body).map_err(|_| Refused::Damaged)?;
    let unpacked = miniz_oxide::inflate::decompress_to_vec_with_limit(&packed, MAX_UNPACKED).map_err(|_| Refused::Damaged)?;
    let value: Json = serde_json::from_slice(&unpacked).map_err(|_| Refused::Damaged)?;
    let object = value.as_object().ok_or(Refused::Damaged)?;
    let ship = object.get("s").and_then(field).filter(|s| !s.is_empty()).ok_or(Refused::Damaged)?;
    let maker = match object.get("m") {
        None => String::new(),
        Some(m) => field(m).ok_or(Refused::Damaged)?,
    };
    let name = match object.get("n") {
        None => String::new(),
        Some(n) => field(n).ok_or(Refused::Damaged)?,
    };
    let paint = match object.get("p") {
        None => None,
        Some(p) => Some(plain(p).ok_or(Refused::Damaged)?).filter(|p| !p.is_empty()),
    };
    let list = object.get("t").and_then(Json::as_array).ok_or(Refused::Damaged)?;
    if list.len() > MAX_SLOTS {
        return Err(Refused::Damaged);
    }
    let mut slots: Vec<(String, String)> = Vec::new();
    for item in list {
        let pair = item.as_array().filter(|p| p.len() == 2).ok_or(Refused::Damaged)?;
        let path = plain(&pair[0]).map(|p| port_path(&p)).filter(|p| !p.is_empty()).ok_or(Refused::Damaged)?;
        let part = plain(&pair[1]).ok_or(Refused::Damaged)?;
        // A port named twice holds the part named last.
        slots.retain(|(p, _)| *p != path);
        slots.push((path, part));
    }
    Ok(Shared { ship, maker, name, slots, paint })
}

/// Every code in a text (a pasted message or a file), in its order; an empty list when there is none.
pub fn read_all(text: &str) -> Vec<Result<Shared, Refused>> {
    codes_in(text)
        .into_iter()
        .map(|(version, body)| match version {
            Some(VERSION) => read_body(body),
            Some(later) if later > VERSION => Err(Refused::Newer),
            _ => Err(Refused::Damaged),
        })
        .collect()
}

/// The first code in a text.
pub fn decode(text: &str) -> Result<Shared, Refused> {
    read_all(text).into_iter().next().unwrap_or(Err(Refused::NoCode))
}

/// What a part of a code comes to with the game data of the receiver.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Landing {
    /// The part, by its identifier.
    Part(String),
    /// The port was emptied on purpose.
    Empty,
    /// The game data at hand do not know the part: the slot keeps the factory part.
    Unknown,
}

/// The part of a code as the receiver's game data know it: a class name is looked up, an identifier is taken
/// where the game data (`known`, or the class names) know it.
pub fn land(token: &str, ids: &PartIds, known: &dyn Fn(&str) -> bool) -> Landing {
    let token = token.trim();
    if token.is_empty() {
        return Landing::Empty;
    }
    if looks_like_guid(token) {
        let guid = token.to_lowercase();
        return if known(&guid) || ids.knows(&guid) { Landing::Part(guid) } else { Landing::Unknown };
    }
    match ids.guid_of(token) {
        Some(guid) => Landing::Part(guid.to_string()),
        None => Landing::Unknown,
    }
}

/// The slots of a code with what each comes to.
pub fn arrivals(shared: &Shared, ids: &PartIds, known: &dyn Fn(&str) -> bool) -> Vec<(String, String, Landing)> {
    shared.slots.iter().map(|(path, token)| (path.clone(), token.clone(), land(token, ids, known))).collect()
}

/// The equipment a code comes to: every slot whose part is known (or emptied), the paint where it is known. How a
/// part was got is not known (`unknown`); a slot of an unknown part is left out, so the factory part stays.
pub fn equipment_of(shared: &Shared, ids: &PartIds, known: &dyn Fn(&str) -> bool) -> Equipment {
    let mut equipment = Equipment { name: shared.name.clone(), ..Equipment::default() };
    for (path, _, landing) in arrivals(shared, ids, known) {
        match landing {
            Landing::Part(guid) => equipment.put(Slot::new(&path, &guid, Acquisition::Unknown)),
            Landing::Empty => equipment.put(Slot::new(&path, "", Acquisition::Unknown)),
            Landing::Unknown => {}
        }
    }
    equipment.paint_item_guid = shared.paint.as_deref().and_then(|token| match land(token, ids, known) {
        Landing::Part(guid) => Some(guid),
        _ => None,
    });
    equipment
}

#[cfg(test)]
mod tests {
    use super::*;

    const LASER: &str = "a1b2c3d4-0000-4000-8000-000000000001";
    const COOLER: &str = "a1b2c3d4-0000-4000-8000-000000000002";
    const MISSILE: &str = "a1b2c3d4-0000-4000-8000-000000000003";
    const PAINT: &str = "a1b2c3d4-0000-4000-8000-000000000004";

    fn ids() -> PartIds {
        PartIds::new([
            (LASER.to_string(), "KLWE_LaserRepeater_S3".to_string()),
            (COOLER.to_string(), "COOL_AEGS_S02_Bracer".to_string()),
            (PAINT.to_string(), "Paint_Gladius_Black".to_string()),
        ])
    }

    fn equipment() -> Equipment {
        let mut e = Equipment { name: "PvP".into(), active: true, ..Equipment::default() };
        e.put(Slot::new("hardpoint_weapon_left", LASER, Acquisition::Crafted));
        e.put(Slot::new("hardpoint_weapon_right", LASER, Acquisition::Purchased));
        e.put(Slot::new("Hardpoint_Cooler", COOLER, Acquisition::Pledge));
        // A part the class names do not know: it goes by its identifier.
        e.put(Slot::new("hardpoint_missile_rack/missile_01_attach", MISSILE, Acquisition::Looted));
        // A port emptied on purpose.
        e.put(Slot::new("hardpoint_tractor_beam", "", Acquisition::Unknown));
        e.paint_item_guid = Some(PAINT.into());
        e
    }

    #[test]
    fn a_code_there_and_back_gives_the_same_equipment() {
        let ids = ids();
        let shared = share(" Gladius ", "Aegis Dynamics", &equipment(), &ids);
        assert_eq!(shared.slots[0].1, "KLWE_LaserRepeater_S3", "a part goes by its class name");
        assert_eq!(shared.slots[3].1, MISSILE, "without a class name by its identifier");
        let code = encode(&shared);
        assert!(code.starts_with("VK1-"), "{code}");
        assert!(code[4..].chars().all(body_char), "a code is one word: {code}");
        let back = decode(&code).expect("own code is read");
        assert_eq!(back, shared);
        let known = |guid: &str| guid == MISSILE;
        let landed = equipment_of(&back, &ids, &known);
        let sent = equipment();
        assert_eq!(landed.name, sent.name);
        assert_eq!(landed.paint_item_guid, sent.paint_item_guid);
        let parts = |e: &Equipment| e.slots.iter().map(|s| (s.port_path.clone(), s.item_guid.clone())).collect::<Vec<_>>();
        assert_eq!(parts(&landed), parts(&sent), "the same parts in the same slots");
        // The way a part was got is the sender's own.
        assert!(landed.slots.iter().all(|s| s.acquisition == Acquisition::Unknown));
    }

    #[test]
    fn the_code_is_found_in_a_message_and_in_a_file_with_several() {
        let ids = ids();
        let one = encode(&share("Gladius", "", &equipment(), &ids));
        let mut other = equipment();
        other.name = "Rennen".into();
        let two = encode(&share("Gladius", "", &other, &ids));
        let message = format!("Hier meine Gladius: `{one}` — viel Spaß!");
        assert_eq!(decode(&message).map(|s| s.name), Ok("PvP".to_string()));
        let file = file_text(&[one.clone(), two.clone()]);
        let all: Vec<String> = read_all(&file).into_iter().map(|r| r.map(|s| s.name).unwrap_or_default()).collect();
        assert_eq!(all, ["PvP", "Rennen"]);
        // Counter-check: a code glued to a word is no code, and neither is a body that holds `VK1-` itself.
        assert_eq!(read_all(&format!("xx{one}")).len(), 0);
        assert_eq!(read_all(&format!("{one}\n{two}")).len(), 2);
    }

    #[test]
    fn a_broken_or_foreign_code_is_refused_with_its_reason_and_never_panics() {
        let code = encode(&share("Gladius", "", &equipment(), &ids()));
        assert_eq!(decode(""), Err(Refused::NoCode));
        assert_eq!(decode("Hallo, wie geht es?"), Err(Refused::NoCode));
        assert_eq!(decode("VK2-abc"), Err(Refused::Newer));
        assert_eq!(decode("VK0-abc"), Err(Refused::Damaged));
        // Cut off, a letter changed, junk, valid base64 of no deflate, deflate of no JSON, JSON of another shape.
        assert_eq!(decode(&code[..code.len() / 2]), Err(Refused::Damaged));
        let mut changed: Vec<char> = code.chars().collect();
        let at = changed.len() / 2;
        changed[at] = if changed[at] == 'A' { 'B' } else { 'A' };
        let changed: String = changed.into_iter().collect();
        assert!(decode(&changed).is_err() || decode(&changed) != decode(&code), "a changed code is not the same one");
        assert_eq!(decode("VK1-!!!"), Err(Refused::Damaged));
        assert_eq!(decode(&format!("VK1-{}", URL_SAFE_NO_PAD.encode(b"plain text"))), Err(Refused::Damaged));
        let packed = |text: &str| format!("VK1-{}", URL_SAFE_NO_PAD.encode(miniz_oxide::deflate::compress_to_vec(text.as_bytes(), 6)));
        assert_eq!(decode(&packed("no json")), Err(Refused::Damaged));
        assert_eq!(decode(&packed(r#"[1, 2]"#)), Err(Refused::Damaged));
        assert_eq!(decode(&packed(r#"{"s": "", "t": []}"#)), Err(Refused::Damaged), "no ship");
        assert_eq!(decode(&packed(r#"{"s": "Gladius"}"#)), Err(Refused::Damaged), "no slots");
        assert_eq!(decode(&packed(r#"{"s": "Gladius", "t": [["a"]]}"#)), Err(Refused::Damaged));
        assert_eq!(decode(&packed(r#"{"s": "Gladius", "t": [["", "x"]]}"#)), Err(Refused::Damaged), "no port");
        assert_eq!(decode(&packed(r#"{"s": "Gladius", "t": [["a b", "x"]]}"#)), Err(Refused::Damaged), "a space");
        assert_eq!(decode(&packed(r#"{"s": 5, "t": []}"#)), Err(Refused::Damaged));
        // A bomb that unpacks to much more than any equipment is refused, not unpacked.
        let bomb = format!(r#"{{"s": "Gladius", "n": "{}", "t": []}}"#, "x".repeat(MAX_UNPACKED * 2));
        assert_eq!(decode(&packed(&bomb)), Err(Refused::Damaged));
        // Also a bomb of many slots that each keep to the limits of a field and of the count: only the limit of
        // what is unpacked stops it.
        let many = |count: usize| {
            let slots: Vec<String> =
                (0..count).map(|i| format!(r#"["p{i:03}{}", "{}"]"#, "a".repeat(280), "x".repeat(290))).collect();
            packed(&format!(r#"{{"s": "Gladius", "t": [{}]}}"#, slots.join(", ")))
        };
        assert!(900 < MAX_SLOTS && many(900).len() < MAX_CODE, "the bomb keeps to every other limit");
        assert_eq!(decode(&many(900)), Err(Refused::Damaged));
        assert_eq!(decode(&many(50)).map(|s| s.slots.len()), Ok(50), "the same shape, small, is read");
        // Counter-check: the same shape, well formed, is read.
        assert!(decode(&packed(r#"{"s": "Gladius", "t": [["A", "x"]]}"#)).is_ok());
        // Every cut of a real code is refused or read, but never panics.
        for end in 0..code.len() {
            let _ = decode(&code[..end]);
        }
    }

    #[test]
    fn a_part_the_receiver_does_not_know_is_said_and_left_out() {
        let shared = share("Gladius", "", &equipment(), &ids());
        // The receiver's game data know the laser by its class name, nothing else.
        let theirs = PartIds::new([(LASER.to_string(), "KLWE_LaserRepeater_S3".to_string())]);
        let nothing = |_: &str| false;
        let landed = arrivals(&shared, &theirs, &nothing);
        let of = |path: &str| landed.iter().find(|(p, _, _)| p == path).map(|(_, _, l)| l.clone());
        assert_eq!(of("hardpoint_weapon_left"), Some(Landing::Part(LASER.into())));
        assert_eq!(of("hardpoint_cooler"), Some(Landing::Unknown), "an unknown class name");
        assert_eq!(of("hardpoint_missile_rack/missile_01_attach"), Some(Landing::Unknown), "an unknown identifier");
        assert_eq!(of("hardpoint_tractor_beam"), Some(Landing::Empty));
        let equipment = equipment_of(&shared, &theirs, &nothing);
        assert!(equipment.slot("hardpoint_cooler").is_none(), "the factory part stays");
        assert_eq!(equipment.paint_item_guid, None, "an unknown paint is left out");
        assert_eq!(equipment.slot("hardpoint_tractor_beam").map(|s| s.item_guid.as_str()), Some(""));
        // Counter-check: with the identifier known to the slots of the receiver, the missile comes.
        let known = |guid: &str| guid == MISSILE;
        assert!(equipment_of(&shared, &theirs, &known).slot("hardpoint_missile_rack/missile_01_attach").is_some());
    }

    /// A big ship with every slot changed: the code fits into one chat message.
    #[test]
    fn the_code_of_a_big_ship_fully_changed_fits_into_a_chat_message() {
        // 32 weapons, 8 turrets with 4 guns, 64 missiles in 8 racks, 28 torpedoes, 20 components: 176 slots, each
        // with a part other than at the factory, from 30 kinds of part (as a big ship is fitted).
        let mut pairs = Vec::new();
        let mut e = Equipment { name: "Volle Breitseite".into(), active: true, ..Equipment::default() };
        let guid = |n: usize| format!("{n:08x}-1234-4abc-8def-{:012x}", n * 7919);
        for n in 0..30 {
            pairs.push((guid(n), format!("PART_Maker_Kind_S{}_Variant{n}", n % 10)));
        }
        let mut put = |path: String, n: usize| e.put(Slot::new(&path, &guid(n % 30), Acquisition::Purchased));
        for i in 0..32 {
            put(format!("hardpoint_weapon_gun_class_{}_{i:02}", i % 4), i % 5);
        }
        for t in 0..8 {
            for g in 0..4 {
                put(format!("hardpoint_turret_remote_{t:02}/hardpoint_weapon_{g:02}"), 5 + t % 3);
            }
        }
        for r in 0..8 {
            for m in 0..8 {
                put(format!("hardpoint_missile_rack_{r:02}/missile_{m:02}_attach"), 10 + r % 2);
            }
        }
        for t in 0..28 {
            put(format!("hardpoint_torpedo_rack_left/missile_{t:02}_attach"), 12);
        }
        for c in 0..20 {
            put(format!("hardpoint_component_{}_{c:02}", ["cooler", "power_plant", "shield"][c % 3]), 13 + c);
        }
        assert_eq!(e.slots.len(), 176);
        let with_names = encode(&share("Polaris", "Roberts Space Industries", &e, &PartIds::new(pairs)));
        assert!(with_names.len() < CHAT_LIMIT, "{} characters with class names", with_names.len());
        // The worst case: no class names at hand, every part by its identifier.
        let without = encode(&share("Polaris", "Roberts Space Industries", &e, &PartIds::default()));
        assert!(without.len() < CHAT_LIMIT, "{} characters with identifiers", without.len());
        // The class names are what makes the code short where they are known.
        assert!(with_names.len() < without.len(), "{} with class names, {} without", with_names.len(), without.len());
        // Counter-check: the limit is no wall the test cannot reach: 176 slots each with its own part do not fit.
        let mut every = e.clone();
        for (n, slot) in every.slots.iter_mut().enumerate() {
            slot.item_guid = guid(1000 + n);
        }
        let apart = encode(&share("Polaris", "", &every, &PartIds::default()));
        assert!(apart.len() > CHAT_LIMIT, "{} characters", apart.len());
    }

    /// The length of the code for the biggest ship of the stored erkul data, every slot with another part that
    /// fits it: `VERSEKIT_MESSORDNER=<folder with erkul-schiffe.json, erkul-teile.json, werte-spieldaten.json>
    /// cargo test --release -p versekit-core code_laenge -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn code_laenge() {
        use crate::erkul_slots::{ErkulParts, hardpoints};
        use crate::ships::{ErkulShips, ShipId};
        let Ok(folder) = std::env::var("VERSEKIT_MESSORDNER") else { return };
        let file = |name: &str| format!("{folder}/{name}");
        let ships = ErkulShips::load(&file("erkul-schiffe.json"));
        let parts = ErkulParts::load(&file("erkul-teile.json"));
        let values: Json = std::fs::read(file("werte-spieldaten.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or(Json::Null);
        let pairs: Vec<(String, String)> = values["values"]["items"]
            .as_object()
            .map(|items| {
                items
                    .values()
                    .map(|i| (i["guid"].as_str().unwrap_or("").to_string(), i["class_name"].as_str().unwrap_or("").to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let ids = PartIds::new(pairs);
        let raw: Json = std::fs::read(file("erkul-schiffe.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Json::Null);
        let mut longest: Vec<(usize, String)> = Vec::new();
        for (key, ship) in raw["schiffe"].as_object().into_iter().flatten() {
            let name = ship["name"].as_str().unwrap_or(key).to_string();
            let maker = ship["hersteller"].as_str().unwrap_or("").to_string();
            let id = ShipId { name: &name, maker: &maker, short: key, maker_short: "" };
            let slots = hardpoints(&ships, &parts, id, &crate::jsonio::Map::new());
            let mut e = Equipment { name: "Alles anders".into(), ..Equipment::default() };
            for slot in &slots {
                let factory = slot.factory.as_ref().map(|(r, _)| r.as_str()).unwrap_or("");
                let other = parts.compatible(slot).into_iter().find(|p| p.reference != factory);
                if let Some(other) = other {
                    e.put(Slot::new(&slot.path, &other.reference, Acquisition::Purchased));
                }
            }
            let code = encode(&share(&name, &maker, &e, &ids));
            longest.push((code.len(), format!("{name}: {} Plätze geändert", e.slots.len())));
        }
        longest.sort();
        for (length, what) in longest.iter().rev().take(8) {
            println!("{length} Zeichen  {what}");
        }
        println!("{} Schiffe", longest.len());
    }

    /// Every ship of the stored erkul slots, every slot changed: a part of erkul that fits where there is one,
    /// else a part of the kind and size, and the first slot of the stock left empty. The code carries all of it,
    /// nested slots too (turret → gun, head → module, rack → missile), and reads back slot for slot. On real
    /// data: `VERSEKIT_MESSORDNER=<data folder> cargo test --release -p versekit-core rundlauf_alle_schiffe --
    /// --ignored --nocapture`.
    #[test]
    #[ignore]
    fn rundlauf_alle_schiffe() {
        use crate::erkul_slots::{ErkulParts, hardpoints};
        use crate::ships::{ErkulShips, ShipId};
        use crate::erkul_slots::{GAME_PARTS_FILE, GAME_SHIPS_FILE};
        let Ok(folder) = std::env::var("VERSEKIT_MESSORDNER") else { return };
        let file = |name: &str| format!("{folder}/{name}");
        // As the program reads them: the slots of the game data, erkul's where those have nothing.
        let ships = ErkulShips::merged(ErkulShips::load(&file(GAME_SHIPS_FILE)), ErkulShips::load(&file("erkul-schiffe.json")));
        let parts = ErkulParts::merged(ErkulParts::load(&file(GAME_PARTS_FILE)), ErkulParts::load(&file("erkul-teile.json")));
        let listed = if std::path::Path::new(&file(GAME_SHIPS_FILE)).exists() { GAME_SHIPS_FILE } else { "erkul-schiffe.json" };
        println!("Schiffe aus {listed}");
        let raw: Json = std::fs::read(file(listed)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Json::Null);
        let made_up = |kind: &str, size: Option<i64>| {
            let seed: u64 = kind.bytes().fold(size.unwrap_or(0) as u64, |h, b| h.wrapping_mul(31).wrapping_add(u64::from(b)));
            format!("{:08x}-0000-4000-8000-{:012x}", seed & 0xffff_ffff, seed & 0xffff_ffff_ffff)
        };
        let (mut count, mut over, mut nested, mut longest) = (0, Vec::new(), 0, (0, String::new()));
        for (key, ship) in raw["schiffe"].as_object().into_iter().flatten() {
            let name = ship["name"].as_str().unwrap_or(key).to_string();
            let id = ShipId { name: &name, maker: "", short: key, maker_short: "" };
            let slots = hardpoints(&ships, &parts, id, &crate::jsonio::Map::new());
            if slots.is_empty() {
                // A vehicle without a slot to change (a ground vehicle with everything built in) has no equipment.
                continue;
            }
            let mut e = Equipment { name: "Alles anders".into(), active: true, ..Equipment::default() };
            let mut emptied = false;
            for slot in &slots {
                let factory = slot.factory.as_ref().map(|(r, _)| r.as_str()).unwrap_or("");
                nested += usize::from(slot.path.contains('/'));
                let guid = if !emptied && !factory.is_empty() {
                    emptied = true;
                    String::new()
                } else {
                    let other = parts.compatible(slot).into_iter().find(|p| p.reference != factory);
                    other.map_or_else(|| made_up(&slot.kind, slot.size), |p| p.reference.clone())
                };
                e.put(Slot::new(&slot.path, &guid, Acquisition::Purchased));
            }
            let shared = share(&name, "", &e, &PartIds::default());
            assert_eq!(shared.slots.len(), slots.len(), "{name}: every slot goes into the code");
            let code = encode(&shared);
            let back = decode(&code).unwrap_or_else(|why| panic!("{name}: {why:?}"));
            let came = equipment_of(&back, &PartIds::default(), &|_: &str| true);
            let pairs = |e: &Equipment| {
                let mut all: Vec<(String, String)> = e.slots.iter().map(|s| (s.port_path.clone(), s.item_guid.clone())).collect();
                all.sort();
                all
            };
            assert_eq!(pairs(&came), pairs(&e), "{name}: slot for slot the same");
            count += 1;
            if code.len() > CHAT_LIMIT {
                over.push(format!("{name} ({} Zeichen, {} Plätze)", code.len(), slots.len()));
            }
            if code.len() > longest.0 {
                longest = (code.len(), format!("{name}, {} Plätze", slots.len()));
            }
        }
        println!("{count} Schiffe im Rundlauf gleich, {nested} verschachtelte Plätze dabei");
        println!("längster Code: {} Zeichen ({})", longest.0, longest.1);
        println!("über {CHAT_LIMIT} Zeichen: {}", if over.is_empty() { "keins".to_string() } else { over.join(", ") });
        assert!(count > 0, "no ship read");
    }
}
