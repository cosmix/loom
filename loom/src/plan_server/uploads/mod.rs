//! `POST /api/attachments`: a raw file body stored under its content hash.
//!
//! The gate has already refused a wrong `Content-Type` (415) and an oversized `Content-Length`
//! (413) before reading the body. The handler checks the bytes against the declared type (422),
//! cleans the display name, stores the file and answers `{id, name, media_type, bytes}`. A chat
//! command later names the file by that id and the server resolves it
//! (`plan_session::attachments::resolve_file`); the client never sends a path and never
//! receives one.

mod check;
mod store;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_store;

use std::sync::Arc;

use super::context::PlanContext;
use super::gate::media_type;
use super::request::{ApiRequest, ApiResponse};
use super::routes::{BodyKind, Method, Role, RouteSpec};
use crate::plan_session::attachments::stored::MEDIA_TYPES;
use crate::planning::runtime::create_runtime_dir;
use store::StoreError;

// The kickoff draft store calls `touch` on every draft write that names a file attachment.
// Remove the allow once that call is wired.
#[allow(unused_imports)]
pub(crate) use store::touch;

/// The route's path.
const PATH: &str = "/api/attachments";

/// Largest upload.
const MAX_UPLOAD_BYTES: usize = 10 * 1024 * 1024;

/// The upload route. The holder uploads, and so does any browser when nobody holds authoring,
/// since a fresh New work page attaches before its first message takes authoring.
pub fn routes() -> Vec<RouteSpec> {
    vec![RouteSpec {
        method: Method::Post,
        path: PATH,
        body: BodyKind::Bytes {
            max: MAX_UPLOAD_BYTES,
            types: &MEDIA_TYPES,
        },
        role: Role::HolderOrTake,
        handler: upload,
    }]
}

/// Remove the files older than 30 days, once per server start.
pub fn install(ctx: &Arc<PlanContext>) {
    let swept = store::sweep(&ctx.runtime().attachments, store::now());
    tracing::info!(
        "attachments: removed {} expired files ({} could not be removed)",
        swept.removed,
        swept.failed
    );
}

fn upload(req: &ApiRequest<'_>) -> ApiResponse {
    let media = media_type(req.head.content_type.as_deref());
    if let Err(reason) = check::content(&media, &req.body) {
        return ApiResponse::error(422, "unprocessable", reason);
    }
    let name = check::display_name(req.query.get("name"));
    // `run` creates the runtime directory at startup; a server started without it (tests) has none.
    let runtime = req.ctx.runtime();
    let stored = create_runtime_dir(runtime)
        .map_err(StoreError::Io)
        .and_then(|()| store::put(&runtime.dir, &name, &media, &req.body, store::now()));
    match stored {
        Ok(meta) => ApiResponse::json(200, &meta),
        Err(StoreError::Full) => ApiResponse::error(
            507,
            "store_full",
            "the attachment store is full (256 MiB); wait for older files to expire",
        ),
        Err(StoreError::Io(error)) => {
            tracing::warn!("cannot store an attachment: {error:#}");
            ApiResponse::error(500, "internal", "the file could not be stored")
        }
    }
}
