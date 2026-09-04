//! Lookup helpers for the dashboard bundle embedded by `build.rs`.

/// A binary asset addressed relative to `web/dist`.
pub type WebAsset = (&'static str, &'static [u8]);

include!(concat!(env!("OUT_DIR"), "/web_assets.rs"));

/// Return an embedded request path and its MIME type.
pub fn lookup(path: &str) -> Option<(&'static [u8], &'static str)> {
    let key = path.strip_prefix('/').unwrap_or(path);
    let key = if key.is_empty() { "index.html" } else { key };
    WEB_ASSETS
        .iter()
        .find(|(asset_key, _)| *asset_key == key)
        .map(|(key, bytes)| (*bytes, mime_for(key)))
}

/// Return the SPA entry page if the bundle is embedded.
pub fn index_html() -> Option<&'static [u8]> {
    lookup("/").map(|(bytes, _)| bytes)
}

/// MIME type appropriate for an embedded asset key.
pub(crate) fn mime_for(key: &str) -> &'static str {
    match key.rsplit_once('.').map(|(_, extension)| extension) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("json" | "map") => "application/json",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}
