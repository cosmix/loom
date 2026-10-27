//! The `/ws` gate, the frames a connection receives, and serving through a shutdown drain.

use std::sync::mpsc;

use tungstenite::Message;

use super::support::{
    bootstrap, connect_ws, cookie_pair, get, host, json_body, project, read_until, serve, skip,
};
use crate::plan_server::begin_shutdown;
use crate::plan_server::gate::{access_cookie_name, browser_cookie_name};
use crate::plan_server::shutdown::ShutdownHook;

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
