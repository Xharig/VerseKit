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

//! Paths as text, following the rules of Python's `ntpath` and `posixpath`.
//!
//! ⚠ Stored paths must stay character-identical: a storage folder is stored
//! as `D:/Spiele/Daten` in the pointer file, and `join` appends to it with
//! `\`, giving `D:/Spiele/Daten\Einstellungen`. `PathBuf` would
//! decide differently in other places (trailing slash, `dirname`), so this
//! module works with text and knows both systems, regardless of where it
//! runs. That also allows checking Linux paths under Windows.

/// Which path rules apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flavor {
    Windows,
    Posix,
}

impl Flavor {
    /// The rules of the system the program runs on.
    pub const fn native() -> Self {
        if cfg!(windows) { Flavor::Windows } else { Flavor::Posix }
    }

    pub const fn sep(self) -> char {
        match self {
            Flavor::Windows => '\\',
            Flavor::Posix => '/',
        }
    }

    fn is_sep(self, c: char) -> bool {
        match self {
            Flavor::Windows => c == '\\' || c == '/',
            Flavor::Posix => c == '/',
        }
    }

    /// Like `ntpath.splitroot`: (drive, root, rest).
    pub fn splitroot(self, p: &str) -> (String, String, String) {
        if self == Flavor::Posix {
            // posixpath.splitroot: exactly two slashes are preserved.
            if !p.starts_with('/') {
                return (String::new(), String::new(), p.to_string());
            }
            if !p.starts_with("//") || p.starts_with("///") {
                return (String::new(), "/".into(), p[1..].to_string());
            }
            return (String::new(), "//".into(), p[2..].to_string());
        }
        let norm: String = p.replace('/', "\\");
        let chars: Vec<char> = p.chars().collect();
        let nchars: Vec<char> = norm.chars().collect();
        let slice = |a: usize, b: usize| chars[a.min(chars.len())..b.min(chars.len())].iter().collect::<String>();
        let rest = |a: usize| chars[a.min(chars.len())..].iter().collect::<String>();
        if nchars.first() == Some(&'\\') {
            if nchars.get(1) == Some(&'\\') {
                let head: String = nchars.iter().take(8).collect::<String>().to_uppercase();
                let start = if head == "\\\\?\\UNC\\" { 8 } else { 2 };
                let find = |from: usize| (from..nchars.len()).find(|&i| nchars[i] == '\\');
                let Some(index) = find(start) else { return (p.to_string(), String::new(), String::new()) };
                let Some(index2) = find(index + 1) else { return (p.to_string(), String::new(), String::new()) };
                return (slice(0, index2), slice(index2, index2 + 1), rest(index2 + 1));
            }
            return (String::new(), slice(0, 1), rest(1));
        }
        if nchars.get(1) == Some(&':') {
            if nchars.get(2) == Some(&'\\') {
                return (slice(0, 2), slice(2, 3), rest(3));
            }
            return (slice(0, 2), String::new(), rest(2));
        }
        (String::new(), String::new(), p.to_string())
    }

    /// Like `os.path.join(a, b)` for a part `b` without a drive.
    pub fn join(self, a: &str, b: &str) -> String {
        match self {
            Flavor::Posix => {
                if b.starts_with('/') {
                    b.to_string()
                } else if a.is_empty() || a.ends_with('/') {
                    format!("{a}{b}")
                } else {
                    format!("{a}/{b}")
                }
            }
            Flavor::Windows => self.join_windows(a, b),
        }
    }

    /// `ntpath.join(a, b)` in full, including drive changes.
    fn join_windows(self, a: &str, b: &str) -> String {
        let (mut rd, mut rr, mut rp) = self.splitroot(a);
        let (pd, pr, pp) = self.splitroot(b);
        if !pr.is_empty() {
            if !pd.is_empty() || rd.is_empty() {
                rd = pd;
            }
            rr = pr;
            rp = pp;
        } else if !pd.is_empty() && pd.to_lowercase() != rd.to_lowercase() {
            rd = pd;
            rr = pr;
            rp = pp;
        } else {
            if !pd.is_empty() {
                rd = pd;
            }
            if rp.chars().last().is_some_and(|c| !self.is_sep(c)) {
                rp.push('\\');
            }
            rp.push_str(&pp);
        }
        if !rp.is_empty() && rr.is_empty() && rd.chars().last().is_some_and(|c| !":\\/".contains(c)) {
            return format!("{rd}\\{rp}");
        }
        format!("{rd}{rr}{rp}")
    }

    /// Append several parts in sequence.
    pub fn join_all(self, a: &str, parts: &[&str]) -> String {
        parts.iter().fold(a.to_string(), |acc, p| self.join(&acc, p))
    }

    /// Like `os.path.split`.
    pub fn split(self, p: &str) -> (String, String) {
        match self {
            Flavor::Posix => {
                let i = p.rfind('/').map_or(0, |i| i + 1);
                let (head, tail) = p.split_at(i);
                let head =
                    if !head.is_empty() && head.chars().any(|c| c != '/') { head.trim_end_matches('/') } else { head };
                (head.to_string(), tail.to_string())
            }
            Flavor::Windows => {
                let (d, r, rest) = self.splitroot(p);
                let i = rest.char_indices().rev().find(|&(_, c)| self.is_sep(c)).map_or(0, |(i, c)| i + c.len_utf8());
                let (head, tail) = rest.split_at(i);
                (format!("{d}{r}{}", head.trim_end_matches(['\\', '/'])), tail.to_string())
            }
        }
    }

    pub fn dirname(self, p: &str) -> String {
        self.split(p).0
    }

    pub fn basename(self, p: &str) -> String {
        self.split(p).1
    }

    /// Like `os.path.normpath`.
    pub fn normpath(self, path: &str) -> String {
        match self {
            Flavor::Windows => {
                let path = path.replace('/', "\\");
                let (drive, root, rest) = self.splitroot(&path);
                let mut comps: Vec<&str> = Vec::new();
                for comp in rest.split('\\') {
                    if comp.is_empty() || comp == "." {
                        continue;
                    }
                    if comp == ".." {
                        if comps.last().is_some_and(|c| *c != "..") {
                            comps.pop();
                            continue;
                        }
                        if comps.is_empty() && !root.is_empty() {
                            continue;
                        }
                    }
                    comps.push(comp);
                }
                let prefix = format!("{drive}{root}");
                if prefix.is_empty() && comps.is_empty() {
                    return ".".into();
                }
                format!("{prefix}{}", comps.join("\\"))
            }
            Flavor::Posix => {
                if path.is_empty() {
                    return ".".into();
                }
                let initial = if path.starts_with("//") && !path.starts_with("///") {
                    2
                } else if path.starts_with('/') {
                    1
                } else {
                    0
                };
                let mut comps: Vec<&str> = Vec::new();
                for comp in path.split('/') {
                    if comp.is_empty() || comp == "." {
                        continue;
                    }
                    if comp != ".." || (initial == 0 && comps.is_empty()) || comps.last() == Some(&"..") {
                        comps.push(comp);
                    } else if !comps.is_empty() {
                        comps.pop();
                    }
                }
                let joined = format!("{}{}", "/".repeat(initial), comps.join("/"));
                if joined.is_empty() { ".".into() } else { joined }
            }
        }
    }

    /// Like `os.path.normcase`.
    pub fn normcase(self, p: &str) -> String {
        match self {
            Flavor::Windows => p.replace('/', "\\").to_lowercase(),
            Flavor::Posix => p.to_string(),
        }
    }

    /// Like `os.path.isabs` (as of Python 3.13).
    pub fn isabs(self, p: &str) -> bool {
        match self {
            Flavor::Posix => p.starts_with('/'),
            Flavor::Windows => {
                let norm = p.replace('/', "\\");
                if norm.starts_with("\\\\") {
                    return true;
                }
                let (d, r, _) = self.splitroot(p);
                !d.is_empty() && !r.is_empty()
            }
        }
    }

    /// Like `os.path.abspath`, with relative paths taken against `cwd`.
    pub fn abspath(self, p: &str, cwd: &str) -> String {
        if self.isabs(p) { self.normpath(p) } else { self.normpath(&self.join(cwd, p)) }
    }
}

#[cfg(test)]
mod tests {
    use super::Flavor::{Posix, Windows};

    #[test]
    fn join_keeps_mixed_separators() {
        assert_eq!(Windows.join("D:/Spiele/Daten", "Einstellungen"), "D:/Spiele/Daten\\Einstellungen");
        assert_eq!(Windows.join("D:/Spiele/Daten/", "x"), "D:/Spiele/Daten/x");
        assert_eq!(Windows.join("C:", "x"), "C:x");
        assert_eq!(Windows.join("", "x"), "x");
        assert_eq!(Windows.join("C:\\a", "F:\\b"), "F:\\b");
        assert_eq!(Windows.join("\\\\srv\\share", "x"), "\\\\srv\\share\\x");
        assert_eq!(Posix.join("/home/a", "Games"), "/home/a/Games");
        assert_eq!(Posix.join("/home/a/", "Games"), "/home/a/Games");
        assert_eq!(Posix.join("/home/a", "/etc"), "/etc");
    }

    #[test]
    fn split_like_python() {
        assert_eq!(Windows.dirname("D:\\foo\\LIVE\\"), "D:\\foo\\LIVE");
        assert_eq!(Windows.dirname("D:/foo/LIVE"), "D:/foo");
        assert_eq!(Windows.dirname("D:\\"), "D:\\");
        assert_eq!(Windows.basename("D:\\foo\\LIVE"), "LIVE");
        assert_eq!(Windows.dirname("LIVE"), "");
        assert_eq!(Posix.dirname("/a/b//"), "/a/b");
        assert_eq!(Posix.dirname("/a"), "/");
        assert_eq!(Posix.dirname("//a"), "//");
        assert_eq!(Posix.basename("/a/b"), "b");
    }

    #[test]
    fn normpath_like_python() {
        assert_eq!(Windows.normpath("D:/a/./b/../c/"), "D:\\a\\c");
        assert_eq!(Windows.normpath("\\..\\a"), "\\a");
        assert_eq!(Windows.normpath("..\\a"), "..\\a");
        assert_eq!(Windows.normpath(""), ".");
        assert_eq!(Posix.normpath("/a/./b/../c/"), "/a/c");
        assert_eq!(Posix.normpath("//a"), "//a");
        assert_eq!(Posix.normpath("///a"), "/a");
        assert_eq!(Posix.normpath("../a"), "../a");
        assert_eq!(Posix.normpath("/.."), "/");
    }

    #[test]
    fn normcase_and_abspath() {
        assert_eq!(Windows.normcase("D:/Daten"), "d:\\daten");
        assert_eq!(Posix.normcase("/Daten"), "/Daten");
        assert_eq!(Windows.abspath("x\\y", "C:\\w"), "C:\\w\\x\\y");
        assert_eq!(Posix.abspath("x/../y", "/w"), "/w/y");
    }
}
