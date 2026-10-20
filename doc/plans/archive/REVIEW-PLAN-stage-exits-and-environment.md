# Code Review: Stage exits and the stage environment

**Plan:** PLAN-stage-exits-and-environment | **Generated:** 2026-10-01 13:25 UTC

## Summary

## Overview

## Changes by Stage

### Completion gates: impact selection, criterion disputes, contract freeze (completion-gates)

**Status:** completed  
**Purpose:** Implement stage 1 of doc/plans/PLAN-stage-exits-and-environment.md ("completion-gates", and "Choices this plan settles" 1-7).
Use parallel subagents and skills to maximize performance.
Every worker first reads doc/plans/briefs/stage-exits-and-environment/common.md, then its own brief.
Spawn every worker BY AGENT TYPE with the fixed prompt plus "Your brief: <path>. Read it in full before anything else."
Territories are DISJOINT. Workers NEVER spawn subagents.
Waves: G1, G2 and G4 in ONE message; then G3. The crate does not compile between the waves (G2 changes DisputeKind; G3 migrates src/orchestrator/adjudication/), so no wave-1 worker runs a check; G3 runs its one check and reports (never fixes) errors in files it does not own. After G3 the main agent runs cargo fmt --all once, routes any remaining compile error to a fresh worker for the file's owner territory, then builds and tests.

| Worker | Role | Tier | Files owned | Shared context | Brief path |
| ------ | ---- | ---- | ----------- | -------------- | ---------- |
| G1 | Impact selection and fixtures | sonnet | src/verify/impact_tests.rs; src/verify/impact_tests/runs.rs; src/verify/impact_tests_tests.rs; src/verify/review/fingerprint.rs; src/commands/stage/complete_verification_v2.rs; src/skills/project/scan.rs; src/skills/project/tests.rs | common.md | doc/plans/briefs/stage-exits-and-environment/completion-gates/g1-impact-selection.md |
| G2 | Dispute field plumbing and bookkeeping | sonnet | src/models/dispute.rs; src/models/dispute_tests.rs; src/models/stage/dispute_budgets.rs; src/cli/types_stage.rs; src/cli/types_stage_disputes.rs; src/cli/dispatch_stage.rs; src/commands/stage/dispute_criteria.rs; src/commands/stage/dispute_criteria_tests.rs; src/commands/stage/dispute_transport.rs; src/commands/stage/state/loop_recovery/mod.rs; src/commands/stage/state_tests.rs; src/daemon/protocol.rs; src/daemon/protocol_debug.rs; src/daemon/wire_tests.rs; src/daemon/server/dispute.rs; src/daemon/server/dispute_tests.rs; src/daemon/server/dispute_kinds.rs; src/daemon/server/dispute_kinds_tests.rs; src/daemon/server/stage_control.rs; src/daemon/server/self_service_tests.rs; src/daemon/server/tests/self_service_client.rs; src/fs/stage_request/types.rs; src/fs/stage_request/apply.rs; src/fs/stage_request/tests/apply.rs; src/fs/stage_request/tests/spool.rs; src/orchestrator/core/inbox_drain/apply.rs; src/orchestrator/core/inbox_drain/test_support.rs; src/orchestrator/core/verdict_apply_tests.rs; src/orchestrator/signals/adjudication.rs; src/relay/payload.rs; src/completions/dynamic/mod.rs; src/completions/dynamic/commands.rs; src/completions/dynamic/tests/tests_stage.rs; tests/adjudication_e2e.rs; maintainability-baseline.txt | common.md | doc/plans/briefs/stage-exits-and-environment/completion-gates/g2-dispute-field-plumbing.md |
| G3 | Dispute field adjudication and gap labels | opus | src/orchestrator/adjudication/prompt.rs; src/orchestrator/adjudication/prompt/criterion.rs; src/orchestrator/adjudication/prompt/criterion_entry.rs; src/orchestrator/adjudication/prompt/tests.rs; src/orchestrator/adjudication/prompt/tests_kinds.rs; src/orchestrator/adjudication/prompt/tests_golden.rs; src/orchestrator/adjudication/verdict.rs; src/orchestrator/adjudication/verdict_tests.rs; src/orchestrator/adjudication/plan_patch.rs; src/orchestrator/adjudication/apply.rs; src/orchestrator/adjudication/tests.rs; src/orchestrator/adjudication/record_tests.rs; src/orchestrator/adjudication/mod.rs; src/orchestrator/adjudication/tests_criterion_field.rs; src/orchestrator/adjudication/closed_disputes.rs; src/orchestrator/adjudication/feedback.rs; src/verify/goal_backward/mod.rs | common.md; G2's CriterionField | doc/plans/briefs/stage-exits-and-environment/completion-gates/g3-dispute-field-adjudication.md |
| G4 | Contract freeze formatter gate | sonnet | src/verify/contracts/format_gate.rs; src/verify/contracts/mod.rs; src/commands/stage/contracts/freeze.rs; src/orchestrator/signals/contract.rs; src/orchestrator/signals/contract_tests.rs | common.md | doc/plans/briefs/stage-exits-and-environment/completion-gates/g4-freeze-format-gate.md |

CONTRACT SURFACE (the contract session writes tests/completion_gates_contracts.rs from this, before any code; top-level #[test] fns, so each contract's test value is its fn name):

- loom::verify::impact_tests::stage_changes(worktree: &Path, target_branch: &str) -> anyhow::Result<Vec<PathBuf>>: worktree-relative paths changed since git merge-base HEAD <target_branch>, committed on the branch, staged, unstaged or untracked and not ignored; sorted, no duplicates. worktree is the worktree root.
- loom::verify::impact_tests::run(stage: &Stage, working_dir: &Path, criteria_config: &loom::verify::criteria::CriteriaConfig, target_branch: &str) -> anyhow::Result<ImpactOutcome>; ImpactOutcome { ran: Vec<String>, notes: Vec<String> }. A JS package with no node_modules gives the note "`<package>` has no node_modules in this worktree (a plan `provision` entry installs it); the full suite runs in integration-verify" and runs nothing. Stage is loom::models::stage::Stage (Stage::default() works).
- loom::skills::project::ProjectProfile::discover(dir).package_details() -> Vec<PackageDetail { path: PathBuf (relative to the checkout root), kinds, runner, skills }>. No package has a path component named `fixtures`.
- loom::models::dispute::{CriterionField, DisputeKind, DisputeRequest, DisputeVerdict, request_file}: CriterionField::{Acceptance, Wiring, WiringTests} (serde "acceptance", "wiring", "wiring-tests"); DisputeKind::criterion(field: CriterionField, criterion_index: usize) -> DisputeKind builds DisputeKind::Criterion { criterion_index, field }. request.md is YAML frontmatter between the first two "---" lines, parsed into DisputeRequest with serde_yaml.
- loom::daemon::handle_dispute_criteria(work_dir: &Path, stage_id: &str, field: CriterionField, criterion_index: usize, reason: String, evidence_commit: Option<String>, failure_output: Option<String>) -> anyhow::Result<loom::daemon::Response>. A filed dispute answers Response::DisputeCreated { id }; a refusal answers Response::Error { message } and writes no request.md (an out-of-range index's message contains "out of range"). Stage setup mirrors daemon/server/dispute.rs tests::setup: loom::fs::work_dir::WorkDir::new(tmp) then .initialize(), then loom::verify::transitions::save_stage(&stage, wd.root()); request.md is loom::models::dispute::request_file(&wd.root().join("disputes"), stage_id, id). Stage fields: id, name, status (loom::models::stage::StageStatus), acceptance (Vec<loom::plan::schema::AcceptanceCriterion>, Simple(String)), wiring (Vec<loom::plan::schema::WiringCheck { source, pattern, description, literal }>), wiring_tests, dispute_count; ..Stage::default().
- loom::orchestrator::adjudication::verdict::{parse_and_validate_for, ValidationOutcome}: parse_and_validate_for(raw: &str, kind: &DisputeKind) -> ValidationOutcome; ValidationOutcome::Verdict(DisputeVerdict::Accept { plan_patch, citations, reasoning }) with plan_patch.inner a serde_json::Value holding "field"; an accept needs "reasoning" and one citation {"file","line","excerpt","claim"}; plan_patch is {"field": ..., "patch": {"op": "replace", "index": 0, "value": "<YAML>"}, "reason": "..."}.
- loom::verify::contracts::format_gate::format_problems(acceptance: &[loom::plan::schema::AcceptanceCriterion], setup: &[String], working_dir: &Path, confinement: loom::models::stage::CommandConfinement) -> anyhow::Result<Vec<String>>: runs only the criteria that are formatter checks (every simple command the criterion expands to is one of: cargo fmt with --check, rustfmt --check, prettier --check, oxfmt --check, biome format or biome check without --write/--fix/--apply, ruff format --check, black --check, a format:check script; gofmt -l is not one, and a compound such as cargo fmt --check && cargo clippy is not run) and returns one problem per failing one, naming its command.
- Git in tests runs with GIT_CONFIG_GLOBAL and GIT_CONFIG_SYSTEM pointed at missing files and GIT_CONFIG_NOSYSTEM=1, user.name/user.email set locally, as src/verify/impact_tests_tests.rs does.

HAZARD (installed binary): this plan runs on the loom binary installed before it (built from main after PLAN-source-graph-mechanism), which still selects impact tests from the nearest published base layer. If loom stage complete's impact-selected step picks web/ vitest files, run cd "$(git rev-parse --show-toplevel)/web" && bun install --frozen-lockfile in this worktree (a bare cd web fails from loom/; registry.npmjs.org is allowed; the bun cache is pre-granted), then run loom stage complete again.
EXPECTED INTEGRITY EVENTS: TI-ratchet-loom/maintainability-baseline.txt only (G2 removes the handle_dispute_criteria line). File ONE dispute-integrity for it after the final review round, reason "tightening only: handle_dispute_criteria refactored under 50 lines as the plan prescribes". No other ledger line and no existing assertion line changes; tests/adjudication_e2e.rs stays at 525 lines.
CONTRACTS: before the final review round, prove each contract red by mutation (Gate conventions) and record loom memory note "mutation: <id> red under <mutation>".
MEMORY: record mistakes, decisions and surprises via loom memory immediately (subagents too); never loom knowledge in this stage; never Claude Code auto-memory.

#### Files Changed

No changes recorded.

#### Key Decisions

- completion-gates contracts: the fixture-package contract expects exactly packages ["", tests/fixtures-extra/pkg] (root Cargo.toml is a package, path ""); the js-runner contract requires a note containing both 'has no node_modules' and 'web'; the refused-dispute contract also requires dispute_count to stay 0 after the refusal. *(Contract surface leaves the exact expected package list and note wording to the contract writer)*
- wiring and wiring-test dispute briefings reuse criterion.rs's verdict rules, run reading, schema and evidence sections verbatim, defining 'the criterion' as the disputed entry, instead of parameterizing the nouns *(the acceptance briefing is pinned byte for byte by tests_golden.rs; sharing the text unchanged keeps it identical and avoids a second copy)*
- ledger the frozen contract fn tests/completion_gates_contracts.rs wiring_dispute_indexes_the_wiring_list at 51 lines instead of editing it *(the contract session froze a 51-line test fn; frozen files cannot change, so cargo test --test maintainability needs a ledger line; the single TI-ratchet event's dispute names both changes)*
- dispute the round-1 major finding on format_gate (non-format failures refuse the freeze) instead of changing it *(the G4 brief settles it: 'Each failed or timed-out criterion gives the problem' and 'A formatter check that already fails at the stage's base blocks every freeze; plans keep formatter commands green at base'; the frozen format_problems signature and the wiring check format_problems( in freeze.rs leave no room for a contract-file filter)*

#### Notes

- state: all work written and the full gate plus loom check are green (uncommitted). F-1-2 and F-1-3 are fixed (legend.rs, launch.rs, 4 doc comments, web statuses.json fixture and web/dist index.js literal) and await round 2. F-1-1 is disputed. Remaining: round-2 review, commit per concern, one dispute-integrity for TI-ratchet (the ledger now removes handle_dispute_criteria and adds the frozen 51-line contract fn), then loom stage complete
- found: the status legend text is mirrored in web/src/api/fixtures/statuses.json (checked by commands::status::web::model::tests::statuses_fixture_matches_stage_status) and inlined into tracked web/dist/assets/index.js; a legend change edits all three
- mutation: freeze-refuses-unformatted-contract-files red under format_problems dropping every failed result
- mutation: refused-dispute-writes-no-request red under refusal() skipping transition_refusal
- mutation: wiring-tests-verdict-must-amend-wiring-tests red under admits() returning true
- mutation: wiring-dispute-indexes-the-wiring-list red under the Wiring range check using stage.acceptance.len()
- mutation: fixture-directories-hold-no-packages red under removing fixtures from scan.rs SKIP_DIRS
- mutation: js-runner-without-node-modules-is-a-note red under missing_node_modules(..) replaced by false in run_group
- mutation: impact-selection-uses-the-stage-merge-base red under changes_since_merge_base diffing from rev-parse HEAD instead of merge-base HEAD main
- found: cancel_signals_a_holder_that_is_the_reconciler flaked ~50%: fake_reconciler returned before the child exec'd sh, so /proc/<pid>/cmdline still held the test binary's argv and is_reconciler said no. Fix: fake_reconciler polls cancel::process_argv (now pub(super)) until the argv holds 'hook'

### Plan environment: provision and plan-time checks (plan-environment)

**Status:** completed  
**Purpose:** Implement stage 2 of doc/plans/PLAN-stage-exits-and-environment.md ("plan-environment", and "Choices this plan settles" 8-12, 17 and 20).
Use parallel subagents and skills to maximize performance.
Every worker first reads doc/plans/briefs/stage-exits-and-environment/common.md, then its own brief.
Spawn every worker BY AGENT TYPE with the fixed prompt plus "Your brief: <path>. Read it in full before anything else."
Territories are DISJOINT. Workers NEVER spawn subagents.
Waves: P1 and P3 in ONE message; then P2 (its JS lint reads the provision field P1 adds); then the main agent runs cargo fmt --all once and the acceptance commands.

| Worker | Role | Tier | Files owned | Shared context | Brief path |
| ------ | ---- | ---- | ----------- | -------------- | ---------- |
| P1 | Provision schema, executor and spawn gate | sonnet | src/plan/schema/types.rs; src/plan/schema/types_v2.rs; src/plan/schema/mod.rs; src/plan/schema/validation/v2_fields.rs; src/plan/schema/tests/v2_tests.rs; src/orchestrator/provision.rs; src/orchestrator/provision_tests.rs; src/orchestrator/mod.rs; src/orchestrator/core/provision_gate.rs; src/orchestrator/core/provision_gate_tests.rs; src/orchestrator/core/mod.rs; src/orchestrator/core/stage_executor.rs; src/commands/init/plan_setup.rs | common.md | doc/plans/briefs/stage-exits-and-environment/plan-environment/p1-provision.md |
| P2 | Environment lints | sonnet | src/plan/schema/validation/v2_lints/mod.rs; src/plan/schema/validation/v2_lints/registry_domains.rs; src/plan/schema/validation/v2_lints/repo_hooks.rs; src/plan/schema/validation/v2_lints/js_provision.rs; src/plan/schema/validation/v2_lints/sandbox_capability.rs; src/plan/schema/tests/mod.rs; src/plan/schema/tests/v2_lint_environment_tests.rs; src/commands/plan/verify.rs; tests/fixtures/plans/v2-environment-lints.md; tests/fixtures/plans/v2-valid.md (header paragraph only) | common.md; P1's ProvisionEntry | doc/plans/briefs/stage-exits-and-environment/plan-environment/p2-environment-lints.md |
| P3 | Plan-writer skill | sonnet | ../skills/loom-plan-writer/SKILL.md; ../skills/loom-plan-writer/references/sandbox.md; ../skills/loom-plan-writer/references/v2-contracts.md; ../skills/loom-plan-writer/references/authoring-detail.md; ../skills/loom-plan-writer/references/grounding-protocols.md | common.md; BLOCK-F text | doc/plans/briefs/stage-exits-and-environment/plan-environment/p3-plan-writer-skill.md |

CONTRACT SURFACE (the contract session writes tests/plan_environment_contracts.rs from this, before any code; top-level #[test] fns):

- loom::plan::schema::ProvisionEntry { working_dir: String, command: String } (serde, deny_unknown_fields); loom::plan::schema::LoomConfig gains provision: Vec<ProvisionEntry> (YAML key loom.provision).
- loom::orchestrator::provision::run_provision(entries: &[ProvisionEntry], worktree: &Path) -> Result<(), String>: runs each command with sh -c in worktree.join(working_dir), in order, stopping at the first failure. Err is the stage's block reason: "provision `<command>` in `<working_dir>` failed: <detail>", where detail is the last 10 non-blank stderr lines joined by newlines (or "exit <code>" when stderr is empty). A working_dir that resolves outside the worktree (a symlink) is an Err and runs nothing.
- loom::plan::schema::validate(&LoomMetadata) -> Result<(), Vec<ValidationError>> (ValidationError has message: String); LoomMetadata parses from YAML text with serde_yaml. A provision working_dir that is absolute or contains `..` is an error whose message contains "provision".
- loom plan verify --json <plan>: spawn the binary ONLY through helpers::loom_cmd(), declared as #[path = "integration/helpers.rs"] #[allow(dead_code)] mod helpers; exactly as tests/map_cli.rs does, with args ["plan", "verify", "--json", "PLAN.md"] and current_dir set to the TempDir git repo that holds PLAN.md (a RELATIVE plan path). tests/integration/binary_spawn_guard.rs fails any other test file that writes Command::new(env!("CARGO_BIN_EXE_loom")), and an unscrubbed spawn inherits the running stage's LOOM_* identity and the real ~/.loom update state; stdout is one JSON object whose "errors" is an array of {"stage_id": string|null, "message": string}; the exit status is 1 when errors is non-empty. For the plan text, copy the YAML of tests/fixtures/plans/v2-valid.md and add a summary: line to every stage (the fixture has none), wrapped in the <!-- loom METADATA --> markers and a yaml code fence as that file is. A sandbox block goes under loom: as sandbox: { network: { allowed_domains: [...] } }; provision goes under loom: as provision: [{ working_dir: web, command: "bun install --frozen-lockfile" }].
- Lint message fragments the contracts may match: a JS package lint names the package in backticks and the word "provision" (for web: "`web`"); a registry lint names the missing domain ("registry.npmjs.org"); a hook lint names "pre-commit hook" and the missing domain. The registry lint's message also contains the word "provision", so a check for the JS lint matches "package `web`", never "provision" alone.
- Git in tests, and the loom binary the tests run, get GIT_CONFIG_GLOBAL and GIT_CONFIG_SYSTEM pointed at missing files and GIT_CONFIG_NOSYSTEM=1, as src/verify/impact_tests_tests.rs does, so a user's global core.hooksPath never leaks in; a repo-local core.hooksPath is set with git config inside the TempDir repo. The hook lint reads core.hooksPath with scoped git config reads (--local, --global, --system), never rev-parse --git-path hooks: loom's git runner forces -c core.hooksPath=/dev/null on every command.
- loom::orchestrator::provision::{persist_plan_snapshots, write_provision_snapshot, read_provision_snapshot}: loom init persists the plan's provision entries into the work directory's config.toml ([plan_provision]); the spawn gate reads only that snapshot, never the plan file.

HAZARD (installed binary): this plan runs on the loom binary installed before it (built from main after PLAN-source-graph-mechanism), which still selects impact tests from the nearest published base layer and rejects a plan-level provision key. If loom stage complete's impact-selected step picks web/ vitest files, run cd "$(git rev-parse --show-toplevel)/web" && bun install --frozen-lockfile in this worktree (a bare cd web fails from loom/; registry.npmjs.org is allowed; the bun cache is pre-granted), then run loom stage complete again.
EXPECTED INTEGRITY EVENTS: none. Never edit loom/maintainability-baseline.txt in this stage: plan/schema/types.rs (416 lines), stage_executor.rs (597; start_stage 149, before_stage_gate_passed 62), commands/plan/verify.rs (555) and commands/init/plan_setup.rs initialize_with_plan (206) stay net-zero, fs/work_dir.rs (433) is not edited, and a new file or function stays under its limit.
CONTRACTS: before the final review round, prove each contract red by mutation (Gate conventions) and record loom memory note "mutation: <id> red under <mutation>".
MEMORY: record mistakes, decisions and surprises via loom memory immediately (subagents too); never loom knowledge in this stage; never Claude Code auto-memory.

#### Files Changed

No changes recorded.

#### Key Decisions

- contracts: CLI lint tests assert only on message fragments, never on an empty errors array, because the v2-valid plan's other lints may legitimately report; the JS-lint clear check in the second run excludes messages naming registry.npmjs.org, since the registry lint also says provision and `web`. provision_working_dir_cannot_escape adds a positive control (working_dir web gives no provision error) so an always-erroring check fails.
- ledgered tests/plan_environment_contracts.rs plan 64 in maintainability-baseline.txt despite the signal's 'never edit the ledger' *(the frozen contract file's plan() helper is 64 lines, so cargo test --test maintainability cannot pass otherwise; the contract file cannot be edited; precedent aa5088c7 ledgered language_packs_contracts.rs the same way. The TI-ratchet event is disputed once after the final review round per common.md)*

#### Notes

- wiring: loom/src/orchestrator/provision_tests.rs and loom/src/orchestrator/core/provision_gate_tests.rs are wired as #[cfg(test)] #[path = "..."] mod tests; in their parent modules; the unwired-file check does not follow #[path] declarations, so it reported them as unwired
- found: loom subagents watch exited 6 (stalled) for a worker that had already handed back via SubagentHandback and completed ~15 min earlier; the watch did not take the handback as terminal evidence and kept waiting on the sibling, then judged the finished one hung after 902s
- mutation: pre-commit-hook-registry-need-is-an-error red under dropping the repo_hooks::check call
- mutation: registry-tool-without-registry-domain-is-an-error red under dropping the registry_domains::check call
- mutation: js-package-without-provision-is-an-error red under dropping the js_provision::check call
- mutation: provision-working-dir-cannot-escape red under dropping the push_provision_problems call
- mutation: provision-refuses-a-symlinked-escape red under replacing dir.starts_with(&root) with true
- mutation: failed-provision-names-command-and-dir red under the reason saying 'broke:' instead of 'failed:'
- mutation: provision-runs-in-its-working-dir red under run_provision joining '.' instead of entry.working_dir
- gotcha: the signal's Acceptance Criteria list drops a criterion's exit_code, so the two negative rg criteria (exit_code: 1) read as must-match; the plan YAML is authoritative

### Stage exits: block lifecycle, attention and doctrine (stage-exits)

**Status:** completed  
**Purpose:** Implement stage 3 of doc/plans/PLAN-stage-exits-and-environment.md ("stage-exits", and "Choices this plan settles" 13-15, 18, 19 and 21).
Use parallel subagents and skills to maximize performance.
Every worker first reads doc/plans/briefs/stage-exits-and-environment/common.md, then its own brief.
Spawn every worker BY AGENT TYPE with the fixed prompt plus "Your brief: <path>. Read it in full before anything else."
Territories are DISJOINT. Workers NEVER spawn subagents. S1, S2 and S3 go in ONE message; then the main agent runs cargo fmt --all once and the acceptance commands. The three share one crate mid-wave: a worker's check can fail on another's unfinished file, and each worker reports such errors instead of fixing them.

| Worker | Role | Tier | Files owned | Shared context | Brief path |
| ------ | ---- | ---- | ----------- | -------------- | ---------- |
| S1 | Block lifecycle | opus | src/orchestrator/core/event_handler.rs; src/orchestrator/core/event_handler/stage_takedown.rs; src/orchestrator/core/event_handler/blocked.rs; src/orchestrator/core/event_handler/verdict_retirement_tests.rs; src/orchestrator/monitor/session_events.rs; src/orchestrator/monitor/session_events/tests.rs; src/daemon/server/control_block.rs; src/commands/stage/state_relay.rs; src/commands/stage/human_review.rs | common.md | doc/plans/briefs/stage-exits-and-environment/stage-exits/s1-block-lifecycle.md |
| S2 | Block reason in status | sonnet | src/commands/status/data/mod.rs; src/commands/status/data/collector.rs; src/commands/status/data/sanitize.rs; src/commands/status/web/model.rs; src/commands/status/web/model_tests_stages.rs; src/commands/status/render/attention_model.rs; src/commands/status/render/attention_model_guidance_tests.rs; src/commands/status/render/attention_model_tests.rs; src/commands/status/render/attention_tests.rs; src/commands/status/render/graph_tests.rs; src/commands/status/ui/tui/ledger/tests.rs; src/commands/status/ui/tui/ledger/rows.rs; src/commands/status/ui/tui/state_tests.rs; src/daemon/wire_tests.rs | common.md | doc/plans/briefs/stage-exits-and-environment/stage-exits/s2-block-attention.md |
| S3 | BLOCK-F and agreeing surfaces | sonnet | src/orchestrator/signals/helpers.rs; src/orchestrator/signals/tests_doctrine_v2.rs; src/orchestrator/signals/tests_doctrine_blocks.rs; src/orchestrator/signals/tests_size.rs; src/orchestrator/signals/v2_section_review.rs; src/orchestrator/signals/format/sandbox_section.rs; src/orchestrator/signals/format/helpers.rs; src/commands/stage/acceptance_runner.rs; src/commands/stage/complete_verification.rs; src/commands/stage/dispute_transport.rs; src/commands/stage/dispute_criteria_tests.rs; ../CLAUDE.md.template; ../loom-hooks/commit-guard.sh; ../loom-hooks/_subagent-preamble.txt; ../skills/loom-orchestration/SKILL.md; ../skills/loom-usage/SKILL.md | common.md; BLOCK-F text | doc/plans/briefs/stage-exits-and-environment/stage-exits/s3-stage-exit-doctrine.md |

BLOCK-F, verbatim (S3 places it; the plan's prose and every surface use these exact bytes):
**When the stage cannot finish.** Fix what the stage can fix. A criterion, wiring check, contract, review finding or test-integrity event that is wrong gets a dispute: `loom stage dispute-criteria` (with `--field` for wiring and wiring-tests entries), `dispute-contract`, `dispute-findings` or `dispute-integrity`. Filing ends this session by design: the daemon starts a fresh session with the verdict, so waiting gains nothing. Never revert, weaken or postpone correct work to avoid a dispute. A need only a person can meet (a credential, a host install, a network domain or path the plan does not grant) gets `loom stage block <stage-id> "<what is needed and why>"`: the daemon retires the session and shows the reason to the operator. Commit your work before filing either. Never end a turn asking the operator to act while the stage is executing.

CONTRACT SURFACE (the contract session writes tests/stage_exits_contracts.rs from this, before any code; top-level #[test] fns):

- loom::commands::status::data::StageSummary (every field pub; build it as src/commands/status/render/attention_model_tests.rs make_stage_summary does) gains close_reason: Option<String>, serialized under the key "close_reason" when Some.
- loom::commands::status::render::attention_entries(&[StageSummary]) -> Vec<AttentionEntry { id, label, command: Option<String>, note: Option<String>, automatic: bool, .. }>.
- A Blocked stage with failure_info None and close_reason Some(r) has one entry: label "BLOCKED", note Some("blocked: " + r) (the same note whether the stage agent, the operator's loom stage block or human-review --reject set the reason), command Some("loom stage retry <id>") while retry_count < max_retries (default 3), automatic false.
- loom::models::failure::{FailureInfo { failure_type, detected_at, evidence }, FailureType::SessionCrash}.

HAZARD (installed binary): this plan runs on the loom binary installed before it (built from main after PLAN-source-graph-mechanism), which still selects impact tests from the nearest published base layer. This stage branches after completion-gates merges, so that base can predate its branch point by a whole stage's work, and completion-gates' fix is not in the running binary. If loom stage complete's impact-selected step picks web/ vitest files, run cd "$(git rev-parse --show-toplevel)/web" && bun install --frozen-lockfile in this worktree (a bare cd web fails from loom/; registry.npmjs.org is allowed; the bun cache is pre-granted), then run loom stage complete again.
EXPECTED INTEGRITY EVENTS: none. handle_one_event stays at 116 lines; cache.rs is not edited; no existing assertion line in a test file changes (the sandbox_section.rs assertion S3 rewrites sits in a production file's inline test module, which the integrity profile does not count; so do S1's new control_block.rs test and S3's format/helpers.rs and acceptance_runner.rs tests).
CONTRACTS: before the final review round, prove each contract red by mutation (Gate conventions) and record loom memory note "mutation: <id> red under <mutation>".
MEMORY: record mistakes, decisions and surprises via loom memory immediately (subagents too); never loom knowledge in this stage; never Claude Code auto-memory.

#### Files Changed

No changes recorded.

#### Key Decisions

- take_down_agents drops the active_sessions entry only when the tracked session was among the agents it took down *(retirable_agents now filters Merge sessions; a block of a MergeConflict stage would otherwise drop the daemon's handle on the live resolver it spared (loom/src/orchestrator/core/event_handler/stage_takedown.rs take_down_agents). No-op for the handoff and dispute callers, whose tracked session is always in the list.)*

#### Notes

- mistake: the frozen contract tests/stage_exits_contracts.rs is not rustfmt-clean (two assert_eq! lines at 85 and 110 wrap under rustfmt), so cargo fmt --all and the pre-commit formatter rewrote it and loom stage complete failed the frozen-hash check; restoring it fails the stage criterion cargo fmt --all -- --check. Why: the contract session ran on the installed binary, which predates completion-gates' freeze-time formatter gate. Prevention: before running cargo fmt --all in a stage with contracts, check the frozen files with rustfmt --check; a frozen file that fails the stage's formatter is a dispute-contract at once
- gotcha: commands::status::web::terminal::tests_pty::pty_child_shutdown_reaps_the_child asserts a sub-1s reap and failed once under the full commands::status:: run while other cargo jobs ran; it passes 3/3 in isolation and this stage does not touch the terminal module
- mistake: S2 added close_reason to build_stage_summary, taking it to 51 lines over the 50-line ledger limit; this stage may not edit the ledger. Fix: folded now into the facts tuple. Prevention: briefs adding a StageSummary field must name build_stage_summary's line budget
- found: the stage-exits signal rendered the criterion rg -q -F 'dispute-criteria <stage-id> "criteria X' without its exit_code: 1, so it reads as a presence check; the stage YAML expects absence. Prevention: check the stage file under .loom/work/stages before treating a signal criterion as failing
- mutation: crash-blocked-stage-is-not-an-agent-block red under a (_, Some(reason)) arm placed before the auto-retry arm
- mutation: agent-block-reason-reaches-attention-over-the-wire red under removing the (None, Some(reason)) arm of blocked_guidance
- gotcha: the loom-control-complete PreToolUse hook refused a Bash heredoc append whose text contained 'loom stage retry' (could not be tokenized safely); appending the same text with the Edit tool worked.

### Integration verification (integration-verify)

**Status:** completed  
**Purpose:** Final verification of doc/plans/PLAN-stage-exits-and-environment.md. Verify FUNCTIONAL INTEGRATION, not just tests passing. NEVER Claude Code auto-memory.
Use parallel subagents and skills to maximize performance.
CONTEXT: read the plan, loom memory show --all, and the knowledge sections the brief quotes.
HAZARD (installed binary): this stage runs on the loom binary installed before the plan. Integration-verify runs no impact-selected tests, and the full Rust suite needs no web/node_modules (web/dist is tracked and embedded), so no install step is needed here.
BUILD AND TEST (zero tolerance; fix every warning and failure through an engineer subagent, sonnet or opus by the rubric): the canonical gate of loom/.githooks/pre-push: the full suite, clippy with warnings denied, fmt, rustdoc with warnings denied, the markdown lint (read-only; fix what it reports, bunx markdownlint-cli2 --fix repairs most of it), and scripts/flake-check.sh with its four default filters (cargo audit --no-fetch runs in completion-gates, whose working_dir holds Cargo.toml; no stage changes Cargo.lock).
CODE REVIEW: spawn parallel loom-code-reviewer subagents, each recording a loom-review block:

- security: loom/src/orchestrator/provision.rs runs plan-authored commands on the host (confinement, working_dir canonicalisation, symlink refusal, the reason's size, and that the spawn gate reads only the loom init snapshot in the work directory, never the plan file a stage can edit); the pre-commit hook reader (bounded read, the scoped git config reads); close_reason flattening before terminal output; the dispute field and index from the wire; that no subagent surface tells a subagent to run loom stage block or a dispute.
- architecture: the block retirement against the takedown invariants (no second writer, session cleared only while Blocked, adjudication and merge-resolution sessions untouched, every block writer clearing failure_info); the gate order (before-stage checks, then provisioning) and the tick stamped while provisioning; the impact selection set against the fingerprint helper both now share; the lints' repository root.
- test coverage: every contract, and two stages' "mutation: <id> red" memories spot-checked by re-applying a rejects implementation and confirming the contract fails.
Fix or dispute every finding; never defer one.
SUGGESTIONS: weigh every pending reviewer suggestion the signal lists; resolve each implemented one with loom memory resolve <id> --outcome implemented --reason <what changed>.
FUNCTIONAL: every stage's wiring checks re-run on the merged tree; the wiring tests below drive the built binary: dispute-criteria --help lists --field, plan verify reports all three environment lints on the fixture plan, and project detect lists no fixture package.
Run every acceptance command once in-session before loom stage complete (each has a 300 s cap).
Record discoveries to loom memory for knowledge-distill, including every knowledge file the tree now contradicts: loom memory note "stale-knowledge: <file>#<heading> claims X; the tree does Y".

#### Files Changed

No changes recorded.

#### Key Decisions

- provision .npmrc refusal counts only when 'test ! -e .npmrc' and 'test ! -L .npmrc' are the first two commands and every top-level operator is && (or a grouping paren; a newline after && or at the end is allowed) *(an argv-sequence check alone accepts 'test ! -e .npmrc; npm ci' or '... || true && ...', where the refusal gates nothing; an AND-only chain makes the first two leaves gate every later one)*
- shared declares_dependencies lives in skills::project::probe and reads package.json through safe_read anchored at the checkout root *(probe.rs's module rule is that every read is root-anchored and refuses symlinks; impact_tests/runs.rs now treats a symlinked or >256 KiB package.json as declaring dependencies (the conservative side). Needed a one-line pub(crate) re-export in src/skills/project.rs, outside the batch-A file table)*
- provision install hardening is a plan validation error (v2_fields/provision_installs.rs), not a lint warning: bun install needs --ignore-scripts --backend=copyfile --config=/dev/null, npm/pnpm/yarn installs --ignore-scripts (pnpm also --ignore-pnpmfile), every JS install the leading '&&'-joined .npmrc refusal with no cd, uv sync --no-install-project; the JS-provision lint suggests exactly these forms (shared constants) and skips dependency-free packages *(IV security review F-1-1: plan Choice 20's 'bun install --frozen-lockfile' let an agent-written postinstall run on the host on the next retry/handoff; validation runs in loom plan verify and loom init, so no unhardened entry reaches the init snapshot. Choice 20's text in the plan is superseded by sandbox.md's hardened form)*

#### Notes

- concern: provision install validation (v2_fields/provision_installs.rs) does not see through eval: 'refusal && eval "cd web" && bun install ...' changes directory unflagged, and 'eval "bun install"' escapes install detection; same indirection class as env -C, npm --prefix, bun --cwd, pnpm -C, corepack pnpm
- found/gotcha: the .npmrc refusal in provision installs checked only the entry's working dir; a cd/pushd/popd (also inside sh -c, or via builtin/command wrappers) moved the install elsewhere. changes_directory() now walks visit_argvs (nested scripts, wrapper-stripped argv) instead of the top-level simple_commands only
- verified mutation spot-check of IV-added tests (scratch copy of the fixed tree): provision_gate::tests::provisioning_stamps_the_daemon_tick red when run_with_fresh_tick calls run_provision without spawning keep_tick_fresh (provision_gate_tests.rs:238); fingerprint::tests::a_commit_on_the_target_after_the_branch_point_is_not_a_stage_change red under rev-parse <target> instead of merge-base HEAD <target> (fingerprint_tests.rs:169)
- concern: provision install detection takes the first non-flag word as the subcommand, so 'npm --prefix web ci', 'bun --cwd web install', 'env -C web bun install' (command_start stops at the -C value) and 'corepack pnpm install' are not checked; and a 'cd sub &&' after the .npmrc refusal tests the wrong directory. Plan authors are trusted, so these are author-mistake gaps, not stage-agent bypasses.
- concern: provision installs read host caches (~/.bun/install/cache, ~/.npm, pnpm store) that an earlier host-side run may have poisoned; --backend=copyfile only stops writes back into the bun cache.
- concern: 'uv sync --frozen --no-install-project' still builds sdist dependencies through their build backends on the host, and reads agent-writable uv.toml / pyproject [tool.uv] index settings (a registry redirect like .npmrc). Validation requires only --no-install-project.
- concern: provision install hardening leaves yarn config channels open: yarn classic reads .yarnrc 'yarn-path' and yarn berry reads .yarnrc.yml 'yarnPath', both run a repository JS file on the host even with --ignore-scripts; berry also rejects --frozen-lockfile (wants --immutable). Validation only checks --ignore-scripts and the .npmrc refusal for yarn.
- concern: three enum mirrors of the dispute field remain (cli/types_stage_disputes.rs DisputeField, AmendField, models/dispute.rs CriterionField), and dispute_criteria_with_mode takes 9 arguments under allow(too_many_arguments); CriterionField keeps Default because DisputeKind::criterion(Default::default(), 0) is used by three tests (inbox_drain/test_support.rs:133, verdict_apply_tests.rs:84, signals/adjudication.rs:86)
- found/gotcha: CriterionField keeps derive(Default)+#[default]: DisputeKind::criterion(Default::default(), 0) in test code at inbox_drain/test_support.rs:133, core/verdict_apply_tests.rs:84, signals/adjudication.rs:86 needs it. Removing it breaks the lib-test build
- verified mutation spot-check (git archive of 5df4b03e, separate target dir): provision-refuses-a-symlinked-escape red under 'if dir.starts_with(&root)' -> 'if true' (panics plan_environment_contracts.rs:78); agent-block-reason-reaches-attention-over-the-wire red with the (None, Some(reason)) arm of blocked_guidance removed (stage_exits_contracts.rs:93); wiring-dispute-indexes-the-wiring-list red under CriterionField::Wiring => stage.acceptance.len() (completion_gates_contracts.rs:244); all three green at baseline

## Open Questions

No open questions.
