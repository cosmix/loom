# C4: signing consumers, the merge landing and the plan-completion commit

Stage `daemon-owned-commits`, wave 1, tier sonnet. Read `../common.md` first (Decision 3 of
`doc/plans/PLAN-open-issues-19-24.md`, and its "Signing failure handling" and "Signing consumers"
bullets). Line numbers were read at `ff3fe947`; locate every edit by symbol.

## Role and issue

Issue #22. C1 makes loom's own merge commit sign (`commit_merge`) and captures the signing environment
into the daemon. Two daemon paths consume that and fail wrongly without you: a signing failure inside the
merge landing is reported as a plain failure, so a resolver is spawned that cannot fix signing and a
blocked merge is retried every tick; and the daemon's plan-completion commit runs a porcelain
`git commit` in a process environment from which `GNUPGHOME` and `SSH_AUTH_SOCK` were removed (Decision 9),
so with `commit.gpgsign=true` it fails and loom prints only a warning. You write both routes. C1 writes
the git core and signing helpers, C2 the relay handler (its own signing refusals), C3 the doctrine.

## Files owned and files to read

Own exactly (repository-relative): `loom/src/orchestrator/core/merge_handler/landing.rs`,
`loom/src/orchestrator/core/merge_handler/landing_tests.rs`,
`loom/src/orchestrator/core/merge_handler/landing_signing_tests.rs` (new),
`loom/src/orchestrator/core/merge_handler/auto_merge_outcome.rs`, `loom/src/fs/plan_lifecycle/commit.rs`.
Also read `merge_handler/auto_merge_outcome.rs:80-125` (`apply_auto_merge_outcome`),
`merge_handler/resolver_spawn.rs:122-136` (`gate_holds_merge_stage`), `merge_handler/resolver_stop.rs:130-180`
(`stop_gated_resolvers`, `stop_resolver`) and `merge_handler/review_route.rs:62-115`
(`route_merge_stage_to_review`, `ReviewRoute`).

Read: `landing.rs:22-40` (`Landing`) and `:76-110` (`land_stage_merge`); `merge_handler/review_route.rs:52-60`
(`route_to_human_review`); `merge_handler/resolver_spawn.rs:90-120` (`clean_merge_settled`),
`blocked_retry.rs:105-130` and `resolver_exit.rs:130-140` (how each caller treats `Landing::Failed` and
`Landing::Held`); `git/merge/tree.rs:196-211` (`commit_merge`), `:245-253` (`PendingMerge::commit`) and
`git/merge/mod.rs:200` (`advance_target(..)?` passes the error up unchanged);
`fs/plan_lifecycle/commit.rs:19-67` (`commit_post_completion_changes`) and `fs/plan_lifecycle.rs:242-247`
(its only caller); `git/runner.rs:134-155` (`run_git`, `run_git_with_env`, `run_git_checked`).

## Pinned interfaces

Consumed from C1 (`../common.md`, "Signing" and "Signing failure handling"; C1 writes them in parallel and
the crate does not compile until all six workers return, so write against these exactly):

- `crate::git::signing::CommitTreeError`: `pub struct CommitTreeError { pub signing: bool, pub detail:
  String }` (Display, Error). `commit_merge` returns it through `anyhow::Error::from`, so it stays
  downcastable anywhere in the anyhow chain.
- `crate::git::signing::{SigningEnv, installed}`: `pub fn installed() -> &'static SigningEnv` (the default
  when nothing was installed); `impl SigningEnv { pub fn env_pairs(&self) -> Vec<(&'static str, &OsStr)> }`
  (`GNUPGHOME` and `SSH_AUTH_SOCK`, each when set); `pub const SIGN_TIMEOUT: Duration` (30 s);
  `pub fn signing_enabled(repo: &Path) -> anyhow::Result<bool>` (not needed here: a porcelain `git commit`
  reads `commit.gpgsign` itself).
- `crate::git::runner::run_git_with_env_within`: `pub(crate) fn run_git_with_env_within(repo: &Path, args:
  &[&str], env: &[(&str, &OsStr)], timeout: Duration) -> Result<Output>`, built on the hardened
  `git_command` (it adds `core.hooksPath=/dev/null` to every call), the env pairs set on that one command
  only.
- Test support: `crate::git::signing::tests::fake_signer(repo: &Path, fail: bool) -> PathBuf` (repo-local
  `commit.gpgsign=true`, `gpg.format=openpgp` and `gpg.program=<script>`; `fail` makes the script exit 1
  with "fake signer refused" on stderr; the returned file is its argv log).

You provide one function to C2: `Orchestrator::hold_merge_for_signing(&mut self, stage_id: &str, detail:
&str) -> String`, with `pub(in crate::orchestrator::core)` visibility (task 5). C2's `InboxHost` impl in
`inbox_drain.rs` calls it with exactly this signature.

## Root cause and current behaviour

- `land_stage_merge` (`landing.rs:79-110`) maps every `Err` of `merge_stage` to
  `Landing::Failed(format!("{error:#}"))` (`:108`). Once `commit_merge` signs, a signing failure arrives
  there. The callers treat `Failed` as transient: `clean_merge_settled` (`resolver_spawn.rs:115`) logs
  "Landing a clean merge failed; spawning a resolver" and returns false, so the spawn pass starts a
  resolver; `blocked_retry.rs:123` warns and retries the landing on the next tick; `resolver_exit.rs:136`
  reports an unresolved merge. A resolver cannot fix signing.
- `Landing::Held` is the outcome that already means "routed to human review" (the control-path gate,
  `:104-107`): `clean_merge_settled` (`resolver_spawn.rs:107`), `blocked_retry.rs:128` and
  `resolver_exit.rs:138` do nothing further for it, so it spawns no resolver and is not retried.
  `route_to_human_review` (`review_route.rs:52`) persists `NeedsHumanReview` with a reason whatever the
  stage's status and mirrors it into the graph.
- `commit_post_completion_changes` (`commit.rs:19-67`, 49 lines) stages the plan rename and tracked
  changes, then runs `run_git_checked(&["commit", "-m", ..], repo_root)?` (`:48-56`): a porcelain
  `git commit` in the daemon's own process environment. `plan_lifecycle.rs:242-247` prints only
  `Warning: Failed to commit post-completion changes: {e}` on failure, so the tracked changes stay
  uncommitted on the default branch.

## Tasks

1. **`land_stage_merge`** in `landing.rs`. In the `Err(error)` arm, when the error chain holds a
   `CommitTreeError` with `signing == true` (`error.chain().find_map(|e| e.downcast_ref::<CommitTreeError>())`),
   call `self.route_to_human_review(stage_id, format!("merge commit signing failed: {detail}; fix signing
   (gpg-agent passphrase cache, GUI pinentry or ssh-agent key), then loom stage human-review {stage_id}
   --approve"), None)` and return `Landing::Held` (no resolver spawn, no per-tick retry; the operator
   fixes signing and approves). Every other `Err`, including a `CommitTreeError` with `signing == false`,
   keeps `Landing::Failed(format!("{error:#}"))`. Put the downcast in a small private function
   (`fn signing_failure(error: &anyhow::Error) -> Option<&CommitTreeError>`, returning `Some` only when
   `signing` is true) so `land_stage_merge` stays under 50 lines. Detect by downcast, never by matching the
   error text. Update the `Landing::Held` variant doc so it names both holds (the control-path gate and a
   signing failure of loom's own merge commit).
2. **`commit_post_completion_changes`** in `fs/plan_lifecycle/commit.rs`. Run the commit through
   `run_git_with_env_within(repo_root, &["commit", "-m", &message], &signing::installed().env_pairs(),
   signing::SIGN_TIMEOUT)` instead of `run_git_checked(&["commit", ..])`: the runner still adds the
   hook-disabling flags, `SigningEnv` reaches only this call, and the signing is bounded at 30 s. The result is the
   raw `Output` (it mirrors `run_git_with_env`): check `output.status.success()` and bail with the exit
   code and the stderr tail, naming `git commit`, because the caller prints the error as its warning.
   The function is 49 lines and the limit is 50: move the commit step into a small private function
   (for example `fn commit_signed(repo_root: &Path, message: &str) -> Result<()>`) and call it. The
   attestation (`attest_completion_commit`) still runs only after a successful commit.
3. **Shared helpers in `landing.rs`.** Make the downcast helper `pub(super) fn signing_failure(error:
   &anyhow::Error) -> Option<&CommitTreeError>`. Add `pub(super) fn merge_signing_reason(stage_id: &str,
   detail: &str) -> String`, which returns the task 1 text. Every merge route uses it.
4. **The first auto-merge** (`auto_merge_outcome.rs`, `apply_auto_merge_outcome`). `attempt_auto_merge`
   reaches `commit_merge` through `merge_stage`, and its `.context("Auto-merge failed")` keeps the chain.
   In the `Err(error)` arm, when `super::landing::signing_failure(&error)` is `Some`, call
   `self.route_to_human_review(stage_id, merge_signing_reason(stage_id, &failure.detail), None)` and
   return `false`. Do not call `persist_merge_blocked` in that case. Every other `Err` keeps
   `persist_merge_blocked`. Today that arm records `MergeBlocked` with an `InfrastructureError`, and the
   next spawn pass lands the merge again, which is a second signing attempt.
5. **The relayed merge hold** (for C2's Merge-scope signing refusal). Add
   `pub(in crate::orchestrator::core) fn hold_merge_for_signing(&mut self, stage_id: &str, detail: &str) ->
   String` to `landing.rs`, built like `gate_holds_merge_stage`:
   1. `let stopped = self.stop_gated_resolvers(stage_id);` It kills a tracked or signalled resolver and
      retires it only on proof of death. Its note names the in-progress merge and the manual merge
      steps.
   2. The reason is `merge_signing_reason(..)`, plus `". {stopped}"` when that is `Some`.
   3. `self.route_merge_stage_to_review(stage_id, reason, None)`.
   4. Return `"held for the operator in needs-human-review"` for `ReviewRoute::Routed`; for any other
      outcome, return text naming it (the stage moved on, or not saved).

   C2's `InboxHost` impl calls this function. Both callers live under `orchestrator::core`, so the
   `pub(in ..)` visibility reaches them.

## Tests to write

- `orchestrator::core::merge_handler::landing::tests::a_merge_signing_failure_holds_without_a_resolver`
  (exact path; acceptance runs it `--exact`), appended to `landing_tests.rs`. Use `worktree_stage()` (its
  stage branch `loom/s` carries one commit main lacks, so landing needs a merge commit) and `fake_signer(repo.path(),
  true)` for the main checkout (it sets `commit.gpgsign`, `gpg.format` and `gpg.program` repo-locally, so a
  host's global config cannot change the test). Record `main_tip(&repo)` first. Assert:
  `orchestrator.land_stage_merge(ID, "main") == Landing::Held`; the stage on disk is `NeedsHumanReview`
  with a `review_reason` containing `merge commit signing failed`, `fake signer refused` and
  `loom stage human-review s --approve`; `main_tip(&repo)` is unchanged; `merge_resolver_attempts` for the
  stage is 0. Model it on `landing_a_control_path_branch_holds_it_for_human_review` (`:297-313`).
  `landing_tests.rs` is 352 lines: the new test adds at most 45 and the file ends at or under 400.
- The two tests for tasks 4 and 5 go in the new file `merge_handler/landing_signing_tests.rs`. Declare it
  at the end of `landing_tests.rs` with `#[path = "landing_signing_tests.rs"] mod signing;`, one added
  line. A `#[path]` in a file resolves from that file's directory, and the child module reuses the
  parent's fixtures through `super::`. Both tests have exact paths, and acceptance runs each `--exact`:
  - `orchestrator::core::merge_handler::landing::tests::signing::a_first_auto_merge_signing_failure_routes_to_review`.
    Set up `worktree_stage()` and `fake_signer(repo.path(), true)`; `orchestrator.try_auto_merge(ID)` is
    false. Assert:
    - the stage on disk is `NeedsHumanReview`, not `MergeBlocked`;
    - its `review_reason` contains `merge commit signing failed` and `fake signer refused`;
    - `failure_info` is not an `InfrastructureError`;
    - `main_tip` is unchanged;
    - `spawn_merge_resolution_sessions()` returns 0.
  - `orchestrator::core::merge_handler::landing::tests::signing::a_relayed_merge_signing_hold_stops_the_resolver`.
    Run it once for `MergeConflict` and once for `MergeBlocked` (through `set_status`). Insert a
    `Session::new_merge` resolver in `active_sessions`, as
    `the_retry_keeps_the_worktree_while_a_resolver_is_tracked` does. Then
    `hold_merge_for_signing(ID, "fake signer refused")` returns text containing `held for the
    operator`. Assert:
    - the stage is `NeedsHumanReview`, with a `review_reason` containing the resolver's session id and
      `loom stage human-review s --approve`;
    - the resolver is no longer in `active_sessions`, or the reason says loom could not prove it
      stopped;
    - `spawn_merge_resolution_sessions()` returns 0.

  `auto_merge_outcome.rs` (126 lines) and `landing.rs` (332 lines) stay under 400.
- `fs::plan_lifecycle::commit::tests::the_plan_done_commit_signs_with_the_signing_environment` (exact
  path; acceptance runs it `--exact`), appended to the inline `mod tests` of `commit.rs`. Build `plan_repo()`,
  call `fake_signer(&repo.root, false)`, then `complete(&repo)` (it renames the plan and runs
  `commit_post_completion_changes`). Assert `git(&repo.root, &["cat-file", "commit", "HEAD"])` contains
  `gpgsig` and the signer's argv log has one line. A second assertion in the same test or a sibling test
  (`a_failing_signer_fails_the_plan_done_commit`, your name) uses `fake_signer(&repo.root, true)` and expects
  `commit_post_completion_changes` to return an `Err` that names `git commit`, with HEAD unmoved.

## Patterns to copy and not copy

Copy `landing_a_control_path_branch_holds_it_for_human_review` for the `Held` assertions and the
`plan_repo()` / `complete()` / `git()` helpers of the `commit.rs` tests. Do not copy the `Err` arm's
`Landing::Failed(format!("{error:#}"))` for the signing case, and do not detect it by searching the
rendered text.

## Traps

- The crate does not compile mid-wave: C1, C2, C3, U1 and U2 write in parallel. Write against the pinned
  signatures above exactly; a compile error in a file you do not own is theirs, report it with file and line.
- Never edit an existing assertion line in `landing_tests.rs` or in the `commit.rs` tests (a test-integrity
  event). New tests and new lines only; keep `landing_tests.rs` at or under 400 lines.
- Never call `signing::install` in a test: it is a process-wide `OnceLock`, and `installed()` returning
  the default is what the tests rely on.
- You run no git write command yourself (`../common.md`); the tests' own git calls run in TempDirs only,
  never in the live checkout.
- A stage session's Bash environment carries `LOOM_STAGE_ID` and `LOOM_SESSION_ID`, and the main agent runs
  these tests in-session. These tests drive git and the orchestrator only (no hook script), so they do not
  depend on either variable: never read, set or assert on them.
- `Landing::Held` is also read by `inbox_drain/merge_resolved.rs` `settle_for_landing` (`:62-65`, C2's
  file), whose `Held` text says the stage branch touches a control path. A signing hold now reaches that
  text too: report it, do not edit that file.
- `route_to_human_review` is `pub(super)` in `merge_handler`; `landing.rs` already calls it in the
  `MergeResult::Held` arm, so no visibility change is needed. `stop_gated_resolvers` and
  `route_merge_stage_to_review` are `pub(super)` in `merge_handler` as well, so `landing.rs` can call
  them as they are.

## The one check

None: the crate does not compile until all six workers return. Do not run cargo; the main agent runs
`cargo test --lib orchestrator::core::merge_handler` and `cargo test --lib fs::plan_lifecycle` afterwards.

## Report format

Files changed; line counts of `landing.rs`, `landing_tests.rs` and `commit.rs`; whether
`commit_post_completion_changes` and `land_stage_merge` stayed under 50 lines; deviations from the pins
with the reason; the `Landing::Held` text note above; any contradiction between the tree and this brief.
