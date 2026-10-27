//! Plan validation for file mode: `loom plan verify`'s report mapped to the wire, and the cache
//! of every unlocked plan's validation the tree and the plan panel read.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use crate::commands::plan::verify::report::{
    verify_text, MetadataPresence, VerifyError, VerifyReport,
};
use crate::plan_server::context::PlanContext;
use crate::plan_server::wire::files::{
    MetadataView, ValidationChanged, ValidationItem, ValidationView, ValidationWarnings,
};
use crate::plan_server::wire::message::PlanMessage;
use crate::planning::project::Project;
use crate::planning::sections::sections_of;

use super::texts::{PinnedText, TextStore};

/// The note a plan without a metadata block gets: drafting prose first is normal (section 06).
pub const NO_METADATA_NOTE: &str = "No metadata block yet; the plan is saved as prose.";

/// Every unlocked `PLAN-*.md` file's validation by repo-relative path, in `ctx.extensions()`.
pub type ValidationCache = Mutex<HashMap<PathBuf, ValidationView>>;

/// Whether `rel` (repo-relative) is a written plan file, `doc/plans/PLAN-*.md`: the only
/// files saves and the cache validate.
pub fn is_plan_file(rel: &Path) -> bool {
    let name = rel.file_name().and_then(|name| name.to_str()).unwrap_or("");
    rel.parent() == Some(Path::new("doc/plans"))
        && name.starts_with("PLAN-")
        && name.ends_with(".md")
}

/// Validate `text` as the content of the plan `rel` (repo-relative) under `root`, with the
/// checks `loom plan verify` runs.
pub fn validate_plan(root: &Path, rel: &Path, text: &str) -> ValidationView {
    let report = verify_text(Some(root), &root.join(rel), text);
    if report.metadata == MetadataPresence::Missing {
        return ValidationView {
            valid: true,
            errors: Vec::new(),
            warnings: warnings(&report),
            notes: vec![NO_METADATA_NOTE.to_owned()],
            metadata: MetadataView::Missing,
        };
    }
    // Reversed, so the first entry of a repeated stage id wins.
    let stage_lines: HashMap<String, u32> = sections_of(text)
        .stages
        .into_iter()
        .rev()
        .map(|(id, span)| (id, line_number(span.start_line.saturating_add(1))))
        .collect();
    let errors = report
        .errors
        .iter()
        .map(|error| error_item(&report, error, &stage_lines))
        .collect();
    ValidationView {
        valid: report.valid,
        errors,
        warnings: warnings(&report),
        notes: report.notes.clone(),
        metadata: MetadataView::Present,
    }
}

/// A stage error points at the stage's `- id:` line; the YAML parse error (a report's only
/// error when YAML fails) at its line and column in the file.
fn error_item(
    report: &VerifyReport,
    error: &VerifyError,
    stage_lines: &HashMap<String, u32>,
) -> ValidationItem {
    let stage_line = error
        .stage_id
        .as_ref()
        .and_then(|id| stage_lines.get(id).copied());
    let yaml = report.yaml_location.filter(|_| error.stage_id.is_none());
    ValidationItem {
        message: error.message.clone(),
        stage_id: error.stage_id.clone(),
        line: stage_line.or_else(|| yaml.map(|at| line_number(at.line))),
        column: yaml.map(|at| line_number(at.column)),
    }
}

fn warnings(report: &VerifyReport) -> ValidationWarnings {
    ValidationWarnings {
        structural: report.warnings.structural.clone(),
        knowledge: report.warnings.knowledge.clone(),
        sandbox: report.warnings.sandbox.clone(),
        baseline: report.warnings.baseline.clone(),
    }
}

fn line_number(n: impl TryInto<u32>) -> u32 {
    n.try_into().unwrap_or(u32::MAX)
}

/// The stored text of every unlocked plan file the project lists.
fn plan_texts(project: &Project, texts: &TextStore) -> Vec<(PathBuf, PinnedText)> {
    project
        .tree
        .docs
        .iter()
        .map(|doc| &doc.path)
        .filter(|rel| is_plan_file(rel) && project.locked.lock_reason(rel).is_none())
        .filter_map(|rel| Some((rel.clone(), texts.current(rel)?)))
        .collect()
}

/// The validation of every unlocked plan file. The project lock is released before the
/// checks run, so a slow validation never holds the watcher's refresh back.
pub fn validate_all(ctx: &PlanContext, texts: &TextStore) -> HashMap<PathBuf, ValidationView> {
    let plans = plan_texts(&ctx.project(), texts);
    plans
        .into_iter()
        .map(|(rel, text)| {
            let view = validate_plan(ctx.root(), &rel, &text.text);
            (rel, view)
        })
        .collect()
}

/// Revalidate every unlocked plan (a batch can change a plan's inputs as well as the plan),
/// replace the cache and broadcast `validation` for each plan whose view changed.
pub fn revalidate(ctx: &PlanContext, cache: &ValidationCache, texts: &TextStore) {
    let fresh = validate_all(ctx, texts);
    let changed: Vec<(PathBuf, ValidationView)> = {
        let mut cache = cache.lock().unwrap_or_else(PoisonError::into_inner);
        let changed = fresh
            .iter()
            .filter(|(rel, view)| cache.get(*rel) != Some(*view))
            .map(|(rel, view)| (rel.clone(), view.clone()))
            .collect();
        *cache = fresh;
        changed
    };
    for (rel, validation) in changed {
        let path = rel.to_string_lossy().into_owned();
        let message = PlanMessage::Validation(ValidationChanged { path, validation });
        ctx.hub().broadcast(&message);
    }
}
