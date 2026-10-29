//! Partition working-tree changes for the documentation commit and run lanes.

use std::path::Path;

use anyhow::{ensure, Context};

use crate::git::runner::run_git;
use crate::plan_server::wire::run::ChangedPath;

/// Return documentation changes and tracked changes that block starting a run.
pub fn split_changes(root: &Path) -> anyhow::Result<(Vec<ChangedPath>, Vec<ChangedPath>)> {
    let output = run_git(
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
        root,
    )?;
    ensure!(
        output.status.success(),
        "git status failed ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    parse_changes(&output.stdout)
}

fn parse_changes(output: &[u8]) -> anyhow::Result<(Vec<ChangedPath>, Vec<ChangedPath>)> {
    // Lossy decoding could put a different filename in the commit allowlist.
    let output = std::str::from_utf8(output).context("git status contains a non-UTF-8 path")?;
    ensure!(
        output.is_empty() || output.ends_with('\0'),
        "git status record lacks a terminating NUL"
    );
    let (mut docs, mut blocking) = (Vec::new(), Vec::new());
    let mut records = output.split_terminator('\0');
    while let Some(record) = records.next() {
        let (status, path) = parse_record(record)?;
        classify(path, status, &mut docs, &mut blocking);
        if status.bytes().any(|byte| matches!(byte, b'R' | b'C')) {
            // Porcelain -z emits the destination first, then the source alone.
            let source = records
                .next()
                .context("git status rename/copy lacks a source")?;
            ensure!(!source.is_empty(), "git status rename/copy source is empty");
            // A rename removes its tracked source, so a source outside doc/ blocks a run;
            // a copy leaves its source unchanged, so only a doc/ source is listed.
            if status.contains('R') || is_doc_path(source) {
                classify(source, status, &mut docs, &mut blocking);
            }
        }
    }
    Ok((docs, blocking))
}

fn parse_record(record: &str) -> anyhow::Result<(&str, &str)> {
    let status = record
        .get(..2)
        .context("git status record lacks an XY status")?;
    ensure!(
        status != "  "
            && status.bytes().all(|byte| {
                matches!(
                    byte,
                    b' ' | b'M' | b'A' | b'D' | b'R' | b'C' | b'U' | b'T' | b'?' | b'!'
                )
            })
            && record.as_bytes().get(2) == Some(&b' '),
        "invalid git status record header"
    );
    let path = record.get(3..).context("git status record lacks a path")?;
    ensure!(!path.is_empty(), "git status path is empty");
    Ok((status, path))
}

fn classify(
    path: &str,
    status: &str,
    docs: &mut Vec<ChangedPath>,
    blocking: &mut Vec<ChangedPath>,
) {
    let target = if is_doc_path(path) {
        if is_summary_temp(path) {
            return;
        }
        docs
    } else if status != "??" && status != "!!" {
        blocking
    } else {
        return;
    };
    target.push(ChangedPath {
        path: path.to_owned(),
        status: status.to_owned(),
    });
}

fn is_doc_path(path: &str) -> bool {
    path.starts_with("doc/") && !path.contains("..")
}

fn is_summary_temp(path: &str) -> bool {
    path.strip_prefix("doc/loom/summaries/")
        .and_then(|relative| relative.rsplit('/').next())
        .and_then(|name| name.strip_prefix('.'))
        .is_some_and(|name| name.contains(".tmp-"))
}

#[cfg(test)]
mod tests;
