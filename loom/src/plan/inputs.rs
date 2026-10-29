//! Discover a plan's inputs: the plan file, its `briefs/<slug>` bundle in the
//! plan's home, and standalone or feature brief references found in them, recursively.
//!
//! Shared by `loom run` (which refuses to start on any problem here) and
//! `loom plan verify` (which reports the same problems before the run).
//! Invalid references are collected, not raised, so one pass lists them all.

use anyhow::{Context, Result};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::fs::safe_read::{read_to_string_bounded, OverLimit};
use crate::plan::schema::extract_brief_paths;
use crate::planning::discover::MAX_DOC_BYTES;
use crate::planning::slug::{plan_home, plan_slug};

mod git_state;
#[cfg(test)]
mod git_state_tests;
#[cfg(test)]
mod retired_tests;
#[cfg(test)]
mod tests;

use git_state::{changed_paths, committed_objects, working_objects};

/// Where a reference was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Origin {
    /// The plan file itself.
    Plan,
    /// The conventional `briefs/<slug>` bundle in the plan's home.
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
    collect_inputs_with_text(root, plan, None)
}

/// [`collect_inputs`] with the plan's own text taken from `plan_text` when it is given, so a
/// proposed edit is checked before it is on disk. Every other input is read from the tree.
pub(crate) fn collect_inputs_with_text(
    root: &Path,
    plan: &Path,
    plan_text: Option<&str>,
) -> Result<Inputs> {
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
            let text = plan_text.filter(|_| path == plan);
            visit_file(&root, path, origin, text, &mut found, &mut pending)?;
        }
    }
    Ok(found)
}

/// The conventional bundle, so every worker brief is an input even when the
/// plan only links to coordinator briefs that in turn name the workers.
fn bundle_dir(root: &Path, plan: &Path) -> Option<PathBuf> {
    let slug = plan_slug(plan.file_name()?.to_str()?)?;
    let bundle = plan_home(plan)?.join("briefs").join(slug);
    root.join(&bundle).exists().then_some(bundle)
}

/// Record a file input and queue the brief references a markdown input names. `text`, when
/// given, is the input's content in place of the file on disk.
fn visit_file(
    root: &Path,
    path: PathBuf,
    origin: Origin,
    text: Option<&str>,
    found: &mut Inputs,
    pending: &mut VecDeque<(PathBuf, Origin)>,
) -> Result<()> {
    found.files.entry(path.clone()).or_insert(origin.clone());
    let Some(content) = markdown_content(root, &path, &origin, text, found)? else {
        return Ok(());
    };
    let source = Origin::File(path);
    pending.extend(
        extract_brief_paths(&content)
            .into_iter()
            .map(|token| (PathBuf::from(token), source.clone())),
    );
    Ok(())
}

/// The content of a markdown input to scan for references: `text` when given, else the file.
/// `None` for a non-markdown or absent input. A markdown input over [`MAX_DOC_BYTES`] stays an
/// input (so it is still locked) but is recorded as invalid and never read: a sparse file would
/// otherwise exhaust memory on every load of the plan.
fn markdown_content<'a>(
    root: &Path,
    path: &Path,
    origin: &Origin,
    text: Option<&'a str>,
    found: &mut Inputs,
) -> Result<Option<Cow<'a, str>>> {
    let limit = || format!("larger than {} MiB", MAX_DOC_BYTES / (1024 * 1024));
    if path.extension().is_none_or(|ext| ext != "md") {
        return Ok(None);
    }
    if let Some(text) = text {
        if text.len() > MAX_DOC_BYTES {
            found.reject(path, origin, limit());
            return Ok(None);
        }
        return Ok(Some(Cow::Borrowed(text)));
    }
    let absolute = root.join(path);
    if !absolute.is_file() {
        return Ok(None);
    }
    match read_to_string_bounded(root, path, MAX_DOC_BYTES) {
        Ok(content) => Ok(Some(Cow::Owned(content))),
        Err(error) if error.is::<OverLimit>() => {
            found.reject(path, origin, limit());
            Ok(None)
        }
        Err(error) => {
            Err(error).with_context(|| format!("Cannot read plan input {}", absolute.display()))
        }
    }
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
    // Checked before globbing: the walk would list directories outside the repository, and
    // `reject` would publish the matched paths.
    if path
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Ok(Expansion::Invalid(
            "must not traverse parent directories".to_string(),
        ));
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
    Ok(uncommitted_reasons(root, &[path])?
        .into_iter()
        .flatten()
        .next())
}

/// Why each of `paths` (repo-relative files that exist, in order) would make `loom run`
/// refuse, or `None` for one that is committed and unchanged. One `ls-tree`, one `status` (and
/// the `rev-parse` that places its paths) and one `hash-object` answer every path, so a plan
/// with many inputs costs four git calls. Errors mean git could not answer.
pub(crate) fn uncommitted_reasons(
    root: &Path,
    paths: &[&Path],
) -> Result<Vec<Option<&'static str>>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let paths: Vec<&str> = paths
        .iter()
        .map(|path| path.to_str().context("Plan input path is not UTF-8"))
        .collect::<Result<_>>()?;
    let literals: Vec<String> = paths.iter().map(|p| format!(":(literal){p}")).collect();
    let committed = committed_objects(root, &literals)?;
    let changed = changed_paths(root, &literals)?;
    // `assume-unchanged` and `skip-worktree` can hide edits from git status.
    let unchanged: Vec<&str> = paths
        .iter()
        .copied()
        .filter(|path| committed.contains_key(*path) && !changed.contains(*path))
        .collect();
    let actual = working_objects(root, &unchanged)?;
    let reason = |path: &str| match committed.get(path) {
        None => Some("not committed (untracked, ignored, or staged only)"),
        Some(_) if changed.contains(path) => Some("uncommitted changes"),
        Some(object) if actual.get(path) != Some(object) => {
            Some("working copy differs from the committed file")
        }
        Some(_) => None,
    };
    Ok(paths.iter().map(|&path| reason(path)).collect())
}

#[cfg(test)]
mod bundle_tests {
    use super::*;

    #[test]
    fn repeated_plan_prefix_keeps_the_same_bundle_slug_as_the_locked_set() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path();
        let plan = Path::new("doc/plans/demo/PLAN-PLAN-x.md");
        let brief = Path::new("doc/plans/demo/briefs/PLAN-x/worker.md");
        let wrong = Path::new("doc/plans/demo/briefs/x/worker.md");
        fs::create_dir_all(root.join(brief.parent().unwrap())).unwrap();
        fs::create_dir_all(root.join(wrong.parent().unwrap())).unwrap();
        fs::write(root.join(plan), "# Plan\n").unwrap();
        fs::write(root.join(brief), "# Worker\n").unwrap();
        fs::write(root.join(wrong), "# Wrong bundle\n").unwrap();

        let inputs = collect_inputs(root, plan).unwrap();

        assert_eq!(inputs.files.get(brief), Some(&Origin::Bundle));
        assert!(!inputs.files.contains_key(wrong));
        assert!(inputs.invalid.is_empty());
    }

    #[test]
    fn non_plan_names_have_no_bundle() {
        let temp = tempfile::TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join("doc/plans/briefs/notes")).unwrap();
        assert_eq!(
            bundle_dir(temp.path(), Path::new("doc/plans/notes.md")),
            None
        );
    }
}
