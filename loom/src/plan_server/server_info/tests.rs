use std::net::{IpAddr, Ipv4Addr};

use serde_json::json;

use crate::plan_server::gate::{access_cookie_name, browser_cookie_name};
use crate::plan_server::ids::BrowserId;
use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::tests::support::{
    bootstrap, cookie_header, cookie_pair, get, json_body, options, project, serve, serve_with,
    skip, status,
};
use crate::plan_server::wire::server::{MissingPrereq, PrereqKind};
use crate::planning::runtime::project_key;

#[test]
fn server_reports_project_prerequisites_and_caller() {
    if skip("server_reports_project_prerequisites_and_caller") {
        return;
    }
    let project = project();
    let mut opts = options(&project);
    opts.read_only = ReadOnly::from_problems(vec![MissingPrereq {
        what: PrereqKind::Tmux,
        message: "tmux missing".into(),
        fix: "Install tmux".into(),
    }]);
    let server = serve_with(opts);
    let cookies = bootstrap(&server);
    let pair = cookie_pair(&cookies, &browser_cookie_name(server.local.port())).unwrap();
    let browser = BrowserId::parse(pair.split_once('=').unwrap().1).unwrap();

    let response = get(&server, "/api/server", &cookies);

    assert_eq!(status(&response), 200, "{response}");
    assert_eq!(
        json_body(&response),
        json!({
            "project": project.path().file_name().unwrap().to_string_lossy(),
            "project_key": project_key(project.path()).unwrap(),
            "port": server.local.port(),
            "remote": false,
            "read_only": [{"what": "tmux", "message": "tmux missing", "fix": "Install tmux"}],
            "stopping": false,
            "you": {
                "public_id": server.ctx.presence().public_id(&browser),
                "name": server.ctx.presence().name_of(&browser),
            },
        })
    );
    server.stop().expect("stop");
}

#[test]
fn server_without_a_browser_cookie_records_no_presence() {
    if skip("server_without_a_browser_cookie_records_no_presence") {
        return;
    }
    let project = project();
    let server = serve(&project);
    let port = server.local.port();
    let cookies = bootstrap(&server);
    let access = cookie_pair(&cookies, &access_cookie_name(port)).expect("access cookie");
    for _ in 0..3 {
        let response = get(&server, "/api/server", access);
        assert_eq!(status(&response), 200, "{response}");
        let minted = cookie_header(&response);
        let pair = cookie_pair(&minted, &browser_cookie_name(port)).expect("a minted cookie");
        let browser = BrowserId::parse(pair.split_once('=').unwrap().1).unwrap();
        assert_eq!(json_body(&response)["you"]["name"], "", "{response}");
        assert_eq!(server.ctx.presence().name_of(&browser), "", "recorded");
    }
    // No Viewer number went to the minted ids: the first browser that sends its cookie is 1.
    let info = json_body(&get(&server, "/api/server", &cookies));
    assert_eq!(info["you"]["name"], "Viewer 1", "{info}");
    server.stop().expect("stop");
}

#[test]
fn share_on_loopback_returns_tokenized_url_and_tunnel() {
    if skip("share_on_loopback_returns_tokenized_url_and_tunnel") {
        return;
    }
    let project = project();
    let server = serve(&project);
    let cookies = bootstrap(&server);
    let port = server.local.port();

    let response = get(&server, "/api/share", &cookies);

    assert_eq!(status(&response), 200, "{response}");
    assert_eq!(
        json_body(&response),
        json!({
            "url": format!("http://127.0.0.1:{port}/?token={}", server.token),
            "bind": "loopback",
            "tunnel": crate::plan_server::startup::tunnel_hint(port, &crate::plan_server::hostname()),
        })
    );
    server.stop().expect("stop");
}

#[test]
fn share_on_wildcard_bind_returns_first_url_without_tunnel() {
    if skip("share_on_wildcard_bind_returns_first_url_without_tunnel") {
        return;
    }
    let project = project();
    let mut opts = options(&project);
    opts.host = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
    let server = serve_with(opts);
    let cookies = bootstrap(&server);

    let response = get(&server, "/api/share", &cookies);

    assert_eq!(status(&response), 200, "{response}");
    assert_eq!(
        json_body(&response),
        json!({
            "url": server.ctx.bind().urls.first().cloned().unwrap_or_default(),
            "bind": "remote",
            "tunnel": null,
        })
    );
    server.stop().expect("stop");
}
