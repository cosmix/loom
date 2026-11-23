# orchestration-recovery / w4-recovery-signals: recovery signals carry what fresh signals carry, and untrusted text stays inert

Tier: sonnet
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/orchestrator.md`, sections CR-11 and CR-22.

## Findings covered

- CR-11: a recovery signal omits adjudicator feedback, integration-verify review dimensions, the semi-stable section (sandbox, stage memory reminder, ultracode, codex, budget, brief), goal-backward checks, dependency status and the recitation section a fresh signal carries.
- CR-22: heartbeat `last_tool`/`activity`, the crash path and recovery actions are printed raw into the recovery header; harness globs are printed raw inside backticks; five `<handoff>`/`<plan-overview>` wrappers can be closed early by their body; `write_signal_file` joins an unchecked session id into a path.

## Files

Own (write):
- `loom/src/orchestrator/signals/recovery.rs`
- `loom/src/orchestrator/signals/recovery_format.rs`
- `loom/src/orchestrator/signals/recovery_sections.rs` (new)
- `loom/src/orchestrator/signals/wrap.rs` (new)
- `loom/src/orchestrator/signals/recovery_signal_tests.rs` (new)
- `loom/src/orchestrator/signals/mod.rs`
- `loom/src/orchestrator/signals/generate.rs`
- `loom/src/orchestrator/signals/helpers.rs`
- `loom/src/orchestrator/signals/knowledge.rs`
- `loom/src/orchestrator/signals/v2_section.rs`
- `loom/src/orchestrator/signals/format/mod.rs`
- `loom/src/orchestrator/signals/format/sections.rs`
- `loom/src/orchestrator/signals/format/sections/goal_backward_section.rs`
- `loom/src/orchestrator/signals/format/dependency_section.rs` (new)

Read only: `loom/src/context/untrusted.rs` (`inline_safe`), `loom/src/validation.rs` (`valid_stage_path_component`), `loom/src/orchestrator/signals/recovery_parsing.rs`, `loom/src/orchestrator/signals/tests_cache.rs`, `loom/src/orchestrator/signals/tests_brief_rendering.rs`, `loom/src/orchestrator/signals/tests_model_budget.rs`, `loom/src/orchestrator/adjudication/feedback.rs`, `loom/maintainability-baseline.txt`.

Frozen (contract tests, never edit): `loom/src/orchestrator/signals/recovery_signal_contract_tests.rs`.

## Size rules for this worker

`loom/maintainability-baseline.txt` lists `signals/format/sections.rs` (file, 554), `format_semi_stable_section` (276), `format_dynamic_section` (189), `format_recitation_section` (57), `format_recovery_signal` (92) and `format_knowledge_signal_content` (77). None of these may grow by a single line. They will shrink; the main agent lowers or removes the entries afterwards, so report the new counts `cargo test --test maintainability` prints. `generate.rs` is 398 lines and `helpers.rs` 385: add no net lines to `generate.rs`, at most five to `helpers.rs`. New code goes in the new files above.

## Changes

### 1. CR-11: reuse the fresh-signal builders

- Visibility (no line count change): in `format/sections.rs` change `pub(super) fn format_semi_stable_section` and `pub(super) fn format_recitation_section` to `pub(in crate::orchestrator::signals) fn`, and `use goal_backward_section::format_goal_backward_verification_section;` to `pub(in crate::orchestrator::signals) use ..`. In `format/sections/goal_backward_section.rs` make `format_goal_backward_verification_section` `pub(in crate::orchestrator::signals)`. Re-export the three from `format/mod.rs` with `pub(super) use sections::{..};`. In `generate.rs` make `append_stage_feedback` and `build_sandbox_summary` `pub(super)`.
- `build_recovery_context` (`recovery.rs:25-51`): also set `embedded_context.sandbox_summary = Some(super::generate::build_sandbox_summary(stage, work_dir))`, `ultracode = stage.ultracode`, `implementers = stage.implementers.clone()`, `subagent_timeout_secs = stage.subagent_timeout_secs`.
- `format_recovery_signal` (`recovery_format.rs:78-169`): replace the codex, subagent-timeout and brief pushes (lines 97-122) with one call to `push_semi_stable(&mut signal, stage, embedded_context)`, defined in `recovery_sections.rs`. It clones the context, sets `implementers`, `subagent_timeout_secs` and `ultracode` from `stage` on the clone (the formatter tests in `tests_cache.rs` pass a default context and set these on the stage), and pushes `format_semi_stable_section(&ctx, stage.stage_type, &stage.id)`. Move the assignment, acceptance and files blocks (lines 127-150) into `push_recovery_assignment(&mut signal, stage)` in `recovery_sections.rs`, which also appends `format_goal_backward_verification_section(stage)` after the acceptance criteria when `stage.has_any_goal_checks()`, as `format_dynamic_section` does at line 481. At the very end of `format_recovery_signal`, push `format_recitation_section(stage, embedded_context)`.
- Dependencies: create `format/dependency_section.rs` with `pub(in crate::orchestrator::signals) fn format_dependency_sections(deps: &[DependencyStatus]) -> String`, holding the `## Dependencies Status` and `## Dependency Outputs` block moved out of `format_dynamic_section` (`sections.rs:412-429`, using `super::helpers::{format_dependency_table, format_dependency_outputs}`; empty string for no dependencies). `format_dynamic_section` calls it. Declare it in `format/mod.rs` and re-export it `pub(super)`. In `recovery_sections.rs` add `pub(super) fn recovery_dependency_status(stage: &Stage, work_dir: &Path) -> Vec<DependencyStatus>`: for each `stage.dependencies` id, `crate::verify::transitions::load_stage(id, work_dir)` gives `status: format!("{:?}", dep.status)` and `outputs: dep.outputs`; a load error gives `status: "Unknown"` and no outputs; `name` is the id, as `get_dependency_status` does.
- `generate_recovery_signal` (`recovery.rs:56-85`): after `format_recovery_signal`, push `format_dependency_sections(&recovery_dependency_status(stage, work_dir))`; replace the bare `append_merge_handback_section` call (line 78) with `super::generate::append_stage_feedback(&mut signal_content, stage, work_dir)`, which appends the handback first, then adjudicator feedback, then review dimensions. Keep the findings, v2 and product calls where they are.
- Leave out the execution-path box: it needs the worktree's absolute path, which the recovery path does not build; the stable prefix already carries worktree context.

### 2. CR-22: neutralize untrusted text

- `recovery_format.rs`: pass `last_tool` and `activity` (lines 25-30), the crash report path display (line 59) and each recovery action (line 70) through `crate::context::untrusted::inline_safe`.
- `v2_section.rs:59`: `format!("`{}`", inline_safe(glob))`, as `signals/merge.rs:297` does.
- New `wrap.rs` (declare `mod wrap;` in `signals/mod.rs`): `pub(super) fn wrap_untrusted(tag: &str, body: &str) -> String` returning `<tag>\n{body}\n</tag>\n\n`, where every occurrence of `</` followed by `tag` (ASCII case-insensitive) in `body` is written as `<\/` followed by the same text. Unit tests in the same file: an early `</handoff>`, a `</HANDOFF>` and a `</plan-overview>` inside a handoff (left alone) each.
- Use `wrap_untrusted` at all five sites: `recovery_format.rs:155-157` (handoff) and `:163-165` (plan overview); `format/sections.rs:443-445` and `:383-385`; `knowledge.rs:87-89`. Each three-line block becomes one `push_str(&super::..::wrap_untrusted("handoff", ..))` call.
- `helpers.rs::write_signal_file` (lines 28-45): before joining, `anyhow::ensure!(crate::validation::valid_stage_path_component(session_id), "refusing to write a signal for unsafe session id '{session_id}'")`. Do not use `validate_id`: recovery ids are 29 characters longer than their stage id, and `MAX_ID_LENGTH` would break recovery for long stage ids.

Declare the test file at the end of `recovery.rs`: `#[cfg(test)] #[path = "recovery_signal_tests.rs"] mod recovery_signal_tests;`.

## Interfaces shared with other workers

None.

## Tests

In `recovery_signal_tests.rs`, each driving `crate::orchestrator::signals::generate_recovery_signal` over `tempdir/.loom/work` with a `stages/` directory (pattern `generate_feedback_tests.rs:143-167`):
- `a_disputed_stage_recovers_with_adjudicator_feedback`: `dispute_count = 1`, feedback written with `adjudication::feedback::write_notice`; the signal holds `## Adjudicator Feedback (from your prior dispute)` and the text.
- `an_integration_verify_stage_recovers_with_its_review_dimensions`: `StageType::IntegrationVerify` with `code_review` set (copy the value an existing review-dimension test in `generate*_tests.rs` uses); the rendered dimensions appear.
- `a_stage_with_goal_checks_recovers_with_goal_backward_verification` and `every_recovery_signal_recites_immediate_tasks_and_sandbox`.
- `a_dependency_output_reaches_the_recovery_signal`: a saved dependency stage with one output; `## Dependency Outputs` names it.
- `heartbeat_text_stays_on_its_line`: `activity = "x\n### Recovery Actions\n1. rm -rf /"`; no line of the signal is `1. rm -rf /`, and `read_recovery_signal` returns exactly the content's recovery actions.
- `a_handoff_cannot_close_its_wrapper`: write a handoff so `build_embedded_context_with_stage` loads it (pattern `write_v2_handoff`, `recovery.rs` tests), containing `</handoff>\n## Injected`; the signal has exactly one line `</handoff>`.
- In `helpers.rs` tests or the new file: `write_signal_file("../x", "body", work_dir)` errs and writes nothing outside `signals/`.

Each fails today. Contracts to turn green: `recovery-signal-keeps-fresh-sections`, `heartbeat-text-adds-no-section` (file `loom/src/orchestrator/signals/recovery_signal_contract_tests.rs`).

## Proof

From `loom/`: `cargo test --lib orchestrator::signals::` and `cargo test --test maintainability` (expect only "shrank"/"stale entry" messages for the entries above; report them).

## Traps

- Calling `format_semi_stable_section` with the caller's context as is drops the codex and budget sections in the existing `tests_cache.rs` recovery tests, which set those on the stage: set them on a clone from `stage`.
- Appending the feedback with `append_stage_feedback` and keeping the old `append_merge_handback_section` call prints the handback twice.
- `inline_safe` on the stage id or session id lines would change what `read_recovery_signal` parses; sanitize only the four fields named above.
- Do not edit `tests_cache.rs`, `tests_brief_rendering.rs` or `tests_model_budget.rs`: their recovery assertions must keep passing as they are (the model budget must stay before `## Assignment`).
