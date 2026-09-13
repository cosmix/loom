//! Path layout for `W/inbox/`: the daemon-owned relay inbox
//! (`doc/plans/PLAN-loom-state-confinement.md` section 8).

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::validation::validate_id;

/// `W/inbox`.
pub fn inbox_root(work_dir: &Path) -> PathBuf {
    work_dir.join("inbox")
}

/// `W/inbox/<session-id>`, after validating the session id.
pub fn session_inbox(work_dir: &Path, session_id: &str) -> Result<PathBuf> {
    validate_id(session_id).context("invalid inbox session id")?;
    Ok(inbox_root(work_dir).join(session_id))
}

/// `W/inbox/<session-id>/<id>.json` as an absolute path. Read-only lookups
/// (`request_status`) use this; a writer building the same place under a
/// `W`-rooted dirfd uses [`tmp_dir_relpath`] plus [`entry_file_name`] instead,
/// since `safe_fs` writes are relpath-based.
pub(crate) fn entry_path(work_dir: &Path, session_id: &str, id: &str) -> Result<PathBuf> {
    Ok(session_inbox(work_dir, session_id)?.join(entry_file_name(id)?))
}

/// `inbox`, relative to a `W`-rooted dirfd.
pub(crate) fn inbox_root_relpath() -> PathBuf {
    PathBuf::from("inbox")
}

/// `inbox/<session-id>`, relative to a `W`-rooted dirfd.
pub(crate) fn session_relpath(session_id: &str) -> Result<PathBuf> {
    validate_id(session_id).context("invalid inbox session id")?;
    Ok(inbox_root_relpath().join(session_id))
}

/// `inbox/<session-id>/.tmp`, relative to a `W`-rooted dirfd.
pub(crate) fn tmp_dir_relpath(session_id: &str) -> Result<PathBuf> {
    Ok(session_relpath(session_id)?.join(".tmp"))
}

/// `inbox/<session-id>/ledger.jsonl`, relative to a `W`-rooted dirfd.
pub(crate) fn ledger_relpath(session_id: &str) -> Result<PathBuf> {
    Ok(session_relpath(session_id)?.join("ledger.jsonl"))
}

/// `<id>.json`, after validating `id` is 32 lowercase hex characters.
pub(crate) fn entry_file_name(id: &str) -> Result<String> {
    validate_request_id(id)?;
    Ok(format!("{id}.json"))
}

/// Validate that `id` is exactly 32 lowercase hex characters — the shape
/// every request id takes (`relay::new_request_id`). Distinct from
/// [`validate_id`], which allows uppercase and underscores and is meant for
/// session/stage identifiers, not content-addressed request ids.
pub fn validate_request_id(id: &str) -> Result<()> {
    let valid = id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if !valid {
        bail!("request id '{id}' must be 32 lowercase hex characters");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_inbox_rejects_a_traversal_attempt() {
        let root = Path::new("/work");
        assert!(session_inbox(root, "../escape").is_err());
    }

    #[test]
    fn session_inbox_joins_a_valid_id() {
        let root = Path::new("/work");
        assert_eq!(
            session_inbox(root, "session-1").unwrap(),
            root.join("inbox").join("session-1")
        );
    }

    #[test]
    fn validate_request_id_accepts_the_expected_shape() {
        assert!(validate_request_id(&crate::relay::new_request_id()).is_ok());
    }

    #[test]
    fn validate_request_id_rejects_uppercase() {
        assert!(validate_request_id("4F1C9E0A7B2D4C6E8F00112233445566").is_err());
    }

    #[test]
    fn validate_request_id_rejects_wrong_length() {
        assert!(validate_request_id("abc").is_err());
    }

    #[test]
    fn entry_path_rejects_an_invalid_id() {
        let root = Path::new("/work");
        assert!(entry_path(root, "session-1", "not-hex").is_err());
    }
}
