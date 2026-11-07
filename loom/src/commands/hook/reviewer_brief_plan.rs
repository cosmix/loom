//! Plan-wide context for an integration-verify reviewer brief: the files the
//! plan's other stages changed, the plan's brief bundle and the stages that
//! declare no consumer yet.

use anyhow::{bail, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::git::merge::control_paths::{accepted_change_base, changed_paths, AcceptedBase};
use crate::models::dispute::truncate_to_byte_limit;
use crate::models::stage::{Stage, StageType};
use crate::planning::slug::{plan_home, plan_slug};
use crate::verify::review::report::single_line;

/// Files listed in the brief before the remainder is summarised.
const MAX_LISTED_FILES: usize = 200;
/// Stages listed under `consumer_deferred`.
const MAX_DEFERRED_STAGES: usize = 50;
/// Byte budgets of the lists, so the section stays inside the brief's 16 KiB
/// cap, which drops events first. Each file costs its path plus 5 bytes of list
/// markup, so 10 KiB holds `MAX_LISTED_FILES` paths of up to about 46 bytes
/// (this repository's typical path is 44); longer ones are cut at the budget and
/// the rest is summarised by directory.
const MAX_FILES_BYTES: usize = 10 * 1024;
const MAX_DEFERRED_BYTES: usize = 2 * 1024;
const MAX_OMITTED_DIRS_BYTES: usize = 1024;
const MAX_PATH_BYTES: usize = 300;
const MAX_REASON_BYTES: usize = 160;

/// Union of the files the plan's other stages brought to the target.
pub(super) struct PlanChange {
    /// Sorted, deduplicated repository-relative paths.
    pub files: Vec<String>,
    /// Stages whose merge into the target was not found; they add no file.
    pub missing: Vec<String>,
}

/// Union, over the plan's other completed stages, of the files each stage's
/// merge brought to `target`. The plan records no base commit, so each stage's
/// own change is used: `merge-base <merge>^1 <completed>` to `<completed>`. A
/// commit made on the target outside any stage never appears. Knowledge stages
/// commit on the target without a merge and are skipped. A stage whose merge
/// cannot be located or diffed (an unmerged commit, one that reached the
/// target without a merge commit, an unreadable object) is listed in `missing`.
pub(super) fn plan_changed_files(
    worktree: &Path,
    target: &str,
    stages: &[Stage],
    this: &str,
) -> Result<PlanChange> {
    let commits: Vec<(&str, &str)> = stages
        .iter()
        .filter(|stage| stage.id != this && stage.stage_type != StageType::Knowledge)
        .filter_map(|stage| Some((stage.id.as_str(), stage.completed_commit.as_deref()?)))
        .collect();
    if commits.is_empty() {
        bail!("no stage of this plan has completed");
    }
    let mut files = BTreeSet::new();
    let mut missing = Vec::new();
    for (id, completed) in commits {
        match stage_change(worktree, target, completed) {
            Some(paths) => files.extend(paths),
            None => missing.push(id.to_string()),
        }
    }
    missing.sort();
    Ok(PlanChange {
        files: files.into_iter().collect(),
        missing,
    })
}

fn stage_change(worktree: &Path, target: &str, completed: &str) -> Option<Vec<String>> {
    match accepted_change_base(worktree, completed, target).ok()? {
        AcceptedBase::Merged { base, .. } => changed_paths(worktree, &base, completed).ok(),
        AcceptedBase::Unmerged { .. } => None,
    }
}

/// The plan's `briefs/<slug>` bundle as a repository-relative path with a
/// trailing slash, when the stage worktree holds that directory.
pub(super) fn plan_briefs_dir(work_dir: &Path, worktree: &Path) -> Option<String> {
    let config = crate::fs::work_dir::load_config(work_dir).ok()??;
    let source = config.source_path()?;
    let slug = plan_slug(source.file_name()?.to_str()?)?;
    let relative = plan_home(&source)?.join("briefs").join(slug);
    worktree
        .join(&relative)
        .is_dir()
        .then(|| format!("{}/", single_line(&relative.to_string_lossy())))
}

/// Everything the integration-verify section of the brief reports.
pub(super) struct IntegrationContext {
    pub briefs_dir: Option<String>,
    /// The plan-wide change, or why it is unavailable.
    pub changed: std::result::Result<PlanChange, String>,
    /// `(stage id, reason)` of every stage that declares no consumer yet.
    pub deferred: Vec<(String, String)>,
}

impl IntegrationContext {
    /// `stages` is the plan's stage listing; a failed listing makes the change
    /// unavailable with that error, never an empty plan.
    pub(super) fn collect(
        work_dir: &Path,
        worktree: &Path,
        target: &str,
        stages: Result<Vec<Stage>>,
        this: &str,
    ) -> Self {
        let briefs_dir = plan_briefs_dir(work_dir, worktree);
        let stages = match stages {
            Ok(stages) => stages,
            Err(error) => {
                return Self {
                    briefs_dir,
                    changed: Err(clean(
                        &format!("stage listing failed: {error:#}"),
                        MAX_REASON_BYTES,
                    )),
                    deferred: Vec::new(),
                };
            }
        };
        let mut deferred: Vec<(String, String)> = stages
            .iter()
            .filter_map(|stage| Some((stage.id.clone(), stage.consumer_deferred.clone()?)))
            .collect();
        deferred.sort();
        Self {
            briefs_dir,
            changed: plan_changed_files(worktree, target, &stages, this)
                .map_err(|error| clean(&format!("{error:#}"), MAX_REASON_BYTES)),
            deferred,
        }
    }
}

/// `text` on one line and cut to `limit` bytes.
pub(super) fn clean(text: &str, limit: usize) -> String {
    truncate_to_byte_limit(&single_line(text), limit)
}

/// The `## Integration-verify context` section, lines ending in `\n`.
pub(super) fn render_integration(context: &IntegrationContext) -> String {
    let mut out = String::from(
        "## Integration-verify context (computed by loom)\n\
         Judge the plan-wide change against what the plan and its briefs ask for.\n",
    );
    if let Some(dir) = &context.briefs_dir {
        out.push_str(&format!(
            "- Plan briefs: `{dir}` (read common.md and each stage brief there).\n"
        ));
    }
    match &context.changed {
        Ok(change) => render_change(&mut out, change),
        Err(reason) => out.push_str(&format!(
            "- Files the plan changed: unavailable ({reason})\n"
        )),
    }
    if !context.deferred.is_empty() {
        out.push_str("- Stages that declare no consumer yet (consumer_deferred):\n");
        let lines = context.deferred.iter().map(|(id, reason)| {
            format!(
                "{}: {}",
                clean(id, MAX_PATH_BYTES),
                clean(reason, MAX_REASON_BYTES)
            )
        });
        push_list(
            &mut out,
            lines,
            context.deferred.len(),
            MAX_DEFERRED_STAGES,
            MAX_DEFERRED_BYTES,
        );
    }
    out
}

fn render_change(out: &mut String, change: &PlanChange) {
    out.push_str(&format!(
        "- Files the plan changed ({}):\n",
        change.files.len()
    ));
    let lines = change.files.iter().map(|path| clean(path, MAX_PATH_BYTES));
    let shown = push_list(
        out,
        lines,
        change.files.len(),
        MAX_LISTED_FILES,
        MAX_FILES_BYTES,
    );
    if let Some(summary) = omitted_by_directory(&change.files[shown..]) {
        out.push_str(&format!("  - Not listed, by directory: {summary}\n"));
    }
    if !change.missing.is_empty() {
        out.push_str(&format!(
            "- Stages not found in the target's history: {}\n",
            inline_list(&change.missing)
        ));
    }
}

/// Ids joined by `, ` up to `MAX_DEFERRED_STAGES` ids and `MAX_DEFERRED_BYTES`
/// bytes, then `... <r> more` for the rest.
fn inline_list(ids: &[String]) -> String {
    let mut shown: Vec<String> = Vec::new();
    let mut used = 0;
    for id in ids.iter().take(MAX_DEFERRED_STAGES) {
        let id = clean(id, MAX_PATH_BYTES);
        used += id.len() + 2;
        if used > MAX_DEFERRED_BYTES {
            break;
        }
        shown.push(id);
    }
    if ids.len() > shown.len() {
        shown.push(format!("... {} more", ids.len() - shown.len()));
    }
    shown.join(", ")
}

/// `dir (count)` for the directories (first two path components) holding the
/// `omitted` paths, largest first and cut at `MAX_OMITTED_DIRS_BYTES`, so a
/// reviewer can Glob what the list left out. `None` when nothing is omitted.
fn omitted_by_directory(omitted: &[String]) -> Option<String> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for path in omitted {
        let mut parts: Vec<&str> = path.split('/').collect();
        parts.pop();
        parts.truncate(2);
        let dir = if parts.is_empty() {
            "(repository root)".to_string()
        } else {
            parts.join("/")
        };
        *counts.entry(clean(&dir, MAX_PATH_BYTES)).or_default() += 1;
    }
    let mut ranked: Vec<(String, usize)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let total = ranked.len();
    let mut shown: Vec<String> = Vec::new();
    let mut used = 0;
    for (dir, count) in &ranked {
        let entry = format!("{dir} ({count})");
        used += entry.len() + 2;
        if used > MAX_OMITTED_DIRS_BYTES {
            break;
        }
        shown.push(entry);
    }
    if shown.is_empty() {
        return None;
    }
    if total > shown.len() {
        shown.push(format!("... {} more directories", total - shown.len()));
    }
    Some(shown.join(", "))
}

/// Appends `  - <line>` per item up to `max_items` and `max_bytes`, then
/// `  - ... <r> more` for the `total - shown` left over. Returns the number of
/// items shown.
fn push_list(
    out: &mut String,
    lines: impl Iterator<Item = String>,
    total: usize,
    max_items: usize,
    max_bytes: usize,
) -> usize {
    let mut shown = 0;
    let mut used = 0;
    for line in lines.take(max_items) {
        used += line.len() + 5;
        if used > max_bytes {
            break;
        }
        out.push_str(&format!("  - {line}\n"));
        shown += 1;
    }
    if total > shown {
        out.push_str(&format!("  - ... {} more\n", total - shown));
    }
    shown
}
