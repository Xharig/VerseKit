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

//! Unpacking the Snappy block format, in which GitHub delivers an attestation behind its `bundle_url`
//! (`….json.sn`). Only unpacking; anything broken or larger than `limit` is `None`, never a panic.

/// The unpacked bytes of `input`, at most `limit` of them.
pub fn decode(input: &[u8], limit: usize) -> Option<Vec<u8>> {
    let (length, mut at) = varint(input)?;
    let length = usize::try_from(length).ok().filter(|n| *n <= limit)?;
    let mut out = Vec::with_capacity(length);
    while at < input.len() {
        let tag = input[at];
        at += 1;
        let (copy_length, offset) = match tag & 3 {
            0 => {
                let mut size = usize::from(tag >> 2);
                if size >= 60 {
                    let count = size - 59;
                    let bytes = input.get(at..at + count)?;
                    size = bytes.iter().rev().fold(0usize, |n, b| (n << 8) | usize::from(*b));
                    at += count;
                }
                let end = at.checked_add(size)?.checked_add(1)?;
                out.extend_from_slice(input.get(at..end)?);
                at = end;
                if out.len() > length {
                    return None;
                }
                continue;
            }
            1 => {
                let low = usize::from(*input.get(at)?);
                at += 1;
                (usize::from((tag >> 2) & 7) + 4, (usize::from(tag >> 5) << 8) | low)
            }
            2 => {
                let bytes = input.get(at..at + 2)?;
                at += 2;
                (usize::from(tag >> 2) + 1, usize::from(u16::from_le_bytes([bytes[0], bytes[1]])))
            }
            _ => {
                let bytes = input.get(at..at + 4)?;
                at += 4;
                let offset = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                (usize::from(tag >> 2) + 1, usize::try_from(offset).ok()?)
            }
        };
        if offset == 0 || offset > out.len() || out.len() + copy_length > length {
            return None;
        }
        // ⚠ Byte by byte: a copy may reach into what it is writing (offset smaller than the length).
        let start = out.len() - offset;
        for i in 0..copy_length {
            let byte = out[start + i];
            out.push(byte);
        }
    }
    (out.len() == length).then_some(out)
}

/// The unsigned number at the start of `input` (7 bits per byte, low first) and how many bytes it took.
fn varint(input: &[u8]) -> Option<(u64, usize)> {
    let mut value = 0u64;
    for (i, byte) in input.iter().enumerate().take(10) {
        value |= u64::from(byte & 0x7f) << (7 * i);
        if byte & 0x80 == 0 {
            return Some((value, i + 1));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn literals_and_copies_unpack() {
        // 11 bytes: the literal "abc" (tag 0x08 = three bytes), then a copy of 8 bytes from 3 back (tag 0x11).
        assert_eq!(decode(&[11, 0x08, b'a', b'b', b'c', 0x11, 3], 100).as_deref(), Some(&b"abcabcabcab"[..]));
        // A literal whose length stands in one byte of its own (tag 60 << 2): 61 bytes.
        let mut long = vec![61, 60 << 2, 60];
        long.extend(std::iter::repeat_n(b'x', 61));
        assert_eq!(decode(&long, 100).map(|v| v.len()), Some(61));
    }

    #[test]
    fn broken_input_is_none() {
        assert_eq!(decode(&[], 100), None);
        assert_eq!(decode(&[5, 0x08, b'a', b'b', b'c'], 100), None, "shorter than announced");
        assert_eq!(decode(&[3, 0x08, b'a', b'b', b'c'], 2), None, "above the limit");
        assert_eq!(decode(&[4, 0x00, b'a', 0x01, 9], 100), None, "a copy from before the start");
        assert_eq!(decode(&[4, 0x00, b'a', 0x01, 0], 100), None, "a copy from offset zero");
        assert_eq!(decode(&[3, 0x08, b'a'], 100), None, "a literal cut off");
        assert_eq!(decode(&[0xff; 12], 100), None, "a length that never ends");
    }
}
