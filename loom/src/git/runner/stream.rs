//! Checked Git output consumed incrementally, with the shared child lifecycle.
use super::{git_command, git_label, git_timeout, global_args};
use anyhow::{bail, Context, Result};
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Stdio;
use std::sync::mpsc;
use std::time::Instant;

/// Consume stdout without retaining the whole command output. The consumer must
/// drain its reader to EOF. Errors and timeouts always retire the process group.
pub(crate) fn run_git_stream<T: Send + 'static>(
    args: &[&str],
    root: &Path,
    consume: impl FnOnce(Box<dyn Read + Send>) -> Result<T> + Send + 'static,
) -> Result<T> {
    let mut command = git_command("git", &global_args(args), &[], root);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0);
    let mut child = crate::process::OwnedProcess::new(command.spawn().context("spawn git stream")?);
    let stdout = child.child().stdout.take().context("git stdout missing")?;
    let (send, receive) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("git-source-reader".into())
        .spawn(move || {
            let _ = send.send(consume(Box::new(stdout)));
        })
        .context("spawn git stream reader")?;
    let status = child.wait(git_timeout(args))?;
    let deadline = child.retire();
    let Some(status) = status else {
        bail!("{} timed out", git_label(args));
    };
    if !status.success() {
        bail!("{} failed in {}: {status}", git_label(args), root.display());
    }
    receive
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .context("git source reader did not finish before cleanup deadline")?
}
