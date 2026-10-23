//! The verdict on a freeze-refusal dispute, checked when it is recorded,
//! against the stage and its worktree:
//!
//! - accept and reject need a non-empty `reasoning` and a citation;
//! - reject needs `instructions` for the next contract writer, kept under
//!   `### Instructions` in its stored reasoning;
//! - accept needs `plan_patch.patches`: entries `{field, op, index, value}` on
//!   `contracts`, `harness` or `acceptance`, replayed in array order over a
//!   copy of the stage's fields, so each index is read against the field as
//!   the earlier entries left it, as the apply reads it;
//! - an `acceptance` entry only replaces a format-check criterion with
//!   another format check that sets no output expectation (`exit_code`,
//!   `stdout_contains`, `stdout_not_contains`, `stderr_empty`), since one
//!   would change what the gate accepts;
//! - a harness glob matches no production source among the worktree's files
//!   and no path the derived scaffold names (loom derives the scaffold; a
//!   harness holds test-only files);
//! - the same entries replayed over the plan's own stage definition leave a
//!   plan the schema accepts and add no blocking `plan verify` lint
//!   (`verdict_freeze_plan`), so the apply cannot refuse what was recorded;
//! - needs-more-evidence needs a question.
//!
//! Every defect is an `Err` the judge reads and corrects. A coerced
//! needs-more-evidence would reach the contract writer and cost it a filing.

use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::path::Path;

use crate::models::dispute::{DisputeVerdict, PlanPatch};
use crate::models::stage::{AcceptanceCriterion, Stage};
use crate::plan::amendment::{AmendmentField, AmendmentPatch};
use crate::plan::amendment_fields::apply_patch_vec;
use crate::plan::schema::ContractSpec;
use crate::stage_service::contracts::Site;
use crate::verify::contracts::changes::{harness_files, stage_base};
use crate::verify::contracts::collection::{harness_source_matches, stage_scaffold, ScaffoldEdit};
use crate::verify::contracts::format_gate::is_format_check;

use super::plan_patch::decode_patch;
use super::verdict::{
    grounding_text, validate_needs_more, verdict_tag, RecordContext, ValidationOutcome,
};

#[path = "verdict_freeze_plan.rs"]
mod plan;

/// Most entries one accept carries: the daemon applies each as one plan
/// amendment within a single tick.
const MAX_PATCHES: usize = 16;

const NO_PATCHES: &str = "an accept verdict needs 'plan_patch': {\"patches\": [{\"field\": ..., \
                          \"op\": ..., \"index\": ..., \"value\": ...}]} with at least one entry";

/// One `plan_patch.patches` entry.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct FreezePatch {
    pub field: AmendmentField,
    pub patch: AmendmentPatch,
    pub reason: Option<String>,
}

/// The verdict `json` states, or the defect that stops it being recorded.
pub(super) fn validate(
    json: &Value,
    context: &RecordContext<'_>,
) -> Result<DisputeVerdict, String> {
    let Some(tag) = verdict_tag(json) else {
        return Err(
            "the verdict has no 'verdict' field; it must be accept, reject or needs-more-evidence"
                .to_string(),
        );
    };
    match tag.as_str() {
        "reject" => reject(json),
        "accept" => accept(json, context),
        "needs-more-evidence" | "needs-evidence" => match validate_needs_more(json) {
            ValidationOutcome::Verdict(verdict) => Ok(verdict),
            ValidationOutcome::Escalate { .. } => Err("a needs-more-evidence verdict needs a \
                 'questions' array holding at least one non-empty question"
                .to_string()),
        },
        other => Err(format!(
            "verdict '{other}' is not one a freeze-refusal dispute takes: accept, reject or \
             needs-more-evidence"
        )),
    }
}

/// The patches of a recorded or relayed `plan_patch`, in apply order.
pub(super) fn decode_patches(plan_patch: &Value) -> Result<Vec<FreezePatch>, String> {
    let entries = plan_patch
        .get("patches")
        .and_then(Value::as_array)
        .filter(|entries| !entries.is_empty())
        .ok_or(NO_PATCHES)?;
    if entries.len() > MAX_PATCHES {
        return Err(format!(
            "plan_patch carries {} patches; at most {MAX_PATCHES}",
            entries.len()
        ));
    }
    entries
        .iter()
        .enumerate()
        .map(|(k, entry)| decode_entry(entry).map_err(|e| format!("plan_patch.patches[{k}]: {e}")))
        .collect()
}

fn decode_entry(entry: &Value) -> Result<FreezePatch, String> {
    let field = match entry.get("field").and_then(Value::as_str) {
        Some("contracts") => AmendmentField::Contracts,
        Some("harness") => AmendmentField::Harness,
        Some("acceptance") => AmendmentField::Acceptance,
        Some(other) => {
            return Err(format!(
                "field '{other}' cannot be amended for a refused freeze; use contracts, harness \
                 or acceptance"
            ))
        }
        None => return Err("missing 'field' string".to_string()),
    };
    let (patch, reason) = decode_patch(entry)?;
    Ok(FreezePatch {
        field,
        patch,
        reason,
    })
}

fn reject(json: &Value) -> Result<DisputeVerdict, String> {
    let (reasoning, citations) = grounding_text(json, "Reject")?;
    let instructions = match json.get("instructions").and_then(Value::as_str) {
        Some(text) if !text.trim().is_empty() => text,
        _ => {
            return Err(
                "a reject verdict needs a non-empty 'instructions' string: what the next \
                        contract writer must change"
                    .to_string(),
            )
        }
    };
    Ok(DisputeVerdict::Reject {
        citations,
        reasoning: format!("{reasoning}\n\n### Instructions\n\n{instructions}"),
    })
}

fn accept(json: &Value, context: &RecordContext<'_>) -> Result<DisputeVerdict, String> {
    let (reasoning, citations) = grounding_text(json, "Accept")?;
    let patches = decode_patches(json.get("plan_patch").ok_or(NO_PATCHES)?)?;
    let site = super::locate_stage_site(context.work_dir, context.stage).map_err(|e| {
        format!("the stage's worktree cannot be located, so no amendment can be checked: {e:#}")
    })?;
    Replay::of(context.stage).run(&patches, context.work_dir, &site)?;
    plan::check(context.work_dir, &context.stage.id, &patches, &site)?;
    Ok(DisputeVerdict::Accept {
        plan_patch: PlanPatch {
            inner: canonical(&patches),
        },
        citations,
        reasoning,
    })
}

/// `{"patches": [{"field", "op", "index", "value", "reason"}, ...]}`, the
/// shape the apply decodes.
fn canonical(patches: &[FreezePatch]) -> Value {
    let entries: Vec<Value> = patches
        .iter()
        .map(|entry| {
            let mut value = serde_json::to_value(&entry.patch).expect("AmendmentPatch serializes");
            value["field"] = serde_json::to_value(entry.field).expect("AmendmentField serializes");
            if let Some(reason) = &entry.reason {
                value["reason"] = Value::String(reason.clone());
            }
            value
        })
        .collect();
    json!({ "patches": entries })
}

/// A copy of the stage's amendable fields, patched in verdict order, and the
/// harness globs the verdict adds.
struct Replay {
    contracts: Vec<ContractSpec>,
    harness: Vec<String>,
    acceptance: Vec<AcceptanceCriterion>,
    added_globs: Vec<String>,
}

impl Replay {
    fn of(stage: &Stage) -> Self {
        Self::new(
            stage.contracts.clone(),
            stage.harness.clone(),
            stage.acceptance.clone(),
        )
    }

    fn new(
        contracts: Vec<ContractSpec>,
        harness: Vec<String>,
        acceptance: Vec<AcceptanceCriterion>,
    ) -> Self {
        Self {
            contracts,
            harness,
            acceptance,
            added_globs: Vec::new(),
        }
    }

    fn run(mut self, patches: &[FreezePatch], work_dir: &Path, site: &Site) -> Result<(), String> {
        self.patch_all(patches, site)?;
        self.check_globs(work_dir, site)
    }

    /// Every entry applied in verdict order.
    fn patch_all(&mut self, patches: &[FreezePatch], site: &Site) -> Result<(), String> {
        for (k, entry) in patches.iter().enumerate() {
            self.apply_nth(k, entry, site)?;
        }
        Ok(())
    }

    /// Entry `k` applied, a defect named by its position.
    fn apply_nth(&mut self, k: usize, entry: &FreezePatch, site: &Site) -> Result<(), String> {
        self.apply(entry, site)
            .map_err(|e| format!("plan_patch.patches[{k}]: {e}"))
    }

    fn apply(&mut self, entry: &FreezePatch, site: &Site) -> Result<(), String> {
        match entry.field {
            AmendmentField::Contracts => {
                let value = parsed::<ContractSpec>(&entry.patch, "contract spec")?;
                replay_patch(&mut self.contracts, &entry.patch, value)
            }
            AmendmentField::Harness => {
                let value = glob(&entry.patch)?;
                self.added_globs.extend(value.clone());
                replay_patch(&mut self.harness, &entry.patch, value)
            }
            AmendmentField::Acceptance => {
                self.replace_format_check(&entry.patch, &site.working_dir)
            }
            AmendmentField::Wiring | AmendmentField::WiringTests => {
                Err("a refused freeze amends only contracts, harness and acceptance".into())
            }
        }
    }

    fn replace_format_check(&mut self, patch: &AmendmentPatch, cwd: &Path) -> Result<(), String> {
        let AmendmentPatch::Replace { index, value } = patch else {
            return Err("an acceptance entry may only replace a format-check criterion".into());
        };
        let Some(current) = self.acceptance.get(*index) else {
            return Err(out_of_range(*index, self.acceptance.len()));
        };
        if !is_format_check(current.command(), cwd) {
            return Err(format!(
                "acceptance[{index}] `{}` is not a format check; only a format-check criterion \
                 may be replaced",
                current.command()
            ));
        }
        let replacement: AcceptanceCriterion = crate::yaml::from_str(value)
            .map_err(|e| format!("value is not an acceptance criterion: {e}"))?;
        if !is_format_check(replacement.command(), cwd) {
            return Err(format!(
                "the replacement `{}` is not a format check",
                replacement.command()
            ));
        }
        if expects_output(&replacement) {
            return Err(format!(
                "the replacement `{}` sets an output expectation (exit_code, stdout_contains, \
                 stdout_not_contains or stderr_empty), which changes what the gate accepts; a \
                 format check passes on exit status 0 alone",
                replacement.command()
            ));
        }
        self.acceptance[*index] = replacement;
        Ok(())
    }

    /// Refuse an added glob that reaches production source: a file of the
    /// worktree (tracked or untracked, under the working directory), or a path
    /// the scaffold of the patched contracts names, which may not exist yet.
    fn check_globs(&self, work_dir: &Path, site: &Site) -> Result<(), String> {
        if self.added_globs.is_empty() {
            return Ok(());
        }
        let scaffold = scaffold_paths(work_dir, site, &self.contracts)?;
        for glob in &self.added_globs {
            let globs = std::slice::from_ref(glob);
            let mut files = harness_files(&site.git, &site.working_dir, globs).map_err(|e| {
                format!("cannot list the worktree for harness glob `{glob}`: {e:#}")
            })?;
            files.extend(scaffold.iter().cloned());
            let source = harness_source_matches(globs, &files);
            if !source.is_empty() {
                return Err(format!(
                    "harness glob `{glob}` matches production source ({}): a harness holds \
                     test-only files, and loom derives the scaffold itself",
                    source.join(", ")
                ));
            }
        }
        Ok(())
    }
}

/// Whether an extended criterion judges more than the command's exit status 0:
/// only `command` and `timeout_secs` leave the format-check gate as it was.
fn expects_output(criterion: &AcceptanceCriterion) -> bool {
    let AcceptanceCriterion::Extended(check) = criterion else {
        return false;
    };
    !check.stdout_contains.is_empty()
        || !check.stdout_not_contains.is_empty()
        || check.stderr_empty.is_some()
        || check.exit_code.is_some_and(|code| code != 0)
}

/// Every path the scaffold of `contracts` names, both alternatives of a new
/// module root included, relative to the working directory.
fn scaffold_paths(
    work_dir: &Path,
    site: &Site,
    contracts: &[ContractSpec],
) -> Result<Vec<String>, String> {
    let unknown = |e: String| {
        format!("the derived scaffold cannot be computed, so no harness glob can be checked: {e}")
    };
    let base = stage_base(&site.git, work_dir).map_err(|e| unknown(format!("{e:#}")))?;
    let edits = stage_scaffold(
        &site.git,
        &site.worktree_root,
        &site.working_dir,
        &base,
        contracts,
    )
    .map_err(unknown)?;
    Ok(edits
        .into_iter()
        .flat_map(|edit| match edit {
            ScaffoldEdit::Declare { path, .. } => vec![path],
            ScaffoldEdit::CreateRoot { paths, .. } => paths,
        })
        .collect())
}

/// The typed value of a replace or insert; `None` for a delete.
fn parsed<T: DeserializeOwned>(patch: &AmendmentPatch, what: &str) -> Result<Option<T>, String> {
    match patch {
        AmendmentPatch::Replace { value, .. } | AmendmentPatch::Insert { value, .. } => {
            crate::yaml::from_str(value)
                .map(Some)
                .map_err(|e| format!("value is not a {what}: {e}"))
        }
        AmendmentPatch::Delete { .. } => Ok(None),
    }
}

/// A harness glob is taken verbatim, trimmed, as the amendment takes it.
fn glob(patch: &AmendmentPatch) -> Result<Option<String>, String> {
    match patch {
        AmendmentPatch::Replace { value, .. } | AmendmentPatch::Insert { value, .. } => {
            let glob = value.trim();
            if glob.is_empty() {
                return Err("a harness value must be a non-empty glob".into());
            }
            Ok(Some(glob.to_string()))
        }
        AmendmentPatch::Delete { .. } => Ok(None),
    }
}

/// `apply_patch_vec` fails here only on an index: `parsed` and `glob` give
/// every replace and insert a value.
fn replay_patch<T: Clone>(
    list: &mut Vec<T>,
    patch: &AmendmentPatch,
    value: Option<T>,
) -> Result<(), String> {
    let len = list.len();
    apply_patch_vec(list, patch, value).map_err(|_| out_of_range(patch.index(), len))
}

fn out_of_range(index: usize, len: usize) -> String {
    format!(
        "index {index} is out of range for the field as the earlier entries left it ({len} \
         entries)"
    )
}
