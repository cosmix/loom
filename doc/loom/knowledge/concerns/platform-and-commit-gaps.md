# Platform And Commit Gaps

>

## macOS CI Jobs and Remaining BSD Gaps

CI runs every gate whose result depends on the target OS on `macos-latest` as well as Linux:
`lint-macos` (clippy, rustdoc), `test-macos` (`cargo test --all-targets --no-fail-fast`, both smoke
scripts), `flake-check-macos`, and `hooks-macos` (hook syntax, the hook suite and the partial-staging
regression under `/bin/bash` 3.2, which a step asserts). fmt, audit, deny and the web job stay Linux-only.
Related portability lessons: [The First macOS Hook Run](../mistakes/hooks-shell-portability.md#the-first-macos-hook-run-found-bash-32-and-bsd-faults-the-linux-shims-hid-2026-10-13),
[Rust Fixtures Assumed Linux](../mistakes/ci-toolchain-and-cargo.md#rust-fixtures-assumed-linux-paths-ptys-and-tool-output-2026-10-13)
and [macOS `proc_pidinfo` Hid Zombies](../mistakes/sessions-and-liveness.md#macos-proc_pidinfo-hid-zombies-so-an-unreaped-dead-session-read-as-alive-2026-10-13).
Open gaps:

- `run-all.sh` hides a passing test's output, so on the macOS runner a SKIP of the E3 Seatbelt case in
  `codex-forward-guard-stage-evidence.sh` cannot be ruled out. E3 relies on macOS refusing a nested
  profile that differs from the outer one; an identical nested profile is admitted, so the guard's
  allow-everything probe only detects confinement by a different outer profile.
- No hook step caps `ulimit -u`, so a runaway in a hook test kills the runner and its log, as a
  BSD-shim recursion once did to the Linux `Hook syntax` job.
- Inside Claude Code's macOS sandbox, `ps`, `sysctl` (the boot ID), `sandbox-exec`, PTY spawn, loopback
  bind and writes under `/var/folders` are denied, so the codex_evidence, worker_evidence, relay e2e,
  PTY, credential-guard and ancestry tests fail or skip there for that reason alone. A local run is no
  evidence either way; run them outside the sandbox (the `!` prefix) or read the CI job log with
  `gh api repos/cosmix/loom/actions/jobs/<job-id>/logs`.
- Unverified: `reject_symlink_components` (`subagent_lifecycle/claude_files.rs`) refuses an evidence
  path with any symlinked component. If a real transcript path runs through a symlinked `~/.claude`
  (dotfile managers create one), production refuses that evidence as unsafe.
- The smoke scripts run under the runner's Homebrew bash 5, never under bash 3.2.
- `loom-hooks/tests/run-all.sh` also runs in the Linux `hook-syntax` job twice, plain and with
  `LOOM_HOOK_TEST_BSD=1`, which puts `loom-hooks/tests/bsd-shims/` (padded `wc`, BSD `stat`, BSD `date`)
  first on PATH. A shim finds the real tool among the PATH entries after its own position (then
  `/usr/bin:/bin`), skipping copies that carry the `# loom-bsd-shim` marker line. A wrapper earlier on
  PATH that hands calls back to the shim is therefore never chosen. `LOOM_BSD_SHIM_DEPTH` stops a
  re-entered shim with exit 125 at depth 3. When the real tool already rejects `--version` (BSD, as on
  macOS), the shim execs it unchanged. `scripts/check-hook-syntax.sh` parses the
  extensionless shims too.
- `knowledge-orient-emits.sh` asserts `3 entries` but not the byte count, so dropping the padded-`wc`
  strip in `knowledge-orient.sh` survives under the shim (cosmetic).
- Hook stdin reads use `loom_read_input` from `loom-hooks/_bounded.sh:63-65`; its portable
  `loom_run_bounded` fallback provides a deadline when GNU timeout is absent. Separate direct
  command-execution fallbacks remain in `user-prompt-context.sh:77-86`,
  `codex-apply-patch.sh:97-103` and `loom-relay.sh:196-204`; audit those independently.
- Bash 3.2 writes every here-string and here-document to a temp file. It tries `$TMPDIR`, `/tmp`,
  `/var/tmp`, `/usr/tmp`, then the current directory, and leaves an `sh-thd-*` file behind when
  killed mid-command. Only when none is writable does the redirect fail ("cannot create temp file for
  here document"). `set -e` does not stop the script then; the command just reads nothing.
  `codex-apply-patch.sh` (`done <<<"$PATCH"`) then sees no targets and blocks in `pre` mode, so it fails
  closed.

## The Production Daemon Re-Exec Has No Automated Test

Nothing automated runs `loom run` spawning `loom run --daemon-child`, the `0x02`/`0x01` readiness
handshake end to end, or `loom stop`. Stage sandboxes deny `AF_UNIX` and no daemon fixture exists, so
the operator's host smoke test (`loom run`, `loom status`, `loom stop` on the machine) is the only
cover. In-crate tests cover the pieces: `launch/tests.rs` drives `await_ready` against scripted
children, `lifecycle/tests.rs` runs `run_server` with a pre-set shutdown flag, and
`commands/run/daemon_child.rs` tests the refusals. Tests that bind a socket guard with
`process::sandbox_probe::skip_unless(unix_socket_bindable)`, which skips inside a stage sandbox and
runs on the host (a local `cargo test`) and in CI.

- `.github/workflows/ci.yml` never sets `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`, so `relay_commit_e2e` (skipped on missing
  `jq`/`bash`/`git`) and the `AF_UNIX`-gated tests can skip silently in CI; set it in the cargo test job.
- The two `run_server` tests in `daemon/server/lifecycle/tests.rs` flip the process-wide umask to
  `0o077` around the bind; a concurrent test asserting a created file's mode could see it for
  microseconds. Mark them `#[serial]`.
- `daemon/server/lifecycle/tests.rs` holds the 103/104/105-byte boundary tests of
  `daemon::socket::socket_path_fits`, which has no boundary test of its own; `rpc_tests.rs`,
  commands/stage/tests/control_complete.rs (removed) and the PR's original test hold three near-identical
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
  `run/mod.rs::require_socket_path_fits` and belongs in `commands/run/tests/`.
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
- **`loom subagents watch --worker codex:<unit>` exits 5 after a forwarder spawn.** It recurred in at least eight stages of the planner-ui-redesign plan (help, feature-ui, kickoff-session, document-ui, reading-experience, design-foundation, and a rejected mix of `claude:` and `codex:` workers): "worker set does not resolve to one Claude parent UUID", or "expected exactly one Codex execution, found 0" for direct-backend forwards. Binding the forwarders as `claude:<agent-id>` works but exits 0 when the forwarders stop, not when the backgrounded Codex job ends. Proposal: resolve a codex unit from the forwarder's `LOOM-FORWARD` markers for direct-backend forwards, or have `loom-orchestration` say plainly to wait on the forwarders' own completion notices and verify by `git diff` ([the mistake](../mistakes/subagent-liveness-and-watch.md#loom-subagents-watch-rejects-a-codex-worker-issued-right-after-its-forwarder-spawn-and-exits-6-for-a-handed-back-worker-2026-10-06)).
- **Reviewer rounds go unrecorded.** Six more `loom-code-reviewer` runs (design-foundation, help, reading-experience, feature-model) ended with `loom stage review status` showing 0 rounds and N stop events not harvested. Causes seen: the reviewer delivers through `SubagentHandback` even when told not to and the harness indents the report, so the `loom-review` fence is not at column zero; the daemon socket is unreachable from the stage sandbox; and a version 1 plan never records rounds at all. Proposal: let the harvest accept an indented fence, and have `loom stage review status` say when the plan is version 1.
- **Completion started in the background.** The plan-writer gate conventions advise `run_in_background` for the
  completion command, which the broker cannot read. Proposal: drop the advice and add a `loom plan verify` note
  for gate text that names it ([Completion Started With `run_in_background`](../mistakes/completion-broker-credential.md#completion-started-with-run_in_background-reached-no-broker-2026-10-06)).

## Tooling and test-fixture constraints

The unwired-file detector is graph-based (`verify/wiring_detection.rs:37-115`), with incomplete or unresolved evidence reported as uncertain. Any remaining `#[path]` false-positive report needs a fixture showing the current extractor/resolver evidence; it is not a grep-only detector.

A stage's longest worker bounds its wall-clock progress. Brief balancing remains a planning consideration, with no automatic balancing policy. PTY shutdown tests using subsecond wall-clock assertions can be load-sensitive; prefer observable retirement plus a generous bounded deadline when revising those fixtures.

## Busy Status Follow-Ups

Open items left by the scheduler busy-status plan (`busy::while_busy`, the tick's busy line, `busy::loop_state`; see [Daemon Liveness and Loop Health](../architecture/status-data-model.md#daemon-liveness-and-loop-health)). None blocks the feature.

### Derivation Duplicated Across Surfaces

- The stall cause (stale tick versus global ceiling) is re-derived in `scheduling_report_loop.rs` (`age_secs < STALL_THRESHOLD_SECS`) and `ui/tui/ledger/header_daemon.rs`, so a change to a `loop_state` rule can leave surfaces disagreeing. Carry the cause in `LoopState::Stalled`, with a `stalled_by_global_ceiling()` helper that keeps the Stalled shape.
- `tick::read` runs twice per invocation: `alerts()` and `loop_state_for()` in `commands/status.rs`, and `alerts()` and the snapshot in `commands/status/web/model.rs`. A frame crossing a boundary can show a busy payload that disagrees with its alert. Read once and pass the `Tick` to both.
- `BusyOp` variants are listed in `parse`, `busy_tests.rs` and `schema_parity.rs`. A `BusyOp::ALL` const helps only when paired with a compile-time exhaustive match (as `schema_parity.rs` has); used alone it would hide a missing variant.
- `valid_stage_id` in `tick.rs` overlaps `models::forward_receipt::is_safe_id`; the busy-line rule does not require an alphanumeric first byte, so the two stay separate. Keep the local one and say why in its comment if it is touched.

### `while_busy` Hardening Not Enforced

- No nesting, and the closure must not write the tick: documented in `busy.rs`, not checked. A `debug_assert` or thread-local depth guard in `while_busy_every` would catch a violation.
- `while_busy_every` with interval 0 makes `park_timeout(0)` spin and rewrite the tick continuously. Floor the interval in `restamp` (for example `interval.max(10ms)`); only tests call it.
- `record_busy` has a `debug_assert` on the stage-id rule, which panics the scheduler thread in a debug build for a hand-edited id. Ids that passed `validate_id` cannot trigger it.
- `tick::read` uses an unbounded `read_to_string` on every TUI refresh and web frame; cap it (`take(1024)`).
- The fixed temp name `orchestrator.tick.tmp` follows a planted symlink on write. The direct write had the same exposure; `.loom/work` is already trusted state.

### Test Gaps

- Only provisioning has a regression test that proves the busy line while the wrapped call runs. The other call sites have none: `stage_spawn_signal.rs`, `stage_spawn.rs`, `stage_knowledge.rs`, `merge_resolved.rs`, `resolver_exit.rs`.
- `daemon_line.rs` `only_the_loop_state_makes_the_headline_stalled` takes no alerts, so its name overclaims; it is the only Running plus Ticking assertion, so rename it rather than delete.
- The 64-column fallback test in `header_daemon.rs` hard-codes `64 - 19 - 12`; render through `top_line` at `MIN_COLS` so it tracks `NAME_ROOM` and logo width.
- `provision_gate_pin_tests.rs` ordering check uses `sleep 1`. It is unnecessary while `started_at` keeps nanosecond precision (a single wrap yields an equal `started_at`, so `>` still rejects the regression); drop it, or keep at least 1 s if `started_at` ever truncates coarser.

### Web Clock

`web/src/lib/format.ts` computes busy elapsed time from the browser clock against the daemon's `started_at`, so a remote browser with clock skew misreports the ceiling tone; anchoring on the snapshot's `generated_at` would fix it. It truncates with `Math.floor`, matching Rust's `num_seconds`.
