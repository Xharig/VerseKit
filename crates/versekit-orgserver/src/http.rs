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

//! A small HTTP/1.1 server on plain TCP: one request per connection, a body
//! only with `Content-Length`, the answer closes the connection. TLS is the
//! job of a reverse proxy in front of it.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::time::Duration;

/// Largest head (request line and headers) read.
pub const HEAD_LIMIT: usize = 16 * 1024;
/// Largest body read.
pub const BODY_LIMIT: usize = 2 * 1024 * 1024;
/// How long a connection may wait between two pieces of its request.
pub const READ_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a connection may take for its whole request, however slowly it sends.
pub const REQUEST_DEADLINE: Duration = Duration::from_secs(20);
/// Connections answered at the same time; more are closed at once.
pub const MAX_CONNECTIONS: usize = 128;

/// One request.
#[derive(Clone, Debug, Default)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    /// A request from a method, a target with an optional query, headers and a body.
    pub fn new(method: &str, target: &str, headers: &[(&str, &str)], body: &[u8]) -> Request {
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        Request {
            method: method.to_string(),
            path: path.to_string(),
            query: query.to_string(),
            headers: headers.iter().map(|(n, v)| (n.to_string(), v.to_string())).collect(),
            body: body.to_vec(),
        }
    }

    /// The first header of that name, in any case.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    /// The pairs of the query.
    pub fn query_pairs(&self) -> Vec<(String, String)> {
        parse_pairs(&self.query)
    }

    /// The pairs of a form body (`application/x-www-form-urlencoded`).
    pub fn form(&self) -> Vec<(String, String)> {
        parse_pairs(&String::from_utf8_lossy(&self.body))
    }
}

/// The first value of `name` in `pairs`.
pub fn value<'a>(pairs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    pairs.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
}

/// One answer.
#[derive(Clone, Debug, Default)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    /// A JSON answer that no cache keeps.
    pub fn json(status: u16, value: &serde_json::Value) -> Response {
        Response {
            status,
            headers: vec![
                ("Content-Type".into(), "application/json".into()),
                ("Cache-Control".into(), "no-store".into()),
            ],
            body: value.to_string().into_bytes(),
        }
    }

    /// An error as `application/problem+json` with the code of the standard.
    pub fn problem(status: u16, code: &str, detail: &str) -> Response {
        let body = serde_json::json!({
            "type": "about:blank",
            "title": reason(status),
            "status": status,
            "code": code,
            "detail": detail,
        });
        let mut answer = Response::json(status, &body);
        answer.headers[0].1 = "application/problem+json".into();
        answer
    }

    /// An OAuth error (`{"error": …}`) of the token endpoints, status 400.
    pub fn oauth_error(error: &str) -> Response {
        Response::json(400, &serde_json::json!({ "error": error }))
    }

    /// An HTML page of the panel: no frames, no scripts, nothing from elsewhere.
    pub fn html(status: u16, page: String) -> Response {
        Response {
            status,
            headers: vec![
                ("Content-Type".into(), "text/html; charset=utf-8".into()),
                ("Cache-Control".into(), "no-store".into()),
                (
                    "Content-Security-Policy".into(),
                    "default-src 'none'; style-src 'self'; font-src 'self'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'"
                        .into(),
                ),
                ("X-Frame-Options".into(), "DENY".into()),
                ("Referrer-Policy".into(), "same-origin".into()),
            ],
            body: page.into_bytes(),
        }
    }

    /// The same answer with one header more.
    pub fn with_header(mut self, name: &str, value: &str) -> Response {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    /// The body as JSON, `Null` if it is none.
    pub fn json_body(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).unwrap_or(serde_json::Value::Null)
    }
}

/// The reason phrase of a status.
pub fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        411 => "Length Required",
        413 => "Content Too Large",
        422 => "Unprocessable Content",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Unknown",
    }
}

/// Why a request could not be read; the answer to send, if any.
#[derive(Debug)]
pub enum ReadFailure {
    /// The connection broke off or timed out: nothing is answered.
    Gone,
    /// The request is malformed or too large.
    Refuse(Response),
}

fn find_head_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|w| w == b"\r\n\r\n")
}

/// Reads one request from `stream`.
pub fn read_request(stream: &mut impl Read) -> Result<Request, ReadFailure> {
    let start = std::time::Instant::now();
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(end) = find_head_end(&buffer) {
            break end;
        }
        if buffer.len() > HEAD_LIMIT {
            return Err(ReadFailure::Refuse(Response::problem(400, "INVALID_REQUEST", "head too large")));
        }
        if start.elapsed() > REQUEST_DEADLINE {
            return Err(ReadFailure::Gone);
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return Err(ReadFailure::Gone),
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
        }
    };
    let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next().unwrap_or_default().split(' ');
    let (Some(method), Some(target)) = (first.next(), first.next()) else {
        return Err(ReadFailure::Refuse(Response::problem(400, "INVALID_REQUEST", "no request line")));
    };
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
        .collect();
    let mut request = Request::new(method, target, &[], &[]);
    request.headers = headers;
    if request.header("Transfer-Encoding").is_some() {
        return Err(ReadFailure::Refuse(Response::problem(411, "INVALID_REQUEST", "send Content-Length")));
    }
    let length = match request.header("Content-Length") {
        None => 0,
        Some(text) => match text.parse::<usize>() {
            Ok(n) if n <= BODY_LIMIT => n,
            Ok(_) => return Err(ReadFailure::Refuse(Response::problem(413, "INVALID_REQUEST", "body too large"))),
            Err(_) => return Err(ReadFailure::Refuse(Response::problem(400, "INVALID_REQUEST", "bad length"))),
        },
    };
    let mut body = buffer[head_end + 4..].to_vec();
    while body.len() < length {
        if start.elapsed() > REQUEST_DEADLINE {
            return Err(ReadFailure::Gone);
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return Err(ReadFailure::Gone),
            Ok(n) => body.extend_from_slice(&chunk[..n]),
        }
    }
    body.truncate(length);
    request.body = body;
    Ok(request)
}

/// Writes `response` and ends the connection.
pub fn write_response(stream: &mut impl Write, response: &Response) -> std::io::Result<()> {
    let mut head = format!("HTTP/1.1 {} {}\r\n", response.status, reason(response.status));
    for (name, value) in &response.headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str(&format!("Content-Length: {}\r\nConnection: close\r\n\r\n", response.body.len()));
    stream.write_all(head.as_bytes())?;
    stream.write_all(&response.body)?;
    stream.flush()
}

/// Answers one connection with `handler`.
pub fn answer(mut stream: TcpStream, handler: &(dyn Fn(&Request) -> Response + Send + Sync)) {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let _ = stream.set_write_timeout(Some(READ_TIMEOUT));
    let response = match read_request(&mut stream) {
        Ok(request) => handler(&request),
        Err(ReadFailure::Refuse(response)) => response,
        Err(ReadFailure::Gone) => return,
    };
    let _ = write_response(&mut stream, &response);
}

/// Accepts connections on `listener`, each in a thread of its own, at most [`MAX_CONNECTIONS`] at a time (a
/// connection beyond that is closed at once), for as long as the listener lives.
pub fn serve(listener: TcpListener, handler: Arc<dyn Fn(&Request) -> Response + Send + Sync>) {
    let active = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    for stream in listener.incoming().flatten() {
        if active.fetch_add(1, std::sync::atomic::Ordering::AcqRel) >= MAX_CONNECTIONS {
            active.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
            drop(stream);
            continue;
        }
        let (handler, active) = (handler.clone(), active.clone());
        std::thread::spawn(move || {
            answer(stream, handler.as_ref());
            active.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
        });
    }
}

/// Decodes `%XX` and `+` of a form or query value; invalid escapes stay as they are.
pub fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => match (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                (Some(high), Some(low)) => {
                    out.push(high * 16 + low);
                    i += 2;
                }
                _ => out.push(b'%'),
            },
            other => out.push(other),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(digit: u8) -> Option<u8> {
    (digit as char).to_digit(16).map(|d| d as u8)
}

/// The pairs of `a=1&b=2`, decoded.
pub fn parse_pairs(text: &str) -> Vec<(String, String)> {
    text.split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            (percent_decode(name), percent_decode(value))
        })
        .collect()
}

/// Escapes text for HTML.
pub fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_are_read_with_their_body_and_limits_hold() {
        let raw = b"POST /a/b?x=1 HTTP/1.1\r\nHost: h\r\nContent-Length: 5\r\n\r\nhello";
        let request = read_request(&mut &raw[..]).unwrap();
        assert_eq!((request.method.as_str(), request.path.as_str(), request.query.as_str()), ("POST", "/a/b", "x=1"));
        assert_eq!(request.body, b"hello");
        assert_eq!(request.header("host"), Some("h"));
        let huge = format!("POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n", BODY_LIMIT + 1);
        assert!(matches!(read_request(&mut huge.as_bytes()), Err(ReadFailure::Refuse(r)) if r.status == 413));
        let chunked = b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n";
        assert!(matches!(read_request(&mut &chunked[..]), Err(ReadFailure::Refuse(r)) if r.status == 411));
        assert!(matches!(read_request(&mut &b"GET / HTTP/1.1\r\n"[..]), Err(ReadFailure::Gone)));
    }

    #[test]
    fn forms_decode_and_html_escapes() {
        let pairs = parse_pairs("a=1+2&b=%C3%A4%2F&c=%zz&d");
        assert_eq!(value(&pairs, "a"), Some("1 2"));
        assert_eq!(value(&pairs, "b"), Some("ä/"));
        assert_eq!(value(&pairs, "c"), Some("%zz"));
        assert_eq!(value(&pairs, "d"), Some(""));
        assert_eq!(percent_decode("%4"), "%4");
        assert_eq!(escape_html("<a href=\"x\">'&'</a>"), "&lt;a href=&quot;x&quot;&gt;&#39;&amp;&#39;&lt;/a&gt;");
    }
}
