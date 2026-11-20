# C2: the commit relay kind, the daemon handler, the CLI wiring

Stage `daemon-owned-commits`, wave 1, tier opus. Read `../common.md` first (Decisions 1-3 of
`doc/plans/PLAN-open-issues-19-24.md`). Line numbers were read at `ff3fe947`; locate edits by symbol.

## Role and issue

Issue #22: sessions cannot sign, so the daemon commits for them. You add the `commit` relay kind and
payload, the matrix rows, the daemon's own subagent refusal, the handler that maps a session to a
`CommitScope` and calls C1's core, and the CLI surface: `StageCommands::Commit`, the `--wait` flag, the
completions, the `Landing::Conflict` text of a resolved merge, and a completion guard that refuses while
staged changes are uncommitted. U1 writes `commands/stage/commit.rs`, U2 writes `commands/request/status.rs`;
C1 writes the git side; C4 handles signing failures in the merge landing and the plan-completion commit.

## Files owned and files to read

Own exactly (repository-relative): `loom/src/relay/kind.rs`, `loom/src/relay/payload.rs`,
`loom/src/relay/matrix.rs`, `loom/src/relay/tests_matrix.rs`, `loom/src/orchestrator/core/inbox_drain.rs`,
`loom/src/orchestrator/core/inbox_drain/apply.rs`, `loom/src/orchestrator/core/inbox_drain/commit.rs`,
`loom/src/orchestrator/core/inbox_drain/commit_tests.rs`,
`loom/src/orchestrator/core/inbox_drain/tests_matrix.rs`, `loom/src/commands/hook/relay.rs`,
`loom/src/cli/types_stage.rs`, `loom/src/cli/dispatch_stage.rs`, `loom/src/cli/types_ops.rs`,
`loom/src/cli/dispatch.rs`, `loom/src/commands/stage/mod.rs`, `loom/src/completions/dynamic/mod.rs`,
`loom/src/relay/mod.rs`, `loom/src/orchestrator/core/inbox_drain/test_support.rs`,
`loom/src/completions/dynamic/tests/tests_commands.rs`,
`loom/src/orchestrator/core/inbox_drain/merge_resolved.rs`, `loom/src/daemon/server/completion_evidence.rs`,
`loom/src/daemon/server/control_complete_tests.rs`.

Read: `relay/kind.rs:10-90,93-149`; `relay/payload.rs:25-70`; `relay/matrix.rs:35-45,296-312`;
`inbox_drain/apply.rs:30-87,172-185` (`admit`, `apply`, `require_owner`);
`inbox_drain.rs:26-60` (`Settle`, `InboxHost`); `inbox_drain/merge_resolved.rs:25-79` (a Merge session
is not the stage's `session`; it resolves in the stage worktree); `inbox_drain/test_support.rs`
(`fixture`, `Fixture::record/stage/relay/ledger`, `entry_for`);
`verify/contracts/test_support.rs:74-101` (`contract_worktree`, `pinned`); `git/target_guard/record.rs:38,104`
(`knowledge_prefix`, `guarded_refs`) and `attestation.rs:101` (`append_attestation`); `git/merge/lock.rs:25`;
`fs/mod.rs:60` (`parse_base_branch_from_config`); `git/branch/operations.rs:106` (`resolve_target_branch`);
`cli/dispatch_stage.rs:113-158`; `cli/dispatch.rs:162-166`; `cli/types_ops.rs:170-185`;
`completions/dynamic/mod.rs:259-285`; `commands/hook/relay.rs:332-349` (`admit`); `daemon/server/control_block.rs:26-55` (`handle_block_stage`:
a refused transition is `Ok(Response::Error { .. })`, not an `Err`); `daemon/server/completion_evidence.rs:121-149`
(`verify_evidence_bindings`) and `:196-260` (the test fixtures). The retired design is in
`doc/plans/briefs/sandbox-escape-hardening/commit-relay/w1-relay-commit-apply.md` sections 1-3, 6.

## Pinned interfaces

Provides (`../common.md`, "Relay" and the two CLI bullets): `RequestKind::Commit` (wire `commit`, control),
`CommitPayload { message, expected_head, expected_tree }` exported from `loom::relay`, matrix Apply for
`Stage`, `Knowledge`, `Merge` and Refuse for `Contract`, `Adjudication`, `BaseConflict`;
`StageCommands::Commit { stage_id: String, #[arg(short = 'm', long = "message")] message: String }`
dispatched as `commit::execute(stage_id, message)`; `--wait <SECS>` on `RequestCommands::Status`, passed to
U2's `execute(id, session, wait_secs: Option<u64>)`.
Consumes C1 (exact, from C1's brief): `crate::git::stage_commit::{Committer, CommitScope, CommitRequest,
CommitRefusal}` where `CommitRefusal` is `Signing { detail }` or `Refused { reason }` with `Display`;
`Committer::new(&WorktreeGit, &Path)` and `committer.commit_staged(&scope, &request) -> Result<String,
CommitRefusal>`; test helpers `crate::git::signing::tests::{fake_signer, git_in}`.

## Root cause and current behaviour

- `RequestKind` has nine variants (`kind.rs:12-22`); `MATRIX` has 54 rows; `decode_payload`
  (`payload.rs:60`), `apply` (`apply.rs:58`), `inbox_drain/test_support.rs::payload_for` and
  `tests_matrix.rs::classify_applied` each match every kind exhaustively.
- The relay hook alone drops control kinds from subagents (`commands/hook/relay.rs:332-344`); `admit`
  (`apply.rs:39`) never repeats it, so a forged inbox entry from a subagent would be applied.
- `require_owner` (`apply.rs:172`) returns `Result<(), String>`.
- A refused entry settles `Settle::Refused(reason)`; an applied one `Settle::Applied(Some(note))`, and the
  note becomes the ledger row's `reason` (`session_pass.rs:164-166`), which U2 prints.
- `RequestStatus::Applied` carries no data, so U2 reads the ledger for the commit id itself.

## Tasks

1. **Kind, payload, matrix.** `RequestKind::Commit` last in the enum, in `all()` (now `[RequestKind; 10]`),
   `is_control()` and `wire_name` (`commit`); update the two doc comments that say "nine" and "seven".
   `payload.rs`: `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] #[serde(deny_unknown_fields)]
   pub struct CommitPayload { pub message: String, pub expected_head: String, pub expected_tree: String }`,
   `RequestPayload::Commit(CommitPayload)`, and the `decode_payload` arm with `decode(payload, "commit")?`.
   `matrix.rs`: six rows: `Stage`, `Knowledge`, `Merge` Apply; `BaseConflict`, `Adjudication`, `Contract`
   Refuse; extend the `MATRIX` doc comment.
2. **Daemon-side subagent refusal.** In `admit`, before the matrix: `if inbox_entry.agent ==
   AgentRole::Subagent && inbox_entry.kind.is_control()` return `Err(format!("a '{}' request from a subagent
   is never applied: only the session's main agent makes it", inbox_entry.kind))`.
3. **`inbox_drain/apply.rs`**: make `require_owner` `pub(super)` returning `Result<Stage, String>` (existing
   callers keep `if let Err(reason) = ..`); add `RequestPayload::Commit(payload) => { let repo_root =
   host.repo_root().to_path_buf(); commit::apply_commit(&CommitSite { work_dir: &work_dir, repo_root:
   &repo_root, stage_id, record }, &payload) }`. Declare `mod commit;` and `#[cfg(test)] mod commit_tests;` in
   `inbox_drain.rs`. Update the module doc.
4. **`inbox_drain/commit.rs`** (under 400 lines, functions under 50): `pub(super) struct CommitSite<'a> {
   work_dir, repo_root, stage_id, record }` and `pub(super) fn apply_commit(site, payload) -> Settle`. It never
   returns `Err`. By `site.record.session_type`:
   - `Stage`: `require_owner`; refuse unless `stage.status == StageStatus::Executing`; worktree path
     `get_worktree_path(stage.worktree.as_deref().unwrap_or(stage_id), repo_root)`;
     `WorktreeGit::pinned(repo_root, &path)` (an `Err` refuses with "not a registered worktree");
     scope `StageBranch { stage_id }`.
   - `Knowledge`: `require_owner`; Executing; git `WorktreeGit::discovered(repo_root)`; target branch
     `resolve_target_branch(&parse_base_branch_from_config(work_dir)?, repo_root)`; scope `Knowledge {
     target_branch, prefix: PathBuf::from(knowledge_prefix().trim_end_matches('/')) }`; hold
     `MergeLock::acquire(work_dir, Duration::from_secs(10))` for the commit and the attestation (a busy lock
     refuses "the merge lock is busy; run the commit again").
   - `Merge`: no `require_owner` (the merge session is not `stage.session`): refuse unless
     `record.status == SessionStatus::Running`, `record.merge_source_branch.as_deref() ==
     Some(branch_name_for_stage(stage_id).as_str())` and the stage is `MergeConflict` or `MergeBlocked`;
     pinned worktree as for `Stage`; scope `Merge { stage_id }`.
   - any other type: refuse (the matrix already does).
   Then `Committer::new(&git, repo_root).commit_staged(&scope, &request)`:
   - `Ok(id)`: for a Knowledge scope, when `guarded_refs(work_dir)` contains `branch_ref(&target_branch)`,
     `append_attestation(work_dir, &reference, &payload.expected_head, &id)` (a failure is `tracing::warn!`,
     the commit stands, the guard then holds the move for the operator as for any unattested move); settle
     `Applied(Some(format!("committed {id}")))`.
   - `Err(CommitRefusal::Signing { detail })` depends on the scope:
     - `Merge` scope: NEVER call `handle_block_stage`, for three reasons:
       - `MergeBlocked -> Blocked` is not a legal edge (`models/stage/transitions.rs`);
       - a blocked `MergeConflict` stage keeps its resolver alive (`verdict_retirement_tests.rs`);
       - `loom stage retry` would re-run the whole stage rather than the merge.

       Call the new `InboxHost` method `fn hold_merge_for_signing(&mut self, stage_id: &str, detail:
       &str) -> String`, declared in `inbox_drain.rs`. The `Orchestrator` impl there forwards to C4's
       `Orchestrator::hold_merge_for_signing` (`merge_handler/landing.rs`). That function stops the
       resolver through `stop_gated_resolvers` and routes the stage to `NeedsHumanReview` with the
       remedy. `FakeHost` in `test_support.rs` records each call and returns `"held for the
       operator"`. Settle `Refused(format!("signing failed: {detail}; {outcome}"))`, where `outcome`
       is the returned text.
     - `StageBranch` and `Knowledge` scope: block the stage exactly as the Block arm does
       (`apply.rs:113`: `handle_block_stage(work_dir, stage_id, &reason)`), with reason `commit signing
       failed: {detail}; fix the signing setup (gpg-agent passphrase cache, GUI pinentry or ssh-agent
       key), then run loom stage retry {stage_id}`. `handle_block_stage` reports a refused transition as
       `Ok(Response::Error { .. })` as well as `Err`, so BOTH mean the block failed. Settle
       `Refused(format!("signing failed: {detail}; the stage is blocked for the operator"))` only when the
       block applied (`Ok(Response::Ok)`); otherwise settle `Refused(format!("signing failed: {detail};
       the stage could not be blocked: {why}"))` with `why` the error text or the response message, and
       `tracing::warn!` it. A block retires the session, which is the intent: only the operator can fix
       signing. Keep the decision in a small function that takes the block call as a closure
       (`FnOnce(&str) -> Result<Response>`) so a test injects `Ok(Response::Error { .. })` without
       building a stage that cannot be blocked.
   - `Err(CommitRefusal::Refused { reason })`: settle `Refused(reason)`.
   Do not use `refusal::park_refused_relay`: it returns without parking when a contract freeze exists (every v2
   contract stage) and words the reason as a contract freeze refusal (`verify/contracts/refusal.rs:100-180`).
5. **CLI wiring.** `commands/stage/mod.rs`: `pub mod commit;` (U1 writes the file). `cli/types_stage.rs`:
   variant `Commit { #[arg(value_parser = clap_id_validator)] stage_id: String, #[arg(short = 'm', long =
   "message")] message: String }` with a doc comment, after `Merge`; the file stays under 400 lines (376
   now). `cli/dispatch_stage.rs`: `use crate::commands::stage::commit;` and, inside `dispatch_stage`, one
   arm `StageCommands::Commit { stage_id, message } => commit::execute(stage_id, message),` (47 lines, under
   the limit). Run `rg 'StageCommands::Skip' loom/src` and fix any other exhaustive match. `cli/types_ops.rs`:
   `wait: Option<u64>` on `RequestCommands::Status` with `#[arg(long, value_name = "SECS", value_parser =
   clap::value_parser!(u64).range(1..=600))]`. `cli/dispatch.rs`: only `dispatch_request` changes (`{ id,
   session, wait } => request::status::execute(id, session, wait)`); `dispatch` is ledgered and must not grow.
   `completions/dynamic/mod.rs`: the arm becomes `("stage", "complete" | "commit") =>
   complete_stage_ids_filtered(cwd, prefix, &EXECUTING),` with a module-level `const EXECUTING: [&str; 1] =
   ["executing"];` (the arm is 97 columns, under rustfmt's 100; inlining `&["executing"]` would wrap the
   arm and grow `complete_after_subcommand`, ledgered at 56 lines, which must stay at 56 after `cargo fmt`).
6. **`commands/hook/relay.rs`**: read `Request::admit` (`:332`); the control-kind list is built from
   `RequestKind::all()`, so no production change is needed. Edit nothing unless a kind table appears there.
7. **`inbox_drain/merge_resolved.rs`**: the `Landing::Conflict` arm of `settle_for_landing` says
   `merge it into this worktree again, resolve, commit, then rerun --resolved`. A resolver never commits
   through git (`git merge --continue` and a bare `git merge <target>` commit inside the sandbox, where
   signing cannot work, and `commit-filter.sh` does not see either). New text: merge the target again with
   `git merge --no-commit --no-ff {target}`, resolve, stage the resolution, commit through `loom stage
   commit`, then rerun `--resolved`; never `git merge --continue`. No existing assertion reads the old text
   (`rg 'conflicts again' src` finds only the production strings). The `Landing::Held` arm says "the stage
   branch touches a control path", but C4 makes a merge-commit signing failure return `Held` as well: it
   now reads `routed to human review: <the stage's review_reason>` (load the stage; when that fails, name
   both causes, a control path or a merge-commit signing failure). Check `rg 'touches a control path' src`
   for a test that pins the old text before changing it, and add new assertions only on new lines.
8. **Completion guard** in `daemon/server/completion_evidence.rs` (326 lines, stays under 400). The commit is
   applied asynchronously, so a session that runs `loom stage complete` before its commit applied would bind
   the old HEAD, and the handler above would then refuse the late commit (the stage is no longer `Executing`).
   Add `fn refuse_uncommitted_index(stage: &Stage, repo_root: &Path) -> Result<()>`, called LAST in
   `verify_evidence_bindings` (after the `expected_stage_commit` comparison). When
   `crate::git::get_worktree_path(&stage.id, repo_root)` exists, run `diff --cached --quiet
   --ignore-submodules=none HEAD` through `WorktreeGit::pinned(repo_root, &worktree)` (a session can rewrite
   its worktree `.git` file). Exit 1 bails `staged changes are not committed: run loom stage commit and wait
   for it with loom request status <id> --wait 90` (the literal text `<id>`); any other failure, a pinned-handle
   error included, bails with git's stderr. A stage with no worktree at that path (a Knowledge stage commits in
   the main checkout; the trusted-checkpoint fixtures create none) passes through unchanged.

## Tests to write

`relay::matrix::tests` (the file `relay/tests_matrix.rs`, declared inside `matrix.rs` with
`#[path = "tests_matrix.rs"] mod tests;`; the module path is `relay::matrix::tests`, never `relay::tests`):
`commit_is_applied_for_stage_knowledge_and_merge_only` with its own six-row table and `verdict`. Leave
`EXPECTED` and the existing test untouched.
`relay::kind::tests` (`relay/kind.rs`): `commit_is_kebab_case_on_the_wire`; in the existing
`is_control_matches_the_seven_control_kinds` insert one line `RequestKind::Commit,` after
`RequestKind::FileDispute,` and change nothing else (keep the name: renaming is a test-declaration event).
`relay::payload::tests`: `decodes_a_commit_payload_and_refuses_an_unknown_field`.
`orchestrator::core::inbox_drain::tests_matrix`: add `RequestKind::Commit` arms to `classify_applied` (and
`run_cell` if it matches exhaustively); the nine-column table zips with `all()`, so the tenth kind is not
driven there; the commit column is covered below. No existing assert line changes.
`orchestrator::core::inbox_drain::commit_tests` (own helpers inside the file; `contract_worktree` plus a
`git_in(&repo, &["config", "user.name", ..])` identity and `commit.gpgsign=false` set repo-locally, `a.txt`
staged with `git_in(&worktree, ..)`, `write-tree` and `rev-parse HEAD` for the payload, `Stage.worktree =
Some(STAGE)`): `a_stage_commit_is_applied_and_the_ledger_carries_the_commit_id`,
`a_signed_commit_lands_when_gpgsign_is_on`, `a_moved_head_is_refused_with_the_ref_unmoved`,
`a_stage_that_is_not_executing_is_refused`, `a_session_that_does_not_own_the_stage_is_refused`,
`a_contract_session_is_refused_by_the_matrix`, `a_subagent_commit_entry_is_refused_in_admit` (the reason
contains `subagent`; entry built with `entry_for`, `agent` set to `AgentRole::Subagent`, written with
`write_entry`), `a_signing_failure_blocks_the_stage_and_leaves_the_ref` (`fake_signer(&repo, true)`; the
stage is `Blocked`, its block reason contains the signer text, the branch unmoved, the ledger outcome
Refused), `a_merge_signing_failure_holds_for_the_operator` (exact path
`orchestrator::core::inbox_drain::commit_tests::a_merge_signing_failure_holds_for_the_operator`, run
`--exact` by acceptance; once from `MergeConflict` and once from `MergeBlocked`: a Merge session with
MERGE_HEAD set, `fake_signer(&repo, true)`; the ledger outcome is Refused with the signer text, `FakeHost`
recorded exactly one `hold_merge_for_signing` call for the stage, the stage is never `Blocked`, the branch
unmoved), `a_refused_block_is_not_reported_as_a_block` (the signing-failure
settle function with a block closure returning `Ok(Response::Error { message })`, and again `Err(..)`: both
settle a reason containing `could not be blocked` and not `is blocked for the operator`), `a_knowledge_commit_lands_on_the_target_and_is_attested_when_guarded` (write `target-guard.refs`
with `ref refs/heads/main` through `git::target_guard` test support, then read the ledger),
`a_knowledge_commit_outside_the_prefix_is_refused`, `a_merge_session_commit_records_two_parents`,
`a_replayed_commit_entry_is_applied_once`. Read results from `fx.ledger(sid)` taking the LAST row for the id.
`daemon::server::control_complete::tests::completion_refuses_a_staged_but_uncommitted_change` (in
`control_complete_tests.rs`; exact path, run `--exact` by acceptance): build the stage worktree at
`crate::git::get_worktree_path(stage_id, repo_root)` (`git worktree add .worktrees/<stage> loom/<stage>`
on the fixture's repository; the fixture makes the `loom/<stage>` branch and no worktree), stage a change in
it, then complete through the existing `trusted_checkpoint_fixture` and assert the error contains
`staged changes are not committed` and `--wait 90`. The fixture's repository is `fixture.work` two levels up
(`.loom/work`), or add a small `pub(crate)` accessor to `TrustedCheckpointFixture` (the file is yours). The
existing completion tests have no worktree at that path and pass through the guard unchanged.

## Patterns to copy and not copy

Copy `HandoffSite` and the `host.repo_root().to_path_buf()` step in `apply.rs:69-78`; the `Settle` mapping of
`stage_request`; `merge_resolved.rs` for how a Merge session is attributed. Do not copy the Block/Dispute
route's `require_owner` for a Merge session (its stage `session` is another id), and do not run any git
command in the worktree yourself: every git call goes through C1's `Committer`.

## Traps

- Knowledge: "A control ticket stays bound to its own line: one a subagent wrote is refused, and sweeping it
  ... would relay the subagent's block, handoff or verdict under the main agent's authority"
  (`architecture/security-and-isolation.md`, Session Requests Travel Through a Hook-Written Inbox). The
  daemon check in `admit` is the second line; keep both.
- Knowledge: "Inbox Ledger Has Two Rows Per Request Id: any reader of the inbox ledger must take the LATEST
  row for an id" (`mistakes/concurrency-and-locking.md`). Your tests read the last row.
- Knowledge: loom's git runs without hooks, so the `reference-transaction` hook never attests a daemon move
  (`architecture/target-guard.md`, Attestation); you attest, only for a guarded ref.
- Never edit an existing assertion line: the test-integrity gate raises `TI-edit`. The one-line vec insertion
  above and new tests are the only changes to existing test files. Do not rename `is_control_matches_the_seven_control_kinds`.
- Three small edits in your row: `relay/mod.rs` exports `CommitPayload` (U1 imports it from
  `crate::relay`); `inbox_drain/test_support.rs` `payload_for` gains a `RequestKind::Commit` arm
  returning `json!({"message": "test(relay): commit a.txt", "expected_head": "<40 hex>", "expected_tree":
  "<40 hex>"})`; and `completions/dynamic/tests/tests_commands.rs`
  `test_complete_subcommands_stage_prefix`, which asserts exactly one stage subcommand starts with `com`,
  now that `commit` is a second: change that assertion to the two names (`commit`, `complete`). That
  edit raises `TI-edit-loom/src/completions/dynamic/tests/tests_commands.rs`, an expected event the
  orchestrator disputes (event ids are checkout-relative, built from `git ls-tree --full-tree`; the
  orchestrator copies the id from `loom stage review integrity daemon-owned-commits`); touch no other
  assertion line there.
- Doctrine and remedy strings use `--wait 90` (90 s stays under the Bash tool's default 120 s timeout).
- Unowned wording to report, never edit: `commands/stage/merge.rs:90`, `commands/stage/merge_verify.rs:62`
  and `orchestrator/core/merge_handler/spawn_failure.rs:28` also say `resolve, commit` for a merge conflict.

## The one check

`cargo test --lib orchestrator::core::inbox_drain::commit_tests` once, after the crate compiles. C1, C3, C4,
U1 and U2 write in parallel; a compile error in a file you do not own is theirs: report it with file and line.

## Report format

Files changed; the check and its result; each file outside your row that needs an edit; deviations from this
brief and `../common.md` with the reason; ledgered units you shrank.
