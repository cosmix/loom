//! Fixtures for planning-server tests over real loopback sockets: a temp project, a started
//! server, raw HTTP requests, the cookie bootstrap and a WebSocket client.

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

use serde_json::{json, Value};
use tempfile::TempDir;
use tungstenite::client::IntoClientRequest;
use tungstenite::http::HeaderValue;
use tungstenite::{Message, WebSocket};

use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::request::{ApiRequest, ApiResponse};
use crate::plan_server::routes::{BodyKind, Method, Role, RouteSpec};
use crate::plan_server::{start_with_routes, RunningServer, ServerOptions};
use crate::process::sandbox_probe::{loopback_bindable, skip_unless};

/// Bound on every blocking read a test makes.
pub(crate) const WAIT: Duration = Duration::from_secs(10);

/// True (after logging why) when this environment cannot bind loopback TCP.
pub(crate) fn skip(test_name: &str) -> bool {
    skip_unless(
        loopback_bindable(),
        test_name,
        "loopback TCP is unavailable",
    )
}

/// A project root holding only `doc/product/index.md`.
pub(crate) fn project() -> TempDir {
    let dir = tempfile::tempdir().expect("create a temp project");
    std::fs::create_dir_all(dir.path().join("doc/product")).expect("create doc/product");
    std::fs::write(dir.path().join("doc/product/index.md"), "# Demo\n").expect("write index");
    dir
}

/// The options of a loopback server on an OS-chosen port with no missing prerequisites.
pub(crate) fn options(project: &TempDir) -> ServerOptions {
    ServerOptions {
        root: project.path().to_path_buf(),
        host: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: Some(0),
        read_only: ReadOnly::from_problems(Vec::new()),
    }
}

/// Start a server with the real routes plus [`test_routes`].
pub(crate) fn serve(project: &TempDir) -> RunningServer {
    serve_with(options(project))
}

pub(crate) fn serve_with(options: ServerOptions) -> RunningServer {
    let mut routes = test_routes();
    routes.extend(crate::plan_server::routes::all_routes());
    start_with_routes(options, routes).expect("start the planning server")
}

/// `POST /api/test/echo` (JSON, any viewer), `POST /api/test/take` (takes authoring when
/// free) and `POST /api/test/hold` (holder only).
pub(crate) fn test_routes() -> Vec<RouteSpec> {
    let route = |path, body, role, handler| RouteSpec {
        method: Method::Post,
        path,
        body,
        role,
        handler,
    };
    vec![
        route("/api/test/echo", BodyKind::Json, Role::Viewer, echo),
        route("/api/test/take", BodyKind::None, Role::HolderOrTake, ok),
        route("/api/test/hold", BodyKind::None, Role::Holder, ok),
    ]
}

fn echo(req: &ApiRequest<'_>) -> ApiResponse {
    match req.json::<Value>() {
        Ok(value) => ApiResponse::json(200, &value),
        Err(response) => response,
    }
}

fn ok(_: &ApiRequest<'_>) -> ApiResponse {
    ApiResponse::json(200, &json!({ "ok": true }))
}

/// Send `raw` and read the whole response (the server always closes after answering).
pub(crate) fn send(addr: SocketAddr, raw: &[u8]) -> String {
    let mut stream = TcpStream::connect(addr).expect("connect to the planning server");
    stream.set_read_timeout(Some(WAIT)).expect("read timeout");
    stream.write_all(raw).expect("send the request");
    let mut response = Vec::new();
    let _ = stream.read_to_end(&mut response);
    String::from_utf8_lossy(&response).into_owned()
}

/// `GET path` with `Host: 127.0.0.1:<port>` and an optional `Cookie` header.
pub(crate) fn get(server: &RunningServer, path: &str, cookie: &str) -> String {
    send(
        server.local,
        request("GET", server, path, cookie, &[], "").as_bytes(),
    )
}

/// A raw request with `Host: 127.0.0.1:<port>`, the cookie when not empty, `extra` header
/// lines, and `body` with its `Content-Length` when not empty.
pub(crate) fn request(
    method: &str,
    server: &RunningServer,
    path: &str,
    cookie: &str,
    extra: &[&str],
    body: &str,
) -> String {
    let mut raw = format!("{method} {path} HTTP/1.1\r\nHost: {}\r\n", host(server));
    if !cookie.is_empty() {
        raw.push_str(&format!("Cookie: {cookie}\r\n"));
    }
    for line in extra {
        raw.push_str(line);
        raw.push_str("\r\n");
    }
    if !body.is_empty() {
        raw.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    raw.push_str("\r\n");
    raw.push_str(body);
    raw
}

/// `127.0.0.1:<port>`.
pub(crate) fn host(server: &RunningServer) -> String {
    format!("127.0.0.1:{}", server.local.port())
}

/// The exact `Origin` the gate accepts.
pub(crate) fn origin(server: &RunningServer) -> String {
    format!("Origin: http://{}", host(server))
}

/// `POST path` with the exact Origin, the cookie and a JSON body.
pub(crate) fn post_json(server: &RunningServer, path: &str, cookie: &str, body: &str) -> String {
    let origin = origin(server);
    let extra = [origin.as_str(), "Content-Type: application/json"];
    send(
        server.local,
        request("POST", server, path, cookie, &extra, body).as_bytes(),
    )
}

pub(crate) fn status(response: &str) -> u16 {
    response
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0)
}

pub(crate) fn body(response: &str) -> &str {
    response.split_once("\r\n\r\n").map_or("", |(_, body)| body)
}

pub(crate) fn json_body(response: &str) -> Value {
    serde_json::from_str(body(response)).unwrap_or_else(|_| panic!("JSON body in {response}"))
}

/// Every `Set-Cookie` value, attributes included.
pub(crate) fn set_cookies(response: &str) -> Vec<String> {
    response
        .lines()
        .filter_map(|line| line.strip_prefix("Set-Cookie: "))
        .map(str::to_owned)
        .collect()
}

/// The `name=value` pairs of every `Set-Cookie`, joined for a `Cookie` header.
pub(crate) fn cookie_header(response: &str) -> String {
    set_cookies(response)
        .iter()
        .filter_map(|cookie| cookie.split(';').next())
        .collect::<Vec<_>>()
        .join("; ")
}

/// Bootstrap a new browser: both cookies, as a `Cookie` header value.
pub(crate) fn bootstrap(server: &RunningServer) -> String {
    let response = get(server, &format!("/?token={}", server.token), "");
    assert_eq!(status(&response), 302, "{response}");
    cookie_header(&response)
}

/// The `name=value` pair of `name` in a `Cookie` header value.
pub(crate) fn cookie_pair<'a>(cookies: &'a str, name: &str) -> Option<&'a str> {
    cookies
        .split("; ")
        .find(|pair| pair.split_once('=').is_some_and(|(key, _)| key == name))
}

pub(crate) type Socket = WebSocket<TcpStream>;

/// Open `/ws` with `cookie` and an optional `Origin`; the HTTP status on refusal.
pub(crate) fn connect_ws(
    server: &RunningServer,
    cookie: &str,
    origin: Option<&str>,
) -> Result<Socket, u16> {
    let stream = TcpStream::connect(server.local).expect("connect");
    stream.set_read_timeout(Some(WAIT)).expect("read timeout");
    let mut request = format!("ws://{}/ws", host(server))
        .into_client_request()
        .expect("client request");
    let headers = request.headers_mut();
    if let Some(origin) = origin {
        headers.insert(
            "Origin",
            HeaderValue::from_str(origin).expect("origin header"),
        );
    }
    if !cookie.is_empty() {
        headers.insert(
            "Cookie",
            HeaderValue::from_str(cookie).expect("cookie header"),
        );
    }
    match tungstenite::client::client(request, stream) {
        Ok((socket, _)) => Ok(socket),
        Err(tungstenite::HandshakeError::Failure(tungstenite::Error::Http(response))) => {
            Err(response.status().as_u16())
        }
        Err(other) => panic!("WebSocket handshake failed: {other}"),
    }
}

/// Read text frames until one whose `type` is `kind`; returns it raw.
pub(crate) fn read_until(socket: &mut Socket, kind: &str) -> String {
    loop {
        match socket.read().expect("read a frame") {
            Message::Text(text) => {
                let value: Value = serde_json::from_str(text.as_str()).expect("JSON frame");
                if value["type"] == kind {
                    return text.as_str().to_owned();
                }
            }
            Message::Close(_) => panic!("closed before a {kind} frame"),
            _ => {}
        }
    }
}
