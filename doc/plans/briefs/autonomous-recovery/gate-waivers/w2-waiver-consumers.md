# gate-waivers W2: completion honours a waiver, and integration-verify sees every waiver

Tier: sonnet. Wave 2, after W0. Read `doc/plans/briefs/autonomous-recovery/common.md` first.
Frozen contracts: `tests/gate_waiver_contracts.rs`.

## Edits (you own these files)

1. `src/commands/stage/complete_verification.rs`: add
   `pub(crate) fn checked_or_waived(stage: &Stage, label: &str, checked: Result<()>) ->
   Result<()>`: when `checked` is an `Err` and `stage.gate_waived(label)` returns a waiver,
   print `  ⊘ <label> failed and is waived by dispute <n> (loom <version>): <reason>` and
   return `Ok(())`; otherwise return `checked` unchanged (a passing gate prints nothing new).
   `run_checks` wraps the `unwired` and `duplicates` sink calls with it. The gate still runs,
   so its output stays in the log. The live-test-lane plan's `live` sink in `run_checks` is a
   plan list (`--field live`), never wrapped; `run_checks` stays under 50 lines with it.
2. `src/commands/stage/complete_verification_v2.rs`: `run_v2` wraps the impact-tests sink call
   with `checked_or_waived(stage, IMPACT_TESTS_LABEL, ...)`. Keep `observe_impact` in
   `host_verify/child_complete.rs` working: it reads the label and the raw result; a waived
   failure still records the observed run (no edit there).
3. `src/plan/amendment.rs`: export `pub fn audit_rows(work_dir: &Path) ->
   Result<Vec<AuditEntry>>` where `pub struct AuditEntry { pub version: u64, pub stage_id:
   String, pub field: AmendmentField, pub op: &'static str, pub index: usize, pub dispute_id:
   Option<String>, pub reason: Option<String> }`, built from the journal's rows
   (`read_audit_rows`). Nothing else in that file changes.
4. New `src/orchestrator/signals/audit_section.rs` with `pub(super) fn
   append_audit_section(content: &mut String, work_dir: &Path)`: a `## Waived Gates and Plan
   Amendments` section listing every stage's `gate_waivers`
   (`- <stage>: <gate> waived by dispute <n> (loom <version>): <reason>`) and every audit entry
   (`- <stage>: <field> <op> <index> (dispute <id>): <reason>`), each value through
   `crate::context::untrusted::inline_safe`; no section when both are empty. Call it from the
   integration-verify signal builder only: `src/orchestrator/signals/generate.rs`, inside the
   `if matches!(stage.stage_type, StageType::IntegrationVerify)` block that renders the
   code-review dimensions (370 lines; the call is one line, the module declaration one more).
   The reviewers it briefs judge every waiver and amendment.
5. Tests: `checked_or_waived` keeps an `Err` without a waiver and for another label; the
   section lists a waiver and an amendment and is absent when there are none.

## Traps

- The host job child loads the stage fresh (`child_complete.rs::verify`), so a waiver recorded
  by the daemon is seen by the next completion. Never cache waivers.
- `checked_or_waived` must not turn a gate that could not RUN for another reason into a pass
  for a different label: match the label exactly.

## Done

Your one check after the stage compiles: `cargo test --lib orchestrator::signals::`.
