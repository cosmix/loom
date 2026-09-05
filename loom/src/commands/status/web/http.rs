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
    pub host: Option<String>,
}

/// Read one header's value as UTF-8, if the request carries it.
///
/// A second copy of the header is an error rather than a first-one-wins pick.
/// Both headers read here gate access, and RFC 9112 section 3.2 forbids a
/// duplicate `Host` outright; taking the first value would let a request that
/// pairs a loopback `Host` with an attacker's own pass the rebinding gate on
/// the strength of a header the far end may never have intended to send.
fn header_value(request: &httparse::Request<'_, '_>, name: &str) -> Result<Option<String>> {
    let mut matching = request
        .headers
        .iter()
        .filter(|header| header.name.eq_ignore_ascii_case(name));
    let Some(header) = matching.next() else {
        return Ok(None);
    };
    if matching.next().is_some() {
        bail!("request carries more than one {name} header");
    }
    std::str::from_utf8(header.value)
        .with_context(|| format!("{name} header is not UTF-8"))
        .map(|value| Some(value.to_owned()))
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
    Ok(Some(RequestHead {
        method,
        path,
        upgrade_websocket,
        origin: header_value(&request, "Origin")?,
        host: header_value(&request, "Host")?,
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

/// Whether `host` names the loopback interface.
fn is_loopback_host(host: &str) -> bool {
    LOOPBACK_HOSTS
        .iter()
        .any(|allowed| host.eq_ignore_ascii_case(allowed))
}

/// Whether the request's `Host` authority names the loopback interface.
///
/// This is the DNS-rebinding gate. A browser sends no `Origin` on a same-origin
/// GET, so [`origin_allowed`] alone lets a page served from an attacker-owned
/// name whose DNS record has been flipped to 127.0.0.1 read the ledger. That
/// request still carries the attacker's name in `Host`. HTTP/1.1 mandates a
/// `Host` header (RFC 9112 section 3.2), so an absent one is rejected rather
/// than waved through.
pub fn host_allowed(host: Option<&str>) -> bool {
    host.is_some_and(|host| is_loopback_host(origin_host(host)))
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
    is_loopback_host(origin_host(authority))
}

/// The dashboard's Content-Security-Policy.
///
/// `base-uri`, `form-action` and `frame-ancestors` have no `default-src`
/// fallback, so each is stated. `connect-src 'self'` covers the page's
/// WebSocket: CSP3 matches a same-origin `ws://` URL against `'self'`, and
/// `web/src/api/ws.ts` builds its URL from `location`. `style-src` keeps
/// `'unsafe-inline'` for React's inline styles.
const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

/// Encode the status line and the dashboard's fixed security headers for a
/// body of `content_length` bytes.
fn response_head(status: u16, reason: &str, content_type: &str, content_length: usize) -> String {
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {content_length}\r\nContent-Type: {content_type}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\nContent-Security-Policy: {CONTENT_SECURITY_POLICY}\r\nConnection: close\r\n\r\n"
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
