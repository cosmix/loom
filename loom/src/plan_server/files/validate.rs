//! Plan validation for file mode: `loom plan verify`'s report mapped to the wire, and the cache
//! of every unlocked plan's validation the tree and the plan panel read.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::commands::plan::verify::report::{
    verify_text, MetadataPresence, VerifyError, VerifyReport,
};
use crate::plan::inputs::collect_inputs_with_text;
use crate::plan_server::context::PlanContext;
use crate::plan_server::wire::files::{
    MetadataView, ValidationChanged, ValidationItem, ValidationView, ValidationWarnings,
};
use crate::plan_server::wire::message::PlanMessage;
use crate::planning::project::Project;
use crate::planning::sections::sections_of;
use crate::planning::slug::{plan_home, plan_slug, plan_state, PlanState};

use super::texts::TextStore;

/// The note a plan without a metadata block gets: drafting prose first is normal (section 06).
pub const NO_METADATA_NOTE: &str = "No metadata block yet; the plan is saved as prose.";

/// Whether `rel` is a written plan in a standalone or feature home: the only files
/// saves and the cache validate.
pub fn is_plan_file(rel: &Path) -> bool {
    plan_state(rel) == Some(PlanState::Written)
}

/// Validate `text` as the content of the plan `rel` (repo-relative) under `root`, with the
/// checks `loom plan verify` runs. Every message is free of host paths.
pub fn validate_plan(root: &Path, rel: &Path, text: &str) -> ValidationView {
    let report = verify_text(Some(root), &root.join(rel), text);
    let strip = root_stripper(root);
    let warnings = warnings(&report, &strip);
    let notes = report.notes.iter().map(|note| strip(note));
    if report.metadata == MetadataPresence::Missing {
        return ValidationView {
            valid: true,
            errors: Vec::new(),
            warnings,
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
        .map(|error| error_item(&report, error, &stage_lines, &strip))
        .collect();
    ValidationView {
        valid: report.valid,
        errors,
        warnings,
        notes: notes.collect(),
        metadata: MetadataView::Present,
    }
}

/// Rewrites a message so the repository root never reaches a viewer (section 12): verify names
/// plan inputs by absolute path (`plan::inputs`, "Cannot read plan input <path>"), which the
/// `--json` output keeps. `root/` is removed and a bare `root` becomes `.`; the canonical root is
/// replaced too, longest first, since input discovery canonicalizes it.
fn root_stripper(root: &Path) -> impl Fn(&str) -> String {
    let mut roots: Vec<String> = [Some(root.to_path_buf()), root.canonicalize().ok()]
        .into_iter()
        .flatten()
        .map(|path| path.to_string_lossy().trim_end_matches('/').to_owned())
        .filter(|path| !path.is_empty())
        .collect();
    roots.sort_by_key(|path| std::cmp::Reverse(path.len()));
    roots.dedup();
    move |message: &str| {
        roots.iter().fold(message.to_owned(), |message, root| {
            message
                .replace(&format!("{root}/"), "")
                .replace(root.as_str(), ".")
        })
    }
}

/// A stage error points at the stage's `- id:` line; the YAML parse error (a report's only
/// error when YAML fails) at its line and column in the file.
fn error_item(
    report: &VerifyReport,
    error: &VerifyError,
    stage_lines: &HashMap<String, u32>,
    strip: &impl Fn(&str) -> String,
) -> ValidationItem {
    let stage_line = error
        .stage_id
        .as_ref()
        .and_then(|id| stage_lines.get(id).copied());
    let yaml = report.yaml_location.filter(|_| error.stage_id.is_none());
    ValidationItem {
        message: strip(&error.message),
        stage_id: error.stage_id.clone(),
        line: stage_line.or_else(|| yaml.map(|at| line_number(at.line))),
        column: yaml.map(|at| line_number(at.column)),
    }
}

fn warnings(report: &VerifyReport, strip: &impl Fn(&str) -> String) -> ValidationWarnings {
    let bucket = |items: &[String]| items.iter().map(|item| strip(item)).collect();
    ValidationWarnings {
        structural: bucket(&report.warnings.structural),
        knowledge: bucket(&report.warnings.knowledge),
        sandbox: bucket(&report.warnings.sandbox),
        baseline: bucket(&report.warnings.baseline),
    }
}

fn line_number(n: impl TryInto<u32>) -> u32 {
    n.try_into().unwrap_or(u32::MAX)
}

/// The knowledge tree: the plan checks read it (the "consider a knowledge-bootstrap stage"
/// warning, the strict `loom knowledge check` error), so a change under it affects every plan.
const KNOWLEDGE_DIR: &str = "doc/loom/knowledge";

/// The repo-relative files a plan's validation read, so a batch revalidates only the plans it
/// can affect.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlanInputs {
    /// Every input file `loom plan verify` discovered (present or missing).
    pub files: BTreeSet<PathBuf>,
    /// The directory a brief added under it becomes an input of: the plan's conventional
    /// bundle, or all briefs in its home when discovery failed.
    pub bundle: Option<PathBuf>,
}

impl PlanInputs {
    /// The inputs of the plan `rel` with `text`. A file that newly matches a glob or lands in a
    /// referenced directory outside the bundle is picked up at the plan's next revalidation;
    /// record directory and glob inputs here if such references become common.
    fn of(root: &Path, rel: &Path, text: &str) -> PlanInputs {
        let briefs = plan_home(rel).map(|home| home.join("briefs"));
        match collect_inputs_with_text(root, &root.join(rel), Some(text)) {
            Ok(inputs) => PlanInputs {
                files: inputs.files.into_keys().collect(),
                bundle: rel
                    .file_name()
                    .and_then(|name| name.to_str())
                    .and_then(plan_slug)
                    .and_then(|slug| briefs.as_ref().map(|base| base.join(slug))),
            },
            // The failure is already one of the view's errors (verify ran the same walk), but
            // which inputs the walk reached is unknown, and any brief may be the one that fixes
            // it: every brief counts until the next walk succeeds.
            Err(_) => PlanInputs {
                files: BTreeSet::new(),
                bundle: briefs,
            },
        }
    }

    fn touched_by(&self, path: &Path) -> bool {
        self.files.contains(path)
            || path.starts_with(KNOWLEDGE_DIR)
            || self
                .bundle
                .as_ref()
                .is_some_and(|bundle| path.starts_with(bundle))
    }
}

/// One plan's cached validation and the inputs it was computed from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedValidation {
    pub view: ValidationView,
    pub inputs: PlanInputs,
}

impl CachedValidation {
    fn of(root: &Path, rel: &Path, text: &str) -> CachedValidation {
        CachedValidation {
            view: validate_plan(root, rel, text),
            inputs: PlanInputs::of(root, rel, text),
        }
    }
}

/// Every unlocked `PLAN-*.md` file's validation by repo-relative path, in `ctx.extensions()`.
#[derive(Debug, Default)]
pub struct ValidationCache(Mutex<HashMap<PathBuf, CachedValidation>>);

impl ValidationCache {
    /// Seed the cache with already computed validation results.
    pub fn new(entries: HashMap<PathBuf, CachedValidation>) -> ValidationCache {
        ValidationCache(Mutex::new(entries))
    }

    /// The cached validation of `rel`; `None` for a locked plan or a file that is no plan.
    pub fn view(&self, rel: &Path) -> Option<ValidationView> {
        self.lock().get(rel).map(|entry| entry.view.clone())
    }

    /// The cached validation of `rel`, or, for an unlocked plan the cache lacks (the startup
    /// pass has not reached it yet), its validation of `text` computed now. That result is not
    /// stored, so a request never overwrites one the pass or a batch computed from newer text.
    pub fn view_or_validate(
        &self,
        ctx: &PlanContext,
        rel: &Path,
        text: &str,
    ) -> Option<ValidationView> {
        if let Some(view) = self.view(rel) {
            return Some(view);
        }
        let unlocked = is_plan_file(rel) && ctx.project().locked.lock_reason(rel).is_none();
        unlocked.then(|| validate_plan(ctx.root(), rel, text))
    }

    /// Each cached plan's error count.
    pub fn error_counts(&self) -> HashMap<PathBuf, u32> {
        self.lock()
            .iter()
            .map(|(rel, entry)| (rel.clone(), line_number(entry.view.errors.len())))
            .collect()
    }

    /// Follow a watcher batch that named `touched` (repo-relative): drop plans no longer listed
    /// or now locked, revalidate the plans the cache lacks (new, renamed, unlocked; every plan
    /// on the startup pass) and those whose own file or an input was touched, and broadcast
    /// `validation` for each plan whose view changed. Each result is stored as soon as it is
    /// computed; the checks run with neither the project nor the cache locked. Stops between
    /// plans once the server is stopping, so a long pass does not hold up the shutdown.
    pub fn revalidate(&self, ctx: &PlanContext, texts: &TextStore, touched: &BTreeSet<PathBuf>) {
        let plans = unlocked_plans(&ctx.project());
        for rel in self.stale(&plans, touched) {
            if ctx.is_stopping() {
                return;
            }
            let fresh = texts
                .current(&rel)
                .map(|pinned| CachedValidation::of(ctx.root(), &rel, &pinned.text));
            self.store(ctx, rel, fresh);
        }
    }

    /// Drop the entries of plans outside `plans`; return the plans to revalidate.
    fn stale(&self, plans: &BTreeSet<PathBuf>, touched: &BTreeSet<PathBuf>) -> Vec<PathBuf> {
        let mut entries = self.lock();
        entries.retain(|rel, _| plans.contains(rel));
        plans
            .iter()
            .filter(|rel| match entries.get(*rel) {
                None => true,
                Some(entry) => {
                    touched.contains(*rel)
                        || touched.iter().any(|path| entry.inputs.touched_by(path))
                }
            })
            .cloned()
            .collect()
    }

    /// Store a fresh result (`None`: the plan has no text now), broadcasting `validation` when
    /// the view changed. The message goes out under the cache lock, before the entry is
    /// visible: once a reader sees the entry, its message has been sent.
    fn store(&self, ctx: &PlanContext, rel: PathBuf, fresh: Option<CachedValidation>) {
        let mut entries = self.lock();
        let Some(fresh) = fresh else {
            entries.remove(&rel);
            return;
        };
        if entries.get(&rel).map(|old| &old.view) != Some(&fresh.view) {
            let path = rel.to_string_lossy().into_owned();
            let validation = fresh.view.clone();
            let message = PlanMessage::Validation(ValidationChanged { path, validation });
            ctx.hub().broadcast(&message);
        }
        entries.insert(rel, fresh);
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<PathBuf, CachedValidation>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Every unlocked plan file the project lists.
fn unlocked_plans(project: &Project) -> BTreeSet<PathBuf> {
    project
        .tree
        .docs
        .iter()
        .map(|doc| &doc.path)
        .filter(|rel| is_plan_file(rel) && project.locked.lock_reason(rel).is_none())
        .cloned()
        .collect()
}
