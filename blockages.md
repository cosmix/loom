# Loom blockages

## Plan 09: contract fixture formatting

- Symptom: chat-server contract freeze was refused because the oxfmt acceptance check failed on review-restore.json, review-compare.json and review-suggestion.json.
- Recovery: Loom automatically filed freeze-refusal dispute 1 and started adjudication session session-c9c72418-1793218269. Dispute 1 received a verdict, the judge closed, and Loom restarted the contract session. A fresh freeze request was queued and chat-server returned to Executing without operator bypass or edits to orchestration state.
- Outcome: resolved. chat-server, chat-web, integration-verify and knowledge-distill all completed and merged without further intervention. The integration-verify session ended at about 359k context and wrote three handoff files in two minutes (handoff-001 to 003, all from session-fcb69c68). One successor session resumed from them, so this was a single handoff and not a loop.

## Plan 08: dynamic guidance dispatch and source-graph reachability

- Effective blocker: an invalid or unsupported `reachable` assertion blocks stage completion, and integration-verify rechecks completed predecessors' assertions, so it blocks downstream integration completion too. Accepting the limitation in retry context does not waive the automated gate. Installed Loom cannot amend or dispute this field, leaving no targeted CLI recovery for the bad check; in this run the user explicitly skipped verification to advance. Future Loom work should provide audited correction/dispute support for `reachable` so a bad assertion does not require a broad verification waiver.
- Takeover recurrence: integration-verify is blocked on the same guidance-engine `reachable` check, reporting all other checks passed at `9e8f36e3`. Installed PATH Loom `1.3.2-dev.194+8c1011a8` exposes neither `reachable` in `stage amend` nor in `stage dispute-criteria` (confirmed from both help outputs). The earlier guidance-engine override does not resolve integration verification of that check.
- Failed recovery: retry context alone cannot resolve this gate. `complete_verification_v2::reverify_reachable` reruns completed stages' reachability assertions during integration completion. The operator retry restarted the agent but did not remove this automated failure.
- Recovery: the user personally ran `loom stage complete integration-verify --no-verify`, which exited 0 and reported completion with verification skipped. This is an explicit verification waiver, not a passing reachability result. Confirm daemon merge before progressing. Updating or reinstalling Loom is not part of this recovery; future work may extend audited amendment support or trait-object graph resolution.
- Prevention for future Loom iterations: represent calls through trait objects with executable wiring coverage instead of an unsupported static reachability assertion. Extend audited amend/dispute commands to cover every goal-backward field, including `reachable`, so a worker can resolve an impossible check without blocking for operator override. Block only when a genuine external prerequisite or unresolved correctness issue requires it.
- Symptom: guidance-engine blocked after tests/review because the goal-backward reachability check cannot resolve the trait-object call from `provider.authoring` to `render_session_guidance`.
- Diagnosis: source inspection confirms routes install PlanGuidance in GuidanceSlot; chat dispatch calls provider.authoring; PlanGuidance::authoring calls render_session_guidance. Installed Loom cannot amend/dispute `reachable` (those CLI enums omit it).
- Recovery: user completed guidance-engine with `--no-verify --force-unsafe`, recording tip `d1894141`. Normal daemon merge `4584da48` landed it; status reports merged.
- Operator overlap: my same recovery command arrived after the user's successful completion and redundantly force-completed an already completed stage. Confirmed the merge and committed tip survived. Prevention: recheck state immediately before any completion mutation when the user is operating concurrently.

## Plan 07: host-only session bridge verification

- User decision: no further direct unsandboxed host tests. The operator e2e requirement is waived; retain failed/unverified status and use confined verification only. Defer live tmux testing and the unresolved submission diagnosis to a separate follow-up after merge; do not hold integration-verify for it. Complete and merge using required confined checks. This applies through plans 08–11.
- Completion: user personally ran `loom stage complete integration-verify --no-verify`; All four plan 07 stages subsequently completed and merged; `loom clean --all` finished. This is an explicit verification waiver, not a passing end-to-end result.

- Symptom: integration-verify blocked after its automated checks and review, requesting the Claude Code end-to-end and real-tmux tests outside the worker sandbox.
- Cause: these tests require host Unix sockets, network access and a logged-in Claude Code; skips are failures with `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`.
- Host result: reviewed integration-verify tree, Claude Code 2.1.312 and tmux 3.6: end-to-end suite failed (1 passed, 1 failed, zero skips). The authoring session emitted Started but timed out waiting for Idle; recovery passed. Interrupt Stop, /compact order and session TMPDIR were not reached. Log: `/tmp/loom-plan07-host-e2e.log`.
- Host lib results: plan_session:: passed 462/462; orchestrator::terminal::tmux:: passed 76/76, zero skips. Logs: `/tmp/loom-plan07-host-session.log`, `/tmp/loom-plan07-host-tmux.log`.
- Recovery: recorded all results in integration-verify memory and retried with the end-to-end timeout as a finding to diagnose and correct.
- Diagnosed cause: Claude Code's folder-trust dialog held the session in Starting. Stage fix `e9f3fdc1` trusts the project before launch and adds pane diagnostics on Idle timeout. Host rerun on `e9f3fdc1` reached Idle, confirming the trust fix, then failed waiting for the queue to drain (1 passed, 1 failed, zero skips). Recovery passed. Log: `/tmp/loom-plan07-host-e2e-r2.log`. Returned this new failure to integration-verify.
- Next recovery: stage added StopFailure handling and hardened per-round test waits, permission handling and failure diagnostics (`a7d644c2`). Third host run failed (1 passed, 1 failed, zero skips): session reached Idle but timed out after 120s waiting for the opening's delivery. Saved pane shows the opening text still in the input box, zero tokens; report shows Undelivered and no UserPromptSubmit. Submission failure is established; exact cause remains unresolved. Log: `.worktrees/integration-verify/loom/target/plan07-host-e2e-r3.log`; diagnostics: `loom/target/tmp/plan-session-e2e/1793208662-the-opening-s-delivery` in that worktree (removed by completed-plan cleanup; the observed failure is preserved here and in distilled knowledge).

## Plan 06 to 07: concurrent operator-checkout edits

- Symptom: the cleanup-only commit's pre-commit gate failed while another agent was editing `loom/src/commands/knowledge/eval.rs`: duplicate `refresh_source_graph`, mismatched arguments and a `?` on `()` prevented compilation.
- Cause: the repository hook checks the live working tree even when `git commit --only` limits the commit's paths. The edits are unfinished concurrent work, not plan 06 changes.
- Recovery: the other agent's fixes and plan 06 cleanup are committed (cleanup `d2c9dc2c`). The operator installed dev.177 and explicitly resumed plans 07–11. The tracked checkout is ready once this monitoring note is committed; unrelated untracked files remain preserved.

## Plan 06: real-host confinement verification

- Symptom: integration-verify intentionally blocked after its ordinary gates, asking for `LOOM_TEST_REQUIRE_SANDBOX_FREE=1 cargo test --manifest-path loom/Cargo.toml --test plan_session_confinement -- --nocapture` outside the worker sandbox.
- Cause: the confinement probes require real host sandbox facilities; the nested worker sandbox cannot establish the same environment. This is a planned operator gate.
- Host result: Linux, srt `0.0.78`, 11 passed and 3 failed. `map_prints_an_outline_with_the_cache_write_denied` cannot create `.loom/cache/context-v1`; `summary_seal_writes_the_summary_with_the_session_environment` refuses the writable planning-root parent; `the_planning_root_and_runtime_are_write_denied` permits the planning-root probe write (`LOOM_PROBE_RC=0`). These are real failures, not skipped tests or sandbox-launch failures.
- Recovery: returned all three findings to integration-verify. After its fixes, the same Linux host command passed all 14 tests with srt `0.0.78` (zero skips). The planning-root probe now verifies that writes never reach the host. Recorded the passing result in stage memory and resumed verification. No macOS host is available in this environment.

## Plan 06: missing srt host prerequisite

- Symptom: planning-sandbox's preflight truth check for host-installed `srt` (`@anthropic-ai/sandbox-runtime 0.0.78`) exited 127 before the stage could start.
- Cause: `srt` was initially absent. Installing it globally still failed preflight: Bun's bin symlink pointed into `.bun/install/global/node_modules`, outside the confined runner's exposed read roots. Before-stage checks are confined even though the plan and knowledge described a host check.
- Recovery: installed and warmed pinned `0.0.78`, then replaced only `.bun/bin/srt`'s symlink with a pinned `bun x --bun --no-install @anthropic-ai/sandbox-runtime@0.0.78` shim. Host version and `srt -c true` checks passed. Retried planning-sandbox and corrected the stale knowledge claim about preflight confinement.
- Follow-up: make preflight tool discovery account for Bun global symlink targets, and report a missing or hidden tool distinctly from a failing behavioral check.
- Prevention: confirm the first stage actually starts after `loom run`; report daemon startup separately from preflight success.

## Plan 05: test-integrity completion gate

- Symptom: concept-web passed host checks but completion rejected changes to assertions in `file-mode.test.tsx` and `tabs.test.ts` (fingerprint `bae5247c132c`).
- Recovery: the stage handled the integrity rejection and subsequently completed and merged. No operator override was applied by this monitor.
- Follow-up: inspect the integrity event and dispute records before changing detection; passing tests alone do not establish assertion integrity.

## Plan 05: adjudicator startup failure

- Symptom: integration-verify reported `SandboxSetupFailure` while spawning an adjudicator, naming the installed loom hook scripts. The visible status truncated the underlying error.
- Recovery: a later adjudication session started successfully and the stage resumed. Other runtime maintenance was in progress; the exact repair is not established by this monitor.
- Follow-up: preserve the complete startup error in operator status and keep resolved issues out of current attention.

## Plan 05: generated asset merge conflict

- Symptom: integration-verify could not merge `web/dist/assets/store.css` after the target advanced concurrently.
- Recovery: Loom's merge resolver regenerated dist from merged sources. Its resolution commit `56bf9bf0` reached the target. The stage later appeared completed with an assumed merge after operator recovery.
- Follow-up: inspect why pending-merges output still reports the conflict while the completed stage permits knowledge-distill to run.

## Plan 05: orchestrator stalled in sync

- Symptom: status reported no tick for 187 seconds, stuck in sync, during merge resolution.
- Recovery: the monitor stopped the daemon through `loom stop`. Restart initially refused because another agent had uncommitted tracked work. The operator later resumed orchestration.
- Follow-up: identify the blocking sync operation; make recovery account for a dirty operator checkout without committing another agent's unfinished work.

## Plan 05: target guard after external landing

- Symptom: target guard held `feat-planner-ui` after external commits carried integration-verify's work (`ec0f59e0` to `99c9894f`).
- Recovery: reviewed the commit list and changed paths, then accepted the current target with `loom target accept --to 99c9894f2ffe19f36d90f24f378c38a4c7c5acd5`. A later status showed the target in sync; the stage's earlier merge-blocked explanation remained visible.
- Follow-up: ensure accepting a target reliably reconciles affected merge state and current attention without requiring an unsafe completion override.
