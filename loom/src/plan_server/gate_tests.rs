//! The gate's checks in order, the route table lookup, query decoding and typed extensions,
//! without sockets.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use tempfile::TempDir;

use super::context::{BindInfo, PlanContext};
use super::extensions::Extensions;
use super::gate::{access_cookie_name, admit, browser_cookie_name, origin_matches, Denied};
use super::ids::BrowserId;
use super::prereq::ReadOnly;
use super::request::{ApiRequest, ApiResponse, QueryParams};
use super::routes::{find, BodyKind, Lookup, Method, Role, RouteSpec};
use super::wire::server::{MissingPrereq, PrereqKind};
use crate::web_server::access::AccessPolicy;
use crate::web_server::auth::Auth;
use crate::web_server::http::RequestHead;

const PORT: u16 = 7272;
const HOST: &str = "127.0.0.1:7272";

fn token() -> String {
    "ab".repeat(32)
}

fn local() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), PORT)
}

fn policy() -> AccessPolicy {
    AccessPolicy::new(local(), Some(Auth::new(token(), access_cookie_name(PORT))))
}

fn context(problems: Vec<MissingPrereq>) -> (TempDir, Arc<PlanContext>) {
    let dir = tempfile::tempdir().expect("temp project");
    std::fs::create_dir_all(dir.path().join("doc/product")).expect("doc");
    std::fs::write(dir.path().join("doc/product/index.md"), "# Demo\n").expect("index");
    let bind = BindInfo::new(local(), &token());
    let read_only = ReadOnly::from_problems(problems);
    let ctx = PlanContext::new(dir.path().to_path_buf(), read_only, bind).expect("context");
    (dir, Arc::new(ctx))
}

fn ok(_: &ApiRequest<'_>) -> ApiResponse {
    ApiResponse::json(200, &true)
}

fn spec(method: Method, path: &'static str, body: BodyKind, role: Role) -> RouteSpec {
    RouteSpec {
        method,
        path,
        body,
        role,
        handler: ok,
    }
}

fn head(method: &str, path: &str) -> RequestHead {
    RequestHead {
        method: method.to_owned(),
        path: path.to_owned(),
        query: None,
        upgrade_websocket: path == "/ws",
        origin: None,
        host: Some(HOST.to_owned()),
        cookie: None,
        content_type: None,
        content_length: None,
        csrf_token: None,
    }
}

/// A head carrying the exact Origin and both cookies of `browser`.
fn admitted_head(method: &str, path: &str, browser: &BrowserId) -> RequestHead {
    let mut head = head(method, path);
    head.origin = Some(format!("http://{HOST}"));
    head.cookie = Some(format!(
        "{}={}; {}={}",
        access_cookie_name(PORT),
        token(),
        browser_cookie_name(PORT),
        browser.as_str()
    ));
    head
}

fn status(result: Result<impl Sized, Denied>) -> u16 {
    result.err().map_or(200, |denied| denied.status)
}

#[test]
fn origin_must_equal_http_host_byte_for_byte() {
    let mut head = head("POST", "/api/x");
    assert!(!origin_matches(&head, true));
    assert!(origin_matches(&head, false));
    for (origin, accepted) in [
        ("http://127.0.0.1:7272", true),
        ("http://localhost:7272", false),
        ("https://127.0.0.1:7272", false),
        ("http://127.0.0.1:7272/", false),
        ("http://127.0.0.1:7273", false),
    ] {
        head.origin = Some(origin.to_owned());
        assert_eq!(origin_matches(&head, true), accepted, "{origin}");
    }
}

#[test]
fn checks_run_in_order_host_origin_cookie() {
    let (_dir, ctx) = context(Vec::new());
    let browser = BrowserId::mint().expect("browser");
    let mut evil_host = admitted_head("GET", "/api/server", &browser);
    evil_host.host = Some("evil.example".to_owned());
    assert_eq!(
        status(admit(&evil_host, local(), &policy(), &ctx, None)),
        403
    );
    // A foreign Origin is refused before the missing cookie is noticed.
    let mut foreign = head("POST", "/api/x");
    foreign.origin = Some("http://evil.example".to_owned());
    assert_eq!(status(admit(&foreign, local(), &policy(), &ctx, None)), 403);
    let mut no_cookie = admitted_head("POST", "/api/x", &browser);
    no_cookie.cookie = None;
    let denied = admit(&no_cookie, local(), &policy(), &ctx, None).expect_err("401");
    assert_eq!((denied.status, denied.code), (401, "unauthenticated"));
    let mut wrong = admitted_head("GET", "/api/server", &browser);
    wrong.cookie = Some(format!("{}={}", access_cookie_name(PORT), "cd".repeat(32)));
    assert_eq!(status(admit(&wrong, local(), &policy(), &ctx, None)), 401);
    let admitted = admit(
        &admitted_head("GET", "/api/server", &browser),
        local(),
        &policy(),
        &ctx,
        None,
    )
    .expect("admitted");
    assert_eq!(admitted.browser, browser);
    assert!(admitted.set_cookie.is_none());
}

#[test]
fn a_missing_browser_cookie_is_minted_for_http_and_refused_for_websockets() {
    let (_dir, ctx) = context(Vec::new());
    let browser = BrowserId::mint().expect("browser");
    let mut http = admitted_head("GET", "/api/server", &browser);
    http.cookie = Some(format!("{}={}", access_cookie_name(PORT), token()));
    let admitted = admit(&http, local(), &policy(), &ctx, None).expect("admitted");
    let cookie = admitted.set_cookie.expect("a new browser cookie");
    assert!(cookie.starts_with(&format!(
        "{}={}",
        browser_cookie_name(PORT),
        admitted.browser.as_str()
    )));
    assert!(cookie.ends_with("; Path=/; HttpOnly; SameSite=Strict"));
    let mut ws = admitted_head("GET", "/ws", &browser);
    ws.cookie = http.cookie.clone();
    assert_eq!(status(admit(&ws, local(), &policy(), &ctx, None)), 401);
    let mut ws_no_origin = admitted_head("GET", "/ws", &browser);
    ws_no_origin.origin = None;
    assert_eq!(
        status(admit(&ws_no_origin, local(), &policy(), &ctx, None)),
        403
    );
    assert_eq!(
        status(admit(
            &admitted_head("GET", "/ws", &browser),
            local(),
            &policy(),
            &ctx,
            None
        )),
        200
    );
}

#[test]
fn bodies_are_checked_by_type_then_length() {
    let (_dir, ctx) = context(Vec::new());
    let browser = BrowserId::mint().expect("browser");
    let json = spec(Method::Post, "/api/x", BodyKind::Json, Role::Viewer);
    let text = spec(
        Method::Post,
        "/api/t",
        BodyKind::Text { max: 16 },
        Role::Viewer,
    );
    let none = spec(Method::Post, "/api/n", BodyKind::None, Role::Viewer);
    let check = |spec: &RouteSpec, content_type: Option<&str>, length: Option<&str>| {
        let mut head = admitted_head("POST", spec.path, &browser);
        head.content_type = content_type.map(str::to_owned);
        head.content_length = length.map(str::to_owned);
        status(admit(&head, local(), &policy(), &ctx, Some(spec)))
    };
    assert_eq!(check(&json, Some("text/plain"), Some("2")), 415);
    assert_eq!(check(&json, Some("application/json"), Some("8193")), 413);
    assert_eq!(
        check(&json, Some("application/json; charset=UTF-8"), Some("8192")),
        200
    );
    assert_eq!(check(&json, Some("application/json"), Some("ten")), 400);
    assert_eq!(
        check(&text, Some("text/plain; charset=utf-8"), Some("16")),
        200
    );
    assert_eq!(
        check(&text, Some("text/plain; charset=utf-8"), Some("17")),
        413
    );
    assert_eq!(check(&text, Some("application/json"), Some("2")), 415);
    assert_eq!(check(&none, None, None), 200);
    assert_eq!(check(&none, None, Some("1")), 413);
}

#[test]
fn roles_refuse_with_the_authoring_text() {
    let (_dir, ctx) = context(Vec::new());
    let holder = BrowserId::mint().expect("holder");
    let other = BrowserId::mint().expect("other");
    let hold = spec(Method::Post, "/api/hold", BodyKind::None, Role::Holder);
    let take = spec(
        Method::Post,
        "/api/take",
        BodyKind::None,
        Role::HolderOrTake,
    );
    let run = |spec: &RouteSpec, browser: &BrowserId| {
        admit(
            &admitted_head("POST", spec.path, browser),
            local(),
            &policy(),
            &ctx,
            Some(spec),
        )
    };
    let nobody = run(&hold, &holder).expect_err("nobody holds");
    assert_eq!(nobody.message, "Nobody holds authoring");
    assert!(run(&take, &holder).is_ok());
    assert!(run(&hold, &holder).is_ok());
    ctx.presence().set_name(&holder, "Dana");
    let refused = run(&take, &other).expect_err("held by another");
    assert_eq!((refused.status, refused.code), (403, "not_holder"));
    assert_eq!(refused.message, "Another browser holds authoring (Dana)");

    let problem = MissingPrereq {
        what: PrereqKind::Tmux,
        message: "tmux is not on PATH".to_owned(),
        fix: "Install tmux".to_owned(),
    };
    let (_dir, read_only) = context(vec![problem]);
    let denied = admit(
        &admitted_head("POST", "/api/take", &other),
        local(),
        &policy(),
        &read_only,
        Some(&take),
    )
    .expect_err("read-only");
    assert_eq!(denied.message, "loom plan is in read-only mode");
}

#[test]
fn route_lookup_handles_prefixes_methods_and_head() {
    let routes = vec![
        spec(Method::Get, "/api/files/", BodyKind::None, Role::Viewer),
        spec(Method::Post, "/api/save", BodyKind::Json, Role::Holder),
    ];
    assert!(matches!(
        find(&routes, "GET", "/api/files/doc/x.md"),
        Lookup::Found(_)
    ));
    assert!(matches!(
        find(&routes, "HEAD", "/api/files/a"),
        Lookup::Found(_)
    ));
    assert!(matches!(
        find(&routes, "GET", "/api/save"),
        Lookup::WrongMethod
    ));
    assert!(matches!(
        find(&routes, "DELETE", "/api/save"),
        Lookup::WrongMethod
    ));
    assert!(matches!(
        find(&routes, "POST", "/api/save/x"),
        Lookup::Missing
    ));
    assert!(matches!(
        find(&routes, "GET", "/api/files"),
        Lookup::Missing
    ));
}

#[test]
fn query_params_decode_and_cap_each_value() {
    let params = QueryParams::parse(Some("path=doc%2Fa+b.md&flag&x=%C3%A9")).expect("parses");
    assert_eq!(params.get("path"), Some("doc/a b.md"));
    assert_eq!(params.get("flag"), Some(""));
    assert_eq!(params.get("x"), Some("é"));
    assert_eq!(params.get("missing"), None);
    let at_limit = format!("v={}", "a".repeat(1024));
    assert!(QueryParams::parse(Some(&at_limit)).is_ok());
    for bad in [
        format!("v={}", "a".repeat(1025)),
        "v=%4".to_owned(),
        "v=%zz".to_owned(),
        "v=%ff".to_owned(),
    ] {
        let refused = QueryParams::parse(Some(&bad)).expect_err(&bad);
        assert_eq!(refused.status, 400, "{bad}");
    }
}

#[test]
fn extensions_hold_one_value_per_type() {
    let extensions = Extensions::default();
    assert!(extensions.get::<String>().is_none());
    extensions.insert(Arc::new(String::from("one")));
    extensions.insert(Arc::new(7_u32));
    extensions.insert(Arc::new(String::from("two")));
    assert_eq!(
        extensions.get::<String>().as_deref().map(String::as_str),
        Some("two")
    );
    assert_eq!(extensions.get::<u32>().as_deref(), Some(&7));
}
