# Daemon Owned Commits

> Relayed signed commits

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

| Piece | Where |
| --- | --- |
| CLI, hooks, relay or in-process commit | `commands/stage/commit.rs`; `RequestKind::Commit` is a control kind (`relay/kind.rs`), so a teammate cannot relay it |
| Plumbing core | `git/stage_commit.rs` (`Committer`, `CommitScope`, `CommitRequest`, `CommitRefusal`), checks in `git/stage_commit/checks.rs` |
| Signing | `git/signing.rs`: `commit_tree`, `SigningEnv`, `SIGN_TIMEOUT` (30 s), `take_from_process`, `probe_in` |
| Daemon handler | `orchestrator/core/inbox_drain/commit.rs` (`apply_commit`, `prepare_*`), merge hold in `merge_handler/landing.rs` |
| Status polling | `commands/request/status.rs` (`loom request status <id> --wait <s>`) |
| Completion guard | `daemon/server/completion_evidence.rs` `refuse_uncommitted_index` |
| Startup check | `commands/run/signing_preflight.rs` |
| Hook block | `loom-hooks/commit-filter.sh` `git_runs_commit` |

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
- the wrong branch, a non-owner session, a symbolic ref (`update-ref --no-deref`, so a planted
  `refs/heads/loom/<id>` symref cannot move the target);
- a message that is empty, over 16 KiB, holds a NUL, or carries AI attribution
  (`validate_commit_message`, called by both the CLI and the daemon, because a session can write a
  ticket without the CLI).

Staged paths are not checked against the stage's `files:`; Conventional Commit types stay doctrine.
A refusal quotes the path with `{:?}`: the daemon logs the reason and a session picks the path, so a newline in it must not start a log line.

## Completion binds the commit

Completion evidence binds HEAD, and the commit applies asynchronously, so `loom stage complete`
before the commit landed would bind the old HEAD (and the stage would no longer be `Executing` for
the late commit). `refuse_uncommitted_index` therefore refuses completion with `staged changes are
not committed: run loom stage commit and wait for it with loom request status <id> --wait 90` while
the worktree index differs from HEAD; its Knowledge branch compares the main checkout's index under
the knowledge prefix, so any staged knowledge change blocks it, including one staged by another
session. Merge sessions never complete.

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
