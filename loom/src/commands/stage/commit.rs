//! Relay or apply a staged stage commit.

use std::fs::File;
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use tempfile::NamedTempFile;

use crate::git::runner::run_git_checked;
use crate::git::stage_commit::{
    commit_staged, validate_commit_message, CommitRequest, CommitScope,
};
use crate::process::run_bounded_output;
use crate::relay::emit::{mode, EnvSnapshot, RelayContext, RelayMode, RelaySink, StdSink};
use crate::relay::{CommitPayload, RequestKind};

/// Submit a staged commit for a stage session.
pub fn execute(stage_id: String, message: String) -> Result<()> {
    let env = EnvSnapshot::from_process_env();
    let relay_mode = mode(&env);
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let mut sink = StdSink::default();
    commit_with(
        &stage_id,
        &message,
        env.stage_id.as_deref(),
        relay_mode,
        &cwd,
        &mut sink,
    )
}

fn commit_with(
    stage_id: &str,
    message: &str,
    session_stage: Option<&str>,
    relay_mode: RelayMode,
    cwd: &Path,
    sink: &mut dyn RelaySink,
) -> Result<()> {
    validate_message(message)?;
    if let Some(session_stage) = session_stage {
        if session_stage != stage_id {
            bail!("stage commit is only allowed for session stage '{session_stage}'");
        }
    }

    match relay_mode {
        RelayMode::Relay(context) => relay_commit(stage_id, message, &context, cwd, sink),
        RelayMode::Operator | RelayMode::Legacy => commit_in_process(stage_id, message, cwd, sink),
    }
}

fn relay_commit(
    stage_id: &str,
    message: &str,
    context: &RelayContext,
    cwd: &Path,
    sink: &mut dyn RelaySink,
) -> Result<()> {
    // SAFETY: `getuid` has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    context.check(RequestKind::Commit, Some(stage_id), cwd, uid)?;
    let message = run_hooks(cwd, message)?;
    let (expected_head, expected_tree) = staged_state(cwd)?;
    let payload = CommitPayload {
        message,
        expected_head,
        expected_tree,
    };
    let line = context.emit(
        RequestKind::Commit,
        serde_json::to_value(&payload)?,
        "stage commit",
        false,
        sink,
    )?;
    writeln!(
        sink.stderr(),
        "Wait for it in your NEXT Bash call: loom request status {} --wait 90",
        line.id
    )
    .context("failed to write stage commit confirmation")?;
    sink.stderr()
        .flush()
        .context("failed to flush stage commit confirmation")
}

fn commit_in_process(
    stage_id: &str,
    message: &str,
    cwd: &Path,
    sink: &mut dyn RelaySink,
) -> Result<()> {
    let message = run_hooks(cwd, message)?;
    let (expected_head, expected_tree) = staged_state(cwd)?;
    let top = run_git_checked(&["rev-parse", "--show-toplevel"], cwd)?;
    let request = CommitRequest {
        message,
        expected_head,
        expected_tree,
    };
    let commit = commit_staged(
        Path::new(&top),
        &CommitScope::StageBranch {
            stage_id: stage_id.to_string(),
        },
        &request,
    )
    .map_err(|refusal| anyhow::anyhow!("{refusal}"))?;
    writeln!(sink.stdout(), "committed {commit}").context("failed to write commit result")?;
    sink.stdout()
        .flush()
        .context("failed to flush commit result")
}

fn validate_message(message: &str) -> Result<()> {
    validate_commit_message(message).map_err(anyhow::Error::msg)
}

fn run_hooks(cwd: &Path, message: &str) -> Result<String> {
    let mut pre_commit = Command::new("git");
    pre_commit
        .args(["hook", "run", "--ignore-missing", "pre-commit"])
        .current_dir(cwd);
    run_checked(&mut pre_commit, "pre-commit")?;

    let mut message_file = NamedTempFile::new().context("failed to create commit message file")?;
    message_file
        .write_all(message.as_bytes())
        .context("failed to write commit message file")?;
    message_file
        .flush()
        .context("failed to flush commit message file")?;
    let path = message_file.path();
    let mut commit_msg = Command::new("git");
    commit_msg
        .args(["hook", "run", "--ignore-missing", "commit-msg", "--"])
        .arg(path)
        .current_dir(cwd);
    run_checked(&mut commit_msg, "commit-msg")?;

    let mut stripspace = Command::new("git");
    stripspace
        .arg("stripspace")
        .current_dir(cwd)
        .stdin(Stdio::from(
            File::open(path).context("failed to open commit message file")?,
        ));
    let output = run_checked(&mut stripspace, "stripspace")?;
    let message =
        String::from_utf8(output.stdout).context("stripspace emitted a non-UTF-8 message")?;
    validate_message(&message)?;
    Ok(message)
}

fn staged_state(cwd: &Path) -> Result<(String, String)> {
    let head = run_git_checked(&["rev-parse", "HEAD"], cwd)?;
    let tree = run_git_checked(&["write-tree"], cwd)?;
    Ok((head, tree))
}

fn run_checked(command: &mut Command, label: &str) -> Result<Output> {
    let output = run_bounded_output(command, Duration::from_secs(600), label)?;
    if !output.status.success() {
        bail!(
            "{label} failed:\nstderr:\n{}\nstdout:\n{}",
            last_lines(&output.stderr),
            last_lines(&output.stdout)
        );
    }
    Ok(output)
}

fn last_lines(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let lines: Vec<_> = text.lines().rev().take(20).collect();
    lines.into_iter().rev().collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests;
