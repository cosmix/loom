//! `POST /api/save?path=&hash=[&with_errors=1]`: write one document (section 06, "What the
//! server does on save"). The checks run in order: authoring, path syntax, discovery, the
//! editable rules, symlinks, size, the opened hash, then plan validation. After the write the
//! text store re-reads the document, so a turn that begins next pins the saved text; nothing
//! else happens, and the watcher sees the write like any outside change.

use std::path::Path;

use super::doc::{editable, lock_view, MAX_EDIT_BYTES};
use super::texts::TextStore;
use super::validate::{is_plan_file, validate_plan};
use crate::fs::safe_replace::replace_no_follow;
use crate::plan_server::authoring::{Refusal, Take};
use crate::plan_server::context::PlanContext;
use crate::plan_server::ids::BrowserId;
use crate::plan_server::request::{ApiRequest, ApiResponse};
use crate::plan_server::wire::files::{LockView, SaveResult, ValidationView};
use crate::planning::discover::read_doc;
use crate::planning::kinds::{feature_dir_name, is_feature_name};
use crate::planning::paths::{check_relpath_syntax, symlink_component};
use crate::planning::sections::normalized_hash;

/// One save. `path` is relative to `doc/` (a caller holding a repo-relative path converts it
/// with `wire::paths::to_doc_rel`); `opened_hash` is the `normalized_hash` of the text the
/// editor opened; `force` is "Save with errors".
#[derive(Debug, Clone, Copy)]
pub struct SaveRequest<'a> {
    pub path: &'a str,
    pub text: &'a str,
    pub opened_hash: &'a str,
    pub force: bool,
}

/// What a save that passed every refusal check did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveOutcome {
    /// Written; `validation` is set for a plan file.
    Saved {
        hash: String,
        validation: Option<ValidationView>,
    },
    /// The file changed since the editor opened it; nothing was written.
    Conflict {
        current_text: String,
        current_hash: String,
    },
    /// The plan has validation errors and the save was not forced; nothing was written.
    Invalid { validation: ValidationView },
}

/// Why a save was refused before any write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveRefusal {
    Authoring(Refusal),
    Path(String),
    NotFound,
    NotEditable(LockView),
    TooLarge,
    Symlink,
    Io(String),
}

/// Save `req` for `browser`. A save never creates a file.
pub fn save_document(
    ctx: &PlanContext,
    browser: &BrowserId,
    req: SaveRequest<'_>,
) -> Result<SaveOutcome, SaveRefusal> {
    check_authoring(ctx, browser)?;
    let rel =
        check_relpath_syntax(req.path).map_err(|error| SaveRefusal::Path(error.to_string()))?;
    let root = ctx.root();
    check_editable(ctx, &rel)?;
    check_no_symlink(root, &rel)?;
    if req.text.len() > MAX_EDIT_BYTES {
        return Err(SaveRefusal::TooLarge);
    }
    // Pressure and amendment writers use this stable lock across replacement.
    let _plan_lock = crate::fs::locking::lock_plan(&root.join(&rel))
        .map_err(|error| SaveRefusal::Io(format!("{error:#}")))?;
    // Re-read now: the stored text may trail the watcher.
    let current = read_doc(root, &rel).map_err(|error| SaveRefusal::Io(format!("{error:#}")))?;
    if current.len() > MAX_EDIT_BYTES {
        return Err(SaveRefusal::TooLarge);
    }
    let current_hash = normalized_hash(&current);
    if current_hash != req.opened_hash {
        return Ok(SaveOutcome::Conflict {
            current_text: current,
            current_hash,
        });
    }
    let validation = is_plan_file(&rel).then(|| validate_plan(root, &rel, req.text));
    if let Some(validation) = validation.as_ref().filter(|v| !v.valid && !req.force) {
        let validation = validation.clone();
        return Ok(SaveOutcome::Invalid { validation });
    }
    ctx.require_authoring(browser, Take::TakeIfFree)
        .map_err(SaveRefusal::Authoring)?;
    replace_no_follow(root, &rel, req.text.as_bytes())
        .map_err(|error| SaveRefusal::Io(format!("{error:#}")))?;
    if let Some(texts) = ctx.extensions().get::<TextStore>() {
        texts.refresh_path(root, &rel);
    }
    Ok(SaveOutcome::Saved {
        hash: normalized_hash(req.text),
        validation,
    })
}

/// Refuse a browser while another one holds authoring, or in read-only mode. `save_document`
/// itself takes free authoring only right before the write, so a `Conflict` or `Invalid` outcome
/// leaves authoring as it was found. Over HTTP the route's `HolderOrTake` gate has already taken
/// it for the requesting browser before this function runs.
fn check_authoring(ctx: &PlanContext, browser: &BrowserId) -> Result<(), SaveRefusal> {
    match ctx.require_authoring(browser, Take::HolderOnly) {
        Ok(()) | Err(Refusal::NoHolder) => Ok(()),
        Err(refusal) => Err(SaveRefusal::Authoring(refusal)),
    }
}

/// Discovered (else `NotFound`), editable (else `NotEditable` with the lock notice) and not
/// reached through a symlink discovery accepted.
fn check_editable(ctx: &PlanContext, rel: &Path) -> Result<(), SaveRefusal> {
    if feature_dir_name(rel).is_some_and(|name| !is_feature_name(name)) {
        return Err(SaveRefusal::Path("the feature name is invalid".to_owned()));
    }
    let project = ctx.project();
    let doc = project.tree.get(rel).ok_or(SaveRefusal::NotFound)?;
    if !editable(&project, rel) {
        return Err(match lock_view(&project, rel) {
            Some(lock) => SaveRefusal::NotEditable(lock),
            None => SaveRefusal::Path("only Markdown documents can be edited".to_owned()),
        });
    }
    if doc.via_symlink {
        return Err(SaveRefusal::Symlink);
    }
    Ok(())
}

/// No component of `rel` is a symlink on disk now (the walk may predate a swap).
fn check_no_symlink(root: &Path, rel: &Path) -> Result<(), SaveRefusal> {
    match symlink_component(root, rel) {
        Ok(None) => Ok(()),
        Ok(Some(_)) => Err(SaveRefusal::Symlink),
        Err(error) => Err(SaveRefusal::Io(format!("{error:#}"))),
    }
}

/// The `POST /api/save` handler: the body is the new text (`text/plain; charset=utf-8`).
pub fn save_route(req: &ApiRequest<'_>) -> ApiResponse {
    let (Some(path), Some(opened_hash)) = (req.query.get("path"), req.query.get("hash")) else {
        return ApiResponse::error(
            400,
            "bad_request",
            "the path and hash parameters are required",
        );
    };
    let text = match req.text() {
        Ok(text) => text,
        Err(response) => return response,
    };
    let save = SaveRequest {
        path,
        text,
        opened_hash,
        force: req.query.get("with_errors") == Some("1"),
    };
    match save_document(req.ctx, req.browser, save) {
        Ok(outcome) => outcome_response(outcome),
        Err(refusal) => refusal_response(req.ctx, &refusal),
    }
}

fn outcome_response(outcome: SaveOutcome) -> ApiResponse {
    let (status, result) = match outcome {
        SaveOutcome::Saved { hash, validation } => (200, SaveResult::Saved { hash, validation }),
        SaveOutcome::Conflict {
            current_text,
            current_hash,
        } => (
            409,
            SaveResult::Conflict {
                current_text,
                current_hash,
            },
        ),
        SaveOutcome::Invalid { validation } => (422, SaveResult::Invalid { validation }),
    };
    ApiResponse::json(status, &result)
}

fn refusal_response(ctx: &PlanContext, refusal: &SaveRefusal) -> ApiResponse {
    match refusal {
        SaveRefusal::Authoring(refusal) => {
            ApiResponse::error(403, "authoring", &ctx.refusal_message(refusal))
        }
        SaveRefusal::Path(message) => ApiResponse::error(400, "bad_request", message),
        SaveRefusal::NotFound => ApiResponse::error(
            404,
            "not_found",
            "no such document; a save never creates a file",
        ),
        SaveRefusal::NotEditable(lock) => ApiResponse::error(403, "locked", &lock.notice),
        SaveRefusal::TooLarge => ApiResponse::error(
            413,
            "too_large",
            "documents over 1 MiB are not edited in file mode",
        ),
        SaveRefusal::Symlink => ApiResponse::error(
            403,
            "symlink",
            "a document reached through a symlink is not saved",
        ),
        SaveRefusal::Io(message) => {
            tracing::warn!("file mode save failed: {message}");
            ApiResponse::error(500, "internal", "the document could not be written")
        }
    }
}
