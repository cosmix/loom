//! Verification-point checks for declared frontmatter sources.

use super::{CatalogIssue, EvidenceUnavailableReason};
use crate::fs::knowledge::frontmatter::Frontmatter;
use crate::git::run_git;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Output;

mod summary;

pub use summary::{evidence_summary, EvidenceSummary};

const MAX_GIT_OUTPUT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct EvidenceKey {
    verified: String,
    sources: BTreeSet<String>,
}

#[derive(Debug)]
struct EvidenceRequest {
    file: PathBuf,
    sources: Vec<String>,
}

/// Defers git work until all knowledge files have been parsed, so matching
/// verification/source declarations share one bounded set of git probes.
pub(super) struct EvidenceCollector {
    project_root: Option<PathBuf>,
    groups: BTreeMap<EvidenceKey, Vec<EvidenceRequest>>,
    issues: Vec<CatalogIssue>,
}

impl EvidenceCollector {
    pub(super) fn new(project_root: Option<&Path>) -> Self {
        Self {
            project_root: project_root.map(Path::to_path_buf),
            groups: BTreeMap::new(),
            issues: Vec::new(),
        }
    }

    fn register(&mut self, file: &Path, frontmatter: &Frontmatter) {
        if frontmatter.sources.is_empty() {
            return;
        }
        let Some(verified) = frontmatter.verified.as_deref().map(str::trim) else {
            self.push_unavailable(
                file,
                &frontmatter.sources,
                EvidenceUnavailableReason::MissingRevision,
            );
            return;
        };
        if !revision_argument_is_safe(verified) {
            self.push_unavailable(
                file,
                &frontmatter.sources,
                EvidenceUnavailableReason::InvalidRevision,
            );
            return;
        }
        self.record_group(file, verified, frontmatter.sources.clone());
    }

    fn record_group(&mut self, file: &Path, verified: &str, sources: Vec<String>) {
        let key = EvidenceKey {
            verified: verified.to_string(),
            sources: sources
                .iter()
                .map(|source| normalize_path(source))
                .collect(),
        };
        self.groups.entry(key).or_default().push(EvidenceRequest {
            file: file.into(),
            sources,
        });
    }

    pub(super) fn finish(mut self) -> Vec<CatalogIssue> {
        if self.groups.is_empty() {
            return self.issues;
        }
        let Some(project_root) = self.project_root.clone() else {
            self.push_all_unavailable(EvidenceUnavailableReason::MissingRepository);
            return self.issues;
        };
        if let Err(reason) = repository_status(&project_root) {
            self.push_all_unavailable(reason);
            return self.issues;
        }
        let groups = std::mem::take(&mut self.groups);
        for (key, requests) in groups {
            match changed_paths(&project_root, &key) {
                Ok(paths) => self.push_changed(&requests, &paths, &key.verified),
                Err(reason) => self.push_unavailable_group(&requests, reason),
            }
        }
        self.issues
    }

    fn push_unavailable(
        &mut self,
        file: &Path,
        sources: &[String],
        reason: EvidenceUnavailableReason,
    ) {
        self.issues.extend(
            sources
                .iter()
                .map(|source_path| CatalogIssue::EvidenceUnavailable {
                    file: file.into(),
                    source_path: source_path.clone(),
                    reason,
                }),
        );
    }

    fn push_all_unavailable(&mut self, reason: EvidenceUnavailableReason) {
        let groups = std::mem::take(&mut self.groups);
        for requests in groups.into_values() {
            self.push_unavailable_group(&requests, reason);
        }
    }

    fn push_unavailable_group(
        &mut self,
        requests: &[EvidenceRequest],
        reason: EvidenceUnavailableReason,
    ) {
        for request in requests {
            self.push_unavailable(&request.file, &request.sources, reason);
        }
    }

    fn push_changed(
        &mut self,
        requests: &[EvidenceRequest],
        paths: &BTreeSet<String>,
        verified: &str,
    ) {
        for request in requests {
            self.issues.extend(
                request
                    .sources
                    .iter()
                    .filter(|source| paths.contains(&normalize_path(source)))
                    .map(|source_path| CatalogIssue::EvidenceChanged {
                        file: request.file.clone(),
                        source_path: source_path.clone(),
                        verified: verified.to_string(),
                    }),
            );
        }
    }
}

/// Register one file for deferred freshness comparison during this catalog build.
pub(super) fn changed_since_verified(
    collector: &mut EvidenceCollector,
    file: &Path,
    frontmatter: &Frontmatter,
) {
    collector.register(file, frontmatter);
}

fn revision_argument_is_safe(revision: &str) -> bool {
    !revision.is_empty() && !revision.starts_with('-') && !revision.contains('\0')
}

fn repository_status(project_root: &Path) -> Result<(), EvidenceUnavailableReason> {
    let output = git_output(
        project_root,
        vec!["rev-parse".into(), "--is-inside-work-tree".into()],
    )?;
    let is_worktree = std::str::from_utf8(&output.stdout).is_ok_and(|value| value.trim() == "true");
    (output.status.success() && is_worktree)
        .then_some(())
        .ok_or(EvidenceUnavailableReason::MissingRepository)
}

fn changed_paths(
    project_root: &Path,
    key: &EvidenceKey,
) -> Result<BTreeSet<String>, EvidenceUnavailableReason> {
    validate_revision(project_root, &key.verified)?;
    let mut paths = committed_paths(project_root, key)?;
    paths.extend(working_tree_paths(project_root, key)?);
    Ok(paths
        .into_iter()
        .filter(|path| key.sources.contains(path))
        .collect())
}

fn validate_revision(project_root: &Path, verified: &str) -> Result<(), EvidenceUnavailableReason> {
    let commit = format!("{verified}^{{commit}}");
    let output = git_output(
        project_root,
        vec!["rev-parse".into(), "--verify".into(), commit],
    )?;
    output
        .status
        .success()
        .then_some(())
        .ok_or(EvidenceUnavailableReason::InvalidRevision)
}

fn committed_paths(
    project_root: &Path,
    key: &EvidenceKey,
) -> Result<BTreeSet<String>, EvidenceUnavailableReason> {
    let mut args = vec![
        "diff".into(),
        "--name-status".into(),
        "-z".into(),
        "--find-renames".into(),
        "--no-ext-diff".into(),
        format!("{}..HEAD", key.verified),
        "--".into(),
    ];
    args.extend(key.sources.iter().map(|source| literal_pathspec(source)));
    let output = git_output(project_root, args)?;
    output
        .status
        .success()
        .then(|| parse_diff_paths(&output.stdout))
        .ok_or(EvidenceUnavailableReason::CommandFailed)
}

fn working_tree_paths(
    project_root: &Path,
    key: &EvidenceKey,
) -> Result<BTreeSet<String>, EvidenceUnavailableReason> {
    let mut args = vec![
        "status".into(),
        "--porcelain=v1".into(),
        "-z".into(),
        "--untracked-files=all".into(),
        "--ignore-submodules=all".into(),
        "--".into(),
    ];
    args.extend(key.sources.iter().map(|source| literal_pathspec(source)));
    let output = git_output(project_root, args)?;
    output
        .status
        .success()
        .then(|| parse_status_paths(&output.stdout))
        .ok_or(EvidenceUnavailableReason::CommandFailed)
}

fn git_output(project_root: &Path, args: Vec<String>) -> Result<Output, EvidenceUnavailableReason> {
    let refs: Vec<_> = args.iter().map(String::as_str).collect();
    let output = run_git(&refs, project_root).map_err(classify_git_error)?;
    // `run_bounded_output` bounds wall-clock time and drains pipes fully, so this caps the
    // accepted result rather than peak memory; memory is bounded in practice because every call
    // restricts output to declared literal paths.
    if output.stdout.len() > MAX_GIT_OUTPUT_BYTES || output.stderr.len() > MAX_GIT_OUTPUT_BYTES {
        return Err(EvidenceUnavailableReason::ResourceLimit);
    }
    Ok(output)
}

fn classify_git_error(error: anyhow::Error) -> EvidenceUnavailableReason {
    if error.chain().any(|cause| {
        cause
            .downcast_ref::<crate::process::ProcessTimeoutError>()
            .is_some()
    }) {
        return EvidenceUnavailableReason::ResourceLimit;
    }
    if error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    }) {
        return EvidenceUnavailableReason::GitUnavailable;
    }
    EvidenceUnavailableReason::CommandFailed
}

fn parse_diff_paths(bytes: &[u8]) -> BTreeSet<String> {
    let mut fields = bytes.split(|byte| *byte == b'\0');
    let mut paths = BTreeSet::new();
    while let Some(status) = fields.next() {
        if status.is_empty() {
            continue;
        }
        let Some(path) = fields.next() else {
            break;
        };
        paths.insert(normalize_bytes(path));
        if status
            .first()
            .is_some_and(|code| matches!(*code, b'R' | b'C'))
        {
            if let Some(other_path) = fields.next() {
                paths.insert(normalize_bytes(other_path));
            }
        }
    }
    paths
}

/// Unlike `loom/src/context/refresh/source_graph/generation.rs::parse_status`, this parses raw bytes because that parser takes `run_git_checked`'s trimmed `String`; both rely on porcelain v1 `-z` rename records listing destination first and the source in the next NUL field.
fn parse_status_paths(bytes: &[u8]) -> BTreeSet<String> {
    let mut fields = bytes.split(|byte| *byte == b'\0');
    let mut paths = BTreeSet::new();
    while let Some(record) = fields.next() {
        if record.len() < 4 {
            continue;
        }
        if let Some((destination, source)) = status_rename_paths(record, &mut fields) {
            paths.insert(normalize_bytes(destination));
            paths.insert(normalize_bytes(source));
        } else {
            paths.insert(normalize_bytes(&record[3..]));
        }
    }
    paths
}

fn status_rename_paths<'a>(
    record: &'a [u8],
    fields: &mut impl Iterator<Item = &'a [u8]>,
) -> Option<(&'a [u8], &'a [u8])> {
    let destination = record.get(3..)?;
    let is_rename = record
        .first()
        .is_some_and(|code| matches!(*code, b'R' | b'C'))
        || record
            .get(1)
            .is_some_and(|code| matches!(*code, b'R' | b'C'));
    if !is_rename {
        return None;
    }
    fields.next().map(|source| (destination, source))
}

fn literal_pathspec(source: &str) -> String {
    format!(":(literal){source}")
}

fn normalize_bytes(path: &[u8]) -> String {
    normalize_path(&String::from_utf8_lossy(path))
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

#[cfg(test)]
mod tests;
