# Platform And Commit Gaps

> Open gaps: commits, launch, stalls

## No macOS CI Runner: BSD Behaviour Is Emulated

CI runs Linux only. The macOS-specific paths (BSD `wc` padding, BSD `stat` and `date` forms,
`sha256sum` versus `shasum`, the `kern.bootsessionuuid` sysctl, objc and CoreFoundation behaviour
after a fork) are exercised through shims and unit seams, not on macOS:

- `loom-hooks/tests/run-all.sh` runs in the `hook-syntax` job twice, plain and with
  `LOOM_HOOK_TEST_BSD=1`, which puts `loom-hooks/tests/bsd-shims/` (padded `wc`, BSD `stat`, BSD `date`)
  first on PATH. The shims find the real tool by skipping PATH entries that are the shim directory
  (`-ef`), then `/usr/bin:/bin`, so a PATH built from symlinks into the shim, or stacked shim
  directories, breaks them. `scripts/check-hook-syntax.sh` parses the extensionless shims too.
- `bsd-shims/date` reads zone-less formats as UTC whatever `-u` says, while real `date -j -f` uses
  local time unless `-u` is given. A mutation deleting `-u` at `_lifecycle.sh`'s epoch parse survives;
  the practical effect is small because start and observed timestamps share one parse. Fix: honour
  `-u` in the shim and add an epoch check under `TZ=America/Los_Angeles`.
- `knowledge-orient-emits.sh` asserts `3 entries` but not the byte count, so dropping the padded-`wc`
  strip in `knowledge-orient.sh` survives under the shim (cosmetic).

## The Production Daemon Re-Exec Has No Automated Test

Nothing automated runs `loom run` spawning `loom run --daemon-child`, the `0x02`/`0x01` readiness
handshake end to end, or `loom stop`. Stage sandboxes deny `AF_UNIX` and no daemon fixture exists, so
the operator's host smoke test (`loom run`, `loom status`, `loom stop` on the machine) is the only
cover. In-crate tests cover the pieces: `launch/tests.rs` drives `await_ready` against scripted
children, `lifecycle/tests.rs` runs `run_server` with a pre-set shutdown flag, and
`commands/run/daemon_child.rs` tests the refusals. Tests that bind a socket guard with
`process::sandbox_probe::skip_unless(unix_socket_bindable)`, which skips inside a stage sandbox and
runs on the host (pre-push) and in CI.

- `.github/workflows/ci.yml` never sets `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`, so `relay_commit_e2e` (skipped on missing
  `jq`/`bash`/`git`) and the `AF_UNIX`-gated tests can skip silently in CI; set it in the cargo test job.
- The two `run_server` tests in `daemon/server/lifecycle/tests.rs` flip the process-wide umask to
  `0o077` around the bind; a concurrent test asserting a created file's mode could see it for
  microseconds. Mark them `#[serial]`.
- `daemon/server/lifecycle/tests.rs` holds the 103/104/105-byte boundary tests of
  `daemon::socket::socket_path_fits`, which has no boundary test of its own; `rpc_tests.rs`,
  `commands/stage/tests/control_complete.rs` and the PR's original test hold three near-identical
  "worktree spelling past `sun_path`" cases.
- `launch.rs` prints the pre-redirect pipe text (`self.text`) as is while the log tail goes through
  `terminal_safe`. The text comes from the daemon child, so the risk is low, but the two are
  inconsistent.
- `loom run --daemon-child <root>` run directly skips the launcher's `env_clear` allowlist, so that
  daemon holds the ambient environment (stage processes are still filtered). Operator-only in practice.
- `signing::take_from_process` removes the signing variables with `remove_var`, which leaves the
  original strings in the initial environment block, readable through `/proc/<daemon pid>/environ` by
  a same-uid process outside a PID namespace. They are paths, not credentials; confirm the stage
  sandbox hides the daemon's `/proc`.
- Several tail helpers keep the last N bytes or lines of text (`launch.rs` via `clamp_from_front`,
  `commit.rs` `last_lines`, `signing.rs` `stderr_tail`, `provision.rs` `tail_lines`,
  `verify/criteria/cache_contract.rs` `diagnostic_tail`); the first reuses a shared helper, the rest
  differ in behaviour (trim, line or byte caps, `display_safe`) and stay separate.

## Sessions Do Not Receive Proxy, CA or CLAUDE_CONFIG_DIR Variables

`AGENT_SESSION_ENV_NAMES` carries no proxy variables, CA bundle locations or `CLAUDE_CONFIG_DIR`, and
no other list forwards `CLAUDE_CONFIG_DIR`, so a host behind a proxy or with a relocated claude
config directory behaves differently for a stage agent than for the operator. Forwarding them widens
what a session can reach, so it is a deliberate non-goal. See
[Three Stage Environment Allowlists](sandbox-and-confinement-gaps.md#three-stage-environment-allowlists).
`STAGE_ENVIRONMENT_POLICY` stays `stage-host-allowlist-v2`: it is an identity label hashed into
completion evidence, and both sides use the same constant.

## Daemon-Owned Commits: Open Suggestions

[Daemon-Owned Commits](../architecture/daemon-owned-commits.md) describes the design; these are open.

- **Tick cost.** `apply_commit` runs on the orchestrator tick thread: signing up to 30 s plus a
  knowledge merge-lock wait up to 10 s per request, and N x 30 s when several relayed commits drain in
  one pass with a hung signer (`session_pass.rs`). Accepted. A signer timeout could short-circuit the
  rest of the pass.
- **`index.lock` after a timeout.** The plan-completion commit (`fs/plan_lifecycle/commit.rs`) runs
  `git commit` under `SIGN_TIMEOUT`; a timeout SIGKILLs it while it holds `index.lock`, leaving a stale
  `.git/index.lock` in the main checkout. `commit-tree` plus `update-ref` would avoid it. The single
  `signing::installed()` argument to `commit_signed` is not mutation-proof, since covering it needs
  the process-wide `OnceLock` installed in a lib test.
- **Merge-scope `MERGE_HEAD`.** The core accepts the target's tip or any ancestor. A session can point
  `MERGE_HEAD` at an old target commit that holds a since-removed gitlink or state path, and that
  entry then passes as `MERGE_HEAD`'s own. Requiring the tip (with a retry when the target moves)
  closes it. A Merge scope without `merging_into` skips the check silently; only the daemon sets it,
  so the target could become part of the scope, or the core could refuse the scope without it.
- **Signing failure class.** Under `-S` every `commit-tree` failure is a signing failure, so a
  missing git identity gets a block reason telling the operator to fix gpg-agent or pinentry; the
  detail text still shows the real cause. The signer stderr tail (key paths, agent sockets) reaches the
  session through the refusal reason: useful to the operator, exposed to the session.
- **Merge hold wording.** `merge_signing_reason` names `human-review --approve`, which requeues a fresh
  Stage session on the reused worktree; in Merge scope the reason also carries `manual_merge_steps`
  (`--force-complete`), so it names two remedies. A requeued session inherits `MERGE_HEAD` and a staged
  resolution and is refused until it aborts the merge. Decision: `--approve` stays; naming only
  `--force-complete` and putting the remedy before the 1,000-byte signer tail (`loom status` cuts at
  200 characters) is the alternative.
- **Message validation edge.** `Co-authored-by : Claude` (a space before the colon, valid to git's
  trailer parser) without a noreply address passes `validate_commit_message`, while
  `Co-authored-by: Claude Monet <...>` is refused. Both are best-effort.
- **Completion guard cost.** `refuse_uncommitted_index` adds up to three git subprocesses inside
  `locked_dir_update(&sessions_dir)`, a blocking flock with no timeout that the tick thread's session
  writes also take. The check is read-only and could run before the lock, like `check_completion_gates`.
- **Hook coverage.** `commit-filter.sh`'s `git_runs_commit` does not step over `--config-env <name>=<var>`
  (a separate value token) or `-c alias.x=commit x`; the file is 494 lines and must not grow. The block
  also hits `git commit` in throwaway fixture repos a stage session runs under `/tmp`.
  `commit-filter-session-git-commit.sh` lacks cases for `git -c k=v commit`, `env git commit`,
  `command git commit`, `bash -c 'git commit'` and `git --config-env`.
- **Test coverage.** `inbox_drain.rs`'s trait forward to `Orchestrator::hold_merge_for_signing` is
  covered by wiring grep only (the drain test uses `FakeHost`, the orchestrator test calls the method
  directly). `daemon_owned_commits_contracts.rs` is a frozen contract file: `planted_hooks_never_run`
  has no positive control (only `reference-transaction` has detection power, and a `noexec` `TMPDIR`
  lets a mutant pass; a control that fires the hooks through the test's `git()` helper would fix it),
  `staged_state_path_is_refused` asserts only `is_err()`, and three message tests in
  `commands/stage/commit/tests.rs` keep a redundant leading `is_err()` assertion.
  `tests/e2e/uncommitted_changes.rs` calls `run::execute` (foreground) three times, so it reaches
  `signing::take_from_process` and removes `GNUPGHOME`/`SSH_AUTH_SOCK` from the e2e test process
  (benign there).
- **Worktree id derivation.** `stage.worktree.unwrap_or(stage.id)` plus `validate_id` is repeated in
  `orchestrator/adjudication/session.rs` and `adjudication/prompt/execution_site.rs`; the helper
  `git::worktree::stage_worktree_path` serves the commit handler, the completion guard, `observer.rs`
  and `verify/contracts/site.rs`. The adjudication sites stay because they are infallible with their own
  fallbacks.

## Stall Parking and the Login Probe: Open Suggestions

- **Park-time probe blocks the tick.** `park_never_worked` runs `claude auth status` (bounded at
  `PROBE_TIMEOUT`, 30 s, in `claude/auth.rs`) on the orchestrator poll thread. Several stages parked in
  one tick during a hung or offline `claude` could freeze it for N x 30 s. Probe once per tick, cache a
  failed probe, or use a shorter bound at that call site.
- **`apiProvider` is ignored.** `parse_auth_status` reads `loggedIn` and `authMethod` only. If the CLI
  reports `loggedIn: false` for Bedrock or Vertex users whose credentials come from settings, `loom run`
  would refuse a working setup; `Unknown` when `apiProvider` is not first-party is the candidate
  (CLI behaviour unverified).
- **Routing is untested.** No test proves `announce_needs_human_review` routes the reason through
  `review_headline`: replacing `review_reason.map(review_headline)` with the raw reason passes every
  test and puts pane text back in `orchestrator.log` and the notification. Extracting a pure function
  that returns the printed line and the notification reason would make it testable.
- **First-call stages.** A stage whose first tool call is a long foreground run is parked at one budget;
  it needs a raised `subagent_timeout_secs`
  ([Long Codex Runs Starve the Loom Heartbeat](codex-heartbeat-starvation.md)).

## Test-Code Hygiene Left As Is

Small items judged low value, recorded so they are not rediscovered:

- `enable_bsd_tools` in `tests/codex_evidence/fixture_runtime.rs` hand-rolls `OpenOptions` although
  `fixture_support::write_executable` exists in the same crate; the same three-shim install loop also
  sits in `worker_evidence/setup.rs` and `_bsd_path.sh`.
- `a_socket_path_past_the_limit_refuses_the_run` in `commands/run/daemon_child.rs` tests
  `run/mod.rs::require_socket_path_fits` and belongs in `run/tests.rs`.
- `verify/review/gate_hint_tests.rs` plants a FIFO at `starts.jsonl` only and hangs instead of failing
  if `O_NONBLOCK` is ever removed; running `harvest_hint` on a thread with `recv_timeout` would turn it
  into an assertion, and `stop-skips.jsonl` goes through the same `read_ledger`.

## Recurring Mistakes Awaiting a Check

Each recurred after its lesson was recorded, so a check beats another paragraph:

- **The signal's acceptance list drops `exit_code`.** Three earlier stages and then platform-portability's
  orchestrator (which quoted a criterion into a worker brief) read a negative `rg` criterion (`exit_code: 1`,
  absence expected) as a presence check, and this stage's own signal again listed its two negative criteria
  without the code ([The Signal's Acceptance List Drops `exit_code`](../mistakes/verification-v2-delivery.md#the-signals-acceptance-list-drops-exit_code)).
  Proposal: print each criterion's `exit_code` in the signal's Acceptance Criteria list, or have
  `loom plan verify` flag a plan whose negative `rg` criterion text contains the matched phrase.
- **`loom subagents watch` exits 6 for a worker that already handed back.** It recurred in a second plan. Proposal: treat a
  recorded `SubagentHandback` as terminal evidence in the hung rule (`commands/subagents/wait/stall.rs`).
- **Completion started in the background.** The plan-writer gate conventions advise `run_in_background` for the
  completion command, which the broker cannot read. Proposal: drop the advice and add a `loom plan verify` note
  for gate text that names it ([Completion Started With `run_in_background`](../mistakes/completion-broker-credential.md#completion-started-with-run_in_background-reached-no-broker-2026-10-06)).

## Tooling Gaps Found While Running the Plan

- **The unwired-file check does not follow `#[path]`.** The stage-completion check flags a test file wired
  by a `#[path = "..."] mod tests;` declaration as unwired; a `wiring:` memory note per file clears it. Files
  hit: `claude/auth_tests.rs`, `commands/stage/human_review_tests.rs`, `remote_control_tests.rs`,
  `git/stage_commit/tests_paths.rs`, `inbox_drain/commit_tests_signing.rs` and
  `merge_handler/landing_signing_tests.rs`. Proposal: make the check follow `#[path]` declarations.
- **Stage wall clock.** The platform-portability stage ran 75+ minutes. One oversized worker brief (hooks, 18
  files, 31 minutes against 2 to 14 for the others) set the wave time, and the completion re-run of every
  criterion (about 440 s) repeats the in-session passes because the acceptance cache keys on HEAD plus
  `git status` (`verify/criteria/cache.rs`) and the commit invalidates them. Candidate fixes: key the cache on
  tree content so a commit of an already-verified tree reuses its passes, and have the plan-writer balance
  worker briefs so none exceeds about twice the median.
- **A load-sensitive timing bound.** `commands::status::web::terminal::tests_pty::pty_child_shutdown_reaps_the_child`
  failed once during the completion run on its `started.elapsed() < 1s` bound while passing 15 of 15 alone.
