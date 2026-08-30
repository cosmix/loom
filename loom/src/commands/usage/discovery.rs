//! Finds transcript files without inferring intent from their contents. A
//! Claude transcript has no plan or stage field, so plan filtering is only
//! permitted when loom's active configuration identifies the requested plan;
//! otherwise guessing would silently mix unrelated work into a usage report.

use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};

use crate::commands::subagents::resolve::project_slug;

#[derive(Debug, Clone)]
pub struct DiscoveredFile {
    pub path: std::path::PathBuf,
    pub project_slug: String,
    pub scope: super::transcript::Scope,
    pub session_id: String,
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DiscoveryOptions {
    pub since: chrono::DateTime<chrono::Utc>,
    pub project: Option<std::path::PathBuf>,
    pub all: bool,
    pub stage: Option<String>,
    pub plan: Option<String>,
}

/// Parse `--since`: a duration (`7d`, `24h`, `30m`) or an ISO date
/// (`2026-08-01`, interpreted as that date's midnight UTC).
pub fn parse_since(spec: &str) -> anyhow::Result<chrono::DateTime<chrono::Utc>> {
    if let Ok(date) = NaiveDate::parse_from_str(spec, "%Y-%m-%d") {
        let midnight = date
            .and_hms_opt(0, 0, 0)
            .context("ISO date has no midnight")?;
        return Ok(Utc.from_utc_datetime(&midnight));
    }
    let duration = duration_spec(spec)?;
    Utc::now()
        .checked_sub_signed(duration)
        .context("--since duration is too large")
}

/// Every transcript file to read, sorted by path for deterministic output.
/// A missing `~/.claude/projects` yields an empty vec, not an error.
pub fn discover(options: &DiscoveryOptions) -> anyhow::Result<Vec<DiscoveredFile>> {
    let Some(projects_root) = claude_projects_root() else {
        return Ok(Vec::new());
    };
    if !projects_root.is_dir() {
        return Ok(Vec::new());
    }
    let mut files = collect_projects(&projects_root, options)?;
    if let Some(stage) = options.stage.as_deref() {
        files = filter_stage(files, stage);
    }
    if let Some(plan) = options.plan.as_deref() {
        files = filter_plan(files, plan)?;
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

fn duration_spec(spec: &str) -> Result<Duration> {
    let Some(unit) = spec.chars().next_back() else {
        bail!("Invalid --since value: {spec}")
    };
    let number = spec
        .strip_suffix(unit)
        .unwrap_or_default()
        .parse::<i64>()
        .with_context(|| format!("Invalid --since value: {spec}"))?;
    if number < 0 {
        bail!("Invalid --since value: {spec}")
    }
    let hours = match unit {
        'm' => return Duration::try_minutes(number).context("--since duration is too large"),
        'h' => number,
        'd' => number
            .checked_mul(24)
            .context("--since duration is too large")?,
        _ => bail!("Invalid --since value: {spec}"),
    };
    Duration::try_hours(hours).context("--since duration is too large")
}

fn claude_projects_root() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".claude/projects"))
}

/// Reads every selected project directory, warning on stderr and skipping
/// any that can't be read rather than failing the whole report - the same
/// posture `parse_all` takes toward an unparseable transcript. The one
/// exception is a `--project <path>` the caller named explicitly: that one
/// fails hard, since a user who pointed at a specific directory deserves to
/// be told it's unreadable rather than see the report silently drop to
/// empty.
fn collect_projects(root: &Path, options: &DiscoveryOptions) -> Result<Vec<DiscoveredFile>> {
    let slugs = selected_slugs(root, options)?;
    let strict = options.project.is_some();
    let mut files = Vec::new();
    for slug in slugs {
        let directory = root.join(&slug);
        match project_files(&directory, slug, options.since) {
            Ok(found) => files.extend(found),
            Err(error) if strict => return Err(error),
            Err(error) => eprintln!("loom usage: skipping {}: {error:#}", directory.display()),
        }
    }
    Ok(files)
}

fn selected_slugs(root: &Path, options: &DiscoveryOptions) -> Result<Vec<String>> {
    if options.all {
        return all_slugs(root);
    }
    if let Some(project) = options.project.as_deref() {
        return Ok(vec![project_slug(&absolute(project)?)]);
    }
    let repo = repository_root()?;
    let mut paths = vec![repo.clone()];
    let worktrees = repo.join(".worktrees");
    if let Ok(entries) = fs::read_dir(worktrees) {
        paths.extend(
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_dir()),
        );
    }
    Ok(paths.iter().map(|path| project_slug(path)).collect())
}

fn all_slugs(root: &Path) -> Result<Vec<String>> {
    Ok(fs::read_dir(root)
        .with_context(|| format!("Failed to read {}", root.display()))?
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect())
}

fn absolute(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return path
            .canonicalize()
            .with_context(|| format!("Failed to resolve {}", path.display()));
    }
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(env::current_dir()?.join(path))
    }
}

fn repository_root() -> Result<PathBuf> {
    let current = env::current_dir()?;
    let Some(ancestor) = current.ancestors().find(|path| path.join(".git").exists()) else {
        return Ok(current);
    };
    let git_path = ancestor.join(".git");
    if git_path.is_file() {
        if let Some(root) = worktree_repository_root(&git_path) {
            return Ok(root);
        }
    }
    Ok(ancestor.to_path_buf())
}

/// Inside a git worktree (loom's own `.worktrees/<stage>` included), `.git`
/// is a FILE, not a directory, and still satisfies `exists()` — so without
/// this, `repository_root` would stop at the worktree and report it as the
/// whole repository, silently narrowing `loom usage` to one worktree instead
/// of the repository plus all of its worktrees.
///
/// The file's single line is `gitdir: <repo>/.git/worktrees/<name>`; walk
/// back up three components (`<name>`, `worktrees`, `.git`) to recover
/// `<repo>`. `None` when the file can't be read or doesn't match that shape,
/// so the caller falls back to treating the worktree itself as the root.
fn worktree_repository_root(git_file: &Path) -> Option<PathBuf> {
    let contents = fs::read_to_string(git_file).ok()?;
    let gitdir = contents.trim().strip_prefix("gitdir:")?.trim();
    Path::new(gitdir)
        .parent()?
        .parent()?
        .parent()
        .map(Path::to_path_buf)
}

fn project_files(
    directory: &Path,
    slug: String,
    since: DateTime<Utc>,
) -> Result<Vec<DiscoveredFile>> {
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(directory)
        .with_context(|| format!("Failed to read {}", directory.display()))?
        .flatten()
    {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|value| value.to_str()) == Some("jsonl") {
            let session_id = path
                .file_stem()
                .and_then(|value| value.to_str())
                .map(str::to_string);
            if let Some(session_id) = session_id {
                add_main(&mut files, path, &slug, &session_id, since);
            }
        } else if path.is_dir() {
            add_subagents(&mut files, &path, &slug, since);
        }
    }
    Ok(files)
}

fn add_main(
    files: &mut Vec<DiscoveredFile>,
    path: PathBuf,
    slug: &str,
    session_id: &str,
    since: DateTime<Utc>,
) {
    if is_recent(&path, since) {
        files.push(DiscoveredFile {
            path,
            project_slug: slug.to_owned(),
            scope: super::transcript::Scope::Main,
            session_id: session_id.to_owned(),
            agent_id: None,
        });
    }
}

fn add_subagents(
    files: &mut Vec<DiscoveredFile>,
    session: &Path,
    slug: &str,
    since: DateTime<Utc>,
) {
    let Some(session_id) = session.file_name().and_then(|value| value.to_str()) else {
        return;
    };
    for path in crate::commands::subagents::resolve::list_agent_files(&session.join("subagents")) {
        if !is_recent(&path, since) {
            continue;
        }
        let agent_id = Some(crate::commands::subagents::resolve::agent_id_from_path(
            &path,
        ));
        files.push(DiscoveredFile {
            path,
            project_slug: slug.to_owned(),
            scope: super::transcript::Scope::Subagent,
            session_id: session_id.to_owned(),
            agent_id,
        });
    }
}

fn is_recent(path: &Path, since: DateTime<Utc>) -> bool {
    let cutoff: SystemTime = since.into();
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .map(|modified| modified >= cutoff)
        .unwrap_or(false)
}

fn filter_stage(files: Vec<DiscoveredFile>, stage: &str) -> Vec<DiscoveredFile> {
    let suffix = format!("-worktrees-{stage}");
    files
        .into_iter()
        .filter(|file| file.project_slug.ends_with(&suffix))
        .collect()
}

fn filter_plan(files: Vec<DiscoveredFile>, plan: &str) -> Result<Vec<DiscoveredFile>> {
    let Some(stages) = plan_stage_ids(plan)? else {
        return Ok(Vec::new());
    };
    Ok(files
        .into_iter()
        .filter(|file| {
            stages
                .iter()
                .any(|stage| file.project_slug.ends_with(&format!("-worktrees-{stage}")))
        })
        .collect())
}

fn plan_stage_ids(plan: &str) -> Result<Option<HashSet<String>>> {
    let work_dir = match crate::commands::common::find_work_dir() {
        Ok(work_dir) => work_dir,
        Err(error) => {
            eprintln!("plan could not be resolved: no .work directory ({error})");
            return Ok(None);
        }
    };
    let Some(config) = crate::fs::work_dir::load_config(&work_dir)? else {
        eprintln!("plan could not be resolved: .work/config.toml is missing");
        return Ok(None);
    };
    let Some(source) = config.source_path() else {
        eprintln!("plan could not be resolved: config has no plan source path");
        return Ok(None);
    };
    if normalize_plan(&source) != normalize_plan(Path::new(plan)) {
        eprintln!("plan could not be resolved: requested plan does not match the active plan");
        return Ok(None);
    }
    stage_ids(&work_dir)
}

/// Strip the status prefix (`IN_PROGRESS-`/`DONE-`) FIRST, then strip
/// `PLAN-` from what remains - the two prefixes are sequential, not
/// alternatives, so `IN_PROGRESS-PLAN-foo` only normalizes to `foo` when
/// both strips run in order.
fn normalize_plan(path: &Path) -> String {
    let value = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let without_status = ["IN_PROGRESS-", "DONE-"]
        .iter()
        .find_map(|prefix| value.strip_prefix(prefix))
        .unwrap_or(value);
    without_status
        .strip_prefix("PLAN-")
        .unwrap_or(without_status)
        .to_ascii_lowercase()
}

fn stage_ids(work_dir: &Path) -> Result<Option<HashSet<String>>> {
    let stages = work_dir.join("stages");
    let entries = match fs::read_dir(&stages) {
        Ok(entries) => entries,
        Err(error) => {
            eprintln!(
                "plan could not be resolved: cannot read {} ({error})",
                stages.display()
            );
            return Ok(None);
        }
    };
    Ok(Some(
        entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                (path.extension().and_then(|extension| extension.to_str()) == Some("md"))
                    .then(|| {
                        path.file_stem()
                            .and_then(|stem| stem.to_str())
                            .map(str::to_owned)
                    })
                    .flatten()
            })
            .collect(),
    ))
}

#[cfg(test)]
#[path = "discovery_tests.rs"]
mod tests;
