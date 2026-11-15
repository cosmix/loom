//! Partition working-tree changes for the documentation commit and run lanes.

use std::path::Path;

use anyhow::ensure;

use crate::git::runner::run_git;
use crate::git::status_entries::{parse_porcelain_v1_z, PORCELAIN_V1_Z};
use crate::plan_server::wire::run::ChangedPath;

/// Return documentation changes and tracked changes that block starting a run.
pub fn split_changes(root: &Path) -> anyhow::Result<(Vec<ChangedPath>, Vec<ChangedPath>)> {
    let output = run_git(&PORCELAIN_V1_Z, root)?;
    ensure!(
        output.status.success(),
        "git status failed ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    parse_changes(&output.stdout)
}

fn parse_changes(output: &[u8]) -> anyhow::Result<(Vec<ChangedPath>, Vec<ChangedPath>)> {
    let (mut docs, mut blocking) = (Vec::new(), Vec::new());
    for entry in parse_porcelain_v1_z(output)? {
        let status = entry.code_str();
        classify(&entry.path, status, &mut docs, &mut blocking);
        if let Some(source) = &entry.origin {
            // A rename removes its tracked source, so a source outside doc/ blocks a run;
            // a copy leaves its source unchanged, so only a doc/ source is listed.
            if entry.code.contains(&b'R') || is_doc_path(source) {
                classify(source, status, &mut docs, &mut blocking);
            }
        }
    }
    Ok((docs, blocking))
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
