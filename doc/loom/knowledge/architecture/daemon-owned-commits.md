# Daemon Owned Commits

>

## Daemon-Owned Commits

A stage session cannot sign: `~/.gnupg` is a read denial in every session sandbox and the agent
sockets are outside it. So no session runs `git commit`. Every stage, knowledge and merge session
stages its change and commits through the relay, and the daemon writes the commit outside the
sandbox with the operator's git configuration. Loom's own merge commits and the plan-completion
commit are signed the same way.

```text
session:  git add <files>; loom stage commit <stage-id> -m "<message>"
            run_hooks: git hook run pre-commit, commit-msg; git stripspace; validate message
            staged_state: HEAD id, git write-tree        -> relay ticket kind `commit`
session:  loom request status <id> --wait 90              (its own NEXT Bash call)
daemon:   inbox_drain::commit::apply_commit -> Committer::commit_staged
            commit-tree (-S when commit.gpgsign) -> update-ref --no-deref (compare-and-swap)
```

The 90 s wait stays under the Bash tool's 120 s default. `-m` is a repeatable `Vec<String>`
joined with a blank line (`join_commit_message`). The hooks run in the session's own sandbox; the
daemon never runs repository hooks (`NO_HOOKS_ARGS` pins `core.hooksPath` away from `.git/hooks`).

## Modules

| Piece                                  | Where                                                                                                                                   |
| -------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| CLI, hooks, relay or in-process commit | `commands/stage/commit.rs`; `RequestKind::Commit` is a control kind (`relay/kind.rs`), so a teammate cannot relay it                    |
| Plumbing core                          | `git/stage_commit.rs` (`Committer`, `CommitScope`, `CommitRequest`, `CommitRefusal`), checks in `git/stage_commit/checks.rs`            |
| Signing                                | `git/signing.rs`: `commit_tree`, `SigningEnv`, `SIGN_TIMEOUT` (30 s), `take_from_process`, `probe_in`                                   |
| Daemon handler                         | `orchestrator/core/inbox_drain/commit.rs` (`apply_commit`, `prepare_*`), merge hold in `merge_handler/landing.rs`                       |
| Status polling                         | `commands/request/status.rs` (`loom request status <id> --wait <s>`)                                                                    |
| Completion guard                       | `orchestrator/core/inbox_drain/complete.rs::validate` (uncommitted changes and a HEAD mismatch are refused before a host job is queued) |
| Startup check                          | `commands/run/signing_preflight.rs`                                                                                                     |
| Hook block                             | `loom-hooks/commit-filter.sh` `git_runs_commit`                                                                                         |

## Scopes

`apply_commit` maps the session kind to the one ref it may move; Contract, Adjudication and
BaseConflict sessions are refused.

- `Stage`: `loom/<stage-id>` in the stage's worktree, while the stage is `Executing` and the
  session owns it.
- `Knowledge`: the target branch in the main checkout, paths under the knowledge prefix only, with
  the merge lock held (10 s wait) across the commit and its target-guard attestation, because
  loom's git runs no hooks and the `reference-transaction` hook never attests a daemon move.
- `Merge`: `loom/<stage-id>` with `MERGE_HEAD` as the second parent, for the merge session that
  resolves that stage (`MergeConflict` or `MergeBlocked`). Merge resolvers never run `git merge
--continue` or a bare `git merge <target>`; they use `git merge --no-commit --no-ff <target>`.
  The daemon passes the target through `Committer::merging_into`, and the core requires `MERGE_HEAD`
  to be the target's tip or an ancestor of it on the same id it records, because a session can write
  `MERGE_HEAD`.

## What the daemon refuses

A refusal leaves the ref unmoved and settles the request `Refused` with the reason.

- a HEAD other than the one the session saw, or an index that no longer writes the tree it named;
- a staged path under `.loom/`, `.work/` or `.worktrees/` (ASCII case ignored, as `is_control_path`
  does, since a case-insensitive filesystem resolves both names) or any gitlink; a Merge scope allows
  an entry only when it equals HEAD's or `MERGE_HEAD`'s;
- in a Merge scope, a path matching the stage's worktree-relative plan/stage `deny_write` globs
  (`Committer::guarding_paths`, filled by `prepare_merge` from `sandbox::worktree_write_denies`;
  matched by `covers` in `git/stage_commit/checks/guarded.rs`, ASCII case ignored, a trailing `/`
  stripped) whose committed entry differs from `git merge-tree`'s result where git merged it
  cleanly, or from both HEAD and `MERGE_HEAD` where it conflicted. The resolver's sandbox releases
  those denies so `git merge` can write them; this check is what keeps them. The refusal prints a ready command per path (first 10),
  `git --literal-pathspecs restore --source=<src> --staged --worktree -- <path>`: the merged tree
  for a clean path, HEAD or `MERGE_HEAD` for a conflicted one. The path is shell-quoted by
  `shell_path` (`checks/guarded/quote.rs`): single quotes, or `$'...'` when the name holds a control
  character or any character `flatten_char` (`context/untrusted.rs`) would rewrite (bidi, zero-width,
  Unicode spaces and line separators), each such character as `\xHH` bytes and `!` as `\x21`, so the
  message stays one line, renders as what runs, and a pasted name never expands. A name decoded lossily
  gets no command. The merge signal lists the guarded globs and the rule up front. A plan
  snapshot that cannot be read refuses the commit;
- the wrong branch, a non-owner session, a symbolic ref (`update-ref --no-deref`, so a planted
  `refs/heads/loom/<id>` symref cannot move the target);
- a message that is empty, over 16 KiB, holds a NUL, or carries AI attribution
  (`validate_commit_message`, called by both the CLI and the daemon, because a session can write a
  ticket without the CLI).

Staged paths are not checked against the stage's `files:`; Conventional Commit types stay doctrine.
A refusal quotes the path with `{:?}`: the daemon logs the reason and a session picks the path, so a newline in it must not start a log line.

## Completion binds the commit

Completion binds HEAD, and the commit applies asynchronously, so a stage completion issued before the commit landed would bind the old HEAD. The inbox drain (`orchestrator/core/inbox_drain/complete.rs::validate`) refuses a relayed completion while the checkout has uncommitted changes (`Commit first`) or its committed HEAD differs from the HEAD the request names. The host job then verifies committed content only, and the application binds the run's head again (`host_verify/apply.rs::bind_and_complete`). Only the completing session types in `host_verify/apply.rs::COMPLETING_SESSION_TYPES` complete a stage.

## Signing failure routing

Signing runs under `SIGN_TIMEOUT` and every failure leaves the ref unmoved; any `commit-tree -S`
failure is classed as a signing failure, so a missing git identity reads as one too (the detail text
shows the cause).

- Stage and Knowledge commits: the request is refused with `signing failed: <stderr tail>` and the
  stage is blocked through `handle_block_stage` with the remedy and `loom stage retry`.
  `handle_block_stage` reports a refused transition as `Ok(Response::Error)` as well as `Err`; either
  means the block failed, and the settle reason says so.
- Merge commits from a merge session: no block (`MergeBlocked -> Blocked` is not a legal edge, and
  block retirement leaves a resolver alive). `InboxHost::hold_merge_for_signing` calls
  `Orchestrator::hold_merge_for_signing` (`landing.rs`), which stops the resolver, describes the
  in-progress merge and routes the stage to `NeedsHumanReview`: fix signing, then
  `loom stage human-review <id> --approve`. The request settles `Refused` with the signer text.
- Loom's own merge commit (`commit_merge`): `land_stage_merge` and the first auto-merge
  (`apply_auto_merge_outcome`) both route a `CommitTreeError { signing: true }` anywhere in the error
  chain to `NeedsHumanReview`, so no resolver spawns and nothing retries every tick.
- The plan-completion commit (`fs/plan_lifecycle/commit.rs`) runs `commit_signed` with
  `SigningEnv::env_pairs()` and `SIGN_TIMEOUT`, since the daemon's own environment no longer holds
  the signing variables.

## Signing environment and the startup probe

`GNUPGHOME` and `SSH_AUTH_SOCK` reach the orchestrating process only to be captured into a
process-wide `SigningEnv` by `signing::take_from_process()` before any thread exists, and are then
removed from its environment. They are passed only to the signing git invocation, never to a session
or an ordinary git call. Both orchestrating entries call it: `daemon_child::execute` (after its
absolute-path check, which spawns no thread) and `foreground::execute`. `signing::current()` returns
the installed value, else the live environment, so the startup probe gives the same result in both
modes. `main.rs` spawns no thread before dispatch.

`loom run` refuses to start through `require_signing` when `commit.gpgsign` differs between the
operator's environment and the daemon's (including operator false, daemon true: a config reached
only through `GIT_CONFIG_COUNT` or `git -c` is invisible to the daemon) or when a probe signature
fails. The probe signs an empty-tree `commit-tree -S`, once in the operator's environment (with
`GPG_TTY` set from `ttyname(stdin)` when stdin is a terminal and it is unset, so a terminal pinentry
can prompt and the agent caches the passphrase) and once under the daemon's exact environment
(`env_clear` plus `daemon_environment_pairs()` plus the signing pairs). With `gpg.format` openpgp it
prints the cache caveat: gpg-agent's `default-cache-ttl` is 600 s and `max-cache-ttl` 7200 s, so raise
both or sign with an ssh-agent key. `XDG_CONFIG_HOME` and `GIT_CONFIG_GLOBAL` are in the daemon
allowlist so it reads the operator's global git configuration.

## Session-side guards

`loom-hooks/commit-filter.sh` blocks `git commit` in a stage session by walking `LOOM_TOKENS`
(`git_runs_commit`), not by probing option words and subcommand across segments. The scan covers the
whole Bash command text, so a codex forward prompt that quotes `git commit` text is blocked before
codex runs; point to the brief instead of quoting it. `RelayMode` comes from the session's env
(`LOOM_HOOK_CONTEXT`, `LOOM_SCRATCH_DIR`), so a session that unsets them sends `loom stage commit`
down `commit_in_process` with its own privileges: it still cannot sign, so the daemon-owned commit is
doctrine plus a best-effort hook, not a boundary.

## Known limits

- The drain runs on the orchestrator tick thread: one commit can hold the tick for 30 s of signing
  plus up to 10 s on the merge lock, and several relayed commits in one pass cost N x 30 s with a hung
  signer. Accepted; see [Daemon-Owned Commits: Open Suggestions](../concerns/platform-and-commit-gaps.md#daemon-owned-commits-open-suggestions).
- A Merge scope without `merging_into` skips the target check; only the daemon sets it.
- The other open suggestions (ancestor `MERGE_HEAD`, the signing failure class, hook coverage) are
  listed in the concern.

## A Restart-Sync Brief Must Enumerate Every Post-Build Graph State, Not Catch-All the Rest

`sync_resumed_node` (`orchestrator/core/loop_recovery/resume.rs::sync_resumed_node`) reconciles the in-memory
`ExecutionGraph` with on-disk stage files after a daemon restart. A brief for it that said "catch-all
for every graph state other than `WaitingForInput`/`Queued`" under-specified the real state space: a
dependent stage read from disk in directory order can still legitimately be `WaitingForDeps` right
after a restart (its dependency stage file simply hasn't been read yet), and `refresh_ready_status`
later promotes it to `Queued` on its own — but the catch-all clause raced that promotion and could
leave the graph reporting `Executing` while disk said `WaitingForDeps`, a dependents-before-dependency
ordering bug. **Prevention:** a restart-sync brief must enumerate every state the post-build graph can
legally be in for EACH stage relationship (not just "everything else"), and its test suite needs a
dependents-before-dependency ordering case specifically, not just single-stage state checks.

## Completion Evidence Attestation and Session Exit Reasons

> How a completed/parked/stalled session's disposition is proven trustworthy, and how stall is told apart from context exhaustion.

**Completion is a host job.** `loom stage complete` relays a `complete` request. The daemon queues a `Complete` host job, runs the completion checks in a child on the host, and completes the stage only when it applies a passing run (`host_verify/apply.rs`). A failed run is recorded as a failure checkpoint from the job result. The completion gates (test integrity and the review gate) run in the daemon, not in the session: through `host_verify/gates.rs::refusal` (`daemon::check_completion_gates`) before the job is queued and again at apply. See [Host Verification Jobs](verification-v2-gates.md).

**`Session.exit_reason: Option<SessionExitReason>`** (`models/session/types.rs::SessionExitReason`) is an independent field alongside `SessionStatus`, with variants `Completed`, `Crashed`, `ContextCeiling`, `Stalled`, `OperatorStop`, `CriteriaBlocked`, `Replaced`. It exists because a bare `SessionStatus` conflated "why a session stopped" with "what state it's in" — display/status/recovery code all needed the reason independently. `mark_session_terminal_reason` routes nonterminal-origin records (e.g. `Spawning`, a session killed before it ran) directly, NOT through `try_transition`, because the general state-machine transition table doesn't accept `Spawning -> ContextExhausted` as legal even though it's a real crash path — a lesson from replacing `mark_session_context_exhausted`, which set status directly: **when a new primitive replaces a writer, diff their accepted input states before assuming the general transition path covers every case the specific one did.** A stalled judge (an adjudicator that stops making progress) keeps status `Crashed` with `exit_reason: Stalled` — NOT a dedicated `Stalled` status — because judge-respawn logic keys on `Crashed` (`orchestrator/core/event_handler/stalled_judge.rs::on_adjudicator_stalled`); an adjudication verdict close is `Completed`/`Completed`; an accepted retirement is `Replaced`. Every full `Session { .. }` struct literal across the tree (~27 files, including ones outside any single stage's file ownership: `coherence.rs`, `monitor/parked.rs`, `inbox_drain`, e2e/integration tests) must be updated whenever this field changes shape — a `rg '\bSession \{$'` count is not proof of the true site list, since it also matches function return-type arrows like `-> Session {`; read hits before acting on a structural grep count, or let the compiler enumerate breakages after the field lands.

**Completion evidence HMAC attestation** (`loom/src/handoff/completion/attest.rs`) closes a forgery path: `handoffs/` is sandbox-writable by any agent (`commands/handoff/create.rs::SANDBOX_RECOVERY_NOTE`), so an untrusted diagnostic written into a folded checkpoint could otherwise fabricate a repeated "verified" blocker and get a stage killed and parked — violating the invariant that untrusted diagnostics cannot create verified authority, complete, or park a stage. Trusted writers only — the host-job application (`host_verify/apply_rejection.rs`, which records attempt evidence and receipts) and receipt persistence — sign evidence, nonce observations and accepted receipts with a host-only key (`ATTESTATION_KEY_FILE = "completion-attestation.key"`, `attestation_key(work_dir)` in `attest.rs::attestation_key`) at `<work_dir>/completion-attestation.key`, created `0600` on first trusted use. The block refusal (`stage_service/control_block.rs`), `loop_recovery/park.rs`, `recover_hung.rs`, `monitor/completion_blockers.rs` and `status` read ONLY the attested view via `handoff/generator/lookup.rs:load_trusted_session_checkpoint`, never the raw handoff file. The key MUST persist across daemon restarts (attested checkpoints are replayed after restart) and must NOT join the daemon shutdown control-file cleanup list (`daemon/server/core.rs`, which removes `orchestrator.sock`/`.pid`, `admin.token`, `user.token`) — rotating it at shutdown would silently untrust every prior trusted blocker and verified checkpoint; tokens may rotate because clients re-read them per-connection, attestations cannot. Every capsule's OS `denyRead` names the key beside both tokens (`sandbox/settings/tests_read_denies.rs`), `credential-guard.sh` blocks the three for the file tools, and the host criteria hide it (`verify/criteria/confine_protected.rs`). Verify before trusting a new secret file's confidentiality inside a stage sandbox.

**`orchestrator.complete` outlives its daemon.** Shutdown leaves the marker so a failed outcome stays readable. A daemon whose essential worker ends through an initialization error, a panic or a failed archive publishes a finalization failure, and a failed outcome or an unreadable or in-flight stage is not reported as success. `remove_previous_outcome` (`daemon/server/status.rs`) removes the marker and the scheduler failure file at the next run's start: daemon start (`daemon/server/lifecycle.rs`) and a foreground run past its preflights (`commands/run/foreground.rs`).

**Merge/fold tie-break on equal timestamps:** a fold over chronologically-ordered inputs (checkpoint merge, handoff merge, heartbeat observation merge) must prefer the LATER input on an exact tie, never keep-self/keep-earlier — second-precision RFC3339 timestamps make same-second updates common (e.g. a same-nonce ack-lost update and a new-fingerprint attempt landing in the same second), and a `self.latest` default on ties silently drops the newer evidence. This bit `checkpoint.rs` `merge_from` (kept `self.latest` on equal `observed_at`) and was then found again in `merge_observation` (used strict `>` while siblings already used `>=`) — the same latent mistake in three related folds. **Prevention:** any new fold over timestamped inputs needs an explicit equal-timestamp test case, and should default to `>=` (prefer incoming) not `>` (prefer self).

**`merge_session_handoff` / HandoffV2 fold:** takes `origin: Option<HandoffOrigin>` and returns `Unchanged` only when the semantic fold equals the newest exact-session artifact AND the incoming origin is `None` or equal to it (`RedBand` also compares `context_tokens`) — an `AgentCeiling` handoff that folds to an equal predecessor must still write a NEW artifact when its origin differs, because the daemon watches origin to decide what to do next; a fold that only compared content would silently swallow that write. No live-job-id field was added to `HandoffV2` — no producer of live job ids exists yet, so it would be dead data.

**Status derivation — writer-ownership-unknown vs parked-and-confirmed** (`fs/session_files/exact.rs::mark_session_terminal_reason`, consumed by `commands/status/`): `writer-ownership-unknown` = stage `NeedsHumanReview` + an actionable blocker for `stage.session` + that session record not yet terminal; `parked-and-confirmed` = `NeedsHumanReview` + session terminal (`ContextExhausted`/`CriteriaBlocked`). W3 escalates an unknown or surviving writer to `NeedsHumanReview` with a `review_reason` naming the session, and NEVER marks that session terminal itself — there is no durable "writer disposition" field (adding one would need its own merge/fold rules), so the session record staying non-terminal IS the signal that ownership is still unresolved; the exact-locked session record remains the sole source of truth for confirmed retirement.

**`knowledge_complete` commit selector** (`commands/stage/knowledge_complete.rs`, `handoff/completion/identity.rs`): a knowledge-distill stage runs in the MAIN checkout with no `loom/<stage>` branch, so `expected_stage_commit` resolves `Knowledge -> main checkout HEAD` (not a stage branch head). One shared selector keeps the host-job application, the monitor and status comparing the same commit for every stage type; add no second implementation.

**A stage stuck `waiting-for-input` blocks its own completion, and cannot self-resolve from inside the sandbox:** a stage file can carry status `waiting-for-input` even though the orchestrator never called `AskUserQuestion` — the session just waited between turns on background subagents. `loom stage complete` in that state still runs every acceptance criterion and goal-backward check to green, but the application refuses it with `stage ... is not executing` (`host_verify/apply.rs::validate_active_identity`). `loom stage resume <stage>` CANNOT be run from inside the stage session — `.loom/work/stages` is read-only there (EROFS on the `.tmp` write) and the daemon control socket is unreachable. **Prevention:** before the final completion attempt, check the stage file's own status (or `loom status`, which shows `?` and hints `loom stage resume`); if it reads `waiting-for-input`, the run needs the OPERATOR to run `loom stage resume <stage>` from outside the sandbox before the completion can be accepted.
