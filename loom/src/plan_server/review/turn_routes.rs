//! Handlers for restore and turn-sections, the routes that read turn pins.

use std::path::Path;

use super::restore::{restore_file, save_path_of, PinnedFile, RestoreResult, SaveResult};
use super::sections;
use crate::plan_server::files::doc::{respond, text_store};
use crate::plan_server::files::save::{save_document, SaveOutcome, SaveRefusal, SaveRequest};
use crate::plan_server::files::texts::{TextStore, TurnId};
use crate::plan_server::request::{ApiRequest, ApiResponse};
use crate::plan_server::wire::review::{
    RestoreFile, RestoreFileResult, RestoreRequest, RestoreResponse, TurnSectionsFile,
    TurnSectionsResponse,
};
use crate::plan_session::bridge::Bridge;

pub(super) fn restore(req: &ApiRequest<'_>) -> ApiResponse {
    respond(req.json().and_then(|body| restore_files(req, body)))
}

fn restore_files(
    req: &ApiRequest<'_>,
    body: RestoreRequest,
) -> Result<RestoreResponse, ApiResponse> {
    let key = turn_key(&body.session, &body.turn)?;
    let store = text_store(req.ctx)?;
    let bridge = req.ctx.extensions().get::<Bridge>();
    if !known_turn(&store, &key, &body, bridge.as_deref()) {
        return Err(unknown_turn());
    }
    let files = body
        .paths
        .iter()
        .map(|path| {
            // Sampled per file: a turn starting mid-request must not be written under.
            let running = bridge.as_ref().is_some_and(|bridge| bridge.turn_running());
            RestoreFile {
                path: path.clone(),
                result: restore_path(req, &store, &key, path, running, body.force),
            }
        })
        .collect();
    Ok(RestoreResponse { files })
}

fn restore_path(
    req: &ApiRequest<'_>,
    store: &TextStore,
    key: &TurnId,
    path: &str,
    running: bool,
    force: bool,
) -> RestoreFileResult {
    let Some(save_path) = save_path_of(path) else {
        return RestoreFileResult::Refused {
            reason: "outside doc/".to_owned(),
        };
    };
    let pin = PinnedFile {
        start_text: store
            .turn_pin(key, Path::new(path))
            .map(|pin| pin.text.to_string()),
        end_hash: store.turn_end(key, Path::new(path)).map(|pin| pin.hash),
    };
    // A running turn keeps its own refusal from `restore_file`.
    if !running && store.turn_ended(key) && pin.start_text.is_none() && pin.end_hash.is_none() {
        return RestoreFileResult::Refused {
            reason: "not changed by this turn".to_owned(),
        };
    }
    let mut save = |path: &str, text: &str, opened_hash: &str, force| {
        save_result(
            req,
            SaveRequest {
                path,
                text,
                opened_hash,
                force,
            },
        )
    };
    restore_result(restore_file(
        save_path,
        Some(&pin),
        running,
        force,
        &mut save,
    ))
}

fn save_result(req: &ApiRequest<'_>, save: SaveRequest<'_>) -> SaveResult {
    match save_document(req.ctx, req.browser, save) {
        Ok(SaveOutcome::Saved { .. }) => SaveResult::Saved,
        Ok(SaveOutcome::Conflict {
            current_text,
            current_hash,
        }) => SaveResult::HashMismatch {
            current_text,
            current_hash,
        },
        Ok(SaveOutcome::Invalid { validation }) => SaveResult::Invalid {
            errors: validation
                .errors
                .into_iter()
                .map(|error| error.message)
                .collect(),
        },
        Err(refusal) => SaveResult::Refused(save_refusal(req, refusal)),
    }
}

fn save_refusal(req: &ApiRequest<'_>, refusal: SaveRefusal) -> String {
    match refusal {
        SaveRefusal::Authoring(refusal) => req.ctx.refusal_message(&refusal),
        SaveRefusal::Path(message) => message,
        SaveRefusal::NotFound => "no such document; a save never creates a file".to_owned(),
        SaveRefusal::NotEditable(lock) => lock.notice,
        SaveRefusal::TooLarge => "documents over 1 MiB are not edited in file mode".to_owned(),
        SaveRefusal::Symlink => "a document reached through a symlink is not saved".to_owned(),
        SaveRefusal::Io(message) => {
            tracing::warn!("review restore save failed: {message}");
            "the document could not be written".to_owned()
        }
    }
}

fn restore_result(result: RestoreResult) -> RestoreFileResult {
    match result {
        RestoreResult::Restored => RestoreFileResult::Restored,
        RestoreResult::Refused(reason) => RestoreFileResult::Refused { reason },
        RestoreResult::Changed {
            current_text,
            current_hash,
            turn_start_text,
        } => RestoreFileResult::Changed {
            current_text,
            current_hash,
            turn_start_text,
        },
        RestoreResult::Invalid { errors } => RestoreFileResult::Invalid { errors },
    }
}

fn known_turn(
    store: &TextStore,
    key: &TurnId,
    body: &RestoreRequest,
    bridge: Option<&Bridge>,
) -> bool {
    if store.turn_ended(key)
        || body.paths.iter().any(|path| {
            save_path_of(path).is_some() && store.turn_pin(key, Path::new(path)).is_some()
        })
    {
        return true;
    }
    let Some(bridge) = bridge else {
        return false;
    };
    let snapshot = bridge.chat_snapshot(false);
    snapshot
        .session
        .as_ref()
        .is_some_and(|session| session.id == body.session)
        && snapshot
            .turns
            .iter()
            .any(|turn| turn.id == body.turn && turn.open)
}

pub(super) fn turn_sections(req: &ApiRequest<'_>) -> ApiResponse {
    respond(turn_sections_response(req))
}

fn turn_sections_response(req: &ApiRequest<'_>) -> Result<TurnSectionsResponse, ApiResponse> {
    let session = req.query.get("session").unwrap_or_default();
    let turn = req.query.get("turn").unwrap_or_default();
    let key = turn_key(session, turn)?;
    let store = text_store(req.ctx)?;
    // An unfinished turn has no end texts; it must never be ended by this GET.
    if !store.turn_ended(&key) {
        return Err(unknown_turn());
    }
    // For an ended turn end_turn returns its retained changes without reading disk.
    let changed = store.end_turn(&key, req.ctx.root(), &req.ctx.project());
    let files = changed
        .into_iter()
        .filter_map(|change| {
            let path = change.path.to_str()?;
            save_path_of(path)?;
            let start = store.turn_pin(&key, &change.path);
            let end = store.turn_end(&key, &change.path);
            Some(TurnSectionsFile {
                path: path.to_owned(),
                anchors: sections::turn_sections(
                    start.as_ref().map_or("", |pin| pin.text.as_ref()),
                    end.as_ref().map_or("", |pin| pin.text.as_ref()),
                ),
            })
        })
        .collect();
    Ok(TurnSectionsResponse {
        turn: turn.to_owned(),
        files,
    })
}

fn turn_key(session: &str, turn: &str) -> Result<TurnId, ApiResponse> {
    for (name, value) in [("session", session), ("turn", turn)] {
        if value.is_empty()
            || value.len() > 128
            || value.contains('/')
            || value.chars().any(char::is_control)
        {
            return Err(ApiResponse::error(
                400,
                "bad_request",
                &format!("{name} must be 1 to 128 bytes with no slash or control characters"),
            ));
        }
    }
    Ok(TurnId(format!("{session}/{turn}")))
}

fn unknown_turn() -> ApiResponse {
    ApiResponse::error(404, "not_found", "the turn is unknown or has not ended")
}
