//! Rebuild claims and draft paths in the runtime directory.
//!
//! A claim is `claims/<id with '/' -> "__">.json`, created with `O_EXCL`: the create failing
//! because the file exists IS the refusal. `claims/` is session-writable, so every read is
//! no-follow and bounded, and a claim that cannot be read or parsed still counts as a claim on
//! its file-name id (fail toward "claimed").

#[cfg(test)]
mod tests;

use std::io;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use super::lock::ensure_runtime_dir;
use super::store;
use crate::fs::safe_fs::{
    safe_create_dir_all_in_workdir, safe_create_new_in_workdir, safe_open_dirfd,
    safe_remove_in_workdir,
};
use crate::fs::safe_read::{is_not_found, list_dir_no_follow, read_bounded};
use crate::planning::runtime::RuntimePaths;

/// Environment variable naming the planning session that runs the command.
const SESSION_VAR: &str = "LOOM_PLAN_SESSION";
/// Name of the claims directory inside the runtime directory.
const CLAIMS_DIR: &str = "claims";
/// Largest claim file read.
const MAX_CLAIM_BYTES: usize = 4 * 1024;
/// Longest session name kept from untrusted input, in characters.
const MAX_SESSION_CHARS: usize = 128;

/// A session's claim to rebuild one summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    /// The claimed summary id.
    pub summary: String,
    /// `$LOOM_PLAN_SESSION` of the claimant; `None` for a terminal.
    pub session: Option<String>,
    /// Process id of the claimant.
    pub pid: u32,
    /// When the claim was made, RFC 3339.
    pub created_at: String,
}

/// `$LOOM_PLAN_SESSION`, sanitised; `None` when unset or empty.
pub fn current_session() -> Option<String> {
    sanitize_session(&std::env::var(SESSION_VAR).ok()?)
}

/// Untrusted session text without control characters, capped at 128 characters; `None` when
/// nothing is left.
fn sanitize_session(raw: &str) -> Option<String> {
    let clean: String = raw
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_SESSION_CHARS)
        .collect();
    (!clean.is_empty()).then_some(clean)
}

/// Runtime-relative path of the claim file of `id`.
fn claim_rel(id: &str) -> PathBuf {
    Path::new(CLAIMS_DIR).join(format!("{}.json", id.replace('/', "__")))
}

/// The summary id a claim file name stands for; ids hold at most one `/`, and the prefixes
/// `product` and `plans` hold no `__`, so the first `__` is the separator.
fn id_from_file(name: &str) -> Option<String> {
    let id = name.strip_suffix(".json")?.replacen("__", "/", 1);
    store::validate_id(&id).is_ok().then_some(id)
}

/// The draft path of summary `id`: `drafts/<id>.md` in the runtime directory. An invalid id
/// yields an empty path; validate the id and report the error first.
pub fn draft_path(paths: &RuntimePaths, id: &str) -> PathBuf {
    if store::validate_id(id).is_err() {
        return PathBuf::new();
    }
    paths.drafts.join(format!("{id}.md"))
}

/// Claim `id` for `$LOOM_PLAN_SESSION`. `Ok(Ok(claim))` when this call created the claim,
/// `Ok(Err(existing))` when a claim file already exists.
pub fn create_claim(paths: &RuntimePaths, id: &str) -> Result<Result<Claim, Claim>> {
    create_claim_as(paths, id, current_session())
}

/// [`create_claim`] for an explicit `session`.
pub(crate) fn create_claim_as(
    paths: &RuntimePaths,
    id: &str,
    session: Option<String>,
) -> Result<Result<Claim, Claim>> {
    store::validate_id(id)?;
    ensure_runtime_dir(paths)?;
    let dirfd = safe_open_dirfd(&paths.dir)?;
    safe_create_dir_all_in_workdir(dirfd.as_raw_fd(), Path::new(CLAIMS_DIR), 0o700)?;
    let claim = Claim {
        summary: id.to_string(),
        session,
        pid: std::process::id(),
        created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    };
    let json = serde_json::to_vec(&claim).context("cannot encode the claim")?;
    match safe_create_new_in_workdir(dirfd.as_raw_fd(), &claim_rel(id), &json) {
        Ok(()) => Ok(Ok(claim)),
        Err(error) if already_exists(&error) => Ok(Err(read_claim(paths, id))),
        Err(error) => Err(error.context(format!("cannot claim {id}"))),
    }
}

fn already_exists(error: &anyhow::Error) -> bool {
    error
        .chain()
        .filter_map(|cause| cause.downcast_ref::<io::Error>())
        .any(|e| e.kind() == io::ErrorKind::AlreadyExists)
}

/// The claim stored for `id`, read no-follow and bounded. A file that cannot be read or
/// parsed, or that names another summary, is a claim on `id` with no session.
fn read_claim(paths: &RuntimePaths, id: &str) -> Claim {
    let parsed = read_bounded(&paths.dir, &claim_rel(id), MAX_CLAIM_BYTES)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Claim>(&bytes).ok())
        .filter(|claim| claim.summary == id);
    match parsed {
        Some(claim) => {
            let session = claim.session.as_deref().and_then(sanitize_session);
            Claim { session, ..claim }
        }
        None => Claim {
            summary: id.to_string(),
            session: None,
            pid: 0,
            created_at: String::new(),
        },
    }
}

/// Every claim, sorted by summary id. An absent runtime or claims directory has none; a
/// symlinked `claims/` is refused.
pub fn list_claims(paths: &RuntimePaths) -> Result<Vec<Claim>> {
    let dirfd = match safe_open_dirfd(&paths.dir) {
        Ok(fd) => fd,
        Err(error) if is_not_found(&error) => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let entries = match list_dir_no_follow(&dirfd, Path::new(CLAIMS_DIR)) {
        Ok(entries) => entries,
        Err(error) if is_not_found(&error) => return Ok(Vec::new()),
        Err(error) => return Err(error.context("cannot list the claims directory")),
    };
    let mut ids: Vec<String> = entries
        .into_iter()
        .filter_map(|(name, _)| id_from_file(name.to_str()?))
        .collect();
    ids.sort();
    Ok(ids.iter().map(|id| read_claim(paths, id)).collect())
}

/// Delete the claim of `id`; an absent claim is not an error.
pub fn delete_claim(paths: &RuntimePaths, id: &str) -> Result<()> {
    store::validate_id(id)?;
    let dirfd = match safe_open_dirfd(&paths.dir) {
        Ok(fd) => fd,
        Err(error) if is_not_found(&error) => return Ok(()),
        Err(error) => return Err(error),
    };
    safe_remove_in_workdir(dirfd.as_raw_fd(), &claim_rel(id))
}

/// Give up the claims on `ids` for `$LOOM_PLAN_SESSION`; returns the ids released.
pub fn release(paths: &RuntimePaths, ids: &[String]) -> Result<Vec<String>> {
    release_as(paths, ids, current_session().as_deref())
}

/// [`release`] for an explicit `session`. With a session, a claim held by anyone else fails
/// the whole call naming the holder, before anything is deleted; without one (a terminal),
/// any claim is released. Ids with no claim are skipped.
pub(crate) fn release_as(
    paths: &RuntimePaths,
    ids: &[String],
    session: Option<&str>,
) -> Result<Vec<String>> {
    for id in ids {
        store::validate_id(id)?;
    }
    let claims = list_claims(paths)?;
    let held: Vec<&Claim> = claims.iter().filter(|c| ids.contains(&c.summary)).collect();
    if let Some(session) = session {
        if let Some(other) = held.iter().find(|c| c.session.as_deref() != Some(session)) {
            let holder = other.session.as_deref().unwrap_or("a terminal");
            bail!(
                "{} is claimed by {holder}, not by this session",
                other.summary
            );
        }
    }
    let mut released = Vec::new();
    for claim in held {
        delete_claim(paths, &claim.summary)?;
        released.push(claim.summary.clone());
    }
    Ok(released)
}

/// Delete every claim `session` holds; returns the ids released.
pub fn clear_session_claims(paths: &RuntimePaths, session: &str) -> Result<Vec<String>> {
    let mut released = Vec::new();
    for claim in list_claims(paths)? {
        if claim.session.as_deref() == Some(session) {
            delete_claim(paths, &claim.summary)?;
            released.push(claim.summary);
        }
    }
    Ok(released)
}
