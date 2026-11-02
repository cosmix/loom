//! Non-destructive publication: prepare durably, then append exactly once.

use super::*;
use std::fs::Permissions;
use std::io::Write;

#[cfg(test)]
thread_local! {
    pub(super) static AFTER_COMPARISON: std::cell::RefCell<Option<BeforePublication>> =
        const { std::cell::RefCell::new(None) };
    pub(super) static AFTER_APPEND: std::cell::RefCell<Option<BeforePublication>> =
        const { std::cell::RefCell::new(None) };
    pub(super) static WRITE_LIMIT: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

pub(super) fn publish(
    plan: &Path,
    candidate: &mut File,
    original: &Metadata,
    snapshot: &str,
    updated: &str,
    suffix: &str,
) -> Result<RecordWrite> {
    candidate
        .write_all(updated.as_bytes())
        .context("Failed to prepare pressure record")?;
    candidate.set_permissions(Permissions::from_mode(
        original.permissions().mode() & 0o7777,
    ))?;
    candidate
        .sync_all()
        .context("Failed to sync pressure record candidate")?;
    #[cfg(test)]
    run_hook(&BEFORE_PUBLICATION, plan);
    let Some(mut output) = open_append(plan, original)? else {
        return Ok(RecordWrite::PlanMoved);
    };
    if !still_matches(plan, original, snapshot)? {
        return Ok(RecordWrite::PlanMoved);
    }
    #[cfg(test)]
    run_hook(&AFTER_COMPARISON, plan);
    append_once(&mut output, suffix.as_bytes())?;
    #[cfg(test)]
    run_hook(&AFTER_APPEND, plan);
    output
        .sync_all()
        .context("Failed to sync appended pressure record; record may be present")?;
    let published = output
        .metadata()
        .context("Failed to inspect appended plan")?;
    if !still_matches(plan, &published, updated)? {
        return Ok(RecordWrite::PlanMoved);
    }
    Ok(RecordWrite::Written)
}

fn open_append(plan: &Path, original: &Metadata) -> Result<Option<File>> {
    let output = match OpenOptions::new()
        .append(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(plan)
    {
        Ok(file) => file,
        Err(error)
            if error.kind() == ErrorKind::NotFound
                || matches!(error.raw_os_error(), Some(libc::ELOOP | libc::ENOTDIR)) =>
        {
            return Ok(None)
        }
        Err(error) => {
            return Err(error).context("Failed to open plan for non-destructive record append")
        }
    };
    let current = output.metadata()?;
    if !same_revision(&current, original) {
        return Ok(None);
    }
    // A write can clear these bits; restoring them would race an external chmod.
    ensure!(
        current.mode() & 0o6000 == 0,
        "Cannot safely append a pressure record to a setuid/setgid plan"
    );
    Ok(Some(output))
}

fn append_once(output: &mut File, bytes: &[u8]) -> Result<()> {
    #[cfg(test)]
    let attempted = WRITE_LIMIT
        .with(|limit| limit.take())
        .map_or(bytes, |limit| &bytes[..limit]);
    #[cfg(not(test))]
    let attempted = bytes;
    // O_APPEND selects EOF in the same operation as this single write. Never
    // retry or truncate after a short write: an ftruncate cannot be conditional
    // on the length it checked, so it could cut an external edit that followed.
    // A fragment adds bytes at the end of the plan and removes none, and a later
    // record starts on its own line after it.
    let record = String::from_utf8_lossy(bytes);
    let recovery = format!(
        "a record fragment may remain at the end of the plan, existing text was not rewritten; \
         remove it and place this record manually:\n{record}"
    );
    let written = output
        .write(attempted)
        .with_context(|| format!("Pressure record append failed; {recovery}"))?;
    ensure!(
        written == bytes.len(),
        "Pressure record append was incomplete ({written}/{} bytes); {recovery}",
        bytes.len()
    );
    Ok(())
}

#[cfg(test)]
fn run_hook(
    hook: &'static std::thread::LocalKey<std::cell::RefCell<Option<BeforePublication>>>,
    path: &Path,
) {
    hook.with(|hook| {
        if let Some(hook) = hook.borrow_mut().take() {
            hook(path);
        }
    });
}
