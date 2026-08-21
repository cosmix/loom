//! Session-scoped delivery dedupe, and its compaction-time reset.
//!
//! `super` answers "has this recipient already been given these exact bytes,
//! this epoch?" — but until this module existed, "recipient" was scoped to
//! checkout+epoch only, never to the process asking. A fresh Claude Code
//! session — new PID, empty context window — inherited every prior session's
//! deliveries under that recipient key and could be starved of a brief an old,
//! possibly long-dead, session had already consumed. And the hook path never
//! rebuilds the source graph on its own, so nothing forced a new epoch to
//! unstick it: a fresh session could go silent indefinitely
//! (`doc/PROPOSAL-retrieval-precision.md` root cause 8, recommendation 16).
//!
//! [`hook_recipient_id`] keys a recipient to the session asking, not just the
//! scope; [`delivered_to_session`] is the read side that respects that key
//! while still honouring the one deliberate exception — a resumed stage
//! session must still see its own spawn brief as delivered. [`session8`] and
//! [`discard_session_delivery`] exist only to serve those two.
//!
//! Every contract `delivery.rs`'s module doc states still holds here without
//! restatement: a record is an optimisation nothing may fail over, and an
//! absent file or directory reads as "nothing delivered" rather than an error.

use super::{delivered_in_epoch, delivery_dir, validate_recipient_id, DeliveryRecord};
use crate::fs::locking::locked_dir_update;
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

/// The delivery-record recipient key for one hook invocation.
///
/// Shape: `prompt-<scope>-<session8>`, where `scope` is the hook's existing
/// stage id or checkout key and `session8` is 16 lowercase hex characters —
/// the first 8 bytes of `sha256(session_id)`, the same truncation
/// [`crate::context::retrieve::context_epoch`] uses for its own digest. An
/// absent, empty, or whitespace-only `session_id` yields the literal
/// `nosession` component instead: one shared recipient for every caller that
/// cannot name a session, exactly today's un-keyed behaviour.
///
/// Hashing rather than filing the raw id is not decoration. The recipient
/// reaches the filesystem as `<recipient_id>.json` and
/// [`validate_recipient_id`] refuses anything outside `[A-Za-z0-9._-]`
/// because of that — but a session id arriving on a hook's stdin JSON is
/// untrusted input, so a raw id would either be refused outright (a starved
/// session, indistinguishable from a bug) or, were that check ever loosened,
/// become a path-traversal write. A sha8 has neither problem: it is always
/// inside `[0-9a-f]`, by construction rather than by validation, whatever
/// bytes the session id itself contains.
pub fn hook_recipient_id(scope: &str, session_id: Option<&str>) -> String {
    let suffix = match session_id.map(str::trim).filter(|id| !id.is_empty()) {
        Some(id) => session8(id),
        None => "nosession".to_string(),
    };
    format!("prompt-{scope}-{suffix}")
}

/// First 8 bytes of `sha256(value)`, hex-encoded: 16 characters, all drawn
/// from `[0-9a-f]`. Same two-step shape as
/// [`crate::context::retrieve::context_epoch`]'s own digest.
fn session8(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..8])
}

/// The `(node_id, content_hash)` pairs THIS session already holds under
/// `epoch`: the union of its own record (`recipient_id`) and, when this
/// process is the session a stage spawned, the spawn record filed under
/// `spawn_recipient`.
///
/// A fresh session must skip only what its OWN context window already
/// holds — its own prior deliveries this epoch, and the spawn brief when it
/// is the spawn session — and never a dead session's. That is why this takes
/// `recipient_id` rather than deriving one internally: the caller already
/// resolved exactly which recipient this invocation is, and only the caller
/// can. `spawn_recipient` is loom's OWN session id (`LOOM_SESSION_ID`), a
/// different id space entirely from the hook payload's `session_id` that
/// [`hook_recipient_id`] hashes — the caller supplies it because only the
/// caller can tell whether this process is the session a stage spawned.
///
/// Builds on [`delivered_in_epoch`] rather than folding its filter in here:
/// that function keeps its own callers and its own contract unchanged.
pub fn delivered_to_session(
    records: &[DeliveryRecord],
    epoch: &str,
    recipient_id: &str,
    spawn_recipient: Option<&str>,
) -> BTreeSet<(String, String)> {
    let own: Vec<DeliveryRecord> = records
        .iter()
        .filter(|record| record.recipient_id == recipient_id)
        .cloned()
        .collect();
    let mut delivered = delivered_in_epoch(&own, epoch);

    if let Some(spawn_recipient) = spawn_recipient {
        let spawn: Vec<DeliveryRecord> = records
            .iter()
            .filter(|record| record.recipient_id == spawn_recipient)
            .cloned()
            .collect();
        delivered.extend(delivered_in_epoch(&spawn, epoch));
    }

    delivered
}

/// Remove one recipient's own delivery record, so it is eligible again for
/// units it was already given — used after a compaction, where the
/// compacted context can no longer be assumed to hold what was delivered
/// before it.
///
/// Removes exactly the ONE file named `<recipient_id>.json`. It never
/// touches anything else in the delivery directory — not another
/// recipient's record, and never the directory itself: that directory is a
/// shared namespace with other writers (`GraphStore::discard_overlay`'s doc
/// comment records the incident a directory-level discard caused here
/// before), so a routine that means "reset one recipient" must never reach
/// for "clear the directory" as a shortcut. A missing directory or a missing
/// file both read as "already reset", not an error — the same "absent means
/// nothing delivered" contract every reader in `super` already holds to.
pub fn discard_session_delivery(
    work_dir: &Path,
    plan: &str,
    stage: &str,
    recipient_id: &str,
) -> Result<()> {
    validate_recipient_id(recipient_id)?;
    let dir = delivery_dir(work_dir, plan, stage);
    if !dir.exists() {
        return Ok(());
    }
    let path = dir.join(format!("{recipient_id}.json"));
    locked_dir_update(&dir, || match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("Failed to remove {}", path.display())),
    })
}
