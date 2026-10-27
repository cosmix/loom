//! The access gate over real loopback sockets: bootstrap, cookies, Origin, body limits, roles.

use super::support::{
    body, bootstrap, cookie_header, cookie_pair, get, host, json_body, origin, post_json, project,
    request, send, serve, set_cookies, skip, status,
};
use crate::plan_server::gate::{access_cookie_name, browser_cookie_name};

#[test]
fn bootstrap_sets_two_session_cookies_and_redirects_home() {
    if skip("bootstrap_sets_two_session_cookies_and_redirects_home") {
        return;
    }
    let project = project();
    let server = serve(&project);
    let port = server.local.port();
    let response = get(&server, &format!("/?token={}", server.token), "");
    assert_eq!(status(&response), 302, "{response}");
    assert!(response.contains("\r\nLocation: /\r\n"), "{response}");
    assert!(
        response.contains("Referrer-Policy: no-referrer"),
        "{response}"
    );
    let cookies = set_cookies(&response);
    assert_eq!(cookies.len(), 2, "{cookies:?}");
    for (cookie, name) in cookies
        .iter()
        .zip([access_cookie_name(port), browser_cookie_name(port)])
    {
        let (pair, attributes) = cookie.split_once("; ").expect("cookie attributes");
        assert!(pair.starts_with(&format!("{name}=")), "{cookie}");
        assert_eq!(attributes, "Path=/; HttpOnly; SameSite=Strict", "{cookie}");
    }
    assert!(cookies[0].ends_with(&format!(
        "={}; Path=/; HttpOnly; SameSite=Strict",
        server.token
    )));
    server.stop().expect("stop");
}

#[test]
fn token_is_64_lowercase_hex_and_new_on_every_start() {
    if skip("token_is_64_lowercase_hex_and_new_on_every_start") {
        return;
    }
    let project = project();
    let first = serve(&project);
    let second = serve(&project);
    for token in [&first.token, &second.token] {
        assert_eq!(token.len(), 64);
        assert!(token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
    }
    assert_ne!(first.token, second.token);
    first.stop().expect("stop");
    second.stop().expect("stop");
}

#[test]
fn wrong_token_is_403_and_token_elsewhere_is_ignored() {
    if skip("wrong_token_is_403_and_token_elsewhere_is_ignored") {
        return;
    }
    let project = project();
    let server = serve(&project);
    let wrong = get(&server, &format!("/?token={}", "0".repeat(64)), "");
    assert_eq!(status(&wrong), 403, "{wrong}");
    assert!(set_cookies(&wrong).is_empty());
    let elsewhere = get(&server, &format!("/files/x.md?token={}", server.token), "");
    assert!(set_cookies(&elsewhere).is_empty(), "{elsewhere}");
    assert_ne!(status(&elsewhere), 302);
    server.stop().expect("stop");
}

#[test]
fn api_needs_the_access_cookie_and_sends_no_cors_headers() {
    if skip("api_needs_the_access_cookie_and_sends_no_cors_headers") {
        return;
    }
    let project = project();
    let server = serve(&project);
    let refused = get(&server, "/api/server", "");
    assert_eq!(status(&refused), 401, "{refused}");
    assert_eq!(json_body(&refused)["error"], "unauthenticated");
    let cookies = bootstrap(&server);
    let served = get(&server, "/api/server", &cookies);
    assert_eq!(status(&served), 200, "{served}");
    for response in [&refused, &served] {
        assert!(
            response.contains("Referrer-Policy: no-referrer"),
            "{response}"
        );
        assert!(!response.contains("Access-Control-Allow-"), "{response}");
    }
    let missing = get(&server, "/api/nothing-here", &cookies);
    assert_eq!(status(&missing), 404, "{missing}");
    assert_eq!(status(&get(&server, "/api/nothing-here", "")), 401);
    server.stop().expect("stop");
}

#[test]
fn a_request_without_a_browser_cookie_gets_a_new_one() {
    if skip("a_request_without_a_browser_cookie_gets_a_new_one") {
        return;
    }
    let project = project();
    let server = serve(&project);
    let port = server.local.port();
    let cookies = bootstrap(&server);
    let access = cookie_pair(&cookies, &access_cookie_name(port)).expect("access cookie");
    let response = get(&server, "/api/server", access);
    assert_eq!(status(&response), 200, "{response}");
    let minted = cookie_header(&response);
    let browser = browser_cookie_name(port);
    let fresh = cookie_pair(&minted, &browser).expect("a new browser cookie");
    assert_ne!(Some(fresh), cookie_pair(&cookies, &browser));
    let again = get(&server, "/api/server", &cookies);
    assert!(
        set_cookies(&again).is_empty(),
        "a valid browser cookie is kept: {again}"
    );
    server.stop().expect("stop");
}

#[test]
fn host_must_name_the_socket() {
    if skip("host_must_name_the_socket") {
        return;
    }
    let project = project();
    let server = serve(&project);
    let cookies = bootstrap(&server);
    let raw =
        format!("GET /api/server HTTP/1.1\r\nHost: evil.example\r\nCookie: {cookies}\r\n\r\n");
    assert_eq!(status(&send(server.local, raw.as_bytes())), 403);
    server.stop().expect("stop");
}

#[test]
fn posts_need_the_exact_origin() {
    if skip("posts_need_the_exact_origin") {
        return;
    }
    let project = project();
    let server = serve(&project);
    let cookies = bootstrap(&server);
    let post = |origin: Option<&str>| {
        let mut extra = vec!["Content-Type: application/json"];
        extra.extend(origin);
        send(
            server.local,
            request("POST", &server, "/api/test/echo", &cookies, &extra, "{}").as_bytes(),
        )
    };
    assert_eq!(status(&post(Some("Origin: http://evil.example"))), 403);
    let alias = format!("Origin: http://localhost:{}", server.local.port());
    assert_eq!(status(&post(Some(alias.as_str()))), 403);
    assert_eq!(status(&post(None)), 403);
    let exact = post(Some(origin(&server).as_str()));
    assert_eq!(status(&exact), 200, "{exact}");
    assert_eq!(body(&exact), "{}");
    server.stop().expect("stop");
}

#[test]
fn json_routes_refuse_other_types_and_oversized_bodies_before_reading() {
    if skip("json_routes_refuse_other_types_and_oversized_bodies_before_reading") {
        return;
    }
    let project = project();
    let server = serve(&project);
    let cookies = bootstrap(&server);
    let origin = origin(&server);
    let text = ["Content-Type: text/plain", origin.as_str()];
    let wrong_type = request("POST", &server, "/api/test/echo", &cookies, &text, "{}");
    assert_eq!(status(&send(server.local, wrong_type.as_bytes())), 415);
    // Only the head is sent: the 413 must arrive without the body.
    let length = format!("Content-Length: {}", 8 * 1024 + 1);
    let json = [
        "Content-Type: application/json",
        origin.as_str(),
        length.as_str(),
    ];
    let head_only = request("POST", &server, "/api/test/echo", &cookies, &json, "");
    assert_eq!(status(&send(server.local, head_only.as_bytes())), 413);
    let broken = post_json(&server, "/api/test/echo", &cookies, "{not json");
    assert_eq!(status(&broken), 400, "{broken}");
    server.stop().expect("stop");
}

#[test]
fn a_query_value_over_1024_bytes_is_400() {
    if skip("a_query_value_over_1024_bytes_is_400") {
        return;
    }
    let project = project();
    let server = serve(&project);
    let cookies = bootstrap(&server);
    let at_limit = get(
        &server,
        &format!("/api/server?x={}", "a".repeat(1024)),
        &cookies,
    );
    assert_eq!(status(&at_limit), 200, "{at_limit}");
    let over = get(
        &server,
        &format!("/api/server?x={}", "a".repeat(1025)),
        &cookies,
    );
    assert_eq!(status(&over), 400, "{over}");
    server.stop().expect("stop");
}

#[test]
fn holder_routes_refuse_a_second_browser() {
    if skip("holder_routes_refuse_a_second_browser") {
        return;
    }
    let project = project();
    let server = serve(&project);
    let first = bootstrap(&server);
    let second = bootstrap(&server);
    assert_eq!(
        status(&post_json(&server, "/api/test/hold", &first, "")),
        403
    );
    assert_eq!(
        status(&post_json(&server, "/api/test/take", &first, "")),
        200
    );
    assert_eq!(
        status(&post_json(&server, "/api/test/hold", &first, "")),
        200
    );
    for path in ["/api/test/take", "/api/test/hold"] {
        let refused = post_json(&server, path, &second, "");
        assert_eq!(status(&refused), 403, "{refused}");
        let reply = json_body(&refused);
        assert_eq!(reply["error"], "not_holder");
        assert!(reply["message"]
            .as_str()
            .unwrap_or("")
            .starts_with("Another browser holds authoring"));
    }
    server.stop().expect("stop");
}

#[test]
fn a_minted_browser_id_cannot_take_or_hold_authoring() {
    if skip("a_minted_browser_id_cannot_take_or_hold_authoring") {
        return;
    }
    let project = project();
    let server = serve(&project);
    let cookies = bootstrap(&server);
    let access = cookie_pair(&cookies, &access_cookie_name(server.local.port())).expect("access");
    for path in ["/api/test/take", "/api/test/hold"] {
        let refused = post_json(&server, path, access, "");
        assert_eq!(status(&refused), 401, "{refused}");
        assert_eq!(json_body(&refused)["error"], "unauthenticated");
        assert!(set_cookies(&refused).is_empty(), "{refused}");
    }
    // Authoring is still free: the cookie-less take recorded no holder.
    assert_eq!(
        status(&post_json(&server, "/api/test/take", &cookies, "")),
        200
    );
    server.stop().expect("stop");
}

#[test]
fn the_app_page_needs_no_cookie() {
    if skip("the_app_page_needs_no_cookie") {
        return;
    }
    let project = project();
    let server = serve(&project);
    for path in ["/", "/files/doc/x.md"] {
        let page = get(&server, path, "");
        assert!(matches!(status(&page), 200 | 503), "{page}");
        assert!(page.contains("Referrer-Policy: no-referrer"), "{page}");
    }
    let raw = format!("POST / HTTP/1.1\r\nHost: {}\r\n\r\n", host(&server));
    assert_eq!(status(&send(server.local, raw.as_bytes())), 405);
    server.stop().expect("stop");
}
