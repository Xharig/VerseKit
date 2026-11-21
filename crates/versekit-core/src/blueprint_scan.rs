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

//! The name of a blueprint read at the crafting terminal and matched against the catalog.
//!
//! The terminal lists its blueprints as pictures; the name stands only in the hint the game shows beside the
//! mouse pointer (`Sedulity (Ind/2/B)`). So a reading takes a part of the screen around the pointer, reads it
//! several times ([`PASSES`]) and matches every line of every reading against the names of the catalog
//! ([`Names`]): only a name of the catalog can come out, nothing is learned freely.
//!
//! A line is matched in steps ([`match_line`]): the same comparison form, the same form after folding characters
//! the recognition mistakes for one another, a catalog name with the tag of the item behind it, and last a close
//! name by the edit distance. The tag behind the name (`(Ind/2/B)`) is checked against class, size and grade of
//! the catalog: a tag that says otherwise turns a hit into a question.
//!
//! ⚠ Nothing unsure is taken silently. Only a name read the same and sure in at least [`MIN_AGREE`] readings,
//! with no reading for another name, is [`Certainty::Sure`]; everything else goes to the player to confirm
//! ([`Certainty::Likely`], [`Certainty::Unclear`]) or is dropped ([`Certainty::None`]). Nothing here writes into the
//! collection: [`decide`] says what to do, the caller does it.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::catalog_cache::{Blueprint, CatalogCache, grade_letter};
use crate::language::{self, Arg, Lang};
use crate::refinery_scan::{Area, Job, Mode, Ocr, ScanError, Word, rows};

/// The most a close name may differ, as a share of the longer comparison form.
pub const MAX_ERROR: f64 = 0.15;

/// How far the best close name must lie ahead of the next one of another blueprint, in shares of one.
pub const MIN_LEAD: f64 = 0.05;

/// The fewest characters a comparison form must have to be matched by closeness; shorter names count only exactly.
pub const MIN_CLOSE_LEN: usize = 6;

/// The fewest readings that must read the same name sure before it is taken without asking.
pub const MIN_AGREE: usize = 2;

/// The readings of the part around the pointer: enlargement and preparation. The hint is bright writing on a dark
/// ground, as the cells of the refinery terminal.
pub const PASSES: [(f64, Mode); 3] = [(2.5, Mode::Plain), (3.5, Mode::Plain), (3.0, Mode::Dark)];

/// How sure a reading is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Certainty {
    /// Nothing of the catalog was read.
    None,
    /// Several blueprints come into question.
    Unclear,
    /// One blueprint, but not read sure enough to take it without asking.
    Likely,
    /// One blueprint, read sure.
    Sure,
}

/// How a line found its blueprint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum How {
    /// The same comparison form as a name of the catalog.
    Exact,
    /// The same after folding the characters the recognition mistakes for one another.
    Lookalike,
    /// A name of the catalog with a tag or a remainder behind it.
    WithTag,
    /// A close name by the edit distance.
    Close,
    /// No name of the catalog.
    Unknown,
}

/// What the tag behind a name says of a blueprint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TagCheck {
    /// No tag read, or one that cannot be compared.
    Unread,
    /// Every part read agrees with the catalog.
    Agrees,
    /// A part read says otherwise than the catalog.
    Contradicts,
}

/// One blueprint of the comparison set: the catalog name and the parts of its tag.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    /// The class as the tag writes it (`Ind`), size and grade letter, where the catalog has them.
    pub class: Option<String>,
    pub size: Option<i64>,
    pub grade: Option<char>,
}

/// One spelling a blueprint can be read as: its comparison form folded ([`fold_lookalikes`]) and the blueprint.
#[derive(Clone, Debug)]
struct Form {
    folded: String,
    entry: usize,
}

/// The comparison set: every blueprint of the catalog under its name and the names it carries in the language of
/// the game.
#[derive(Clone, Debug, Default)]
pub struct Names {
    entries: Vec<Entry>,
    forms: Vec<Form>,
    by_plain: HashMap<String, Vec<usize>>,
    by_folded: HashMap<String, Vec<usize>>,
    by_name: HashMap<String, usize>,
    /// The folded forms with their blueprints, sorted, for [`Names::longer_than`]; computed once.
    longer: OnceLock<Vec<(String, usize)>>,
}

impl Names {
    /// An empty set.
    pub fn new() -> Names {
        Names::default()
    }

    /// The set of all blueprints of `catalog`, under their catalog names.
    pub fn of_catalog(catalog: &CatalogCache) -> Names {
        let mut names = Names::new();
        for blueprint in &catalog.blueprints {
            names.add(entry_of(blueprint));
        }
        names
    }

    /// Adds a blueprint under its name; a name already there is not added twice. Its index.
    pub fn add(&mut self, entry: Entry) -> usize {
        if let Some(&at) = self.by_name.get(&entry.name) {
            return at;
        }
        let at = self.entries.len();
        let name = entry.name.clone();
        self.by_name.insert(name.clone(), at);
        self.entries.push(entry);
        self.add_form(&name, at);
        at
    }

    /// Adds `text` as a further spelling of the blueprint named `name` (its name in the language of the game).
    /// `false` when the catalog has no such blueprint.
    pub fn add_spelling(&mut self, text: &str, name: &str) -> bool {
        let Some(&at) = self.by_name.get(name) else { return false };
        let text = crate::injection::specs::strip_tag(text);
        self.add_form(&text, at);
        true
    }

    /// Adds the names of the language of the game: pairs (translated name, English name), see
    /// [`translation_pairs`]. How many were taken.
    pub fn add_translations(&mut self, pairs: &[(String, String)]) -> usize {
        pairs.iter().filter(|(translated, english)| self.add_spelling(translated, english)).count()
    }

    fn add_form(&mut self, text: &str, entry: usize) {
        let plain = plain(text);
        if plain.is_empty() {
            return;
        }
        self.longer = OnceLock::new();
        let folded = fold_lookalikes(&plain);
        let list = self.by_plain.entry(plain).or_default();
        if list.iter().any(|&f| self.forms[f].entry == entry) {
            return;
        }
        let at = self.forms.len();
        list.push(at);
        self.by_folded.entry(folded.clone()).or_default().push(at);
        self.forms.push(Form { folded, entry });
    }

    /// How many blueprints the set holds.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The blueprint of a catalog name.
    pub fn entry(&self, name: &str) -> Option<&Entry> {
        self.by_name.get(name).map(|&at| &self.entries[at])
    }

    /// The catalog names of all blueprints, in the order they were added.
    pub fn all(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|e| e.name.as_str())
    }

    /// The blueprints other than `entry` with a spelling that begins with the folded form `folded` and goes on after
    /// a blank: what `folded` would be with its last words lost.
    pub fn longer_than(&self, folded: &str, entry: usize) -> Vec<usize> {
        let sorted = self.longer.get_or_init(|| {
            let mut sorted: Vec<(String, usize)> = self.forms.iter().map(|f| (f.folded.clone(), f.entry)).collect();
            sorted.sort_unstable();
            sorted
        });
        let start = format!("{folded} ");
        let from = sorted.partition_point(|(text, _)| text.as_str() < start.as_str());
        let mut out: Vec<usize> = Vec::new();
        for (text, other) in &sorted[from..] {
            if !text.starts_with(&start) {
                break;
            }
            if *other != entry && !out.contains(other) {
                out.push(*other);
            }
        }
        out.sort_by(|a, b| self.entries[*a].name.cmp(&self.entries[*b].name));
        out
    }

    /// The different blueprints of some forms, each once, in their order.
    fn entries_of(&self, forms: &[usize]) -> Vec<usize> {
        let mut out: Vec<usize> = Vec::new();
        for &f in forms {
            let entry = self.forms[f].entry;
            if !out.contains(&entry) {
                out.push(entry);
            }
        }
        out
    }
}

/// The entry of a blueprint of the catalog: its name and the parts of its tag.
pub fn entry_of(blueprint: &Blueprint) -> Entry {
    // Personal weapons are kept as `Energy` or `Ballistic`; the hint names their kind (`(Las)`, `(Bal)`).
    let broad = blueprint.class.as_deref().and_then(|c| match c.to_lowercase().as_str() {
        "energy" => Some(ENERGY.to_string()),
        "ballistic" => Some("Bal".to_string()),
        _ => None,
    });
    Entry {
        name: blueprint.name.clone(),
        class: crate::injection::specs::class_short(blueprint.class.as_deref()).map(str::to_string).or(broad),
        size: blueprint.size,
        grade: blueprint.grade.and_then(grade_letter),
    }
}

/// Pairs (translated name, English name) of the item names two `global.ini` texts share by key: the English one
/// of the game and the one of the language it runs in. A translated name that equals the English one is left out.
pub fn translation_pairs(english_ini: &str, translated_ini: &str) -> Vec<(String, String)> {
    let english = crate::phrases::item_texts(english_ini);
    let translated = crate::phrases::item_texts(translated_ini);
    let mut pairs: Vec<(String, String)> = translated
        .into_iter()
        .filter_map(|(key, text)| {
            let original = english.get(&key)?;
            (!text.is_empty() && text != *original).then(|| (text, original.clone()))
        })
        .collect();
    pairs.sort();
    pairs
}

// ------------------------------------------------------------ Comparison forms

/// A letter with its accent taken off, or the letters it stands for.
fn plain_letter(c: char) -> Option<&'static str> {
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'å' | 'ā' | 'ă' | 'ą' => "a",
        'ä' | 'æ' => "ae",
        'ç' | 'ć' | 'č' => "c",
        'ď' | 'đ' => "d",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => "e",
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' | 'ı' => "i",
        'ł' | 'ľ' | 'ĺ' => "l",
        'ñ' | 'ń' | 'ň' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ō' | 'ő' => "o",
        'ö' | 'ø' | 'œ' => "oe",
        'ř' | 'ŕ' => "r",
        'ś' | 'š' | 'ş' => "s",
        'ß' => "ss",
        'ť' | 'ţ' => "t",
        'ù' | 'ú' | 'û' | 'ū' | 'ů' | 'ű' | 'ų' => "u",
        'ü' => "ue",
        'ý' | 'ÿ' => "y",
        'ź' | 'ż' | 'ž' => "z",
        _ => return None,
    })
}

/// The comparison form of a text: lower case, accents off, every other sign a blank, one blank between words.
/// The quotes of a name (`7CA 'Nargun'`) fall away with the other signs.
pub fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        if let Some(letters) = plain_letter(c) {
            out.push_str(letters);
        } else if c.is_alphanumeric() {
            out.push(c);
        } else {
            out.push(' ');
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Characters the recognition mistakes for one another, each group written as its first one.
fn lookalike(c: char) -> char {
    match c {
        'l' | '1' | 'i' | '!' | '|' => 'i',
        '0' | 'o' => 'o',
        '5' | 's' => 's',
        '8' | 'b' => 'b',
        '2' | 'z' => 'z',
        other => other,
    }
}

/// A comparison form with the mistakable characters folded together: `rn` as `m`, `vv` as `w`, `cl` as `d`, then
/// each group of [`lookalike`] as one character.
pub fn fold_lookalikes(plain: &str) -> String {
    let joined = plain.replace("rn", "m").replace("vv", "w").replace("cl", "d");
    joined.chars().map(lookalike).collect()
}

/// The edit distance of two texts, by characters.
pub fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let swap = previous[j] + usize::from(ca != cb);
            current[j + 1] = swap.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

/// How alike two texts are: 1 less the edit distance as a share of the longer one.
pub fn likeness(a: &str, b: &str) -> f64 {
    let longer = a.chars().count().max(b.chars().count());
    if longer == 0 {
        return 1.0;
    }
    1.0 - edit_distance(a, b) as f64 / longer as f64
}

// ------------------------------------------------------------ The tag

/// A tag behind a name as read: class, size, grade; each part only where it could be read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tag {
    pub class: Option<String>,
    pub size: Option<i64>,
    pub grade: Option<char>,
}

/// Splits a line into the name and the tag in brackets at its end (`Sedulity (Ind/2/B)`). `None` for the tag when
/// the line ends without one in the form class/size/grade.
pub fn split_tag(line: &str) -> (String, Option<Tag>) {
    let trimmed = line.trim_end().trim_end_matches(|c: char| !c.is_alphanumeric() && c != ')');
    let Some(open) = trimmed.rfind('(') else { return without_brackets(line) };
    let inside = trimmed[open + 1..].trim_end_matches(')');
    let parts: Vec<&str> = inside.split(['/', '\\']).map(str::trim).collect();
    if parts.len() == 1 {
        // A weapon carries only its kind (`(Bal)`), a missile its seeker and size (`(IR2)`).
        let letters: String = inside.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
        let rest = &inside[letters.len()..];
        let seeker = ["IR", "EM", "CS"].contains(&letters.as_str());
        let fits = (2..=4).contains(&letters.len()) && (rest.is_empty() || (seeker && size_of(rest).is_some()));
        if !fits {
            return (line.trim().to_string(), None);
        }
        let tag = Tag { class: Some(letters), size: size_of(rest), grade: None };
        return (trimmed[..open].trim().to_string(), Some(tag));
    }
    if parts.len() != 3 {
        return (line.trim().to_string(), None);
    }
    let class = Some(parts[0])
        .filter(|p| (2..=4).contains(&p.chars().count()) && p.chars().all(char::is_alphabetic))
        .map(str::to_string);
    let size = size_of(parts[1]);
    let grade = grade_of(parts[2]);
    let tag = Tag { class, size, grade };
    (trimmed[..open].trim().to_string(), Some(tag))
}

/// A tag whose brackets the recognition lost (`PowerBolt tiv/1/O`): the last word with two slashes in it, of a
/// class, a size and a grade. The line as it is where there is none.
fn without_brackets(line: &str) -> (String, Option<Tag>) {
    let trimmed = line.trim();
    let Some((base, last)) = trimmed.rsplit_once(' ') else { return (trimmed.to_string(), None) };
    let parts: Vec<&str> = last.split('/').collect();
    if parts.len() != 3 || !(2..=4).contains(&parts[0].chars().count()) || !parts[0].chars().all(char::is_alphabetic) {
        return (trimmed.to_string(), None);
    }
    let tag = Tag { class: Some(parts[0].to_string()), size: size_of(parts[1]), grade: grade_of(parts[2]) };
    (base.trim().to_string(), Some(tag))
}

/// A size as the tag writes it; the recognition may read the 1 as `l` or `I`, the 2 as `Z`.
fn size_of(part: &str) -> Option<i64> {
    let digits: String = part
        .chars()
        .map(|c| match c {
            'l' | 'I' | 'i' | '|' => '1',
            'Z' | 'z' => '2',
            'S' | 's' => '5',
            'O' | 'o' => '0',
            other => other,
        })
        .collect();
    (!digits.is_empty() && digits.len() <= 2 && digits.chars().all(|c| c.is_ascii_digit()))
        .then(|| digits.parse().ok())
        .flatten()
}

/// A grade letter as the tag writes it; an 8 is a B read wrong.
fn grade_of(part: &str) -> Option<char> {
    let mut chars = part.chars();
    let first = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    match first.to_ascii_uppercase() {
        '8' => Some('B'),
        c @ 'A'..='D' => Some(c),
        _ => None,
    }
}

/// The classes a tag names (`injection::specs`), for a class read with a fault.
const CLASSES: [&str; 18] = [
    "Civ", "Mil", "Ind", "Sth", "Cmp", "Las", "Ele", "Pla", "Dis", "Mic", "Bal", "Nah", "Min", "Slv", "Med", "IR",
    "EM", "CS",
];

/// The class a tag means: the one of [`CLASSES`] it is, else the only one a single character away (`Bat` is `Bal`,
/// `tiv` is `Civ`); as read where none or several are.
fn class_meant(read: &str) -> String {
    let low = read.to_lowercase();
    if let Some(class) = CLASSES.iter().find(|c| c.to_lowercase() == low) {
        return (*class).to_string();
    }
    let near: Vec<&str> = CLASSES
        .iter()
        .copied()
        .filter(|c| c.len() == low.chars().count() && edit_distance(&c.to_lowercase(), &low) == 1)
        .collect();
    if near.len() == 1 { near[0].to_string() } else { read.to_string() }
}

/// The class of a personal weapon the catalog keeps only as energy.
const ENERGY: &str = "Energy";
/// The kinds a hint names for an energy weapon.
const ENERGY_KINDS: [&str; 4] = ["Las", "Ele", "Pla", "Dis"];

/// What a tag read says of a blueprint: every part read must agree with the catalog, and at least two must have
/// been compared for it to count as agreeing. A part read that the blueprint has not (a class behind a magazine, a
/// size where the catalog knows none) says it is another one.
pub fn check_tag(tag: Option<&Tag>, entry: &Entry) -> TagCheck {
    let Some(tag) = tag else { return TagCheck::Unread };
    let mut compared = 0;
    match (&tag.class, &entry.class) {
        (Some(read), Some(known)) => {
            let meant = class_meant(read);
            let energy = known == ENERGY && ENERGY_KINDS.iter().any(|kind| meant.eq_ignore_ascii_case(kind));
            if !energy && !meant.eq_ignore_ascii_case(known) {
                return TagCheck::Contradicts;
            }
            compared += 1;
        }
        (Some(_), None) => return TagCheck::Contradicts,
        _ => {}
    }
    // Size 0 in a tag is what the catalog keeps as none.
    match (tag.size, entry.size) {
        (Some(read), Some(known)) => {
            if read != known {
                return TagCheck::Contradicts;
            }
            compared += 1;
        }
        (Some(0), None) => compared += 1,
        (Some(_), None) => return TagCheck::Contradicts,
        _ => {}
    }
    match (tag.grade, entry.grade) {
        (Some(read), Some(known)) => {
            if read != known {
                return TagCheck::Contradicts;
            }
            compared += 1;
        }
        (Some(_), None) => return TagCheck::Contradicts,
        _ => {}
    }
    if compared >= 2 { TagCheck::Agrees } else { TagCheck::Unread }
}

// ------------------------------------------------------------ One line

/// A line matched against the catalog.
#[derive(Clone, Debug, PartialEq)]
pub struct Reading {
    /// The line as it was read.
    pub text: String,
    pub certainty: Certainty,
    pub how: How,
    /// The blueprint, where there is one.
    pub name: Option<String>,
    /// The blueprints in question, best first, with their likeness.
    pub candidates: Vec<(String, f64)>,
    pub tag: TagCheck,
}

impl Reading {
    fn none(text: &str) -> Reading {
        Reading {
            text: text.to_string(),
            certainty: Certainty::None,
            how: How::Unknown,
            name: None,
            candidates: Vec::new(),
            tag: TagCheck::Unread,
        }
    }
}

/// Drops leading words of one character: the recognition reads a symbol in front of a name as a letter.
fn without_stray_front(plain: &str) -> String {
    let mut words: Vec<&str> = plain.split(' ').collect();
    while words.len() > 1 && words[0].chars().count() == 1 {
        words.remove(0);
    }
    words.join(" ")
}

/// Turns the blueprints a form led to into a reading: one blueprint is a hit of `certainty`, several are
/// unclear unless the tag picks one of them. A tag that contradicts the hit makes it a question. `key` is the
/// folded form that was read, to find the longer names it may be the beginning of.
fn settle(
    text: &str,
    names: &Names,
    found: &[usize],
    (how, certainty): (How, Certainty),
    tag: Option<&Tag>,
    key: Option<&str>,
) -> Reading {
    let mut entries = found.to_vec();
    if entries.len() > 1 {
        let agreeing: Vec<usize> =
            entries.iter().copied().filter(|&e| check_tag(tag, &names.entries[e]) == TagCheck::Agrees).collect();
        if agreeing.len() == 1 {
            entries = agreeing;
        }
    }
    let candidates: Vec<(String, f64)> = entries.iter().map(|&e| (names.entries[e].name.clone(), 1.0)).collect();
    if entries.len() != 1 {
        return Reading {
            text: text.to_string(),
            certainty: Certainty::Unclear,
            how,
            name: None,
            candidates,
            tag: TagCheck::Unread,
        };
    }
    let entry = &names.entries[entries[0]];
    let check = check_tag(tag, entry);
    let mut certainty = if check == TagCheck::Contradicts { certainty.min(Certainty::Likely) } else { certainty };
    let mut candidates = candidates;
    // A name that is the beginning of longer ones (`Antium Helmet`, `Antium Helmet Jet`) may be a longer one whose
    // last word was lost: sure only when a tag was read that leaves it and says otherwise for every longer one (the
    // tag stands behind the whole name and is the one of the item the hint is about).
    let longer = key.map(|key| names.longer_than(key, entries[0])).unwrap_or_default();
    if certainty == Certainty::Sure && !longer.is_empty() {
        let decided = check != TagCheck::Contradicts
            && longer.iter().all(|&l| check_tag(tag, &names.entries[l]) == TagCheck::Contradicts);
        if !decided {
            certainty = Certainty::Likely;
            candidates.extend(longer.iter().map(|&l| (names.entries[l].name.clone(), 1.0)));
        }
    }
    Reading { text: text.to_string(), certainty, how, name: Some(entry.name.clone()), candidates, tag: check }
}

/// Matches one line of the recognition against the catalog, in steps; the first step with a hit decides.
///
/// 1. The comparison form of the whole line, then of the line without its tag.
/// 2. The same with the mistakable characters folded together.
/// 3. The longest name the line begins with, the rest being a tag or what is left of one.
/// 4. The closest name, at most [`MAX_ERROR`] off and [`MIN_LEAD`] ahead of the next blueprint.
///
/// Steps 1 and 2 are sure, except for a name that is the beginning of longer ones ([`Names::longer_than`]) and is
/// not told apart from them by the tag, and for a line in capitals ([`in_capitals`]); steps 3 and 4 are never sure.
pub fn match_line(line: &str, names: &Names) -> Reading {
    let reading = match_words(line, names);
    let (base, tag) = split_tag(line);
    if reading.certainty == Certainty::Sure && tag.is_none() && in_capitals(&base) {
        return Reading { certainty: Certainty::Likely, ..reading };
    }
    reading
}

/// Whether every letter of `text` is a capital, with at least three letters. The terminal writes its headings so
/// (`ARMOR`, `VEHIKEL`), and so stand labels in the world; the hint with the name of a blueprint is written in
/// small letters with capitals, or carries a tag (`FS-9 LMG (Bal)`). A heading that happens to be the name of a
/// blueprint must not count as its hint, so a line in capitals without a tag is never sure.
pub fn in_capitals(text: &str) -> bool {
    let letters: Vec<char> = text.chars().filter(|c| c.is_alphabetic()).collect();
    letters.len() >= 3 && letters.iter().all(|c| c.is_uppercase())
}

/// The steps of [`match_line`], without its rule for capitals.
fn match_words(line: &str, names: &Names) -> Reading {
    let (base, tag) = split_tag(line);
    let (whole, short) = (plain(line), plain(&base));
    if short.chars().count() < 3 {
        return Reading::none(line);
    }
    // First the line as read, then without single characters in front (`G-2 Helmet` begins with two of them).
    let mut tries: Vec<String> = Vec::new();
    for form in [whole.clone(), short.clone(), without_stray_front(&whole), without_stray_front(&short)] {
        if form.chars().count() >= 3 && !tries.contains(&form) {
            tries.push(form);
        }
    }
    // A form that holds the brackets (`Singe Cannon (S1)`) found a name whose brackets are its own: they are no tag.
    let tag_of = |form: &String| if *form != short && form.starts_with(short.as_str()) { None } else { tag.as_ref() };
    for form in &tries {
        if let Some(found) = names.by_plain.get(form) {
            let key = fold_lookalikes(form);
            let found = names.entries_of(found);
            return settle(line, names, &found, (How::Exact, Certainty::Sure), tag_of(form), Some(&key));
        }
    }
    for form in &tries {
        let key = fold_lookalikes(form);
        if let Some(found) = names.by_folded.get(&key) {
            let found = names.entries_of(found);
            return settle(line, names, &found, (How::Lookalike, Certainty::Sure), tag_of(form), Some(&key));
        }
    }
    let front = match_front(line, &whole, names, tag.as_ref())
        .or_else(|| match_front(line, &without_stray_front(&whole), names, tag.as_ref()));
    // A name read with a fault in its last word (`Antium Helmet Jot`) begins with a shorter name of the catalog
    // (`Antium Helmet`): the closest whole name is asked for beside it, never the shorter one alone.
    let close = match match_close(line, &short, names, tag.as_ref()) {
        none if none.certainty == Certainty::None => {
            match_close(line, &without_stray_front(&short), names, tag.as_ref())
        }
        found => found,
    };
    match (front, close.certainty) {
        (None, _) => close,
        (Some(front), Certainty::None) => front,
        (Some(front), _) => {
            let mut candidates = close.candidates.clone();
            for candidate in front.candidates {
                if !candidates.iter().any(|(name, _)| *name == candidate.0) {
                    candidates.push(candidate);
                }
            }
            let agree = candidates.len() == 1 && close.name.is_some();
            Reading {
                certainty: if agree { Certainty::Likely } else { Certainty::Unclear },
                name: if agree { close.name.clone() } else { None },
                candidates,
                ..close
            }
        }
    }
}

/// Step 3: the longest name (folded) the line begins with, word for word. Sure only when a tag stands behind it
/// that agrees; a rest of more than three words is no tag.
fn match_front(line: &str, whole: &str, names: &Names, tag: Option<&Tag>) -> Option<Reading> {
    let folded = fold_lookalikes(whole);
    let mut best: Option<(usize, Vec<usize>)> = None;
    for (at, form) in names.forms.iter().enumerate() {
        let length = form.folded.len();
        if length >= folded.len() || !folded.starts_with(&form.folded) || folded.as_bytes()[length] != b' ' {
            continue;
        }
        match &mut best {
            Some((longest, list)) if *longest == length => list.push(at),
            Some((longest, _)) if *longest > length => {}
            _ => best = Some((length, vec![at])),
        }
    }
    let (length, forms) = best?;
    let rest = &folded[length + 1..];
    if rest.split(' ').count() > 3 {
        return None;
    }
    let found = names.entries_of(&forms);
    Some(settle(line, names, &found, (How::WithTag, Certainty::Likely), tag, None))
}

/// Step 4: the closest names by the edit distance of the folded forms.
fn match_close(line: &str, short: &str, names: &Names, tag: Option<&Tag>) -> Reading {
    let folded = fold_lookalikes(short);
    if folded.chars().count() < MIN_CLOSE_LEN {
        return Reading::none(line);
    }
    let mut best: HashMap<usize, f64> = HashMap::new();
    for form in &names.forms {
        if form.folded.chars().count() < MIN_CLOSE_LEN {
            continue;
        }
        let score = likeness(&folded, &form.folded);
        let slot = best.entry(form.entry).or_insert(0.0);
        *slot = slot.max(score);
    }
    let mut ranked: Vec<(usize, f64)> = best.into_iter().filter(|(_, s)| *s >= 1.0 - MAX_ERROR).collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| names.entries[a.0].name.cmp(&names.entries[b.0].name)));
    let Some(&(first, score)) = ranked.first() else { return Reading::none(line) };
    let candidates: Vec<(String, f64)> =
        ranked.iter().take(3).map(|(e, s)| (names.entries[*e].name.clone(), *s)).collect();
    let lead = ranked.get(1).map_or(1.0, |(_, s)| score - s);
    if lead < MIN_LEAD {
        return Reading {
            text: line.to_string(),
            certainty: Certainty::Unclear,
            how: How::Close,
            name: None,
            candidates,
            tag: TagCheck::Unread,
        };
    }
    let entry = &names.entries[first];
    Reading {
        text: line.to_string(),
        certainty: Certainty::Likely,
        how: How::Close,
        name: Some(entry.name.clone()),
        candidates,
        tag: check_tag(tag, entry),
    }
}

// ------------------------------------------------------------ Several readings

/// The outcome of a reading at the pointer: what came out and how sure.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub certainty: Certainty,
    /// The blueprint, for [`Certainty::Sure`] and [`Certainty::Likely`].
    pub name: Option<String>,
    /// The blueprints in question, best first.
    pub candidates: Vec<String>,
    /// The lines that matched, as they were read, for a bug report and the question to the player.
    pub lines: Vec<String>,
    /// The part of the screen around the pointer as PNG, for the question to the player; `None` where it was not
    /// kept (a sure reading, a reading without a picture).
    pub picture: Option<Vec<u8>>,
}

impl Outcome {
    fn nothing() -> Outcome {
        Outcome { certainty: Certainty::None, name: None, candidates: Vec::new(), lines: Vec::new(), picture: None }
    }
}

/// The best reading of the lines of one reading: the surest one. Several different blueprints read in one reading
/// make it unclear.
pub fn best_of_lines(lines: &[String], names: &Names) -> Reading {
    let readings: Vec<Reading> = lines.iter().map(|line| match_line(line, names)).collect();
    let top = readings.iter().map(|r| r.certainty).max().unwrap_or(Certainty::None);
    let mut hits: Vec<&Reading> = readings.iter().filter(|r| r.certainty == top).collect();
    let mut named: Vec<&str> = hits.iter().filter_map(|r| r.name.as_deref()).collect();
    named.sort_unstable();
    named.dedup();
    if top >= Certainty::Likely && named.len() > 1 {
        let candidates = named.iter().map(|n| (n.to_string(), 1.0)).collect();
        return Reading {
            text: hits.iter().map(|r| r.text.as_str()).collect::<Vec<_>>().join(" | "),
            certainty: Certainty::Unclear,
            how: How::Unknown,
            name: None,
            candidates,
            tag: TagCheck::Unread,
        };
    }
    if hits.is_empty() {
        return Reading::none("");
    }
    hits.remove(0).clone()
}

/// Whether `reading` is the whole of `name` read exactly (or with mistakable characters), held back only because
/// longer names begin with it: no tag against it, not a line in capitals, and no other blueprint in question but
/// the longer ones.
fn read_whole_beginning_of_longer(reading: &Reading, name: &str) -> bool {
    let longer = format!("{name} ");
    reading.certainty == Certainty::Likely
        && reading.name.as_deref() == Some(name)
        && matches!(reading.how, How::Exact | How::Lookalike)
        && reading.tag != TagCheck::Contradicts
        && !in_capitals(&split_tag(&reading.text).0)
        && reading.candidates.iter().all(|(other, _)| other == name || other.starts_with(&longer))
        && reading.candidates.len() > 1
}

/// Puts the readings of several passes together. Sure when at least [`MIN_AGREE`] passes read the same blueprint
/// sure and no pass read another one, or when every pass read the whole of a name that longer names begin with and
/// none of them read one of the longer names (a last word lost in every pass at once is not to be feared); a single
/// hit, or hits that differ, go to the player.
pub fn combine(readings: &[Reading]) -> Outcome {
    let mut outcome = Outcome::nothing();
    for reading in readings.iter().filter(|r| r.certainty > Certainty::None) {
        outcome.lines.push(reading.text.clone());
        for (name, _) in &reading.candidates {
            if !outcome.candidates.contains(name) {
                outcome.candidates.push(name.clone());
            }
        }
    }
    let named: Vec<&Reading> = readings.iter().filter(|r| r.certainty >= Certainty::Likely).collect();
    let mut different: Vec<&str> = named.iter().filter_map(|r| r.name.as_deref()).collect();
    different.sort_unstable();
    different.dedup();
    if different.len() > 1 {
        outcome.certainty = Certainty::Unclear;
        return outcome;
    }
    if let Some(name) = different.first() {
        let sure = named.iter().filter(|r| r.certainty == Certainty::Sure).count();
        outcome.name = Some((*name).to_string());
        let whole_in_every_pass =
            readings.len() >= PASSES.len() && readings.iter().all(|r| read_whole_beginning_of_longer(r, name));
        // One pass read it sure and every other pass read something unclear that names it among its candidates (a
        // narrow stroke of `III` lost: `II` lies as near to `III` as to `IX`): nothing speaks against it.
        let unclear_but_naming_it = |r: &&Reading| {
            r.certainty == Certainty::Unclear && r.candidates.iter().any(|(candidate, _)| candidate == name)
        };
        let backed_by_every_pass = sure >= 1
            && readings.len() >= PASSES.len()
            && readings.iter().all(|r| (r.certainty == Certainty::Sure && r.name.as_deref() == Some(*name)) || unclear_but_naming_it(&r));
        outcome.certainty = if sure >= MIN_AGREE || whole_in_every_pass || backed_by_every_pass {
            Certainty::Sure
        } else {
            Certainty::Likely
        };
        outcome.candidates.retain(|c| c != name);
        outcome.candidates.insert(0, (*name).to_string());
        return outcome;
    }
    if readings.iter().any(|r| r.certainty == Certainty::Unclear) {
        outcome.certainty = Certainty::Unclear;
    }
    outcome
}

/// The part of the screen around the pointer where the game shows the hint with the name. The hint begins at the
/// pointer and goes right; near the edge of the list it stands above or below the picture, and a long name breaks
/// onto a second line. The terminal is the larger the nearer the player stands, so the part is generous: in
/// heights of the screen a little left, a tile's height up and down, wide to the right; cut to the screen.
pub fn hint_area(pointer: (i64, i64), size: (i64, i64)) -> Area {
    let unit = size.1 as f64;
    let left = (pointer.0 as f64 - 0.08 * unit) as i64;
    let top = (pointer.1 as f64 - 0.12 * unit) as i64;
    let right = (pointer.0 as f64 + 0.75 * unit) as i64;
    let bottom = (pointer.1 as f64 + 0.18 * unit) as i64;
    let (left, top) = (left.max(0), top.max(0));
    let (right, bottom) = (right.min(size.0), bottom.min(size.1));
    (left, top, (right - left).max(0), (bottom - top).max(0))
}

/// The lines of one reading, top to bottom, each the words of a row joined by blanks. A row is cut where two words
/// stand further apart than twice the height of the row: text beside the hint is no part of the name.
pub fn lines_of(words: &[Word]) -> Vec<String> {
    let mut out = Vec::new();
    for row in rows(words) {
        let height = row.iter().map(|w| w.h).fold(0.0, f64::max);
        let mut line: Vec<&str> = Vec::new();
        let mut end: Option<f64> = None;
        for word in &row {
            if end.is_some_and(|end| word.x - end > 2.0 * height) && !line.is_empty() {
                out.push(line.join(" "));
                line.clear();
            }
            line.push(word.t.as_str());
            end = Some(word.x + word.w);
        }
        if !line.is_empty() {
            out.push(line.join(" "));
        }
    }
    out
}

/// The lines and, behind them, every two following lines joined: a long name the hint breaks onto a second line
/// is read whole too. Its first line alone, the beginning of the longer name, is never sure ([`match_line`]), so
/// the whole name wins.
pub fn with_joined(lines: &[String]) -> Vec<String> {
    let mut out = lines.to_vec();
    out.extend(lines.windows(2).map(|pair| joined(&pair[0], &pair[1])));
    out
}

/// Two following lines of a hint as one: with a blank between them, or without one where the first ends in a
/// hyphen (`"Hazard-` and `Zone" Repeater` are `"Hazard-Zone" Repeater`).
pub fn joined(first: &str, second: &str) -> String {
    if first.ends_with(['-', '‐', '‑']) { format!("{first}{second}") } else { format!("{first} {second}") }
}

/// Reads the name at the pointer: the part [`hint_area`] in every pass of [`PASSES`], each matched against the
/// catalog, put together by [`combine`].
pub fn read_at(size: (i64, i64), pointer: (i64, i64), ocr: &mut dyn Ocr, names: &Names) -> Result<Outcome, ScanError> {
    read_with_area(size, pointer, ocr, names).map(|(outcome, _)| outcome)
}

/// [`read_at`] with the part of the screen that shows the name in question ([`name_area`]), where one was read.
pub fn read_with_area(
    size: (i64, i64),
    pointer: (i64, i64),
    ocr: &mut dyn Ocr,
    names: &Names,
) -> Result<(Outcome, Option<Area>), ScanError> {
    let area = hint_area(pointer, size);
    if area.2 <= 0 || area.3 <= 0 {
        return Ok((Outcome::nothing(), None));
    }
    let jobs: Vec<Job> = PASSES.iter().map(|(scale, mode)| Job { area, scale: *scale, mode: *mode }).collect();
    let passes = ocr.read(&jobs)?;
    let at = (pointer.0 as f64, pointer.1 as f64);
    let readings: Vec<Reading> = passes.iter().map(|words| nearest_of_lines(words, at, names)).collect();
    let outcome = combine(&readings);
    let shown = passes.iter().find_map(|words| name_area(words, &outcome.candidates, names, size));
    Ok((outcome, shown))
}

/// The margin around the lines of the name in [`name_area`], in heights of the screen.
pub const NAME_MARGIN: f64 = 0.03;

/// The lines of one reading, each with its frame (left, top, right, bottom), cut as [`lines_of`] cuts them.
fn framed_lines(words: &[Word]) -> Vec<(String, (f64, f64, f64, f64))> {
    let mut out = Vec::new();
    for row in rows(words) {
        let height = row.iter().map(|w| w.h).fold(0.0, f64::max);
        let mut line: Vec<&Word> = Vec::new();
        let mut flush = |line: &mut Vec<&Word>| {
            if line.is_empty() {
                return;
            }
            let text = line.iter().map(|w| w.t.as_str()).collect::<Vec<_>>().join(" ");
            let frame = line.iter().fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |f, w| {
                (f.0.min(w.x), f.1.min(w.y), f.2.max(w.x + w.w), f.3.max(w.y + w.h))
            });
            out.push((text, frame));
            line.clear();
        };
        for word in &row {
            if line.last().is_some_and(|last| word.x - (last.x + last.w) > 2.0 * height) {
                flush(&mut line);
            }
            line.push(word);
        }
        flush(&mut line);
    }
    out
}

/// The part of the screen that shows the name in question: the lines of a reading that name one of `candidates` (a
/// line alone, or two following lines joined), with a margin of [`NAME_MARGIN`] around them, cut to the screen.
/// `None` when no such line was read.
pub fn name_area(words: &[Word], candidates: &[String], names: &Names, size: (i64, i64)) -> Option<Area> {
    let lines = framed_lines(words);
    // A line names a blueprint in question by its name, or by its best candidate where it is not sure of one.
    let names_one = |text: &str| {
        let reading = match_line(text, names);
        let best = reading.name.or_else(|| reading.candidates.first().map(|(name, _)| name.clone()));
        best.is_some_and(|name| candidates.contains(&name))
    };
    let mut frames = Vec::new();
    for (i, (text, frame)) in lines.iter().enumerate() {
        if names_one(text) {
            frames.push(*frame);
        }
        // Two lines are one name only where the second follows right below the first.
        if let Some((next, below)) = lines.get(i + 1)
            && below.1 - frame.3 <= frame.3 - frame.1
            && names_one(&joined(text, next))
        {
            frames.extend([*frame, *below]);
        }
    }
    let (left, top, right, bottom) =
        frames.into_iter().reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))?;
    let margin = NAME_MARGIN * size.1 as f64;
    let left = ((left - margin) as i64).max(0);
    let top = ((top - margin) as i64).max(0);
    let right = ((right + margin) as i64).min(size.0);
    let bottom = ((bottom + margin) as i64).min(size.1);
    (right > left && bottom > top).then_some((left, top, right - left, bottom - top))
}

/// How much nearer to the pointer the nearest named line must stand than the nearest line with another blueprint,
/// as a share of that one's distance, to be taken alone.
pub const NEAR_SHARE: f64 = 0.5;

/// The lines of one reading with where each begins (left edge, middle of its height), as [`lines_of`] cuts them.
fn placed_lines(words: &[Word]) -> Vec<(String, (f64, f64))> {
    let mut out = Vec::new();
    for row in rows(words) {
        let height = row.iter().map(|w| w.h).fold(0.0, f64::max);
        let mut line: Vec<&str> = Vec::new();
        let mut start = (0.0, 0.0);
        let mut end: Option<f64> = None;
        for word in &row {
            if end.is_some_and(|end| word.x - end > 2.0 * height) && !line.is_empty() {
                out.push((line.join(" "), start));
                line.clear();
            }
            if line.is_empty() {
                start = (word.x, word.y + word.h / 2.0);
            }
            line.push(word.t.as_str());
            end = Some(word.x + word.w);
        }
        if !line.is_empty() {
            out.push((line.join(" "), start));
        }
    }
    out
}

/// The distance of a line beginning at `start` from the pointer: the hint begins at the pointer and goes right, so a
/// line that begins left of the pointer counts from where it begins.
fn distance(start: (f64, f64), pointer: (f64, f64)) -> f64 {
    (start.0 - pointer.0).abs().hypot(start.1 - pointer.1)
}

/// [`best_of_lines`] of one reading, where the line beside the pointer wins: when lines of several blueprints were
/// read (the hint, and a name elsewhere in the part such as the heading of the blueprint chosen on the right), the
/// one clearly nearest to the pointer is the hint. Without such a lead it is as unclear as before.
pub fn nearest_of_lines(words: &[Word], pointer: (f64, f64), names: &Names) -> Reading {
    let placed = placed_lines(words);
    let mut lines: Vec<(String, (f64, f64))> = placed.clone();
    lines.extend(placed.windows(2).map(|pair| (joined(&pair[0].0, &pair[1].0), pair[0].1)));
    let texts: Vec<String> = lines.iter().map(|(text, _)| text.clone()).collect();
    let whole = best_of_lines(&texts, names);
    if whole.certainty != Certainty::Unclear || whole.how != How::Unknown {
        return whole;
    }
    // Several blueprints named: each with the distance of its nearest line.
    let mut named: Vec<(f64, Reading)> = lines
        .iter()
        .map(|(text, start)| (distance(*start, pointer), match_line(text, names)))
        .filter(|(_, reading)| reading.certainty >= Certainty::Likely && reading.name.is_some())
        .collect();
    named.sort_by(|a, b| a.0.total_cmp(&b.0));
    let Some((near, first)) = named.first() else { return whole };
    let other = named.iter().find(|(_, r)| r.name != first.name).map(|(d, _)| *d);
    match other {
        Some(far) if *near <= NEAR_SHARE * far => {
            // The lines of the nearest blueprint alone decide, as one reading.
            let own: Vec<String> = named.iter().filter(|(_, r)| r.name == first.name).map(|(_, r)| r.text.clone()).collect();
            best_of_lines(&own, names)
        }
        _ => whole,
    }
}

// ------------------------------------------------------------ The hotkey in the game

/// The comparison set of the catalog the program works with now (`catalog_cache::load_current`); `None` without one.
pub fn names_of(paths: &crate::paths::Paths) -> Option<Names> {
    crate::catalog_cache::load_current(paths).map(|catalog| Names::of_catalog(&catalog))
}

/// Reads the name at the mouse pointer (`pointer`, in points of the screen) in the window of the game while it is in
/// front. `Ok(None)` when the game is not in front or the pointer stands outside its window.
pub fn read_game(pointer: (i32, i32), names: &Names) -> Result<Option<Outcome>, ScanError> {
    use crate::refinery_scan::Reason;
    let Some(rect) = crate::screen_grab::game_rect() else { return Ok(None) };
    let at = (i64::from(pointer.0 - rect.0), i64::from(pointer.1 - rect.1));
    let size = (i64::from(rect.2), i64::from(rect.3));
    if at.0 < 0 || at.1 < 0 || at.0 >= size.0 || at.1 >= size.1 {
        return Ok(None);
    }
    let raw = crate::screen_grab::grab_raw(rect).map_err(|e| ScanError::new(Reason::Grab, e.key()))?;
    let bitmap = crate::win_ocr::Bitmap { width: size.0, height: size.1, raw };
    let mut ocr = crate::win_ocr::Recognition::new(&bitmap);
    let (mut outcome, shown) = read_with_area(size, at, &mut ocr, names)?;
    // A reading the player is asked about keeps the part that showed the name, else the part around the pointer.
    if outcome.certainty != Certainty::Sure && !outcome.candidates.is_empty() {
        let part = shown.unwrap_or_else(|| hint_area(at, size));
        outcome.picture = crate::win_ocr::png_of(&bitmap, part, Some(QUESTION_PICTURE_WIDTH)).ok();
    }
    Ok(Some(outcome))
}

/// The widest picture kept with a question, in pixels.
pub const QUESTION_PICTURE_WIDTH: i64 = 640;

/// No translated names: the scan writes names of the catalog.
struct Untranslated;

impl crate::phrases::EnglishNames for Untranslated {
    fn english_names(&self, _name: &str) -> Vec<String> {
        Vec::new()
    }
}

/// The source the scan writes into the collection.
pub const SOURCE: &str = "scan";

/// What to do with `outcome` for the collection of `paths`, done: a blueprint read sure that the collection does
/// not have is added under [`SOURCE`] and the collection saved, under the lock of the saves. `Err` when the save
/// failed; the action is then not done.
pub fn record(paths: &crate::paths::Paths, outcome: &Outcome) -> Result<Action, String> {
    use crate::collection::{self, Collection, NoCatalog};
    let exports =
        crate::export::FileExports { paths, version: env!("CARGO_PKG_VERSION"), clock: crate::export::now_unix };
    let collection =
        Collection { paths, catalog: &NoCatalog, names: &Untranslated, exports: &exports, clock: collection::now_text };
    let _lock = crate::jsonio::lock_saves();
    let mut data = collection.load();
    let action = decide(outcome, |name| collection::contains(&data, name));
    match &action {
        Action::Add(name) => {
            collection.add(&mut data, name, SOURCE, None);
            if !collection.save(&mut data) {
                return Err("the collection could not be written".into());
            }
        }
        Action::Confirm(names) => {
            ask(paths, Question::of(outcome, names, crate::export::now_unix())).map_err(|e| e.to_string())?;
        }
        Action::AlreadyThere(_) | Action::NotFound => {}
    }
    let now = crate::export::now_unix();
    if let Action::Add(name) | Action::AlreadyThere(name) = &action {
        settle_recent_questions(paths, name, now);
    }
    note_scan(paths, &action, now);
    Ok(action)
}

/// How long ago a question may have been asked to be settled by a later sure scan of one of its candidates.
pub const SETTLE_SECONDS: i64 = 120;

/// A blueprint read sure answers the questions of the last [`SETTLE_SECONDS`] that had it among their candidates
/// (the same tile scanned again once the hint of its neighbour was gone): they go. A failed write only goes to the
/// error log.
fn settle_recent_questions(paths: &crate::paths::Paths, name: &str, now: i64) {
    let _lock = crate::jsonio::lock_saves();
    let all = questions(paths);
    let kept: Vec<Question> = all
        .iter()
        .filter(|q| !(now - q.at <= SETTLE_SECONDS && q.at <= now && q.candidates.iter().any(|c| c == name)))
        .cloned()
        .collect();
    if kept.len() != all.len()
        && let Err(error) = write_questions(paths, &kept)
    {
        crate::errors::record("blueprint_scan.settle_recent_questions", &error);
    }
}

/// Whole seconds from a number of the file, 0 where there is none.
fn seconds_of(value: Option<&crate::jsonio::Value>) -> i64 {
    use crate::jsonio::Value;
    match value {
        Some(Value::Int(text)) => text.parse().unwrap_or(0),
        Some(Value::Float(f)) if f.is_finite() => *f as i64,
        _ => 0,
    }
}

/// The file of the last scans, newest last, whatever came of them (`{"scans": [{"zeit", "was", "name"}]}`).
pub const SCAN_LOG_FILE: &str = "bauplan-scans.json";

/// The most scans the file keeps.
pub const SCAN_LOG_MOST: usize = 30;

/// One scan as the list of the last scans shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanEntry {
    /// Unix seconds.
    pub at: i64,
    /// What came of it: `erfasst`, `schon`, `frage` or `keiner`.
    pub what: String,
    /// The blueprint (the first in question for a question), empty when nothing was read.
    pub name: String,
}

/// The last scans, newest first, at most `most`.
pub fn last_scans(paths: &crate::paths::Paths, most: usize) -> Vec<ScanEntry> {
    use crate::jsonio::{self, Value};
    let Ok(Value::Object(map)) = jsonio::read_json(&paths.app_file(SCAN_LOG_FILE)) else { return Vec::new() };
    let Some((_, Value::Array(items))) = map.into_iter().find(|(key, _)| key == "scans") else { return Vec::new() };
    let text = |item: &Value, key: &str| match item.get(key) {
        Some(Value::Str(text)) => text.clone(),
        _ => String::new(),
    };
    let mut all: Vec<ScanEntry> = items
        .iter()
        .map(|item| ScanEntry {
            at: seconds_of(item.get("zeit")),
            what: text(item, "was"),
            name: text(item, "name"),
        })
        .collect();
    all.reverse();
    all.truncate(most);
    all
}

/// Notes a scan and what came of it at the end of [`SCAN_LOG_FILE`]; a failed write only goes to the error log.
fn note_scan(paths: &crate::paths::Paths, action: &Action, at: i64) {
    use crate::jsonio::{self, Value};
    let (what, name) = match action {
        Action::Add(name) => ("erfasst", name.clone()),
        Action::AlreadyThere(name) => ("schon", name.clone()),
        Action::Confirm(names) => ("frage", names.first().cloned().unwrap_or_default()),
        Action::NotFound => ("keiner", String::new()),
    };
    let _lock = jsonio::lock_saves();
    let mut entries: Vec<Value> = last_scans(paths, SCAN_LOG_MOST)
        .into_iter()
        .rev()
        .map(|e| {
            Value::Object(vec![
                ("zeit".into(), Value::int(e.at)),
                ("was".into(), Value::str(&e.what)),
                ("name".into(), Value::str(&e.name)),
            ])
        })
        .collect();
    entries.push(Value::Object(vec![
        ("zeit".into(), Value::int(at)),
        ("was".into(), Value::str(what)),
        ("name".into(), Value::str(&name)),
    ]));
    if entries.len() > SCAN_LOG_MOST {
        entries.drain(..entries.len() - SCAN_LOG_MOST);
    }
    let data = Value::Object(vec![("scans".into(), Value::Array(entries))]);
    if let Err(error) = jsonio::save_json(&paths.app_file(SCAN_LOG_FILE), &data, 1, false) {
        crate::errors::record("blueprint_scan.note_scan", &error);
    }
}

/// The file of the questions to the player: each scan read unsure, oldest first, for the blueprint list
/// (`{"fragen": [{"kandidaten": [...], "zeit": …, "gelesen": "…", "bild": "<PNG, base64>"}]}`; a file of one
/// question `{"kandidaten": [...]}` is read as one).
pub const QUESTION_FILE: &str = "bauplan-rueckfrage.json";

/// The most questions kept; beyond them the oldest goes.
pub const MAX_QUESTIONS: usize = 50;

/// A question to the player: the blueprints in question, when the scan was, what was read and what the screen
/// showed around the pointer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Question {
    pub candidates: Vec<String>,
    /// The moment of the scan (Unix seconds), 0 where it is not known.
    pub at: i64,
    /// The lines read, joined; empty where none were kept.
    pub read: String,
    /// The part around the pointer as PNG.
    pub picture: Option<Vec<u8>>,
}

impl Question {
    /// The question of an outcome read unsure at `at`.
    pub fn of(outcome: &Outcome, candidates: &[String], at: i64) -> Question {
        Question {
            candidates: candidates.to_vec(),
            at,
            read: outcome.lines.join(" | "),
            picture: outcome.picture.clone(),
        }
    }

    fn from_value(value: &crate::jsonio::Value) -> Question {
        use base64::Engine as _;
        use crate::jsonio::Value;
        let Value::Object(map) = value else { return Question::default() };
        let field = |key: &str| map.iter().find(|(k, _)| k == key).map(|(_, v)| v);
        let candidates = match field("kandidaten") {
            Some(Value::Array(names)) => names
                .iter()
                .filter_map(|n| match n {
                    Value::Str(name) => Some(name.clone()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        let text = |key: &str| match field(key) {
            Some(Value::Str(text)) => text.clone(),
            _ => String::new(),
        };
        let picture = Some(text("bild"))
            .filter(|b| !b.is_empty())
            .and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok());
        let at = seconds_of(field("zeit"));
        Question { candidates, at, read: text("gelesen"), picture }
    }

    fn to_value(&self) -> crate::jsonio::Value {
        use base64::Engine as _;
        use crate::jsonio::Value;
        let mut fields = vec![
            ("kandidaten".to_string(), Value::Array(self.candidates.iter().map(Value::str).collect())),
            ("zeit".to_string(), Value::int(self.at)),
            ("gelesen".to_string(), Value::str(&self.read)),
        ];
        if let Some(picture) = &self.picture {
            fields.push(("bild".to_string(), Value::str(base64::engine::general_purpose::STANDARD.encode(picture))));
        }
        Value::Object(fields)
    }
}

/// All open questions, oldest first.
pub fn questions(paths: &crate::paths::Paths) -> Vec<Question> {
    use crate::jsonio::{self, Value};
    let Ok(data) = jsonio::read_json(&paths.app_file(QUESTION_FILE)) else { return Vec::new() };
    let listed = match &data {
        Value::Object(map) => map.iter().find(|(key, _)| key == "fragen").map(|(_, v)| v),
        _ => None,
    };
    let all: Vec<Question> = match listed {
        Some(Value::Array(items)) => items.iter().map(Question::from_value).collect(),
        _ => vec![Question::from_value(&data)],
    };
    all.into_iter().filter(|q| !q.candidates.is_empty()).collect()
}

fn write_questions(paths: &crate::paths::Paths, all: &[Question]) -> std::io::Result<()> {
    use crate::jsonio::{self, Value};
    let file = paths.app_file(QUESTION_FILE);
    if all.is_empty() {
        return match jsonio::remove_file(&file) {
            Err(e) if !jsonio::is_not_found(&e) => Err(e),
            _ => Ok(()),
        };
    }
    let items = all.iter().map(Question::to_value).collect();
    jsonio::save_json(&file, &Value::Object(vec![("fragen".into(), Value::Array(items))]), 1, false)
}

/// Keeps a scan read unsure for the player to choose from in the blueprint list, behind the questions already
/// open. The same candidates as the last open question are not asked twice.
pub fn ask(paths: &crate::paths::Paths, question: Question) -> std::io::Result<()> {
    let _lock = crate::jsonio::lock_saves();
    let mut all = questions(paths);
    if all.last().is_some_and(|last| last.candidates == question.candidates) {
        return Ok(());
    }
    all.push(question);
    if all.len() > MAX_QUESTIONS {
        all.drain(..all.len() - MAX_QUESTIONS);
    }
    write_questions(paths, &all)
}

/// Drops every open question; nothing goes into the collection.
pub fn discard_questions(paths: &crate::paths::Paths) -> std::io::Result<()> {
    let _lock = crate::jsonio::lock_saves();
    write_questions(paths, &[])
}

/// The oldest open question, `None` when there is none.
pub fn question(paths: &crate::paths::Paths) -> Option<Question> {
    questions(paths).into_iter().next()
}

/// The player's answer to the oldest open question: the blueprint chosen goes into the collection under [`SOURCE`],
/// none leaves it as it is; either way that question is gone and the next one is the oldest.
pub fn answer(paths: &crate::paths::Paths, chosen: Option<&str>) -> Result<(), String> {
    answer_at(paths, 0, chosen)
}

/// [`answer`] for the open question at `index` (oldest first); an index beyond the questions answers none.
pub fn answer_at(paths: &crate::paths::Paths, index: usize, chosen: Option<&str>) -> Result<(), String> {
    use crate::collection::{self, Collection, NoCatalog};
    if let Some(name) = chosen {
        let exports =
            crate::export::FileExports { paths, version: env!("CARGO_PKG_VERSION"), clock: crate::export::now_unix };
        let collection = Collection {
            paths,
            catalog: &NoCatalog,
            names: &Untranslated,
            exports: &exports,
            clock: collection::now_text,
        };
        let _lock = crate::jsonio::lock_saves();
        let mut data = collection.load();
        if !collection::contains(&data, name) {
            collection.add(&mut data, name, SOURCE, None);
            if !collection.save(&mut data) {
                return Err("the collection could not be written".into());
            }
        }
    }
    let _lock = crate::jsonio::lock_saves();
    let mut all = questions(paths);
    if index < all.len() {
        all.remove(index);
    }
    write_questions(paths, &all).map_err(|e| e.to_string())
}

// ------------------------------------------------------------ What to do

/// What to do with an outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// A blueprint read sure that the collection does not have yet: to be added.
    Add(String),
    /// A blueprint read sure that the collection has already.
    AlreadyThere(String),
    /// A blueprint, or several, the player must confirm before anything is added.
    Confirm(Vec<String>),
    /// Nothing of the catalog read.
    NotFound,
}

/// What to do with an outcome, given which blueprints the collection has. Only a sure outcome adds.
pub fn decide(outcome: &Outcome, have: impl Fn(&str) -> bool) -> Action {
    match (outcome.certainty, &outcome.name) {
        (Certainty::Sure, Some(name)) if have(name) => Action::AlreadyThere(name.clone()),
        (Certainty::Sure, Some(name)) => Action::Add(name.clone()),
        (Certainty::None, _) => Action::NotFound,
        _ if outcome.candidates.is_empty() => Action::NotFound,
        _ => Action::Confirm(outcome.candidates.iter().take(3).cloned().collect()),
    }
}

/// The message to an action, as the player sees it.
pub fn message_in(lang: Lang, action: &Action) -> String {
    match action {
        Action::Add(name) => language::tf_in(lang, "s_bpscan_erfasst", &[("0", Arg::Text(name.clone()))]),
        Action::AlreadyThere(name) => language::tf_in(lang, "s_bpscan_schon", &[("0", Arg::Text(name.clone()))]),
        Action::Confirm(names) if names.len() == 1 => {
            language::tf_in(lang, "s_bpscan_bestaetigen", &[("0", Arg::Text(names[0].clone()))])
        }
        Action::Confirm(names) => language::tf_in(lang, "s_bpscan_welcher", &[("0", Arg::Text(names.join(" · ")))]),
        Action::NotFound => language::t_in(lang, "s_bpscan_keiner"),
    }
}

/// The notice over the game to an action ([`crate::scan_notice`]): the title and the name, or the candidates.
pub fn notice_in(lang: Lang, action: &Action) -> crate::scan_notice::Notice {
    use crate::scan_notice::{Notice, NoticeKind};
    match action {
        Action::Add(name) => Notice::new(NoticeKind::Done, language::t_in(lang, "s_bpscan_t_erfasst"), name.clone()),
        Action::AlreadyThere(name) => {
            Notice::new(NoticeKind::AlreadyThere, language::t_in(lang, "s_bpscan_t_schon"), name.clone())
        }
        Action::Confirm(names) => {
            Notice::new(NoticeKind::Unsure, language::t_in(lang, "s_bpscan_t_bestaetigen"), names.join(" · "))
        }
        Action::NotFound => Notice::new(NoticeKind::Nothing, language::t_in(lang, "s_bpscan_t_keiner"), ""),
    }
}

// ------------------------------------------------------------ Hit rate

/// How a set of test readings came out against the names they should give.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    /// Sure and right.
    pub right: usize,
    /// Sure and wrong: the error that must not happen.
    pub wrong: usize,
    /// Asked to confirm, with the right name among the ones offered.
    pub asked_right: usize,
    /// Asked to confirm, without the right name.
    pub asked_wrong: usize,
    /// Nothing read.
    pub missed: usize,
    /// Read something for a picture that shows no blueprint.
    pub false_alarm: usize,
}

impl Tally {
    /// Counts one outcome against the name it should give (`None`: the picture shows none).
    pub fn count(&mut self, outcome: &Outcome, expected: Option<&str>) {
        let Some(expected) = expected else {
            if outcome.certainty == Certainty::None {
                self.right += 1;
            } else if outcome.certainty == Certainty::Sure {
                self.wrong += 1;
            } else {
                self.false_alarm += 1;
            }
            return;
        };
        match outcome.certainty {
            Certainty::Sure if outcome.name.as_deref() == Some(expected) => self.right += 1,
            Certainty::Sure => self.wrong += 1,
            Certainty::None => self.missed += 1,
            _ if outcome.candidates.iter().any(|c| c == expected) => self.asked_right += 1,
            _ => self.asked_wrong += 1,
        }
    }

    /// All counted.
    pub fn total(&self) -> usize {
        self.right + self.wrong + self.asked_right + self.asked_wrong + self.missed + self.false_alarm
    }

    /// The share taken without asking and right, of all.
    pub fn hit_rate(&self) -> f64 {
        if self.total() == 0 { 0.0 } else { self.right as f64 / self.total() as f64 }
    }
}

impl std::fmt::Display for Tally {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} total: {} sure and right ({:.1} %), {} sure and WRONG, {} asked with the right one, {} asked without \
             it, {} missed, {} false alarms",
            self.total(),
            self.right,
            self.hit_rate() * 100.0,
            self.wrong,
            self.asked_right,
            self.asked_wrong,
            self.missed,
            self.false_alarm
        )
    }
}

#[cfg(test)]
mod tests;
