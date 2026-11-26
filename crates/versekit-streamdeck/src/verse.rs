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

//! The plugin's side of the interface to VerseKit: the key file read anew whenever a request fails, one request per
//! connection, answers of at most [`MAX_ANSWER`] bytes.

use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

use serde_json::Value;
use versekit_core::paths::Paths;
use versekit_core::streamdeck::{self, Command, KeyFile, State};

/// The most bytes of an answer of VerseKit.
pub const MAX_ANSWER: usize = 2 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(2);

/// Why VerseKit gave no answer to use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Missing {
    /// No key file: VerseKit does not run or its interface is off.
    NoKeyFile,
    /// Nobody answers on the port of the key file.
    Unreachable,
    /// The key was refused, or the interface is locked.
    Refused(u16),
    /// VerseKit refused the command, with its code.
    Command(u16, String),
    /// The answer was not one of the interface.
    Broken,
}

/// What the plugin asks VerseKit; a test puts a fake in its place.
pub trait Verse {
    /// The state; `Ok(None)` when it is still the one of revision `since`.
    ///
    /// # Errors
    ///
    /// [`Missing`].
    fn state(&mut self, since: Option<u64>) -> Result<Option<State>, Missing>;
    /// Sends a command.
    ///
    /// # Errors
    ///
    /// [`Missing`].
    fn command(&mut self, command: &Command) -> Result<(), Missing>;
    /// The bindings of the game actions.
    ///
    /// # Errors
    ///
    /// [`Missing`].
    fn bindings(&mut self) -> Result<Value, Missing>;
}

/// VerseKit over HTTP on `127.0.0.1`.
pub struct Http {
    paths: Paths,
    key: Option<KeyFile>,
}

impl Http {
    pub fn new(paths: Paths) -> Http {
        Http { paths, key: None }
    }

    fn key(&mut self) -> Result<KeyFile, Missing> {
        if self.key.is_none() {
            self.key = streamdeck::read_key_file(&self.paths);
        }
        self.key.clone().ok_or(Missing::NoKeyFile)
    }

    /// One request; the key file is read anew after a failure (VerseKit may have started again on another port).
    fn request(&mut self, method: &str, path: &str, body: Option<&str>) -> Result<(u16, String), Missing> {
        let key = self.key()?;
        let result = exchange(&key, method, path, body);
        if matches!(result, Err(Missing::Unreachable) | Ok((401 | 429, _))) {
            self.key = None;
        }
        result
    }
}

/// Sends one request and reads the whole answer.
///
/// # Errors
///
/// [`Missing::Unreachable`] or [`Missing::Broken`].
pub fn exchange(key: &KeyFile, method: &str, path: &str, body: Option<&str>) -> Result<(u16, String), Missing> {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, key.port));
    let mut stream = TcpStream::connect_timeout(&address, TIMEOUT).map_err(|_| Missing::Unreachable)?;
    let _ = stream.set_read_timeout(Some(TIMEOUT));
    let _ = stream.set_write_timeout(Some(TIMEOUT));
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\nConnection: close\r\n",
        key.port, key.key
    );
    if let Some(body) = body {
        request.push_str(&format!("Content-Type: application/json\r\nContent-Length: {}\r\n", body.len()));
    }
    request.push_str("\r\n");
    if let Some(body) = body {
        request.push_str(body);
    }
    stream.write_all(request.as_bytes()).map_err(|_| Missing::Unreachable)?;
    let answer = read_limited(&mut stream, MAX_ANSWER).map_err(|_| Missing::Broken)?;
    parse_answer(&answer).ok_or(Missing::Broken)
}

/// Status and body of an HTTP answer.
pub fn parse_answer(bytes: &[u8]) -> Option<(u16, String)> {
    let end = bytes.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&bytes[..end]).ok()?;
    let status = head.split(' ').nth(1)?.parse().ok()?;
    let body = String::from_utf8(bytes[end + 4..].to_vec()).ok()?;
    Some((status, body))
}

fn code_of(body: &str) -> String {
    serde_json::from_str::<Value>(body).ok().and_then(|v| v["code"].as_str().map(String::from)).unwrap_or_default()
}

impl Verse for Http {
    fn state(&mut self, since: Option<u64>) -> Result<Option<State>, Missing> {
        let path = match since {
            Some(rev) => format!("{}?seit={rev}", streamdeck::PATH_STATE),
            None => streamdeck::PATH_STATE.to_string(),
        };
        match self.request("GET", &path, None)? {
            (304, _) => Ok(None),
            (200, body) => State::parse(&body).map(Some).ok_or(Missing::Broken),
            (status, _) => Err(Missing::Refused(status)),
        }
    }

    fn command(&mut self, command: &Command) -> Result<(), Missing> {
        match self.request("POST", streamdeck::PATH_COMMAND, Some(&streamdeck::command_body(command)))? {
            (200, _) => Ok(()),
            (401 | 429, body) if code_of(&body) != "zu_oft" => Err(Missing::Refused(401)),
            (status, body) => Err(Missing::Command(status, code_of(&body))),
        }
    }

    fn bindings(&mut self) -> Result<Value, Missing> {
        match self.request("GET", streamdeck::PATH_BINDINGS, None)? {
            (200, body) => serde_json::from_str(&body).map_err(|_| Missing::Broken),
            (status, _) => Err(Missing::Refused(status)),
        }
    }
}

/// Reads to the end, but refuses more than `limit` bytes.
///
/// # Errors
///
/// The reading failed or there was more.
pub fn read_limited(input: &mut impl Read, limit: usize) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    input.take(limit as u64 + 1).read_to_end(&mut out)?;
    if out.len() > limit {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "answer too large"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use versekit_core::streamdeck_server::{self, Host, Refused, Server};

    const KEY: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

    struct Fake;

    impl Host for Fake {
        fn state(&self) -> serde_json::Map<String, Value> {
            let mut map = serde_json::Map::new();
            map.insert("sprache".into(), Value::from("en"));
            map
        }
        fn command(&self, command: &Command) -> Result<(), Refused> {
            match command {
                Command::Page(_) => Err(Refused { status: 409, code: "kein_hauptfenster" }),
                _ => Ok(()),
            }
        }
        fn bindings(&self) -> Value {
            serde_json::json!({"aktionen": [{"id": "v_eject"}]})
        }
    }

    fn real_server() -> KeyFile {
        let listener = streamdeck_server::bind().expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let server = Server::new(KEY.into());
        std::thread::spawn(move || streamdeck_server::serve(listener, server, Arc::new(Fake)));
        KeyFile { port, key: KEY.into(), pid: 1 }
    }

    #[test]
    fn the_plugin_talks_to_the_real_server() {
        let key = real_server();
        let (status, body) = exchange(&key, "GET", streamdeck::PATH_STATE, None).expect("answer");
        assert_eq!(status, 200);
        let state = State::parse(&body).expect("state");
        assert_eq!(state.language(), versekit_core::language::Lang::En);
        let rev = state.rev;
        let (status, _) = exchange(&key, "GET", &format!("/v1/zustand?seit={rev}"), None).expect("answer");
        assert_eq!(status, 304);
        let body = streamdeck::command_body(&Command::Overlay);
        assert_eq!(exchange(&key, "POST", streamdeck::PATH_COMMAND, Some(&body)).expect("answer").0, 200);
        let page = streamdeck::command_body(&Command::Page("hf_bank".into()));
        let (status, body) = exchange(&key, "POST", streamdeck::PATH_COMMAND, Some(&page)).expect("answer");
        assert_eq!((status, code_of(&body)), (409, "kein_hauptfenster".into()));
        let wrong = KeyFile { key: KEY.replace('f', "0"), ..key };
        assert_eq!(exchange(&wrong, "GET", streamdeck::PATH_STATE, None).expect("answer").0, 401);
    }

    #[test]
    fn nobody_on_the_port_is_unreachable() {
        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind");
        let port = listener.local_addr().expect("addr").port();
        drop(listener);
        let key = KeyFile { port, key: KEY.into(), pid: 1 };
        assert_eq!(exchange(&key, "GET", streamdeck::PATH_STATE, None), Err(Missing::Unreachable));
    }

    #[test]
    fn an_answer_over_the_limit_is_not_read_whole() {
        let big = vec![b'a'; 100];
        assert!(read_limited(&mut big.as_slice(), 99).is_err());
        assert_eq!(read_limited(&mut big.as_slice(), 100).expect("fits").len(), 100);
    }

    #[test]
    fn an_answer_is_split_into_status_and_body() {
        assert_eq!(parse_answer(b"HTTP/1.1 200 OK\r\nX: y\r\n\r\n{}"), Some((200, "{}".into())));
        assert_eq!(parse_answer(b"kein http"), None);
    }
}
