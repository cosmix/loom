//! A freeze-refusal accept checked against the plan the apply amends.
//!
//! The apply makes one `plan::apply_amendment` call per entry, in array
//! order, and each call runs `plan::schema::validate` over the whole plan as
//! that single entry leaves it ("Amendment would produce an invalid plan"):
//! a duplicate or malformed contract id, a contract `file` or `cwd` outside
//! the worktree, a standard stage left without contracts. So an accept whose
//! entries are valid only together (delete the sole contract, then insert its
//! successor) fails at the first entry with the earlier ones already written,
//! and the retries escalate the stage to a person. Recording therefore
//! replays the entries over the plan's stage definition and runs the same
//! `validate` after each one, naming the entry the apply would stop at, and
//! then refuses a blocking `plan verify` lint the final plan introduces (a
//! contract runner no adapter answers to). Every problem returns to the judge.
//!
//! The lints run without a repository root: the worktree checks the
//! recording needs (harness over production source, the derived scaffold)
//! are made by the parent module against the stage's worktree. Revisit if a
//! blocking lint that needs the repository must hold for an amended stage.

use std::collections::HashSet;
use std::path::Path;

use crate::orchestrator::adjudication::apply_accept::field_name;
use crate::plan::amendment::AmendmentPatch;
use crate::plan::schema::{split_lint_findings, v2_lints, LoomMetadata, StageDefinition};
use crate::stage_service::contracts::Site;

use super::{FreezePatch, Replay};

/// Refuse `patches` when, replayed over stage `stage_id` of the plan, any
/// entry leaves a plan the apply would reject, or the final plan adds a
/// blocking lint error.
pub(super) fn check(
    work_dir: &Path,
    stage_id: &str,
    patches: &[FreezePatch],
    site: &Site,
) -> Result<(), String> {
    let plan = load(work_dir)?;
    let mut patched = plan.clone();
    let position = patched
        .loom
        .stages
        .iter()
        .position(|stage| stage.id == stage_id)
        .ok_or_else(|| format!("stage '{stage_id}' is not in the plan, so no amendment applies"))?;
    let definition = &patched.loom.stages[position];
    let mut replay = Replay::new(
        definition.contracts.clone(),
        definition.harness.clone(),
        definition.acceptance.clone(),
    );
    for (k, entry) in patches.iter().enumerate() {
        replay.apply_nth(k, entry, site)?;
        copy_fields(&replay, &mut patched.loom.stages[position]);
        if let Err(errors) = crate::plan::schema::validate(&patched) {
            return Err(invalid_after(
                k,
                entry,
                errors.iter().map(ToString::to_string).collect(),
            ));
        }
    }
    let before: HashSet<String> = lint_errors(&plan).into_iter().collect();
    let added: Vec<String> = lint_errors(&patched)
        .into_iter()
        .filter(|error| !before.contains(error))
        .collect();
    if added.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "the patches leave a plan `plan verify` would refuse: {}",
            added.join("; ")
        ))
    }
}

/// The replayed fields written onto the plan's stage definition, as the
/// apply's patch leaves them.
fn copy_fields(replay: &Replay, definition: &mut StageDefinition) {
    definition.contracts = replay.contracts.clone();
    definition.harness = replay.harness.clone();
    definition.acceptance = replay.acceptance.clone();
}

/// The plan the apply amends, resolved as the apply resolves it.
fn load(work_dir: &Path) -> Result<LoomMetadata, String> {
    let path = super::super::resolve_plan_path(work_dir).ok_or_else(|| {
        "the plan's source_path cannot be resolved, so no amendment can be checked".to_string()
    })?;
    crate::plan::parse_plan(&path)
        .map(|plan| plan.metadata)
        .map_err(|e| {
            format!(
                "the plan {} cannot be read, so no amendment can be checked: {e:#}",
                path.display()
            )
        })
}

/// The `plan verify` lint findings that are errors for this plan's version.
fn lint_errors(metadata: &LoomMetadata) -> Vec<String> {
    let context = v2_lints::LintContext {
        metadata,
        repo_root: None,
    };
    let findings = v2_lints::run(&context, &mut Vec::new());
    let (errors, _warnings) = split_lint_findings(metadata.loom.version, findings);
    errors.iter().map(ToString::to_string).collect()
}

/// The defect entry `k` leaves, with the way around a two-entry change that
/// is valid only once both entries are applied.
fn invalid_after(k: usize, entry: &FreezePatch, problems: Vec<String>) -> String {
    let (op, hint) = match entry.patch {
        AmendmentPatch::Replace { .. } => ("replace", ""),
        AmendmentPatch::Insert { .. } => (
            "insert",
            "; each entry is applied and checked on its own, so change an entry in place \
             with one `replace` rather than an `insert` beside a `delete`",
        ),
        AmendmentPatch::Delete { .. } => (
            "delete",
            "; each entry is applied and checked on its own, so change an entry in place \
             with one `replace` rather than a `delete` followed by an `insert`",
        ),
    };
    format!(
        "after plan_patch.patches[{k}] ({op} {}[{}]) the plan is invalid, which the \
         amendment would refuse there: {}{hint}",
        field_name(entry.field),
        entry.patch.index(),
        problems.join("; ")
    )
}
