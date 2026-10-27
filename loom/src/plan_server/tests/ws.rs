//! The `/ws` gate, the frames a connection receives, and serving through a shutdown drain.

use std::sync::mpsc;
use std::time::Instant;

use serde_json::Value;
use tungstenite::Message;

use super::support::{
    bootstrap, connect_ws, cookie_pair, get, host, json_body, post_json, project, read_until,
    serve, skip, status, Socket, WAIT,
};
use crate::plan_server::gate::{access_cookie_name, browser_cookie_name};
use crate::plan_server::shutdown::ShutdownHook;
use crate::plan_server::{begin_shutdown, RunningServer};

#[test]
fn websocket_upgrades_need_exact_origin_and_both_cookies() {
    if skip("websocket_upgrades_need_exact_origin_and_both_cookies") {
        return;
    }
    let project = project();
    let server = serve(&project);
    let port = server.local.port();
    let cookies = bootstrap(&server);
    let exact = format!("http://{}", host(&server));
    let alias = format!("http://localhost:{port}");
    let access = cookie_pair(&cookies, &access_cookie_name(port)).expect("access cookie");
    assert_eq!(connect_ws(&server, &cookies, None).err(), Some(403));
    assert_eq!(
        connect_ws(&server, &cookies, Some(alias.as_str())).err(),
        Some(403)
    );
    assert_eq!(
        connect_ws(&server, "", Some(exact.as_str())).err(),
        Some(401)
    );
    assert_eq!(
        connect_ws(&server, access, Some(exact.as_str())).err(),
        Some(401)
    );
    let mut socket =
        connect_ws(&server, &cookies, Some(exact.as_str())).expect("an admitted socket");
    read_until(&mut socket, "hello");
    server.stop().expect("stop");
}

#[test]
fn the_browser_cookie_never_leaves_the_server() {
    if skip("the_browser_cookie_never_leaves_the_server") {
        return;
    }
    let project = project();
    let server = serve(&project);
    let port = server.local.port();
    let cookies = bootstrap(&server);
    let browser = cookie_pair(&cookies, &browser_cookie_name(port))
        .and_then(|pair| pair.split_once('='))
        .map(|(_, value)| value.to_owned())
        .expect("browser cookie");
    let info = get(&server, "/api/server", &cookies);
    let info_body = info.split_once("\r\n\r\n").map_or("", |(_, body)| body);
    assert!(!info_body.contains(&browser), "{info_body}");
    let mut socket = connect_ws(
        &server,
        &cookies,
        Some(format!("http://{}", host(&server)).as_str()),
    )
    .expect("an admitted socket");
    let hello = read_until(&mut socket, "hello");
    assert!(!hello.contains(&browser), "{hello}");
    let named = r#"{"type":"set_name","name":"Dana"}"#;
    socket.send(Message::text(named)).expect("send set_name");
    let presence = read_until(&mut socket, "presence");
    assert!(!presence.contains(&browser), "{presence}");
    server.stop().expect("stop");
}

#[test]
fn a_bad_frame_is_refused_and_the_connection_stays_open() {
    if skip("a_bad_frame_is_refused_and_the_connection_stays_open") {
        return;
    }
    let project = project();
    let server = serve(&project);
    let cookies = bootstrap(&server);
    let mut socket = connect_ws(
        &server,
        &cookies,
        Some(format!("http://{}", host(&server)).as_str()),
    )
    .expect("an admitted socket");
    read_until(&mut socket, "hello");
    socket
        .send(Message::text("{\"type\":\"nope\"}"))
        .expect("send a bad frame");
    let refused: serde_json::Value =
        serde_json::from_str(&read_until(&mut socket, "refused")).expect("JSON");
    assert_eq!(refused["command"], "frame");
    let reading = r#"{"type":"reading","path":"doc/product/index.md"}"#;
    socket.send(Message::text(reading)).expect("send reading");
    // The presence broadcast of this tab opening may come first; the socket's read timeout
    // bounds the wait.
    while !read_until(&mut socket, "presence").contains("doc/product/index.md") {}
    server.stop().expect("stop");
}

/// While a drain runs, routes and WebSockets stay up and report stopping; releasing the drain
/// stops the server.
#[test]
fn the_server_keeps_serving_while_shutdown_drains() {
    if skip("the_server_keeps_serving_while_shutdown_drains") {
        return;
    }
    let project = project();
    let server = serve(&project);
    let (release, wait) = mpsc::channel::<()>();
    server.ctx.shutdown().register(ShutdownHook {
        name: "test-drain",
        drain: Box::new(move || {
            let _ = wait.recv();
        }),
        abort: Box::new(|| {}),
    });
    let cookies = bootstrap(&server);
    let origin = format!("http://{}", host(&server));
    let mut socket =
        connect_ws(&server, &cookies, Some(origin.as_str())).expect("an admitted socket");
    read_until(&mut socket, "hello");
    let drains = begin_shutdown(&server.ctx);
    read_until(&mut socket, "server_stopping");
    server.ctx.broadcast_presence();
    read_until(&mut socket, "presence");
    let info = get(&server, "/api/server", &cookies);
    assert_eq!(json_body(&info)["stopping"], true, "{info}");
    assert!(!drains.is_finished(), "the drain is still waiting");
    let addr = server.local;
    release.send(()).expect("release the drain");
    server
        .stop_when_drained(drains)
        .expect("stop after the drain");
    assert!(
        std::net::TcpStream::connect(addr).is_err(),
        "the listener is closed"
    );
}

/// One browser with an open, admitted socket: its cookies, the socket and its public id.
struct Browser {
    cookies: String,
    socket: Socket,
    public_id: String,
}

/// Bootstrap a new browser and open its socket; its `hello` is read, so the connection is
/// registered with the hub and receives every later broadcast.
fn join(server: &RunningServer) -> Browser {
    let cookies = bootstrap(server);
    let origin = format!("http://{}", host(server));
    let mut socket =
        connect_ws(server, &cookies, Some(origin.as_str())).expect("an admitted socket");
    let hello: Value = serde_json::from_str(&read_until(&mut socket, "hello")).expect("JSON");
    let public_id = hello["you"]["public_id"]
        .as_str()
        .expect("a public id in hello")
        .to_owned();
    Browser {
        cookies,
        socket,
        public_id,
    }
}

/// Read frames of type `kind` until `accept` holds for one; other types are skipped. Bounded by
/// `WAIT`, by the socket's own read timeout, and so by a panic when no such frame arrives.
fn frame_where(socket: &mut Socket, kind: &str, accept: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        let frame: Value = serde_json::from_str(&read_until(socket, kind)).expect("JSON frame");
        if accept(&frame) {
            return frame;
        }
    }
    panic!("no matching {kind} frame within {WAIT:?}");
}

/// The entry of `public_id` in a `presence` frame.
fn entry<'a>(presence: &'a Value, public_id: &str) -> Option<&'a Value> {
    presence["entries"]
        .as_array()?
        .iter()
        .find(|entry| entry["public_id"] == public_id)
}

/// Taking authoring reaches another browser's socket as an `authoring` frame naming the holder
/// and a `presence` frame marking it.
#[test]
fn taking_authoring_reaches_every_socket() {
    if skip("taking_authoring_reaches_every_socket") {
        return;
    }
    let project = project();
    let server = serve(&project);
    let holder = join(&server);
    let mut viewer = join(&server);
    let taken = post_json(&server, "/api/test/take", &holder.cookies, "");
    assert_eq!(status(&taken), 200, "{taken}");
    let authoring = frame_where(&mut viewer.socket, "authoring", |frame| {
        frame["holder"]["public_id"] == holder.public_id.as_str()
    });
    assert!(authoring["holder"]["away_since"].is_null(), "{authoring}");
    let presence = frame_where(&mut viewer.socket, "presence", |frame| {
        entry(frame, &holder.public_id).is_some_and(|entry| entry["holder"] == true)
    });
    let marked = entry(&presence, &holder.public_id).expect("the holder's entry");
    assert_eq!(marked["away"], false, "{presence}");
    server.stop().expect("stop");
}

/// The holder's last socket closing marks it away for everyone and keeps its authoring: a
/// second browser is still refused.
#[test]
fn the_holder_closing_its_socket_goes_away_and_keeps_authoring() {
    if skip("the_holder_closing_its_socket_goes_away_and_keeps_authoring") {
        return;
    }
    let project = project();
    let server = serve(&project);
    let mut holder = join(&server);
    let mut viewer = join(&server);
    let taken = post_json(&server, "/api/test/take", &holder.cookies, "");
    assert_eq!(status(&taken), 200, "{taken}");
    frame_where(&mut viewer.socket, "authoring", |frame| {
        frame["holder"]["public_id"] == holder.public_id.as_str()
    });
    holder
        .socket
        .close(None)
        .expect("close the holder's socket");
    drop(holder.socket);
    let away = frame_where(&mut viewer.socket, "authoring", |frame| {
        assert!(
            !frame["holder"].is_null(),
            "authoring freed when its holder's socket closed: {frame}"
        );
        !frame["holder"]["away_since"].is_null()
    });
    assert_eq!(away["holder"]["public_id"], holder.public_id, "{away}");
    assert!(!away["holder"]["frees_at"].is_null(), "{away}");
    let presence = frame_where(&mut viewer.socket, "presence", |frame| {
        entry(frame, &holder.public_id).is_some_and(|entry| entry["away"] == true)
    });
    let marked = entry(&presence, &holder.public_id).expect("the holder's entry");
    assert_eq!(marked["holder"], true, "{presence}");
    assert_eq!(marked["tabs"], 0, "{presence}");
    let refused = post_json(&server, "/api/test/take", &viewer.cookies, "");
    assert_eq!(status(&refused), 403, "{refused}");
    assert_eq!(json_body(&refused)["error"], "not_holder", "{refused}");
    server.stop().expect("stop");
}
