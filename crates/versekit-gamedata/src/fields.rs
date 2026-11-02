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

//! Reading fields out of DataForge record trees, and building the trees of
//! linked records once.

use std::collections::HashMap;
use std::rc::Rc;

use p4kx_dataforge::{DataForge, Guid, RecordTree, StructValue, TreeBuilder, Value};

/// The struct the field `name` of `parent` holds.
pub(crate) fn structure<'t, 'f>(parent: Option<&'t StructValue<'f>>, name: &str) -> Option<&'t StructValue<'f>> {
    match parent?.field(name)? {
        Value::Struct(value) => Some(value),
        _ => None,
    }
}

/// The elements of the array the field `name` of `parent` holds; none for a
/// field that is absent or no array.
pub(crate) fn elements<'t, 'f>(parent: Option<&'t StructValue<'f>>, name: &str) -> &'t [Value<'f>] {
    match parent.and_then(|parent| parent.field(name)) {
        Some(Value::Array(items)) => items,
        _ => &[],
    }
}

/// The struct elements of the array the field `name` of `parent` holds.
pub(crate) fn structs<'t, 'f>(
    parent: Option<&'t StructValue<'f>>,
    name: &str,
) -> impl Iterator<Item = &'t StructValue<'f>> {
    elements(parent, name).iter().filter_map(as_struct)
}

/// The value as a struct.
pub(crate) const fn as_struct<'t, 'f>(value: &'t Value<'f>) -> Option<&'t StructValue<'f>> {
    match value {
        Value::Struct(value) => Some(value),
        _ => None,
    }
}

/// The first element of the `Components` array of `tree` whose struct is
/// `type_name`.
pub(crate) fn component<'t, 'f>(tree: &'t RecordTree<'f>, type_name: &str) -> Option<&'t StructValue<'f>> {
    structs(Some(tree.root()), "Components").find(|component| component.type_name() == type_name)
}

/// The localisation key the field `name` of `parent` holds; an empty key is
/// none. A string starting with `@` counts as a key.
pub(crate) fn locale_key<'f>(parent: Option<&StructValue<'f>>, name: &str) -> Option<&'f str> {
    match parent?.field(name)? {
        Value::Locale(key) if !key.is_empty() => Some(*key),
        Value::String(text) if text.starts_with('@') && text.len() > 1 => Some(*text),
        _ => None,
    }
}

/// The string or enum option the field `name` of `parent` holds; an empty one
/// is none.
pub(crate) fn text<'f>(parent: Option<&StructValue<'f>>, name: &str) -> Option<&'f str> {
    match parent?.field(name)? {
        Value::String(text) | Value::Enum(text) if !text.is_empty() => Some(*text),
        _ => None,
    }
}

/// The integer the field `name` of `parent` holds; `None` for one beyond
/// `i64`.
pub(crate) fn integer(parent: Option<&StructValue<'_>>, name: &str) -> Option<i64> {
    match *parent?.field(name)? {
        Value::Int8(value) => Some(value.into()),
        Value::Int16(value) => Some(value.into()),
        Value::Int32(value) => Some(value.into()),
        Value::Int64(value) => Some(value),
        Value::UInt8(value) => Some(value.into()),
        Value::UInt16(value) => Some(value.into()),
        Value::UInt32(value) => Some(value.into()),
        Value::UInt64(value) => i64::try_from(value).ok(),
        _ => None,
    }
}

/// The finite number the field `name` of `parent` holds, integer or float.
/// A 32-bit float is taken as the shortest decimal that gives it back
/// (`0.03`, not `0.029999999329447746`), as the game's own tools and
/// scmdb show it.
pub(crate) fn number(parent: Option<&StructValue<'_>>, name: &str) -> Option<f64> {
    match *parent?.field(name)? {
        Value::Single(value) if value.is_finite() => value.to_string().parse().ok(),
        Value::Double(value) if value.is_finite() => Some(value),
        Value::Int8(value) => Some(value.into()),
        Value::Int16(value) => Some(value.into()),
        Value::Int32(value) => Some(value.into()),
        Value::Int64(value) => Some(value as f64),
        Value::UInt8(value) => Some(value.into()),
        Value::UInt16(value) => Some(value.into()),
        Value::UInt32(value) => Some(value.into()),
        Value::UInt64(value) => Some(value as f64),
        _ => None,
    }
}

/// The boolean the field `name` of `parent` holds.
pub(crate) fn boolean(parent: Option<&StructValue<'_>>, name: &str) -> Option<bool> {
    match *parent?.field(name)? {
        Value::Bool(value) => Some(value),
        _ => None,
    }
}

/// The GUID of the record the reference in the field `name` of `parent`
/// links to. A weak pointer is none: it carries the GUID of the record that
/// holds it and the location of an instance inside it.
pub(crate) fn reference(parent: Option<&StructValue<'_>>, name: &str) -> Option<Guid> {
    parent?.field(name).and_then(link)
}

/// The GUID of the record `value` links to, when it is a reference.
pub(crate) fn link(value: &Value<'_>) -> Option<Guid> {
    match value {
        Value::Reference(reference) if reference.location().is_none() => Some(reference.guid()),
        _ => None,
    }
}

/// The record name without the leading `<type>.`, e.g. `BEHR` for
/// `SCItemManufacturer.BEHR`; a name without that start is returned whole.
pub(crate) fn class_name<'f>(tree: &RecordTree<'f>) -> &'f str {
    let name = tree.name();
    name.strip_prefix(tree.type_name())
        .and_then(|rest| rest.strip_prefix('.'))
        .filter(|rest| !rest.is_empty())
        .unwrap_or(name)
}

/// Builds record trees on request and keeps each one, so that a record many
/// others link to is built once.
pub(crate) struct Trees<'f, 'a> {
    forge: &'f DataForge<'a>,
    builder: TreeBuilder<'f>,
    built: HashMap<Guid, Option<Rc<RecordTree<'f>>>>,
    failed: Vec<String>,
}

impl<'f, 'a> Trees<'f, 'a> {
    pub(crate) fn new(forge: &'f DataForge<'a>) -> Self {
        Self { forge, builder: TreeBuilder::new(), built: HashMap::new(), failed: Vec::new() }
    }

    /// The tree of the record with `guid`; `None` when there is no such
    /// record or it cannot be read, which is noted in [`Trees::failed`].
    pub(crate) fn get(&mut self, guid: Guid) -> Option<Rc<RecordTree<'f>>> {
        if let Some(known) = self.built.get(&guid) {
            return known.clone();
        }
        let tree = self.forge.record_by_guid(guid).and_then(|record| match self.builder.build(&record) {
            Ok(tree) => Some(Rc::new(tree)),
            Err(error) => {
                let path = record.record_path().unwrap_or("?");
                self.failed.push(format!("{path} ({guid}): {error}"));
                None
            }
        });
        self.built.insert(guid, tree.clone());
        tree
    }

    /// Whether a record with `guid` exists, readable or not.
    pub(crate) fn exists(&self, guid: Guid) -> bool {
        self.forge.record_by_guid(guid).is_some()
    }

    /// The tree of the record with `guid`, built without keeping it; for
    /// records only one reader looks at.
    pub(crate) fn once(&mut self, guid: Guid) -> Option<Rc<RecordTree<'f>>> {
        if let Some(known) = self.built.get(&guid) {
            return known.clone();
        }
        let record = self.forge.record_by_guid(guid)?;
        match self.builder.build(&record) {
            Ok(tree) => Some(Rc::new(tree)),
            Err(error) => {
                let path = record.record_path().unwrap_or("?");
                self.failed.push(format!("{path} ({guid}): {error}"));
                None
            }
        }
    }

    /// The tree of the record the reference in the field `name` of `parent`
    /// links to.
    pub(crate) fn follow(&mut self, parent: Option<&StructValue<'_>>, name: &str) -> Option<Rc<RecordTree<'f>>> {
        reference(parent, name).and_then(|guid| self.get(guid))
    }

    /// The records that could not be read: record path, GUID and reason.
    pub(crate) fn into_failed(self) -> Vec<String> {
        self.failed
    }
}
