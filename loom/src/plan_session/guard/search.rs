//! Search rules of the planning guard beyond the read denies: a `Glob` pattern may not leave its
//! root, and no search may walk into the project's orchestration state root.
//!
//! The state-root rule ports `walks_into_state_root` of `loom-hooks/credential-guard.sh`, which
//! guards stage sessions. Native `Grep` and `Glob` run outside the OS sandbox; ripgrep walks an
//! explicitly named root whatever the ignore files say, and a Grep `glob` whitelist overrides
//! them. The daemon's `.ignore` at the state root (`fs/permissions/state_root.rs`,
//! `search_ignore_body`) names only the token files, not `terminal-history`, so it covers no
//! more than a plain search rooted above the directory holding the state root.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::paths::{canonical_of, inside};
use crate::fs::permissions::state_root::resolve_state_root;

/// Deny a `Grep`/`Glob` rooted at `path` (absolute, normalised) whose pattern escapes the root
/// or whose walk would reach the state root of `project_root`.
pub(super) fn check_search(
    project_root: &Path,
    tool: &str,
    input: &Value,
    path: &Path,
) -> Result<(), String> {
    if tool == "Glob" {
        check_glob_pattern(input)?;
    }
    let state = StateRoot::new(project_root);
    let canonical = canonical_of(path)?;
    for form in [path, canonical.as_path()] {
        if let Some(why) = state.walk(form, tool, input) {
            return Err(format!("a search of {}: {why}", path.display()));
        }
    }
    Ok(())
}

/// A `Glob` pattern is matched below its `path`; an absolute, `~` or `..` pattern would search
/// a root the guard never checked. Any `..` counts, as in the shell guard's `filtered_search`:
/// a brace alternative such as `{..,x}/*` climbs without being a `..` component.
fn check_glob_pattern(input: &Value) -> Result<(), String> {
    let Some(pattern) = input.get("pattern").and_then(Value::as_str) else {
        return Ok(());
    };
    if pattern.starts_with('/') || pattern.starts_with('~') || pattern.contains("..") {
        return Err(format!("the Glob pattern {pattern} leaves its path"));
    }
    Ok(())
}

/// The project's state root in its lexical (`<root>/.loom/work`) and resolved forms, and the
/// directories holding them.
struct StateRoot {
    roots: Vec<PathBuf>,
    parents: Vec<PathBuf>,
}

impl StateRoot {
    fn new(project_root: &Path) -> Self {
        let mut roots = vec![project_root.join(".loom").join("work")];
        roots.extend(resolve_state_root(project_root));
        let canonical_root = canonical_of(project_root).ok();
        // A legacy `.work` state root sits directly in the project root; its parent is the
        // project root, which a plain search may walk (the read rules' frozen contract), so it
        // is not a parent target. The state root itself and the filter rule still apply.
        let parents = roots
            .iter()
            .filter_map(|root| root.parent())
            .filter(|parent| *parent != project_root && Some(*parent) != canonical_root.as_deref())
            .map(Path::to_path_buf)
            .collect();
        StateRoot { roots, parents }
    }

    /// Why a search rooted at `form` walks into the state root: the shell guard's
    /// `state_root_target` (widened to any root inside it), `state_root_parent_target`, and
    /// `state_root_below_target` with `filtered_search`.
    fn walk(&self, form: &Path, tool: &str, input: &Value) -> Option<&'static str> {
        if self.roots.iter().any(|root| inside(form, root)) {
            Some("the search is rooted in the state root")
        } else if self.parents.iter().any(|parent| parent == form) {
            Some(
                "the search is rooted at the directory holding the state root, and no ignore \
                 file prunes an explicit root",
            )
        } else if self.roots.iter().any(|root| inside(root, form)) && grep_filtered(tool, input) {
            Some("a glob filter on a search rooted above the state root reaches into it")
        } else {
            None
        }
    }
}

/// A Grep `glob` filter, which ripgrep lets override the ignore files for any directory it
/// matches. Its text is not parsed: a glob can name a directory in more ways than a check can
/// list. A non-string value counts too. The shell guard's `Glob` half of `filtered_search` (an
/// absolute or `..` pattern) is [`check_glob_pattern`], refused for every root.
fn grep_filtered(tool: &str, input: &Value) -> bool {
    if tool != "Grep" {
        return false;
    }
    match input.get("glob") {
        None | Some(Value::Null) => false,
        Some(Value::String(glob)) => !glob.is_empty(),
        Some(_) => true,
    }
}
