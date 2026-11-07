//! The choices a pending freeze request was sent with.
//!
//! The wait (`host_verify::wait`) follows a request state that matches HEAD
//! and the tree digest without relaying anything, so a rerun under other
//! flags would follow the job run under the old ones and its flags would
//! never reach the daemon. The request state records no choices, so the
//! freeze keeps them in a companion file beside it and refuses such a rerun
//! before it follows or relays anything. The request state outlives a finished
//! freeze job until `freeze_follow` releases it, so this check covers a job
//! that has finished but whose application is not yet followed.

use std::io::Write;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::host_verify::wait::{freeze_choices_path, pending_request_id, WaitSite};
use crate::plan::schema::ContractChoices;
use crate::verify::contracts::choices::merge_choices;

/// The companion file: the request it belongs to and the flags it was sent with.
#[derive(Serialize, Deserialize)]
struct SentChoices {
    request_id: String,
    choices: ContractChoices,
}

/// Refuse a call whose flags add or change a value of the pending request's
/// choices. Repeating the same flags, a subset of them, or none follows the
/// pending request. A companion file that is missing, unreadable or names
/// another request refuses nothing.
pub(super) fn refuse_other_flags(site: &WaitSite<'_>, parsed: &ContractChoices) -> Result<()> {
    let Some(pending) = pending_request_id(site)? else {
        return Ok(());
    };
    let sent = read_sent(site).filter(|sent| sent.request_id == pending);
    match sent {
        Some(sent) if merge_choices(&sent.choices, parsed) != sent.choices => bail!(
            "a freeze under other flags is pending; wait for its Host job result, then rerun \
             with the new flags"
        ),
        _ => Ok(()),
    }
}

/// Keep the choices `request_id` was sent with. Best-effort: without the
/// file a rerun under other flags follows the pending request, so a write
/// failure is reported and the freeze goes on.
pub(super) fn record_sent(site: &WaitSite<'_>, request_id: &str, choices: &ContractChoices) {
    if let Err(error) = write_sent(site, request_id, choices) {
        eprintln!("warning: the flags of this freeze cannot be kept for a rerun: {error:#}");
    }
}

fn read_sent(site: &WaitSite<'_>) -> Option<SentChoices> {
    let path = freeze_choices_path(site).ok()?;
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// Atomic: a temp file in the same directory, renamed over.
fn write_sent(site: &WaitSite<'_>, request_id: &str, choices: &ContractChoices) -> Result<()> {
    let path = freeze_choices_path(site)?;
    let dir = path.parent().context("the choices file has no directory")?;
    std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let sent = SentChoices {
        request_id: request_id.to_string(),
        choices: choices.clone(),
    };
    let mut file = tempfile::NamedTempFile::new_in(dir)?;
    file.write_all(&serde_json::to_vec(&sent)?)?;
    file.persist(&path)
        .with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}
