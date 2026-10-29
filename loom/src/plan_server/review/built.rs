//! Built notes: sections changed after the plan implementing them was built.
//!
//! A done plan's build date is the `**Generated:**` stamp of its run review (the line
//! `fs::plan_review::generate` writes); the section's change date comes from git. A running
//! plan is reported as running; written and retired plans give nothing.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, NaiveDateTime, SecondsFormat, Utc};

use super::compare::is_commit_id;
use crate::git::runner::run_git_checked;
use crate::planning::discover::read_doc;
use crate::planning::kinds::DocKind;
use crate::planning::paths::check_relpath_syntax;
use crate::planning::project::Project;
use crate::planning::slug::{plan_home, PlanState};

/// The marker before the build stamp in a run review.
const GENERATED_MARK: &str = "**Generated:** ";
/// The stamp format `fs::plan_review` renders.
const GENERATED_FORMAT: &str = "%Y-%m-%d %H:%M UTC";

/// A note on a product section from one plan implementing it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuiltNote {
    /// The section changed after the plan was built.
    ChangedAfter {
        /// The plan's slug.
        slug: String,
        /// The build date, RFC 3339 in UTC.
        built: String,
        /// The section's last commit before the build, when it has one.
        base_commit: Option<String>,
    },
    /// The plan is running now.
    Running {
        /// The plan's slug.
        slug: String,
    },
}

/// The build date from the first line holding `**Generated:** `; `None` when that line's
/// stamp does not parse.
pub fn parse_generated(review: &str) -> Option<DateTime<Utc>> {
    let line = review.lines().find(|line| line.contains(GENERATED_MARK))?;
    let (_, stamp) = line.split_once(GENERATED_MARK)?;
    NaiveDateTime::parse_from_str(stamp.trim(), GENERATED_FORMAT)
        .ok()
        .map(|at| at.and_utc())
}

/// What `built_notes` reads from the `Project`, owned so the project guard can be dropped
/// before any git subprocess runs.
#[derive(Debug)]
pub(crate) struct BuiltInputs {
    root: PathBuf,
    /// The repository-relative section path (`doc/...`, as `check_relpath_syntax` returns it);
    /// empty when there are no plans.
    rel: String,
    plans: Vec<PlanInput>,
}

#[derive(Debug)]
enum PlanInput {
    Running {
        slug: String,
    },
    /// A done plan with the document paths of its run reviews.
    Done {
        slug: String,
        reviews: Vec<PathBuf>,
    },
}

/// Notes for a section DocId from every plan implementing it in the same feature.
/// Unknown ids and documents outside feature product sections give no notes.
pub fn built_notes(project: &Project, section: &str) -> Result<Vec<BuiltNote>> {
    built_from(built_inputs(project, section)?)
}

/// The part of `built_notes` that reads `project`: no git, no file reads.
pub(crate) fn built_inputs(project: &Project, section: &str) -> Result<BuiltInputs> {
    let root = project.root.clone();
    check_relpath_syntax(section).map_err(|e| anyhow!("invalid section id: {e}"))?;
    let none = |root| BuiltInputs {
        root,
        rel: String::new(),
        plans: Vec::new(),
    };
    let Some(doc) = project.tree.by_id(section) else {
        return Ok(none(root));
    };
    let DocKind::ProductSection {
        number,
        feature: Some(feature),
    } = &doc.kind
    else {
        return Ok(none(root));
    };
    let Some(slugs) = project
        .feature(feature)
        .and_then(|feature| feature.implemented_by.get(number))
    else {
        return Ok(none(root));
    };
    let rel = doc
        .path
        .to_str()
        .context("section path is not UTF-8")?
        .to_owned();
    let plans = slugs
        .iter()
        .filter_map(
            |slug| match project.tree.plan(slug).map(|plan| plan.state) {
                Some(PlanState::Running) => Some(PlanInput::Running { slug: slug.clone() }),
                Some(PlanState::Done) => Some(PlanInput::Done {
                    slug: slug.clone(),
                    reviews: review_paths(project, slug),
                }),
                Some(PlanState::Written | PlanState::Retired) | None => None,
            },
        )
        .collect();
    Ok(BuiltInputs { root, rel, plans })
}

/// The git and file reads of `built_notes`, on owned inputs.
pub(crate) fn built_from(inputs: BuiltInputs) -> Result<Vec<BuiltNote>> {
    let BuiltInputs { root, rel, plans } = inputs;
    let mut changed: Option<Option<DateTime<Utc>>> = None;
    let mut notes = Vec::new();
    for plan in plans {
        match plan {
            PlanInput::Running { slug } => notes.push(BuiltNote::Running { slug }),
            PlanInput::Done { slug, reviews } => {
                let Some(built) = latest_build(&root, &reviews)? else {
                    continue;
                };
                let changed_at = match changed {
                    Some(at) => at,
                    None => {
                        let at = section_changed(&root, &rel)?;
                        changed = Some(at);
                        at
                    }
                };
                if changed_at.is_some_and(|at| at > built) {
                    notes.push(changed_after(&root, &rel, &slug, built)?);
                }
            }
        }
    }
    Ok(notes)
}

/// Every run review of `slug`, wherever discovery found it and however `generate` spelled
/// its name.
fn review_paths(project: &Project, slug: &str) -> Vec<PathBuf> {
    let Some(home) = project
        .tree
        .plan(slug)
        .and_then(|plan| plan_home(&plan.path))
    else {
        return Vec::new();
    };
    let archive = home.join("archive");
    project
        .tree
        .docs
        .iter()
        .filter(|doc| matches!(&doc.kind, DocKind::RunReview { slug: of } if of == slug))
        .filter(|doc| {
            doc.path.parent() == Some(home.as_path())
                || doc.path.parent() == Some(archive.as_path())
        })
        .map(|doc| doc.path.clone())
        .collect()
}

/// The newest `**Generated:**` stamp among the run reviews.
fn latest_build(root: &Path, reviews: &[PathBuf]) -> Result<Option<DateTime<Utc>>> {
    let mut newest = None;
    for path in reviews {
        let text = read_doc(root, path)
            .with_context(|| format!("reading run review {}", path.display()))?;
        newest = newest.max(parse_generated(&text));
    }
    Ok(newest)
}

/// When `rel` last changed: now when the working tree differs from HEAD, else its last
/// commit date; `None` when it has neither.
fn section_changed(root: &Path, rel: &str) -> Result<Option<DateTime<Utc>>> {
    if !run_git_checked(&["status", "--porcelain", "--", rel], root)?.is_empty() {
        return Ok(Some(Utc::now()));
    }
    let date = run_git_checked(&["log", "-1", "--format=%cI", "--", rel], root)?;
    if date.is_empty() {
        return Ok(None);
    }
    let at = DateTime::parse_from_rfc3339(&date)
        .with_context(|| format!("git log gave an unparseable date {date:?} for {rel}"))?;
    Ok(Some(at.with_timezone(&Utc)))
}

/// The `ChangedAfter` note for `slug`, with the section's last commit before the build.
fn changed_after(root: &Path, rel: &str, slug: &str, built: DateTime<Utc>) -> Result<BuiltNote> {
    let before = format!("--before={}", built.to_rfc3339());
    let base = run_git_checked(&["log", "-1", "--format=%H", &before, "--", rel], root)?;
    Ok(BuiltNote::ChangedAfter {
        slug: slug.to_owned(),
        built: built.to_rfc3339_opts(SecondsFormat::Secs, true),
        base_commit: is_commit_id(&base).then_some(base),
    })
}
