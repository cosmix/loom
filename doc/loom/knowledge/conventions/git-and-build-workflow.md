---
---
# Git And Build Workflow

> Git, cargo, CI paths, size ledger

## Git Operations

```bash
git worktree add .worktrees/{stage-id} -b loom/{stage-id}
git worktree remove --force .worktrees/{stage-id}
git branch -D loom/{stage-id}   # Delete after merge
```

Loom never runs a three-way merge in the operator's main checkout; the only merge there is a `git merge --ff-only` when the target is checked out in it. `merge_stage` computes the merge with `merge-tree` and `commit-tree` and advances the target by `update-ref` or a guarded fast-forward ([merge-flow](../architecture/merge-flow.md)).

**Operator-operation guard rule:** `merge_stage` returns `Blocked(OperatorOperation { marker })` when the main checkout has a `MERGE_HEAD`, `CHERRY_PICK_HEAD`, `REVERT_HEAD`, `rebase-merge/` or `rebase-apply/` (`git/merge/in_progress.rs` is the single source for `MERGE_HEAD` detection). Never silently `git merge --abort` an operator's merge.

**Phantom-merge revert logging:** All phantom-merge reverts (sync-time merged=true revert, daemon `reconcile_main_repo_active_merge`, CLI `RevertAndSpawnResolver`) MUST log at `tracing::error!` level — not `warn` — so they show up in production logs. Reverts represent invariants violated; the noise is the point.

## Formatting and Test Invocation in a Shared Worktree

Only the main agent runs Cargo, formatters, and exhaustive verification in a shared checkout. Retire and reap a superseded command tree before starting its replacement; keep complete logs and exit codes.

The root `.cargo/config.toml` defaults to four Cargo build jobs and `RUST_TEST_THREADS=4`. These limit build-job and libtest concurrency separately; individual tests and compilers can create more threads. Job limits control concurrency and the watchdog controls command lifetime; both apply.

Use `/usr/bin/python3 scripts/run-rust-tests.py` for the complete all-target suite with isolated homes and an owned short OS temporary root. Use its separate `--doc` mode for workspace/all-feature doctests. The invocation-scoped wrappers preserve ordinary product execution outside tests. Direct Cargo calls inherit the concurrency default but do not provide aggregate fixture ownership or home isolation.

The supervisor uses native PID/start-time identities, wall and output-idle deadlines, and per-run cleanup journals. It retains actual command status and sampled process-tree thread/write metrics; samples are lower bounds and do not replace per-test child guards. A timeout, missing harness, surviving process, or zero-executed-test result cannot certify coverage. Do not use global name-based process killing or rely on a subprocess-based process monitor during native-startup stalls. See `doc/testing.md` for commands, limits, and opt-in output exclusions.

## `cargo test` Is Not This Repo's Test Gate — `--all-targets --no-fail-fast` Is

Run Rust gates from `loom/`, the crate directory. The exhaustive target gate is:

```bash
cargo test --all-targets --no-fail-fast -- --test-threads=4
```

`--no-fail-fast` continues after a failed target. `--all-targets` selects the
library, binary, integration, example and benchmark test targets that exist in
the manifest. Plain `cargo test` also includes the enabled integration targets;
the claim that integration tests require `--all-targets` is incorrect.

Target selection excludes doctests. Documentation generation alone does not
execute examples. Run the separate documentation test and static gates:

```bash
cargo test --doc --workspace --all-features -- --test-threads=1
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps
```

Reconcile Cargo metadata with completed harness summaries, retaining counts for
passed, failed and existing ignored tests. Account separately for executed,
compile-only and ignored doctests. A timeout before harness entry leaves that
target unverified. Retain full output, exit codes and process cleanup evidence;
never combine earlier-source subsets into a final-source suite pass.

CI (`.github/workflows/ci.yml`) runs platform test/lint jobs, the default
`scripts/flake-check.sh` workload, dashboard/terminal smoke checks, hook tests,
security audit, cargo-deny and dashboard checks under their path filters.
`loom/.githooks/pre-push` also requires Markdown lint, dependency audit and the
flake workload. The Linux and macOS test jobs require tmux through
`LOOM_E2E_REQUIRE_TMUX=1`; local platform coverage must be stated explicitly.
Doctests need the explicit command above because the CI all-target test jobs do
not select them.

Before interpreting failures from a stage environment, inspect ambient
`LOOM_STAGE_ID`, `LOOM_SESSION_ID` and `LOOM_WORKTREE_PATH`: control-session paths
can depend on these identities. Do not weaken identity or confinement assertions
to make a host run pass.

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

## Install a New loom Binary Only After the Running Plan Completes

The operator installs a freshly built `loom` (`dev-install.sh`) only once the plan being executed has completed, never between its stages: `dev-install.sh` stops the daemon, and a plan runs to completion on the binary it started with. Fixes committed to `main` mid-plan therefore take effect only for the next plan. Anything measured with the installed binary before then, such as `loom knowledge eval` scores, measures the older code. Re-measure after the install.

`dev-install.sh` keeps release optimization but overrides the build invocation with LTO off, 16 codegen units, and incremental compilation. Cargo jobs default to the available CPU count (`nproc`, then `getconf _NPROCESSORS_ONLN`, then four); an explicit `CARGO_BUILD_JOBS` takes precedence. These command-local environment overrides preserve the ordinary release profile (fat LTO and one codegen unit) and the installer contract at `loom/target/release/loom`. The first build with these settings rebuilds affected artifacts; later dev installs reuse the incremental cache.

## CI Jobs Run on Path Filters; the `rust` Filter Must Name Every Repo-Root Path the Crate Reads

`.github/workflows/ci.yml` triggers on every push to main and every pull request; its `changes` job (`dorny/paths-filter`) decides which jobs run. `rust` gates lint, test, flake-check, their macOS counterparts lint-macos, test-macos and flake-check-macos, audit and deny; `web` gates the dashboard job; `shell` gates hook-syntax and hooks-macos. The workflow has no `paths:` trigger, so the filter list is the only place CI names paths. A new gate whose result can depend on the target OS gets a `macos-latest` job beside its Linux one.

The crate compiles in files outside `loom/`: the asset roots in `loom/build/assets.rs` (`agents/`, `commands/`, `skills/`, `codex/skills/`, `web/dist/`, both `*.md.template` files), `loom-hooks/`, `install.sh` and `dev-install.sh` through `include_str!`, and `web/src/api/` fixtures through tests. A new asset root, or a new `include_str!` or `CARGO_MANIFEST_DIR`-relative read of a repo-root path, needs an entry in the `rust` filter. Without it, a change to that path alone skips every Rust job. To audit, list `rg -o 'include_(str|bytes)!\("(\.\./)+[A-Za-z0-9._-]+' loom/src loom/tests` against the filter.

## Signed Commits: What the Operator's Setup Must Provide

With `commit.gpgsign` true, the daemon signs every stage, knowledge and merge commit and the plan-completion
commit unattended, with no terminal ([Daemon-Owned Commits](../architecture/daemon-owned-commits.md)). The
signer must therefore work without a prompt, and `loom run` refuses to start when a probe signature fails in
either the operator's environment or the daemon's:

- **gpg-agent with a cached passphrase, or a GUI pinentry.** A terminal pinentry cannot prompt the daemon.
  The agent forgets a cached passphrase `default-cache-ttl` (600 s) after its last use and at most
  `max-cache-ttl` (7200 s) after the first, so raise both in `gpg-agent.conf` for a long run; `loom run`
  prints this caveat when `gpg.format` is openpgp. `loom run` sets `GPG_TTY` from `ttyname(stdin)` for its own
  probe when stdin is a terminal and `GPG_TTY` is unset, so a terminal pinentry can prompt once and the agent
  caches the passphrase.
- **SSH signing through the agent.** With `gpg.format` ssh, the key must be loaded in a running ssh-agent
  reachable through `SSH_AUTH_SOCK`; no cache expiry applies.
- **`GNUPGHOME` is honoured** (a non-default home works). `GNUPGHOME` and `SSH_AUTH_SOCK` are captured by
  the daemon at startup and passed only to the signing call, never to a session.
- **`commit.gpgsign` comes from a config file.** The daemon sees the operator's global configuration through
  `XDG_CONFIG_HOME` and `GIT_CONFIG_GLOBAL` but not `GIT_CONFIG_COUNT`/`GIT_CONFIG_KEY_*` or `git -c`; `loom run`
  refuses when the two environments read different values (`git config --global commit.gpgsign true`).
- **Check before running:** `git commit-tree -S` on an empty tree in the repository, from the shell that
  will run `loom run`.

A stage whose commit fails to sign is blocked (fix signing, then `loom stage retry`); a merge commit that
fails to sign parks the stage in needs-human-review (fix signing, then `loom stage human-review <id> --approve`).
