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

//! A small WebSocket client (RFC 6455) for the connection to the Stream Deck program on `127.0.0.1`, behind the
//! trait [`DeckLink`] so that another implementation can take its place.
//!
//! What it accepts from the other side:
//! * text frames, their continuation, ping (answered with pong), pong, close (answered with close);
//! * frames of at most [`MAX_FRAME`] bytes and messages of at most [`MAX_MESSAGE`] bytes.
//!
//! What ends the connection: a binary frame, a masked frame (the server never masks), a reserved bit or opcode, a
//! control frame longer than 125 bytes or fragmented, a frame or message over its limit, text that is not UTF-8.

use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, TcpStream};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

/// The most bytes of one frame from the other side.
pub const MAX_FRAME: usize = 1024 * 1024;
/// The most bytes of one message (all its frames).
pub const MAX_MESSAGE: usize = 1024 * 1024;
const MAX_HANDSHAKE: usize = 8 * 1024;
const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// A connection that carries text messages both ways.
pub trait DeckLink: Send {
    /// Sends one text message.
    ///
    /// # Errors
    ///
    /// The connection broke.
    fn send_text(&mut self, text: &str) -> io::Result<()>;
    /// Waits for the next text message; `None` when the other side closed.
    ///
    /// # Errors
    ///
    /// The connection broke or the other side broke the rules.
    fn recv_text(&mut self) -> io::Result<Option<String>>;
}

fn broken(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why.to_string())
}

/// The value of `Sec-WebSocket-Accept` for a key.
pub fn accept_value(key: &str) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY, format!("{key}{GUID}").as_bytes());
    STANDARD.encode(digest.as_ref())
}

/// A frame as it goes out from the client: always masked.
pub fn client_frame(opcode: u8, payload: &[u8], mask: [u8; 4]) -> Vec<u8> {
    let mut out = vec![0x80 | opcode];
    let len = payload.len();
    if len < 126 {
        out.push(0x80 | len as u8);
    } else if len <= 0xFFFF {
        out.push(0x80 | 126);
        out.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        out.push(0x80 | 127);
        out.extend_from_slice(&(len as u64).to_be_bytes());
    }
    out.extend_from_slice(&mask);
    out.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
    out
}

/// One frame read from the other side.
#[derive(Debug, PartialEq, Eq)]
pub struct Frame {
    pub fin: bool,
    pub opcode: u8,
    pub payload: Vec<u8>,
}

/// Reads one frame and checks it against the rules of a client.
///
/// # Errors
///
/// The connection broke or the frame broke a rule.
pub fn read_frame(input: &mut impl Read) -> io::Result<Frame> {
    let mut head = [0u8; 2];
    input.read_exact(&mut head)?;
    let fin = head[0] & 0x80 != 0;
    if head[0] & 0x70 != 0 {
        return Err(broken("reserved bits"));
    }
    let opcode = head[0] & 0x0F;
    if head[1] & 0x80 != 0 {
        return Err(broken("masked frame from the server"));
    }
    let mut len = u64::from(head[1] & 0x7F);
    if len == 126 {
        let mut b = [0u8; 2];
        input.read_exact(&mut b)?;
        len = u64::from(u16::from_be_bytes(b));
    } else if len == 127 {
        let mut b = [0u8; 8];
        input.read_exact(&mut b)?;
        len = u64::from_be_bytes(b);
    }
    match opcode {
        0x0 | 0x1 => {}
        0x8..=0xA => {
            if len > 125 || !fin {
                return Err(broken("control frame too long or fragmented"));
            }
        }
        0x2 => return Err(broken("binary frame")),
        _ => return Err(broken("unknown opcode")),
    }
    if len > MAX_FRAME as u64 {
        return Err(broken("frame too large"));
    }
    let mut payload = vec![0u8; len as usize];
    input.read_exact(&mut payload)?;
    Ok(Frame { fin, opcode, payload })
}

/// The WebSocket client over a TCP connection.
pub struct WsClient {
    stream: TcpStream,
    closed: bool,
}

impl WsClient {
    /// Connects to `ws://127.0.0.1:<port>/` and does the handshake.
    ///
    /// # Errors
    ///
    /// The connection or the handshake failed.
    pub fn connect(port: u16) -> io::Result<WsClient> {
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port))?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        let mut nonce = [0u8; 16];
        getrandom::fill(&mut nonce).map_err(|_| broken("no random numbers"))?;
        let key = STANDARD.encode(nonce);
        let request = format!(
            "GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        );
        stream.write_all(request.as_bytes())?;
        let mut answer = Vec::new();
        let mut byte = [0u8; 1];
        while !answer.ends_with(b"\r\n\r\n") {
            if answer.len() > MAX_HANDSHAKE {
                return Err(broken("handshake too long"));
            }
            if stream.read(&mut byte)? == 0 {
                return Err(broken("closed during the handshake"));
            }
            answer.push(byte[0]);
        }
        let text = String::from_utf8_lossy(&answer);
        check_handshake(&text, &key)?;
        stream.set_read_timeout(None)?;
        Ok(WsClient { stream, closed: false })
    }

    fn write_frame(&mut self, opcode: u8, payload: &[u8]) -> io::Result<()> {
        let mut mask = [0u8; 4];
        getrandom::fill(&mut mask).map_err(|_| broken("no random numbers"))?;
        self.stream.write_all(&client_frame(opcode, payload, mask))
    }

    /// A second handle on the same connection, for a writer in another thread.
    ///
    /// # Errors
    ///
    /// The system could not copy the handle.
    pub fn try_clone(&self) -> io::Result<WsClient> {
        Ok(WsClient { stream: self.stream.try_clone()?, closed: self.closed })
    }
}

/// Checks the answer of the server to the handshake.
///
/// # Errors
///
/// Not `101`, no upgrade to websocket, or the wrong accept value.
pub fn check_handshake(text: &str, key: &str) -> io::Result<()> {
    let mut lines = text.split("\r\n");
    let status = lines.next().unwrap_or("");
    if status.split(' ').nth(1) != Some("101") {
        return Err(broken("no switch of protocols"));
    }
    let mut upgrade = false;
    let mut accept = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
        match name.trim().to_ascii_lowercase().as_str() {
            "upgrade" => upgrade = value.trim().eq_ignore_ascii_case("websocket"),
            "sec-websocket-accept" => accept = Some(value.trim().to_string()),
            _ => {}
        }
    }
    if !upgrade || accept.as_deref() != Some(accept_value(key).as_str()) {
        return Err(broken("wrong handshake"));
    }
    Ok(())
}

impl DeckLink for WsClient {
    fn send_text(&mut self, text: &str) -> io::Result<()> {
        self.write_frame(0x1, text.as_bytes())
    }

    fn recv_text(&mut self) -> io::Result<Option<String>> {
        let mut message: Vec<u8> = Vec::new();
        let mut in_message = false;
        loop {
            if self.closed {
                return Ok(None);
            }
            let frame = read_frame(&mut self.stream)?;
            match frame.opcode {
                0x9 => self.write_frame(0xA, &frame.payload)?,
                0xA => {}
                0x8 => {
                    let _ = self.write_frame(0x8, &frame.payload.get(..2).map(<[u8]>::to_vec).unwrap_or_default());
                    self.closed = true;
                    return Ok(None);
                }
                opcode => {
                    if (opcode == 0x1) == in_message {
                        return Err(broken("continuation out of order"));
                    }
                    in_message = true;
                    if message.len() + frame.payload.len() > MAX_MESSAGE {
                        return Err(broken("message too large"));
                    }
                    message.extend_from_slice(&frame.payload);
                    if frame.fin {
                        return String::from_utf8(message).map(Some).map_err(|_| broken("text is not UTF-8"));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::net::TcpListener;

    /// A frame as a server sends it (unmasked).
    fn server_frame(fin: bool, opcode: u8, payload: &[u8]) -> Vec<u8> {
        let mut out = vec![if fin { 0x80 } else { 0 } | opcode];
        let len = payload.len();
        if len < 126 {
            out.push(len as u8);
        } else if len <= 0xFFFF {
            out.push(126);
            out.extend_from_slice(&(len as u16).to_be_bytes());
        } else {
            out.push(127);
            out.extend_from_slice(&(len as u64).to_be_bytes());
        }
        out.extend_from_slice(payload);
        out
    }

    /// Unmasks a client frame (as the server would).
    fn unmask(frame: &[u8]) -> (u8, Vec<u8>) {
        assert!(frame[1] & 0x80 != 0, "a client frame is always masked");
        let (len, start) = match frame[1] & 0x7F {
            126 => (u16::from_be_bytes([frame[2], frame[3]]) as usize, 4),
            127 => (u64::from_be_bytes(frame[2..10].try_into().unwrap()) as usize, 10),
            n => (n as usize, 2),
        };
        let mask = &frame[start..start + 4];
        let data = frame[start + 4..start + 4 + len].iter().enumerate().map(|(i, b)| b ^ mask[i % 4]).collect();
        (frame[0] & 0x0F, data)
    }

    #[test]
    fn the_accept_value_is_the_one_of_the_rfc() {
        assert_eq!(accept_value("dGhlIHNhbXBsZSBub25jZQ=="), "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }

    #[test]
    fn a_client_frame_is_masked_and_carries_every_length() {
        for len in [0, 5, 125, 126, 300, 70_000] {
            let payload = vec![b'x'; len];
            let frame = client_frame(0x1, &payload, [1, 2, 3, 4]);
            assert_eq!(unmask(&frame), (0x1, payload));
        }
    }

    #[test]
    fn frames_that_break_the_rules_end_the_connection() {
        let cases: Vec<(Vec<u8>, &str)> = vec![
            (vec![0x81, 0x85, 1, 2, 3, 4, 0, 0, 0, 0, 0], "masked"),
            (server_frame(true, 0x2, b"bin"), "binary"),
            (vec![0xC1, 0x00], "reserved"),
            (server_frame(true, 0x3, b""), "opcode"),
            (server_frame(false, 0x9, b"ping"), "control"),
            (server_frame(true, 0x9, &[0u8; 126]), "control"),
            (
                {
                    let mut f = vec![0x81, 127];
                    f.extend_from_slice(&((MAX_FRAME as u64) + 1).to_be_bytes());
                    f
                },
                "too large",
            ),
        ];
        for (bytes, why) in cases {
            let error = read_frame(&mut Cursor::new(bytes)).expect_err(why);
            assert_eq!(error.kind(), io::ErrorKind::InvalidData, "{why}");
            assert!(error.to_string().contains(why), "{why}: {error}");
        }
        let fine = read_frame(&mut Cursor::new(server_frame(true, 0x1, b"hallo"))).expect("text");
        assert_eq!(fine, Frame { fin: true, opcode: 1, payload: b"hallo".to_vec() });
    }

    #[test]
    fn a_wrong_handshake_answer_is_refused() {
        let key = "dGhlIHNhbXBsZSBub25jZQ==";
        let good = "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n\r\n";
        assert!(check_handshake(good, key).is_ok());
        assert!(check_handshake(&good.replace("101", "200"), key).is_err());
        assert!(check_handshake(&good.replace("s3pP", "XXXX"), key).is_err());
        assert!(check_handshake(&good.replace("Upgrade: websocket", "Upgrade: h2c"), key).is_err());
    }

    /// A tiny server: does the handshake, then sends `frames` and records what the client sends.
    fn serve(frames: Vec<u8>) -> (u16, std::thread::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let thread = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).expect("head");
                head.push(byte[0]);
            }
            let text = String::from_utf8_lossy(&head).into_owned();
            let key = text
                .lines()
                .find_map(|l| l.strip_prefix("Sec-WebSocket-Key: "))
                .expect("key")
                .trim()
                .to_string();
            let answer = format!(
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
                accept_value(&key)
            );
            stream.write_all(answer.as_bytes()).expect("answer");
            stream.write_all(&frames).expect("frames");
            stream.set_read_timeout(Some(Duration::from_millis(500))).ok();
            let mut got = Vec::new();
            let _ = stream.read_to_end(&mut got);
            got
        });
        (port, thread)
    }

    #[test]
    fn a_message_in_pieces_arrives_whole_a_ping_is_answered_and_a_close_ends_it() {
        let mut frames = server_frame(false, 0x1, b"{\"event\":");
        frames.extend(server_frame(true, 0x9, b"p"));
        frames.extend(server_frame(true, 0x0, b"\"keyDown\"}"));
        frames.extend(server_frame(true, 0x8, &[0x03, 0xE8]));
        let (port, server) = serve(frames);
        let mut client = WsClient::connect(port).expect("connect");
        assert_eq!(client.recv_text().expect("text").as_deref(), Some("{\"event\":\"keyDown\"}"));
        client.send_text("hallo").expect("send");
        assert_eq!(client.recv_text().expect("close"), None);
        drop(client);
        let sent = server.join().expect("server");
        let pong = unmask(&sent);
        assert_eq!(pong, (0xA, b"p".to_vec()), "the ping is answered first");
        let rest = &sent[2 + 4 + 1..];
        assert_eq!(unmask(rest), (0x1, b"hallo".to_vec()));
    }

    #[test]
    fn a_message_over_the_limit_in_many_pieces_ends_the_connection() {
        let piece = vec![b'a'; 64 * 1024];
        let mut frames = server_frame(false, 0x1, &piece);
        for _ in 0..(MAX_MESSAGE / piece.len()) {
            frames.extend(server_frame(false, 0x0, &piece));
        }
        let (port, _server) = serve(frames);
        let mut client = WsClient::connect(port).expect("connect");
        let error = client.recv_text().expect_err("too large");
        assert!(error.to_string().contains("message too large"), "{error}");
    }

    #[test]
    fn text_that_is_not_utf8_ends_the_connection() {
        let (port, _server) = serve(server_frame(true, 0x1, &[0xFF, 0xFE]));
        let mut client = WsClient::connect(port).expect("connect");
        assert!(client.recv_text().is_err());
    }
}
