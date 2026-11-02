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

//! Just enough of a zip reader for the archives of the translations: the
//! names in the central directory, in its order, and one entry, stored or
//! deflated. No ZIP64, no encryption (none of the sources uses them).

/// Why an archive could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BadZip(pub String);

impl std::fmt::Display for BadZip {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bad zip file: {}", self.0)
    }
}

impl std::error::Error for BadZip {}

/// One entry of the central directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    method: u16,
    crc: u32,
    compressed: usize,
    size: usize,
    local: usize,
}

fn u16_at(data: &[u8], at: usize) -> Result<u16, BadZip> {
    data.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or_else(|| BadZip("truncated".into()))
}

fn u32_at(data: &[u8], at: usize) -> Result<u32, BadZip> {
    data.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok_or_else(|| BadZip("truncated".into()))
}

/// The largest entry unpacked, against archives that grow without end.
pub const MAX_ENTRY: usize = 256 * 1024 * 1024;

/// The entries of an archive, in the order of its central directory.
///
/// # Errors
///
/// When the archive is not a zip file or is cut off.
pub fn entries(data: &[u8]) -> Result<Vec<Entry>, BadZip> {
    // The end record sits in the last 64 KiB plus its own 22 bytes.
    let from = data.len().saturating_sub(65_557);
    let end = (from..data.len().saturating_sub(21))
        .rev()
        .find(|&i| data[i..].starts_with(b"PK\x05\x06"))
        .ok_or_else(|| BadZip("no end record".into()))?;
    let count = u16_at(data, end + 10)? as usize;
    let mut at = u32_at(data, end + 16)? as usize;
    if u32_at(data, end + 16)? == u32::MAX {
        return Err(BadZip("zip64 is not supported".into()));
    }
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        if !data.get(at..).is_some_and(|d| d.starts_with(b"PK\x01\x02")) {
            return Err(BadZip("broken central directory".into()));
        }
        let flags = u16_at(data, at + 8)?;
        let method = u16_at(data, at + 10)?;
        let crc = u32_at(data, at + 16)?;
        let compressed = u32_at(data, at + 20)? as usize;
        let size = u32_at(data, at + 24)? as usize;
        let name_len = u16_at(data, at + 28)? as usize;
        let extra_len = u16_at(data, at + 30)? as usize;
        let comment_len = u16_at(data, at + 32)? as usize;
        let local = u32_at(data, at + 42)? as usize;
        let raw = data.get(at + 46..at + 46 + name_len).ok_or_else(|| BadZip("truncated name".into()))?;
        // Bit 11: the name is UTF-8; otherwise code page 437, which matters
        // only outside ASCII.
        let name = if flags & 0x800 != 0 {
            String::from_utf8_lossy(raw).into_owned()
        } else {
            raw.iter().map(|&b| if b < 0x80 { b as char } else { '\u{fffd}' }).collect()
        };
        out.push(Entry { name, method, crc, compressed, size, local });
        at += 46 + name_len + extra_len + comment_len;
    }
    Ok(out)
}

/// The content of one entry.
///
/// # Errors
///
/// When the entry is cut off, too large, encrypted or packed another way.
pub fn read(data: &[u8], entry: &Entry) -> Result<Vec<u8>, BadZip> {
    let at = entry.local;
    if !data.get(at..).is_some_and(|d| d.starts_with(b"PK\x03\x04")) {
        return Err(BadZip("broken local header".into()));
    }
    if u16_at(data, at + 6)? & 1 != 0 {
        return Err(BadZip("encrypted".into()));
    }
    let start = at + 30 + u16_at(data, at + 26)? as usize + u16_at(data, at + 28)? as usize;
    let packed = data.get(start..start + entry.compressed).ok_or_else(|| BadZip("truncated entry".into()))?;
    if entry.size > MAX_ENTRY {
        return Err(BadZip("entry too large".into()));
    }
    let content = match entry.method {
        0 => packed.to_vec(),
        8 => miniz_oxide::inflate::decompress_to_vec_with_limit(packed, MAX_ENTRY)
            .map_err(|e| BadZip(format!("inflate: {e:?}")))?,
        other => return Err(BadZip(format!("compression method {other}"))),
    };
    // A download cut off or spoilt on the way must not become the text file.
    if content.len() != entry.size || crc32(&content) != entry.crc {
        return Err(BadZip("checksum mismatch".into()));
    }
    Ok(content)
}

/// An archive written in memory: entries deflated (stored where that is not smaller), names in UTF-8, no
/// ZIP64. For the backup, whose files are far below the limits of the format; whatever passes them is refused.
pub struct Builder {
    out: Vec<u8>,
    central: Vec<u8>,
    count: usize,
    /// Modification time of every entry, in the DOS format (date, time).
    stamp: (u16, u16),
}

/// The DOS date and time of a local time (years from 1980; the format has two-second steps).
fn dos_stamp(year: u32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> (u16, u16) {
    let year = year.clamp(1980, 2107) - 1980;
    let date = (year << 9) | (month.clamp(1, 12) << 5) | day.clamp(1, 31);
    let time = (hour.min(23) << 11) | (minute.min(59) << 5) | (second.min(59) / 2);
    (date as u16, time as u16)
}

impl Builder {
    /// An empty archive whose entries carry this local time (year, month, day, hour, minute, second).
    pub fn new(local: (u32, u32, u32, u32, u32, u32)) -> Builder {
        let (y, mo, d, h, mi, s) = local;
        Builder { out: Vec::new(), central: Vec::new(), count: 0, stamp: dos_stamp(y, mo, d, h, mi, s) }
    }

    /// Adds a file.
    ///
    /// # Errors
    ///
    /// When the name, the content or the number of entries is beyond what the format holds.
    pub fn add(&mut self, name: &str, content: &[u8]) -> Result<(), BadZip> {
        if name.len() > usize::from(u16::MAX) || self.count >= usize::from(u16::MAX) {
            return Err(BadZip("too many entries or a name too long".into()));
        }
        let deflated = miniz_oxide::deflate::compress_to_vec(content, 6);
        let (method, packed): (u16, &[u8]) = if deflated.len() < content.len() { (8, &deflated) } else { (0, content) };
        let to_u32 = |n: usize| u32::try_from(n).map_err(|_| BadZip("entry too large".into()));
        let (size, compressed, local) = (to_u32(content.len())?, to_u32(packed.len())?, to_u32(self.out.len())?);
        let crc = crc32(content);
        // Bit 11: the name is UTF-8.
        let flags: u16 = 0x800;
        let (date, time) = self.stamp;
        self.out.extend_from_slice(b"PK\x03\x04");
        self.out.extend_from_slice(&20u16.to_le_bytes());
        self.out.extend_from_slice(&flags.to_le_bytes());
        self.out.extend_from_slice(&method.to_le_bytes());
        self.out.extend_from_slice(&time.to_le_bytes());
        self.out.extend_from_slice(&date.to_le_bytes());
        self.out.extend_from_slice(&crc.to_le_bytes());
        self.out.extend_from_slice(&compressed.to_le_bytes());
        self.out.extend_from_slice(&size.to_le_bytes());
        self.out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        self.out.extend_from_slice(&0u16.to_le_bytes());
        self.out.extend_from_slice(name.as_bytes());
        self.out.extend_from_slice(packed);
        self.central.extend_from_slice(b"PK\x01\x02");
        self.central.extend_from_slice(&20u16.to_le_bytes());
        self.central.extend_from_slice(&20u16.to_le_bytes());
        self.central.extend_from_slice(&flags.to_le_bytes());
        self.central.extend_from_slice(&method.to_le_bytes());
        self.central.extend_from_slice(&time.to_le_bytes());
        self.central.extend_from_slice(&date.to_le_bytes());
        self.central.extend_from_slice(&crc.to_le_bytes());
        self.central.extend_from_slice(&compressed.to_le_bytes());
        self.central.extend_from_slice(&size.to_le_bytes());
        self.central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        // Extra field, comment, disk, internal and external attributes: none.
        self.central.extend_from_slice(&[0; 12]);
        self.central.extend_from_slice(&local.to_le_bytes());
        self.central.extend_from_slice(name.as_bytes());
        self.count += 1;
        Ok(())
    }

    /// The archive with its central directory.
    ///
    /// # Errors
    ///
    /// When the archive is larger than the format holds.
    pub fn finish(mut self) -> Result<Vec<u8>, BadZip> {
        let offset = u32::try_from(self.out.len()).map_err(|_| BadZip("archive too large".into()))?;
        let size = u32::try_from(self.central.len()).map_err(|_| BadZip("archive too large".into()))?;
        self.out.extend_from_slice(&self.central);
        self.out.extend_from_slice(b"PK\x05\x06");
        self.out.extend_from_slice(&[0; 4]);
        self.out.extend_from_slice(&(self.count as u16).to_le_bytes());
        self.out.extend_from_slice(&(self.count as u16).to_le_bytes());
        self.out.extend_from_slice(&size.to_le_bytes());
        self.out.extend_from_slice(&offset.to_le_bytes());
        self.out.extend_from_slice(&0u16.to_le_bytes());
        Ok(self.out)
    }
}

/// The CRC-32 of zip (polynomial 0xEDB88320).
pub(crate) fn crc32(data: &[u8]) -> u32 {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut table = [0u32; 256];
        for (n, slot) in table.iter_mut().enumerate() {
            let mut c = n as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
            *slot = c;
        }
        table
    });
    !data.iter().fold(!0u32, |c, &b| table[((c ^ u32::from(b)) & 0xFF) as usize] ^ (c >> 8))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A zip archive of stored and deflated entries, built by hand.
    pub(crate) fn build(files: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, content, deflate) in files {
            let packed = if *deflate { miniz_oxide::deflate::compress_to_vec(content, 6) } else { content.to_vec() };
            let method: u16 = if *deflate { 8 } else { 0 };
            let local = out.len() as u32;
            let mut head = Vec::new();
            head.extend_from_slice(b"PK\x03\x04");
            head.extend_from_slice(&20u16.to_le_bytes());
            head.extend_from_slice(&0x800u16.to_le_bytes());
            head.extend_from_slice(&method.to_le_bytes());
            head.extend_from_slice(&[0; 4]);
            head.extend_from_slice(&crc32(content).to_le_bytes());
            head.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            head.extend_from_slice(&(content.len() as u32).to_le_bytes());
            head.extend_from_slice(&(name.len() as u16).to_le_bytes());
            head.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&head);
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&packed);
            central.extend_from_slice(b"PK\x01\x02");
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&0x800u16.to_le_bytes());
            central.extend_from_slice(&method.to_le_bytes());
            central.extend_from_slice(&[0; 4]);
            central.extend_from_slice(&crc32(content).to_le_bytes());
            central.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            central.extend_from_slice(&(content.len() as u32).to_le_bytes());
            central.extend_from_slice(&(name.len() as u16).to_le_bytes());
            central.extend_from_slice(&[0; 12]);
            central.extend_from_slice(&local.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let offset = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(b"PK\x05\x06");
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    #[test]
    fn stored_and_deflated_entries_come_back() {
        let big = "KEY=Text\n".repeat(1000);
        let zip = build(&[
            ("Data/Localization/german_(germany)/global.ini", big.as_bytes(), true),
            ("readme.txt", b"hi", false),
        ]);
        let list = entries(&zip).unwrap();
        assert_eq!(
            list.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            ["Data/Localization/german_(germany)/global.ini", "readme.txt"]
        );
        assert_eq!(read(&zip, &list[0]).unwrap(), big.as_bytes());
        assert_eq!(read(&zip, &list[1]).unwrap(), b"hi");
    }

    #[test]
    fn a_spoilt_entry_is_refused() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926, "the check value of CRC-32");
        let mut zip = build(&[("global.ini", b"KEY=Text\n", false)]);
        let entry = entries(&zip).unwrap().remove(0);
        let at = zip.windows(9).position(|w| w == b"KEY=Text\n").unwrap();
        zip[at] = b'X';
        assert_eq!(read(&zip, &entry), Err(BadZip("checksum mismatch".into())));
    }

    #[test]
    fn no_zip_is_an_error() {
        assert!(entries(b"PK not really").is_err());
        assert!(entries(b"").is_err());
    }
}
