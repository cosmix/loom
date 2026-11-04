//! Contract for `POST /api/attachments`: a file is stored under its content hash with the
//! extension of its checked media type, a body whose bytes contradict its `Content-Type` is
//! refused, and a media type outside the allowed set is refused.

use sha2::{Digest, Sha256};

use super::support::{
    bootstrap, json_body, origin, post_json, project, request, send, serve, skip, status,
};
use crate::plan_server::RunningServer;

const PATH: &str = "/api/attachments?name=sketch%20one.png";

/// The 8-byte PNG signature followed by 24 zero bytes.
fn png_body() -> Vec<u8> {
    let mut body = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    body.extend_from_slice(&[0u8; 24]);
    body
}

fn upload_bytes(server: &RunningServer, cookies: &str, media_type: &str, body: &[u8]) -> String {
    let origin = origin(server);
    let content_type = format!("Content-Type: {media_type}");
    let length = format!("Content-Length: {}", body.len());
    let extra = [origin.as_str(), content_type.as_str(), length.as_str()];
    let mut raw = request("POST", server, PATH, cookies, &extra, "").into_bytes();
    raw.extend_from_slice(body);
    send(server.local, &raw)
}

#[test]
fn an_upload_is_stored_by_content_and_a_mislabelled_body_is_refused() {
    if skip("an_upload_is_stored_by_content_and_a_mislabelled_body_is_refused") {
        return;
    }
    let project = project();
    let server = serve(&project);
    let cookies = bootstrap(&server);
    let taken = post_json(&server, "/api/test/take", &cookies, "");
    assert_eq!(status(&taken), 200, "{taken}");
    let body = png_body();
    let stored = upload_bytes(&server, &cookies, "image/png", &body);
    assert_eq!(status(&stored), 200, "{stored}");
    let meta = json_body(&stored);
    let id = hex::encode(Sha256::digest(&body))[..16].to_owned();
    assert_eq!(meta["id"], id.as_str(), "{meta}");
    assert_eq!(meta["name"], "sketch one.png", "{meta}");
    assert_eq!(meta["media_type"], "image/png", "{meta}");
    assert_eq!(meta["bytes"], 32, "{meta}");
    let file = server
        .ctx
        .runtime()
        .dir
        .join("attachments")
        .join(format!("{id}.png"));
    let on_disk = std::fs::read(&file).unwrap_or_else(|err| panic!("{}: {err}", file.display()));
    assert_eq!(on_disk, body, "the stored file holds the uploaded bytes");
    let lie = upload_bytes(&server, &cookies, "image/png", b"hello");
    assert_eq!(status(&lie), 422, "{lie}");
    let unsupported = upload_bytes(&server, &cookies, "application/x-msdownload", b"hello");
    assert_eq!(status(&unsupported), 415, "{unsupported}");
    server.stop().expect("stop");
}
