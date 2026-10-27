//! Section 12's checks for every `/api/*` request and every `/ws` upgrade, in order:
//! `Host`, exact `Origin` (every `POST` and WebSocket), the access cookie, the body's type and
//! length (before any body byte is read), then the route's role.

use std::net::SocketAddr;

use super::authoring::{Refusal, Take};
use super::context::PlanContext;
use super::ids::BrowserId;
use super::routes::{BodyKind, Method, Role, RouteSpec};
use crate::web_server::access::AccessPolicy;
use crate::web_server::http::{RequestHead, MAX_BODY_BYTES};
use crate::web_server::token;

/// The planning WebSocket's path.
pub const WS_PATH: &str = "/ws";

/// A request that passed the gate.
#[derive(Debug)]
pub struct Admitted {
    pub browser: BrowserId,
    /// A `Set-Cookie` value when the request carried no valid browser cookie and got a new one.
    pub set_cookie: Option<String>,
}

impl Admitted {
    /// True when the request presented no browser cookie and the gate minted this id. Such an
    /// id is one-shot until the browser sends it back, so nothing records it.
    pub fn minted(&self) -> bool {
        self.set_cookie.is_some()
    }
}

/// Why the gate turned a request away: the status and the `{"error", "message"}` body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Denied {
    pub status: u16,
    pub code: &'static str,
    pub message: String,
}

fn denied(status: u16, code: &'static str, message: impl Into<String>) -> Denied {
    Denied {
        status,
        code,
        message: message.into(),
    }
}

/// The access cookie's name: `loom_plan_<port>`.
pub fn access_cookie_name(port: u16) -> String {
    format!("loom_plan_{port}")
}

/// The browser cookie's name: `loom_plan_browser_<port>`.
pub fn browser_cookie_name(port: u16) -> String {
    format!("loom_plan_browser_{port}")
}

/// A session cookie carrying `id`, with the bootstrap cookie's attributes.
pub fn browser_cookie(port: u16, id: &BrowserId) -> String {
    format!(
        "{}={}; Path=/; HttpOnly; SameSite=Strict",
        browser_cookie_name(port),
        id.as_str()
    )
}

/// The browser id the request's cookie carries, if it is well formed.
pub fn browser_of(head: &RequestHead, port: u16) -> Option<BrowserId> {
    token::cookie_token(head.cookie.as_deref(), &browser_cookie_name(port))
        .and_then(BrowserId::parse)
}

/// Whether `Origin` is byte-equal to `http://<Host>`. `required` refuses an absent one.
///
/// The dashboard's loopback rule accepts any loopback alias; section 12 forbids that here, so
/// the comparison is exact in every posture.
pub fn origin_matches(head: &RequestHead, required: bool) -> bool {
    match (head.origin.as_deref(), head.host.as_deref()) {
        (None, _) => !required,
        (Some(origin), Some(host)) => origin.strip_prefix("http://") == Some(host),
        (Some(_), None) => false,
    }
}

/// Run section 12's checks on one request. `spec` is the matched API route (body and role
/// checks run only with one); `/ws` is recognised by its path.
pub fn admit(
    head: &RequestHead,
    local: SocketAddr,
    policy: &AccessPolicy,
    ctx: &PlanContext,
    spec: Option<&RouteSpec>,
) -> Result<Admitted, Denied> {
    if !policy.host_allowed(local, head) {
        return Err(denied(403, "forbidden", "host not allowed"));
    }
    let websocket = head.path == WS_PATH;
    if (websocket || head.method == "POST") && !origin_matches(head, true) {
        return Err(denied(403, "forbidden", "origin not allowed"));
    }
    if policy.auth().is_none() || !policy.authenticated(head.cookie.as_deref()) {
        return Err(denied(
            401,
            "unauthenticated",
            "open the link loom plan printed",
        ));
    }
    let admitted = identify(head, local.port(), websocket)?;
    if let Some(spec) = spec {
        check_body(head, spec)?;
        check_role(ctx, &admitted, spec.role)?;
    }
    Ok(admitted)
}

/// The request's browser id. An HTTP request without a valid one gets a new id and a cookie;
/// a WebSocket without one is refused (the app calls `GET /api/server` first).
fn identify(head: &RequestHead, port: u16, websocket: bool) -> Result<Admitted, Denied> {
    if let Some(browser) = browser_of(head, port) {
        return Ok(Admitted {
            browser,
            set_cookie: None,
        });
    }
    if websocket {
        return Err(missing_browser_cookie());
    }
    let browser = BrowserId::mint().map_err(|error| {
        denied(
            500,
            "internal",
            format!("cannot mint a browser id: {error}"),
        )
    })?;
    let set_cookie = Some(browser_cookie(port, &browser));
    Ok(Admitted {
        browser,
        set_cookie,
    })
}

/// `Content-Type` and `Content-Length` of a `POST`, refused before the body is read.
fn check_body(head: &RequestHead, spec: &RouteSpec) -> Result<(), Denied> {
    if spec.method != Method::Post {
        return Ok(());
    }
    let length = content_length(head)?;
    let (accepted, limit) = match spec.body {
        BodyKind::None => (true, 0),
        BodyKind::Json => (is_json(head.content_type.as_deref()), MAX_BODY_BYTES),
        BodyKind::Text { max } => (is_utf8_text(head.content_type.as_deref()), max),
    };
    if !accepted {
        return Err(denied(
            415,
            "unsupported_media_type",
            "wrong Content-Type for this route",
        ));
    }
    if length > limit {
        return Err(denied(
            413,
            "too_large",
            format!("the body is over {limit} bytes"),
        ));
    }
    Ok(())
}

/// The declared body length; an absent header means no body.
pub(crate) fn content_length(head: &RequestHead) -> Result<usize, Denied> {
    match head.content_length.as_deref() {
        None => Ok(0),
        Some(raw) => raw
            .trim()
            .parse()
            .map_err(|_| denied(400, "bad_request", "invalid Content-Length")),
    }
}

/// A media type with its parameters, lowercased and without spaces.
fn normalized(content_type: Option<&str>) -> String {
    content_type
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect::<String>()
        .to_ascii_lowercase()
}

fn is_json(content_type: Option<&str>) -> bool {
    matches!(
        normalized(content_type).as_str(),
        "application/json" | "application/json;charset=utf-8"
    )
}

fn is_utf8_text(content_type: Option<&str>) -> bool {
    normalized(content_type) == "text/plain;charset=utf-8"
}

fn missing_browser_cookie() -> Denied {
    denied(401, "unauthenticated", "the browser cookie is missing")
}

/// The route's role. A minted id may only reach viewer routes: it can never be the holder, and
/// taking authoring for an id no browser has stored would strand authoring on nobody (and record
/// a presence entry per cookie-less request), so it is refused like a cookie-less `/ws`.
fn check_role(ctx: &PlanContext, admitted: &Admitted, role: Role) -> Result<(), Denied> {
    let take = match role {
        Role::Viewer => return Ok(()),
        _ if admitted.minted() => return Err(missing_browser_cookie()),
        Role::Holder => Take::HolderOnly,
        Role::HolderOrTake => Take::TakeIfFree,
    };
    ctx.require_authoring(&admitted.browser, take)
        .map_err(|refusal| denied(403, refusal_code(&refusal), ctx.refusal_message(&refusal)))
}

fn refusal_code(refusal: &Refusal) -> &'static str {
    match refusal {
        Refusal::NotHolder { .. } => "not_holder",
        Refusal::NoHolder => "no_holder",
        Refusal::ReadOnlyMode => "read_only",
        Refusal::Stopping => "stopping",
    }
}
