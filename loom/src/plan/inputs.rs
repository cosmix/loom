//! Discover a plan's inputs: the plan file, its conventional brief bundle, and
//! every `doc/plans/briefs/...` reference found in them, recursively.
//!
//! Shared by `loom run` (which refuses to start on any problem here) and
//! `loom plan verify` (which reports the same problems before the run).
//! Invalid references are collected, not raised, so one pass lists them all.

use anyhow::{Context, Result};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::git::runner::run_git_checked;
use crate::plan::schema::extract_brief_paths;

#[cfg(test)]
mod tests;

/// Where a reference was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Origin {
    /// The plan file itself.
    Plan,
    /// The conventional `doc/plans/briefs/<slug>` bundle.
    Bundle,
    /// A token extracted from this input file.
    File(PathBuf),
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Origin::Plan => write!(f, "the plan itself"),
            Origin::Bundle => write!(f, "the plan's brief bundle"),
            Origin::File(path) => write!(f, "{}", path.display()),
        }
    }
}

/// A reference that `loom run` cannot follow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InvalidReference {
    pub token: String,
    pub origin: Origin,
    pub reason: String,
}

impl fmt::Display for InvalidReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} (referenced in {}): {}",
            self.token, self.origin, self.reason
        )
    }
}

/// Result of discovery.
#[derive(Debug, Default)]
pub(crate) struct Inputs {
    /// Every input file path (repo-relative, present or not) with the origin
    /// that first reached it.
    pub files: BTreeMap<PathBuf, Origin>,
    pub invalid: Vec<InvalidReference>,
}

impl Inputs {
    fn reject(&mut self, path: &Path, origin: &Origin, reason: impl Into<String>) {
        let found = InvalidReference {
            token: path.display().to_string(),
            origin: origin.clone(),
            reason: reason.into(),
        };
        if !self.invalid.contains(&found) {
            self.invalid.push(found);
        }
    }
}

enum Expansion {
    NotAPattern,
    Expanded,
    Invalid(String),
}

/// Walk the plan's inputs. I/O failures propagate; unusable references are
/// recorded in `Inputs::invalid`.
pub(crate) fn collect_inputs(root: &Path, plan: &Path) -> Result<Inputs> {
    let plan = plan.strip_prefix(root).unwrap_or(plan);
    let root = root.canonicalize()?;
    let mut found = Inputs::default();
    let plan = match relative_path(&root, plan) {
        Ok(plan) => plan,
        Err(reason) => {
            found.reject(plan, &Origin::Plan, reason);
            return Ok(found);
        }
    };
    let mut pending = VecDeque::from([(plan.clone(), Origin::Plan)]);
    pending.extend(bundle_dir(&root, &plan).map(|bundle| (bundle, Origin::Bundle)));
    let mut visited = BTreeSet::new();
    while let Some((path, origin)) = pending.pop_front() {
        match expand_pattern(&root, &path, &origin, &mut pending)? {
            Expansion::Expanded => continue,
            Expansion::Invalid(reason) => {
                found.reject(&path, &origin, reason);
                continue;
            }
            Expansion::NotAPattern => {}
        }
        let path = match relative_path(&root, &path) {
            Ok(path) => path,
            Err(reason) => {
                found.reject(&path, &origin, reason);
                continue;
            }
        };
        if !visited.insert(path.clone()) {
            continue;
        }
        let absolute = root.join(&path);
        if absolute.is_dir() {
            for entry in fs::read_dir(&absolute)? {
                pending.push_back((path.join(entry?.file_name()), origin.clone()));
            }
        } else {
            visit_file(&root, path, origin, &mut found, &mut pending)?;
        }
    }
    Ok(found)
}

/// The conventional bundle, so every worker brief is an input even when the
/// plan only links to coordinator briefs that in turn name the workers.
fn bundle_dir(root: &Path, plan: &Path) -> Option<PathBuf> {
    let slug = plan
        .file_stem()?
        .to_str()?
        .trim_start_matches("IN_PROGRESS-")
        .trim_start_matches("DONE-")
        .trim_start_matches("PLAN-");
    let bundle = PathBuf::from("doc/plans/briefs").join(slug);
    root.join(&bundle).exists().then_some(bundle)
}

/// Record a file input and queue the brief references a markdown input names.
fn visit_file(
    root: &Path,
    path: PathBuf,
    origin: Origin,
    found: &mut Inputs,
    pending: &mut VecDeque<(PathBuf, Origin)>,
) -> Result<()> {
    let absolute = root.join(&path);
    found.files.entry(path.clone()).or_insert(origin);
    if absolute.is_file() && absolute.extension().is_some_and(|ext| ext == "md") {
        let content = fs::read_to_string(&absolute)
            .with_context(|| format!("Cannot read plan input {}", absolute.display()))?;
        let source = Origin::File(path);
        pending.extend(
            extract_brief_paths(&content)
                .into_iter()
                .map(|token| (PathBuf::from(token), source.clone())),
        );
    }
    Ok(())
}

fn expand_pattern(
    root: &Path,
    path: &Path,
    origin: &Origin,
    pending: &mut VecDeque<(PathBuf, Origin)>,
) -> Result<Expansion> {
    let text = path.to_str().context("Plan input path is not UTF-8")?;
    if root.join(path).exists() || !text.contains(['*', '?', '[']) {
        return Ok(Expansion::NotAPattern);
    }
    let pattern = format!(
        "{}/{}",
        glob::Pattern::escape(&root.to_string_lossy()),
        text
    );
    let paths = match glob::glob(&pattern) {
        Ok(paths) => paths,
        Err(error) => {
            return Ok(Expansion::Invalid(format!(
                "not a valid path or glob: {}",
                error.msg
            )))
        }
    };
    let matches = paths.collect::<std::result::Result<Vec<_>, _>>()?;
    if matches.is_empty() {
        // Retain the literal path for the missing-input diagnostic.
        return Ok(Expansion::NotAPattern);
    }
    pending.extend(matches.into_iter().map(|m| (m, origin.clone())));
    Ok(Expansion::Expanded)
}

fn relative_path(root: &Path, path: &Path) -> std::result::Result<PathBuf, &'static str> {
    let absolute = root.join(path);
    let relative = absolute
        .strip_prefix(root)
        .map_err(|_| "must be inside the repository")?;
    if relative
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err("must not traverse parent directories");
    }
    if let Ok(resolved) = absolute.canonicalize() {
        if resolved != absolute.components().collect::<PathBuf>() {
            return Err("must use a regular repository path, not a symlink");
        }
    }
    Ok(relative.components().collect())
}

/// Why `path` would make `loom run` refuse, or `None` when it is committed
/// and unchanged. Errors mean git could not answer.
pub(crate) fn uncommitted_reason(root: &Path, path: &Path) -> Result<Option<&'static str>> {
    if !root.join(path).is_file() {
        return Ok(Some("missing file"));
    }
    let path = path.to_str().context("Plan input path is not UTF-8")?;
    let literal = format!(":(literal){path}");
    let committed = run_git_checked(&["ls-tree", "HEAD", "--", &literal], root)?;
    if committed.is_empty() {
        return Ok(Some("not committed (untracked, ignored, or staged only)"));
    }
    let status = run_git_checked(
        &[
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "--",
            &literal,
        ],
        root,
    )?;
    if !status.is_empty() {
        return Ok(Some("uncommitted changes"));
    }
    // `assume-unchanged` and `skip-worktree` can hide edits from git status.
    let actual = run_git_checked(&["hash-object", "--path", path, "--", path], root)?;
    if committed.split_whitespace().nth(2) != Some(actual.as_str()) {
        return Ok(Some("working copy differs from the committed file"));
    }
    Ok(None)
}
