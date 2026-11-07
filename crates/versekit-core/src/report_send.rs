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

//! Where the error report goes and in what shape (`report_target.py` and the
//! second half of `report.py`): the address, the body of the request, the
//! images and the prefilled issue form. The report is not saved as a file.
//!
//! ⚠ Nothing leaves by itself. The pieces here are called by a click on a
//! button, after the player ticked the box, and with the text the player saw.
//! The address is [`Target`]: the built-in one, another one for test runs
//! (`SC_BP_BERICHT_ZIEL`), or none. Only a run on the player's own data gets
//! one ([`Target::for_run`]); copies of the data, test runs and previews use
//! [`Target::off`] or a made-up server on the own computer ([`Target::local`]),
//! never the built-in one.

use crate::basetool::http::{Request, Transport};
use crate::browser;
use crate::language::{Lang, t_in};
use crate::paths::Host;
use crate::redact::RELAY;
use crate::text::py_strip;

/// The largest request the relay takes.
pub const SEND_LIMIT: usize = 8 * 1024 * 1024;
/// So many images go along at most, and so many bytes of images in all.
pub const IMAGE_LIMIT: usize = 4;
pub const IMAGE_BUDGET: usize = 7 * 1024 * 1024;
/// So many files go in one request at most: the report and nine more.
pub const FILES_MAX: usize = 9;
/// The longest the head line of the request is.
const HEAD_MAX: usize = 1900;

/// GitHub cuts very long addresses; the report is normally a good 1 KB.
pub const URL_LIMIT: usize = 6000;
pub const ISSUE_ADDRESS: &str = "https://github.com/Xharig/VerseKit/issues/new";

/// A file that goes along with the report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attachment {
    pub name: String,
    pub data: Vec<u8>,
    pub mime: String,
}

// ---------------------------------------------------------------- Target

/// Where a report is sent, if anywhere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target(Option<String>);

/// `http://` to the own computer only (a made-up server of the checks).
pub(crate) fn loopback_http(address: &str) -> bool {
    let Some(rest) = address.strip_prefix("http://") else { return false };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    // "127.0.0.1:x@other.example" names another computer. privacy-ok: example address
    if authority.contains('@') {
        return false;
    }
    let host = match authority.strip_prefix('[') {
        Some(inner) => inner.split(']').next().unwrap_or_default(),
        None => authority.split(':').next().unwrap_or_default(),
    };
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

impl Target {
    /// The address of a host: `SC_BP_BERICHT_ZIEL` if set (stripped), else
    /// the built-in relay. Only `https://` counts as a target; `http://` only
    /// to the own computer, for a made-up server.
    pub fn from_host(host: &Host) -> Target {
        let address = match host.var(crate::legacy::ENV_REPORT_TARGET) {
            Some(value) => py_strip(value).to_string(),
            None => RELAY.to_string(),
        };
        Target((address.starts_with("https://") || loopback_http(&address)).then_some(address))
    }

    /// The target of a run: [`Target::from_host`] on the player's own data
    /// (`own_data`), else none. A copy of the data and a data folder named from
    /// outside never send.
    pub fn for_run(host: &Host, own_data: bool) -> Target {
        if own_data { Target::from_host(host) } else { Target::off() }
    }

    /// No target: nothing can be sent.
    pub fn off() -> Target {
        Target(None)
    }

    /// A made-up server on the own computer; any other address gives no target.
    pub fn local(address: &str) -> Target {
        Target(loopback_http(address).then(|| address.to_string()))
    }

    pub fn available(&self) -> bool {
        self.0.is_some()
    }

    pub fn address(&self) -> Option<&str> {
        self.0.as_deref()
    }
}

// ---------------------------------------------------------------- Request body

/// The body of the request as bytes, so images fit in. `files[0]` is the
/// report, `files[1…]` the attachments (at most [`FILES_MAX`]).
pub fn multipart(boundary: &str, head: &str, name: &str, text: &str, attachments: &[Attachment]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"content\"\r\n\r\n{head}\r\n").as_bytes(),
    );
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"files[0]\"; filename=\"{name}\"\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(text.as_bytes());
    body.extend_from_slice(b"\r\n");
    for (index, file) in attachments.iter().take(FILES_MAX).enumerate() {
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"files[{}]\"; filename=\"{}\"\r\nContent-Type: {}\r\n\r\n",
                index + 1,
                file.name,
                file.mime
            )
            .as_bytes(),
        );
        body.extend_from_slice(&file.data);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    body
}

/// Why a report did not arrive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendFailure {
    /// No target in this version or switched off.
    NoTarget,
    /// More than [`SEND_LIMIT`] bytes, or the relay said so (413).
    TooLarge,
    /// The relay slows down senders (429).
    TooMany,
    /// Anything else. The reason is not passed on: the address may stand in it.
    Unreachable,
}

impl SendFailure {
    /// The text for the player.
    pub fn text(self, lang: Lang) -> String {
        t_in(
            lang,
            match self {
                SendFailure::NoTarget => "m_bericht_kein_ziel",
                SendFailure::TooLarge => "m_berichtv_zugross",
                SendFailure::TooMany => "m_berichtv_zuviel",
                SendFailure::Unreachable => "m_berichtv_weg",
            },
        )
    }
}

/// A random boundary of 32 hexadecimal digits.
fn boundary() -> String {
    let mut bytes = [0u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        // Without random numbers still unique enough for one request.
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        bytes = now.to_le_bytes();
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The name of the report file: `bericht-2026-10-21-0905.txt`.
pub fn file_name(stamp: &str) -> String {
    format!("bericht-{stamp}.txt")
}

/// The head line of the request.
pub fn head_line(version: &str) -> String {
    let head = format!("**Fehlerbericht** · {}", if version.is_empty() { "?" } else { version });
    head.chars().take(HEAD_MAX).collect()
}

/// Sends the report to the target: one request, 30 seconds at most.
///
/// `stamp` is `YYYY-MM-DD-HHMM` for the file name. The size is checked before
/// the upload, since the relay refuses larger requests.
pub fn submit(
    transport: &dyn Transport,
    target: &Target,
    text: &str,
    version: &str,
    stamp: &str,
    attachments: &[Attachment],
) -> Result<(), SendFailure> {
    let Some(address) = target.address() else { return Err(SendFailure::NoTarget) };
    let limit = boundary();
    let body = multipart(&limit, &head_line(version), &file_name(stamp), text, attachments);
    if body.len() > SEND_LIMIT {
        return Err(SendFailure::TooLarge);
    }
    let request = Request {
        method: "POST".into(),
        url: address.to_string(),
        headers: vec![
            ("Content-Type".into(), format!("multipart/form-data; boundary={limit}")),
            ("User-Agent".into(), "Verse-Kit".into()),
        ],
        body: Some(body),
    };
    match transport.send(&request) {
        Ok(answer) if (200..300).contains(&answer.status) => Ok(()),
        Ok(answer) => {
            crate::errors::record("report.submit", &format!("HTTP {}", answer.status));
            Err(match answer.status {
                429 => SendFailure::TooMany,
                413 => SendFailure::TooLarge,
                _ => SendFailure::Unreachable,
            })
        }
        Err(error) => {
            crate::errors::record("report.submit", &error.code);
            Err(SendFailure::Unreachable)
        }
    }
}

// ---------------------------------------------------------------- Images

pub const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
const JPEG_START: [u8; 3] = [0xff, 0xd8, 0xff];
/// Text fields, Exif and time stamps: not needed to show the image.
const PNG_METADATA: [&[u8; 4]; 5] = [b"tEXt", b"zTXt", b"iTXt", b"eXIf", b"tIME"];
/// APP0 (JFIF), APP2 (ICC profile) and APP14 (Adobe colors) the decoder
/// needs; the other APPn (Exif, XMP, IPTC …) and comments it does not.
const JPEG_KEEP_APP: [u8; 3] = [0xe0, 0xe2, 0xee];

/// What kind of image the first bytes say (not the file ending).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageKind {
    Png,
    Jpg,
}

impl ImageKind {
    pub fn extension(self) -> &'static str {
        match self {
            ImageKind::Png => "png",
            ImageKind::Jpg => "jpg",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            ImageKind::Png => "image/png",
            ImageKind::Jpg => "image/jpeg",
        }
    }
}

pub fn image_kind(data: &[u8]) -> Option<ImageKind> {
    if data.starts_with(&PNG_SIGNATURE) {
        Some(ImageKind::Png)
    } else if data.starts_with(&JPEG_START) {
        Some(ImageKind::Jpg)
    } else {
        None
    }
}

/// The PNG without text and Exif chunks; `None` if it is broken. Everything
/// after `IEND` goes: anything can be appended there.
fn png_without_metadata(data: &[u8]) -> Option<Vec<u8>> {
    let mut out = PNG_SIGNATURE.to_vec();
    let mut pos = PNG_SIGNATURE.len();
    while pos + 12 <= data.len() {
        let length = u32::from_be_bytes(data[pos..pos + 4].try_into().ok()?) as usize;
        let chunk: &[u8] = &data[pos + 4..pos + 8];
        let end = pos.checked_add(12)?.checked_add(length)?;
        if end > data.len() {
            return None;
        }
        if !PNG_METADATA.iter().any(|m| m.as_slice() == chunk) {
            out.extend_from_slice(&data[pos..end]);
        }
        pos = end;
        if chunk == b"IEND" {
            return Some(out);
        }
    }
    None
}

/// The JPEG without Exif/XMP/IPTC and comments; `None` if it is broken.
///
/// The image data after each SOS is taken up to the next real marker (`FF 00`
/// and `FF D0` to `D7` belong to the data). Everything after `EOI` goes: some
/// cameras append whole videos there.
fn jpeg_without_metadata(data: &[u8]) -> Option<Vec<u8>> {
    let mut out = data.get(..2)?.to_vec();
    let mut pos = 2usize;
    let size = data.len();
    while pos + 2 <= size {
        if data[pos] != 0xff {
            return None;
        }
        let marker = data[pos + 1];
        if marker == 0xff {
            pos += 1;
            continue;
        }
        if marker == 0xd9 {
            out.extend_from_slice(&data[pos..pos + 2]);
            return Some(out);
        }
        if (0xd0..=0xd7).contains(&marker) || marker == 0x01 {
            out.extend_from_slice(&data[pos..pos + 2]);
            pos += 2;
            continue;
        }
        if pos + 4 > size {
            return None;
        }
        let length = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
        let end = pos + 2 + length;
        if length < 2 || end > size {
            return None;
        }
        if marker == 0xda {
            let mut scan = end;
            let found = loop {
                let at = data.get(scan..)?.iter().position(|b| *b == 0xff)? + scan;
                if at + 1 >= size {
                    return None;
                }
                let follow = data[at + 1];
                if follow == 0x00 || (0xd0..=0xd7).contains(&follow) {
                    scan = at + 2;
                } else if follow == 0xff {
                    scan = at + 1;
                } else {
                    break at;
                }
            };
            out.extend_from_slice(&data[pos..found]);
            pos = found;
            continue;
        }
        let drop = marker == 0xfe || ((0xe0..=0xef).contains(&marker) && !JPEG_KEEP_APP.contains(&marker));
        if !drop {
            out.extend_from_slice(&data[pos..end]);
        }
        pos = end;
    }
    None
}

/// The image content unchanged, hidden additions removed (or `None`).
pub fn strip_metadata(data: &[u8], kind: ImageKind) -> Option<Vec<u8>> {
    match kind {
        ImageKind::Png => png_without_metadata(data),
        ImageKind::Jpg => jpeg_without_metadata(data),
    }
}

/// Why an image was not taken: the text key to show.
pub type ImageProblem = &'static str;

/// An image chosen by the player. Checked is the content, not the ending:
/// only real PNG and JPEG get through.
pub fn load_image(path: &str) -> Result<(Vec<u8>, ImageKind), ImageProblem> {
    use std::io::Read;
    let mut data = Vec::new();
    let read = std::fs::File::open(path).and_then(|f| f.take(IMAGE_BUDGET as u64 + 1).read_to_end(&mut data));
    if let Err(error) = read {
        crate::errors::record("report.load_image", &error);
        return Err("s_di_bild_unlesbar");
    }
    if data.len() > IMAGE_BUDGET {
        return Err("s_div_bild_zugross");
    }
    let kind = image_kind(&data);
    match kind.and_then(|k| strip_metadata(&data, k).map(|clean| (clean, k))) {
        Some(found) => Ok(found),
        None => Err("s_di_bild_art"),
    }
}

/// The chosen images as attachments. The names are given anew: the original
/// can carry the user name or a path.
pub fn image_files(images: &[(Vec<u8>, ImageKind)]) -> Vec<Attachment> {
    images
        .iter()
        .take(IMAGE_LIMIT)
        .enumerate()
        .map(|(number, (data, kind))| Attachment {
            name: format!("bild-{}.{}", number + 1, kind.extension()),
            data: data.clone(),
            mime: kind.mime().to_string(),
        })
        .collect()
}

// ---------------------------------------------------------------- Issue and file

/// `urllib.parse.quote_plus`: letters, digits and `_.-~` stay, a space is
/// `+`, everything else `%XX` per byte of UTF-8.
fn quote_plus(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'.' | b'-' | b'~' => out.push(byte as char),
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// The form of the issue: German interface, German form; otherwise the
/// English one.
///
/// ⚠ The two file names are fixed: every shipped version sends
/// `template=bug.yml` or `template=fehler.yml`; renaming one empties the
/// prefilled report for all older versions.
pub fn template_for(lang: Lang) -> &'static str {
    if lang == Lang::En { "bug.yml" } else { "fehler.yml" }
}

/// An address that opens a **prefilled** form at GitHub. The player reads it
/// and presses send himself; an issue cannot be created from the program
/// without a key, and a key in a program file is a key for everyone.
pub fn issue_url(text: &str, title: &str, template: &str, lang: Lang) -> String {
    let mut body: String = text.chars().take(URL_LIMIT).collect();
    if text.chars().count() > URL_LIMIT {
        body.push_str(&t_in(lang, "m_berichtv_gekuerzt"));
    }
    let mut query = format!("template={}&bericht={}", quote_plus(template), quote_plus(&body));
    if !title.is_empty() {
        query.push_str(&format!("&title={}", quote_plus(title)));
    }
    format!("{ISSUE_ADDRESS}?{query}")
}

/// Opens the prefilled form in the browser; true if it started.
pub fn open_issue(text: &str, title: &str, lang: Lang) -> bool {
    browser::open_in_browser(&issue_url(text, title, template_for(lang), lang))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basetool::error::ApiError;
    use crate::basetool::http::Response;
    use crate::pypath::Flavor;
    use std::sync::Mutex;

    #[test]
    fn the_target_follows_the_environment() {
        let with = |value: Option<&str>| {
            let vars: Vec<(&str, &str)> = value.map(|v| (crate::legacy::ENV_REPORT_TARGET, v)).into_iter().collect();
            Target::from_host(&Host::new(Flavor::Posix, &vars, "/"))
        };
        assert_eq!(with(None).address(), Some(RELAY));
        assert!(!with(Some("aus")).available());
        assert!(!with(Some("")).available());
        assert_eq!(with(Some("  https://example.org/x  ")).address(), Some("https://example.org/x"));
        assert!(!with(Some("http://example.org/x")).available(), "plain http only to the own computer");
        assert!(with(Some("http://127.0.0.1:9/x")).available());
        assert!(Target::local("http://localhost:1/x").available());
        assert!(!Target::local("https://example.org/x").available(), "a made-up server is on this computer");
        assert!(!Target::off().available());
        assert!(!Target::local("http://127.0.0.1:x@example.org/x").available(), "user part names another computer");
        assert!(!Target::local("http://localhost@example.org/").available());
        assert!(Target::local("http://[::1]:9/x").available());
    }

    #[test]
    fn only_a_run_on_the_players_own_data_sends() {
        let host = |vars: &[(&str, &str)]| Host::new(Flavor::Posix, vars, "/");
        assert_eq!(Target::for_run(&host(&[]), true).address(), Some(RELAY), "own data: the built-in relay");
        assert!(!Target::for_run(&host(&[]), false).available(), "a copy or a test run sends nothing");
        let local = [(crate::legacy::ENV_REPORT_TARGET, "http://127.0.0.1:9/x")];
        assert!(!Target::for_run(&host(&local), false).available(), "not even to a server named from outside");
        let off = [(crate::legacy::ENV_REPORT_TARGET, "aus")];
        assert!(!Target::for_run(&host(&off), true).available(), "the switch off holds on own data too");
    }

    #[test]
    fn the_body_has_the_report_first_and_the_images_after() {
        let images = vec![Attachment { name: "bild-1.png".into(), data: vec![1, 2, 3], mime: "image/png".into() }];
        let body = multipart("B", "kopf", "bericht-x.txt", "Text ä", &images);
        let mut expected = Vec::new();
        expected.extend_from_slice(b"--B\r\nContent-Disposition: form-data; name=\"content\"\r\n\r\nkopf\r\n");
        expected.extend_from_slice(b"--B\r\nContent-Disposition: form-data; name=\"files[0]\"; filename=\"bericht-x.txt\"\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n");
        expected.extend_from_slice("Text ä\r\n".as_bytes());
        expected.extend_from_slice(b"--B\r\nContent-Disposition: form-data; name=\"files[1]\"; filename=\"bild-1.png\"\r\nContent-Type: image/png\r\n\r\n");
        expected.extend_from_slice(&[1, 2, 3, b'\r', b'\n']);
        expected.extend_from_slice(b"--B--\r\n");
        assert_eq!(body, expected);
        assert_eq!(head_line(""), "**Fehlerbericht** · ?");
        assert_eq!(head_line("5.0.0"), "**Fehlerbericht** · 5.0.0");
    }

    struct Recording {
        answers: Mutex<Vec<Result<Response, ApiError>>>,
        seen: Mutex<Vec<Request>>,
    }

    impl Transport for Recording {
        fn send(&self, request: &Request) -> Result<Response, ApiError> {
            self.seen.lock().unwrap().push(request.clone());
            self.answers.lock().unwrap().remove(0)
        }
    }

    fn answer(status: u16) -> Result<Response, ApiError> {
        Ok(Response { status, ..Default::default() })
    }

    #[test]
    fn sending_maps_the_answers() {
        let target = Target::local("http://127.0.0.1:9/bericht");
        let transport = Recording {
            answers: Mutex::new(vec![
                answer(200),
                answer(204),
                answer(429),
                answer(413),
                answer(500),
                Err(ApiError::network("x")),
            ]),
            seen: Mutex::new(Vec::new()),
        };
        let send = || submit(&transport, &target, "Verse-Kit 5 text", "5.0.0", "2026-10-21-0905", &[]);
        assert_eq!(send(), Ok(()));
        assert_eq!(send(), Ok(()));
        assert_eq!(send(), Err(SendFailure::TooMany));
        assert_eq!(send(), Err(SendFailure::TooLarge));
        assert_eq!(send(), Err(SendFailure::Unreachable));
        assert_eq!(send(), Err(SendFailure::Unreachable));
        let seen = transport.seen.lock().unwrap();
        assert_eq!(seen[0].method, "POST");
        assert_eq!(seen[0].url, "http://127.0.0.1:9/bericht");
        assert!(
            seen[0].headers.iter().any(|(k, v)| k == "Content-Type" && v.starts_with("multipart/form-data; boundary="))
        );
        let body = String::from_utf8_lossy(seen[0].body.as_deref().unwrap_or_default()).into_owned();
        assert!(body.contains("filename=\"bericht-2026-10-21-0905.txt\""));
    }

    #[test]
    fn without_a_target_or_too_large_nothing_goes_out() {
        let transport = Recording { answers: Mutex::new(Vec::new()), seen: Mutex::new(Vec::new()) };
        assert_eq!(submit(&transport, &Target::off(), "x", "5", "s", &[]), Err(SendFailure::NoTarget));
        let big = Attachment { name: "bild-1.png".into(), data: vec![0; SEND_LIMIT], mime: "image/png".into() };
        let target = Target::local("http://127.0.0.1:9/bericht");
        assert_eq!(submit(&transport, &target, "x", "5", "s", &[big]), Err(SendFailure::TooLarge));
        assert!(transport.seen.lock().unwrap().is_empty(), "nothing was sent");
    }

    #[test]
    fn png_metadata_goes_and_the_image_stays() {
        fn chunk(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
            let mut out = (body.len() as u32).to_be_bytes().to_vec();
            out.extend_from_slice(kind);
            out.extend_from_slice(body);
            out.extend_from_slice(&[0, 0, 0, 0]);
            out
        }
        let mut png = PNG_SIGNATURE.to_vec();
        for part in [
            chunk(b"IHDR", &[0; 13]),
            chunk(b"tEXt", b"Ort=Zuhause"),
            chunk(b"IDAT", &[1, 2]),
            chunk(b"eXIf", b"x"),
            chunk(b"IEND", &[]),
        ] {
            png.extend(part);
        }
        png.extend_from_slice(b"angehaengt");
        let clean = strip_metadata(&png, ImageKind::Png).unwrap();
        assert!(!clean.windows(4).any(|w| w == b"tEXt" || w == b"eXIf" || w == b"ange"));
        assert!(clean.windows(4).any(|w| w == b"IDAT"));
        assert_eq!(strip_metadata(&png[..40], ImageKind::Png), None, "a broken file is refused");
    }

    #[test]
    fn jpeg_exif_goes_and_the_scan_stays() {
        let mut jpeg = vec![0xff, 0xd8];
        jpeg.extend_from_slice(&[0xff, 0xe1, 0x00, 0x06, b'E', b'x', b'i', b'f']); // APP1
        jpeg.extend_from_slice(&[0xff, 0xe0, 0x00, 0x04, 1, 2]); // APP0 stays
        jpeg.extend_from_slice(&[0xff, 0xfe, 0x00, 0x04, b'h', b'i']); // comment
        jpeg.extend_from_slice(&[0xff, 0xda, 0x00, 0x03, 0x00]); // SOS
        jpeg.extend_from_slice(&[9, 8, 0xff, 0x00, 7]); // data with a stuffed byte
        jpeg.extend_from_slice(&[0xff, 0xd9, b'v', b'i', b'd', b'e', b'o']);
        let clean = strip_metadata(&jpeg, ImageKind::Jpg).unwrap();
        let expected: Vec<u8> = [
            &[0xff, 0xd8][..],
            &[0xff, 0xe0, 0x00, 0x04, 1, 2],
            &[0xff, 0xda, 0x00, 0x03, 0x00],
            &[9, 8, 0xff, 0x00, 7],
            &[0xff, 0xd9],
        ]
        .concat();
        assert_eq!(clean, expected);
        assert_eq!(strip_metadata(&jpeg[..20], ImageKind::Jpg), None);
    }

    #[test]
    fn the_issue_address_is_built_like_urlencode() {
        let url = issue_url("a b/ä\n", "T", "fehler.yml", Lang::De);
        assert_eq!(url, format!("{ISSUE_ADDRESS}?template=fehler.yml&bericht=a+b%2F%C3%A4%0A&title=T"));
        let long = "x".repeat(URL_LIMIT + 1);
        let cut = issue_url(&long, "", "bug.yml", Lang::En);
        assert!(cut.contains("template=bug.yml") && !cut.contains("title="));
        assert_eq!(template_for(Lang::En), "bug.yml");
        assert_eq!(template_for(Lang::De), "fehler.yml");
    }
}
