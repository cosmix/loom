//! Small HTTP/1.1 request parsing and response writing helpers.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use anyhow::{bail, Context, Result};

/// Largest request head accepted by the server.
pub const MAX_HEAD_BYTES: usize = 16 * 1024;

/// The parsed subset of an HTTP request head needed for routing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestHead {
    pub method: String,
    pub path: String,
    pub upgrade_websocket: bool,
    pub origin: Option<String>,
}

/// Parse a complete request head, or return `None` while it remains partial.
pub fn parse_head(buf: &[u8]) -> Result<Option<RequestHead>> {
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut request = httparse::Request::new(&mut headers);
    if request.parse(buf)? == httparse::Status::Partial {
        return Ok(None);
    }
    let method = request
        .method
        .context("request method is missing")?
        .to_owned();
    let raw_path = request.path.context("request path is missing")?;
    let path = raw_path
        .split_once('?')
        .map_or(raw_path, |(path, _)| path)
        .to_owned();
    if !path.starts_with('/') {
        bail!("request path must start with '/'");
    }
    let upgrade_websocket = request.headers.iter().any(|header| {
        header.name.eq_ignore_ascii_case("upgrade")
            && std::str::from_utf8(header.value)
                .is_ok_and(|value| value.eq_ignore_ascii_case("websocket"))
    });
    let origin = request
        .headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("origin"))
        .map(|header| std::str::from_utf8(header.value).context("Origin header is not UTF-8"))
        .transpose()?
        .map(str::to_owned);
    Ok(Some(RequestHead {
        method,
        path,
        upgrade_websocket,
        origin,
    }))
}

/// Consume an HTTP request head from `stream`.
pub fn read_head(stream: &mut TcpStream) -> Result<RequestHead> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 1024];
    loop {
        let read = stream
            .read(&mut chunk)
            .context("failed to read request head")?;
        if read == 0 {
            bail!("client closed connection before completing request head");
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(head) = parse_head(&buffer)? {
            return Ok(head);
        }
        if buffer.len() >= MAX_HEAD_BYTES {
            bail!("request head exceeds {MAX_HEAD_BYTES} bytes");
        }
    }
}

/// Hosts whose origin the dashboard accepts: the loopback interface only.
const LOOPBACK_HOSTS: [&str; 3] = ["127.0.0.1", "localhost", "::1"];

/// Strip an optional `:port`, and IPv6 `[...]` brackets, from an authority.
fn origin_host(authority: &str) -> &str {
    match authority.strip_prefix('[') {
        Some(rest) => rest.split_once(']').map_or(rest, |(host, _)| host),
        None => authority
            .split_once(':')
            .map_or(authority, |(host, _)| host),
    }
}

/// Whether an absent or loopback HTTP(S) origin is permitted.
pub fn origin_allowed(origin: Option<&str>) -> bool {
    let Some(origin) = origin else {
        return true;
    };
    let Some((scheme, remainder)) = origin.split_once("://") else {
        return false;
    };
    if !matches!(scheme, "http" | "https") {
        return false;
    }
    let authority = remainder.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() || authority.contains('@') {
        return false;
    }
    let host = origin_host(authority);
    LOOPBACK_HOSTS
        .iter()
        .any(|allowed| host.eq_ignore_ascii_case(allowed))
}

/// Encode the status line and the dashboard's fixed security headers for a
/// body of `content_length` bytes.
fn response_head(status: u16, reason: &str, content_type: &str, content_length: usize) -> String {
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {content_length}\r\nContent-Type: {content_type}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\nContent-Security-Policy: default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self' ws://127.0.0.1:* ws://localhost:*\r\nConnection: close\r\n\r\n"
    )
}

/// Encode an HTTP response with the dashboard's fixed security headers.
pub(crate) fn response_bytes(
    status: u16,
    reason: &str,
    content_type: &str,
    body: &[u8],
) -> Vec<u8> {
    response_head(status, reason, content_type, body.len())
        .into_bytes()
        .into_iter()
        .chain(body.iter().copied())
        .collect()
}

/// Write a complete response and flush it to the client. `send_body` is false
/// for a HEAD request, whose response keeps the `Content-Length` a GET would
/// have reported but must carry no body (RFC 9110 section 9.3.2).
pub fn write_response(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    content_type: &str,
    body: &[u8],
    send_body: bool,
) -> std::io::Result<()> {
    let bytes = if send_body {
        response_bytes(status, reason, content_type, body)
    } else {
        response_head(status, reason, content_type, body.len()).into_bytes()
    };
    stream.write_all(&bytes)?;
    stream.flush()
}
