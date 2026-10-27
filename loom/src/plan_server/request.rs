//! What an API handler receives and returns, and how a response reaches the socket.

use std::io::Read;
use std::net::TcpStream;

use anyhow::{ensure, Context, Result};
use serde::de::DeserializeOwned;
use serde::Serialize;

use super::context::PlanContext;
use super::ids::{BrowserId, PublicId};
use crate::web_server::connection::drain_pending;
use crate::web_server::http::{self, RequestHead};

/// Longest decoded query key or value (section 12, "Request limits").
pub const MAX_PARAM_BYTES: usize = 1024;

/// Header every planning response carries besides the shared security headers.
pub(crate) const REFERRER_POLICY: (&str, &str) = ("Referrer-Policy", "no-referrer");

const JSON: &str = "application/json";

/// One admitted API request.
pub struct ApiRequest<'a> {
    pub ctx: &'a PlanContext,
    pub head: &'a RequestHead,
    /// The browser cookie's id. Never put it in a response: it is the browser's secret.
    pub browser: &'a BrowserId,
    /// The browser's public id (`presence.public_id(browser)`), or a throwaway for an id the
    /// gate just minted; the gate keeps such a request to viewer routes.
    pub public_id: &'a PublicId,
    pub query: QueryParams,
    pub body: Vec<u8>,
}

impl ApiRequest<'_> {
    /// The body parsed as JSON, or a 400 naming why it does not parse.
    pub fn json<T: DeserializeOwned>(&self) -> Result<T, ApiResponse> {
        serde_json::from_slice(&self.body).map_err(|error| {
            ApiResponse::error(400, "bad_request", &format!("invalid JSON body: {error}"))
        })
    }

    /// The body as UTF-8 text, or a 400.
    pub fn text(&self) -> Result<&str, ApiResponse> {
        std::str::from_utf8(&self.body)
            .map_err(|_| ApiResponse::error(400, "bad_request", "the body is not UTF-8 text"))
    }
}

/// A complete API answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiResponse {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
}

impl ApiResponse {
    /// `value` as a JSON body; a value that cannot serialize is a 500.
    pub fn json<T: Serialize>(status: u16, value: &T) -> ApiResponse {
        match serde_json::to_vec(value) {
            Ok(body) => ApiResponse {
                status,
                content_type: JSON.to_owned(),
                body,
            },
            Err(error) => {
                tracing::warn!("planning API response did not serialize: {error}");
                ApiResponse::error(500, "internal", "the response could not be encoded")
            }
        }
    }

    /// `{"error": code, "message": message}`.
    pub fn error(status: u16, code: &str, message: &str) -> ApiResponse {
        let body = serde_json::json!({ "error": code, "message": message });
        ApiResponse {
            status,
            content_type: JSON.to_owned(),
            body: body.to_string().into_bytes(),
        }
    }

    pub fn text(status: u16, mime: &str, body: impl Into<Vec<u8>>) -> ApiResponse {
        ApiResponse {
            status,
            content_type: mime.to_owned(),
            body: body.into(),
        }
    }

    /// Write this response through `write_response`.
    pub(crate) fn write(
        &self,
        stream: &mut TcpStream,
        head: &RequestHead,
        set_cookie: Option<&str>,
    ) {
        write_response(
            stream,
            head,
            self.status,
            &self.content_type,
            &self.body,
            set_cookie,
        );
    }
}

/// Write a response with `Referrer-Policy` and an optional `Set-Cookie`, omitting the body
/// for `HEAD`. An error status first drains whatever request bytes are still unread, so the
/// close is a FIN and the answer reaches the client.
pub(crate) fn write_response(
    stream: &mut TcpStream,
    head: &RequestHead,
    status: u16,
    content_type: &str,
    body: &[u8],
    set_cookie: Option<&str>,
) {
    if status >= 400 {
        drain_pending(stream);
    }
    let mut headers = vec![REFERRER_POLICY];
    if let Some(cookie) = set_cookie {
        headers.push(("Set-Cookie", cookie));
    }
    let send_body = head.method != "HEAD";
    let reason = reason_phrase(status);
    let written = http::write_response_with_headers(
        stream,
        status,
        reason,
        content_type,
        body,
        send_body,
        &headers,
    );
    if let Err(error) = written {
        tracing::debug!("planning response write failed: {error}");
    }
}

/// The reason phrase for every status the planning server answers with.
pub(crate) fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        302 => "Found",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        411 => "Length Required",
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        429 => "Too Many Requests",
        503 => "Service Unavailable",
        _ if status >= 500 => "Internal Server Error",
        _ => "Unknown",
    }
}

/// Decoded query parameters, in request order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueryParams(Vec<(String, String)>);

impl QueryParams {
    /// Percent-decode (`%XX`, and `+` as a space) every `key=value` pair of `query`. A key or
    /// value over [`MAX_PARAM_BYTES`] decoded bytes, a malformed escape, or non-UTF-8 is a 400.
    pub fn parse(query: Option<&str>) -> Result<QueryParams, ApiResponse> {
        let mut params = Vec::new();
        for pair in query
            .unwrap_or("")
            .split('&')
            .filter(|pair| !pair.is_empty())
        {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            params.push((decode(key)?, decode(value)?));
        }
        Ok(QueryParams(params))
    }

    /// The first value of `key`.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }
}

fn decode(raw: &str) -> Result<String, ApiResponse> {
    let bad = |message: &str| ApiResponse::error(400, "bad_request", message);
    let mut bytes = Vec::with_capacity(raw.len());
    let mut input = raw.bytes();
    while let Some(byte) = input.next() {
        let decoded = match byte {
            b'+' => b' ',
            b'%' => {
                let pair = [input.next(), input.next()];
                let hex = match pair {
                    [Some(high), Some(low)] => [high, low],
                    _ => return Err(bad("a query escape is cut short")),
                };
                // `from_str_radix` would accept a leading '+', so check the digits first.
                if !hex.iter().all(u8::is_ascii_hexdigit) {
                    return Err(bad("a query escape is not hex"));
                }
                let text =
                    std::str::from_utf8(&hex).map_err(|_| bad("a query escape is not hex"))?;
                u8::from_str_radix(text, 16).map_err(|_| bad("a query escape is not hex"))?
            }
            other => other,
        };
        bytes.push(decoded);
        if bytes.len() > MAX_PARAM_BYTES {
            return Err(bad("a query parameter is over 1024 bytes"));
        }
    }
    String::from_utf8(bytes).map_err(|_| bad("a query parameter is not UTF-8"))
}

/// Read exactly `length` body bytes, counting the `prefix` that arrived with the head.
///
/// `web_server::http::read_body` caps every body at 8 KiB, below a text route's own limit,
/// so the read is bounded here by the length the gate already checked against the route.
/// Each read keeps the 5 s timeout `read_head` installed.
pub(crate) fn read_body(stream: &mut TcpStream, prefix: Vec<u8>, length: usize) -> Result<Vec<u8>> {
    let mut body = prefix;
    body.truncate(length);
    let missing = u64::try_from(length - body.len()).context("body length overflows u64")?;
    stream
        .by_ref()
        .take(missing)
        .read_to_end(&mut body)
        .context("failed to read the request body")?;
    ensure!(
        body.len() == length,
        "client closed the connection inside the request body"
    );
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::QueryParams;

    #[test]
    fn percent_escape_needs_two_hex_digits_not_a_sign() {
        for bad in ["v=%+1", "v=%-1", "v=%+f"] {
            let refused = QueryParams::parse(Some(bad)).expect_err(bad);
            assert_eq!(refused.status, 400, "{bad}");
        }
        assert!(QueryParams::parse(Some("v=%2B")).is_ok());
    }
}
