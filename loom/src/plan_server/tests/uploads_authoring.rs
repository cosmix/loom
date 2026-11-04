//! `POST /api/attachments` is a `HolderOrTake` route: a fresh New work page attaches before its
//! first message, when nobody holds authoring yet, so the first upload takes authoring. Another
//! browser, and a server in read-only mode, are still refused.

use super::support::{
    bootstrap, json_body, options, origin, post_json, project, request, send, serve, serve_with,
    skip, status,
};
use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::wire::server::{MissingPrereq, PrereqKind};
use crate::plan_server::RunningServer;

const PATH: &str = "/api/attachments?name=shot.png";

/// The 8-byte PNG signature followed by 8 zero bytes.
const PNG: [u8; 16] = [
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0, 0, 0, 0, 0,
];

fn upload(server: &RunningServer, cookies: &str) -> String {
    let origin = origin(server);
    let length = format!("Content-Length: {}", PNG.len());
    let extra = [origin.as_str(), "Content-Type: image/png", length.as_str()];
    let mut raw = request("POST", server, PATH, cookies, &extra, "").into_bytes();
    raw.extend_from_slice(&PNG);
    send(server.local, &raw)
}

#[test]
fn an_upload_before_any_start_takes_authoring() {
    if skip("an_upload_before_any_start_takes_authoring") {
        return;
    }
    let project = project();
    let server = serve(&project);
    let first = bootstrap(&server);
    let second = bootstrap(&server);
    assert!(
        server.ctx.authoring().holder().is_none(),
        "nobody holds authoring yet"
    );

    let stored = upload(&server, &first);
    assert_eq!(status(&stored), 200, "{stored}");
    assert_eq!(json_body(&stored)["name"], "shot.png");
    let held = post_json(&server, "/api/test/hold", &first, "");
    assert_eq!(
        status(&held),
        200,
        "the uploader now holds authoring: {held}"
    );

    let refused = upload(&server, &second);
    assert_eq!(status(&refused), 403, "{refused}");
    assert_eq!(json_body(&refused)["error"], "not_holder");
    let again = upload(&server, &first);
    assert_eq!(status(&again), 200, "the holder uploads again: {again}");
    server.stop().expect("stop");
}

#[test]
fn a_read_only_server_refuses_the_first_upload() {
    if skip("a_read_only_server_refuses_the_first_upload") {
        return;
    }
    let project = project();
    let mut opts = options(&project);
    let problem = MissingPrereq {
        what: PrereqKind::Tmux,
        message: "tmux is missing".into(),
        fix: "install tmux".into(),
    };
    opts.read_only = ReadOnly::from_problems(vec![problem]);
    let server = serve_with(opts);
    let cookies = bootstrap(&server);

    let refused = upload(&server, &cookies);

    assert_eq!(status(&refused), 403, "{refused}");
    assert_eq!(json_body(&refused)["error"], "read_only");
    assert!(server.ctx.authoring().holder().is_none());
    server.stop().expect("stop");
}
