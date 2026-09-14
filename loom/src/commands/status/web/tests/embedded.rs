//! Proves the embedded dashboard bundle is actually served: the real routing
//! and index-response functions, driven with the real `web/dist` bytes
//! embedded at build time - not just that the embed step produced something
//! non-empty.

use crate::commands::status::web::assets;
use crate::commands::status::web::connection::{self, Route};

#[test]
fn root_route_serves_the_embedded_index_html_byte_for_byte() {
    assert_eq!(connection::route("/"), Route::Spa);
    let index = assets::index_html().expect(
        "the dashboard bundle is not embedded; run `cd web && bun install && bun run build`, \
         then rebuild loom",
    );

    let (status, _reason, content_type, body) = connection::index_response(assets::index_html());

    assert_eq!(status, 200);
    assert_eq!(content_type, "text/html; charset=utf-8");
    assert_eq!(body, index);
}

#[test]
fn an_asset_referenced_by_the_embedded_index_is_served_with_its_mime_type() {
    let index = assets::index_html().expect("embedded index.html");
    let html = std::str::from_utf8(index).expect("embedded index.html is UTF-8");

    let js = referenced_asset_path(html, ".js").expect("index.html references a /assets/*.js path");
    assert_eq!(
        routed_mime(&js),
        "text/javascript; charset=utf-8",
        "expected {js} to route as JavaScript"
    );

    let css =
        referenced_asset_path(html, ".css").expect("index.html references a /assets/*.css path");
    assert_eq!(
        routed_mime(&css),
        "text/css; charset=utf-8",
        "expected {css} to route as CSS"
    );
}

fn routed_mime(path: &str) -> &'static str {
    match connection::route(path) {
        Route::Asset { mime, .. } => mime,
        other => panic!("expected {path} to route to an embedded asset, got {other:?}"),
    }
}

/// Pull the first `/assets/...` reference ending in `extension` out of the
/// index page's markup, stopping at the closing quote. Works whether or not
/// the build hashes asset filenames.
fn referenced_asset_path(html: &str, extension: &str) -> Option<String> {
    let mut search = html;
    loop {
        let start = search.find("/assets/")?;
        let rest = &search[start..];
        let end = rest.find(['"', '\''])?;
        let candidate = &rest[..end];
        if candidate.ends_with(extension) {
            return Some(candidate.to_string());
        }
        search = &rest[end..];
    }
}
