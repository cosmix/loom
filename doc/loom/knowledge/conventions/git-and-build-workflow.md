---
---
# Git And Build Workflow

> Git/worktree ops, cargo discipline, the maintainability ledger

## Git Operations

```bash
git worktree add .worktrees/{stage-id} -b loom/{stage-id}
git worktree remove --force .worktrees/{stage-id}
git merge --no-ff -m "Merge loom/{stage-id}" loom/{stage-id}
git branch -D loom/{stage-id}   # Delete after merge
```

**Active-merge guard rule (2026-04-27):** Helpers that mutate git merge state (`merge_stage`, `get_conflicting_files_from_status`) MUST refuse via `require_no_active_merge` when `MERGE_HEAD` is set on the repo path. Never silently `git merge --abort`. Defense in depth: even if attribution misses an active merge upstream, the guard surfaces an error instead of corrupting in-progress resolution.

**Phantom-merge revert logging (2026-04-27):** All phantom-merge reverts (sync-time merged=true revert, daemon `reconcile_main_repo_active_merge`, CLI `RevertAndSpawnResolver`) MUST log at `tracing::error!` level — not `warn` — so they show up in production logs. Reverts represent invariants violated; the noise is the point.

## Formatting and Test Invocation in a Shared Worktree

- **Never run `cargo fmt` while sibling subagents are live.** `cargo fmt -- <path>`
  **IGNORES its path arguments** and formats the ENTIRE crate, silently reformatting files
  another agent owns — which shows up in `git status` as an ownership violation and can
  collide with an in-flight edit. Use `rustfmt --edition 2021 <file>` for your own files.
  Only the main agent runs repo-wide `cargo fmt`, and only after every subagent has landed.
- **`cargo test` accepts exactly ONE testname filter.** Extra filters are rejected with
  "unexpected argument" BEFORE compiling, so zero tests run. Use one common prefix
  (`cargo test --lib context::`) or separate invocations chained with `&&`.
- **Filter by the real module path.** Tests under a `tests` submodule need it spelled out:
  `context::tests::delivery`, not `context::delivery`, which matches nothing.
- **`rustfmt`'s `fn_call_width = 60` is what forces a call vertical, not `max_width = 100`.**
  Sum the argument names including `,` separators; over 60 and rustfmt goes one-arg-per-line,
  which can explode a match arm and trip the 50-line function gate. Renaming in the pattern
  (`budget_tokens: budget`) is a legitimate way back under the limit.
- **Only the main agent runs `cargo` at all**, and it runs under a RAM watchdog rather than
  a job-count throttle. Measured on this 32-core machine, a full `cargo build --all-targets`
  peaks around 3 GB and the whole test suite barely moves the needle — so throttling `-j` is
  the wrong lever and just wastes the machine. What actually exhausted 125 GB was **leaked
  detached child processes**, not build parallelism (see
  [Never Spawn a Surviving Process From a Test](../mistakes/detached-spawn-in-tests.md)).
  Run cargo wrapped in a watchdog that samples `free`, kills the whole **process group** on a
  low-headroom trip (killing cargo alone orphans the `rustc` children holding the pages), and
  reports any `loom` process still alive after exit.
- **A test may never create a process that outlives the test harness.** `cargo test` gives no
  warning for a leaked detached child; it simply exits green while the child keeps running.
  See [Never Spawn a Surviving Process From a Test](../mistakes/detached-spawn-in-tests.md) —
  this cost a reboot once already.

## `cargo test` Is Not This Repo's Test Gate — `--all-targets --no-fail-fast` Is

Never write plain `cargo test` into a loom plan's acceptance criteria. The gate is:

```bash
cargo test --all-targets --no-fail-fast
```

What each flag does:

- **`--no-fail-fast` is what makes the report exhaustive.** Stopping at the first
  failing target hides how much else is red; an agent then fixes one failure, re-runs,
  and discovers the next — one round trip at a time.
- **`--all-targets` keeps the command identical to the pre-push hook, CI and
  `release.yml`.** Correction 2026-09-14: this bullet used to say that without
  `--all-targets` the `loom/tests/**` integration tests are never built. That is wrong
  for `cargo test`: every target in `loom/Cargo.toml` has `test = true` (check with
  `cargo metadata --no-deps`), so plain `cargo test` builds and runs every `loom/tests/`
  target. The flag does change `cargo build` and `cargo clippy`, which default to the lib
  and bin only (see "Clippy --all-targets Required to Catch Test-Module Lints" in
  [Testing & Lint](../mistakes/testing-and-lint.md)). It also disables doctests, as any
  target-selection flag does, so none of the three gates runs them; `cargo test --doc`
  does.

Know the two non-hermetic tests, so a red run inside a stage session is not
misdiagnosed as your own breakage. The stage-finalisation tests
(`commands/stage/tests/complete.rs`) route through `sandbox_control_session`
(`control_session.rs:70,94`), which reads `LOOM_STAGE_ID` / `LOOM_SESSION_ID` /
`LOOM_WORKTREE_PATH` from the ambient process environment. Running the suite from
INSIDE a loom worktree session leaves those set, silently routing the call down the
sandboxed worktree path instead of the host-side one the test means to exercise, and
it fails with a wrapper-identity mismatch. It is also order-dependent: it failed in
one full `--all-targets` run and passed in the next.

Re-run with `env -u LOOM_STAGE_ID -u LOOM_SESSION_ID` BEFORE concluding your change
broke it. The durable fix is an RAII env guard at test start — mirroring `EnvGuard`
in `commands/memory/handlers/tests.rs` — that restores on `Drop`, so a panic mid-test
cannot leak state into later tests. Do not apply that fix from an unrelated stage:
touching a file outside your territory is cross-stage merge-conflict bait.

## The Maintainability Ledger Is Shared State, and Only One Concurrent Stage May Own It

`loom/maintainability-baseline.txt` is an EXACT-match ledger: it fails when the code
SHRINKS as well as when it grows, and a plain `cargo test` runs it. It is also one
file at one path, shared by every worktree in a plan.

Three consequences a plan author has to design around:

1. **Exactly one CONCURRENT stage may own the ledger.** Two parallel stages that both
   grow or delete ledgered code will conflict on merge, and each will have reconciled
   against a baseline the other invalidated.
2. **A plan that grows or deletes ledgered code without owning the ledger cannot pass
   its own acceptance.** Deleting a ledgered function fails exactly like adding an
   over-long one, so a stage that removes ~4000 lines of orphaned surface MUST also
   hold the ledger.
3. **When a refactor drops an entry under the limit, DELETE the entry rather than
   lowering it.** Lowering keeps a permanent claim on a function that no longer needs
   one. This applies to FILE-path entries too, not just function entries: splitting a
   file's tests into a new file (e.g. `fs/locking.rs` -> `fs/locking/tests.rs`) leaves
   the original's baseline line stale once it shrinks below its limit, and
   `tests/maintainability.rs` fails with "stale entry ... no longer violates its
   limit; remove it" until that line is deleted in the same change (2026-09-22).

Before adding lines to any function: `rg '<fn name>' loom/maintainability-baseline.txt`.
If it is listed, refactor rather than extend. After any file split: `rg '<path>' loom/maintainability-baseline.txt`.

## Working Directory

The Bash tool keeps its working directory ACROSS calls in this harness, so one `cd` silently
redirects every later relative command. Prefix verification commands with an explicit absolute
`cd`, or pass absolute paths. **If EVERY independent check fails at once — fmt and clippy and
build and an unrelated gate — suspect the working directory before the code**; a real
regression almost never breaks all of them in the same instant.

## Git Push Requires Explicit User Request

Never `git push` unless the user explicitly asks — commit locally and stop. "Fix the CI failure" does NOT imply pushing to make CI green; the user decides when commits leave the machine. (Learned 2026-07-22: pushed after fixing a red CI run on the theory that CI-green was the deliverable — user rejected: "i didn't ask you to push.")

## "Clean Repo" Means Tracked Files Only

A request to commit and leave the repo clean means no uncommitted changes to tracked files. The untracked files in the main checkout (root-level scratch, `doc/` reports, uncommitted plans and briefs) belong to the operator: never delete, stage, or ask about them. (Learned 2026-09-18: after committing, the agent offered to delete them — user: "i didn't ask you to delete something. ignore untracked files.")

## Never Read the Daemon Credential Files

`.loom/work/admin.token` and `.loom/work/user.token` (and their legacy `.work/` equivalents) are operator secrets. Never read them, never run a command whose output could include them, and never widen a shell command into a directory sweep. On 2026-09-03 the operator rejected two batched commands as attempts to read the admin token: one combined `rg`/`fd` sweeps over `loom-hooks/tests` and `loom/tests` with a whole-file `rg -n "" loom-hooks/<script>`; the other combined `loom plan verify` with `rg` sweeps over `doc/plans/briefs/<plan>/`. Run `loom plan verify` on its own, or ask the operator to run it with the `!` prefix; search with `rg -n <pattern> <explicit file>` on named files, never on directories that may hold fixtures or state; read hook scripts through narrow `rg -n <pattern> -A <n> loom-hooks/<file>` queries, never a whole-file dump.

`rg` and `fd` skip the token files through the state root's `.ignore`; sessions the daemon spawns additionally get `RIPGREP_CONFIG_PATH`, which backstops `rg` alone, so `rg -uu` sweeps skip them too. The daemon publishes both `.ignore` and `ripgreprc` before either token exists (`daemon/server/tokens.rs::publish_fresh_tokens`), and the wrapper only exports `RIPGREP_CONFIG_PATH` when `ripgreprc` is already on disk, so a `loom run --foreground` session — which has no daemon, no tokens, and no exclusion files — never exports it either.

## Integration Tests Spawn the Loom Binary Only Through `tests/integration/helpers.rs` (2026-09-13)

Every integration test that spawns the loom binary goes through `loom_cmd`, `clear_relay_env` and
`loom_bin_path` in `tests/integration/helpers.rs`; `tests/integration/binary_spawn_guard.rs`
enforces it. Route a new integration test through the same helpers rather than shelling out to the
binary directly — a bespoke spawn skips the relay-env clearing the helpers do for every test, and
the guard fails the build.

**A target outside `tests/integration/` can have the same gap.** `loom/tests/worktree_remove_safety.rs`
is not covered by `binary_spawn_guard.rs` (it lives outside that directory) and fails 8/8 when the
test process inherits a live stage session's `LOOM_*` variables (it then takes the "daemon removes
the worktree" code path instead of the one under test) — it passes 8/8 with them unset. Run any such
target with `LOOM_*` unset, or scrub them the way the sanctioned spawners do, when it fails only
inside a live stage session and not in a clean shell.

## A Function Called From `$(...)` Cannot `exit` to Block Its Caller

A shell function invoked inside a command substitution (`X=$(normalize_lexical ...)`) runs in a
subshell — calling `exit` inside it only terminates that subshell, not the calling script, and
relying on `set -e`'s command-substitution-assignment behavior to propagate the failure is fragile
and non-obvious. Convention in this repo's hook scripts: have the function return a plain nonzero
status and let the CALL SITE do `if ! X=$(normalize_lexical ...); then block_target; exit 2; fi` — an
`if`-tested command is explicitly exempt from `set -e`, so this is both correct and matches the rest
of the file's style.

## Shared-Checkout Git Hygiene (2026-09-13)

- **Never `git stash` in a tree that carries other work.** In the shared main checkout a
  stash, `checkout stash@{0} -- <files>`, `stash drop` sequence rewrote files other agents were
  editing and destroyed their stash entries; the user's correction was "stop dropping stashes. stop
  disrupting other agents' work." Another time the pop aborted on a busy `README.md` and the tree
  had to be restored by hand. Touch only the paths you own (`git add -- <paths>` then
  `git commit -- <paths>`), and compare against HEAD by piping `git show HEAD:<file>` to the tool.
- **zsh applies history modifiers to `$VAR:x`.** `git show "$M:agents/file"` expanded `$M:a` as
  the absolute-path modifier, git failed, and the redirect had already truncated the target. Brace
  variables before a colon (`${M}:path`), write `git show` output to a temp file and `test -s` it
  before copying it over a tracked file, and chain dependent steps with `&&`: `set -e` does not stop
  a Bash-tool script.
- **zsh does not word-split an unquoted `$VAR`.** `L="a b"; for f in $L` runs the body once with
  `f` set to the whole string, so a park-and-restore loop around a partial commit tried to `cp` one
  path made of four names (nothing was touched only because `cp` failed first). Write the list
  inline (`for f in a b`) or use an array (`L=(a b)`).
- **In zsh, a bare word starting with `=` triggers `=cmd` path expansion.** A chained verification
  command using `echo ====` as a section separator failed with `=== not found`, and every check
  after it in the same chain silently never ran (its output just never printed). Quote separators
  (`echo "===="`) or use a non-`=`-leading marker (`echo ----`), and read every chained command's
  full output for a missing section rather than assuming a later command ran because the command
  overall "succeeded."
- **`jq --arg` hits `MAX_ARG_STRLEN` well under `ARG_MAX`.** A hook test piping a large fixture
  through `jq --arg name "$(cat file)"` failed with "Argument list too long" once that one argument
  reached 128 KiB — Linux caps a single exec argument at `MAX_ARG_STRLEN` (32 pages), independent of
  the much larger `ARG_MAX`. Keep any single `--arg` value under ~100 KiB, or feed a large fixture to
  `jq` on stdin / via `--rawfile` instead.
- **A bash here-string larger than the pipe buffer falls back to a real `$TMPDIR` file — treat that
  fallback as load-bearing, not incidental.** Replacing a SIGPIPE-prone `printf | sed | head -n1`
  pipeline with a pure-bash `while read ... done <<<"$text"` removes the pipefail/SIGPIPE risk, but
  bash silently backs a here-string bigger than the pipe buffer with a temp file under `$TMPDIR`; if
  that directory is unwritable the redirect fails closed (empty result), which is safe ONLY if the
  caller is written to treat an empty result as "degrade to a fallback," never as "nothing to do."
  Prefer bash parameter expansion on the whole string over a here-string when a helper must never
  silently degrade.
- **A spawned `std::process::Child` held across test assertions leaks on any failing assertion.**
  `Child`'s `Drop` neither kills nor waits, so a test that spawns a child (e.g. to drive
  `loom subagents watch --timeout 300` as a fixture), then runs several assertions before
  `child.wait()`, leaks a live process for the full timeout if any assertion panics first. Wrap any
  test-held `Child` in a kill-and-wait `Drop` guard so it is reaped on every panic path too, and
  bound any blocking read from it with a short timeout.
- **No AI attribution trailers in this repo**, whatever a harness reminder asks. CLAUDE.md Rule 9
  forbids them and `commit-filter.sh` blocks the whole Bash call; see [Commits](commits.md).
