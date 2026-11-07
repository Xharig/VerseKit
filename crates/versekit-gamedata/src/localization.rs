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

//! The texts of one language from its `global.ini`, with English as fallback.

use p4kx_localization::{GlobalIni, IniText, OwnedTextSet, TextSet, decode_line_breaks};

use crate::Error;

/// A language the archive can hold a `global.ini` for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Language {
    English,
    German,
    French,
    Italian,
    SpanishSpain,
    SpanishLatinAmerica,
    PortugueseBrazil,
    Japanese,
    Korean,
    ChineseSimplified,
    ChineseTraditional,
}

impl Language {
    /// Every language, English first.
    pub const ALL: [Self; 11] = [
        Self::English,
        Self::German,
        Self::French,
        Self::Italian,
        Self::SpanishSpain,
        Self::SpanishLatinAmerica,
        Self::PortugueseBrazil,
        Self::Japanese,
        Self::Korean,
        Self::ChineseSimplified,
        Self::ChineseTraditional,
    ];

    /// The language code, e.g. `de`, `es-419` or `zh-Hans`.
    pub fn code(self) -> &'static str {
        self.reader().code()
    }

    /// The path of the language's `global.ini` entry in the archive.
    pub(crate) fn global_ini_path(self) -> &'static str {
        self.reader().global_ini_path()
    }

    fn reader(self) -> p4kx_localization::Language {
        use p4kx_localization::Language as Reader;
        match self {
            Self::English => Reader::English,
            Self::German => Reader::German,
            Self::French => Reader::French,
            Self::Italian => Reader::Italian,
            Self::SpanishSpain => Reader::SpanishSpain,
            Self::SpanishLatinAmerica => Reader::SpanishLatinAmerica,
            Self::PortugueseBrazil => Reader::PortugueseBrazil,
            Self::Japanese => Reader::Japanese,
            Self::Korean => Reader::Korean,
            Self::ChineseSimplified => Reader::ChineseSimplified,
            Self::ChineseTraditional => Reader::ChineseTraditional,
        }
    }
}

/// The texts of one language, resolving localisation keys such as
/// `@item_NameBEHR_LMG`.
#[derive(Debug, Clone)]
pub struct Localization {
    language: Language,
    has_own: bool,
    texts: OwnedTextSet,
}

impl Localization {
    /// Indexes the English `global.ini` and, when given, the one of
    /// `language`; both are bytes as the archive stores them.
    ///
    /// # Errors
    ///
    /// [`Error::Localization`] when a file cannot be read as texts.
    pub fn from_files(language: Language, english: &[u8], own: Option<&[u8]>) -> Result<Self, Error> {
        let english_text = decode(Language::English, english)?;
        let mut files = vec![(Language::English.reader(), index(Language::English, &english_text)?)];
        let own_text = own.map(|bytes| decode(language, bytes)).transpose()?;
        if let Some(text) = &own_text {
            files.push((language.reader(), index(language, text)?));
        }
        let has_own = language == Language::English || own_text.is_some();
        Ok(Self { language, has_own, texts: TextSet::new(files).to_owned_set() })
    }

    /// The language the texts are looked up in first.
    pub fn language(&self) -> Language {
        self.language
    }

    /// Whether the archive holds a `global.ini` of the language; without one,
    /// every text is English.
    pub fn has_own_texts(&self) -> bool {
        self.has_own
    }

    /// The number of keys of the language's file, or of the English one
    /// when the language has none.
    pub fn len(&self) -> usize {
        let language = if self.has_own { self.language } else { Language::English };
        self.texts.len(language.reader())
    }

    /// Whether no file holds a key.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The text of `key`, with or without its leading `@`, ignoring ASCII
    /// case: in the language, else in English, with `\n` turned into line
    /// feeds. `None` for a placeholder key and for a key without a non-empty
    /// text.
    pub fn get(&self, key: &str) -> Option<String> {
        let found = self.texts.lookup(key, self.language.reader(), Some(Language::English.reader()));
        found.text().map(|text| decode_line_breaks(text).into_owned())
    }

    /// The text of `key` in English only, as [`Localization::get`] returns it.
    pub fn english(&self, key: &str) -> Option<String> {
        let found = self.texts.lookup(key, Language::English.reader(), None);
        found.text().map(|text| decode_line_breaks(text).into_owned())
    }
}

fn decode(language: Language, bytes: &[u8]) -> Result<IniText<'_>, Error> {
    IniText::decode(bytes).map_err(|source| Error::Localization {
        entry: language.global_ini_path().to_string(),
        source: Box::new(source),
    })
}

fn index<'a>(language: Language, text: &'a IniText<'_>) -> Result<GlobalIni<'a>, Error> {
    text.index().map_err(|source| Error::Localization {
        entry: language.global_ini_path().to_string(),
        source: Box::new(source),
    })
}
