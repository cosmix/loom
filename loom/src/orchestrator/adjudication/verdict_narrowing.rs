//! The narrowing rule of a criterion verdict: an accept may leave tests out of
//! a stage's check (delete a criterion, or replace it with one that selects
//! fewer tests) only on loom's own base run of that criterion
//! (`loom check <stage> --criterion <n> --at-base`) showing it fails at the
//! stage's base, and every test it leaves out is named in
//! `plan_patch.leaves_out`, each becoming a [`Handoff`].
//!
//! A criterion that passes at the base, selects no test there, or does not
//! build there fails because of the stage's own change: narrowing it would
//! hide the stage's defect. Which tests a `replace` selects is not computed:
//! a replace without `leaves_out` is taken as selecting the same tests, and
//! the briefing tells the judge that such an amendment is invalid.

use serde::Deserialize;
use serde_json::Value;

use crate::models::dispute::{DisputeKind, DisputeVerdict};
use crate::models::stage::{AcceptanceCriterion, Stage, TruthCheck};
use crate::verify::review::handoffs::{load_base_evidence, BaseEvidence, Handoff};

use super::verdict::{RecordContext, ValidationOutcome};

/// The field a criterion narrowing amends: base runs are of acceptance criteria.
pub(super) const NARROWED_FIELD: &str = "acceptance";

/// One `plan_patch.leaves_out` entry: the tests an amendment of criterion
/// `index` leaves out of the stage's check.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LeftOut {
    index: usize,
    tests: Vec<String>,
    runs_at: String,
    reason: String,
}

/// Check the narrowing `plan_patch` makes of `current` (the stage's entries of
/// the patched field, as JSON) against the stage's recorded base runs, and
/// return the hand-offs it records. The `Err` text is for the judge.
pub fn check_narrowing(
    stage_id: &str,
    dispute: &str,
    plan_patch: &Value,
    current: &[Value],
    evidence: &[BaseEvidence],
) -> Result<Vec<Handoff>, String> {
    let left_out = left_out(plan_patch)?;
    let patches = patches(plan_patch);
    for patch in &patches {
        check_patch(patch, current, &left_out)?;
    }
    if let Some(entry) = left_out
        .iter()
        .find(|entry| !patched(&patches, entry.index))
    {
        return Err(format!(
            "plan_patch.leaves_out names criterion {}, which no replace or delete patch \
             amends",
            entry.index
        ));
    }
    left_out
        .into_iter()
        .map(|entry| handoff(stage_id, dispute, entry, current, evidence))
        .collect()
}

/// What a recorded verdict's narrowing is checked on: its `plan_patch`, the
/// stage's acceptance as JSON and the stage's base runs.
pub(super) struct NarrowingInputs<'a> {
    pub plan_patch: &'a Value,
    pub current: Vec<Value>,
    pub evidence: Vec<BaseEvidence>,
}

/// The [`check_narrowing`] inputs of `outcome` on a dispute of `kind`: `Some`
/// for a criterion dispute's accept that amends the stage's acceptance, `None`
/// for every other outcome, which records unchanged.
pub(super) fn narrowing_inputs<'a>(
    outcome: &'a ValidationOutcome,
    kind: &DisputeKind,
    context: &RecordContext<'_>,
) -> Result<Option<NarrowingInputs<'a>>, String> {
    let ValidationOutcome::Verdict(DisputeVerdict::Accept { plan_patch, .. }) = outcome else {
        return Ok(None);
    };
    if !matches!(kind, DisputeKind::Criterion { .. }) || !narrows_acceptance(&plan_patch.inner) {
        return Ok(None);
    }
    let evidence = load_base_evidence(context.work_dir, &context.stage.id)
        .map_err(|e| format!("the stage's base runs could not be read: {e:#}"))?;
    Ok(Some(NarrowingInputs {
        plan_patch: &plan_patch.inner,
        current: acceptance_json(context.stage),
        evidence,
    }))
}

/// Whether a criterion dispute's accept amends the stage's acceptance list.
pub(super) fn narrows_acceptance(plan_patch: &Value) -> bool {
    plan_patch.get("field").and_then(Value::as_str) == Some(NARROWED_FIELD)
}

/// `stage`'s acceptance criteria as JSON: a string, or an object with `command`.
pub(super) fn acceptance_json(stage: &Stage) -> Vec<Value> {
    stage
        .acceptance
        .iter()
        .map(|criterion| serde_json::to_value(criterion).expect("a criterion always serializes"))
        .collect()
}

/// The patches of `plan_patch` in any of its shapes: a `patches` array, a
/// nested `patch` object or the flat `{op, index, value}` object.
fn patches(plan_patch: &Value) -> Vec<&Value> {
    if let Some(items) = plan_patch.get("patches").and_then(Value::as_array) {
        return items.iter().collect();
    }
    if let Some(patch) = plan_patch.get("patch") {
        return vec![patch];
    }
    if plan_patch.get("op").is_some() {
        return vec![plan_patch];
    }
    Vec::new()
}

/// Whether a `replace` or `delete` of `patches` names `index`.
fn patched(patches: &[&Value], index: usize) -> bool {
    patches.iter().any(|patch| {
        matches!(
            patch.get("op").and_then(Value::as_str),
            Some("replace" | "delete")
        ) && patch.get("index").and_then(Value::as_u64) == Some(index as u64)
    })
}

fn left_out(plan_patch: &Value) -> Result<Vec<LeftOut>, String> {
    let Some(raw) = plan_patch.get("leaves_out") else {
        return Ok(Vec::new());
    };
    let entries: Vec<LeftOut> = serde_json::from_value(raw.clone()).map_err(|e| {
        format!(
            "plan_patch.leaves_out is malformed: {e}; each entry is {{\"index\": <n>, \
             \"tests\": [\"<test>\", ...], \"runs_at\": \"integration-verify\"|\"ci\"|\"nowhere\", \
             \"reason\": \"<why>\"}}"
        )
    })?;
    let mut seen = std::collections::HashSet::new();
    for entry in &entries {
        if !seen.insert(entry.index) {
            return Err(format!(
                "plan_patch.leaves_out names criterion {} twice; list its tests in one entry",
                entry.index
            ));
        }
        if entry.tests.iter().all(|test| test.trim().is_empty()) {
            return Err(format!(
                "plan_patch.leaves_out entry for criterion {} names no test; name every test \
                 the amendment leaves out",
                entry.index
            ));
        }
        if entry.runs_at.trim().is_empty() {
            return Err(format!(
                "plan_patch.leaves_out entry for criterion {} has an empty runs_at; say where \
                 its tests run: integration-verify, ci, or nowhere",
                entry.index
            ));
        }
    }
    Ok(entries)
}

/// A `replace` must change its criterion; a `delete` must say what it leaves out.
fn check_patch(patch: &Value, current: &[Value], left_out: &[LeftOut]) -> Result<(), String> {
    let Some(index) = patch.get("index").and_then(Value::as_u64) else {
        return Ok(());
    };
    let index = index as usize;
    match patch.get("op").and_then(Value::as_str) {
        Some("replace") => {
            let unchanged = match (patch.get("value"), current.get(index)) {
                (Some(value), Some(entry)) => same_entry(value, entry),
                _ => false,
            };
            if unchanged {
                return Err(format!(
                    "the replace of criterion {index} changes nothing: its value equals the \
                     current criterion. Name the change the criterion needs, or reject"
                ));
            }
        }
        Some("delete") if !left_out.iter().any(|entry| entry.index == index) => {
            return Err(format!(
                "the delete of criterion {index} has no plan_patch.leaves_out entry for index \
                 {index}; name every test the deletion leaves out, where it will run and why"
            ));
        }
        _ => {}
    }
    Ok(())
}

/// Whether a patch `value` (JSON, or YAML text) is the criterion `entry`,
/// both read as an [`AcceptanceCriterion`]: `"cargo test"` and
/// `{command: "cargo test"}` are the same criterion.
fn same_entry(value: &Value, entry: &Value) -> bool {
    if value == entry {
        return true;
    }
    let Some(entry) = as_check(entry) else {
        return false;
    };
    let parsed = value
        .as_str()
        .and_then(|text| crate::yaml::from_str::<Value>(text).ok());
    std::iter::once(value)
        .chain(parsed.as_ref())
        .any(|candidate| as_check(candidate).as_ref() == Some(&entry))
}

/// `value` as a criterion, a bare command read as a check with only `command`.
fn as_check(value: &Value) -> Option<TruthCheck> {
    match serde_json::from_value::<AcceptanceCriterion>(value.clone()).ok()? {
        AcceptanceCriterion::Simple(command) => {
            serde_json::from_value(serde_json::json!({ "command": command })).ok()
        }
        AcceptanceCriterion::Extended(check) => Some(check),
    }
}

fn command_of(entry: &Value) -> Option<&str> {
    entry
        .as_str()
        .or_else(|| entry.get("command").and_then(Value::as_str))
}

/// The hand-off of `entry`, on the base run of its criterion.
fn handoff(
    stage_id: &str,
    dispute: &str,
    entry: LeftOut,
    current: &[Value],
    evidence: &[BaseEvidence],
) -> Result<Handoff, String> {
    let index = entry.index;
    let command = current.get(index).and_then(command_of).ok_or_else(|| {
        format!(
            "plan_patch.leaves_out names criterion {index}, which the stage's acceptance \
             list does not have"
        )
    })?;
    let run = evidence
        .iter()
        .find(|run| run.index == index && run.command == command)
        .ok_or_else(|| {
            format!(
                "criterion {index} has no base run: run `loom check {stage_id} --criterion \
                 {index} --at-base` and read its `Host job:` line before narrowing it"
            )
        })?;
    check_run(index, run, entry.tests.len())?;
    Ok(Handoff {
        stage: stage_id.to_string(),
        field: NARROWED_FIELD.to_string(),
        index,
        tests: entry.tests,
        runs_at: entry.runs_at,
        reason: entry.reason,
        dispute: dispute.to_string(),
        // A timed-out run's count covers only the tests that ran before the kill.
        verified: run.failed.is_some() && !run.timed_out,
    })
}

/// The base run of criterion `index` must fail, and fail at least the
/// `left_out` tests the amendment drops.
fn check_run(index: usize, run: &BaseEvidence, left_out: usize) -> Result<(), String> {
    const FIX: &str = "answer needs-more-evidence telling the agent to fix its test, or, for a \
                       frozen contract, to file `loom stage dispute-contract` with a replacement";
    if run.passed {
        return Err(format!(
            "criterion {index} passes at the base: it fails because of the stage's own \
             change. Do not narrow it; {FIX}"
        ));
    }
    if run.build_failed || run.executed == Some(0) {
        return Err(format!(
            "criterion {index} does not build or selects no test at the base: it fails \
             because of the stage's own change or its own new test. Do not narrow it; {FIX}"
        ));
    }
    match run.failed {
        Some(failed) if (failed as usize) < left_out => Err(format!(
            "criterion {index} fails only {failed} test(s) at the base, but the amendment \
             leaves out {left_out}; leave out only the tests that fail at the base"
        )),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;

    fn narrowing() -> Value {
        json!({
            "field": "acceptance",
            "patches": [{"op": "delete", "index": 0}],
            "leaves_out": [{
                "index": 0, "tests": ["net_fetch"], "runs_at": "ci", "reason": "network",
            }],
        })
    }

    fn base_run(timed_out: bool) -> BaseEvidence {
        BaseEvidence {
            index: 0,
            command: "cargo test".to_string(),
            passed: false,
            executed: Some(3),
            failed: Some(1),
            build_failed: false,
            timed_out,
        }
    }

    #[test]
    fn a_timed_out_base_run_hands_off_unverified() {
        let current = [json!("cargo test")];
        let check = |timed_out| {
            check_narrowing("s1", "1", &narrowing(), &current, &[base_run(timed_out)]).unwrap()
        };
        assert!(check(false)[0].verified);
        assert!(!check(true)[0].verified);
    }

    #[test]
    fn a_bare_command_and_its_command_object_are_the_same_criterion() {
        let entry = json!("cargo test");
        assert!(same_entry(&json!({"command": "cargo test"}), &entry));
        assert!(same_entry(&json!("command: cargo test"), &entry));
        assert!(same_entry(
            &json!("cargo test"),
            &json!({"command": "cargo test"})
        ));
        assert!(!same_entry(
            &json!({"command": "cargo test", "timeout_secs": 60}),
            &entry
        ));
        assert!(!same_entry(&json!("cargo test --lib"), &entry));
    }
}
