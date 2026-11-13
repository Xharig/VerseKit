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

//! The few pieces of DER an X.509 certificate of Sigstore needs: elements, the certificate's fields, a public key,
//! a time. Reads only; anything unexpected is `None`, never a panic.

pub const SEQUENCE: u8 = 0x30;
pub const OID: u8 = 0x06;
pub const OCTET_STRING: u8 = 0x04;
pub const BIT_STRING: u8 = 0x03;
pub const BOOLEAN: u8 = 0x01;
pub const UTF8_STRING: u8 = 0x0c;
pub const UTC_TIME: u8 = 0x17;
pub const GENERALIZED_TIME: u8 = 0x18;
/// `[0]` explicit: the version of a certificate.
pub const CONTEXT_0: u8 = 0xa0;
/// `[3]` explicit: the extensions of a certificate.
pub const CONTEXT_3: u8 = 0xa3;

/// 1.2.840.10045.2.1, an elliptic curve key.
pub const OID_EC_KEY: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01];
/// 1.2.840.10045.3.1.7, the curve P-256.
pub const OID_P256: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07];
/// 1.3.132.0.34, the curve P-384.
pub const OID_P384: &[u8] = &[0x2b, 0x81, 0x04, 0x00, 0x22];
/// 1.2.840.10045.4.3.3, ECDSA with SHA-384.
pub const OID_ECDSA_SHA384: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03, 0x03];

/// One element: its tag, its content, and the whole of it with tag and length.
#[derive(Clone, Copy, Debug)]
pub struct Tlv<'a> {
    pub tag: u8,
    pub body: &'a [u8],
    pub whole: &'a [u8],
}

/// The first element of `input` and what follows it. Only the definite length, at most four length bytes.
pub fn read(input: &[u8]) -> Option<(Tlv<'_>, &[u8])> {
    let tag = *input.first()?;
    let first = *input.get(1)?;
    let (length, head) = if first < 0x80 {
        (usize::from(first), 2)
    } else {
        let count = usize::from(first & 0x7f);
        if count == 0 || count > 4 {
            return None;
        }
        let bytes = input.get(2..2 + count)?;
        (bytes.iter().fold(0usize, |n, b| (n << 8) | usize::from(*b)), 2 + count)
    };
    let end = head.checked_add(length)?;
    let body = input.get(head..end)?;
    Some((Tlv { tag, body, whole: input.get(..end)? }, input.get(end..)?))
}

/// The first element of `input`, which must carry `tag`, and what follows it.
pub fn expect(input: &[u8], tag: u8) -> Option<(Tlv<'_>, &[u8])> {
    read(input).filter(|(tlv, _)| tlv.tag == tag)
}

/// All elements one after the other in `input`.
pub fn elements(mut input: &[u8]) -> Option<Vec<Tlv<'_>>> {
    let mut out = Vec::new();
    while !input.is_empty() {
        let (tlv, rest) = read(input)?;
        out.push(tlv);
        input = rest;
    }
    Some(out)
}

/// The bits of a BIT STRING without its count of unused bits, which must be zero.
pub fn bits<'a>(tlv: &Tlv<'a>) -> Option<&'a [u8]> {
    match tlv.body.split_first() {
        Some((0, rest)) if tlv.tag == BIT_STRING => Some(rest),
        _ => None,
    }
}

/// An elliptic curve public key (`SubjectPublicKeyInfo`): the OID of its curve and the point.
pub fn ec_public_key(spki: &[u8]) -> Option<(&[u8], &[u8])> {
    let (outer, _) = expect(spki, SEQUENCE)?;
    let (algorithm, rest) = expect(outer.body, SEQUENCE)?;
    let (kind, after) = expect(algorithm.body, OID)?;
    let (curve, _) = expect(after, OID)?;
    if kind.body != OID_EC_KEY {
        return None;
    }
    let (key, _) = expect(rest, BIT_STRING)?;
    Some((curve.body, bits(&key)?))
}

/// The fields of a certificate this check needs.
#[derive(Clone, Debug)]
pub struct Certificate<'a> {
    /// The signed part, exactly as signed.
    pub signed: &'a [u8],
    pub algorithm: &'a [u8],
    pub signature: &'a [u8],
    /// Seconds since 1970.
    pub not_before: i64,
    pub not_after: i64,
    /// The `SubjectPublicKeyInfo` as a whole.
    pub public_key: &'a [u8],
    /// The extensions: OID and the content of their OCTET STRING.
    pub extensions: Vec<(&'a [u8], &'a [u8])>,
}

/// Read a certificate.
pub fn certificate(der: &[u8]) -> Option<Certificate<'_>> {
    let (outer, _) = expect(der, SEQUENCE)?;
    let (tbs, rest) = expect(outer.body, SEQUENCE)?;
    let (algorithm, rest) = expect(rest, SEQUENCE)?;
    let (algorithm, _) = expect(algorithm.body, OID)?;
    let (signature, _) = expect(rest, BIT_STRING)?;
    let mut fields = elements(tbs.body)?.into_iter().peekable();
    if fields.peek().is_some_and(|f| f.tag == CONTEXT_0) {
        fields.next();
    }
    let _serial = fields.next()?;
    let _algorithm = fields.next()?;
    let _issuer = fields.next()?;
    let validity = fields.next().filter(|f| f.tag == SEQUENCE)?;
    let _subject = fields.next()?;
    let public_key = fields.next().filter(|f| f.tag == SEQUENCE)?;
    let mut extensions = Vec::new();
    for field in fields.filter(|f| f.tag == CONTEXT_3) {
        let (list, _) = expect(field.body, SEQUENCE)?;
        for extension in elements(list.body)? {
            let (oid, rest) = expect(extension.body, OID)?;
            let rest = match expect(rest, BOOLEAN) {
                Some((_, after)) => after,
                None => rest,
            };
            let (value, _) = expect(rest, OCTET_STRING)?;
            extensions.push((oid.body, value.body));
        }
    }
    let times = elements(validity.body)?;
    Some(Certificate {
        signed: tbs.whole,
        algorithm: algorithm.body,
        signature: bits(&signature)?,
        not_before: time(times.first()?)?,
        not_after: time(times.get(1)?)?,
        public_key: public_key.whole,
        extensions,
    })
}

/// A UTCTime or GeneralizedTime in UTC (`…Z`, whole seconds) as seconds since 1970.
pub fn time(tlv: &Tlv<'_>) -> Option<i64> {
    let text = std::str::from_utf8(tlv.body).ok()?.strip_suffix('Z')?;
    let (year, rest) = match tlv.tag {
        UTC_TIME => {
            let short: i64 = text.get(..2)?.parse().ok()?;
            (if short < 50 { 2000 + short } else { 1900 + short }, text.get(2..)?)
        }
        GENERALIZED_TIME => (text.get(..4)?.parse().ok()?, text.get(4..)?),
        _ => return None,
    };
    if rest.len() != 10 || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let number = |at: usize| rest.get(at..at + 2).and_then(|n| n.parse::<i64>().ok());
    let (month, day, hour, minute, second) = (number(0)?, number(2)?, number(4)?, number(6)?, number(8)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second)
}

/// Days since 1970-01-01 of a date of the Gregorian calendar.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year - era * 400;
    let of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let of_cycle = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    era * 146_097 + of_cycle - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_count_from_1970() {
        let utc = |text: &str| time(&Tlv { tag: UTC_TIME, body: text.as_bytes(), whole: &[] });
        let general = |text: &str| time(&Tlv { tag: GENERALIZED_TIME, body: text.as_bytes(), whole: &[] });
        assert_eq!(utc("700101000000Z"), Some(0));
        assert_eq!(utc("261026191815Z"), Some(1_793_042_295));
        assert_eq!(general("20261026191815Z"), Some(1_793_042_295));
        assert_eq!(general("20000229120000Z"), Some(951_825_600), "a leap day");
        assert_eq!(utc("261026191815"), None, "without Z");
        assert_eq!(utc("261326191815Z"), None, "month 13");
        assert_eq!(utc("2610261918Z"), None, "too short");
    }

    #[test]
    fn broken_lengths_are_no_element() {
        assert!(read(&[]).is_none());
        assert!(read(&[0x30]).is_none());
        assert!(read(&[0x30, 0x05, 0x01]).is_none(), "longer than what is there");
        assert!(read(&[0x30, 0x80, 0x00, 0x00]).is_none(), "indefinite length");
        assert!(read(&[0x30, 0x85, 1, 1, 1, 1, 1]).is_none(), "five length bytes");
        let (tlv, rest) = read(&[0x04, 0x81, 0x01, 0xaa, 0x05]).expect("long form");
        assert_eq!((tlv.tag, tlv.body, rest), (0x04, &[0xaa][..], &[0x05][..]));
        assert!(certificate(&[0x30, 0x00]).is_none());
    }
}
