//! Small HTTP/1.1 request parsing and response writing helpers.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use anyhow::{bail, Context, Result};

mod body;
pub use body::read_body;

/// Largest request head accepted by the server.
pub const MAX_HEAD_BYTES: usize = 16 * 1024;

/// Largest request body accepted by the server.
///
/// The only body the dashboard reads is a `/api/config` update — one key name
/// and one value — so the cap sits far below anything a browser would stream.
/// It bounds the declared `Content-Length` *and* the bytes [`read_body`]
/// actually consumes, so a length that understates what the client goes on to
/// send cannot make the server buffer more than this.
pub const MAX_BODY_BYTES: usize = 8 * 1024;

/// Header carrying the double-submit CSRF token on a mutating request.
pub const CSRF_HEADER: &str = "X-Loom-Csrf";

/// The parsed subset of an HTTP request head needed for routing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestHead {
    pub method: String,
    pub path: String,
    /// The raw request query, without the leading `?`.
    pub query: Option<String>,
    pub upgrade_websocket: bool,
    pub origin: Option<String>,
    pub host: Option<String>,
    /// The raw `Cookie` header, used only by the opt-in terminal lane.
    pub cookie: Option<String>,
    /// `Content-Type`, needed only by the config write route.
    pub content_type: Option<String>,
    /// `Content-Length`, unparsed: a malformed one is a client error the write
    /// route reports, not a parse failure for every request that carries one.
    pub content_length: Option<String>,
    /// The [`CSRF_HEADER`] value presented by a mutating request.
    pub csrf_token: Option<String>,
}

/// Read one header's value as UTF-8, if the request carries it.
///
/// A second copy of the header is an error rather than a first-one-wins pick.
/// Every header read through here gates access or frames the body, and RFC 9112
/// section 3.2 forbids a duplicate `Host` outright; taking the first value
/// would let a request that pairs a loopback `Host` with an attacker's own pass
/// the rebinding gate on the strength of a header the far end may never have
/// intended to send, and a request carrying two `Content-Length` values would
/// get its body framed by whichever this server picked rather than by
/// agreement.
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
    Ok(parse_head_with_len(buf)?.map(|(head, _)| head))
}

/// [`parse_head`], also reporting how many bytes of `buf` the head occupied.
///
/// The count is what separates the head from a body that arrived in the same
/// read: httparse reports it exactly, including for the bare-LF line endings a
/// scan for `\r\n\r\n` would misjudge.
pub fn parse_head_with_len(buf: &[u8]) -> Result<Option<(RequestHead, usize)>> {
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut request = httparse::Request::new(&mut headers);
    let head_len = match request.parse(buf)? {
        httparse::Status::Complete(len) => len,
        httparse::Status::Partial => return Ok(None),
    };
    let method = request
        .method
        .context("request method is missing")?
        .to_owned();
    let raw_path = request.path.context("request path is missing")?;
    let (path, query) = raw_path
        .split_once('?')
        .map_or((raw_path, None), |(path, query)| (path, Some(query)));
    if !path.starts_with('/') {
        bail!("request path must start with '/'");
    }
    let upgrade_websocket = request.headers.iter().any(|header| {
        header.name.eq_ignore_ascii_case("upgrade")
            && std::str::from_utf8(header.value)
                .is_ok_and(|value| value.eq_ignore_ascii_case("websocket"))
    });
    Ok(Some((
        RequestHead {
            method,
            path: path.to_owned(),
            query: query.map(str::to_owned),
            upgrade_websocket,
            origin: header_value(&request, "Origin")?,
            host: header_value(&request, "Host")?,
            cookie: header_value(&request, "Cookie")?,
            content_type: header_value(&request, "Content-Type")?,
            content_length: header_value(&request, "Content-Length")?,
            csrf_token: header_value(&request, CSRF_HEADER)?,
        },
        head_len,
    )))
}

/// Consume an HTTP request head from `stream`, with whatever body bytes
/// arrived alongside it.
///
/// Reads land in chunks, so the last one routinely carries the head's final
/// bytes and the start of a body. Returning that remainder is what lets
/// [`read_body`] account for bytes already off the socket instead of waiting
/// for them a second time.
pub fn read_head(stream: &mut TcpStream) -> Result<(RequestHead, Vec<u8>)> {
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
        if let Some((head, head_len)) = parse_head_with_len(&buffer)? {
            let body = buffer.split_off(head_len);
            return Ok((head, body));
        }
        if buffer.len() >= MAX_HEAD_BYTES {
            bail!("request head exceeds {MAX_HEAD_BYTES} bytes");
        }
    }
}

/// A declared `Content-Length` as a byte count, or `None` when it is not one.
///
/// Digits only, per RFC 9110 section 8.6: `str::parse` alone would accept `+8`, and a length
/// this server reads differently from whatever wrote it is the shape request smuggling takes.
/// Surrounding whitespace is tolerated (the head parser already strips it); an empty value and
/// a value too large for `usize` are not counts.
pub fn parse_content_length(raw: &str) -> Option<usize> {
    let raw = raw.trim();
    if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    raw.parse().ok()
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

/// Whether a *mutating* request's `Origin` is present and names loopback.
///
/// [`origin_allowed`] waves an absent `Origin` through, and must: a browser
/// sends none on a same-origin `GET`, so rejecting absence would break the
/// dashboard's own reads. A state-changing request is the opposite case —
/// browsers attach `Origin` to every `POST`, same-origin included — so an
/// absent one means the request came from something that is not a page under
/// this origin, and there is nothing for the gate to check. Writes therefore
/// require the header rather than defaulting open. Kept as a separate
/// predicate so loosening or tightening one lane cannot silently move the
/// other.
pub fn origin_allowed_strict(origin: Option<&str>) -> bool {
    origin.is_some() && origin_allowed(origin)
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

/// Encode the status line, the fixed security headers and any
/// `extra_headers` for a body of `content_length` bytes.
fn response_head(
    status: u16,
    reason: &str,
    content_type: &str,
    content_length: usize,
    location: Option<&str>,
    set_cookie: Option<&str>,
    extra_headers: &[(&str, &str)],
) -> String {
    let extra: String = extra_headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect();
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {content_length}\r\nContent-Type: {content_type}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\nContent-Security-Policy: {CONTENT_SECURITY_POLICY}\r\n{}{}{extra}Connection: close\r\n\r\n",
        location.map(|value| format!("Location: {value}\r\n")).unwrap_or_default(),
        set_cookie.map(|value| format!("Set-Cookie: {value}\r\n")).unwrap_or_default(),
    )
}

/// Encode an HTTP response with the fixed security headers, through `response_head`.
///
/// Test-only: `commands/status/web/tests/pure.rs` is its sole caller; the servers write through
/// `write_response_with_headers`.
#[cfg(test)]
pub fn response_bytes(status: u16, reason: &str, content_type: &str, body: &[u8]) -> Vec<u8> {
    let mut bytes =
        response_head(status, reason, content_type, body.len(), None, None, &[]).into_bytes();
    bytes.extend_from_slice(body);
    bytes
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
    write_response_with_headers(stream, status, reason, content_type, body, send_body, &[])
}

/// [`write_response`], adding `extra_headers` after the fixed security
/// headers. The fixed headers and the Content-Security-Policy are never
/// replaced.
pub fn write_response_with_headers(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    content_type: &str,
    body: &[u8],
    send_body: bool,
    extra_headers: &[(&str, &str)],
) -> std::io::Result<()> {
    let mut bytes = response_head(
        status,
        reason,
        content_type,
        body.len(),
        None,
        None,
        extra_headers,
    )
    .into_bytes();
    if send_body {
        bytes.extend_from_slice(body);
    }
    stream.write_all(&bytes)?;
    stream.flush()
}

/// Drain a peeked request, then redirect while setting an optional cookie and
/// any `extra_headers`.
pub(crate) fn write_redirect(
    stream: &mut TcpStream,
    location: &str,
    set_cookie: Option<&str>,
    extra_headers: &[(&str, &str)],
) -> std::io::Result<()> {
    super::connection::drain_pending(stream);
    let bytes = response_head(
        302,
        "Found",
        "text/plain; charset=utf-8",
        0,
        Some(location),
        set_cookie,
        extra_headers,
    );
    stream.write_all(bytes.as_bytes())?;
    stream.flush()
}
